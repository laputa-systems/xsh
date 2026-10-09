#!/bin/xsh
use lib.gnu

const USAGE = """Usage: cp [OPTION]... SOURCE... DEST
  or:  cp [OPTION]... -t DIRECTORY SOURCE...
Copy SOURCE to DEST, or multiple SOURCEs to DIRECTORY.

  -a, --archive                same as -dR --preserve=all
  -f, --force                  remove destination before opening
  -i, --interactive            prompt before overwrite
  -l, --link                   hard link files instead of copying
  -n, --no-clobber             do not overwrite an existing file
  -p                           same as --preserve=mode,ownership,timestamps
  -P, --no-dereference         never follow symbolic links in SOURCE
  -R, -r, --recursive          copy directories recursively
  -s, --symbolic-link          make symbolic links instead of copying
  -t, --target-directory=DIR   copy all SOURCE arguments into DIR
  -T, --no-target-directory    treat DEST as a normal file
  -u, --update[=WHEN]          copy only when SOURCE is newer
  -v, --verbose                explain what is being done
      --backup[=CONTROL]       make a backup of each existing destination
      --attributes-only        copy only the file attributes
      --copy-contents           copy contents of special files when recursive
      --no-preserve=ATTR_LIST  don't preserve the specified attributes
      --parents                use full source file name under DIRECTORY
      --preserve[=ATTR_LIST]   preserve the specified attributes
      --reflink[=WHEN]         control clone copies
      --remove-destination     remove each existing destination first
      --sparse=WHEN            control creation of sparse files
      --strip-trailing-slashes remove trailing slashes from each SOURCE
      --suffix=SUFFIX          override the usual backup suffix
      --help                   display this help and exit
      --version                output version information and exit
"""

type CpOptions = {
  archive: Bool,
  attributes_only: Bool,
  backup: Str,
  backup_short: Bool,
  cli_dereference: Bool,
  copy_contents: Bool,
  dereference: Bool,
  force: Bool,
  help: Bool,
  hardlink: Bool,
  interactive: Bool,
  no_clobber: Bool,
  no_dereference: Bool,
  no_deref_links: Bool,
  no_preserve: Str,
  no_target_directory: Bool,
  one_file_system: Bool,
  parents: Bool,
  preserve: Str,
  recursive: Bool,
  reflink: Str,
  remove_destination: Bool,
  sparse: Str,
  strip_trailing_slashes: Bool,
  suffix: Str,
  symlink: Bool,
  target: Str,
  update: Str,
  version: Bool,
  verbose: Bool,
  operands: List[Str],
}

type CpStat = {
  atime_ns: Int,
  birth_ns: Int?,
  blksize: Int,
  blocks_512: Int,
  ctime_ns: Int,
  dev: Int,
  gid: Int,
  ino: Int,
  kind: Str,
  mode: Int,
  mtime_ns: Int,
  nlink: Int,
  rdev: Int,
  size: Int,
  uid: Int,
}

pure dereference_mode(arguments: List[Str]) -> Str {
  var mode = "default"
  var parsing = true

  for argument in arguments {
    if ! parsing { continue }
    if argument == "--" {
      parsing = false
      continue
    }
    if argument == "--archive" { mode = "preserve"; continue }
    if argument == "--dereference" { mode = "follow"; continue }
    if argument == "--no-dereference" { mode = "never"; continue }
    if argument.starts_with("--") or argument == "-" or ! argument.starts_with("-") { continue }

    var index = 1
    while index < argument.byte_len() {
      let flag = argument.byte_slice(index, length: 1)
      if flag == "a" { mode = "preserve" }
      if flag == "L" { mode = "follow" }
      if flag == "P" or flag == "d" { mode = "never" }
      if flag == "H" { mode = "command-line" }
      if flag == "S" or flag == "t" { break }
      index += 1
    }
  }

  mode
}

proc path_exists(candidate: Path) [fs] -> Bool {
  match fs.stat(candidate, follow_symlinks: false) {
    Ok(_) => true
    Err(_) => false
  }
}

pure same_identity(left: CpStat, right: CpStat) -> Bool {
  left.dev == right.dev and left.ino == right.ino
}

pure has_attribute(opts: CpOptions, name: Str) -> Bool {
  let preserve_parts = opts.preserve.split(",")
  let chosen = if opts.archive or opts.preserve == "all" {
    ["mode", "ownership", "timestamps", "links"]
  } else if opts.no_deref_links {
    ["links"]
  } else if opts.preserve == "default" or "default" in preserve_parts {
    ["mode", "ownership", "timestamps"]
  } else {
    preserve_parts
  }
  let removed = opts.no_preserve.split(",")

  ! ("all" in removed) and name in chosen and ! (name in removed)
}

pure slashless(text: Str) -> Str {
  var end = text.byte_len()
  while end > 1 and text.byte_slice(end - 1, length: 1) == "/" {
    end -= 1
  }

  text.byte_slice(0, length: end)
}

pure dest_for(source: Path, target: Path, target_is_dir: Bool) -> Path {
  return target when source.display().ends_with("/.")

  return fp"{target}/{source.name()}" when target_is_dir

  target
}

pure parent_dest(source: Str, target: Path) -> Path {
  var result = target
  for component in source.split("/") {
    continue when component == "" or component == "."
    result = fp"{result}/{component}"
  }

  result
}

pure simple_backup(target_path: Path, suffix: Str) -> Path {
  fp"{target_path}{suffix}"
}

proc numbered_backup(target_path: Path) [fs] -> Path {
  var number = 1
  var candidate = fp"{target_path}.~{number}~"
  while path_exists(candidate) {
    number += 1
    candidate = fp"{target_path}.~{number}~"
  }

  candidate
}

proc choose_backup(target_path: Path, mode: Str, suffix: Str) [fs] -> Path {
  if mode == "numbered" or mode == "t" {
    return numbered_backup(target_path)
  }

  if mode == "simple" or mode == "never" {
    return simple_backup(target_path, suffix)
  }

  if mode == "existing" or mode == "nil" {
    let simple = simple_backup(target_path, suffix)
    var numbered = false
    for number in range(1, 1000) {
      if path_exists(fp"{target_path}.~{number}~") {
        numbered = true
        break
      }
    }

    return numbered_backup(target_path) when numbered
    return simple
  }

  simple_backup(target_path, suffix)
}

proc same_file(source: Path, source_meta: CpStat, dest: Path, follow_source: Bool) [fs] -> Bool {
  if let Ok(dest_link) = fs.stat(dest, follow_symlinks: false) {
    if let Ok(source_link) = fs.stat(source, follow_symlinks: false) {
      if same_identity(source_link, dest_link) { return true }
    }
  }

  if let Ok(source_follow) = fs.stat(source, follow_symlinks: follow_source) {
    if let Ok(dest_follow) = fs.stat(dest, follow_symlinks: true) {
      return same_identity(source_follow, dest_follow)
    }
  }

  let _ = source_meta
  false
}

proc prompt_overwrite(target_path: Path) [process, env, io] -> Bool {
  eprint f"cp: overwrite {gnu.quote(target_path.display())}?"
  let answer = match io.stdin_line() {
    Ok(text) => text.trim().lower()
    Err(_) => ""
  }

  answer.starts_with("y")
}

proc prepare_leaf(source_meta: CpStat, source: Path, dest: Path, opts: CpOptions, follow_source: Bool) [fs, process, env, io, error] -> Result[Bool] {
  if ! path_exists(dest) {
    return true
  }

  if opts.no_clobber {
    return false
  }

  if opts.update == "none" or opts.update == "none-fail" {
    if opts.update == "none-fail" {
      error.fail(f"not overwriting {gnu.quote(dest.display())}")?
    }

    return false
  }

  if opts.update == "older" {
    if let Ok(dest_meta) = fs.stat(dest, follow_symlinks: true) {
      if source_meta.mtime_ns <= dest_meta.mtime_ns {
        return false
      }
    }
  }

  if opts.interactive and ! prompt_overwrite(dest) {
    return false
  }

  if ! opts.remove_destination {
    if let Ok(dest_link) = fs.stat(dest, follow_symlinks: false) {
      let posix = env.get_or("POSIXLY_CORRECT", "") ?? ""
      if dest_link.kind == "symlink" and posix == "" {
        if let Err(_) = fs.stat(dest, follow_symlinks: true) {
          error.fail(f"not writing through dangling symlink {gnu.quote(dest.display())}")?
        }
      }
    }
  }

  let mode = if opts.backup != "" {
    opts.backup
  } else if opts.backup_short or opts.suffix != "" {
    env.get_or("VERSION_CONTROL", "existing") ?? "existing"
  } else {
    "none"
  }
  let actual_mode = if mode == "off" { "none" } else { mode }
  if actual_mode != "none" {
    let suffix = if opts.suffix != "" { opts.suffix } else { env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~" }
    let backup = choose_backup(dest, actual_mode, suffix)
    fs.rename(dest, backup, overwrite: true)?
    return true
  }

  if opts.remove_destination or (opts.force and (opts.hardlink or opts.symlink)) {
    let dest_meta = fs.stat(dest, follow_symlinks: false)?
    if dest_meta.kind == "dir" {
      error.fail(f"cannot remove {gnu.quote(dest.display())}: Is a directory")?
    }
    fs.remove(dest)?
  }

  let _ = follow_source
  let _ = source
  true
}

proc preserve_attributes(source: Path, dest: Path, meta: CpStat, opts: CpOptions, follow: Bool) [fs, error] -> Result[Unit] {
  if has_attribute(opts, "ownership") {
    fs.set_owner(dest, uid: meta.uid, gid: meta.gid, follow_symlinks: follow)?
  }

  if has_attribute(opts, "mode") and meta.kind != "symlink" {
    fs.chmod(dest, meta.mode.bit_and(0o7777), follow_symlinks: follow)?
  }

  if has_attribute(opts, "timestamps") {
    let current = fs.stat(source, follow_symlinks: follow)?
    fs.set_times(dest, atime_ns: current.atime_ns, mtime_ns: meta.mtime_ns, follow_symlinks: follow)?
  }

  let _ = source
  Ok()
}

proc verbose_action(source: Path, dest: Path, mode: Str) [process, env, io] {
  let separator = if mode == "hardlink" { " => " } else { " -> " }
  gnu.write_text(f"{gnu.quote(source.display())}{separator}{gnu.quote(dest.display())}\n")
}

proc copy_node(source: Path, dest: Path, opts: CpOptions, follow: Bool, dereference_all: Bool, root: Bool, root_device: Int?, ancestors: List[Str]) [fs, process, env, io, error] -> Result[Unit] {
  let link_meta = fs.stat(source, follow_symlinks: false)?
  let meta = if follow { fs.stat(source, follow_symlinks: true)? } else { link_meta }
  let identity = f"{meta.dev}:{meta.ino}"

  if path_exists(dest) and same_file(source, meta, dest, follow) {
    let dest_link_meta = fs.stat(dest, follow_symlinks: false)?
    let source_is_link = link_meta.kind == "symlink"
    let dest_is_link = dest_link_meta.kind == "symlink"
    let backup_active = opts.backup != "" or opts.backup_short or opts.suffix != ""
    let backup_mode = if opts.backup != "" { opts.backup } else { env.get_or("VERSION_CONTROL", "existing") ?? "existing" }
    let backup_enabled = backup_active and backup_mode != "none" and backup_mode != "off"

    if opts.hardlink { return Ok() }
    if opts.symlink and dest_is_link { return Ok() }
    if source_is_link and dest_is_link and ! follow and source.display() != dest.display() { return Ok() }
    if opts.remove_destination and dest_is_link { } else if backup_enabled and (source_is_link or dest_is_link) { } else if backup_enabled and ! source_is_link and ! dest_is_link and (opts.force or source.display() != dest.display()) {
      if source.display() == dest.display() {
        let suffix = if opts.suffix != "" { opts.suffix } else { env.get_or("SIMPLE_BACKUP_SUFFIX", "~") ?? "~" }
        let backup = choose_backup(dest, backup_mode, suffix)
        let _ = fs.copy_file(source, backup, sparse: opts.sparse, reflink: opts.reflink, overwrite: true, mode: meta.mode.bit_and(0o7777))?
        return Ok()
      }
    } else {
      return error.fail(f"{gnu.quote(source.display())} and {gnu.quote(dest.display())} are the same file")
    }
  }

  if meta.kind == "dir" {
    if ! opts.recursive and ! opts.archive {
      return error.fail(f"-r not specified; omitting directory {gnu.quote(source.display())}")
    }

    if identity in ancestors {
      return error.fail(f"cannot copy cyclic symbolic link {gnu.quote(source.display())}")
    }
    if dest.starts_with(source) {
      return error.fail(f"cannot copy a directory, {gnu.quote(source.display())}, into itself {gnu.quote(dest.display())}")
    }

    let destination_existed = path_exists(dest)
    if destination_existed {
      let dest_meta = fs.stat(dest, follow_symlinks: false)?
      if dest_meta.kind != "dir" {
        return error.fail(f"cannot overwrite non-directory {gnu.quote(dest.display())} with directory {gnu.quote(source.display())}")
      }
    } else {
      dest.mkdir()?
      if has_attribute(opts, "mode") or has_attribute(opts, "ownership") {
        fs.chmod(dest, 0o700)?
      }
    }

    let children = fs.children(source, stat: true, ordered: true)?
    for child in children {
      let child_source = child.path
      let child_dest = fp"{dest}/{child.name}"
      let child_lstat = fs.stat(child_source, follow_symlinks: false)?
      let child_device = child_lstat.dev
      if opts.one_file_system and root_device != null and child_device != (root_device ?? -1) {
        continue
      }
      copy_node(child_source, child_dest, opts, dereference_all, dereference_all, false, root_device, ancestors + [identity])?
    }

    if has_attribute(opts, "ownership") {
      fs.set_owner(dest, uid: meta.uid, gid: meta.gid)?
    }
    if has_attribute(opts, "mode") {
      fs.chmod(dest, meta.mode.bit_and(0o7777))?
    } else if ! destination_existed {
      fs.chmod(dest, meta.mode.bit_and(0o777).clear_bits(fs.umask()?))?
    }
    if has_attribute(opts, "timestamps") {
      let current = fs.stat(source, follow_symlinks: follow)?
      fs.set_times(dest, atime_ns: current.atime_ns, mtime_ns: meta.mtime_ns)?
    }
    if opts.verbose and !(root and source.display().ends_with("/.")) { verbose_action(source, dest, "copy") }
    return Ok()
  }

  if opts.hardlink {
    let proceed = prepare_leaf(meta, source, dest, opts, follow)?
    if ! proceed { return Ok() }
    fs.link(source, dest, follow_symlinks: true)?
    if opts.verbose { verbose_action(source, dest, "hardlink") }
    return Ok()
  }

  if opts.symlink {
    let proceed = prepare_leaf(meta, source, dest, opts, follow)?
    if ! proceed { return Ok() }
    if path_exists(dest) { fs.remove(dest)? }
    fs.symlink(source, dest)?
    if opts.verbose { verbose_action(source, dest, "symlink") }
    return Ok()
  }

  if meta.kind == "symlink" and ! follow {
    let proceed = prepare_leaf(meta, source, dest, opts, false)?
    if ! proceed { return Ok() }
    if path_exists(dest) { fs.remove(dest)? }
    fs.symlink(source.readlink()?, dest)?
    if has_attribute(opts, "ownership") {
      fs.set_owner(dest, uid: meta.uid, gid: meta.gid, follow_symlinks: false)?
    }
    if has_attribute(opts, "timestamps") {
      fs.set_times(dest, atime_ns: meta.atime_ns, mtime_ns: meta.mtime_ns, follow_symlinks: false)?
    }
    if opts.verbose { verbose_action(source, dest, "copy") }
    return Ok()
  }

  let proceed = prepare_leaf(meta, source, dest, opts, follow)?
  if ! proceed { return Ok() }

  if meta.kind == "file" {
    let creation_mode = if has_attribute(opts, "mode") {
      meta.mode.bit_and(0o7777)
    } else {
      meta.mode.bit_and(0o777).clear_bits(fs.umask()?)
    }
    if opts.attributes_only {
      if ! path_exists(dest) {
        dest.write(b"")?
        fs.chmod(dest, creation_mode)?
      }
    } else {
      let copied = fs.copy_file(
        source,
        dest,
        sparse: opts.sparse,
        reflink: opts.reflink,
        overwrite: true,
        mode: creation_mode,
      )
      match copied {
        Ok(_) => {},
        Err(failure) => {
          if opts.force and path_exists(dest) {
            fs.remove(dest)?
            let _ = fs.copy_file(source, dest, sparse: opts.sparse, reflink: opts.reflink, overwrite: false, mode: creation_mode)?
          } else {
            return Err(failure)
          }
        },
      }
    }
  } else {
    if opts.attributes_only {
      if ! path_exists(dest) {
        let creation_mode = if has_attribute(opts, "mode") {
          meta.mode.bit_and(0o7777)
        } else {
          meta.mode.bit_and(0o777).clear_bits(fs.umask()?)
        }
        fs.mknod(dest, meta.kind, creation_mode, major: fs.dev_major(meta.rdev), minor: fs.dev_minor(meta.rdev))?
      }
    } else if opts.copy_contents or ! opts.recursive {
      dest.write(source.read_bytes()?)?
    } else if ! path_exists(dest) or opts.remove_destination {
      if path_exists(dest) { fs.remove(dest)? }
      let creation_mode = if has_attribute(opts, "mode") {
        meta.mode.bit_and(0o7777)
      } else {
        meta.mode.bit_and(0o777).clear_bits(fs.umask()?)
      }
      fs.mknod(dest, meta.kind, creation_mode, major: fs.dev_major(meta.rdev), minor: fs.dev_minor(meta.rdev))?
    }
  }

  preserve_attributes(source, dest, meta, opts, follow)?
  if opts.verbose { verbose_action(source, dest, "copy") }
  let _ = root
  Ok()
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: CpOptions = cli.applet(
    argv,
    {
      gnu: {
        status: 1,
        permute: true,
        unsupported: {
          "--context": "SELinux context setting is not available",
          "-Z": "SELinux context setting is not available",
          "--debug": "copy diagnostics are not available",
          "--progress": "progress display is not available",
          "-g": "progress display is not available",
        },
      },
      archive: {form: "-a --archive", default: false},
      attributes_only: {form: "--attributes-only", default: false},
      backup: {form: "--backup[=CONTROL]", default: "", optional_default: "existing"},
      backup_short: {form: "-b", default: false},
      dereference: {form: "-L --dereference", default: false},
      force: {form: "-f --force", default: false},
      hardlink: {form: "-l --link", default: false},
      interactive: {form: "-i --interactive", default: false},
      no_clobber: {form: "-n --no-clobber", default: false},
      no_dereference: {form: "-P --no-dereference", default: false},
      no_deref_links: {form: "-d", default: false},
      no_preserve: {form: "--no-preserve ATTR_LIST", default: ""},
      no_target_directory: {form: "-T --no-target-directory", default: false},
      one_file_system: {form: "-x --one-file-system", default: false},
      parents: {form: "--parents --parent", default: false},
      preserve: {form: "-p --preserve[=ATTR_LIST]", default: "", optional_default: "default"},
      recursive: {form: "-R -r --recursive", default: false},
      reflink: {form: "--reflink[=WHEN]", default: "auto", optional_default: "always"},
      remove_destination: {form: "--remove-destination", default: false},
      sparse: {form: "--sparse WHEN", default: "auto"},
      strip_trailing_slashes: {form: "--strip-trailing-slashes", default: false},
      suffix: {form: "-S --suffix SUFFIX", default: ""},
      symlink: {form: "-s --symbolic-link", default: false},
      target: {form: "-t --target-directory DIR", default: "", conflicts: ["no_target_directory"]},
      cli_dereference: {form: "-H", default: false, conflicts: ["archive", "dereference", "no_dereference", "no_deref_links"]},
      copy_contents: {form: "--copy-contents", default: false},
      update: {form: "-u --update[=WHEN]", default: "all", optional_default: "older"},
      verbose: {form: "-v --verbose", default: false},
      operands: {form: "...SOURCE"},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
    },
  )?

  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("cp"); return }

  if opts.backup_short and opts.backup == "" {
    let version_control = env.get_or("VERSION_CONTROL", "existing") ?? "existing"
    let _ = version_control
  }

  if opts.operands.len() < 2 and opts.target == "" { gnu.usage_error("missing file operand") }
  if opts.operands.len() == 0 { gnu.usage_error("missing file operand") }
  let backup_active = opts.backup != "" or opts.backup_short or opts.suffix != ""
  let backup_mode = if opts.backup != "" { opts.backup } else if backup_active { env.get_or("VERSION_CONTROL", "existing") ?? "existing" } else { "none" }
  let backup_enabled = backup_active and backup_mode != "none" and backup_mode != "off"
  if opts.no_clobber and backup_enabled { gnu.usage_error("--backup and --no-clobber are mutually exclusive") }
  if opts.update != "all" and opts.update != "older" and opts.update != "none" and opts.update != "none-fail" {
    gnu.error(f"invalid argument {gnu.quote_value(opts.update)} for '--update'")
    exit 1
  }
  if opts.sparse != "auto" and opts.sparse != "always" and opts.sparse != "never" {
    gnu.error(f"invalid argument {gnu.quote_value(opts.sparse)} for '--sparse'")
    exit 1
  }
  if opts.reflink != "auto" and opts.reflink != "always" and opts.reflink != "never" {
    gnu.error(f"invalid argument {gnu.quote_value(opts.reflink)} for '--reflink'")
    exit 1
  }

  let backup_values = ["numbered", "t", "existing", "nil", "simple", "never", "none", "off"]
  if backup_mode != "none" and backup_mode != "off" and ! (backup_mode in backup_values) {
    gnu.error(f"invalid argument {gnu.quote_value(backup_mode)} for '--backup'")
    exit 1
  }
  if opts.update == "none" or opts.update == "none-fail" {
    if backup_enabled { gnu.error("--backup is not allowed with --update=none") ; exit 1 }
  }

  let attributes = if opts.preserve == "" { "" } else { opts.preserve.lower() }
  var invalid_attribute = ""
  for attribute in attributes.split(",") {
    if attribute != "" and ! (attribute in ["mode", "ownership", "timestamps", "links", "xattr", "context", "all", "default"]) {
      invalid_attribute = attribute
      break
    }
  }
  if invalid_attribute != "" {
    gnu.error(f"invalid argument {gnu.quote_value(invalid_attribute)} for '--preserve'")
    exit 1
  }
  if "xattr" in attributes.split(",") or "context" in attributes.split(",") {
    gnu.error("preserving xattr or context is not supported")
    exit 1
  }
  let no_preserve_attrs = opts.no_preserve.lower().split(",")
  for attribute in no_preserve_attrs {
    if attribute != "" and ! (attribute in ["mode", "ownership", "timestamps", "links", "xattr", "context", "all"]) {
      gnu.error(f"invalid argument {gnu.quote_value(attribute)} for '--no-preserve'")
      exit 1
    }
    if attribute == "xattr" or attribute == "context" {
      gnu.error("preserving xattr or context is not supported")
      exit 1
    }
  }

  let paths = opts.operands
  let target_from_option = opts.target != ""
  let dest = if target_from_option { fp"{opts.target}" } else { fp"{paths[paths.len() - 1]}" }
  let sources = if target_from_option { paths } else { paths |> take(paths.len() - 1) }
  var target_is_dir = false

  if target_from_option {
    if ! path_exists(dest) {
      gnu.error(f"target directory {gnu.quote(dest.display())} does not exist")
      exit 1
    }
    target_is_dir = fs.stat(dest, follow_symlinks: true)?.kind == "dir"
    if ! target_is_dir {
      gnu.error(f"target directory {gnu.quote(dest.display())} is not a directory")
      exit 1
    }
  } else if ! opts.no_target_directory and path_exists(dest) {
    target_is_dir = fs.stat(dest, follow_symlinks: false)?.kind == "dir"
  }

  if opts.parents and ! target_is_dir {
    gnu.error("with --parents, the destination must be a directory")
    exit 1
  }
  if sources.len() > 1 and ! target_is_dir and ! opts.parents {
    gnu.error(f"target {gnu.quote(dest.display())} is not a directory")
    exit 1
  }

  var failed = false
  for source_text in sources {
    let source_name = if opts.strip_trailing_slashes { slashless(source_text) } else { source_text }
    let source = fp"{source_name}"
    let target = if opts.parents {
      parent_dest(source_name, dest)
    } else {
      dest_for(source, dest, target_is_dir)
    }
    let mode = dereference_mode(argv)
    let follow_all = mode == "follow"
    let follow_root = follow_all or mode == "command-line" or opts.hardlink or (mode == "default" and ! opts.recursive)
    let root_device: Int? = if let Ok(stat) = fs.stat(source, follow_symlinks: follow_root) { stat.dev } else { null }
    match copy_node(source, target, opts, follow_root, follow_all, true, root_device, []) {
      Ok(_) => {},
      Err(failure) => {
        gnu.error(failure.message)
        failed = true
      },
    }
  }

  if failed { exit 1 }
}
