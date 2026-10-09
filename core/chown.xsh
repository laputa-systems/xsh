#!/bin/xsh
use lib.gnu

const USAGE = """Usage: chown [OPTION]... [OWNER][:[GROUP]] FILE...
Change the owner and/or group of each FILE to OWNER and/or GROUP.

  -c, --changes          like verbose but report only when a change is made
  -f, --silent, --quiet  suppress most error messages
  -v, --verbose          output a diagnostic for every file processed
      --from=CURRENT_OWNER:CURRENT_GROUP
                         change only when current owner/group match
      --reference=RFILE  use RFILE's owner and group
  -h, --no-dereference   affect symbolic links instead of referenced files
  -H                     follow command-line symbolic links in recursive mode
  -L                     follow every symbolic link to a directory
  -P                     do not follow symbolic links (default)
  -R, --recursive        operate on files and directories recursively
      --preserve-root    fail to operate recursively on '/'
      --no-preserve-root do not treat '/' specially
      --help             display this help and exit
      --version          output version information and exit
"""

type Owner = {gid: Int, home: Path, name: Str, shell: Str, uid: Int}
type GroupRecord = {gid: Int, members: List[Str], name: Str}
type OwnerGroup = {owner: Str, group: Str}
type Identity = {uid: Int?, gid: Int?}
type ChownOptions = {recursive: Bool, changes: Bool, quiet: Bool, verbose: Bool, no_dereference: Bool, dereference: Bool, follow_root: Bool, follow_all: Bool, follow_none: Bool, preserve_root: Bool, no_preserve_root: Bool, reference: Str?, from: Str?, help: Bool, version: Bool, operands: List[Str]}
type ChangeResult = {failed: Bool, changed: Bool}

proc user_name(uid: Int) [fs] -> Str {
  match user.by_uid(uid) { Ok(found) => found.name; Err(_) => f"{uid}" }
}

proc group_name(gid: Int) [fs] -> Str {
  match group.by_gid(gid) { Ok(found) => found.name; Err(_) => f"{gid}" }
}

proc ownership_label(uid: Int, gid: Int, owner: Owner?, group_rec: GroupRecord?) [fs] -> Str {
  if owner != null and group_rec != null { f"{user_name(uid)}:{group_name(gid)}" } else { if owner != null { user_name(uid) } else { if group_rec != null { group_name(gid) } else { f"{user_name(uid)}:{group_name(gid)}" } } }
}

pure split_owner_group(spec: Str) -> OwnerGroup {
  if let at = spec.find(":") {
    {owner: spec.byte_slice(0, length: at), group: spec.byte_slice(at + 1)}
  } else if let at = spec.find(".") {
    {owner: spec.byte_slice(0, length: at), group: spec.byte_slice(at + 1)}
  } else {
    {owner: spec, group: ""}
  }
}

proc lookup_owner(name: Str) [fs, process, env, io] -> Owner? {
  return null when name == ""
  if let Ok(uid) = name.parse_int() { return {uid: uid, gid: 0, name: name, home: p"/", shell: ""} }
  match user.lookup(name) {
    Ok(found) => found
    Err(_) => { gnu.error(f"invalid user: {gnu.quote(name)}"); exit 1 }
  }
}

proc lookup_group(name: Str) [fs, process, env, io] -> GroupRecord? {
  return null when name == ""
  if let Ok(gid) = name.parse_int() { return {gid: gid, name: name, members: []} }
  match group.lookup(name) {
    Ok(found) => found
    Err(_) => { gnu.error(f"invalid group: {gnu.quote(name)}"); exit 1 }
  }
}

proc parse_identity(spec: Str) [fs, process, env, io] -> Identity {
  let parts = split_owner_group(spec)
  if spec.starts_with(".") and spec.find(":") == null {
    gnu.error(f"invalid group: {gnu.quote(spec)}")
    exit 1
  }
  if spec == "::" or (parts.owner == "" and parts.group == "" and spec != ":") {
    gnu.error(f"invalid group: {gnu.quote(spec)}")
    exit 1
  }
  if spec.find(".") != null and spec.find(":") == null { gnu.error("warning: '.' should be ':'") }
  var owner: Owner? = null
  if parts.owner != "" {
    if let Ok(uid) = parts.owner.parse_int() {
      owner = owner_record(uid)
    } else {
      match user.lookup(parts.owner) {
        Ok(found) => owner = found
        Err(_) => { gnu.error(f"invalid user: {gnu.quote(spec)}"); exit 1 }
      }
    }
  }
  let group_value = lookup_group(parts.group)
  let uid: Int? = if owner == null { null } else { owner.uid }
  let gid: Int? = if group_value == null { null } else { group_value.gid }
  {uid: uid, gid: gid}
}

pure owner_record(uid: Int) -> Owner {
  {uid: uid, gid: 0, name: f"{uid}", home: p"/", shell: ""}
}

pure group_record(gid: Int) -> GroupRecord {
  {gid: gid, name: f"{gid}", members: []}
}

proc root_directory(target_path: Path, follow: Bool) [fs] -> Bool {
  match fs.stat(target_path, follow) {
    Err(_) => false
    Ok(meta) => {
      match fs.stat(p"/", true) {
        Ok(root) => meta.dev == root.dev and meta.ino == root.ino
        Err(_) => false
      }
    }
  }
}

proc change_one(target_path: Path, display_name: Str, owner: Owner?, group_rec: GroupRecord?, from: Identity, follow: Bool, quiet: Bool, verbose: Bool, changes_only: Bool, utility: Str) [fs, process, env, error, io] -> ChangeResult {
  match fs.stat(target_path, follow) {
    Err(failure) => {
      if verbose {
        if utility == "chown" { gnu.write_text(f"failed to change ownership of {gnu.quote(display_name)} to {if owner != null { user_name((owner ?? owner_record(0)).uid) } else { "" }}\n") } else { gnu.write_text(f"failed to change group of {gnu.quote(display_name)} to {if group_rec != null { group_name((group_rec ?? group_record(0)).gid) } else { "" }}\n") }
      }
      if ! quiet { gnu.error(f"cannot access {gnu.quote(display_name)}: {gnu.strerror(failure)}") }
      {failed: true, changed: false}
    }
    Ok(before) => {
      if (from.uid != null and from.uid != before.uid) or (from.gid != null and from.gid != before.gid) {
        if verbose { gnu.write_text(f"ownership of {gnu.quote(display_name)} retained as {ownership_label(before.uid, before.gid, owner, group_rec)}\n") }
        {failed: false, changed: false}
      } else if owner == null and group_rec == null {
        if verbose { gnu.write_text(f"ownership of {gnu.quote(display_name)} retained as {ownership_label(before.uid, before.gid, owner, group_rec)}\n") }
        {failed: false, changed: false}
      } else {
        let ids = unix.id()?
        let denied = ids.euid != 0 and before.uid != ids.euid
        var failure_message: Str? = null
        if owner != null {
          if ! denied {
            match fs.chown(target_path, owner ?? owner_record(0), follow_symlinks: follow) {
              Err(problem) => failure_message = gnu.strerror(problem)
              Ok(_) => {}
            }
          }
        }
        if group_rec != null and ! denied {
          if failure_message == null {
            match fs.chgrp(target_path, group_rec ?? group_record(0), follow_symlinks: follow) {
              Err(problem) => failure_message = gnu.strerror(problem)
              Ok(_) => {}
            }
          }
        }
        if denied or failure_message != null {
            if ! quiet {
              let changing = if utility == "chown" { "ownership" } else { "group" }
              if denied { gnu.error(f"changing {changing} of {gnu.quote(display_name)}: Operation not permitted") } else { gnu.error(f"changing {changing} of {gnu.quote(display_name)}: {failure_message ?? "Operation not permitted"}") }
              if verbose { gnu.error(f"failed to change {changing} of {gnu.quote(display_name)} from {ownership_label(before.uid, before.gid, owner, group_rec)}") }
            }
            {failed: true, changed: false}
        } else {
            let after = fs.stat(target_path, follow)?
            let changed = before.uid != after.uid or before.gid != after.gid
            if (verbose or changes_only) and (! changes_only or changed) {
              if utility == "chown" {
                gnu.write_text(f"ownership of {gnu.quote(display_name)} {if changed { "changed" } else { "retained" }} as {ownership_label(after.uid, after.gid, owner, group_rec)}\n")
              } else {
                gnu.write_text(f"group of {gnu.quote(display_name)} {if changed { "changed" } else { "retained" }} as {group_name(after.gid)}\n")
              }
            }
            {failed: false, changed: changed}
        }
      }
    }
  }
}

proc run_chown(argv: List[Str], utility: Str) [fs, process, env, error, io] {
  let opts: ChownOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      recursive: {form: "-R --recursive", default: false},
      changes: {form: "-c --changes", default: false},
      quiet: {form: "-f --silent --quiet", default: false},
      verbose: {form: "-v --verbose", default: false},
      no_dereference: {form: "-h --no-dereference", default: false},
      dereference: {form: "--dereference", default: false, conflicts: ["no_dereference"]},
      follow_root: {form: "-H", default: false, conflicts: ["follow_all", "follow_none"]},
      follow_all: {form: "-L", default: false, conflicts: ["follow_root", "follow_none"]},
      follow_none: {form: "-P", default: false, conflicts: ["follow_root", "follow_all"]},
      preserve_root: {form: "--preserve-root", default: false, conflicts: "no_preserve_root"},
      no_preserve_root: {form: "--no-preserve-root", default: false, conflicts: "preserve_root"},
      from: {form: "--from CURRENT_OWNER:CURRENT_GROUP"},
      reference: {form: "--reference RFILE"},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      operands: {form: "...ARG"},
    },
  )?

  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version(utility); return }

  var owner: Owner? = null
  var group_rec: GroupRecord? = null
  var from: Identity = {uid: null, gid: null}
  var paths = opts.operands
  if opts.reference != null {
    if paths.len() == 0 { gnu.missing_operand() }
    match fs.stat(fp"{opts.reference ?? ""}", true) {
      Ok(meta) => { owner = owner_record(meta.uid); group_rec = group_record(meta.gid) }
      Err(failure) => { gnu.error(f"cannot stat {gnu.quote(opts.reference ?? "")}: {gnu.strerror(failure)}"); exit 1 }
    }
  } else {
    if paths.len() < 2 { gnu.missing_operand() }
    let spec = parse_identity(paths[0])
    paths = paths |> drop(1)
    owner = if spec.uid == null { null } else { lookup_owner(f"{spec.uid ?? 0}") }
    group_rec = if spec.gid == null { null } else { lookup_group(f"{spec.gid ?? 0}") }
  }
  if opts.from != null { from = parse_identity(opts.from ?? "") }

  let root_follow = ! opts.no_dereference and (! opts.recursive or opts.dereference or opts.follow_root or opts.follow_all)
  let descend_follow = ! opts.no_dereference and (opts.follow_all or opts.dereference)
  var failed = false
  for name in paths {
    let target = fp"{name}"
    if opts.recursive and root_directory(target, root_follow) and ! opts.no_preserve_root {
      gnu.error(f"it is dangerous to operate recursively on {gnu.quote(name)}")
      gnu.error("use --no-preserve-root to override this failsafe")
      failed = true
      continue
    }
    match fs.stat(target, root_follow) {
      Err(problem) => {
        if opts.verbose { gnu.write_text(f"failed to change ownership of {gnu.quote(name)} to {if owner != null { user_name((owner ?? owner_record(0)).uid) } else { "" }}\n") }
        if ! opts.quiet { gnu.error(f"cannot access {gnu.quote(name)}: {gnu.strerror(problem)}") }
        failed = true
      }
      Ok(meta) if opts.recursive and meta.kind == "dir" => {
        match fs.walk(target, stat: true) {
          Err(problem) => { if ! opts.quiet { gnu.error(f"cannot access {gnu.quote(name)}: {gnu.strerror(problem)}") }; failed = true }
          Ok(entries) => {
            let walk_root = target.resolve()?
            for entry in entries |> sort-by(desc: true) .path {
              let follow = if opts.no_dereference { false } else if entry.path == target { root_follow } else { descend_follow }
              let relative = entry.path.relative_to(walk_root).display()
              let shown = if relative == "." { name } else { f"{name}/{relative}" }
              let result = change_one(entry.path, shown, owner, group_rec, from, follow, opts.quiet, opts.verbose, opts.changes, utility)
              failed = failed or result.failed
            }
          }
        }
      }
      Ok(_) => {
        let result = change_one(target, name, owner, group_rec, from, root_follow, opts.quiet, opts.verbose, opts.changes, utility)
        failed = failed or result.failed
      }
    }
  }
  if failed { exit 1 }
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  run_chown(argv, "chown")
}
