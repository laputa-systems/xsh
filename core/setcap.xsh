#!/bin/xsh
use lib.gnu
use lib.fileattrs
use lib.capability

type Operation = {name: Str, remove: Bool, record: capability.FileCaps?}
proc main(...argv: List[Str]) [fs, process, env, io, error] {
  var verify = false; var quiet = false; var force = false
  var rootid: Int? = null
  var operations: List[Operation] = []
  var at = 0
  while at < argv.len() {
    let word = argv[at]; at += 1
    if word in ["-h", "--help"] { gnu.help("Usage: setcap [-q] [-v] [-n ROOTID] [-f] CAPABILITIES FILE...\n       setcap [-q] [-v] -r FILE...\n-v verifies without modifying files; -r removes the capability attribute.\n"); return }
    if word == "-v" { verify = true; continue }
    if word == "-q" { quiet = true; continue }
    if word == "-f" { force = true; continue }
    if word == "-n" {
      if at >= argv.len() { gnu.usage_error("-n requires a root ID") }
      let value = argv[at].parse_int(); at += 1
      if value is Err(_) or (value ?? -1) < 0 or (value ?? 0) > 4294967295 { gnu.usage_error("invalid root ID") }
      rootid = value?; continue
    }
    if at >= argv.len() { gnu.usage_error("capability expression requires a filename") }
    let name = argv[at]; at += 1
    if word == "-r" { operations += [{name: name, remove: true, record: null}] } else {
      if word.starts_with("-") and word != "-" { gnu.usage_error(f"invalid option {word}") }
      let raw = if word == "-" { gnu.read_operand("-")?.utf8()?.trim() } else { word }
      let parsed = capability.parse(raw, rootid: rootid)
      if let Err(failure) = parsed { gnu.error(failure.message); exit 1 }
      operations += [{name: name, remove: false, record: parsed?}]
    }
  }
  if operations.is_empty() { gnu.missing_operand() }
  var success = true
  for operation in operations {
    let target = fp"{operation.name}"
    let metadata = fs.stat(target, follow_symlinks: false)
    if let Err(failure) = metadata { if ! quiet { fileattrs.report(operation.name, failure) }; success = false; continue }
    if metadata?.kind != "file" { if ! quiet { gnu.error(f"{operation.name}: not a regular file") }; success = false; continue }
    if verify {
      let found = fs.xattr_get(target, "security.capability", follow_symlinks: false)
      var matches = false
      if let Err(failure) = found {
        if failure.errno == 61 or failure.errno == 93 { matches = operation.remove or (operation.record != null and capability.format(operation.record ?? capability.parse("=")?) == "=") } else { if ! quiet { fileattrs.report(operation.name, failure) }; success = false; continue }
      } else {
        let decoded = capability.decode(found?)
        if let Err(failure) = decoded { if ! quiet { fileattrs.report(operation.name, failure) }; success = false; continue }
        if let wanted = operation.record {
          let actual = decoded?
          matches = actual.permitted == wanted.permitted and actual.inheritable == wanted.inheritable and actual.effective == wanted.effective and (wanted.rootid == null or actual.rootid == wanted.rootid)
        }
      }
      if ! quiet { gnu.write_text(operation.name + (if matches { ": OK\n" } else { ": differs\n" })) }
      if ! matches { success = false }
      continue
    }
    let result = if operation.remove { fs.xattr_remove(target, "security.capability", follow_symlinks: false) } else {
      guard let record = operation.record else { gnu.usage_error("missing capability record"); exit 1 }
      fs.xattr_set(target, "security.capability", capability.encode(record)?, follow_symlinks: false)
    }
    if let Err(failure) = result { if ! quiet { fileattrs.report(operation.name, failure) }; if ! (force and operation.remove and (failure.errno == 61 or failure.errno == 93)) { success = false } }
  }
  if ! success { exit 1 }
}
