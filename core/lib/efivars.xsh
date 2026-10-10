##! EFI global variables through efivarfs: attribute-prefixed variable files,
##! boot load options, device path nodes and 16-bit boot-order lists.
use gnu

## The vendor namespace that owns Boot####, Driver####, SysPrep####, their
## order lists, BootNext, BootCurrent and Timeout.
export const GLOBAL_GUID = "8be4df61-93ca-11d2-aa0d-00e098032b8c"

## Non-volatile, boot-service and runtime access: what firmware expects of
## boot configuration variables.
export const BOOT_ATTRIBUTES = 7

## Load option attribute: the entry is eligible for automatic boot.
export const LOAD_OPTION_ACTIVE = 1

## Load option attribute: reconnect all drivers after this driver loads.
export const LOAD_OPTION_FORCE_RECONNECT = 2

# The inode flag efivarfs sets on every variable file; the kernel refuses
# writes and unlinks until it is cleared.
const FS_IMMUTABLE = 16

const VARIABLE_FILE = rx"^[^/-]+-[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$"
const VARIABLE_NAME = rx"^[A-Za-z0-9]+$"
const HEX_DIGITS = rx"^[0-9A-Fa-f]+$"

## A validated variable directory. Only `open_store` builds one, so every write
## below lands in a directory that was checked to be efivarfs-shaped.
export type Store = {dir: Path}

## One variable as efivarfs presents it: the attribute word and the payload.
export type Variable = {name: Str, attributes: Int, data: Bytes}

## A list-kind variable family: entries are `prefix` plus four hex digits and
## `order` names the 16-bit list that ranks them.
export type Family = {prefix: Str, order: Str, label: Str}

## Boot entries ranked by BootOrder.
export const BOOT: Family = {prefix: "Boot", order: "BootOrder", label: "Boot"}
## Driver entries ranked by DriverOrder.
export const DRIVER: Family = {prefix: "Driver", order: "DriverOrder", label: "Driver"}
## System-preparation entries ranked by SysPrepOrder.
export const SYSPREP: Family = {prefix: "SysPrep", order: "SysPrepOrder", label: "SysPrep"}

## One device path node: `kind` and `subtype` as the specification numbers
## them and the payload without the four-byte node header.
export type Node = {kind: Int, subtype: Int, data: Bytes}

## A decoded load option: attribute word, description, device path and the
## optional data that follows the path.
export type LoadOption = {attributes: Int, description: Str, path: List[Node], data: Bytes}

## Hex digits of `value`, at least `width` wide, in the requested case.
export pure hex(value: Int, width: Int, upper: Bool = false) -> Str {
  var number = value
  var text = ""
  let digits = if upper { "0123456789ABCDEF" } else { "0123456789abcdef" }
  while number > 0 { text = digits.byte_slice(number % 16, 1) + text; number /= 16 }
  while text.byte_len() < width { text = "0" + text }
  text
}

## Parses strictly hexadecimal digits, with an optional `0x` prefix.
export pure parse_hex(text: Str) -> Int? {
  let digits = if text.starts_with("0x") or text.starts_with("0X") { text.byte_slice(2) } else { text }
  return null when ! HEX_DIGITS.matches(digits) or digits.byte_len() > 15

  let value = ("0x" + digits).parse_int()
  if let Ok(number) = value { return number }

  null
}

## Space-separated lowercase hex of every byte.
export pure hex_bytes(data: Bytes) -> Str {
  var words: List[Str] = []
  for index in range(data.len()) { words += [hex(data.byte_at(index) ?? 0, 2)] }
  words.join(" ")
}

## Opens `dir` for variable access. Unless `any_dir` is set the directory must
## look like efivarfs: only regular files named NAME-GUID. A mistaken
## variable directory such as `/etc` is refused before anything is written.
export proc open_store(dir: Path, any_dir: Bool) [fs, error] -> Result[Store, Error] {
  let info = fs.stat(dir)?
  guard info.kind == "dir" else { return Err(error.failure(f"{dir} is not a directory")) }
  if ! any_dir {
    for child in fs.children(dir)? {
      guard child.kind == "file" and VARIABLE_FILE.matches(child.name) else {
        return Err(error.failure(f"{dir} is not an efivarfs directory: unexpected entry {child.name}"))
      }
    }
  }
  Ok({dir: dir})
}

pure variable_path(store: Store, name: Str) -> Result[Path, Error] {
  guard VARIABLE_NAME.matches(name) else { return Err(error.failure(f"invalid EFI variable name {name}")) }
  Ok(fp"{store.dir}/{name}-{GLOBAL_GUID}")
}

## Names of the variables in the global namespace, sorted.
export proc variable_names(store: Store) [fs, error] -> Result[List[Str], Error] {
  let suffix = "-" + GLOBAL_GUID
  var names: List[Str] = []
  for child in fs.children(store.dir)? {
    continue when ! child.name.lower().ends_with(suffix)
    names += [child.name.byte_slice(0, child.name.byte_len() - suffix.byte_len())]
  }
  Ok(names |> sort-by .)
}

## Reads one global variable; `null` means the variable does not exist.
export proc read_variable(store: Store, name: Str) [fs, error] -> Result[Variable?, Error] {
  let file = variable_path(store, name)?
  return Ok(null) when ! file.exists()

  let raw = file.read_bytes()?
  guard raw.len() >= 4 else { return Err(error.failure(f"variable {name} is shorter than its attribute prefix")) }
  Ok({name: name, attributes: bytes.unpack_le(raw, 4)?, data: raw.slice(4)})
}

# Filesystems without inode flags (a fixture directory) answer ENOTTY or
# EOPNOTSUPP and have no immutable flag to clear. Every other failure is
# surfaced.
proc clear_immutable(file: Path) [process, error] -> Result[Bool, Error] {
  let found = linux.file_attrs(file)
  if let Err(failure) = found {
    return Ok(false) when gnu.errno(failure) in [25, 38, 95]

    return Err(failure)
  }
  let flags = found?.flags
  return Ok(false) when flags.bit_and(FS_IMMUTABLE) == 0

  linux.set_file_attrs(file, flags.clear_bits(FS_IMMUTABLE))?
  Ok(true)
}

proc restore_immutable(file: Path) [process, error] {
  let found = linux.file_attrs(file)?
  linux.set_file_attrs(file, found.flags.bit_or(FS_IMMUTABLE))
}

## Writes one variable as the kernel expects it: the four attribute bytes and
## the payload in a single file write. A variable that was immutable is made
## immutable again afterwards, even when the write failed.
export proc write_variable(store: Store, name: Str, attributes: Int, data: Bytes) [fs, process, error] -> Result[Unit, Error] {
  let file = variable_path(store, name)?
  let restore = if file.exists() { clear_immutable(file)? } else { false }
  let written = file.write(bytes.concat([bytes.pack_le(attributes, 4)?, data]))
  if restore { restore_immutable(file)? }
  written
}

## Removes one global variable after clearing its immutable flag.
export proc delete_variable(store: Store, name: Str) [fs, process, error] -> Result[Unit, Error] {
  let file = variable_path(store, name)?
  if file.exists() { let _ = clear_immutable(file)? }
  file.unlink()
}

## A decoded entry name: the number and whether it spelled hex digits in
## lowercase.
export type EntryName = {number: Int, lower: Bool}

## Decodes `Boot0001`-style names. `lower` reports a name that uses lowercase
## hex digits, which the specification does not allow but firmware tooling
## has produced.
export pure entry_number(family: Family, name: Str) -> EntryName? {
  return null when ! name.starts_with(family.prefix) or name.byte_len() != family.prefix.byte_len() + 4

  let digits = name.byte_slice(family.prefix.byte_len())
  let number = parse_hex(digits)
  return null when number == null or ! HEX_DIGITS.matches(digits)

  {number: number ?? 0, lower: digits != digits.upper()}
}

## `Boot0001`-style name of an entry number.
export pure entry_name(family: Family, number: Int) -> Str {
  family.prefix + hex(number, 4, upper: true)
}

## One stored entry variable: its decoded number and the name as stored.
export type Named = {number: Int, name: Str, lower: Bool}

## The entry variables of a family in ascending number order.
export proc entry_names(store: Store, family: Family) [fs, error] -> Result[List[Named], Error] {
  var found: List[Named] = []
  for name in variable_names(store)? {
    let decoded = entry_number(family, name)
    continue when decoded == null

    let item = decoded ?? {number: 0, lower: false}
    found += [{number: item.number, name: name, lower: item.lower}]
  }
  Ok(found |> sort-by .number)
}

## Decodes a list of 16-bit little-endian entry numbers.
export pure parse_order(data: Bytes) -> Result[List[Int], Error] {
  guard data.len() % 2 == 0 else { return Err(error.failure("order list has an odd number of bytes")) }
  var numbers: List[Int] = []
  for at in range(data.len() / 2) { numbers += [bytes.unpack_le(data, 2, at * 2)?] }
  Ok(numbers)
}

## Encodes a list of 16-bit entry numbers.
export pure encode_order(numbers: List[Int]) -> Result[Bytes, Error] {
  var chunks: List[Bytes] = []
  for number in numbers { chunks += [bytes.pack_le(number, 2)?] }
  Ok(bytes.concat(chunks))
}

## The numbers of a family's order variable; empty when it does not exist.
export proc read_order(store: Store, family: Family) [fs, error] -> Result[List[Int]?, Error] {
  let variable = read_variable(store, family.order)?
  guard let value = variable else { return Ok(null) }
  Ok(parse_order(value.data)?)
}

pure code_points(text: Str) -> List[Int] {
  let raw = bytes.from_text(text)
  var points: List[Int] = []
  var at = 0
  while at < raw.len() {
    let lead = raw.byte_at(at) ?? 0
    let width = if lead < 128 { 1 } else if lead < 224 { 2 } else if lead < 240 { 3 } else { 4 }
    var value = if width == 1 { lead } else if width == 2 { lead - 192 } else if width == 3 { lead - 224 } else { lead - 240 }
    for step in range(1, width) { value = value * 64 + ((raw.byte_at(at + step) ?? 128) - 128) }
    points += [value]
    at += width
  }
  points
}

## UTF-16LE code units of `text`, without a terminator.
export pure utf16_units(text: Str) -> List[Int] {
  var units: List[Int] = []
  for point in code_points(text) {
    if point < 65536 { units += [point]; continue }
    let offset = point - 65536
    units += [55296 + offset / 1024, 56320 + offset % 1024]
  }
  units
}

pure units_to_bytes(units: List[Int]) -> Result[Bytes, Error] {
  var chunks: List[Bytes] = []
  for unit in units { chunks += [bytes.pack_le(unit, 2)?] }
  Ok(bytes.concat(chunks))
}

## UTF-16LE bytes of `text`, without a terminator.
export pure utf16(text: Str) -> Result[Bytes, Error] { units_to_bytes(utf16_units(text)) }

## UTF-16LE bytes of `text` with the terminating NUL used in load options.
export pure utf16_terminated(text: Str) -> Result[Bytes, Error] { units_to_bytes(utf16_units(text) + [0]) }

pure utf8_ints(point: Int) -> List[Int] {
  return [point] when point < 128
  return [192 + point / 64, 128 + point % 64] when point < 2048
  return [224 + point / 4096, 128 + point / 64 % 64, 128 + point % 64] when point < 65536

  [240 + point / 262144, 128 + point / 4096 % 64, 128 + point / 64 % 64, 128 + point % 64]
}

## Decodes UTF-16 code units; an unpaired surrogate becomes U+FFFD.
export pure text_from_units(units: List[Int]) -> Result[Str, Error] {
  var ints: List[Int] = []
  var at = 0
  while at < units.len() {
    let unit = units[at]
    at += 1
    if unit >= 55296 and unit < 56320 and at < units.len() and units[at] >= 56320 and units[at] < 57344 {
      ints += utf8_ints(65536 + (unit - 55296) * 1024 + (units[at] - 56320))
      at += 1
    } else if unit >= 55296 and unit < 57344 {
      ints += utf8_ints(65533)
    } else {
      ints += utf8_ints(unit)
    }
  }
  bytes.from_ints(ints)?.utf8()
}

type UnitRun = {units: List[Int], next: Int}

# Reads UTF-16LE units from `at` up to a NUL unit or the end of `data` and
# reports the offset just past the terminator.
pure unit_run(data: Bytes, at: Int, stop: Int) -> Result[UnitRun, Error] {
  var units: List[Int] = []
  var cursor = at
  while cursor + 2 <= stop {
    let unit = bytes.unpack_le(data, 2, cursor)?
    cursor += 2
    return Ok({units: units, next: cursor}) when unit == 0

    units += [unit]
  }
  Err(error.failure("UTF-16 text is not NUL terminated"))
}

## One device path node from its type, subtype and payload.
export pure node(kind: Int, subtype: Int, data: Bytes) -> Result[Node, Error] {
  guard data.len() + 4 <= 65535 else { return Err(error.failure("device path node is too long")) }
  Ok({kind: kind, subtype: subtype, data: data})
}

## The node that terminates a device path.
export const END_NODE: Node = {kind: 127, subtype: 255, data: b""}

## A File Path media node: `path` as a NUL-terminated UTF-16 string.
export pure file_node(location: Str) -> Result[Node, Error] { node(4, 4, utf16_terminated(location)?) }

## GUID text to its 16-byte mixed-endian wire form.
export pure guid_bytes(text: Str) -> Result[Bytes, Error] {
  guard rx"^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$".matches(text) else { return Err(error.failure(f"invalid GUID {text}")) }
  let groups = text.split("-")
  let first = bytes.pack_le(parse_hex(groups[0]) ?? 0, 4)?
  let second = bytes.pack_le(parse_hex(groups[1]) ?? 0, 2)?
  let third = bytes.pack_le(parse_hex(groups[2]) ?? 0, 2)?
  let fourth = bytes.pack_be(parse_hex(groups[3]) ?? 0, 2)?
  let fifth = bytes.pack_be(parse_hex(groups[4]) ?? 0, 8)?.slice(2)
  Ok(bytes.concat([first, second, third, fourth, fifth]))
}

pure guid_text(data: Bytes, at: Int) -> Str {
  var parts: List[Str] = []
  for offset in [3, 2, 1, 0] { parts += [hex(data.byte_at(at + offset) ?? 0, 2)] }
  let first = parts.join("")
  let second = hex(data.byte_at(at + 5) ?? 0, 2) + hex(data.byte_at(at + 4) ?? 0, 2)
  let third = hex(data.byte_at(at + 7) ?? 0, 2) + hex(data.byte_at(at + 6) ?? 0, 2)
  var rest: List[Str] = []
  for offset in range(8, 10) { rest += [hex(data.byte_at(at + offset) ?? 0, 2)] }
  var tail: List[Str] = []
  for offset in range(10, 16) { tail += [hex(data.byte_at(at + offset) ?? 0, 2)] }
  f"{first}-{second}-{third}-{rest.join("")}-{tail.join("")}"
}

## A Hard Drive media node. `signature` is the 16-byte partition identity,
## `table` is 1 for a DOS signature and 2 for a GPT partition GUID.
export pure hard_drive_node(partition: Int, start: Int, size: Int, signature: Bytes, table: Int) -> Result[Node, Error] {
  guard signature.len() == 16 and table in [1, 2] else { return Err(error.failure("invalid hard drive signature")) }
  let payload = bytes.concat([bytes.pack_le(partition, 4)?, bytes.pack_le(start, 8)?, bytes.pack_le(size, 8)?, signature, bytes.from_ints([table, table])?])
  node(4, 1, payload)
}

## Serializes a device path, without adding a terminator.
export pure encode_path(nodes: List[Node]) -> Result[Bytes, Error] {
  var chunks: List[Bytes] = []
  for item in nodes {
    chunks += [bytes.from_ints([item.kind, item.subtype])?, bytes.pack_le(item.data.len() + 4, 2)?, item.data]
  }
  Ok(bytes.concat(chunks))
}

## Splits a device path list into its nodes, up to and including the
## terminating end node when there is one.
export pure parse_path(data: Bytes) -> Result[List[Node], Error] {
  var nodes: List[Node] = []
  var at = 0
  while at < data.len() {
    guard at + 4 <= data.len() else { return Err(error.failure("device path node header is truncated")) }
    let kind = data.byte_at(at) ?? 0
    let subtype = data.byte_at(at + 1) ?? 0
    let length = bytes.unpack_le(data, 2, at + 2)?
    guard length >= 4 and at + length <= data.len() else { return Err(error.failure("device path node length is invalid")) }
    nodes += [{kind: kind, subtype: subtype, data: data.slice(at + 4, length: length - 4)}]
    at += length
    break when kind == 127 and subtype == 255
  }
  Ok(nodes)
}

pure format_node(item: Node) -> Str {
  let data = item.data
  if item.kind == 4 and item.subtype == 1 and data.len() == 38 {
    let table = data.byte_at(37) ?? 0
    let start = bytes.unpack_le(data, 8, 4) ?? 0
    let size = bytes.unpack_le(data, 8, 12) ?? 0
    let partition = bytes.unpack_le(data, 4, 0) ?? 0
    if table == 2 { return f"HD({partition},GPT,{guid_text(data, 20)},0x{hex(start, 1)},0x{hex(size, 1)})" }
    if table == 1 { return f"HD({partition},MBR,0x{hex(bytes.unpack_le(data, 4, 20) ?? 0, 8)},0x{hex(start, 1)},0x{hex(size, 1)})" }
  }
  if item.kind == 4 and item.subtype == 4 and data.len() >= 2 {
    var units: List[Int] = []
    for at in range(data.len() / 2) {
      let unit = bytes.unpack_le(data, 2, at * 2) ?? 0
      break when unit == 0

      units += [unit]
    }
    return text_from_units(units) ?? "?"
  }
  if item.kind == 2 and item.subtype == 1 and data.len() == 8 {
    let hid = bytes.unpack_le(data, 4, 0) ?? 0
    let uid = bytes.unpack_le(data, 4, 4) ?? 0
    return f"PciRoot(0x{hex(uid, 1)})" when hid == 167985616
    return f"PcieRoot(0x{hex(uid, 1)})" when hid == 168313296
    return f"Acpi(0x{hex(hid, 1)},0x{hex(uid, 1)})"
  }
  if item.kind == 1 and item.subtype == 1 and data.len() == 2 {
    return f"Pci(0x{hex(data.byte_at(1) ?? 0, 1)},0x{hex(data.byte_at(0) ?? 0, 1)})"
  }
  if item.kind == 3 and item.subtype == 18 and data.len() == 6 {
    return f"Sata({bytes.unpack_le(data, 2, 0) ?? 0},{bytes.unpack_le(data, 2, 2) ?? 0},{bytes.unpack_le(data, 2, 4) ?? 0})"
  }
  if item.kind == 127 { return if item.subtype == 1 { "," } else { "" } }

  let family = if item.kind == 1 { "HardwarePath" } else if item.kind == 2 { "AcpiPath" } else if item.kind == 3 { "Msg" } else if item.kind == 4 { "MediaPath" } else if item.kind == 5 { "BbsPath" } else { f"Path({item.kind})" }
  f"{family}({item.subtype},{hex_bytes(data).replace(" ", with: "")})"
}

## Renders a device path the way the boot manager lists it: nodes joined by
## `/`, the end node omitted.
export pure format_path(nodes: List[Node]) -> Str {
  var parts: List[Str] = []
  for item in nodes {
    continue when item.kind == 127 and item.subtype == 255

    parts += [format_node(item)]
  }
  parts.join("/")
}

## Decodes `EFI_LOAD_OPTION`: attributes, path-list length, description, the
## path list and the remaining optional data.
export pure parse_load_option(data: Bytes) -> Result[LoadOption, Error] {
  guard data.len() >= 6 else { return Err(error.failure("load option is shorter than its header")) }
  let attributes = bytes.unpack_le(data, 4, 0)?
  let path_length = bytes.unpack_le(data, 2, 4)?
  let described = unit_run(data, 6, data.len())?
  let path_end = described.next + path_length
  guard path_end <= data.len() else { return Err(error.failure("load option path list is longer than the variable")) }
  Ok({
    attributes: attributes,
    description: text_from_units(described.units)?,
    path: parse_path(data.slice(described.next, length: path_length))?,
    data: data.slice(path_end),
  })
}

## Builds the variable payload of a load option.
export pure encode_load_option(option: LoadOption) -> Result[Bytes, Error] {
  let last = if option.path.is_empty() { END_NODE } else { option.path[-1] }
  let closed = ! option.path.is_empty() and last.kind == 127 and last.subtype == 255
  let list = encode_path(if closed { option.path } else { option.path + [END_NODE] })?
  guard list.len() <= 65535 else { return Err(error.failure("device path is too long")) }
  Ok(bytes.concat([bytes.pack_le(option.attributes, 4)?, bytes.pack_le(list.len(), 2)?, utf16_terminated(option.description)?, list, option.data]))
}

## Optional data as the boot manager listing shows it: text when every byte is
## printable ASCII, otherwise lowercase hex.
export pure format_optional_data(data: Bytes) -> Str {
  return "" when data.is_empty()

  for index in range(data.len()) {
    let value = data.byte_at(index) ?? 0
    return hex_bytes(data).replace(" ", with: "") when value < 32 or value > 127
  }
  data.utf8() ?? hex_bytes(data).replace(" ", with: "")
}

## A partition's Hard Drive node from the partition table of `disk` (a block
## device or an image file).
export proc partition_node(disk: Path, number: Int) [process, error] -> Result[Node, Error] {
  let table = linux.partition_table(disk)?
  var found = null
  for partition in table.partitions {
    if partition.index == number { found = partition }
  }
  guard let chosen = found else { return Err(error.failure(f"partition {number} not found on {disk}")) }
  if table.label == "gpt" { return hard_drive_node(number, chosen.start, chosen.size, guid_bytes(chosen.uuid)?, 2) }
  guard table.label == "dos" else { return Err(error.failure(f"unsupported partition table {table.label}")) }
  let signature = parse_hex(table.id) ?? 0
  hard_drive_node(number, chosen.start, chosen.size, bytes.concat([bytes.pack_le(signature, 4)?, bytes.from_ints([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])?]), 1)
}
