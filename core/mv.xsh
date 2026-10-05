#!/bin/xsh
use lib.gnu
use lib.file_publish as files

type Options = {
  no_target_directory: Bool, no_clobber: Bool, force: Bool, interactive: Bool,
  target: Str?, verbose: Bool, update: Str?, older: Bool,
  backup: Str?, simple_backup: Bool, suffix: Str?, strip_slashes: Bool,
  help: Bool, version: Bool, operands: List[Str],
}

proc move_one(source: Path, target: Path, opts: Options, policy: Str, backup: Str) -> Result[Bool] {
  let metadata = fs.stat(source)?
  let existing = files.present(target)?
  if existing {
    let dest_meta = fs.stat(target)?
    if metadata.dev == dest_meta.dev and metadata.ino == dest_meta.ino {
      gnu.error(f"{gnu.quote(source.display())} and {gnu.quote(target.display())} are the same file")
      return false
    }
    if policy == "skip" or opts.update == "none" { return true }
    if opts.update == "none-fail" {
      gnu.error(f"not replacing {gnu.quote(target.display())}")
      return false
    }
    if opts.update == "older" and metadata.mtime_ns <= dest_meta.mtime_ns { return true }
    if policy == "interactive" and ! files.confirm(target)? { return true }
    if metadata.kind == "dir" and dest_meta.kind != "dir" {
      gnu.error(f"cannot overwrite non-directory {gnu.quote(target.display())} with directory {gnu.quote(source.display())}")
      return false
    }
    if metadata.kind != "dir" and dest_meta.kind == "dir" {
      gnu.error(f"cannot overwrite directory {gnu.quote(target.display())} with non-directory")
      return false
    }
  }
  var saved: Path? = null
  if existing {
    saved = files.backup_name(target, backup, opts.suffix ?? env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~")?
    if saved != null {
      if files.same_entry(saved, source)? {
        gnu.error(f"backing up {gnu.quote(target.display())} might destroy source; {gnu.quote(source.display())} not moved")
        return false
      }
      target.rename(to: saved, overwrite: true)
    }
  }
  let moved = if policy == "skip" { fs.rename_noreplace(source, target) } else { source.rename(to: target, overwrite: true) }
  if let Err(failure) = moved {
    if saved != null { saved.rename(to: target, overwrite: true) }
    return Err(failure)
  }
  if opts.verbose {
    let tail = if saved != null { f" (backup: {gnu.quote(saved.display())})" } else { "" }
    print f"renamed {gnu.quote(source.display())} -> {gnu.quote(target.display())}{tail}"
  }
  true
}

proc main(...argv: List[Str]) {
  var opts: Options = cli.applet(argv, {
    gnu: {status: 1, unsupported: {
      "--exchange": "atomic exchange is not available",
      "--no-copy": "cross-device copy is not available",
      "--debug": "copy diagnostics are not available",
      "-Z": "security contexts are not available",
    }},
    no_target_directory: {form: "-T --no-target-directory", default: false},
    no_clobber: {form: "-n --no-clobber", default: false},
    force: {form: "-f --force", default: false},
    interactive: {form: "-i --interactive", default: false},
    target: {form: "-t --target-directory DIR"},
    verbose: {form: "-v --verbose", default: false},
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
  if opts.operands.is_empty() { gnu.missing_operand() }
  if opts.target == null and opts.operands.len() == 1 { gnu.missing_operand_after(opts.operands[0]) }
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
  let policy = files.overwrite(argv, "force")?
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
    let target = if is_dir { fp"{dest}/{source.name()}" } else { dest }
    if target in seen {
      gnu.error(f"will not overwrite just-created {gnu.quote(target.display())} with {gnu.quote(text)}")
      failed = true
      continue
    }
    match move_one(source, target, opts, policy, backup) {
      Ok(ok) => { if ! ok { failed = true } else { seen += [target] } }
      Err(failure) => {
        if gnu.errno(failure) == 2 and ! files.present(source)? { gnu.cannot("stat", text, failure) } else { gnu.error(f"cannot move {gnu.quote(text)} to {gnu.quote(target.display())}: {gnu.strerror(failure)}") }
        failed = true
      }
    }
  }
  if failed { exit 1 }
}
