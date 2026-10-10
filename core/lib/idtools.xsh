##! Account and group identity printing shared by `id` and `groups`.
##!
##! Follows GNU `group-list.c`: a process lists its real gid, then its
##! effective gid when it differs, then the supplementary groups. Names come
##! from the user and group databases; an id without an entry prints as a
##! number, and `-n` reports it as an error.

use gnu

## The ids of the calling process. `groups` holds the supplementary groups
## without the effective gid, sorted, because `unix.id` does not expose the
## order of `getgroups`.
export type Process = {ruid: Int, euid: Int, rgid: Int, egid: Int, groups: List[Int]}

## The ids of the calling process.
export proc process_ids() [process, error] -> Result[Process, Error] {
  let me = unix.id()?

  Ok({
    ruid: me.uid,
    euid: me.euid,
    rgid: me.gid,
    egid: me.egid,
    groups: [entry.gid for entry in me.groups if entry.gid != me.egid],
  })
}

## The ids of the account `name`: the primary gid is both real and effective,
## and `groups` holds the other memberships. `user.groups` returns them sorted
## by gid rather than in getgrouplist order.
export proc account_ids(name: Str) [fs, process, error] -> Result[Process, Error] {
  let account = user.lookup(name)?
  let memberships = user.groups(name)?

  Ok({
    ruid: account.uid,
    euid: account.uid,
    rgid: account.gid,
    egid: account.gid,
    groups: [gid for gid in memberships if gid != account.gid],
  })
}

## The name of user ID `uid`, or null without a database entry.
export proc user_name(uid: Int) [fs] -> Str? {
  guard let account = user.by_uid(uid) else {
    return null
  }

  account.name
}

## The name of group ID `gid`, or null without a database entry.
export proc group_name(gid: Int) [fs] -> Str? {
  guard let entry = group.by_gid(gid) else {
    return null
  }

  entry.name
}

## Print the name or number of user ID `uid`. Returns false after reporting a
## missing name under `use_name`.
export proc print_user(uid: Int, use_name: Bool) [fs, process, env, io] -> Bool {
  let name = if use_name { user_name(uid) } else { null }

  if use_name and name == null {
    gnu.error(f"cannot find name for user ID {uid}")
  }

  gnu.write_text(if name == null { f"{uid}" } else { name })

  ! use_name or name != null
}

## Print the name or number of group ID `gid`. Returns false after reporting a
## missing name under `use_name`.
export proc print_group(gid: Int, use_name: Bool) [fs, process, env, io] -> Bool {
  let name = if use_name { group_name(gid) } else { null }

  if use_name and name == null {
    gnu.error(f"cannot find name for group ID {gid}")
  }

  gnu.write_text(if name == null { f"{gid}" } else { name })

  ! use_name or name != null
}

## Print the distinct groups of the calling process separated by `delimiter`:
## real gid, effective gid, then the supplementary groups.
export proc print_group_list(who: Process, use_names: Bool, delimiter: Str) [fs, process, env, io] -> Bool {
  var ok = print_group(who.rgid, use_names)

  if who.egid != who.rgid {
    gnu.write_text(delimiter)
    ok = print_group(who.egid, use_names) and ok
  }

  for gid in who.groups {
    if gid != who.rgid {
      gnu.write_text(delimiter)
      ok = print_group(gid, use_names) and ok
    }
  }

  ok
}
