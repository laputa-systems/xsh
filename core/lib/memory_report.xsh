##! Collects memory block data from a rooted sysfs tree and renders lsmem output.
##!
##! All inventory reads stay beneath the supplied root. Rendering preserves
##! util-linux column, summary, and serialization behavior for the selected mode.
use lib.gnu as gnu

const MEMORY_PATH = "sys/devices/system/memory"
const CONFIG_PATH = "sys/firmware/memory"
const PARAMETER_PATH = "sys/module/memory_hotplug/parameters/memmap_on_memory"
const COLUMN_NAMES = ["RANGE", "SIZE", "STATE", "REMOVABLE", "BLOCK", "NODE", "ZONES", "CONFIGURED", "MEMMAP-ON-MEMORY"]
const DEFAULT_COLUMNS = ["RANGE", "SIZE", "STATE", "REMOVABLE", "BLOCK"]
const ZONE_NAMES = ["DMA", "DMA32", "Normal", "Highmem", "Movable", "Device", "None", "Unknown"]
const WORD_BASE: UInt = 4294967296
const WORD_MAX: UInt = 4294967295

## Describes parsed lsmem options passed from the applet entry point.
export type Options = {
  all: Bool,
  bytes: Bool,
  json: Bool,
  noheadings: Bool,
  output: Str?,
  output_all: Bool,
  pairs: Bool,
  raw: Bool,
  split: Str?,
  sysroot: Path?,
  summary: Str?,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

## Selects whether lsmem emits its table and summary sections.
export type Modes = {table: Bool, summary: Bool}

## Holds one memory block or a contiguous merged range of blocks.
export type Block = {
  index: Int,
  count: Int,
  state: Str,
  removable: Bool,
  node: Int,
  zones: List[Str],
  configured: Bool,
  memmap_on_memory: Bool,
}

## Stores one limb of an unsigned 64-bit sysfs value.
export type Word32 = UInt range 0..=4294967295

## Represents the unsigned 64-bit values read from the memory sysfs interface.
export type U64 = {high: Word32, low: Word32}

type U64Division = {quotient: U64, remainder: UInt}

proc u64_add(left: U64, right: U64) [error] -> U64 {
  let low = left.low + right.low
  let high = left.high + right.high + low / WORD_BASE
  {high: (high % WORD_BASE) as Word32, low: (low % WORD_BASE) as Word32}
}

proc u64_add_small(value: U64, amount: UInt) [error] -> U64 {
  let low = value.low + amount
  let high = value.high + low / WORD_BASE
  {high: (high % WORD_BASE) as Word32, low: (low % WORD_BASE) as Word32}
}

proc u64_subtract_one(value: U64) [error] -> U64 {
  if value.low != 0 {
    return {high: value.high, low: (value.low - 1) as Word32}
  }
  let high = if value.high == 0 { WORD_MAX } else { value.high - 1 }
  {high: high as Word32, low: WORD_MAX as Word32}
}

proc u64_multiply_small(value: U64, factor: UInt) [error] -> U64 {
  let low = value.low * factor
  let high = value.high * factor + low / WORD_BASE
  {high: (high % WORD_BASE) as Word32, low: (low % WORD_BASE) as Word32}
}

pure u32_product_low(left: UInt, right: UInt) -> UInt {
  let base: UInt = 65536
  let left0 = left % base
  let left1 = left / base
  let right0 = right % base
  let right1 = right / base
  let product0 = left0 * right0
  let cross = left0 * right1 + left1 * right0
  (product0 + (cross % base) * base) % WORD_BASE
}

proc u64_multiply(left: U64, right: U64) [error] -> U64 {
  let base: UInt = 65536
  let left_low = left.low
  let left_high = left.high
  let right_low = right.low
  let right_high = right.high
  let left0 = left_low % base
  let left1 = left_low / base
  let right0 = right_low % base
  let right1 = right_low / base
  let product0 = left0 * right0
  let word0 = product0 % base
  let carry0 = product0 / base
  let product1 = left1 * right0 + left0 * right1 + carry0
  let word1 = product1 % base
  let carry1 = product1 / base
  let product2 = left1 * right1 + carry1
  let low = word0 + word1 * base
  let high = (product2 % base) + (product2 / base) * base + u32_product_low(left_low, right_high) + u32_product_low(left_high, right_low)
  {high: (high % WORD_BASE) as Word32, low: low as Word32}
}

proc u64_divide(value: U64, divisor: UInt) [error] -> U64Division {
  let high = value.high
  let quotient_high = high / divisor
  let high_remainder = high % divisor
  let combined = high_remainder * WORD_BASE + value.low
  {
    quotient: {high: quotient_high as Word32, low: (combined / divisor) as Word32},
    remainder: combined % divisor,
  }
}

proc u64_from_int(value: Int) [error] -> U64 {
  let number = value as UInt
  {high: (number / WORD_BASE) as Word32, low: (number % WORD_BASE) as Word32}
}

## Holds the rooted block inventory and totals used for rendering.
export type Inventory = {
  block_size: U64,
  online_bytes: U64,
  offline_bytes: U64,
  have_nodes: Bool,
  have_zones: Bool,
  have_config: Bool,
  memmap_parameter: Str?,
  blocks: List[Block],
}

type Entry = {name: Str, index: Int}

proc fail(message: Str) [process, env] -> Unit {
  gnu.error(message)
  exit 1
}

pure decimal_digits(value: Str) -> Bool {
  return false when value == ""
  for digit in value {
    if "0123456789".find(digit) == null { return false }
  }
  true
}

pure block_entry(name: Str, prefix: Str) -> Entry? {
  return null when ! name.starts_with(prefix)
  let suffix = name.byte_slice(prefix.byte_len())
  return null when ! decimal_digits(suffix)
  if let Ok(index) = suffix.parse_int() { return {name: name, index: index} }
  null
}

pure memory_entries(paths: List[Path]) -> List[Entry] {
  let entries = collect {
    for item_path in paths {
      if let entry = block_entry(item_path.name(), "memory") { yield entry }
    }
  }
  entries |> sort-by .index
}

pure hex_digit_byte(value: Int) -> Int? {
  if value >= 48 and value <= 57 { return value - 48 }
  if value >= 65 and value <= 70 { return value - 55 }
  if value >= 97 and value <= 102 { return value - 87 }
  null
}

proc block_size_value(text: Bytes) [process, env, error] -> U64 {
  var length = if text.len() > 127 { 127 } else { text.len() }
  for at in range(length) {
    if (text.byte_at(at) ?? 0) == 0 { length = at; break }
  }
  var position = 0
  while position < length and (text.byte_at(position) ?? 0) in [9, 10, 11, 12, 13, 32] { position += 1 }
  var negative = false
  if position < length and (text.byte_at(position) ?? 0) in [43, 45] {
    negative = (text.byte_at(position) ?? 0) == 45
    position += 1
  }
  if position + 1 < length and (text.byte_at(position) ?? 0) == 48 and (text.byte_at(position + 1) ?? 0) in [88, 120] { position += 2 }
  var number: U64 = {high: 0, low: 0}
  var found = false
  while position < length {
    let digit = hex_digit_byte(text.byte_at(position) ?? 0)
    if digit == null { break }
    let value = (digit ?? 0) as UInt
    let low = number.low * 16 + value
    let carry = low / WORD_BASE
    let high = number.high * 16 + carry
    if high > WORD_MAX { fail("failed to read memory block size"); return {high: 0, low: 0} }
    number = {high: high as Word32, low: (low % WORD_BASE) as Word32}
    found = true
    position += 1
  }
  return {high: 0, low: 0} when ! found
  if negative and (number.high != 0 or number.low != 0) {
    number = u64_add_small({high: (WORD_MAX - number.high) as Word32, low: (WORD_MAX - number.low) as Word32}, 1)
  }
  number
}

pure decimal_value(text: Str?) -> Int {
  return 0 when text == null
  let value = text ?? ""
  var start = 0
  while start < value.byte_len() and " \t\n\r\u{000b}\u{000c}".find(value.byte_slice(start, length: 1)) != null { start += 1 }
  let digits_start = start + (if value.byte_slice(start, length: 1) in ["+", "-"] { 1 } else { 0 })
  var end = digits_start
  while end < value.byte_len() and "0123456789".find(value.byte_slice(end, length: 1)) != null { end += 1 }
  return 0 when end == digits_start
  if let Ok(parsed) = value.byte_slice(start, length: end - start).parse_int() { parsed } else { 0 }
}

proc optional_text(root: FsRoot, attr_path: Path) [fs, error] -> Str? {
  if let Ok(text) = root.read_text(attr_path) { return without_final_newline(text) }
  null
}

pure without_final_newline(text: Str) -> Str {
  if text.ends_with("\n") { text.byte_slice(0, length: text.byte_len() - 1) } else { text }
}

proc children(root: FsRoot, directory_path: Path, label: Str) [fs, process, env, error] -> List[Path] {
  let listing = root.children(directory_path, max_entries: 65536)?
  if listing.state != "complete" {
    let detail = if listing.error_kind == null { "" } else { f": {listing.error_kind}" }
    fail(f"Failed to read {label}{detail}")
  }
  listing.children
}

proc node_id(root: FsRoot, block_path: Path, block_name: Str) [fs, process, env, error] -> Int {
  let entries = children(root, block_path, f"{block_name}")
  for entry_path in entries {
    let name = entry_path.name()
    continue when ! name.starts_with("node")
    let digits = name.byte_slice(4)
    continue when ! decimal_digits(digits)
    if let Ok(value) = digits.parse_int() { return value }
  }
  -1
}

proc zone_values(root: FsRoot, block_path: Path) [fs, error] -> List[Str] {
  let zones_path = fp"{block_path}/valid_zones"
  match root.read_bytes(zones_path) {
    Ok(raw) => {
      var length = raw.len()
      for at in range(length) {
        if (raw.byte_at(at) ?? 0) == 0 { length = at; break }
      }
      if length > 0 and (raw.byte_at(length - 1) ?? 0) == 10 { length -= 1 }
      var zones = []
      var start = 0
      for at in range(length + 1) {
        if at < length and (raw.byte_at(at) ?? 0) != 32 { continue }
        if at > start {
          let token = match raw.slice(start, length: at - start).utf8() {
            Ok(text) => text.lower()
            Err(_) => ""
          }
          var zone = "Unknown"
          for known in ZONE_NAMES {
            if known.lower() == token { zone = known; break }
          }
          if zones.len() < 8 { zones += [zone] }
        }
        start = at + 1
      }
      zones
    }
    Err(_) => []
  }
}

proc block_attributes(root: FsRoot, entry: Entry, have_config: Bool, have_nodes: Bool, have_zones: Bool) [fs, process, env, error] -> Block {
  let block_path = fp"{MEMORY_PATH}/{entry.name}"
  var state = "?"
  var removable = false
  var node = -1
  var zones: List[Str] = []
  var configured = false
  var memmap_on_memory = false
  if have_config {
    let config = optional_text(root, fp"{CONFIG_PATH}/{entry.name}/config")
    let memmap = optional_text(root, fp"{CONFIG_PATH}/{entry.name}/memmap_on_memory")
    configured = decimal_value(config) == 1
    memmap_on_memory = decimal_value(memmap) == 1
    state = "offline"
    if ! configured {
      return {index: entry.index, count: 1, state: state, removable: removable, node: 0, zones: zones, configured: configured, memmap_on_memory: memmap_on_memory}
    }
  }
  let removable_text = optional_text(root, fp"{block_path}/removable")
  removable = decimal_value(removable_text) == 1
  if let text = optional_text(root, fp"{block_path}/state") {
    state = if text == "online" { "online" } else if text == "offline" { "offline" } else if text == "going-offline" { "on->off" } else { "?" }
  }
  if have_nodes { node = node_id(root, block_path, entry.name) }
  if have_zones { zones = zone_values(root, block_path) }
  {index: entry.index, count: 1, state: state, removable: removable, node: node, zones: zones, configured: configured, memmap_on_memory: memmap_on_memory}
}

pure mergeable(current: Block, next: Block, inventory: Inventory, split: List[Str], all: Bool) -> Bool {
  return false when all or current.index + current.count != next.index
  if "STATE" in split and current.state != next.state { return false }
  if "REMOVABLE" in split and current.removable != next.removable { return false }
  if "NODE" in split and inventory.have_nodes and current.node != next.node { return false }
  if "ZONES" in split and inventory.have_zones {
    return false when current.zones.len() != next.zones.len()
    for index in range(current.zones.len()) {
      if current.zones[index] == "Unknown" or current.zones[index] != next.zones[index] { return false }
    }
  }
  if "CONFIGURED" in split and current.configured != next.configured { return false }
  if "MEMMAP-ON-MEMORY" in split and current.memmap_on_memory != next.memmap_on_memory { return false }
  true
}

proc inventory_blocks(root: FsRoot, entries: List[Entry], config_entries: List[Entry], block_size: U64, have_config: Bool, have_nodes: Bool, have_zones: Bool, split: List[Str], all: Bool, include_summary: Bool) [fs, process, env, error] -> Inventory {
  var inventory: Inventory = {
    block_size: block_size, online_bytes: {high: 0, low: 0}, offline_bytes: {high: 0, low: 0},
    have_nodes: have_nodes, have_zones: have_zones, have_config: have_config,
    memmap_parameter: null, blocks: [],
  }
  let selected = if have_config { config_entries } else { entries }
  for entry in selected {
    let block = block_attributes(root, entry, have_config, have_nodes, have_zones)
    if block.state == "online" { inventory.online_bytes = u64_add(inventory.online_bytes, block_size) } else { inventory.offline_bytes = u64_add(inventory.offline_bytes, block_size) }
    if ! inventory.blocks.is_empty() and mergeable(inventory.blocks[-1], block, inventory, split, all) {
      let previous = inventory.blocks[-1]
      inventory.blocks[inventory.blocks.len() - 1] = {...previous, count: previous.count + 1}
    } else {
      inventory.blocks += [block]
    }
  }
  if include_summary {
    if let text = optional_text(root, fp"{PARAMETER_PATH}") {
      inventory.memmap_parameter = if text.starts_with("Y") { "yes" } else if text.starts_with("N") { "no" } else if text.starts_with("force") { "force" } else { null }
    }
  }
  inventory
}

## Reads the memory block inventory and optional summary parameter beneath root.
export proc collect(root: FsRoot, split: List[Str], all: Bool, include_summary: Bool) [fs, process, env, error] -> Inventory {
  let memory_path = p"sys/devices/system/memory"
  let directory = match root.metadata(memory_path) {
    Ok(metadata) => metadata
    Err(failure) => {
      fail(f"cannot open /sys/devices/system/memory: {gnu.strerror(failure)}")
      return {block_size: {high: 0, low: 0}, online_bytes: {high: 0, low: 0}, offline_bytes: {high: 0, low: 0}, have_nodes: false, have_zones: false, have_config: false, memmap_parameter: null, blocks: []}
    }
  }
  if directory.kind != "dir" { fail("cannot open /sys/devices/system/memory: Not a directory") }
  let block_size_path = fp"{MEMORY_PATH}/block_size_bytes"
  if ! root.exists(block_size_path)? { fail("This system does not support memory blocks") }
  let block_size_contents = match root.read_bytes(block_size_path) {
    Ok(text) => text
    Err(_) => {
      fail("failed to read memory block size")
      b""
    }
  }
  if block_size_contents.is_empty() { fail("failed to read memory block size") }
  let block_size = block_size_value(block_size_contents)
  let memory_paths = children(root, memory_path, "/sys/devices/system/memory")
  let entries = memory_entries(memory_paths)
  if entries.is_empty() { fail("Failed to read /sys/devices/system/memory") }
  let have_config = root.exists(p"sys/firmware/memory/memory0")?
  let config_paths = if have_config { children(root, p"sys/firmware/memory", "/sys/firmware/memory") } else { [] }
  let config_entries = memory_entries(config_paths)
  if have_config and config_entries.is_empty() { fail("Failed to read /sys/firmware/memory") }
  var have_nodes = false
  var have_zones = false
  for entry in entries {
    let block_path = fp"{MEMORY_PATH}/{entry.name}"
    if ! have_nodes and node_id(root, block_path, entry.name) != -1 { have_nodes = true }
    if ! have_zones and root.exists(fp"{block_path}/valid_zones")? { have_zones = true }
  }
  inventory_blocks(root, entries, config_entries, block_size, have_config, have_nodes, have_zones, split, all, include_summary)
}

pure exclusive_options(argv: List[Str]) -> List[Str] {
  var names: List[Str] = []
  var index = 0
  var end_options = false
  while index < argv.len() {
    let word = argv[index]
    index += 1
    if end_options { continue }
    if word == "--" { end_options = true; continue }
    if word.starts_with("--") {
      let parts = word.byte_slice(2).split("=", maxsplit: 1)
      let name = parts[0]
      if name == "all" { names += ["all"] }
      if name == "split" { names += ["split"]; if parts.len() == 1 { index += 1 } }
      if name == "json" { names += ["json"] }
      if name == "pairs" { names += ["pairs"] }
      if name == "raw" { names += ["raw"] }
      if name in ["output", "sysroot"] and parts.len() == 1 { index += 1 }
      continue
    }
    if word.starts_with("-") and word != "-" {
      let flags = word.byte_slice(1)
      for flag in flags {
        if flag == "a" { names += ["all"] }
        if flag == "J" { names += ["json"] }
        if flag == "P" { names += ["pairs"] }
        if flag == "r" { names += ["raw"] }
        if flag == "S" { names += ["split"]; if flags.byte_len() == 1 { index += 1 }; break }
        if flag in ["o", "s"] { if flags.byte_len() == 1 { index += 1 }; break }
      }
    }
  }
  names
}

## Rejects mutually exclusive output formats and all/split combinations.
export proc check_exclusive(argv: List[Str]) [process, env] {
  let names = exclusive_options(argv)
  for pair in [["json", "pairs", "raw"], ["split", "all"]] {
    var first = null
    for name in names {
      continue when name not in pair
      if first == null { first = name; continue }
      if first != name {
        fail(f"options --{first} and --{name} cannot be combined")
      }
    }
  }
}

## Resolves table and summary emission from the original argument order.
export proc modes(argv: List[Str]) [process, env] -> Modes {
  var table = true
  var summary = true
  var index = 0
  var end_options = false
  while index < argv.len() {
    let word = argv[index]
    index += 1
    if end_options { continue }
    if word == "--" { end_options = true; continue }
    if word.starts_with("--") {
      let parts = word.byte_slice(2).split("=", maxsplit: 1)
      let name = parts[0]
      if name == "json" or name == "pairs" or name == "raw" { summary = false }
      if name == "summary" {
        if parts.len() == 1 { table = false } else {
          let value = parts[1]
          if value == "never" { summary = false } else if value == "only" { table = false } else if value == "always" { summary = true } else { gnu.error("unsupported --summary argument"); exit 1 }
        }
      }
      if name in ["output", "sysroot", "split"] and parts.len() == 1 { index += 1 }
      continue
    }
    if word.starts_with("-") and word != "-" {
      let flags = word.byte_slice(1)
      for flag in flags {
        if flag in ["J", "P", "r"] { summary = false }
        if flag in ["o", "s", "S"] { if flags.byte_len() == 1 { index += 1 }; break }
      }
    }
  }
  {table: table, summary: summary}
}

## Selects output columns from explicit output, environment output, or defaults.
export proc columns(opts: Options, env_columns: Str?) [process, env] -> List[Str] {
  var selected = if opts.output_all { COLUMN_NAMES } else { DEFAULT_COLUMNS }
  var output = ""
  if opts.output != null { output = opts.output ?? "" } else if env_columns != null { output = env_columns ?? "" } else { return selected }
  let text = output
  let append = text.starts_with("+")
  let names = if append { text.byte_slice(1).split(",") } else { text.split(",") }
  if names.is_empty() or names[0] == "" { gnu.error("unknown column: "); exit 1 }
  if ! append { selected = [] }
  for name in names {
    var found = false
    for column in COLUMN_NAMES {
      if name.upper() == column { selected += [column]; found = true; break }
    }
    if ! found { gnu.error(f"unknown column: {name}"); exit 1 }
    if selected.len() > 17 { gnu.error("too many columns specified, the limit is 17"); exit 1 }
  }
  selected
}

## Selects which attributes prevent adjacent memory blocks from merging.
export proc split_columns(opts: Options, output_columns: List[Str]) [process, env] -> List[Str] {
  if opts.split == null { return output_columns }
  let text = opts.split ?? ""
  return [] when text.lower() == "none"
  var selected = []
  for name in text.split(",") {
    var found = false
    for column in COLUMN_NAMES {
      if name.upper() == column { selected += [column]; found = true; break }
    }
    if ! found { gnu.error(f"unknown column: {name}"); exit 1 }
    if selected.len() > 8 { gnu.error("too many columns specified, the limit is 8 columns"); exit 1 }
  }
  selected
}

pure block_number(block: Block) -> Str {
  if block.count == 1 { return f"{block.index}" }
  f"{block.index}-{block.index + block.count - 1}"
}

pure wider(left: Int, right: Int) -> Int {
  if left > right { left } else { right }
}

proc range_text(block: Block, block_size: U64) [error] -> Str {
  let start = u64_multiply(u64_from_int(block.index), block_size)
  let size = u64_multiply(block_size, u64_from_int(block.count))
  let end = u64_subtract_one(u64_add(start, size))
  f"0x{u64_hex(start)}-0x{u64_hex(end)}"
}

pure hex_value(value: UInt, width: Int) -> Str {
  var number = value
  var text = ""
  let digits = "0123456789abcdef"
  while number > 0 {
    let digit: Int = number % 16
    text = digits.byte_slice(digit, length: 1) + text
    number /= 16
  }
  while text.byte_len() < width { text = "0" + text }
  text
}

pure u64_hex(value: U64) -> Str {
  hex_value(value.high, 8) + hex_value(value.low, 8)
}

proc u64_decimal(value: U64) [error] -> Str {
  if value.high == 0 and value.low == 0 { return "0" }
  var number = value
  var text = ""
  while number.high != 0 or number.low != 0 {
    let division = u64_divide(number, 10)
    let digit: Int = division.remainder
    text = "0123456789".byte_slice(digit, 1) + text
    number = division.quotient
  }
  text
}

proc u64_signed_decimal(value: U64) [error] -> Str {
  if value.high < 2147483648 { return u64_decimal(value) }
  let magnitude = u64_add_small({high: (WORD_MAX - value.high) as Word32, low: (WORD_MAX - value.low) as Word32}, 1)
  "-" + u64_decimal(magnitude)
}

proc lsmem_size(value: U64) [error] -> Str {
  var whole = value
  var exponent = 0
  var fraction: U64 = {high: 0, low: 0}
  var place: UInt = 1
  while exponent < 6 and (whole.high != 0 or whole.low >= 1024) {
    let division = u64_divide(whole, 1024)
    whole = division.quotient
    fraction = u64_add(fraction, u64_multiply_small({high: 0, low: division.remainder as Word32}, place))
    place *= 1024
    exponent += 1
  }
  var tenths: UInt = 0
  if exponent > 0 {
    var fractional_digit = fraction
    for _ in range(exponent - 1) { fractional_digit = u64_divide(fractional_digit, 1024).quotient }
    tenths = (fractional_digit.low + 50) / 100
  }
  if tenths == 10 { whole = u64_add_small(whole, 1); tenths = 0 }
  let integer = u64_decimal(whole)
  let letter = "BKMGTPE".byte_slice(exponent, length: 1)
  if tenths > 0 { f"{integer}.{tenths}{letter}" } else { f"{integer}{letter}" }
}

proc block_value(inventory: Inventory, block: Block, column: Str, byte_mode: Bool) [error] -> Str? {
  let size = u64_multiply_small(inventory.block_size, block.count as UInt)
  match column {
    "RANGE" => range_text(block, inventory.block_size)
    "SIZE" => if byte_mode { u64_signed_decimal(size) } else { lsmem_size(size) }
    "STATE" => block.state
    "REMOVABLE" => if block.state == "online" and block.removable { "yes" } else { "no" }
    "BLOCK" => block_number(block)
    "NODE" => if inventory.have_nodes { f"{block.node}" } else { null }
    "ZONES" => if inventory.have_zones { block.zones.join("/") } else { null }
    "CONFIGURED" => if inventory.have_config { if block.configured { "yes" } else { "no" } } else { null }
    "MEMMAP-ON-MEMORY" => if inventory.have_config { if block.memmap_on_memory { "yes" } else { "no" } } else { null }
    else => null
  }
}

proc block_json_value(inventory: Inventory, block: Block, column: Str, byte_mode: Bool) [error] -> Str? {
  let value = block_value(inventory, block, column, byte_mode)
  if value == null { return null }
  let text = value ?? ""
  if column == "REMOVABLE" { return if text == "yes" { "true" } else { "false" } }
  if column == "NODE" or (column == "SIZE" and byte_mode) { return text }
  json.encode(text)?
}

pure pad_left(text: Str, width: Int) -> Str {
  let count = wider(width - text.byte_len(), 0)
  let spaces = [" "] |> repeat(count).join("")
  spaces + text
}

pure pad_right(text: Str, width: Int) -> Str {
  let count = wider(width - text.byte_len(), 0)
  let spaces = [" "] |> repeat(count).join("")
  text + spaces
}

proc table_text(inventory: Inventory, columns: List[Str], opts: Options) [error] -> Str {
  let shown = [column for column in columns if inventory.have_config or column not in ["CONFIGURED", "MEMMAP-ON-MEMORY"]]
  let rows = [[block_value(inventory, block, column, opts.bytes) ?? "" for column in shown] for block in inventory.blocks]
  var widths = [column.byte_len() for column in shown]
  for row in rows {
    for index in range(shown.len()) { widths[index] = wider(widths[index], row[index].byte_len()) }
  }
  var output = ""
  if ! opts.noheadings {
    var headings = []
    for index in range(shown.len()) {
      let column = shown[index]
      headings += [if column == "RANGE" { if index == shown.len() - 1 { column } else { pad_right(column, widths[index]) } } else { pad_left(column, widths[index]) }]
    }
    output += headings.join(" ") + "\n"
  }
  for row in rows {
    var cells = []
    for index in range(shown.len()) {
      let column = shown[index]
      cells += [if column == "RANGE" { if index == shown.len() - 1 { row[index] } else { pad_right(row[index], widths[index]) } } else { pad_left(row[index], widths[index]) }]
    }
    output += cells.join(" ") + "\n"
  }
  output
}

proc raw_text(inventory: Inventory, columns: List[Str], opts: Options) [error] -> Str {
  let shown = [column for column in columns if inventory.have_config or column not in ["CONFIGURED", "MEMMAP-ON-MEMORY"]]
  var output = ""
  if ! opts.noheadings { output += shown.join(" ") + "\n" }
  for block in inventory.blocks {
    output += [block_value(inventory, block, column, opts.bytes) ?? "" for column in shown].join(" ") + "\n"
  }
  output
}

proc pairs_text(inventory: Inventory, columns: List[Str], opts: Options) [error] -> Str {
  let shown = [column for column in columns if inventory.have_config or column not in ["CONFIGURED", "MEMMAP-ON-MEMORY"]]
  var output = ""
  for block in inventory.blocks {
    let fields = collect {
      for column in shown {
        let value = block_value(inventory, block, column, opts.bytes)
        let escaped = (value ?? "").replace("\\", with: "\\\\").replace("\"", with: "\\\"").replace(" ", with: "\\x20")
        yield f"{column}=\"{escaped}\""
      }
    }
    output += fields.join(" ") + "\n"
  }
  output
}

proc json_text(inventory: Inventory, columns: List[Str], opts: Options) [error] -> Str {
  let shown = [column for column in columns if inventory.have_config or column not in ["CONFIGURED", "MEMMAP-ON-MEMORY"]]
  var output = "{\n   \"memory\": ["
  if ! inventory.blocks.is_empty() { output += "\n" }
  for block_index in range(inventory.blocks.len()) {
    let block = inventory.blocks[block_index]
    if block_index == 0 { output += "      {\n" } else { output += "{\n" }
    var fields = []
    for column in shown {
      let value = block_json_value(inventory, block, column, opts.bytes)
      let key = json.encode(column.lower())?
      let encoded = value ?? "null"
      fields += [f"         {key}: {encoded}"]
    }
    output += fields.join(",\n") + "\n      }"
    if block_index + 1 < inventory.blocks.len() { output += "," } else { output += "\n" }
  }
  output += "   ]\n}\n"
  output
}

proc summary_value(value: U64, byte_mode: Bool) [error] -> Str {
  if byte_mode { return u64_signed_decimal(value) }
  lsmem_size(value)
}

proc summary_text(inventory: Inventory, byte_mode: Bool) [error] -> Str {
  let values = [
    {label: "Memory block size:", value: summary_value(inventory.block_size, byte_mode)},
    {label: "Total online memory:", value: summary_value(inventory.online_bytes, byte_mode)},
    {label: "Total offline memory:", value: summary_value(inventory.offline_bytes, byte_mode)},
  ]
  var output = ""
  for item in values {
    output += f"{pad_right(item.label, 32)} {pad_left(item.value, if byte_mode { 15 } else { 5 })}\n"
  }
  if inventory.memmap_parameter != null {
    let label = "Memmap on memory parameter:"
    let value = inventory.memmap_parameter ?? ""
    let width = if byte_mode { 15 } else { 5 }
    output += f"{pad_right(label, 32)} {pad_left(value, width)}\n"
  }
  output
}

## Renders the requested table format and summary as one output string.
export proc render(inventory: Inventory, opts: Options, columns: List[Str], modes: Modes) [error] -> Str {
  var output = ""
  if modes.table {
    if opts.json { output = json_text(inventory, columns, opts) } else if opts.pairs { output = pairs_text(inventory, columns, opts) } else if opts.raw { output = raw_text(inventory, columns, opts) } else { output = table_text(inventory, columns, opts) }
    if modes.summary { output += "\n" }
  }
  if modes.summary { output += summary_text(inventory, opts.bytes) }
  output
}
