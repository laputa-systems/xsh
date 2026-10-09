#!/bin/xsh
use lib.gnu

const USAGE = """Usage: install [OPTION]... [-T] SOURCE DEST
  or:  install [OPTION]... SOURCE... DIRECTORY
  or:  install [OPTION]... -t DIRECTORY SOURCE...
  or:  install [OPTION]... -d DIRECTORY...
Copy files and set attributes.

  -c                          (ignored)
  -C, --compare               compare content before installing
  -d, --directory             create directories
  -D                          create leading destination directories
  -g, --group=GROUP           set group ownership
  -m, --mode=MODE             set permission mode
  -o, --owner=OWNER           set ownership
  -p, --preserve-timestamps   apply source timestamps to installed files
  -s, --strip                 strip symbol tables
  -S, --suffix=SUFFIX         override the usual backup suffix
  -t, --target-directory=DIR  install all SOURCE arguments into DIR
  -T, --no-target-directory   treat DEST as a normal file
  -U, --unprivileged          skip ownership changes
  -v, --verbose               print the name of each created file or directory
  -b, --backup[=CONTROL]      make a backup of each existing destination file
      --help                  display this help and exit
      --version               output version information and exit
"""

type InstallOptions = {
  compare: Bool,
  gnu_copy: Bool,
  directory: Bool,
  create_leading: Bool,
  no_target_directory: Bool,
  preserve_timestamps: Bool,
  strip: Bool,
  unprivileged: Bool,
  verbose: Bool,
  target: List[Str],
  mode: List[Str],
  owner: List[Str],
  group: List[Str],
  backup: Str,
  suffix: Str,
  strip_program: Str,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

type InstallStat = {kind: Str, dev: Int, ino: Int, mode: Int, uid: Int, gid: Int, mtime_ns: Int}

pure mode_digit(ch: Str) -> Int? {
  match ch {
    "0" => 0
    "1" => 1
    "2" => 2
    "3" => 3
    "4" => 4
    "5" => 5
    "6" => 6
    "7" => 7
    else => null
  }
}

pure mode_who(who: Str) -> Str {
  if who == "" or "a" in who { "ugo" } else { who }
}

pure class_bits(who: Str) -> Int {
  let classes = mode_who(who)
  var bits = 0
  if "u" in classes { bits = bits + 0o4700 }
  if "g" in classes { bits = bits + 0o2070 }
  if "o" in classes { bits = bits + 0o1007 }
  bits
}

pure permission_bits(perms: Str, who: Str, current: Int, is_dir: Bool) -> Int {
  let classes = mode_who(who)
  let executable = is_dir or current.bit_and(0o111) != 0
  var bits = 0
  for perm in perms {
    if "u" in classes {
      match perm {
        "r" => bits = bits + 0o400
        "w" => bits = bits + 0o200
        "x" => bits = bits + 0o100
        "X" if executable => bits = bits + 0o100
        "s" => bits = bits + 0o4000
        else => {}
      }
    }
    if "g" in classes {
      match perm {
        "r" => bits = bits + 0o40
        "w" => bits = bits + 0o20
        "x" => bits = bits + 0o10
        "X" if executable => bits = bits + 0o10
        "s" => bits = bits + 0o2000
        else => {}
      }
    }
    if "o" in classes {
      match perm {
        "r" => bits = bits + 0o4
        "w" => bits = bits + 0o2
        "x" => bits = bits + 0o1
        "X" if executable => bits = bits + 0o1
        "t" => bits = bits + 0o1000
        else => {}
      }
    }
  }
  bits
}

pure symbolic_mode(spec: Str, is_dir: Bool) -> Result[Int, Str] {
  var result = 0
  for clause in spec.split(",") {
    var who = ""
    var op = ""
    var perms = ""
    for ch in clause {
      if op == "" and ch in "ugoa" {
        who = f"{who}{ch}"
      } else if op == "" and ch in "+-=" {
        op = ch
      } else {
        perms = f"{perms}{ch}"
      }
    }
    if op == "" { return Err("invalid mode string") }
    for perm in perms {
      if perm not in ["r", "w", "x", "X", "s", "t", "u", "g", "o"] {
        return Err("invalid operator")
      }
    }
    let mask = permission_bits(perms, who, result, is_dir)
    match op {
      "+" => result = result.bit_or(mask)
      "-" => result = result.clear_bits(mask)
      "=" => result = result.clear_bits(class_bits(who)).bit_or(mask)
      else => return Err("invalid mode string")
    }
  }
  Ok(result)
}

pure parse_mode(spec: Str, is_dir: Bool) -> Result[Int, Str] {
  let spec = spec.trim()
  if "+" in spec or "-" in spec or "=" in spec {
    return symbolic_mode(spec, is_dir)
  }
  if spec == "" { return Err("invalid digit found in string") }
  var value = 0
  for ch in spec {
    let digit = mode_digit(ch)
    if digit == null { return Err("invalid digit found in string") }
    value = value * 8 + (digit ?? 0)
  }
  Ok(value)
}

proc occupied(target_path: Path) [fs] -> Bool {
  match fs.stat(target_path) {
    Ok(_) => true
    Err(_) => false
  }
}

proc same_file(left: Path, right: Path) [fs] -> Bool {
  match fs.stat(left, follow_symlinks: true) {
    Ok(a) => {
      match fs.stat(right, follow_symlinks: true) {
        Ok(b) => a.dev == b.dev and a.ino == b.ino
        Err(_) => false
      }
    }
    Err(_) => false
  }
}

proc numbered_backup(name: Str, suffix: Str) [fs] -> Path {
  var n = 1
  while n < 10000 {
    let candidate = fp"{name}.~{n}~"
    if ! occupied(candidate) { return candidate }
    n += 1
  }
  fp"{name}{suffix}"
}

proc backup_name(name: Str, mode: Str, suffix: Str) [fs] -> Path {
  if mode == "numbered" { return numbered_backup(name, suffix) }
  if (mode == "existing" or mode == "nil") and (occupied(fp"{name}.~1~") or occupied(fp"{name}.~2~")) {
    return numbered_backup(name, suffix)
  }
  fp"{name}{suffix}"
}

proc resolve_owner(name: Str) [fs, process, env] -> Int? {
  return null when name == ""
  if let Ok(id) = name.parse_int() {
    if let Ok(found) = user.by_uid(id) { return found.uid }
  }
  if let Ok(found) = user.lookup(name) { return found.uid }
  gnu.error(f"invalid user: {gnu.quote(name)}")
  exit 1
}

proc resolve_group(name: Str) [fs, process, env] -> Int? {
  return null when name == ""
  if let Ok(id) = name.parse_int() {
    if let Ok(found) = group.by_gid(id) { return found.gid }
  }
  if let Ok(found) = group.lookup(name) { return found.gid }
  gnu.error(f"invalid group: {gnu.quote(name)}")
  exit 1
}

proc mkdir_parents(target_path: Path, verbose: Bool) [fs, process, env, io, error] -> Bool {
  if target_path.display() == "/" { return true }
  if occupied(target_path) {
    match fs.stat(target_path, follow_symlinks: true) {
      Ok(meta) if meta.kind == "dir" => return true
      Ok(_) => {
        gnu.error(f"cannot create directory {gnu.quote(target_path.display())}: File exists")
        return false
      }
      Err(failure) => {
        gnu.error(f"cannot create directory {gnu.quote(target_path.display())}: {gnu.strerror(failure)}")
        return false
      }
    }
  }
  let parent = target_path.parent()
  if parent.display() != target_path.display() and ! mkdir_parents(parent, verbose) { return false }
  match fs.mkdir(target_path) {
    Ok(_) => {
      if verbose { gnu.write_text(f"install: creating directory {gnu.quote(target_path.display())}\n") }
      true
    }
    Err(_) => {
      match fs.stat(target_path, follow_symlinks: true) {
        Ok(meta) if meta.kind == "dir" => true
        Ok(_) => {
          gnu.error(f"cannot create directory {gnu.quote(target_path.display())}: File exists")
          false
        }
        Err(failure) => {
          gnu.error(f"cannot create directory {gnu.quote(target_path.display())}: {gnu.strerror(failure)}")
          false
        }
      }
    }
  }
}

proc set_attrs(target_path: Path, mode: Int, owner: Int?, group_id: Int?, options: InstallOptions) [fs, process, env, error] -> Bool {
  if ! options.unprivileged and (owner != null or group_id != null) {
    if let Err(failure) = fs.set_owner(target_path, uid: owner, gid: group_id) {
      gnu.error(f"cannot change ownership of {gnu.quote(target_path.display())}: {gnu.strerror(failure)}")
      return false
    }
  }
  if let Err(failure) = fs.chmod(target_path, mode) {
    gnu.error(f"cannot change permissions of {gnu.quote(target_path.display())}: {gnu.strerror(failure)}")
    return false
  }
  true
}

pure normalize_directory(name: Str) -> Str {
  let absolute = name.starts_with("/")
  var parts = []
  for part in name.split("/") {
    continue when part == "" or part == "."
    if part == ".." and parts.len() > 0 and parts[parts.len() - 1] != ".." {
      parts = parts |> take(parts.len() - 1)
    } else if part != ".." or ! absolute {
      parts += [part]
    }
  }
  let joined = parts.join("/")
  if absolute { f"/{joined}" } else if joined == "" { "." } else { joined }
}

proc install_directory(name: Str, mode: Int, owner: Int?, group_id: Int?, options: InstallOptions) [fs, process, env, io, error] -> Bool {
  var prefix = []
  for part in name.split("/") {
    continue when part == "" or part == "."
    if part == ".." {
      if prefix.len() > 0 and prefix[prefix.len() - 1] != ".." {
        let prefix_name = prefix.join("/")
        let parent_to_create = if name.starts_with("/") { f"/{prefix_name}" } else { prefix_name }
        if ! mkdir_parents(fp"{parent_to_create}", options.verbose) { return false }
        prefix = prefix |> take(prefix.len() - 1)
      } else if ! name.starts_with("/") {
        prefix += [".."]
      }
    } else {
      prefix += [part]
    }
  }
  let normalized = normalize_directory(name)
  let keep_inner_dots = ! name.ends_with("/.") and ! name.ends_with("/..") and ! name.ends_with("/")
  let target_path = fp"{if keep_inner_dots { name } else { normalized }}"
  if ! mkdir_parents(target_path, options.verbose) { return false }
  set_attrs(target_path, mode, owner, group_id, options)
}

pure normalized_name(name: Str, cwd: Str) -> Str {
  let absolute = if name.starts_with("/") { name } else { f"{cwd}/{name}" }
  var parts = []
  for part in absolute.split("/") {
    continue when part == "" or part == "."
    if part == ".." {
      if parts.len() > 0 { parts = parts |> take(parts.len() - 1) }
    } else {
      parts += [part]
    }
  }
  f"/{parts.join("/")}"
}

pure trim_slashes(name: Str) -> Str {
  var end = name.byte_len()
  while end > 1 and name.byte_slice(end - 1, length: 1) == "/" { end -= 1 }
  name.byte_slice(0, length: end)
}

proc source_bytes(source: Path) [fs, error] -> Result[Bytes] {
  source.read_bytes()
}

proc install_one(source_name: Str, dest: Path, mode: Int, owner: Int?, group_id: Int?, backup_mode: Str, suffix: Str, options: InstallOptions) [fs, process, env, io, error] -> Bool {
  let source = fp"{source_name}"
  let dest_name = dest.display()
  let source_is_symlink = match fs.stat(source, follow_symlinks: false) {
    Ok(meta) => meta.kind == "symlink"
    Err(_) => false
  }
  let source_stat = match fs.stat(source, follow_symlinks: true) {
    Ok(meta) => meta
    Err(failure) => {
      gnu.error(f"cannot stat {gnu.quote(source_name)}: {gnu.strerror(failure)}")
      return false
    }
  }
  if source_stat.kind == "dir" {
    gnu.error(f"omitting directory {gnu.quote(source_name)}")
    return false
  }
  if same_file(source, dest) {
    gnu.error(f"{gnu.quote(source_name)} and {gnu.quote(dest_name)} are the same file")
    return false
  }
  let dest_stat: InstallStat? = match fs.stat(dest) {
    Ok(meta) => {kind: meta.kind, dev: meta.dev, ino: meta.ino, mode: meta.mode, uid: meta.uid, gid: meta.gid, mtime_ns: meta.mtime_ns}
    Err(_) => null
  }
  if dest_stat != null {
    let old = dest_stat ?? {kind: "", dev: 0, ino: 0, mode: 0, uid: 0, gid: 0, mtime_ns: 0}
    if old.kind == "dir" {
      gnu.error(f"cannot overwrite directory {gnu.quote(dest_name)} with non-directory")
      return false
    }
  }

  if options.compare and dest_stat != null and mode.bit_and(0o7000) == 0 {
    let same_content = match source.read_bytes() {
      Ok(source_data) => match dest.read_bytes() {
        Ok(dest_data) => source_data == dest_data
        Err(_) => false
      }
      Err(_) => false
    }
    let old = dest_stat ?? {kind: "", dev: 0, ino: 0, mode: 0, uid: 0, gid: 0, mtime_ns: 0}
    let same_mode = old.mode.bit_and(0o7777) == mode.bit_and(0o7777)
    let same_owner = owner == null or old.uid == (owner ?? -2)
    let same_group = group_id == null or old.gid == (group_id ?? -2)
    let times_match = ! options.preserve_timestamps or old.mtime_ns == source_stat.mtime_ns
    if old.kind == "file" and same_content and same_mode and same_owner and same_group and times_match { return true }
  }

  var backup: Path? = null
  if dest_stat != null and backup_mode != "none" {
    let candidate = backup_name(dest_name, backup_mode, suffix)
    let cwd = fs.cwd()?.display()
    if normalized_name(source_name, cwd) == normalized_name(candidate.display(), cwd) {
      gnu.error(f"backing up {gnu.quote(dest_name)} might destroy source;  {gnu.quote(source_name)} not copied")
      return false
    }
    match fs.rename(dest, candidate, overwrite: true) {
      Ok(_) => backup = candidate
      Err(failure) => {
        gnu.error(f"cannot backup {gnu.quote(dest_name)}: {gnu.strerror(failure)}")
        return false
      }
    }
  } else if dest_stat != null {
    if options.verbose { gnu.write_text(f"removed {gnu.quote(dest_name)}\n") }
    if let Err(failure) = fs.remove(dest) {
      gnu.error(f"cannot remove {gnu.quote(dest_name)}: {gnu.strerror(failure)}")
      return false
    }
  }

  let initial_mode = if ! options.unprivileged and (owner != null or group_id != null) { mode.clear_bits(0o6000) } else { mode }
  let copy_result = if source_stat.kind == "file" and ! source_is_symlink {
    fs.install(source, dest, initial_mode, parents: false, overwrite: true)
  } else {
    let read_result = if source_name == "/dev/fd/0" { Ok(io.stdin_bytes() ?? b"") } else { source_bytes(source) }
    match read_result {
      Ok(contents) => {
        match dest.write(contents) {
          Ok(_) => fs.chmod(dest, initial_mode)
          Err(failure) => Err(failure)
        }
      }
      Err(failure) => Err(failure)
    }
  }
  if let Err(failure) = copy_result {
    if backup != null {
      let saved = backup ?? dest
      let _ = fs.rename(saved, dest, overwrite: true)
    }
    gnu.error(f"cannot install {gnu.quote(source_name)}: {gnu.strerror(failure)}")
    return false
  }

  if ! set_attrs(dest, mode, owner, group_id, options) { return false }
  if options.preserve_timestamps {
    let copied_source = match fs.stat(source, follow_symlinks: true) { Ok(meta) => meta, Err(_) => source_stat }
    if let Err(failure) = fs.set_times(dest, atime_ns: copied_source.atime_ns, mtime_ns: copied_source.mtime_ns) {
      gnu.error(f"cannot preserve timestamps for {gnu.quote(dest_name)}: {gnu.strerror(failure)}")
      return false
    }
  }
  if options.strip {
    let program = if options.strip_program == "" { "strip" } else { options.strip_program }
    let program_missing = "/" in program and ! occupied(fp"{program}")
    let strip_path = if dest_name.starts_with("-") { f"./{dest_name}" } else { dest_name }
    match process.run(process.command_argv(program, [program, strip_path])) {
      Ok(status) if status.exited() and status.exit_code()? == 0 => {}
      Ok(status) if status.signaled() => {
        let _ = fs.remove(dest, missing_ok: true)
        gnu.error("strip process terminated abnormally")
        return false
      }
      Ok(status) if status.exited() and status.exit_code()? == 127 => {
        let _ = fs.remove(dest, missing_ok: true)
        gnu.error("strip program failed: No such file or directory")
        return false
      }
      Ok(_) => {
        let _ = fs.remove(dest, missing_ok: true)
        if program_missing {
          gnu.error("strip program failed: No such file or directory")
        } else {
          gnu.error("strip program failed")
        }
        return false
      }
      Err(_) => {
        let _ = fs.remove(dest, missing_ok: true)
        gnu.error("strip program failed: No such file or directory")
        return false
      }
    }
  } else if options.strip_program != "" {
    gnu.error("WARNING: ignoring --strip-program option as -s option was not specified")
  }
  if options.verbose { gnu.write_text(f"{gnu.quote(source_name)} -> {gnu.quote(dest_name)}\n") }
  true
}

proc main(...argv: List[Str]) [fs, process, env, io, error] {
  let options: InstallOptions = cli.applet(
    argv,
    {
      gnu: {status: 1, unsupported: {
        "-P": "SELinux contexts are not available",
        "-Z": "SELinux contexts are not available",
        "--preserve-context": "SELinux contexts are not available",
        "--default-context": "SELinux contexts are not available",
        "--context": "SELinux contexts are not available",
      }},
      # GNU install documents -c as a compatibility option with no effect.
      gnu_copy: {form: "-c", default: false},
      compare: {form: "-C --compare", default: false},
      directory: {form: "-d --directory", default: false},
      create_leading: {form: "-D", default: false},
      group: {form: "-g --group GROUP", repeated: true},
      mode: {form: "-m --mode MODE", repeated: true},
      owner: {form: "-o --owner OWNER", repeated: true},
      preserve_timestamps: {form: "-p --preserve-timestamps", default: false},
      strip: {form: "-s --strip", default: false},
      suffix: {form: "-S --suffix SUFFIX", default: ""},
      target: {form: "-t --target-directory DIR", repeated: true},
      no_target_directory: {form: "-T --no-target-directory", default: false},
      unprivileged: {form: "-U --unprivileged", default: false},
      verbose: {form: "-v --verbose", default: false},
      backup: {form: "-b --backup[=CONTROL]", default: "", optional_default: "@env"},
      strip_program: {form: "--strip-program PROGRAM", default: ""},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      operands: {form: "...FILE"},
    },
  )?

  if options.help { gnu.help(USAGE); return }
  if options.version { gnu.version("install"); return }
  if options.target.len() > 1 { gnu.error("multiple target directories specified"); exit 1 }
  if options.target.len() > 0 and options.no_target_directory {
    gnu.error("Options --target-directory and --no-target-directory are mutually exclusive")
    gnu.try_help()
    exit 1
  }
  if options.compare and options.strip { gnu.error("Options --compare and --strip are mutually exclusive"); exit 1 }

  let mode_spec = if options.mode.len() == 0 { "755" } else { options.mode[options.mode.len() - 1] }
  let file_mode = match parse_mode(mode_spec, false) {
    Ok(value) => value
    Err(reason) => { gnu.error(f"Invalid mode string: {reason}"); exit 1 }
  }
  let directory_mode = if options.mode.len() == 0 { 0o755 } else {
    match parse_mode(mode_spec, true) {
      Ok(value) => value
      Err(reason) => { gnu.error(f"Invalid mode string: {reason}"); exit 1 }
    }
  }
  if options.compare and file_mode.bit_and(0o7000) != 0 {
    gnu.error("the --compare (-C) option is ignored when you specify a mode with non-permission bits")
  }

  let owner_name = if options.owner.len() == 0 { "" } else { options.owner[options.owner.len() - 1] }
  let group_name = if options.group.len() == 0 { "" } else { options.group[options.group.len() - 1] }
  let owner = resolve_owner(owner_name)
  let group_id = resolve_group(group_name)

  var backup_mode = options.backup
  if backup_mode == "@env" { backup_mode = env.get_or("VERSION_CONTROL", "existing") ?? "existing" }
  if backup_mode == "" and options.suffix != "" { backup_mode = env.get_or("VERSION_CONTROL", "existing") ?? "existing" }
  if backup_mode == "" { backup_mode = "none" }
  if backup_mode == "never" { backup_mode = "simple" }
  if backup_mode == "t" { backup_mode = "numbered" }
  if backup_mode == "nil" { backup_mode = "existing" }
  if backup_mode == "off" { backup_mode = "none" }
  if backup_mode not in ["none", "simple", "numbered", "existing"] {
    gnu.error(f"invalid backup type {gnu.quote_value(backup_mode)}")
    exit 1
  }
  let suffix_env = env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~"
  let suffix = if options.suffix != "" { options.suffix } else { suffix_env }
  let operands = options.operands

  if options.directory {
    if operands.len() == 0 { gnu.error("missing file operand"); gnu.try_help(); exit 1 }
    if options.target.len() > 0 { gnu.usage_error("target directory not allowed when installing a directory") }
    if options.strip { gnu.error("the strip option may not be used when installing a directory"); exit 1 }
    if backup_mode != "none" { gnu.error("backup not allowed when installing a directory"); exit 1 }
    var failed = false
    for name in operands {
      if ! install_directory(name, directory_mode, owner, group_id, options) { failed = true }
    }
    if failed { exit 1 }
    return
  }

  if operands.len() == 0 { gnu.error("missing file operand"); gnu.try_help(); exit 1 }
  if options.target.len() == 0 and operands.len() == 1 { gnu.error(f"missing destination file operand after {gnu.quote(operands[0])}"); gnu.try_help(); exit 1 }
  var sources = operands
  var target = ""
  if options.target.len() > 0 {
    target = options.target[0]
  } else {
    target = operands[operands.len() - 1]
    sources = operands |> take(operands.len() - 1)
  }

  var target_directory = options.target.len() > 0
  if options.target.len() > 0 {
    let target_for_stat = if target.ends_with("/") { trim_slashes(target) } else { target }
    if options.create_leading and ! occupied(fp"{target_for_stat}") {
      if ! mkdir_parents(fp"{target_for_stat}", options.verbose) { exit 1 }
    }
    match fs.stat(fp"{target_for_stat}", follow_symlinks: true) {
      Ok(meta) if meta.kind == "dir" => {}
      Err(failure) => { gnu.error(f"failed to access {gnu.quote(target)}: {gnu.strerror(failure)}"); exit 1 }
      _ => { gnu.error(f"failed to access {gnu.quote(target)}: Not a directory"); exit 1 }
    }
    target_directory = true
  } else if ! options.no_target_directory {
    match fs.stat(fp"{target}", follow_symlinks: true) {
      Ok(meta) => target_directory = meta.kind == "dir"
      Err(_) => target_directory = false
    }
  }
  if options.no_target_directory { target_directory = false }
  if sources.len() > 1 and ! target_directory {
    if options.no_target_directory {
      gnu.error(f"extra operand {gnu.quote(operands[2])}")
      gnu.error("usage: install [OPTION]... [FILE]...")
    } else {
      gnu.error(f"target {gnu.quote(target)} is not a directory")
    }
    exit 1
  }

  if options.create_leading and options.target.len() == 0 {
    let trailing_slash = target.ends_with("/")
    if trailing_slash and ! target_directory {
      gnu.error(f"{gnu.quote(target)} is not a directory")
      exit 1
    }
    if ! target_directory and ! trailing_slash and sources.len() == 1 {
      let parent = fp"{target}".parent()
      if parent.display() != "." and ! mkdir_parents(parent, options.verbose) {
        gnu.error(f"cannot create directory {gnu.quote(parent.display())}")
        exit 1
      }
    }
  }

  var failed = false
  var installed = []
  for source_name in sources {
    let source = fp"{source_name}"
    let destination = if target_directory {
      fp"{trim_slashes(target)}/{source.basename()}"
    } else {
      fp"{target}"
    }
    if backup_mode != "numbered" and destination.display() in installed {
      gnu.error(f"will not overwrite just-created {gnu.quote(destination.display())} with {gnu.quote(source_name)}")
      failed = true
      continue
    }
    if install_one(source_name, destination, file_mode, owner, group_id, backup_mode, suffix, options) {
      installed += [destination.display()]
    } else {
      failed = true
    }
  }
  if failed { exit 1 }
}
