#!/bin/xsh
use lib.gnu

error CopyError = Invalid(message: Str)

type Options = {
  recursive: Bool, dereference: Str, overwrite: Str, force: Bool,
  update: Str, target: Str?, no_target: Bool, hardlink: Bool, symlink: Bool,
  mode: Bool, no_mode: Bool, xattr: Str, context: Str, owner: Bool, times: Bool, links: Bool, parents: Bool,
  remove: Bool, backup: Str, suffix: Str, sparse: Str, reflink: Str,
  verbose: Bool, attributes: Bool, one_fs: Bool, strip: Bool, copy_contents: Bool, debug: Bool,
}
type FileIdentity = {dev: Int, ino: Int, path: Path, kind: Str}
type Outcome = {copies: List[FileIdentity], failed: Bool}

pure basename_path(target: Path) -> Result[Path] {
  var raw = target.bytes()
  while raw.len() > 1 and raw.byte_at(raw.len() - 1) == 47 { raw = raw[..raw.len() - 1] }
  var at = raw.len()
  while at > 0 and raw.byte_at(at - 1) != 47 { at -= 1 }
  Path.parse_bytes(raw[at..])
}

pure child_path(parent: Path, child: Path) -> Path {
  let raw = parent.bytes()
  if raw.byte_at(raw.len() - 1) == 47 { fp"{parent}{child}" } else { fp"{parent}/{child}" }
}

pure mode_permissions(mode: Int) -> Str {
  let user_execute = if mode.bit_and(0o4000) != 0 { if mode.bit_and(0o100) != 0 { "s" } else { "S" } } else if mode.bit_and(0o100) != 0 { "x" } else { "-" }
  let group_execute = if mode.bit_and(0o2000) != 0 { if mode.bit_and(0o10) != 0 { "s" } else { "S" } } else if mode.bit_and(0o10) != 0 { "x" } else { "-" }
  let other_execute = if mode.bit_and(0o1000) != 0 { if mode.bit_and(0o1) != 0 { "t" } else { "T" } } else if mode.bit_and(0o1) != 0 { "x" } else { "-" }
  let user_permissions = f"{if mode.bit_and(0o400) != 0 { "r" } else { "-" }}{if mode.bit_and(0o200) != 0 { "w" } else { "-" }}{user_execute}"
  let group_permissions = f"{if mode.bit_and(0o40) != 0 { "r" } else { "-" }}{if mode.bit_and(0o20) != 0 { "w" } else { "-" }}{group_execute}"
  let other_permissions = f"{if mode.bit_and(0o4) != 0 { "r" } else { "-" }}{if mode.bit_and(0o2) != 0 { "w" } else { "-" }}{other_execute}"
  f"{user_permissions}{group_permissions}{other_permissions}"
}

pure mode_octal(mode: Int) -> Str {
  let bits = mode.bit_and(0o7777)
  f"{bits / 512}{bits / 64 % 8}{bits / 8 % 8}{bits % 8}"
}

proc invalid(message: Str) -> Result[Unit] {
  Err(CopyError.Invalid(message:))
}

proc source_stat(source: Path, follow: Bool) {
  match fs.stat(source, follow_symlinks: follow) {
    Ok(meta) => Ok(meta)
    Err(failure) => Err(CopyError.Invalid(message: f"cannot stat {gnu.quote_bytes(source.bytes())}: {gnu.strerror(failure)}"))
  }
}

# Archive preservation treats unavailable extended attributes and security
# labels as optional; an explicitly requested attribute reports its failure.
proc preserve_xattrs(source: Path, target: Path, opts: Options, follow: Bool, target_link: Bool) {
  if ! opts.mode and opts.xattr == "none" and opts.context == "none" { return }
  let listed = fs.xattr_list(source, follow_symlinks: follow)
  if let Err(failure) = listed {
    if opts.xattr == "required" or opts.context == "required" or (opts.mode and failure.errno != 95) {
      return Err(failure)
    }
    return
  }
  let source_names = listed?
  if opts.mode and ! target_link {
    for name in fs.xattr_list(target)? {
      if name in ["system.posix_acl_access", "system.posix_acl_default"] and name not in source_names {
        fs.xattr_remove(target, name)
      }
    }
  }
  var has_context = false
  for name in source_names {
    let acl = name == "system.posix_acl_access" or name == "system.posix_acl_default"
    let context = name == "security.selinux"
    if context { has_context = true }
    let selected = if acl { opts.mode } else if context { opts.context != "none" } else { opts.xattr != "none" }
    if selected {
      let applied: Result[Unit] = try {
        let value = fs.xattr_get(source, name, follow_symlinks: follow)?
        fs.xattr_set(target, name, value, follow_symlinks: ! target_link)
      }
      if let Err(failure) = applied {
        if acl or (context and opts.context == "required") or (! context and opts.xattr == "required") {
          return Err(failure)
        }
      }
    }
  }
  if opts.context == "required" and ! has_context {
    invalid(f"failed to get security context of {gnu.quote_bytes(source.bytes())}: No data available")?
  }
}

# A failure to set an attribute names the destination; validation errors pass through.
proc preserve_attributes(source: Path, target: Path, opts: Options, follow: Bool, target_link: Bool) {
  let applied = preserve_xattrs(source, target, opts, follow, target_link)
  if let Err(failure) = applied {
    match failure {
      CopyError.Invalid {..} => return Err(failure)
      _ => return Err(CopyError.Invalid(message: f"setting attributes for {gnu.quote_bytes(target.bytes())}: {gnu.strerror(failure)}"))
    }
  }
  applied
}

# Use metadata captured before copying reads a file or walks a directory.
proc preserve(source: Path, target: Path, opts: Options, follow: Bool, source_meta: FsStat, created = false) {
  let target_link = source_meta.kind == "symlink" or opts.symlink
  if opts.owner {
    fs.set_owner(target, uid: source_meta.uid, gid: source_meta.gid, follow_symlinks: ! target_link)
  }
  if ! target_link and (opts.xattr != "none" or opts.context != "none") {
    let target_mode = fs.stat(target)?.mode.bit_and(0o7777)
    if target_mode.bit_and(0o200) == 0 {
      target.chmod(target_mode.bit_or(0o200))
      let attributes = preserve_attributes(source, target, opts, follow, target_link)
      target.chmod(target_mode)
      attributes?
    } else {
      preserve_attributes(source, target, opts, follow, target_link)?
    }
  } else {
    preserve_attributes(source, target, opts, follow, target_link)?
  }
  if opts.mode and ! target_link {
    target.chmod(source_meta.mode.bit_and(0o7777))
  } else if opts.owner and created and ! target_link {
    let mask = if source_meta.kind == "dir" { 0o1777 } else { 0o777 }
    let defaults = if source_meta.kind == "dir" { 0o777 } else { 0o666 }
    target.chmod((if opts.no_mode { defaults } else { source_meta.mode.bit_and(mask) }).clear_bits(fs.umask()?))
  }
  if opts.times {
    fs.set_times(target, atime_ns: source_meta.atime_ns, mtime_ns: source_meta.mtime_ns,
      follow_symlinks: ! target_link)
  }
}

# Preserved ownership and modes require a private directory from its first
# instant of existence; opening its parent pins the kernel-relative creation.
proc mkdir_copy(target: Path, source_mode: Int, opts: Options) {
  let parent = fs.open_root(target.parent().resolve()?)?
  defer parent.close()
  # Without mode preservation the default mode is narrowed by the umask and the
  # parent's setgid bit is inherited, as mkdir(1) does. Callers must not chmod
  # such a directory afterwards, because that would drop the inherited bit.
  if opts.no_mode and ! opts.mode and ! opts.owner {
    let inherited = fs.stat(target.parent().resolve()?, follow_symlinks: true)?.mode.bit_and(0o2000)
    parent.mkdir(basename_path(target)?, mode: 0o777.clear_bits(fs.umask()?).bit_or(inherited))
    return
  }
  let mode = if opts.mode or opts.owner { 0o700 } else { source_mode.bit_and(0o1777).clear_bits(fs.umask()?).bit_or(0o700) }
  parent.mkdir(basename_path(target)?, mode: mode)
  target.chmod(mode)
}

type ParentDirectory = {source: Path, target: Path, meta: FsStat, created: Bool}

# Newly created ancestors stay searchable during the copy. Their source modes
# and timestamps are applied after the child operations finish.
proc prepare_parents(source: Path, target: Path, root: Path, opts: Options) -> Result[List[ParentDirectory]] {
  var pairs: List[ParentDirectory] = []
  var from = source.parent()
  var dest = target.parent()
  while dest != root and dest != dest.parent() {
    let meta = source_stat(from, true)?
    let exists = dest.exists()?
    if ! exists or fs.stat(dest, follow_symlinks: true)?.kind == "dir" {
      pairs = [{source: from, target: dest, meta: meta, created: ! exists}] + pairs
    }
    from = from.parent()
    dest = dest.parent()
  }
  for pair in pairs {
    if pair.created {
      mkdir_copy(pair.target, pair.meta.mode, opts)
      if opts.verbose { gnu.write_text(f"{gnu.quote_maybe(pair.source.display())} -> {gnu.quote_maybe(pair.target.display())}\n") }
    }
  }
  Ok(pairs)
}

proc finish_parents(pairs: List[ParentDirectory], opts: Options) {
  for index in range(pairs.len()) {
    let pair = pairs[pairs.len() - index - 1]
    if pair.created and ! opts.mode and ! opts.owner and ! opts.no_mode {
      pair.target.chmod((if opts.no_mode { 0o777 } else { pair.meta.mode.bit_and(0o1777) }).clear_bits(fs.umask()?))
    }
    preserve(pair.source, pair.target, opts, true, pair.meta, created: pair.created)?
  }
}

proc backup_path(target: Path, opts: Options) -> Path {
  var numbered = opts.backup == "numbered" or opts.backup == "t"
  var highest = 0
  let prefix = f"{target.basename()}.~"
  for entry in fs.children(target.parent())? {
    let name = entry.path.basename()
    if name.starts_with(prefix) and name.ends_with("~") {
      let num = name.byte_slice(prefix.byte_len(), name.byte_len() - prefix.byte_len() - 1).parse_int() ?? 0
      if num > highest { highest = num }
    }
  }
  if opts.backup == "existing" or opts.backup == "nil" {
    numbered = highest > 0
  }
  if numbered { fp"{target}.~{highest + 1}~" } else { fp"{target}{opts.suffix}" }
}

# Directory ancestry is checked by inode before traversal, including aliases
# and missing destination components, so copying into the source cannot recurse.
proc inside(source: Path, target: Path) -> Bool {
  let source_meta = fs.stat(source, follow_symlinks: true)?
  var parent = target
  while true {
    var next = parent.parent()
    if let Ok(meta) = fs.stat(parent, follow_symlinks: true) {
      return true when meta.dev == source_meta.dev and meta.ino == source_meta.ino
      let absolute = parent.resolve()?
      next = absolute.parent()
      break when next == absolute
    } else {
      break when next == parent
    }
    parent = next
  }
  false
}

proc copy_node(source: Path, target: Path, opts: Options, command_line: Bool,
  root_dev: Int, ancestors: List[FileIdentity], saved: List[FileIdentity]) -> Result[Outcome] {
  let follow = opts.dereference == "all" or (command_line and opts.dereference == "command") or
    (opts.dereference == "default" and (! opts.recursive or opts.hardlink))
  let meta = source_stat(source, follow)?
  var input = source
  let existence = target.exists()
  if let Err(failure) = existence {
    if failure.errno == 20 {
      invalid(f"cannot create regular file {gnu.quote_bytes(target.bytes())}: {gnu.strerror(failure)}")?
    }
  }
  let exists = existence?
  var created = ! exists
  let dest_kind = if exists { fs.stat(target)?.kind } else { "" }
  var copies = saved
  let parent = target.parent().resolve()
  if let Err(failure) = parent {
    invalid(f"cannot create regular file {gnu.quote_bytes(target.bytes())}: {gnu.strerror(failure)}")?
  }
  if fs.stat(parent?, follow_symlinks: true)?.kind != "dir" {
    invalid(f"cannot create regular file {gnu.quote_bytes(target.bytes())}: Not a directory")?
  }
  let target_key = fp"{parent?}/{basename_path(target)?}".normalize()
  if exists {
    for previous in saved {
      if previous.path == target_key {
        if previous.kind == "symlink" and meta.kind != "symlink" and ! opts.symlink and ! opts.hardlink and ! opts.remove and opts.backup == "none" {
          invalid(f"will not copy {gnu.quote_bytes(source.bytes())} through just-created symlink {gnu.quote_bytes(target.bytes())}")?
        }
        if command_line and previous.dev == meta.dev and previous.ino == meta.ino and opts.backup == "none" {
          let kind = if meta.kind == "dir" { "directory" } else { "file" }
          gnu.error(f"warning: source {kind} {gnu.quote_bytes(source.bytes())} specified more than once")
          return Ok({copies: copies, failed: false})
        }
        if command_line and meta.kind != "dir" and opts.backup not in ["numbered", "t"] and opts.overwrite != "never" and opts.update != "none" {
          invalid(f"will not overwrite just-created {gnu.quote_bytes(target.bytes())} with {gnu.quote_bytes(source.bytes())}")?
        }
      }
    }
  }
  if meta.kind == "dir" {
    if ! opts.recursive { invalid(f"-r not specified; omitting directory {gnu.quote_bytes(source.bytes())}")? }
    for ancestor in ancestors {
      if ancestor.dev == meta.dev and ancestor.ino == meta.ino {
        invalid(f"cannot copy cyclic symbolic link {gnu.quote_bytes(source.bytes())}")?
      }
    }
    if command_line and inside(source, target) {
      invalid(f"cannot copy a directory, {gnu.quote_bytes(source.bytes())}, into itself, {gnu.quote_bytes(target.bytes())}")?
    }
    if exists and dest_kind != "dir" {
      invalid(f"cannot overwrite non-directory {gnu.quote_bytes(target.bytes())} with directory {gnu.quote_bytes(source.bytes())}")?
    }
    if opts.parents and target.basename() == ".." {
      invalid(f"cannot create directory {gnu.quote_bytes(target.bytes())}: File exists")?
    }
    if ! exists { mkdir_copy(target, meta.mode, opts) }
    if opts.verbose and ! exists {
      let target_bytes = target.bytes()
      let verbose_target = if opts.parents or target_bytes.byte_at(target_bytes.len() - 1) == 47 {
        target_bytes
      } else {
        bytes.concat([target_bytes, b"/"])
      }
      gnu.write_text(f"{gnu.quote_bytes(source.bytes())} -> {gnu.quote_bytes(verbose_target)}\n")
    }
    var failed = false
    if ! opts.one_fs or command_line or meta.dev == root_dev {
      let next = ancestors + [{dev: meta.dev, ino: meta.ino, path: source, kind: meta.kind}]
      for entry in fs.children(source)? {
        let dest = child_path(target, basename_path(entry.path)?)
        let child_source = child_path(source, basename_path(entry.path)?)
        match copy_node(child_source, dest, opts, false, root_dev, next, copies) {
          Ok(result) => { copies = result.copies; failed = failed or result.failed }
          Err(failure) => { report(child_source, dest, failure); failed = true }
        }
      }
    }
    if ! exists and ! opts.mode and ! opts.owner and ! opts.no_mode { target.chmod((if opts.no_mode { 0o777 } else { meta.mode.bit_and(0o1777) }).clear_bits(fs.umask()?)) }
    preserve(source, target, opts, follow, meta, created: created)?
    copies += [{dev: meta.dev, ino: meta.ino, path: target_key, kind: meta.kind}]
    return Ok({copies: copies, failed: failed})
  }
  if exists {
    if dest_kind == "dir" { invalid(f"cannot overwrite directory {gnu.quote_bytes(target.bytes())} with non-directory")? }
    if opts.overwrite == "never" or opts.update == "none" { if opts.debug { gnu.write_text(f"skipped {gnu.quote_bytes(target.bytes())}\n") }; return Ok({copies: copies, failed: false}) }
    if opts.update == "none-fail" { invalid(f"not replacing {gnu.quote_bytes(target.bytes())}")? }
    if opts.update == "older" {
      if let Ok(dest) = fs.stat(target, follow_symlinks: true) {
        return Ok({copies: copies, failed: false}) when dest.mtime_ns >= meta.mtime_ns
      }
    }
    var remove_readonly = false
    if opts.overwrite == "ask" {
      let destination_mode = fs.stat(target)?.mode
      remove_readonly = opts.force and opts.backup == "none" and destination_mode.bit_and(0o222) == 0
      if remove_readonly {
        io.write_stderr(f"cp: replace {gnu.quote_bytes(target.bytes())}, overriding mode {mode_octal(destination_mode)} ({mode_permissions(destination_mode)})? ")?
      } else {
        io.write_stderr(f"cp: overwrite {gnu.quote_bytes(target.bytes())}? ")?
      }
      io.flush_stderr()?
      let answer = io.stdin_line()?.lower()
      return Ok({copies: copies, failed: true}) when ! answer.starts_with("y")
    }
    let source_meta = fs.stat(source)?
    let source_key = fp"{source.parent().resolve()?}/{basename_path(source)?}".normalize()
    let same_name = source_key == target_key
    var same_data = false
    if let Ok(from) = fs.stat(source, follow_symlinks: true) {
      if let Ok(to) = fs.stat(target, follow_symlinks: true) {
        same_data = from.dev == to.dev and from.ino == to.ino
      }
    }
    let dest_meta = fs.stat(target)?
    let same_link = source_meta.dev == dest_meta.dev and source_meta.ino == dest_meta.ino
    if opts.hardlink and same_data and dest_kind != "symlink" and ! (same_name and opts.force and opts.backup != "none") {
      return Ok({copies: copies, failed: false})
    }
    if ! follow and meta.kind == "symlink" and same_link and ! same_name {
      return Ok({copies: copies, failed: false}) when opts.backup == "none" or opts.hardlink
    }
    var unsafe_same = same_name or (same_data and (follow or source_meta.kind == "file")) or
      (same_data and source_meta.kind == "symlink" and dest_kind != "symlink")
    if ! same_name and (source_meta.kind == "file" or (! follow and meta.kind == "symlink")) and (opts.backup != "none" or opts.remove) { unsafe_same = false }
    if ! same_name and dest_kind == "symlink" and
      (opts.backup != "none" or opts.remove or (opts.force and (opts.hardlink or opts.symlink))) {
      unsafe_same = false
    }
    if same_name and opts.force and opts.backup != "none" and source_meta.kind == "file" { unsafe_same = false }
    if opts.hardlink and dest_kind == "symlink" and ! same_name { unsafe_same = false }
    if opts.symlink and dest_kind == "symlink" and ! same_name { unsafe_same = false }
    if unsafe_same {
      invalid(f"{gnu.quote_bytes(source.bytes())} and {gnu.quote_bytes(target.bytes())} are the same file")?
    }
    if (opts.hardlink or opts.symlink) and ! opts.force and ! opts.remove and opts.backup == "none" {
      let kind = if opts.hardlink { "hard link" } else { "symlink" }
      invalid(f"cannot create {kind} {gnu.quote_bytes(target.bytes())} to {gnu.quote_bytes(source.bytes())}: File exists")?
    }
    if opts.attributes and meta.kind == "symlink" and ! opts.remove and opts.backup == "none" {
      invalid(f"cannot create symbolic link {gnu.quote_bytes(target.bytes())}: File exists")?
    }
    if opts.backup != "none" {
      let backup = backup_path(target, opts)
      let backup_key = fp"{backup.parent().resolve()?}/{basename_path(backup)?}".normalize()
      let source_key = fp"{source.parent().resolve()?}/{basename_path(source)?}".normalize()
      if backup_key == source_key {
        invalid(f"backing up {gnu.quote_bytes(target.bytes())} might destroy source;  {gnu.quote_bytes(source.bytes())} not copied")?
      }
      target.rename(to: backup, overwrite: true)
      created = true
      if source_key == target_key { input = backup }
    } else if remove_readonly or opts.remove or opts.symlink or (! follow and meta.kind == "symlink") or opts.hardlink {
      target.remove()
      created = true
      if opts.verbose { gnu.write_text(f"removed {gnu.quote_bytes(target.bytes())}\n") }
    } else if dest_kind == "symlink" and ! (e"POSIXLY_CORRECT" is Ok(_)) {
      if let Err(failure) = fs.stat(target, follow_symlinks: true) {
        if failure.errno == 2 {
          invalid(f"not writing through dangling symlink {gnu.quote_bytes(target.bytes())}")?
        }
      }
    }
  }
  var linked = false
  if opts.links and ! opts.symlink and ! opts.hardlink {
    for previous in copies {
      if previous.dev == meta.dev and previous.ino == meta.ino {
        if target.exists()? { target.remove() }
        fs.link(previous.path, target)
        linked = true
        break
      }
    }
  }
  if ! linked {
    if opts.symlink {
      if ! source.display().starts_with("/") and target.parent().resolve()? != p".".resolve()? {
        invalid(f"{gnu.quote_bytes(target.bytes())}: can make relative symbolic copies only in current directory")?
      }
      target.symlink(to: source)
    } else if opts.hardlink {
      fs.link(input, target, follow_symlinks: follow)
    } else if meta.kind == "symlink" {
      target.symlink(to: input.readlink()?)
    } else if meta.kind != "file" and opts.recursive and ! opts.copy_contents {
      if let Err(failure) = fs.mknod(target, meta.kind, meta.mode.bit_and(0o777),
        major: fs.dev_major(meta.rdev), minor: fs.dev_minor(meta.rdev)) {
        invalid(f"cannot create special file {gnu.quote_bytes(target.bytes())}: {gnu.strerror(failure)}")?
      }
    } else if opts.attributes {
      if ! target.exists()? {
        let made = fs.mknod(target, "file", if opts.mode or opts.owner { 0o600 } else if opts.no_mode { 0o666 } else { meta.mode.bit_and(0o777) })
        if let Err(failure) = made {
          created = false
          if failure.errno != 17 { return Err(failure) }
          if opts.overwrite == "never" or opts.update == "none" { return Ok({copies: copies, failed: false}) }
          if opts.update == "none-fail" { invalid(f"not replacing {gnu.quote_bytes(target.bytes())}")? }
        }
      }
    } else {
      let copied = fs.copy_file(input, target, sparse: opts.sparse, reflink: opts.reflink, overwrite: opts.overwrite != "never" and opts.update not in ["none", "none-fail"], mode: if opts.mode or opts.owner { 0o600 } else if opts.no_mode { 0o666 } else { meta.mode.bit_and(0o777) }, force: opts.force)
      if let Err(failure) = copied {
        if failure.errno == 17 and (opts.overwrite == "never" or opts.update == "none") {
          return Ok({copies: copies, failed: false})
        }
        if failure.errno == 17 and opts.update == "none-fail" {
          invalid(f"not replacing {gnu.quote_bytes(target.bytes())}")?
        }
        return Err(failure)
      }
      let result = copied?
      created = created or result.destination_replaced
      if opts.debug {
        # Virtual files can report a size that differs from the bytes read; use
        # that together with allocated blocks to distinguish them from holes.
        let source_has_holes = meta.blocks_512 < meta.size / 512
        let virtual_source = meta.kind == "file" and meta.blocks_512 == 0 and meta.size != result.bytes
        let source_has_data = if meta.kind != "file" or meta.size == 0 {
          result.bytes > 0
        } else {
          meta.blocks_512 > 0 or virtual_source
        }
        let offload = if opts.reflink == "always" {
          "unknown"
        } else if opts.reflink == "auto" and opts.sparse == "auto" {
          if source_has_data and (meta.size == 0 or (source_has_holes and meta.blocks_512 == 0)) {
            "unsupported"
          } else if (source_has_data and meta.size > 0) or (meta.size > 0 and meta.size < 512) {
            "yes"
          } else {
            "unknown"
          }
        } else if meta.kind != "file" or source_has_data or meta.size < 512 {
          "avoided"
        } else {
          "unknown"
        }
        let reflink = if opts.reflink == "always" { "yes" } else if opts.reflink == "never" or opts.sparse == "never" { "no" } else { "unsupported" }
        let sparse = if opts.sparse == "always" {
          if source_has_holes and source_has_data { "SEEK_HOLE + zeros" } else if source_has_holes { "SEEK_HOLE" } else { "zeros" }
        } else if opts.reflink == "always" {
          "no"
        } else if source_has_holes {
          "SEEK_HOLE"
        } else {
          "no"
        }
        gnu.write_text(f"copy offload: {offload}, reflink: {reflink}, sparse detection: {sparse}\n")
      }
    }
    preserve(input, target, opts, follow, meta, created: created)?
  }
  copies += [{dev: meta.dev, ino: meta.ino, path: target_key, kind: fs.stat(target)?.kind}]
  if opts.verbose { gnu.write_text(f"{gnu.quote_bytes(source.bytes())} -> {gnu.quote_bytes(target.bytes())}\n") }
  Ok({copies: copies, failed: false})
}

proc report(source: Path, target: Path, failure: Error) {
  match failure {
    CopyError.Invalid {message} => gnu.error(message)
    _ => gnu.error(f"cannot copy {gnu.quote_bytes(source.bytes())} to {gnu.quote_bytes(target.bytes())}: {gnu.strerror(failure)}")
  }
}

# Operands stay bytes so undecodable names reach the kernel unchanged.
pure strip_end(raw: Bytes) -> Bytes {
  var end = raw.len()
  while end > 1 and raw.byte_at(end - 1) == 47 { end -= 1 }
  raw[..end]
}

pure strip_start(raw: Bytes) -> Bytes {
  var at = 0
  while at < raw.len() and raw.byte_at(at) == 47 { at += 1 }
  raw[at..]
}

proc option_value(value: Str, choices: List[Str], option: Str) -> Str {
  return value when value in choices
  var matches: List[Str] = []
  for choice in choices {
    if value != "" and choice.starts_with(value) { matches += [choice] }
  }
  return matches[0] when matches.len() == 1
  let kind = if matches.is_empty() { "invalid" } else { "ambiguous" }
  gnu.usage_error(f"{kind} argument {gnu.quote(value)} for '{option}'")
  value
}

proc main(...raw_argv: List[Bytes]) {
  let prepared = gnu.prepare_arguments(raw_argv)
  let argv = prepared.text
  var opts: Options = {recursive: false, dereference: "default", overwrite: "always", force: false,
    update: "all", target: null, no_target: false, hardlink: false, symlink: false,
    mode: false, no_mode: false, xattr: "none", context: "none", owner: false, times: false, links: false, parents: false,
    remove: false, backup: "none", suffix: e"SIMPLE_BACKUP_SUFFIX" ?? "~", sparse: "auto", reflink: "auto",
    verbose: false, attributes: false, one_fs: false, strip: false, copy_contents: false, debug: false}
  var operands: List[Str] = []
  var at = 0
  var ended = false
  let names = ["archive", "recursive", "no-clobber", "force", "interactive", "update", "no-target-directory",
    "target-directory", "link", "symbolic-link", "no-dereference", "dereference", "preserve", "no-preserve",
    "parents", "remove-destination", "backup", "suffix", "sparse", "reflink", "verbose", "attributes-only",
    "one-file-system", "strip-trailing-slashes", "help", "version", "copy-contents", "context", "debug"]
  while at < argv.len() {
    let arg = argv[at]
    at += 1
    if ended or arg == "-" or ! arg.starts_with("-") {
      operands += [arg]
      if e"POSIXLY_CORRECT" is Ok(_) { ended = true }
      continue
    }
    if arg == "--" { ended = true; continue }
    var flags: List[Str] = []
    var value: Str? = null
    if arg.starts_with("--") {
      let parts = arg.byte_slice(2).split("=")
      let requested = parts[0]
      var matches: List[Str] = []
      for name in names { if name.starts_with(requested) { matches += [name] } }
      if requested in names { flags = [requested] } else if matches.len() == 1 { flags = matches } else if matches.len() > 1 { gnu.usage_error(f"option {gnu.quote(arg)} is ambiguous") } else { gnu.usage_error(f"unrecognized option {gnu.quote(arg)}") }
      if parts.len() > 1 {
        value = parts[1..].join("=")
        if flags[0] not in ["target-directory", "suffix", "sparse", "no-preserve", "preserve", "backup", "update", "reflink", "context"] {
          gnu.usage_error(f"option {gnu.quote(arg)} does not allow an argument")
        }
      }
    } else {
      var pos = 1
      while pos < arg.byte_len() {
        let flag = arg.byte_slice(pos, 1)
        flags += [flag]
        pos += 1
        if flag == "t" or flag == "S" {
          if pos < arg.byte_len() { value = arg.byte_slice(pos) }
          break
        }
      }
    }
    for flag in flags {
      if flag in ["t", "target-directory", "S", "suffix", "sparse", "no-preserve"] {
        if value == null {
          if at >= argv.len() { gnu.usage_error(f"option {gnu.quote(arg)} requires an argument") }
          value = argv[at]; at += 1
        }
      }
      let val = value ?? ""
      match flag {
        "a" | "archive" => { opts.recursive = true; opts.dereference = "none"; opts.mode = true; opts.no_mode = false; opts.xattr = "optional"; opts.context = "optional"; opts.owner = true; opts.times = true; opts.links = true }
        "R" | "r" | "recursive" => opts.recursive = true
        "d" => { opts.dereference = "none"; opts.links = true }
        "P" | "no-dereference" => opts.dereference = "none"
        "L" | "dereference" => opts.dereference = "all"
        "H" => opts.dereference = "command"
        "n" | "no-clobber" => opts.overwrite = "never"
        "i" | "interactive" => opts.overwrite = "ask"
        "f" | "force" => opts.force = true
        "u" => opts.update = "older"
        "update" => opts.update = option_value(value ?? "older", ["all", "older", "none", "none-fail"], "update")
        "T" | "no-target-directory" => opts.no_target = true
        "t" | "target-directory" => { if opts.target != null { gnu.usage_error("multiple target directories specified") }; opts.target = val }
        "l" | "link" => { opts.hardlink = true; opts.symlink = false }
        "s" | "symbolic-link" => { opts.symlink = true; opts.hardlink = false }
        "p" => { opts.mode = true; opts.no_mode = false; opts.owner = true; opts.times = true }
        "preserve" | "no-preserve" => {
          let attrs = (value ?? "mode,ownership,timestamps").split(",")
          let enable = flag == "preserve"
          for attribute in attrs {
            var attr = attribute
            for name in ["mode", "ownership", "timestamps", "links", "context", "xattr", "all"] {
              if name.starts_with(attribute) and attribute != "" { attr = name; break }
            }
            match attr {
              "all" => { opts.mode = enable; opts.no_mode = ! enable; opts.xattr = if enable { "optional" } else { "none" }; opts.context = if enable { "optional" } else { "none" }; opts.owner = enable; opts.times = enable; opts.links = enable }
              "mode" => { opts.mode = enable; opts.no_mode = ! enable }
              "ownership" => opts.owner = enable
              "timestamps" => opts.times = enable
              "links" | "link" => opts.links = enable
              "xattr" => opts.xattr = if enable { "required" } else { "none" }
              "context" => opts.context = if enable { "required" } else { "none" }
              _ => gnu.usage_error(f"invalid argument {gnu.quote(attr)} for 'preserve'")
            }
          }
        }
        "parents" => opts.parents = true
        "remove-destination" => opts.remove = true
        "b" => opts.backup = e"VERSION_CONTROL" ?? "existing"
        "backup" => opts.backup = value ?? e"VERSION_CONTROL" ?? "existing"
        "S" | "suffix" => {
          opts.suffix = if val == "" { "~" } else { val }
          if opts.backup == "none" { opts.backup = e"VERSION_CONTROL" ?? "existing" }
        }
        "sparse" => opts.sparse = option_value(val, ["auto", "always", "never"], "sparse")
        "reflink" => opts.reflink = option_value(value ?? "always", ["auto", "always", "never"], "reflink")
        "v" | "verbose" => opts.verbose = true
        "attributes-only" => opts.attributes = true
        "x" | "one-file-system" => opts.one_fs = true
        "strip-trailing-slashes" => opts.strip = true
        "help" => { gnu.help("Usage: cp [OPTION]... SOURCE... DEST\nCopy SOURCE to DEST, or multiple SOURCE(s) to DIRECTORY."); return }
        "version" => { gnu.version("cp"); return }
        "copy-contents" => opts.copy_contents = true
        "debug" => { opts.debug = true; opts.verbose = true }
        "context" | "Z" => gnu.usage_error(f"option {gnu.quote(arg)} is not supported")
        _ => gnu.usage_error(f"invalid option -- {gnu.quote(flag)}")
      }
    }
  }
  if ! (opts.sparse in ["auto", "always", "never"]) { gnu.usage_error(f"invalid argument {gnu.quote(opts.sparse)} for 'sparse'") }
  if ! (opts.reflink in ["auto", "always", "never"]) { gnu.usage_error(f"invalid argument {gnu.quote(opts.reflink)} for 'reflink'") }
  opts.backup = option_value(opts.backup, ["none", "off", "simple", "never", "existing", "nil", "numbered", "t"], "backup")
  if opts.backup in ["none", "off"] { opts.backup = "none" } else if opts.backup in ["simple", "never"] { opts.backup = "simple" } else if ! (opts.backup in ["existing", "nil", "numbered", "t"]) { gnu.usage_error(f"invalid argument {gnu.quote(opts.backup)} for 'backup'") }
  if opts.reflink == "always" and opts.sparse != "auto" { gnu.usage_error("--reflink can be used only with --sparse=auto") }
  if opts.backup != "none" and opts.overwrite == "never" { gnu.usage_error("options --backup and --no-clobber are mutually exclusive") }
  if opts.backup != "none" and opts.update in ["none", "none-fail"] { gnu.usage_error("--backup is mutually exclusive with -n or --update=none-fail") }
  if opts.no_target and opts.target != null { gnu.usage_error("cannot combine --target-directory (-t) and --no-target-directory (-T)") }
  if operands.is_empty() { gnu.missing_operand() }
  if opts.target == null and operands.len() == 1 { gnu.usage_error(f"missing destination file operand after {gnu.quote_bytes(gnu.argument_bytes(operands[0], prepared.raw))}") }
  if opts.no_target and operands.len() > 2 { gnu.usage_error(f"extra operand {gnu.quote_bytes(gnu.argument_bytes(operands[2], prepared.raw))}") }
  let dest_arg = gnu.argument_bytes(opts.target ?? operands[-1], prepared.raw)
  if dest_arg == b"" { gnu.error("cannot create regular file '': No such file or directory"); exit 1 }
  let dest = Path.parse_bytes(dest_arg)?
  let sources = if opts.target == null { operands[..operands.len() - 1] } else { operands }
  let source_args: List[Bytes] = [gnu.argument_bytes(text, prepared.raw) for text in sources]
  var directory = false
  if ! opts.no_target {
    if let Ok(meta) = fs.stat(dest, follow_symlinks: true) { directory = meta.kind == "dir" }
  }
  if dest.display().ends_with("/") and fs.stat(dest, follow_symlinks: true) is Err(_) and
    (! opts.recursive or fs.stat(Path.parse_bytes(source_args[0])?, follow_symlinks: true)?.kind != "dir") {
    gnu.error(f"{gnu.quote_bytes(dest.bytes())} is not a directory")
    exit 1
  }
  if opts.parents and ! directory { gnu.usage_error("with --parents, the destination must be a directory") }
  if (sources.len() > 1 or opts.target != null) and ! directory { gnu.usage_error(f"target {gnu.quote_bytes(dest.bytes())} is not a directory") }
  var saved: List[FileIdentity] = []
  var failed = false
  for text in source_args {
    if text == b"" { gnu.error("cannot stat '': No such file or directory"); failed = true; continue }
    let source = Path.parse_bytes(if opts.strip { strip_end(text) } else { text })?
    var target = if directory { child_path(dest, basename_path(source)?) } else { dest }
    if opts.parents {
      target = Path.parse_bytes(bytes.concat([dest.bytes(), b"/", strip_start(text)]))?
    }
    var parent_pairs: List[ParentDirectory] = []
    if opts.parents {
      match prepare_parents(source, target, dest, opts) {
        Ok(pairs) => parent_pairs = pairs
        Err(failure) => { report(source, target, failure); failed = true; continue }
      }
    }
    match fs.stat(source, follow_symlinks: true) {
      Err(failure) => {
        if fs.stat(source) is Ok(_) and (opts.dereference == "none" or opts.recursive) {
          match copy_node(source, target, opts, true, 0, [], saved) {
            Ok(result) => { saved = result.copies; failed = failed or result.failed }
            Err(error) => { report(source, target, error); failed = true }
          }
        } else { gnu.error(f"cannot stat {gnu.quote_bytes(text)}: {gnu.strerror(failure)}"); failed = true }
      }
      Ok(meta) => {
        match copy_node(source, target, opts, true, meta.dev, [], saved) {
          Ok(result) => { saved = result.copies; failed = failed or result.failed }
          Err(failure) => { report(source, target, failure); failed = true }
        }
      }
    }
    if let Err(failure) = try { finish_parents(parent_pairs, opts) } {
      report(source, target, failure)
      failed = true
    }
  }
  if let Err(failure) = io.flush_stdout() { gnu.write_failed(failure) }
  exit if failed { 1 } else { 0 }
}
