#!/bin/xsh
use lib.gnu

type Options = {portable: Bool, special: Bool, portability: Bool, help: Bool, version: Bool, paths: List[Str]}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    portable: {form: "-p", default: false},
    special: {form: "-P", default: false},
    portability: {form: "--portability", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...NAME"},
  })?
  if opts.help { gnu.help("Usage: pathchk [OPTION]... NAME...\nCheck whether file names are valid or portable.\n  -p  check for most POSIX systems\n  -P  check for empty names and leading hyphens\n  --portability  check all portability constraints\n"); return }
  if opts.version { gnu.version("pathchk"); return }
  if opts.paths.is_empty() { gnu.missing_operand() }
  let portable = opts.portable or opts.portability
  let special = opts.special or opts.portability
  var failed = false
  for name in opts.paths {
    if name == "" {
      gnu.error(if portable or special { "empty file name" } else { "'': No such file or directory" })
      failed = true
      continue
    }
    var parent_dir = if name.starts_with("/") { p"/" } else { p"." }
    var path_max = 256
    var name_max = 14
    if ! portable {
      let limits = fs.path_limits(parent_dir)?
      path_max = limits.path_max
      name_max = limits.name_max
    }
    if name.byte_len() > path_max and path_max > 0 { gnu.error(f"limit {path_max} exceeded by length {name.byte_len()} of file name {gnu.quote(name)}"); failed = true; continue }
    var valid = true
    for part in name.split("/") {
      continue when part == ""
      if special and part.starts_with("-") { gnu.error(f"leading '-' in a component of file name {gnu.quote(name)}"); failed = true; valid = false; break }
      if portable and ! rx"^[A-Za-z0-9._-]+$".matches(part) { gnu.error(f"nonportable character in file name {gnu.quote(name)}"); failed = true; valid = false; break }
      if name_max > 0 and part.byte_len() > name_max { gnu.error(f"limit {name_max} exceeded by length {part.byte_len()} of file name component {gnu.quote(part)}"); failed = true; valid = false; break }
      if ! portable {
        let candidate = fp"{parent_dir}/{part}"
        let info = fs.stat(candidate, follow_symlinks: true)
        if let Ok(found) = info {
          if found.kind == "dir" {
            parent_dir = candidate
            let limits = fs.path_limits(parent_dir)?
            name_max = limits.name_max
          }
        } else if let Err(failure) = info {
          if failure.errno != 2 { gnu.name_error(name, failure); failed = true; valid = false; break }
        }
        parent_dir = candidate
      }
    }
    if valid {
      if let Err(failure) = fs.stat(fp"{name}", follow_symlinks: false) {
        if failure.errno != 2 { gnu.name_error(name, failure); failed = true }
      }
    }
  }
  if failed { exit 1 }
}
