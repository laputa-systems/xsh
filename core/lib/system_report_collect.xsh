## Parses bounded Linux inventory source records for system-report.
use lib.system_report as report

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

export type PciCollection = {
  status: report.SectionStatus,
  functions: List[report.PciFunction],
  issues: List[report.CollectionIssue],
}

export type SourceRead = {
  observation: report.TextObservation,
  errno: Int?,
  error_kind: Str?,
}

type NumericRead = {
  value: Int?,
  state: report.ObservationState,
  errno: Int?,
  error_kind: Str?,
}

pure source_error(kind: Str, message: Str) -> Error {
  return Error(kind: kind, message: message)
}

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
    return Err(source_error("system-report-pci-address", "PCI address has invalid domain or bus syntax"))
  }

  let device_function = parts[2].split(".")
  if device_function.len() != 2 or !is_hex_component(device_function[0], 2) or !is_hex_component(device_function[1], 1) {
    return Err(source_error("system-report-pci-address", "PCI address has invalid device or function syntax"))
  }

  let domain = parse_hex_component(parts[0])?
  let bus = parse_hex_component(parts[1])?
  let device = parse_hex_component(device_function[0])?
  let function = parse_hex_component(device_function[1])?
  if device > 31 or function > 7 {
    return Err(source_error("system-report-pci-address", "PCI device or function value exceeds its ABI range"))
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
    return Err(source_error("system-report-pci-id", "PCI identifier is not hexadecimal"))
  }

  let parsed = parse_hex_component(digits)?
  if parsed > 4294967295 {
    return Err(source_error("system-report-pci-id", "PCI identifier exceeds the supported unsigned range"))
  }
  return Ok(parsed)
}

## Parses USB descriptor framing while preserving unknown descriptor payloads.
export pure parse_usb_descriptor_stream(data: Bytes) -> Result[List[UsbDescriptorRecord]] {
  let max_bytes = 1048576
  let max_descriptors = 65536
  if data.len() > max_bytes {
    return Err(source_error("system-report-usb-descriptor", "USB descriptor input exceeds the 1 MiB limit"))
  }

  var descriptors: List[UsbDescriptorRecord] = []
  var offset = 0
  while offset < data.len() {
    let remaining = data.len() - offset
    if remaining < 2 {
      return Err(source_error("system-report-usb-descriptor", "USB descriptor header is truncated"))
    }

    let length = bytes.unpack_le(data, 1, offset)?
    let descriptor_type = bytes.unpack_le(data, 1, offset + 1)?
    if length < 2 {
      return Err(source_error("system-report-usb-descriptor", "USB descriptor length is smaller than its header"))
    }
    if length > remaining {
      return Err(source_error("system-report-usb-descriptor", "USB descriptor extends beyond the available bytes"))
    }
    if descriptors.len() == max_descriptors {
      return Err(source_error("system-report-usb-descriptor", "USB descriptor count exceeds the 65,536 descriptor limit"))
    }

    descriptors = descriptors.push({
      offset: offset,
      length: length,
      descriptor_type: descriptor_type,
      raw: data.slice(offset, length)?,
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

export proc read_source_text(root: FsRoot, path: Path, max_bytes: Int = 65536) [fs, error] -> SourceRead {
  let raw = fs.root_read_result(root, path, max_bytes: max_bytes)?
  var state = source_observation_state(raw.state, raw.truncated)
  var value: Str? = null
  var raw_bytes_base64: Str? = null

  if raw.data != null {
    let data = raw.data
    match data.utf8() {
      Ok(text) => value = text.trim()
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

proc read_numeric_attribute(root: FsRoot, path: Path, hexadecimal: Bool) [fs, error] -> NumericRead {
  let source = read_source_text(root, path)?
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

  let parsed = if hexadecimal {
    parse_pci_hex_value(source.observation.value)
  } else {
    source.observation.value.parse_int()
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
    return report.Absent
  }
  if enumeration == "truncated" {
    return report.Truncated
  }
  if enumeration == "complete" and issues.len() == 0 {
    return report.Complete
  }
  return report.Partial
}

pure pci_parent_address(target: Path) -> Str? {
  var result: Str? = null
  for component in target.display().split("/") {
    match parse_pci_address(component) {
      Ok(_) => result = component
      Err(_) => continue
    }
  }
  return result
}

proc optional_link_name(root: FsRoot, path: Path) [fs, error] -> Str? {
  match fs.root_readlink(root, path) {
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
    match parse_pci_address(address_text) {
      Err(_) => {
        issues = issues.push(source_issue(
          "pci",
          f"functions.${address_text}",
          report.Malformed,
          null,
          "invalid_pci_address",
        ))
        continue
      }
      Ok(address) => {
        let vendor = read_numeric_attribute(root, fp"${device_path}/vendor", true)?
        let device = read_numeric_attribute(root, fp"${device_path}/device", true)?
        let subsystem_vendor = read_numeric_attribute(root, fp"${device_path}/subsystem_vendor", true)?
        let subsystem_device = read_numeric_attribute(root, fp"${device_path}/subsystem_device", true)?
        let class = read_numeric_attribute(root, fp"${device_path}/class", true)?
        let revision = read_numeric_attribute(root, fp"${device_path}/revision", true)?

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
          optional_link_name(root, fp"${device_path}/driver")?
        } else {
          null
        }
        let parent_target = if fs.root_exists(root, fp"${device_path}/device")? {
          match fs.root_readlink(root, fp"${device_path}/device") {
            Ok(target) => pci_parent_address(target)
            Err(_) => null
          }
        } else {
          null
        }
        let numa = read_numeric_attribute(root, fp"${device_path}/numa_node", false)?
        let iommu_group = optional_link_name(root, fp"${device_path}/iommu_group")?
        let current_link_speed = read_source_text(root, fp"${device_path}/current_link_speed", max_bytes: 4096)?
        let current_link_width = read_numeric_attribute(root, fp"${device_path}/current_link_width", false)?
        let maximum_link_speed = read_source_text(root, fp"${device_path}/max_link_speed", max_bytes: 4096)?
        let maximum_link_width = read_numeric_attribute(root, fp"${device_path}/max_link_width", false)?
        if numa.state != report.Observed and numa.state != report.Absent and numa.state != report.Disappeared {
          issues = issues.push(source_issue("pci", f"functions.${address_text}.numa_node", numa.state, numa.errno, numa.error_kind))
        }
        if current_link_speed.observation.state != report.Observed and current_link_speed.observation.state != report.Absent {
          issues = issues.push(source_issue("pci", f"functions.${address_text}.current_link_speed", current_link_speed.observation.state, current_link_speed.errno, current_link_speed.error_kind))
        }
        if maximum_link_speed.observation.state != report.Observed and maximum_link_speed.observation.state != report.Absent {
          issues = issues.push(source_issue("pci", f"functions.${address_text}.maximum_link_speed", maximum_link_speed.observation.state, maximum_link_speed.errno, maximum_link_speed.error_kind))
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
          numa_node: if numa.value == -1 {null} else {numa.value},
          iommu_group: iommu_group,
          current_link_speed: current_link_speed.observation.value,
          current_link_width: current_link_width.value,
          maximum_link_speed: maximum_link_speed.observation.value,
          maximum_link_width: maximum_link_width.value,
        })
        parent_addresses = parent_addresses.push(parent_target)
      }
    }
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
