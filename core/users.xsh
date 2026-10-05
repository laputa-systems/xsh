#!/bin/xsh
use lib.gnu

const USAGE = """Usage: users [OPTION]... [FILE]
Output who is currently logged in according to FILE.
If FILE is not specified, use /var/run/utmp.  /var/log/wtmp as FILE is common.

      --help        display this help and exit
      --version     output version information and exit
"""

type Session = {
  addr: Str,
  exit_status: Int,
  host: Str,
  id: Str,
  kind: Str,
  line: Str,
  pid: Int,
  session: Int,
  termination: Int,
  time_sec: Int,
  time_usec: Int,
  type: Int,
  user: Str,
}

type UsersOptions = {help: Bool, version: Bool, files: List[Str]}

# Like glibc's getutxent, an unreadable or missing file lists nobody; only a
# host without utmp support (a failure that carries no errno) is an error.
proc read_sessions(file: Path) [process, env, error] -> List[Session] {
  match unix.read_utmp(file) {
    Ok(records) => records
    Err(failure) => {
      if failure.errno == null {
        gnu.error(f"{gnu.quote_maybe(file.display())}: {gnu.strerror(failure)}")
        exit 1
      }

      let none: List[Session] = []
      none
    }
  }
}

proc main(...argv: List[Str]) [process, env, error, io] {
  let opts: UsersOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("users")
    return
  }

  if opts.files.len() > 1 {
    gnu.extra_operand(opts.files[1])
  }

  let file = if opts.files.len() == 1 { Path(opts.files[0]) } else { p"/var/run/utmp" }
  let names = [entry.user for entry in read_sessions(file) if entry.kind == "user_process" and entry.user != ""]

  if names.len() > 0 {
    gnu.write_text(f"{names |> sort |> join(" ")}\n")
  }

  if let Err(failure) = io.flush_stdout() {
    gnu.write_failed(failure)
  }
}
