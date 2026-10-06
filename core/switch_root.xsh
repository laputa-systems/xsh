#!/bin/xsh
use lib.gnu

proc main(...argv: List[Str]) {
  let opts = cli.applet(argv, {gnu: {permute: false, unsupported: {"-c": "redirecting init to a console is not available", "--console": "redirecting init to a console is not available"}}, help: {form: "-h --help", default: false, stop: true}, version: {form: "-V --version", default: false, stop: true}, operands: {form: "...ARG"}})?
  if opts.help { gnu.help("Usage: switch_root NEW_ROOT INIT\nSwitch to a new root and execute init; init arguments are unavailable."); return }
  if opts.version { gnu.version("switch_root"); return }
  if opts.operands.len() < 2 { gnu.usage_error("expected new root and init") }
  if opts.operands.len() > 2 { gnu.usage_error("init arguments are not available") }
  linux.switch_root(fp"{opts.operands[0]}", fp"{opts.operands[1]}")
}
