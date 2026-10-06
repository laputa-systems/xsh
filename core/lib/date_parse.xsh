##! Calendar and relative date grammar shared by date and touch.
##!
##! Grammar, epoch decimal handling and civil arithmetic are script logic.
##! Calendar conversion and timezone rules use the typed time primitives.

error DateError = Invalid

const NANOS = 1000000000
const MAX_INT = 9223372036854775807
const MIN_INT = -9223372036854775807 - 1

type Civil = {year: Int, month: Int, day: Int}
type Clock = {hour: Int, minute: Int, second: Int, nanosecond: Int}
type Zoned = {text: Str, offset: Int?}

pure invalid(text: Str) -> Error {
  DateError.Invalid(f"invalid date: {text}")
}

pure floor_div(value: Int, divisor: Int) -> Int {
  value / divisor - (if value < 0 and value % divisor != 0 { 1 } else { 0 })
}

pure positive_mod(value: Int, divisor: Int) -> Int {
  let remainder = value % divisor
  if remainder < 0 { remainder + divisor } else { remainder }
}

pure add(left: Int, right: Int) -> Result[Int] {
  if (right > 0 and left > MAX_INT - right) or (right < 0 and left < MIN_INT - right) {
    return Err(invalid("timestamp out of range"))
  }
  Ok(left + right)
}

pure multiply(value: Int, positive: Int) -> Result[Int] {
  if value > MAX_INT / positive or value < MIN_INT / positive { return Err(invalid("timestamp out of range")) }
  Ok(value * positive)
}

# Date fields are decimal even when padded with zeros. Normalize once to
# the canonical integer spelling required by the standard text conversion.
pure decimal(text: Str) -> Result[Int] {
  if ! rx"^[+-]?[0-9]+$".matches(text) { return Err(invalid(text)) }
  let negative = text.starts_with("-")
  var start = if text.starts_with("+") or negative { 1 } else { 0 }
  while start < text.byte_len() - 1 and text.byte_at(start) == 48 { start += 1 }
  let digits = text.byte_slice(start)
  let normalized = if negative and digits != "0" { f"-{digits}" } else { digits }
  normalized.parse_int_decimal()
}

pure optional_decimal(text: Str) -> Result[Int] {
  if text == "" { Ok(0) } else { decimal(text) }
}

pure fraction(text: Str) -> Result[Int] {
  if text.byte_len() > 9 or ! rx"^[0-9]*$".matches(text) { return Err(invalid(text)) }
  var value = optional_decimal(text)?
  for _ in range(9 - text.byte_len()) { value *= 10 }
  Ok(value)
}

pure epoch(text: Str) -> Result[Int] {
  let fields = rx"^([+-]?)([0-9]+)(?:\.([0-9]*))?$".captures(text)
  if fields.is_empty() { return Err(invalid(text)) }
  let whole = decimal(fields[2])?
  let nanos = fraction(fields[3])?
  let seconds = if fields[1] == "-" { -whole } else { whole }
  let value = multiply(seconds, NANOS)?
  add(value, if fields[1] == "-" { -nanos } else { nanos })
}

pure uncomment(text: Str) -> Str {
  var output = ""
  var depth = 0
  for char in text {
    if char == "(" { depth += 1 } else if char == ")" and depth > 0 { depth -= 1 } else if depth == 0 { output = f"{output}{char}" }
  }
  rx"[[:space:]]+".replace(output, with: " ").trim()
}

pure month(text: Str) -> Int? {
  match text.lower() {
    "jan" | "january" => 1
    "feb" | "february" => 2
    "mar" | "march" => 3
    "apr" | "april" => 4
    "may" => 5
    "jun" | "june" => 6
    "jul" | "july" => 7
    "aug" | "august" => 8
    "sep" | "sept" | "september" => 9
    "oct" | "october" => 10
    "nov" | "november" => 11
    "dec" | "december" => 12
    else => null
  }
}

pure weekday(text: Str) -> Int? {
  match text.lower() {
    "sun" | "sunday" => 0
    "mon" | "monday" => 1
    "tue" | "tuesday" => 2
    "wed" | "wednesday" => 3
    "thu" | "thursday" => 4
    "fri" | "friday" => 5
    "sat" | "saturday" => 6
    else => null
  }
}

pure timezone(text: Str) -> Int? {
  let name = text.upper()
  match name {
    "UTC" | "GMT" | "WET" | "Z" => return 0
    "WEST" | "CET" | "MET" | "MEZ" => return 3600
    "CEST" | "MEST" | "MESZ" => return 7200
    "IST" => return 19800
    "KST" | "JST" => return 32400
    "AWST" => return 28800
    "ACST" => return 34200
    "ACDT" => return 37800
    "AEST" => return 36000
    "AEDT" => return 39600
    "EST" => return -18000
    "EDT" => return -14400
    "CST" => return -21600
    "CDT" => return -18000
    "MST" => return -25200
    "MDT" => return -21600
    "PST" => return -28800
    "PDT" => return -25200
    else => {}
  }
  if name.byte_len() != 1 { return null }
  let east = "ABCDEFGHI".find(name) ?? -1
  if east >= 0 { return (east + 1) * 3600 }
  let far_east = "KLM".find(name) ?? -1
  if far_east >= 0 { return (far_east + 10) * 3600 }
  let west = "NOPQRSTUVWXY".find(name) ?? -1
  if west >= 0 { return -(west + 1) * 3600 }
  null
}

pure zone_suffix(text: Str) -> Result[Zoned] {
  let words = text.replace("\t", with: " ").split(" ") |> where . != ""
  if ! words.is_empty() {
    let word = words[-1]
    let offset = timezone(word)
    if offset != null { return Ok({text: text.byte_slice(0, text.byte_len() - word.byte_len()).trim(), offset: offset}) }
  }
  if text.lower().ends_with("z") { return Ok({text: text.byte_slice(0, text.byte_len() - 1).trim(), offset: 0}) }
  let fields = rx"^(.*[0-9])\s*([+-])([0-9]{1,2})(?::?([0-9]{2}))?(?::?([0-9]{2}))?$".captures(text)
  if ! fields.is_empty() and fields[1].byte_len() > 9 {
    let hours = decimal(fields[3])?
    let minutes = optional_decimal(fields[4])?
    let seconds = optional_decimal(fields[5])?
    if hours > 23 or minutes > 59 or seconds > 59 { return Err(invalid(text)) }
    let sign = if fields[2] == "-" { -1 } else { 1 }
    return Ok({text: fields[1].trim(), offset: sign * (hours * 3600 + minutes * 60 + seconds)})
  }
  Ok({text: text, offset: null})
}

pure days_from_civil(year: Int, month: Int, day: Int) -> Int {
  let y = year - (if month <= 2 { 1 } else { 0 })
  let era = floor_div(y, 400)
  let within = y - era * 400
  let m = month + (if month > 2 { -3 } else { 9 })
  let ordinal = (153 * m + 2) / 5 + day - 1
  era * 146097 + within * 365 + within / 4 - within / 100 + ordinal - 719468
}

pure civil_from_days(days: Int) -> Civil {
  let shifted = days + 719468
  let era = floor_div(shifted, 146097)
  let day_of_era = shifted - era * 146097
  let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365
  let year = year_of_era + era * 400
  let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100)
  let month_prime = (5 * day_of_year + 2) / 153
  let day = day_of_year - (153 * month_prime + 2) / 5 + 1
  let month = month_prime + (if month_prime < 10 { 3 } else { -9 })
  {year: year + (if month <= 2 { 1 } else { 0 }), month: month, day: day}
}

proc relative(base_ns: Int, unit: Str, count: Int, utc: Bool) -> Result[Int] {
  let duration = match unit { "sec" | "second" => 1; "min" | "minute" => 60; "hour" => 3600; else => 0 }
  if duration != 0 { return add(base_ns, multiply(multiply(count, duration)?, NANOS)?) }
  if count < -1000000 or count > 1000000 { return Err(invalid("relative date out of range")) }
  let fields = time.to_calendar(base_ns, utc)?
  let months = fields.month - 1 + (if unit == "month" { count } else { 0 })
  let year = fields.year + floor_div(months, 12) + (if unit == "year" { count } else { 0 })
  let month = positive_mod(months, 12) + 1
  let days = if unit == "day" { count } else if unit == "week" { count * 7 } else if unit == "fortnight" { count * 14 } else { 0 }
  let civil = civil_from_days(days_from_civil(year, month, fields.day) + days)
  let result = time.from_calendar(civil.year, civil.month, civil.day, fields.hour, fields.minute, fields.second, utc:, normalize: true)?
  add(result, fields.nanosecond)
}

pure unit_name(text: Str) -> Str {
  let lower = text.lower()
  let value = if lower.ends_with("s") { lower.byte_slice(0, lower.byte_len() - 1) } else { lower }
  if value in ["sec", "second", "min", "minute", "hour", "day", "week", "fortnight", "month", "year"] { value } else { "" }
}

pure clock(text: Str) -> Result[Clock] {
  if text == "" { return Ok({hour: 0, minute: 0, second: 0, nanosecond: 0}) }
  let normalized = text.lower().replace("a.m.", with: "am").replace("p.m.", with: "pm")
  let fields = rx"^([0-9]{1,2})(?::([0-9]{1,2}))?(?::([0-9]{1,2})(?:[.,]([0-9]{1,9}))?)?\s*([ap]m)?$".captures(normalized)
  if fields.is_empty() { return Err(invalid(text)) }
  var hour = decimal(fields[1])?
  let minute = optional_decimal(fields[2])?
  let second = optional_decimal(fields[3])?
  if fields[5] != "" {
    if hour < 1 or hour > 12 { return Err(invalid(text)) }
    hour = hour % 12 + (if fields[5] == "pm" { 12 } else { 0 })
  }
  if hour > 23 or minute > 59 or second > 59 { return Err(invalid(text)) }
  Ok({hour: hour, minute: minute, second: second, nanosecond: fraction(fields[4])?})
}

proc civil_epoch(year: Int, month: Int, day: Int, clock_text: Str, utc: Bool, offset: Int?) -> Result[Int] {
  let parsed = clock(clock_text)?
  let value = time.from_calendar(year, month, day, parsed.hour, parsed.minute, parsed.second, utc: utc or offset != null)?
  let adjusted = add(value, -(offset ?? 0) * NANOS)?
  add(adjusted, parsed.nanosecond)
}

proc absolute(text: Str, utc: Bool, base_ns: Int) -> Result[Int] {
  let zone = zone_suffix(text)?
  let body = zone.text
  if body == "" and zone.offset != null {
    let today = time.to_calendar(base_ns, true)?
    return civil_epoch(today.year, today.month, today.day, "", true, zone.offset)
  }
  for word in ["today", "yesterday", "tomorrow"] {
    if body.lower().starts_with(f"{word} ") {
      let zone_utc = utc or zone.offset != null
      let midnight = parse(word, utc: zone_utc, base_ns:)?
      let today = time.to_calendar(midnight, zone_utc)?
      return civil_epoch(today.year, today.month, today.day, body.byte_slice(word.byte_len() + 1), zone_utc, zone.offset)
    }
  }
  let current = time.to_calendar(base_ns, utc)?
  let iso = rx"^([0-9]{4,})[-/]([0-9]{1,2})[-/]([0-9]{1,2})(?:[Tt ](.+))?$".captures(body)
  if ! iso.is_empty() { return civil_epoch(decimal(iso[1])?, decimal(iso[2])?, decimal(iso[3])?, iso[4], utc, zone.offset) }
  let us = rx"^([0-9]{1,2})/([0-9]{1,2})/([0-9]{4})(?:\s+(.+))?$".captures(body)
  if ! us.is_empty() { return civil_epoch(decimal(us[3])?, decimal(us[1])?, decimal(us[2])?, us[4], utc, zone.offset) }
  let compact = rx"^([0-9]{8}|[0-9]{10}|[0-9]{12})(?:\.([0-9]{2}))?$".captures(body)
  if ! compact.is_empty() {
    let digits = compact[1]
    let length = digits.byte_len()
    if length == 8 and compact[2] == "" {
      let year = decimal(digits.byte_slice(0, 4))?
      if year >= 1677 and year <= 2262 {
        return civil_epoch(year, decimal(digits.byte_slice(4, 2))?, decimal(digits.byte_slice(6, 2))?, "", utc, zone.offset)
      }
    }
    var year = current.year
    var at = 0
    if length == 12 { year = decimal(digits.byte_slice(0, 4))?; at = 4 }
    if length == 10 { let short_year = decimal(digits.byte_slice(0, 2))?; year = short_year + (if short_year >= 69 { 1900 } else { 2000 }); at = 2 }
    let month = decimal(digits.byte_slice(at, 2))?
    let day = decimal(digits.byte_slice(at + 2, 2))?
    let hour = digits.byte_slice(at + 4, 2)
    let minute = digits.byte_slice(at + 6, 2)
    let second = if compact[2] == "" { "00" } else { compact[2] }
    return civil_epoch(year, month, day, f"{hour}:{minute}:{second}", utc, zone.offset)
  }
  var named = body
  let words = body.replace("\t", with: " ").split(" ") |> where . != ""
  if ! words.is_empty() and weekday(words[0].replace(",", with: "")) != null {
    let remaining = words |> drop(1)
    named = remaining.join(" ")
  }
  let month_first = rx"^([A-Za-z]+)\s+([0-9]{1,2}),?\s+([0-9]{4})(?:\s+(.+))?$".captures(named)
  if ! month_first.is_empty() {
    let number = month(month_first[1])
    if number != null { return civil_epoch(decimal(month_first[3])?, number, decimal(month_first[2])?, month_first[4], utc, zone.offset) }
  }
  let day_first = rx"^([0-9]{1,2})\s+([A-Za-z]+)\s+([0-9]{4})(?:\s+(.+))?$".captures(named)
  if ! day_first.is_empty() {
    let number = month(day_first[2])
    if number != null { return civil_epoch(decimal(day_first[3])?, number, decimal(day_first[1])?, day_first[4], utc, zone.offset) }
  }
  let asctime = rx"^([A-Za-z]+)\s+([0-9]{1,2})\s+(.+)\s+([0-9]{4})$".captures(named)
  if ! asctime.is_empty() {
    let number = month(asctime[1])
    if number != null { return civil_epoch(decimal(asctime[4])?, number, decimal(asctime[2])?, asctime[3], utc, zone.offset) }
  }
  civil_epoch(current.year, current.month, current.day, body, utc, zone.offset)
}

## Parse date or touch input, using base_ns as the sole relative baseline.
## Explicit offsets describe the input calendar; utc controls local calendars.
export proc parse(text: Str, utc = false, base_ns: Int? = null) -> Result[Int, Error] {
  let input = uncomment(text)
  if input.find("\0") != null { return Err(invalid(input)) }
  if input.starts_with("@") { return epoch(input.byte_slice(1)) }
  let base = base_ns ?? time.now() * 1000000
  let lower = input.lower()
  if lower == "now" { return Ok(base) }
  if lower == "" or lower == "today" or lower == "yesterday" or lower == "tomorrow" {
    let today = time.to_calendar(base, utc)?
    let midnight = time.from_calendar(today.year, today.month, today.day, utc:)?
    if lower == "yesterday" { return relative(midnight, "day", -1, utc) }
    if lower == "tomorrow" { return relative(midnight, "day", 1, utc) }
    return Ok(midnight)
  }
  let words = input.replace("\t", with: " ").split(" ") |> where . != ""
  let weekday_value = weekday(words[-1])
  if weekday_value != null and words.len() <= 2 and (words.len() == 1 or words[0].lower() in ["last", "this", "next"]) {
    let today = time.to_calendar(base, utc)?
    var days = positive_mod(weekday_value - today.weekday, 7)
    if words[0].lower() == "last" { days -= 7 }
    if days == 0 and words[0].lower() == "next" { days = 7 }
    let midnight = time.from_calendar(today.year, today.month, today.day, utc:)?
    return relative(midnight, "day", days, utc)
  }
  let clock_digits = rx"^([0-9]{1,4})[jJ]?$".captures(input)
  if ! clock_digits.is_empty() or lower == "j" {
    let digits = if lower == "j" { "0" } else { clock_digits[1] }
    let value = decimal(digits)?
    let hour = if digits.byte_len() <= 2 { value } else { value / 100 }
    let minute = if digits.byte_len() <= 2 { 0 } else { value % 100 }
    let today = time.to_calendar(base, utc)?
    return time.from_calendar(today.year, today.month, today.day, hour, minute, utc:)
  }
  let military = rx"^([A-Za-z])([0-9]{1,2})$".captures(input)
  if ! military.is_empty() and timezone(military[1]) != null {
    let hours = decimal(military[2])?
    if hours > 23 { return Err(invalid(input)) }
    return add(parse(military[1], utc:, base_ns: base)?, hours * 3600 * NANOS)
  }
  let ago = words[-1].lower() == "ago"
  let end = words.len() - (if ago { 1 } else { 0 })
  let unit = if end > 0 { unit_name(words[end - 1]) } else { "" }
  if unit != "" {
    var count = 1
    var prefix_end = end - 1
    if end >= 2 {
      let previous = words[end - 2].lower()
      match decimal(previous) {
        Ok(number) => { count = number; prefix_end = end - 2 }
        Err(problem) => {
          if previous in ["next", "last", "this"] { count = if previous == "last" { -1 } else if previous == "this" { 0 } else { 1 }; prefix_end = end - 2 }
        }
      }
    }
    if prefix_end > 0 and words[prefix_end - 1] in ["+", "-"] {
      if words[prefix_end - 1] == "-" { if count == MIN_INT { return Err(invalid(input)) }; count = -count }
      prefix_end -= 1
    }
    if ago { if count == MIN_INT { return Err(invalid(input)) }; count = -count }
    let prefix_words = words[0..prefix_end]
    let prefix = prefix_words.join(" ")
    var origin = base
    if prefix != "" {
      let invert_prefix = ago and unit_name(prefix_words[-1]) != ""
      origin = parse(if invert_prefix { f"{prefix} ago" } else { prefix }, utc:, base_ns: base)?
    }
    return relative(origin, unit, count, utc)
  }
  absolute(input, utc, base)
}
