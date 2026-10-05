#!/bin/xsh
use lib.gnu
use lib.fs_misc

type Options = {canonical: Bool, existing: Bool, missing: Bool, no_newline: Bool, quiet: Bool, verbose: Bool, zero: Bool, help: Bool, version: Bool, paths: List[Str]}

proc link_value(name: Str) [fs, error] -> Result[Str] {
  Ok(fp"{name}".readlink()?.display())
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    canonical: {form: "-f --canonicalize", default: false},
    existing: {form: "-e --canonicalize-existing", default: false},
    missing: {form: "-m --canonicalize-missing", default: false},
    no_newline: {form: "-n --no-newline", default: false},
    quiet: {form: "-q -s --quiet --silent", default: false},
    verbose: {form: "-v --verbose", default: false},
    zero: {form: "-z --zero", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: readlink [OPTION]... FILE...\nPrint symbolic link targets or canonical file names.\n  -f, --canonicalize\n  -e, --canonicalize-existing\n  -m, --canonicalize-missing\n  -n, --no-newline\n  -z, --zero\n  -v, --verbose\n  -q, --quiet\n  -s, --silent\n"); return }
  if opts.version { gnu.version("readlink"); return }
  if opts.paths.is_empty() { gnu.missing_operand() }
  if opts.no_newline and opts.paths.len() > 1 { gnu.error("ignoring --no-newline with multiple arguments") }
  let ending = if opts.no_newline and opts.paths.len() == 1 { "" } else if opts.zero { "\0" } else { "\n" }
  var verbose = opts.verbose or env.get("POSIXLY_CORRECT") is Ok(_)
  if opts.quiet { verbose = false }
  for arg in argv {
    break when arg == "--"
    if arg == "--verbose" { verbose = true } else if arg == "--quiet" or arg == "--silent" { verbose = false } else if arg.starts_with("-") and ! arg.starts_with("--") { for flag in arg { if flag == "v" { verbose = true } else if flag == "q" or flag == "s" { verbose = false } } }
  }
  let mode = fs_misc.canonical_mode(argv, if opts.existing { "existing" } else if opts.missing { "missing" } else { "normal" })
  if env.get("POSIXLY_CORRECT") is Ok(_) { verbose = true }
  var failed = false
  for name in opts.paths {
    let result = if opts.canonical or opts.existing or opts.missing {
      fs_misc.canonical(name, mode)
    } else { link_value(name) }
    if let Ok(value) = result { gnu.write_text(value + ending) } else if let Err(failure) = result { failed = true; if verbose { gnu.name_error(name, failure) } }
  }
  if failed { exit 1 }
}
