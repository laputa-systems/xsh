## Collects a bounded, process-visible Linux report from one rooted source tree.
use lib.system_report as report
use lib.system_report_collect as collectors

pure empty_text(state: report.ObservationState) -> report.TextObservation {
  return {state: state, value: null, raw_bytes_base64: null}
}

pure empty_status(state: report.SectionState) -> report.SectionStatus {
  return {state: state, enumeration_succeeded: false}
}

pure empty_cpu(state: report.SectionState) -> report.CpuSection {
  return {
    status: empty_status(state),
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

pure empty_memory(state: report.SectionState) -> report.MemorySection {
  return {
    status: empty_status(state),
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

pure empty_report(started: Int, page_size_bytes: Int, clock_ticks_per_second: Int) -> report.SystemReport {
  return {
    schema_version: 1,
    producer: {name: "system-report", version: "1"},
    source_mode: report.SyntheticFixture,
    collection_started_unix_ms: started,
    collection_ended_unix_ms: null,
    elapsed_ms: null,
    scope: {
      platform: "Linux",
      host_claim: "process-visible Linux sources; physical-host completeness is unverified",
      source_roots: ["/proc", "/sys", "/etc"],
      mount_namespace: empty_text(report.NotRequested),
      network_namespace: empty_text(report.NotRequested),
      pid_namespace: empty_text(report.NotRequested),
      cgroup_namespace: empty_text(report.NotRequested),
      visible_cgroup: empty_text(report.NotRequested),
      page_size_bytes: page_size_bytes,
      clock_ticks_per_second: clock_ticks_per_second,
      ancestors_may_be_hidden: true,
    },
    redacted: false,
    identity: {
      status: empty_status(report.NotRequested),
      kernel_release: null,
      kernel_build: null,
      architecture: null,
      os_release: null,
      hostname: empty_text(report.NotRequested),
      uptime_seconds: null,
      boot_id: empty_text(report.NotRequested),
      firmware: null,
    },
    cpu: empty_cpu(report.NotRequested),
    memory: empty_memory(report.NotRequested),
    pci: {status: empty_status(report.NotRequested), functions: []},
    usb: {status: empty_status(report.NotRequested), devices: []},
    storage: {status: empty_status(report.NotRequested), devices: [], mounts: []},
    network: {status: empty_status(report.NotRequested), links: [], routes: [], rules: []},
    sensors: {status: empty_status(report.NotRequested), channels: [], thermal_zones: []},
    power: {status: empty_status(report.NotRequested), supplies: [], cap_zones: []},
    firmware: {
      status: empty_status(report.NotRequested),
      source: "not-requested",
      records: [],
      limitation: empty_text(report.NotRequested),
    },
    kernel: {
      status: empty_status(report.NotRequested),
      command_line: empty_text(report.NotRequested),
      modules: [],
      parameters: [],
      sysctls: [],
    },
    processes: {status: empty_status(report.NotRequested), processes: []},
    devices: {status: empty_status(report.NotRequested), devices: []},
    issues: [],
  }
}

pure issue(
  section: Str,
  field: Str,
  state: report.ObservationState,
  error_kind: Str?,
  errno: Int?,
) -> report.CollectionIssue {
  return {
    section: section,
    field: field,
    state: state,
    error_kind: error_kind,
    errno: errno,
    detail: empty_text(state),
  }
}

pure issue_with_detail(
  section: Str,
  field: Str,
  state: report.ObservationState,
  error_kind: Str?,
  errno: Int?,
  detail: Str,
) -> report.CollectionIssue {
  return {
    ...issue(section, field, state, error_kind, errno),
    detail: {state: report.Observed, value: detail, raw_bytes_base64: null},
  }
}

proc read_value(root: FsRoot, path: Path, max_bytes: Int = 65536) [fs, error] -> collectors.SourceRead {
  return collectors.read_source_text(root, path, max_bytes: max_bytes)?
}

pure parse_integer(value: Str?) -> Int? {
  if value == null {
    return null
  }
  return value.parse_int() ?? null
}

pure parse_bool01(value: Str?) -> Bool? {
  let parsed = parse_integer(value)
  if parsed == null {
    return null
  }
  if parsed == 0 { return false }
  if parsed == 1 { return true }
  return null
}

pure parse_words(value: Str?) -> List[Str] {
  if value == null or value == "" {
    return []
  }
  return value.split(" ") |> where .trim() != ""
}

pure parse_list(value: Str?) -> List[Int] {
  if value == null or value == "" {
    return []
  }
  return report.parse_cpu_list(value) ?? []
}

type CpuInfo = {
  id: Int,
  vendor: Str?,
  model: Str?,
  family: Str?,
  model_id: Str?,
  stepping: Str?,
  features: List[Str],
}

pure empty_cpu_info(id: Int) -> CpuInfo {
  return {id: id, vendor: null, model: null, family: null, model_id: null, stepping: null, features: []}
}

proc read_cpu_info(root: FsRoot) [fs, error] -> List[CpuInfo] {
  let source = read_value(root, p"proc/cpuinfo", max_bytes: 8388608)?
  if source.observation.value == null {
    return []
  }

  var values: List[CpuInfo] = []
  var current: CpuInfo? = null
  for line in source.observation.value.lines() {
    if line.trim() == "" {
      if current != null {
        values = values.push(current)
        current = null
      }
      continue
    }
    let pair = line.split(":", maxsplit: 1)
    if pair.len() != 2 {
      continue
    }
    let key = pair[0].trim()
    let value = pair[1].trim()
    if key == "processor" or key == "processor number" {
      if current != null {
        values = values.push(current)
      }
      current = empty_cpu_info(value.parse_int() ?? -1)
      continue
    }
    if current == null {
      continue
    }
    match key {
      "vendor_id" | "CPU implementer" => current = {...current, vendor: value}
      "model name" | "Processor" | "Hardware" => current = {...current, model: value}
      "cpu family" | "CPU architecture" => current = {...current, family: value}
      "model" | "CPU part" => current = {...current, model_id: value}
      "stepping" => current = {...current, stepping: value}
      "flags" | "Features" => current = {...current, features: parse_words(value)}
      _ => continue
    }
  }
  if current != null {
    values = values.push(current)
  }
  return values |> where .id >= 0 |> sort-by .id
}

pure cpu_info_for_id(infos: List[CpuInfo], id: Int) -> CpuInfo {
  for item in infos {
    if item.id == id {
      return item
    }
  }
  return empty_cpu_info(id)
}

type CpuSetRead = {
  cpus: List[Int],
  state: report.ObservationState,
  error_kind: Str?,
  errno: Int?,
}

proc read_effective_cgroup_cpuset(root: FsRoot) [fs, error] -> CpuSetRead {
  let membership = read_value(root, p"proc/self/cgroup", max_bytes: 65536)?
  let mounts = read_value(root, p"proc/self/mountinfo", max_bytes: 4194304)?
  var group_path: Str? = null
  var mount_root: Str? = null
  var mount_point: Str? = null
  var has_v1 = false

  if membership.observation.value != null {
    for line in membership.observation.value.lines() {
      let fields = line.split(":", maxsplit: 2)
      if fields.len() == 3 and fields[0] == "0" and fields[1] == "" {
        group_path = fields[2]
      } else if fields.len() >= 3 and fields[1] != "" {
        has_v1 = true
      }
    }
  }

  if mounts.observation.value != null {
    for line in mounts.observation.value.lines() {
      let fields = parse_words(line)
      var separator = 0
      while separator < fields.len() and fields[separator] != "-" {
        separator += 1
      }
      if separator + 1 >= fields.len() {
        continue
      }
      if fields[separator + 1] == "cgroup2" and mount_root == null and fields.len() > 4 {
        mount_root = decode_mount_field(fields[3])
        mount_point = decode_mount_field(fields[4])
      } else if fields[separator + 1] == "cgroup" {
        has_v1 = true
      }
    }
  }

  if mounts.observation.value == null {
    return {cpus: [], state: mounts.observation.state, error_kind: mounts.error_kind, errno: mounts.errno}
  }
  if membership.observation.value == null {
    return {cpus: [], state: membership.observation.state, error_kind: membership.error_kind, errno: membership.errno}
  }
  if group_path == null or mount_root == null or mount_point == null {
    return {
      cpus: [],
      state: if has_v1 {report.Unsupported} else {report.Absent},
      error_kind: if has_v1 {"cgroup_v1_or_hybrid"} else {"cgroup_v2_mount_unavailable"},
      errno: null,
    }
  }

  let group = group_path ?? ""
  let root_path = mount_root ?? ""
  let target = mount_point ?? ""
  if !group.starts_with("/") or !root_path.starts_with("/") or !target.starts_with("/") {
    return {cpus: [], state: report.Malformed, error_kind: "invalid_cgroup_mount_path", errno: null}
  }

  var relative = ""
  if root_path == "/" {
    relative = (group.split("") |> drop(1)).join("")
  } else if group == root_path {
    relative = ""
  } else if group.starts_with(f"${root_path}/") {
    relative = (group.split("") |> drop(root_path.count_chars() + 1)).join("")
  } else {
    return {cpus: [], state: report.Unsupported, error_kind: "cgroup_path_outside_visible_mount", errno: null}
  }

  let mount_relative = (target.split("/") |> where .trim() != "") |> join("/")
  let source_path = if relative == "" {
    if mount_relative == "" {p"."} else {fp"${mount_relative}"}
  } else if mount_relative == "" {
    fp"${relative}"
  } else {
    fp"${mount_relative}/${relative}"
  }
  let source = read_value(root, fp"${source_path}/cpuset.cpus.effective", max_bytes: 65536)?
  if source.observation.value == null {
    return {cpus: [], state: source.observation.state, error_kind: source.error_kind, errno: source.errno}
  }
  if source.observation.value == "" {
    return {cpus: [], state: report.Observed, error_kind: null, errno: null}
  }
  match report.parse_cpu_list(source.observation.value) {
    Ok(cpus) => return {cpus: cpus, state: report.Observed, error_kind: null, errno: null}
    Err(_) => return {cpus: [], state: report.Malformed, error_kind: "invalid_effective_cpuset", errno: null}
  }
}

type UsbDescriptorAlternate = {
  interface_number: Int,
  setting_number: Int,
  class_code: Int,
  subclass: Int,
  protocol: Int,
  endpoints: List[report.UsbEndpoint],
}

type UsbCollection = {
  status: report.SectionStatus,
  devices: List[report.UsbDevice],
  issues: List[report.CollectionIssue],
}

type BlockCandidate = {
  device: report.BlockDevice,
  parent_name: Str?,
  holders: List[Str],
  slaves: List[Str],
}

type StorageCollection = {
  status: report.SectionStatus,
  devices: List[report.BlockDevice],
  mounts: List[report.Mount],
  issues: List[report.CollectionIssue],
}

type SensorCollection = {
  status: report.SectionStatus,
  channels: List[report.SensorChannel],
  thermal_zones: List[report.ThermalZone],
  issues: List[report.CollectionIssue],
}

type PowerCollection = {
  status: report.SectionStatus,
  supplies: List[report.PowerSupply],
  cap_zones: List[report.PowerCapZone],
  issues: List[report.CollectionIssue],
}

export type NetworkCollection = {
  status: report.SectionStatus,
  links: List[report.NetworkLink],
  routes: List[report.NetworkRoute],
  rules: List[report.NetworkRule],
  issues: List[report.CollectionIssue],
}

export type ProcStat = {
  pid: Int,
  parent_pid: Int,
  command: Str,
  state: Str,
  thread_count: Int?,
  start_ticks: Int,
  virtual_bytes: Int?,
  resident_pages: Int?,
}

type ProcessCollection = {
  status: report.SectionStatus,
  processes: List[report.ProcessRecord],
  issues: List[report.CollectionIssue],
}

type KernelCollection = {
  status: report.SectionStatus,
  command_line: report.TextObservation,
  modules: List[report.KernelModule],
  parameters: List[report.KernelParameter],
  sysctls: List[report.KernelParameter],
  issues: List[report.CollectionIssue],
}

type DeviceCollection = {
  status: report.SectionStatus,
  devices: List[report.DeviceClassRecord],
  issues: List[report.CollectionIssue],
}

export type SmbiosParseResult = {
  records: List[report.FirmwareRecord],
  issues: List[Str],
  truncated: Bool,
}

type FirmwareCollection = {
  status: report.SectionStatus,
  source: Str,
  records: List[report.FirmwareRecord],
  limitation: report.TextObservation,
  issues: List[report.CollectionIssue],
}

type CgroupCollection = {
  resources: List[report.CgroupResource],
  issues: List[report.CollectionIssue],
}

pure decode_mount_field(value: Str) -> Str {
  return value
    .replace("\\040", " ")
    .replace("\\011", "\t")
    .replace("\\012", "\n")
    .replace("\\134", "\\")
}

pure mount_usage_eligible(filesystem: Str) -> Bool {
  return filesystem in [
    "btrfs", "exfat", "ext2", "ext3", "ext4", "f2fs", "ntfs", "ntfs3",
    "overlay", "tmpfs", "vfat", "xfs",
  ]
}

pure decimal_identifier(value: Str) -> Bool {
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

pure proc_stat_error(message: Str) -> Error {
  return Error(kind: "system-report-proc-stat", message: message)
}

export pure parse_proc_stat(text: Str) -> Result[ProcStat] {
  let pieces = text.trim().split(") ")
  if pieces.len() < 2 {
    return Err(proc_stat_error("process stat record has no command terminator"))
  }
  let prefix = pieces[0].split(" (", maxsplit: 1)
  if prefix.len() != 2 {
    return Err(proc_stat_error("process stat record has invalid PID and command fields"))
  }
  let pid = prefix[0].parse_int() ?? null
  if pid == null or pid <= 0 {
    return Err(proc_stat_error("process stat record has an invalid PID"))
  }

  var command_parts = [prefix[1]]
  var piece_index = 1
  while piece_index < pieces.len() - 1 {
    command_parts = command_parts.push(pieces[piece_index])
    piece_index += 1
  }
  let command = command_parts.join(") ")
  let fields = parse_words(pieces[pieces.len() - 1])
  if fields.len() < 22 {
    return Err(proc_stat_error("process stat record is missing resource fields"))
  }
  let parent_pid = fields[1].parse_int() ?? null
  let start_ticks = fields[19].parse_int() ?? null
  if parent_pid == null or start_ticks == null or parent_pid < 0 or start_ticks < 0 {
    return Err(proc_stat_error("process stat record has invalid parent or start identity"))
  }
  return Ok({
    pid: pid,
    parent_pid: parent_pid,
    command: command,
    state: fields[0],
    thread_count: fields[17].parse_int() ?? null,
    start_ticks: start_ticks,
    virtual_bytes: fields[20].parse_int() ?? null,
    resident_pages: fields[21].parse_int() ?? null,
  })
}

pure split_csv(value: Str) -> List[Str] {
  if value == "" {
    return []
  }
  return value.split(",")
}

pure block_index(devices: List[report.BlockDevice], major: Int, minor: Int) -> Int? {
  var index = 0
  for device in devices {
    if device.major != null and device.minor != null and device.major == major and device.minor == minor {
      return index
    }
    index += 1
  }
  return null
}

pure block_name_index(devices: List[report.BlockDevice], name: Str?) -> Int? {
  if name == null {
    return null
  }
  var index = 0
  for device in devices {
    if device.name == name {
      return index
    }
    index += 1
  }
  return null
}

pure pci_address_in_target(target: Path) -> Str? {
  var address: Str? = null
  for component in target.display().split("/") {
    match collectors.parse_pci_address(component) {
      Ok(_) => address = component
      Err(_) => continue
    }
  }
  return address
}

proc collect_storage(root: FsRoot, pci_functions: List[report.PciFunction], include_local_mount_usage: Bool) [fs, error] -> StorageCollection {
  let listing = fs.root_children(root, p"sys/class/block", max_entries: 4096)?
  var issues: List[report.CollectionIssue] = []
  var candidates: List[BlockCandidate] = []
  if listing.state != "complete" {
    issues = issues.push(issue("storage", "devices", usb_observation_state(listing.state, false), listing.error_kind, listing.errno))
  }

  for device_path in listing.children {
    let name = device_path.name()
    let dev = read_value(root, fp"${device_path}/dev", max_bytes: 4096)?
    let dev_parts = dev.observation.value?.split(":")
    if dev_parts == null or dev_parts.len() != 2 {
      issues = issues.push(issue("storage", f"devices.${name}.major_minor", report.Malformed, "invalid_device_number", dev.errno))
      continue
    }
    let major = dev_parts[0].parse_int() ?? null
    let minor = dev_parts[1].parse_int() ?? null
    if major == null or minor == null or major < 0 or minor < 0 {
      issues = issues.push(issue("storage", f"devices.${name}.major_minor", report.Malformed, "invalid_device_number", null))
      continue
    }
    let size = read_value(root, fp"${device_path}/size", max_bytes: 4096)?
    let logical = read_value(root, fp"${device_path}/queue/logical_block_size", max_bytes: 4096)?
    let physical = read_value(root, fp"${device_path}/queue/physical_block_size", max_bytes: 4096)?
    let removable = read_value(root, fp"${device_path}/removable", max_bytes: 4096)?
    let rotational = read_value(root, fp"${device_path}/queue/rotational", max_bytes: 4096)?
    let read_only = read_value(root, fp"${device_path}/ro", max_bytes: 4096)?
    let model = read_value(root, fp"${device_path}/device/model", max_bytes: 4096)?
    let firmware = read_value(root, fp"${device_path}/device/firmware_rev", max_bytes: 4096)?
    let fallback_firmware = if firmware.observation.value == null {
      read_value(root, fp"${device_path}/device/rev", max_bytes: 4096)?
    } else {
      firmware
    }
    let scheduler = read_value(root, fp"${device_path}/queue/scheduler", max_bytes: 4096)?
    var active_scheduler: Str? = null
    var available_schedulers: List[Str] = []
    for word in parse_words(scheduler.observation.value) {
      if word.starts_with("[") and word.ends_with("]") {
        active_scheduler = (word.split("") |> drop(1) |> take(word.count_chars() - 2)).join("")
        available_schedulers = available_schedulers.push(active_scheduler)
      } else {
        available_schedulers = available_schedulers.push(word)
      }
    }
    let read_ahead = read_value(root, fp"${device_path}/queue/read_ahead_kb", max_bytes: 4096)?
    let discard_granularity = read_value(root, fp"${device_path}/queue/discard_granularity", max_bytes: 4096)?
    let discard_max = read_value(root, fp"${device_path}/queue/discard_max_bytes", max_bytes: 4096)?
    let stats = read_value(root, fp"${device_path}/stat", max_bytes: 4096)?
    let stats_values = parse_words(stats.observation.value)
    var io_counters: List[report.MemoryCounter] = []
    let counter_names = ["read_ios", "read_merges", "read_sectors", "read_ms", "write_ios", "write_merges", "write_sectors", "write_ms", "in_flight", "io_ms", "weighted_io_ms"]
    let counter_units = ["requests", "requests", "sectors", "milliseconds", "requests", "requests", "sectors", "milliseconds", "requests", "milliseconds", "milliseconds"]
    var counter_index = 0
    while counter_index < stats_values.len() and counter_index < counter_names.len() {
      let value = stats_values[counter_index].parse_int() ?? null
      if value != null and value >= 0 {
        io_counters = io_counters.push({name: counter_names[counter_index], value: value, unit: counter_units[counter_index]})
      } else {
        issues = issues.push(issue("storage", f"devices.${name}.stat.${counter_names[counter_index]}", report.Malformed, "invalid_io_counter", null))
      }
      counter_index += 1
    }
    if stats_values.len() > counter_names.len() {
      issues = issues.push(issue("storage", f"devices.${name}.stat", report.Truncated, "extra_io_counter_fields", null))
    }
    let sectors = parse_integer(size.observation.value)
    if sectors != null and sectors > 18014398509481983 {
      issues = issues.push(issue("storage", f"devices.${name}.size", report.RangeFailure, "byte_count_overflow", null))
    }
    let is_partition = fs.root_exists(root, fp"${device_path}/partition")?
    let target_path = match fs.root_readlink(root, device_path) {
      Ok(value) => value
      Err(_) => null
    }
    if target_path == null {
      issues = issues.push(issue("storage", f"devices.${name}.sysfs_target", report.Disappeared, "sysfs_device_link_unavailable", null))
    }
    let target = if target_path == null {""} else {target_path.display()}
    let kind = if is_partition {"partition"} else if target_path == null {"unknown"} else if target.contains("/virtual/") {"virtual"} else {"disk"}
    var parent_pci_function_index: Int? = null
    if fs.root_exists(root, fp"${device_path}/device")? {
      match fs.root_readlink(root, fp"${device_path}/device") {
        Ok(controller_target) => parent_pci_function_index = pci_function_index(pci_functions, pci_address_in_target(controller_target))
        Err(_) => issues = issues.push(issue("storage", f"devices.${name}.controller", report.ReadFailure, "controller_readlink_failed", null))
      }
    }
    let holders_listing = fs.root_children(root, fp"${device_path}/holders", max_entries: 4096)?
    let slaves_listing = fs.root_children(root, fp"${device_path}/slaves", max_entries: 4096)?
    var holders: List[Str] = []
    var slaves: List[Str] = []
    for holder in holders_listing.children { holders = holders.push(holder.name()) }
    for slave in slaves_listing.children { slaves = slaves.push(slave.name()) }

    var parent_name: Str? = null
    let target_components = target.split("/")
    for component in target_components {
      if component != name and component != "block" {
        for candidate_path in listing.children {
          if candidate_path.name() == component {
            parent_name = component
          }
        }
      }
    }
    let sector_bytes = if sectors == null or sectors < 0 or sectors > 18014398509481983 {null} else {sectors * 512}
    candidates = candidates.push({
      device: {
        name: name,
        major: major,
        minor: minor,
        kind: kind,
        size_bytes: sector_bytes,
        logical_sector_bytes: parse_integer(logical.observation.value),
        physical_sector_bytes: parse_integer(physical.observation.value),
        removable: parse_bool01(removable.observation.value),
        rotational: parse_bool01(rotational.observation.value),
        read_only: parse_bool01(read_only.observation.value),
        model: model.observation,
        firmware: fallback_firmware.observation,
        parent_device_index: null,
        parent_pci_function_index: parent_pci_function_index,
        holder_indices: [],
        slave_indices: [],
        active_scheduler: active_scheduler,
        available_schedulers: available_schedulers,
        read_ahead_kb: parse_integer(read_ahead.observation.value),
        discard_granularity_bytes: parse_integer(discard_granularity.observation.value),
        discard_max_bytes: parse_integer(discard_max.observation.value),
        io_counters: io_counters,
      },
      parent_name: parent_name,
      holders: holders,
      slaves: slaves,
    })
  }

  var devices: List[report.BlockDevice] = []
  for candidate in candidates {
    devices = devices.push(candidate.device)
  }
  var linked_devices: List[report.BlockDevice] = []
  var candidate_index = 0
  while candidate_index < candidates.len() {
    let candidate = candidates[candidate_index]
    var holders: List[Int] = []
    var slaves: List[Int] = []
    for name in candidate.holders {
      let index = block_name_index(devices, name)
      if index != null { holders = holders.push(index) }
    }
    for name in candidate.slaves {
      let index = block_name_index(devices, name)
      if index != null { slaves = slaves.push(index) }
    }
    linked_devices = linked_devices.push({
      ...candidate.device,
      parent_device_index: block_name_index(devices, candidate.parent_name),
      holder_indices: holders,
      slave_indices: slaves,
    })
    candidate_index += 1
  }

  let mount_source = read_value(root, p"proc/self/mountinfo", max_bytes: 4194304)?
  var mounts: List[report.Mount] = []
  if mount_source.observation.value == null {
    issues = issues.push(issue("storage", "mounts", mount_source.observation.state, mount_source.error_kind, mount_source.errno))
  } else {
    for line_item in mount_source.observation.value.lines() |> enumerate() {
      let line_index = line_item.index
      let line = line_item.value
      let fields = parse_words(line)
      var separator = 0
      while separator < fields.len() and fields[separator] != "-" {
        separator += 1
      }
      if separator < 6 or separator + 3 >= fields.len() {
        issues = issues.push(issue("storage", f"mounts.line.${line_index}", report.Malformed, "invalid_mountinfo_row", null))
        continue
      }
      let ids = fields[0].parse_int() ?? null
      let parent_id = fields[1].parse_int() ?? null
      let device_ids = fields[2].split(":")
      let major = device_ids.get(0, "").parse_int() ?? null
      let minor = device_ids.get(1, "").parse_int() ?? null
      if ids == null or parent_id == null or major == null or minor == null {
        issues = issues.push(issue("storage", f"mounts.line.${line_index}", report.Malformed, "invalid_mount_identity", null))
        continue
      }
      var optional_fields: List[Str] = []
      var index = 6
      while index < separator {
        optional_fields = optional_fields.push(decode_mount_field(fields[index]))
        index += 1
      }
      let target = decode_mount_field(fields[4])
      let source = decode_mount_field(fields[separator + 2])
      var usage_state = report.NotRequested
      var usage_total_bytes: Int? = null
      var usage_used_bytes: Int? = null
      var usage_available_bytes: Int? = null
      if include_local_mount_usage and mount_usage_eligible(fields[separator + 1]) {
        if !target.starts_with("/") {
          usage_state = report.Malformed
          issues = issues.push(issue_with_detail(
            "storage", f"mounts.${ids}.usage", usage_state, "invalid_mount_target", null,
            "The mount target was not absolute.",
          ))
        } else {
          let usage_path = if target == "/" {
            p"."
          } else {
            match fp"${target}".strip_prefix(p"/") {
              Ok(path) => path
              Err(_) => null
            }
          }
          if usage_path == null {
            usage_state = report.Malformed
            issues = issues.push(issue_with_detail(
              "storage", f"mounts.${ids}.usage", usage_state, "invalid_mount_target", null,
              "The mount target could not be made relative to the observation root.",
            ))
          } else {
            let usage = fs.root_filesystem_stats(root, usage_path)?
            usage_state = match usage.state {
              "observed" => report.Observed
              "absent" => report.Disappeared
              "permission_denied" => report.PermissionDenied
              "malformed" => report.Malformed
              "range_failure" => report.RangeFailure
              _ => report.ReadFailure
            }
            usage_total_bytes = usage.total_bytes
            usage_used_bytes = usage.used_bytes
            usage_available_bytes = usage.available_bytes
            if usage.state != "observed" {
              issues = issues.push(issue(
                "storage", f"mounts.${ids}.usage", usage_state, usage.error_kind, usage.errno,
              ))
            }
          }
        }
      }
      mounts = mounts.push({
        mount_id: ids,
        parent_id: parent_id,
        major: major,
        minor: minor,
        root: {state: report.Observed, value: decode_mount_field(fields[3]), raw_bytes_base64: null},
        target: {state: report.Observed, value: target, raw_bytes_base64: null},
        mount_options: split_csv(fields[5]),
        optional_fields: optional_fields,
        filesystem: fields[separator + 1],
        source: {state: report.Observed, value: source, raw_bytes_base64: null},
        super_options: split_csv(fields[separator + 3]),
        block_device_index: block_index(linked_devices, major, minor),
        usage_state: usage_state,
        usage_total_bytes: usage_total_bytes,
        usage_used_bytes: usage_used_bytes,
        usage_available_bytes: usage_available_bytes,
      })
    }
  }

  var state = report.Complete
  if listing.state == "absent" and mount_source.observation.state == report.Absent {
    state = report.Absent
  } else if listing.state != "complete" or mount_source.observation.state != report.Observed or issues.len() > 0 {
    state = report.Partial
  }
  return {
    status: {state: state, enumeration_succeeded: listing.enumeration_succeeded and mount_source.observation.state == report.Observed},
    devices: linked_devices,
    mounts: mounts,
    issues: issues,
  }
}

pure sensor_kind(channel: Str) -> Str? {
  if channel.starts_with("temp") { return "temperature" }
  if channel.starts_with("in") { return "voltage" }
  if channel.starts_with("fan") { return "fan" }
  if channel.starts_with("power") { return "power" }
  if channel.starts_with("energy") { return "energy" }
  if channel.starts_with("curr") { return "current" }
  return null
}

pure sensor_unit(kind: Str) -> Str {
  match kind {
    "temperature" => return "millidegrees_celsius"
    "voltage" => return "millivolts"
    "fan" => return "rpm"
    "power" => return "microwatts"
    "energy" => return "microjoules"
    _ => return "milliamps"
  }
}

pure parse_boolean(value: Str?) -> Bool? {
  let parsed = parse_integer(value)
  if parsed == null { return null }
  return parsed != 0
}

proc collect_sensors(root: FsRoot) [fs, error] -> SensorCollection {
  let hwmon_listing = fs.root_children(root, p"sys/class/hwmon", max_entries: 1024)?
  let thermal_listing = fs.root_children(root, p"sys/class/thermal", max_entries: 1024)?
  var channels: List[report.SensorChannel] = []
  var zones: List[report.ThermalZone] = []
  var issues: List[report.CollectionIssue] = []
  if hwmon_listing.state != "complete" and hwmon_listing.state != "absent" {
    issues = issues.push(issue("sensors", "hwmon", usb_observation_state(hwmon_listing.state, false), hwmon_listing.error_kind, hwmon_listing.errno))
  }

  for chip_path in hwmon_listing.children {
    if !chip_path.name().starts_with("hwmon") {
      continue
    }
    let chip_name_source = read_value(root, fp"${chip_path}/name", max_bytes: 4096)?
    let chip = chip_name_source.observation.value ?? chip_path.name()
    let attribute_listing = fs.root_children(root, chip_path, max_entries: 1024)?
    for attribute in attribute_listing.children {
      let attribute_name = attribute.name()
      if !attribute_name.ends_with("_input") {
        continue
      }
      let channel_name = attribute_name.replace("_input", "")
      let kind = sensor_kind(channel_name)
      if kind == null {
        continue
      }
      let value = read_value(root, attribute, max_bytes: 4096)?
      let label = read_value(root, fp"${chip_path}/${channel_name}_label", max_bytes: 4096)?
      let minimum = read_value(root, fp"${chip_path}/${channel_name}_min", max_bytes: 4096)?
      let maximum = read_value(root, fp"${chip_path}/${channel_name}_max", max_bytes: 4096)?
      let critical = read_value(root, fp"${chip_path}/${channel_name}_crit", max_bytes: 4096)?
      let alarm = read_value(root, fp"${chip_path}/${channel_name}_alarm", max_bytes: 4096)?
      let parsed_value = parse_integer(value.observation.value)
      if value.observation.state == report.Observed and parsed_value == null {
        issues = issues.push(issue("sensors", f"${chip}.${attribute_name}", report.Malformed, "invalid_sensor_value", null))
      }
      channels = channels.push({
        chip: chip,
        channel: channel_name,
        label: label.observation,
        kind: kind,
        value: parsed_value,
        unit: sensor_unit(kind),
        minimum: parse_integer(minimum.observation.value),
        maximum: parse_integer(maximum.observation.value),
        critical: parse_integer(critical.observation.value),
        alarm: parse_boolean(alarm.observation.value),
        parent_device_class_index: null,
      })
    }
  }

  if thermal_listing.state != "complete" and thermal_listing.state != "absent" {
    issues = issues.push(issue("sensors", "thermal_zones", usb_observation_state(thermal_listing.state, false), thermal_listing.error_kind, thermal_listing.errno))
  }
  for zone_path in thermal_listing.children {
    if !zone_path.name().starts_with("thermal_zone") {
      continue
    }
    let id = (zone_path.name().split("") |> drop("thermal_zone".count_chars())).join("").parse_int() ?? null
    if id == null or id < 0 {
      issues = issues.push(issue("sensors", f"thermal_zones.${zone_path.name()}", report.Malformed, "invalid_thermal_zone_id", null))
      continue
    }
    let kind = read_value(root, fp"${zone_path}/type", max_bytes: 4096)?
    let temperature = read_value(root, fp"${zone_path}/temp", max_bytes: 4096)?
    let attributes = fs.root_children(root, zone_path, max_entries: 256)?
    var trips: List[report.ThermalTrip] = []
    for attribute in attributes.children {
      let attribute_name = attribute.name()
      if !attribute_name.starts_with("trip_point_") or !attribute_name.ends_with("_temp") {
        continue
      }
      let trip_number = attribute_name.split("_").get(2, "").parse_int() ?? null
      if trip_number == null {
        continue
      }
      let trip_temp = read_value(root, attribute, max_bytes: 4096)?
      let trip_type = read_value(root, fp"${zone_path}/trip_point_${trip_number}_type", max_bytes: 4096)?
      let hysteresis = read_value(root, fp"${zone_path}/trip_point_${trip_number}_hyst", max_bytes: 4096)?
      trips = trips.push({
        kind: trip_type.observation.value ?? "unknown",
        temperature_millidegrees: parse_integer(trip_temp.observation.value),
        hysteresis_millidegrees: parse_integer(hysteresis.observation.value),
      })
    }
    zones = zones.push({
      id: id,
      kind: kind.observation.value,
      temperature_millidegrees: parse_integer(temperature.observation.value),
      trips: trips,
      parent_device_class_index: null,
    })
  }

  var state = report.Complete
  if (hwmon_listing.state == "absent" and thermal_listing.state == "absent") {
    state = report.Absent
  } else if issues.len() > 0 or (hwmon_listing.state != "complete" and hwmon_listing.state != "absent") or (thermal_listing.state != "complete" and thermal_listing.state != "absent") {
    state = report.Partial
  }
  return {
    status: {state: state, enumeration_succeeded: (hwmon_listing.enumeration_succeeded or hwmon_listing.state == "absent") and (thermal_listing.enumeration_succeeded or thermal_listing.state == "absent")},
    channels: channels,
    thermal_zones: zones,
    issues: issues,
  }
}

proc collect_power(root: FsRoot) [fs, error] -> PowerCollection {
  let supply_listing = fs.root_children(root, p"sys/class/power_supply", max_entries: 1024)?
  let cap_listing = fs.root_children(root, p"sys/class/powercap", max_entries: 1024)?
  var supplies: List[report.PowerSupply] = []
  var cap_zones: List[report.PowerCapZone] = []
  var issues: List[report.CollectionIssue] = []
  if supply_listing.state != "complete" and supply_listing.state != "absent" {
    issues = issues.push(issue("power", "supplies", usb_observation_state(supply_listing.state, false), supply_listing.error_kind, supply_listing.errno))
  }
  for supply_path in supply_listing.children {
    let kind = read_value(root, fp"${supply_path}/type", max_bytes: 4096)?
    let status = read_value(root, fp"${supply_path}/status", max_bytes: 4096)?
    let health = read_value(root, fp"${supply_path}/health", max_bytes: 4096)?
    let capacity = read_value(root, fp"${supply_path}/capacity", max_bytes: 4096)?
    let energy_now = read_value(root, fp"${supply_path}/energy_now", max_bytes: 4096)?
    let energy_full = read_value(root, fp"${supply_path}/energy_full", max_bytes: 4096)?
    let charge_now = read_value(root, fp"${supply_path}/charge_now", max_bytes: 4096)?
    let charge_full = read_value(root, fp"${supply_path}/charge_full", max_bytes: 4096)?
    let voltage = read_value(root, fp"${supply_path}/voltage_now", max_bytes: 4096)?
    let current = read_value(root, fp"${supply_path}/current_now", max_bytes: 4096)?
    let cycles = read_value(root, fp"${supply_path}/cycle_count", max_bytes: 4096)?
    supplies = supplies.push({
      name: supply_path.name(),
      kind: kind.observation.value,
      status: status.observation.value,
      health: health.observation.value,
      capacity_percent: parse_integer(capacity.observation.value),
      energy_now_uwh: parse_integer(energy_now.observation.value),
      energy_full_uwh: parse_integer(energy_full.observation.value),
      charge_now_uah: parse_integer(charge_now.observation.value),
      charge_full_uah: parse_integer(charge_full.observation.value),
      voltage_now_uv: parse_integer(voltage.observation.value),
      current_now_ua: parse_integer(current.observation.value),
      cycle_count: parse_integer(cycles.observation.value),
      parent_device_class_index: null,
    })
  }

  var cap_paths: List[Path] = []
  for item in cap_listing.children {
    cap_paths = cap_paths.push(item)
    let children = fs.root_children(root, item, max_entries: 256)?
    for child in children.children {
      if child.name().starts_with("intel-rapl:") or child.name().starts_with("amd-rapl:") {
        cap_paths = cap_paths.push(child)
      }
    }
  }
  for zone_path in cap_paths {
    let name = read_value(root, fp"${zone_path}/name", max_bytes: 4096)?
    let energy = read_value(root, fp"${zone_path}/energy_uj", max_bytes: 4096)?
    let range = read_value(root, fp"${zone_path}/max_energy_range_uj", max_bytes: 4096)?
    let cap_attributes = fs.root_children(root, zone_path, max_entries: 256)?
    var power_limit: Int? = null
    var time_window: Int? = null
    var constraint_name: Str? = null
    for attribute in cap_attributes.children {
      if !attribute.name().starts_with("constraint_") or !attribute.name().ends_with("_power_limit_uw") {
        continue
      }
      let index = attribute.name().split("_").get(1, "")
      let limit = read_value(root, attribute, max_bytes: 4096)?
      power_limit = parse_integer(limit.observation.value)
      let name_source = read_value(root, fp"${zone_path}/constraint_${index}_name", max_bytes: 4096)?
      let window = read_value(root, fp"${zone_path}/constraint_${index}_time_window_us", max_bytes: 4096)?
      constraint_name = name_source.observation.value
      time_window = parse_integer(window.observation.value)
      break
    }
    let parent = zone_path.parent().name()
    cap_zones = cap_zones.push({
      name: name.observation.value ?? zone_path.name(),
      parent: if parent == zone_path.name() {null} else {parent},
      energy_uj: parse_integer(energy.observation.value),
      maximum_energy_range_uj: parse_integer(range.observation.value),
      constraint_name: constraint_name,
      power_limit_uw: power_limit,
      time_window_us: time_window,
    })
  }

  var state = report.Complete
  if supply_listing.state == "absent" and cap_listing.state == "absent" {
    state = report.Absent
  } else if issues.len() > 0 {
    state = report.Partial
  }
  return {
    status: {state: state, enumeration_succeeded: (supply_listing.enumeration_succeeded or supply_listing.state == "absent") and (cap_listing.enumeration_succeeded or cap_listing.state == "absent")},
    supplies: supplies,
    cap_zones: cap_zones,
    issues: issues,
  }
}

pure checked_page_bytes(pages: Int?, page_size_bytes: Int) -> Int? {
  if pages == null or pages < 0 or page_size_bytes <= 0 {
    return null
  }
  if pages > 9223372036854775807 / page_size_bytes {
    return null
  }
  return pages * page_size_bytes
}

pure parse_process_cgroup_path(value: Str) -> Str? {
  for line in value.lines() {
    let fields = line.split(":", maxsplit: 2)
    if fields.len() == 3 and fields[0] == "0" and fields[1] == "" {
      return fields[2]
    }
  }
  return null
}

pure process_cgroup_resource_index(
  path: report.TextObservation,
  resources: List[report.CgroupResource],
) -> Int? {
  if path.value == null {
    return null
  }
  var index = 0
  for resource in resources {
    if resource.path.value == path.value {
      return index
    }
    index += 1
  }
  return null
}

pure link_process_cgroups(
  processes: List[report.ProcessRecord],
  resources: List[report.CgroupResource],
) -> List[report.ProcessRecord] {
  var linked: List[report.ProcessRecord] = []
  for process in processes {
    linked = linked.push({
      ...process,
      cgroup_resource_index: process_cgroup_resource_index(process.cgroup, resources),
    })
  }
  return linked
}

proc collect_processes(root: FsRoot, page_size_bytes: Int) [fs, error] -> ProcessCollection {
  let listing = fs.root_children(root, p"proc", max_entries: 8192)?
  var processes: List[report.ProcessRecord] = []
  var issues: List[report.CollectionIssue] = []
  if listing.state != "complete" {
    issues = issues.push(issue("processes", "enumeration", usb_observation_state(listing.state, false), listing.error_kind, listing.errno))
  }

  for process_path in listing.children {
    let pid_text = process_path.name()
    if !decimal_identifier(pid_text) {
      continue
    }
    let pid = pid_text.parse_int() ?? null
    if pid == null or pid <= 0 {
      issues = issues.push(issue("processes", f"${pid_text}.pid", report.RangeFailure, "invalid_pid", null))
      continue
    }
    let stat_before = read_value(root, fp"${process_path}/stat", max_bytes: 16384)?
    if stat_before.observation.value == null {
      issues = issues.push(issue("processes", f"${pid}.stat", stat_before.observation.state, stat_before.error_kind, stat_before.errno))
      continue
    }
    let parsed_stat = parse_proc_stat(stat_before.observation.value)
    match parsed_stat {
      Err(_) => {
        issues = issues.push(issue("processes", f"${pid}.stat", report.Malformed, "invalid_process_stat", null))
        continue
      }
      Ok(stat) => {
        if stat.pid != pid {
          issues = issues.push(issue("processes", f"${pid}.stat", report.Raced, "pid_changed_during_read", null))
          continue
        }
        let statm = read_value(root, fp"${process_path}/statm", max_bytes: 4096)?
        let status = read_value(root, fp"${process_path}/status", max_bytes: 16384)?
        let cgroup = read_value(root, fp"${process_path}/cgroup", max_bytes: 16384)?
        var process_cgroup = cgroup.observation
        if cgroup.observation.value != null {
          let path = parse_process_cgroup_path(cgroup.observation.value)
          if path == null {
            process_cgroup = {state: report.Unsupported, value: null, raw_bytes_base64: null}
            issues = issues.push(issue("processes", f"${pid}.cgroup", report.Unsupported, "unified_cgroup_path_unavailable", null))
          } else {
            process_cgroup = {state: report.Observed, value: path, raw_bytes_base64: null}
          }
        }
        let stat_after = read_value(root, fp"${process_path}/stat", max_bytes: 16384)?
        if stat_after.observation.value == null {
          issues = issues.push(issue("processes", f"${pid}.stat", report.Disappeared, stat_after.error_kind, stat_after.errno))
          continue
        }
        let final_stat = parse_proc_stat(stat_after.observation.value)
        let same_start = match final_stat {
          Ok(after) => after.pid == pid and after.start_ticks == stat.start_ticks
          Err(_) => false
        }
        if !same_start {
          issues = issues.push(issue("processes", f"${pid}.stat", report.Raced, "pid_start_identity_changed", null))
          continue
        }

        let statm_fields = parse_words(statm.observation.value)
        let virtual_pages = statm_fields.get(0, "").parse_int() ?? null
        let resident_pages = statm_fields.get(1, "").parse_int() ?? stat.resident_pages
        let virtual_bytes = checked_page_bytes(virtual_pages, page_size_bytes)
        let resident_bytes = checked_page_bytes(resident_pages, page_size_bytes)
        if statm.observation.state == report.Observed and virtual_pages != null and virtual_bytes == null {
          issues = issues.push(issue("processes", f"${pid}.statm.virtual_bytes", report.RangeFailure, "page_count_overflow", null))
        }
        if statm.observation.state == report.Observed and resident_pages != null and resident_bytes == null {
          issues = issues.push(issue("processes", f"${pid}.statm.resident_bytes", report.RangeFailure, "page_count_overflow", null))
        }

        var uid: Int? = null
        if status.observation.value != null {
          for line in status.observation.value.lines() {
            if line.starts_with("Uid:") {
              uid = parse_words(line.split(":", maxsplit: 1).get(1, "")).get(0, "").parse_int() ?? null
              break
            }
          }
        }
        if status.observation.state == report.Observed and uid == null {
          issues = issues.push(issue("processes", f"${pid}.uid", report.Malformed, "numeric_uid_unavailable", null))
        }
        if statm.observation.state != report.Observed {
          issues = issues.push(issue("processes", f"${pid}.statm", statm.observation.state, statm.error_kind, statm.errno))
        }
        if status.observation.state != report.Observed {
          issues = issues.push(issue("processes", f"${pid}.status", status.observation.state, status.error_kind, status.errno))
        }
        if cgroup.observation.state != report.Observed {
          issues = issues.push(issue("processes", f"${pid}.cgroup", cgroup.observation.state, cgroup.error_kind, cgroup.errno))
        }
        processes = processes.push({
          pid: stat.pid,
          parent_pid: stat.parent_pid,
          uid: uid,
          command: {state: report.Observed, value: stat.command, raw_bytes_base64: null},
          state: stat.state,
          start_ticks: stat.start_ticks,
          thread_count: stat.thread_count,
          resident_bytes: resident_bytes,
          virtual_bytes: virtual_bytes ?? stat.virtual_bytes,
          cgroup: process_cgroup,
          cgroup_resource_index: null,
        })
      }
    }
  }

  var state = report.Complete
  if listing.state == "truncated" {
    state = report.Truncated
  } else if listing.state == "absent" {
    state = report.Absent
  } else if listing.state != "complete" or issues.len() > 0 {
    state = report.Partial
  }
  return {
    status: {state: state, enumeration_succeeded: listing.enumeration_succeeded},
    processes: processes |> sort-by .pid,
    issues: issues,
  }
}

proc collect_kernel(root: FsRoot) [fs, error] -> KernelCollection {
  let command_line = read_value(root, p"proc/cmdline", max_bytes: 65536)?
  let source = read_value(root, p"proc/modules", max_bytes: 1048576)?
  var issues: List[report.CollectionIssue] = []
  var modules: List[report.KernelModule] = []
  if command_line.observation.state != report.Observed {
    issues = issues.push(issue("kernel", "command_line", command_line.observation.state, command_line.error_kind, command_line.errno))
  }
  if source.observation.value == null {
    issues = issues.push(issue("kernel", "modules", source.observation.state, source.error_kind, source.errno))
  } else {
    for line_item in source.observation.value.lines() |> enumerate() {
      let columns = parse_words(line_item.value)
      if columns.len() < 6 {
        issues = issues.push(issue("kernel", f"modules.line.${line_item.index}", report.Malformed, "invalid_module_row", null))
        continue
      }
      let size = columns[1].parse_int() ?? null
      let users = columns[2].parse_int() ?? null
      if size == null or users == null or size < 0 or users < 0 {
        issues = issues.push(issue("kernel", f"modules.line.${line_item.index}", report.Malformed, "invalid_module_numeric_field", null))
        continue
      }
      modules = modules.push({name: columns[0], size_bytes: size, users: users, state: columns[4]})
    }
  }

  var sysctls: List[report.KernelParameter] = []
  for (name, path) in [
    ("kernel.pid_max", p"proc/sys/kernel/pid_max"),
    ("kernel.threads-max", p"proc/sys/kernel/threads-max"),
    ("vm.swappiness", p"proc/sys/vm/swappiness"),
    ("vm.overcommit_memory", p"proc/sys/vm/overcommit_memory"),
    ("net.ipv4.ip_forward", p"proc/sys/net/ipv4/ip_forward"),
    ("net.ipv6.conf.all.forwarding", p"proc/sys/net/ipv6/conf/all/forwarding"),
  ] {
    let value = read_value(root, path, max_bytes: 4096)?
    sysctls = sysctls.push({name: name, value: value.observation})
    if value.observation.state != report.Observed and value.observation.state != report.Absent {
      issues = issues.push(issue("kernel", f"sysctl.${name}", value.observation.state, value.error_kind, value.errno))
    }
  }

  var parameters: List[report.KernelParameter] = []
  for (name, path) in [
    ("usbcore.autosuspend", p"sys/module/usbcore/parameters/autosuspend"),
    ("nvme_core.default_ps_max_latency_us", p"sys/module/nvme_core/parameters/default_ps_max_latency_us"),
    ("intel_pstate.no_turbo", p"sys/module/intel_pstate/parameters/no_turbo"),
  ] {
    let value = read_value(root, path, max_bytes: 4096)?
    parameters = parameters.push({name: name, value: value.observation})
    if value.observation.state != report.Observed and value.observation.state != report.Absent {
      issues = issues.push(issue("kernel", f"parameters.${name}", value.observation.state, value.error_kind, value.errno))
    }
  }

  var state = if source.observation.state == report.Observed and command_line.observation.state == report.Observed {report.Complete} else {report.Partial}
  if issues.len() > 0 { state = report.Partial }
  return {
    status: {state: state, enumeration_succeeded: source.observation.state == report.Observed},
    command_line: command_line.observation,
    modules: modules |> sort-by .name,
    parameters: parameters,
    sysctls: sysctls,
    issues: issues,
  }
}

pure usb_device_index_from_target(devices: List[report.UsbDevice], target: Path) -> Int? {
  for component in target.display().split("/") {
    let candidate = component.split(":").get(0, "")
    let index = usb_device_index(devices, candidate)
    if index != null {
      return index
    }
  }
  return null
}

proc collect_device_classes(
  root: FsRoot,
  pci_functions: List[report.PciFunction],
  usb_devices: List[report.UsbDevice],
) [fs, error] -> DeviceCollection {
  var devices: List[report.DeviceClassRecord] = []
  var issues: List[report.CollectionIssue] = []
  var available_classes = 0
  var enumerated_classes = 0
  for (class_name, path) in [
    ("drm", p"sys/class/drm"),
    ("sound", p"sys/class/sound"),
    ("input", p"sys/class/input"),
  ] {
    let listing = fs.root_children(root, path, max_entries: 4096)?
    if listing.state != "absent" {
      available_classes += 1
    }
    if listing.enumeration_succeeded or listing.state == "absent" {
      enumerated_classes += 1
    } else {
      issues = issues.push(issue("devices", f"${class_name}.enumeration", usb_observation_state(listing.state, false), listing.error_kind, listing.errno))
    }
    for entry in listing.children {
      let entry_name = entry.name()
      let target = match fs.root_readlink(root, entry) {
        Ok(value) => value
        Err(_) => entry
      }
      var name = entry_name
      let name_file = match class_name {
        "input" => read_value(root, fp"${entry}/name", max_bytes: 4096)?,
        "sound" => read_value(root, fp"${entry}/id", max_bytes: 4096)?,
        _ => read_value(root, fp"${entry}/status", max_bytes: 4096)?,
      }
      if name_file.observation.value != null and class_name != "drm" {
        name = name_file.observation.value
      }
      let driver = usb_optional_link_name(root, fp"${entry}/device/driver")?
      let parent_target = match fs.root_readlink(root, fp"${entry}/device") {
        Ok(value) => value
        Err(_) => target
      }
      let parent_pci_address = usb_parent_address(parent_target)
      var attributes: List[report.KernelParameter] = []
      let allowlisted_attributes = match class_name {
        "drm" => ["status", "enabled", "modes"],
        "sound" => ["number"],
        "input" => [],
        _ => [],
      }
      for attribute_name in allowlisted_attributes {
        let attribute = read_value(root, fp"${entry}/${attribute_name}", max_bytes: 16384)?
        if attribute.observation.state == report.Observed {
          attributes = attributes.push({name: attribute_name, value: attribute.observation})
        }
      }
      devices = devices.push({
        class: class_name,
        name: {state: report.Observed, value: name, raw_bytes_base64: null},
        parent_device_class_index: null,
        parent_pci_function_index: pci_function_index(pci_functions, parent_pci_address),
        parent_usb_device_index: usb_device_index_from_target(usb_devices, parent_target),
        driver: driver,
        attributes: attributes,
      })
    }
  }
  var state = report.Complete
  if available_classes == 0 {
    state = report.Absent
  } else if enumerated_classes < 3 or issues.len() > 0 {
    state = report.Partial
  }
  return {
    status: {state: state, enumeration_succeeded: enumerated_classes == 3},
    devices: devices,
    issues: issues,
  }
}

proc collect_firmware(root: FsRoot) [fs, error] -> FirmwareCollection {
  let source = fs.root_read_result(root, p"sys/firmware/dmi/tables/DMI", max_bytes: 1048576)?
  var records: List[report.FirmwareRecord] = []
  var issues: List[report.CollectionIssue] = []
  if source.state == "observed" and source.data != null and !source.truncated {
    let parsed = parse_smbios_table(source.data)?
    records = parsed.records
    for line_item in parsed.issues |> enumerate() {
      issues = issues.push(issue_with_detail(
        "firmware",
        f"smbios.issue.${line_item.index}",
        if parsed.truncated {report.Truncated} else {report.Malformed},
        "smbios_record_validation",
        null,
        line_item.value,
      ))
    }
    let state = if parsed.issues.len() == 0 {report.Complete} else if parsed.truncated {report.Truncated} else {report.Partial}
    return {
      status: {state: state, enumeration_succeeded: !parsed.truncated},
      source: "smbios",
      records: records,
      limitation: {state: report.Observed, value: "The DMI table does not expose the SMBIOS entry-point version metadata.", raw_bytes_base64: null},
      issues: issues,
    }
  }

  let device_tree = read_value(root, p"sys/firmware/devicetree/base/model", max_bytes: 4096)?
  if source.truncated {
    issues = issues.push(issue("firmware", "smbios", report.Truncated, "dmi_table_limit", source.errno))
  } else if source.state != "absent" {
    issues = issues.push(issue("firmware", "smbios", usb_observation_state(source.state, false), source.error_kind, source.errno))
  }
  if device_tree.observation.state == report.Observed {
    return {
      status: {state: report.Partial, enumeration_succeeded: false},
      source: "device-tree",
      records: [],
      limitation: {state: report.Absent, value: "SMBIOS records are not exported; device-tree identity is retained in the identity section.", raw_bytes_base64: null},
      issues: issues,
    }
  }
  if issues.len() == 0 {
    issues = issues.push(issue("firmware", "smbios", report.Absent, "firmware_table_unavailable", null))
  }
  return {
    status: {state: report.Unsupported, enumeration_succeeded: false},
    source: "unavailable",
    records: [],
    limitation: {state: report.Unsupported, value: null, raw_bytes_base64: null},
    issues: issues,
  }
}

pure smbios_string_index(value: Int, name: Str) -> report.MemoryCounter {
  return {name: name, value: value, unit: "string_index"}
}

pure smbios_raw_field(value: Int, name: Str) -> report.MemoryCounter {
  return {name: name, value: value, unit: "smbios_raw"}
}

pure smbios_fields(record_type: Int, data: Bytes, offset: Int, length: Int) -> Result[List[report.MemoryCounter]] {
  var fields: List[report.MemoryCounter] = []
  if record_type == 0 {
    if length > 4 { fields = fields.push(smbios_string_index(data.byte_at(offset + 4), "vendor_index")) }
    if length > 5 { fields = fields.push(smbios_string_index(data.byte_at(offset + 5), "version_index")) }
    if length > 8 { fields = fields.push(smbios_string_index(data.byte_at(offset + 8), "release_date_index")) }
    if length > 9 { fields = fields.push(smbios_raw_field(data.byte_at(offset + 9), "rom_size_raw")) }
  } else if record_type == 1 {
    if length > 4 { fields = fields.push(smbios_string_index(data.byte_at(offset + 4), "manufacturer_index")) }
    if length > 5 { fields = fields.push(smbios_string_index(data.byte_at(offset + 5), "product_index")) }
    if length > 6 { fields = fields.push(smbios_string_index(data.byte_at(offset + 6), "version_index")) }
    if length > 7 { fields = fields.push(smbios_string_index(data.byte_at(offset + 7), "serial_index")) }
    if length > 24 { fields = fields.push(smbios_raw_field(data.byte_at(offset + 24), "wake_up_type_raw")) }
    if length > 25 { fields = fields.push(smbios_string_index(data.byte_at(offset + 25), "sku_index")) }
    if length > 26 { fields = fields.push(smbios_string_index(data.byte_at(offset + 26), "family_index")) }
  } else if record_type == 2 {
    if length > 4 { fields = fields.push(smbios_string_index(data.byte_at(offset + 4), "manufacturer_index")) }
    if length > 5 { fields = fields.push(smbios_string_index(data.byte_at(offset + 5), "product_index")) }
    if length > 6 { fields = fields.push(smbios_string_index(data.byte_at(offset + 6), "version_index")) }
    if length > 7 { fields = fields.push(smbios_string_index(data.byte_at(offset + 7), "serial_index")) }
    if length > 8 { fields = fields.push(smbios_string_index(data.byte_at(offset + 8), "asset_tag_index")) }
    if length > 13 { fields = fields.push(smbios_raw_field(data.byte_at(offset + 13), "board_type_raw")) }
  } else if record_type == 4 {
    if length > 4 { fields = fields.push(smbios_string_index(data.byte_at(offset + 4), "socket_designation_index")) }
    if length > 7 { fields = fields.push(smbios_string_index(data.byte_at(offset + 7), "manufacturer_index")) }
    if length > 16 { fields = fields.push(smbios_string_index(data.byte_at(offset + 16), "version_index")) }
    if length > 23 { fields = fields.push(smbios_raw_field(data.byte_at(offset + 23), "core_count_raw")) }
    if length > 24 { fields = fields.push(smbios_raw_field(data.byte_at(offset + 24), "core_enabled_raw")) }
    if length > 25 { fields = fields.push(smbios_raw_field(data.byte_at(offset + 25), "thread_count_raw")) }
  } else if record_type == 16 {
    if length >= 11 {
      fields = fields.push({name: "maximum_capacity_raw", value: bytes.unpack_le(data, 4, offset + 7)?, unit: "smbios_raw"})
    }
    if length >= 17 {
      fields = fields.push({name: "number_of_devices", value: bytes.unpack_le(data, 2, offset + 13)?, unit: "count"})
    }
  } else if record_type == 17 {
    if length >= 14 {
      fields = fields.push({name: "total_width_raw", value: bytes.unpack_le(data, 2, offset + 8)?, unit: "smbios_raw"})
      fields = fields.push({name: "data_width_raw", value: bytes.unpack_le(data, 2, offset + 10)?, unit: "smbios_raw"})
      fields = fields.push({name: "size_raw", value: bytes.unpack_le(data, 2, offset + 12)?, unit: "smbios_raw"})
    }
    if length > 16 { fields = fields.push(smbios_string_index(data.byte_at(offset + 16), "device_locator_index")) }
    if length > 17 { fields = fields.push(smbios_string_index(data.byte_at(offset + 17), "bank_locator_index")) }
    if length > 26 { fields = fields.push(smbios_string_index(data.byte_at(offset + 26), "part_number_index")) }
    if length >= 30 and bytes.unpack_le(data, 2, offset + 12)? == 32767 {
      fields = fields.push({name: "extended_size_raw", value: bytes.unpack_le(data, 4, offset + 28)?, unit: "smbios_raw"})
    }
  }
  return Ok(fields)
}

## Parses kernel-exported SMBIOS records without reading physical memory.
export pure parse_smbios_table(data: Bytes) -> Result[SmbiosParseResult] {
  var records: List[report.FirmwareRecord] = []
  var issues: List[Str] = []
  if data.len() > 1048576 {
    return {records: records, issues: ["SMBIOS table exceeds the 1 MiB bound"], truncated: true}
  }
  var offset = 0
  var saw_end_marker = false
  while offset < data.len() {
    if records.len() >= 4096 {
      issues = issues.push("SMBIOS table exceeds the 4,096 record bound")
      return {records: records, issues: issues, truncated: true}
    }
    if data.len() - offset < 4 {
      issues = issues.push("SMBIOS record header is truncated")
      return {records: records, issues: issues, truncated: true}
    }
    let record_type = data.byte_at(offset)
    let formatted_length = data.byte_at(offset + 1)
    if formatted_length < 4 {
      issues = issues.push(f"SMBIOS record type ${record_type} has a formatted length below four bytes")
      return {records: records, issues: issues, truncated: false}
    }
    if formatted_length > data.len() - offset {
      issues = issues.push(f"SMBIOS record type ${record_type} extends beyond the table")
      return {records: records, issues: issues, truncated: true}
    }
    let handle = bytes.unpack_le(data, 2, offset + 2)?
    let parsed_fields = smbios_fields(record_type, data, offset, formatted_length)?
    let fields = parsed_fields
    let strings_start = offset + formatted_length
    var position = strings_start
    var string_start = strings_start
    var strings: List[report.TextObservation] = []
    var found_terminator = false
    while position < data.len() {
      if data.byte_at(position) == 0 {
        if position + 1 < data.len() and data.byte_at(position + 1) == 0 {
          if position > string_start {
            let raw = data.slice(string_start, position - string_start)?
            match raw.utf8() {
              Ok(value) => strings = strings.push({state: report.Observed, value: value, raw_bytes_base64: null})
              Err(_) => strings = strings.push({state: report.Malformed, value: null, raw_bytes_base64: raw.base64()})
            }
          }
          position += 2
          found_terminator = true
          break
        }
        if position > string_start {
          let raw = data.slice(string_start, position - string_start)?
          match raw.utf8() {
            Ok(value) => strings = strings.push({state: report.Observed, value: value, raw_bytes_base64: null})
            Err(_) => strings = strings.push({state: report.Malformed, value: null, raw_bytes_base64: raw.base64()})
          }
        }
        position += 1
        string_start = position
      } else {
        position += 1
      }
    }
    if !found_terminator {
      issues = issues.push(f"SMBIOS record type ${record_type} has no complete string-set terminator")
      return {
        records: records,
        issues: issues,
        truncated: true,
      }
    }
    for field in fields {
      if field.unit == "string_index" and field.value > strings.len() {
        issues = issues.push(f"SMBIOS record type ${record_type} has an out-of-range ${field.name}")
      }
    }
    records = records.push({
      record_type: record_type,
      handle: handle,
      formatted_length: formatted_length,
      fields: fields,
      strings: strings,
    })
    offset = position
    if record_type == 127 {
      saw_end_marker = true
      break
    }
  }
  if !saw_end_marker {
    issues = issues.push("SMBIOS table has no end-of-table record")
  }
  return {records: records, issues: issues, truncated: false}
}

proc parse_usb_alternates(data: Bytes) [error] -> List[UsbDescriptorAlternate] {
  let records = collectors.parse_usb_descriptor_stream(data)?
  var alternates: List[UsbDescriptorAlternate] = []
  var current_interface: Int? = null
  var current_setting: Int? = null
  for record in records {
    if record.descriptor_type == 4 and record.length >= 9 {
      current_interface = bytes.unpack_le(record.raw, 1, 2)?
      current_setting = bytes.unpack_le(record.raw, 1, 3)?
      alternates = alternates.push({
        interface_number: current_interface,
        setting_number: current_setting,
        class_code: bytes.unpack_le(record.raw, 1, 5)?,
        subclass: bytes.unpack_le(record.raw, 1, 6)?,
        protocol: bytes.unpack_le(record.raw, 1, 7)?,
        endpoints: [],
      })
      continue
    }
    if record.descriptor_type != 5 or record.length < 7 or current_interface == null or current_setting == null {
      continue
    }
    let address = bytes.unpack_le(record.raw, 1, 2)?
    let attributes = bytes.unpack_le(record.raw, 1, 3)?
    let packet_size = bytes.unpack_le(record.raw, 2, 4)?
    let interval = bytes.unpack_le(record.raw, 1, 6)?
    let transfer_type = match attributes % 4 {
      0 => "control"
      1 => "isochronous"
      2 => "bulk"
      _ => "interrupt"
    }
    let endpoint: report.UsbEndpoint = {
      address: address,
      direction: if address >= 128 {"in"} else {"out"},
      transfer_type: transfer_type,
      max_packet_size: packet_size,
      interval: interval,
    }
    var updated: List[UsbDescriptorAlternate] = []
    for alternate in alternates {
      if alternate.interface_number == current_interface and alternate.setting_number == current_setting {
        updated = updated.push({...alternate, endpoints: alternate.endpoints.push(endpoint)})
      } else {
        updated = updated.push(alternate)
      }
    }
    alternates = updated
  }
  return alternates
}

pure usb_parent_name(name: Str, bus_number: Int?) -> Str? {
  if name.starts_with("usb") or bus_number == null {
    return null
  }
  let prefix = f"${bus_number}-"
  if !name.starts_with(prefix) {
    return null
  }
  let components = name.split(".")
  if components.len() <= 1 {
    return f"usb${bus_number}"
  }
  return components |> take(components.len() - 1) |> join(".")
}

pure usb_port_path(name: Str) -> Str? {
  if name.starts_with("usb") {
    return null
  }
  let parts = name.split("-")
  if parts.len() < 2 {
    return null
  }
  return parts[1]
}

pure usb_device_index(devices: List[report.UsbDevice], name: Str) -> Int? {
  var index = 0
  for device in devices {
    if device.sysfs_name != null and device.sysfs_name == name {
      return index
    }
    index += 1
  }
  return null
}

pure pci_function_index(functions: List[report.PciFunction], address: Str?) -> Int? {
  if address == null {
    return null
  }
  var index = 0
  for function in functions {
    if function.address != null and function.address == address {
      return index
    }
    index += 1
  }
  return null
}

pure usb_observation_state(state: Str, truncated: Bool) -> report.ObservationState {
  if truncated { return report.Truncated }
  match state {
    "observed" => return report.Observed
    "absent" => return report.Absent
    "permission_denied" => return report.PermissionDenied
    _ => return report.ReadFailure
  }
}

pure usb_parent_address(target: Path) -> Str? {
  var result: Str? = null
  for component in target.display().split("/") {
    match collectors.parse_pci_address(component) {
      Ok(_) => result = component
      Err(_) => continue
    }
  }
  return result
}

proc usb_optional_link_name(root: FsRoot, path: Path) [fs, error] -> Str? {
  match fs.root_readlink(root, path) {
    Ok(target) => return target.name()
    Err(_) => return null
  }
}

pure parse_hex_optional(value: Str?) -> Int? {
  if value == null {
    return null
  }
  return collectors.parse_pci_hex_value(value) ?? null
}

proc collect_usb(root: FsRoot, pci_functions: List[report.PciFunction]) [fs, error] -> UsbCollection {
  let listing = fs.root_children(root, p"sys/bus/usb/devices", max_entries: 4096)?
  var devices: List[report.UsbDevice] = []
  var issues: List[report.CollectionIssue] = []
  if listing.state != "complete" {
    issues = issues.push(issue("usb", "devices", collectors.source_observation_state(listing.state, false), listing.error_kind, listing.errno))
  }

  for device_path in listing.children {
    if device_path.name().contains(":") {
      continue
    }
    let vendor = read_value(root, fp"${device_path}/idVendor", max_bytes: 4096)?
    let product = read_value(root, fp"${device_path}/idProduct", max_bytes: 4096)?
    if vendor.observation.value == null or product.observation.value == null {
      continue
    }
    let bus = read_value(root, fp"${device_path}/busnum", max_bytes: 4096)?
    let number = read_value(root, fp"${device_path}/devnum", max_bytes: 4096)?
    let version = read_value(root, fp"${device_path}/bcdDevice", max_bytes: 4096)?
    let class = read_value(root, fp"${device_path}/bDeviceClass", max_bytes: 4096)?
    let subclass = read_value(root, fp"${device_path}/bDeviceSubClass", max_bytes: 4096)?
    let protocol = read_value(root, fp"${device_path}/bDeviceProtocol", max_bytes: 4096)?
    let manufacturer = read_value(root, fp"${device_path}/manufacturer", max_bytes: 4096)?
    let product_text = read_value(root, fp"${device_path}/product", max_bytes: 4096)?
    let serial = read_value(root, fp"${device_path}/serial", max_bytes: 4096)?
    let speed = read_value(root, fp"${device_path}/speed", max_bytes: 4096)?
    let configurations = read_value(root, fp"${device_path}/bNumConfigurations", max_bytes: 4096)?
    let active_configuration = read_value(root, fp"${device_path}/bConfigurationValue", max_bytes: 4096)?
    let power_control = read_value(root, fp"${device_path}/power/control", max_bytes: 4096)?
    let autosuspend = read_value(root, fp"${device_path}/power/autosuspend_delay_ms", max_bytes: 4096)?
    let raw_descriptors = fs.root_read_result(root, fp"${device_path}/descriptors", max_bytes: 1048576)?
    var descriptor_alternates: List[UsbDescriptorAlternate] = []
    if raw_descriptors.truncated {
      issues = issues.push(issue("usb", f"devices.${device_path.name()}.descriptors", report.Truncated, "descriptor_input_limit", raw_descriptors.errno))
    } else if raw_descriptors.state == "observed" and raw_descriptors.data != null {
      match parse_usb_alternates(raw_descriptors.data) {
        Ok(alternates) => descriptor_alternates = alternates
        Err(_) => issues = issues.push(issue("usb", f"devices.${device_path.name()}.descriptors", report.Malformed, "invalid_usb_descriptor_stream", null))
      }
    } else if raw_descriptors.state == "read_failure" or raw_descriptors.state == "permission_denied" {
      issues = issues.push(issue("usb", f"devices.${device_path.name()}.descriptors", usb_observation_state(raw_descriptors.state, raw_descriptors.truncated), raw_descriptors.error_kind, raw_descriptors.errno))
    }

    var interfaces: List[report.UsbInterface] = []
    for interface_path in listing.children {
      let interface_name = interface_path.name()
      if !interface_name.starts_with(f"${device_path.name()}:") {
        continue
      }
      let interface_number_text = interface_name.split(":").get(1, "").split(".").get(1, "")
      let interface_number = interface_number_text.parse_int() ?? null
      if interface_number == null {
        issues = issues.push(issue("usb", f"devices.${device_path.name()}.interfaces.${interface_name}", report.Malformed, "invalid_interface_name", null))
        continue
      }
      let driver = usb_optional_link_name(root, fp"${interface_path}/driver")?
      let active = read_value(root, fp"${interface_path}/bAlternateSetting", max_bytes: 4096)?
      var alternate_settings: List[report.UsbAlternateSetting] = []
      for alternate in descriptor_alternates {
        if alternate.interface_number == interface_number {
          alternate_settings = alternate_settings.push({
            number: alternate.setting_number,
            class_code: alternate.class_code,
            subclass: alternate.subclass,
            protocol: alternate.protocol,
            endpoints: alternate.endpoints,
          })
        }
      }
      interfaces = interfaces.push({
        number: interface_number,
        name: interface_name,
        driver: driver,
        active_alternate: parse_integer(active.observation.value),
        alternate_settings: alternate_settings,
      })
    }

    let bus_number = parse_integer(bus.observation.value)
    let parent_name = usb_parent_name(device_path.name(), bus_number)
    let parent_index = if parent_name == null {null} else {usb_device_index(devices, parent_name)}
    let controller_target = if fs.root_exists(root, fp"${device_path}/device")? {
      match fs.root_readlink(root, fp"${device_path}/device") {
        Ok(target) => usb_parent_address(target)
        Err(_) => null
      }
    } else {
      null
    }
    devices = devices.push({
      sysfs_name: device_path.name(),
      parent_device_index: parent_index,
      controller_pci_index: pci_function_index(pci_functions, controller_target),
      port_path: usb_port_path(device_path.name()),
      bus_number: bus_number,
      device_number: parse_integer(number.observation.value),
      vendor_id: parse_hex_optional(vendor.observation.value),
      product_id: parse_hex_optional(product.observation.value),
      device_version: version.observation.value,
      class_code: parse_hex_optional(class.observation.value),
      subclass: parse_hex_optional(subclass.observation.value),
      protocol: parse_hex_optional(protocol.observation.value),
      manufacturer: manufacturer.observation,
      product: product_text.observation,
      serial: serial.observation,
      speed_mbps: speed.observation.value,
      configuration_count: parse_integer(configurations.observation.value),
      active_configuration: parse_integer(active_configuration.observation.value),
      power_control: power_control.observation.value,
      autosuspend_delay_ms: parse_integer(autosuspend.observation.value),
      is_root_hub: device_path.name().starts_with("usb"),
      interfaces: interfaces,
    })
  }

  var state = report.Complete
  if listing.state == "absent" {
    state = report.Absent
  } else if listing.state != "complete" or issues.len() > 0 {
    state = report.Partial
  }
  return {
    status: {state: state, enumeration_succeeded: listing.enumeration_succeeded},
    devices: devices,
    issues: issues,
  }
}

pure value_or_null(value: collectors.SourceRead) -> Str? {
  if value.observation.state == report.Observed {
    return value.observation.value
  }
  return null
}

proc collect_identity(root: FsRoot, base: report.SystemReport) [fs, error] -> report.SystemReport {
  let release = read_value(root, p"proc/sys/kernel/osrelease")?
  let version = read_value(root, p"proc/version", max_bytes: 65536)?
  let hostname = read_value(root, p"proc/sys/kernel/hostname")?
  let boot_id = read_value(root, p"proc/sys/kernel/random/boot_id", max_bytes: 4096)?
  let uptime = read_value(root, p"proc/uptime", max_bytes: 4096)?
  let os_release = read_value(root, p"etc/os-release", max_bytes: 65536)?

  var issues = base.issues
  if release.observation.state != report.Observed {
    issues = issues.push(issue("identity", "kernel_release", release.observation.state, release.error_kind, release.errno))
  }
  if version.observation.state != report.Observed {
    issues = issues.push(issue("identity", "kernel_build", version.observation.state, version.error_kind, version.errno))
  }
  if os_release.observation.state != report.Observed {
    issues = issues.push(issue("identity", "os_release", os_release.observation.state, os_release.error_kind, os_release.errno))
  }

  var identity_status = report.Complete
  if issues.len() > base.issues.len() {
    identity_status = report.Partial
  }

  var os: report.OsRelease? = null
  if os_release.observation.value != null {
    var id: Str? = null
    var name: Str? = null
    var pretty_name: Str? = null
    var os_version: Str? = null
    var version_id: Str? = null
    for line in os_release.observation.value.lines() {
      let pair = line.split("=", maxsplit: 1)
      if pair.len() != 2 {
        continue
      }
      let key = pair[0].trim()
      var value = pair[1].trim()
      if value.count_chars() >= 2 and ((value.starts_with("\"") and value.ends_with("\"")) or (value.starts_with("'") and value.ends_with("'"))) {
        value = (value.split("") |> drop(1) |> take(value.count_chars() - 2)).join("")
      }
      match key {
        "ID" => id = value
        "NAME" => name = value
        "PRETTY_NAME" => pretty_name = value
        "VERSION" => os_version = value
        "VERSION_ID" => version_id = value
        _ => continue
      }
    }
    os = {id: id, name: name, pretty_name: pretty_name, version: os_version, version_id: version_id}
  }

  var uptime_seconds: Int? = null
  if uptime.observation.value != null {
    let fields = uptime.observation.value.split(" ")
    if fields.len() > 0 {
      uptime_seconds = fields[0].split(".")[0].parse_int() ?? null
    }
  }

  let firmware = read_firmware_identity(root)?
  let identity = {
    status: {state: identity_status, enumeration_succeeded: release.observation.state == report.Observed},
    kernel_release: value_or_null(release),
    kernel_build: value_or_null(version),
    architecture: null,
    os_release: os,
    hostname: hostname.observation,
    uptime_seconds: uptime_seconds,
    boot_id: boot_id.observation,
    firmware: firmware,
  }

  let scope = {
    ...base.scope,
    mount_namespace: namespace_observation(root, p"proc/self/ns/mnt")?,
    network_namespace: namespace_observation(root, p"proc/self/ns/net")?,
    pid_namespace: namespace_observation(root, p"proc/self/ns/pid")?,
    cgroup_namespace: namespace_observation(root, p"proc/self/ns/cgroup")?,
    visible_cgroup: read_value(root, p"proc/self/cgroup", max_bytes: 65536)?.observation,
  }

  return {...base, identity: identity, scope: scope, issues: issues}
}

proc namespace_observation(root: FsRoot, path: Path) [fs, error] -> report.TextObservation {
  match fs.root_readlink(root, path) {
    Ok(target) => return {state: report.Observed, value: target.display(), raw_bytes_base64: null}
    Err(_) => return empty_text(report.Absent)
  }
}

proc read_firmware_identity(root: FsRoot) [fs, error] -> report.FirmwareIdentity {
  let vendor = read_value(root, p"sys/class/dmi/id/sys_vendor", max_bytes: 4096)?
  let product = read_value(root, p"sys/class/dmi/id/product_name", max_bytes: 4096)?
  let board_vendor = read_value(root, p"sys/class/dmi/id/board_vendor", max_bytes: 4096)?
  let board_product = read_value(root, p"sys/class/dmi/id/board_name", max_bytes: 4096)?
  let bios_vendor = read_value(root, p"sys/class/dmi/id/bios_vendor", max_bytes: 4096)?
  let bios_version = read_value(root, p"sys/class/dmi/id/bios_version", max_bytes: 4096)?
  let serial = read_value(root, p"sys/class/dmi/id/product_serial", max_bytes: 4096)?
  let uuid = read_value(root, p"sys/class/dmi/id/product_uuid", max_bytes: 4096)?
  let dt_model = read_value(root, p"sys/firmware/devicetree/base/model", max_bytes: 4096)?
  let dt_compatible = read_value(root, p"sys/firmware/devicetree/base/compatible", max_bytes: 16384)?
  var compatible: List[report.TextObservation] = []
  if dt_compatible.observation.value != null {
    for value in dt_compatible.observation.value.split("\0") {
      if value != "" {
        compatible = compatible.push({state: report.Observed, value: value, raw_bytes_base64: null})
      }
    }
  }

  var source = "dmi"
  if vendor.observation.state != report.Observed and product.observation.state != report.Observed {
    if dt_model.observation.state == report.Observed or compatible.len() > 0 {
      source = "device-tree"
    } else {
      source = "unavailable"
    }
  }

  return {
    source: source,
    vendor: value_or_null(vendor),
    product: value_or_null(product),
    board_vendor: value_or_null(board_vendor),
    board_product: value_or_null(board_product),
    bios_vendor: value_or_null(bios_vendor),
    bios_version: value_or_null(bios_version),
    serial: serial.observation,
    uuid: uuid.observation,
    device_tree_model: dt_model.observation,
    device_tree_compatible: compatible,
  }
}

proc collect_cpu(root: FsRoot, base: report.SystemReport) [fs, error] -> report.SystemReport {
  let possible_source = read_value(root, p"sys/devices/system/cpu/possible", max_bytes: 65536)?
  let present_source = read_value(root, p"sys/devices/system/cpu/present", max_bytes: 65536)?
  let online_source = read_value(root, p"sys/devices/system/cpu/online", max_bytes: 65536)?
  let offline_source = read_value(root, p"sys/devices/system/cpu/offline", max_bytes: 65536)?
  var issues = base.issues
  let initial_issue_count = issues.len()
  let cpuset_source = read_effective_cgroup_cpuset(root)?
  if cpuset_source.state != report.Observed {
    issues = issues.push(issue("cpu", "cgroup.effective_cpuset", cpuset_source.state, cpuset_source.error_kind, cpuset_source.errno))
  }
  var possible: List[Int] = []
  var present: List[Int] = []
  var online: List[Int] = []
  var offline: List[Int] = []
  for (name, source) in [
    ("possible", possible_source),
    ("present", present_source),
    ("online", online_source),
    ("offline", offline_source),
  ] {
    if source.observation.value == null {
      issues = issues.push(issue("cpu", name, source.observation.state, source.error_kind, source.errno))
      continue
    }
    if name == "offline" and source.observation.value == "" {
      offline = []
      continue
    }
    let parsed = report.parse_cpu_list(source.observation.value)
    match parsed {
      Ok(ids) => {
        match name {
          "possible" => possible = ids
          "present" => present = ids
          "online" => online = ids
          "offline" => offline = ids
          _ => continue
        }
      }
      Err(_) => issues = issues.push(issue("cpu", name, report.Malformed, "invalid_cpu_list", null))
    }
  }

  var cpus: List[report.Cpu] = []
  var caches: List[report.CpuCache] = []
  let cpu_info = read_cpu_info(root)?
  for cpu_id in present {
    let info = cpu_info_for_id(cpu_info, cpu_id)
    let cpu_path = fp"sys/devices/system/cpu/cpu${cpu_id}"
    let package = read_value(root, fp"${cpu_path}/topology/physical_package_id", max_bytes: 4096)?
    let die = read_value(root, fp"${cpu_path}/topology/die_id", max_bytes: 4096)?
    let core = read_value(root, fp"${cpu_path}/topology/core_id", max_bytes: 4096)?
    let siblings = read_value(root, fp"${cpu_path}/topology/thread_siblings_list", max_bytes: 4096)?
    let node_listing = fs.root_children(root, fp"${cpu_path}", max_entries: 256)?
    var numa_node: Int? = null
    for node_path in node_listing.children {
      if node_path.name().starts_with("node") {
        numa_node = (node_path.name().split("") |> drop(4)).join("").parse_int() ?? null
        if numa_node != null {
          break
        }
      }
    }
    let is_online = cpu_id in online
    cpus = cpus.push({
      id: cpu_id,
      present: true,
      online: is_online,
      vendor: info.vendor,
      model: info.model,
      family: info.family,
      model_id: info.model_id,
      stepping: info.stepping,
      features: info.features,
      package_id: parse_integer(package.observation.value),
      die_id: parse_integer(die.observation.value),
      core_id: parse_integer(core.observation.value),
      thread_siblings: parse_list(siblings.observation.value),
      cache_ids: [],
      cache_indices: [],
      numa_node: numa_node,
      policy: null,
    })

    let cache_listing = fs.root_children(root, fp"${cpu_path}/cache", max_entries: 64)?
    for cache_path in cache_listing.children {
      if !cache_path.name().starts_with("index") {
        continue
      }
      let sysfs_index = (cache_path.name().split("") |> drop(5)).join("").parse_int() ?? null
      if sysfs_index == null or sysfs_index < 0 {
        issues = issues.push(issue("cpu", f"${cpu_id}.cache.${cache_path.name()}", report.Malformed, "invalid_cache_index", null))
        continue
      }
      let level_text = read_value(root, fp"${cache_path}/level", max_bytes: 4096)?
      let kind = read_value(root, fp"${cache_path}/type", max_bytes: 4096)?
      let size = read_value(root, fp"${cache_path}/size", max_bytes: 4096)?
      let line_size = read_value(root, fp"${cache_path}/coherency_line_size", max_bytes: 4096)?
      let sets = read_value(root, fp"${cache_path}/number_of_sets", max_bytes: 4096)?
      let shared = read_value(root, fp"${cache_path}/shared_cpu_list", max_bytes: 4096)?
      let level = parse_integer(level_text.observation.value) ?? 0
      let shared_cpus = parse_list(shared.observation.value)
      let cache_kind = kind.observation.value ?? "unknown"
      var already_seen = false
      for previous in caches {
        if shared_cpus.len() > 0 and previous.level == level and previous.kind == cache_kind and previous.shared_cpus == shared_cpus {
          already_seen = true
        }
      }
      if already_seen {
        continue
      }
      let size_bytes = parse_size_bytes(size.observation.value)
      caches = caches.push({
        id: caches.len(),
        sysfs_index: sysfs_index,
        owner_cpu_id: cpu_id,
        level: level,
        kind: cache_kind,
        size_bytes: size_bytes,
        line_size_bytes: parse_integer(line_size.observation.value),
        sets: parse_integer(sets.observation.value),
        shared_cpus: shared_cpus,
      })
    }
  }

  let policies = collect_frequency_policies(root, issues)?
  issues = policies.issues
  var linked_cpus: List[report.Cpu] = []
  for cpu in cpus {
    var policy_name: Str? = null
    var cache_ids: List[Int] = []
    for policy in policies.policies {
      if cpu.id in policy.related_cpus {
        policy_name = policy.name
        break
      }
    }
    for cache in caches {
      if cpu.id in cache.shared_cpus or (cache.shared_cpus.len() == 0 and cache.owner_cpu_id == cpu.id) {
        cache_ids = cache_ids.push(cache.id)
      }
    }
    linked_cpus = linked_cpus.push({
      ...cpu,
      policy: policy_name,
      cache_ids: cache_ids,
      cache_indices: cache_ids,
    })
  }
  let vulnerabilities_listing = fs.root_children(root, p"sys/devices/system/cpu/vulnerabilities", max_entries: 256)?
  var vulnerabilities: List[report.CpuVulnerability] = []
  for item in vulnerabilities_listing.children {
    let value = read_value(root, item, max_bytes: 16384)?
    vulnerabilities = vulnerabilities.push({name: item.name(), description: value.observation})
  }
  let idle_driver = read_value(root, p"sys/devices/system/cpu/cpuidle/current_driver", max_bytes: 4096)?
  let idle_governor = read_value(root, p"sys/devices/system/cpu/cpuidle/current_governor", max_bytes: 4096)?
  let available_idle_governors = read_value(root, p"sys/devices/system/cpu/cpuidle/available_governors", max_bytes: 4096)?
  let affinity = read_value(root, p"proc/self/status", max_bytes: 65536)?
  var affinity_cpus: List[Int] = []
  if affinity.observation.value != null {
    for line in affinity.observation.value.lines() {
      if line.starts_with("Cpus_allowed_list:") {
        affinity_cpus = parse_list(line.split(":", maxsplit: 1).get(1, "").trim())
      }
    }
  }

  var idle_states: List[report.CpuIdleState] = []
  for cpu_id in present {
    let cpuidle = fs.root_children(root, fp"sys/devices/system/cpu/cpu${cpu_id}/cpuidle", max_entries: 256)?
    for state_path in cpuidle.children {
      if !state_path.name().starts_with("state") {
        continue
      }
      let name = read_value(root, fp"${state_path}/name", max_bytes: 4096)?
      let description = read_value(root, fp"${state_path}/desc", max_bytes: 4096)?
      let disable = read_value(root, fp"${state_path}/disable", max_bytes: 4096)?
      let latency = read_value(root, fp"${state_path}/latency", max_bytes: 4096)?
      let residency = read_value(root, fp"${state_path}/residency", max_bytes: 4096)?
      let usage = read_value(root, fp"${state_path}/usage", max_bytes: 4096)?
      let time = read_value(root, fp"${state_path}/time", max_bytes: 4096)?
      idle_states = idle_states.push({
        cpu_id: cpu_id,
        name: name.observation.value ?? state_path.name(),
        description: description.observation.value,
        disable_setting: parse_integer(disable.observation.value),
        latency_us: parse_integer(latency.observation.value),
        residency_us: parse_integer(residency.observation.value),
        usage_count: parse_integer(usage.observation.value),
        time_us: parse_integer(time.observation.value),
      })
    }
  }

  var cpu_state = if issues.len() == initial_issue_count {report.Complete} else {report.Partial}
  if possible_source.observation.state != report.Observed {
    cpu_state = report.Partial
  }
  let cpu_section: report.CpuSection = {
    status: {state: cpu_state, enumeration_succeeded: possible_source.observation.state == report.Observed},
    possible: possible,
    present: present,
    online: online,
    offline: offline,
    affinity: affinity_cpus,
    effective_cpuset: cpuset_source.cpus,
    global_idle_driver: idle_driver.observation.value,
    global_idle_governor: idle_governor.observation.value,
    cpus: linked_cpus,
    caches: caches,
    frequency_policies: policies.policies,
    idle_states: idle_states,
    vulnerabilities: vulnerabilities,
    available_idle_governors: parse_words(available_idle_governors.observation.value),
  }
  return {...base, cpu: cpu_section, issues: issues}
}

type PolicyCollection = {policies: List[report.CpuFreqPolicy], issues: List[report.CollectionIssue]}

proc collect_frequency_policies(root: FsRoot, issues: List[report.CollectionIssue]) [fs, error] -> PolicyCollection {
  let listing = fs.root_children(root, p"sys/devices/system/cpu/cpufreq", max_entries: 1024)?
  var policies: List[report.CpuFreqPolicy] = []
  var collected_issues = issues
  if listing.state == "absent" {
    collected_issues = collected_issues.push(issue("cpu", "frequency_policies", report.Absent, "cpufreq_not_exposed", null))
  } else if listing.state != "complete" {
    collected_issues = collected_issues.push(issue("cpu", "frequency_policies", usb_observation_state(listing.state, false), listing.error_kind, listing.errno))
  }
  for directory in listing.children {
    if !directory.name().starts_with("policy") {
      continue
    }
    let related = read_value(root, fp"${directory}/related_cpus", max_bytes: 4096)?
    let affected = read_value(root, fp"${directory}/affected_cpus", max_bytes: 4096)?
    let driver = read_value(root, fp"${directory}/scaling_driver", max_bytes: 4096)?
    let governor = read_value(root, fp"${directory}/scaling_governor", max_bytes: 4096)?
    let available_governors = read_value(root, fp"${directory}/scaling_available_governors", max_bytes: 4096)?
    let hardware_min = read_value(root, fp"${directory}/cpuinfo_min_freq", max_bytes: 4096)?
    let hardware_max = read_value(root, fp"${directory}/cpuinfo_max_freq", max_bytes: 4096)?
    let scaling_min = read_value(root, fp"${directory}/scaling_min_freq", max_bytes: 4096)?
    let scaling_max = read_value(root, fp"${directory}/scaling_max_freq", max_bytes: 4096)?
    let hardware_current = read_value(root, fp"${directory}/cpuinfo_cur_freq", max_bytes: 4096)?
    let requested_current = read_value(root, fp"${directory}/scaling_cur_freq", max_bytes: 4096)?
    let average = read_value(root, fp"${directory}/cpuinfo_avg_freq", max_bytes: 4096)?
    let bios_limit = read_value(root, fp"${directory}/bios_limit", max_bytes: 4096)?
    let epp = read_value(root, fp"${directory}/energy_performance_preference", max_bytes: 4096)?
    let available_epp = read_value(root, fp"${directory}/energy_performance_available_preferences", max_bytes: 4096)?
    if related.observation.state != report.Observed {
      collected_issues = collected_issues.push(issue("cpu", f"${directory.name()}.related_cpus", related.observation.state, related.error_kind, related.errno))
    }
    if affected.observation.state != report.Observed {
      collected_issues = collected_issues.push(issue("cpu", f"${directory.name()}.affected_cpus", affected.observation.state, affected.error_kind, affected.errno))
    }
    if driver.observation.state != report.Observed {
      collected_issues = collected_issues.push(issue("cpu", f"${directory.name()}.driver", driver.observation.state, driver.error_kind, driver.errno))
    }
    if governor.observation.state != report.Observed {
      collected_issues = collected_issues.push(issue("cpu", f"${directory.name()}.governor", governor.observation.state, governor.error_kind, governor.errno))
    }
    var related_cpus: List[Int] = []
    var affected_cpus: List[Int] = []
    if related.observation.value != null {
      match report.parse_cpu_list(related.observation.value) {
        Ok(ids) => related_cpus = ids
        Err(_) => collected_issues = collected_issues.push(issue("cpu", f"${directory.name()}.related_cpus", report.Malformed, "invalid_cpu_list", null))
      }
    }
    if affected.observation.value != null {
      match report.parse_cpu_list(affected.observation.value) {
        Ok(ids) => affected_cpus = ids
        Err(_) => collected_issues = collected_issues.push(issue("cpu", f"${directory.name()}.affected_cpus", report.Malformed, "invalid_cpu_list", null))
      }
    }
    let boost = read_value(root, p"sys/devices/system/cpu/cpufreq/boost", max_bytes: 4096)?
    let no_turbo = read_value(root, p"sys/devices/system/cpu/intel_pstate/no_turbo", max_bytes: 4096)?
    var boost_supported: Bool? = null
    var boost_allowed: Bool? = null
    if boost.observation.value != null {
      boost_supported = true
      boost_allowed = boost.observation.value == "1"
    } else if no_turbo.observation.value != null {
      boost_supported = true
      boost_allowed = no_turbo.observation.value == "0"
    }
    let available_frequency = read_value(root, fp"${directory}/scaling_available_frequencies", max_bytes: 65536)?
    var frequencies: List[Int] = []
    for value in parse_words(available_frequency.observation.value) {
      let parsed = value.parse_int() ?? null
      if parsed != null and parsed >= 0 {
        frequencies = frequencies.push(parsed)
      }
    }
    policies = policies.push({
      name: directory.name(),
      related_cpus: related_cpus,
      affected_cpus: affected_cpus,
      driver: driver.observation.value,
      governor: governor.observation.value,
      available_governors: parse_words(available_governors.observation.value),
      hardware_min_khz: parse_integer(hardware_min.observation.value),
      hardware_max_khz: parse_integer(hardware_max.observation.value),
      scaling_min_khz: parse_integer(scaling_min.observation.value),
      scaling_max_khz: parse_integer(scaling_max.observation.value),
      hardware_current_khz: parse_integer(hardware_current.observation.value),
      requested_current_khz: parse_integer(requested_current.observation.value),
      governor_requested_khz: null,
      average_current_khz: parse_integer(average.observation.value),
      bios_limit_khz: parse_integer(bios_limit.observation.value),
      transition_latency_ns: null,
      available_frequencies_khz: frequencies,
      energy_performance_preference: epp.observation.value,
      available_energy_performance_preferences: parse_words(available_epp.observation.value),
      boost_supported: boost_supported,
      boost_allowed: boost_allowed,
      boost_active: null,
      boost_scope: if boost.observation.value != null {"system"} else if no_turbo.observation.value != null {"intel_pstate"} else {null},
    })
  }
  return {policies: policies, issues: collected_issues}
}

pure parse_size_bytes(value: Str?) -> Int? {
  if value == null {
    return null
  }
  let text = value.trim()
  if text.ends_with("K") {
    let number = (text.split("") |> take(text.count_chars() - 1)).join("").parse_int() ?? null
    if number == null or number < 0 or number > 9007199254740991 { return null }
    return number * 1024
  }
  if text.ends_with("M") {
    let number = (text.split("") |> take(text.count_chars() - 1)).join("").parse_int() ?? null
    if number == null or number < 0 or number > 8796093022207 { return null }
    return number * 1048576
  }
  if text.ends_with("G") {
    let number = (text.split("") |> take(text.count_chars() - 1)).join("").parse_int() ?? null
    if number == null or number < 0 or number > 8589934591 { return null }
    return number * 1073741824
  }
  let number = text.parse_int() ?? null
  if number != null and number >= 0 {
    return number
  }
  return null
}

proc collect_memory(root: FsRoot, base: report.SystemReport) [fs, error] -> report.SystemReport {
  let source = read_value(root, p"proc/meminfo", max_bytes: 1048576)?
  var total: Int? = null
  var free: Int? = null
  var available: Int? = null
  var buffers: Int? = null
  var cached: Int? = null
  var active: Int? = null
  var inactive: Int? = null
  var dirty: Int? = null
  var writeback: Int? = null
  var swap_total: Int? = null
  var swap_free: Int? = null
  var counters: List<report.MemoryCounter> = []
  var issues = base.issues
  if source.observation.value == null {
    issues = issues.push(issue("memory", "meminfo", source.observation.state, source.error_kind, source.errno))
  } else {
    for line in source.observation.value.lines() {
      let pair = line.split(":", maxsplit: 1)
      if pair.len() != 2 {
        continue
      }
      let name = pair[0].trim()
      let values = pair[1].trim().split(" ") |> where .trim() != ""
      if values.len() == 0 {
        continue
      }
      let parsed = values[0].parse_int() ?? null
      let has_kib_unit = values.len() > 1 and values[1] == "kB"
      let byte_counter = name in ["MemTotal", "MemFree", "MemAvailable", "Buffers", "Cached", "Active", "Inactive", "Dirty", "Writeback", "SwapTotal", "SwapFree"]
      if values.len() > 2 {
        issues = issues.push(issue("memory", f"meminfo.${name}", report.Malformed, "unexpected_meminfo_columns", null))
        continue
      }
      if parsed == null {
        let state = if decimal_identifier(values[0]) {report.RangeFailure} else {report.Malformed}
        let error_kind = if state == report.RangeFailure {"integer_out_of_range"} else {"invalid_integer"}
        issues = issues.push(issue("memory", f"meminfo.${name}", state, error_kind, null))
        continue
      }
      if parsed < 0 {
        issues = issues.push(issue("memory", f"meminfo.${name}", report.Malformed, "negative_integer", null))
        continue
      }
      if has_kib_unit and parsed > 9007199254740991 {
        issues = issues.push(issue("memory", f"meminfo.${name}", report.RangeFailure, "byte_count_out_of_range", null))
        continue
      }
      if byte_counter and !has_kib_unit {
        issues = issues.push(issue("memory", f"meminfo.${name}", report.Malformed, "invalid_byte_counter_unit", null))
        continue
      }
      let multiplier = if has_kib_unit {1024} else {1}
      let value = parsed * multiplier
      let unit = if has_kib_unit {"bytes"} else {values.get(1, "count")}
      counters = counters.push({name: name, value: value, unit: unit})
      match name {
        "MemTotal" => total = value
        "MemFree" => free = value
        "MemAvailable" => available = value
        "Buffers" => buffers = value
        "Cached" => cached = value
        "Active" => active = value
        "Inactive" => inactive = value
        "Dirty" => dirty = value
        "Writeback" => writeback = value
        "SwapTotal" => swap_total = value
        "SwapFree" => swap_free = value
        _ => continue
      }
    }
  }
  let swaps_source = read_value(root, p"proc/swaps", max_bytes: 262144)?
  var swaps: List[report.SwapDevice] = []
  if swaps_source.observation.value != null {
    var first_line = true
    for line in swaps_source.observation.value.lines() {
      if first_line {
        first_line = false
        continue
      }
      let columns = line.split(" ") |> where .trim() != ""
      if columns.len() < 5 {
        continue
      }
      let size_kib = columns[2].parse_int() ?? null
      let used_kib = columns[3].parse_int() ?? null
      if size_kib == null or used_kib == null or size_kib < 0 or used_kib < 0 {
        let overflow = (size_kib == null and decimal_identifier(columns[2])) or (used_kib == null and decimal_identifier(columns[3]))
        let state = if overflow {report.RangeFailure} else {report.Malformed}
        let error_kind = if overflow {"swap_counter_out_of_range"} else {"invalid_swap_counter"}
        issues = issues.push(issue("memory", "swaps", state, error_kind, null))
        continue
      }
      if size_kib > 9007199254740991 or used_kib > 9007199254740991 {
        issues = issues.push(issue("memory", "swaps", report.RangeFailure, "byte_count_overflow", null))
        continue
      }
      swaps = swaps.push({
        name: {state: report.Observed, value: columns[0], raw_bytes_base64: null},
        kind: columns[1],
        size_bytes: if size_kib == null {null} else {size_kib * 1024},
        used_bytes: if used_kib == null {null} else {used_kib * 1024},
        priority: columns[4].parse_int() ?? null,
      })
    }
  }

  var huge_pages: List[report.HugePagePool] = []
  let huge_listing = fs.root_children(root, p"sys/kernel/mm/hugepages", max_entries: 1024)?
  for huge_path in huge_listing.children {
    if !huge_path.name().starts_with("hugepages-") {
      continue
    }
    let size_text = huge_path.name().split("-").get(1, "").split("kB").get(0, "")
    let size_kib = size_text.parse_int() ?? null
    if size_kib == null or size_kib < 0 or size_kib > 9007199254740991 {
      issues = issues.push(issue("memory", f"huge_pages.${huge_path.name()}.page_size", report.Malformed, "invalid_page_size", null))
      continue
    }
    let total = read_value(root, fp"${huge_path}/nr_hugepages", max_bytes: 4096)?
    let free = read_value(root, fp"${huge_path}/free_hugepages", max_bytes: 4096)?
    let reserved = read_value(root, fp"${huge_path}/resv_hugepages", max_bytes: 4096)?
    let surplus = read_value(root, fp"${huge_path}/surplus_hugepages", max_bytes: 4096)?
    let total_count = parse_integer(total.observation.value)
    if total_count == null or total_count < 0 {
      issues = issues.push(issue("memory", f"huge_pages.${huge_path.name()}.total", report.Malformed, "invalid_count", total.errno))
      continue
    }
    huge_pages = huge_pages.push({
      node_id: null,
      page_size_bytes: size_kib * 1024,
      total: total_count,
      free: parse_integer(free.observation.value),
      reserved: parse_integer(reserved.observation.value),
      surplus: parse_integer(surplus.observation.value),
    })
  }

  let node_listing = fs.root_children(root, p"sys/devices/system/node", max_entries: 1024)?
  for node_path in node_listing.children {
    if !node_path.name().starts_with("node") {
      continue
    }
    let node_suffix = (node_path.name().split("") |> drop(4)).join("")
    let node_id = node_suffix.parse_int() ?? null
    let node_huge_listing = fs.root_children(root, fp"${node_path}/hugepages", max_entries: 1024)?
    for huge_path in node_huge_listing.children {
      if !huge_path.name().starts_with("hugepages-") {
        continue
      }
      let size_kib = huge_path.name().split("-").get(1, "").split("kB").get(0, "").parse_int() ?? null
      if size_kib == null or node_id == null or size_kib < 0 or size_kib > 9007199254740991 {
        issues = issues.push(issue("memory", f"${node_path.name()}/huge_pages/${huge_path.name()}", report.Malformed, "invalid_page_identity", null))
        continue
      }
      let total = read_value(root, fp"${huge_path}/nr_hugepages", max_bytes: 4096)?
      let free = read_value(root, fp"${huge_path}/free_hugepages", max_bytes: 4096)?
      let reserved = read_value(root, fp"${huge_path}/resv_hugepages", max_bytes: 4096)?
      let surplus = read_value(root, fp"${huge_path}/surplus_hugepages", max_bytes: 4096)?
      let total_count = parse_integer(total.observation.value)
      if total_count != null and total_count >= 0 {
        huge_pages = huge_pages.push({
          node_id: node_id,
          page_size_bytes: size_kib * 1024,
          total: total_count,
          free: parse_integer(free.observation.value),
          reserved: parse_integer(reserved.observation.value),
          surplus: parse_integer(surplus.observation.value),
        })
      }
    }
  }

  let cgroup_data = collect_cgroups(root)?
  issues = issues.extend(cgroup_data.issues)

  var transparent_huge_pages: List[Str] = []
  for name in ["enabled", "defrag"] {
    let policy = read_value(root, fp"sys/kernel/mm/transparent_hugepage/${name}", max_bytes: 4096)?
    if policy.observation.value != null {
      transparent_huge_pages = transparent_huge_pages.push(f"${name}=${policy.observation.value}")
    }
  }

  var pressure: List[report.PressureLine] = []
  for resource in ["cpu", "memory", "io"] {
    let source = read_value(root, fp"proc/pressure/${resource}", max_bytes: 16384)?
    if source.observation.value == null {
      if source.observation.state != report.Absent {
        issues = issues.push(issue("memory", f"pressure.${resource}", source.observation.state, source.error_kind, source.errno))
      }
      continue
    }
    for line in source.observation.value.lines() {
      let columns = parse_words(line)
      if columns.len() < 2 {
        issues = issues.push(issue("memory", f"pressure.${resource}", report.Malformed, "invalid_psi_line", null))
        continue
      }
      var avg10: Str? = null
      var avg60: Str? = null
      var avg300: Str? = null
      var total_us: Int? = null
      for column in columns |> drop(1) {
        let pair = column.split("=", maxsplit: 1)
        if pair.len() != 2 {
          continue
        }
        match pair[0] {
          "avg10" => avg10 = pair[1]
          "avg60" => avg60 = pair[1]
          "avg300" => avg300 = pair[1]
          "total" => total_us = pair[1].parse_int() ?? null
          _ => continue
        }
      }
      pressure = pressure.push({
        resource: resource,
        kind: columns[0],
        avg10: avg10,
        avg60: avg60,
        avg300: avg300,
        total_us: total_us,
      })
    }
  }

  var numa: List[report.MemoryCounter] = []
  for node_path in node_listing.children {
    if !node_path.name().starts_with("node") {
      continue
    }
    let node_id = (node_path.name().split("") |> drop(4)).join("").parse_int() ?? null
    let source = read_value(root, fp"${node_path}/meminfo", max_bytes: 65536)?
    if source.observation.value == null {
      continue
    }
    for line in source.observation.value.lines() {
      let pair = line.split(":", maxsplit: 1)
      if pair.len() != 2 {
        continue
      }
      let values = pair[1].trim().split(" ") |> where .trim() != ""
      if values.len() == 0 {
        continue
      }
      let raw = values[0].parse_int() ?? null
      if raw == null or raw < 0 {
        continue
      }
      let kib = values.len() > 1 and values[1] == "kB"
      if kib and raw > 9007199254740991 {
        issues = issues.push(issue("memory", f"numa.${node_path.name()}.${pair[0].trim()}", report.RangeFailure, "byte_count_overflow", null))
        continue
      }
      let value = if kib {raw * 1024} else {raw}
      let unit = if kib {"bytes"} else {"count"}
      numa = numa.push({name: f"node${node_id}.${pair[0].trim()}", value: value, unit: unit})
    }
  }

  var state = if source.observation.state == report.Observed {report.Complete} else {report.Partial}
  if total == null or issues.len() > base.issues.len() {
    state = report.Partial
  }
  let memory: report.MemorySection = {
    status: {state: state, enumeration_succeeded: source.observation.state == report.Observed},
    host: {
      total_bytes: total,
      free_bytes: free,
      available_bytes: available,
      buffers_bytes: buffers,
      cached_bytes: cached,
      active_bytes: active,
      inactive_bytes: inactive,
      dirty_bytes: dirty,
      writeback_bytes: writeback,
      swap_total_bytes: swap_total,
      swap_free_bytes: swap_free,
      counters: counters,
    },
    swaps: swaps,
    huge_pages: huge_pages,
    transparent_huge_pages: transparent_huge_pages,
    numa: numa,
    pressure: pressure,
    cgroup: cgroup_data.resources,
  }
  return {...base, memory: memory, issues: issues}
}

pure parent_relative_path(value: Str) -> Str {
  let components = value.split("/") |> where .trim() != ""
  if components.len() <= 1 {
    return ""
  }
  return components |> take(components.len() - 1) |> join("/")
}

pure cgroup_observation_state(maximum: collectors.SourceRead, current: collectors.SourceRead) -> report.ObservationState {
  if maximum.observation.state == report.Observed {
    return report.Observed
  }
  if maximum.observation.state != report.Absent and maximum.observation.state != report.Disappeared {
    return maximum.observation.state
  }
  return current.observation.state
}

pure cgroup_resource(
  path: Str,
  level: Int,
  controller: Str,
  resource: Str,
  state: report.ObservationState,
  maximum: Int?,
  current: Int?,
  unit: Str,
  unlimited: Bool?,
  quota: Int?,
  period: Int?,
  cpus: List[Int],
) -> report.CgroupResource {
  return {
    path: {state: report.Observed, value: path, raw_bytes_base64: null},
    hierarchy_level: level,
    controller: controller,
    resource: resource,
    state: state,
    maximum_value: maximum,
    current_value: current,
    unit: unit,
    maximum_unlimited: unlimited,
    quota: quota,
    period: period,
    effective_cpus: cpus,
    hidden_ancestors_possible: true,
  }
}

proc collect_cgroups(root: FsRoot) [fs, error] -> CgroupCollection {
  let self_cgroup = read_value(root, p"proc/self/cgroup", max_bytes: 65536)?
  let mountinfo = read_value(root, p"proc/self/mountinfo", max_bytes: 4194304)?
  var issues: List[report.CollectionIssue] = []
  var cgroup_path: Str? = null
  var mount_root: Str? = null
  var mount_point: Str? = null
  var has_v1 = false
  if self_cgroup.observation.value != null {
    for line in self_cgroup.observation.value.lines() {
      let fields = line.split(":")
      if fields.len() >= 3 and fields[0] == "0" and fields[1] == "" {
        cgroup_path = fields[2]
      }
    }
  }
  if mountinfo.observation.value != null {
    for line in mountinfo.observation.value.lines() {
      let fields = parse_words(line)
      var separator = 0
      while separator < fields.len() and fields[separator] != "-" {
        separator += 1
      }
      if separator + 1 >= fields.len() {
        continue
      }
      if fields[separator + 1] == "cgroup2" and mount_root == null and fields.len() > 4 {
        mount_root = decode_mount_field(fields[3])
        mount_point = decode_mount_field(fields[4])
      } else if fields[separator + 1] == "cgroup" {
        has_v1 = true
      }
    }
  }

  var resources: List[report.CgroupResource] = []
  if cgroup_path == null or mount_root == null or mount_point == null {
    if has_v1 {
      issues = issues.push(issue("memory", "cgroup.v1", report.Unsupported, "cgroup_v1_or_hybrid", null))
    } else {
      let state = if self_cgroup.observation.state != report.Observed {self_cgroup.observation.state} else {report.Absent}
      issues = issues.push(issue("memory", "cgroup_v2", state, "cgroup_v2_mount_unavailable", self_cgroup.errno))
    }
    return {resources: resources, issues: issues}
  }
  if has_v1 {
    issues = issues.push(issue("memory", "cgroup.v1", report.Unsupported, "cgroup_v1_controllers_present", null))
  }

  let group = cgroup_path ?? ""
  let root_path = mount_root ?? ""
  let absolute_mount_point = mount_point ?? ""
  if !group.starts_with("/") or !root_path.starts_with("/") or !absolute_mount_point.starts_with("/") {
    issues = issues.push(issue("memory", "cgroup.path", report.Malformed, "invalid_cgroup_mount_path", null))
    return {resources: resources, issues: issues}
  }
  var relative = ""
  if root_path == "/" {
    relative = (group.split("") |> drop(1)).join("")
  } else if group == root_path {
    relative = ""
  } else if group.starts_with(f"${root_path}/") {
    let prefix_length = (root_path.count_chars()) + 1
    relative = (group.split("") |> drop(prefix_length)).join("")
  } else {
    issues = issues.push(issue("memory", "cgroup.path", report.Unsupported, "cgroup_path_outside_visible_mount", null))
    return {resources: resources, issues: issues}
  }

  let mount_relative = (absolute_mount_point.split("/") |> where .trim() != "").join("/")
  var hierarchy_level = 0
  var visited = 0
  var current_relative = relative
  while visited < 64 {
    let source_path = if current_relative == "" {
      fp"${mount_relative}"
    } else {
      fp"${mount_relative}/${current_relative}"
    }
    let visible_path = if current_relative == "" {root_path} else {f"${root_path}/${current_relative}"}

    let memory_max = read_value(root, fp"${source_path}/memory.max", max_bytes: 4096)?
    let memory_current = read_value(root, fp"${source_path}/memory.current", max_bytes: 4096)?
    if memory_max.observation.state != report.Absent or memory_current.observation.state != report.Absent {
      let max_text = memory_max.observation.value
      let unlimited = if max_text == "max" {true} else if max_text != null {false} else {null}
      let maximum = if unlimited == true {null} else {parse_integer(max_text)}
      let current = parse_integer(memory_current.observation.value)
      resources = resources.push(cgroup_resource(
        visible_path, hierarchy_level, "memory", "memory.max", cgroup_observation_state(memory_max, memory_current),
        maximum, current, "bytes", unlimited, null, null, [],
      ))
      if memory_max.observation.state != report.Observed or memory_current.observation.state != report.Observed {
        issues = issues.push(issue("memory", f"cgroup.${hierarchy_level}.memory", cgroup_observation_state(memory_max, memory_current), "cgroup_memory_value_unavailable", memory_max.errno ?? memory_current.errno))
      }
    }

    let memory_swap_max = read_value(root, fp"${source_path}/memory.swap.max", max_bytes: 4096)?
    let memory_swap_current = read_value(root, fp"${source_path}/memory.swap.current", max_bytes: 4096)?
    if memory_swap_max.observation.state != report.Absent or memory_swap_current.observation.state != report.Absent {
      let max_text = memory_swap_max.observation.value
      let unlimited = if max_text == "max" {true} else if max_text != null {false} else {null}
      resources = resources.push(cgroup_resource(
        visible_path, hierarchy_level, "memory", "memory.swap.max",
        cgroup_observation_state(memory_swap_max, memory_swap_current),
        if unlimited == true {null} else {parse_integer(max_text)},
        parse_integer(memory_swap_current.observation.value), "bytes", unlimited, null, null, [],
      ))
      if memory_swap_max.observation.state != report.Observed or memory_swap_current.observation.state != report.Observed {
        issues = issues.push(issue("memory", f"cgroup.${hierarchy_level}.memory.swap", cgroup_observation_state(memory_swap_max, memory_swap_current), "cgroup_memory_swap_value_unavailable", memory_swap_max.errno ?? memory_swap_current.errno))
      }
    }

    let cpu_max = read_value(root, fp"${source_path}/cpu.max", max_bytes: 4096)?
    if cpu_max.observation.value != null {
      let fields = parse_words(cpu_max.observation.value)
      if fields.len() >= 2 {
        let unlimited = fields[0] == "max"
        let quota = if unlimited {null} else {fields[0].parse_int() ?? null}
        let period = fields[1].parse_int() ?? null
        if (!unlimited and (quota == null or quota <= 0)) or period == null or period <= 0 {
          issues = issues.push(issue("memory", f"cgroup.${hierarchy_level}.cpu.max", report.Malformed, "invalid_cpu_quota", null))
        } else {
          resources = resources.push(cgroup_resource(
            visible_path, hierarchy_level, "cpu", "cpu.max", report.Observed,
            null, null, "quota_period", unlimited, quota, period, [],
          ))
        }
      } else {
        issues = issues.push(issue("memory", f"cgroup.${hierarchy_level}.cpu.max", report.Malformed, "invalid_cpu_max", null))
      }
    } else if cpu_max.observation.state != report.Absent {
      issues = issues.push(issue("memory", f"cgroup.${hierarchy_level}.cpu.max", cpu_max.observation.state, cpu_max.error_kind, cpu_max.errno))
    }

    let cpu_stat = read_value(root, fp"${source_path}/cpu.stat", max_bytes: 16384)?
    if cpu_stat.observation.value != null {
      for line in cpu_stat.observation.value.lines() {
        let fields = parse_words(line)
        if fields.len() != 2 {
          issues = issues.push(issue("memory", f"cgroup.${hierarchy_level}.cpu.stat", report.Malformed, "invalid_cpu_stat_row", null))
          continue
        }
        if fields[0] not in ["usage_usec", "user_usec", "system_usec", "nr_periods", "nr_throttled", "throttled_usec", "nr_bursts", "burst_usec"] {
          continue
        }
        let value = fields[1].parse_int() ?? null
        if value == null or value < 0 {
          issues = issues.push(issue("memory", f"cgroup.${hierarchy_level}.cpu.stat.${fields[0]}", report.Malformed, "invalid_cpu_stat_value", null))
          continue
        }
        let unit = if fields[0].starts_with("nr_") {"count"} else {"microseconds"}
        resources = resources.push(cgroup_resource(
          visible_path, hierarchy_level, "cpu", f"cpu.stat.${fields[0]}", report.Observed,
          null, value, unit, null, null, null, [],
        ))
      }
    } else if cpu_stat.observation.state != report.Absent {
      issues = issues.push(issue("memory", f"cgroup.${hierarchy_level}.cpu.stat", cpu_stat.observation.state, cpu_stat.error_kind, cpu_stat.errno))
    }

    let cpuset = read_value(root, fp"${source_path}/cpuset.cpus.effective", max_bytes: 65536)?
    if cpuset.observation.state == report.Observed and cpuset.observation.value != null {
      if cpuset.observation.value == "" {
        resources = resources.push(cgroup_resource(
          visible_path, hierarchy_level, "cpuset", "cpuset.cpus.effective", report.Observed,
          null, null, "cpu_ids", null, null, null, [],
        ))
      } else {
        match report.parse_cpu_list(cpuset.observation.value) {
          Ok(cpus) => resources = resources.push(cgroup_resource(
            visible_path, hierarchy_level, "cpuset", "cpuset.cpus.effective", report.Observed,
            null, null, "cpu_ids", null, null, null, cpus,
          ))
          Err(_) => issues = issues.push(issue("memory", f"cgroup.${hierarchy_level}.cpuset.cpus.effective", report.Malformed, "invalid_cgroup_cpu_list", null))
        }
      }
    } else if cpuset.observation.state != report.Absent and cpuset.observation.value == null {
      issues = issues.push(issue("memory", f"cgroup.${hierarchy_level}.cpuset.cpus.effective", cpuset.observation.state, cpuset.error_kind, cpuset.errno))
    }

    let pids_max = read_value(root, fp"${source_path}/pids.max", max_bytes: 4096)?
    let pids_current = read_value(root, fp"${source_path}/pids.current", max_bytes: 4096)?
    if pids_max.observation.state != report.Absent or pids_current.observation.state != report.Absent {
      let max_text = pids_max.observation.value
      let unlimited = if max_text == "max" {true} else if max_text != null {false} else {null}
      resources = resources.push(cgroup_resource(
        visible_path, hierarchy_level, "pids", "pids.max", cgroup_observation_state(pids_max, pids_current),
        if unlimited == true {null} else {parse_integer(max_text)}, parse_integer(pids_current.observation.value),
        "count", unlimited, null, null, [],
      ))
      if pids_max.observation.state != report.Observed or pids_current.observation.state != report.Observed {
        issues = issues.push(issue("memory", f"cgroup.${hierarchy_level}.pids", cgroup_observation_state(pids_max, pids_current), "cgroup_pids_value_unavailable", pids_max.errno ?? pids_current.errno))
      }
    }

    let io_stat = read_value(root, fp"${source_path}/io.stat", max_bytes: 262144)?
    if io_stat.observation.value != null {
      for line in io_stat.observation.value.lines() {
        let fields = parse_words(line)
        if fields.len() < 2 {
          issues = issues.push(issue("memory", f"cgroup.${hierarchy_level}.io.stat", report.Malformed, "invalid_io_stat_row", null))
          continue
        }
        for item in fields |> drop(1) {
          let pair = item.split("=", maxsplit: 1)
          if pair.len() != 2 or pair[0] not in ["rbytes", "wbytes", "rios", "wios", "dbytes", "dios"] {
            continue
          }
          let value = pair[1].parse_int() ?? null
          if value == null or value < 0 {
            issues = issues.push(issue("memory", f"cgroup.${hierarchy_level}.io.stat.${fields[0]}.${pair[0]}", report.Malformed, "invalid_io_counter", null))
            continue
          }
          let unit = if pair[0].ends_with("bytes") {"bytes"} else {"requests"}
          resources = resources.push(cgroup_resource(
            visible_path, hierarchy_level, "io", f"io.stat.${fields[0]}.${pair[0]}", report.Observed,
            null, value, unit, null, null, null, [],
          ))
        }
      }
    } else if io_stat.observation.state != report.Absent {
      issues = issues.push(issue("memory", f"cgroup.${hierarchy_level}.io.stat", io_stat.observation.state, io_stat.error_kind, io_stat.errno))
    }

    if current_relative == "" {
      return {resources: resources, issues: issues}
    }
    current_relative = parent_relative_path(current_relative)
    hierarchy_level += 1
    visited += 1
  }
  issues = issues.push(issue("memory", "cgroup.ancestors", report.Truncated, "ancestor_limit_64", null))
  return {resources: resources, issues: issues}
}

pure network_text(value: Str?) -> report.TextObservation {
  if value == null {
    return empty_text(report.Absent)
  }
  return {state: report.Observed, value: value, raw_bytes_base64: null}
}

pure network_link_name(value: LinuxNetworkLink) -> report.TextObservation {
  if value.name != null {
    return network_text(value.name)
  }
  if value.name_bytes != null {
    return {state: report.Malformed, value: null, raw_bytes_base64: value.name_bytes.base64()}
  }
  return empty_text(report.Absent)
}

pure network_mac(value: Bytes?) -> report.TextObservation {
  if value == null {
    return empty_text(report.Absent)
  }
  var parts: List[Str] = []
  for byte in value.chunks(1) {
    parts = parts.push(byte.hex())
  }
  return {state: report.Observed, value: parts.join(":"), raw_bytes_base64: null}
}

pure network_attributes(values: List[LinuxNetlinkAttribute]) -> List[report.NetworkAttribute] {
  var output: List[report.NetworkAttribute] = []
  for attribute in values {
    output = output.push({
      kind: attribute.kind,
      data: {state: report.Observed, value: attribute.data.base64(), raw_bytes_base64: null},
    })
  }
  return output
}

pure network_family(value: Str) -> Str {
  match value {
    "inet" => return "ipv4"
    "inet6" => return "ipv6"
    _ => return value
  }
}

pure network_link_flags(value: Int) -> List[Str] {
  var flags: List[Str] = []
  if value.bit_and(1) != 0 { flags = flags.push("up") }
  if value.bit_and(2) != 0 { flags = flags.push("broadcast") }
  if value.bit_and(8) != 0 { flags = flags.push("loopback") }
  if value.bit_and(16) != 0 { flags = flags.push("point_to_point") }
  if value.bit_and(64) != 0 { flags = flags.push("running") }
  if value.bit_and(256) != 0 { flags = flags.push("promiscuous") }
  if value.bit_and(4096) != 0 { flags = flags.push("multicast") }
  if value.bit_and(65536) != 0 { flags = flags.push("lower_up") }
  if value.bit_and(131072) != 0 { flags = flags.push("dormant") }
  flags = flags.push(f"raw_bits=${value}")
  return flags
}

pure network_operstate(value: Int?) -> Str? {
  if value == null { return null }
  match value {
    0 => return "unknown"
    1 => return "not_present"
    2 => return "down"
    3 => return "lower_layer_down"
    4 => return "testing"
    5 => return "dormant"
    6 => return "up"
    _ => return f"operstate_${value}"
  }
}

pure network_address_scope(value: Int) -> Str {
  match value {
    0 => return "global"
    200 => return "site"
    253 => return "link"
    254 => return "host"
    255 => return "nowhere"
    _ => return f"scope_${value}"
  }
}

pure network_route_type(value: Int) -> Str {
  match value {
    1 => return "unicast"
    2 => return "local"
    3 => return "broadcast"
    4 => return "anycast"
    5 => return "multicast"
    6 => return "blackhole"
    7 => return "unreachable"
    8 => return "prohibit"
    9 => return "throw"
    10 => return "nat"
    11 => return "external_resolve"
    _ => return f"route_type_${value}"
  }
}

pure network_route_protocol(value: Int) -> Str {
  match value {
    0 => return "unspecified"
    1 => return "redirect"
    2 => return "kernel"
    3 => return "boot"
    4 => return "static"
    8 => return "gated"
    9 => return "router_advertisement"
    16 => return "dhcp"
    _ => return f"protocol_${value}"
  }
}

pure network_rule_action(value: Int) -> Str {
  match value {
    1 => return "to_table"
    2 => return "goto"
    3 => return "nop"
    6 => return "blackhole"
    7 => return "unreachable"
    8 => return "prohibit"
    _ => return f"action_${value}"
  }
}

pure network_section_state(value: Str) -> report.SectionState {
  match value {
    "complete" => return report.Complete
    "unsupported" => return report.Unsupported
    "permission_denied" => return report.PermissionDenied
    "malformed" => return report.Malformed
    "truncated" or "limited" => return report.Truncated
    "interrupted" => return report.Raced
    _ => return report.Partial
  }
}

pure network_issue_state(value: Str) -> report.ObservationState {
  match value {
    "permission_denied" => return report.PermissionDenied
    "unsupported" => return report.Unsupported
    "malformed" => return report.Malformed
    "truncated" => return report.Truncated
    "limited" => return report.Truncated
    "interrupted" => return report.Raced
    _ => return report.ReadFailure
  }
}

pure link_index_by_name(links: List[LinuxNetworkLink], name: Str?) -> Int? {
  if name == null { return null }
  for link in links {
    if link.name == name { return link.ifindex }
  }
  return null
}

## Converts one typed route-netlink result into the report's stable network model.
export pure assemble_network_dump(value: LinuxNetworkDump) -> NetworkCollection {
  var links: List[report.NetworkLink] = []
  for raw_link in value.links {
    var addresses: List[report.NetworkAddress] = []
    for raw_address in value.addresses {
      if raw_address.ifindex != raw_link.ifindex { continue }
      let address_value = if raw_address.local != null {raw_address.local} else {raw_address.address}
      addresses = addresses.push({
        family: network_family(raw_address.family),
        address: network_text(address_value),
        prefix_length: raw_address.prefix_length,
        broadcast: network_text(raw_address.broadcast),
        scope: network_address_scope(raw_address.scope),
        flags: raw_address.flags,
        valid_lifetime_seconds: raw_address.valid_lifetime_seconds,
        preferred_lifetime_seconds: raw_address.preferred_lifetime_seconds,
        attributes: network_attributes(raw_address.attributes),
      })
    }

    var counters: List[report.MemoryCounter] = []
    if raw_link.rx_bytes != null {
      counters = counters.push({name: "rx_bytes", value: raw_link.rx_bytes, unit: "bytes"})
    }
    if raw_link.tx_bytes != null {
      counters = counters.push({name: "tx_bytes", value: raw_link.tx_bytes, unit: "bytes"})
    }
    links = links.push({
      ifindex: raw_link.ifindex,
      hardware_type: raw_link.hardware_type,
      name: network_link_name(raw_link),
      kind: raw_link.kind,
      mtu: raw_link.mtu,
      admin_up: raw_link.flags.bit_and(1) != 0,
      operational_state: network_operstate(raw_link.operstate),
      flags: network_link_flags(raw_link.flags),
      mac: network_mac(raw_link.address),
      master_ifindex: raw_link.master_ifindex,
      lower_ifindex: raw_link.lower_ifindex,
      parent_pci_function_index: null,
      parent_usb_device_index: null,
      driver: null,
      addresses: addresses,
      counters: counters,
      attributes: network_attributes(raw_link.attributes),
    })
  }

  var routes: List[report.NetworkRoute] = []
  for raw_route in value.routes {
    let family = network_family(raw_route.family)
    let default_destination: Str? = if family == "ipv6" {"::"} else if family == "ipv4" {"0.0.0.0"} else {null}
    let destination = if raw_route.destination != null {raw_route.destination} else {default_destination}
    var nexthops: List[report.NetworkNexthop] = []
    for nexthop in raw_route.nexthops {
      nexthops = nexthops.push({
        ifindex: nexthop.ifindex,
        flags: nexthop.flags,
        hops: nexthop.hops,
        gateway: network_text(nexthop.gateway),
      })
    }
    routes = routes.push({
      family: family,
      destination: network_text(destination),
      prefix_length: raw_route.destination_prefix_length,
      source_prefix_length: raw_route.source_prefix_length,
      source: network_text(raw_route.source),
      preferred_source: network_text(raw_route.preferred_source),
      gateway: network_text(raw_route.gateway),
      table: raw_route.table,
      metric: raw_route.priority,
      route_type: network_route_type(raw_route.route_type),
      scope: network_address_scope(raw_route.scope),
      protocol: network_route_protocol(raw_route.protocol),
      output_ifindex: raw_route.output_ifindex,
      input_ifindex: raw_route.input_ifindex,
      flags: raw_route.flags,
      nexthops: nexthops,
      attributes: network_attributes(raw_route.attributes),
    })
  }

  var rules: List[report.NetworkRule] = []
  for raw_rule in value.rules {
    rules = rules.push({
      family: network_family(raw_rule.family),
      destination_prefix_length: raw_rule.destination_prefix_length,
      source_prefix_length: raw_rule.source_prefix_length,
      priority: raw_rule.priority,
      source: network_text(raw_rule.source),
      destination: network_text(raw_rule.destination),
      fwmark: raw_rule.fwmark,
      fwmask: raw_rule.fwmask,
      table: raw_rule.table,
      action: network_rule_action(raw_rule.action),
      input_ifindex: link_index_by_name(value.links, raw_rule.input_name),
      output_ifindex: link_index_by_name(value.links, raw_rule.output_name),
      flags: raw_rule.flags,
      attributes: network_attributes(raw_rule.attributes),
    })
  }

  var issues: List[report.CollectionIssue] = []
  for raw_issue in value.issues {
    issues = issues.push(issue_with_detail(
      "network",
      f"netlink.${raw_issue.object}",
      network_issue_state(raw_issue.state),
      raw_issue.error_kind,
      raw_issue.errno,
      raw_issue.message,
    ))
  }
  return {
    status: {
      state: network_section_state(value.state),
      enumeration_succeeded: value.state == "complete",
    },
    links: links,
    routes: routes,
    rules: rules,
    issues: issues,
  }
}

proc collect_network(root: FsRoot, pci_functions: List[report.PciFunction], usb_devices: List[report.UsbDevice]) [env, fs, error] -> NetworkCollection {
  let result = env XSH_LINUX_REAL="1" {
    linux.network_dump()
  }
  match result {
    Ok(value) => {
      let assembled = assemble_network_dump(value)
      var links: List[report.NetworkLink] = []
      var issues = assembled.issues
      for link in assembled.links {
        var driver: Str? = null
        var parent_pci_function_index: Int? = null
        var parent_usb_device_index: Int? = null
        if link.name.value != null {
          let name = link.name.value
          let driver_path = fp"sys/class/net/${name}/device/driver"
          match fs.root_exists(root, driver_path) {
            Ok(true) => {
              match fs.root_readlink(root, driver_path) {
                Ok(target) => driver = target.name()
                Err(failure) => issues = issues.push(issue_with_detail(
                  "network", f"links.${link.ifindex}.driver", report.ReadFailure,
                  failure.kind, null, failure.message,
                ))
              }
            }
            Err(failure) => issues = issues.push(issue_with_detail(
              "network", f"links.${link.ifindex}.driver", report.ReadFailure,
              failure.kind, null, failure.message,
            ))
            _ => {}
          }

          let device_path = fp"sys/class/net/${name}/device"
          match fs.root_exists(root, device_path) {
            Ok(true) => {
              match fs.root_readlink(root, device_path) {
                Ok(target) => {
                  parent_pci_function_index = pci_function_index(pci_functions, usb_parent_address(target))
                  parent_usb_device_index = usb_device_index_from_target(usb_devices, target)
                }
                Err(failure) => issues = issues.push(issue_with_detail(
                  "network", f"links.${link.ifindex}.parent", report.ReadFailure,
                  failure.kind, null, failure.message,
                ))
              }
            }
            Err(failure) => issues = issues.push(issue_with_detail(
              "network", f"links.${link.ifindex}.parent", report.ReadFailure,
              failure.kind, null, failure.message,
            ))
            _ => {}
          }
        }
        links = links.push({
          ...link,
          driver: driver,
          parent_pci_function_index: parent_pci_function_index,
          parent_usb_device_index: parent_usb_device_index,
        })
      }
      let status = if issues.len() > assembled.issues.len() and assembled.status.state == report.Complete {
        {state: report.Partial, enumeration_succeeded: assembled.status.enumeration_succeeded}
      } else {
        assembled.status
      }
      return {...assembled, status: status, links: links, issues: issues}
    }
    Err(failure) => {
      let collection_issue = issue_with_detail(
        "network", "netlink", report.ReadFailure, failure.kind, null, failure.message,
      )
      return {
        status: {state: report.Partial, enumeration_succeeded: false},
        links: [],
        routes: [],
        rules: [],
        issues: [collection_issue],
      }
    }
  }
}

pure requested(selected: Str, name: Str) -> Bool {
  return selected == "" or selected == name
}

pure unsupported_issue(section: Str, field: Str) -> report.CollectionIssue {
  return issue(section, field, report.Unsupported, "collector_not_implemented", null)
}

## Collects from a caller-owned source root; local capacity queries are opt-in.
export proc collect_from_root(root: FsRoot, architecture: Str, page_size_bytes: Int, clock_ticks_per_second: Int, selected: Str = "", sensitive: Bool = false, include_local_mount_usage: Bool = false) [fs, time, error] -> report.SystemReport {
  if page_size_bytes <= 0 or clock_ticks_per_second <= 0 {
    return Err(Error(kind: "system-report-execution-units", message: "page size and clock ticks per second must be positive"))
  }
  if selected != "" {
    report.parse_report_section(selected)?
  }
  let started = time.now()
  var output = empty_report(started, page_size_bytes, clock_ticks_per_second)
  output = collect_identity(root, output)?
  output = {...output, identity: {...output.identity, architecture: architecture}}
  if requested(selected, "cpu") {
    output = collect_cpu(root, output)?
  }
  if requested(selected, "memory") {
    output = collect_memory(root, output)?
  }
  let pci_dependency = selected == "usb" or selected == "storage" or selected == "network" or selected == "devices"
  if requested(selected, "pci") or pci_dependency {
    let pci = collectors.collect_pci(root)?
    output = {...output, pci: {status: pci.status, functions: pci.functions}, issues: output.issues.extend(pci.issues)}
  }
  if requested(selected, "usb") or selected == "network" or selected == "devices" {
    let usb = collect_usb(root, output.pci.functions)?
    output = {...output, usb: {status: usb.status, devices: usb.devices}, issues: output.issues.extend(usb.issues)}
  }
  if requested(selected, "storage") {
    let storage = collect_storage(root, output.pci.functions, include_local_mount_usage)?
    output = {
      ...output,
      storage: {status: storage.status, devices: storage.devices, mounts: storage.mounts},
      issues: output.issues.extend(storage.issues),
    }
  }
  if requested(selected, "sensors") {
    let sensors = collect_sensors(root)?
    output = {
      ...output,
      sensors: {status: sensors.status, channels: sensors.channels, thermal_zones: sensors.thermal_zones},
      issues: output.issues.extend(sensors.issues),
    }
  }
  if requested(selected, "power") {
    let power = collect_power(root)?
    output = {
      ...output,
      power: {status: power.status, supplies: power.supplies, cap_zones: power.cap_zones},
      issues: output.issues.extend(power.issues),
    }
  }
  if requested(selected, "processes") {
    let processes = collect_processes(root, page_size_bytes)?
    output = {
      ...output,
      processes: {
        status: processes.status,
        processes: link_process_cgroups(processes.processes, output.memory.cgroup),
      },
      issues: output.issues.extend(processes.issues),
    }
  }
  if requested(selected, "kernel") {
    let kernel = collect_kernel(root)?
    output = {
      ...output,
      kernel: {
        status: kernel.status,
        command_line: kernel.command_line,
        modules: kernel.modules,
        parameters: kernel.parameters,
        sysctls: kernel.sysctls,
      },
      issues: output.issues.extend(kernel.issues),
    }
  }
  if requested(selected, "firmware") {
    let firmware = collect_firmware(root)?
    output = {
      ...output,
      firmware: {
        status: firmware.status,
        source: firmware.source,
        records: firmware.records,
        limitation: firmware.limitation,
      },
      issues: output.issues.extend(firmware.issues),
    }
  }
  if requested(selected, "devices") {
    let devices = collect_device_classes(root, output.pci.functions, output.usb.devices)?
    output = {
      ...output,
      devices: {status: devices.status, devices: devices.devices},
      issues: output.issues.extend(devices.issues),
    }
  }

  for (name, state) in [
    ("usb", output.usb.status.state),
    ("storage", output.storage.status.state),
    ("network", output.network.status.state),
    ("sensors", output.sensors.status.state),
    ("power", output.power.status.state),
    ("firmware", output.firmware.status.state),
    ("kernel", output.kernel.status.state),
    ("processes", output.processes.status.state),
    ("devices", output.devices.status.state),
  ] {
    if requested(selected, name) and state == report.NotRequested {
      if name == "network" and include_local_mount_usage {
        continue
      }
      output = mark_unsupported(output, name)
    }
  }

  let ended = time.now()
  output = {
    ...output,
    source_mode: report.SyntheticFixture,
    collection_ended_unix_ms: ended,
    elapsed_ms: ended - started,
  }
  if !sensitive {
    output = report.redact_report(output)
  }
  return output
}

pure mark_unsupported(value: report.SystemReport, name: Str) -> report.SystemReport {
  var issues = value.issues
  issues = issues.push(unsupported_issue(name, "section"))
  match name {
    "usb" => return {...value, usb: {status: empty_status(report.Unsupported), devices: []}, issues: issues}
    "storage" => return {...value, storage: {status: empty_status(report.Unsupported), devices: [], mounts: []}, issues: issues}
    "network" => return {...value, network: {status: empty_status(report.Unsupported), links: [], routes: [], rules: []}, issues: issues}
    "sensors" => return {...value, sensors: {status: empty_status(report.Unsupported), channels: [], thermal_zones: []}, issues: issues}
    "power" => return {...value, power: {status: empty_status(report.Unsupported), supplies: [], cap_zones: []}, issues: issues}
    "firmware" => return {...value, firmware: {status: empty_status(report.Unsupported), source: "unavailable", records: [], limitation: empty_text(report.Unsupported)}, issues: issues}
    "kernel" => return {...value, kernel: {status: empty_status(report.Unsupported), command_line: empty_text(report.Unsupported), modules: [], parameters: [], sysctls: []}, issues: issues}
    "processes" => return {...value, processes: {status: empty_status(report.Unsupported), processes: []}, issues: issues}
    "devices" => return {...value, devices: {status: empty_status(report.Unsupported), devices: []}, issues: issues}
    _ => return value
  }
}

export type SystemReportLiveCollector = module {
  export proc collect_from_root(root: FsRoot, architecture: Str, page_size_bytes: Int, clock_ticks_per_second: Int, selected: Str = "", sensitive: Bool = false, include_local_mount_usage: Bool = false) [fs, time, error] -> report.SystemReport
  export proc collect_live(selected: Str = "", sensitive: Bool = false) [env, fs, time, system, error] -> report.SystemReport
}

pure is_truthy(value: Str) -> Bool {
  return value == "1" or value == "true" or value == "yes" or value == "on"
}

proc dry_run_enabled() [env] -> Bool {
  match env.get("XSH_LINUX_DRY_RUN") {
    Ok(value) => return is_truthy(value)
    Err(_) => return false
  }
}

## Collects the current process-visible Linux view and eligible local mount capacity.
export proc collect_live(selected: Str = "", sensitive: Bool = false) [env, fs, time, system, error] -> report.SystemReport {
  if dry_run_enabled()? {
    return Err(Error(kind: "system-report-dry-run", message: "system-report refuses live collection while XSH Linux dry-run mode is active"))
  }
  let uname = system.uname()?
  if uname.sysname != "Linux" {
    return Err(Error(kind: "system-report-platform", message: "live system-report collection is supported on Linux only"))
  }
  let root = fs.open_root(p"/")?
  defer fs.close_root(root)?
  let units = system.execution_units()?
  var collected = collect_from_root(root, uname.machine, units.page_size_bytes, units.clock_ticks_per_second, selected, true, true)?
  if requested(selected, "network") {
    let network = collect_network(root, collected.pci.functions, collected.usb.devices)?
    collected = {
      ...collected,
      network: {
        status: network.status,
        links: network.links,
        routes: network.routes,
        rules: network.rules,
      },
      issues: collected.issues.extend(network.issues),
    }
  }
  collected = {
    ...collected,
    source_mode: report.LiveLinux,
    identity: {...collected.identity, architecture: uname.machine},
  }
  if !sensitive {
    collected = report.redact_report(collected)
  }
  return collected
}
