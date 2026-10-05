##! Parses bounded Linux inventory source records for system-report.
use sys_block
use sys_pci
use sys_source as src
use system_report as report

## Retains one bounded USB descriptor after validating its framing.
export type UsbDescriptorRecord = {
  offset: Int,
  length: Int,
  descriptor_type: Int,
  raw: Bytes,
}

## Preserves the process-visible cgroup v2 path and any valid legacy membership.
export type UnifiedCgroupPath = {state: report.ObservationState, path: Str?, has_v1: Bool}

## Identifies one visible cgroup mount by its filesystem root and mount point.
export type CgroupMount = {root: Str, point: Str}

## Keeps the selected transparent-huge-page policy alongside all kernel choices.
export type TransparentHugePagePolicy = {selected: Str, available: List[Str]}

## Keeps the active block scheduler with every scheduler offered by the device.
export type BlockScheduler = sys_block.BlockScheduler

## Stores the four components of a PCI function address.
export type PciAddress = sys_pci.PciAddress

## Retains PCI enumeration status, valid functions, and field-level issues.
export type PciCollection = {
  status: report.SectionStatus,
  functions: List[report.PciFunction],
  issues: List[report.CollectionIssue],
}

## Retains a source observation and stable read-error details.
export type SourceRead = src.SourceRead

## Retains a bounded integer or the source, syntax, or range state that withheld it.
export type BoundedNumber = src.BoundedNumber

error SystemReportSourceError {
    InvalidPciAddress(message: Str)
    InvalidPciId(message: Str)
    InvalidUsbDescriptor(message: Str)
    InvalidThpPolicy(message: Str)
    InvalidCgroupMount(message: Str)
    InvalidCpuFreqMembers(message: Str)
    InvalidCacheSharing(message: Str)
    InvalidIdleStateIndex(message: Str)
}

## Parses a canonical kernel state directory without publishing an inexact JSON integer.
export pure parse_idle_state_index(name: Str) -> Result[Int, Error] {
  guard name.starts_with("state") else {
    return Err(SystemReportSourceError.InvalidIdleStateIndex(message: "CPUIdle directory does not start with state"))
  }

  let suffix = name.split("") |> drop(5).join("")
  if suffix == "" {
    return Err(SystemReportSourceError.InvalidIdleStateIndex(message: "CPUIdle state index is empty"))
  }

  for digit in suffix {
    if digit not in "0123456789" {
      return Err(SystemReportSourceError.InvalidIdleStateIndex(message: "CPUIdle state index is not decimal"))
    }
  }

  var index = -1
  if let Ok(value) = suffix.parse_int() {
    index = value
  } else {
    return Err(
      SystemReportSourceError.InvalidIdleStateIndex(message: "CPUIdle state index is outside the supported integer range"),
    )
  }

  if index > 9007199254740991 or f"state{index}" != name {
    return Err(
      SystemReportSourceError.InvalidIdleStateIndex(
        message: "CPUIdle state index is noncanonical or outside the exact JSON range",
      ),
    )
  }

  index
}

## Requires an unambiguous CPU list before a cache can claim shared ownership.
export pure parse_cache_shared_cpus(value: Str) -> Result[List[Int], Error] {
  if let Ok(ids) = report.parse_cpu_list(value) {
    Ok(ids)
  } else {
    Err(SystemReportSourceError.InvalidCacheSharing(message: "cache shared CPU list is malformed"))
  }
}

## Parses the space-separated CPU identifiers exported by CPUFreq policy membership files.
export pure parse_cpufreq_members(value: Str) -> Result[List[Int], Error] {
  if value == "" or value.trim() != value {
    return Err(
      SystemReportSourceError.InvalidCpuFreqMembers(message: "CPUFreq membership is empty or has surrounding whitespace"),
    )
  }

  var ids: List[Int] = []
  var seen = set.empty()
  for word in value.split(" ") {
    continue when word == ""
    for character in word {
      if character not in "0123456789" {
        return Err(
          SystemReportSourceError.InvalidCpuFreqMembers(message: "CPUFreq membership has a non-decimal CPU identifier"),
        )
      }
    }

    let parsed = report.parse_cpu_list(word)
    var cpu_id = -1
    if let Ok(members) = parsed {
      guard members.len() == 1 else {
        return Err(
          SystemReportSourceError.InvalidCpuFreqMembers(message: "CPUFreq membership must use individual CPU identifiers"),
        )
      }

      cpu_id = members[0]
    } else {
      return Err(
        SystemReportSourceError.InvalidCpuFreqMembers(message: "CPUFreq membership has a non-decimal CPU identifier"),
      )
    }

    let key = f"{cpu_id}"
    if key in seen {
      return Err(SystemReportSourceError.InvalidCpuFreqMembers(message: "CPUFreq membership repeats a CPU identifier"))
    }

    if ids.len() >= 65536 {
      return Err(SystemReportSourceError.InvalidCpuFreqMembers(message: "CPUFreq membership exceeds 65536 CPUs"))
    }

    seen = set.add(seen, key)
    ids += [cpu_id]
  }

  if ids.len() == 0 {
    return Err(SystemReportSourceError.InvalidCpuFreqMembers(message: "CPUFreq membership is empty"))
  }

  ids |> sort-by .
}

## Parses USB descriptor framing while preserving unknown descriptor payloads.
export pure parse_usb_descriptor_stream(data: Bytes) -> Result[List[UsbDescriptorRecord], Error] {
  let max_bytes = 1048576
  let max_descriptors = 65536
  if data.len() > max_bytes {
    return Err(SystemReportSourceError.InvalidUsbDescriptor(message: "USB descriptor input exceeds the 1 MiB limit"))
  }

  var descriptors: List[UsbDescriptorRecord] = []
  var offset = 0
  while offset < data.len() {
    let remaining = data.len() - offset
    if remaining < 2 {
      return Err(SystemReportSourceError.InvalidUsbDescriptor(message: "USB descriptor header is truncated"))
    }

    let length = bytes.unpack_le(data, 1, offset)?
    let descriptor_type = bytes.unpack_le(data, 1, offset + 1)?
    if length < 2 {
      return Err(
        SystemReportSourceError.InvalidUsbDescriptor(message: "USB descriptor length is smaller than its header"),
      )
    }

    if length > remaining {
      return Err(
        SystemReportSourceError.InvalidUsbDescriptor(message: "USB descriptor extends beyond the available bytes"),
      )
    }

    if descriptors.len() == max_descriptors {
      return Err(
        SystemReportSourceError.InvalidUsbDescriptor(message: "USB descriptor count exceeds the 65,536 descriptor limit"),
      )
    }

    descriptors += [
      {
        offset: offset,
        length: length,
        descriptor_type: descriptor_type,
        raw: data[offset..offset + length],
      },
    ]
    offset += length
  }

  descriptors
}

## Separates the two membership headers without splitting a colon in the pathname.
export pure parse_unified_cgroup_path(value: Str) -> UnifiedCgroupPath {
  return {state: report.Malformed, path: null, has_v1: false} when value == ""

  var found: Str? = null
  var has_v1 = false
  for line in value.lines() {
    let fields = line.split(":", maxsplit: 2)
    if fields.len() != 3 or ! src.decimal_digits(fields[0]) or ! fields[2].starts_with("/") {
      return {state: report.Malformed, path: null, has_v1: false}
    }

    let hierarchy = fields[0].parse_int() ?? -1
    if hierarchy < 0 or hierarchy > 9007199254740991 or (hierarchy == 0 and fields[1] != "") or (hierarchy != 0 and fields[1] == "") {
      return {state: report.Malformed, path: null, has_v1: false}
    }

    if hierarchy == 0 {
      guard found == null else {
        return {state: report.Malformed, path: null, has_v1: false}
      }

      found = fields[2]
    } else {
      has_v1 = true
    }
  }

  return {state: report.Unsupported, path: null, has_v1: has_v1} when found == null

  {state: report.Observed, path: found, has_v1: has_v1}
}

## Chooses the most specific visible cgroup mount whose root contains the membership path.
export pure select_cgroup_mount(group_path: Str, mounts: List[CgroupMount]) -> Result[CgroupMount?, Error] {
  guard group_path.starts_with("/") else {
    return Err(SystemReportSourceError.InvalidCgroupMount(message: "cgroup membership path is not absolute"))
  }

  var selected: CgroupMount? = null
  var selected_root_length = -1
  for mount in mounts {
    if ! mount.root.starts_with("/") or ! mount.point.starts_with("/") {
      return Err(SystemReportSourceError.InvalidCgroupMount(message: "cgroup mount root or point is not absolute"))
    }

    let contains_group = mount.root == "/" or group_path == mount.root or group_path.starts_with(f"{mount.root}/")
    if contains_group and mount.root.count_chars() > selected_root_length {
      selected = mount
      selected_root_length = mount.root.count_chars()
    }
  }

  selected
}

## Accepts the kernel's fixed two-decimal PSI percentage without special float values.
export pure valid_psi_average(value: Str) -> Bool {
  let parts = value.split(".")
  if parts.len() != 2 or ! src.decimal_digits(parts[0]) or parts[1].count_chars() != 2 or ! src.decimal_digits(parts[1]) {
    return false
  }

  if let Ok(whole) = parts[0].parse_int() {
    whole >= 0 and whole <= 100 and (whole < 100 or parts[1] == "00")
  } else {
    false
  }
}

## Parses the bracketed selected value without assuming a fixed policy vocabulary.
export pure parse_thp_policy(value: Str) -> Result[TransparentHugePagePolicy, Error] {
  guard value.lines().len() == 1 else {
    return Err(SystemReportSourceError.InvalidThpPolicy(message: "THP policy must contain one line"))
  }

  let choices = value.replace("\t", " ").split(" ") |> where .trim() != ""
  var available: List[Str] = []
  var selected: Str? = null
  for choice in choices {
    var name = choice
    if choice.starts_with("[") and choice.ends_with("]") and choice.count_chars() >= 3 {
      guard selected == null else {
        return Err(SystemReportSourceError.InvalidThpPolicy(message: "THP policy has multiple selected values"))
      }

      name = choice.split("")
        |> drop(1)
        |> take(choice.count_chars() - 2).join("")
      selected = name
    }

    if name == "" or name.split("[").len() != 1 or name.split("]").len() != 1 or name in available {
      return Err(SystemReportSourceError.InvalidThpPolicy(message: "THP policy has an invalid or repeated value"))
    }

    available += [name]
  }

  if selected == null {
    return Err(SystemReportSourceError.InvalidThpPolicy(message: "THP policy has no selected value"))
  }

  Ok({selected: selected, available: available})
}

## Decodes one os-release value as data and withholds malformed quoting.
export pure decode_os_release_value(raw: Str) -> Str? {
  let value = raw
  return "" when value == ""

  let single_quoted = value.starts_with("'")
  let quoted = value.starts_with("\"")
  if single_quoted or quoted {
    let delimiter = if single_quoted { "'" } else { "\"" }
    return null when value.count_chars() < 2 or ! value.ends_with(delimiter)
  } else if value.ends_with("'") or value.ends_with("\"") {
    return null
  }

  if single_quoted {
    let content = value.split("")
      |> drop(1)
      |> take(value.count_chars() - 2).join("")
    return null when "'" in content

    return content
  }

  let content = if quoted {
    value.split("")
      |> drop(1)
      |> take(value.count_chars() - 2).join("")
  } else {
    value
  }
  var output = ""
  var escaped = false
  for character in content {
    if escaped {
      if character in ["$", "`", "\"", "\\"] {
        output = f"{output}{character}"
      } else if ! quoted and character not in "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._-" {
        return null
      } else {
        output = f"{output}\\{character}"
      }

      escaped = false
    } else if character == "\\" {
      escaped = true
    } else if character in ["$", "`", "\""] or (! quoted and character not in "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._-") {
      return null
    } else {
      output = f"{output}{character}"
    }
  }

  return null when escaped

  output
}

## Accepts only shell assignment identifiers for os-release keys.
export pure valid_os_release_key(key: Str) -> Bool {
  return false when key == ""

  let characters = key.split("")
  if characters[0] not in "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_" {
    return false
  }

  for character in characters {
    if character not in "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_0123456789" {
      return false
    }
  }

  true
}

## Accepts only lowercase os-release identities suitable for script and filename use.
export pure valid_os_release_id(value: Str) -> Bool {
  return false when value == ""

  for character in value {
    return false when character not in "0123456789abcdefghijklmnopqrstuvwxyz._-"
  }

  true
}

## Decodes a device-tree string list only when every element has its NUL terminator.
export pure decode_device_tree_strings(raw: Str) -> List[Str]? {
  var values: List[Str] = []
  var current = ""
  for character in raw {
    if character == "\0" {
      return null when current == ""

      values += [current]
      current = ""
    } else {
      current = current + character
    }
  }

  return null when current != "" or values.len() == 0

  values
}

## Parses the two complete decimal counters in /proc/uptime and retains exact whole seconds.
export pure parse_uptime_seconds(source: SourceRead) -> BoundedNumber {
  let observed = source.observation
  if observed.state != report.Observed {
    return {value: null, state: observed.state, error_kind: source.error_kind, errno: source.errno}
  }

  let columns = (observed.value ?? "").replace("\t", " ").split(" ") |> where .trim() != ""
  if columns.len() != 2 {
    return {value: null, state: report.Malformed, error_kind: "invalid_uptime_columns", errno: null}
  }

  for column in columns {
    let parts = column.split(".")
    if parts.len() != 2 or ! src.decimal_digits(parts[0]) or ! src.decimal_digits(parts[1]) {
      return {value: null, state: report.Malformed, error_kind: "invalid_uptime_decimal", errno: null}
    }
  }

  let seconds = columns[0].split(".")[0]
  src.bounded_number({...source, observation: {...observed, value: seconds}}, true)
}

## Converts kernel size suffixes only after a complete read and bounds the byte value.
export pure bounded_size_bytes(source: SourceRead) -> BoundedNumber {
  let observed = source.observation
  return src.bounded_number(source, true) when observed.state != report.Observed

  let raw = observed.value ?? ""
  var number_text = raw
  var multiplier = 1
  var maximum = 9007199254740991
  if raw.ends_with("K") {
    number_text = raw.split("") |> take(raw.count_chars() - 1).join("")
    multiplier = 1024
    maximum = 8796093022207
  } else if raw.ends_with("M") {
    number_text = raw.split("") |> take(raw.count_chars() - 1).join("")
    multiplier = 1048576
    maximum = 8589934591
  } else if raw.ends_with("G") {
    number_text = raw.split("") |> take(raw.count_chars() - 1).join("")
    multiplier = 1073741824
    maximum = 8388607
  }

  let parsed = src.bounded_number({...source, observation: {...observed, value: number_text}}, true)
  guard let number = parsed.value else {
    return parsed
  }
  if number > maximum {
    return {value: null, state: report.RangeFailure, error_kind: "json_integer_out_of_range", errno: null}
  }

  {value: number * multiplier, state: null, error_kind: null, errno: null}
}

## Parses the complete domain:bus:device.function sysfs identity.
export pure parse_pci_address(value: Str) -> Result[PciAddress, Error] {
  match sys_pci.parse_address(value) {
    Ok(address) => Ok(address)
    Err(error) => Err(SystemReportSourceError.InvalidPciAddress(message: error.message), cause: error)
  }
}

## Parses one hexadecimal PCI sysfs identifier without converting it to text labels.
export pure parse_pci_hex_value(value: Str) -> Result[Int, Error] {
  match sys_pci.parse_hex_value(value) {
    Ok(parsed) => Ok(parsed)
    Err(error) => Err(SystemReportSourceError.InvalidPciId(message: error.message), cause: error)
  }
}

## Parses a nonnegative PCI decimal attribute within JSON's exact integer range.
export pure parse_pci_decimal_value(value: Str) -> Result[Int, Error] {
  match sys_pci.parse_decimal_value(value) {
    Ok(parsed) => Ok(parsed)
    Err(error) => Err(SystemReportSourceError.InvalidPciId(message: error.message), cause: error)
  }
}

## Resolves the bridge immediately before a PCI function in its sysfs path.
export pure pci_parent_address(target: Path, child_address: Str) -> Str? {
  sys_pci.parent_bridge_address(target, child_address)
}

## Reads bounded text; see `sys_source.read_source_text`.
export proc read_source_text(
  root: FsRoot,
  source_path: Path,
  max_bytes: Int = 65536,
  preserve_whitespace: Bool = false,
) [fs, error] -> SourceRead {
  src.read_source_text(root, source_path, max_bytes, preserve_whitespace)
}

## Parses only complete source observations; see `sys_source.bounded_number`.
export pure bounded_number(source: SourceRead, nonnegative: Bool) -> BoundedNumber {
  src.bounded_number(source, nonnegative)
}

## Collects PCI functions with section-tagged issues for the report.
export proc collect_pci(root: FsRoot) [fs, error] -> PciCollection {
  let inventory = sys_pci.collect(root)
  {
    status: inventory.status,
    functions: inventory.functions,
    issues: src.with_section("pci", inventory.issues),
  }
}

## Requires exactly one bracketed active scheduler in one complete sysfs row.
export pure parse_block_scheduler(value: Str) -> BlockScheduler? {
  sys_block.parse_scheduler(value)
}
