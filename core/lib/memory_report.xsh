use lib.gnu as gnu

const MEMORY_PATH = "sys/devices/system/memory"
const CONFIG_PATH = "sys/firmware/memory"
const PARAMETER_PATH = "sys/module/memory_hotplug/parameters/memmap_on_memory"
const COLUMN_NAMES = ["RANGE", "SIZE", "STATE", "REMOVABLE", "BLOCK", "NODE", "ZONES", "CONFIGURED", "MEMMAP-ON-MEMORY"]
const DEFAULT_COLUMNS = ["RANGE", "SIZE", "STATE", "REMOVABLE", "BLOCK"]
const ZONE_NAMES = ["DMA", "DMA32", "Normal", "Highmem", "Movable", "Device", "None", "Unknown"]

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

export type Modes = {table: Bool, summary: Bool}

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

export type Inventory = {
  block_size: Int,
  online_bytes: Int,
  offline_bytes: Int,
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
    for path in paths {
      if let entry = block_entry(path.name(), "memory") { yield entry }
    }
  }
  entries |> sort-by .index
}

pure hex_digit(value: Str) -> Int? {
  let index = "0123456789abcdef".find(value.lower())
  index
}

pure block_size_value(text: Str) -> Int {
  let value = text.trim().lower()
  let signed = value.starts_with("+") or value.starts_with("-")
  let body = if signed { value.byte_slice(1) } else { value }
  let digits = if body.starts_with("0x") { body.byte_slice(2) } else { body }
  var number = 0
  var found = false
  for digit in digits {
    if let value = hex_digit(digit) {
      number = number * 16 + value
      found = true
    } else {
      break
    }
  }
  return 0 when ! found
  if value.starts_with("-") { return 0 }
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

proc optional_text(root: FsRoot, path: Path) [fs, error] -> Str? {
  if let Ok(text) = root.read_text(path) { return without_final_newline(text) }
  null
}

pure without_final_newline(text: Str) -> Str {
  if text.ends_with("\n") { text.byte_slice(0, length: text.byte_len() - 1) } else { text }
}

proc children(root: FsRoot, path: Path, label: Str) [fs, process, env, error] -> List[Path] {
  let listing = root.children(path, max_entries: 65536)?
  if listing.state != "complete" {
    let detail = if listing.error_kind == null { "" } else { f": {listing.error_kind}" }
    fail(f"Failed to read {label}{detail}")
  }
  listing.children
}

proc node_id(root: FsRoot, block_path: Path, block_name: Str) [fs, process, env, error] -> Int {
  let entries = children(root, block_path, f"{block_name}")
  for path in entries {
    let name = path.name()
    continue when ! name.starts_with("node")
    let digits = name.byte_slice(4)
    continue when ! decimal_digits(digits)
    if let Ok(value) = digits.parse_int() { return value }
  }
  -1
}

proc zone_values(root: FsRoot, block_path: Path) [fs, error] -> List[Str] {
  let path = fp"{block_path}/valid_zones"
  if let Ok(raw) = root.read_text(path) {
    let text = without_final_newline(raw)
    var zones = []
    for token in text.split(" ") {
      continue when token == ""
      var zone = "Unknown"
      for known in ZONE_NAMES {
        if known.lower() == token.lower() { zone = known; break }
      }
      if zones.len() < 8 { zones += [zone] }
    }
    return zones
  }
  []
}

proc block_attributes(root: FsRoot, entry: Entry, have_config: Bool, have_nodes: Bool, have_zones: Bool) [fs, process, env, error] -> Block {
  let path = fp"{MEMORY_PATH}/{entry.name}"
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
  let removable_text = optional_text(root, fp"{path}/removable")
  removable = decimal_value(removable_text) == 1
  if let text = optional_text(root, fp"{path}/state") {
    state = if text == "online" { "online" } else if text == "offline" { "offline" } else if text == "going-offline" { "on->off" } else { "?" }
  }
  if have_nodes { node = node_id(root, path, entry.name) }
  if have_zones { zones = zone_values(root, path) }
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

proc inventory_blocks(root: FsRoot, entries: List[Entry], config_entries: List[Entry], block_size: Int, have_config: Bool, have_nodes: Bool, have_zones: Bool, split: List[Str], all: Bool, include_summary: Bool) [fs, process, env, error] -> Inventory {
  var inventory: Inventory = {
    block_size: block_size, online_bytes: 0, offline_bytes: 0,
    have_nodes: have_nodes, have_zones: have_zones, have_config: have_config,
    memmap_parameter: null, blocks: [],
  }
  let selected = if have_config { config_entries } else { entries }
  for entry in selected {
    let block = block_attributes(root, entry, have_config, have_nodes, have_zones)
    if block.state == "online" { inventory.online_bytes += block_size } else { inventory.offline_bytes += block_size }
    if ! inventory.blocks.is_empty() and mergeable(inventory.blocks[-1], block, inventory, split, all) {
      let previous = inventory.blocks[-1]
      inventory.blocks[-1] = {...previous, count: previous.count + 1}
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

export proc collect(root: FsRoot, split: List[Str], all: Bool, include_summary: Bool) [fs, process, env, error] -> Inventory {
  let memory_path = p"sys/devices/system/memory"
  let directory = match root.metadata(memory_path) {
    Ok(metadata) => metadata
    Err(failure) => {
      fail(f"cannot open /sys/devices/system/memory: {gnu.strerror(failure)}")
      return {block_size: 0, online_bytes: 0, offline_bytes: 0, have_nodes: false, have_zones: false, have_config: false, memmap_parameter: null, blocks: []}
    }
  }
  if directory.kind != "dir" { fail("cannot open /sys/devices/system/memory: Not a directory") }
  let block_size_path = fp"{MEMORY_PATH}/block_size_bytes"
  if ! root.exists(block_size_path)? { fail("This system does not support memory blocks") }
  let block_size_text = match root.read_text(block_size_path) {
    Ok(text) => without_final_newline(text)
    Err(_) => {
      fail("failed to read memory block size")
      ""
    }
  }
  let block_size = block_size_value(block_size_text)
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
    let path = fp"{MEMORY_PATH}/{entry.name}"
    if ! have_nodes and node_id(root, path, entry.name) != -1 { have_nodes = true }
    if ! have_zones and root.exists(fp"{path}/valid_zones")? { have_zones = true }
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
          if value == "never" { summary = false }
          else if value == "only" { table = false }
          else if value == "always" { summary = true }
          else { gnu.error("unsupported --summary argument"); exit 1 }
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

export proc columns(opts: Options, env_columns: Str?) [process, env] -> List[Str] {
  var selected = if opts.output_all { COLUMN_NAMES } else { DEFAULT_COLUMNS }
  var output = opts.output ?? env_columns
  if output == null { return selected }
  let text = output ?? ""
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

proc split_columns(opts: Options, output_columns: List[Str]) [process, env] -> List[Str] {
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

pure range_text(block: Block, block_size: Int) -> Str {
  let start = block.index * block_size
  let size = block.count * block_size
  let end = if size == 0 { -1 } else { start + size - 1 }
  let start_hex = hex_value(start, 16)
  let end_hex = if end < 0 { "ffffffffffffffff" } else { hex_value(end, 16) }
  f"0x{start_hex}-0x{end_hex}"
}

pure hex_value(value: Int, width: Int) -> Str {
  var number = value
  var text = ""
  let digits = "0123456789abcdef"
  while number > 0 { text = digits.byte_slice(number % 16, 1) + text; number /= 16 }
  while text.byte_len() < width { text = "0" + text }
  text
}

pure lsmem_size(value: Int) -> Str {
  var unit = 1
  var exponent = 0
  while exponent < 6 and value / unit >= 1024 {
    unit *= 1024
    exponent += 1
  }
  var whole = value / unit
  var tenths = if exponent == 0 { 0 } else { (value % unit / (unit / 1024) + 50) / 100 }
  if tenths == 10 { whole += 1; tenths = 0 }
  let letter = "BKMGTPE".byte_slice(exponent, length: 1)
  if tenths > 0 { f"{whole}.{tenths}{letter}" } else { f"{whole}{letter}" }
}

pure block_value(inventory: Inventory, block: Block, column: Str, byte_mode: Bool) -> Str? {
  let size = block.count * inventory.block_size
  match column {
    "RANGE" => range_text(block, inventory.block_size)
    "SIZE" => if byte_mode { f"{size}" } else { lsmem_size(size) }
    "STATE" => block.state
    "REMOVABLE" => if block.state == "online" { if block.removable { "yes" } else { "no" } } else { null }
    "BLOCK" => block_number(block)
    "NODE" => if inventory.have_nodes { f"{block.node}" } else { null }
    "ZONES" => if inventory.have_zones { block.zones.join("/") } else { null }
    "CONFIGURED" => if inventory.have_config { if block.configured { "yes" } else { "no" } } else { null }
    "MEMMAP-ON-MEMORY" => if inventory.have_config { if block.memmap_on_memory { "yes" } else { "no" } } else { null }
    else => null
  }
}

pure block_json_value(inventory: Inventory, block: Block, column: Str, byte_mode: Bool) -> Str? {
  let value = block_value(inventory, block, column, byte_mode)
  return null when value == null
  let text = value ?? ""
  if column == "REMOVABLE" { return if text == "yes" { "true" } else { "false" } }
  if column == "NODE" or (column == "SIZE" and byte_mode) { return text }
  json.encode(text)
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

pure table_text(inventory: Inventory, columns: List[Str], opts: Options) -> Str {
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
      headings += [if index == shown.len() - 1 { column } else if column == "RANGE" { pad_right(column, widths[index]) } else { pad_left(column, widths[index]) }]
    }
    output += headings.join(" ") + "\n"
  }
  for row in rows {
    var cells = []
    for index in range(shown.len()) {
      let column = shown[index]
      cells += [if index == shown.len() - 1 { row[index] } else if column == "RANGE" { pad_right(row[index], widths[index]) } else { pad_left(row[index], widths[index]) }]
    }
    output += cells.join(" ") + "\n"
  }
  output
}

pure raw_text(inventory: Inventory, columns: List[Str], opts: Options) -> Str {
  let shown = [column for column in columns if inventory.have_config or column not in ["CONFIGURED", "MEMMAP-ON-MEMORY"]]
  var output = ""
  for block in inventory.blocks {
    output += [for column in shown { block_value(inventory, block, column, opts.bytes) ?? "" }].join(" ") + "\n"
  }
  output
}

pure pairs_text(inventory: Inventory, columns: List[Str], opts: Options) -> Str {
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
    output += "      {\n"
    var fields = []
    for column in shown {
      let value = block_json_value(inventory, block, column, opts.bytes)
      fields += [f"         {json.encode(column.lower())?}: {value ?? \"null\"}"]
    }
    output += fields.join(",\n") + "\n      }"
    if block_index + 1 < inventory.blocks.len() { output += "," }
    output += "\n"
  }
  output += "   ]\n}\n"
  Ok(output)
}

pure summary_value(value: Int, byte_mode: Bool) -> Str {
  if byte_mode { return f"{value}" }
  lsmem_size(value)
}

pure summary_text(inventory: Inventory, byte_mode: Bool) -> Str {
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
    output += f"{pad_right(\"Memmap on memory parameter:\", 32)} {pad_left(inventory.memmap_parameter ?? \"\", if byte_mode { 15 } else { 5 })}\n"
  }
  output
}

export proc render(inventory: Inventory, opts: Options, columns: List[Str], modes: Modes) [error] -> Str {
  var output = ""
  if modes.table {
    if opts.json { output = json_text(inventory, columns, opts)? }
    else if opts.pairs { output = pairs_text(inventory, columns, opts) }
    else if opts.raw { output = raw_text(inventory, columns, opts) }
    else { output = table_text(inventory, columns, opts) }
    if modes.summary { output += "\n" }
  }
  if modes.summary { output += summary_text(inventory, opts.bytes) }
  output
}
