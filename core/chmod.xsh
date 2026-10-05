#!/bin/xsh
use lib.gnu
use lib.perm

error ModeError = Invalid : Usage

pure octal(raw: Str) -> Result[Int] {
  return Err(ModeError.Invalid("invalid mode")) when ! rx"^[0-7]+$".matches(raw)
  var value = 0
  for digit in raw { value = value * 8 + digit.parse_int()? }
  return Err(ModeError.Invalid("invalid mode")) when value > 0o7777
  value
}

pure permission_bits(perms: Str, who: Str, mode: Int, directory: Bool) -> Result[Int] {
  var bits = 0
  var copied = 0
  if perms == "u" { copied = mode / 64 % 8 } else if perms == "g" { copied = mode / 8 % 8 } else if perms == "o" { copied = mode % 8 } else {
    for p in perms {
      match p {
        "r" => copied = copied.bit_or(4)
        "w" => copied = copied.bit_or(2)
        "x" => copied = copied.bit_or(1)
        "X" => if directory or mode.bit_and(0o111) != 0 { copied = copied.bit_or(1) }
        "s" => { if "u" in who { bits = bits.bit_or(0o4000) }; if "g" in who { bits = bits.bit_or(0o2000) } }
        "t" => if "o" in who { bits = bits.bit_or(0o1000) } else => return Err(ModeError.Invalid("invalid mode"))
      }
    }
  }
  if "u" in who { bits = bits.bit_or(copied * 64) }
  if "g" in who { bits = bits.bit_or(copied * 8) }
  if "o" in who { bits = bits.bit_or(copied) }
  bits
}

# Omitted classes honor umask; each action sees the previous action's result.
# Directory set-ID bits survive ordinary assignment unless explicitly named.
pure mode_for(spec: Str, current: Int, directory: Bool, umask: Int) -> Result[Int] {
  if rx"^[+=-]?[0-9]+$".matches(spec) {
    let op = spec.byte_slice(0, length: 1)
    if op in "+-=" {
      let value = octal(spec.byte_slice(1))?
      if op == "+" { return current.bit_and(0o7777).bit_or(value) }
      if op == "-" { return current.bit_and(0o7777).clear_bits(value) }
      return value
    }
    let numeric = octal(spec)?
    return numeric.bit_or(current.bit_and(0o6000)) when directory and spec.byte_len() < 5
    return numeric
  }
  var mode = current.bit_and(0o7777)
  for clause in spec.split(",") {
    var at = 0
    var who = ""
    while at < clause.byte_len() and clause.byte_slice(at, length: 1) in "ugoa" {
      who = f"{who}{clause.byte_slice(at, length: 1)}"
      at += 1
    }
    let omitted = who == ""
    if omitted or "a" in who { who = "ugo" }
    return Err(ModeError.Invalid("invalid mode")) when at >= clause.byte_len()
    while at < clause.byte_len() {
      let op = clause.byte_slice(at, length: 1)
      return Err(ModeError.Invalid("invalid mode")) when ! (op in "+-=")
      at += 1
      let begin = at
      while at < clause.byte_len() and ! (clause.byte_slice(at, length: 1) in "+-=") { at += 1 }
      let permissions = clause.byte_slice(begin, length: at - begin)
      var bits = permission_bits(permissions, who, mode, directory)?
      var mask = 0
      if "u" in who { mask = mask.bit_or(0o4700) }
      if "g" in who { mask = mask.bit_or(0o2070) }
      if "o" in who { mask = mask.bit_or(0o1007) }
      if directory and ! ("s" in permissions) { mask = mask.clear_bits(0o6000) }
      if omitted { bits = bits.clear_bits(umask) }
      if op == "+" { mode = mode.bit_or(bits) } else if op == "-" { mode = mode.clear_bits(bits) } else { mode = mode.clear_bits(mask).bit_or(bits) }
    }
  }
  mode
}

pure octal_text(mode: Int) -> Str { f"0{mode / 512 % 8}{mode / 64 % 8}{mode / 8 % 8}{mode % 8}" }

pure symbolic_text(mode: Int) -> Str {
  var text = ""
  for shift in [64, 8, 1] {
    let bits = mode / shift % 8
    text = f"{text}{if bits.bit_and(4) != 0 { "r" } else { "-" }}{if bits.bit_and(2) != 0 { "w" } else { "-" }}"
    let special = if shift == 64 { mode.bit_and(0o4000) != 0 } else if shift == 8 { mode.bit_and(0o2000) != 0 } else { mode.bit_and(0o1000) != 0 }
    let execute = bits.bit_and(1) != 0
    let mark = if special { if shift == 1 { if execute { "t" } else { "T" } } else { if execute { "s" } else { "S" } } } else { if execute { "x" } else { "-" } }
    text = f"{text}{mark}"
  }
  text
}

proc change(target: Path, spec: Str, reference: Int?, opts: perm.Options, top: Bool, ancestors: List[Str], umask: Int) [fs, error, process, env, io] -> Bool {
  let link = fs.stat(target, follow_symlinks: false)
  if let Err(failure) = link { if ! opts.quiet { gnu.cannot_access(f"{target}", failure) }; return false }
  let is_link = link?.kind == "symlink"
  let traverse = opts.traversal == "L" or (top and opts.traversal == "H")
  if is_link and ((opts.recursive and ! traverse) or ! opts.dereference) {
    if opts.verbosity == "verbose" { gnu.write_text(f"neither symbolic link {gnu.quote(f"{target}")} nor referent has been changed\n") }
    return true
  }
  let found = fs.stat(target, follow_symlinks: opts.dereference)
  if let Err(failure) = found {
    if ! opts.quiet {
      if is_link and (failure.errno ?? gnu.errno(failure)) == 2 {
        gnu.error(f"cannot operate on dangling symlink {gnu.quote(f"{target}")}")
      } else { gnu.cannot_access(f"{target}", failure) }
    }
    return false
  }
  let meta = found?
  var success = true
  let before = meta.mode.bit_and(0o7777)
  let after = if let bits = reference { bits } else { mode_for(spec, before, meta.kind == "dir", umask)? }
  if opts.recursive and meta.kind == "dir" {
    let key = f"{meta.dev}:{meta.ino}"
    if key in ancestors { gnu.error(f"cycle detected at {gnu.quote(f"{target}")}"); return false }
    if opts.preserve_root {
      let root = fs.stat(p"/", follow_symlinks: true)?
      if root.dev == meta.dev and root.ino == meta.ino { gnu.error(f"it is dangerous to operate recursively on {gnu.quote(f"{target}")}"); return false }
    }
    # Grant requested access before descent, and defer removals until children
    # have been visited so the directory remains searchable throughout.
    let added = after.clear_bits(before).bit_and(0o777)
    if added != 0 {
      if let Err(failure) = target.chmod(before.bit_or(added)) {
        if ! opts.quiet { gnu.error(f"changing permissions of {gnu.quote(f"{target}")}: {gnu.strerror(failure)}") }
        return false
      }
    }
    match fs.children(target) {
      Ok(children) => for child in children { if ! change(child.path, spec, reference, opts, false, ancestors + [key], umask) { success = false } }
      Err(failure) => { if ! opts.quiet { gnu.cannot("read directory", f"{target}", failure) }; success = false }
    }
  }
  match target.chmod(after, follow_symlinks: opts.dereference) {
    Err(failure) => { if ! opts.quiet { gnu.error(f"changing permissions of {gnu.quote(f"{target}")}: {gnu.strerror(failure)}") }; return false }
    Ok(_) => {}
  }
  if opts.verbosity == "verbose" or (opts.verbosity == "changes" and before != after) {
    let message = if before == after { f"mode of {gnu.quote(f"{target}")} retained as {octal_text(before)} ({symbolic_text(before)})" } else { f"mode of {gnu.quote(f"{target}")} changed from {octal_text(before)} ({symbolic_text(before)}) to {octal_text(after)} ({symbolic_text(after)})" }
    gnu.write_text(f"{message}\n")
  }
  if reference == null and opts.option_like_mode {
    let naive = mode_for(spec, before, meta.kind == "dir", 0)?
    if after.clear_bits(naive) != 0 {
      if ! opts.quiet { gnu.error(f"{gnu.quote_maybe(f"{target}")}: new permissions are {symbolic_text(after)}, not {symbolic_text(naive)}") }
      success = false
    }
  }
  success
}

proc main(...argv: List[Str]) [fs, error, process, env, io] {
  let opts = perm.options(argv, chmod: true)?
  if opts.help { gnu.help("Usage: chmod [OPTION]... MODE FILE...\n  -R, --recursive\n  -c, --changes\n  -v, --verbose\n  -f, --silent\n      --reference=RFILE\n      --preserve-root\n      --no-preserve-root\n      --help\n      --version"); return }
  if opts.version { gnu.version("chmod"); return }
  if opts.operands.is_empty() { gnu.missing_operand() }
  var spec = ""
  var reference: Int? = null
  var targets = opts.operands
  let umask = fs.umask()?
  if let file = opts.reference {
    match fs.stat(fp"{file}", follow_symlinks: true) {
      Ok(meta) => reference = meta.mode.bit_and(0o7777)
      Err(failure) => { gnu.cannot("stat", file, failure); exit 1 }
    }
  } else {
    spec = opts.operands[0]
    if mode_for(spec, 0, false, umask) is Err(_) { gnu.usage_error(f"invalid mode: {gnu.quote(spec)}") }
    targets = opts.operands[1..]
    if targets.is_empty() { gnu.missing_operand_after(spec) }
  }
  var success = true
  for target in targets { if ! change(fp"{target}", spec, reference, opts, true, [], umask) { success = false } }
  if ! success { exit 1 }
}
