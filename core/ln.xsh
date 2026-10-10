#!/bin/xsh
use lib.gnu
use lib.file_publish as files

type Options = {
  symbolic: Bool, force: Bool, interactive: Bool, no_dereference: Bool,
  no_target_directory: Bool, target: Str?, logical: Bool, physical: Bool,
  relative: Bool, verbose: Bool, backup: Str?, simple_backup: Bool, suffix: Str?,
  hard_dir: Bool, help: Bool, version: Bool, paths: List[Str],
}

# GNU's accepted backup types, grouped as its usage listing shows them.
const BACKUP_TYPE_GROUPS = [["none", "off"], ["simple", "never"], ["existing", "nil"], ["numbered", "t"]]

# The backup type named by TEXT. CONTEXT names where the value came from, because
# GNU names VERSION_CONTROL in its diagnostic when that variable supplied it.
proc backup_control(text: Str, context: Str) [process, env] -> Str {
  for pair in BACKUP_TYPE_GROUPS {
    return text when text in pair
  }

  var message = f"invalid argument {gnu.quote_value(text)} for {gnu.quote_value(context)}\nValid arguments are:"
  for pair in BACKUP_TYPE_GROUPS {
    message = f"{message}\n  - {gnu.quote_value(pair[0])}, {gnu.quote_value(pair[1])}"
  }
  gnu.usage_error(message)
  ""
}

# A bare -b, --backup, or -S selects the type named by VERSION_CONTROL. An empty
# or unset value means "existing", which is also GNU's default.
proc version_control_backup() [process, env] -> Str {
  let named = env.get_or("VERSION_CONTROL", "") ?? ""

  return "existing" when named == ""

  backup_control(named, "$VERSION_CONTROL")
}

# The link text that names TARGET from the directory BASE, climbing with ".." where
# the two paths diverge. Both paths are absolute and resolved.
proc relative_link(target: Path, base: Path) -> Result[Path] {
  let to = target.components()
  let from = base.components()
  var common = 0
  while common < to.len() and common < from.len() and to[common].bytes() == from[common].bytes() {
    common += 1
  }
  var parts: List[Bytes] = []
  for _ in range(from.len() - common) { parts += [b".."] }
  for index in range(common, to.len()) { parts += [to[index].bytes()] }
  if parts.len() == 0 { parts = [b"."] }
  var text = parts[0]
  for index in range(1, parts.len()) { text = bytes.concat([text, b"/", parts[index]]) }
  Path.parse_bytes(text)
}

# A symbolic link is refused only when a regular source names the destination
# entry itself; another name for the same inode, or a link source, is replaced
# as GNU does. A hard link is refused when the two directory entries are the same name.
proc same_destination(source: Path, target: Path, symbolic: Bool) -> Result[Bool] {
  if symbolic { files.same_symlink_entry(source, target) } else { files.same_entry(source, target) }
}

proc link_one(source: Path, target: Path, opts: Options, policy: Str, backup: Str) -> Result[Bool] {
  if ! opts.symbolic {
    # The source is statted before the destination changes, so a missing source is an access error that removes nothing.
    let source_meta = match fs.stat(source, follow_symlinks: opts.logical) {
      Ok(meta) => meta
      Err(failure) => {
        gnu.error(f"failed to access {gnu.quote_bytes(source.bytes())}: {gnu.strerror(failure)}")
        return false
      }
    }
    if source_meta.kind == "dir" and ! opts.hard_dir { gnu.error(f"{gnu.quote_bytes(source.bytes(), always: false)}: hard link not allowed for directory"); return false }
  }
  # Only -f, -i, or a backup may remove the destination. Without them an existing
  # destination reaches link(2), which reports EEXIST, so no overwrite diagnostics apply.
  let replacing = policy != "default" or backup != "none"
  var existing = match files.present(target) {
    Ok(found) => found
    Err(failure) => {
      # Only a replacing link examines the destination first, so only it reports
      # an examination failure as an access error; otherwise link(2) reports it.
      if ! replacing { return Err(failure) }
      gnu.error(f"failed to access {gnu.quote_bytes(target.bytes())}: {gnu.strerror(failure)}")
      return false
    }
  }
  if existing and replacing and fs.stat(target)?.kind == "dir" {
    gnu.error(f"{gnu.quote_bytes(target.bytes(), always: false)}: cannot overwrite directory")
    exit 1
  }
  # GNU backs up a destination that names the source before making a symbolic
  # link, so the link may name its own destination. A hard link to itself is refused.
  let self_symlink_with_backup = opts.symbolic and backup != "none"
  if existing and replacing and ! self_symlink_with_backup and same_destination(source, target, opts.symbolic)? {
    gnu.error(f"{gnu.quote_bytes(source.bytes())} and {gnu.quote_bytes(target.bytes())} are the same file")
    exit 1
  }
  if existing and policy == "interactive" and ! files.confirm(target, "replace")? { return false }
  var saved: Path? = null
  if existing and backup != "none" {
    saved = files.backup_name(target, backup, opts.suffix ?? "~")?
    if saved != null {
      target.rename(to: saved, overwrite: true)
      # rename(2) does nothing when the destination and the backup name are one hard
      # link, so the original name is removed here as GNU does before linking.
      if files.present(target)? { target.remove() }
      existing = false
    }
  }
  let linked_source = if opts.relative {
    relative_link(files.canonical(source)?, target.parent().resolve()?)?
  } else { source }
  let result = files.publish_link(linked_source, target, opts.symbolic, opts.logical, existing and (policy != "default" or backup not in ["none", "off"]))
  if let Err(failure) = result {
    if saved != null { saved.rename(to: target, overwrite: true) }
    return Err(failure)
  }
  if opts.verbose {
    let arrow = if opts.symbolic { "->" } else { "=>" }
    let tail = if saved != null { f" (backup: {gnu.quote_bytes(saved.bytes())})" } else { "" }
    print f"{gnu.quote_bytes(target.bytes())} {arrow} {gnu.quote_bytes(linked_source.bytes())}{tail}"
  }
  true
}

proc main(...argv: List[Bytes]) {
  let prepared = gnu.prepare_arguments(argv)
  var opts: Options = cli.applet(prepared.text, {
    gnu: {status: 1},
    symbolic: {form: "-s --symbolic", default: false},
    force: {form: "-f --force", default: false},
    interactive: {form: "-i --interactive", default: false},
    no_dereference: {form: "-n --no-dereference", default: false},
    no_target_directory: {form: "-T --no-target-directory", default: false},
    target: {form: "-t --target-directory DIR"},
    logical: {form: "-L --logical", default: false},
    physical: {form: "-P --physical", default: false},
    relative: {form: "-r --relative", default: false},
    verbose: {form: "-v --verbose", default: false},
    backup: {form: "--backup[=CONTROL]", optional_default: ""},
    simple_backup: {form: "-b", default: false},
    suffix: {form: "-S --suffix SUFFIX"},
    hard_dir: {form: "-d -F --directory", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...TARGET"},
  })?
  if opts.help { gnu.help("Usage: ln [OPTION]... TARGET [LINK_NAME]\n  or: ln [OPTION]... TARGET... DIRECTORY\nCreate hard or symbolic links."); return }
  if opts.version { gnu.version("ln"); return }
  if opts.paths.is_empty() { gnu.usage_error("missing file operand") }
  if opts.relative and ! opts.symbolic {
    gnu.error("cannot do --relative without --symbolic")
    exit 1
  }
  var paths: List[Path] = []
  for raw_path in opts.paths { paths += [Path.parse_bytes(gnu.argument_bytes(raw_path, prepared.raw))?] }
  let target_directory: Path? = if let target_text = opts.target {
    Path.parse_bytes(gnu.argument_bytes(target_text, prepared.raw))?
  } else { null }
  # GNU follows a symbolic link named by -t even under -n, and every failure to
  # examine it is an access error. This runs before the -T conflict checks.
  if let directory = target_directory {
    match fs.stat(directory, follow_symlinks: true) {
      Ok(meta) => {
        if meta.kind != "dir" {
          gnu.error(f"target {gnu.quote_bytes(directory.bytes())} is not a directory")
          exit 1
        }
      }
      Err(failure) => {
        gnu.error(f"failed to access {gnu.quote_bytes(directory.bytes())}: {gnu.strerror(failure)}")
        exit 1
      }
    }
  }
  if opts.target != null and opts.no_target_directory {
    gnu.error("cannot combine --target-directory and --no-target-directory")
    exit 1
  }
  if opts.no_target_directory {
    if paths.len() == 1 {
      gnu.usage_error(f"missing destination file operand after {gnu.quote_bytes(paths[0].bytes())}")
    }
    if paths.len() > 2 {
      gnu.usage_error(f"extra operand {gnu.quote_bytes(paths[2].bytes())}")
    }
  }
  let implicit = target_directory == null and paths.len() == 1
  let dest = target_directory ?? (if implicit { p"." } else { paths[-1] })
  let sources = if target_directory != null or implicit { paths } else { paths |> take(paths.len() - 1) }
  var is_dir = target_directory != null
  if target_directory == null and ! opts.no_target_directory {
    match files.directory(dest, ! opts.no_dereference) {
      Ok(found) => is_dir = found
      Err(failure) => {
        # A name that is too long or loops cannot be a directory; GNU links over it
        # anyway, and reports it only where a directory is required.
        if gnu.errno(failure) in [36, 40] { is_dir = false } else {
          gnu.error(f"failed to access {gnu.quote_bytes(dest.bytes())}: {gnu.strerror(failure)}")
          exit 1
        }
      }
    }
  }
  if sources.len() > 1 and ! is_dir {
    match fs.stat(dest, follow_symlinks: ! opts.no_dereference) {
      Ok(_) => gnu.error(f"target {gnu.quote_bytes(dest.bytes())}: Not a directory")
      Err(failure) => gnu.error(f"target {gnu.quote_bytes(dest.bytes())}: {gnu.strerror(failure)}")
    }
    exit 1
  }
  opts.logical = files.logical(prepared.text)?
  let policy = files.overwrite(prepared.text, "default", no_clobber: false)?
  var backup = "none"
  if let requested = opts.backup {
    backup = if requested == "" { version_control_backup() } else { backup_control(requested, "backup type") }
  } else if opts.simple_backup or opts.suffix != null {
    backup = version_control_backup()
  }
  var suffix = opts.suffix ?? ""
  if suffix == "" { suffix = env.get_or("SIMPLE_BACKUP_SUFFIX", "") ?? "" }
  # A backup suffix cannot name another path or escape the destination directory.
  if suffix == "" or "/" in suffix { suffix = "~" }
  opts.suffix = suffix
  let replacing = policy != "default" or backup != "none"
  var failed = false
  var seen: List[Path] = []
  for source in sources {
    let target = if is_dir { files.destination(dest, source) } else { dest }
    # A plain link into a name this run already created fails with EEXIST below;
    # only a replacing link reports the overwrite it would have performed.
    if replacing and target in seen {
      gnu.error(f"will not overwrite just-created {gnu.quote_bytes(target.bytes())} with {gnu.quote_bytes(source.bytes())}")
      failed = true
      continue
    }
    match link_one(source, target, opts, policy, backup) {
      Ok(created) => { if created { seen += [target] } else { failed = true } }
      Err(failure) => {
        let kind = if opts.symbolic { "symbolic link" } else { "hard link" }
        let code = gnu.errno(failure)
        # EEXIST (17) names only the destination; other hard-link failures also name the source.
        if ! opts.symbolic and code != 17 {
          gnu.error(f"failed to create hard link {gnu.quote_bytes(target.bytes())} => {gnu.quote_bytes(source.bytes())}: {gnu.strerror(failure)}")
        } else { gnu.error(f"failed to create {kind} {gnu.quote_bytes(target.bytes())}: {gnu.strerror(failure)}") }
        failed = true
      }
    }
  }
  if failed { exit 1 }
}
