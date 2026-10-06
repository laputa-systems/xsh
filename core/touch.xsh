#!/bin/xsh
use lib.gnu
use lib.date_parse

type TouchOptions = {
  obsolete_force: Bool, no_create: Bool, reference: Str?, access: Bool, modify: Bool,
  no_dereference: Bool, date: Str?, timestamp: Str?, time: Str?, help: Bool, version: Bool, paths: List[Str],
}

proc main(...argv: List[Str]) [fs, process, env, error, io, time] {
  let opts: TouchOptions = cli.applet(argv, {
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
  var atime: Int? = null
  var mtime: Int? = null
  if let reference = opts.reference {
    match fs.stat(fp"{reference}", follow_symlinks: ! opts.no_dereference) {
      Ok(meta) => { atime = meta.atime_ns
        mtime = meta.mtime_ns }
      Err(failure) => { gnu.error(f"failed to get attributes of {gnu.quote(reference)}: {gnu.strerror(failure)}")
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
    let base = date_parse.parse("now")?
    let access_time = date_parse.parse(text, base_ns: atime ?? base)
    let modify_time = date_parse.parse(text, base_ns: mtime ?? base)
    match access_time {
      Ok(value) => atime = value + (if leap_second { 1000000000 } else { 0 })
      Err(_) => { gnu.error(f"invalid date format {gnu.quote_value(text)}")
        exit 1 }
    }
    match modify_time {
      Ok(value) => mtime = value + (if leap_second { 1000000000 } else { 0 })
      Err(_) => { gnu.error(f"invalid date format {gnu.quote_value(text)}")
        exit 1 }
    }
  }
  var failed = false
  for name in paths {
    let target = if name == "-" { p"/dev/stdout" } else { fp"{name}" }
    # Try timestamps before opening: directories and unwritable owned files
    # can be touched without obtaining a writable descriptor.
    var creating = false
    var changed = fs.set_times(target,
      atime_ns: if access { atime } else { null },
      mtime_ns: if modify { mtime } else { null },
      atime_now: access and atime == null,
      mtime_now: modify and mtime == null,
      follow_symlinks: name == "-" or ! opts.no_dereference)
    if let Err(failure) = changed {
      if gnu.errno(failure) == 2 {
        continue when opts.no_create
        if ! opts.no_dereference and name != "-" {
          creating = true
          if name.ends_with("/") {
            gnu.error(f"cannot touch {gnu.quote(name)}: No such file or directory")
            failed = true
            continue
          }
          match target.touch() {
          Ok(_) => changed = fs.set_times(target,
            atime_ns: if access { atime } else { null },
            mtime_ns: if modify { mtime } else { null },
            atime_now: access and atime == null,
            mtime_now: modify and mtime == null,
            follow_symlinks: true)
          Err(create_failure) => changed = Err(create_failure)
          }
        }
      }
    }
    if let Err(failure) = changed {
      if creating { gnu.cannot("touch", name, failure) } else { gnu.error(f"setting times of {gnu.quote(name)}: {gnu.strerror(failure)}") }
      failed = true
    }
  }
  if failed { exit 1 }
}
