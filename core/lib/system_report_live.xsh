##! Collects a bounded, process-visible Linux report from one rooted source tree.
use system_report as report
use system_report_collect as collectors

error SystemReportUsbDescriptorError = Invalid(message: Str)

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
    producer: {
      name: "system-report",
      version: "1",
    },
    source_mode: report.SyntheticFixture,
    collection_started_unix_ms: started,
    collection_ended_unix_ms: null,
    elapsed_ms: null,
    scope: {
      platform: "Linux",
      host_claim: "process-visible Linux sources; physical-host completeness is unverified",
      source_roots: [
        "/proc",
        "/sys",
        "/etc",
      ],
      mount_namespace: empty_text(report.NotRequested),
      network_namespace: empty_text(report.NotRequested),
      pid_namespace: empty_text(report.NotRequested),
      cgroup_namespace: empty_text(report.NotRequested),
      uts_namespace: empty_text(report.NotRequested),
      ipc_namespace: empty_text(report.NotRequested),
      user_namespace: empty_text(report.NotRequested),
      time_namespace: empty_text(report.NotRequested),
      visible_cgroup: empty_text(report.NotRequested),
      page_size_bytes: page_size_bytes,
      clock_ticks_per_second: clock_ticks_per_second,
      ancestors_may_be_hidden: true,
    },
    redacted: false,
    identity: {
      status: empty_status(report.SectionNotRequested),
      kernel_release: null,
      kernel_build: null,
      architecture: null,
      os_release: null,
      hostname: empty_text(report.NotRequested),
      uptime_seconds: null,
      boot_id: empty_text(report.NotRequested),
      firmware: null,
    },
    cpu: empty_cpu(report.SectionNotRequested),
    memory: empty_memory(report.SectionNotRequested),
    pci: {
      status: empty_status(report.SectionNotRequested),
      functions: [],
    },
    usb: {
      status: empty_status(report.SectionNotRequested),
      devices: [],
    },
    storage: {
      status: empty_status(report.SectionNotRequested),
      devices: [],
      mounts: [],
    },
    network: {
      status: empty_status(report.SectionNotRequested),
      links: [],
      routes: [],
      rules: [],
    },
    sensors: {
      status: empty_status(report.SectionNotRequested),
      channels: [],
      thermal_zones: [],
    },
    power: {
      status: empty_status(report.SectionNotRequested),
      supplies: [],
      cap_zones: [],
    },
    firmware: {
      status: empty_status(report.SectionNotRequested),
      source: "not-requested",
      records: [],
      limitation: empty_text(report.NotRequested),
    },
    kernel: {
      status: empty_status(report.SectionNotRequested),
      command_line: empty_text(report.NotRequested),
      modules: [],
      parameters: [],
      sysctls: [],
    },
    processes: {
      status: empty_status(report.SectionNotRequested),
      processes: [],
    },
    devices: {
      status: empty_status(report.SectionNotRequested),
      devices: [],
    },
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
    detail: {
      state: report.Observed,
      value: detail,
      raw_bytes_base64: null,
    },
  }
}

proc read_value(root: FsRoot, source_path: Path, max_bytes: Int = 65536) [fs, error] -> collectors.SourceRead {
  collectors.read_source_text(root, source_path, max_bytes)
}

type DeviceTreeStringRead = {source: collectors.SourceRead, values: List[Str]}

proc read_device_tree_strings(
  root: FsRoot,
  source_path: Path,
  max_bytes: Int,
  single: Bool,
) [fs, error] -> DeviceTreeStringRead {
  let source = collectors.read_source_text(root, source_path, max_bytes, true)
  if source.observation.state != report.Observed or source.observation.value == null {
    return {source: source, values: []}
  }

  let decoded = collectors.decode_device_tree_strings(source.observation.value ?? "")
  if decoded == null {
    return {
      source: {
        ...source,
        observation: {
          ...source.observation,
          state: report.Malformed,
          value: null,
        },
        error_kind: "invalid_device_tree_strings",
      },
      values: [],
    }
  }

  let values = decoded ?? []
  if single and values.len() != 1 {
    return {
      source: {
        ...source,
        observation: {
          ...source.observation,
          state: report.Malformed,
          value: null,
        },
        error_kind: "invalid_device_tree_model",
      },
      values: [],
    }
  }

  if single {
    return {source: {...source, observation: {...source.observation, value: values[0]}}, values: values}
  }

  return {source: source, values: values}
}

pure observed_source_text(source: collectors.SourceRead) -> Str? {
  if source.observation.state == report.Observed {
    return source.observation.value
  }

  return null
}

pure parse_integer(value: Str?) -> Int? {
  if value == null {
    return null
  }

  let source_text = (value ?? "").trim()
  let signed_digits = source_text.starts_with("-") and decimal_identifier((source_text.split("") |> drop(1)).join(""))
  if ! decimal_identifier(source_text) and ! signed_digits {
    return null
  }

  match source_text.parse_int() {
    Ok(parsed) => return parsed
    Err(_) => return null
  }
}

pure parse_bool01(value: Str?) -> Bool? {
  let parsed = parse_integer(value)
  if parsed == null {
    return null
  }

  if parsed == 0 {
    return false
  }

  if parsed == 1 {
    return true
  }

  return null
}

pure parse_words(value: Str?) -> List[Str] {
  if value == null or value == "" {
    return []
  }

  return value.split(" ") |> where .trim() != ""
}

type HugePageRead = {pool: report.HugePagePool?, issues: List[report.CollectionIssue]}

pure parse_list(value: Str?) -> List[Int] {
  if value == null or value == "" {
    return []
  }

  match report.parse_cpu_list(value ?? "") {
    Ok(parsed) => return parsed
    Err(_) => return []
  }
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

type CpuInfoRead = {infos: List[CpuInfo], state: report.ObservationState, errno: Int?, error_kind: Str?}

pure empty_cpu_info(id: Int) -> CpuInfo {
  return {
    id: id,
    vendor: null,
    model: null,
    family: null,
    model_id: null,
    stepping: null,
    features: [],
  }
}

proc read_cpu_info(root: FsRoot) [fs, error] -> CpuInfoRead {
  let source = read_value(root, p"proc/cpuinfo", max_bytes: 8388608)
  if source.observation.state != report.Observed or source.observation.value == null {
    return {infos: [], state: source.observation.state, errno: source.errno, error_kind: source.error_kind}
  }

  var values: List[CpuInfo] = []
  var current = empty_cpu_info(-1)
  var has_current = false
  for line in source.observation.value.lines() {
    if line.trim() == "" {
      if has_current {
        values = values.push(current)
        has_current = false
      }

      continue
    }

    let pair = line.split(":", maxsplit: 1)
    continue when pair.len() != 2
    let key = pair[0].trim()
    let value = pair[1].trim()
    if key == "processor" or key == "processor number" {
      if has_current {
        values = values.push(current)
      }

      current = empty_cpu_info(parse_integer(value) ?? -1)
      has_current = true
      continue
    }

    continue unless has_current
    match key {
      "vendor_id" | "CPU implementer" => current = {...current, vendor: value}
      "model name" | "Processor" | "Hardware" => current = {...current, model: value}
      "cpu family" | "CPU architecture" => current = {...current, family: value}
      "model" | "CPU part" => current = {...current, model_id: value}
      "stepping" => current = {...current, stepping: value}
      "flags" | "Features" => current = {...current, features: parse_words(value)}
      _ => {}
    }
  }

  if has_current {
    values = values.push(current)
  }

  return {
    infos: values
      |> where .id >= 0
      |> sort-by .id,
    state: report.Observed,
    errno: null,
    error_kind: null,
  }
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

type CgroupMountInventory = {mounts: List[collectors.CgroupMount], has_v1: Bool, malformed: Bool}

pure cgroup_mount_inventory(value: Str) -> CgroupMountInventory {
  var mounts: List[collectors.CgroupMount] = []
  var has_v1 = false
  var malformed = false
  for line in value.lines() {
    let fields = parse_words(line)
    var separator = 0
    while separator < fields.len() and fields[separator] != "-" {
      separator += 1
    }

    if separator == fields.len() {
      malformed = true
      continue
    }

    if separator < 6 or separator + 3 >= fields.len() {
      malformed = true
      continue
    }

    let filesystem = fields[separator + 1]
    if filesystem in ["cgroup2", "cgroup"] {
      let mount_root = decode_mount_field(fields[3])
      let mount_point = decode_mount_field(fields[4])
      if ! mount_root.starts_with("/") or ! mount_point.starts_with("/") {
        malformed = true
        continue
      }

      if filesystem == "cgroup2" {
        mounts = mounts.push({root: mount_root, point: mount_point})
      } else {
        has_v1 = true
      }
    }
  }

  return {mounts: mounts, has_v1: has_v1, malformed: malformed}
}

proc read_effective_cgroup_cpuset(root: FsRoot) [fs, error] -> CpuSetRead {
  let membership = read_value(root, p"proc/self/cgroup", max_bytes: 65536)
  let mounts = read_value(root, p"proc/self/mountinfo", max_bytes: 4194304)
  if mounts.observation.state != report.Observed {
    return {cpus: [], state: mounts.observation.state, error_kind: mounts.error_kind, errno: mounts.errno}
  }

  if membership.observation.state != report.Observed {
    return {cpus: [], state: membership.observation.state, error_kind: membership.error_kind, errno: membership.errno}
  }

  let parsed_membership = collectors.parse_unified_cgroup_path(membership.observation.value ?? "")
  if parsed_membership.state == report.Malformed {
    return {cpus: [], state: report.Malformed, error_kind: "invalid_cgroup_membership", errno: null}
  }

  let group_path = parsed_membership.path
  let inventory = cgroup_mount_inventory(mounts.observation.value ?? "")
  if inventory.malformed {
    return {cpus: [], state: report.Malformed, error_kind: "invalid_cgroup_mountinfo", errno: null}
  }

  let has_v1 = parsed_membership.has_v1 or inventory.has_v1
  if group_path == null or inventory.mounts.len() == 0 {
    return {
      cpus: [],
      state: if has_v1 { report.Unsupported } else { report.Absent },
      error_kind: if has_v1 { "cgroup_v1_or_hybrid" } else { "cgroup_v2_mount_unavailable" },
      errno: null,
    }
  }

  let group_name = group_path ?? ""
  var selected: collectors.CgroupMount? = null
  match collectors.select_cgroup_mount(group_name, inventory.mounts) {
    Ok(mount) => selected = mount
    Err(_) => return {cpus: [], state: report.Malformed, error_kind: "invalid_cgroup_mount_path", errno: null}
  }

  if selected == null {
    return {cpus: [], state: report.Unsupported, error_kind: "cgroup_path_outside_visible_mount", errno: null}
  }

  let chosen = selected ?? {root: "", point: ""}
  let root_path = chosen.root
  let target = chosen.point
  if ! group_name.starts_with("/") {
    return {cpus: [], state: report.Malformed, error_kind: "invalid_cgroup_mount_path", errno: null}
  }

  var relative = ""
  if root_path == "/" {
    relative = (group_name.split("") |> drop(1)).join("")
  } else if group_name == root_path {
    relative = ""
  } else if group_name.starts_with(f"${root_path}/") {
    relative = (group_name.split("") |> drop(root_path.count_chars() + 1)).join("")
  }

  let mount_relative = (target.split("/") |> where .trim() != "").join("/")
  let source_path = if relative == "" {
    if mount_relative == "" { p"." } else { fp"${mount_relative}" }
  } else if mount_relative == "" {
    fp"${relative}"
  } else {
    fp"${mount_relative}/${relative}"
  }
  let source = read_value(root, fp"${source_path}/cpuset.cpus.effective", max_bytes: 65536)
  if source.observation.state != report.Observed {
    return {cpus: [], state: source.observation.state, error_kind: source.error_kind, errno: source.errno}
  }

  if source.observation.value == "" {
    return {cpus: [], state: report.Observed, error_kind: null, errno: null}
  }

  match report.parse_cpu_list(source.observation.value ?? "") {
    Ok(cpus) => return {cpus: cpus, state: report.Observed, error_kind: null, errno: null}
    Err(_) => return {cpus: [], state: report.Malformed, error_kind: "invalid_effective_cpuset", errno: null}
  }
}

## Retains the configuration that owns each parsed USB interface setting.
export type UsbDescriptorAlternate = {
  configuration_value: Int?,
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

type PowerCapSource = {path: Path, name: collectors.SourceRead}

## Retains a route-netlink snapshot with its enumeration status and issues.
export type NetworkCollection = {
  status: report.SectionStatus,
  links: List[report.NetworkLink],
  routes: List[report.NetworkRoute],
  rules: List[report.NetworkRule],
  issues: List[report.CollectionIssue],
}

## Separates a USB bus entry's controller relation from source-read failures.
export type UsbControllerObservation = {
  address: Str?,
  state: report.ObservationState,
  errno: Int?,
  error_kind: Str?,
}

## Keeps a class device's parent target and source failure state together.
export type ClassParentObservation = {
  target: Path?,
  state: report.ObservationState,
  errno: Int?,
  error_kind: Str?,
}

type NetworkLinkTarget = {
  target: Path?,
  issues: List[report.CollectionIssue],
}

## Retains process identity and memory fields parsed from one stat record.
export type ProcStatFieldIssue = {field: Str, state: report.ObservationState}

## Keeps complete stat identity while allowing individual resource fields to be unavailable.
export type ProcStat = {
  pid: Int,
  parent_pid: Int,
  command: Str,
  state: Str,
  thread_count: Int?,
  start_ticks: Int,
  virtual_bytes: Int?,
  resident_pages: Int?,
  field_issues: List[ProcStatFieldIssue],
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

## Retains valid SMBIOS records alongside malformed-record issues.
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
  return value.replace("\\040", " ")
    .replace("\\011", "\t")
    .replace("\\012", "\n")
    .replace("\\134", "\\")
}

pure mount_usage_eligible(filesystem: Str) -> Bool {
  return filesystem in [
    "btrfs",
    "exfat",
    "ext2",
    "ext3",
    "ext4",
    "f2fs",
    "ntfs",
    "ntfs3",
    "overlay",
    "tmpfs",
    "vfat",
    "xfs",
  ]
}

type MountUsageRow = {mount_id: Int, parent_id: Int, target: Str, filesystem: Str}

type MountUsageIndex = {rows: List[MountUsageRow], by_id: Map[Int], target_counts: Map[Int], valid_graph: Bool}

pure mount_usage_index(mountinfo: Str) -> MountUsageIndex {
  var rows: List[MountUsageRow] = []
  var by_id: Map[Int] = {}
  var target_counts: Map[Int] = {}
  var valid_graph = true
  for line in mountinfo.lines() {
    let fields = parse_words(line)
    var separator = 0
    while separator < fields.len() and fields[separator] != "-" {
      separator += 1
    }

    if separator < 6 or separator + 3 >= fields.len() {
      valid_graph = false
      continue
    }

    let mount_id = parse_integer(fields[0]) ?? -1
    let parent_id = parse_integer(fields[1]) ?? -1
    if mount_id < 0 or parent_id < 0 or mount_id > 9007199254740991 or parent_id > 9007199254740991 {
      valid_graph = false
      continue
    }

    let numbers = fields[2].split(":")
    let major = parse_integer(numbers.get(0, "")) ?? -1
    let minor = parse_integer(numbers.get(1, "")) ?? -1
    let target = decode_mount_field(fields[4])
    if major < 0 or minor < 0 or major > 9007199254740991 or minor > 9007199254740991 or ! target.starts_with("/") {
      valid_graph = false
      continue
    }

    let id_key = f"${mount_id}"
    if by_id.has(id_key) {
      valid_graph = false
    }

    by_id = by_id.set(id_key, if by_id.has(id_key) { -1 } else { rows.len() })
    target_counts = target_counts.set(target, target_counts.get(target, 0) + 1)
    rows = rows.push({
      mount_id: mount_id,
      parent_id: parent_id,
      target: target,
      filesystem: fields[separator + 1],
    })
  }

  return {rows: rows, by_id: by_id, target_counts: target_counts, valid_graph: valid_graph}
}

# A target can resolve through a different mount when its own or an ancestor path is shadowed.
pure mount_usage_safe(index: MountUsageIndex, mount_id: Int) -> Bool {
  if ! index.valid_graph {
    return false
  }

  var current_id = mount_id
  var seen = set.empty()
  var depth = 0
  while depth < index.rows.len() {
    let key = f"${current_id}"
    if set.has(seen, key) {
      return false
    }

    seen = set.add(seen, key)
    let row_index = index.by_id.get(key, -1)
    if row_index < 0 {
      return false
    }

    let entry = index.rows[row_index]
    if ! mount_usage_eligible(entry.filesystem) or index.target_counts.get(entry.target, 0) != 1 {
      return false
    }

    if entry.parent_id == 0 {
      return true
    }

    if entry.parent_id == current_id {
      return false
    }

    if ! index.by_id.has(f"${entry.parent_id}") {
      return true
    }

    current_id = entry.parent_id
    depth += 1
  }

  return false
}

pure decimal_identifier(value: Str) -> Bool {
  if value == "" {
    return false
  }

  for digit in value.split("") {
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
    ] {
      return false
    }
  }

  return true
}

pure proc_stat_error(message: Str) -> Error {
  return report.SystemReportError.InvalidProcStat(message:)
}

type ProcOptionalNumber = {value: Int?, issue_state: report.ObservationState?}

pure proc_optional_number(raw: Str, positive: Bool = false) -> ProcOptionalNumber {
  let parsed = parse_integer(raw)
  if parsed == null {
    return {value: null, issue_state: if decimal_identifier(raw) { report.RangeFailure } else { report.Malformed }}
  }

  let number = parsed ?? -1
  if number < 0 or positive and number == 0 {
    return {value: null, issue_state: report.Malformed}
  }

  if number > 9007199254740991 {
    return {value: null, issue_state: report.RangeFailure}
  }

  return {value: number, issue_state: null}
}

## Parses a process stat record while preserving the command's parentheses.
export pure parse_proc_stat(text: Str) -> Result[ProcStat] {
  let pieces = text.trim().split(") ")
  if pieces.len() < 2 {
    return Err(proc_stat_error("process stat record has no command terminator"))
  }

  let prefix = pieces[0].split(" (", maxsplit: 1)
  if prefix.len() != 2 {
    return Err(proc_stat_error("process stat record has invalid PID and command fields"))
  }

  let pid = parse_integer(prefix[0]) ?? -1
  if pid <= 0 or pid > 9007199254740991 {
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

  let parent_pid = parse_integer(fields[1]) ?? -1
  let start_ticks = parse_integer(fields[19]) ?? -1
  if parent_pid < 0 or start_ticks < 0 or parent_pid > 9007199254740991 or start_ticks > 9007199254740991 {
    return Err(proc_stat_error("process stat record has invalid parent or start identity"))
  }

  let thread_count = proc_optional_number(fields[17], positive: true)
  let virtual_bytes = proc_optional_number(fields[20])
  let resident_pages = proc_optional_number(fields[21])
  var field_issues = [
    {field: value.field, state: value.issue_state ?? report.Malformed}
    for value in [
      {
        field: "thread_count",
        issue_state: thread_count.issue_state,
      },
      {
        field: "virtual_bytes",
        issue_state: virtual_bytes.issue_state,
      },
      {
        field: "resident_pages",
        issue_state: resident_pages.issue_state,
      },
    ]
    if value.issue_state != null
  ]
  return Ok({
    pid: pid,
    parent_pid: parent_pid,
    command: command,
    state: fields[0],
    thread_count: thread_count.value,
    start_ticks: start_ticks,
    virtual_bytes: virtual_bytes.value,
    resident_pages: resident_pages.value,
    field_issues: field_issues,
  })
}

pure split_csv(value: Str) -> List[Str] {
  if value == "" {
    return []
  }

  return value.split(",")
}

pure block_index(indices: Map[Int], major: Int, minor: Int) -> Int? {
  let key = f"${major}:${minor}"
  if ! indices.has(key) {
    return null
  }

  return indices.get(key, 0)
}

pure block_name_index(indices: Map[Int], name: Str?) -> Int? {
  if name == null or ! indices.has(name ?? "") {
    return null
  }

  return indices.get(name ?? "", 0)
}

## Finds the last PCI function in a sysfs class-entry symlink target.
export pure pci_address_in_target(target: Path) -> Str? {
  var address: Str? = null
  for component in target.display().split("/") {
    match collectors.parse_pci_address(component) {
      Ok(_) => address = component
      Err(_) => {}
    }
  }

  return address
}

proc collect_storage(
  root: FsRoot,
  pci_functions: List[report.PciFunction],
  include_local_mount_usage: Bool,
) [fs, error] -> StorageCollection {
  let listing = fs.root_children(root, p"sys/class/block", max_entries: 4096)?
  let pci_indices = pci_function_indices(pci_functions)
  var issues: List[report.CollectionIssue] = []
  var candidates: List[BlockCandidate] = []
  var listed_names = set.empty()
  for device_path in listing.children {
    listed_names = set.add(listed_names, device_path.name())
  }

  if listing.state != "complete" {
    issues = issues.push(
      issue("storage", "devices", live_source_observation_state(listing.state, false), listing.error_kind, listing.errno),
    )
  }

  for device_path in listing.children {
    let name = device_path.name()
    let dev = read_value(root, fp"${device_path}/dev", max_bytes: 4096)
    let dev_text = observed_source_text(dev)
    let dev_parts = (dev_text ?? "").split(":")
    var major: Int? = null
    var minor: Int? = null
    if dev_text == null {
      issues = issues.push(
        issue("storage", f"devices.${name}.major_minor", dev.observation.state, dev.error_kind, dev.errno),
      )
    } else if dev_parts.len() != 2 {
      issues = issues.push(
        issue("storage", f"devices.${name}.major_minor", report.Malformed, "invalid_device_number", null),
      )
    } else {
      let parsed_major = parse_integer(dev_parts[0]) ?? -1
      let parsed_minor = parse_integer(dev_parts[1]) ?? -1
      if parsed_major < 0 or parsed_minor < 0 {
        let out_of_range = parsed_major < 0 and decimal_identifier(dev_parts[0]) or parsed_minor < 0 and decimal_identifier(
          dev_parts[1],
        )
        let state = if out_of_range { report.RangeFailure } else { report.Malformed }
        let error_kind = if out_of_range { "device_number_out_of_range" } else { "invalid_device_number" }
        issues = issues.push(issue("storage", f"devices.${name}.major_minor", state, error_kind, null))
      } else if parsed_major > 9007199254740991 or parsed_minor > 9007199254740991 {
        issues = issues.push(
          issue("storage", f"devices.${name}.major_minor", report.RangeFailure, "device_number_out_of_range", null),
        )
      } else {
        major = parsed_major
        minor = parsed_minor
      }
    }

    let size = read_value(root, fp"${device_path}/size", max_bytes: 4096)
    let logical = read_value(root, fp"${device_path}/queue/logical_block_size", max_bytes: 4096)
    let physical = read_value(root, fp"${device_path}/queue/physical_block_size", max_bytes: 4096)
    let removable = read_value(root, fp"${device_path}/removable", max_bytes: 4096)
    let rotational = read_value(root, fp"${device_path}/queue/rotational", max_bytes: 4096)
    let read_only = read_value(root, fp"${device_path}/ro", max_bytes: 4096)
    let model = read_value(root, fp"${device_path}/device/model", max_bytes: 4096)
    let firmware = read_value(root, fp"${device_path}/device/firmware_rev", max_bytes: 4096)
    let fallback_firmware = if firmware.observation.state == report.Absent {
      read_value(root, fp"${device_path}/device/rev", max_bytes: 4096)
    } else {
      firmware
    }
    for field_source in [
      {
        field: "model",
        source: model,
      },
      {
        field: "firmware",
        source: fallback_firmware,
      },
    ] {
      let observed = field_source.source
      if observed.observation.state != report.Observed and observed.observation.state != report.Absent {
        issues = issues.push(
          issue(
            "storage",
            f"devices.${name}.${field_source.field}",
            observed.observation.state,
            observed.error_kind,
            observed.errno,
          ),
        )
      }
    }

    let scheduler = read_value(root, fp"${device_path}/queue/scheduler", max_bytes: 4096)
    var active_scheduler: Str? = null
    var available_schedulers: List[Str] = []
    if scheduler.observation.state != report.Observed and scheduler.observation.state != report.Absent {
      issues = issues.push(
        issue("storage", f"devices.${name}.scheduler", scheduler.observation.state, scheduler.error_kind, scheduler.errno),
      )
    }

    if scheduler.observation.state == report.Observed {
      let parsed_scheduler = collectors.parse_block_scheduler(observed_source_text(scheduler) ?? "")
      if parsed_scheduler == null {
        issues = issues.push(
          issue("storage", f"devices.${name}.scheduler", report.Malformed, "invalid_scheduler_selection", null),
        )
      } else {
        active_scheduler = parsed_scheduler.active
        available_schedulers = parsed_scheduler.available
      }
    }

    let read_ahead = read_value(root, fp"${device_path}/queue/read_ahead_kb", max_bytes: 4096)
    let discard_granularity = read_value(root, fp"${device_path}/queue/discard_granularity", max_bytes: 4096)
    let discard_max = read_value(root, fp"${device_path}/queue/discard_max_bytes", max_bytes: 4096)
    let logical_number = collectors.bounded_number(logical, true)
    let physical_number = collectors.bounded_number(physical, true)
    let removable_number = collectors.bounded_number(removable, true)
    let rotational_number = collectors.bounded_number(rotational, true)
    let read_only_number = collectors.bounded_number(read_only, true)
    let read_ahead_number = collectors.bounded_number(read_ahead, true)
    let discard_granularity_number = collectors.bounded_number(discard_granularity, true)
    let discard_max_number = collectors.bounded_number(discard_max, true)
    for field_number in [
      {
        field: "logical_sector_bytes",
        number: logical_number,
        boolean: false,
      },
      {
        field: "physical_sector_bytes",
        number: physical_number,
        boolean: false,
      },
      {
        field: "removable",
        number: removable_number,
        boolean: true,
      },
      {
        field: "rotational",
        number: rotational_number,
        boolean: true,
      },
      {
        field: "read_only",
        number: read_only_number,
        boolean: true,
      },
      {
        field: "read_ahead_kb",
        number: read_ahead_number,
        boolean: false,
      },
      {
        field: "discard_granularity_bytes",
        number: discard_granularity_number,
        boolean: false,
      },
      {
        field: "discard_max_bytes",
        number: discard_max_number,
        boolean: false,
      },
    ] {
      let parsed = field_number.number
      let field = f"devices.${name}.${field_number.field}"
      if parsed.state != null {
        issues = issues.push(issue("storage", field, parsed.state ?? report.Malformed, parsed.error_kind, parsed.errno))
      } else if field_number.boolean and (parsed.value ?? -1) not in [0, 1] and parsed.value != null {
        issues = issues.push(issue("storage", field, report.Malformed, "invalid_boolean", null))
      }
    }

    let stats = read_value(root, fp"${device_path}/stat", max_bytes: 4096)
    if stats.observation.state != report.Observed and stats.observation.state != report.Absent {
      issues = issues.push(
        issue("storage", f"devices.${name}.stat", stats.observation.state, stats.error_kind, stats.errno),
      )
    }

    let stats_values = parse_words(observed_source_text(stats))
    let stat_field_count_valid = stats_values.len() in [11, 15, 17] or stats_values.len() > 17
    if stats.observation.state == report.Observed and ! stat_field_count_valid {
      issues = issues.push(
        issue("storage", f"devices.${name}.stat", report.Malformed, "invalid_io_counter_count", null),
      )
    }

    var io_counters: List[report.MemoryCounter] = []
    let counter_names = [
      "read_ios",
      "read_merges",
      "read_sectors",
      "read_ms",
      "write_ios",
      "write_merges",
      "write_sectors",
      "write_ms",
      "in_flight",
      "io_ms",
      "weighted_io_ms",
      "discard_ios",
      "discard_merges",
      "discard_sectors",
      "discard_ms",
      "flush_ios",
      "flush_ms",
    ]
    let counter_units = [
      "requests",
      "requests",
      "sectors",
      "milliseconds",
      "requests",
      "requests",
      "sectors",
      "milliseconds",
      "requests",
      "milliseconds",
      "milliseconds",
      "requests",
      "requests",
      "sectors",
      "milliseconds",
      "requests",
      "milliseconds",
    ]
    var counter_index = 0
    while stat_field_count_valid and counter_index < stats_values.len() and counter_index < counter_names.len() {
      let counter_value = parse_integer(stats_values[counter_index]) ?? -1
      if counter_value >= 0 and counter_value <= 9007199254740991 {
        io_counters = io_counters.push(
          {name: counter_names[counter_index], value: counter_value, unit: counter_units[counter_index]},
        )
      } else {
        let out_of_range = decimal_identifier(stats_values[counter_index])
        let state = if out_of_range { report.RangeFailure } else { report.Malformed }
        let error_kind = if out_of_range { "io_counter_out_of_range" } else { "invalid_io_counter" }
        issues = issues.push(
          issue("storage", f"devices.${name}.stat.${counter_names[counter_index]}", state, error_kind, null),
        )
      }

      counter_index += 1
    }

    if stats_values.len() > counter_names.len() {
      issues = issues.push(
        issue("storage", f"devices.${name}.stat", report.Unsupported, "unknown_io_counter_fields", null),
      )
    }

    let size_number = collectors.bounded_number(size, true)
    let sectors = size_number.value ?? -1
    if size.observation.state == report.Absent {
      issues = issues.push(issue("storage", f"devices.${name}.size", report.Absent, size.error_kind, size.errno))
    } else if size_number.state != null {
      issues = issues.push(
        issue(
          "storage",
          f"devices.${name}.size",
          size_number.state ?? report.Malformed,
          size_number.error_kind,
          size_number.errno,
        ),
      )
    } else if sectors > 17592186044415 {
      issues = issues.push(issue("storage", f"devices.${name}.size", report.RangeFailure, "byte_count_overflow", null))
    }

    var is_partition = false
    match fs.root_exists(root, fp"${device_path}/partition") {
      Ok(present) => is_partition = present
      Err(is PermissionDenied) => issues = issues.push(
        issue("storage", f"devices.${name}.partition", report.PermissionDenied, "permission_denied", null),
      )
      Err(_) => issues = issues.push(
        issue("storage", f"devices.${name}.partition", report.ReadFailure, "partition_probe_failed", null),
      )
    }

    let target_source = class_entry_target(root, device_path)
    let target_path = target_source.target
    if target_source.state != report.Observed {
      issues = issues.push(
        issue("storage", f"devices.${name}.sysfs_target", target_source.state, target_source.error_kind, target_source.errno),
      )
    }

    let target = if target_path == null { "" } else { target_path.display() }
    let kind = if is_partition {
      "partition"
    } else if target_path == null {
      "unknown"
    } else if "/virtual/" in target {
      "virtual"
    } else {
      "disk"
    }
    var parent_pci_function_index: Int? = null
    if target_path != null {
      parent_pci_function_index = pci_function_index(pci_indices, pci_address_in_target(target_path))
    }

    let holders_listing = fs.root_children(root, fp"${device_path}/holders", max_entries: 4096)?
    let slaves_listing = fs.root_children(root, fp"${device_path}/slaves", max_entries: 4096)?
    if holders_listing.state != "complete" {
      issues = issues.push(
        issue(
          "storage",
          f"devices.${name}.holders",
          live_source_observation_state(holders_listing.state, false),
          holders_listing.error_kind,
          holders_listing.errno,
        ),
      )
    }

    # Partitions expose holders but have no slaves directory of their own.
    if slaves_listing.state != "complete" and ! (is_partition and slaves_listing.state == "absent") {
      issues = issues.push(
        issue(
          "storage",
          f"devices.${name}.slaves",
          live_source_observation_state(slaves_listing.state, false),
          slaves_listing.error_kind,
          slaves_listing.errno,
        ),
      )
    }

    var holders: List[Str] = []
    var slaves: List[Str] = []
    for holder in holders_listing.children {
      holders = holders.push(holder.name())
    }

    for slave in slaves_listing.children {
      slaves = slaves.push(slave.name())
    }

    var parent_name: Str? = null
    let target_components = target.split("/")
    for component in target_components {
      if component != name and component != "block" and set.has(listed_names, component) {
        parent_name = component
      }
    }

    var sector_bytes: Int? = null
    if sectors >= 0 and sectors <= 17592186044415 {
      sector_bytes = sectors * 512
    }

    candidates = candidates.push(
      {
        device: {
          name: name,
          major: major,
          minor: minor,
          kind: kind,
          size_bytes: sector_bytes,
          logical_sector_bytes: logical_number.value,
          physical_sector_bytes: physical_number.value,
          removable: if (removable_number.value ?? -1) in [
            0,
            1,
          ] {
            parse_bool01(observed_source_text(removable))
          } else {
            null
          },
          rotational: if (rotational_number.value ?? -1) in [
            0,
            1,
          ] {
            parse_bool01(observed_source_text(rotational))
          } else {
            null
          },
          read_only: if (read_only_number.value ?? -1) in [
            0,
            1,
          ] {
            parse_bool01(observed_source_text(read_only))
          } else {
            null
          },
          model: {
            ...model.observation,
            value: observed_source_text(model),
          },
          firmware: {
            ...fallback_firmware.observation,
            value: observed_source_text(fallback_firmware),
          },
          parent_device_index: null,
          parent_pci_function_index: parent_pci_function_index,
          holder_indices: [],
          slave_indices: [],
          active_scheduler: active_scheduler,
          available_schedulers: available_schedulers,
          read_ahead_kb: read_ahead_number.value,
          discard_granularity_bytes: discard_granularity_number.value,
          discard_max_bytes: discard_max_number.value,
          io_counters: io_counters,
        },
        parent_name: parent_name,
        holders: holders,
        slaves: slaves,
      },
    )
  }

  # Preserve the first enumerated identity while linking layered devices and mounts.
  var block_indices_by_name: Map[Int] = {}
  var block_indices_by_device: Map[Int] = {}
  for index in range(candidates.len()) {
    let device = candidates[index].device
    if device.name != null and ! block_indices_by_name.has(device.name ?? "") {
      block_indices_by_name = block_indices_by_name.set(device.name ?? "", index)
    }

    if device.major != null and device.minor != null {
      let key = f"${device.major ?? 0}:${device.minor ?? 0}"
      if ! block_indices_by_device.has(key) {
        block_indices_by_device = block_indices_by_device.set(key, index)
      }
    }
  }

  var linked_devices: List[report.BlockDevice] = []
  var candidate_index = 0
  while candidate_index < candidates.len() {
    let candidate = candidates[candidate_index]
    var holders: List[Int] = []
    var slaves: List[Int] = []
    for name in candidate.holders {
      let index = block_name_index(block_indices_by_name, name)
      if index != null {
        holders = holders.push(index)
      }
    }

    for name in candidate.slaves {
      let index = block_name_index(block_indices_by_name, name)
      if index != null {
        slaves = slaves.push(index)
      }
    }

    linked_devices = linked_devices.push({
      ...candidate.device,
      parent_device_index: block_name_index(block_indices_by_name, candidate.parent_name),
      holder_indices: holders,
      slave_indices: slaves,
    })
    candidate_index += 1
  }

  let mount_source = read_value(root, p"proc/self/mountinfo", max_bytes: 4194304)
  var mounts: List[report.Mount] = []
  if mount_source.observation.state != report.Observed or mount_source.observation.value == null {
    issues = issues.push(
      issue("storage", "mounts", mount_source.observation.state, mount_source.error_kind, mount_source.errno),
    )
  } else {
    let usage_index = mount_usage_index(mount_source.observation.value ?? "")
    for line_item in mount_source.observation.value.lines() |> enumerate() {
      let line_index = line_item.index
      let line = line_item.value
      let fields = parse_words(line)
      var separator = 0
      while separator < fields.len() and fields[separator] != "-" {
        separator += 1
      }

      if separator < 6 or separator + 3 >= fields.len() {
        issues = issues.push(
          issue("storage", f"mounts.line.${line_index}", report.Malformed, "invalid_mountinfo_row", null),
        )
        continue
      }

      let ids = parse_integer(fields[0]) ?? -1
      let parent_id = parse_integer(fields[1]) ?? -1
      let device_ids = fields[2].split(":")
      let major = parse_integer(device_ids.get(0, "")) ?? -1
      let minor = parse_integer(device_ids.get(1, "")) ?? -1
      if ids < 0 or parent_id < 0 or major < 0 or minor < 0 {
        issues = issues.push(
          issue("storage", f"mounts.line.${line_index}", report.Malformed, "invalid_mount_identity", null),
        )
        continue
      }

      if ids > 9007199254740991 or parent_id > 9007199254740991 or major > 9007199254740991 or minor > 9007199254740991 {
        issues = issues.push(
          issue("storage", f"mounts.line.${line_index}", report.RangeFailure, "mount_identity_out_of_json_range", null),
        )
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
      if include_local_mount_usage and mount_usage_safe(usage_index, ids) {
        if ! target.starts_with("/") {
          usage_state = report.Malformed
          issues = issues.push(
            issue_with_detail(
              "storage",
              f"mounts.${ids}.usage",
              usage_state,
              "invalid_mount_target",
              null,
              "The mount target was not absolute.",
            ),
          )
        } else {
          var usage_path: Path? = null
          if target == "/" {
            usage_path = p"."
          } else {
            match fp"${target}".strip_prefix(/) {
              Ok(relative_path) => usage_path = relative_path
              Err(_) => usage_path = null
            }
          }

          if usage_path == null {
            usage_state = report.Malformed
            issues = issues.push(
              issue_with_detail(
                "storage",
                f"mounts.${ids}.usage",
                usage_state,
                "invalid_mount_target",
                null,
                "The mount target could not be made relative to the observation root.",
              ),
            )
          } else {
            let usage = fs.root_filesystem_stats(root, usage_path)?
            usage_state = match usage.state { "observed" => report.Observed, "absent" => report.Disappeared, "permission_denied" => report.PermissionDenied, "malformed" => report.Malformed, "range_failure" => report.RangeFailure, _ => report.ReadFailure }
            usage_total_bytes = usage.total_bytes
            usage_used_bytes = usage.used_bytes
            usage_available_bytes = usage.available_bytes
            if usage.state != "observed" {
              issues = issues.push(
                issue(
                  "storage",
                  f"mounts.${ids}.usage",
                  usage_state,
                  usage.error_kind,
                  usage.errno,
                ),
              )
            }
          }
        }
      }

      mounts = mounts.push({
        mount_id: ids,
        parent_id: parent_id,
        major: major,
        minor: minor,
        root: {
          state: report.Observed,
          value: decode_mount_field(fields[3]),
          raw_bytes_base64: null,
        },
        target: {
          state: report.Observed,
          value: target,
          raw_bytes_base64: null,
        },
        mount_options: report.sanitize_mount_options(split_csv(fields[5])),
        optional_fields: report.sanitize_mount_optional_fields(optional_fields),
        filesystem: fields[separator + 1],
        source: report.sanitize_mount_source({state: report.Observed, value: source, raw_bytes_base64: null}),
        super_options: report.sanitize_mount_options(split_csv(fields[separator + 3])),
        block_device_index: block_index(block_indices_by_device, major, minor),
        usage_state: usage_state,
        usage_total_bytes: usage_total_bytes,
        usage_used_bytes: usage_used_bytes,
        usage_available_bytes: usage_available_bytes,
      })
    }
  }

  var state = report.Complete
  if listing.state == "absent" and mount_source.observation.state == report.Absent {
    state = report.SectionAbsent
  } else if listing.state != "complete" or mount_source.observation.state != report.Observed or issues.len() > 0 {
    state = report.Partial
  }

  return {
    status: {
      state: state,
      enumeration_succeeded: listing.enumeration_succeeded and mount_source.observation.state == report.Observed,
    },
    devices: linked_devices,
    mounts: mounts,
    issues: issues,
  }
}

pure sensor_kind(channel: Str) -> Str? {
  for spec in [
    {
      prefix: "temp",
      kind: "temperature",
    },
    {
      prefix: "in",
      kind: "voltage",
    },
    {
      prefix: "fan",
      kind: "fan",
    },
    {
      prefix: "power",
      kind: "power",
    },
    {
      prefix: "energy",
      kind: "energy",
    },
    {
      prefix: "curr",
      kind: "current",
    },
  ] {
    if channel.starts_with(spec.prefix) {
      let suffix = (channel.split("") |> drop(spec.prefix.count_chars())).join("")
      if decimal_identifier(suffix) {
        return spec.kind
      }
    }
  }

  return null
}

pure sensor_unit(kind: Str) -> Str {
  match kind {
    "temperature" => return "millidegrees_celsius"
    "voltage" => return "millivolts"
    "fan" => return "rpm"
    "power" => return "microwatts"
    "energy" => return "microjoules"
    "current" => return "milliamps"
    _ => return "raw"
  }
}

pure append_number_issue(
  issues: List[report.CollectionIssue],
  section: Str,
  field: Str,
  number: collectors.BoundedNumber,
) -> List[report.CollectionIssue] {
  if number.state == null {
    return issues
  }

  return issues.push(issue(section, field, number.state ?? report.Malformed, number.error_kind, number.errno))
}

pure append_text_issue(
  issues: List[report.CollectionIssue],
  section: Str,
  field: Str,
  source: collectors.SourceRead,
) -> List[report.CollectionIssue] {
  let state = source.observation.state
  if state == report.Observed or state == report.Absent {
    return issues
  }

  return issues.push(issue(section, field, state, source.error_kind, source.errno))
}

proc collect_sensors(
  root: FsRoot,
  pci_functions: List[report.PciFunction],
  usb_devices: List[report.UsbDevice],
) [fs, error] -> SensorCollection {
  let pci_indices = pci_function_indices(pci_functions)
  let usb_indices = usb_device_indices(usb_devices)
  let hwmon_listing = fs.root_children(root, p"sys/class/hwmon", max_entries: 1024)?
  let thermal_listing = fs.root_children(root, p"sys/class/thermal", max_entries: 1024)?
  var channels: List[report.SensorChannel] = []
  var zones: List[report.ThermalZone] = []
  var issues: List[report.CollectionIssue] = []
  if hwmon_listing.state != "complete" and hwmon_listing.state != "absent" {
    issues = issues.push(
      issue(
        "sensors",
        "hwmon",
        live_source_observation_state(hwmon_listing.state, false),
        hwmon_listing.error_kind,
        hwmon_listing.errno,
      ),
    )
  }

  for chip_path in hwmon_listing.children {
    continue unless chip_path.name().starts_with("hwmon")
    let chip_name_source = read_value(root, fp"${chip_path}/name", max_bytes: 4096)
    issues = append_text_issue(issues, "sensors", f"hwmon.${chip_path.name()}.name", chip_name_source)
    let chip = observed_source_text(chip_name_source) ?? chip_path.name()
    let parent = class_parent_target(root, chip_path)
    if parent.state != report.Observed {
      issues = issues.push(
        issue("sensors", f"hwmon.${chip_path.name()}.parent", parent.state, parent.error_kind, parent.errno),
      )
    }

    var parent_pci_address: Str? = null
    var parent_usb_index: Int? = null
    if parent.target != null {
      let target = parent.target ?? p""
      parent_pci_address = usb_parent_address(target)
      parent_usb_index = usb_device_index_from_target(usb_indices, target)
    }

    let attribute_listing = fs.root_children(root, chip_path, max_entries: 1024)?
    if attribute_listing.state != "complete" {
      issues = issues.push(
        issue(
          "sensors",
          f"hwmon.${chip_path.name()}.attributes",
          live_source_observation_state(attribute_listing.state, false),
          attribute_listing.error_kind,
          attribute_listing.errno,
        ),
      )
    }

    for attribute in attribute_listing.children {
      let attribute_name = attribute.name()
      continue unless attribute_name.ends_with("_input")
      let channel_name = (attribute_name.split("") |> take(attribute_name.count_chars() - 6)).join("")
      let kind = sensor_kind(channel_name)
      let sensor_kind_name = kind ?? "unknown"
      let value = read_value(root, attribute, max_bytes: 4096)
      let label = read_value(root, fp"${chip_path}/${channel_name}_label", max_bytes: 4096)
      let minimum = read_value(root, fp"${chip_path}/${channel_name}_min", max_bytes: 4096)
      let maximum = read_value(root, fp"${chip_path}/${channel_name}_max", max_bytes: 4096)
      let critical = read_value(root, fp"${chip_path}/${channel_name}_crit", max_bytes: 4096)
      let alarm = read_value(root, fp"${chip_path}/${channel_name}_alarm", max_bytes: 4096)
      issues = append_text_issue(issues, "sensors", f"hwmon.${chip_path.name()}.${channel_name}_label", label)
      let measured = collectors.bounded_number(value, false)
      let minimum_number = collectors.bounded_number(minimum, false)
      let maximum_number = collectors.bounded_number(maximum, false)
      let critical_number = collectors.bounded_number(critical, false)
      let alarm_number = collectors.bounded_number(alarm, true)
      issues = append_number_issue(issues, "sensors", f"hwmon.${chip_path.name()}.${attribute_name}", measured)
      issues = append_number_issue(issues, "sensors", f"hwmon.${chip_path.name()}.${channel_name}_min", minimum_number)
      issues = append_number_issue(issues, "sensors", f"hwmon.${chip_path.name()}.${channel_name}_max", maximum_number)
      issues = append_number_issue(
        issues,
        "sensors",
        f"hwmon.${chip_path.name()}.${channel_name}_crit",
        critical_number,
      )
      issues = append_number_issue(issues, "sensors", f"hwmon.${chip_path.name()}.${channel_name}_alarm", alarm_number)
      var alarm_value: Bool? = null
      if alarm_number.value != null {
        if alarm_number.value == 0 {
          alarm_value = false
        } else if alarm_number.value == 1 {
          alarm_value = true
        } else {
          issues = issues.push(
            issue("sensors", f"hwmon.${chip_path.name()}.${channel_name}_alarm", report.Malformed, "invalid_boolean", null),
          )
        }
      }

      channels = channels.push({
        chip: chip,
        chip_entry_name: chip_path.name(),
        channel: channel_name,
        label: label.observation,
        kind: sensor_kind_name,
        value: measured.value,
        unit: sensor_unit(sensor_kind_name),
        minimum: minimum_number.value,
        maximum: maximum_number.value,
        critical: critical_number.value,
        alarm: alarm_value,
        parent_device_class_index: null,
        parent_pci_function_index: pci_function_index(pci_indices, parent_pci_address),
        parent_usb_device_index: parent_usb_index,
      })
    }
  }

  if thermal_listing.state != "complete" and thermal_listing.state != "absent" {
    issues = issues.push(
      issue(
        "sensors",
        "thermal_zones",
        live_source_observation_state(thermal_listing.state, false),
        thermal_listing.error_kind,
        thermal_listing.errno,
      ),
    )
  }

  for zone_path in thermal_listing.children {
    continue unless zone_path.name().starts_with("thermal_zone")
    let id = parse_integer((zone_path.name().split("") |> drop("thermal_zone".count_chars())).join("")) ?? -1
    if id < 0 or id > 9007199254740991 or zone_path.name() != f"thermal_zone${id}" {
      issues = issues.push(
        issue("sensors", f"thermal_zones.${zone_path.name()}", report.Malformed, "invalid_thermal_zone_id", null),
      )
      continue
    }

    let kind = read_value(root, fp"${zone_path}/type", max_bytes: 4096)
    issues = append_text_issue(issues, "sensors", f"thermal_zones.${zone_path.name()}.type", kind)
    let temperature = read_value(root, fp"${zone_path}/temp", max_bytes: 4096)
    let temperature_number = collectors.bounded_number(temperature, false)
    issues = append_number_issue(issues, "sensors", f"thermal_zones.${zone_path.name()}.temp", temperature_number)
    let attributes = fs.root_children(root, zone_path, max_entries: 256)?
    if attributes.state != "complete" {
      issues = issues.push(
        issue(
          "sensors",
          f"thermal_zones.${zone_path.name()}.attributes",
          live_source_observation_state(attributes.state, false),
          attributes.error_kind,
          attributes.errno,
        ),
      )
    }

    var trip_indices: List[Int] = []
    var seen_trip_indices = set.empty()
    for attribute in attributes.children {
      let attribute_name = attribute.name()
      continue when ! attribute_name.starts_with("trip_point_") or ! attribute_name.ends_with("_temp")
      let trip_number = parse_integer(attribute_name.split("_").get(2, "")) ?? -1
      if trip_number < 0 or trip_number > 9007199254740991 or attribute_name != f"trip_point_${trip_number}_temp" or set.has(
        seen_trip_indices,
        f"${trip_number}",
      ) {
        issues = issues.push(
          issue(
            "sensors",
            f"thermal_zones.${zone_path.name()}.${attribute_name}",
            report.Malformed,
            "invalid_thermal_trip_index",
            null,
          ),
        )
        continue
      }

      seen_trip_indices = set.add(seen_trip_indices, f"${trip_number}")
      trip_indices = trip_indices.push(trip_number)
    }

    var trips: List[report.ThermalTrip] = []
    for trip_number in trip_indices |> sort-by . {
      let trip_temp = read_value(root, fp"${zone_path}/trip_point_${trip_number}_temp", max_bytes: 4096)
      let trip_type = read_value(root, fp"${zone_path}/trip_point_${trip_number}_type", max_bytes: 4096)
      issues = append_text_issue(
        issues,
        "sensors",
        f"thermal_zones.${zone_path.name()}.trip_point_${trip_number}_type",
        trip_type,
      )
      let hysteresis = read_value(root, fp"${zone_path}/trip_point_${trip_number}_hyst", max_bytes: 4096)
      let trip_number_value = collectors.bounded_number(trip_temp, false)
      let hysteresis_number = collectors.bounded_number(hysteresis, true)
      issues = append_number_issue(
        issues,
        "sensors",
        f"thermal_zones.${zone_path.name()}.trip_point_${trip_number}_temp",
        trip_number_value,
      )
      issues = append_number_issue(
        issues,
        "sensors",
        f"thermal_zones.${zone_path.name()}.trip_point_${trip_number}_hyst",
        hysteresis_number,
      )
      trips = trips.push({
        index: trip_number,
        kind: observed_source_text(trip_type) ?? "unknown",
        temperature_millidegrees: trip_number_value.value,
        hysteresis_millidegrees: hysteresis_number.value,
      })
    }

    zones = zones.push({
      id: id,
      kind: observed_source_text(kind),
      temperature_millidegrees: temperature_number.value,
      trips: trips,
      parent_device_class_index: null,
    })
  }

  var state = report.Complete
  if hwmon_listing.state == "absent" and thermal_listing.state == "absent" {
    state = report.SectionAbsent
  } else if issues.len() > 0 or hwmon_listing.state != "complete" and hwmon_listing.state != "absent" or thermal_listing.state != "complete" and thermal_listing.state != "absent" {
    state = report.Partial
  }

  return {
    status: {
      state: state,
      enumeration_succeeded: (hwmon_listing.enumeration_succeeded or hwmon_listing.state == "absent") and (thermal_listing.enumeration_succeeded or thermal_listing.state == "absent"),
    },
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
    issues = issues.push(
      issue(
        "power",
        "supplies",
        live_source_observation_state(supply_listing.state, false),
        supply_listing.error_kind,
        supply_listing.errno,
      ),
    )
  }

  if cap_listing.state != "complete" and cap_listing.state != "absent" {
    issues = issues.push(
      issue(
        "power",
        "cap_zones",
        live_source_observation_state(cap_listing.state, false),
        cap_listing.error_kind,
        cap_listing.errno,
      ),
    )
  }

  for supply_path in supply_listing.children {
    let kind = read_value(root, fp"${supply_path}/type", max_bytes: 4096)
    let status = read_value(root, fp"${supply_path}/status", max_bytes: 4096)
    let health = read_value(root, fp"${supply_path}/health", max_bytes: 4096)
    let capacity = read_value(root, fp"${supply_path}/capacity", max_bytes: 4096)
    let energy_now = read_value(root, fp"${supply_path}/energy_now", max_bytes: 4096)
    let energy_full = read_value(root, fp"${supply_path}/energy_full", max_bytes: 4096)
    let charge_now = read_value(root, fp"${supply_path}/charge_now", max_bytes: 4096)
    let charge_full = read_value(root, fp"${supply_path}/charge_full", max_bytes: 4096)
    let voltage = read_value(root, fp"${supply_path}/voltage_now", max_bytes: 4096)
    let current = read_value(root, fp"${supply_path}/current_now", max_bytes: 4096)
    let cycles = read_value(root, fp"${supply_path}/cycle_count", max_bytes: 4096)
    let capacity_number = collectors.bounded_number(capacity, true)
    let energy_now_number = collectors.bounded_number(energy_now, true)
    let energy_full_number = collectors.bounded_number(energy_full, true)
    let charge_now_number = collectors.bounded_number(charge_now, true)
    let charge_full_number = collectors.bounded_number(charge_full, true)
    let voltage_number = collectors.bounded_number(voltage, true)
    let current_number = collectors.bounded_number(current, false)
    let cycles_number = collectors.bounded_number(cycles, true)
    let prefix = f"supplies.${supply_path.name()}"
    issues = append_text_issue(issues, "power", f"${prefix}.type", kind)
    issues = append_text_issue(issues, "power", f"${prefix}.status", status)
    issues = append_text_issue(issues, "power", f"${prefix}.health", health)
    issues = append_number_issue(issues, "power", f"${prefix}.capacity", capacity_number)
    issues = append_number_issue(issues, "power", f"${prefix}.energy_now", energy_now_number)
    issues = append_number_issue(issues, "power", f"${prefix}.energy_full", energy_full_number)
    issues = append_number_issue(issues, "power", f"${prefix}.charge_now", charge_now_number)
    issues = append_number_issue(issues, "power", f"${prefix}.charge_full", charge_full_number)
    issues = append_number_issue(issues, "power", f"${prefix}.voltage_now", voltage_number)
    issues = append_number_issue(issues, "power", f"${prefix}.current_now", current_number)
    issues = append_number_issue(issues, "power", f"${prefix}.cycle_count", cycles_number)
    let capacity_out_of_range = capacity_number.value != null and (capacity_number.value ?? 0) > 100
    let capacity_value: Int? = if capacity_out_of_range { null } else { capacity_number.value }
    if capacity_out_of_range {
      issues = issues.push(issue("power", f"${prefix}.capacity", report.Malformed, "percent_out_of_range", null))
    }

    supplies = supplies.push({
      name: supply_path.name(),
      kind: observed_source_text(kind),
      status: observed_source_text(status),
      health: observed_source_text(health),
      capacity_percent: capacity_value,
      energy_now_uwh: energy_now_number.value,
      energy_full_uwh: energy_full_number.value,
      charge_now_uah: charge_now_number.value,
      charge_full_uah: charge_full_number.value,
      voltage_now_uv: voltage_number.value,
      current_now_ua: current_number.value,
      cycle_count: cycles_number.value,
      parent_device_class_index: null,
    })
  }

  var cap_paths: List[PowerCapSource] = []
  for item in cap_listing.children {
    let name = read_value(root, fp"${item}/name", max_bytes: 4096)
    continue when name.observation.state == report.Absent
    issues = append_text_issue(issues, "power", f"cap_zones.${item.name()}.name", name)
    cap_paths = cap_paths.push({path: item, name: name})
  }

  for cap_source in cap_paths {
    let zone_path = cap_source.path
    let name = cap_source.name
    let energy = read_value(root, fp"${zone_path}/energy_uj", max_bytes: 4096)
    let range = read_value(root, fp"${zone_path}/max_energy_range_uj", max_bytes: 4096)
    let energy_number = collectors.bounded_number(energy, true)
    let range_number = collectors.bounded_number(range, true)
    issues = append_number_issue(issues, "power", f"cap_zones.${zone_path.name()}.energy_uj", energy_number)
    issues = append_number_issue(issues, "power", f"cap_zones.${zone_path.name()}.max_energy_range_uj", range_number)
    let cap_attributes = fs.root_children(root, zone_path, max_entries: 256)?
    if cap_attributes.state != "complete" {
      issues = issues.push(
        issue(
          "power",
          f"cap_zones.${zone_path.name()}.attributes",
          live_source_observation_state(cap_attributes.state, false),
          cap_attributes.error_kind,
          cap_attributes.errno,
        ),
      )
    }

    var constraints: List[report.PowerCapConstraint] = []
    for attribute in cap_attributes.children {
      continue when ! attribute.name().starts_with("constraint_") or ! attribute.name().ends_with("_power_limit_uw")
      let index_text = attribute.name().split("_").get(1, "")
      let index_number = parse_integer(index_text)
      let parsed_index = index_number ?? -1
      if ! decimal_identifier(index_text) or index_number == null or parsed_index > 9007199254740991 or index_text != f"${parsed_index}" {
        issues = issues.push(
          issue("power", f"cap_zones.${zone_path.name()}.${attribute.name()}", report.Malformed, "invalid_constraint_index", null),
        )
        continue
      }

      let index = parsed_index
      let limit = read_value(root, attribute, max_bytes: 4096)
      let limit_number = collectors.bounded_number(limit, true)
      issues = append_number_issue(issues, "power", f"cap_zones.${zone_path.name()}.${attribute.name()}", limit_number)
      let name_source = read_value(root, fp"${zone_path}/constraint_${index}_name", max_bytes: 4096)
      let window = read_value(root, fp"${zone_path}/constraint_${index}_time_window_us", max_bytes: 4096)
      let window_number = collectors.bounded_number(window, true)
      issues = append_text_issue(
        issues,
        "power",
        f"cap_zones.${zone_path.name()}.constraint_${index}_name",
        name_source,
      )
      issues = append_number_issue(
        issues,
        "power",
        f"cap_zones.${zone_path.name()}.constraint_${index}_time_window_us",
        window_number,
      )
      let constraint_name = observed_source_text(name_source)
      let observed_constraint: report.PowerCapConstraint = {
        index: index,
        name: constraint_name,
        power_limit_uw: limit_number.value,
        time_window_us: window_number.value,
      }
      var ordered: List[report.PowerCapConstraint] = []
      var inserted = false
      for existing in constraints {
        if ! inserted and index < existing.index {
          ordered = ordered.push(observed_constraint)
          inserted = true
        }

        ordered = ordered.push(existing)
      }

      if ! inserted {
        ordered = ordered.push(observed_constraint)
      }

      constraints = ordered
    }

    var parent_name: Str? = null
    let link = fs.root_readlink_result(root, zone_path)?
    if link.state == "observed" {
      let candidate = (link.target ?? p"").parent().name()
      if cap_paths |> any .path.name() == candidate {
        parent_name = candidate
      }
    } else if link.state != "read_failure" or link.errno != 22 {
      issues = issues.push(
        issue(
          "power",
          f"cap_zones.${zone_path.name()}.parent",
          live_source_observation_state(link.state, false),
          link.error_kind,
          link.errno,
        ),
      )
    }

    cap_zones = cap_zones.push({
      entry_name: zone_path.name(),
      name: observed_source_text(name) ?? zone_path.name(),
      parent: parent_name,
      energy_uj: energy_number.value,
      maximum_energy_range_uj: range_number.value,
      constraints: constraints,
    })
  }

  var state = report.Complete
  if supply_listing.state == "absent" and cap_listing.state == "absent" {
    state = report.SectionAbsent
  } else if issues.len() > 0 {
    state = report.Partial
  }

  return {
    status: {
      state: state,
      enumeration_succeeded: (supply_listing.enumeration_succeeded or supply_listing.state == "absent") and (cap_listing.enumeration_succeeded or cap_listing.state == "absent"),
    },
    supplies: supplies,
    cap_zones: cap_zones,
    issues: issues,
  }
}

pure checked_page_bytes(pages: Int?, page_size_bytes: Int) -> Int? {
  let page_count = pages ?? -1
  if page_count < 0 or page_size_bytes <= 0 {
    return null
  }

  if page_count > 9007199254740991 / page_size_bytes {
    return null
  }

  return page_count * page_size_bytes
}

pure process_cgroup_resource_index(
  cgroup_path: report.TextObservation,
  resources: List[report.CgroupResource],
) -> Int? {
  if cgroup_path.value == null {
    return null
  }

  var index = 0
  for resource in resources {
    if resource.path.value == cgroup_path.value {
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
  [{
    ...process_item,
    cgroup_resource_index: process_cgroup_resource_index(process_item.cgroup, resources),
  } for process_item in processes]
}

type ProcessRead = {
  processes: List[report.ProcessRecord],
  issues: List[report.CollectionIssue],
}

proc read_process(root: FsRoot, process_path: Path, pid: Int, page_size_bytes: Int) [fs, error] -> ProcessRead {
  var issues: List[report.CollectionIssue] = []
  let stat_before = read_value(root, fp"${process_path}/stat", max_bytes: 16384)
  if stat_before.observation.state != report.Observed or stat_before.observation.value == null {
    issues = issues.push(
      issue("processes", f"${pid}.stat", stat_before.observation.state, stat_before.error_kind, stat_before.errno),
    )
    return {processes: [], issues: issues}
  }

  guard let stat = parse_proc_stat(stat_before.observation.value ?? "") else |_| {
    issues = issues.push(issue("processes", f"${pid}.stat", report.Malformed, "invalid_process_stat", null))
    return {processes: [], issues: issues}
  }

  if stat.pid != pid {
    issues = issues.push(issue("processes", f"${pid}.stat", report.Raced, "pid_changed_during_read", null))
    return {processes: [], issues: issues}
  }

  for field_issue in stat.field_issues {
    issues = issues.push(
      issue("processes", f"${pid}.stat.${field_issue.field}", field_issue.state, "invalid_process_stat_field", null),
    )
  }

  let statm = read_value(root, fp"${process_path}/statm", max_bytes: 4096)
  let status = read_value(root, fp"${process_path}/status", max_bytes: 16384)
  let cgroup = read_value(root, fp"${process_path}/cgroup", max_bytes: 16384)
  var process_cgroup = cgroup.observation
  if cgroup.observation.state != report.Observed {
    process_cgroup = {...process_cgroup, value: null}
  } else if cgroup.observation.value != null {
    let parsed_cgroup = collectors.parse_unified_cgroup_path(cgroup.observation.value ?? "")
    if parsed_cgroup.path == null {
      let error_kind = if parsed_cgroup.state == report.Malformed {
        "invalid_process_cgroup"
      } else {
        "unified_cgroup_path_unavailable"
      }
      process_cgroup = {state: parsed_cgroup.state, value: null, raw_bytes_base64: null}
      issues = issues.push(issue("processes", f"${pid}.cgroup", parsed_cgroup.state, error_kind, null))
    } else {
      process_cgroup = {state: report.Observed, value: parsed_cgroup.path, raw_bytes_base64: null}
    }
  }

  let stat_after = read_value(root, fp"${process_path}/stat", max_bytes: 16384)
  if stat_after.observation.state != report.Observed or stat_after.observation.value == null {
    let state = if stat_after.observation.state == report.Absent {
      report.Disappeared
    } else {
      stat_after.observation.state
    }
    issues = issues.push(issue("processes", f"${pid}.stat", state, stat_after.error_kind, stat_after.errno))
    return {processes: [], issues: issues}
  }

  let final_stat = parse_proc_stat(stat_after.observation.value ?? "")
  let same_start = match final_stat {
    Ok(after) => after.pid == pid and after.start_ticks == stat.start_ticks,
    Err(_) => false,
  }
  if ! same_start {
    issues = issues.push(issue("processes", f"${pid}.stat", report.Raced, "pid_start_identity_changed", null))
    return {processes: [], issues: issues}
  }

  let statm_observed = statm.observation.state == report.Observed
  let statm_fields = if statm_observed { parse_words(statm.observation.value) } else { [] }
  var statm_fields_complete = statm_fields.len() == 7
  if statm_fields_complete {
    for field in statm_fields |> drop(2) {
      if ! decimal_identifier(field) {
        statm_fields_complete = false
        break
      }
    }
  }

  let virtual_pages = proc_optional_number(if statm_fields_complete { statm_fields[0] } else { "" })
  let resident_pages = proc_optional_number(if statm_fields_complete { statm_fields[1] } else { "" })
  let virtual_bytes = if statm_observed { checked_page_bytes(virtual_pages.value, page_size_bytes) } else { null }
  let resident_count = if statm_observed { resident_pages.value } else { stat.resident_pages }
  let resident_bytes = checked_page_bytes(resident_count, page_size_bytes)
  if statm_observed and ! statm_fields_complete {
    issues = issues.push(issue("processes", f"${pid}.statm", report.Malformed, "invalid_statm_row", null))
  } else if statm_observed {
    for value in [
      {
        field: "virtual_bytes",
        parsed: virtual_pages,
        scaled: virtual_bytes,
      },
      {
        field: "resident_bytes",
        parsed: resident_pages,
        scaled: resident_bytes,
      },
    ] {
      if value.parsed.issue_state != null {
        issues = issues.push(
          issue(
            "processes",
            f"${pid}.statm.${value.field}",
            value.parsed.issue_state ?? report.Malformed,
            "invalid_page_count",
            null,
          ),
        )
      } else if value.scaled == null {
        issues = issues.push(
          issue("processes", f"${pid}.statm.${value.field}", report.RangeFailure, "page_count_overflow", null),
        )
      }
    }
  } else if stat.resident_pages != null and resident_bytes == null {
    issues = issues.push(
      issue("processes", f"${pid}.stat.resident_bytes", report.RangeFailure, "page_count_overflow", null),
    )
  }

  var uid: Int? = null
  if status.observation.state == report.Observed and status.observation.value != null {
    var uid_rows = 0
    var uid_issue: report.ObservationState? = null
    for line in status.observation.value.lines() {
      if line.starts_with("Uid:") {
        uid_rows += 1
        let fields = parse_words(line.split(":", maxsplit: 1).get(1, "").replace("\t", " "))
        if fields.len() != 4 {
          uid_issue = report.Malformed
          continue
        }

        var values: List[Int] = []
        for field in fields {
          let parsed = proc_optional_number(field)
          if parsed.issue_state != null {
            uid_issue = parsed.issue_state
          } else {
            values = values.push(parsed.value ?? 0)
          }
        }

        if uid_issue == null and uid_rows == 1 {
          uid = values[0]
        }
      }
    }

    if uid_rows != 1 {
      uid = null
      uid_issue = report.Malformed
    }

    if uid_issue != null {
      uid = null
      let error_kind = if uid_rows == 0 { "numeric_uid_unavailable" } else { "invalid_numeric_uid" }
      issues = issues.push(issue("processes", f"${pid}.uid", uid_issue, error_kind, null))
    }
  } else if status.observation.state == report.Observed {
    issues = issues.push(issue("processes", f"${pid}.uid", report.Malformed, "numeric_uid_unavailable", null))
  }

  if statm.observation.state != report.Observed {
    issues = issues.push(issue("processes", f"${pid}.statm", statm.observation.state, statm.error_kind, statm.errno))
  }

  if status.observation.state != report.Observed {
    issues = issues.push(
      issue("processes", f"${pid}.status", status.observation.state, status.error_kind, status.errno),
    )
  }

  if cgroup.observation.state != report.Observed {
    issues = issues.push(
      issue("processes", f"${pid}.cgroup", cgroup.observation.state, cgroup.error_kind, cgroup.errno),
    )
  }

  let observed_virtual_bytes = if statm_observed { virtual_bytes } else { stat.virtual_bytes }
  let process_item: report.ProcessRecord = {
    pid: stat.pid,
    parent_pid: stat.parent_pid,
    uid: uid,
    command: {
      state: report.Observed,
      value: stat.command,
      raw_bytes_base64: null,
    },
    state: stat.state,
    start_ticks: stat.start_ticks,
    thread_count: stat.thread_count,
    resident_bytes: resident_bytes,
    virtual_bytes: observed_virtual_bytes,
    cgroup: process_cgroup,
    cgroup_resource_index: null,
  }
  return {processes: [process_item], issues: issues}
}

proc collect_processes(root: FsRoot, page_size_bytes: Int) [fs, error] -> ProcessCollection {
  let listing = fs.root_children(root, p"proc", max_entries: 8192)?
  var processes: List[report.ProcessRecord] = []
  var issues: List[report.CollectionIssue] = []
  if listing.state != "complete" {
    issues = issues.push(
      issue(
        "processes",
        "enumeration",
        live_source_observation_state(listing.state, false),
        listing.error_kind,
        listing.errno,
      ),
    )
  }

  for process_path in listing.children {
    let pid_text = process_path.name()
    continue unless decimal_identifier(pid_text)
    let pid = parse_integer(pid_text) ?? -1
    if pid <= 0 or pid > 9007199254740991 {
      issues = issues.push(issue("processes", f"${pid_text}.pid", report.RangeFailure, "invalid_pid", null))
      continue
    }

    let process_read = read_process(root, process_path, pid, page_size_bytes)
    issues = issues.extend(process_read.issues)
    processes = processes.extend(process_read.processes)
  }

  var state = report.Complete
  if listing.state == "truncated" {
    state = report.SectionTruncated
  } else if listing.state == "absent" {
    state = report.SectionAbsent
  } else if listing.state != "complete" or issues.len() > 0 {
    state = report.Partial
  }

  return {
    status: {
      state: state,
      enumeration_succeeded: listing.enumeration_succeeded,
    },
    processes: processes |> sort-by .pid,
    issues: issues,
  }
}

proc collect_kernel(root: FsRoot) [fs, error] -> KernelCollection {
  let command_line = collectors.read_source_text(root, p"proc/cmdline", 65536, true)
  let source = read_value(root, p"proc/modules", max_bytes: 1048576)
  var issues: List[report.CollectionIssue] = []
  var modules: List[report.KernelModule] = []
  var seen_modules = set.empty()
  if command_line.observation.state != report.Observed {
    issues = issues.push(
      issue("kernel", "command_line", command_line.observation.state, command_line.error_kind, command_line.errno),
    )
  }

  if source.observation.state != report.Observed or source.observation.value == null {
    issues = issues.push(issue("kernel", "modules", source.observation.state, source.error_kind, source.errno))
  } else {
    for line_item in source.observation.value.lines() |> enumerate() {
      let columns = parse_words(line_item.value)

      # A tainted module has one additional flag word after the address.
      if columns.len() not in [6, 7] {
        issues = issues.push(
          issue("kernel", f"modules.line.${line_item.index}", report.Malformed, "invalid_module_row", null),
        )
        continue
      }

      let size = parse_integer(columns[1]) ?? -1
      let users_number = parse_integer(columns[2]) ?? -1
      let users_unavailable = columns[2] == "-"
      if size < 0 or users_number < 0 and ! users_unavailable {
        let out_of_range = size < 0 and decimal_identifier(columns[1]) or users_number < 0 and decimal_identifier(
          columns[2],
        )
        let state = if out_of_range { report.RangeFailure } else { report.Malformed }
        let error_kind = if out_of_range { "module_numeric_out_of_range" } else { "invalid_module_numeric_field" }
        issues = issues.push(issue("kernel", f"modules.line.${line_item.index}", state, error_kind, null))
        continue
      }

      if size > 9007199254740991 or users_number > 9007199254740991 {
        issues = issues.push(
          issue("kernel", f"modules.line.${line_item.index}", report.RangeFailure, "module_numeric_out_of_range", null),
        )
        continue
      }

      if set.has(seen_modules, columns[0]) {
        issues = issues.push(
          issue("kernel", f"modules.line.${line_item.index}", report.Malformed, "duplicate_module_name", null),
        )
        continue
      }

      seen_modules = set.add(seen_modules, columns[0])
      let users: Int? = if users_unavailable { null } else { users_number }
      modules = modules.push({name: columns[0], size_bytes: size, users: users, state: columns[4]})
    }
  }

  var sysctls: List[report.KernelParameter] = []
  for named_source in [
    {
      name: "kernel.pid_max",
      source_path: p"proc/sys/kernel/pid_max",
    },
    {
      name: "kernel.threads-max",
      source_path: p"proc/sys/kernel/threads-max",
    },
    {
      name: "vm.swappiness",
      source_path: p"proc/sys/vm/swappiness",
    },
    {
      name: "vm.overcommit_memory",
      source_path: p"proc/sys/vm/overcommit_memory",
    },
    {
      name: "net.ipv4.ip_forward",
      source_path: p"proc/sys/net/ipv4/ip_forward",
    },
    {
      name: "net.ipv6.conf.all.forwarding",
      source_path: p"proc/sys/net/ipv6/conf/all/forwarding",
    },
  ] {
    let name = named_source.name
    let source_path = named_source.source_path
    let value = read_value(root, source_path, max_bytes: 4096)
    sysctls = sysctls.push({name: name, value: value.observation})
    if value.observation.state != report.Observed and value.observation.state != report.Absent {
      issues = issues.push(issue("kernel", f"sysctl.${name}", value.observation.state, value.error_kind, value.errno))
    }
  }

  var parameters: List[report.KernelParameter] = []
  for named_source in [
    {
      name: "usbcore.autosuspend",
      source_path: p"sys/module/usbcore/parameters/autosuspend",
    },
    {
      name: "nvme_core.default_ps_max_latency_us",
      source_path: p"sys/module/nvme_core/parameters/default_ps_max_latency_us",
    },
    {
      name: "intel_pstate.no_turbo",
      source_path: p"sys/module/intel_pstate/parameters/no_turbo",
    },
  ] {
    let name = named_source.name
    let source_path = named_source.source_path
    let value = read_value(root, source_path, max_bytes: 4096)
    parameters = parameters.push({name: name, value: value.observation})
    if value.observation.state != report.Observed and value.observation.state != report.Absent {
      issues = issues.push(
        issue("kernel", f"parameters.${name}", value.observation.state, value.error_kind, value.errno),
      )
    }
  }

  var state = if source.observation.state == report.Observed and command_line.observation.state == report.Observed {
    report.Complete
  } else {
    report.Partial
  }
  if issues.len() > 0 {
    state = report.Partial
  }

  return {
    status: {
      state: state,
      enumeration_succeeded: source.observation.state == report.Observed,
    },
    command_line: command_line.observation,
    modules: modules |> sort-by .name,
    parameters: parameters,
    sysctls: sysctls,
    issues: issues,
  }
}

pure usb_device_index_from_target(indices: Map[Int], target: Path) -> Int? {
  for component in target.display().split("/") {
    let candidate = component.split(":").get(0, "")
    if indices.has(candidate) {
      return indices.get(candidate, 0)
    }
  }

  return null
}

proc collect_device_classes(
  root: FsRoot,
  pci_functions: List[report.PciFunction],
  usb_devices: List[report.UsbDevice],
) [fs, error] -> DeviceCollection {
  let pci_indices = pci_function_indices(pci_functions)
  let usb_indices = usb_device_indices(usb_devices)
  var devices: List[report.DeviceClassRecord] = []
  var issues: List[report.CollectionIssue] = []
  var available_classes = 0
  var enumerated_classes = 0
  for class_source in [
    {
      name: "drm",
      source_path: p"sys/class/drm",
    },
    {
      name: "sound",
      source_path: p"sys/class/sound",
    },
    {
      name: "input",
      source_path: p"sys/class/input",
    },
  ] {
    let class_name = class_source.name
    let source_path = class_source.source_path
    let listing = fs.root_children(root, source_path, max_entries: 4096)?
    if listing.state != "absent" {
      available_classes += 1
    }

    if listing.enumeration_succeeded or listing.state == "absent" {
      enumerated_classes += 1
    } else {
      issues = issues.push(
        issue(
          "devices",
          f"${class_name}.enumeration",
          live_source_observation_state(listing.state, false),
          listing.error_kind,
          listing.errno,
        ),
      )
    }

    for entry in listing.children {
      let entry_name = entry.name()
      var name = entry_name
      if class_name == "input" or class_name == "sound" {
        let name_field = if class_name == "input" { "name" } else { "id" }
        let name_file = read_value(root, fp"${entry}/${name_field}", max_bytes: 4096)
        let observed_name = observed_source_text(name_file)
        if observed_name != null {
          name = observed_name
        } else {
          issues = append_text_issue(issues, "devices", f"${class_name}.${entry_name}.${name_field}", name_file)
        }
      }

      let driver_link = optional_driver_name(root, fp"${entry}/device/driver")
      if driver_link.observation.state != report.Observed and driver_link.observation.state != report.Absent {
        issues = issues.push(
          issue(
            "devices",
            f"${class_name}.${entry_name}.driver",
            driver_link.observation.state,
            driver_link.error_kind,
            driver_link.errno,
          ),
        )
      }

      let parent = class_parent_target(root, entry)
      if parent.state != report.Observed {
        issues = issues.push(
          issue("devices", f"${class_name}.${entry_name}.parent", parent.state, parent.error_kind, parent.errno),
        )
      }

      var parent_pci_address: Str? = null
      var parent_usb_device_index: Int? = null
      if parent.target != null {
        let parent_target = parent.target ?? p""
        parent_pci_address = usb_parent_address(parent_target)
        parent_usb_device_index = usb_device_index_from_target(usb_indices, parent_target)
      }

      var attributes: List[report.KernelParameter] = []
      let allowlisted_attributes = match class_name {
        "drm" => ["status", "enabled", "modes"],
        "sound" => ["number"],
        "input" => [],
        _ => [],
      }
      for attribute_name in allowlisted_attributes {
        let attribute = read_value(root, fp"${entry}/${attribute_name}", max_bytes: 16384)
        if attribute.observation.state == report.Observed {
          attributes = attributes.push({name: attribute_name, value: attribute.observation})
        } else {
          issues = append_text_issue(issues, "devices", f"${class_name}.${entry_name}.${attribute_name}", attribute)
        }
      }

      devices = devices.push({
        class: class_name,
        entry_name: {
          state: report.Observed,
          value: entry_name,
          raw_bytes_base64: null,
        },
        name: {
          state: report.Observed,
          value: name,
          raw_bytes_base64: null,
        },
        parent_device_class_index: null,
        parent_pci_function_index: pci_function_index(pci_indices, parent_pci_address),
        parent_usb_device_index: parent_usb_device_index,
        driver: observed_source_text(driver_link),
        attributes: attributes,
      })
    }
  }

  var state = report.Complete
  if available_classes == 0 {
    state = report.SectionAbsent
  } else if enumerated_classes < 3 or issues.len() > 0 {
    state = report.Partial
  }

  return {
    status: {
      state: state,
      enumeration_succeeded: enumerated_classes == 3,
    },
    devices: devices,
    issues: issues,
  }
}

proc collect_firmware(root: FsRoot) [fs, error] -> FirmwareCollection {
  let source = fs.root_read_result(root, p"sys/firmware/dmi/tables/DMI", max_bytes: 1048576)?
  var records: List[report.FirmwareRecord] = []
  var issues: List[report.CollectionIssue] = []
  if source.state == "observed" and source.data != null and ! source.truncated {
    let parsed = parse_smbios_table(source.data ?? b"")?
    records = parsed.records
    for line_item in parsed.issues |> enumerate() {
      issues = issues.push(
        issue_with_detail(
          "firmware",
          f"smbios.issue.${line_item.index}",
          if parsed.truncated {
            report.Truncated
          } else {
            report.Malformed
          },
          "smbios_record_validation",
          null,
          line_item.value,
        ),
      )
    }

    let state = if parsed.issues.len() == 0 {
      report.Complete
    } else if parsed.truncated {
      report.SectionTruncated
    } else {
      report.Partial
    }
    return {
      status: {
        state: state,
        enumeration_succeeded: ! parsed.truncated,
      },
      source: "smbios",
      records: records,
      limitation: {
        state: report.Observed,
        value: "The DMI table does not expose the SMBIOS entry-point version metadata.",
        raw_bytes_base64: null,
      },
      issues: issues,
    }
  }

  let device_tree_model = read_device_tree_strings(root, p"sys/firmware/devicetree/base/model", 4096, true).source
  let device_tree_compatible = read_device_tree_strings(root, p"sys/firmware/devicetree/base/compatible", 16384, false).source
  if source.truncated {
    issues = issues.push(issue("firmware", "smbios", report.Truncated, "dmi_table_limit", source.errno))
  } else if source.state != "absent" {
    issues = issues.push(
      issue("firmware", "smbios", live_source_observation_state(source.state, false), source.error_kind, source.errno),
    )
  }

  issues = append_text_issue(issues, "firmware", "device_tree_model", device_tree_model)
  issues = append_text_issue(issues, "firmware", "device_tree_compatible", device_tree_compatible)
  if device_tree_model.observation.state == report.Observed or device_tree_compatible.observation.state == report.Observed {
    return {
      status: {
        state: report.Partial,
        enumeration_succeeded: false,
      },
      source: "device-tree",
      records: [],
      limitation: {
        state: report.Absent,
        value: "SMBIOS records are not exported; device-tree identity is retained in the identity section.",
        raw_bytes_base64: null,
      },
      issues: issues,
    }
  }

  if issues.len() == 0 {
    issues = issues.push(issue("firmware", "smbios", report.Absent, "firmware_table_unavailable", null))
  }

  return {
    status: {
      state: report.SectionUnsupported,
      enumeration_succeeded: false,
    },
    source: "unavailable",
    records: [],
    limitation: {
      state: report.Unsupported,
      value: null,
      raw_bytes_base64: null,
    },
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
    if length > 4 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 4), "vendor_index"))
    }

    if length > 5 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 5), "version_index"))
    }

    if length > 8 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 8), "release_date_index"))
    }

    if length > 9 {
      fields = fields.push(smbios_raw_field(data.byte_at(offset + 9), "rom_size_raw"))
    }
  } else if record_type == 1 {
    if length > 4 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 4), "manufacturer_index"))
    }

    if length > 5 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 5), "product_index"))
    }

    if length > 6 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 6), "version_index"))
    }

    if length > 7 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 7), "serial_index"))
    }

    if length > 24 {
      fields = fields.push(smbios_raw_field(data.byte_at(offset + 24), "wake_up_type_raw"))
    }

    if length > 25 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 25), "sku_index"))
    }

    if length > 26 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 26), "family_index"))
    }
  } else if record_type == 2 {
    if length > 4 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 4), "manufacturer_index"))
    }

    if length > 5 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 5), "product_index"))
    }

    if length > 6 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 6), "version_index"))
    }

    if length > 7 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 7), "serial_index"))
    }

    if length > 8 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 8), "asset_tag_index"))
    }

    if length > 13 {
      fields = fields.push(smbios_raw_field(data.byte_at(offset + 13), "board_type_raw"))
    }
  } else if record_type == 4 {
    if length > 4 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 4), "socket_designation_index"))
    }

    if length > 7 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 7), "manufacturer_index"))
    }

    if length > 16 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 16), "version_index"))
    }

    if length > 23 {
      fields = fields.push(smbios_raw_field(data.byte_at(offset + 23), "core_count_raw"))
    }

    if length > 24 {
      fields = fields.push(smbios_raw_field(data.byte_at(offset + 24), "core_enabled_raw"))
    }

    if length > 25 {
      fields = fields.push(smbios_raw_field(data.byte_at(offset + 25), "thread_count_raw"))
    }
  } else if record_type == 16 {
    if length >= 11 {
      fields = fields.push(
        {name: "maximum_capacity_raw", value: bytes.unpack_le(data, 4, offset + 7)?, unit: "smbios_raw"},
      )
    }

    if length >= 15 {
      fields = fields.push({name: "number_of_devices", value: bytes.unpack_le(data, 2, offset + 13)?, unit: "count"})
    }
  } else if record_type == 17 {
    if length >= 14 {
      fields = fields.push({name: "total_width_raw", value: bytes.unpack_le(data, 2, offset + 8)?, unit: "smbios_raw"})
      fields = fields.push({name: "data_width_raw", value: bytes.unpack_le(data, 2, offset + 10)?, unit: "smbios_raw"})
      fields = fields.push({name: "size_raw", value: bytes.unpack_le(data, 2, offset + 12)?, unit: "smbios_raw"})
    }

    if length > 16 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 16), "device_locator_index"))
    }

    if length > 17 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 17), "bank_locator_index"))
    }

    if length > 26 {
      fields = fields.push(smbios_string_index(data.byte_at(offset + 26), "part_number_index"))
    }

    if length >= 32 and bytes.unpack_le(data, 2, offset + 12)? == 32767 {
      fields = fields.push(
        {name: "extended_size_raw", value: bytes.unpack_le(data, 4, offset + 28)?, unit: "smbios_raw"},
      )
    }
  }

  fields
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
            let raw = data.slice(string_start, position - string_start)
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
          let raw = data.slice(string_start, position - string_start)
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

    if ! found_terminator {
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

  if ! saw_end_marker {
    issues = issues.push("SMBIOS table has no end-of-table record")
  }

  return {records: records, issues: issues, truncated: false}
}

## Parses interface settings and endpoints without joining identical numbers across configurations.
export proc parse_usb_alternates(data: Bytes) [error] -> Result[List[UsbDescriptorAlternate]] {
  let records = collectors.parse_usb_descriptor_stream(data)?
  var alternates: List[UsbDescriptorAlternate] = []
  var current_configuration: Int? = null
  var current_configuration_end: Int? = null
  var active: UsbDescriptorAlternate? = null
  for descriptor in records {
    if current_configuration_end != null {
      let configuration_end = current_configuration_end
      if descriptor.descriptor_type == 1 or descriptor.descriptor_type == 2 {
        if descriptor.offset != configuration_end {
          return Err(
            SystemReportUsbDescriptorError.Invalid(
              message: "USB configuration descriptor bytes do not match their declared total length",
            ),
          )
        }
      } else if descriptor.offset >= configuration_end or descriptor.offset + descriptor.length > configuration_end {
        return Err(SystemReportUsbDescriptorError.Invalid(message: "USB descriptor extends outside its configuration"))
      }
    }

    if descriptor.descriptor_type == 1 {
      if descriptor.length < 18 {
        return Err(
          SystemReportUsbDescriptorError.Invalid(message: "USB device descriptor is shorter than its fixed header"),
        )
      }

      if active != null {
        alternates = alternates.push(active)
      }

      current_configuration = null
      current_configuration_end = null
      active = null
      continue
    }

    if descriptor.descriptor_type == 2 {
      if descriptor.length < 9 {
        return Err(
          SystemReportUsbDescriptorError.Invalid(message: "USB configuration descriptor is shorter than its fixed header"),
        )
      }

      let total_length = bytes.unpack_le(descriptor.raw, 2, 2)?
      if total_length < descriptor.length or total_length > data.len() - descriptor.offset {
        return Err(
          SystemReportUsbDescriptorError.Invalid(
            message: "USB configuration total length is outside the available descriptor bytes",
          ),
        )
      }

      if active != null {
        alternates = alternates.push(active)
      }

      current_configuration = bytes.unpack_le(descriptor.raw, 1, 5)?
      current_configuration_end = descriptor.offset + total_length
      active = null
      continue
    }

    if descriptor.descriptor_type == 4 {
      if descriptor.length < 9 {
        return Err(
          SystemReportUsbDescriptorError.Invalid(message: "USB interface descriptor is shorter than its fixed header"),
        )
      }

      let interface_number = bytes.unpack_le(descriptor.raw, 1, 2)?
      let setting_number = bytes.unpack_le(descriptor.raw, 1, 3)?
      if active != null {
        alternates = alternates.push(active)
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
        SystemReportUsbDescriptorError.Invalid(message: "USB endpoint descriptor is truncated or has no owning interface"),
      )
    }

    if active == null {
      return Err(
        SystemReportUsbDescriptorError.Invalid(message: "USB endpoint descriptor is truncated or has no owning interface"),
      )
    }

    let current = active.require(UsbDescriptorAlternate)?
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
    let endpoint: report.UsbEndpoint = {
      address: address,
      direction: if address >= 128 { "in" } else { "out" },
      transfer_type: transfer_type,
      max_packet_size: packet_size,
      interval: interval,
    }
    active = {...current, endpoints: current.endpoints.push(endpoint)}
  }

  if active != null {
    alternates = alternates.push(active)
  }

  if current_configuration_end != null and current_configuration_end != data.len() {
    return Err(
      SystemReportUsbDescriptorError.Invalid(
        message: "USB configuration descriptor bytes do not match their declared total length",
      ),
    )
  }

  alternates
}

pure usb_parent_name(name: Str, bus_number: Int?) -> Str? {
  if name.starts_with("usb") or bus_number == null {
    return null
  }

  let bus_id = bus_number ?? -1
  let prefix = f"${bus_id}-"
  if ! name.starts_with(prefix) {
    return null
  }

  let components = name.split(".")
  if components.len() <= 1 {
    return f"usb${bus_id}"
  }

  return (components |> take(components.len() - 1)).join(".")
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

# Preserves the first sysfs-name index for joins without rescanning the device list.
pure usb_device_indices(devices: List[report.UsbDevice]) -> Map[Int] {
  var indices: Map[Int] = {}
  for index in range(devices.len()) {
    let name = devices[index].sysfs_name ?? ""
    if devices[index].sysfs_name != null and ! indices.has(name) {
      indices = indices.set(name, index)
    }
  }

  return indices
}

## Resolves USB parent indexes after every device has been enumerated.
export pure link_usb_parents(devices: List[report.UsbDevice]) -> List[report.UsbDevice] {
  let device_index_by_name = usb_device_indices(devices)
  var linked: List[report.UsbDevice] = []
  for device in devices {
    let parent_name = usb_parent_name(device.sysfs_name ?? "", device.bus_number)
    var parent_index: Int? = null
    if parent_name != null {
      if device_index_by_name.has(parent_name) {
        match device_index_by_name.get(parent_name) {
          Ok(index) => parent_index = index
          Err(_) => {}
        }
      }
    }

    linked = linked.push({...device, parent_device_index: parent_index})
  }

  return linked
}

# Preserves the first BDF index for joins without rescanning the function list.
pure pci_function_indices(functions: List[report.PciFunction]) -> Map[Int] {
  var indices: Map[Int] = {}
  for index in range(functions.len()) {
    let address = functions[index].address ?? ""
    if functions[index].address != null and ! indices.has(address) {
      indices = indices.set(address, index)
    }
  }

  return indices
}

pure pci_function_index(indices: Map[Int], address: Str?) -> Int? {
  if address == null or ! indices.has(address ?? "") {
    return null
  }

  return indices.get(address ?? "", 0)
}

pure live_source_observation_state(state: Str, truncated: Bool) -> report.ObservationState {
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

## Finds a USB controller's PCI function in its bus-entry symlink target.
export pure usb_parent_address(target: Path) -> Str? {
  var result: Str? = null
  for component in target.display().split("/") {
    match collectors.parse_pci_address(component) {
      Ok(_) => result = component
      Err(_) => {}
    }
  }

  return result
}

proc usb_source_is_directory(root: FsRoot, device_path: Path) [fs, error] -> Bool {
  match fs.root_metadata(root, device_path) {
    Ok(metadata) => return metadata.kind == "dir"
    Err(_) => return false
  }
}

proc class_entry_target(root: FsRoot, entry: Path) [fs, error] -> ClassParentObservation {
  match fs.root_readlink_result(root, entry) {
    Ok(observed) => {
      if observed.state == "observed" {
        return {target: observed.target, state: report.Observed, errno: observed.errno, error_kind: observed.error_kind}
      }

      if observed.state == "absent" {
        return {target: null, state: report.Disappeared, errno: observed.errno, error_kind: observed.error_kind}
      }

      if observed.error_kind == "invalid_input" and usb_source_is_directory(root, entry) {
        return {target: null, state: report.Observed, errno: null, error_kind: null}
      }

      return {
        target: null,
        state: live_source_observation_state(observed.state, false),
        errno: observed.errno,
        error_kind: observed.error_kind,
      }
    }
    Err(_) => return {target: null, state: report.Malformed, errno: null, error_kind: "invalid_class_entry_path"}
  }
}

## Prefers a device link and keeps a class-entry link as independent parent evidence.
export proc class_parent_target(root: FsRoot, entry: Path) [fs, error] -> ClassParentObservation {
  let fallback = class_entry_target(root, entry)
  match fs.root_readlink_result(root, fp"${entry}/device") {
    Ok(observed) => {
      if observed.state == "observed" {
        if observed.target != null {
          return {target: observed.target, state: report.Observed, errno: null, error_kind: null}
        }

        return {target: fallback.target, state: report.Malformed, errno: null, error_kind: "missing_device_target"}
      }

      if observed.state == "absent" {
        return fallback
      }

      return {
        target: fallback.target,
        state: live_source_observation_state(observed.state, false),
        errno: observed.errno,
        error_kind: observed.error_kind,
      }
    }
    Err(_) => return {
      target: fallback.target,
      state: report.Malformed,
      errno: null,
      error_kind: "invalid_device_link_path",
    }
  }
}

## Reads a USB bus-entry link and accepts direct directories in rooted fixtures.
export proc usb_controller_address(root: FsRoot, device_path: Path) [fs, error] -> UsbControllerObservation {
  match fs.root_readlink_result(root, device_path) {
    Ok(observed) => {
      if observed.state == "observed" {
        if observed.target == null {
          return {address: null, state: report.Malformed, errno: null, error_kind: "missing_controller_target"}
        }

        return {
          address: usb_parent_address(observed.target ?? p""),
          state: report.Observed,
          errno: null,
          error_kind: null,
        }
      }

      if observed.state == "absent" {
        return {address: null, state: report.Disappeared, errno: observed.errno, error_kind: observed.error_kind}
      }

      if observed.error_kind == "invalid_input" and usb_source_is_directory(root, device_path) {
        return {address: null, state: report.Observed, errno: null, error_kind: null}
      }

      return {
        address: null,
        state: live_source_observation_state(observed.state, false),
        errno: observed.errno,
        error_kind: observed.error_kind,
      }
    }
    Err(_) => return {address: null, state: report.Malformed, errno: null, error_kind: "invalid_usb_source_path"}
  }
}

## Reads an optional driver binding while retaining failures separate from an unbound device.
export proc optional_driver_name(root: FsRoot, source_path: Path) [fs, error] -> collectors.SourceRead {
  match fs.root_readlink_result(root, source_path) {
    Ok(observed) => {
      let state = live_source_observation_state(observed.state, false)
      var value: Str? = null
      if observed.target != null {
        value = (observed.target ?? p"").name()
      }

      if state == report.Observed and value == null {
        return {
          observation: empty_text(report.Malformed),
          errno: null,
          error_kind: "missing_driver_target",
        }
      }

      return {
        observation: {
          state: state,
          value: value,
          raw_bytes_base64: null,
        },
        errno: observed.errno,
        error_kind: observed.error_kind,
      }
    }
    Err(_) => return {
      observation: empty_text(report.Malformed),
      errno: null,
      error_kind: "invalid_driver_link_path",
    }
  }
}

pure parse_hex_optional(value: Str?, width: Int) -> Int? {
  if value == null {
    return null
  }

  if (value ?? "").byte_len() != width {
    return null
  }

  match collectors.parse_pci_hex_value(value ?? "") {
    Ok(parsed) => return parsed
    Err(_) => return null
  }
}

pure usb_decimal_optional(value: Str?, minimum: Int) -> Int? {
  let parsed = parse_integer(value)
  if parsed == null {
    return null
  }

  let number = parsed ?? 0
  if number < minimum or number > 9007199254740991 {
    return null
  }

  return parsed
}

proc collect_usb(root: FsRoot, pci_functions: List[report.PciFunction]) [fs, error] -> UsbCollection {
  let listing = fs.root_children(root, p"sys/bus/usb/devices", max_entries: 4096)?
  let pci_indices = pci_function_indices(pci_functions)
  var devices: List[report.UsbDevice] = []
  var issues: List[report.CollectionIssue] = []
  if listing.state != "complete" {
    issues = issues.push(
      issue("usb", "devices", live_source_observation_state(listing.state, false), listing.error_kind, listing.errno),
    )
  }

  for device_path in listing.children {
    continue when ":" in device_path.name()
    let vendor = read_value(root, fp"${device_path}/idVendor", max_bytes: 4096)
    let product = read_value(root, fp"${device_path}/idProduct", max_bytes: 4096)
    let vendor_id = parse_hex_optional(observed_source_text(vendor), 4)
    let product_id = parse_hex_optional(observed_source_text(product), 4)
    if vendor.observation.state != report.Observed {
      issues = issues.push(
        issue("usb", f"devices.${device_path.name()}.vendor_id", vendor.observation.state, vendor.error_kind, vendor.errno),
      )
    } else if vendor_id == null {
      issues = issues.push(
        issue("usb", f"devices.${device_path.name()}.vendor_id", report.Malformed, "invalid_usb_vendor_id", null),
      )
    }

    if product.observation.state != report.Observed {
      issues = issues.push(
        issue("usb", f"devices.${device_path.name()}.product_id", product.observation.state, product.error_kind, product.errno),
      )
    } else if product_id == null {
      issues = issues.push(
        issue("usb", f"devices.${device_path.name()}.product_id", report.Malformed, "invalid_usb_product_id", null),
      )
    }

    let bus = read_value(root, fp"${device_path}/busnum", max_bytes: 4096)
    let number = read_value(root, fp"${device_path}/devnum", max_bytes: 4096)
    let version = read_value(root, fp"${device_path}/bcdDevice", max_bytes: 4096)
    let class = read_value(root, fp"${device_path}/bDeviceClass", max_bytes: 4096)
    let subclass = read_value(root, fp"${device_path}/bDeviceSubClass", max_bytes: 4096)
    let protocol = read_value(root, fp"${device_path}/bDeviceProtocol", max_bytes: 4096)
    let manufacturer = read_value(root, fp"${device_path}/manufacturer", max_bytes: 4096)
    let product_text = read_value(root, fp"${device_path}/product", max_bytes: 4096)
    let serial = read_value(root, fp"${device_path}/serial", max_bytes: 4096)
    let speed = read_value(root, fp"${device_path}/speed", max_bytes: 4096)
    let configurations = read_value(root, fp"${device_path}/bNumConfigurations", max_bytes: 4096)
    let active_configuration = read_value(root, fp"${device_path}/bConfigurationValue", max_bytes: 4096)
    let power_control = read_value(root, fp"${device_path}/power/control", max_bytes: 4096)
    let autosuspend = read_value(root, fp"${device_path}/power/autosuspend_delay_ms", max_bytes: 4096)
    let runtime_status = read_value(root, fp"${device_path}/power/runtime_status", max_bytes: 4096)
    let class_code = parse_hex_optional(observed_source_text(class), 2)
    let subclass_code = parse_hex_optional(observed_source_text(subclass), 2)
    let protocol_code = parse_hex_optional(observed_source_text(protocol), 2)
    let version_code = parse_hex_optional(observed_source_text(version), 4)
    let version_value: Str? = if version_code == null { null } else { observed_source_text(version) }
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
          issue("usb", f"devices.${device_path.name()}.${named_value.name}", report.Malformed, "invalid_usb_hex_value", null),
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
      issues = append_text_issue(
        issues,
        "usb",
        f"devices.${device_path.name()}.${named_source.name}",
        named_source.source,
      )
    }

    if power_control.observation.state != report.Observed and power_control.observation.state != report.Absent {
      issues = issues.push(
        issue(
          "usb",
          f"devices.${device_path.name()}.power_control",
          power_control.observation.state,
          power_control.error_kind,
          power_control.errno,
        ),
      )
    }

    if autosuspend.observation.state != report.Observed and autosuspend.observation.state != report.Absent {
      issues = issues.push(
        issue(
          "usb",
          f"devices.${device_path.name()}.autosuspend_delay_ms",
          autosuspend.observation.state,
          autosuspend.error_kind,
          autosuspend.errno,
        ),
      )
    }

    if runtime_status.observation.state != report.Observed and runtime_status.observation.state != report.Absent {
      issues = issues.push(
        issue(
          "usb",
          f"devices.${device_path.name()}.runtime_status",
          runtime_status.observation.state,
          runtime_status.error_kind,
          runtime_status.errno,
        ),
      )
    }

    let raw_descriptors = fs.root_read_result(root, fp"${device_path}/descriptors", max_bytes: 1048576)?
    var descriptor_alternates: List[UsbDescriptorAlternate] = []
    if raw_descriptors.truncated {
      issues = issues.push(
        issue(
          "usb",
          f"devices.${device_path.name()}.descriptors",
          report.Truncated,
          "descriptor_input_limit",
          raw_descriptors.errno,
        ),
      )
    } else if raw_descriptors.state == "observed" and raw_descriptors.data != null {
      match parse_usb_alternates(raw_descriptors.data ?? b"") {
        Ok(alternates) => descriptor_alternates = alternates
        Err(_) => issues = issues.push(
          issue("usb", f"devices.${device_path.name()}.descriptors", report.Malformed, "invalid_usb_descriptor_stream", null),
        )
      }
    } else if raw_descriptors.state == "read_failure" or raw_descriptors.state == "permission_denied" {
      issues = issues.push(
        issue(
          "usb",
          f"devices.${device_path.name()}.descriptors",
          live_source_observation_state(raw_descriptors.state, raw_descriptors.truncated),
          raw_descriptors.error_kind,
          raw_descriptors.errno,
        ),
      )
    }

    var interfaces: List[report.UsbInterface] = []
    for interface_path in listing.children {
      let interface_name = interface_path.name()
      continue unless interface_name.starts_with(f"${device_path.name()}:")
      let interface_number_text = interface_name.split(":").get(1, "").split(".").get(1, "")
      let interface_number = parse_integer(interface_number_text) ?? -1
      if interface_number < 0 {
        issues = issues.push(
          issue(
            "usb",
            f"devices.${device_path.name()}.interfaces.${interface_name}",
            report.Malformed,
            "invalid_interface_name",
            null,
          ),
        )
        continue
      }

      let driver_link = optional_driver_name(root, fp"${interface_path}/driver")
      if driver_link.observation.state != report.Observed and driver_link.observation.state != report.Absent {
        issues = issues.push(
          issue(
            "usb",
            f"devices.${device_path.name()}.interfaces.${interface_name}.driver",
            driver_link.observation.state,
            driver_link.error_kind,
            driver_link.errno,
          ),
        )
      }

      let active = read_value(root, fp"${interface_path}/bAlternateSetting", max_bytes: 4096)
      let active_alternate = usb_decimal_optional(observed_source_text(active), 0)
      if active.observation.state == report.Observed and active_alternate == null {
        issues = issues.push(
          issue(
            "usb",
            f"devices.${device_path.name()}.interfaces.${interface_name}.active_alternate",
            report.Malformed,
            "invalid_usb_alternate",
            null,
          ),
        )
      } else if active.observation.state != report.Observed and active.observation.state != report.Absent {
        issues = append_text_issue(
          issues,
          "usb",
          f"devices.${device_path.name()}.interfaces.${interface_name}.active_alternate",
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
        driver: observed_source_text(driver_link),
        active_alternate: active_alternate,
        alternate_settings: alternate_settings,
      })
    }

    let bus_number = usb_decimal_optional(observed_source_text(bus), 1)
    let device_number = usb_decimal_optional(observed_source_text(number), 1)
    let configuration_count = usb_decimal_optional(observed_source_text(configurations), 0)
    let active_configuration_number = usb_decimal_optional(observed_source_text(active_configuration), -1)
    let autosuspend_delay = usb_decimal_optional(observed_source_text(autosuspend), -9007199254740991)
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
          issue("usb", f"devices.${device_path.name()}.${named_number.name}", report.Malformed, "invalid_usb_number", null),
        )
      }
    }

    let controller = usb_controller_address(root, device_path)
    if controller.state != report.Observed {
      issues = issues.push(
        issue("usb", f"devices.${device_path.name()}.controller", controller.state, controller.error_kind, controller.errno),
      )
    }

    devices = devices.push({
      sysfs_name: device_path.name(),
      parent_device_index: null,
      controller_pci_index: pci_function_index(pci_indices, controller.address),
      port_path: usb_port_path(device_path.name()),
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
      speed_mbps: observed_source_text(speed),
      configuration_count: configuration_count,
      active_configuration: active_configuration_number,
      power_control: observed_source_text(power_control),
      autosuspend_delay_ms: autosuspend_delay,
      runtime_status: observed_source_text(runtime_status),
      is_root_hub: device_path.name().starts_with("usb"),
      interfaces: interfaces,
    })
  }

  var state = report.Complete
  if listing.state == "absent" {
    state = report.SectionAbsent
  } else if listing.state != "complete" or issues.len() > 0 {
    state = report.Partial
  }

  return {
    status: {
      state: state,
      enumeration_succeeded: listing.enumeration_succeeded,
    },
    devices: link_usb_parents(devices),
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
  let release = read_value(root, p"proc/sys/kernel/osrelease")
  let version = read_value(root, p"proc/version", max_bytes: 65536)
  let hostname = read_value(root, p"proc/sys/kernel/hostname")
  let boot_id = read_value(root, p"proc/sys/kernel/random/boot_id", max_bytes: 4096)
  let uptime = read_value(root, p"proc/uptime", max_bytes: 4096)
  var os_release = read_value(root, p"etc/os-release", max_bytes: 65536)
  if os_release.observation.state == report.Absent {
    os_release = read_value(root, p"usr/lib/os-release", max_bytes: 65536)
  }

  var issues = base.issues
  if release.observation.state != report.Observed {
    issues = issues.push(
      issue("identity", "kernel_release", release.observation.state, release.error_kind, release.errno),
    )
  }

  if version.observation.state != report.Observed {
    issues = issues.push(
      issue("identity", "kernel_build", version.observation.state, version.error_kind, version.errno),
    )
  }

  if os_release.observation.state != report.Observed {
    issues = issues.push(
      issue("identity", "os_release", os_release.observation.state, os_release.error_kind, os_release.errno),
    )
  }

  issues = append_text_issue(issues, "identity", "hostname", hostname)
  issues = append_text_issue(issues, "identity", "boot_id", boot_id)

  var os: report.OsRelease? = null
  let os_release_text = value_or_null(os_release)
  if os_release_text != null {
    var id: Str? = null
    var id_issue_recorded = false
    var name: Str? = null
    var pretty_name: Str? = null
    var os_version: Str? = null
    var version_id: Str? = null
    for line_item in os_release_text.lines() |> enumerate() {
      let line = line_item.value
      continue when line.trim() == "" or line.trim().starts_with("#")
      let pair = line.split("=", maxsplit: 1)
      if pair.len() != 2 or ! collectors.valid_os_release_key(pair[0]) {
        issues = issues.push(
          issue("identity", f"os_release.line.${line_item.index}", report.Malformed, "invalid_assignment", null),
        )
        continue
      }

      let key = pair[0]
      continue when key not in ["ID", "NAME", "PRETTY_NAME", "VERSION", "VERSION_ID"]
      let parsed = collectors.decode_os_release_value(pair[1])
      if parsed == null or key == "ID" and ! collectors.valid_os_release_id(parsed ?? "") {
        issues = issues.push(issue("identity", f"os_release.${key}", report.Malformed, "invalid_value", null))
        if key == "ID" {
          id_issue_recorded = true
        }

        continue
      }

      match key {
        "ID" => id = parsed
        "NAME" => name = parsed
        "PRETTY_NAME" => pretty_name = parsed
        "VERSION" => os_version = parsed
        "VERSION_ID" => version_id = parsed
        _ => {}
      }
    }

    if id == null and ! id_issue_recorded {
      issues = issues.push(issue("identity", "os_release.ID", report.Malformed, "missing_id", null))
    }

    os = {id: id, name: name, pretty_name: pretty_name, version: os_version, version_id: version_id}
  }

  let uptime_number = collectors.parse_uptime_seconds(uptime)
  issues = append_number_issue(issues, "identity", "uptime", uptime_number)

  let firmware_read = read_firmware_identity(root)
  issues = issues.extend(firmware_read.issues)
  let identity_status = if issues.len() == base.issues.len() { report.Complete } else { report.Partial }
  let identity = {
    status: {
      state: identity_status,
      enumeration_succeeded: release.observation.state == report.Observed,
    },
    kernel_release: value_or_null(release),
    kernel_build: value_or_null(version),
    architecture: null,
    os_release: os,
    hostname: hostname.observation,
    uptime_seconds: uptime_number.value,
    boot_id: boot_id.observation,
    firmware: firmware_read.identity,
  }

  let mount_namespace = namespace_observation(root, p"proc/self/ns/mnt")?
  let network_namespace = namespace_observation(root, p"proc/self/ns/net")?
  let pid_namespace = namespace_observation(root, p"proc/self/ns/pid")?
  let cgroup_namespace = namespace_observation(root, p"proc/self/ns/cgroup")?
  let uts_namespace = namespace_observation(root, p"proc/self/ns/uts")?
  let ipc_namespace = namespace_observation(root, p"proc/self/ns/ipc")?
  let user_namespace = namespace_observation(root, p"proc/self/ns/user")?
  let time_namespace = namespace_observation(root, p"proc/self/ns/time")?
  for namespace in [
    {
      field: "mount_namespace",
      source: mount_namespace,
    },
    {
      field: "network_namespace",
      source: network_namespace,
    },
    {
      field: "pid_namespace",
      source: pid_namespace,
    },
    {
      field: "cgroup_namespace",
      source: cgroup_namespace,
    },
    {
      field: "uts_namespace",
      source: uts_namespace,
    },
    {
      field: "ipc_namespace",
      source: ipc_namespace,
    },
    {
      field: "user_namespace",
      source: user_namespace,
    },
    {
      field: "time_namespace",
      source: time_namespace,
    },
  ] {
    if namespace.source.observation.state != report.Observed {
      issues = issues.push(
        issue("scope", namespace.field, namespace.source.observation.state, namespace.source.error_kind, namespace.source.errno),
      )
    }
  }

  let scope = {
    ...base.scope,
    mount_namespace: mount_namespace.observation,
    network_namespace: network_namespace.observation,
    pid_namespace: pid_namespace.observation,
    cgroup_namespace: cgroup_namespace.observation,
    uts_namespace: uts_namespace.observation,
    ipc_namespace: ipc_namespace.observation,
    user_namespace: user_namespace.observation,
    time_namespace: time_namespace.observation,
    visible_cgroup: read_value(root, p"proc/self/cgroup", max_bytes: 65536).observation,
  }

  return {...base, identity: identity, scope: scope, issues: issues}
}

proc namespace_observation(root: FsRoot, source_path: Path) [fs, error] -> Result[collectors.SourceRead] {
  let result = fs.root_readlink_result(root, source_path)?
  let state = live_source_observation_state(result.state, false)
  var value: Str? = null
  if result.target != null {
    value = result.target.require(Path)?.display()
  }

  return {
    observation: {
      state: state,
      value: value,
      raw_bytes_base64: null,
    },
    errno: result.errno,
    error_kind: result.error_kind,
  }
}

type FirmwareIdentityRead = {identity: report.FirmwareIdentity, issues: List[report.CollectionIssue]}

proc read_firmware_identity(root: FsRoot) [fs, error] -> FirmwareIdentityRead {
  let vendor = read_value(root, p"sys/class/dmi/id/sys_vendor", max_bytes: 4096)
  let product = read_value(root, p"sys/class/dmi/id/product_name", max_bytes: 4096)
  let board_vendor = read_value(root, p"sys/class/dmi/id/board_vendor", max_bytes: 4096)
  let board_product = read_value(root, p"sys/class/dmi/id/board_name", max_bytes: 4096)
  let bios_vendor = read_value(root, p"sys/class/dmi/id/bios_vendor", max_bytes: 4096)
  let bios_version = read_value(root, p"sys/class/dmi/id/bios_version", max_bytes: 4096)
  let serial = read_value(root, p"sys/class/dmi/id/product_serial", max_bytes: 4096)
  let uuid = read_value(root, p"sys/class/dmi/id/product_uuid", max_bytes: 4096)
  let dt_model = read_device_tree_strings(root, p"sys/firmware/devicetree/base/model", 4096, true).source
  let dt_compatible_read = read_device_tree_strings(root, p"sys/firmware/devicetree/base/compatible", 16384, false)
  let dt_compatible = dt_compatible_read.source
  var issues: List[report.CollectionIssue] = []
  for named_source in [
    {
      name: "vendor",
      source: vendor,
    },
    {
      name: "product",
      source: product,
    },
    {
      name: "board_vendor",
      source: board_vendor,
    },
    {
      name: "board_product",
      source: board_product,
    },
    {
      name: "bios_vendor",
      source: bios_vendor,
    },
    {
      name: "bios_version",
      source: bios_version,
    },
    {
      name: "serial",
      source: serial,
    },
    {
      name: "uuid",
      source: uuid,
    },
    {
      name: "device_tree_model",
      source: dt_model,
    },
    {
      name: "device_tree_compatible",
      source: dt_compatible,
    },
  ] {
    issues = append_text_issue(issues, "identity", f"firmware.${named_source.name}", named_source.source)
  }

  var compatible = [
    {state: report.Observed, value: value, raw_bytes_base64: null}
    for value in dt_compatible_read.values
  ]
  var source = "dmi"
  if vendor.observation.state != report.Observed and product.observation.state != report.Observed {
    if dt_model.observation.state == report.Observed or compatible.len() > 0 {
      source = "device-tree"
    } else {
      source = "unavailable"
    }
  }

  return {
    identity: {
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
    },
    issues: issues,
  }
}

proc collect_cpu(root: FsRoot, base: report.SystemReport) [fs, error] -> report.SystemReport {
  let possible_source = read_value(root, p"sys/devices/system/cpu/possible", max_bytes: 65536)
  let present_source = read_value(root, p"sys/devices/system/cpu/present", max_bytes: 65536)
  let online_source = read_value(root, p"sys/devices/system/cpu/online", max_bytes: 65536)
  let offline_source = read_value(root, p"sys/devices/system/cpu/offline", max_bytes: 65536)
  var issues = base.issues
  let initial_issue_count = issues.len()
  let cpuset_source = read_effective_cgroup_cpuset(root)
  if cpuset_source.state != report.Observed {
    issues = issues.push(
      issue("cpu", "cgroup.effective_cpuset", cpuset_source.state, cpuset_source.error_kind, cpuset_source.errno),
    )
  }

  var possible: List[Int] = []
  var present: List[Int] = []
  var online: List[Int] = []
  var offline: List[Int] = []
  var present_enumerated = false
  for named_source in [
    {
      name: "possible",
      source: possible_source,
    },
    {
      name: "present",
      source: present_source,
    },
    {
      name: "online",
      source: online_source,
    },
    {
      name: "offline",
      source: offline_source,
    },
  ] {
    let name = named_source.name
    let source = named_source.source
    if source.observation.state != report.Observed or source.observation.value == null {
      issues = issues.push(issue("cpu", name, source.observation.state, source.error_kind, source.errno))
      continue
    }

    if name == "offline" and source.observation.value == "" {
      offline = []
      continue
    }

    let parsed = report.parse_cpu_list(source.observation.value ?? "")
    match parsed {
      Ok(ids) => {
        match name {
          "possible" => possible = ids
          "present" => {
            present = ids
            present_enumerated = true
          }
          "online" => online = ids
          "offline" => offline = ids
          _ => {}
        }
      }
      Err(_) => issues = issues.push(issue("cpu", name, report.Malformed, "invalid_cpu_list", null))
    }
  }

  var cpus: List[report.Cpu] = []
  var caches: List[report.CpuCache] = []
  var cache_by_key: Map[Int] = {}
  var cache_ids_by_cpu: Map[List[Int]] = {}
  let cpu_info_read = read_cpu_info(root)
  if cpu_info_read.state != report.Observed {
    issues = issues.push(issue("cpu", "cpuinfo", cpu_info_read.state, cpu_info_read.error_kind, cpu_info_read.errno))
  }

  let cpu_info = cpu_info_read.infos
  for cpu_id in present {
    let info = cpu_info_for_id(cpu_info, cpu_id)
    let cpu_path = fp"sys/devices/system/cpu/cpu${cpu_id}"
    let package = read_value(root, fp"${cpu_path}/topology/physical_package_id", max_bytes: 4096)
    let die = read_value(root, fp"${cpu_path}/topology/die_id", max_bytes: 4096)
    let core = read_value(root, fp"${cpu_path}/topology/core_id", max_bytes: 4096)
    let siblings = read_value(root, fp"${cpu_path}/topology/thread_siblings_list", max_bytes: 4096)
    let package_number = collectors.bounded_number(package, false)
    let die_number = collectors.bounded_number(die, false)
    let core_number = collectors.bounded_number(core, false)
    issues = append_number_issue(issues, "cpu", f"cpu${cpu_id}.topology.physical_package_id", package_number)
    issues = append_number_issue(issues, "cpu", f"cpu${cpu_id}.topology.die_id", die_number)
    issues = append_number_issue(issues, "cpu", f"cpu${cpu_id}.topology.core_id", core_number)
    issues = append_text_issue(issues, "cpu", f"cpu${cpu_id}.topology.thread_siblings_list", siblings)
    let sibling_ids = parse_list(observed_source_text(siblings))
    if siblings.observation.state == report.Observed and sibling_ids.len() == 0 {
      issues = issues.push(
        issue("cpu", f"cpu${cpu_id}.topology.thread_siblings_list", report.Malformed, "invalid_cpu_list", null),
      )
    }

    let node_listing = fs.root_children(root, cpu_path, max_entries: 256)?
    if node_listing.state != "complete" {
      issues = issues.push(
        issue(
          "cpu",
          f"cpu${cpu_id}.enumeration",
          live_source_observation_state(node_listing.state, false),
          node_listing.error_kind,
          node_listing.errno,
        ),
      )
    }

    var numa_node: Int? = null
    for node_path in node_listing.children {
      if node_path.name().starts_with("node") {
        numa_node = parse_integer((node_path.name().split("") |> drop(4)).join(""))
        break when numa_node != null
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
      package_id: package_number.value,
      die_id: die_number.value,
      core_id: core_number.value,
      thread_siblings: sibling_ids,
      cache_ids: [],
      cache_indices: [],
      numa_node: numa_node,
      policy: null,
    })

    let cache_listing = fs.root_children(root, fp"${cpu_path}/cache", max_entries: 64)?
    if cache_listing.state != "complete" and cache_listing.state != "absent" {
      issues = issues.push(
        issue(
          "cpu",
          f"cpu${cpu_id}.cache",
          live_source_observation_state(cache_listing.state, false),
          cache_listing.error_kind,
          cache_listing.errno,
        ),
      )
    }

    for cache_path in cache_listing.children {
      continue unless cache_path.name().starts_with("index")
      let sysfs_index = parse_integer((cache_path.name().split("") |> drop(5)).join("")) ?? -1
      if sysfs_index < 0 {
        issues = issues.push(
          issue("cpu", f"${cpu_id}.cache.${cache_path.name()}", report.Malformed, "invalid_cache_index", null),
        )
        continue
      }

      let level_text = read_value(root, fp"${cache_path}/level", max_bytes: 4096)
      let kind = read_value(root, fp"${cache_path}/type", max_bytes: 4096)
      let size = read_value(root, fp"${cache_path}/size", max_bytes: 4096)
      let line_size = read_value(root, fp"${cache_path}/coherency_line_size", max_bytes: 4096)
      let sets = read_value(root, fp"${cache_path}/number_of_sets", max_bytes: 4096)
      let shared = read_value(root, fp"${cache_path}/shared_cpu_list", max_bytes: 4096)
      let kernel_id_source = read_value(root, fp"${cache_path}/id", max_bytes: 4096)
      let level_number = collectors.bounded_number(level_text, true)
      issues = append_number_issue(issues, "cpu", f"cpu${cpu_id}.cache.${cache_path.name()}.level", level_number)
      let level = level_number.value ?? 0
      if level <= 0 {
        if level_number.state == null {
          let state = if level_text.observation.state == report.Absent { report.Absent } else { report.Malformed }
          issues = issues.push(
            issue("cpu", f"cpu${cpu_id}.cache.${cache_path.name()}.level", state, "invalid_cache_level", null),
          )
        }

        continue
      }

      issues = append_text_issue(issues, "cpu", f"cpu${cpu_id}.cache.${cache_path.name()}.type", kind)
      let cache_kind = observed_source_text(kind) ?? ""
      if cache_kind == "" {
        if kind.observation.state == report.Observed or kind.observation.state == report.Absent {
          let state = if kind.observation.state == report.Absent { report.Absent } else { report.Malformed }
          issues = issues.push(
            issue("cpu", f"cpu${cpu_id}.cache.${cache_path.name()}.type", state, "invalid_cache_type", null),
          )
        }

        continue
      }

      var shared_cpus: List[Int] = []
      if shared.observation.state != report.Observed or shared.observation.value == null {
        issues = append_text_issue(issues, "cpu", f"cpu${cpu_id}.cache.${cache_path.name()}.shared_cpu_list", shared)
      } else {
        match collectors.parse_cache_shared_cpus(shared.observation.value ?? "") {
          Ok(ids) => shared_cpus = ids
          Err(_) => issues = issues.push(
            issue(
              "cpu",
              f"cpu${cpu_id}.cache.${cache_path.name()}.shared_cpu_list",
              report.Malformed,
              "invalid_cache_cpu_list",
              null,
            ),
          )
        }
      }

      var kernel_id: Int? = null
      if kernel_id_source.observation.state == report.Observed {
        let parsed_id = collectors.bounded_number(kernel_id_source, true)
        issues = append_number_issue(issues, "cpu", f"cpu${cpu_id}.cache.${cache_path.name()}.id", parsed_id)
        kernel_id = parsed_id.value
      } else if kernel_id_source.observation.state != report.Absent {
        issues = append_text_issue(issues, "cpu", f"cpu${cpu_id}.cache.${cache_path.name()}.id", kernel_id_source)
      }

      let cache_key = if shared_cpus.len() > 0 {
        if kernel_id != null {
          json.encode({level: level, kind: cache_kind, kernel_id: kernel_id ?? -1})?
        } else {
          json.encode({level: level, kind: cache_kind, shared_cpus: shared_cpus, sysfs_index: sysfs_index})?
        }
      } else {
        f"owner:${cpu_id}:${sysfs_index}"
      }
      let size_number = collectors.bounded_size_bytes(size)
      issues = append_number_issue(issues, "cpu", f"cpu${cpu_id}.cache.${cache_path.name()}.size", size_number)
      let line_size_number = collectors.bounded_number(line_size, true)
      let sets_number = collectors.bounded_number(sets, true)
      issues = append_number_issue(
        issues,
        "cpu",
        f"cpu${cpu_id}.cache.${cache_path.name()}.coherency_line_size",
        line_size_number,
      )
      issues = append_number_issue(
        issues,
        "cpu",
        f"cpu${cpu_id}.cache.${cache_path.name()}.number_of_sets",
        sets_number,
      )
      if cache_by_key.has(cache_key) {
        let previous = caches[cache_by_key.get(cache_key)?]
        if previous.shared_cpus != shared_cpus or previous.level != level or previous.kind != cache_kind or previous.size_bytes != size_number.value or previous.line_size_bytes != line_size_number.value or previous.sets != sets_number.value {
          issues = issues.push(
            issue("cpu", f"cpu${cpu_id}.cache.${cache_path.name()}", report.Malformed, "inconsistent_cache_instance", null),
          )
        }

        continue
      }

      let cache_id = caches.len()
      caches = caches.push({
        id: cache_id,
        sysfs_index: sysfs_index,
        owner_cpu_id: cpu_id,
        level: level,
        kind: cache_kind,
        size_bytes: size_number.value,
        line_size_bytes: line_size_number.value,
        sets: sets_number.value,
        shared_cpus: shared_cpus,
      })
      cache_by_key = cache_by_key.set(cache_key, cache_id)
      let members = if shared_cpus.len() > 0 { shared_cpus } else { [cpu_id] }
      for member in members {
        let key = f"${member}"
        cache_ids_by_cpu = cache_ids_by_cpu.set(key, cache_ids_by_cpu.get(key, []).push(cache_id))
      }
    }
  }

  let policies = collect_frequency_policies(root, issues)
  issues = policies.issues
  var linked_cpus: List[report.Cpu] = []
  for cpu_item in cpus {
    var policy_name: Str? = null
    let cache_ids = cache_ids_by_cpu.get(f"${cpu_item.id}", [])
    for policy in policies.policies {
      if cpu_item.id in policy.related_cpus {
        policy_name = policy.name
        break
      }
    }

    linked_cpus = linked_cpus.push({
      ...cpu_item,
      policy: policy_name,
      cache_ids: cache_ids,
      cache_indices: cache_ids,
    })
  }

  let vulnerabilities_listing = fs.root_children(root, p"sys/devices/system/cpu/vulnerabilities", max_entries: 256)?
  if vulnerabilities_listing.state != "complete" and vulnerabilities_listing.state != "absent" {
    issues = issues.push(
      issue(
        "cpu",
        "vulnerabilities",
        live_source_observation_state(vulnerabilities_listing.state, false),
        vulnerabilities_listing.error_kind,
        vulnerabilities_listing.errno,
      ),
    )
  }

  var vulnerabilities: List[report.CpuVulnerability] = []
  for item in vulnerabilities_listing.children {
    let value = read_value(root, item, max_bytes: 16384)
    vulnerabilities = vulnerabilities.push({name: item.name(), description: value.observation})
    if value.observation.state != report.Observed {
      issues = issues.push(
        issue("cpu", f"vulnerabilities.${item.name()}", value.observation.state, value.error_kind, value.errno),
      )
    }
  }

  let idle_driver = read_value(root, p"sys/devices/system/cpu/cpuidle/current_driver", max_bytes: 4096)
  var idle_governor = read_value(root, p"sys/devices/system/cpu/cpuidle/current_governor", max_bytes: 4096)
  var idle_governor_field = "cpuidle.current_governor"
  if idle_governor.observation.state == report.Absent {
    idle_governor = read_value(root, p"sys/devices/system/cpu/cpuidle/current_governor_ro", max_bytes: 4096)
    idle_governor_field = "cpuidle.current_governor_ro"
  }

  let available_idle_governors = read_value(
    root,
    p"sys/devices/system/cpu/cpuidle/available_governors",
    max_bytes: 4096,
  )
  issues = append_text_issue(issues, "cpu", "cpuidle.current_driver", idle_driver)
  issues = append_text_issue(issues, "cpu", idle_governor_field, idle_governor)
  issues = append_text_issue(issues, "cpu", "cpuidle.available_governors", available_idle_governors)
  let affinity = read_value(root, p"proc/self/status", max_bytes: 65536)
  var affinity_cpus: List[Int] = []
  if affinity.observation.state != report.Observed or affinity.observation.value == null {
    issues = issues.push(issue("cpu", "affinity", affinity.observation.state, affinity.error_kind, affinity.errno))
  } else {
    var affinity_text: Str? = null
    var duplicate_affinity = false
    for line in affinity.observation.value.lines() {
      if line.starts_with("Cpus_allowed_list:") {
        if affinity_text != null {
          duplicate_affinity = true
        } else {
          affinity_text = line.split(":", maxsplit: 1).get(1, "").trim()
        }
      }
    }

    if duplicate_affinity {
      issues = issues.push(issue("cpu", "affinity", report.Malformed, "duplicate_cpu_list", null))
    } else if affinity_text == null {
      issues = issues.push(issue("cpu", "affinity", report.Malformed, "missing_cpu_list", null))
    } else {
      affinity_cpus = parse_list(affinity_text)
      if affinity_cpus.len() == 0 {
        issues = issues.push(issue("cpu", "affinity", report.Malformed, "invalid_cpu_list", null))
      }
    }
  }

  var idle_states: List[report.CpuIdleState] = []
  for cpu_id in present {
    let cpuidle = fs.root_children(root, fp"sys/devices/system/cpu/cpu${cpu_id}/cpuidle", max_entries: 256)?
    if cpuidle.state != "complete" and cpuidle.state != "absent" {
      issues = issues.push(
        issue(
          "cpu",
          f"cpu${cpu_id}.cpuidle",
          live_source_observation_state(cpuidle.state, false),
          cpuidle.error_kind,
          cpuidle.errno,
        ),
      )
    }

    for state_path in cpuidle.children {
      continue unless state_path.name().starts_with("state")
      var state_index: Int? = null
      match collectors.parse_idle_state_index(state_path.name()) {
        Ok(index) => state_index = index
        Err(_) => issues = issues.push(
          issue("cpu", f"cpu${cpu_id}.${state_path.name()}", report.Malformed, "invalid_idle_state_index", null),
        )
      }

      continue when state_index == null
      let state_number = state_index ?? -1
      let name = read_value(root, fp"${state_path}/name", max_bytes: 4096)
      let description = read_value(root, fp"${state_path}/desc", max_bytes: 4096)
      let disable = read_value(root, fp"${state_path}/disable", max_bytes: 4096)
      let latency = read_value(root, fp"${state_path}/latency", max_bytes: 4096)
      let residency = read_value(root, fp"${state_path}/residency", max_bytes: 4096)
      let usage = read_value(root, fp"${state_path}/usage", max_bytes: 4096)
      let time_counter = read_value(root, fp"${state_path}/time", max_bytes: 4096)
      let field_prefix = f"cpu${cpu_id}.${state_path.name()}"
      if name.observation.state != report.Observed {
        issues = issues.push(issue("cpu", f"${field_prefix}.name", name.observation.state, name.error_kind, name.errno))
      }

      issues = append_text_issue(issues, "cpu", f"${field_prefix}.desc", description)
      let disable_number = collectors.bounded_number(disable, true)
      let latency_number = collectors.bounded_number(latency, true)
      let residency_number = collectors.bounded_number(residency, true)
      let usage_number = collectors.bounded_number(usage, true)
      let time_number = collectors.bounded_number(time_counter, true)
      for named_number in [
        {
          name: "disable",
          number: disable_number,
        },
        {
          name: "latency",
          number: latency_number,
        },
        {
          name: "residency",
          number: residency_number,
        },
        {
          name: "usage",
          number: usage_number,
        },
        {
          name: "time",
          number: time_number,
        },
      ] {
        issues = append_number_issue(issues, "cpu", f"${field_prefix}.${named_number.name}", named_number.number)
      }

      let raw_disable = disable_number.value ?? -1
      var disable_setting: Int? = null
      if raw_disable == 0 or raw_disable == 1 {
        disable_setting = raw_disable
      } else if disable_number.value != null {
        issues = issues.push(
          issue("cpu", f"${field_prefix}.disable", report.Malformed, "invalid_disable_setting", null),
        )
      }

      idle_states = idle_states.push({
        cpu_id: cpu_id,
        state_index: state_number,
        name: observed_source_text(name) ?? state_path.name(),
        description: observed_source_text(description),
        disable_setting: disable_setting,
        latency_us: latency_number.value,
        residency_us: residency_number.value,
        usage_count: usage_number.value,
        time_us: time_number.value,
      })
    }
  }

  var cpu_state = if issues.len() == initial_issue_count { report.Complete } else { report.Partial }
  if possible_source.observation.state != report.Observed {
    cpu_state = report.Partial
  }

  let cpu_section: report.CpuSection = {
    status: {
      state: cpu_state,
      enumeration_succeeded: present_enumerated,
    },
    possible: possible,
    present: present,
    online: online,
    offline: offline,
    affinity: affinity_cpus,
    effective_cpuset: cpuset_source.cpus,
    global_idle_driver: observed_source_text(idle_driver),
    global_idle_governor: observed_source_text(idle_governor),
    cpus: linked_cpus,
    caches: caches,
    frequency_policies: policies.policies,
    idle_states: idle_states,
    vulnerabilities: vulnerabilities,
    available_idle_governors: parse_words(observed_source_text(available_idle_governors)),
  }
  return {...base, cpu: cpu_section, issues: issues}
}

type PolicyCollection = {policies: List[report.CpuFreqPolicy], issues: List[report.CollectionIssue]}

proc collect_frequency_policies(root: FsRoot, issues: List[report.CollectionIssue]) [fs, error] -> PolicyCollection {
  let listing = fs.root_children(root, p"sys/devices/system/cpu/cpufreq", max_entries: 1024)?
  var policies: List[report.CpuFreqPolicy] = []
  var collected_issues = issues
  if listing.state == "absent" {
    collected_issues = collected_issues.push(
      issue("cpu", "frequency_policies", report.Absent, "cpufreq_not_exposed", null),
    )
  } else if listing.state != "complete" {
    collected_issues = collected_issues.push(
      issue(
        "cpu",
        "frequency_policies",
        live_source_observation_state(listing.state, false),
        listing.error_kind,
        listing.errno,
      ),
    )
  }

  let boost = read_value(root, p"sys/devices/system/cpu/cpufreq/boost", max_bytes: 4096)
  let no_turbo = read_value(root, p"sys/devices/system/cpu/intel_pstate/no_turbo", max_bytes: 4096)
  collected_issues = append_text_issue(collected_issues, "cpu", "boost", boost)
  collected_issues = append_text_issue(collected_issues, "cpu", "intel_pstate.no_turbo", no_turbo)
  let boost_text = observed_source_text(boost)
  let no_turbo_text = observed_source_text(no_turbo)
  var boost_supported: Bool? = null
  var boost_allowed: Bool? = null
  var boost_scope: Str? = null
  if boost_text != null {
    let parsed = parse_bool01(boost_text)
    if parsed == null {
      collected_issues = collected_issues.push(issue("cpu", "boost", report.Malformed, "invalid_boost_value", null))
    } else {
      boost_supported = true
      boost_allowed = parsed
      boost_scope = "system"
    }
  } else if no_turbo_text != null {
    let parsed = parse_bool01(no_turbo_text)
    if parsed == null {
      collected_issues = collected_issues.push(
        issue("cpu", "intel_pstate.no_turbo", report.Malformed, "invalid_no_turbo_value", null),
      )
    } else {
      boost_supported = true
      boost_allowed = ! parsed
      boost_scope = "intel_pstate"
    }
  }

  for directory in listing.children {
    continue unless directory.name().starts_with("policy")
    let related = read_value(root, fp"${directory}/related_cpus", max_bytes: 4096)
    let affected = read_value(root, fp"${directory}/affected_cpus", max_bytes: 4096)
    let driver = read_value(root, fp"${directory}/scaling_driver", max_bytes: 4096)
    let governor = read_value(root, fp"${directory}/scaling_governor", max_bytes: 4096)
    let available_governors = read_value(root, fp"${directory}/scaling_available_governors", max_bytes: 4096)
    let hardware_min = read_value(root, fp"${directory}/cpuinfo_min_freq", max_bytes: 4096)
    let hardware_max = read_value(root, fp"${directory}/cpuinfo_max_freq", max_bytes: 4096)
    let scaling_min = read_value(root, fp"${directory}/scaling_min_freq", max_bytes: 4096)
    let scaling_max = read_value(root, fp"${directory}/scaling_max_freq", max_bytes: 4096)
    let hardware_current = read_value(root, fp"${directory}/cpuinfo_cur_freq", max_bytes: 4096)
    let scaling_current = read_value(root, fp"${directory}/scaling_cur_freq", max_bytes: 4096)
    let average = read_value(root, fp"${directory}/cpuinfo_avg_freq", max_bytes: 4096)
    let bios_limit = read_value(root, fp"${directory}/bios_limit", max_bytes: 4096)
    let epp = read_value(root, fp"${directory}/energy_performance_preference", max_bytes: 4096)
    let available_epp = read_value(root, fp"${directory}/energy_performance_available_preferences", max_bytes: 4096)
    let available_frequency = read_value(root, fp"${directory}/scaling_available_frequencies", max_bytes: 65536)
    if related.observation.state != report.Observed {
      collected_issues = collected_issues.push(
        issue("cpu", f"${directory.name()}.related_cpus", related.observation.state, related.error_kind, related.errno),
      )
    }

    if affected.observation.state != report.Observed {
      collected_issues = collected_issues.push(
        issue("cpu", f"${directory.name()}.affected_cpus", affected.observation.state, affected.error_kind, affected.errno),
      )
    }

    if driver.observation.state != report.Observed {
      collected_issues = collected_issues.push(
        issue("cpu", f"${directory.name()}.driver", driver.observation.state, driver.error_kind, driver.errno),
      )
    }

    if governor.observation.state != report.Observed {
      collected_issues = collected_issues.push(
        issue("cpu", f"${directory.name()}.governor", governor.observation.state, governor.error_kind, governor.errno),
      )
    }

    for named_source in [
      {
        name: "scaling_available_governors",
        source: available_governors,
      },
      {
        name: "scaling_available_frequencies",
        source: available_frequency,
      },
      {
        name: "energy_performance_preference",
        source: epp,
      },
      {
        name: "energy_performance_available_preferences",
        source: available_epp,
      },
    ] {
      collected_issues = append_text_issue(
        collected_issues,
        "cpu",
        f"${directory.name()}.${named_source.name}",
        named_source.source,
      )
    }

    var related_cpus: List[Int] = []
    var affected_cpus: List[Int] = []
    let related_text = observed_source_text(related)
    let affected_text = observed_source_text(affected)
    if related_text != null {
      match collectors.parse_cpufreq_members(related_text) {
        Ok(ids) => related_cpus = ids
        Err(_) => collected_issues = collected_issues.push(
          issue("cpu", f"${directory.name()}.related_cpus", report.Malformed, "invalid_cpu_list", null),
        )
      }
    }

    if affected_text != null {
      match collectors.parse_cpufreq_members(affected_text) {
        Ok(ids) => affected_cpus = ids
        Err(_) => collected_issues = collected_issues.push(
          issue("cpu", f"${directory.name()}.affected_cpus", report.Malformed, "invalid_cpu_list", null),
        )
      }
    }

    var frequencies: List[Int] = []
    for value in parse_words(observed_source_text(available_frequency)) {
      let parsed = collectors.bounded_number(
        {...available_frequency, observation: {...available_frequency.observation, value: value}},
        true,
      )
      collected_issues = append_number_issue(
        collected_issues,
        "cpu",
        f"${directory.name()}.scaling_available_frequencies",
        parsed,
      )
      let exact = parsed.value ?? -1
      if exact >= 0 {
        frequencies = frequencies.push(exact)
      }
    }

    let hardware_min_number = collectors.bounded_number(hardware_min, true)
    let hardware_max_number = collectors.bounded_number(hardware_max, true)
    let scaling_min_number = collectors.bounded_number(scaling_min, true)
    let scaling_max_number = collectors.bounded_number(scaling_max, true)
    let hardware_current_number = collectors.bounded_number(hardware_current, true)
    let scaling_current_number = collectors.bounded_number(scaling_current, true)
    let average_number = collectors.bounded_number(average, true)
    var governor_requested_number: Int? = null
    if observed_source_text(governor) == "userspace" {
      let requested_source = read_value(root, fp"${directory}/scaling_setspeed", max_bytes: 4096)
      let requested_number = collectors.bounded_number(requested_source, true)
      collected_issues = append_number_issue(
        collected_issues,
        "cpu",
        f"${directory.name()}.scaling_setspeed",
        requested_number,
      )
      governor_requested_number = requested_number.value
    }

    let bios_limit_number = collectors.bounded_number(bios_limit, true)
    for named_number in [
      {
        name: "cpuinfo_min_freq",
        number: hardware_min_number,
      },
      {
        name: "cpuinfo_max_freq",
        number: hardware_max_number,
      },
      {
        name: "scaling_min_freq",
        number: scaling_min_number,
      },
      {
        name: "scaling_max_freq",
        number: scaling_max_number,
      },
      {
        name: "cpuinfo_cur_freq",
        number: hardware_current_number,
      },
      {
        name: "scaling_cur_freq",
        number: scaling_current_number,
      },
      {
        name: "cpuinfo_avg_freq",
        number: average_number,
      },
      {
        name: "bios_limit",
        number: bios_limit_number,
      },
    ] {
      collected_issues = append_number_issue(
        collected_issues,
        "cpu",
        f"${directory.name()}.${named_number.name}",
        named_number.number,
      )
    }

    policies = policies.push({
      name: directory.name(),
      related_cpus: related_cpus,
      affected_cpus: affected_cpus,
      driver: observed_source_text(driver),
      governor: observed_source_text(governor),
      available_governors: parse_words(observed_source_text(available_governors)),
      hardware_min_khz: hardware_min_number.value,
      hardware_max_khz: hardware_max_number.value,
      scaling_min_khz: scaling_min_number.value,
      scaling_max_khz: scaling_max_number.value,
      hardware_current_khz: hardware_current_number.value,
      scaling_current_khz: scaling_current_number.value,
      governor_requested_khz: governor_requested_number,
      average_current_khz: average_number.value,
      bios_limit_khz: bios_limit_number.value,
      transition_latency_ns: null,
      available_frequencies_khz: frequencies,
      energy_performance_preference: observed_source_text(epp),
      available_energy_performance_preferences: parse_words(observed_source_text(available_epp)),
      boost_supported: boost_supported,
      boost_allowed: boost_allowed,
      boost_active: null,
      boost_scope: boost_scope,
    })
  }

  return {policies: policies, issues: collected_issues}
}

proc read_huge_page_pool(root: FsRoot, huge_path: Path, node_id: Int?, field: Str) [fs, error] -> HugePageRead {
  let name = huge_path.name()
  let parts = name.split("-")
  if parts.len() != 2 or parts[0] != "hugepages" or ! parts[1].ends_with("kB") {
    return {pool: null, issues: [issue("memory", f"${field}.page_size", report.Malformed, "invalid_page_size", null)]}
  }

  let size_text = parts[1].split("kB").get(0, "")
  if f"${size_text}kB" != parts[1] {
    return {pool: null, issues: [issue("memory", f"${field}.page_size", report.Malformed, "invalid_page_size", null)]}
  }

  let size_kib = parse_integer(size_text)
  let size_count = size_kib ?? -1
  if size_count <= 0 {
    let state = if size_kib == null and decimal_identifier(size_text) { report.RangeFailure } else { report.Malformed }
    return {pool: null, issues: [issue("memory", f"${field}.page_size", state, "invalid_page_size", null)]}
  }

  if size_count > 8796093022207 {
    return {
      pool: null,
      issues: [
        issue("memory", f"${field}.page_size", report.RangeFailure, "byte_count_overflow", null),
      ],
    }
  }

  let total = read_value(root, fp"${huge_path}/nr_hugepages", max_bytes: 4096)
  let total_number = collectors.bounded_number(total, true)
  if total_number.value == null {
    let state = total_number.state ?? report.Absent
    let error_kind: Str? = if state == report.Absent { "missing_count" } else { total_number.error_kind }
    return {pool: null, issues: [issue("memory", f"${field}.total", state, error_kind, total_number.errno)]}
  }

  let free = collectors.bounded_number(read_value(root, fp"${huge_path}/free_hugepages", max_bytes: 4096), true)
  let reserved = collectors.bounded_number(read_value(root, fp"${huge_path}/resv_hugepages", max_bytes: 4096), true)
  let surplus = collectors.bounded_number(read_value(root, fp"${huge_path}/surplus_hugepages", max_bytes: 4096), true)
  var issues: List[report.CollectionIssue] = []
  issues = append_number_issue(issues, "memory", f"${field}.free", free)
  issues = append_number_issue(issues, "memory", f"${field}.reserved", reserved)
  issues = append_number_issue(issues, "memory", f"${field}.surplus", surplus)
  return {
    pool: {
      node_id: node_id,
      page_size_bytes: size_count * 1024,
      total: total_number.value ?? 0,
      free: free.value,
      reserved: reserved.value,
      surplus: surplus.value,
    },
    issues: issues,
  }
}

proc collect_memory(root: FsRoot, base: report.SystemReport) [fs, error] -> report.SystemReport {
  let source = read_value(root, p"proc/meminfo", max_bytes: 1048576)
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
  var counters: List[report.MemoryCounter] = []
  var issues = base.issues
  if source.observation.state != report.Observed or source.observation.value == null {
    issues = issues.push(issue("memory", "meminfo", source.observation.state, source.error_kind, source.errno))
  } else if source.observation.value.trim() == "" {
    issues = issues.push(issue("memory", "meminfo", report.Malformed, "empty_meminfo", null))
  } else {
    let meminfo_lines = source.observation.value.lines()
    var seen_names = set.empty()
    var duplicate_names = set.empty()
    for line in meminfo_lines {
      let pair = line.split(":", maxsplit: 1)
      continue when pair.len() != 2
      let name = pair[0].trim()
      continue when name == ""
      if set.has(seen_names, name) and ! set.has(duplicate_names, name) {
        duplicate_names = set.add(duplicate_names, name)
        issues = issues.push(issue("memory", f"meminfo.${name}", report.Malformed, "duplicate_field", null))
      }

      seen_names = set.add(seen_names, name)
    }

    for line_item in meminfo_lines |> enumerate() {
      let line = line_item.value
      continue when line.trim() == ""
      let pair = line.split(":", maxsplit: 1)
      if pair.len() != 2 {
        issues = issues.push(
          issue("memory", f"meminfo.line.${line_item.index}", report.Malformed, "missing_field_separator", null),
        )
        continue
      }

      let name = pair[0].trim()
      continue when set.has(duplicate_names, name)
      let values = parse_words(pair[1].trim().replace("\t", " "))
      if values.len() == 0 {
        issues = issues.push(issue("memory", f"meminfo.${name}", report.Malformed, "missing_integer", null))
        continue
      }

      let parsed = parse_integer(values[0])
      let has_kib_unit = values.len() > 1 and values[1] == "kB"
      let byte_counter = name in [
        "MemTotal",
        "MemFree",
        "MemAvailable",
        "Buffers",
        "Cached",
        "Active",
        "Inactive",
        "Dirty",
        "Writeback",
        "SwapTotal",
        "SwapFree",
      ]
      if values.len() > 2 {
        issues = issues.push(issue("memory", f"meminfo.${name}", report.Malformed, "unexpected_meminfo_columns", null))
        continue
      }

      if parsed == null {
        let state = if decimal_identifier(values[0]) { report.RangeFailure } else { report.Malformed }
        let error_kind = if state == report.RangeFailure { "integer_out_of_range" } else { "invalid_integer" }
        issues = issues.push(issue("memory", f"meminfo.${name}", state, error_kind, null))
        continue
      }

      let parsed_value = parsed ?? -1
      if parsed_value < 0 {
        issues = issues.push(issue("memory", f"meminfo.${name}", report.Malformed, "negative_integer", null))
        continue
      }

      let maximum_source_value = if has_kib_unit { 8796093022207 } else { 9007199254740991 }
      if parsed_value > maximum_source_value {
        let error_kind = if has_kib_unit { "byte_count_out_of_range" } else { "json_integer_out_of_range" }
        issues = issues.push(issue("memory", f"meminfo.${name}", report.RangeFailure, error_kind, null))
        continue
      }

      if byte_counter and ! has_kib_unit {
        issues = issues.push(issue("memory", f"meminfo.${name}", report.Malformed, "invalid_byte_counter_unit", null))
        continue
      }

      let multiplier = if has_kib_unit { 1024 } else { 1 }
      let value = parsed_value * multiplier
      let unit = if has_kib_unit { "bytes" } else { values.get(1, "count") }
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
        _ => {}
      }
    }
  }

  let swaps_source = read_value(root, p"proc/swaps", max_bytes: 262144)
  var swaps: List[report.SwapDevice] = []
  var seen_swaps = set.empty()
  if swaps_source.observation.state != report.Observed or swaps_source.observation.value == null {
    issues = issues.push(
      issue("memory", "swaps", swaps_source.observation.state, swaps_source.error_kind, swaps_source.errno),
    )
  } else {
    let swap_lines = swaps_source.observation.value.lines()
    if swap_lines.len() == 0 or parse_words(swap_lines[0].replace("\t", " ")) != [
      "Filename",
      "Type",
      "Size",
      "Used",
      "Priority",
    ] {
      issues = issues.push(issue("memory", "swaps", report.Malformed, "invalid_swap_header", null))
    } else {
      for line in swap_lines |> drop(1) {
        let columns = parse_words(line.replace("\t", " "))
        if columns.len() != 5 {
          issues = issues.push(issue("memory", "swaps", report.Malformed, "invalid_swap_row", null))
          continue
        }

        let size_kib = parse_integer(columns[2])
        let used_kib = parse_integer(columns[3])
        let size_count = size_kib ?? -1
        let used_count = used_kib ?? -1
        if size_count < 0 or used_count < 0 {
          let overflow = size_kib == null and decimal_identifier(columns[2]) or used_kib == null and decimal_identifier(
            columns[3],
          )
          let state = if overflow { report.RangeFailure } else { report.Malformed }
          let error_kind = if overflow { "swap_counter_out_of_range" } else { "invalid_swap_counter" }
          issues = issues.push(issue("memory", "swaps", state, error_kind, null))
          continue
        }

        if size_count > 8796093022207 or used_count > 8796093022207 {
          issues = issues.push(issue("memory", "swaps", report.RangeFailure, "byte_count_overflow", null))
          continue
        }

        let priority = parse_integer(columns[4])
        if priority == null {
          issues = issues.push(issue("memory", "swaps", report.Malformed, "invalid_swap_priority", null))
          continue
        }

        if used_count > size_count {
          issues = issues.push(issue("memory", "swaps", report.Malformed, "invalid_swap_usage", null))
          continue
        }

        let name = decode_mount_field(columns[0])
        if set.has(seen_swaps, name) {
          issues = issues.push(issue("memory", "swaps", report.Malformed, "duplicate_swap_name", null))
          continue
        }

        seen_swaps = set.add(seen_swaps, name)
        swaps = swaps.push({
          name: {
            state: report.Observed,
            value: name,
            raw_bytes_base64: null,
          },
          kind: columns[1],
          size_bytes: size_count * 1024,
          used_bytes: used_count * 1024,
          priority: priority,
        })
      }
    }
  }

  var huge_pages: List[report.HugePagePool] = []
  let huge_listing = fs.root_children(root, p"sys/kernel/mm/hugepages", max_entries: 1024)?
  if huge_listing.state != "complete" and huge_listing.state != "absent" {
    issues = issues.push(
      issue(
        "memory",
        "huge_pages.enumeration",
        live_source_observation_state(huge_listing.state, false),
        huge_listing.error_kind,
        huge_listing.errno,
      ),
    )
  }

  for huge_path in huge_listing.children {
    continue unless huge_path.name().starts_with("hugepages-")
    let collected = read_huge_page_pool(root, huge_path, null, f"huge_pages.${huge_path.name()}")
    issues = issues.extend(collected.issues)
    if collected.pool != null {
      huge_pages = huge_pages.push(collected.pool.require(report.HugePagePool)?)
    }
  }

  let node_listing = fs.root_children(root, p"sys/devices/system/node", max_entries: 1024)?
  if node_listing.state != "complete" and node_listing.state != "absent" {
    issues = issues.push(
      issue(
        "memory",
        "numa.enumeration",
        live_source_observation_state(node_listing.state, false),
        node_listing.error_kind,
        node_listing.errno,
      ),
    )
  }

  for node_path in node_listing.children {
    continue unless node_path.name().starts_with("node")
    let node_suffix = (node_path.name().split("") |> drop(4)).join("")
    let node_id = parse_integer(node_suffix) ?? -1
    if node_id < 0 {
      issues = issues.push(issue("memory", f"numa.${node_path.name()}", report.Malformed, "invalid_node_id", null))
      continue
    }

    let node_huge_listing = fs.root_children(root, fp"${node_path}/hugepages", max_entries: 1024)?
    if node_huge_listing.state != "complete" and node_huge_listing.state != "absent" {
      issues = issues.push(
        issue(
          "memory",
          f"numa.${node_path.name()}.huge_pages",
          live_source_observation_state(node_huge_listing.state, false),
          node_huge_listing.error_kind,
          node_huge_listing.errno,
        ),
      )
    }

    for huge_path in node_huge_listing.children {
      continue unless huge_path.name().starts_with("hugepages-")
      let collected = read_huge_page_pool(
        root,
        huge_path,
        node_id,
        f"numa.${node_path.name()}/huge_pages/${huge_path.name()}",
      )
      issues = issues.extend(collected.issues)
      if collected.pool != null {
        huge_pages = huge_pages.push(collected.pool.require(report.HugePagePool)?)
      }
    }
  }

  let cgroup_data = collect_cgroups(root)
  issues = issues.extend(cgroup_data.issues)

  var transparent_huge_pages: List[Str] = []
  for name in ["enabled", "defrag"] {
    let policy = read_value(root, fp"sys/kernel/mm/transparent_hugepage/${name}", max_bytes: 4096)
    continue when policy.observation.state == report.Absent
    if policy.observation.state != report.Observed or policy.observation.value == null {
      issues = issues.push(
        issue("memory", f"transparent_huge_pages.${name}", policy.observation.state, policy.error_kind, policy.errno),
      )
      continue
    }

    let policy_text = policy.observation.value ?? ""
    match collectors.parse_thp_policy(policy_text) {
      Ok(_) => transparent_huge_pages = transparent_huge_pages.push(f"${name}=${policy_text}")
      Err(_) => issues = issues.push(
        issue("memory", f"transparent_huge_pages.${name}", report.Malformed, "invalid_thp_policy", null),
      )
    }
  }

  var pressure: List[report.PressureLine] = []
  for resource in ["cpu", "memory", "io"] {
    let pressure_source = read_value(root, fp"proc/pressure/${resource}", max_bytes: 16384)
    if pressure_source.observation.state != report.Observed or pressure_source.observation.value == null {
      issues = issues.push(
        issue(
          "memory",
          f"pressure.${resource}",
          pressure_source.observation.state,
          pressure_source.error_kind,
          pressure_source.errno,
        ),
      )
      continue
    }

    if (pressure_source.observation.value ?? "").trim() == "" {
      issues = issues.push(issue("memory", f"pressure.${resource}", report.Malformed, "empty_psi_source", null))
      continue
    }

    var seen_kinds: List[Str] = []
    for line in pressure_source.observation.value.lines() {
      let columns = parse_words(line.replace("\t", " "))
      if columns.len() != 5 or columns[0] not in ["some", "full"] {
        issues = issues.push(issue("memory", f"pressure.${resource}", report.Malformed, "invalid_psi_row", null))
        continue
      }

      let kind = columns[0]
      let field = f"pressure.${resource}.${kind}"
      if kind in seen_kinds {
        issues = issues.push(issue("memory", field, report.Malformed, "duplicate_psi_kind", null))
        continue
      }

      seen_kinds = seen_kinds.push(kind)
      var avg10: Str? = null
      var avg60: Str? = null
      var avg300: Str? = null
      var total_text: Str? = null
      var invalid = false
      var seen_fields: List[Str] = []
      for column in columns |> drop(1) {
        let pair = column.split("=", maxsplit: 1)
        if pair.len() != 2 or pair[0] in seen_fields or pair[0] not in ["avg10", "avg60", "avg300", "total"] {
          invalid = true
          continue
        }

        seen_fields = seen_fields.push(pair[0])
        match pair[0] {
          "avg10" => avg10 = pair[1]
          "avg60" => avg60 = pair[1]
          "avg300" => avg300 = pair[1]
          "total" => total_text = pair[1]
          _ => {}
        }
      }

      if invalid or avg10 == null or avg60 == null or avg300 == null or total_text == null {
        issues = issues.push(issue("memory", field, report.Malformed, "invalid_psi_row", null))
        continue
      }

      if ! collectors.valid_psi_average(avg10 ?? "") or ! collectors.valid_psi_average(avg60 ?? "") or ! collectors.valid_psi_average(
        avg300 ?? "",
      ) {
        issues = issues.push(issue("memory", field, report.Malformed, "invalid_psi_average", null))
        continue
      }

      let total_us = parse_integer(total_text)
      let total_count = total_us ?? -1
      if total_count < 0 or total_count > 9007199254740991 {
        let range = total_us == null and decimal_identifier(total_text ?? "") or total_count > 9007199254740991
        let state = if range { report.RangeFailure } else { report.Malformed }
        issues = issues.push(issue("memory", field, state, "invalid_psi_total", null))
        continue
      }

      pressure = pressure.push({
        resource: resource,
        kind: kind,
        avg10: avg10,
        avg60: avg60,
        avg300: avg300,
        total_us: total_count,
      })
    }
  }

  var numa: List[report.MemoryCounter] = []
  for node_path in node_listing.children {
    continue unless node_path.name().starts_with("node")
    let node_id = parse_integer((node_path.name().split("") |> drop(4)).join("")) ?? -1
    continue when node_id < 0
    let numa_source = read_value(root, fp"${node_path}/meminfo", max_bytes: 65536)
    if numa_source.observation.state != report.Observed or numa_source.observation.value == null {
      issues = issues.push(
        issue(
          "memory",
          f"numa.${node_path.name()}.meminfo",
          numa_source.observation.state,
          numa_source.error_kind,
          numa_source.errno,
        ),
      )
      continue
    }

    for line_item in numa_source.observation.value.lines() |> enumerate() {
      let line = line_item.value
      let pair = line.split(":", maxsplit: 1)
      if pair.len() != 2 {
        issues = issues.push(
          issue(
            "memory",
            f"numa.${node_path.name()}.meminfo.line.${line_item.index}",
            report.Malformed,
            "missing_field_separator",
            null,
          ),
        )
        continue
      }

      let labels = parse_words(pair[0].trim().replace("\t", " "))
      let field_name = labels.get(2, "")
      if labels.len() != 3 or labels[0] != "Node" or labels[1] != f"${node_id}" or field_name == "" {
        let field = if field_name != "" {
          f"numa.${node_path.name()}.${field_name}"
        } else {
          f"numa.${node_path.name()}.meminfo.line.${line_item.index}"
        }
        issues = issues.push(issue("memory", field, report.Malformed, "invalid_numa_row_identity", null))
        continue
      }

      let field = f"numa.${node_path.name()}.${field_name}"
      let values = parse_words(pair[1].trim().replace("\t", " "))
      if values.len() < 1 or values.len() > 2 {
        issues = issues.push(issue("memory", field, report.Malformed, "invalid_numa_columns", null))
        continue
      }

      let parsed = parse_integer(values[0])
      let raw = parsed ?? -1
      if raw < 0 {
        let state = if parsed == null and decimal_identifier(values[0]) {
          report.RangeFailure
        } else {
          report.Malformed
        }
        issues = issues.push(issue("memory", field, state, "invalid_numa_counter", null))
        continue
      }

      let kib = values.len() == 2 and values[1] == "kB"
      let maximum = if kib { 8796093022207 } else { 9007199254740991 }
      if raw > maximum {
        issues = issues.push(issue("memory", field, report.RangeFailure, "byte_count_overflow", null))
        continue
      }

      let value = if kib { raw * 1024 } else { raw }
      let unit = if kib { "bytes" } else { "count" }
      numa = numa.push({name: f"node${node_id}.${field_name}", value: value, unit: unit})
    }
  }

  var state = if source.observation.state == report.Observed { report.Complete } else { report.Partial }
  if total == null or issues.len() > base.issues.len() {
    state = report.Partial
  }

  let memory: report.MemorySection = {
    status: {
      state: state,
      enumeration_succeeded: source.observation.state == report.Observed,
    },
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

  return (components |> take(components.len() - 1)).join("/")
}

pure cgroup_observation_state(
  maximum: collectors.SourceRead,
  current: collectors.SourceRead,
) -> report.ObservationState {
  if maximum.observation.state != report.Observed {
    return maximum.observation.state
  }

  return current.observation.state
}

pure cgroup_numeric_state(
  maximum: collectors.SourceRead,
  current: collectors.SourceRead,
  maximum_number: collectors.BoundedNumber,
  current_number: collectors.BoundedNumber,
) -> report.ObservationState {
  if maximum_number.state != null {
    return maximum_number.state ?? report.Malformed
  }

  if current_number.state != null {
    return current_number.state ?? report.Malformed
  }

  return cgroup_observation_state(maximum, current)
}

pure cgroup_resource(
  visible_path: Str,
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
    path: {
      state: report.Observed,
      value: visible_path,
      raw_bytes_base64: null,
    },
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

pure cgroup_unlimited(source: collectors.SourceRead) -> Bool? {
  if source.observation.state != report.Observed or source.observation.value == null {
    return null
  }

  return source.observation.value == "max"
}

pure cgroup_maximum(source: collectors.SourceRead) -> collectors.BoundedNumber {
  if source.observation.state == report.Observed and source.observation.value == "max" {
    return {value: null, state: null, error_kind: null, errno: null}
  }

  return collectors.bounded_number(source, true)
}

pure cgroup_token_number(source: collectors.SourceRead, token: Str) -> collectors.BoundedNumber {
  return collectors.bounded_number({...source, observation: {...source.observation, value: token}}, true)
}

pure first_errno(primary: Int?, secondary: Int?) -> Int? {
  if primary != null {
    return primary
  }

  return secondary
}

proc collect_cgroups(root: FsRoot) [fs, error] -> CgroupCollection {
  let self_cgroup = read_value(root, p"proc/self/cgroup", max_bytes: 65536)
  let mountinfo = read_value(root, p"proc/self/mountinfo", max_bytes: 4194304)
  var issues: List[report.CollectionIssue] = []
  if self_cgroup.observation.state != report.Observed or self_cgroup.observation.value == null {
    issues = issues.push(
      issue("memory", "cgroup.membership", self_cgroup.observation.state, self_cgroup.error_kind, self_cgroup.errno),
    )
    return {resources: [], issues: issues}
  }

  if mountinfo.observation.state != report.Observed or mountinfo.observation.value == null {
    issues = issues.push(
      issue("memory", "cgroup.mountinfo", mountinfo.observation.state, mountinfo.error_kind, mountinfo.errno),
    )
    return {resources: [], issues: issues}
  }

  let parsed_membership = collectors.parse_unified_cgroup_path(self_cgroup.observation.value ?? "")
  if parsed_membership.state == report.Malformed {
    issues = issues.push(issue("memory", "cgroup.membership", report.Malformed, "invalid_cgroup_membership", null))
    return {resources: [], issues: issues}
  }

  let cgroup_path = parsed_membership.path
  let inventory = cgroup_mount_inventory(mountinfo.observation.value ?? "")
  if inventory.malformed {
    issues = issues.push(issue("memory", "cgroup.mountinfo", report.Malformed, "invalid_cgroup_mountinfo", null))
    return {resources: [], issues: issues}
  }

  let has_v1 = parsed_membership.has_v1 or inventory.has_v1

  var resources: List[report.CgroupResource] = []
  if cgroup_path == null or inventory.mounts.len() == 0 {
    if has_v1 {
      issues = issues.push(issue("memory", "cgroup.v1", report.Unsupported, "cgroup_v1_or_hybrid", null))
    } else {
      let state = if self_cgroup.observation.state != report.Observed {
        self_cgroup.observation.state
      } else {
        report.Absent
      }
      issues = issues.push(issue("memory", "cgroup_v2", state, "cgroup_v2_mount_unavailable", self_cgroup.errno))
    }

    return {resources: resources, issues: issues}
  }

  if has_v1 {
    issues = issues.push(issue("memory", "cgroup.v1", report.Unsupported, "cgroup_v1_controllers_present", null))
  }

  let group_name = cgroup_path ?? ""
  var selected: collectors.CgroupMount? = null
  match collectors.select_cgroup_mount(group_name, inventory.mounts) {
    Ok(mount) => selected = mount
    Err(_) => {
      issues = issues.push(issue("memory", "cgroup.path", report.Malformed, "invalid_cgroup_mount_path", null))
      return {resources: resources, issues: issues}
    }
  }

  if selected == null {
    issues = issues.push(issue("memory", "cgroup.path", report.Unsupported, "cgroup_path_outside_visible_mount", null))
    return {resources: resources, issues: issues}
  }

  let chosen = selected ?? {root: "", point: ""}
  let root_path = chosen.root
  let absolute_mount_point = chosen.point
  if ! group_name.starts_with("/") {
    issues = issues.push(issue("memory", "cgroup.path", report.Malformed, "invalid_cgroup_mount_path", null))
    return {resources: resources, issues: issues}
  }

  var relative = ""
  if root_path == "/" {
    relative = (group_name.split("") |> drop(1)).join("")
  } else if group_name == root_path {
    relative = ""
  } else if group_name.starts_with(f"${root_path}/") {
    let prefix_length = root_path.count_chars() + 1
    relative = (group_name.split("") |> drop(prefix_length)).join("")
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
    let visible_path = if current_relative == "" {
      root_path
    } else if root_path == "/" {
      f"/${current_relative}"
    } else {
      f"${root_path}/${current_relative}"
    }

    let memory_max = read_value(root, fp"${source_path}/memory.max", max_bytes: 4096)
    let memory_current = read_value(root, fp"${source_path}/memory.current", max_bytes: 4096)
    if memory_max.observation.state != report.Absent or memory_current.observation.state != report.Absent {
      let maximum = cgroup_maximum(memory_max)
      let current = collectors.bounded_number(memory_current, true)
      let state = cgroup_numeric_state(memory_max, memory_current, maximum, current)
      resources = resources.push(
        cgroup_resource(
          visible_path,
          hierarchy_level,
          "memory",
          "memory.max",
          state,
          maximum.value,
          current.value,
          "bytes",
          cgroup_unlimited(memory_max),
          null,
          null,
          [],
        ),
      )
      issues = append_number_issue(issues, "memory", f"cgroup.${hierarchy_level}.memory.max", maximum)
      issues = append_number_issue(issues, "memory", f"cgroup.${hierarchy_level}.memory.current", current)
      if memory_max.observation.state != report.Observed or memory_current.observation.state != report.Observed {
        issues = issues.push(
          issue(
            "memory",
            f"cgroup.${hierarchy_level}.memory",
            state,
            "cgroup_memory_value_unavailable",
            first_errno(memory_max.errno, memory_current.errno),
          ),
        )
      }
    }

    let memory_swap_max = read_value(root, fp"${source_path}/memory.swap.max", max_bytes: 4096)
    let memory_swap_current = read_value(root, fp"${source_path}/memory.swap.current", max_bytes: 4096)
    if memory_swap_max.observation.state != report.Absent or memory_swap_current.observation.state != report.Absent {
      let maximum = cgroup_maximum(memory_swap_max)
      let current = collectors.bounded_number(memory_swap_current, true)
      let state = cgroup_numeric_state(memory_swap_max, memory_swap_current, maximum, current)
      resources = resources.push(
        cgroup_resource(
          visible_path,
          hierarchy_level,
          "memory",
          "memory.swap.max",
          state,
          maximum.value,
          current.value,
          "bytes",
          cgroup_unlimited(memory_swap_max),
          null,
          null,
          [],
        ),
      )
      issues = append_number_issue(issues, "memory", f"cgroup.${hierarchy_level}.memory.swap.max", maximum)
      issues = append_number_issue(issues, "memory", f"cgroup.${hierarchy_level}.memory.swap.current", current)
      if memory_swap_max.observation.state != report.Observed or memory_swap_current.observation.state != report.Observed {
        issues = issues.push(
          issue(
            "memory",
            f"cgroup.${hierarchy_level}.memory.swap",
            state,
            "cgroup_memory_swap_value_unavailable",
            first_errno(memory_swap_max.errno, memory_swap_current.errno),
          ),
        )
      }
    }

    let cpu_max = read_value(root, fp"${source_path}/cpu.max", max_bytes: 4096)
    if cpu_max.observation.state == report.Observed and cpu_max.observation.value != null {
      let fields = parse_words(cpu_max.observation.value)
      if fields.len() == 2 {
        let unlimited = fields[0] == "max"
        var quota_number: collectors.BoundedNumber = {value: null, state: null, error_kind: null, errno: null}
        if ! unlimited {
          quota_number = cgroup_token_number(cpu_max, fields[0])
        }

        let period_number = cgroup_token_number(cpu_max, fields[1])
        if quota_number.state != null or period_number.state != null {
          let state = if quota_number.state != null {
            quota_number.state ?? report.Malformed
          } else {
            period_number.state ?? report.Malformed
          }
          issues = issues.push(issue("memory", f"cgroup.${hierarchy_level}.cpu.max", state, "invalid_cpu_quota", null))
        } else if ! unlimited and (quota_number.value ?? -1) <= 0 or (period_number.value ?? -1) <= 0 {
          issues = issues.push(
            issue("memory", f"cgroup.${hierarchy_level}.cpu.max", report.Malformed, "invalid_cpu_quota", null),
          )
        } else {
          resources = resources.push(
            cgroup_resource(
              visible_path,
              hierarchy_level,
              "cpu",
              "cpu.max",
              report.Observed,
              null,
              null,
              "quota_period",
              unlimited,
              quota_number.value,
              period_number.value,
              [],
            ),
          )
        }
      } else {
        issues = issues.push(
          issue("memory", f"cgroup.${hierarchy_level}.cpu.max", report.Malformed, "invalid_cpu_max", null),
        )
      }
    } else if cpu_max.observation.state != report.Absent {
      issues = issues.push(
        issue("memory", f"cgroup.${hierarchy_level}.cpu.max", cpu_max.observation.state, cpu_max.error_kind, cpu_max.errno),
      )
    }

    let cpu_stat = read_value(root, fp"${source_path}/cpu.stat", max_bytes: 16384)
    if cpu_stat.observation.state == report.Observed and cpu_stat.observation.value != null {
      var seen_cpu_stat = set.empty()
      var duplicate_cpu_stat = set.empty()
      for line in cpu_stat.observation.value.lines() {
        let fields = parse_words(line)
        if fields.len() == 2 and fields[0] in [
          "usage_usec",
          "user_usec",
          "system_usec",
          "nr_periods",
          "nr_throttled",
          "throttled_usec",
          "nr_bursts",
          "burst_usec",
        ] {
          if set.has(seen_cpu_stat, fields[0]) and ! set.has(duplicate_cpu_stat, fields[0]) {
            duplicate_cpu_stat = set.add(duplicate_cpu_stat, fields[0])
            issues = issues.push(
              issue("memory", f"cgroup.${hierarchy_level}.cpu.stat.${fields[0]}", report.Malformed, "duplicate_cpu_stat_field", null),
            )
          }

          seen_cpu_stat = set.add(seen_cpu_stat, fields[0])
        }
      }

      for line in cpu_stat.observation.value.lines() {
        let fields = parse_words(line)
        if fields.len() != 2 {
          issues = issues.push(
            issue("memory", f"cgroup.${hierarchy_level}.cpu.stat", report.Malformed, "invalid_cpu_stat_row", null),
          )
          continue
        }

        continue when fields[0] not in [
          "usage_usec",
          "user_usec",
          "system_usec",
          "nr_periods",
          "nr_throttled",
          "throttled_usec",
          "nr_bursts",
          "burst_usec",
        ]
        continue when set.has(duplicate_cpu_stat, fields[0])
        let number = cgroup_token_number(cpu_stat, fields[1])
        if number.value == null {
          issues = issues.push(
            issue(
              "memory",
              f"cgroup.${hierarchy_level}.cpu.stat.${fields[0]}",
              number.state ?? report.Malformed,
              "invalid_cpu_stat_value",
              null,
            ),
          )
          continue
        }

        let unit = if fields[0].starts_with("nr_") { "count" } else { "microseconds" }
        resources = resources.push(
          cgroup_resource(
            visible_path,
            hierarchy_level,
            "cpu",
            f"cpu.stat.${fields[0]}",
            report.Observed,
            null,
            number.value,
            unit,
            null,
            null,
            null,
            [],
          ),
        )
      }
    } else if cpu_stat.observation.state != report.Absent {
      issues = issues.push(
        issue("memory", f"cgroup.${hierarchy_level}.cpu.stat", cpu_stat.observation.state, cpu_stat.error_kind, cpu_stat.errno),
      )
    }

    let cpuset = read_value(root, fp"${source_path}/cpuset.cpus.effective", max_bytes: 65536)
    if cpuset.observation.state == report.Observed and cpuset.observation.value != null {
      if cpuset.observation.value == "" {
        resources = resources.push(
          cgroup_resource(
            visible_path,
            hierarchy_level,
            "cpuset",
            "cpuset.cpus.effective",
            report.Observed,
            null,
            null,
            "cpu_ids",
            null,
            null,
            null,
            [],
          ),
        )
      } else {
        match report.parse_cpu_list(cpuset.observation.value ?? "") {
          Ok(cpus) => resources = resources.push(
            cgroup_resource(
              visible_path,
              hierarchy_level,
              "cpuset",
              "cpuset.cpus.effective",
              report.Observed,
              null,
              null,
              "cpu_ids",
              null,
              null,
              null,
              cpus,
            ),
          )
          Err(_) => issues = issues.push(
            issue("memory", f"cgroup.${hierarchy_level}.cpuset.cpus.effective", report.Malformed, "invalid_cgroup_cpu_list", null),
          )
        }
      }
    } else if cpuset.observation.state != report.Absent {
      issues = issues.push(
        issue(
          "memory",
          f"cgroup.${hierarchy_level}.cpuset.cpus.effective",
          cpuset.observation.state,
          cpuset.error_kind,
          cpuset.errno,
        ),
      )
    }

    let pids_max = read_value(root, fp"${source_path}/pids.max", max_bytes: 4096)
    let pids_current = read_value(root, fp"${source_path}/pids.current", max_bytes: 4096)
    if pids_max.observation.state != report.Absent or pids_current.observation.state != report.Absent {
      let maximum = cgroup_maximum(pids_max)
      let current = collectors.bounded_number(pids_current, true)
      let state = cgroup_numeric_state(pids_max, pids_current, maximum, current)
      resources = resources.push(
        cgroup_resource(
          visible_path,
          hierarchy_level,
          "pids",
          "pids.max",
          state,
          maximum.value,
          current.value,
          "count",
          cgroup_unlimited(pids_max),
          null,
          null,
          [],
        ),
      )
      issues = append_number_issue(issues, "memory", f"cgroup.${hierarchy_level}.pids.max", maximum)
      issues = append_number_issue(issues, "memory", f"cgroup.${hierarchy_level}.pids.current", current)
      if pids_max.observation.state != report.Observed or pids_current.observation.state != report.Observed {
        issues = issues.push(
          issue(
            "memory",
            f"cgroup.${hierarchy_level}.pids",
            state,
            "cgroup_pids_value_unavailable",
            first_errno(pids_max.errno, pids_current.errno),
          ),
        )
      }
    }

    let io_stat = read_value(root, fp"${source_path}/io.stat", max_bytes: 262144)
    if io_stat.observation.state == report.Observed and io_stat.observation.value != null {
      var seen_io_devices = set.empty()
      var duplicate_io_devices = set.empty()
      for line in io_stat.observation.value.lines() {
        let fields = parse_words(line)
        if fields.len() >= 1 {
          if set.has(seen_io_devices, fields[0]) and ! set.has(duplicate_io_devices, fields[0]) {
            duplicate_io_devices = set.add(duplicate_io_devices, fields[0])
            issues = issues.push(
              issue("memory", f"cgroup.${hierarchy_level}.io.stat.${fields[0]}", report.Malformed, "duplicate_io_device", null),
            )
          }

          seen_io_devices = set.add(seen_io_devices, fields[0])
        }
      }

      for line in io_stat.observation.value.lines() {
        let fields = parse_words(line)
        continue when fields.len() == 0
        let device_parts = fields[0].split(":")
        if device_parts.len() != 2 or cgroup_token_number(io_stat, device_parts[0]).value == null or cgroup_token_number(
          io_stat,
          device_parts[1],
        ).value == null {
          issues = issues.push(
            issue("memory", f"cgroup.${hierarchy_level}.io.stat", report.Malformed, "invalid_io_device", null),
          )
          continue
        }

        continue when set.has(duplicate_io_devices, fields[0])
        var seen_io_fields = set.empty()
        var duplicate_io_fields = set.empty()
        for item in fields |> drop(1) {
          let pair = item.split("=", maxsplit: 1)
          if pair.len() == 2 and pair[0] in ["rbytes", "wbytes", "rios", "wios", "dbytes", "dios"] {
            if set.has(seen_io_fields, pair[0]) and ! set.has(duplicate_io_fields, pair[0]) {
              duplicate_io_fields = set.add(duplicate_io_fields, pair[0])
              issues = issues.push(
                issue(
                  "memory",
                  f"cgroup.${hierarchy_level}.io.stat.${fields[0]}.${pair[0]}",
                  report.Malformed,
                  "duplicate_io_counter",
                  null,
                ),
              )
            }

            seen_io_fields = set.add(seen_io_fields, pair[0])
          }
        }

        for item in fields |> drop(1) {
          let pair = item.split("=", maxsplit: 1)
          continue when pair.len() != 2 or pair[0] not in ["rbytes", "wbytes", "rios", "wios", "dbytes", "dios"]
          continue when set.has(duplicate_io_fields, pair[0])
          let number = cgroup_token_number(io_stat, pair[1])
          if number.value == null {
            issues = issues.push(
              issue(
                "memory",
                f"cgroup.${hierarchy_level}.io.stat.${fields[0]}.${pair[0]}",
                number.state ?? report.Malformed,
                "invalid_io_counter",
                null,
              ),
            )
            continue
          }

          let unit = if pair[0].ends_with("bytes") { "bytes" } else { "requests" }
          resources = resources.push(
            cgroup_resource(
              visible_path,
              hierarchy_level,
              "io",
              f"io.stat.${fields[0]}.${pair[0]}",
              report.Observed,
              null,
              number.value,
              unit,
              null,
              null,
              null,
              [],
            ),
          )
        }
      }
    } else if io_stat.observation.state != report.Absent {
      issues = issues.push(
        issue("memory", f"cgroup.${hierarchy_level}.io.stat", io_stat.observation.state, io_stat.error_kind, io_stat.errno),
      )
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
  let digits = "0123456789abcdef".split("")
  for octet_bytes in value.chunks(1) {
    match bytes.unpack_le(octet_bytes, 1, 0) {
      Ok(octet) => parts = parts.push(f"${digits[octet / 16]}${digits[octet % 16]}")
      Err(_) => return empty_text(report.Malformed)
    }
  }

  return {state: report.Observed, value: parts.join(":"), raw_bytes_base64: null}
}

pure network_attributes(values: List[LinuxNetlinkAttribute]) -> List[report.NetworkAttribute] {
  [{
    kind: attribute.kind,
    data: {
      state: report.Observed,
      value: attribute.data.base64(),
      raw_bytes_base64: null,
    },
  } for attribute in values]
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
  if value.bit_and(1) != 0 {
    flags = flags.push("up")
  }

  if value.bit_and(2) != 0 {
    flags = flags.push("broadcast")
  }

  if value.bit_and(8) != 0 {
    flags = flags.push("loopback")
  }

  if value.bit_and(16) != 0 {
    flags = flags.push("point_to_point")
  }

  if value.bit_and(64) != 0 {
    flags = flags.push("running")
  }

  if value.bit_and(256) != 0 {
    flags = flags.push("promiscuous")
  }

  if value.bit_and(4096) != 0 {
    flags = flags.push("multicast")
  }

  if value.bit_and(65536) != 0 {
    flags = flags.push("lower_up")
  }

  if value.bit_and(131072) != 0 {
    flags = flags.push("dormant")
  }

  flags = flags.push(f"raw_bits=${value}")
  return flags
}

pure network_operstate(value: Int?) -> Str? {
  if value == null {
    return null
  }

  match value {
    0 => return "unknown"
    1 => return "not_present"
    2 => return "down"
    3 => return "lower_layer_down"
    4 => return "testing"
    5 => return "dormant"
    6 => return "up"
    _ => return f"operstate_${value ?? -1}"
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
    "unsupported" => return report.SectionUnsupported
    "permission_denied" => return report.SectionPermissionDenied
    "malformed" => return report.SectionMalformed
    "truncated" => return report.SectionTruncated
    "limited" => return report.SectionTruncated
    "interrupted" => return report.SectionRaced
    _ => return report.Partial
  }
}

pure network_issue_state(value: Str) -> report.ObservationState {
  match value {
    "permission_denied" => return report.PermissionDenied
    "unsupported" => return report.Unsupported
    "malformed" => return report.Malformed
    "range_failure" => return report.RangeFailure
    "truncated" => return report.Truncated
    "limited" => return report.Truncated
    "interrupted" => return report.Raced
    _ => return report.ReadFailure
  }
}

pure link_index_by_name(links: Map[Int], name: Str?) -> Int? {
  if name == null {
    return null
  }

  let link_name = name ?? ""
  if links.has(link_name) {
    return links.get(link_name, 0)
  }

  return null
}

## Converts one typed route-netlink result into the report's stable network model.
export pure assemble_network_dump(value: LinuxNetworkDump) -> NetworkCollection {
  var addresses_by_link: Map[List[report.NetworkAddress]] = {}
  for raw_address in value.addresses {
    let address_value = if raw_address.local != null { raw_address.local } else { raw_address.address }
    let link_key = f"${raw_address.ifindex}"
    let address: report.NetworkAddress = {
      family: network_family(raw_address.family),
      address: network_text(address_value),
      prefix_length: raw_address.prefix_length,
      broadcast: network_text(raw_address.broadcast),
      scope: network_address_scope(raw_address.scope),
      flags: raw_address.flags,
      valid_lifetime_seconds: raw_address.valid_lifetime_seconds,
      preferred_lifetime_seconds: raw_address.preferred_lifetime_seconds,
      attributes: network_attributes(raw_address.attributes),
    }
    addresses_by_link = addresses_by_link.push(link_key, address)
  }

  var links: List[report.NetworkLink] = []
  for raw_link in value.links {
    let addresses = addresses_by_link.get(f"${raw_link.ifindex}", [])

    var counters: List[report.MemoryCounter] = []
    if raw_link.rx_bytes != null {
      counters = counters.push({name: "rx_bytes", value: raw_link.rx_bytes ?? 0, unit: "bytes"})
    }

    if raw_link.tx_bytes != null {
      counters = counters.push({name: "tx_bytes", value: raw_link.tx_bytes ?? 0, unit: "bytes"})
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
    let default_destination: Str? = if family == "ipv6" { "::" } else if family == "ipv4" { "0.0.0.0" } else { null }
    let destination = if raw_route.destination != null { raw_route.destination } else { default_destination }
    var nexthops = [
      {
        ifindex: nexthop.ifindex,
        flags: nexthop.flags,
        hops: nexthop.hops,
        gateway: network_text(nexthop.gateway),
      }
      for nexthop in raw_route.nexthops
    ]
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

  var link_indices_by_name: Map[Int] = {}
  for raw_link in value.links {
    if raw_link.name != null {
      let link_name = raw_link.name ?? ""
      if ! link_indices_by_name.has(link_name) {
        link_indices_by_name = link_indices_by_name.set(link_name, raw_link.ifindex)
      }
    }
  }

  var rules = [
    {
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
      input_ifindex: link_index_by_name(link_indices_by_name, raw_rule.input_name),
      output_ifindex: link_index_by_name(link_indices_by_name, raw_rule.output_name),
      flags: raw_rule.flags,
      attributes: network_attributes(raw_rule.attributes),
    }
    for raw_rule in value.rules
  ]
  var issues = [
    issue_with_detail(
      "network",
      f"netlink.${raw_issue.object}",
      network_issue_state(raw_issue.state),
      raw_issue.error_kind,
      raw_issue.errno,
      raw_issue.message,
    )
    for raw_issue in value.issues
  ]
  return {
    status: {
      state: network_section_state(value.state),
      enumeration_succeeded: value.enumeration_succeeded,
    },
    links: links,
    routes: routes,
    rules: rules,
    issues: issues,
  }
}

pure source_error_kind(failure: Error) -> Str {
  match failure {
    is PermissionDenied => return "permission_denied"
    is NotFound => return "not_found"
    _ => return "read_failure"
  }
}

proc network_link_target(root: FsRoot, source_path: Path, field: Str) [fs, error] -> NetworkLinkTarget {
  match fs.root_readlink_result(root, source_path) {
    Ok(observed) => {
      if observed.state == "observed" {
        if observed.target != null {
          return {target: observed.target, issues: []}
        }

        return {target: null, issues: [issue("network", field, report.Malformed, "missing_link_target", null)]}
      }

      if observed.state == "absent" {
        return {target: null, issues: []}
      }

      return {
        target: null,
        issues: [
          issue(
            "network",
            field,
            live_source_observation_state(observed.state, false),
            observed.error_kind,
            observed.errno,
          ),
        ],
      }
    }
    Err(_) => return {target: null, issues: [issue("network", field, report.Malformed, "invalid_link_path", null)]}
  }
}

## Joins network interfaces to visible device sources without treating missing links as failures.
export proc link_network_device_sources(
  root: FsRoot,
  assembled: NetworkCollection,
  pci_functions: List[report.PciFunction],
  usb_devices: List[report.UsbDevice],
) [fs, error] -> NetworkCollection {
  let pci_indices = pci_function_indices(pci_functions)
  let usb_indices = usb_device_indices(usb_devices)
  var links: List[report.NetworkLink] = []
  var issues = assembled.issues
  for link in assembled.links {
    var driver: Str? = null
    var parent_pci_function_index: Int? = null
    var parent_usb_device_index: Int? = null
    if link.name.value != null {
      let name = link.name.value ?? ""
      let driver_link = network_link_target(
        root,
        fp"sys/class/net/${name}/device/driver",
        f"links.${link.ifindex}.driver",
      )
      issues = issues.extend(driver_link.issues)
      if driver_link.target != null {
        driver = (driver_link.target ?? p"").name()
      }

      let device_link = network_link_target(root, fp"sys/class/net/${name}/device", f"links.${link.ifindex}.parent")
      issues = issues.extend(device_link.issues)
      if device_link.target != null {
        let target = device_link.target ?? p""
        parent_pci_function_index = pci_function_index(pci_indices, usb_parent_address(target))
        parent_usb_device_index = usb_device_index_from_target(usb_indices, target)
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

proc collect_network(
  root: FsRoot,
  pci_functions: List[report.PciFunction],
  usb_devices: List[report.UsbDevice],
) [fs, process, env, error] -> NetworkCollection {
  var result: Result[LinuxNetworkDump] = Err(
    report.SystemReportError.InvalidJson(message: "network dump was not collected"),
  )
  env XSH_LINUX_REAL="1" {
    result = linux.network_dump()
  }
  match result {
    Ok(value) => {
      let assembled = assemble_network_dump(value)
      return link_network_device_sources(root, assembled, pci_functions, usb_devices)
    }
    Err(failure) => {
      let collection_issue = issue_with_detail(
        "network",
        "netlink",
        report.ReadFailure,
        source_error_kind(failure),
        null,
        failure.message,
      )
      return {
        status: {
          state: report.Partial,
          enumeration_succeeded: false,
        },
        links: [],
        routes: [],
        rules: [],
        issues: [
          collection_issue,
        ],
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
export proc collect_from_root(
  root: FsRoot,
  architecture: Str,
  page_size_bytes: Int,
  clock_ticks_per_second: Int,
  selected: Str = "",
  sensitive: Bool = false,
  include_local_mount_usage: Bool = false,
) [fs, time, error] -> Result[report.SystemReport] {
  if page_size_bytes <= 0 or clock_ticks_per_second <= 0 {
    return Err(
      report.SystemReportError.InvalidExecutionUnits(message: "page size and clock ticks per second must be positive"),
    )
  }

  if selected != "" {
    report.parse_report_section(selected)?
  }

  let started = time.now()
  var output = empty_report(started, page_size_bytes, clock_ticks_per_second)
  output = collect_identity(root, output)
  output = {...output, identity: {...output.identity, architecture: architecture}}
  if requested(selected, "cpu") {
    output = collect_cpu(root, output)
  }

  if requested(selected, "memory") {
    output = collect_memory(root, output)
  }

  let pci_dependency = selected == "usb" or selected == "storage" or selected == "network" or selected == "devices" or selected == "sensors"
  if requested(selected, "pci") or pci_dependency {
    let pci = collectors.collect_pci(root)
    output = {...output, pci: {status: pci.status, functions: pci.functions}, issues: output.issues.extend(pci.issues)}
  }

  if requested(selected, "usb") or selected == "network" or selected == "devices" or selected == "sensors" {
    let usb = collect_usb(root, output.pci.functions)
    output = {...output, usb: {status: usb.status, devices: usb.devices}, issues: output.issues.extend(usb.issues)}
  }

  if requested(selected, "storage") {
    let storage = collect_storage(root, output.pci.functions, include_local_mount_usage)
    output = {
      ...output,
      storage: {
        status: storage.status,
        devices: storage.devices,
        mounts: storage.mounts,
      },
      issues: output.issues.extend(storage.issues),
    }
  }

  if requested(selected, "sensors") {
    let sensors = collect_sensors(root, output.pci.functions, output.usb.devices)
    output = {
      ...output,
      sensors: {
        status: sensors.status,
        channels: sensors.channels,
        thermal_zones: sensors.thermal_zones,
      },
      issues: output.issues.extend(sensors.issues),
    }
  }

  if requested(selected, "power") {
    let power = collect_power(root)
    output = {
      ...output,
      power: {
        status: power.status,
        supplies: power.supplies,
        cap_zones: power.cap_zones,
      },
      issues: output.issues.extend(power.issues),
    }
  }

  if requested(selected, "processes") {
    let processes = collect_processes(root, page_size_bytes)
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
    let kernel = collect_kernel(root)
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
    let firmware = collect_firmware(root)
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
    let devices = collect_device_classes(root, output.pci.functions, output.usb.devices)
    output = {
      ...output,
      devices: {
        status: devices.status,
        devices: devices.devices,
      },
      issues: output.issues.extend(devices.issues),
    }
  }

  for section_item in [
    {
      name: "usb",
      state: output.usb.status.state,
    },
    {
      name: "storage",
      state: output.storage.status.state,
    },
    {
      name: "network",
      state: output.network.status.state,
    },
    {
      name: "sensors",
      state: output.sensors.status.state,
    },
    {
      name: "power",
      state: output.power.status.state,
    },
    {
      name: "firmware",
      state: output.firmware.status.state,
    },
    {
      name: "kernel",
      state: output.kernel.status.state,
    },
    {
      name: "processes",
      state: output.processes.status.state,
    },
    {
      name: "devices",
      state: output.devices.status.state,
    },
  ] {
    let name = section_item.name
    let state = section_item.state
    if requested(selected, name) and state == report.SectionNotRequested {
      continue when name == "network" and include_local_mount_usage
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
  if ! sensitive {
    output = report.redact_report(output)
  }

  return Ok(output)
}

pure mark_unsupported(value: report.SystemReport, name: Str) -> report.SystemReport {
  var issues = value.issues
  issues = issues.push(unsupported_issue(name, "section"))
  match name {
    "usb" => return {...value, usb: {status: empty_status(report.SectionUnsupported), devices: []}, issues: issues}
    "storage" => return {
      ...value,
      storage: {
        status: empty_status(report.SectionUnsupported),
        devices: [],
        mounts: [],
      },
      issues: issues,
    }
    "network" => return {
      ...value,
      network: {
        status: empty_status(report.SectionUnsupported),
        links: [],
        routes: [],
        rules: [],
      },
      issues: issues,
    }
    "sensors" => return {
      ...value,
      sensors: {
        status: empty_status(report.SectionUnsupported),
        channels: [],
        thermal_zones: [],
      },
      issues: issues,
    }
    "power" => return {
      ...value,
      power: {
        status: empty_status(report.SectionUnsupported),
        supplies: [],
        cap_zones: [],
      },
      issues: issues,
    }
    "firmware" => return {
      ...value,
      firmware: {
        status: empty_status(report.SectionUnsupported),
        source: "unavailable",
        records: [],
        limitation: empty_text(report.Unsupported),
      },
      issues: issues,
    }
    "kernel" => return {
      ...value,
      kernel: {
        status: empty_status(report.SectionUnsupported),
        command_line: empty_text(report.Unsupported),
        modules: [],
        parameters: [],
        sysctls: [],
      },
      issues: issues,
    }
    "processes" => return {
      ...value,
      processes: {
        status: empty_status(report.SectionUnsupported),
        processes: [],
      },
      issues: issues,
    }
    "devices" => return {
      ...value,
      devices: {
        status: empty_status(report.SectionUnsupported),
        devices: [],
      },
      issues: issues,
    }
    _ => return value
  }
}

## Exposes fixture-root and live Linux collection through the same typed report.
export type SystemReportLiveCollector = module {
  export proc collect_from_root(root: FsRoot, architecture: Str, page_size_bytes: Int, clock_ticks_per_second: Int, selected: Str = "", sensitive: Bool = false, include_local_mount_usage: Bool = false) [fs, time, error] -> Result[report.SystemReport]
  export proc collect_live(selected: Str = "", sensitive: Bool = false) [fs, process, env, time, error] -> Result[report.SystemReport]
  export proc link_network_device_sources(root: FsRoot, assembled: NetworkCollection, pci_functions: List[report.PciFunction], usb_devices: List[report.UsbDevice]) [fs, error] -> NetworkCollection
  export proc optional_driver_name(root: FsRoot, source_path: Path) [fs, error] -> collectors.SourceRead
  export proc usb_controller_address(root: FsRoot, device_path: Path) [fs, error] -> UsbControllerObservation
  export proc class_parent_target(root: FsRoot, entry: Path) [fs, error] -> ClassParentObservation
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
export proc collect_live(
  selected: Str = "",
  sensitive: Bool = false,
) [fs, process, env, time, error] -> Result[report.SystemReport] {
  if dry_run_enabled() {
    return Err(
      report.SystemReportError.DryRun(message: "system-report refuses live collection while XSH Linux dry-run mode is active"),
    )
  }

  let uname = system.uname()?
  if uname.sysname != "Linux" {
    return Err(
      report.SystemReportError.UnsupportedPlatform(message: "live system-report collection is supported on Linux only"),
    )
  }

  let root = fs.open_root(/)?
  defer fs.close_root(root)?
  let units = system.execution_units()?
  var collected = collect_from_root(
    root,
    uname.machine,
    units.page_size_bytes,
    units.clock_ticks_per_second,
    selected,
    true,
    true,
  )?
  if requested(selected, "network") {
    let network = collect_network(root, collected.pci.functions, collected.usb.devices)
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
    identity: {
      ...collected.identity,
      architecture: uname.machine,
    },
  }
  if ! sensitive {
    collected = report.redact_report(collected)
  }

  return Ok(collected)
}
