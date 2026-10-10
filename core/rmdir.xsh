#!/bin/xsh
use lib.gnu

type RmdirOptions = {parents: Bool, ignore_nonempty: Bool, verbose: Bool, help: Bool, version: Bool, targets: List[Str]}

# The kernel can report permission or mount errors before checking emptiness.
# Ignore those only when directory enumeration proves a retained child exists.
proc ignored_nonempty(target: Path, failure: Error) [fs] -> Bool {
  let code = gnu.errno(failure)
  return true when code in [17, 39, 66]
  return false unless code in [1, 13, 16, 30]
  if let Ok(children) = fs.children(target, stat: false, ordered: false) {
    for child in children { return true }
  }
  false
}

# A trailing slash asks the kernel for a directory, so ENOTDIR can hide
# a symbolic link. Explain that the link was left untouched.
proc removal_reason(target: Path, failure: Error) [fs, error] -> Str {
  let name = target.display()
  if gnu.errno(failure) == 20 and name.ends_with("/") {
    var end = name.byte_len()
    while end > 0 and name.byte_slice(end - 1, length: 1) == "/" { end -= 1 }
    let trimmed = fp"{name.byte_slice(0, length: end)}"
    if let Ok(meta) = fs.stat(trimmed) {
      if meta.kind == "symlink" {
        match fs.stat(trimmed, follow_symlinks: true) {
          Ok(referent) => { return "Symbolic link not followed" when referent.kind == "dir" }
          Err(_) => return "Symbolic link not followed"
        }
      }
    }
  }
  gnu.strerror(failure)
}

# The next operand of the -p walk, computed on the operand text as GNU does
# rather than with Path.parent(): trailing slashes are dropped, then the last
# component and the slashes before it are cut. A name with no slash left ends
# the walk, so "dir/" removes only "dir" and never "." or "".
pure parent_operand(name: Bytes) -> Bytes? {
  var end = name.len()
  while end > 0 and name.byte_at(end - 1) == 47 { end -= 1 }
  var at = end
  while at > 0 and name.byte_at(at - 1) != 47 { at -= 1 }
  return null when at == 0
  var slash = at - 1
  while slash > 0 and name.byte_at(slash) == 47 { slash -= 1 }
  name.slice(0, length: slash + 1)
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: RmdirOptions = cli.applet(argv, {
    gnu: {status: 1},
    parents: {form: "-p --parents", default: false},
    ignore_nonempty: {form: "--ignore-fail-on-non-empty", default: false},
    verbose: {form: "-v --verbose", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    targets: {form: "...DIRECTORY"},
  })?
  if opts.help {
    gnu.help("Usage: rmdir [OPTION]... DIRECTORY...\nRemove empty directories.\n  -p, --parents  remove DIRECTORY and its ancestors\n  -v, --verbose  report each removal\n      --ignore-fail-on-non-empty  ignore nonempty directories\n")
    return
  }
  if opts.version { gnu.version("rmdir")
    return }
  if opts.targets.is_empty() { gnu.missing_operand() }
  var failed = false
  for name in opts.targets {
    var current = bytes.from_text(name)
    var ancestor = false
    loop {
      let directory = Path.parse_bytes(current)?
      if opts.verbose { gnu.write_text(f"rmdir: removing directory, {gnu.quote_bytes(current)}\n") }
      match directory.remove_dir() {
        Ok(_) => {}
        Err(failure) => {
          if ! (opts.ignore_nonempty and ignored_nonempty(directory, failure)) {
            if ! ancestor {
              gnu.error(f"failed to remove {gnu.quote(directory.display())}: {removal_reason(directory, failure)}")
            } else if gnu.errno(failure) == 20 {
              gnu.error(f"failed to remove {gnu.quote_bytes(current)}: {gnu.strerror(failure)}")
            } else {
              gnu.error(f"failed to remove directory {gnu.quote_bytes(current)}: {gnu.strerror(failure)}")
            }
            failed = true
          }
          break
        }
      }
      break when ! opts.parents
      guard let next = parent_operand(current) else { break }
      ancestor = true
      current = next
    }
  }
  if failed { exit 1 }
}
