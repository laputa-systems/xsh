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

# Attempt cwd first, preserving existing permissions and creating private output.
proc output_file() [fs, process, env, error] -> Path {
  let local = p"nohup.out"
  let attempted = unix.redirect_fd(1, local, write: true, append: true, mode: 384)
  if let Ok(_) = attempted { return local }
  let home = env.get_or("HOME", "")?
  if home == "" {
    if let Err(failure) = attempted {
      gnu.error(f"failed to open {gnu.quote(local.display())}: {gnu.strerror(failure)}")
    }
    exit 125
  }
  let fallback = fp"{home}/nohup.out"
  if let Err(failure) = unix.redirect_fd(1, fallback, write: true, append: true, mode: 384) {
    if let Err(local_failure) = attempted {
      gnu.error(f"failed to open {gnu.quote(local.display())}: {gnu.strerror(local_failure)}")
    }
    gnu.error(f"failed to open {gnu.quote(fallback.display())}: {gnu.strerror(failure)}")
    exit 125
  }
  fallback
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 125, permute: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    command: {form: "...COMMAND"},
  })?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("nohup"); return }
  if opts.command.is_empty() { gnu.missing_operand(125) }
  let input_terminal = unix.isatty(0)
  let output_terminal = unix.isatty(1)
  let error_terminal = unix.isatty(2)
  if input_terminal {
    if let Err(failure) = unix.redirect_fd(0, /dev/null, write: true) {
      gnu.error(f"failed to redirect standard input: {gnu.strerror(failure)}")
      exit 125
    }
  }
  if output_terminal {
    let destination = output_file()
    let prefix = if input_terminal { "ignoring input and appending output to" } else { "appending output to" }
    gnu.error(f"{prefix} {gnu.quote(destination.display())}")
  } else if input_terminal {
    gnu.error("ignoring input")
  }
  if error_terminal {
    if ! output_terminal { gnu.error("redirecting stderr to stdout") }
    unix.dup_fd(1, 2)?
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
