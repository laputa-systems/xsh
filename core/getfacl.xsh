#!/bin/xsh
use lib.gnu
use lib.fileattrs
use lib.acl

type EntryKey = {tag: acl.Tag, id: Int?}
type Options = {access: Bool, defaults: Bool, omit: Bool, all_effective: Bool, no_effective: Bool, skip: Bool, recursive: Bool, logical: Bool, physical: Bool, tabular: Bool, absolute: Bool, numeric: Bool, help: Bool, version: Bool, files: List[Str]}

proc table(entries: List[acl.Entry], uid: Int, gid: Int, numeric: Bool) [fs, io, error] {
  var keys: List[EntryKey] = []
  var width = 8
  for entry in entries {
    let name = if entry.tag == acl.AclOwner { acl.identity(uid, false, numeric) } else if entry.tag == acl.AclGroupOwner { acl.identity(gid, true, numeric) } else if let id = entry.id { acl.identity(id, entry.tag == acl.AclGroup, numeric) } else { "" }
    if name.byte_len() > width { width = name.byte_len() }
  }
  for entry in entries {
    let key = {tag: entry.tag, id: entry.id}
    continue when key in keys
    keys += [key]
    let label = match entry.tag { acl.AclOwner => "USER", acl.AclUser => "user", acl.AclGroupOwner => "GROUP", acl.AclGroup => "group", acl.AclMask => "mask", acl.AclOther => "other" }
    let name = if entry.tag == acl.AclOwner { acl.identity(uid, false, numeric) } else if entry.tag == acl.AclGroupOwner { acl.identity(gid, true, numeric) } else if let id = entry.id { acl.identity(id, entry.tag == acl.AclGroup, numeric) } else { "" }
    var columns: List[Str] = []
    for default in [false, true] {
      var value = "   "
      for candidate in entries {
        if candidate.default == default and candidate.tag == entry.tag and candidate.id == entry.id {
          let raw = acl.permissions(candidate.permissions)
          let allowed = acl.permissions(acl.effective(candidate, entries))
          value = ""
          for at in range(3) { let ch = raw.byte_slice(at, length: 1); value += if allowed.byte_slice(at, length: 1) == "-" { ch.upper() } else { ch } }
          break
        }
      }
      columns += [value]
    }
    gnu.write_text(fileattrs.field(label, 5) + "  " + fileattrs.field(name, width) + "  " + columns[0] + "  " + columns[1] + "\n")
  }
}

proc main(...argv: List[Str]) [fs, process, env, io, error] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 2}, access: {form: "-a --access", default: false}, defaults: {form: "-d --default", default: false},
    omit: {form: "-c --omit-header", default: false}, all_effective: {form: "-e --all-effective", default: false}, no_effective: {form: "-E --no-effective", default: false},
    skip: {form: "-s --skip-base", default: false}, recursive: {form: "-R --recursive", default: false}, logical: {form: "-L --logical", default: false}, physical: {form: "-P --physical", default: false},
    tabular: {form: "-t --tabular", default: false}, absolute: {form: "-p --absolute-names", default: false}, numeric: {form: "-n --numeric", default: false},
    help: {form: "-h --help", default: false, stop: true}, version: {form: "-v --version", default: false, stop: true}, files: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: getfacl [-acdeEsRLPtpn] FILE...\n-a access ACL; -d default ACL; -c omit header; -e all effective permissions; -E no effective comments; -s skip base ACLs; -R recursive; -L/-P follow policy; -t table; -p absolute names; -n numeric IDs.\n"); return }
  if opts.version { gnu.version("getfacl"); return }
  var files = opts.files
  if files.is_empty() { gnu.missing_operand(status: 2) }
  var names: List[Str] = []
  for name in files { if name == "-" { names += gnu.read_operand("-")?.utf8()?.lines() } else { names += [name] } }
  var traversal = if opts.physical { "P" } else if opts.logical { "L" } else { "H" }
  for word in argv {
    break when word == "--"
    if word == "--logical" { traversal = "L" } else if word == "--physical" { traversal = "P" } else if word.starts_with("-") and ! word.starts_with("--") { for ch in word.byte_slice(1) { if ch == "L" { traversal = "L" } else if ch == "P" { traversal = "P" } } }
  }
  var success = true; var warned = false
  for operand in names {
    let visited = acl.visit_names([operand], opts.recursive, traversal)
    if let Err(failure) = visited { fileattrs.report(operand, failure); success = false; continue }
    for name in visited? {
      let target = fp"{name}"
      let loaded = acl.read(target)
      if let Err(failure) = loaded { fileattrs.report(name, failure); success = false; continue }
      let entries = loaded?
      if opts.skip and entries.len() == 3 { continue }
      let stat = fs.stat(target, follow_symlinks: true)?
      var shown = name
      if ! opts.absolute and shown.starts_with("/") {
        if ! warned { gnu.error("Removing leading '/' from absolute path names"); warned = true }
        while shown.starts_with("/") { shown = shown.byte_slice(1) }
      }
      if ! opts.omit {
        gnu.write_text("# file: " + fileattrs.quote_name(shown) + "\n# owner: " + acl.identity(stat.uid, false, opts.numeric) + "\n# group: " + acl.identity(stat.gid, true, opts.numeric) + "\n")
        if stat.mode.bit_and(0o7000) != 0 { gnu.write_text("# flags: " + (if stat.mode.bit_and(0o4000) != 0 { "s" } else { "-" }) + (if stat.mode.bit_and(0o2000) != 0 { "s" } else { "-" }) + (if stat.mode.bit_and(0o1000) != 0 { "t" } else { "-" }) + "\n") }
      }
      let selected = collect { for entry in entries { yield entry when (entry.default and (! opts.access or opts.defaults)) or (! entry.default and (! opts.defaults or opts.access)) } }
      if opts.tabular { table(selected, stat.uid, stat.gid, opts.numeric) } else {
        for entry in selected {
          var line = acl.text(entry, numeric: opts.numeric)
          let effective = acl.effective(entry, entries)
          if ! opts.no_effective and entry.tag in [acl.AclUser, acl.AclGroup, acl.AclGroupOwner] and (opts.all_effective or effective != entry.permissions) { line += "\t#effective:" + acl.permissions(effective) }
          gnu.write_text(line + "\n")
        }
      }
      gnu.write_text("\n")
    }
  }
  if ! success { exit 1 }
}
