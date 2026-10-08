#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio
use lib.text_a2 as text

# Exiting through a signal hook runs deferred cleanup for active filter FIFOs.
on INT [] {
  exit 130
}

on TERM [] {
  exit 143
}

const USAGE = """Usage: split [OPTION]... [FILE [PREFIX]]
Output pieces of FILE to PREFIXaa, PREFIXab, ...;
default size is 1000 lines, and default PREFIX is 'x'.

With no FILE, or when FILE is -, read standard input.

Mandatory arguments to long options are mandatory for short options too.
  -a, --suffix-length=N   generate suffixes of length N (default 2)
      --additional-suffix=SUFFIX  append an additional SUFFIX to file names
  -b, --bytes=SIZE        put SIZE bytes per output file
  -C, --line-bytes=SIZE   put at most SIZE bytes of records per output file
  -d                      use numeric suffixes starting at 0, not alphabetic
      --numeric-suffixes[=FROM]  same as -d, but allow setting the start value
  -x                      use hex suffixes starting at 0, not alphabetic
      --hex-suffixes[=FROM]  same as -x, but allow setting the start value
  -e, --elide-empty-files  do not generate empty output files with '-n'
      --filter=COMMAND    write to shell COMMAND; file name is $FILE
  -l, --lines=NUMBER      put NUMBER lines/records per output file
  -n, --number=CHUNKS     generate CHUNKS output files; see explanation below
  -t, --separator=SEP     use SEP instead of newline as the record separator;
                            '\\0' (zero) specifies the NUL character
  -u, --unbuffered        immediately copy input to output with '-n r/...'
      --verbose           print a diagnostic just before each
                            output file is opened
      --help        display this help and exit
      --version     output version information and exit

The SIZE argument is an integer and optional unit (example: 10K is 10*1024).
Units are K,M,G,T,P,E,Z,Y,R,Q (powers of 1024) or KB,MB,... (powers of 1000).
Binary prefixes can be used, too: KiB=K, MiB=M, and so on.

CHUNKS may be:
  N       split into N files based on size of input
  K/N     output Kth of N to stdout
  l/N     split into N files without splitting lines/records
  l/K/N   output Kth of N to stdout without splitting lines/records
  r/N     like 'l' but use round robin distribution
  r/K/N   likewise but only output Kth of N to stdout
"""

type SplitOptions = {
  suffix_length: Str,
  additional: Str,
  bytes: Str,
  line_bytes: Str,
  short_numeric: Bool,
  numeric: Str,
  short_hex: Bool,
  hex: Str,
  elide: Bool,
  filter: Str?,
  lines: Str,
  number: Str,
  separator: List[Str],
  unbuffered: Bool,
  verbose: Bool,
  help: Bool,
  version: Bool,
  files: List[Str],
}

# Chunks for `-n`: `kind` is bytes, l or r; `k` is the chunk to print (0 for
# every chunk); `n` the chunk count.
type Chunks = {kind: Str, k: Int, n: Int}

# How output names are built: radix and width of the suffix, its first value,
# and whether the width grows when the names run out.
type Naming = {prefix: Bytes, radix: Int, width: Int, start: Int, widen: Bool, extra: Bytes}

type Rewritten = {argv: List[Str], obsolete: Str, blksize: Str}

const DIGITS = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]

# Rewrite the obsolete `-NUM` (lines per file) out of the argument list: digit
# runs inside a short cluster are removed and remembered, the way `-NUM` and
# `-d200a4` read. A value that merely starts with `-` stays untouched. The
# undocumented `---io-blksize` is taken out as well (the cli cannot declare an
# option name that starts with a dash).
pure modernize(argv: List[Str]) -> Rewritten {
  var obsolete = ""
  var blksize = ""
  var pending = false
  var options = true
  var value = false

  let out: List[Str] = collect {
    for item in argv {
      if pending {
        blksize = item
        pending = false
      } else if ! options or value {
        yield item
        value = false
      } else if item == "--" {
        options = false
        yield item
      } else if item == "---io-blksize" {
        pending = true
      } else if item.starts_with("---io-blksize=") {
        blksize = item.byte_slice(14)
      } else if item.starts_with("--") or ! item.starts_with("-") or item.byte_len() < 2 {
        yield item
        value = item in [
          "--suffix-length",
          "--additional-suffix",
          "--bytes",
          "--line-bytes",
          "--filter",
          "--lines",
          "--number",
          "--separator",
        ]
      } else {
        var kept = "-"
        var at = 1
        var taken = false

        while at < item.byte_len() {
          let ch = item.byte_slice(at, length: 1)

          if ch in DIGITS {
            var stop = at

            while stop < item.byte_len() and item.byte_slice(stop, length: 1) in DIGITS {
              stop += 1
            }

            obsolete = item.byte_slice(at, length: stop - at)
            at = stop
          } else if ch in ["a", "b", "C", "l", "n", "t"] {
            kept = kept + item.byte_slice(at)
            taken = at == item.byte_len() - 1
            at = item.byte_len()
          } else {
            kept = kept + ch
            at += 1
          }
        }

        yield kept when kept != "-"

        value = taken
      }
    }
  }

  {argv: out, obsolete: obsolete, blksize: blksize}
}

# uutils reports an invalid short option left behind by the obsolete -NUM
# spelling with its argv-parser wording rather than GNU's single-letter hint.
pure invalid_obsolete_short(argv: List[Str]) -> Str? {
  var options = true
  var value = false

  for item in argv {
    if item == "--" {
      options = false
      continue
    }

    if ! options or value {
      value = false
      continue
    }

    if item.starts_with("--") {
      value = item in [
        "--suffix-length",
        "--additional-suffix",
        "--bytes",
        "--line-bytes",
        "--filter",
        "--lines",
        "--number",
        "--separator",
      ]
      continue
    }

    if item.starts_with("-") and item.byte_len() > 1 {
      var at = 1

      while at < item.byte_len() and item.byte_slice(at, length: 1) in DIGITS {
        at += 1
      }

      if at > 1 and at < item.byte_len() {
        let option = item.byte_slice(at, length: 1)

        return "-" + option when option not in ["a", "b", "C", "d", "e", "l", "n", "t", "u", "x"]
      }
    }

    value = item in ["-a", "-b", "-C", "-l", "-n", "-t"]
  }

  null
}

pure missing_separator_value(argv: List[Str]) -> Bool {
  var options = true
  var value = false

  for item in argv {
    if value {
      value = false
      continue
    }

    if item == "--" {
      options = false
      continue
    }

    if options and item in ["-t", "--separator"] {
      return true when item == argv[-1]
      value = true
      continue
    }

    if options {
      value = item in [
        "-a",
        "-b",
        "-C",
        "-l",
        "-n",
        "-t",
        "--suffix-length",
        "--additional-suffix",
        "--bytes",
        "--line-bytes",
        "--filter",
        "--lines",
        "--number",
        "--separator",
      ]
    }
  }

  false
}

# An unsigned decimal; values past u64 are null, values past Int clamp.
pure parse_u64(text: Str) -> Int? {
  return null when ! rx"^[0-9]+$".matches(text)

  let digits = rx"^0+".replace(text, with: "")

  return 0 when digits == ""
  return null when digits.byte_len() > 20 or (digits.byte_len() == 20 and digits > "18446744073709551615")
  return tio.MAX_COUNT when digits.byte_len() > 18

  digits.parse_int() ?? 0
}

# The number of digits in RADIX needed to write the values below LIMIT.
pure digits_needed(limit: Int, radix: Int) -> Int {
  var width = 0
  var reach = 1

  while reach < limit {
    return width + 1 when reach > tio.MAX_COUNT / radix

    reach *= radix
    width += 1
  }

  width
}

pure digit_char(value: Int, radix: Int) -> Str {
  return "abcdefghijklmnopqrstuvwxyz".byte_slice(value, length: 1) when radix == 26

  "0123456789abcdef".byte_slice(value, length: 1)
}

# VALUE written with exactly WIDTH digits, or "" when it does not fit.
pure fixed_name(value: Int, radix: Int, width: Int) -> Str {
  var rest = value
  var out = ""

  repeat width times {
    out = digit_char(rest % radix, radix) + out
    rest = rest / radix
  }

  if rest > 0 { "" } else { out }
}

# The suffix of the file at position INDEX; "" when the names are exhausted.
# Auto-widening names put a fill of top digits in front: xaa..xyz, xzaaa, ...
pure suffix_name(index: Int, naming: Naming) -> Str {
  let current = naming.start + index

  return fixed_name(current, naming.radix, naming.width) when ! naming.widen

  var remaining = current
  var span = (naming.radix - 1) * naming.radix
  var width = 2

  while remaining >= span {
    remaining -= span
    span *= naming.radix
    width += 1
  }

  var digits = ""

  repeat width times {
    digits = digit_char(remaining % naming.radix, naming.radix) + digits
    remaining = remaining / naming.radix
  }

  var fill = ""

  repeat width - 2 times {
    fill = fill + digit_char(naming.radix - 1, naming.radix)
  }

  fill + digits
}

pure output_name(index: Int, naming: Naming) -> Bytes {
  bytes.concat([naming.prefix, bytes.from_text(suffix_name(index, naming)), naming.extra])
}

pure contains_slash(value: Bytes) -> Bool {
  for at in range(value.len()) {
    return true when value.byte_at(at) == 47
  }

  false
}

# Offsets just past each record separator, plus the end of an unterminated
# final record.
proc record_ends(data: Bytes, sep: Int) [error] -> Result[List[Int]] {
  var ends: List[Int] = []

  if sep == 10 {
    ends = tio.line_ends(data, false)
  } else if let Ok(text) = data.utf8() {
    let mark = if sep == 0 { "\0" } else { bytes.from_ints([sep])?.utf8() ?? "\n" }
    let parts = text.split(mark)
    var at = 0

    for part in parts[..parts.len() - 1] {
      at += part.byte_len() + 1
      ends += [at]
    }
  } else {
    for index in range(data.len()) {
      if data.byte_at(index) == sep {
        ends += [index + 1]
      }
    }
  }

  if ! data.is_empty() and (ends.is_empty() or ends[-1] != data.len()) {
    ends += [data.len()]
  }

  ends
}

pure bytes_pieces(data: Bytes, size: Int) -> List[Bytes] {
  var at = 0
  let total = data.len()

  let out: List[Bytes] = collect {
    while at < total {
      let stop = if size < total - at { at + size } else { total }

      yield data[at..stop]
      at = stop
    }
  }

  out
}

pure lines_pieces(data: Bytes, ends: List[Int], per: Int) -> List[Bytes] {
  var first = 0
  let count = ends.len()

  let out: List[Bytes] = collect {
    while first < count {
      let last = if per < count - first { first + per } else { count }
      let from = if first == 0 { 0 } else { ends[first - 1] }

      yield data[from..ends[last - 1]]
      first = last
    }
  }

  out
}

# `-C`: whole records up to SIZE bytes per piece; a record longer than SIZE
# starts a fresh piece and is cut every SIZE bytes, its tail staying open.
pure line_bytes_pieces(data: Bytes, ends: List[Int], size: Int, sep: Int) -> List[Bytes] {
  var out: List[Bytes] = []
  var origin = 0
  var used = 0

  for index in range(ends.len()) {
    let from = if index == 0 { 0 } else { ends[index - 1] }
    let width = ends[index] - from

    # A final record with no separator cannot fill the piece exactly: GNU cannot
    # tell that the line has ended, so it starts the next piece with it.
    let open_end = data.byte_at(ends[index] - 1) != sep

    if used > 0 and (used + width > size or (open_end and used + width == size)) {
      out += [data[origin..origin + used]]
      origin = from
      used = 0
    }

    if used + width <= size {
      used += width
    } else {
      var at = from

      while ends[index] - at > size {
        out += [data[at..at + size]]
        at += size
      }

      origin = at
      used = ends[index] - at
    }
  }

  if used > 0 {
    out += [data[origin..origin + used]]
  }

  out
}

# The 1-based chunk of `-n N` that contains byte POSITION of a SIZE-byte input.
pure chunk_of(position: Int, size: Int, count: Int) -> Int {
  let quotient = size / count
  let remainder = size % count
  let border = remainder * (quotient + 1)

  return position / (quotient + 1) + 1 when position < border

  remainder + (position - border) / quotient + 1
}

pure chunk_bytes(data: Bytes, count: Int, k: Int) -> Bytes {
  let size = data.len()
  let quotient = size / count
  let remainder = size % count
  let from = (k - 1) * quotient + (if k - 1 < remainder { k - 1 } else { remainder })
  let to = from + quotient + (if k <= remainder { 1 } else { 0 })

  data[from..to]
}

# The Kth of COUNT chunks without splitting records, or every chunk (K = 0).
pure chunk_lines(data: Bytes, ends: List[Int], count: Int, k: Int, elide: Bool) -> List[Bytes] {
  var out: List[Bytes] = []
  var current = 0
  var origin = 0
  var stop = 0
  let size = data.len()

  for index in range(ends.len()) {
    let from = if index == 0 { 0 } else { ends[index - 1] }
    let chunk = chunk_of(from, size, count)

    if chunk != current {
      if current > 0 and (k == 0 or k == current) {
        out += [data[origin..stop]]
      }

      if k == 0 and ! elide {
        repeat chunk - current - 1 times {
          out += [b""]
        }
      }

      current = chunk
      origin = from
    }

    stop = ends[index]
  }

  if current > 0 and (k == 0 or k == current) {
    out += [data[origin..stop]]
  }

  if k == 0 and ! elide {
    repeat count - current times {
      out += [b""]
    }
  } else if k > 0 and out.is_empty() {
    out += [b""]
  }

  out
}

pure round_robin(data: Bytes, ends: List[Int], count: Int, k: Int, elide: Bool) -> List[Bytes] {
  if k > 0 {
    let parts: List[Bytes] = collect {
      for index in range(ends.len()) {
        if index % count == k - 1 {
          let from = if index == 0 { 0 } else { ends[index - 1] }
          yield data[from..ends[index]]
        }
      }
    }

    return [bytes.concat(parts)]
  }

  let width = if count < ends.len() { count } else { ends.len() }
  var buckets: List[List[Bytes]] = [[] for _ in range(width)]

  for index in range(ends.len()) {
    let from = if index == 0 { 0 } else { ends[index - 1] }
    let slot = index % count

    buckets[slot] += [data[from..ends[index]]]
  }

  var out: List[Bytes] = [bytes.concat(parts) for parts in buckets]

  if ! elide {
    repeat count - width times {
      out += [b""]
    }
  }

  out
}

# Round-robin filters run concurrently so one can stop reading without buffering all input.
type FilterPipe = {handle: ProcessHandle, fd: Int, open: Bool, done: Bool, status: Int}

# Short waits let managed timeout signals interrupt a blocked filter pipeline.
const FILTER_POLL = 50ms

proc run_filter(command: Str, name: Path, input: Bytes) [process, error] -> Result[Int] {
  let status = process.run(process.command_argv("sh", ["sh", "-c", command], p".", {FILE: name}, input))?
  status.shell_code()
}

proc check_filter(command: Str, status: Int) [process, env, io] {
  if status != 0 {
    gnu.error(f"with filter '{command}': failed")
    exit 1
  }
}

proc filter_bytes_from_stdin(command: Str, naming: Naming, size: Int, verbose: Bool) [fs, process, env, error, io] {
  var made = 0
  var eof = false

  while ! eof {
    var parts: List[Bytes] = []
    var remaining = size

    while remaining > 0 {
      let count = if remaining < tio.CHUNK { remaining } else { tio.CHUNK }
      let data = io.stdin_read(count)?

      if data.is_empty() {
        eof = true
        break
      }

      parts += [data]
      remaining -= data.len()
    }

    let piece = bytes.concat(parts)

    if piece.is_empty() {
      return
    }

    let tail = suffix_name(made, naming)

    if tail == "" {
      gnu.error("output file suffixes exhausted")
      exit 1
    }

    let name = Path.parse_bytes(output_name(made, naming))?

    if verbose {
      gnu.write_text(f"creating file {gnu.quote_bytes(name.bytes())}\n")
    }

    check_filter(command, run_filter(command, name, piece)?)
    made += 1
  }
}

proc start_filter_pipe(root: Path, slot: Int, command: Str, name: Path) [fs, process, error] -> Result[FilterPipe] {
  let fifo = fp"{root}/filter-{slot}"
  fs.mkfifo(fifo, 0o600)?
  let reader = unix.open_fd(fifo, nonblock: true)?
  let writer = unix.open_fd(fifo, write: true, nonblock: true)?
  let plan = process.command_argv("sh", ["sh", "-c", command], p".", {FILE: name}, stdin: fifo)
  let child = spawn plan?
  unix.close_fd(reader)?

  Ok({handle: child, fd: writer, open: true, done: false, status: -1})
}

proc close_filter_pipe(pipe: FilterPipe) [process, error] -> Result[FilterPipe] {
  if pipe.fd >= 0 {
    unix.close_fd(pipe.fd)?
  }

  Ok({...pipe, fd: -1, open: false})
}

proc close_filter_pipes(pipes: List[FilterPipe?]) [process, error] {
  for pipe in pipes {
    if let current = pipe {
      if current.fd >= 0 {
        unix.close_fd(current.fd)?
      }
    }
  }
}

# Pipe writes report EAGAIN as 11 on Linux and 35 on Darwin and BSD.
proc write_filter_pipe(pipe: FilterPipe, data: Bytes) [process, error] -> Result[FilterPipe] {
  return Ok(pipe) when ! pipe.open or data.is_empty()

  var offset = 0
  var current = pipe

  while offset < data.len() {
    match unix.write_fd(current.fd, data[offset..]) {
      Ok(written) => {
        if written == 0 {
          return Err(error.failure("filter input made no write progress"))
        }

        offset += written
      }
      Err(failure) => {
        let number = gnu.errno(failure)

        if number == 32 {
          current = close_filter_pipe(current)?
          break
        } else if number == 4 {
          continue
        } else if number == 11 or number == 35 {
          let events = unix.poll_fd(current.fd, ["writable"], timeout_ms: 50)?

          if "error" in events or "hangup" in events or "invalid" in events {
            current = close_filter_pipe(current)?
            break
          }
        } else {
          return Err(failure)
        }
      }
    }
  }

  Ok(current)
}

proc refresh_filter_pipe(pipe: FilterPipe) [process, error] -> Result[FilterPipe] {
  return Ok(pipe) when pipe.done

  if let finished = process.wait_timeout([pipe.handle], 0ms)? {
    let closed = close_filter_pipe(pipe)?
    return Ok({...closed, done: true, status: finished.status.shell_code()?})
  }

  Ok(pipe)
}

proc wait_filter_pipe(pipe: FilterPipe) [process, error] -> Result[FilterPipe] {
  var current = close_filter_pipe(pipe)?

  while ! current.done {
    if let finished = process.wait_timeout([current.handle], FILTER_POLL)? {
      current = {...current, done: true, status: finished.status.shell_code()?}
    }
  }

  Ok(current)
}

proc refresh_filter_pipes(pipes: List[FilterPipe?]) [process, error] -> Result[List[FilterPipe?]] {
  var current = pipes

  for slot in range(current.len()) {
    if let pipe = current[slot] {
      current[slot] = refresh_filter_pipe(pipe)?
    }
  }

  Ok(current)
}

pure filter_pipes_finished(pipes: List[FilterPipe?]) -> Bool {
  for pipe in pipes {
    if let current = pipe {
      return false when ! current.done
    } else {
      return false
    }
  }

  true
}

proc feed_filter_record(
  pipes: List[FilterPipe?],
  root: Path,
  slot: Int,
  command: Str,
  naming: Naming,
  data: Bytes,
  verbose: Bool,
) [fs, process, env, error, io] -> Result[List[FilterPipe?]] {
  var current = pipes

  if current[slot] == null {
    let tail = suffix_name(slot, naming)

    if tail == "" {
      gnu.error("output file suffixes exhausted")
      exit 1
    }

    let name = Path.parse_bytes(output_name(slot, naming))?

    if verbose {
      gnu.write_text(f"creating file {gnu.quote_bytes(name.bytes())}\n")
    }

    current[slot] = start_filter_pipe(root, slot, command, name)?
  }

  if let pipe = current[slot] {
    current[slot] = write_filter_pipe(pipe, data)?
  }

  refresh_filter_pipes(current)
}

proc filter_round_robin_stdin(command: Str, naming: Naming, count: Int, sep: Int, elide: Bool, verbose: Bool) [fs, process, env, error, io] {
  let scratch = fs.tempdir()?
  defer scratch.close()
  let root = scratch.host_path()?
  var pipes: List[FilterPipe?] = [null for _ in range(count)]
  defer { close_filter_pipes(pipes)? }
  var pending = b""
  var record_index = 0
  var eof = false
  var stopped = false

  while ! eof and ! stopped {
    let chunk = io.stdin_read(tio.CHUNK)?
    eof = chunk.is_empty()
    pending = bytes.concat([pending, chunk])
    var start = 0

    for index in range(pending.len()) {
      if pending.byte_at(index) == sep {
        pipes = feed_filter_record(pipes, root, record_index % count, command, naming, pending[start..index + 1], verbose)?
        start = index + 1
        record_index += 1
        stopped = filter_pipes_finished(pipes)

        if stopped {
          break
        }
      }
    }

    pending = pending[start..]
  }

  if ! stopped and eof and ! pending.is_empty() {
    pipes = feed_filter_record(pipes, root, record_index % count, command, naming, pending, verbose)?
    stopped = filter_pipes_finished(pipes)
  }

  for slot in range(pipes.len()) {
    if let pipe = pipes[slot] {
      pipes[slot] = wait_filter_pipe(pipe)?
    }
  }

  if ! elide {
    for slot in range(pipes.len()) {
      if pipes[slot] == null {
        let tail = suffix_name(slot, naming)

        if tail == "" {
          gnu.error("output file suffixes exhausted")
          exit 1
        }

        let name = Path.parse_bytes(output_name(slot, naming))?

        if verbose {
          gnu.write_text(f"creating file {gnu.quote_bytes(name.bytes())}\n")
        }

        check_filter(command, run_filter(command, name, b"")?)
      }
    }
  }

  for pipe in pipes {
    if let current = pipe {
      check_filter(command, current.status)
    }
  }
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  let prepared = gnu.prepare_arguments(argv)
  let rewritten = modernize(prepared.text)

  if rewritten.obsolete != "" {
    if let option = invalid_obsolete_short(prepared.text) {
      gnu.error(f"error: unexpected argument '{option}' found")
      gnu.error("For more information, try '--help'.")
      exit 1
    }
  }

  if missing_separator_value(prepared.text) {
    gnu.error("error: a value is required for '--separator <SEP>' but none was supplied")
    gnu.error("For more information, try '--help'.")
    exit 1
  }

  let opts: SplitOptions = cli.applet(
    rewritten.argv,
    {
      gnu: {
        status: 1,
      },
      suffix_length: {
        form: "-a --suffix-length N",
        default: "",
      },
      additional: {
        form: "--additional-suffix SUFFIX",
        default: "",
      },
      bytes: {
        form: "-b --bytes SIZE",
        default: "",
      },
      line_bytes: {
        form: "-C --line-bytes SIZE",
        default: "",
      },
      short_numeric: {
        form: "-d",
        default: false,
        conflicts: [
          "numeric",
          "short_hex",
          "hex",
        ],
      },
      numeric: {
        form: "--numeric-suffixes[=FROM]",
        default: "-",
        optional_default: "",
        conflicts: [
          "short_numeric",
          "short_hex",
          "hex",
        ],
      },
      short_hex: {
        form: "-x",
        default: false,
        conflicts: [
          "numeric",
          "short_numeric",
          "hex",
        ],
      },
      hex: {
        form: "--hex-suffixes[=FROM]",
        default: "-",
        optional_default: "",
        conflicts: [
          "numeric",
          "short_numeric",
          "short_hex",
        ],
      },
      elide: {
        form: "-e --elide-empty-files",
        default: false,
      },
      filter: {
        form: "--filter COMMAND",
      },
      lines: {
        form: "-l --lines NUMBER",
        default: "",
      },
      number: {
        form: "-n --number CHUNKS",
        default: "",
      },
      separator: {
        form: "-t --separator SEP",
        repeated: true,
      },
      unbuffered: {
        form: "-u --unbuffered",
        default: false,
      },
      verbose: {
        form: "--verbose",
        default: false,
      },
      help: {
        form: "--help",
        default: false,
        stop: true,
      },
      version: {
        form: "--version",
        default: false,
        stop: true,
      },
      files: {
        form: "...FILE",
      },
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("split")
    return
  }

  if opts.files.len() > 2 {
    gnu.usage_error(f"extra operand {gnu.quote_bytes(gnu.argument_bytes(opts.files[2], prepared.raw))}")
  }

  var blksize = 131072

  if rewritten.blksize != "" {
    let parsed = if rx"^[0-9]".matches(rewritten.blksize) { tio.parse_count(rewritten.blksize) } else { null }

    if parsed == null or parsed > 2147483647 or parsed == 0 {
      gnu.error(f"invalid IO block size: {gnu.quote(rewritten.blksize)}")
      exit 1
    }

    blksize = parsed ?? blksize
  }

  let line_text = if rewritten.obsolete != "" { rewritten.obsolete } else { opts.lines }
  var ways = 0

  for given in [opts.lines != "", rewritten.obsolete != "", opts.bytes != "", opts.line_bytes != "", opts.number != ""] {
    if given {
      ways += 1
    }
  }

  if ways > 1 {
    gnu.error("cannot split in more than one way")
    exit 1
  }

  var lines = 1000
  var size = 0
  var line_size = 0
  var chunks: Chunks = Chunks(kind: "", k: 0, n: 0)

  if line_text != "" {
    # A count past the unsigned range reads as the largest count, so one piece.
    let parsed: Int? = if rx"^[0-9]{19,}$".matches(line_text) { tio.MAX_COUNT } else { parse_u64(line_text) }

    if parsed == null or parsed == 0 {
      gnu.error(f"invalid number of lines: {if parsed == null { gnu.quote(line_text) } else { "0" }}")
      exit 1
    }

    lines = parsed
  }

  for text in [opts.bytes, opts.line_bytes] {
    if text != "" {
      let parsed = if rx"^[0-9]".matches(text) { tio.parse_count(text) } else { null }

      if parsed == null or parsed == 0 {
        gnu.error(f"invalid number of bytes: {if parsed == null { gnu.quote(text) } else { "0" }}")
        exit 1
      }

      if text == opts.bytes {
        size = parsed
      } else {
        line_size = parsed
      }
    }
  }

  if opts.number != "" {
    let parts = opts.number.split("/")
    var kind = "bytes"
    var k_text = ""
    var n_text = ""

    if parts.len() == 1 {
      n_text = parts[0]
    } else if parts.len() == 2 and parts[0] in ["l", "r"] {
      kind = parts[0]
      n_text = parts[1]
    } else if parts.len() == 2 {
      k_text = parts[0]
      n_text = parts[1]
    } else if parts.len() == 3 and parts[0] in ["l", "r"] {
      kind = parts[0]
      k_text = parts[1]
      n_text = parts[2]
    } else {
      gnu.error(f"invalid number of chunks: {gnu.quote(opts.number)}")
      exit 1
    }

    let total = parse_u64(n_text)

    if total == null or total == 0 {
      gnu.error(f"invalid number of chunks: {gnu.quote(n_text)}")
      exit 1
    }

    var which = 0

    if k_text != "" {
      let picked = parse_u64(k_text)

      if picked == null or picked == 0 or picked > total {
        gnu.error(f"invalid chunk number: {gnu.quote(k_text)}")
        exit 1
      }

      which = picked
    }

    chunks = {kind: kind, k: which, n: total}
  }

  if opts.filter != null and chunks.k > 0 {
    gnu.error("--filter does not process a chunk extracted to stdout")
    exit 1
  }

  var sep = 10
  var distinct: List[Bytes] = []

  for text in opts.separator {
    let value = gnu.argument_bytes(text, prepared.raw)
    if ! (value in distinct) {
      distinct += [value]
    }
  }

  if distinct.len() > 1 {
    gnu.error("multiple separator characters specified")
    exit 1
  }

  if distinct.len() == 1 {
    let value = distinct[0]

    if value == b"\\0" {
      sep = 0
    } else if value.len() == 1 {
      sep = value.byte_at(0) ?? 10
    } else {
      gnu.error(f"multi-character separator {gnu.quote_bytes(value)}")
      exit 1
    }
  }

  let additional = gnu.argument_bytes(opts.additional, prepared.raw)

  if contains_slash(additional) {
    gnu.usage_error(f"invalid suffix {gnu.quote_bytes(additional)}, contains directory separator")
  }

  let numbered = opts.short_numeric or opts.numeric != "-"
  let hexed = opts.short_hex or opts.hex != "-"
  let radix = if hexed { 16 } else if numbered { 10 } else { 26 }
  let start_text = if opts.numeric != "-" { opts.numeric } else if opts.hex != "-" { opts.hex } else { "" }
  var start = 0
  var widen = true

  if start_text != "" {
    let parsed = if hexed { ("0x" + start_text).parse_int() ?? -1 } else { parse_u64(start_text) ?? -1 }

    if parsed < 0 or ! rx"^[0-9a-fA-F]+$".matches(start_text) {
      gnu.error(f"invalid suffix length: {gnu.quote(start_text)}")
      exit 1
    }

    start = parsed
    widen = false
  }

  var width = 2
  var length_given = false

  if opts.suffix_length != "" {
    let parsed = parse_u64(opts.suffix_length)

    if parsed == null or parsed > 4096 {
      gnu.error(f"invalid suffix length: {gnu.quote(opts.suffix_length)}")
      exit 1
    }

    width = parsed
    length_given = true

    if width > 0 {
      widen = false
    }
  }

  if chunks.n > 0 {
    let required = digits_needed(start + chunks.n, radix)

    if start < chunks.n and ! (length_given and width > 0) {
      widen = false

      if width < required {
        width = required
      }
    }

    if width < required {
      gnu.error(f"the suffix length needs to be at least {required}")
      exit 1
    }
  }

  if length_given and width == 0 {
    width = 2
  }

  let prefix = if opts.files.len() > 1 { gnu.argument_bytes(opts.files[1], prepared.raw) } else { b"x" }
  let naming: Naming = Naming(prefix:, radix:, width:, start:, widen:, extra: additional)

  if ! widen and fixed_name(start, radix, width) == "" {
    gnu.error("numerical suffix start value is too large for the suffix length")
    exit 1
  }

  let input_name = if ! opts.files.is_empty() { gnu.argument_bytes(opts.files[0], prepared.raw) } else { b"-" }

  if opts.filter != null and input_name == b"-" {
    let command = opts.filter

    if size > 0 {
      filter_bytes_from_stdin(command, naming, size, opts.verbose)
      return
    }

    if chunks.kind == "r" {
      filter_round_robin_stdin(command, naming, chunks.n, sep, opts.elide, opts.verbose)
      return
    }
  }

  var input_ino = -1
  var input_dev = -1

  if input_name != b"-" {
    let input_path = Path.parse_bytes(input_name)?
    if let Ok(found) = fs.stat(input_path, follow_symlinks: true) {
      input_ino = found.ino
      input_dev = found.dev

      # Size-dependent chunks must reject virtual devices before reading:
      # a zero metadata size does not imply a finite or empty byte stream.
      if chunks.n > 0 and found.kind != "file" and input_name != b"/dev/null" {
        gnu.error(f"{gnu.quote_bytes(input_name, always: false)}: cannot determine file size")
        exit 1
      }
    }
  }

  guard let data = text.read_operand_bytes(input_name) else { |failure|
    if gnu.errno(failure) == 21 {
      gnu.error(f"error reading {gnu.quote_bytes(input_name)}: {gnu.strerror(failure)}")
    } else {
      gnu.error(f"cannot open {gnu.quote_bytes(input_name)} for reading: {gnu.strerror(failure)}")
    }

    exit 1
  }

  if input_name == b"-" and chunks.n > 0 and data.len() > blksize {
    gnu.error("-: cannot determine input size")
    exit 1
  }

  var pieces: List[Bytes] = []
  let ends = if chunks.kind in ["l", "r"] or (chunks.n == 0 and size == 0) { record_ends(data, sep)? } else { [] }

  if chunks.n > 0 {
    if chunks.kind == "bytes" {
      if chunks.k > 0 {
        pieces = [chunk_bytes(data, chunks.n, chunks.k)]
      } else {
        let limit = if opts.elide and chunks.n > data.len() { data.len() } else { chunks.n }

        for index in range(limit) {
          let piece = chunk_bytes(data, chunks.n, index + 1)

          if ! piece.is_empty() or ! opts.elide {
            pieces += [piece]
          }
        }
      }
    } else if chunks.kind == "l" {
      pieces = chunk_lines(data, ends, chunks.n, chunks.k, opts.elide)
    } else {
      pieces = round_robin(data, ends, chunks.n, chunks.k, opts.elide)
    }

    if opts.elide {
      pieces = [piece for piece in pieces if ! piece.is_empty()]
    }
  } else if size > 0 {
    pieces = bytes_pieces(data, size)
  } else if line_size > 0 {
    pieces = line_bytes_pieces(data, record_ends(data, sep)?, line_size, sep)
  } else {
    pieces = lines_pieces(data, ends, lines)
  }

  if chunks.k > 0 {
    for piece in pieces {
      gnu.write_bytes(piece)
    }

    return
  }

  # -u only changes how `-n r/...` output is buffered; every piece is written
  # as soon as it is complete, so there is nothing further to do for it.
  var made = 0

  for piece in pieces {
    let tail = suffix_name(made, naming)

    if tail == "" {
      gnu.error("output file suffixes exhausted")
      exit 1
    }

    let name = Path.parse_bytes(output_name(made, naming))?

    if input_ino >= 0 {
      if let Ok(found) = fs.stat(name, follow_symlinks: true) {
        if found.ino == input_ino and found.dev == input_dev {
          gnu.error(f"{gnu.quote_bytes(name.bytes())} would overwrite input; aborting")
          exit 1
        }
      }
    }

    if opts.verbose {
      gnu.write_text(f"creating file {gnu.quote_bytes(name.bytes())}\n")
    }

    if opts.filter != null {
      let command = opts.filter
      let plan = process.command_argv("sh", ["sh", "-c", command], p".", {FILE: name}, bytes.concat([piece]))
      let status = process.run(plan)?

      if (status.exit_code() ?? 0) != 0 {
        gnu.error(f"with filter '{command}': failed")
        exit 1
      }
    } else if let Err(failure) = name.write(piece) {
      if gnu.errno(failure) == 28 {
        text.name_error_bytes(name.bytes(), failure)
      } else {
        gnu.error(f"{gnu.quote_bytes(name.bytes())}: {gnu.strerror(failure)}")
      }

      exit 1
    }

    made += 1
  }
}
