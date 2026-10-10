##! Calendar and relative date grammar shared by date and touch.
##!
##! Grammar, epoch decimal handling and civil arithmetic are script logic.
##! Calendar conversion and timezone rules use the typed time primitives.
##! Instants carry seconds and nanoseconds separately, so years outside the signed
##! nanosecond range parse; `parse` keeps the nanosecond contract for callers that need it.

error DateError = Invalid

const NANOS = 1000000000
const MAX_INT = 9223372036854775807
const MIN_INT = -9223372036854775807 - 1

# Years in [NATIVE_FIRST_YEAR, NATIVE_END_YEAR) convert without a cycle shift, so
# their local-zone rules are the host's own.
const NATIVE_FIRST_YEAR = 1678
const NATIVE_END_YEAR = 2262
# The Gregorian calendar repeats every 400 years, which is a whole number of
# weeks, so a shift by whole cycles keeps weekdays, leap years and recurring
# zone rules. Shifted years land in [CYCLE_ANCHOR_YEAR, CYCLE_ANCHOR_YEAR + 400),
# the latest window the host's nanosecond range holds. Zones whose rules changed
# inside that window (pre-1883 local mean time, for example) can differ for
# instants outside the native range.
const CYCLE_YEARS = 400
const CYCLE_SECONDS = 12622780800
const CYCLE_ANCHOR_YEAR = 1862

type Civil = {year: Int, month: Int, day: Int}
type Clock = {hour: Int, minute: Int, second: Int, nanosecond: Int}
type Zoned = {text: Str, offset: Int?}
## Seconds since the Unix epoch and a nanosecond part in [0, NANOS).
export type Instant = {seconds: Int, nanoseconds: Int}
type Calendar = {year: Int, month: Int, day: Int, hour: Int, minute: Int, second: Int, nanosecond: Int, weekday: Int}

pure invalid(text: Str) -> Error {
  DateError.Invalid(f"invalid date: {text}")
}

## Integer division rounding toward negative infinity.
export pure floor_div(value: Int, divisor: Int) -> Int {
  value / divisor - (if value < 0 and value % divisor != 0 { 1 } else { 0 })
}

## Remainder in [0, divisor) for a positive divisor.
export pure positive_mod(value: Int, divisor: Int) -> Int {
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

# Moves a signed nanosecond part into the seconds field, so the stored nanoseconds
# are always in [0, NANOS).
pure instant_normalized(seconds: Int, nanoseconds: Int) -> Result[Instant] {
  Ok({seconds: add(seconds, floor_div(nanoseconds, NANOS))?, nanoseconds: positive_mod(nanoseconds, NANOS)})
}

## The instant at the given signed nanosecond count, with the nanoseconds normalized into [0, NANOS).
export pure instant_from_ns(nanoseconds: Int) -> Instant {
  {seconds: floor_div(nanoseconds, NANOS), nanoseconds: positive_mod(nanoseconds, NANOS)}
}

# Checked addition of whole seconds and a nanosecond delta, carrying the delta into seconds.
pure instant_add(instant: Instant, seconds: Int, nanoseconds: Int) -> Result[Instant] {
  let total = instant.nanoseconds + nanoseconds
  let whole = add(instant.seconds, seconds)?
  Ok({seconds: add(whole, floor_div(total, NANOS))?, nanoseconds: positive_mod(total, NANOS)})
}

## The signed 64-bit nanosecond count of an instant, or an error when it does not fit.
export pure instant_ns(instant: Instant) -> Result[Int, Error] {
  # The instant -9223372037 seconds only fits through its nanosecond part, so it is
  # handled apart from the product that would overflow.
  if instant.seconds == -9223372037 {
    return Err(invalid("timestamp out of range")) when instant.nanoseconds < 145224192
    return Ok(MIN_INT + (instant.nanoseconds - 145224192))
  }
  return Err(invalid("timestamp out of range")) when instant.seconds > 9223372036 or instant.seconds < -9223372036
  add(instant.seconds * NANOS, instant.nanoseconds)
}

pure epoch(text: Str) -> Result[Instant] {
  let fields = rx"^([+-]?)([0-9]+)(?:\.([0-9]*))?$".captures(text)
  if fields.is_empty() { return Err(invalid(text)) }
  let whole = decimal(fields[2])?
  let nanos = fraction(fields[3])?
  let negative = fields[1] == "-"
  instant_normalized(if negative { -whole } else { whole }, if negative { -nanos } else { nanos })
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

## Days since 1970-01-01 of a proleptic Gregorian date. Shared with date's calendar
## conversions, which need the same day numbering.
export pure days_from_civil(year: Int, month: Int, day: Int) -> Int {
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

# Whole Gregorian cycles to subtract so that the instant converts natively. An
# instant that already fits the native range keeps the host's zone rules.
pure cycles_for_instant(instant: Instant) -> Result[Int, Error] {
  return Ok(0) when instant_ns(instant) is Ok(_)
  let shifted = add(instant.seconds, -days_from_civil(CYCLE_ANCHOR_YEAR, 1, 1) * 86400)?
  Ok(floor_div(shifted, CYCLE_SECONDS))
}

pure cycles_for_year(year: Int) -> Int {
  return 0 when year >= NATIVE_FIRST_YEAR and year < NATIVE_END_YEAR
  floor_div(year - CYCLE_ANCHOR_YEAR, CYCLE_YEARS)
}

## Calendar years that the instant's native conversion is shifted by; zero when the
## instant converts natively. Formatters add this back to years they print.
export pure shift_years(instant: Instant) -> Result[Int, Error] {
  Ok(cycles_for_instant(instant)? * CYCLE_YEARS)
}

## Nanoseconds for the host time primitives, which only accept the signed 64-bit range.
export pure native_ns(instant: Instant) -> Result[Int, Error] {
  let cycles = cycles_for_instant(instant)?
  instant_ns({seconds: instant.seconds - cycles * CYCLE_SECONDS, nanoseconds: instant.nanoseconds})
}

# Calendar fields of an instant in the local zone or UTC. The year is the true
## year, even when the native conversion used a shifted one.
export proc calendar(instant: Instant, utc: Bool) -> Result[Calendar, Error] {
  let fields = time.to_calendar(native_ns(instant)?, utc)?
  Ok({year: fields.year + shift_years(instant)?, month: fields.month, day: fields.day, hour: fields.hour, minute: fields.minute, second: fields.second, nanosecond: instant.nanoseconds, weekday: fields.weekday})
}

# Converts calendar fields to an instant. Years outside the native window convert
# through a shifted year with the same leap-year and weekday pattern.
proc civil_instant(year: Int, month: Int, day: Int, hour: Int, minute: Int, second: Int, utc: Bool, normalize = false) -> Result[Instant] {
  let cycles = cycles_for_year(year)
  let ns = time.from_calendar(year - cycles * CYCLE_YEARS, month, day, hour, minute, second, utc:, normalize:)?
  Ok({seconds: floor_div(ns, NANOS) + cycles * CYCLE_SECONDS, nanoseconds: positive_mod(ns, NANOS)})
}

proc relative(base: Instant, unit: Str, count: Int, utc: Bool) -> Result[Instant] {
  let duration = match unit { "sec" | "second" => 1; "min" | "minute" => 60; "hour" => 3600; else => 0 }
  if duration != 0 { return instant_add(base, multiply(count, duration)?, 0) }
  if count < -1000000 or count > 1000000 { return Err(invalid("relative date out of range")) }
  let fields = calendar(base, utc)?
  let months = fields.month - 1 + (if unit == "month" { count } else { 0 })
  let year = fields.year + floor_div(months, 12) + (if unit == "year" { count } else { 0 })
  let month = positive_mod(months, 12) + 1
  let days = if unit == "day" { count } else if unit == "week" { count * 7 } else if unit == "fortnight" { count * 14 } else { 0 }
  let civil = civil_from_days(days_from_civil(year, month, fields.day) + days)
  let result = civil_instant(civil.year, civil.month, civil.day, fields.hour, fields.minute, fields.second, utc, normalize: true)?
  instant_add(result, 0, fields.nanosecond)
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

proc civil_epoch(year: Int, month: Int, day: Int, clock_text: Str, utc: Bool, offset: Int?) -> Result[Instant] {
  let parsed = clock(clock_text)?
  let value = civil_instant(year, month, day, parsed.hour, parsed.minute, parsed.second, utc or offset != null)?
  instant_add(value, -(offset ?? 0), parsed.nanosecond)
}

proc absolute(text: Str, utc: Bool, base: Instant) -> Result[Instant] {
  let zone = zone_suffix(text)?
  let body = zone.text
  if body == "" and zone.offset != null {
    let today = calendar(base, true)?
    return civil_epoch(today.year, today.month, today.day, "", true, zone.offset)
  }
  for word in ["today", "yesterday", "tomorrow"] {
    if body.lower().starts_with(f"{word} ") {
      let zone_utc = utc or zone.offset != null
      let midnight = parse_instant(word, utc: zone_utc, base:)?
      let today = calendar(midnight, zone_utc)?
      return civil_epoch(today.year, today.month, today.day, body.byte_slice(word.byte_len() + 1), zone_utc, zone.offset)
    }
  }
  let current = calendar(base, utc)?
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

## Parse date or touch input into an instant, using base as the sole relative baseline
## (the current time when null). Explicit offsets describe the input calendar; utc controls local calendars.
export proc parse_instant(text: Str, utc = false, base: Instant? = null) -> Result[Instant, Error] {
  let input = uncomment(text)
  if input.find("\0") != null { return Err(invalid(input)) }
  if input.starts_with("@") { return epoch(input.byte_slice(1)) }
  let origin_base = base ?? instant_from_ns(time.now() * 1000000)
  let lower = input.lower()
  if lower == "now" { return Ok(origin_base) }
  if lower == "" or lower == "today" or lower == "yesterday" or lower == "tomorrow" {
    let today = calendar(origin_base, utc)?
    let midnight = civil_instant(today.year, today.month, today.day, 0, 0, 0, utc)?
    if lower == "yesterday" { return relative(midnight, "day", -1, utc) }
    if lower == "tomorrow" { return relative(midnight, "day", 1, utc) }
    return Ok(midnight)
  }
  let words = input.replace("\t", with: " ").split(" ") |> where . != ""
  let weekday_value = weekday(words[-1])
  if weekday_value != null and words.len() <= 2 and (words.len() == 1 or words[0].lower() in ["last", "this", "next"]) {
    let today = calendar(origin_base, utc)?
    var days = positive_mod(weekday_value - today.weekday, 7)
    if words[0].lower() == "last" { days -= 7 }
    if days == 0 and words[0].lower() == "next" { days = 7 }
    let midnight = civil_instant(today.year, today.month, today.day, 0, 0, 0, utc)?
    return relative(midnight, "day", days, utc)
  }
  let clock_digits = rx"^([0-9]{1,4})[jJ]?$".captures(input)
  if ! clock_digits.is_empty() or lower == "j" {
    let digits = if lower == "j" { "0" } else { clock_digits[1] }
    let value = decimal(digits)?
    let hour = if digits.byte_len() <= 2 { value } else { value / 100 }
    let minute = if digits.byte_len() <= 2 { 0 } else { value % 100 }
    let today = calendar(origin_base, utc)?
    return civil_instant(today.year, today.month, today.day, hour, minute, 0, utc)
  }
  let military = rx"^([A-Za-z])([0-9]{1,2})$".captures(input)
  if ! military.is_empty() and timezone(military[1]) != null {
    let hours = decimal(military[2])?
    if hours > 23 { return Err(invalid(input)) }
    return instant_add(parse_instant(military[1], utc:, base: origin_base)?, hours * 3600, 0)
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
    var origin = origin_base
    if prefix != "" {
      let invert_prefix = ago and unit_name(prefix_words[-1]) != ""
      origin = parse_instant(if invert_prefix { f"{prefix} ago" } else { prefix }, utc:, base: origin_base)?
    }
    return relative(origin, unit, count, utc)
  }
  absolute(input, utc, origin_base)
}

## Parse date or touch input into nanoseconds since the Unix epoch, using base_ns as the
## sole relative baseline. Fails when the result does not fit the signed 64-bit nanosecond range.
export proc parse(text: Str, utc = false, base_ns: Int? = null) -> Result[Int, Error] {
  if let ns = base_ns { return instant_ns(parse_instant(text, utc:, base: instant_from_ns(ns))?) }
  instant_ns(parse_instant(text, utc:)?)
}
