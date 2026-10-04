#!/bin/xsh
use lib.gnu
use lib.idtools

const USAGE = """Usage: groups [OPTION]... [USERNAME]...
Print group memberships for each USERNAME or, if no USERNAME is specified, for
the current process (which may differ if the groups database has changed).
      --help     display this help and exit
      --version  output version information and exit
"""

type GroupsOptions = {help: Bool, version: Bool, users: List[Str]}

proc main(...argv: List[Str]) [process, env, io, fs, error] {
  let opts: GroupsOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      users: {form: "...USERNAME"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("groups")
    return
  }

  if opts.users.len() == 0 {
    let who = idtools.process_ids()?
    let ok = idtools.print_group_list(who, true, " ")

    gnu.write_text("\n")

    if ! ok {
      abort(1)
    }

    return
  }

  var ok = true

  for name in opts.users {
    if let Err(_) = user.lookup(name) {
      gnu.error(f"{gnu.quote(name)}: no such user")
      ok = false
    } else {
      idtools.unsupported_user_groups(name)
    }
  }

  if ! ok {
    abort(1)
  }
}
