#!/bin/xsh
use lib.gnu

const USAGE = """Usage: realpath [OPTION]... FILE...
Print the resolved absolute file name.

  -e, --canonicalize-existing  all components of the path must exist
  -E, --canonicalize           all but the last component must exist (default)
  -m, --canonicalize-missing   no components need to exist
  -L, --logical                resolve components before following symlinks
  -P, --physical               resolve symlinks as encountered (default)
  -s, --strip, --no-symlinks   do not expand symlinks
  -q, --quiet                  suppress most error messages
  -z, --zero                   end each output line with NUL, not newline
      --relative-to=DIR        print paths relative to DIR
      --relative-base=DIR      print relative paths only below DIR
      --help                   display this help and exit
      --version                output version information and exit
"""

type RealpathOptions = {existing: Bool, canonicalize: Bool, missing: Bool, logical: Bool, physical: Bool, strip: Bool, quiet: Bool, zero: Bool, relative_to: Str, relative_base: Str, help: Bool, version: Bool, paths: List[Str]}
type RelativePath = {text: Str, below: Bool}

pure raw_paths(argv: List[Str], raw: List[Bytes]) -> List[Bytes] {
  var paths: List[Bytes] = []
  var index = 0
  var options = true
  while index < argv.len() {
    let arg = argv[index]
    if options and arg == "--" { options = false; index += 1; continue }
    if options and (arg == "--relative-to" or arg == "--relative-base") { index += 2; continue }
    if options and (arg.starts_with("--relative-to=") or arg.starts_with("--relative-base=") or arg == "-e" or arg == "-E" or arg == "-m" or arg == "-L" or arg == "-P" or arg == "-s" or arg == "-q" or arg == "-z" or arg == "--canonicalize-existing" or arg == "--canonicalize" or arg == "--canonicalize-missing" or arg == "--logical" or arg == "--physical" or arg == "--strip" or arg == "--no-symlinks" or arg == "--quiet" or arg == "--zero" or arg == "--help" or arg == "--version") { index += 1; continue }
    if options and arg.starts_with("-") and arg != "-" { index += 1; continue }
    paths += [raw[index]]
    index += 1
  }
  paths
}

pure join_parts(base: Path, parts: List[Str]) -> Path {
  var text = base.display()
  for part in parts { text = if text == "/" { f"/{part}" } else { f"{text}/{part}" } }
  fp"{text}"
}

pure normalized_parts(parts: List[Str]) -> List[Str] {
  var out: List[Str] = []
  for part in parts {
    if part == ".." {
      if out.len() > 0 { out = out |> take(out.len() - 1) }
    } else if part != "" and part != "." { out += [part] }
  }
  out
}

proc canonical(path_arg: Path, cwd: Path, allow_missing: Bool, allow_all_missing: Bool, strip: Bool, logical: Bool) [fs] -> Path? {
  let original = path_arg.display()
  return null when original == ""
  let trailing = original.ends_with("/")
  let absolute = if original.starts_with("/") { original } else { f"{cwd.display()}/{original}" }
  var parts = [part for part in absolute.split("/") if part != ""]
  if logical { parts = normalized_parts(parts) }
  var current = p"/"
  var pending: List[Str] = []
  var index = 0
  var followed = 0
  var dangling_final_link = false
  while index < parts.len() {
    let part = parts[index]
    if ! logical and part == ".." {
      if pending.len() > 0 { pending = pending |> take(pending.len() - 1) } else if current.display() != "/" { current = current.parent() }
      index += 1
      continue
    }
    if part == "." or part == "" { index += 1; continue }
    let candidate = join_parts(current, pending + [part])
    if trailing and allow_missing and index == parts.len() - 1 {
      if let Ok(_) = candidate.readlink() {
        if let Err(_) = fs.stat(candidate) { dangling_final_link = true }
      }
    }
    if strip {
      pending += [part]
    } else {
      let resolution = candidate.resolve()
      if let Ok(resolved) = resolution {
        current = resolved
        pending = []
        index += 1
        continue
      }
      if let Err(failure) = resolution { return null when gnu.errno(failure) == 40 }
      if let Ok(target) = candidate.readlink() {
        followed += 1
        return null when followed > 40
        dangling_final_link = dangling_final_link or index == parts.len() - 1
        let parent = join_parts(current, pending)
        let target_text = target.display()
        let expanded = if target_text.starts_with("/") { target_text } else { f"{parent.display()}/{target_text}" }
        parts = [item for item in expanded.split("/") if item != ""] + parts[index + 1..]
        if logical { parts = normalized_parts(parts) }
        current = p"/"
        pending = []
        index = 0
        continue
      }
      if allow_all_missing {
        pending += [part]
      } else if allow_missing and index == parts.len() - 1 and (! trailing or dangling_final_link) {
        let parent = join_parts(current, pending)
        if let Ok(meta) = fs.stat(parent) {
          return null when meta.kind != "dir"
          pending += [part]
        } else {
          return null
        }
      } else {
        return null
      }
    }
    index += 1
  }
  let result = join_parts(current, pending)
  if trailing and ! allow_all_missing and pending.len() == 0 {
    if let Ok(meta) = fs.stat(result) { return null when meta.kind != "dir" } else { return null when ! (allow_missing and dangling_final_link) }
  }
  return null when trailing and pending.len() > 0 and ! allow_all_missing and ! (allow_missing and dangling_final_link)
  result
}

pure relative_text(target: Path, base: Path) -> RelativePath {
  let target_parts = [part for part in target.display().split("/") if part != ""]
  let base_parts = [part for part in base.display().split("/") if part != ""]
  var common = 0
  while common < target_parts.len() and common < base_parts.len() and target_parts[common] == base_parts[common] { common += 1 }
  var pieces: List[Str] = []
  for _ in range(base_parts.len() - common) { pieces += [".."] }
  pieces += target_parts[common..]
  {text: if pieces.len() == 0 { "." } else { pieces.join("/") }, below: common == base_parts.len()}
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: RealpathOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      existing: {form: "-e --canonicalize-existing", default: false},
      canonicalize: {form: "-E --canonicalize", default: false},
      missing: {form: "-m --canonicalize-missing", default: false},
      logical: {form: "-L --logical", default: false},
      physical: {form: "-P --physical", default: false},
      strip: {form: "-s --strip --no-symlinks", default: false},
      quiet: {form: "-q --quiet", default: false},
      zero: {form: "-z --zero", default: false},
      relative_to: {form: "--relative-to DIR", default: ""},
      relative_base: {form: "--relative-base DIR", default: ""},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      paths: {form: "...FILE"},
    },
  )?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("realpath"); return }
  if opts.paths.len() == 0 { gnu.missing_operand() }

  var relative_to_given = false
  var relative_base_given = false
  for arg in argv {
    if arg == "--relative-to" or arg.starts_with("--relative-to=") { relative_to_given = true }
    if arg == "--relative-base" or arg.starts_with("--relative-base=") { relative_base_given = true }
  }
  if relative_to_given and opts.relative_to == "" { gnu.error("invalid empty relative-to directory"); exit 1 }
  if relative_base_given and opts.relative_base == "" { gnu.error("invalid empty relative-base directory"); exit 1 }

  var mode = "normal"
  for arg in argv {
    if arg == "-e" or arg == "--canonicalize-existing" { mode = "existing" }
    if arg == "-E" or arg == "--canonicalize" { mode = "normal" }
    if arg == "-m" or arg == "--canonicalize-missing" { mode = "missing" }
  }
  let allow_missing = mode == "normal"
  let allow_all = mode == "missing"
  let cwd = fs.cwd()?
  var relative_to: Path? = null
  var relative_base: Path? = null
  if opts.relative_to != "" {
    relative_to = canonical(fp"{opts.relative_to}", cwd, allow_missing, allow_all, opts.strip, opts.logical)
    if relative_to == null { gnu.error(f"{gnu.quote(opts.relative_to)}: cannot resolve relative directory"); exit 1 }
    if mode == "existing" {
      if let Ok(meta) = fs.stat(relative_to ?? p"/") {
        if meta.kind != "dir" { gnu.error(f"{gnu.quote(opts.relative_to)}: not a directory"); exit 1 }
      } else {
        gnu.error(f"{gnu.quote(opts.relative_to)}: cannot resolve relative directory")
        exit 1
      }
    }
  }
  if opts.relative_base != "" {
    relative_base = canonical(fp"{opts.relative_base}", cwd, allow_missing, allow_all, opts.strip, opts.logical)
    if relative_base == null { gnu.error(f"{gnu.quote(opts.relative_base)}: cannot resolve relative directory"); exit 1 }
    if mode == "existing" {
      if let Ok(meta) = fs.stat(relative_base ?? p"/") {
        if meta.kind != "dir" { gnu.error(f"{gnu.quote(opts.relative_base)}: not a directory"); exit 1 }
      } else {
        gnu.error(f"{gnu.quote(opts.relative_base)}: cannot resolve relative directory")
        exit 1
      }
    }
  }
  let relative_options_compatible = if relative_to != null and relative_base != null {
    relative_text(relative_to ?? p"/", relative_base ?? p"/").below
  } else { true }

  let ending = if opts.zero { "\0" } else { "\n" }
  var failed = false
  let raw_names = raw_paths(argv, cli.argv_bytes())
  for index in range(opts.paths.len()) {
    let name = opts.paths[index]
    let raw_name = raw_names[index]
    let target_path = Path.parse_bytes(raw_name)?
    let invalid_utf8 = if let Err(_) = raw_name.utf8() { true } else { false }
    let result: Path? = if name == "" { null } else if invalid_utf8 {
      if opts.strip { target_path.normalize() } else if let Ok(resolved) = target_path.resolve() { resolved } else { null }
    } else { canonical(target_path, cwd, allow_missing, allow_all, opts.strip, opts.logical) }
    if result == null {
      failed = true
      if ! opts.quiet {
        let message = if let Err(failure) = target_path.resolve() { gnu.strerror(failure) } else { "No such file or directory" }
        gnu.error(f"{gnu.quote_bytes(raw_name, always: false)}: {message}")
      }
    } else {
      let target = result ?? p"/"
      var rendered = target.display()
      if relative_to != null {
        let to = relative_to ?? p"/"
        let rel = relative_text(target, to)
        if relative_base == null {
          rendered = rel.text
        } else if relative_options_compatible and relative_text(target, relative_base ?? p"/").below {
          rendered = rel.text
        }
      } else if relative_base != null {
        let base = relative_base ?? p"/"
        let rel = relative_text(target, base)
        if rel.below { rendered = rel.text }
      }
      let rendered_bytes = if invalid_utf8 and opts.relative_to == "" and opts.relative_base == "" { target.bytes() } else { bytes.from_text(rendered) }
      gnu.write_bytes(bytes.concat([rendered_bytes, bytes.from_text(ending)]))
    }
  }
  if failed { exit 1 }
}
