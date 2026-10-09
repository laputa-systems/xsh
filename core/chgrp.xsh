#!/bin/xsh
use lib.gnu

const USAGE = """Usage: chgrp [OPTION]... GROUP FILE...
Change the group of each FILE to GROUP.

  -c, --changes          like verbose but report only when a change is made
  -f, --silent, --quiet  suppress most error messages
  -v, --verbose          output a diagnostic for every file processed
      --from=CURRENT_OWNER:CURRENT_GROUP
                         change only when current owner/group match
      --reference=RFILE  use RFILE's group instead of a GROUP value
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

type GroupRecord = {gid: Int, members: List[Str], name: Str}
type ChgrpOptions = {recursive: Bool, changes: Bool, quiet: Bool, verbose: Bool, no_dereference: Bool, dereference: Bool, follow_root: Bool, follow_all: Bool, follow_none: Bool, preserve_root: Bool, no_preserve_root: Bool, reference: Str?, from: Str?, help: Bool, version: Bool, operands: List[Str]}
type ChangeResult = {failed: Bool, changed: Bool}
type GroupFilter = {uid: Int?, gid: Int?}

proc group_name(gid: Int) [fs] -> Str {
  match group.by_gid(gid) { Ok(found) => found.name; Err(_) => f"{gid}" }
}

proc lookup_group(name: Str) [fs, process, env, io] -> GroupRecord? {
  return null when name == ""
  if let Ok(gid) = name.parse_int() { return {gid: gid, name: name, members: []} }
  match group.lookup(name) {
    Ok(found) => found
    Err(_) => { gnu.error(f"invalid group: {gnu.quote(name)}"); exit 1 }
  }
}

pure group_record(gid: Int) -> GroupRecord {
  {gid: gid, name: f"{gid}", members: []}
}

proc group_filter(spec: Str) [fs, process, env, io] -> GroupFilter {
  let colon = spec.find(":")
  let dot = if colon == null { spec.find(".") } else { null }
  let split = if colon == null { dot } else { colon }
  if spec == "::" or (spec == "" and split == null) { gnu.error(f"invalid group: {gnu.quote(spec)}"); exit 1 }
  if dot != null { gnu.error("warning: '.' should be ':'") }
  let owner_name = if split == null { spec } else { spec.byte_slice(0, length: split ?? 0) }
  let group_name = if split == null { "" } else { spec.byte_slice((split ?? 0) + 1) }
  var uid: Int? = null
  if owner_name != "" {
    if let Ok(number) = owner_name.parse_int() { uid = number } else { if let Ok(found) = user.lookup(owner_name) { uid = found.uid } else { gnu.error(f"invalid user: {gnu.quote(owner_name)}"); exit 1 } }
  }
  let group_value = lookup_group(group_name)
  let gid: Int? = if group_value == null { null } else { group_value.gid }
  {uid: uid, gid: gid}
}

proc root_directory(target_path: Path, follow: Bool) [fs] -> Bool {
  match fs.stat(target_path, follow) {
    Err(_) => false
    Ok(meta) => match fs.stat(p"/", true) {
      Ok(root) => meta.dev == root.dev and meta.ino == root.ino
      Err(_) => false
    }
  }
}

proc change_group(target_path: Path, display_name: Str, group_rec: GroupRecord?, filter: GroupFilter, follow: Bool, quiet: Bool, verbose: Bool, changes_only: Bool) [fs, process, env, error, io] -> ChangeResult {
  match fs.stat(target_path, follow) {
    Err(failure) => {
      if verbose { gnu.write_text(f"failed to change group of {gnu.quote(display_name)} to {if group_rec != null { group_name((group_rec ?? group_record(0)).gid) } else { "" }}\n") }
      if ! quiet { gnu.error(f"cannot access {gnu.quote(display_name)}: {gnu.strerror(failure)}") }
      {failed: true, changed: false}
    }
    Ok(before) => {
      if (filter.uid != null and filter.uid != before.uid) or (filter.gid != null and filter.gid != before.gid) {
        {failed: false, changed: false}
      } else if group_rec == null {
        {failed: false, changed: false}
      } else {
        let ids = unix.id()?
        let denied = ids.euid != 0 and before.uid != ids.euid
        if denied {
          if ! quiet {
            gnu.error(f"changing group of {gnu.quote(display_name)}: Operation not permitted")
            if verbose { eprint f"failed to change group of {gnu.quote(display_name)} from {group_name(before.gid)}" }
          }
          {failed: true, changed: false}
        } else { match fs.chgrp(target_path, group_rec ?? group_record(0), follow_symlinks: follow) {
          Err(failure) => {
            if ! quiet {
              gnu.error(f"changing group of {gnu.quote(display_name)}: {gnu.strerror(failure)}")
              if verbose { eprint f"failed to change group of {gnu.quote(display_name)} from {group_name(before.gid)}" }
            }
            {failed: true, changed: false}
          }
          Ok(_) => {
            let after = fs.stat(target_path, follow)?
            let changed = before.gid != after.gid
            if (verbose or changes_only) and (! changes_only or changed) {
              gnu.write_text(f"group of {gnu.quote(display_name)} {if changed { "changed" } else { "retained" }} as {group_name(after.gid)}\n")
            }
            {failed: false, changed: changed}
          }
        } }
      }
    }
  }
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: ChgrpOptions = cli.applet(
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
  if opts.version { gnu.version("chgrp"); return }

  var filter: GroupFilter = {uid: null, gid: null}
  if opts.from != null { filter = group_filter(opts.from ?? "") }
  var group_rec: GroupRecord? = null
  var paths = opts.operands
  if opts.reference != null {
    if paths.len() == 0 { gnu.missing_operand() }
    match fs.stat(fp"{opts.reference ?? ""}", true) {
      Ok(meta) => group_rec = group_record(meta.gid)
      Err(failure) => { gnu.error(f"cannot stat {gnu.quote(opts.reference ?? "")}: {gnu.strerror(failure)}"); exit 1 }
    }
  } else {
    if paths.len() < 2 { gnu.missing_operand() }
    group_rec = lookup_group(paths[0])
    paths = paths |> drop(1)
  }
  let requested_group = group_rec

  let root_follow = ! opts.no_dereference and (! opts.recursive or opts.dereference or opts.follow_root or opts.follow_all)
  let descend_follow = ! opts.no_dereference and (opts.follow_all or opts.dereference)
  var failed = false
  for name in paths {
    let target = fp"{name}"
    if opts.recursive and root_directory(target, root_follow) and ! opts.no_preserve_root {
      gnu.error(f"it is dangerous to operate recursively on {gnu.quote(name)}{if name != "/" { " (same as '/')" } else { "" }}")
      gnu.error("use --no-preserve-root to override this failsafe")
      failed = true
      continue
    }
    match fs.stat(target, root_follow) {
      Err(problem) => {
        if opts.verbose { gnu.write_text(f"failed to change group of {gnu.quote(name)} to {if requested_group != null { group_name((requested_group ?? group_record(0)).gid) } else { "" }}\n") }
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
              let result = change_group(entry.path, shown, requested_group, filter, follow, opts.quiet, opts.verbose, opts.changes)
              failed = failed or result.failed
            }
          }
        }
      }
      Ok(_) => {
        let result = change_group(target, name, requested_group, filter, root_follow, opts.quiet, opts.verbose, opts.changes)
        failed = failed or result.failed
      }
    }
  }
  if failed { exit 1 }
}
