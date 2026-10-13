use core.lib.btop_sensors as sensors
use core.lib.btop_model as model
use core.lib.btop_render as renderer
use core.lib.btop_source as btop_source
use core.lib.system_report as report
use core.lib.system_report_live as live

type TestChannel = {id: Int, label: Str, value: Str}

proc write_hwmon(root: FsRoot, directory: Str, chip: Str, channels: List[TestChannel]) [fs, error] {
  root.mkdir(fp"sys/class/hwmon/{directory}", parents: true)
  root.write(fp"sys/class/hwmon/{directory}/name", f"{chip}\n")
  for channel in channels {
    root.write(fp"sys/class/hwmon/{directory}/temp{channel.id}_label", f"{channel.label}\n")
    root.write(fp"sys/class/hwmon/{directory}/temp{channel.id}_input", f"{channel.value}\n")
  }
}

proc sources(root: FsRoot) [fs, error] -> live.MonitorSources {
  live.collect_monitor_sources(root)
}

test test_btop_sensor_selects_signed_package_and_deduplicates_core_channels {
  let root = fs.tempdir()?
  defer root.close()
  write_hwmon(
    root,
    "hwmon0",
    "coretemp",
    [
      {id: 1, label: "Package id 0", value: "-5500"},
      {id: 2, label: "Core 0", value: "42000"},
      {id: 3, label: "Core 1", value: "43000"},
    ],
  )
  write_hwmon(root, "hwmon1", "coretemp", [{id: 1, label: "Core 0", value: "99000"}])
  root.mkdir(p"proc", parents: true)
  root.write(p"proc/cpuinfo", "processor : 0\ncore id : 0\nprocessor : 1\ncore id : 1\n")

  let captured = sources(root)
  let temperature = sensors.select(captured, [0, 1]).temperature
  assert temperature.state == .Observed
  assert temperature.package_millidegrees == -5500
  assert temperature.cores == [{id: 0, millidegrees: 42000}, {id: 1, millidegrees: 43000}]
}

test test_btop_cpu_core_map_uses_physical_ids_for_sparse_logical_cpu_numbers {
  let root = fs.tempdir()?
  defer root.close()
  write_hwmon(
    root,
    "hwmon0",
    "coretemp",
    [
      {id: 1, label: "Package id 0", value: "60000"},
      {id: 2, label: "Core 0", value: "41000"},
    ],
  )
  write_hwmon(root, "hwmon1", "coretemp", [{id: 1, label: "Core 1", value: "47000"}])
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/cpuinfo",
    "processor : 0\ncore id : 0\n\nprocessor : 1\ncore id : 0\n\nprocessor : 4\ncore id : 1\n\nprocessor : 9\ncore id : 1\n",
  )

  let temperature = sensors.select(sources(root), [0, 1, 4, 9]).temperature
  assert temperature.cores == [
    {id: 0, millidegrees: 41000},
    {id: 1, millidegrees: 41000},
    {id: 4, millidegrees: 47000},
    {id: 9, millidegrees: 47000},
  ]
}

test test_btop_cpu_core_map_bounds_large_physical_ids_without_overflow {
  let root = fs.tempdir()?
  defer root.close()
  write_hwmon(
    root,
    "hwmon0",
    "coretemp",
    [
      {id: 1, label: "Core 0", value: "41000"},
      {id: 2, label: "Core 1", value: "47000"},
    ],
  )
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/cpuinfo",
    "processor : 0\ncore id : 0\n\nprocessor : 1\ncore id : 9223372036854775807\n",
  )

  let temperature = sensors.select(sources(root), [0, 1]).temperature
  assert temperature.cores == [
    {id: 0, millidegrees: 41000},
    {id: 1, millidegrees: 47000},
  ]
}

test test_btop_sensor_keeps_zero_and_withholds_malformed_or_missing_readings {
  let root = fs.tempdir()?
  defer root.close()
  write_hwmon(
    root,
    "hwmon0",
    "coretemp",
    [
      {id: 1, label: "Package id 0", value: "0"},
      {id: 2, label: "Core 0", value: "broken"},
    ],
  )

  let zero = sensors.select(sources(root)).temperature
  assert zero.state == .Observed
  assert zero.package_millidegrees == 0
  assert zero.cores == []

  root.write(p"sys/class/hwmon/hwmon0/temp1_input", "broken\n")
  let malformed = sensors.select(sources(root)).temperature
  assert malformed.state == .Malformed
  assert malformed.package_millidegrees == null
}

test test_btop_sensor_uses_thermal_zone_when_hwmon_has_no_cpu_channel {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"sys/class/thermal/thermal_zone2", parents: true)
  root.write(p"sys/class/thermal/thermal_zone2/type", "cpu-thermal\n")
  root.write(p"sys/class/thermal/thermal_zone2/temp", "51500\n")

  let temperature = sensors.select(sources(root)).temperature
  assert temperature.state == .Observed
  assert temperature.package_millidegrees == 51500
}

test test_btop_temperature_display_matches_all_pinned_scales_for_signed_input {
  assert sensors.temperature_display(-5500, "celsius") == {value: -6, unit: "°C"}
  assert sensors.temperature_display(-5500, "fahrenheit") == {value: 22, unit: "°F"}
  assert sensors.temperature_display(-5500, "kelvin") == {value: 268, unit: "K"}
  assert sensors.temperature_display(-5500, "rankine") == {value: 482, unit: "°R"}
  assert sensors.temperature_display(1, "unknown") == null
}

test test_btop_frequency_modes_reduce_policy_values_and_cpuinfo_fallback {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"sys/devices/system/cpu/cpufreq/policy0", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpufreq/policy1", parents: true)
  root.write(p"sys/devices/system/cpu/cpufreq/policy0/related_cpus", "0\n")
  root.write(p"sys/devices/system/cpu/cpufreq/policy0/affected_cpus", "0\n")
  root.write(p"sys/devices/system/cpu/cpufreq/policy0/scaling_cur_freq", "1800000\n")
  root.write(p"sys/devices/system/cpu/cpufreq/policy1/related_cpus", "1\n")
  root.write(p"sys/devices/system/cpu/cpufreq/policy1/affected_cpus", "1\n")
  root.write(p"sys/devices/system/cpu/cpufreq/policy1/scaling_cur_freq", "3200000\n")

  let collected = sources(root)
  assert sensors.frequency(collected, sensors.First).value_mhz == 1800.0
  let range = sensors.frequency(collected, sensors.Range)
  assert range.minimum_mhz == 1800.0 and range.maximum_mhz == 3200.0
  assert sensors.frequency(collected, sensors.Lowest).value_mhz == 1800.0
  assert sensors.frequency(collected, sensors.Highest).value_mhz == 3200.0
  assert sensors.frequency(collected, sensors.Average).value_mhz == 2500.0

  root.write(p"sys/devices/system/cpu/cpufreq/policy0/scaling_cur_freq", "invalid\n")
  root.write(p"sys/devices/system/cpu/cpufreq/policy1/scaling_cur_freq", "0\n")
  root.mkdir(p"proc", parents: true)
  root.write(p"proc/cpuinfo", "processor : 0\ncpu MHz : 2400.000\n\n")
  let fallback = sensors.select(sources(root)).frequency
  assert fallback.state == .Observed and fallback.value_mhz == 2400.0
  root.write(p"proc/cpuinfo", "processor : 0\ncpu MHz : invalid\n\n")
  let malformed = sensors.select(sources(root)).frequency
  assert malformed.state == .Malformed and malformed.value_mhz == null
}

test test_btop_battery_selects_present_battery_and_derives_level_power_and_time {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"sys/class/power_supply/BAT0", parents: true)
  root.write(p"sys/class/power_supply/BAT0/type", "Battery\n")
  root.write(p"sys/class/power_supply/BAT0/present", "1\n")
  root.write(p"sys/class/power_supply/BAT0/status", "Discharging\n")
  root.write(p"sys/class/power_supply/BAT0/energy_now", "4500000\n")
  root.write(p"sys/class/power_supply/BAT0/energy_full", "10000000\n")
  root.write(p"sys/class/power_supply/BAT0/power_now", "4500000\n")
  root.mkdir(p"sys/class/power_supply/UPS0", parents: true)
  root.write(p"sys/class/power_supply/UPS0/type", "UPS\n")
  root.write(p"sys/class/power_supply/UPS0/present", "1\n")
  root.write(p"sys/class/power_supply/UPS0/capacity", "99\n")

  let battery = sensors.select(sources(root)).battery
  assert battery.state == .Observed
  assert battery.name == "BAT0"
  assert battery.status == "discharging"
  assert battery.capacity_percent == 45
  assert battery.power_watts == 4.5
  assert battery.minutes_remaining == 60
}

test test_btop_battery_absent_and_unreadable_states_do_not_invent_a_level {
  let root = fs.tempdir()?
  defer root.close()
  let absent = sensors.select(sources(root)).battery
  assert absent.state == .Absent and absent.capacity_percent == null

  root.mkdir(p"sys/class/power_supply/BAT0", parents: true)
  root.write(p"sys/class/power_supply/BAT0/type", "Battery\n")
  root.write(p"sys/class/power_supply/BAT0/present", "1\n")
  root.write(p"sys/class/power_supply/BAT0/status", "Charging\n")
  root.write(p"sys/class/power_supply/BAT0/energy_now", "invalid\n")
  root.write(p"sys/class/power_supply/BAT0/energy_full", "0\n")
  let invalid = sensors.select(sources(root)).battery
  assert invalid.state == .Malformed and invalid.capacity_percent == null
}

test test_btop_battery_uses_ac_online_and_charge_current_fallbacks {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"sys/class/power_supply/BAT0/AC0", parents: true)
  root.write(p"sys/class/power_supply/BAT0/type", "Battery\n")
  root.write(p"sys/class/power_supply/BAT0/present", "1\n")
  root.write(p"sys/class/power_supply/BAT0/status", "Unknown\n")
  root.write(p"sys/class/power_supply/BAT0/capacity", "80\n")
  root.write(p"sys/class/power_supply/BAT0/charge_now", "2000000\n")
  root.write(p"sys/class/power_supply/BAT0/charge_full", "4000000\n")
  root.write(p"sys/class/power_supply/BAT0/current_now", "1000000\n")
  root.write(p"sys/class/power_supply/BAT0/voltage_now", "12000000\n")
  root.write(p"sys/class/power_supply/BAT0/AC0/online", "1\n")

  let battery = sensors.select(sources(root)).battery
  assert battery.status == "charging"
  assert battery.capacity_percent == 80
  assert battery.power_watts == 12.0
  assert battery.minutes_remaining == 120
}

test test_btop_capture_carries_sensor_values_into_the_visible_cpu_panel {
  let root = fs.tempdir()?
  defer root.close()
  write_hwmon(root, "hwmon0", "coretemp", [
    {id: 1, label: "Package id 0", value: "42500"},
    {id: 2, label: "Core 0", value: "40000"},
  ])
  root.mkdir(p"sys/devices/system/cpu/cpufreq/policy0", parents: true)
  root.write(p"sys/devices/system/cpu/cpufreq/policy0/related_cpus", "0\n")
  root.write(p"sys/devices/system/cpu/cpufreq/policy0/affected_cpus", "0\n")
  root.write(p"sys/devices/system/cpu/cpufreq/policy0/scaling_cur_freq", "3200000\n")
  root.mkdir(p"sys/class/power_supply/BAT0", parents: true)
  root.write(p"sys/class/power_supply/BAT0/type", "Battery\n")
  root.write(p"sys/class/power_supply/BAT0/present", "1\n")
  root.write(p"sys/class/power_supply/BAT0/status", "Charging\n")
  root.write(p"sys/class/power_supply/BAT0/capacity", "55\n")

  let observation = btop_source.capture(root, "sensors", 1000000000, 4096, 100)?
  let value = model.step(observation, null)
  let screen = renderer.render(value, 80, 24)?
  assert value.telemetry.temperature.package_millidegrees == 42500
  assert value.telemetry.frequency.value_mhz == 3200.0
  assert value.telemetry.battery.capacity_percent == 55
  assert "Temperature 43°C" in screen.lines[5]
  assert "Cores C0 40°C" in screen.lines[6]
  assert "Frequency 3.2 GHz" in screen.lines[5]
  assert "Battery BAT0 55% charging" in screen.lines[6]
}

test test_btop_capture_reuses_gauges_until_the_collection_schedule_is_due {
  let root = fs.tempdir()?
  defer root.close()
  write_hwmon(root, "hwmon0", "coretemp", [{id: 1, label: "Package id 0", value: "41000"}])
  let first = btop_source.capture(root, "gauges", 1000000000, 4096, 100)?
  root.write(p"sys/class/hwmon/hwmon0/temp1_input", "55000\n")

  let cached = btop_source.capture(root, "gauges", 2000000000, 4096, 100, first.telemetry)?
  let refreshed = btop_source.capture(root, "gauges", 6000000000, 4096, 100)?
  assert sensors.select(cached.telemetry).temperature.package_millidegrees == 41000
  assert sensors.select(refreshed.telemetry).temperature.package_millidegrees == 55000
}
