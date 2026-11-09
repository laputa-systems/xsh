##! Collects a bounded, process-visible Linux report from one rooted source tree.
use monitor_sources as monitor
use sys_block
use sys_mem as memory
use sys_mount
use sys_pci
use sys_source as src
use sys_usb
use system_report as report
use system_report_collect as collectors


pure empty_status(state: report.SectionState) -> report.SectionStatus {
  {state: state, enumeration_succeeded: false}
}

pure empty_cpu(state: report.SectionState) -> report.CpuSection {
  {
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
  {
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
  {
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
      mount_namespace: monitor.empty_text(report.NotRequested),
      network_namespace: monitor.empty_text(report.NotRequested),
      pid_namespace: monitor.empty_text(report.NotRequested),
      cgroup_namespace: monitor.empty_text(report.NotRequested),
      uts_namespace: monitor.empty_text(report.NotRequested),
      ipc_namespace: monitor.empty_text(report.NotRequested),
      user_namespace: monitor.empty_text(report.NotRequested),
      time_namespace: monitor.empty_text(report.NotRequested),
      visible_cgroup: monitor.empty_text(report.NotRequested),
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
      hostname: monitor.empty_text(report.NotRequested),
      uptime_seconds: null,
      boot_id: monitor.empty_text(report.NotRequested),
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
      limitation: monitor.empty_text(report.NotRequested),
    },
    kernel: {
      status: empty_status(report.SectionNotRequested),
      command_line: monitor.empty_text(report.NotRequested),
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


pure issue_with_detail(
  section: Str,
  field: Str,
  state: report.ObservationState,
  error_kind: Str?,
  errno: Int?,
  detail: Str,
) -> report.CollectionIssue {
  {
    ...monitor.issue(section, field, state, error_kind, errno),
    detail: {
      state: report.Observed,
      value: detail,
      raw_bytes_base64: null,
    },
  }
}


type DeviceTreeStringRead = {source: collectors.SourceRead, values: List[Str]}
type NamedPathSource = {name: Str, source_path: RelPath}

proc read_device_tree_strings(
  root: FsRoot,
  source_path: RelPath,
  max_bytes: Int,
  single: Bool,
) [fs, error] -> DeviceTreeStringRead {
  let source = collectors.read_source_text(root, source_path, max_bytes, true)
  if source.observation.state != .Observed or source.observation.value == null {
    return {source: source, values: []}
  }

  let decoded = collectors.decode_device_tree_strings(source.observation.value)
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

  let values = decoded
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

  {source: source, values: values}
}





type HugePageRead = {pool: report.HugePagePool?, issues: List[report.CollectionIssue]}

pure parse_list(value: Str?) -> List[Int] {
  return [] when value == null or value == ""

  if let Ok(parsed) = report.parse_cpu_list(value) {
    parsed
  } else {
    []
  }
}






pure cpu_info_for_id(infos: List[monitor.CpuInfo], id: Int) -> monitor.CpuInfo {
  for item in infos {
    return item when item.id == id
  }

  monitor.empty_cpu_info(id)
}

type CpuSetRead = {
  cpus: List[Int],
  state: report.ObservationState,
  error_kind: Str?,
  errno: Int?,
}

proc read_effective_cgroup_cpuset(root: FsRoot) [fs, error] -> CpuSetRead {
  let membership = monitor.read_value(root, p"proc/self/cgroup", max_bytes: 65536)
  let mount_table = sys_mount.collect(root)
  if mount_table.source_state != .Observed {
    let failure = mount_table.issues[0]
    return {cpus: [], state: failure.state, error_kind: failure.error_kind, errno: failure.errno}
  }

  if membership.observation.state != .Observed {
    return {cpus: [], state: membership.observation.state, error_kind: membership.error_kind, errno: membership.errno}
  }

  let parsed_membership = collectors.parse_unified_cgroup_path(membership.observation.value ?? "")
  if parsed_membership.state == .Malformed {
    return {cpus: [], state: report.Malformed, error_kind: "invalid_cgroup_membership", errno: null}
  }

  let group_path = parsed_membership.path
  let inventory = sys_mount.cgroup_mount_inventory(mount_table)
  if inventory.malformed {
    return {cpus: [], state: report.Malformed, error_kind: "invalid_cgroup_mountinfo", errno: null}
  }

  let has_v1 = parsed_membership.has_v1 or inventory.has_v1
  if group_path == null or inventory.mounts.is_empty() {
    return {
      cpus: [],
      state: if has_v1 { report.Unsupported } else { report.Absent },
      error_kind: if has_v1 { "cgroup_v1_or_hybrid" } else { "cgroup_v2_mount_unavailable" },
      errno: null,
    }
  }

  let group_name = group_path
  var selected: collectors.CgroupMount? = null
  if let Ok(mount) = collectors.select_cgroup_mount(group_name, inventory.mounts) {
    selected = mount
  } else {
    return {cpus: [], state: report.Malformed, error_kind: "invalid_cgroup_mount_path", errno: null}
  }

  if selected == null {
    return {cpus: [], state: report.Unsupported, error_kind: "cgroup_path_outside_visible_mount", errno: null}
  }

  let chosen = selected
  let {root: root_path, point: target, ..} = chosen
  if ! group_name.starts_with("/") {
    return {cpus: [], state: report.Malformed, error_kind: "invalid_cgroup_mount_path", errno: null}
  }

  var relative = ""
  if root_path == "/" {
    relative = group_name.split("") |> drop(1).join("")
  } else if group_name == root_path {
    relative = ""
  } else if group_name.starts_with(f"{root_path}/") {
    relative = group_name.split("") |> drop(root_path.count_chars() + 1).join("")
  }

  let mount_relative = (target.split("/") |> where .trim() != "").join("/")
  let source_path = if relative == "" {
    if mount_relative == "" {
      p"."
    } else {
      fp"{mount_relative}"
    }
  } else if mount_relative == "" {
    fp"{relative}"
  } else {
    fp"{mount_relative}/{relative}"
  }
  let source_relative = source_path.require(RelPath)?
  let source = monitor.read_value(root, fp"{source_relative}/cpuset.cpus.effective", max_bytes: 65536)
  if source.observation.state != .Observed {
    return {cpus: [], state: source.observation.state, error_kind: source.error_kind, errno: source.errno}
  }

  if source.observation.value == "" {
    return {cpus: [], state: report.Observed, error_kind: null, errno: null}
  }

  if let Ok(cpus) = report.parse_cpu_list(source.observation.value ?? "") {
    {cpus: cpus, state: report.Observed, error_kind: null, errno: null}
  } else {
    {cpus: [], state: report.Malformed, error_kind: "invalid_effective_cpuset", errno: null}
  }
}

## Retains the configuration that owns each parsed USB interface setting.
export type UsbDescriptorAlternate = sys_usb.DescriptorAlternate

type CollectedDevices[T] = {
  status: report.SectionStatus,
  devices: List[T],
  issues: List[report.CollectionIssue],
}

type UsbCollection = CollectedDevices[report.UsbDevice]

type StorageCollection = {
  status: report.SectionStatus,
  devices: List[report.BlockDevice],
  mounts: List[report.Mount],
  issues: List[report.CollectionIssue],
}







## Retains a route-netlink snapshot with its enumeration status and issues.
export type NetworkCollection = {
  status: report.SectionStatus,
  links: List[report.NetworkLink],
  routes: List[report.NetworkRoute],
  rules: List[report.NetworkRule],
  issues: List[report.CollectionIssue],
}

## Separates a USB bus entry's controller relation from source-read failures.
export type UsbControllerObservation = sys_usb.ControllerObservation

## Keeps a class device's parent target and source failure state together.
export type ClassParentObservation = src.ClassParentObservation

type NetworkLinkTarget = {
  target: Path?,
  issues: List[report.CollectionIssue],
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

type DeviceCollection = CollectedDevices[report.DeviceClassRecord]

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










pure report_block_device(device: sys_block.BlockDevice, pci_indices: Map[Int]) -> report.BlockDevice {
  {
    name: device.name,
    major: device.major,
    minor: device.minor,
    kind: device.kind,
    size_bytes: device.size_bytes,
    logical_sector_bytes: device.logical_sector_bytes,
    physical_sector_bytes: device.physical_sector_bytes,
    removable: device.removable,
    rotational: device.rotational,
    read_only: device.read_only,
    model: device.model,
    firmware: device.firmware,
    parent_device_index: device.parent_device_index,
    parent_pci_function_index: sys_pci.function_index(pci_indices, device.parent_pci_address),
    holder_indices: device.holder_indices,
    slave_indices: device.slave_indices,
    active_scheduler: device.active_scheduler,
    available_schedulers: device.available_schedulers,
    read_ahead_kb: device.read_ahead_kb,
    discard_granularity_bytes: device.discard_granularity_bytes,
    discard_max_bytes: device.discard_max_bytes,
    io_counters: [
      {name: counter.name, value: counter.value, unit: counter.unit}
      for counter in device.io_counters
      if counter.value <= 9007199254740991
    ],
  }
}

# Redaction-safe option and source policy is applied here, never inside the collector.
pure report_mount(entry: sys_mount.MountEntry, block_indices: Map[Int]) -> report.Mount {
  {
    mount_id: entry.mount_id,
    parent_id: entry.parent_id,
    major: entry.major,
    minor: entry.minor,
    root: {state: report.Observed, value: entry.root, raw_bytes_base64: null},
    target: {state: report.Observed, value: entry.target, raw_bytes_base64: null},
    mount_options: report.sanitize_mount_options(entry.mount_options),
    optional_fields: report.sanitize_mount_optional_fields(entry.optional_fields),
    filesystem: entry.filesystem,
    source: report.sanitize_mount_source({state: report.Observed, value: entry.source, raw_bytes_base64: null}),
    super_options: report.sanitize_mount_options(entry.super_options),
    block_device_index: sys_block.index_of_number(block_indices, entry.major, entry.minor),
    usage_state: entry.usage_state,
    usage_total_bytes: entry.usage_total_bytes,
    usage_used_bytes: entry.usage_used_bytes,
    usage_available_bytes: entry.usage_available_bytes,
  }
}

proc collect_storage(
  root: FsRoot,
  pci_functions: List[report.PciFunction],
  include_local_mount_usage: Bool,
) [fs, error] -> StorageCollection {
  let pci_indices = sys_pci.function_indices(pci_functions)
  let blocks = sys_block.collect(root)
  let table = sys_mount.collect(root, include_local_mount_usage)
  let issues = src.with_section("storage", blocks.issues).extend(src.with_section("storage", table.issues))
  var state = report.Complete
  if blocks.listing_state == "absent" and table.source_state == .Absent {
    state = report.SectionAbsent
  } else if blocks.listing_state != "complete" or table.source_state != .Observed or ! issues.is_empty() {
    state = report.Partial
  }

  let block_indices = sys_block.index_by_number(blocks.devices)
  {
    status: {
      state: state,
      enumeration_succeeded: blocks.enumeration_succeeded and table.source_state == .Observed,
    },
    devices: [report_block_device(device, pci_indices) for device in blocks.devices],
    mounts: [report_mount(entry, block_indices) for entry in table.mounts],
    issues: issues,
  }
}








pure checked_page_bytes(pages: Int?, page_size_bytes: Int) -> Int? {
  let page_count = pages ?? -1
  return null when page_count < 0 or page_size_bytes <= 0

  return null when page_count > 9007199254740991 / page_size_bytes

  page_count * page_size_bytes
}

pure process_cgroup_resource_index(
  cgroup_path: report.TextObservation,
  resources: List[report.CgroupResource],
) -> Int? {
  guard cgroup_path.value != null else {
    return null
  }

  var index = 0
  for resource in resources {
    return index when resource.path.value == cgroup_path.value

    index += 1
  }

  null
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

proc read_process(root: FsRoot, process_path: RelPath, pid: Int, page_size_bytes: Int) [fs, error] -> ProcessRead {
  var issues: List[report.CollectionIssue] = []
  let stat_before = monitor.read_value(root, fp"{process_path}/stat", max_bytes: 16384)
  if stat_before.observation.state != .Observed or stat_before.observation.value == null {
    issues += [
      monitor.issue("processes", f"{pid}.stat", stat_before.observation.state, stat_before.error_kind, stat_before.errno),
    ]
    return {processes: [], issues: issues}
  }

  guard let stat = monitor.parse_proc_stat(stat_before.observation.value) else { |_|
    issues += [monitor.issue("processes", f"{pid}.stat", report.Malformed, "invalid_process_stat", null)]
    return {processes: [], issues: issues}
  }

  if stat.pid != pid {
    issues += [monitor.issue("processes", f"{pid}.stat", report.Raced, "pid_changed_during_read", null)]
    return {processes: [], issues: issues}
  }

  for field_issue in stat.field_issues {
    issues += [
      monitor.issue("processes", f"{pid}.stat.{field_issue.field}", field_issue.state, "invalid_process_stat_field", null),
    ]
  }

  let statm = monitor.read_value(root, fp"{process_path}/statm", max_bytes: 4096)
  let status = monitor.read_value(root, fp"{process_path}/status", max_bytes: 16384)
  let cgroup = monitor.read_value(root, fp"{process_path}/cgroup", max_bytes: 16384)
  var process_cgroup = cgroup.observation
  if cgroup.observation.state != .Observed {
    process_cgroup = {...process_cgroup, value: null}
  } else if cgroup.observation.value != null {
    let parsed_cgroup = collectors.parse_unified_cgroup_path(cgroup.observation.value)
    if parsed_cgroup.path == null {
      let error_kind = if parsed_cgroup.state == .Malformed {
        "invalid_process_cgroup"
      } else {
        "unified_cgroup_path_unavailable"
      }
      process_cgroup = {state: parsed_cgroup.state, value: null, raw_bytes_base64: null}
      issues += [monitor.issue("processes", f"{pid}.cgroup", parsed_cgroup.state, error_kind, null)]
    } else {
      process_cgroup = {state: report.Observed, value: parsed_cgroup.path, raw_bytes_base64: null}
    }
  }

  let stat_after = monitor.read_value(root, fp"{process_path}/stat", max_bytes: 16384)
  if stat_after.observation.state != .Observed or stat_after.observation.value == null {
    let state = if stat_after.observation.state == .Absent {
      report.Disappeared
    } else {
      stat_after.observation.state
    }
    issues += [monitor.issue("processes", f"{pid}.stat", state, stat_after.error_kind, stat_after.errno)]
    return {processes: [], issues: issues}
  }

  let final_stat = monitor.parse_proc_stat(stat_after.observation.value)
  let same_start = if let Ok(after) = final_stat {
    after.pid == pid and after.start_ticks == stat.start_ticks
  } else {
    false
  }
  if ! same_start {
    issues += [monitor.issue("processes", f"{pid}.stat", report.Raced, "pid_start_identity_changed", null)]
    return {processes: [], issues: issues}
  }

  let statm_observed = statm.observation.state == .Observed
  let statm_fields = if statm_observed { monitor.parse_words(statm.observation.value) } else { [] }
  var statm_fields_complete = statm_fields.len() == 7
  if statm_fields_complete {
    for field in statm_fields |> drop(2) {
      guard monitor.decimal_identifier(field) else {
        statm_fields_complete = false
        break
      }
    }
  }

  let virtual_pages = monitor.proc_optional_number(if statm_fields_complete { statm_fields[0] } else { "" })
  let resident_pages = monitor.proc_optional_number(if statm_fields_complete { statm_fields[1] } else { "" })
  let virtual_bytes = if statm_observed { checked_page_bytes(virtual_pages.value, page_size_bytes) } else { null }
  let resident_count = if statm_observed { resident_pages.value } else { stat.resident_pages }
  let resident_bytes = checked_page_bytes(resident_count, page_size_bytes)
  if statm_observed and ! statm_fields_complete {
    issues += [monitor.issue("processes", f"{pid}.statm", report.Malformed, "invalid_statm_row", null)]
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
        issues += [
          monitor.issue(
            "processes",
            f"{pid}.statm.{value.field}",
            value.parsed.issue_state ?? report.Malformed,
            "invalid_page_count",
            null,
          ),
        ]
      } else if value.scaled == null {
        issues += [monitor.issue("processes", f"{pid}.statm.{value.field}", report.RangeFailure, "page_count_overflow", null)]
      }
    }
  } else if stat.resident_pages != null and resident_bytes == null {
    issues += [monitor.issue("processes", f"{pid}.stat.resident_bytes", report.RangeFailure, "page_count_overflow", null)]
  }

  var uid = null
  if status.observation.state == .Observed and status.observation.value != null {
    var uid_rows = 0
    var uid_issue: report.ObservationState? = null
    for line in status.observation.value.lines() {
      if line.starts_with("Uid:") {
        uid_rows += 1
        let fields = monitor.parse_words((line.split(":", maxsplit: 1).get(1) ?? "").replace("\t", with: " "))
        if fields.len() != 4 {
          uid_issue = report.Malformed
          continue
        }

        let values: List[Int] = collect {
          for field in fields {
            let parsed = monitor.proc_optional_number(field)
            if parsed.issue_state != null {
              uid_issue = parsed.issue_state
            } else {
              yield parsed.value ?? 0
            }
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
      issues += [monitor.issue("processes", f"{pid}.uid", uid_issue, error_kind, null)]
    }
  } else if status.observation.state == .Observed {
    issues += [monitor.issue("processes", f"{pid}.uid", report.Malformed, "numeric_uid_unavailable", null)]
  }

  if statm.observation.state != .Observed {
    issues += [monitor.issue("processes", f"{pid}.statm", statm.observation.state, statm.error_kind, statm.errno)]
  }

  if status.observation.state != .Observed {
    issues += [monitor.issue("processes", f"{pid}.status", status.observation.state, status.error_kind, status.errno)]
  }

  if cgroup.observation.state != .Observed {
    issues += [monitor.issue("processes", f"{pid}.cgroup", cgroup.observation.state, cgroup.error_kind, cgroup.errno)]
  }

  let observed_virtual_bytes = if statm_observed { virtual_bytes } else { stat.virtual_bytes }
  let process_item: report.ProcessRecord = report.ProcessRecord(
    pid: stat.pid,
    parent_pid: stat.parent_pid,
    uid:,
    command: {
      state: report.Observed,
      value: stat.command,
      raw_bytes_base64: null,
    },
    state: stat.state,
    start_ticks: stat.start_ticks,
    thread_count: stat.thread_count,
    resident_bytes:,
    virtual_bytes: observed_virtual_bytes,
    cgroup: process_cgroup,
    cgroup_resource_index: null,
  )
  {processes: [process_item], issues: issues}
}

proc collect_processes(root: FsRoot, page_size_bytes: Int) [fs, error] -> ProcessCollection {
  let listing = root.children(p"proc", max_entries: 8192)?
  var processes: List[report.ProcessRecord] = []
  var issues: List[report.CollectionIssue] = []
  if listing.state != "complete" {
    issues += [
      monitor.issue(
        "processes",
        "enumeration",
        monitor.live_source_observation_state(listing.state, false),
        listing.error_kind,
        listing.errno,
      ),
    ]
  }

  for process_path in listing.children {
    let pid_text = process_path.name()
    continue unless monitor.decimal_identifier(pid_text)
    let pid = monitor.parse_integer(pid_text) ?? -1
    if pid <= 0 or pid > 9007199254740991 {
      issues += [monitor.issue("processes", f"{pid_text}.pid", report.RangeFailure, "invalid_pid", null)]
      continue
    }

    let process_read = read_process(root, process_path, pid, page_size_bytes)
    issues += process_read.issues
    processes += process_read.processes
  }

  var state = report.Complete
  if listing.state == "truncated" {
    state = report.SectionTruncated
  } else if listing.state == "absent" {
    state = report.SectionAbsent
  } else if listing.state != "complete" or ! issues.is_empty() {
    state = report.Partial
  }

  {
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
  let source = monitor.read_value(root, p"proc/modules", max_bytes: 1048576)
  var issues: List[report.CollectionIssue] = []
  var modules: List[report.KernelModule] = []
  var seen_modules: Set[Str] = set.empty()
  if command_line.observation.state != .Observed {
    issues += [
      monitor.issue("kernel", "command_line", command_line.observation.state, command_line.error_kind, command_line.errno),
    ]
  }

  if source.observation.state != .Observed or source.observation.value == null {
    issues += [monitor.issue("kernel", "modules", source.observation.state, source.error_kind, source.errno)]
  } else {
    for line_item in source.observation.value.lines() |> enumerate() {
      let columns = monitor.parse_words(line_item.value)

      # A tainted module has one additional flag word after the address.
      if columns.len() not in [6, 7] {
        issues += [monitor.issue("kernel", f"modules.line.{line_item.index}", report.Malformed, "invalid_module_row", null)]
        continue
      }

      let size = monitor.parse_integer(columns[1]) ?? -1
      let users_number = monitor.parse_integer(columns[2]) ?? -1
      let users_unavailable = columns[2] == "-"
      if size < 0 or (users_number < 0 and ! users_unavailable) {
        let out_of_range = (size < 0 and monitor.decimal_identifier(columns[1])) or (users_number < 0 and monitor.decimal_identifier(
          columns[2],
        ))
        let state = if out_of_range { report.RangeFailure } else { report.Malformed }
        let error_kind = if out_of_range { "module_numeric_out_of_range" } else { "invalid_module_numeric_field" }
        issues += [monitor.issue("kernel", f"modules.line.{line_item.index}", state, error_kind, null)]
        continue
      }

      if size > 9007199254740991 or users_number > 9007199254740991 {
        issues += [
          monitor.issue("kernel", f"modules.line.{line_item.index}", report.RangeFailure, "module_numeric_out_of_range", null),
        ]
        continue
      }

      if columns[0] in seen_modules {
        issues += [monitor.issue("kernel", f"modules.line.{line_item.index}", report.Malformed, "duplicate_module_name", null)]
        continue
      }

      seen_modules = seen_modules.add(columns[0])
      let users: Int? = if users_unavailable { null } else { users_number }
      modules += [{name: columns[0], size_bytes: size, users: users, state: columns[4]}]
    }
  }

  var sysctls: List[report.KernelParameter] = []
  let named_sysctls: List[NamedPathSource] = [
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
  ]
  for named_source in named_sysctls {
    let {name, source_path, ..} = named_source
    let value = monitor.read_value(root, source_path, max_bytes: 4096)
    sysctls += [{name: name, value: value.observation}]
    if value.observation.state != .Observed and value.observation.state != .Absent {
      issues += [monitor.issue("kernel", f"sysctl.{name}", value.observation.state, value.error_kind, value.errno)]
    }
  }

  var parameters: List[report.KernelParameter] = []
  let named_parameters: List[NamedPathSource] = [
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
  ]
  for named_source in named_parameters {
    let {name, source_path, ..} = named_source
    let value = monitor.read_value(root, source_path, max_bytes: 4096)
    parameters += [{name: name, value: value.observation}]
    if value.observation.state != .Observed and value.observation.state != .Absent {
      issues += [monitor.issue("kernel", f"parameters.{name}", value.observation.state, value.error_kind, value.errno)]
    }
  }

  var state = if source.observation.state == .Observed and command_line.observation.state == .Observed {
    report.Complete
  } else {
    report.Partial
  }
  if ! issues.is_empty() {
    state = report.Partial
  }

  {
    status: {
      state: state,
      enumeration_succeeded: source.observation.state == .Observed,
    },
    command_line: command_line.observation,
    modules: modules |> sort-by .name,
    parameters: parameters,
    sysctls: sysctls,
    issues: issues,
  }
}


proc collect_device_classes(
  root: FsRoot,
  pci_functions: List[report.PciFunction],
  usb_devices: List[report.UsbDevice],
) [fs, error] -> DeviceCollection {
  let pci_indices = sys_pci.function_indices(pci_functions)
  let usb_indices = monitor.usb_device_indices(usb_devices)
  var devices: List[report.DeviceClassRecord] = []
  var issues: List[report.CollectionIssue] = []
  var available_classes = 0
  var enumerated_classes = 0
  let class_sources: List[NamedPathSource] = [
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
  ]
  for class_source in class_sources {
    let {name: class_name, source_path, ..} = class_source
    let listing = root.children(source_path, max_entries: 4096)?
    if listing.state != "absent" {
      available_classes += 1
    }

    if listing.enumeration_succeeded or listing.state == "absent" {
      enumerated_classes += 1
    } else {
      issues += [
        monitor.issue(
          "devices",
          f"{class_name}.enumeration",
          monitor.live_source_observation_state(listing.state, false),
          listing.error_kind,
          listing.errno,
        ),
      ]
    }

    for entry in listing.children {
      let entry_name = entry.name()
      var name = entry_name
      if class_name == "input" or class_name == "sound" {
        let name_field = if class_name == "input" { "name" } else { "id" }
        let name_path = fp"{entry}/{name_field}".require(RelPath)?
        let name_file = monitor.read_value(root, name_path, max_bytes: 4096)
        let observed_name = monitor.observed_source_text(name_file)
        if observed_name != null {
          name = observed_name
        } else {
          issues = monitor.append_text_issue(issues, "devices", f"{class_name}.{entry_name}.{name_field}", name_file)
        }
      }

      let driver_link = optional_driver_name(root, fp"{entry}/device/driver")
      if driver_link.observation.state != .Observed and driver_link.observation.state != .Absent {
        issues += [
          monitor.issue(
            "devices",
            f"{class_name}.{entry_name}.driver",
            driver_link.observation.state,
            driver_link.error_kind,
            driver_link.errno,
          ),
        ]
      }

      let parent = class_parent_target(root, entry)
      if parent.state != .Observed {
        issues += [monitor.issue("devices", f"{class_name}.{entry_name}.parent", parent.state, parent.error_kind, parent.errno)]
      }

      var parent_pci_address = null
      var parent_usb_device_index = null
      if parent.target != null {
        let parent_target = parent.target
        parent_pci_address = sys_pci.address_in_target(parent_target)
        parent_usb_device_index = monitor.usb_device_index_from_target(usb_indices, parent_target)
      }

      let allowlisted_attributes = match class_name {
        "drm" => ["status", "enabled", "modes"],
        "sound" => ["number"],
        "input" => [],
        else => [],
      }
      let attributes: List[report.KernelParameter] = collect {
        for attribute_name in allowlisted_attributes {
          let attribute_path = fp"{entry}/{attribute_name}".require(RelPath)?
          let attribute = monitor.read_value(root, attribute_path, max_bytes: 16384)
          if attribute.observation.state == .Observed {
            yield {name: attribute_name, value: attribute.observation}
          } else {
            issues = monitor.append_text_issue(issues, "devices", f"{class_name}.{entry_name}.{attribute_name}", attribute)
          }
        }
      }

      devices += [
        {
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
          parent_pci_function_index: sys_pci.function_index(pci_indices, parent_pci_address),
          parent_usb_device_index: parent_usb_device_index,
          driver: monitor.observed_source_text(driver_link),
          attributes: attributes,
        },
      ]
    }
  }

  var state = report.Complete
  if available_classes == 0 {
    state = report.SectionAbsent
  } else if enumerated_classes < 3 or ! issues.is_empty() {
    state = report.Partial
  }

  {
    status: {
      state: state,
      enumeration_succeeded: enumerated_classes == 3,
    },
    devices: devices,
    issues: issues,
  }
}

proc collect_firmware(root: FsRoot) [fs, error] -> FirmwareCollection {
  let source = root.read_result(p"sys/firmware/dmi/tables/DMI", max_bytes: 1048576)?
  var records: List[report.FirmwareRecord] = []
  var issues: List[report.CollectionIssue] = []
  if source.state == "observed" and source.data != null and ! source.truncated {
    let parsed = parse_smbios_table(source.data)?
    records = parsed.records
    for line_item in parsed.issues |> enumerate() {
      issues += [
        issue_with_detail(
          "firmware",
          f"smbios.issue.{line_item.index}",
          if parsed.truncated {
            report.Truncated
          } else {
            report.Malformed
          },
          "smbios_record_validation",
          null,
          line_item.value,
        ),
      ]
    }

    let state = if parsed.issues.is_empty() {
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
    issues += [monitor.issue("firmware", "smbios", report.Truncated, "dmi_table_limit", source.errno)]
  } else if source.state != "absent" {
    issues += [
      monitor.issue("firmware", "smbios", monitor.live_source_observation_state(source.state, false), source.error_kind, source.errno),
    ]
  }

  issues = monitor.append_text_issue(issues, "firmware", "device_tree_model", device_tree_model)
  issues = monitor.append_text_issue(issues, "firmware", "device_tree_compatible", device_tree_compatible)
  if device_tree_model.observation.state == .Observed or device_tree_compatible.observation.state == .Observed {
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

  if issues.is_empty() {
    issues += [monitor.issue("firmware", "smbios", report.Absent, "firmware_table_unavailable", null)]
  }

  {
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
  {name: name, value: value, unit: "string_index"}
}

pure smbios_raw_field(value: Int, name: Str) -> report.MemoryCounter {
  {name: name, value: value, unit: "smbios_raw"}
}

pure smbios_fields(record_type: Int, data: Bytes, offset: Int, length: Int) -> Result[List[report.MemoryCounter]] {
  let fields: List[report.MemoryCounter] = collect {
    if record_type == 0 {
      if length > 4 {
        yield smbios_string_index(data.byte_at(offset + 4) ?? -1, "vendor_index")
      }

      if length > 5 {
        yield smbios_string_index(data.byte_at(offset + 5) ?? -1, "version_index")
      }

      if length > 8 {
        yield smbios_string_index(data.byte_at(offset + 8) ?? -1, "release_date_index")
      }

      yield smbios_raw_field(data.byte_at(offset + 9) ?? -1, "rom_size_raw") when length > 9
    } else if record_type == 1 {
      if length > 4 {
        yield smbios_string_index(data.byte_at(offset + 4) ?? -1, "manufacturer_index")
      }

      if length > 5 {
        yield smbios_string_index(data.byte_at(offset + 5) ?? -1, "product_index")
      }

      if length > 6 {
        yield smbios_string_index(data.byte_at(offset + 6) ?? -1, "version_index")
      }

      if length > 7 {
        yield smbios_string_index(data.byte_at(offset + 7) ?? -1, "serial_index")
      }

      if length > 24 {
        yield smbios_raw_field(data.byte_at(offset + 24) ?? -1, "wake_up_type_raw")
      }

      yield smbios_string_index(data.byte_at(offset + 25) ?? -1, "sku_index") when length > 25

      if length > 26 {
        yield smbios_string_index(data.byte_at(offset + 26) ?? -1, "family_index")
      }
    } else if record_type == 2 {
      if length > 4 {
        yield smbios_string_index(data.byte_at(offset + 4) ?? -1, "manufacturer_index")
      }

      if length > 5 {
        yield smbios_string_index(data.byte_at(offset + 5) ?? -1, "product_index")
      }

      if length > 6 {
        yield smbios_string_index(data.byte_at(offset + 6) ?? -1, "version_index")
      }

      if length > 7 {
        yield smbios_string_index(data.byte_at(offset + 7) ?? -1, "serial_index")
      }

      if length > 8 {
        yield smbios_string_index(data.byte_at(offset + 8) ?? -1, "asset_tag_index")
      }

      if length > 13 {
        yield smbios_raw_field(data.byte_at(offset + 13) ?? -1, "board_type_raw")
      }
    } else if record_type == 4 {
      if length > 4 {
        yield smbios_string_index(data.byte_at(offset + 4) ?? -1, "socket_designation_index")
      }

      if length > 7 {
        yield smbios_string_index(data.byte_at(offset + 7) ?? -1, "manufacturer_index")
      }

      if length > 16 {
        yield smbios_string_index(data.byte_at(offset + 16) ?? -1, "version_index")
      }

      if length > 23 {
        yield smbios_raw_field(data.byte_at(offset + 23) ?? -1, "core_count_raw")
      }

      if length > 24 {
        yield smbios_raw_field(data.byte_at(offset + 24) ?? -1, "core_enabled_raw")
      }

      if length > 25 {
        yield smbios_raw_field(data.byte_at(offset + 25) ?? -1, "thread_count_raw")
      }
    } else if record_type == 16 {
      if length >= 11 {
        yield {name: "maximum_capacity_raw", value: bytes.unpack_le(data, 4, offset + 7)?, unit: "smbios_raw"}
      }

      if length >= 15 {
        yield {name: "number_of_devices", value: bytes.unpack_le(data, 2, offset + 13)?, unit: "count"}
      }
    } else if record_type == 17 {
      if length >= 14 {
        yield {name: "total_width_raw", value: bytes.unpack_le(data, 2, offset + 8)?, unit: "smbios_raw"}
        yield {name: "data_width_raw", value: bytes.unpack_le(data, 2, offset + 10)?, unit: "smbios_raw"}
        yield {name: "size_raw", value: bytes.unpack_le(data, 2, offset + 12)?, unit: "smbios_raw"}
      }

      if length > 16 {
        yield smbios_string_index(data.byte_at(offset + 16) ?? -1, "device_locator_index")
      }

      if length > 17 {
        yield smbios_string_index(data.byte_at(offset + 17) ?? -1, "bank_locator_index")
      }

      if length > 26 {
        yield smbios_string_index(data.byte_at(offset + 26) ?? -1, "part_number_index")
      }

      if length >= 32 and bytes.unpack_le(data, 2, offset + 12)? == 32767 {
        yield {name: "extended_size_raw", value: bytes.unpack_le(data, 4, offset + 28)?, unit: "smbios_raw"}
      }
    }
  }

  fields
}

## Parses kernel-exported SMBIOS records without reading physical memory.
export pure parse_smbios_table(data: Bytes) -> Result[SmbiosParseResult, Error] {
  var records: List[report.FirmwareRecord] = []
  var issues = []
  if data.len() > 1048576 {
    return {records: records, issues: ["SMBIOS table exceeds the 1 MiB bound"], truncated: true}
  }

  var offset = 0
  var saw_end_marker = false
  while offset < data.len() {
    if records.len() >= 4096 {
      issues += ["SMBIOS table exceeds the 4,096 record bound"]
      return {records: records, issues: issues, truncated: true}
    }

    if data.len() - offset < 4 {
      issues += ["SMBIOS record header is truncated"]
      return {records: records, issues: issues, truncated: true}
    }

    let record_type = data.byte_at(offset) ?? -1
    let formatted_length = data.byte_at(offset + 1) ?? -1
    if formatted_length < 4 {
      issues += [f"SMBIOS record type {record_type} has a formatted length below four bytes"]
      return {records: records, issues: issues, truncated: false}
    }

    if formatted_length > data.len() - offset {
      issues += [f"SMBIOS record type {record_type} extends beyond the table"]
      return {records: records, issues: issues, truncated: true}
    }

    let handle = bytes.unpack_le(data, 2, offset + 2)?
    let parsed_fields = smbios_fields(record_type, data, offset, formatted_length)?
    let fields = parsed_fields
    let strings_start = offset + formatted_length
    var position = strings_start
    var string_start = strings_start
    var found_terminator = false
    let strings: List[report.TextObservation] = collect {
      while position < data.len() {
        if (data.byte_at(position) ?? -1) == 0 {
          if position + 1 < data.len() and (data.byte_at(position + 1) ?? -1) == 0 {
            if position > string_start {
              let raw = data[string_start..position]
              if let Ok(value) = raw.utf8() {
                yield {state: report.Observed, value: value, raw_bytes_base64: null}
              } else {
                yield {state: report.Malformed, value: null, raw_bytes_base64: raw.base64()}
              }
            }

            position += 2
            found_terminator = true
            break
          }

          if position > string_start {
            let raw = data[string_start..position]
            if let Ok(value) = raw.utf8() {
              yield {state: report.Observed, value: value, raw_bytes_base64: null}
            } else {
              yield {state: report.Malformed, value: null, raw_bytes_base64: raw.base64()}
            }
          }

          position += 1
          string_start = position
        } else {
          position += 1
        }
      }
    }

    if ! found_terminator {
      issues += [f"SMBIOS record type {record_type} has no complete string-set terminator"]
      return {
        records: records,
        issues: issues,
        truncated: true,
      }
    }

    for field in fields {
      if field.unit == "string_index" and field.value > strings.len() {
        issues += [f"SMBIOS record type {record_type} has an out-of-range {field.name}"]
      }
    }

    records += [
      {
        record_type: record_type,
        handle: handle,
        formatted_length: formatted_length,
        fields: fields,
        strings: strings,
      },
    ]
    offset = position
    if record_type == 127 {
      saw_end_marker = true
      break
    }
  }

  if ! saw_end_marker {
    issues += ["SMBIOS table has no end-of-table record"]
  }

  {records: records, issues: issues, truncated: false}
}


## Resolves USB parent indexes after every device has been enumerated.
export pure link_usb_parents(devices: List[report.UsbDevice]) -> List[report.UsbDevice] {
  let parents = sys_usb.parent_indices(
    [
      device.sysfs_name
      for device in devices
    ],
    [
      device.bus_number
      for device in devices
    ],
  )
  [{...devices[index], parent_device_index: parents[index]} for index in range(devices.len())]
}


## Finds a USB controller's PCI function in its bus-entry symlink target.
export pure usb_parent_address(target: Path) -> Str? {
  sys_pci.address_in_target(target)
}

## Finds the last PCI function in a sysfs class-entry symlink target.
export pure pci_address_in_target(target: Path) -> Str? {
  sys_pci.address_in_target(target)
}

## Prefers a device link and keeps a class-entry link as independent parent evidence.
export proc class_parent_target(root: FsRoot, entry: RelPath) [fs, error] -> ClassParentObservation {
  src.class_parent_target(root, entry)
}

## Reads a USB bus-entry link and accepts direct directories in rooted fixtures.
export proc usb_controller_address(root: FsRoot, device_path: RelPath) [fs, error] -> UsbControllerObservation {
  sys_usb.controller_address(root, device_path)
}

## Reads an optional driver binding while retaining failures separate from an unbound device.
export proc optional_driver_name(root: FsRoot, source_path: RelPath) [fs, error] -> collectors.SourceRead {
  src.driver_name(root, source_path)
}

## Parses interface settings and endpoints without joining identical numbers across configurations.
export proc parse_usb_alternates(data: Bytes) [error] -> Result[List[UsbDescriptorAlternate], Error] {
  match sys_usb.parse_alternates(data) {
    Ok(alternates) => Ok(alternates)
    Err(error) => Err(error.failure(error.message), cause: error)
  }
}

pure report_usb_device(device: sys_usb.UsbDevice, pci_indices: Map[Int]) -> report.UsbDevice {
  {
    sysfs_name: device.sysfs_name,
    parent_device_index: device.parent_device_index,
    controller_pci_index: sys_pci.function_index(pci_indices, device.controller_pci_address),
    port_path: device.port_path,
    bus_number: device.bus_number,
    device_number: device.device_number,
    vendor_id: device.vendor_id,
    product_id: device.product_id,
    device_version: device.device_version,
    class_code: device.class_code,
    subclass: device.subclass,
    protocol: device.protocol,
    manufacturer: device.manufacturer,
    product: device.product,
    serial: device.serial,
    speed_mbps: device.speed_mbps,
    configuration_count: device.configuration_count,
    active_configuration: device.active_configuration,
    power_control: device.power_control,
    autosuspend_delay_ms: device.autosuspend_delay_ms,
    runtime_status: device.runtime_status,
    is_root_hub: device.is_root_hub,
    interfaces: device.interfaces,
  }
}

proc collect_usb(root: FsRoot, pci_functions: List[report.PciFunction]) [fs, error] -> UsbCollection {
  let pci_indices = sys_pci.function_indices(pci_functions)
  let inventory = sys_usb.collect(root)
  var state = report.Complete
  if inventory.listing_state == "absent" {
    state = report.SectionAbsent
  } else if inventory.listing_state != "complete" or ! inventory.issues.is_empty() {
    state = report.Partial
  }

  {
    status: {
      state: state,
      enumeration_succeeded: inventory.enumeration_succeeded,
    },
    devices: [report_usb_device(device, pci_indices) for device in inventory.devices],
    issues: src.with_section("usb", inventory.issues),
  }
}

pure value_or_null(value: collectors.SourceRead) -> Str? {
  return value.observation.value when value.observation.state == .Observed

  null
}

proc collect_identity(root: FsRoot, base: report.SystemReport) [fs, error] -> report.SystemReport {
  let release = monitor.read_value(root, p"proc/sys/kernel/osrelease")
  let version = monitor.read_value(root, p"proc/version", max_bytes: 65536)
  let hostname = monitor.read_value(root, p"proc/sys/kernel/hostname")
  let boot_id = monitor.read_value(root, p"proc/sys/kernel/random/boot_id", max_bytes: 4096)
  let uptime = monitor.read_value(root, p"proc/uptime", max_bytes: 4096)
  var os_release = monitor.read_value(root, p"etc/os-release", max_bytes: 65536)
  if os_release.observation.state == .Absent {
    os_release = monitor.read_value(root, p"usr/lib/os-release", max_bytes: 65536)
  }

  var issues = base.issues
  if release.observation.state != .Observed {
    issues += [monitor.issue("identity", "kernel_release", release.observation.state, release.error_kind, release.errno)]
  }

  if version.observation.state != .Observed {
    issues += [monitor.issue("identity", "kernel_build", version.observation.state, version.error_kind, version.errno)]
  }

  if os_release.observation.state != .Observed {
    issues += [monitor.issue("identity", "os_release", os_release.observation.state, os_release.error_kind, os_release.errno)]
  }

  issues = monitor.append_text_issue(issues, "identity", "hostname", hostname)
  issues = monitor.append_text_issue(issues, "identity", "boot_id", boot_id)

  var os: report.OsRelease? = null
  let os_release_text = value_or_null(os_release)
  if os_release_text != null {
    var id = null
    var id_issue_recorded = false
    var name = null
    var pretty_name = null
    var os_version = null
    var version_id = null
    for line_item in os_release_text.lines() |> enumerate() {
      let line = line_item.value
      continue when line.trim() == "" or line.trim().starts_with("#")
      let pair = line.split("=", maxsplit: 1)
      if pair.len() != 2 or ! collectors.valid_os_release_key(pair[0]) {
        issues += [
          monitor.issue("identity", f"os_release.line.{line_item.index}", report.Malformed, "invalid_assignment", null),
        ]
        continue
      }

      let key = pair[0]
      continue when key not in ["ID", "NAME", "PRETTY_NAME", "VERSION", "VERSION_ID"]
      let parsed = collectors.decode_os_release_value(pair[1])
      if parsed == null or (key == "ID" and ! collectors.valid_os_release_id(parsed)) {
        issues += [monitor.issue("identity", f"os_release.{key}", report.Malformed, "invalid_value", null)]
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
        else => {}
      }
    }

    if id == null and ! id_issue_recorded {
      issues += [monitor.issue("identity", "os_release.ID", report.Malformed, "missing_id", null)]
    }

    os = {id: id, name: name, pretty_name: pretty_name, version: os_version, version_id: version_id}
  }

  let uptime_number = collectors.parse_uptime_seconds(uptime)
  issues = monitor.append_number_issue(issues, "identity", "uptime", uptime_number)

  let firmware_read = read_firmware_identity(root)
  issues += firmware_read.issues
  let identity_status = if issues.len() == base.issues.len() { report.Complete } else { report.Partial }
  let identity = {
    status: {
      state: identity_status,
      enumeration_succeeded: release.observation.state == .Observed,
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
    if namespace.source.observation.state != .Observed {
      issues += [
        monitor.issue(
          "scope",
          namespace.field,
          namespace.source.observation.state,
          namespace.source.error_kind,
          namespace.source.errno,
        ),
      ]
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
    visible_cgroup: monitor.read_value(root, p"proc/self/cgroup", max_bytes: 65536).observation,
  }

  {...base, identity: identity, scope: scope, issues: issues}
}

proc namespace_observation(root: FsRoot, source_path: RelPath) [fs, error] -> Result[collectors.SourceRead] {
  let result = root.readlink_result(source_path)?
  let state = monitor.live_source_observation_state(result.state, false)
  var value = null
  if result.target != null {
    value = result.target.require(Path)?.display()
  }

  {
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
  let vendor = monitor.read_value(root, p"sys/class/dmi/id/sys_vendor", max_bytes: 4096)
  let product = monitor.read_value(root, p"sys/class/dmi/id/product_name", max_bytes: 4096)
  let board_vendor = monitor.read_value(root, p"sys/class/dmi/id/board_vendor", max_bytes: 4096)
  let board_product = monitor.read_value(root, p"sys/class/dmi/id/board_name", max_bytes: 4096)
  let bios_vendor = monitor.read_value(root, p"sys/class/dmi/id/bios_vendor", max_bytes: 4096)
  let bios_version = monitor.read_value(root, p"sys/class/dmi/id/bios_version", max_bytes: 4096)
  let serial = monitor.read_value(root, p"sys/class/dmi/id/product_serial", max_bytes: 4096)
  let uuid = monitor.read_value(root, p"sys/class/dmi/id/product_uuid", max_bytes: 4096)
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
    issues = monitor.append_text_issue(issues, "identity", f"firmware.{named_source.name}", named_source.source)
  }

  var compatible: List[report.TextObservation] = [
    {state: report.Observed, value: value, raw_bytes_base64: null}
    for value in dt_compatible_read.values
  ]
  var source = "dmi"
  if vendor.observation.state != .Observed and product.observation.state != .Observed {
    if dt_model.observation.state == .Observed or ! compatible.is_empty() {
      source = "device-tree"
    } else {
      source = "unavailable"
    }
  }

  {
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
  let possible_source = monitor.read_value(root, p"sys/devices/system/cpu/possible", max_bytes: 65536)
  let present_source = monitor.read_value(root, p"sys/devices/system/cpu/present", max_bytes: 65536)
  let online_source = monitor.read_value(root, p"sys/devices/system/cpu/online", max_bytes: 65536)
  let offline_source = monitor.read_value(root, p"sys/devices/system/cpu/offline", max_bytes: 65536)
  var issues = base.issues
  let initial_issue_count = issues.len()
  let cpuset_source = read_effective_cgroup_cpuset(root)
  if cpuset_source.state != .Observed {
    issues += [
      monitor.issue("cpu", "cgroup.effective_cpuset", cpuset_source.state, cpuset_source.error_kind, cpuset_source.errno),
    ]
  }

  var possible = []
  var present = []
  var online = []
  var offline = []
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
    let {name, source, ..} = named_source
    if source.observation.state != .Observed or source.observation.value == null {
      issues += [monitor.issue("cpu", name, source.observation.state, source.error_kind, source.errno)]
      continue
    }

    if name == "offline" and source.observation.value == "" {
      offline = []
      continue
    }

    let parsed = report.parse_cpu_list(source.observation.value)
    if let Ok(ids) = parsed {
      match name {
        "possible" => possible = ids
        "present" => {
          present = ids
          present_enumerated = true
        }
        "online" => online = ids
        "offline" => offline = ids
        else => {}
      }
    } else {
      issues += [monitor.issue("cpu", name, report.Malformed, "invalid_cpu_list", null)]
    }
  }

  var cpus: List[report.Cpu] = []
  var caches: List[report.CpuCache] = []
  var cache_by_key: Map[Int] = {}
  var cache_ids_by_cpu: Map[List[Int]] = {}
  let cpu_info_read = monitor.read_cpu_info(root)
  if cpu_info_read.state != .Observed {
    issues += [monitor.issue("cpu", "cpuinfo", cpu_info_read.state, cpu_info_read.error_kind, cpu_info_read.errno)]
  }

  let cpu_info = cpu_info_read.infos
  for cpu_id in present {
    let info = cpu_info_for_id(cpu_info, cpu_id)
    let cpu_path = fp"sys/devices/system/cpu/cpu{cpu_id}".require(RelPath)?
    let package = monitor.read_value(root, fp"{cpu_path}/topology/physical_package_id", max_bytes: 4096)
    let die = monitor.read_value(root, fp"{cpu_path}/topology/die_id", max_bytes: 4096)
    let core = monitor.read_value(root, fp"{cpu_path}/topology/core_id", max_bytes: 4096)
    let siblings = monitor.read_value(root, fp"{cpu_path}/topology/thread_siblings_list", max_bytes: 4096)
    let package_number = collectors.bounded_number(package, false)
    let die_number = collectors.bounded_number(die, false)
    let core_number = collectors.bounded_number(core, false)
    issues = monitor.append_number_issue(issues, "cpu", f"cpu{cpu_id}.topology.physical_package_id", package_number)
    issues = monitor.append_number_issue(issues, "cpu", f"cpu{cpu_id}.topology.die_id", die_number)
    issues = monitor.append_number_issue(issues, "cpu", f"cpu{cpu_id}.topology.core_id", core_number)
    issues = monitor.append_text_issue(issues, "cpu", f"cpu{cpu_id}.topology.thread_siblings_list", siblings)
    let sibling_ids = parse_list(monitor.observed_source_text(siblings))
    if siblings.observation.state == .Observed and sibling_ids.is_empty() {
      issues += [monitor.issue("cpu", f"cpu{cpu_id}.topology.thread_siblings_list", report.Malformed, "invalid_cpu_list", null)]
    }

    let node_listing = root.children(cpu_path, max_entries: 256)?
    if node_listing.state != "complete" {
      issues += [
        monitor.issue(
          "cpu",
          f"cpu{cpu_id}.enumeration",
          monitor.live_source_observation_state(node_listing.state, false),
          node_listing.error_kind,
          node_listing.errno,
        ),
      ]
    }

    var numa_node = null
    for node_path in node_listing.children {
      if node_path.name().starts_with("node") {
        numa_node = monitor.parse_integer(node_path.name().split("") |> drop(4).join(""))
        break when numa_node != null
      }
    }

    let is_online = cpu_id in online
    cpus += [
      {
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
      },
    ]

    let cache_listing = root.children(fp"{cpu_path}/cache", max_entries: 64)?
    if cache_listing.state != "complete" and cache_listing.state != "absent" {
      issues += [
        monitor.issue(
          "cpu",
          f"cpu{cpu_id}.cache",
          monitor.live_source_observation_state(cache_listing.state, false),
          cache_listing.error_kind,
          cache_listing.errno,
        ),
      ]
    }

    for cache_path in cache_listing.children {
      continue unless cache_path.name().starts_with("index")
      let sysfs_index = monitor.parse_integer(cache_path.name().split("") |> drop(5).join("")) ?? -1
      if sysfs_index < 0 {
        issues += [monitor.issue("cpu", f"{cpu_id}.cache.{cache_path.name()}", report.Malformed, "invalid_cache_index", null)]
        continue
      }

      let level_text = monitor.read_value(root, fp"{cache_path}/level", max_bytes: 4096)
      let kind = monitor.read_value(root, fp"{cache_path}/type", max_bytes: 4096)
      let size = monitor.read_value(root, fp"{cache_path}/size", max_bytes: 4096)
      let line_size = monitor.read_value(root, fp"{cache_path}/coherency_line_size", max_bytes: 4096)
      let sets = monitor.read_value(root, fp"{cache_path}/number_of_sets", max_bytes: 4096)
      let shared = monitor.read_value(root, fp"{cache_path}/shared_cpu_list", max_bytes: 4096)
      let kernel_id_source = monitor.read_value(root, fp"{cache_path}/id", max_bytes: 4096)
      let level_number = collectors.bounded_number(level_text, true)
      issues = monitor.append_number_issue(issues, "cpu", f"cpu{cpu_id}.cache.{cache_path.name()}.level", level_number)
      let level = level_number.value ?? 0
      if level <= 0 {
        if level_number.state == null {
          let state = if level_text.observation.state == .Absent { report.Absent } else { report.Malformed }
          issues += [monitor.issue("cpu", f"cpu{cpu_id}.cache.{cache_path.name()}.level", state, "invalid_cache_level", null)]
        }

        continue
      }

      issues = monitor.append_text_issue(issues, "cpu", f"cpu{cpu_id}.cache.{cache_path.name()}.type", kind)
      let cache_kind = monitor.observed_source_text(kind) ?? ""
      if cache_kind == "" {
        if kind.observation.state == .Observed or kind.observation.state == .Absent {
          let state = if kind.observation.state == .Absent { report.Absent } else { report.Malformed }
          issues += [monitor.issue("cpu", f"cpu{cpu_id}.cache.{cache_path.name()}.type", state, "invalid_cache_type", null)]
        }

        continue
      }

      var shared_cpus = []
      if shared.observation.state != .Observed or shared.observation.value == null {
        issues = monitor.append_text_issue(issues, "cpu", f"cpu{cpu_id}.cache.{cache_path.name()}.shared_cpu_list", shared)
      } else {
        if let Ok(ids) = collectors.parse_cache_shared_cpus(shared.observation.value) {
          shared_cpus = ids
        } else {
          issues += [
            monitor.issue(
              "cpu",
              f"cpu{cpu_id}.cache.{cache_path.name()}.shared_cpu_list",
              report.Malformed,
              "invalid_cache_cpu_list",
              null,
            ),
          ]
        }
      }

      var kernel_id = null
      if kernel_id_source.observation.state == .Observed {
        let parsed_id = collectors.bounded_number(kernel_id_source, true)
        issues = monitor.append_number_issue(issues, "cpu", f"cpu{cpu_id}.cache.{cache_path.name()}.id", parsed_id)
        kernel_id = parsed_id.value
      } else if kernel_id_source.observation.state != .Absent {
        issues = monitor.append_text_issue(issues, "cpu", f"cpu{cpu_id}.cache.{cache_path.name()}.id", kernel_id_source)
      }

      let cache_key = if ! shared_cpus.is_empty() {
        if kernel_id != null {
          json.encode({level: level, kind: cache_kind, kernel_id: kernel_id})?
        } else {
          json.encode({level: level, kind: cache_kind, shared_cpus: shared_cpus, sysfs_index: sysfs_index})?
        }
      } else {
        f"owner:{cpu_id}:{sysfs_index}"
      }
      let size_number = collectors.bounded_size_bytes(size)
      issues = monitor.append_number_issue(issues, "cpu", f"cpu{cpu_id}.cache.{cache_path.name()}.size", size_number)
      let line_size_number = collectors.bounded_number(line_size, true)
      let sets_number = collectors.bounded_number(sets, true)
      issues = monitor.append_number_issue(
        issues,
        "cpu",
        f"cpu{cpu_id}.cache.{cache_path.name()}.coherency_line_size",
        line_size_number,
      )
      issues = monitor.append_number_issue(
        issues,
        "cpu",
        f"cpu{cpu_id}.cache.{cache_path.name()}.number_of_sets",
        sets_number,
      )
      if cache_key in cache_by_key {
        let previous = caches[cache_by_key.get(cache_key)?]
        if previous.shared_cpus != shared_cpus or previous.level != level or previous.kind != cache_kind or previous.size_bytes != size_number.value or previous.line_size_bytes != line_size_number.value or previous.sets != sets_number.value {
          issues += [
            monitor.issue(
              "cpu",
              f"cpu{cpu_id}.cache.{cache_path.name()}",
              report.Malformed,
              "inconsistent_cache_instance",
              null,
            ),
          ]
        }

        continue
      }

      let cache_id = caches.len()
      caches += [
        {
          id: cache_id,
          sysfs_index: sysfs_index,
          owner_cpu_id: cpu_id,
          level: level,
          kind: cache_kind,
          size_bytes: size_number.value,
          line_size_bytes: line_size_number.value,
          sets: sets_number.value,
          shared_cpus: shared_cpus,
        },
      ]
      cache_by_key = cache_by_key.set(cache_key, cache_id)
      let members = if ! shared_cpus.is_empty() { shared_cpus } else { [cpu_id] }
      for member in members {
        let key = f"{member}"
        cache_ids_by_cpu = cache_ids_by_cpu.set(key, (cache_ids_by_cpu.get(key) ?? []).push(cache_id))
      }
    }
  }

  let policies = monitor.collect_frequency_policies(root, issues)
  issues = policies.issues
  let linked_cpus: List[report.Cpu] = collect {
    for cpu_item in cpus {
      var policy_name = null
      let cache_ids = cache_ids_by_cpu.get(f"{cpu_item.id}") ?? []
      for policy in policies.policies {
        if cpu_item.id in policy.related_cpus {
          policy_name = policy.name
          break
        }
      }

      yield {
        ...cpu_item,
        policy: policy_name,
        cache_ids: cache_ids,
        cache_indices: cache_ids,
      }
    }
  }

  let vulnerabilities_listing = root.children(p"sys/devices/system/cpu/vulnerabilities", max_entries: 256)?
  if vulnerabilities_listing.state != "complete" and vulnerabilities_listing.state != "absent" {
    issues += [
      monitor.issue(
        "cpu",
        "vulnerabilities",
        monitor.live_source_observation_state(vulnerabilities_listing.state, false),
        vulnerabilities_listing.error_kind,
        vulnerabilities_listing.errno,
      ),
    ]
  }

  var vulnerabilities: List[report.CpuVulnerability] = []
  for item in vulnerabilities_listing.children {
    let value = monitor.read_value(root, item, max_bytes: 16384)
    vulnerabilities += [{name: item.name(), description: value.observation}]
    if value.observation.state != .Observed {
      issues += [monitor.issue("cpu", f"vulnerabilities.{item.name()}", value.observation.state, value.error_kind, value.errno)]
    }
  }

  let idle_driver = monitor.read_value(root, p"sys/devices/system/cpu/cpuidle/current_driver", max_bytes: 4096)
  var idle_governor = monitor.read_value(root, p"sys/devices/system/cpu/cpuidle/current_governor", max_bytes: 4096)
  var idle_governor_field = "cpuidle.current_governor"
  if idle_governor.observation.state == .Absent {
    idle_governor = monitor.read_value(root, p"sys/devices/system/cpu/cpuidle/current_governor_ro", max_bytes: 4096)
    idle_governor_field = "cpuidle.current_governor_ro"
  }

  let available_idle_governors = monitor.read_value(
    root,
    p"sys/devices/system/cpu/cpuidle/available_governors",
    max_bytes: 4096,
  )
  issues = monitor.append_text_issue(issues, "cpu", "cpuidle.current_driver", idle_driver)
  issues = monitor.append_text_issue(issues, "cpu", idle_governor_field, idle_governor)
  issues = monitor.append_text_issue(issues, "cpu", "cpuidle.available_governors", available_idle_governors)
  let affinity = monitor.read_value(root, p"proc/self/status", max_bytes: 65536)
  var affinity_cpus = []
  if affinity.observation.state != .Observed or affinity.observation.value == null {
    issues += [monitor.issue("cpu", "affinity", affinity.observation.state, affinity.error_kind, affinity.errno)]
  } else {
    var affinity_text = null
    var duplicate_affinity = false
    for line in affinity.observation.value.lines() {
      if line.starts_with("Cpus_allowed_list:") {
        if affinity_text != null {
          duplicate_affinity = true
        } else {
          affinity_text = (line.split(":", maxsplit: 1).get(1) ?? "").trim()
        }
      }
    }

    if duplicate_affinity {
      issues += [monitor.issue("cpu", "affinity", report.Malformed, "duplicate_cpu_list", null)]
    } else if affinity_text == null {
      issues += [monitor.issue("cpu", "affinity", report.Malformed, "missing_cpu_list", null)]
    } else {
      affinity_cpus = parse_list(affinity_text)
      if affinity_cpus.is_empty() {
        issues += [monitor.issue("cpu", "affinity", report.Malformed, "invalid_cpu_list", null)]
      }
    }
  }

  var idle_states: List[report.CpuIdleState] = []
  for cpu_id in present {
    let cpuidle_path = fp"sys/devices/system/cpu/cpu{cpu_id}/cpuidle".require(RelPath)?
    let cpuidle = root.children(cpuidle_path, max_entries: 256)?
    if cpuidle.state != "complete" and cpuidle.state != "absent" {
      issues += [
        monitor.issue(
          "cpu",
          f"cpu{cpu_id}.cpuidle",
          monitor.live_source_observation_state(cpuidle.state, false),
          cpuidle.error_kind,
          cpuidle.errno,
        ),
      ]
    }

    for state_path in cpuidle.children {
      continue unless state_path.name().starts_with("state")
      var state_index = null
      if let Ok(index) = collectors.parse_idle_state_index(state_path.name()) {
        state_index = index
      } else {
        issues += [monitor.issue("cpu", f"cpu{cpu_id}.{state_path.name()}", report.Malformed, "invalid_idle_state_index", null)]
      }

      continue when state_index == null
      let state_number = state_index ?? -1
      let name = monitor.read_value(root, fp"{state_path}/name", max_bytes: 4096)
      let description = monitor.read_value(root, fp"{state_path}/desc", max_bytes: 4096)
      let disable = monitor.read_value(root, fp"{state_path}/disable", max_bytes: 4096)
      let latency = monitor.read_value(root, fp"{state_path}/latency", max_bytes: 4096)
      let residency = monitor.read_value(root, fp"{state_path}/residency", max_bytes: 4096)
      let usage = monitor.read_value(root, fp"{state_path}/usage", max_bytes: 4096)
      let time_counter = monitor.read_value(root, fp"{state_path}/time", max_bytes: 4096)
      let field_prefix = f"cpu{cpu_id}.{state_path.name()}"
      if name.observation.state != .Observed {
        issues += [monitor.issue("cpu", f"{field_prefix}.name", name.observation.state, name.error_kind, name.errno)]
      }

      issues = monitor.append_text_issue(issues, "cpu", f"{field_prefix}.desc", description)
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
        issues = monitor.append_number_issue(issues, "cpu", f"{field_prefix}.{named_number.name}", named_number.number)
      }

      let raw_disable = disable_number.value ?? -1
      var disable_setting = null
      if raw_disable == 0 or raw_disable == 1 {
        disable_setting = raw_disable
      } else if disable_number.value != null {
        issues += [monitor.issue("cpu", f"{field_prefix}.disable", report.Malformed, "invalid_disable_setting", null)]
      }

      idle_states += [
        {
          cpu_id: cpu_id,
          state_index: state_number,
          name: monitor.observed_source_text(name) ?? state_path.name(),
          description: monitor.observed_source_text(description),
          disable_setting: disable_setting,
          latency_us: latency_number.value,
          residency_us: residency_number.value,
          usage_count: usage_number.value,
          time_us: time_number.value,
        },
      ]
    }
  }

  var cpu_state = if issues.len() == initial_issue_count { report.Complete } else { report.Partial }
  if possible_source.observation.state != .Observed {
    cpu_state = report.Partial
  }

  let cpu_section: report.CpuSection = report.CpuSection(
    status: {
      state: cpu_state,
      enumeration_succeeded: present_enumerated,
    },
    possible:,
    present:,
    online:,
    offline:,
    affinity: affinity_cpus,
    effective_cpuset: cpuset_source.cpus,
    global_idle_driver: monitor.observed_source_text(idle_driver),
    global_idle_governor: monitor.observed_source_text(idle_governor),
    cpus: linked_cpus,
    caches:,
    frequency_policies: policies.policies,
    idle_states:,
    vulnerabilities:,
    available_idle_governors: monitor.parse_words(monitor.observed_source_text(available_idle_governors)),
  )
  {...base, cpu: cpu_section, issues: issues}
}



proc read_huge_page_pool(root: FsRoot, huge_path: RelPath, node_id: Int?, field: Str) [fs, error] -> HugePageRead {
  let name = huge_path.name()
  let parts = name.split("-")
  if parts.len() != 2 or parts[0] != "hugepages" or ! parts[1].ends_with("kB") {
    return {pool: null, issues: [monitor.issue("memory", f"{field}.page_size", report.Malformed, "invalid_page_size", null)]}
  }

  let size_text = parts[1].split("kB").get(0) ?? ""
  if f"{size_text}kB" != parts[1] {
    return {pool: null, issues: [monitor.issue("memory", f"{field}.page_size", report.Malformed, "invalid_page_size", null)]}
  }

  let size_kib = monitor.parse_integer(size_text)
  let size_count = size_kib ?? -1
  if size_count <= 0 {
    let state = if size_kib == null and monitor.decimal_identifier(size_text) { report.RangeFailure } else { report.Malformed }
    return {pool: null, issues: [monitor.issue("memory", f"{field}.page_size", state, "invalid_page_size", null)]}
  }

  if size_count > 8796093022207 {
    return {
      pool: null,
      issues: [
        monitor.issue("memory", f"{field}.page_size", report.RangeFailure, "byte_count_overflow", null),
      ],
    }
  }

  let total = monitor.read_value(root, fp"{huge_path}/nr_hugepages", max_bytes: 4096)
  let total_number = collectors.bounded_number(total, true)
  if total_number.value == null {
    let state = total_number.state ?? report.Absent
    let error_kind: Str? = if state == .Absent { "missing_count" } else { total_number.error_kind }
    return {pool: null, issues: [monitor.issue("memory", f"{field}.total", state, error_kind, total_number.errno)]}
  }

  let free = collectors.bounded_number(monitor.read_value(root, fp"{huge_path}/free_hugepages", max_bytes: 4096), true)
  let reserved = collectors.bounded_number(monitor.read_value(root, fp"{huge_path}/resv_hugepages", max_bytes: 4096), true)
  let surplus = collectors.bounded_number(monitor.read_value(root, fp"{huge_path}/surplus_hugepages", max_bytes: 4096), true)
  var issues: List[report.CollectionIssue] = []
  issues = monitor.append_number_issue(issues, "memory", f"{field}.free", free)
  issues = monitor.append_number_issue(issues, "memory", f"{field}.reserved", reserved)
  issues = monitor.append_number_issue(issues, "memory", f"{field}.surplus", surplus)
  {
    pool: {
      node_id: node_id,
      page_size_bytes: size_count * 1024,
      total: total_number.value,
      free: free.value,
      reserved: reserved.value,
      surplus: surplus.value,
    },
    issues: issues,
  }
}

proc collect_memory(root: FsRoot, base: report.SystemReport) [fs, error] -> report.SystemReport {
  let source = monitor.read_value(root, p"proc/meminfo", max_bytes: 1048576)
  let meminfo = memory.parse(source)
  let {total: {bytes: total, ..}, free: {bytes: free, ..}, available: {bytes: available, ..}, buffers: {bytes: buffers, ..}, cached: {bytes: cached, ..}, active: {bytes: active, ..}, inactive: {bytes: inactive, ..}, dirty: {bytes: dirty, ..}, writeback: {bytes: writeback, ..}, swap_total: {bytes: swap_total, ..}, swap_free: {bytes: swap_free, ..}, counters, ..} = meminfo
  var issues = base.issues
  issues += src.with_section("memory", meminfo.issues)

  let swaps_source = monitor.read_value(root, p"proc/swaps", max_bytes: 262144)
  var swaps: List[report.SwapDevice] = []
  var seen_swaps: Set[Str] = set.empty()
  if swaps_source.observation.state != .Observed or swaps_source.observation.value == null {
    issues += [monitor.issue("memory", "swaps", swaps_source.observation.state, swaps_source.error_kind, swaps_source.errno)]
  } else {
    let swap_lines = swaps_source.observation.value.lines()
    if swap_lines.is_empty() or monitor.parse_words(swap_lines[0].replace("\t", with: " ")) != [
      "Filename",
      "Type",
      "Size",
      "Used",
      "Priority",
    ] {
      issues += [monitor.issue("memory", "swaps", report.Malformed, "invalid_swap_header", null)]
    } else {
      for line in swap_lines |> drop(1) {
        let columns = monitor.parse_words(line.replace("\t", with: " "))
        if columns.len() != 5 {
          issues += [monitor.issue("memory", "swaps", report.Malformed, "invalid_swap_row", null)]
          continue
        }

        let size_kib = monitor.parse_integer(columns[2])
        let used_kib = monitor.parse_integer(columns[3])
        let size_count = size_kib ?? -1
        let used_count = used_kib ?? -1
        if size_count < 0 or used_count < 0 {
          let overflow = (size_kib == null and monitor.decimal_identifier(columns[2])) or (used_kib == null and monitor.decimal_identifier(
            columns[3],
          ))
          let state = if overflow { report.RangeFailure } else { report.Malformed }
          let error_kind = if overflow { "swap_counter_out_of_range" } else { "invalid_swap_counter" }
          issues += [monitor.issue("memory", "swaps", state, error_kind, null)]
          continue
        }

        if size_count > 8796093022207 or used_count > 8796093022207 {
          issues += [monitor.issue("memory", "swaps", report.RangeFailure, "byte_count_overflow", null)]
          continue
        }

        let priority = monitor.parse_integer(columns[4])
        if priority == null {
          issues += [monitor.issue("memory", "swaps", report.Malformed, "invalid_swap_priority", null)]
          continue
        }

        if used_count > size_count {
          issues += [monitor.issue("memory", "swaps", report.Malformed, "invalid_swap_usage", null)]
          continue
        }

        let name = sys_mount.decode_mount_field(columns[0])
        if name in seen_swaps {
          issues += [monitor.issue("memory", "swaps", report.Malformed, "duplicate_swap_name", null)]
          continue
        }

        seen_swaps = seen_swaps.add(name)
        swaps += [
          {
            name: {
              state: report.Observed,
              value: name,
              raw_bytes_base64: null,
            },
            kind: columns[1],
            size_bytes: size_count * 1024,
            used_bytes: used_count * 1024,
            priority: priority,
          },
        ]
      }
    }
  }

  var huge_pages: List[report.HugePagePool] = []
  let huge_listing = root.children(p"sys/kernel/mm/hugepages", max_entries: 1024)?
  if huge_listing.state != "complete" and huge_listing.state != "absent" {
    issues += [
      monitor.issue(
        "memory",
        "huge_pages.enumeration",
        monitor.live_source_observation_state(huge_listing.state, false),
        huge_listing.error_kind,
        huge_listing.errno,
      ),
    ]
  }

  for huge_path in huge_listing.children {
    continue unless huge_path.name().starts_with("hugepages-")
    let collected = read_huge_page_pool(root, huge_path, null, f"huge_pages.{huge_path.name()}")
    issues += collected.issues
    if collected.pool != null {
      huge_pages += [collected.pool]
    }
  }

  let node_listing = root.children(p"sys/devices/system/node", max_entries: 1024)?
  if node_listing.state != "complete" and node_listing.state != "absent" {
    issues += [
      monitor.issue(
        "memory",
        "numa.enumeration",
        monitor.live_source_observation_state(node_listing.state, false),
        node_listing.error_kind,
        node_listing.errno,
      ),
    ]
  }

  for node_path in node_listing.children {
    continue unless node_path.name().starts_with("node")
    let node_suffix = node_path.name().split("") |> drop(4).join("")
    let node_id = monitor.parse_integer(node_suffix) ?? -1
    if node_id < 0 {
      issues += [monitor.issue("memory", f"numa.{node_path.name()}", report.Malformed, "invalid_node_id", null)]
      continue
    }

    let node_huge_listing = root.children(fp"{node_path}/hugepages", max_entries: 1024)?
    if node_huge_listing.state != "complete" and node_huge_listing.state != "absent" {
      issues += [
        monitor.issue(
          "memory",
          f"numa.{node_path.name()}.huge_pages",
          monitor.live_source_observation_state(node_huge_listing.state, false),
          node_huge_listing.error_kind,
          node_huge_listing.errno,
        ),
      ]
    }

    for huge_path in node_huge_listing.children {
      continue unless huge_path.name().starts_with("hugepages-")
      let collected = read_huge_page_pool(
        root,
        huge_path,
        node_id,
        f"numa.{node_path.name()}/huge_pages/{huge_path.name()}",
      )
      issues += collected.issues
      if collected.pool != null {
        huge_pages += [collected.pool]
      }
    }
  }

  let cgroup_data = collect_cgroups(root)
  issues += cgroup_data.issues

  var transparent_huge_pages = []
  for name in ["enabled", "defrag"] {
    let policy_path = fp"sys/kernel/mm/transparent_hugepage/{name}".require(RelPath)?
    let policy = monitor.read_value(root, policy_path, max_bytes: 4096)
    continue when policy.observation.state == .Absent
    if policy.observation.state != .Observed or policy.observation.value == null {
      issues += [
        monitor.issue("memory", f"transparent_huge_pages.{name}", policy.observation.state, policy.error_kind, policy.errno),
      ]
      continue
    }

    let policy_text = policy.observation.value
    match collectors.parse_thp_policy(policy_text) {
      Ok(_) => transparent_huge_pages += [f"{name}={policy_text}"]
      Err(_) => issues += [
        monitor.issue("memory", f"transparent_huge_pages.{name}", report.Malformed, "invalid_thp_policy", null),
      ]
    }
  }

  var pressure: List[report.PressureLine] = []
  for resource in ["cpu", "memory", "io"] {
    let pressure_path = fp"proc/pressure/{resource}".require(RelPath)?
    let pressure_source = monitor.read_value(root, pressure_path, max_bytes: 16384)
    if pressure_source.observation.state != .Observed or pressure_source.observation.value == null {
      issues += [
        monitor.issue(
          "memory",
          f"pressure.{resource}",
          pressure_source.observation.state,
          pressure_source.error_kind,
          pressure_source.errno,
        ),
      ]
      continue
    }

    if pressure_source.observation.value.trim() == "" {
      issues += [monitor.issue("memory", f"pressure.{resource}", report.Malformed, "empty_psi_source", null)]
      continue
    }

    var seen_kinds = []
    for line in pressure_source.observation.value.lines() {
      let columns = monitor.parse_words(line.replace("\t", with: " "))
      if columns.len() != 5 or columns[0] not in ["some", "full"] {
        issues += [monitor.issue("memory", f"pressure.{resource}", report.Malformed, "invalid_psi_row", null)]
        continue
      }

      let kind = columns[0]
      let field = f"pressure.{resource}.{kind}"
      if kind in seen_kinds {
        issues += [monitor.issue("memory", field, report.Malformed, "duplicate_psi_kind", null)]
        continue
      }

      seen_kinds += [kind]
      var avg10 = null
      var avg60 = null
      var avg300 = null
      var total_text = null
      var invalid = false
      var seen_fields = []
      for column in columns |> drop(1) {
        let pair = column.split("=", maxsplit: 1)
        if pair.len() != 2 or pair[0] in seen_fields or pair[0] not in ["avg10", "avg60", "avg300", "total"] {
          invalid = true
          continue
        }

        seen_fields += [pair[0]]
        match pair[0] {
          "avg10" => avg10 = pair[1]
          "avg60" => avg60 = pair[1]
          "avg300" => avg300 = pair[1]
          "total" => total_text = pair[1]
          else => {}
        }
      }

      if invalid or avg10 == null or avg60 == null or avg300 == null or total_text == null {
        issues += [monitor.issue("memory", field, report.Malformed, "invalid_psi_row", null)]
        continue
      }

      if ! collectors.valid_psi_average(avg10) or ! collectors.valid_psi_average(avg60) or ! collectors.valid_psi_average(
        avg300,
      ) {
        issues += [monitor.issue("memory", field, report.Malformed, "invalid_psi_average", null)]
        continue
      }

      let total_us = monitor.parse_integer(total_text)
      let total_count = total_us ?? -1
      if total_count < 0 or total_count > 9007199254740991 {
        let range = (total_us == null and monitor.decimal_identifier(total_text)) or total_count > 9007199254740991
        let state = if range { report.RangeFailure } else { report.Malformed }
        issues += [monitor.issue("memory", field, state, "invalid_psi_total", null)]
        continue
      }

      pressure += [
        {
          resource: resource,
          kind: kind,
          avg10: avg10,
          avg60: avg60,
          avg300: avg300,
          total_us: total_count,
        },
      ]
    }
  }

  var numa: List[report.MemoryCounter] = []
  for node_path in node_listing.children {
    continue unless node_path.name().starts_with("node")
    let node_id = monitor.parse_integer(node_path.name().split("") |> drop(4).join("")) ?? -1
    continue when node_id < 0
    let numa_source = monitor.read_value(root, fp"{node_path}/meminfo", max_bytes: 65536)
    if numa_source.observation.state != .Observed or numa_source.observation.value == null {
      issues += [
        monitor.issue(
          "memory",
          f"numa.{node_path.name()}.meminfo",
          numa_source.observation.state,
          numa_source.error_kind,
          numa_source.errno,
        ),
      ]
      continue
    }

    for line_item in numa_source.observation.value.lines() |> enumerate() {
      let line = line_item.value
      let pair = line.split(":", maxsplit: 1)
      if pair.len() != 2 {
        issues += [
          monitor.issue(
            "memory",
            f"numa.{node_path.name()}.meminfo.line.{line_item.index}",
            report.Malformed,
            "missing_field_separator",
            null,
          ),
        ]
        continue
      }

      let labels = monitor.parse_words(pair[0].trim().replace("\t", with: " "))
      let field_name = labels.get(2) ?? ""
      if labels.len() != 3 or labels[0] != "Node" or labels[1] != f"{node_id}" or field_name == "" {
        let field = if field_name != "" {
          f"numa.{node_path.name()}.{field_name}"
        } else {
          f"numa.{node_path.name()}.meminfo.line.{line_item.index}"
        }
        issues += [monitor.issue("memory", field, report.Malformed, "invalid_numa_row_identity", null)]
        continue
      }

      let field = f"numa.{node_path.name()}.{field_name}"
      let values = monitor.parse_words(pair[1].trim().replace("\t", with: " "))
      if values.len() < 1 or values.len() > 2 {
        issues += [monitor.issue("memory", field, report.Malformed, "invalid_numa_columns", null)]
        continue
      }

      let parsed = monitor.parse_integer(values[0])
      let raw = parsed ?? -1
      if raw < 0 {
        let state = if parsed == null and monitor.decimal_identifier(values[0]) {
          report.RangeFailure
        } else {
          report.Malformed
        }
        issues += [monitor.issue("memory", field, state, "invalid_numa_counter", null)]
        continue
      }

      let kib = values.len() == 2 and values[1] == "kB"
      let maximum = if kib { 8796093022207 } else { 9007199254740991 }
      if raw > maximum {
        issues += [monitor.issue("memory", field, report.RangeFailure, "byte_count_overflow", null)]
        continue
      }

      let value = if kib { raw * 1024 } else { raw }
      let unit = if kib { "bytes" } else { "count" }
      numa += [{name: f"node{node_id}.{field_name}", value: value, unit: unit}]
    }
  }

  var state = if source.observation.state == .Observed { report.Complete } else { report.Partial }
  if total == null or issues.len() > base.issues.len() {
    state = report.Partial
  }

  let memory: report.MemorySection = report.MemorySection(
    status: {
      state: state,
      enumeration_succeeded: source.observation.state == .Observed,
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
    swaps:,
    huge_pages:,
    transparent_huge_pages:,
    numa:,
    pressure:,
    cgroup: cgroup_data.resources,
  )
  {...base, memory: memory, issues: issues}
}

pure parent_relative_path(value: Str) -> Str {
  let components = value.split("/") |> where .trim() != ""
  return "" when components.len() <= 1

  components |> take(components.len() - 1).join("/")
}

pure cgroup_observation_state(
  maximum: collectors.SourceRead,
  current: collectors.SourceRead,
) -> report.ObservationState {
  guard maximum.observation.state == .Observed else {
    return maximum.observation.state
  }

  current.observation.state
}

pure cgroup_numeric_state(
  maximum: collectors.SourceRead,
  current: collectors.SourceRead,
  maximum_number: collectors.BoundedNumber,
  current_number: collectors.BoundedNumber,
) -> report.ObservationState {
  guard maximum_number.state == null else {
    return maximum_number.state ?? report.Malformed
  }

  return current_number.state ?? report.Malformed when current_number.state != null

  cgroup_observation_state(maximum, current)
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
  {
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
  if source.observation.state != .Observed or source.observation.value == null {
    return null
  }

  source.observation.value == "max"
}

pure cgroup_maximum(source: collectors.SourceRead) -> collectors.BoundedNumber {
  if source.observation.state == .Observed and source.observation.value == "max" {
    return {value: null, state: null, error_kind: null, errno: null}
  }

  collectors.bounded_number(source, true)
}

pure cgroup_token_number(source: collectors.SourceRead, token: Str) -> collectors.BoundedNumber {
  collectors.bounded_number({...source, observation: {...source.observation, value: token}}, true)
}

pure first_errno(primary: Int?, secondary: Int?) -> Int? {
  guard primary == null else {
    return primary
  }

  secondary
}

proc collect_cgroups(root: FsRoot) [fs, error] -> CgroupCollection {
  let self_cgroup = monitor.read_value(root, p"proc/self/cgroup", max_bytes: 65536)
  let mount_table = sys_mount.collect(root)
  var issues: List[report.CollectionIssue] = []
  if self_cgroup.observation.state != .Observed or self_cgroup.observation.value == null {
    issues += [
      monitor.issue("memory", "cgroup.membership", self_cgroup.observation.state, self_cgroup.error_kind, self_cgroup.errno),
    ]
    return {resources: [], issues: issues}
  }

  if mount_table.source_state != .Observed {
    let failure = mount_table.issues[0]
    issues += [monitor.issue("memory", "cgroup.mountinfo", failure.state, failure.error_kind, failure.errno)]
    return {resources: [], issues: issues}
  }

  let parsed_membership = collectors.parse_unified_cgroup_path(self_cgroup.observation.value)
  if parsed_membership.state == .Malformed {
    issues += [monitor.issue("memory", "cgroup.membership", report.Malformed, "invalid_cgroup_membership", null)]
    return {resources: [], issues: issues}
  }

  let cgroup_path = parsed_membership.path
  let inventory = sys_mount.cgroup_mount_inventory(mount_table)
  if inventory.malformed {
    issues += [monitor.issue("memory", "cgroup.mountinfo", report.Malformed, "invalid_cgroup_mountinfo", null)]
    return {resources: [], issues: issues}
  }

  let has_v1 = parsed_membership.has_v1 or inventory.has_v1

  var resources: List[report.CgroupResource] = []
  if cgroup_path == null or inventory.mounts.is_empty() {
    if has_v1 {
      issues += [monitor.issue("memory", "cgroup.v1", report.Unsupported, "cgroup_v1_or_hybrid", null)]
    } else {
      let state = if self_cgroup.observation.state != .Observed {
        self_cgroup.observation.state
      } else {
        report.Absent
      }
      issues += [monitor.issue("memory", "cgroup_v2", state, "cgroup_v2_mount_unavailable", self_cgroup.errno)]
    }

    return {resources: resources, issues: issues}
  }

  if has_v1 {
    issues += [monitor.issue("memory", "cgroup.v1", report.Unsupported, "cgroup_v1_controllers_present", null)]
  }

  let group_name = cgroup_path
  var selected: collectors.CgroupMount? = null
  if let Ok(mount) = collectors.select_cgroup_mount(group_name, inventory.mounts) {
    selected = mount
  } else {
    issues += [monitor.issue("memory", "cgroup.path", report.Malformed, "invalid_cgroup_mount_path", null)]
    return {resources: resources, issues: issues}
  }

  if selected == null {
    issues += [monitor.issue("memory", "cgroup.path", report.Unsupported, "cgroup_path_outside_visible_mount", null)]
    return {resources: resources, issues: issues}
  }

  let chosen = selected
  let {root: root_path, point: absolute_mount_point, ..} = chosen
  if ! group_name.starts_with("/") {
    issues += [monitor.issue("memory", "cgroup.path", report.Malformed, "invalid_cgroup_mount_path", null)]
    return {resources: resources, issues: issues}
  }

  var relative = ""
  if root_path == "/" {
    relative = group_name.split("") |> drop(1).join("")
  } else if group_name == root_path {
    relative = ""
  } else if group_name.starts_with(f"{root_path}/") {
    let prefix_length = root_path.count_chars() + 1
    relative = group_name.split("") |> drop(prefix_length).join("")
  }

  let mount_relative = (absolute_mount_point.split("/") |> where .trim() != "").join("/")
  var hierarchy_level = 0
  var visited = 0
  var current_relative = relative
  while visited < 64 {
    let source_path = (if current_relative == "" {
      if mount_relative == "" {
        p"."
      } else {
        fp"{mount_relative}"
      }
    } else if mount_relative == "" {
      fp"{current_relative}"
    } else {
      fp"{mount_relative}/{current_relative}"
    }).require(RelPath)?
    let visible_path = if current_relative == "" {
      root_path
    } else if root_path == "/" {
      f"/{current_relative}"
    } else {
      f"{root_path}/{current_relative}"
    }

    let memory_max = monitor.read_value(root, fp"{source_path}/memory.max", max_bytes: 4096)
    let memory_current = monitor.read_value(root, fp"{source_path}/memory.current", max_bytes: 4096)
    if memory_max.observation.state != .Absent or memory_current.observation.state != .Absent {
      let maximum = cgroup_maximum(memory_max)
      let current = collectors.bounded_number(memory_current, true)
      let state = cgroup_numeric_state(memory_max, memory_current, maximum, current)
      resources += [
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
      ]
      issues = monitor.append_number_issue(issues, "memory", f"cgroup.{hierarchy_level}.memory.max", maximum)
      issues = monitor.append_number_issue(issues, "memory", f"cgroup.{hierarchy_level}.memory.current", current)
      if memory_max.observation.state != .Observed or memory_current.observation.state != .Observed {
        issues += [
          monitor.issue(
            "memory",
            f"cgroup.{hierarchy_level}.memory",
            state,
            "cgroup_memory_value_unavailable",
            first_errno(memory_max.errno, memory_current.errno),
          ),
        ]
      }
    }

    let memory_swap_max = monitor.read_value(root, fp"{source_path}/memory.swap.max", max_bytes: 4096)
    let memory_swap_current = monitor.read_value(root, fp"{source_path}/memory.swap.current", max_bytes: 4096)
    if memory_swap_max.observation.state != .Absent or memory_swap_current.observation.state != .Absent {
      let maximum = cgroup_maximum(memory_swap_max)
      let current = collectors.bounded_number(memory_swap_current, true)
      let state = cgroup_numeric_state(memory_swap_max, memory_swap_current, maximum, current)
      resources += [
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
      ]
      issues = monitor.append_number_issue(issues, "memory", f"cgroup.{hierarchy_level}.memory.swap.max", maximum)
      issues = monitor.append_number_issue(issues, "memory", f"cgroup.{hierarchy_level}.memory.swap.current", current)
      if memory_swap_max.observation.state != .Observed or memory_swap_current.observation.state != .Observed {
        issues += [
          monitor.issue(
            "memory",
            f"cgroup.{hierarchy_level}.memory.swap",
            state,
            "cgroup_memory_swap_value_unavailable",
            first_errno(memory_swap_max.errno, memory_swap_current.errno),
          ),
        ]
      }
    }

    let cpu_max = monitor.read_value(root, fp"{source_path}/cpu.max", max_bytes: 4096)
    if cpu_max.observation.state == .Observed and cpu_max.observation.value != null {
      let fields = monitor.parse_words(cpu_max.observation.value)
      if fields.len() == 2 {
        let unlimited = fields[0] == "max"
        var quota_number: collectors.BoundedNumber = collectors.BoundedNumber(
          value: null,
          state: null,
          error_kind: null,
          errno: null,
        )
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
          issues += [monitor.issue("memory", f"cgroup.{hierarchy_level}.cpu.max", state, "invalid_cpu_quota", null)]
        } else if (! unlimited and (quota_number.value ?? -1) <= 0) or (period_number.value ?? -1) <= 0 {
          issues += [monitor.issue("memory", f"cgroup.{hierarchy_level}.cpu.max", report.Malformed, "invalid_cpu_quota", null)]
        } else {
          resources += [
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
          ]
        }
      } else {
        issues += [monitor.issue("memory", f"cgroup.{hierarchy_level}.cpu.max", report.Malformed, "invalid_cpu_max", null)]
      }
    } else if cpu_max.observation.state != .Absent {
      issues += [
        monitor.issue(
          "memory",
          f"cgroup.{hierarchy_level}.cpu.max",
          cpu_max.observation.state,
          cpu_max.error_kind,
          cpu_max.errno,
        ),
      ]
    }

    let cpu_stat = monitor.read_value(root, fp"{source_path}/cpu.stat", max_bytes: 16384)
    if cpu_stat.observation.state == .Observed and cpu_stat.observation.value != null {
      var seen_cpu_stat: Set[Str] = set.empty()
      var duplicate_cpu_stat: Set[Str] = set.empty()
      for line in cpu_stat.observation.value.lines() {
        let fields = monitor.parse_words(line)
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
          if fields[0] in seen_cpu_stat and fields[0] not in duplicate_cpu_stat {
            duplicate_cpu_stat = duplicate_cpu_stat.add(fields[0])
            issues += [
              monitor.issue(
                "memory",
                f"cgroup.{hierarchy_level}.cpu.stat.{fields[0]}",
                report.Malformed,
                "duplicate_cpu_stat_field",
                null,
              ),
            ]
          }

          seen_cpu_stat = seen_cpu_stat.add(fields[0])
        }
      }

      for line in cpu_stat.observation.value.lines() {
        let fields = monitor.parse_words(line)
        if fields.len() != 2 {
          issues += [
            monitor.issue("memory", f"cgroup.{hierarchy_level}.cpu.stat", report.Malformed, "invalid_cpu_stat_row", null),
          ]
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
        continue when fields[0] in duplicate_cpu_stat
        let number = cgroup_token_number(cpu_stat, fields[1])
        if number.value == null {
          issues += [
            monitor.issue(
              "memory",
              f"cgroup.{hierarchy_level}.cpu.stat.{fields[0]}",
              number.state ?? report.Malformed,
              "invalid_cpu_stat_value",
              null,
            ),
          ]
          continue
        }

        let unit = if fields[0].starts_with("nr_") { "count" } else { "microseconds" }
        resources += [
          cgroup_resource(
            visible_path,
            hierarchy_level,
            "cpu",
            f"cpu.stat.{fields[0]}",
            report.Observed,
            null,
            number.value,
            unit,
            null,
            null,
            null,
            [],
          ),
        ]
      }
    } else if cpu_stat.observation.state != .Absent {
      issues += [
        monitor.issue(
          "memory",
          f"cgroup.{hierarchy_level}.cpu.stat",
          cpu_stat.observation.state,
          cpu_stat.error_kind,
          cpu_stat.errno,
        ),
      ]
    }

    let cpuset = monitor.read_value(root, fp"{source_path}/cpuset.cpus.effective", max_bytes: 65536)
    if cpuset.observation.state == .Observed and cpuset.observation.value != null {
      if cpuset.observation.value == "" {
        resources += [
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
        ]
      } else {
        if let Ok(cpus) = report.parse_cpu_list(cpuset.observation.value) {
          resources += [
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
          ]
        } else {
          issues += [
            monitor.issue(
              "memory",
              f"cgroup.{hierarchy_level}.cpuset.cpus.effective",
              report.Malformed,
              "invalid_cgroup_cpu_list",
              null,
            ),
          ]
        }
      }
    } else if cpuset.observation.state != .Absent {
      issues += [
        monitor.issue(
          "memory",
          f"cgroup.{hierarchy_level}.cpuset.cpus.effective",
          cpuset.observation.state,
          cpuset.error_kind,
          cpuset.errno,
        ),
      ]
    }

    let pids_max = monitor.read_value(root, fp"{source_path}/pids.max", max_bytes: 4096)
    let pids_current = monitor.read_value(root, fp"{source_path}/pids.current", max_bytes: 4096)
    if pids_max.observation.state != .Absent or pids_current.observation.state != .Absent {
      let maximum = cgroup_maximum(pids_max)
      let current = collectors.bounded_number(pids_current, true)
      let state = cgroup_numeric_state(pids_max, pids_current, maximum, current)
      resources += [
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
      ]
      issues = monitor.append_number_issue(issues, "memory", f"cgroup.{hierarchy_level}.pids.max", maximum)
      issues = monitor.append_number_issue(issues, "memory", f"cgroup.{hierarchy_level}.pids.current", current)
      if pids_max.observation.state != .Observed or pids_current.observation.state != .Observed {
        issues += [
          monitor.issue(
            "memory",
            f"cgroup.{hierarchy_level}.pids",
            state,
            "cgroup_pids_value_unavailable",
            first_errno(pids_max.errno, pids_current.errno),
          ),
        ]
      }
    }

    let io_stat = monitor.read_value(root, fp"{source_path}/io.stat", max_bytes: 262144)
    if io_stat.observation.state == .Observed and io_stat.observation.value != null {
      var seen_io_devices: Set[Str] = set.empty()
      var duplicate_io_devices: Set[Str] = set.empty()
      for line in io_stat.observation.value.lines() {
        let fields = monitor.parse_words(line)
        if fields.len() >= 1 {
          if fields[0] in seen_io_devices and fields[0] not in duplicate_io_devices {
            duplicate_io_devices = duplicate_io_devices.add(fields[0])
            issues += [
              monitor.issue(
                "memory",
                f"cgroup.{hierarchy_level}.io.stat.{fields[0]}",
                report.Malformed,
                "duplicate_io_device",
                null,
              ),
            ]
          }

          seen_io_devices = seen_io_devices.add(fields[0])
        }
      }

      for line in io_stat.observation.value.lines() {
        let fields = monitor.parse_words(line)
        continue when fields.is_empty()
        let device_parts = fields[0].split(":")
        if device_parts.len() != 2 or cgroup_token_number(io_stat, device_parts[0]).value == null or cgroup_token_number(
          io_stat,
          device_parts[1],
        ).value == null {
          issues += [monitor.issue("memory", f"cgroup.{hierarchy_level}.io.stat", report.Malformed, "invalid_io_device", null)]
          continue
        }

        continue when fields[0] in duplicate_io_devices
        var seen_io_fields: Set[Str] = set.empty()
        var duplicate_io_fields: Set[Str] = set.empty()
        for item in fields |> drop(1) {
          let pair = item.split("=", maxsplit: 1)
          if pair.len() == 2 and pair[0] in ["rbytes", "wbytes", "rios", "wios", "dbytes", "dios"] {
            if pair[0] in seen_io_fields and pair[0] not in duplicate_io_fields {
              duplicate_io_fields = duplicate_io_fields.add(pair[0])
              issues += [
                monitor.issue(
                  "memory",
                  f"cgroup.{hierarchy_level}.io.stat.{fields[0]}.{pair[0]}",
                  report.Malformed,
                  "duplicate_io_counter",
                  null,
                ),
              ]
            }

            seen_io_fields = seen_io_fields.add(pair[0])
          }
        }

        for item in fields |> drop(1) {
          let pair = item.split("=", maxsplit: 1)
          continue when pair.len() != 2 or pair[0] not in ["rbytes", "wbytes", "rios", "wios", "dbytes", "dios"]
          continue when pair[0] in duplicate_io_fields
          let number = cgroup_token_number(io_stat, pair[1])
          if number.value == null {
            issues += [
              monitor.issue(
                "memory",
                f"cgroup.{hierarchy_level}.io.stat.{fields[0]}.{pair[0]}",
                number.state ?? report.Malformed,
                "invalid_io_counter",
                null,
              ),
            ]
            continue
          }

          let unit = if pair[0].ends_with("bytes") { "bytes" } else { "requests" }
          resources += [
            cgroup_resource(
              visible_path,
              hierarchy_level,
              "io",
              f"io.stat.{fields[0]}.{pair[0]}",
              report.Observed,
              null,
              number.value,
              unit,
              null,
              null,
              null,
              [],
            ),
          ]
        }
      }
    } else if io_stat.observation.state != .Absent {
      issues += [
        monitor.issue(
          "memory",
          f"cgroup.{hierarchy_level}.io.stat",
          io_stat.observation.state,
          io_stat.error_kind,
          io_stat.errno,
        ),
      ]
    }

    return {resources: resources, issues: issues} when current_relative == ""

    current_relative = parent_relative_path(current_relative)
    hierarchy_level += 1
    visited += 1
  }

  issues += [monitor.issue("memory", "cgroup.ancestors", report.Truncated, "ancestor_limit_64", null)]
  {resources: resources, issues: issues}
}

pure network_text(value: Str?) -> report.TextObservation {
  guard value != null else {
    return monitor.empty_text(report.Absent)
  }

  {state: report.Observed, value: value, raw_bytes_base64: null}
}

pure network_link_name(value: LinuxNetworkLink) -> report.TextObservation {
  guard value.name == null else {
    return network_text(value.name)
  }

  if value.name_bytes != null {
    return {state: report.Malformed, value: null, raw_bytes_base64: value.name_bytes.base64()}
  }

  monitor.empty_text(report.Absent)
}

pure network_mac(value: Bytes?) -> report.TextObservation {
  guard value != null else {
    return monitor.empty_text(report.Absent)
  }

  let digits = "0123456789abcdef".split("")
  let parts: List[Str] = collect {
    for octet_bytes in value.chunks(1) {
      if let Ok(octet) = bytes.unpack_le(octet_bytes, 1, 0) {
        yield f"{digits[octet / 16]}{digits[octet % 16]}"
      } else {
        return monitor.empty_text(report.Malformed)
      }
    }
  }

  {state: report.Observed, value: parts.join(":"), raw_bytes_base64: null}
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
    "inet" => "ipv4"
    "inet6" => "ipv6"
    else => value
  }
}

pure network_link_flags(value: Int) -> List[Str] {
  let flags: List[Str] = collect {
    yield "up" when value.bit_and(1) != 0

    yield "broadcast" when value.bit_and(2) != 0

    yield "loopback" when value.bit_and(8) != 0

    yield "point_to_point" when value.bit_and(16) != 0

    yield "running" when value.bit_and(64) != 0

    yield "promiscuous" when value.bit_and(256) != 0

    yield "multicast" when value.bit_and(4096) != 0

    yield "lower_up" when value.bit_and(65536) != 0

    yield "dormant" when value.bit_and(131072) != 0

    yield f"raw_bits={value}"
  }
  flags
}

pure network_operstate(value: Int?) -> Str? {
  guard value != null else {
    return null
  }

  match value {
    0 => "unknown"
    1 => "not_present"
    2 => "down"
    3 => "lower_layer_down"
    4 => "testing"
    5 => "dormant"
    6 => "up"
    else => f"operstate_{value ?? -1}"
  }
}

pure network_address_scope(value: Int) -> Str {
  match value {
    0 => "global"
    200 => "site"
    253 => "link"
    254 => "host"
    255 => "nowhere"
    else => f"scope_{value}"
  }
}

pure network_route_type(value: Int) -> Str {
  match value {
    1 => "unicast"
    2 => "local"
    3 => "broadcast"
    4 => "anycast"
    5 => "multicast"
    6 => "blackhole"
    7 => "unreachable"
    8 => "prohibit"
    9 => "throw"
    10 => "nat"
    11 => "external_resolve"
    else => f"route_type_{value}"
  }
}

pure network_route_protocol(value: Int) -> Str {
  match value {
    0 => "unspecified"
    1 => "redirect"
    2 => "kernel"
    3 => "boot"
    4 => "static"
    8 => "gated"
    9 => "router_advertisement"
    16 => "dhcp"
    else => f"protocol_{value}"
  }
}

pure network_rule_action(value: Int) -> Str {
  match value {
    1 => "to_table"
    2 => "goto"
    3 => "nop"
    6 => "blackhole"
    7 => "unreachable"
    8 => "prohibit"
    else => f"action_{value}"
  }
}

pure network_section_state(value: Str) -> report.SectionState {
  match value {
    "complete" => report.Complete
    "unsupported" => report.SectionUnsupported
    "permission_denied" => report.SectionPermissionDenied
    "malformed" => report.SectionMalformed
    "truncated" => report.SectionTruncated
    "limited" => report.SectionTruncated
    "interrupted" => report.SectionRaced
    else => report.Partial
  }
}

pure network_issue_state(value: Str) -> report.ObservationState {
  match value {
    "permission_denied" => report.PermissionDenied
    "unsupported" => report.Unsupported
    "malformed" => report.Malformed
    "range_failure" => report.RangeFailure
    "truncated" => report.Truncated
    "limited" => report.Truncated
    "interrupted" => report.Raced
    else => report.ReadFailure
  }
}

pure link_index_by_name(links: Map[Int], name: Str?) -> Int? {
  guard let link_name = name else {
    return null
  }
  return links.get(link_name) ?? 0 when link_name in links

  null
}

## Converts one typed route-netlink result into the report's stable network model.
export pure assemble_network_dump(value: LinuxNetworkDump) -> NetworkCollection {
  var addresses_by_link: Map[List[report.NetworkAddress]] = {}
  for raw_address in value.addresses {
    let address_value: Str? = if raw_address.local != null { raw_address.local } else { raw_address.address }
    let link_key = f"{raw_address.ifindex}"
    let address: report.NetworkAddress = report.NetworkAddress(
      family: network_family(raw_address.family),
      address: network_text(address_value),
      prefix_length: raw_address.prefix_length,
      broadcast: network_text(raw_address.broadcast),
      scope: network_address_scope(raw_address.scope),
      flags: raw_address.flags,
      valid_lifetime_seconds: raw_address.valid_lifetime_seconds,
      preferred_lifetime_seconds: raw_address.preferred_lifetime_seconds,
      attributes: network_attributes(raw_address.attributes),
    )
    addresses_by_link = addresses_by_link.push(link_key, address)
  }

  var links: List[report.NetworkLink] = []
  for raw_link in value.links {
    let addresses = addresses_by_link.get(f"{raw_link.ifindex}") ?? []

    let counters: List[report.MemoryCounter] = collect {
      if raw_link.rx_bytes != null {
        yield {name: "rx_bytes", value: raw_link.rx_bytes, unit: "bytes"}
      }

      if raw_link.tx_bytes != null {
        yield {name: "tx_bytes", value: raw_link.tx_bytes, unit: "bytes"}
      }
    }

    links += [
      {
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
      },
    ]
  }

  var routes: List[report.NetworkRoute] = []
  for raw_route in value.routes {
    let family = network_family(raw_route.family)
    let default_destination: Str? = if family == "ipv6" { "::" } else if family == "ipv4" { "0.0.0.0" } else { null }
    let destination: Str? = if raw_route.destination != null { raw_route.destination } else { default_destination }
    var nexthops = [
      {
        ifindex: nexthop.ifindex,
        flags: nexthop.flags,
        hops: nexthop.hops,
        gateway: network_text(nexthop.gateway),
      }
      for nexthop in raw_route.nexthops
    ]
    routes += [
      {
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
      },
    ]
  }

  var link_indices_by_name: Map[Int] = {}
  for raw_link in value.links {
    if raw_link.name != null {
      let link_name = raw_link.name
      if link_name not in link_indices_by_name {
        link_indices_by_name = link_indices_by_name.set(link_name, raw_link.ifindex)
      }
    }
  }

  var rules: List[report.NetworkRule] = [
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
      f"netlink.{raw_issue.object}",
      network_issue_state(raw_issue.state),
      raw_issue.error_kind,
      raw_issue.errno,
      raw_issue.message,
    )
    for raw_issue in value.issues
  ]
  {
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
    is PermissionDenied => "permission_denied"
    is NotFound => "not_found"
    else => "read_failure"
  }
}

proc network_link_target(root: FsRoot, source_path: RelPath, field: Str) [fs, error] -> NetworkLinkTarget {
  if let Ok(observed) = root.readlink_result(source_path) {
    if observed.state == "observed" {
      guard observed.target == null else {
        return {target: observed.target, issues: []}
      }

      return {target: null, issues: [monitor.issue("network", field, report.Malformed, "missing_link_target", null)]}
    }

    return {target: null, issues: []} when observed.state == "absent"

    {
      target: null,
      issues: [
        monitor.issue(
          "network",
          field,
          monitor.live_source_observation_state(observed.state, false),
          observed.error_kind,
          observed.errno,
        ),
      ],
    }
  } else {
    {target: null, issues: [monitor.issue("network", field, report.Malformed, "invalid_link_path", null)]}
  }
}

## Joins network interfaces to visible device sources without treating missing links as failures.
export proc link_network_device_sources(
  root: FsRoot,
  assembled: NetworkCollection,
  pci_functions: List[report.PciFunction],
  usb_devices: List[report.UsbDevice],
) [fs, error] -> NetworkCollection {
  let pci_indices = sys_pci.function_indices(pci_functions)
  let usb_indices = monitor.usb_device_indices(usb_devices)
  var links: List[report.NetworkLink] = []
  var issues = assembled.issues
  for link in assembled.links {
    var driver = null
    var parent_pci_function_index = null
    var parent_usb_device_index = null
    if link.name.value != null {
      let name = link.name.value
      let interface_path = fp"sys/class/net/{name}".require(RelPath)?
      let driver_link = network_link_target(
        root,
        fp"{interface_path}/device/driver",
        f"links.{link.ifindex}.driver",
      )
      issues += driver_link.issues
      if driver_link.target != null {
        driver = driver_link.target.name()
      }

      let device_link = network_link_target(root, fp"{interface_path}/device", f"links.{link.ifindex}.parent")
      issues += device_link.issues
      if device_link.target != null {
        let target = device_link.target
        parent_pci_function_index = sys_pci.function_index(pci_indices, sys_pci.address_in_target(target))
        parent_usb_device_index = monitor.usb_device_index_from_target(usb_indices, target)
      }
    }

    links += [
      {
        ...link,
        driver: driver,
        parent_pci_function_index: parent_pci_function_index,
        parent_usb_device_index: parent_usb_device_index,
      },
    ]
  }

  let status = if issues.len() > assembled.issues.len() and assembled.status.state == .Complete {
    {state: report.Partial, enumeration_succeeded: assembled.status.enumeration_succeeded}
  } else {
    assembled.status
  }
  {...assembled, status: status, links: links, issues: issues}
}

proc collect_network(
  root: FsRoot,
  pci_functions: List[report.PciFunction],
  usb_devices: List[report.UsbDevice],
) [fs, process, env, error] -> NetworkCollection {
  match linux.network_dump() {
    Ok(value) => {
      let assembled = assemble_network_dump(value)
      link_network_device_sources(root, assembled, pci_functions, usb_devices)
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
      {
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
  selected == "" or selected == name
}

pure unsupported_issue(section: Str, field: Str) -> report.CollectionIssue {
  monitor.issue(section, field, report.Unsupported, "collector_not_implemented", null)
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
) [fs, time, error] -> Result[report.SystemReport, Error] {
  if page_size_bytes <= 0 or clock_ticks_per_second <= 0 {
    return Err(
      report.SystemReportError.InvalidExecutionUnits("page size and clock ticks per second must be positive"),
    )
  }

  if selected != "" {
    let _ = report.parse_report_section(selected)?
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
    let sensors = monitor.collect_sensors(root, output.pci.functions, output.usb.devices)
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
    let power = monitor.collect_power(root)
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
    let {name, state, ..} = section_item
    if requested(selected, name) and state == .SectionNotRequested {
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

  Ok(output)
}

pure mark_unsupported(value: report.SystemReport, name: Str) -> report.SystemReport {
  var issues = value.issues
  issues += [unsupported_issue(name, "section")]
  match name {
    "usb" => {...value, usb: {status: empty_status(report.SectionUnsupported), devices: []}, issues: issues}
    "storage" => {
      ...value,
      storage: {
        status: empty_status(report.SectionUnsupported),
        devices: [],
        mounts: [],
      },
      issues: issues,
    }
    "network" => {
      ...value,
      network: {
        status: empty_status(report.SectionUnsupported),
        links: [],
        routes: [],
        rules: [],
      },
      issues: issues,
    }
    "sensors" => {
      ...value,
      sensors: {
        status: empty_status(report.SectionUnsupported),
        channels: [],
        thermal_zones: [],
      },
      issues: issues,
    }
    "power" => {
      ...value,
      power: {
        status: empty_status(report.SectionUnsupported),
        supplies: [],
        cap_zones: [],
      },
      issues: issues,
    }
    "firmware" => {
      ...value,
      firmware: {
        status: empty_status(report.SectionUnsupported),
        source: "unavailable",
        records: [],
        limitation: monitor.empty_text(report.Unsupported),
      },
      issues: issues,
    }
    "kernel" => {
      ...value,
      kernel: {
        status: empty_status(report.SectionUnsupported),
        command_line: monitor.empty_text(report.Unsupported),
        modules: [],
        parameters: [],
        sysctls: [],
      },
      issues: issues,
    }
    "processes" => {
      ...value,
      processes: {
        status: empty_status(report.SectionUnsupported),
        processes: [],
      },
      issues: issues,
    }
    "devices" => {
      ...value,
      devices: {
        status: empty_status(report.SectionUnsupported),
        devices: [],
      },
      issues: issues,
    }
    else => value
  }
}

## Exposes fixture-root and live Linux collection through the same typed report.
export type SystemReportLiveCollector = module {
  export proc collect_from_root(root: FsRoot, architecture: Str, page_size_bytes: Int, clock_ticks_per_second: Int, selected: Str = "", sensitive: Bool = false, include_local_mount_usage: Bool = false) [fs, time, error] -> Result[report.SystemReport, Error]
  export proc collect_live(selected: Str = "", sensitive: Bool = false) [fs, process, env, time, error] -> Result[report.SystemReport, Error]
  export proc link_network_device_sources(root: FsRoot, assembled: NetworkCollection, pci_functions: List[report.PciFunction], usb_devices: List[report.UsbDevice]) [fs, error] -> NetworkCollection
  export proc optional_driver_name(root: FsRoot, source_path: RelPath) [fs, error] -> collectors.SourceRead
  export proc usb_controller_address(root: FsRoot, device_path: RelPath) [fs, error] -> UsbControllerObservation
  export proc class_parent_target(root: FsRoot, entry: RelPath) [fs, error] -> ClassParentObservation
}

## Collects the current process-visible Linux view and eligible local mount capacity.
export proc collect_live(
  selected: Str = "",
  sensitive: Bool = false,
) [fs, process, env, time, error] -> Result[report.SystemReport, Error] {
  let uname = system.uname()?
  if uname.sysname != "Linux" {
    return Err(
      report.SystemReportError.UnsupportedPlatform("live system-report collection is supported on Linux only"),
    )
  }

  let root = fs.open_root(p"/")?
  defer root.close()
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

  Ok(collected)
}
