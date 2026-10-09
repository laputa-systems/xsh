#!/bin/xsh
use lib.gnu

const USAGE = """Usage: mv [OPTION]... [-T] SOURCE DEST
  or:  mv [OPTION]... SOURCE... DIRECTORY
  or:  mv [OPTION]... -t DIRECTORY SOURCE...
Rename SOURCE to DEST, or move SOURCEs to DIRECTORY.

  -f, --force                 do not prompt before overwriting
  -i, --interactive           prompt before overwrite
  -n, --no-clobber            do not overwrite an existing file
  -b, --backup[=CONTROL]      make a backup of each existing destination file
  -S, --suffix=SUFFIX         override the usual backup suffix
  -t, --target-directory=DIR  move all SOURCE arguments into DIR
  -T, --no-target-directory   treat DEST as a normal file
  -u, --update[=UPDATE]       control which existing files are updated
      --strip-trailing-slashes  remove trailing slashes from each SOURCE
  -v, --verbose               explain what is being done
      --help                  display this help and exit
      --version               output version information and exit
"""

type MvOptions = {
  force: Bool,
  interactive: Bool,
  no_clobber: Bool,
  no_target_directory: Bool,
  no_copy: Bool,
  strip_trailing_slashes: Bool,
  verbose: Bool,
  target: List[Str],
  backup: Str,
  suffix: Str,
  update: Str,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

type MvStat = {found: Bool, kind: Str, mtime_ns: Int, failure: Str}

pure strip_slashes(raw: Str) -> Str {
  var end = raw.byte_len()
  while end > 1 and raw.byte_slice(end - 1, length: 1) == "/" {
    end -= 1
  }
  raw.byte_slice(0, length: end)
}

pure dest_for(source: Path, target: Path, target_is_dir: Bool) -> Path {
  if target_is_dir { fp"{target}/{source.basename()}" } else { target }
}

pure overwrite_choice(args: List[Str]) -> Str {
  var choice = "force"
  var at = 0
  while at < args.len() {
    let item = args[at]
    if item == "--" { break }
    if item == "-t" or item == "-S" or item == "--target-directory" or item == "--suffix" {
      at += 2
      continue
    }
    if item.starts_with("--") {
      let option = item.split("=")[0]
      if option == "--force" or "--force".starts_with(option) { choice = "force" }
      if option == "--interactive" or "--interactive".starts_with(option) { choice = "interactive" }
      if option == "--no-clobber" or "--no-clobber".starts_with(option) { choice = "no_clobber" }
    } else if item.starts_with("-") and item != "-" {
      var index = 1
      while index < item.byte_len() {
        let option = item.byte_slice(index, length: 1)
        if option == "f" { choice = "force" }
        if option == "i" { choice = "interactive" }
        if option == "n" { choice = "no_clobber" }
        break when option == "t" or option == "S"
        index += 1
      }
    }
    at += 1
  }
  choice
}

proc stat_optional(target: Path) [fs, env, process] -> MvStat {
  match fs.stat(target) {
    Ok(meta) => {found: true, kind: meta.kind, mtime_ns: meta.mtime_ns, failure: ""}
    Err(failure) if gnu.errno(failure) == 2 => {found: false, kind: "", mtime_ns: 0, failure: ""}
    Err(failure) => {
      gnu.error(f"cannot stat {gnu.quote(target.display())}: {gnu.strerror(failure)}")
      {found: false, kind: "", mtime_ns: 0, failure: failure.message}
    }
  }
}

proc numbered_backup(name: Str, suffix: Str) [fs] -> Path {
  var number = 1
  while number < 10000 {
    let candidate = fp"{name}.~{number}~"
    match fs.stat(candidate) {
      Err(_) => return candidate
      Ok(_) => number += 1
    }
  }
  fp"{name}{suffix}"
}

proc occupied(target: Path) [fs] -> Bool {
  match fs.stat(target) {
    Ok(_) => true
    Err(_) => false
  }
}

proc backup_name(name: Str, mode: Str, suffix: Str) [fs] -> Path {
  if mode == "numbered" { return numbered_backup(name, suffix) }
  if (mode == "existing" or mode == "nil") and (occupied(fp"{name}.~1~") or occupied(fp"{name}.~2~")) {
    return numbered_backup(name, suffix)
  }
  fp"{name}{suffix}"
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

proc prompt_overwrite(name: Str) [process, env, io] -> Bool {
  eprint f"mv: overwrite {gnu.quote(name)}? "
  let answer = io.stdin_bytes() ?? b""
  return false when answer.len() == 0
  let first = answer.byte_at(0) ?? 0
  first == 121 or first == 89
}

proc backup_mode_from(opts: MvOptions) [env, process] -> Str {
  var mode = opts.backup
  if mode == "@env" {
    mode = env.get_or("VERSION_CONTROL", "existing") ?? "existing"
  } else if mode == "" and opts.suffix != "" {
    mode = env.get_or("VERSION_CONTROL", "existing") ?? "existing"
  } else if mode == "" {
    mode = "none"
  }
  if mode == "never" { mode = "simple" }
  if mode == "t" { mode = "numbered" }
  if mode == "nil" { mode = "existing" }
  if mode == "off" { mode = "none" }
  if mode not in ["none", "simple", "numbered", "existing"] {
    gnu.error(f"invalid backup type {gnu.quote_value(mode)}")
    exit 1
  }
  mode
}

proc move_one(source_name: Str, destination: Path, opts: MvOptions, overwrite: Str, backup_mode: Str, suffix: Str) [fs, process, env, io, error] -> Bool {
  let source = fp"{source_name}"
  let dest_name = destination.display()
  let source_state = stat_optional(source)
  if ! source_state.found {
    if source_state.failure == "" {
      gnu.error(f"cannot stat {gnu.quote(source_name)}: No such file or directory")
    }
    return false
  }
  let target_state = stat_optional(destination)
  if ! target_state.found and target_state.failure != "" {
    return false
  }
  let dest_exists = target_state.found

  if dest_exists {
    if overwrite == "no_clobber" {
      return true
    }
    if opts.update == "none" or opts.update == "none-fail" {
      if opts.update == "none-fail" {
        gnu.error(f"not replacing {gnu.quote(dest_name)}")
        return false
      }
      return true
    }
    if opts.update == "older" and source_state.mtime_ns <= target_state.mtime_ns {
      return true
    }
    if same_file(source, destination) and backup_mode == "none" {
      return true
    }
    if overwrite == "interactive" and ! prompt_overwrite(dest_name) {
      return true
    }
    if source_state.kind == "dir" and target_state.kind != "dir" {
      gnu.error(f"cannot overwrite non-directory {gnu.quote(dest_name)} with directory {gnu.quote(source_name)}")
      return false
    }
    if source_state.kind != "dir" and target_state.kind == "dir" {
      gnu.error(f"cannot overwrite directory {gnu.quote(dest_name)} with non-directory")
      return false
    }
  }

  var moved_backup = false
  var backup_path: Path? = null
  if dest_exists and backup_mode != "none" {
    if same_file(source, destination) {
      gnu.error(f"{gnu.quote(source_name)} and {gnu.quote(dest_name)} are the same file")
      return false
    }
    let backup = backup_name(dest_name, backup_mode, suffix)
    backup_path = backup
    if same_file(source, backup) {
      gnu.error(f"backing up {gnu.quote(dest_name)} might destroy source; {gnu.quote(source_name)} not moved")
      return false
    }
    match fs.rename(destination, backup, overwrite: true) {
      Ok(_) => moved_backup = true
      Err(failure) => {
        gnu.error(f"cannot move {gnu.quote(dest_name)} to {gnu.quote(backup.display())}: {gnu.strerror(failure)}")
        return false
      }
    }
  }

  match fs.rename(source, destination, overwrite: true) {
    Ok(_) => {
      if opts.verbose { gnu.write_text(f"{gnu.quote(source_name)} -> {gnu.quote(dest_name)}\n") }
      true
    }
    Err(failure) => {
      if moved_backup and backup_path != null and ! destination.exists()? {
        let backup = backup_path ?? destination
        let _ = fs.rename(backup, destination, overwrite: true)
      }
      gnu.error(f"cannot move {gnu.quote(source_name)} to {gnu.quote(dest_name)}: {gnu.strerror(failure)}")
      false
    }
  }
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: MvOptions = cli.applet(
    argv,
    {
      gnu: {status: 1, unsupported: {
        "-g": "progress reporting is not available",
        "-Z": "SELinux contexts are not available",
        "--context": "SELinux contexts are not available",
        "--debug": "debug diagnostics are not available",
        "--exchange": "atomic exchange is not available",
      }},
      force: {form: "-f --force", default: false},
      interactive: {form: "-i --interactive", default: false},
      no_clobber: {form: "-n --no-clobber", default: false},
      target: {form: "-t --target-directory DIR", repeated: true},
      no_target_directory: {form: "-T --no-target-directory", default: false},
      no_copy: {form: "--no-copy", default: false},
      strip_trailing_slashes: {form: "--strip-trailing-slashes", default: false},
      verbose: {form: "-v --verbose", default: false},
      backup: {form: "-b --backup[=CONTROL]", default: "", optional_default: "@env"},
      suffix: {form: "-S --suffix SUFFIX", default: ""},
      update: {form: "-u --update[=UPDATE]", default: "all", optional_default: "older"},
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
    gnu.version("mv")
    return
  }

  if opts.update not in ["all", "none", "none-fail", "older"] {
    gnu.error(f"invalid argument {gnu.quote_value(opts.update)} for '--update'")
    exit 1
  }
  if opts.target.len() > 1 {
    gnu.error("multiple target directories specified")
    gnu.try_help()
    exit 1
  }
  if opts.target.len() > 0 and opts.no_target_directory {
    gnu.usage_error("cannot combine --target-directory and --no-target-directory")
  }

  let backup_mode = backup_mode_from(opts)
  let overwrite = overwrite_choice(argv)
  if backup_mode != "none" and (overwrite == "no_clobber" or opts.update == "none" or opts.update == "none-fail") {
    gnu.usage_error("cannot combine --backup with --exchange, -n, or --update=none-fail")
  }
  let suffix_env = env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~"
  let suffix = if opts.suffix != "" { opts.suffix } else { suffix_env }
  var paths = opts.operands
  if opts.strip_trailing_slashes {
    var stripped = []
    for item in paths { stripped += [strip_slashes(item)] }
    paths = stripped
  }

  if paths.len() == 0 {
    gnu.usage_error("missing file operand")
  }
  let has_target = opts.target.len() > 0
  if ! has_target and paths.len() == 1 {
    gnu.usage_error(f"missing destination file operand after {gnu.quote(paths[0])}")
  }
  let target = if has_target { fp"{opts.target[0]}" } else { fp"{paths[paths.len() - 1]}" }
  let sources = if has_target { paths } else { paths |> take(paths.len() - 1) }
  if has_target {
    match fs.stat(target) {
      Ok(meta) if meta.kind == "dir" => {}
      _ => {
        gnu.error(f"target directory {gnu.quote(target.display())}: Not a directory")
        exit 1
      }
    }
  }

  var target_is_dir = has_target
  if ! has_target and ! opts.no_target_directory {
    match fs.stat(target, follow_symlinks: true) {
      Ok(meta) => target_is_dir = meta.kind == "dir"
      Err(_) => target_is_dir = false
    }
  }
  if opts.no_target_directory { target_is_dir = false }
  if sources.len() > 1 and ! target_is_dir {
    gnu.error(f"target {gnu.quote(target.display())} is not a directory")
    exit 1
  }

  var failed = false
  for source_name in sources {
    let source = fp"{source_name}"
    let destination = dest_for(source, target, target_is_dir)
    if ! move_one(source_name, destination, opts, overwrite, backup_mode, suffix) { failed = true }
  }
  if failed { exit 1 }
}
