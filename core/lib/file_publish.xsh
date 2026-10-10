##! File identity, backup naming, and publication helpers for file applets.
use gnu

## Observe the directory entry itself, including dangling symlinks.
export proc present(entry: Path) -> Result[Bool, Error] {
  match fs.stat(entry) {
    Ok(_) => true
    Err(failure) => {
      return false when gnu.errno(failure) == 2
      Err(failure)
    }
  }
}

## Compare inode identity without following the final symlink.
export proc same(source: Path, dest: Path) -> Result[Bool, Error] {
  let a = fs.stat(source)?
  return false when ! present(dest)?
  let b = fs.stat(dest)?
  a.dev == b.dev and a.ino == b.ino
}

# Long abbreviations have already been checked by cli.applet. Include their
# value-taking spellings so values that begin with '-' cannot become controls.
pure value_flags() -> List[Str] {
  var names = ["t", "S"]
  for name in ["target-directory", "suffix"] {
    for length in range(2, name.byte_len() + 1) { names += [name.byte_slice(0, length)] }
  }
  names
}

## Select the last overwrite option after expanding short option clusters.
export proc overwrite(argv: List[Str], fallback: Str, no_clobber = true) -> Result[Str, Error] {
  var selected = fallback
  for token in cli.tokens(argv, value_flags())? {
    if token.kind == "short" {
      match token.name {
        "f" => selected = "force"
        "i" => selected = "interactive"
        "n" => { if no_clobber { selected = "skip" } }
        else => {}
      }
    } else if token.kind == "long" {
      if "force".starts_with(token.name) { selected = "force" }
      if "interactive".starts_with(token.name) { selected = "interactive" }
      if no_clobber and "no-clobber".starts_with(token.name) { selected = "skip" }
    }
  }
  selected
}

## A symbolic link names its target without being it, so a regular source names
## the destination's own directory entry only when the two are the same entry. A
## source that is itself a symbolic link is never refused, and neither is a name
## that cannot be resolved.
export proc same_symlink_entry(source: Path, dest: Path) -> Result[Bool, Error] {
  match fs.stat(source, follow_symlinks: false) {
    Ok(meta) => {
      return false when meta.kind == "symlink"
    }
    Err(failure) => {
      return false when unresolved(failure)
      return Err(failure)
    }
  }
  same_entry(source, dest)
}

# ENOENT, ENOTDIR, ENAMETOOLONG, and ELOOP: the name does not lead to a file.
proc unresolved(failure: Error) -> Bool {
  gnu.errno(failure) in [2, 20, 36, 40]
}

## Choose a backup name without ever interpreting the suffix as a path.
export proc backup_name(dest: Path, control: Str, suffix: Str) -> Result[Path?, Error] {
  return null when control in ["none", "off"]
  if "/" in suffix {
    gnu.usage_error(f"invalid suffix {gnu.quote(suffix)}")
  }
  # A trailing slash only requires the destination to be a directory; appending a
  # suffix after it would name a path inside that directory.
  let stem = if dest.display().ends_with("/") { dest.normalize() } else { dest }
  var number = 1
  for entry in fs.children(dest.parent())? {
    let prefix = f"{dest.name()}.~"
    let name = entry.path.name()
    if name.starts_with(prefix) and name.ends_with("~") {
      let found = name.byte_slice(prefix.byte_len(), name.byte_len() - prefix.byte_len() - 1).parse_int() ?? 0
      if found >= number { number = found + 1 }
    }
  }
  return fp"{stem}.~{number}~" when control in ["numbered", "t"] or
    (control in ["existing", "nil"] and number > 1)
  return fp"{stem}{suffix}" when control in ["simple", "never", "existing", "nil"]
  gnu.usage_error(f"invalid argument {gnu.quote(control)} for 'backup type'")
  null
}

## Read exactly one answer so each operand can ask its own question.
export proc confirm(dest: Path, verb = "overwrite") -> Result[Bool, Error] {
  io.write_stderr(f"{gnu.prog()}: {verb} {gnu.quote(dest.display())}? ")?
  io.flush_stderr()?
  let answer = io.stdin_line()?
  answer.lower().starts_with("y")
}

## Stage a link in the destination directory so replacement is atomic and a
## failed source lookup leaves the old destination intact.
export proc publish_link(source: Path, dest: Path, symbolic: Bool, logical: Bool, replace: Bool) -> Result[Unit, Error] {
  if ! replace {
    if symbolic { dest.symlink(to: source) } else { fs.link(source, dest, follow_symlinks: logical) }
    return
  }
  let scratch = fs.tempfile()?
  defer scratch.root.close()
  let staged = fp"{dest.parent()}/.xsh-link-{scratch.root.host_path()?.name()}"
  if symbolic { staged.symlink(to: source) } else { fs.link(source, staged, follow_symlinks: logical) }
  defer staged.remove()
  staged.rename(to: dest, overwrite: true)
}

## Resolve existing ancestors while retaining a nonexistent final name.
export proc canonical(entry: Path) -> Result[Path, Error] {
  match entry.resolve() {
    Ok(found) => found
    Err(failure) => {
      if gnu.errno(failure) != 2 or entry.parent() == entry { return Err(failure) }
      match fs.stat(entry) {
        Ok(meta) => {
          if meta.kind == "symlink" {
            let target = entry.readlink()?
            let resolved = if target.display().starts_with("/") { target } else { fp"{entry.parent()}/{target}" }
            return canonical(resolved)
          }
        }
        Err(missing) => { if gnu.errno(missing) != 2 { return Err(missing) } }
      }
      fp"{canonical(entry.parent())?}/{entry.name()}".normalize()
    }
  }
}

## Select physical or logical hard linking in argv order.
export proc logical(argv: List[Str]) -> Result[Bool, Error] {
  var follow = false
  for token in cli.tokens(argv, value_flags())? {
    if token.kind != "operand" {
      if token.name == "L" or (token.kind == "long" and "logical".starts_with(token.name)) { follow = true }
      if token.name == "P" or (token.kind == "long" and "physical".starts_with(token.name)) { follow = false }
    }
  }
  follow
}

## Compare directory entries through resolved parents, keeping distinct hard
## links separate even when they share an inode.
export proc same_entry(source: Path, dest: Path) -> Result[Bool, Error] {
  fp"{canonical(source.parent())?}/{source.name()}".normalize() ==
    fp"{canonical(dest.parent())?}/{dest.name()}".normalize()
}

## Reject an invalid backup control before performing any mutation.
export proc validate_backup(control: Str, suffix: Str) -> Unit {
  if control not in ["none", "off", "numbered", "t", "existing", "nil", "simple", "never"] {
    gnu.usage_error(f"invalid argument {gnu.quote(control)} for 'backup type'")
  }
  if "/" in suffix { gnu.usage_error(f"invalid suffix {gnu.quote(suffix)}") }
}

## Keep short and long update options in their original argv order.
export proc update(argv: List[Str]) -> Result[Str, Error] {
  var selected = "all"
  for token in cli.tokens(argv, value_flags())? {
    if token.kind != "operand" {
      if token.name == "u" { selected = "older" }
      if token.kind == "long" and "update".starts_with(token.name) { selected = if token.value == "" { "older" } else { token.value } }
    }
  }
  selected
}

## Probe a target directory; only absence or a non-directory path means false.
export proc directory(entry: Path, follow: Bool) -> Result[Bool, Error] {
  match fs.stat(entry, follow_symlinks: follow) {
    Ok(meta) => meta.kind == "dir"
    Err(failure) => {
      if gnu.errno(failure) in [2, 20] { return false }
      Err(failure)
    }
  }
}

## Append the native basename while preserving dot paths' empty final name.
export pure destination(directory: Path, source: Path) -> Path {
  let components = source.components()
  let basename = if source.name() == "" { p"" } else { components[-1] }
  let directory_bytes = directory.bytes()
  if directory_bytes.len() > 0 and directory_bytes.byte_at(directory_bytes.len() - 1) == 47 {
    fp"{directory}{basename}"
  } else {
    fp"{directory}/{basename}"
  }
}
