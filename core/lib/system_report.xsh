##! Typed Linux inventory model and pure presentation helpers.

## Describes the outcome for one observed value or field.
export type ObservationState =
    Observed
  | Absent
  | Unsupported
  | PermissionDenied
  | NotRequested
  | Redacted
  | Malformed
  | Disappeared
  | Raced
  | Truncated
  | RangeFailure
  | ReadFailure

## Describes whether a section's requested enumeration completed.
export type SectionState =
    Complete
  | Partial
  | SectionAbsent
  | SectionUnsupported
  | SectionPermissionDenied
  | SectionNotRequested
  | SectionRedacted
  | SectionMalformed
  | SectionDisappeared
  | SectionRaced
  | SectionTruncated

## Failures returned by report parsing, selection, and schema validation.
export error SystemReportError =
    InvalidCpuList(message: Str)
  | InvalidSection(message: Str)
  | InvalidJson(message: Str)
  | UnsupportedSchema(version: Int)
  | InvalidProcStat(message: Str)
  | InvalidExecutionUnits(message: Str)
  | DryRun(message: Str)
  | UnsupportedPlatform(message: Str)

## Identifies whether observations came from a live host or a replay source.
export type SourceMode = LiveLinux | Replay | SyntheticFixture | CapturedReplay | ContainerLive | PhysicalLive

## Stores text with its observation status and optional original bytes.
## Sensitive text uses the same shape and becomes Redacted when omitted.
export type TextObservation = {
  state: ObservationState,
  value: Str?,
  raw_bytes_base64: Str?,
}

## Separates section enumeration outcome from its items, including empty successes.
export type SectionStatus = {
  state: SectionState,
  enumeration_succeeded: Bool,
}

## Associates an observed failure or limitation with its section and field.
export type CollectionIssue = {
  section: Str,
  field: Str,
  state: ObservationState,
  error_kind: Str?,
  errno: Int?,
  detail: TextObservation,
}

## Identifies the program that created the snapshot.
export type Producer = {name: Str, version: Str}

## Identities are meaningful only inside the namespace and observation interval recorded here.
## Records namespaces and source roots that bound the collected view.
export type ObservationScope = {
  platform: Str,
  host_claim: Str,
  source_roots: List[Str],
  mount_namespace: TextObservation,
  network_namespace: TextObservation,
  pid_namespace: TextObservation,
  cgroup_namespace: TextObservation,
  visible_cgroup: TextObservation,
  page_size_bytes: Int?,
  clock_ticks_per_second: Int?,
  ancestors_may_be_hidden: Bool,
}

## Holds selected parsed fields from the operating system release metadata.
export type OsRelease = {
  id: Str?,
  name: Str?,
  pretty_name: Str?,
  version: Str?,
  version_id: Str?,
}

## Holds platform identity from firmware or device-tree sources.
export type FirmwareIdentity = {
  source: Str,
  vendor: Str?,
  product: Str?,
  board_vendor: Str?,
  board_product: Str?,
  bios_vendor: Str?,
  bios_version: Str?,
  serial: TextObservation,
  uuid: TextObservation,
  device_tree_model: TextObservation,
  device_tree_compatible: List[TextObservation],
}

## Groups kernel, operating system, uptime, and firmware identity observations.
export type IdentitySection = {
  status: SectionStatus,
  kernel_release: Str?,
  kernel_build: Str?,
  architecture: Str?,
  os_release: OsRelease?,
  hostname: TextObservation,
  uptime_seconds: Int?,
  boot_id: TextObservation,
  firmware: FirmwareIdentity?,
}

## Describes one logical CPU and its topology relationships.
export type Cpu = {
  id: Int,
  present: Bool?,
  online: Bool?,
  vendor: Str?,
  model: Str?,
  family: Str?,
  model_id: Str?,
  stepping: Str?,
  features: List[Str],
  package_id: Int?,
  die_id: Int?,
  core_id: Int?,
  thread_siblings: List[Int],
  cache_ids: List[Int],
  cache_indices: List[Int],
  numa_node: Int?,
  policy: Str?,
}

## Describes one cache instance and the CPUs that share it.
export type CpuCache = {
  id: Int,
  sysfs_index: Int,
  owner_cpu_id: Int,
  level: Int,
  kind: Str,
  size_bytes: Int?,
  line_size_bytes: Int?,
  sets: Int?,
  shared_cpus: List[Int],
}

## Describes one CPUFreq policy with requested and hardware-observed values separated.
export type CpuFreqPolicy = {
  name: Str,
  related_cpus: List[Int],
  affected_cpus: List[Int],
  driver: Str?,
  governor: Str?,
  available_governors: List[Str],
  hardware_min_khz: Int?,
  hardware_max_khz: Int?,
  scaling_min_khz: Int?,
  scaling_max_khz: Int?,
  hardware_current_khz: Int?,
  requested_current_khz: Int?,
  governor_requested_khz: Int?,
  average_current_khz: Int?,
  bios_limit_khz: Int?,
  transition_latency_ns: Int?,
  available_frequencies_khz: List[Int],
  energy_performance_preference: Str?,
  available_energy_performance_preferences: List[Str],
  boost_supported: Bool?,
  boost_allowed: Bool?,
  boost_active: Bool?,
  boost_scope: Str?,
}

## Describes one kernel-exported CPU idle state and its accounting.
export type CpuIdleState = {
  cpu_id: Int?,
  name: Str,
  description: Str?,
  disable_setting: Int?,
  latency_us: Int?,
  residency_us: Int?,
  usage_count: Int?,
  time_us: Int?,
}

## Stores one kernel-reported CPU vulnerability status.
export type CpuVulnerability = {name: Str, description: TextObservation}

## Collects CPU identity, topology, frequency, idle, and vulnerability data.
export type CpuSection = {
  status: SectionStatus,
  possible: List[Int],
  present: List[Int],
  online: List[Int],
  offline: List[Int],
  affinity: List[Int],
  effective_cpuset: List[Int],
  global_idle_driver: Str?,
  global_idle_governor: Str?,
  cpus: List[Cpu],
  caches: List[CpuCache],
  frequency_policies: List[CpuFreqPolicy],
  idle_states: List[CpuIdleState],
  vulnerabilities: List[CpuVulnerability],
  available_idle_governors: List[Str],
}

## Selects every CPUFreq policy whose related CPU set includes the requested CPU.
export pure frequency_policies_for_cpu(
  policies: List[CpuFreqPolicy],
  cpu_id: Int,
) -> List[CpuFreqPolicy] {
  var selected: List[CpuFreqPolicy] = []

  for policy in policies {
    if policy.related_cpus.contains(cpu_id) {
      selected = selected.push(policy)
    }
  }

  return selected
}

pure cpu_list_error(message: Str) -> SystemReportError {
  return SystemReportError.InvalidCpuList(message: message)
}

let max_cpu_list_identifiers = 65536

pure parse_cpu_list_integer(value: Str, decimal: Regex) -> Result[Int] {
  if ! decimal.matches(value) {
    return Err(cpu_list_error("CPU list contains a non-decimal identifier"))
  }

  match value.parse_int() {
    Ok(identifier) => return Ok(identifier)
    Err(_) => return Err(cpu_list_error("CPU identifier is outside the supported integer range"))
  }
}

## Parses Linux cpulist syntax with sparse IDs and a bounded expanded size.
export pure parse_cpu_list(text: Str) -> Result[List[Int]] {
  if text == "" or text.trim() != text {
    return Err(cpu_list_error("CPU list is empty or contains surrounding whitespace"))
  }

  let decimal = regex.compile("^[0-9]+$")?
  var identifiers: List[Int] = []

  for item in text.split(",") {
    let bounds = item.split("-")
    if bounds.len() == 1 {
      let value = parse_cpu_list_integer(item, decimal)?
      if identifiers.len() >= max_cpu_list_identifiers {
        return Err(cpu_list_error("CPU list exceeds 65536 identifiers"))
      }
      identifiers = identifiers.push(value)
      continue
    }

    if bounds.len() != 2 {
      return Err(cpu_list_error("CPU list contains a malformed range"))
    }

    let start_text = bounds[0]
    let end_text = bounds[1]
    let start = parse_cpu_list_integer(start_text, decimal)?
    let end = parse_cpu_list_integer(end_text, decimal)?
    if end < start {
      return Err(cpu_list_error("CPU list range ends before it starts"))
    }

    let width = end - start
    if width >= max_cpu_list_identifiers {
      return Err(cpu_list_error("CPU list exceeds 65536 identifiers"))
    }
    let added = width + 1
    if identifiers.len() > max_cpu_list_identifiers - added {
      return Err(cpu_list_error("CPU list exceeds 65536 identifiers"))
    }

    var identifier = start
    while identifier < end {
      identifiers = identifiers.push(identifier)
      identifier = identifier + 1
    }
    identifiers = identifiers.push(end)
  }

  if identifiers.len() == 0 {
    return Err(cpu_list_error("CPU list contains no identifiers"))
  }

  var seen = set.empty()
  for identifier in identifiers {
    let key = f"${identifier}"
    if set.has(seen, key) {
      return Err(cpu_list_error("CPU list contains a duplicate identifier"))
    }
    seen = set.add(seen, key)
  }

  return identifiers |> sort-by .
}

## Stores an exact integer counter together with its source unit.
export type MemoryCounter = {name: Str, value: Int, unit: Str}

## Holds memory gauges and counters without converting them to floating point.
export type MemoryStats = {
  total_bytes: Int?,
  free_bytes: Int?,
  available_bytes: Int?,
  buffers_bytes: Int?,
  cached_bytes: Int?,
  active_bytes: Int?,
  inactive_bytes: Int?,
  dirty_bytes: Int?,
  writeback_bytes: Int?,
  swap_total_bytes: Int?,
  swap_free_bytes: Int?,
  counters: List[MemoryCounter],
}

## Describes a swap area visible to the current system view.
export type SwapDevice = {
  name: TextObservation,
  kind: Str,
  size_bytes: Int?,
  used_bytes: Int?,
  priority: Int?,
}

## Describes a huge-page pool with its explicit page size.
export type HugePagePool = {
  node_id: Int?,
  page_size_bytes: Int,
  total: Int,
  free: Int?,
  reserved: Int?,
  surplus: Int?,
}

## Stores one pressure-stall measurement and its kernel-provided averages.
export type PressureLine = {
  resource: Str,
  kind: Str,
  avg10: Str?,
  avg60: Str?,
  avg300: Str?,
  total_us: Int?,
}

## Describes one process-visible cgroup resource limit and current value.
export type CgroupResource = {
  path: TextObservation,
  hierarchy_level: Int,
  controller: Str,
  resource: Str,
  state: ObservationState,
  maximum_value: Int?,
  current_value: Int?,
  unit: Str,
  maximum_unlimited: Bool?,
  quota: Int?,
  period: Int?,
  effective_cpus: List[Int],
  hidden_ancestors_possible: Bool,
}

## Groups host memory, swap, huge-page, NUMA, pressure, and cgroup data.
export type MemorySection = {
  status: SectionStatus,
  host: MemoryStats,
  swaps: List[SwapDevice],
  huge_pages: List[HugePagePool],
  transparent_huge_pages: List[Str],
  numa: List[MemoryCounter],
  pressure: List[PressureLine],
  cgroup: List[CgroupResource],
}

## Describes one PCI function and preserves indexed parent links when location IDs are redacted.
export type PciFunction = {
  address: Str?,
  domain: Int?,
  bus: Int?,
  device: Int?,
  function: Int?,
  vendor_id: Int?,
  device_id: Int?,
  subsystem_vendor_id: Int?,
  subsystem_device_id: Int?,
  class_code: Int?,
  revision: Int?,
  driver: Str?,
  parent_function_index: Int?,
  numa_node: Int?,
  iommu_group: Str?,
  current_link_speed: Str?,
  current_link_width: Int?,
  maximum_link_speed: Str?,
  maximum_link_width: Int?,
}

## Collects PCI functions while preserving enumeration status.
export type PciSection = {status: SectionStatus, functions: List[PciFunction]}

## Resolves a PCI parent by its index in the stable function list.
export pure pci_parent_function(
  functions: List[PciFunction],
  child: PciFunction,
) -> PciFunction? {
  let parent_index = match child.parent_function_index {
    Some(index) => index
    None => return null
  }

  if parent_index < 0 or parent_index >= functions.len() {
    return null
  }

  return functions[parent_index]
}

## Describes one USB endpoint from an available interface descriptor.
export type UsbEndpoint = {
  address: Int,
  direction: Str,
  transfer_type: Str,
  max_packet_size: Int,
  interval: Int,
}

## Describes one alternate setting and its endpoints.
export type UsbAlternateSetting = {
  number: Int,
  class_code: Int,
  subclass: Int,
  protocol: Int,
  endpoints: List[UsbEndpoint],
}

## Describes a USB interface and the kernel's active alternate setting.
export type UsbInterface = {
  number: Int,
  name: Str?,
  driver: Str?,
  active_alternate: Int?,
  alternate_settings: List[UsbAlternateSetting],
}

## Describes a USB device with indexed parent links and optional redacted location identifiers.
export type UsbDevice = {
  sysfs_name: Str?,
  parent_device_index: Int?,
  controller_pci_index: Int?,
  port_path: Str?,
  bus_number: Int?,
  device_number: Int?,
  vendor_id: Int?,
  product_id: Int?,
  device_version: Str?,
  class_code: Int?,
  subclass: Int?,
  protocol: Int?,
  manufacturer: TextObservation,
  product: TextObservation,
  serial: TextObservation,
  speed_mbps: Str?,
  configuration_count: Int?,
  active_configuration: Int?,
  power_control: Str?,
  autosuspend_delay_ms: Int?,
  is_root_hub: Bool,
  interfaces: List[UsbInterface],
}

## Collects USB devices while preserving enumeration status.
export type UsbSection = {status: SectionStatus, devices: List[UsbDevice]}

## Describes a block device with optional redacted names and indexed parent, holder, and slave links.
export type BlockDevice = {
  name: Str?,
  major: Int?,
  minor: Int?,
  kind: Str,
  size_bytes: Int?,
  logical_sector_bytes: Int?,
  physical_sector_bytes: Int?,
  removable: Bool?,
  rotational: Bool?,
  read_only: Bool?,
  model: TextObservation,
  firmware: TextObservation,
  parent_device_index: Int?,
  parent_pci_function_index: Int?,
  holder_indices: List[Int],
  slave_indices: List[Int],
  active_scheduler: Str?,
  available_schedulers: List[Str],
  read_ahead_kb: Int?,
  discard_granularity_bytes: Int?,
  discard_max_bytes: Int?,
  io_counters: List[MemoryCounter],
}

## Describes one mount entry with optional redacted device numbers and a block-device index.
export type Mount = {
  mount_id: Int,
  parent_id: Int,
  major: Int?,
  minor: Int?,
  root: TextObservation,
  target: TextObservation,
  mount_options: List[Str],
  optional_fields: List[Str],
  filesystem: Str,
  source: TextObservation,
  super_options: List[Str],
  block_device_index: Int?,
  usage_state: ObservationState,
  usage_total_bytes: Int?,
  usage_used_bytes: Int?,
  usage_available_bytes: Int?,
}

## Groups block devices and mount entries from the current mount namespace.
export type StorageSection = {
  status: SectionStatus,
  devices: List[BlockDevice],
  mounts: List[Mount],
}

## Preserves one route-netlink attribute as lossless base64 data.
export type NetworkAttribute = {
  kind: Int,
  data: TextObservation,
}

## Describes one assigned network address and its prefix and lifetimes.
export type NetworkAddress = {
  family: Str,
  address: TextObservation,
  prefix_length: Int,
  broadcast: TextObservation,
  scope: Str?,
  flags: Int,
  valid_lifetime_seconds: Int?,
  preferred_lifetime_seconds: Int?,
  attributes: List[NetworkAttribute],
}

## Describes one network link, its counters, addresses, and parent links.
export type NetworkLink = {
  ifindex: Int,
  hardware_type: Int,
  name: TextObservation,
  kind: Str?,
  mtu: Int?,
  admin_up: Bool?,
  operational_state: Str?,
  flags: List[Str],
  mac: TextObservation,
  master_ifindex: Int?,
  lower_ifindex: Int?,
  parent_pci_function_index: Int?,
  parent_usb_device_index: Int?,
  driver: Str?,
  addresses: List[NetworkAddress],
  counters: List[MemoryCounter],
  attributes: List[NetworkAttribute],
}

## Describes one route entry without assuming the main routing table.
export type NetworkNexthop = {
  ifindex: Int,
  flags: Int,
  hops: Int,
  gateway: TextObservation,
}

## Describes one route entry without assuming the main routing table.
export type NetworkRoute = {
  family: Str,
  destination: TextObservation,
  prefix_length: Int,
  source_prefix_length: Int,
  source: TextObservation,
  preferred_source: TextObservation,
  gateway: TextObservation,
  table: Int,
  metric: Int?,
  route_type: Str,
  scope: Str?,
  protocol: Str?,
  output_ifindex: Int?,
  input_ifindex: Int?,
  flags: Int,
  nexthops: List[NetworkNexthop],
  attributes: List[NetworkAttribute],
}

## Describes one policy-routing rule.
export type NetworkRule = {
  family: Str,
  destination_prefix_length: Int,
  source_prefix_length: Int,
  priority: Int?,
  source: TextObservation,
  destination: TextObservation,
  fwmark: Int?,
  fwmask: Int?,
  table: Int?,
  action: Str,
  input_ifindex: Int?,
  output_ifindex: Int?,
  flags: Int,
  attributes: List[NetworkAttribute],
}

## Groups network links, routes, and policy rules.
export type NetworkSection = {
  status: SectionStatus,
  links: List[NetworkLink],
  routes: List[NetworkRoute],
  rules: List[NetworkRule],
}

## Describes one raw kernel sensor channel and its source units.
export type SensorChannel = {
  chip: Str,
  channel: Str,
  label: TextObservation,
  kind: Str,
  value: Int?,
  unit: Str,
  minimum: Int?,
  maximum: Int?,
  critical: Int?,
  alarm: Bool?,
  parent_device_class_index: Int?,
}

## Describes one thermal-zone trip point and its hysteresis.
export type ThermalTrip = {kind: Str, temperature_millidegrees: Int?, hysteresis_millidegrees: Int?}
## Describes one thermal zone and its reported trip points.
export type ThermalZone = {
  id: Int,
  kind: Str?,
  temperature_millidegrees: Int?,
  trips: List[ThermalTrip],
  parent_device_class_index: Int?,
}

## Groups hwmon channels and thermal zones.
export type SensorSection = {
  status: SectionStatus,
  channels: List[SensorChannel],
  thermal_zones: List[ThermalZone],
}

## Describes one power supply or battery with exact source quantities.
export type PowerSupply = {
  name: Str,
  kind: Str?,
  status: Str?,
  health: Str?,
  capacity_percent: Int?,
  energy_now_uwh: Int?,
  energy_full_uwh: Int?,
  charge_now_uah: Int?,
  charge_full_uah: Int?,
  voltage_now_uv: Int?,
  current_now_ua: Int?,
  cycle_count: Int?,
  parent_device_class_index: Int?,
}

## Describes one kernel-exported powercap zone and constraint.
export type PowerCapZone = {
  name: Str,
  parent: Str?,
  energy_uj: Int?,
  maximum_energy_range_uj: Int?,
  constraint_name: Str?,
  power_limit_uw: Int?,
  time_window_us: Int?,
}

## Groups power supplies and powercap observations.
export type PowerSection = {
  status: SectionStatus,
  supplies: List[PowerSupply],
  cap_zones: List[PowerCapZone],
}

## Stores one validated firmware-table record with typed numeric fields.
export type FirmwareRecord = {
  record_type: Int,
  handle: Int,
  formatted_length: Int,
  fields: List[MemoryCounter],
  strings: List[TextObservation],
}

## Groups firmware records and source limitations.
export type FirmwareSection = {
  status: SectionStatus,
  source: Str,
  records: List[FirmwareRecord],
  limitation: TextObservation,
}

## Describes one loaded kernel module.
export type KernelModule = {name: Str, size_bytes: Int, users: Int, state: Str}
## Stores one named kernel parameter observation.
export type KernelParameter = {name: Str, value: TextObservation}
## Groups selected kernel command-line, module, parameter, and sysctl data.
export type KernelSection = {
  status: SectionStatus,
  command_line: TextObservation,
  modules: List[KernelModule],
  parameters: List[KernelParameter],
  sysctls: List[KernelParameter],
}

## Describes one visible process without command-line or environment contents; cgroup indexes resolve into MemorySection.cgroup.
export type ProcessRecord = {
  pid: Int,
  parent_pid: Int,
  uid: Int?,
  command: TextObservation,
  state: Str,
  start_ticks: Int?,
  thread_count: Int?,
  resident_bytes: Int?,
  virtual_bytes: Int?,
  cgroup: TextObservation,
  cgroup_resource_index: Int?,
}

## Collects visible processes while preserving enumeration status.
export type ProcessSection = {status: SectionStatus, processes: List[ProcessRecord]}

## Describes one kernel device-class entry and its available parent relations.
export type DeviceClassRecord = {
  class: Str,
  name: TextObservation,
  parent_device_class_index: Int?,
  parent_pci_function_index: Int?,
  parent_usb_device_index: Int?,
  driver: Str?,
  attributes: List[KernelParameter],
}

## Collects selected kernel device classes while preserving enumeration status.
export type DeviceSection = {status: SectionStatus, devices: List[DeviceClassRecord]}

## Combines typed section observations collected during one bounded read interval.
## The interval is a bounded set of reads, not an atomic kernel snapshot.
export type SystemReport = {
  schema_version: Int,
  producer: Producer,
  source_mode: SourceMode,
  collection_started_unix_ms: Int?,
  collection_ended_unix_ms: Int?,
  elapsed_ms: Int?,
  scope: ObservationScope,
  redacted: Bool,
  identity: IdentitySection,
  cpu: CpuSection,
  memory: MemorySection,
  pci: PciSection,
  usb: UsbSection,
  storage: StorageSection,
  network: NetworkSection,
  sensors: SensorSection,
  power: PowerSection,
  firmware: FirmwareSection,
  kernel: KernelSection,
  processes: ProcessSection,
  devices: DeviceSection,
  issues: List[CollectionIssue],
}

## Names one collection domain for a typed report projection.
export type ReportSection =
    ReportIdentity
  | ReportCpu
  | ReportMemory
  | ReportPci
  | ReportUsb
  | ReportStorage
  | ReportNetwork
  | ReportSensors
  | ReportPower
  | ReportFirmware
  | ReportKernel
  | ReportProcesses
  | ReportDevices

pure report_section_name(section: ReportSection) -> Str {
  match section {
    ReportIdentity => return "identity"
    ReportCpu => return "cpu"
    ReportMemory => return "memory"
    ReportPci => return "pci"
    ReportUsb => return "usb"
    ReportStorage => return "storage"
    ReportNetwork => return "network"
    ReportSensors => return "sensors"
    ReportPower => return "power"
    ReportFirmware => return "firmware"
    ReportKernel => return "kernel"
    ReportProcesses => return "processes"
    ReportDevices => return "devices"
  }
}

## Converts the command's section spelling into its closed report selector.
export pure parse_report_section(value: Str) -> Result[ReportSection] {
  match value {
    "identity" => return Ok(ReportIdentity)
    "cpu" => return Ok(ReportCpu)
    "memory" => return Ok(ReportMemory)
    "pci" => return Ok(ReportPci)
    "usb" => return Ok(ReportUsb)
    "storage" => return Ok(ReportStorage)
    "network" => return Ok(ReportNetwork)
    "sensors" => return Ok(ReportSensors)
    "power" => return Ok(ReportPower)
    "firmware" => return Ok(ReportFirmware)
    "kernel" => return Ok(ReportKernel)
    "processes" => return Ok(ReportProcesses)
    "devices" => return Ok(ReportDevices)
    _ => return Err(SystemReportError.InvalidSection(message: f"unknown report section '${value}'"))
  }
}

pure not_requested_status() -> SectionStatus {
  return {state: SectionNotRequested, enumeration_succeeded: false}
}

pure not_requested_text() -> TextObservation {
  return {state: NotRequested, value: null, raw_bytes_base64: null}
}

pure not_requested_cpu(section: CpuSection) -> CpuSection {
  return {
    ...section,
    status: not_requested_status(),
    possible: [],
    present: [],
    online: [],
    offline: [],
    affinity: [],
    effective_cpuset: [],
    global_idle_driver: null,
    global_idle_governor: null,
    cpus: [],
    caches: [],
    frequency_policies: [],
    idle_states: [],
    vulnerabilities: [],
    available_idle_governors: [],
  }
}

pure not_requested_memory(section: MemorySection) -> MemorySection {
  return {
    ...section,
    status: not_requested_status(),
    host: {
      total_bytes: null,
      free_bytes: null,
      available_bytes: null,
      buffers_bytes: null,
      cached_bytes: null,
      active_bytes: null,
      inactive_bytes: null,
      dirty_bytes: null,
      writeback_bytes: null,
      swap_total_bytes: null,
      swap_free_bytes: null,
      counters: [],
    },
    swaps: [],
    huge_pages: [],
    transparent_huge_pages: [],
    numa: [],
    pressure: [],
    cgroup: [],
  }
}

pure not_requested_pci(section: PciSection) -> PciSection {
  return {...section, status: not_requested_status(), functions: []}
}

pure not_requested_usb(section: UsbSection) -> UsbSection {
  return {...section, status: not_requested_status(), devices: []}
}

pure not_requested_storage(section: StorageSection) -> StorageSection {
  return {...section, status: not_requested_status(), devices: [], mounts: []}
}

pure not_requested_network(section: NetworkSection) -> NetworkSection {
  return {...section, status: not_requested_status(), links: [], routes: [], rules: []}
}

pure not_requested_sensors(section: SensorSection) -> SensorSection {
  return {...section, status: not_requested_status(), channels: [], thermal_zones: []}
}

pure not_requested_power(section: PowerSection) -> PowerSection {
  return {...section, status: not_requested_status(), supplies: [], cap_zones: []}
}

pure not_requested_firmware(section: FirmwareSection) -> FirmwareSection {
  return {
    ...section,
    status: not_requested_status(),
    source: "not-requested",
    records: [],
    limitation: not_requested_text(),
  }
}

pure not_requested_kernel(section: KernelSection) -> KernelSection {
  return {
    ...section,
    status: not_requested_status(),
    command_line: not_requested_text(),
    modules: [],
    parameters: [],
    sysctls: [],
  }
}

pure not_requested_processes(section: ProcessSection) -> ProcessSection {
  return {...section, status: not_requested_status(), processes: []}
}

pure clear_process_cgroup_resource_links(section: ProcessSection) -> ProcessSection {
  var processes: List[ProcessRecord] = []
  for process in section.processes {
    processes = processes.push({...process, cgroup_resource_index: null})
  }
  return {...section, processes: processes}
}

pure not_requested_devices(section: DeviceSection) -> DeviceSection {
  return {...section, status: not_requested_status(), devices: []}
}

pure selection_needs_pci(selected: ReportSection) -> Bool {
  return selected == ReportPci or selected == ReportUsb or selected == ReportNetwork or selected == ReportDevices
}

pure selection_needs_usb(selected: ReportSection) -> Bool {
  return selected == ReportUsb or selected == ReportNetwork or selected == ReportDevices
}

pure select_report_domain(
  report: SystemReport,
  selected: ReportSection,
) -> SystemReport {
  var issues: List[CollectionIssue] = []
  let selected_name = report_section_name(selected)
  let keep_pci = selection_needs_pci(selected)
  let keep_usb = selection_needs_usb(selected)
  for issue in report.issues {
    if issue.section == "identity" or issue.section == selected_name or (keep_pci and issue.section == "pci") or (keep_usb and issue.section == "usb") {
      issues = issues.push(issue)
    }
  }

  return {
    ...report,
    cpu: if selected == ReportCpu { report.cpu } else { not_requested_cpu(report.cpu) },
    memory: if selected == ReportMemory { report.memory } else { not_requested_memory(report.memory) },
    pci: if keep_pci { report.pci } else { not_requested_pci(report.pci) },
    usb: if keep_usb { report.usb } else { not_requested_usb(report.usb) },
    storage: if selected == ReportStorage { report.storage } else { not_requested_storage(report.storage) },
    network: if selected == ReportNetwork { report.network } else { not_requested_network(report.network) },
    sensors: if selected == ReportSensors { report.sensors } else { not_requested_sensors(report.sensors) },
    power: if selected == ReportPower { report.power } else { not_requested_power(report.power) },
    firmware: if selected == ReportFirmware { report.firmware } else { not_requested_firmware(report.firmware) },
    kernel: if selected == ReportKernel { report.kernel } else { not_requested_kernel(report.kernel) },
    processes: if selected == ReportProcesses { clear_process_cgroup_resource_links(report.processes) } else { not_requested_processes(report.processes) },
    devices: if selected == ReportDevices { report.devices } else { not_requested_devices(report.devices) },
    issues: issues,
  }
}

## Keeps identity and one named domain, marking every excluded domain.
export pure select_report_section(
  report: Record,
  selected: Str,
) -> Result[Record] {
  let typed = report.require(SystemReport)?
  return select_report_domain(typed, parse_report_section(selected)?)
}

type JsonTextObservation = {
  state: Str,
  value: Str?,
  raw_bytes_base64: Str?,
}

type JsonSectionStatus = {
  state: Str,
  enumeration_succeeded: Bool,
}

type JsonCollectionIssue = {
  section: Str,
  field: Str,
  state: Str,
  error_kind: Str?,
  errno: Int?,
  detail: JsonTextObservation,
}

type JsonObservationScope = {
  platform: Str,
  host_claim: Str,
  source_roots: List[Str],
  mount_namespace: JsonTextObservation,
  network_namespace: JsonTextObservation,
  pid_namespace: JsonTextObservation,
  cgroup_namespace: JsonTextObservation,
  visible_cgroup: JsonTextObservation,
  page_size_bytes: Int?,
  clock_ticks_per_second: Int?,
  ancestors_may_be_hidden: Bool,
}

type JsonFirmwareIdentity = {
  source: Str,
  vendor: Str?,
  product: Str?,
  board_vendor: Str?,
  board_product: Str?,
  bios_vendor: Str?,
  bios_version: Str?,
  serial: JsonTextObservation,
  uuid: JsonTextObservation,
  device_tree_model: JsonTextObservation,
  device_tree_compatible: List[JsonTextObservation],
}

type JsonIdentitySection = {
  status: JsonSectionStatus,
  kernel_release: Str?,
  kernel_build: Str?,
  architecture: Str?,
  os_release: OsRelease?,
  hostname: JsonTextObservation,
  uptime_seconds: Int?,
  boot_id: JsonTextObservation,
  firmware: JsonFirmwareIdentity?,
}

type JsonCpuVulnerability = {name: Str, description: JsonTextObservation}

type JsonCpuSection = {
  status: JsonSectionStatus,
  possible: List[Int],
  present: List[Int],
  online: List[Int],
  offline: List[Int],
  affinity: List[Int],
  effective_cpuset: List[Int],
  global_idle_driver: Str?,
  global_idle_governor: Str?,
  cpus: List[Cpu],
  caches: List[CpuCache],
  frequency_policies: List[CpuFreqPolicy],
  idle_states: List[CpuIdleState],
  vulnerabilities: List[JsonCpuVulnerability],
  available_idle_governors: List[Str],
}

type JsonCgroupResource = {
  path: JsonTextObservation,
  hierarchy_level: Int,
  controller: Str,
  resource: Str,
  state: Str,
  maximum_value: Int?,
  current_value: Int?,
  unit: Str,
  maximum_unlimited: Bool?,
  quota: Int?,
  period: Int?,
  effective_cpus: List[Int],
  hidden_ancestors_possible: Bool,
}

type JsonSwapDevice = {
  name: JsonTextObservation,
  kind: Str,
  size_bytes: Int?,
  used_bytes: Int?,
  priority: Int?,
}

type JsonMemorySection = {
  status: JsonSectionStatus,
  host: MemoryStats,
  swaps: List[JsonSwapDevice],
  huge_pages: List[HugePagePool],
  transparent_huge_pages: List[Str],
  numa: List[MemoryCounter],
  pressure: List[PressureLine],
  cgroup: List[JsonCgroupResource],
}

type JsonPciSection = {status: JsonSectionStatus, functions: List[PciFunction]}

type JsonUsbDevice = {
  sysfs_name: Str?,
  parent_device_index: Int?,
  controller_pci_index: Int?,
  port_path: Str?,
  bus_number: Int?,
  device_number: Int?,
  vendor_id: Int?,
  product_id: Int?,
  device_version: Str?,
  class_code: Int?,
  subclass: Int?,
  protocol: Int?,
  manufacturer: JsonTextObservation,
  product: JsonTextObservation,
  serial: JsonTextObservation,
  speed_mbps: Str?,
  configuration_count: Int?,
  active_configuration: Int?,
  power_control: Str?,
  autosuspend_delay_ms: Int?,
  is_root_hub: Bool,
  interfaces: List[UsbInterface],
}

type JsonUsbSection = {status: JsonSectionStatus, devices: List[JsonUsbDevice]}

type JsonMount = {
  mount_id: Int,
  parent_id: Int,
  major: Int?,
  minor: Int?,
  root: JsonTextObservation,
  target: JsonTextObservation,
  mount_options: List[Str],
  optional_fields: List[Str],
  filesystem: Str,
  source: JsonTextObservation,
  super_options: List[Str],
  block_device_index: Int?,
  usage_state: Str,
  usage_total_bytes: Int?,
  usage_used_bytes: Int?,
  usage_available_bytes: Int?,
}

type JsonBlockDevice = {
  name: Str?,
  major: Int?,
  minor: Int?,
  kind: Str,
  size_bytes: Int?,
  logical_sector_bytes: Int?,
  physical_sector_bytes: Int?,
  removable: Bool?,
  rotational: Bool?,
  read_only: Bool?,
  model: JsonTextObservation,
  firmware: JsonTextObservation,
  parent_device_index: Int?,
  parent_pci_function_index: Int?,
  holder_indices: List[Int],
  slave_indices: List[Int],
  active_scheduler: Str?,
  available_schedulers: List[Str],
  read_ahead_kb: Int?,
  discard_granularity_bytes: Int?,
  discard_max_bytes: Int?,
  io_counters: List[MemoryCounter],
}

type JsonStorageSection = {
  status: JsonSectionStatus,
  devices: List[JsonBlockDevice],
  mounts: List[JsonMount],
}

type JsonNetworkAttribute = {
  kind: Int,
  data: JsonTextObservation,
}

type JsonNetworkAddress = {
  family: Str,
  address: JsonTextObservation,
  prefix_length: Int,
  broadcast: JsonTextObservation,
  scope: Str?,
  flags: Int,
  valid_lifetime_seconds: Int?,
  preferred_lifetime_seconds: Int?,
  attributes: List[JsonNetworkAttribute],
}

type JsonNetworkLink = {
  ifindex: Int,
  hardware_type: Int,
  name: JsonTextObservation,
  kind: Str?,
  mtu: Int?,
  admin_up: Bool?,
  operational_state: Str?,
  flags: List[Str],
  mac: JsonTextObservation,
  master_ifindex: Int?,
  lower_ifindex: Int?,
  parent_pci_function_index: Int?,
  parent_usb_device_index: Int?,
  driver: Str?,
  addresses: List[JsonNetworkAddress],
  counters: List[MemoryCounter],
  attributes: List[JsonNetworkAttribute],
}

type JsonNetworkRoute = {
  family: Str,
  destination: JsonTextObservation,
  prefix_length: Int,
  source_prefix_length: Int,
  source: JsonTextObservation,
  preferred_source: JsonTextObservation,
  gateway: JsonTextObservation,
  table: Int,
  metric: Int?,
  route_type: Str,
  scope: Str?,
  protocol: Str?,
  output_ifindex: Int?,
  input_ifindex: Int?,
  flags: Int,
  nexthops: List[JsonNetworkNexthop],
  attributes: List[JsonNetworkAttribute],
}

type JsonNetworkNexthop = {
  ifindex: Int,
  flags: Int,
  hops: Int,
  gateway: JsonTextObservation,
}

type JsonNetworkRule = {
  family: Str,
  destination_prefix_length: Int,
  source_prefix_length: Int,
  priority: Int?,
  source: JsonTextObservation,
  destination: JsonTextObservation,
  fwmark: Int?,
  fwmask: Int?,
  table: Int?,
  action: Str,
  input_ifindex: Int?,
  output_ifindex: Int?,
  flags: Int,
  attributes: List[JsonNetworkAttribute],
}

type JsonNetworkSection = {
  status: JsonSectionStatus,
  links: List[JsonNetworkLink],
  routes: List[JsonNetworkRoute],
  rules: List[JsonNetworkRule],
}

type JsonSensorChannel = {
  chip: Str,
  channel: Str,
  label: JsonTextObservation,
  kind: Str,
  value: Int?,
  unit: Str,
  minimum: Int?,
  maximum: Int?,
  critical: Int?,
  alarm: Bool?,
  parent_device_class_index: Int?,
}

type JsonSensorSection = {
  status: JsonSectionStatus,
  channels: List[JsonSensorChannel],
  thermal_zones: List[ThermalZone],
}

type JsonPowerSection = {
  status: JsonSectionStatus,
  supplies: List[PowerSupply],
  cap_zones: List[PowerCapZone],
}

type JsonFirmwareRecord = {
  record_type: Int,
  handle: Int,
  formatted_length: Int,
  fields: List[MemoryCounter],
  strings: List[JsonTextObservation],
}

type JsonFirmwareSection = {
  status: JsonSectionStatus,
  source: Str,
  records: List[JsonFirmwareRecord],
  limitation: JsonTextObservation,
}

type JsonKernelParameter = {name: Str, value: JsonTextObservation}

type JsonKernelSection = {
  status: JsonSectionStatus,
  command_line: JsonTextObservation,
  modules: List[KernelModule],
  parameters: List[JsonKernelParameter],
  sysctls: List[JsonKernelParameter],
}

type JsonProcessRecord = {
  pid: Int,
  parent_pid: Int,
  uid: Int?,
  command: JsonTextObservation,
  state: Str,
  start_ticks: Int?,
  thread_count: Int?,
  resident_bytes: Int?,
  virtual_bytes: Int?,
  cgroup: JsonTextObservation,
  cgroup_resource_index: Int?,
}

type JsonProcessSection = {status: JsonSectionStatus, processes: List[JsonProcessRecord]}

type JsonDeviceClassRecord = {
  class: Str,
  name: JsonTextObservation,
  parent_device_class_index: Int?,
  parent_pci_function_index: Int?,
  parent_usb_device_index: Int?,
  driver: Str?,
  attributes: List[JsonKernelParameter],
}

type JsonDeviceSection = {status: JsonSectionStatus, devices: List[JsonDeviceClassRecord]}

## JSON v1 uses stable string spellings for the report's closed tag unions.
export type SystemReportJson = {
  schema_version: Int,
  producer: Producer,
  source_mode: Str,
  collection_started_unix_ms: Int?,
  collection_ended_unix_ms: Int?,
  elapsed_ms: Int?,
  scope: JsonObservationScope,
  redacted: Bool,
  identity: JsonIdentitySection,
  cpu: JsonCpuSection,
  memory: JsonMemorySection,
  pci: JsonPciSection,
  usb: JsonUsbSection,
  storage: JsonStorageSection,
  network: JsonNetworkSection,
  sensors: JsonSensorSection,
  power: JsonPowerSection,
  firmware: JsonFirmwareSection,
  kernel: JsonKernelSection,
  processes: JsonProcessSection,
  devices: JsonDeviceSection,
  issues: List[JsonCollectionIssue],
}

pure observation_state_json(state: ObservationState) -> Str {
  match state {
    Observed => return "observed"
    Absent => return "absent"
    Unsupported => return "unsupported"
    PermissionDenied => return "permission_denied"
    NotRequested => return "not_requested"
    Redacted => return "redacted"
    Malformed => return "malformed"
    Disappeared => return "disappeared"
    Raced => return "raced"
    Truncated => return "truncated"
    RangeFailure => return "range_failure"
    ReadFailure => return "read_failure"
  }
}

pure observation_state_xsh(value: Str) -> Result[ObservationState] {
  match value {
    "observed" => return Ok(Observed)
    "absent" => return Ok(Absent)
    "unsupported" => return Ok(Unsupported)
    "permission_denied" => return Ok(PermissionDenied)
    "not_requested" => return Ok(NotRequested)
    "redacted" => return Ok(Redacted)
    "malformed" => return Ok(Malformed)
    "disappeared" => return Ok(Disappeared)
    "raced" => return Ok(Raced)
    "truncated" => return Ok(Truncated)
    "range_failure" => return Ok(RangeFailure)
    "read_failure" => return Ok(ReadFailure)
    _ => return Err(SystemReportError.InvalidJson(message: f"unknown observation state '${value}'"))
  }
}

pure section_state_json(state: SectionState) -> Str {
  match state {
    Complete => return "complete"
    Partial => return "partial"
    SectionAbsent => return "absent"
    SectionUnsupported => return "unsupported"
    SectionPermissionDenied => return "permission_denied"
    SectionNotRequested => return "not_requested"
    SectionRedacted => return "redacted"
    SectionMalformed => return "malformed"
    SectionDisappeared => return "disappeared"
    SectionRaced => return "raced"
    SectionTruncated => return "truncated"
  }
}

pure section_state_xsh(value: Str) -> Result[SectionState] {
  match value {
    "complete" => return Ok(Complete)
    "partial" => return Ok(Partial)
    "absent" => return Ok(Absent)
    "unsupported" => return Ok(Unsupported)
    "permission_denied" => return Ok(PermissionDenied)
    "not_requested" => return Ok(NotRequested)
    "redacted" => return Ok(Redacted)
    "malformed" => return Ok(Malformed)
    "disappeared" => return Ok(Disappeared)
    "raced" => return Ok(Raced)
    "truncated" => return Ok(Truncated)
    _ => return Err(SystemReportError.InvalidJson(message: f"unknown section state '${value}'"))
  }
}

pure source_mode_json(mode: SourceMode) -> Str {
  match mode {
    LiveLinux => return "live_linux"
    Replay => return "replay"
    SyntheticFixture => return "synthetic_fixture"
    CapturedReplay => return "captured_replay"
    ContainerLive => return "container_live"
    PhysicalLive => return "physical_live"
  }
}

pure source_mode_xsh(value: Str) -> Result[SourceMode] {
  match value {
    "live_linux" => return Ok(LiveLinux)
    "replay" => return Ok(Replay)
    "synthetic_fixture" => return Ok(SyntheticFixture)
    "captured_replay" => return Ok(CapturedReplay)
    "container_live" => return Ok(ContainerLive)
    "physical_live" => return Ok(PhysicalLive)
    _ => return Err(SystemReportError.InvalidJson(message: f"unknown source mode '${value}'"))
  }
}

pure text_observation_json(value: TextObservation) -> JsonTextObservation {
  return {...value, state: observation_state_json(value.state)}
}

pure text_observation_xsh(value: JsonTextObservation) -> Result[TextObservation] {
  return {
    state: observation_state_xsh(value.state)?,
    value: value.value,
    raw_bytes_base64: value.raw_bytes_base64,
  }
}

pure section_status_json(value: SectionStatus) -> JsonSectionStatus {
  return {...value, state: section_state_json(value.state)}
}

pure section_status_xsh(value: JsonSectionStatus) -> Result[SectionStatus] {
  let state = section_state_xsh(value.state)?
  if state == Complete and ! value.enumeration_succeeded {
    return Err(Error(kind: "system-report-json", message: "complete section has no successful enumeration"))
  }
  if (state == NotRequested or state == Unsupported) and value.enumeration_succeeded {
    return Err(Error(kind: "system-report-json", message: "unavailable section claims a successful enumeration"))
  }

  return {
    state: state,
    enumeration_succeeded: value.enumeration_succeeded,
  }
}

pure collection_issue_json(value: CollectionIssue) -> JsonCollectionIssue {
  return {
    ...value,
    state: observation_state_json(value.state),
    detail: text_observation_json(value.detail),
  }
}

pure collection_issue_xsh(value: JsonCollectionIssue) -> Result[CollectionIssue] {
  return {
    section: value.section,
    field: value.field,
    state: observation_state_xsh(value.state)?,
    error_kind: value.error_kind,
    errno: value.errno,
    detail: text_observation_xsh(value.detail)?,
  }
}

pure observation_scope_json(value: ObservationScope) -> JsonObservationScope {
  return {
    ...value,
    mount_namespace: text_observation_json(value.mount_namespace),
    network_namespace: text_observation_json(value.network_namespace),
    pid_namespace: text_observation_json(value.pid_namespace),
    cgroup_namespace: text_observation_json(value.cgroup_namespace),
    visible_cgroup: text_observation_json(value.visible_cgroup),
  }
}

pure observation_scope_xsh(value: JsonObservationScope) -> Result[ObservationScope] {
  return {
    ...value,
    mount_namespace: text_observation_xsh(value.mount_namespace)?,
    network_namespace: text_observation_xsh(value.network_namespace)?,
    pid_namespace: text_observation_xsh(value.pid_namespace)?,
    cgroup_namespace: text_observation_xsh(value.cgroup_namespace)?,
    visible_cgroup: text_observation_xsh(value.visible_cgroup)?,
  }
}

pure firmware_identity_json(value: FirmwareIdentity) -> JsonFirmwareIdentity {
  var compatible: List[JsonTextObservation] = []
  for item in value.device_tree_compatible {
    compatible = compatible.push(text_observation_json(item))
  }

  return {
    ...value,
    serial: text_observation_json(value.serial),
    uuid: text_observation_json(value.uuid),
    device_tree_model: text_observation_json(value.device_tree_model),
    device_tree_compatible: compatible,
  }
}

pure firmware_identity_xsh(value: JsonFirmwareIdentity) -> Result[FirmwareIdentity] {
  var compatible: List[TextObservation] = []
  for item in value.device_tree_compatible {
    compatible = compatible.push(text_observation_xsh(item)?)
  }

  return {
    ...value,
    serial: text_observation_xsh(value.serial)?,
    uuid: text_observation_xsh(value.uuid)?,
    device_tree_model: text_observation_xsh(value.device_tree_model)?,
    device_tree_compatible: compatible,
  }
}

pure identity_section_json(value: IdentitySection) -> JsonIdentitySection {
  var firmware: JsonFirmwareIdentity? = null
  if value.firmware != null {
    firmware = firmware_identity_json(value.firmware)
  }

  return {
    ...value,
    status: section_status_json(value.status),
    hostname: text_observation_json(value.hostname),
    boot_id: text_observation_json(value.boot_id),
    firmware: firmware,
  }
}

pure identity_section_xsh(value: JsonIdentitySection) -> Result[IdentitySection] {
  var firmware: FirmwareIdentity? = null
  if value.firmware != null {
    firmware = firmware_identity_xsh(value.firmware)?
  }

  return {
    ...value,
    status: section_status_xsh(value.status)?,
    hostname: text_observation_xsh(value.hostname)?,
    boot_id: text_observation_xsh(value.boot_id)?,
    firmware: firmware,
  }
}

pure cpu_section_json(value: CpuSection) -> JsonCpuSection {
  var vulnerabilities: List[JsonCpuVulnerability] = []
  for item in value.vulnerabilities {
    vulnerabilities = vulnerabilities.push({
      name: item.name,
      description: text_observation_json(item.description),
    })
  }

  return {
    ...value,
    status: section_status_json(value.status),
    vulnerabilities: vulnerabilities,
  }
}

pure cpu_section_xsh(value: JsonCpuSection) -> Result[CpuSection] {
  var vulnerabilities: List[CpuVulnerability] = []
  for item in value.vulnerabilities {
    vulnerabilities = vulnerabilities.push({
      name: item.name,
      description: text_observation_xsh(item.description)?,
    })
  }

  return {
    ...value,
    status: section_status_xsh(value.status)?,
    vulnerabilities: vulnerabilities,
  }
}

pure cgroup_resource_json(value: CgroupResource) -> JsonCgroupResource {
  return {
    ...value,
    path: text_observation_json(value.path),
    state: observation_state_json(value.state),
  }
}

pure cgroup_resource_xsh(value: JsonCgroupResource) -> Result[CgroupResource] {
  return {
    ...value,
    path: text_observation_xsh(value.path)?,
    state: observation_state_xsh(value.state)?,
  }
}

pure swap_device_json(value: SwapDevice) -> JsonSwapDevice {
  return {...value, name: text_observation_json(value.name)}
}

pure swap_device_xsh(value: JsonSwapDevice) -> Result[SwapDevice] {
  return {...value, name: text_observation_xsh(value.name)?}
}

pure memory_section_json(value: MemorySection) -> JsonMemorySection {
  var swaps: List[JsonSwapDevice] = []
  for swap in value.swaps {
    swaps = swaps.push(swap_device_json(swap))
  }

  var cgroup: List[JsonCgroupResource] = []
  for resource in value.cgroup {
    cgroup = cgroup.push(cgroup_resource_json(resource))
  }

  return {
    ...value,
    status: section_status_json(value.status),
    swaps: swaps,
    cgroup: cgroup,
  }
}

pure memory_section_xsh(value: JsonMemorySection) -> Result[MemorySection] {
  var swaps: List[SwapDevice] = []
  for swap in value.swaps {
    swaps = swaps.push(swap_device_xsh(swap)?)
  }

  var cgroup: List[CgroupResource] = []
  for resource in value.cgroup {
    cgroup = cgroup.push(cgroup_resource_xsh(resource)?)
  }

  return {
    ...value,
    status: section_status_xsh(value.status)?,
    swaps: swaps,
    cgroup: cgroup,
  }
}

pure pci_section_json(value: PciSection) -> JsonPciSection {
  return {...value, status: section_status_json(value.status)}
}

pure pci_section_xsh(value: JsonPciSection) -> Result[PciSection] {
  return {...value, status: section_status_xsh(value.status)?}
}

pure usb_device_json(value: UsbDevice) -> JsonUsbDevice {
  return {
    ...value,
    manufacturer: text_observation_json(value.manufacturer),
    product: text_observation_json(value.product),
    serial: text_observation_json(value.serial),
  }
}

pure usb_device_xsh(value: JsonUsbDevice) -> Result[UsbDevice] {
  return {
    ...value,
    manufacturer: text_observation_xsh(value.manufacturer)?,
    product: text_observation_xsh(value.product)?,
    serial: text_observation_xsh(value.serial)?,
  }
}

pure usb_section_json(value: UsbSection) -> JsonUsbSection {
  var devices: List[JsonUsbDevice] = []
  for device in value.devices {
    devices = devices.push(usb_device_json(device))
  }

  return {
    status: section_status_json(value.status),
    devices: devices,
  }
}

pure usb_section_xsh(value: JsonUsbSection) -> Result[UsbSection] {
  var devices: List[UsbDevice] = []
  for device in value.devices {
    devices = devices.push(usb_device_xsh(device)?)
  }

  return {
    status: section_status_xsh(value.status)?,
    devices: devices,
  }
}

pure mount_json(value: Mount) -> JsonMount {
  return {
    ...value,
    root: text_observation_json(value.root),
    target: text_observation_json(value.target),
    source: text_observation_json(value.source),
    usage_state: observation_state_json(value.usage_state),
  }
}

pure mount_xsh(value: JsonMount) -> Result[Mount] {
  return {
    ...value,
    root: text_observation_xsh(value.root)?,
    target: text_observation_xsh(value.target)?,
    source: text_observation_xsh(value.source)?,
    usage_state: observation_state_xsh(value.usage_state)?,
  }
}

pure block_device_json(value: BlockDevice) -> JsonBlockDevice {
  return {
    ...value,
    model: text_observation_json(value.model),
    firmware: text_observation_json(value.firmware),
  }
}

pure block_device_xsh(value: JsonBlockDevice) -> Result[BlockDevice] {
  return {
    ...value,
    model: text_observation_xsh(value.model)?,
    firmware: text_observation_xsh(value.firmware)?,
  }
}

pure storage_section_json(value: StorageSection) -> JsonStorageSection {
  var devices: List[JsonBlockDevice] = []
  for device in value.devices {
    devices = devices.push(block_device_json(device))
  }

  var mounts: List[JsonMount] = []
  for mount in value.mounts {
    mounts = mounts.push(mount_json(mount))
  }

  return {
    status: section_status_json(value.status),
    devices: devices,
    mounts: mounts,
  }
}

pure storage_section_xsh(value: JsonStorageSection) -> Result[StorageSection] {
  var devices: List[BlockDevice] = []
  for device in value.devices {
    devices = devices.push(block_device_xsh(device)?)
  }

  var mounts: List[Mount] = []
  for mount in value.mounts {
    mounts = mounts.push(mount_xsh(mount)?)
  }

  return {
    status: section_status_xsh(value.status)?,
    devices: devices,
    mounts: mounts,
  }
}

pure network_attributes_json(values: List[NetworkAttribute]) -> List[JsonNetworkAttribute] {
  var output: List[JsonNetworkAttribute] = []
  for attribute in values {
    output = output.push({
      kind: attribute.kind,
      data: text_observation_json(attribute.data),
    })
  }
  return output
}

pure network_attributes_xsh(values: List[JsonNetworkAttribute]) -> Result[List[NetworkAttribute]] {
  var output: List[NetworkAttribute] = []
  for attribute in values {
    output = output.push({
      kind: attribute.kind,
      data: text_observation_xsh(attribute.data)?,
    })
  }
  return Ok(output)
}

pure network_address_json(value: NetworkAddress) -> JsonNetworkAddress {
  return {
    ...value,
    address: text_observation_json(value.address),
    broadcast: text_observation_json(value.broadcast),
    attributes: network_attributes_json(value.attributes),
  }
}

pure network_address_xsh(value: JsonNetworkAddress) -> Result[NetworkAddress] {
  return {
    ...value,
    address: text_observation_xsh(value.address)?,
    broadcast: text_observation_xsh(value.broadcast)?,
    attributes: network_attributes_xsh(value.attributes)?,
  }
}

pure network_link_json(value: NetworkLink) -> JsonNetworkLink {
  var addresses: List[JsonNetworkAddress] = []
  for address in value.addresses {
    addresses = addresses.push(network_address_json(address))
  }

  return {
    ...value,
    name: text_observation_json(value.name),
    mac: text_observation_json(value.mac),
    addresses: addresses,
    attributes: network_attributes_json(value.attributes),
  }
}

pure network_link_xsh(value: JsonNetworkLink) -> Result[NetworkLink] {
  var addresses: List[NetworkAddress] = []
  for address in value.addresses {
    addresses = addresses.push(network_address_xsh(address)?)
  }

  return {
    ...value,
    name: text_observation_xsh(value.name)?,
    mac: text_observation_xsh(value.mac)?,
    addresses: addresses,
    attributes: network_attributes_xsh(value.attributes)?,
  }
}

pure network_nexthops_json(values: List[NetworkNexthop]) -> List[JsonNetworkNexthop] {
  var output: List[JsonNetworkNexthop] = []
  for nexthop in values {
    output = output.push({
      ...nexthop,
      gateway: text_observation_json(nexthop.gateway),
    })
  }
  return output
}

pure network_nexthops_xsh(values: List[JsonNetworkNexthop]) -> Result[List[NetworkNexthop]] {
  var output: List[NetworkNexthop] = []
  for nexthop in values {
    output = output.push({
      ...nexthop,
      gateway: text_observation_xsh(nexthop.gateway)?,
    })
  }
  return Ok(output)
}

pure network_route_json(value: NetworkRoute) -> JsonNetworkRoute {
  return {
    ...value,
    destination: text_observation_json(value.destination),
    source: text_observation_json(value.source),
    preferred_source: text_observation_json(value.preferred_source),
    gateway: text_observation_json(value.gateway),
    nexthops: network_nexthops_json(value.nexthops),
    attributes: network_attributes_json(value.attributes),
  }
}

pure network_route_xsh(value: JsonNetworkRoute) -> Result[NetworkRoute] {
  return {
    ...value,
    destination: text_observation_xsh(value.destination)?,
    source: text_observation_xsh(value.source)?,
    preferred_source: text_observation_xsh(value.preferred_source)?,
    gateway: text_observation_xsh(value.gateway)?,
    nexthops: network_nexthops_xsh(value.nexthops)?,
    attributes: network_attributes_xsh(value.attributes)?,
  }
}

pure network_rule_json(value: NetworkRule) -> JsonNetworkRule {
  return {
    ...value,
    source: text_observation_json(value.source),
    destination: text_observation_json(value.destination),
    attributes: network_attributes_json(value.attributes),
  }
}

pure network_rule_xsh(value: JsonNetworkRule) -> Result[NetworkRule] {
  return {
    ...value,
    source: text_observation_xsh(value.source)?,
    destination: text_observation_xsh(value.destination)?,
    attributes: network_attributes_xsh(value.attributes)?,
  }
}

pure network_section_json(value: NetworkSection) -> JsonNetworkSection {
  var links: List[JsonNetworkLink] = []
  for link in value.links {
    links = links.push(network_link_json(link))
  }

  var routes: List[JsonNetworkRoute] = []
  for route in value.routes {
    routes = routes.push(network_route_json(route))
  }

  var rules: List[JsonNetworkRule] = []
  for rule in value.rules {
    rules = rules.push(network_rule_json(rule))
  }

  return {
    status: section_status_json(value.status),
    links: links,
    routes: routes,
    rules: rules,
  }
}

pure network_section_xsh(value: JsonNetworkSection) -> Result[NetworkSection] {
  var links: List[NetworkLink] = []
  for link in value.links {
    links = links.push(network_link_xsh(link)?)
  }

  var routes: List[NetworkRoute] = []
  for route in value.routes {
    routes = routes.push(network_route_xsh(route)?)
  }

  var rules: List[NetworkRule] = []
  for rule in value.rules {
    rules = rules.push(network_rule_xsh(rule)?)
  }

  return {
    status: section_status_xsh(value.status)?,
    links: links,
    routes: routes,
    rules: rules,
  }
}

pure sensor_channel_json(value: SensorChannel) -> JsonSensorChannel {
  return {...value, label: text_observation_json(value.label)}
}

pure sensor_channel_xsh(value: JsonSensorChannel) -> Result[SensorChannel] {
  return {...value, label: text_observation_xsh(value.label)?}
}

pure sensor_section_json(value: SensorSection) -> JsonSensorSection {
  var channels: List[JsonSensorChannel] = []
  for channel in value.channels {
    channels = channels.push(sensor_channel_json(channel))
  }

  return {
    status: section_status_json(value.status),
    channels: channels,
    thermal_zones: value.thermal_zones,
  }
}

pure sensor_section_xsh(value: JsonSensorSection) -> Result[SensorSection] {
  var channels: List[SensorChannel] = []
  for channel in value.channels {
    channels = channels.push(sensor_channel_xsh(channel)?)
  }

  return {
    status: section_status_xsh(value.status)?,
    channels: channels,
    thermal_zones: value.thermal_zones,
  }
}

pure power_section_json(value: PowerSection) -> JsonPowerSection {
  return {
    status: section_status_json(value.status),
    supplies: value.supplies,
    cap_zones: value.cap_zones,
  }
}

pure power_section_xsh(value: JsonPowerSection) -> Result[PowerSection] {
  return {
    status: section_status_xsh(value.status)?,
    supplies: value.supplies,
    cap_zones: value.cap_zones,
  }
}

pure firmware_record_json(value: FirmwareRecord) -> JsonFirmwareRecord {
  var strings: List[JsonTextObservation] = []
  for item in value.strings {
    strings = strings.push(text_observation_json(item))
  }

  return {...value, strings: strings}
}

pure firmware_record_xsh(value: JsonFirmwareRecord) -> Result[FirmwareRecord] {
  var strings: List[TextObservation] = []
  for item in value.strings {
    strings = strings.push(text_observation_xsh(item)?)
  }

  return {...value, strings: strings}
}

pure firmware_section_json(value: FirmwareSection) -> JsonFirmwareSection {
  var records: List[JsonFirmwareRecord] = []
  for record in value.records {
    records = records.push(firmware_record_json(record))
  }

  return {
    status: section_status_json(value.status),
    source: value.source,
    records: records,
    limitation: text_observation_json(value.limitation),
  }
}

pure firmware_section_xsh(value: JsonFirmwareSection) -> Result[FirmwareSection] {
  var records: List[FirmwareRecord] = []
  for record in value.records {
    records = records.push(firmware_record_xsh(record)?)
  }

  return {
    status: section_status_xsh(value.status)?,
    source: value.source,
    records: records,
    limitation: text_observation_xsh(value.limitation)?,
  }
}

pure kernel_parameter_json(value: KernelParameter) -> JsonKernelParameter {
  return {...value, value: text_observation_json(value.value)}
}

pure kernel_parameter_xsh(value: JsonKernelParameter) -> Result[KernelParameter] {
  return {...value, value: text_observation_xsh(value.value)?}
}

pure kernel_section_json(value: KernelSection) -> JsonKernelSection {
  var parameters: List[JsonKernelParameter] = []
  for parameter in value.parameters {
    parameters = parameters.push(kernel_parameter_json(parameter))
  }

  var sysctls: List[JsonKernelParameter] = []
  for parameter in value.sysctls {
    sysctls = sysctls.push(kernel_parameter_json(parameter))
  }

  return {
    status: section_status_json(value.status),
    command_line: text_observation_json(value.command_line),
    modules: value.modules,
    parameters: parameters,
    sysctls: sysctls,
  }
}

pure kernel_section_xsh(value: JsonKernelSection) -> Result[KernelSection] {
  var parameters: List[KernelParameter] = []
  for parameter in value.parameters {
    parameters = parameters.push(kernel_parameter_xsh(parameter)?)
  }

  var sysctls: List[KernelParameter] = []
  for parameter in value.sysctls {
    sysctls = sysctls.push(kernel_parameter_xsh(parameter)?)
  }

  return {
    status: section_status_xsh(value.status)?,
    command_line: text_observation_xsh(value.command_line)?,
    modules: value.modules,
    parameters: parameters,
    sysctls: sysctls,
  }
}

pure process_record_json(value: ProcessRecord) -> JsonProcessRecord {
  return {
    ...value,
    command: text_observation_json(value.command),
    cgroup: text_observation_json(value.cgroup),
  }
}

pure process_record_xsh(value: JsonProcessRecord) -> Result[ProcessRecord] {
  return {
    ...value,
    command: text_observation_xsh(value.command)?,
    cgroup: text_observation_xsh(value.cgroup)?,
  }
}

pure process_section_json(value: ProcessSection) -> JsonProcessSection {
  var processes: List[JsonProcessRecord] = []
  for process in value.processes {
    processes = processes.push(process_record_json(process))
  }

  return {
    status: section_status_json(value.status),
    processes: processes,
  }
}

pure process_section_xsh(value: JsonProcessSection) -> Result[ProcessSection] {
  var processes: List[ProcessRecord] = []
  for process in value.processes {
    processes = processes.push(process_record_xsh(process)?)
  }

  return {
    status: section_status_xsh(value.status)?,
    processes: processes,
  }
}

pure device_record_json(value: DeviceClassRecord) -> JsonDeviceClassRecord {
  var attributes: List[JsonKernelParameter] = []
  for attribute in value.attributes {
    attributes = attributes.push(kernel_parameter_json(attribute))
  }

  return {
    ...value,
    name: text_observation_json(value.name),
    attributes: attributes,
  }
}

pure device_record_xsh(value: JsonDeviceClassRecord) -> Result[DeviceClassRecord] {
  var attributes: List[KernelParameter] = []
  for attribute in value.attributes {
    attributes = attributes.push(kernel_parameter_xsh(attribute)?)
  }

  return {
    ...value,
    name: text_observation_xsh(value.name)?,
    attributes: attributes,
  }
}

pure device_section_json(value: DeviceSection) -> JsonDeviceSection {
  var devices: List[JsonDeviceClassRecord] = []
  for device in value.devices {
    devices = devices.push(device_record_json(device))
  }

  return {
    status: section_status_json(value.status),
    devices: devices,
  }
}

pure device_section_xsh(value: JsonDeviceSection) -> Result[DeviceSection] {
  var devices: List[DeviceClassRecord] = []
  for device in value.devices {
    devices = devices.push(device_record_xsh(device)?)
  }

  return {
    status: section_status_xsh(value.status)?,
    devices: devices,
  }
}

pure report_json(value: SystemReport) -> SystemReportJson {
  var issues: List[JsonCollectionIssue] = []
  for issue in value.issues {
    issues = issues.push(collection_issue_json(issue))
  }

  return {
    ...value,
    source_mode: source_mode_json(value.source_mode),
    scope: observation_scope_json(value.scope),
    identity: identity_section_json(value.identity),
    cpu: cpu_section_json(value.cpu),
    memory: memory_section_json(value.memory),
    pci: pci_section_json(value.pci),
    usb: usb_section_json(value.usb),
    storage: storage_section_json(value.storage),
    network: network_section_json(value.network),
    sensors: sensor_section_json(value.sensors),
    power: power_section_json(value.power),
    firmware: firmware_section_json(value.firmware),
    kernel: kernel_section_json(value.kernel),
    processes: process_section_json(value.processes),
    devices: device_section_json(value.devices),
    issues: issues,
  }
}

pure report_xsh(value: SystemReportJson) -> Result[SystemReport] {
  var issues: List[CollectionIssue] = []
  for issue in value.issues {
    issues = issues.push(collection_issue_xsh(issue)?)
  }

  return {
    ...value,
    source_mode: source_mode_xsh(value.source_mode)?,
    scope: observation_scope_xsh(value.scope)?,
    identity: identity_section_xsh(value.identity)?,
    cpu: cpu_section_xsh(value.cpu)?,
    memory: memory_section_xsh(value.memory)?,
    pci: pci_section_xsh(value.pci)?,
    usb: usb_section_xsh(value.usb)?,
    storage: storage_section_xsh(value.storage)?,
    network: network_section_xsh(value.network)?,
    sensors: sensor_section_xsh(value.sensors)?,
    power: power_section_xsh(value.power)?,
    firmware: firmware_section_xsh(value.firmware)?,
    kernel: kernel_section_xsh(value.kernel)?,
    processes: process_section_xsh(value.processes)?,
    devices: device_section_xsh(value.devices)?,
    issues: issues,
  }
}

pure require_report_v1(report: SystemReport) -> Result[Unit] {
  if report.schema_version != 1 {
    return Err(Error(kind: "system-report-json", message: f"unsupported schema version '${report.schema_version}'"))
  }
  return Ok()
}

pure encode_typed_report_json(
  report: SystemReport,
  sensitive: Bool,
  pretty: Bool,
) -> Result[Str] {
  require_report_v1(report)?
  let output_report = if sensitive { report } else { redact_report(report) }
  return json.encode(report_json(output_report), pretty: pretty)
}

## Validates a dynamic report at the JSON boundary and emits one document.
export pure encode_report_json(
  report: Record,
  sensitive: Bool,
  pretty: Bool,
) -> Result[Str] {
  return encode_typed_report_json(report.require(SystemReport)?, sensitive, pretty)
}

## Validates the JSON v1 wire schema and restores its typed tag unions.
export pure decode_report_json(text: Str) -> Result[SystemReport] {
  let raw = json.decode(text)?
  let wire = raw.require(SystemReportJson)?

  if wire.schema_version != 1 {
    return Err(Error(kind: "system-report-json", message: f"unsupported schema version '${wire.schema_version}'"))
  }

  return report_xsh(wire)
}

pure terminal_quote(value: Str) -> Result[Str] {
  var safe = value
  for escape in [
    {source: "\u{0080}", replacement: "\\u{0080}"},
    {source: "\u{0081}", replacement: "\\u{0081}"},
    {source: "\u{0082}", replacement: "\\u{0082}"},
    {source: "\u{0083}", replacement: "\\u{0083}"},
    {source: "\u{0084}", replacement: "\\u{0084}"},
    {source: "\u{0085}", replacement: "\\u{0085}"},
    {source: "\u{0086}", replacement: "\\u{0086}"},
    {source: "\u{0087}", replacement: "\\u{0087}"},
    {source: "\u{0088}", replacement: "\\u{0088}"},
    {source: "\u{0089}", replacement: "\\u{0089}"},
    {source: "\u{008a}", replacement: "\\u{008a}"},
    {source: "\u{008b}", replacement: "\\u{008b}"},
    {source: "\u{008c}", replacement: "\\u{008c}"},
    {source: "\u{008d}", replacement: "\\u{008d}"},
    {source: "\u{008e}", replacement: "\\u{008e}"},
    {source: "\u{008f}", replacement: "\\u{008f}"},
    {source: "\u{0090}", replacement: "\\u{0090}"},
    {source: "\u{0091}", replacement: "\\u{0091}"},
    {source: "\u{0092}", replacement: "\\u{0092}"},
    {source: "\u{0093}", replacement: "\\u{0093}"},
    {source: "\u{0094}", replacement: "\\u{0094}"},
    {source: "\u{0095}", replacement: "\\u{0095}"},
    {source: "\u{0096}", replacement: "\\u{0096}"},
    {source: "\u{0097}", replacement: "\\u{0097}"},
    {source: "\u{0098}", replacement: "\\u{0098}"},
    {source: "\u{0099}", replacement: "\\u{0099}"},
    {source: "\u{009a}", replacement: "\\u{009a}"},
    {source: "\u{009b}", replacement: "\\u{009b}"},
    {source: "\u{009c}", replacement: "\\u{009c}"},
    {source: "\u{009d}", replacement: "\\u{009d}"},
    {source: "\u{009e}", replacement: "\\u{009e}"},
    {source: "\u{009f}", replacement: "\\u{009f}"},
    {source: "\u{00ad}", replacement: "\\u{00ad}"},
    {source: "\u{034f}", replacement: "\\u{034f}"},
    {source: "\u{061c}", replacement: "\\u{061c}"},
    {source: "\u{200b}", replacement: "\\u{200b}"},
    {source: "\u{200c}", replacement: "\\u{200c}"},
    {source: "\u{200d}", replacement: "\\u{200d}"},
    {source: "\u{200e}", replacement: "\\u{200e}"},
    {source: "\u{200f}", replacement: "\\u{200f}"},
    {source: "\u{2028}", replacement: "\\u{2028}"},
    {source: "\u{2029}", replacement: "\\u{2029}"},
    {source: "\u{202a}", replacement: "\\u{202a}"},
    {source: "\u{202b}", replacement: "\\u{202b}"},
    {source: "\u{202c}", replacement: "\\u{202c}"},
    {source: "\u{202d}", replacement: "\\u{202d}"},
    {source: "\u{202e}", replacement: "\\u{202e}"},
    {source: "\u{2060}", replacement: "\\u{2060}"},
    {source: "\u{2066}", replacement: "\\u{2066}"},
    {source: "\u{2067}", replacement: "\\u{2067}"},
    {source: "\u{2068}", replacement: "\\u{2068}"},
    {source: "\u{2069}", replacement: "\\u{2069}"},
    {source: "\u{206a}", replacement: "\\u{206a}"},
    {source: "\u{206b}", replacement: "\\u{206b}"},
    {source: "\u{206c}", replacement: "\\u{206c}"},
    {source: "\u{206d}", replacement: "\\u{206d}"},
    {source: "\u{206e}", replacement: "\\u{206e}"},
    {source: "\u{206f}", replacement: "\\u{206f}"},
    {source: "\u{feff}", replacement: "\\u{feff}"},
  ] {
    safe = safe.replace(escape.source, escape.replacement)
  }

  return json.encode(safe)
}

pure observation_display(value: TextObservation) -> Result[Str] {
  if value.value != null {
    let quoted = terminal_quote(value.value)?
    if value.state == Observed {
      return quoted
    }
    return f"[${observation_state_json(value.state)}] ${quoted}"
  }

  if value.raw_bytes_base64 != null {
    return terminal_quote(f"base64:${value.raw_bytes_base64}")
  }

  return f"[${observation_state_json(value.state)}]"
}

pure optional_text_display(value: Str?) -> Result[Str] {
  if value == null {
    return "unknown"
  }

  return terminal_quote(value)
}

pure byte_quantity(value: Int?) -> Str {
  if value == null {
    return "unknown"
  }
  if value < 0 {
    return "out of range"
  }

  let divisor = 1073741824
  let whole = value / divisor
  let fraction = (value % divisor) * 10 / divisor
  return f"${whole}.${fraction} GiB (${value} bytes)"
}

pure optional_frequency(value: Int?) -> Str {
  if value == null {
    return "unknown"
  }
  return f"${value} kHz"
}

pure optional_int_display(value: Int?) -> Str {
  if value == null {
    return "unknown"
  }
  return f"${value}"
}

pure key_part(prefix: Str, value: Str) -> Str {
  return f"${prefix}${value.count_chars()}:${value}"
}

pure optional_int_key(value: Int?) -> Str {
  if value == null {
    return "none;"
  }
  return f"int:${value};"
}

pure optional_str_key(value: Str?) -> Str {
  if value == null {
    return "none;"
  }
  return key_part("str:", value)
}

pure optional_bool_key(value: Bool?) -> Str {
  if value == null {
    return "none;"
  }
  return if value { "bool:true;" } else { "bool:false;" }
}

pure cpu_policy_key(policy: CpuFreqPolicy) -> Str {
  var key = ""
  key = f"${key}${optional_str_key(policy.driver)}"
  key = f"${key}${optional_str_key(policy.governor)}"
  key = f"${key}${policy.available_governors.len()}:"
  for governor in policy.available_governors {
    key = key_part(key, governor)
  }
  key = f"${key}${optional_int_key(policy.hardware_min_khz)}"
  key = f"${key}${optional_int_key(policy.hardware_max_khz)}"
  key = f"${key}${optional_int_key(policy.scaling_min_khz)}"
  key = f"${key}${optional_int_key(policy.scaling_max_khz)}"
  key = f"${key}${optional_int_key(policy.hardware_current_khz)}"
  key = f"${key}${optional_int_key(policy.requested_current_khz)}"
  key = f"${key}${optional_int_key(policy.governor_requested_khz)}"
  key = f"${key}${optional_int_key(policy.average_current_khz)}"
  key = f"${key}${optional_int_key(policy.bios_limit_khz)}"
  key = f"${key}${optional_int_key(policy.transition_latency_ns)}"
  key = f"${key}${policy.available_frequencies_khz.len()}:"
  for frequency in policy.available_frequencies_khz {
    key = f"${key}int:${frequency};"
  }
  key = f"${key}${optional_str_key(policy.energy_performance_preference)}"
  key = f"${key}${policy.available_energy_performance_preferences.len()}:"
  for preference in policy.available_energy_performance_preferences {
    key = key_part(key, preference)
  }
  key = f"${key}${optional_bool_key(policy.boost_supported)}"
  key = f"${key}${optional_bool_key(policy.boost_allowed)}"
  key = f"${key}${optional_bool_key(policy.boost_active)}"
  return f"${key}${optional_str_key(policy.boost_scope)}"
}

pure grouped_policy_lines(policies: List[CpuFreqPolicy]) -> Result[List[Str]] {
  let groups = policies |> group-by cpu_policy_key(.) |> sort-by .key
  var lines: List[Str] = []

  for group in groups {
    var cpu_ids: List[Int] = []
    for policy in group.items {
      for cpu_id in policy.related_cpus {
        if cpu_id not in cpu_ids {
          cpu_ids = cpu_ids.push(cpu_id)
        }
      }
    }

    cpu_ids = cpu_ids |> sort-by .
    let first = group.items[0]
    if first != null {
      let driver = optional_text_display(first.driver)?
      let governor = optional_text_display(first.governor)?
      lines = lines.push(
        f"  ${group.items.len()} identical policy group on CPUs ${cpu_ids.join(",")}: ${driver}, ${governor}, ${optional_frequency(first.scaling_min_khz)}..${optional_frequency(first.scaling_max_khz)}",
      )
    }
  }

  return lines
}

pure section_entity_summary(status: SectionStatus, count: Int, label: Str) -> Str {
  if status.state == NotRequested {
    return "not requested"
  }
  if status.state == Unsupported {
    return "unsupported"
  }
  if status.state == Absent {
    return "absent"
  }
  if status.state == Truncated {
    return f"at least ${count} ${label}"
  }
  if status.state == Partial or ! status.enumeration_succeeded {
    return f"${count} ${label} (partial)"
  }
  return f"${count} ${label}"
}

pure mount_usage_summary(section: StorageSection) -> Str {
  var observed = 0
  var skipped = 0
  var unavailable = 0
  for mount in section.mounts {
    if mount.usage_state == Observed {
      observed += 1
    } else if mount.usage_state == NotRequested {
      skipped += 1
    } else {
      unavailable += 1
    }
  }
  return f"capacity observed on ${observed} mounts; ${skipped} skipped by policy; ${unavailable} unavailable"
}

pure render_typed_text(
  report: SystemReport,
  full: Bool,
  sensitive: Bool,
) -> Result[Str] {
  require_report_v1(report)?
  let output_report = if sensitive { report } else { redact_report(report) }
  let os_name = output_report.identity.os_release?.pretty_name
    ?? output_report.identity.os_release?.name
    ?? "unknown"
  let kernel = terminal_quote(output_report.identity.kernel_release ?? "unknown")?
  let architecture = terminal_quote(output_report.identity.architecture ?? "unknown")?
  let host_claim = terminal_quote(output_report.scope.host_claim)?
  let platform = terminal_quote(output_report.scope.platform)?
  let os = terminal_quote(os_name)?
  let privacy = if output_report.redacted {
    "Identifiers are redacted; this reduces exposure but does not guarantee anonymity."
  } else {
    "Sensitive identifiers are included."
  }
  let memory_summary = if output_report.memory.status.state == NotRequested {
    "not requested"
  } else {
    f"${byte_quantity(output_report.memory.host.total_bytes)} total; ${byte_quantity(output_report.memory.host.available_bytes)} available; swap ${byte_quantity(output_report.memory.host.swap_total_bytes)} total/${byte_quantity(output_report.memory.host.swap_free_bytes)} free"
  }
  let storage_usage = if output_report.storage.status.state == NotRequested {
    "not requested"
  } else {
    mount_usage_summary(output_report.storage)
  }
  let power_summary = f"${section_entity_summary(output_report.power.status, output_report.power.supplies.len(), "supplies")}; ${section_entity_summary(output_report.power.status, output_report.power.cap_zones.len(), "powercap zones")}"
  var lines = [
    "XSH system report v1",
    f"Scope: ${host_claim} on ${platform}; source mode ${source_mode_json(output_report.source_mode)}",
    f"Privacy: ${privacy}",
    f"System: ${os}; kernel ${kernel}; architecture ${architecture}",
    f"Uptime: ${optional_int_display(output_report.identity.uptime_seconds)} seconds",
    f"CPU: ${section_entity_summary(output_report.cpu.status, output_report.cpu.cpus.len(), "CPUs")}; ${section_entity_summary(output_report.cpu.status, output_report.cpu.frequency_policies.len(), "frequency policies")}",
    f"Memory: ${memory_summary}",
    f"PCI: ${section_entity_summary(output_report.pci.status, output_report.pci.functions.len(), "functions")}; USB: ${section_entity_summary(output_report.usb.status, output_report.usb.devices.len(), "devices")}",
    f"Storage: ${section_entity_summary(output_report.storage.status, output_report.storage.devices.len(), "block devices")}; ${section_entity_summary(output_report.storage.status, output_report.storage.mounts.len(), "mounts")}; ${storage_usage}",
    f"Network: ${section_entity_summary(output_report.network.status, output_report.network.links.len(), "links")}; ${section_entity_summary(output_report.network.status, output_report.network.routes.len(), "routes")}; ${section_entity_summary(output_report.network.status, output_report.network.rules.len(), "rules")}",
    f"Sensors: ${section_entity_summary(output_report.sensors.status, output_report.sensors.channels.len(), "channels")}; power: ${power_summary}",
    f"Kernel: ${section_entity_summary(output_report.kernel.status, output_report.kernel.modules.len(), "modules")}; visible processes: ${section_entity_summary(output_report.processes.status, output_report.processes.processes.len(), "processes")}",
    f"Firmware: ${section_entity_summary(output_report.firmware.status, output_report.firmware.records.len(), "records")}; device classes: ${section_entity_summary(output_report.devices.status, output_report.devices.devices.len(), "devices")}",
    f"Issues: ${output_report.issues.len()}",
    "Section status:",
    f"  identity=${section_state_json(output_report.identity.status.state)} cpu=${section_state_json(output_report.cpu.status.state)} memory=${section_state_json(output_report.memory.status.state)}",
    f"  pci=${section_state_json(output_report.pci.status.state)} usb=${section_state_json(output_report.usb.status.state)} storage=${section_state_json(output_report.storage.status.state)}",
    f"  network=${section_state_json(output_report.network.status.state)} sensors=${section_state_json(output_report.sensors.status.state)} power=${section_state_json(output_report.power.status.state)}",
    f"  firmware=${section_state_json(output_report.firmware.status.state)} kernel=${section_state_json(output_report.kernel.status.state)} processes=${section_state_json(output_report.processes.status.state)} devices=${section_state_json(output_report.devices.status.state)}",
  ]

  if output_report.cpu.status.state != NotRequested {
    let policies = grouped_policy_lines(output_report.cpu.frequency_policies)?
    if policies.len() > 0 {
      lines = lines.push("CPU frequency policy groups:")
      lines = lines.extend(policies)
    }
  }

  if full {
    if output_report.cpu.status.state != NotRequested {
      lines = lines.push("CPU caches:")
      for cache in output_report.cpu.caches {
        lines = lines.push(f"  L${cache.level} ${terminal_quote(cache.kind)?} cache on CPU ${cache.owner_cpu_id} (sysfs index ${cache.sysfs_index}): ${byte_quantity(cache.size_bytes)}; shared CPUs ${cache.shared_cpus.join(",")}")
      }

      lines = lines.push("CPU idle states:")
      for idle in output_report.cpu.idle_states {
        lines = lines.push(f"  CPU ${optional_int_display(idle.cpu_id)} ${terminal_quote(idle.name)?}: latency ${optional_int_display(idle.latency_us)} us, residency ${optional_int_display(idle.residency_us)} us")
      }

      lines = lines.push("CPU vulnerabilities:")
      for vulnerability in output_report.cpu.vulnerabilities {
        lines = lines.push(f"  ${terminal_quote(vulnerability.name)?}: ${observation_display(vulnerability.description)?}")
      }
    }

    if output_report.pci.status.state != NotRequested {
      lines = lines.push("PCI functions:")
      for function in output_report.pci.functions {
        let address = optional_text_display(function.address)?
        let driver = optional_text_display(function.driver)?
        lines = lines.push(f"  ${address} vendor=${optional_int_display(function.vendor_id)} device=${optional_int_display(function.device_id)} class=${optional_int_display(function.class_code)} driver=${driver}")
      }
    }

    if output_report.usb.status.state != NotRequested {
      lines = lines.push("USB devices:")
      for device in output_report.usb.devices {
        let name = optional_text_display(device.sysfs_name)?
        let port = optional_text_display(device.port_path)?
        let serial = observation_display(device.serial)?
        lines = lines.push(f"  ${name} port=${port} id=${optional_int_display(device.vendor_id)}:${optional_int_display(device.product_id)} serial=${serial}")
        for interface in device.interfaces {
          lines = lines.push(f"    interface ${interface.number} ${optional_text_display(interface.name)?} driver=${optional_text_display(interface.driver)?} alternate=${optional_int_display(interface.active_alternate)}")
        }
      }
    }

    if output_report.storage.status.state != NotRequested {
      lines = lines.push("Block devices:")
      for device in output_report.storage.devices {
        let name = optional_text_display(device.name)?
        let model = observation_display(device.model)?
        lines = lines.push(f"  ${name} ${optional_int_display(device.major)}:${optional_int_display(device.minor)} ${byte_quantity(device.size_bytes)} model=${model}")
        lines = lines.push(f"    kind=${terminal_quote(device.kind)?} block-parent=${optional_int_display(device.parent_device_index)} pci-parent=${optional_int_display(device.parent_pci_function_index)} holders=${device.holder_indices.join(",")} slaves=${device.slave_indices.join(",")} scheduler=${optional_text_display(device.active_scheduler)?}")
      }
      lines = lines.push("Mounts:")
      for mount in output_report.storage.mounts {
        let target = observation_display(mount.target)?
        let source = observation_display(mount.source)?
        lines = lines.push(f"  ${target} type=${terminal_quote(mount.filesystem)?} source=${source} usage=${byte_quantity(mount.usage_used_bytes)}/${byte_quantity(mount.usage_total_bytes)}")
      }
    }

    if output_report.network.status.state != NotRequested {
      lines = lines.push("Network links:")
      for link in output_report.network.links {
        let name = observation_display(link.name)?
        let mac = observation_display(link.mac)?
        lines = lines.push(f"  ifindex=${link.ifindex} name=${name} mtu=${optional_int_display(link.mtu)} state=${optional_text_display(link.operational_state)?} mac=${mac}")
        for address in link.addresses {
          lines = lines.push(f"    ${terminal_quote(address.family)?} ${observation_display(address.address)?}/${address.prefix_length}")
        }
      }

      lines = lines.push("Network routes:")
      for route in output_report.network.routes {
        lines = lines.push(f"  ${terminal_quote(route.family)?} ${observation_display(route.destination)?}/${route.prefix_length} via ${observation_display(route.gateway)?} table=${route.table} metric=${optional_int_display(route.metric)} output-ifindex=${optional_int_display(route.output_ifindex)}")
        for nexthop in route.nexthops {
          lines = lines.push(f"    nexthop ifindex=${nexthop.ifindex} flags=${nexthop.flags} hops=${nexthop.hops} gateway=${observation_display(nexthop.gateway)?}")
        }
      }

      lines = lines.push("Network policy rules:")
      for rule in output_report.network.rules {
        lines = lines.push(f"  ${terminal_quote(rule.family)?} priority=${optional_int_display(rule.priority)} from ${observation_display(rule.source)?} to ${observation_display(rule.destination)?} table=${optional_int_display(rule.table)} action=${terminal_quote(rule.action)?}")
      }
    }

    if output_report.sensors.status.state != NotRequested {
      lines = lines.push("Sensors:")
      for channel in output_report.sensors.channels {
        lines = lines.push(f"  ${terminal_quote(channel.chip)?}/${terminal_quote(channel.channel)?} ${observation_display(channel.label)?}=${optional_int_display(channel.value)} ${terminal_quote(channel.unit)?}")
      }
      lines = lines.push("Thermal zones:")
      for zone in output_report.sensors.thermal_zones {
        lines = lines.push(f"  zone ${zone.id} kind=${optional_text_display(zone.kind)?} temperature=${optional_int_display(zone.temperature_millidegrees)} millidegrees Celsius")
      }
    }

    if output_report.power.status.state != NotRequested {
      lines = lines.push("Power supplies:")
      for supply in output_report.power.supplies {
        lines = lines.push(f"  ${terminal_quote(supply.name)?} kind=${optional_text_display(supply.kind)?} status=${optional_text_display(supply.status)?} capacity=${optional_int_display(supply.capacity_percent)}%")
      }
      lines = lines.push("Power limits:")
      for zone in output_report.power.cap_zones {
        lines = lines.push(f"  ${terminal_quote(zone.name)?} energy=${optional_int_display(zone.energy_uj)} uJ limit=${optional_int_display(zone.power_limit_uw)} uW window=${optional_int_display(zone.time_window_us)} us")
      }
    }

    if output_report.kernel.status.state != NotRequested {
      lines = lines.push("Kernel modules:")
      for module in output_report.kernel.modules {
        lines = lines.push(f"  ${terminal_quote(module.name)?} size=${module.size_bytes} bytes users=${module.users} state=${terminal_quote(module.state)?}")
      }
      lines = lines.push(f"Kernel command line: ${observation_display(output_report.kernel.command_line)?}")
      lines = lines.push("Kernel parameters:")
      for parameter in output_report.kernel.parameters {
        lines = lines.push(f"  ${terminal_quote(parameter.name)?}=${observation_display(parameter.value)?}")
      }
      lines = lines.push("Selected sysctls:")
      for parameter in output_report.kernel.sysctls {
        lines = lines.push(f"  ${terminal_quote(parameter.name)?}=${observation_display(parameter.value)?}")
      }
    }

    if output_report.processes.status.state != NotRequested {
      lines = lines.push("Visible processes:")
      for process in output_report.processes.processes {
        lines = lines.push(f"  pid=${process.pid} ppid=${process.parent_pid} state=${terminal_quote(process.state)?} threads=${optional_int_display(process.thread_count)} rss=${byte_quantity(process.resident_bytes)} virtual=${byte_quantity(process.virtual_bytes)} command=${observation_display(process.command)?}")
      }
    }

    if output_report.firmware.status.state != NotRequested {
      lines = lines.push("Firmware records:")
      for record in output_report.firmware.records {
        lines = lines.push(f"  type=${record.record_type} handle=${record.handle} formatted-length=${record.formatted_length} string-count=${record.strings.len()}")
      }
    }

    if output_report.devices.status.state != NotRequested {
      lines = lines.push("Device classes:")
      for device in output_report.devices.devices {
        lines = lines.push(f"  ${terminal_quote(device.class)?} name=${observation_display(device.name)?} driver=${optional_text_display(device.driver)?} pci-parent=${optional_int_display(device.parent_pci_function_index)} usb-parent=${optional_int_display(device.parent_usb_device_index)}")
      }
    }

    lines = lines.push("Collection issues:")
    for issue in output_report.issues {
      lines = lines.push(f"  ${terminal_quote(issue.section)?}.${terminal_quote(issue.field)?}: ${observation_state_json(issue.state)} ${observation_display(issue.detail)?}")
    }
  }

  return f"""${lines.join("\n")}
"""
}

## Validates a dynamic report and renders terminal-safe text.
export pure render_text(
  report: Record,
  full: Bool,
  sensitive: Bool,
) -> Result[Str] {
  return render_typed_text(report.require(SystemReport)?, full, sensitive)
}

## Removes the payload of a sensitive observation while retaining its shape.
export pure redact_text_observation(observation: TextObservation) -> TextObservation {
  if observation.value != null or observation.raw_bytes_base64 != null {
    return {
      state: Redacted,
      value: null,
      raw_bytes_base64: null,
    }
  }

  return observation
}

pure redact_firmware_identity(identity: FirmwareIdentity) -> FirmwareIdentity {
  return {
    ...identity,
    serial: redact_text_observation(identity.serial),
    uuid: redact_text_observation(identity.uuid),
  }
}

pure redact_identity_section(section: IdentitySection) -> IdentitySection {
  var firmware = section.firmware
  if firmware != null {
    firmware = redact_firmware_identity(firmware)
  }

  return {
    ...section,
    hostname: redact_text_observation(section.hostname),
    boot_id: redact_text_observation(section.boot_id),
    firmware: firmware,
  }
}

pure redact_memory_section(section: MemorySection) -> MemorySection {
  var swaps: List[SwapDevice] = []
  for swap in section.swaps {
    swaps = swaps.push({
      ...swap,
      name: redact_text_observation(swap.name),
    })
  }

  var cgroup: List[CgroupResource] = []
  for resource in section.cgroup {
    cgroup = cgroup.push({
      ...resource,
      path: redact_text_observation(resource.path),
    })
  }

  return {...section, swaps: swaps, cgroup: cgroup}
}

pure redact_pci_section(section: PciSection) -> PciSection {
  var functions: List[PciFunction] = []
  for function in section.functions {
    functions = functions.push({
      ...function,
      address: null,
      domain: null,
      bus: null,
      device: null,
      function: null,
      iommu_group: null,
    })
  }

  return {...section, functions: functions}
}

pure redact_usb_section(section: UsbSection) -> UsbSection {
  var devices: List[UsbDevice] = []
  for device in section.devices {
    var interfaces: List[UsbInterface] = []
    for interface in device.interfaces {
      interfaces = interfaces.push({...interface, name: null})
    }
    devices = devices.push({
      ...device,
      sysfs_name: null,
      port_path: null,
      bus_number: null,
      device_number: null,
      serial: redact_text_observation(device.serial),
      interfaces: interfaces,
    })
  }

  return {...section, devices: devices}
}

pure redact_storage_section(section: StorageSection) -> StorageSection {
  var devices: List[BlockDevice] = []
  for device in section.devices {
    devices = devices.push({
      ...device,
      name: null,
      major: null,
      minor: null,
      model: redact_text_observation(device.model),
      firmware: redact_text_observation(device.firmware),
    })
  }

  var mounts: List[Mount] = []
  for mount in section.mounts {
    mounts = mounts.push({
      ...mount,
      major: null,
      minor: null,
      root: redact_text_observation(mount.root),
      target: redact_text_observation(mount.target),
      source: redact_text_observation(mount.source),
    })
  }

  return {...section, devices: devices, mounts: mounts}
}

pure redact_network_section(section: NetworkSection) -> NetworkSection {
  var links: List[NetworkLink] = []
  for link in section.links {
    var addresses: List[NetworkAddress] = []
    for address in link.addresses {
      addresses = addresses.push({
        ...address,
        address: redact_text_observation(address.address),
        broadcast: redact_text_observation(address.broadcast),
        attributes: redact_network_attributes(address.attributes),
      })
    }

    links = links.push({
      ...link,
      name: redact_text_observation(link.name),
      mac: redact_text_observation(link.mac),
      addresses: addresses,
      attributes: redact_network_attributes(link.attributes),
    })
  }

  var routes: List[NetworkRoute] = []
  for route in section.routes {
    var nexthops: List[NetworkNexthop] = []
    for nexthop in route.nexthops {
      nexthops = nexthops.push({
        ...nexthop,
        gateway: redact_text_observation(nexthop.gateway),
      })
    }
    routes = routes.push({
      ...route,
      destination: redact_text_observation(route.destination),
      source: redact_text_observation(route.source),
      preferred_source: redact_text_observation(route.preferred_source),
      gateway: redact_text_observation(route.gateway),
      nexthops: nexthops,
      attributes: redact_network_attributes(route.attributes),
    })
  }

  var rules: List[NetworkRule] = []
  for rule in section.rules {
    rules = rules.push({
      ...rule,
      source: redact_text_observation(rule.source),
      destination: redact_text_observation(rule.destination),
      attributes: redact_network_attributes(rule.attributes),
    })
  }

  return {
    ...section,
    links: links,
    routes: routes,
    rules: rules,
  }
}

pure redact_network_attributes(values: List[NetworkAttribute]) -> List[NetworkAttribute] {
  var output: List[NetworkAttribute] = []
  for attribute in values {
    output = output.push({...attribute, data: redact_text_observation(attribute.data)})
  }
  return output
}

pure redact_firmware_section(section: FirmwareSection) -> FirmwareSection {
  var records: List[FirmwareRecord] = []
  for record in section.records {
    var strings: List[TextObservation] = []
    for value in record.strings {
      strings = strings.push(redact_text_observation(value))
    }
    records = records.push({...record, strings: strings})
  }

  return {
    ...section,
    records: records,
    limitation: redact_text_observation(section.limitation),
  }
}

pure redact_kernel_section(section: KernelSection) -> KernelSection {
  var parameters: List[KernelParameter] = []
  for parameter in section.parameters {
    parameters = parameters.push({
      ...parameter,
      value: redact_text_observation(parameter.value),
    })
  }

  var sysctls: List[KernelParameter] = []
  for parameter in section.sysctls {
    sysctls = sysctls.push({
      ...parameter,
      value: redact_text_observation(parameter.value),
    })
  }

  return {
    ...section,
    command_line: redact_text_observation(section.command_line),
    parameters: parameters,
    sysctls: sysctls,
  }
}

pure redact_process_section(section: ProcessSection) -> ProcessSection {
  var processes: List[ProcessRecord] = []
  for process in section.processes {
    processes = processes.push({
      ...process,
      cgroup: redact_text_observation(process.cgroup),
    })
  }

  return {...section, processes: processes}
}

pure redact_device_section(section: DeviceSection) -> DeviceSection {
  var devices: List[DeviceClassRecord] = []
  for device in section.devices {
    var attributes: List[KernelParameter] = []
    for attribute in device.attributes {
      attributes = attributes.push({
        ...attribute,
        value: redact_text_observation(attribute.value),
      })
    }

    devices = devices.push({
      ...device,
      name: redact_text_observation(device.name),
      attributes: attributes,
    })
  }

  return {...section, devices: devices}
}

pure redact_issue_field(section: Str, field: Str) -> Str {
  let parts = field.split(".")
  if parts.len() < 3 {
    return field
  }

  let root = parts[0]
  if root == null {
    return field
  }
  if section == "pci" and root == "functions" {
    let leaf = parts[parts.len() - 1] ?? ""
    return f"functions.redacted.${leaf}"
  }
  if section == "usb" and root == "devices" {
    return "devices.redacted"
  }
  let leaf = parts[parts.len() - 1] ?? ""
  if section == "storage" and root == "devices" {
    return f"devices.redacted.${leaf}"
  }
  if section == "devices" and (root == "drm" or root == "sound" or root == "input") {
    return f"${root}.redacted.${leaf}"
  }
  return field
}

## Applies share-safer redaction while keeping indexed parent and device relationships.
export pure redact_report(report: SystemReport) -> SystemReport {
  var scope = report.scope
  var source_roots: List[Str] = []
  for _ in scope.source_roots {
    source_roots = source_roots.push("redacted")
  }

  scope = {
    ...scope,
    source_roots: source_roots,
    mount_namespace: redact_text_observation(scope.mount_namespace),
    network_namespace: redact_text_observation(scope.network_namespace),
    pid_namespace: redact_text_observation(scope.pid_namespace),
    cgroup_namespace: redact_text_observation(scope.cgroup_namespace),
    visible_cgroup: redact_text_observation(scope.visible_cgroup),
  }

  var issues: List[CollectionIssue] = []
  for issue in report.issues {
    issues = issues.push({
      ...issue,
      field: redact_issue_field(issue.section, issue.field),
      detail: redact_text_observation(issue.detail),
    })
  }

  return {
      ...report,
      scope: scope,
      redacted: true,
      identity: redact_identity_section(report.identity),
      memory: redact_memory_section(report.memory),
      pci: redact_pci_section(report.pci),
      usb: redact_usb_section(report.usb),
    storage: redact_storage_section(report.storage),
    network: redact_network_section(report.network),
    firmware: redact_firmware_section(report.firmware),
    kernel: redact_kernel_section(report.kernel),
    processes: redact_process_section(report.processes),
    devices: redact_device_section(report.devices),
    issues: issues,
  }
}
