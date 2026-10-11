#!/bin/xsh
use lib.gnu

type Options = {directory: Bool, dry: Bool, quiet: Bool, suffix: Str?, tmpdir: Str?, parent: Str?, legacy: Bool, help: Bool, version: Bool, templates: List[Str]}
type RawArgument = {marker: Str, value: Bytes}
type PreparedArguments = {text: List[Str], raw: List[RawArgument]}

pure prepare_arguments(argv: List[Bytes]) -> PreparedArguments {
  var text: List[Str] = []
  var raw: List[RawArgument] = []
  for index in range(argv.len()) {
    let argument = argv[index]
    match argument.utf8() {
      Ok(value) => text += [value]
      Err(_) => {
        let marker = f"\0mktemp-raw-argument-{index}\0"
        if argument.starts_with(b"--suffix=") {
          text += [f"--suffix={marker}"]
          raw += [{marker: marker, value: argument[9..]}]
        } else if argument.starts_with(b"--tmpdir=") {
          text += [f"--tmpdir={marker}"]
          raw += [{marker: marker, value: argument[9..]}]
        } else {
          text += [marker]
          raw += [{marker: marker, value: argument}]
        }
      }
    }
  }
  {text: text, raw: raw}
}

pure directory_option(argv: List[Bytes], initial: Bytes) -> Bytes {
  var directory = initial
  var at = 0
  while at < argv.len() {
    let arg = argv[at]
    break when arg == b"--"
    if arg.starts_with(b"--") {
      var equals = 0
      while equals < arg.len() {
        break when arg.byte_at(equals) == 61
        equals += 1
      }
      let name = arg[0..equals].utf8() ?? ""
      if name.starts_with("--") and name.byte_len() > 2 and "--tmpdir".starts_with(name) {
        directory = if equals < arg.len() { arg[equals + 1..] } else { b"" }
      }
    } else if arg.starts_with(b"-") {
      for index in range(1, arg.len()) {
        if arg.byte_at(index) == 112 {
          if index + 1 < arg.len() {
            directory = arg[index + 1..]
          } else if at + 1 < argv.len() {
            at += 1
            directory = argv[at]
          }
          break
        }
      }
    }
    at += 1
  }
  directory
}

pure contains_separator(value: Bytes) -> Bool {
  for index in range(value.len()) { if value.byte_at(index) == 47 { return true } }
  false
}

proc create_directory(target: Path) [fs, error] -> Result[Unit] {
  let parent_root = fs.open_root(target.parent())?
  let components = target.components()
  parent_root.mkdir(components[components.len() - 1], mode: 0o700.clear_bits(fs.umask()?))
}

# Creation is rolled back if its name cannot be delivered to the caller.
proc remove_created(target: Path, name: Bytes, directory: Bool) [fs, process, env, error] {
  if directory {
    if let Err(failure) = target.remove_dir() { gnu.error(f"cannot remove {gnu.quote_bytes(name)}: {gnu.strerror(failure)}") }
  } else {
    if let Err(failure) = target.remove() { gnu.error(f"cannot remove {gnu.quote_bytes(name)}: {gnu.strerror(failure)}") }
  }
}

proc report_created(target: Path, name: Bytes, directory: Bool) [fs, process, env, error, io] {
  let output = bytes.concat([name, b"\n"])
  if let Err(failure) = io.write_stdout_bytes(output) {
    remove_created(target, name, directory)
    gnu.write_failed(failure)
  }
  if let Err(failure) = io.flush_stdout() {
    remove_created(target, name, directory)
    gnu.write_failed(failure)
  }
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  let prepared = prepare_arguments(argv)
  let opts: Options = cli.applet(prepared.text, {
    gnu: {status: 1},
    directory: {form: "-d --directory", default: false},
    dry: {form: "-u --dry-run", default: false},
    quiet: {form: "-q --quiet", default: false},
    suffix: {form: "--suffix SUFF"},
    tmpdir: {form: "--tmpdir[=DIR]", optional_default: ""},
    parent: {form: "-p DIR"},
    legacy: {form: "-t", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    templates: {form: "...TEMPLATE"},
  })?
  if opts.help { gnu.help("Usage: mktemp [OPTION]... [TEMPLATE]\nCreate a temporary file or directory, safely, and print its name.\n  -d, --directory\n  -u, --dry-run\n  -q, --quiet\n  --suffix=SUFF\n  --tmpdir[=DIR]\n  -p DIR\n  -t\n"); return }
  if opts.version { gnu.version("mktemp"); return }
  if opts.templates.len() > 1 { gnu.usage_error("too many templates") }
  let pattern = if opts.templates.is_empty() { b"tmp.XXXXXXXXXX" } else { gnu.argument_bytes(opts.templates[0], prepared.raw) }
  let option_suffix = if opts.suffix == null { b"" } else { gnu.argument_bytes(opts.suffix ?? "", prepared.raw) }
  if opts.suffix != null and !pattern.ends_with(b"X") { gnu.error(f"with --suffix, template {gnu.quote_bytes(pattern)} must end in X"); exit 1 }
  var end = pattern.len()
  while end > 0 and pattern.byte_at(end - 1) != 88 { end -= 1 }
  var start = end
  while start > 0 and pattern.byte_at(start - 1) == 88 { start -= 1 }
  if end - start < 3 { gnu.error(f"too few X's in template {gnu.quote_bytes(pattern)}"); exit 1 }
  let suffix = bytes.concat([pattern[end..], option_suffix])
  if contains_separator(suffix) { gnu.error(f"invalid suffix {gnu.quote_bytes(suffix)}, contains directory separator"); exit 1 }
  if opts.legacy and contains_separator(pattern) { gnu.error(f"invalid template, {gnu.quote_bytes(pattern)}, contains directory separator"); exit 1 }
  # An empty fallback preserves raw TMPDIR bytes and shows whether -t should override -p.
  let configured_tmpdir = env.path("TMPDIR", p"")?.bytes()
  let has_tmpdir = !configured_tmpdir.is_empty()
  let default_dir = if has_tmpdir { configured_tmpdir } else { b"/tmp" }
  var directory = if opts.parent != null { gnu.argument_bytes(opts.parent ?? "", prepared.raw) } else if opts.tmpdir != null { gnu.argument_bytes(opts.tmpdir ?? "", prepared.raw) } else { b"" }
  directory = directory_option(argv, directory)
  let use_dir = opts.tmpdir != null or opts.parent != null or opts.legacy or opts.templates.is_empty()
  if opts.legacy and has_tmpdir { directory = default_dir }
  if use_dir and directory.is_empty() { directory = if default_dir.is_empty() { b"/tmp" } else { default_dir } }
  if use_dir and pattern.starts_with(b"/") { gnu.error(f"invalid template, {gnu.quote_bytes(pattern)}; with --tmpdir, it may not be absolute"); exit 1 }
  let path_prefix = if use_dir { bytes.concat([directory, b"/"]) } else { b"" }
  let prefix = bytes.concat([path_prefix, pattern[0..start]])
  let alphabet = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
  for attempt in range(256) {
    var random = ""
    while random.byte_len() < end - start {
      let entropy = bytes.read_at(p"/dev/urandom", 0, end - start)
      if let Err(failure) = entropy { gnu.error(f"cannot get random bytes: {gnu.strerror(failure)}"); exit 1 }
      for index in range(end - start) {
        let byte = entropy?.byte_at(index) ?? 0
        continue when byte >= 248
        random += alphabet.byte_slice(byte % 62, length: 1)
        break when random.byte_len() == end - start
      }
    }
    let name = bytes.concat([prefix, bytes.from_text(random), suffix])
    let target = Path.parse_bytes(name)?
    if opts.dry { gnu.write_bytes(bytes.concat([name, b"\n"])); return }
    let made = if opts.directory {
      create_directory(target)
    } else { fs.mknod(target, "file", 0o600) }
    if made is Ok(_) { report_created(target, name, opts.directory); return }
    if let Err(failure) = made {
      continue when failure.errno == 17
      if ! opts.quiet { gnu.error(f"failed to create {if opts.directory { "directory" } else { "file" }} via template {gnu.quote_bytes(bytes.concat([path_prefix, pattern]))}: {gnu.strerror(failure)}") }
      exit 1
    }
  }
  if ! opts.quiet { gnu.error(f"failed to create via template {gnu.quote_bytes(pattern)}: File exists") }
  exit 1
}
