##! Conventional storage command presentation over typed Linux APIs and collectors.
use gnu
use sys_block as block
use sys_mount as mounts
use system_report as report

type Arguments = {flags: List[Str], values: Map[Str], multiple: Map[List[Str]], operands: List[Str]}

proc unsupported(value: Str) {
  gnu.usage_error(f"{value} is not supported")
}

# Only declared options reach controllers; reject a complete invocation before
# opening a device, including any unknown option after a valid operand.
proc arguments(argv: List[Str], booleans: List[Str], valued: List[Str]) -> Arguments {
  var flags: List[Str] = []
  var values: Map[Str] = {}
  var multiple: Map[List[Str]] = {}
  var operands: List[Str] = []
  var index = 0
  var ended = false
  while index < argv.len() {
    let word = argv[index]
    index += 1
    if ended or ! word.starts_with("-") or word == "-" { operands += [word]; continue }
    if word == "--" { ended = true; continue }
    let pieces = word.split("=", maxsplit: 1)
    let option = pieces[0]
    var found = false
    for forms in booleans {
      let aliases = forms.split(" ")
      if option in aliases {
        if pieces.len() > 1 { gnu.usage_error(f"option {option} does not take an argument") }
        flags += [aliases[0]]
        found = true
        break
      }
    }
    if found { continue }
    for forms in valued {
      let aliases = forms.split(" ")
      var attached: Str? = null
      if option not in aliases and word.starts_with("-") and ! word.starts_with("--") and word.byte_len() > 2 and word.byte_slice(0, 2) in aliases {
        attached = word.byte_slice(2)
      }
      if option in aliases or attached != null {
        var value = attached ?? ""
        if attached == null {
          if pieces.len() == 2 { value = pieces[1] } else {
            if index >= argv.len() { gnu.usage_error(f"option {option} requires an argument") }
            value = argv[index]
            index += 1
          }
        }
        values = values.set(aliases[0], value)
        multiple = multiple.set(aliases[0], (multiple.get(aliases[0]) ?? []) + [value])
        found = true
        break
      }
    }
    if found { continue }
    if ! word.starts_with("--") and word.byte_len() > 2 {
      var bundled: List[Str] = []
      for char in word.byte_slice(1) {
        var canonical: Str? = null
        for forms in booleans {
          let aliases = forms.split(" ")
          if f"-{char}" in aliases { canonical = aliases[0]; break }
        }
        if canonical == null { unsupported(f"option {gnu.quote(word)}") }
        bundled += [canonical ?? ""]
      }
      flags += bundled
      continue
    }
    unsupported(f"option {gnu.quote(word)}")
  }
  {flags: flags, values: values, multiple: multiple, operands: operands}
}

proc require_operands(args: Arguments, minimum: Int, maximum: Int) {
  if args.operands.len() < minimum { gnu.usage_error("missing operand") }
  if maximum >= 0 and args.operands.len() > maximum { gnu.extra_operand(args.operands[maximum]) }
}

## Match util-linux filesystem lists; a leading no excludes the entire list.
export pure type_matches(filesystem: Str, filter: Str) -> Bool {
  if filter == "" { return true }
  let types = filter.split(",")
  if types[0].starts_with("no") {
    return ! (filesystem in filter.byte_slice(2).split(","))
  }
  filesystem in types
}

proc mount_table() -> mounts.MountTable {
  let root = fs.open_root(/)?
  defer root.close()
  let table = mounts.collect(root)
  if table.source_state != report.Observed { gnu.error("cannot read mount table"); exit 1 }
  table
}

pure mount_value(entry: mounts.MountEntry, column: Str) -> Str {
  match column {
    "TARGET" => entry.target,
    "SOURCE" => entry.source,
    "FSTYPE" => entry.filesystem,
    "OPTIONS" => (entry.mount_options + [item for item in entry.super_options if item not in entry.mount_options]).join(","),
    "VFS-OPTIONS" => entry.mount_options.join(","),
    "FS-OPTIONS" => entry.super_options.join(","),
    "MAJ:MIN" => f"{entry.major}:{entry.minor}",
    "FSROOT" => entry.root,
    "ID" => f"{entry.mount_id}",
    "PARENT" => f"{entry.parent_id}",
    _ => "",
  }
}

proc columns(value: Str, allowed: List[Str]) -> List[Str] {
  let selected = value.split(",")
  for column in selected { if column not in allowed { gnu.usage_error(f"unknown column: {column}") } }
  selected
}

proc json_fields(keys: List[Str], values: List[Str]) -> Str {
  var fields: List[Str] = []
  for index in range(keys.len()) { fields += [json.encode(keys[index].lower())? + ":" + json.encode(values[index])?] }
  "{" + fields.join(",") + "}"
}

# A containing-path lookup chooses the deepest mount at a component boundary.
# Repeated targets resolve to the newest visible mount rather than prefix text.
pure target_mount(entries: List[mounts.MountEntry], target: Str) -> Int? {
  var selected: Int? = null
  var length = -1
  for index in range(entries.len()) {
    let entry = entries[index]
    if (target == entry.target or entry.target == "/" or target.starts_with(entry.target + "/")) and entry.target.byte_len() >= length {
      selected = index
      length = entry.target.byte_len()
    }
  }
  selected
}

pure raw_field(value: Str) -> Str {
  value.replace("\\", with: "\\x5c").replace(" ", with: "\\x20").replace("\t", with: "\\x09").replace("\n", with: "\\x0a")
}

pure mount_children(entries: List[mounts.MountEntry], index: Int) -> List[Int] {
  [at for at in range(entries.len()) if entries[at].parent_id == entries[index].mount_id and at != index]
}

proc mount_json_row(entries: List[mounts.MountEntry], index: Int, cols: List[Str], ancestors: List[Int]) -> Str {
  let row = json_fields(cols, [mount_value(entries[index], column) for column in cols])
  var children: List[Str] = []
  for child in mount_children(entries, index) { if child not in ancestors { children += [mount_json_row(entries, child, cols, ancestors + [index])] } }
  if children.is_empty() { return row }
  row.byte_slice(0, row.byte_len() - 1) + ",\"children\":[" + children.join(",") + "]}"
}

proc mount_text_row(entries: List[mounts.MountEntry], index: Int, cols: List[Str], ancestors: List[Int], prefix: Str) {
  var values = [mount_value(entries[index], column) for column in cols]
  for at in range(cols.len()) { if cols[at] == "TARGET" { values[at] = prefix + values[at] } }
  gnu.write_text(values.join(" ") + "\n")
  for child in mount_children(entries, index) { if child not in ancestors { mount_text_row(entries, child, cols, ancestors + [index], prefix + "  ") } }
}

## Render a rooted mount table using decoded targets and mount identities.
export proc findmnt_from_root(root: FsRoot, argv: List[Str]) {
  let args = arguments(argv, ["-J --json", "-n --noheadings", "-r --raw", "-l --list"], ["-t --types", "-S --source", "-T --target", "-o --output"])
  require_operands(args, 0, 1)
  let cols = columns(args.values.get("-o") ?? "TARGET,SOURCE,FSTYPE,OPTIONS", ["TARGET", "SOURCE", "FSTYPE", "OPTIONS", "VFS-OPTIONS", "FS-OPTIONS", "MAJ:MIN", "FSROOT", "ID", "PARENT"])
  let table = mounts.collect(root)
  if table.source_state != report.Observed { gnu.error("cannot read mount table"); exit 1 }
  var entries = table.mounts
  let source: Str? = if "-S" in args.values { args.values.get("-S")? } else { null }
  if source != null { entries = [entry for entry in entries if entry.source == source] }
  let target: Str? = if "-T" in args.values { args.values.get("-T")? } else { null }
  if target != null {
    let absolute = fp"{target}".resolve()?
    let selected = target_mount(entries, f"{absolute}")
    entries = if selected == null { [] } else { [entries[selected]] }
  }
  if ! args.operands.is_empty() { entries = [entry for entry in entries if entry.source == args.operands[0] or entry.target == args.operands[0]] }
  entries = [entry for entry in entries if type_matches(entry.filesystem, args.values.get("-t") ?? "")]
  if entries.is_empty() { exit 1 }
  let tree = "-l" not in args.flags and "-r" not in args.flags and "TARGET" in cols
  var roots: List[Int] = []
  let ids = [entry.mount_id for entry in entries]
  for index in range(entries.len()) { if ! tree or entries[index].parent_id not in ids or entries[index].parent_id == entries[index].mount_id { roots += [index] } }
  if roots.is_empty() { gnu.error("mount table contains a cyclic hierarchy"); exit 1 }
  if "-J" in args.flags {
    let rows = if tree { [mount_json_row(entries, index, cols, []) for index in roots] } else { [json_fields(cols, [mount_value(entry, column) for column in cols]) for entry in entries] }
    gnu.write_text("{\"filesystems\":[" + rows.join(",") + "]}\n")
    return
  }
  if "-n" not in args.flags { gnu.write_text(cols.join(" ") + "\n") }
  if tree { for index in roots { mount_text_row(entries, index, cols, [], "") } } else {
    for entry in entries {
      let values = [mount_value(entry, column) for column in cols]
      gnu.write_text((if "-r" in args.flags { [raw_field(value) for value in values] } else { values }).join(" ") + "\n")
    }
  }
}

proc findmnt(argv: List[Str]) {
  let root = fs.open_root(/)?
  defer root.close()
  findmnt_from_root(root, argv)
}

pure block_children(devices: List[block.BlockDevice], parent: Int) -> List[Int] {
  var children: List[Int] = []
  for index in range(devices.len()) {
    if devices[index].parent_device_index == parent or parent in devices[index].slave_indices { children += [index] }
  }
  children
}

proc block_values(device: block.BlockDevice, table: mounts.MountTable, cols: List[Str], byte_sizes: Bool, paths: Bool) -> List[Str] {
  let mounted = [entry.target for entry in table.mounts if entry.major == device.major and entry.minor == device.minor]
  var metadata = {type: "", uuid: "", label: "", part_table_type: "", part_entry_uuid: ""}
  if "FSTYPE" in cols or "UUID" in cols or "LABEL" in cols {
    metadata = linux.blkid(fp"/dev/{device.name}")?
  }
  let size = device.size_bytes
  let kind = if device.kind == "partition" { "part" } else if device.name.starts_with("loop") { "loop" } else if ! device.slave_indices.is_empty() { "dm" } else { "disk" }
  let fields: Map[Str] = {
    "NAME": if paths { f"/dev/{device.name}" } else { device.name }, "KNAME": device.name, "PATH": f"/dev/{device.name}",
    "MAJ:MIN": if device.major != null and device.minor != null { f"{device.major}:{device.minor}" } else { "" },
    "SIZE": if size == null { "" } else if byte_sizes { f"{size}" } else { bytes.human(size) },
    "RM": if device.removable == null { "" } else if device.removable { "1" } else { "0" },
    "RO": if device.read_only == null { "" } else if device.read_only { "1" } else { "0" },
    "TYPE": kind, "PKNAME": device.parent_name ?? "", "MOUNTPOINT": if mounted.is_empty() { "" } else { mounted[-1] }, "MOUNTPOINTS": mounted.join("\n"),
    "FSTYPE": metadata.type, "UUID": metadata.uuid, "LABEL": metadata.label, "MODEL": device.model.value ?? "",
    "LOG-SEC": if device.logical_sector_bytes == null { "" } else { f"{device.logical_sector_bytes}" },
    "PHY-SEC": if device.physical_sector_bytes == null { "" } else { f"{device.physical_sector_bytes}" },
  }
  [fields.get(column) ?? "" for column in cols]
}

proc block_json_row(devices: List[block.BlockDevice], index: Int, table: mounts.MountTable, cols: List[Str], args: Arguments, ancestors: List[Int]) -> Str {
  let values = block_values(devices[index], table, cols, "-b" in args.flags, "-p" in args.flags)
  var fields: List[Str] = []
  for at in range(cols.len()) {
    let key = json.encode(cols[at].lower())?
    let value = values[at]
    var encoded = if value == "" { "null" } else { json.encode(value)? }
    if cols[at] in ["RM", "RO"] and value != "" { encoded = if value == "1" { "true" } else { "false" } }
    if (cols[at] in ["LOG-SEC", "PHY-SEC"] or (cols[at] == "SIZE" and "-b" in args.flags)) and value != "" { encoded = value }
    if cols[at] == "MOUNTPOINTS" { encoded = if value == "" { "[null]" } else { json.encode(value.split("\n"))? } }
    fields += [key + ":" + encoded]
  }
  if "-d" not in args.flags and "-l" not in args.flags and "-r" not in args.flags {
    var children: List[Str] = []
    for child in block_children(devices, index) {
      if child not in ancestors and child != index { children += [block_json_row(devices, child, table, cols, args, ancestors + [index])] }
    }
    if ! children.is_empty() { fields += ["\"children\":[" + children.join(",") + "]"] }
  }
  "{" + fields.join(",") + "}"
}

proc block_text_row(devices: List[block.BlockDevice], index: Int, table: mounts.MountTable, cols: List[Str], args: Arguments, ancestors: List[Int], prefix: Str) {
  var values = block_values(devices[index], table, cols, "-b" in args.flags, "-p" in args.flags)
  if "NAME" in cols and prefix != "" {
    for at in range(cols.len()) { if cols[at] == "NAME" { values[at] = prefix + values[at] } }
  }
  gnu.write_text((if "-r" in args.flags { [raw_field(value) for value in values] } else { values }).join(" ") + "\n")
  if "-d" not in args.flags and "-l" not in args.flags and "-r" not in args.flags {
    for child in block_children(devices, index) {
      if child not in ancestors and child != index { block_text_row(devices, child, table, cols, args, ancestors + [index], prefix + "  ") }
    }
  }
}

## Render one rooted inventory; device relationships come from collector indexes.
export proc lsblk_from_root(root: FsRoot, argv: List[Str]) {
  let args = arguments(argv, ["-a --all", "-b --bytes", "-d --nodeps", "-f --fs", "-J --json", "-l --list", "-n --noheadings", "-p --paths", "-r --raw"], ["-o --output"])
  let cols = columns(args.values.get("-o") ?? (if "-f" in args.flags { "NAME,FSTYPE,LABEL,UUID,MOUNTPOINTS" } else { "NAME,MAJ:MIN,RM,SIZE,RO,TYPE,MOUNTPOINTS" }), ["NAME", "KNAME", "PATH", "MAJ:MIN", "RM", "SIZE", "RO", "TYPE", "PKNAME", "MOUNTPOINT", "MOUNTPOINTS", "FSTYPE", "UUID", "LABEL", "MODEL", "LOG-SEC", "PHY-SEC"])
  let inventory = block.collect(root)
  if ! inventory.enumeration_succeeded { gnu.error("cannot enumerate block devices"); exit 1 }
  let table = mounts.collect(root)
  let devices = inventory.devices
  var selected: List[Int] = []
  for index in range(devices.len()) {
    let device = devices[index]
    if "-a" not in args.flags and device.size_bytes == 0 { continue }
    if ! args.operands.is_empty() {
      if f"/dev/{device.name}" in args.operands { selected += [index] }
    } else if "-d" in args.flags {
      if device.parent_device_index == null and device.slave_indices.is_empty() { selected += [index] }
    } else if "-l" in args.flags or "-r" in args.flags or (device.parent_device_index == null and device.slave_indices.is_empty()) { selected += [index] }
  }
  for operand in args.operands { if operand not in [f"/dev/{devices[index].name}" for index in selected] { gnu.error(f"{operand}: not a block device"); exit 1 } }
  if "-J" in args.flags { gnu.write_text("{\"blockdevices\":[" + [block_json_row(devices, index, table, cols, args, []) for index in selected].join(",") + "]}\n"); return }
  if "-n" not in args.flags { gnu.write_text(cols.join(" ") + "\n") }
  for index in selected { block_text_row(devices, index, table, cols, args, [], "") }
}

proc lsblk(argv: List[Str]) {
  let root = fs.open_root(/)?
  defer root.close()
  lsblk_from_root(root, argv)
}

pure hexadecimal(value: UInt) -> Str {
  var number: UInt = value
  var text = ""
  while number > 0 {
    let remainder: UInt = number % 16
    let digit: Int = remainder
    text = "0123456789abcdef".byte_slice(digit, length: 1) + text
    number /= 16
  }
  "0x" + (if text == "" { "0" } else { text })
}

proc wipefs(argv: List[Str]) {
  let args = arguments(argv, ["-a --all", "-n --no-act", "-J --json", "--noheadings"], ["-o --offset", "-t --types", "-O --output"])
  require_operands(args, 1, -1)
  let cols = columns(args.values.get("-O") ?? "DEVICE,OFFSET,TYPE", ["DEVICE", "OFFSET", "TYPE"])
  let given_offsets = args.multiple.get("-o") ?? []
  if "-a" in args.flags and ! given_offsets.is_empty() { gnu.usage_error("--all and --offset are mutually exclusive") }
  var offsets: List[UInt] = []
  for value in given_offsets {
    if value.starts_with("0x") {
      let digits = value.byte_slice(2)
      if digits == "" { gnu.usage_error("invalid signature offset") }
      var parsed: UInt = 0
      for char in digits.lower() {
        let at = "0123456789abcdef".find(char)
        if at == null { gnu.usage_error("invalid hexadecimal signature offset") }
        parsed = parsed * 16 + (at ?? 0) as UInt
      }
      offsets += [parsed]
    } else { offsets += [byte_count(value)] }
  }
  let erase = "-a" in args.flags or ! offsets.is_empty()
  if erase and "-J" in args.flags { gnu.usage_error("JSON cannot be combined with signature erasure") }
  var json_rows: List[Str] = []
  if ! erase and "-J" not in args.flags and "--noheadings" not in args.flags { gnu.write_text(cols.join(" ") + "\n") }
  for name in args.operands {
    let signatures = linux.block_signatures(fp"{name}")?
    let selected = [item for item in signatures if type_matches(item.type, args.values.get("-t") ?? "") and (offsets.is_empty() or item.offset in offsets)]
    for offset in offsets { if offset not in [item.offset for item in selected] { gnu.usage_error(f"no matching signature at offset {offset} on {name}") } }
    if erase {
      if "-n" not in args.flags { linux.wipe_block_signatures(fp"{name}", [item.offset for item in selected])? }
      for item in selected { gnu.write_text(f"{name}: {item.magic.len()} bytes {if "-n" in args.flags { "would be erased" } else { "were erased" }} at offset {item.offset} ({item.type})\n") }
    } else {
      for item in selected {
        let values: Map[Str] = {"DEVICE": fp"{name}".basename(), "OFFSET": hexadecimal(item.offset), "TYPE": item.type}
        if "-J" in args.flags { json_rows += [json_fields(cols, [values.get(column) ?? "" for column in cols])] } else { gnu.write_text([values.get(column) ?? "" for column in cols].join(" ") + "\n") }
      }
    }
  }
  if "-J" in args.flags {
    gnu.write_text("{\"signatures\":[" + json_rows.join(",") + "]}\n")
  }
}

proc mount(argv: List[Str]) {
  let args = arguments(argv, ["-a --all", "-r --read-only", "-w --rw", "-B --bind", "-R --rbind", "-M --move"], ["-t --types", "-o --options"])
  if "-r" in args.flags and "-w" in args.flags { gnu.usage_error("read-only and read-write are mutually exclusive") }
  var types = args.values.get("-t") ?? ""
  var options = (args.values.get("-o") ?? "").split(",") |> where . != ""
  if "-B" in args.flags { options += ["bind"] }
  if "-R" in args.flags { options += ["rbind"] }
  if "-M" in args.flags { options += ["move"] }
  if "-r" in args.flags { options += ["ro"] }
  if "-w" in args.flags { options += ["rw"] }
  if "-a" in args.flags {
    require_operands(args, 0, 0)
    if types != "" or ! options.is_empty() { unsupported("filtered or option-overridden mount --all") }
    linux.mount_all()?
    return
  }
  if args.operands.is_empty() {
    if ! options.is_empty() { gnu.usage_error("missing mount source and target") }
    for entry in mount_table().mounts {
      if type_matches(entry.filesystem, types) { gnu.write_text(f"{entry.source} on {entry.target} type {entry.filesystem} ({mount_value(entry, "OPTIONS")})\n") }
    }
    return
  }
  if args.operands.len() == 1 and "remount" in options { linux.mount("none", fp"{args.operands[0]}", fstype: types, options: options)?; return }
  require_operands(args, 2, 2)
  if "," in types { unsupported("multiple filesystem types for a direct mount") }
  if (types == "" or types == "auto") and ! ("bind" in options or "rbind" in options or "move" in options) {
    types = linux.blkid(fp"{args.operands[0]}")?.type
    if types == "" { gnu.usage_error("cannot identify filesystem type; specify --types") }
  }
  linux.mount(args.operands[0], fp"{args.operands[1]}", fstype: types, options: options)?
}

proc blkid(argv: List[Str]) {
  let args = arguments(argv, ["-p --probe"], ["-o --output", "-s --match-tag"])
  let format = args.values.get("-o") ?? "full"
  if format not in ["full", "value", "export", "device"] { unsupported(f"output format {format}") }
  let selected_tags = args.multiple.get("-s") ?? []
  for tag in selected_tags { if tag not in ["TYPE", "UUID", "LABEL", "PTTYPE", "PART_ENTRY_UUID"] { unsupported(f"tag {tag}") } }
  var devices = args.operands
  if devices.is_empty() {
    for device in linux.block_devices()? { devices += [f"{device.path}"] + [f"{partition}" for partition in device.partitions] }
  }
  var observed = false
  for name in devices {
    let info = linux.blkid(fp"{name}")?
    let tags = ["UUID", "LABEL", "TYPE", "PTTYPE", "PART_ENTRY_UUID"]
    let values = [info.uuid, info.label, info.type, info.part_table_type, info.part_entry_uuid]
    var fields: List[Str] = []
    for index in range(tags.len()) {
      if values[index] == "" or (! selected_tags.is_empty() and tags[index] not in selected_tags) { continue }
      observed = true
      if format == "value" { fields += [values[index]] } else if format == "export" { fields += [f"{tags[index]}={values[index]}"] } else { fields += [tags[index] + "=" + json.encode(values[index])?] }
    }
    if fields.is_empty() { continue }
    if format == "device" { gnu.write_text(name + "\n") } else if format == "value" { gnu.write_text(fields.join("\n") + "\n") } else if format == "export" { gnu.write_text(f"DEVNAME={name}\n" + fields.join("\n") + "\n\n") } else { gnu.write_text(name + ": " + fields.join(" ") + "\n") }
  }
  if ! observed { exit 2 }
}

proc losetup(argv: List[Str]) {
  let args = arguments(argv, ["-a --all", "-l --list", "-f --find", "--show", "-d --detach", "-j --associated"], [])
  if "-d" in args.flags {
    if ! [flag for flag in args.flags if flag != "-d"].is_empty() { gnu.usage_error("detach cannot be combined with other modes") }
    require_operands(args, 1, -1)
    for device in args.operands { linux.loop_detach(fp"{device}")? }
    return
  }
  if "--show" in args.flags and "-f" not in args.flags { gnu.usage_error("--show requires --find") }
  if "-f" in args.flags {
    require_operands(args, 1, 1)
    if "-a" in args.flags or "-l" in args.flags or "-j" in args.flags { gnu.usage_error("incompatible loop modes") }
    let device = linux.loop_attach(fp"{args.operands[0]}")?
    if "--show" in args.flags { gnu.write_text(f"{device}\n") }
    return
  }
  if args.flags.is_empty() and args.operands.len() == 2 { let _ = linux.loop_attach(fp"{args.operands[1]}", device: fp"{args.operands[0]}")?; return }
  require_operands(args, if "-j" in args.flags { 1 } else { 0 }, 1)
  let listed = "-l" in args.flags
  if listed { gnu.write_text("NAME OFFSET SIZELIMIT BACK-FILE\n") }
  var found = false
  for entry in linux.loop_list()? {
    if ! args.operands.is_empty() and (("-j" in args.flags and f"{entry.file}" != args.operands[0]) or ("-j" not in args.flags and f"{entry.device}" != args.operands[0])) { continue }
    found = true
    if listed { gnu.write_text(f"{entry.device} {entry.offset} {entry.size} {entry.file}\n") } else { gnu.write_text(f"{entry.device}: ({entry.file}), offset {entry.offset}, sizelimit {entry.size}\n") }
  }
  if ! found and ! args.operands.is_empty() and "-j" not in args.flags { exit 1 }
}

proc swap_command(command: Str, argv: List[Str]) {
  let args = arguments(argv, if command == "mkswap" { [] } else { ["-a --all"] }, if command == "swapon" { ["-p --priority"] } else { [] })
  if "-a" in args.flags {
    require_operands(args, 0, 0)
    if ! args.values.is_empty() { unsupported("priority override with --all") }
    if command == "swapon" { linux.swapon_all()? } else { linux.swapoff_all()? }
    return
  }
  require_operands(args, 1, if command == "mkswap" { 1 } else { -1 })
  let priority = (args.values.get("-p") ?? "-1").parse_int()?
  if priority < -1 or priority > 32767 or ("-p" in args.values and priority < 0) { gnu.usage_error("priority must be between 0 and 32767") }
  for name in args.operands {
    if command == "mkswap" {
      let device = fp"{name}".resolve()?
      let metadata = fs.stat(device, follow_symlinks: true)?
      if metadata.kind not in ["file", "block"] { gnu.usage_error("swap target must be a regular file or block device") }
      linux.mkswap(device)?
    } else if command == "swapon" { linux.swapon(fp"{name}", priority: priority)? } else { linux.swapoff(fp"{name}")? }
  }
}

proc umount(argv: List[Str]) {
  let args = arguments(argv, ["-a --all", "-l --lazy", "-f --force"], ["-t --types"])
  if "-a" in args.flags {
    require_operands(args, 0, 0)
    if "-l" in args.flags or "-f" in args.flags { unsupported("lazy or forced --all unmount") }
    linux.umount_all(types: (args.values.get("-t") ?? "").split(",") |> where . != "")?
    return
  }
  require_operands(args, 1, -1)
  if "-t" in args.values { unsupported("filesystem type filter for explicit unmount targets") }
  for name in args.operands { linux.umount(fp"{name}", lazy: "-l" in args.flags, force: "-f" in args.flags)? }
}

proc blockdev(argv: List[Str]) {
  let args = arguments(argv, ["--getsize64", "--getsz", "--getsize", "--getss", "--getpbsz", "--getro", "--setro", "--setrw", "--flushbufs", "--rereadpt"], [])
  require_operands(args, 1, -1)
  if args.flags.is_empty() { gnu.usage_error("missing block-device operation") }
  for name in args.operands {
    for operation in args.flags {
      if operation == "--setro" or operation == "--setrw" { linux.blockdev_set_read_only(fp"{name}", operation == "--setro")? } else if operation == "--flushbufs" { linux.blockdev_flush(fp"{name}")? } else if operation == "--rereadpt" { linux.blockdev_reread_partition_table(fp"{name}")? } else {
        let info = linux.blockdev_info(fp"{name}")?
        let value: UInt = if operation == "--getsize64" { info.size_bytes } else if operation == "--getss" { info.logical_sector_bytes } else if operation == "--getpbsz" { info.physical_sector_bytes } else if operation == "--getro" { if info.read_only { 1 } else { 0 } } else { info.size_bytes / 512 }
        gnu.write_text(f"{value}\n")
      }
    }
  }
}

proc byte_count(value: Str) -> UInt {
  if rx"^[0-9]+$".matches(value) { return value.parse_uint()? }
  if rx"^[0-9]+[KMGT]$".matches(value) {
    let number = value.byte_slice(0, value.byte_len() - 1).parse_uint()?
    let suffix = value.byte_slice(value.byte_len() - 1)
    let factor: UInt = if suffix == "K" { 1024 } else if suffix == "M" { 1048576 } else if suffix == "G" { 1073741824 } else { 1099511627776 }
    return number * factor
  }
  gnu.usage_error(f"invalid byte count: {gnu.quote(value)}")
  0
}

proc fstrim(argv: List[Str]) {
  let args = arguments(argv, ["-v --verbose"], ["-o --offset", "-l --length", "-m --minimum"])
  require_operands(args, 1, 1)
  let offset = byte_count(args.values.get("-o") ?? "0")
  let minimum = byte_count(args.values.get("-m") ?? "0")
  let length: UInt? = if "-l" in args.values { byte_count(args.values.get("-l")?) } else { null }
  let trimmed = linux.fstrim(fp"{args.operands[0]}", offset: offset, length: length, minlen: minimum)?
  if "-v" in args.flags { gnu.write_text(f"{args.operands[0]}: {trimmed} bytes trimmed\n") }
}

proc fsfreeze(argv: List[Str]) {
  let args = arguments(argv, ["-f --freeze", "-u --unfreeze"], [])
  require_operands(args, 1, 1)
  let freeze = "-f" in args.flags
  let thaw = "-u" in args.flags
  if freeze == thaw { gnu.usage_error("specify exactly one of --freeze and --unfreeze") }
  linux.fsfreeze(fp"{args.operands[0]}", "-f" in args.flags)?
}

proc partprobe(argv: List[Str]) {
  let args = arguments(argv, ["-d --dry-run", "-s --summary"], [])
  require_operands(args, 1, -1)
  for name in args.operands {
    let table = linux.partition_table(fp"{name}")?
    if "-s" in args.flags { gnu.write_text(f"{name}: {table.label} partitions " + [f"{item.index}" for item in table.partitions].join(" ") + "\n") }
    if "-d" not in args.flags { linux.blockdev_reread_partition_table(fp"{name}")? }
  }
}

pure partition_node(name: Str, index: Int) -> Str {
  let separator = if name.byte_slice(name.byte_len() - 1) in ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"] { "p" } else { "" }
  name + separator + f"{index}"
}

type PartitionInput = {index: Int, start: Int, end: Int, size: Int, type: Str, uuid: Str, name: Str}

proc sector_count(value: Str, sector_size: Int) -> Int {
  let text = value.trim()
  if rx"^[0-9]+$".matches(text) { return text.parse_int()? }
  if rx"^[0-9]+[KMGT]$".matches(text) {
    let number = text.byte_slice(0, text.byte_len() - 1).parse_int()?
    let suffix = text.byte_slice(text.byte_len() - 1)
    let factor = if suffix == "K" { 1024 } else if suffix == "M" { 1048576 } else if suffix == "G" { 1073741824 } else { 1099511627776 }
    return (number * factor + sector_size - 1) / sector_size
  }
  unsupported(f"partition size {gnu.quote(value)} (use sectors or K/M/G/T)")
  0
}

# Parse the conventional sfdisk named-field input before issuing any write.
# Sequential explicit ranges keep every affected byte and partition visible.
proc write_sfdisk(name: Str, args: Arguments) {
  require_operands(args, 1, 1)
  let text = io.stdin_text()?
  var label = args.values.get("--label") ?? "dos"
  var id = ""
  var sector_size = 512
  var partitions: List[PartitionInput] = []
  for input in text.lines() {
    let line = input.trim()
    if line == "" or line.starts_with("#") { continue }
    if rx"^(label|label-id|unit|sector-size|device):".matches(line) and ! partitions.is_empty() { gnu.usage_error("partition table headers must precede partition rows") }
    if line.starts_with("label:") {
      let supplied = line.byte_slice(6).trim()
      if "--label" in args.values and supplied != args.values.get("--label")? { gnu.usage_error("partition input label disagrees with --label") }
      label = supplied
      continue
    }
    if line.starts_with("label-id:") { id = line.byte_slice(9).trim(); continue }
    if line.starts_with("unit:") {
      if line.byte_slice(5).trim() != "sectors" { unsupported("partition input unit other than sectors") }
      continue
    }
    if line.starts_with("sector-size:") { sector_size = line.byte_slice(12).trim().parse_int()?; continue }
    if line.starts_with("device:") {
      if line.byte_slice(7).trim() != name { gnu.usage_error("partition input device disagrees with operand") }
      continue
    }
    if line.starts_with("first-lba:") or line.starts_with("last-lba:") or line.starts_with("table-length:") { unsupported("custom GPT geometry headers") }
    if label not in ["gpt", "dos"] { gnu.usage_error("partition label must be gpt or dos") }
    if sector_size <= 0 { gnu.usage_error("sector size must be positive") }
    var body = line
    var partition_index = partitions.len() + 1
    if ":" in body {
      let prefix = body.split(":", maxsplit: 1)
      let node = prefix[0].trim()
      let base = partition_node(name, 1).byte_slice(0, partition_node(name, 1).byte_len() - 1)
      if ! node.starts_with(base) { gnu.usage_error("partition node disagrees with device operand") }
      partition_index = node.byte_slice(base.byte_len()).parse_uint_positive()?
      if node != partition_node(name, partition_index) { gnu.usage_error("invalid partition node") }
      body = prefix[1].trim()
    }
    var start: Int? = null
    var size: Int? = null
    var kind = if label == "gpt" { "0fc63daf-8483-4772-8e79-3d69d8477de4" } else { "83" }
    var uuid = ""
    var part_name = ""
    var seen: Set[Str] = set.empty()
    for field in body.split(",") {
      let pieces = field.trim().split("=", maxsplit: 1)
      if pieces.len() != 2 { unsupported("positional or bootable sfdisk fields; use start=, size=, type=") }
      let key = pieces[0].trim()
      if key in seen { gnu.usage_error(f"duplicate partition field: {key}") }
      seen = seen.add(key)
      let value = pieces[1].trim()
      if key == "start" { start = sector_count(value, sector_size) } else if key == "size" { size = sector_count(value, sector_size) } else if key == "type" {
        kind = value
        if value == "L" { kind = if label == "gpt" { "0fc63daf-8483-4772-8e79-3d69d8477de4" } else { "83" } }
        if value == "U" { kind = if label == "gpt" { "c12a7328-f81f-11d2-ba4b-00a0c93ec93b" } else { "ef" } }
      } else if key == "uuid" { uuid = value } else if key == "name" {
        if value.starts_with("\"") and value.ends_with("\"") {
          part_name = json.decode(value)?.require(Str)?
        } else { part_name = value }
      } else { unsupported(f"partition input field {key}") }
    }
    if start == null or size == null { unsupported("implicit partition ranges; specify start= and size=") }
    let first = start ?? 0
    let length = size ?? 0
    if first <= 0 or length <= 0 { gnu.usage_error("partition start and size must be positive") }
    let last = first + length - 1
    for previous in partitions {
      if partition_index == previous.index { gnu.usage_error("duplicate partition index") }
      if first <= previous.end and last >= previous.start { gnu.usage_error("partition ranges overlap") }
    }
    partitions += [{index: partition_index, start: first, end: last, size: length, type: kind, uuid: uuid, name: part_name}]
  }
  if label not in ["gpt", "dos"] { gnu.usage_error("partition label must be gpt or dos") }
  if partitions.is_empty() { gnu.usage_error("partition input is empty") }
  if (label == "dos" and partitions.len() > 4) or partitions.len() > 128 { gnu.usage_error("too many primary partitions") }
  let table = {label: label, id: id, sector_size: sector_size, partitions: partitions}
  if "-n" in args.flags { gnu.write_text(json.encode(table, pretty: true)? + "\n"); return }
  linux.write_partition_table(fp"{name}", table)?
}

proc partition_command(command: Str, argv: List[Str]) {
  let booleans = if command == "sfdisk" { ["-l --list", "-J --json", "-d --dump", "-n --no-act"] } else if command == "fdisk" { ["-l --list"] } else { ["-l --list", "-s --show", "--noheadings"] }
  let valued = if command == "sfdisk" { ["--label"] } else { ["-o --output"] }
  let args = arguments(argv, booleans, valued)
  require_operands(args, 1, -1)
  if command == "fdisk" and "-l" not in args.flags { unsupported("interactive fdisk editing; use sfdisk for scripted partition tables") }
  if command == "sfdisk" and [flag for flag in args.flags if flag != "-n"].is_empty() { write_sfdisk(args.operands[0], args); return }
  if ("-n" in args.flags and command == "sfdisk") or "--label" in args.values { gnu.usage_error("write options cannot be combined with inspection modes") }
  let cols = columns(args.values.get("-o") ?? "NR,START,END,SECTORS,SIZE,NAME,UUID,TYPE", ["NR", "START", "END", "SECTORS", "SIZE", "NAME", "UUID", "TYPE"])
  for name in args.operands {
    let table = linux.partition_table(fp"{name}")?
    if "-J" in args.flags {
      var partitions: List[Str] = []
      for item in table.partitions {
        let node = partition_node(name, item.index)
        partitions += [json.encode({node: node, start: item.start, size: item.size, type: item.type, uuid: item.uuid, name: item.name})?]
      }
      let prefix = json.encode({label: table.label, id: table.id, device: name, unit: "sectors", sectorsize: table.sector_size})?
      gnu.write_text("{\"partitiontable\":" + prefix.byte_slice(0, prefix.byte_len() - 1) + ",\"partitions\":[" + partitions.join(",") + "]}}\n")
    } else if "-d" in args.flags {
      gnu.write_text(f"label: {table.label}\nlabel-id: {table.id}\ndevice: {name}\nunit: sectors\nsector-size: {table.sector_size}\n\n")
      for item in table.partitions { gnu.write_text(f"{partition_node(name, item.index)} : start={item.start}, size={item.size}, type={item.type}, uuid={item.uuid}, name=" + json.encode(item.name)? + "\n") }
    } else {
      if command == "fdisk" { gnu.write_text(f"Disk {name}: disklabel type: {table.label}\n") }
      if "--noheadings" not in args.flags { gnu.write_text(cols.join(" ") + "\n") }
      for item in table.partitions {
        let row: Map[Str] = {"NR": f"{item.index}", "START": f"{item.start}", "END": f"{item.end}", "SECTORS": f"{item.size}", "SIZE": f"{item.size * table.sector_size}", "NAME": item.name, "UUID": item.uuid, "TYPE": item.type}
        gnu.write_text([row.get(column) ?? "" for column in cols].join(" ") + "\n")
      }
    }
  }
}

pure usage(command: Str) -> Str {
  let details = match command {
    "lsblk" => "[-a -b -d -f -J -l -n -p -r] [-o COLUMNS] [DEVICE...]\nList block devices and typed partition/dependency relationships.",
    "blkid" => "[-p] [-o full|value|export|device] [-s TAG] [DEVICE...]\nProbe filesystem and partition tags directly. Repeated -s selects multiple tags.",
    "findmnt" => "[-J -l -n -r] [-t TYPES] [-S SOURCE] [-T PATH] [-o COLUMNS] [SOURCE|TARGET]\nRead the live mountinfo hierarchy. --fstab is not supported.",
    "mount" => "[-r|-w] [-B|-R|-M] [-t TYPE] [-o OPTIONS] SOURCE TARGET\n       mount [-t TYPES]\n       mount -a\nMount filesystems, bind/move mounts, or list mounts. A single fstab operand and filtered --all are not supported.",
    "umount" => "[-l|-f] TARGET...\n       umount -a [-t TYPES]\nUnmount named targets. Lazy/forced --all is not supported.",
    "losetup" => "[-a|-l] [LOOP]\n       losetup -f [--show] FILE\n       losetup LOOP FILE\n       losetup -d LOOP...\n       losetup -j FILE\nInspect, attach or detach loop devices. Offset, size-limit, read-only and partition-scan options are not supported.",
    "blockdev" => "OPERATION... DEVICE...\nOperations: --getsize64 --getsz --getsize --getss --getpbsz --getro --setro --setrw --flushbufs --rereadpt.",
    "wipefs" => "[-J|--noheadings] [-O DEVICE,OFFSET,TYPE] [-t TYPES] DEVICE...\n       wipefs [-n] [-a|-o OFFSET...] [-t TYPES] DEVICE...\nProbe known signatures or erase only their magic bytes. Repeated -o selects multiple offsets. Backup and forced nested-signature modes are not supported.",
    "partx" => "[-s|-l] [--noheadings] [-o COLUMNS] DEVICE...\nInspect partition table extents. Kernel partition add/delete/update is not supported.",
    "partprobe" => "[-d] [-s] DEVICE...\nRead partition tables and request a kernel reread for named devices. -d validates without rereading.",
    "fstrim" => "[-v] [-o OFFSET] [-l LENGTH] [-m MINIMUM] MOUNTPOINT\nTrim one explicitly named filesystem; byte counts accept K/M/G/T. --all is not supported.",
    "fsfreeze" => "(-f|-u) MOUNTPOINT\nFreeze or thaw one explicitly named filesystem.",
    "sfdisk" => "(--json|--dump|--list) DEVICE...\n       sfdisk [-n] [--label gpt|dos] DEVICE < INPUT\nWrite conventional named start=,size=,type=,uuid=,name= fields with explicit nonoverlapping ranges. GPT and DOS primary partitions are supported. Positional fields, implicit ranges, bootable flags, custom GPT geometry and DOS logical/extended tables are not supported.",
    "fdisk" => "-l [-o COLUMNS] DEVICE...\nInspect GPT and DOS primary partition tables. Interactive editing and DOS logical/extended tables are not supported; use sfdisk for scripted writes.",
    "swapon" => "[-p PRIORITY] DEVICE...\n       swapon -a\nActivate named swap devices/files, or fstab swap entries. Swap listing and discard options are not supported.",
    "swapoff" => "DEVICE...\n       swapoff -a\nDeactivate named swap devices/files, or all active swap.",
    "mkswap" => "DEVICE\nInitialize an existing swap device or file. Label, UUID, page-size and creation options are not supported.",
    _ => "[OPTIONS] [DEVICE...]",
  }
  f"Usage: {command} {details}\n"
}

## Dispatch storage applets, keeping unsupported kernel operations explicit.
export proc dispatch(command: Str, argv: List[Str]) {
  for argument in argv {
    if argument == "--" { break }
    if argument == "--help" or argument == "-h" { gnu.help(usage(command)); return }
    if argument == "--version" or argument == "-V" { gnu.version(command); return }
  }
  if let Err(failure) = execute(command, argv) { gnu.error(gnu.strerror(failure)); exit 1 }
}

proc execute(command: Str, argv: List[Str]) -> Result[Unit] {
  match command {
    "lsblk" => lsblk(argv),
    "blkid" => blkid(argv),
    "findmnt" => findmnt(argv),
    "mount" => mount(argv),
    "umount" => umount(argv),
    "blockdev" => blockdev(argv),
    "wipefs" => wipefs(argv),
    "fstrim" => fstrim(argv),
    "fsfreeze" => fsfreeze(argv),
    "partprobe" => partprobe(argv),
    "losetup" => losetup(argv),
    "swapon" => swap_command(command, argv),
    "swapoff" => swap_command(command, argv),
    "mkswap" => swap_command(command, argv),
    "sfdisk" => partition_command(command, argv),
    "fdisk" => partition_command(command, argv),
    "partx" => partition_command(command, argv),
    _ => unsupported(f"{command} operation"),
  }
}
