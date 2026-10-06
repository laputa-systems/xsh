use core.lib.hardware
use core.lib.system_report as report

pure temperature() -> report.SensorChannel {
  {chip: "example", chip_entry_name: "hwmon0", channel: "temp1", label: {state: .Observed, value: "CPU", raw_bytes_base64: null},
    kind: "temperature", value: 42500, unit: "millidegrees_celsius", minimum: null,
    maximum: 80000, critical: 100000, alarm: false, parent_device_class_index: null,
    parent_pci_function_index: null, parent_usb_device_index: null}
}

test test_sensors_units_and_fahrenheit_are_rendered_from_typed_values {
  let channel = temperature()
  let lines = hardware.sensor_lines([channel], false, false)
  assert lines[0] == "example-hwmon0"
  assert "CPU: +42.5°C" in lines[2]
  assert "high = +80.0°C" in lines[2]
  let fahrenheit = hardware.sensor_lines([channel], true, false)
  assert "CPU: +108.5°F" in fahrenheit[2]
  assert "temp1_input: 42.5" in hardware.sensor_lines([channel], false, true)[3]
}

test test_sensors_json_escapes_labels_and_preserves_measurements {
  let channel = {...temperature(), label: {state: report.Observed, value: "CPU \"A\"", raw_bytes_base64: null}}
  let encoded = hardware.sensor_json([channel])?
  let parsed = json.decode(encoded)?
  assert json.get(parsed, ["example-hwmon0", "CPU \"A\"", "temp1_input"])?.require(Float)? == 42.5
}

test test_sensors_chip_identity_does_not_merge_identical_driver_names {
  let first = temperature()
  let second = {...temperature(), chip_entry_name: "hwmon1", value: 55250}
  let encoded = json.decode(hardware.sensor_json([first, second])?)?
  assert json.get(encoded, ["example-hwmon0", "CPU", "temp1_input"])?.require(Float)? == 42.5
  assert json.get(encoded, ["example-hwmon1", "CPU", "temp1_input"])?.require(Float)? == 55.25
  assert hardware.chip_matches("example-hwmon0", "example-*")?
  assert hardware.chip_matches("a.b", "a.b")?
  assert ! hardware.chip_matches("axb", "a.b")?
  assert hardware.chip_matches("anything", "[") is Err(_)
}

test test_sensors_voltage_preserves_millivolt_precision {
  let voltage = {...temperature(), channel: "in1", label: {state: report.Observed, value: "Supply", raw_bytes_base64: null}, kind: "voltage", value: 1025, unit: "millivolts", maximum: null, critical: null}
  assert hardware.sensor_lines([voltage], false, false)[2] == "Supply: 1.025 V"
}

test test_sensors_rejects_configuration_and_mutation_options { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{ctx.core_dir}/sensors.xsh"
  for option in ["-c", "-s"] {
    let err = fp"{root}/err"
    let command = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), option], root, {}, b"", fp"{root}/out", err, timeout: 5s)
    assert process.run(command)?.exited_with(1)
    assert "not supported" in err.read_text()?
  }
}


test test_sensors_no_adapter_keeps_measurement_labels_with_adapter_prefix {
  let channel = {...temperature(), label: {state: report.Observed, value: "Adapter: CPU", raw_bytes_base64: null}}
  let lines = hardware.sensor_lines([channel], false, false, true)
  assert lines[1].starts_with("Adapter: CPU: +42.5°C")
  assert "Adapter: Unknown adapter" not in lines
}

test test_sensors_collects_a_rooted_hwmon_fixture {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"sys/class/hwmon/hwmon0", parents: true)
  root.write(p"sys/class/hwmon/hwmon0/name", "example\n")
  root.write(p"sys/class/hwmon/hwmon0/temp1_input", "42500\n")
  root.write(p"sys/class/hwmon/hwmon0/temp1_label", "CPU\n")
  root.write(p"sys/class/hwmon/hwmon0/temp1_max", "80000\n")
  root.write(p"sys/class/hwmon/hwmon0/temp1_crit", "100000\n")
  let section = hardware.sensors_from_root(root)?
  assert section.status.enumeration_succeeded
  assert section.channels.len() == 1
  assert section.channels[0].value == 42500
  assert section.channels[0].unit == "millidegrees_celsius"
  assert hardware.sensor_lines(section.channels, false, false)[2] == "CPU: +42.5°C  (high = +80.0°C, crit = +100.0°C)"
}
