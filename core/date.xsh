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

pure width_is_bounded(format: Str) -> Bool {
  var at = 0
  let size = format.byte_len()

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
  let day = format_moment(moment, "%F", timezone, "locale") ?? ""
  let clock = format_moment(moment, "%T", timezone, "locale") ?? ""
  gnu.error(f"parsed date part: (Y-M-D) {day}")
  gnu.error(f"parsed time part: (H:M:S) {clock}")
  let input_timezone = env.get_or("TZ", "local") ?? "local"
  gnu.error(f"input timezone: {input_timezone}")
  if debug_needs_midnight_warning(text) { gnu.error("warning: using midnight") }
}

proc parse_date(text: Str, now: Moment, timezone: Str, debug: Bool) [time, error, process, env] -> Result[Moment, Error] {
  let input = strip_comments(text)
  if input == "" or input == "-" {
    let day = time.format(now.seconds, now.nanoseconds, "%F", timezone, "gregorian")?
    let moment = time.parse(f"{day} 00:00:00", now.seconds, timezone)?
    if debug { debug_parse(text, moment, timezone) }
    return Ok(moment)
  }

  let moment = time.parse(normalize_au_zone(input), now.seconds, timezone)?
  if debug { debug_parse(text, moment, timezone) }
  Ok(moment)
}

proc format_moment(moment: Moment, format: Str, timezone: Str, calendar: Str) [time] -> Str? {
  return null when ! width_is_bounded(format)

  match time.format(moment.seconds, moment.nanoseconds, format, timezone, calendar) {
    Ok(text) => text,
    Err(_) => null,
  }
}

proc show_moment(moment: Moment, format: Str, timezone: Str, calendar: Str) [process, time] {
  if ! width_is_bounded(format) {
    gnu.error("format modifier width exceeds 65535")
    exit 1
  }

  let rendered = format_moment(moment, format, timezone, calendar)
  if let text = rendered {
    print $text
  } else {
    gnu.error("invalid format or format width exceeds 65535")
    exit 1
  }
}

proc invalid_date(text: Str) [process, env] -> Unit {
  gnu.error(f"invalid date {gnu.quote_value(text)}")
}

proc choose_format(opts: DateOptions) [process, env] -> Str {
  if opts.formats.len() > 0 {
    let operand = opts.formats[0]
    if ! operand.starts_with("+") {
      if opts.date == ABSENT {
        invalid_date(operand)
        exit 1
      }
      gnu.error(f"the argument {operand} lacks a leading '+';\nwhen using an option to specify date(s), any non-option\nargument must be a format string beginning with '+'")
      exit 1
    }
    return operand.byte_slice(1, length: operand.byte_len() - 1)
  }

  if opts.iso != "" {
    let iso = if opts.iso == "auto" { "date" } else { shortcut(opts.iso, ["date", "hours", "minutes", "seconds", "ns"]) ?? "date" }
    return match iso {
      "hours" => "%Y-%m-%dT%H%:z",
      "minutes" => "%Y-%m-%dT%H:%M%:z",
      "seconds" => "%Y-%m-%dT%H:%M:%S%:z",
      "ns" => "%Y-%m-%dT%H:%M:%S,%N%:z",
      else => "%Y-%m-%d",
    }
  }

  if opts.rfc_email { return "%a, %d %b %Y %H:%M:%S %z" }

  if opts.rfc3339 != "" {
    let rfc = shortcut(opts.rfc3339, ["date", "seconds", "ns"]) ?? "date"
    return match rfc {
      "seconds" => "%Y-%m-%d %H:%M:%S%:z",
      "ns" => "%Y-%m-%d %H:%M:%S.%N%:z",
      else => "%Y-%m-%d",
    }
  }

  if opts.resolution { return "%s.%N" }

  "%a %b %e %H:%M:%S %Z %Y"
}

proc main(...argv: List[Str]) [fs, process, env, error, io, time] {
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
  if opts.formats.len() > 1 { gnu.extra_operand(opts.formats[1]) }

  if opts.iso != "" and opts.iso != "auto" and shortcut(opts.iso, ["date", "hours", "minutes", "seconds", "ns"]) == null {
    gnu.usage_error(f"invalid date format {gnu.quote_value(opts.iso)} for --iso-8601")
  }
  if opts.rfc3339 != "" and shortcut(opts.rfc3339, ["date", "seconds", "ns"]) == null {
    gnu.usage_error(f"invalid date format {gnu.quote_value(opts.rfc3339)} for --rfc-3339")
  }

  let format = choose_format(opts)
  let timezone = if opts.utc { "UTC" } else { "local" }
  let calendar = if opts.iso != "" or opts.rfc_email or opts.rfc3339 != "" { "gregorian" } else { "locale" }
  let now = time.wall_now()

  if opts.set != ABSENT {
    match parse_date(opts.set, now, timezone, opts.debug) {
      Err(_) => { invalid_date(opts.set); exit 1 },
      Ok(moment) => {
        show_moment(moment, format, timezone, calendar)
        let epoch_ms = moment.seconds * 1000 + moment.nanoseconds / 1000000
        match linux.set_system_clock(epoch_ms) {
          Ok(_) => return,
          Err(failure) => { gnu.error(f"cannot set date: {failure.message}"); exit 1 },
        }
      },
    }
  }

  if opts.reference != ABSENT {
    let reference_path = fp"{opts.reference}"
    guard let stat = fs.stat(reference_path) else { |failure|
      gnu.name_error(opts.reference, failure)
      exit 1
    }
    let stamp = stat.mtime_ns
    var seconds = stamp / 1000000000
    var nanoseconds = stamp % 1000000000
    if nanoseconds < 0 { seconds -= 1; nanoseconds += 1000000000 }
    show_moment({seconds: seconds, nanoseconds: nanoseconds}, format, timezone, calendar)
    return
  }

  if opts.file != ABSENT {
    let input = if opts.file == "-" {
      match io.stdin_bytes() {
        Ok(data) => data,
        Err(_) => { gnu.error("error reading '-'" ); exit 1 },
      }
    } else {
      let input_path = fp"{opts.file}"
      if let Ok(meta) = input_path.metadata() {
        if meta.kind == "dir" { gnu.error(f"expected file, got directory {gnu.quote(opts.file)}"); exit 1 }
      }
      match input_path.read_bytes() {
        Ok(data) => data,
        Err(failure) => { gnu.name_error(opts.file, failure); exit 1 },
      }
    }
    var failed = false
    for raw_line in input_lines(input) {
      match raw_line.utf8() {
        Ok(line) => match parse_date(line, now, timezone, opts.debug) {
          Ok(moment) => show_moment(moment, format, timezone, calendar),
          Err(_) => { invalid_date(line); failed = true },
        },
        Err(_) => { gnu.error(f"invalid date '{escaped_invalid_line(raw_line)}'"); failed = true },
      }
    }
    if failed { exit 1 }
    return
  }

  if opts.date != ABSENT {
    match parse_date(opts.date, now, timezone, opts.debug) {
      Ok(moment) => show_moment(moment, format, timezone, calendar),
      Err(_) => { invalid_date(opts.date); exit 1 },
    }
    return
  }

  if opts.resolution {
    match time.clock_resolution() {
      Ok(moment) => show_moment(moment, format, timezone, calendar),
      Err(_) => { gnu.error("cannot determine system clock resolution"); exit 1 },
    }
    return
  }

  show_moment(now, format, timezone, calendar)
}
