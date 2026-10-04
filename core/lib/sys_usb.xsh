##! Typed USB device inventory read from `sys/bus/usb/devices` in one rooted view.
##!
##! `collect` returns one record per device entry (including root hubs) with its
##! sysfs name, bus and device numbers, hexadecimal vendor, product, class, and
##! version identifiers, controller PCI address, port path, power state, string
##! descriptors, interfaces with bound drivers, and the alternate settings and
##! endpoints parsed from the bounded `descriptors` blob. Parent relationships
##! resolve by sysfs name to indexes into `devices`. Descriptor parsing is
##! strict about framing yet preserves unknown descriptor types. The collector
##! applies no redaction and no formatting.
use system_report as report
use sys_pci
use sys_source as src

## Describes one endpoint of an interface alternate setting.
export type UsbEndpoint = report.UsbEndpoint

## Describes one USB interface with its driver and alternate settings.
export type UsbInterface = report.UsbInterface

## Failures returned by USB descriptor parsing; `InvalidStream` is framing, `InvalidDescriptor` is structure.
export error SysUsbError = InvalidStream(message: Str) | InvalidDescriptor(message: Str)

## Describes one USB device; `parent_device_index` indexes the inventory's `devices`.
export type UsbDevice = {
  sysfs_name: Str,
  parent_device_index: Int?,
  controller_pci_address: Str?,
  port_path: Str?,
  bus_number: Int?,
  device_number: Int?,
  vendor_id: Int?,
  product_id: Int?,
  device_version: Str?,
  class_code: Int?,
  subclass: Int?,
  protocol: Int?,
  manufacturer: report.TextObservation,
  product: report.TextObservation,
  serial: report.TextObservation,
  speed_mbps: Str?,
  configuration_count: Int?,
  active_configuration: Int?,
  power_control: Str?,
  autosuspend_delay_ms: Int?,
  runtime_status: Str?,
  is_root_hub: Bool,
  interfaces: List[UsbInterface],
}

## Retains USB devices, the bus listing state, and field-level issues.
export type UsbInventory = {
  listing_state: Str,
  enumeration_succeeded: Bool,
  devices: List[UsbDevice],
  issues: List[src.Issue],
}

## Separates a USB bus entry's controller relation from source-read failures.
export type ControllerObservation = {
  address: Str?,
  state: report.ObservationState,
  errno: Int?,
  error_kind: Str?,
}

## Retains one bounded USB descriptor after validating its framing.
export type DescriptorRecord = {
  offset: Int,
  length: Int,
  descriptor_type: Int,
  raw: Bytes,
}

## Retains the configuration that owns each parsed USB interface setting.
export type DescriptorAlternate = {
  configuration_value: Int?,
  interface_number: Int,
  setting_number: Int,
  class_code: Int,
  subclass: Int,
  protocol: Int,
  endpoints: List[report.UsbEndpoint],
}

## Parses USB descriptor framing while preserving unknown descriptor payloads.
export pure parse_descriptor_stream(data: Bytes) -> Result[List[DescriptorRecord]] {
  let max_bytes = 1048576
  let max_descriptors = 65536
  if data.len() > max_bytes {
    return Err(SysUsbError.InvalidStream(message: "USB descriptor input exceeds the 1 MiB limit"))
  }

  var descriptors: List[DescriptorRecord] = []
  var offset = 0
  while offset < data.len() {
    let remaining = data.len() - offset
    if remaining < 2 {
      return Err(SysUsbError.InvalidStream(message: "USB descriptor header is truncated"))
    }

    let length = bytes.unpack_le(data, 1, offset)?
    let descriptor_type = bytes.unpack_le(data, 1, offset + 1)?
    if length < 2 {
      return Err(
        SysUsbError.InvalidStream(message: "USB descriptor length is smaller than its header"),
      )
    }

    if length > remaining {
      return Err(
        SysUsbError.InvalidStream(message: "USB descriptor extends beyond the available bytes"),
      )
    }

    if descriptors.len() == max_descriptors {
      return Err(
        SysUsbError.InvalidStream(message: "USB descriptor count exceeds the 65,536 descriptor limit"),
      )
    }

    descriptors = descriptors.push({
      offset: offset,
      length: length,
      descriptor_type: descriptor_type,
      raw: data[offset..offset + length],
    })
    offset += length
  }

  descriptors
}

## Parses interface settings and endpoints without joining identical numbers across configurations.
export proc parse_alternates(data: Bytes) [error] -> Result[List[DescriptorAlternate]] {
  let records = parse_descriptor_stream(data)?
  var alternates: List[DescriptorAlternate] = []
  var current_configuration: Int? = null
  var current_configuration_end: Int? = null
  var active: DescriptorAlternate? = null
  for descriptor in records {
    if current_configuration_end != null {
      let configuration_end = current_configuration_end
      if descriptor.descriptor_type == 1 or descriptor.descriptor_type == 2 {
        guard descriptor.offset == configuration_end else {
          return Err(
            SysUsbError.InvalidDescriptor(
              message: "USB configuration descriptor bytes do not match their declared total length",
            ),
          )
        }
      } else if descriptor.offset >= configuration_end or descriptor.offset + descriptor.length > configuration_end {
        return Err(SysUsbError.InvalidDescriptor(message: "USB descriptor extends outside its configuration"))
      }
    }

    if descriptor.descriptor_type == 1 {
      guard descriptor.length >= 18 else {
        return Err(
          SysUsbError.InvalidDescriptor(message: "USB device descriptor is shorter than its fixed header"),
        )
      }

      if active != null {
        alternates += [active]
      }

      current_configuration = null
      current_configuration_end = null
      active = null
      continue
    }

    if descriptor.descriptor_type == 2 {
      guard descriptor.length >= 9 else {
        return Err(
          SysUsbError.InvalidDescriptor(message: "USB configuration descriptor is shorter than its fixed header"),
        )
      }

      let total_length = bytes.unpack_le(descriptor.raw, 2, 2)?
      if total_length < descriptor.length or total_length > data.len() - descriptor.offset {
        return Err(
          SysUsbError.InvalidDescriptor(
            message: "USB configuration total length is outside the available descriptor bytes",
          ),
        )
      }

      if active != null {
        alternates += [active]
      }

      current_configuration = bytes.unpack_le(descriptor.raw, 1, 5)?
      current_configuration_end = descriptor.offset + total_length
      active = null
      continue
    }

    if descriptor.descriptor_type == 4 {
      guard descriptor.length >= 9 else {
        return Err(
          SysUsbError.InvalidDescriptor(message: "USB interface descriptor is shorter than its fixed header"),
        )
      }

      let interface_number = bytes.unpack_le(descriptor.raw, 1, 2)?
      let setting_number = bytes.unpack_le(descriptor.raw, 1, 3)?
      if active != null {
        alternates += [active]
      }

      active = {
        configuration_value: current_configuration,
        interface_number: interface_number,
        setting_number: setting_number,
        class_code: bytes.unpack_le(descriptor.raw, 1, 5)?,
        subclass: bytes.unpack_le(descriptor.raw, 1, 6)?,
        protocol: bytes.unpack_le(descriptor.raw, 1, 7)?,
        endpoints: [],
      }
      continue
    }

    continue when descriptor.descriptor_type != 5
    if descriptor.length < 7 {
      return Err(
        SysUsbError.InvalidDescriptor(message: "USB endpoint descriptor is truncated or has no owning interface"),
      )
    }

    if active == null {
      return Err(
        SysUsbError.InvalidDescriptor(message: "USB endpoint descriptor is truncated or has no owning interface"),
      )
    }

    let current = active
    let address = bytes.unpack_le(descriptor.raw, 1, 2)?
    let attributes = bytes.unpack_le(descriptor.raw, 1, 3)?
    let packet_size = bytes.unpack_le(descriptor.raw, 2, 4)?
    let interval = bytes.unpack_le(descriptor.raw, 1, 6)?
    let transfer_type = match attributes % 4 {
      0 => "control",
      1 => "isochronous",
      2 => "bulk",
      _ => "interrupt",
    }
    let endpoint: report.UsbEndpoint = report.UsbEndpoint(
      address:,
      direction: if address >= 128 {
        "in"
      } else {
        "out"
      },
      transfer_type:,
      max_packet_size: packet_size,
      interval:,
    )
    active = {...current, endpoints: current.endpoints.push(endpoint)}
  }

  if active != null {
    alternates += [active]
  }

  if current_configuration_end != null and current_configuration_end != data.len() {
    return Err(
      SysUsbError.InvalidDescriptor(
        message: "USB configuration descriptor bytes do not match their declared total length",
      ),
    )
  }

  alternates
}

## Names the parent of a device entry: a hub port prefix or the bus root hub.
export pure parent_name(name: Str, bus_number: Int?) -> Str? {
  return null when name.starts_with("usb") or bus_number == null

  let bus_id = bus_number ?? -1
  let prefix = f"{bus_id}-"
  return null unless name.starts_with(prefix)

  let components = name.split(".")
  return f"usb{bus_id}" when components.len() <= 1

  components |> take(components.len() - 1).join(".")
}

pure port_path(name: Str) -> Str? {
  return null when name.starts_with("usb")

  let parts = name.split("-")
  return null when parts.len() < 2

  parts[1]
}

## Preserves the first sysfs-name index for joins without rescanning the device list.
export pure name_indices(names: List[Str?]) -> Map[Int] {
  var indices: Map[Int] = {}
  for index in range(names.len()) {
    let name = names[index] ?? ""
    if names[index] != null and name not in indices {
      indices = indices.set(name, index)
    }
  }

  indices
}

## Resolves each device's parent index after every device has been enumerated.
export pure parent_indices(names: List[Str?], bus_numbers: List[Int?]) -> List[Int?] {
  let by_name = name_indices(names)
  var parents: List[Int?] = []
  for index in range(names.len()) {
    let parent = parent_name(names[index] ?? "", bus_numbers[index])
    var parent_index: Int? = null
    if parent != null {
      if parent in by_name {
        if let Ok(found) = by_name.get(parent) {
          parent_index = found
        }
      }
    }

    parents = parents.push(parent_index)
  }

  parents
}

pure link_parents(devices: List[UsbDevice]) -> List[UsbDevice] {
  let parents = parent_indices([device.sysfs_name for device in devices], [device.bus_number for device in devices])
  [{...devices[index], parent_device_index: parents[index]} for index in range(devices.len())]
}

## Reads a USB bus-entry link and accepts direct directories in rooted fixtures.
export proc controller_address(root: FsRoot, device_path: Path) [fs, error] -> ControllerObservation {
  if let Ok(observed) = root.readlink_result(device_path) {
    if observed.state == "observed" {
      guard observed.target != null else {
        return {address: null, state: report.Malformed, errno: null, error_kind: "missing_controller_target"}
      }

      return {
        address: sys_pci.address_in_target(observed.target),
        state: report.Observed,
        errno: null,
        error_kind: null,
      }
    }

    if observed.state == "absent" {
      return {address: null, state: report.Disappeared, errno: observed.errno, error_kind: observed.error_kind}
    }

    if observed.error_kind == "invalid_input" and src.is_directory(root, device_path) {
      return {address: null, state: report.Observed, errno: null, error_kind: null}
    }

    {
      address: null,
      state: src.source_state(observed.state, false),
      errno: observed.errno,
      error_kind: observed.error_kind,
    }
  } else {
    {address: null, state: report.Malformed, errno: null, error_kind: "invalid_usb_source_path"}
  }
}

pure hex_optional(value: Str?, width: Int) -> Int? {
  guard value != null else {
    return null
  }

  return null when value.byte_len() != width

  if let Ok(parsed) = sys_pci.parse_hex_value(value) {
    parsed
  } else {
    null
  }
}

pure decimal_optional(value: Str?, minimum: Int) -> Int? {
  let parsed = src.parse_integer(value)
  return null when parsed == null

  let number = parsed
  return null when number < minimum or number > 9007199254740991

  parsed
}

## Collects every USB bus entry with typed identity, interfaces, descriptors, and field issues.
## Controller PCI addresses and parent links are identity; indexes are into the returned `devices`.
export proc collect(root: FsRoot) [fs, error] -> UsbInventory {
  let listing = root.children(p"sys/bus/usb/devices", max_entries: 4096)?
  var devices: List[UsbDevice] = []
  var issues: List[src.Issue] = []
  if listing.state != "complete" {
    issues = issues.push(
      src.issue("devices", src.source_state(listing.state, false), listing.error_kind, listing.errno),
    )
  }

  for device_path in listing.children {
    continue when ":" in device_path.name()
    let vendor = src.read_source_text(root, fp"{device_path}/idVendor", max_bytes: 4096)
    let product = src.read_source_text(root, fp"{device_path}/idProduct", max_bytes: 4096)
    let vendor_id = hex_optional(src.observed_text(vendor), 4)
    let product_id = hex_optional(src.observed_text(product), 4)
    if vendor.observation.state != report.Observed {
      issues = issues.push(
        src.issue(f"devices.{device_path.name()}.vendor_id", vendor.observation.state, vendor.error_kind, vendor.errno),
      )
    } else if vendor_id == null {
      issues = issues.push(
        src.issue(f"devices.{device_path.name()}.vendor_id", report.Malformed, "invalid_usb_vendor_id", null),
      )
    }

    if product.observation.state != report.Observed {
      issues = issues.push(
        src.issue(f"devices.{device_path.name()}.product_id", product.observation.state, product.error_kind, product.errno),
      )
    } else if product_id == null {
      issues = issues.push(
        src.issue(f"devices.{device_path.name()}.product_id", report.Malformed, "invalid_usb_product_id", null),
      )
    }

    let bus = src.read_source_text(root, fp"{device_path}/busnum", max_bytes: 4096)
    let number = src.read_source_text(root, fp"{device_path}/devnum", max_bytes: 4096)
    let version = src.read_source_text(root, fp"{device_path}/bcdDevice", max_bytes: 4096)
    let class = src.read_source_text(root, fp"{device_path}/bDeviceClass", max_bytes: 4096)
    let subclass = src.read_source_text(root, fp"{device_path}/bDeviceSubClass", max_bytes: 4096)
    let protocol = src.read_source_text(root, fp"{device_path}/bDeviceProtocol", max_bytes: 4096)
    let manufacturer = src.read_source_text(root, fp"{device_path}/manufacturer", max_bytes: 4096)
    let product_text = src.read_source_text(root, fp"{device_path}/product", max_bytes: 4096)
    let serial = src.read_source_text(root, fp"{device_path}/serial", max_bytes: 4096)
    let speed = src.read_source_text(root, fp"{device_path}/speed", max_bytes: 4096)
    let configurations = src.read_source_text(root, fp"{device_path}/bNumConfigurations", max_bytes: 4096)
    let active_configuration = src.read_source_text(root, fp"{device_path}/bConfigurationValue", max_bytes: 4096)
    let power_control = src.read_source_text(root, fp"{device_path}/power/control", max_bytes: 4096)
    let autosuspend = src.read_source_text(root, fp"{device_path}/power/autosuspend_delay_ms", max_bytes: 4096)
    let runtime_status = src.read_source_text(root, fp"{device_path}/power/runtime_status", max_bytes: 4096)
    let class_code = hex_optional(src.observed_text(class), 2)
    let subclass_code = hex_optional(src.observed_text(subclass), 2)
    let protocol_code = hex_optional(src.observed_text(protocol), 2)
    let version_code = hex_optional(src.observed_text(version), 4)
    let version_value: Str? = if version_code == null { null } else { src.observed_text(version) }
    for named_value in [
      {
        name: "class_code",
        source: class,
        value: class_code,
      },
      {
        name: "subclass",
        source: subclass,
        value: subclass_code,
      },
      {
        name: "protocol",
        source: protocol,
        value: protocol_code,
      },
      {
        name: "device_version",
        source: version,
        value: version_code,
      },
    ] {
      if named_value.source.observation.state == report.Observed and named_value.value == null {
        issues = issues.push(
          src.issue(f"devices.{device_path.name()}.{named_value.name}", report.Malformed, "invalid_usb_hex_value", null),
        )
      }
    }

    for named_source in [
      {
        name: "bus_number",
        source: bus,
      },
      {
        name: "device_number",
        source: number,
      },
      {
        name: "device_version",
        source: version,
      },
      {
        name: "class_code",
        source: class,
      },
      {
        name: "subclass",
        source: subclass,
      },
      {
        name: "protocol",
        source: protocol,
      },
      {
        name: "manufacturer",
        source: manufacturer,
      },
      {
        name: "product",
        source: product_text,
      },
      {
        name: "serial",
        source: serial,
      },
      {
        name: "speed_mbps",
        source: speed,
      },
      {
        name: "configuration_count",
        source: configurations,
      },
      {
        name: "active_configuration",
        source: active_configuration,
      },
    ] {
      issues = src.append_text_issue(issues, f"devices.{device_path.name()}.{named_source.name}",
        named_source.source,
      )
    }

    if power_control.observation.state != report.Observed and power_control.observation.state != report.Absent {
      issues = issues.push(
        src.issue(f"devices.{device_path.name()}.power_control",
          power_control.observation.state,
          power_control.error_kind,
          power_control.errno,
        ),
      )
    }

    if autosuspend.observation.state != report.Observed and autosuspend.observation.state != report.Absent {
      issues = issues.push(
        src.issue(f"devices.{device_path.name()}.autosuspend_delay_ms",
          autosuspend.observation.state,
          autosuspend.error_kind,
          autosuspend.errno,
        ),
      )
    }

    if runtime_status.observation.state != report.Observed and runtime_status.observation.state != report.Absent {
      issues = issues.push(
        src.issue(f"devices.{device_path.name()}.runtime_status",
          runtime_status.observation.state,
          runtime_status.error_kind,
          runtime_status.errno,
        ),
      )
    }

    let raw_descriptors = root.read_result(fp"{device_path}/descriptors", max_bytes: 1048576)?
    var descriptor_alternates: List[DescriptorAlternate] = []
    if raw_descriptors.truncated {
      issues = issues.push(
        src.issue(f"devices.{device_path.name()}.descriptors",
          report.Truncated,
          "descriptor_input_limit",
          raw_descriptors.errno,
        ),
      )
    } else if raw_descriptors.state == "observed" and raw_descriptors.data != null {
      if let Ok(alternates) = parse_alternates(raw_descriptors.data) {
        descriptor_alternates = alternates
      } else {
        issues = issues.push(
          src.issue(f"devices.{device_path.name()}.descriptors", report.Malformed, "invalid_usb_descriptor_stream", null),
        )
      }
    } else if raw_descriptors.state == "read_failure" or raw_descriptors.state == "permission_denied" {
      issues = issues.push(
        src.issue(f"devices.{device_path.name()}.descriptors",
          src.source_state(raw_descriptors.state, raw_descriptors.truncated),
          raw_descriptors.error_kind,
          raw_descriptors.errno,
        ),
      )
    }

    var interfaces: List[report.UsbInterface] = []
    for interface_path in listing.children {
      let interface_name = interface_path.name()
      continue unless interface_name.starts_with(f"{device_path.name()}:")
      let interface_number_text = (interface_name.split(":").get(1) ?? "").split(".").get(1) ?? ""
      let interface_number = src.parse_integer(interface_number_text) ?? -1
      if interface_number < 0 {
        issues = issues.push(
          src.issue(f"devices.{device_path.name()}.interfaces.{interface_name}",
            report.Malformed,
            "invalid_interface_name",
            null,
          ),
        )
        continue
      }

      let driver_link = src.driver_name(root, fp"{interface_path}/driver")
      if driver_link.observation.state != report.Observed and driver_link.observation.state != report.Absent {
        issues = issues.push(
          src.issue(f"devices.{device_path.name()}.interfaces.{interface_name}.driver",
            driver_link.observation.state,
            driver_link.error_kind,
            driver_link.errno,
          ),
        )
      }

      let active = src.read_source_text(root, fp"{interface_path}/bAlternateSetting", max_bytes: 4096)
      let active_alternate = decimal_optional(src.observed_text(active), 0)
      if active.observation.state == report.Observed and active_alternate == null {
        issues = issues.push(
          src.issue(f"devices.{device_path.name()}.interfaces.{interface_name}.active_alternate",
            report.Malformed,
            "invalid_usb_alternate",
            null,
          ),
        )
      } else if active.observation.state != report.Observed and active.observation.state != report.Absent {
        issues = src.append_text_issue(issues, f"devices.{device_path.name()}.interfaces.{interface_name}.active_alternate",
          active,
        )
      }

      var alternate_settings = [
        {
          configuration_value: alternate.configuration_value,
          number: alternate.setting_number,
          class_code: alternate.class_code,
          subclass: alternate.subclass,
          protocol: alternate.protocol,
          endpoints: alternate.endpoints,
        }
        for alternate in descriptor_alternates
        if alternate.interface_number == interface_number
      ]
      interfaces = interfaces.push({
        number: interface_number,
        name: interface_name,
        driver: src.observed_text(driver_link),
        active_alternate: active_alternate,
        alternate_settings: alternate_settings,
      })
    }

    let bus_number = decimal_optional(src.observed_text(bus), 1)
    let device_number = decimal_optional(src.observed_text(number), 1)
    let configuration_count = decimal_optional(src.observed_text(configurations), 0)
    let active_configuration_number = decimal_optional(src.observed_text(active_configuration), -1)
    let autosuspend_delay = decimal_optional(src.observed_text(autosuspend), -9007199254740991)
    for named_number in [
      {
        name: "bus_number",
        source: bus,
        value: bus_number,
      },
      {
        name: "device_number",
        source: number,
        value: device_number,
      },
      {
        name: "configuration_count",
        source: configurations,
        value: configuration_count,
      },
      {
        name: "active_configuration",
        source: active_configuration,
        value: active_configuration_number,
      },
      {
        name: "autosuspend_delay_ms",
        source: autosuspend,
        value: autosuspend_delay,
      },
    ] {
      if named_number.source.observation.state == report.Observed and named_number.value == null {
        issues = issues.push(
          src.issue(f"devices.{device_path.name()}.{named_number.name}", report.Malformed, "invalid_usb_number", null),
        )
      }
    }

    let controller = controller_address(root, device_path)
    if controller.state != report.Observed {
      issues = issues.push(
        src.issue(f"devices.{device_path.name()}.controller", controller.state, controller.error_kind, controller.errno),
      )
    }

    devices = devices.push({
      sysfs_name: device_path.name(),
      parent_device_index: null,
      controller_pci_address: controller.address,
      port_path: port_path(device_path.name()),
      bus_number: bus_number,
      device_number: device_number,
      vendor_id: vendor_id,
      product_id: product_id,
      device_version: version_value,
      class_code: class_code,
      subclass: subclass_code,
      protocol: protocol_code,
      manufacturer: manufacturer.observation,
      product: product_text.observation,
      serial: serial.observation,
      speed_mbps: src.observed_text(speed),
      configuration_count: configuration_count,
      active_configuration: active_configuration_number,
      power_control: src.observed_text(power_control),
      autosuspend_delay_ms: autosuspend_delay,
      runtime_status: src.observed_text(runtime_status),
      is_root_hub: device_path.name().starts_with("usb"),
      interfaces: interfaces,
    })
  }

  {
    listing_state: listing.state,
    enumeration_succeeded: listing.enumeration_succeeded,
    devices: link_parents(devices),
    issues: issues,
  }
}
