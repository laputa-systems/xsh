#!/bin/xsh
use lib.gnu
use lib.proc_launch

const USAGE = """Usage: nohup COMMAND [ARG]...
  or:  nohup OPTION
Run COMMAND, ignoring hangup signals.
      --help     display this help and exit
      --version  output version information and exit
If standard input is a terminal, replace it with an unreadable descriptor.
If standard output is a terminal, append to nohup.out or $HOME/nohup.out.
If standard error is a terminal, redirect it to standard output.
"""

type Options = {help: Bool, version: Bool, command: List[Str]}

# An advisory must reach the original stderr before exec replaces this process.
# Retrying a failed write after redirecting stderr could pollute command output.
proc advisory(message: Str, failure_status: Int) [io, process] {
  if io.write_stderr(f"{gnu.prog()}: {message}\n") is Err(_) { exit failure_status }
  if io.flush_stderr() is Err(_) { exit failure_status }
}

# Attempt cwd first, preserving existing permissions and creating private output.
proc output_file(failure_status: Int) [fs, process, env, error] -> Path {
  let local = p"nohup.out"
  let attempted = unix.redirect_fd(1, local, write: true, append: true, mode: 384)
  if let Ok(_) = attempted { return local }
  let home = env.get_or("HOME", "")?
  if home == "" {
    if let Err(failure) = attempted {
      gnu.error(f"failed to open {gnu.quote(local.display())}: {gnu.strerror(failure)}")
    }
    exit failure_status
  }
  let fallback = fp"{home}/nohup.out"
  if let Err(failure) = unix.redirect_fd(1, fallback, write: true, append: true, mode: 384) {
    if let Err(local_failure) = attempted {
      gnu.error(f"failed to open {gnu.quote(local.display())}: {gnu.strerror(local_failure)}")
    }
    gnu.error(f"failed to open {gnu.quote(fallback.display())}: {gnu.strerror(failure)}")
    exit failure_status
  }
  fallback
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let failure_status = if let Ok(_) = env.get("POSIXLY_CORRECT") { 127 } else { 125 }
  let opts: Options = cli.applet(argv, {
    gnu: {status: failure_status, permute: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    command: {form: "...COMMAND"},
  })?.require(Options)?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("nohup"); return }
  if opts.command.is_empty() { gnu.missing_operand(failure_status) }
  let input_terminal = unix.isatty(0)
  let output_terminal = unix.isatty(1)
  let error_terminal = unix.isatty(2)
  if input_terminal {
    if let Err(failure) = unix.redirect_fd(0, /dev/null, write: true) {
      gnu.error(f"failed to render standard input unusable: {gnu.strerror(failure)}")
      exit failure_status
    }
  }
  if output_terminal {
    let destination = output_file(failure_status)
    let prefix = if input_terminal { "ignoring input and appending output to" } else { "appending output to" }
    advisory(f"{prefix} {gnu.quote(destination.display())}", failure_status)
  } else if input_terminal and ! error_terminal {
    advisory("ignoring input", failure_status)
  }
  if error_terminal {
    if ! output_terminal {
      let prefix = if input_terminal { "ignoring input and redirecting" } else { "redirecting" }
      advisory(f"{prefix} standard error to standard output", failure_status)
    }
    if let Err(failure) = unix.dup_fd(1, 2) {
      gnu.error(f"failed to redirect standard error: {gnu.strerror(failure)}")
      exit failure_status
    }
  }
  process.set_signal_action("HUP", "ignore")?
  proc_launch.check_command(opts.command[0])
  io.flush_stdout()?
  let plan = process.command_argv(opts.command[0], opts.command)
  if let Err(failure) = unix.exec(plan) {
    gnu.error(f"failed to run command {gnu.quote(opts.command[0])}: {gnu.strerror(failure)}")
    exit 126
  }
}
