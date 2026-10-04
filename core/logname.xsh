#!/bin/xsh
use lib.gnu

const USAGE = """Usage: logname [OPTION]
Print the user's login name.

      --help     display this help and exit
      --version  output version information and exit
"""

type LognameOptions = {help: Bool, version: Bool}

# POSIX requires getlogin() and forbids fallbacks such as the environment, but
# no typed API exposes it yet (request: user.login_name). Until then the
# applet reports what GNU reports when the process has no login session.
proc main(...argv: List[Str]) [process, env, io, error] {
  let opts: LognameOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("logname")
    return
  }

  gnu.error("no login name")
  abort(1)
}
