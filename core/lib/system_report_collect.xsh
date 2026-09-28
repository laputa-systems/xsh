##! Parses bounded Linux inventory source records for system-report.
use system_report as report

## Stores the four components of a PCI function address.
export type PciAddress = {
  domain: Int,
  bus: Int,
  device: Int,
  function: Int,
}

## Retains one bounded USB descriptor after validating its framing.
export type UsbDescriptorRecord = {
  offset: Int,
  length: Int,
  descriptor_type: Int,
  raw: Bytes,
}

## Retains PCI enumeration status, valid functions, and field-level issues.
export type PciCollection = {
  status: report.SectionStatus,
  functions: List[report.PciFunction],
  issues: List[report.CollectionIssue],
}

## Retains a source observation and stable read-error details.
export type SourceRead = {
  observation: report.TextObservation,
  errno: Int?,
  error_kind: Str?,
}

## Retains a bounded integer or the source, syntax, or range state that withheld it.
export type BoundedNumber = {value: Int?, state: report.ObservationState?, error_kind: Str?, errno: Int?}

## Keeps the selected transparent-huge-page policy alongside all kernel choices.
export type TransparentHugePagePolicy = {selected: Str, available: List[Str]}

## Keeps the active block scheduler with every scheduler offered by the device.
export type BlockScheduler = {active: Str, available: List[Str]}

type NumericRead = {
  value: Int?,
  state: report.ObservationState,
  errno: Int?,
  error_kind: Str?,
}

error SystemReportSourceError = InvalidPciAddress(message: Str) | InvalidPciId(message: Str) | InvalidUsbDescriptor(message: Str) | InvalidThpPolicy(message: Str)

pure is_hex_component(value: Str, width: Int) -> Bool {
  if value.count_chars() != width {
    return false
  }

  for digit in value.split("") {
    if digit not in ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f", "A", "B", "C", "D", "E", "F"] {
      return false
    }
  }

  return true
}

pure parse_hex_component(value: Str) -> Result[Int] {
  return f"0x${value}".parse_int()
}

## Parses the complete domain:bus:device.function sysfs identity.
export pure parse_pci_address(value: Str) -> Result[PciAddress] {
  let parts = value.split(":")
  if parts.len() != 3 or !is_hex_component(parts[0], 4) or !is_hex_component(parts[1], 2) {
    return Err(SystemReportSourceError.InvalidPciAddress(message: "PCI address has invalid domain or bus syntax"))
  }

  let device_function = parts[2].split(".")
  if device_function.len() != 2 or !is_hex_component(device_function[0], 2) or !is_hex_component(device_function[1], 1) {
    return Err(SystemReportSourceError.InvalidPciAddress(message: "PCI address has invalid device or function syntax"))
  }

  let domain = parse_hex_component(parts[0])?
  let bus = parse_hex_component(parts[1])?
  let device = parse_hex_component(device_function[0])?
  let function = parse_hex_component(device_function[1])?
  if device > 31 or function > 7 {
    return Err(SystemReportSourceError.InvalidPciAddress(message: "PCI device or function value exceeds its ABI range"))
  }

  return Ok({domain: domain, bus: bus, device: device, function: function})
}

## Parses one hexadecimal PCI sysfs identifier without converting it to text labels.
export pure parse_pci_hex_value(value: Str) -> Result[Int] {
  let text = value.trim()
  let digits = if text.starts_with("0x") or text.starts_with("0X") {
    (text.split("") |> drop(2)).join("")
  } else {
    text
  }

  if digits == "" or !is_hex_component(digits, digits.count_chars()) {
    return Err(SystemReportSourceError.InvalidPciId(message: "PCI identifier is not hexadecimal"))
  }

  let parsed = parse_hex_component(digits)?
  if parsed > 4294967295 {
    return Err(SystemReportSourceError.InvalidPciId(message: "PCI identifier exceeds the supported unsigned range"))
  }
  return Ok(parsed)
}

## Parses USB descriptor framing while preserving unknown descriptor payloads.
export pure parse_usb_descriptor_stream(data: Bytes) -> Result[List[UsbDescriptorRecord]] {
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
      return Err(SystemReportSourceError.InvalidUsbDescriptor(message: "USB descriptor length is smaller than its header"))
    }
    if length > remaining {
      return Err(SystemReportSourceError.InvalidUsbDescriptor(message: "USB descriptor extends beyond the available bytes"))
    }
    if descriptors.len() == max_descriptors {
      return Err(SystemReportSourceError.InvalidUsbDescriptor(message: "USB descriptor count exceeds the 65,536 descriptor limit"))
    }

    descriptors = descriptors.push({
      offset: offset,
      length: length,
      descriptor_type: descriptor_type,
      raw: data.slice(offset, length),
    })
    offset += length
  }

  return Ok(descriptors)
}

pure source_observation_state(state: Str, truncated: Bool) -> report.ObservationState {
  if truncated {
    return report.Truncated
  }

  match state {
    "observed" => return report.Observed
    "absent" => return report.Absent
    "permission_denied" => return report.PermissionDenied
    _ => return report.ReadFailure
  }
}

## Reads bounded text without treating absence or invalid UTF-8 as an empty value.
## Preserving whitespace keeps command-line token boundaries faithful to the source.
export proc read_source_text(root: FsRoot, source_path: Path, max_bytes: Int = 65536, preserve_whitespace: Bool = false) [fs, error] -> SourceRead {
  let raw = fs.root_read_result(root, source_path, max_bytes: max_bytes)?
  var state = source_observation_state(raw.state, raw.truncated)
  var value: Str? = null
  var raw_bytes_base64: Str? = null

  if raw.data != null {
    let data = raw.data
    match data.utf8() {
      Ok(text) => value = if preserve_whitespace {text} else {text.trim()}
      Err(_) => {
        if state == report.Observed {
          state = report.Malformed
        }
        raw_bytes_base64 = data.base64()
      }
    }
  } else if state == report.Observed {
    state = report.Malformed
  }

  return {
    observation: {
      state: state,
      value: value,
      raw_bytes_base64: raw_bytes_base64,
    },
    errno: raw.errno,
    error_kind: raw.error_kind,
  }
}

pure decimal_digits(value: Str) -> Bool {
  if value == "" {
    return false
  }
  for digit in value.split("") {
    if digit not in ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"] {
      return false
    }
  }
  return true
}

## Accepts the kernel's fixed two-decimal PSI percentage without special float values.
export pure valid_psi_average(value: Str) -> Bool {
  let parts = value.split(".")
  if parts.len() != 2 or !decimal_digits(parts[0]) or parts[1].count_chars() != 2 or !decimal_digits(parts[1]) {
    return false
  }
  match parts[0].parse_int() {
    Ok(whole) => return whole >= 0 and whole <= 100 and (whole < 100 or parts[1] == "00")
    Err(_) => return false
  }
}

## Parses the bracketed selected value without assuming a fixed policy vocabulary.
export pure parse_thp_policy(value: Str) -> Result[TransparentHugePagePolicy] {
  if value.lines().len() != 1 {
    return Err(SystemReportSourceError.InvalidThpPolicy(message: "THP policy must contain one line"))
  }
  let choices = value.replace("\t", " ").split(" ") |> where .trim() != ""
  var available: List[Str] = []
  var selected: Str? = null
  for choice in choices {
    var name = choice
    if choice.starts_with("[") and choice.ends_with("]") and choice.count_chars() >= 3 {
      if selected != null {
        return Err(SystemReportSourceError.InvalidThpPolicy(message: "THP policy has multiple selected values"))
      }
      name = (choice.split("") |> drop(1) |> take(choice.count_chars() - 2)).join("")
      selected = name
    }
    if name == "" or name.split("[").len() != 1 or name.split("]").len() != 1 or name in available {
      return Err(SystemReportSourceError.InvalidThpPolicy(message: "THP policy has an invalid or repeated value"))
    }
    available = available.push(name)
  }
  if selected == null {
    return Err(SystemReportSourceError.InvalidThpPolicy(message: "THP policy has no selected value"))
  }
  return Ok({selected: selected ?? "", available: available})
}

## Requires exactly one bracketed active scheduler in one complete sysfs row.
export pure parse_block_scheduler(value: Str) -> BlockScheduler? {
  if value.trim() == "" or value.lines().len() != 1 {return null}
  let choices = value.replace("\t", " ").split(" ") |> where .trim() != ""
  var available: List[Str] = []
  var active: Str? = null
  for choice in choices {
    var name = choice
    if choice.starts_with("[") and choice.ends_with("]") and choice.count_chars() >= 3 {
      if active != null {return null}
      name = (choice.split("") |> drop(1) |> take(choice.count_chars() - 2)).join("")
      active = name
    }
    if name == "" or name.contains("[") or name.contains("]") or name in available {return null}
    available = available.push(name)
  }
  if active == null {return null}
  return {active: active ?? "", available: available}
}

## Decodes one os-release value as data and withholds malformed quoting.
export pure decode_os_release_value(raw: Str) -> Str? {
  let value = raw
  if value == "" {
    return ""
  }
  let single_quoted = value.starts_with("'")
  let quoted = value.starts_with("\"")
  if single_quoted or quoted {
    let delimiter = if single_quoted {"'"} else {"\""}
    if value.count_chars() < 2 or !value.ends_with(delimiter) {
      return null
    }
  } else if value.ends_with("'") or value.ends_with("\"") {
    return null
  }
  if single_quoted {
    let content = (value.split("") |> drop(1) |> take(value.count_chars() - 2)).join("")
    if content.contains("'") {
      return null
    }
    return content
  }
  let content = if quoted {(value.split("") |> drop(1) |> take(value.count_chars() - 2)).join("")} else {value}
  var output = ""
  var escaped = false
  for character in content.split("") {
    if escaped {
      if character in ["$", "`", "\"", "\\"] {
        output = f"${output}${character}"
      } else if !quoted and !"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._-".contains(character) {
        return null
      } else {
        output = f"${output}\\${character}"
      }
      escaped = false
    } else if character == "\\" {
      escaped = true
    } else if character in ["$", "`", "\""] or (!quoted and !"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._-".contains(character)) {
      return null
    } else {
      output = f"${output}${character}"
    }
  }
  if escaped {
    return null
  }
  return output
}

## Accepts only shell assignment identifiers for os-release keys.
export pure valid_os_release_key(key: Str) -> Bool {
  if key == "" {
    return false
  }
  let characters = key.split("")
  if !"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_".contains(characters[0]) {
    return false
  }
  for character in characters {
    if !"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_0123456789".contains(character) {
      return false
    }
  }
  return true
}

## Accepts only lowercase os-release identities suitable for script and filename use.
export pure valid_os_release_id(value: Str) -> Bool {
  if value == "" {return false}
  for character in value.split("") {
    if !"0123456789abcdefghijklmnopqrstuvwxyz._-".contains(character) {
      return false
    }
  }
  return true
}

## Decodes a device-tree string list only when every element has its NUL terminator.
export pure decode_device_tree_strings(raw: Str) -> List[Str]? {
  var values: List[Str] = []
  var current = ""
  for character in raw.split("") {
    if character == "\0" {
      if current == "" {
        return null
      }
      values = values.push(current)
      current = ""
    } else {
      current = current + character
    }
  }
  if current != "" or values.len() == 0 {
    return null
  }
  return values
}

## Parses only complete source observations and keeps integers exact in JSON.
export pure bounded_number(source: SourceRead, nonnegative: Bool) -> BoundedNumber {
  let observed = source.observation
  if observed.state == report.Absent {
    return {value: null, state: null, error_kind: null, errno: null}
  }
  if observed.state != report.Observed {
    return {value: null, state: observed.state, error_kind: source.error_kind, errno: source.errno}
  }
  let raw = observed.value ?? ""
  let signed_digits = raw.starts_with("-") and decimal_digits((raw.split("") |> drop(1)).join(""))
  if !decimal_digits(raw) and !signed_digits {
    return {value: null, state: report.Malformed, error_kind: "invalid_integer", errno: null}
  }
  if nonnegative and raw.starts_with("-") {
    return {value: null, state: report.Malformed, error_kind: "negative_integer", errno: null}
  }
  match raw.parse_int() {
    Ok(number) => {
      if number < -9007199254740991 or number > 9007199254740991 {
        return {value: null, state: report.RangeFailure, error_kind: "json_integer_out_of_range", errno: null}
      }
      return {value: number, state: null, error_kind: null, errno: null}
    }
    Err(_) => {
      return {value: null, state: report.RangeFailure, error_kind: "integer_out_of_range", errno: null}
    }
  }
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
    if parts.len() != 2 or !decimal_digits(parts[0]) or !decimal_digits(parts[1]) {
      return {value: null, state: report.Malformed, error_kind: "invalid_uptime_decimal", errno: null}
    }
  }
  let seconds = columns[0].split(".")[0]
  return bounded_number({...source, observation: {...observed, value: seconds}}, true)
}

## Converts kernel size suffixes only after a complete read and bounds the byte value.
export pure bounded_size_bytes(source: SourceRead) -> BoundedNumber {
  let observed = source.observation
  if observed.state != report.Observed {
    return bounded_number(source, true)
  }
  let raw = observed.value ?? ""
  var number_text = raw
  var multiplier = 1
  var maximum = 9007199254740991
  if raw.ends_with("K") {
    number_text = (raw.split("") |> take(raw.count_chars() - 1)).join("")
    multiplier = 1024
    maximum = 8796093022207
  } else if raw.ends_with("M") {
    number_text = (raw.split("") |> take(raw.count_chars() - 1)).join("")
    multiplier = 1048576
    maximum = 8589934591
  } else if raw.ends_with("G") {
    number_text = (raw.split("") |> take(raw.count_chars() - 1)).join("")
    multiplier = 1073741824
    maximum = 8388607
  }
  let parsed = bounded_number({...source, observation: {...observed, value: number_text}}, true)
  if parsed.value == null {
    return parsed
  }
  let number = parsed.value ?? 0
  if number > maximum {
    return {value: null, state: report.RangeFailure, error_kind: "json_integer_out_of_range", errno: null}
  }
  return {value: number * multiplier, state: null, error_kind: null, errno: null}
}

pure source_issue(
  section: Str,
  field: Str,
  state: report.ObservationState,
  errno: Int?,
  error_kind: Str?,
) -> report.CollectionIssue {
  return {
    section: section,
    field: field,
    state: state,
    error_kind: error_kind,
    errno: errno,
    detail: {state: state, value: null, raw_bytes_base64: null},
  }
}

proc read_numeric_attribute(root: FsRoot, source_path: Path, hexadecimal: Bool) [fs, error] -> NumericRead {
  let source = read_source_text(root, source_path)
  var state = source.observation.state
  if state == report.Absent {
    state = report.Disappeared
  }
  if state != report.Observed {
    return {
      value: null,
      state: state,
      errno: source.errno,
      error_kind: source.error_kind,
    }
  }
  if source.observation.value == null {
    return {value: null, state: report.Malformed, errno: source.errno, error_kind: "invalid_text"}
  }

  let source_text = source.observation.value ?? ""
  let parsed = if hexadecimal {
    parse_pci_hex_value(source_text)
  } else {
    source_text.parse_int()
  }
  match parsed {
    Ok(value) => {
      if value < 0 {
        return {value: null, state: report.RangeFailure, errno: null, error_kind: "negative_value"}
      }
      return {value: value, state: report.Observed, errno: null, error_kind: null}
    }
    Err(_) => return {value: null, state: report.Malformed, errno: null, error_kind: "invalid_integer"}
  }
}

pure pci_section_state(enumeration: Str, issues: List[report.CollectionIssue]) -> report.SectionState {
  if enumeration == "absent" {
    return report.SectionAbsent
  }
  if enumeration == "truncated" {
    return report.SectionTruncated
  }
  if enumeration == "complete" and issues.len() == 0 {
    return report.Complete
  }
  return report.Partial
}

## Resolves the bridge immediately before a PCI function in its sysfs path.
export pure pci_parent_address(target: Path, child_address: Str) -> Str? {
  var previous: Str? = null
  for component in target.display().split("/") {
    if component == child_address {
      return previous
    }
    match parse_pci_address(component) {
      Ok(_) => previous = component
      Err(_) => {}
    }
  }
  return null
}

proc optional_link_name(root: FsRoot, source_path: Path) [fs, error] -> Str? {
  match fs.root_readlink(root, source_path) {
    Ok(target) => return target.name()
    Err(_) => return null
  }
}

## Collects PCI function identity fields from one rooted sysfs view.
export proc collect_pci(root: FsRoot) [fs, error] -> PciCollection {
  let listing = fs.root_children(root, p"sys/bus/pci/devices")?
  var issues: List[report.CollectionIssue] = []
  var functions: List[report.PciFunction] = []
  var parent_addresses: List[Str?] = []

  if listing.state != "complete" {
    let state = source_observation_state(listing.state, false)
    issues = issues.push(source_issue(
      "pci",
      "functions",
      state,
      listing.errno,
      listing.error_kind,
    ))
  }

  for device_path in listing.children {
    let address_text = device_path.name()
    let address_result = parse_pci_address(address_text)
    let valid_address = match address_result {
      Ok(_) => true
      Err(_) => false
    }
    if !valid_address {
      issues = issues.push(source_issue(
        "pci",
        f"functions.${address_text}",
        report.Malformed,
        null,
        "invalid_pci_address",
      ))
      continue
    }
    let address = address_result?
    let vendor = read_numeric_attribute(root, fp"${device_path}/vendor", true)
    let device = read_numeric_attribute(root, fp"${device_path}/device", true)
    let subsystem_vendor = read_numeric_attribute(root, fp"${device_path}/subsystem_vendor", true)
    let subsystem_device = read_numeric_attribute(root, fp"${device_path}/subsystem_device", true)
    let class = read_numeric_attribute(root, fp"${device_path}/class", true)
    let revision = read_numeric_attribute(root, fp"${device_path}/revision", true)

    if vendor.state != report.Observed {
      issues = issues.push(source_issue("pci", f"functions.${address_text}.vendor_id", vendor.state, vendor.errno, vendor.error_kind))
    }
    if device.state != report.Observed {
      issues = issues.push(source_issue("pci", f"functions.${address_text}.device_id", device.state, device.errno, device.error_kind))
    }
    if subsystem_vendor.state != report.Observed {
      issues = issues.push(source_issue("pci", f"functions.${address_text}.subsystem_vendor_id", subsystem_vendor.state, subsystem_vendor.errno, subsystem_vendor.error_kind))
    }
    if subsystem_device.state != report.Observed {
      issues = issues.push(source_issue("pci", f"functions.${address_text}.subsystem_device_id", subsystem_device.state, subsystem_device.errno, subsystem_device.error_kind))
    }
    if class.state != report.Observed {
      issues = issues.push(source_issue("pci", f"functions.${address_text}.class_code", class.state, class.errno, class.error_kind))
    }
    if revision.state != report.Observed {
      issues = issues.push(source_issue("pci", f"functions.${address_text}.revision", revision.state, revision.errno, revision.error_kind))
    }
    let driver_exists = fs.root_exists(root, fp"${device_path}/driver")?
    let driver = if driver_exists {
      optional_link_name(root, fp"${device_path}/driver")
    } else {
      null
    }
    let parent_target = match fs.root_readlink(root, device_path) {
      Ok(target) => pci_parent_address(target, address_text)
      Err(_) => null
    }
    let numa = read_numeric_attribute(root, fp"${device_path}/numa_node", false)
    let iommu_group = optional_link_name(root, fp"${device_path}/iommu_group")
    let current_link_speed = read_source_text(root, fp"${device_path}/current_link_speed", max_bytes: 4096)
    let current_link_width = read_numeric_attribute(root, fp"${device_path}/current_link_width", false)
    let maximum_link_speed = read_source_text(root, fp"${device_path}/max_link_speed", max_bytes: 4096)
    let maximum_link_width = read_numeric_attribute(root, fp"${device_path}/max_link_width", false)
    if numa.state != report.Observed and numa.state != report.Absent and numa.state != report.Disappeared {
      issues = issues.push(source_issue("pci", f"functions.${address_text}.numa_node", numa.state, numa.errno, numa.error_kind))
    }
    if current_link_speed.observation.state != report.Observed and current_link_speed.observation.state != report.Absent {
      issues = issues.push(source_issue("pci", f"functions.${address_text}.current_link_speed", current_link_speed.observation.state, current_link_speed.errno, current_link_speed.error_kind))
    }
    if current_link_width.state != report.Observed and current_link_width.state != report.Disappeared {
      issues = issues.push(source_issue("pci", f"functions.${address_text}.current_link_width", current_link_width.state, current_link_width.errno, current_link_width.error_kind))
    }
    if maximum_link_speed.observation.state != report.Observed and maximum_link_speed.observation.state != report.Absent {
      issues = issues.push(source_issue("pci", f"functions.${address_text}.maximum_link_speed", maximum_link_speed.observation.state, maximum_link_speed.errno, maximum_link_speed.error_kind))
    }
    if maximum_link_width.state != report.Observed and maximum_link_width.state != report.Disappeared {
      issues = issues.push(source_issue("pci", f"functions.${address_text}.maximum_link_width", maximum_link_width.state, maximum_link_width.errno, maximum_link_width.error_kind))
    }

    var numa_node: Int? = null
    if numa.value != -1 {
      numa_node = numa.value
    }
    functions = functions.push({
      address: address_text,
      domain: address.domain,
      bus: address.bus,
      device: address.device,
      function: address.function,
      vendor_id: vendor.value,
      device_id: device.value,
      subsystem_vendor_id: subsystem_vendor.value,
      subsystem_device_id: subsystem_device.value,
      class_code: class.value,
      revision: revision.value,
      driver: driver,
      parent_function_index: null,
      numa_node: numa_node,
      iommu_group: iommu_group,
      current_link_speed: current_link_speed.observation.value,
      current_link_width: current_link_width.value,
      maximum_link_speed: maximum_link_speed.observation.value,
      maximum_link_width: maximum_link_width.value,
    })
    parent_addresses = parent_addresses.push(parent_target)
  }

  var linked_functions: List[report.PciFunction] = []
  var function_index = 0
  while function_index < functions.len() {
    let parent_address = parent_addresses[function_index]
    var parent_index: Int? = null
    if parent_address != null {
      var candidate_index = 0
      while candidate_index < functions.len() {
        if functions[candidate_index].address == parent_address {
          parent_index = candidate_index
          break
        }
        candidate_index += 1
      }
    }
    linked_functions = linked_functions.push({
      ...functions[function_index],
      parent_function_index: parent_index,
    })
    function_index += 1
  }

  return {
    status: {
      state: pci_section_state(listing.state, issues),
      enumeration_succeeded: listing.enumeration_succeeded,
    },
    functions: linked_functions,
    issues: issues,
  }
}
