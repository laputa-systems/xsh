#!/bin/xsh
use lib.gnu as gnu

const USAGE = "Usage: date [OPTION]... [+FORMAT]\n  or:  date [-u|--utc|--universal] [MMDDhhmm[[CC]YY][.ss]]\n\nDisplay the current time in the given FORMAT.\n\n  -d, --date=STRING        display time described by STRING, not 'now'\n      --debug              annotate parsed dates and warn about questionable use\n  -f, --file=DATEFILE      like --date; once for each line of DATEFILE\n  -I[FMT], --iso-8601[=FMT]  output ISO 8601 date/time (default: date)\n  -r, --reference=FILE     display the last modification time of FILE\n  -R, --rfc-email          output date and time in RFC 5322 format\n      --rfc-3339=FMT       output date/time in RFC 3339 format\n  -s, --set=STRING         set system time\n  -u, --utc, --universal   use Coordinated Universal Time (UTC)\n      --help               display this help and exit\n      --version            output version information and exit"

const MAX_FORMAT_WIDTH = 65535
const ABSENT = "__xsh_date_argument_not_given__"

type Moment = {seconds: Int, nanoseconds: Int}
type DateOptions = {
  date: Str,
  file: Str,
  reference: Str,
  utc: Bool,
  iso: Str,
  rfc_email: Bool,
  rfc3339: Str,
  resolution: Bool,
  debug: Bool,
  set: Str,
  help: Bool,
  version: Bool,
  formats: List[Str],
}
type DateFormat = {bytes: Bytes}

pure width_is_bounded(format: Bytes) -> Bool {
  var at = 0
  let size = format.len()

  while at < size {
    if (format.byte_at(at) ?? 0) != 37 {
      at += 1
      continue
    }

    at += 1
    if at < size and (format.byte_at(at) ?? 0) == 37 {
      at += 1
      continue
    }

    while at < size and (format.byte_at(at) ?? 0) in [45, 95, 48, 94, 35, 43] {
      at += 1
    }

    var width = 0
    while at < size {
      let byte = format.byte_at(at) ?? 0
      if byte < 48 or byte > 57 { break }
      let digit = byte - 48
      if width > (MAX_FORMAT_WIDTH - digit) / 10 { return false }
      width = width * 10 + digit
      at += 1
    }

    while at < size and (format.byte_at(at) ?? 0) == 58 { at += 1 }
    if at < size { at += 1 }
  }

  true
}

pure oversized_width_error(format: Bytes) -> Str? {
  var at = 0
  let size = format.len()

  while at < size {
    if (format.byte_at(at) ?? 0) != 37 {
      at += 1
      continue
    }

    at += 1
    if at < size and (format.byte_at(at) ?? 0) == 37 {
      at += 1
      continue
    }

    while at < size and (format.byte_at(at) ?? 0) in [45, 95, 48, 94, 35, 43] {
      at += 1
    }

    let width_start = at
    while at < size {
      let byte = format.byte_at(at) ?? 0
      if byte < 48 or byte > 57 { break }
      at += 1
    }
    if at == width_start { continue }

    let width = format[width_start..at].utf8() ?? ""
    let parsed = width.parse_int() ?? MAX_FORMAT_WIDTH + 1
    if parsed <= MAX_FORMAT_WIDTH { continue }

    while at < size and (format.byte_at(at) ?? 0) == 58 { at += 1 }
    if at < size and (format.byte_at(at) ?? 0) in [69, 79] { at += 1 }
    if at < size {
      let specifier = format[at..at + 1].utf8() ?? "?"
      return f"format modifier width '{width}' is too large for specifier '%{specifier}'"
    }
    return f"format modifier width '{width}' exceeds {MAX_FORMAT_WIDTH}"
  }

  null
}

pure shortcut(value: Str, choices: List[Str]) -> Str? {
  for choice in choices {
    return choice when choice == value
  }

  var found = ""
  for choice in choices {
    if choice.starts_with(value) {
      return null when found != ""
      found = choice
    }
  }

  if found == "" { null } else { found }
}

pure has_long_option(args: List[Str], name: Str) -> Bool {
  for arg in args {
    if arg == name or arg.starts_with(f"{name}=") { return true }
  }
  false
}

pure has_date_long_option_prefix(name: Str) -> Bool {
  let names = ["--date", "--debug", "--file", "--iso-8601", "--i", "--reference", "--rfc-email", "--rfc-e", "--rfc-822", "--rfc-2822", "--rfc-3339", "--rfc-3", "--resolution", "--set", "--utc", "--universal", "--uct", "--uni", "--help", "--version"]
  for known in names {
    if known.starts_with(name) { return true }
  }
  false
}

pure unexpected_date_option(args: List[Str]) -> Str? {
  var at = 0
  while at < args.len() {
    let arg = args[at]
    if arg == "--" { return null }
    if arg.starts_with("--") {
      let equal = arg.find("=") ?? arg.byte_len()
      let name = arg.byte_slice(0, length: equal)
      if ! has_date_long_option_prefix(name) { return arg }
      if equal == arg.byte_len() and name in ["--date", "--file", "--reference", "--rfc-3339", "--rfc-3", "--set"] {
        at += if at + 1 < args.len() { 2 } else { 1 }
        continue
      }
    } else if arg.starts_with("-") and arg != "-" {
      let short = arg.byte_slice(1)
      var pos = 0
      while pos < short.byte_len() {
        let letter = short.byte_slice(pos, length: 1)
        if letter in ["d", "f", "r", "s", "I"] { break }
        if letter != "u" and letter != "R" { return f"-{letter}" }
        pos += 1
      }
      if short in ["d", "f", "r", "s"] and at + 1 < args.len() {
        at += 2
        continue
      }
    }
    at += 1
  }
  null
}

pure argument_bytes(args: List[Str], raw_args: List[Bytes], value: Str, options: List[Str]) -> Bytes {
  for index in range(args.len()) {
    let arg = args[index]
    if arg == value { return raw_args[index] }

    for option in options {
      if arg.starts_with(f"{option}=") {
        return raw_args[index][option.byte_len() + 1..raw_args[index].len()]
      }
      if ! option.starts_with("--") and arg.starts_with(option) and arg != option {
        return raw_args[index][option.byte_len()..raw_args[index].len()]
      }
      if arg == option and index + 1 < args.len() and args[index + 1] == value {
        return raw_args[index + 1]
      }
    }
  }
  bytes.from_text(value)
}

pure strip_comments(text: Str) -> Str {
  var depth = 0
  var out = ""
  for char in text {
    if char == "(" {
      depth += 1
    } else if char == ")" and depth > 0 {
      depth -= 1
    } else if depth == 0 {
      out = f"{out}{char}"
    }
  }
  out.trim()
}

pure normalize_au_zone(text: Str) -> Str {
  let zones = ["AWST", "ACST", "ACDT", "AEST", "AEDT"]
  let offsets = ["+0800", "+0930", "+1030", "+1000", "+1100"]
  for at in range(zones.len()) {
    let suffix = f" {zones[at]}"
    if text.ends_with(suffix) {
      return f"{text.byte_slice(0, length: text.byte_len() - suffix.byte_len())} {offsets[at]}"
    }
  }
  text
}

pure debug_needs_midnight_warning(text: Str) -> Bool {
  let trimmed = text.trim()
  trimmed == "" or (! trimmed.starts_with("@") and ":" not in trimmed and ! trimmed.starts_with("m"))
}

pure octal_byte(byte: Int) -> Str {
  f"\\{byte / 64}{byte / 8 % 8}{byte % 8}"
}

pure escaped_invalid_line(raw: Bytes) -> Str {
  var out = ""
  for at in range(raw.len()) {
    let byte = raw.byte_at(at) ?? 0
    if byte >= 32 and byte < 127 and byte != 92 {
      out = f"{out}{raw[at..at + 1].utf8() ?? ""}"
    } else {
      out = f"{out}{octal_byte(byte)}"
    }
  }
  out
}

pure quote_octal_operand(raw: Bytes) -> Str {
  var out = ""
  for at in range(raw.len()) {
    let byte = raw.byte_at(at) ?? 0
    if byte >= 32 and byte < 127 and byte != 39 and byte != 92 {
      out = f"{out}{raw[at..at + 1].utf8() ?? ""}"
    } else {
      out = f"{out}{octal_byte(byte)}"
    }
  }
  f"'{out}'"
}

pure input_lines(raw: Bytes) -> List[Bytes] {
  var lines: List[Bytes] = []
  var start = 0
  for at in range(raw.len()) {
    if raw.byte_at(at) == 10 {
      let end = if at > start and raw.byte_at(at - 1) == 13 { at - 1 } else { at }
      let line = raw[start..end]
      var end_line = line.len()
      for index in range(line.len()) {
        if line.byte_at(index) == 0 { end_line = index; break }
      }
      lines += [line[0..end_line]]
      start = at + 1
    }
  }
  if start < raw.len() {
    let line = raw[start..raw.len()]
    var end_line = line.len()
    for index in range(line.len()) {
      if line.byte_at(index) == 0 { end_line = index; break }
    }
    lines += [line[0..end_line]]
  }
  lines
}

proc debug_parse(text: Str, moment: Moment, timezone: Str) [process, env, time] {
  gnu.error(f"input string: {text}")
  let day = (format_moment(moment, b"%F", timezone, "locale", "locale") ?? b"").utf8() ?? ""
  let clock = (format_moment(moment, b"%T", timezone, "locale", "locale") ?? b"").utf8() ?? ""
  gnu.error(f"parsed date part: (Y-M-D) {day}")
  gnu.error(f"parsed time part: (H:M:S) {clock}")
  let input_timezone = env.get_or("TZ", "local") ?? "local"
  gnu.error(f"input timezone: {input_timezone}")
  if debug_needs_midnight_warning(text) { gnu.error("warning: using midnight") }
}

proc parse_date(text: Str, now: Moment, timezone: Str, debug: Bool) [time, error, process, env] -> Result[Moment, Error] {
  let input = strip_comments(text)
  if input == "" or input == "-" {
    let day = time.format(now.seconds, now.nanoseconds, "%F", timezone, "gregorian", "locale")?
    let moment = time.parse(f"{day} 00:00:00", now.seconds, timezone)?
    if debug { debug_parse(text, moment, timezone) }
    return Ok(moment)
  }

  let moment = time.parse(normalize_au_zone(input), now.seconds, timezone)?
  if debug { debug_parse(text, moment, timezone) }
  Ok(moment)
}

proc format_moment(moment: Moment, format: Bytes, timezone: Str, calendar: Str, locale: Str) [time] -> Bytes? {
  return null when ! width_is_bounded(format)

  var parts: List[Bytes] = []
  var start = 0
  var at = 0
  let size = format.len()
  while at < size {
    if format.byte_at(at) != 37 { at += 1; continue }
    if start < at { parts += [format[start..at]] }
    let spec_start = at
    at += 1
    if at < size and format.byte_at(at) == 37 {
      parts += [b"%"]
      at += 1
      start = at
      continue
    }
    while at < size and (format.byte_at(at) ?? 0) in [45, 95, 48, 94, 35, 43] { at += 1 }
    while at < size and (format.byte_at(at) ?? 0) >= 48 and (format.byte_at(at) ?? 0) <= 57 { at += 1 }
    while at < size and format.byte_at(at) == 58 { at += 1 }
    if at < size and (format.byte_at(at) ?? 0) in [69, 79] { at += 1 }
    if at == size {
      parts += [format[spec_start..size]]
      start = size
      break
    }
    let specifier = format.byte_at(at) ?? 0
    at += 1
    if specifier >= 128 or (! (specifier >= 65 and specifier <= 90) and ! (specifier >= 97 and specifier <= 122) and specifier != 43) {
      parts += [format[spec_start..at]]
      start = at
      continue
    }
    let directive = format[spec_start..at].utf8() ?? ""
    match time.format(moment.seconds, moment.nanoseconds, directive, timezone, calendar, locale) {
      Ok(text) => parts += [bytes.from_text(text)],
      Err(_) => return null,
    }
    start = at
  }
  if start < size { parts += [format[start..size]] }
  bytes.concat(parts)
}

proc show_moment(moment: Moment, format: Bytes, timezone: Str, calendar: Str, locale: Str) [process, time, io] {
  if ! width_is_bounded(format) {
    gnu.error(oversized_width_error(format) ?? f"format modifier width exceeds {MAX_FORMAT_WIDTH}")
    exit 1
  }

  let rendered = format_moment(moment, format, timezone, calendar, locale)
  if let text = rendered {
    gnu.write_bytes(bytes.concat([text, b"\n"]))
    if let Err(failure) = io.flush_stdout() { gnu.write_failed(failure) }
  } else {
    gnu.error("invalid format or format width exceeds 65535")
    exit 1
  }
}

proc invalid_date(text: Str) [process, env] -> Unit {
  gnu.error(f"invalid date {gnu.quote_value(text)}")
}

proc choose_format(opts: DateOptions, argv: List[Str], raw_argv: List[Bytes]) [process, env] -> DateFormat {
  if opts.formats.len() > 0 {
    let operand = opts.formats[0]
    let operand_bytes = argument_bytes(argv, raw_argv, operand, [])
    if operand_bytes.byte_at(0) != 43 {
      if opts.date == ABSENT {
        if let Ok(text) = operand_bytes.utf8() { invalid_date(text) } else { gnu.error(f"invalid date {gnu.quote_value_bytes(operand_bytes)}") }
        exit 1
      }
      let text = operand_bytes.utf8() ?? escaped_invalid_line(operand_bytes)
      gnu.error(f"the argument {text} lacks a leading '+';\nwhen using an option to specify date(s), any non-option\nargument must be a format string beginning with '+'")
      exit 1
    }
    return {bytes: operand_bytes[1..operand_bytes.len()]}
  }

  if opts.iso != "" {
    let iso = if opts.iso == "auto" { "date" } else { shortcut(opts.iso, ["date", "hours", "minutes", "seconds", "ns"]) ?? "date" }
    let text = match iso {
      "hours" => "%Y-%m-%dT%H%:z",
      "minutes" => "%Y-%m-%dT%H:%M%:z",
      "seconds" => "%Y-%m-%dT%H:%M:%S%:z",
      "ns" => "%Y-%m-%dT%H:%M:%S,%N%:z",
      else => "%Y-%m-%d",
    }
    return {bytes: bytes.from_text(text)}
  }

  if opts.rfc_email {
    let text = "%a, %d %b %Y %H:%M:%S %z"
    return {bytes: bytes.from_text(text)}
  }

  if opts.rfc3339 != "" {
    let rfc = shortcut(opts.rfc3339, ["date", "seconds", "ns"]) ?? "date"
    let text = match rfc {
      "seconds" => "%Y-%m-%d %H:%M:%S%:z",
      "ns" => "%Y-%m-%d %H:%M:%S.%N%:z",
      else => "%Y-%m-%d",
    }
    return {bytes: bytes.from_text(text)}
  }

  if opts.resolution {
    let text = "%s.%N"
    return {bytes: bytes.from_text(text)}
  }

  let text = "%a %b %e %H:%M:%S %Z %Y"
  {bytes: bytes.from_text(text)}
}

proc main(...argv: List[Str]) [fs, process, env, error, io, time] {
  let raw_argv = cli.argv_bytes()
  if let unknown = unexpected_date_option(argv) { gnu.usage_error(f"unexpected argument '{unknown}'") }
  let opts: DateOptions = cli.applet(
    argv,
    {
      gnu: {prog: "date", status: 1},
      date: {form: "-d --date STRING", default: ABSENT, conflicts: ["file", "reference", "resolution"]},
      file: {form: "-f --file DATEFILE", default: ABSENT, conflicts: ["date", "reference", "resolution"]},
      reference: {form: "-r --reference FILE", default: ABSENT, conflicts: ["date", "file", "resolution"]},
      utc: {form: "-u --utc --universal --uct --uni", default: false},
      iso: {form: "-I --iso-8601 --i[=WHEN]", default: "", optional_default: "date"},
      rfc_email: {form: "-R --rfc-email --rfc-e --rfc-822 --rfc-2822", default: false},
      rfc3339: {form: "--rfc-3339 --rfc-3 WHEN", default: ""},
      resolution: {form: "--resolution", default: false},
      debug: {form: "--debug", default: false},
      set: {form: "-s --set STRING", default: ABSENT},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      formats: {form: "...FORMAT"},
    },
  )?

  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("date"); return }

  let resolution_given = has_long_option(argv, "--resolution")
  let date_given = opts.date != ABSENT
  let file_given = opts.file != ABSENT
  let reference_given = opts.reference != ABSENT
  if resolution_given and (date_given or file_given or reference_given) {
    gnu.usage_error("--resolution cannot be used with --date, --file, or --reference")
  }
  if opts.formats.len() > 1 {
    let extra = argument_bytes(argv, raw_argv, opts.formats[1], [])
    if let Err(_) = extra.utf8() { gnu.usage_error(f"extra operand {quote_octal_operand(extra)}") }
    gnu.extra_operand(opts.formats[1])
  }

  if opts.iso != "" and opts.iso != "auto" and shortcut(opts.iso, ["date", "hours", "minutes", "seconds", "ns"]) == null {
    gnu.usage_error(f"invalid date format {gnu.quote_value(opts.iso)} for --iso-8601")
  }
  if opts.rfc3339 != "" and shortcut(opts.rfc3339, ["date", "seconds", "ns"]) == null {
    gnu.usage_error(f"invalid date format {gnu.quote_value(opts.rfc3339)} for --rfc-3339")
  }

  let selected_format = choose_format(opts, argv, raw_argv)
  let timezone = if opts.utc { "UTC" } else { "local" }
  let calendar = if opts.iso != "" or opts.rfc_email or opts.rfc3339 != "" { "gregorian" } else { "locale" }
  let locale = if opts.rfc_email { "C" } else { "locale" }
  let now = time.wall_now()

  if opts.set != ABSENT {
    let raw_set = argument_bytes(argv, raw_argv, opts.set, ["--set", "-s"])
    let set_text = match raw_set.utf8() {
      Ok(text) => text,
      Err(_) => { gnu.error(f"invalid date {gnu.quote_value_bytes(raw_set)}"); exit 1 },
    }
    match parse_date(set_text, now, timezone, opts.debug) {
      Err(_) => { invalid_date(set_text); exit 1 },
      Ok(moment) => {
        show_moment(moment, selected_format.bytes, timezone, calendar, locale)
        let epoch_ms = moment.seconds * 1000 + moment.nanoseconds / 1000000
        match linux.set_system_clock(epoch_ms) {
          Ok(_) => return,
          Err(failure) => { gnu.error(f"cannot set date: {failure.message}"); exit 1 },
        }
      },
    }
  }

  if opts.reference != ABSENT {
    let raw_path = argument_bytes(argv, raw_argv, opts.reference, ["--reference", "-r"])
    let reference_path = Path.parse_bytes(raw_path)?
    guard let stat = fs.stat(reference_path) else { |failure|
      gnu.error(f"{gnu.quote_bytes(raw_path, always: false)}: {gnu.strerror(failure)}")
      exit 1
    }
    let stamp = stat.mtime_ns
    var seconds = stamp / 1000000000
    var nanoseconds = stamp % 1000000000
    if nanoseconds < 0 { seconds -= 1; nanoseconds += 1000000000 }
    show_moment({seconds: seconds, nanoseconds: nanoseconds}, selected_format.bytes, timezone, calendar, locale)
    return
  }

  if opts.file != ABSENT {
    let input = if opts.file == "-" {
      match io.stdin_bytes() {
        Ok(data) => data,
        Err(_) => { gnu.error("error reading '-'" ); exit 1 },
      }
    } else {
      let raw_path = argument_bytes(argv, raw_argv, opts.file, ["--file", "-f"])
      let input_path = Path.parse_bytes(raw_path)?
      if let Ok(meta) = input_path.metadata() {
        if meta.kind == "dir" { gnu.error(f"expected file, got directory {gnu.quote_bytes(raw_path)}"); exit 1 }
      }
      match input_path.read_bytes() {
        Ok(data) => data,
        Err(failure) => { gnu.error(f"{gnu.quote_bytes(raw_path, always: false)}: {gnu.strerror(failure)}"); exit 1 },
      }
    }
    var failed = false
    for raw_line in input_lines(input) {
      match raw_line.utf8() {
        Ok(line) => match parse_date(line, now, timezone, opts.debug) {
          Ok(moment) => show_moment(moment, selected_format.bytes, timezone, calendar, locale),
          Err(_) => { invalid_date(line); failed = true },
        },
        Err(_) => { gnu.error(f"invalid date '{escaped_invalid_line(raw_line)}'"); failed = true },
      }
    }
    if failed { exit 1 }
    return
  }

  if opts.date != ABSENT {
    let raw_date = argument_bytes(argv, raw_argv, opts.date, ["--date", "-d"])
    let date_text = match raw_date.utf8() {
      Ok(text) => text,
      Err(_) => { gnu.error(f"invalid date {gnu.quote_value_bytes(raw_date)}"); exit 1 },
    }
    match parse_date(date_text, now, timezone, opts.debug) {
      Ok(moment) => show_moment(moment, selected_format.bytes, timezone, calendar, locale),
      Err(_) => { invalid_date(date_text); exit 1 },
    }
    return
  }

  if opts.resolution {
    match time.clock_resolution() {
      Ok(moment) => show_moment(moment, selected_format.bytes, timezone, calendar, locale),
      Err(_) => { gnu.error("cannot determine system clock resolution"); exit 1 },
    }
    return
  }

  show_moment(now, selected_format.bytes, timezone, calendar, locale)
}
