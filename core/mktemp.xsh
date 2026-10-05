#!/bin/xsh
use lib.gnu

type Options = {directory: Bool, dry: Bool, quiet: Bool, suffix: Str?, tmpdir: Str?, parent: Str?, legacy: Bool, help: Bool, version: Bool, templates: List[Str]}

proc create_directory(target: Path) [fs, error] -> Result[Unit] {
  let parent_root = fs.open_root(target.parent())?
  parent_root.mkdir(fp"{target.basename()}", mode: 0o700.clear_bits(fs.umask()?))
}

# Creation is rolled back if its name cannot be delivered to the caller.
proc remove_created(target: Path, name: Str, directory: Bool) [fs, process, env, error] {
  if directory {
    if let Err(failure) = target.remove_dir() { gnu.cannot("remove", name, failure) }
  } else {
    if let Err(failure) = target.remove() { gnu.cannot("remove", name, failure) }
  }
}

proc report_created(target: Path, name: Str, directory: Bool) [fs, process, env, error, io] {
  if let Err(failure) = io.write_stdout(name + "\n") {
    remove_created(target, name, directory)
    gnu.write_failed(failure)
  }
  if let Err(failure) = io.flush_stdout() {
    remove_created(target, name, directory)
    gnu.write_failed(failure)
  }
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(argv, {
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
  let pattern = if opts.templates.is_empty() { "tmp.XXXXXXXXXX" } else { opts.templates[0] }
  if opts.suffix != null and ! pattern.ends_with("X") { gnu.error(f"with --suffix, template {gnu.quote(pattern)} must end in X"); exit 1 }
  let raw = bytes.from_text(pattern)
  var end = raw.len()
  while end > 0 and raw.byte_at(end - 1) != 88 { end -= 1 }
  var start = end
  while start > 0 and raw.byte_at(start - 1) == 88 { start -= 1 }
  if end - start < 3 { gnu.error(f"too few X's in template {gnu.quote(pattern)}"); exit 1 }
  let suffix = pattern.byte_slice(end) + (opts.suffix ?? "")
  if "/" in suffix { gnu.error(f"invalid suffix {gnu.quote(suffix)}, contains directory separator"); exit 1 }
  if opts.legacy and "/" in pattern { gnu.error(f"invalid template, {gnu.quote(pattern)}, contains directory separator"); exit 1 }
  let default_dir = env.get_or("TMPDIR", "/tmp") ?? "/tmp"
  var directory = opts.parent ?? opts.tmpdir ?? ""
  var option_at = 0
  while option_at < argv.len() {
    let arg = argv[option_at]
    break when arg == "--"
    let long = arg.split("=")
    if long[0].starts_with("--") and long[0].byte_len() > 2 and "--tmpdir".starts_with(long[0]) {
      directory = if long.len() > 1 { long[1] } else { "" }
    } else if arg.starts_with("-") and ! arg.starts_with("--") {
      let at = arg.find("p") ?? -1
      if at >= 1 {
        if at + 1 < arg.byte_len() { directory = arg.byte_slice(at + 1) } else if option_at + 1 < argv.len() { option_at += 1; directory = argv[option_at] }
      }
    }
    option_at += 1
  }
  let use_dir = opts.tmpdir != null or opts.parent != null or opts.legacy or opts.templates.is_empty()
  if opts.legacy and (env.get_or("TMPDIR", "") ?? "") != "" { directory = default_dir }
  if use_dir and directory == "" { directory = if default_dir == "" { "/tmp" } else { default_dir } }
  if use_dir and pattern.starts_with("/") { gnu.error(f"invalid template, {gnu.quote(pattern)}; with --tmpdir, it may not be absolute"); exit 1 }
  let prefix = (if use_dir { directory + "/" } else { "" }) + pattern.byte_slice(0, length: start)
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
    let name = prefix + random + suffix
    let target = fp"{name}"
    if opts.dry { gnu.write_text(name + "\n"); return }
    let made = if opts.directory {
      create_directory(target)
    } else { fs.mknod(target, "file", 0o600) }
    if made is Ok(_) { report_created(target, name, opts.directory); return }
    if let Err(failure) = made {
      continue when failure.errno == 17
      if ! opts.quiet { gnu.error(f"failed to create {if opts.directory { "directory" } else { "file" }} via template {gnu.quote((if use_dir { directory + "/" } else { "" }) + pattern)}: {gnu.strerror(failure)}") }
      exit 1
    }
  }
  if ! opts.quiet { gnu.error(f"failed to create via template {gnu.quote(pattern)}: File exists") }
  exit 1
}
