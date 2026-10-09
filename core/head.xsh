#!/bin/xsh
use lib.gnu
use lib.textio_a1 as tio

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

# A parsed NUM: `elide` is the leading `-` (all but the last NUM units).
type Count = {value: Int, elide: Bool}

pure raw_file_arguments(argv: List[Str], raw: List[Bytes]) -> List[Bytes] {
  var files: List[Bytes] = []
  var options = true
  var skip_value = false

  for index in range(argv.len()) {
    let argument = argv[index]

    if skip_value {
      skip_value = false
      continue
    }

    if options and argument == "--" {
      options = false
      continue
    }

    if options and argument in ["-n", "--lines", "-c", "--bytes"] {
      skip_value = true
      continue
    }

    if options and (argument.starts_with("--lines=") or argument.starts_with("--bytes=")) {
      continue
    }

    if options and (argument == "--presume-input-pipe" or (argument != "-" and argument.starts_with("-"))) {
      continue
    }

    files += [raw[index]]
  }

  files
}

# Rewrite the obsolete first argument `-NUM[bkm][cqvz]...` into the options it
# stands for.
pure modernize(argv: List[Str]) -> List[Str] {
  guard argv.len() > 0 else {
    return argv
  }

  let parts = rx"^-([0-9]+[bkm]?)([cqvz]*)$".captures(argv[0])

  return argv when parts.len() == 0

  let flags = parts[2]
  let unit = if flags.find("c") != null { "-c" } else { "-n" }
  let extra = [f"-{letter}" for letter in ["q", "v", "z"] if flags.find(letter) != null]

  [unit, parts[1], @extra, @argv[1..]]
}

proc parse_count(text: Str, what: Str) [process, env] -> Count {
  let normalized = if text.starts_with("=") { text.byte_slice(1) } else { text }
  let elide = normalized.starts_with("-")
  let digits = if elide or normalized.starts_with("+") { normalized.byte_slice(1) } else { normalized }
  let value = tio.parse_count(digits)

  if value == null {
    gnu.error(f"invalid number of {what}: {gnu.quote_value(digits)}")
    exit 1
  }

  {value: value, elide: elide}
}

proc write_output(data: Bytes) [process, env, io] {
  if let Err(failure) = io.write_stdout_bytes(data) {
    if gnu.errno(failure) == 32 {
      exit 141
    }

    gnu.error(f"error writing 'standard output': {gnu.strerror(failure)}")
    exit 1
  }
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let raw_files = raw_file_arguments(argv, cli.argv_bytes())
  let opts: HeadOptions = cli.applet(
    modernize(tio.without_presume_pipe(argv)),
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
  let operands = if opts.files.len() == 0 { ["-"] } else { opts.files }
  let raw_operands = if opts.files.len() == 0 { [b"-"] } else { raw_files }
  let headers = opts.verbose or (operands.len() > 1 and ! opts.quiet)
  var first = true
  var failed = false

  for index in range(operands.len()) {
    let name = operands[index]
    let label = if name == "-" { "standard input" } else { name }

    guard let source = tio.open_source_path(name, Path.parse_bytes(raw_operands[index])?) else { |failure|
      gnu.cannot_open(name, failure)
      failed = true
      continue
    }

    var offset = 0
    var left = total
    var held = b""
    var shown = false

    loop {
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
          gnu.cannot_open(name, failure)
          failed = true
          break
        }

        if headers and ! shown {
          gnu.write_text(f"{if first { "" } else { "\n" }}==> {gnu.quote_maybe(label)} <==\n")
          first = false
        }

        gnu.error_reading(label, failure)
        failed = true
        break
      }

      if headers and ! shown {
        gnu.write_text(f"{if first { "" } else { "\n" }}==> {gnu.quote_maybe(label)} <==\n")
        first = false
      }

      shown = true

      break when chunk.len() == 0

      offset += chunk.len()

      if elide and by_bytes {
        let data = bytes.concat([held, chunk])

        if data.len() > total {
          write_output(data[..data.len() - total])
          held = data[data.len() - total..]
        } else {
          held = data
        }
      } else if elide {
        let data = bytes.concat([held, chunk])
        let ends = tio.line_ends(data, opts.zero)
        let lines = ends.len() + (if ends.len() > 0 and ends[ends.len() - 1] == data.len() { 0 } else { 1 })

        if lines > total {
          let cut = ends[lines - total - 1]
          write_output(data[..cut])
          held = data[cut..]
        } else {
          held = data
        }
      } else if by_bytes {
        let take = if left < chunk.len() { left } else { chunk.len() }
        write_output(chunk[..take])
        left -= chunk.len()
        break when left <= 0
      } else {
        let ends = tio.line_ends(chunk, opts.zero)

        if left <= ends.len() {
          write_output(chunk[..ends[left - 1]])
          break
        }

        write_output(chunk)
        left -= ends.len()
      }
    }

    if source.mode == "stdin" and elide and held.len() > 0 {
      let _ = io.stdin_seek_relative(-held.len())?
    }
  }

  if failed {
    exit 1
  }
}
