#!/bin/xsh
use lib.gnu

const USAGE = """Usage: chmod [OPTION]... MODE FILE...
Change the mode of each FILE to MODE.

  -c, --changes          like verbose but report only when a change is made
  -f, --silent, --quiet  suppress most error messages
  -v, --verbose          output a diagnostic for every file processed
  -R, --recursive        change files and directories recursively
      --reference=RFILE  use RFILE's mode instead of MODE values
  -h, --no-dereference   affect symbolic links instead of referenced files
      --preserve-root    fail to operate recursively on '/'
      --no-preserve-root do not treat '/' specially (default)
      --help             display this help and exit
      --version          output version information and exit
"""

type ChmodOptions = {recursive: Bool, changes: Bool, quiet: Bool, verbose: Bool, reference: Str?, no_dereference: Bool, dereference: Bool, follow_root: Bool, follow_all: Bool, follow_none: Bool, preserve_root: Bool, no_preserve_root: Bool, help: Bool, version: Bool, operands: List[Str]}
type ChmodResult = {failed: Bool, changed: Bool}
type ModePaths = {mode: Str, paths: List[Str]}

pure mode_candidate(arg: Str) -> Bool {
  return false when ! arg.starts_with("-") or arg.starts_with("--") or arg == "-"
  return false when arg == "-R" or arg == "-c" or arg == "-f" or arg == "-v" or arg == "-h" or arg == "-H" or arg == "-L" or arg == "-P"

  let suffix = arg.byte_slice(1)
  for ch in suffix {
    return true when ch in "01234567rwxXstugoa,=+-"
  }
  false
}

pure normalize_modes(argv: List[Str]) -> List[Str] {
  var out: List[Str] = []
  var index = 0
  var options = true

  while index < argv.len() {
    let arg = argv[index]
    if options and arg == "--" {
      options = false
      out += [arg]
      index += 1
    } else if mode_candidate(arg) {
      out += [f"\u{1}chmod-mode:{arg}\u{1}"]
      index += 1
    } else {
      out += [arg]
      index += 1
    }
  }

  out
}

pure is_mode_placeholder(arg: Str) -> Bool {
  arg.starts_with("\u{1}chmod-mode:")
}

pure placeholder_mode(arg: Str) -> Str {
  arg.replace("\u{1}chmod-mode:", "").replace("\u{1}", "")
}

pure is_mode_text(arg: Str) -> Bool {
  return true when rx"^[+-]?[0-7]+$".matches(arg)
  return true when rx"^[ugoa]*[+=-][rwxXstugo]*([,+-][rwxXstugo]*)*$".matches(arg)
  false
}

pure mode_and_paths(operands: List[Str], has_reference: Bool) -> ModePaths {
  var mode = ""
  var paths: List[Str] = []
  var mode_index: Int? = null
  if ! has_reference and operands.len() > 0 {
    if is_mode_placeholder(operands[0]) or is_mode_text(operands[0]) {
      mode_index = 0
    } else {
      for index in range(operands.len()) {
        if is_mode_placeholder(operands[index]) { mode_index = index; break }
      }
      if mode_index == null { mode_index = 0 }
    }
  } else if has_reference and operands.len() > 1 and is_mode_text(operands[0]) {
    mode_index = 0
  }

  for index in range(operands.len()) {
    let arg = operands[index]
    if is_mode_placeholder(arg) {
      let negative = placeholder_mode(arg)
      if mode == "" {
        mode = negative
      } else {
        mode = f"{mode},{negative}"
      }
    } else if mode_index == index {
      mode = arg
    } else {
      paths += [arg]
    }
  }

  {mode: mode, paths: paths}
}

pure class_name(who: Str) -> Str {
  if who == "" or "a" in who { "ugo" } else { who }
}

pure selected_classes(who: Str) -> Str {
  class_name(who)
}

pure class_bits(who: Str) -> Int {
  let classes = class_name(who)
  var bits = 0
  if "u" in classes { bits = bits.bit_or(0o4700) }
  if "g" in classes { bits = bits.bit_or(0o2070) }
  if "o" in classes { bits = bits.bit_or(0o1007) }
  bits
}

pure allowed_permission_bits(who: Str, mask: Int) -> Int {
  let classes = class_name(who)
  var selected = 0
  if "u" in classes { selected = selected.bit_or(0o700) }
  if "g" in classes { selected = selected.bit_or(0o070) }
  if "o" in classes { selected = selected.bit_or(0o007) }
  selected.bit_and(0o777 - mask.bit_and(0o777))
}

pure source_permissions(mode: Int, source: Str) -> Int {
  if source == "u" { mode / 0o100 % 8 } else { if source == "g" { mode / 0o10 % 8 } else { mode % 8 } }
}

pure copy_bits(mode: Int, who: Str, source: Str) -> Int {
  let classes = class_name(who)
  let value = source_permissions(mode, source)
  var bits = 0
  if "u" in classes { bits = bits.bit_or(value * 0o100) }
  if "g" in classes { bits = bits.bit_or(value * 0o10) }
  if "o" in classes { bits = bits.bit_or(value) }
  bits
}

pure symbolic_mode(spec: Str, current: Int, is_dir: Bool, umask: Int) -> Int? {
  var mode = current.bit_and(0o7777)
  let clauses = spec.split(",")
  return null when clauses.len() == 0

  for clause in clauses {
    var at = 0
    var who = ""
    while at < clause.byte_len() and clause.byte_slice(at, length: 1) in "ugoa" {
      who = f"{who}{clause.byte_slice(at, length: 1)}"
      at += 1
    }
    return null when at >= clause.byte_len()
    let op = clause.byte_slice(at, length: 1)
    return null when op != "+" and op != "-" and op != "="
    at += 1
    let perms = clause.byte_slice(at)
    let implicit = who == ""
    let classes = if who == "" { "ugo" } else { class_name(who) }
    var bits = 0
    var copy: Str? = null
    for perm in perms {
      if copy != null and ! (perm in "ugo") { return null }
      match perm {
        "r" => { if "u" in classes { bits = bits.bit_or(0o400) }; if "g" in classes { bits = bits.bit_or(0o40) }; if "o" in classes { bits = bits.bit_or(0o4) } }
        "w" => { if "u" in classes { bits = bits.bit_or(0o200) }; if "g" in classes { bits = bits.bit_or(0o20) }; if "o" in classes { bits = bits.bit_or(0o2) } }
        "x" => { if "u" in classes { bits = bits.bit_or(0o100) }; if "g" in classes { bits = bits.bit_or(0o10) }; if "o" in classes { bits = bits.bit_or(0o1) } }
        "X" => { if is_dir or current.bit_and(0o111) != 0 { if "u" in classes { bits = bits.bit_or(0o100) }; if "g" in classes { bits = bits.bit_or(0o10) }; if "o" in classes { bits = bits.bit_or(0o1) } } }
        "s" => { if "u" in classes { bits = bits.bit_or(0o4000) }; if "g" in classes { bits = bits.bit_or(0o2000) } }
        "t" => { if "o" in classes { bits = bits.bit_or(0o1000) } }
        "u" | "g" | "o" => {
          return null when copy != null or bits != 0
          copy = perm
        }
        else => return null
      }
    }
    if copy != null { bits = bits.bit_or(copy_bits(current, who, copy ?? "u")) }
    let implicit_allowed = 0o777 - umask.bit_and(0o777)
    if implicit {
      let special = bits.bit_and(0o7000)
      bits = bits.bit_and(0o777).bit_and(implicit_allowed).bit_or(special)
    }
    let cleared = if implicit { implicit_allowed.bit_or(0o7000) } else { class_bits(who) }
    if op == "=" { mode = mode.clear_bits(cleared) }
    if op == "-" { mode = mode.clear_bits(bits) } else { mode = mode.bit_or(bits) }
  }

  mode
}

pure octal_value(text: Str) -> Int? {
  return null when text == ""
  var value = 0
  for ch in text {
    return null when ! (ch in "01234567")
    value = value * 8 + (ch.byte_at(0) ?? 48) - 48
    return null when value > 4095
  }
  value
}

pure mode_for(spec: Str, current: Int, is_dir: Bool, umask: Int) -> Int? {
  let sign = spec.byte_slice(0, length: 1)
  if sign == "+" or sign == "-" {
    let value = octal_value(spec.byte_slice(1))
    if value != null { return if sign == "+" { current.bit_or(value ?? 0) } else { current.clear_bits(value ?? 0) } }
  }
  let octal = octal_value(spec)
  return octal when octal != null
  symbolic_mode(spec, current, is_dir, umask)
}

proc report_failure(target_path: Path, display_name: Str, failure: Error, quiet: Bool) [process, env, io] {
  if ! quiet { gnu.error(f"cannot access {gnu.quote(display_name)}: {gnu.strerror(failure)}") }
}

proc set_mode(target_path: Path, display_name: Str, mode_spec: Str, reference_mode: Int?, no_dereference: Bool, quiet: Bool, changes: Bool, verbose: Bool) [fs, process, env, error, io] -> ChmodResult {
  let follow = ! no_dereference
  match fs.stat(target_path, follow) {
    Err(failure) => {
      report_failure(target_path, display_name, failure, quiet)
      {failed: true, changed: false}
    }
    Ok(meta) => {
      var desired: Int? = reference_mode
      if desired == null { desired = mode_for(mode_spec, meta.mode, meta.kind == "dir", fs.umask()?) }
      if desired == null {
        if ! quiet { gnu.error(f"invalid mode: {gnu.quote_value(mode_spec)}") }
        {failed: true, changed: false}
      } else {
        let new_mode = (desired ?? 0).bit_and(0o7777)
        if no_dereference and meta.kind == "symlink" {
          {failed: false, changed: false}
        } else if let Err(failure) = fs.chmod(target_path, new_mode, follow_symlinks: follow) {
          if ! quiet { gnu.error(f"changing permissions of {gnu.quote(display_name)}: {gnu.strerror(failure)}") }
          {failed: true, changed: false}
        } else {
          let changed = meta.mode.bit_and(0o7777) != new_mode
          if (verbose or changes) and (! changes or changed) { gnu.write_text(f"mode of {gnu.quote(display_name)} changed from {meta.mode.bit_and(0o7777)} to {new_mode}\n") }
          {failed: false, changed: changed}
        }
      }
    }
  }
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: ChmodOptions = cli.applet(
    normalize_modes(argv),
    {
      gnu: {status: 1},
      recursive: {form: "-R --recursive", default: false},
      changes: {form: "-c --changes", default: false},
      quiet: {form: "-f --silent --quiet", default: false},
      verbose: {form: "-v --verbose", default: false},
      no_dereference: {form: "-h --no-dereference", default: false, conflicts: ["dereference"]},
      dereference: {form: "--dereference", default: false, conflicts: ["no_dereference"]},
      follow_root: {form: "-H", default: false, conflicts: ["follow_all", "follow_none"]},
      follow_all: {form: "-L", default: false, conflicts: ["follow_root", "follow_none"]},
      follow_none: {form: "-P", default: false, conflicts: ["follow_root", "follow_all"]},
      preserve_root: {form: "--preserve-root", default: false, conflicts: "no_preserve_root"},
      no_preserve_root: {form: "--no-preserve-root", default: false, conflicts: "preserve_root"},
      reference: {form: "--reference RFILE"},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      operands: {form: "...ARG"},
    },
  )?

  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("chmod"); return }

  let {mode, paths} = mode_and_paths(opts.operands, opts.reference != null)
  if (opts.reference == null and paths.len() == 0) or (opts.reference != null and paths.len() == 0) { gnu.missing_operand() }
  if opts.reference == null and mode == "" { gnu.missing_operand() }

  var reference_mode: Int? = null
  if let reference = opts.reference {
    match fs.stat(fp"{reference}", true) {
      Ok(meta) => reference_mode = meta.mode.bit_and(0o7777)
      Err(failure) => { gnu.error(f"cannot stat {gnu.quote(reference)}: {gnu.strerror(failure)}"); exit 1 }
    }
  }

  var failed = false
  let root_stat = fs.stat(p"/", true)?
  for name in paths {
    let target = fp"{name}"
    let root_follow = ! opts.no_dereference and (! opts.recursive or opts.dereference or opts.follow_root or opts.follow_all)
    let meta = fs.stat(target, root_follow)
    if let Err(failure) = meta {
      if ! opts.quiet { gnu.error(f"cannot access {gnu.quote(name)}: {gnu.strerror(failure)}") }
      failed = true
      continue
    }
    let root = meta?.dev == root_stat.dev and meta?.ino == root_stat.ino
    if opts.recursive and root and ! opts.no_preserve_root {
      gnu.error(f"it is dangerous to operate recursively on {gnu.quote(name)}{if name != "/" { " (same as '/')" } else { "" }}")
      gnu.error("use --no-preserve-root to override this failsafe")
      failed = true
      continue
    }
    if opts.recursive and meta?.kind == "dir" {
      match try { fs.walk(target, stat: true)? |> sort-by .path } {
        Err(failure) => { if ! opts.quiet { gnu.error(f"cannot access {gnu.quote(name)}: {gnu.strerror(failure)}") }; failed = true }
        Ok(entries) => {
          let walk_root = target.resolve()?
          for entry in entries {
            let follow = if opts.no_dereference { false } else if entry.path == target { root_follow } else { opts.follow_all or opts.dereference }
            let relative = entry.path.relative_to(walk_root).display()
            let shown = if relative == "." { name } else { f"{name}/{relative}" }
            let result = set_mode(entry.path, shown, mode, reference_mode, ! follow, opts.quiet, opts.changes, opts.verbose)
            failed = failed or result.failed
          }
        }
      }
    } else {
      let result = set_mode(target, name, mode, reference_mode, ! root_follow, opts.quiet, opts.changes, opts.verbose)
      failed = failed or result.failed
    }
  }

  if failed { exit 1 }
}
