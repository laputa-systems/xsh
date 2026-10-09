#!/bin/xsh
use lib.gnu

const USAGE = """Usage: dirname [OPTION]... NAME...
Output each NAME with its last non-slash component and trailing slashes removed.

  -z, --zero  end each output line with NUL, not newline
      --help  display this help and exit
      --version  output version information and exit
"""

type DirnameOptions = {zero: Bool, help: Bool, version: Bool, names: List[Str]}

pure raw_names(argv: List[Str], raw: List[Bytes]) -> List[Bytes] {
  var names: List[Bytes] = []
  var options = true
  for index in range(argv.len()) {
    let arg = argv[index]
    if options and arg == "--" { options = false; continue }
    if options and (arg == "-z" or arg == "--zero" or arg == "--help" or arg == "--version") { continue }
    if options and arg.starts_with("-") and arg != "-" { continue }
    names += [raw[index]]
  }
  names
}

pure byte_is(text: Bytes, at: Int, expected: Int) -> Bool {
  (text.byte_at(at) ?? -1) == expected
}

pure dirname_value(name: Bytes) -> Bytes {
  return bytes.from_text(".") when name.len() == 0

  let total = name.len()
  var end = total

  while end > 0 and byte_is(name, end - 1, 47) {
    end -= 1
  }

  return bytes.from_text("/") when end == 0

  # GNU's dirname treats a final `/.` as a trailing component and preserves
  # preceding `/.` components as ordinary text.
  if end >= 2 and byte_is(name, end - 1, 46) {
    var dot = end - 1
    if byte_is(name, dot - 1, 47) {
      while dot > 1 and byte_is(name, dot - 2, 47) {
        dot -= 1
      }
      end = dot - 1
      while end > 1 and byte_is(name, end - 1, 47) {
        end -= 1
      }
      return bytes.from_text("/") when end == 0
      return name.slice(0, length: end) when end > 0
    }
  }

  var slash = -1
  var at = 0
  while at < end {
    if byte_is(name, at, 47) {
      slash = at
    }
    at += 1
  }

  return bytes.from_text(".") when slash < 0
  return bytes.from_text("/") when slash == 0

  var parent_end = slash
  while parent_end > 1 and byte_is(name, parent_end - 1, 47) {
    parent_end -= 1
  }

  name.slice(0, length: parent_end)
}

proc main(...argv: List[Str]) [process, env, error, io] {
  let opts: DirnameOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      zero: {form: "-z --zero", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      names: {form: "...NAME"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }
  if opts.version {
    gnu.version("dirname")
    return
  }
  let names = raw_names(argv, cli.argv_bytes())
  if opts.names.len() == 0 {
    gnu.missing_operand()
  }

  let ending = bytes.from_text(if opts.zero { "\0" } else { "\n" })
  for name in names {
    gnu.write_bytes(bytes.concat([dirname_value(name), ending]))
  }
}
