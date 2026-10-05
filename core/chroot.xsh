#!/bin/xsh
use lib.gnu
use lib.perm

type Options = {userspec: Str?, groups: Str?, skip_chdir: Bool, help: Bool, version: Bool, operands: List[Str]}

proc main(...argv: List[Str]) [fs, error, process, env, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 125, permute: false},
    userspec: {form: "--userspec USER:GROUP"},
    groups: {form: "--groups G_LIST"},
    skip_chdir: {form: "--skip-chdir", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    operands: {form: "...ARG"},
  })?
  if opts.help {
    gnu.help("Usage: chroot [OPTION] NEWROOT [COMMAND [ARG]...]\nRun COMMAND with root directory set to NEWROOT.\n      --userspec=USER:GROUP\n      --groups=G_LIST\n      --skip-chdir\n      --help\n      --version")
    return
  }
  if opts.version { gnu.version("chroot"); return }
  if opts.operands.is_empty() { gnu.missing_operand(125) }
  let new_root = fp"{opts.operands[0]}"
  if opts.skip_chdir and (new_root.resolve() ?? p".") != p"/" {
    gnu.usage_error("option --skip-chdir only permitted if NEWROOT is old '/'", 125)
  }
  # Account databases inside the new root can assign different IDs. Prime
  # lookups outside first; supplementary groups can fall back to that result
  # when the new root has no account database.
  let outer = if let spec = opts.userspec { identity(spec) } else { Ok(empty_identity()) }
  let outer_groups = if let spec = opts.groups { additional_groups(spec) } else {
    if let Ok(ids) = outer {
      if let name = ids.username { user.groups(name, primary_gid: ids.gid) } else { Ok([]) }
    } else { Ok([]) }
  }
  if let Err(failure) = linux.chroot(new_root) {
    gnu.cannot("change root directory to", f"{new_root}", failure)
    exit 125
  }
  set_identity(opts, outer, outer_groups)
  let words = if opts.operands.len() > 1 { opts.operands[1..] } else { [env.get_or("SHELL", "/bin/sh") ?? "/bin/sh", "-i"] }
  if opts.skip_chdir {
    launch(words)
  } else {
    let working_dir = p"/"
    cd $working_dir { launch(words) }
  }
}

# Relative commands and PATH entries resolve in the new working directory.
proc launch(words: List[Str]) [fs, error, process, env, io] {
  let command = words[0]
  if command.find("/") != null {
    let target = fp"{command}"
    match fs.stat(target, follow_symlinks: true) {
      Ok(meta) => {
        if meta.kind != "file" or meta.mode.bit_and(0o111) == 0 {
          gnu.error(f"failed to run command {gnu.quote(command)}: Permission denied")
          exit 126
        }
      }
      Err(failure) => {
        let missing = (failure.errno ?? gnu.errno(failure)) == 2
        gnu.error(f"failed to run command {gnu.quote(command)}: {gnu.strerror(failure)}")
        exit if missing { 127 } else { 126 }
      }
    }
  } else if let Err(failure) = process.which(command) {
    let missing = failure is NotFound
    gnu.error(f"failed to run command {gnu.quote(command)}: {if missing { "No such file or directory" } else { "Permission denied" }}")
    exit if missing { 127 } else { 126 }
  }
  let plan = process.command_argv(command, words)
  if let Err(failure) = unix.exec(plan) {
    let reason = gnu.strerror(failure)
    gnu.error(f"failed to run command {gnu.quote(command)}: {reason}")
    exit if (failure.errno ?? gnu.errno(failure)) == 2 { 127 } else { 126 }
  }
}

error IdentityError = Invalid : Usage

type Identity = {uid: Int?, gid: Int?, username: Str?}

pure empty_identity() -> Identity { {uid: null, gid: null, username: null} }

proc identity(spec: Str) [fs, error] -> Result[Identity] {
  let parts = spec.split(":")
  return Err(IdentityError.Invalid("invalid group")) when parts.len() > 2
  var uid: Int? = null
  var gid: Int? = null
  var username: Str? = null
  if parts[0] != "" {
    match perm.uid(parts[0]) {
      Ok(id) => uid = id
      Err(_) => return Err(IdentityError.Invalid("invalid user"))
    }
  }
  if parts.len() == 2 and parts[1] != "" {
    match perm.gid(parts[1]) {
      Ok(id) => gid = id
      Err(_) => return Err(IdentityError.Invalid("invalid group"))
    }
  }
  if let id = uid {
    if let Ok(account) = user.by_uid(id) {
      username = account.name
      if gid == null { gid = account.gid }
    }
  }
  {uid: uid, gid: gid, username: username}
}

proc additional_groups(spec: Str) [fs, error, env] -> Result[List[Int]] {
  if spec == "" { return [] }
  var groups: List[Int] = []
  for name in spec.split(",") {
    if name == "" { continue }
    let group_name = name.trim()
    match perm.gid(group_name) {
      Ok(id) => groups += [id]
      Err(_) => return Err(IdentityError.Invalid(f"invalid group {gnu.quote(name)}"))
    }
  }
  return Err(IdentityError.Invalid(f"invalid group list {gnu.quote(spec)}")) when groups.is_empty()
  groups
}

proc set_identity(opts: Options, outer: Result[Identity], outer_groups: Result[List[Int]]) [fs, error, process, env] {
  var ids = empty_identity()
  if let spec = opts.userspec {
    match identity(spec) {
      Ok(parsed) => ids = parsed
      Err(failure) => { gnu.error(failure.message); exit 125 }
    }
    if let uid = ids.uid {
      if let Ok(before) = outer {
        if before.uid == ids.uid {
          ids = {uid: ids.uid, gid: if ids.gid != null { ids.gid } else { before.gid }, username: if ids.username != null { ids.username } else { before.username }}
        }
      }
      if ids.gid == null { gnu.error(f"no group specified for unknown uid: {uid}"); exit 125 }
    }
  }
  if ids.uid != null or opts.groups != null {
    let memberships = if let spec = opts.groups { additional_groups(spec) } else {
      if let name = ids.username { user.groups(name, primary_gid: ids.gid) } else { Ok([]) }
    }
    let groups = match memberships {
      Ok(found) => found
      Err(failure) => match outer_groups {
        Ok(before) => {
          if before.is_empty() { gnu.error(failure.message); exit 125 }
          before
        }
        Err(_) => { gnu.error(failure.message); exit 125 }
      }
    }
    if let Err(failure) = unix.set_groups(groups) {
      gnu.error(f"failed to set supplemental groups: {gnu.strerror(failure)}")
      exit 125
    }
  }
  if let gid = ids.gid {
    if let Err(failure) = unix.set_gid(gid) {
      gnu.error(f"failed to set group-ID: {gnu.strerror(failure)}")
      exit 125
    }
  }
  if let uid = ids.uid {
    if let Err(failure) = unix.set_uid(uid) {
      gnu.error(f"failed to set user-ID: {gnu.strerror(failure)}")
      exit 125
    }
  }
}
