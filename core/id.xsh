#!/bin/xsh
use lib.gnu
use lib.idtools

const USAGE = """Usage: id [OPTION]... [USER]...
Print user and group information for each specified USER,
or (when USER omitted) for the current process.

  -a
         ignore, for compatibility with other versions
  -Z, --context
         print only the security context of the process
  -g, --group
         print only the effective group ID
  -G, --groups
         print all group IDs
  -n, --name
         print a name instead of a number, for -u, -g, -G
  -r, --real
         print the real ID instead of the effective ID, with -u, -g, -G
  -u, --user
         print only the effective user ID
  -z, --zero
         delimit entries with NUL characters, not whitespace;
         not permitted in default format
      --help     display this help and exit
      --version  output version information and exit

Without any OPTION, print some useful set of identified information.
"""

type IdOptions = {
  svr4_all: Bool,
  context: Bool,
  group: Bool,
  groups: Bool,
  name: Bool,
  real: Bool,
  user: Bool,
  zero: Bool,
  help: Bool,
  version: Bool,
  users: List[Str],
}

type Found = {uid: Int, gid: Int, name: Str}

# The user an operand names: a login name, or a numeric ID without an entry
# under that name. Null when there is no such user.
proc find_user(spec: Str) [fs] -> Found? {
  return null when spec == ""

  if let Ok(found) = user.lookup(spec) {
    return {uid: found.uid, gid: found.gid, name: found.name}
  }

  return null when ! rx"^[0-9]+$".matches(spec)

  guard let found = user.by_uid(spec.parse_int() ?? -1) else {
    return null
  }

  {uid: found.uid, gid: found.gid, name: found.name}
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: IdOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      svr4_all: {form: "-a", default: false},
      context: {form: "-Z --context", default: false},
      group: {form: "-g --group", default: false},
      groups: {form: "-G --groups", default: false},
      name: {form: "-n --name", default: false},
      real: {form: "-r --real", default: false},
      user: {form: "-u --user", default: false},
      zero: {form: "-z --zero", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      users: {form: "...USER"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("id")
    return
  }

  if opts.context {
    gnu.error("--context (-Z) works only on an SELinux-enabled kernel")
    exit 1
  }

  let chosen = (if opts.user { 1 } else { 0 }) + (if opts.group { 1 } else { 0 }) + (if opts.groups { 1 } else { 0 })

  if chosen > 1 {
    gnu.error("cannot print \"only\" of more than one choice")
    exit 1
  }

  let default_format = chosen == 0

  if default_format and (opts.real or opts.name) {
    gnu.error("printing only names or real IDs requires -u, -g, or -G")
    exit 1
  }

  if default_format and opts.zero {
    gnu.error("option --zero not permitted in default format")
    exit 1
  }

  let separator = if opts.zero { "\0" } else { " " }
  let multiple = opts.users.len() > 1
  var ok = true
  var targets: List[Str?] = [null]

  if opts.users.len() > 0 {
    targets = [spec for spec in opts.users]
  }

  for target in targets {
    var who = idtools.process_ids()?
    var named = ""

    if target != null {
      let found = find_user(target)

      if found == null {
        gnu.error(f"{gnu.quote_value(target)}: no such user")
        ok = false
        continue
      }

      if ! opts.user and ! opts.group {
        idtools.unsupported_user_groups(target)
      }

      named = found.name
      who = {ruid: found.uid, euid: found.uid, rgid: found.gid, egid: found.gid, groups: []}
    }

    if opts.user {
      ok = idtools.print_user(if opts.real { who.ruid } else { who.euid }, opts.name) and ok
    } else if opts.group {
      ok = idtools.print_group(if opts.real { who.rgid } else { who.egid }, opts.name) and ok
    } else if opts.groups {
      ok = idtools.print_group_list(who, opts.name, separator) and ok
    } else {
      var line = f"uid={who.ruid}"
      let ruser = idtools.user_name(who.ruid)
      let rgroup = idtools.group_name(who.rgid)

      line += if ruser == null { "" } else { f"({ruser})" }
      line += f" gid={who.rgid}"
      line += if rgroup == null { "" } else { f"({rgroup})" }

      if who.euid != who.ruid {
        let euser = idtools.user_name(who.euid)

        line += f" euid={who.euid}"
        line += if euser == null { "" } else { f"({euser})" }
      }

      if who.egid != who.rgid {
        let egroup = idtools.group_name(who.egid)

        line += f" egid={who.egid}"
        line += if egroup == null { "" } else { f"({egroup})" }
      }

      var listed: List[Str] = []

      for gid in [who.egid, @who.groups] {
        let label = idtools.group_name(gid)

        listed += [if label == null { f"{gid}" } else { f"{gid}({label})" }]
      }

      gnu.write_text(f"{line} groups={listed.join(",")}")
    }

    if opts.zero and opts.groups and multiple {
      gnu.write_text("\0\0")
    } else {
      gnu.write_text(if opts.zero { "\0" } else { "\n" })
    }
  }

  if ! ok {
    exit 1
  }
}
