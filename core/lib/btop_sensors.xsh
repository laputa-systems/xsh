##! Selects monitor telemetry from the shared Linux report readers.
use system_report as report
use system_report_live as live

## Retains a CPU temperature in Celsius and maps labeled core sensors to logical IDs.
export type CpuTemperature = {
  state: report.ObservationState,
  package_millidegrees: Int?,
  cores: List[CoreTemperature],
}

## Associates one observed core temperature with its Linux logical CPU ID.
export type CoreTemperature = {id: Int, millidegrees: Int}

## Retains the selected frequency and range in MHz.
export type CpuFrequency = {
  state: report.ObservationState,
  mode: FrequencyMode,
  value_mhz: Float?,
  minimum_mhz: Float?,
  maximum_mhz: Float?,
}

## Names the upstream frequency reduction modes.
export enum FrequencyMode: Str {
  First = "first",
  Range = "range",
  Lowest = "lowest",
  Highest = "highest",
  Average = "average",
}

## Describes one selected battery using kernel-exported values only.
export type Battery = {
  state: report.ObservationState,
  name: Str?,
  status: Str?,
  capacity_percent: Int?,
  power_watts: Float?,
  minutes_remaining: Int?,
}

## Groups the CPU and battery telemetry shown by the monitor.
export type Telemetry = {temperature: CpuTemperature, frequency: CpuFrequency, battery: Battery}

## Holds a rounded temperature and the symbol for its display scale.
export type TemperatureDisplay = {value: Int, unit: Str}

type TemperatureCandidate = {key: Str, order_key: Str, label: Str, millidegrees: Int?}
type CoreMapping = {logical_id: Int, sensor_index: Int}

pure empty_temperature(state: report.ObservationState) -> CpuTemperature {
  {state: state, package_millidegrees: null, cores: []}
}

pure label_or_channel(channel: report.SensorChannel) -> Str {
  channel.label.value ?? channel.channel
}

pure core_number(label: Str) -> Int? {
  return null unless label.starts_with("Core ")
  let suffix = label.split("") |> drop(5).join("")
  if let Ok(value) = suffix.parse_int() {
    return value when value >= 0 and f"{value}" == suffix
  }
  null
}

pure temperature_candidates(sources: live.MonitorSources) -> List[TemperatureCandidate] {
  let channels: List[TemperatureCandidate] = collect {
    for channel in sources.sensor_channels {
      continue unless channel.kind == "temperature"
      continue when "nvme" in channel.chip.lower()
      let label = label_or_channel(channel)
      yield {
        key: f"{channel.chip}/{label}",
        order_key: f"{channel.chip_entry_name ?? channel.chip}/{channel.channel}",
        label: label,
        millidegrees: channel.value,
      }
    }
  }
  let zones: List[TemperatureCandidate] = collect {
    for zone in sources.thermal_zones {
      yield {
        key: f"thermal{zone.id}/{zone.kind ?? f"zone{zone.id}"}",
        order_key: f"thermal{zone.id}",
        label: zone.kind ?? f"zone{zone.id}",
        millidegrees: zone.temperature_millidegrees,
      }
    }
  }
  channels.extend(zones) |> sort-by { |item| f"{item.label.lower()}/{item.key}/{item.order_key}" }
}

pure selected_cpu_sensor(candidates: List[TemperatureCandidate]) -> TemperatureCandidate? {
  for candidate in candidates {
    continue when candidate.millidegrees == null
    if candidate.label.starts_with("Package id") or candidate.label.starts_with("Tdie") or candidate.label.starts_with("SoC Temperature") {
      return candidate
    }
  }
  for candidate in candidates {
    continue when candidate.millidegrees == null
    if "cpu" in candidate.label.lower() or "cpu" in candidate.key.lower() or "k10temp" in candidate.key.lower() {
      return candidate
    }
  }

  if let Ok(first) = candidates.get(0) { first } else { null }
}

# Orders distinct Core and Tccd channels for stable logical-core mapping.
pure core_sensor_candidates(candidates: List[TemperatureCandidate]) -> List[TemperatureCandidate] {
  var seen: Set[Str] = set.empty()
  let ordered = candidates
    |> where .label.starts_with("Core") or .label.starts_with("Tccd")
    |> sort-by { |item| item.key }
    |> sort-by { |item| item.key.count_chars() }
  let unique: List[TemperatureCandidate] = collect {
    for item in ordered {
      continue when item.key in seen
      seen = seen.add(item.key)
      yield item
    }
  }
  unique
}

# Core IDs come from procfs, so scaling stays bounded even for an extreme value.
pure core_sensor_index(core_id: Int, maximum_core_id: Int, sensor_count: Int) -> Int {
  let scaled = (core_id.float() * sensor_count.float() / (maximum_core_id.float() + 1.0)).floor() ?? 0
  if scaled < 0 {
    0
  } else if scaled >= sensor_count {
    sensor_count - 1
  } else {
    scaled
  }
}

# Uses the empty default override: procfs topology first, then a deterministic spread.
pure core_mapping(
  sources: live.MonitorSources,
  logical_cpu_ids: List[Int],
  sensor_count: Int,
) -> List[CoreMapping] {
  return [] when logical_cpu_ids.is_empty() or sensor_count <= 0
  var identities: Map[Int] = {}
  var maximum_core_id = -1
  for identity in sources.cpu_core_identities {
    continue unless identity.logical_id in logical_cpu_ids and identity.core_id >= 0
    let key = f"{identity.logical_id}"
    continue when key in identities
    identities = identities.set(key, identity.core_id)
    maximum_core_id = if identity.core_id > maximum_core_id { identity.core_id } else { maximum_core_id }
  }
  let has_complete_topology = ! identities.is_empty() and identities.len() == logical_cpu_ids.len()
  if has_complete_topology {
    return [
      {
        logical_id: cpu_id,
        sensor_index: core_sensor_index(
          if let Ok(core_id) = identities.get(f"{cpu_id}") { core_id } else { 0 },
          maximum_core_id,
          sensor_count,
        ),
      }
      for cpu_id in logical_cpu_ids
    ]
  }

  let half = logical_cpu_ids.len() / 2
  if logical_cpu_ids.len() % 2 == 0 and identities.len() == half {
    let base = [
      if let Ok(core_id) = identities.get(f"{item.value}") {
        core_sensor_index(core_id, maximum_core_id, sensor_count)
      } else {
        item.index % sensor_count
      }
      for item in logical_cpu_ids |> take(half) |> enumerate()
    ]
    return [
      {
        logical_id: item.value,
        sensor_index: if item.index < half { base[item.index] } else { base[item.index - half] },
      }
      for item in logical_cpu_ids |> enumerate()
    ]
  }

  [
    {
      logical_id: item.value,
      sensor_index: item.index * sensor_count / logical_cpu_ids.len(),
    }
    for item in logical_cpu_ids |> enumerate()
  ]
}

pure cpu_temperature(sources: live.MonitorSources, logical_cpu_ids: List[Int]) -> CpuTemperature {
  let candidates = temperature_candidates(sources)
  let observed = candidates |> where .millidegrees != null
  if observed.is_empty() {
    let has_temperature_channels = sources.sensor_channels |> any .kind == "temperature"
    let has_zones = ! sources.thermal_zones.is_empty()
    let state = if has_temperature_channels or has_zones {
      report.Malformed
    } else if sources.sensor_state == .Partial {
      report.ReadFailure
    } else {
      report.Absent
    }
    return empty_temperature(state)
  }

  let package = selected_cpu_sensor(observed)
  let core_sensors = core_sensor_candidates(candidates)
  let labeled_cpu_ids: List[Int] = collect {
    for item in core_sensors {
      if let id = core_number(item.label) {
        yield id
      }
    }
  }
  let cpu_ids = if logical_cpu_ids.is_empty() { labeled_cpu_ids } else { logical_cpu_ids }
  let mapping = core_mapping(sources, cpu_ids, core_sensors.len())
  let cores: List[CoreTemperature] = collect {
    for item in mapping {
      if let Ok(observed_sensor) = core_sensors.get(item.sensor_index) {
        if let millidegrees = observed_sensor.millidegrees {
          yield {id: item.logical_id, millidegrees: millidegrees}
        }
      }
    }
  }

  {
    state: report.Observed,
    package_millidegrees: if let sensor = package { sensor.millidegrees } else { null },
    cores: cores |> sort-by .id,
  }
}

pure frequency_value_mhz(frequency_khz: Int?) -> Float? {
  return null when frequency_khz == null
  let value = (frequency_khz).float() / 1000.0
  return null when value <= 1.0 or value >= 999999999.0
  value
}

## Formats an observed MHz value with the pinned MHz, GHz, or THz unit.
export pure format_frequency_mhz(value: Float) -> Str {
  if value > 999999.0 {
    f"{(value / 1000000.0).format(precision: 1)} THz"
  } else if value > 999.0 {
    f"{(value / 1000.0).format(precision: 1)} GHz"
  } else {
    f"{value.format(precision: 0)} MHz"
  }
}

pure selected_frequency_mhz(sources: live.MonitorSources) -> List[Float] {
  let policy_values: List[Float] = collect {
    for policy in sources.frequency_policies |> sort-by .name {
      let frequency = frequency_value_mhz(policy.scaling_current_khz)
      if let value = frequency {
        yield value
      }
    }
  }
  return policy_values unless policy_values.is_empty()

  let cpuinfo_values = sources.cpuinfo_frequencies_mhz |> where . > 1.0 and . < 999999999.0
  cpuinfo_values |> take(1)
}

## Reduces the observed policy or CPU information values with one pinned mode.
export pure frequency(sources: live.MonitorSources, mode: FrequencyMode) -> CpuFrequency {
  let values = selected_frequency_mhz(sources)
  if values.is_empty() {
    let has_policy = ! sources.frequency_policies.is_empty()
    let has_cpuinfo = ! sources.cpuinfo_frequencies_mhz.is_empty()
    return {
      state: if has_policy or has_cpuinfo { report.Malformed } else if sources.cpuinfo_state != .Observed {
        sources.cpuinfo_state
      } else {
        report.Malformed
      },
      mode: mode,
      value_mhz: null,
      minimum_mhz: null,
      maximum_mhz: null,
    }
  }

  let first = values[0]
  var minimum = first
  var maximum = first
  var sum = 0.0
  for value in values {
    minimum = if value < minimum { value } else { minimum }
    maximum = if value > maximum { value } else { maximum }
    sum += value
  }
  let average = sum / values.len().float()
  {
    state: report.Observed,
    mode: mode,
    value_mhz: match mode {
      First => first
      Range => null
      Lowest => minimum
      Highest => maximum
      Average => average
    },
    minimum_mhz: if mode == .Range { minimum } else { null },
    maximum_mhz: if mode == .Range { maximum } else { null },
  }
}

pure battery_percent(battery: live.MonitorBattery) -> Int? {
  let reported = battery.capacity_percent
  if reported != null and (reported ?? -1) >= 0 and (reported) <= 100 {
    return reported
  }

  let energy_now = battery.energy_now_uwh ?? -1
  let energy_full = battery.energy_full_uwh ?? 0
  let charge_now = battery.charge_now_uah ?? -1
  let charge_full = battery.charge_full_uah ?? 0
  let ratio = if energy_now >= 0 and energy_full > 0 {
    energy_now.float() / energy_full.float()
  } else if charge_now >= 0 and charge_full > 0 {
    charge_now.float() / charge_full.float()
  } else {
    return null
  }
  return null when ratio < 0.0 or ratio > 1.0
  (ratio * 100.0).round() ?? 0
}

pure battery_watts(battery: live.MonitorBattery) -> Float? {
  if battery.power_now_uw != null {
    let microwatts = battery.power_now_uw
    return null when microwatts < 0
    microwatts.float() / 1000000.0
  } else if battery.current_now_ua != null and battery.voltage_now_uv != null {
    let current = battery.current_now_ua
    let voltage = battery.voltage_now_uv
    return null when current == 0 or voltage <= 0
    (if current < 0 { -current } else { current }).float() * voltage.float() / 1000000000000.0
  } else {
    null
  }
}

pure battery_minutes(battery: live.MonitorBattery) -> Int? {
  let status = (battery.status ?? "").lower()
  if status == "charging" {
    let now = battery.energy_now_uwh ?? -1
    let full = battery.energy_full_uwh ?? -1
    let power = battery.power_now_uw ?? -1
    if now >= 0 and full >= now and power > 0 {
      return ((full - now).float() / power.float() * 60.0).round() ?? 0
    }
    let charge = battery.charge_now_uah ?? -1
    let charge_full = battery.charge_full_uah ?? -1
    let current = battery.current_now_ua ?? 0
    if charge >= 0 and charge_full >= charge and current != 0 {
      return ((charge_full - charge).float() / (if current < 0 { -current } else { current }).float() * 60.0).round() ?? 0
    }
  } else if status != "full" {
    let now = battery.energy_now_uwh ?? -1
    let power = battery.power_now_uw ?? -1
    return (now.float() / power.float() * 60.0).round() ?? 0 when now >= 0 and power > 0
    let charge = battery.charge_now_uah ?? -1
    let current = battery.current_now_ua ?? 0
    if charge >= 0 and current != 0 {
      return (charge.float() / (if current < 0 { -current } else { current }).float() * 60.0).round() ?? 0
    }
  }

  battery.time_to_empty_minutes
}

pure battery(sources: live.MonitorSources) -> Battery {
  let candidates = sources.batteries
    |> where .present == true
    |> sort-by { |item| f"{item.kind == "UPS"}/{item.name}" }
  if candidates.is_empty() {
    let has_unconfirmed = sources.batteries |> any .present == null
    return {
      state: if has_unconfirmed or sources.power_state == .Partial {
        report.ReadFailure
      } else {
        report.Absent
      },
      name: null,
      status: null,
      capacity_percent: null,
      power_watts: null,
      minutes_remaining: null,
    }
  }

  let selected = candidates[0]
  let percent = battery_percent(selected)
  let watts = battery_watts(selected)
  let reported_status = (selected.status ?? "").lower()
  let status = if reported_status != "" and reported_status != "unknown" {
    reported_status
  } else if selected.online == true and (percent ?? 0) < 100 {
    "charging"
  } else if selected.online == true {
    "full"
  } else if selected.online == false {
    "discharging"
  } else {
    "unknown"
  }
  {
    state: if percent == null { report.Malformed } else { report.Observed },
    name: selected.name,
    status: status.lower(),
    capacity_percent: percent,
    power_watts: watts,
    minutes_remaining: battery_minutes(selected),
  }
}

## Applies the pinned default sensor and frequency choices to rooted observations.
export pure select(sources: live.MonitorSources, logical_cpu_ids: List[Int] = []) -> Telemetry {
  {
    temperature: cpu_temperature(sources, logical_cpu_ids),
    frequency: frequency(sources, .First),
    battery: battery(sources),
  }
}

## Converts a Celsius value using the pinned display scale and rounded degree.
export pure temperature_display(millidegrees: Int, scale: Str) -> TemperatureDisplay? {
  let celsius = millidegrees.float() / 1000.0
  match scale {
    "celsius" => {value: celsius.round() ?? 0, unit: "°C"}
    "fahrenheit" => {value: (celsius * 1.8 + 32.0).round() ?? 0, unit: "°F"}
    "kelvin" => {value: (celsius + 273.15).round() ?? 0, unit: "K"}
    "rankine" => {value: (celsius * 1.8 + 491.67).round() ?? 0, unit: "°R"}
    else => null
  }
}
