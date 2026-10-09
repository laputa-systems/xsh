#!/bin/xsh
use lib.gnu as gnu

const ABSENT = "__xsh_touch_option_absent__"
error TouchError = InvalidTimestamp(message: Str) : Usage
const USAGE = """Usage: touch [OPTION]... FILE...
Update the access and modification times of each FILE to the current time.

  -a                    change only the access time
  -c, --no-create       do not create any files
  -d, --date=STRING     parse STRING and use it instead of current time
  -f                    (ignored)
  -h, --no-dereference  affect each symbolic link instead of any referenced file
  -m                    change only the modification time
  -r, --reference=FILE  use this file's times instead of current time
  -t STAMP              use [[CC]YY]MMDDhhmm[.ss] instead of current time
      --time=WORD       change the specified time: access, atime, use, modify, mtime
      --help            display this help and exit
      --version         output version information and exit
"""

type TouchOptions = {
  atime: Bool,
  date: Str,
  force: Bool,
  modification: Bool,
  no_create: Bool,
  no_dereference: Bool,
  reference: Str,
  time_word: Str,
  timestamp: Str,
  help: Bool,
  version: Bool,
  paths: List[Str],
}

type Moment = {seconds: Int, nanoseconds: Int}
type TouchStat = {
  atime_seconds: Int,
  atime_nanoseconds: Int,
  mtime_seconds: Int,
  mtime_nanoseconds: Int,
}

pure touch_short_value_start(arg: Str) -> Int? {
  return null when ! arg.starts_with("-") or arg.starts_with("--")
  var at = 1
  while at < arg.byte_len() {
    let option = arg.byte_slice(at, length: 1)
    if option in ["d", "r", "t"] { return at + 1 }
    at += 1
  }
  null
}

pure touch_long_value(arg: Str) -> Bool {
  let equal_at = arg.find("=")
  let name = if equal_at == null { arg } else { arg.byte_slice(0, length: equal_at ?? 0) }
  for option in ["--date", "--reference", "--time"] {
    if option.starts_with(name) { return true }
  }
  false
}

pure touch_long_is(arg: Str, expected: Str) -> Bool {
  let equal_at = arg.find("=")
  let name = if equal_at == null { arg } else { arg.byte_slice(0, length: equal_at ?? 0) }
  expected.starts_with(name)
}

pure time_option_given(argv: List[Str], posixly_correct: Bool) -> Bool {
  var options = true
  for arg in argv {
    if options and arg == "--" { options = false; continue }
    if ! options { continue }
    if posixly_correct and (! arg.starts_with("-") or arg == "-") { options = false; continue }
    if arg.starts_with("--") and touch_long_is(arg, "--time") { return true }
  }
  false
}

pure raw_touch_paths(argv: List[Str], raw: List[Bytes], posixly_correct: Bool) -> List[Bytes] {
  var paths: List[Bytes] = []
  var index = 0
  var options = true
  while index < argv.len() {
    let arg = argv[index]
    if options and arg == "--" { options = false; index += 1; continue }
    if options and arg.starts_with("--") and touch_long_value(arg) {
      index += if arg.find("=") == null { 2 } else { 1 }
      continue
    }
    let value_start = touch_short_value_start(arg)
    if options and value_start != null {
      index += if (value_start ?? 0) >= arg.byte_len() { 2 } else { 1 }
      continue
    }
    if options and arg.starts_with("-") and arg != "-" { index += 1; continue }
    paths += [raw[index]]
    if posixly_correct { options = false }
    index += 1
  }
  paths
}

pure raw_touch_reference(argv: List[Str], raw: List[Bytes], fallback: Bytes, posixly_correct: Bool) -> Bytes {
  var selected: Bytes? = null
  var index = 0
  var options = true
  while index < argv.len() {
    let arg = argv[index]
    if options and arg == "--" { options = false; index += 1; continue }
    if options and arg.starts_with("--") and touch_long_value(arg) {
      if touch_long_is(arg, "--reference") {
        let equal_at = arg.find("=")
        selected = if equal_at == null { raw[index + 1] } else { raw[index].slice((equal_at ?? 0) + 1) }
      }
      index += if arg.find("=") == null { 2 } else { 1 }
      continue
    }
    let value_start = touch_short_value_start(arg)
    if options and value_start != null {
      let option = arg.byte_slice((value_start ?? 1) - 1, length: 1)
      if option == "r" {
        selected = if (value_start ?? 0) >= arg.byte_len() { raw[index + 1] } else { raw[index].slice(value_start ?? 0) }
      }
      index += if (value_start ?? 0) >= arg.byte_len() { 2 } else { 1 }
      continue
    }
    if options and posixly_correct and (! arg.starts_with("-") or arg == "-") { break }
    index += 1
  }
  selected ?? fallback
}

pure long_time_value(word: Str) -> Str? {
  let access = ["access", "atime", "use"]
  let modify = ["modify", "mtime"]
  return "access" when word in access
  return "modify" when word in modify
  var access_match = false
  var modify_match = false
  for choice in access { access_match = access_match or choice.starts_with(word) }
  for choice in modify { modify_match = modify_match or choice.starts_with(word) }
  return "access" when access_match and ! modify_match
  return "modify" when modify_match and ! access_match
  null
}

proc touch_timestamp(raw: Str, now: Moment, legacy_posix: Bool) [time, error] -> Result[Moment] {
  let size = raw.byte_len()
  var date = ""
  var second = "00"
  let current_year = time.format(now.seconds, now.nanoseconds, "%Y", "local", "gregorian", "C")?
  var leap_second = false

  if size == 8 or size == 10 or size == 11 or size == 12 or size == 13 or size == 15 {
    let has_seconds = size == 11 or size == 13 or size == 15
    let core_len = if has_seconds { size - 3 } else { size }
    if has_seconds {
      return Err(TouchError.InvalidTimestamp("invalid timestamp")) when raw.byte_slice(core_len, length: 1) != "."
      second = raw.byte_slice(core_len + 1)
      if second == "60" { second = "59"; leap_second = true }
    }

    var digits = ""
    for ch in raw.byte_slice(0, length: core_len) {
      let value = ch.parse_int() ?? -1
      if value < 0 or value > 9 { return Err(TouchError.InvalidTimestamp("invalid timestamp")) }
      digits = f"{digits}{ch}"
    }

    if core_len == 8 {
      date = f"{current_year}-{digits.byte_slice(0, length: 2)}-{digits.byte_slice(2, length: 2)} {digits.byte_slice(4, length: 2)}:{digits.byte_slice(6, length: 2)}:{second}"
    } else if core_len == 10 and legacy_posix {
      let yy = digits.byte_slice(8, length: 2).parse_int() ?? 0
      let year = if yy <= 68 { f"20{digits.byte_slice(8, length: 2)}" } else { f"19{digits.byte_slice(8, length: 2)}" }
      date = f"{year}-{digits.byte_slice(0, length: 2)}-{digits.byte_slice(2, length: 2)} {digits.byte_slice(4, length: 2)}:{digits.byte_slice(6, length: 2)}:{second}"
    } else if core_len == 10 or core_len == 13 {
      let yy = digits.byte_slice(0, length: 2).parse_int() ?? 0
      let year = if yy <= 68 { f"20{digits.byte_slice(0, length: 2)}" } else { f"19{digits.byte_slice(0, length: 2)}" }
      date = f"{year}-{digits.byte_slice(2, length: 2)}-{digits.byte_slice(4, length: 2)} {digits.byte_slice(6, length: 2)}:{digits.byte_slice(8, length: 2)}:{second}"
    } else {
      date = f"{digits.byte_slice(0, length: 4)}-{digits.byte_slice(4, length: 2)}-{digits.byte_slice(6, length: 2)} {digits.byte_slice(8, length: 2)}:{digits.byte_slice(10, length: 2)}:{second}"
    }
  } else {
    return Err(TouchError.InvalidTimestamp("invalid timestamp"))
  }

  match time.parse(date, now.seconds, "local") {
    Err(failure) => Err(failure)
    Ok(moment) => {
      let rendered = time.format(moment.seconds, moment.nanoseconds, "%Y-%m-%d %H:%M:%S", "local", "gregorian", "C")?
      return Err(TouchError.InvalidTimestamp("invalid timestamp")) when rendered != date

      if leap_second {
        return Err(TouchError.InvalidTimestamp("invalid timestamp")) when moment.seconds == 9223372036854775807
        Ok({seconds: moment.seconds + 1, nanoseconds: moment.nanoseconds})
      } else {
        Ok(moment)
      }
    }
  }
}

proc parse_moment(raw: Str, base_seconds: Int, now: Moment, timestamp: Bool) [time, error] -> Result[Moment] {
  if timestamp { return touch_timestamp(raw, now, false) }
  time.parse(raw, base_seconds, "local")
}

proc main(...argv: List[Str]) [fs, process, env, io, time, error] {
  let opts: TouchOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      atime: {form: "-a", default: false},
      no_create: {form: "-c --no-create", default: false},
      date: {form: "-d --date STRING", default: ABSENT},
      force: {form: "-f", default: false},
      modification: {form: "-m", default: false},
      no_dereference: {form: "-h --no-dereference", default: false},
      reference: {form: "-r --reference FILE", default: ABSENT},
      time_word: {form: "--time WORD", default: ""},
      timestamp: {form: "-t STAMP", default: ABSENT},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      paths: {form: "...FILE"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }
  if opts.version {
    gnu.version("touch")
    return
  }
  if opts.paths.len() == 0 {
    gnu.usage_error("missing file operand")
  }

  let posixly_correct = (env.get_or("POSIXLY_CORRECT", "") ?? "") != ""
  if time_option_given(argv, posixly_correct) and long_time_value(opts.time_word) == null {
    gnu.usage_error(f"invalid argument {gnu.quote_value(opts.time_word)} for '--time'")
  }

  # GNU retains -f as a documented obsolete flag and gives it no effect.
  let _legacy_force = opts.force

  let now_raw = time.wall_now()
  let now: Moment = {seconds: now_raw.seconds, nanoseconds: now_raw.nanoseconds}
  let raw_argv = cli.argv_bytes()
  var raw_paths = raw_touch_paths(argv, raw_argv, posixly_correct)
  let raw_reference = raw_touch_reference(argv, raw_argv, bytes.from_text(opts.reference), posixly_correct)
  let source_path = Path.parse_bytes(raw_reference)?
  let reference_given = opts.reference != ABSENT
  var timestamp_given = opts.timestamp != ABSENT
  let date_given = opts.date != ABSENT
  if timestamp_given and (date_given or reference_given) {
    gnu.usage_error("cannot specify times from more than one source")
  }
  let reference_stat: TouchStat? = if reference_given {
    match fs.stat(source_path, follow_symlinks: ! opts.no_dereference) {
      Ok(stat) => stat
      Err(failure) => {
        gnu.error(f"failed to get attributes of {gnu.quote_bytes(raw_reference)}: {gnu.strerror(failure)}")
        exit 1
      }
    }
  } else { null }

  var base_seconds = now.seconds
  var atime_value = now
  var mtime_value = now
  if let stat = reference_stat {
    atime_value = {seconds: stat.atime_seconds, nanoseconds: stat.atime_nanoseconds}
    mtime_value = {seconds: stat.mtime_seconds, nanoseconds: stat.mtime_nanoseconds}
    base_seconds = mtime_value.seconds
  }

  var paths = opts.paths
  var timestamp = opts.timestamp
  var legacy_timestamp = false
  if ! timestamp_given and ! date_given and ! reference_given and paths.len() > 1 and (env.get_or("_POSIX2_VERSION", "") ?? "") == "199209" {
    let first = paths[0]
    if first.byte_len() in [8, 10] {
      timestamp = first
      timestamp_given = true
      legacy_timestamp = first.byte_len() == 10
      paths = paths |> drop(1)
      raw_paths = raw_paths |> drop(1)
    }
  }

  if timestamp_given {
    let parsed = touch_timestamp(timestamp, now, legacy_timestamp)
    match parsed {
      Ok(moment) => { atime_value = moment; mtime_value = moment }
      Err(_) => {
        gnu.error(f"invalid date format {gnu.quote_value(timestamp)}")
        exit 1
      }
    }
  } else if date_given {
    match parse_moment(opts.date, base_seconds, now, false) {
      Ok(moment) => { atime_value = moment; mtime_value = moment }
      Err(_) => {
        gnu.error(f"invalid date format {gnu.quote_value(opts.date)}")
        exit 1
      }
    }
  }

  let time_word = if opts.time_word == "" { "" } else { long_time_value(opts.time_word) ?? "" }
  var set_atime = opts.atime or time_word == "access"
  var set_mtime = opts.modification or time_word == "modify"
  if set_atime == set_mtime {
    set_atime = true
    set_mtime = true
  }

  var failed = false
  for index in range(paths.len()) {
    let item = paths[index]
    let raw_item = raw_paths[index]
    let shown = gnu.quote_bytes(raw_item)
    let use_current_time = ! (date_given or timestamp_given or reference_given)
    if raw_item == bytes.from_text("-") {
      let atime_now = set_atime and use_current_time
      let mtime_now = set_mtime and use_current_time
      let atime_seconds: Int? = if set_atime and ! atime_now { atime_value.seconds } else { null }
      let atime_nanoseconds: Int? = if set_atime and ! atime_now { atime_value.nanoseconds } else { null }
      let mtime_seconds: Int? = if set_mtime and ! mtime_now { mtime_value.seconds } else { null }
      let mtime_nanoseconds: Int? = if set_mtime and ! mtime_now { mtime_value.nanoseconds } else { null }
      match fs.set_times_fd(
        1,
        null,
        null,
        atime_now: atime_now,
        mtime_now: mtime_now,
        atime_seconds: atime_seconds,
        atime_nanoseconds: atime_nanoseconds,
        mtime_seconds: mtime_seconds,
        mtime_nanoseconds: mtime_nanoseconds,
      ) {
        Ok(_) => {}
        Err(failure) => {
          gnu.error(f"setting times of {shown}: {gnu.strerror(failure)}")
          failed = true
        }
      }
      continue
    }

    let target = Path.parse_bytes(raw_item)?
    let follow_symlinks = ! opts.no_dereference
    let current = fs.stat(target, follow_symlinks: follow_symlinks)
    if let Err(failure) = current {
      if gnu.errno(failure) != 2 {
        gnu.error(f"setting times of {shown}: {gnu.strerror(failure)}")
        failed = true
        continue
      }
      if opts.no_create { continue }
      if item.ends_with("/") {
        gnu.error(f"cannot touch {shown}: No such file or directory")
        failed = true
        continue
      }
      if opts.no_dereference {
        gnu.error(f"setting times of {shown}: {gnu.strerror(failure)}")
        failed = true
        continue
      }
      match target.touch(create: true) {
        Ok(_) => {}
        Err(create_failure) => {
          gnu.error(f"cannot touch {shown}: {gnu.strerror(create_failure)}")
          failed = true
          continue
        }
      }
    }
    let atime_now = set_atime and use_current_time
    let mtime_now = set_mtime and use_current_time
    let atime_seconds: Int? = if set_atime and ! atime_now { atime_value.seconds } else { null }
    let atime_nanoseconds: Int? = if set_atime and ! atime_now { atime_value.nanoseconds } else { null }
    let mtime_seconds: Int? = if set_mtime and ! mtime_now { mtime_value.seconds } else { null }
    let mtime_nanoseconds: Int? = if set_mtime and ! mtime_now { mtime_value.nanoseconds } else { null }
    match fs.set_times(
      target,
      null,
      null,
      atime_now: atime_now,
      mtime_now: mtime_now,
      atime_seconds: atime_seconds,
      atime_nanoseconds: atime_nanoseconds,
      mtime_seconds: mtime_seconds,
      mtime_nanoseconds: mtime_nanoseconds,
      follow_symlinks: follow_symlinks,
    ) {
      Ok(_) => {}
      Err(failure) => {
        gnu.error(f"setting times of {shown}: {gnu.strerror(failure)}")
        failed = true
      }
    }
  }

  exit 1 when failed
}
