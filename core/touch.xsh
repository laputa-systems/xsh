#!/bin/xsh
use lib.gnu
use lib.date_parse

type TouchOptions = {
  obsolete_force: Bool, no_create: Bool, reference: Str?, access: Bool, modify: Bool,
  no_dereference: Bool, date: Str?, timestamp: Str?, time: Str?, help: Bool, version: Bool, paths: List[Str],
}
type RawArgument = {marker: Str, value: Bytes}
type PreparedArguments = {text: List[Str], raw: List[RawArgument]}

pure prepare_arguments(argv: List[Bytes]) -> PreparedArguments {
  var text: List[Str] = []
  var raw: List[RawArgument] = []
  for index in range(argv.len()) {
    let argument = argv[index]
    match argument.utf8() {
      Ok(value) => text += [value]
      Err(_) => {
        let marker = f"\0touch-raw-argument-{index}\0"
        text += [marker]
        raw += [{marker: marker, value: argument}]
      }
    }
  }
  {text: text, raw: raw}
}

pure argument_bytes(value: Str, raw: List[RawArgument]) -> Bytes {
  for argument in raw { if argument.marker == value { return argument.value } }
  bytes.from_text(value)
}

# date_parse recognizes a signed displacement as a separate token, while GNU
# touch accepts the sign attached to its number.
pure normalize_date_displacement(text: Str) -> Str {
  let signed = rx"^([+-])([0-9]+) (.+)$".captures(text)
  if signed.is_empty() { text } else { f"{signed[1]} {signed[2]} {signed[3]}" }
}

# Sets the selected timestamps: an explicit instant, or the kernel's current
# time when none was given; the unselected one is left unchanged.
proc set_times(target: Path, atime: date_parse.Instant?, mtime: date_parse.Instant?, access: Bool, modify: Bool, follow_symlinks: Bool) [fs] -> Result[Unit, Error] {
  var atime_sec: Int? = null
  var atime_nsec: Int? = null
  var mtime_sec: Int? = null
  var mtime_nsec: Int? = null
  if access {
    if let instant = atime { atime_sec = instant.seconds
      atime_nsec = instant.nanoseconds }
  }
  if modify {
    if let instant = mtime { mtime_sec = instant.seconds
      mtime_nsec = instant.nanoseconds }
  }
  fs.set_times(target, atime_sec:, atime_nsec:, mtime_sec:, mtime_nsec:,
    atime_now: access and atime == null,
    mtime_now: modify and mtime == null,
    follow_symlinks:)
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io, time] {
  let prepared = prepare_arguments(argv)
  let opts: TouchOptions = cli.applet(prepared.text, {
    gnu: {status: 1},
    # GNU retains the obsolete -f spelling for compatibility; it requests
    # no operation and does not alter creation or error handling.
    obsolete_force: {form: "-f", default: false},
    no_create: {form: "-c --no-create", default: false},
    reference: {form: "-r --reference FILE"},
    access: {form: "-a", default: false},
    modify: {form: "-m", default: false},
    no_dereference: {form: "-h --no-dereference", default: false},
    date: {form: "-d --date STRING"},
    timestamp: {form: "-t STAMP"},
    time: {form: "--time WORD"},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...FILE"},
  })?
  if opts.help {
    gnu.help("Usage: touch [OPTION]... FILE...\nUpdate access and modification times.\n  -a  change only access time\n  -m  change only modification time\n  -c, --no-create  do not create files\n  -h, --no-dereference  affect symbolic links\n  -r, --reference=FILE  copy reference times\n  -d, --date=STRING  use the specified date\n  -t STAMP  use [[CC]YY]MMDDhhmm[.ss]\n      --time=WORD  access, atime, use, modify, or mtime\n")
    return
  }
  if opts.version {
    gnu.version("touch")
    return
  }
  if opts.paths.is_empty() { gnu.usage_error("missing file operand") }
  var access = opts.access
  var modify = opts.modify
  if let word = opts.time {
    if word != "" and ("access".starts_with(word) or "atime".starts_with(word) or "use".starts_with(word)) {
      access = true
    } else if word != "" and ("modify".starts_with(word) or "mtime".starts_with(word)) {
      modify = true
    } else {
      gnu.usage_error(f"invalid argument {gnu.quote_value(word)} for 'time'")
    }
  }
  if ! access and ! modify { access = true
    modify = true }
  # Instants keep seconds apart from nanoseconds: dates such as year 0 are
  # outside the signed nanosecond range but inside what the kernel accepts.
  var atime: date_parse.Instant? = null
  var mtime: date_parse.Instant? = null
  if let reference = opts.reference {
    let reference_bytes = argument_bytes(reference, prepared.raw)
    let reference_path = Path.parse_bytes(reference_bytes)?
    match fs.stat(reference_path, follow_symlinks: ! opts.no_dereference) {
      Ok(meta) => { atime = date_parse.instant_from_ns(meta.atime_ns)
        mtime = date_parse.instant_from_ns(meta.mtime_ns) }
      Err(failure) => { gnu.error(f"failed to get attributes of {gnu.quote_bytes(reference_bytes)}: {gnu.strerror(failure)}")
        exit 1 }
    }
  }
  if opts.date != null and opts.timestamp != null {
    gnu.usage_error("cannot specify times from more than one source")
  }
  if opts.timestamp != null and opts.reference != null {
    gnu.usage_error("cannot specify times from more than one source")
  }
  var paths = opts.paths
  var stamp: Str? = opts.date
  var leap_second = false
  if let compact = opts.timestamp {
    stamp = compact
    if ! rx"^[0-9]{8}([0-9]{2})?([0-9]{2})?(\.[0-9]{2})?$".matches(compact) {
      gnu.error(f"invalid date format {gnu.quote_value(compact)}")
      exit 1
    }
    # GNU compact timestamps permit second 60 and normalize it to the next
    # minute; calendar conversion validates ordinary seconds strictly.
    if compact.ends_with(".60") {
      stamp = compact.byte_slice(0, length: compact.byte_len() - 2) + "59"
      leap_second = true
    }
  }
  let posix_version = (env.get_or("_POSIX2_VERSION", "200809") ?? "200809").parse_int() ?? 200809
  if stamp == null and opts.reference == null and posix_version <= 199209 and paths.len() > 1 {
    let legacy = paths[0]
    if rx"^[0-9]{8}([0-9]{2})?$".matches(legacy) {
      # The obsolete positional form puts its optional year after minutes;
      # the explicit timestamp form puts its year before the month.
      stamp = if legacy.byte_len() == 10 { legacy.byte_slice(8) + legacy.byte_slice(0, length: 8) } else { legacy }
      paths = paths[1..]
    }
  }
  if let text = stamp {
    let base = date_parse.parse_instant("now")?
    let date_input = normalize_date_displacement(text)
    let access_time = date_parse.parse_instant(date_input, base: atime ?? base)
    let modify_time = date_parse.parse_instant(date_input, base: mtime ?? base)
    match access_time {
      Ok(value) => atime = {seconds: value.seconds + (if leap_second { 1 } else { 0 }), nanoseconds: value.nanoseconds}
      Err(_) => { gnu.error(f"invalid date format {gnu.quote_value(text)}")
        exit 1 }
    }
    match modify_time {
      Ok(value) => mtime = {seconds: value.seconds + (if leap_second { 1 } else { 0 }), nanoseconds: value.nanoseconds}
      Err(_) => { gnu.error(f"invalid date format {gnu.quote_value(text)}")
        exit 1 }
    }
  }
  var failed = false
  let path_names: List[Bytes] = collect { for path_value in paths { yield argument_bytes(path_value, prepared.raw) } }
  for name in path_names {
    let target = if name == b"-" { p"/dev/stdout" } else { Path.parse_bytes(name)? }
    # Try timestamps before opening: directories and unwritable owned files
    # can be touched without obtaining a writable descriptor.
    var creating = false
    var changed = set_times(target, atime, mtime, access, modify, name == b"-" or ! opts.no_dereference)
    if let Err(failure) = changed {
      if gnu.errno(failure) == 2 {
        continue when opts.no_create
        if ! opts.no_dereference and name != b"-" {
          creating = true
          if name.len() > 0 and name.byte_at(name.len() - 1) == 47 {
            gnu.error(f"cannot touch {gnu.quote_bytes(name)}: No such file or directory")
            failed = true
            continue
          }
          match target.touch() {
          Ok(_) => changed = set_times(target, atime, mtime, access, modify, true)
          Err(create_failure) => changed = Err(create_failure)
          }
        }
      }
    }
    if let Err(failure) = changed {
      if creating { gnu.error(f"cannot touch {gnu.quote_bytes(name)}: {gnu.strerror(failure)}") } else { gnu.error(f"setting times of {gnu.quote_bytes(name)}: {gnu.strerror(failure)}") }
      failed = true
    }
  }
  if failed { exit 1 }
}
