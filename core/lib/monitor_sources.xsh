##! Reads the Linux sources shared by the system report and the btop monitor.
use sys_pci
use sys_source as src
use sys_usb
use system_report as report
use system_report_collect as collectors

## Builds a text observation with no value in the given state.
export pure empty_text(state: report.ObservationState) -> report.TextObservation {
  {state: state, value: null, raw_bytes_base64: null}
}

## Builds a collection issue for one section field.
export pure issue(
  section: Str,
  field: Str,
  state: report.ObservationState,
  error_kind: Str?,
  errno: Int?,
) -> report.CollectionIssue {
  {
    section: section,
    field: field,
    state: state,
    error_kind: error_kind,
    errno: errno,
    detail: empty_text(state),
  }
}

## Reads one bounded source file as an observation.
export proc read_value(root: FsRoot, source_path: RelPath, max_bytes: Int = 65536) [fs, error] -> collectors.SourceRead {
  src.read_source_text(root, source_path, max_bytes)
}

## Returns the text of a source only when it was observed.
export pure observed_source_text(source: collectors.SourceRead) -> Str? {
  src.observed_text(source)
}

## Parses a decimal integer from optional text.
export pure parse_integer(value: Str?) -> Int? {
  src.parse_integer(value)
}

pure parse_bool01(value: Str?) -> Bool? {
  src.parse_bool01(value)
}

## Splits whitespace-separated text into words.
export pure parse_words(value: Str?) -> List[Str] {
  src.parse_words(value)
}

## Holds the fields parsed from one cpuinfo processor record.
export type CpuInfo = {
  id: Int,
  core_id: Int?,
  vendor: Str?,
  model: Str?,
  family: Str?,
  model_id: Str?,
  stepping: Str?,
  features: List[Str],
  frequency_mhz: Float?,
}

type CpuInfoRead = {infos: List[CpuInfo], state: report.ObservationState, errno: Int?, error_kind: Str?}

## Builds a cpuinfo record with no parsed fields.
export pure empty_cpu_info(id: Int) -> CpuInfo {
  {
    id: id,
    core_id: null,
    vendor: null,
    model: null,
    family: null,
    model_id: null,
    stepping: null,
    features: [],
    frequency_mhz: null,
  }
}

pure parse_cpu_frequency_mhz(value: Str) -> Float? {
  return null when value == ""
  var dot_seen = false
  var digits = 0
  for character in value {
    if character == "." and ! dot_seen {
      dot_seen = true
      continue
    }
    return null when character < "0" or character > "9"
    digits += 1
  }
  return null when digits == 0
  if let Ok(frequency) = value.parse_float() {
    if frequency > 0.0 { frequency } else { null }
  } else {
    null
  }
}

## Reads and parses the cpuinfo processor records.
export proc read_cpu_info(root: FsRoot) [fs, error] -> CpuInfoRead {
  let source = read_value(root, p"proc/cpuinfo", max_bytes: 8388608)
  if source.observation.state != .Observed or source.observation.value == null {
    return {infos: [], state: source.observation.state, errno: source.errno, error_kind: source.error_kind}
  }

  var current = empty_cpu_info(-1)
  var has_current = false
  let values: List[CpuInfo] = collect {
    for line in source.observation.value.lines() {
      if line.trim() == "" {
        if has_current {
          yield current
          has_current = false
        }

        continue
      }

      let pair = line.split(":", maxsplit: 1)
      continue when pair.len() != 2
      let key = pair[0].trim()
      let value = pair[1].trim()
      if key == "processor" or key == "processor number" {
        yield current when has_current

        current = empty_cpu_info(parse_integer(value) ?? -1)
        has_current = true
        continue
      }

      continue unless has_current
      match key {
        "core id" => current = {...current, core_id: parse_integer(value)}
        "vendor_id" | "CPU implementer" => current = {...current, vendor: value}
        "model name" | "Processor" | "Hardware" => current = {...current, model: value}
        "cpu family" | "CPU architecture" => current = {...current, family: value}
        "model" | "CPU part" => current = {...current, model_id: value}
        "stepping" => current = {...current, stepping: value}
        "flags" | "Features" => current = {...current, features: parse_words(value)}
        "cpu MHz" => current = {...current, frequency_mhz: parse_cpu_frequency_mhz(value)}
        else => {}
      }
    }

    yield current when has_current
  }

  {
    infos: values
      |> where .id >= 0
      |> sort-by .id,
    state: report.Observed,
    errno: null,
    error_kind: null,
  }
}

type SensorCollection = {
  status: report.SectionStatus,
  channels: List[report.SensorChannel],
  thermal_zones: List[report.ThermalZone],
  issues: List[report.CollectionIssue],
}

## Retains the power attributes used by the monitor without widening report JSON.
export type MonitorBattery = {
  name: Str,
  kind: Str?,
  present: Bool?,
  status: Str?,
  online: Bool?,
  capacity_percent: Int?,
  energy_now_uwh: Int?,
  energy_full_uwh: Int?,
  charge_now_uah: Int?,
  charge_full_uah: Int?,
  power_now_uw: Int?,
  current_now_ua: Int?,
  voltage_now_uv: Int?,
  time_to_empty_minutes: Int?,
}

## Maps Linux logical CPU IDs to physical core IDs read from procfs.
export type CpuCoreIdentity = {logical_id: Int, core_id: Int}

type PowerCollection = {
  status: report.SectionStatus,
  supplies: List[report.PowerSupply],
  cap_zones: List[report.PowerCapZone],
  issues: List[report.CollectionIssue],
  monitor_batteries: List[MonitorBattery],
}

## Groups the existing system-report readers needed for monitor telemetry.
export type MonitorSources = {
  sensor_state: report.SectionState,
  sensor_channels: List[report.SensorChannel],
  thermal_zones: List[report.ThermalZone],
  frequency_policies: List[report.CpuFreqPolicy],
  cpuinfo_state: report.ObservationState,
  cpuinfo_frequencies_mhz: List[Float],
  cpu_core_identities: List[CpuCoreIdentity],
  power_state: report.SectionState,
  batteries: List[MonitorBattery],
}

type PowerCapSource = {path: RelPath, name: collectors.SourceRead}

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
  cpu_ticks: Int?,
  cpu_ticks_state: report.ObservationState?,
  virtual_bytes: Int?,
  resident_pages: Int?,
  field_issues: List[ProcStatFieldIssue],
}

## Reports whether text is a run of decimal digits.
export pure decimal_identifier(value: Str) -> Bool {
  src.decimal_digits(value)
}

pure proc_stat_error(message: Str) -> Error {
  report.SystemReportError.InvalidProcStat(message)
}

type ProcOptionalNumber = {value: Int?, issue_state: report.ObservationState?}

type ProcCounter = {value: Int?, issue_state: report.ObservationState?}

# Parses a signed-i64 kernel counter without narrowing it to JSON's exact integer range.
pure proc_counter(raw: Str) -> ProcCounter {
  let parsed = parse_integer(raw)
  if parsed == null {
    return {value: null, issue_state: if decimal_identifier(raw) { report.RangeFailure } else { report.Malformed }}
  }
  let number = parsed ?? -1
  return {value: null, issue_state: report.Malformed} when number < 0
  {value: number, issue_state: null}
}

## Parses an optional process counter bounded to the exact-number range.
export pure proc_optional_number(raw: Str, positive: Bool = false) -> ProcOptionalNumber {
  let parsed = parse_integer(raw)
  if parsed == null {
    return {value: null, issue_state: if decimal_identifier(raw) { report.RangeFailure } else { report.Malformed }}
  }

  let number = parsed ?? -1
  if number < 0 or (positive and number == 0) {
    return {value: null, issue_state: report.Malformed}
  }

  return {value: null, issue_state: report.RangeFailure} when number > 9007199254740991

  {value: number, issue_state: null}
}

# Parses a process stat record while preserving the command's parentheses.
pure parse_proc_stat_with_policy(text: Str, allow_wide_start_ticks: Bool) -> Result[ProcStat, Error] {
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
    command_parts += [pieces[piece_index]]
    piece_index += 1
  }

  let command = command_parts.join(") ")
  let fields = parse_words(pieces[-1])
  if fields.len() < 22 {
    return Err(proc_stat_error("process stat record is missing resource fields"))
  }

  let parent_pid = parse_integer(fields[1]) ?? -1
  let start_ticks = parse_integer(fields[19]) ?? -1
  if parent_pid < 0 or start_ticks < 0 or parent_pid > 9007199254740991 or
      (! allow_wide_start_ticks and start_ticks > 9007199254740991) {
    return Err(proc_stat_error("process stat record has invalid parent or start identity"))
  }

  let thread_count = proc_optional_number(fields[17], positive: true)
  let user_ticks = proc_counter(fields[11])
  let system_ticks = proc_counter(fields[12])
  var cpu_ticks = null
  var cpu_ticks_state: report.ObservationState? = null
  if user_ticks.issue_state != null {
    cpu_ticks_state = user_ticks.issue_state
  } else if system_ticks.issue_state != null {
    cpu_ticks_state = system_ticks.issue_state
  } else {
    let user_value = user_ticks.value ?? 0
    let system_value = system_ticks.value ?? 0
    if user_value > 9223372036854775807 - system_value {
      cpu_ticks_state = report.RangeFailure
    } else {
      cpu_ticks = user_value + system_value
    }
  }
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
  Ok({
    pid: pid,
    parent_pid: parent_pid,
    command: command,
    state: fields[0],
    thread_count: thread_count.value,
    start_ticks: start_ticks,
    cpu_ticks: cpu_ticks,
    cpu_ticks_state: cpu_ticks_state,
    virtual_bytes: virtual_bytes.value,
    resident_pages: resident_pages.value,
    field_issues: field_issues,
  })
}

## Parses process identities under the exact-number bound of system-report JSON v1.
export pure parse_proc_stat(text: Str) -> Result[ProcStat, Error] {
  parse_proc_stat_with_policy(text, false)
}

## Parses the same proc ABI with an i64 start-time counter for monitor identity.
export pure parse_proc_stat_for_monitor(text: Str) -> Result[ProcStat, Error] {
  parse_proc_stat_with_policy(text, true)
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
      let suffix = channel.split("") |> drop(spec.prefix.count_chars()).join("")
      return spec.kind when decimal_identifier(suffix)
    }
  }

  null
}

pure sensor_unit(kind: Str) -> Str {
  match kind {
    "temperature" => "millidegrees_celsius"
    "voltage" => "millivolts"
    "fan" => "rpm"
    "power" => "microwatts"
    "energy" => "microjoules"
    "current" => "milliamps"
    else => "raw"
  }
}

## Adds an issue for a numeric observation that did not parse.
export pure append_number_issue(
  issues: List[report.CollectionIssue],
  section: Str,
  field: Str,
  number: collectors.BoundedNumber,
) -> List[report.CollectionIssue] {
  guard number.state != null else {
    return issues
  }

  issues.push(issue(section, field, number.state ?? report.Malformed, number.error_kind, number.errno))
}

## Adds an issue for a text observation that was not observed.
export pure append_text_issue(
  issues: List[report.CollectionIssue],
  section: Str,
  field: Str,
  source: collectors.SourceRead,
) -> List[report.CollectionIssue] {
  let state = source.observation.state
  return issues when state == .Observed or state == .Absent

  issues.push(issue(section, field, state, source.error_kind, source.errno))
}

## Collects hwmon channels and thermal zones from a rooted tree.
export proc collect_sensors(
  root: FsRoot,
  pci_functions: List[report.PciFunction],
  usb_devices: List[report.UsbDevice],
) [fs, error] -> SensorCollection {
  let pci_indices = sys_pci.function_indices(pci_functions)
  let usb_indices = usb_device_indices(usb_devices)
  let hwmon_listing = root.children(p"sys/class/hwmon", max_entries: 1024)?
  let thermal_listing = root.children(p"sys/class/thermal", max_entries: 1024)?
  var channels: List[report.SensorChannel] = []
  var zones: List[report.ThermalZone] = []
  var issues: List[report.CollectionIssue] = []
  if hwmon_listing.state != "complete" and hwmon_listing.state != "absent" {
    issues += [
      issue(
        "sensors",
        "hwmon",
        live_source_observation_state(hwmon_listing.state, false),
        hwmon_listing.error_kind,
        hwmon_listing.errno,
      ),
    ]
  }

  for chip_path in hwmon_listing.children {
    continue unless chip_path.name().starts_with("hwmon")
    let chip_name_source = read_value(root, fp"{chip_path}/name", max_bytes: 4096)
    issues = append_text_issue(issues, "sensors", f"hwmon.{chip_path.name()}.name", chip_name_source)
    let chip = observed_source_text(chip_name_source) ?? chip_path.name()
    let parent = src.class_parent_target(root, chip_path)
    if parent.state != .Observed {
      issues += [issue("sensors", f"hwmon.{chip_path.name()}.parent", parent.state, parent.error_kind, parent.errno)]
    }

    var parent_pci_address = null
    var parent_usb_index = null
    if parent.target != null {
      let target = parent.target
      parent_pci_address = sys_pci.address_in_target(target)
      parent_usb_index = usb_device_index_from_target(usb_indices, target)
    }

    let attribute_listing = root.children(chip_path, max_entries: 1024)?
    if attribute_listing.state != "complete" {
      issues += [
        issue(
          "sensors",
          f"hwmon.{chip_path.name()}.attributes",
          live_source_observation_state(attribute_listing.state, false),
          attribute_listing.error_kind,
          attribute_listing.errno,
        ),
      ]
    }

    for attribute in attribute_listing.children {
      let attribute_name = attribute.name()
      continue unless attribute_name.ends_with("_input")
      let channel_name = attribute_name.split("") |> take(attribute_name.count_chars() - 6).join("")
      let label_path = fp"{chip_path}/{channel_name}_label".require(RelPath)?
      let minimum_path = fp"{chip_path}/{channel_name}_min".require(RelPath)?
      let maximum_path = fp"{chip_path}/{channel_name}_max".require(RelPath)?
      let critical_path = fp"{chip_path}/{channel_name}_crit".require(RelPath)?
      let alarm_path = fp"{chip_path}/{channel_name}_alarm".require(RelPath)?
      let kind = sensor_kind(channel_name)
      let sensor_kind_name = kind ?? "unknown"
      let value = read_value(root, attribute, max_bytes: 4096)
      let label = read_value(root, label_path, max_bytes: 4096)
      let minimum = read_value(root, minimum_path, max_bytes: 4096)
      let maximum = read_value(root, maximum_path, max_bytes: 4096)
      let critical = read_value(root, critical_path, max_bytes: 4096)
      let alarm = read_value(root, alarm_path, max_bytes: 4096)
      issues = append_text_issue(issues, "sensors", f"hwmon.{chip_path.name()}.{channel_name}_label", label)
      let measured = collectors.bounded_number(value, false)
      let minimum_number = collectors.bounded_number(minimum, false)
      let maximum_number = collectors.bounded_number(maximum, false)
      let critical_number = collectors.bounded_number(critical, false)
      let alarm_number = collectors.bounded_number(alarm, true)
      issues = append_number_issue(issues, "sensors", f"hwmon.{chip_path.name()}.{attribute_name}", measured)
      issues = append_number_issue(issues, "sensors", f"hwmon.{chip_path.name()}.{channel_name}_min", minimum_number)
      issues = append_number_issue(issues, "sensors", f"hwmon.{chip_path.name()}.{channel_name}_max", maximum_number)
      issues = append_number_issue(
        issues,
        "sensors",
        f"hwmon.{chip_path.name()}.{channel_name}_crit",
        critical_number,
      )
      issues = append_number_issue(issues, "sensors", f"hwmon.{chip_path.name()}.{channel_name}_alarm", alarm_number)
      var alarm_value = null
      if alarm_number.value != null {
        if alarm_number.value == 0 {
          alarm_value = false
        } else if alarm_number.value == 1 {
          alarm_value = true
        } else {
          issues += [
            issue(
              "sensors",
              f"hwmon.{chip_path.name()}.{channel_name}_alarm",
              report.Malformed,
              "invalid_boolean",
              null,
            ),
          ]
        }
      }

      channels += [
        {
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
          parent_pci_function_index: sys_pci.function_index(pci_indices, parent_pci_address),
          parent_usb_device_index: parent_usb_index,
        },
      ]
    }
  }

  if thermal_listing.state != "complete" and thermal_listing.state != "absent" {
    issues += [
      issue(
        "sensors",
        "thermal_zones",
        live_source_observation_state(thermal_listing.state, false),
        thermal_listing.error_kind,
        thermal_listing.errno,
      ),
    ]
  }

  for zone_path in thermal_listing.children {
    continue unless zone_path.name().starts_with("thermal_zone")
    let id = parse_integer(zone_path.name().split("") |> drop("thermal_zone".count_chars()).join("")) ?? -1
    if id < 0 or id > 9007199254740991 or zone_path.name() != f"thermal_zone{id}" {
      issues += [
        issue("sensors", f"thermal_zones.{zone_path.name()}", report.Malformed, "invalid_thermal_zone_id", null),
      ]
      continue
    }

    let kind = read_value(root, fp"{zone_path}/type", max_bytes: 4096)
    issues = append_text_issue(issues, "sensors", f"thermal_zones.{zone_path.name()}.type", kind)
    let temperature = read_value(root, fp"{zone_path}/temp", max_bytes: 4096)
    let temperature_number = collectors.bounded_number(temperature, false)
    issues = append_number_issue(issues, "sensors", f"thermal_zones.{zone_path.name()}.temp", temperature_number)
    let attributes = root.children(zone_path, max_entries: 256)?
    if attributes.state != "complete" {
      issues += [
        issue(
          "sensors",
          f"thermal_zones.{zone_path.name()}.attributes",
          live_source_observation_state(attributes.state, false),
          attributes.error_kind,
          attributes.errno,
        ),
      ]
    }

    var trip_indices = []
    var seen_trip_indices: Set[Str] = set.empty()
    for attribute in attributes.children {
      let attribute_name = attribute.name()
      continue when ! attribute_name.starts_with("trip_point_") or ! attribute_name.ends_with("_temp")
      let trip_number = parse_integer(attribute_name.split("_").get(2) ?? "") ?? -1
      if trip_number < 0 or trip_number > 9007199254740991 or attribute_name != f"trip_point_{trip_number}_temp" or f"{trip_number}" in seen_trip_indices {
        issues += [
          issue(
            "sensors",
            f"thermal_zones.{zone_path.name()}.{attribute_name}",
            report.Malformed,
            "invalid_thermal_trip_index",
            null,
          ),
        ]
        continue
      }

      seen_trip_indices = seen_trip_indices.add(f"{trip_number}")
      trip_indices += [trip_number]
    }

    let trips: List[report.ThermalTrip] = collect {
      for trip_number in trip_indices |> sort-by . {
        let temperature_path = fp"{zone_path}/trip_point_{trip_number}_temp".require(RelPath)?
        let type_path = fp"{zone_path}/trip_point_{trip_number}_type".require(RelPath)?
        let hysteresis_path = fp"{zone_path}/trip_point_{trip_number}_hyst".require(RelPath)?
        let trip_temp = read_value(root, temperature_path, max_bytes: 4096)
        let trip_type = read_value(root, type_path, max_bytes: 4096)
        issues = append_text_issue(
          issues,
          "sensors",
          f"thermal_zones.{zone_path.name()}.trip_point_{trip_number}_type",
          trip_type,
        )
        let hysteresis = read_value(root, hysteresis_path, max_bytes: 4096)
        let trip_number_value = collectors.bounded_number(trip_temp, false)
        let hysteresis_number = collectors.bounded_number(hysteresis, true)
        issues = append_number_issue(
          issues,
          "sensors",
          f"thermal_zones.{zone_path.name()}.trip_point_{trip_number}_temp",
          trip_number_value,
        )
        issues = append_number_issue(
          issues,
          "sensors",
          f"thermal_zones.{zone_path.name()}.trip_point_{trip_number}_hyst",
          hysteresis_number,
        )
        yield {
          index: trip_number,
          kind: observed_source_text(trip_type) ?? "unknown",
          temperature_millidegrees: trip_number_value.value,
          hysteresis_millidegrees: hysteresis_number.value,
        }
      }
    }

    zones += [
      {
        id: id,
        kind: observed_source_text(kind),
        temperature_millidegrees: temperature_number.value,
        trips: trips,
        parent_device_class_index: null,
      },
    ]
  }

  var state = report.Complete
  if hwmon_listing.state == "absent" and thermal_listing.state == "absent" {
    state = report.SectionAbsent
  } else if ! issues.is_empty() or (hwmon_listing.state != "complete" and hwmon_listing.state != "absent") or (thermal_listing.state != "complete" and thermal_listing.state != "absent") {
    state = report.Partial
  }

  {
    status: {
      state: state,
      enumeration_succeeded: (hwmon_listing.enumeration_succeeded or hwmon_listing.state == "absent") and (thermal_listing.enumeration_succeeded or thermal_listing.state == "absent"),
    },
    channels: channels,
    thermal_zones: zones,
    issues: issues,
  }
}

## Collects power supplies, capping zones and monitor batteries from a rooted tree.
export proc collect_power(root: FsRoot) [fs, error] -> PowerCollection {
  let supply_listing = root.children(p"sys/class/power_supply", max_entries: 1024)?
  let cap_listing = root.children(p"sys/class/powercap", max_entries: 1024)?
  var supplies: List[report.PowerSupply] = []
  var monitor_batteries: List[MonitorBattery] = []
  var cap_zones: List[report.PowerCapZone] = []
  var issues: List[report.CollectionIssue] = []
  if supply_listing.state != "complete" and supply_listing.state != "absent" {
    issues += [
      issue(
        "power",
        "supplies",
        live_source_observation_state(supply_listing.state, false),
        supply_listing.error_kind,
        supply_listing.errno,
      ),
    ]
  }

  if cap_listing.state != "complete" and cap_listing.state != "absent" {
    issues += [
      issue(
        "power",
        "cap_zones",
        live_source_observation_state(cap_listing.state, false),
        cap_listing.error_kind,
        cap_listing.errno,
      ),
    ]
  }

  for supply_path in supply_listing.children {
    let kind = read_value(root, fp"{supply_path}/type", max_bytes: 4096)
    let status = read_value(root, fp"{supply_path}/status", max_bytes: 4096)
    let present = read_value(root, fp"{supply_path}/present", max_bytes: 4096)
    let online_direct = read_value(root, fp"{supply_path}/online", max_bytes: 4096)
    let online_ac0 = read_value(root, fp"{supply_path}/AC0/online", max_bytes: 4096)
    let online_ac = read_value(root, fp"{supply_path}/AC/online", max_bytes: 4096)
    let health = read_value(root, fp"{supply_path}/health", max_bytes: 4096)
    let capacity = read_value(root, fp"{supply_path}/capacity", max_bytes: 4096)
    let energy_now = read_value(root, fp"{supply_path}/energy_now", max_bytes: 4096)
    let energy_full = read_value(root, fp"{supply_path}/energy_full", max_bytes: 4096)
    let charge_now = read_value(root, fp"{supply_path}/charge_now", max_bytes: 4096)
    let charge_full = read_value(root, fp"{supply_path}/charge_full", max_bytes: 4096)
    let voltage = read_value(root, fp"{supply_path}/voltage_now", max_bytes: 4096)
    let current = read_value(root, fp"{supply_path}/current_now", max_bytes: 4096)
    let power = read_value(root, fp"{supply_path}/power_now", max_bytes: 4096)
    let time_to_empty = read_value(root, fp"{supply_path}/time_to_empty", max_bytes: 4096)
    let cycles = read_value(root, fp"{supply_path}/cycle_count", max_bytes: 4096)
    let capacity_number = collectors.bounded_number(capacity, true)
    let energy_now_number = collectors.bounded_number(energy_now, true)
    let energy_full_number = collectors.bounded_number(energy_full, true)
    let charge_now_number = collectors.bounded_number(charge_now, true)
    let charge_full_number = collectors.bounded_number(charge_full, true)
    let voltage_number = collectors.bounded_number(voltage, true)
    let current_number = collectors.bounded_number(current, false)
    let power_number = collectors.bounded_number(power, true)
    let time_to_empty_number = collectors.bounded_number(time_to_empty, true)
    let cycles_number = collectors.bounded_number(cycles, true)
    let present_number = collectors.bounded_number(present, true)
    let online = if online_ac0.observation.state == .Observed {
      online_ac0
    } else if online_ac.observation.state == .Observed {
      online_ac
    } else {
      online_direct
    }
    let online_number = collectors.bounded_number(online, true)
    let prefix = f"supplies.{supply_path.name()}"
    issues = append_text_issue(issues, "power", f"{prefix}.type", kind)
    issues = append_text_issue(issues, "power", f"{prefix}.status", status)
    issues = append_text_issue(issues, "power", f"{prefix}.health", health)
    issues = append_number_issue(issues, "power", f"{prefix}.capacity", capacity_number)
    issues = append_number_issue(issues, "power", f"{prefix}.energy_now", energy_now_number)
    issues = append_number_issue(issues, "power", f"{prefix}.energy_full", energy_full_number)
    issues = append_number_issue(issues, "power", f"{prefix}.charge_now", charge_now_number)
    issues = append_number_issue(issues, "power", f"{prefix}.charge_full", charge_full_number)
    issues = append_number_issue(issues, "power", f"{prefix}.voltage_now", voltage_number)
    issues = append_number_issue(issues, "power", f"{prefix}.current_now", current_number)
    issues = append_number_issue(issues, "power", f"{prefix}.cycle_count", cycles_number)
    let capacity_out_of_range = capacity_number.value != null and capacity_number.value > 100
    let capacity_value: Int? = if capacity_out_of_range { null } else { capacity_number.value }
    if capacity_out_of_range {
      issues += [issue("power", f"{prefix}.capacity", report.Malformed, "percent_out_of_range", null)]
    }

    supplies += [
      {
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
      },
    ]

    let kind_text = observed_source_text(kind)
    if kind_text == "Battery" or kind_text == "UPS" {
      monitor_batteries += [
        {
          name: supply_path.name(),
          kind: kind_text,
          present: if present_number.value == 1 { true } else if present_number.value == 0 { false } else { null },
          status: observed_source_text(status),
          online: if online_number.value == 1 { true } else if online_number.value == 0 { false } else { null },
          capacity_percent: capacity_value,
          energy_now_uwh: energy_now_number.value,
          energy_full_uwh: energy_full_number.value,
          charge_now_uah: charge_now_number.value,
          charge_full_uah: charge_full_number.value,
          power_now_uw: power_number.value,
          current_now_ua: current_number.value,
          voltage_now_uv: voltage_number.value,
          time_to_empty_minutes: time_to_empty_number.value,
        },
      ]
    }
  }

  let cap_paths: List[PowerCapSource] = collect {
    for item in cap_listing.children {
      let name = read_value(root, fp"{item}/name", max_bytes: 4096)
      continue when name.observation.state == .Absent
      issues = append_text_issue(issues, "power", f"cap_zones.{item.name()}.name", name)
      yield {path: item, name: name}
    }
  }

  for cap_source in cap_paths {
    let {path: zone_path, name, ..} = cap_source
    let energy = read_value(root, fp"{zone_path}/energy_uj", max_bytes: 4096)
    let range = read_value(root, fp"{zone_path}/max_energy_range_uj", max_bytes: 4096)
    let energy_number = collectors.bounded_number(energy, true)
    let range_number = collectors.bounded_number(range, true)
    issues = append_number_issue(issues, "power", f"cap_zones.{zone_path.name()}.energy_uj", energy_number)
    issues = append_number_issue(issues, "power", f"cap_zones.{zone_path.name()}.max_energy_range_uj", range_number)
    let cap_attributes = root.children(zone_path, max_entries: 256)?
    if cap_attributes.state != "complete" {
      issues += [
        issue(
          "power",
          f"cap_zones.{zone_path.name()}.attributes",
          live_source_observation_state(cap_attributes.state, false),
          cap_attributes.error_kind,
          cap_attributes.errno,
        ),
      ]
    }

    var constraints: List[report.PowerCapConstraint] = []
    for attribute in cap_attributes.children {
      continue when ! attribute.name().starts_with("constraint_") or ! attribute.name().ends_with("_power_limit_uw")
      let index_text = attribute.name().split("_").get(1) ?? ""
      let index_number = parse_integer(index_text)
      let parsed_index = index_number ?? -1
      if ! decimal_identifier(index_text) or index_number == null or parsed_index > 9007199254740991 or index_text != f"{parsed_index}" {
        issues += [
          issue(
            "power",
            f"cap_zones.{zone_path.name()}.{attribute.name()}",
            report.Malformed,
            "invalid_constraint_index",
            null,
          ),
        ]
        continue
      }

      let index = parsed_index
      let limit = read_value(root, attribute, max_bytes: 4096)
      let limit_number = collectors.bounded_number(limit, true)
      issues = append_number_issue(issues, "power", f"cap_zones.{zone_path.name()}.{attribute.name()}", limit_number)
      let name_path = fp"{zone_path}/constraint_{index}_name".require(RelPath)?
      let window_path = fp"{zone_path}/constraint_{index}_time_window_us".require(RelPath)?
      let name_source = read_value(root, name_path, max_bytes: 4096)
      let window = read_value(root, window_path, max_bytes: 4096)
      let window_number = collectors.bounded_number(window, true)
      issues = append_text_issue(
        issues,
        "power",
        f"cap_zones.{zone_path.name()}.constraint_{index}_name",
        name_source,
      )
      issues = append_number_issue(
        issues,
        "power",
        f"cap_zones.{zone_path.name()}.constraint_{index}_time_window_us",
        window_number,
      )
      let constraint_name = observed_source_text(name_source)
      let observed_constraint: report.PowerCapConstraint = report.PowerCapConstraint(
        index:,
        name: constraint_name,
        power_limit_uw: limit_number.value,
        time_window_us: window_number.value,
      )
      var inserted = false
      let ordered: List[report.PowerCapConstraint] = collect {
        for existing in constraints {
          if ! inserted and index < existing.index {
            yield observed_constraint
            inserted = true
          }

          yield existing
        }

        yield observed_constraint unless inserted
      }

      constraints = ordered
    }

    var parent_name = null
    let link = root.readlink_result(zone_path)?
    if link.state == "observed" {
      let candidate = (link.target ?? p"").parent().name()
      if cap_paths |> any .path.name() == candidate {
        parent_name = candidate
      }
    } else if link.state != "read_failure" or link.errno != 22 {
      issues += [
        issue(
          "power",
          f"cap_zones.{zone_path.name()}.parent",
          live_source_observation_state(link.state, false),
          link.error_kind,
          link.errno,
        ),
      ]
    }

    cap_zones += [
      {
        entry_name: zone_path.name(),
        name: observed_source_text(name) ?? zone_path.name(),
        parent: parent_name,
        energy_uj: energy_number.value,
        maximum_energy_range_uj: range_number.value,
        constraints: constraints,
      },
    ]
  }

  var state = report.Complete
  if supply_listing.state == "absent" and cap_listing.state == "absent" {
    state = report.SectionAbsent
  } else if ! issues.is_empty() {
    state = report.Partial
  }

  {
    status: {
      state: state,
      enumeration_succeeded: (supply_listing.enumeration_succeeded or supply_listing.state == "absent") and (cap_listing.enumeration_succeeded or cap_listing.state == "absent"),
    },
    supplies: supplies,
    cap_zones: cap_zones,
    issues: issues,
    monitor_batteries: monitor_batteries |> sort-by .name,
  }
}

## Captures the existing Linux readers needed by btop without building a full report.
export proc collect_monitor_sources(root: FsRoot) [fs, error] -> MonitorSources {
  let sensors = collect_sensors(root, [], [])
  let frequencies = collect_frequency_policies(root, [])
  let power = collect_power(root)
  let cpu_info = read_cpu_info(root)
  let cpuinfo_frequencies: List[Float] = collect {
    for info in cpu_info.infos {
      if let frequency = info.frequency_mhz {
        yield frequency
      }
    }
  }
  let cpu_core_identities: List[CpuCoreIdentity] = collect {
    for info in cpu_info.infos {
      if let core_id = info.core_id {
        yield {logical_id: info.id, core_id: core_id}
      }
    }
  }
  {
    sensor_state: sensors.status.state,
    sensor_channels: sensors.channels,
    thermal_zones: sensors.thermal_zones,
    frequency_policies: frequencies.policies,
    cpuinfo_state: cpu_info.state,
    cpuinfo_frequencies_mhz: cpuinfo_frequencies,
    cpu_core_identities: cpu_core_identities,
    power_state: power.status.state,
    batteries: power.monitor_batteries,
  }
}

## Finds the USB device index named by a class-entry link target.
export pure usb_device_index_from_target(indices: Map[Int], target: Path) -> Int? {
  for component in target.display().split("/") {
    let candidate = component.split(":").get(0) ?? ""
    return indices.get(candidate) ?? 0 when candidate in indices
  }

  null
}

## Indexes USB devices by their sysfs names.
export pure usb_device_indices(devices: List[report.UsbDevice]) -> Map[Int] {
  sys_usb.name_indices([device.sysfs_name for device in devices])
}

## Maps a source read state to an observation state.
export pure live_source_observation_state(state: Str, truncated: Bool) -> report.ObservationState {
  src.source_state(state, truncated)
}

type PolicyCollection = {policies: List[report.CpuFreqPolicy], issues: List[report.CollectionIssue]}

## Collects cpufreq policies from a rooted tree.
export proc collect_frequency_policies(root: FsRoot, issues: List[report.CollectionIssue]) [fs, error] -> PolicyCollection {
  let listing = root.children(p"sys/devices/system/cpu/cpufreq", max_entries: 1024)?
  var policies: List[report.CpuFreqPolicy] = []
  var collected_issues = issues
  if listing.state == "absent" {
    collected_issues += [issue("cpu", "frequency_policies", report.Absent, "cpufreq_not_exposed", null)]
  } else if listing.state != "complete" {
    collected_issues += [
      issue(
        "cpu",
        "frequency_policies",
        live_source_observation_state(listing.state, false),
        listing.error_kind,
        listing.errno,
      ),
    ]
  }

  let boost = read_value(root, p"sys/devices/system/cpu/cpufreq/boost", max_bytes: 4096)
  let no_turbo = read_value(root, p"sys/devices/system/cpu/intel_pstate/no_turbo", max_bytes: 4096)
  collected_issues = append_text_issue(collected_issues, "cpu", "boost", boost)
  collected_issues = append_text_issue(collected_issues, "cpu", "intel_pstate.no_turbo", no_turbo)
  let boost_text = observed_source_text(boost)
  let no_turbo_text = observed_source_text(no_turbo)
  var boost_supported = null
  var boost_allowed = null
  var boost_scope = null
  if boost_text != null {
    let parsed = parse_bool01(boost_text)
    if parsed == null {
      collected_issues += [issue("cpu", "boost", report.Malformed, "invalid_boost_value", null)]
    } else {
      boost_supported = true
      boost_allowed = parsed
      boost_scope = "system"
    }
  } else if no_turbo_text != null {
    let parsed = parse_bool01(no_turbo_text)
    if parsed == null {
      collected_issues += [issue("cpu", "intel_pstate.no_turbo", report.Malformed, "invalid_no_turbo_value", null)]
    } else {
      boost_supported = true
      boost_allowed = ! parsed
      boost_scope = "intel_pstate"
    }
  }

  for directory in listing.children {
    continue unless directory.name().starts_with("policy")
    let related = read_value(root, fp"{directory}/related_cpus", max_bytes: 4096)
    let affected = read_value(root, fp"{directory}/affected_cpus", max_bytes: 4096)
    let driver = read_value(root, fp"{directory}/scaling_driver", max_bytes: 4096)
    let governor = read_value(root, fp"{directory}/scaling_governor", max_bytes: 4096)
    let available_governors = read_value(root, fp"{directory}/scaling_available_governors", max_bytes: 4096)
    let hardware_min = read_value(root, fp"{directory}/cpuinfo_min_freq", max_bytes: 4096)
    let hardware_max = read_value(root, fp"{directory}/cpuinfo_max_freq", max_bytes: 4096)
    let scaling_min = read_value(root, fp"{directory}/scaling_min_freq", max_bytes: 4096)
    let scaling_max = read_value(root, fp"{directory}/scaling_max_freq", max_bytes: 4096)
    let hardware_current = read_value(root, fp"{directory}/cpuinfo_cur_freq", max_bytes: 4096)
    let scaling_current = read_value(root, fp"{directory}/scaling_cur_freq", max_bytes: 4096)
    let average = read_value(root, fp"{directory}/cpuinfo_avg_freq", max_bytes: 4096)
    let bios_limit = read_value(root, fp"{directory}/bios_limit", max_bytes: 4096)
    let epp = read_value(root, fp"{directory}/energy_performance_preference", max_bytes: 4096)
    let available_epp = read_value(root, fp"{directory}/energy_performance_available_preferences", max_bytes: 4096)
    let available_frequency = read_value(root, fp"{directory}/scaling_available_frequencies", max_bytes: 65536)
    if related.observation.state != .Observed {
      collected_issues += [
        issue("cpu", f"{directory.name()}.related_cpus", related.observation.state, related.error_kind, related.errno),
      ]
    }

    if affected.observation.state != .Observed {
      collected_issues += [
        issue(
          "cpu",
          f"{directory.name()}.affected_cpus",
          affected.observation.state,
          affected.error_kind,
          affected.errno,
        ),
      ]
    }

    if driver.observation.state != .Observed {
      collected_issues += [
        issue("cpu", f"{directory.name()}.driver", driver.observation.state, driver.error_kind, driver.errno),
      ]
    }

    if governor.observation.state != .Observed {
      collected_issues += [
        issue("cpu", f"{directory.name()}.governor", governor.observation.state, governor.error_kind, governor.errno),
      ]
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
        f"{directory.name()}.{named_source.name}",
        named_source.source,
      )
    }

    var related_cpus = []
    var affected_cpus = []
    let related_text = observed_source_text(related)
    let affected_text = observed_source_text(affected)
    if related_text != null {
      if let Ok(ids) = collectors.parse_cpufreq_members(related_text) {
        related_cpus = ids
      } else {
        collected_issues += [
          issue("cpu", f"{directory.name()}.related_cpus", report.Malformed, "invalid_cpu_list", null),
        ]
      }
    }

    if affected_text != null {
      if let Ok(ids) = collectors.parse_cpufreq_members(affected_text) {
        affected_cpus = ids
      } else {
        collected_issues += [
          issue("cpu", f"{directory.name()}.affected_cpus", report.Malformed, "invalid_cpu_list", null),
        ]
      }
    }

    let frequencies: List[Int] = collect {
      for value in parse_words(observed_source_text(available_frequency)) {
        let parsed = collectors.bounded_number(
          {...available_frequency, observation.value: value},
          true,
        )
        collected_issues = append_number_issue(
          collected_issues,
          "cpu",
          f"{directory.name()}.scaling_available_frequencies",
          parsed,
        )
        let exact = parsed.value ?? -1
        yield exact when exact >= 0
      }
    }

    let hardware_min_number = collectors.bounded_number(hardware_min, true)
    let hardware_max_number = collectors.bounded_number(hardware_max, true)
    let scaling_min_number = collectors.bounded_number(scaling_min, true)
    let scaling_max_number = collectors.bounded_number(scaling_max, true)
    let hardware_current_number = collectors.bounded_number(hardware_current, true)
    let scaling_current_number = collectors.bounded_number(scaling_current, true)
    let average_number = collectors.bounded_number(average, true)
    var governor_requested_number = null
    if observed_source_text(governor) == "userspace" {
      let requested_source = read_value(root, fp"{directory}/scaling_setspeed", max_bytes: 4096)
      let requested_number = collectors.bounded_number(requested_source, true)
      collected_issues = append_number_issue(
        collected_issues,
        "cpu",
        f"{directory.name()}.scaling_setspeed",
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
        f"{directory.name()}.{named_number.name}",
        named_number.number,
      )
    }

    policies += [
      {
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
      },
    ]
  }

  {policies: policies, issues: collected_issues}
}
