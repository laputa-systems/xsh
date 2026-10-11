#!/bin/xsh
use lib.gnu
use lib.fs_misc

type Options = {canonical: Bool, existing: Bool, missing: Bool, no_newline: Bool, quiet: Bool, verbose: Bool, zero: Bool, help: Bool, version: Bool, paths: List[Str]}
type RawArgument = {marker: Str, value: Bytes}
type PreparedArguments = {text: List[Str], raw: List[RawArgument]}

# The shared option parser takes text; NUL-marked operands preserve Unix argv bytes.
pure prepare_arguments(argv: List[Bytes]) -> PreparedArguments {
  var text: List[Str] = []
  var raw: List[RawArgument] = []
  for index in range(argv.len()) {
    let argument = argv[index]
    match argument.utf8() {
      Ok(value) => text += [value]
      Err(_) => {
        let marker = f"\0readlink-raw-argument-{index}\0"
        text += [marker]
        raw += [{marker: marker, value: argument}]
      }
    }
  }
  {text: text, raw: raw}
}

pure argument_bytes(value: Str, raw: List[RawArgument]) -> Bytes {
  for argument in raw {
    if argument.marker == value { return argument.value }
  }
  bytes.from_text(value)
}

proc link_value(name: Bytes) [fs, error] -> Result[Bytes] {
  Ok(Path.parse_bytes(name)?.readlink()?.bytes())
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  let prepared = prepare_arguments(argv)
  let opts: Options = cli.applet(prepared.text, {
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
  for arg in prepared.text {
    break when arg == "--"
    if arg == "--verbose" { verbose = true } else if arg == "--quiet" or arg == "--silent" { verbose = false } else if arg.starts_with("-") and ! arg.starts_with("--") { for flag in arg { if flag == "v" { verbose = true } else if flag == "q" or flag == "s" { verbose = false } } }
  }
  let mode = fs_misc.canonical_mode(prepared.text, if opts.existing { "existing" } else if opts.missing { "missing" } else { "normal" })
  if env.get("POSIXLY_CORRECT") is Ok(_) { verbose = true }
  var failed = false
  for operand in opts.paths {
    let name = argument_bytes(operand, prepared.raw)
    let value = if let Ok(text) = name.utf8() {
      if opts.canonical or opts.existing or opts.missing {
        match fs_misc.canonical(text, mode) { Ok(value) => Ok(bytes.from_text(value)), Err(failure) => Err(failure) }
      } else { link_value(name) }
    } else if opts.canonical or opts.existing or opts.missing {
      match Path.parse_bytes(name)?.resolve() { Ok(resolved_path) => Ok(resolved_path.bytes()), Err(failure) => Err(failure) }
    } else { link_value(name) }
    if let Ok(value) = value { gnu.write_bytes(bytes.concat([value, bytes.from_text(ending)])) } else if let Err(failure) = value { failed = true; if verbose { gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}") } }
  }
  if failed { exit 1 }
}
