#!/bin/xsh
use lib.gnu
use lib.file_publish as files

type Options = {
  no_target_directory: Bool, no_clobber: Bool, force: Bool, interactive: Bool, exchange: Bool,
  target: Str?, verbose: Bool, debug: Bool, update: Str?, older: Bool,
  backup: Str?, simple_backup: Bool, suffix: Str?, strip_slashes: Bool,
  help: Bool, version: Bool, operands: List[Str],
}

enum MoveOutcome { Moved, Skipped, Failed }

pure permission_text(mode: Int) -> Str {
  let masks = [0o400, 0o200, 0o100, 0o40, 0o20, 0o10, 0o4, 0o2, 0o1]
  var text = ""
  for index in range(9) {
    let enabled = mode.bit_and(masks[index]) != 0
    var letter = if enabled { "rwx".byte_slice(index % 3, 1) } else { "-" }
    if index % 3 == 2 {
      let special = if index == 2 { 0o4000 } else if index == 5 { 0o2000 } else { 0o1000 }
      if mode.bit_and(special) != 0 {
        letter = if index == 8 { if enabled { "t" } else { "T" } } else { if enabled { "s" } else { "S" } }
      }
    }
    text += letter
  }
  text
}

# A terminal user gets the protected-file prompt unless force was requested.
proc confirm_protected(target: Path, mode: Int) -> Result[Bool] {
  let bits = mode.bit_and(0o777)
  let octal = f"0{bits / 64}{bits / 8 % 8}{bits % 8}"
  io.write_stderr(f"mv: replace {gnu.quote(target.display())}, overriding mode {octal} ({permission_text(mode)})? ")?
  io.flush_stderr()?
  io.stdin_line()?.lower().starts_with("y")
}

proc move_one(source: Path, target: Path, opts: Options, policy: Str, backup: Str) -> Result[MoveOutcome] {
  let metadata = fs.stat(source)?
  let existing = files.present(target)?
  if existing {
    let dest_meta = fs.stat(target)?
    if metadata.kind == "symlink" and dest_meta.kind != "symlink" and backup in ["none", "off"] {
      if let Ok(followed) = fs.stat(source, follow_symlinks: true) {
        if followed.dev == dest_meta.dev and followed.ino == dest_meta.ino {
          gnu.error(f"{gnu.quote(source.display())} and {gnu.quote(target.display())} are the same file")
          return Failed
        }
      }
    }
    if metadata.dev == dest_meta.dev and metadata.ino == dest_meta.ino and
      (backup in ["none", "off"] or files.same_entry(source, target)?) {
      gnu.error(f"{gnu.quote(source.display())} and {gnu.quote(target.display())} are the same file")
      return Failed
    }
    if policy == "skip" or opts.update == "none" {
      if opts.debug { print f"skipped {gnu.quote(target.display())}" }
      return Skipped
    }
    if opts.update == "none-fail" {
      gnu.error(f"not replacing {gnu.quote(target.display())}")
      return Failed
    }
    if opts.update == "older" and metadata.mtime_ns <= dest_meta.mtime_ns {
      if opts.debug { print f"skipped {gnu.quote(target.display())}" }
      return Skipped
    }
    if policy == "interactive" and ! files.confirm(target)? { return Failed }
    if policy == "default" and unix.isatty(0) and dest_meta.mode.bit_and(0o200) == 0 and
      ! confirm_protected(target, dest_meta.mode)? { return Failed }
    if opts.exchange {
      fs.rename_exchange(source, target)?
      if opts.verbose or opts.debug { print f"exchanged {gnu.quote(source.display())} <-> {gnu.quote(target.display())}" }
      return Moved
    }
    if metadata.kind == "dir" and dest_meta.kind != "dir" {
      gnu.error(f"cannot overwrite non-directory {gnu.quote(target.display())} with directory {gnu.quote(source.display())}")
      return Failed
    }
    if metadata.kind != "dir" and dest_meta.kind == "dir" {
      gnu.error(f"cannot overwrite directory {gnu.quote(target.display())} with non-directory")
      return Failed
    }
  }
  if opts.exchange { fs.rename_exchange(source, target)?; return Moved }
  if metadata.kind == "dir" and files.canonical(target)?.starts_with(source.resolve()?) {
    gnu.error(f"cannot move {gnu.quote(source.display())} to a subdirectory of itself, {gnu.quote(target.display())}")
    return Failed
  }
  if existing and metadata.kind == "dir" and backup in ["none", "off"] {
    if (fs.children(target)? |> take(1) |> count()) > 0 {
      gnu.error(f"cannot overwrite {gnu.quote(target.display())}: Directory not empty")
      return Failed
    }
  }
  let suffix = opts.suffix ?? env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~"
  if existing and backup in ["simple", "never", "existing", "nil"] and metadata.kind != "symlink" and
    files.same_entry(source, fp"{target}{suffix}")? {
    gnu.error(f"backing up {gnu.quote(target.display())} might destroy source; {gnu.quote(source.display())} not moved")
    return Failed
  }
  var moving_source = source
  var saved: Path? = null
  if existing {
    saved = files.backup_name(target, backup, suffix)?
    if saved != null {
      if files.same_entry(saved, source)? {
        if metadata.kind != "symlink" {
          gnu.error(f"backing up {gnu.quote(target.display())} might destroy source; {gnu.quote(source.display())} not moved")
          return Failed
        }
        # Keep the source link alive when its name is also the backup name.
        let scratch = fs.tempfile()?
        defer scratch.root.close()
        moving_source = fp"{source.parent()}/.xsh-move-{scratch.root.host_path()?.name()}"
        source.rename(to: moving_source)
      }
      if let Err(failure) = target.rename(to: saved, overwrite: true) {
        if moving_source != source { moving_source.rename(to: source) }
        return Err(failure)
      }
    }
  }
  let moved = if policy == "skip" { fs.rename_noreplace(moving_source, target) } else { moving_source.rename(to: target, overwrite: true) }
  if let Err(failure) = moved {
    if saved != null { saved.rename(to: target, overwrite: true) }
    if moving_source != source { moving_source.rename(to: source) }
    if policy == "skip" and gnu.errno(failure) == 17 { return Skipped }
    return Err(failure)
  }
  if opts.verbose or opts.debug {
    let tail = if saved != null { f" (backup: {gnu.quote(saved.display())})" } else { "" }
    print f"renamed {gnu.quote(source.display())} -> {gnu.quote(target.display())}{tail}"
  }
  Moved
}

proc main(...argv: List[Str]) {
  var opts: Options = cli.applet(argv, {
    gnu: {status: 1, unsupported: {
      "--no-copy": "cross-device copy is not available",
      "-Z": "security contexts are not available",
    }},
    exchange: {form: "--exchange", default: false},
    no_target_directory: {form: "-T --no-target-directory", default: false},
    no_clobber: {form: "-n --no-clobber", default: false},
    force: {form: "-f --force", default: false},
    interactive: {form: "-i --interactive", default: false},
    target: {form: "-t --target-directory DIR"},
    verbose: {form: "-v --verbose", default: false},
    debug: {form: "--debug", default: false},
    older: {form: "-u", default: false},
    update: {form: "--update[=UPDATE]", optional_default: "older"},
    backup: {form: "--backup[=CONTROL]", optional_default: "existing"},
    simple_backup: {form: "-b", default: false},
    suffix: {form: "-S --suffix SUFFIX"},
    strip_slashes: {form: "--strip-trailing-slashes", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    operands: {form: "...SOURCE"},
  })?
  if opts.help { gnu.help("Usage: mv [OPTION]... SOURCE... DEST\nRename SOURCE to DEST, or move SOURCE(s) to DIRECTORY."); return }
  if opts.version { gnu.version("mv"); return }
  if opts.operands.is_empty() { gnu.usage_error("missing file operand") }
  if opts.target == null and opts.operands.len() == 1 { gnu.usage_error(f"missing destination file operand after {gnu.quote(opts.operands[0])}") }
  if opts.target != null and opts.no_target_directory { gnu.usage_error("cannot combine --target-directory and --no-target-directory") }
  if opts.update != null and opts.update not in ["all", "none", "none-fail", "older"] { gnu.usage_error(f"invalid argument {gnu.quote(opts.update)} for 'update'") }
  let dest = if opts.target != null { fp"{opts.target}" } else { fp"{opts.operands[-1]}" }
  let sources = if opts.target != null { opts.operands } else { opts.operands |> take(opts.operands.len() - 1) }
  var is_dir = false
  if ! opts.no_target_directory {
    match files.directory(dest, true) {
      Ok(found) => is_dir = found
      Err(failure) => { gnu.cannot_access(dest.display(), failure); exit 1 }
    }
  }
  if (sources.len() > 1 or opts.target != null) and ! is_dir {
    gnu.error(f"target {gnu.quote(dest.display())}: Not a directory")
    exit 1
  }
  opts.update = files.update(argv)?
  let policy = files.overwrite(argv, "default")?
  let backup = opts.backup ?? (if opts.simple_backup or opts.suffix != null { env.get_or("VERSION_CONTROL", "existing") ?? "existing" } else { "none" })
  if policy == "skip" and backup not in ["none", "off"] { gnu.usage_error("options --backup and --no-clobber are mutually exclusive") }
  files.validate_backup(backup, opts.suffix ?? env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~")
  var failed = false
  var seen: List[Path] = []
  for text in sources {
    var stripped = text
    if opts.strip_slashes {
      while stripped.ends_with("/") and stripped.byte_len() > 1 { stripped = stripped.byte_slice(0, stripped.byte_len() - 1) }
    }
    let source = fp"{stripped}"
    let target = if is_dir { files.destination(dest, source) } else { dest }
    if target in seen {
      gnu.error(f"will not overwrite just-created {gnu.quote(target.display())} with {gnu.quote(text)}")
      failed = true
      continue
    }
    match move_one(source, target, opts, policy, backup) {
      Ok(outcome) => {
        if outcome == Failed { failed = true }
        if outcome == Moved { seen += [target] }
      }
      Err(failure) => {
        if gnu.errno(failure) == 2 and ! files.present(source)? { gnu.cannot("stat", text, failure) } else { gnu.error(f"cannot move {gnu.quote(text)} to {gnu.quote(target.display())}: {gnu.strerror(failure)}") }
        failed = true
      }
    }
  }
  if failed { exit 1 }
}
