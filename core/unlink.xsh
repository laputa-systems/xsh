#!/bin/xsh
use lib.gnu

type Options = {help: Bool, version: Bool, operands: List[Str]}

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    operands: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: unlink FILE\nCall the unlink system call."); return }
  if opts.version { gnu.version("unlink"); return }
  if opts.operands.is_empty() { gnu.missing_operand() }
  if opts.operands.len() > 1 { gnu.extra_operand(opts.operands[1]) }
  if let Ok(meta) = fs.stat(fp"{opts.operands[0]}") {
    if meta.kind == "dir" { gnu.error(f"cannot unlink {gnu.quote(opts.operands[0])}: Is a directory"); exit 1 }
  }
  if let Err(failure) = fp"{opts.operands[0]}".unlink() {
    gnu.cannot("unlink", opts.operands[0], failure)
    exit 1
  }
}
