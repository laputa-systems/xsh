##! Hardware inventory presentation over shared typed Linux collectors.
use gnu
use sys_pci as pci
use sys_usb as usb
use system_report as report
use system_report_live as live

## Reports invalid selectors, columns, label databases, or inventory relationships.
export error HardwareError = InvalidSelector | InvalidColumn | InvalidDatabase | InvalidInventory

## Selects PCI numeric identifiers, kernel driver details, verbosity and topology.
export type PciView = {numeric: Int, driver: Bool, verbose: Int, domain: Bool, tree: Bool}

## Selects USB descriptor or topology presentation.
export type UsbView = {verbose: Bool, tree: Bool}

## Retains PCI address selector components; null denotes a wildcard.
export type PciSelector = {domain: Int?, bus: Int?, device: Int?, function: Int?}

## Retains vendor, product and optional PCI class/program-interface selectors.
export type DeviceSelector = {vendor: Int?, product: Int?, class: Int?, program: Int?}

## Retains the decimal USB bus and device selector components.
export type UsbSelector = {bus: Int?, device: Int?}

## Stores one lscpu summary label and its presentation value.
export type SummaryField = {field: Str, data: Str}

## Formats an observed decimal number without turning missing identity into zero.
export pure decimal(value: Int?, width: Int) -> Str {
  if value == null { return ["?"] |> repeat(width).join("") }
  var text = f"{value}"
  while text.byte_len() < width { text = "0" + text }
  text
}

## Formats an observed hexadecimal identifier; question marks retain unknown values.
export pure hex(value: Int?, width: Int) -> Str {
  if value == null { return ["?"] |> repeat(width).join("") }
  var number = value
  var text = ""
  while number > 0 { text = "0123456789abcdef".byte_slice(number % 16, 1) + text; number /= 16 }
  while text.byte_len() < width { text = "0" + text }
  text
}

## Reads vendor, product and class labels from pci.ids or usb.ids grammar.
export pure parse_ids(text: Str) -> Result[Map[Str], Error] {
  var labels: Map[Str] = {}
  var vendor = ""
  var base_class = ""
  for raw in text.lines() {
    if raw.trim() == "" or raw.trim().starts_with("#") { continue }
    if raw.starts_with("\t\t") { continue }
    if raw.starts_with("C ") {
      let parts = raw.fields()
      if parts.len() < 3 { return Err(HardwareError.InvalidDatabase("class label has no name")) }
      base_class = parts[1].lower()
      vendor = ""
      labels[f"c:{base_class}"] = raw.byte_slice(1).trim().byte_slice(parts[1].byte_len()).trim()
    } else if raw.starts_with("\t") {
      let parts = raw.fields()
      if parts.len() < 2 { return Err(HardwareError.InvalidDatabase("device label has no name")) }
      let id = parts[0].lower()
      if vendor != "" { labels[f"{vendor}:{id}"] = raw.byte_slice(1).trim().byte_slice(parts[0].byte_len()).trim() } else if base_class != "" { labels[f"c:{base_class}{id}"] = raw.byte_slice(1).trim().byte_slice(parts[0].byte_len()).trim() }
    } else {
      let parts = raw.fields()
      if parts.len() >= 2 and rx"^[0-9a-fA-F]{4}$".matches(parts[0]) {
        vendor = parts[0].lower()
        base_class = ""
        labels[vendor] = raw.byte_slice(parts[0].byte_len()).trim()
      }
    }
  }
  labels
}

## Loads the first available conventional label database without embedding vendor data.
export proc load_labels(paths: List[Path]) -> Result[Map[Str], Error] {
  for file in paths {
    match file.read_text() {
      Ok(text) => return parse_ids(text)
      Err(failure) => { if gnu.errno(failure) != 2 { return Err(failure) } }
    }
  }
  let empty: Map[Str] = {}
  empty
}

pure component(text: Str, maximum: Int, hexadecimal: Bool) -> Result[Int?] {
  if text == "" or text == "*" { return null }
  let parsed = if hexadecimal { pci.parse_hex_value(text)? } else { text.parse_int()? }
  if parsed < 0 or parsed > maximum { return Err(HardwareError.InvalidSelector(f"selector component '{text}' is out of range")) }
  parsed
}

## Parses [[domain:]bus:]device[.function], retaining wildcard components.
export pure pci_selector(text: Str) -> Result[PciSelector, Error] {
  let parts = text.split(":")
  if parts.len() > 3 { return Err(HardwareError.InvalidSelector("too many PCI address components")) }
  let slot = parts[-1].split(".")
  if slot.len() > 2 { return Err(HardwareError.InvalidSelector("too many PCI function components")) }
  let domain = if parts.len() == 3 { component(parts[0], 65535, true)? } else { null }
  let bus = if parts.len() >= 2 { component(parts[-2], 255, true)? } else { null }
  let device = component(slot[0], 31, true)?
  let function = if slot.len() == 2 { component(slot[1], 7, true)? } else { null }
  {domain: domain, bus: bus, device: device, function: function}
}

## Parses vendor:product[:class[:program-interface]] hexadecimal selectors.
export pure id_selector(text: Str) -> Result[DeviceSelector, Error] {
  let parts = text.split(":")
  if parts.len() > 4 { return Err(HardwareError.InvalidSelector("too many identifier components")) }
  {vendor: component(parts[0], 65535, true)?, product: if parts.len() >= 2 { component(parts[1], 65535, true)? } else { null },
    class: if parts.len() >= 3 { component(parts[2], 65535, true)? } else { null },
    program: if parts.len() == 4 { component(parts[3], 255, true)? } else { null }}
}

## Applies independent PCI address and vendor:device[:class] selectors.
export pure pci_matches(device: pci.PciFunction, slot: Str, ids: Str) -> Result[Bool, Error] {
  let address = pci_selector(slot)?
  let filter = id_selector(ids)?
  (address.domain == null or address.domain == device.domain) and
    (address.bus == null or address.bus == device.bus) and
    (address.device == null or address.device == device.device) and
    (address.function == null or address.function == device.function) and
    (filter.vendor == null or filter.vendor == device.vendor_id) and
    (filter.product == null or filter.product == device.device_id) and
    (filter.class == null or (device.class_code != null and filter.class == device.class_code / 256)) and
    (filter.program == null or (device.class_code != null and filter.program == device.class_code % 256))
}

pure pci_line(device: pci.PciFunction, names: Map[Str], view: PciView) -> Str {
  let address = device.address ?? "????:??:??.?"
  let slot = if view.domain or device.domain != 0 { address } else { address.byte_slice(5) }
  let class_id = hex(if device.class_code != null { device.class_code / 256 } else { null }, 4)
  let vendor = hex(device.vendor_id, 4)
  let product = hex(device.device_id, 4)
  let class_name = names.get(f"c:{class_id}") ?? names.get(f"c:{class_id.byte_slice(0, 2)}") ?? class_id
  let vendor_name = names.get(vendor) ?? vendor
  let product_name = names.get(f"{vendor}:{product}") ?? product
  let identity = if vendor_name == vendor and product_name == product { f"{vendor}:{product}" } else { f"{vendor_name} {product_name}" }
  let description = if view.numeric == 1 { f"{class_id}: {vendor}:{product}" } else if view.numeric >= 2 { f"{class_name} [{class_id}]: {identity} [{vendor}:{product}]" } else { f"{class_name}: {identity}" }
  let revision = if device.revision != null { f" (rev {hex(device.revision, 2)})" } else { "" }
  f"{slot} {description}{revision}"
}

pure pci_branch(devices: List[pci.PciFunction], index: Int, names: Map[Str], view: PciView, prefix: Str, visited: List[Int]) -> Result[List[Str]] {
  if index in visited { return Err(HardwareError.InvalidInventory("PCI parent relationships contain a cycle")) }
  var lines = [f"{prefix}+-{pci_line(devices[index], names, view)}"]
  for child in range(devices.len()) {
    if devices[child].parent_function_index == index {
      lines += pci_branch(devices, child, names, view, prefix + "  ", visited + [index])?
    }
  }
  lines
}

## Renders PCI functions using collected identity, links, drivers and parent indexes.
export pure pci_lines(devices: List[pci.PciFunction], names: Map[Str], view: PciView) -> Result[List[Str], Error] {
  var lines: List[Str] = []
  if view.tree {
    for index in range(devices.len()) {
      if devices[index].parent_function_index == null { lines += pci_branch(devices, index, names, view, "", [])? }
    }
    if lines.len() != devices.len() { return Err(HardwareError.InvalidInventory("PCI inventory contains unresolved parent relationships")) }
    return lines
  }
  for device in devices |> sort-by .address ?? "" {
    lines += [pci_line(device, names, view)]
    if view.verbose > 0 {
      if device.subsystem_vendor_id != null or device.subsystem_device_id != null {
        lines += [f"\tSubsystem: {hex(device.subsystem_vendor_id, 4)}:{hex(device.subsystem_device_id, 4)}"]
      }
      if device.numa_node != null and device.numa_node >= 0 { lines += [f"\tNUMA node: {device.numa_node}"] }
      if device.current_link_speed != null or device.current_link_width != null {
        lines += [f"\tLnkSta: Speed {device.current_link_speed ?? "unknown"}, Width x{decimal(device.current_link_width, 1)}"]
      }
      if device.maximum_link_speed != null or device.maximum_link_width != null {
        lines += [f"\tLnkCap: Speed {device.maximum_link_speed ?? "unknown"}, Width x{decimal(device.maximum_link_width, 1)}"]
      }
      if device.iommu_group != null { lines += [f"\tIOMMU group: {device.iommu_group}"] }
    }
    if view.driver or view.verbose > 0 {
      if device.driver != null { lines += [f"\tKernel driver in use: {device.driver}"] }
    }
    if view.verbose > 0 { lines += [""] }
  }
  lines
}


## Parses a decimal [bus:]device USB selector without inspecting live inventory.
export pure usb_selector(slot: Str) -> Result[UsbSelector, Error] {
  let parts = slot.split(":")
  if parts.len() > 2 { return Err(HardwareError.InvalidSelector("USB selector accepts [bus:]device")) }
  {bus: if parts.len() == 2 { component(parts[0], 65535, false)? } else { null },
    device: component(parts[-1], 65535, false)?}
}

## Applies hexadecimal USB identifiers and decimal [bus:]device selectors.
export pure usb_matches(device: usb.UsbDevice, ids: Str, slot: Str) -> Result[Bool, Error] {
  if ids.split(":").len() > 2 { return Err(HardwareError.InvalidSelector("USB identifiers accept vendor:product only")) }
  let filter = id_selector(ids)?
  if filter.class != null or filter.program != null { return Err(HardwareError.InvalidSelector("USB identifiers accept vendor:product only")) }
  let selected = usb_selector(slot)?
  let bus = selected.bus
  let number = selected.device
  (filter.vendor == null or filter.vendor == device.vendor_id) and
    (filter.product == null or filter.product == device.product_id) and
    (bus == null or bus == device.bus_number) and (number == null or number == device.device_number)
}

pure usb_title(device: usb.UsbDevice, names: Map[Str]) -> Str {
  let vendor = hex(device.vendor_id, 4)
  let product = hex(device.product_id, 4)
  let title = f"{names.get(vendor) ?? device.manufacturer.value ?? vendor} {names.get(f"{vendor}:{product}") ?? device.product.value ?? product}"
  f"Bus {decimal(device.bus_number, 3)} Device {decimal(device.device_number, 3)}: ID {vendor}:{product} {title}"
}

pure usb_depth(devices: List[usb.UsbDevice], index: Int) -> Result[Int] {
  var at = index
  var seen: List[Int] = []
  var depth = 0
  while devices[at].parent_device_index != null {
    if at in seen { return Err(HardwareError.InvalidInventory("USB parent relationships contain a cycle")) }
    seen += [at]
    at = devices[at].parent_device_index ?? -1
    if at < 0 or at >= devices.len() { return Err(HardwareError.InvalidInventory("USB parent index is out of range")) }
    depth += 1
  }
  depth
}

## Renders USB identities, descriptor fields, interfaces, endpoints and topology.
export pure usb_lines(devices: List[usb.UsbDevice], names: Map[Str], view: UsbView) -> Result[List[Str], Error] {
  var lines: List[Str] = []
  for index in range(devices.len()) {
    let device = devices[index]
    if view.tree {
      let indent = ["    "] |> repeat(usb_depth(devices, index)?).join("")
      if device.is_root_hub { lines += [f"/:  Bus {decimal(device.bus_number, 3)}.Port 001: Dev {decimal(device.device_number, 1)}, Class=root_hub, {device.speed_mbps ?? "?"}M"] } else {
        for interface in device.interfaces {
          let matches = interface.alternate_settings |> where .number == interface.active_alternate
          let alternate: report.UsbAlternateSetting? = if matches.is_empty() { null } else { matches[0] }
          let class_name = if alternate != null { names.get(f"c:{hex(alternate.class_code, 2)}") ?? hex(alternate.class_code, 2) } else { "?" }
          lines += [f"{indent}|__ Port {device.port_path ?? device.sysfs_name}: Dev {decimal(device.device_number, 1)}, If {interface.number}, Class={class_name}, Driver={interface.driver ?? ""}, {device.speed_mbps ?? "?"}M"]
        }
        if device.interfaces.is_empty() { lines += [f"{indent}|__ Port {device.port_path ?? device.sysfs_name}: Dev {decimal(device.device_number, 1)}, {device.speed_mbps ?? "?"}M"] }
      }
      continue
    }
    lines += [usb_title(device, names)]
    if ! view.verbose { continue }
    lines += ["Device Descriptor:", f"  bDeviceClass        {decimal(device.class_code, 1)}", f"  bDeviceSubClass     {decimal(device.subclass, 1)}",
      f"  bDeviceProtocol     {decimal(device.protocol, 1)}", f"  idVendor           0x{hex(device.vendor_id, 4)}", f"  idProduct          0x{hex(device.product_id, 4)}"]
    if device.device_version != null { lines += [f"  bcdDevice          {device.device_version}"] }
    if device.manufacturer.value != null { lines += [f"  iManufacturer      {device.manufacturer.value}"] }
    if device.product.value != null { lines += [f"  iProduct           {device.product.value}"] }
    if device.serial.value != null { lines += [f"  iSerialNumber      {device.serial.value}"] }
    if device.configuration_count != null { lines += [f"  bNumConfigurations {device.configuration_count}"] }
    for interface in device.interfaces {
      for alternate in interface.alternate_settings {
        lines += ["    Interface Descriptor:", f"      bInterfaceNumber    {interface.number}", f"      bAlternateSetting   {alternate.number}",
          f"      bInterfaceClass     {alternate.class_code}", f"      bInterfaceSubClass  {alternate.subclass}", f"      bInterfaceProtocol  {alternate.protocol}"]
        for endpoint in alternate.endpoints {
          lines += ["      Endpoint Descriptor:", f"        bEndpointAddress   0x{hex(endpoint.address, 2)}", f"        wMaxPacketSize     {endpoint.max_packet_size}", f"        bInterval          {endpoint.interval}"]
        }
      }
    }
    lines += [""]
  }
  lines
}

## Collects CPU topology from a rooted live or synthetic source through the report collector.
export proc cpu_from_root(root: FsRoot, architecture: Str) -> Result[report.CpuSection, Error] {
  let units = system.execution_units()?
  live.collect_from_root(root, architecture, units.page_size_bytes, units.clock_ticks_per_second, "cpu", true)?.cpu
}

## Collects CPU topology without applying the report's sharing redactions.
export proc cpu_live() -> Result[report.CpuSection, Error] { live.collect_live("cpu", true)?.cpu }

## Collects hwmon channels through the existing bounded sensor collector.
export proc sensors_live() -> Result[report.SensorSection, Error] { live.collect_live("sensors", true)?.sensors }

## Collects hwmon channels from a caller-owned fixture or captured source root.
export proc sensors_from_root(root: FsRoot) -> Result[report.SensorSection, Error] {
  let units = system.execution_units()?
  live.collect_from_root(root, system.uname()?.machine, units.page_size_bytes, units.clock_ticks_per_second, "sensors", true)?.sensors
}

type CpuIdentifiers = {sockets: Map[Int], cores: Map[Int], dies: Map[Int], nodes: Map[Int], caches: Map[Map[Int]]}

# Logical topology IDs are assigned from zero; physical IDs stay as the kernel
# reported them. Core and die identities include their enclosing package.
pure cpu_identifiers(section: report.CpuSection) -> CpuIdentifiers {
  var ids: CpuIdentifiers = {sockets: {}, cores: {}, dies: {}, nodes: {}, caches: {}}
  for processor in section.cpus |> sort-by .id {
    if processor.package_id != null {
      let key = f"{processor.package_id}"
      if key not in ids.sockets { ids.sockets[key] = ids.sockets.len() }
      if processor.core_id != null {
        let core = f"{processor.package_id}:{processor.die_id ?? -1}:{processor.core_id}"
        if core not in ids.cores { ids.cores[core] = ids.cores.len() }
      }
      if processor.die_id != null {
        let die = f"{processor.package_id}:{processor.die_id}"
        if die not in ids.dies { ids.dies[die] = ids.dies.len() }
      }
    }
    if processor.numa_node != null {
      let key = f"{processor.numa_node}"
      if key not in ids.nodes { ids.nodes[key] = ids.nodes.len() }
    }
  }
  for index in range(section.caches.len()) {
    let cache = section.caches[index]
    let key = f"{cache.level}:{cache.kind.lower()}"
    var cache_group: Map[Int] = ids.caches.get(key) ?? {}
    cache_group[f"{index}"] = cache_group.len()
    ids.caches[key] = cache_group
  }
  ids
}

pure cpu_cell(section: report.CpuSection, processor: report.Cpu, column: Str, ids: CpuIdentifiers, physical: Bool) -> Result[Str] {
  match column {
    "CPU" => f"{processor.id}"
    "NODE" => if processor.numa_node == null { "-" } else if physical { f"{processor.numa_node}" } else { f"{ids.nodes.get(f"{processor.numa_node}")?}" }
    "SOCKET" => if processor.package_id == null { "-" } else if physical { f"{processor.package_id}" } else { f"{ids.sockets.get(f"{processor.package_id}")?}" }
    "CORE" => {
      if processor.core_id == null { return "-" }
      if physical { return f"{processor.core_id}" }
      if processor.package_id == null { return "-" }
      f"{ids.cores.get(f"{processor.package_id}:{processor.die_id ?? -1}:{processor.core_id}")?}"
    }
    "DIE" => {
      if processor.die_id == null { return "-" }
      if physical { return f"{processor.die_id}" }
      if processor.package_id == null { return "-" }
      f"{ids.dies.get(f"{processor.package_id}:{processor.die_id}")?}"
    }
    "ONLINE" => if processor.online == null { "-" } else if processor.online { "Y" } else { "N" }
    "CACHE" => {
      var cache_ids: List[Str] = []
      for index in processor.cache_indices {
        if index < 0 or index >= section.caches.len() { return Err(HardwareError.InvalidInventory("CPU cache index is out of range")) }
        let cache = section.caches[index]
        let cache_group = ids.caches.get(f"{cache.level}:{cache.kind.lower()}")?
        cache_ids += [f"{cache_group.get(f"{index}")?}"]
      }
      if cache_ids.is_empty() { "-" } else { cache_ids.join(":") }
    }
    "MHZ" | "MAXMHZ" | "MINMHZ" => {
      let policies = report.frequency_policies_for_cpu(section.frequency_policies, processor.id)
      if policies.is_empty() { return "-" }
      let policy = policies[0]
      let khz: Int? = if column == "MHZ" { if policy.scaling_current_khz != null { policy.scaling_current_khz } else { policy.hardware_current_khz } } else if column == "MAXMHZ" { policy.hardware_max_khz } else { policy.hardware_min_khz }
      if khz == null { "-" } else { f"{khz / 1000}.{khz % 1000:03}" }
    } else => Err(HardwareError.InvalidColumn(f"unsupported CPU column '{column}'"))
  }
}

## Renders explicit per-CPU columns, keeping missing topology distinct from CPU zero.
export pure cpu_rows(section: report.CpuSection, columns: List[Str], selection: Str, parse: Bool, physical = false) -> Result[List[Str], Error] {
  if columns.is_empty() { return Err(HardwareError.InvalidColumn("CPU column list is empty")) }
  if selection not in ["all", "online", "offline"] { return Err(HardwareError.InvalidSelector("unsupported CPU selection")) }
  for column in columns {
    if column.upper() not in ["CPU", "NODE", "SOCKET", "CORE", "DIE", "ONLINE", "CACHE", "MHZ", "MAXMHZ", "MINMHZ"] {
      return Err(HardwareError.InvalidColumn(f"unsupported CPU column '{column}'"))
    }
  }
  let checked = columns |> map { |column| column.upper() }
  if physical and "CACHE" in checked { return Err(HardwareError.InvalidColumn("physical cache IDs are not available")) }
  let ids = cpu_identifiers(section)
  var rows: List[List[Str]] = []
  for processor in section.cpus |> sort-by .id {
    if selection == "online" and processor.online != true { continue }
    if selection == "offline" and processor.online != false { continue }
    var cells: List[Str] = []
    for column in checked {
      let value = cpu_cell(section, processor, column, ids, physical)?
      if parse { cells += [if value == "-" { "" } else { value }] } else if column == "ONLINE" { cells += [if value == "Y" { "yes" } else if value == "N" { "no" } else { value }] } else { cells += [value] }
    }
    rows += [cells]
  }
  if parse { return ["# " + checked.join(",")] + [row.join(",") for row in rows] }
  aligned_lines(checked, rows, true)
}

pure cache_size(value: Int) -> Str {
  let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"]
  var index = 0
  var divisor = 1
  while value / divisor >= 1024 and index < units.len() - 1 { index += 1; divisor *= 1024 }
  if value % divisor == 0 { return f"{value / divisor} {units[index]}" }
  var text = quantity(value, divisor, false, 2)
  while text.ends_with("0") { text = text.byte_slice(0, text.byte_len() - 1) }
  if text.ends_with(".") { text = text.byte_slice(0, text.byte_len() - 1) }
  f"{text} {units[index]}"
}

## Renders summary field/data pairs suitable for human tables or lscpu JSON.
export pure cpu_summary(section: report.CpuSection, architecture: Str, bytes_mode = false) -> List[SummaryField] {
  var rows = [{field: "Architecture:", data: architecture}, {field: "CPU(s):", data: f"{section.cpus.len()}"},
    {field: "On-line CPU(s) list:", data: section.online |> map { |id| f"{id}" }.join(",")}]
  if ! section.offline.is_empty() { rows += [{field: "Off-line CPU(s) list:", data: section.offline |> map { |id| f"{id}" }.join(",")}] }
  if ! section.cpus.is_empty() {
    let first = section.cpus[0]
    if first.vendor != null { rows += [{field: "Vendor ID:", data: first.vendor}] }
    if first.model != null { rows += [{field: "Model name:", data: first.model}] }
    if first.family != null { rows += [{field: "CPU family:", data: first.family}] }
    if first.model_id != null { rows += [{field: "Model:", data: first.model_id}] }
    if first.stepping != null { rows += [{field: "Stepping:", data: first.stepping}] }
    if ! first.features.is_empty() { rows += [{field: "Flags:", data: first.features.join(" ")}] }
    if "vmx" in first.features { rows += [{field: "Virtualization:", data: "VT-x"}] } else if "svm" in first.features { rows += [{field: "Virtualization:", data: "AMD-V"}] }
  }
  let sockets = section.cpus |> where .package_id != null |> unique-by .package_id ?? -1
  let nodes = section.cpus |> where .numa_node != null |> unique-by .numa_node ?? -1
  if ! sockets.is_empty() { rows += [{field: "Socket(s):", data: f"{sockets.len()}"}] }
  if ! nodes.is_empty() { rows += [{field: "NUMA node(s):", data: f"{nodes.len()}"}] }
  var core_counts: List[Int] = []
  for socket in sockets {
    let cores = section.cpus |> where .package_id == socket.package_id |> where .core_id != null
      |> unique-by { |processor| f"{processor.die_id ?? -1}:{processor.core_id ?? -1}" }
    core_counts += [cores.len()]
  }
  let unique_cores = core_counts |> unique-by .
  if unique_cores.len() == 1 and unique_cores[0] > 0 { rows += [{field: "Core(s) per socket:", data: f"{unique_cores[0]}"}] }
  let thread_counts = section.cpus |> where { |processor| ! processor.thread_siblings.is_empty() } |> map .thread_siblings.len() |> unique-by .
  if thread_counts.len() == 1 { rows += [{field: "Thread(s) per core:", data: f"{thread_counts[0]}"}] }

  var cache_sizes: Map[Int] = {}
  var cache_counts: Map[Int] = {}
  var missing_sizes: Map[Bool] = {}
  var cache_order: List[Str] = []
  for cache in section.caches {
    let suffix = if cache.kind.lower() == "data" { "d" } else if cache.kind.lower() == "instruction" { "i" } else if cache.kind.lower() == "unified" { "" } else { cache.kind }
    let name = f"L{cache.level}{suffix} cache:"
    if name not in cache_order { cache_order += [name] }
    cache_counts = cache_counts.set(name, (cache_counts.get(name) ?? 0) + 1)
    if cache.size_bytes == null { missing_sizes = missing_sizes.set(name, true) } else { cache_sizes = cache_sizes.set(name, (cache_sizes.get(name) ?? 0) + cache.size_bytes) }
  }
  for name in cache_order {
    let count = cache_counts.get(name) ?? 0
    let size = if missing_sizes.get(name) == Ok(true) { "unknown" } else if bytes_mode { f"{cache_sizes.get(name) ?? 0}" } else { cache_size(cache_sizes.get(name) ?? 0) }
    let unit = if count == 1 { "instance" } else { "instances" }
    rows += [{field: name, data: f"{size} ({count} {unit})"}]
  }
  for vulnerability in section.vulnerabilities {
    if vulnerability.description.value != null { rows += [{field: f"Vulnerability {vulnerability.name}:", data: vulnerability.description.value}] }
  }
  rows
}

## Selects radio devices by numeric ID or conventional radio type aliases.
export pure rfkill_select(devices: List[LinuxRfkill], selectors: List[Str]) -> Result[List[LinuxRfkill], Error] {
  if selectors.is_empty() { return devices }
  var selected: List[LinuxRfkill] = []
  for selector in selectors {
    let radio = if selector == "wifi" { "wlan" } else { selector }
    let id = selector.parse_int()
    if id is Ok(_) and (id? < 0 or id? > 4294967295) { return Err(HardwareError.InvalidSelector("radio ID is out of range")) }
    if selector != "all" and radio not in ["wlan", "bluetooth", "uwb", "wimax", "wwan", "gps", "fm", "nfc"] and id is Err(_) {
      return Err(HardwareError.InvalidSelector(f"invalid radio selector '{selector}'"))
    }
    for device in devices {
      if selector == "all" or device.type == radio or (id is Ok(_) and device.id == id?) {
        if device not in selected { selected += [device] }
      }
    }
  }
  selected
}

pure radio_name(name: Str) -> Str {
  match name { "wlan" => "Wireless LAN", "bluetooth" => "Bluetooth", "wwan" => "Wireless WAN", "uwb" => "Ultra-Wideband", "gps" => "GPS", "fm" => "FM", "nfc" => "NFC", "wimax" => "WiMAX", else => name }
}

## Renders the traditional rfkill list blocks with both independent block states.
export pure rfkill_list_lines(devices: List[LinuxRfkill]) -> List[Str] {
  var lines: List[Str] = []
  for device in devices |> sort-by .id {
    lines += [f"{device.id}: {device.name}: {radio_name(device.type)}", f"\tSoft blocked: {if device.soft_blocked { "yes" } else { "no" }}",
      f"\tHard blocked: {if device.hard_blocked { "yes" } else { "no" }}"]
  }
  lines
}

## Applies only the explicitly selected software block state using native rfkill operations.
export proc rfkill_set(devices: List[LinuxRfkill], selectors: List[Str], blocked: Bool) -> Result[Unit, Error] {
  for selector in selectors {
    if let Ok(id) = selector.parse_int() {
      if ! (devices |> any .id == id) { return Err(HardwareError.InvalidSelector(f"radio ID '{selector}' was not found")) }
    }
  }
  for device in rfkill_select(devices, selectors)? {
    if blocked { linux.rfkill_block(device.id) } else { linux.rfkill_unblock(device.id) }
  }
}

pure quantity(value: Int, divisor: Int, signed: Bool, digits = 1) -> Str {
  let prefix = if value < 0 { "-" } else if signed { "+" } else { "" }
  let magnitude = if value < 0 { -value } else { value }
  var scale = 1
  for index in range(digits) { scale *= 10 }
  let fraction = (magnitude % divisor * scale + divisor / 2) / divisor
  f"{prefix}{magnitude / divisor + fraction / scale}.{decimal(fraction % scale, digits)}"
}

pure sensor_value(channel: report.SensorChannel, value: Int, fahrenheit: Bool) -> Str {
  match channel.kind {
    "temperature" => {
      let converted = if fahrenheit { value * 9 / 5 + 32000 } else { value }
      quantity(converted, 1000, true) + (if fahrenheit { "°F" } else { "°C" })
    }
    "voltage" => quantity(value, 1000, false, 3) + " V"
    "current" => quantity(value, 1000, false, 3) + " A"
    "power" => quantity(value, 1000000, false, 6) + " W"
    "energy" => quantity(value, 1000000, false, 6) + " J"
    "fan" => f"{value} RPM"
    else => f"{value} {channel.unit}"
  }
}

pure sensor_divisor(channel: report.SensorChannel) -> Int {
  if channel.kind in ["temperature", "voltage", "current"] { 1000 } else if channel.kind in ["power", "energy"] { 1000000 } else { 1 }
}

## Keeps two chips with the same driver label separate using their observed hwmon entries.
export pure sensor_chip_name(channel: report.SensorChannel) -> Str {
  if channel.chip_entry_name != null { f"{channel.chip}-{channel.chip_entry_name}" } else { channel.chip }
}

## Renders human sensor labels and limits or raw feature names in source units.
export pure sensor_lines(channels: List[report.SensorChannel], fahrenheit: Bool, raw: Bool, no_adapter = false) -> List[Str] {
  var lines: List[Str] = []
  let chips = channels |> map { |channel| sensor_chip_name(channel) } |> unique-by . |> sort()
  for chip in chips {
    lines += [chip]
    if ! no_adapter { lines += ["Adapter: Unknown adapter"] }
    for channel in channels |> where { |channel| sensor_chip_name(channel) == chip } |> sort-by .channel {
      let label = channel.label.value ?? channel.channel
      if raw {
        lines += [f"{label}:"]
        if channel.value != null { lines += [f"  {channel.channel}_input: {channel.value.float() / sensor_divisor(channel).float()}"] }
        if channel.minimum != null { lines += [f"  {channel.channel}_min: {channel.minimum.float() / sensor_divisor(channel).float()}"] }
        if channel.maximum != null { lines += [f"  {channel.channel}_max: {channel.maximum.float() / sensor_divisor(channel).float()}"] }
        if channel.critical != null { lines += [f"  {channel.channel}_crit: {channel.critical.float() / sensor_divisor(channel).float()}"] }
        if channel.alarm != null { lines += [f"  {channel.channel}_alarm: {if channel.alarm { 1 } else { 0 }}"] }
      } else {
        var line = f"{label}: {if channel.value != null { sensor_value(channel, channel.value, fahrenheit) } else { "N/A" }}"
        var limits: List[Str] = []
        if channel.minimum != null { limits += [f"low = {sensor_value(channel, channel.minimum, fahrenheit)}"] }
        if channel.maximum != null { limits += [f"high = {sensor_value(channel, channel.maximum, fahrenheit)}"] }
        if channel.critical != null { limits += [f"crit = {sensor_value(channel, channel.critical, fahrenheit)}"] }
        if ! limits.is_empty() { line += "  (" + limits.join(", ") + ")" }
        if channel.alarm == true { line += "  ALARM" }
        lines += [line]
      }
    }
    lines += [""]
  }
  lines
}

## Encodes native sensor feature values under escaped chip and label keys.
export pure sensor_json(channels: List[report.SensorChannel]) -> Result[Str, Error] {
  var output: Map[Map[Map[Float]]] = {}
  for channel in channels {
    let chip_name = sensor_chip_name(channel)
    let label = channel.label.value ?? channel.channel
    var chip = output.get(chip_name) ?? {}
    var features: Map[Float] = chip.get(label) ?? {}
    let divisor = sensor_divisor(channel)
    if channel.value != null { features = features.set(f"{channel.channel}_input", channel.value.float() / divisor.float()) }
    if channel.minimum != null { features = features.set(f"{channel.channel}_min", channel.minimum.float() / divisor.float()) }
    if channel.maximum != null { features = features.set(f"{channel.channel}_max", channel.maximum.float() / divisor.float()) }
    if channel.critical != null { features = features.set(f"{channel.channel}_crit", channel.critical.float() / divisor.float()) }
    if channel.alarm != null { features = features.set(f"{channel.channel}_alarm", if channel.alarm { 1.0 } else { 0.0 }) }
    chip = chip.set(label, features)
    output = output.set(chip_name, chip)
  }
  json.encode(output, pretty: true)
}

## Matches chip selectors with '*' and '?' without treating names as regular expressions.
export pure chip_matches(name: Str, selector: Str) -> Result[Bool, Error] {
  var expression = "^"
  for character in selector {
    if character in "[]" { return Err(HardwareError.InvalidSelector("chip character-class selectors are not available")) }
    if character == "*" { expression += ".*" } else if character == "?" { expression += "." } else {
      if character in ".+^$(){}|\\" { expression += "\\" }
      expression += character
    }
  }
  regex.compile(expression + "$")?.matches(name)
}

pure radio_cell(device: LinuxRfkill, column: Str) -> Result[Str] {
  match column.upper() {
    "ID" => f"{device.id}"
    "TYPE" => device.type
    "TYPE-DESC" => radio_name(device.type)
    "DEVICE" => device.name
    "SOFT" => if device.soft_blocked { "blocked" } else { "unblocked" }
    "HARD" => if device.hard_blocked { "blocked" } else { "unblocked" }
    else => Err(HardwareError.InvalidColumn(f"unsupported rfkill column '{column}'"))
  }
}

## Validates radio table columns even when no devices are present.
export pure radio_columns(value: Str) -> Result[List[Str], Error] {
  let columns = value.upper().split(",")
  for column in columns {
    if column not in ["ID", "TYPE", "TYPE-DESC", "DEVICE", "SOFT", "HARD"] { return Err(HardwareError.InvalidColumn(f"unsupported rfkill column '{column}'")) }
  }
  columns
}

pure radio_raw(value: Str) -> Str {
  var text = ""
  for character in value {
    let byte = bytes.from_text(character).byte_at(0) ?? 0
    if byte < 32 or byte == 127 or character in [" ", "\\"] { text += "\\x" + hex(byte, 2) } else { text += character }
  }
  text
}

pure table_row(values: List[Str], widths: List[Int]) -> Str {
  var pieces: List[Str] = []
  for index in range(values.len()) {
    let spaces = [" "] |> repeat(if index == values.len() - 1 { 0 } else { widths[index] - values[index].count_chars() })
    pieces += [values[index] + spaces.join("")]
  }
  pieces.join(" ")
}

pure aligned_lines(headers: List[Str], rows: List[List[Str]], headings: Bool) -> List[Str] {
  var lines: List[Str] = []
  var widths: List[Int] = []
  for index in range(headers.len()) {
    var width = headers[index].count_chars()
    for row in rows { if row[index].count_chars() > width { width = row[index].count_chars() } }
    widths += [width]
  }
  if headings { lines += [table_row(headers, widths)] }
  for row in rows { lines += [table_row(row, widths)] }
  lines
}

## Renders aligned radio tables or escaped raw rows, with optional headings.
export pure rfkill_table_lines(devices: List[LinuxRfkill], columns: List[Str], headings: Bool, raw: Bool) -> Result[List[Str], Error] {
  let checked = radio_columns(columns.join(","))?
  let rows = [[radio_cell(device, column)? for column in checked] for device in devices]
  var lines: List[Str] = []
  if raw {
    if headings { lines += [checked.join(" ")] }
    for row in rows { lines += [row |> map { |value| radio_raw(value) }.join(" ")] }
    return lines
  }
  aligned_lines(checked, rows, headings)
}

## Encodes numeric radio IDs and string columns without flattening their types.
export pure rfkill_json(devices: List[LinuxRfkill], columns: List[Str]) -> Result[Str, Error] {
  let checked = radio_columns(columns.join(","))?
  var records: List[Str] = []
  for device in devices {
    var fields: List[Str] = []
    for column in checked {
      let value = if column == "ID" { json.encode(device.id)? } else { json.encode(radio_cell(device, column)?)? }
      fields += [json.encode(column.lower())? + ":" + value]
    }
    records += ["{" + fields.join(",") + "}"]
  }
  "{\"rfkilldevices\":[" + records.join(",") + "]}"
}
