#!/bin/xsh
use lib.gnu
use lib.fs_misc

type Options = {canonical: Bool, existing: Bool, missing: Bool, logical: Bool, physical: Bool, strip: Bool, quiet: Bool, zero: Bool, relative_to: Str?, relative_base: Str?, help: Bool, version: Bool, paths: List[Str]}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    canonical: {form: "-E --canonicalize", default: false},
    existing: {form: "-e --canonicalize-existing", default: false},
    missing: {form: "-m --canonicalize-missing", default: false},
    logical: {form: "-L --logical", default: false},
    physical: {form: "-P --physical", default: false},
    strip: {form: "-s --strip --no-symlinks", default: false},
    quiet: {form: "-q --quiet", default: false},
    zero: {form: "-z --zero", default: false},
    relative_to: {form: "--relative-to DIR"},
    relative_base: {form: "--relative-base DIR"},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: realpath [OPTION]... FILE...\nPrint resolved absolute file names.\n  -e, --canonicalize-existing\n  -m, --canonicalize-missing\n  -L, --logical\n  -P, --physical\n  -s, --strip, --no-symlinks\n  --relative-to=DIR\n  --relative-base=DIR\n  -q, --quiet\n  -z, --zero\n"); return }
  if opts.version { gnu.version("realpath"); return }
  if opts.paths.is_empty() { gnu.missing_operand() }
  var logical = opts.logical and ! opts.physical
  for arg in argv {
    break when arg == "--"
    if arg == "--logical" { logical = true } else if arg == "--physical" { logical = false } else if arg.starts_with("-") and ! arg.starts_with("--") {
      for flag in arg { if flag == "L" { logical = true } else if flag == "P" { logical = false } }
    }
  }
  let missing = fs_misc.canonical_mode(argv, if opts.existing { "existing" } else if opts.missing { "missing" } else { "normal" })
  let base_name = opts.relative_base ?? ""
  let to_name = opts.relative_to ?? base_name
  var base = ""
  var to = ""
  if base_name != "" {
    let result = fs_misc.canonical(base_name, missing, logical: logical, strip: opts.strip)
    if let Err(failure) = result { gnu.name_error(base_name, failure); exit 1 }
    base = result?
    if missing == "existing" and fs.stat(fp"{base}", follow_symlinks: true)?.kind != "dir" { gnu.error(f"{gnu.quote(base_name)}: Not a directory"); exit 1 }
  }
  if to_name != "" {
    let result = fs_misc.canonical(to_name, missing, logical: logical, strip: opts.strip)
    if let Err(failure) = result { gnu.name_error(to_name, failure); exit 1 }
    to = result?
    if missing == "existing" and fs.stat(fp"{to}", follow_symlinks: true)?.kind != "dir" { gnu.error(f"{gnu.quote(to_name)}: Not a directory"); exit 1 }
  }
  var failed = false
  for name in opts.paths {
    let result = fs_misc.canonical(name, missing, logical: logical, strip: opts.strip)
    if let Ok(resolved) = result {
      let within = base == "" or base == "/" or resolved == base or resolved.starts_with(base + "/")
      let to_within = base == "" or base == "/" or to == base or to.starts_with(base + "/")
      let value = if to != "" and within and to_within { fs_misc.relative(resolved, to) } else { resolved }
      gnu.write_text(value + (if opts.zero { "\0" } else { "\n" }))
    } else if let Err(failure) = result { failed = true; if ! opts.quiet { gnu.name_error(name, failure) } }
  }
  if failed { exit 1 }
}
