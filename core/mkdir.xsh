#!/bin/xsh
use lib.gnu
use lib.selinux
error AppletError = Usage : Usage

type MkdirOptions = {parents: Bool, mode: Str?, verbose: Bool, default_context: Bool, context: List[Str], help: Bool, version: Bool, directories: List[Str]}

# --context takes an optional value, so a bare --context is recorded with this
# sentinel. No argv element can contain NUL, so it never equals a real value.
const NO_CONTEXT_VALUE = "\0"

pure parse_mode(spec: Str, umask: Int) -> Result[Int] {
  if rx"^[0-7]+$".matches(spec) {
    var value = 0
    for digit in spec {
      value = value * 8 + digit.parse_int()?
      return Err(AppletError.Usage("invalid mode")) when value > 0o7777
    }
    return value
  }
  var mode = 0o777
  for clause in spec.split(",") {
    let parsed = rx"^([ugoa]*)([-+=])([rwxXstugo]*)$".captures(clause)
    return Err(AppletError.Usage("invalid mode")) when parsed.is_empty()
    let groups = parsed
    let who = groups[1]
    let op = groups[2]
    let perms = groups[3]
    let classes = if who == "" or "a" in who { "ugo" } else { who }
    var mask = 0
    var bits = 0
    for class in classes {
      let shift = if class == "u" { 64 } else if class == "g" { 8 } else { 1 }
      mask = mask.bit_or(7 * shift)
      for perm in perms {
        if perm == "r" { bits = bits.bit_or(4 * shift) }
        if perm == "w" { bits = bits.bit_or(2 * shift) }
        if perm in ["x", "X"] { bits = bits.bit_or(shift) }
        if perm in ["u", "g", "o"] {
          let source = if perm == "u" { 64 } else if perm == "g" { 8 } else { 1 }
          bits = bits.bit_or(mode / source % 8 * shift)
        }
        if perm == "s" and class == "u" { bits = bits.bit_or(0o4000) }
        if perm == "s" and class == "g" { bits = bits.bit_or(0o2000) }
        if perm == "t" and class == "o" { bits = bits.bit_or(0o1000) }
      }
      if class == "u" { mask = mask.bit_or(0o4000) }
      if class == "g" { mask = mask.bit_or(0o2000) }
      if class == "o" { mask = mask.bit_or(0o1000) }
    }
    if who == "" { bits = bits.clear_bits(umask)
      mask = mask.clear_bits(umask) }
    if op == "+" { mode = mode.bit_or(bits) }
    if op == "-" { mode = mode.clear_bits(bits) }
    if op == "=" { mode = mode.clear_bits(mask).bit_or(bits) }
  }
  mode
}

# Path.parent normalizes trailing dots; mkdir must create the directory
# named before a literal final '.' before attempting the complete operand.
pure lexical_parent(target: Path) -> Path {
  let raw = target.display()
  var end = raw.byte_len()
  while end > 1 and raw.byte_slice(end - 1, length: 1) == "/" { end -= 1 }
  let text = raw.byte_slice(0, length: end)
  let parts = text.split("/")
  return p"." when parts.len() <= 1
  let parent = parts[0..parts.len() - 1].join("/")
  if parent == "" { p"/" } else { fp"{parent}" }
}

proc create_directory(target: Path, parents: Bool, mode: Int?, verbose: Bool, parent_mode: Int) [fs, process, env, io, error] -> Result[Unit] {
  if parents {
    match fs.stat(target, follow_symlinks: true) {
      Ok(meta) => { return when meta.kind == "dir" }
      Err(_) => {}
    }
    let parent = lexical_parent(target)
    if parent != "" and parent != target {
      create_directory(parent, true, null, verbose, parent_mode)?
    }
  }
  if let Err(failure) = target.mkdir(parents: false) {
    if parents and gnu.errno(failure) == 17 {
      if let Ok(meta) = fs.stat(target, follow_symlinks: true) {
        return when meta.kind == "dir"
      }
    }
    return Err(failure)
  }
  if let desired = mode { target.chmod(desired)? } else if parent_mode != 0 {
    let inherited = target.metadata()?.mode.bit_and(0o7777)
    if inherited.bit_and(parent_mode) != parent_mode { target.chmod(inherited.bit_or(parent_mode))? }
  }
  if verbose { gnu.write_text(f"mkdir: created directory {gnu.quote(target.display())}\n") }
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: MkdirOptions = cli.applet(argv, {
    gnu: {status: 1},
    parents: {form: "-p --parents", default: false},
    mode: {form: "-m --mode MODE"},
    verbose: {form: "-v --verbose", default: false},
    default_context: {form: "-Z", default: false},
    context: {form: "--context[=CONTEXT]", repeated: true, optional_default: NO_CONTEXT_VALUE},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    directories: {form: "...DIRECTORY"},
  })?
  var valued_contexts = 0
  for value in opts.context {
    if value != NO_CONTEXT_VALUE { valued_contexts += 1 }
  }
  let labelled = opts.default_context or valued_contexts > 0
  # Labelling created directories is not implemented, so a label request is
  # only GNU's silent or warning no-op on kernels without SELinux; on SELinux
  # kernels it must fail.
  let enabled = labelled and selinux.enabled()?
  if valued_contexts > 0 and !enabled {
    for _ in range(valued_contexts) {
      gnu.error("warning: ignoring --context; it requires an SELinux/SMACK-enabled kernel")
    }
  }
  if opts.help {
    gnu.help("Usage: mkdir [OPTION]... DIRECTORY...\nCreate directories.\n  -p, --parents  create missing parents\n  -m, --mode=MODE  set permission bits\n  -v, --verbose  report created directories\n")
    return
  }
  if opts.version { gnu.version("mkdir")
    return }
  if enabled {
    gnu.error("--context (-Z) is not supported on SELinux-enabled systems")
    exit 1
  }
  if opts.directories.is_empty() { gnu.missing_operand() }
  let umask = fs.umask()?
  var mode: Int? = null
  if let spec = opts.mode {
    match parse_mode(spec, umask) {
      Ok(value) => mode = value
      Err(_) => { gnu.error(f"invalid mode {gnu.quote_value(spec)}")
        exit 1 }
    }
  }
  var failed = false
  for name in opts.directories {
    if name == "" {
      gnu.error("cannot create directory '': No such file or directory")
      failed = true
      continue
    }
    # Missing ancestors need owner write/search permission even when the
    # caller's mask removes them; the final directory keeps its usual mode.
    let target = fp"{name}"
    if opts.parents {
      let parent = lexical_parent(target)
      if parent != "" {
        if let Err(failure) = create_directory(parent, true, null, opts.verbose, 0o300) {
          gnu.cannot("create directory", name, failure)
          failed = true
          continue
        }
      }
    }
    match create_directory(target, opts.parents, mode, opts.verbose, 0) {
      Ok(_) => {}
      Err(failure) => { gnu.cannot("create directory", name, failure)
        failed = true }
    }
  }
  if failed { exit 1 }
}
