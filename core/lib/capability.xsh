##! Validated Linux file capability records and reusable capability names.
## Invalid capability names, text or binary records.
export error CapabilityError = Invalid : InvalidArgument
## Linux file capability encoding revisions.
export enum Revision { One, Two, Three }
## Unsigned 64-bit bitmap stored as two uint32 words.
export type Mask = {low: Int, high: Int}
## One fully decoded capability xattr, including namespace root identity.
export type FileCaps = {revision: Revision, effective: Bool, permitted: Mask, inheritable: Mask, rootid: Int?}
## A capability name and its bit index.
export type CapabilityName = {name: Str, bit: Int}

## Kernel capability names, shared by file and process capability policy.
export const CAPABILITIES: List[CapabilityName] = [
  {name: "cap_chown", bit: 0},
  {name: "cap_dac_override", bit: 1},
  {name: "cap_dac_read_search", bit: 2},
  {name: "cap_fowner", bit: 3},
  {name: "cap_fsetid", bit: 4},
  {name: "cap_kill", bit: 5},
  {name: "cap_setgid", bit: 6},
  {name: "cap_setuid", bit: 7},
  {name: "cap_setpcap", bit: 8},
  {name: "cap_linux_immutable", bit: 9},
  {name: "cap_net_bind_service", bit: 10},
  {name: "cap_net_broadcast", bit: 11},
  {name: "cap_net_admin", bit: 12},
  {name: "cap_net_raw", bit: 13},
  {name: "cap_ipc_lock", bit: 14},
  {name: "cap_ipc_owner", bit: 15},
  {name: "cap_sys_module", bit: 16},
  {name: "cap_sys_rawio", bit: 17},
  {name: "cap_sys_chroot", bit: 18},
  {name: "cap_sys_ptrace", bit: 19},
  {name: "cap_sys_pacct", bit: 20},
  {name: "cap_sys_admin", bit: 21},
  {name: "cap_sys_boot", bit: 22},
  {name: "cap_sys_nice", bit: 23},
  {name: "cap_sys_resource", bit: 24},
  {name: "cap_sys_time", bit: 25},
  {name: "cap_sys_tty_config", bit: 26},
  {name: "cap_mknod", bit: 27},
  {name: "cap_lease", bit: 28},
  {name: "cap_audit_write", bit: 29},
  {name: "cap_audit_control", bit: 30},
  {name: "cap_setfcap", bit: 31},
  {name: "cap_mac_override", bit: 32},
  {name: "cap_mac_admin", bit: 33},
  {name: "cap_syslog", bit: 34},
  {name: "cap_wake_alarm", bit: 35},
  {name: "cap_block_suspend", bit: 36},
  {name: "cap_audit_read", bit: 37},
  {name: "cap_perfmon", bit: 38},
  {name: "cap_bpf", bit: 39},
  {name: "cap_checkpoint_restore", bit: 40},
]

pure power(bit: Int) -> Int { var value = 1; for index in range(bit) { value *= 2 }; value }
pure empty() -> Mask { {low: 0, high: 0} }
pure union(a: Mask, b: Mask) -> Mask { {low: a.low.bit_or(b.low), high: a.high.bit_or(b.high)} }
pure minus(a: Mask, b: Mask) -> Mask { {low: a.low.clear_bits(b.low), high: a.high.clear_bits(b.high)} }
pure equal(a: Mask, b: Mask) -> Bool { a == b }
pure contains(mask: Mask, bit: Int) -> Bool { if bit < 32 { mask.low.bit_and(power(bit)) != 0 } else { mask.high.bit_and(power(bit - 32)) != 0 } }
pure singleton(bit: Int) -> Mask { if bit < 32 { {low: power(bit), high: 0} } else { {low: 0, high: power(bit - 32)} } }

## Resolve a known capability name or an explicit numeric bit in the ABI range.
export pure bit(name: Str) -> Result[Int, Error] {
  let normalized = name.lower()
  for entry in CAPABILITIES { return entry.bit when normalized == entry.name or normalized == entry.name.byte_slice(4) }
  if rx"^[0-9]+$".matches(normalized) {
    let value = normalized.parse_int()?
    return value when value >= 0 and value < 64
  }
  Err(CapabilityError.Invalid(f"unknown capability {name}"))
}

## Parse a comma-separated set without losing the upper half of a file bitmap.
export pure parse_mask(text: Str) -> Result[Mask, Error] {
  if text.lower() == "all" { return {low: 4294967295, high: 511} }
  var mask = empty()
  return mask when text == ""
  for name in text.split(",") { mask = union(mask, singleton(bit(name)?)) }
  mask
}

## Process policy uses the named kernel capabilities, which fit a signed integer.
export pure parse_set(text: Str) -> Result[Int, Error] {
  let mask = parse_mask(text)?
  return Err(CapabilityError.Invalid("capability set exceeds signed integer range")) when mask.high >= 2147483648
  mask.low + mask.high * 4294967296
}

## Render a bitmap in numeric order, retaining unknown future capability bits.
export pure format_mask(mask: Mask) -> Str {
  var names: List[Str] = []
  for index in range(64) {
    continue when ! contains(mask, index)
    var name = f"{index}"
    for entry in CAPABILITIES { if entry.bit == index { name = entry.name; break } }
    names += [name]
  }
  names.join(",")
}

## Render the process bitmap using the same names as file capability records.
export pure format_set(mask: Int) -> Str { format_mask({low: mask % 4294967296, high: mask / 4294967296}) }

## Select an explicit on-disk revision.
export pure revision(value: Int) -> Result[Revision, Error] {
  if value == 1 { .One } else if value == 2 { .Two } else if value == 3 { .Three } else { Err(CapabilityError.Invalid("unsupported capability revision")) }
}

## Decode the exact Linux little-endian layout, rejecting unknown flag bits.
export pure decode(data: Bytes) -> Result[FileCaps, Error] {
  return Err(CapabilityError.Invalid("truncated capability record")) when data.len() < 4
  let magic = bytes.unpack_le(data[0..4], 4)?
  let rev = magic / 16777216
  return Err(CapabilityError.Invalid("invalid capability flags or length")) when magic % 16777216 > 1 or rev < 1 or rev > 3 or data.len() != (if rev == 1 { 12 } else if rev == 2 { 20 } else { 24 })
  let permitted: Mask = {low: bytes.unpack_le(data[4..8], 4)?, high: if rev > 1 { bytes.unpack_le(data[12..16], 4)? } else { 0 }}
  let inheritable: Mask = {low: bytes.unpack_le(data[8..12], 4)?, high: if rev > 1 { bytes.unpack_le(data[16..20], 4)? } else { 0 }}
  {revision: revision(rev)?, effective: magic % 2 == 1, permitted: permitted, inheritable: inheritable, rootid: if rev == 3 { bytes.unpack_le(data[20..24], 4)? } else { null }}
}

## Validate the complete record before constructing a payload for mutation.
export pure encode(record: FileCaps) -> Result[Bytes, Error] {
  for mask in [record.permitted, record.inheritable] {
    return Err(CapabilityError.Invalid("capability word outside uint32 range")) when mask.low < 0 or mask.low > 4294967295 or mask.high < 0 or mask.high > 4294967295
  }
  let rev = match record.revision { One => 1, Two => 2, Three => 3 }
  return Err(CapabilityError.Invalid("revision one cannot represent upper capability bits")) when rev == 1 and (record.permitted.high != 0 or record.inheritable.high != 0)
  return Err(CapabilityError.Invalid("invalid root ID for capability revision")) when (rev == 3 and (record.rootid == null or (record.rootid ?? -1) < 0 or (record.rootid ?? 0) > 4294967295)) or (rev != 3 and record.rootid != null)
  var chunks = [bytes.pack_le(rev * 16777216 + (if record.effective { 1 } else { 0 }), 4)?, bytes.pack_le(record.permitted.low, 4)?, bytes.pack_le(record.inheritable.low, 4)?]
  if rev > 1 { chunks += [bytes.pack_le(record.permitted.high, 4)?, bytes.pack_le(record.inheritable.high, 4)?] }
  if rev == 3 { chunks += [bytes.pack_le(record.rootid ?? 0, 4)?] }
  bytes.concat(chunks)
}

## Parse libcap clauses, then reject effective sets that cannot be stored in a file.
export pure parse(text: Str, rootid: Int? = null) -> Result[FileCaps, Error] {
  var permitted = empty()
  var inheritable = empty()
  var effective = empty()
  for clause in rx"[[:space:]]+".replace(text.trim(), with: " ").split(" ") {
    continue when clause == ""
    var at = 0
    while at < clause.byte_len() and clause.byte_slice(at, length: 1) not in "=+-" { at += 1 }
    return Err(CapabilityError.Invalid("capability clause needs =, +, or -")) when at == clause.byte_len()
    let names = clause.byte_slice(0, length: at)
    let selected = parse_mask(if names == "" { "all" } else { names })?
    var op = clause.byte_slice(at, length: 1)
    if op == "=" { permitted = minus(permitted, selected); inheritable = minus(inheritable, selected); effective = minus(effective, selected) }
    at += 1
    while at < clause.byte_len() {
      let flag = clause.byte_slice(at, length: 1)
      at += 1
      if flag in "+-" { op = flag; continue }
      return Err(CapabilityError.Invalid("invalid capability set flag")) when flag not in "eip"
      if flag == "p" { permitted = if op == "-" { minus(permitted, selected) } else { union(permitted, selected) } }
      if flag == "i" { inheritable = if op == "-" { minus(inheritable, selected) } else { union(inheritable, selected) } }
      if flag == "e" { effective = if op == "-" { minus(effective, selected) } else { union(effective, selected) } }
    }
  }
  return Err(CapabilityError.Invalid("file effective set must equal permitted union inheritable, or be empty")) when ! equal(effective, empty()) and ! equal(effective, union(permitted, inheritable))
  let record: FileCaps = {revision: if rootid == null { .Two } else { .Three }, effective: ! equal(effective, empty()), permitted: permitted, inheritable: inheritable, rootid: rootid}
  let validated = encode(record)?
  record
}

## Group equal per-capability flags using canonical libcap expression syntax.
export pure format(record: FileCaps) -> Str {
  var groups: List[Str] = []
  for flags in ["i", "p", "ip", "e", "ei", "ep", "eip"] {
    var mask = empty()
    for index in range(64) {
      let p = contains(record.permitted, index)
      let i = contains(record.inheritable, index)
      let e = record.effective and (p or i)
      let actual = (if e { "e" } else { "" }) + (if i { "i" } else { "" }) + (if p { "p" } else { "" })
      if actual == flags { mask = union(mask, singleton(index)) }
    }
    if mask != empty() { groups += [format_mask(mask) + (if groups.is_empty() { "=" } else { "+" }) + flags] }
  }
  if groups.is_empty() { "=" } else { groups.join(" ") }
}
