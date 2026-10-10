#!/bin/xsh
use lib.efivars as efi
use lib.gnu

# Boot-manager variables live in the directory named by EFIVARFS_PATH, which
# defaults to the kernel's efivarfs mount. A directory that is not shaped like
# efivarfs is refused unless EFIBOOTMGR_ALLOW_ANY_DIR is set.
const DEFAULT_LOADER = "\\EFI\\BOOT\\BOOTX64.EFI"

const USAGE = """efibootmgr version 18
usage: efibootmgr [options]
\t-a | --active         Set bootnum active.
\t-A | --inactive       Set bootnum inactive.
\t-b | --bootnum XXXX   Modify BootXXXX (hex).
\t-B | --delete-bootnum Delete bootnum (or every entry labelled by -L).
\t-c | --create         Create new variable bootnum and add to bootorder at index (-I).
\t-C | --create-only    Create new variable bootnum and do not add to bootorder.
\t-d | --disk disk      Disk (block device or image) containing boot loader (defaults to /dev/sda).
\t-D | --remove-dups    Remove duplicate values from BootOrder.
\t     --file-dev-path  Use an abbreviated File() device path.
\t-f | --reconnect      Re-connect devices after driver is loaded.
\t-F | --no-reconnect   Do not re-connect devices after driver is loaded.
\t-I | --index number   When creating an entry, insert it in bootorder at specified position (default: 0).
\t-l | --loader name    Loader path on the partition (defaults to "\\EFI\\BOOT\\BOOTX64.EFI").
\t-L | --label label    Boot manager display label (defaults to "Linux").
\t-n | --bootnext XXXX  Set BootNext to XXXX (hex).
\t-N | --delete-bootnext Delete BootNext.
\t-o | --bootorder XXXX,YYYY,ZZZZ,...     Explicitly set BootOrder (hex).
\t-O | --delete-bootorder Delete BootOrder.
\t-p | --part part      Partition containing loader (defaults to 1).
\t-q | --quiet          Be quiet.
\t-r | --driver         Operate on Driver variables, not Boot Variables.
\t-t | --timeout seconds  Set boot manager timeout waiting for user input.
\t-T | --delete-timeout Delete Timeout.
\t-u | --unicode | --UCS-2  Handle extra args as UCS-2 (default is ASCII).
\t-v | --verbose        Print additional information.
\t-V | --version        Return version and exit.
\t-y | --sysprep        Operate on SysPrep variables, not Boot Variables.
\t-@ | --append-binary-args file  Append extra args from file (use "-" for stdin).
\t-h | --help           Show help/usage.
"""

# Options the reference tool accepts that need device probing or disk writes
# this implementation does not do. They fail by name instead of being ignored.
const UNSUPPORTED: Map[Str] = {
  e: "EDD device-path generation is not available",
  E: "EDD device-path generation is not available",
  edd: "EDD device-path generation is not available",
  device: "EDD device-path generation is not available",
  "full-dev-path": "full hardware device paths are not available; use the abbreviated default or --file-dev-path",
  g: "forcing GPT detection is not available",
  gpt: "forcing GPT detection is not available",
  i: "netboot entries are not available",
  iface: "netboot entries are not available",
  m: "memory mirroring is not available",
  "mirror-below-4G": "memory mirroring is not available",
  M: "memory mirroring is not available",
  "mirror-above-4G": "memory mirroring is not available",
  w: "writing an MBR signature would modify the disk, which this tool never does",
  "write-signature": "writing an MBR signature would modify the disk, which this tool never does",
}

# Option name to canonical key and argument mode (0 none, 1 required).
type Spec = {key: Str, mode: Int}

const SHORT: Map[Spec] = {
  a: {key: "active", mode: 0}, A: {key: "inactive", mode: 0}, b: {key: "bootnum", mode: 1},
  B: {key: "delete-bootnum", mode: 0}, c: {key: "create", mode: 0}, C: {key: "create-only", mode: 0},
  d: {key: "disk", mode: 1}, D: {key: "remove-dups", mode: 0}, f: {key: "reconnect", mode: 0},
  F: {key: "no-reconnect", mode: 0}, I: {key: "index", mode: 1}, l: {key: "loader", mode: 1},
  L: {key: "label", mode: 1}, n: {key: "bootnext", mode: 1}, N: {key: "delete-bootnext", mode: 0},
  o: {key: "bootorder", mode: 1}, O: {key: "delete-bootorder", mode: 0}, p: {key: "part", mode: 1},
  q: {key: "quiet", mode: 0}, r: {key: "driver", mode: 0}, t: {key: "timeout", mode: 1},
  T: {key: "delete-timeout", mode: 0}, u: {key: "unicode", mode: 0}, v: {key: "verbose", mode: 0},
  V: {key: "version", mode: 0}, y: {key: "sysprep", mode: 0}, h: {key: "help", mode: 0},
  e: {key: "edd", mode: 1}, E: {key: "device", mode: 1}, g: {key: "gpt", mode: 0},
  i: {key: "iface", mode: 1}, m: {key: "mirror-below-4G", mode: 1}, M: {key: "mirror-above-4G", mode: 1},
  w: {key: "write-signature", mode: 0}, "@": {key: "append-binary-args", mode: 1},
}

const LONG: Map[Spec] = {
  active: {key: "active", mode: 0}, inactive: {key: "inactive", mode: 0}, bootnum: {key: "bootnum", mode: 1},
  "delete-bootnum": {key: "delete-bootnum", mode: 0}, create: {key: "create", mode: 0}, "create-only": {key: "create-only", mode: 0},
  disk: {key: "disk", mode: 1}, "remove-dups": {key: "remove-dups", mode: 0}, reconnect: {key: "reconnect", mode: 0},
  "no-reconnect": {key: "no-reconnect", mode: 0}, index: {key: "index", mode: 1}, loader: {key: "loader", mode: 1},
  label: {key: "label", mode: 1}, bootnext: {key: "bootnext", mode: 1}, "delete-bootnext": {key: "delete-bootnext", mode: 0},
  bootorder: {key: "bootorder", mode: 1}, "delete-bootorder": {key: "delete-bootorder", mode: 0}, part: {key: "part", mode: 1},
  quiet: {key: "quiet", mode: 0}, driver: {key: "driver", mode: 0}, timeout: {key: "timeout", mode: 1},
  "delete-timeout": {key: "delete-timeout", mode: 0}, unicode: {key: "unicode", mode: 0}, "UCS-2": {key: "unicode", mode: 0},
  verbose: {key: "verbose", mode: 0}, version: {key: "version", mode: 0}, sysprep: {key: "sysprep", mode: 0},
  "append-binary-args": {key: "append-binary-args", mode: 1}, help: {key: "help", mode: 0},
  "file-dev-path": {key: "file-dev-path", mode: 0}, "full-dev-path": {key: "full-dev-path", mode: 0},
  edd: {key: "edd", mode: 1}, device: {key: "device", mode: 1}, gpt: {key: "gpt", mode: 0},
  iface: {key: "iface", mode: 1}, "mirror-below-4G": {key: "mirror-below-4G", mode: 1},
  "mirror-above-4G": {key: "mirror-above-4G", mode: 1}, "write-signature": {key: "write-signature", mode: 0},
}

type Request = {
  activate: Bool, deactivate: Bool, bootnum: Int?, delete: Bool, create: Bool, create_only: Bool,
  disk: Str?, remove_dups: Bool, reconnect: Bool, no_reconnect: Bool, index: Int?, loader: Str?,
  label: Str?, bootnext: Int?, delete_bootnext: Bool, bootorder: Str?, delete_bootorder: Bool,
  part: Int?, quiet: Bool, driver: Bool, sysprep: Bool, timeout: Int?, delete_timeout: Bool,
  unicode: Bool, verbose: Bool, file_dev_path: Bool, append_file: Str?, operands: List[Str],
}

proc warn(message: Str) [process] -> Unit {
  eprint f"efibootmgr: {message}"
}

proc stop(status: Int, message: Str) [process] -> Unit {
  eprint $message
  exit status
}

proc usage_failure(message: Str) [process, io, env] -> Unit {
  gnu.write_text(USAGE)
  eprint f"efibootmgr: {message}"
  exit 1
}

# Quotes a malformed operand the way the reference does: the message and the
# value run together on one line and a caret marks the offending column.
proc show_malformed(message: Str, value: Str, index: Int) [process] -> Unit {
  let shown = if value == "" { "''" } else { value }
  let pad = [" "] |> repeat(message.byte_len() + index + 2).join("")
  eprint f"{message}{shown}"
  eprint f"{pad}^"
}

proc malformed(message: Str, value: Str, index: Int, status: Int) [process] -> Unit {
  show_malformed(message, value, index)
  exit status
}

# strtoul wraps a leading minus into the unsigned range, so `-1` reaches the
# range check as FFFFFFFFFFFFFFFF.
pure wrapped_hex(digits: Str) -> Str {
  let magnitude = efi.parse_hex(digits) ?? 1
  var complement = ""
  for digit in efi.hex(magnitude - 1, 16, upper: true) {
    complement += "FEDCBA9876543210".byte_slice("0123456789ABCDEF".find(digit) ?? 0, 1)
  }
  complement
}

# Hex entry number with the strtoul rules the reference applies: an optional
# 0x prefix, hex digits only, and a 16-bit range.
proc hex_number(text: Str, label: Str, syntax_status: Int, range_status: Int) [process] -> Int {
  let negative = text.starts_with("-")
  let unsigned = if negative or text.starts_with("+") { text.byte_slice(1) } else { text }
  let digits = if unsigned.starts_with("0x") or unsigned.starts_with("0X") { unsigned.byte_slice(2) } else { unsigned }
  let prefix = text.byte_len() - digits.byte_len()
  var bad = 0
  while bad < digits.byte_len() and efi.parse_hex(digits.byte_slice(bad, 1)) != null { bad += 1 }
  if digits == "" or bad < digits.byte_len() { malformed(f"Invalid {label} value", text, prefix + bad, syntax_status) }
  if negative and digits.byte_len() <= 15 { stop(range_status, f"Invalid {label} value: {wrapped_hex(digits)}\n") }
  let value = efi.parse_hex(digits)
  if value == null or value > 65535 { stop(range_status, f"Invalid {label} value: {digits.upper()}\n") }
  value ?? 0
}

# Decimal with the strtoul rules: optional sign, then digits only.
proc decimal_number(text: Str, status: Int) [process] -> Int {
  let negative = text.starts_with("-")
  let digits = if negative or text.starts_with("+") { text.byte_slice(1) } else { text }
  if ! rx"^[0-9]{1,15}$".matches(digits) { stop(status, f"invalid numeric value {text}\n") }
  let number = digits.parse_uint() ?? 0
  if negative { 0 - number } else { number }
}

proc parse_request(argv: List[Str]) [process, io, env] -> Request {
  var values: Map[Str] = {}
  var flags: Set[Str] = set.empty()
  var operands: List[Str] = []
  var at = 0
  var verbose = false
  while at < argv.len() {
    let word = argv[at]
    at += 1
    if word == "--" { operands += argv[at..]; break }
    if word.starts_with("--") {
      let body = word.byte_slice(2)
      let equals = body.find("=")
      let name = if equals == null { body } else { body.byte_slice(0, equals) }
      var matches: List[Str] = []
      for candidate in LONG.keys() {
        if candidate == name { matches = [candidate]; break }
        if candidate.starts_with(name) and name != "" { matches += [candidate] }
      }
      var keys: Set[Str] = set.empty()
      for candidate_name in matches { keys = keys.add(LONG[candidate_name].key) }
      if matches.is_empty() { usage_failure(f"unrecognized option: {name}") }
      if keys.len() > 1 { usage_failure(f"option is ambiguous: {name}") }
      let spec = LONG[matches[0]]
      if spec.mode == 0 and equals != null { usage_failure(f"option does not take an argument: {name}") }
      var value = if equals == null { "" } else { body.byte_slice(equals + 1) }
      if spec.mode == 1 and equals == null {
        if at >= argv.len() { usage_failure(f"option requires an argument: {name}") }
        value = argv[at]
        at += 1
      }
      if spec.key in UNSUPPORTED { stop(1, f"efibootmgr: option '--{name}' is not supported: {UNSUPPORTED[spec.key]}") }
      if spec.key == "verbose" { verbose = true } else if spec.mode == 0 { flags = flags.add(spec.key) } else { values[spec.key] = value }
      continue
    }
    if word.starts_with("-") and word.byte_len() > 1 {
      var offset = 1
      while offset < word.byte_len() {
        let letter = word.byte_slice(offset, 1)
        offset += 1
        if letter not in SHORT { usage_failure(f"unrecognized option: {letter}") }
        let spec = SHORT[letter]
        if spec.key in UNSUPPORTED { stop(1, f"efibootmgr: option '-{letter}' is not supported: {UNSUPPORTED[spec.key]}") }
        if spec.mode == 0 {
          if spec.key == "verbose" { verbose = true } else { flags = flags.add(spec.key) }
          continue
        }
        var value = word.byte_slice(offset)
        offset = word.byte_len()
        if value == "" {
          if at >= argv.len() { usage_failure(f"option requires an argument: {letter}") }
          value = argv[at]
          at += 1
        }
        values[spec.key] = value
      }
      continue
    }
    operands += [word]
  }
  if "help" in flags { gnu.write_text(USAGE); exit 0 }
  if "version" in flags { gnu.write_text("version 18\n"); exit 0 }
  if "driver" in flags and "sysprep" in flags { stop(25, "efibootmgr: --sysprep and --driver may not be used together.") }
  var bootnum: Int? = null
  var bootnext: Int? = null
  var timeout: Int? = null
  var index: Int? = null
  var part: Int? = null
  if "bootnum" in values { bootnum = hex_number(values["bootnum"], "bootnum", 28, 29) }
  if "bootnext" in values { bootnext = hex_number(values["bootnext"], "BootNext", 35, 36) }
  if "timeout" in values { timeout = (decimal_number(values["timeout"], 38) % 65536 + 65536) % 65536 }
  if "index" in values { index = decimal_number(values["index"], 1) }
  if "part" in values { part = decimal_number(values["part"], 38) }
  let request: Request = {
    activate: "active" in flags, deactivate: "inactive" in flags, bootnum: bootnum,
    delete: "delete-bootnum" in flags, create: "create" in flags, create_only: "create-only" in flags,
    disk: if "disk" in values { values["disk"] } else { null }, remove_dups: "remove-dups" in flags,
    reconnect: "reconnect" in flags, no_reconnect: "no-reconnect" in flags, index: index,
    loader: if "loader" in values { values["loader"] } else { null },
    label: if "label" in values { values["label"] } else { null }, bootnext: bootnext,
    delete_bootnext: "delete-bootnext" in flags, bootorder: if "bootorder" in values { values["bootorder"] } else { null },
    delete_bootorder: "delete-bootorder" in flags, part: part, quiet: "quiet" in flags,
    driver: "driver" in flags, sysprep: "sysprep" in flags, timeout: timeout,
    delete_timeout: "delete-timeout" in flags, unicode: "unicode" in flags, verbose: verbose,
    file_dev_path: "file-dev-path" in flags,
    append_file: if "append-binary-args" in values { values["append-binary-args"] } else { null },
    operands: operands,
  }
  request
}

proc failure_text(failure: Error) -> Str {
  gnu.strerror(failure)
}

# The variables a family's listing reads, in the reference's order.
proc read_u16(store: efi.Store, name: Str) [fs, error] -> Result[Int?, Error] {
  let variable = efi.read_variable(store, name)?
  guard let value = variable else { return Ok(null) }
  guard value.data.len() >= 2 else { return Err(error.failure(f"variable {name} is too short")) }
  Ok(bytes.unpack_le(value.data, 2)?)
}

proc show(store: efi.Store, family: efi.Family, request: Request) [fs, error, process, io, env] {
  var lines: List[Str] = []
  let names = efi.entry_names(store, family)?
  var lowercase = false
  for item in names { if item.lower { lowercase = true } }
  if lowercase { gnu.write_text("** Warning ** : please recreate these using efibootmgr to remove this warning.\n") }
  for item in names {
    if item.lower { eprint f"** Warning ** : {item.name} is not UEFI Spec compliant (lowercase hex in name)" }
  }
  if family.prefix == "Boot" {
    let next = read_u16(store, "BootNext")?
    if next != null { lines += [f"BootNext: {efi.hex(next, 4, upper: true)}"] }
    let current = read_u16(store, "BootCurrent")?
    if current != null { lines += [f"BootCurrent: {efi.hex(current, 4, upper: true)}"] }
    let timeout = read_u16(store, "Timeout")?
    if timeout != null { lines += [f"Timeout: {timeout} seconds"] }
  }
  let order = efi.read_order(store, family)?
  if order == null {
    lines += [if family.prefix == "Boot" { "No BootOrder is set; firmware will attempt recovery" } else { f"No {family.order} is set" }]
  } else {
    lines += [f"{family.order}: " + [efi.hex(n, 4, upper: true) for n in order].join(",")]
  }
  for item in names {
    let variable = efi.read_variable(store, item.name)
    if let Err(failure) = variable {
      eprint f"Skipping unreadable variable \"{item.name}\": {failure_text(failure)}"
      continue
    }
    guard let value = variable? else { continue }
    let option = efi.parse_load_option(value.data)
    if let Err(failure) = option {
      lines += [f"{item.name}  Could not parse load option: {failure.message}"]
      continue
    }
    let loaded = option?
    let star = if loaded.attributes.bit_and(efi.LOAD_OPTION_ACTIVE) != 0 { "*" } else { " " }
    lines += [f"{item.name}{star} {loaded.description}\t{efi.format_path(loaded.path)}{efi.format_optional_data(loaded.data)}"]
    if request.verbose {
      let rendered = [efi.hex_bytes(efi.encode_path([node])?) for node in loaded.path]
      lines += [f"      dp: {rendered.join(" / ")}"]
      if ! loaded.data.is_empty() { lines += [f"    data: {efi.hex_bytes(loaded.data)}"] }
    }
  }
  for line in lines { gnu.write_text(line + "\n") }
}

proc write_or_stop(store: efi.Store, name: Str, data: Bytes, status: Int, what: Str) [fs, process, error] {
  if let Err(failure) = efi.write_variable(store, name, efi.BOOT_ATTRIBUTES, data) {
    stop(status, f"Could not set {what}: {failure_text(failure)}")
  }
}

proc u16_bytes(value: Int) [process, error] -> Bytes {
  let packed = bytes.pack_le(value, 2)
  if let Err(failure) = packed { stop(1, f"efibootmgr: {failure.message}") }
  packed ?? b""
}

proc order_bytes(numbers: List[Int]) [process, error] -> Bytes {
  let packed = efi.encode_order(numbers)
  if let Err(failure) = packed { stop(1, f"efibootmgr: {failure.message}") }
  packed ?? b""
}

# An emptied order list is removed instead of written as an attribute-only
# variable, which firmware would treat as a deletion anyway.
proc store_order(store: efi.Store, family: efi.Family, numbers: List[Int]) [fs, process, error] {
  if numbers.is_empty() {
    if let Err(failure) = efi.delete_variable(store, family.order) {
      stop(1, f"Could not remove entry from {family.order}: {failure_text(failure)}")
    }
    return
  }
  write_or_stop(store, family.order, order_bytes(numbers), 1, family.order)
}

proc read_order_or_stop(store: efi.Store, family: efi.Family) [fs, process, error] -> List[Int] {
  let order = efi.read_order(store, family)
  if let Err(failure) = order { stop(1, f"Could not read variable '{family.order}': {failure.message}") }
  let list = order?
  return [] when list == null

  list
}

proc stored_name(names: List[efi.Named], family: efi.Family, number: Int) -> Str {
  for item in names {
    return item.name when item.number == number
  }
  efi.entry_name(family, number)
}

proc option_of(store: efi.Store, name: Str) [fs, process, error] -> efi.LoadOption? {
  let variable = efi.read_variable(store, name)
  if let Err(_) = variable { return null }
  guard let value = variable? else { return null }
  let option = efi.parse_load_option(value.data)
  if let Err(_) = option { return null }
  option?
}

proc delete_entries(store: efi.Store, family: efi.Family, request: Request, names: List[efi.Named]) [fs, process, error] {
  var numbers: List[Int] = []
  if request.bootnum != null {
    numbers = [request.bootnum]
  } else if request.label != null {
    for item in names {
      let option = option_of(store, item.name)
      if option != null and option.description == request.label { numbers += [item.number] }
    }
    if numbers.is_empty() { stop(15, "Could not delete variable") }
  } else {
    stop(3, "You must specify an entry to delete (see the -b option or -L option).")
  }
  for number in numbers {
    let deleted = efi.delete_variable(store, stored_name(names, family, number))
    if let Err(failure) = deleted { stop(15, f"Could not delete variable: {failure_text(failure)}") }
  }
  let order = efi.read_order(store, family)
  if let Err(failure) = order { stop(1, f"Could not read variable '{family.order}': {failure.message}") }
  let current = order?
  if current != null {
    let kept = [n for n in current if n not in numbers]
    if kept.len() != current.len() { store_order(store, family, kept) }
  }
}

proc create_entry(store: efi.Store, family: efi.Family, request: Request, names: List[efi.Named]) [fs, process, error, io, env] {
  let label = request.label ?? "Linux"
  for item in names {
    let option = option_of(store, item.name)
    if option != null and option.description == label { warn(f"** Warning ** : {item.name} has same label {label}") }
  }
  var number = request.bootnum ?? -1
  if request.bootnum != null {
    for item in names {
      if item.number == number { stop(40, f"efibootmgr: Cannot create {item.name}: already exists.") }
    }
  } else {
    number = 0
    for item in names {
      if item.number == number { number += 1 }
    }
    if number > 65535 { stop(5, f"Could not prepare {family.label} variable: no free entry number") }
  }
  let loader = (request.loader ?? DEFAULT_LOADER).replace("/", with: "\\")
  let located = if loader.starts_with("\\") { loader } else { "\\" + loader }
  var nodes: List[efi.Node] = []
  if ! request.file_dev_path {
    let disk = fp"{request.disk ?? "/dev/sda"}"
    let drive = efi.partition_node(disk, request.part ?? 1)
    if let Err(failure) = drive { stop(5, f"Could not prepare {family.label} variable: {failure_text(failure)}") }
    nodes += [drive?]
  }
  let file = efi.file_node(located)
  if let Err(failure) = file { stop(5, f"Could not prepare {family.label} variable: {failure.message}") }
  nodes += [file?]
  var optional = b""
  if ! request.operands.is_empty() {
    let joined = request.operands.join(" ")
    if request.unicode {
      let units = efi.utf16(joined)
      if let Err(failure) = units { stop(5, f"Could not prepare {family.label} variable: {failure.message}") }
      optional = units ?? b""
    } else {
      optional = bytes.from_text(joined)
    }
  }
  if request.append_file != null {
    let extra = gnu.read_operand(request.append_file)
    if let Err(failure) = extra { stop(5, f"Could not prepare {family.label} variable: {failure_text(failure)}") }
    optional = bytes.concat([optional, extra ?? b""])
  }
  let encoded = efi.encode_load_option({attributes: efi.LOAD_OPTION_ACTIVE, description: label, path: nodes, data: optional})
  if let Err(failure) = encoded { stop(5, f"Could not prepare {family.label} variable: {failure.message}") }
  let name = efi.entry_name(family, number)
  if let Err(failure) = efi.write_variable(store, name, efi.BOOT_ATTRIBUTES, encoded ?? b"") {
    stop(5, f"Could not set variable {name}: {failure_text(failure)}")
  }
  return when request.create_only

  let order = read_order_or_stop(store, family)
  let at = if request.index == null { 0 } else if request.index > order.len() { order.len() } else { request.index }
  store_order(store, family, [@order[0..at], number, @order[at..]])
}

proc change_entry(store: efi.Store, family: efi.Family, request: Request, names: List[efi.Named]) [fs, process, error] {
  let number = request.bootnum ?? 0
  let reconnecting = request.reconnect or request.no_reconnect
  let context = if reconnecting { "re-connect" } else { "active" }
  var found = false
  for item in names {
    continue when item.number != number

    found = true
    let variable = efi.read_variable(store, item.name)
    if let Err(failure) = variable { stop(16, f"Could not set {context} state for {item.name}: {failure_text(failure)}") }
    guard let value = variable? else { continue }
    let option = efi.parse_load_option(value.data)
    if let Err(failure) = option { stop(16, f"Could not set {context} state for {item.name}: {failure.message}") }
    var attributes = option?.attributes
    if request.activate { attributes = attributes.bit_or(efi.LOAD_OPTION_ACTIVE) }
    if request.deactivate { attributes = attributes.clear_bits(efi.LOAD_OPTION_ACTIVE) }
    if request.reconnect { attributes = attributes.bit_or(efi.LOAD_OPTION_FORCE_RECONNECT) }
    if request.no_reconnect { attributes = attributes.clear_bits(efi.LOAD_OPTION_FORCE_RECONNECT) }
    continue when attributes == option?.attributes

    let payload = bytes.concat([bytes.pack_le(attributes, 4) ?? b"", value.data.slice(4)])
    if let Err(failure) = efi.write_variable(store, item.name, value.attributes, payload) {
      stop(16, f"Could not set {context} state for {item.name}: {failure_text(failure)}")
    }
  }
  if ! found {
    warn(f"{family.label} entry {efi.hex(number, 1)} not found")
    stop(16, f"Could not set {if reconnecting { "re-connect" } else { "active" }} state for {efi.entry_name(family, number)}: No such file or directory")
  }
}

# Parses `-o`: comma-separated hex entry numbers, each of which must name a
# stored entry. Diagnostics point at the offending element.
proc parse_order_list(text: Str, family: efi.Family, names: List[efi.Named]) [process] -> List[Int] {
  var numbers: List[Int] = []
  var offset = 0
  for token in text.split(",") {
    if token == "" { malformed(f"Malformed {family.order} order", text, offset, 8) }
    offset += token.byte_len() + 1
  }
  offset = 0
  for token in text.split(",") {
    # The reference tokenizes in place, so the echoed text ends at the element.
    let shown = text.byte_slice(0, offset + token.byte_len())
    var bad = 0
    while bad < token.byte_len() and efi.parse_hex(token.byte_slice(bad, 1)) != null { bad += 1 }
    if bad < token.byte_len() { malformed(f"Invalid {family.order} order", shown, offset + bad, 8) }
    let value = efi.parse_hex(token)
    if value == null or value > 65535 {
      warn(f"Invalid {family.order} order entry value: {token.upper()}")
      malformed(f"Invalid {family.order} order", shown, offset, 8)
    }
    let number = value ?? 0
    var present = false
    for item in names { if item.number == number { present = true } }
    if ! present {
      show_malformed(f"Invalid {family.order} order entry value", shown, offset)
      stop(8, f"efibootmgr: entry {efi.hex(number, 4, upper: true)} does not exist")
    }
    numbers += [number]
    offset += token.byte_len() + 1
  }
  numbers
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let request = parse_request(argv)
  let family = if request.driver { efi.DRIVER } else if request.sysprep { efi.SYSPREP } else { efi.BOOT }
  let mode = if request.driver { "--driver" } else { "--sysprep" }
  if family.prefix != "Boot" and (request.bootnext != null or request.delete_bootnext) { stop(26, f"efibootmgr: {mode} mode does not support BootNext options.") }
  if family.prefix != "Boot" and (request.timeout != null or request.delete_timeout) { stop(27, f"efibootmgr: {mode} mode does not support timeout options.") }
  if (request.reconnect or request.no_reconnect) and ! request.driver { stop(30, "--reconnect is supported only for driver entries.") }
  if request.create and request.create_only { usage_failure("--create and --create-only may not be used together") }
  if request.activate and request.deactivate { usage_failure("--active and --inactive may not be used together") }
  if request.reconnect and request.no_reconnect { usage_failure("--reconnect and --no-reconnect may not be used together") }
  if request.delete and (request.create or request.create_only) { usage_failure("--delete-bootnum cannot be combined with --create") }
  let creating = request.create or request.create_only
  if request.index != null and ! creating { stop(1, "Index is meaningless without create") }
  if ! creating {
    if request.disk != null { usage_failure("--disk is meaningful only with --create or --create-only") }
    if request.part != null { usage_failure("--part is meaningful only with --create or --create-only") }
    if request.loader != null { usage_failure("--loader is meaningful only with --create or --create-only") }
    if request.file_dev_path { usage_failure("--file-dev-path is meaningful only with --create or --create-only") }
    if request.unicode { usage_failure("--unicode is meaningful only with --create or --create-only") }
    if request.append_file != null { usage_failure("--append-binary-args is meaningful only with --create or --create-only") }
    if request.label != null and ! request.delete { usage_failure("--label is meaningful only with --create, --create-only or --delete-bootnum") }
    if ! request.operands.is_empty() { usage_failure(f"unexpected argument: {request.operands[0]}; extra arguments are optional data for --create") }
  }
  if request.file_dev_path and (request.disk != null or request.part != null) { usage_failure("--file-dev-path does not read a disk; omit --disk and --part") }
  if request.unicode and request.operands.is_empty() { usage_failure("--unicode applies to extra arguments, but none were given") }
  if (request.activate or request.deactivate) and request.bootnum == null { stop(4, "You must specify a entry to activate (see the -b option)") }
  if (request.reconnect or request.no_reconnect) and request.bootnum == null { stop(4, "You must specify a driver entry to set re-connect on (see the -b option)") }

  let dir = efi.store_dir()
  let any_dir = (env.get_or("EFIBOOTMGR_ALLOW_ANY_DIR", "") ?? "") != ""
  if ! efi.supported(dir) { stop(2, "EFI variables are not supported on this system.") }
  let opened = efi.open_store(dir, any_dir)
  if let Err(failure) = opened { stop(2, f"efibootmgr: {failure_text(failure)}") }
  let store = opened?

  var names = efi.entry_names(store, family)?
  if request.delete {
    delete_entries(store, family, request, names)
    names = efi.entry_names(store, family)?
  }
  if creating {
    create_entry(store, family, request, names)
    names = efi.entry_names(store, family)?
  }
  if request.activate or request.deactivate or request.reconnect or request.no_reconnect { change_entry(store, family, request, names) }
  if request.remove_dups {
    let order = read_order_or_stop(store, family)
    var unique: List[Int] = []
    for number in order { if number not in unique { unique += [number] } }
    if unique.len() != order.len() { store_order(store, family, unique) }
  }
  if request.delete_bootorder {
    if let Err(failure) = efi.delete_variable(store, family.order) { stop(1, f"Could not delete {family.order}: {failure_text(failure)}") }
  }
  if request.bootorder != null {
    store_order(store, family, parse_order_list(request.bootorder, family, names))
  }
  if request.delete_bootnext {
    if let Err(failure) = efi.delete_variable(store, "BootNext") { stop(10, f"Could not delete BootNext: {failure_text(failure)}") }
  }
  if request.bootnext != null {
    let number = request.bootnext
    var present = false
    for item in names { if item.number == number { present = true } }
    if ! present { stop(12, f"Boot entry {efi.hex(number, 1, upper: true)} does not exist") }
    write_or_stop(store, "BootNext", u16_bytes(number), 13, "BootNext")
  }
  if request.delete_timeout {
    if let Err(failure) = efi.delete_variable(store, "Timeout") { stop(11, f"Could not delete Timeout: {failure_text(failure)}") }
  }
  if request.timeout != null { write_or_stop(store, "Timeout", u16_bytes(request.timeout), 14, "Timeout") }
  if ! request.quiet { show(store, family, request) }
}
