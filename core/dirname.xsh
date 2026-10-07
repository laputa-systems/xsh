#!/bin/xsh
use lib.gnu

type Options = {zero: Bool, help: Bool, version: Bool, paths: List[Str]}
type RawArgument = {marker: Str, value: Bytes}
type PreparedArguments = {text: List[Str], raw: List[RawArgument]}

# The shared option parser takes text; NUL-marked operands preserve Unix argv bytes.
pure prepare_arguments(argv: List[Bytes]) -> PreparedArguments {
  var text: List[Str] = []
  var raw: List[RawArgument] = []
  for index in range(argv.len()) {
    let argument = argv[index]
    match argument.utf8() {
      Ok(value) => text += [value]
      Err(_) => {
        let marker = f"\0dirname-raw-argument-{index}\0"
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

# dirname removes components lexically, retaining dots and repeated interior slashes.
pure dirname_value(name: Bytes) -> Bytes {
  var end = name.len()
  while end > 0 and name.byte_at(end - 1) == 47 { end -= 1 }
  return b"/" when end == 0 and name != b""
  while end > 0 and name.byte_at(end - 1) != 47 { end -= 1 }
  return b"." when end == 0
  while end > 0 and name.byte_at(end - 1) == 47 { end -= 1 }
  if end == 0 { b"/" } else { name.slice(0, length: end) }
}

proc main(...argv: List[Bytes]) [process, env, error, io] {
  let prepared = prepare_arguments(argv)
  let opts: Options = cli.applet(prepared.text, {
    gnu: {status: 1},
    zero: {form: "-z --zero", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...NAME"},
  })?
  if opts.help { gnu.help("Usage: dirname [OPTION] NAME...\nPrint NAME with its last component removed.\n  -z, --zero  end output with NUL\n"); return }
  if opts.version { gnu.version("dirname"); return }
  if opts.paths.is_empty() { gnu.missing_operand() }
  for name in opts.paths {
    let ending = if opts.zero { b"\0" } else { b"\n" }
    gnu.write_bytes(bytes.concat([dirname_value(argument_bytes(name, prepared.raw)), ending]))
  }
}
