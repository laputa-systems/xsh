#!/bin/xsh
use lib.gnu
use lib.fileattrs
use lib.acl
use lib.perm

enum ActionKind { Modify, Remove, Replace, StripExtended, StripDefault }
type Action = {kind: ActionKind, entries: List[acl.Entry]}
type Work = {name: Str, actions: List[Action], mask_policy: Str, recursive: Bool, traversal: Str, testing: Bool}
type Restore = {name: Str, entries: List[acl.Entry], uid: Int?, gid: Int?, special: Int}

proc apply(target: Path, actions: List[Action], mask_policy: Str, testing: Bool, recursive = false) [fs, process, io, error] -> Result[Unit, Error] {
  let stat = fs.stat(target, follow_symlinks: true)?
  var entries = acl.read(target)?
  var access_mask = false; var default_mask = false; var access_changed = false; var default_changed = false
  for action in actions {
    let updates = acl.resolve_execute(action.entries, stat.mode, stat.kind == "dir")
    for entry in updates { if entry.default { default_changed = true } else { access_changed = true }; if entry.tag == acl.AclMask { if entry.default { default_mask = action.kind != Remove } else { access_mask = action.kind != Remove } } }
    match action.kind {
      StripExtended => {
        access_changed = true; default_changed = true; access_mask = false; default_mask = false
        var base: List[acl.Entry] = []
        for entry in entries { if ! entry.default and entry.tag in [acl.AclOwner, acl.AclGroupOwner, acl.AclOther] { base += [{tag: entry.tag, id: entry.id, permissions: acl.effective(entry, entries), default: false, conditional_execute: false}] } }
        entries = base
      }
      StripDefault => { default_changed = true; default_mask = false; entries = collect { for entry in entries { yield entry when ! entry.default } } }
      Replace => {
        var access_replaced = false; var default_replaced = false
        for entry in updates { if entry.default { default_replaced = true } else { access_replaced = true } }
        entries = collect { for entry in entries { yield entry when (entry.default and ! default_replaced) or (! entry.default and ! access_replaced) } }.extend(updates)
      }
      Modify => { entries = acl.merge(entries, updates) }
      Remove => { entries = acl.merge(entries, updates, remove: true) }
    }
    # A new default ACL inherits missing base entries from the access ACL.
    var has_default = false
    for entry in entries { if entry.default { has_default = true; break } }
    if has_default {
      for tag in [acl.AclOwner, acl.AclGroupOwner, acl.AclOther] {
        var present = false
        for entry in entries { if entry.default and entry.tag == tag { present = true; break } }
        if ! present { for entry in entries { if ! entry.default and entry.tag == tag { entries += [{tag: tag, id: null, permissions: entry.permissions, default: true, conditional_execute: false}]; break } } }
      }
    }
  }
  if recursive and stat.kind != "dir" { entries = collect { for entry in entries { yield entry when ! entry.default } } }
  # A missing mask starts with the owning group's permissions even with -n.
  for default in [false, true] {
    var named = false; var has_mask = false; var group_permissions = 0
    for entry in entries {
      continue when entry.default != default
      if entry.tag in [acl.AclUser, acl.AclGroup] { named = true }
      if entry.tag == acl.AclMask { has_mask = true }
      if entry.tag == acl.AclGroupOwner { group_permissions = entry.permissions }
    }
    if named and ! has_mask { entries += [{tag: acl.AclMask, id: null, permissions: group_permissions, default: default, conditional_execute: false}] }
  }
  if mask_policy != "none" {
    let calculated = acl.calculate_mask(entries, force: mask_policy == "force")
    var explicit: List[acl.Entry] = []
    for entry in entries { if entry.tag == acl.AclMask and ((entry.default and default_mask) or (! entry.default and access_mask)) { explicit += [entry] } }
    entries = collect { for entry in calculated { yield entry when (! entry.default and access_changed) or (entry.default and default_changed) } }.extend(collect { for entry in entries { yield entry when (! entry.default and ! access_changed) or (entry.default and ! default_changed) } })
    if mask_policy != "force" { entries = acl.merge(entries, explicit) }
  }
  acl.validate(entries)?
  if testing {
    var access: List[Str] = []; var defaults: List[Str] = []
    for entry in acl.canonical(entries) { if entry.default { defaults += [acl.text(entry).replace("default:", with: "d:").replace("user:", with: "u:").replace("group:", with: "g:").replace("mask:", with: "m:").replace("other:", with: "o:")] } else { access += [acl.text(entry).replace("default:", with: "d:").replace("user:", with: "u:").replace("group:", with: "g:").replace("mask:", with: "m:").replace("other:", with: "o:")] } }
    gnu.write_text(target.display() + ": " + access.join(",") + "," + (if defaults.is_empty() { "*" } else { defaults.join(",") }) + "\n")
    return Ok()
  }
  acl.store(target, entries)?
  Ok()
}

# Restore input is parsed and validated in full before its first inode changes.
proc parse_restore(text: Str) [fs, error] -> Result[List[Restore], Error] {
  var records: List[Restore] = []
  var name: Str? = null; var lines: List[Str] = []
  var uid: Int? = null; var gid: Int? = null; var special = 0
  for line in text.lines().extend(["# file: "]) {
    if line.starts_with("# file: ") {
      if let previous = name {
        let entries = acl.parse(lines.join("\n"))?
        acl.validate(entries)?
        for default in [false, true] {
          let selected = collect { for entry in entries { yield entry when entry.default == default } }
          if ! selected.is_empty() { let encoded = acl.encode(selected)? }
        }
        records += [{name: previous, entries: entries, uid: uid, gid: gid, special: special}]
      }
      name = if line == "# file: " { null } else { fileattrs.unquote_name(line.byte_slice(8))? }
      lines = []; uid = null; gid = null; special = 0
    } else if line.starts_with("# owner: ") { uid = perm.uid(line.byte_slice(9))? } else if line.starts_with("# group: ") { gid = perm.gid(line.byte_slice(9))? } else if line.starts_with("# flags: ") {
      let flags = line.byte_slice(9)
      return Err(acl.AclError.Invalid("invalid restore flags")) when ! rx"^[s-][s-][t-]$".matches(flags)
      special = (if flags.byte_slice(0, length: 1) == "s" { 0o4000 } else { 0 }) + (if flags.byte_slice(1, length: 1) == "s" { 0o2000 } else { 0 }) + (if flags.byte_slice(2, length: 1) == "t" { 0o1000 } else { 0 })
    } else if line.trim() != "" and ! line.starts_with("#") {
      return Err(acl.AclError.Invalid("restore record has no file header")) when name == null
      lines += [line]
    }
  }
  return Err(acl.AclError.Invalid("no ACL restore records")) when records.is_empty()
  records
}

proc restore_record(target: Path, record: Restore) [fs, error] -> Result[Unit, Error] {
  acl.store(target, record.entries)?
  if record.uid != null or record.gid != null { fs.set_owner(target, uid: record.uid, gid: record.gid)? }
  target.chmod(acl.mode(record.entries)? + record.special)?
  Ok()
}

proc main(...argv: List[Str]) [fs, process, env, io, error] {
  var actions: List[Action] = []; var names: List[Str] = []; var works: List[Work] = []; var saw_files = false
  var mask_policy = "auto"; var defaults = false; var recursive = false; var traversal = "H"; var testing = false
  var restore: Str? = null; var stopped = false; var at = 0
  var words: List[Str] = []
  var literal = false; var argument = false
  for word in argv {
    if argument { words += [word]; argument = false; continue }
    if word in ["-m", "-M", "-x", "-X", "--modify", "--remove", "--modify-file", "--remove-file", "--set", "--set-file", "--restore"] { words += [word]; argument = true; continue }
    if word == "--" { literal = true; words += [word]; continue }
    if ! literal and word.starts_with("-") and ! word.starts_with("--") and word.byte_len() > 2 {
      var index = 1
      while index < word.byte_len() {
        let ch = word.byte_slice(index, length: 1)
        if ch in "mxMX" { words += ["-" + ch + word.byte_slice(index + 1)]; if index + 1 == word.byte_len() { argument = true }; break }
        words += ["-" + ch]; index += 1
      }
    } else { words += [word] }
  }
  while at < words.len() {
    let word = words[at]; at += 1
    if stopped or word == "-" or ! word.starts_with("-") {
      if actions.is_empty() { gnu.usage_error("no ACL operation specified") }
      names += [word]; works += [{name: word, actions: actions, mask_policy: mask_policy, recursive: recursive, traversal: traversal, testing: testing}]; saw_files = true; continue
    }
    if word == "--" { stopped = true; continue }
    if saw_files { actions = []; saw_files = false }
    if word in ["--help", "-h"] { gnu.help("Usage: setfacl [-RLPdn] [-m ACL|-x ACL|-M FILE|-X FILE] FILE...\n       setfacl --set=ACL FILE... | --set-file=FILE FILE...\n       setfacl [-b|-k] FILE... | --restore=FILE\n--mask recalculates masks; -n preserves masks; -d selects default entries; --test displays results without mutation.\n"); return }
    if word in ["--version", "-v"] { gnu.version("setfacl"); return }
    var option = word; var attached: Str? = null
    if let equal = word.find("=") { option = word.byte_slice(0, length: equal); attached = word.byte_slice(equal + 1) }
    if option.starts_with("--") {
      let known = ["--modify", "--remove", "--modify-file", "--remove-file", "--set", "--set-file", "--restore", "--remove-all", "--remove-default", "--no-mask", "--mask", "--default", "--recursive", "--logical", "--physical", "--test"]
      let candidates = collect { for candidate in known { yield candidate when candidate.starts_with(option) } }
      if candidates.len() == 1 { option = candidates[0] } else if option not in known { gnu.usage_error(f"invalid or ambiguous option {word}") }
    } else if option.byte_len() > 2 and option.byte_slice(1, length: 1) in "mxMX" { attached = option.byte_slice(2); option = option.byte_slice(0, length: 2) }
    if option in ["-m", "--modify", "-x", "--remove", "-M", "--modify-file", "-X", "--remove-file", "--set", "--set-file", "--restore"] {
      var value = attached ?? ""
      if attached == null { if at >= words.len() { gnu.usage_error(f"{option} requires an argument") }; value = words[at]; at += 1 }
      if option == "--restore" { restore = value; continue }
      let remove = option in ["-x", "--remove", "-X", "--remove-file"]
      let source = if option in ["-M", "--modify-file", "-X", "--remove-file", "--set-file"] { gnu.read_operand(value)?.utf8()? } else { value }
      let entries = acl.parse(source, remove: remove, defaults: defaults)
      if let Err(failure) = entries { gnu.error(failure.message); exit 2 }
      actions += [{kind: if remove { .Remove } else if option in ["--set", "--set-file"] { .Replace } else { .Modify }, entries: entries?}]
      continue
    }
    if attached != null { gnu.usage_error(f"{option} does not take an argument") }
    let flags = if option.starts_with("--") { [option] } else { collect { for ch in option.byte_slice(1) { yield "-" + ch } } }
    for flag in flags {
      match flag {
        "-b" | "--remove-all" => actions += [{kind: .StripExtended, entries: []}]
        "-k" | "--remove-default" => actions += [{kind: .StripDefault, entries: []}]
        "-n" | "--no-mask" => mask_policy = "none"
        "--mask" => mask_policy = "force"
        "-d" | "--default" => defaults = true
        "-R" | "--recursive" => recursive = true
        "-L" | "--logical" => traversal = "L"
        "-P" | "--physical" => traversal = "P"
        "--test" => testing = true
        else => gnu.usage_error(f"invalid option {flag}")
      }
    }
  }
  if let input = restore {
    if ! actions.is_empty() or ! names.is_empty() or recursive { gnu.usage_error("--restore cannot be combined with ACL operations or filenames") }
    let records = parse_restore(gnu.read_operand(input)?.utf8()?)
    if let Err(failure) = records { gnu.error(failure.message); exit 1 }
    var success = true
    for record in records? {
      let target = fp"{record.name}"
      # Ownership precedes special bits, since ownership changes may clear them.
      let result = if testing { apply(target, [{kind: .Replace, entries: record.entries}], "none", true) } else { restore_record(target, record) }
      if let Err(failure) = result { fileattrs.report(record.name, failure); success = false }
    }
    if ! success { exit 1 }; return
  }
  if works.is_empty() and actions.is_empty() { gnu.usage_error("no ACL operation specified") }
  if names.is_empty() { gnu.missing_operand() }
  var success = true
  for work in works {
    let expanded = if work.name == "-" { gnu.read_operand("-")?.utf8()?.lines() } else { [work.name] }
    for name in expanded {
      let visited = acl.visit_names([name], work.recursive, work.traversal)
      if let Err(failure) = visited { fileattrs.report(name, failure); success = false; continue }
      for item in visited? { if let Err(failure) = apply(fp"{item}", work.actions, work.mask_policy, work.testing, recursive: work.recursive) { fileattrs.report(item, failure); success = false } }
    }
  }
  if ! success { exit 1 }
}
