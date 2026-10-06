#!/bin/xsh
use lib.gnu
use lib.fileattrs

type Changes = {add: Int, remove: Int, assigned: Int?, generation: Int?, project: Int?, recursive: Bool, verbose: Bool, quiet: Bool}

# Recursive changes skip symbolic links. Directory synchronization is meaningful
# only for directories; all other flag bits survive additive changes.
proc changed(name: Str, changes: Changes, descendant: Bool) [fs, process, env, error, io] -> Bool {
  let target = fp"{name}"
  let metadata = fs.stat(target, follow_symlinks: false)
  if let Err(failure) = metadata {
    if ! changes.quiet { gnu.error(f"{gnu.strerror(failure)} while trying to stat {name}") }
    return false
  }
  let kind = metadata?.kind
  if kind == "symlink" and descendant { return true }
  if kind not in ["file", "dir"] {
    if ! changes.quiet { gnu.error(f"Operation not supported while reading flags on {name}") }
    return false
  }
  let found = linux.file_attrs(target)
  if let Err(failure) = found {
    if ! changes.quiet { gnu.error(f"{gnu.strerror(failure)} while reading flags on {name}") }
    return false
  }
  var flags = changes.assigned ?? found?.flags.clear_bits(changes.remove).bit_or(changes.add)
  if kind != "dir" { flags = flags.clear_bits(65536) }
  if changes.verbose { gnu.write_text(f"Flags of {name} set as {fileattrs.flag_text(flags)}\n") }
  if let Err(failure) = linux.set_file_attrs(target, flags) {
    if ! changes.quiet { gnu.error(f"{gnu.strerror(failure)} while setting flags on {name}") }
    return false
  }
  if changes.generation != null {
    if changes.verbose { gnu.write_text(f"Version of {name} set as {changes.generation ?? 0}\n") }
    if let Err(failure) = linux.set_file_version(target, changes.generation ?? 0) {
      if ! changes.quiet { gnu.error(f"{gnu.strerror(failure)} while setting version on {name}") }
      return false
    }
  }
  if changes.project != null {
    if changes.verbose { gnu.write_text(f"Project of {name} set as {changes.project ?? 0}\n") }
    if let Err(failure) = linux.set_file_project(target, changes.project ?? 0) {
      if ! changes.quiet { gnu.error(f"{gnu.strerror(failure)} while setting project on {name}") }
      return false
    }
  }
  true
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  var add = 0
  var remove = 0
  var assigned: Int? = null
  var generation: Int? = null
  var project: Int? = null
  var recursive = false
  var verbose = false
  var quiet = false
  var mode_seen = false
  var additive = false
  var subtractive = false
  var at = 0
  while at < argv.len() {
    let arg = argv[at]
    if arg == "--help" { gnu.help("Usage: chattr [-RVf] [-v VERSION] [-p PROJECT] [+-=FLAGS] FILE...\nChange Linux inode flags; -R recurses, -V reports changes, -f suppresses errors.\n"); return }
    if arg == "--version" { gnu.version("chattr"); return }
    if arg == "--" { at += 1; break }
    break when ! arg.starts_with("-") and ! arg.starts_with("+") and ! arg.starts_with("=")
    let operation = arg.byte_slice(0, length: 1)
    let letters = arg.byte_slice(1)
    if operation != "-" { mode_seen = true }
    if operation == "+" { additive = true }
    if operation == "=" and assigned == null { assigned = 0 }
    for letter in letters {
      if operation == "-" {
        if letter == "R" { recursive = true; continue }
        if letter == "V" { verbose = true; continue }
        if letter == "f" { quiet = true; continue }
        if letter == "v" or letter == "p" {
          at += 1
          if at >= argv.len() { gnu.usage_error(f"option requires an argument -- '{letter}'") }
          let raw = argv[at]
          let domain = if letter == "p" { "project" } else { "version" }
          if ! rx"^(0[xX][0-9A-Fa-f]+|[0-9]+)$".matches(raw) { gnu.usage_error(f"bad {domain} - {raw}") }
          let value = (if raw.byte_len() > 1 and raw.starts_with("0") and ! raw.lower().starts_with("0x") { "0o" + raw } else { raw }).parse_int()
          if value is Err(_) or (value ?? -1) < 0 or (value ?? -1) > 4294967295 { gnu.usage_error(f"bad {domain} - {raw}") }
          if letter == "p" { project = value? } else { generation = value? }
          continue
        }
      }
      let bit = fileattrs.flag_bit(letter)
      if bit == null { gnu.usage_error(f"invalid flag {gnu.quote(letter)}") }
      mode_seen = true
      if operation == "+" { add = add.bit_or(bit ?? 0) } else if operation == "-" { subtractive = true; remove = remove.bit_or(bit ?? 0) } else { assigned = (assigned ?? 0).bit_or(bit ?? 0) }
    }
    at += 1
  }
  if at == argv.len() { gnu.missing_operand() }
  if ! mode_seen and generation == null and project == null { gnu.usage_error("Must use '-v', =, - or +") }
  if assigned != null and (additive or subtractive) { gnu.error("= is incompatible with - and +"); exit 1 }
  if add.bit_and(remove) != 0 { gnu.error("Can't both set and unset same flag."); exit 1 }
  if verbose { eprint (gnu.version_text("chattr")) }
  let changes: Changes = {add: add, remove: remove, assigned: assigned, generation: generation, project: project, recursive: recursive, verbose: verbose, quiet: quiet}
  var pending: List[Str] = argv[at..]
  var top = pending.len()
  var success = true
  while ! pending.is_empty() {
    let name = pending[0]
    pending = pending[1..]
    let descendant = top == 0
    if top > 0 { top -= 1 }
    if ! changed(name, changes, descendant) { success = false; continue }
    if recursive {
      let info = fs.stat(fp"{name}", follow_symlinks: false)
      if let Err(failure) = info { if ! quiet { fileattrs.report(name, failure) }; success = false; continue }
      continue when info?.kind != "dir"
      let children = fs.children(fp"{name}")
      if let Err(failure) = children { if ! quiet { fileattrs.report(name, failure) }; success = false; continue }
      for child in children? { pending += [f"{name}/{child.name}"] }
    }
  }
  if ! success { exit 1 }
}
