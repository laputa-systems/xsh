#!/bin/xsh
use lib.gnu

const DEFAULT_TEMPLATE = "tmp.XXXXXXXXXX"
const USAGE = """Usage: mktemp [OPTION]... [TEMPLATE]
Create a temporary file or directory, safely, and print its name.

  -d, --directory       create a directory, not a file
  -p, --tmpdir[=DIR]    use DIR as the prefix; without DIR use $TMPDIR or /tmp
  -t                    interpret TEMPLATE relative to $TMPDIR
  -u, --dry-run         print a name without creating it (unsafe)
  -q, --quiet           suppress diagnostics about failed name creation
      --suffix=SUFF     append SUFF to TEMPLATE
      --help            display this help and exit
      --version         output version information and exit
"""

type MktempOptions = {directory: Bool, tmpdir: Str, treat_as_template: Bool, dry_run: Bool, quiet: Bool, suffix: Str, help: Bool, version: Bool, templates: List[Str]}

pure parent_of(text: Str) -> Str {
  var at = 0
  var slash = -1
  while at < text.byte_len() {
    if text.byte_slice(at, length: 1) == "/" { slash = at }
    at += 1
  }
  return "." when slash < 0
  return "/" when slash == 0
  text.byte_slice(0, length: slash)
}

pure leaf_of(text: Str) -> Str {
  var at = 0
  var slash = -1
  while at < text.byte_len() {
    if text.byte_slice(at, length: 1) == "/" { slash = at }
    at += 1
  }
  text.byte_slice(slash + 1)
}

pure randomize(pattern_text: Str, start: Int, end: Int, seed: Str) -> Str {
  var token = ""
  let count = end - start
  var at = 0
  while at < count {
    token = f"{token}{seed.byte_slice(at % seed.byte_len(), length: 1)}"
    at += 1
  }
  f"{pattern_text.byte_slice(0, length: start)}{token}{pattern_text.byte_slice(end)}"
}

proc create_name(directory: Path, pattern: Str, suffix: Str, make_directory: Bool, dry_run: Bool, display_dot_slash: Bool) [fs, env, process, error] -> Result[Path] {
  for _ in range(128) {
    if make_directory {
      let root = fs.tempdir()?
      let source = root.host_path()?
      let raw_seed = source.basename()
      let seed = if raw_seed.starts_with(".") { raw_seed.byte_slice(1) } else { raw_seed }
      let candidate = randomize(pattern, find_x_start(pattern), find_x_end(pattern), seed) + suffix
      let target_bytes = if directory.display() == "." and display_dot_slash { bytes.from_text(f"./{candidate}") } else if directory.display() == "." { bytes.from_text(candidate) } else { bytes.concat([directory.bytes(), b"/", bytes.from_text(candidate)]) }
      let target = Path.parse_bytes(target_bytes)?
      if dry_run { return Ok(target) }
      let created = fs.mkdir(target)
      if let Ok(_) = created {
        if let Err(failure) = target.chmod(0o700) { return Err(failure) }
        return Ok(target)
      }
    } else {
      let temp = fs.tempfile()?
      let temp_root = temp.root.host_path()?
      let source = fp"{temp_root}/{temp.path}"
      let raw_seed = temp_root.basename()
      let seed = if raw_seed.starts_with(".") { raw_seed.byte_slice(1) } else { raw_seed }
      let candidate = randomize(pattern, find_x_start(pattern), find_x_end(pattern), seed) + suffix
      let target_bytes = if directory.display() == "." and display_dot_slash { bytes.from_text(f"./{candidate}") } else if directory.display() == "." { bytes.from_text(candidate) } else { bytes.concat([directory.bytes(), b"/", bytes.from_text(candidate)]) }
      let target = Path.parse_bytes(target_bytes)?
      if dry_run { return Ok(target) }
      let copied = fs.copy_file(source, target, "auto", "auto", false, 0o600)
      if let Ok(_) = copied {
        fs.remove(source)?
        return Ok(target)
      }
    }
  }
  gnu.error("cannot create temporary name")
  exit 1
  Ok(p"/")
}

pure find_x_end(text: Str) -> Int {
  var end = 0
  var at = 0
  while at < text.byte_len() {
    if text.byte_slice(at, length: 1) == "X" { end = at + 1 }
    at += 1
  }
  end
}

pure find_x_start(text: Str) -> Int {
  var end = find_x_end(text)
  var start = end
  while start > 0 and text.byte_slice(start - 1, length: 1) == "X" { start -= 1 }
  start
}

pure raw_templates(argv: List[Str], raw: List[Bytes]) -> List[Bytes] {
  var templates: List[Bytes] = []
  var index = 0
  var options = true
  while index < argv.len() {
    let arg = argv[index]
    if options and arg == "--" { options = false; index += 1; continue }
    if options and (arg == "--suffix" or arg == "-p") { index += 2; continue }
    if options and arg == "--tmpdir" { index += 1; continue }
    if options and (arg.starts_with("--suffix=") or arg.starts_with("--tmpdir=") or (arg.starts_with("-p") and arg.byte_len() > 2)) { index += 1; continue }
    if options and (arg == "-d" or arg == "--directory" or arg == "-t" or arg == "-u" or arg == "--dry-run" or arg == "-q" or arg == "--quiet" or arg == "--help" or arg == "--version") { index += 1; continue }
    if options and arg.starts_with("-") and arg != "-" { index += 1; continue }
    templates += [raw[index]]
    index += 1
  }
  templates
}

pure raw_tmpdir(argv: List[Str], raw: List[Bytes]) -> Bytes? {
  var index = 0
  var options = true
  var found: Bytes? = null
  while index < argv.len() {
    let arg = argv[index]
    if options and arg == "--" { options = false; index += 1; continue }
    if options and arg == "-p" {
      if index + 1 < argv.len() and ! argv[index + 1].starts_with("-") {
        found = raw[index + 1]
        index += 2
      } else {
        found = b""
        index += 1
      }
      continue
    }
    if options and arg == "--tmpdir" { found = b""; index += 1; continue }
    if options and arg.starts_with("--tmpdir=") { found = raw[index].slice(9); index += 1; continue }
    if options and arg.starts_with("-p") and arg.byte_len() > 2 { found = raw[index].slice(2); index += 1; continue }
    if options and arg == "--suffix" { index += 2; continue }
    if options and (arg.starts_with("--suffix=") or arg == "-d" or arg == "--directory" or arg == "-t" or arg == "-u" or arg == "--dry-run" or arg == "-q" or arg == "--quiet" or arg == "--help" or arg == "--version") { index += 1; continue }
    if options and arg.starts_with("-") and arg != "-" { index += 1; continue }
    options = false
    index += 1
  }
  found
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let raw = cli.argv_bytes()
  let templates_raw = raw_templates(argv, raw)
  if templates_raw.len() > 0 and (if let Err(_) = templates_raw[0].utf8() { true } else { false }) {
    gnu.error("invalid template")
    exit 1
  }
  let tmpdir_raw = raw_tmpdir(argv, raw)
  var adjusted = []
  for index in range(argv.len()) {
    let arg = argv[index]
    let missing_short_tmpdir = arg == "-p" and (index + 1 >= argv.len() or (argv.get(index + 1) ?? "").starts_with("-"))
    adjusted += [if arg == "--tmpdir" or missing_short_tmpdir { "--tmpdir=" } else { arg }]
  }
  let opts: MktempOptions = cli.applet(
    adjusted,
    {
      gnu: {status: 1},
      directory: {form: "-d --directory", default: false},
      tmpdir: {form: "-p --tmpdir DIR", default: ""},
      treat_as_template: {form: "-t", default: false},
      dry_run: {form: "-u --dry-run", default: false},
      quiet: {form: "-q --quiet", default: false},
      suffix: {form: "--suffix SUFFIX", default: ""},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      templates: {form: "...TEMPLATE"},
    },
  )?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("mktemp"); return }
  if opts.templates.len() > 1 { gnu.usage_error("too many templates") }

  var tmpdir_given = false
  var suffix_given = false
  var short_tmpdir_missing = false
  for index in range(argv.len()) {
    let arg = argv[index]
    if arg == "--tmpdir" or arg.starts_with("--tmpdir=") or arg.starts_with("-p") { tmpdir_given = true }
    if arg == "--suffix" or arg.starts_with("--suffix=") { suffix_given = true }
    if arg == "-p" and (index + 1 >= argv.len() or (argv.get(index + 1) ?? "").starts_with("-")) { short_tmpdir_missing = true }
  }
  if short_tmpdir_missing { gnu.error("a value is required for '-p <DIR>' but none was supplied"); exit 1 }

  let raw_template = if opts.templates.len() == 0 { DEFAULT_TEMPLATE } else { opts.templates[0] }
  let has_template = opts.templates.len() > 0
  if opts.suffix.find("/") != null {
    let slash = opts.suffix.find("/") ?? 0
    gnu.error(f"invalid suffix {gnu.quote(opts.suffix.byte_slice(slash))}, contains directory separator")
    exit 1
  }
  if suffix_given and ! raw_template.ends_with("X") {
    gnu.error(f"with --suffix, template {gnu.quote(raw_template)} must end in X")
    exit 1
  }

  let start = find_x_start(raw_template)
  let end = find_x_end(raw_template)
  let template_suffix = if suffix_given { "" } else { raw_template.byte_slice(end) }
  if template_suffix.find("/") != null {
    let slash = template_suffix.find("/") ?? 0
    gnu.error(f"invalid suffix {gnu.quote(template_suffix.byte_slice(slash))}, contains directory separator")
    exit 1
  }
  if raw_template.find("/") != null and opts.treat_as_template {
    gnu.error(f"invalid template, {gnu.quote(raw_template)}, contains directory separator")
    exit 1
  }
  if end - start < 3 { gnu.error(f"too few X's in template {gnu.quote(raw_template)}"); exit 1 }
  let prefix = raw_template.byte_slice(0, length: start)
  let stem = raw_template.byte_slice(0, length: end)
  let template_dir = if stem.find("/") != null { parent_of(prefix + "X") } else { "." }
  if tmpdir_given and ! opts.treat_as_template and prefix.starts_with("/") {
    gnu.error(f"invalid template, {gnu.quote(raw_template)}; with --tmpdir, it may not be absolute")
    exit 1
  }

  var directory = fp"."
  var pattern = ""
  if opts.treat_as_template {
    let temp_root = env.get_or("TMPDIR", if tmpdir_given and opts.tmpdir != "" { opts.tmpdir } else { "/tmp" }) ?? "/tmp"
    directory = if temp_root == "" { p"/tmp" } else { fp"{temp_root}" }
    pattern = leaf_of(stem)
  } else if tmpdir_given {
    let temp_root = if opts.tmpdir != "" { opts.tmpdir } else { env.get_or("TMPDIR", "/tmp") ?? "/tmp" }
    if tmpdir_raw != null and (tmpdir_raw ?? b"").len() > 0 {
      let raw_directory = if raw_template.find("/") != null {
        bytes.concat([tmpdir_raw ?? b"", b"/", bytes.from_text(template_dir)])
      } else { tmpdir_raw ?? b"" }
      directory = Path.parse_bytes(raw_directory)?
    } else {
      directory = if temp_root == "" { p"/tmp" } else if raw_template.find("/") != null { fp"{temp_root}/{template_dir}" } else { fp"{temp_root}" }
    }
    pattern = leaf_of(stem)
  } else if ! has_template {
    let temp_root = env.get_or("TMPDIR", "/tmp") ?? "/tmp"
    directory = if temp_root == "" { p"/tmp" } else { fp"{temp_root}" }
    pattern = leaf_of(stem)
  } else if raw_template.find("/") != null {
    directory = fp"{template_dir}"
    pattern = leaf_of(stem)
  } else {
    pattern = stem
  }

  # Keep Xs in the pattern while normalizing it to the final path component.
  let pattern_start = find_x_start(pattern)
  let pattern_end = find_x_end(pattern)
  if pattern_end - pattern_start < 3 { gnu.error(f"too few X's in template {gnu.quote(raw_template)}"); exit 1 }
  if opts.treat_as_template and (pattern.find("/") != null or raw_template.find("/") != null) {
    gnu.usage_error("template prefix contains directory separator")
  }
  let display_dot_slash = !has_template or tmpdir_given or opts.treat_as_template
  let directory_prefix = if directory.display() == "." { if display_dot_slash { "./" } else { "" } } else { f"{directory}/" }
  let error_template = if has_template { raw_template } else { directory_prefix + pattern + template_suffix + opts.suffix }
  if let Err(failure) = directory.metadata() {
    if ! opts.quiet { gnu.error(f"failed to create {if opts.directory {"directory"} else {"file"}} via template {gnu.quote(error_template)}: {gnu.strerror(failure)}") }
    exit 1
  }

  let output_suffix = template_suffix + opts.suffix
  let made = create_name(directory, pattern, output_suffix, opts.directory, opts.dry_run, display_dot_slash)
  if let Err(failure) = made {
    if ! opts.quiet { gnu.error(f"failed to create {if opts.directory {"directory"} else {"file"}} via template {gnu.quote(error_template)}: {gnu.strerror(failure)}") }
    exit 1
  } else if let Ok(result_path) = made {
    let output = bytes.concat([result_path.bytes(), b"\n"])
    if let Err(failure) = io.write_stdout_bytes(output) {
      if ! opts.dry_run {
        if let Err(_) = result_path.remove() {}
      }
      gnu.write_failed(failure)
    }
    if let Err(failure) = io.flush_stdout() {
      if ! opts.dry_run {
        if let Err(_) = result_path.remove() {}
      }
      gnu.write_failed(failure)
    }
  }
}
