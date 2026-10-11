#!/bin/xsh
use lib.gnu
use lib.date_parse

error DateError = Invalid

# GNU date treats a lone dash like an empty date expression, which means midnight today.
proc parse_date(text: Str, utc: Bool, base: date_parse.Instant? = null) [time, error] -> Result[date_parse.Instant, Error] {
  var input = if text == "-" { "" } else { text }
  let words = input.replace("\t", with: " ").split(" ") |> where . != ""
  if words.len() >= 2 {
    let count = rx"^-([0-9]+)$".captures(words[-2])
    let unit = words[-1].lower()
    if ! count.is_empty() and unit in ["sec", "secs", "second", "seconds", "min", "mins", "minute", "minutes", "hour", "hours", "day", "days", "week", "weeks", "fortnight", "fortnights", "month", "months", "year", "years"] {
      let prefix = words[0..-2].join(" ")
      input = f"{if prefix == "" { "" } else { f"{prefix} " }}{count[1]} {words[-1]} ago"
    }
  }
  # A fixed POSIX TZ offset can use the shared parser's explicit numeric zone without changing process TZ.
  let timezone_prefix = "TZ=\""
  let timezone_end = input.find("\" ") ?? -1
  if input.starts_with(timezone_prefix) and timezone_end > timezone_prefix.byte_len() {
    let embedded_timezone = rx"^([A-Za-z]{3,})([+-]?)([0-9]{1,2})?(?::([0-9]{2}))?(?::([0-9]{2}))?$".captures(input.byte_slice(timezone_prefix.byte_len(), length: timezone_end - timezone_prefix.byte_len()))
    if ! embedded_timezone.is_empty() and (embedded_timezone[2] == "" or embedded_timezone[3] != "") {
      let hours = if embedded_timezone[3] == "" { 0 } else { embedded_timezone[3].parse_int_decimal()? }
      let minutes = if embedded_timezone[4] == "" { 0 } else { embedded_timezone[4].parse_int_decimal()? }
      let seconds = if embedded_timezone[5] == "" { 0 } else { embedded_timezone[5].parse_int_decimal()? }
      let sign = if embedded_timezone[2] == "-" { "+" } else { "-" }
      let offset = f"{sign}{hours:02}{minutes:02}{if seconds == 0 { "" } else { f"{seconds:02}" }}"
      input = f"{input.byte_slice(timezone_end + 2)} {offset}"
    }
  }
  # These names are absent from GNU date's zone vocabulary; parenthesized comments are not tokens.
  var zone_input = ""
  var comment_depth = 0
  for char in input {
    if char == "(" { comment_depth += 1 } else if char == ")" and comment_depth > 0 { comment_depth -= 1 } else if comment_depth == 0 { zone_input = f"{zone_input}{char}" }
  }
  if rx"(?i)\b(AWST|ACST|ACDT|AEST|AEDT)\b".matches(zone_input) { return Err(DateError.Invalid(f"invalid date: {input}")) }
  date_parse.parse_instant(input, utc:, base:)
}

# GNU date removes fractional trailing zeroes up to the requested precision, then pads after the digits.
proc format_nanoseconds(instant: date_parse.Instant, flags: Str, width_text: Str, utc: Bool) [time, error] -> Result[Str] {
  var width = if width_text == "" { 9 } else { width_text.parse_int_decimal()? }
  if width <= 0 { width = 9 }
  let digits = time.format(date_parse.native_ns(instant)?, "%9N", utc:)?
  var digit_count = 9
  while digit_count > width or (digit_count > 1 and (digits.byte_at(digit_count - 1) ?? 0) == 48) { digit_count -= 1 }
  var output = digits.byte_slice(0, digit_count)
  var padding = "0"
  var no_padding = false
  for flag in flags {
    match flag {
      "_" => { padding = " "; no_padding = false }
      "-" => no_padding = true
      "0" | "+" => { padding = "0"; no_padding = false }
      else => {}
    }
  }
  if flags == "-" and width_text == "" {
    let resolution = time.clock_resolution()?
    width = 9
    var threshold = 10
    while threshold <= resolution { width -= 1; threshold *= 10 }
    if width <= 0 { width = 9 }
    return time.format(date_parse.native_ns(instant)?, f"%{width}N", utc:)
  }
  if ! no_padding {
    var remaining = width - digit_count
    var fill = padding
    var suffix = ""
    while remaining > 0 {
      if remaining % 2 == 1 { suffix = f"{suffix}{fill}" }
      remaining /= 2
      if remaining > 0 { fill = f"{fill}{fill}" }
    }
    output = f"{output}{suffix}"
  }
  Ok(output)
}

# Width of the valid UTF-8 sequence starting at AT, or 0 when the bytes there do
# not form one.
pure utf8_char_width(raw: Bytes, at: Int) -> Int {
  let lead = raw.byte_at(at) ?? 0
  return 1 when lead < 128

  let width = if lead >= 194 and lead <= 223 { 2 } else if lead >= 224 and lead <= 239 { 3 } else if lead >= 240 and lead <= 244 { 4 } else { 0 }
  return 0 when width == 0 or at + width > raw.len()

  if let Ok(_) = raw[at..at + width].utf8() { return width }

  0
}

# Each valid UTF-8 run is formatted as text. An undecodable byte is copied
# unchanged, so a `%` before it prints literally and the byte is never read as a
# conversion.
proc format_date(instant: date_parse.Instant, format: Bytes, utc: Bool, style: DateStyle) [time, error, process, env] -> Result[Bytes] {
  var pieces: List[Bytes] = []
  var piece_start = 0
  var at = 0
  while at < format.len() {
    let width = utf8_char_width(format, at)
    if width == 0 {
      if piece_start < at { pieces += [bytes.from_text(format_text(instant, format[piece_start..at].utf8()?, utc, style)?)] }
      pieces += [format[at..at + 1]]
      at += 1
      piece_start = at
    } else {
      at += width
    }
  }
  if piece_start < format.len() { pieces += [bytes.from_text(format_text(instant, format[piece_start..format.len()].utf8()?, utc, style)?)] }
  Ok(bytes.concat(pieces))
}

proc reject_extra_operand(raw: Bytes) [process, env] -> Unit {
  match raw.utf8() {
    Ok(text) => gnu.extra_operand(text)
    Err(_) => gnu.usage_error(f"extra operand {gnu.quote_value_bytes(raw)}")
  }
}

# Month and weekday names for the languages that have a table. A locale without a
# table keeps the host's English names.
type NameSet = {months: List[Str], short_months: List[Str], weekdays: List[Str], short_weekdays: List[Str]}

pure name_set(language: Str) -> NameSet? {
  match language {
    "fr" => {
      months: ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"],
      short_months: ["janv.", "févr.", "mars", "avr", "mai", "juin", "juil", "août", "sept", "oct", "nov", "déc."],
      weekdays: ["dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi"],
      short_weekdays: ["dim.", "lun.", "mar.", "mer.", "jeu.", "ven.", "sam."],
    }
    "de" => {
      months: ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"],
      short_months: ["Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez"],
      weekdays: ["Sonntag", "Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag"],
      short_weekdays: ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"],
    }
    "es" => {
      months: ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"],
      short_months: ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"],
      weekdays: ["domingo", "lunes", "martes", "miércoles", "jueves", "viernes", "sábado"],
      short_weekdays: ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"],
    }
    "it" => {
      months: ["gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre"],
      short_months: ["gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic"],
      weekdays: ["domenica", "lunedì", "martedì", "mercoledì", "giovedì", "venerdì", "sabato"],
      short_weekdays: ["dom", "lun", "mar", "mer", "gio", "ven", "sab"],
    }
    "pt" => {
      months: ["janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"],
      short_months: ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"],
      weekdays: ["domingo", "segunda-feira", "terça-feira", "quarta-feira", "quinta-feira", "sexta-feira", "sábado"],
      short_weekdays: ["dom", "seg", "ter", "qua", "qui", "sex", "sáb"],
    }
    "hu" => {
      months: ["január", "február", "március", "április", "május", "június", "július", "augusztus", "szeptember", "október", "november", "december"],
      short_months: ["jan", "febr", "márc", "ápr", "máj", "jún", "júl", "aug", "szept", "okt", "nov", "dec"],
      weekdays: ["vasárnap", "hétfő", "kedd", "szerda", "csütörtök", "péntek", "szombat"],
      short_weekdays: ["V", "H", "K", "Sze", "Cs", "P", "Szo"],
    }
    "th" => {
      months: ["มกราคม", "กุมภาพันธ์", "มีนาคม", "เมษายน", "พฤษภาคม", "มิถุนายน", "กรกฎาคม", "สิงหาคม", "กันยายน", "ตุลาคม", "พฤศจิกายน", "ธันวาคม"],
      short_months: ["ม.ค.", "ก.พ.", "มี.ค.", "เม.ย.", "พ.ค.", "มิ.ย.", "ก.ค.", "ส.ค.", "ก.ย.", "ต.ค.", "พ.ย.", "ธ.ค."],
      weekdays: ["อาทิตย์", "จันทร์", "อังคาร", "พุธ", "พฤหัสบดี", "ศุกร์", "เสาร์"],
      short_weekdays: ["อา.", "จ.", "อ.", "พ.", "พฤ.", "ศ.", "ส."],
    }
    "ja" => {
      months: ["1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月"],
      short_months: ["1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月"],
      weekdays: ["日曜日", "月曜日", "火曜日", "水曜日", "木曜日", "金曜日", "土曜日"],
      short_weekdays: ["日", "月", "火", "水", "木", "金", "土"],
    }
    "zh" => {
      months: ["一月", "二月", "三月", "四月", "五月", "六月", "七月", "八月", "九月", "十月", "十一月", "十二月"],
      short_months: ["1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月"],
      weekdays: ["星期日", "星期一", "星期二", "星期三", "星期四", "星期五", "星期六"],
      short_weekdays: ["周日", "周一", "周二", "周三", "周四", "周五", "周六"],
    }
    else => null
  }
}

# Calendar systems selected by locale. Only the year, month and day directives
# follow these; the rest of the format keeps the Gregorian fields.
pure calendar_name(locale: Str) -> Str {
  match locale {
    "fa_IR" => "persian"
    "th_TH" => "buddhist"
    "am_ET" => "ethiopian"
    else => "gregorian"
  }
}

type DateStyle = {names: NameSet?, calendar: Str}
type CalendarDate = {year: Int, month: Int, day: Int}

# The name table and calendar for a locale such as fr_FR.UTF-8; the codeset and
# modifier are not part of the table key.
pure date_style(locale: Str) -> DateStyle {
  let base_with_modifier = locale.split(".")[0]
  let base = base_with_modifier.split("@")[0]
  let language = base.split("_")[0].lower()
  {names: name_set(language), calendar: calendar_name(base)}
}

proc locale_name() [env] -> Str {
  for name in ["LC_ALL", "LC_TIME", "LANG"] {
    let found = env.get_or(name, "") ?? ""
    if found != "" { return found }
  }
  ""
}

# Persian years follow the 33-year leap cycle; the anchor places 1405-01-01 on 2026-03-21.
pure persian_is_leap(year: Int) -> Bool {
  date_parse.positive_mod(year, 33) in [1, 5, 9, 13, 17, 22, 26, 30]
}

# Days from the start of the Persian calendar's numbering to the given date.
pure persian_day_number(year: Int, month: Int, day: Int) -> Int {
  let previous = year - 1
  var leaps = date_parse.floor_div(previous, 33) * 8
  for offset in range(date_parse.positive_mod(previous, 33)) {
    if persian_is_leap(offset + 1) { leaps += 1 }
  }
  let month_start = if month <= 6 { 31 * (month - 1) } else { 186 + 30 * (month - 7) }
  365 * previous + leaps + month_start + day - 1
}

pure persian_from_days(days: Int) -> CalendarDate {
  let number = days + persian_day_number(1405, 1, 1) - date_parse.days_from_civil(2026, 3, 21)
  var year = date_parse.floor_div(number, 365) + 1
  while persian_day_number(year + 1, 1, 1) <= number { year += 1 }
  while persian_day_number(year, 1, 1) > number { year -= 1 }
  let offset = number - persian_day_number(year, 1, 1)
  return {year: year, month: offset / 31 + 1, day: offset % 31 + 1} when offset < 186
  {year: year, month: (offset - 186) / 30 + 7, day: (offset - 186) % 30 + 1}
}

# Ethiopian years have 13 months, the last of 5 days or 6 in a leap year. Year Y
# is leap when Y is 3 modulo 4, so every four years span 1461 days.
pure ethiopian_from_days(days: Int) -> CalendarDate {
  let number = days + 2440588 - 1724221
  let cycle = date_parse.floor_div(number, 1461)
  let within = date_parse.positive_mod(number, 1461)
  let index = if within < 365 { 0 } else if within < 730 { 1 } else if within < 1096 { 2 } else { 3 }
  let start = if within < 365 { 0 } else if within < 730 { 365 } else if within < 1096 { 730 } else { 1096 }
  let offset = within - start
  {year: cycle * 4 + index + 1, month: offset / 30 + 1, day: offset % 30 + 1}
}

pure calendar_date(calendar: Str, year: Int, month: Int, day: Int) -> CalendarDate {
  match calendar {
    "buddhist" => {year: year + 543, month: month, day: day}
    "persian" => persian_from_days(date_parse.days_from_civil(year, month, day))
    "ethiopian" => ethiopian_from_days(date_parse.days_from_civil(year, month, day))
    else => {year: year, month: month, day: day}
  }
}

# Whether a format needs the per-directive path: names, a non-Gregorian calendar,
# or a year shifted for native conversion.
pure needs_pieces(style: DateStyle, shift: Int) -> Bool {
  shift != 0 or style.names != null or style.calendar != "gregorian"
}

# One directive or literal run. Directives that depend on the year, the calendar
# or the locale's names are computed here. The host formatter handles the rest,
# applied to the native instant, which shares the true weekday, month and day.
proc format_directive(instant: date_parse.Instant, spec: Str, utc: Bool, style: DateStyle) [time, error, process, env] -> Result[Str] {
  let native = time.format(date_parse.native_ns(instant)?, spec, utc:)?
  if (spec.byte_at(0) ?? 0) != 37 { return Ok(native) }
  let conversion = spec.byte_slice(spec.byte_len() - 1)
  let shift = date_parse.shift_years(instant)?
  let year_directive = conversion in ["Y", "C", "y", "F", "D", "x", "c", "G", "g", "s", "+"]
  let calendar_directive = conversion in ["Y", "C", "y", "m", "d", "e", "F", "D"]
  let name_directive = conversion in ["a", "A", "b", "B", "h"]
  if spec.byte_len() != 2 {
    if (shift != 0 and year_directive) or (style.calendar != "gregorian" and calendar_directive) or (style.names != null and name_directive) {
      gnu.error(f"format directive {spec} is not supported for this year or locale")
      exit 1
    }
    return Ok(native)
  }
  let name_set_for_locale = style.names
  if name_directive and name_set_for_locale != null {
    let names = name_set_for_locale
    let fields = date_parse.calendar(instant, utc)?
    let text = match conversion {
      "a" => names.short_weekdays[fields.weekday]
      "A" => names.weekdays[fields.weekday]
      "b" | "h" => names.short_months[fields.month - 1]
      else => names.months[fields.month - 1]
    }
    return Ok(text)
  }
  if shift == 0 and style.calendar == "gregorian" { return Ok(native) }
  let fields = date_parse.calendar(instant, utc)?
  let date = calendar_date(style.calendar, fields.year, fields.month, fields.day)
  # ISO week-based years repeat with the same 400-year cycle, so the shifted
  # native year only needs the shift added back.
  let iso_year = if shift != 0 and conversion in ["G", "g"] { time.format(date_parse.native_ns(instant)?, "%G", utc:)?.parse_int_decimal()? + shift } else { 0 }
  match conversion {
    "Y" => Ok(f"{date.year:04}")
    "C" => Ok(f"{date.year / 100:02}")
    "y" => Ok(f"{date_parse.positive_mod(date.year, 100):02}")
    "m" => Ok(f"{date.month:02}")
    "d" => Ok(f"{date.day:02}")
    "e" => Ok(f"{date.day:>2}")
    "F" => Ok(f"{date.year:04}-{date.month:02}-{date.day:02}")
    "D" => Ok(f"{date.month:02}/{date.day:02}/{date_parse.positive_mod(date.year, 100):02}")
    "s" => if shift != 0 { Ok(f"{instant.seconds}") } else { Ok(native) }
    "c" => if shift != 0 or style.names != null { format_text(instant, "%a %b %e %H:%M:%S %Y", utc, style) } else { Ok(native) }
    "x" => if shift != 0 or style.names != null { format_text(instant, "%m/%d/%y", utc, style) } else { Ok(native) }
    "G" => if shift != 0 { Ok(f"{iso_year:04}") } else { Ok(native) }
    "g" => if shift != 0 { Ok(f"{date_parse.positive_mod(iso_year, 100):02}") } else { Ok(native) }
    "+" => if shift != 0 {
      gnu.error(f"format directive {spec} is not supported for this year or locale")
      exit 1
    } else { Ok(native) }
    else => Ok(native)
  }
}

# The shared formatter enforces width and output limits before date adjusts %N padding.
proc format_text(instant: date_parse.Instant, format: Str, utc: Bool, style: DateStyle) [time, error, process, env] -> Result[Str] {
  let formatted = time.format(date_parse.native_ns(instant)?, format, utc:)?
  let empty_timezone = ! utc and env.get("TZ") is Ok("")
  if ! empty_timezone and ! needs_pieces(style, date_parse.shift_years(instant)?) and format.find("N") == null { return Ok(formatted) }
  var output = ""
  var index = 0
  let length = format.byte_len()
  while index < length {
    if (format.byte_at(index) ?? 0) != 37 {
      let start = index
      while index < length and (format.byte_at(index) ?? 0) != 37 { index += 1 }
      output = f"{output}{format_directive(instant, format.byte_slice(start, length: index - start), utc, style)?}"
      continue
    }
    let start = index
    index += 1
    if index >= length {
      output = f"{output}{format_directive(instant, format.byte_slice(start), utc, style)?}"
      break
    }
    if (format.byte_at(index) ?? 0) == 37 { output = f"{output}%"; index += 1; continue }
    var flags = ""
    while index < length and (format.byte_at(index) ?? 0) in [35, 43, 45, 48, 94, 95] {
      flags = f"{flags}{format.byte_slice(index, length: 1)}"
      index += 1
    }
    var width_text = ""
    while index < length and (format.byte_at(index) ?? 0) >= 48 and (format.byte_at(index) ?? 0) <= 57 {
      width_text = f"{width_text}{format.byte_slice(index, length: 1)}"
      index += 1
    }
    var colons = 0
    while index < length and (format.byte_at(index) ?? 0) == 58 { colons += 1; index += 1 }
    var modifier = ""
    if index < length and ((format.byte_at(index) ?? 0) == 69 or (format.byte_at(index) ?? 0) == 79) {
      modifier = format.byte_slice(index, length: 1)
      index += 1
    }
    if index >= length {
      output = f"{output}{format_directive(instant, format.byte_slice(start), utc, style)?}"
      break
    }
    let first_byte = format.byte_at(index) ?? 0
    let spec_width = if first_byte < 128 { 1 } else if first_byte < 224 { 2 } else if first_byte < 240 { 3 } else { 4 }
    let end = index + spec_width
    let specifier = format.byte_slice(index, length: spec_width)
    let piece = if specifier == "Z" and empty_timezone and flags == "" and width_text == "" and modifier == "" and colons == 0 {
      "Universal"
    } else if specifier == "N" and colons == 0 {
      if modifier == "E" { format.byte_slice(start, length: end - start) } else { format_nanoseconds(instant, flags, width_text, utc:)? }
    } else {
      format_directive(instant, format.byte_slice(start, length: end - start), utc, style)?
    }
    output = f"{output}{piece}"
    index = end
  }
  Ok(output)
}

# Debug offsets omit minutes and seconds when both are zero.
pure debug_offset(seconds: Int) -> Str {
  let sign = if seconds < 0 { "-" } else { "+" }
  let magnitude = if seconds < 0 { -seconds } else { seconds }
  f"{sign}{magnitude / 3600:02}{if magnitude % 3600 == 0 { "" } else { f":{magnitude / 60 % 60:02}{if magnitude % 60 == 0 { "" } else { f":{magnitude % 60:02}" }}" }}"
}

# Parsing and tracing share one baseline, so relative values cannot cross a clock
# tick between their starting value and the emitted instant.
proc debug_date(text: Str, instant: date_parse.Instant, base: date_parse.Instant, utc: Bool) [time, env, error, process] -> Result[Unit] {
  let style: DateStyle = {names: null, calendar: "gregorian"}
  let timezone = if utc { "UTC0" } else { env.get_or("TZ", "") ?? "" }
  let input_zone = if timezone == "UTC0" { "TZ=\"UTC0\" environment value or -u" } else if env.get("TZ") is Ok(_) { f"TZ=\"{timezone}\" environment value" } else { "system default" }
  let final_zone = if timezone == "UTC0" { "Universal Time" } else if env.get("TZ") is Ok(_) { f"TZ=\"{timezone}\" environment value" } else { "system default" }
  let input = text.trim()
  if input.starts_with("@") {
    gnu.error(f"parsed number of seconds part: number of seconds: {instant.seconds}{if instant.nanoseconds == 0 { "" } else { f".{instant.nanoseconds:09}" }}")
    gnu.error("input timezone: '@timespec' - always UTC")
  } else {
    let relative = rx"^(.*?)([+-]?[0-9]+)\s+(years?|months?|fortnights?|weeks?|days?|hours?|minutes?|mins?|seconds?|secs?)(\s+ago)?$".captures(input.lower())
    let prefix = if relative.is_empty() { input } else { relative[1].trim() }
    let starting = if prefix == "" and ! relative.is_empty() { base } else { parse_date(prefix, utc:, base:)? }
    var count = if relative.is_empty() { 0 } else { (if relative[2].starts_with("+") { relative[2].byte_slice(1) } else { relative[2] }).parse_int_decimal()? * (if relative[4] == "" { 1 } else { -1 }) }
    var unit = if relative.is_empty() { "" } else { relative[3] }
    if unit.ends_with("s") { unit = unit.byte_slice(0, length: unit.byte_len() - 1) }
    if unit == "week" { unit = "day"; count *= 7 }
    if unit == "fortnight" { unit = "day"; count *= 14 }
    if unit == "min" { unit = "minute" }
    if unit == "sec" { unit = "second" }
    let iso = rx"^([0-9]{4,}-[0-9]{1,2}-[0-9]{1,2})(?:[ T](.*))?$".captures(prefix)
    let clock = rx"([0-9]{1,2}:[0-9]{2}(?::[0-9]{2}(?:\.[0-9]+)?)?)".captures(prefix)
    let military = rx"^([A-Za-z])([0-9]{1,2})$".captures(prefix)
    let number = rx"^[0-9]{1,4}[jJ]?$".matches(prefix) or (prefix == "" and relative.is_empty())
    var offset: Int? = null
    var civil = starting
    let zone = rx"^(.*\S)\s+([A-Za-z]+|[+-][0-9]{1,6}(?::[0-9]{2}){0,2})$".captures(prefix)
    if ! zone.is_empty() and (! clock.is_empty() or ! iso.is_empty()) {
      if let Ok(unzoned) = parse_date(zone[1], utc: true, base:) {
        offset = unzoned.seconds - starting.seconds
        civil = unzoned
      }
    } else if ! military.is_empty() {
      let today = format_text(base, "%F", utc, style)?
      civil = parse_date(f"{today} {military[2]}:00", utc: true, base:)?
      offset = civil.seconds - starting.seconds
    }
    let civil_utc = utc or offset != null
    let start_date = format_text(civil, "%F", utc: civil_utc, style)?
    let start_time = format_text(civil, "%T", utc: civil_utc, style)?
    let dated = ! iso.is_empty() or (rx"[0-9]{4}".matches(prefix) and ! number)
    if dated { gnu.error(f"parsed date part: (Y-M-D) {start_date}") }
    if ! clock.is_empty() { gnu.error(f"parsed time part: {start_time}") }
    if ! military.is_empty() { gnu.error(f"parsed zone part: UTC{debug_offset(offset ?? 0)}") }
    if number or ! military.is_empty() { gnu.error(f"parsed number part: {start_time}") }
    if offset != null and military.is_empty() { gnu.error(f"parsed zone part: UTC{debug_offset(offset)}") }
    if ! relative.is_empty() { gnu.error(f"parsed relative part: {if count < 0 { "" } else { "+" }}{count} {unit}(s)") }
    gnu.error(f"input timezone: {if offset == null { input_zone } else { f"parsed date/time string ({debug_offset(offset)})" }}")
    if ! clock.is_empty() or number or ! military.is_empty() {
      gnu.error(f"using specified time as starting value: '{start_time}'")
    } else if prefix == "" {
      gnu.error(f"using current time as starting value: '{start_time}'")
    } else {
      gnu.error(f"warning: using midnight as starting time: {start_time}")
    }
    if ! dated { gnu.error(f"using current date as starting value: '(Y-M-D) {start_date}'") }
    let adjusted_civil: date_parse.Instant = if offset == null { instant } else { {seconds: instant.seconds + offset, nanoseconds: instant.nanoseconds} }
    let suffix = if offset == null { "" } else { f" TZ={debug_offset(offset)}" }
    gnu.error(f"starting date/time: '(Y-M-D) {start_date} {start_time}{suffix}'")
    if unit in ["year", "month", "day"] {
      if unit == "day" and start_time != "12:00:00" { gnu.error("warning: when adding relative days, it is recommended to specify noon") }
      if unit in ["year", "month"] { gnu.error("warning: when adding relative months/years, it is recommended to specify the 15th of the months") }
      gnu.error(f"after date adjustment ({if unit == "year" { f"{if count < 0 { "" } else { "+" }}{count}" } else { "+0" }} years, {if unit == "month" { f"{if count < 0 { "" } else { "+" }}{count}" } else { "+0" }} months, {if unit == "day" { f"{if count < 0 { "" } else { "+" }}{count}" } else { "+0" }} days),")
      gnu.error(f"    new date/time = '(Y-M-D) {format_text(adjusted_civil, "%F %T", utc: civil_utc, style)?}{suffix}'")
    }
    gnu.error(f"'(Y-M-D) {format_text(adjusted_civil, "%F %T", utc: civil_utc, style)?}{suffix}' = {instant.seconds} epoch-seconds")
  }
  gnu.error(f"timezone: {final_zone}")
  gnu.error(f"final: {instant.seconds}.{instant.nanoseconds:09} (epoch-seconds)")
  gnu.error(f"final: (Y-M-D) {format_text(instant, "%F %T", utc: true, style)?} (UTC)")
  let zone_text = format_text(instant, "%z", utc, style)?
  let zone_sign = if zone_text.starts_with("-") { -1 } else { 1 }
  let zone_seconds = zone_sign * (((zone_text.byte_at(1) ?? 48) - 48) * 36000 + ((zone_text.byte_at(2) ?? 48) - 48) * 3600 + ((zone_text.byte_at(3) ?? 48) - 48) * 600 + ((zone_text.byte_at(4) ?? 48) - 48) * 60)
  gnu.error(f"final: (Y-M-D) {format_text(instant, "%F %T", utc, style)?} (UTC{debug_offset(zone_seconds)})")
}

# Date-only inputs inherit midnight; epoch timestamps already identify a complete instant.
proc emit_date(raw: Bytes, format: Bytes, utc: Bool, debug: Bool, style: DateStyle) [time, process, env, io, error] -> Bool {
  # A date with undecodable bytes cannot match any date syntax, so it is reported
  # with octal escapes instead of being parsed.
  var text = ""
  match raw.utf8() {
    Ok(value) => text = value
    Err(_) => {
      gnu.error(f"invalid date {gnu.quote_value_bytes(raw)}")
      return false
    }
  }
  let base = date_parse.instant_from_ns(time.now() * 1000000)
  match parse_date(text, utc:, base:) {
    Ok(epoch) => {
      if debug { if let Err(failure) = debug_date(text, epoch, base, utc) { gnu.error(failure.message); return false } }
      if debug { gnu.error(f"output format: {gnu.quote_value_bytes(format)}") }
      match format_date(epoch, format, utc, style) {
        Ok(output) => { gnu.write_bytes(bytes.concat([output, b"\n"])); return true }
        Err(failure) => gnu.error(failure.message)
      }
    }
    Err(failure) => gnu.error(f"invalid date {gnu.quote(text)}")
  }
  false
}

proc main(...raw: List[Bytes]) [time, process, env, io, fs, error] {
  let prepared = gnu.prepare_arguments(raw)
  var style = date_style(locale_name())
  var argv: List[Str] = []
  for item in prepared.text {
    if item.starts_with("-") and ! item.starts_with("--") and item.byte_len() > 2 {
      var rest = item.byte_slice(1)
      while rest != "" {
        let first = rest.byte_slice(0, 1)
        if first == "u" or first == "R" { argv += [f"-{first}"]; rest = rest.byte_slice(1) } else { argv += [f"-{rest}"]; break }
      }
    } else { argv += [item] }
  }
  var utc = false
  var date: Bytes = b"now"
  var file: Bytes = b""
  var reference: Bytes = b""
  var setting = false
  var set_option = false
  var format: Bytes = b"%a %b %e %H:%M:%S %Z %Y"
  var specified_format = false
  var source = ""
  var resolution = false
  var debug = false
  var index = 0
  var operands = false
  var date_operand = false
  while index < argv.len() {
    let arg = argv[index]
    index += 1
    if ! operands and arg == "--" { operands = true; continue }
    if ! operands and arg == "--help" {
      gnu.help("Usage: date [OPTION]... [+FORMAT]\nDisplay a calendar date.\n  -d, --date=STRING       display STRING\n  -f, --file=FILE         display each date in FILE\n  -r, --reference=FILE    display FILE modification time\n  -u, --utc              use UTC\n  -R, --rfc-email        RFC email format\n  -I[TIMESPEC]           ISO 8601 format\n      --rfc-3339=SPEC     RFC 3339 format\n      --debug              annotate parsed date input\n  -s, --set=STRING        set system clock\n      --resolution       display clock resolution")
      return
    }
    if ! operands and arg == "--version" { gnu.version("date"); return }
    if ! operands and arg == "--debug" { debug = true; continue }
    if ! operands and (arg == "-u" or arg == "--utc" or arg == "--universal" or arg == "--uct" or arg == "--uni" or arg == "--u") { utc = true; continue }
    # RFC 5322 dates use the C names and Gregorian fields whatever the locale.
    if ! operands and (arg == "-R" or arg == "--rfc-email" or arg == "--rfc-822" or arg == "--rfc-2822" or arg == "--rfc-e") { format = b"%a, %d %b %Y %H:%M:%S %z"; style = {names: null, calendar: "gregorian"}; continue }
    if ! operands and arg == "--resolution" { resolution = true; continue }
    if ! operands and (arg.starts_with("-I") or arg.starts_with("--iso-8601") or arg == "--i" or arg.starts_with("--i=") or arg.starts_with("--rfc-3339") or arg.starts_with("--rfc-3=")) {
      var spec = "date"
      var rfc = arg.starts_with("--rfc-3339") or arg.starts_with("--rfc-3=")
      if arg.starts_with("-I") { spec = arg.byte_slice(2) } else if arg.find("=") != null { spec = arg.split("=", maxsplit: 1)[1] } else if rfc { gnu.usage_error("option '--rfc-3339' requires an argument") }
      if spec == "" { spec = "date" }
      let separator = if rfc { " " } else { "T" }
      style = {names: style.names, calendar: "gregorian"}
      match spec {
        "date" => format = b"%Y-%m-%d"
        "hour" | "hours" => format = bytes.from_text(f"%Y-%m-%d{separator}%H%:z")
        "minute" | "minutes" => format = bytes.from_text(f"%Y-%m-%d{separator}%H:%M%:z")
        "second" | "seconds" => format = bytes.from_text(f"%Y-%m-%d{separator}%H:%M:%S%:z")
        "ns" => { let decimal = if rfc { "." } else { "," }; format = bytes.from_text(f"%Y-%m-%d{separator}%H:%M:%S{decimal}%N%:z") }
        else => gnu.usage_error(f"invalid argument {gnu.quote(spec)}")
      }
      continue
    }
    if ! operands and (arg == "-d" or arg == "--date" or arg == "-f" or arg == "--file" or arg == "-r" or arg == "--reference" or arg == "-s" or arg == "--set" or arg.starts_with("--date=") or arg.starts_with("--file=") or arg.starts_with("--reference=") or arg.starts_with("--set=") or (arg.starts_with("-d") and arg.byte_len() > 2) or (arg.starts_with("-f") and arg.byte_len() > 2) or (arg.starts_with("-r") and arg.byte_len() > 2) or (arg.starts_with("-s") and arg.byte_len() > 2)) {
      var value = ""
      if arg.starts_with("--") and arg.find("=") != null { value = arg.split("=", maxsplit: 1)[1] } else if ! arg.starts_with("--") and arg.byte_len() > 2 { value = arg.byte_slice(2) } else {
        if index >= argv.len() { gnu.usage_error(f"option {gnu.quote(arg)} requires an argument") }
        value = argv[index]; index += 1
      }
      let kind = if arg.starts_with("-f") or arg.starts_with("--file") { "file" } else if arg.starts_with("-r") or arg.starts_with("--reference") { "reference" } else if arg.starts_with("-s") or arg.starts_with("--set") { "set" } else { "date" }
      if source != "" and source != kind { gnu.usage_error("the options to specify dates for printing are mutually exclusive") }
      source = kind
      let operand = gnu.argument_bytes(value, prepared.raw)
      if arg.starts_with("-f") or arg.starts_with("--file") { file = operand } else if arg.starts_with("-r") or arg.starts_with("--reference") { reference = operand } else { date = operand; setting = arg.starts_with("-s") or arg.starts_with("--set"); set_option = setting }
      continue
    }
    let operand = gnu.argument_bytes(arg, prepared.raw)
    if operand.byte_at(0) == 43 {
      if specified_format { reject_extra_operand(operand) }
      format = operand[1..operand.len()]; specified_format = true
    } else if ! operands and arg.starts_with("-") and arg != "-" { gnu.usage_error(if arg.starts_with("--") { f"unrecognized option {gnu.quote(arg)}" } else { f"invalid option -- {gnu.quote(arg.byte_slice(1, length: 1))}" }) } else {
      # A format operand takes the place of the date, so a second non-format operand is extra.
      if date_operand or (specified_format and source == "") { reject_extra_operand(operand) }
      if source != "" {
        gnu.usage_error(f"the argument {gnu.quote_value_bytes(operand)} lacks a leading '+';\nwhen using an option to specify date(s), any non-option\nargument must be a format string beginning with '+'")
        exit 1
      }
      source = "set"; date = operand; setting = true; date_operand = true
    }
  }
  if resolution {
    if source != "" { gnu.usage_error("the options to specify dates for printing are mutually exclusive") }
    let nanos = time.clock_resolution()?
    if specified_format or format != b"%a %b %e %H:%M:%S %Z %Y" { gnu.write_bytes(bytes.concat([format_date(date_parse.instant_from_ns(nanos), format, utc, style)?, b"\n"])) } else { gnu.write_text(f"{nanos / 1000000000}.{format_text(date_parse.instant_from_ns(nanos % 1000000000), "%N", utc: true, style)?}\n") }
    return
  }
  if source == "reference" {
    match fs.stat(Path.parse_bytes(reference)?, follow_symlinks: true) {
      Ok(meta) => { gnu.write_bytes(bytes.concat([format_date(date_parse.instant_from_ns(meta.mtime_ns), format, utc, style)?, b"\n"])); return }
      Err(failure) => { gnu.error(f"{gnu.quote_bytes(reference, always: false)}: {gnu.strerror(failure)}"); exit 1 }
    }
  }
  if source == "file" {
    let from_stdin = file == b"-"
    if ! from_stdin and (Path.parse_bytes(file)?.is_dir() ?? false) { gnu.error(f"{gnu.quote_bytes(file, always: false)}: read error: Is a directory"); exit 1 }
    var contents = b""
    let read = if from_stdin { io.stdin_bytes() } else { Path.parse_bytes(file)?.read_bytes() }
    match read {
      Ok(data) => contents = data
      Err(failure) => { gnu.error(f"{gnu.quote_bytes(file, always: false)}: {gnu.strerror(failure)}"); exit 1 }
    }
    var success = true
    for raw_line in contents.lines() {
      var end = 0
      while end < raw_line.len() and raw_line.byte_at(end) != 0 { end += 1 }
      let line = raw_line[0..end]
      if ! emit_date(line, format, utc, debug, style) { success = false }
    }
    if ! success { exit 1 }
    return
  }
  if setting {
    if date_operand and date.len() == 0 { gnu.error(f"invalid date {gnu.quote_bytes(date)}"); exit 1 }
    var text = ""
    match date.utf8() {
      Ok(value) => text = value
      Err(_) => { gnu.error(f"invalid date {gnu.quote_value_bytes(date)}"); exit 1 }
    }
    let parsed = if set_option { parse_date(text, utc:) } else { date_parse.parse_instant(text, utc:) }
    match parsed {
      Ok(epoch) => {
        match date_parse.instant_ns(epoch) {
          Ok(nanos) => {
            if let Err(failure) = linux.set_system_clock(nanos / 1000000) {
              gnu.error(f"cannot set date: {gnu.strerror(failure)}")
              let _ = emit_date(date, format, utc, false, style)
              exit 1
            }
          }
          Err(failure) => { gnu.error(f"invalid date {gnu.quote(text)}"); exit 1 }
        }
      }
      Err(failure) => { gnu.error(f"invalid date {gnu.quote(text)}"); exit 1 }
    }
  }
  if debug and source == "" { gnu.error(f"output format: {gnu.quote_value_bytes(format)}") }
  if ! emit_date(date, format, utc, debug and source == "date", style) { exit 1 }
}
