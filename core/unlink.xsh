#!/bin/xsh
use lib.gnu

type Options = {help: Bool, version: Bool, operands: List[Str]}
type RawArgument = {marker: Str, value: Bytes}
type PreparedArguments = {text: List[Str], raw: List[RawArgument]}

pure prepare_arguments(argv: List[Bytes]) -> PreparedArguments {
  var text: List[Str] = []
  var raw: List[RawArgument] = []
  for index in range(argv.len()) {
    let argument = argv[index]
    match argument.utf8() {
      Ok(value) => text += [value]
      Err(_) => {
        let marker = f"\0unlink-raw-argument-{index}\0"
        text += [marker]
        raw += [{marker: marker, value: argument}]
      }
    }
  }
  {text: text, raw: raw}
}

pure argument_bytes(value: Str, raw: List[RawArgument]) -> Bytes {
  for argument in raw { if argument.marker == value { return argument.value } }
  bytes.from_text(value)
}

proc main(...argv: List[Bytes]) [fs, error, process, env, io] {
  let prepared = prepare_arguments(argv)
  let opts: Options = cli.applet(prepared.text, {
    gnu: {status: 1},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    operands: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: unlink FILE\nCall the unlink system call."); return }
  if opts.version { gnu.version("unlink"); return }
  let operands: List[Bytes] = collect { for operand in opts.operands { yield argument_bytes(operand, prepared.raw) } }
  if operands.is_empty() { gnu.usage_error("missing operand\nUsage: unlink FILE") }
  if operands.len() > 1 { gnu.usage_error(f"extra operand {gnu.quote_bytes(operands[1])}\nUsage: unlink FILE") }
  let target = Path.parse_bytes(operands[0])?
  if let Ok(meta) = fs.stat(target) {
    if meta.kind == "dir" { gnu.error(f"cannot unlink {gnu.quote_bytes(operands[0])}: Is a directory"); exit 1 }
  }
  if let Err(failure) = target.unlink() {
    gnu.error(f"cannot unlink {gnu.quote_bytes(operands[0])}: {gnu.strerror(failure)}")
    exit 1
  }
}
