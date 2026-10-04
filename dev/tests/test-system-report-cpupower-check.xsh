use system_report_cpupower_check as cpupower_reference

test test_system_report_cpupower_saved_output_scores_policy_and_idle_metadata {
  let frequency = cpupower_reference.parse_cpupower_frequency(
    """analyzing CPU 0:
  driver: amd-pstate-epp
""",
    """analyzing CPU 0:
624194 5756452
""",
  )?
  assert frequency.driver == "amd-pstate-epp"
  assert frequency.minimum_khz == 624194
  assert frequency.maximum_khz == 5756452
  let idle = cpupower_reference.parse_cpupower_idle("""CPUidle driver: acpi_idle
CPUidle governor: menu
analyzing CPU 0:
Number of idle states: 2
Available idle states: POLL C1
""")?
  let reference = cpupower_reference.CpupowerReference(
    driver: frequency.driver,
    hardware_min_khz: frequency.minimum_khz,
    hardware_max_khz: frequency.maximum_khz,
    idle_driver: idle.driver,
    idle_governor: idle.governor,
    idle_state_names: idle.names,
  )
  let candidate = """{"cpu":{"frequency_policies":[{"name":"policy7","related_cpus":[0,2],"driver":"amd-pstate-epp","hardware_min_khz":624194,"hardware_max_khz":5756452}],"global_idle_driver":"acpi_idle","global_idle_governor":"menu","idle_states":[{"cpu_id":0,"state_index":0,"name":"POLL"},{"cpu_id":0,"state_index":1,"name":"C1"}]}}"""
  let exact = cpupower_reference.compare_cpupower(candidate, reference)?
  assert exact.matched_fields == 6
  assert exact.mismatches.len() == 0 and exact.partial.len() == 0
  let wrong = candidate.replace("\"hardware_max_khz\":5756452", "\"hardware_max_khz\":5756451")
  let mismatch = cpupower_reference.compare_cpupower(wrong, reference)?
  assert "hardware_max_khz" in mismatch.mismatches
  let shifted = candidate.replace("\"state_index\":1", "\"state_index\":2")
  let shifted_comparison = cpupower_reference.compare_cpupower(shifted, reference)?
  assert "idle_state_names" in shifted_comparison.mismatches
}

test test_system_report_cpupower_rejects_ambiguous_or_malformed_utility_output {
  test.error_kind(
    cpupower_reference.parse_cpupower_frequency(
  """driver: x
driver: y
""",
  """1 2
""",
),
    "CpupowerCheckError.Invalid",
  )?
  test.error_kind(
    cpupower_reference.parse_cpupower_frequency(
  """driver: x
""",
  """2 1
""",
),
    "CpupowerCheckError.Invalid",
  )?
  test.error_kind(
    cpupower_reference.parse_cpupower_idle("""CPUidle driver: x
CPUidle governor: y
Number of idle states: 2
Available idle states: C1
"""),
    "CpupowerCheckError.Invalid",
  )?
}

test test_system_report_cpupower_live_reference_runs_only_selected_forms {
  let tools_root = fs.tempdir()?
  defer tools_root.close()?
  tools_root.write(
    p"cpupower",
    """#!/bin/sh
case "$*" in
  "--version") printf 'cpupower 7.1.5-0\n' ;;
  "-c 0 frequency-info --driver") printf 'analyzing CPU 0:\n  driver: fixture-driver\n' ;;
  "-c 0 frequency-info --hwlimits") printf 'analyzing CPU 0:\n100 200\n' ;;
  "-c 0 idle-info") printf 'CPUidle driver: fixture-idle\nCPUidle governor: fixture-governor\nNumber of idle states: 1\nAvailable idle states: C1\n' ;;
  *) exit 4 ;;
esac
""",
  )?
  tools_root.write(
    p"xsh",
    """#!/bin/sh
printf '{"source_mode":"live_linux","cpu":{"frequency_policies":[{"name":"policy7","related_cpus":[0,2],"driver":"fixture-driver","hardware_min_khz":100,"hardware_max_khz":200}],"global_idle_driver":"fixture-idle","global_idle_governor":"fixture-governor","idle_states":[{"cpu_id":0,"state_index":0,"name":"C1"}]}}\n'
""",
  )?
  tools_root.chmod(p"cpupower", 0o700)?
  tools_root.chmod(p"xsh", 0o700)?
  let root_path = tools_root.host_path()?
  let result = cpupower_reference.compare_live_cpupower(
    fp"{root_path}/xsh".display(),
    fp"{root_path}/script".display(),
    fp"{root_path}/cpupower".display(),
  )?
  assert result.comparison.matched_fields == 6
  assert result.comparison.mismatches.len() == 0 and result.comparison.partial.len() == 0
  assert result.version == "cpupower 7.1.5-0"
}
