#!/bin/xsh
use lib.gnu

type Options = {bsd: Bool, sysv: Bool, help: Bool, version: Bool, files: List[Str]}
type RawArgument = {marker: Str, value: Bytes}
type PreparedArguments = {text: List[Str], raw: List[RawArgument]}

# The shared option parser takes text, so map undecodable operands through
# NUL-marked placeholders; OS arguments cannot contain NUL and cannot collide.
pure prepare_arguments(argv: List[Bytes]) -> PreparedArguments {
  var text = []
  var raw: List[RawArgument] = []
  for index in range(argv.len()) {
    let argument = argv[index]
    if let Ok(value) = argument.utf8() {
      text += [value]
    } else {
      let marker = f"\0sum-raw-argument-{index}\0"
      text += [marker]
      raw += [{marker: marker, value: argument}]
    }
  }
  {text: text, raw: raw}
}

pure argument_bytes(value: Str, raw: List[RawArgument]) -> Bytes {
  for argument in raw {
    return argument.value when argument.marker == value
  }
  bytes.from_text(value)
}

pure padded(value: Int, width: Int, fill: Str) -> Str {
  var result = f"{value}"
  while result.byte_len() < width { result = fill + result }
  result
}

proc main(...argv: List[Bytes]) [fs, io, error, process, env] {
  let prepared = prepare_arguments(argv)
  let opts: Options = cli.applet(prepared.text, {
    gnu: {status: 1},
    bsd: {form: "-r", default: false},
    sysv: {form: "-s --sysv", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    files: {form: "...FILE"},
  })?
  if opts.help {
    gnu.help("Usage: sum [OPTION]... [FILE]...\nPrint checksum and block counts for each FILE.\n  -r       BSD checksum, 1024-byte blocks (default)\n  -s, --sysv  System V checksum, 512-byte blocks\nWith no FILE or when FILE is -, read standard input.")
    return
  }
  if opts.version { gnu.version("sum"); return }
  var sysv = false
  for arg in argv {
    break when arg == b"--"
    if arg == b"--sysv" { sysv = true }
    if arg.starts_with(b"-") and !arg.starts_with(b"--") {
      for index in range(1, arg.len()) {
        let flag = arg.byte_at(index)
        if flag == 115 { sysv = true }
      }
    }
  }
  let algorithm = if sysv { "sysv" } else { "bsd" }
  var files = []
  for file in opts.files { files += [argument_bytes(file, prepared.raw)] }
  if files.is_empty() { files = [b"-"] }
  var failed = false
  for name in files {
    let result = if name == b"-" { hash.checksum_stdin(algorithm) } else { hash.checksum(name as Path, algorithm) }
    match result {
      Ok(value) => {
        let block_size = if sysv { 512 } else { 1024 }
        let blocks = (value.size + block_size - 1) / block_size
        let suffix = if name == b"-" { b"" } else { bytes.concat([b" ", name]) }
        let checksum = if sysv { f"{value.checksum}" } else { padded(value.checksum, 5, "0") }
        let count = if sysv { f"{blocks}" } else { padded(blocks, 5, " ") }
        gnu.write_bytes(bytes.concat([bytes.from_text(f"{checksum} {count}"), suffix, b"\n"]))
      }
      Err(failure) => { gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}"); failed = true }
    }
  }
  if failed { exit 1 }
}
