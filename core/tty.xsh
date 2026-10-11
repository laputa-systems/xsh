#!/bin/xsh
use lib.gnu

const USAGE = """Usage: tty [OPTION]...
Print the file name of the terminal connected to standard input.

  -s, --silent, --quiet
         print nothing, only return an exit status
      --help     display this help and exit
      --version  output version information and exit
"""

type TtyOptions = {silent: Bool, help: Bool, version: Bool}

# Exit statuses: 0 on a terminal, 1 not a terminal, 2 usage, 3 ttyname error.
# A failed stdout write ends with status 3 when SIGPIPE is ignored; otherwise
# the signal terminates tty before the write returns.
proc write_failed(failure: Error) [process, env] -> Unit {
  gnu.error(f"write error: {gnu.strerror(failure)}")

  exit 3
}

proc write_result(text: Str) [process, env, io] -> Unit {
  if let Err(failure) = io.write_stdout(text) {
    write_failed(failure)
  }
  if let Err(failure) = io.flush_stdout() {
    write_failed(failure)
  }
}

proc main(...argv: List[Str]) [process, env, error, io] {
  # The runtime ignores SIGPIPE for its own writes. Restore the caller's default
  # action so a closed reader terminates tty by signal; preserve an explicit ignore.
  if process.signal_action("PIPE")? == "default" {
    process.set_signal_action("PIPE", "default")?
  }

  let opts: TtyOptions = cli.applet(
    argv,
    {
      gnu: {status: 2},
      silent: {form: "-s --silent --quiet", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("tty")
    return
  }

  let terminal = if let Ok(_) = unix.tty_attrs(0) { true } else { false }

  if opts.silent {
    exit if terminal { 0 } else { 1 }
  }

  match unix.tty() {
    Ok(name) => write_result(f"{name}\n")
    Err(failure) => {
      if terminal {
        gnu.error(f"ttyname error: {gnu.strerror(failure)}")
        exit 3
      }

      write_result("not a tty\n")
      exit 1
    }
  }
}
