#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

const USAGE = """Usage: tail [OPTION]... [FILE]...
Print the last 10 lines of each FILE to standard output.
With more than one FILE, precede each with a header giving the file name.

With no FILE, or when FILE is -, read standard input.

  -c, --bytes=[+]NUM       output the last NUM bytes; or use -c +NUM to
                             output starting with byte NUM of each file
  -f, --follow[={name|descriptor}]
                           output appended data as the file grows;
                             an absent option argument means 'descriptor'
  -F                       same as --follow=name --retry
  -n, --lines=[+]NUM       output the last NUM lines, instead of the last 10;
                             or use -n +NUM to skip NUM-1 lines at the start
      --max-unchanged-stats=N
                           with --follow=name, reopen a FILE which has not
                             changed size after N (default 5) iterations
                             to see if it has been unlinked or renamed
      --pid=PID            with -f, terminate after process ID, PID dies
  -q, --quiet, --silent    never output headers giving file names
      --retry              keep trying to open a file if it is inaccessible
  -s, --sleep-interval=N   with -f, sleep for approximately N seconds
                             (default 1.0) between iterations
  -v, --verbose            always output headers giving file names
  -z, --zero-terminated    line delimiter is NUL, not newline
      --help        display this help and exit
      --version     output version information and exit

NUM may have a multiplier suffix:
b 512, kB 1000, K 1024, MB 1000*1000, M 1024*1024,
GB 1000*1000*1000, G 1024*1024*1024, and so on for T, P, E, Z, Y, R, Q.
Binary prefixes can be used, too: KiB=K, MiB=M, and so on.
"""

type TailOptions = {
  lines: Str,
  bytes: Str,
  follow: Str,
  big_f: Bool,
  retry: Bool,
  max_unchanged: Str,
  pid: Str,
  sleep: Str,
  quiet: Bool,
  verbose: Bool,
  zero: Bool,
  help: Bool,
  version: Bool,
  files: List[Str],
}

# `from_start` is `+NUM`: output starting at unit NUM instead of the last NUM.
type Spec = {value: Int, from_start: Bool, bytes: Bool}

# Where output begins: `data` holds the whole input for sources that are read
# at once, and `start` is an offset into it (or into the file when chunked).
type Plan = {data: Bytes, start: Int}

const FOLLOW_MODES = ["descriptor", "name"]

# Rewrite the obsolete first argument `[+-]N[bcl][f]` into the options it
# stands for.
pure modernize(argv: List[Str]) -> List[Str] {
  guard ! argv.is_empty() else {
    return argv
  }

  let parts = rx"^([+-])([0-9]*)([bcl]?)(f?)$".captures(argv[0])

  return argv when parts.is_empty() or (parts[2] == "" and (parts[1] == "-" or parts[3] == ""))

  let sign = if parts[1] == "+" { "+" } else { "" }
  let digits = if parts[2] == "" { "10" } else { parts[2] }
  let unit = parts[3]
  let option = if unit == "b" or unit == "c" { "-c" } else { "-n" }
  let count = if unit == "b" { f"{sign}{digits}b" } else { f"{sign}{digits}" }

  [option, count, @if parts[4] == "f" { ["-f"] } else { [] }, @argv[1..]]
}

proc parse_spec(text: Str, by_bytes: Bool) [process, env] -> Spec {
  let from_start = text.starts_with("+")
  let digits = if from_start or text.starts_with("-") { text[1..] } else { text }
  let value = tio.parse_count(digits)

  if value == null {
    gnu.error(
      f"invalid number of {if by_bytes { "bytes" } else { "lines" }}: {gnu.quote_value(if from_start { text } else { digits })}",
    )
    exit 1
  }

  {value: value, from_start: from_start, bytes: by_bytes}
}

# GNU `argmatch` for an option argument: an exact name wins, otherwise a unique
# prefix.
proc match_choice(text: Str, choices: List[Str], option: Str) [process, env] -> Str {
  return text when text in choices

  let found = [choice for choice in choices if choice.starts_with(text)]

  return found[0] when found.len() == 1

  let kind = if found.is_empty() { "invalid" } else { "ambiguous" }
  gnu.error(f"{kind} argument {gnu.quote_value(text)} for '--{option}'")
  eprint "Valid arguments are:"

  for choice in choices {
    eprint f"  - '{choice}'"
  }

  gnu.try_help()
  exit 1
}

# The offset where the last `count` lines of a chunked file start, scanning
# backward from the end. A final separator ends the last line, so it is not
# counted.
proc last_lines_start(source: tio.Source, count: Int, zero: Bool) [fs, error, io] -> Result[Int] {
  let last = bytes.read_at(source.path, source.size - 1, 1)?
  var high = if last.byte_at(0) == (if zero { 0 } else { 10 }) { source.size - 1 } else { source.size }
  var need = count

  while high > 0 {
    let low = if high > tio.CHUNK { high - tio.CHUNK } else { 0 }
    let ends = tio.line_ends(bytes.read_at(source.path, low, high - low)?, zero)

    return Ok(low + ends[ends.len() - need]) when ends.len() >= need

    need -= ends.len()
    high = low
  }

  Ok(0)
}

# The offset in `data` where the output of `spec` begins.
pure data_start(data: Bytes, spec: Spec, zero: Bool) -> Int {
  let size = data.len()

  if spec.bytes {
    let skip = if spec.value > 0 { spec.value - 1 } else { 0 }

    return if spec.from_start {
      if skip > size { size } else { skip }
    } else if spec.value > size {
      0
    } else {
      size - spec.value
    }
  }

  let ends = tio.line_ends(data, zero)

  if spec.from_start {
    return 0 when spec.value <= 1

    return if ends.len() >= spec.value - 1 { ends[spec.value - 2] } else { size }
  }

  let partial = if size > 0 and (ends.is_empty() or ends[-1] != size) { 1 } else { 0 }
  let total = ends.len() + partial

  return 0 when total <= spec.value

  ends[total - spec.value - 1]
}

# Where to start in a chunked file. The reads here also surface the open
# errors that `open_source` cannot see (an unreadable file).
proc file_start(source: tio.Source, spec: Spec, zero: Bool) [fs, error, io] -> Result[Int] {
  if spec.bytes {
    let _ = bytes.read_at(source.path, 0, 1)?
    let skip = if spec.value > 0 { spec.value - 1 } else { 0 }

    return Ok(
      if spec.from_start {
        if skip > source.size { source.size } else { skip }
      } else if spec.value > source.size {
        0
      } else {
        source.size - spec.value
      },
    )
  }

  if spec.from_start {
    var skip = spec.value - 1
    var offset = 0

    while skip > 0 and offset < source.size {
      let chunk = tio.read_chunk(source, offset)?
      let ends = tio.line_ends(chunk, zero)

      return Ok(offset + ends[skip - 1]) when ends.len() >= skip

      skip -= ends.len()
      offset += chunk.len()
    }

    return Ok(offset)
  }

  last_lines_start(source, spec.value, zero)
}

proc prepare(source: tio.Source, spec: Spec, zero: Bool) [fs, error, io] -> Result[Plan] {
  if source.mode == "file" {
    return Ok({data: b"", start: file_start(source, spec, zero)?})
  }

  var offset = 0

  let chunks: List[Bytes] = collect {
    loop {
      let chunk = tio.read_chunk(source, offset)?
      break when chunk.is_empty()

      yield chunk
      offset += chunk.len()
      break when source.mode == "device" and spec.bytes and ! spec.from_start and offset >= spec.value
    }
  }

  let data = bytes.concat(chunks)

  Ok({data: data, start: data_start(data, spec, zero)})
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let args = modernize(tio.without_presume_pipe(argv))

  if ! args.is_empty() and rx"^-[0-9]".matches(args[0]) {
    gnu.usage_error(f"option used in invalid context -- {args[0][1..2]}")
  }

  let opts: TailOptions = cli.applet(
    args,
    {
      gnu: {status: 1},
      lines: {form: "-n --lines N", default: "", conflicts: ["bytes"]},
      bytes: {form: "-c --bytes N", default: "", conflicts: ["lines"]},
      follow: {form: "-f --follow[=HOW]", default: "", optional_default: "descriptor"},
      big_f: {form: "-F", default: false},
      retry: {form: "--retry", default: false},
      max_unchanged: {form: "--max-unchanged-stats N", default: ""},
      pid: {form: "--pid PID", default: ""},
      sleep: {form: "-s --sleep-interval N", default: ""},
      quiet: {form: "-q --quiet --silent", default: false, conflicts: ["verbose"]},
      verbose: {form: "-v --verbose", default: false, conflicts: ["quiet"]},
      zero: {form: "-z --zero-terminated", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("tail")
    return
  }

  let spec = if opts.bytes != "" {
    parse_spec(opts.bytes, true)
  } else {
    parse_spec(if opts.lines == "" { "10" } else { opts.lines }, false)
  }
  let follow_mode = if opts.big_f {
    "name"
  } else if opts.follow == "" {
    ""
  } else {
    match_choice(opts.follow, FOLLOW_MODES, "follow")
  }
  let following = follow_mode != ""
  let retrying = opts.retry or opts.big_f

  if opts.max_unchanged != "" and ! rx"^[0-9]+$".matches(opts.max_unchanged) {
    gnu.error(f"invalid maximum number of unchanged stats between opens: {gnu.quote_value(opts.max_unchanged)}")
    exit 1
  }

  if opts.sleep != "" and ! rx"^([0-9]+\.?[0-9]*|\.[0-9]+)([eE][-+]?[0-9]+)?$".matches(opts.sleep) {
    gnu.error(f"invalid number of seconds: {gnu.quote_value(opts.sleep)}")
    exit 1
  }

  var pid = 0

  if opts.pid != "" {
    let parsed = if rx"^[0-9]{1,10}$".matches(opts.pid) { opts.pid.parse_int() ?? -1 } else { -1 }

    if parsed < 0 or parsed > 2147483647 {
      gnu.error(f"invalid PID: {gnu.quote_value(opts.pid)}")
      exit 1
    }

    pid = parsed
  }

  if retrying and ! following {
    gnu.error("warning: --retry ignored; --retry is useful only when following")
  }

  if opts.pid != "" and ! following {
    gnu.error("warning: PID ignored; --pid=PID is useful only when following")
  }

  let operands = if opts.files.is_empty() { ["-"] } else { opts.files }

  if follow_mode == "name" and "-" in operands {
    gnu.error("cannot follow '-' by name")
    exit 1
  }

  return when ! following and spec.value == 0 and ! spec.from_start

  let headers = opts.verbose or (operands.len() > 1 and ! opts.quiet)
  var first = true
  var failed = false
  var growing: List[Str] = []

  for name in operands {
    let label = if name == "-" { "standard input" } else { name }

    guard let source = tio.open_source(name) else { |failure|
      gnu.cannot_open(name, failure)
      failed = true
      growing += if retrying and following { [name] } else { [] }
      continue
    }

    guard let plan = prepare(source, spec, opts.zero) else { |failure|
      if tio.is_directory(failure) {
        if headers {
          gnu.write_text(f"{if first { "" } else { "\n" }}==> {gnu.quote_maybe(label)} <==\n")
          first = false
        }

        gnu.error_reading(label, failure)

        if following {
          gnu.error(f"{gnu.quote_maybe(label)}: cannot follow end of this type of file; giving up on this name")
        }
      } else {
        gnu.cannot_open(name, failure)
      }

      failed = true
      continue
    }

    if headers {
      gnu.write_text(f"{if first { "" } else { "\n" }}==> {gnu.quote_maybe(label)} <==\n")
      first = false
    }

    if source.mode == "file" {
      var offset = plan.start

      loop {
        guard let chunk = tio.read_chunk(source, offset) else { |failure|
          gnu.error_reading(label, failure)
          failed = true
          break
        }

        break when chunk.is_empty()

        gnu.write_bytes(chunk)
        offset += chunk.len()
      }
    } else {
      gnu.write_bytes(plan.data[plan.start..])
    }

    if following and (source.kind == 8 or (name == "-" and tio.standard_file(0) != "")) {
      growing += [name]
    }
  }

  if following and ! growing.is_empty() {
    let running = [entry for entry in process.list()? if entry.pid == pid]
    let alive = pid > 0 and ! running.is_empty()

    if pid == 0 or alive {
      gnu.error(
        f"cannot follow {gnu.quote_value(growing[0])}: following a growing file is not supported because output is not flushed incrementally",
      )
      exit 1
    }
  } else if following and failed {
    gnu.error("no files remaining")
  }

  if failed {
    exit 1
  }
}
