##! Typed PCI function inventory read from one rooted sysfs view.
##!
##! `collect` reads only `sys/bus/pci/devices` with bounded reads and returns one
##! record per valid function: numeric vendor, device, subsystem, class, and
##! revision identifiers, the bus address as both text and integers, the bound
##! driver, link speed and width, NUMA node, IOMMU group, and the index of the
##! bridge that contains it. Names come from a label database that callers own.
##! The collector applies no redaction and no formatting; `system-report`,
##! `lspci`, and other renderers decide that.
use system_report as report
use sys_source as src

## Stores the four components of a PCI function address.
export type PciAddress = {
  domain: Int,
  bus: Int,
  device: Int,
  function: Int,
}

## Describes one PCI function; `parent_function_index` indexes the inventory's `functions`.
export type PciFunction = report.PciFunction

## Retains PCI enumeration status, valid functions, and field-level issues.
export type PciInventory = {
  status: report.SectionStatus,
  functions: List[PciFunction],
  issues: List[src.Issue],
}

## Failures returned by PCI identifier parsing.
export error SysPciError = InvalidAddress(message: Str) | InvalidId(message: Str)

type NumericRead = {
  value: Int?,
  state: report.ObservationState,
  errno: Int?,
  error_kind: Str?,
}

pure is_hex_component(value: Str, width: Int) -> Bool {
  guard value.count_chars() == width else {
    return false
  }

  for digit in value {
    if digit not in [
      "0",
      "1",
      "2",
      "3",
      "4",
      "5",
      "6",
      "7",
      "8",
      "9",
      "a",
      "b",
      "c",
      "d",
      "e",
      "f",
      "A",
      "B",
      "C",
      "D",
      "E",
      "F",
    ] {
      return false
    }
  }

  true
}

pure parse_hex_component(value: Str) -> Result[Int] {
  f"0x{value}".parse_int()
}

## Parses the complete domain:bus:device.function sysfs identity.
export pure parse_address(value: Str) -> Result[PciAddress] {
  let parts = value.split(":")
  if parts.len() != 3 or ! is_hex_component(parts[0], 4) or ! is_hex_component(parts[1], 2) {
    return Err(SysPciError.InvalidAddress(message: "PCI address has invalid domain or bus syntax"))
  }

  let device_function = parts[2].split(".")
  if device_function.len() != 2 or ! is_hex_component(device_function[0], 2) or ! is_hex_component(
    device_function[1],
    1,
  ) {
    return Err(SysPciError.InvalidAddress(message: "PCI address has invalid device or function syntax"))
  }

  let domain = parse_hex_component(parts[0])?
  let bus = parse_hex_component(parts[1])?
  let device = parse_hex_component(device_function[0])?
  let function = parse_hex_component(device_function[1])?
  if device > 31 or function > 7 {
    return Err(SysPciError.InvalidAddress(message: "PCI device or function value exceeds its ABI range"))
  }

  Ok({domain: domain, bus: bus, device: device, function: function})
}

## Parses one hexadecimal PCI sysfs identifier without converting it to text labels.
export pure parse_hex_value(value: Str) -> Result[Int] {
  let text = value.trim()
  let digits = if text.starts_with("0x") or text.starts_with("0X") {
    text.split("") |> drop(2).join("")
  } else {
    text
  }

  if digits == "" or ! is_hex_component(digits, digits.count_chars()) {
    return Err(SysPciError.InvalidId(message: "PCI identifier is not hexadecimal"))
  }

  let parsed = parse_hex_component(digits)?
  if parsed > 4294967295 {
    return Err(SysPciError.InvalidId(message: "PCI identifier exceeds the supported unsigned range"))
  }

  parsed
}

## Parses a nonnegative PCI decimal attribute within JSON's exact integer range.
export pure parse_decimal_value(value: Str) -> Result[Int] {
  if value == "" {
    return Err(SysPciError.InvalidId(message: "PCI decimal attribute is empty"))
  }

  for digit in value {
    if digit not in "0123456789" {
      return Err(SysPciError.InvalidId(message: "PCI decimal attribute is not unsigned decimal"))
    }
  }

  let parsed = value.parse_int()?
  if parsed > 9007199254740991 {
    return Err(
      SysPciError.InvalidId(message: "PCI decimal attribute exceeds the exact JSON integer range"),
    )
  }

  parsed
}

proc read_numeric_attribute(
  root: FsRoot,
  source_path: Path,
  hexadecimal: Bool,
  allow_unknown_numa: Bool = false,
) [fs, error] -> NumericRead {
  let source = src.read_source_text(root, source_path)
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

  let source_text = source.observation.value
  let parsed = if hexadecimal {
    parse_hex_value(source_text)
  } else if allow_unknown_numa and source_text == "-1" {
    Ok(-1)
  } else {
    parse_decimal_value(source_text)
  }
  if let Ok(value) = parsed {
    if value < 0 and ! (allow_unknown_numa and value == -1) {
      return {value: null, state: report.RangeFailure, errno: null, error_kind: "negative_value"}
    }

    {value: value, state: report.Observed, errno: null, error_kind: null}
  } else {
    {value: null, state: report.Malformed, errno: null, error_kind: "invalid_integer"}
  }
}

pure section_state(enumeration: Str, issues: List[src.Issue]) -> report.SectionState {
  return report.SectionAbsent when enumeration == "absent"

  return report.SectionTruncated when enumeration == "truncated"

  return report.Complete when enumeration == "complete" and issues.len() == 0

  report.Partial
}

## Resolves the bridge immediately before a PCI function in its sysfs path.
export pure parent_bridge_address(target: Path, child_address: Str) -> Str? {
  var previous: Str? = null
  for component in target.display().split("/") {
    return previous when component == child_address

    match parse_address(component) {
      Ok(_) => previous = component
      Err(_) => {}
    }
  }

  null
}

proc optional_link_name(root: FsRoot, source_path: Path) [fs, error] -> src.SourceRead {
  let source = root.readlink_result(source_path)?
  let state = src.source_state(source.state, false)
  if state != report.Observed {
    return {
      observation: {
        state: state,
        value: null,
        raw_bytes_base64: null,
      },
      errno: source.errno,
      error_kind: source.error_kind,
    }
  }

  if source.target == null or source.target.require(Path)?.name() == "" {
    return {
      observation: {
        state: report.Malformed,
        value: null,
        raw_bytes_base64: null,
      },
      errno: null,
      error_kind: "invalid_link_target",
    }
  }

  {
    observation: {
      state: report.Observed,
      value: source.target.require(Path)?.name(),
      raw_bytes_base64: null,
    },
    errno: null,
    error_kind: null,
  }
}

## Collects every PCI function from `sys/bus/pci/devices` with typed identity, links, and field issues.
## A malformed function never discards its neighbors; relationships resolve to indexes into `functions`.
export proc collect(root: FsRoot) [fs, error] -> PciInventory {
  let listing = root.children(p"sys/bus/pci/devices")?
  var issues: List[src.Issue] = []
  var functions: List[report.PciFunction] = []
  var parent_addresses: List[Str?] = []

  if listing.state != "complete" {
    let state = src.source_state(listing.state, false)
    issues = issues.push(
      src.issue("functions", state, listing.error_kind, listing.errno),
    )
  }

  for device_path in listing.children {
    let address_text = device_path.name()
    let address_result = parse_address(address_text)
    let valid_address = address_result is Ok(_)
    if ! valid_address {
      issues = issues.push(
        src.issue(f"functions.{address_text}", report.Malformed, "invalid_pci_address", null),
      )
      continue
    }

    let address = address_result?
    let vendor = read_numeric_attribute(root, fp"{device_path}/vendor", true)
    let device = read_numeric_attribute(root, fp"{device_path}/device", true)
    let subsystem_vendor = read_numeric_attribute(root, fp"{device_path}/subsystem_vendor", true)
    let subsystem_device = read_numeric_attribute(root, fp"{device_path}/subsystem_device", true)
    let class = read_numeric_attribute(root, fp"{device_path}/class", true)
    let revision = read_numeric_attribute(root, fp"{device_path}/revision", true)

    if vendor.state != report.Observed {
      issues = issues.push(
        src.issue(f"functions.{address_text}.vendor_id", vendor.state, vendor.error_kind, vendor.errno),
      )
    }

    if device.state != report.Observed {
      issues = issues.push(
        src.issue(f"functions.{address_text}.device_id", device.state, device.error_kind, device.errno),
      )
    }

    if subsystem_vendor.state != report.Observed {
      issues = issues.push(
        src.issue(f"functions.{address_text}.subsystem_vendor_id", subsystem_vendor.state, subsystem_vendor.error_kind, subsystem_vendor.errno),
      )
    }

    if subsystem_device.state != report.Observed {
      issues = issues.push(
        src.issue(f"functions.{address_text}.subsystem_device_id", subsystem_device.state, subsystem_device.error_kind, subsystem_device.errno),
      )
    }

    if class.state != report.Observed {
      issues = issues.push(
        src.issue(f"functions.{address_text}.class_code", class.state, class.error_kind, class.errno),
      )
    }

    if revision.state != report.Observed {
      issues = issues.push(
        src.issue(f"functions.{address_text}.revision", revision.state, revision.error_kind, revision.errno),
      )
    }

    let driver = optional_link_name(root, fp"{device_path}/driver")
    let parent_source = root.readlink_result(device_path)?
    var parent_target: Str? = null
    if parent_source.state == "observed" and parent_source.target != null {
      parent_target = parent_bridge_address(parent_source.target, address_text)
    } else if parent_source.state == "observed" {
      issues = issues.push(
        src.issue(f"functions.{address_text}.parent_function_index", report.Malformed, "invalid_parent_target", null),
      )
    } else {
      issues = issues.push(
        src.issue(f"functions.{address_text}.parent_function_index", src.source_state(parent_source.state, false), parent_source.error_kind, parent_source.errno),
      )
    }

    let numa = read_numeric_attribute(root, fp"{device_path}/numa_node", false, allow_unknown_numa: true)
    let iommu_group = optional_link_name(root, fp"{device_path}/iommu_group")
    let current_link_speed = src.read_source_text(root, fp"{device_path}/current_link_speed", max_bytes: 4096)
    let current_link_width = read_numeric_attribute(root, fp"{device_path}/current_link_width", false)
    let maximum_link_speed = src.read_source_text(root, fp"{device_path}/max_link_speed", max_bytes: 4096)
    let maximum_link_width = read_numeric_attribute(root, fp"{device_path}/max_link_width", false)
    if numa.state != report.Observed and numa.state != report.Absent and numa.state != report.Disappeared {
      issues = issues.push(
        src.issue(f"functions.{address_text}.numa_node", numa.state, numa.error_kind, numa.errno),
      )
    }

    if driver.observation.state != report.Observed and driver.observation.state != report.Absent {
      issues = issues.push(
        src.issue(f"functions.{address_text}.driver", driver.observation.state, driver.error_kind, driver.errno),
      )
    }

    if iommu_group.observation.state != report.Observed and iommu_group.observation.state != report.Absent {
      issues = issues.push(
        src.issue(f"functions.{address_text}.iommu_group", iommu_group.observation.state, iommu_group.error_kind, iommu_group.errno),
      )
    }

    if current_link_speed.observation.state != report.Observed and current_link_speed.observation.state != report.Absent {
      issues = issues.push(
        src.issue(f"functions.{address_text}.current_link_speed", current_link_speed.observation.state, current_link_speed.error_kind, current_link_speed.errno),
      )
    }

    if current_link_width.state != report.Observed and current_link_width.state != report.Disappeared {
      issues = issues.push(
        src.issue(f"functions.{address_text}.current_link_width", current_link_width.state, current_link_width.error_kind, current_link_width.errno),
      )
    }

    if maximum_link_speed.observation.state != report.Observed and maximum_link_speed.observation.state != report.Absent {
      issues = issues.push(
        src.issue(f"functions.{address_text}.maximum_link_speed", maximum_link_speed.observation.state, maximum_link_speed.error_kind, maximum_link_speed.errno),
      )
    }

    if maximum_link_width.state != report.Observed and maximum_link_width.state != report.Disappeared {
      issues = issues.push(
        src.issue(f"functions.{address_text}.maximum_link_width", maximum_link_width.state, maximum_link_width.error_kind, maximum_link_width.errno),
      )
    }

    var numa_node: Int? = null
    if numa.value != -1 {
      numa_node = numa.value
    }

    functions = functions.push(
      {
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
        driver: driver.observation.value,
        parent_function_index: null,
        numa_node: numa_node,
        iommu_group: iommu_group.observation.value,
        current_link_speed: if current_link_speed.observation.state == report.Observed {
          current_link_speed.observation.value
        } else {
          null
        },
        current_link_width: current_link_width.value,
        maximum_link_speed: if maximum_link_speed.observation.state == report.Observed {
          maximum_link_speed.observation.value
        } else {
          null
        },
        maximum_link_width: maximum_link_width.value,
      },
    )
    parent_addresses += [parent_target]
  }

  var function_index_by_address: Map[Int] = {}
  for index in range(functions.len()) {
    let address = functions[index].address
    if address != null {
      if address not in function_index_by_address {
        function_index_by_address = function_index_by_address.set(address, index)
      }
    }
  }

  var linked_functions: List[report.PciFunction] = []
  var function_index = 0
  while function_index < functions.len() {
    let parent_address = parent_addresses[function_index]
    var parent_index: Int? = null
    if parent_address != null {
      if parent_address in function_index_by_address {
        parent_index = function_index_by_address.get(parent_address)?
      }
    }

    linked_functions = linked_functions.push({
      ...functions[function_index],
      parent_function_index: parent_index,
    })
    function_index += 1
  }

  {
    status: {
      state: section_state(listing.state, issues),
      enumeration_succeeded: listing.enumeration_succeeded,
    },
    functions: linked_functions,
    issues: issues,
  }
}

## Finds the last PCI function address in a sysfs symlink target.
export pure address_in_target(target: Path) -> Str? {
  var address: Str? = null
  for component in target.display().split("/") {
    match parse_address(component) {
      Ok(_) => address = component
      Err(_) => {}
    }
  }

  address
}

## Preserves the first BDF index for joins without rescanning the function list.
export pure function_indices(functions: List[PciFunction]) -> Map[Int] {
  var indices: Map[Int] = {}
  for index in range(functions.len()) {
    let address = functions[index].address ?? ""
    if functions[index].address != null and address not in indices {
      indices = indices.set(address, index)
    }
  }

  indices
}

## Looks up a function index by address in a map built by `function_indices`.
export pure function_index(indices: Map[Int], address: Str?) -> Int? {
  return null when address == null or address not in indices

  indices.get(address) ?? 0
}
