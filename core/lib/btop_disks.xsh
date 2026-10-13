##! Joins captured mount capacity with block-device counters for btop.
##!
##! The block ABI has one shared parser; this module owns only monitor identity,
##! interval math, mount grouping, and the 512-byte accounting-sector rule.
use btop_rates as rates
use sys_block as block
use sys_mount as mounts
use system_report as report

## Keeps current counters and the timestamped slow mount-capacity observation.
export type Source = {
  blocks: block.BlockInventory,
  mounts: mounts.MountTable,
  mounts_observed_at_ns: Int,
}

## Holds one mounted device without summing counters from stacked layers.
export type Device = {
  identity: Str?,
  identity_state: report.ObservationState,
  name: Str?,
  major: Int,
  minor: Int,
  mount_targets: List[Str],
  filesystem: Str,
  capacity_state: report.ObservationState,
  capacity_age_ns: Int?,
  total_bytes: Int?,
  used_bytes: Int?,
  available_bytes: Int?,
  read_sectors: Int?,
  read_state: report.ObservationState,
  read_bytes_per_second: rates.Rate,
  write_sectors: Int?,
  write_state: report.ObservationState,
  write_bytes_per_second: rates.Rate,
  in_flight: Int?,
  in_flight_state: report.ObservationState,
}

## Retains source availability separately from each mounted device's fields.
export type Panel = {
  block_state: report.ObservationState,
  mount_state: report.ObservationState,
  devices: List[Device],
}

type CounterValue = {value: Int?, state: report.ObservationState}
const max_counter = 9223372036854775807

## Refreshes block counters while reusing slow mount capacity until its deadline.
export proc collect(
  root: FsRoot,
  at_ns: Int,
  cached_mounts: mounts.MountTable? = null,
  cached_mounts_at_ns: Int? = null,
  captured_usage: List[mounts.CapturedUsage]? = null,
) [fs, error] -> Result[Source, Error] {
  guard at_ns >= 0 else {
    return Err(error.failure("btop storage capture time must be nonnegative"))
  }
  let blocks = block.collect(root)
  if (cached_mounts != null and cached_mounts_at_ns == null) or
      (cached_mounts == null and cached_mounts_at_ns != null) {
    return Err(error.failure("cached mount capacity requires its original observation time"))
  }
  if cached_mounts != null and captured_usage != null {
    return Err(error.failure("btop storage capture cannot combine cached and recorded mount capacity"))
  }
  let mount_table = if let replay_usage = captured_usage {
    let raw_mounts = mounts.collect(root)
    mounts.apply_captured_usage(raw_mounts, replay_usage)?
  } else if let current_mounts = cached_mounts {
    current_mounts
  } else {
    mounts.collect(root, include_usage: true)
  }
  Ok({
    blocks: blocks,
    mounts: mount_table,
    mounts_observed_at_ns: cached_mounts_at_ns ?? at_ns,
  })
}

pure block_state(source: Source) -> report.ObservationState {
  return report.Observed when source.blocks.enumeration_succeeded
  match source.blocks.listing_state {
    "absent" => report.Absent
    "permission_denied" => report.PermissionDenied
    "truncated" => report.Truncated
    else => report.ReadFailure
  }
}

pure counter(source: Source, device: block.BlockDevice?, name: Str) -> CounterValue {
  guard device != null else {
    let state = block_state(source)
    return {value: null, state: if state == .Observed { report.Absent } else { state }}
  }
  let current = device ?? source.blocks.devices[0]
  for value in current.io_counters {
    return {value: value.value, state: report.Observed} when value.name == name
  }

  let field = f"devices.{current.name}.stat.{name}"
  for issue in source.blocks.issues {
    return {value: null, state: issue.state} when issue.field == field
  }
  for issue in source.blocks.issues {
    if issue.field == f"devices.{current.name}.stat" {
      return {value: null, state: issue.state}
    }
  }
  {value: null, state: report.Absent}
}

pure device_identity(device: block.BlockDevice?) -> Str? {
  if let current = device {
    if current.disk_sequence_state != .Observed or current.disk_sequence == null or
        current.major == null or current.minor == null {
      return null
    }
    return f"{current.disk_sequence}:{current.major}:{current.minor}"
  }
  null
}

pure find_device(source: Source, major: Int, minor: Int) -> block.BlockDevice? {
  let indices = block.index_by_number(source.blocks.devices)
  let index = block.index_of_number(indices, major, minor)
  if index != null { source.blocks.devices[index ?? -1] } else { null }
}

pure previous_device(source: Source?, current: block.BlockDevice?) -> block.BlockDevice? {
  if let before = source {
    if let current_device = current {
      return null when current_device.major == null or current_device.minor == null
      let candidate = find_device(before, current_device.major ?? -1, current_device.minor ?? -1)
      return candidate when device_identity(candidate) == device_identity(current_device)
    }
  }
  null
}

pure sector_rate(current: CounterValue, previous: CounterValue, elapsed_ns: Int?, gap: Bool) -> rates.Rate {
  return {state: rates.Unavailable, value: null} when current.state != .Observed
  return {state: rates.Warming, value: null} when elapsed_ns == null
  let elapsed = elapsed_ns ?? 0
  return {state: rates.Invalid, value: null} when elapsed <= 0
  if previous.state != .Observed or previous.value == null {
    return {state: rates.Warming, value: null}
  }
  return {state: rates.Gap, value: null} when gap

  let now = current.value ?? -1
  let before = previous.value ?? -1
  return {state: rates.Invalid, value: null} when now < 0 or before < 0
  return {state: rates.Reset, value: null} when now < before
  let delta = now - before
  return {state: rates.Invalid, value: null} when delta > max_counter / 512
  rates.counter_rate(delta * 512, 0, elapsed)
}

pure selected_mount(entries: List[mounts.MountEntry]) -> mounts.MountEntry {
  for entry in entries {
    return entry when entry.usage_state == .Observed
  }
  entries[0]
}

pure grouped_targets(entries: List[mounts.MountEntry]) -> List[Str] {
  var values = []
  for entry in entries {
    if entry.target not in values {
      values += [entry.target]
    }
  }
  values
}

## Derives one row per mounted device number, never summing stacked counters.
export pure step(
  current: Source,
  previous: Source?,
  at_ns: Int,
  previous_at_ns: Int?,
  gap: Bool = false,
) -> Panel {
  let indices = block.index_by_number(current.blocks.devices)
  var seen: Set[Str] = set.empty()
  var devices: List[Device] = []
  for mount in current.mounts.mounts {
    let key = f"{mount.major}:{mount.minor}"
    continue when key in seen
    seen = seen.add(key)
    let entries = current.mounts.mounts |> where .major == mount.major and .minor == mount.minor
    let capacity = selected_mount(entries)
    let block_index = block.index_of_number(indices, mount.major, mount.minor)
    let device: block.BlockDevice? = if block_index == null { null } else { current.blocks.devices[block_index ?? -1] }
    let prior = previous_device(previous, device)
    let read = counter(current, device, "read_sectors")
    let prior_read = counter(previous ?? current, prior, "read_sectors")
    let write = counter(current, device, "write_sectors")
    let prior_write = counter(previous ?? current, prior, "write_sectors")
    let flight = counter(current, device, "in_flight")
    let identity = device_identity(device)
    let elapsed_ns: Int? = if previous_at_ns == null {
      null
    } else if at_ns < 0 or (previous_at_ns ?? -1) < 0 {
      0
    } else {
      at_ns - (previous_at_ns ?? at_ns)
    }
    devices += [
      {
        identity: identity,
        identity_state: if let found = device {
          found.disk_sequence_state
        } else if block_state(current) == .Observed {
          report.Absent
        } else {
          block_state(current)
        },
        name: if let found = device { found.name } else { null },
        major: mount.major,
        minor: mount.minor,
        mount_targets: grouped_targets(entries),
        filesystem: capacity.filesystem,
        capacity_state: capacity.usage_state,
        capacity_age_ns: if at_ns >= current.mounts_observed_at_ns { at_ns - current.mounts_observed_at_ns } else { null },
        total_bytes: capacity.usage_total_bytes,
        used_bytes: capacity.usage_used_bytes,
        available_bytes: capacity.usage_available_bytes,
        read_sectors: read.value,
        read_state: read.state,
        read_bytes_per_second: if identity == null {
          {state: rates.Unavailable, value: null}
        } else {
          sector_rate(read, prior_read, elapsed_ns, gap)
        },
        write_sectors: write.value,
        write_state: write.state,
        write_bytes_per_second: if identity == null {
          {state: rates.Unavailable, value: null}
        } else {
          sector_rate(write, prior_write, elapsed_ns, gap)
        },
        in_flight: flight.value,
        in_flight_state: flight.state,
      },
    ]
  }

  {
    block_state: block_state(current),
    mount_state: current.mounts.source_state,
    devices: devices,
  }
}
