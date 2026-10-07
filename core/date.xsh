#!/bin/xsh
use lib.gnu
use lib.date_parse

# GNU date treats a lone dash like an empty date expression, which means midnight today.
proc parse_date(text: Str, utc: Bool) [time, error] -> Result[Int, Error] {
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
  date_parse.parse(input, utc:)
}

# GNU date removes fractional trailing zeroes up to the requested precision, then pads after the digits.
proc format_nanoseconds(epoch_ns: Int, flags: Str, width_text: Str, utc: Bool) [time, error] -> Result[Str] {
  var width = if width_text == "" { 9 } else { width_text.parse_int_decimal()? }
  if width <= 0 { width = 9 }
  let digits = time.format(epoch_ns, "%9N", utc:)?
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
    return time.format(epoch_ns, f"%{width}N", utc:)
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

proc format_date(epoch_ns: Int, format: Str, utc: Bool) [time, error] -> Result[Str] {
  # The shared formatter enforces width and output limits before date adjusts %N padding.
  let formatted = time.format(epoch_ns, format, utc:)?
  if format.find("N") == null { return Ok(formatted) }
  var output = ""
  var index = 0
  let length = format.byte_len()
  while index < length {
    if (format.byte_at(index) ?? 0) != 37 {
      let start = index
      while index < length and (format.byte_at(index) ?? 0) != 37 { index += 1 }
      output = f"{output}{time.format(epoch_ns, format.byte_slice(start, length: index - start), utc:)?}"
      continue
    }
    let start = index
    index += 1
    if index >= length {
      output = f"{output}{time.format(epoch_ns, format.byte_slice(start), utc:)?}"
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
      output = f"{output}{time.format(epoch_ns, format.byte_slice(start), utc:)?}"
      break
    }
    let first_byte = format.byte_at(index) ?? 0
    let spec_width = if first_byte < 128 { 1 } else if first_byte < 224 { 2 } else if first_byte < 240 { 3 } else { 4 }
    let end = index + spec_width
    let specifier = format.byte_slice(index, length: spec_width)
    let piece = if specifier == "N" and colons == 0 {
      if modifier == "E" { format.byte_slice(start, length: end - start) } else { format_nanoseconds(epoch_ns, flags, width_text, utc:)? }
    } else {
      time.format(epoch_ns, format.byte_slice(start, length: end - start), utc:)?
    }
    output = f"{output}{piece}"
    index = end
  }
  Ok(output)
}

pure has_explicit_time(input: Str) -> Bool {
  input.starts_with("@") or rx"[0-9]{1,2}:[0-9]{2}".matches(input) or rx"^[A-Za-z][0-9]{1,2}$".matches(input) or rx"^[0-9]{3,4}[jJ]?$".matches(input)
}

# Date-only inputs inherit midnight; epoch timestamps already identify a complete instant.
proc emit_date(text: Str, format: Str, utc: Bool, debug: Bool) [time, process, env, io, error] -> Bool {
  match parse_date(text, utc:) {
    Ok(epoch) => {
      if debug {
        gnu.error(f"input string: {text}")
        gnu.error(f"parsed date part: (Y-M-D) {time.format(epoch, "%F", utc:)?}")
        gnu.error(f"parsed time part: (H:M:S) {time.format(epoch, "%T", utc:)?}")
        gnu.error(f"input timezone: {time.format(epoch, "%Z", utc:)?}")
        if ! has_explicit_time(text) {
          gnu.error("warning: using midnight")
        }
      }
      match format_date(epoch, format, utc:) {
        Ok(output) => { gnu.write_text(f"{output}\n"); return true }
        Err(failure) => gnu.error(failure.message)
      }
    }
    Err(failure) => gnu.error(f"invalid date {gnu.quote(text)}")
  }
  false
}

proc main(...raw: List[Str]) [time, process, env, io, fs, error] {
  var argv: List[Str] = []
  for item in raw {
    if item.starts_with("-") and ! item.starts_with("--") and item.byte_len() > 2 {
      var rest = item.byte_slice(1)
      while rest != "" {
        let first = rest.byte_slice(0, 1)
        if first == "u" or first == "R" { argv += [f"-{first}"]; rest = rest.byte_slice(1) } else { argv += [f"-{rest}"]; break }
      }
    } else { argv += [item] }
  }
  var utc = false
  var date = "now"
  var file = ""
  var reference = ""
  var setting = false
  var set_option = false
  var format = "%a %b %e %H:%M:%S %Z %Y"
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
    if ! operands and (arg == "-R" or arg == "--rfc-email" or arg == "--rfc-822" or arg == "--rfc-2822" or arg == "--rfc-e") { format = "%a, %d %b %Y %H:%M:%S %z"; continue }
    if ! operands and arg == "--resolution" { resolution = true; continue }
    if ! operands and (arg.starts_with("-I") or arg.starts_with("--iso-8601") or arg == "--i" or arg.starts_with("--i=") or arg.starts_with("--rfc-3339") or arg.starts_with("--rfc-3=")) {
      var spec = "date"
      var rfc = arg.starts_with("--rfc-3339") or arg.starts_with("--rfc-3=")
      if arg.starts_with("-I") { spec = arg.byte_slice(2) } else if arg.find("=") != null { spec = arg.split("=", maxsplit: 1)[1] } else if rfc { gnu.usage_error("option '--rfc-3339' requires an argument") }
      if spec == "" { spec = "date" }
      let separator = if rfc { " " } else { "T" }
      match spec {
        "date" => format = "%Y-%m-%d"
        "hour" | "hours" => format = f"%Y-%m-%d{separator}%H%:z"
        "minute" | "minutes" => format = f"%Y-%m-%d{separator}%H:%M%:z"
        "second" | "seconds" => format = f"%Y-%m-%d{separator}%H:%M:%S%:z"
        "ns" => { let decimal = if rfc { "." } else { "," }; format = f"%Y-%m-%d{separator}%H:%M:%S{decimal}%N%:z" }
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
      if arg.starts_with("-f") or arg.starts_with("--file") { file = value } else if arg.starts_with("-r") or arg.starts_with("--reference") { reference = value } else { date = value; setting = arg.starts_with("-s") or arg.starts_with("--set"); set_option = setting }
      continue
    }
    if arg.starts_with("+") {
      if specified_format { gnu.extra_operand(arg) }
      format = arg.byte_slice(1); specified_format = true
    } else if ! operands and arg.starts_with("-") and arg != "-" { gnu.usage_error(f"unexpected argument {gnu.quote(arg)}") } else {
      if source != "" {
        if date_operand { gnu.extra_operand(arg) }
        gnu.error(f"the argument {arg} lacks a leading '+';\nwhen using an option to specify date(s), any non-option\nargument must be a format string beginning with '+'")
        exit 1
      }
      source = "set"; date = arg; setting = true; date_operand = true
    }
  }
  if resolution {
    if source != "" { gnu.usage_error("the options to specify dates for printing are mutually exclusive") }
    let nanos = time.clock_resolution()?
    if specified_format or format != "%a %b %e %H:%M:%S %Z %Y" { gnu.write_text(f"{format_date(nanos, format, utc:)?}\n") } else { gnu.write_text(f"{nanos / 1000000000}.{format_date(nanos % 1000000000, "%N", utc: true)?}\n") }
    return
  }
  if source == "reference" {
    match fs.stat(fp"{reference}", follow_symlinks: true) {
      Ok(meta) => { gnu.write_text(f"{format_date(meta.mtime_ns, format, utc:)?}\n"); return }
      Err(failure) => { gnu.name_error(reference, failure); exit 1 }
    }
  }
  if source == "file" {
    if file != "-" and (fp"{file}".is_dir() ?? false) { gnu.error(f"expected file, got directory {gnu.quote(file)}"); exit 1 }
    var contents = b""
    match gnu.read_operand(file) {
      Ok(data) => contents = data
      Err(failure) => { gnu.name_error(file, failure); exit 1 }
    }
    var success = true
    for raw_line in contents.lines() {
      var end = 0
      while end < raw_line.len() and raw_line.byte_at(end) != 0 { end += 1 }
      let line = raw_line[0..end]
      match line.utf8() {
        Ok(text) => { if ! emit_date(text, format, utc, debug) { success = false } }
        Err(failure) => { gnu.error(f"invalid date {gnu.quote_value_bytes(line)}"); success = false }
      }
    }
    if ! success { exit 1 }
    return
  }
  if setting {
    if date_operand and date == "" { gnu.error(f"invalid date {gnu.quote(date)}"); exit 1 }
    let parsed = if set_option { parse_date(date, utc:) } else { date_parse.parse(date, utc:) }
    match parsed {
      Ok(epoch) => {
        if let Err(failure) = linux.set_system_clock(epoch / 1000000) {
          gnu.error(f"cannot set date: {gnu.strerror(failure)}")
          let _ = emit_date(date, format, utc, false)
          exit 1
        }
      }
      Err(failure) => { gnu.error(f"invalid date {gnu.quote(date)}"); exit 1 }
    }
  }
  if ! emit_date(date, format, utc, debug and source == "date") { exit 1 }
}
