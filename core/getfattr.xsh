#!/bin/xsh
use lib.gnu
use lib.fileattrs

type Options = {name: Str?, dump: Bool, encoding: Str, pattern: Str, values: Bool, nofollow: Bool, absolute: Bool, recursive: Bool, logical: Bool, physical: Bool, help: Bool, version: Bool, files: List[Str]}
enum WalkMode { Hybrid, Logical, Physical }
type DirectoryIdentity = {device: Int, inode: Int}
type Visit = {name: Str, ancestors: List[DirectoryIdentity], operand: Bool}
type Printed = {success: Bool, absolute_warning: Bool}

proc print_attributes(name: Str, opts: Options, pattern: Regex, warned: Bool) [fs, process, env, error, io] -> Printed {
  let target = fp"{name}"
  var names: List[Str] = []
  if opts.name != null { names = [opts.name ?? ""] } else {
    let listed = fs.xattr_list(target, follow_symlinks: ! opts.nofollow)
    if let Err(failure) = listed { fileattrs.report(name, failure); return {success: false, absolute_warning: warned} }
    for attribute in listed? { if pattern.matches(attribute) { names += [attribute] } }
    names = names |> sort |> collect
  }
  var success = true
  var header = false
  var warning = warned
  for attribute in names {
    var value = b""
    if opts.dump or opts.name != null or opts.values {
      let found = fs.xattr_get(target, attribute, follow_symlinks: ! opts.nofollow)
      if let Err(failure) = found { fileattrs.report(name, failure, attribute); success = false; continue }
      value = found?
    }
    var shown = name
    if ! opts.absolute {
      if shown.starts_with("/") {
        if ! warning { gnu.error("Removing leading '/' from absolute path names"); warning = true }
        while shown.starts_with("/") { shown = shown.byte_slice(1) }
      } else if shown.starts_with("./") {
        shown = shown.byte_slice(1)
        while shown.starts_with("/") { shown = shown.byte_slice(1) }
      }
      if shown == "" { shown = "." }
    }
    if ! opts.values and ! header {
      gnu.write_text(f"# file: {fileattrs.quote_name(shown)}\n")
      header = true
    }
    if opts.values { gnu.write_bytes(value) } else if opts.dump or opts.name != null {
      gnu.write_text(fileattrs.quote_name(attribute, attribute: true) + "=")
      gnu.write_bytes(fileattrs.encode(value, opts.encoding)?)
      gnu.write_text("\n")
    } else { gnu.write_text(fileattrs.quote_name(attribute, attribute: true) + "\n") }
  }
  if header { gnu.write_text("\n") }
  {success: success, absolute_warning: warning}
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 2},
    name: {form: "-n --name NAME"},
    dump: {form: "-d --dump", default: false},
    encoding: {form: "-e --encoding ENCODING", default: ""},
    pattern: {form: "-m --match PATTERN", default: "^user\\."},
    values: {form: "--only-values", default: false},
    nofollow: {form: "-h --no-dereference", default: false},
    absolute: {form: "--absolute-names", default: false},
    recursive: {form: "-R --recursive", default: false},
    logical: {form: "-L --logical", default: false},
    physical: {form: "-P --physical", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    files: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: getfattr [-hRLP] [-n NAME | -d] [-e ENCODING] [-m PATTERN] FILE...\n  --only-values     write raw attribute values\n  --absolute-names  retain leading '/' in names\nENCODING is text, hex, or base64; PATTERN '-' matches all attributes.\n"); return }
  if opts.version { gnu.version("getfattr"); return }
  if opts.files.is_empty() { gnu.missing_operand(status: 2) }
  if opts.encoding not in ["", "text", "hex", "base64"] { gnu.usage_error(f"invalid encoding {gnu.quote(opts.encoding)}", status: 2) }
  let compiled = regex.compile(if opts.pattern == "-" { "" } else { opts.pattern })
  if let Err(failure) = compiled { gnu.error(f"invalid regular expression {gnu.quote(opts.pattern)}"); exit 1 }
  var walk: WalkMode = if opts.physical { .Physical } else if opts.logical { .Logical } else { .Hybrid }
  var option_at = 0
  while option_at < argv.len() {
    let arg = argv[option_at]
    break when arg == "--"
    if arg.starts_with("--") {
      let parts = arg.split("=")
      let option = parts[0]
      if option.byte_len() > 2 and "--logical".starts_with(option) { walk = .Logical } else if option.byte_len() > 2 and "--physical".starts_with(option) { walk = .Physical } else if parts.len() == 1 {
        for takes_value in ["--name", "--encoding", "--match"] {
          if option.byte_len() > 2 and takes_value.starts_with(option) { option_at += 1; break }
        }
      }
    } else if arg.starts_with("-") {
      var short_at = 1
      while short_at < arg.byte_len() {
        let letter = arg.byte_slice(short_at, length: 1)
        if letter == "L" { walk = .Logical } else if letter == "P" { walk = .Physical }
        if letter in "nem" {
          if short_at + 1 == arg.byte_len() { option_at += 1 }
          break
        }
        short_at += 1
      }
    }
    option_at += 1
  }
  var pending: List[Visit] = []
  for name in opts.files { pending += [{name: name, ancestors: [], operand: true}] }
  var success = true
  var warned = false
  while ! pending.is_empty() {
    let visit = pending[0]
    pending = pending[1..]
    let target = fp"{visit.name}"
    let metadata = fs.stat(target, follow_symlinks: false)
    if let Err(failure) = metadata { fileattrs.report(visit.name, failure); success = false; continue }
    var info = metadata?
    if info.kind == "symlink" {
      continue when walk == .Physical or (! visit.operand and walk != .Logical)
      let follow = walk == .Logical or (visit.operand and ! opts.nofollow)
      if follow {
        let resolved = fs.stat(target, follow_symlinks: true)
        if let Err(failure) = resolved { fileattrs.report(visit.name, failure); success = false; continue }
        info = resolved?
      }
    }
    let identity: DirectoryIdentity = {device: info.dev, inode: info.ino}
    if opts.recursive and info.kind == "dir" and identity in visit.ancestors {
      gnu.error(f"{fileattrs.quote_name(visit.name)}: directory cycle detected")
      success = false
      continue
    }
    let printed = print_attributes(visit.name, opts, compiled?, warned)
    if ! printed.success { success = false }
    warned = printed.absolute_warning
    continue when ! opts.recursive or info.kind != "dir"
    let children = fs.children(target)
    if let Err(failure) = children { fileattrs.report(visit.name, failure); success = false; continue }
    var descendants: List[Visit] = []
    for child in children? { descendants += [{name: f"{visit.name}/{child.name}", ancestors: visit.ancestors.extend([identity]), operand: false}] }
    pending = descendants.extend(pending)
  }
  if ! success { exit 1 }
}
