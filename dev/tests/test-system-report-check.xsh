use system_report_check as report_checks
use system_report_smbios_check as smbios_reference

type ReportCoverageAssertion = {
  id: Str,
  domain: Str,
  field: Str,
  relation: Str,
  tier: Str,
  source_abi: Str,
  reference_adapter: Str,
  reference_commands: List[List[Str]],
  eligibility: Str,
  equality_rule: Str,
  fixture_scenarios: List[Str],
}

type ReportFixtureCase = {scenario: Str, tests: List[Str]}

type KernelParameterReferenceFixture = {name: Str, source: Str, state: Str, value: Str?, raw_bytes_base64: Str?}

type HugePageReferenceFixture = {
  node_id: Int?,
  page_size_bytes: Int,
  total: Int,
  free: Int?,
  reserved: Int?,
  surplus: Int?,
}

type PsiReferenceFixture = {resource: Str, kind: Str, avg10: Str, avg60: Str, avg300: Str, total_us: Int}

type ReportCoverageManifest = {
  schema_version: Int,
  producer: Str,
  assertions: List[ReportCoverageAssertion],
  fixture_cases: List[ReportFixtureCase],
  macos_fixture_cases: List[ReportFixtureCase],
  fixture_scenarios: List[Str],
}

test test_system_report_fixture_failure_shows_cargo_diagnostic {
  assert report_checks.fixture_failure_summary(
    "",
    """error[E0433]: missing crate
error: could not compile xsh test
""",
  ) == "error: could not compile xsh test"
  assert report_checks.fixture_failure_summary(
    """running 1 test
test result: FAILED. 0 passed; 1 failed
""",
    """error: test failed
""",
  ) == "test result: FAILED. 0 passed; 1 failed"
  assert report_checks.fixture_failure_summary("", "") == "no test output"
}

pure assertion(id: Str, tier: Str) -> ReportCoverageAssertion {
  ReportCoverageAssertion(
    id:,
    domain: "cpu",
    field: "frequency_policy.related_cpus",
    relation: "membership",
    tier:,
    source_abi: "/sys/devices/system/cpu/cpufreq/policy*/related_cpus",
    reference_adapter: "lscpu-json",
    reference_commands: [
      [
        "lscpu",
        "--json",
      ],
    ],
    eligibility: "CPUFreq policy exists and is readable",
    equality_rule: "exact CPU ID set",
    fixture_scenarios: [
      "offline_related_cpu",
    ],
  )
}

pure manifest(assertions: List[ReportCoverageAssertion]) -> ReportCoverageManifest {
  ReportCoverageManifest(
    schema_version: 4,
    producer: "system-report",
    assertions:,
    fixture_cases: [],
    macos_fixture_cases: [],
    fixture_scenarios: [
      "offline_related_cpu",
    ],
  )
}

test test_system_report_cpufreq_reference_scores_policy_membership_and_bounds {
  let policy = {
    name: "policy3",
    related_cpus: [
      0,
      2,
    ],
    affected_cpus: [
      0,
    ],
    driver: "fixture-driver",
    governor: "powersave",
    hardware_min_khz: 800000,
    hardware_max_khz: 4000000,
    scaling_min_khz: 1000000,
    scaling_max_khz: 3000000,
    hardware_current: {
      value: 1800000,
      complete: true,
    },
    scaling_current: {
      value: 1800000,
      complete: true,
    },
    average_current: {
      value: null,
      complete: true,
    },
    governor_requested: {
      value: null,
      complete: true,
    },
    energy_performance_preference: "balance_performance",
    available_energy_performance_preferences: [
      "performance",
      "balance_performance",
    ],
    boost_supported: true,
    boost_allowed: true,
    boost_active: null,
    boost_scope: "system",
  }
  let candidate = """{"cpu":{"status":{"state":"complete","enumeration_succeeded":true},"frequency_policies":[{"name":"policy3","related_cpus":[0,2],"affected_cpus":[0],"driver":"fixture-driver","governor":"powersave","hardware_min_khz":800000,"hardware_max_khz":4000000,"scaling_min_khz":1000000,"scaling_max_khz":3000000,"hardware_current_khz":1800000,"scaling_current_khz":1800000,"average_current_khz":null,"governor_requested_khz":null,"energy_performance_preference":"balance_performance","available_energy_performance_preferences":["performance","balance_performance"],"boost_supported":true,"boost_allowed":true,"boost_active":null,"boost_scope":"system"}]}}"""
  let exact = report_checks.compare_cpufreq_policies(candidate, [policy], [policy])?
  assert exact.exact_policies
  assert exact.exact_bounds
  assert exact.eligible_controls
  assert exact.exact_controls
  let wrong_membership = report_checks.compare_cpufreq_policies(candidate.replace("[0,2]", "[0]"), [policy], [policy])?
  assert wrong_membership.policy_mismatches == ["policy3.related_cpus"]
  assert ! wrong_membership.exact_policies
  let wrong_bound = report_checks.compare_cpufreq_policies(
    candidate.replace("\"scaling_max_khz\":3000000", "\"scaling_max_khz\":null"),
    [policy],
    [policy],
  )?
  assert wrong_bound.bound_mismatches == ["policy3.scaling_max_khz"]
  assert ! wrong_bound.exact_bounds
  let unstable = report_checks.compare_cpufreq_policies(candidate, [policy], [{...policy, scaling_min_khz: 1100000}])?
  assert unstable.unstable_bounds == ["policy3.scaling_min_khz"]
  assert ! unstable.exact_bounds
  let missing_gauge = report_checks.compare_cpufreq_policies(
    candidate.replace("\"hardware_current_khz\":1800000", "\"hardware_current_khz\":null"),
    [policy],
    [policy],
  )?
  assert missing_gauge.gauge_mismatches == ["policy3.hardware_current_khz"]
  assert ! missing_gauge.exact_bounds
  let would_block_candidate = candidate.replace("\"hardware_current_khz\":1800000", "\"hardware_current_khz\":null")
    .replace("]}}", "]},\"issues\":[{\"section\":\"cpu\",\"field\":\"policy3.cpuinfo_cur_freq\",\"errno\":11}]}")
  let would_block_during_collection = report_checks.compare_cpufreq_policies(would_block_candidate, [policy], [policy])?
  assert would_block_during_collection.unstable_gauges == ["policy3.hardware_current_khz"]
  assert would_block_during_collection.gauge_mismatches == []
  let denied_candidate = would_block_candidate.replace("\"errno\":11", "\"errno\":13")
  let denied_during_collection = report_checks.compare_cpufreq_policies(denied_candidate, [policy], [policy])?
  assert denied_during_collection.unstable_gauges == ["policy3.hardware_current_khz"]
  let changed_gauge = report_checks.compare_cpufreq_policies(
    candidate,
    [policy],
    [{...policy, hardware_current: {value: 1900000, complete: true}}],
  )?
  assert changed_gauge.unstable_gauges == ["policy3.hardware_current_khz"]
  assert ! changed_gauge.exact_bounds
  let intervening_gauge = report_checks.compare_cpufreq_policies(
    candidate.replace("\"hardware_current_khz\":1800000", "\"hardware_current_khz\":1900000"),
    [policy],
    [policy],
  )?
  assert intervening_gauge.unstable_gauges == ["policy3.hardware_current_khz"]
  let would_block = report_checks.compare_cpufreq_policies(
    candidate,
    [{...policy, hardware_current: {value: null, complete: false}}],
    [policy],
  )?
  assert would_block.unstable_gauges == ["policy3.hardware_current_khz"]
  let absent = report_checks.compare_cpufreq_policies(
    candidate.replace("\"average_current_khz\":null", "\"average_current_khz\":1700000"),
    [policy],
    [policy],
  )?
  assert absent.unstable_gauges == ["policy3.average_current_khz"]
  let userspace_policy = {...policy, governor: "userspace", governor_requested: {value: 1900000, complete: true}}
  let userspace_candidate = candidate.replace("\"governor\":\"powersave\"", "\"governor\":\"userspace\"")
    .replace("\"governor_requested_khz\":null", "\"governor_requested_khz\":1900000")
  let userspace = report_checks.compare_cpufreq_policies(userspace_candidate, [userspace_policy], [userspace_policy])?
  assert userspace.exact_bounds
  let missing_request = report_checks.compare_cpufreq_policies(
    candidate.replace("\"governor\":\"powersave\"", "\"governor\":\"userspace\""),
    [userspace_policy],
    [userspace_policy],
  )?
  assert missing_request.gauge_mismatches == ["policy3.governor_requested_khz"]
  let wrong_epp = report_checks.compare_cpufreq_policies(
    candidate.replace(
      "\"energy_performance_preference\":\"balance_performance\"",
      "\"energy_performance_preference\":\"power\"",
    ),
    [policy],
    [policy],
  )?
  assert wrong_epp.control_mismatches == ["policy3.energy_performance_preference"]
  let wrong_boost = report_checks.compare_cpufreq_policies(
    candidate.replace("\"boost_allowed\":true", "\"boost_allowed\":false"),
    [policy],
    [policy],
  )?
  assert wrong_boost.control_mismatches == ["policy3.boost_allowed"]
  let inferred_boost = report_checks.compare_cpufreq_policies(
    candidate.replace("\"boost_active\":null", "\"boost_active\":true"),
    [policy],
    [policy],
  )?
  assert inferred_boost.control_mismatches == ["policy3.boost_active"]
  let wrong_choices = report_checks.compare_cpufreq_policies(
    candidate.replace("[\"performance\",\"balance_performance\"]", "[\"performance\"]"),
    [policy],
    [policy],
  )?
  assert wrong_choices.control_mismatches == ["policy3.available_energy_performance_preferences"]
  let changed_control = report_checks.compare_cpufreq_policies(candidate, [policy], [{...policy, boost_allowed: false}])?
  assert changed_control.unstable_controls == ["policy3.boost_allowed"]
  assert ! changed_control.exact_controls
  let unsupported = {
    ...policy,
    energy_performance_preference: null,
    available_energy_performance_preferences: [],
    boost_supported: null,
    boost_allowed: null,
    boost_scope: null,
  }
  let unsupported_candidate = candidate.replace(
    "\"energy_performance_preference\":\"balance_performance\"",
    "\"energy_performance_preference\":null",
  )
    .replace("[\"performance\",\"balance_performance\"]", "[]")
    .replace("\"boost_supported\":true", "\"boost_supported\":null")
    .replace("\"boost_allowed\":true", "\"boost_allowed\":null")
    .replace("\"boost_scope\":\"system\"", "\"boost_scope\":null")
  let unexposed = report_checks.compare_cpufreq_policies(unsupported_candidate, [unsupported], [unsupported])?
  assert ! unexposed.eligible_controls
  assert ! unexposed.exact_controls
  let no_bounds = {
    ...policy,
    hardware_min_khz: null,
    hardware_max_khz: null,
    scaling_min_khz: null,
    scaling_max_khz: null,
  }
  let no_bounds_candidate = candidate.replace("\"hardware_min_khz\":800000", "\"hardware_min_khz\":null")
    .replace("\"hardware_max_khz\":4000000", "\"hardware_max_khz\":null")
    .replace("\"scaling_min_khz\":1000000", "\"scaling_min_khz\":null")
    .replace("\"scaling_max_khz\":3000000", "\"scaling_max_khz\":null")
  let unbounded = report_checks.compare_cpufreq_policies(no_bounds_candidate, [no_bounds], [no_bounds])?
  assert ! unbounded.eligible_bounds
  assert ! unbounded.exact_bounds
  test.error_kind(report_checks.compare_cpufreq_policies(candidate, [policy], []), "SystemReportCheckError.Invalid")?
}

test test_system_report_cpufreq_boost_reference_preserves_scope_and_inverts_no_turbo {
  let generic = report_checks.parse_cpufreq_boost_reference("1", "1")?
  assert generic.supported == true
  assert generic.allowed == true
  assert generic.scope == "system"
  let intel = report_checks.parse_cpufreq_boost_reference(null, "1")?
  assert intel.allowed == false
  assert intel.scope == "intel_pstate"
  let absent = report_checks.parse_cpufreq_boost_reference(null, null)?
  assert absent.supported == null
  test.error_kind(report_checks.parse_cpufreq_boost_reference("invalid", null), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_cpufreq_boost_reference(null, "invalid"), "SystemReportCheckError.Invalid")?
}

test test_system_report_usb_topology_reference_scores_parent_links_and_stable_numbers {
  let root = {
    name: "usb1",
    parent_name: null,
    port_path: null,
    bus_number: 1,
    device_number: 1,
    speed_mbps: "480",
    is_root_hub: true,
  }
  let hub = {
    name: "1-2",
    parent_name: "usb1",
    port_path: "2",
    bus_number: 1,
    device_number: 2,
    speed_mbps: "480",
    is_root_hub: false,
  }
  let child = {
    name: "1-2.3",
    parent_name: "1-2",
    port_path: "2.3",
    bus_number: 1,
    device_number: 7,
    speed_mbps: "12",
    is_root_hub: false,
  }
  let candidate = """{"usb":{"status":{"state":"complete","enumeration_succeeded":true},"devices":[{"sysfs_name":"1-2.3","parent_device_index":1,"port_path":"2.3","bus_number":1,"device_number":7,"speed_mbps":"12","is_root_hub":false},{"sysfs_name":"1-2","parent_device_index":2,"port_path":"2","bus_number":1,"device_number":2,"speed_mbps":"480","is_root_hub":false},{"sysfs_name":"usb1","parent_device_index":null,"port_path":null,"bus_number":1,"device_number":1,"speed_mbps":"480","is_root_hub":true}]}}"""
  let exact = report_checks.compare_usb_topology(candidate, [root, hub, child], [root, hub, child])?
  assert exact.exact
  assert exact.matched_count == 3
  let wrong_parent = report_checks.compare_usb_topology(
    candidate.replace("\"parent_device_index\":1", "\"parent_device_index\":2"),
    [root, hub, child],
    [root, hub, child],
  )?
  assert wrong_parent.field_mismatches == ["1-2.3.parent_name"]
  let changed_number = report_checks.compare_usb_topology(
    candidate,
    [root, hub, child],
    [root, hub, {...child, device_number: 8}],
  )?
  assert changed_number.unstable_fields == ["1-2.3.device_number"]
  assert ! changed_number.exact
  let wrong_speed = report_checks.compare_usb_topology(
    candidate.replace("\"speed_mbps\":\"12\"", "\"speed_mbps\":\"480\""),
    [root, hub, child],
    [root, hub, child],
  )?
  assert wrong_speed.field_mismatches == ["1-2.3.speed_mbps"]
  let unreadable_speed_candidate = candidate.replace("\"speed_mbps\":\"12\"", "\"speed_mbps\":null")
    .replace("]}}", "]},\"issues\":[{\"section\":\"usb\",\"field\":\"devices.1-2.3.speed_mbps\"}]}")
  let unreadable_speed = report_checks.compare_usb_topology(
    unreadable_speed_candidate,
    [root, hub, child],
    [root, hub, child],
  )?
  assert unreadable_speed.unstable_fields == ["1-2.3.speed_mbps"]
  assert unreadable_speed.field_mismatches == []
  let arrived = report_checks.compare_usb_topology(candidate, [root, hub], [root, hub, child])?
  assert arrived.unstable_fields == ["1-2.3.presence"]
  assert arrived.unexpected_names == []
  test.error_kind(
    report_checks.compare_usb_topology(candidate, [root, hub, {...child, bus_number: 5}], [root, hub, child]),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.compare_usb_topology(
      candidate.replace("\"parent_device_index\":1", "\"parent_device_index\":9"),
      [root, hub, child],
      [root, hub, child],
    ),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_usb_topology_name_parser_keeps_root_hubs_and_sparse_ports {
  assert report_checks.parse_usb_topology_name("usb4")? == {
    parent_name: null,
    port_path: null,
    bus_number: 4,
    is_root_hub: true,
  }
  assert report_checks.parse_usb_topology_name("4-2")? == {
    parent_name: "usb4",
    port_path: "2",
    bus_number: 4,
    is_root_hub: false,
  }
  assert report_checks.parse_usb_topology_name("4-2.9")? == {
    parent_name: "4-2",
    port_path: "2.9",
    bus_number: 4,
    is_root_hub: false,
  }
  for invalid in [
    "usb",
    "usb0",
    "4-",
    "4-0",
    "4-2..3",
    "4-2:1.0",
    "bad",
    "999999999999999999999999-2",
  ] {
    test.error_kind(report_checks.parse_usb_topology_name(invalid), "SystemReportCheckError.Invalid")?
  }
}

test test_system_report_usb_topology_rooted_reference_reads_devices_without_interfaces {
  let root = fs.tempdir()?
  defer root.close()?
  for device in [
    {
      name: "usb4",
      bus: "4",
      number: "1",
      speed: "5000",
    },
    {
      name: "4-2",
      bus: "4",
      number: "2",
      speed: "5000",
    },
    {
      name: "4-2.9",
      bus: "4",
      number: "7",
      speed: "480",
    },
  ] {
    let device_path = fp"sys/bus/usb/devices/${device.name}"
    root.mkdir(device_path, parents: true)?
    root.write(
      fp"${device_path}/busnum",
      f"""${device.bus}
""",
    )?
    root.write(
      fp"${device_path}/devnum",
      f"""${device.number}
""",
    )?
    root.write(
      fp"${device_path}/speed",
      f"""${device.speed}
""",
    )?
  }

  root.mkdir(p"sys/bus/usb/devices/4-2:1.0")?
  let devices = report_checks.read_usb_topology_reference(root)?
  assert devices.len() == 3
  assert devices[0].name == "4-2"
  assert devices[1].parent_name == "4-2"
  assert devices[1].port_path == "2.9"
  assert devices[2].is_root_hub == true
  root.write(
    p"sys/bus/usb/devices/4-2/busnum",
    """5
""",
  )?
  test.error_kind(report_checks.read_usb_topology_reference(root), "SystemReportCheckError.Invalid")?
}

test test_system_report_usb_ids_reference_scores_raw_ids_and_observed_labels {
  let observed_vendor = {value: 1507, complete: true}
  let observed_product = {value: 1573, complete: true}
  let observed_class = {value: 9, complete: true}
  let observed_subclass = {value: 0, complete: true}
  let observed_protocol = {value: 3, complete: true}
  let reference = {
    name: "4-2",
    vendor_id: observed_vendor,
    product_id: observed_product,
    device_version: {
      value: "9406",
      complete: true,
    },
    class_code: observed_class,
    subclass: observed_subclass,
    protocol: observed_protocol,
    manufacturer: {
      value: "GenesysLogic",
      complete: true,
    },
    product: {
      value: "USB3.2 Hub",
      complete: true,
    },
  }
  let candidate = """{"usb":{"status":{"state":"complete","enumeration_succeeded":true},"devices":[{"sysfs_name":"4-2","vendor_id":1507,"product_id":1573,"device_version":"9406","class_code":9,"subclass":0,"protocol":3,"manufacturer":{"state":"observed","value":"GenesysLogic"},"product":{"state":"observed","value":"USB3.2 Hub"}}]}}"""
  let exact = report_checks.compare_usb_ids(candidate, [reference], [reference])?
  assert exact.exact
  let wrong_id = report_checks.compare_usb_ids(
    candidate.replace("\"product_id\":1573", "\"product_id\":1574"),
    [reference],
    [reference],
  )?
  assert wrong_id.field_mismatches == ["4-2.product_id"]
  let wrong_label = report_checks.compare_usb_ids(
    candidate.replace("USB3.2 Hub", "USB2.1 Hub"),
    [reference],
    [reference],
  )?
  assert wrong_label.field_mismatches == ["4-2.product"]
  let changed = report_checks.compare_usb_ids(
    candidate,
    [reference],
    [{...reference, product_id: {value: 1574, complete: true}}],
  )?
  assert changed.unstable_fields == ["4-2.product_id"]
  let unreadable = report_checks.compare_usb_ids(
    candidate,
    [{...reference, product_id: {value: null, complete: false}}],
    [reference],
  )?
  assert unreadable.unstable_fields == ["4-2.product_id"]
  let unreadable_label_candidate = candidate.replace(
    "\"manufacturer\":{\"state\":\"observed\",\"value\":\"GenesysLogic\"}",
    "\"manufacturer\":{\"state\":\"read_failure\",\"value\":null}",
  )
    .replace("]}}", "]},\"issues\":[{\"section\":\"usb\",\"field\":\"devices.4-2.manufacturer\"}]}")
  let unreadable_label = report_checks.compare_usb_ids(unreadable_label_candidate, [reference], [reference])?
  assert unreadable_label.unstable_fields == ["4-2.manufacturer"]
  assert unreadable_label.field_mismatches == []
  let sibling = {...reference, name: "4-3", product: {value: "Another Hub", complete: true}}
  let repeated_ids_candidate = candidate.replace(
    "]}}",
    ",{\"sysfs_name\":\"4-3\",\"vendor_id\":1507,\"product_id\":1573,\"device_version\":\"9406\",\"class_code\":9,\"subclass\":0,\"protocol\":3,\"manufacturer\":{\"state\":\"observed\",\"value\":\"GenesysLogic\"},\"product\":{\"state\":\"observed\",\"value\":\"Another Hub\"}}]}}",
  )
  assert report_checks.compare_usb_ids(repeated_ids_candidate, [reference, sibling], [reference, sibling])?.exact
  let mislabeled = report_checks.compare_usb_ids(
    repeated_ids_candidate.replace("Another Hub", "USB3.2 Hub"),
    [reference, sibling],
    [reference, sibling],
  )?
  assert mislabeled.field_mismatches == ["4-3.product"]
  test.error_kind(
    report_checks.compare_usb_ids(
      candidate.replace("\"sysfs_name\":\"4-2\"", "\"sysfs_name\":null"),
      [reference],
      [reference],
    ),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_usb_ids_rooted_reference_reads_fixed_width_values {
  let root = fs.tempdir()?
  defer root.close()?
  let device_path = p"sys/bus/usb/devices/4-2"
  root.mkdir(device_path, parents: true)?
  root.write(
    fp"${device_path}/idVendor",
    """05e3
""",
  )?
  root.write(
    fp"${device_path}/idProduct",
    """0625
""",
  )?
  root.write(
    fp"${device_path}/bcdDevice",
    """9406
""",
  )?
  root.write(
    fp"${device_path}/bDeviceClass",
    """09
""",
  )?
  root.write(
    fp"${device_path}/bDeviceSubClass",
    """00
""",
  )?
  root.write(
    fp"${device_path}/bDeviceProtocol",
    """03
""",
  )?
  root.write(
    fp"${device_path}/manufacturer",
    """GenesysLogic
""",
  )?
  root.write(
    fp"${device_path}/product",
    """USB3.2 Hub
""",
  )?
  let devices = report_checks.read_usb_ids_reference(root)?
  assert devices.len() == 1
  assert devices[0].vendor_id == {value: 1507, complete: true}
  assert devices[0].product_id == {value: 1573, complete: true}
  assert devices[0].device_version == {value: "9406", complete: true}
  assert devices[0].class_code == {value: 9, complete: true}
  assert devices[0].manufacturer == {value: "GenesysLogic", complete: true}
  root.write(
    fp"${device_path}/bDeviceClass",
    """9
""",
  )?
  test.error_kind(report_checks.read_usb_ids_reference(root), "SystemReportCheckError.Invalid")?
}

test test_system_report_usb_power_reference_scores_controls_and_brackets_runtime_state {
  let reference = {
    name: "4-2",
    power_control: {
      value: "auto",
      complete: true,
    },
    autosuspend_delay_ms: {
      value: -1,
      complete: true,
    },
    runtime_status: {
      value: "active",
      complete: true,
    },
    configuration_count: {
      value: 2,
      complete: true,
    },
    active_configuration: {
      value: 1,
      complete: true,
    },
  }
  let candidate = """{"usb":{"status":{"state":"complete","enumeration_succeeded":true},"devices":[{"sysfs_name":"4-2","power_control":"auto","autosuspend_delay_ms":-1,"runtime_status":"active","configuration_count":2,"active_configuration":1}]}}"""
  assert report_checks.compare_usb_power(candidate, [reference], [reference])?.exact
  let wrong = report_checks.compare_usb_power(
    candidate.replace("\"power_control\":\"auto\"", "\"power_control\":\"on\""),
    [reference],
    [reference],
  )?
  assert wrong.field_mismatches == ["4-2.power_control"]
  let changed = report_checks.compare_usb_power(
    candidate,
    [reference],
    [{...reference, runtime_status: {value: "suspended", complete: true}}],
  )?
  assert changed.unstable_fields == ["4-2.runtime_status"]
  let incomplete = report_checks.compare_usb_power(
    candidate.replace("\"runtime_status\":\"active\"", "\"runtime_status\":null")
      .replace("]}}", "]},\"issues\":[{\"section\":\"usb\",\"field\":\"devices.4-2.runtime_status\"}]}"),
    [reference],
    [reference],
  )?
  assert incomplete.unstable_fields == ["4-2.runtime_status"]
}

test test_system_report_usb_power_number_reference_keeps_signed_autosuspend_delay {
  assert report_checks.parse_usb_power_number("-1", true)? == -1
  assert report_checks.parse_usb_power_number("0", true)? == 0
  assert report_checks.parse_usb_power_number("2", false)? == 2
  for invalid in ["", "1.5", "-", "9007199254740992"] {
    test.error_kind(report_checks.parse_usb_power_number(invalid, true), "SystemReportCheckError.Invalid")?
  }

  test.error_kind(report_checks.parse_usb_power_number("-1", false), "SystemReportCheckError.Invalid")?
}

test test_system_report_usb_power_rooted_reference_reads_runtime_and_configuration {
  let root = fs.tempdir()?
  defer root.close()?
  let device_path = p"sys/bus/usb/devices/4-2"
  root.mkdir(fp"${device_path}/power", parents: true)?
  root.write(
    fp"${device_path}/power/control",
    """auto
""",
  )?
  root.write(
    fp"${device_path}/power/autosuspend_delay_ms",
    """-1
""",
  )?
  root.write(
    fp"${device_path}/power/runtime_status",
    """active
""",
  )?
  root.write(
    fp"${device_path}/bNumConfigurations",
    """2
""",
  )?
  root.write(
    fp"${device_path}/bConfigurationValue",
    """1
""",
  )?
  let devices = report_checks.read_usb_power_reference(root)?
  assert devices.len() == 1
  assert devices[0].power_control == {value: "auto", complete: true}
  assert devices[0].autosuspend_delay_ms == {value: -1, complete: true}
  assert devices[0].runtime_status == {value: "active", complete: true}
  assert devices[0].configuration_count == {value: 2, complete: true}
  root.write(
    fp"${device_path}/bNumConfigurations",
    """-1
""",
  )?
  test.error_kind(report_checks.read_usb_power_reference(root), "SystemReportCheckError.Invalid")?
}

test test_system_report_usb_interface_descriptors_keep_alternates_and_endpoint_ownership {
  let descriptors = b"\t\x02,\0\x01\x01\0\x802\t\x04\0\0\x01\xff\0\0\0\x030\0\x07\x05\x81\x02@\0\0\t\x04\0\x01\x01\x08\x06P\0\x07\x05\x02\x03 \0\x04"
  let settings = report_checks.parse_usb_interface_descriptors(descriptors)?
  assert settings.len() == 2
  assert settings[0].configuration_value == 1
  assert settings[0].number == 0
  assert settings[0].endpoints[0].address == 129
  assert settings[1].number == 1
  assert settings[1].class_code == 8
  assert settings[1].endpoints[0].address == 2
  let configurations = report_checks.parse_usb_interface_descriptors(
    b"\t\x02\x19\0\x01\x01\0\x802\t\x04\0\0\x01\xff\0\0\0\x07\x05\x81\x02@\0\0\t\x02\x19\0\x01\x02\0\x802\t\x04\0\0\x01\x08\x06P\0\x07\x05\x82\x02\0\x02\0",
  )?
  assert configurations.len() == 2
  assert configurations[0].configuration_value == 1
  assert configurations[1].configuration_value == 2
  assert configurations[1].endpoints[0].address == 130
  test.error_kind(
    report_checks.parse_usb_interface_descriptors(b"\t\x02\t\0\x01\x01\0\x802\x07\x05\x81\x02@\0\0"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_usb_interface_reference_scores_active_and_available_settings {
  let settings = report_checks.parse_usb_interface_descriptors(
    b"\t\x02\x19\0\x01\x01\0\x802\t\x04\0\0\x01\xff\0\0\0\x07\x05\x81\x02@\0\0",
  )?
  let reference = {
    device_name: "4-2",
    name: "4-2:1.0",
    number: 0,
    driver: {
      value: "usbhid",
      complete: true,
    },
    active_alternate: {
      value: 0,
      complete: true,
    },
    active_class: {
      value: 255,
      complete: true,
    },
    active_subclass: {
      value: 0,
      complete: true,
    },
    active_protocol: {
      value: 0,
      complete: true,
    },
    active_endpoint_count: {
      value: 1,
      complete: true,
    },
    settings: settings,
    descriptors_complete: true,
  }
  let candidate = """{"usb":{"status":{"state":"complete","enumeration_succeeded":true},"devices":[{"sysfs_name":"4-2","interfaces":[{"number":0,"name":"4-2:1.0","driver":"usbhid","active_alternate":0,"alternate_settings":[{"configuration_value":1,"number":0,"class_code":255,"subclass":0,"protocol":0,"endpoints":[{"address":129,"direction":"in","transfer_type":"bulk","max_packet_size":64,"interval":0}]}]}]}]}}"""
  assert report_checks.compare_usb_interfaces(candidate, [reference], [reference])?.exact
  let wrong_driver = report_checks.compare_usb_interfaces(
    candidate.replace("usbhid", "usb-storage"),
    [reference],
    [reference],
  )?
  assert wrong_driver.field_mismatches == ["4-2:1.0.driver"]
  let wrong_endpoint = report_checks.compare_usb_interfaces(
    candidate.replace("\"address\":129", "\"address\":130"),
    [reference],
    [reference],
  )?
  assert wrong_endpoint.field_mismatches == ["4-2:1.0.alternate_settings"]
  let changed = report_checks.compare_usb_interfaces(
    candidate,
    [reference],
    [{...reference, active_alternate: {value: 1, complete: true}}],
  )?
  assert "4-2:1.0.active_alternate" in changed.unstable_fields
  let partial_candidate = """{"usb":{"status":{"state":"partial","enumeration_succeeded":false},"devices":[{"sysfs_name":"4-2","interfaces":[]}]}}"""
  let incomplete = report_checks.compare_usb_interfaces(partial_candidate, [reference], [reference])?
  assert incomplete.missing_names == []
  assert "4-2:1.0.presence" in incomplete.unstable_fields
}

test test_system_report_usb_interface_reference_selects_active_alternate_from_available_settings {
  let settings = report_checks.parse_usb_interface_descriptors(
    b"\t\x02)\0\x01\x01\0\x802\t\x04\0\0\x01\xff\0\0\0\x07\x05\x81\x02@\0\0\t\x04\0\x01\x01\x08\x06P\0\x07\x05\x02\x03 \0\x04",
  )?
  let reference = {
    device_name: "4-2",
    name: "4-2:1.0",
    number: 0,
    driver: {
      value: null,
      complete: true,
    },
    active_alternate: {
      value: 1,
      complete: true,
    },
    active_class: {
      value: 8,
      complete: true,
    },
    active_subclass: {
      value: 6,
      complete: true,
    },
    active_protocol: {
      value: 80,
      complete: true,
    },
    active_endpoint_count: {
      value: 1,
      complete: true,
    },
    settings: settings,
    descriptors_complete: true,
  }
  let candidate = """{"usb":{"status":{"state":"complete","enumeration_succeeded":true},"devices":[{"sysfs_name":"4-2","interfaces":[{"number":0,"name":"4-2:1.0","driver":null,"active_alternate":1,"alternate_settings":[{"configuration_value":1,"number":0,"class_code":255,"subclass":0,"protocol":0,"endpoints":[{"address":129,"direction":"in","transfer_type":"bulk","max_packet_size":64,"interval":0}]},{"configuration_value":1,"number":1,"class_code":8,"subclass":6,"protocol":80,"endpoints":[{"address":2,"direction":"out","transfer_type":"interrupt","max_packet_size":32,"interval":4}]}]}]}]}}"""
  assert report_checks.compare_usb_interfaces(candidate, [reference], [reference])?.exact
  let wrong_active_class = report_checks.compare_usb_interfaces(
    candidate,
    [{...reference, active_class: {value: 255, complete: true}}],
    [{...reference, active_class: {value: 255, complete: true}}],
  )?
  assert "4-2:1.0.active_class" in wrong_active_class.field_mismatches
}

test test_system_report_usb_interface_rooted_reference_reads_driver_active_class_and_descriptors {
  let root = fs.tempdir()?
  defer root.close()?
  let device_path = p"sys/bus/usb/devices/4-2"
  let interface_path = p"sys/bus/usb/devices/4-2:1.0"
  root.mkdir(device_path, parents: true)?
  root.mkdir(interface_path, parents: true)?
  root.write(
    fp"${device_path}/descriptors",
    b"\t\x02\x19\0\x01\x01\0\x802\t\x04\0\0\x01\xff\0\0\0\x07\x05\x81\x02@\0\0",
  )?
  root.write(
    fp"${interface_path}/bInterfaceNumber",
    """00
""",
  )?
  root.write(
    fp"${interface_path}/bAlternateSetting",
    """0
""",
  )?
  root.write(
    fp"${interface_path}/bInterfaceClass",
    """ff
""",
  )?
  root.write(
    fp"${interface_path}/bInterfaceSubClass",
    """00
""",
  )?
  root.write(
    fp"${interface_path}/bInterfaceProtocol",
    """00
""",
  )?
  root.write(
    fp"${interface_path}/bNumEndpoints",
    """01
""",
  )?
  root.symlink(../../drivers/usbhid, fp"${interface_path}/driver")?
  let rows = report_checks.read_usb_interface_reference(root)?
  assert rows.len() == 1
  assert rows[0].driver == {value: "usbhid", complete: true}
  assert rows[0].active_class == {value: 255, complete: true}
  assert rows[0].active_endpoint_count == {value: 1, complete: true}
  assert rows[0].settings[0].endpoints[0].address == 129
  root.write(
    fp"${interface_path}/bInterfaceNumber",
    """01
""",
  )?
  test.error_kind(report_checks.read_usb_interface_reference(root), "SystemReportCheckError.Invalid")?
}

test test_system_report_power_supply_reference_scores_units_and_brackets_gauges {
  let battery = {
    name: "BAT0",
    kind: {
      value: "Battery",
      complete: true,
    },
    status: {
      value: "Charging",
      complete: true,
    },
    health: {
      value: "Good",
      complete: true,
    },
    capacity_percent: {
      value: 68,
      complete: true,
    },
    energy_now_uwh: {
      value: null,
      complete: true,
    },
    energy_full_uwh: {
      value: null,
      complete: true,
    },
    charge_now_uah: {
      value: 2000000,
      complete: true,
    },
    charge_full_uah: {
      value: 3000000,
      complete: true,
    },
    voltage_now_uv: {
      value: 12000000,
      complete: true,
    },
    current_now_ua: {
      value: -250000,
      complete: true,
    },
    cycle_count: {
      value: null,
      complete: true,
    },
  }
  let candidate = """{"power":{"status":{"state":"complete","enumeration_succeeded":true},"supplies":[{"name":"BAT0","kind":"Battery","status":"Charging","health":"Good","capacity_percent":68,"energy_now_uwh":null,"energy_full_uwh":null,"charge_now_uah":2000000,"charge_full_uah":3000000,"voltage_now_uv":12000000,"current_now_ua":-250000,"cycle_count":null}]}}"""
  assert report_checks.compare_power_supplies(candidate, [battery], [battery])?.exact
  let wrong = report_checks.compare_power_supplies(
    candidate.replace("\"capacity_percent\":68", "\"capacity_percent\":67"),
    [battery],
    [battery],
  )?
  assert wrong.field_mismatches == ["BAT0.capacity_percent"]
  let changed = report_checks.compare_power_supplies(
    candidate,
    [battery],
    [{...battery, status: {value: "Full", complete: true}}],
  )?
  assert changed.unstable_fields == ["BAT0.status"]
  let partial_candidate = """{"power":{"status":{"state":"partial","enumeration_succeeded":false},"supplies":[]},"issues":[{"section":"power","field":"supplies"}]}"""
  let partial = report_checks.compare_power_supplies(partial_candidate, [battery], [battery])?
  assert partial.missing_names == []
  assert "BAT0.presence" in partial.unstable_fields
  let cap_only_issue = candidate.replace("\"state\":\"complete\"", "\"state\":\"partial\"")
    .replace("]}}", "]},\"issues\":[{\"section\":\"power\",\"field\":\"cap_zones\"}]}")
  assert report_checks.compare_power_supplies(cap_only_issue, [battery], [battery])?.exact
}

test test_system_report_power_supply_number_reference_keeps_signed_current_and_exact_range {
  assert report_checks.parse_power_supply_number("-250000", true)? == -250000
  assert report_checks.parse_power_supply_number("0", false)? == 0
  for invalid in ["", "-", "1.5", "9007199254740992"] {
    test.error_kind(report_checks.parse_power_supply_number(invalid, true), "SystemReportCheckError.Invalid")?
  }

  test.error_kind(report_checks.parse_power_supply_number("-1", false), "SystemReportCheckError.Invalid")?
}

test test_system_report_power_supply_rooted_reference_reads_signed_current_and_missing_fields {
  let root = fs.tempdir()?
  defer root.close()?
  let battery = p"sys/class/power_supply/BAT0"
  root.mkdir(battery, parents: true)?
  root.write(
    fp"${battery}/type",
    """Battery
""",
  )?
  root.write(
    fp"${battery}/capacity",
    """68
""",
  )?
  root.write(
    fp"${battery}/current_now",
    """-250000
""",
  )?
  let supplies = report_checks.read_power_supply_reference(root)?
  assert supplies.len() == 1
  assert supplies[0].kind == {value: "Battery", complete: true}
  assert supplies[0].capacity_percent == {value: 68, complete: true}
  assert supplies[0].current_now_ua == {value: -250000, complete: true}
  assert supplies[0].energy_now_uwh == {value: null, complete: true}
  root.write(
    fp"${battery}/capacity",
    """101
""",
  )?
  test.error_kind(report_checks.read_power_supply_reference(root), "SystemReportCheckError.Invalid")?
}

test test_system_report_power_supply_bundle_replays_raw_attributes_and_rejects_tampering { |ctx|
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let battery = p"sys/devices/platform/example/power_supply/BAT0"
  source.mkdir(battery, parents: true)?
  source.mkdir(p"sys/class/power_supply", parents: true)?
  source.symlink(../../devices/platform/example/power_supply/BAT0, p"sys/class/power_supply/BAT0")?
  for item in [
    {
      name: "type",
      value: """Battery
""",
    },
    {
      name: "status",
      value: """Discharging
""",
    },
    {
      name: "health",
      value: """Good
""",
    },
    {
      name: "capacity",
      value: """68
""",
    },
    {
      name: "energy_now",
      value: """50000000
""",
    },
    {
      name: "energy_full",
      value: """90000000
""",
    },
    {
      name: "charge_now",
      value: """1000000
""",
    },
    {
      name: "charge_full",
      value: """1500000
""",
    },
    {
      name: "voltage_now",
      value: """12000000
""",
    },
    {
      name: "current_now",
      value: """-250000
""",
    },
    {
      name: "cycle_count",
      value: """120
""",
    },
  ] {
    source.write(fp"${battery}/${item.name}", item.value)?
  }

  report_checks.capture_power_supply_bundle(source, bundle, "synthetic_fixture")?
  let replay = report_checks.replay_power_supply_bundle(bundle)?
  assert replay.exact
  assert replay.matched_count == 1
  let bundle_path = bundle.host_path()?
  let output = test.temp_path(ctx, name: "power-supply-replay.stdout")
  let stderr = test.temp_path(ctx, name: "power-supply-replay.stderr")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --replay-power-supply-bundle $bundle_path > $output 2> $stderr
  let exited_successfully = status.exited_with(0)
  let diagnostic = stderr.read_text()?
  assert exited_successfully, diagnostic
  assert "power supply raw replay: exact" in output.read_text()?
  bundle.write(
    fp"${battery}/energy_now",
    """51000000
""",
  )?
  test.error_kind(report_checks.validate_power_supply_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_power_supply_bundle_keeps_absent_class_unscoreable {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  report_checks.capture_power_supply_bundle(source, bundle, "synthetic_fixture")?
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"listing_state\": \"absent\"" in metadata
  assert "\"scoreable\": false" in metadata
  test.error_kind(report_checks.validate_power_supply_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_power_supply_bundle_keeps_missing_type_unscoreable {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/class/power_supply/BAT0", parents: true)?
  source.write(
    p"sys/class/power_supply/BAT0/status",
    """Charging
""",
  )?
  report_checks.capture_power_supply_bundle(source, bundle, "synthetic_fixture")?
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"scoreable\": false" in metadata
  test.error_kind(report_checks.validate_power_supply_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_power_supply_bundle_rejects_escaping_class_link {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/class/power_supply", parents: true)?
  source.symlink(../../devices/../rogue/BAT0, p"sys/class/power_supply/BAT0")?
  test.error_kind(
    report_checks.capture_power_supply_bundle(source, bundle, "synthetic_fixture"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_powercap_reference_scores_nested_zones_constraints_and_counter_brackets {
  let package = report_checks.PowerCapZoneReference(
    entry_name: "intel-rapl:0",
    name: {
      value: "package-0",
      complete: true,
    },
    parent: {
      value: null,
      complete: true,
    },
    energy_uj: {
      value: 100,
      complete: true,
    },
    maximum_energy_range_uj: {
      value: 1000,
      complete: true,
    },
    constraints: [
      {
        index: 0,
        name: {
          value: "long_term",
          complete: true,
        },
        power_limit_uw: {
          value: 45000000,
          complete: true,
        },
        time_window_us: {
          value: 1000000,
          complete: true,
        },
      },
      {
        index: 10,
        name: {
          value: null,
          complete: true,
        },
        power_limit_uw: {
          value: 80000000,
          complete: true,
        },
        time_window_us: {
          value: null,
          complete: true,
        },
      },
    ],
    constraints_complete: true,
  )
  let core = report_checks.PowerCapZoneReference(
    entry_name: "intel-rapl:0:0",
    name: {
      value: "core-0",
      complete: true,
    },
    parent: {
      value: "intel-rapl:0",
      complete: true,
    },
    energy_uj: {
      value: null,
      complete: true,
    },
    maximum_energy_range_uj: {
      value: null,
      complete: true,
    },
    constraints: [],
    constraints_complete: true,
  )
  let candidate = """{"power":{"status":{"state":"complete","enumeration_succeeded":true},"cap_zones":[{"entry_name":"intel-rapl:0","name":"package-0","parent":null,"energy_uj":120,"maximum_energy_range_uj":1000,"constraints":[{"index":0,"name":"long_term","power_limit_uw":45000000,"time_window_us":1000000},{"index":10,"name":null,"power_limit_uw":80000000,"time_window_us":null}]},{"entry_name":"intel-rapl:0:0","name":"core-0","parent":"intel-rapl:0","energy_uj":null,"maximum_energy_range_uj":null,"constraints":[]}]}}"""
  let later = {...package, energy_uj: {value: 150, complete: true}}
  assert report_checks.compare_powercap(candidate, [package, core], [later, core])?.exact
  let wrong_parent = report_checks.compare_powercap(
    candidate.replace("\"parent\":\"intel-rapl:0\"", "\"parent\":null"),
    [package, core],
    [later, core],
  )?
  assert wrong_parent.field_mismatches == ["intel-rapl:0:0.parent"]
  let wrong_counter = report_checks.compare_powercap(
    candidate.replace("\"energy_uj\":120", "\"energy_uj\":200"),
    [package, core],
    [later, core],
  )?
  assert wrong_counter.field_mismatches == ["intel-rapl:0.energy_uj"]
  let wrong_limit = report_checks.compare_powercap(
    candidate.replace("\"power_limit_uw\":80000000", "\"power_limit_uw\":80000001"),
    [package, core],
    [later, core],
  )?
  assert wrong_limit.field_mismatches == ["intel-rapl:0.constraint_10.power_limit_uw"]
  let changed_constraint = {
    ...package,
    constraints: [
      package.constraints[0],
      {
        ...package.constraints[1],
        power_limit_uw: {
          value: 80000001,
          complete: true,
        },
      },
    ],
  }
  let changed_limit = report_checks.compare_powercap(
    candidate,
    [package, core],
    [{...changed_constraint, energy_uj: {value: 150, complete: true}}, core],
  )?
  assert "intel-rapl:0.constraint_10.power_limit_uw" in changed_limit.unstable_fields
  let wrapped = report_checks.compare_powercap(
    candidate,
    [{...package, energy_uj: {value: 950, complete: true}}, core],
    [{...package, energy_uj: {value: 50, complete: true}}, core],
  )?
  assert "intel-rapl:0.energy_uj" in wrapped.unstable_fields
}

test test_system_report_powercap_rooted_reference_keeps_zone_parent_and_sparse_constraint_indices {
  let root = fs.tempdir()?
  defer root.close()?
  let package = p"sys/class/powercap/intel-rapl:0"
  let core = fp"${package}/intel-rapl:0:0"
  root.mkdir(core, parents: true)?
  root.write(
    fp"${package}/name",
    """package-0
""",
  )?
  root.write(
    fp"${package}/energy_uj",
    """100
""",
  )?
  root.write(
    fp"${package}/max_energy_range_uj",
    """1000
""",
  )?
  root.write(
    fp"${package}/constraint_0_power_limit_uw",
    """45000000
""",
  )?
  root.write(
    fp"${package}/constraint_0_name",
    """long_term
""",
  )?
  root.write(
    fp"${package}/constraint_0_time_window_us",
    """1000000
""",
  )?
  root.write(
    fp"${package}/constraint_10_power_limit_uw",
    """80000000
""",
  )?
  root.write(
    fp"${core}/name",
    """core-0
""",
  )?
  root.symlink(p"intel-rapl:0/intel-rapl:0:0", p"sys/class/powercap/intel-rapl:0:0")?
  let zones = report_checks.read_powercap_reference(root)?
  assert zones.len() == 2
  let package_zone = (zones
    |> where .entry_name == "intel-rapl:0"
    |> first())?
  let core_zone = (zones
    |> where .entry_name == "intel-rapl:0:0"
    |> first())?
  assert (package_zone.constraints |> map .index) == [0, 10]
  assert core_zone.parent == {value: "intel-rapl:0", complete: true}
  root.write(
    fp"${package}/constraint_10_power_limit_uw",
    """-1
""",
  )?
  test.error_kind(report_checks.read_powercap_reference(root), "SystemReportCheckError.Invalid")?
}

test test_system_report_powercap_capture_replays_nested_zones_and_rejects_tampering {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let package = p"sys/devices/virtual/powercap/intel-rapl/intel-rapl:0"
  let core = fp"${package}/intel-rapl:0:0"
  source.mkdir(core, parents: true)?
  source.mkdir(p"sys/class/powercap", parents: true)?
  source.write(
    fp"${package}/name",
    """package-0
""",
  )?
  source.write(
    fp"${package}/energy_uj",
    """100
""",
  )?
  source.write(
    fp"${package}/max_energy_range_uj",
    """1000
""",
  )?
  source.write(
    fp"${package}/constraint_10_power_limit_uw",
    """80000000
""",
  )?
  source.write(
    fp"${package}/constraint_10_name",
    """long_term
""",
  )?
  source.write(
    fp"${core}/name",
    """core-0
""",
  )?
  source.symlink(../../devices/virtual/powercap/intel-rapl/intel-rapl:0, p"sys/class/powercap/intel-rapl:0")?
  source.symlink(
    ../../devices/virtual/powercap/intel-rapl/intel-rapl:0/intel-rapl:0:0,
    p"sys/class/powercap/intel-rapl:0:0",
  )?
  report_checks.capture_powercap_bundle(source, bundle, "synthetic_fixture")?
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"reference_adapter\": \"powercap-raw-v1\"" in metadata
  assert report_checks.replay_powercap_bundle(bundle)?.exact
  assert report_checks.validate_powercap_bundle(bundle)?.len() == 2
  bundle.write(
    fp"${package}/constraint_10_power_limit_uw",
    """80000001
""",
  )?
  test.error_kind(report_checks.validate_powercap_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_powercap_capture_preserves_absence_and_rejects_unrelated_links {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  report_checks.capture_powercap_bundle(source, bundle, "synthetic_fixture")?
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"listing_state\": \"absent\"" in metadata
  assert "\"scoreable\": false" in metadata
  assert ! bundle.exists(p"sys/class/powercap")?
  test.error_kind(report_checks.validate_powercap_bundle(bundle), "SystemReportCheckError.Invalid")?
  let rogue = p"sys/devices/virtual/rogue"
  source.mkdir(rogue, parents: true)?
  source.mkdir(p"sys/devices/virtual/powercap", parents: true)?
  source.write(
    fp"${rogue}/name",
    """rogue
""",
  )?
  source.mkdir(p"sys/class/powercap", parents: true)?
  source.symlink(../../devices/virtual/powercap/../rogue, p"sys/class/powercap/rogue")?
  let second_bundle = fs.tempdir()?
  defer second_bundle.close()?
  test.error_kind(
    report_checks.capture_powercap_bundle(source, second_bundle, "synthetic_fixture"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_pci_capture_replays_raw_identity_links_and_rejects_tampering {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let device = p"sys/devices/pci0000:00/0000:00:1f.0"
  source.mkdir(device, parents: true)?
  source.mkdir(p"sys/bus/pci/devices", parents: true)?
  source.symlink(../../../devices/pci0000:00/0000:00:1f.0, p"sys/bus/pci/devices/0000:00:1f.0")?
  for item in [
    {
      name: "vendor",
      value: """0x8086
""",
    },
    {
      name: "device",
      value: """0x1234
""",
    },
    {
      name: "subsystem_vendor",
      value: """0x8086
""",
    },
    {
      name: "subsystem_device",
      value: """0x0001
""",
    },
    {
      name: "class",
      value: """0x060400
""",
    },
    {
      name: "revision",
      value: """0x02
""",
    },
    {
      name: "numa_node",
      value: """-1
""",
    },
    {
      name: "current_link_speed",
      value: """8.0 GT/s PCIe
""",
    },
    {
      name: "current_link_width",
      value: """8
""",
    },
    {
      name: "max_link_speed",
      value: """16.0 GT/s PCIe
""",
    },
    {
      name: "max_link_width",
      value: """16
""",
    },
  ] {
    source.write(fp"${device}/${item.name}", item.value)?
  }

  source.symlink(../../../bus/pci/drivers/example, fp"${device}/driver")?
  source.symlink(../../../kernel/iommu_groups/7, fp"${device}/iommu_group")?
  report_checks.capture_pci_bundle(source, bundle, "synthetic_fixture")?
  let replay = report_checks.replay_pci_bundle(bundle)?
  assert replay.identity.exact_static
  assert replay.binding.exact
  assert replay.link.exact
  assert replay.identity.matched_count == 1
  bundle.write(
    fp"${device}/vendor",
    """0x8087
""",
  )?
  test.error_kind(report_checks.validate_pci_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(
    fp"${device}/vendor",
    """0x8086
""",
  )?
  bundle.remove(fp"${device}/current_link_width")?
  test.error_kind(report_checks.validate_pci_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_hwmon_capture_replays_raw_channels_and_rejects_tampering {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let chip = p"sys/devices/platform/example/hwmon/hwmon3"
  source.mkdir(chip, parents: true)?
  source.mkdir(p"sys/class/hwmon", parents: true)?
  source.symlink(../../devices/platform/example/hwmon/hwmon3, p"sys/class/hwmon/hwmon3")?
  source.write(
    fp"${chip}/name",
    """example
""",
  )?
  source.write(
    fp"${chip}/temp1_input",
    """42000
""",
  )?
  source.write(
    fp"${chip}/temp1_label",
    """package
""",
  )?
  source.write(
    fp"${chip}/temp1_min",
    """10000
""",
  )?
  source.write(
    fp"${chip}/temp1_max",
    """75000
""",
  )?
  source.write(
    fp"${chip}/temp1_crit",
    """95000
""",
  )?
  source.write(
    fp"${chip}/temp1_alarm",
    """0
""",
  )?
  report_checks.capture_hwmon_bundle(source, bundle, "synthetic_fixture")?
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"reference_adapter\": \"hwmon-sysfs-raw-v1\"" in metadata
  assert "\"scoreable\": true" in metadata
  let replay = report_checks.replay_hwmon_bundle(bundle)?
  assert replay.exact
  assert replay.matched_count == 1
  bundle.write(
    fp"${chip}/temp1_input",
    """43000
""",
  )?
  test.error_kind(report_checks.validate_hwmon_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_hwmon_capture_rejects_escaping_class_link {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/class/hwmon", parents: true)?
  source.symlink(../../devices/../rogue/hwmon3, p"sys/class/hwmon/hwmon3")?
  test.error_kind(
    report_checks.capture_hwmon_bundle(source, bundle, "synthetic_fixture"),
    "SystemReportCheckError.Invalid",
  )?
  source.remove(p"sys/class/hwmon/hwmon3")?
  source.mkdir(p"sys/class/hwmon/hwmon3")?
  source.symlink(../../../etc, p"sys/class/hwmon/hwmon3/device")?
  test.error_kind(
    report_checks.capture_hwmon_bundle(source, bundle, "synthetic_fixture"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_hwmon_capture_keeps_absent_class_unscoreable {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  report_checks.capture_hwmon_bundle(source, bundle, "synthetic_fixture")?
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"listing_state\": \"absent\"" in metadata
  assert "\"scoreable\": false" in metadata
  assert ! bundle.exists(p"sys/class/hwmon")?
  test.error_kind(report_checks.validate_hwmon_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_block_bundle_replays_sparse_partition_and_layered_edges {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let disk = p"sys/devices/pci0000:00/0000:00:01.0/block/sda"
  let partition = fp"${disk}/sda3"
  let stacked = p"sys/devices/virtual/block/dm-0"
  source.mkdir(p"sys/class/block", parents: true)?
  for device in [disk, partition, stacked] {
    source.mkdir(fp"${device}/holders", parents: true)?
    source.write(
      fp"${device}/ro",
      """0
""",
    )?
  }

  for device in [disk, stacked] {
    source.mkdir(fp"${device}/slaves", parents: true)?
  }

  source.symlink(../../devices/pci0000:00/0000:00:01.0/block/sda, p"sys/class/block/sda")?
  source.symlink(../../devices/pci0000:00/0000:00:01.0/block/sda/sda3, p"sys/class/block/sda3")?
  source.symlink(../../devices/virtual/block/dm-0, p"sys/class/block/dm-0")?
  source.symlink(../../../../../virtual/block/dm-0, fp"${disk}/holders/dm-0")?
  source.symlink(../../../../pci0000:00/0000:00:01.0/block/sda, fp"${stacked}/slaves/sda")?
  for item in [
    {
      device: disk,
      dev: """8:0
""",
      size: """1024
""",
    },
    {
      device: partition,
      dev: """8:3
""",
      size: """128
""",
    },
    {
      device: stacked,
      dev: """253:0
""",
      size: """512
""",
    },
  ] {
    source.write(fp"${item.device}/dev", item.dev)?
    source.write(fp"${item.device}/size", item.size)?
  }

  source.write(
    fp"${partition}/partition",
    """3
""",
  )?
  source.mkdir(fp"${disk}/queue", parents: true)?
  for item in [
    {
      name: "logical_block_size",
      value: """512
""",
    },
    {
      name: "physical_block_size",
      value: """4096
""",
    },
    {
      name: "rotational",
      value: """1
""",
    },
    {
      name: "scheduler",
      value: """none [mq-deadline]
""",
    },
    {
      name: "read_ahead_kb",
      value: """128
""",
    },
    {
      name: "discard_granularity",
      value: """4096
""",
    },
    {
      name: "discard_max_bytes",
      value: """1048576
""",
    },
  ] {
    source.write(fp"${disk}/queue/${item.name}", item.value)?
  }

  source.write(
    fp"${disk}/removable",
    """0
""",
  )?
  source.write(
    fp"${disk}/stat",
    """10 0 8 1 2 0 16 2 0 3 4
""",
  )?
  report_checks.capture_block_bundle(source, bundle, "synthetic_fixture")?
  let replay = report_checks.replay_block_bundle(bundle)?
  assert replay.identity.exact
  assert replay.queue.exact
  assert replay.sources.exact
  assert replay.identity.reference_count == 3
  assert replay.identity.matched_edges == 2
  bundle.write(
    fp"${partition}/size",
    """129
""",
  )?
  test.error_kind(report_checks.validate_block_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(
    fp"${partition}/size",
    """128
""",
  )?
  bundle.write(
    fp"${partition}/queue/logical_block_size",
    """512
""",
  )?
  test.error_kind(report_checks.validate_block_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.remove(fp"${partition}/queue/logical_block_size")?
  bundle.remove(fp"${disk}/holders/dm-0")?
  bundle.symlink(../../dm-1, fp"${disk}/holders/dm-0")?
  test.error_kind(report_checks.validate_block_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_block_bundle_rejects_escaping_class_link {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/class/block", parents: true)?
  source.symlink(../../devices/../rogue/sda, p"sys/class/block/sda")?
  test.error_kind(
    report_checks.capture_block_bundle(source, bundle, "synthetic_fixture"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_block_bundle_rejects_misdirected_layer_link {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/class/block/sda/holders", parents: true)?
  source.mkdir(p"sys/class/block/sda/slaves")?
  source.mkdir(p"sys/class/block/dm-0/holders", parents: true)?
  source.mkdir(p"sys/class/block/dm-0/slaves")?
  source.symlink(../../dm-1, p"sys/class/block/sda/holders/dm-0")?
  test.error_kind(
    report_checks.capture_block_bundle(source, bundle, "synthetic_fixture"),
    "SystemReportCheckError.Invalid",
  )?
  source.remove(p"sys/class/block/sda/holders/dm-0")?
  source.symlink(../../rogue/dm-0, p"sys/class/block/sda/holders/dm-0")?
  test.error_kind(
    report_checks.capture_block_bundle(source, bundle, "synthetic_fixture"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_block_bundle_keeps_absent_class_unscoreable {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  report_checks.capture_block_bundle(source, bundle, "synthetic_fixture")?
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"listing_state\": \"absent\"" in metadata
  assert "\"scoreable\": false" in metadata
  test.error_kind(report_checks.validate_block_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_block_raw_reference_checks_each_layer_direction {
  let reference = report_checks.BlockRawReference(
    devices: [
      {
        name: "sda",
        major: 8,
        minor: 0,
        kind: "disk",
        size_bytes: 8192,
        logical_sector_bytes: null,
        physical_sector_bytes: null,
        removable: null,
        rotational: null,
        read_only: null,
        parent_name: null,
        holders: [
          "dm-0",
        ],
        slaves: [],
      },
      {
        name: "dm-0",
        major: 253,
        minor: 0,
        kind: "virtual",
        size_bytes: 8192,
        logical_sector_bytes: null,
        physical_sector_bytes: null,
        removable: null,
        rotational: null,
        read_only: null,
        parent_name: null,
        holders: [],
        slaves: [
          "sda",
        ],
      },
    ],
    edges: [
      {
        parent_name: "sda",
        child_name: "dm-0",
        partition: false,
      },
    ],
    queue: [],
  )
  let candidate = """{"storage":{"devices":[{"name":"sda","major":8,"minor":0,"kind":"disk","size_bytes":8192,"logical_sector_bytes":null,"physical_sector_bytes":null,"removable":null,"rotational":null,"read_only":null,"parent_device_index":null,"holder_indices":[],"slave_indices":[]},{"name":"dm-0","major":253,"minor":0,"kind":"virtual","size_bytes":8192,"logical_sector_bytes":null,"physical_sector_bytes":null,"removable":null,"rotational":null,"read_only":null,"parent_device_index":null,"holder_indices":[],"slave_indices":[0]}]}}"""
  let compared = report_checks.compare_block_raw(candidate, reference)?
  assert ! compared.exact
}

test test_system_report_pci_capture_rejects_bus_link_outside_devices_tree {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/bus/pci/devices", parents: true)?
  source.symlink(../../../devices/../rogue/0000:00:1f.0, p"sys/bus/pci/devices/0000:00:1f.0")?
  test.error_kind(
    report_checks.capture_pci_bundle(source, bundle, "synthetic_fixture"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_pci_capture_keeps_unavailable_pcie_links_unscored { |ctx|
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let device = p"sys/devices/pci0000:00/0000:00:1f.0"
  source.mkdir(device, parents: true)?
  source.mkdir(p"sys/bus/pci/devices", parents: true)?
  source.symlink(../../../devices/pci0000:00/0000:00:1f.0, p"sys/bus/pci/devices/0000:00:1f.0")?
  for item in [
    {
      name: "vendor",
      value: """0x8086
""",
    },
    {
      name: "device",
      value: """0x1234
""",
    },
    {
      name: "subsystem_vendor",
      value: """0x8086
""",
    },
    {
      name: "subsystem_device",
      value: """0x0001
""",
    },
    {
      name: "class",
      value: """0x060400
""",
    },
    {
      name: "revision",
      value: """0x02
""",
    },
  ] {
    source.write(fp"${device}/${item.name}", item.value)?
  }

  report_checks.capture_pci_bundle(source, bundle, "synthetic_fixture")?
  let replay = report_checks.replay_pci_bundle(bundle)?
  assert replay.identity.exact_static
  assert replay.binding.exact
  assert ! replay.link.eligible
  assert ! replay.link.exact
  let bundle_path = bundle.host_path()?
  let output = test.temp_path(ctx, name: "pci-unavailable.stdout")
  let stderr = test.temp_path(ctx, name: "pci-unavailable.stderr")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --replay-pci-bundle $bundle_path > $output 2> $stderr
  let exited_successfully = status.exited_with(0)
  let diagnostic = stderr.read_text()?
  assert exited_successfully, diagnostic
  assert "link=unavailable" in output.read_text()?
}

test test_system_report_usb_capture_replays_raw_devices_interfaces_and_rejects_tampering {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let device = p"sys/devices/pci0000:00/0000:00:14.0/usb1"
  let interface = p"sys/devices/pci0000:00/0000:00:14.0/usb1/usb1:1.0"
  let root_interface = p"sys/devices/pci0000:00/0000:00:14.0/usb1/1-0:1.0"
  source.mkdir(interface, parents: true)?
  source.mkdir(root_interface)?
  source.mkdir(p"sys/bus/usb/devices", parents: true)?
  source.mkdir(fp"${device}/power", parents: true)?
  source.symlink(../../../devices/pci0000:00/0000:00:14.0/usb1, p"sys/bus/usb/devices/usb1")?
  source.symlink(../../../devices/pci0000:00/0000:00:14.0/usb1/usb1:1.0, p"sys/bus/usb/devices/usb1:1.0")?
  source.symlink(../../../devices/pci0000:00/0000:00:14.0/usb1/1-0:1.0, p"sys/bus/usb/devices/1-0:1.0")?
  for item in [
    {
      name: "busnum",
      value: """1
""",
    },
    {
      name: "devnum",
      value: """1
""",
    },
    {
      name: "speed",
      value: """480
""",
    },
    {
      name: "idVendor",
      value: """1d6b
""",
    },
    {
      name: "idProduct",
      value: """0002
""",
    },
    {
      name: "bcdDevice",
      value: """0612
""",
    },
    {
      name: "bDeviceClass",
      value: """09
""",
    },
    {
      name: "bDeviceSubClass",
      value: """00
""",
    },
    {
      name: "bDeviceProtocol",
      value: """01
""",
    },
    {
      name: "manufacturer",
      value: """Fixture
""",
    },
    {
      name: "product",
      value: """Root Hub
""",
    },
    {
      name: "bNumConfigurations",
      value: """1
""",
    },
    {
      name: "bConfigurationValue",
      value: """1
""",
    },
    {
      name: "power/control",
      value: """auto
""",
    },
    {
      name: "power/autosuspend_delay_ms",
      value: """-1
""",
    },
    {
      name: "power/runtime_status",
      value: """active
""",
    },
  ] {
    source.write(fp"${device}/${item.name}", item.value)?
  }

  source.write(fp"${device}/descriptors", b"")?
  source.write(
    fp"${interface}/bInterfaceNumber",
    """00
""",
  )?
  source.write(
    fp"${interface}/bAlternateSetting",
    """0
""",
  )?
  source.symlink(../../../bus/usb/drivers/hub, fp"${interface}/driver")?
  report_checks.capture_usb_bundle(source, bundle, "synthetic_fixture")?
  let replay = report_checks.replay_usb_bundle(bundle)?
  assert replay.topology.exact
  assert replay.ids.exact
  assert replay.power.exact
  assert replay.interface.exact
  bundle.write(
    fp"${device}/idVendor",
    """1d6c
""",
  )?
  test.error_kind(report_checks.validate_usb_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(
    fp"${device}/idVendor",
    """1d6b
""",
  )?
  bundle.write(fp"${device}/descriptors", b"corrupt")?
  test.error_kind(report_checks.validate_usb_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_usb_capture_rejects_bus_link_outside_devices_tree {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/bus/usb/devices", parents: true)?
  source.symlink(../../../devices/../rogue/usb1, p"sys/bus/usb/devices/usb1")?
  test.error_kind(
    report_checks.capture_usb_bundle(source, bundle, "synthetic_fixture"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_usb_capture_keeps_unavailable_power_and_interfaces_unscored { |ctx|
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let device = p"sys/devices/pci0000:00/0000:00:14.0/usb1"
  source.mkdir(device, parents: true)?
  source.mkdir(p"sys/bus/usb/devices", parents: true)?
  source.symlink(../../../devices/pci0000:00/0000:00:14.0/usb1, p"sys/bus/usb/devices/usb1")?
  source.write(
    fp"${device}/idVendor",
    """1d6b
""",
  )?
  source.write(
    fp"${device}/idProduct",
    """0002
""",
  )?
  report_checks.capture_usb_bundle(source, bundle, "synthetic_fixture")?
  let replay = report_checks.replay_usb_bundle(bundle)?
  assert replay.topology.exact
  assert replay.ids.exact
  assert ! replay.power.eligible
  assert ! replay.interface.eligible
  let bundle_path = bundle.host_path()?
  let output = test.temp_path(ctx, name: "usb-unavailable.stdout")
  let stderr = test.temp_path(ctx, name: "usb-unavailable.stderr")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --replay-usb-bundle $bundle_path > $output 2> $stderr
  let exited_successfully = status.exited_with(0)
  let diagnostic = stderr.read_text()?
  assert exited_successfully, diagnostic
  assert "power=unavailable" in output.read_text()?
  assert "interfaces=unavailable" in output.read_text()?
}

test test_system_report_device_class_reference_scores_duplicate_labels_and_parent_indexes {
  let first = {
    class: "sound",
    entry_name: "card0",
    name: "Shared label",
    name_complete: true,
    parent_target: "../../../devices/pci0000:00/0000:00:1f.3",
    parent_complete: true,
  }
  let second = {
    ...first,
    entry_name: "card1",
    parent_target: "../../../devices/pci0000:00/0000:00:14.0/usb1/1-2/1-2:1.0",
  }
  let candidate = """{"devices":{"status":{"state":"complete","enumeration_succeeded":true},"devices":[{"class":"sound","entry_name":{"state":"observed","value":"card0"},"name":{"state":"observed","value":"Shared label"},"parent_pci_function_index":0,"parent_usb_device_index":null},{"class":"sound","entry_name":{"state":"observed","value":"card1"},"name":{"state":"observed","value":"Shared label"},"parent_pci_function_index":1,"parent_usb_device_index":0}]},"pci":{"status":{"state":"complete","enumeration_succeeded":true},"functions":[{"address":"0000:00:1f.3"},{"address":"0000:00:14.0"}]},"usb":{"status":{"state":"complete","enumeration_succeeded":true},"devices":[{"sysfs_name":"1-2"}]}}"""
  assert report_checks.compare_device_classes(candidate, [first, second], [first, second])?.exact
  let missing_parent = json.set(json.decode(candidate)?, ["devices", "devices", 1, "parent_usb_device_index"], null)?
  let mismatch = report_checks.compare_device_classes(json.encode(missing_parent)?, [first, second], [first, second])?
  assert mismatch.field_mismatches == ["sound:card1.usb_parent"]
  let unavailable_inventory = json.set(
    missing_parent,
    ["usb", "status"],
    {state: "absent", enumeration_succeeded: false},
  )?
  let partial_parent = report_checks.compare_device_classes(
    json.encode(unavailable_inventory)?,
    [first, second],
    [first, second],
  )?
  assert partial_parent.field_mismatches == []
  assert "sound:card1.usb_parent" in partial_parent.unstable_fields
  let changed = report_checks.compare_device_classes(
    candidate,
    [first, second],
    [first, {...second, parent_target: "../../../devices/pci0000:00/0000:00:14.0/usb1/1-3"}],
  )?
  assert "sound:card1.parent" in changed.unstable_fields
  let absent = json.remove(json.decode(candidate)?, ["devices", "devices", 1])?
  let missing = report_checks.compare_device_classes(json.encode(absent)?, [first, second], [first, second])?
  assert missing.missing_names == ["sound:card1"]
}

test test_system_report_device_class_rooted_reference_keeps_sysfs_entry_identity {
  let root = fs.tempdir()?
  defer root.close()?
  for entry in ["card0", "card1"] {
    root.mkdir(fp"sys/class/sound/${entry}", parents: true)?
    root.write(
      fp"sys/class/sound/${entry}/id",
      """Shared label
""",
    )?
  }

  let records = report_checks.read_device_class_reference(root)?
  assert records.len() == 2
  assert records |> any .entry_name == "card0"
  assert records |> any .entry_name == "card1"
  assert records |> all .name == "Shared label"
}

test test_system_report_hwmon_reference_scores_duplicate_chip_names_and_raw_units {
  let first = {
    chip_entry_name: "hwmon0",
    chip: {
      value: "same_chip",
      complete: true,
    },
    channel: "temp1",
    kind: "temperature",
    unit: "millidegrees_celsius",
    value: {
      value: 42000,
      complete: true,
    },
    label: {
      value: "CPU",
      complete: true,
    },
    minimum: {
      value: null,
      complete: true,
    },
    maximum: {
      value: 100000,
      complete: true,
    },
    critical: {
      value: null,
      complete: true,
    },
    alarm: {
      value: 0,
      complete: true,
    },
    parent_target: "../../../devices/pci0000:00/0000:00:1f.3",
    parent_complete: true,
  }
  let second = {
    ...first,
    chip_entry_name: "hwmon1",
    value: {
      value: 43000,
      complete: true,
    },
    parent_target: "../../../devices/pci0000:00/0000:00:14.0/usb1/1-2",
  }
  let candidate = """{"sensors":{"status":{"state":"complete","enumeration_succeeded":true},"channels":[{"chip_entry_name":"hwmon0","chip":"same_chip","channel":"temp1","kind":"temperature","unit":"millidegrees_celsius","value":42000,"label":{"state":"observed","value":"CPU"},"minimum":null,"maximum":100000,"critical":null,"alarm":false,"parent_device_class_index":null,"parent_pci_function_index":0,"parent_usb_device_index":null},{"chip_entry_name":"hwmon1","chip":"same_chip","channel":"temp1","kind":"temperature","unit":"millidegrees_celsius","value":43000,"label":{"state":"observed","value":"CPU"},"minimum":null,"maximum":100000,"critical":null,"alarm":false,"parent_device_class_index":null,"parent_pci_function_index":1,"parent_usb_device_index":0}]},"pci":{"status":{"state":"complete","enumeration_succeeded":true},"functions":[{"address":"0000:00:1f.3"},{"address":"0000:00:14.0"}]},"usb":{"status":{"state":"complete","enumeration_succeeded":true},"devices":[{"sysfs_name":"1-2"}]}}"""
  assert report_checks.compare_hwmon(candidate, [first, second], [first, second])?.exact
  let wrong_maximum = json.set(json.decode(candidate)?, ["sensors", "channels", 1, "maximum"], 90000)?
  let mismatch = report_checks.compare_hwmon(json.encode(wrong_maximum)?, [first, second], [first, second])?
  assert mismatch.field_mismatches == ["hwmon1:temp1.maximum"]
  let missing_parent = json.set(json.decode(candidate)?, ["sensors", "channels", 1, "parent_usb_device_index"], null)?
  let parent_mismatch = report_checks.compare_hwmon(json.encode(missing_parent)?, [first, second], [first, second])?
  assert parent_mismatch.field_mismatches == ["hwmon1:temp1.usb_parent"]
  let unavailable_inventory = json.set(
    missing_parent,
    ["usb", "status"],
    {state: "absent", enumeration_succeeded: false},
  )?
  let partial_parent = report_checks.compare_hwmon(
    json.encode(unavailable_inventory)?,
    [first, second],
    [first, second],
  )?
  assert partial_parent.field_mismatches == []
  assert "hwmon1:temp1.usb_parent" in partial_parent.unstable_fields
  let changed = report_checks.compare_hwmon(
    candidate,
    [first, second],
    [first, {...second, value: {value: 44000, complete: true}}],
  )?
  assert "hwmon1:temp1.value" in changed.unstable_fields
  let transient_input = json.set(json.decode(candidate)?, ["sensors", "channels", 1, "value"], 44000)?
  let transient = report_checks.compare_hwmon(json.encode(transient_input)?, [first, second], [first, second])?
  assert transient.field_mismatches == []
  assert "hwmon1:temp1.value" in transient.unstable_fields
}

test test_system_report_hwmon_rooted_reference_reads_channel_attributes {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/hwmon/hwmon0", parents: true)?
  root.write(
    p"sys/class/hwmon/hwmon0/name",
    """fixture
""",
  )?
  root.write(
    p"sys/class/hwmon/hwmon0/temp1_input",
    """-5000
""",
  )?
  root.write(
    p"sys/class/hwmon/hwmon0/temp1_max",
    """100000
""",
  )?
  root.write(
    p"sys/class/hwmon/hwmon0/temp1_alarm",
    """1
""",
  )?
  let channels = report_checks.read_hwmon_reference(root)?
  assert channels.len() == 1
  assert channels[0].chip_entry_name == "hwmon0"
  assert channels[0].value == {value: -5000, complete: true}
  assert channels[0].maximum == {value: 100000, complete: true}
  assert channels[0].alarm == {value: 1, complete: true}
}

test test_system_report_smbios_raw_reference_scores_records_fields_and_strings {
  let table = b"\x01\x084\x12\x01\x02\x03\0Vendor\0Model\0Version\0\0\x7f\x04\0\0\0\0"
  let reference = smbios_reference.parse_smbios_reference(table)?
  assert reference.records.len() == 2
  assert reference.records[0].handle == 4660
  assert reference.records[0].strings.len() == 3
  let candidate = """{"firmware":{"status":{"state":"complete","enumeration_succeeded":true},"source":"smbios","records":[{"record_type":1,"handle":4660,"formatted_length":8,"fields":[{"name":"manufacturer_index","value":1,"unit":"string_index"},{"name":"product_index","value":2,"unit":"string_index"},{"name":"version_index","value":3,"unit":"string_index"},{"name":"serial_index","value":0,"unit":"string_index"}],"strings":[{"state":"observed","value":"Vendor","raw_bytes_base64":null},{"state":"observed","value":"Model","raw_bytes_base64":null},{"state":"observed","value":"Version","raw_bytes_base64":null}]},{"record_type":127,"handle":0,"formatted_length":4,"fields":[],"strings":[]}]}}"""
  assert smbios_reference.compare_smbios(candidate, table, table)?.exact
  let wrong = candidate.replace("\"product_index\",\"value\":2", "\"product_index\",\"value\":1")
  let mismatch = smbios_reference.compare_smbios(wrong, table, table)?
  assert mismatch.field_mismatches == ["1:4660.field.product_index"]
  let unknown = smbios_reference.parse_smbios_reference(b"\x90\x06E#\xaa\xbb\0\0\x7f\x04\0\0\0\0")?
  assert unknown.records[0].record_type == 144
  assert unknown.records[0].fields == []
  let bad_index = smbios_reference.parse_smbios_reference(
    b"\x01\x084\x12\x04\x02\x03\0Vendor\0Model\0Version\0\0\x7f\x04\0\0\0\0",
  )?
  assert bad_index.invalid_indices == ["1:4660.manufacturer_index"]
  let short_size = bytes.concat(
    [
      bytes.from_ints([17, 30, 1, 0])?,
      bytes.zero(8)?,
      bytes.from_ints([255, 127])?,
      bytes.zero(14)?,
      bytes.from_ints([9, 8, 0, 0, 127, 4, 0, 0, 0, 0])?,
    ],
  )
  let short_reference = smbios_reference.parse_smbios_reference(short_size)?
  assert ! (short_reference.records[0].fields |> any .name == "extended_size_raw")
}

test test_system_report_smbios_type16_reference_reads_short_form_device_count {
  let table = bytes.concat(
    [
      bytes.from_ints([16, 15, 1, 0])?,
      bytes.zero(9)?,
      bytes.from_ints([2, 0, 0, 0])?,
      bytes.from_ints([127, 4, 0, 0, 0, 0])?,
    ],
  )
  let parsed = smbios_reference.parse_smbios_reference(table)?
  assert parsed.complete
  let count = (parsed.records[0].fields
    |> where .name == "number_of_devices"
    |> first())?
  assert count.value == 2
  assert count.unit == "count"
}

test test_system_report_dmidecode_dump_relocates_smbios3_entry_point {
  let entry = bytes.from_ints([95, 83, 77, 51, 95, 59, 24, 3, 2, 0, 1, 0, 6, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0])?
  let table = b"\x7f\x04\0\0\0\0"
  let dump = smbios_reference.craft_dmidecode_dump(entry, table)?
  assert dump.len() == 38
  assert dump[16..24] == bytes.from_ints([32, 0, 0, 0, 0, 0, 0, 0])?
  assert (dump.byte_at(5) ?? -1) == 43
  assert dump[32..38] == table
  var checksum = 0
  for index in range(24) {
    checksum += dump.byte_at(index) ?? -1
  }

  assert checksum % 256 == 0
  assert (entry.byte_at(17) ?? -1) == 16
  test.error_kind(
    smbios_reference.craft_dmidecode_dump(
      bytes.from_ints([95, 83, 77, 51, 95, 60, 24, 3, 2, 0, 1, 0, 6, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0])?,
      table,
    ),
    "SmbiosCheckError.Invalid",
  )?
  test.error_kind(smbios_reference.craft_dmidecode_dump(entry, bytes.concat([table, b"x"])), "SmbiosCheckError.Invalid")?
}

test test_system_report_dmidecode_dump_relocates_smbios2_entry_point {
  let entry = bytes.from_ints(
    [
      95,
      83,
      77,
      95,
      121,
      31,
      2,
      8,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      95,
      68,
      77,
      73,
      95,
      41,
      6,
      0,
      0,
      16,
      0,
      0,
      1,
      0,
      40,
    ],
  )?
  let table = b"\x7f\x04\0\0\0\0"
  let dump = smbios_reference.craft_dmidecode_dump(entry, table)?
  assert dump.len() == 38
  assert dump[24..28] == bytes.from_ints([32, 0, 0, 0])?
  assert (dump.byte_at(21) ?? -1) == 25
  assert dump[32..38] == table
  var primary_checksum = 0
  for index in range(31) {
    primary_checksum += dump.byte_at(index) ?? -1
  }

  assert primary_checksum % 256 == 0
  var dmi_checksum = 0
  for index in range(16, 31) {
    dmi_checksum += dump.byte_at(index) ?? -1
  }

  assert dmi_checksum % 256 == 0
}

test test_system_report_dmidecode_hex_output_corroborates_raw_records_and_strings {
  let table = b"\x01\x084\x12\x01\x02\x03\0Vendor\0Model\0Version\0\0\x7f\x04\0\0\0\0"
  let raw = smbios_reference.parse_smbios_reference(table)?
  let output = """# dmidecode 3.7
SMBIOS 3.2.0 present.

Handle 0x1234, DMI type 1, 8 bytes
System Information
  Header and Data:
    01 08 34 12 01 02 03 00
  Strings:
    56 65 6E 64 6F 72 00
    Vendor
    4D 6F 64 65 6C 00
    Model
    56 65 72 73 69 6F 6E 00
    Version

Handle 0x0000, DMI type 127, 4 bytes
End Of Table
  Header and Data:
    7F 04 00 00
"""
  let parsed = smbios_reference.parse_dmidecode_hex_output(output)?
  assert parsed.len() == 2
  assert parsed[0].record_type == 1
  assert parsed[0].formatted == b"\x01\x084\x12\x01\x02\x03\0"
  assert parsed[0].strings == [b"Vendor", b"Model", b"Version"]
  assert smbios_reference.compare_dmidecode_hex_output(raw, output)?.exact
  assert smbios_reference.compare_dmidecode_hex_output(
    raw,
    output.replace(
  """    Vendor
""",
  """    Handle 0xDEAD
""",
),
  )?.exact
  let wrong_field = output.replace("01 08 34 12 01 02 03 00", "01 08 34 12 01 01 03 00")
  assert smbios_reference.compare_dmidecode_hex_output(raw, wrong_field)?.field_mismatches == [
    "1:4660.field.product_index",
  ]
  let wrong_string = output.replace("4D 6F 64 65 6C 00", "4D 6F 64 65 58 00")
  assert smbios_reference.compare_dmidecode_hex_output(raw, wrong_string)?.field_mismatches == ["1:4660.string.2"]
  test.error_kind(
    smbios_reference.parse_dmidecode_hex_output(output.replace("01 08 34 12 01 02 03 00", "GG 08 34 12 01 02 03 00")),
    "SmbiosCheckError.Invalid",
  )?
}

test test_system_report_smbios_rooted_reference_reads_only_exported_table {
  let root = fs.tempdir()?
  defer root.close()?
  let absent = smbios_reference.read_smbios_reference(root)?
  assert absent.absent
  let table = b"\x90\x06E#\xaa\xbb\0\0\x7f\x04\0\0\0\0"
  root.mkdir(p"sys/firmware/dmi/tables", parents: true)?
  root.write(p"sys/firmware/dmi/tables/DMI", table)?
  let observed = smbios_reference.read_smbios_reference(root)?
  assert observed.complete
  assert observed.data == table
  assert smbios_reference.parse_smbios_reference(observed.data ?? b"")?.records[0].record_type == 144
}

test test_system_report_smbios_capture_validates_saved_raw_table_and_oracle {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let table = b"\x10\x0f\x01\0\0\0\0\0\0\0\0\0\0\x02\0\0\0\x7f\x04\0\0\0\0"
  source.mkdir(p"sys/firmware/dmi/tables", parents: true)?
  source.write(p"sys/firmware/dmi/tables/DMI", table)?
  source.write(p"sys/firmware/dmi/tables/smbios_entry_point", b"_SM_\x1f\0")?
  let captured = smbios_reference.capture_smbios_bundle(source, bundle, "synthetic_fixture")?
  assert captured.stable
  assert captured.scoreable
  assert smbios_reference.validate_smbios_bundle(bundle)?.records[0].fields[1].value == 2
  let metadata = bundle.read_text(p"capture.json")?
  let contradictory = json.set(json.decode(metadata)?, ["source", "error_kind"], "permission_denied")?
  bundle.write_atomic(p"capture.json", json.encode(contradictory)?)?
  test.error_kind(smbios_reference.validate_smbios_bundle(bundle), "SmbiosCheckError.Invalid")?
  bundle.write_atomic(p"capture.json", metadata)?
  let bad_entry = json.set(json.decode(metadata)?, ["entry_point", "errno"], 13)?
  bundle.write_atomic(p"capture.json", json.encode(bad_entry)?)?
  test.error_kind(smbios_reference.validate_smbios_bundle(bundle), "SmbiosCheckError.Invalid")?
  bundle.write_atomic(p"capture.json", metadata)?
  assert bundle.read_result(p"sys/firmware/dmi/tables/smbios_entry_point")?.data == b"_SM_\x1f\0"
  source.write(p"sys/firmware/dmi/tables/DMI", b"changed")?
  assert smbios_reference.validate_smbios_bundle(bundle)?.records.len() == 2
  bundle.write(p"sys/firmware/dmi/tables/DMI", b"changed")?
  test.error_kind(smbios_reference.validate_smbios_bundle(bundle), "SmbiosCheckError.Invalid")?
  bundle.write(p"sys/firmware/dmi/tables/DMI", table)?
  bundle.write(p"sys/firmware/dmi/tables/smbios_entry_point", b"changed")?
  test.error_kind(smbios_reference.validate_smbios_bundle(bundle), "SmbiosCheckError.Invalid")?
  test.error_kind(
    smbios_reference.capture_smbios_bundle(source, bundle, "synthetic_fixture"),
    "SmbiosCheckError.Invalid",
  )?
}

test test_system_report_smbios_capture_scores_table_without_optional_entry_point {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/firmware/dmi/tables", parents: true)?
  source.write(p"sys/firmware/dmi/tables/DMI", b"\x7f\x04\0\0\0\0")?
  assert smbios_reference.capture_smbios_bundle(source, bundle, "synthetic_fixture")?.scoreable
  assert smbios_reference.validate_smbios_bundle(bundle)?.records.len() == 1
  assert "\"path\": \"sys/firmware/dmi/tables/smbios_entry_point\"" in bundle.read_text(p"capture.json")?
}

test test_system_report_smbios_capture_preserves_absent_source_without_scoring {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let captured = smbios_reference.capture_smbios_bundle(source, bundle, "synthetic_fixture")?
  assert captured.stable
  assert captured.scoreable == false
  assert "\"state\": \"absent\"" in bundle.read_text(p"capture.json")?
  test.error_kind(smbios_reference.validate_smbios_bundle(bundle), "SmbiosCheckError.Invalid")?
}

test test_system_report_smbios_capture_replays_production_collector_from_raw_table {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/firmware/dmi/tables", parents: true)?
  source.write(p"sys/firmware/dmi/tables/DMI", b"\x90\x06E#\xaa\xbb\0\0\x7f\x04\0\0\0\0")?
  let _ = smbios_reference.capture_smbios_bundle(source, bundle, "synthetic_fixture")?
  assert smbios_reference.replay_smbios_bundle(bundle)?.exact
}

test test_system_report_dmidecode_corroboration_records_opt_in_utility_provenance {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let tool_root = fs.tempdir()?
  defer tool_root.close()?
  let entry = bytes.from_ints([95, 83, 77, 51, 95, 59, 24, 3, 2, 0, 1, 0, 6, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0])?
  source.mkdir(p"sys/firmware/dmi/tables", parents: true)?
  source.write(p"sys/firmware/dmi/tables/DMI", b"\x7f\x04\0\0\0\0")?
  source.write(p"sys/firmware/dmi/tables/smbios_entry_point", entry)?
  let _ = smbios_reference.capture_smbios_bundle(source, bundle, "synthetic_fixture")?
  tool_root.write(
    p"dmidecode",
    """#!/bin/sh
if [ "$1" = "--version" ]; then
  printf 'dmidecode 3.7\n'
  exit 0
fi
if [ "$1" != "--no-quirks" ] || [ "$2" != "--dump" ] || [ "$3" != "--from-dump" ] || [ ! -f "$4" ]; then
  exit 3
fi
printf 'Handle 0x0000, DMI type 127, 4 bytes\nEnd Of Table\n  Header and Data:\n    7F 04 00 00\n'
""",
  )?
  tool_root.chmod(p"dmidecode", 0o700)?
  let tool_path = tool_root.host_path()?
  let result = smbios_reference.corroborate_smbios_bundle(bundle, fp"${tool_path}/dmidecode".display())?
  assert result.comparison.exact
  assert result.comparison.reference_count == 1
  let metadata = bundle.read_text(p"dmidecode-reference.json")?
  assert "\"reference_adapter\": \"dmidecode-hex-v1\"" in metadata
  assert "\"source_mode\": \"captured_replay\"" in metadata
  assert "\"origin\": \"synthetic_fixture\"" in metadata
  assert "\"exit_status\": 0" in metadata
  assert "Handle 0x0000" in bundle.read_text(p"dmidecode-output.txt")?
  test.error_kind(
    smbios_reference.corroborate_smbios_bundle(bundle, fp"${tool_path}/dmidecode".display()),
    "SmbiosCheckError.Invalid",
  )?
}

test test_system_report_dmidecode_corroboration_rejects_changed_capture_origin {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let tool_root = fs.tempdir()?
  defer tool_root.close()?
  let entry = bytes.from_ints([95, 83, 77, 51, 95, 59, 24, 3, 2, 0, 1, 0, 6, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0])?
  source.mkdir(p"sys/firmware/dmi/tables", parents: true)?
  source.write(p"sys/firmware/dmi/tables/DMI", b"\x7f\x04\0\0\0\0")?
  source.write(p"sys/firmware/dmi/tables/smbios_entry_point", entry)?
  let _ = smbios_reference.capture_smbios_bundle(source, bundle, "synthetic_fixture")?
  let metadata = json.decode(bundle.read_text(p"capture.json")?)?
  let changed = json.set(metadata, ["origin"], "live_capture")?
  tool_root.write_atomic(p"changed-capture.json", json.encode(changed)?)?
  let bundle_path = bundle.host_path()?
  let tool_path = tool_root.host_path()?
  tool_root.write(
    p"dmidecode",
    f"""#!/bin/sh
if [ "$1" = "--version" ]; then
  printf 'dmidecode 3.7\n'
  exit 0
fi
/bin/cp "${tool_path}/changed-capture.json" "${bundle_path}/capture.json"
printf 'Handle 0x0000, DMI type 127, 4 bytes\nEnd Of Table\n  Header and Data:\n    7F 04 00 00\n'
""",
  )?
  tool_root.chmod(p"dmidecode", 0o700)?
  test.error_kind(
    smbios_reference.corroborate_smbios_bundle(bundle, fp"${tool_path}/dmidecode".display()),
    "SmbiosCheckError.Invalid",
  )?
  assert ! bundle.exists(p"dmidecode-comparison.json")?
}

test test_system_report_dmidecode_version_failure_keeps_reference_provenance {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let tool_root = fs.tempdir()?
  defer tool_root.close()?
  let entry = bytes.from_ints([95, 83, 77, 51, 95, 59, 24, 3, 2, 0, 1, 0, 6, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0])?
  source.mkdir(p"sys/firmware/dmi/tables", parents: true)?
  source.write(p"sys/firmware/dmi/tables/DMI", b"\x7f\x04\0\0\0\0")?
  source.write(p"sys/firmware/dmi/tables/smbios_entry_point", entry)?
  let _ = smbios_reference.capture_smbios_bundle(source, bundle, "synthetic_fixture")?
  tool_root.write(
    p"dmidecode",
    """#!/bin/sh
printf 'unsupported version probe\n' >&2
exit 2
""",
  )?
  tool_root.chmod(p"dmidecode", 0o700)?
  let tool_path = tool_root.host_path()?
  test.error_kind(
    smbios_reference.corroborate_smbios_bundle(bundle, fp"${tool_path}/dmidecode".display()),
    "SmbiosCheckError.Invalid",
  )?
  assert "\"version_exit_status\": 2" in bundle.read_text(p"dmidecode-probe.json")?
  assert "unsupported version probe" in bundle.read_text(p"dmidecode-version-error.txt")?
  assert ! bundle.exists(p"dmidecode-comparison.json")?
}

test test_system_report_dmidecode_cli_runs_only_on_explicit_captured_bundle { |ctx|
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let tool_root = fs.tempdir()?
  defer tool_root.close()?
  let entry = bytes.from_ints([95, 83, 77, 51, 95, 59, 24, 3, 2, 0, 1, 0, 6, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0])?
  source.mkdir(p"sys/firmware/dmi/tables", parents: true)?
  source.write(p"sys/firmware/dmi/tables/DMI", b"\x7f\x04\0\0\0\0")?
  source.write(p"sys/firmware/dmi/tables/smbios_entry_point", entry)?
  let _ = smbios_reference.capture_smbios_bundle(source, bundle, "synthetic_fixture")?
  tool_root.write(
    p"dmidecode",
    """#!/bin/sh
if [ "$1" = "--version" ]; then
  printf 'dmidecode 3.7\n'
  exit 0
fi
printf 'Handle 0x0000, DMI type 127, 4 bytes\nEnd Of Table\n  Header and Data:\n    7F 04 00 00\n'
""",
  )?
  tool_root.chmod(p"dmidecode", 0o700)?
  let bundle_path = bundle.host_path()?
  let tool_root_path = tool_root.host_path()?
  let executable = fp"${tool_root_path}/dmidecode"
  let output = test.temp_path(ctx, name: "system-report-dmidecode.stdout")
  let stderr = test.temp_path(ctx, name: "system-report-dmidecode.stderr")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --corroborate-smbios-bundle $bundle_path --dmidecode-bin $executable > $output 2> $stderr
  let exited_successfully = status.exited_with(0)
  let diagnostic = stderr.read_text()?
  assert exited_successfully, diagnostic
  assert "firmware.dmidecode:" in output.read_text()?
  assert "\"exact\": true" in bundle.read_text(p"dmidecode-comparison.json")?
}

test test_system_report_cpufreq_rooted_reference_reads_every_policy {
  let root = fs.tempdir()?
  defer root.close()?
  for policy_name in ["policy3", "policy9"] {
    let base = fp"sys/devices/system/cpu/cpufreq/${policy_name}"
    root.mkdir(base, parents: true)?
    root.write(
      fp"${base}/related_cpus",
      """0 2
""",
    )?
    root.write(
      fp"${base}/affected_cpus",
      """0
""",
    )?
    root.write(
      fp"${base}/scaling_driver",
      """fixture-driver
""",
    )?
    root.write(
      fp"${base}/scaling_governor",
      if policy_name == "policy9" {
  """userspace
"""
} else {
  """powersave
"""
},
    )?
    root.write(
      fp"${base}/cpuinfo_min_freq",
      """800000
""",
    )?
    root.write(
      fp"${base}/cpuinfo_max_freq",
      """4000000
""",
    )?
    root.write(
      fp"${base}/scaling_min_freq",
      """1000000
""",
    )?
    root.write(
      fp"${base}/scaling_max_freq",
      """3000000
""",
    )?
    root.write(
      fp"${base}/cpuinfo_cur_freq",
      """1800000
""",
    )?
    root.write(
      fp"${base}/scaling_cur_freq",
      """1700000
""",
    )?
    if policy_name == "policy9" {
      root.write(
        fp"${base}/scaling_setspeed",
        """1900000
""",
      )?
    }

    root.write(
      fp"${base}/energy_performance_preference",
      """balance_performance
""",
    )?
    root.write(
      fp"${base}/energy_performance_available_preferences",
      """performance balance_performance
""",
    )?
  }

  root.write(
    p"sys/devices/system/cpu/cpufreq/boost",
    """1
""",
  )?
  assert root.children(p"sys/devices/system/cpu/cpufreq", max_entries: 1024)?.state == "complete"
  let reference = report_checks.read_cpufreq_policy_reference(root)?
  assert reference.len() == 2
  assert reference[0].name == "policy3"
  assert reference[1].name == "policy9"
  assert reference[0].related_cpus == [0, 2]
  assert reference[1].hardware_max_khz == 4000000
  assert reference[0].hardware_current == {value: 1800000, complete: true}
  assert reference[1].scaling_current == {value: 1700000, complete: true}
  assert reference[0].average_current == {value: null, complete: true}
  assert reference[0].governor_requested == {value: null, complete: true}
  assert reference[1].governor_requested == {value: 1900000, complete: true}
  assert reference[0].energy_performance_preference == "balance_performance"
  assert reference[0].available_energy_performance_preferences == ["performance", "balance_performance"]
  assert reference[1].boost_allowed == true
  assert reference[1].boost_scope == "system"
  root.remove(p"sys/devices/system/cpu/cpufreq/boost")?
  root.mkdir(p"sys/devices/system/cpu/intel_pstate", parents: true)?
  root.write(
    p"sys/devices/system/cpu/intel_pstate/no_turbo",
    """1
""",
  )?
  let intel_reference = report_checks.read_cpufreq_policy_reference(root)?
  assert intel_reference[0].boost_allowed == false
  assert intel_reference[0].boost_scope == "intel_pstate"
  root.write(
    p"sys/devices/system/cpu/intel_pstate/no_turbo",
    """invalid
""",
  )?
  test.error_kind(report_checks.read_cpufreq_policy_reference(root), "SystemReportCheckError.Invalid")?
  root.write(
    p"sys/devices/system/cpu/intel_pstate/no_turbo",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpufreq/policy9/scaling_max_freq",
    """invalid
""",
  )?
  test.error_kind(report_checks.read_cpufreq_policy_reference(root), "SystemReportCheckError.Invalid")?
}

test test_system_report_cpufreq_capture_replays_raw_policies_and_rejects_tampering {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let cpu_root = p"sys/devices/system/cpu"
  let policy = p"sys/devices/system/cpu/cpufreq/policy0"
  source.mkdir(policy, parents: true)?
  for item in [
    {
      name: "possible",
      value: """0
""",
    },
    {
      name: "present",
      value: """0
""",
    },
    {
      name: "online",
      value: """0
""",
    },
    {
      name: "offline",
      value: "\n",
    },
  ] {
    source.write(fp"${cpu_root}/${item.name}", item.value)?
  }

  for item in [
    {
      name: "related_cpus",
      value: """0
""",
    },
    {
      name: "affected_cpus",
      value: """0
""",
    },
    {
      name: "scaling_driver",
      value: """fixture-driver
""",
    },
    {
      name: "scaling_governor",
      value: """powersave
""",
    },
    {
      name: "cpuinfo_min_freq",
      value: """800000
""",
    },
    {
      name: "cpuinfo_max_freq",
      value: """4000000
""",
    },
    {
      name: "scaling_min_freq",
      value: """1000000
""",
    },
    {
      name: "scaling_max_freq",
      value: """3000000
""",
    },
    {
      name: "cpuinfo_cur_freq",
      value: """1800000
""",
    },
    {
      name: "scaling_cur_freq",
      value: """1700000
""",
    },
    {
      name: "energy_performance_preference",
      value: """balance_performance
""",
    },
    {
      name: "energy_performance_available_preferences",
      value: """performance balance_performance
""",
    },
  ] {
    source.write(fp"${policy}/${item.name}", item.value)?
  }

  source.write(
    fp"${cpu_root}/cpufreq/boost",
    """1
""",
  )?
  report_checks.capture_cpufreq_bundle(source, bundle, "synthetic_fixture")?
  let replay = report_checks.replay_cpufreq_bundle(bundle)?
  assert replay.sets.exact
  assert replay.policies.exact_policies
  assert replay.policies.exact_bounds
  assert replay.policies.exact_controls
  bundle.write(
    fp"${policy}/related_cpus",
    """1
""",
  )?
  test.error_kind(report_checks.validate_cpufreq_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(
    fp"${policy}/related_cpus",
    """0
""",
  )?
  bundle.write(
    fp"${cpu_root}/intel_pstate/no_turbo",
    """1
""",
  )?
  test.error_kind(report_checks.validate_cpufreq_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_cpufreq_capture_rejects_noncanonical_policy_directory {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/devices/system/cpu/cpufreq/policy01", parents: true)?
  test.error_kind(
    report_checks.capture_cpufreq_bundle(source, bundle, "synthetic_fixture"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_cpu_topology_capture_replays_raw_siblings_and_nodes {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let cpu_root = p"sys/devices/system/cpu"
  source.mkdir(cpu_root, parents: true)?
  for item in [
    {
      name: "possible",
      value: """0-1
""",
    },
    {
      name: "present",
      value: """0-1
""",
    },
    {
      name: "online",
      value: """0-1
""",
    },
    {
      name: "offline",
      value: "\n",
    },
  ] {
    source.write(fp"${cpu_root}/${item.name}", item.value)?
  }

  for id in [0, 1] {
    let cpu_path = fp"${cpu_root}/cpu${id}"
    source.mkdir(fp"${cpu_path}/topology", parents: true)?
    for item in [
      {
        name: "physical_package_id",
        value: """0
""",
      },
      {
        name: "die_id",
        value: """0
""",
      },
      {
        name: "core_id",
        value: """0
""",
      },
      {
        name: "thread_siblings_list",
        value: """0-1
""",
      },
    ] {
      source.write(fp"${cpu_path}/topology/${item.name}", item.value)?
    }

    source.symlink(../../node/node0, fp"${cpu_path}/node0")?
  }

  report_checks.capture_cpu_topology_bundle(source, bundle, "synthetic_fixture")?
  let replay = report_checks.replay_cpu_topology_bundle(bundle)?
  assert replay.sets.exact
  assert replay.topology.exact
  assert replay.topology.matched_count == 2
  bundle.write(
    fp"${cpu_root}/cpu1/topology/core_id",
    """1
""",
  )?
  test.error_kind(report_checks.validate_cpu_topology_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(
    fp"${cpu_root}/cpu1/topology/core_id",
    """0
""",
  )?
  bundle.remove(fp"${cpu_root}/cpu1/node0")?
  test.error_kind(report_checks.validate_cpu_topology_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_cpu_topology_capture_rejects_escaping_numa_link {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let cpu_root = p"sys/devices/system/cpu"
  source.mkdir(fp"${cpu_root}/cpu0/topology", parents: true)?
  for item in [
    {
      name: "possible",
      value: """0
""",
    },
    {
      name: "present",
      value: """0
""",
    },
    {
      name: "online",
      value: """0
""",
    },
    {
      name: "offline",
      value: "\n",
    },
  ] {
    source.write(fp"${cpu_root}/${item.name}", item.value)?
  }

  source.symlink(../../node/../rogue, fp"${cpu_root}/cpu0/node0")?
  test.error_kind(
    report_checks.capture_cpu_topology_bundle(source, bundle, "synthetic_fixture"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_cpuidle_reference_scores_state_indices_and_bracketed_counters {
  let shallow = report_checks.CpuIdleStateReference(
    cpu_id: 0,
    state_index: 0,
    name: "C1",
    description: "first",
    disable_setting: 0,
    latency_us: 1,
    residency_us: 2,
    usage_count: 3,
    time_us: 10,
  )
  let deep = {
    ...shallow,
    state_index: 1,
    description: "second",
    latency_us: 8,
    residency_us: 20,
    usage_count: 6,
    time_us: 40,
  }
  let before = report_checks.CpuIdleReference(
    driver: "intel_idle",
    governor: "menu",
    available_governors: ["menu", "teo"],
    states: [shallow, deep],
  )
  let after = report_checks.CpuIdleReference(
    driver: before.driver,
    governor: before.governor,
    available_governors: before.available_governors,
    states: [{...shallow, usage_count: 5, time_us: 30}, {...deep, usage_count: 8, time_us: 60}],
  )
  let candidate = """{"cpu":{"status":{"state":"complete","enumeration_succeeded":true},"global_idle_driver":"intel_idle","global_idle_governor":"menu","available_idle_governors":["menu","teo"],"idle_states":[{"cpu_id":0,"state_index":1,"name":"C1","description":"second","disable_setting":0,"latency_us":8,"residency_us":20,"usage_count":7,"time_us":50},{"cpu_id":0,"state_index":0,"name":"C1","description":"first","disable_setting":0,"latency_us":1,"residency_us":2,"usage_count":4,"time_us":20}]}}"""
  let exact = report_checks.compare_cpuidle(candidate, before, after)?
  assert exact.eligible
  assert exact.exact
  assert exact.matched_count == 2
  let bad_disable = report_checks.compare_cpuidle(
    candidate.replace(
      "\"state_index\":0,\"name\":\"C1\",\"description\":\"first\",\"disable_setting\":0",
      "\"state_index\":0,\"name\":\"C1\",\"description\":\"first\",\"disable_setting\":1",
    ),
    before,
    after,
  )?
  assert bad_disable.field_mismatches == ["0:0.disable_setting"]
  let bad_counter = report_checks.compare_cpuidle(
    candidate.replace("\"usage_count\":4", "\"usage_count\":9"),
    before,
    after,
  )?
  assert bad_counter.counter_mismatches == ["0:0.usage_count"]
  let unstable = report_checks.compare_cpuidle(candidate, before, {...after, governor: "teo"})?
  assert unstable.unstable_fields == ["global_idle_governor"]
  assert ! unstable.exact
  let missing = report_checks.compare_cpuidle(
    candidate.replace(
      ",{\"cpu_id\":0,\"state_index\":0,\"name\":\"C1\",\"description\":\"first\",\"disable_setting\":0,\"latency_us\":1,\"residency_us\":2,\"usage_count\":4,\"time_us\":20}",
      "",
    ),
    before,
    after,
  )?
  assert ! missing.exact
  let state_race = report_checks.compare_cpuidle(candidate, before, {...after, states: [after.states[0]]})?
  assert state_race.unstable_fields == ["0:1.presence"]
  assert ! state_race.exact
  let reset = report_checks.compare_cpuidle(
    candidate,
    before,
    {...after, states: [{...after.states[0], usage_count: 2}, after.states[1]]},
  )?
  assert reset.unstable_fields == ["0:0.usage_count"]
  assert ! reset.exact
  let absent = {driver: null, governor: null, available_governors: [], states: []}
  let absent_candidate = """{"cpu":{"status":{"state":"complete","enumeration_succeeded":true},"global_idle_driver":null,"global_idle_governor":null,"available_idle_governors":[],"idle_states":[]}}"""
  assert ! report_checks.compare_cpuidle(absent_candidate, absent, absent)?.eligible
}

test test_system_report_cpuidle_rooted_reference_reads_each_present_cpu_state {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu/cpuidle", parents: true)?
  root.write(
    p"sys/devices/system/cpu/present",
    """0,2
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpuidle/current_driver",
    """intel_idle
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpuidle/current_governor_ro",
    """menu
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpuidle/available_governors",
    """menu teo
""",
  )?
  for cpu_id in [0, 2] {
    for state_index in [0, 1] {
      let base = fp"sys/devices/system/cpu/cpu${cpu_id}/cpuidle/state${state_index}"
      root.mkdir(base, parents: true)?
      root.write(
        fp"${base}/name",
        """C1
""",
      )?
      root.write(
        fp"${base}/disable",
        """0
""",
      )?
      root.write(
        fp"${base}/latency",
        """8
""",
      )?
      root.write(
        fp"${base}/residency",
        """20
""",
      )?
      root.write(
        fp"${base}/usage",
        """12
""",
      )?
      root.write(
        fp"${base}/time",
        """40
""",
      )?
    }
  }

  let reference = report_checks.read_cpuidle_reference(root)?
  assert reference.governor == "menu"
  assert reference.available_governors == ["menu", "teo"]
  assert reference.states.len() == 4
  assert reference.states |> any .cpu_id == 2 and .state_index == 1 and .name == "C1"
  root.write(
    p"sys/devices/system/cpu/cpu2/cpuidle/state1/disable",
    """2
""",
  )?
  test.error_kind(report_checks.read_cpuidle_reference(root), "SystemReportCheckError.Invalid")?
  root.write(
    p"sys/devices/system/cpu/cpu2/cpuidle/state1/disable",
    """0
""",
  )?
  root.mkdir(p"sys/devices/system/cpu/cpu2/cpuidle/state9007199254740992")?
  test.error_kind(report_checks.read_cpuidle_reference(root), "SystemReportCheckError.Invalid")?
}

test test_system_report_capture_rejects_simultaneous_live_comparison { |ctx|
  let bundle = test.temp_path(ctx, name: "system-report-cpu-bundle")
  let stderr = test.temp_path(ctx, name: "system-report-cpu-bundle.stderr")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-cpu-bundle $bundle --compare-cpuidle 2> $stderr
  assert ! status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let usb_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-cpu-bundle $bundle --compare-usb-topology 2> $stderr
  assert ! usb_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let usb_ids_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-cpu-bundle $bundle --compare-usb-ids 2> $stderr
  assert ! usb_ids_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let usb_power_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-cpu-bundle $bundle --compare-usb-power 2> $stderr
  assert ! usb_power_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let smbios_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-smbios-bundle $bundle --compare-smbios 2> $stderr
  assert ! smbios_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let thermal_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-thermal-bundle $bundle --compare-thermal 2> $stderr
  assert ! thermal_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let hwmon_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-hwmon-bundle $bundle --compare-hwmon 2> $stderr
  assert ! hwmon_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let block_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-block-bundle $bundle --compare-storage 2> $stderr
  assert ! block_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let cgroup_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-cgroup2-bundle $bundle --compare-cgroup-v2 2> $stderr
  assert ! cgroup_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let process_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-process-bundle $bundle --compare-processes 2> $stderr
  assert ! process_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let supply_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-power-supply-bundle $bundle --compare-power-supplies 2> $stderr
  assert ! supply_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let identity_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-os-release-bundle $bundle --compare-identity 2> $stderr
  assert ! identity_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let uptime_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-uptime-bundle $bundle --compare-identity 2> $stderr
  assert ! uptime_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let dmi_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-dmi-identity-bundle $bundle --compare-identity 2> $stderr
  assert ! dmi_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let device_tree_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-device-tree-bundle $bundle --compare-identity 2> $stderr
  assert ! device_tree_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let command_line_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-kernel-command-line-bundle $bundle --compare-command-line 2> $stderr
  assert ! command_line_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let modules_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-kernel-modules-bundle $bundle --compare-modules 2> $stderr
  assert ! modules_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let swaps_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-swaps-bundle $bundle --compare-swaps 2> $stderr
  assert ! swaps_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let pressure_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-pressure-bundle $bundle --compare-pressure 2> $stderr
  assert ! pressure_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let mountinfo_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-mountinfo-bundle $bundle --compare-mounts 2> $stderr
  assert ! mountinfo_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let parameters_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --capture-kernel-parameters-bundle $bundle --compare-parameters 2> $stderr
  assert ! parameters_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  assert ! bundle.exists()?
  let utility_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --corroborate-smbios-bundle $bundle --dmidecode-bin /nonexistent --compare-smbios 2> $stderr
  assert ! utility_status.exited_with(0)
  assert "bundle operations cannot be combined" in stderr.read_text()?
  let missing_binary_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/dev/main.xsh" -- system-report-check --corroborate-smbios-bundle $bundle 2> $stderr
  assert ! missing_binary_status.exited_with(0)
  assert "requires --dmidecode-bin" in stderr.read_text()?
  assert ! bundle.exists()?
}

test test_system_report_lscpu_topology_scores_relationships_across_id_spaces {
  let output = """{"cpus":[{"cpu":0,"online":true,"socket":0,"core":20,"node":0},{"cpu":1,"online":true,"socket":0,"core":20,"node":0},{"cpu":2,"online":true,"socket":1,"core":21,"node":1}]}"""
  let reference = report_checks.parse_lscpu_topology(output)?
  let possible_extra = report_checks.parse_lscpu_topology(
    output.replace("]}", ",{\"cpu\":3,\"online\":false,\"socket\":1,\"core\":22,\"node\":1}]}"),
  )?
  assert report_checks.select_present_lscpu_topology(possible_extra, [0, 1, 2])? == reference
  test.error_kind(
    report_checks.select_present_lscpu_topology(reference, [0, 1, 2, 3]),
    "SystemReportCheckError.Invalid",
  )?
  let candidate = """{"cpu":{"status":{"state":"complete","enumeration_succeeded":true},"cpus":[{"id":2,"present":true,"online":true,"package_id":8,"core_id":6,"thread_siblings":[2],"numa_node":1},{"id":0,"present":true,"online":true,"package_id":7,"core_id":5,"thread_siblings":[0,1],"numa_node":0},{"id":1,"present":true,"online":true,"package_id":7,"core_id":5,"thread_siblings":[0,1],"numa_node":0}]}}"""
  let exact = report_checks.compare_lscpu_topology(candidate, reference, reference)?
  assert exact.exact
  assert exact.matched_count == 3
  let bad_siblings = report_checks.compare_lscpu_topology(
    candidate.replace("\"thread_siblings\":[0,1]", "\"thread_siblings\":[0]"),
    reference,
    reference,
  )?
  assert bad_siblings.sibling_mismatches == ["0.thread_siblings", "1.thread_siblings"]
  assert ! bad_siblings.exact
  let bad_package = report_checks.compare_lscpu_topology(
    candidate.replace(
      "\"package_id\":7,\"core_id\":5,\"thread_siblings\":[0,1],\"numa_node\":0}]",
      "\"package_id\":8,\"core_id\":5,\"thread_siblings\":[0,1],\"numa_node\":0}]",
    ),
    reference,
    reference,
  )?
  assert ! bad_package.exact
  assert bad_package.package_group_mismatches.len() > 0
  let missing_package = report_checks.compare_lscpu_topology(
    candidate.replace("\"package_id\":7", "\"package_id\":null"),
    reference,
    reference,
  )?
  assert ! missing_package.exact
  assert missing_package.field_missing.len() > 0
  let changed = report_checks.parse_lscpu_topology(output.replace("\"node\":1", "\"node\":0"))?
  test.error_kind(report_checks.compare_lscpu_topology(candidate, reference, changed), "SystemReportCheckError.Invalid")?
  let duplicate = """{"cpus":[{"cpu":0,"online":true,"socket":0,"core":0,"node":0},{"cpu":0,"online":true,"socket":0,"core":0,"node":0}]}"""
  test.error_kind(report_checks.parse_lscpu_topology(duplicate), "SystemReportCheckError.Invalid")?
}

test test_system_report_cache_reference_scores_unique_instances_and_cpu_links {
  let source0 = {
    owner_cpu_id: 0,
    sysfs_index: 7,
    kernel_id: 9,
    level: 2,
    kind: "Unified",
    size_bytes: 1048576,
    line_size_bytes: 64,
    sets: 16384,
    shared_cpus: [
      0,
      2,
    ],
  }
  let source2 = {...source0, owner_cpu_id: 2}
  let candidate = """{"cpu":{"status":{"state":"complete","enumeration_succeeded":true},"cpus":[{"id":0,"cache_ids":[0]},{"id":2,"cache_ids":[0]}],"caches":[{"id":0,"owner_cpu_id":0,"sysfs_index":7,"level":2,"kind":"Unified","size_bytes":1048576,"line_size_bytes":64,"sets":16384,"shared_cpus":[0,2]}]}}"""
  let exact = report_checks.compare_cpu_cache_sharing(candidate, [source0, source2], [source0, source2])?
  assert exact.exact
  assert exact.reference_count == 1
  assert exact.matched_count == 1
  let missing_link = report_checks.compare_cpu_cache_sharing(
    candidate.replace("\"id\":2,\"cache_ids\":[0]", "\"id\":2,\"cache_ids\":[]"),
    [source0, source2],
    [source0, source2],
  )?
  assert missing_link.relationship_mismatches == ["2.cache_ids"]
  assert ! missing_link.exact
  let wrong_size = report_checks.compare_cpu_cache_sharing(
    candidate.replace("\"size_bytes\":1048576", "\"size_bytes\":null"),
    [source0, source2],
    [source0, source2],
  )?
  assert wrong_size.field_mismatches == ["0:7.size_bytes"]
  assert ! wrong_size.exact
  let duplicate = candidate.replace(
    "}]}}",
    "},{\"id\":1,\"owner_cpu_id\":0,\"sysfs_index\":7,\"level\":2,\"kind\":\"Unified\",\"size_bytes\":1048576,\"line_size_bytes\":64,\"sets\":16384,\"shared_cpus\":[0,2]}]}}",
  )
  test.error_kind(
    report_checks.compare_cpu_cache_sharing(duplicate, [source0, source2], [source0, source2]),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.compare_cpu_cache_sharing(candidate, [source0], [source2]),
    "SystemReportCheckError.Invalid",
  )?
  let second0 = {...source0, sysfs_index: 8, kernel_id: 10}
  let second2 = {...source2, sysfs_index: 8, kernel_id: 10}
  let omitted = report_checks.compare_cpu_cache_sharing(
    candidate,
    [source0, source2, second0, second2],
    [source0, source2, second0, second2],
  )?
  assert omitted.reference_count == 2
  assert omitted.missing_keys == ["0:8"]
  assert ! omitted.exact
  let without_kernel_id = report_checks.compare_cpu_cache_sharing(
    candidate,
    [{...source0, kernel_id: null}],
    [{...source0, kernel_id: null}],
  )?
  assert ! without_kernel_id.eligible
  assert ! without_kernel_id.exact
}

test test_system_report_cache_reference_parses_sizes_without_candidate_rules {
  assert report_checks.parse_cpu_cache_size_reference("1M")? == 1048576
  assert report_checks.parse_cpu_cache_size_reference("48K")? == 49152
  test.error_kind(report_checks.parse_cpu_cache_size_reference("1T"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_cpu_cache_size_reference("8796093022208K"), "SystemReportCheckError.Invalid")?
}

test test_system_report_cache_rooted_reference_reads_shared_instance_sources {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu", parents: true)?
  root.write(
    p"sys/devices/system/cpu/present",
    """0,2
""",
  )?
  for cpu_id in [0, 2] {
    let base = fp"sys/devices/system/cpu/cpu${cpu_id}/cache/index7"
    root.mkdir(base, parents: true)?
    root.write(
      fp"${base}/level",
      """2
""",
    )?
    root.write(
      fp"${base}/type",
      """Unified
""",
    )?
    root.write(
      fp"${base}/size",
      """1M
""",
    )?
    root.write(
      fp"${base}/coherency_line_size",
      """64
""",
    )?
    root.write(
      fp"${base}/number_of_sets",
      """16384
""",
    )?
    root.write(
      fp"${base}/shared_cpu_list",
      """0,2
""",
    )?
    root.write(
      fp"${base}/id",
      """9
""",
    )?
  }

  let reference = report_checks.read_cpu_cache_reference(root)?
  assert reference.len() == 2
  assert reference[0].owner_cpu_id == 0
  assert reference[1].owner_cpu_id == 2
  assert reference[0].size_bytes == 1048576
  assert reference[0].shared_cpus == [0, 2]
  assert reference[0].kernel_id == 9
  root.write(
    p"sys/devices/system/cpu/cpu2/cache/index7/shared_cpu_list",
    """0,,2
""",
  )?
  test.error_kind(report_checks.read_cpu_cache_reference(root), "SystemReportCheckError.Invalid")?
}

test test_system_report_network_link_raw_reference_scores_flags_type_and_stable_counters {
  let reference = [
    report_checks.NetworkLinkRawReference(
      ifindex: 2,
      name: "eth0",
      hardware_type: 1,
      flags: 4099,
      rx_bytes: 100,
      tx_bytes: 200,
      complete: true,
    ),
  ]
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":2,"name":{"state":"observed","value":"eth0"},"hardware_type":1,"flags":["up","broadcast","multicast","raw_bits=4099"],"counters":[{"name":"rx_bytes","value":100,"unit":"bytes"},{"name":"tx_bytes","value":200,"unit":"bytes"}]}]}}"""
  let exact = report_checks.compare_network_link_raw(candidate, reference, reference)?
  assert exact.exact
  assert exact.matched_count == 1
  let operational_flags = candidate.replace(
    "\"multicast\",\"raw_bits=4099\"",
    "\"multicast\",\"running\",\"lower_up\",\"raw_bits=69699\"",
  )
  assert report_checks.compare_network_link_raw(operational_flags, reference, reference)?.exact
  let wrong_flags = report_checks.compare_network_link_raw(
    candidate.replace("\"multicast\",", ""),
    reference,
    reference,
  )?
  assert wrong_flags.field_mismatches == ["2.flags"]
  let wrong_type = report_checks.compare_network_link_raw(
    candidate.replace("\"hardware_type\":1", "\"hardware_type\":772"),
    reference,
    reference,
  )?
  assert wrong_type.field_mismatches == ["2.hardware_type"]
  let wrong_counter = report_checks.compare_network_link_raw(
    candidate.replace("\"value\":100", "\"value\":90"),
    reference,
    reference,
  )?
  assert wrong_counter.field_mismatches == ["2.rx_bytes"]
  let changed = report_checks.compare_network_link_raw(candidate, reference, [{...reference[0], rx_bytes: 120}])?
  assert changed.unstable_fields == []
  assert changed.exact
  let outside = report_checks.compare_network_link_raw(
    candidate.replace("\"value\":100", "\"value\":121"),
    reference,
    [{...reference[0], rx_bytes: 120}],
  )?
  assert outside.field_mismatches == ["2.rx_bytes"]
  let reset = report_checks.compare_network_link_raw(candidate, reference, [{...reference[0], rx_bytes: 90}])?
  assert reset.unstable_fields == ["2.rx_bytes"]
  let incomplete = report_checks.compare_network_link_raw(candidate, [{...reference[0], complete: false}], reference)?
  assert ! incomplete.exact
}

test test_system_report_network_link_raw_reference_reads_bounded_sysfs_attributes {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/net/eth0/statistics", parents: true)?
  root.write(
    p"sys/class/net/eth0/ifindex",
    """2
""",
  )?
  root.write(
    p"sys/class/net/eth0/type",
    """1
""",
  )?
  root.write(
    p"sys/class/net/eth0/flags",
    """0x1003
""",
  )?
  root.write(
    p"sys/class/net/eth0/statistics/rx_bytes",
    """100
""",
  )?
  root.write(
    p"sys/class/net/eth0/statistics/tx_bytes",
    """200
""",
  )?
  let reference = report_checks.read_network_link_raw_reference(root)?
  assert reference.len() == 1
  assert reference[0].ifindex == 2
  assert reference[0].hardware_type == 1
  assert reference[0].flags == 4099
  assert reference[0].rx_bytes == 100
  assert reference[0].complete
  root.write(
    p"sys/class/net/eth0/statistics/tx_bytes",
    """invalid
""",
  )?
  let incomplete = report_checks.read_network_link_raw_reference(root)?
  assert ! incomplete[0].complete
  assert incomplete[0].tx_bytes == null
  root.write(
    p"sys/class/net/eth0/statistics/tx_bytes",
    """200
""",
  )?
  root.write(
    p"sys/class/net/eth0/flags",
    """1003
""",
  )?
  let malformed_flags = report_checks.read_network_link_raw_reference(root)?
  assert ! malformed_flags[0].complete
  assert malformed_flags[0].flags == null
}

test test_system_report_ip_link_reference_scores_stable_identity_and_state {
  let ip_output = """[{"ifindex":2,"ifname":"eth0","flags":["BROADCAST","MULTICAST","UP","LOWER_UP"],"mtu":1500,"operstate":"UP","link_type":"ether"},{"ifindex":1,"ifname":"lo","flags":["LOOPBACK","UP"],"mtu":65536,"operstate":"UNKNOWN","link_type":"loopback"}]"""
  let reference = report_checks.parse_ip_link_json(ip_output)?
  assert reference.len() == 2
  assert reference[0].ifindex == 2
  assert reference[0].name == "eth0"
  assert reference[0].admin_up == true
  assert reference[0].operstate == "up"
  assert report_checks.ip_link_reference_stable(reference, [reference[1], reference[0]])
  assert ! report_checks.ip_link_reference_stable(reference, [{...reference[0], mtu: 1400}, reference[1]])
  let reordered_candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":1,"name":{"state":"observed","value":"lo"},"mtu":65536,"admin_up":true,"operational_state":"unknown","kind":null,"master_ifindex":null,"lower_ifindex":null},{"ifindex":2,"name":{"state":"observed","value":"eth0"},"mtu":1500,"admin_up":true,"operational_state":"up","kind":null,"master_ifindex":null,"lower_ifindex":null}]}}"""
  let exact = report_checks.compare_ip_links(reordered_candidate, reference)?
  assert exact.exact
  assert exact.matched_count == 2
  let partial = report_checks.compare_ip_links(reordered_candidate.replace("\"complete\"", "\"partial\""), reference)?
  assert ! partial.candidate_field_missing
  assert partial.exact
  let incomplete = report_checks.compare_ip_links(
    reordered_candidate.replace("\"enumeration_succeeded\":true", "\"enumeration_succeeded\":false"),
    reference,
  )?
  assert incomplete.candidate_field_missing
  assert ! incomplete.exact
  let changed_candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":1,"name":{"state":"observed","value":"lo"},"mtu":65536,"admin_up":true,"operational_state":"unknown","kind":null,"master_ifindex":null,"lower_ifindex":null},{"ifindex":3,"name":{"state":"observed","value":"eth0"},"mtu":1400,"admin_up":false,"operational_state":"down","kind":null,"master_ifindex":null,"lower_ifindex":null}]}}"""
  let changed = report_checks.compare_ip_links(changed_candidate, reference)?
  assert ! changed.exact
  assert changed.missing_ids == [2]
  assert changed.unexpected_ids == [3]
  let field_changed_candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":2,"name":{"state":"observed","value":"eth0"},"mtu":1400,"admin_up":false,"operational_state":"down","kind":null,"master_ifindex":null,"lower_ifindex":null},{"ifindex":1,"name":{"state":"observed","value":"lo"},"mtu":65536,"admin_up":true,"operational_state":"unknown","kind":null,"master_ifindex":null,"lower_ifindex":null}]}}"""
  let fields = report_checks.compare_ip_links(field_changed_candidate, reference)?
  assert fields.field_mismatches == ["2.mtu", "2.admin_up", "2.operational_state"]
  assert ! fields.exact
  let unknown_state = """[{"ifindex":7,"ifname":"vlan7","flags":["UP"],"mtu":1500,"operstate_index":77}]"""
  assert report_checks.parse_ip_link_json(unknown_state)?[0].operstate == "operstate_77"
  let malformed = """[{"ifindex":2,"ifname":"eth0","flags":["UP"],"mtu":1500},{"ifindex":2,"ifname":"eth1","flags":[],"mtu":1500}]"""
  test.error_kind(report_checks.parse_ip_link_json(malformed), "SystemReportCheckError.Invalid")?
  let duplicate_name = """[{"ifindex":2,"ifname":"eth0","flags":[],"mtu":1500},{"ifindex":3,"ifname":"eth0","flags":[],"mtu":1500}]"""
  test.error_kind(report_checks.parse_ip_link_json(duplicate_name), "SystemReportCheckError.Invalid")?

  let typed_reference = report_checks.parse_ip_link_json(
    """[{"ifindex":5,"ifname":"eth0.42","flags":["UP"],"mtu":1500,"operstate":"UP","linkinfo":{"info_kind":"vlan"}}]""",
  )?
  assert typed_reference[0].kind == "vlan"
  assert ! report_checks.ip_link_reference_stable(typed_reference, [{...typed_reference[0], kind: "bridge"}])
  test.error_kind(
    report_checks.parse_ip_link_json(
  """[{"ifindex":5,"ifname":"eth0.42","flags":["UP"],"mtu":1500,"linkinfo":{"info_kind":""}}]""",
),
    "SystemReportCheckError.Invalid",
  )?
  let typed_candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":5,"name":{"state":"observed","value":"eth0.42"},"mtu":1500,"admin_up":true,"operational_state":"up","kind":"vlan","master_ifindex":null,"lower_ifindex":null}]}}"""
  assert report_checks.compare_ip_links(typed_candidate, typed_reference)?.exact
  let wrong_kind = report_checks.compare_ip_links(
    typed_candidate.replace("\"kind\":\"vlan\"", "\"kind\":\"bridge\""),
    typed_reference,
  )?
  assert wrong_kind.field_mismatches == ["5.kind"]
  assert ! wrong_kind.exact
}

test test_system_report_ip_link_reference_checks_master_relationship {
  let reference = report_checks.parse_ip_link_json(
    """[{"ifindex":6,"ifname":"br0","flags":["UP"],"mtu":1500,"operstate":"UP","linkinfo":{"info_kind":"bridge"}},{"ifindex":5,"ifname":"eth0","flags":["UP"],"mtu":1500,"operstate":"UP","master":"br0"}]""",
  )?
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":5,"name":{"state":"observed","value":"eth0"},"mtu":1500,"admin_up":true,"operational_state":"up","kind":null,"master_ifindex":6,"lower_ifindex":null},{"ifindex":6,"name":{"state":"observed","value":"br0"},"mtu":1500,"admin_up":true,"operational_state":"up","kind":"bridge","master_ifindex":null,"lower_ifindex":null}]}}"""
  assert report_checks.compare_ip_links(candidate, reference)?.exact
  let wrong_parent = report_checks.compare_ip_links(
    candidate.replace("\"master_ifindex\":6", "\"master_ifindex\":7"),
    reference,
  )?
  assert wrong_parent.field_mismatches == ["5.master_ifindex"]
  assert ! wrong_parent.exact
  let unresolved_master = report_checks.parse_ip_link_json(
    """[{"ifindex":5,"ifname":"eth0","flags":["UP"],"mtu":1500,"operstate":"UP","master":"missing"}]""",
  )?
  test.error_kind(report_checks.compare_ip_links(candidate, unresolved_master), "SystemReportCheckError.Invalid")?
}

test test_system_report_ip_link_reference_checks_lower_link_relationship {
  let reference = report_checks.parse_ip_link_json(
    """[{"ifindex":2,"ifname":"eth0","flags":["UP"],"mtu":1500,"operstate":"UP"},{"ifindex":5,"ifname":"eth0.42","flags":["UP"],"mtu":1500,"operstate":"UP","link":"eth0","linkinfo":{"info_kind":"vlan"}}]""",
  )?
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":5,"name":{"state":"observed","value":"eth0.42"},"mtu":1500,"admin_up":true,"operational_state":"up","kind":"vlan","master_ifindex":null,"lower_ifindex":2},{"ifindex":2,"name":{"state":"observed","value":"eth0"},"mtu":1500,"admin_up":true,"operational_state":"up","kind":null,"master_ifindex":null,"lower_ifindex":null}]}}"""
  assert report_checks.compare_ip_links(candidate, reference)?.exact
  let wrong_lower = report_checks.compare_ip_links(
    candidate.replace("\"lower_ifindex\":2", "\"lower_ifindex\":7"),
    reference,
  )?
  assert wrong_lower.field_mismatches == ["5.lower_ifindex"]
  let numeric_reference = report_checks.parse_ip_link_json(
    """[{"ifindex":5,"ifname":"veth0","flags":["UP"],"mtu":1500,"operstate":"UP","link_index":27,"link_netnsid":1}]""",
  )?
  let numeric_candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":5,"name":{"state":"observed","value":"veth0"},"mtu":1500,"admin_up":true,"operational_state":"up","kind":null,"master_ifindex":null,"lower_ifindex":27}]}}"""
  assert report_checks.compare_ip_links(numeric_candidate, numeric_reference)?.exact
  let ambiguous_link = """[{"ifindex":5,"ifname":"veth0","flags":["UP"],"mtu":1500,"link":"eth0","link_index":27}]"""
  test.error_kind(report_checks.parse_ip_link_json(ambiguous_link), "SystemReportCheckError.Invalid")?
  let unresolved_lower = report_checks.parse_ip_link_json(
    """[{"ifindex":5,"ifname":"eth0.42","flags":["UP"],"mtu":1500,"link":"missing"}]""",
  )?
  test.error_kind(report_checks.compare_ip_links(candidate, unresolved_lower), "SystemReportCheckError.Invalid")?
}

test test_system_report_ip_address_reference_preserves_ipv6_identity_and_link_membership {
  let ip_output = """[{"ifindex":2,"ifname":"eth0","addr_info":[{"family":"inet","local":"192.0.2.10","prefixlen":24,"broadcast":"192.0.2.255","scope":"global","valid_life_time":300,"preferred_life_time":120},{"family":"inet6","local":"2001:db8::10","prefixlen":64,"scope":"global","valid_life_time":240,"preferred_life_time":100}]},{"ifindex":3,"ifname":"eth0.42","addr_info":[{"family":"inet6","local":"2001:db8:42::5","prefixlen":64,"scope":"link"}]}]"""
  let reference = report_checks.parse_ip_address_json(ip_output)?
  assert reference.len() == 3
  assert reference[1].ifindex == 2
  assert reference[1].family == "ipv6"
  assert reference[1].address == "2001:db8::10"
  assert reference[2].ifindex == 3
  let later = report_checks.parse_ip_address_json(
    ip_output.replace("\"valid_life_time\":300", "\"valid_life_time\":299"),
  )?
  assert report_checks.ip_address_reference_stable(reference, later)
  let moved = report_checks.parse_ip_address_json(ip_output.replace("2001:db8:42::5", "2001:db8:42::6"))?
  assert ! report_checks.ip_address_reference_stable(reference, moved)
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":3,"addresses":[{"family":"ipv6","address":{"state":"observed","value":"2001:db8:42::5"},"prefix_length":64,"broadcast":{"state":"absent","value":null},"scope":"link"}]},{"ifindex":2,"addresses":[{"family":"ipv6","address":{"state":"observed","value":"2001:db8::10"},"prefix_length":64,"broadcast":{"state":"absent","value":null},"scope":"global"},{"family":"ipv4","address":{"state":"observed","value":"192.0.2.10"},"prefix_length":24,"broadcast":{"state":"observed","value":"192.0.2.255"},"scope":"global"}]}]}}"""
  let exact = report_checks.compare_ip_addresses(candidate, reference)?
  assert exact.exact_static
  assert exact.matched_count == 3
  assert report_checks.compare_ip_addresses(candidate.replace("\"complete\"", "\"partial\""), reference)?.exact_static
  assert report_checks.compare_ip_addresses(
    candidate.replace("\"enumeration_succeeded\":true", "\"enumeration_succeeded\":false"),
    reference,
  )?.candidate_field_missing
  let missing = candidate.replace("2001:db8:42::5", "2001:db8:42::6")
  let changed = report_checks.compare_ip_addresses(missing, reference)?
  assert ! changed.exact_static
  assert changed.missing_keys.len() == 1
  assert changed.unexpected_keys.len() == 1
  let wrong_scope = report_checks.compare_ip_addresses(
    candidate.replace("\"scope\":\"link\"", "\"scope\":\"host\""),
    reference,
  )?
  assert wrong_scope.field_mismatches.len() == 1
  assert ! wrong_scope.exact_static
  let malformed = """[{"ifindex":2,"ifname":"eth0","addr_info":[{"family":"inet6","local":"2001:db8::10","prefixlen":129,"scope":"global"}]}]"""
  test.error_kind(report_checks.parse_ip_address_json(malformed), "SystemReportCheckError.Invalid")?
  let unsupported = """[{"ifindex":2,"ifname":"eth0","addr_info":[{"family":"mpls","local":"100","prefixlen":20,"scope":"global"}]}]"""
  test.error_kind(report_checks.parse_ip_address_json(unsupported), "SystemReportCheckError.Invalid")?
}

test test_system_report_ip_address_lifetimes_score_bracketed_countdowns {
  let first = [
    report_checks.IpAddressReference(
      ifindex: 2,
      family: "ipv6",
      address: "2001:db8::10",
      prefix_length: 64,
      scope: "global",
      broadcast: null,
      valid_lifetime_seconds: 300,
      preferred_lifetime_seconds: 120,
    ),
  ]
  let later = [
    report_checks.IpAddressReference(
      ifindex: first[0].ifindex,
      family: first[0].family,
      address: first[0].address,
      prefix_length: first[0].prefix_length,
      scope: first[0].scope,
      broadcast: first[0].broadcast,
      valid_lifetime_seconds: 297,
      preferred_lifetime_seconds: 117,
    ),
  ]
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":2,"addresses":[{"family":"ipv6","address":{"state":"observed","value":"2001:db8::10"},"prefix_length":64,"valid_lifetime_seconds":299,"preferred_lifetime_seconds":119}]}]}}"""
  let exact = report_checks.compare_ip_address_lifetimes(candidate, first, later)?
  assert exact.exact
  assert exact.field_mismatches == []
  let outside = report_checks.compare_ip_address_lifetimes(
    candidate.replace("\"valid_lifetime_seconds\":299", "\"valid_lifetime_seconds\":301"),
    first,
    later,
  )?
  assert outside.field_mismatches == ["2|ipv6|2001:db8::10|64.valid_lifetime_seconds"]
  let missing = report_checks.compare_ip_address_lifetimes(
    candidate.replace("\"preferred_lifetime_seconds\":119", "\"preferred_lifetime_seconds\":null"),
    first,
    later,
  )?
  assert missing.field_mismatches == ["2|ipv6|2001:db8::10|64.preferred_lifetime_seconds"]
  let renewed = report_checks.compare_ip_address_lifetimes(
    candidate,
    first,
    [{...first[0], valid_lifetime_seconds: 305}],
  )?
  assert renewed.unstable_fields == ["2|ipv6|2001:db8::10|64.valid_lifetime_seconds"]
  let unknown = report_checks.compare_ip_address_lifetimes(
    candidate,
    [{...first[0], preferred_lifetime_seconds: null}],
    [{...first[0], preferred_lifetime_seconds: null}],
  )?
  assert unknown.unstable_fields == ["2|ipv6|2001:db8::10|64.preferred_lifetime_seconds"]
  assert ! unknown.exact
}

test test_system_report_ip_rule_reference_keeps_family_and_static_selectors {
  let ipv4 = report_checks.parse_ip_rule_json(
    """[{"priority":0,"src":"all","table":"local"},{"priority":100,"src":"192.0.2.0","srclen":24,"dst":"all","fwmark":"0x7","fwmask":"0xff","iif":"eth0","table":"main"}]""",
    "ipv4",
  )?
  let ipv6 = report_checks.parse_ip_rule_json(
    """[{"priority":101,"src":"2001:db8::","srclen":64,"table":"1000"}]""",
    "ipv6",
  )?
  let reference = ipv4.extend(ipv6)
  assert reference.len() == 3
  assert reference[0].table == 255
  assert reference[1].source_prefix_length == 24
  assert reference[2].family == "ipv6"
  assert report_checks.parse_ip_rule_json("""[{"priority":2,"src":"all","nop":null}]""", "ipv4")?[0].action == "nop"
  assert report_checks.parse_ip_rule_json(
    """[{"priority":3,"src":"all","fwmark":"0x7","fwmask":"0xffffffff"}]""",
    "ipv4",
  )?[0].fwmask == null
  test.error_kind(report_checks.parse_ip_rule_json("""[{"priority":4,"src":"all","fwmark":7}]""", "ipv4"), "schema")?
  assert report_checks.ip_rule_reference_stable(reference, [reference[2], reference[0], reference[1]])?
  assert ! report_checks.ip_rule_reference_stable(reference, [reference[0], reference[1]])?
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":2,"name":{"state":"observed","value":"eth0"}}],"rules":[{"family":"ipv6","priority":101,"source":{"state":"observed","value":"2001:db8::"},"source_prefix_length":64,"destination":{"state":"absent","value":null},"destination_prefix_length":0,"fwmark":null,"fwmask":null,"table":1000,"action":"to_table","input_ifindex":null,"output_ifindex":null,"attributes":[]},{"family":"ipv4","priority":100,"source":{"state":"observed","value":"192.0.2.0"},"source_prefix_length":24,"destination":{"state":"absent","value":null},"destination_prefix_length":0,"fwmark":7,"fwmask":255,"table":254,"action":"to_table","input_ifindex":2,"output_ifindex":null,"attributes":[]},{"family":"ipv4","priority":0,"source":{"state":"absent","value":null},"source_prefix_length":0,"destination":{"state":"absent","value":null},"destination_prefix_length":0,"fwmark":null,"fwmask":null,"table":255,"action":"to_table","input_ifindex":null,"output_ifindex":null,"attributes":[]}]}}"""
  let exact = report_checks.compare_ip_rules(candidate, reference)?
  assert exact.exact_static
  assert exact.matched_count == 3
  assert report_checks.compare_ip_rules(candidate.replace("\"priority\":0", "\"priority\":null"), reference)?.exact_static
  let wrong_mark = report_checks.compare_ip_rules(candidate.replace("\"fwmark\":7", "\"fwmark\":8"), reference)?
  assert ! wrong_mark.exact_static
  assert wrong_mark.missing_keys.len() == 1
  assert wrong_mark.unexpected_keys.len() == 1
  let partial = report_checks.compare_ip_rules(candidate.replace("\"complete\"", "\"partial\""), reference)?
  assert partial.exact_static
  assert report_checks.compare_ip_rules(
    candidate.replace("\"enumeration_succeeded\":true", "\"enumeration_succeeded\":false"),
    reference,
  )?.candidate_field_missing
  test.error_kind(
    report_checks.parse_ip_rule_json(
  """[{"priority":1,"src":"all","table":"main"},{"priority":1,"src":"all","table":"main"}]""",
  "ipv4",
),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(report_checks.parse_ip_rule_json("[]", "unspec"), "SystemReportCheckError.Invalid")?
}

test test_system_report_ip_rule_goto_target_is_part_of_rule_identity {
  let before = report_checks.parse_ip_rule_json("""[{"priority":150,"src":"all","goto":123}]""", "ipv4")?
  let changed = report_checks.parse_ip_rule_json("""[{"priority":150,"src":"all","goto":124}]""", "ipv4")?
  assert ! report_checks.ip_rule_reference_stable(before, changed)?
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[],"rules":[{"family":"ipv4","priority":150,"source":{"state":"absent","value":null},"source_prefix_length":0,"destination":{"state":"absent","value":null},"destination_prefix_length":0,"fwmark":null,"fwmask":null,"table":0,"action":"goto","input_ifindex":null,"output_ifindex":null,"attributes":[{"kind":4,"data":{"state":"observed","value":"ewAAAA=="}}]}]}}"""
  assert report_checks.compare_ip_rules(candidate, before)?.exact_static
  assert report_checks.compare_ip_rules(candidate.replace("\"kind\":4", "\"kind\":32772"), before)?.exact_static
  let wrong = report_checks.compare_ip_rules(candidate.replace("ewAAAA==", "fAAAAA=="), before)?
  assert ! wrong.exact_static
  assert wrong.missing_keys.len() == 1
  assert wrong.unexpected_keys.len() == 1
  let missing = report_checks.compare_ip_rules(candidate.replace("\"kind\":4", "\"kind\":5"), before)?
  assert missing.candidate_field_missing
  assert ! missing.exact_static
}

test test_system_report_ip_rule_full_score_requires_representable_selectors {
  let reference = report_checks.parse_ip_rule_json("""[{"priority":100,"src":"all","table":"main"}]""", "ipv4")?
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[],"rules":[{"family":"ipv4","priority":100,"source":{"state":"absent","value":null},"source_prefix_length":0,"destination":{"state":"absent","value":null},"destination_prefix_length":0,"fwmark":null,"fwmask":null,"table":254,"action":"to_table","input_ifindex":null,"output_ifindex":null,"flags":0,"attributes":[]}]}}"""
  assert report_checks.compare_ip_rules(candidate, reference)?.exact_scored
  let document = json.decode(candidate)?
  let ipv4_rule = json.get(document, ["network", "rules", 0])?
  let other_family_rule = json.set(ipv4_rule, ["family"], "af_128")?
  let with_other_family = json.set(document, ["network", "rules"], [ipv4_rule, other_family_rule])?
  let scoped = report_checks.compare_ip_rules(json.encode(with_other_family)?, reference)?
  assert scoped.exact_scored
  assert scoped.candidate_count == 1
  let extra_reference = report_checks.parse_ip_rule_json(
    """[{"priority":100,"src":"all","table":"main","suppress_prefixlen":0}]""",
    "ipv4",
  )?
  let extra = report_checks.compare_ip_rules(candidate, extra_reference)?
  assert extra.exact_static
  assert ! extra.exact_scored
  let unknown_attribute = report_checks.compare_ip_rules(
    candidate.replace(
      "\"attributes\":[]",
      "\"attributes\":[{\"kind\":99,\"data\":{\"state\":\"observed\",\"value\":\"AA==\"}}]",
    ),
    reference,
  )?
  assert unknown_attribute.exact_static
  assert ! unknown_attribute.exact_scored
  let flagged = report_checks.compare_ip_rules(candidate.replace("\"flags\":0", "\"flags\":1"), reference)?
  assert flagged.exact_static
  assert ! flagged.exact_scored
  let zero_reference = report_checks.parse_ip_rule_json("""[{"priority":0,"src":"all","table":"main"}]""", "ipv4")?
  let missing_priority = report_checks.compare_ip_rules(
    candidate.replace("\"priority\":100", "\"priority\":null"),
    zero_reference,
  )?
  assert missing_priority.exact_static
  assert ! missing_priority.exact_scored
}

test test_system_report_ip_rule_reference_preserves_unknown_numeric_action {
  let reference = report_checks.parse_ip_rule_json("""[{"priority":150,"src":"all","action":"50"}]""", "ipv4")?
  assert reference[0].action == "action_50"
  let known = report_checks.parse_ip_rule_json("""[{"priority":150,"src":"all","action":"6"}]""", "ipv4")?
  assert known[0].action == "blackhole"
  test.error_kind(
    report_checks.parse_ip_rule_json("""[{"priority":150,"src":"all","action":"999"}]""", "ipv4"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_ip_rule_reference_normalizes_named_kernel_actions {
  let reference = report_checks.parse_ip_rule_json(
    """[{"priority":100,"src":"all","action":"none"},{"priority":101,"src":"all","action":"anycast"},{"priority":102,"src":"all","action":"multicast"},{"priority":103,"src":"all","action":"throw"},{"priority":104,"src":"all","action":"xresolve"},{"priority":105,"src":"all","masquerade":null}]""",
    "ipv4",
  )?
  assert reference[0].action == "action_0"
  assert reference[1].action == "action_4"
  assert reference[2].action == "action_5"
  assert reference[3].action == "action_9"
  assert reference[4].action == "action_11"
  assert reference[5].action == "action_10"
  assert reference[5].unscored_fields == ["masquerade"]
}

test test_system_report_ip_rule_reference_preserves_prefix_without_address_attribute {
  let reference = report_checks.parse_ip_rule_json(
    """[{"priority":100,"src":"0","srclen":24,"dst":"0","dstlen":16,"table":"main"}]""",
    "ipv4",
  )?
  assert reference[0].source == null
  assert reference[0].source_prefix_length == 24
  assert reference[0].destination == null
  assert reference[0].destination_prefix_length == 16
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[],"rules":[{"family":"ipv4","priority":100,"source":{"state":"absent","value":null},"source_prefix_length":24,"destination":{"state":"absent","value":null},"destination_prefix_length":16,"fwmark":null,"fwmask":null,"table":254,"action":"to_table","input_ifindex":null,"output_ifindex":null,"flags":0,"attributes":[]}]}}"""
  assert report_checks.compare_ip_rules(candidate, reference)?.exact_scored
  let ipv6 = report_checks.parse_ip_rule_json("""[{"priority":101,"src":"0","srclen":64,"table":"main"}]""", "ipv6")?
  assert ipv6[0].source == null
  assert ipv6[0].source_prefix_length == 64
}

test test_system_report_ip_route_reference_keeps_family_table_and_link_identity {
  let ipv4 = report_checks.parse_ip_route_json(
    """[{"dst":"default","gateway":"192.0.2.1","dev":"eth0","metric":100},{"type":"local","dst":"192.0.2.10","dev":"eth0","table":"local","scope":"host","protocol":"kernel"}]""",
    "ipv4",
  )?
  let ipv6 = report_checks.parse_ip_route_json(
    """[{"dst":"2001:db8:42::/64","dev":"eth0.42","table":"1000","protocol":"static","metric":20}]""",
    "ipv6",
  )?
  let reference = ipv4.extend(ipv6)
  assert reference[0].destination == "0.0.0.0"
  assert reference[0].prefix_length == 0
  assert reference[0].table == 254
  assert reference[1].prefix_length == 32
  assert reference[2].family == "ipv6"
  assert report_checks.parse_ip_route_json("""[{"dst":"2001:db8::/64","protocol":"ra"}]""", "ipv6")?[0].protocol == "router_advertisement"
  assert report_checks.ip_route_reference_stable(reference, [reference[2], reference[0], reference[1]])?
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":2,"name":{"state":"observed","value":"eth0"}},{"ifindex":3,"name":{"state":"observed","value":"eth0.42"}}],"routes":[{"family":"ipv6","destination":{"state":"observed","value":"2001:db8:42::"},"source":{"state":"absent","value":null},"source_prefix_length":0,"preferred_source":{"state":"absent","value":null},"prefix_length":64,"gateway":{"state":"absent","value":null},"table":1000,"metric":20,"route_type":"unicast","scope":"global","protocol":"static","flags":0,"nexthops":[],"output_ifindex":3},{"family":"ipv4","destination":{"state":"observed","value":"192.0.2.10"},"source":{"state":"absent","value":null},"source_prefix_length":0,"preferred_source":{"state":"absent","value":null},"prefix_length":32,"gateway":{"state":"absent","value":null},"table":255,"metric":null,"route_type":"local","scope":"host","protocol":"kernel","flags":0,"nexthops":[],"output_ifindex":2},{"family":"ipv4","destination":{"state":"observed","value":"0.0.0.0"},"source":{"state":"absent","value":null},"source_prefix_length":0,"preferred_source":{"state":"absent","value":null},"prefix_length":0,"gateway":{"state":"observed","value":"192.0.2.1"},"table":254,"metric":100,"route_type":"unicast","scope":"global","protocol":"boot","flags":0,"nexthops":[],"output_ifindex":2}]}}"""
  let exact = report_checks.compare_ip_routes(candidate, reference)?
  assert exact.exact_static
  assert exact.matched_count == 3
  assert report_checks.compare_ip_routes(candidate.replace("\"complete\"", "\"partial\""), reference)?.exact_static
  assert report_checks.compare_ip_routes(
    candidate.replace("\"enumeration_succeeded\":true", "\"enumeration_succeeded\":false"),
    reference,
  )?.candidate_field_missing
  let changed = report_checks.compare_ip_routes(candidate.replace("\"metric\":100", "\"metric\":101"), reference)?
  assert ! changed.exact_static
  assert changed.missing_keys.len() == 1
  assert changed.unexpected_keys.len() == 1
  test.error_kind(
    report_checks.parse_ip_route_json("""[{"dst":"2001:db8::/129"}]""", "ipv6"),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_ip_route_json("""[{"dst":"default"},{"dst":"default"}]""", "ipv4"),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(report_checks.parse_ip_route_json("[]", "unspec"), "SystemReportCheckError.Invalid")?
}

test test_system_report_ip_route_reference_scores_source_and_multipath_hops {
  let output = """[{"dst":"2001:db8::/64","from":"2001:db8:1::/64","prefsrc":"2001:db8::10","table":"1000","flags":["notify"],"nexthops":[{"gateway":"2001:db8::1","dev":"eth0","weight":2,"flags":["onlink"]},{"gateway":"2001:db8::2","dev":"eth1","weight":1,"flags":[]}]}]"""
  let reference = report_checks.parse_ip_route_json(output, "ipv6")?
  assert reference[0].source == "2001:db8:1::"
  assert reference[0].source_prefix_length == 64
  assert reference[0].preferred_source == "2001:db8::10"
  assert reference[0].flags == 256
  assert reference[0].nexthops.len() == 2
  assert reference[0].nexthops[0].weight == 2
  assert reference[0].nexthops[0].flags == 4
  let zero_source = report_checks.parse_ip_route_json("""[{"dst":"2001:db8::/64","src":"0/64"}]""", "ipv6")?
  assert zero_source[0].source == null
  assert zero_source[0].source_prefix_length == 64
  let reordered = report_checks.parse_ip_route_json(
    output.replace(
      "\"nexthops\":[{\"gateway\":\"2001:db8::1\",\"dev\":\"eth0\",\"weight\":2,\"flags\":[\"onlink\"]},{\"gateway\":\"2001:db8::2\",\"dev\":\"eth1\",\"weight\":1,\"flags\":[]}]",
      "\"nexthops\":[{\"gateway\":\"2001:db8::2\",\"dev\":\"eth1\",\"weight\":1,\"flags\":[]},{\"gateway\":\"2001:db8::1\",\"dev\":\"eth0\",\"weight\":2,\"flags\":[\"onlink\"]}]",
    ),
    "ipv6",
  )?
  assert report_checks.ip_route_reference_stable(reference, reordered)?
  let changed_flags = report_checks.parse_ip_route_json(output.replace("\"onlink\"", "\"offload\""), "ipv6")?
  assert ! report_checks.ip_route_reference_stable(reference, changed_flags)?
  let changed_route_flags = report_checks.parse_ip_route_json(output.replace("\"notify\"", "\"rt_offload\""), "ipv6")?
  assert ! report_checks.ip_route_reference_stable(reference, changed_route_flags)?
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":2,"name":{"state":"observed","value":"eth0"}},{"ifindex":3,"name":{"state":"observed","value":"eth1"}}],"routes":[{"family":"ipv6","destination":{"state":"observed","value":"2001:db8::"},"prefix_length":64,"source":{"state":"observed","value":"2001:db8:1::"},"source_prefix_length":64,"preferred_source":{"state":"observed","value":"2001:db8::10"},"gateway":{"state":"absent","value":null},"table":1000,"metric":null,"route_type":"unicast","scope":"global","protocol":"boot","flags":256,"output_ifindex":null,"nexthops":[{"ifindex":3,"hops":0,"flags":0,"gateway":{"state":"observed","value":"2001:db8::2"}},{"ifindex":2,"hops":1,"flags":4,"gateway":{"state":"observed","value":"2001:db8::1"}}]}]}}"""
  assert report_checks.compare_ip_routes(candidate, reference)?.exact_static
  let wrong_source = report_checks.compare_ip_routes(candidate.replace("2001:db8:1::", "2001:db8:2::"), reference)?
  assert ! wrong_source.exact_static
  let wrong_source_prefix = report_checks.compare_ip_routes(
    candidate.replace("\"source_prefix_length\":64", "\"source_prefix_length\":63"),
    reference,
  )?
  assert ! wrong_source_prefix.exact_static
  let wrong_preferred_source = report_checks.compare_ip_routes(
    candidate.replace("2001:db8::10", "2001:db8::11"),
    reference,
  )?
  assert ! wrong_preferred_source.exact_static
  let wrong_gateway = report_checks.compare_ip_routes(candidate.replace("2001:db8::1", "2001:db8::3"), reference)?
  assert ! wrong_gateway.exact_static
  let wrong_weight = report_checks.compare_ip_routes(candidate.replace("\"hops\":1", "\"hops\":2"), reference)?
  assert ! wrong_weight.exact_static
  assert ! report_checks.compare_ip_routes(candidate.replace("\"flags\":256", "\"flags\":0"), reference)?.exact_static
  let extra_route_flag = report_checks.compare_ip_routes(candidate.replace("\"flags\":256", "\"flags\":768"), reference)?
  assert extra_route_flag.exact_static
  assert ! extra_route_flag.exact_scored
  let missing_hop = report_checks.compare_ip_routes(
    candidate.replace(
      "},{\"ifindex\":2,\"hops\":1,\"flags\":4,\"gateway\":{\"state\":\"observed\",\"value\":\"2001:db8::1\"}}",
      "}",
    ),
    reference,
  )?
  assert ! missing_hop.exact_static
  assert ! report_checks.compare_ip_routes(candidate.replace("\"flags\":4", "\"flags\":8"), reference)?.exact_static
  let unknown_candidate_flag = report_checks.compare_ip_routes(
    candidate.replace("\"flags\":4", "\"flags\":132"),
    reference,
  )?
  assert unknown_candidate_flag.exact_static
  assert ! unknown_candidate_flag.exact_scored
  test.error_kind(
    report_checks.parse_ip_route_json(output.replace("\"weight\":2", "\"weight\":0"), "ipv6"),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_ip_route_json(output.replace("\"onlink\"", "\"future_flag\""), "ipv6"),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_ip_route_json(output.replace("\"onlink\"", "\"onlink\",\"onlink\""), "ipv6"),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_ip_route_json(output.replace("\"notify\"", "\"future_flag\""), "ipv6"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_ip_route_full_score_requires_representable_fields {
  let reference = report_checks.parse_ip_route_json(
    """[{"dst":"192.0.2.0/24","dev":"eth0","table":"main","protocol":"static"}]""",
    "ipv4",
  )?
  let candidate = """{"network":{"status":{"state":"complete","enumeration_succeeded":true},"links":[{"ifindex":2,"name":{"state":"observed","value":"eth0"}}],"routes":[{"family":"ipv4","destination":{"state":"observed","value":"192.0.2.0"},"prefix_length":24,"source":{"state":"absent","value":null},"source_prefix_length":0,"preferred_source":{"state":"absent","value":null},"gateway":{"state":"absent","value":null},"table":254,"metric":null,"route_type":"unicast","scope":"global","protocol":"static","output_ifindex":2,"input_ifindex":null,"flags":0,"nexthops":[],"attributes":[{"kind":1,"data":{"state":"observed","value":"wAACAQ=="}}]}]}}"""
  assert report_checks.compare_ip_routes(candidate, reference)?.exact_scored
  let extra_reference = report_checks.parse_ip_route_json(
    """[{"dst":"192.0.2.0/24","dev":"eth0","table":"main","protocol":"static","expires":10}]""",
    "ipv4",
  )?
  let extra = report_checks.compare_ip_routes(candidate, extra_reference)?
  assert extra.exact_static
  assert ! extra.exact_scored
  let unknown_attribute = report_checks.compare_ip_routes(candidate.replace("\"kind\":1", "\"kind\":99"), reference)?
  assert unknown_attribute.exact_static
  assert ! unknown_attribute.exact_scored
  let flagged_attribute = report_checks.compare_ip_routes(candidate.replace("\"kind\":1", "\"kind\":32769"), reference)?
  assert flagged_attribute.exact_static
  assert ! flagged_attribute.exact_scored
  let input_link = report_checks.compare_ip_routes(
    candidate.replace("\"input_ifindex\":null", "\"input_ifindex\":2"),
    reference,
  )?
  assert input_link.exact_static
  assert ! input_link.exact_scored
  let hop_reference = report_checks.parse_ip_route_json(
    """[{"dst":"192.0.2.0/24","table":"main","protocol":"static","nexthops":[{"dev":"eth0","weight":2,"flags":[]}]}]""",
    "ipv4",
  )?
  let with_hop = candidate.replace("\"output_ifindex\":2", "\"output_ifindex\":null")
    .replace(
      "\"nexthops\":[]",
      "\"nexthops\":[{\"ifindex\":2,\"hops\":1,\"flags\":0,\"gateway\":{\"state\":\"absent\",\"value\":null}}]",
    )
    .replace(
      "\"kind\":1,\"data\":{\"state\":\"observed\",\"value\":\"wAACAQ==\"}",
      "\"kind\":9,\"data\":{\"state\":\"observed\",\"value\":\"CAAAAQIAAAA=\"}",
    )
  assert report_checks.compare_ip_routes(with_hop, hop_reference)?.exact_scored
  let unknown_hop = report_checks.compare_ip_routes(
    with_hop.replace("CAAAAQIAAAA=", "EAAAAQIAAAAIAGMAAAAAAA=="),
    hop_reference,
  )?
  assert unknown_hop.exact_static
  assert ! unknown_hop.exact_scored
}

test test_system_report_ip_route_reference_preserves_unknown_numeric_enums {
  let reference = report_checks.parse_ip_route_json(
    """[{"dst":"192.0.2.0/24","dev":"eth0","table":"main","protocol":"77","type":"222","scope":"77"}]""",
    "ipv4",
  )?
  assert reference[0].protocol == "protocol_77"
  assert reference[0].route_type == "route_type_222"
  assert reference[0].scope == "scope_77"
  test.error_kind(
    report_checks.parse_ip_route_json("""[{"dst":"192.0.2.0/24","protocol":"999"}]""", "ipv4"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_cpu_set_capture_validates_saved_reference {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/devices/system/cpu", parents: true)?
  source.write(
    p"sys/devices/system/cpu/possible",
    """0-2
""",
  )?
  source.write(
    p"sys/devices/system/cpu/present",
    """0,2
""",
  )?
  source.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )?
  source.write(
    p"sys/devices/system/cpu/offline",
    """2
""",
  )?
  report_checks.capture_cpu_set_bundle(source, bundle, "synthetic_fixture")?
  assert report_checks.validate_cpu_set_bundle(bundle)?.present == [0, 2]
  bundle.write(p"sys/devices/system/cpu/present", "0,2 ")?
  test.error_kind(report_checks.validate_cpu_set_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(
    p"sys/devices/system/cpu/present",
    """0,2
""",
  )?
  test.error_kind(
    report_checks.capture_cpu_set_bundle(source, bundle, "synthetic_fixture"),
    "SystemReportCheckError.Invalid",
  )?
  source.write(p"sys/devices/system/cpu/present", "1")?
  assert report_checks.validate_cpu_set_bundle(bundle)?.present == [0, 2]
  bundle.write(
    p"sys/devices/system/cpu/present",
    """0,1
""",
  )?
  test.error_kind(report_checks.validate_cpu_set_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_cpu_set_capture_rejects_observed_error_metadata {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/devices/system/cpu", parents: true)?
  source.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )?
  source.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )?
  source.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )?
  source.write(p"sys/devices/system/cpu/offline", "\n")?
  report_checks.capture_cpu_set_bundle(source, bundle, "synthetic_fixture")?
  let metadata = json.decode(bundle.read_text(p"capture.json")?)?
  let errno = json.set(metadata, ["sources", 0, "errno"], 13)?
  bundle.write_atomic(p"capture.json", json.encode(errno)?)?
  test.error_kind(report_checks.validate_cpu_set_bundle(bundle), "SystemReportCheckError.Invalid")?
  let error_kind = json.set(metadata, ["sources", 0, "error_kind"], "permission_denied")?
  bundle.write_atomic(p"capture.json", json.encode(error_kind)?)?
  test.error_kind(report_checks.validate_cpu_set_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_cpu_set_capture_replays_raw_sources {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/devices/system/cpu", parents: true)?
  source.write(
    p"sys/devices/system/cpu/possible",
    """0-4
""",
  )?
  source.write(
    p"sys/devices/system/cpu/present",
    """2,4
""",
  )?
  source.write(
    p"sys/devices/system/cpu/online",
    """2
""",
  )?
  source.write(
    p"sys/devices/system/cpu/offline",
    """4
""",
  )?
  report_checks.capture_cpu_set_bundle(source, bundle, "synthetic_fixture")?
  let replay = report_checks.replay_cpu_set_bundle(bundle)?
  assert replay.exact
  assert replay.present.reference_count == 2
  assert replay.present.candidate_count == 2
  let raw = bundle.read_result(p"sys/devices/system/cpu/present")?
  assert raw.data == b"2,4\n"
  source.write(p"sys/devices/system/cpu/present", "0")?
  assert report_checks.replay_cpu_set_bundle(bundle)?.exact
  bundle.write(
    p"sys/devices/system/cpu/present",
    """0,4
""",
  )?
  test.error_kind(report_checks.replay_cpu_set_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_cpu_set_capture_records_missing_source {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/devices/system/cpu", parents: true)?
  source.write(p"sys/devices/system/cpu/possible", "0")?
  report_checks.capture_cpu_set_bundle(source, bundle, "synthetic_fixture")?
  assert "\"state\": \"absent\"" in bundle.read_text(p"capture.json")?
  assert "\"reference\": null" in bundle.read_text(p"capture.json")?
  test.error_kind(report_checks.replay_cpu_set_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_memory_capture_validates_raw_sources_and_oracles {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc", parents: true)?
  source.mkdir(p"sys/kernel/mm/transparent_hugepage", parents: true)?
  source.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
MemFree: 4 kB
""",
  )?
  source.write(
    p"sys/kernel/mm/transparent_hugepage/enabled",
    """always [madvise] never
""",
  )?
  report_checks.capture_memory_bundle(source, bundle, "synthetic_fixture")?
  let reference = report_checks.validate_memory_bundle(bundle)?
  assert reference.meminfo.len() == 2
  assert reference.thp.len() == 1
  assert reference.thp[0].name == "enabled"
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"stable\": true" in metadata
  bundle.write(p"capture.json", metadata.replace("\"stable\": true", "\"stable\": false"))?
  assert report_checks.validate_memory_bundle(bundle)?.meminfo.len() == 2
  source.write(
    p"proc/meminfo",
    """MemTotal: 64 kB
""",
  )?
  assert report_checks.validate_memory_bundle(bundle)?.meminfo[0].value == reference.meminfo[0].value
  bundle.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
MemFree: 5 kB
""",
  )?
  test.error_kind(report_checks.validate_memory_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
MemFree: 4 kB
""",
  )?
  assert report_checks.validate_memory_bundle(bundle)?.thp.len() == 1
  bundle.write(
    p"sys/kernel/mm/transparent_hugepage/defrag",
    """[always] never
""",
  )?
  test.error_kind(report_checks.validate_memory_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_memory_capture_rejects_observed_error_metadata {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc", parents: true)?
  source.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
MemFree: 4 kB
""",
  )?
  report_checks.capture_memory_bundle(source, bundle, "synthetic_fixture")?
  let metadata = json.decode(bundle.read_text(p"capture.json")?)?
  let errno = json.set(metadata, ["sources", 0, "errno"], 13)?
  bundle.write_atomic(p"capture.json", json.encode(errno)?)?
  test.error_kind(report_checks.validate_memory_bundle(bundle), "SystemReportCheckError.Invalid")?
  let error_kind = json.set(metadata, ["sources", 0, "error_kind"], "permission_denied")?
  bundle.write_atomic(p"capture.json", json.encode(error_kind)?)?
  test.error_kind(report_checks.validate_memory_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_memory_capture_replays_raw_sources {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc", parents: true)?
  source.mkdir(p"sys/kernel/mm/transparent_hugepage", parents: true)?
  source.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
MemFree: 4 kB
""",
  )?
  source.write(
    p"sys/kernel/mm/transparent_hugepage/enabled",
    """always [madvise] never
""",
  )?
  report_checks.capture_memory_bundle(source, bundle, "synthetic_fixture")?
  let replay = report_checks.replay_memory_bundle(bundle)?
  assert replay.meminfo.exact_scored
  assert replay.thp.exact
}

test test_system_report_os_release_capture_validates_selected_raw_source {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"usr/lib", parents: true)?
  source.write(
    p"usr/lib/os-release",
    """ID=vendor
VERSION_ID=2
""",
  )?
  report_checks.capture_os_release_bundle(source, bundle, "synthetic_fixture")?
  let reference = report_checks.validate_os_release_bundle(bundle)?
  assert reference.id == "vendor"
  assert reference.version_id == "2"
  assert "\"selected_path\": \"usr/lib/os-release\"" in bundle.read_text(p"capture.json")?
  bundle.write(
    p"etc/os-release",
    """ID=inserted
""",
  )?
  test.error_kind(report_checks.validate_os_release_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(
    p"usr/lib/os-release",
    """ID=tampered
VERSION_ID=2
""",
  )?
  test.error_kind(report_checks.validate_os_release_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_os_release_capture_rejects_observed_error_metadata {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"etc", parents: true)?
  source.mkdir(p"usr/lib", parents: true)?
  source.write(
    p"etc/os-release",
    """ID=local
""",
  )?
  source.write(
    p"usr/lib/os-release",
    """ID=vendor
""",
  )?
  report_checks.capture_os_release_bundle(source, bundle, "synthetic_fixture")?
  let metadata = json.decode(bundle.read_text(p"capture.json")?)?
  let errno = json.set(metadata, ["sources", 1, "errno"], 13)?
  bundle.write_atomic(p"capture.json", json.encode(errno)?)?
  test.error_kind(report_checks.validate_os_release_bundle(bundle), "SystemReportCheckError.Invalid")?
  let error_kind = json.set(metadata, ["sources", 0, "error_kind"], "permission_denied")?
  bundle.write_atomic(p"capture.json", json.encode(error_kind)?)?
  test.error_kind(report_checks.validate_os_release_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_os_release_capture_replays_product_identity {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"etc", parents: true)?
  source.mkdir(p"usr/lib", parents: true)?
  source.write(
    p"etc/os-release",
    """ID=local
VERSION_ID=1
""",
  )?
  source.write(
    p"usr/lib/os-release",
    """ID=vendor
VERSION_ID=2
""",
  )?
  report_checks.capture_os_release_bundle(source, bundle, "synthetic_fixture")?
  assert report_checks.validate_os_release_bundle(bundle)?.id == "local"
  assert report_checks.replay_os_release_bundle(bundle)?.exact
}

test test_system_report_os_release_capture_ignores_unselected_vendor_read_failure {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"etc", parents: true)?
  source.mkdir(p"usr/lib/os-release", parents: true)?
  source.write(
    p"etc/os-release",
    """ID=local
""",
  )?
  report_checks.capture_os_release_bundle(source, bundle, "synthetic_fixture")?
  assert report_checks.validate_os_release_bundle(bundle)?.id == "local"
}

test test_system_report_os_release_capture_does_not_fallback_from_malformed_local_source {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"etc", parents: true)?
  source.mkdir(p"usr/lib", parents: true)?
  source.write(
    p"etc/os-release",
    """ID=bad value
""",
  )?
  source.write(
    p"usr/lib/os-release",
    """ID=vendor
""",
  )?
  report_checks.capture_os_release_bundle(source, bundle, "synthetic_fixture")?
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"selected_path\": \"etc/os-release\"" in metadata
  assert "\"reference\": null" in metadata
  test.error_kind(report_checks.validate_os_release_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_kernel_command_line_capture_validates_raw_bytes {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc", parents: true)?
  source.write(p"proc/cmdline", b"quiet secret=fixture\0\xff\n")?
  report_checks.capture_kernel_command_line_bundle(source, bundle, "synthetic_fixture")?
  assert report_checks.validate_kernel_command_line_bundle(bundle)? == b"quiet secret=fixture\0\xff\n"
  let metadata = bundle.read_text(p"capture.json")?
  let contradictory = json.set(json.decode(metadata)?, ["errno"], 13)?
  bundle.write_atomic(p"capture.json", json.encode(contradictory)?)?
  test.error_kind(report_checks.validate_kernel_command_line_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write_atomic(p"capture.json", metadata)?
  source.write(
    p"proc/cmdline",
    """changed
""",
  )?
  assert report_checks.validate_kernel_command_line_bundle(bundle)? == b"quiet secret=fixture\0\xff\n"
  bundle.write(
    p"proc/cmdline",
    """quiet secret=changed
""",
  )?
  test.error_kind(report_checks.validate_kernel_command_line_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_kernel_command_line_capture_replays_redaction {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc", parents: true)?
  source.write(
    p"proc/cmdline",
    """quiet secret=fixture
""",
  )?
  report_checks.capture_kernel_command_line_bundle(source, bundle, "synthetic_fixture")?
  let result = report_checks.replay_kernel_command_line_bundle(bundle)?
  assert result.exact
  assert result.sensitive_exact
  assert result.redacted_exact
}

test test_system_report_kernel_command_line_capture_records_absence_without_scoring {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  report_checks.capture_kernel_command_line_bundle(source, bundle, "synthetic_fixture")?
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"source_state\": \"absent\"" in metadata
  assert "\"reference_base64\": null" in metadata
  test.error_kind(report_checks.validate_kernel_command_line_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_proc_stat_reference_preserves_identity_and_rejects_unsafe_fields {
  let stat = """123 (worker) pool) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2
"""
  let parsed = report_checks.parse_proc_stat_identity_reference(stat)?
  assert parsed.pid == 123
  assert parsed.command == "worker) pool"
  assert parsed.parent_pid == 1
  assert parsed.state == "S"
  assert parsed.start_ticks == 100
  assert report_checks.parse_proc_stat_thread_reference(stat)?.thread_count == 2
  test.error_kind(
    report_checks.parse_proc_stat_identity_reference(stat.replace("123 (", "0x7b (")),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_stat_identity_reference(stat.replace("100 8192 2", "9007199254740992 8192 2")),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_stat_thread_reference(stat.replace("2 0 100", "9007199254740992 0 100")),
    "SystemReportCheckError.Invalid",
  )?
  let valid_identity = report_checks.parse_proc_stat_identity_reference(
    stat.replace("100 8192 2", "100 8192 9007199254740992"),
  )?
  assert valid_identity.pid == 123
  assert valid_identity.start_ticks == 100
  assert valid_identity.command == "worker) pool"
  let valid_threads = report_checks.parse_proc_stat_thread_reference(
    stat.replace("100 8192 2", "100 9007199254740992 9007199254740992"),
  )?
  assert valid_threads.thread_count == 2
  assert valid_threads.start_ticks == 100
  test.error_kind(
    report_checks.parse_proc_stat_identity_reference(stat.replace("100 8192 2", "9007199254740992 8192 2")),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_stat_identity_reference("""123 (worker) S 1 1 1
"""),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_proc_status_uid_reference_requires_one_numeric_row {
  assert report_checks.parse_proc_status_uid_reference("""Name:	worker
Uid:	1000	1001	1001	1001
""")? == 1000
  test.error_kind(
    report_checks.parse_proc_status_uid_reference("""Name:	worker
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_status_uid_reference("""Uid:	1000	1001	1001	1001
Uid:	2000	2000	2000	2000
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_status_uid_reference("""Uid:	0x3e8	1001	1001	1001
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_status_uid_reference("""Uid:	9007199254740992	1001	1001	1001
"""),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_proc_statm_reference_uses_reported_page_size_and_exact_bytes {
  let parsed = report_checks.parse_proc_statm_reference(
    """2 1 0 0 0 0 0
""",
    65536,
  )?
  assert parsed.virtual_bytes == 131072
  assert parsed.resident_bytes == 65536
  test.error_kind(report_checks.parse_proc_statm_reference("2 1 0 0 0 0 0", 0), "SystemReportCheckError.Invalid")?
  test.error_kind(
    report_checks.parse_proc_statm_reference("137438953472 1 0 0 0 0 0", 65536),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_statm_reference("2 9007199254740992 0 0 0 0 0", 4096),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(report_checks.parse_proc_statm_reference("2 0x1 0 0 0 0 0", 4096), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_proc_statm_reference("2 1", 4096), "SystemReportCheckError.Invalid")?
  test.error_kind(
    report_checks.parse_proc_statm_reference("2 1 malformed 0 0 0 0", 4096),
    "SystemReportCheckError.Invalid",
  )?
  assert report_checks.parse_proc_statm_reference("2 1 9007199254740992 0 0 0 0", 4096)?.resident_bytes == 4096
}

test test_system_report_proc_cgroup_reference_requires_one_absolute_v2_path {
  assert report_checks.parse_proc_cgroup_reference("""0::/tenant/worker
2:cpu:/legacy
""")? == "/tenant/worker"
  assert report_checks.parse_proc_cgroup_reference("""2:cpu:/legacy
""")? == null
  test.error_kind(
    report_checks.parse_proc_cgroup_reference("""0::relative
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_cgroup_reference("""0::/first
0::/second
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_cgroup_reference("""0:/missing-controller:/path
"""),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_cpu_scope_reference_selects_visible_cgroup_mount {
  let membership = """0::/tenant/team:blue
"""
  let mounts = """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
32 20 0:25 /tenant /sys/fs/cgroup-alt rw - cgroup2 cgroup rw
"""
  let selected = report_checks.resolve_visible_cgroup2_location(membership, mounts)?
  assert selected.group_path == "/tenant/team:blue"
  assert selected.visible_path == "/tenant/team:blue"
  assert selected.source_path == "sys/fs/cgroup-alt/team:blue"
  assert selected.mount_root == "/tenant"
  assert report_checks.visible_cgroup2_ancestors(selected)? == [
    {
      visible_path: "/tenant/team:blue",
      source_path: "sys/fs/cgroup-alt/team:blue",
      hierarchy_level: 0,
    },
    {
      visible_path: "/tenant",
      source_path: "sys/fs/cgroup-alt",
      hierarchy_level: 1,
    },
  ]
  let escaped = report_checks.resolve_visible_cgroup2_location(
    """0::/team
""",
    """31 20 0:25 / /sys/fs/cgroup\\040space rw - cgroup2 cgroup rw
""",
  )?
  assert escaped.source_path == "sys/fs/cgroup space/team"
  test.error_kind(
    report_checks.resolve_visible_cgroup2_location(
  membership,
  """31 20 0:25 /tenant /sys/fs/cgroup rw - cgroup2 cgroup rw
32 20 0:25 /tenant /sys/fs/other rw - cgroup2 cgroup rw
""",
),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.resolve_visible_cgroup2_location(
  """0::/tenant/../escape
""",
  mounts,
),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.resolve_visible_cgroup2_location(
  membership,
  """31 20 0:25 /tenant /sys/fs/cgroup rw - cgroup2
""",
),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_cpu_scope_reference_parses_exact_quota_period {
  assert report_checks.parse_cpu_scope_quota("""50000 100000
""")? == {quota: 50000, period: 100000, unlimited: false}
  assert report_checks.parse_cpu_scope_quota("""max	100000
""")? == {quota: null, period: 100000, unlimited: true}
  for invalid in ["", "max", "0 100000", "50000 0", "-1 100000", "50000 100000 extra", "9007199254740992 100000"] {
    test.error_kind(report_checks.parse_cpu_scope_quota(invalid), "SystemReportCheckError.Invalid")?
  }
}

test test_system_report_cpu_scope_rooted_reference_reads_current_and_visible_ancestors {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)?
  root.mkdir(p"sys/fs/cgroup/worker", parents: true)?
  root.write(
    p"proc/self/cgroup",
    """0::/tenant/worker
""",
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 /tenant /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )?
  root.write(
    p"sys/fs/cgroup/worker/cpuset.cpus.effective",
    """0,2
""",
  )?
  root.write(
    p"sys/fs/cgroup/worker/cpu.max",
    """50000 100000
""",
  )?
  root.write(
    p"sys/fs/cgroup/cpuset.cpus.effective",
    """0-3
""",
  )?
  root.write(
    p"sys/fs/cgroup/cpu.max",
    """max 100000
""",
  )?
  let snapshot = report_checks.read_cpu_scope_cgroup_reference(root)?
  assert snapshot.ancestors.len() == 2
  assert snapshot.ancestors[0] == {
    path: "/tenant/worker",
    hierarchy_level: 0,
    effective_cpus: [
      0,
      2,
    ],
    quota: {
      quota: 50000,
      period: 100000,
      unlimited: false,
    },
  }
  assert snapshot.ancestors[1] == {
    path: "/tenant",
    hierarchy_level: 1,
    effective_cpus: [
      0,
      1,
      2,
      3,
    ],
    quota: {
      quota: null,
      period: 100000,
      unlimited: true,
    },
  }
  root.remove(p"sys/fs/cgroup/worker/cpu.max")?
  let missing = report_checks.read_cpu_scope_cgroup_reference(root)?
  assert missing.ancestors[0].quota == null
  root.write(
    p"sys/fs/cgroup/worker/cpu.max",
    """50000 0
""",
  )?
  test.error_kind(report_checks.read_cpu_scope_cgroup_reference(root), "SystemReportCheckError.Invalid")?
  root.write(
    p"proc/self/cgroup",
    """2:cpu:/legacy
""",
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup/cpu rw - cgroup cgroup rw,cpu
""",
  )?
  assert report_checks.read_cpu_scope_cgroup_reference(root)?.ancestors == []
  root.write(
    p"proc/self/cgroup",
    """0::/tenant/worker
""",
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 /outside /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )?
  assert report_checks.read_cpu_scope_cgroup_reference(root)?.ancestors == []
  assert report_checks.read_cgroup2_resource_reference(root)?.resources == []
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 /tenant /sys/fs/cgroup rw - cgroup2
""",
  )?
  test.error_kind(report_checks.read_cpu_scope_cgroup_reference(root), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.read_cgroup2_resource_reference(root), "SystemReportCheckError.Invalid")?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 /tenant /sys/fs/cgroup rw cgroup2 cgroup rw
""",
  )?
  test.error_kind(report_checks.read_cpu_scope_cgroup_reference(root), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.read_cgroup2_resource_reference(root), "SystemReportCheckError.Invalid")?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 /tenant /sys/fs/cgroup rw -
""",
  )?
  test.error_kind(report_checks.read_cpu_scope_cgroup_reference(root), "SystemReportCheckError.Invalid")?
}

test test_system_report_cpu_scope_cgroup_comparison_scores_visible_limits {
  let before = report_checks.CpuScopeCgroupObservation(
    ancestors: [
      {
        path: "/tenant/worker",
        hierarchy_level: 0,
        effective_cpus: [
          0,
          2,
        ],
        quota: {
          quota: 50000,
          period: 100000,
          unlimited: false,
        },
      },
      {
        path: "/tenant",
        hierarchy_level: 1,
        effective_cpus: [
          0,
          1,
          2,
          3,
        ],
        quota: {
          quota: null,
          period: 100000,
          unlimited: true,
        },
      },
    ],
    started: 1,
    ended: 2,
  )
  let candidate = """{"cpu":{"effective_cpuset":[2,0]},"memory":{"cgroup":[{"path":{"state":"observed","value":"/tenant/worker"},"hierarchy_level":0,"resource":"cpuset.cpus.effective","state":"observed","quota":null,"period":null,"maximum_unlimited":null,"effective_cpus":[0,2]},{"path":{"state":"observed","value":"/tenant/worker"},"hierarchy_level":0,"resource":"cpu.max","state":"observed","quota":50000,"period":100000,"maximum_unlimited":false,"effective_cpus":[]},{"path":{"state":"observed","value":"/tenant"},"hierarchy_level":1,"resource":"cpuset.cpus.effective","state":"observed","quota":null,"period":null,"maximum_unlimited":null,"effective_cpus":[0,1,2,3]},{"path":{"state":"observed","value":"/tenant"},"hierarchy_level":1,"resource":"cpu.max","state":"observed","quota":null,"period":100000,"maximum_unlimited":true,"effective_cpus":[]}]}}"""
  let exact = report_checks.compare_cpu_scope_cgroup(candidate, before, before)?
  assert exact.eligible == true
  assert exact.exact == true
  assert exact.checked_resources == 4
  let wrong = candidate.replace("\"quota\":50000", "\"quota\":60000")
  assert report_checks.compare_cpu_scope_cgroup(wrong, before, before)?.exact == false
  let changed = {
    ...before,
    ancestors: [
      {
        ...before.ancestors[0],
        quota: {
          quota: 60000,
          period: 100000,
          unlimited: false,
        },
      },
      before.ancestors[1],
    ],
  }
  test.error_kind(report_checks.compare_cpu_scope_cgroup(candidate, before, changed), "SystemReportCheckError.Invalid")?
  let duplicate = candidate.replace(
    "\"cgroup\":[",
    "\"cgroup\":[{\"path\":{\"state\":\"observed\",\"value\":\"/tenant/worker\"},\"hierarchy_level\":0,\"resource\":\"cpu.max\",\"state\":\"observed\",\"quota\":50000,\"period\":100000,\"maximum_unlimited\":false,\"effective_cpus\":[]},",
  )
  test.error_kind(report_checks.compare_cpu_scope_cgroup(duplicate, before, before), "SystemReportCheckError.Invalid")?
  let no_cgroup = {...before, ancestors: []}
  let unavailable = report_checks.compare_cpu_scope_cgroup(
    """{"cpu":{"effective_cpuset":[]},"memory":{"cgroup":[]}}""",
    no_cgroup,
    no_cgroup,
  )?
  assert unavailable.eligible == false
  assert unavailable.exact == true
}

test test_system_report_cgroup_v2_reference_parses_limits_and_named_counters {
  assert report_checks.parse_cgroup2_limit("""max
""")? == {value: null, unlimited: true}
  assert report_checks.parse_cgroup2_limit("""8192
""")? == {value: 8192, unlimited: false}
  for invalid in ["", "-1", "8192 4", "9007199254740992"] {
    test.error_kind(report_checks.parse_cgroup2_limit(invalid), "SystemReportCheckError.Invalid")?
  }

  assert report_checks.parse_cgroup2_cpu_stat("""usage_usec 9
user_usec 7
unknown_future 4
""")? == [
  {
    resource: "cpu.stat.usage_usec",
    value: 9,
    unit: "microseconds",
  },
  {
    resource: "cpu.stat.user_usec",
    value: 7,
    unit: "microseconds",
  },
]
  test.error_kind(
    report_checks.parse_cgroup2_cpu_stat("""usage_usec 9
usage_usec 10
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_cgroup2_cpu_stat("""usage_usec 9007199254740992
"""),
    "SystemReportCheckError.Invalid",
  )?
  assert report_checks.parse_cgroup2_io_stat("""8:0 rbytes=100 wbytes=200 rios=3
8:16 rbytes=10 unknown=5
""")? == [
  {
    resource: "io.stat.8:0.rbytes",
    value: 100,
    unit: "bytes",
  },
  {
    resource: "io.stat.8:0.wbytes",
    value: 200,
    unit: "bytes",
  },
  {
    resource: "io.stat.8:0.rios",
    value: 3,
    unit: "requests",
  },
  {
    resource: "io.stat.8:16.rbytes",
    value: 10,
    unit: "bytes",
  },
]
  assert report_checks.parse_cgroup2_io_stat("""7:7 
8:0 rbytes=1
""")? == [{resource: "io.stat.8:0.rbytes", value: 1, unit: "bytes"}]
  test.error_kind(
    report_checks.parse_cgroup2_io_stat("""8:0 rbytes=1
8:0 wbytes=2
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_cgroup2_io_stat("""8:0 rbytes=1 rbytes=2
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_cgroup2_io_stat("""bad rbytes=1
"""),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_cgroup_v2_rooted_reference_reads_all_visible_resource_families {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)?
  root.mkdir(p"sys/fs/cgroup/worker", parents: true)?
  root.write(
    p"proc/self/cgroup",
    """0::/tenant/worker
""",
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 /tenant /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )?
  root.write(
    p"sys/fs/cgroup/worker/memory.max",
    """8192
""",
  )?
  root.write(
    p"sys/fs/cgroup/worker/memory.current",
    """4096
""",
  )?
  root.write(
    p"sys/fs/cgroup/worker/memory.swap.max",
    """max
""",
  )?
  root.write(
    p"sys/fs/cgroup/worker/memory.swap.current",
    """0
""",
  )?
  root.write(
    p"sys/fs/cgroup/worker/pids.max",
    """32
""",
  )?
  root.write(
    p"sys/fs/cgroup/worker/pids.current",
    """2
""",
  )?
  root.write(
    p"sys/fs/cgroup/worker/cpu.max",
    """50000 100000
""",
  )?
  root.write(
    p"sys/fs/cgroup/worker/cpu.stat",
    """usage_usec 9
nr_periods 2
""",
  )?
  root.write(
    p"sys/fs/cgroup/worker/cpuset.cpus.effective",
    """0,2
""",
  )?
  root.write(
    p"sys/fs/cgroup/worker/io.stat",
    """8:0 rbytes=100 rios=3
""",
  )?
  root.write(
    p"sys/fs/cgroup/cpu.max",
    """max 100000
""",
  )?
  let snapshot = report_checks.read_cgroup2_resource_reference(root)?
  assert snapshot.ancestors == ["/tenant/worker", "/tenant"]
  assert snapshot.resources
    |> any .path == "/tenant/worker" and .resource == "memory.max" and .maximum_value == 8192 and .current_value == 4096 and .unit == "bytes"
  assert snapshot.resources |> any .resource == "memory.swap.max" and .maximum_unlimited == true and .current_value == 0
  assert snapshot.resources
    |> any .resource == "pids.max" and .maximum_value == 32 and .current_value == 2 and .unit == "count"
  assert snapshot.resources |> any .resource == "cpu.max" and .quota == 50000 and .period == 100000
  assert snapshot.resources
    |> any .resource == "cpu.stat.usage_usec" and .current_value == 9 and .unit == "microseconds"
  assert snapshot.resources |> any .resource == "cpuset.cpus.effective" and .effective_cpus == [0, 2]
  assert snapshot.resources |> any .resource == "io.stat.8:0.rbytes" and .current_value == 100 and .unit == "bytes"
  assert snapshot.resources |> any .path == "/tenant" and .resource == "cpu.max" and .maximum_unlimited == true
  root.write(
    p"sys/fs/cgroup/worker/memory.current",
    """broken
""",
  )?
  test.error_kind(report_checks.read_cgroup2_resource_reference(root), "SystemReportCheckError.Invalid")?
}

test test_system_report_cgroup_v2_bundle_replays_visible_ancestors_and_rejects_tampering {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc/self", parents: true)?
  source.mkdir(p"sys/fs/cgroup/worker", parents: true)?
  source.write(
    p"proc/self/cgroup",
    """0::/tenant/worker
""",
  )?
  source.write(
    p"proc/self/mountinfo",
    """31 20 0:25 /tenant /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )?
  source.write(
    p"sys/fs/cgroup/worker/memory.max",
    """8192
""",
  )?
  source.write(
    p"sys/fs/cgroup/worker/memory.current",
    """4096
""",
  )?
  source.write(
    p"sys/fs/cgroup/worker/cpu.max",
    """50000 100000
""",
  )?
  source.write(
    p"sys/fs/cgroup/worker/cpu.stat",
    """usage_usec 9
nr_periods 2
""",
  )?
  source.write(
    p"sys/fs/cgroup/worker/cpuset.cpus.effective",
    """0,2
""",
  )?
  source.write(
    p"sys/fs/cgroup/cpu.max",
    """max 100000
""",
  )?
  report_checks.capture_cgroup2_bundle(source, bundle, "synthetic_fixture")?
  let replay = report_checks.replay_cgroup2_bundle(bundle)?
  assert replay.exact_scored
  assert replay.reference_count == 6
  bundle.write(
    p"sys/fs/cgroup/worker/memory.max",
    """4096
""",
  )?
  test.error_kind(report_checks.validate_cgroup2_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_cgroup_v2_bundle_keeps_missing_mount_unscoreable {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc/self", parents: true)?
  source.write(
    p"proc/self/cgroup",
    """0::/
""",
  )?
  source.write(
    p"proc/self/mountinfo",
    """31 20 8:0 / / rw - ext4 /dev/sda rw
""",
  )?
  report_checks.capture_cgroup2_bundle(source, bundle, "synthetic_fixture")?
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"scoreable\": false" in metadata
  test.error_kind(report_checks.validate_cgroup2_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_cgroup_v2_bundle_records_missing_membership_source {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc/self", parents: true)?
  source.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )?
  report_checks.capture_cgroup2_bundle(source, bundle, "synthetic_fixture")?
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"path\": \"proc/self/cgroup\"" in metadata
  assert "\"state\": \"absent\"" in metadata
  assert "\"scoreable\": false" in metadata
  test.error_kind(report_checks.validate_cgroup2_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_cgroup_v2_comparison_scores_stable_limits_and_bracketed_counters {
  let before = report_checks.Cgroup2ResourceObservation(
    ancestors: [
      "/tenant/worker",
    ],
    started: 1,
    ended: 2,
    resources: [
      {
        path: "/tenant/worker",
        hierarchy_level: 0,
        controller: "memory",
        resource: "memory.max",
        maximum_value: 8192,
        current_value: 4000,
        unit: "bytes",
        maximum_unlimited: false,
        quota: null,
        period: null,
        effective_cpus: [],
      },
      {
        path: "/tenant/worker",
        hierarchy_level: 0,
        controller: "cpu",
        resource: "cpu.stat.usage_usec",
        maximum_value: null,
        current_value: 9,
        unit: "microseconds",
        maximum_unlimited: null,
        quota: null,
        period: null,
        effective_cpus: [],
      },
    ],
  )
  let after = report_checks.Cgroup2ResourceObservation(
    ancestors: before.ancestors,
    started: before.started,
    ended: before.ended,
    resources: [
      {
        ...before.resources[0],
        current_value: 4000,
      },
      {
        ...before.resources[1],
        current_value: 12,
      },
    ],
  )
  let candidate = """{"memory":{"cgroup":[{"path":{"state":"observed","value":"/tenant/worker"},"hierarchy_level":0,"controller":"memory","resource":"memory.max","state":"observed","maximum_value":8192,"current_value":4000,"unit":"bytes","maximum_unlimited":false,"quota":null,"period":null,"effective_cpus":[],"hidden_ancestors_possible":true},{"path":{"state":"observed","value":"/tenant/worker"},"hierarchy_level":0,"controller":"cpu","resource":"cpu.stat.usage_usec","state":"observed","maximum_value":null,"current_value":10,"unit":"microseconds","maximum_unlimited":null,"quota":null,"period":null,"effective_cpus":[],"hidden_ancestors_possible":true}]}}"""
  let exact = report_checks.compare_cgroup2_resources(candidate, before, after)?
  assert exact.eligible == true
  assert exact.exact_scored == true
  assert exact.reference_count == 2
  assert report_checks.compare_cgroup2_resources(
    candidate.replace("\"maximum_value\":8192", "\"maximum_value\":9000"),
    before,
    after,
  )?.exact_scored == false
  assert report_checks.compare_cgroup2_resources(
    candidate.replace("\"current_value\":10", "\"current_value\":13"),
    before,
    after,
  )?.exact_scored == false
  let changing = report_checks.Cgroup2ResourceObservation(
    ancestors: after.ancestors,
    started: after.started,
    ended: after.ended,
    resources: [{...after.resources[0], current_value: 4500}, after.resources[1]],
  )
  let partial = report_checks.compare_cgroup2_resources(candidate, before, changing)?
  assert partial.exact_stable == true
  assert partial.exact_scored == false
  assert partial.unscored_gauges == ["0:memory.max"]
  test.error_kind(
    report_checks.compare_cgroup2_resources(
      candidate,
      before,
      {...after, resources: [{...after.resources[0], maximum_value: 9000}, after.resources[1]]},
    ),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_meminfo_reference_preserves_units_and_rejects_ambiguous_rows {
  let source = """MemTotal: 16 kB
MemFree:	4	kB
HugePages_Total: 2
VendorCounter: 12 widgets
"""
  let counters = report_checks.parse_meminfo_reference(source)?
  assert counters.len() == 4
  assert counters |> any .name == "MemTotal" and .value == 16384 and .unit == "bytes"
  assert counters |> any .name == "MemFree" and .value == 4096 and .unit == "bytes"
  assert counters |> any .name == "HugePages_Total" and .value == 2 and .unit == "count"
  assert counters |> any .name == "VendorCounter" and .value == 12 and .unit == "widgets"
  test.error_kind(
    report_checks.parse_meminfo_reference("""MemTotal: 16 MB
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_meminfo_reference("""MemTotal: 16 kB
MemTotal: 32 kB
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_meminfo_reference("""MemTotal: 8796093022208 kB
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_meminfo_reference("""MemTotal: 16 kB
BrokenRow
"""),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_meminfo_comparison_scores_stable_fields_and_host_projection {
  let before = report_checks.parse_meminfo_reference("""MemTotal: 16 kB
MemFree: 4 kB
VendorCounter: 12 widgets
""")?
  let after = report_checks.parse_meminfo_reference("""MemTotal: 16 kB
MemFree: 5 kB
VendorCounter: 12 widgets
""")?
  let candidate = """{"memory":{"host":{"total_bytes":16384,"free_bytes":4608,"counters":[{"name":"MemTotal","value":16384,"unit":"bytes"},{"name":"MemFree","value":4608,"unit":"bytes"},{"name":"VendorCounter","value":12,"unit":"widgets"}]}}}"""
  let compared = report_checks.compare_meminfo(candidate, before, after)?
  assert compared.reference_count == 3
  assert compared.stable_count == 2
  assert compared.changed_count == 1
  assert compared.missing_names == []
  assert compared.mismatched_names == []
  assert compared.exact_scored

  let wrong_scalar = candidate.replace("\"total_bytes\":16384", "\"total_bytes\":32768")
  let projection = report_checks.compare_meminfo(wrong_scalar, before, after)?
  assert projection.scalar_mismatches == ["MemTotal"]
  assert ! projection.exact_scored
  let missing = candidate.replace(",{\"name\":\"VendorCounter\",\"value\":12,\"unit\":\"widgets\"}", "")
  assert report_checks.compare_meminfo(missing, before, after)?.missing_names == ["VendorCounter"]
  let duplicate = report_checks.parse_meminfo_reference("""MemTotal: 16 kB
""")?
  test.error_kind(
    report_checks.compare_meminfo(candidate, duplicate.extend(duplicate), duplicate),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_thp_reference_requires_one_selected_policy_and_stable_fields {
  assert report_checks.parse_thp_reference("""always [future_policy] never
""")? == "always [future_policy] never"
  for bad in [
    "",
    """always never
""",
    """[always] [never]
""",
    """[always] always
""",
    """[always]
[never]
""",
  ] {
    test.error_kind(report_checks.parse_thp_reference(bad), "SystemReportCheckError.Invalid")?
  }

  let before = [
    {
      name: "enabled",
      value: "always [future_policy] never",
    },
    {
      name: "defrag",
      value: "always defer [madvise] never",
    },
  ]
  let candidate = """{"memory":{"transparent_huge_pages":["defrag=always defer [madvise] never","enabled=always [future_policy] never"]}}"""
  let matched = report_checks.compare_thp(candidate, before, before)?
  assert matched.exact
  assert matched.matched_count == 2
  let missing = candidate.replace("\"defrag=always defer [madvise] never\",", "")
  assert report_checks.compare_thp(missing, before, before)?.missing_names == ["defrag"]
  let wrong = candidate.replace("[future_policy]", "[always]")
  assert report_checks.compare_thp(wrong, before, before)?.mismatched_names == ["enabled"]
  let duplicate = candidate.replace(
    "\"enabled=always [future_policy] never\"",
    "\"enabled=always [future_policy] never\",\"enabled=always [future_policy] never\"",
  )
  test.error_kind(report_checks.compare_thp(duplicate, before, before), "SystemReportCheckError.Invalid")?
  let changed = [{name: "enabled", value: "[always] never"}, {name: "defrag", value: "always defer [madvise] never"}]
  test.error_kind(report_checks.compare_thp(candidate, before, changed), "SystemReportCheckError.Invalid")?
}

test test_system_report_vulnerability_reference_requires_complete_stable_named_values {
  let before = [
    {
      name: "spectre_v1",
      description: "Mitigation: custom policy",
    },
    {
      name: "mmio_stale_data",
      description: "Not affected",
    },
  ]
  let candidate = """{"cpu":{"vulnerabilities":[{"name":"mmio_stale_data","description":{"state":"observed","value":"Not affected","raw_bytes_base64":null}},{"name":"spectre_v1","description":{"state":"observed","value":"Mitigation: custom policy","raw_bytes_base64":null}}]}}"""
  let compared = report_checks.compare_vulnerabilities(candidate, before, before)?
  assert compared.exact
  assert compared.matched_count == 2
  let missing = """{"cpu":{"vulnerabilities":[{"name":"mmio_stale_data","description":{"state":"observed","value":"Not affected","raw_bytes_base64":null}}]}}"""
  assert report_checks.compare_vulnerabilities(missing, before, before)?.missing_names == ["spectre_v1"]
  let unexpected = candidate.replace(
    "]}}",
    ",{\"name\":\"new_issue\",\"description\":{\"state\":\"observed\",\"value\":\"Unknown\",\"raw_bytes_base64\":null}}]}}",
  )
  assert report_checks.compare_vulnerabilities(unexpected, before, before)?.unexpected_names == ["new_issue"]
  let changed_value = candidate.replace("Mitigation: custom policy", "Vulnerable: custom policy")
  assert report_checks.compare_vulnerabilities(changed_value, before, before)?.mismatched_names == ["spectre_v1"]
  let unavailable = candidate.replace(
    "\"state\":\"observed\",\"value\":\"Mitigation: custom policy\"",
    "\"state\":\"absent\",\"value\":null",
  )
  assert report_checks.compare_vulnerabilities(unavailable, before, before)?.mismatched_names == ["spectre_v1"]
  let duplicate = candidate.replace(
    "}]}}",
    "},{\"name\":\"spectre_v1\",\"description\":{\"state\":\"observed\",\"value\":\"Mitigation: custom policy\",\"raw_bytes_base64\":null}}]}}",
  )
  test.error_kind(report_checks.compare_vulnerabilities(duplicate, before, before), "SystemReportCheckError.Invalid")?
  test.error_kind(
    report_checks.compare_vulnerabilities(candidate, before.extend(before), before),
    "SystemReportCheckError.Invalid",
  )?
  assert ! report_checks.compare_vulnerabilities("""{"cpu":{"vulnerabilities":[]}}""", [], [])?.exact
  let changed_reference = [
    {
      name: "spectre_v1",
      description: "Vulnerable",
    },
    {
      name: "mmio_stale_data",
      description: "Not affected",
    },
  ]
  test.error_kind(
    report_checks.compare_vulnerabilities(candidate, before, changed_reference),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_vulnerability_capture_validates_saved_files_and_oracle {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let directory = p"sys/devices/system/cpu/vulnerabilities"
  source.mkdir(directory, parents: true)?
  source.write(
    fp"${directory}/spectre_v1",
    """Mitigation: custom policy
""",
  )?
  source.write(
    fp"${directory}/mmio_stale_data",
    """Not affected
""",
  )?
  report_checks.capture_vulnerabilities_bundle(source, bundle, "synthetic_fixture")?
  let reference = report_checks.validate_vulnerabilities_bundle(bundle)? |> sort-by .name
  assert reference.len() == 2
  assert reference[0].name == "mmio_stale_data"
  assert reference[0].description == "Not affected"
  assert reference[1].description == "Mitigation: custom policy"
  let metadata = bundle.read_text(p"capture.json")?
  let contradictory = json.set(json.decode(metadata)?, ["sources", 0, "errno"], 13)?
  bundle.write_atomic(p"capture.json", json.encode(contradictory)?)?
  test.error_kind(report_checks.validate_vulnerabilities_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write_atomic(p"capture.json", metadata)?
  bundle.write(
    fp"${directory}/spectre_v1",
    """Vulnerable
""",
  )?
  test.error_kind(report_checks.validate_vulnerabilities_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(
    fp"${directory}/spectre_v1",
    """Mitigation: custom policy
""",
  )?
  bundle.write(
    fp"${directory}/extra",
    """Not affected
""",
  )?
  test.error_kind(report_checks.validate_vulnerabilities_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_vulnerability_capture_replays_production_cpu_collector {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/devices/system/cpu/vulnerabilities", parents: true)?
  source.write(
    p"sys/devices/system/cpu/vulnerabilities/spectre_v1",
    """Mitigation: custom policy
""",
  )?
  report_checks.capture_vulnerabilities_bundle(source, bundle, "synthetic_fixture")?
  assert report_checks.replay_vulnerabilities_bundle(bundle)?.exact
}

test test_system_report_vulnerability_capture_preserves_absent_class_without_scoring {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  report_checks.capture_vulnerabilities_bundle(source, bundle, "synthetic_fixture")?
  assert "\"listing_state\": \"absent\"" in bundle.read_text(p"capture.json")?
  test.error_kind(report_checks.validate_vulnerabilities_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_bundle_replay_rejects_changed_capture_metadata {
  let bundle = fs.tempdir()?
  defer bundle.close()?
  bundle.write(p"capture.json", "{\"origin\":\"synthetic_fixture\"}")?
  let original = bundle.read_result(p"capture.json")?.data ?? b""
  report_checks.require_capture_metadata_unchanged(bundle, original)?
  bundle.write(p"capture.json", "{\"origin\":\"live_capture\"}")?
  test.error_kind(report_checks.require_capture_metadata_unchanged(bundle, original), "SystemReportCheckError.Invalid")?
}

test test_system_report_huge_page_reference_scores_global_and_numa_pools {
  let before: List[HugePageReferenceFixture] = [
    {
      node_id: null,
      page_size_bytes: 2097152,
      total: 4,
      free: 3,
      reserved: 1,
      surplus: 0,
    },
    {
      node_id: 0,
      page_size_bytes: 2097152,
      total: 2,
      free: 1,
      reserved: null,
      surplus: 0,
    },
  ]
  let candidate = """{"memory":{"huge_pages":[{"node_id":0,"page_size_bytes":2097152,"total":2,"free":1,"reserved":null,"surplus":0},{"node_id":null,"page_size_bytes":2097152,"total":4,"free":3,"reserved":1,"surplus":0}]}}"""
  let matched = report_checks.compare_huge_pages(candidate, before, before)?
  assert matched.exact_scored
  assert matched.matched_count == 2
  let wrong = candidate.replace("\"total\":4", "\"total\":5")
  assert report_checks.compare_huge_pages(wrong, before, before)?.mismatched_fields == ["global:2097152.total"]
  let after = [{...before[0], free: 2}, before[1]]
  let changing = report_checks.compare_huge_pages(candidate, before, after)?
  assert changing.changed_fields == ["global:2097152.free"]
  assert changing.exact_stable
  assert ! changing.exact_scored
  let missing = """{"memory":{"huge_pages":[{"node_id":null,"page_size_bytes":2097152,"total":4,"free":3,"reserved":1,"surplus":0}]}}"""
  assert report_checks.compare_huge_pages(missing, before, before)?.missing_keys == ["node0:2097152"]
  let duplicate = candidate.replace(
    "]}}",
    ",{\"node_id\":0,\"page_size_bytes\":2097152,\"total\":2,\"free\":1,\"reserved\":null,\"surplus\":0}]}}",
  )
  test.error_kind(report_checks.compare_huge_pages(duplicate, before, before), "SystemReportCheckError.Invalid")?
  test.error_kind(
    report_checks.compare_huge_pages(candidate, before.extend(before), before),
    "SystemReportCheckError.Invalid",
  )?
  assert ! report_checks.compare_huge_pages("""{"memory":{"huge_pages":[]}}""", [], [])?.exact_scored
  assert report_checks.compare_huge_pages(candidate, [], [])?.unexpected_keys == ["global:2097152", "node0:2097152"]
}

test test_system_report_huge_page_reference_parses_complete_decimal_counters {
  assert report_checks.parse_huge_page_counter_reference("""42
""")? == 42
  assert report_checks.parse_huge_page_counter_reference("""0008
""")? == 8
  for bad in [
    "",
    """-1
""",
    """0x2
""",
    """1 2
""",
    """9007199254740992
""",
  ] {
    test.error_kind(report_checks.parse_huge_page_counter_reference(bad), "SystemReportCheckError.Invalid")?
  }
}

test test_system_report_huge_page_reference_reads_visible_global_and_numa_sources {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/kernel/mm/hugepages/hugepages-2048kB", parents: true)?
  root.mkdir(p"sys/devices/system/node/node0/hugepages/hugepages-1048576kB", parents: true)?
  root.write(
    p"sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages",
    """4
""",
  )?
  root.write(
    p"sys/kernel/mm/hugepages/hugepages-2048kB/free_hugepages",
    """3
""",
  )?
  root.write(
    p"sys/devices/system/node/node0/hugepages/hugepages-1048576kB/nr_hugepages",
    """2
""",
  )?
  let observed = report_checks.read_huge_page_reference(root)?
  assert observed.pools.len() == 2
  let global = observed.pools |> where .node_id == null
  assert global.len() == 1
  assert global[0].page_size_bytes == 2097152
  assert global[0].total == 4
  assert global[0].free == 3
  assert global[0].reserved == null
  let node = observed.pools |> where .node_id == 0
  assert node.len() == 1
  assert node[0].page_size_bytes == 1073741824
  assert node[0].total == 2
}

test test_system_report_psi_reference_parses_complete_rows_and_brackets_counters {
  let raw = """some avg10=0.10 avg60=1.25 avg300=2.50 total=10
full avg10=0.00 avg60=0.00 avg300=0.00 total=2
"""
  let before: List[PsiReferenceFixture] = report_checks.parse_psi_reference(raw, "memory")?
  assert before.len() == 2
  assert before[0].total_us == 10
  assert before[1].kind == "full"
  let after = [{...before[0], total_us: 12}, before[1]]
  let candidate = """{"memory":{"pressure":[{"resource":"memory","kind":"full","avg10":"0.00","avg60":"0.00","avg300":"0.00","total_us":2},{"resource":"memory","kind":"some","avg10":"0.10","avg60":"1.25","avg300":"2.50","total_us":11}]}}"""
  let exact = report_checks.compare_psi(candidate, before, after)?
  assert exact.exact_scored
  assert exact.matched_count == 2
  let changing = [{...after[0], avg10: "0.20"}, after[1]]
  let partial = report_checks.compare_psi(candidate, before, changing)?
  assert partial.changing_averages == ["memory.some.avg10"]
  assert partial.exact_stable
  assert ! partial.exact_scored
  let outside = candidate.replace("\"total_us\":11", "\"total_us\":13")
  assert report_checks.compare_psi(outside, before, after)?.mismatched_fields == ["memory.some.total_us"]
  let below = candidate.replace("\"total_us\":11", "\"total_us\":9")
  assert report_checks.compare_psi(below, before, after)?.mismatched_fields == ["memory.some.total_us"]
  let missing_average = candidate.replace("\"avg10\":\"0.10\"", "\"avg10\":null")
  assert report_checks.compare_psi(missing_average, before, after)?.mismatched_fields == ["memory.some.avg10"]
  let missing = """{"memory":{"pressure":[{"resource":"memory","kind":"full","avg10":"0.00","avg60":"0.00","avg300":"0.00","total_us":2}]}}"""
  assert report_checks.compare_psi(missing, before, after)?.missing_keys == ["memory.some"]
  test.error_kind(
    report_checks.compare_psi(candidate, before, [{...before[0], kind: "full"}, before[1]]),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.compare_psi(candidate, before, [{...before[0], total_us: 9}, before[1]]),
    "SystemReportCheckError.Invalid",
  )?
  assert report_checks.compare_psi(candidate, [], [])?.unexpected_keys == ["memory.full", "memory.some"]
  let duplicate = raw.replace(
    """full avg10=0.00 avg60=0.00 avg300=0.00 total=2
""",
    """some avg10=0.00 avg60=0.00 avg300=0.00 total=2
""",
  )
  test.error_kind(report_checks.parse_psi_reference(duplicate, "memory"), "SystemReportCheckError.Invalid")?
  for bad in [
    "",
    """some avg10=nan avg60=0.00 avg300=0.00 total=1
""",
    """some avg10=0.00 avg60=0.00 avg300=0.00 total=0x1
""",
    """some avg10=0.00 avg60=0.00 avg300=0.00 total=1 extra=2
""",
  ] {
    test.error_kind(report_checks.parse_psi_reference(bad, "memory"), "SystemReportCheckError.Invalid")?
  }
}

test test_system_report_pressure_capture_validates_saved_sources_and_oracle {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc/pressure", parents: true)?
  source.write(
    p"proc/pressure/cpu",
    """some avg10=0.10 avg60=0.20 avg300=0.30 total=10
""",
  )?
  source.write(
    p"proc/pressure/memory",
    """some avg10=0.00 avg60=0.00 avg300=0.00 total=20
full avg10=0.00 avg60=0.00 avg300=0.00 total=2
""",
  )?
  report_checks.capture_pressure_bundle(source, bundle, "synthetic_fixture")?
  let reference = report_checks.validate_pressure_bundle(bundle)?
  assert reference.len() == 3
  assert reference[0].resource == "cpu"
  assert reference[2].total_us == 2
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"state\": \"absent\"" in metadata
  source.write(
    p"proc/pressure/cpu",
    """changed
""",
  )?
  assert report_checks.validate_pressure_bundle(bundle)?[0].total_us == 10
  let changed_oracle = json.set(json.decode(metadata)?, ["reference", 0, "total_us"], 11)?
  bundle.write_atomic(p"capture.json", json.encode(changed_oracle)?)?
  test.error_kind(report_checks.validate_pressure_bundle(bundle), "SystemReportCheckError.Invalid")?
  let contradictory = json.set(json.decode(metadata)?, ["sources", 0, "errno"], 13)?
  bundle.write_atomic(p"capture.json", json.encode(contradictory)?)?
  test.error_kind(report_checks.validate_pressure_bundle(bundle), "SystemReportCheckError.Invalid")?
  let error_kind = json.set(json.decode(metadata)?, ["sources", 1, "error_kind"], "permission_denied")?
  bundle.write_atomic(p"capture.json", json.encode(error_kind)?)?
  test.error_kind(report_checks.validate_pressure_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write_atomic(p"capture.json", metadata)?
  bundle.write(
    p"proc/pressure/io",
    """some avg10=0.00 avg60=0.00 avg300=0.00 total=1
""",
  )?
  test.error_kind(report_checks.validate_pressure_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.remove(p"proc/pressure/io")?
  bundle.write(
    p"proc/pressure/cpu",
    """some avg10=0.10 avg60=0.20 avg300=0.30 total=11
""",
  )?
  test.error_kind(report_checks.validate_pressure_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_pressure_capture_keeps_incomplete_sources_unscoreable {
  let source = fs.tempdir()?
  defer source.close()?
  let absent = fs.tempdir()?
  defer absent.close()?
  report_checks.capture_pressure_bundle(source, absent, "synthetic_fixture")?
  test.error_kind(report_checks.validate_pressure_bundle(absent), "SystemReportCheckError.Invalid")?
  source.mkdir(p"proc/pressure", parents: true)?
  source.write(
    p"proc/pressure/cpu",
    """some avg10=nan avg60=0.00 avg300=0.00 total=1
""",
  )?
  let malformed = fs.tempdir()?
  defer malformed.close()?
  report_checks.capture_pressure_bundle(source, malformed, "synthetic_fixture")?
  test.error_kind(report_checks.validate_pressure_bundle(malformed), "SystemReportCheckError.Invalid")?
  var padding = "x"
  while padding.count_chars() <= 16384 {
    padding = f"${padding}${padding}"
  }

  source.write(p"proc/pressure/cpu", padding)?
  let truncated = fs.tempdir()?
  defer truncated.close()?
  report_checks.capture_pressure_bundle(source, truncated, "synthetic_fixture")?
  test.error_kind(report_checks.validate_pressure_bundle(truncated), "SystemReportCheckError.Invalid")?
}

test test_system_report_pressure_capture_replays_production_collector {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc/pressure", parents: true)?
  source.write(
    p"proc/pressure/cpu",
    """some avg10=0.10 avg60=0.20 avg300=0.30 total=10
""",
  )?
  report_checks.capture_pressure_bundle(source, bundle, "synthetic_fixture")?
  assert report_checks.replay_pressure_bundle(bundle)?.exact_scored
}

test test_system_report_process_identity_snapshot_keeps_complete_stable_sources {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)?
  root.mkdir(p"proc/124", parents: true)?
  root.write(
    p"proc/123/stat",
    """123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2
""",
  )?
  root.write(
    p"proc/123/status",
    """Name:	worker
Uid:	1000	1000	1000	1000
""",
  )?
  root.write(
    p"proc/124/stat",
    """124 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 101 8192 2
""",
  )?
  let captured = report_checks.read_process_identity_snapshot(root)?
  assert captured.processes.len() == 1
  assert captured.processes[0].pid == 123
  assert captured.processes[0].uid == 1000
  assert captured.skipped_count == 1
}

test test_system_report_process_resource_snapshot_requires_complete_per_pid_sources {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)?
  root.mkdir(p"proc/124", parents: true)?
  root.write(
    p"proc/123/stat",
    """123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2
""",
  )?
  root.write(
    p"proc/123/statm",
    """2 1 0 0 0 0 0
""",
  )?
  root.write(
    p"proc/123/cgroup",
    """0::/tenant
""",
  )?
  root.write(
    p"proc/124/stat",
    """124 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 101 8192 2
""",
  )?
  root.write(
    p"proc/124/statm",
    """2 1
""",
  )?
  root.write(
    p"proc/124/cgroup",
    """0::/tenant
""",
  )?
  let captured = report_checks.read_process_resource_snapshot(root, 65536)?
  assert captured.processes.len() == 1
  assert captured.processes[0].pid == 123
  assert captured.processes[0].resident_bytes == 65536
  assert captured.processes[0].virtual_bytes == 131072
  assert captured.processes[0].cgroup == "/tenant"
  assert captured.skipped_count == 1
}

test test_system_report_process_bundle_replays_complete_pid_and_records_skips {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc/123", parents: true)?
  source.mkdir(p"proc/124", parents: true)?
  source.mkdir(p"proc/9", parents: true)?
  source.write(
    p"proc/123/stat",
    """123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2
""",
  )?
  source.write(
    p"proc/123/statm",
    """2 1 0 0 0 0 0
""",
  )?
  source.write(
    p"proc/123/status",
    """Name:	worker
Uid:	1000	1000	1000	1000
""",
  )?
  source.write(
    p"proc/123/cgroup",
    """0::/tenant
""",
  )?
  source.write(
    p"proc/9/stat",
    """9 (helper) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 1 0 99 4096 1
""",
  )?
  source.write(
    p"proc/9/statm",
    """1 1 0 0 0 0 0
""",
  )?
  source.write(
    p"proc/9/status",
    """Name:	helper
Uid:	1001	1001	1001	1001
""",
  )?
  source.write(
    p"proc/9/cgroup",
    """0::/tenant
""",
  )?
  source.write(
    p"proc/124/stat",
    """124 (short-lived) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 101 8192 2
""",
  )?
  report_checks.capture_process_bundle(source, bundle, "synthetic_fixture", 4096)?
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"skipped_count\": 1" in metadata
  assert "\"name\": \"124\"" in metadata
  assert "\"source\": \"statm\"" in metadata
  assert "\"state\": \"absent\"" in metadata
  assert ! bundle.exists(p"proc/124")?
  let replay = report_checks.replay_process_bundle(bundle)?
  assert replay.identity.exact_static
  assert replay.resources.exact_scored
  assert replay.identity.matched_count == 2
  bundle.write(
    p"proc/123/statm",
    """3 1 0 0 0 0 0
""",
  )?
  test.error_kind(report_checks.validate_process_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(
    p"proc/123/statm",
    """2 1 0 0 0 0 0
""",
  )?
  bundle.mkdir(p"proc/125")?
  test.error_kind(report_checks.validate_process_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_process_bundle_keeps_absent_proc_unscoreable {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  report_checks.capture_process_bundle(source, bundle, "synthetic_fixture", 4096)?
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"listing_state\": \"absent\"" in metadata
  assert "\"scoreable\": false" in metadata
  test.error_kind(report_checks.validate_process_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_process_identity_reference_excludes_pid_reuse_and_scores_stable_values {
  let stable = {
    pid: 123,
    start_ticks: 100,
    parent_pid: 1,
    uid: 1000,
    command: "worker",
    state: "S",
  }
  let reused_before = {
    pid: 124,
    start_ticks: 200,
    parent_pid: 1,
    uid: 1000,
    command: "old",
    state: "S",
  }
  let reused_after = {...reused_before, start_ticks: 201, command: "new"}
  let candidate = """{"processes":{"processes":[{"pid":123,"parent_pid":1,"uid":1000,"command":{"state":"observed","value":"worker"},"state":"R","start_ticks":100},{"pid":124,"parent_pid":1,"uid":1000,"command":{"state":"observed","value":"new"},"state":"S","start_ticks":201}]}}"""
  let compared = report_checks.compare_process_identity(candidate, [stable, reused_before], [stable, reused_after])?
  assert compared.stable_count == 1
  assert compared.unstable_count == 1
  assert compared.matched_count == 1
  assert compared.missing_pids == []
  assert compared.mismatched_pids == []
  assert compared.state_unscored_count == 1
  assert compared.exact_static

  let wrong_uid = candidate.replace("\"uid\":1000", "\"uid\":1001")
  let changed = report_checks.compare_process_identity(wrong_uid, [stable], [stable])?
  assert changed.mismatched_pids == [123]
  assert ! changed.exact_static
  let wrong_start = candidate.replace("\"start_ticks\":100", "\"start_ticks\":101")
  let missing = report_checks.compare_process_identity(wrong_start, [stable], [stable])?
  assert missing.missing_pids == [123]
  assert ! missing.exact_static
  test.error_kind(
    report_checks.compare_process_identity(candidate, [stable, stable], [stable]),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.compare_process_identity(candidate, [{...stable, parent_pid: -1}], [stable]),
    "SystemReportCheckError.Invalid",
  )?
  assert ! report_checks.compare_process_identity(candidate, [], [])?.exact_static
}

test test_system_report_process_reference_excludes_exits_and_new_arrivals_from_static_scoring {
  let stable = {
    pid: 123,
    start_ticks: 100,
    parent_pid: 1,
    uid: 1000,
    command: "worker",
    state: "S",
  }
  let exited = {...stable, pid: 124, start_ticks: 200, command: "short-lived"}
  let arrived = {...stable, pid: 125, start_ticks: 300, command: "new"}
  let candidate = """{"processes":{"processes":[{"pid":123,"parent_pid":1,"uid":1000,"command":{"state":"observed","value":"worker"},"state":"R","start_ticks":100},{"pid":124,"parent_pid":1,"uid":1000,"command":{"state":"observed","value":"short-lived"},"state":"S","start_ticks":200},{"pid":125,"parent_pid":1,"uid":1000,"command":{"state":"observed","value":"new"},"state":"S","start_ticks":300}]}}"""
  let compared = report_checks.compare_process_identity(candidate, [stable, exited], [stable, arrived])?
  assert compared.stable_count == 1
  assert compared.unstable_count == 1
  assert compared.candidate_count == 3
  assert compared.matched_count == 1
  assert compared.missing_pids == []
  assert compared.mismatched_pids == []
  assert compared.exact_static

  let only_exited = report_checks.compare_process_identity(candidate, [exited], [arrived])?
  assert only_exited.stable_count == 0
  assert only_exited.unstable_count == 1
  assert ! only_exited.exact_static

  let stable_resources = {
    pid: 123,
    start_ticks: 100,
    thread_count: 2,
    resident_bytes: 65536,
    virtual_bytes: 131072,
    cgroup: "/tenant",
  }
  let exited_resources = {...stable_resources, pid: 124, start_ticks: 200}
  let arrived_resources = {...stable_resources, pid: 125, start_ticks: 300}
  let resource_candidate = """{"processes":{"processes":[{"pid":123,"start_ticks":100,"thread_count":2,"resident_bytes":65536,"virtual_bytes":131072,"cgroup":{"state":"observed","value":"/tenant"}},{"pid":124,"start_ticks":200,"thread_count":2,"resident_bytes":65536,"virtual_bytes":131072,"cgroup":{"state":"observed","value":"/tenant"}},{"pid":125,"start_ticks":300,"thread_count":2,"resident_bytes":65536,"virtual_bytes":131072,"cgroup":{"state":"observed","value":"/tenant"}}]}}"""
  let resources = report_checks.compare_process_resources(
    resource_candidate,
    [stable_resources, exited_resources],
    [stable_resources, arrived_resources],
  )?
  assert resources.stable_count == 1
  assert resources.unstable_count == 1
  assert resources.candidate_count == 3
  assert resources.scored_fields == 4
  assert resources.missing_pids == []
  assert resources.mismatched_fields == []
  assert resources.exact_scored
}

test test_system_report_traced_live_and_replay_paths_keep_host_effect_contract { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("the production syscall audit requires Linux strace")
    return
  }

  let script = fp"${ctx.core_dir.parent()}/core/system-report.xsh"
  report_checks.audit_no_subprocess(ctx.xsh_bin.display(), script.display())?
}

test test_system_report_process_resource_reference_scores_only_stable_fields {
  let stable = {
    pid: 123,
    start_ticks: 100,
    thread_count: 2,
    resident_bytes: 65536,
    virtual_bytes: 131072,
    cgroup: "/tenant",
  }
  let reused = {...stable, pid: 124, start_ticks: 200}
  let reused_after = {...reused, start_ticks: 201}
  let candidate = """{"processes":{"processes":[{"pid":123,"start_ticks":100,"thread_count":2,"resident_bytes":65536,"virtual_bytes":131072,"cgroup":{"state":"observed","value":"/tenant"}},{"pid":124,"start_ticks":201,"thread_count":2,"resident_bytes":65536,"virtual_bytes":131072,"cgroup":{"state":"observed","value":"/tenant"}}]}}"""
  let exact = report_checks.compare_process_resources(candidate, [stable, reused], [stable, reused_after])?
  assert exact.stable_count == 1
  assert exact.unstable_count == 1
  assert exact.scored_fields == 4
  assert exact.unscored_fields == []
  assert exact.mismatched_fields == []
  assert exact.exact_scored

  let moved = {...stable, resident_bytes: 131072}
  let changed = report_checks.compare_process_resources(candidate, [stable], [moved])?
  assert changed.scored_fields == 3
  assert changed.unscored_fields == ["123.resident_bytes"]
  assert changed.exact_scored

  let wrong = candidate.replace("\"virtual_bytes\":131072", "\"virtual_bytes\":262144")
  let mismatch = report_checks.compare_process_resources(wrong, [stable], [stable])?
  assert mismatch.mismatched_fields == ["123.virtual_bytes"]
  assert ! mismatch.exact_scored
  let reused_candidate = candidate.replace("\"start_ticks\":100", "\"start_ticks\":101")
  let missing = report_checks.compare_process_resources(reused_candidate, [stable], [stable])?
  assert missing.missing_pids == [123]
  test.error_kind(
    report_checks.compare_process_resources(candidate, [stable, stable], [stable]),
    "SystemReportCheckError.Invalid",
  )?
  assert ! report_checks.compare_process_resources(candidate, [], [])?.exact_scored
}

test test_system_report_rust_fixture_argv_accepts_target_override {
  let argv = report_checks.rust_fixture_argv_for_target(
    "/opt/cargo",
    "src/modules/linux/real/netlink.rs::dump_accumulator_reads_multipart_messages_across_datagrams",
    "x86_64-unknown-linux-musl",
  )?
  assert argv[7] == "x86_64-unknown-linux-musl"
}

test test_system_report_coverage_manifest_contract {
  let required = assertion("cpu.policy.related-cpus", "mandatory")
  let supplemental = assertion("cpu.policy.governor", "supplemental")
  let valid = manifest([required, supplemental])

  report_checks.validate(valid)?
  test.error_kind(report_checks.validate({...valid, schema_version: 3}), "SystemReportCheckError.Invalid")?
  let rooted_process = {
    ...required,
    id: "process.identity",
    domain: "process",
    reference_adapter: "procfs-rooted-v1",
    reference_commands: [],
  }
  report_checks.validate(manifest([rooted_process]))?
  let rooted_huge_pages = {
    ...required,
    id: "memory.hugepages",
    domain: "memory",
    reference_adapter: "hugepage-sysfs-rooted-v1",
    reference_commands: [],
  }
  report_checks.validate(manifest([rooted_huge_pages]))?
  test.error_kind(
    report_checks.validate(manifest([{...rooted_huge_pages, reference_adapter: "hugepage-sysfs"}])),
    "SystemReportCheckError.Invalid",
  )?
  let rooted_cpufreq = {
    ...required,
    id: "cpu.freq-policies",
    reference_adapter: "cpufreq-sysfs-rooted-v1",
    reference_commands: [],
  }
  report_checks.validate(manifest([rooted_cpufreq]))?
  test.error_kind(
    report_checks.validate(manifest([{...rooted_cpufreq, reference_commands: [["cpupower", "frequency-info"]]}])),
    "SystemReportCheckError.Invalid",
  )?
  let rooted_controls = {...rooted_cpufreq, id: "cpu.epp-boost"}
  report_checks.validate(manifest([rooted_controls]))?
  test.error_kind(
    report_checks.validate(
      manifest([{...rooted_controls, reference_adapter: "cpupower", reference_commands: [["cpupower", "frequency-info"]]}]),
    ),
    "SystemReportCheckError.Invalid",
  )?
  let rooted_usb = {
    ...required,
    id: "usb.topology",
    domain: "usb",
    reference_adapter: "usb-topology-sysfs-rooted-v1",
    reference_commands: [],
  }
  report_checks.validate(manifest([rooted_usb]))?
  test.error_kind(
    report_checks.validate(manifest([{...rooted_usb, reference_commands: [["lsusb", "-t"]]}])),
    "SystemReportCheckError.Invalid",
  )?
  let rooted_usb_ids = {...rooted_usb, id: "usb.ids", reference_adapter: "usb-ids-sysfs-rooted-v1"}
  report_checks.validate(manifest([rooted_usb_ids]))?
  test.error_kind(
    report_checks.validate(manifest([{...rooted_usb_ids, reference_commands: [["lsusb", "-v"]]}])),
    "SystemReportCheckError.Invalid",
  )?
  let rooted_usb_power = {...rooted_usb, id: "usb.power", reference_adapter: "usb-power-sysfs-rooted-v1"}
  report_checks.validate(manifest([rooted_usb_power]))?
  test.error_kind(
    report_checks.validate(manifest([{...rooted_usb_power, reference_commands: [["find", "/sys/bus/usb/devices"]]}])),
    "SystemReportCheckError.Invalid",
  )?
  let rooted_devices = {
    ...rooted_usb,
    id: "devices.graphics-audio-input",
    domain: "devices",
    reference_adapter: "device-class-sysfs-rooted-v1",
  }
  report_checks.validate(manifest([rooted_devices]))?
  test.error_kind(
    report_checks.validate(manifest([{...rooted_devices, reference_adapter: "sysfs-device-classes"}])),
    "SystemReportCheckError.Invalid",
  )?
  let rooted_hwmon = {...rooted_usb, id: "sensors.hwmon", domain: "sensors", reference_adapter: "hwmon-sysfs-rooted-v1"}
  report_checks.validate(manifest([rooted_hwmon]))?
  test.error_kind(
    report_checks.validate(manifest([{...rooted_hwmon, reference_adapter: "lm-sensors"}])),
    "SystemReportCheckError.Invalid",
  )?
  let sensors_json = {
    ...rooted_hwmon,
    id: "sensors.lm-sensors",
    tier: "supplemental",
    reference_adapter: "sensors-json-v1",
    reference_commands: [
      [
        "sensors",
        "-v",
      ],
      [
        "sensors",
        "-j",
        "-c",
        "/dev/null",
      ],
    ],
  }
  report_checks.validate(manifest([rooted_hwmon, sensors_json]))?
  test.error_kind(
    report_checks.validate(manifest([rooted_hwmon, {...sensors_json, reference_commands: [["sensors", "-j"]]}])),
    "SystemReportCheckError.Invalid",
  )?
  let rooted_smbios = {
    ...rooted_usb,
    id: "firmware.smbios",
    domain: "firmware",
    reference_adapter: "smbios-raw-rooted-v1",
  }
  report_checks.validate(manifest([rooted_smbios]))?
  test.error_kind(
    report_checks.validate(manifest([{...rooted_smbios, reference_commands: [["dmidecode", "--type", "17"]]}])),
    "SystemReportCheckError.Invalid",
  )?
  let dmidecode = {
    ...rooted_smbios,
    id: "firmware.dmidecode",
    tier: "supplemental",
    reference_adapter: "dmidecode-hex-v1",
    reference_commands: [
      [
        "dmidecode",
        "--version",
      ],
      [
        "dmidecode",
        "--no-quirks",
        "--dump",
        "--from-dump",
        "<captured SMBIOS dump>",
      ],
    ],
  }
  report_checks.validate(manifest([rooted_smbios, dmidecode]))?
  test.error_kind(
    report_checks.validate(
      manifest([rooted_smbios, {...dmidecode, reference_commands: [["dmidecode", "--from-dump", "<captured SMBIOS dump>"]]}]),
    ),
    "SystemReportCheckError.Invalid",
  )?
  let rooted_idle = {...rooted_cpufreq, id: "cpu.idle", reference_adapter: "cpuidle-sysfs-rooted-v1"}
  report_checks.validate(manifest([rooted_idle]))?
  test.error_kind(
    report_checks.validate(
      manifest([{...rooted_idle, reference_adapter: "cpupower", reference_commands: [["cpupower", "idle-info"]]}]),
    ),
    "SystemReportCheckError.Invalid",
  )?
  let topology = {
    ...required,
    id: "cpu.topology",
    reference_adapter: "lscpu",
    reference_commands: [
      [
        "lscpu",
        "--json",
        "--extended=CPU,ONLINE,SOCKET,CORE,NODE",
        "--all",
      ],
    ],
  }
  report_checks.validate(manifest([topology]))?
  test.error_kind(
    report_checks.validate(manifest([{...topology, reference_commands: [["lscpu", "--json"]]}])),
    "SystemReportCheckError.Invalid",
  )?
  let cache_sharing = {
    ...required,
    id: "cpu.cache-sharing",
    reference_adapter: "cache-sysfs-rooted-v1",
    reference_commands: [],
  }
  report_checks.validate(manifest([cache_sharing]))?
  test.error_kind(
    report_checks.validate(manifest([{...cache_sharing, reference_adapter: "lscpu"}])),
    "SystemReportCheckError.Invalid",
  )?
  let pressure_commands = [
    [
      "/bin/cat",
      "/proc/pressure/cpu",
    ],
    [
      "/bin/cat",
      "/proc/pressure/memory",
    ],
    [
      "/bin/cat",
      "/proc/pressure/io",
    ],
  ]
  let pressure = {
    ...required,
    id: "memory.pressure",
    domain: "memory",
    reference_adapter: "pressure-procfs-v1",
    reference_commands: pressure_commands,
  }
  report_checks.validate(manifest([pressure]))?
  test.error_kind(
    report_checks.validate(manifest([{...pressure, reference_adapter: "pressure-procfs"}])),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(manifest([{...pressure, reference_commands: [pressure_commands[1]]}])),
    "SystemReportCheckError.Invalid",
  )?
  let cpu_scope_command = ["/bin/cat", "/proc/self/status"]
  let cpu_scope = {
    ...required,
    id: "memory.cpu-scope",
    domain: "memory",
    reference_adapter: "cpu-scope-procfs-cgroup-v1",
    reference_commands: [
      cpu_scope_command,
    ],
  }
  report_checks.validate(manifest([cpu_scope]))?
  test.error_kind(
    report_checks.validate(manifest([{...cpu_scope, reference_adapter: "taskset"}])),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(manifest([{...cpu_scope, reference_commands: [["taskset", "-pc", "self"]]}])),
    "SystemReportCheckError.Invalid",
  )?
  let cgroup = {
    ...required,
    id: "memory.cgroup-v2",
    domain: "memory",
    reference_adapter: "cgroup-v2-rooted-v1",
    reference_commands: [],
  }
  report_checks.validate(manifest([cgroup]))?
  test.error_kind(
    report_checks.validate(manifest([{...cgroup, reference_adapter: "cgroup-v2-files"}])),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(manifest([{...cgroup, reference_commands: [["cat", "/proc/self/cgroup"]]}])),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(manifest([{...required, reference_commands: []}])),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(manifest([{...required, reference_adapter: "procfs-rooted-v1", reference_commands: []}])),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(manifest([{...required, reference_commands: [[]]}])),
    "SystemReportCheckError.Invalid",
  )?
  let route_commands = [
    [
      "ip",
      "-json",
      "-family",
      "inet",
      "route",
      "show",
      "table",
      "all",
    ],
    [
      "ip",
      "-json",
      "-family",
      "inet6",
      "route",
      "show",
      "table",
      "all",
    ],
  ]
  let rule_commands = [
    [
      "ip",
      "-json",
      "-family",
      "inet",
      "rule",
      "show",
    ],
    [
      "ip",
      "-json",
      "-family",
      "inet6",
      "rule",
      "show",
    ],
  ]
  let link = {
    ...required,
    id: "network.links",
    domain: "network",
    reference_adapter: "iproute2+network-link-sysfs-rooted-v1",
    reference_commands: [
      [
        "ip",
        "-json",
        "-details",
        "link",
        "show",
      ],
    ],
  }
  let address = {
    ...required,
    id: "network.addresses",
    domain: "network",
    reference_adapter: "iproute2",
    reference_commands: [
      [
        "ip",
        "-json",
        "address",
        "show",
      ],
    ],
  }
  let route = {
    ...required,
    id: "network.routes",
    domain: "network",
    reference_adapter: "iproute2",
    reference_commands: route_commands,
  }
  let rule = {
    ...required,
    id: "network.rules",
    domain: "network",
    reference_adapter: "iproute2",
    reference_commands: rule_commands,
  }
  report_checks.validate(manifest([link, address, route, rule]))?
  test.error_kind(
    report_checks.validate(manifest([{...link, reference_adapter: "iproute2"}])),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(manifest([{...link, reference_commands: [["ip", "-json", "link", "show"]]}])),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(manifest([{...address, reference_commands: [["ip", "address", "show"]]}])),
    "SystemReportCheckError.Invalid",
  )?
  let vulnerabilities = {
    ...required,
    id: "cpu.vulnerabilities",
    domain: "cpu",
    reference_adapter: "kernel-vulnerability-files",
    reference_commands: [
      [
        "/bin/cat",
        "/sys/devices/system/cpu/vulnerabilities/<name>",
      ],
    ],
  }
  report_checks.validate(manifest([vulnerabilities]))?
  test.error_kind(
    report_checks.validate(
      manifest([{...vulnerabilities, reference_commands: [["cat", "/sys/devices/system/cpu/vulnerabilities/spectre_v1"]]}]),
    ),
    "SystemReportCheckError.Invalid",
  )?
  let pci_command = ["lspci", "-D", "-vmm", "-n", "-k"]
  let pci_identity = {
    ...required,
    id: "pci.identity",
    domain: "pci",
    reference_adapter: "lspci",
    reference_commands: [
      pci_command,
    ],
  }
  let pci_binding = {
    ...pci_identity,
    id: "pci.binding",
    reference_adapter: "pci-binding-sysfs-rooted-v1",
    reference_commands: [],
  }
  report_checks.validate(manifest([pci_identity, pci_binding]))?
  let pci_link = {
    ...pci_identity,
    id: "pci.link",
    reference_adapter: "pci-link-sysfs-rooted-v1",
    reference_commands: [],
  }
  report_checks.validate(manifest([pci_link]))?
  let thermal = {
    ...pci_identity,
    id: "sensors.thermal",
    domain: "sensors",
    reference_adapter: "thermal-sysfs-rooted-v1",
    reference_commands: [],
  }
  report_checks.validate(manifest([thermal]))?
  test.error_kind(
    report_checks.validate(
      manifest([{...thermal, reference_adapter: "thermal-sysfs", reference_commands: [["find", "/sys/class/thermal"]]}]),
    ),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(
      manifest([{...pci_link, reference_adapter: "lspci-detail", reference_commands: [["lspci", "-D", "-vv"]]}]),
    ),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(manifest([{...pci_identity, reference_adapter: "pci-sysfs"}])),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(manifest([{...pci_identity, reference_commands: [["lspci", "-D", "-vmm", "-n"]]}])),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(manifest([{...pci_binding, reference_adapter: "lspci", reference_commands: [pci_command]}])),
    "SystemReportCheckError.Invalid",
  )?
  let trace_command = [
    "strace",
    "-f",
    "-qq",
    "-yy",
    "-s",
    "4096",
    "-e",
    "trace=process,network,file,init_module,finit_module,delete_module,swapon,swapoff,write,writev,pwrite64,pwritev,pwritev2,utimensat,fchmodat2,fallocate,copy_file_range,sendfile,splice,vmsplice,tee,ioctl,setuid,setgid,setreuid,setregid,setresuid,setresgid,setfsuid,setfsgid,capset,unshare,setns,chroot,pivot_root,reboot,kexec_load,kexec_file_load,sethostname,setdomainname,clock_settime,settimeofday,clock_adjtime,adjtimex",
    "-o",
    "<trace file>",
    "--",
    "<xsh binary>",
    "<system-report script>",
    "--",
    "--json",
  ]
  let trace_assertion = {
    ...required,
    id: "safety.no-child",
    domain: "safety",
    reference_adapter: "strace",
    reference_commands: [
      trace_command,
    ],
  }
  report_checks.validate(manifest([trace_assertion]))?
  test.error_kind(
    report_checks.validate(
      manifest([{...trace_assertion, reference_commands: [["strace", "-f", "-qq", "system-report", "--json"]]}]),
    ),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(manifest([{...trace_assertion, reference_adapter: "shell-trace"}])),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(manifest([{...route, reference_commands: [route_commands[0]]}])),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(manifest([{...rule, reference_commands: [rule_commands[1]]}])),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(manifest([{...route, reference_commands: [route_commands[0], route_commands[0]]}])),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(report_checks.validate({...valid, schema_version: 1}), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.validate({...valid, schema_version: 2}), "SystemReportCheckError.Invalid")?
  let summary = report_checks.summary(valid.assertions)
  assert "cpu: 1 mandatory, 1 supplemental" in summary
  assert "total: 1 mandatory, 1 supplemental" in summary

  let duplicate = manifest([required, required])
  test.error_kind(report_checks.validate(duplicate), "SystemReportCheckError.Invalid")?

  let no_required_cases = manifest([])
  test.error_kind(report_checks.validate(no_required_cases), "SystemReportCheckError.Invalid")?

  let undeclared_scenario = manifest([{...required, fixture_scenarios: ["not-in-manifest"]}])
  test.error_kind(report_checks.validate(undeclared_scenario), "SystemReportCheckError.Invalid")?

  let mapped = {
    ...valid,
    fixture_cases: [
      {
        scenario: "offline_related_cpu",
        tests: [
          "tests/xsh/system-report.xsh::test_system_report_cpu_collection_keeps_sparse_models_policies_and_cpuset",
        ],
      },
    ],
  }
  report_checks.validate(mapped)?
  let macos_mapped = {
    ...valid,
    macos_fixture_cases: [
      {
        scenario: "offline_related_cpu",
        tests: [
          "tests/xsh/system-report.xsh::test_system_report_command_replays_saved_json_offline",
        ],
      },
    ],
  }
  report_checks.validate(macos_mapped)?
  test.error_kind(
    report_checks.validate({...macos_mapped, fixture_cases: mapped.fixture_cases}),
    "SystemReportCheckError.Invalid",
  )?
  report_checks.validate({
    ...valid,
    fixture_cases: [{
    scenario: "offline_related_cpu",
    tests: ["dev/tests/test-system-report-check.xsh::test_system_report_cpu_set_comparison_scores_all_four_kernel_sets"],
  }],
  })?
  report_checks.validate(
    {
      ...valid,
      fixture_cases: [
        {
          scenario: "offline_related_cpu",
          tests: [
            "tests/xsh/system-report-collect.xsh::test_system_report_unified_cgroup_path_preserves_name_and_rejects_ambiguous_rows",
          ],
        },
      ],
    },
  )?
  report_checks.validate({
    ...valid,
    fixture_cases: [{
    scenario: "offline_related_cpu",
    tests: ["src/modules/linux/real/netlink.rs::dump_accumulator_reads_multipart_messages_across_datagrams"],
  }],
  })?
  report_checks.validate({
    ...valid,
    fixture_cases: [{
    scenario: "offline_related_cpu",
    tests: ["tests/linux_priv.rs::system_report_sysctl_denial_as_unprivileged_reader_creates_no_child"],
  }],
  })?
  test.error_kind(
    report_checks.validate({
      ...valid,
      fixture_cases: [{
      scenario: "offline_related_cpu",
      tests: ["dev/tests/unrelated.xsh::test_system_report_cpu_set_comparison_scores_all_four_kernel_sets"],
    }],
    }),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate({...mapped, fixture_cases: mapped.fixture_cases.extend(mapped.fixture_cases)}),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate({...valid, fixture_cases: [{scenario: "not-in-manifest", tests: mapped.fixture_cases[0].tests}]}),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate({...valid, fixture_cases: [{scenario: "offline_related_cpu", tests: []}]}),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(
      {
        ...valid,
        fixture_cases: [
          {
            scenario: "offline_related_cpu",
            tests: [
              "tests/xsh/system-report.xsh::test_system_report_cpu_collection_keeps_sparse_models_policies_and_cpuset",
              "tests/xsh/system-report.xsh::test_system_report_cpu_collection_keeps_sparse_models_policies_and_cpuset",
            ],
          },
        ],
      },
    ),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.validate(
      {
        ...valid,
        fixture_cases: [
          {
            scenario: "offline_related_cpu",
            tests: [
              "tests/xsh/system-report.xsh::test_system_report_cpu_collection_keeps_sparse_models_policies_and_cpuset::extra",
            ],
          },
        ],
      },
    ),
    "SystemReportCheckError.Invalid",
  )?

  assert report_checks.fixture_single_test_passed("""running 1 tests
test result: ok. 1 passed; 0 failed; 0 skipped
""")
  assert ! report_checks.fixture_single_test_passed("""running 0 tests
test result: ok. 0 passed; 0 failed; 0 skipped
""")
  assert ! report_checks.fixture_single_test_passed("""running 2 tests
test result: ok. 2 passed; 0 failed; 0 skipped
""")
  assert ! report_checks.fixture_single_test_passed("""running 1 tests
test result: ok. 0 passed; 0 failed; 1 skipped
""")
  let rust_fixture = "src/modules/linux/real/netlink.rs::dump_accumulator_reads_multipart_messages_across_datagrams"
  assert report_checks.rust_fixture_argv("/opt/cargo", rust_fixture)? == [
    "/opt/cargo",
    "test",
    "--offline",
    "-p",
    "xsh",
    "--lib",
    "--target",
    "aarch64-unknown-linux-musl",
    "modules::linux::real::netlink::tests::dump_accumulator_reads_multipart_messages_across_datagrams",
    "--",
    "--exact",
    "--test-threads=1",
  ]
  assert report_checks.rust_fixture_argv(
    "/opt/cargo",
    "tests/linux_priv.rs::system_report_sysctl_denial_as_unprivileged_reader_creates_no_child",
  )? == [
    "/opt/cargo",
    "test",
    "--offline",
    "-p",
    "xsh",
    "--test",
    "linux_priv",
    "--features",
    "linux-priv-tests",
    "--target",
    "aarch64-unknown-linux-musl",
    "system_report_sysctl_denial_as_unprivileged_reader_creates_no_child",
    "--",
    "--exact",
    "--test-threads=1",
  ]
  test.error_kind(
    report_checks.rust_fixture_argv("/opt/cargo", "src/modules/linux/real/netlink.rs::missing::extra"),
    "SystemReportCheckError.Invalid",
  )?
  assert report_checks.rust_fixture_single_test_passed("""running 1 test
test modules::linux::real::netlink::tests::example ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 30 filtered out; finished in 0.00s
""")
  assert ! report_checks.rust_fixture_single_test_passed("""running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 30 filtered out
""")
  assert ! report_checks.rust_fixture_single_test_passed("""running 1 test
test result: ok. 0 passed; 1 failed; 0 ignored; 0 measured; 30 filtered out
""")

  let thread_trace = """execve("/target/xsh", ["xsh"], 0x0) = 0
clone3({flags=CLONE_VM|CLONE_FS|CLONE_FILES|CLONE_SIGHAND|CLONE_THREAD}, 88) = 42
"""
  assert report_checks.process_trace_violations(thread_trace) == []

  let child_trace = """execve("/target/xsh", ["xsh"], 0x0) = 0
clone3({flags=CLONE_VM|CLONE_VFORK}, 88) = 42
execve("/bin/sh", ["sh"], 0x0) = 0
"""
  let violations = report_checks.process_trace_violations(child_trace)
  assert "process clone syscall" in violations
  assert "secondary exec syscall" in violations
  assert report_checks.process_trace_violations("") == ["initial XSH exec was not traced"]

  let read_only_trace = """42 openat2(3</>, "proc/cpuinfo", {flags=O_RDONLY|O_CLOEXEC}, 24) = 4
42 socket(AF_NETLINK, SOCK_RAW|SOCK_CLOEXEC, NETLINK_ROUTE) = 4
42 sendto(4, [{nlmsg_type=RTM_GETLINK}], 32, 0, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=00000000}, 12) = 32
"""
  assert report_checks.host_effect_trace_violations(read_only_trace).len() == 0
  assert report_checks.host_effect_trace_violations(
    "42 socket(AF_INET, SOCK_DGRAM|SOCK_CLOEXEC, IPPROTO_UDP) = -1 EPERM",
  ) == ["external network socket"]
  assert report_checks.host_effect_trace_violations("42 socket(AF_UNIX, SOCK_STREAM|SOCK_CLOEXEC, 0) = 4") == [
    "unexpected socket family",
  ]
  assert report_checks.host_effect_trace_violations("42 socket(AF_NETLINK, SOCK_RAW|SOCK_CLOEXEC, NETLINK_GENERIC) = 4") == [
    "unexpected netlink protocol",
  ]
  assert report_checks.host_effect_trace_violations(
    "42 socket(AF_NETLINK, SOCK_STREAM|SOCK_CLOEXEC, NETLINK_ROUTE) = -1 EPROTONOSUPPORT",
  ) == ["unexpected netlink socket type"]
  assert report_checks.host_effect_trace_violations("42 socket(AF_NETLINK, SOCK_RAW|SOCK_CLOEXEC, NETLINK_ROUTE) = 4") == []
  assert report_checks.host_effect_trace_violations("42 socketpair(AF_UNIX, SOCK_STREAM, 0, [4, 5]) = 0") == [
    "unexpected socket pair",
  ]
  assert report_checks.host_effect_trace_violations("42 listen(4, 1) = -1 EACCES") == ["unexpected network listener"]
  assert report_checks.host_effect_trace_violations("42 accept4(4, NULL, NULL, SOCK_CLOEXEC) = -1 EAGAIN") == [
    "unexpected network accept",
  ]
  let mixed_queries = "42 sendto(4, [{nlmsg_type=RTM_GETLINK}, {nlmsg_type=RTM_GETADDR}], 64, 0, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=00000000}, 12) = 64"
  assert report_checks.host_effect_trace_violations(mixed_queries) == []
  let userspace_route_socket = "42 sendto(4, [{nlmsg_type=RTM_GETLINK}], 32, 0, {sa_family=AF_NETLINK, nl_pid=123, nl_groups=00000000}, 12) = 32"
  assert report_checks.host_effect_trace_violations(userspace_route_socket) == ["non-kernel netlink destination"]
  let multicast_route_socket = "42 sendto(4, [{nlmsg_type=RTM_GETLINK}], 32, 0, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=00000001}, 12) = 32"
  assert report_checks.host_effect_trace_violations(multicast_route_socket) == ["non-kernel netlink destination"]
  let unknown_mixed_request = "42 sendto(4, [{nlmsg_type=RTM_GETLINK}, {nlmsg_type=RTM_UNKNOWN}], 64, 0, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=00000000}, 12) = 64"
  assert report_checks.host_effect_trace_violations(unknown_mixed_request) == ["non-query netlink request"]
  let numeric_mixed_request = "42 sendto(4, [{nlmsg_type=RTM_GETLINK}, {nlmsg_type=0x12}], 64, 0, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=00000000}, 12) = 64"
  assert report_checks.host_effect_trace_violations(numeric_mixed_request) == ["non-query netlink request"]
  let abbreviated_request = "42 sendto(4, [{nlmsg_type=RTM_GETLINK}, ...], 64, 0, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=00000000}, 12) = 64"
  assert report_checks.host_effect_trace_violations(abbreviated_request) == ["non-query netlink request"]

  let violating_trace = """42 openat2(3, "sys/kernel/test", {flags=O_WRONLY|O_CLOEXEC}, 24) = -1 EACCES
42 unlinkat(3, "file", 0) = -1 EPERM
42 connect(4, {sa_family=AF_INET, sin_port=htons(53)}, 16) = -1 ENETUNREACH
42 sendto(4, [{nlmsg_type=RTM_GETLINK}, {nlmsg_type=RTM_SETLINK}], 64, 0, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=00000000}, 12) = -1 EPERM
42 sendmsg(4, {msg_name={nl_family=AF_NETLINK}, msg_iov=[{iov_base={nlmsg_type=RTM_NEWROUTE}}]}, 0) = -1 EPERM
"""
  let host_violations = report_checks.host_effect_trace_violations(violating_trace)
  assert "writable file open" in host_violations
  assert "system mutation syscall unlinkat" in host_violations
  assert "external network syscall" in host_violations
  assert "non-query netlink request" in host_violations
  let sendmsg_trace = "42 sendmsg(4, {msg_name={nl_family=AF_NETLINK}, msg_iov=[{iov_base={nlmsg_type=RTM_NEWROUTE}}]}, 0) = -1 EPERM"
  assert report_checks.host_effect_trace_violations(sendmsg_trace) == ["unexpected network send"]
  let packet_send_trace = "42 sendto(4, \"probe\", 5, 0, {sa_family=AF_PACKET, sll_protocol=htons(0x0800)}, 20) = 5"
  assert "external network syscall" in report_checks.host_effect_trace_violations(packet_send_trace)
  let local_helper_trace = "42 sendto(4, \"query\", 5, 0, {sa_family=AF_UNIX, sun_path=\"/var/run/nscd/socket\"}, 110) = 5"
  assert report_checks.host_effect_trace_violations(local_helper_trace) == ["unexpected network send"]
  let attempted_local_helper = "42 connect(4, {sa_family=AF_UNIX, sun_path=\"/var/run/nscd/socket\"}, 110) = -1 ENOENT"
  assert report_checks.host_effect_trace_violations(attempted_local_helper) == ["local socket connection"]
  let inherited_socket_trace = "42 sendmsg(7, {msg_iov=[{iov_base=\"probe\", iov_len=5}]}, 0) = 5"
  assert report_checks.host_effect_trace_violations(inherited_socket_trace) == ["unexpected network send"]
  let spoofed_query_trace = "42 sendmsg(7, {msg_name={sa_family=AF_UNIX, sun_path=\"/tmp/helper\"}, msg_iov=[{iov_base={nlmsg_type=RTM_GETLINK}}]}, 0) = 32"
  assert report_checks.host_effect_trace_violations(spoofed_query_trace) == ["unexpected network send"]
  let targetless_query_trace = "42 sendto(7, [{nlmsg_type=RTM_GETLINK}], 32, 0, NULL, 0) = 32"
  assert report_checks.host_effect_trace_violations(targetless_query_trace) == ["unexpected network send"]
  let forged_sendmsg_target = "42 sendmsg(7, {msg_name={sa_family=AF_UNIX, sun_path=\"/tmp/helper\"}, msg_iov=[{iov_base={sa_family=AF_NETLINK, nlmsg_type=RTM_GETLINK}}]}, 0) = 32"
  assert report_checks.host_effect_trace_violations(forged_sendmsg_target) == ["unexpected network send"]
  let forged_sendto_target = "42 sendto(7, [{sa_family=AF_NETLINK, nlmsg_type=RTM_GETLINK}], 32, 0, {sa_family=AF_UNIX, sun_path=\"/tmp/helper\"}, 110) = 32"
  assert report_checks.host_effect_trace_violations(forged_sendto_target) == ["unexpected network send"]
  let targetless_sendmsg_query = "42 sendmsg(7, {msg_name=NULL, msg_iov=[{iov_base={nlmsg_type=RTM_GETLINK}}]}, 0) = 32"
  assert report_checks.host_effect_trace_violations(targetless_sendmsg_query) == ["unexpected network send"]
  let quoted_query_marker = "42 sendto(4, \"nlmsg_type=RTM_GETLINK,\", 23, 0, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=00000000}, 12) = 23"
  assert report_checks.host_effect_trace_violations(quoted_query_marker) == ["non-query netlink request"]
  let abbreviated_sendto = "42 sendto(4, [{nlmsg_type=RTM_GETLINK}], 32, 0, {sa_family=AF_NETLINK, nl_pid=0}, ..."
  assert report_checks.host_effect_trace_violations(abbreviated_sendto) == ["unexpected network send"]
  let local_bind = "42 bind(7, {sa_family=AF_UNIX, sun_path=\"/tmp/helper\"}, 110) = 0"
  assert report_checks.host_effect_trace_violations(local_bind) == ["unexpected network bind"]
  let forged_bind_target = "42 bind(7, {sa_family=AF_UNIX, sun_path=\"/tmp/sa_family=AF_NETLINK\"}, 110) = 0"
  assert report_checks.host_effect_trace_violations(forged_bind_target) == ["unexpected network bind"]
  let route_bind = "42 bind(7, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=0}, 12) = 0"
  assert report_checks.host_effect_trace_violations(route_bind) == []
  let netlink_connect = "42 connect(7, {sa_family=AF_NETLINK, nl_pid=0}, 12) = 0"
  assert report_checks.host_effect_trace_violations(netlink_connect) == ["unexpected network connection"]
  let output_writes = """42 write(1, "report", 6) = 6
42 writev(2, [{iov_base="error", iov_len=5}], 1) = 5"""
  assert report_checks.host_effect_trace_violations(output_writes) == []
  let inherited_write = "42 write(3, \"unexpected\", 10) = 10"
  assert report_checks.host_effect_trace_violations(inherited_write) == ["write to non-output descriptor"]
  let positioned_write = "42 pwrite64(3, \"unexpected\", 10, 0) = 10"
  assert report_checks.host_effect_trace_violations(positioned_write) == ["positioned write syscall"]
  let other_mutations = """42 utimensat(AT_FDCWD, "/etc/config", NULL, 0) = -1 EPERM
42 fallocate(4, 0, 0, 4096) = -1 EACCES
42 copy_file_range(3, NULL, 4, NULL, 16, 0) = -1 EBADF
42 setresuid(0, 0, 0) = -1 EPERM
42 clock_settime(CLOCK_REALTIME, {tv_sec=0, tv_nsec=0}) = -1 EPERM
"""
  assert report_checks.host_effect_trace_violations(other_mutations).len() == 5
  let mutable_ioctl = "42 ioctl(3</sys/class/net/eth0>, SIOCSIFFLAGS, 0x7fff) = -1 EPERM"
  assert report_checks.host_effect_trace_violations(mutable_ioctl) == ["unapproved ioctl request"]
  let unknown_ioctl = "42 ioctl(3</dev/null>, 0xfeed, 0x7fff) = -1 ENOTTY"
  assert report_checks.host_effect_trace_violations(unknown_ioctl) == ["unapproved ioctl request"]
  let terminal_size_ioctl = "42 ioctl(2<pipe:[123]>, TIOCGWINSZ, 0x7fff) = -1 ENOTTY"
  assert report_checks.host_effect_trace_violations(terminal_size_ioctl) == []
  let unfinished_open = "42 openat(AT_FDCWD, \"/tmp/output\", O_WRONLY <unfinished ...>"
  assert "incomplete syscall trace" in report_checks.host_effect_trace_violations(unfinished_open)
  let resumed_open = "42 <... openat resumed> ) = 4"
  assert "incomplete syscall trace" in report_checks.host_effect_trace_violations(resumed_open)
  let truncated_open = "42 openat(AT_FDCWD, \"/tmp/output\", O_WRONLY"
  assert "incomplete file open trace" in report_checks.host_effect_trace_violations(truncated_open)

  let replay_trace = """42 openat2(3, "stdout-live-json", {flags=O_RDONLY}, 24) = 4
42 openat2(3, "proc/meminfo", {flags=O_RDONLY}, 24) = 5
"""
  assert report_checks.replay_host_read_violations(replay_trace) == ["saved-report replay read a live source"]
  let absolute_replay_trace = """42 openat(AT_FDCWD, "/proc/meminfo", O_RDONLY) = 5
42 openat2(3, "/sys/devices/system/cpu/online", {flags=O_RDONLY}, 24) = 6
"""
  assert report_checks.replay_host_read_violations(absolute_replay_trace).len() == 2
  let replay_metadata_trace = """42 readlinkat(3, "sys/class/net/eth0", "../../devices/pci0000:00", 4096) = 25
42 open("/etc/os-release", O_RDONLY) = -1 EACCES
42 statx(3, "proc/meminfo", AT_STATX_SYNC_AS_STAT, STATX_ALL, 0x7ffc) = 0
"""
  assert report_checks.replay_host_read_violations(replay_metadata_trace).len() == 3
  let saved_link_target = "42 readlinkat(3, \"saved-report\", \"/sys/devices/virtual\", 4096) = 20"
  assert report_checks.replay_host_read_violations(saved_link_target) == []
  assert report_checks.replay_host_read_violations("42 openat(3, \"proc\", O_RDONLY|O_DIRECTORY) = 5") == [
    "saved-report replay read a live source",
  ]

  let permitted_process_trace = """42 openat2(3, "proc/123/stat", {flags=O_RDONLY}, 24) = 4
42 openat2(3, "proc/123/status", {flags=O_RDONLY}, 24) = 4
42 openat2(3, "proc/self/mountinfo", {flags=O_RDONLY}, 24) = 4
42 openat2(3, "proc/cmdline", {flags=O_RDONLY}, 24) = 4
"""
  assert report_checks.forbidden_process_read_violations(permitted_process_trace) == []
  let forbidden_process_trace = """42 openat2(3, "proc/123/environ", {flags=O_RDONLY}, 24) = -1 EACCES
42 openat(AT_FDCWD, "/proc/self/cmdline", O_RDONLY) = 4
42 readlinkat(3, "proc/123/fd/1", "/private/path", 4096) = 13
42 openat2(3, "proc/123/task/123/mem", {flags=O_RDONLY}, 24) = -1 EACCES
"""
  assert report_checks.forbidden_process_read_violations(forbidden_process_trace).len() == 4
  let permitted_process_json = """{"processes":{"processes":[{"pid":123,"command":{"state":"observed","value":"worker"}}]}}"""
  assert report_checks.forbidden_process_field_violations(permitted_process_json)? == []
  let forbidden_process_json = """{"processes":{"processes":[{"pid":123,"environment":"secret","cmdline":"private","open_paths":["/private"]}]}}"""
  assert report_checks.forbidden_process_field_violations(forbidden_process_json)?.len() == 3
}

test test_system_report_trace_ignores_syscall_names_inside_file_paths {
  let trace = """42 execve("/target/xsh", ["xsh"], 0x0) = 0
42 openat(AT_FDCWD, "/tmp/ execve( fork( clone( socket( O_WRONLY", O_RDONLY) = 3
"""
  assert report_checks.process_trace_violations(trace).len() == 0
  assert report_checks.host_effect_trace_violations(trace).len() == 0
}

test test_system_report_fixture_definition_check_ignores_comments_and_partial_names {
  let source = """# test test_system_report_comment_only {}
test test_system_report_existing {}
test test_system_report_existing_more {}
"""
  assert report_checks.fixture_test_definition_exists(source, "test_system_report_existing")
  assert ! report_checks.fixture_test_definition_exists(source, "test_system_report_comment_only")
  assert ! report_checks.fixture_test_definition_exists(source, "test_system_report_missing")
  assert ! report_checks.fixture_test_definition_exists(source, "test_system_report_existing_m")
  let rust_source = """// #[test]
fn comment_only() {}
/*
#[test]
fn block_comment_only() {}
*/
#[test]
fn observed() {}
fn unmarked() {}
#[test]
fn observed_more() {}
"""
  assert report_checks.rust_fixture_test_definition_exists(rust_source, "observed")
  assert ! report_checks.rust_fixture_test_definition_exists(rust_source, "comment_only")
  assert ! report_checks.rust_fixture_test_definition_exists(rust_source, "block_comment_only")
  assert ! report_checks.rust_fixture_test_definition_exists(rust_source, "unmarked")
  assert ! report_checks.rust_fixture_test_definition_exists(rust_source, "observed_m")
}

test test_system_report_forbidden_process_read_trace_normalizes_source_paths {
  let forbidden = """42 openat(AT_FDCWD, "/proc/./123/environ", O_RDONLY) = -1 EACCES
42 openat2(3, "proc//self/cmdline", {flags=O_RDONLY}, 24) = 4
42 readlinkat(3, "proc/self/task/321/../321/fd/4", "/private/path", 4096) = 13
"""
  assert report_checks.forbidden_process_read_violations(forbidden).len() == 3
  let permitted = """42 openat(AT_FDCWD, "/proc/./123/stat", O_RDONLY) = 4
42 readlinkat(3, "/tmp/safe", "proc/123/environ", 4096) = 19
"""
  assert report_checks.forbidden_process_read_violations(permitted) == []
}

test test_system_report_replay_trace_rejects_normalized_live_source_paths {
  let forbidden = """42 openat(AT_FDCWD, "/tmp/../proc/meminfo", O_RDONLY) = 4
42 newfstatat(AT_FDCWD, "cache/../../sys/devices/system/cpu/online", 0x7fff, 0) = 0
42 readlinkat(3, "tmp/../etc/os-release", "/private/link", 4096) = 13
"""
  assert report_checks.replay_host_read_violations(forbidden).len() == 3
  let permitted = """42 openat(AT_FDCWD, "/tmp/etc/os-release", O_RDONLY) = 4
42 readlinkat(3, "/tmp/saved-report", "/proc/meminfo", 4096) = 13
"""
  assert report_checks.replay_host_read_violations(permitted) == []
}

test test_system_report_replay_trace_subtracts_exact_startup_reads {
  let startup = "42 open(\"/proc/sys/vm/overcommit_memory\", O_RDONLY) = 3"
  assert report_checks.replay_host_read_violations_after_baseline(startup, startup) == []
  let repeated = f"""${startup}
${startup}"""
  assert report_checks.replay_host_read_violations_after_baseline(repeated, startup).len() == 1
  let new_source = f"""${startup}
42 open("/proc/meminfo", O_RDONLY) = 3"""
  assert report_checks.replay_host_read_violations_after_baseline(new_source, startup).len() == 1
  let different_call = "42 newfstatat(AT_FDCWD, \"/proc/sys/vm/overcommit_memory\", 0x7fff, 0) = 0"
  assert report_checks.replay_host_read_violations_after_baseline(different_call, startup).len() == 1
}

test test_system_report_trace_resolves_annotated_directory_descriptors {
  let unresolved_directory = "42 openat(3, \"self/environ\", O_RDONLY) = -1 EACCES"
  assert report_checks.host_effect_trace_violations(unresolved_directory) == ["unresolved source directory descriptor"]
  let unresolved_metadata = "42 statx(3, \"devices/system/cpu/online\", AT_STATX_SYNC_AS_STAT, STATX_ALL, 0x7fff) = 0"
  assert report_checks.host_effect_trace_violations(unresolved_metadata) == ["unresolved source directory descriptor"]
  let forbidden_process = "42 openat(3</proc>, \"self/environ\", O_RDONLY) = 4"
  assert report_checks.forbidden_process_read_violations(forbidden_process) == ["forbidden process source read"]
  let comma_directory = "42 openat(3</tmp/a,b>, \"../../proc/self/environ\", O_RDONLY) = 4"
  assert report_checks.forbidden_process_read_violations(comma_directory) == ["forbidden process source read"]
  let punctuation_directory = "42 openat(3</tmp/a,b)>, \"../../proc/self/environ\", O_RDONLY) = 4"
  assert report_checks.forbidden_process_read_violations(punctuation_directory) == ["forbidden process source read"]
  let forbidden_replay = """42 openat2(3</sys>, "devices/system/cpu/online", {flags=O_RDONLY}, 24) = 4
42 newfstatat(5</etc>, "os-release", 0x7fff, 0) = 0
"""
  assert report_checks.replay_host_read_violations(forbidden_replay).len() == 2
  let permitted_replay = "42 openat(3</tmp>, \"sys/devices/system/cpu/online\", O_RDONLY) = 4"
  assert report_checks.replay_host_read_violations(permitted_replay) == []
  let output_write = "42 write(1</tmp/report-output>, \"report\", 6) = 6"
  assert report_checks.host_effect_trace_violations(output_write) == []
}

test test_system_report_lscpu_reference_parser_keeps_sparse_online_ids {
  let reference = """{"cpus":[{"cpu":0,"online":true,"node":0},{"cpu":1,"online":false,"node":0},{"cpu":65,"online":true,"node":1}]}"""
  assert report_checks.parse_lscpu_online_cpu_ids(reference)? == [0, 65]
  let repeated = """{"cpus":[{"cpu":0,"online":true},{"cpu":0,"online":false}]}"""
  test.error_kind(report_checks.parse_lscpu_online_cpu_ids(repeated), "SystemReportCheckError.Invalid")?
  let unknown = """{"cpus":[{"cpu":0,"online":"yes"}]}"""
  test.error_kind(report_checks.parse_lscpu_online_cpu_ids(unknown), "schema")?
}

test test_system_report_lscpu_reference_parser_rejects_empty_cpu_set {
  test.error_kind(report_checks.parse_lscpu_online_cpu_ids("""{"cpus":[]}"""), "SystemReportCheckError.Invalid")?
  test.error_kind(
    report_checks.parse_lscpu_online_cpu_ids("""{"cpus":[{"cpu":0,"online":false}]}"""),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_sysfs_reference_cpu_list_parser_handles_sparse_ids {
  assert report_checks.parse_reference_cpu_list("0-2,65,129-130", false)? == [0, 1, 2, 65, 129, 130]
  assert report_checks.parse_reference_cpu_list("", true)? == []
  test.error_kind(report_checks.parse_reference_cpu_list("", false), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_cpu_list("2-1", false), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_cpu_list("0,0", false), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_cpu_list("0-65536", false), "SystemReportCheckError.Invalid")?
}

test test_system_report_proc_status_affinity_reference_requires_one_valid_cpu_list {
  assert report_checks.parse_proc_status_affinity("""Name:	cat
Cpus_allowed_list:	0-2,65,129-130
""")? == [0, 1, 2, 65, 129, 130]
  test.error_kind(
    report_checks.parse_proc_status_affinity("""Name:	cat
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_status_affinity("""Cpus_allowed_list:	
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_status_affinity("""Cpus_allowed_list:	1
Cpus_allowed_list:	2
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_status_affinity("""Cpus_allowed_list:	2-1
"""),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_cpu_scope_affinity_comparison_requires_stable_reference {
  let before = """Cpus_allowed_list:	0-1,65
"""
  let candidate = """{"cpu":{"affinity":[65,0,1]}}"""
  let compared = report_checks.compare_cpu_scope_affinity(candidate, before, before)?
  assert compared.exact == true
  assert compared.matched_count == 3
  let missing = report_checks.compare_cpu_scope_affinity("""{"cpu":{"affinity":[0,1]}}""", before, before)?
  assert missing.missing_ids == [65]
  test.error_kind(
    report_checks.compare_cpu_scope_affinity(
  candidate,
  before,
  """Cpus_allowed_list:	0-1
""",
),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.compare_cpu_scope_affinity("""{"cpu":{"affinity":[0,0]}}""", before, before),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_cpu_online_comparison_counts_missing_and_unexpected_ids {
  let reference = """{"cpus":[{"cpu":0,"online":true},{"cpu":1,"online":false},{"cpu":65,"online":true}]}"""
  let candidate = """{"cpu":{"online":[0,2]}}"""
  let compared = report_checks.compare_cpu_online_ids(candidate, reference)?
  assert compared.reference_count == 2
  assert compared.candidate_count == 2
  assert compared.matched_count == 1
  assert compared.missing_ids == [65]
  assert compared.unexpected_ids == [2]
  assert ! compared.exact
  let matching = report_checks.compare_cpu_online_ids("""{"cpu":{"online":[65,0]}}""", reference)?
  assert matching.exact
  test.error_kind(
    report_checks.compare_cpu_online_ids("""{"cpu":{"online":[0,0]}}""", reference),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_cpu_set_comparison_scores_all_four_kernel_sets {
  let reference = {
    possible: [
      0,
      1,
      2,
    ],
    present: [
      0,
      2,
    ],
    online: [
      0,
      2,
    ],
    offline: [],
  }
  let candidate = """{"cpu":{"possible":[0,1,2],"present":[0,2],"online":[0],"offline":[2]}}"""
  let compared = report_checks.compare_cpu_sets(candidate, reference)?
  assert compared.possible.exact
  assert compared.present.exact
  assert compared.online.missing_ids == [2]
  assert compared.offline.unexpected_ids == [2]
  assert ! compared.exact
  let matching = """{"cpu":{"possible":[2,1,0],"present":[2,0],"online":[0,2],"offline":[]}}"""
  assert report_checks.compare_cpu_sets(matching, reference)?.exact
  let duplicate = """{"cpu":{"possible":[0,0],"present":[],"online":[],"offline":[]}}"""
  test.error_kind(report_checks.compare_cpu_sets(duplicate, reference), "SystemReportCheckError.Invalid")?
}

test test_system_report_swapon_raw_parser_and_comparison_keep_swap_identity {
  let output = """NAME TYPE SIZE USED PRIO
/dev/zram0 partition 4096 1024 42
/swapfile file 8192 0 -1
"""
  let reference = report_checks.parse_swapon_raw(output)?
  assert reference.len() == 2
  assert reference[0].name == "/dev/zram0"
  assert reference[0].size_bytes == 4096
  assert reference[1].priority == -1
  let reordered = report_checks.parse_swapon_raw("""NAME TYPE SIZE USED PRIO
/swapfile file 8192 0 -1
/dev/zram0 partition 4096 1024 42
""")?
  assert report_checks.swap_reference_stable(reference, reordered)
  let changed = report_checks.parse_swapon_raw("""NAME TYPE SIZE USED PRIO
/dev/zram0 partition 4096 2048 42
/swapfile file 8192 0 -1
""")?
  assert ! report_checks.swap_reference_stable(reference, changed)
  let candidate = """{"memory":{"status":{"state":"complete"},"swaps":[{"name":{"state":"observed","value":"/swapfile"},"kind":"file","size_bytes":8192,"used_bytes":0,"priority":-1},{"name":{"state":"observed","value":"/dev/zram0"},"kind":"partition","size_bytes":4096,"used_bytes":1024,"priority":42}]},"issues":[]}"""
  let exact = report_checks.compare_swap_devices(candidate, reference)?
  assert exact.exact
  assert exact.matched_count == 2
  let missing = report_checks.compare_swap_devices("""{"memory":{"swaps":[]}}""", reference)?
  assert missing.missing_names == ["/dev/zram0", "/swapfile"]
  assert ! missing.exact
  let wrong_used = """{"memory":{"swaps":[{"name":{"state":"observed","value":"/dev/zram0"},"kind":"partition","size_bytes":4096,"used_bytes":2048,"priority":42},{"name":{"state":"observed","value":"/swapfile"},"kind":"file","size_bytes":8192,"used_bytes":0,"priority":-1}]}}"""
  let mismatch = report_checks.compare_swap_devices(wrong_used, reference)?
  assert mismatch.field_mismatches == ["/dev/zram0"]
  assert mismatch.used_mismatches == 1
  assert mismatch.kind_mismatches + mismatch.size_mismatches + mismatch.priority_mismatches == 0
  assert ! mismatch.exact
  let absent_field = report_checks.compare_swap_devices("""{"memory":{}}""", reference)?
  assert absent_field.candidate_field_missing
  assert ! absent_field.exact
  test.error_kind(
    report_checks.compare_swap_devices("""{"memory":{}}""", reference.extend([reference[0]])),
    "SystemReportCheckError.Invalid",
  )?
  let redacted = """{"memory":{"swaps":[{"name":{"state":"redacted","value":null},"kind":"partition","size_bytes":4096,"used_bytes":1024,"priority":42}]}}"""
  let redacted_result = report_checks.compare_swap_devices(redacted, reference)?
  assert redacted_result.candidate_field_missing
  assert redacted_result.matched_count == 0
  let duplicate = """{"memory":{"swaps":[{"name":{"state":"observed","value":"/dev/zram0"},"kind":"partition","size_bytes":4096,"used_bytes":1024,"priority":42},{"name":{"state":"observed","value":"/dev/zram0"},"kind":"partition","size_bytes":4096,"used_bytes":1024,"priority":42}]}}"""
  test.error_kind(report_checks.compare_swap_devices(duplicate, reference), "SystemReportCheckError.Invalid")?
}

test test_system_report_swap_comparison_requires_source_evidence_for_empty_set {
  let complete = """{"memory":{"status":{"state":"complete"},"swaps":[]},"issues":[]}"""
  assert report_checks.compare_swap_devices(complete, [])?.exact
  let failed_read = """{"memory":{"status":{"state":"partial"},"swaps":[]},"issues":[{"section":"memory","field":"swaps"}]}"""
  assert ! report_checks.compare_swap_devices(failed_read, [])?.exact
  let not_requested = """{"memory":{"status":{"state":"not_requested"},"swaps":[]},"issues":[]}"""
  assert ! report_checks.compare_swap_devices(not_requested, [])?.exact
  let missing_provenance = """{"memory":{"swaps":[]}}"""
  assert ! report_checks.compare_swap_devices(missing_provenance, [])?.exact
}

test test_system_report_swapon_raw_parser_rejects_ambiguous_or_unsafe_rows {
  for output in [
    "",
    """/dev/zram0 partition 4096 0 42
""",
    """NAME TYPE SIZE USED PRIO
/swap file file 4096 0 42
""",
    """NAME TYPE SIZE USED PRIO
/dev/zram0 partition 0x1000 0 42
""",
    """NAME TYPE SIZE USED PRIO
/dev/zram0 partition 9007199254740992 0 42
""",
    """NAME TYPE SIZE USED PRIO
/dev/zram0 partition 4096 8192 42
""",
    """NAME TYPE SIZE USED PRIO
/dev/zram0 partition 4096 0 42
/dev/zram0 partition 4096 0 42
""",
  ] {
    test.error_kind(report_checks.parse_swapon_raw(output), "SystemReportCheckError.Invalid")?
  }
}

test test_system_report_proc_swaps_raw_reference_decodes_units_paths_and_empty_inventory {
  let raw = """Filename	Type	Size	Used	Priority
/dev/zram0	partition	4	1	42
/swap\\040file	file	8	0	-1
"""
  let reference = report_checks.parse_proc_swaps_raw_reference(raw)?
  assert reference.len() == 2
  assert reference[0].size_bytes == 4096
  assert reference[0].used_bytes == 1024
  assert reference[1].name == "/swap file"
  assert reference[1].priority == -1
  let literal_escape = report_checks.parse_proc_swaps_raw_reference("""Filename Type Size Used Priority
/swap\\134040file file 1 0 1
""")?
  assert literal_escape[0].name == "/swap\\040file"
  assert report_checks.parse_proc_swaps_raw_reference("""Filename Type Size Used Priority
""")? == []
  test.error_kind(report_checks.parse_proc_swaps_raw_reference(""), "SystemReportCheckError.Invalid")?
  test.error_kind(
    report_checks.parse_proc_swaps_raw_reference("""Filename Type Size Used Priority
/dev/zram0 partition 8796093022208 0 42
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_swaps_raw_reference("""Filename Type Size Used Priority
/dev/zram0 partition 4 5 42
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_swaps_raw_reference("""Filename Type Size Used Priority
/dev/zram0 partition 4 0 42
/dev/zram0 partition 4 0 42
"""),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_proc_swaps_capture_validates_saved_source_and_oracle {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc", parents: true)?
  source.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
/dev/zram0 partition 4 1 42
""",
  )?
  report_checks.capture_proc_swaps_bundle(source, bundle, "synthetic_fixture")?
  assert report_checks.validate_proc_swaps_bundle(bundle)?[0].size_bytes == 4096
  source.write(
    p"proc/swaps",
    """changed
""",
  )?
  assert report_checks.validate_proc_swaps_bundle(bundle)?[0].used_bytes == 1024
  let metadata = bundle.read_text(p"capture.json")?
  let changed_oracle = json.encode(json.set(json.decode(metadata)?, ["reference", 0, "size_bytes"], 1)?)?
  bundle.write(p"capture.json", changed_oracle)?
  test.error_kind(report_checks.validate_proc_swaps_bundle(bundle), "SystemReportCheckError.Invalid")?
  let contradictory = json.encode(json.set(json.decode(metadata)?, ["errno"], 13)?)?
  bundle.write(p"capture.json", contradictory)?
  test.error_kind(report_checks.validate_proc_swaps_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(p"capture.json", metadata)?
  bundle.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
/dev/zram0 partition 8 1 42
""",
  )?
  test.error_kind(report_checks.validate_proc_swaps_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_proc_swaps_capture_marks_absent_malformed_and_truncated_unscoreable {
  let source = fs.tempdir()?
  defer source.close()?
  let absent = fs.tempdir()?
  defer absent.close()?
  report_checks.capture_proc_swaps_bundle(source, absent, "synthetic_fixture")?
  test.error_kind(report_checks.validate_proc_swaps_bundle(absent), "SystemReportCheckError.Invalid")?
  source.mkdir(p"proc", parents: true)?
  source.write(
    p"proc/swaps",
    """broken row
""",
  )?
  let malformed = fs.tempdir()?
  defer malformed.close()?
  report_checks.capture_proc_swaps_bundle(source, malformed, "synthetic_fixture")?
  test.error_kind(report_checks.validate_proc_swaps_bundle(malformed), "SystemReportCheckError.Invalid")?
  var padding = "x"
  while padding.count_chars() <= 262144 {
    padding = f"${padding}${padding}"
  }

  source.write(
    p"proc/swaps",
    f"""Filename Type Size Used Priority
${padding}""",
  )?
  let truncated = fs.tempdir()?
  defer truncated.close()?
  report_checks.capture_proc_swaps_bundle(source, truncated, "synthetic_fixture")?
  test.error_kind(report_checks.validate_proc_swaps_bundle(truncated), "SystemReportCheckError.Invalid")?
  source.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
""",
  )?
  let empty = fs.tempdir()?
  defer empty.close()?
  report_checks.capture_proc_swaps_bundle(source, empty, "synthetic_fixture")?
  assert report_checks.validate_proc_swaps_bundle(empty)? == []
}

test test_system_report_proc_swaps_capture_replays_production_collector {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc", parents: true)?
  source.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
/dev/zram0 partition 4 1 42
""",
  )?
  report_checks.capture_proc_swaps_bundle(source, bundle, "synthetic_fixture")?
  assert report_checks.replay_proc_swaps_bundle(bundle)?.exact
}

test test_system_report_lsblk_json_scores_devices_and_layering {
  let output = """{"blockdevices":[{"name":"sda","kname":"sda","maj:min":"8:0","size":8192,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096,"children":[{"name":"sda1","kname":"sda1","maj:min":"8:1","size":4096,"type":"part","pkname":"sda","ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096,"children":[{"name":"cryptroot","kname":"dm-0","maj:min":"253:0","size":4096,"type":"crypt","pkname":"sda1","ro":false,"rm":false,"rota":false,"log-sec":512,"phy-sec":4096}]}]}]}"""
  let reference = report_checks.parse_lsblk_json(output)?
  assert reference.devices.len() == 3
  assert reference.edges.len() == 2
  assert reference.devices[2].name == "dm-0"
  assert reference.edges[0].parent_name == "sda"
  assert reference.edges[1].child_name == "dm-0"
  let candidate = """{"storage":{"devices":[{"name":"dm-0","major":253,"minor":0,"kind":"virtual","size_bytes":4096,"logical_sector_bytes":512,"physical_sector_bytes":4096,"removable":false,"rotational":false,"read_only":false,"parent_device_index":null,"holder_indices":[],"slave_indices":[2]},{"name":"sda","major":8,"minor":0,"kind":"disk","size_bytes":8192,"logical_sector_bytes":512,"physical_sector_bytes":4096,"removable":false,"rotational":true,"read_only":false,"parent_device_index":null,"holder_indices":[],"slave_indices":[]},{"name":"sda1","major":8,"minor":1,"kind":"partition","size_bytes":4096,"logical_sector_bytes":512,"physical_sector_bytes":4096,"removable":false,"rotational":true,"read_only":false,"parent_device_index":1,"holder_indices":[0],"slave_indices":[]}]}}"""
  let exact = report_checks.compare_block_devices(candidate, reference)?
  assert exact.exact
  assert exact.matched_count == 3
  assert exact.matched_edges == 2
  let wrong_size = json.encode(json.set(json.decode(candidate)?, ["storage", "devices", 2, "size_bytes"], 2048)?)?
  let size_mismatch = report_checks.compare_block_devices(wrong_size, reference)?
  assert size_mismatch.size_mismatches == 1
  assert ! size_mismatch.exact
  let lost_edge = json.encode(json.set(json.decode(candidate)?, ["storage", "devices", 0, "slave_indices"], [])?)?
  let relation_mismatch = report_checks.compare_block_devices(lost_edge, reference)?
  assert relation_mismatch.missing_edges == 1
  assert ! relation_mismatch.exact
  let extra_edge = json.encode(json.set(json.decode(candidate)?, ["storage", "devices", 0, "slave_indices"], [1, 2])?)?
  let extra_relation = report_checks.compare_block_devices(extra_edge, reference)?
  assert extra_relation.unexpected_edges == 1
  assert ! extra_relation.exact
  let duplicate_relation = json.encode(
    json.set(json.decode(candidate)?, ["storage", "devices", 0, "slave_indices"], [2, 2])?,
  )?
  test.error_kind(report_checks.compare_block_devices(duplicate_relation, reference), "SystemReportCheckError.Invalid")?
  let missing = report_checks.compare_block_devices("""{"storage":{"devices":[]}}""", reference)?
  assert missing.missing_names == ["dm-0", "sda", "sda1"]
  assert ! missing.exact
  assert report_checks.block_reference_stable(reference, report_checks.parse_lsblk_json(output)?)
}

test test_system_report_lsblk_json_preserves_sparse_partition_identity_and_parent_edges {
  let output = """{"blockdevices":[{"name":"sda","kname":"sda","maj:min":"8:0","size":524288,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096,"children":[{"name":"sda1","kname":"sda1","maj:min":"8:1","size":65536,"type":"part","pkname":"sda","ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096},{"name":"sda3","kname":"sda3","maj:min":"8:3","size":65536,"type":"part","pkname":"sda","ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096}]}]}"""
  let reference = report_checks.parse_lsblk_json(output)?
  assert reference.devices.len() == 3
  assert reference.edges.len() == 2
  assert (reference.devices |> where .name == "sda2").len() == 0
  let candidate = """{"storage":{"devices":[{"name":"sda3","major":8,"minor":3,"kind":"partition","size_bytes":65536,"logical_sector_bytes":512,"physical_sector_bytes":4096,"removable":false,"rotational":true,"read_only":false,"parent_device_index":1,"holder_indices":[],"slave_indices":[]},{"name":"sda","major":8,"minor":0,"kind":"disk","size_bytes":524288,"logical_sector_bytes":512,"physical_sector_bytes":4096,"removable":false,"rotational":true,"read_only":false,"parent_device_index":null,"holder_indices":[],"slave_indices":[]},{"name":"sda1","major":8,"minor":1,"kind":"partition","size_bytes":65536,"logical_sector_bytes":512,"physical_sector_bytes":4096,"removable":false,"rotational":true,"read_only":false,"parent_device_index":1,"holder_indices":[],"slave_indices":[]}]}}"""
  let exact = report_checks.compare_block_devices(candidate, reference)?
  assert exact.exact
  assert exact.matched_edges == 2
  var partition_sources = json.decode(candidate)?
  for index in [0, 2] {
    for field in ["logical_sector_bytes", "physical_sector_bytes", "removable", "rotational"] {
      partition_sources = json.set(partition_sources, ["storage", "devices", index, field], null)?
    }
  }

  assert report_checks.compare_block_devices(json.encode(partition_sources)?, reference)?.exact
  let wrong_parent = json.encode(
    json.set(json.decode(candidate)?, ["storage", "devices", 0, "parent_device_index"], null)?,
  )?
  assert ! report_checks.compare_block_devices(wrong_parent, reference)?.exact
}

test test_system_report_lspci_vmm_numeric_identity_keeps_repeated_ids_distinct {
  let first = """Slot:	0000:00:1f.6
Class:	0600
Vendor:	8086
Device:	1234
SVendor:	8086
SDevice:	0001
Rev:	01
ProgIf:	00
Driver:	pcieport
NUMANode:	0
IOMMUGroup:	42
"""
  let second = """Slot:	0001:02:03.0
Class:	0600
Vendor:	8086
Device:	1234
ProgIf:	00
"""
  let reference = report_checks.parse_lspci_vmm_numeric(first + "\n" + second + "\n")?
  assert reference.len() == 2
  assert reference[0].address == "0000:00:1f.6"
  assert reference[1].address == "0001:02:03.0"
  assert reference[0].vendor_id == reference[1].vendor_id
  assert reference[0].class_code == 393216
  assert report_checks.pci_reference_stable(
    reference,
    report_checks.parse_lspci_vmm_numeric(second + "\n" + first + "\n")?,
  )
  let candidate = """{"pci":{"status":{"state":"complete","enumeration_succeeded":true},"functions":[{"address":"0001:02:03.0","domain":1,"bus":2,"device":3,"function":0,"vendor_id":32902,"device_id":4660,"class_code":393216,"revision":0,"subsystem_vendor_id":null,"subsystem_device_id":null,"driver":null,"numa_node":null,"iommu_group":null},{"address":"0000:00:1f.6","domain":0,"bus":0,"device":31,"function":6,"vendor_id":32902,"device_id":4660,"class_code":393216,"revision":1,"subsystem_vendor_id":32902,"subsystem_device_id":1,"driver":"pcieport","numa_node":0,"iommu_group":"42"}]}}"""
  let exact = report_checks.compare_lspci_identity(candidate, reference)?
  assert exact.exact_static
  assert exact.matched_count == 2
  let changed_vendor = json.encode(json.set(json.decode(candidate)?, ["pci", "functions", 0, "vendor_id"], 32903)?)?
  let wrong_vendor = report_checks.compare_lspci_identity(changed_vendor, reference)?
  assert ! wrong_vendor.exact_static
  assert wrong_vendor.field_mismatches == ["0001:02:03.0.vendor_id"]
  let changed_driver = json.encode(json.set(json.decode(candidate)?, ["pci", "functions", 1, "driver"], "other")?)?
  assert report_checks.compare_lspci_identity(changed_driver, reference)?.field_mismatches == ["0000:00:1f.6.driver"]
  let missing_vendor = json.encode(json.set(json.decode(candidate)?, ["pci", "functions", 0, "vendor_id"], null)?)?
  assert report_checks.compare_lspci_identity(missing_vendor, reference)?.candidate_field_missing
  assert ! report_checks.pci_reference_stable(
    reference,
    report_checks.parse_lspci_vmm_numeric(first.replace("Driver:\tpcieport", "Driver:\tother") + "\n" + second + "\n")?,
  )
  let no_prog_if = report_checks.parse_lspci_vmm_numeric(
    first + "\n" + second.replace(
  """ProgIf:	00
""",
  "",
) + "\n",
  )?
  let candidate_unknown_prog_if = json.encode(
    json.set(json.decode(candidate)?, ["pci", "functions", 0, "class_code"], 393217)?,
  )?
  assert report_checks.compare_lspci_identity(candidate_unknown_prog_if, no_prog_if)?.exact_static
  test.error_kind(report_checks.parse_lspci_vmm_numeric(first + "\n" + first + "\n"), "SystemReportCheckError.Invalid")?
  test.error_kind(
    report_checks.parse_lspci_vmm_numeric(first.replace("Class:\t0600", "Class:\t06zz") + "\n"),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_lspci_vmm_numeric(first.replace("NUMANode:\t0", "NUMANode:\t0x1") + "\n"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_pci_link_reference_scores_all_functions_and_stable_link_fields {
  let linked = {
    address: "0000:00:1f.6",
    current_speed: "8.0 GT/s PCIe",
    current_width: 4,
    maximum_speed: "16.0 GT/s PCIe",
    maximum_width: 8,
  }
  let unlinked = {
    address: "0001:02:03.0",
    current_speed: null,
    current_width: null,
    maximum_speed: null,
    maximum_width: null,
  }
  let candidate = """{"pci":{"status":{"state":"complete","enumeration_succeeded":true},"functions":[{"address":"0001:02:03.0","current_link_speed":null,"current_link_width":null,"maximum_link_speed":null,"maximum_link_width":null},{"address":"0000:00:1f.6","current_link_speed":"8.0 GT/s PCIe","current_link_width":4,"maximum_link_speed":"16.0 GT/s PCIe","maximum_link_width":8}]}}"""
  let exact = report_checks.compare_pci_links(candidate, [linked, unlinked], [linked, unlinked])?
  assert exact.eligible
  assert exact.exact
  assert exact.matched_count == 2
  let wrong = report_checks.compare_pci_links(
    candidate.replace("\"current_link_width\":4", "\"current_link_width\":2"),
    [linked, unlinked],
    [linked, unlinked],
  )?
  assert wrong.field_mismatches == ["0000:00:1f.6.current_link_width"]
  let changed = report_checks.compare_pci_links(
    candidate,
    [linked, unlinked],
    [{...linked, current_speed: "16.0 GT/s PCIe"}, unlinked],
  )?
  assert changed.unstable_fields == ["0000:00:1f.6.current_link_speed"]
  assert ! changed.exact
  let unlinked_candidate = """{"pci":{"status":{"state":"complete","enumeration_succeeded":true},"functions":[{"address":"0001:02:03.0","current_link_speed":null,"current_link_width":null,"maximum_link_speed":null,"maximum_link_width":null}]}}"""
  let absent = report_checks.compare_pci_links(unlinked_candidate, [unlinked], [unlinked])?
  assert ! absent.eligible
  assert ! absent.exact
}

test test_system_report_pci_binding_reference_resolves_parent_indexes_and_absent_links {
  let bridge = {address: "0001:02:01.0", driver: "pcieport", parent_address: null, numa_node: 0, iommu_group: "42"}
  let child = {
    address: "0001:03:00.0",
    driver: null,
    parent_address: "0001:02:01.0",
    numa_node: null,
    iommu_group: null,
  }
  let candidate = """{"pci":{"status":{"state":"complete","enumeration_succeeded":true},"functions":[{"address":"0001:03:00.0","driver":null,"parent_function_index":1,"numa_node":null,"iommu_group":null},{"address":"0001:02:01.0","driver":"pcieport","parent_function_index":null,"numa_node":0,"iommu_group":"42"}]}}"""
  let exact = report_checks.compare_pci_bindings(candidate, [bridge, child], [bridge, child])?
  assert exact.exact
  assert exact.matched_count == 2
  let wrong_parent = report_checks.compare_pci_bindings(
    candidate.replace("\"parent_function_index\":1", "\"parent_function_index\":null"),
    [bridge, child],
    [bridge, child],
  )?
  assert wrong_parent.field_mismatches == ["0001:03:00.0.parent_function_index"]
  let invalid_parent = report_checks.compare_pci_bindings(
    candidate.replace("\"parent_function_index\":1", "\"parent_function_index\":8"),
    [bridge, child],
    [bridge, child],
  )?
  assert invalid_parent.field_mismatches == ["0001:03:00.0.parent_function_index"]
  let wrong_absence = report_checks.compare_pci_bindings(
    candidate.replace("\"driver\":null", "\"driver\":\"other\""),
    [bridge, child],
    [bridge, child],
  )?
  assert wrong_absence.field_mismatches == ["0001:03:00.0.driver"]
  let changed = report_checks.compare_pci_bindings(candidate, [bridge, child], [bridge, {...child, driver: "vfio-pci"}])?
  assert changed.unstable_fields == ["0001:03:00.0.driver"]
  assert ! changed.exact
  let incomplete = report_checks.compare_pci_bindings(
    candidate.replace("\"state\":\"complete\"", "\"state\":\"partial\""),
    [bridge, child],
    [bridge, child],
  )?
  assert incomplete.candidate_field_missing
  assert ! incomplete.exact
}

test test_system_report_pci_binding_parent_reference_requires_own_bdf_and_keeps_bridge {
  assert report_checks.pci_binding_parent_from_target(
    ../../../devices/pci0001:02/0001:02:01.0/0001:03:00.0,
    "0001:03:00.0",
  )? == "0001:02:01.0"
  assert report_checks.pci_binding_parent_from_target(../../../devices/pci0001:02/0001:02:01.0, "0001:02:01.0")? == null
  test.error_kind(
    report_checks.pci_binding_parent_from_target(../../../devices/pci0001:02/0001:02:01.0, "0001:03:00.0"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_pci_binding_rooted_reference_reads_links_and_unknown_numa {
  let root = fs.tempdir()?
  defer root.close()?
  let bridge = p"sys/devices/pci0001:02/0001:02:01.0"
  let child = p"sys/devices/pci0001:02/0001:02:01.0/0001:03:00.0"
  root.mkdir(child, parents: true)?
  root.mkdir(p"sys/bus/pci/devices", parents: true)?
  root.symlink(../../../devices/pci0001:02/0001:02:01.0, p"sys/bus/pci/devices/0001:02:01.0")?
  root.symlink(../../../devices/pci0001:02/0001:02:01.0/0001:03:00.0, p"sys/bus/pci/devices/0001:03:00.0")?
  root.symlink(../../../../bus/pci/drivers/pcieport, fp"${bridge}/driver")?
  root.symlink(../../../../kernel/iommu_groups/42, fp"${bridge}/iommu_group")?
  root.write(
    fp"${bridge}/numa_node",
    """0
""",
  )?
  root.write(
    fp"${child}/numa_node",
    """-1
""",
  )?
  let reference = report_checks.read_pci_binding_reference(root)?
  assert reference.len() == 2
  assert reference[0].driver == "pcieport"
  assert reference[0].parent_address == null
  assert reference[0].iommu_group == "42"
  assert reference[1].parent_address == "0001:02:01.0"
  assert reference[1].driver == null
  assert reference[1].numa_node == null
  root.write(
    fp"${child}/numa_node",
    """0x1
""",
  )?
  test.error_kind(report_checks.read_pci_binding_reference(root), "SystemReportCheckError.Invalid")?
}

test test_system_report_thermal_reference_preserves_sparse_trip_indexes_and_brackets_temperature {
  let zone = report_checks.ThermalZoneReference(
    id: 3,
    kind: "cpu_thermal",
    temperature_millidegrees: 41000,
    trips: [
      {
        index: 0,
        kind: "critical",
        temperature_millidegrees: 95000,
        hysteresis_millidegrees: 2000,
      },
      {
        index: 2,
        kind: "passive",
        temperature_millidegrees: 85000,
        hysteresis_millidegrees: 0,
      },
    ],
  )
  let candidate = """{"sensors":{"status":{"state":"complete","enumeration_succeeded":true},"thermal_zones":[{"id":3,"kind":"cpu_thermal","temperature_millidegrees":41000,"trips":[{"index":0,"kind":"critical","temperature_millidegrees":95000,"hysteresis_millidegrees":2000},{"index":2,"kind":"passive","temperature_millidegrees":85000,"hysteresis_millidegrees":0}]}]}}"""
  let exact = report_checks.compare_thermal_zones(candidate, [zone], [zone])?
  assert exact.exact
  let wrong_index = report_checks.compare_thermal_zones(candidate.replace("\"index\":2", "\"index\":1"), [zone], [zone])?
  assert ! wrong_index.exact
  assert wrong_index.field_mismatches |> any "trip.2" in .
  let changed = report_checks.compare_thermal_zones(candidate, [zone], [{...zone, temperature_millidegrees: 42000}])?
  assert changed.unstable_fields == ["zone.3.temperature_millidegrees"]
  assert ! changed.exact
  let unrelated_hwmon = candidate.replace("\"state\":\"complete\"", "\"state\":\"partial\"")
    .replace("\"enumeration_succeeded\":true", "\"enumeration_succeeded\":false")
  let with_hwmon_issue = unrelated_hwmon.replace(
    "\"sensors\":",
    "\"issues\":[{\"section\":\"sensors\",\"field\":\"hwmon.hwmon0.name\"}],\"sensors\":",
  )
  assert report_checks.compare_thermal_zones(with_hwmon_issue, [zone], [zone])?.exact
  let thermal_issue = candidate.replace(
    "\"sensors\":",
    "\"issues\":[{\"section\":\"sensors\",\"field\":\"thermal_zones.thermal_zone3.temp\"}],\"sensors\":",
  )
  assert report_checks.compare_thermal_zones(thermal_issue, [zone], [zone])?.candidate_field_missing
}

test test_system_report_thermal_rooted_reference_reads_indexed_sources {
  let root = fs.tempdir()?
  defer root.close()?
  let zone = p"sys/class/thermal/thermal_zone3"
  root.mkdir(zone, parents: true)?
  root.write(
    fp"${zone}/type",
    """cpu_thermal
""",
  )?
  root.write(
    fp"${zone}/temp",
    """-500
""",
  )?
  root.write(
    fp"${zone}/trip_point_2_type",
    """passive
""",
  )?
  root.write(
    fp"${zone}/trip_point_2_temp",
    """85000
""",
  )?
  root.write(
    fp"${zone}/trip_point_0_type",
    """critical
""",
  )?
  root.write(
    fp"${zone}/trip_point_0_temp",
    """95000
""",
  )?
  let zones = report_checks.read_thermal_zone_reference(root)?
  assert zones.len() == 1
  assert zones[0].temperature_millidegrees == -500
  assert (zones[0].trips |> map .index) == [0, 2]
  assert zones[0].trips[1].hysteresis_millidegrees == null
  root.write(
    fp"${zone}/trip_point_02_temp",
    """85000
""",
  )?
  test.error_kind(report_checks.read_thermal_zone_reference(root), "SystemReportCheckError.Invalid")?
}

test test_system_report_thermal_capture_replays_raw_zone_and_rejects_tampering {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let zone = p"sys/class/thermal/thermal_zone3"
  source.mkdir(zone, parents: true)?
  source.write(
    fp"${zone}/type",
    """cpu_thermal
""",
  )?
  source.write(
    fp"${zone}/temp",
    """42000
""",
  )?
  source.write(
    fp"${zone}/trip_point_2_type",
    """passive
""",
  )?
  source.write(
    fp"${zone}/trip_point_2_temp",
    """85000
""",
  )?
  source.write(
    fp"${zone}/trip_point_2_hyst",
    """2000
""",
  )?
  report_checks.capture_thermal_bundle(source, bundle, "synthetic_fixture")?
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"origin\": \"synthetic_fixture\"" in metadata
  assert "\"reference_adapter\": \"thermal-raw-v1\"" in metadata
  assert "\"scoreable\": true" in metadata, metadata
  bundle.write(p"capture.json", metadata.replace("\"stable\": true", "\"stable\": false"))?
  assert report_checks.validate_thermal_bundle(bundle)?.len() == 1
  bundle.write(p"capture.json", metadata.replace("\"errno\": null", "\"errno\": 13"))?
  test.error_kind(report_checks.validate_thermal_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(p"capture.json", metadata)?
  assert report_checks.replay_thermal_bundle(bundle)?.exact
  bundle.write(
    fp"${zone}/trip_point_2_temp",
    """86000
""",
  )?
  test.error_kind(report_checks.validate_thermal_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_thermal_capture_preserves_absent_class_without_scoring {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  report_checks.capture_thermal_bundle(source, bundle, "synthetic_fixture")?
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"listing_state\": \"absent\"" in metadata
  assert "\"scoreable\": false" in metadata
  assert ! bundle.exists(p"sys/class/thermal")?
  test.error_kind(report_checks.validate_thermal_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_pci_link_rooted_reference_reads_bounded_attributes {
  let root = fs.tempdir()?
  defer root.close()?
  let first = p"sys/bus/pci/devices/0000:00:1f.6"
  let second = p"sys/bus/pci/devices/0001:02:03.0"
  root.mkdir(first, parents: true)?
  root.mkdir(second, parents: true)?
  root.write(
    fp"${first}/current_link_speed",
    """8.0 GT/s PCIe
""",
  )?
  root.write(
    fp"${first}/current_link_width",
    """4
""",
  )?
  root.write(
    fp"${first}/max_link_speed",
    """16.0 GT/s PCIe
""",
  )?
  root.write(
    fp"${first}/max_link_width",
    """8
""",
  )?
  let reference = report_checks.read_pci_link_reference(root)?
  assert reference.len() == 2
  assert reference[0].address == "0000:00:1f.6"
  assert reference[0].current_width == 4
  assert reference[1].current_width == null
  root.write(
    fp"${first}/current_link_width",
    """0x4
""",
  )?
  test.error_kind(report_checks.read_pci_link_reference(root), "SystemReportCheckError.Invalid")?
  root.write(
    fp"${first}/current_link_width",
    """9007199254740992
""",
  )?
  test.error_kind(report_checks.read_pci_link_reference(root), "SystemReportCheckError.Invalid")?
}

test test_system_report_lsblk_json_rejects_incomplete_or_unsafe_rows {
  for output in [
    """{"blockdevices":[{"name":"sda"}]}""",
    """{"blockdevices":[{"name":"sda","kname":"sda","maj:min":"8:0","size":9007199254740992,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096}]}""",
    """{"blockdevices":[{"name":"sda","kname":"sda","maj:min":"8:0","size":4096,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096},{"name":"duplicate","kname":"sda","maj:min":"8:1","size":4096,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096}]}""",
    """{"blockdevices":[{"name":"sda","kname":"sda","maj:min":"8:0","size":4096,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096},{"name":"sdb","kname":"sdb","maj:min":"08:0","size":4096,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096}]}""",
  ] {
    test.error_kind(report_checks.parse_lsblk_json(output), "SystemReportCheckError.Invalid")?
  }
}

test test_system_report_lsblk_json_preserves_shared_tree_edges {
  let first = """{"name":"sda","kname":"sda","maj:min":"8:0","size":8192,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096,"children":[{"name":"cryptroot","kname":"dm-0","maj:min":"253:0","size":4096,"type":"crypt","pkname":"sda","ro":false,"rm":false,"rota":false,"log-sec":512,"phy-sec":4096}]}"""
  let second = """{"name":"sdb","kname":"sdb","maj:min":"8:16","size":8192,"type":"disk","pkname":null,"ro":false,"rm":false,"rota":true,"log-sec":512,"phy-sec":4096,"children":[{"name":"cryptroot","kname":"dm-0","maj:min":"253:0","size":4096,"type":"crypt","pkname":"sdb","ro":false,"rm":false,"rota":false,"log-sec":512,"phy-sec":4096}]}"""
  let before = report_checks.parse_lsblk_json("{\"blockdevices\":[" + first + "," + second + "]}")?
  let after = report_checks.parse_lsblk_json("{\"blockdevices\":[" + second + "," + first + "]}")?
  assert before.devices.len() == 3
  assert before.edges.len() == 2
  assert report_checks.block_reference_stable(before, after)
  let changed = report_checks.parse_lsblk_json("{\"blockdevices\":[" + first + "]}")?
  assert ! report_checks.block_reference_stable(before, changed)
}

test test_system_report_lsblk_queue_json_scores_supported_fields {
  let output = """{"blockdevices":[{"kname":"sda","sched":"mq-deadline","ra":128,"disc-gran":4096,"disc-max":1048576,"model":"Fixture Disk   ","rev":"1.0"},{"kname":"loop0","sched":null,"ra":null,"disc-gran":null,"disc-max":null,"model":null,"rev":null}]}"""
  let reference = report_checks.parse_lsblk_queue_json(output)?
  assert reference.len() == 2
  assert reference[0].model == "Fixture Disk"
  assert reference[0].revision_hint == "1.0"
  let candidate = """{"storage":{"devices":[{"name":"loop0","active_scheduler":null,"read_ahead_kb":null,"discard_granularity_bytes":null,"discard_max_bytes":null,"model":{"state":"absent","value":null}},{"name":"sda","active_scheduler":"mq-deadline","read_ahead_kb":128,"discard_granularity_bytes":4096,"discard_max_bytes":1048576,"model":{"state":"observed","value":"Fixture Disk"}}]}}"""
  let exact = report_checks.compare_block_queue_fields(candidate, reference)?
  assert exact.exact
  assert exact.matched_count == 2
  let wrong = json.encode(json.set(json.decode(candidate)?, ["storage", "devices", 1, "read_ahead_kb"], 256)?)?
  let compared = report_checks.compare_block_queue_fields(wrong, reference)?
  assert compared.read_ahead_mismatches == 1
  assert ! compared.exact
  let projected = report_checks.parse_lsblk_queue_json(
    """{"blockdevices":[{"kname":"sda1","type":"part","sched":"mq-deadline","ra":128,"disc-gran":4096,"disc-max":1048576,"model":null,"rev":null}]}""",
  )?
  let partition = """{"storage":{"devices":[{"name":"sda1","kind":"partition","active_scheduler":null,"read_ahead_kb":null,"discard_granularity_bytes":null,"discard_max_bytes":null,"model":{"state":"absent","value":null}}]}}"""
  assert report_checks.compare_block_queue_fields(partition, projected)?.exact
}

test test_system_report_lsblk_queue_json_rejects_duplicate_and_unsafe_rows {
  let row = """{"kname":"sda","sched":"none","ra":128,"disc-gran":4096,"disc-max":1048576,"model":null,"rev":null}"""
  test.error_kind(
    report_checks.parse_lsblk_queue_json("{\"blockdevices\":[" + row + "," + row + "]}"),
    "SystemReportCheckError.Invalid",
  )?
  let unsafe = """{"kname":"sda","sched":"none","ra":9007199254740992,"disc-gran":4096,"disc-max":1048576,"model":null,"rev":null}"""
  test.error_kind(
    report_checks.parse_lsblk_queue_json("{\"blockdevices\":[" + unsafe + "]}"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_block_queue_raw_sources_bracket_counters_and_firmware {
  let output = """{"blockdevices":[{"kname":"sda","sched":"none","ra":128,"disc-gran":4096,"disc-max":1048576,"model":"Fixture Disk","rev":null}]}"""
  let queue = report_checks.parse_lsblk_queue_json(output)?
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/block/sda/device", parents: true)?
  root.write(
    p"sys/class/block/sda/device/firmware_rev",
    """firmware-7
""",
  )?
  root.write(
    p"sys/class/block/sda/stat",
    """10 0 8 1 2 0 16 2 0 3 4
""",
  )?
  let before = report_checks.read_block_queue_sources(root, queue)?
  assert before[0].firmware == "firmware-7"
  assert before[0].counters.len() == 11
  root.write(
    p"sys/class/block/sda/stat",
    """12 0 8 1 2 0 16 2 0 3 4
""",
  )?
  let after = report_checks.read_block_queue_sources(root, queue)?
  let candidate = """{"storage":{"devices":[{"name":"sda","firmware":{"state":"observed","value":"firmware-7"},"io_counters":[{"name":"read_ios","value":11,"unit":"requests"},{"name":"read_merges","value":0,"unit":"requests"},{"name":"read_sectors","value":8,"unit":"sectors"},{"name":"read_ms","value":1,"unit":"milliseconds"},{"name":"write_ios","value":2,"unit":"requests"},{"name":"write_merges","value":0,"unit":"requests"},{"name":"write_sectors","value":16,"unit":"sectors"},{"name":"write_ms","value":2,"unit":"milliseconds"},{"name":"in_flight","value":0,"unit":"requests"},{"name":"io_ms","value":3,"unit":"milliseconds"},{"name":"weighted_io_ms","value":4,"unit":"milliseconds"}]}]}}"""
  let compared = report_checks.compare_block_queue_sources(candidate, before, after)?
  assert compared.exact
  assert compared.counter_mismatches == 0
  let outside = json.encode(
    json.set(json.decode(candidate)?, ["storage", "devices", 0, "io_counters", 0, "value"], 13)?,
  )?
  assert report_checks.compare_block_queue_sources(outside, before, after)?.counter_mismatches == 1
  let firmware_wrong = json.encode(
    json.set(json.decode(candidate)?, ["storage", "devices", 0, "firmware", "value"], "other")?,
  )?
  assert report_checks.compare_block_queue_sources(firmware_wrong, before, after)?.firmware_mismatches == 1
  let gauge_changed = json.encode(
    json.set(json.decode(candidate)?, ["storage", "devices", 0, "io_counters", 8, "value"], 1)?,
  )?
  assert report_checks.compare_block_queue_sources(gauge_changed, before, after)?.unstable
  root.write(
    p"sys/class/block/sda/stat",
    """12 0 8 1 2 0 16 2 0 3 4 5 6 7 8 9 10
""",
  )?
  assert report_checks.read_block_queue_sources(root, queue)?[0].counters.len() == 17
}

test test_system_report_block_queue_raw_sources_reject_incomplete_and_unsafe_stats {
  let queue = report_checks.parse_lsblk_queue_json(
    """{"blockdevices":[{"kname":"sda","sched":null,"ra":null,"disc-gran":null,"disc-max":null,"model":null,"rev":null}]}""",
  )?
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/block/sda", parents: true)?
  root.write(
    p"sys/class/block/sda/stat",
    """1 0 8 1 2 0 16 2 0 3 4 5 6
""",
  )?
  test.error_kind(report_checks.read_block_queue_sources(root, queue), "SystemReportCheckError.Invalid")?
  root.write(
    p"sys/class/block/sda/stat",
    """9007199254740992 0 8 1 2 0 16 2 0 3 4
""",
  )?
  test.error_kind(report_checks.read_block_queue_sources(root, queue), "SystemReportCheckError.Invalid")?
  let unsafe_name = report_checks.parse_lsblk_queue_json(
    """{"blockdevices":[{"kname":"../outside","sched":null,"ra":null,"disc-gran":null,"disc-max":null,"model":null,"rev":null}]}""",
  )?
  test.error_kind(report_checks.read_block_queue_sources(root, unsafe_name), "SystemReportCheckError.Invalid")?
}

test test_system_report_findmnt_json_scores_repeated_targets_and_redaction {
  let output = """{"filesystems":[{"id":12,"parent":1,"maj:min":"8:1","fsroot":"/","target":"/mnt/data","fstype":"ext4","source":"/dev/sda1","vfs-options":"rw,relatime","fs-options":"rw,errors=remount-ro,password=private","propagation":"private"},{"id":13,"parent":12,"maj:min":"0:2","fsroot":"/","target":"/mnt/data","fstype":"cifs","source":"//user:private@server/share","vfs-options":"rw,nosuid","fs-options":"rw","propagation":"shared"}]}"""
  let reference = report_checks.parse_findmnt_json(output)?
  assert reference.len() == 2
  assert reference[1].parent_id == 12
  let nsfs = """{"filesystems":[{"id":320,"parent":28,"maj:min":"0:4","fsroot":"net:[4026532945]","target":"/run/netns/demo","fstype":"nsfs","source":"nsfs","vfs-options":"rw","fs-options":"rw","propagation":"private"}]}"""
  assert report_checks.parse_findmnt_json(nsfs)?[0].root == "net:[4026532945]"
  let candidate = """{"storage":{"mounts":[{"mount_id":13,"parent_id":12,"major":0,"minor":2,"root":{"state":"observed","value":"/"},"target":{"state":"observed","value":"/mnt/data"},"mount_options":["rw","nosuid"],"optional_fields":["shared:8"],"filesystem":"cifs","source":{"state":"redacted","value":null},"super_options":["rw"]},{"mount_id":12,"parent_id":1,"major":8,"minor":1,"root":{"state":"observed","value":"/"},"target":{"state":"observed","value":"/mnt/data"},"mount_options":["rw","relatime"],"optional_fields":[],"filesystem":"ext4","source":{"state":"observed","value":"/dev/sda1"},"super_options":["rw","errors=remount-ro","redacted"]}]}}"""
  let exact = report_checks.compare_mounts(candidate, reference)?
  assert exact.exact
  assert exact.matched_count == 2
  let unrelated_optional = json.encode(
    json.set(json.decode(candidate)?, ["storage", "mounts", 1, "optional_fields"], ["redacted"])?,
  )?
  assert report_checks.compare_mounts(unrelated_optional, reference)?.exact
  let changed = json.encode(json.set(json.decode(candidate)?, ["storage", "mounts", 0, "parent_id"], 1)?)?
  let mismatch = report_checks.compare_mounts(changed, reference)?
  assert mismatch.parent_mismatches == 1
  assert ! mismatch.exact
  let unredacted = json.encode(
    json.set(
      json.decode(candidate)?,
      ["storage", "mounts", 0, "source"],
      {state: "observed", value: "//user:private@server/share"},
    )?,
  )?
  assert report_checks.compare_mounts(unredacted, reference)?.source_mismatches == 1
  assert report_checks.mount_reference_stable(reference, report_checks.parse_findmnt_json(output)?)
}

test test_system_report_findmnt_json_rejects_ambiguous_rows {
  let base = """{"id":12,"parent":1,"maj:min":"8:1","fsroot":"/","target":"/mnt/data","fstype":"ext4","source":"/dev/sda1","vfs-options":"rw","fs-options":"rw","propagation":"private"}"""
  test.error_kind(
    report_checks.parse_findmnt_json("{\"filesystems\":[" + base + "," + base + "]}"),
    "SystemReportCheckError.Invalid",
  )?
  let malformed = """{"id":12,"parent":1,"maj:min":"8:x","fsroot":"/","target":"/mnt/data","fstype":"ext4","source":"/dev/sda1","vfs-options":"rw","fs-options":"rw","propagation":"private"}"""
  test.error_kind(
    report_checks.parse_findmnt_json("{\"filesystems\":[" + malformed + "]}"),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_mountinfo_raw_reference_preserves_ids_escapes_and_options {
  let raw = """12 1 8:1 / /mnt/data rw,relatime shared:8 - ext4 /dev/sda1 rw,errors=remount-ro,password=private
13 12 0:2 /tenant\\040one /mnt/tenant\\040one rw master:8 - tmpfs tmpfs rw
"""
  let reference = report_checks.parse_mountinfo_raw_reference(raw)?
  assert reference.len() == 2
  assert reference[0].mount_id == 12
  assert reference[0].parent_id == 1
  assert reference[0].major == 8
  assert reference[0].minor == 1
  assert reference[0].propagation == "shared"
  assert reference[0].optional_fields == ["shared:8"]
  assert reference[0].super_options == ["rw", "errors=remount-ro", "password=private"]
  assert reference[1].root == "/tenant one"
  assert reference[1].target == "/mnt/tenant one"
  assert reference[1].propagation == "slave"
  assert reference[1].optional_fields == ["master:8"]
  let candidate = """{"storage":{"mounts":[{"mount_id":12,"parent_id":1,"major":8,"minor":1,"root":{"state":"observed","value":"/"},"target":{"state":"observed","value":"/mnt/data"},"mount_options":["rw","relatime"],"optional_fields":["shared:8"],"filesystem":"ext4","source":{"state":"observed","value":"/dev/sda1"},"super_options":["rw","errors=remount-ro","redacted"]},{"mount_id":13,"parent_id":12,"major":0,"minor":2,"root":{"state":"observed","value":"/tenant one"},"target":{"state":"observed","value":"/mnt/tenant one"},"mount_options":["rw"],"optional_fields":["master:8"],"filesystem":"tmpfs","source":{"state":"observed","value":"tmpfs"},"super_options":["rw"]}]}}"""
  assert report_checks.compare_mounts(candidate, reference)?.exact
  let changed_group = json.encode(
    json.set(json.decode(candidate)?, ["storage", "mounts", 0, "optional_fields", 0], "shared:9")?,
  )?
  let changed_group_result = report_checks.compare_mounts(changed_group, reference)?
  assert changed_group_result.propagation_mismatches == 1
  assert ! changed_group_result.exact
  let exposed_option = json.encode(
    json.set(json.decode(candidate)?, ["storage", "mounts", 0, "super_options", 2], "password=private")?,
  )?
  assert report_checks.compare_mounts(exposed_option, reference)?.option_mismatches == 1
  let future_field = report_checks.parse_mountinfo_raw_reference("""14 1 0:3 / /mnt/future rw idmapped - tmpfs tmpfs rw
""")?
  assert future_field[0].optional_fields == ["redacted"]
  assert future_field[0].propagation == "private"
  test.error_kind(
    report_checks.parse_mountinfo_raw_reference(
  raw + """12 1 8:1 / /dup rw - ext4 /dev/sda1 rw
""",
),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_mountinfo_raw_reference("""12 1 8:1 / /mnt rw ext4 /dev/sda1 rw
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_mountinfo_raw_reference("""9007199254740992 1 8:1 / /mnt rw - ext4 /dev/sda1 rw
"""),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_mountinfo_capture_validates_saved_bytes_and_oracle {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  let raw = """12 1 8:1 / /mnt/data rw,relatime shared:8 - ext4 /dev/sda1 rw
13 12 0:2 / /mnt/shared rw - tmpfs tmpfs rw
"""
  source.mkdir(p"proc/self", parents: true)?
  source.write(p"proc/self/mountinfo", raw)?
  report_checks.capture_mountinfo_bundle(source, bundle, "synthetic_fixture")?
  assert report_checks.validate_mountinfo_bundle(bundle)?.len() == 2
  let metadata = bundle.read_text(p"capture.json")?
  let changed_stability = json.set(json.decode(metadata)?, ["stable"], false)?
  bundle.write_atomic(p"capture.json", json.encode(changed_stability)?)?
  assert report_checks.validate_mountinfo_bundle(bundle)?.len() == 2
  let legacy = json.set(json.decode(metadata)?, ["schema_version"], 1)?
  bundle.write_atomic(p"capture.json", json.encode(legacy)?)?
  test.error_kind(report_checks.validate_mountinfo_bundle(bundle), "SystemReportCheckError.Invalid")?
  let changed_group = json.set(json.decode(metadata)?, ["reference", 0, "optional_fields", 0], "shared:9")?
  bundle.write_atomic(p"capture.json", json.encode(changed_group)?)?
  test.error_kind(report_checks.validate_mountinfo_bundle(bundle), "SystemReportCheckError.Invalid")?
  let contradictory = json.set(json.decode(metadata)?, ["errno"], 13)?
  bundle.write_atomic(p"capture.json", json.encode(contradictory)?)?
  test.error_kind(report_checks.validate_mountinfo_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write_atomic(p"capture.json", metadata)?
  bundle.write(p"proc/self/mountinfo", raw.replace("/mnt/shared", "/mnt/other"))?
  test.error_kind(report_checks.validate_mountinfo_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_mountinfo_capture_preserves_absent_source_without_scoring {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  report_checks.capture_mountinfo_bundle(source, bundle, "synthetic_fixture")?
  assert "\"source_state\": \"absent\"" in bundle.read_text(p"capture.json")?
  test.error_kind(report_checks.validate_mountinfo_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_mountinfo_capture_replays_production_storage_collector {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc/self", parents: true)?
  source.write(
    p"proc/self/mountinfo",
    """12 1 8:1 / /mnt/data rw - ext4 /dev/sda1 rw
""",
  )?
  report_checks.capture_mountinfo_bundle(source, bundle, "synthetic_fixture")?
  assert report_checks.replay_mountinfo_bundle(bundle)?.exact
}

test test_system_report_findmnt_usage_scores_safe_mounts_and_explicit_skips {
  let mounts = report_checks.parse_findmnt_json(
    """{"filesystems":[{"id":1,"parent":0,"maj:min":"8:1","fsroot":"/","target":"/","fstype":"ext4","source":"/dev/sda1","vfs-options":"rw","fs-options":"rw","propagation":"private"},{"id":2,"parent":1,"maj:min":"0:2","fsroot":"/","target":"/auto","fstype":"autofs","source":"autofs","vfs-options":"rw","fs-options":"rw","propagation":"private"},{"id":3,"parent":2,"maj:min":"8:3","fsroot":"/","target":"/auto/local","fstype":"ext4","source":"/dev/sdb1","vfs-options":"rw","fs-options":"rw","propagation":"private"},{"id":4,"parent":1,"maj:min":"0:4","fsroot":"/","target":"/shared","fstype":"tmpfs","source":"tmpfs","vfs-options":"rw","fs-options":"rw","propagation":"private"},{"id":5,"parent":1,"maj:min":"0:5","fsroot":"/","target":"/shared","fstype":"tmpfs","source":"tmpfs","vfs-options":"rw","fs-options":"rw","propagation":"private"}]}""",
  )?
  assert report_checks.mount_usage_eligible_ids(mounts) == [1]
  let before = [
    report_checks.parse_findmnt_usage_json("""{"filesystems":[{"id":1,"size":1000,"used":300,"avail":650}]}""", 1)?,
  ]
  let after = [
    report_checks.parse_findmnt_usage_json("""{"filesystems":[{"id":1,"size":1000,"used":320,"avail":630}]}""", 1)?,
  ]
  let candidate = """{"storage":{"mounts":[{"mount_id":1,"usage_state":"observed","usage_total_bytes":1000,"usage_used_bytes":310,"usage_available_bytes":640},{"mount_id":2,"usage_state":"not_requested","usage_total_bytes":null,"usage_used_bytes":null,"usage_available_bytes":null},{"mount_id":3,"usage_state":"not_requested","usage_total_bytes":null,"usage_used_bytes":null,"usage_available_bytes":null},{"mount_id":4,"usage_state":"not_requested","usage_total_bytes":null,"usage_used_bytes":null,"usage_available_bytes":null},{"mount_id":5,"usage_state":"not_requested","usage_total_bytes":null,"usage_used_bytes":null,"usage_available_bytes":null}]}}"""
  assert report_checks.compare_mount_usage(candidate, mounts, before, after)?.exact
  let unsafe_value = json.encode(
    json.set(json.decode(candidate)?, ["storage", "mounts", 3, "usage_state"], "observed")?,
  )?
  assert ! report_checks.compare_mount_usage(unsafe_value, mounts, before, after)?.exact
  let wrong_usage = json.encode(json.set(json.decode(candidate)?, ["storage", "mounts", 0, "usage_used_bytes"], 321)?)?
  assert ! report_checks.compare_mount_usage(wrong_usage, mounts, before, after)?.exact
}

test test_system_report_findmnt_usage_rejects_ambiguous_or_unsafe_rows {
  test.error_kind(
    report_checks.parse_findmnt_usage_json(
  """{"filesystems":[{"id":1,"size":100,"used":20,"avail":80},{"id":1,"size":100,"used":20,"avail":80}]}""",
  1,
),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_findmnt_usage_json("""{"filesystems":[{"id":1,"size":100,"used":101,"avail":0}]}""", 1),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_findmnt_usage_json("""{"filesystems":[{"id":2,"size":100,"used":20,"avail":80}]}""", 1),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_findmnt_usage_json("""{"filesystems":[{"id":1,"size":null,"used":20,"avail":null}]}""", 1),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_findmnt_usage_keeps_unavailable_capacity_explicit {
  let mounts = report_checks.parse_findmnt_json(
    """{"filesystems":[{"id":8,"parent":0,"maj:min":"0:8","fsroot":"/","target":"/opaque","fstype":"overlay","source":"overlay","vfs-options":"rw","fs-options":"rw","propagation":"private"}]}""",
  )?
  let unavailable = report_checks.parse_findmnt_usage_json(
    """{"filesystems":[{"id":8,"size":null,"used":null,"avail":null}]}""",
    8,
  )?
  let candidate = """{"storage":{"mounts":[{"mount_id":8,"usage_state":"disappeared","usage_total_bytes":null,"usage_used_bytes":null,"usage_available_bytes":null}]}}"""
  assert report_checks.compare_mount_usage(candidate, mounts, [unavailable], [unavailable])?.exact
  let invented = json.encode(json.set(json.decode(candidate)?, ["storage", "mounts", 0, "usage_state"], "observed")?)?
  assert ! report_checks.compare_mount_usage(invented, mounts, [unavailable], [unavailable])?.exact
  let observed = report_checks.parse_findmnt_usage_json(
    """{"filesystems":[{"id":8,"size":100,"used":20,"avail":80}]}""",
    8,
  )?
  assert report_checks.compare_mount_usage(candidate, mounts, [unavailable], [observed])?.unstable
}

test test_system_report_lsmod_reference_scores_module_values_and_state {
  let formatted = """Module                  Size  Used by
alpha                  4096  2 beta,gamma
beta                   8192  0
"""
  let raw = """alpha 4096 2 beta,gamma, Live 0x0
beta 8192 0 - Loading 0x0
"""
  let reference = report_checks.parse_lsmod_reference(formatted, raw)?
  assert reference.len() == 2
  assert reference[0].state == "Live"
  let candidate = """{"kernel":{"status":{"state":"complete","enumeration_succeeded":true},"modules":[{"name":"beta","size_bytes":8192,"users":0,"state":"Loading"},{"name":"alpha","size_bytes":4096,"users":2,"state":"Live"}]},"issues":[]}"""
  let exact = report_checks.compare_kernel_modules(candidate, reference)?
  assert exact.exact
  assert exact.matched_count == 2
  let changed = json.encode(json.set(json.decode(candidate)?, ["kernel", "modules", 0, "state"], "Live")?)?
  let mismatch = report_checks.compare_kernel_modules(changed, reference)?
  assert mismatch.state_mismatches == 1
  assert ! mismatch.exact
  let missing = report_checks.compare_kernel_modules(
    """{"kernel":{"status":{"state":"complete","enumeration_succeeded":true},"modules":[]},"issues":[]}""",
    reference,
  )?
  assert missing.missing_names == ["alpha", "beta"]
  assert ! missing.exact
  let failed_empty = report_checks.compare_kernel_modules(
    """{"kernel":{"status":{"state":"partial","enumeration_succeeded":false},"modules":[]},"issues":[]}""",
    [],
  )?
  assert ! failed_empty.exact
  assert ! failed_empty.enumeration_succeeded
  let issue = json.encode(
    json.set(json.decode(candidate)?, ["issues"], [{section: "kernel", field: "modules.line.0"}])?,
  )?
  let with_issue = report_checks.compare_kernel_modules(issue, reference)?
  assert with_issue.source_issue
  assert ! with_issue.exact
  let partial_kernel = json.set(json.decode(candidate)?, ["kernel", "status", "state"], "partial")?
  let unrelated_issue = json.encode(json.set(partial_kernel, ["issues"], [{section: "kernel", field: "command_line"}])?)?
  let with_unrelated_issue = report_checks.compare_kernel_modules(unrelated_issue, reference)?
  assert with_unrelated_issue.enumeration_succeeded
  assert ! with_unrelated_issue.source_issue
  assert with_unrelated_issue.exact
  assert report_checks.kernel_module_reference_stable(reference, report_checks.parse_lsmod_reference(formatted, raw)?)
}

test test_system_report_lsmod_reference_rejects_conflicting_or_unsafe_rows {
  let header = """Module Size Used by
"""
  assert report_checks.parse_lsmod_reference(
    header + """alpha 4096 0
""",
    """alpha 4096 0 - Live 0x0 (OE)
""",
  )?.len() == 1
  test.error_kind(
    report_checks.parse_lsmod_reference(
  header + """alpha 4096 0
""",
  """alpha 4096 0 - Live 0x0 (OE) extra
""",
),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_lsmod_reference(
  header + """alpha 4096 0
""",
  """alpha 4096 1 - Live 0x0
""",
),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_lsmod_reference(
  header + """alpha 9007199254740992 0
""",
  """alpha 9007199254740992 0 - Live 0x0
""",
),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_lsmod_reference(
  header + """alpha 4096 0
alpha 4096 0
""",
  """alpha 4096 0 - Live 0x0
""",
),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_lsmod_reference(
  header + """alpha 4096 0
""",
  """beta 4096 0 - Live 0x0
""",
),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_proc_modules_raw_reference_requires_complete_unique_rows {
  let reference = report_checks.parse_proc_modules_raw_reference("""alpha 4096 0 - Live 0x0
beta 8192 1 alpha Live 0x1
""")?
  assert reference.len() == 2
  assert reference[0].name == "alpha"
  assert reference[1].users == 1
  assert reference[1].state == "Live"
  assert report_checks.parse_proc_modules_raw_reference("")?.len() == 0
  assert report_checks.parse_proc_modules_raw_reference("""alpha 4096 0 - Live 0x0 (OE)
""")?.len() == 1
  test.error_kind(
    report_checks.parse_proc_modules_raw_reference("""alpha 4096 0 - Live 0x0 (OE) extra
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_modules_raw_reference("""alpha 4096 0 - Live
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_modules_raw_reference("""alpha 4096 0 - Live 0x0
alpha 4096 0 - Live 0x0
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_proc_modules_raw_reference("""alpha 9007199254740992 0 - Live 0x0
"""),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_proc_modules_preserves_unavailable_use_count {
  let reference = report_checks.parse_proc_modules_raw_reference("""permanent 4096 - - Live 0x0
""")?
  assert reference.len() == 1
  assert reference[0].users == null
  let candidate = """{"kernel":{"status":{"state":"complete","enumeration_succeeded":true},"modules":[{"name":"permanent","size_bytes":4096,"users":null,"state":"Live"}]},"issues":[]}"""
  assert report_checks.compare_kernel_modules(candidate, reference)?.exact
  let invented = json.encode(json.set(json.decode(candidate)?, ["kernel", "modules", 0, "users"], 0)?)?
  assert report_checks.compare_kernel_modules(invented, reference)?.users_mismatches == 1
  let omitted = """{"kernel":{"status":{"state":"complete","enumeration_succeeded":true},"modules":[{"name":"permanent","size_bytes":4096,"state":"Live"}]},"issues":[]}"""
  test.error_kind(report_checks.compare_kernel_modules(omitted, reference), "schema")?
  test.error_kind(
    report_checks.parse_proc_modules_raw_reference("""permanent 4096 unknown - Live 0x0
"""),
    "SystemReportCheckError.Invalid",
  )?
}

test test_system_report_kernel_modules_capture_validates_raw_source_and_oracle {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc", parents: true)?
  source.write(
    p"proc/modules",
    """alpha 4096 0 - Live 0x0
beta 8192 1 alpha Live 0x1
""",
  )?
  report_checks.capture_kernel_modules_bundle(source, bundle, "synthetic_fixture")?
  let reference = report_checks.validate_kernel_modules_bundle(bundle)?
  assert reference.len() == 2
  assert reference[1].name == "beta"
  source.write(
    p"proc/modules",
    """changed 1 0 - Live 0x0
""",
  )?
  assert report_checks.validate_kernel_modules_bundle(bundle)?.len() == 2
  let metadata = bundle.read_text(p"capture.json")?
  assert "\"users\": 1" in metadata
  bundle.write(p"capture.json", metadata.replace("\"users\": 1", "\"users\": 2"))?
  test.error_kind(report_checks.validate_kernel_modules_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(p"capture.json", metadata)?
  assert "\"errno\": null" in metadata
  bundle.write(p"capture.json", metadata.replace("\"errno\": null", "\"errno\": 13"))?
  test.error_kind(report_checks.validate_kernel_modules_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(p"capture.json", metadata)?
  bundle.write(
    p"proc/modules",
    """alpha 4096 0 - Live 0x0
beta 8192 2 alpha Live 0x1
""",
  )?
  test.error_kind(report_checks.validate_kernel_modules_bundle(bundle), "SystemReportCheckError.Invalid")?
  let unavailable_bundle = fs.tempdir()?
  defer unavailable_bundle.close()?
  source.write(
    p"proc/modules",
    """permanent 4096 - - Live 0x0
""",
  )?
  report_checks.capture_kernel_modules_bundle(source, unavailable_bundle, "synthetic_fixture")?
  assert report_checks.validate_kernel_modules_bundle(unavailable_bundle)?[0].users == null
}

test test_system_report_kernel_modules_capture_marks_absent_and_malformed_unscoreable {
  let source = fs.tempdir()?
  defer source.close()?
  let absent_bundle = fs.tempdir()?
  defer absent_bundle.close()?
  report_checks.capture_kernel_modules_bundle(source, absent_bundle, "synthetic_fixture")?
  assert "\"source_state\": \"absent\"" in absent_bundle.read_text(p"capture.json")?
  test.error_kind(report_checks.validate_kernel_modules_bundle(absent_bundle), "SystemReportCheckError.Invalid")?
  source.mkdir(p"proc", parents: true)?
  source.write(
    p"proc/modules",
    """broken row
""",
  )?
  let malformed_bundle = fs.tempdir()?
  defer malformed_bundle.close()?
  report_checks.capture_kernel_modules_bundle(source, malformed_bundle, "synthetic_fixture")?
  assert "\"reference\": null" in malformed_bundle.read_text(p"capture.json")?
  test.error_kind(report_checks.validate_kernel_modules_bundle(malformed_bundle), "SystemReportCheckError.Invalid")?
  source.write(p"proc/modules", "")?
  let empty_bundle = fs.tempdir()?
  defer empty_bundle.close()?
  report_checks.capture_kernel_modules_bundle(source, empty_bundle, "synthetic_fixture")?
  assert report_checks.validate_kernel_modules_bundle(empty_bundle)?.len() == 0
}

test test_system_report_kernel_modules_capture_replays_production_collector {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc", parents: true)?
  source.write(
    p"proc/modules",
    """alpha 4096 0 - Live 0x0
beta 8192 1 alpha Live 0x1
""",
  )?
  report_checks.capture_kernel_modules_bundle(source, bundle, "synthetic_fixture")?
  assert report_checks.replay_kernel_modules_bundle(bundle)?.exact
}

test test_system_report_kernel_command_line_reference_preserves_bytes_and_redaction {
  let raw = b"  root=UUID=private  quiet  \n"
  let sensitive = json.encode({
    kernel: {
      command_line: {
        state: "observed",
        value: "  root=UUID=private  quiet  " + "\n",
        raw_bytes_base64: null,
      },
    },
  })?
  let redacted = json.encode({kernel: {command_line: {state: "redacted", value: null, raw_bytes_base64: null}}})?
  assert report_checks.compare_kernel_command_line(sensitive, redacted, raw)?.exact
  let trimmed = json.encode(
    {kernel: {command_line: {state: "observed", value: "root=UUID=private  quiet", raw_bytes_base64: null}}},
  )?
  assert ! report_checks.compare_kernel_command_line(trimmed, redacted, raw)?.exact
  assert ! report_checks.compare_kernel_command_line(sensitive, sensitive, raw)?.exact

  let malformed = b"\xff private\n"
  let encoded = malformed.base64()
  let raw_sensitive = json.encode(
    {kernel: {command_line: {state: "malformed", value: null, raw_bytes_base64: encoded}}},
  )?
  assert report_checks.compare_kernel_command_line(raw_sensitive, redacted, malformed)?.exact
  let leaked = json.encode({kernel: {command_line: {state: "redacted", value: null, raw_bytes_base64: encoded}}})?
  assert ! report_checks.compare_kernel_command_line(raw_sensitive, leaked, malformed)?.exact
}

test test_system_report_kernel_parameter_reference_scores_values_and_absence {
  let reference: List[KernelParameterReferenceFixture] = [
    {
      name: "kernel.pid_max",
      source: "sysctl",
      state: "observed",
      value: "4194304",
      raw_bytes_base64: null,
    },
    {
      name: "intel_pstate.no_turbo",
      source: "module",
      state: "absent",
      value: null,
      raw_bytes_base64: null,
    },
  ]
  let candidate = json.encode({
    kernel: {
      sysctls: [{name: "kernel.pid_max", value: {state: "observed", value: "4194304", raw_bytes_base64: null}}],
      parameters: [{name: "intel_pstate.no_turbo", value: {state: "absent", value: null, raw_bytes_base64: null}}],
    },
  })?
  assert report_checks.compare_kernel_parameters(candidate, reference)?.exact
  let wrong = json.encode(json.set(json.decode(candidate)?, ["kernel", "sysctls", 0, "value", "value"], "4194303")?)?
  assert ! report_checks.compare_kernel_parameters(wrong, reference)?.exact
  let invented = json.encode(
    json.set(json.decode(candidate)?, ["kernel", "parameters", 0, "value", "state"], "observed")?,
  )?
  assert ! report_checks.compare_kernel_parameters(invented, reference)?.exact
}

test test_system_report_kernel_parameter_reference_rejects_duplicate_or_unexpected_names {
  let reference = [
    report_checks.KernelParameterReference(
      name: "kernel.pid_max",
      source: "sysctl",
      state: "observed",
      value: "4194304",
      raw_bytes_base64: null,
    ),
  ]
  test.error_kind(
    report_checks.compare_kernel_parameters("{}", reference.extend(reference)),
    "SystemReportCheckError.Invalid",
  )?
  let invalid = [
    report_checks.KernelParameterReference(
      name: "kernel.pid_max",
      source: "sysctl",
      state: "absent",
      value: "4194304",
      raw_bytes_base64: null,
    ),
  ]
  test.error_kind(report_checks.compare_kernel_parameters("{}", invalid), "SystemReportCheckError.Invalid")?
  let repeated = json.encode({
    kernel: {
      sysctls: [{name: "kernel.pid_max", value: {state: "observed", value: "4194304", raw_bytes_base64: null}}, {
    name: "kernel.pid_max",
    value: {
      state: "observed",
      value: "4194304",
      raw_bytes_base64: null,
    },
  }],
      parameters: [],
    },
  })?
  test.error_kind(report_checks.compare_kernel_parameters(repeated, reference), "SystemReportCheckError.Invalid")?
  let extra = json.encode({
    kernel: {
      sysctls: [{name: "kernel.pid_max", value: {state: "observed", value: "4194304", raw_bytes_base64: null}}, {
    name: "kernel.hostname",
    value: {
      state: "observed",
      value: "private",
      raw_bytes_base64: null,
    },
  }],
      parameters: [],
    },
  })?
  assert ! report_checks.compare_kernel_parameters(extra, reference)?.exact
}

test test_system_report_kernel_parameter_capture_validates_observed_absent_and_tampered_sources {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc/sys/kernel", parents: true)?
  source.mkdir(p"sys/module/usbcore/parameters", parents: true)?
  source.write(
    p"proc/sys/kernel/pid_max",
    """4194304
""",
  )?
  source.write(
    p"sys/module/usbcore/parameters/autosuspend",
    """2
""",
  )?
  report_checks.capture_kernel_parameters_bundle(source, bundle, "synthetic_fixture")?
  let reference = report_checks.validate_kernel_parameters_bundle(bundle)?
  assert reference.len() == 9
  assert reference[0].name == "kernel.pid_max"
  assert reference[0].value == "4194304"
  assert reference[1].state == "absent"
  assert reference[6].name == "usbcore.autosuspend"
  assert reference[6].value == "2"
  source.write(
    p"proc/sys/kernel/pid_max",
    """changed
""",
  )?
  assert report_checks.validate_kernel_parameters_bundle(bundle)?[0].value == "4194304"
  let metadata = bundle.read_text(p"capture.json")?
  let changed_oracle = json.encode(json.set(json.decode(metadata)?, ["reference", 0, "value"], "1")?)?
  bundle.write(p"capture.json", changed_oracle)?
  test.error_kind(report_checks.validate_kernel_parameters_bundle(bundle), "SystemReportCheckError.Invalid")?
  let contradictory = json.encode(json.set(json.decode(metadata)?, ["sources", 0, "errno"], 13)?)?
  bundle.write(p"capture.json", contradictory)?
  test.error_kind(report_checks.validate_kernel_parameters_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(p"capture.json", metadata)?
  bundle.write(
    p"proc/sys/kernel/pid_max",
    """1
""",
  )?
  test.error_kind(report_checks.validate_kernel_parameters_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write(
    p"proc/sys/kernel/pid_max",
    """4194304
""",
  )?
  bundle.mkdir(p"proc/sys/vm", parents: true)?
  bundle.write(
    p"proc/sys/vm/swappiness",
    """60
""",
  )?
  test.error_kind(report_checks.validate_kernel_parameters_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_kernel_parameter_capture_preserves_malformed_utf8_and_rejects_truncation {
  let source = fs.tempdir()?
  defer source.close()?
  source.mkdir(p"proc/sys/kernel", parents: true)?
  source.write(p"proc/sys/kernel/pid_max", b"\xff\n")?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  report_checks.capture_kernel_parameters_bundle(source, bundle, "synthetic_fixture")?
  let reference = report_checks.validate_kernel_parameters_bundle(bundle)?
  assert reference[0].state == "malformed"
  assert reference[0].raw_bytes_base64 == b"\xff\n".base64()
  var padding = "x"
  while padding.count_chars() <= 4096 {
    padding = f"${padding}${padding}"
  }

  source.write(p"proc/sys/kernel/pid_max", padding)?
  let truncated = fs.tempdir()?
  defer truncated.close()?
  report_checks.capture_kernel_parameters_bundle(source, truncated, "synthetic_fixture")?
  test.error_kind(report_checks.validate_kernel_parameters_bundle(truncated), "SystemReportCheckError.Invalid")?
}

test test_system_report_kernel_parameter_capture_replays_production_collector {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc/sys/kernel", parents: true)?
  source.write(
    p"proc/sys/kernel/pid_max",
    """4194304
""",
  )?
  report_checks.capture_kernel_parameters_bundle(source, bundle, "synthetic_fixture")?
  assert report_checks.replay_kernel_parameters_bundle(bundle)?.exact
}

test test_system_report_identity_comparison_scores_release_and_architecture_separately {
  let matching = """{"identity":{"kernel_release":"6.1-test","architecture":"aarch64"}}"""
  let exact = report_checks.compare_identity(matching, "6.1-test", "aarch64")?
  assert exact.release_exact
  assert exact.architecture_exact
  assert exact.exact

  let incomplete = """{"identity":{"kernel_release":null,"architecture":"x86_64"}}"""
  let compared = report_checks.compare_identity(incomplete, "6.1-test", "aarch64")?
  assert compared.release_missing
  assert ! compared.release_exact
  assert ! compared.architecture_missing
  assert ! compared.architecture_exact
  assert ! compared.exact
}

test test_system_report_os_release_reference_decodes_data_and_requires_candidate_agreement {
  let reference = report_checks.parse_reference_os_release("""ID=example
VERSION_ID="2026\\"release"
""")?
  assert reference.id == "example"
  assert reference.version_id == "2026\"release"
  let matching = """{"identity":{"os_release":{"id":"example","version_id":"2026\\"release"}}}"""
  assert report_checks.compare_os_release(matching, reference)?.exact
  let missing = """{"identity":{"os_release":null}}"""
  let compared = report_checks.compare_os_release(missing, reference)?
  assert compared.id_missing
  assert compared.version_id_missing
  assert ! compared.exact
  let absent_version = report_checks.parse_reference_os_release("""ID=example
""")?
  assert report_checks.compare_os_release(
    """{"identity":{"os_release":{"id":"example","version_id":null}}}""",
    absent_version,
  )?.exact
  let wrong_version = report_checks.compare_os_release(
    """{"identity":{"os_release":{"id":"example","version_id":"other"}}}""",
    reference,
  )?
  assert wrong_version.id_exact
  assert ! wrong_version.version_id_exact
  assert ! wrong_version.exact
  test.error_kind(
    report_checks.parse_reference_os_release("""VERSION_ID=1
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_os_release("""ID="unterminated
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_os_release("""ID=Unquoted Name
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_os_release("""ID=path/segment
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_os_release("""ID=pipe|value
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_os_release("""ID=escaped\\ space
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_os_release("""ID= leading
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_os_release("""ID=trailing 
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_os_release("""ID="quoted" 
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_os_release("""ID="Not A Distro"
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_os_release("""ID=Ubuntu
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_os_release("""ID=example
VERSION_ID="unescaped $value"
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_os_release(""" ID =example
"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_os_release("""ID=example
not-an-assignment
"""),
    "SystemReportCheckError.Invalid",
  )?
  assert report_checks.parse_reference_os_release("""ID=example
VERSION_ID="v\\$token"
""")?.version_id == "v$token"
  assert report_checks.parse_reference_os_release("""ID=example
VERSION_ID=v1.2-release_3
""")?.version_id == "v1.2-release_3"
  let repeated = report_checks.parse_reference_os_release("""# local source
ID=first
ID=second
VERSION_ID=2
""")?
  assert repeated.id == "second"
  assert repeated.version_id == "2"
}

test test_system_report_device_tree_reference_requires_exact_terminated_bytes_and_order {
  assert report_checks.parse_reference_od_bytes(
    """ 41 52 4d 00
""",
    4,
  )? == b"ARM\0"
  test.error_kind(
    report_checks.parse_reference_od_bytes(
  """ 41 5g 00
""",
  4,
),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_od_bytes(
  """ 41 52 4d 00 00
""",
  4,
),
    "SystemReportCheckError.Invalid",
  )?
  let reference = report_checks.parse_reference_device_tree(b"ARM Board\0", b"vendor,board\0arm,v8\0")?
  assert reference.model == "ARM Board"
  assert reference.compatible == ["vendor,board", "arm,v8"]
  test.error_kind(
    report_checks.parse_reference_device_tree(b"ARM Board", b"vendor,board\0"),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_device_tree(b"ARM\0Board\0", b"vendor,board\0"),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_device_tree(b"ARM Board\0", b"vendor,board\0\0"),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_device_tree(bytes.from_ints([255, 0])?, b"vendor,board\0"),
    "SystemReportCheckError.Invalid",
  )?
  let matching = """{"identity":{"firmware":{"source":"device-tree","device_tree_model":{"state":"observed","value":"ARM Board"},"device_tree_compatible":[{"state":"observed","value":"vendor,board"},{"state":"observed","value":"arm,v8"}]}} ,"issues":[]}"""
  assert report_checks.compare_device_tree(matching, reference, false)?.exact
  let hybrid = """{"identity":{"firmware":{"source":"dmi","vendor":"Example Vendor","product":"Example Host","device_tree_model":{"state":"observed","value":"ARM Board"},"device_tree_compatible":[{"state":"observed","value":"vendor,board"},{"state":"observed","value":"arm,v8"}]}} ,"issues":[]}"""
  assert report_checks.compare_device_tree(hybrid, reference, true)?.exact
  assert ! report_checks.compare_device_tree(hybrid, reference, false)?.source_exact
  let invented_vendor = """{"identity":{"firmware":{"source":"device-tree","vendor":"Invented Vendor","device_tree_model":{"state":"observed","value":"ARM Board"},"device_tree_compatible":[{"state":"observed","value":"vendor,board"},{"state":"observed","value":"arm,v8"}]}} ,"issues":[]}"""
  assert ! report_checks.compare_device_tree(invented_vendor, reference, false)?.source_exact
  let reordered = """{"identity":{"firmware":{"source":"device-tree","device_tree_model":{"state":"observed","value":"ARM Board"},"device_tree_compatible":[{"state":"observed","value":"arm,v8"},{"state":"observed","value":"vendor,board"}]}} ,"issues":[]}"""
  assert ! report_checks.compare_device_tree(reordered, reference, false)?.exact
  let missing = """{"identity":{"firmware":{"source":"dmi","device_tree_model":{"state":"absent","value":null},"device_tree_compatible":[]}} ,"issues":[]}"""
  assert ! report_checks.compare_device_tree(missing, reference, false)?.exact
  let compatible_only = report_checks.parse_reference_device_tree(null, b"vendor,board\0")?
  let compatible_candidate = """{"identity":{"firmware":{"source":"device-tree","device_tree_model":{"state":"absent","value":null},"device_tree_compatible":[{"state":"observed","value":"vendor,board"}]}} ,"issues":[]}"""
  assert report_checks.compare_device_tree(compatible_candidate, compatible_only, false)?.exact
  let failed_model = """{"identity":{"firmware":{"source":"device-tree","device_tree_model":{"state":"absent","value":null},"device_tree_compatible":[{"state":"observed","value":"vendor,board"}]}} ,"issues":[{"field":"firmware.device_tree_model"}]}"""
  assert ! report_checks.compare_device_tree(failed_model, compatible_only, false)?.exact
  let model_only = report_checks.parse_reference_device_tree(b"ARM\0", null)?
  let malformed_optional = """{"identity":{"firmware":{"source":"device-tree","device_tree_model":{"state":"observed","value":"ARM"},"device_tree_compatible":[]}} ,"issues":[{"field":"firmware.device_tree_compatible"}]}"""
  assert ! report_checks.compare_device_tree(malformed_optional, model_only, false)?.exact
}

test test_system_report_device_tree_capture_validates_saved_sources_and_oracle {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/firmware/devicetree/base", parents: true)?
  source.write(p"sys/firmware/devicetree/base/model", b"ARM Board\0")?
  source.write(p"sys/firmware/devicetree/base/compatible", b"vendor,board\0arm,v8\0")?
  report_checks.capture_device_tree_bundle(source, bundle, "synthetic_fixture")?
  let reference = report_checks.validate_device_tree_bundle(bundle)?
  assert reference.model == "ARM Board"
  assert reference.compatible == ["vendor,board", "arm,v8"]
  let metadata = bundle.read_text(p"capture.json")?
  let changed_oracle = json.set(json.decode(metadata)?, ["reference", "model"], "other")?
  bundle.write_atomic(p"capture.json", json.encode(changed_oracle)?)?
  test.error_kind(report_checks.validate_device_tree_bundle(bundle), "SystemReportCheckError.Invalid")?
  let contradictory = json.set(json.decode(metadata)?, ["sources", 0, "errno"], 13)?
  bundle.write_atomic(p"capture.json", json.encode(contradictory)?)?
  test.error_kind(report_checks.validate_device_tree_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write_atomic(p"capture.json", metadata)?
  bundle.write(p"sys/firmware/devicetree/base/model", b"Changed\0")?
  test.error_kind(report_checks.validate_device_tree_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_device_tree_capture_keeps_unavailable_sources_unscoreable {
  let source = fs.tempdir()?
  defer source.close()?
  let absent = fs.tempdir()?
  defer absent.close()?
  report_checks.capture_device_tree_bundle(source, absent, "synthetic_fixture")?
  test.error_kind(report_checks.validate_device_tree_bundle(absent), "SystemReportCheckError.Invalid")?
  source.mkdir(p"sys/firmware/devicetree/base", parents: true)?
  source.write(p"sys/firmware/devicetree/base/compatible", b"vendor,board\0")?
  let compatible_only = fs.tempdir()?
  defer compatible_only.close()?
  report_checks.capture_device_tree_bundle(source, compatible_only, "synthetic_fixture")?
  assert report_checks.validate_device_tree_bundle(compatible_only)?.model == null
  compatible_only.write(p"sys/firmware/devicetree/base/model", b"invented\0")?
  test.error_kind(report_checks.validate_device_tree_bundle(compatible_only), "SystemReportCheckError.Invalid")?
  compatible_only.remove(p"sys/firmware/devicetree/base/model")?
  source.write(p"sys/firmware/devicetree/base/model", b"unterminated")?
  let malformed = fs.tempdir()?
  defer malformed.close()?
  report_checks.capture_device_tree_bundle(source, malformed, "synthetic_fixture")?
  test.error_kind(report_checks.validate_device_tree_bundle(malformed), "SystemReportCheckError.Invalid")?
  var padding = "x"
  while padding.count_chars() <= 4096 {
    padding = f"${padding}${padding}"
  }

  source.write(p"sys/firmware/devicetree/base/model", padding)?
  let truncated = fs.tempdir()?
  defer truncated.close()?
  report_checks.capture_device_tree_bundle(source, truncated, "synthetic_fixture")?
  test.error_kind(report_checks.validate_device_tree_bundle(truncated), "SystemReportCheckError.Invalid")?
}

test test_system_report_device_tree_capture_replays_production_collector {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/firmware/devicetree/base", parents: true)?
  source.write(p"sys/firmware/devicetree/base/model", b"ARM Board\0")?
  source.write(p"sys/firmware/devicetree/base/compatible", b"vendor,board\0arm,v8\0")?
  report_checks.capture_device_tree_bundle(source, bundle, "synthetic_fixture")?
  assert report_checks.replay_device_tree_bundle(bundle)?.exact
}

test test_system_report_dmi_identity_reference_scores_raw_fields_and_sensitive_observations {
  let reference = {
    vendor: {
      value: "Example Vendor",
      complete: true,
    },
    product: {
      value: "Example Host",
      complete: true,
    },
    board_vendor: {
      value: "Board Vendor",
      complete: true,
    },
    board_product: {
      value: "Board A",
      complete: true,
    },
    bios_vendor: {
      value: "BIOS Vendor",
      complete: true,
    },
    bios_version: {
      value: "1.2",
      complete: true,
    },
    serial: {
      value: "serial-123",
      complete: true,
    },
    uuid: {
      value: null,
      complete: true,
    },
  }
  let candidate = """{"identity":{"firmware":{"source":"dmi","vendor":"Example Vendor","product":"Example Host","board_vendor":"Board Vendor","board_product":"Board A","bios_vendor":"BIOS Vendor","bios_version":"1.2","serial":{"state":"observed","value":"serial-123"},"uuid":{"state":"absent","value":null}}},"issues":[]}"""
  assert report_checks.compare_dmi_identity(candidate, reference, reference)?.exact
  let wrong_board = report_checks.compare_dmi_identity(candidate.replace("Board A", "Board B"), reference, reference)?
  assert wrong_board.field_mismatches == ["board_product"]
  let changed_serial = report_checks.compare_dmi_identity(
    candidate,
    reference,
    {...reference, serial: {value: "serial-456", complete: true}},
  )?
  assert changed_serial.unstable_fields == ["serial"]
  let changed_vendor = report_checks.compare_dmi_identity(
    candidate.replace("\"source\":\"dmi\"", "\"source\":\"unavailable\""),
    reference,
    {...reference, vendor: {value: null, complete: false}},
  )?
  assert "source" in changed_vendor.unstable_fields
  assert "source" not in changed_vendor.field_mismatches
  let redacted = report_checks.compare_dmi_identity(
    candidate.replace("\"state\":\"observed\",\"value\":\"serial-123\"", "\"state\":\"redacted\",\"value\":null"),
    reference,
    reference,
  )?
  assert redacted.field_mismatches == ["serial"]
  let default_report = candidate.replace(
    "\"state\":\"observed\",\"value\":\"serial-123\"",
    "\"state\":\"redacted\",\"value\":null",
  )
  assert report_checks.dmi_identity_redacted(default_report, reference)?
  assert ! report_checks.dmi_identity_redacted(candidate, reference)?
  let invented_uuid = default_report.replace(
    "\"state\":\"absent\",\"value\":null",
    "\"state\":\"observed\",\"value\":\"invented\"",
  )
  assert ! report_checks.dmi_identity_redacted(invented_uuid, reference)?
  let leaked_bytes = json.set(
    json.decode(default_report)?,
    ["identity", "firmware", "serial", "raw_bytes_base64"],
    "c2VjcmV0",
  )?
  assert ! report_checks.dmi_identity_redacted(json.encode(leaked_bytes)?, reference)?
  let absent = {
    vendor: {
      value: null,
      complete: true,
    },
    product: {
      value: null,
      complete: true,
    },
    board_vendor: {
      value: null,
      complete: true,
    },
    board_product: {
      value: null,
      complete: true,
    },
    bios_vendor: {
      value: null,
      complete: true,
    },
    bios_version: {
      value: null,
      complete: true,
    },
    serial: {
      value: null,
      complete: true,
    },
    uuid: {
      value: null,
      complete: true,
    },
  }
  let fabricated = report_checks.compare_dmi_identity(candidate, absent, absent)?
  assert ! fabricated.eligible
  assert "vendor" in fabricated.field_mismatches
}

test test_system_report_dmi_identity_rooted_reference_reads_optional_fields {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/dmi/id", parents: true)?
  root.write(
    p"sys/class/dmi/id/sys_vendor",
    """Example Vendor
""",
  )?
  root.write(
    p"sys/class/dmi/id/product_name",
    """Example Host
""",
  )?
  root.write(
    p"sys/class/dmi/id/product_serial",
    """serial-123
""",
  )?
  let reference = report_checks.read_dmi_identity_reference(root)?
  assert reference.vendor == {value: "Example Vendor", complete: true}
  assert reference.serial == {value: "serial-123", complete: true}
  assert reference.uuid == {value: null, complete: true}
}

test test_system_report_dmi_identity_capture_validates_saved_sources_and_oracle {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/class/dmi/id", parents: true)?
  source.write(
    p"sys/class/dmi/id/sys_vendor",
    """Example Vendor
""",
  )?
  source.write(
    p"sys/class/dmi/id/product_name",
    """Example Host
""",
  )?
  source.write(
    p"sys/class/dmi/id/product_serial",
    """serial-123
""",
  )?
  report_checks.capture_dmi_identity_bundle(source, bundle, "synthetic_fixture")?
  let reference = report_checks.validate_dmi_identity_bundle(bundle)?
  assert reference.vendor.value == "Example Vendor"
  assert reference.serial.value == "serial-123"
  assert reference.uuid == {value: null, complete: true}
  source.write(
    p"sys/class/dmi/id/sys_vendor",
    """changed
""",
  )?
  assert report_checks.validate_dmi_identity_bundle(bundle)?.vendor.value == "Example Vendor"
  let metadata = bundle.read_text(p"capture.json")?
  let changed_oracle = json.set(json.decode(metadata)?, ["reference", "vendor", "value"], "other")?
  bundle.write_atomic(p"capture.json", json.encode(changed_oracle)?)?
  test.error_kind(report_checks.validate_dmi_identity_bundle(bundle), "SystemReportCheckError.Invalid")?
  let contradictory = json.set(json.decode(metadata)?, ["sources", 0, "errno"], 13)?
  bundle.write_atomic(p"capture.json", json.encode(contradictory)?)?
  test.error_kind(report_checks.validate_dmi_identity_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write_atomic(p"capture.json", metadata)?
  bundle.write(
    p"sys/class/dmi/id/product_uuid",
    """invented
""",
  )?
  test.error_kind(report_checks.validate_dmi_identity_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.remove(p"sys/class/dmi/id/product_uuid")?
  bundle.write(p"sys/class/dmi/id/sys_vendor", "Example Vendor ")?
  test.error_kind(report_checks.validate_dmi_identity_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_dmi_identity_capture_keeps_unavailable_sources_unscoreable {
  let source = fs.tempdir()?
  defer source.close()?
  let absent = fs.tempdir()?
  defer absent.close()?
  report_checks.capture_dmi_identity_bundle(source, absent, "synthetic_fixture")?
  test.error_kind(report_checks.validate_dmi_identity_bundle(absent), "SystemReportCheckError.Invalid")?
  source.mkdir(p"sys/class/dmi/id", parents: true)?
  source.write(p"sys/class/dmi/id/sys_vendor", b"\xff")?
  let malformed = fs.tempdir()?
  defer malformed.close()?
  report_checks.capture_dmi_identity_bundle(source, malformed, "synthetic_fixture")?
  test.error_kind(report_checks.validate_dmi_identity_bundle(malformed), "SystemReportCheckError.Invalid")?
  var padding = "x"
  while padding.count_chars() <= 4096 {
    padding = f"${padding}${padding}"
  }

  source.write(p"sys/class/dmi/id/sys_vendor", padding)?
  let truncated = fs.tempdir()?
  defer truncated.close()?
  report_checks.capture_dmi_identity_bundle(source, truncated, "synthetic_fixture")?
  test.error_kind(report_checks.validate_dmi_identity_bundle(truncated), "SystemReportCheckError.Invalid")?
}

test test_system_report_dmi_identity_capture_replays_production_collector {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"sys/class/dmi/id", parents: true)?
  source.write(
    p"sys/class/dmi/id/sys_vendor",
    """Example Vendor
""",
  )?
  source.write(
    p"sys/class/dmi/id/product_name",
    """Example Host
""",
  )?
  source.write(
    p"sys/class/dmi/id/product_serial",
    """serial-123
""",
  )?
  report_checks.capture_dmi_identity_bundle(source, bundle, "synthetic_fixture")?
  let replay = report_checks.replay_dmi_identity_bundle(bundle)?
  assert replay.exact
  assert replay.default_redacted
}

test test_system_report_device_tree_od_reference_reads_bounded_raw_source {
  let source = fs.tempdir()?
  defer source.close()?
  let scratch = fs.tempdir()?
  defer scratch.close()?
  source.write(p"model", b"ARM\0")?
  let source_root = source.host_path()?
  let source_path = fp"${source_root}/model"
  let observed = report_checks.read_device_tree_raw_reference(scratch, source_path, 4, "model-before")?
  assert observed.state == "observed"
  assert observed.data == b"ARM\0"
  assert observed.ended >= observed.started
  source.write(p"model", b"ARM\0X")?
  test.error_kind(
    report_checks.read_device_tree_raw_reference(scratch, source_path, 4, "model-oversize"),
    "SystemReportCheckError.Invalid",
  )?
  source.remove(p"model")?
  let absent = report_checks.read_device_tree_raw_reference(scratch, source_path, 4, "model-absent")?
  assert absent.state == "absent"
  assert absent.data == null
}

test test_system_report_live_reference_rejects_synthetic_candidate_mode {
  report_checks.require_live_linux_report("""{"source_mode":"live_linux"}""")?
  test.error_kind(
    report_checks.require_live_linux_report("""{"source_mode":"synthetic_fixture"}"""),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(report_checks.require_live_linux_report("""{"source_mode":null}"""), "SystemReportCheckError.Invalid")?
}

test test_system_report_uptime_reference_requires_a_bracketed_integer_second {
  assert report_checks.parse_reference_uptime_seconds("""123.456 987.654
""")? == 123
  assert report_checks.parse_reference_uptime_seconds("""123.456	987.654
""")? == 123
  assert report_checks.parse_reference_uptime_seconds("""123.999999999999999999999999 999999999999999999999999.00
""")? == 123
  assert report_checks.parse_reference_uptime_seconds("""9007199254740991.99 0.00
""")? == 9007199254740991
  test.error_kind(report_checks.parse_reference_uptime_seconds("12x.5 4.0"), "SystemReportCheckError.Invalid")?
  test.error_kind(report_checks.parse_reference_uptime_seconds("123.456 invalid"), "SystemReportCheckError.Invalid")?
  test.error_kind(
    report_checks.parse_reference_uptime_seconds("123.456 987.654 extra"),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_uptime_seconds("9007199254740992.00 0.00"),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(
    report_checks.parse_reference_uptime_seconds("999999999999999999999999999999.00 0.00"),
    "SystemReportCheckError.Invalid",
  )?
  test.error_kind(report_checks.parse_reference_uptime_seconds(""), "SystemReportCheckError.Invalid")?
  let candidate = """{"identity":{"uptime_seconds":124}}"""
  let bracketed = report_checks.compare_uptime(candidate, 123, 125)?
  assert bracketed.bracketed
  assert ! bracketed.candidate_missing
  let missing = report_checks.compare_uptime("""{"identity":{"uptime_seconds":null}}""", 123, 125)?
  assert missing.candidate_missing
  assert ! missing.bracketed
  let outside = report_checks.compare_uptime("""{"identity":{"uptime_seconds":126}}""", 123, 125)?
  assert ! outside.bracketed
  test.error_kind(report_checks.compare_uptime(candidate, 125, 123), "SystemReportCheckError.Invalid")?
}

test test_system_report_uptime_capture_validates_saved_source_and_oracle {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc", parents: true)?
  source.write(
    p"proc/uptime",
    """73.50 12.00
""",
  )?
  report_checks.capture_uptime_bundle(source, bundle, "synthetic_fixture")?
  assert report_checks.validate_uptime_bundle(bundle)? == 73
  source.write(
    p"proc/uptime",
    """74.00 12.50
""",
  )?
  assert report_checks.validate_uptime_bundle(bundle)? == 73
  let metadata = bundle.read_text(p"capture.json")?
  let changed_oracle = json.set(json.decode(metadata)?, ["reference_seconds"], 74)?
  bundle.write_atomic(p"capture.json", json.encode(changed_oracle)?)?
  test.error_kind(report_checks.validate_uptime_bundle(bundle), "SystemReportCheckError.Invalid")?
  let contradictory = json.set(json.decode(metadata)?, ["errno"], 13)?
  bundle.write_atomic(p"capture.json", json.encode(contradictory)?)?
  test.error_kind(report_checks.validate_uptime_bundle(bundle), "SystemReportCheckError.Invalid")?
  bundle.write_atomic(p"capture.json", metadata)?
  bundle.write(
    p"proc/uptime",
    """74.00 12.50
""",
  )?
  test.error_kind(report_checks.validate_uptime_bundle(bundle), "SystemReportCheckError.Invalid")?
}

test test_system_report_uptime_capture_keeps_absent_malformed_and_truncated_unscoreable {
  let source = fs.tempdir()?
  defer source.close()?
  let absent = fs.tempdir()?
  defer absent.close()?
  report_checks.capture_uptime_bundle(source, absent, "synthetic_fixture")?
  test.error_kind(report_checks.validate_uptime_bundle(absent), "SystemReportCheckError.Invalid")?
  source.mkdir(p"proc", parents: true)?
  source.write(
    p"proc/uptime",
    """73.50 12.00 extra
""",
  )?
  let malformed = fs.tempdir()?
  defer malformed.close()?
  report_checks.capture_uptime_bundle(source, malformed, "synthetic_fixture")?
  test.error_kind(report_checks.validate_uptime_bundle(malformed), "SystemReportCheckError.Invalid")?
  var padding = "x"
  while padding.count_chars() <= 4096 {
    padding = f"${padding}${padding}"
  }

  source.write(
    p"proc/uptime",
    f"""73.50 12.00
${padding}""",
  )?
  let truncated = fs.tempdir()?
  defer truncated.close()?
  report_checks.capture_uptime_bundle(source, truncated, "synthetic_fixture")?
  test.error_kind(report_checks.validate_uptime_bundle(truncated), "SystemReportCheckError.Invalid")?
}

test test_system_report_uptime_capture_replays_production_identity {
  let source = fs.tempdir()?
  defer source.close()?
  let bundle = fs.tempdir()?
  defer bundle.close()?
  source.mkdir(p"proc", parents: true)?
  source.write(
    p"proc/uptime",
    """73.50 12.00
""",
  )?
  report_checks.capture_uptime_bundle(source, bundle, "synthetic_fixture")?
  assert report_checks.replay_uptime_bundle(bundle)?.bracketed
}

test test_system_report_namespace_reference_requires_observed_exact_targets {
  let reference = [
    {
      field: "mount_namespace",
      target: "mnt:[10]",
    },
    {
      field: "time_namespace",
      target: "time:[20]",
    },
  ]
  let matching = """{"scope":{"mount_namespace":{"state":"observed","value":"mnt:[10]"},"time_namespace":{"state":"observed","value":"time:[20]"}}}"""
  let exact = report_checks.compare_namespace_scope(matching, reference)?
  assert exact.exact
  assert exact.matched_count == 2

  let incomplete = """{"scope":{"mount_namespace":{"state":"redacted","value":null},"time_namespace":{"state":"observed","value":"time:[21]"}}}"""
  let compared = report_checks.compare_namespace_scope(incomplete, reference)?
  assert compared.missing_fields == ["mount_namespace"]
  assert compared.mismatched_fields == ["time_namespace"]
  assert ! compared.exact
  let duplicate = [reference[0], reference[0]]
  test.error_kind(report_checks.compare_namespace_scope(matching, duplicate), "SystemReportCheckError.Invalid")?
}
