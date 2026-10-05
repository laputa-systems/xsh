#!/bin/xsh
use lib.gnu

proc emit_date(text: Str, format: Str, utc: Bool) [time, process, env, io] -> Bool {
  match time.parse(text, utc:) {
    Ok(epoch) => {
      match time.format(epoch, format, utc:) {
        Ok(output) => { gnu.write_text(f"{output}\n"); return true }
        Err(failure) => gnu.error(failure.message)
      }
    }
    Err(failure) => gnu.error(f"invalid date {gnu.quote(text)}")
  }
  false
}

proc main(...argv: List[Str]) [time, process, env, io, fs, error] {
  var utc = false
  var date = "now"
  var file = ""
  var reference = ""
  var setting = false
  var format = "%a %b %e %H:%M:%S %Z %Y"
  var specified_format = false
  var sources = 0
  var index = 0
  var operands = false
  while index < argv.len() {
    let arg = argv[index]
    index += 1
    if ! operands and arg == "--" { operands = true; continue }
    if ! operands and arg == "--help" {
      gnu.help("Usage: date [OPTION]... [+FORMAT]\nDisplay a calendar date.\n  -d, --date=STRING       display STRING\n  -f, --file=FILE         display each date in FILE\n  -r, --reference=FILE    display FILE modification time\n  -u, --utc              use UTC\n  -R, --rfc-email        RFC email format\n  -I[TIMESPEC]           ISO 8601 format\n      --rfc-3339=SPEC     RFC 3339 format\n  -s, --set=STRING        set system clock\n      --resolution       display clock resolution")
      return
    }
    if ! operands and arg == "--version" { gnu.version("date"); return }
    if ! operands and (arg == "-u" or arg == "--utc" or arg == "--universal" or arg == "--uct" or arg == "--uni" or arg == "--u") { utc = true; continue }
    if ! operands and (arg == "-R" or arg == "--rfc-email" or arg == "--rfc-822" or arg == "--rfc-2822") { format = "%a, %d %b %Y %H:%M:%S %z"; continue }
    if ! operands and arg == "--resolution" { gnu.error("clock resolution reporting is not supported"); exit 1 }
    if ! operands and (arg.starts_with("-I") or arg.starts_with("--iso-8601") or arg == "--i" or arg.starts_with("--i=") or arg.starts_with("--rfc-3339")) {
      var spec = "date"
      var rfc = arg.starts_with("--rfc-3339")
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
      sources += 1
      if arg.starts_with("-f") or arg.starts_with("--file") { file = value } else if arg.starts_with("-r") or arg.starts_with("--reference") { reference = value } else { date = value; setting = arg.starts_with("-s") or arg.starts_with("--set") }
      continue
    }
    if arg.starts_with("+") {
      if specified_format { gnu.extra_operand(arg) }
      format = arg.byte_slice(1); specified_format = true
    } else if ! operands and arg.starts_with("-") { gnu.usage_error(f"unrecognized option {gnu.quote(arg)}") } else { gnu.usage_error(f"invalid date {gnu.quote(arg)}") }
  }
  if sources > 1 { gnu.usage_error("the options to specify dates for printing are mutually exclusive") }
  if reference != "" {
    match fs.stat(fp"{reference}", follow_symlinks: true) {
      Ok(meta) => { gnu.write_text(f"{time.format(meta.mtime_ns, format, utc:)?}\n"); return }
      Err(failure) => { gnu.name_error(reference, failure); exit 1 }
    }
  }
  if file != "" {
    var contents = ""
    match gnu.read_operand(file) {
      Ok(data) => {
        match data.utf8() {
          Ok(text) => contents = text
          Err(failure) => { gnu.error("date input is not UTF-8"); exit 1 }
        }
      }
      Err(failure) => { gnu.name_error(file, failure); exit 1 }
    }
    var success = true
    for line in contents.lines() { if ! emit_date(line, format, utc) { success = false } }
    if ! success { exit 1 }
    return
  }
  if setting {
    match time.parse(date, utc:) {
      Ok(epoch) => {
        if let Err(failure) = linux.set_system_clock(epoch / 1000000) {
          gnu.error(f"cannot set date: {gnu.strerror(failure)}")
          let _ = emit_date(date, format, utc)
          exit 1
        }
      }
      Err(failure) => { gnu.error(f"invalid date {gnu.quote(date)}"); exit 1 }
    }
  }
  if ! emit_date(date, format, utc) { exit 1 }
}
