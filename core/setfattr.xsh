#!/bin/xsh
use lib.gnu
use lib.fileattrs

type Options = {name: Str?, remove: Str?, value: Str?, restore: Str?, raw: Bool, nofollow: Bool, help: Bool, version: Bool, files: List[Str]}

proc apply(name: Str, attribute: Str, value: Bytes, remove: Bool, follow: Bool) [fs, process, error] -> Bool {
  let target = fp"{name}"
  if remove {
    if let Err(failure) = fs.xattr_remove(target, attribute, follow_symlinks: follow) { fileattrs.report(name, failure); return false }
  } else {
    if let Err(failure) = fs.xattr_set(target, attribute, value, follow_symlinks: follow) { fileattrs.report(name, failure); return false }
  }
  true
}

# Dump entries are applied in source order; a failed entry does not suppress
# later operands. An invalid encoding is rejected before its attribute changes.
proc restore_dump(data: Bytes, follow: Bool, raw: Bool) [fs, process, error] -> Bool {
  var name: Str? = null
  var success = true
  var at = 0
  while at < data.len() {
    let start = at
    while at < data.len() and data.byte_at(at) != 10 { at += 1 }
    let line = data[start..at]
    at += 1
    continue when line.is_empty()
    if line.len() >= 8 and line[0..8] == b"# file: " {
      let header = line[8..].utf8()
      if let Err(failure) = header { gnu.error(failure.message); return false }
      let decoded = fileattrs.unquote_name(header?)
      if let Err(failure) = decoded { gnu.error(failure.message); return false }
      name = decoded?
      continue
    }
    continue when line.byte_at(0) == 35
    if name == null { gnu.error("No filename found in restore input, aborting"); return false }
    var equal = -1
    for index in range(line.len()) { if line.byte_at(index) == 61 { equal = index; break } }
    let raw_name = (if equal >= 0 { line[0..equal] } else { line }).utf8()
    if let Err(failure) = raw_name { gnu.error(failure.message); return false }
    let attribute = fileattrs.unquote_name(raw_name?)
    if let Err(failure) = attribute { gnu.error(failure.message); return false }
    let value: Result[Bytes, Error] = if equal >= 0 {
      if raw { Ok(line[equal + 1..]) } else { fileattrs.decode_bytes(line[equal + 1..]) }
    } else { Ok(b"") }
    if let Err(failure) = value { gnu.error(failure.message); success = false; continue }
    if ! apply(name ?? "", attribute?, value?, false, follow) { success = false }
  }
  success
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 2},
    name: {form: "-n --name NAME", conflicts: ["remove", "restore"]},
    remove: {form: "-x --remove NAME", conflicts: ["name", "value", "restore"]},
    value: {form: "-v --value VALUE", conflicts: ["remove", "restore"]},
    restore: {form: "--restore FILE", conflicts: ["name", "remove", "value"]},
    raw: {form: "--raw", default: false},
    nofollow: {form: "-h --no-dereference", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    files: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: setfattr {-n NAME [-v VALUE] | -x NAME} [-h] FILE...\n       setfattr --restore=FILE\n  --raw  treat VALUE as unencoded bytes\n"); return }
  if opts.version { gnu.version("setfattr"); return }
  if opts.restore != null {
    if ! opts.files.is_empty() { gnu.extra_operand(opts.files[0], status: 2) }
    let input = gnu.read_operand(opts.restore ?? "")
    if let Err(failure) = input { fileattrs.report(opts.restore ?? "", failure); exit 1 }
    if ! restore_dump(input?, ! opts.nofollow, opts.raw) { exit 1 }
    return
  }
  if opts.name == null and opts.remove == null { gnu.usage_error("must specify --name or --remove", status: 2) }
  if opts.files.is_empty() { gnu.missing_operand(status: 2) }
  let attribute = fileattrs.unquote_name(opts.name ?? opts.remove ?? "")
  if let Err(failure) = attribute { gnu.error(failure.message); exit 1 }
  let value = if opts.raw { Ok(bytes.from_text(opts.value ?? "")) } else { fileattrs.decode(opts.value ?? "") }
  if let Err(failure) = value { gnu.error("bad input encoding"); exit 1 }
  var success = true
  for name in opts.files { if ! apply(name, attribute?, value?, opts.remove != null, ! opts.nofollow) { success = false } }
  if ! success { exit 1 }
}
