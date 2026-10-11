#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio
use unix

const USAGE = """Usage: head [OPTION]... [FILE]...
Print the first 10 lines of each FILE to standard output.
With more than one FILE, precede each with a header giving the file name.

With no FILE, or when FILE is -, read standard input.

  -c, --bytes=[-]NUM       print the first NUM bytes of each file;
                             with the leading '-', print all but the last
                             NUM bytes of each file
  -n, --lines=[-]NUM       print the first NUM lines instead of the first 10;
                             with the leading '-', print all but the last
                             NUM lines of each file
  -q, --quiet, --silent    never print headers giving file names
  -v, --verbose            always print headers giving file names
  -z, --zero-terminated    line delimiter is NUL, not newline
      --help        display this help and exit
      --version     output version information and exit

NUM may have a multiplier suffix:
b 512, kB 1000, K 1024, MB 1000*1000, M 1024*1024,
GB 1000*1000*1000, G 1024*1024*1024, and so on for T, P, E, Z, Y, R, Q.
Binary prefixes can be used, too: KiB=K, MiB=M, and so on.
"""

type HeadOptions = {
  lines: Str,
  bytes: Str,
  quiet: Bool,
  verbose: Bool,
  zero: Bool,
  help: Bool,
  version: Bool,
  files: List[Str],
}
type RawArgument = {marker: Str, value: Bytes}
type PreparedArguments = {text: List[Str], raw: List[RawArgument]}

# Keep raw operands for paths while giving the text option parser safe markers.
pure prepare_arguments(argv: List[Bytes]) -> PreparedArguments {
  var text: List[Str] = []
  var raw: List[RawArgument] = []

  for index in range(argv.len()) {
    let argument = argv[index]
    match argument.utf8() {
      Ok(value) => text += [value]
      Err(_) => {
        let marker = f"\0head-raw-argument-{index}\0"
        text += [marker]
        raw += [{marker: marker, value: argument}]
      }
    }
  }

  {text: text, raw: raw}
}

pure argument_bytes(value: Str, raw: List[RawArgument]) -> Bytes {
  for argument in raw {
    if argument.marker == value { return argument.value }
  }

  bytes.from_text(value)
}

proc source_for(name: Bytes) [fs, error] -> Result[tio.Source, Error] {
  if let Ok(text) = name.utf8() {
    return tio.open_source(text)
  }

  let input_path = Path.parse_bytes(name)?
  let target = input_path.resolve()?

  tio.source_of(target.display(), target)
}

proc report_cannot_open(name: Bytes, failure: Error) [process, env] {
  if let Ok(text) = name.utf8() {
    gnu.cannot_open(text, failure)
  } else {
    gnu.error(f"cannot open {gnu.quote_bytes(name, always: false)} for reading: {gnu.strerror(failure)}")
  }
}

# A parsed NUM: `elide` is the leading `-` (all but the last NUM units).
type Count = {value: Int, elide: Bool}

# Rewrite the obsolete first argument `-NUM[FLAGS]` into the options it stands
# for. The letters are read in order, as GNU does: `c` selects bytes and drops a
# multiplier, `b`, `k`, or `m` select bytes with that multiplier, `l` selects
# lines again, and the last of `q` or `v` wins. Any other letter is a usage error.
proc modernize(argv: List[Str]) [process, env] -> List[Str] {
  guard ! argv.is_empty() else {
    return argv
  }

  let parts = rx"^-([0-9]+)(.*)$".captures(argv[0])

  return argv when parts.is_empty()

  var by_lines = true
  var multiplier = ""
  var header = ""
  var zero = false

  for letter in parts[2] {
    match letter {
      "c" => {
        by_lines = false
        multiplier = ""
      }
      "b" | "k" | "m" => {
        by_lines = false
        multiplier = letter
      }
      "l" => by_lines = true
      "q" | "v" => header = letter
      "z" => zero = true
      else => gnu.usage_error(f"invalid trailing option -- {letter}")
    }
  }

  let unit = if by_lines { "-n" } else { "-c" }
  let extra = [@if header != "" { [f"-{header}"] } else { [] }, @if zero { ["-z"] } else { [] }]

  [unit, f"{parts[1]}{multiplier}", @extra, @argv[1..]]
}

# A write failure on standard output is reported with head's own wording; a
# closed reader ends the applet with the SIGPIPE status.
proc output_failed(failure: Error, closing = false) [process, env] {
  if gnu.errno(failure) == 32 {
    exit 141
  }

  gnu.error(if closing { f"write error: {gnu.strerror(failure)}" } else { f"error writing {gnu.quote("standard output")}: {gnu.strerror(failure)}" })
  exit 1
}

# Keep a bounded stdio-sized buffer so errors while writing and errors while
# closing standard output retain their distinct GNU diagnostics.
proc write_output(pending: Bytes, data: Bytes) [process, env, error, io] -> Bytes {
  let output = bytes.concat([pending, data])
  if output.len() < 4096 { return output }

  if let Err(failure) = io.write_stdout_bytes(output) {
    output_failed(failure)
  }
  if let Err(failure) = io.flush_stdout() {
    output_failed(failure)
  }
  b""
}

proc close_output(pending: Bytes) [process, env, error, io] {
  if let Err(failure) = io.write_stdout_bytes(pending) {
    output_failed(failure, closing: true)
  }
  if let Err(failure) = io.flush_stdout() {
    output_failed(failure, closing: true)
  }
}

proc parse_count(text: Str, what: Str) [process, env] -> Count {
  let elide = text.starts_with("-")
  let digits = if elide or text.starts_with("+") { text.byte_slice(1) } else { text }
  let value = tio.parse_count(digits)

  if value == null {
    gnu.error(f"invalid number of {what}: {gnu.quote_value(digits)}")
    exit 1
  }

  {value: value, elide: elide}
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  let prepared = prepare_arguments(argv)
  let opts: HeadOptions = cli.applet(
    modernize(tio.without_presume_pipe(prepared.text)),
    {
      gnu: {status: 1},
      lines: {form: "-n --lines N", default: "", conflicts: ["bytes"]},
      bytes: {form: "-c --bytes N", default: "", conflicts: ["lines"]},
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
    gnu.version("head")
    return
  }

  let by_bytes = opts.bytes != ""
  let spec = if by_bytes {
    parse_count(opts.bytes, "bytes")
  } else {
    parse_count(if opts.lines == "" { "10" } else { opts.lines }, "lines")
  }
  let elide = spec.elide and spec.value > 0
  let total = if spec.elide and spec.value == 0 { tio.MAX_COUNT } else { spec.value }
  let wants = elide or total > 0
  let operands = if opts.files.is_empty() { ["-"] } else { opts.files }
  let headers = opts.verbose or (operands.len() > 1 and ! opts.quiet)
  var first = true
  var failed = false
  var pending = b""

  for name in operands {
    let raw_name = argument_bytes(name, prepared.raw)
    let label = if raw_name == b"-" {
      "standard input"
    } else if let Ok(text) = raw_name.utf8() {
      text
    } else {
      Path.parse_bytes(raw_name)?.display()
    }

    guard let source = source_for(raw_name) else { |failure|
      report_cannot_open(raw_name, failure)
      failed = true
      continue
    }

    var offset = 0
    var left = total
    var held = b""
    var shown = false

    loop {
      # Standard input is read only as far as the output needs, so the bytes after
      # the printed part stay unread for the next reader of the same descriptor.
      # Elided output must read to the end, and is rewound afterwards instead.
      let count = if source.mode == "stdin" and ! elide {
        if by_bytes {
          if left < tio.CHUNK { left } else { tio.CHUNK }
        } else {
          1
        }
      } else {
        tio.CHUNK
      }
      let step = if wants { tio.read_chunk(source, offset, count) } else { Ok(b"") }

      guard let chunk = step else { |failure|
        if offset == 0 and ! tio.is_directory(failure) {
          report_cannot_open(raw_name, failure)
          failed = true
          break
        }

        if headers and ! shown {
          pending = write_output(pending, bytes.from_text(f"{if first { "" } else { "\n" }}==> {gnu.quote_maybe(label)} <==\n"))
          first = false
        }

        gnu.error_reading(label, failure)
        failed = true
        break
      }

      if headers and ! shown {
        pending = write_output(pending, bytes.from_text(f"{if first { "" } else { "\n" }}==> {gnu.quote_maybe(label)} <==\n"))
        first = false
      }

      shown = true

      break when chunk.is_empty()

      offset += chunk.len()

      if elide and by_bytes {
        let data = bytes.concat([held, chunk])

        if data.len() > total {
          pending = write_output(pending, data[..data.len() - total])
          held = data[data.len() - total..]
        } else {
          held = data
        }
      } else if elide {
        let data = bytes.concat([held, chunk])
        let ends = tio.line_ends(data, opts.zero)
        let lines = ends.len() + (if ! ends.is_empty() and ends[-1] == data.len() { 0 } else { 1 })

        if lines > total {
          let cut = ends[lines - total - 1]
          pending = write_output(pending, data[..cut])
          held = data[cut..]
        } else {
          held = data
        }
      } else if by_bytes {
        pending = write_output(pending, chunk[..left])
        left -= chunk.len()
        break when left <= 0
      } else {
        let ends = tio.line_ends(chunk, opts.zero)

        if left <= ends.len() {
          pending = write_output(pending, chunk[..ends[left - 1]])
          break
        }

        pending = write_output(pending, chunk)
        left -= ends.len()
      }
    }

    if elide and source.mode == "stdin" and held.len() > 0 and tio.standard_file(0) != "" {
      let _ = unix.seek_fd(0, tio.stdin_offset()? - held.len())?
    }
  }

  close_output(pending)

  if failed {
    exit 1
  }
}
