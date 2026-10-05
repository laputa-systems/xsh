##! Typed Linux inventory model and pure presentation helpers.
const max_cpu_list_identifiers = 65536

## Describes the outcome for one observed value or field.
export enum ObservationState: Str {
    Observed = "observed",
    Absent = "absent",
    Unsupported = "unsupported",
    PermissionDenied = "permission_denied",
    NotRequested = "not_requested",
    Redacted = "redacted",
    Malformed = "malformed",
    Disappeared = "disappeared",
    Raced = "raced",
    Truncated = "truncated",
    RangeFailure = "range_failure",
    ReadFailure = "read_failure",
}

## Describes whether a section's requested enumeration completed.
export enum SectionState: Str {
    Complete = "complete",
    Partial = "partial",
    SectionAbsent = "absent",
    SectionUnsupported = "unsupported",
    SectionPermissionDenied = "permission_denied",
    SectionNotRequested = "not_requested",
    SectionRedacted = "redacted",
    SectionMalformed = "malformed",
    SectionDisappeared = "disappeared",
    SectionRaced = "raced",
    SectionTruncated = "truncated",
}

## Failures returned by report parsing, selection, and schema validation.
export error SystemReportError {
    InvalidCpuList
    InvalidSection
    InvalidJson
    UnsupportedSchema(version: Int, message: Str)
    InvalidProcStat
    InvalidExecutionUnits
    UnsupportedPlatform
}

## Identifies whether observations came from a live host or a replay source.
export enum SourceMode: Str {
    LiveLinux = "live_linux",
    Replay = "replay",
    SyntheticFixture = "synthetic_fixture",
    CapturedReplay = "captured_replay",
    ContainerLive = "container_live",
    PhysicalLive = "physical_live",
}

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
  uts_namespace: TextObservation,
  ipc_namespace: TextObservation,
  user_namespace: TextObservation,
  time_namespace: TextObservation,
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

## Describes one CPUFreq policy with kernel scaling, governor request, and hardware values separated.
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
  scaling_current_khz: Int?,
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
  state_index: Int?,
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
export pure frequency_policies_for_cpu(policies: List[CpuFreqPolicy], cpu_id: Int) -> List[CpuFreqPolicy] {
  [policy for policy in policies if cpu_id in policy.related_cpus]
}

pure cpu_list_error(message: Str) -> SystemReportError {
  .InvalidCpuList(message)
}

pure parse_cpu_list_integer(value: Str, decimal: Regex) -> Result[Int] {
  guard decimal.matches(value) else {
    return Err(cpu_list_error("CPU list contains a non-decimal identifier"))
  }

  if let Ok(identifier) = value.parse_int() {
    Ok(identifier)
  } else {
    Err(cpu_list_error("CPU identifier is outside the supported integer range"))
  }
}

## Parses Linux cpulist syntax with sparse IDs and a bounded expanded size.
export pure parse_cpu_list(text: Str) -> Result[List[Int], Error] {
  if text == "" or text.trim() != text {
    return Err(cpu_list_error("CPU list is empty or contains surrounding whitespace"))
  }

  let decimal = rx"^[0-9]+$"
  var identifiers = []

  for item in text.split(",") {
    let bounds = item.split("-")
    if bounds.len() == 1 {
      let value = parse_cpu_list_integer(item, decimal)?
      if identifiers.len() >= max_cpu_list_identifiers {
        return Err(cpu_list_error("CPU list exceeds 65536 identifiers"))
      }

      identifiers += [value]
      continue
    }

    return Err(cpu_list_error("CPU list contains a malformed range")) when bounds.len() != 2

    let start_text = bounds[0]
    let end_text = bounds[1]
    let start = parse_cpu_list_integer(start_text, decimal)?
    let end = parse_cpu_list_integer(end_text, decimal)?
    return Err(cpu_list_error("CPU list range ends before it starts")) when end < start

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
      identifiers += [identifier]
      identifier = identifier + 1
    }

    identifiers += [end]
  }

  if identifiers.is_empty() {
    return Err(cpu_list_error("CPU list contains no identifiers"))
  }

  var seen: Set[Str] = set.empty()
  for identifier in identifiers {
    let key = f"{identifier}"
    return Err(cpu_list_error("CPU list contains a duplicate identifier")) when key in seen

    seen = seen.add(key)
  }

  identifiers |> sort-by .
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
export pure pci_parent_function(functions: List[PciFunction], child: PciFunction) -> PciFunction? {
  let parent_index = child.parent_function_index ?? -1

  return null when parent_index < 0 or parent_index >= functions.len()

  functions[parent_index]
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
  configuration_value: Int?,
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
  runtime_status: Str?,
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

## Keeps the hwmon class entry separate from its possibly repeated chip name.
export type SensorChannel = {
  chip: Str,
  chip_entry_name: Str?,
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
  parent_pci_function_index: Int?,
  parent_usb_device_index: Int?,
}

## Describes one thermal-zone trip point by its exported index and hysteresis.
export type ThermalTrip = {index: Int?, kind: Str, temperature_millidegrees: Int?, hysteresis_millidegrees: Int?}

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

## Describes one indexed power limit exported by a powercap zone.
export type PowerCapConstraint = {
  index: Int,
  name: Str?,
  power_limit_uw: Int?,
  time_window_us: Int?,
}

## Describes one kernel-exported powercap zone; parent matches another zone's
## entry_name and is absent for zones directly beneath the powercap class.
export type PowerCapZone = {
  entry_name: Str,
  name: Str,
  parent: Str?,
  energy_uj: Int?,
  maximum_energy_range_uj: Int?,
  constraints: List[PowerCapConstraint],
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

## Describes one loaded kernel module; users is null when procfs cannot expose its use count.
export type KernelModule = {name: Str, size_bytes: Int, users: Int?, state: Str}

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

## Describes one visible process without command-line or environment contents; uid is the real UID from status, and cgroup indexes resolve into MemorySection.cgroup.
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

## Keeps the class entry name separate from its possibly repeated display label.
## Parent indexes refer to records in the same report, not stable host identities.
export type DeviceClassRecord = {
  class: Str,
  entry_name: TextObservation,
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
export enum ReportSection {
    ReportIdentity,
    ReportCpu,
    ReportMemory,
    ReportPci,
    ReportUsb,
    ReportStorage,
    ReportNetwork,
    ReportSensors,
    ReportPower,
    ReportFirmware,
    ReportKernel,
    ReportProcesses,
    ReportDevices,
}

pure report_section_name(section: ReportSection) -> Str {
  match section {
    ReportIdentity => "identity"
    ReportCpu => "cpu"
    ReportMemory => "memory"
    ReportPci => "pci"
    ReportUsb => "usb"
    ReportStorage => "storage"
    ReportNetwork => "network"
    ReportSensors => "sensors"
    ReportPower => "power"
    ReportFirmware => "firmware"
    ReportKernel => "kernel"
    ReportProcesses => "processes"
    ReportDevices => "devices"
  }
}

## Converts the command's section spelling into its closed report selector.
export pure parse_report_section(value: Str) -> Result[ReportSection, Error] {
  match value {
    "identity" => Ok(ReportIdentity)
    "cpu" => Ok(ReportCpu)
    "memory" => Ok(ReportMemory)
    "pci" => Ok(ReportPci)
    "usb" => Ok(ReportUsb)
    "storage" => Ok(ReportStorage)
    "network" => Ok(ReportNetwork)
    "sensors" => Ok(ReportSensors)
    "power" => Ok(ReportPower)
    "firmware" => Ok(ReportFirmware)
    "kernel" => Ok(ReportKernel)
    "processes" => Ok(ReportProcesses)
    "devices" => Ok(ReportDevices)
    else => Err(SystemReportError.InvalidSection(f"unknown report section '{value}'"))
  }
}

pure not_requested_status() -> SectionStatus {
  {state: SectionNotRequested, enumeration_succeeded: false}
}

pure not_requested_text() -> TextObservation {
  {state: NotRequested, value: null, raw_bytes_base64: null}
}

pure not_requested_cpu(section: CpuSection) -> CpuSection {
  {
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
  {
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
  {...section, status: not_requested_status(), functions: []}
}

pure not_requested_usb(section: UsbSection) -> UsbSection {
  {...section, status: not_requested_status(), devices: []}
}

pure not_requested_storage(section: StorageSection) -> StorageSection {
  {...section, status: not_requested_status(), devices: [], mounts: []}
}

pure not_requested_network(section: NetworkSection) -> NetworkSection {
  {...section, status: not_requested_status(), links: [], routes: [], rules: []}
}

pure not_requested_sensors(section: SensorSection) -> SensorSection {
  {...section, status: not_requested_status(), channels: [], thermal_zones: []}
}

pure not_requested_power(section: PowerSection) -> PowerSection {
  {...section, status: not_requested_status(), supplies: [], cap_zones: []}
}

pure not_requested_firmware(section: FirmwareSection) -> FirmwareSection {
  {
    ...section,
    status: not_requested_status(),
    source: "not-requested",
    records: [],
    limitation: not_requested_text(),
  }
}

pure not_requested_kernel(section: KernelSection) -> KernelSection {
  {
    ...section,
    status: not_requested_status(),
    command_line: not_requested_text(),
    modules: [],
    parameters: [],
    sysctls: [],
  }
}

pure not_requested_processes(section: ProcessSection) -> ProcessSection {
  {...section, status: not_requested_status(), processes: []}
}

pure clear_process_cgroup_resource_links(section: ProcessSection) -> ProcessSection {
  var processes: List[ProcessRecord] = [
    {...process_item, cgroup_resource_index: null}
    for process_item in section.processes
  ]
  {...section, processes: processes}
}

pure not_requested_devices(section: DeviceSection) -> DeviceSection {
  {...section, status: not_requested_status(), devices: []}
}

pure selection_needs_pci(selected: ReportSection) {
  selected == ReportPci or selected == ReportUsb or selected == ReportNetwork or selected == ReportDevices or selected == ReportSensors
}

pure selection_needs_usb(selected: ReportSection) {
  selected == ReportUsb or selected == ReportNetwork or selected == ReportDevices or selected == ReportSensors
}

pure select_report_domain(report: SystemReport, selected: ReportSection) -> SystemReport {
  var issues: List[CollectionIssue] = []
  let selected_name = report_section_name(selected)
  let keep_pci = selection_needs_pci(selected)
  let keep_usb = selection_needs_usb(selected)
  for issue in report.issues {
    if issue.section == "identity" or issue.section == "scope" or issue.section == selected_name or (keep_pci and issue.section == "pci") or (keep_usb and issue.section == "usb") {
      issues += [issue]
    }
  }

  {
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
    processes: if selected == ReportProcesses {
      clear_process_cgroup_resource_links(report.processes)
    } else {
      not_requested_processes(report.processes)
    },
    devices: if selected == ReportDevices { report.devices } else { not_requested_devices(report.devices) },
    issues: issues,
  }
}

## Keeps identity and one named domain, marking every excluded domain.
export pure select_report_section(report: Record, selected: Str) -> Result[Record, Error] {
  let typed = report.require(SystemReport)?
  select_report_domain(typed, parse_report_section(selected)?)
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
  uts_namespace: JsonTextObservation,
  ipc_namespace: JsonTextObservation,
  user_namespace: JsonTextObservation,
  time_namespace: JsonTextObservation,
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
  runtime_status: Str?,
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
  chip_entry_name: Str?,
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
  parent_pci_function_index: Int?,
  parent_usb_device_index: Int?,
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

# Saved reports with one limit lacked the kernel entry name, so replay cannot
# reconstruct a missing parent link from their display names.
type LegacyPowerCapZone = {
  name: Str,
  parent: Str?,
  energy_uj: Int?,
  maximum_energy_range_uj: Int?,
  constraint_name: Str?,
  power_limit_uw: Int?,
  time_window_us: Int?,
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
  entry_name: JsonTextObservation,
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
    Observed => "observed"
    Absent => "absent"
    Unsupported => "unsupported"
    PermissionDenied => "permission_denied"
    NotRequested => "not_requested"
    Redacted => "redacted"
    Malformed => "malformed"
    Disappeared => "disappeared"
    Raced => "raced"
    Truncated => "truncated"
    RangeFailure => "range_failure"
    ReadFailure => "read_failure"
  }
}

pure observation_state_xsh(value: Str) -> Result[ObservationState] {
  if let Ok(state) = value.require(ObservationState) {
    Ok(state)
  } else {
    Err(SystemReportError.InvalidJson(f"unknown observation state '{value}'"))
  }
}

pure section_state_json(state: SectionState) -> Str {
  match state {
    Complete => "complete"
    Partial => "partial"
    SectionAbsent => "absent"
    SectionUnsupported => "unsupported"
    SectionPermissionDenied => "permission_denied"
    SectionNotRequested => "not_requested"
    SectionRedacted => "redacted"
    SectionMalformed => "malformed"
    SectionDisappeared => "disappeared"
    SectionRaced => "raced"
    SectionTruncated => "truncated"
  }
}

pure section_state_xsh(value: Str) -> Result[SectionState] {
  if let Ok(state) = value.require(SectionState) {
    Ok(state)
  } else {
    Err(SystemReportError.InvalidJson(f"unknown section state '{value}'"))
  }
}

pure source_mode_json(mode: SourceMode) -> Str {
  match mode {
    LiveLinux => "live_linux"
    Replay => "replay"
    SyntheticFixture => "synthetic_fixture"
    CapturedReplay => "captured_replay"
    ContainerLive => "container_live"
    PhysicalLive => "physical_live"
  }
}

pure source_mode_xsh(value: Str) -> Result[SourceMode] {
  if let Ok(state) = value.require(SourceMode) {
    Ok(state)
  } else {
    Err(SystemReportError.InvalidJson(f"unknown source mode '{value}'"))
  }
}

pure text_observation_json(value: TextObservation) -> JsonTextObservation {
  {...value, state: observation_state_json(value.state)}
}

pure text_observation_xsh(value: JsonTextObservation) -> Result[TextObservation] {
  {
    state: observation_state_xsh(value.state)?,
    value: value.value,
    raw_bytes_base64: value.raw_bytes_base64,
  }
}

pure section_status_json(value: SectionStatus) -> JsonSectionStatus {
  {...value, state: section_state_json(value.state)}
}

pure section_status_xsh(value: JsonSectionStatus) -> Result[SectionStatus] {
  let state = section_state_xsh(value.state)?
  if state == Complete and ! value.enumeration_succeeded {
    return Err(SystemReportError.InvalidJson("complete section has no successful enumeration"))
  }

  if (state == SectionNotRequested or state == SectionUnsupported) and value.enumeration_succeeded {
    return Err(SystemReportError.InvalidJson("unavailable section claims a successful enumeration"))
  }

  {
    state: state,
    enumeration_succeeded: value.enumeration_succeeded,
  }
}

pure collection_issue_json(value: CollectionIssue) -> JsonCollectionIssue {
  {
    ...value,
    state: observation_state_json(value.state),
    detail: text_observation_json(value.detail),
  }
}

pure collection_issue_xsh(value: JsonCollectionIssue) -> Result[CollectionIssue] {
  {
    section: value.section,
    field: value.field,
    state: observation_state_xsh(value.state)?,
    error_kind: value.error_kind,
    errno: value.errno,
    detail: text_observation_xsh(value.detail)?,
  }
}

pure observation_scope_json(value: ObservationScope) -> JsonObservationScope {
  {
    ...value,
    mount_namespace: text_observation_json(value.mount_namespace),
    network_namespace: text_observation_json(value.network_namespace),
    pid_namespace: text_observation_json(value.pid_namespace),
    cgroup_namespace: text_observation_json(value.cgroup_namespace),
    uts_namespace: text_observation_json(value.uts_namespace),
    ipc_namespace: text_observation_json(value.ipc_namespace),
    user_namespace: text_observation_json(value.user_namespace),
    time_namespace: text_observation_json(value.time_namespace),
    visible_cgroup: text_observation_json(value.visible_cgroup),
  }
}

pure observation_scope_xsh(value: JsonObservationScope) -> Result[ObservationScope] {
  {
    ...value,
    mount_namespace: text_observation_xsh(value.mount_namespace)?,
    network_namespace: text_observation_xsh(value.network_namespace)?,
    pid_namespace: text_observation_xsh(value.pid_namespace)?,
    cgroup_namespace: text_observation_xsh(value.cgroup_namespace)?,
    uts_namespace: text_observation_xsh(value.uts_namespace)?,
    ipc_namespace: text_observation_xsh(value.ipc_namespace)?,
    user_namespace: text_observation_xsh(value.user_namespace)?,
    time_namespace: text_observation_xsh(value.time_namespace)?,
    visible_cgroup: text_observation_xsh(value.visible_cgroup)?,
  }
}

pure firmware_identity_json(value: FirmwareIdentity) -> JsonFirmwareIdentity {
  var compatible = [text_observation_json(item) for item in value.device_tree_compatible]
  {
    ...value,
    serial: text_observation_json(value.serial),
    uuid: text_observation_json(value.uuid),
    device_tree_model: text_observation_json(value.device_tree_model),
    device_tree_compatible: compatible,
  }
}

pure firmware_identity_xsh(value: JsonFirmwareIdentity) -> Result[FirmwareIdentity] {
  var compatible = [text_observation_xsh(item)? for item in value.device_tree_compatible]
  {
    ...value,
    serial: text_observation_xsh(value.serial)?,
    uuid: text_observation_xsh(value.uuid)?,
    device_tree_model: text_observation_xsh(value.device_tree_model)?,
    device_tree_compatible: compatible,
  }
}

pure optional_firmware_identity_json(value: FirmwareIdentity?) -> JsonFirmwareIdentity? {
  guard value == null else {
    return firmware_identity_json(value)
  }

  null
}

pure optional_firmware_identity_xsh(value: JsonFirmwareIdentity?) -> Result[FirmwareIdentity?] {
  guard value == null else {
    return firmware_identity_xsh(value)
  }

  null
}

pure identity_section_json(value: IdentitySection) -> JsonIdentitySection {
  {
    ...value,
    status: section_status_json(value.status),
    hostname: text_observation_json(value.hostname),
    boot_id: text_observation_json(value.boot_id),
    firmware: optional_firmware_identity_json(value.firmware),
  }
}

pure identity_section_xsh(value: JsonIdentitySection) -> Result[IdentitySection] {
  {
    ...value,
    status: section_status_xsh(value.status)?,
    hostname: text_observation_xsh(value.hostname)?,
    boot_id: text_observation_xsh(value.boot_id)?,
    firmware: optional_firmware_identity_xsh(value.firmware)?,
  }
}

pure cpu_section_json(value: CpuSection) -> JsonCpuSection {
  var vulnerabilities = [
    {
      name: item.name,
      description: text_observation_json(item.description),
    }
    for item in value.vulnerabilities
  ]
  {
    ...value,
    status: section_status_json(value.status),
    vulnerabilities: vulnerabilities,
  }
}

pure cpu_section_xsh(value: JsonCpuSection) -> Result[CpuSection] {
  var vulnerabilities = [
    {
      name: item.name,
      description: text_observation_xsh(item.description)?,
    }
    for item in value.vulnerabilities
  ]
  {
    ...value,
    status: section_status_xsh(value.status)?,
    vulnerabilities: vulnerabilities,
  }
}

pure cgroup_resource_json(value: CgroupResource) -> JsonCgroupResource {
  {
    ...value,
    path: text_observation_json(value.path),
    state: observation_state_json(value.state),
  }
}

pure cgroup_resource_xsh(value: JsonCgroupResource) -> Result[CgroupResource] {
  {
    ...value,
    path: text_observation_xsh(value.path)?,
    state: observation_state_xsh(value.state)?,
  }
}

pure swap_device_json(value: SwapDevice) -> JsonSwapDevice {
  {...value, name: text_observation_json(value.name)}
}

pure swap_device_xsh(value: JsonSwapDevice) -> Result[SwapDevice] {
  {...value, name: text_observation_xsh(value.name)?}
}

pure memory_section_json(value: MemorySection) -> JsonMemorySection {
  var swaps = [swap_device_json(swap) for swap in value.swaps]
  var cgroup = [cgroup_resource_json(resource) for resource in value.cgroup]
  {
    ...value,
    status: section_status_json(value.status),
    swaps: swaps,
    cgroup: cgroup,
  }
}

pure memory_section_xsh(value: JsonMemorySection) -> Result[MemorySection] {
  var swaps = [swap_device_xsh(swap)? for swap in value.swaps]
  var cgroup = [cgroup_resource_xsh(resource)? for resource in value.cgroup]
  {
    ...value,
    status: section_status_xsh(value.status)?,
    swaps: swaps,
    cgroup: cgroup,
  }
}

pure pci_section_json(value: PciSection) -> JsonPciSection {
  {...value, status: section_status_json(value.status)}
}

pure pci_section_xsh(value: JsonPciSection) -> Result[PciSection] {
  {...value, status: section_status_xsh(value.status)?}
}

pure usb_device_json(value: UsbDevice) -> JsonUsbDevice {
  {
    ...value,
    manufacturer: text_observation_json(value.manufacturer),
    product: text_observation_json(value.product),
    serial: text_observation_json(value.serial),
  }
}

pure usb_device_xsh(value: JsonUsbDevice) -> Result[UsbDevice] {
  {
    ...value,
    manufacturer: text_observation_xsh(value.manufacturer)?,
    product: text_observation_xsh(value.product)?,
    serial: text_observation_xsh(value.serial)?,
  }
}

pure usb_section_json(value: UsbSection) -> JsonUsbSection {
  var devices = [usb_device_json(device) for device in value.devices]
  {
    status: section_status_json(value.status),
    devices: devices,
  }
}

pure usb_section_xsh(value: JsonUsbSection) -> Result[UsbSection] {
  var devices = [usb_device_xsh(device)? for device in value.devices]
  {
    status: section_status_xsh(value.status)?,
    devices: devices,
  }
}

pure mount_json(value: Mount) -> JsonMount {
  {
    ...value,
    root: text_observation_json(value.root),
    target: text_observation_json(value.target),
    source: text_observation_json(value.source),
    usage_state: observation_state_json(value.usage_state),
  }
}

pure mount_xsh(value: JsonMount) -> Result[Mount] {
  {
    ...value,
    root: text_observation_xsh(value.root)?,
    target: text_observation_xsh(value.target)?,
    source: text_observation_xsh(value.source)?,
    usage_state: observation_state_xsh(value.usage_state)?,
  }
}

pure block_device_json(value: BlockDevice) -> JsonBlockDevice {
  {
    ...value,
    model: text_observation_json(value.model),
    firmware: text_observation_json(value.firmware),
  }
}

pure block_device_xsh(value: JsonBlockDevice) -> Result[BlockDevice] {
  {
    ...value,
    model: text_observation_xsh(value.model)?,
    firmware: text_observation_xsh(value.firmware)?,
  }
}

pure storage_section_json(value: StorageSection) -> JsonStorageSection {
  var devices = [block_device_json(device) for device in value.devices]
  var mounts = [mount_json(mount) for mount in value.mounts]
  {
    status: section_status_json(value.status),
    devices: devices,
    mounts: mounts,
  }
}

pure storage_section_xsh(value: JsonStorageSection) -> Result[StorageSection] {
  var devices = [block_device_xsh(device)? for device in value.devices]
  var mounts = [mount_xsh(mount)? for mount in value.mounts]
  {
    status: section_status_xsh(value.status)?,
    devices: devices,
    mounts: mounts,
  }
}

pure network_attributes_json(values: List[NetworkAttribute]) -> List[JsonNetworkAttribute] {
  [{
    kind: attribute.kind,
    data: text_observation_json(attribute.data),
  } for attribute in values]
}

pure network_attributes_xsh(values: List[JsonNetworkAttribute]) -> Result[List[NetworkAttribute]] {
  var output = [
    {
      kind: attribute.kind,
      data: text_observation_xsh(attribute.data)?,
    }
    for attribute in values
  ]
  output
}

pure network_address_json(value: NetworkAddress) -> JsonNetworkAddress {
  {
    ...value,
    address: text_observation_json(value.address),
    broadcast: text_observation_json(value.broadcast),
    attributes: network_attributes_json(value.attributes),
  }
}

pure network_address_xsh(value: JsonNetworkAddress) -> Result[NetworkAddress] {
  {
    ...value,
    address: text_observation_xsh(value.address)?,
    broadcast: text_observation_xsh(value.broadcast)?,
    attributes: network_attributes_xsh(value.attributes)?,
  }
}

pure network_link_json(value: NetworkLink) -> JsonNetworkLink {
  var addresses = [network_address_json(address) for address in value.addresses]
  {
    ...value,
    name: text_observation_json(value.name),
    mac: text_observation_json(value.mac),
    addresses: addresses,
    attributes: network_attributes_json(value.attributes),
  }
}

pure network_link_xsh(value: JsonNetworkLink) -> Result[NetworkLink] {
  var addresses = [network_address_xsh(address)? for address in value.addresses]
  {
    ...value,
    name: text_observation_xsh(value.name)?,
    mac: text_observation_xsh(value.mac)?,
    addresses: addresses,
    attributes: network_attributes_xsh(value.attributes)?,
  }
}

pure network_nexthops_json(values: List[NetworkNexthop]) -> List[JsonNetworkNexthop] {
  [{
    ...nexthop,
    gateway: text_observation_json(nexthop.gateway),
  } for nexthop in values]
}

pure network_nexthops_xsh(values: List[JsonNetworkNexthop]) -> Result[List[NetworkNexthop]] {
  var output = [
    {
      ...nexthop,
      gateway: text_observation_xsh(nexthop.gateway)?,
    }
    for nexthop in values
  ]
  output
}

pure network_route_json(value: NetworkRoute) -> JsonNetworkRoute {
  {
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
  {
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
  {
    ...value,
    source: text_observation_json(value.source),
    destination: text_observation_json(value.destination),
    attributes: network_attributes_json(value.attributes),
  }
}

pure network_rule_xsh(value: JsonNetworkRule) -> Result[NetworkRule] {
  {
    ...value,
    source: text_observation_xsh(value.source)?,
    destination: text_observation_xsh(value.destination)?,
    attributes: network_attributes_xsh(value.attributes)?,
  }
}

pure network_section_json(value: NetworkSection) -> JsonNetworkSection {
  var links = [network_link_json(link) for link in value.links]
  var routes = [network_route_json(route) for route in value.routes]
  var rules = [network_rule_json(rule) for rule in value.rules]
  {
    status: section_status_json(value.status),
    links: links,
    routes: routes,
    rules: rules,
  }
}

pure network_section_xsh(value: JsonNetworkSection) -> Result[NetworkSection] {
  var links = [network_link_xsh(link)? for link in value.links]
  var routes = [network_route_xsh(route)? for route in value.routes]
  var rules = [network_rule_xsh(rule)? for rule in value.rules]
  {
    status: section_status_xsh(value.status)?,
    links: links,
    routes: routes,
    rules: rules,
  }
}

pure sensor_channel_json(value: SensorChannel) -> JsonSensorChannel {
  {...value, label: text_observation_json(value.label)}
}

pure sensor_channel_xsh(value: JsonSensorChannel) -> Result[SensorChannel] {
  {...value, label: text_observation_xsh(value.label)?}
}

pure sensor_section_json(value: SensorSection) -> JsonSensorSection {
  var channels = [sensor_channel_json(channel) for channel in value.channels]
  {
    status: section_status_json(value.status),
    channels: channels,
    thermal_zones: value.thermal_zones,
  }
}

pure sensor_section_xsh(value: JsonSensorSection) -> Result[SensorSection] {
  var channels = [sensor_channel_xsh(channel)? for channel in value.channels]
  {
    status: section_status_xsh(value.status)?,
    channels: channels,
    thermal_zones: value.thermal_zones,
  }
}

pure power_section_json(value: PowerSection) -> JsonPowerSection {
  {
    status: section_status_json(value.status),
    supplies: value.supplies,
    cap_zones: value.cap_zones,
  }
}

pure power_section_xsh(value: JsonPowerSection) -> Result[PowerSection] {
  {
    status: section_status_xsh(value.status)?,
    supplies: value.supplies,
    cap_zones: value.cap_zones,
  }
}

pure firmware_record_json(value: FirmwareRecord) -> JsonFirmwareRecord {
  var strings = [text_observation_json(item) for item in value.strings]
  {...value, strings: strings}
}

pure firmware_record_xsh(value: JsonFirmwareRecord) -> Result[FirmwareRecord] {
  var strings = [text_observation_xsh(item)? for item in value.strings]
  {...value, strings: strings}
}

pure firmware_section_json(value: FirmwareSection) -> JsonFirmwareSection {
  var records = [firmware_record_json(firmware_item) for firmware_item in value.records]
  {
    status: section_status_json(value.status),
    source: value.source,
    records: records,
    limitation: text_observation_json(value.limitation),
  }
}

pure firmware_section_xsh(value: JsonFirmwareSection) -> Result[FirmwareSection] {
  var records = [firmware_record_xsh(firmware_item)? for firmware_item in value.records]
  {
    status: section_status_xsh(value.status)?,
    source: value.source,
    records: records,
    limitation: text_observation_xsh(value.limitation)?,
  }
}

pure kernel_parameter_json(value: KernelParameter) -> JsonKernelParameter {
  {...value, value: text_observation_json(value.value)}
}

pure kernel_parameter_xsh(value: JsonKernelParameter) -> Result[KernelParameter] {
  {...value, value: text_observation_xsh(value.value)?}
}

pure kernel_section_json(value: KernelSection) -> JsonKernelSection {
  var parameters = [kernel_parameter_json(parameter) for parameter in value.parameters]
  var sysctls = [kernel_parameter_json(parameter) for parameter in value.sysctls]
  {
    status: section_status_json(value.status),
    command_line: text_observation_json(value.command_line),
    modules: value.modules,
    parameters: parameters,
    sysctls: sysctls,
  }
}

pure kernel_section_xsh(value: JsonKernelSection) -> Result[KernelSection] {
  var parameters = [kernel_parameter_xsh(parameter)? for parameter in value.parameters]
  var sysctls = [kernel_parameter_xsh(parameter)? for parameter in value.sysctls]
  {
    status: section_status_xsh(value.status)?,
    command_line: text_observation_xsh(value.command_line)?,
    modules: value.modules,
    parameters: parameters,
    sysctls: sysctls,
  }
}

pure process_record_json(value: ProcessRecord) -> JsonProcessRecord {
  {
    ...value,
    command: text_observation_json(value.command),
    cgroup: text_observation_json(value.cgroup),
  }
}

pure process_record_xsh(value: JsonProcessRecord) -> Result[ProcessRecord] {
  {
    ...value,
    command: text_observation_xsh(value.command)?,
    cgroup: text_observation_xsh(value.cgroup)?,
  }
}

pure process_section_json(value: ProcessSection) -> JsonProcessSection {
  var processes = [process_record_json(process_item) for process_item in value.processes]
  {
    status: section_status_json(value.status),
    processes: processes,
  }
}

pure process_section_xsh(value: JsonProcessSection) -> Result[ProcessSection] {
  var processes = [process_record_xsh(process_item)? for process_item in value.processes]
  {
    status: section_status_xsh(value.status)?,
    processes: processes,
  }
}

pure device_record_json(value: DeviceClassRecord) -> JsonDeviceClassRecord {
  var attributes = [kernel_parameter_json(attribute) for attribute in value.attributes]
  {
    ...value,
    entry_name: text_observation_json(value.entry_name),
    name: text_observation_json(value.name),
    attributes: attributes,
  }
}

pure device_record_xsh(value: JsonDeviceClassRecord) -> Result[DeviceClassRecord] {
  var attributes = [kernel_parameter_xsh(attribute)? for attribute in value.attributes]
  {
    ...value,
    entry_name: text_observation_xsh(value.entry_name)?,
    name: text_observation_xsh(value.name)?,
    attributes: attributes,
  }
}

pure device_section_json(value: DeviceSection) -> JsonDeviceSection {
  var devices = [device_record_json(device) for device in value.devices]
  {
    status: section_status_json(value.status),
    devices: devices,
  }
}

pure device_section_xsh(value: JsonDeviceSection) -> Result[DeviceSection] {
  var devices = [device_record_xsh(device)? for device in value.devices]
  {
    status: section_status_xsh(value.status)?,
    devices: devices,
  }
}

pure report_json(value: SystemReport) -> SystemReportJson {
  var issues = [collection_issue_json(issue) for issue in value.issues]
  {
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
  var issues = [collection_issue_xsh(issue)? for issue in value.issues]
  {
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
  guard report.schema_version == 1 else {
    return Err(
      SystemReportError.UnsupportedSchema(
        version: report.schema_version,
        message: f"unsupported schema version {report.schema_version}",
      ),
    )
  }

  var class_keys: Set[Str] = set.empty()
  for device in report.devices.devices {
    if device.parent_device_class_index != null and ((device.parent_device_class_index ?? -1) < 0 or (device.parent_device_class_index ?? -1) >= report.devices.devices.len()) {
      return Err(SystemReportError.InvalidJson("device-class parent index is outside the report"))
    }

    if device.parent_pci_function_index != null and ((device.parent_pci_function_index ?? -1) < 0 or (device.parent_pci_function_index ?? -1) >= report.pci.functions.len()) {
      return Err(SystemReportError.InvalidJson("device-class PCI parent index is outside the report"))
    }

    if device.parent_usb_device_index != null and ((device.parent_usb_device_index ?? -1) < 0 or (device.parent_usb_device_index ?? -1) >= report.usb.devices.len()) {
      return Err(SystemReportError.InvalidJson("device-class USB parent index is outside the report"))
    }

    if device.entry_name.state == Observed {
      let entry = device.entry_name.value ?? ""
      if entry == "" or entry == "." or entry == ".." or "/" in entry or "\0" in entry {
        return Err(SystemReportError.InvalidJson("device-class entry names must be nonempty path components"))
      }

      let key = f"{device.class.byte_len()}:{device.class}{entry}"
      if key in class_keys {
        return Err(
          SystemReportError.InvalidJson("device-class entries must have unique class and entry names"),
        )
      }

      class_keys = class_keys.add(key)
    }
  }

  var sensor_keys: Set[Str] = set.empty()
  for channel in report.sensors.channels {
    if channel.parent_pci_function_index != null and ((channel.parent_pci_function_index ?? -1) < 0 or (channel.parent_pci_function_index ?? -1) >= report.pci.functions.len()) {
      return Err(SystemReportError.InvalidJson("hwmon PCI parent index is outside the report"))
    }

    if channel.parent_usb_device_index != null and ((channel.parent_usb_device_index ?? -1) < 0 or (channel.parent_usb_device_index ?? -1) >= report.usb.devices.len()) {
      return Err(SystemReportError.InvalidJson("hwmon USB parent index is outside the report"))
    }

    if channel.chip_entry_name != null {
      let entry = channel.chip_entry_name
      if entry == "" or entry == "." or entry == ".." or "/" in entry or "\0" in entry {
        return Err(SystemReportError.InvalidJson("hwmon chip entry names must be nonempty path components"))
      }

      let key = f"{entry.byte_len()}:{entry}{channel.channel}"
      if key in sensor_keys {
        return Err(
          SystemReportError.InvalidJson("hwmon channels must have unique chip entries and channel names"),
        )
      }

      sensor_keys = sensor_keys.add(key)
    }
  }

  for zone in report.power.cap_zones {
    var previous_index = -1
    for constraint in zone.constraints {
      if constraint.index <= previous_index or constraint.index > 9007199254740991 {
        return Err(
          SystemReportError.InvalidJson("powercap constraint indexes must be unique, ascending, and JSON-safe"),
        )
      }

      previous_index = constraint.index
    }
  }

  for zone in report.sensors.thermal_zones {
    var previous_index = -1
    var has_index = false
    var missing_index = false
    for trip in zone.trips {
      if trip.index == null {
        missing_index = true
      } else {
        has_index = true
        let index = trip.index ?? -1
        if index <= previous_index or index > 9007199254740991 {
          return Err(
            SystemReportError.InvalidJson("thermal trip indexes must be unique, ascending, and JSON-safe"),
          )
        }

        previous_index = index
      }
    }

    if has_index and missing_index {
      return Err(
        SystemReportError.InvalidJson("thermal trip indexes cannot mix known and legacy-unknown values"),
      )
    }
  }

  var idle_keys: Set[Str] = set.empty()
  for state in report.cpu.idle_states {
    if state.state_index != null {
      let index = state.state_index ?? -1
      if index < 0 or index > 9007199254740991 {
        return Err(SystemReportError.InvalidJson("CPUIdle state indexes must be nonnegative and JSON-safe"))
      }

      if state.cpu_id != null {
        let key = f"{state.cpu_id ?? -1}:{index}"
        if key in idle_keys {
          return Err(SystemReportError.InvalidJson("CPUIdle states must have unique CPU and state indexes"))
        }

        idle_keys = idle_keys.add(key)
      }
    }
  }
}

pure encode_typed_report_json(report: SystemReport, sensitive: Bool, pretty: Bool) -> Result[Str] {
  require_report_v1(report)
  let clean = sanitize_report_mount_options(report)
  let output_report = if sensitive { clean } else { redact_report(clean) }
  let wire: Any = report_json(output_report)
  json.encode(wire, pretty:)
}

## Validates a dynamic report at the JSON boundary and emits one document.
export pure encode_report_json(report: Record, sensitive: Bool, pretty: Bool) -> Result[Str, Error] {
  encode_typed_report_json(report.require()?, sensitive, pretty)
}

## Validates the JSON v1 wire schema and restores its typed tag unions.
export pure decode_report_json(text: Str) -> Result[SystemReport, Error] {
  guard let raw = json.decode(text) else { |error|
    return Err(SystemReportError.InvalidJson(error.message))
  }

  var normalized = raw
  for field in ["uts_namespace", "ipc_namespace", "user_namespace", "time_namespace"] {
    if let Ok(value) = json.get(normalized, ["scope", field]) {
      guard value != null else {
        return Err(SystemReportError.InvalidJson(f"scope.{field} cannot be null"))
      }

      guard let _ = value.require(JsonTextObservation) else { |error|
        return Err(SystemReportError.InvalidJson(error.message))
      }
    } else {
      guard let updated = json.set(
        normalized,
        ["scope", field],
        {
          state: "unsupported",
          value: null,
          raw_bytes_base64: null,
        },
      ) else { |error|
        return Err(SystemReportError.InvalidJson(error.message))
      }

      normalized = updated
    }
  }

  if let Ok(raw_states) = json.get(normalized, ["cpu", "idle_states"]) {
    guard let states = raw_states.require(List[Any]) else { |error|
      return Err(SystemReportError.InvalidJson(error.message))
    }

    for state_index in range(states.len()) {
      match json.get(states[state_index], ["state_index"]) {
        Ok(_) => {}
        Err(_) => {
          guard let updated = json.set(normalized, ["cpu", "idle_states", state_index, "state_index"], null) else { |error|
            return Err(SystemReportError.InvalidJson(error.message))
          }

          normalized = updated
        }
      }
    }
  }

  if let Ok(raw_policies) = json.get(normalized, ["cpu", "frequency_policies"]) {
    guard let policies = raw_policies.require(List[Any]) else { |error|
      return Err(SystemReportError.InvalidJson(error.message))
    }

    for policy_index in range(policies.len()) {
      var legacy_current = null
      var has_legacy_current = false
      if let Ok(raw_legacy) = json.get(policies[policy_index], ["requested_current_khz"]) {
        has_legacy_current = true
        guard let value = raw_legacy.require(Int?) else { |error|
          return Err(SystemReportError.InvalidJson(error.message))
        }

        legacy_current = value
      }

      if let Ok(raw_current) = json.get(policies[policy_index], ["scaling_current_khz"]) {
        guard let value = raw_current.require(Int?) else { |error|
          return Err(SystemReportError.InvalidJson(error.message))
        }

        if has_legacy_current and value != legacy_current {
          return Err(
            SystemReportError.InvalidJson("CPUFreq current frequency has conflicting v1 field names"),
          )
        }
      } else {
        let replacement: Any = if legacy_current == null { null } else { legacy_current }
        guard let updated = json.set(
          normalized,
          ["cpu", "frequency_policies", policy_index, "scaling_current_khz"],
          replacement,
        ) else { |error|
          return Err(SystemReportError.InvalidJson(error.message))
        }

        normalized = updated
      }

      if has_legacy_current {
        guard let updated = json.remove(
          normalized,
          ["cpu", "frequency_policies", policy_index, "requested_current_khz"],
        ) else { |error|
          return Err(SystemReportError.InvalidJson(error.message))
        }

        normalized = updated
      }
    }
  }

  if let Ok(raw_zones) = json.get(normalized, ["sensors", "thermal_zones"]) {
    guard let zones = raw_zones.require(List[Any]) else { |error|
      return Err(SystemReportError.InvalidJson(error.message))
    }

    for zone_index in range(zones.len()) {
      guard let trips = json.get(zones[zone_index], ["trips"])?.require(List[Any]) else { |error|
        return Err(SystemReportError.InvalidJson(error.message))
      }

      for trip_index in range(trips.len()) {
        match json.get(trips[trip_index], ["index"]) {
          Ok(_) => {}
          Err(_) => {
            guard let updated = json.set(
              normalized,
              ["sensors", "thermal_zones", zone_index, "trips", trip_index, "index"],
              null,
            ) else { |error|
              return Err(SystemReportError.InvalidJson(error.message))
            }

            normalized = updated
          }
        }
      }
    }
  }

  if let Ok(raw_devices) = json.get(normalized, ["usb", "devices"]) {
    guard let devices = raw_devices.require(List[Any]) else { |error|
      return Err(SystemReportError.InvalidJson(error.message))
    }

    for device_index in range(devices.len()) {
      match json.get(devices[device_index], ["runtime_status"]) {
        Ok(_) => {}
        Err(_) => {
          guard let updated = json.set(normalized, ["usb", "devices", device_index, "runtime_status"], null) else { |error|
            return Err(SystemReportError.InvalidJson(error.message))
          }

          normalized = updated
        }
      }
    }
  }

  if let Ok(raw_channels) = json.get(normalized, ["sensors", "channels"]) {
    guard let channels = raw_channels.require(List[Any]) else { |error|
      return Err(SystemReportError.InvalidJson(error.message))
    }

    for channel_index in range(channels.len()) {
      for field in ["chip_entry_name", "parent_pci_function_index", "parent_usb_device_index"] {
        match json.get(channels[channel_index], [field]) {
          Ok(_) => {}
          Err(_) => {
            guard let updated = json.set(normalized, ["sensors", "channels", channel_index, field], null) else { |error|
              return Err(SystemReportError.InvalidJson(error.message))
            }

            normalized = updated
          }
        }
      }
    }
  }

  if let Ok(raw_devices) = json.get(normalized, ["devices", "devices"]) {
    guard let devices = raw_devices.require(List[Any]) else { |error|
      return Err(SystemReportError.InvalidJson(error.message))
    }

    for device_index in range(devices.len()) {
      match json.get(devices[device_index], ["entry_name"]) {
        Ok(_) => {}
        Err(_) => {
          guard let updated = json.set(
            normalized,
            ["devices", "devices", device_index, "entry_name"],
            {state: "unsupported", value: null, raw_bytes_base64: null},
          ) else { |error|
            return Err(SystemReportError.InvalidJson(error.message))
          }

          normalized = updated
        }
      }
    }
  }

  if let Ok(raw_zones) = json.get(normalized, ["power", "cap_zones"]) {
    guard let zones = raw_zones.require(List[Any]) else { |error|
      return Err(SystemReportError.InvalidJson(error.message))
    }

    var zone_index = 0
    for raw_zone in zones {
      if json.get(raw_zone, ["entry_name"], null) == null {
        guard let legacy = raw_zone.require(LegacyPowerCapZone) else { |error|
          return Err(SystemReportError.InvalidJson(error.message))
        }

        var constraints: List[PowerCapConstraint] = []
        if legacy.constraint_name != null or legacy.power_limit_uw != null or legacy.time_window_us != null {
          constraints = [
            {
              index: 0,
              name: legacy.constraint_name,
              power_limit_uw: legacy.power_limit_uw,
              time_window_us: legacy.time_window_us,
            },
          ]
        }

        let zone: PowerCapZone = PowerCapZone(
          entry_name: legacy.name,
          name: legacy.name,
          parent: legacy.parent,
          energy_uj: legacy.energy_uj,
          maximum_energy_range_uj: legacy.maximum_energy_range_uj,
          constraints:,
        )
        let wire_zone: Any = zone
        guard let updated = json.set(normalized, ["power", "cap_zones", zone_index], wire_zone) else { |error|
          return Err(SystemReportError.InvalidJson(error.message))
        }

        normalized = updated
      }

      zone_index += 1
    }
  }

  guard let wire = normalized.require(SystemReportJson) else { |error|
    return Err(SystemReportError.InvalidJson(error.message))
  }

  if wire.schema_version != 1 {
    return Err(
      SystemReportError.UnsupportedSchema(
        version: wire.schema_version,
        message: f"unsupported schema version {wire.schema_version}",
      ),
    )
  }

  let parsed = report_xsh(wire)? |> sanitize_report_mount_options(_)
  require_report_v1(parsed)
  Ok(parsed)
}

pure terminal_quote(value: Str) -> Result[Str] {
  var safe = value
  for escape in [
    {
      source: "\u{80}",
      replacement: "\\u{0080}",
    },
    {
      source: "\u{81}",
      replacement: "\\u{0081}",
    },
    {
      source: "\u{82}",
      replacement: "\\u{0082}",
    },
    {
      source: "\u{83}",
      replacement: "\\u{0083}",
    },
    {
      source: "\u{84}",
      replacement: "\\u{0084}",
    },
    {
      source: "\u{85}",
      replacement: "\\u{0085}",
    },
    {
      source: "\u{86}",
      replacement: "\\u{0086}",
    },
    {
      source: "\u{87}",
      replacement: "\\u{0087}",
    },
    {
      source: "\u{88}",
      replacement: "\\u{0088}",
    },
    {
      source: "\u{89}",
      replacement: "\\u{0089}",
    },
    {
      source: "\u{8a}",
      replacement: "\\u{008a}",
    },
    {
      source: "\u{8b}",
      replacement: "\\u{008b}",
    },
    {
      source: "\u{8c}",
      replacement: "\\u{008c}",
    },
    {
      source: "\u{8d}",
      replacement: "\\u{008d}",
    },
    {
      source: "\u{8e}",
      replacement: "\\u{008e}",
    },
    {
      source: "\u{8f}",
      replacement: "\\u{008f}",
    },
    {
      source: "\u{90}",
      replacement: "\\u{0090}",
    },
    {
      source: "\u{91}",
      replacement: "\\u{0091}",
    },
    {
      source: "\u{92}",
      replacement: "\\u{0092}",
    },
    {
      source: "\u{93}",
      replacement: "\\u{0093}",
    },
    {
      source: "\u{94}",
      replacement: "\\u{0094}",
    },
    {
      source: "\u{95}",
      replacement: "\\u{0095}",
    },
    {
      source: "\u{96}",
      replacement: "\\u{0096}",
    },
    {
      source: "\u{97}",
      replacement: "\\u{0097}",
    },
    {
      source: "\u{98}",
      replacement: "\\u{0098}",
    },
    {
      source: "\u{99}",
      replacement: "\\u{0099}",
    },
    {
      source: "\u{9a}",
      replacement: "\\u{009a}",
    },
    {
      source: "\u{9b}",
      replacement: "\\u{009b}",
    },
    {
      source: "\u{9c}",
      replacement: "\\u{009c}",
    },
    {
      source: "\u{9d}",
      replacement: "\\u{009d}",
    },
    {
      source: "\u{9e}",
      replacement: "\\u{009e}",
    },
    {
      source: "\u{9f}",
      replacement: "\\u{009f}",
    },
    {
      source: "­",
      replacement: "\\u{00ad}",
    },
    {
      source: "͏",
      replacement: "\\u{034f}",
    },
    {
      source: "؜",
      replacement: "\\u{061c}",
    },
    {
      source: "\u{200b}",
      replacement: "\\u{200b}",
    },
    {
      source: "\u{200c}",
      replacement: "\\u{200c}",
    },
    {
      source: "\u{200d}",
      replacement: "\\u{200d}",
    },
    {
      source: "\u{200e}",
      replacement: "\\u{200e}",
    },
    {
      source: "\u{200f}",
      replacement: "\\u{200f}",
    },
    {
      source: "\u{2028}",
      replacement: "\\u{2028}",
    },
    {
      source: "\u{2029}",
      replacement: "\\u{2029}",
    },
    {
      source: "\u{202a}",
      replacement: "\\u{202a}",
    },
    {
      source: "\u{202b}",
      replacement: "\\u{202b}",
    },
    {
      source: "\u{202c}",
      replacement: "\\u{202c}",
    },
    {
      source: "\u{202d}",
      replacement: "\\u{202d}",
    },
    {
      source: "\u{202e}",
      replacement: "\\u{202e}",
    },
    {
      source: "\u{2060}",
      replacement: "\\u{2060}",
    },
    {
      source: "\u{2066}",
      replacement: "\\u{2066}",
    },
    {
      source: "\u{2067}",
      replacement: "\\u{2067}",
    },
    {
      source: "\u{2068}",
      replacement: "\\u{2068}",
    },
    {
      source: "\u{2069}",
      replacement: "\\u{2069}",
    },
    {
      source: "⁪",
      replacement: "\\u{206a}",
    },
    {
      source: "⁫",
      replacement: "\\u{206b}",
    },
    {
      source: "⁬",
      replacement: "\\u{206c}",
    },
    {
      source: "⁭",
      replacement: "\\u{206d}",
    },
    {
      source: "⁮",
      replacement: "\\u{206e}",
    },
    {
      source: "⁯",
      replacement: "\\u{206f}",
    },
    {
      source: "\u{feff}",
      replacement: "\\u{feff}",
    },
  ] {
    safe = safe.replace(escape.source, with: escape.replacement)
  }

  json.encode(safe)
}

pure observation_display(value: TextObservation) -> Result[Str] {
  guard value.value == null else {
    let quoted = terminal_quote(value.value)?
    return quoted when value.state == Observed

    return f"[{observation_state_json(value.state)}] {quoted}"
  }

  if value.raw_bytes_base64 != null {
    return terminal_quote(f"base64:{value.raw_bytes_base64}")
  }

  f"[{observation_state_json(value.state)}]"
}

pure optional_text_display(value: Str?) -> Result[Str] {
  guard value != null else {
    return "unknown"
  }

  terminal_quote(value)
}

pure byte_quantity(value: Int?) -> Str {
  guard let amount = value else {
    return "unknown"
  }
  return "out of range" when amount < 0

  let divisor = 1073741824
  let whole = amount / divisor
  let fraction = amount % divisor * 10 / divisor
  f"{whole}.{fraction} GiB ({amount} bytes)"
}

pure optional_frequency(value: Int?) -> Str {
  guard value != null else {
    return "unknown"
  }

  f"{value} kHz"
}

pure optional_int_display(value: Int?) -> Str {
  guard value != null else {
    return "unknown"
  }

  f"{value}"
}

pure integer_list_display(values: List[Int]) -> Str {
  var texts = [f"{value}" for value in values]
  texts.join(",")
}

pure key_part(prefix: Str, value: Str) {
  f"{prefix}{value.count_chars()}:{value}"
}

pure optional_int_key(value: Int?) -> Str {
  guard value != null else {
    return "none;"
  }

  f"int:{value};"
}

pure optional_str_key(value: Str?) -> Str {
  guard value != null else {
    return "none;"
  }

  key_part("str:", value)
}

pure optional_bool_key(value: Bool?) -> Str {
  guard value != null else {
    return "none;"
  }

  if value {
    "bool:true;"
  } else {
    "bool:false;"
  }
}

pure cpu_policy_key(policy: CpuFreqPolicy) -> Str {
  var key = ""
  key = f"{key}{optional_str_key(policy.driver)}"
  key = f"{key}{optional_str_key(policy.governor)}"
  key = f"{key}{policy.available_governors.len()}:"
  for governor in policy.available_governors {
    key = key_part(key, governor)
  }

  key = f"{key}{optional_int_key(policy.hardware_min_khz)}"
  key = f"{key}{optional_int_key(policy.hardware_max_khz)}"
  key = f"{key}{optional_int_key(policy.scaling_min_khz)}"
  key = f"{key}{optional_int_key(policy.scaling_max_khz)}"
  key = f"{key}{optional_int_key(policy.hardware_current_khz)}"
  key = f"{key}{optional_int_key(policy.scaling_current_khz)}"
  key = f"{key}{optional_int_key(policy.governor_requested_khz)}"
  key = f"{key}{optional_int_key(policy.average_current_khz)}"
  key = f"{key}{optional_int_key(policy.bios_limit_khz)}"
  key = f"{key}{optional_int_key(policy.transition_latency_ns)}"
  key = f"{key}{policy.available_frequencies_khz.len()}:"
  for frequency in policy.available_frequencies_khz {
    key = f"{key}int:{frequency};"
  }

  key = f"{key}{optional_str_key(policy.energy_performance_preference)}"
  key = f"{key}{policy.available_energy_performance_preferences.len()}:"
  for preference in policy.available_energy_performance_preferences {
    key = key_part(key, preference)
  }

  key = f"{key}{optional_bool_key(policy.boost_supported)}"
  key = f"{key}{optional_bool_key(policy.boost_allowed)}"
  key = f"{key}{optional_bool_key(policy.boost_active)}"
  f"{key}{optional_str_key(policy.boost_scope)}"
}

pure grouped_policy_lines(policies: List[CpuFreqPolicy]) -> Result[List[Str]] {
  let groups = policies
    |> group-by cpu_policy_key(.)
    |> sort-by .key
  var lines = []

  for policy_group in groups {
    var cpu_ids = []
    for policy in policy_group.items {
      for cpu_id in policy.related_cpus {
        if cpu_id not in cpu_ids {
          cpu_ids += [cpu_id]
        }
      }
    }

    cpu_ids = cpu_ids |> sort-by .
    var cpu_id_texts = [f"{cpu_id}" for cpu_id in cpu_ids]
    let first = policy_group.items[0]
    let driver = optional_text_display(first.driver)?
    let governor = optional_text_display(first.governor)?
    lines += [
      f"  {policy_group.items.len()} identical policy group on CPUs {cpu_id_texts.join(",")}: {driver}, {governor}, {optional_frequency(first.scaling_min_khz)}..{optional_frequency(first.scaling_max_khz)}",
    ]
  }

  lines
}

pure section_entity_summary(status: SectionStatus, count: Int, label: Str) -> Str {
  return "not requested" when status.state == SectionNotRequested

  return "unsupported" when status.state == SectionUnsupported

  return "absent" when status.state == SectionAbsent

  return f"at least {count} {label}" when status.state == SectionTruncated

  if status.state == Partial or ! status.enumeration_succeeded {
    return f"{count} {label} (partial)"
  }

  f"{count} {label}"
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

  f"capacity observed on {observed} mounts; {skipped} skipped by policy; {unavailable} unavailable"
}

pure render_typed_text(report: SystemReport, full: Bool, sensitive: Bool) -> Result[Str] {
  require_report_v1(report)
  let output_report = if sensitive { report } else { redact_report(report) }
  var os_name = "unknown"
  if output_report.identity.os_release != null {
    let release = output_report.identity.os_release
    os_name = release.pretty_name ?? release.name ?? "unknown"
  }

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
  let memory_summary = if output_report.memory.status.state == SectionNotRequested {
    "not requested"
  } else {
    f"{byte_quantity(output_report.memory.host.total_bytes)} total; {byte_quantity(output_report.memory.host.available_bytes)} available; swap {byte_quantity(output_report.memory.host.swap_total_bytes)} total/{byte_quantity(output_report.memory.host.swap_free_bytes)} free"
  }
  let storage_usage = if output_report.storage.status.state == SectionNotRequested {
    "not requested"
  } else {
    mount_usage_summary(output_report.storage)
  }
  let power_summary = f"{section_entity_summary(output_report.power.status, output_report.power.supplies.len(), "supplies")}; {section_entity_summary(output_report.power.status, output_report.power.cap_zones.len(), "powercap zones")}"
  var lines = [
    "XSH system report v1",
    f"Scope: {host_claim} on {platform}; source mode {source_mode_json(output_report.source_mode)}",
    f"Privacy: {privacy}",
    f"System: {os}; kernel {kernel}; architecture {architecture}",
    f"Uptime: {optional_int_display(output_report.identity.uptime_seconds)} seconds",
    f"CPU: {section_entity_summary(output_report.cpu.status, output_report.cpu.cpus.len(), "CPUs")}; {section_entity_summary(output_report.cpu.status, output_report.cpu.frequency_policies.len(), "frequency policies")}",
    f"Memory: {memory_summary}",
    f"PCI: {section_entity_summary(output_report.pci.status, output_report.pci.functions.len(), "functions")}; USB: {section_entity_summary(output_report.usb.status, output_report.usb.devices.len(), "devices")}",
    f"Storage: {section_entity_summary(output_report.storage.status, output_report.storage.devices.len(), "block devices")}; {section_entity_summary(output_report.storage.status, output_report.storage.mounts.len(), "mounts")}; {storage_usage}",
    f"Network: {section_entity_summary(output_report.network.status, output_report.network.links.len(), "links")}; {section_entity_summary(output_report.network.status, output_report.network.routes.len(), "routes")}; {section_entity_summary(output_report.network.status, output_report.network.rules.len(), "rules")}",
    f"Sensors: {section_entity_summary(output_report.sensors.status, output_report.sensors.channels.len(), "channels")}; power: {power_summary}",
    f"Kernel: {section_entity_summary(output_report.kernel.status, output_report.kernel.modules.len(), "modules")}; visible processes: {section_entity_summary(output_report.processes.status, output_report.processes.processes.len(), "processes")}",
    f"Firmware: {section_entity_summary(output_report.firmware.status, output_report.firmware.records.len(), "records")}; device classes: {section_entity_summary(output_report.devices.status, output_report.devices.devices.len(), "devices")}",
    f"Issues: {output_report.issues.len()}",
    "Section status:",
    f"  identity={section_state_json(output_report.identity.status.state)} cpu={section_state_json(output_report.cpu.status.state)} memory={section_state_json(output_report.memory.status.state)}",
    f"  pci={section_state_json(output_report.pci.status.state)} usb={section_state_json(output_report.usb.status.state)} storage={section_state_json(output_report.storage.status.state)}",
    f"  network={section_state_json(output_report.network.status.state)} sensors={section_state_json(output_report.sensors.status.state)} power={section_state_json(output_report.power.status.state)}",
    f"  firmware={section_state_json(output_report.firmware.status.state)} kernel={section_state_json(output_report.kernel.status.state)} processes={section_state_json(output_report.processes.status.state)} devices={section_state_json(output_report.devices.status.state)}",
  ]

  if output_report.cpu.status.state != SectionNotRequested {
    let policies = grouped_policy_lines(output_report.cpu.frequency_policies)?
    if ! policies.is_empty() {
      lines += ["CPU frequency policy groups:"]
      lines += policies
    }
  }

  if full {
    if output_report.cpu.status.state != SectionNotRequested {
      lines += ["CPU caches:"]
      for cache in output_report.cpu.caches {
        lines += [
          f"  L{cache.level} {terminal_quote(cache.kind)?} cache on CPU {cache.owner_cpu_id} (sysfs index {cache.sysfs_index}): {byte_quantity(cache.size_bytes)}; shared CPUs {integer_list_display(cache.shared_cpus)}",
        ]
      }

      lines += ["CPU idle states:"]
      for idle in output_report.cpu.idle_states {
        lines += [
          f"  CPU {optional_int_display(idle.cpu_id)} {terminal_quote(idle.name)?}: latency {optional_int_display(idle.latency_us)} us, residency {optional_int_display(idle.residency_us)} us",
        ]
      }

      lines += ["CPU vulnerabilities:"]
      for vulnerability in output_report.cpu.vulnerabilities {
        lines += [f"  {terminal_quote(vulnerability.name)?}: {observation_display(vulnerability.description)?}"]
      }
    }

    if output_report.pci.status.state != SectionNotRequested {
      lines += ["PCI functions:"]
      for function in output_report.pci.functions {
        let address = optional_text_display(function.address)?
        let driver = optional_text_display(function.driver)?
        lines += [
          f"  {address} vendor={optional_int_display(function.vendor_id)} device={optional_int_display(function.device_id)} class={optional_int_display(function.class_code)} driver={driver}",
        ]
      }
    }

    if output_report.usb.status.state != SectionNotRequested {
      lines += ["USB devices:"]
      for device in output_report.usb.devices {
        let name = optional_text_display(device.sysfs_name)?
        let port = optional_text_display(device.port_path)?
        let serial = observation_display(device.serial)?
        lines += [
          f"  {name} port={port} id={optional_int_display(device.vendor_id)}:{optional_int_display(device.product_id)} serial={serial}",
        ]
        lines += [
          f"    runtime={optional_text_display(device.runtime_status)?} control={optional_text_display(device.power_control)?} autosuspend={optional_int_display(device.autosuspend_delay_ms)} ms configuration={optional_int_display(device.active_configuration)}/{optional_int_display(device.configuration_count)}",
        ]
        for interface in device.interfaces {
          lines += [
            f"    interface {interface.number} {optional_text_display(interface.name)?} driver={optional_text_display(interface.driver)?} alternate={optional_int_display(interface.active_alternate)}",
          ]
        }
      }
    }

    if output_report.storage.status.state != SectionNotRequested {
      lines += ["Block devices:"]
      for device in output_report.storage.devices {
        let name = optional_text_display(device.name)?
        let model = observation_display(device.model)?
        lines += [
          f"  {name} {optional_int_display(device.major)}:{optional_int_display(device.minor)} {byte_quantity(device.size_bytes)} model={model}",
        ]
        lines += [
          f"    kind={terminal_quote(device.kind)?} block-parent={optional_int_display(device.parent_device_index)} pci-parent={optional_int_display(device.parent_pci_function_index)} holders={integer_list_display(device.holder_indices)} slaves={integer_list_display(device.slave_indices)} scheduler={optional_text_display(device.active_scheduler)?}",
        ]
      }

      lines += ["Mounts:"]
      for mount in output_report.storage.mounts {
        let target = observation_display(mount.target)?
        let source = observation_display(mount.source)?
        lines += [
          f"  {target} type={terminal_quote(mount.filesystem)?} source={source} usage={byte_quantity(mount.usage_used_bytes)}/{byte_quantity(mount.usage_total_bytes)}",
        ]
      }
    }

    if output_report.network.status.state != SectionNotRequested {
      lines += ["Network links:"]
      for link in output_report.network.links {
        let name = observation_display(link.name)?
        let mac = observation_display(link.mac)?
        lines += [
          f"  ifindex={link.ifindex} name={name} mtu={optional_int_display(link.mtu)} state={optional_text_display(link.operational_state)?} mac={mac}",
        ]
        for address in link.addresses {
          lines += [
            f"    {terminal_quote(address.family)?} {observation_display(address.address)?}/{address.prefix_length}",
          ]
        }
      }

      lines += ["Network routes:"]
      for route in output_report.network.routes {
        lines += [
          f"  {terminal_quote(route.family)?} {observation_display(route.destination)?}/{route.prefix_length} via {observation_display(route.gateway)?} table={route.table} metric={optional_int_display(route.metric)} output-ifindex={optional_int_display(route.output_ifindex)}",
        ]
        for nexthop in route.nexthops {
          lines += [
            f"    nexthop ifindex={nexthop.ifindex} flags={nexthop.flags} hops={nexthop.hops} gateway={observation_display(nexthop.gateway)?}",
          ]
        }
      }

      lines += ["Network policy rules:"]
      for rule in output_report.network.rules {
        lines += [
          f"  {terminal_quote(rule.family)?} priority={optional_int_display(rule.priority)} from {observation_display(rule.source)?} to {observation_display(rule.destination)?} table={optional_int_display(rule.table)} action={terminal_quote(rule.action)?}",
        ]
      }
    }

    if output_report.sensors.status.state != SectionNotRequested {
      lines += ["Sensors:"]
      for channel in output_report.sensors.channels {
        lines += [
          f"  {terminal_quote(channel.chip)?}/{terminal_quote(channel.channel)?} {observation_display(channel.label)?}={optional_int_display(channel.value)} {terminal_quote(channel.unit)?}",
        ]
      }

      lines += ["Thermal zones:"]
      for zone in output_report.sensors.thermal_zones {
        lines += [
          f"  zone {zone.id} kind={optional_text_display(zone.kind)?} temperature={optional_int_display(zone.temperature_millidegrees)} millidegrees Celsius",
        ]
        for trip in zone.trips {
          lines += [
            f"    trip {optional_int_display(trip.index)} {terminal_quote(trip.kind)?} temperature={optional_int_display(trip.temperature_millidegrees)} hysteresis={optional_int_display(trip.hysteresis_millidegrees)} millidegrees Celsius",
          ]
        }
      }
    }

    if output_report.power.status.state != SectionNotRequested {
      lines += ["Power supplies:"]
      for supply in output_report.power.supplies {
        lines += [
          f"  {terminal_quote(supply.name)?} kind={optional_text_display(supply.kind)?} status={optional_text_display(supply.status)?} capacity={optional_int_display(supply.capacity_percent)}%",
        ]
      }

      lines += ["Power limits:"]
      for zone in output_report.power.cap_zones {
        lines += [f"  {terminal_quote(zone.name)?} energy={optional_int_display(zone.energy_uj)} uJ"]
        for constraint in zone.constraints {
          lines += [
            f"    constraint {constraint.index} {optional_text_display(constraint.name)?} limit={optional_int_display(constraint.power_limit_uw)} uW window={optional_int_display(constraint.time_window_us)} us",
          ]
        }
      }
    }

    if output_report.kernel.status.state != SectionNotRequested {
      lines += ["Kernel modules:"]
      for kernel_module in output_report.kernel.modules {
        lines += [
          f"  {terminal_quote(kernel_module.name)?} size={kernel_module.size_bytes} bytes users={optional_int_display(kernel_module.users)} state={terminal_quote(kernel_module.state)?}",
        ]
      }

      lines += [f"Kernel command line: {observation_display(output_report.kernel.command_line)?}"]
      lines += ["Kernel parameters:"]
      for parameter in output_report.kernel.parameters {
        lines += [f"  {terminal_quote(parameter.name)?}={observation_display(parameter.value)?}"]
      }

      lines += ["Selected sysctls:"]
      for parameter in output_report.kernel.sysctls {
        lines += [f"  {terminal_quote(parameter.name)?}={observation_display(parameter.value)?}"]
      }
    }

    if output_report.processes.status.state != SectionNotRequested {
      lines += ["Visible processes:"]
      for process_item in output_report.processes.processes {
        lines += [
          f"  pid={process_item.pid} ppid={process_item.parent_pid} state={terminal_quote(process_item.state)?} threads={optional_int_display(process_item.thread_count)} rss={byte_quantity(process_item.resident_bytes)} virtual={byte_quantity(process_item.virtual_bytes)} command={observation_display(process_item.command)?}",
        ]
      }
    }

    if output_report.firmware.status.state != SectionNotRequested {
      lines += ["Firmware records:"]
      for firmware_item in output_report.firmware.records {
        lines += [
          f"  type={firmware_item.record_type} handle={firmware_item.handle} formatted-length={firmware_item.formatted_length} string-count={firmware_item.strings.len()}",
        ]
      }
    }

    if output_report.devices.status.state != SectionNotRequested {
      lines += ["Device classes:"]
      for device in output_report.devices.devices {
        lines += [
          f"  {terminal_quote(device.class)?} name={observation_display(device.name)?} driver={optional_text_display(device.driver)?} pci-parent={optional_int_display(device.parent_pci_function_index)} usb-parent={optional_int_display(device.parent_usb_device_index)}",
        ]
      }
    }

    lines += ["Collection issues:"]
    for issue in output_report.issues {
      lines += [
        f"  {terminal_quote(issue.section)?}.{terminal_quote(issue.field)?}: {observation_state_json(issue.state)} {observation_display(issue.detail)?}",
      ]
    }
  }

  f"""{lines.join("\n")}
"""
}

## Validates a dynamic report and renders terminal-safe text.
export pure render_text(report: Record, full: Bool, sensitive: Bool) -> Result[Str, Error] {
  render_typed_text(report.require()?, full, sensitive)
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

  observation
}

pure redact_firmware_identity(identity: FirmwareIdentity) -> FirmwareIdentity {
  {
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

  {
    ...section,
    kernel_build: null,
    hostname: redact_text_observation(section.hostname),
    boot_id: redact_text_observation(section.boot_id),
    firmware: firmware,
  }
}

pure redact_memory_section(section: MemorySection) -> MemorySection {
  var swaps = [
    {
      ...swap,
      name: redact_text_observation(swap.name),
    }
    for swap in section.swaps
  ]
  var cgroup = [
    {
      ...resource,
      path: redact_text_observation(resource.path),
    }
    for resource in section.cgroup
  ]
  {...section, swaps: swaps, cgroup: cgroup}
}

pure redact_pci_section(section: PciSection) -> PciSection {
  var functions: List[PciFunction] = [
    {
      ...function,
      address: null,
      domain: null,
      bus: null,
      device: null,
      function: null,
      iommu_group: null,
    }
    for function in section.functions
  ]
  {...section, functions: functions}
}

pure redact_usb_section(section: UsbSection) -> UsbSection {
  var devices: List[UsbDevice] = []
  for device in section.devices {
    var interfaces: List[UsbInterface] = [{...interface, name: null} for interface in device.interfaces]
    devices += [
      {
        ...device,
        sysfs_name: null,
        port_path: null,
        bus_number: null,
        device_number: null,
        serial: redact_text_observation(device.serial),
        interfaces: interfaces,
      },
    ]
  }

  {...section, devices: devices}
}

pure allowed_mount_flag(option: Str) -> Bool {
  option in [
    "ro",
    "rw",
    "nosuid",
    "suid",
    "nodev",
    "dev",
    "noexec",
    "exec",
    "sync",
    "async",
    "dirsync",
    "relatime",
    "norelatime",
    "strictatime",
    "noatime",
    "nodiratime",
    "lazytime",
    "nolazytime",
    "mand",
    "nomand",
  ]
}

pure mount_decimal_text(value: Str, octal = false) -> Bool {
  return false when value == ""

  let digits = if octal { "01234567" } else { "0123456789" }
  for character in value {
    return false when character not in digits
  }

  true
}

pure allowed_mount_value(key: Str, value: Str) -> Bool {
  match key {
    "errors" => value in ["continue", "remount-ro", "panic"]
    "lowerdir" | "upperdir" | "workdir" => value.starts_with("/")
    "uid" | "gid" | "rsize" | "wsize" | "size" => mount_decimal_text(value)
    "mode" => mount_decimal_text(value, true)
    "vers" => {
      let components = value.split(".")
      return false when components.is_empty() or components.len() > 3

      for component in components {
        guard mount_decimal_text(component) else {
          return false
        }
      }

      true
    }
    else => false
  }
}

## Keeps only known configuration options; unknown values can carry credentials.
export pure sanitize_mount_options(options: List[Str]) -> List[Str] {
  let sanitized = collect {
    for option in options {
      let parts = option.split("=")
      if allowed_mount_flag(option) or (parts.len() == 2 and allowed_mount_value(parts[0], parts[1])) {
        yield option
      } else {
        yield "redacted"
      }
    }
  }

  sanitized
}

## Retains only numeric propagation identities from the kernel's optional mount fields.
export pure sanitize_mount_optional_fields(fields: List[Str]) -> List[Str] {
  let sanitized = collect {
    for field in fields {
      let parts = field.split(":")
      if field == "unbindable" or (parts.len() == 2 and parts[0] in ["shared", "master", "propagate_from"] and mount_decimal_text(
        parts[1],
      )) {
        yield field
      } else {
        yield "redacted"
      }
    }
  }

  sanitized
}

## Removes mount sources whose userinfo or option text may embed credentials.
export pure sanitize_mount_source(source: TextObservation) -> TextObservation {
  let value = source.value ?? ""
  let lower = value.lower()
  if "@" in value or "password=" in lower or "token=" in lower or "secret=" in lower {
    return redact_text_observation(source)
  }

  source
}

pure sanitize_report_mount_options(report: SystemReport) -> SystemReport {
  var mounts = [
    {
      ...mount,
      source: sanitize_mount_source(mount.source),
      mount_options: sanitize_mount_options(mount.mount_options),
      optional_fields: sanitize_mount_optional_fields(mount.optional_fields),
      super_options: sanitize_mount_options(mount.super_options),
    }
    for mount in report.storage.mounts
  ]
  {...report, storage.mounts: mounts}
}

pure redact_mount_options(options: List[Str]) -> List[Str] {
  var redacted = []
  for option in options {
    if allowed_mount_flag(option) {
      redacted += [option]
    } else {
      redacted += ["redacted"]
    }
  }

  redacted
}

pure redact_storage_section(section: StorageSection) -> StorageSection {
  var devices: List[BlockDevice] = [
    {
      ...device,
      name: null,
      major: null,
      minor: null,
      model: redact_text_observation(device.model),
      firmware: redact_text_observation(device.firmware),
    }
    for device in section.devices
  ]
  var mounts: List[Mount] = [
    {
      ...mount,
      major: null,
      minor: null,
      root: redact_text_observation(mount.root),
      target: redact_text_observation(mount.target),
      source: redact_text_observation(mount.source),
      mount_options: redact_mount_options(mount.mount_options),
      optional_fields: sanitize_mount_optional_fields(mount.optional_fields),
      super_options: redact_mount_options(mount.super_options),
    }
    for mount in section.mounts
  ]
  {...section, devices: devices, mounts: mounts}
}

pure redact_network_section(section: NetworkSection) -> NetworkSection {
  var links: List[NetworkLink] = []
  for link in section.links {
    var addresses = [
      {
        ...address,
        address: redact_text_observation(address.address),
        broadcast: redact_text_observation(address.broadcast),
        attributes: redact_network_attributes(address.attributes),
      }
      for address in link.addresses
    ]
    links += [
      {
        ...link,
        name: redact_text_observation(link.name),
        mac: redact_text_observation(link.mac),
        addresses: addresses,
        attributes: redact_network_attributes(link.attributes),
      },
    ]
  }

  var routes: List[NetworkRoute] = []
  for route in section.routes {
    var nexthops = [
      {
        ...nexthop,
        gateway: redact_text_observation(nexthop.gateway),
      }
      for nexthop in route.nexthops
    ]
    routes += [
      {
        ...route,
        destination: redact_text_observation(route.destination),
        source: redact_text_observation(route.source),
        preferred_source: redact_text_observation(route.preferred_source),
        gateway: redact_text_observation(route.gateway),
        nexthops: nexthops,
        attributes: redact_network_attributes(route.attributes),
      },
    ]
  }

  var rules = [
    {
      ...rule,
      source: redact_text_observation(rule.source),
      destination: redact_text_observation(rule.destination),
      attributes: redact_network_attributes(rule.attributes),
    }
    for rule in section.rules
  ]
  {
    ...section,
    links: links,
    routes: routes,
    rules: rules,
  }
}

pure redact_network_attributes(values: List[NetworkAttribute]) -> List[NetworkAttribute] {
  [{...attribute, data: redact_text_observation(attribute.data)} for attribute in values]
}

pure redact_sensor_section(section: SensorSection) -> SensorSection {
  var channels = [{...channel, label: redact_text_observation(channel.label)} for channel in section.channels]
  {...section, channels: channels}
}

pure redact_firmware_section(section: FirmwareSection) -> FirmwareSection {
  var records: List[FirmwareRecord] = []
  for firmware_item in section.records {
    var strings = [redact_text_observation(value) for value in firmware_item.strings]
    records += [{...firmware_item, strings: strings}]
  }

  {
    ...section,
    records: records,
    limitation: redact_text_observation(section.limitation),
  }
}

pure redact_kernel_section(section: KernelSection) -> KernelSection {
  var parameters = [
    {
      ...parameter,
      value: redact_text_observation(parameter.value),
    }
    for parameter in section.parameters
  ]
  var sysctls = [
    {
      ...parameter,
      value: redact_text_observation(parameter.value),
    }
    for parameter in section.sysctls
  ]
  {
    ...section,
    command_line: redact_text_observation(section.command_line),
    parameters: parameters,
    sysctls: sysctls,
  }
}

pure redact_process_section(section: ProcessSection) -> ProcessSection {
  var processes = [
    {
      ...process_item,
      cgroup: redact_text_observation(process_item.cgroup),
    }
    for process_item in section.processes
  ]
  {...section, processes: processes}
}

pure redact_device_section(section: DeviceSection) -> DeviceSection {
  var devices: List[DeviceClassRecord] = []
  for device in section.devices {
    var attributes = [
      {
        ...attribute,
        value: redact_text_observation(attribute.value),
      }
      for attribute in device.attributes
    ]
    devices += [
      {
        ...device,
        name: redact_text_observation(device.name),
        attributes: attributes,
      },
    ]
  }

  {...section, devices: devices}
}

pure redact_issue_field(section: Str, field: Str) -> Str {
  let parts = field.split(".")
  return field when parts.len() < 3

  let root = parts[0]
  if section == "pci" and root == "functions" {
    let leaf = parts[-1]
    return f"functions.redacted.{leaf}"
  }

  return "devices.redacted" when section == "usb" and root == "devices"

  let leaf = parts[-1]
  return f"devices.redacted.{leaf}" when section == "storage" and root == "devices"

  if section == "devices" and (root == "drm" or root == "sound" or root == "input") {
    return f"{root}.redacted.{leaf}"
  }

  field
}

## Applies share-safer redaction while keeping indexed parent and device relationships.
export pure redact_report(report: SystemReport) -> SystemReport {
  var scope = report.scope
  var source_roots = ["redacted" for _ in scope.source_roots]
  scope = {
    ...scope,
    source_roots: source_roots,
    mount_namespace: redact_text_observation(scope.mount_namespace),
    network_namespace: redact_text_observation(scope.network_namespace),
    pid_namespace: redact_text_observation(scope.pid_namespace),
    cgroup_namespace: redact_text_observation(scope.cgroup_namespace),
    uts_namespace: redact_text_observation(scope.uts_namespace),
    ipc_namespace: redact_text_observation(scope.ipc_namespace),
    user_namespace: redact_text_observation(scope.user_namespace),
    time_namespace: redact_text_observation(scope.time_namespace),
    visible_cgroup: redact_text_observation(scope.visible_cgroup),
  }

  var issues = [
    {
      ...issue,
      field: redact_issue_field(issue.section, issue.field),
      detail: redact_text_observation(issue.detail),
    }
    for issue in report.issues
  ]
  if report.identity.kernel_build != null {
    issues += [
      {
        section: "identity",
        field: "kernel_build",
        state: Redacted,
        error_kind: null,
        errno: null,
        detail: {
          state: Redacted,
          value: null,
          raw_bytes_base64: null,
        },
      },
    ]
  }

  {
    ...report,
    scope: scope,
    redacted: true,
    identity: redact_identity_section(report.identity),
    memory: redact_memory_section(report.memory),
    pci: redact_pci_section(report.pci),
    usb: redact_usb_section(report.usb),
    storage: redact_storage_section(report.storage),
    network: redact_network_section(report.network),
    sensors: redact_sensor_section(report.sensors),
    firmware: redact_firmware_section(report.firmware),
    kernel: redact_kernel_section(report.kernel),
    processes: redact_process_section(report.processes),
    devices: redact_device_section(report.devices),
    issues: issues,
  }
}
