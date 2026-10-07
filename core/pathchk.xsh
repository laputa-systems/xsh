#!/bin/xsh
use lib.gnu

type Options = {portable: Bool, special: Bool, portability: Bool, help: Bool, version: Bool, paths: List[Str]}
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
        let marker = f"\0pathchk-raw-argument-{index}\0"
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

pure portable_component(part: Bytes) -> Bool {
  for index in range(part.len()) {
    let byte = part.byte_at(index) ?? 0
    if ! ((byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122) or (byte >= 48 and byte <= 57) or byte == 46 or byte == 95 or byte == 45) { return false }
  }
  true
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  let prepared = prepare_arguments(argv)
  let opts: Options = cli.applet(prepared.text, {
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
  if opts.paths.is_empty() { gnu.usage_error("error: the following required arguments were not provided: <NAME>") }
  let portable = opts.portable or opts.portability
  let special = opts.special or opts.portability
  var failed = false
  for operand in opts.paths {
    let name = argument_bytes(operand, prepared.raw)
    if name == b"" {
      gnu.error(if portable or special { "empty file name" } else { "'': No such file or directory" })
      failed = true
      continue
    }
    var parent_dir = if name.starts_with(b"/") { p"/" } else { p"." }
    var path_max = 256
    var name_max = 14
    if ! portable {
      let limits = fs.path_limits(parent_dir)?
      path_max = limits.path_max
      name_max = limits.name_max
    }
    if name.len() > path_max and path_max > 0 { gnu.error(f"limit {path_max} exceeded by length {name.len()} of file name {gnu.quote_bytes(name)}"); failed = true; continue }
    var valid = true
    for part in path_components(name) {
      if special and part.starts_with(b"-") { gnu.error(f"leading '-' in a component of file name {gnu.quote_bytes(name)}"); failed = true; valid = false; break }
      if portable and ! portable_component(part) { gnu.error(f"nonportable character in file name {gnu.quote_bytes(name)}"); failed = true; valid = false; break }
      if name_max > 0 and part.len() > name_max { gnu.error(f"limit {name_max} exceeded by length {part.len()} of file name component {gnu.quote_bytes(part)}"); failed = true; valid = false; break }
      if ! portable {
        let candidate = Path.parse_bytes(bytes.concat([parent_dir.bytes(), b"/", part]))?
        let info = fs.stat(candidate, follow_symlinks: true)
        if let Ok(found) = info {
          if found.kind == "dir" {
            parent_dir = candidate
            let limits = fs.path_limits(parent_dir)?
            name_max = limits.name_max
          }
        } else if let Err(failure) = info {
          if failure.errno != 2 { gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}"); failed = true; valid = false; break }
        }
        parent_dir = candidate
      }
    }
    if valid {
      if let Err(failure) = fs.stat(Path.parse_bytes(name)?, follow_symlinks: false) {
        if failure.errno != 2 { gnu.error(f"{gnu.quote_bytes(name, always: false)}: {gnu.strerror(failure)}"); failed = true }
      }
    }
  }
  if failed { exit 1 }
}
