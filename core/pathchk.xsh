#!/bin/xsh
use lib.gnu

const USAGE = """Usage: pathchk [OPTION]... NAME...
Check whether each NAME is valid or portable.

  -p        check for most POSIX portability problems
  -P        check for empty names and leading '-' characters
      --portability  check all portability problems
      --help         display this help and exit
      --version      output version information and exit
"""

type PathchkOptions = {posix: Bool, special: Bool, portability: Bool, help: Bool, version: Bool, paths: List[Str]}

pure portable_char(byte: Int) -> Bool {
  (byte >= 48 and byte <= 57) or (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122) or byte == 45 or byte == 46 or byte == 95
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: PathchkOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      posix: {form: "-p", default: false},
      special: {form: "-P", default: false},
      portability: {form: "--portability", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      paths: {form: "...NAME"},
    },
  )?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("pathchk"); return }
  if opts.paths.len() == 0 { gnu.missing_operand() }

  let posix = opts.posix or opts.portability
  let special = opts.special or opts.portability
  var failed = false
  for name in opts.paths {
    let components = name.split("/")
    if name == "" and (posix or special) {
      gnu.error("empty file name")
      failed = true
      continue
    }
    if name == "" {
      gnu.error("'': No such file or directory")
      failed = true
      continue
    }
    if special {
      var found = false
      for component in components {
        if ! found and component.starts_with("-") {
          gnu.error(f"leading '-' in a component of file name {gnu.quote(name)}")
          failed = true
          found = true
        }
      }
    }

    if posix {
      if name.byte_len() > 256 {
        gnu.error(f"{gnu.quote(name)}: file name too long (limit 256 bytes)")
        failed = true
      }
      for component in components {
        if component.byte_len() > 14 {
          gnu.error(f"{gnu.quote(component)}: component is longer than 14 bytes")
          failed = true
        }
        let raw = bytes.from_text(component)
        for index in range(raw.len()) {
          if ! portable_char(raw.byte_at(index) ?? 0) {
            gnu.error(f"{gnu.quote(component)}: character is not portable")
            failed = true
            break
          }
        }
      }
      continue
    }

    if name.byte_len() > 4096 {
      gnu.error(f"{gnu.quote(name)}: file name too long (limit 4096 bytes)")
      failed = true
    }
    let raw_parts = [part for part in components if part != ""]
    for component in raw_parts {
      if component.byte_len() > 255 {
        gnu.error(f"{gnu.quote(component)}: component is too long (limit 255 bytes)")
        failed = true
      }
    }

    # Existing non-directory components cannot be parents of a path.
    if raw_parts.len() > 1 {
      var prefix = if name.starts_with("/") { "/" } else { "." }
      for index in range(raw_parts.len() - 1) {
        prefix = if prefix == "/" { f"/{raw_parts[index]}" } else { f"{prefix}/{raw_parts[index]}" }
        if let Ok(meta) = fs.stat(fp"{prefix}") {
          if meta.kind != "dir" {
            gnu.error(f"{gnu.quote(prefix)}: Not a directory")
            failed = true
            break
          }
        }
      }
    }
    if name.ends_with("/") {
      let final_path = fs.stat(fp"{name}")
      if let Err(failure) = final_path {
        if gnu.errno(failure) == 20 {
            gnu.error(f"{gnu.quote_maybe(name)}: Not a directory")
          failed = true
        }
      } else if let Ok(meta) = final_path {
        if meta.kind != "dir" {
          gnu.error(f"{gnu.quote_maybe(name)}: Not a directory")
          failed = true
        }
      }
    }
  }
  if failed { exit 1 }
}
