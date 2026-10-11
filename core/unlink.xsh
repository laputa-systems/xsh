#!/bin/xsh
use lib.gnu

type Options = {help: Bool, version: Bool, operands: List[Str]}

proc main(...argv: List[Bytes]) [fs, error, process, env, io] {
  let prepared = gnu.prepare_arguments(argv)
  let opts: Options = cli.applet(prepared.text, {
    gnu: {status: 1},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    operands: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: unlink FILE\nCall the unlink system call."); return }
  if opts.version { gnu.version("unlink"); return }
  if opts.operands.is_empty() { gnu.missing_operand() }
  if opts.operands.len() > 1 {
    let extra = gnu.argument_bytes(opts.operands[1], prepared.raw)
    gnu.usage_error(f"extra operand {gnu.quote_bytes(extra)}")
  }
  let operand = gnu.argument_bytes(opts.operands[0], prepared.raw)
  let target = Path.parse_bytes(operand)?
  if let Ok(meta) = fs.stat(target) {
    if meta.kind == "dir" { gnu.error(f"cannot unlink {gnu.quote_bytes(operand)}: Is a directory"); exit 1 }
  }
  if let Err(failure) = target.unlink() {
    gnu.error(f"cannot unlink {gnu.quote_bytes(operand)}: {gnu.strerror(failure)}")
    exit 1
  }
}
