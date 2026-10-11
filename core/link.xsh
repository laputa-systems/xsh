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
  if opts.help { gnu.help("Usage: link FILE1 FILE2\nCall the link system call."); return }
  if opts.version { gnu.version("link"); return }
  if opts.operands.is_empty() { gnu.missing_operand() }
  if opts.operands.len() == 1 { gnu.usage_error(f"missing operand after {gnu.quote_value(opts.operands[0])}") }
  if opts.operands.len() > 2 { gnu.usage_error(f"extra operand {gnu.quote_value(opts.operands[2])}") }
  if let Err(failure) = fs.link(fp"{opts.operands[0]}", fp"{opts.operands[1]}") {
    gnu.error(f"cannot create link {gnu.quote(opts.operands[1])} to {gnu.quote(opts.operands[0])}: {gnu.strerror(failure)}")
    exit 1
  }
}
