#!/bin/xsh
use lib.gnu
use lib.system_control as control

proc main(...argv: List[Str]) -> Result[Unit] {
  let opts = cli.applet(argv, {
    gnu: {permute: false, unsupported: {"-c": "acquiring a controlling terminal is not available", "--ctty": "acquiring a controlling terminal is not available"}},
    fork: {form: "-f --fork", default: false}, wait: {form: "-w --wait", default: false},
    help: {form: "-h --help", default: false, stop: true}, version: {form: "-V --version", default: false, stop: true}, operands: {form: "...COMMAND"},
  })?
  if opts.help { gnu.help("Usage: setsid [-f] [-w] COMMAND [ARG...]\nRun a command in a new session."); return }
  if opts.version { gnu.version("setsid"); return }
  if opts.operands.is_empty() { gnu.usage_error("missing command") }
  let direct_command = control.command(opts.operands)?
  if ! opts.fork {
    match process.new_session() {
      Ok(_) => { unix.exec(direct_command); return }
      Err(failure) => { if gnu.errno(failure) != 1 { return Err(failure) } }
    }
  }
  let command = control.command(opts.operands, new_session: true, detach: ! opts.wait)?
  if opts.wait {
    let child = spawn command?
    let status = wait child?
    exit control.status_code(status)?
  }
  let _ = process.spawn(command)?
  return
}
