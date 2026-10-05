#!/bin/xsh
use lib.gnu

type RmdirOptions = {parents: Bool, ignore_nonempty: Bool, verbose: Bool, help: Bool, version: Bool, targets: List[Str]}

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
    var current = fp"{name}"
    loop {
      if opts.verbose { gnu.write_text(f"rmdir: removing directory, {gnu.quote(current.display())}\n") }
      match current.remove_dir() {
        Ok(_) => {}
        Err(failure) => {
          if ! (opts.ignore_nonempty and gnu.errno(failure) in [17, 39, 66]) {
            gnu.error(f"failed to remove {gnu.quote(current.display())}: {removal_reason(current, failure)}")
            failed = true
          }
          break
        }
      }
      break when ! opts.parents
      let parent = current.parent()
      break when parent == "" or parent == current or (parent == "." and "/" not in current.display())
      current = parent
    }
  }
  if failed { exit 1 }
}
