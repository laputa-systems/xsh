#!/bin/xsh
use lib.gnu
use lib.file_publish as files

type Options = {
  directory: Bool, parents: Bool, mode: Str, owner: Str?, group_name: Str?,
  preserve: Bool, compare: Bool, verbose: Bool, target: Str?, no_target_directory: Bool,
  backup: Str?, simple_backup: Bool, suffix: Str?, copy: Bool, unprivileged: Bool,
  strip: Bool, strip_program: Str?,
  security_context: Bool, context: List[Str], preserve_context: Bool,
  help: Bool, version: Bool, operands: List[Str],
}

# argv cannot contain a NUL byte, so this marks a bare --context, which carries no value.
const NO_CONTEXT_VALUE = "\0"

enum InstallOutcome { Installed, Unchanged, Failed }

# libselinux reports SELinux as enabled when the kernel lists selinuxfs. Labelling
# is not implemented, so a context request is refused there instead of being
# dropped; on kernels without SELinux GNU's no-op is exact.
proc selinux_enabled() -> Result[Bool] {
  Ok("selinuxfs" in fp"/proc/filesystems".read_text()?)
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

proc install_one(source: Path, target: Path, opts: Options, mode: Int, uid: Int?, gid: Int?, backup: Str) -> Result[InstallOutcome] {
  # Kernel descriptor aliases can name streams that have no canonical pathname.
  # Follow them for metadata and let the copier open the original source path.
  let metadata = fs.stat(source, follow_symlinks: true)?
  if metadata.kind == "dir" {
    gnu.error(f"omitting directory {gnu.quote_bytes(source.bytes())}")
    exit 1
  }
  if metadata.kind not in ["file", "fifo", "char", "block"] {
    gnu.error(f"cannot install {gnu.quote_bytes(source.bytes())}: unsupported file type {gnu.quote(metadata.kind)}")
    exit 1
  }
  let existing = files.present(target)?
  if existing {
    let old = fs.stat(target)?
    if (metadata.dev == old.dev and metadata.ino == old.ino) or files.same_entry(source, target)? {
      gnu.error(f"{gnu.quote_bytes(source.bytes())} and {gnu.quote_bytes(target.bytes())} are the same file")
      exit 1
    }
  }
  if existing and fs.stat(target)?.kind == "dir" {
    gnu.error(f"cannot overwrite directory {gnu.quote_bytes(target.bytes())} with non-directory")
    exit 1
  }
  if existing and opts.compare and metadata.kind == "file" {
    let old = fs.stat(target)?
    if old.kind == "file" and old.size == metadata.size and old.mode.bit_and(0o7777) == mode and
      mode.bit_and(0o7000) == 0 and metadata.mode.bit_and(0o7000) == 0 and
      old.mode.bit_and(0o7000) == 0 and (! opts.preserve or old.mtime_ns == metadata.mtime_ns) and
      old.uid == (uid ?? user.current()?.uid) and old.gid == (gid ?? group.current()?.gid) and
      (uid == null or old.uid == uid) and (gid == null or old.gid == gid) and
      equal_contents(source, target, metadata.size)? { return Unchanged }
  }
  var saved: Path? = null
  if existing { saved = files.backup_name(target, backup, opts.suffix ?? env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~")? }
  if saved != null and files.same_entry(saved, source)? {
    gnu.error(f"backing up {gnu.quote_bytes(target.bytes())} might destroy source;  {gnu.quote_bytes(source.bytes())} not copied")
    exit 1
  }
  if opts.parents { make_ancestors(target.parent(), opts.verbose)? }
  let resolved_parent = match target.parent().resolve() {
    Ok(parent) => parent
    Err(failure) => {
      if existing {
        gnu.error(f"cannot remove {gnu.quote_bytes(target.bytes())}: {gnu.strerror(failure)}")
        return Failed
      }
      return Err(failure)
    }
  }
  let parent_root = match fs.open_root(resolved_parent) {
    Ok(root) => root
    Err(failure) => {
      if existing {
        gnu.error(f"cannot remove {gnu.quote_bytes(target.bytes())}: {gnu.strerror(failure)}")
        return Failed
      }
      return Err(failure)
    }
  }
  defer parent_root.close()
  # Publishing a prepared regular file replaces a symlink entry without
  # touching the object it names, and leaves existing files intact on failure.
  let scratch = fs.tempfile()?
  defer scratch.root.close()
  let staged_dir_name = fp".xsh-install-{scratch.root.host_path()?.name()}"
  if let Err(failure) = parent_root.mkdir(staged_dir_name, mode: 0o700.clear_bits(fs.umask()?)) {
    if existing and gnu.strerror(failure) in ["No such file or directory", "Permission denied", "Operation not permitted", "Read-only file system"] {
      gnu.error(f"cannot remove {gnu.quote_bytes(target.bytes())}: {gnu.strerror(failure)}")
      return Failed
    }
    return Err(failure)
  }
  let staged_dir = fp"{target.parent()}/{staged_dir_name}"
  defer staged_dir.remove()
  let staged = fp"{staged_dir}/{target.name()}"
  let _ = fs.copy_file(source, staged, reflink: "auto", sparse: "never", mode: 0o600)?
  defer staged.remove()
  if opts.strip {
    let program = opts.strip_program ?? "strip"
    if "/" in program {
      if let Err(failure) = fs.stat(fp"{program}") {
        gnu.error(f"strip program failed: {gnu.strerror(failure)}")
        return Failed
      }
    }
    let executable = match process.which(program) {
      Ok(found) => found
      Err(failure) => {
        gnu.error(f"strip program failed: {gnu.strerror(failure)}")
        return Failed
      }
    }
    let strip_path = if target.name().starts_with("-") { "./" + target.name() } else { target.name() }
    match process.run(process.command_argv(executable, [program, strip_path], cwd: staged_dir)) {
      Ok(status) => {
        if ! status.exited() {
          gnu.error("strip process terminated abnormally")
          return Failed
        }
        if ! status.exited_with(0) {
          gnu.error("strip program failed")
          return Failed
        }
      }
      Err(failure) => {
        gnu.error(f"strip program failed: {gnu.strerror(failure)}")
        return Failed
      }
    }
  }
  fs.set_owner(staged, uid: uid, gid: gid)
  staged.chmod(mode)
  if opts.preserve {
    let copied = fs.stat(source, follow_symlinks: true)?
    fs.set_times(staged, atime_ns: copied.atime_ns, mtime_ns: copied.mtime_ns)
  }
  if saved != null {
    if let Err(failure) = target.rename(to: saved, overwrite: true) {
      gnu.error(f"cannot backup {gnu.quote_bytes(target.bytes())}: {gnu.strerror(failure)}")
      return Failed
    }
  }
  if let Err(failure) = staged.rename(to: target, overwrite: true) {
    if saved != null { saved.rename(to: target, overwrite: true) }
    return Err(failure)
  }
  if opts.verbose {
    if existing and saved == null { print f"removed {gnu.quote_bytes(target.bytes())}" }
    print f"{gnu.quote_bytes(source.bytes())} -> {gnu.quote_bytes(target.bytes())}"
  }
  Installed
}

# Intermediate directories keep the default mode the kernel gives new directories
# (modified by the umask), not the installed mode, so -m never reaches them.
proc make_ancestors(dest: Path, verbose = false) -> Result[Unit] {
  if files.present(dest)? { return }
  make_ancestors(dest.parent(), verbose)?
  if let Err(failure) = dest.mkdir() {
    if ! files.directory(dest, true)? { return Err(failure) }
  }
  if verbose { print f"install: creating directory {gnu.quote_bytes(dest.bytes())}" }
}

proc install_directory(raw: Path, opts: Options, mode: Int, uid: Int?, gid: Int?) -> Result[Unit] {
  var raw_bytes = raw.bytes()
  while raw_bytes.len() > 1 and raw_bytes.byte_at(raw_bytes.len() - 1) == 47 {
    raw_bytes = raw_bytes[..raw_bytes.len() - 1]
  }
  while raw_bytes.ends_with(b"/.") { raw_bytes = raw_bytes[..raw_bytes.len() - 2] }
  let dest = if raw_bytes.is_empty() { p"/" } else { Path.parse_bytes(raw_bytes)? }
  let existed = files.present(dest)?
  make_ancestors(dest.parent(), opts.verbose)?
  dest.mkdir(parents: true)
  if let Err(failure) = fs.set_owner(dest, uid: uid, gid: gid, follow_symlinks: true) {
    # A new directory can inherit setgid from a setgid parent; a failed ownership
    # change must not leave special bits on the directory it created.
    if ! existed { dest.chmod(mode.clear_bits(0o6000))? }
    return Err(failure)
  }
  dest.chmod(mode)
  if opts.verbose and ! existed {
    let raw_bytes = raw.bytes()
    let has_trailing_dot = raw_bytes.ends_with(b"/.") or raw_bytes.ends_with(b"/./")
    let name = if has_trailing_dot { dest.normalize() } else { dest }
    print f"install: creating directory {gnu.quote_bytes(name.bytes())}"
  }
}

proc main(...argv: List[Bytes]) {
  let prepared = gnu.prepare_arguments(argv)
  if (b"-C" in argv or b"--compare" in argv) and (b"-s" in argv or b"--strip" in argv) {
    gnu.error("Options --compare and --strip are mutually exclusive")
    exit 1
  }
  if b"-T" in argv and argv.len() > 0 and argv[-1] in [b"-t", b"--target-directory"] {
    gnu.error("a value is required for '--target-directory <DIRECTORY>' but none was supplied")
    eprint "For more information, try '--help'"
    exit 1
  }
  let opts: Options = cli.applet(prepared.text, {
    gnu: {status: 1, unsupported: {
      "--debug": "copy diagnostics are not available",
    }},
    security_context: {form: "-Z", default: false},
    context: {form: "--context[=CONTEXT]", repeated: true, optional_default: NO_CONTEXT_VALUE},
    preserve_context: {form: "--preserve-context", default: false},
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
    unprivileged: {form: "-U", default: false},
    copy: {form: "-c", default: false},
    strip: {form: "-s --strip", default: false},
    strip_program: {form: "--strip-program PROGRAM"},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    operands: {form: "...SOURCE"},
  })?
  # Warnings come before --help so that options parsed ahead of it still report, as in GNU.
  let selinux = selinux_enabled()?
  if ! selinux {
    for context in opts.context {
      if context != NO_CONTEXT_VALUE { gnu.error("warning: ignoring --context; it requires an SELinux-enabled kernel") }
    }
    if opts.preserve_context { gnu.error("WARNING: ignoring --preserve-context; this kernel is not SELinux-enabled") }
  }
  if opts.help { gnu.help("Usage: install [OPTION]... SOURCE... DEST\n  or: install -d [OPTION]... DIRECTORY...\nCopy files and set their attributes."); return }
  if opts.version { gnu.version("install"); return }
  if selinux and (opts.security_context or opts.preserve_context or ! opts.context.is_empty()) {
    gnu.error("security contexts are not supported on SELinux-enabled systems")
    exit 1
  }
  if opts.operands.is_empty() { gnu.usage_error("missing file operand") }
  if opts.strip_program != null and ! opts.strip { gnu.error("WARNING: ignoring --strip-program option as -s option was not specified") }
  let mode_bytes = gnu.argument_bytes(opts.mode, prepared.raw)
  let mode_spec = match mode_bytes.utf8() {
    Ok(text) => text
    Err(_) => { gnu.usage_error(f"invalid mode {gnu.quote_bytes(mode_bytes)}"); exit 1 }
  }
  let mode = install_mode(mode_spec.trim(), opts.directory)
  if mode == null {
    if rx"^[0-9]+$".matches(mode_spec.trim()) and ! rx"^[0-7]+$".matches(mode_spec.trim()) {
      gnu.usage_error("Invalid mode string: invalid digit found in string")
    }
    var bad_operator = ""
    var bad_operator_offset = 0
    var mode_offset = 0
    for character in mode_spec.trim() {
      if character not in "ugoa=+-rwxXstugo," {
        bad_operator = character
        bad_operator_offset = mode_offset
        break
      }
      mode_offset += character.byte_len()
    }
    if bad_operator != "" and bad_operator.byte_len() == 1 and ! rx"^[A-Za-z0-9]$".matches(bad_operator) {
      if let Ok(_) = unix.tty_attrs(2) {
        gnu.error(f"invalid operator {gnu.quote(bad_operator)}")
        eprint f"╭─[ {gnu.prog()}:1:{4 + bad_operator_offset} ]"
        eprint f"│ -m {mode_spec.trim()}"
        let spaces = [" "] |> repeat(3 + bad_operator_offset) |> join("")
        eprint f"│ {spaces}^"
        eprint "╰─"
      } else {
        gnu.error(f"invalid operator {gnu.quote(bad_operator)}")
      }
    } else {
      gnu.usage_error(f"invalid mode {gnu.quote(mode_spec)}")
    }
    exit 1
  }
  var uid: Int? = null
  var gid: Int? = null
  if opts.owner != null {
    if let Ok(number) = opts.owner.parse_int() { uid = number } else {
      match user.lookup(opts.owner) {
        Ok(owner) => uid = owner.uid
        Err(_) => { gnu.error(f"invalid user: {gnu.quote(opts.owner)}"); exit 1 }
      }
    }
  }
  if opts.group_name != null {
    if let Ok(number) = opts.group_name.parse_int() { gid = number } else {
      match group.lookup(opts.group_name) {
        Ok(found) => gid = found.gid
        Err(_) => { gnu.error(f"invalid group: {gnu.quote(opts.group_name)}"); exit 1 }
      }
    }
  }
  if opts.unprivileged { uid = null; gid = null }
  if opts.compare and mode.bit_and(0o7000) != 0 { gnu.error("the --compare (-C) option is ignored when you specify a mode with non-permission bits") }
  var operands: List[Path] = []
  for operand in opts.operands { operands += [Path.parse_bytes(gnu.argument_bytes(operand, prepared.raw))?] }
  let target_directory: Path? = if let target_text = opts.target {
    Path.parse_bytes(gnu.argument_bytes(target_text, prepared.raw))?
  } else { null }
  if opts.directory {
    if target_directory != null or opts.no_target_directory { gnu.usage_error("target directory not allowed when installing a directory") }
    var failed = false
    for dest in operands {
      if let Err(failure) = install_directory(dest, opts, mode, uid, gid) {
        gnu.error(f"cannot create directory {gnu.quote_bytes(dest.bytes())}: {gnu.strerror(failure)}")
        failed = true
      }
    }
    if failed { exit 1 }
    return
  }
  if target_directory == null and operands.len() == 1 { gnu.usage_error(f"missing destination file operand after {gnu.quote_bytes(operands[0].bytes())}") }
  if target_directory != null and opts.no_target_directory {
    gnu.usage_error("Options --target-directory and --no-target-directory are mutually exclusive")
  }
  if opts.no_target_directory and target_directory == null and operands.len() > 2 {
    gnu.usage_error(f"extra operand {gnu.quote_bytes(operands[2].bytes())}\nUsage: install [OPTION]... [FILE]...")
  }
  let dest = target_directory ?? operands[-1]
  if target_directory != null and dest.bytes().ends_with(b"/") {
    if let Err(failure) = fs.stat(dest, follow_symlinks: true) {
      if gnu.errno(failure) == 20 {
        gnu.error(f"failed to access {gnu.quote_bytes(dest.bytes())}: {gnu.strerror(failure)}")
        exit 1
      }
    }
  }
  if opts.parents and target_directory != null { make_ancestors(dest, opts.verbose)? }
  var is_dir = false
  if ! opts.no_target_directory {
    match files.directory(dest, true) {
      Ok(found) => is_dir = found
      Err(failure) => {
        let reason = gnu.strerror(failure)
        if opts.parents and reason in ["Filename too long", "File name too long"] {
          gnu.error(f"cannot create directory {gnu.quote_bytes(dest.parent().bytes())}: {reason}")
        } else {
          gnu.error(f"cannot access {gnu.quote_bytes(dest.bytes())}: {reason}")
        }
        exit 1
      }
    }
  }
  if target_directory != null and ! is_dir {
    match fs.stat(dest, follow_symlinks: true) {
      Ok(_) => { gnu.error(f"failed to access {gnu.quote_bytes(dest.bytes())}: Not a directory"); exit 1 }
      Err(failure) => { gnu.error(f"failed to access {gnu.quote_bytes(dest.bytes())}: {gnu.strerror(failure)}"); exit 1 }
    }
  }
  if dest.bytes().ends_with(b"/") and ! files.directory(dest, true)? {
    gnu.error(f"target {gnu.quote_bytes(dest.bytes())} is not a directory")
    exit 1
  }
  let sources = if target_directory != null { operands } else { operands |> take(operands.len() - 1) }
  if (sources.len() > 1 or target_directory != null) and ! is_dir { gnu.error(f"target {gnu.quote_bytes(dest.bytes())} is not a directory"); exit 1 }
  let backup = opts.backup ?? (if opts.simple_backup or opts.suffix != null { env.get_or("VERSION_CONTROL", "existing") ?? "existing" } else { "none" })
  files.validate_backup(backup, opts.suffix ?? env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~")
  var failed = false
  var seen: List[Path] = []
  for source in sources {
    let target = if is_dir { files.destination(dest, source) } else { dest }
    if target in seen {
      gnu.error(f"will not overwrite just-created {gnu.quote_bytes(target.bytes())} with {gnu.quote_bytes(source.bytes())}")
      failed = true
      continue
    }
    match install_one(source, target, opts, mode, uid, gid, backup) {
      Ok(outcome) => {
        if outcome == Installed { seen += [target] }
        if outcome == Failed { failed = true }
      }
      Err(failure) => {
        if gnu.errno(failure) == 2 and ! files.present(source)? {
          gnu.error(f"cannot stat {gnu.quote_bytes(source.bytes())}: {gnu.strerror(failure)}")
        } else {
          gnu.error(f"cannot install {gnu.quote_bytes(source.bytes())} to {gnu.quote_bytes(target.bytes())}: {gnu.strerror(failure)}")
        }
        failed = true
      }
    }
  }
  if failed { exit 1 }
}
