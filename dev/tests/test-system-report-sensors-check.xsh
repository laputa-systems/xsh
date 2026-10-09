use system_report_sensors_check as sensors_reference

test test_system_report_sensors_json_reference_uses_raw_subfeature_names {
  let output = """{"coretemp-isa-0000":{"Adapter":"ISA adapter","Core 0":{"temp2_input":41.125,"temp2_max":100.0}},"nvme-pci-0100":{"Adapter":"PCI adapter","Composite":{"temp1_input":36.85,"temp1_alarm":0.0}},"BAT1-isa-00ba":{"Adapter":"ISA adapter","curr1":{"curr1_input":0.0}}}"""
  let parsed = sensors_reference.parse_sensors_json(output)?
  assert parsed.len() == 5
  assert parsed[0].chip == "BAT1"
  assert parsed[0].subfeature == "curr1_input"
  assert parsed[1].chip == "coretemp"
  assert parsed[1].subfeature == "temp2_input"
  assert parsed[3].chip == "nvme"
  assert parsed[3].subfeature == "temp1_alarm"
  test.error_kind(sensors_reference.parse_sensors_json("{bad json"), "json")
}

test test_system_report_sensors_json_corrob_keeps_ambiguous_and_changing_readings_partial {
  let first = sensors_reference.parse_sensors_json(
    """{"coretemp-isa-0000":{"Adapter":"ISA adapter","Core 0":{"temp2_input":41.125}},"nvme-pci-0100":{"Adapter":"PCI adapter","Composite":{"temp1_input":36.85}}}""",
  )?
  let candidate = """{"sensors":{"channels":[{"chip_entry_name":"hwmon0","chip":"coretemp","channel":"temp2","value":41125},{"chip_entry_name":"hwmon1","chip":"nvme","channel":"temp1","value":36850}]}}"""
  let exact = sensors_reference.compare_sensors_json(candidate, first, first)?
  assert exact.compared == 2
  assert exact.mismatches == []
  assert exact.partial == []
  let wrong = sensors_reference.compare_sensors_json(candidate.replace("41125", "42000"), first, first)?
  assert wrong.mismatches == []
  assert wrong.partial == ["coretemp-isa-0000:temp2_input"]
  assert wrong.compared == 1
  let changed = sensors_reference.compare_sensors_json(
    candidate,
    first,
    sensors_reference.parse_sensors_json(
  """{"coretemp-isa-0000":{"Core 0":{"temp2_input":42.0}},"nvme-pci-0100":{"Composite":{"temp1_input":36.85}}}""",
)?,
  )?
  assert changed.compared == 1
  assert changed.partial == ["coretemp-isa-0000:temp2_input"]
  let ambiguous = sensors_reference.compare_sensors_json(
    candidate.replace("\"chip\":\"nvme\"", "\"chip\":\"coretemp\"")
      .replace("\"channel\":\"temp1\"", "\"channel\":\"temp2\""),
    first,
    first,
  )?
  assert ambiguous.partial == ["coretemp-isa-0000:temp2_input", "nvme-pci-0100:temp1_input"]
  let duplicate_chips = sensors_reference.parse_sensors_json(
    """{"coretemp-isa-0000":{"Core 0":{"temp2_input":41.125}},"coretemp-isa-0001":{"Core 0":{"temp2_input":42.0}}}""",
  )?
  let duplicate_match = sensors_reference.compare_sensors_json(candidate, duplicate_chips, duplicate_chips)?
  assert duplicate_match.compared == 0
  assert duplicate_match.partial == ["coretemp-isa-0000:temp2_input", "coretemp-isa-0001:temp2_input"]
  let unknown = sensors_reference.parse_sensors_json(
    """{"coretemp-isa-0000":{"Unknown":{"tempfoo_input":41.0,"temp2_input":41.125}}}""",
  )?
  let known_only = sensors_reference.compare_sensors_json(candidate, unknown, unknown)?
  assert known_only.reference_count == 1
  assert known_only.compared == 1
}

test test_system_report_sensors_json_live_reference_runs_only_explicit_tools {
  let tools_root = fs.tempdir()?
  defer tools_root.close()?
  tools_root.write(
    p"sensors",
    """#!/bin/sh
if [ "$1" = "-v" ]; then
  printf 'sensors version 3.6.2\n'
  exit 0
fi
if [ "$1" != "-j" ] || [ "$2" != "-c" ] || [ "$3" != "/dev/null" ]; then
  exit 3
fi
printf '{"coretemp-isa-0000":{"Core 0":{"temp2_input":41.125}}}\n'
""",
  )
  tools_root.write(
    p"xsh",
    """#!/bin/sh
printf '{"source_mode":"live_linux","sensors":{"channels":[{"chip_entry_name":"hwmon0","chip":"coretemp","channel":"temp2","value":41125}]}}\n'
""",
  )
  tools_root.chmod(p"sensors", 0o700)
  tools_root.chmod(p"xsh", 0o700)
  let root_path = tools_root.host_path()?
  let result = sensors_reference.compare_live_sensors_json(
    fp"{root_path}/xsh".display(),
    fp"{root_path}/script".display(),
    fp"{root_path}/sensors".display(),
  )?
  assert result.comparison.compared == 1
  assert result.comparison.mismatches == []
  assert result.version == "sensors version 3.6.2"
  assert result.before_sha256_hex == result.after_sha256_hex
}

test test_system_report_sensors_json_cli_dispatch_requires_explicit_utility { |ctx|
  let tools_root = fs.tempdir()?
  defer tools_root.close()?
  tools_root.write(
    p"sensors",
    """#!/bin/sh
if [ "$1" = "-v" ]; then
  printf 'sensors version 3.6.2\n'
else
  printf '{"coretemp-isa-0000":{"Core 0":{"temp2_input":41.125}}}\n'
fi
""",
  )
  tools_root.write(
    p"xsh",
    """#!/bin/sh
printf '{"source_mode":"live_linux","sensors":{"channels":[{"chip_entry_name":"hwmon0","chip":"coretemp","channel":"temp2","value":41125}]}}\n'
""",
  )
  tools_root.chmod(p"sensors", 0o700)
  tools_root.chmod(p"xsh", 0o700)
  let root_path = tools_root.host_path()?
  let sensors_path = fp"{root_path}/sensors"
  let xsh_path = fp"{root_path}/xsh"
  let output = test.temp_path(ctx, name: "system-report-sensors-json.stdout")
  let stderr = test.temp_path(ctx, name: "system-report-sensors-json.stderr")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir.parent()}/dev/main.xsh" system-report-check \
    --compare-sensors-json --sensors-bin $sensors_path --xsh-bin $xsh_path --script $xsh_path > $output 2> $stderr
  let exited_successfully = status.exited_with(0)
  let diagnostic = stderr.read_text()?
  assert exited_successfully, diagnostic
  assert "sensors.lm-sensors: reference=1, compared=1, mismatched=0" in output.read_text()?
  assert "adapter=sensors-json-v1" in output.read_text()?
}
