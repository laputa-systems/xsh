#!/bin/xsh
use lib.gnu as gnu

const USAGE = """Usage: rmdir [OPTION]... DIRECTORY...
Remove the DIRECTORY(ies), if they are empty.

  -p, --parents              remove DIRECTORY and its ancestors
      --ignore-fail-on-non-empty  ignore failures that are solely because a directory is non-empty
  -v, --verbose              output a diagnostic for every directory processed
      --help                 display this help and exit
      --version              output version information and exit
"""

type RmdirOptions = {
  parents: Bool,
  ignore_nonempty: Bool,
  verbose: Bool,
  help: Bool,
  version: Bool,
  targets: List[Str],
}

pure trim_trailing_slashes(name: Str) -> Str {
  var trimmed = name
  while trimmed.byte_len() > 1 and trimmed.ends_with("/") {
    trimmed = trimmed.byte_slice(0, length: trimmed.byte_len() - 1)
  }
  trimmed
}

proc trailing_symlink(name: Str) [fs] -> Bool {
  return false when ! name.ends_with("/") or name == "/"
  let target = fp"{trim_trailing_slashes(name)}"
  match fs.stat(target, follow_symlinks: false) {
    Ok(metadata) => {
      if metadata.kind != "symlink" { return false }
      match fs.stat(target, follow_symlinks: true) {
        Ok(followed) => followed.kind == "dir",
        Err(_) => true,
      }
    },
    Err(_) => false,
  }
}

proc main(...argv: List[Str]) [fs, process, env, io, error] {
  let opts: RmdirOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      parents: {form: "-p --parents", default: false},
      ignore_nonempty: {
        form: "--ignore-fail-on-non-empty",
        default: false,
      },
      verbose: {form: "-v --verbose", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      targets: {form: "...DIRECTORY"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }
  if opts.version {
    gnu.version("rmdir")
    return
  }
  if opts.targets.len() == 0 {
    gnu.missing_operand()
  }

  var had_error = false
  for item in opts.targets {
    var current = fp"{item}"
    var first = true
    while first or (opts.parents and current != "" and current != "." and current != "/") {
      first = false
      let shown = current.display()
      if opts.verbose {
        gnu.write_text(f"rmdir: removing directory, {gnu.quote(shown)}\n")
      }
      if trailing_symlink(shown) {
        gnu.error(f"failed to remove {gnu.quote(shown)}: Symbolic link not followed")
        had_error = true
        break
      }
      match current.remove_dir() {
        Ok(_) => {
          if ! opts.parents { break }
          current = current.parent()
        }
        Err(failure) => {
          let errno = gnu.errno(failure)
          if opts.ignore_nonempty and errno in [13, 17, 39] {
            break
          }
          gnu.error(f"failed to remove {gnu.quote(shown)}: {gnu.strerror(failure)}")
          had_error = true
          break
        }
      }
    }
  }

  exit 1 when had_error
}
