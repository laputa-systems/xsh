#!/bin/xsh
use lib.gnu
use lib.system_control as control

proc main(...argv: List[Str]) {
  let opts = cli.applet(argv, {
    gnu: {permute: false, unsupported: {"-u": "unlocking an inherited descriptor is not available", "--unlock": "unlocking an inherited descriptor is not available", "-w": "monotonic lock timeouts are not available", "--timeout": "monotonic lock timeouts are not available", "-F": "lock inheritance across exec is not available", "--no-fork": "lock inheritance across exec is not available", "--fcntl": "open-file-description locking is not available", "--verbose": "lock timing reports are not available"}},
    shared: {form: "-s --shared", default: false}, exclusive: {form: "-x -e --exclusive", default: false}, nonblock: {form: "-n --nonblock", default: false}, conflict: {form: "-E --conflict-exit-code CODE", default: "1"}, shell: {form: "-c --command COMMAND"},
    help: {form: "-h --help", default: false, stop: true}, version: {form: "-V --version", default: false, stop: true}, operands: {form: "...ARG"},
  })?
  if opts.help { gnu.help("Usage: flock [-s|-x] [-n] [-E CODE] FILE COMMAND [ARG...]\n       flock [-s|-x] [-n] -c COMMAND FILE\nHold a pathname lock while a child command runs."); return }
  if opts.version { gnu.version("flock"); return }
  if opts.shared and opts.exclusive { gnu.usage_error("--shared and --exclusive are mutually exclusive") }
  let conflict = opts.conflict.parse_int() ?? -1
  if conflict < 0 or conflict > 255 { gnu.usage_error("conflict exit code must be 0 through 255") }
  if opts.operands.is_empty() { gnu.usage_error("missing lock file") }
  if rx"^[0-9]+$".matches(opts.operands[0]) and opts.operands.len() == 1 { gnu.usage_error("locking an inherited descriptor is not available") }
  if opts.shell != null and opts.operands.len() != 1 { gnu.usage_error("--command requires exactly one lock file") }
  if opts.shell == null and opts.operands.len() < 2 { gnu.usage_error("missing command") }
  let child_argv = if opts.shell != null { ["/bin/sh", "-c", opts.shell ?? ""] } else if opts.operands[1] in ["-c", "--command"] {
    if opts.operands.len() != 3 { gnu.usage_error("--command requires one shell command") }
    ["/bin/sh", "-c", opts.operands[2]]
  } else { opts.operands[1..] }
  let command = control.command(child_argv)?
  let lock = match fs.lock(fp"{opts.operands[0]}", shared: opts.shared, nonblocking: opts.nonblock) {
    Ok(held) => held
    Err(failure) => { if opts.nonblock and gnu.errno(failure) in [11, 35] { exit conflict }; return Err(failure) }
  }
  defer fs.unlock(lock)
  exit control.status_code(process.run(command)?)?
}
