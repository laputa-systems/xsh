#!/bin/xsh
use lib.gnu
use lib.fs_misc

type Options = {canonical: Bool, existing: Bool, missing: Bool, logical: Bool, physical: Bool, strip: Bool, quiet: Bool, zero: Bool, relative_to: Str?, relative_base: Str?, help: Bool, version: Bool, paths: List[Str]}
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
        let marker = f"\0realpath-raw-argument-{index}\0"
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

pure path_components(name: Bytes) -> List[Bytes] {
  var parts: List[Bytes] = []
  var begin = 0
  for index in range(name.len()) {
    if name.byte_at(index) == 47 {
      if index > begin { parts += [name.slice(begin, length: index - begin)] }
      begin = index + 1
    }
  }
  if name.len() > begin { parts += [name.slice(begin)] }
  parts
}

pure within(target_path: Bytes, base: Bytes) -> Bool {
  return true when base == b"/"
  target_path == base or target_path.starts_with(bytes.concat([base, b"/"]))
}

pure relative_path(target: Bytes, base: Bytes) -> Bytes {
  let target_parts = path_components(target)
  let base_parts = path_components(base)
  var common = 0
  while common < target_parts.len() and common < base_parts.len() and target_parts[common] == base_parts[common] { common += 1 }
  var parts: List[Bytes] = []
  for unused in range(base_parts.len() - common) { parts += [b".."] }
  for index in range(common, target_parts.len()) { parts += [target_parts[index]] }
  if parts.is_empty() { return b"." }
  var output = parts[0]
  for index in range(1, parts.len()) { output = bytes.concat([output, b"/", parts[index]]) }
  output
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  let prepared = prepare_arguments(argv)
  let opts: Options = cli.applet(prepared.text, {
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
  for arg in prepared.text {
    break when arg == "--"
    if arg == "--logical" { logical = true } else if arg == "--physical" { logical = false } else if arg.starts_with("-") and ! arg.starts_with("--") {
      for flag in arg { if flag == "L" { logical = true } else if flag == "P" { logical = false } }
    }
  }
  let missing = fs_misc.canonical_mode(prepared.text, if opts.existing { "existing" } else if opts.missing { "missing" } else { "normal" })
  if opts.relative_base == "" or opts.relative_to == "" { gnu.error("No such file or directory"); exit 1 }
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
  for operand in opts.paths {
    let name = argument_bytes(operand, prepared.raw)
    let result = if let Ok(text) = name.utf8() {
      match fs_misc.canonical(text, missing, logical: logical, strip: opts.strip) { Ok(value) => Ok(bytes.from_text(value)), Err(failure) => Err(failure) }
    } else {
      match Path.parse_bytes(name)?.resolve() { Ok(resolved_path) => Ok(resolved_path.bytes()), Err(failure) => Err(failure) }
    }
    if let Ok(resolved_bytes) = result {
      let base_bytes = bytes.from_text(base)
      let to_bytes = bytes.from_text(to)
      let inside_base = base == "" or base == "/" or within(resolved_bytes, base_bytes)
      let to_inside_base = base == "" or base == "/" or within(to_bytes, base_bytes)
      let value = if to != "" and inside_base and to_inside_base { relative_path(resolved_bytes, to_bytes) } else { resolved_bytes }
      gnu.write_bytes(bytes.concat([value, if opts.zero { b"\0" } else { b"\n" }]))
    } else if let Err(failure) = result { failed = true; if ! opts.quiet { gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}") } }
  }
  if failed { exit 1 }
}
