#!/bin/xsh
use lib.gnu
use lib.file_publish as files

type Options = {
  symbolic: Bool, force: Bool, interactive: Bool, no_dereference: Bool,
  no_target_directory: Bool, target: Str?, logical: Bool, physical: Bool,
  relative: Bool, verbose: Bool, backup: Str?, simple_backup: Bool, suffix: Str?,
  help: Bool, version: Bool, paths: List[Str],
}

proc link_one(source: Path, target: Path, opts: Options, policy: Str, backup: Str) -> Result[Bool] {
  if ! opts.symbolic {
    let source_meta = fs.stat(source, follow_symlinks: opts.logical)?
    if source_meta.kind == "dir" { gnu.error(f"{gnu.quote(source.display())}: hard link not allowed for directory"); return false }
  }
  var existing = files.present(target)?
  if existing and fs.stat(target)?.kind == "dir" {
    gnu.error(f"{gnu.quote(target.display())}: cannot overwrite directory")
    exit 1
  }
  if existing and files.same_entry(source, target)? {
    gnu.error(f"{gnu.quote(source.display())} and {gnu.quote(target.display())} are the same file")
    exit 1
  }
  if existing and policy == "interactive" and ! files.confirm(target, "replace")? { return false }
  var saved: Path? = null
  if existing and backup != "none" {
    saved = files.backup_name(target, backup, opts.suffix ?? env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~")?
    if saved != null {
      if files.same_entry(saved, source)? {
        gnu.error(f"backing up {gnu.quote(target.display())} might destroy source; {gnu.quote(source.display())} not linked")
        exit 1
      }
      target.rename(to: saved, overwrite: true)
      existing = false
    }
  }
  let linked_source = if opts.relative {
    files.canonical(source)?.relative_to(target.parent().resolve()?)
  } else { source }
  let result = files.publish_link(linked_source, target, opts.symbolic, opts.logical, existing and (policy != "default" or backup not in ["none", "off"]))
  if let Err(failure) = result {
    if saved != null { saved.rename(to: target, overwrite: true) }
    return Err(failure)
  }
  if opts.verbose {
    let arrow = if opts.symbolic { "->" } else { "=>" }
    let tail = if saved != null { f" (backup: {gnu.quote(saved.display())})" } else { "" }
    print f"{gnu.quote(target.display())} {arrow} {gnu.quote(linked_source.display())}{tail}"
  }
  true
}

proc main(...argv: List[Str]) {
  var opts: Options = cli.applet(argv, {
    gnu: {status: 1, unsupported: {"-d": "hard links to directories are not supported"}},
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
    backup: {form: "--backup[=CONTROL]", optional_default: "existing"},
    simple_backup: {form: "-b", default: false},
    suffix: {form: "-S --suffix SUFFIX"},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...TARGET"},
  })?
  if opts.help { gnu.help("Usage: ln [OPTION]... TARGET [LINK_NAME]\n  or: ln [OPTION]... TARGET... DIRECTORY\nCreate hard or symbolic links."); return }
  if opts.version { gnu.version("ln"); return }
  if opts.paths.is_empty() { gnu.missing_operand() }
  if opts.relative and ! opts.symbolic { gnu.usage_error("cannot do --relative without --symbolic") }
  if opts.target != null and opts.no_target_directory { gnu.usage_error("cannot combine --target-directory and --no-target-directory") }
  if opts.no_target_directory and opts.paths.len() == 1 {
    gnu.error(f"missing destination file operand after {gnu.quote(opts.paths[0])}")
    exit 1
  }
  let implicit = opts.target == null and opts.paths.len() == 1
  let dest = if opts.target != null { fp"{opts.target}" } else if implicit { p"." } else { fp"{opts.paths[-1]}" }
  let sources = if opts.target != null or implicit { opts.paths } else { opts.paths |> take(opts.paths.len() - 1) }
  var is_dir = false
  if ! opts.no_target_directory {
    match files.directory(dest, ! opts.no_dereference) {
      Ok(found) => is_dir = found
      Err(failure) => { gnu.cannot_access(dest.display(), failure); exit 1 }
    }
  }
  if (sources.len() > 1 or opts.target != null) and ! is_dir {
    gnu.error(f"target {gnu.quote(dest.display())}: Not a directory")
    exit 1
  }
  opts.logical = files.logical(argv)?
  let policy = files.overwrite(argv, "default", no_clobber: false)?
  let backup = opts.backup ?? (if opts.simple_backup or opts.suffix != null { env.get_or("VERSION_CONTROL", "existing") ?? "existing" } else { "none" })
  let configured_suffix = opts.suffix ?? env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~"
  if "/" in configured_suffix {
    # A backup suffix cannot name another path or escape the destination directory.
    opts.suffix = "~"
  }
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
    match link_one(source, target, opts, policy, backup) {
      Ok(created) => { if created { seen += [target] } else { failed = true } }
      Err(failure) => {
        let kind = if opts.symbolic { "symbolic link" } else { "hard link" }
        if opts.logical and ! opts.symbolic and gnu.errno(failure) == 2 and files.present(source)? {
          gnu.error(f"failed to access {gnu.quote(text)}: {gnu.strerror(failure)}")
        } else { gnu.error(f"failed to create {kind} {gnu.quote(target.display())}: {gnu.strerror(failure)}") }
        failed = true
      }
    }
  }
  if failed { exit 1 }
}
