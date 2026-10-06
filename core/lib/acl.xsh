##! Typed POSIX ACL entries, binary records, mask policy and permission projection.
use perm
## Invalid ACL records and text.
export error AclError = Invalid : InvalidArgument
## POSIX access control entry categories.
export enum Tag { AclOwner, AclUser, AclGroupOwner, AclGroup, AclMask, AclOther }
## One access or default entry; only named identities carry IDs.
export type Entry = {tag: Tag, id: Int?, permissions: Int, default: Bool, conditional_execute: Bool}

pure tag_number(tag: Tag) -> Int { match tag { AclOwner => 1, AclUser => 2, AclGroupOwner => 4, AclGroup => 8, AclMask => 16, AclOther => 32 } }
pure tag_from(value: Int) -> Result[Tag, Error] {
  match value { 1 => AclOwner, 2 => AclUser, 4 => AclGroupOwner, 8 => AclGroup, 16 => AclMask, 32 => AclOther, else => Err(AclError.Invalid("unknown ACL entry tag")) }
}

## Canonical kernel order: owner, named users, group, named groups, mask, other.
export pure canonical(entries: List[Entry]) -> List[Entry] {
  var result: List[Entry] = []
  for default in [false, true] {
    for number in [1, 2, 4, 8, 16, 32] {
      var selected: List[Entry] = []
      for entry in entries { if entry.default == default and tag_number(entry.tag) == number { selected += [entry] } }
      while ! selected.is_empty() {
        var best = 0
        for index in range(selected.len()) { if (selected[index].id ?? -1) < (selected[best].id ?? -1) { best = index } }
        result += [selected[best]]
        selected = selected[0..best].extend(selected[best + 1..])
      }
    }
  }
  result
}

## Require complete, unique entries and a mask whenever a named identity occurs.
export pure validate(entries: List[Entry], allow_empty = false) -> Result[Unit, Error] {
  if entries.is_empty() and allow_empty { return Ok() }
  var keys: List[Str] = []
  for entry in entries {
    return Err(AclError.Invalid("permissions outside ACL range")) when entry.permissions < 0 or entry.permissions > 7
    let named = entry.tag == AclUser or entry.tag == AclGroup
    return Err(AclError.Invalid("invalid ACL identity")) when (named and (entry.id == null or (entry.id ?? -1) < 0 or (entry.id ?? 0) >= 4294967295)) or (! named and entry.id != null)
    let key = f"{entry.default}:{tag_number(entry.tag)}:{entry.id ?? -1}"
    return Err(AclError.Invalid("duplicate ACL entry")) when key in keys
    keys += [key]
  }
  for default in [false, true] {
    var tags: List[Tag] = []
    for entry in entries { if entry.default == default { tags += [entry.tag] } }
    continue when default and tags.is_empty()
    return Err(AclError.Invalid("ACL requires owner, group and other entries")) when AclOwner not in tags or AclGroupOwner not in tags or AclOther not in tags
    return Err(AclError.Invalid("named ACL entries require a mask")) when (AclUser in tags or AclGroup in tags) and AclMask not in tags
  }
  Ok()
}

## Decode exact Linux ACL v2 fields; default status belongs to the xattr name.
export pure decode(data: Bytes, default = false) -> Result[List[Entry], Error] {
  return Err(AclError.Invalid("invalid ACL version or length")) when data.len() < 4 or (data.len() - 4) % 8 != 0
  return Err(AclError.Invalid("unsupported ACL version")) when bytes.unpack_le(data[0..4], 4)? != 2
  var entries: List[Entry] = []
  for index in range((data.len() - 4) / 8) {
    let at = 4 + index * 8
    let tag = tag_from(bytes.unpack_le(data[at..at + 2], 2)?)?
    let id = bytes.unpack_le(data[at + 4..at + 8], 4)?
    entries += [{tag: tag, id: if id == 4294967295 { null } else { id }, permissions: bytes.unpack_le(data[at + 2..at + 4], 2)?, default: default, conditional_execute: false}]
  }
  # Default records are validated as a complete standalone ACL.
  let validation = collect { for entry in entries { yield {tag: entry.tag, id: entry.id, permissions: entry.permissions, default: false, conditional_execute: false} } }
  validate(validation)?
  entries
}

## Encode one complete access or default ACL after validation, without side effects.
export pure encode(entries: List[Entry]) -> Result[Bytes, Error] {
  for entry in entries { return Err(AclError.Invalid("conditional execute requires inode mode resolution")) when entry.conditional_execute }
  for entry in entries { return Err(AclError.Invalid("cannot encode mixed access and default ACLs")) when entry.default != entries[0].default }
  let validation = collect { for entry in entries { yield {tag: entry.tag, id: entry.id, permissions: entry.permissions, default: false, conditional_execute: false} } }
  validate(validation)?
  var chunks: List[Bytes] = [bytes.pack_le(2, 4)?]
  for entry in canonical(entries) { chunks += [bytes.pack_le(tag_number(entry.tag), 2)?, bytes.pack_le(entry.permissions, 2)?, bytes.pack_le(entry.id ?? 4294967295, 4)?] }
  bytes.concat(chunks)
}

## Parse rwx positional permissions, symbolic subsets, or a single octal digit.
export pure parse_permissions(text: Str) -> Result[Int, Error] {
  if rx"^[0-7]$".matches(text) { return text.parse_int()? }
  return Err(AclError.Invalid("invalid ACL permissions")) when text == "" or ! rx"^[rwxX-]+$".matches(text)
  var value = 0
  for ch in text { value = value.bit_or(if ch == "r" { 4 } else if ch == "w" { 2 } else if ch in ["x", "X"] { 1 } else { 0 }) }
  value
}

## Convert permission bits to stable rwx columns.
export pure permissions(value: Int) -> Str { (if value.bit_and(4) != 0 { "r" } else { "-" }) + (if value.bit_and(2) != 0 { "w" } else { "-" }) + (if value.bit_and(1) != 0 { "x" } else { "-" }) }

## Parse text records before mutation, resolving named users and groups natively.
export proc parse(text: Str, remove = false, defaults = false) [fs, error] -> Result[List[Entry], Error] {
  var entries: List[Entry] = []
  for line in text.replace(",", with: "\n").lines() {
    let raw = line.split("#")[0].trim()
    continue when raw == ""
    var parts = raw.split(":")
    var default = defaults
    if parts[0] in ["d", "default"] { default = true; parts = parts[1..] }
    return Err(AclError.Invalid("invalid ACL text entry")) when parts.len() < 2 or parts.len() > 3 or (! remove and parts.len() != 3)
    let identity = parts[1].trim()
    let kind = parts[0].trim()
    var tag: Tag = AclOther
    var id: Int? = null
    if kind in ["u", "user"] { tag = if identity == "" { AclOwner } else { AclUser }; if identity != "" { id = perm.uid(identity)? } } else if kind in ["g", "group"] { tag = if identity == "" { AclGroupOwner } else { AclGroup }; if identity != "" { id = perm.gid(identity)? } } else if kind in ["m", "mask"] and identity == "" { tag = AclMask } else if kind in ["o", "other"] and identity == "" { tag = AclOther } else { return Err(AclError.Invalid("invalid ACL tag or identity")) }
    entries += [{tag: tag, id: id, permissions: if parts.len() == 3 { parse_permissions(parts[2].trim())? } else { 0 }, default: default, conditional_execute: parts.len() == 3 and "X" in parts[2]}]
  }
  entries
}

## Infer the initial access ACL from inode mode when no extended ACL is present.
export pure from_mode(mode: Int, default = false) -> List[Entry] { [
  {tag: AclOwner, id: null, permissions: mode / 64 % 8, default: default, conditional_execute: false},
  {tag: AclGroupOwner, id: null, permissions: mode / 8 % 8, default: default, conditional_execute: false},
  {tag: AclOther, id: null, permissions: mode % 8, default: default, conditional_execute: false},
] }

## Replace matching entries, retaining unrelated identities and scopes.
export pure merge(entries: List[Entry], updates: List[Entry], remove = false) -> List[Entry] {
  var result = entries
  for update in updates {
    result = collect { for entry in result { yield entry when entry.tag != update.tag or entry.id != update.id or entry.default != update.default } }
    if ! remove { result += [update] }
  }
  canonical(result)
}

## Recalculate masks from group class permissions, including named users.
export pure calculate_mask(entries: List[Entry], force = false) -> List[Entry] {
  var result = entries
  for default in [false, true] {
    var bits = 0
    var needed = force
    var present = false
    for entry in entries {
      continue when entry.default != default
      present = true
      if entry.tag in [AclGroupOwner, AclGroup, AclUser] { bits = bits.bit_or(entry.permissions) }
      if entry.tag in [AclGroup, AclUser, AclMask] { needed = true }
    }
    if present and needed { result = merge(result, [{tag: AclMask, id: null, permissions: bits, default: default, conditional_execute: false}]) }
  }
  result
}

## Project an access ACL onto owner, masked group, and other mode bits.
export pure mode(entries: List[Entry]) -> Result[Int, Error] {
  validate(entries)?
  var owner = 0; var group_bits = 0; var other = 0; var mask: Int? = null
  for entry in entries {
    continue when entry.default
    if entry.tag == AclOwner { owner = entry.permissions }
    if entry.tag == AclGroupOwner { group_bits = entry.permissions }
    if entry.tag == AclOther { other = entry.permissions }
    if entry.tag == AclMask { mask = entry.permissions }
  }
  owner * 64 + (mask ?? group_bits) * 8 + other
}

## Read access/default records with missing attributes distinguished from failures.
export proc read(target: Path) [fs, error] -> Result[List[Entry], Error] {
  let stat = fs.stat(target, follow_symlinks: true)?
  var entries: List[Entry] = []
  for default in [false, true] {
    let found = fs.xattr_get(target, if default { "system.posix_acl_default" } else { "system.posix_acl_access" })
    if let Ok(payload) = found { entries += decode(payload, default: default)? } else if let Err(failure) = found {
      if failure.errno == 61 or failure.errno == 93 { if ! default { entries += from_mode(stat.mode) } } else { return Err(failure) }
    }
  }
  canonical(entries)
}

## Validate both records and the directory constraint before making any syscall.
export proc store(target: Path, entries: List[Entry]) [fs, error] -> Result[Unit, Error] {
  validate(entries)?
  let stat = fs.stat(target, follow_symlinks: true)?
  let access = collect { for entry in entries { yield entry when ! entry.default } }
  let defaults = collect { for entry in entries { yield entry when entry.default } }
  return Err(AclError.Invalid("only directories can have a default ACL")) when ! defaults.is_empty() and stat.kind != "dir"
  let payload = encode(access)?
  let default_payload = if defaults.is_empty() { b"" } else { encode(defaults)? }
  fs.xattr_set(target, "system.posix_acl_access", payload)?
  if ! defaults.is_empty() { fs.xattr_set(target, "system.posix_acl_default", default_payload)? } else if "system.posix_acl_default" in fs.xattr_list(target)? { fs.xattr_remove(target, "system.posix_acl_default")? }
  Ok()
}

## Resolve display identities, falling back to numeric IDs only when no account exists.
export proc identity(id: Int, is_group: Bool, numeric: Bool) [fs] -> Str {
  if ! numeric {
    if is_group { if let Ok(account) = group.by_gid(id) { return account.name } } else { if let Ok(account) = user.by_uid(id) { return account.name } }
  }
  f"{id}"
}

## Render one entry with a reversible numeric or native account qualifier.
export proc text(entry: Entry, numeric = false) [fs] -> Str {
  let kind = match entry.tag { AclOwner => "user", AclUser => "user", AclGroupOwner => "group", AclGroup => "group", AclMask => "mask", AclOther => "other" }
  let name = if let id = entry.id { identity(id, entry.tag == AclGroup, numeric) } else { "" }
  (if entry.default { "default:" } else { "" }) + kind + ":" + name + ":" + permissions(entry.permissions)
}

## Group-class permissions are limited by the mask in the same ACL scope.
export pure effective(entry: Entry, entries: List[Entry]) -> Int {
  if entry.tag not in [AclGroupOwner, AclGroup, AclUser] { return entry.permissions }
  for mask in entries { return entry.permissions.bit_and(mask.permissions) when mask.tag == AclMask and mask.default == entry.default }
  entry.permissions
}

type Visit = {name: Str, top: Bool, ancestors: List[Str]}

## Follow policy affects top-level links separately from recursive descendants.
export proc visit_names(names: List[Str], recursive: Bool, traversal: Str) [fs, error] -> Result[List[Str], Error] {
  var pending: List[Visit] = collect { for name in names { yield {name: name, top: true, ancestors: []} } }
  var result: List[Str] = []
  while ! pending.is_empty() {
    let visit = pending[0]; pending = pending[1..]
    let target = fp"{visit.name}"
    var stat = fs.stat(target, follow_symlinks: false)?
    if stat.kind == "symlink" {
      continue when traversal == "P" or (! visit.top and traversal != "L")
      stat = fs.stat(target, follow_symlinks: true)?
    }
    let key = f"{stat.dev}:{stat.ino}"
    return Err(AclError.Invalid(f"{visit.name}: directory cycle detected")) when recursive and stat.kind == "dir" and key in visit.ancestors
    result += [visit.name]
    if recursive and stat.kind == "dir" {
      let children = fs.children(target)?
      let next: List[Visit] = collect { for child in children { yield {name: f"{visit.name}/{child.name}", top: false, ancestors: visit.ancestors.extend([key])} } }
      pending = next.extend(pending)
    }
  }
  result
}

## Resolve conditional execute from the original inode, before merging updates.
export pure resolve_execute(entries: List[Entry], mode: Int, directory: Bool) -> List[Entry] {
  collect { for entry in entries { yield {tag: entry.tag, id: entry.id, permissions: if entry.conditional_execute and ! directory and mode.bit_and(0o111) == 0 { entry.permissions.clear_bits(1) } else { entry.permissions }, default: entry.default, conditional_execute: false} } }
}
