#!/bin/xsh
use lib.gnu

const USAGE = """Usage: ln [OPTION]... [-T] TARGET LINK_NAME
  or:  ln [OPTION]... TARGET
  or:  ln [OPTION]... TARGET... DIRECTORY
  or:  ln [OPTION]... -t DIRECTORY TARGET...
In the 1st form, create a link to TARGET with the name LINK_NAME.
In the 2nd form, create a link to TARGET in the current directory.
In the 3rd and 4th forms, create links to each TARGET in DIRECTORY.

  -s, --symbolic          make symbolic links instead of hard links
  -f, --force             remove existing destination files
  -i, --interactive       prompt whether to remove destinations
  -L, --logical           dereference TARGETs that are symbolic links
  -P, --physical          make hard links directly to symbolic links
  -r, --relative          create symbolic links relative to link location
  -n, --no-dereference    treat LINK_NAME as a normal file if it is a symlink to a directory
  -t, --target-directory=DIRECTORY  specify the DIRECTORY in which to create links
  -T, --no-target-directory  treat LINK_NAME as a normal file
  -b, --backup[=CONTROL]  make a backup of each existing destination file
  -S, --suffix=SUFFIX     override the usual backup suffix
  -v, --verbose           print the name of each linked file
      --help              display this help and exit
      --version           output version information and exit
"""

type LnOptions = {
  force: Bool,
  interactive: Bool,
  symbolic: Bool,
  logical: Bool,
  physical: Bool,
  relative: Bool,
  no_target_directory: Bool,
  no_dereference: Bool,
  verbose: Bool,
  target: Str,
  backup: Str,
  suffix: Str,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

pure path_parts(raw: Str, cwd: Str) -> List[Str] {
  let absolute = if raw.starts_with("/") { raw } else { f"{cwd}/{raw}" }
  var parts = []

  for item in absolute.split("/") {
    continue when item == "" or item == "."

    if item == ".." {
      if parts.len() > 0 {
        parts = parts |> take(parts.len() - 1)
      }
    } else {
      parts += [item]
    }
  }

  parts
}

pure relative_name(source: Str, destination: Path, cwd: Str) -> Str {
  let source_parts = path_parts(source, cwd)
  let parent_parts = path_parts(destination.parent().display(), cwd)
  var common = 0

  while common < source_parts.len() and common < parent_parts.len() and source_parts[common] == parent_parts[common] {
    common += 1
  }

  var parts = []
  for _ in range(parent_parts.len() - common) {
    parts += [".."]
  }
  parts = parts.extend(source_parts |> drop(common))

  if parts.len() == 0 { "." } else { parts.join("/") }
}

proc occupied(target: Path) [fs] -> Bool {
  match fs.stat(target) {
    Ok(_) => true
    Err(_) => false
  }
}

proc numbered_backup(name: Str, suffix: Str) [fs] -> Path {
  var number = 1
  while number < 10000 {
    let candidate = fp"{name}.~{number}~"
    if ! occupied(candidate) {
      return candidate
    }
    number += 1
  }

  fp"{name}{suffix}"
}

proc backup_name(name: Str, mode: Str, suffix: Str) [fs] -> Path {
  if mode == "numbered" or mode == "t" {
    return numbered_backup(name, suffix)
  }

  let existing_numbered = occupied(fp"{name}.~1~")

  if (mode == "existing" or mode == "nil") and existing_numbered {
    return numbered_backup(name, suffix)
  }

  fp"{name}{suffix}"
}

proc same_inode(left: Path, right: Path) [fs] -> Bool {
  match fs.stat(left) {
    Ok(a) => {
      match fs.stat(right) {
        Ok(b) => a.dev == b.dev and a.ino == b.ino
        Err(_) => false
      }
    }
    Err(_) => false
  }
}

pure normalized_path(name: Str) -> Str {
  let absolute = name.starts_with("/")
  var parts = []
  for part in name.split("/") {
    continue when part == "" or part == "."
    if part == ".." {
      if parts.len() > 0 and parts[parts.len() - 1] != ".." {
        parts = parts |> take(parts.len() - 1)
      } else if ! absolute {
        parts += [".."]
      }
    } else {
      parts += [part]
    }
  }
  let joined = parts.join("/")
  if absolute { f"/{joined}" } else if joined == "" { "." } else { joined }
}

pure same_path(left: Str, right: Str) -> Bool {
  normalized_path(left) == normalized_path(right)
}

proc prompt_replace(name: Str) [process, io] -> Bool {
  eprint f"ln: replace {gnu.quote(name)}? "
  let answer = io.stdin_bytes() ?? b""
  return false when answer.len() == 0

  let first = answer.byte_at(0) ?? 0
  first == 121 or first == 89
}

proc report_link_error(source: Str, dest: Str, symbolic: Bool, message: Str) [process, env] -> Unit {
  let operation = if symbolic {
    f"failed to create symbolic link {gnu.quote(dest)} -> {gnu.quote(source)}"
  } else {
    f"failed to create hard link {gnu.quote(dest)} => {gnu.quote(source)}"
  }
  gnu.error(f"{operation}: {message}")
}

proc link_one(source_text: Str, dest: Path, options: LnOptions, backup_mode: Str, suffix: Str) [fs, process, env, io, error] -> Bool {
  let source = fp"{source_text}"
  let dest_text = dest.display()

  if ! options.symbolic {
    match fs.stat(source, follow_symlinks: options.logical) {
      Ok(_) => {}
      Err(failure) => {
        gnu.error(f"failed to access {gnu.quote(source_text)}: {gnu.strerror(failure)}")
        return false
      }
    }
  }

  let exists = occupied(dest)
  let backup = if exists and backup_mode != "none" { backup_name(dest_text, backup_mode, suffix) } else { dest }
  let has_backup = exists and backup_mode != "none"

  if exists and ! has_backup and options.interactive and ! prompt_replace(dest_text) {
    return false
  }

  if exists and ! has_backup and ! (options.force or options.interactive) {
    report_link_error(source_text, dest_text, options.symbolic, "File exists")
    return false
  }

  if ! options.symbolic and same_inode(source, dest) {
    if options.force and ! has_backup and ! same_path(source_text, dest_text) { return true }
    gnu.error(f"{gnu.quote(source_text)} and {gnu.quote(dest_text)} are the same file")
    return false
  }

  var moved_backup = false
  if has_backup {
    match fs.rename(dest, backup, overwrite: true) {
      Ok(_) => moved_backup = true
      Err(failure) => {
        gnu.error(f"cannot backup {gnu.quote(dest_text)}: {gnu.strerror(failure)}")
        return false
      }
    }
  }

  let link_source = if options.relative and options.symbolic {
    let cwd = fs.cwd()?.display()
    relative_name(source_text, dest, cwd)
  } else {
    source_text
  }

  if exists and (options.force or options.interactive or has_backup) {
    var index = 0
    var temp: Path? = null
    var link_failed = false
    var link_message = ""

    while index < 100 and temp == null and ! link_failed {
      let candidate = fp"{dest_text}.xsh-tmp-{index}"
      let attempt = if options.symbolic {
        fs.symlink(fp"{link_source}", candidate)
      } else {
        fs.link(source, candidate, follow_symlinks: options.logical)
      }

      match attempt {
        Ok(_) => temp = candidate
        Err(failure) if gnu.errno(failure) == 17 => index += 1
        Err(failure) => {
          link_failed = true
          link_message = gnu.strerror(failure)
        }
      }
    }

    if link_failed {
      if moved_backup { let _ = fs.rename(backup, dest, overwrite: true) }
      report_link_error(link_source, dest_text, options.symbolic, link_message)
      return false
    }

    if temp == null {
      if moved_backup { let _ = fs.rename(backup, dest, overwrite: true) }
      report_link_error(link_source, dest_text, options.symbolic, "File exists")
      return false
    }

    let temp_path = temp ?? dest
    if let Err(failure) = fs.rename(temp_path, dest, overwrite: true) {
      let _ = temp_path.remove(missing_ok: true)
      if moved_backup { let _ = fs.rename(backup, dest, overwrite: true) }
      report_link_error(link_source, dest_text, options.symbolic, gnu.strerror(failure))
      return false
    }
  } else {
    let result = if options.symbolic {
      fs.symlink(fp"{link_source}", dest)
    } else {
      fs.link(source, dest, follow_symlinks: options.logical)
    }
    if let Err(failure) = result {
      if moved_backup { let _ = fs.rename(backup, dest, overwrite: true) }
      report_link_error(link_source, dest_text, options.symbolic, gnu.strerror(failure))
      return false
    }
  }

  if options.verbose {
    gnu.write_text(f"{gnu.quote(dest_text)} {if options.symbolic { "->" } else { "=>" }} {gnu.quote(link_source)}\n")
  }

  true
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: LnOptions = cli.applet(
    argv,
    {
      gnu: {status: 1, unsupported: {"-d": "directory hard links are not supported"}},
      force: {form: "-f --force", default: false, conflicts: "interactive"},
      interactive: {form: "-i --interactive", default: false, conflicts: "force"},
      no_dereference: {form: "-n --no-dereference", default: false},
      logical: {form: "-L --logical", default: false, conflicts: "physical"},
      physical: {form: "-P --physical", default: true, conflicts: "logical"},
      symbolic: {form: "-s --symbolic", default: false},
      relative: {form: "-r --relative", default: false},
      target: {form: "-t --target-directory DIR", default: ""},
      no_target_directory: {form: "-T --no-target-directory", default: false},
      verbose: {form: "-v --verbose", default: false},
      backup: {form: "-b --backup[=CONTROL]", default: "", optional_default: "simple"},
      suffix: {form: "-S --suffix SUFFIX", default: ""},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      operands: {form: "...FILE"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }
  if opts.version {
    gnu.version("ln")
    return
  }
  if opts.relative and ! opts.symbolic {
    gnu.usage_error("--relative is only meaningful with --symbolic")
  }

  var backup_mode = opts.backup
  if backup_mode == "" {
    backup_mode = env.get_or("VERSION_CONTROL", "none") ?? "none"
  }
  if backup_mode == "" {
    backup_mode = "simple"
  }
  if backup_mode == "never" { backup_mode = "simple" }
  if backup_mode == "t" { backup_mode = "numbered" }
  if backup_mode == "nil" { backup_mode = "existing" }
  if backup_mode == "off" { backup_mode = "none" }
  if backup_mode not in ["none", "simple", "numbered", "existing"] {
    gnu.error(f"invalid backup type {gnu.quote_value(backup_mode)}")
    exit 1
  }

  let suffix_env = env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~"
  let suffix = if opts.suffix != "" { opts.suffix } else { suffix_env }
  let operands = opts.operands
  if operands.len() == 0 { gnu.missing_operand() }
  if opts.target != "" and opts.no_target_directory {
    gnu.usage_error("cannot combine --target-directory and --no-target-directory")
  }

  var directory: Path? = null
  var sources = operands
  if opts.target != "" {
    directory = fp"{opts.target}"
  } else if ! opts.no_target_directory {
    if operands.len() == 1 {
      directory = p"."
    } else {
      let last = fp"{operands[operands.len() - 1]}"
      let is_dir = match fs.stat(last, follow_symlinks: ! (opts.symbolic and opts.no_dereference)) {
        Ok(meta) => meta.kind == "dir"
        Err(_) => false
      }
      if is_dir {
        directory = last
        sources = operands |> take(operands.len() - 1)
      }
    }
  }

  if directory != null {
    let target_directory = directory ?? p"."
    match fs.stat(target_directory) {
      Ok(meta) if meta.kind == "dir" => {}
      Ok(_) => {
        gnu.error(f"target {gnu.quote(target_directory.display())} is not a directory")
        exit 1
      }
      Err(failure) => {
        gnu.error(f"target {gnu.quote(target_directory.display())} is not a directory: {gnu.strerror(failure)}")
        exit 1
      }
    }
  } else if operands.len() == 1 {
    gnu.missing_operand_after(operands[0])
  } else if operands.len() > 2 {
    gnu.extra_operand(operands[1])
  }

  let actual_suffix = suffix
  var failed = false

  if directory != null {
    let target_directory = directory ?? p"."
    for source in sources {
      let target = fp"{target_directory}/{fp"{source}".basename()}"
      if ! link_one(source, target, opts, backup_mode, actual_suffix) { failed = true }
    }
  } else {
    let source = operands[0]
    let target = fp"{operands[1]}"
    if ! link_one(source, target, opts, backup_mode, actual_suffix) { failed = true }
  }

  if failed { exit 1 }
}
