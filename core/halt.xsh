#!/bin/xsh
use lib.gnu
use lib.system_control as control

proc main(...argv: List[Str]) {
  let opts = cli.applet(argv, {
    gnu: {unsupported: {"-w": "shutdown accounting without shutdown is not available", "--wtmp-only": "shutdown accounting without shutdown is not available", "-d": "shutdown accounting policy is not available", "--no-wtmp": "shutdown accounting policy is not available", "-i": "pre-shutdown interface control is not available", "--no-wall": "service manager wall policy is not available"}},
    force: {form: "-f --force", default: false}, no_sync: {form: "-n --no-sync", default: false}, poweroff: {form: "-p --poweroff", default: false},
    help: {form: "--help", default: false, stop: true}, version: {form: "--version", default: false, stop: true}, operands: {form: "...ARG"},
  })?
  if opts.help { gnu.help("Usage: halt --force [--no-sync] [--poweroff]\nPerform the direct kernel operation; service manager shutdown is unavailable."); return }
  if opts.version { gnu.version("halt"); return }
  if ! opts.operands.is_empty() { gnu.extra_operand(opts.operands[0]) }
  control.power(if opts.poweroff { "poweroff" } else { "halt" }, opts.no_sync, opts.force)
}
