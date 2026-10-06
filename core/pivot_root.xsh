#!/bin/xsh
use lib.gnu

type Options = {help: Bool, version: Bool, operands: List[Str]}

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(argv, {gnu: {}, help: {form: "-h --help", default: false, stop: true}, version: {form: "-V --version", default: false, stop: true}, operands: {form: "...PATH"}})?
  if opts.help { gnu.help("Usage: pivot_root NEW_ROOT PUT_OLD\nChange the root mount in the caller's mount namespace."); return }
  if opts.version { gnu.version("pivot_root"); return }
  if opts.operands.len() != 2 { gnu.usage_error("expected new root and old root directory") }
  linux.pivot_root(fp"{opts.operands[0]}", fp"{opts.operands[1]}")
}
