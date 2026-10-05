#!/bin/xsh
use lib.gnu
use lib.file_publish as files

type Options = {
  directory: Bool, parents: Bool, mode: Str, owner: Str?, group_name: Str?,
  preserve: Bool, compare: Bool, verbose: Bool, target: Str?, no_target_directory: Bool,
  backup: Str?, simple_backup: Bool, suffix: Str?, copy: Bool,
  help: Bool, version: Bool, operands: List[Str],
}

# Installation modes start from zero, so omitted classes cannot inherit source
# permissions or the caller's umask.
pure install_mode(spec: Str, directory: Bool) -> Int? {
  if rx"^[0-7]+$".matches(spec) {
    var mode = 0
    for digit in spec { mode = mode * 8 + (digit.parse_int() ?? 0) }
    return if mode <= 0o7777 { mode } else { null }
  }
  var mode = 0
  for clause in spec.split(",") {
    let pieces = rx"^([ugoa]*)([=+-])([rwxXstugo]*)$".captures(clause)
    if pieces.is_empty() { return null }
    let who = pieces[1]
    let op = pieces[2]
    let perms = pieces[3]
    var bits = 0
    var mask = 0
    for class in "ugo" {
      if who == "" or "a" in who or class in who {
        let shift = if class == "u" { 64 } else if class == "g" { 8 } else { 1 }
        mask = mask.bit_or(7 * shift)
        if "r" in perms { bits = bits.bit_or(4 * shift) }
        if "w" in perms { bits = bits.bit_or(2 * shift) }
        if "x" in perms or ("X" in perms and (directory or mode.bit_and(0o111) != 0)) { bits = bits.bit_or(shift) }
        for copy_class in "ugo" {
          if copy_class in perms {
            let divisor = if copy_class == "u" { 64 } else if copy_class == "g" { 8 } else { 1 }
            bits = bits.bit_or(mode / divisor % 8 * shift)
          }
        }
        if class == "u" { mask = mask.bit_or(0o4000); if "s" in perms { bits = bits.bit_or(0o4000) } }
        if class == "g" { mask = mask.bit_or(0o2000); if "s" in perms { bits = bits.bit_or(0o2000) } }
        if class == "o" { mask = mask.bit_or(0o1000); if "t" in perms { bits = bits.bit_or(0o1000) } }
      }
    }
    mode = if op == "=" { mode.clear_bits(mask).bit_or(bits) } else if op == "+" { mode.bit_or(bits) } else { mode.clear_bits(bits) }
  }
  mode
}

# Compare exact bytes in bounded chunks so -C does not allocate whole files.
proc equal_contents(source: Path, dest: Path, size: Int) -> Result[Bool] {
  var offset = 0
  while offset < size {
    let length = if size - offset > 65536 { 65536 } else { size - offset }
    if bytes.read_at(source, offset, length)? != bytes.read_at(dest, offset, length)? { return false }
    offset += length
  }
  true
}

proc install_one(source: Path, target: Path, opts: Options, mode: Int, uid: Int?, gid: Int?, backup: Str) -> Result[Bool] {
  # Kernel descriptor aliases can name streams that have no canonical pathname.
  # Follow them for metadata and let the copier open the original source path.
  let metadata = fs.stat(source, follow_symlinks: true)?
  if metadata.kind == "dir" {
    gnu.error(f"omitting directory {gnu.quote(source.display())}")
    exit 1
  }
  if metadata.kind not in ["file", "fifo", "char", "block"] {
    gnu.error(f"cannot install {gnu.quote(source.display())}: unsupported file type {gnu.quote(metadata.kind)}")
    exit 1
  }
  let existing = files.present(target)?
  if existing {
    let old = fs.stat(target)?
    if (metadata.dev == old.dev and metadata.ino == old.ino) or files.same_entry(source, target)? {
      gnu.error(f"{gnu.quote(source.display())} and {gnu.quote(target.display())} are the same file")
      exit 1
    }
  }
  if existing and fs.stat(target)?.kind == "dir" {
    gnu.error(f"cannot overwrite directory {gnu.quote(target.display())} with non-directory")
    exit 1
  }
  if existing and opts.compare and metadata.kind == "file" {
    let old = fs.stat(target)?
    if old.kind == "file" and old.size == metadata.size and old.mode.bit_and(0o7777) == mode and
      mode.bit_and(0o7000) == 0 and metadata.mode.bit_and(0o7000) == 0 and
      old.mode.bit_and(0o7000) == 0 and (! opts.preserve or old.mtime_ns == metadata.mtime_ns) and
      old.uid == (uid ?? user.current()?.uid) and old.gid == (gid ?? group.current()?.gid) and
      (uid == null or old.uid == uid) and (gid == null or old.gid == gid) and
      equal_contents(source, target, metadata.size)? { return false }
  }
  var saved: Path? = null
  if existing { saved = files.backup_name(target, backup, opts.suffix ?? env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~")? }
  if saved != null and files.same_entry(saved, source)? {
    gnu.error(f"backing up {gnu.quote(target.display())} might destroy source;  {gnu.quote(source.display())} not copied")
    exit 1
  }
  if opts.parents { make_ancestors(target.parent(), opts.verbose)? }
  # Publishing a prepared regular file replaces a symlink entry without
  # touching the object it names, and leaves existing files intact on failure.
  let scratch = fs.tempfile()?
  defer scratch.root.close()
  let staged = fp"{target.parent()}/.xsh-install-{scratch.root.host_path()?.name()}"
  let _ = fs.copy_file(source, staged, reflink: "auto", sparse: "never", mode: 0o600)?
  defer staged.remove()
  fs.set_owner(staged, uid: uid, gid: gid)
  staged.chmod(mode)
  if opts.preserve {
    let copied = fs.stat(source, follow_symlinks: true)?
    fs.set_times(staged, atime_ns: copied.atime_ns, mtime_ns: copied.mtime_ns)
  }
  if saved != null {
    if let Err(failure) = target.rename(to: saved, overwrite: true) {
      gnu.cannot("backup", target.display(), failure)
      return false
    }
  }
  if let Err(failure) = staged.rename(to: target, overwrite: true) {
    if saved != null { saved.rename(to: target, overwrite: true) }
    return Err(failure)
  }
  if opts.verbose {
    if existing and saved == null { print f"removed {gnu.quote(target.display())}" }
    print f"{gnu.quote(source.display())} -> {gnu.quote(target.display())}"
  }
  true
}

# Intermediate installation directories use fixed searchable permissions even
# when the installed leaf is deliberately inaccessible.
proc make_ancestors(dest: Path, verbose = false) -> Result[Unit] {
  if files.present(dest)? { return }
  make_ancestors(dest.parent(), verbose)?
  if let Err(failure) = dest.mkdir() {
    if ! files.directory(dest, true)? { return Err(failure) }
  }
  dest.chmod(0o755)
  if verbose { print f"install: creating directory {gnu.quote(dest.display())}" }
}

proc install_directory(raw: Path, opts: Options, mode: Int, uid: Int?, gid: Int?) -> Result[Unit] {
  var text = raw.display()
  while text.ends_with("/") and text.byte_len() > 1 { text = text.byte_slice(0, text.byte_len() - 1) }
  while text.ends_with("/.") { text = text.byte_slice(0, text.byte_len() - 2) }
  let dest = if text == "" { p"/" } else { fp"{text}" }
  let existed = files.present(dest)?
  make_ancestors(dest.parent(), opts.verbose)?
  dest.mkdir(parents: true)
  fs.set_owner(dest, uid: uid, gid: gid, follow_symlinks: true)
  dest.chmod(mode)
  if opts.verbose and ! existed {
    let name = if raw.display().ends_with("/.") or raw.display().ends_with("/./") { dest.normalize().display() } else { dest.display() }
    print f"install: creating directory {gnu.quote(name)}"
  }
}

proc main(...argv: List[Str]) {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1, unsupported: {
      "-s": "binary stripping is not available", "--strip": "binary stripping is not available",
      "--strip-program": "binary stripping is not available", "-Z": "security contexts are not available",
      "--context": "security contexts are not available", "--preserve-context": "security contexts are not available",
      "--debug": "copy diagnostics are not available",
    }},
    directory: {form: "-d --directory", default: false},
    parents: {form: "-D", default: false},
    mode: {form: "-m --mode MODE", default: "755"},
    owner: {form: "-o --owner OWNER"},
    group_name: {form: "-g --group GROUP"},
    preserve: {form: "-p --preserve-timestamps", default: false},
    compare: {form: "-C --compare", default: false},
    verbose: {form: "-v --verbose", default: false},
    target: {form: "-t --target-directory DIR"},
    no_target_directory: {form: "-T --no-target-directory", default: false},
    backup: {form: "--backup[=CONTROL]", optional_default: "existing"},
    simple_backup: {form: "-b", default: false},
    suffix: {form: "-S --suffix SUFFIX"},
    copy: {form: "-c", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    operands: {form: "...SOURCE"},
  })?
  if opts.help { gnu.help("Usage: install [OPTION]... SOURCE... DEST\n  or: install -d [OPTION]... DIRECTORY...\nCopy files and set their attributes."); return }
  if opts.version { gnu.version("install"); return }
  if opts.operands.is_empty() { gnu.usage_error("missing file operand") }
  let mode = install_mode(opts.mode.trim(), opts.directory)
  if mode == null { gnu.usage_error(f"invalid mode {gnu.quote(opts.mode)}"); return }
  var uid: Int? = null
  var gid: Int? = null
  if opts.owner != null {
    if let Ok(number) = opts.owner.parse_int() { uid = number } else {
      match user.lookup(opts.owner) {
        Ok(owner) => uid = owner.uid
        Err(_) => { gnu.error(f"invalid user {gnu.quote(opts.owner)}"); exit 1 }
      }
    }
  }
  if opts.group_name != null {
    if let Ok(number) = opts.group_name.parse_int() { gid = number } else {
      match group.lookup(opts.group_name) {
        Ok(found) => gid = found.gid
        Err(_) => { gnu.error(f"invalid group {gnu.quote(opts.group_name)}"); exit 1 }
      }
    }
  }
  if opts.compare and mode.bit_and(0o7000) != 0 { gnu.error("the --compare (-C) option is ignored when you specify a mode with non-permission bits") }
  if opts.directory {
    if opts.target != null or opts.no_target_directory { gnu.usage_error("target directory not allowed when installing a directory") }
    var failed = false
    for text in opts.operands {
      let dest = fp"{text}"
      if let Err(failure) = install_directory(dest, opts, mode, uid, gid) {
        gnu.cannot("create directory", text, failure)
        failed = true
      }
    }
    if failed { exit 1 }
    return
  }
  if opts.target == null and opts.operands.len() == 1 { gnu.usage_error(f"missing destination file operand after {gnu.quote(opts.operands[0])}") }
  if opts.target != null and opts.no_target_directory { gnu.usage_error("cannot combine --target-directory and --no-target-directory") }
  let dest = if opts.target != null { fp"{opts.target}" } else { fp"{opts.operands[-1]}" }
  if opts.parents and opts.target != null { make_ancestors(dest, opts.verbose)? }
  var is_dir = false
  if ! opts.no_target_directory {
    match files.directory(dest, true) {
      Ok(found) => is_dir = found
      Err(failure) => { gnu.cannot_access(dest.display(), failure); exit 1 }
    }
  }
  if dest.display().ends_with("/") and ! files.directory(dest, true)? {
    gnu.error(f"target {gnu.quote(dest.display())} is not a directory")
    exit 1
  }
  let sources = if opts.target != null { opts.operands } else { opts.operands |> take(opts.operands.len() - 1) }
  if (sources.len() > 1 or opts.target != null) and ! is_dir { gnu.error(f"target {gnu.quote(dest.display())} is not a directory"); exit 1 }
  let backup = opts.backup ?? (if opts.simple_backup or opts.suffix != null { env.get_or("VERSION_CONTROL", "existing") ?? "existing" } else { "none" })
  files.validate_backup(backup, opts.suffix ?? env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~")
  var failed = false
  var seen: List[Path] = []
  for text in sources {
    let source = fp"{text}"
    let target = if is_dir { files.destination(dest, source) } else { dest }
    if target in seen {
      gnu.error(f"will not overwrite just-created {gnu.quote(target.display())} with {gnu.quote(text)}")
      failed = true
      continue
    }
    match install_one(source, target, opts, mode, uid, gid, backup) {
      Ok(created) => { if created { seen += [target] } }
      Err(failure) => {
        if gnu.errno(failure) == 2 and ! files.present(source)? { gnu.cannot("stat", text, failure) } else { gnu.error(f"cannot install {gnu.quote(text)} to {gnu.quote(target.display())}: {gnu.strerror(failure)}") }
        failed = true
      }
    }
  }
  if failed { exit 1 }
}
