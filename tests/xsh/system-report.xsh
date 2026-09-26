use core.lib.system_report as report_model

type CpuFreqPolicy = {
  name: Str,
  related_cpus: List[Int],
  affected_cpus: List[Int],
  driver: Str?,
  governor: Str?,
  available_governors: List[Str],
  hardware_min_khz: Int?,
  hardware_max_khz: Int?,
  scaling_min_khz: Int?,
  scaling_max_khz: Int?,
  hardware_current_khz: Int?,
  requested_current_khz: Int?,
  governor_requested_khz: Int?,
  average_current_khz: Int?,
  bios_limit_khz: Int?,
  transition_latency_ns: Int?,
  available_frequencies_khz: List[Int],
  energy_performance_preference: Str?,
  available_energy_performance_preferences: List[Str],
  boost_supported: Bool?,
  boost_allowed: Bool?,
  boost_active: Bool?,
  boost_scope: Str?,
}

type PciFunction = {
  address: Str?,
  domain: Int?,
  bus: Int?,
  device: Int?,
  function: Int?,
  vendor_id: Int?,
  device_id: Int?,
  subsystem_vendor_id: Int?,
  subsystem_device_id: Int?,
  class_code: Int?,
  revision: Int?,
  driver: Str?,
  parent_function_index: Int?,
  numa_node: Int?,
  iommu_group: Str?,
  current_link_speed: Str?,
  current_link_width: Int?,
  maximum_link_speed: Str?,
  maximum_link_width: Int?,
}

type SystemReportModel = module {
  export pure frequency_policies_for_cpu(policies: List[CpuFreqPolicy], cpu_id: Int) -> List[CpuFreqPolicy]
  export pure pci_parent_function(functions: List[PciFunction], child: PciFunction) -> PciFunction?
  export pure parse_cpu_list(text: Str) -> Result[List[Int]]
  export pure select_report_section(report: Record, selected: Str) -> Result[Record]
  export pure encode_report_json(report: Record, sensitive: Bool, pretty: Bool) -> Result[Str]
  export pure decode_report_json(text: Str) -> Result[Record]
  export pure render_text(report: Record, full: Bool, sensitive: Bool) -> Result[Str]
}

type PciAddress = {domain: Int, bus: Int, device: Int, function: Int}
type UsbDescriptorRecord = {offset: Int, length: Int, descriptor_type: Int, raw: Bytes}
type SourceRead = {observation: report_model.TextObservation, errno: Int?, error_kind: Str?}
type PciCollection = {status: report_model.SectionStatus, functions: List[report_model.PciFunction], issues: List[report_model.CollectionIssue]}
type NetworkCollection = {status: report_model.SectionStatus, links: List[report_model.NetworkLink], routes: List[report_model.NetworkRoute], rules: List[report_model.NetworkRule], issues: List[report_model.CollectionIssue]}
type SmbiosParseResult = {records: List[report_model.FirmwareRecord], issues: List[Str], truncated: Bool}

type SystemReportCollectors = module {
  export pure parse_pci_address(value: Str) -> Result[PciAddress]
  export pure parse_pci_hex_value(value: Str) -> Result[Int]
  export pure parse_usb_descriptor_stream(data: Bytes) -> Result[List[UsbDescriptorRecord]]
  export proc read_source_text(root: FsRoot, path: Path, max_bytes: Int = 65536) [fs, error] -> SourceRead
  export proc collect_pci(root: FsRoot) [fs, error] -> PciCollection
}

type SystemReportLiveCollector = module {
  export proc collect_from_root(root: FsRoot, architecture: Str, page_size_bytes: Int, clock_ticks_per_second: Int, selected: Str = "", sensitive: Bool = false, include_local_mount_usage: Bool = false) [fs, time, error] -> report_model.SystemReport
  export proc collect_live(selected: Str = "", sensitive: Bool = false) [env, fs, time, error] -> report_model.SystemReport
  export pure assemble_network_dump(value: LinuxNetworkDump) -> NetworkCollection
  export pure parse_smbios_table(data: Bytes) -> Result[SmbiosParseResult]
}

pure cpu_policy(name: Str, related_cpus: List[Int], affected_cpus: List[Int]) -> CpuFreqPolicy {
  return {
    name: name,
    related_cpus: related_cpus,
    affected_cpus: affected_cpus,
    driver: "intel_pstate",
    governor: "powersave",
    available_governors: ["powersave", "performance"],
    hardware_min_khz: 800000,
    hardware_max_khz: 4200000,
    scaling_min_khz: 800000,
    scaling_max_khz: 4200000,
    hardware_current_khz: null,
    requested_current_khz: 1800000,
    governor_requested_khz: null,
    average_current_khz: null,
    bios_limit_khz: null,
    transition_latency_ns: null,
    available_frequencies_khz: [],
    energy_performance_preference: null,
    available_energy_performance_preferences: [],
    boost_supported: true,
    boost_allowed: true,
    boost_active: true,
    boost_scope: "system",
  }
}

pure pci_function(address: Str, parent_function_index: Int?) -> PciFunction {
  return {
    address: address,
    domain: 0,
    bus: 0,
    device: 0,
    function: 0,
    vendor_id: 32902,
    device_id: 4660,
    subsystem_vendor_id: null,
    subsystem_device_id: null,
    class_code: 393216,
    revision: 1,
    driver: "pcieport",
    parent_function_index: parent_function_index,
    numa_node: 0,
    iommu_group: null,
    current_link_speed: null,
    current_link_width: null,
    maximum_link_speed: null,
    maximum_link_width: null,
  }
}

pure json_observation(state: Str, value: Str?) -> Record {
  return {state: state, value: value, raw_bytes_base64: null}
}

pure json_section(state: Str) -> Record {
  return {state: state, enumeration_succeeded: true}
}

pure json_report_fixture() -> Record {
  return {
    schema_version: 1,
    producer: {name: "system-report", version: "test"},
    source_mode: "synthetic_fixture",
    collection_started_unix_ms: 10,
    collection_ended_unix_ms: 20,
    elapsed_ms: 10,
    scope: {
      platform: "Linux",
      host_claim: "container",
      source_roots: ["/proc", "/sys"],
      mount_namespace: json_observation("observed", "mnt:[4026531840]"),
      network_namespace: json_observation("observed", "net:[4026531840]"),
      pid_namespace: json_observation("observed", "pid:[4026531836]"),
      cgroup_namespace: json_observation("observed", "cgroup:[4026531835]"),
      visible_cgroup: json_observation("observed", "/user.slice/user-1000.slice"),
      page_size_bytes: 4096,
      clock_ticks_per_second: 100,
      ancestors_may_be_hidden: true,
    },
    redacted: false,
    identity: {
      status: json_section("complete"),
      kernel_release: "6.12-test",
      kernel_build: null,
      architecture: "x86_64",
      os_release: {id: "linux", name: "Linux", pretty_name: "Test Linux", version: null, version_id: null},
      hostname: json_observation("observed", "workstation-name"),
      uptime_seconds: 100,
      boot_id: json_observation("observed", "10000000-0000-0000-0000-000000000000"),
      firmware: null,
    },
    cpu: {
      status: json_section("complete"),
      possible: [0, 1, 2],
      present: [0, 1, 2],
      online: [0, 1, 2],
      offline: [],
      affinity: [0, 1, 2],
      effective_cpuset: [0, 1, 2],
      global_idle_driver: null,
      global_idle_governor: null,
      cpus: [],
      caches: [],
      frequency_policies: [
        cpu_policy("policy0", [0], [0]),
        cpu_policy("policy1", [1], [1]),
        {...cpu_policy("policy2", [2], [2]), scaling_max_khz: 3600000},
      ],
      idle_states: [],
      vulnerabilities: [{
        name: "spectre_v1",
        description: json_observation("observed", "mitigation active"),
      }],
      available_idle_governors: [],
    },
    memory: {
      status: json_section("complete"),
      host: {
        total_bytes: 8192,
        free_bytes: 4096,
        available_bytes: 6144,
        buffers_bytes: 0,
        cached_bytes: 1024,
        active_bytes: null,
        inactive_bytes: null,
        dirty_bytes: 0,
        writeback_bytes: 0,
        swap_total_bytes: 0,
        swap_free_bytes: 0,
        counters: [],
      },
      swaps: [{
        name: json_observation("observed", "/dev/mapper/swap-private"),
        kind: "partition",
        size_bytes: 4096,
        used_bytes: 0,
        priority: -2,
      }],
      huge_pages: [],
      transparent_huge_pages: [],
      numa: [],
      pressure: [],
      cgroup: [{
        path: json_observation("observed", "/user.slice/user-1000.slice/private"),
        hierarchy_level: 2,
        controller: "memory",
        resource: "memory.max",
        state: "observed",
        maximum_value: 8192,
        current_value: 4096,
        unit: "bytes",
        maximum_unlimited: false,
        quota: null,
        period: null,
        effective_cpus: [0, 1],
        hidden_ancestors_possible: true,
      }],
    },
    pci: {
      status: json_section("complete"),
      functions: [
        pci_function("0000:00:1f.6", null),
        pci_function("0001:02:03.0", 0),
      ],
    },
    usb: {
      status: json_section("complete"),
      devices: [{
        sysfs_name: "1-2.3",
        parent_device_index: null,
        controller_pci_index: 1,
        port_path: "2.3",
        bus_number: 1,
        device_number: 7,
        vendor_id: 4660,
        product_id: 22136,
        device_version: "1.00",
        class_code: 0,
        subclass: 0,
        protocol: 0,
        manufacturer: json_observation("observed", "Example Vendor"),
        product: json_observation("observed", "Composite Device"),
        serial: json_observation("observed", "usb-serial-private"),
        speed_mbps: "480",
        configuration_count: 1,
        active_configuration: 1,
        power_control: "on",
        autosuspend_delay_ms: 2000,
        is_root_hub: false,
        interfaces: [{
          number: 0,
          name: "1-2.3:1.0",
          driver: "usbhid",
          active_alternate: 0,
          alternate_settings: [],
        }],
      }],
    },
    storage: {
      status: json_section("complete"),
      devices: [{
        name: "nvme0n1",
        major: 259,
        minor: 0,
        kind: "disk",
        size_bytes: 8192,
        logical_sector_bytes: 512,
        physical_sector_bytes: 4096,
        removable: false,
        rotational: false,
        read_only: false,
        model: json_observation("observed", "Example NVMe"),
        firmware: json_observation("observed", "1.2.3"),
        parent_device_index: null,
        parent_pci_function_index: 1,
        holder_indices: [],
        slave_indices: [],
        active_scheduler: "none",
        available_schedulers: ["none", "mq-deadline"],
        read_ahead_kb: 128,
        discard_granularity_bytes: 4096,
        discard_max_bytes: 1048576,
        io_counters: [],
      }],
      mounts: [{
        mount_id: 42,
        parent_id: 1,
        major: 259,
        minor: 1,
        root: json_observation("observed", "/"),
        target: json_observation("observed", "/home/private"),
        mount_options: ["rw", "relatime"],
        optional_fields: [],
        filesystem: "ext4",
        source: json_observation("observed", "/dev/nvme0n1p1"),
        super_options: ["rw"],
        block_device_index: 0,
        usage_state: "observed",
        usage_total_bytes: 8192,
        usage_used_bytes: 4096,
        usage_available_bytes: 4096,
      }],
    },
    network: {
      status: json_section("complete"),
      links: [{
        ifindex: 2,
        hardware_type: 1,
        name: json_observation("observed", "eth0"),
        kind: "ether",
        mtu: 1500,
        admin_up: true,
        operational_state: "up",
        flags: ["broadcast", "multicast", "up"],
        mac: json_observation("observed", "02:00:00:00:00:01"),
        master_ifindex: null,
        lower_ifindex: null,
        parent_pci_function_index: null,
        parent_usb_device_index: null,
        driver: "example_net",
        addresses: [{
          family: "ipv6",
          address: json_observation("observed", "2001:db8::1"),
          prefix_length: 64,
          broadcast: json_observation("absent", null),
          scope: "global",
          flags: 0,
          valid_lifetime_seconds: null,
          preferred_lifetime_seconds: null,
          attributes: [{kind: 77, data: json_observation("observed", "eA==")}],
        }],
        counters: [],
        attributes: [{kind: 88, data: json_observation("observed", "eA==")}],
      }],
      routes: [{
        family: "ipv4",
        destination: json_observation("observed", "0.0.0.0"),
        prefix_length: 0,
        source_prefix_length: 0,
        source: json_observation("absent", null),
        preferred_source: json_observation("absent", null),
        gateway: json_observation("observed", "192.0.2.1"),
        table: 100,
        metric: 5,
        route_type: "unicast",
        scope: "universe",
        protocol: "static",
        output_ifindex: 2,
        input_ifindex: null,
        flags: 0,
        nexthops: [{
          ifindex: 2,
          flags: 1,
          hops: 0,
          gateway: json_observation("observed", "192.0.2.1"),
        }],
        attributes: [{kind: 99, data: json_observation("observed", "AQI=")}],
      }],
      rules: [],
    },
    sensors: {
      status: json_section("complete"),
      channels: [{
        chip: "example_hwmon",
        channel: "temp1_input",
        label: json_observation("observed", "Package temperature"),
        kind: "temperature",
        value: 42000,
        unit: "millidegrees_celsius",
        minimum: null,
        maximum: null,
        critical: 100000,
        alarm: false,
        parent_device_class_index: null,
      }],
      thermal_zones: [],
    },
    power: {status: json_section("complete"), supplies: [], cap_zones: []},
    firmware: {
      status: json_section("complete"),
      source: "fixture",
      records: [{
        record_type: 1,
        handle: 64,
        formatted_length: 27,
        fields: [{name: "wake_up_type", value: 6, unit: "enumeration"}],
        strings: [json_observation("observed", "Example System")],
      }],
      limitation: json_observation("absent", null),
    },
    kernel: {
      status: json_section("complete"),
      command_line: json_observation("observed", "root=UUID=machine-private"),
      modules: [],
      parameters: [{name: "example_module/value", value: json_observation("observed", "private-value")}],
      sysctls: [{name: "kernel.ostype", value: json_observation("observed", "Linux")}],
    },
    processes: {
      status: json_section("complete"),
      processes: [{
        pid: 10,
        parent_pid: 1,
        uid: 1000,
        command: json_observation("observed", "worker"),
        state: "sleeping",
        start_ticks: 100,
        thread_count: 2,
        resident_bytes: 4096,
        virtual_bytes: 8192,
        cgroup: json_observation("observed", "/user.slice/private"),
        cgroup_resource_index: 0,
      }],
    },
    devices: {
      status: json_section("complete"),
      devices: [{
        class: "drm",
        name: json_observation("observed", "card0"),
        parent_device_class_index: null,
        parent_pci_function_index: null,
        parent_usb_device_index: null,
        driver: "example_gpu",
        attributes: [{name: "status", value: json_observation("observed", "connected")}],
      }],
    },
    issues: [{
      section: "identity",
      field: "hostname",
      state: "permission_denied",
      error_kind: "permission_denied",
      errno: 13,
      detail: json_observation("observed", "private-path detail"),
    }, {
      section: "pci",
      field: "functions.0000:00:1f.6.vendor_id",
      state: "permission_denied",
      error_kind: "permission_denied",
      errno: 13,
      detail: json_observation("observed", "private PCI source path"),
    }],
  }
}

proc test_system_report_model_relationships() [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let policies = [
    cpu_policy("policy0", [0, 2], [0]),
    cpu_policy("policy1", [1, 3], [1, 3]),
  ]

  let offline_member_policy = model.frequency_policies_for_cpu(policies, 2)
  test.eq(offline_member_policy.len(), 1)?
  let selected_policy = offline_member_policy[0]
  if selected_policy != null {
    test.eq(selected_policy.name, "policy0")?
  } else {
    test.fail("related CPU did not resolve to its policy")?
  }
  test.eq(model.frequency_policies_for_cpu(policies, 4).len(), 0)?

  let functions = [
    pci_function("0000:00:01.0", null),
    pci_function("0001:02:03.0", 0),
    pci_function("0002:04:05.0", 9),
  ]
  let child = functions[1]
  if child != null {
    let parent = model.pci_parent_function(functions, child)
    if parent != null {
      test.eq(parent.address, "0000:00:01.0")?
    } else {
      test.fail("indexed PCI parent did not resolve")?
    }
  } else {
    test.fail("PCI child fixture is missing")?
  }

  let root_function = functions[0]
  if root_function != null {
    test.eq(model.pci_parent_function(functions, root_function), null)?
  } else {
    test.fail("PCI root fixture is missing")?
  }

  let unresolved_child = functions[2]
  if unresolved_child != null {
    test.eq(model.pci_parent_function(functions, unresolved_child), null)?
  } else {
    test.fail("unresolved PCI fixture is missing")?
  }

}

proc test_system_report_cpu_list_parser_handles_sparse_and_large_ids() [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let sparse = model.parse_cpu_list("2-4,66,129-130")?
  test.eq(sparse, [2, 3, 4, 66, 129, 130])?

  let many = model.parse_cpu_list("0-127")?
  test.eq(many.len(), 128)?
  test.ok(65 in many)?
  test.ok(127 in many)?

  test.error_kind(model.parse_cpu_list(""), "system-report-cpu-list")?
  test.error_kind(model.parse_cpu_list("4,,8"), "system-report-cpu-list")?
  test.error_kind(model.parse_cpu_list("5-2"), "system-report-cpu-list")?
  test.error_kind(model.parse_cpu_list("1,1"), "system-report-cpu-list")?
  test.error_kind(model.parse_cpu_list("0-65536"), "system-report-cpu-list")?
}

proc test_system_report_pci_and_usb_source_parsers() [fs, error] {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?

  let address: PciAddress = collectors.parse_pci_address("0001:af:1f.7")?
  test.eq(address, {domain: 1, bus: 175, device: 31, function: 7})?
  test.eq(collectors.parse_pci_hex_value("0x10DE")?, 4318)?
  test.eq(collectors.parse_pci_hex_value("10de")?, 4318)?
  test.error_kind(collectors.parse_pci_address("0000:00:20.0"), "system-report-pci-address")?
  test.error_kind(collectors.parse_pci_address("0000:00:01.8"), "system-report-pci-address")?
  test.error_kind(collectors.parse_pci_address("00:00:01.0"), "system-report-pci-address")?
  test.error_kind(collectors.parse_pci_hex_value("0x10xz"), "system-report-pci-id")?

  let descriptors = collectors.parse_usb_descriptor_stream(b"\x03\x99\x42\x02\xfe")?
  test.eq(descriptors.len(), 2)?
  test.eq(descriptors[0].offset, 0)?
  test.eq(descriptors[0].descriptor_type, 153)?
  test.eq(descriptors[0].raw, b"\x03\x99\x42")?
  test.eq(descriptors[1].offset, 3)?
  test.eq(descriptors[1].descriptor_type, 254)?
  test.error_kind(collectors.parse_usb_descriptor_stream(b"\x01\x02"), "system-report-usb-descriptor")?
  test.error_kind(collectors.parse_usb_descriptor_stream(b"\x04\x01x"), "system-report-usb-descriptor")?
  test.error_kind(collectors.parse_usb_descriptor_stream(b"\x09"), "system-report-usb-descriptor")?
}

proc test_system_report_assembles_route_netlink_snapshot_with_unknown_values() [fs, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let dump: LinuxNetworkDump = {
    state: "partial",
    links: [{
      ifindex: 2,
      name: "eth0",
      name_bytes: b"eth0\0",
      hardware_type: 1,
      flags: 65539,
      mtu: 1500,
      address: b"\x02\x00\x00\x00\x00\x01",
      broadcast: null,
      master_ifindex: null,
      lower_ifindex: null,
      operstate: 6,
      kind: "veth",
      rx_bytes: 4096,
      tx_bytes: 2048,
      attributes: [{kind: 32769, data: b"\x02\x00"}],
    }],
    addresses: [{
      ifindex: 2,
      family: "inet",
      prefix_length: 24,
      scope: 0,
      flags: 0,
      address: null,
      local: "192.0.2.10",
      broadcast: "192.0.2.255",
      label: "eth0",
      preferred_lifetime_seconds: 300,
      valid_lifetime_seconds: 600,
      attributes: [],
    }],
    routes: [{
      family: "inet",
      destination_prefix_length: 0,
      source_prefix_length: 0,
      destination: null,
      source: null,
      gateway: "192.0.2.1",
      preferred_source: null,
      output_ifindex: 2,
      input_ifindex: null,
      table: 1000,
      priority: 55,
      route_type: 222,
      protocol: 77,
      scope: 1,
      flags: 0,
      nexthops: [],
      attributes: [{kind: 16389, data: b"\x04"}],
    }, {
      family: "inet6",
      destination_prefix_length: 64,
      source_prefix_length: 0,
      destination: "2001:db8::",
      source: null,
      gateway: null,
      preferred_source: null,
      output_ifindex: null,
      input_ifindex: null,
      table: 254,
      priority: null,
      route_type: 1,
      protocol: 3,
      scope: 0,
      flags: 4,
      nexthops: [{ifindex: 9, flags: 2, hops: 3, gateway: "2001:db8::1"}],
      attributes: [],
    }],
    rules: [{
      family: "inet",
      destination_prefix_length: 0,
      source_prefix_length: 0,
      destination: null,
      source: null,
      input_name: "eth0",
      output_name: null,
      priority: 123,
      table: 1000,
      fwmark: null,
      fwmask: null,
      action: 50,
      flags: 0,
      attributes: [],
    }],
    issues: [{
      object: "address",
      message: "address attribute has an unsupported size",
      state: "malformed",
      errno: null,
      error_kind: "malformed",
    }],
  }

  let result = collector.assemble_network_dump(dump)
  test.eq(result.status.state, report_model.Partial)?
  test.eq(result.status.enumeration_succeeded, false)?
  test.eq(result.links[0].name.value, "eth0")?
  test.eq(result.links[0].mac.value, "02:00:00:00:00:01")?
  test.ok("lower_up" in result.links[0].flags)?
  test.eq(result.links[0].counters[0].value, 4096)?
  test.eq(result.links[0].addresses[0].address.value, "192.0.2.10")?
  test.eq(result.links[0].addresses[0].family, "ipv4")?
  test.eq(result.links[0].attributes[0].data.value, "AgA=")?
  test.eq(result.routes[0].destination.value, "0.0.0.0")?
  test.eq(result.routes[0].route_type, "route_type_222")?
  test.eq(result.routes[0].protocol, "protocol_77")?
  test.eq(result.routes[0].nexthops.len(), 0)?
  test.eq(result.routes[1].family, "ipv6")?
  test.eq(result.routes[1].nexthops[0].ifindex, 9)?
  test.eq(result.routes[1].nexthops[0].flags, 2)?
  test.eq(result.routes[1].nexthops[0].hops, 3)?
  test.eq(result.routes[1].nexthops[0].gateway.value, "2001:db8::1")?
  test.eq(result.rules[0].input_ifindex, 2)?
  test.eq(result.rules[0].action, "action_50")?
  test.eq(result.issues[0].state, report_model.Malformed)?

  let denied = collector.assemble_network_dump({
    ...dump,
    state: "permission_denied",
    links: [],
    addresses: [],
    routes: [],
    rules: [],
    issues: [{
      object: "socket",
      message: "route-netlink access was denied",
      state: "permission_denied",
      errno: 13,
      error_kind: "io",
    }],
  })
  test.eq(denied.status.state, report_model.PermissionDenied)?
  test.eq(denied.status.enumeration_succeeded, false)?
  test.eq(denied.issues[0].errno, 13)?
}

proc test_system_report_section_selection_marks_excluded_domains() [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let source = json_report_fixture()
  let report = model.decode_report_json(json.encode(source)?)?
  let cpu_only = model.select_report_section(report, "cpu")?
  let cpu_only_wire = json.decode(model.encode_report_json(cpu_only, true, false)?)?
  let cpu_only_text = model.render_text(cpu_only, true, false)?

  test.eq(cpu_only.identity.hostname.value, "workstation-name")?
  test.eq(cpu_only_wire.cpu.status.state, "complete")?
  test.eq(cpu_only_wire.memory.status.state, "not_requested")?
  test.ok(!cpu_only_wire.memory.status.enumeration_succeeded)?
  test.eq(cpu_only_wire.memory.host.total_bytes, null)?
  test.eq(cpu_only_wire.pci.status.state, "not_requested")?
  test.eq(cpu_only_wire.pci.functions.len(), 0)?
  test.eq(cpu_only_wire.network.status.state, "not_requested")?
  test.eq(cpu_only.issues.len(), 1)?
  test.contains(cpu_only_text, "PCI: not requested")?
  test.ok(!cpu_only_text.contains("PCI functions:"))?

  let usb_only = model.select_report_section(report, "usb")?
  test.eq(usb_only.pci.status.state, report_model.Complete)?
  test.eq(usb_only.usb.status.state, report_model.Complete)?
  test.eq(usb_only.storage.status.state, report_model.NotRequested)?
  test.eq(usb_only.network.status.state, report_model.NotRequested)?

  let network_only = model.select_report_section(report, "network")?
  test.eq(network_only.pci.status.state, report_model.Complete)?
  test.eq(network_only.usb.status.state, report_model.Complete)?
  test.eq(network_only.network.status.state, report_model.Complete)?
  test.eq(network_only.storage.status.state, report_model.NotRequested)?

  let processes_only = model.select_report_section(report, "processes")?
  test.eq(processes_only.processes.status.state, report_model.Complete)?
  test.eq(processes_only.processes.processes[0].cgroup.value, "/user.slice/private")?
  test.eq(processes_only.processes.processes[0].cgroup_resource_index, null)?
  test.eq(processes_only.memory.status.state, report_model.NotRequested)?

  test.error_kind(model.select_report_section(report, "hardware"), "system-report-section")?
}

proc test_system_report_json_round_trip_and_redaction() [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let source = json_report_fixture()
  let encoded_source = json.encode(source)?
  let decoded = model.decode_report_json(encoded_source)?
  let decoded_wire = json.decode(model.encode_report_json(decoded, true, false)?)?

  test.eq(decoded.schema_version, 1)?
  test.eq(decoded_wire.source_mode, "synthetic_fixture")?
  test.eq(decoded.identity.hostname.value, "workstation-name")?
  test.ok(decoded.pci.status.enumeration_succeeded)?
  test.eq(decoded.pci.functions.len(), 2)?

  let safe_json = model.encode_report_json(decoded, false, false)?
  let safe = model.decode_report_json(safe_json)?
  let safe_wire = json.decode(safe_json)?
  let safe_text = model.render_text(decoded, true, false)?
  test.ok(safe.redacted)?
  test.eq(safe_wire.identity.status.state, "complete")?
  test.eq(safe_wire.identity.hostname.state, "redacted")?
  test.eq(safe.identity.hostname.value, null)?
  test.eq(safe_wire.scope.network_namespace.state, "redacted")?
  test.eq(safe_wire.scope.source_roots, ["redacted", "redacted"])?
  test.eq(safe.cpu.online, [0, 1])?
  test.eq(safe_wire.pci.functions[0].address, null)?
  test.eq(safe_wire.pci.functions[0].domain, null)?
  test.eq(safe_wire.pci.functions[0].vendor_id, 32902)?
  test.eq(safe_wire.pci.functions[1].parent_function_index, 0)?
  test.ok(!safe_json.contains("0000:00:1f.6"))?
  test.ok(!safe_text.contains("0000:00:1f.6"))?
  test.eq(safe_wire.issues[1].field, "functions.redacted.vendor_id")?
  test.ok(!safe_json.contains("private PCI source path"))?
  let vulnerability = safe.cpu.vulnerabilities[0]
  if vulnerability != null {
    test.eq(vulnerability.description.value, "mitigation active")?
  } else {
    test.fail("CPU vulnerability fixture did not round-trip")?
  }
  let swap = safe.memory.swaps[0]
  if swap != null {
    test.eq(safe_wire.memory.swaps[0].name.state, "redacted")?
  } else {
    test.fail("swap fixture did not round-trip")?
  }
  let usb_device = safe.usb.devices[0]
  if usb_device != null {
    test.eq(safe_wire.usb.devices[0].sysfs_name, null)?
    test.eq(safe_wire.usb.devices[0].port_path, null)?
    test.eq(safe_wire.usb.devices[0].bus_number, null)?
    test.eq(safe_wire.usb.devices[0].controller_pci_index, 1)?
    test.eq(safe_wire.usb.devices[0].vendor_id, 4660)?
    test.eq(safe_wire.usb.devices[0].interfaces[0].name, null)?
    test.eq(safe_wire.usb.devices[0].serial.state, "redacted")?
    test.ok(!safe_json.contains("1-2.3"))?
    test.ok(!safe_text.contains("1-2.3"))?
  } else {
    test.fail("USB fixture did not round-trip")?
  }
  let block_device = safe.storage.devices[0]
  if block_device != null {
    test.eq(safe_wire.storage.devices[0].name, null)?
    test.eq(safe_wire.storage.devices[0].major, null)?
    test.eq(block_device.parent_pci_function_index, 1)?
    test.eq(safe_wire.storage.devices[0].parent_pci_function_index, 1)?
    test.eq(safe_wire.storage.devices[0].model.state, "redacted")?
    test.ok(!safe_json.contains("nvme0n1"))?
    test.ok(!safe_text.contains("nvme0n1"))?
  } else {
    test.fail("block device fixture did not round-trip")?
  }
  let mount = safe.storage.mounts[0]
  if mount != null {
    test.eq(safe_wire.storage.mounts[0].major, null)?
    test.eq(safe_wire.storage.mounts[0].minor, null)?
    test.eq(safe_wire.storage.mounts[0].block_device_index, 0)?
    test.eq(safe_wire.storage.mounts[0].target.state, "redacted")?
  } else {
    test.fail("mount fixture did not round-trip")?
  }
  let link = safe.network.links[0]
  if link != null {
    test.eq(safe_wire.network.links[0].mac.state, "redacted")?
    test.eq(safe_wire.network.links[0].attributes[0].data.state, "redacted")?
    let address = link.addresses[0]
    if address != null {
      test.eq(safe_wire.network.links[0].addresses[0].address.state, "redacted")?
    } else {
      test.fail("network address fixture did not round-trip")?
    }
  } else {
    test.fail("network link fixture did not round-trip")?
  }
  let safe_route = safe.network.routes[0]
  if safe_route != null {
    test.eq(safe_route.output_ifindex, 2)?
    test.eq(safe_route.nexthops[0].ifindex, 2)?
    test.eq(safe_route.gateway.state, report_model.Redacted)?
    test.eq(safe_route.nexthops[0].gateway.state, report_model.Redacted)?
    test.eq(safe_wire.network.routes[0].attributes[0].data.state, "redacted")?
  } else {
    test.fail("network route fixture did not round-trip")?
  }
  let firmware_record = safe.firmware.records[0]
  if firmware_record != null {
    let firmware_string = firmware_record.strings[0]
    if firmware_string != null {
      test.eq(safe_wire.firmware.records[0].strings[0].state, "redacted")?
    } else {
      test.fail("firmware string fixture did not round-trip")?
    }
  } else {
    test.fail("firmware record fixture did not round-trip")?
  }
  let kernel_parameter = safe.kernel.parameters[0]
  if kernel_parameter != null {
    test.eq(safe_wire.kernel.parameters[0].value.state, "redacted")?
  } else {
    test.fail("kernel parameter fixture did not round-trip")?
  }
  let process = safe.processes.processes[0]
  if process != null {
    test.eq(process.command.value, "worker")?
    test.eq(safe_wire.processes.processes[0].cgroup.state, "redacted")?
  } else {
    test.fail("process fixture did not round-trip")?
  }
  let device = safe.devices.devices[0]
  if device != null {
    let attribute = device.attributes[0]
    if attribute != null {
      test.eq(safe_wire.devices.devices[0].attributes[0].value.state, "redacted")?
    } else {
      test.fail("device attribute fixture did not round-trip")?
    }
  } else {
    test.fail("device fixture did not round-trip")?
  }
  let issue = safe.issues[0]
  if issue != null {
    test.eq(safe_wire.issues[0].detail.state, "redacted")?
  } else {
    test.fail("issue fixture did not round-trip")?
  }

  let sensitive_json = model.encode_report_json(decoded, true, false)?
  let sensitive = model.decode_report_json(sensitive_json)?
  test.ok(!sensitive.redacted)?
  test.eq(sensitive.identity.hostname.value, "workstation-name")?
  test.eq(sensitive.pci.functions[0].address, "0000:00:1f.6")?
  test.eq(sensitive.usb.devices[0].sysfs_name, "1-2.3")?
  test.eq(sensitive.storage.devices[0].name, "nvme0n1")?
  test.eq(sensitive.network.links[0].attributes[0].data.value, "eA==")?
  test.eq(sensitive.network.routes[0].nexthops[0].gateway.value, "192.0.2.1")?

  let route_text = model.render_text(sensitive, true, true)?
  test.contains(route_text, "nexthop ifindex=2 flags=1 hops=0 gateway=192.0.2.1")?

  let unsupported_version = json.encode({...source, schema_version: 2})?
  test.error_kind(model.decode_report_json(unsupported_version), "system-report-json")?

  let unknown_state = json.encode({...source, source_mode: "unknown-mode"})?
  test.error_kind(model.decode_report_json(unknown_state), "system-report-json")?

  let invalid_cpu_status = {...source.cpu.status, enumeration_succeeded: false}
  let invalid_cpu = {...source.cpu, status: invalid_cpu_status}
  let invalid_section = {...source, cpu: invalid_cpu}
  test.error_kind(model.decode_report_json(json.encode(invalid_section)?), "system-report-json")?
}

proc test_system_report_text_output_escapes_untrusted_controls() [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let source = json_report_fixture()
  let hostile_scope = {
    ...source.scope,
    host_claim: "host\u{202e}name\u{001b}[31m",
  }
  let hostile_identity = {
    ...source.identity,
    kernel_release: "6.12\u{000a}attack",
    uptime_seconds: null,
  }
  let hostile_source = {...source, scope: hostile_scope, identity: hostile_identity}
  let decoded = model.decode_report_json(json.encode(hostile_source)?)?
  let rendered = model.render_text(decoded, true, false)?

  test.ok(!rendered.contains("\u{202e}"))?
  test.ok(!rendered.contains("\u{001b}"))?
  test.contains(rendered, "\\u{202e}")?
  test.contains(rendered, "6.12\\nattack")?
  test.ok(!rendered.contains("6.12\nattack"))?
  test.contains(rendered, "does not guarantee anonymity")?
  test.contains(rendered, "Uptime: unknown seconds")?
}

proc test_system_report_command_replays_saved_json_offline(ctx: TestContext) [fs, process, error] {
  let path = test.temp_path(ctx, name: "system-report-v1.json")
  path.write(json.encode(json_report_fixture())?)?

  let projected = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/system-report.xsh" -- --from $path --section cpu --json ?
  let decoded = json.decode(projected)?
  test.eq(decoded.schema_version, 1)?
  test.eq(decoded.identity.hostname.state, "redacted")?
  test.eq(decoded.cpu.status.state, "complete")?
  test.eq(decoded.memory.status.state, "not_requested")?
  test.ok(!projected.contains("workstation-name"))?
  let json_full = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/system-report.xsh" -- --from $path --section cpu --json --full ?
  test.eq(json_full, projected)?

  let sensitive = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/system-report.xsh" -- --from $path --sensitive --json ?
  test.ok(sensitive.contains("workstation-name"))?

  let overview = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/system-report.xsh" -- --from $path ?
  test.contains(overview, "XSH system report v1")?
  test.contains(overview, "2 identical policy group on CPUs 0,1")?
  test.contains(overview, "1 identical policy group on CPUs 2")?
  test.ok(!overview.contains("3 identical policy group"))?

  let version = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/system-report.xsh" -- --version ?
  test.eq(version, "system-report schema v1\n")?

  let help = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/system-report.xsh" -- --help ?
  test.contains(help, "--from FILE")?
  test.contains(help, "--section NAME")?
}

proc test_system_report_command_rejects_malformed_replay(ctx: TestContext) [fs, process, error] {
  let path = test.temp_file(ctx, name: "system-report-invalid.json", contents: b"{invalid")?
  let stderr = test.temp_path(ctx, name: "system-report-invalid.stderr")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/system-report.xsh" -- --from $path 2> $stderr
  test.ok(!status.exited_with(0))?
  test.contains(stderr.read_text()?, "invalid replay report")?

  let invalid_utf8 = test.temp_file(ctx, name: "system-report-invalid-utf8.json", contents: b"\xff")?
  let utf8_stderr = test.temp_path(ctx, name: "system-report-invalid-utf8.stderr")
  let utf8_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/system-report.xsh" -- --from $invalid_utf8 2> $utf8_stderr
  test.ok(!utf8_status.exited_with(0))?
  test.contains(utf8_stderr.read_text()?, "not valid UTF-8")?

  let unsupported_path = test.temp_path(ctx, name: "system-report-unsupported-schema.json")
  path.write(json.encode({...json_report_fixture(), schema_version: 99})?)?
  let unsupported_stderr = test.temp_path(ctx, name: "system-report-unsupported-schema.stderr")
  let unsupported_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/system-report.xsh" -- --from $path 2> $unsupported_stderr
  test.ok(!unsupported_status.exited_with(0))?
  test.contains(unsupported_stderr.read_text()?, "unsupported schema version")?

  let section_stderr = test.temp_path(ctx, name: "system-report-invalid-section.stderr")
  let section_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/system-report.xsh" -- --section hardware 2> $section_stderr
  test.ok(!section_status.exited_with(0))?
  test.ok(section_stderr.read_text()?.trim() != "")?
}

proc test_system_report_live_collection_uses_explicit_root_and_redacts_by_default() [fs, time, error] {
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"proc/sys/kernel", parents: true)?
  fs.root_mkdir(root, p"proc/sys/kernel/random", parents: true)?
  fs.root_mkdir(root, p"etc", parents: true)?
  fs.root_write(root, p"proc/sys/kernel/osrelease", "fixture-release\n")?
  fs.root_write(root, p"proc/version", "Linux fixture version 1\n")?
  fs.root_write(root, p"proc/sys/kernel/hostname", "private-fixture-host\n")?
  fs.root_write(root, p"proc/sys/kernel/random/boot_id", "private-fixture-boot-id\n")?
  fs.root_write(root, p"proc/uptime", "73.5 12.0\n")?
  fs.root_write(root, p"etc/os-release", "ID=fixture\nNAME=Fixture OS\nPRETTY_NAME=\"Fixture Operating System\"\nVERSION_ID=1\n")?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let share_safe = collector.collect_from_root(root, "fixture-arch", 65536, 250, "identity")?
  test.eq(share_safe.source_mode, report_model.SyntheticFixture)?
  test.eq(share_safe.identity.kernel_release, "fixture-release")?
  test.eq(share_safe.identity.architecture, "fixture-arch")?
  test.eq(share_safe.identity.uptime_seconds, 73)?
  test.eq(share_safe.identity.hostname.state, report_model.Redacted)?
  test.eq(share_safe.identity.hostname.value, null)?
  test.eq(share_safe.cpu.status.state, report_model.NotRequested)?
  test.eq(share_safe.redacted, true)?

  let sensitive = collector.collect_from_root(root, "fixture-arch", 65536, 250, "identity", true)?
  test.eq(sensitive.identity.hostname.value, "private-fixture-host")?
  test.eq(sensitive.identity.hostname.state, report_model.Observed)?
}

proc test_system_report_cpu_collection_keeps_sparse_models_policies_and_cpuset() [fs, time, error] {
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"proc/sys/kernel/random", parents: true)?
  fs.root_mkdir(root, p"proc/self", parents: true)?
  fs.root_mkdir(root, p"etc", parents: true)?
  fs.root_mkdir(root, p"sys/devices/system/cpu/cpu0/topology", parents: true)?
  fs.root_mkdir(root, p"sys/devices/system/cpu/cpu0/cache/index7", parents: true)?
  fs.root_mkdir(root, p"sys/devices/system/cpu/cpu2/topology", parents: true)?
  fs.root_mkdir(root, p"sys/devices/system/cpu/cpu2/cache/index7", parents: true)?
  fs.root_mkdir(root, p"sys/devices/system/cpu/cpufreq/policy3", parents: true)?
  fs.root_mkdir(root, p"sys/devices/system/cpu/cpufreq/policy9", parents: true)?
  fs.root_mkdir(root, p"sys/devices/system/cpu/vulnerabilities", parents: true)?
  fs.root_mkdir(root, p"sys/fs/cgroup/worker", parents: true)?
  fs.root_write(root, p"proc/sys/kernel/osrelease", "fixture-release\n")?
  fs.root_write(root, p"proc/version", "Linux fixture version 1\n")?
  fs.root_write(root, p"proc/sys/kernel/hostname", "fixture-host\n")?
  fs.root_write(root, p"proc/sys/kernel/random/boot_id", "fixture-boot-id\n")?
  fs.root_write(root, p"proc/uptime", "1.0 0.0\n")?
  fs.root_write(root, p"etc/os-release", "ID=fixture\n")?
  fs.root_write(root, p"proc/self/cgroup", "0::/tenant/worker\n")?
  fs.root_write(root, p"proc/self/mountinfo", "31 20 0:25 /tenant /sys/fs/cgroup rw - cgroup2 cgroup rw\n")?
  fs.root_write(root, p"sys/fs/cgroup/worker/cpuset.cpus.effective", "0,2\n")?
  fs.root_write(root, p"proc/self/status", "Cpus_allowed_list:\t0,2\n")?
  fs.root_write(root, p"proc/cpuinfo", "processor: 0\nvendor_id: GenuineIntel\nmodel name: Intel fixture\nflags: fpu sse\n\nprocessor: 2\nvendor_id: AuthenticAMD\nmodel name: AMD fixture\nFeatures: fp asimd\n")?
  fs.root_write(root, p"sys/devices/system/cpu/possible", "0-2\n")?
  fs.root_write(root, p"sys/devices/system/cpu/present", "0,2\n")?
  fs.root_write(root, p"sys/devices/system/cpu/online", "0,2\n")?
  fs.root_write(root, p"sys/devices/system/cpu/offline", "1\n")?
  fs.root_write(root, p"sys/devices/system/cpu/cpu0/topology/physical_package_id", "0\n")?
  fs.root_write(root, p"sys/devices/system/cpu/cpu0/topology/die_id", "0\n")?
  fs.root_write(root, p"sys/devices/system/cpu/cpu0/topology/core_id", "0\n")?
  fs.root_write(root, p"sys/devices/system/cpu/cpu0/topology/thread_siblings_list", "0\n")?
  fs.root_write(root, p"sys/devices/system/cpu/cpu2/topology/physical_package_id", "1\n")?
  fs.root_write(root, p"sys/devices/system/cpu/cpu2/topology/die_id", "0\n")?
  fs.root_write(root, p"sys/devices/system/cpu/cpu2/topology/core_id", "0\n")?
  fs.root_write(root, p"sys/devices/system/cpu/cpu2/topology/thread_siblings_list", "2\n")?
  for cpu_path in [p"sys/devices/system/cpu/cpu0/cache/index7", p"sys/devices/system/cpu/cpu2/cache/index7"] {
    fs.root_write(root, fp"${cpu_path}/level", "2\n")?
    fs.root_write(root, fp"${cpu_path}/type", "Unified\n")?
    fs.root_write(root, fp"${cpu_path}/size", "1M\n")?
    fs.root_write(root, fp"${cpu_path}/coherency_line_size", "64\n")?
    fs.root_write(root, fp"${cpu_path}/number_of_sets", "16384\n")?
    fs.root_write(root, fp"${cpu_path}/shared_cpu_list", "0,2\n")?
  }
  for policy in [
    {path: p"sys/devices/system/cpu/cpufreq/policy3", cpus: "0", driver: "intel_pstate", governor: "powersave"},
    {path: p"sys/devices/system/cpu/cpufreq/policy9", cpus: "2", driver: "acme_cpufreq", governor: "unlisted-governor"},
  ] {
    fs.root_write(root, fp"${policy.path}/related_cpus", f"${policy.cpus}\n")?
    fs.root_write(root, fp"${policy.path}/affected_cpus", f"${policy.cpus}\n")?
    fs.root_write(root, fp"${policy.path}/scaling_driver", f"${policy.driver}\n")?
    fs.root_write(root, fp"${policy.path}/scaling_governor", f"${policy.governor}\n")?
    fs.root_write(root, fp"${policy.path}/scaling_available_governors", "powersave performance\n")?
  }
  fs.root_write(root, p"sys/devices/system/cpu/vulnerabilities/spectre_v2", "Mitigation: fixture policy\n")?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(value.cpu.possible, [0, 1, 2])?
  test.eq(value.cpu.present, [0, 2])?
  test.eq(value.cpu.offline, [1])?
  test.eq(value.cpu.affinity, [0, 2])?
  test.eq(value.cpu.effective_cpuset, [0, 2])?
  test.eq(value.cpu.cpus[0].model, "Intel fixture")?
  test.eq(value.cpu.cpus[1].model, "AMD fixture")?
  test.eq(value.cpu.cpus[0].policy, "policy3")?
  test.eq(value.cpu.cpus[1].policy, "policy9")?
  test.eq(value.cpu.frequency_policies.len(), 2)?
  test.eq(value.cpu.frequency_policies[1].governor, "unlisted-governor")?
  test.eq(value.cpu.caches.len(), 1)?
  test.eq(value.cpu.caches[0].sysfs_index, 7)?
  test.eq(value.cpu.caches[0].level, 2)?
  test.eq(value.cpu.caches[0].shared_cpus, [0, 2])?
  test.eq(value.cpu.vulnerabilities[0].description.value, "Mitigation: fixture policy")?
}

proc test_system_report_live_collection_rejects_linux_dry_run() [env, fs, time, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  env XSH_LINUX_DRY_RUN=1 {
    test.error_kind(collector.collect_live(), "system-report-dry-run")?
  }
}

proc test_system_report_storage_parses_mountinfo_escapes_and_stacked_mounts() [fs, time, error] {
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"proc/self", parents: true)?
  fs.root_write(
    root,
    p"proc/self/mountinfo",
    "12 1 8:1 / /mnt/a\\040b rw,relatime - ext4 /dev/sda1 rw,errors=remount-ro\n13 1 8:1 / /mnt/alias rw - ext4 /dev/sda1 rw\n14 1 0:2 / /mnt/remote rw - nfs server:/data rw\n",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 65536, 250, "storage", true, true)?
  test.eq(value.storage.mounts.len(), 3)?
  let first = value.storage.mounts[0]
  if first == null {
    test.fail("first mountinfo record was dropped")?
  }
  test.eq(first.mount_id, 12)?
  test.eq(first.target.value, "/mnt/a b")?
  test.eq(first.filesystem, "ext4")?
  test.eq(first.source.value, "/dev/sda1")?
  test.eq(first.usage_state, report_model.Disappeared)?
  test.eq(value.storage.mounts[1].mount_id, 13)?
  test.eq(value.storage.mounts[1].usage_state, report_model.Disappeared)?
  test.eq(value.storage.mounts[2].mount_id, 14)?
  test.eq(value.storage.mounts[2].usage_state, report_model.NotRequested)?
  let fixture_only = collector.collect_from_root(root, "fixture-arch", 65536, 250, "storage", true)?
  test.eq(fixture_only.storage.mounts[0].usage_state, report_model.NotRequested)?
}

proc test_system_report_storage_links_block_devices_to_pci_controllers() [fs, time, error] {
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"proc/sys/kernel/random", parents: true)?
  fs.root_mkdir(root, p"proc/self", parents: true)?
  fs.root_mkdir(root, p"etc", parents: true)?
  fs.root_mkdir(root, p"sys/bus/pci/devices/0001:02:03.0", parents: true)?
  fs.root_mkdir(root, p"sys/class/block", parents: true)?
  fs.root_mkdir(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue", parents: true)?
  fs.root_mkdir(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/holders", parents: true)?
  fs.root_mkdir(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/slaves", parents: true)?
  fs.root_mkdir(root, p"sys/bus/pci/devices/0001:02:03.0/iommu_group", parents: true)?
  fs.root_write(root, p"proc/sys/kernel/osrelease", "fixture-release\n")?
  fs.root_write(root, p"proc/version", "Linux fixture version 1\n")?
  fs.root_write(root, p"proc/sys/kernel/hostname", "fixture-host\n")?
  fs.root_write(root, p"proc/sys/kernel/random/boot_id", "fixture-boot-id\n")?
  fs.root_write(root, p"proc/uptime", "1.0 0.0\n")?
  fs.root_write(root, p"etc/os-release", "ID=fixture\n")?
  fs.root_write(root, p"proc/self/mountinfo", "12 1 259:0 / /mnt/data rw - ext4 /dev/nvme0n1 rw\n")?
  fs.root_write(root, p"sys/bus/pci/devices/0001:02:03.0/vendor", "0x1234\n")?
  fs.root_write(root, p"sys/bus/pci/devices/0001:02:03.0/device", "0xabcd\n")?
  fs.root_write(root, p"sys/bus/pci/devices/0001:02:03.0/subsystem_vendor", "0x1234\n")?
  fs.root_write(root, p"sys/bus/pci/devices/0001:02:03.0/subsystem_device", "0x0001\n")?
  fs.root_write(root, p"sys/bus/pci/devices/0001:02:03.0/class", "0x010802\n")?
  fs.root_write(root, p"sys/bus/pci/devices/0001:02:03.0/revision", "0x01\n")?
  fs.root_symlink(root, p"../../devices/pci0001:02/0001:02:03.0/block/nvme0n1", p"sys/class/block/nvme0n1")?
  fs.root_symlink(root, p"../../../0001:02:03.0", p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/device")?
  fs.root_write(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/dev", "259:0\n")?
  fs.root_write(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/size", "16\n")?
  fs.root_write(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/logical_block_size", "512\n")?
  fs.root_write(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/physical_block_size", "4096\n")?
  fs.root_write(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/removable", "0\n")?
  fs.root_write(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/rotational", "0\n")?
  fs.root_write(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/ro", "0\n")?
  fs.root_write(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/device/model", "Fixture NVMe\n")?
  fs.root_write(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/device/firmware_rev", "1.0\n")?
  fs.root_write(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/scheduler", "[none] mq-deadline\n")?
  fs.root_write(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/read_ahead_kb", "128\n")?
  fs.root_write(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/discard_granularity", "4096\n")?
  fs.root_write(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/discard_max_bytes", "1048576\n")?
  fs.root_write(root, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/stat", "1 0 8 1 2 0 16 2 0 3 4\n")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(value.pci.functions.len(), 1)?
  test.eq(value.pci.functions[0].domain, 1)?
  test.eq(value.storage.devices.len(), 1)?
  test.eq(value.storage.devices[0].kind, "disk")?
  test.eq(value.storage.devices[0].size_bytes, 8192)?
  test.eq(value.storage.devices[0].parent_pci_function_index, 0)?
  test.eq(value.storage.mounts[0].block_device_index, 0)?
}

proc test_system_report_process_collection_uses_reported_page_size_and_raw_start_ticks() [fs, time, error] {
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"proc/123", parents: true)?
  fs.root_write(root, p"proc/123/stat", "123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2\n")?
  fs.root_write(root, p"proc/123/statm", "2 1 0 0 0 0 0\n")?
  fs.root_write(root, p"proc/123/status", "Name:\tworker\nUid:\t1234\t1234\t1234\t1234\n")?
  fs.root_write(root, p"proc/123/cgroup", "0::/fixture/group\n")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 65536, 250, "processes", true)?
  test.eq(value.scope.page_size_bytes, 65536)?
  test.eq(value.scope.clock_ticks_per_second, 250)?
  test.eq(value.processes.processes.len(), 1)?
  let process = value.processes.processes[0]
  if process == null {
    test.fail("fixture process was not collected")?
  }
  test.eq(process.pid, 123)?
  test.eq(process.parent_pid, 1)?
  test.eq(process.uid, 1234)?
  test.eq(process.command.value, "worker")?
  test.eq(process.start_ticks, 100)?
  test.eq(process.resident_bytes, 65536)?
  test.eq(process.virtual_bytes, 131072)?
  test.eq(process.cgroup.value, "/fixture/group")?
  test.eq(process.cgroup_resource_index, null)?
}

proc test_system_report_joins_visible_process_cgroups_to_resource_records() [fs, time, error] {
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"proc/123", parents: true)?
  fs.root_mkdir(root, p"proc/self", parents: true)?
  fs.root_mkdir(root, p"sys/fs/cgroup/fixture/group", parents: true)?
  fs.root_write(root, p"proc/meminfo", "MemTotal: 16 kB\nMemFree: 4 kB\nMemAvailable: 8 kB\nVendorCounter: 12 widgets\n")?
  fs.root_write(root, p"proc/swaps", "Filename Type Size Used Priority\n")?
  fs.root_write(root, p"proc/self/cgroup", "0::/fixture/group\n")?
  fs.root_write(root, p"proc/self/mountinfo", "31 20 0:25 / /sys/fs/cgroup rw,nosuid,nodev - cgroup2 cgroup rw\n")?
  fs.root_write(root, p"proc/123/stat", "123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2\n")?
  fs.root_write(root, p"proc/123/statm", "2 1 0 0 0 0 0\n")?
  fs.root_write(root, p"proc/123/status", "Uid:\t1234\t1234\t1234\t1234\n")?
  fs.root_write(root, p"proc/123/cgroup", "0::/fixture/group\n")?
  fs.root_write(root, p"sys/fs/cgroup/fixture/group/memory.max", "1048576\n")?
  fs.root_write(root, p"sys/fs/cgroup/fixture/group/memory.current", "524288\n")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "", true)?
  let process = (value.processes.processes |> where .pid == 123 |> first())?
  test.eq(process.cgroup.value, "/fixture/group")?
  if process.cgroup_resource_index == null {
    test.fail("process cgroup relationship was not resolved")?
  }
  let resource = value.memory.cgroup[process.cgroup_resource_index]
  test.eq(resource.path.value, process.cgroup.value)?
}

proc test_system_report_memory_collects_cgroup_v2_limits_and_visible_ancestors() [fs, time, error] {
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"proc/self", parents: true)?
  fs.root_mkdir(root, p"proc", parents: true)?
  fs.root_mkdir(root, p"sys/fs/cgroup/a/b", parents: true)?
  fs.root_write(root, p"proc/meminfo", "MemTotal: 16 kB\nMemFree: 4 kB\nMemAvailable: 8 kB\nVendorCounter: 12 widgets\n")?
  fs.root_write(root, p"proc/swaps", "Filename Type Size Used Priority\n")?
  fs.root_write(root, p"proc/self/cgroup", "0::/a/b\n")?
  fs.root_write(root, p"proc/self/mountinfo", "31 20 0:25 / /sys/fs/cgroup rw,nosuid,nodev - cgroup2 cgroup rw\n")?
  fs.root_write(root, p"sys/fs/cgroup/a/b/memory.max", "max\n")?
  fs.root_write(root, p"sys/fs/cgroup/a/b/memory.current", "1024\n")?
  fs.root_write(root, p"sys/fs/cgroup/a/b/memory.swap.max", "262144\n")?
  fs.root_write(root, p"sys/fs/cgroup/a/b/memory.swap.current", "65536\n")?
  fs.root_write(root, p"sys/fs/cgroup/a/b/cpu.max", "50000 100000\n")?
  fs.root_write(root, p"sys/fs/cgroup/a/b/cpu.stat", "usage_usec 9000\nuser_usec 7000\nsystem_usec 2000\nnr_periods 12\nnr_throttled 2\nthrottled_usec 450\n")?
  fs.root_write(root, p"sys/fs/cgroup/a/b/cpuset.cpus.effective", "0-1\n")?
  fs.root_write(root, p"sys/fs/cgroup/a/b/pids.max", "max\n")?
  fs.root_write(root, p"sys/fs/cgroup/a/b/pids.current", "8\n")?
  fs.root_write(root, p"sys/fs/cgroup/a/b/io.stat", "8:0 rbytes=4096 wbytes=2048 rios=2 wios=1\n")?
  fs.root_write(root, p"sys/fs/cgroup/a/memory.max", "8192\n")?
  fs.root_write(root, p"sys/fs/cgroup/a/memory.current", "2048\n")?
  fs.root_write(root, p"sys/fs/cgroup/a/cpu.max", "max 100000\n")?
  fs.root_write(root, p"sys/fs/cgroup/a/pids.max", "100\n")?
  fs.root_write(root, p"sys/fs/cgroup/a/pids.current", "12\n")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 65536, 250, "memory", true)?
  let memory_limits = value.memory.cgroup |> where .controller == "memory" and .resource == "memory.max"
  test.eq(memory_limits.len(), 2)?
  let current_limit = memory_limits[0]
  if current_limit == null {
    test.fail("current cgroup memory limit was not collected")?
  }
  test.eq(current_limit.hierarchy_level, 0)?
  test.eq(current_limit.maximum_unlimited, true)?
  test.eq(current_limit.current_value, 1024)?
  test.eq(current_limit.unit, "bytes")?
  let cpu_limit = (value.memory.cgroup |> where .resource == "cpu.max" |> first())?
  test.eq(cpu_limit.quota, 50000)?
  test.eq(cpu_limit.period, 100000)?
  let swap_limit = (value.memory.cgroup |> where .resource == "memory.swap.max" |> first())?
  test.eq(swap_limit.maximum_value, 262144)?
  test.eq(swap_limit.current_value, 65536)?
  let cpu_usage = (value.memory.cgroup |> where .resource == "cpu.stat.usage_usec" |> first())?
  test.eq(cpu_usage.current_value, 9000)?
  test.eq(cpu_usage.unit, "microseconds")?
  let cpuset = (value.memory.cgroup |> where .resource == "cpuset.cpus.effective" |> first())?
  test.eq(cpuset.effective_cpus, [0, 1])?
  let io_bytes = (value.memory.cgroup |> where .resource == "io.stat.8:0.rbytes" |> first())?
  test.eq(io_bytes.current_value, 4096)?
  test.eq(io_bytes.unit, "bytes")?
  test.ok(value.memory.host.counters |> any .name == "VendorCounter" and .value == 12 and .unit == "widgets")?

  let invalid_root = fs.tempdir()?
  defer fs.close_root(invalid_root)?
  fs.root_mkdir(invalid_root, p"proc/self", parents: true)?
  fs.root_write(invalid_root, p"proc/meminfo", "MemTotal: 16 MB\n")?
  fs.root_write(invalid_root, p"proc/swaps", "Filename Type Size Used Priority\n")?
  let invalid = collector.collect_from_root(invalid_root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(invalid.memory.host.total_bytes, null)?
  test.ok(invalid.issues |> any .field == "meminfo.MemTotal" and .state == report_model.Malformed)?
}

proc test_system_report_collects_swap_limit_without_memory_limit_files() [fs, time, error] {
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"proc/self", parents: true)?
  fs.root_mkdir(root, p"sys/fs/cgroup/group", parents: true)?
  fs.root_write(root, p"proc/meminfo", "MemTotal: 16 kB\n")?
  fs.root_write(root, p"proc/swaps", "Filename Type Size Used Priority\n")?
  fs.root_write(root, p"proc/self/cgroup", "0::/group\n")?
  fs.root_write(root, p"proc/self/mountinfo", "31 20 0:25 / /sys/fs/cgroup rw,nosuid,nodev - cgroup2 cgroup rw\n")?
  fs.root_write(root, p"sys/fs/cgroup/group/memory.swap.max", "262144\n")?
  fs.root_write(root, p"sys/fs/cgroup/group/memory.swap.current", "65536\n")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let swap_limit = value.memory.cgroup |> where .resource == "memory.swap.max" |> first()
  if swap_limit == null {
    test.fail("cgroup swap accounting disappeared when memory limits were absent")?
  }
  test.eq(swap_limit.maximum_value, 262144)?
  test.eq(swap_limit.current_value, 65536)?
}

proc test_system_report_smbios_parser_preserves_records_and_reports_bad_string_indexes() [fs, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let table = b"\x01\x08\x34\x12\x01\x02\x03\x00Vendor\x00Model\x00Version\x00\x00\x7f\x04\x00\x00\x00\x00"
  let parsed = collector.parse_smbios_table(table)?
  test.eq(parsed.truncated, false)?
  test.eq(parsed.issues, [])?
  test.eq(parsed.records.len(), 2)?
  test.eq(parsed.records[0].record_type, 1)?
  test.eq(parsed.records[0].handle, 4660)?
  test.eq(parsed.records[0].strings.len(), 3)?
  test.eq(parsed.records[0].strings[1].value, "Model")?
  test.eq(parsed.records[1].record_type, 127)?

  let bad_index = b"\x01\x08\x34\x12\x04\x02\x03\x00Vendor\x00Model\x00Version\x00\x00\x7f\x04\x00\x00\x00\x00"
  let partial = collector.parse_smbios_table(bad_index)?
  test.eq(partial.records.len(), 2)?
  test.ok(partial.issues.len() > 0)?

  let invalid_length = collector.parse_smbios_table(b"\x01\x03\x00\x00")?
  test.eq(invalid_length.records.len(), 0)?
  test.eq(invalid_length.truncated, false)?
  test.eq(invalid_length.issues.len(), 1)?

  let root = fs.tempdir()?
  defer fs.close_root(root)?
  fs.root_mkdir(root, p"sys/firmware/dmi/tables", parents: true)?
  fs.root_write(root, p"sys/firmware/dmi/tables/DMI", bad_index)?
  let collected = collector.collect_from_root(root, "fixture-arch", 4096, 100, "firmware", true)?
  test.eq(collected.firmware.status.state, report_model.Partial)?
  let parser_issue = collected.issues |> where .field == "smbios.issue.0" |> first()
  if parser_issue == null {
    test.fail("SMBIOS parse issue was not retained")?
  }
  test.ok(parser_issue.detail.value != null)?
}
