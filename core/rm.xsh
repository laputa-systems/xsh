#!/bin/xsh
use lib.gnu as gnu

const USAGE = """Usage: rm [OPTION]... FILE...
Remove (unlink) the FILE(s).

  -f, --force           ignore nonexistent files and arguments, never prompt
  -i                    prompt before every removal
  -I                    prompt once before removing more than three files, or when removing recursively
  -d, --dir             remove empty directories
  -r, -R, --recursive   remove directories and their contents recursively
      --one-file-system when removing a hierarchy, skip any directory on a different file system
      --no-preserve-root do not treat '/' specially
      --preserve-root[=all] do not remove '/' (default); with 'all', reject any mount point
  -v, --verbose         explain what is being done
  -g, --progress        show removal progress
      --help            display this help and exit
      --version         output version information and exit
"""

type RmOptions = {
  force: Bool,
  recursive: Bool,
  directory: Bool,
  verbose: Bool,
  progress: Bool,
  one_file_system: Bool,
  preserve_root: Str,
  no_preserve_root: Bool,
  interactive: Str,
  interactive_once: Bool,
  help: Bool,
  version: Bool,
  targets: List[Str],
}

type RmStat = {dev: Int, kind: Str, mode: Int, size: Int}
type RmTreeResult = {failed: Bool, write_failure: Error?}

pure contains_dot_component(name: Str) -> Bool {
  var last = ""
  for part in name.split("/") {
    if part != "" { last = part }
  }
  last == "." or last == ".."
}

pure operand_display(name: Str) -> Str {
  var end = name
  while end.byte_len() > 1 and end.ends_with("/") {
    end = end.byte_slice(0, length: end.byte_len() - 1)
  }
  end
}

pure protected_operand_display(name: Str) -> Str {
  var end = operand_display(name)
  return end when end == name or name == "/"
  f"{end}/"
}

pure invalid_short_option(arg: Str) -> Str? {
  return null when ! arg.starts_with("-") or arg == "-" or arg.starts_with("--")
  for ch in arg.byte_slice(1) {
    return f"{ch}" when ch not in "frRdiIvg"
  }
  null
}

pure known_long_option(arg: Str) -> Bool {
  let equal = arg.find("=") ?? arg.byte_len()
  let name = arg.byte_slice(0, length: equal)
  for known in ["--force", "--recursive", "--dir", "--verbose", "--progress", "--one-file-system", "--no-preserve-root", "--preserve-root", "--interactive", "--help", "--version"] {
    if known.starts_with(name) { return true }
  }
  false
}

pure raw_rm_targets(argv: List[Str], raw: List[Bytes], posixly_correct: Bool) -> List[Bytes] {
  var targets: List[Bytes] = []
  var index = 0
  var options = true
  while index < argv.len() {
    let arg = argv[index]
    if arg == "---presume-input-tty" { index += 1; continue }
    if options and arg == "--" { options = false; index += 1; continue }
    if options and arg.starts_with("-") and arg != "-" { index += 1; continue }
    targets += [raw[index]]
    if posixly_correct { options = false }
    index += 1
  }
  targets
}

pure presumed_interactive_mode(argv: List[Str], default: Str, posixly_correct: Bool) -> Str {
  var mode = default
  var options = true
  for arg in argv {
    if options and arg == "--" { options = false; continue }
    if ! options { continue }
    if posixly_correct and (! arg.starts_with("-") or arg == "-") { options = false; continue }
    if arg == "-i" or arg == "--interactive" {
      mode = "always"
    } else if arg == "--force" {
      mode = "never"
    } else if arg == "-I" {
      mode = "once"
    } else if arg.starts_with("--interactive=") {
      mode = arg.byte_slice("--interactive=".byte_len())
    } else if arg.starts_with("-") and ! arg.starts_with("--") and arg != "-" {
      for ch in arg.byte_slice(1) {
        if ch == "i" { mode = "always" }
        if ch == "I" { mode = "once" }
        if ch == "f" { mode = "never" }
      }
    }
  }
  mode
}

pure verbose_option_given(argv: List[Str], posixly_correct: Bool) -> Bool {
  var options = true
  for arg in argv {
    if options and arg == "--" { options = false; continue }
    if ! options { continue }
    if posixly_correct and (! arg.starts_with("-") or arg == "-") { options = false; continue }
    if arg == "--verbose" or (arg.starts_with("--") and "--verbose".starts_with(arg)) { return true }
    if arg.starts_with("-") and arg != "-" and ! arg.starts_with("--") {
      for option in arg.byte_slice(1) {
        if option == "v" { return true }
      }
    }
  }
  false
}

proc confirm(question: Str) [process, env, io, error] -> Bool {
  io.write_stderr(f"rm: {question} ")?
  io.flush_stdout()?
  match io.stdin_line() {
    Ok(answer) => answer.trim().lower().starts_with("y")
    Err(_) => false
  }
}

pure prompt_for(mode: Str, assumed_tty: Bool, stat: RmStat) -> Bool {
  mode == "always" or (mode == "protected" and assumed_tty and stat.mode.bit_and(0o222) == 0 and stat.kind != "symlink")
}

proc entry_prompt(path_name: Str, stat: RmStat, protected_prompt: Bool) [env] -> Str {
  let name = gnu.quote(path_name)
  if stat.kind == "dir" {
    if stat.mode.bit_and(0o500) == 0 {
      f"attempt removal of inaccessible directory {name}?"
    } else if protected_prompt and stat.mode.bit_and(0o222) == 0 {
      f"remove write-protected directory {name}?"
    } else {
      f"remove directory {name}?"
    }
  } else if stat.kind == "symlink" {
    f"remove symbolic link {name}?"
  } else if stat.kind == "file" and stat.size == 0 {
    if protected_prompt and stat.mode.bit_and(0o222) == 0 { f"remove write-protected regular empty file {name}?" } else { f"remove regular empty file {name}?" }
  } else if protected_prompt and stat.kind == "file" and stat.mode.bit_and(0o222) == 0 {
    f"remove write-protected regular file {name}?"
  } else if protected_prompt and stat.mode.bit_and(0o222) == 0 {
    f"remove write-protected {stat.kind} {name}?"
  } else {
    f"remove {stat.kind} {name}?"
  }
}

proc relative_name(root: Path, root_name: Str, entry_path: Path) [fs] -> Str {
  let absolute_root = match root.resolve() {
    Ok(resolved_root) => resolved_root
    Err(_) => root.normalize()
  }
  if entry_path == absolute_root { return root_name }
  let relative = entry_path.relative_to(absolute_root)
  if relative.display() == "." or relative.display() == "" { return root_name }
  return f"/{relative.display()}" when root_name == "/"
  f"{root_name}/{relative.display()}"
}

proc invalid_dash_file_hint(argv: List[Str]) [fs, process, env, io, error] {
  var options = true
  let posixly_correct = (env.get_or("POSIXLY_CORRECT", "") ?? "") != ""
  for arg in argv {
    if options and arg == "--" { options = false; continue }
    if ! options { continue }
    if posixly_correct and (! arg.starts_with("-") or arg == "-") { options = false; continue }
    let invalid: Str? = if arg.starts_with("--") and arg != "--" {
      if known_long_option(arg) { null } else { arg }
    } else {
      invalid_short_option(arg)
    }
    if invalid == null { continue }
    if ! fs.exists(fp"{arg}")? { continue }
    if arg.starts_with("--") {
      gnu.error(f"unrecognized option {gnu.quote(arg)}")
    } else {
      gnu.error(f"invalid option -- {gnu.quote(invalid ?? "?")}")
    }
    gnu.try_help()
    let escaped = gnu.quote_bytes(bytes.from_text(arg), always: false)
    gnu.error(f"use 'rm ./{escaped}' to remove the file {gnu.quote(arg)}.")
    exit 1
  }
}

proc remove_tree(root: Path, root_name: Str, root_device: Int, one_file_system: Bool, verbose: Bool, progress: Bool, interactive: Str, assumed_tty: Bool) [fs, process, env, io, error] -> Result[RmTreeResult] {
  var failed = false
  var write_failure: Error? = null
  var blocked: List[Path] = []
  let entries = fs.walk(root, gitignore: false, stat: true, hidden: true)? |> sort-by(desc: true) .path

  for entry in entries {
    let entry_path = entry.path
    var skip = false
    for parent in blocked {
      if entry_path == parent { skip = true; break }
    }
    if skip { continue }

    let stat = match fs.stat(entry_path, follow_symlinks: false) {
      Ok(metadata) => metadata
      Err(failure) => {
        gnu.cannot("remove", relative_name(root, root_name, entry_path), failure)
        failed = true
        var parent = entry_path.parent()
        while parent != "" and parent != "." and parent != "/" {
          blocked += [parent]
          parent = parent.parent()
        }
        continue
      }
    }
    if one_file_system and stat.dev != root_device { continue }

    let shown = relative_name(root, root_name, entry_path)
    if prompt_for(interactive, assumed_tty, stat) and ! confirm(entry_prompt(shown, stat, true)) {
      continue
    }

    if progress { gnu.error(f"removing {gnu.quote(shown)}") }
    let removed = if stat.kind == "dir" { entry_path.remove_dir() } else { entry_path.remove() }
    match removed {
      Ok(_) => {
        if verbose and write_failure == null {
          let noun = if stat.kind == "dir" { "removed directory" } else { "removed" }
          gnu.write_text(f"{noun} {gnu.quote(shown)}\n")
          if let Err(failure) = io.flush_stdout() { write_failure = failure }
        }
      }
      Err(failure) => {
        gnu.cannot("remove", shown, failure)
        failed = true
        var parent = entry_path.parent()
        while parent != "" and parent != "." and parent != "/" {
          blocked += [parent]
          parent = parent.parent()
        }
      }
    }
  }

  Ok({failed: failed, write_failure: write_failure})
}

proc main(...argv: List[Str]) [fs, process, env, io, error] {
  var parse_args: List[Str] = []
  var assumed_tty = false
  for arg in argv {
    if arg == "---presume-input-tty" {
      assumed_tty = true
    } else {
      parse_args += [arg]
    }
  }

  for arg in parse_args {
    if arg != "--no-preserve-root" and arg.starts_with("--") and "--no-preserve-root".starts_with(arg) {
      gnu.usage_error("you may not abbreviate the --no-preserve-root option")
    }
  }
  invalid_dash_file_hint(parse_args)

  let opts: RmOptions = cli.applet(
    parse_args,
    {
      gnu: {status: 1},
      force: {form: "-f --force", default: false},
      recursive: {form: "-r -R --recursive", default: false},
      directory: {form: "-d --dir", default: false},
      verbose: {form: "-v --verbose", default: false},
      progress: {form: "-g --progress", default: false},
      one_file_system: {form: "--one-file-system", default: false},
      preserve_root: {form: "--preserve-root[=all]", default: "", optional_default: "root", conflicts: ["no_preserve_root"]},
      no_preserve_root: {form: "--no-preserve-root", default: false, conflicts: ["preserve_root"]},
      interactive: {form: "-i --interactive[=WHEN]", default: "protected", optional_default: "always"},
      interactive_once: {form: "-I", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      targets: {form: "...FILE"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }
  if opts.version {
    gnu.version("rm")
    return
  }
  if opts.targets.len() == 0 {
    return when opts.force
    gnu.missing_operand()
  }

  if opts.preserve_root != "" and opts.preserve_root != "root" and opts.preserve_root != "all" {
    gnu.usage_error(f"invalid value {gnu.quote_value(opts.preserve_root)} for '--preserve-root'")
  }
  let posixly_correct = (env.get_or("POSIXLY_CORRECT", "") ?? "") != ""
  let verbose = opts.verbose or verbose_option_given(parse_args, posixly_correct)
  var checking_options = true
  for arg in parse_args {
    if checking_options and arg == "--" { checking_options = false; continue }
    if ! checking_options { continue }
    if posixly_correct and (! arg.starts_with("-") or arg == "-") { checking_options = false; continue }
    let equal_at = arg.find("=")
    let option_name = if equal_at == null { arg } else { arg.byte_slice(0, length: equal_at ?? 0) }
    if equal_at != null and option_name.starts_with("--") and "--preserve-root".starts_with(option_name) {
      let value = arg.byte_slice((equal_at ?? 0) + 1)
      if value not in ["root", "all"] {
        gnu.usage_error(f"invalid value {gnu.quote_value(value)} for '--preserve-root'")
      }
    }
  }
  let interactive = presumed_interactive_mode(parse_args, opts.interactive, posixly_correct)
  if interactive not in ["protected", "always", "once", "never", "no", "none", "yes"] {
    gnu.usage_error(f"invalid argument {gnu.quote_value(interactive)} for '--interactive'")
  }
  let prompt_mode = if interactive in ["no", "none"] { "never" } else if interactive == "yes" { "always" } else { interactive }

  if prompt_mode == "once" and (opts.recursive or opts.targets.len() > 3) {
    let recurse = if opts.recursive { " recursively?" } else { "?" }
    let count = opts.targets.len()
    let noun = if count == 1 { "argument" } else { "arguments" }
    if ! confirm(f"remove {count} {noun}{recurse}") { return }
  }

  let raw_targets = raw_rm_targets(argv, cli.argv_bytes(), posixly_correct)
  var had_error = false
  var write_failure: Error? = null
  for index in range(opts.targets.len()) {
    let item = opts.targets[index]
    let raw_item = raw_targets[index]
    if contains_dot_component(item) {
      gnu.error(f"refusing to remove '.' or '..' directory: skipping {gnu.quote(protected_operand_display(item))}")
      had_error = true
      continue
    }

    let target = Path.parse_bytes(raw_item)?
    let stat = match fs.stat(target, follow_symlinks: false) {
      Ok(metadata) => metadata
      Err(failure) => {
        if opts.force and gnu.errno(failure) == 2 { continue }
        gnu.error(f"cannot remove {gnu.quote_bytes(raw_item)}: {gnu.strerror(failure)}")
        had_error = true
        continue
      }
    }
    let preserve = ! opts.no_preserve_root
    let is_root_inode = if preserve and opts.recursive {
      let root_stat = fs.stat(fp"/")?
      stat.dev == root_stat.dev and stat.ino == root_stat.ino
    } else { false }
    let is_root_path = if preserve and opts.recursive {
      match target.resolve() {
        Ok(resolved) => resolved == "/" and (target.normalize() == "/" or item.ends_with("/"))
        Err(_) => false
      }
    } else { false }
    if is_root_inode or is_root_path {
      gnu.error(f"it is dangerous to operate recursively on {gnu.quote(item)} (same as '/')")
      gnu.error("use --no-preserve-root to override this failsafe")
      had_error = true
      continue
    }

    let one_file_system = opts.one_file_system or opts.preserve_root == "all"
    let shown_target = operand_display(item)
    if stat.kind == "dir" and opts.recursive {
      if prompt_mode == "always" and ! confirm(f"descend into directory {gnu.quote(shown_target)}?") {
        continue
      }
      if opts.progress { gnu.error(f"removing {gnu.quote_bytes(raw_item)}") }
      if ! verbose and ! one_file_system and prompt_mode == "never" {
        match target.remove() {
          Ok(_) => {}
          Err(failure) => { gnu.error(f"cannot remove {gnu.quote_bytes(raw_item)}: {gnu.strerror(failure)}"); had_error = true }
        }
      } else {
        let result = remove_tree(target, shown_target, stat.dev, one_file_system, verbose and write_failure == null, opts.progress, prompt_mode, assumed_tty)?
        had_error = had_error or result.failed
        if write_failure == null { write_failure = result.write_failure }
      }
      continue
    }

    if stat.kind == "dir" and ! opts.directory {
      gnu.error(f"cannot remove {gnu.quote_bytes(raw_item)}: Is a directory")
      had_error = true
      continue
    }

    if prompt_for(prompt_mode, assumed_tty, stat) and ! confirm(entry_prompt(shown_target, stat, prompt_mode == "protected")) {
      continue
    }
    if opts.progress { gnu.error(f"removing {gnu.quote_bytes(raw_item)}") }
    let removed = if stat.kind == "dir" { target.remove_dir() } else { target.remove() }
    match removed {
      Ok(_) => {
        if verbose and write_failure == null {
          let noun = if stat.kind == "dir" { "removed directory" } else { "removed" }
          gnu.write_text(f"{noun} {gnu.quote_bytes(raw_item)}\n")
          if let Err(failure) = io.flush_stdout() { write_failure = failure }
        }
      }
      Err(failure) => { gnu.error(f"cannot remove {gnu.quote_bytes(raw_item)}: {gnu.strerror(failure)}"); had_error = true }
    }
  }

  if let failure = write_failure { gnu.write_failed(failure) }
  exit 1 when had_error
}
