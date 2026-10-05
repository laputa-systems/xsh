#!/bin/xsh
use lib.gnu

const USAGE = """Usage: whoami [OPTION]...
Print the user name associated with the current effective user ID.
Same as id -un.

      --help     display this help and exit
      --version  output version information and exit
"""

type WhoamiOptions = {help: Bool, version: Bool}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: WhoamiOptions = cli.applet(
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
    gnu.version("whoami")
    return
  }

  let uid = unix.id()?.euid

  guard let account = user.by_uid(uid) else {
    gnu.error(f"cannot find name for user ID {uid}")
    exit 1
  }

  gnu.write_text(f"{account.name}\n")
}
