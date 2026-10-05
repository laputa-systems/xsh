#!/bin/xsh
use lib.gnu

const USAGE = """Usage: users [OPTION]... [FILE]
Output who is currently logged in according to FILE.
If FILE is not specified, use /var/run/utmp.  /var/log/wtmp as FILE is common.

      --help        display this help and exit
      --version     output version information and exit
"""

type UsersOptions = {help: Bool, version: Bool, files: List[Str]}

# Like glibc's getutxent, an unreadable or missing file lists nobody; only a
# host without utmp support (a failure that carries no errno) is an error.
proc read_sessions(file: Path) [process, env, error] -> List[UnixUtmp] {
  match unix.read_utmp(file) {
    Ok(records) => records
    Err(failure) => {
      if failure.errno == null {
        gnu.error(f"{gnu.quote_maybe(file.display())}: {gnu.strerror(failure)}")
        exit 1
      }

      let none: List[UnixUtmp] = []
      none
    }
  }
}

# The default utmp file outlives the sessions it lists, so (as gnulib's
# READ_UTMP_CHECK_PIDS does) a user session whose process is gone is dropped.
# An explicit FILE is reported as written.
proc drop_dead_sessions(sessions: List[UnixUtmp]) [process] -> List[UnixUtmp] {
  [entry for entry in sessions if entry.kind != "user_process" or entry.pid <= 0 or process_exists(entry.pid)]
}

proc process_exists(pid: Int) [process] -> Bool {
  match process.kill(pid, "EXIT") {
    Ok(_) => true
    Err(failure) => failure.errno != 3
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
  let listed = read_sessions(file)
  let sessions = if opts.files.len() == 1 { listed } else { drop_dead_sessions(listed) }
  let names = [entry.user for entry in sessions if entry.kind == "user_process" and entry.user != ""]

  if names.len() > 0 {
    gnu.write_text(f"{names |> sort |> join(" ")}\n")
  }

  if let Err(failure) = io.flush_stdout() {
    gnu.write_failed(failure)
  }
}
