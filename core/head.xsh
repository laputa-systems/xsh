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
  let elide = text.starts_with("-")
  let digits = if elide or text.starts_with("+") { text.byte_slice(1) } else { text }
  let value = tio.parse_count(digits)

  if value == null {
    gnu.error(f"invalid number of {what}: {gnu.quote(digits)}")
    exit 1
  }

  {value: value, elide: elide}
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
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
  let headers = opts.verbose or (operands.len() > 1 and ! opts.quiet)
  var first = true
  var failed = false

  for name in operands {
    let label = if name == "-" { "standard input" } else { name }

    guard let source = tio.open_source(name) else { |failure|
      gnu.cannot_open(name, failure)
      failed = true
      continue
    }

    var offset = 0
    var left = total
    var held = b""
    var shown = false

    loop {
      let step = if wants { tio.read_chunk(source, offset) } else { Ok(b"") }

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
          gnu.write_bytes(data[..data.len() - total])
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
          gnu.write_bytes(data[..cut])
          held = data[cut..]
        } else {
          held = data
        }
      } else if by_bytes {
        gnu.write_bytes(chunk[..left])
        left -= chunk.len()
        break when left <= 0
      } else {
        let ends = tio.line_ends(chunk, opts.zero)

        if left <= ends.len() {
          gnu.write_bytes(chunk[..ends[left - 1]])
          break
        }

        gnu.write_bytes(chunk)
        left -= ends.len()
      }
    }
  }

  if failed {
    exit 1
  }
}
