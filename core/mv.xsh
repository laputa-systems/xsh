#!/bin/xsh
use lib.gnu
use lib.file_publish as files

type Options = {
  no_target_directory: Bool, no_clobber: Bool, force: Bool, interactive: Bool, exchange: Bool,
  target: Str?, verbose: Bool, debug: Bool, progress: Bool, update: Str?, older: Bool,
  backup: Str?, simple_backup: Bool, suffix: Str?, strip_slashes: Bool,
  help: Bool, version: Bool, operands: List[Str],
}

enum MoveOutcome { Moved, Skipped, Failed }
type MoveMetadata = {kind: Str, uid: Int, gid: Int, mode: Int, atime_ns: Int, mtime_ns: Int, rdev: Int}
type MoveResult = {outcome: MoveOutcome, copies: List[FileIdentity]}
type FileIdentity = {dev: Int, ino: Int, path: Path}
type CopyResult = {moved: Bool, copies: List[FileIdentity]}

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

proc usage_error(message: Str) -> Unit {
  gnu.error(f"{message}\nUsage: mv [OPTION]... [-T] SOURCE DEST\nFor more information, try '--help'.")
  exit 1
}

pure basename_path(value: Path) -> Result[Path] {
  var raw = value.bytes()
  while raw.len() > 1 and raw.byte_at(raw.len() - 1) == 47 { raw = raw[..raw.len() - 1] }
  var at = raw.len()
  while at > 0 and raw.byte_at(at - 1) != 47 { at -= 1 }
  Path.parse_bytes(raw[at..])
}

pure child_path(parent: Path, child: Path) -> Path {
  let raw = parent.bytes()
  if raw.byte_at(raw.len() - 1) == 47 { fp"{parent}{child}" } else { fp"{parent}/{child}" }
}

pure same_entry_display(value: Path) -> Str {
  let shown = value.display()
  if shown == "./" { "." } else { shown }
}

proc preserve_move_metadata(source: Path, target: Path, metadata: MoveMetadata) -> Result[Unit] {
  let follow = metadata.kind != "symlink"
  fs.set_owner(target, uid: metadata.uid, gid: metadata.gid, follow_symlinks: follow)?
  let names = match fs.xattr_list(source, follow_symlinks: follow) {
    Ok(names) => names
    Err(failure) => {
      if failure.errno == 95 { [] } else { return Err(failure) }
    }
  }
  for name in names {
    let value = fs.xattr_get(source, name, follow_symlinks: follow)?
    fs.xattr_set(target, name, value, follow_symlinks: follow)?
  }
  if follow { target.chmod(metadata.mode.bit_and(0o7777))? }
  fs.set_times(target, atime_ns: metadata.atime_ns, mtime_ns: metadata.mtime_ns,
    follow_symlinks: follow)
}

proc copy_move_node(source: Path, target: Path, copies: List[FileIdentity]) -> Result[List[FileIdentity]] {
  let metadata = fs.stat(source)?
  if metadata.kind == "dir" {
    let parent = fs.open_root(target.parent().resolve()?)?
    defer parent.close()
    parent.mkdir(basename_path(target)?, mode: 0o700.clear_bits(fs.umask()?))?
    var copied = copies
    for entry in fs.children(source)? {
      let name = basename_path(entry.path)?
      copied = copy_move_node(child_path(source, name), child_path(target, name), copied)?
    }
    preserve_move_metadata(source, target, metadata)?
    return copied
  }
  if metadata.kind == "symlink" {
    target.symlink(to: source.readlink()?)
    preserve_move_metadata(source, target, metadata)?
    return copies
  }
  if metadata.kind == "file" {
    let destination_dev = fs.stat(target.parent())?.dev
    for previous in copies {
      if previous.dev == metadata.dev and previous.ino == metadata.ino {
        if let Ok(previous_metadata) = fs.stat(previous.path) {
          if previous_metadata.dev == destination_dev {
            fs.link(previous.path, target)?
            return copies
          }
        }
      }
    }
    let _ = fs.copy_file(source, target, reflink: "auto", sparse: "auto", mode: 0o600)?
    preserve_move_metadata(source, target, metadata)?
    return copies + [{dev: metadata.dev, ino: metadata.ino, path: target}]
  }
  fs.mknod(target, metadata.kind, metadata.mode.bit_and(0o600),
    major: fs.dev_major(metadata.rdev), minor: fs.dev_minor(metadata.rdev))?
  preserve_move_metadata(source, target, metadata)?
  copies
}

proc remove_move_tree(entry_path: Path) -> Result[Unit] {
  let metadata = fs.stat(entry_path)?
  if metadata.kind == "dir" {
    for entry in fs.children(entry_path)? {
      let name = basename_path(entry.path)?
      remove_move_tree(child_path(entry_path, name))?
    }
    return entry_path.remove_dir()
  }
  entry_path.remove()
}

proc report_move_children(source: Path, target: Path) -> Result[Unit] {
  for entry in fs.children(source)? {
    let name = basename_path(entry.path)?
    let source_child = child_path(source, name)
    let target_child = child_path(target, name)
    print f"{gnu.quote(source_child.display())} -> {gnu.quote(target_child.display())}"
    if fs.stat(source_child)?.kind == "dir" { report_move_children(source_child, target_child)? }
  }
}

proc copy_across_devices(source: Path, target: Path, policy: Str, verbose: Bool,
  copies: List[FileIdentity]) -> Result[CopyResult] {
  # A complete private copy is published by rename so failures leave the source and old destination intact.
  let scratch = fs.tempfile()?
  defer scratch.root.close()
  let staged_dir = fp"{target.parent()}/.xsh-move-{scratch.root.host_path()?.name()}"
  let parent = fs.open_root(staged_dir.parent().resolve()?)?
  defer parent.close()
  parent.mkdir(basename_path(staged_dir)?, mode: 0o700.clear_bits(fs.umask()?))?
  let staged = fp"{staged_dir}/entry"
  let copied = copy_move_node(source, staged, copies)
  if let Err(failure) = copied {
    remove_move_tree(staged_dir)?
    return Err(failure)
  }
  let updated = copied?
  let published = if policy == "skip" { fs.rename_noreplace(staged, target) } else { staged.rename(to: target, overwrite: true) }
  if let Err(failure) = published {
    remove_move_tree(staged_dir)?
    if policy == "skip" and gnu.errno(failure) == 17 { return {moved: false, copies} }
    return Err(failure)
  }
  var published_copies: List[FileIdentity] = []
  let staged_bytes = staged.bytes()
  for index in range(updated.len()) {
    let entry = updated[index]
    let published_path = if index < copies.len() {
      entry.path
    } else {
      let raw = entry.path.bytes()
      let suffix = raw[staged_bytes.len()..]
      Path.parse_bytes(bytes.concat([target.bytes(), suffix]))?
    }
    published_copies += [{dev: entry.dev, ino: entry.ino, path: published_path}]
  }
  staged_dir.remove_dir()?
  if verbose and fs.stat(source)?.kind == "dir" { report_move_children(source, target)? }
  remove_move_tree(source)?
  {moved: true, copies: published_copies}
}

# A terminal user gets the protected-file prompt unless force was requested.
proc confirm_protected(target: Path, mode: Int) -> Result[Bool] {
  let bits = mode.bit_and(0o777)
  let octal = f"0{bits / 64}{bits / 8 % 8}{bits % 8}"
  io.write_stderr(f"mv: replace {gnu.quote(target.display())}, overriding mode {octal} ({permission_text(mode)})? ")?
  io.flush_stderr()?
  io.stdin_line()?.lower().starts_with("y")
}

proc move_one(source: Path, target: Path, opts: Options, policy: Str, backup: Str,
  copies: List[FileIdentity]) -> Result[MoveResult] {
  let metadata = fs.stat(source)?
  let existing = files.present(target)?
  if existing {
    let dest_meta = fs.stat(target)?
    if metadata.kind == "symlink" and dest_meta.kind != "symlink" and backup in ["none", "off"] {
      if let Ok(followed) = fs.stat(source, follow_symlinks: true) {
        if followed.dev == dest_meta.dev and followed.ino == dest_meta.ino {
          gnu.error(f"{gnu.quote(same_entry_display(source))} and {gnu.quote(same_entry_display(target))} are the same file")
          return {outcome: Failed, copies}
        }
      }
    }
    if metadata.dev == dest_meta.dev and metadata.ino == dest_meta.ino and
      (backup in ["none", "off"] or files.same_entry(source, target)?) {
      gnu.error(f"{gnu.quote(same_entry_display(source))} and {gnu.quote(same_entry_display(target))} are the same file")
      return {outcome: Failed, copies}
    }
    if policy == "skip" or opts.update == "none" {
      if opts.debug { print f"skipped {gnu.quote(target.display())}" }
      return {outcome: Skipped, copies}
    }
    if opts.update == "none-fail" {
      gnu.error(f"not replacing {gnu.quote(target.display())}")
      return {outcome: Failed, copies}
    }
    if opts.update == "older" and metadata.mtime_ns <= dest_meta.mtime_ns {
      if opts.debug { print f"skipped {gnu.quote(target.display())}" }
      return {outcome: Skipped, copies}
    }
    if policy == "interactive" and ! files.confirm(target)? { return {outcome: Failed, copies} }
    if policy == "default" and unix.isatty(0) and dest_meta.mode.bit_and(0o200) == 0 and
      ! confirm_protected(target, dest_meta.mode)? { return {outcome: Failed, copies} }
    if opts.exchange {
      fs.rename_exchange(source, target)?
      if opts.verbose or opts.debug { print f"exchanged {gnu.quote(source.display())} <-> {gnu.quote(target.display())}" }
      return {outcome: Moved, copies}
    }
    if metadata.kind == "dir" and dest_meta.kind != "dir" {
      gnu.error(f"cannot overwrite non-directory {gnu.quote(target.display())} with directory {gnu.quote(source.display())}")
      return {outcome: Failed, copies}
    }
    if metadata.kind != "dir" and dest_meta.kind == "dir" {
      gnu.error(f"cannot overwrite directory {gnu.quote(target.display())} with non-directory")
      return {outcome: Failed, copies}
    }
  }
  if opts.exchange { fs.rename_exchange(source, target)?; return {outcome: Moved, copies} }
  if metadata.kind == "dir" and files.canonical(target)?.starts_with(source.resolve()?) {
    gnu.error(f"cannot move {gnu.quote(source.display())} to a subdirectory of itself, {gnu.quote(target.display())}")
    return {outcome: Failed, copies}
  }
  if existing and metadata.kind == "dir" and backup in ["none", "off"] {
    if (fs.children(target)? |> take(1) |> count()) > 0 {
      gnu.error(f"cannot overwrite {gnu.quote(target.display())}: Directory not empty")
      return {outcome: Failed, copies}
    }
  }
  let suffix = opts.suffix ?? env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~"
  if existing and backup in ["simple", "never", "existing", "nil"] and metadata.kind != "symlink" and
    files.same_entry(source, fp"{target}{suffix}")? {
    gnu.error(f"backing up {gnu.quote(target.display())} might destroy source; {gnu.quote(source.display())} not moved")
    return {outcome: Failed, copies}
  }
  var moving_source = source
  var saved: Path? = null
  if existing {
    saved = files.backup_name(target, backup, suffix)?
    if saved != null {
      if files.same_entry(saved, source)? {
        if metadata.kind != "symlink" {
          gnu.error(f"backing up {gnu.quote(target.display())} might destroy source; {gnu.quote(source.display())} not moved")
          return {outcome: Failed, copies}
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
    if gnu.errno(failure) == 18 {
      let copied = copy_across_devices(moving_source, target, policy, opts.verbose, copies)
      if let Err(copy_failure) = copied {
        if saved != null { saved.rename(to: target, overwrite: true) }
        if moving_source != source { moving_source.rename(to: source, overwrite: true) }
        gnu.error(f"cannot move {gnu.quote(source.display())} to {gnu.quote(target.display())}: inter-device move failed: {gnu.strerror(copy_failure)}")
        return {outcome: Failed, copies}
      }
      let result = copied?
      if ! result.moved {
        if saved != null { saved.rename(to: target, overwrite: true) }
        if moving_source != source { moving_source.rename(to: source, overwrite: true) }
        return {outcome: Skipped, copies: result.copies}
      }
      if opts.verbose or opts.debug {
        let tail = if saved != null { f" (backup: {gnu.quote(saved.display())})" } else { "" }
        if opts.verbose { print f"{gnu.quote(source.display())} -> {gnu.quote(target.display())}{tail}" } else { print f"renamed {gnu.quote(source.display())} -> {gnu.quote(target.display())}{tail}" }
      }
      return {outcome: Moved, copies: result.copies}
    }
    if saved != null { saved.rename(to: target, overwrite: true) }
    if moving_source != source { moving_source.rename(to: source) }
    if policy == "skip" and gnu.errno(failure) == 17 { return {outcome: Skipped, copies} }
    return Err(failure)
  }
  if opts.verbose or opts.debug or (opts.progress and unix.tty_attrs(2) is Ok(_)) {
    let tail = if saved != null { f" (backup: {gnu.quote(saved.display())})" } else { "" }
    print f"renamed {gnu.quote(source.display())} -> {gnu.quote(target.display())}{tail}"
  }
  {outcome: Moved, copies}
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
    progress: {form: "--progress", default: false},
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
  if opts.operands.is_empty() {
    usage_error("error: the following required arguments were not provided:\n  <files>...")
  }
  if opts.target == null and opts.operands.len() == 1 {
    usage_error("requires at least 2 values, but only 1 was provided")
  }
  if opts.target != null and opts.no_target_directory {
    gnu.usage_error("--target-directory cannot be used with --no-target-directory")
  }
  if opts.update != null and opts.update not in ["all", "none", "none-fail", "older"] { gnu.usage_error(f"invalid argument {gnu.quote(opts.update)} for 'update'") }
  let dest = if opts.target != null { fp"{opts.target}" } else { fp"{opts.operands[-1]}" }
  let sources = if opts.target != null { opts.operands } else { opts.operands |> take(opts.operands.len() - 1) }
  var is_dir = false
  if ! opts.no_target_directory {
    match files.directory(dest, true) {
      Ok(found) => is_dir = found
      Err(failure) => {
        if dest.display().ends_with("/") { gnu.error(f"failed to access {gnu.quote(dest.display())}: {gnu.strerror(failure)}") } else { gnu.cannot_access(dest.display(), failure) }
        exit 1
      }
    }
  }
  if dest.display().ends_with("/") and ! is_dir {
    match fs.stat(dest, follow_symlinks: true) {
      Ok(_) => { gnu.error(f"failed to access {gnu.quote(dest.display())}: Not a directory"); exit 1 }
      Err(failure) => {
        if gnu.errno(failure) == 20 { gnu.error(f"failed to access {gnu.quote(dest.display())}: {gnu.strerror(failure)}"); exit 1 }
      }
    }
  }
  if (sources.len() > 1 or opts.target != null) and ! is_dir {
    let kind = if opts.target != null or opts.exchange { "target directory" } else { "target" }
    gnu.error(f"{kind} {gnu.quote(dest.display())}: Not a directory")
    exit 1
  }
  opts.update = files.update(argv)?
  let policy = files.overwrite(argv, "default")?
  let backup = opts.backup ?? (if opts.simple_backup or opts.suffix != null { env.get_or("VERSION_CONTROL", "existing") ?? "existing" } else { "none" })
  if backup not in ["none", "off"] and (policy == "skip" or (opts.update != null and opts.update in ["none", "none-fail"])) {
    gnu.usage_error("cannot combine --backup with -n/--no-clobber or --update=none-fail")
  }
  files.validate_backup(backup, opts.suffix ?? env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~")
  var failed = false
  var seen: List[Path] = []
  var copies: List[FileIdentity] = []
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
    match move_one(source, target, opts, policy, backup, copies) {
      Ok(result) => {
        copies = result.copies
        if result.outcome == Failed { failed = true }
        if result.outcome == Moved { seen += [target] }
      }
      Err(failure) => {
        let source_missing = match fs.stat(source) {
          Ok(_) => false
          Err(source_failure) => gnu.errno(source_failure) in [2, 20]
        }
        if gnu.errno(failure) in [2, 20] and source_missing {
          gnu.cannot("stat", text, failure)
        } else {
          gnu.error(f"cannot move {gnu.quote(text)} to {gnu.quote(target.display())}: {gnu.strerror(failure)}")
        }
        failed = true
      }
    }
  }
  if failed { exit 1 }
}
