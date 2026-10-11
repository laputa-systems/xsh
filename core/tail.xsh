#!/bin/xsh
use lib.gnu
use lib.proc_launch
use lib.textio_a1 as tio
use time
use unix

const USAGE = """Usage: tail [OPTION]... [FILE]...
Print the last 10 lines of each FILE to standard output.
With more than one FILE, precede each with a header giving the file name.

With no FILE, or when FILE is -, read standard input.

  -c, --bytes=[+]NUM       output the last NUM bytes; or use -c +NUM to
                             output starting with byte NUM of each file
      --debug       indicate which --follow implementation is used
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
  sleep: Str?,
  quiet: Bool,
  verbose: Bool,
  zero: Bool,
  debug: Bool,
  help: Bool,
  version: Bool,
  files: List[Str],
}

# `from_start` is `+NUM`: output starting at unit NUM instead of the last NUM.
type Spec = {value: Int, from_start: Bool, bytes: Bool}
type FollowFile = {label: Str, fd: Int, offset: Int}
# A followed name. `state` is `tailing` (reading the file identified by `dev` and
# `ino`), `waiting` (no file under the name yet), `vanished` (it was tailing and
# the name went missing since), or `untailable` (the name is a directory, FIFO, or
# link that may not be read). `symlink` records that the name was a link when
# following began; a name that becomes a link later is untailable, so its target
# is never read. `dir_watched` records that the directory holding the name existed
# when following began; GNU watches only such directories for removal.
type NameFile = {name: Str, label: Str, state: Str, symlink: Bool, dir_watched: Bool, dev: Int, ino: Int, offset: Int}
# One pass over a followed name: the updated state, the label of the last banner
# written, whether the name stays in the list, whether it reported a failure, and
# whether the directory holding a vanished name was found to be removed.
type NameStep = {file: NameFile, last: Str, keep: Bool, failed: Bool, dir_removed: Bool}

# Where output begins: `data` holds the whole input for sources that are read
# at once, and `start` is an offset into it (or into the file when chunked).
type Plan = {data: Bytes, start: Int}
# A FIFO operand that --pid follows once every operand's initial output is written.
type PidFifo = {label: Str, path: Path}

const FOLLOW_MODES = ["descriptor", "name"]

# GNU learns of changes from inotify as they happen, while this applet finds them
# by polling. A pass therefore never sleeps longer than this, so a change is
# reported about as promptly as GNU reports it; a shorter -s interval still applies.
const POLL_CEILING = 10ms

pure poll_pause(interval: Duration) -> Duration {
  return interval when interval < POLL_CEILING
  POLL_CEILING
}

# Following has no child process handle to carry a cancellation signal through
# evaluator cleanup, so handle TERM explicitly and stop the polling loop.
on TERM [] {}

# Rewrite the obsolete first argument `[+-]N[bcl][f]` into the options it
# stands for.
pure modernize(argv: List[Str]) -> List[Str] {
  guard ! argv.is_empty() else {
    return argv
  }

  let parts = rx"^([+-])([0-9]*)([bcl]?)(f?)$".captures(argv[0])
  return argv when parts.is_empty()

  let default_count = parts[2] == "" and (
    (parts[1] == "-" and parts[3] in ["b", "l"])
    or (parts[1] == "+" and (parts[3] in ["b", "c", "l"] or parts[4] == "f"))
  )

  return argv when parts[2] == "" and ! default_count

  let sign = if parts[1] == "+" { "+" } else { "" }
  let digits = if parts[2] == "" { "10" } else { parts[2] }
  let unit = parts[3]
  let option = if unit == "b" or unit == "c" { "-c" } else { "-n" }
  let count = if unit == "b" { f"{sign}{digits}b" } else { f"{sign}{digits}" }

  [option, count, @if parts[4] == "f" { ["-f"] } else { [] }, @argv[1..]]
}

# The option's sign is removed before strtoumax skips leading blanks. A minus
# after those blanks is therefore an unsigned underflow, not a tail count sign.
proc parse_spec(raw: Str, by_bytes: Bool) [process, env] -> Spec {
  let from_start = raw.starts_with("+")
  let digits = if from_start or raw.starts_with("-") { raw[1..] } else { raw }
  let text = rx"^[[:space:]]+".replace(digits, with: "")
  let unsigned = if text.starts_with("+") or text.starts_with("-") { text[1..] } else { text }
  let value = tio.parse_count(unsigned)
  let underflow = text.starts_with("-") and value != null and value != 0

  if value == null or underflow {
    let reason = if underflow { ": Value too large for defined data type" } else { "" }
    gnu.error(
      f"invalid number of {if by_bytes { "bytes" } else { "lines" }}: {gnu.quote_value(if from_start { raw } else { digits })}{reason}",
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
  return Ok(source.size) when spec.value == 0 and ! spec.from_start

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

# Poll a FIFO with O_NONBLOCK so timeout and --pid can run while no writer has
# connected or while a writer is silent.
proc followed_fifo_data(fifo: Path, spec: Spec, zero: Bool, pid: Int, interval: Duration) [fs, process, time, error] -> Result[Bytes] {
  let fd = unix.open_fd(fifo, nonblock: true)?
  var chunks: List[Bytes] = []

  loop {
    match unix.read_fd(fd, tio.CHUNK) {
      Ok(chunk) => { if ! chunk.is_empty() { chunks += [chunk] } }
      Err(failure) => {
        if gnu.errno(failure) not in [11, 35] {
          unix.close_fd(fd)?
          return Err(failure)
        }
      }
    }

    let alive = pid == 0 or ! [entry for entry in process.list()? if entry.pid == pid].is_empty()
    break when ! alive
    time.sleep(interval)?
  }

  unix.close_fd(fd)?
  let data = bytes.concat(chunks)
  Ok(data[data_start(data, spec, zero)..])
}

# Polling hangup only applies to pipes and sockets; regular output files never
# become readable just because their reader closed.
proc stdout_pollable() [fs, error] -> Result[Bool] {
  let metadata = fs.stat(fp"/dev/fd/1", follow_symlinks: true)?
  Ok(metadata.kind in ["fifo", "socket"])
}

# GNU writes diagnostics unbuffered, so a diagnostic reported before some output
# must reach the output stream first. Stderr is buffered here, so it is flushed
# before every write to standard output.
proc write_output(data: Bytes, flush = true) [process, env, io] {
  if let Err(_) = io.flush_stderr() {
    exit 1
  }

  if flush {
    gnu.write_bytes(data)
  } else if let Err(failure) = io.write_stdout_bytes(data) {
    gnu.write_failed(failure)
  }
}

# Writes the `==> NAME <==` line that introduces output from a file. `previous`
# is the label of the last banner, "" before any output; a banner after earlier
# output starts on a new line.
proc write_banner(label: Str, previous: Str, flush = true) [process, env, io] {
  let separator = if previous == "" { "" } else { "\n" }
  write_output(bytes.from_text(f"{separator}==> {gnu.quote_maybe(label)} <==\n"), flush)
}

# Warnings are written before any operand is read. A stderr write that fails
# cannot be reported on stderr, so the status alone carries the failure.
proc warn(message: Str) [process, env, io] {
  if let Err(_) = io.write_stderr(f"{gnu.prog()}: warning: {message}\n") {
    exit 1
  }

  if let Err(_) = io.flush_stderr() {
    exit 1
  }
}

# Whether the regular file behind `fd` is now shorter than the `offset` already
# read from it. Other kinds (such as /dev/null) have no size to shrink below.
proc descriptor_truncated(fd: Int, offset: Int) [fs, error] -> Bool {
  let entry = fs.stat(fp"/dev/fd/{fd}", follow_symlinks: true)?
  entry.kind == "file" and entry.size < offset
}

# Follow already-open regular files from their current offsets. Reading each
# descriptor once per pass lets every operand make progress while another file
# is idle. `after` is the label of the last banner already written; a banner is
# written whenever output switches to another file.
proc follow_descriptors(files: List[FollowFile], waiting: List[Str], pid: Int, interval: Duration, headers: Bool, after: Str) [fs, process, time, error, io] -> Bool {
  var active = files
  var pending = waiting
  var failed = false
  var last = after
  let poll_stdout = stdout_pollable()?

  loop {
    var next: List[FollowFile] = []
    var produced = false

    # A name that was missing when following began (under --retry) is opened once it
    # is a regular file, and read from its start like any file that appears later.
    var still_missing: List[Str] = []
    for name in pending {
      match fs.stat(fp"{name}", follow_symlinks: true) {
        Ok(entry) => {
          if entry.kind == "file" {
            gnu.error(f"{gnu.quote(name)} has appeared;  following new file")
            match unix.open_fd(fp"{name}") {
              Ok(fd) => active += [{label: name, fd: fd, offset: 0}]
              Err(failure) => {
                gnu.cannot_open(name, failure)
                failed = true
              }
            }
          } else {
            # Descriptors never retry a name that became a directory, FIFO, or device:
            # it is reported and given up, even under --retry, as GNU does.
            gnu.error(f"{gnu.quote(name)} has been replaced with an untailable file; giving up on this name")
            failed = true
          }
        }
        Err(_) => still_missing += [name]
      }
    }
    pending = still_missing

    for entry in active {
      var file = entry

      if descriptor_truncated(file.fd, file.offset) {
        gnu.error(f"{gnu.quote_maybe(file.label)}: file truncated")
        let _ = unix.seek_fd(file.fd, 0)?
        file = {...file, offset: 0}
      }

      match unix.read_fd(file.fd, tio.CHUNK) {
        Ok(chunk) => {
          if chunk.is_empty() {
            next += [file]
          } else {
            if headers and file.label != last {
              write_banner(file.label, last)
              last = file.label
            }

            write_output(chunk)
            next += [{...file, offset: file.offset + chunk.len()}]
            produced = true
          }
        }
        Err(failure) => {
          # A FIFO is opened without blocking, so an empty pipe reports EAGAIN until
          # data or a writer arrives; that is not a failure to read.
          if gnu.errno(failure) in [11, 35] {
            next += [file]
          } else {
            gnu.error_reading(file.label, failure)
            unix.close_fd(file.fd)?
            failed = true
          }
        }
      }
    }

    active = next
    break when active.is_empty() and pending.is_empty()
    let alive = pid == 0 or ! [entry for entry in process.list()? if entry.pid == pid].is_empty()
    break when ! alive and ! produced
    if ! produced {
      if poll_stdout {
        let events = unix.poll_fd(1, [])?
        break when "error" in events or "hangup" in events
      }
      io.flush_stderr()?
      time.sleep(poll_pause(interval))?
    }
  }

  if active.is_empty() {
    gnu.error("no files remaining")
  }

  for file in active { unix.close_fd(file.fd)? }
  failed
}

# Whether NAME is a symbolic link itself. A missing name is not a link.
proc is_link(name: Str) [fs] -> Bool {
  match fs.stat(fp"{name}", follow_symlinks: false) {
    Ok(entry) => entry.kind == "symlink"
    Err(_) => false
  }
}

# Report when a followed name stops naming an accessible file.
proc name_gone(label: Str, failure: Error) [process, env] -> Unit {
  gnu.error(f"{gnu.quote(label)} has become inaccessible: {gnu.strerror(failure)}")
}

# Whether the directory holding NAME does not exist. Only a missing directory
# counts; other failures to inspect it say nothing about the directory's presence.
proc parent_missing(name: Str) [fs] -> Bool {
  match fs.stat(fp"{name}".parent(), follow_symlinks: true) {
    Ok(_) => false
    Err(failure) => gnu.errno(failure) == 2
  }
}

# A name that is missing or dangling. A name already waiting stays silent. A name
# that stops being tailed becomes `vanished`, which lets a later pass notice that
# its directory was removed as well.
proc gone_step(file: NameFile, failure: Error, retrying: Bool, last: Str) [fs, process, env] -> NameStep {
  if file.state == "tailing" {
    name_gone(file.label, failure)
  }

  let was_tailed = file.state == "tailing" or file.state == "vanished"
  let state = if was_tailed { "vanished" } else { "waiting" }

  {file: {...file, state: state}, last: last, keep: retrying, failed: ! retrying, dir_removed: was_tailed and file.dir_watched and parent_missing(file.name)}
}

# A name that is a directory, FIFO, or a link that was not a link at startup.
# The message is printed once. Without retrying the name is given up.
proc untailable_step(file: NameFile, kind: Str, retrying: Bool, last: Str) [process, env] -> NameStep {
  let suffix = if retrying { "" } else { "; giving up on this name" }

  if file.state != "untailable" {
    gnu.error(f"{gnu.quote(file.label)} has been replaced with an untailable {kind}{suffix}")
  }

  {file: {...file, state: "untailable"}, last: last, keep: retrying, failed: ! retrying, dir_removed: false}
}

# Whether TARGET opens for reading. The descriptor is closed at once; the data is
# read separately by the caller.
proc openable(target: Path) [fs, process, error] -> Result[Bool, Error] {
  let fd = unix.open_fd(target)?
  defer unix.close_fd(fd)

  Ok(true)
}

# One pass over a followed name. A name's file identity (device and inode) that
# changed is read from its start; that also covers a file that appears after it
# was missing. A file that became shorter than the offset already read is
# reported as truncated and read from its start.
proc follow_name_once(file: NameFile, retrying: Bool, headers: Bool, last: Str) [fs, process, env, error, io] -> NameStep {
  let name_path = fp"{file.name}"

  guard let link = fs.stat(name_path, follow_symlinks: false) else { |failure|
    return gone_step(file, failure, retrying, last)
  }

  if link.kind == "symlink" and ! file.symlink {
    return untailable_step(file, "symbolic link", retrying, last)
  }

  guard let entry = fs.stat(name_path, follow_symlinks: true) else { |failure|
    return gone_step(file, failure, retrying, last)
  }

  if entry.kind != "file" {
    return untailable_step(file, "file", retrying, last)
  }

  var current = file
  var position = last
  var failed = false

  if file.state != "tailing" {
    # A name that exists but cannot be opened keeps waiting without a report, as GNU
    # does under --retry; it is reported once it can be read.
    match openable(name_path) {
      Ok(_) => {}
      Err(failure) => return gone_step(file, failure, retrying, last)
    }

    gnu.error(f"{gnu.quote(file.label)} has appeared;  following new file")
    current = {...file, state: "tailing", dev: entry.dev, ino: entry.ino, offset: 0}
  } else if entry.dev != file.dev or entry.ino != file.ino {
    gnu.error(f"{gnu.quote(file.label)} has been replaced;  following new file")
    current = {...file, dev: entry.dev, ino: entry.ino, offset: 0}
  }

  if entry.size < current.offset {
    gnu.error(f"{gnu.quote_maybe(file.label)}: file truncated")
    current = {...current, offset: 0}
  }

  if entry.size > current.offset {
    match bytes.read_at(name_path, current.offset, entry.size - current.offset) {
      Ok(data) => {
        if headers and file.label != position {
          write_banner(file.label, position)
          position = file.label
        }

        write_output(data)
        current = {...current, offset: current.offset + data.len()}
      }
      Err(failure) => {
        gnu.error_reading(file.label, failure)
        failed = true
      }
    }
  }

  {file: current, last: position, keep: true, failed: failed, dir_removed: false}
}

# Follow named files by checking each name every pass. A name that is gone is
# kept waiting only with retry; otherwise it is given up.
# With inotify in use, GNU reports the first removed directory and switches to
# polling for the rest of the run; that switch is reproduced once here.
proc follow_names(files: List[NameFile], headers: Bool, retrying: Bool, pid: Int, interval: Duration, after: Str, inotify: Bool) [fs, process, time, env, error, io] -> Bool {
  var active = files
  var failed = false
  var last = after
  var reverted = false

  loop {
    var next: List[NameFile] = []

    for file in active {
      let step = follow_name_once(file, retrying, headers, last)
      last = step.last
      failed = failed or step.failed

      if step.dir_removed and inotify and ! reverted {
        gnu.error("directory containing watched file was removed")
        gnu.error("inotify cannot be used, reverting to polling")
        reverted = true
      }

      if step.keep {
        next += [step.file]
      }
    }

    active = next
    break when active.is_empty()
    let alive = pid == 0 or ! [entry for entry in process.list()? if entry.pid == pid].is_empty()
    break when ! alive
    io.flush_stderr()?
    time.sleep(poll_pause(interval))?
  }

  if active.is_empty() {
    gnu.error("no files remaining")
  }

  failed
}

# How an operand is classed for the --debug report. GNU chooses the follow
# implementation from all operands together. Standard input that is a pipe is
# never followed, so it returns "pipe" and is left out of that choice.
proc classify_operand(name: Str, source: tio.Source) [fs] -> Str {
  if name == "-" {
    # An unstattable standard input cannot be watched, so it counts as unwatchable.
    guard let entry = fs.stat(fp"/dev/fd/0", follow_symlinks: true) else {
      return "unwatchable"
    }

    return "pipe" when entry.kind == "fifo"
    return if entry.kind == "char" { "device" } else { "unwatchable" }
  }

  return "watchable" when source.kind == 1 or source.kind == 8
  return "device" when source.kind == 2 or source.kind == 6
  "unwatchable"
}

proc main(...argv: List[Str]) [fs, process, env, error, io, time] {
  # GNU's hidden `---disable-inotify` selects polling instead of inotify. This applet
  # always polls, so the flag only silences the inotify-specific reports. The cli form
  # parser strips the leading dashes of long names, so this spelling cannot be declared
  # as an option; it is removed here before parsing, and this pre-scan can go once the
  # form can name it.
  var rest: List[Str] = []
  var operands_only = false
  var inotify = true
  for arg in tio.without_presume_pipe(argv) {
    if operands_only or arg == "--" {
      operands_only = true
      rest += [arg]
    } else if arg == "---disable-inotify" {
      inotify = false
    } else {
      rest += [arg]
    }
  }

  let args = modernize(rest)

  if ! args.is_empty() and rx"^-[0-9]".matches(args[0]) {
    gnu.error(f"option used in invalid context -- {args[0][1..2]}")
    exit 1
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
      sleep: {form: "-s --sleep-interval N"},
      quiet: {form: "-q --quiet --silent", default: false, conflicts: ["verbose"]},
      verbose: {form: "-v --verbose", default: false, conflicts: ["quiet"]},
      zero: {form: "-z --zero-terminated", default: false},
      debug: {form: "--debug", default: false},
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

  # Every pass re-stats each followed name, so a name is reopened no later than
  # the N-th unchanged pass the option allows; the count needs no separate state.
  if opts.max_unchanged != "" and ! rx"^[0-9]+$".matches(opts.max_unchanged) {
    gnu.error(f"invalid maximum number of unchanged stats between opens: {gnu.quote_value(opts.max_unchanged)}")
    exit 1
  }

  if let sleep_interval = opts.sleep {
    if ! rx"^([0-9]+\.?[0-9]*|\.[0-9]+)([eE][-+]?[0-9]+)?$".matches(sleep_interval) {
      gnu.error(f"invalid number of seconds: {gnu.quote_value(sleep_interval)}")
      exit 1
    }
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
    warn("--retry ignored; --retry is useful only when following")
  }

  if opts.pid != "" and ! following {
    warn("PID ignored; --pid=PID is useful only when following")
  }

  # Preserve warning order when stderr and stdout share the same destination.
  if ! following and (retrying or opts.pid != "") { io.flush_stderr()? }

  let operands = if opts.files.is_empty() { ["-"] } else { opts.files }

  if retrying and follow_mode == "descriptor" {
    warn("--retry only effective for the initial open")
  }

  if following and unix.isatty(0) and "-" in operands {
    warn("following standard input indefinitely is ineffective")
  }

  if follow_mode == "name" and "-" in operands {
    gnu.error("cannot follow '-' by name")
    exit 1
  }

  return when ! following and spec.value == 0 and ! spec.from_start

  # GNU leaves initial output buffered while reading a terminal operand. A
  # redirected stream must not expose that buffer before the blocked read ends.
  let buffer_initial = following and "-" in operands and unix.isatty(0) and ! unix.isatty(1)
  let headers = opts.verbose or (operands.len() > 1 and ! opts.quiet)
  var last = ""
  var failed = false
  var waiting: List[Str] = []
  var name_files: List[NameFile] = []
  var follow_files: List[FollowFile] = []
  var follow_classes: List[Str] = []
  var pid_fifos: List[PidFifo] = []

  for name in operands {
    let label = if name == "-" { "standard input" } else { name }

    guard let source = tio.open_source(name) else { |failure|
      gnu.cannot_open(name, failure)
      failed = true
      follow_classes += ["unwatchable"]

      if retrying and following and follow_mode == "name" {
        name_files += [{name: name, label: label, state: "waiting", symlink: is_link(name), dir_watched: ! parent_missing(name), dev: 0, ino: 0, offset: 0}]
      } else if retrying and following {
        waiting += [name]
      }

      continue
    }

    let operand_class = classify_operand(name, source)
    if operand_class != "pipe" {
      follow_classes += [operand_class]
    }

    if following and opts.pid != "" and source.kind == 1 {
      pid_fifos += [{label: label, path: source.path}]
      continue
    }

    # A FIFO operand is followed from its first byte without waiting for a writer to
    # close, so its output streams as it arrives; the descriptor is opened without
    # blocking for that reason.
    if following and follow_mode == "descriptor" and source.kind == 1 {
      guard let fd = unix.open_fd(source.path, nonblock: true) else { |failure|
        gnu.error_reading(label, failure)
        follow_classes += ["unwatchable"]
        failed = true
        continue
      }

      follow_files += [{label: label, fd: fd, offset: 0}]
      continue
    }

    # A read from standard input can block (a terminal, or a pipe still open), so
    # its header is written before the read rather than after it.
    let header_written = headers and source.mode == "stdin"

    if header_written {
      write_banner(label, last, ! buffer_initial)
      last = label
    }

    guard let plan = prepare(source, spec, opts.zero) else { |failure|
      if tio.is_directory(failure) {
        if headers {
          write_banner(label, last, ! buffer_initial)
          last = label
        }

        gnu.error_reading(label, failure)

        # Retry keeps a followed name waiting; descriptors give up on a directory.
        if following {
          let giving_up = if retrying { "" } else { "; giving up on this name" }
          gnu.error(f"{gnu.quote_maybe(label)}: cannot follow end of this type of file{giving_up}")
        }

        if following and retrying and follow_mode == "name" {
          name_files += [{name: name, label: label, state: "untailable", symlink: is_link(name), dir_watched: ! parent_missing(name), dev: 0, ino: 0, offset: 0}]
        }
      } else {
        gnu.cannot_open(name, failure)
      }

      follow_classes += ["unwatchable"]
      failed = true
      continue
    }

    if headers and ! header_written {
      write_banner(label, last, ! buffer_initial)
      last = label
    }

    var follow_offset = 0

    if source.mode == "file" {
      var offset = plan.start

      loop {
        guard let chunk = tio.read_chunk(source, offset) else { |failure|
          gnu.error_reading(label, failure)
          failed = true
          break
        }

        break when chunk.is_empty()

        write_output(chunk, ! buffer_initial)
        offset += chunk.len()
      }
      follow_offset = offset
    } else {
      write_output(plan.data[plan.start..], ! buffer_initial)
    }

    # Regular files and character devices (such as /dev/null, which never ends)
    # are followed by reading the open descriptor again on each pass.
    if following and follow_mode == "descriptor" and (source.kind == 8 or source.kind == 2) {
      guard let fd = unix.open_fd(source.path) else { |failure|
        gnu.error_reading(label, failure)
        follow_classes += ["unwatchable"]
        failed = true
        continue
      }
      let _ = unix.seek_fd(fd, follow_offset)?
      follow_files += [{label: label, fd: fd, offset: follow_offset}]
    } else if following and follow_mode == "name" and source.kind == 8 {
      let entry = fs.stat(source.path, follow_symlinks: true)?
      name_files += [{name: name, label: label, state: "tailing", symlink: is_link(name), dir_watched: ! parent_missing(name), dev: entry.dev, ino: entry.ino, offset: follow_offset}]
    } else if following and follow_mode == "descriptor" and name == "-" and tio.standard_file(0) != "" {
      follow_files += [{label: label, fd: 0, offset: tio.stdin_offset()?}]
    }
  }

  if buffer_initial { gnu.write_bytes(b"") }

  # The report follows the initial output of every operand, as GNU orders it, and
  # precedes any follow. Standard input that is a pipe yields no class, so a
  # command that follows nothing reports nothing.
  if following and opts.debug and ! follow_classes.is_empty() {
    # A lone character device is read without polling. Any operand that cannot be
    # watched, and --disable-inotify, select polling; otherwise inotify is used.
    let implementation = if operands.len() == 1 and follow_classes == ["device"] and follow_mode == "descriptor" and pid == 0 {
      "blocking"
    } else if ! inotify or "device" in follow_classes or "unwatchable" in follow_classes {
      "polling"
    } else {
      "notification"
    }

    gnu.error(f"using {implementation} mode")
  }

  for fifo in pid_fifos {
    let interval = if let sleep_interval = opts.sleep { proc_launch.interval(sleep_interval) ?? 1s } else { 1s }
    guard let data = followed_fifo_data(fifo.path, spec, opts.zero, pid, interval) else { |failure|
      gnu.error_reading(fifo.label, failure)
      failed = true
      continue
    }

    if headers {
      write_banner(fifo.label, last)
      last = fifo.label
    }
    write_output(data)
  }

  if following and follow_mode == "descriptor" and (! follow_files.is_empty() or ! waiting.is_empty()) {
    let interval = if let sleep_interval = opts.sleep { proc_launch.interval(sleep_interval) ?? 1s } else { 1s }
    failed = follow_descriptors(follow_files, waiting, pid, interval, headers, last) or failed
  }

  if following and follow_mode == "name" and ! name_files.is_empty() {
    let interval = if let sleep_interval = opts.sleep { proc_launch.interval(sleep_interval) ?? 1s } else { 1s }
    failed = follow_names(name_files, headers, retrying, pid, interval, last, inotify) or failed
  }

  if following and failed and follow_files.is_empty() and name_files.is_empty() and waiting.is_empty() {
    gnu.error("no files remaining")
  }

  if failed {
    exit 1
  }
}
