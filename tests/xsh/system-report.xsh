use core.lib.system_report as report_model

type FixtureCpuFreqPolicy = {
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
  scaling_current_khz: Int?,
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

type FixturePciFunction = {
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
  export pure frequency_policies_for_cpu(policies: List[FixtureCpuFreqPolicy], cpu_id: Int) -> List[FixtureCpuFreqPolicy]
  export pure pci_parent_function(functions: List[FixturePciFunction], child: FixturePciFunction) -> FixturePciFunction?
  export pure parse_cpu_list(text: Str) -> Result[List[Int]]
  export pure select_report_section(report: Record, selected: Str) -> Result[Record]
  export pure encode_report_json(report: Record, sensitive: Bool, pretty: Bool) -> Result[Str]
  export pure decode_report_json(text: Str) -> Result[Record]
  export pure render_text(report: Record, full: Bool, sensitive: Bool) -> Result[Str]
}

type PciAddress = {domain: Int, bus: Int, device: Int, function: Int}

type UsbDescriptorRecord = {offset: Int, length: Int, descriptor_type: Int, raw: Bytes}

type SourceRead = {observation: report_model.TextObservation, errno: Int?, error_kind: Str?}

type BoundedNumericObservation = {value: Int?, state: report_model.ObservationState?, error_kind: Str?, errno: Int?}

type TransparentHugePagePolicy = {selected: Str, available: List[Str]}

type PciCollection = {
  status: report_model.SectionStatus,
  functions: List[report_model.PciFunction],
  issues: List[report_model.CollectionIssue],
}

type NetworkCollection = {
  status: report_model.SectionStatus,
  links: List[report_model.NetworkLink],
  routes: List[report_model.NetworkRoute],
  rules: List[report_model.NetworkRule],
  issues: List[report_model.CollectionIssue],
}

type UsbControllerObservation = {address: Str?, state: report_model.ObservationState, errno: Int?, error_kind: Str?}

type ClassParentObservation = {target: Path?, state: report_model.ObservationState, errno: Int?, error_kind: Str?}

type SmbiosParseResult = {records: List[report_model.FirmwareRecord], issues: List[Str], truncated: Bool}

type ProcStatFieldIssue = {field: Str, state: report_model.ObservationState}

type ProcStat = {
  pid: Int,
  parent_pid: Int,
  command: Str,
  state: Str,
  thread_count: Int?,
  start_ticks: Int,
  virtual_bytes: Int?,
  resident_pages: Int?,
  field_issues: List[ProcStatFieldIssue],
}

type UsbDescriptorAlternateFixture = {
  configuration_value: Int?,
  interface_number: Int,
  setting_number: Int,
  class_code: Int,
  subclass: Int,
  protocol: Int,
  endpoints: List[report_model.UsbEndpoint],
}

type SystemReportCollectors = module {
  export pure parse_pci_address(value: Str) -> Result[PciAddress]
  export pure parse_pci_hex_value(value: Str) -> Result[Int]
  export pure pci_parent_address(target: Path, child_address: Str) -> Str?
  export pure parse_usb_descriptor_stream(data: Bytes) -> Result[List[UsbDescriptorRecord]]
  export proc read_source_text(root: FsRoot, path: Path, max_bytes: Int = 65536, preserve_whitespace: Bool = false) [fs, error] -> SourceRead
  export pure bounded_number(source: SourceRead, nonnegative: Bool) -> BoundedNumericObservation
  export pure parse_uptime_seconds(source: SourceRead) -> BoundedNumericObservation
  export pure bounded_size_bytes(source: SourceRead) -> BoundedNumericObservation
  export pure valid_psi_average(value: Str) -> Bool
  export pure parse_thp_policy(value: Str) -> Result[TransparentHugePagePolicy]
  export pure decode_os_release_value(raw: Str) -> Str?
  export pure valid_os_release_key(key: Str) -> Bool
  export pure valid_os_release_id(value: Str) -> Bool
  export pure parse_block_scheduler(value: Str) -> BlockScheduler?
  export pure decode_device_tree_strings(raw: Str) -> List[Str]?
  export proc collect_pci(root: FsRoot) [fs, error] -> PciCollection
}

type BlockScheduler = {active: Str, available: List[Str]}

type SystemReportLiveCollector = module {
  export proc parse_usb_alternates(data: Bytes) [error] -> Result[List[UsbDescriptorAlternateFixture]]
  export proc collect_from_root(root: FsRoot, architecture: Str, page_size_bytes: Int, clock_ticks_per_second: Int, selected: Str = "", sensitive: Bool = false, include_local_mount_usage: Bool = false) [fs, time, error] -> Result[report_model.SystemReport]
  export proc collect_live(selected: Str = "", sensitive: Bool = false) [fs, process, env, time, error] -> Result[report_model.SystemReport]
  export pure assemble_network_dump(value: LinuxNetworkDump) -> NetworkCollection
  export proc link_network_device_sources(root: FsRoot, assembled: NetworkCollection, pci_functions: List[report_model.PciFunction], usb_devices: List[report_model.UsbDevice]) [fs, error] -> NetworkCollection
  export proc optional_driver_name(root: FsRoot, source_path: Path) [fs, error] -> SourceRead
  export proc usb_controller_address(root: FsRoot, device_path: Path) [fs, error] -> UsbControllerObservation
  export proc class_parent_target(root: FsRoot, entry: Path) [fs, error] -> ClassParentObservation
  export pure parse_smbios_table(data: Bytes) -> Result[SmbiosParseResult]
  export pure parse_proc_stat(text: Str) -> Result[ProcStat]
  export pure usb_parent_address(target: Path) -> Str?
  export pure pci_address_in_target(target: Path) -> Str?
  export pure link_usb_parents(devices: List[report_model.UsbDevice]) -> List[report_model.UsbDevice]
}

test test_system_report_checker_keeps_cpu_policy_members_typed [error] { |ctx|
  let output = test.run_script(
    ctx,
    r"""use core.lib.system_report as model
pure cpu_policy_members(policy: model.CpuFreqPolicy) -> Str {
  return policy.related_cpus
}
""",
    [],
    {XSH_MODULE_PATH: ctx.core_dir.parent().display()},
  )?
  test.eq(output.status, 2)?
  test.contains(output.stderr, "expected Str, found List[Int]")?
}

test test_system_report_checker_rejects_live_collection_in_pure_code [error] { |ctx|
  let output = test.run_script(
    ctx,
    r"""use core.lib.system_report_live as collector
pure forbidden_live_collection() -> Result[Unit] {
  let _ = collector.collect_live()?
  return Ok()
}
""",
    [],
    {XSH_MODULE_PATH: ctx.core_dir.parent().display()},
  )?
  test.eq(output.status, 2)?
  test.contains(output.stderr, "effectful proc is not allowed in pure functions")?
}

test test_system_report_class_parent_retains_independent_fallback_and_link_failure [fs, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/drm/card0", parents: true)?

  let directory = collector.class_parent_target(root, p"sys/class/drm/card0")
  test.ok(directory.state == report_model.Observed)?
  test.eq(directory.target, null)?

  root.symlink(../../devices/pci0000:00/0000:03:00.0/drm/card1, p"sys/class/drm/card1")?
  let fallback = collector.class_parent_target(root, p"sys/class/drm/card1")
  test.ok(fallback.state == report_model.Observed)?
  test.eq(collector.usb_parent_address(fallback.target.require(Path)?), "0000:03:00.0")?

  root.write(p"sys/class/drm/card0/device", "not a symlink")?
  let failed = collector.class_parent_target(root, p"sys/class/drm/card0")
  test.ok(failed.state == report_model.ReadFailure)?
  test.ok(failed.errno != null)?

  let disappeared = collector.class_parent_target(root, p"sys/class/drm/card2")
  test.ok(disappeared.state == report_model.Disappeared)?
}

test test_system_report_device_classes_reject_truncated_names_and_attributes [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/input/input0", parents: true)?
  root.mkdir(p"sys/class/sound/card0", parents: true)?
  root.mkdir(p"sys/class/drm/card0", parents: true)?
  var padding = " "
  while padding.count_chars() < 16384 {
    padding = f"${padding}${padding}"
  }

  root.write(
    p"sys/class/input/input0/name",
    f"""private keyboard
${padding}""",
  )?
  root.write(
    p"sys/class/sound/card0/id",
    f"""private card
${padding}""",
  )?
  root.write(
    p"sys/class/drm/card0/status",
    f"""connected
${padding}""",
  )?
  root.write(
    p"sys/class/drm/card0/enabled",
    """enabled
""",
  )?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "devices", true)?
  test.eq(value.devices.devices.len(), 3)?
  let input = (value.devices.devices
    |> where .class == "input"
    |> first())?
  let sound = (value.devices.devices
    |> where .class == "sound"
    |> first())?
  let drm = (value.devices.devices
    |> where .class == "drm"
    |> first())?
  test.eq(input.name.value, "input0")?
  test.eq(sound.name.value, "card0")?
  test.eq(drm.name.value, "card0")?
  test.ok(drm.attributes |> any .name == "enabled" and .value.value == "enabled")?
  test.ok(! (drm.attributes |> any .name == "status"))?
  for field in ["input.input0.name", "sound.card0.id", "drm.card0.status"] {
    test.ok(value.issues |> any .section == "devices" and .field == field and .state == report_model.Truncated)?
  }

  test.eq(value.devices.status.state, report_model.Partial)?
}

test test_system_report_device_classes_keep_sound_and_input_without_drm [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/sound/card0", parents: true)?
  root.mkdir(p"sys/class/input/input0", parents: true)?
  root.write(
    p"sys/class/sound/card0/id",
    """fixture sound
""",
  )?
  root.write(
    p"sys/class/sound/card0/number",
    """0
""",
  )?
  root.write(
    p"sys/class/input/input0/name",
    """fixture keyboard
""",
  )?
  root.symlink(../../../devices/virtual/sound/card0, p"sys/class/sound/card0/device")?
  root.symlink(../../../devices/virtual/input/input0, p"sys/class/input/input0/device")?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "devices", true)?
  test.eq(value.devices.status.state, report_model.Complete)?
  test.ok(value.devices.status.enumeration_succeeded)?
  test.eq(value.devices.devices.len(), 2)?
  let sound = (value.devices.devices
    |> where .class == "sound"
    |> first())?
  let input = (value.devices.devices
    |> where .class == "input"
    |> first())?
  test.eq(sound.name.value, "fixture sound")?
  test.eq(input.name.value, "fixture keyboard")?
  test.ok(sound.parent_pci_function_index == null)?
  test.ok(input.parent_usb_device_index == null)?
  test.ok(! (value.devices.devices |> any .class == "drm"))?
}

test test_system_report_device_class_entry_identity_survives_duplicate_labels_and_replay [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  for entry in ["card0", "card1"] {
    root.mkdir(fp"sys/class/sound/${entry}", parents: true)?
    root.write(
      fp"sys/class/sound/${entry}/id",
      """Shared card label
""",
    )?
  }

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "devices", true)?
  let sound = value.devices.devices |> where .class == "sound"
  test.eq(sound.len(), 2)?
  test.eq(sound[0].name.value, sound[1].name.value)?
  test.ok(sound |> any .entry_name.value == "card0")?
  test.ok(sound |> any .entry_name.value == "card1")?
  let sensitive = model.encode_report_json(value, true, false)?
  test.eq(json.decode(sensitive)?.devices.devices[0].entry_name.value, sound[0].entry_name.value)?
  test.eq(json.decode(sensitive)?.devices.devices[1].entry_name.value, sound[1].entry_name.value)?
  let redacted = model.encode_report_json(value, false, false)?
  test.eq(json.decode(redacted)?.devices.devices[0].entry_name.state, "observed")?
  let legacy = json.remove(json.decode(sensitive)?, ["devices", "devices", 0, "entry_name"])?
  let replay = model.decode_report_json(json.encode(legacy)?)?
  test.ok(replay.devices.devices[0].entry_name.state == report_model.Unsupported)?
  let repeated = json.set(
    json.decode(sensitive)?,
    ["devices", "devices", 1, "entry_name"],
    json.decode(sensitive)?.devices.devices[0].entry_name,
  )?
  test.error_kind(model.decode_report_json(json.encode(repeated)?), "SystemReportError.InvalidJson")?
}

test test_system_report_usb_controller_link_distinguishes_directories_disappearance_and_failures [fs, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/bus/usb/devices/1-2", parents: true)?

  let directory = collector.usb_controller_address(root, p"sys/bus/usb/devices/1-2")
  test.ok(directory.state == report_model.Observed)?
  test.eq(directory.address, null)?

  root.symlink(../../../devices/pci0000:00/0000:04:00.4/usb4/4-2, p"sys/bus/usb/devices/4-2")?
  let linked = collector.usb_controller_address(root, p"sys/bus/usb/devices/4-2")
  test.ok(linked.state == report_model.Observed)?
  test.eq(linked.address, "0000:04:00.4")?

  let disappeared = collector.usb_controller_address(root, p"sys/bus/usb/devices/4-3")
  test.ok(disappeared.state == report_model.Disappeared)?

  root.write(p"sys/bus/usb/devices/4-4", "not a directory")?
  let failed = collector.usb_controller_address(root, p"sys/bus/usb/devices/4-4")
  test.ok(failed.state == report_model.ReadFailure)?
  test.ok(failed.errno != null)?
}

test test_system_report_driver_link_distinguishes_unbound_and_unreadable_devices [fs, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/example", parents: true)?

  let unbound = collector.optional_driver_name(root, p"sys/devices/example/driver")
  test.ok(unbound.observation.state == report_model.Absent)?
  test.eq(unbound.observation.value, null)?

  root.symlink(p"example-driver", p"sys/devices/example/driver")?
  let bound = collector.optional_driver_name(root, p"sys/devices/example/driver")
  test.ok(bound.observation.state == report_model.Observed)?
  test.eq(bound.observation.value, "example-driver")?
  root.remove(p"sys/devices/example/driver")?

  root.write(p"sys/devices/example/driver", "not a symlink")?
  let failed = collector.optional_driver_name(root, p"sys/devices/example/driver")
  test.ok(failed.observation.state == report_model.ReadFailure)?
  test.eq(failed.observation.value, null)?
  test.ok(failed.errno != null)?
}

test test_system_report_network_device_links_keep_absence_separate_from_failures [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let snapshot = model.decode_report_json(json.encode(json_report_fixture())?)?.require(report_model.SystemReport)?
  let source: NetworkCollection = {
    status: {
      state: report_model.Complete,
      enumeration_succeeded: true,
    },
    links: [
      {
        ...snapshot.network.links[0],
        driver: null,
      },
    ],
    routes: [],
    rules: [],
    issues: [],
  }
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/net/eth0", parents: true)?

  let absent = collector.link_network_device_sources(root, source, [], [])
  test.ok(absent.status.state == report_model.Complete)?
  test.eq(absent.issues, [])?
  test.eq(absent.links[0].parent_pci_function_index, null)?

  root.symlink(../../../devices/pci0000:00/0001:02:03.0, p"sys/class/net/eth0/device")?
  let linked = collector.link_network_device_sources(root, source, snapshot.pci.functions, [])
  test.ok(linked.status.state == report_model.Complete)?
  test.eq(linked.links[0].parent_pci_function_index, 1)?
  root.remove(p"sys/class/net/eth0/device")?
  root.write(p"sys/class/net/eth0/device", "not a symlink")?
  let failed = collector.link_network_device_sources(root, source, [], [])
  test.ok(failed.status.state == report_model.Partial)?
  let parent_issues = failed.issues |> where .field == "links.2.parent"
  test.eq(parent_issues.len(), 1)?
  test.ok(parent_issues[0].state == report_model.ReadFailure)?
  test.ok(parent_issues[0].errno != null)?
}

test test_system_report_usb_descriptors_keep_configuration_and_endpoint_ownership [fs, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let descriptors = b"\t\x02\x19\0\x01\x01\0\x802\t\x04\0\0\x01\xff\0\0\0\x07\x05\x81\x02@\0\0\t\x02\x19\0\x01\x02\0\x802\t\x04\0\0\x01\x08\x06P\0\x07\x05\x82\x02\0\x02\0"
  let alternates = collector.parse_usb_alternates(descriptors)?
  test.eq(alternates.len(), 2)?
  test.eq(alternates[0].configuration_value, 1)?
  test.eq(alternates[1].configuration_value, 2)?
  test.eq(alternates[0].endpoints.len(), 1)?
  test.eq(alternates[1].endpoints.len(), 1)?
  test.eq(alternates[0].endpoints[0].address, 129)?
  test.eq(alternates[1].endpoints[0].address, 130)?
}

test test_system_report_usb_descriptor_parser_rejects_truncated_and_orphan_records [fs, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  test.error_kind(collector.parse_usb_alternates(b"\x02\x01"), "SystemReportUsbDescriptorError.Invalid")?
  test.error_kind(
    collector.parse_usb_alternates(b"\t\x02\x08\0\x01\x01\0\x802"),
    "SystemReportUsbDescriptorError.Invalid",
  )?
  test.error_kind(
    collector.parse_usb_alternates(b"\t\x02\xff\xff\x01\x01\0\x802"),
    "SystemReportUsbDescriptorError.Invalid",
  )?
  test.error_kind(collector.parse_usb_alternates(b"\x04\x04\0\0"), "SystemReportUsbDescriptorError.Invalid")?
  test.error_kind(collector.parse_usb_alternates(b"\x07\x05\x81\x02@\0\0"), "SystemReportUsbDescriptorError.Invalid")?
}

test test_system_report_usb_descriptor_parser_enforces_configuration_total_length [fs, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  test.error_kind(
    collector.parse_usb_alternates(b"\t\x02\t\0\x01\x01\0\x802\t\x04\0\0\0\xff\0\0\0"),
    "SystemReportUsbDescriptorError.Invalid",
  )?
  test.error_kind(
    collector.parse_usb_alternates(b"\t\x02\x12\0\x01\x01\0\x802\t\x04\0\0\x01\xff\0\0\0\x07\x05\x81\x02@\0\0"),
    "SystemReportUsbDescriptorError.Invalid",
  )?
}

test test_system_report_identity_uses_vendor_os_release_only_when_local_file_is_absent [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"usr/lib", parents: true)?
  root.mkdir(p"proc/self/ns", parents: true)?
  root.symlink(p"uts:[1001]", p"proc/self/ns/uts")?
  root.symlink(p"ipc:[1002]", p"proc/self/ns/ipc")?
  root.symlink(p"user:[1003]", p"proc/self/ns/user")?
  root.symlink(p"time:[1004]", p"proc/self/ns/time")?
  root.write(
    p"usr/lib/os-release",
    """ID=vendor
PRETTY_NAME="Vendor \\"Linux\\""
VERSION="v\\$token"
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let fallback = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let fallback_os = fallback.identity.os_release.require(report_model.OsRelease)?
  test.eq(fallback_os.id, "vendor")?
  test.eq(fallback_os.pretty_name, "Vendor \"Linux\"")?
  test.eq(fallback_os.version, "v$token")?
  test.eq(fallback.scope.uts_namespace.value, "uts:[1001]")?
  test.eq(fallback.scope.ipc_namespace.value, "ipc:[1002]")?
  test.eq(fallback.scope.user_namespace.value, "user:[1003]")?
  test.eq(fallback.scope.time_namespace.value, "time:[1004]")?
  root.mkdir(p"etc", parents: true)?
  root.write(
    p"etc/os-release",
    """ID=local
""",
  )?
  let local = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let local_os = local.identity.os_release.require(report_model.OsRelease)?
  test.eq(local_os.id, "local")?
  test.eq(local_os.pretty_name, null)?
}

test test_system_report_identity_withholds_malformed_os_release_values_without_vendor_fallback [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"etc", parents: true)?
  root.mkdir(p"usr/lib", parents: true)?
  root.write(
    p"usr/lib/os-release",
    """ID=vendor
VERSION_ID=99
""",
  )?
  root.write(
    p"etc/os-release",
    """# local source
ID=local
VERSION_ID="unterminated
PRETTY_NAME="Local System"
NAME=Unquoted Name
not-an-assignment
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let report = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let os = report.identity.os_release.require(report_model.OsRelease)?
  test.eq(os.id, "local")?
  test.eq(os.version_id, null)?
  test.eq(os.pretty_name, "Local System")?
  test.eq(os.name, null)?
  test.ok(report.issues |> any .field == "os_release.VERSION_ID" and .state == report_model.Malformed)?
  test.ok(report.issues |> any .field == "os_release.NAME" and .state == report_model.Malformed)?
  test.ok(report.issues |> any .field == "os_release.line.5" and .state == report_model.Malformed)?

  root.write(
    p"etc/os-release",
    """ID=first
ID=second
VERSION_ID=2
""",
  )?
  let repeated = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let repeated_os = repeated.identity.os_release.require(report_model.OsRelease)?
  test.eq(repeated_os.id, "second")?
  test.eq(repeated_os.version_id, "2")?
  test.ok(! (repeated.issues |> any .field.starts_with("os_release.")))?

  root.write(
    p"etc/os-release",
    """ID=first
ID=bad value
VERSION_ID=2
""",
  )?
  let malformed_duplicate = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let retained_os = malformed_duplicate.identity.os_release.require(report_model.OsRelease)?
  test.eq(retained_os.id, "first")?
  test.eq(retained_os.version_id, "2")?
  test.ok(malformed_duplicate.issues |> any .field == "os_release.ID" and .state == report_model.Malformed)?
}

test test_system_report_identity_marks_os_release_without_id_partial [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/sys/kernel/random", parents: true)?
  root.mkdir(p"etc", parents: true)?
  root.mkdir(p"usr/lib", parents: true)?
  root.write(
    p"proc/sys/kernel/osrelease",
    """fixture-release
""",
  )?
  root.write(
    p"proc/version",
    """Linux fixture build
""",
  )?
  root.write(
    p"proc/sys/kernel/hostname",
    """fixture-host
""",
  )?
  root.write(
    p"proc/sys/kernel/random/boot_id",
    """fixture-boot-id
""",
  )?
  root.write(
    p"proc/uptime",
    """73.5 12.0
""",
  )?
  root.write(
    p"etc/os-release",
    """NAME="Local System"
PRETTY_NAME="Local Test System"
""",
  )?
  root.write(
    p"usr/lib/os-release",
    """ID=vendor
""",
  )?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let collected = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let os = collected.identity.os_release.require(report_model.OsRelease)?
  test.eq(os.id, null)?
  test.eq(os.name, "Local System")?
  test.eq(os.pretty_name, "Local Test System")?
  test.eq(collected.identity.status.state, report_model.Partial)?
  test.ok(
    collected.issues |> any .section == "identity" and .field == "os_release.ID" and .state == report_model.Malformed,
  )?

  root.write(
    p"etc/os-release",
    """ID=first
ID=
NAME="Local System"
""",
  )?
  let malformed_duplicate = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let retained = malformed_duplicate.identity.os_release.require(report_model.OsRelease)?
  test.eq(retained.id, "first")?
  test.ok(
    malformed_duplicate.issues
      |> any .section == "identity" and .field == "os_release.ID" and .state == report_model.Malformed,
  )?

  root.write(
    p"etc/os-release",
    """ID=first
ID="Not A Distro"
""",
  )?
  let invalid_spelling = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  test.eq(invalid_spelling.identity.os_release.require(report_model.OsRelease)?.id, "first")?
  test.ok(
    invalid_spelling.issues |> any .section == "identity" and .field == "os_release.ID" and .state == report_model.Malformed,
  )?
}

test test_system_report_os_release_value_parser_rejects_malformed_assignments [fs, error] {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  test.eq(collectors.decode_os_release_value("\"Local \\\"System\\\"\""), "Local \"System\"")?
  test.eq(collectors.decode_os_release_value("\"v\\$token\""), "v$token")?
  test.eq(collectors.decode_os_release_value("'literal $value'"), "literal $value")?
  test.eq(collectors.decode_os_release_value("plain"), "plain")?
  test.eq(collectors.decode_os_release_value("v1.2-release_3"), "v1.2-release_3")?
  test.eq(
    collectors.decode_os_release_value("\"https://example.test/path?query=yes\""),
    "https://example.test/path?query=yes",
  )?
  for malformed in [
    "\"unterminated",
    "Unquoted Name",
    "\"unescaped $value\"",
    "plain\\",
    "semi;colon",
    "path/segment",
    "pipe|value",
    "hash#value",
    "escaped\\ space",
    " leading",
    "trailing ",
    "\"quoted\" ",
  ] {
    test.ok(collectors.decode_os_release_value(malformed) == null)?
  }

  test.ok(collectors.valid_os_release_key("VERSION_ID"))?
  test.ok(collectors.valid_os_release_key("VENDOR_FIELD2"))?
  for valid_id in ["linux", "my_os-2.3", "0"] {
    test.ok(collectors.valid_os_release_id(valid_id))?
  }

  for invalid_id in ["", "Ubuntu", "with space", "has/slash", "with:colon", "\u{e9}"] {
    test.ok(! collectors.valid_os_release_id(invalid_id))?
  }

  for malformed_key in ["", "1ID", "ID-NAME", "ID "] {
    test.ok(! collectors.valid_os_release_key(malformed_key))?
  }
}

test test_system_report_device_tree_strings_require_complete_terminated_values [fs, error] {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  test.eq(collectors.decode_device_tree_strings("ARM Test Board\0"), ["ARM Test Board"])?
  test.eq(collectors.decode_device_tree_strings("vendor,board\0vendor,soc\0"), ["vendor,board", "vendor,soc"])?
  for malformed in ["", "ARM Test Board", "vendor,board\0vendor,soc", "vendor,board\0\0", "\0"] {
    test.ok(collectors.decode_device_tree_strings(malformed) == null)?
  }
}

test test_system_report_arm_identity_preserves_heterogeneous_cpus_without_dmi [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.mkdir(p"sys/firmware/devicetree/base", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu0", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu1", parents: true)?
  root.write(p"sys/firmware/devicetree/base/model", "ARM Example Board\0")?
  root.write(p"sys/firmware/devicetree/base/compatible", "vendor,example\0arm,v8\0")?
  root.write(
    p"sys/devices/system/cpu/possible",
    """0-1
""",
  )?
  root.write(
    p"sys/devices/system/cpu/present",
    """0-1
""",
  )?
  root.write(
    p"sys/devices/system/cpu/online",
    """0-1
""",
  )?
  root.write(p"sys/devices/system/cpu/offline", "\n")?
  root.write(
    p"proc/cpuinfo",
    """processor: 0
CPU implementer: 0x41
CPU part: 0xd05
Processor: Cortex-A55
Features: fp asimd

processor: 1
CPU implementer: 0x41
CPU part: 0xd0b
Processor: Cortex-A76
Features: fp asimd crc32
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let identity = collector.collect_from_root(root, "aarch64", 65536, 100, "identity", true)?
  let platform = identity.identity.firmware.require(report_model.FirmwareIdentity)?
  test.eq(identity.identity.architecture, "aarch64")?
  test.eq(platform.source, "device-tree")?
  test.eq(platform.device_tree_model.value, "ARM Example Board")?
  test.eq(platform.device_tree_compatible.len(), 2)?
  test.eq(platform.device_tree_compatible[0].value, "vendor,example")?
  test.eq(platform.device_tree_compatible[1].value, "arm,v8")?
  test.eq(platform.vendor, null)?
  let cpus = collector.collect_from_root(root, "aarch64", 65536, 100, "cpu", true)?
  test.eq(cpus.cpu.cpus.len(), 2)?
  test.eq(cpus.cpu.cpus[0].model_id, "0xd05")?
  test.eq(cpus.cpu.cpus[0].model, "Cortex-A55")?
  test.eq(cpus.cpu.cpus[1].model_id, "0xd0b")?
  test.eq(cpus.cpu.cpus[1].model, "Cortex-A76")?
  let firmware = collector.collect_from_root(root, "aarch64", 65536, 100, "firmware", true)?
  test.eq(firmware.firmware.source, "device-tree")?
  test.eq(firmware.firmware.records.len(), 0)?

  root.remove(p"sys/firmware/devicetree/base/model")?
  let compatible_only = collector.collect_from_root(root, "aarch64", 65536, 100, "firmware", true)?
  test.eq(compatible_only.firmware.source, "device-tree")?
  test.eq(compatible_only.firmware.records.len(), 0)?

  root.write(p"sys/firmware/devicetree/base/model", "ARM Example Board")?
  root.write(p"sys/firmware/devicetree/base/compatible", "vendor,example\0arm,v8")?
  let invalid = collector.collect_from_root(root, "aarch64", 65536, 100, "identity", true)?
  let invalid_platform = invalid.identity.firmware.require(report_model.FirmwareIdentity)?
  test.eq(invalid_platform.source, "unavailable")?
  test.eq(invalid_platform.device_tree_model.state, report_model.Malformed)?
  test.eq(invalid_platform.device_tree_compatible.len(), 0)?
  test.ok(invalid.issues |> any .field == "firmware.device_tree_model" and .state == report_model.Malformed)?
  test.ok(invalid.issues |> any .field == "firmware.device_tree_compatible" and .state == report_model.Malformed)?
  let invalid_firmware = collector.collect_from_root(root, "aarch64", 65536, 100, "firmware", true)?
  test.eq(invalid_firmware.firmware.source, "unavailable")?
  test.ok(invalid_firmware.issues |> any .field == "device_tree_model" and .state == report_model.Malformed)?
  test.ok(invalid_firmware.issues |> any .field == "device_tree_compatible" and .state == report_model.Malformed)?
}

test test_system_report_identity_rejects_truncated_source_prefixes [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/sys/kernel", parents: true)?
  root.mkdir(p"etc", parents: true)?
  root.mkdir(p"usr/lib", parents: true)?
  root.mkdir(p"sys/class/dmi/id", parents: true)?
  root.mkdir(p"sys/firmware/devicetree/base", parents: true)?
  root.write(
    p"proc/sys/kernel/osrelease",
    """fixture-release
""",
  )?
  root.write(
    p"proc/version",
    """Linux fixture build
""",
  )?
  root.write(
    p"usr/lib/os-release",
    """ID=vendor
""",
  )?
  var padding = " "
  while padding.count_chars() < 65536 {
    padding = f"${padding}${padding}"
  }

  root.write(
    p"etc/os-release",
    f"""ID=partial
${padding}""",
  )?
  root.write(
    p"proc/uptime",
    f"""73.5 12.0
${padding}""",
  )?
  root.write(
    p"sys/class/dmi/id/sys_vendor",
    f"""Acme
${padding}""",
  )?
  root.write(p"sys/firmware/devicetree/base/compatible", f"acme,board\0${padding}")?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let partial = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  test.eq(partial.identity.os_release, null)?
  test.eq(partial.identity.uptime_seconds, null)?
  let firmware = partial.identity.firmware.require(report_model.FirmwareIdentity)?
  test.eq(firmware.vendor, null)?
  test.eq(firmware.device_tree_compatible, [])?
  test.eq(firmware.source, "unavailable")?
  for field in ["os_release", "uptime", "firmware.vendor", "firmware.device_tree_compatible"] {
    test.ok(partial.issues |> any .section == "identity" and .field == field and .state == report_model.Truncated)?
  }

  test.eq(partial.identity.status.state, report_model.Partial)?

  root.write(p"sys/firmware/devicetree/base/compatible", "acme,board\0acme,soc\0")?
  let complete_dt = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let observed = complete_dt.identity.firmware.require(report_model.FirmwareIdentity)?
  test.eq(observed.source, "device-tree")?
  test.eq(observed.device_tree_compatible.len(), 2)?
  test.eq(observed.device_tree_compatible[0].value, "acme,board")?
}

test test_system_report_identity_retains_dmi_placeholder_text_as_raw_values [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/dmi/id", parents: true)?
  root.write(
    p"sys/class/dmi/id/sys_vendor",
    """To Be Filled By O.E.M.
""",
  )?
  root.write(
    p"sys/class/dmi/id/product_name",
    """Default string
""",
  )?
  root.write(
    p"sys/class/dmi/id/board_name",
    """Not Specified
""",
  )?
  root.write(
    p"sys/class/dmi/id/product_serial",
    """System Serial Number
""",
  )?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let collected = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let firmware = collected.identity.firmware.require(report_model.FirmwareIdentity)?
  test.eq(firmware.source, "dmi")?
  test.eq(firmware.vendor, "To Be Filled By O.E.M.")?
  test.eq(firmware.product, "Default string")?
  test.eq(firmware.board_product, "Not Specified")?
  test.eq(firmware.serial.state, report_model.Observed)?
  test.eq(firmware.serial.value, "System Serial Number")?
  test.ok(! (collected.issues |> any .field.starts_with("firmware.")))?
}

test test_system_report_scope_keeps_all_process_visible_namespace_identities [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self/ns", parents: true)?
  for namespace in [
    {
      name: "mnt",
      target: "mnt:[101]",
    },
    {
      name: "net",
      target: "net:[102]",
    },
    {
      name: "pid",
      target: "pid:[103]",
    },
    {
      name: "cgroup",
      target: "cgroup:[104]",
    },
    {
      name: "uts",
      target: "uts:[105]",
    },
    {
      name: "ipc",
      target: "ipc:[106]",
    },
    {
      name: "user",
      target: "user:[107]",
    },
    {
      name: "time",
      target: "time:[108]",
    },
  ] {
    root.symlink(fp"${namespace.target}", fp"proc/self/ns/${namespace.name}")?
  }

  root.write(
    p"proc/self/cgroup",
    """0::/container.slice/workload
""",
  )?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let collected = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  test.contains(collected.scope.host_claim, "process-visible")?
  test.eq(collected.scope.mount_namespace.value, "mnt:[101]")?
  test.eq(collected.scope.network_namespace.value, "net:[102]")?
  test.eq(collected.scope.pid_namespace.value, "pid:[103]")?
  test.eq(collected.scope.cgroup_namespace.value, "cgroup:[104]")?
  test.eq(collected.scope.uts_namespace.value, "uts:[105]")?
  test.eq(collected.scope.ipc_namespace.value, "ipc:[106]")?
  test.eq(collected.scope.user_namespace.value, "user:[107]")?
  test.eq(collected.scope.time_namespace.value, "time:[108]")?
  test.eq(collected.scope.visible_cgroup.value, "0::/container.slice/workload")?
  test.ok(! (collected.issues |> any .section == "scope"))?
}

test test_system_report_identity_preserves_namespace_link_failures [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self/ns", parents: true)?
  root.write(p"proc/self/ns/mnt", "not a symlink")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let snapshot = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  test.ok(snapshot.scope.mount_namespace.state == report_model.ReadFailure)?
  test.eq(snapshot.scope.mount_namespace.value, null)?
  let failures = snapshot.issues |> where .section == "scope" and .field == "mount_namespace"
  test.eq(failures.len(), 1)?
  test.ok(failures[0].state == report_model.ReadFailure)?
  test.ok(failures[0].errno != null)?
  test.ok(snapshot.scope.network_namespace.state == report_model.Absent)?
  let selected = report_model.select_report_section(snapshot, "identity")?.require(report_model.SystemReport)?
  test.eq((selected.issues |> where .section == "scope" and .field == "mount_namespace").len(), 1)?
}

pure cpu_policy(name: Str, related_cpus: List[Int], affected_cpus: List[Int]) -> FixtureCpuFreqPolicy {
  return {
    name: name,
    related_cpus: related_cpus,
    affected_cpus: affected_cpus,
    driver: "intel_pstate",
    governor: "powersave",
    available_governors: [
      "powersave",
      "performance",
    ],
    hardware_min_khz: 800000,
    hardware_max_khz: 4200000,
    scaling_min_khz: 800000,
    scaling_max_khz: 4200000,
    hardware_current_khz: null,
    scaling_current_khz: 1800000,
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

pure pci_function(address: Str, parent_function_index: Int?) -> FixturePciFunction {
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
    producer: {
      name: "system-report",
      version: "test",
    },
    source_mode: "synthetic_fixture",
    collection_started_unix_ms: 10,
    collection_ended_unix_ms: 20,
    elapsed_ms: 10,
    scope: {
      platform: "Linux",
      host_claim: "container",
      source_roots: [
        "/proc",
        "/sys",
      ],
      mount_namespace: json_observation("observed", "mnt:[4026531840]"),
      network_namespace: json_observation("observed", "net:[4026531840]"),
      pid_namespace: json_observation("observed", "pid:[4026531836]"),
      cgroup_namespace: json_observation("observed", "cgroup:[4026531835]"),
      uts_namespace: json_observation("observed", "uts:[4026531838]"),
      ipc_namespace: json_observation("observed", "ipc:[4026531839]"),
      user_namespace: json_observation("observed", "user:[4026531841]"),
      time_namespace: json_observation("observed", "time:[4026531842]"),
      visible_cgroup: json_observation("observed", "/user.slice/user-1000.slice"),
      page_size_bytes: 4096,
      clock_ticks_per_second: 100,
      ancestors_may_be_hidden: true,
    },
    redacted: false,
    identity: {
      status: json_section("complete"),
      kernel_release: "6.12-test",
      kernel_build: "Linux version 6.12-test (builder@private-build-host)",
      architecture: "x86_64",
      os_release: {
        id: "linux",
        name: "Linux",
        pretty_name: "Test Linux",
        version: null,
        version_id: null,
      },
      hostname: json_observation("observed", "workstation-name"),
      uptime_seconds: 100,
      boot_id: json_observation("observed", "10000000-0000-0000-0000-000000000000"),
      firmware: null,
    },
    cpu: {
      status: json_section("complete"),
      possible: [
        0,
        1,
        2,
      ],
      present: [
        0,
        1,
        2,
      ],
      online: [
        0,
        1,
        2,
      ],
      offline: [],
      affinity: [
        0,
        1,
        2,
      ],
      effective_cpuset: [
        0,
        1,
        2,
      ],
      global_idle_driver: null,
      global_idle_governor: null,
      cpus: [],
      caches: [],
      frequency_policies: [
        cpu_policy("policy0", [0], [0]),
        cpu_policy("policy1", [1], [1]),
        {
          ...cpu_policy("policy2", [2], [2]),
          scaling_max_khz: 3600000,
        },
      ],
      idle_states: [],
      vulnerabilities: [
        {
          name: "spectre_v1",
          description: json_observation("observed", "mitigation active"),
        },
      ],
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
      swaps: [
        {
          name: json_observation("observed", "/dev/mapper/swap-private"),
          kind: "partition",
          size_bytes: 4096,
          used_bytes: 0,
          priority: -2,
        },
      ],
      huge_pages: [],
      transparent_huge_pages: [],
      numa: [],
      pressure: [],
      cgroup: [
        {
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
          effective_cpus: [
            0,
            1,
          ],
          hidden_ancestors_possible: true,
        },
      ],
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
      devices: [
        {
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
          configuration_count: 2,
          active_configuration: 1,
          power_control: "on",
          autosuspend_delay_ms: 2000,
          runtime_status: "active",
          is_root_hub: false,
          interfaces: [
            {
              number: 0,
              name: "1-2.3:1.0",
              driver: "usbhid",
              active_alternate: 0,
              alternate_settings: [
                {
                  configuration_value: 1,
                  number: 0,
                  class_code: 3,
                  subclass: 1,
                  protocol: 1,
                  endpoints: [
                    {
                      address: 129,
                      direction: "in",
                      transfer_type: "interrupt",
                      max_packet_size: 64,
                      interval: 10,
                    },
                  ],
                },
                {
                  configuration_value: 2,
                  number: 0,
                  class_code: 8,
                  subclass: 6,
                  protocol: 80,
                  endpoints: [
                    {
                      address: 130,
                      direction: "in",
                      transfer_type: "bulk",
                      max_packet_size: 512,
                      interval: 0,
                    },
                  ],
                },
              ],
            },
          ],
        },
      ],
    },
    storage: {
      status: json_section("complete"),
      devices: [
        {
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
          available_schedulers: [
            "none",
            "mq-deadline",
          ],
          read_ahead_kb: 128,
          discard_granularity_bytes: 4096,
          discard_max_bytes: 1048576,
          io_counters: [],
        },
      ],
      mounts: [
        {
          mount_id: 42,
          parent_id: 1,
          major: 259,
          minor: 1,
          root: json_observation("observed", "/"),
          target: json_observation("observed", "/home/private"),
          mount_options: [
            "rw",
            "relatime",
            "context=private-mount-label",
          ],
          optional_fields: [
            "shared:42",
            "unknown=private-mount-field",
          ],
          filesystem: "ext4",
          source: json_observation("observed", "/dev/nvme0n1p1"),
          super_options: [
            "rw",
            "lowerdir=/private/host/snapshot",
            "password=mount-secret",
            "uid=mount-secret",
          ],
          block_device_index: 0,
          usage_state: "observed",
          usage_total_bytes: 8192,
          usage_used_bytes: 4096,
          usage_available_bytes: 4096,
        },
      ],
    },
    network: {
      status: json_section("complete"),
      links: [
        {
          ifindex: 2,
          hardware_type: 1,
          name: json_observation("observed", "eth0"),
          kind: "ether",
          mtu: 1500,
          admin_up: true,
          operational_state: "up",
          flags: [
            "broadcast",
            "multicast",
            "up",
          ],
          mac: json_observation("observed", "02:00:00:00:00:01"),
          master_ifindex: null,
          lower_ifindex: null,
          parent_pci_function_index: null,
          parent_usb_device_index: null,
          driver: "example_net",
          addresses: [
            {
              family: "ipv6",
              address: json_observation("observed", "2001:db8::1"),
              prefix_length: 64,
              broadcast: json_observation("absent", null),
              scope: "global",
              flags: 0,
              valid_lifetime_seconds: null,
              preferred_lifetime_seconds: null,
              attributes: [
                {
                  kind: 77,
                  data: json_observation("observed", "eA=="),
                },
              ],
            },
          ],
          counters: [],
          attributes: [
            {
              kind: 88,
              data: json_observation("observed", "eA=="),
            },
          ],
        },
      ],
      routes: [
        {
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
          nexthops: [
            {
              ifindex: 2,
              flags: 1,
              hops: 0,
              gateway: json_observation("observed", "192.0.2.1"),
            },
          ],
          attributes: [
            {
              kind: 99,
              data: json_observation("observed", "AQI="),
            },
          ],
        },
      ],
      rules: [],
    },
    sensors: {
      status: json_section("complete"),
      channels: [
        {
          chip: "example_hwmon",
          chip_entry_name: "hwmon0",
          channel: "temp1",
          label: json_observation("observed", "private-sensor-label"),
          kind: "temperature",
          value: 42000,
          unit: "millidegrees_celsius",
          minimum: null,
          maximum: null,
          critical: 100000,
          alarm: false,
          parent_device_class_index: null,
          parent_pci_function_index: null,
          parent_usb_device_index: null,
        },
      ],
      thermal_zones: [],
    },
    power: {
      status: json_section("complete"),
      supplies: [],
      cap_zones: [],
    },
    firmware: {
      status: json_section("complete"),
      source: "fixture",
      records: [
        {
          record_type: 1,
          handle: 64,
          formatted_length: 27,
          fields: [
            {
              name: "wake_up_type",
              value: 6,
              unit: "enumeration",
            },
          ],
          strings: [
            json_observation("observed", "Example System"),
          ],
        },
      ],
      limitation: json_observation("absent", null),
    },
    kernel: {
      status: json_section("complete"),
      command_line: json_observation("observed", "root=UUID=machine-private"),
      modules: [],
      parameters: [
        {
          name: "example_module/value",
          value: json_observation("observed", "private-value"),
        },
      ],
      sysctls: [
        {
          name: "kernel.ostype",
          value: json_observation("observed", "Linux"),
        },
      ],
    },
    processes: {
      status: json_section("complete"),
      processes: [
        {
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
        },
      ],
    },
    devices: {
      status: json_section("complete"),
      devices: [
        {
          class: "drm",
          entry_name: json_observation("observed", "card0"),
          name: json_observation("observed", "card0"),
          parent_device_class_index: null,
          parent_pci_function_index: null,
          parent_usb_device_index: null,
          driver: "example_gpu",
          attributes: [
            {
              name: "status",
              value: json_observation("observed", "connected"),
            },
          ],
        },
      ],
    },
    issues: [
      {
        section: "identity",
        field: "hostname",
        state: "permission_denied",
        error_kind: "permission_denied",
        errno: 13,
        detail: json_observation("observed", "private-path detail"),
      },
      {
        section: "pci",
        field: "functions.0000:00:1f.6.vendor_id",
        state: "permission_denied",
        error_kind: "permission_denied",
        errno: 13,
        detail: json_observation("observed", "private PCI source path"),
      },
    ],
  }
}

test test_system_report_model_relationships [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let policies = [
    cpu_policy("policy0", [0, 2], [0]),
    cpu_policy("policy1", [1, 3], [1, 3]),
  ]

  let offline_member_policy = model.frequency_policies_for_cpu(policies, 2)
  test.eq(offline_member_policy.len(), 1)?
  let selected_policy = offline_member_policy[0]
  test.eq(selected_policy.name, "policy0")?
  test.eq(model.frequency_policies_for_cpu(policies, 4).len(), 0)?

  let functions = [
    pci_function("0000:00:01.0", null),
    pci_function("0001:02:03.0", 0),
    pci_function("0002:04:05.0", 9),
  ]
  let child = functions[1]
  let parent = model.pci_parent_function(functions, child)
  if parent != null {
    test.eq(parent.address, "0000:00:01.0")?
  } else {
    test.fail("indexed PCI parent did not resolve")?
  }

  let root_function = functions[0]
  test.eq(model.pci_parent_function(functions, root_function), null)?

  let unresolved_child = functions[2]
  test.eq(model.pci_parent_function(functions, unresolved_child), null)?
}

test test_system_report_cpu_list_parser_handles_sparse_and_large_ids [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let sparse = model.parse_cpu_list("2-4,66,129-130")?
  test.eq(sparse, [2, 3, 4, 66, 129, 130])?

  let many = model.parse_cpu_list("0-127")?
  test.eq(many.len(), 128)?
  test.ok(65 in many)?
  test.ok(127 in many)?

  test.error_kind(model.parse_cpu_list(""), "SystemReportError.InvalidCpuList")?
  test.error_kind(model.parse_cpu_list("4,,8"), "SystemReportError.InvalidCpuList")?
  test.error_kind(model.parse_cpu_list("5-2"), "SystemReportError.InvalidCpuList")?
  test.error_kind(model.parse_cpu_list("1,1"), "SystemReportError.InvalidCpuList")?
  test.error_kind(model.parse_cpu_list("0-65536"), "SystemReportError.InvalidCpuList")?
}

test test_system_report_cpu_collection_preserves_128_present_ids [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu", parents: true)?
  for cpu_id in range(128) {
    root.mkdir(fp"sys/devices/system/cpu/cpu${cpu_id}")?
  }

  root.write(
    p"sys/devices/system/cpu/possible",
    """0-127
""",
  )?
  root.write(
    p"sys/devices/system/cpu/present",
    """0-127
""",
  )?
  root.write(
    p"sys/devices/system/cpu/online",
    """0-127
""",
  )?
  root.write(p"sys/devices/system/cpu/offline", "\n")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.ok(value.cpu.status.enumeration_succeeded)?
  test.eq(value.cpu.present.len(), 128)?
  test.eq(value.cpu.cpus.len(), 128)?
  test.eq(value.cpu.cpus[0].id, 0)?
  test.eq(value.cpu.cpus[127].id, 127)?
  test.eq(value.cpu.online.len(), 128)?
}

test test_system_report_cpu_collection_keeps_absent_cpufreq_unavailable [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu/cpu0", parents: true)?
  root.mkdir(p"proc/sys/kernel", parents: true)?
  root.write(
    p"proc/sys/kernel/osrelease",
    """fixture-vm-release
""",
  )?
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )?
  root.write(p"sys/devices/system/cpu/offline", "\n")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.ok(value.cpu.status.enumeration_succeeded)?
  test.eq(value.cpu.cpus.len(), 1)?
  test.eq(value.cpu.frequency_policies.len(), 0)?
  test.eq(value.cpu.cpus[0].policy, null)?
  test.eq(value.cpu.idle_states.len(), 0)?
  test.eq(value.cpu.global_idle_driver, null)?
  test.eq(value.identity.kernel_release, "fixture-vm-release")?
}

test test_system_report_uptime_parser_requires_complete_two_column_decimal [fs, error] {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let observed: SourceRead = {
    observation: {
      state: report_model.Observed,
      value: "73.50 12.34",
      raw_bytes_base64: null,
    },
    errno: null,
    error_kind: null,
  }
  test.eq(collectors.parse_uptime_seconds(observed).value, 73)?
  test.eq(
    collectors.parse_uptime_seconds(
      {...observed, observation: {...observed.observation, value: "9007199254740991.99 0.00"}},
    ).value,
    9007199254740991,
  )?
  let unsafe = collectors.parse_uptime_seconds(
    {...observed, observation: {...observed.observation, value: "9007199254740992.00 0.00"}},
  )
  test.eq(unsafe.value, null)?
  test.ok(unsafe.state == report_model.RangeFailure)?
  for malformed in ["73", "73 0.00", "73.50", "-1.00 0.00", "73. 0.00", "73.50 0.x", "73.50 0.00 extra"] {
    let parsed = collectors.parse_uptime_seconds(
      {...observed, observation: {...observed.observation, value: malformed}},
    )
    test.eq(parsed.value, null)?
    test.ok(parsed.state == report_model.Malformed)?
  }

  let truncated = collectors.parse_uptime_seconds(
    {...observed, observation: {...observed.observation, state: report_model.Truncated}},
  )
  test.eq(truncated.value, null)?
  test.ok(truncated.state == report_model.Truncated)?
  let absent = collectors.parse_uptime_seconds(
    {...observed, observation: {...observed.observation, state: report_model.Absent, value: null}},
  )
  test.eq(absent.value, null)?
  test.ok(absent.state == report_model.Absent)?
}

test test_system_report_bounded_number_respects_source_state_and_json_range [fs, error] {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let observed: SourceRead = {
    observation: {
      state: report_model.Observed,
      value: "-5000",
      raw_bytes_base64: null,
    },
    errno: null,
    error_kind: null,
  }
  let signed = collectors.bounded_number(observed, false)
  test.eq(signed.value, -5000)?
  test.ok(signed.state == null)?
  let unsigned = collectors.bounded_number(observed, true)
  test.ok(unsigned.value == null)?
  test.ok(unsigned.state == report_model.Malformed)?
  test.eq(unsigned.error_kind, "negative_integer")?
  test.ok(
    collectors.bounded_number({...observed, observation: {...observed.observation, value: "-0"}}, true).state == report_model.Malformed,
  )?
  let maximum = collectors.bounded_number(
    {...observed, observation: {...observed.observation, value: "9007199254740991"}},
    true,
  )
  test.eq(maximum.value, 9007199254740991)?
  let unsafe_json = collectors.bounded_number(
    {...observed, observation: {...observed.observation, value: "9007199254740992"}},
    true,
  )
  test.ok(unsafe_json.value == null)?
  test.ok(unsafe_json.state == report_model.RangeFailure)?
  let overflow = collectors.bounded_number(
    {...observed, observation: {...observed.observation, value: "999999999999999999999999"}},
    true,
  )
  test.ok(overflow.state == report_model.RangeFailure)?
  let invalid = collectors.bounded_number({...observed, observation: {...observed.observation, value: "42 C"}}, false)
  test.ok(invalid.state == report_model.Malformed)?
  for text_value in ["0x2a", "1_000", "+42"] {
    let nondecimal = collectors.bounded_number(
      {...observed, observation: {...observed.observation, value: text_value}},
      false,
    )
    test.ok(nondecimal.value == null)?
    test.ok(nondecimal.state == report_model.Malformed)?
  }

  test.eq(collectors.bounded_number({...observed, observation: {...observed.observation, value: "007"}}, true).value, 7)?
  let truncated = collectors.bounded_number(
    {...observed, observation: {...observed.observation, state: report_model.Truncated, value: "68"}},
    true,
  )
  test.ok(truncated.value == null)?
  test.ok(truncated.state == report_model.Truncated)?
  let absent = collectors.bounded_number(
    {...observed, observation: {...observed.observation, state: report_model.Absent, value: null}},
    true,
  )
  test.ok(absent.value == null)?
  test.ok(absent.state == null)?
  let denied = collectors.bounded_number(
    {
      ...observed,
      observation: {
        ...observed.observation,
        state: report_model.PermissionDenied,
        value: null,
      },
      errno: 13,
      error_kind: "permission_denied",
    },
    true,
  )
  test.ok(denied.state == report_model.PermissionDenied)?
  test.eq(denied.errno, 13)?
}

test test_system_report_bounded_size_bytes_checks_scaled_json_range [fs, error] {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let observed: SourceRead = {
    observation: {
      state: report_model.Observed,
      value: "8796093022207K",
      raw_bytes_base64: null,
    },
    errno: null,
    error_kind: null,
  }
  test.eq(collectors.bounded_size_bytes(observed).value, 9007199254739968)?
  let kilobyte_overflow = collectors.bounded_size_bytes(
    {...observed, observation: {...observed.observation, value: "8796093022208K"}},
  )
  test.ok(kilobyte_overflow.value == null)?
  test.ok(kilobyte_overflow.state == report_model.RangeFailure)?
  test.eq(
    collectors.bounded_size_bytes({...observed, observation: {...observed.observation, value: "8589934591M"}}).value,
    9007199253692416,
  )?
  test.ok(
    collectors.bounded_size_bytes({...observed, observation: {...observed.observation, value: "8589934592M"}}).state == report_model.RangeFailure,
  )?
  test.eq(
    collectors.bounded_size_bytes({...observed, observation: {...observed.observation, value: "8388607G"}}).value,
    9007198180999168,
  )?
  test.ok(
    collectors.bounded_size_bytes({...observed, observation: {...observed.observation, value: "8388608G"}}).state == report_model.RangeFailure,
  )?
  test.eq(
    collectors.bounded_size_bytes({...observed, observation: {...observed.observation, value: "9007199254740991"}}).value,
    9007199254740991,
  )?
  test.ok(
    collectors.bounded_size_bytes({...observed, observation: {...observed.observation, value: "9007199254740992"}}).state == report_model.RangeFailure,
  )?
  test.ok(
    collectors.bounded_size_bytes({...observed, observation: {...observed.observation, value: "1T"}}).state == report_model.Malformed,
  )?
  test.ok(
    collectors.bounded_size_bytes({...observed, observation: {...observed.observation, value: "0x10K"}}).state == report_model.Malformed,
  )?
  test.ok(
    collectors.bounded_size_bytes({...observed, observation: {...observed.observation, value: "-1K"}}).state == report_model.Malformed,
  )?
  let truncated = collectors.bounded_size_bytes(
    {...observed, observation: {...observed.observation, state: report_model.Truncated, value: "512K"}},
  )
  test.ok(truncated.value == null)?
  test.ok(truncated.state == report_model.Truncated)?
}

test test_system_report_psi_average_parser_rejects_invalid_percentages [fs, error] {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  for value in ["0.00", "1.50", "99.99", "100.00"] {
    test.ok(collectors.valid_psi_average(value))?
  }

  for value in [
    "nan",
    "inf",
    "-1.00",
    "0.0",
    "0.000",
    "100.01",
    "101.00",
    "0x1.00",
    "1_0.00",
  ] {
    test.ok(! collectors.valid_psi_average(value))?
  }
}

test test_system_report_thp_policy_parser_keeps_unknown_selected_value [fs, error] {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let policy = collectors.parse_thp_policy("always [future_policy] never")?
  test.eq(policy.selected, "future_policy")?
  test.eq(policy.available, ["always", "future_policy", "never"])?
  for value in [
    "always future_policy never",
    "[always] [never]",
    "[always] always",
    "[[]",
    """always [future_policy] never
extra""",
  ] {
    test.error_kind(collectors.parse_thp_policy(value), "SystemReportSourceError.InvalidThpPolicy")?
  }
}

test test_system_report_pci_and_usb_source_parsers [fs, error] {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?

  let address: PciAddress = collectors.parse_pci_address("0001:af:1f.7")?
  test.eq(address, {domain: 1, bus: 175, device: 31, function: 7})?
  test.eq(collectors.parse_pci_hex_value("0x10DE")?, 4318)?
  test.eq(collectors.parse_pci_hex_value("10de")?, 4318)?
  test.error_kind(collectors.parse_pci_address("0000:00:20.0"), "SystemReportSourceError.InvalidPciAddress")?
  test.error_kind(collectors.parse_pci_address("0000:00:01.8"), "SystemReportSourceError.InvalidPciAddress")?
  test.error_kind(collectors.parse_pci_address("00:00:01.0"), "SystemReportSourceError.InvalidPciAddress")?
  test.error_kind(collectors.parse_pci_hex_value("0x10xz"), "SystemReportSourceError.InvalidPciId")?
  test.eq(
    collectors.pci_parent_address(../../../devices/pci0001:02/0001:02:01.0/0001:02:03.0, "0001:02:03.0"),
    "0001:02:01.0",
  )?
  test.ok(collectors.pci_parent_address(../../../devices/pci0001:02/0001:02:03.0, "0001:02:03.0") == null)?
  test.ok(collectors.pci_parent_address(../../../devices/pci0001:02/0001:02:01.0, "0001:02:03.0") == null)?

  let descriptors = collectors.parse_usb_descriptor_stream(b"\x03\x99B\x02\xfe")?
  test.eq(descriptors.len(), 2)?
  test.eq(descriptors[0].offset, 0)?
  test.eq(descriptors[0].descriptor_type, 153)?
  test.eq(descriptors[0].raw, b"\x03\x99B")?
  test.eq(descriptors[1].offset, 3)?
  test.eq(descriptors[1].descriptor_type, 254)?
  test.error_kind(collectors.parse_usb_descriptor_stream(b"\x01\x02"), "SystemReportSourceError.InvalidUsbDescriptor")?
  test.error_kind(collectors.parse_usb_descriptor_stream(b"\x04\x01x"), "SystemReportSourceError.InvalidUsbDescriptor")?
  test.error_kind(collectors.parse_usb_descriptor_stream(b"\t"), "SystemReportSourceError.InvalidUsbDescriptor")?
}

test test_system_report_pci_collection_links_a_child_to_its_bridge [fs, error] {
  let root = fs.tempdir()?
  defer root.close()?
  let parent_path = p"sys/devices/pci0001:02/0001:02:01.0"
  let child_path = p"sys/devices/pci0001:02/0001:02:01.0/0001:02:03.0"
  root.mkdir(child_path, parents: true)?
  root.mkdir(p"sys/bus/pci/devices", parents: true)?
  root.symlink(../../../devices/pci0001:02/0001:02:01.0, p"sys/bus/pci/devices/0001:02:01.0")?
  root.symlink(../../../devices/pci0001:02/0001:02:01.0/0001:02:03.0, p"sys/bus/pci/devices/0001:02:03.0")?
  for device_path in [parent_path, child_path] {
    root.write(
      fp"${device_path}/vendor",
      """0x1234
""",
    )?
    root.write(
      fp"${device_path}/device",
      """0xabcd
""",
    )?
    root.write(
      fp"${device_path}/subsystem_vendor",
      """0x1234
""",
    )?
    root.write(
      fp"${device_path}/subsystem_device",
      """0x0001
""",
    )?
    root.write(
      fp"${device_path}/class",
      """0x060400
""",
    )?
    root.write(
      fp"${device_path}/revision",
      """0x01
""",
    )?
  }

  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let collection = collectors.collect_pci(root)
  test.ok(collection.status.enumeration_succeeded)?
  test.eq(collection.functions.len(), 2)?
  test.eq(collection.functions[0].vendor_id, collection.functions[1].vendor_id)?
  test.eq(collection.functions[0].device_id, collection.functions[1].device_id)?
  test.ok(collection.functions[0].address != collection.functions[1].address)?
  test.eq(collection.functions[0].driver, null)?
  test.eq(collection.functions[1].driver, null)?
  test.ok(collection.functions[0].parent_function_index == null)?
  test.eq(collection.functions[1].parent_function_index, 0)?
}

test test_system_report_pci_collection_reports_non_utf8_names_without_losing_valid_functions [fs, env, error] {
  if system.uname()?.sysname == "Darwin" {
    test.skip("macOS filesystems reject non-UTF-8 filenames")
    return
  }

  let root = fs.tempdir()?
  defer root.close()?
  let valid = p"sys/bus/pci/devices/0000:00:01.0"
  let invalid = Path.parse_bytes(b"sys/bus/pci/devices/raw\xffname")?
  root.mkdir(valid, parents: true)?
  root.mkdir(invalid, parents: true)?
  for source in ["vendor", "device", "subsystem_vendor", "subsystem_device", "class", "revision"] {
    root.write(
      fp"${valid}/${source}",
      """0x0001
""",
    )?
  }

  test.eq(root.children(p"sys/bus/pci/devices")?.children.len(), 2)?

  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let collected = collectors.collect_pci(root)
  test.eq(collected.functions.len(), 1)?
  test.eq(collected.functions[0].address, "0000:00:01.0")?
  let invalid_issues = collected.issues |> where .error_kind == "invalid_pci_address"
  test.eq(invalid_issues.len(), 1)?
  test.ok(collected.status.state == report_model.Partial)?
}

test test_system_report_pci_multifunction_keeps_optional_link_sources_distinct [fs, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/bus/pci/devices", parents: true)?
  let first_path = p"sys/devices/pci0000:01/0000:01:02.0"
  let second_path = p"sys/devices/pci0000:01/0000:01:02.1"
  for source_path in [first_path, second_path] {
    root.mkdir(source_path, parents: true)?
    root.write(
      fp"${source_path}/vendor",
      """0x1234
""",
    )?
    root.write(
      fp"${source_path}/device",
      """0xabcd
""",
    )?
    root.write(
      fp"${source_path}/subsystem_vendor",
      """0x1234
""",
    )?
    root.write(
      fp"${source_path}/subsystem_device",
      """0x0001
""",
    )?
    root.write(
      fp"${source_path}/class",
      """0x020000
""",
    )?
    root.write(
      fp"${source_path}/revision",
      """0x01
""",
    )?
  }

  root.symlink(../../../devices/pci0000:01/0000:01:02.0, p"sys/bus/pci/devices/0000:01:02.0")?
  root.symlink(../../../devices/pci0000:01/0000:01:02.1, p"sys/bus/pci/devices/0000:01:02.1")?
  root.write(
    fp"${first_path}/current_link_speed",
    """8.0 GT/s PCIe
""",
  )?
  root.write(
    fp"${first_path}/current_link_width",
    """8
""",
  )?
  root.write(
    fp"${first_path}/max_link_speed",
    """16.0 GT/s PCIe
""",
  )?
  root.write(
    fp"${first_path}/max_link_width",
    """16
""",
  )?
  root.write(
    fp"${first_path}/numa_node",
    """-1
""",
  )?

  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let collection = collectors.collect_pci(root)
  test.eq(collection.functions.len(), 2)?
  test.eq(collection.functions[0].address, "0000:01:02.0")?
  test.eq(collection.functions[0].function, 0)?
  test.eq(collection.functions[1].address, "0000:01:02.1")?
  test.eq(collection.functions[1].function, 1)?
  test.eq(collection.functions[0].current_link_speed, "8.0 GT/s PCIe")?
  test.eq(collection.functions[0].current_link_width, 8)?
  test.eq(collection.functions[0].maximum_link_speed, "16.0 GT/s PCIe")?
  test.eq(collection.functions[0].maximum_link_width, 16)?
  test.eq(collection.functions[0].numa_node, null)?
  test.ok(! (collection.issues |> any .field == "functions.0000:01:02.0.numa_node"))?
  test.eq(collection.functions[1].current_link_speed, null)?
  test.eq(collection.functions[1].current_link_width, null)?
  test.eq(collection.functions[1].maximum_link_speed, null)?
  test.eq(collection.functions[1].maximum_link_width, null)?
  test.ok(! (collection.issues |> any .field.starts_with("functions.0000:01:02.1.current_link")))?
  test.ok(! (collection.issues |> any .field.starts_with("functions.0000:01:02.1.maximum_link")))?

  root.write(
    fp"${second_path}/current_link_width",
    """invalid
""",
  )?
  let malformed = collectors.collect_pci(root)
  test.ok(malformed.status.state == report_model.Partial)?
  test.ok(
    malformed.issues |> any .field == "functions.0000:01:02.1.current_link_width" and .state == report_model.Malformed,
  )?
  test.eq(malformed.functions[1].current_link_width, null)?
  root.write(
    fp"${second_path}/current_link_width",
    """0x8
""",
  )?
  let radix = collectors.collect_pci(root)
  test.ok(
    radix.issues |> any .field == "functions.0000:01:02.1.current_link_width" and .state == report_model.Malformed,
  )?
  test.eq(radix.functions[1].current_link_width, null)?
  root.write(fp"${first_path}/driver", "not-a-symlink")?
  root.write(fp"${first_path}/iommu_group", "not-a-symlink")?
  let failed_links = collectors.collect_pci(root)
  test.ok(failed_links.issues |> any .field == "functions.0000:01:02.0.driver" and .state == report_model.ReadFailure)?
  test.ok(
    failed_links.issues |> any .field == "functions.0000:01:02.0.iommu_group" and .state == report_model.ReadFailure,
  )?
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"${padding}${padding}"
  }

  root.write(
    fp"${first_path}/current_link_speed",
    f"""8.0 GT/s PCIe
${padding}""",
  )?
  root.write(
    fp"${first_path}/max_link_speed",
    f"""16.0 GT/s PCIe
${padding}""",
  )?
  let truncated_links = collectors.collect_pci(root)
  test.eq(truncated_links.functions[0].current_link_speed, null)?
  test.eq(truncated_links.functions[0].maximum_link_speed, null)?
  test.ok(
    truncated_links.issues |> any .field == "functions.0000:01:02.0.current_link_speed" and .state == report_model.Truncated,
  )?
  test.ok(
    truncated_links.issues |> any .field == "functions.0000:01:02.0.maximum_link_speed" and .state == report_model.Truncated,
  )?
}

test test_system_report_usb_controller_path_handles_pci_and_platform_roots [fs, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  test.eq(collector.usb_parent_address(../../../devices/pci0000:00/0000:00:08.1/0000:04:00.4/usb4/4-2), "0000:04:00.4")?
  test.ok(collector.usb_parent_address(../../../devices/platform/soc/usb1/1-2) == null)?
}

test test_system_report_usb_parent_join_handles_root_hubs_sorted_last [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let decoded = model.decode_report_json(json.encode(json_report_fixture())?)?
  let source = decoded.usb.devices[0].require(report_model.UsbDevice)?
  let child = {...source, sysfs_name: "4-2", bus_number: 4, parent_device_index: null, is_root_hub: false}
  let grandchild = {...source, sysfs_name: "4-2.3", bus_number: 4, parent_device_index: null, is_root_hub: false}
  let root_hub = {...source, sysfs_name: "usb4", bus_number: 4, parent_device_index: null, is_root_hub: true}
  let linked = collector.link_usb_parents([child, grandchild, root_hub])
  test.eq(linked[0].vendor_id, linked[1].vendor_id)?
  test.eq(linked[0].product_id, linked[1].product_id)?
  test.ok(linked[0].sysfs_name != linked[1].sysfs_name)?
  test.eq(linked[0].parent_device_index, 2)?
  test.eq(linked[1].parent_device_index, 0)?
  test.ok(linked[2].parent_device_index == null)?
}

test test_system_report_usb_keeps_a_device_with_missing_numeric_identity [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/bus/usb/devices/1-2", parents: true)?
  root.mkdir(p"sys/bus/usb/devices/1-3", parents: true)?
  root.mkdir(p"sys/bus/usb/devices/1-4", parents: true)?
  root.write(
    p"sys/bus/usb/devices/1-2/idVendor",
    """1234
""",
  )?
  root.write(
    p"sys/bus/usb/devices/1-2/idProduct",
    """5678
""",
  )?
  root.write(
    p"sys/bus/usb/devices/1-2/busnum",
    """1
""",
  )?
  root.write(
    p"sys/bus/usb/devices/1-3/idProduct",
    """5678
""",
  )?
  root.write(
    p"sys/bus/usb/devices/1-3/busnum",
    """1
""",
  )?
  root.write(
    p"sys/bus/usb/devices/1-4/idVendor",
    """zzzz
""",
  )?
  root.write(
    p"sys/bus/usb/devices/1-4/idProduct",
    """5678
""",
  )?
  root.write(
    p"sys/bus/usb/devices/1-4/busnum",
    """1
""",
  )?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "usb", true)?
  test.eq(value.usb.devices.len(), 3)?
  test.eq(value.usb.devices[0].vendor_id, 4660)?
  test.ok(value.usb.devices[1].vendor_id == null)?
  let missing_vendor = value.issues |> where .field == "devices.1-3.vendor_id"
  test.eq(missing_vendor.len(), 1)?
  test.eq(missing_vendor[0].state, report_model.Absent)?
  test.ok(value.usb.devices[2].vendor_id == null)?
  let malformed_vendor = value.issues |> where .field == "devices.1-4.vendor_id"
  test.eq(malformed_vendor.len(), 1)?
  test.eq(malformed_vendor[0].state, report_model.Malformed)?
}

test test_system_report_usb_power_read_failures_make_section_partial [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/bus/usb/devices/1-2/power/control", parents: true)?
  root.mkdir(p"sys/bus/usb/devices/1-2/power/autosuspend_delay_ms", parents: true)?
  root.mkdir(p"sys/bus/usb/devices/1-2/power/runtime_status", parents: true)?
  root.write(
    p"sys/bus/usb/devices/1-2/idVendor",
    """1234
""",
  )?
  root.write(
    p"sys/bus/usb/devices/1-2/idProduct",
    """5678
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "usb", true)?
  test.eq(value.usb.devices.len(), 1)?
  test.eq(value.usb.devices[0].power_control, null)?
  test.eq(value.usb.devices[0].autosuspend_delay_ms, null)?
  test.eq(value.usb.devices[0].runtime_status, null)?
  test.eq(value.usb.status.state, report_model.Partial)?
  for field in ["devices.1-2.power_control", "devices.1-2.autosuspend_delay_ms", "devices.1-2.runtime_status"] {
    let matches = value.issues |> where .section == "usb" and .field == field
    test.eq(matches.len(), 1)?
    test.eq(matches[0].state, report_model.ReadFailure)?
  }
}

test test_system_report_usb_identity_read_failures_keep_field_issues [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/bus/usb/devices/1-2/bDeviceClass", parents: true)?
  root.mkdir(p"sys/bus/usb/devices/1-2/manufacturer", parents: true)?
  root.write(
    p"sys/bus/usb/devices/1-2/idVendor",
    """1234
""",
  )?
  root.write(
    p"sys/bus/usb/devices/1-2/idProduct",
    """5678
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "usb", true)?
  test.eq(value.usb.devices.len(), 1)?
  test.eq(value.usb.devices[0].class_code, null)?
  test.eq(value.usb.devices[0].manufacturer.value, null)?
  test.eq(value.usb.status.state, report_model.Partial)?
  for field in ["devices.1-2.class_code", "devices.1-2.manufacturer"] {
    let matches = value.issues |> where .section == "usb" and .field == field
    test.eq(matches.len(), 1)?
    test.eq(matches[0].state, report_model.ReadFailure)?
  }

  root.remove(p"sys/bus/usb/devices/1-2/bDeviceClass", dir: true)?
  root.write(
    p"sys/bus/usb/devices/1-2/bDeviceClass",
    """9
""",
  )?
  let malformed = collector.collect_from_root(root, "fixture-arch", 4096, 100, "usb", true)?
  test.eq(malformed.usb.devices[0].class_code, null)?
  test.ok(
    malformed.issues |> any .section == "usb" and .field == "devices.1-2.class_code" and .state == report_model.Malformed,
  )?
  root.write(
    p"sys/bus/usb/devices/1-2/busnum",
    """invalid
""",
  )?
  let invalid_bus = collector.collect_from_root(root, "fixture-arch", 4096, 100, "usb", true)?
  test.eq(invalid_bus.usb.devices[0].bus_number, null)?
  test.ok(
    invalid_bus.issues |> any .section == "usb" and .field == "devices.1-2.bus_number" and .state == report_model.Malformed,
  )?
  root.write(
    p"sys/bus/usb/devices/1-2/busnum",
    """9007199254740992
""",
  )?
  let unsafe_bus = collector.collect_from_root(root, "fixture-arch", 4096, 100, "usb", true)?
  test.eq(unsafe_bus.usb.devices[0].bus_number, null)?
  test.ok(
    unsafe_bus.issues |> any .section == "usb" and .field == "devices.1-2.bus_number" and .state == report_model.Malformed,
  )?
}

test test_system_report_usb_truncated_scalar_sources_do_not_publish_prefixes [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  let device = p"sys/bus/usb/devices/1-2"
  root.mkdir(device, parents: true)?
  root.mkdir(p"sys/bus/usb/devices/1-2/power", parents: true)?
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"${padding}${padding}"
  }

  for field in [
    {
      name: "idVendor",
      prefix: "1234",
    },
    {
      name: "busnum",
      prefix: "1",
    },
    {
      name: "speed",
      prefix: "480",
    },
    {
      name: "power/control",
      prefix: "auto",
    },
    {
      name: "power/runtime_status",
      prefix: "active",
    },
  ] {
    root.write(
      fp"${device}/${field.name}",
      f"""${field.prefix}
${padding}""",
    )?
  }

  root.write(
    p"sys/bus/usb/devices/1-2/idProduct",
    """5678
""",
  )?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "usb", true)?
  test.eq(value.usb.devices.len(), 1)?
  let observed = value.usb.devices[0]
  test.eq(observed.vendor_id, null)?
  test.eq(observed.product_id, 22136)?
  test.eq(observed.bus_number, null)?
  test.eq(observed.speed_mbps, null)?
  test.eq(observed.power_control, null)?
  test.eq(observed.runtime_status, null)?
  for field in ["vendor_id", "bus_number", "speed_mbps", "power_control", "runtime_status"] {
    test.ok(
      value.issues |> any .section == "usb" and .field == f"devices.1-2.${field}" and .state == report_model.Truncated,
    )?
  }
}

test test_system_report_block_class_path_identifies_its_pci_controller [fs, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  test.eq(
    collector.pci_address_in_target(../../devices/pci0000:00/0000:00:01.2/0000:01:00.0/nvme/nvme0/nvme0n1),
    "0000:01:00.0",
  )?
  test.ok(collector.pci_address_in_target(../../devices/virtual/block/loop0) == null)?
}

test test_system_report_assembles_network_links_addresses_routes_and_rules [fs, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let dump: LinuxNetworkDump = {
    state: "partial",
    enumeration_succeeded: true,
    links: [
      {
        ifindex: 2,
        name: "eth0",
        name_bytes: b"eth0\0",
        hardware_type: 1,
        flags: 65539,
        mtu: 1500,
        address: b"\x02\0\0\0\0\x01",
        broadcast: null,
        master_ifindex: null,
        lower_ifindex: null,
        operstate: 6,
        kind: "veth",
        rx_bytes: 4096,
        tx_bytes: 2048,
        attributes: [
          {
            kind: 32769,
            data: b"\x02\0",
          },
        ],
      },
      {
        ifindex: 3,
        name: "eth0.42",
        name_bytes: b"eth0.42\0",
        hardware_type: 1,
        flags: 69635,
        mtu: 1500,
        address: b"\x02\0\0\0\0\x02",
        broadcast: null,
        master_ifindex: null,
        lower_ifindex: 2,
        operstate: 6,
        kind: "vlan",
        rx_bytes: null,
        tx_bytes: null,
        attributes: [],
      },
    ],
    addresses: [
      {
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
      },
      {
        ifindex: 2,
        family: "inet6",
        prefix_length: 64,
        scope: 0,
        flags: 1,
        address: "2001:db8::10",
        local: null,
        broadcast: null,
        label: null,
        preferred_lifetime_seconds: 120,
        valid_lifetime_seconds: 240,
        attributes: [],
      },
      {
        ifindex: 3,
        family: "inet6",
        prefix_length: 64,
        scope: 0,
        flags: 0,
        address: "2001:db8:42::5",
        local: null,
        broadcast: null,
        label: null,
        preferred_lifetime_seconds: null,
        valid_lifetime_seconds: null,
        attributes: [],
      },
    ],
    routes: [
      {
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
        attributes: [
          {
            kind: 16389,
            data: b"\x04",
          },
        ],
      },
      {
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
        nexthops: [
          {
            ifindex: 9,
            flags: 2,
            hops: 3,
            gateway: "2001:db8::1",
          },
        ],
        attributes: [],
      },
    ],
    rules: [
      {
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
      },
      {
        family: "inet6",
        destination_prefix_length: 0,
        source_prefix_length: 64,
        destination: null,
        source: "2001:db8::",
        input_name: "eth0.42",
        output_name: "eth0",
        priority: 124,
        table: 1000,
        fwmark: 7,
        fwmask: 255,
        action: 1,
        flags: 4,
        attributes: [],
      },
      {
        family: "inet",
        destination_prefix_length: 0,
        source_prefix_length: 0,
        destination: null,
        source: null,
        input_name: null,
        output_name: null,
        priority: 150,
        table: 0,
        fwmark: null,
        fwmask: null,
        action: 2,
        flags: 0,
        attributes: [
          {
            kind: 4,
            data: b"{\0\0\0",
          },
        ],
      },
    ],
    issues: [
      {
        object: "address",
        message: "address attribute has an unsupported size",
        state: "malformed",
        errno: null,
        error_kind: "malformed",
      },
      {
        object: "links.2.rx_bytes",
        message: "network counter exceeds the exact JSON integer range",
        state: "range_failure",
        errno: null,
        error_kind: "integer_out_of_range",
      },
    ],
  }

  let result = collector.assemble_network_dump(dump)
  test.eq(result.status.state, report_model.Partial)?
  test.eq(result.status.enumeration_succeeded, true)?
  test.eq(result.links[0].name.value, "eth0")?
  test.eq(result.links[0].mac.value, "02:00:00:00:00:01")?
  test.ok("lower_up" in result.links[0].flags)?
  test.eq(result.links[0].counters[0].value, 4096)?
  test.eq(result.links[0].addresses[0].address.value, "192.0.2.10")?
  test.eq(result.links[0].addresses[0].family, "ipv4")?
  test.eq(result.links[0].addresses[1].family, "ipv6")?
  test.eq(result.links[0].addresses[1].address.value, "2001:db8::10")?
  test.eq(result.links[0].addresses[1].prefix_length, 64)?
  test.eq(result.links[0].addresses[1].preferred_lifetime_seconds, 120)?
  test.eq(result.links[0].addresses[1].valid_lifetime_seconds, 240)?
  test.eq(result.links[0].addresses.len(), 2)?
  test.eq(result.links[1].name.value, "eth0.42")?
  test.eq(result.links[1].kind, "vlan")?
  test.eq(result.links[1].lower_ifindex, 2)?
  test.eq(result.links[1].addresses.len(), 1)?
  test.eq(result.links[1].addresses[0].address.value, "2001:db8:42::5")?
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
  test.eq(result.rules[1].family, "ipv6")?
  test.eq(result.rules[1].source.value, "2001:db8::")?
  test.eq(result.rules[1].source_prefix_length, 64)?
  test.eq(result.rules[1].input_ifindex, 3)?
  test.eq(result.rules[1].output_ifindex, 2)?
  test.eq(result.rules[1].table, 1000)?
  test.eq(result.rules[1].fwmark, 7)?
  test.eq(result.rules[1].fwmask, 255)?
  test.eq(result.rules[1].action, "to_table")?
  test.eq(result.rules[2].action, "goto")?
  test.eq(result.rules[2].attributes[0].kind, 4)?
  test.eq(result.rules[2].attributes[0].data.value, "ewAAAA==")?
  test.eq(result.issues[0].state, report_model.Malformed)?
  test.eq(result.issues[1].field, "netlink.links.2.rx_bytes")?
  test.eq(result.issues[1].state, report_model.RangeFailure)?

  let denied = collector.assemble_network_dump({
    ...dump,
    state: "permission_denied",
    enumeration_succeeded: false,
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
  test.eq(denied.status.state, report_model.SectionPermissionDenied)?
  test.eq(denied.status.enumeration_succeeded, false)?
  test.eq(denied.issues[0].errno, 13)?
}

test test_system_report_section_selection_marks_excluded_domains [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let source = json_report_fixture()
  let report = model.decode_report_json(json.encode(source)?)?
  let cpu_only = model.select_report_section(report, "cpu")?
  let cpu_only_wire = json.decode(model.encode_report_json(cpu_only, true, false)?)?
  let cpu_only_text = model.render_text(cpu_only, true, false)?

  test.eq(cpu_only.identity.hostname.value, "workstation-name")?
  test.eq(cpu_only_wire.cpu.status.state, "complete")?
  test.eq(cpu_only_wire.memory.status.state, "not_requested")?
  test.ok(! cpu_only_wire.memory.status.enumeration_succeeded)?
  test.eq(cpu_only_wire.memory.host.total_bytes, null)?
  test.eq(cpu_only_wire.pci.status.state, "not_requested")?
  test.eq(cpu_only_wire.pci.functions.len(), 0)?
  test.eq(cpu_only_wire.network.status.state, "not_requested")?
  test.eq(cpu_only.issues.len(), 1)?
  test.contains(cpu_only_text, "PCI: not requested")?
  test.ok("PCI functions:" not in cpu_only_text)?

  let usb_only = model.select_report_section(report, "usb")?
  test.eq(usb_only.pci.status.state, report_model.Complete)?
  test.eq(usb_only.usb.status.state, report_model.Complete)?
  test.eq(usb_only.storage.status.state, report_model.SectionNotRequested)?
  test.eq(usb_only.network.status.state, report_model.SectionNotRequested)?

  let network_only = model.select_report_section(report, "network")?
  test.eq(network_only.pci.status.state, report_model.Complete)?
  test.eq(network_only.usb.status.state, report_model.Complete)?
  test.eq(network_only.network.status.state, report_model.Complete)?
  test.eq(network_only.storage.status.state, report_model.SectionNotRequested)?

  let sensors_only = model.select_report_section(report, "sensors")?
  test.eq(sensors_only.pci.status.state, report_model.Complete)?
  test.eq(sensors_only.usb.status.state, report_model.Complete)?
  test.eq(sensors_only.sensors.status.state, report_model.Complete)?
  test.eq(sensors_only.devices.status.state, report_model.SectionNotRequested)?

  let processes_only = model.select_report_section(report, "processes")?
  test.eq(processes_only.processes.status.state, report_model.Complete)?
  test.eq(processes_only.processes.processes[0].cgroup.value, "/user.slice/private")?
  test.eq(processes_only.processes.processes[0].cgroup_resource_index, null)?
  test.eq(processes_only.memory.status.state, report_model.SectionNotRequested)?

  test.error_kind(model.select_report_section(report, "hardware"), "SystemReportError.InvalidSection")?
}

test test_system_report_json_round_trip_and_redaction [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let source = json_report_fixture()
  let encoded_source = json.encode(source)?
  let decoded = model.decode_report_json(encoded_source)?
  let decoded_wire = json.decode(model.encode_report_json(decoded, true, false)?)?

  test.eq(decoded.schema_version, 1)?
  test.eq(decoded_wire.source_mode, "synthetic_fixture")?
  test.eq(decoded.identity.hostname.value, "workstation-name")?
  test.eq(decoded_wire.identity.kernel_build, "Linux version 6.12-test (builder@private-build-host)")?
  test.ok(decoded.pci.status.enumeration_succeeded)?
  test.eq(decoded.pci.functions.len(), 2)?

  let safe_json = model.encode_report_json(decoded, false, false)?
  let safe = model.decode_report_json(safe_json)?
  let safe_wire = json.decode(safe_json)?
  let safe_text = model.render_text(decoded, true, false)?
  test.ok(safe.redacted)?
  test.eq(safe_wire.identity.status.state, "complete")?
  test.eq(safe_wire.identity.hostname.state, "redacted")?
  test.ok(safe_wire.identity.kernel_build == null)?
  test.ok("private-build-host" not in safe_json)?
  test.ok("private-build-host" not in safe_text)?
  test.ok(safe.issues |> any .section == "identity" and .field == "kernel_build" and .state == report_model.Redacted)?
  test.eq(model.encode_report_json(safe, false, false)?, safe_json)?
  test.eq(safe.identity.hostname.value, null)?
  test.eq(safe_wire.scope.network_namespace.state, "redacted")?
  test.eq(safe_wire.scope.uts_namespace.state, "redacted")?
  test.eq(safe_wire.scope.ipc_namespace.state, "redacted")?
  test.eq(safe_wire.scope.user_namespace.state, "redacted")?
  test.eq(safe_wire.scope.time_namespace.state, "redacted")?
  test.eq(safe_wire.scope.source_roots, ["redacted", "redacted"])?
  test.eq(safe.cpu.online, [0, 1, 2])?
  test.eq(safe_wire.pci.functions[0].address, null)?
  test.eq(safe_wire.pci.functions[0].domain, null)?
  test.eq(safe_wire.pci.functions[0].vendor_id, 32902)?
  test.eq(safe_wire.pci.functions[1].parent_function_index, 0)?
  test.ok("0000:00:1f.6" not in safe_json)?
  test.ok("0000:00:1f.6" not in safe_text)?
  test.eq(safe_wire.issues[1].field, "functions.redacted.vendor_id")?
  test.ok("private PCI source path" not in safe_json)?
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
    test.eq(safe_wire.usb.devices[0].interfaces[0].alternate_settings[0].configuration_value, 1)?
    test.eq(safe_wire.usb.devices[0].interfaces[0].alternate_settings[1].configuration_value, 2)?
    test.eq(safe_wire.usb.devices[0].serial.state, "redacted")?
    test.ok("1-2.3" not in safe_json)?
    test.ok("1-2.3" not in safe_text)?
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
    test.ok("nvme0n1" not in safe_json)?
    test.ok("nvme0n1" not in safe_text)?
  } else {
    test.fail("block device fixture did not round-trip")?
  }

  let mount = safe.storage.mounts[0]
  if mount != null {
    test.eq(safe_wire.storage.mounts[0].major, null)?
    test.eq(safe_wire.storage.mounts[0].minor, null)?
    test.eq(safe_wire.storage.mounts[0].block_device_index, 0)?
    test.eq(safe_wire.storage.mounts[0].target.state, "redacted")?
    test.eq(safe_wire.storage.mounts[0].mount_options, ["rw", "relatime", "redacted"])?
    test.eq(safe_wire.storage.mounts[0].optional_fields, ["shared:42", "redacted"])?
    test.eq(safe_wire.storage.mounts[0].super_options, ["rw", "redacted", "redacted", "redacted"])?
    test.ok("private-mount-label" not in safe_json)?
    test.ok("private-mount-field" not in safe_json)?
    test.ok("/private/host/snapshot" not in safe_json)?
    test.ok("mount-secret" not in safe_json)?
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

  test.eq(safe_wire.sensors.channels[0].label.state, "redacted")?
  test.ok("private-sensor-label" not in safe_json)?
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

  let process_item = safe.processes.processes[0]
  if process_item != null {
    test.eq(process_item.command.value, "worker")?
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
  test.ok("/private/host/snapshot" in sensitive_json)?
  test.ok("mount-secret" not in sensitive_json)?
  test.ok("private-mount-field" not in sensitive_json)?
  test.eq(json.decode(sensitive_json)?.storage.mounts[0].super_options[2], "redacted")?
  test.eq(json.decode(sensitive_json)?.storage.mounts[0].super_options[3], "redacted")?
  test.eq(json.decode(sensitive_json)?.storage.mounts[0].optional_fields, ["shared:42", "redacted"])?
  test.ok("private-sensor-label" in sensitive_json)?
  let sensitive = model.decode_report_json(sensitive_json)?
  test.ok(! sensitive.redacted)?
  test.eq(sensitive.identity.hostname.value, "workstation-name")?
  test.eq(sensitive.pci.functions[0].address, "0000:00:1f.6")?
  test.eq(sensitive.usb.devices[0].sysfs_name, "1-2.3")?
  test.eq(sensitive.storage.devices[0].name, "nvme0n1")?
  test.eq(sensitive.network.links[0].attributes[0].data.value, "eA==")?
  test.eq(sensitive.network.routes[0].nexthops[0].gateway.value, "192.0.2.1")?

  let route_text = model.render_text(sensitive, true, true)?
  test.contains(route_text, "nexthop ifindex=2 flags=1 hops=0 gateway=\"192.0.2.1\"")?

  let unsupported_version = json.encode({...source, schema_version: 2})?
  test.error_kind(model.decode_report_json(unsupported_version), "SystemReportError.UnsupportedSchema")?

  let unknown_state = json.encode({...source, source_mode: "unknown-mode"})?
  test.error_kind(model.decode_report_json(unknown_state), "SystemReportError.InvalidJson")?

  let invalid_cpu_status = {...source.cpu.status, enumeration_succeeded: false}
  let invalid_cpu = {...source.cpu, status: invalid_cpu_status}
  let invalid_section = {...source, cpu: invalid_cpu}
  test.error_kind(model.decode_report_json(json.encode(invalid_section)?), "SystemReportError.InvalidJson")?
}

test test_system_report_thermal_trip_indexes_round_trip_and_legacy_unknown [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let encoded_fixture = json.encode(json_report_fixture())?
  let source = json.decode(encoded_fixture)?
  let zone = {
    id: 3,
    kind: "fixture",
    temperature_millidegrees: 41000,
    parent_device_class_index: null,
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
  }
  let indexed = json.set(source, ["sensors", "thermal_zones"], [zone])?
  let decoded = model.decode_report_json(json.encode(indexed)?)?
  test.eq(decoded.sensors.thermal_zones[0].trips |> map .index, [0, 2])?
  let encoded = model.encode_report_json(decoded, true, false)?
  test.eq(json.get(json.decode(encoded)?, ["sensors", "thermal_zones", 0, "trips", 1, "index"])?.require(Int)?, 2)?
  test.contains(model.render_text(decoded, true, false)?, "trip 2 \"passive\"")?
  let legacy_zone = {
    ...zone,
    trips: [
      {
        kind: "critical",
        temperature_millidegrees: 95000,
        hysteresis_millidegrees: 2000,
      },
    ],
  }
  let legacy = model.decode_report_json(json.encode(json.set(source, ["sensors", "thermal_zones"], [legacy_zone])?)?)?
  test.eq(legacy.sensors.thermal_zones[0].trips[0].index, null)?
  let duplicate = {...zone, trips: [zone.trips[0], {...zone.trips[1], index: 0}]}
  test.error_kind(
    model.decode_report_json(json.encode(json.set(source, ["sensors", "thermal_zones"], [duplicate])?)?),
    "SystemReportError.InvalidJson",
  )?
  let mixed = json.set(indexed, ["sensors", "thermal_zones", 0, "trips", 1, "index"], null)?
  test.error_kind(model.decode_report_json(json.encode(mixed)?), "SystemReportError.InvalidJson")?
  let negative = json.set(indexed, ["sensors", "thermal_zones", 0, "trips", 0, "index"], -1)?
  test.error_kind(model.decode_report_json(json.encode(negative)?), "SystemReportError.InvalidJson")?
}

test test_system_report_cpufreq_scaling_current_replays_legacy_requested_name [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let encoded_fixture = json.encode(json_report_fixture())?
  let source = json.decode(encoded_fixture)?
  let decoded = model.decode_report_json(json.encode(source)?)?
  test.eq(decoded.cpu.frequency_policies[0].scaling_current_khz, 1800000)?
  let saved = json.decode(model.encode_report_json(decoded, true, false)?)?
  test.eq(json.get(saved, ["cpu", "frequency_policies", 0, "scaling_current_khz"])?.require(Int)?, 1800000)?
  test.ok(json.get(saved, ["cpu", "frequency_policies", 0, "requested_current_khz"], null) == null)?
  var legacy = json.remove(source, ["cpu", "frequency_policies", 0, "scaling_current_khz"])?
  legacy = json.set(legacy, ["cpu", "frequency_policies", 0, "requested_current_khz"], 1800000)?
  test.eq(model.decode_report_json(json.encode(legacy)?)?.cpu.frequency_policies[0].scaling_current_khz, 1800000)?
  let redundant = json.set(source, ["cpu", "frequency_policies", 0, "requested_current_khz"], 1800000)?
  test.eq(model.decode_report_json(json.encode(redundant)?)?.cpu.frequency_policies[0].scaling_current_khz, 1800000)?
  let legacy_absent = json.set(
    json.remove(source, ["cpu", "frequency_policies", 0, "scaling_current_khz"])?,
    ["cpu", "frequency_policies", 0, "requested_current_khz"],
    null,
  )?
  test.eq(model.decode_report_json(json.encode(legacy_absent)?)?.cpu.frequency_policies[0].scaling_current_khz, null)?
  let conflicting = json.set(source, ["cpu", "frequency_policies", 0, "requested_current_khz"], 1700000)?
  test.error_kind(model.decode_report_json(json.encode(conflicting)?), "SystemReportError.InvalidJson")?
}

test test_system_report_usb_runtime_status_replays_legacy_absence [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let encoded_fixture = json.encode(json_report_fixture())?
  let source = json.decode(encoded_fixture)?
  let decoded = model.decode_report_json(json.encode(source)?)?
  test.eq(decoded.usb.devices[0].runtime_status, "active")?
  let old = json.remove(source, ["usb", "devices", 0, "runtime_status"])?
  test.eq(model.decode_report_json(json.encode(old)?)?.usb.devices[0].runtime_status, null)?
  let malformed = json.set(source, ["usb", "devices", 0, "runtime_status"], 7)?
  test.error_kind(model.decode_report_json(json.encode(malformed)?), "SystemReportError.InvalidJson")?
}

test test_system_report_v1_replay_marks_unrecorded_namespaces_unsupported [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let encoded_fixture = json.encode(json_report_fixture())?
  var old = json.decode(encoded_fixture)?
  for field in ["uts_namespace", "ipc_namespace", "user_namespace", "time_namespace"] {
    old = json.remove(old, ["scope", field])?
  }

  let restored = model.decode_report_json(json.encode(old)?)?
  test.eq(restored.scope.uts_namespace.state, report_model.Unsupported)?
  test.eq(restored.scope.ipc_namespace.state, report_model.Unsupported)?
  test.eq(restored.scope.user_namespace.state, report_model.Unsupported)?
  test.eq(restored.scope.time_namespace.state, report_model.Unsupported)?

  let malformed = json.set(old, ["scope", "uts_namespace"], null)?
  test.error_kind(model.decode_report_json(json.encode(malformed)?), "SystemReportError.InvalidJson")?
}

test test_system_report_v1_replay_keeps_unrecorded_idle_state_index_unknown [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let legacy_state = {
    cpu_id: 0,
    name: "C1",
    description: null,
    disable_setting: 0,
    latency_us: 1,
    residency_us: 2,
    usage_count: 3,
    time_us: 4,
  }
  let encoded_fixture = json.encode(json_report_fixture())?
  let old = json.set(json.decode(encoded_fixture)?, ["cpu", "idle_states"], [legacy_state])?
  let restored = model.decode_report_json(json.encode(old)?)?
  test.eq(restored.cpu.idle_states.len(), 1)?
  test.eq(restored.cpu.idle_states[0].state_index, null)?
  let current = json.decode(model.encode_report_json(restored, true, false)?)?
  test.eq(json.get(current, ["cpu", "idle_states", 0, "state_index"])?, null)?
  let invalid = json.set(old, ["cpu", "idle_states", 0, "state_index"], -1)?
  test.error_kind(model.decode_report_json(json.encode(invalid)?), "SystemReportError.InvalidJson")?
  let indexed_state = {...legacy_state, state_index: 0}
  let duplicate = json.set(old, ["cpu", "idle_states"], [indexed_state, indexed_state])?
  test.error_kind(model.decode_report_json(json.encode(duplicate)?), "SystemReportError.InvalidJson")?
}

test test_system_report_v1_replay_restores_legacy_powercap_constraint [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let legacy_zone = {
    name: "package-0",
    parent: null,
    energy_uj: 123456,
    maximum_energy_range_uj: 999999,
    constraint_name: "long_term",
    power_limit_uw: 45000000,
    time_window_us: 1000000,
  }
  let encoded_fixture = json.encode(json_report_fixture())?
  let old = json.set(json.decode(encoded_fixture)?, ["power", "cap_zones"], [legacy_zone])?
  let restored = model.decode_report_json(json.encode(old)?)?
  test.eq(restored.power.cap_zones.len(), 1)?
  test.eq(restored.power.cap_zones[0].entry_name, "package-0")?
  test.eq(restored.power.cap_zones[0].constraints.len(), 1)?
  test.eq(restored.power.cap_zones[0].constraints[0].index, 0)?
  test.eq(restored.power.cap_zones[0].constraints[0].power_limit_uw, 45000000)?
  let wire = json.decode(model.encode_report_json(restored, true, false)?)?
  test.eq(json.get(wire, ["power", "cap_zones", 0, "constraints", 0, "power_limit_uw"])?, 45000000)?
  test.error_kind(json.get(wire, ["power", "cap_zones", 0, "power_limit_uw"]), "json-path")?

  let invalid = json.set(old, ["power", "cap_zones", 0, "power_limit_uw"], "invalid")?
  test.error_kind(model.decode_report_json(json.encode(invalid)?), "SystemReportError.InvalidJson")?
}

test test_system_report_powercap_constraints_round_trip_and_render [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let zone = {
    entry_name: "intel-rapl:0",
    name: "package-0",
    parent: null,
    energy_uj: 123456,
    maximum_energy_range_uj: 999999,
    constraints: [
      {
        index: 0,
        name: "long_term",
        power_limit_uw: 45000000,
        time_window_us: 1000000,
      },
      {
        index: 1,
        name: "short_term",
        power_limit_uw: 65000000,
        time_window_us: 250000,
      },
    ],
  }
  let encoded_fixture = json.encode(json_report_fixture())?
  let source = json.set(json.decode(encoded_fixture)?, ["power", "cap_zones"], [zone])?
  let decoded = model.decode_report_json(json.encode(source)?)?
  test.eq(decoded.power.cap_zones[0].entry_name, "intel-rapl:0")?
  test.eq(decoded.power.cap_zones[0].constraints.len(), 2)?
  let text = model.render_text(decoded, true, true)?
  test.contains(text, "constraint 0 \"long_term\" limit=45000000 uW window=1000000 us")?
  test.contains(text, "constraint 1 \"short_term\" limit=65000000 uW window=250000 us")?
  let encoded = model.encode_report_json(decoded, true, false)?
  let restored = model.decode_report_json(encoded)?
  test.eq(restored.power.cap_zones[0].constraints, decoded.power.cap_zones[0].constraints)?
  let duplicate = json.set(source, ["power", "cap_zones", 0, "constraints", 1, "index"], 0)?
  test.error_kind(model.decode_report_json(json.encode(duplicate)?), "SystemReportError.InvalidJson")?
}

test test_system_report_replay_withholds_mount_credentials_in_sensitive_json [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let raw = json.set(
    json_report_fixture(),
    ["storage", "mounts", 0, "source"],
    json_observation("observed", "smb://user:private-secret@host/share"),
  )?
  let decoded = model.decode_report_json(json.encode(raw)?)?
  test.eq(decoded.storage.mounts[0].source.state, report_model.Redacted)?
  let sensitive_json = model.encode_report_json(decoded, true, false)?
  test.ok("private-secret" not in sensitive_json)?
  test.eq(json.decode(sensitive_json)?.storage.mounts[0].source.state, "redacted")?
}

test test_system_report_text_output_escapes_untrusted_controls [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let source = json_report_fixture()
  let hostile_scope = {
    ...source.scope,
    host_claim: "host\u{202e}name\u{1b}[31m",
  }
  let hostile_identity = {
    ...source.identity,
    kernel_release: """6.12
attack""",
    uptime_seconds: null,
  }
  let hostile_source = {...source, scope: hostile_scope, identity: hostile_identity}
  let decoded = model.decode_report_json(json.encode(hostile_source)?)?
  let rendered = model.render_text(decoded, true, false)?

  test.ok("\u{202e}" not in rendered)?
  test.ok("\u{1b}" not in rendered)?
  test.contains(rendered, "\\u{202e}")?
  test.contains(rendered, "6.12\\nattack")?
  test.ok(
    """6.12
attack""" not in rendered,
  )?
  test.contains(rendered, "does not guarantee anonymity")?
  test.contains(rendered, "Uptime: unknown seconds")?
}

test test_system_report_full_text_renders_numeric_relationship_lists [fs, error] {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let source = json_report_fixture()
  let cache = {
    id: 3,
    sysfs_index: 0,
    owner_cpu_id: 0,
    level: 2,
    kind: "Unified",
    size_bytes: 1048576,
    line_size_bytes: 64,
    sets: 1024,
    shared_cpus: [
      0,
      2,
    ],
  }
  let with_cache = json.set(source, ["cpu", "caches"], [cache])?
  let disk = json.get(source, ["storage", "devices", 0])?.require(Record)?
  let with_devices = json.set(
    with_cache,
    ["storage", "devices"],
    [
      {
        ...disk,
        holder_indices: [
          1,
        ],
      },
      {
        ...disk,
        name: "dm-0",
        major: 253,
        minor: 0,
        parent_device_index: 0,
        parent_pci_function_index: null,
        holder_indices: [],
        slave_indices: [
          0,
        ],
      },
    ],
  )?
  let report = model.decode_report_json(json.encode(with_devices)?)?
  let rendered = model.render_text(report, true, true)?
  test.contains(rendered, "shared CPUs 0,2")?
  test.contains(rendered, "holders=1 slaves=")?
  test.contains(rendered, "holders= slaves=0")?
}

test test_system_report_command_replays_saved_json_offline [fs, process, error] { |ctx|
  let report_path = test.temp_path(ctx, name: "system-report-v1.json")
  report_path.write(json.encode(json_report_fixture())?)?

  let projected = run.text ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/core/system-report.xsh" -- --from $report_path --section cpu --json ?
  let decoded = json.decode(projected)?
  test.eq(decoded.schema_version, 1)?
  test.eq(decoded.identity.hostname.state, "redacted")?
  test.eq(decoded.cpu.status.state, "complete")?
  test.eq(decoded.memory.status.state, "not_requested")?
  test.ok("workstation-name" not in projected)?
  let json_full = run.text ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/core/system-report.xsh" -- --from $report_path --section cpu --json --full ?
  test.eq(json_full, projected)?

  let sensitive = run.text ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/core/system-report.xsh" -- --from $report_path --sensitive --json ?
  test.ok("workstation-name" in sensitive)?
  let default_json = run.text ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/core/system-report.xsh" -- --from $report_path --json ?
  test.ok("/private/host/snapshot" not in default_json)?
  test.ok("mount-secret" not in default_json)?
  test.ok("private-sensor-label" not in default_json)?

  let overview = run.text ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/core/system-report.xsh" -- --from $report_path ?
  test.contains(overview, "XSH system report v1")?
  test.contains(overview, "2 identical policy group on CPUs 0,1")?
  test.contains(overview, "1 identical policy group on CPUs 2")?
  test.ok("3 identical policy group" not in overview)?

  let version = run.text ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/core/system-report.xsh" -- --version ?
  test.eq(
    version,
    """system-report schema v1
""",
  )?

  let help = run.text ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/core/system-report.xsh" -- --help ?
  test.contains(help, "--from FILE")?
  test.contains(help, "--section NAME")?
}

test test_system_report_command_rejects_malformed_replay [fs, process, error] { |ctx|
  let report_path = test.temp_file(ctx, name: "system-report-invalid.json", contents: b"{invalid")?
  let stderr = test.temp_path(ctx, name: "system-report-invalid.stderr")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/core/system-report.xsh" -- --from $report_path 2> $stderr
  test.ok(! status.exited_with(0))?
  test.contains(stderr.read_text()?, "invalid replay report")?

  let invalid_utf8 = test.temp_file(ctx, name: "system-report-invalid-utf8.json", contents: b"\xff")?
  let utf8_stderr = test.temp_path(ctx, name: "system-report-invalid-utf8.stderr")
  let utf8_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/core/system-report.xsh" -- --from $invalid_utf8 2> $utf8_stderr
  test.ok(! utf8_status.exited_with(0))?
  test.contains(utf8_stderr.read_text()?, "not valid UTF-8")?

  let unsupported_path = test.temp_path(ctx, name: "system-report-unsupported-schema.json")
  unsupported_path.write(json.encode({...json_report_fixture(), schema_version: 99})?)?
  let unsupported_stderr = test.temp_path(ctx, name: "system-report-unsupported-schema.stderr")
  let unsupported_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/core/system-report.xsh" -- --from $unsupported_path 2> $unsupported_stderr
  test.ok(! unsupported_status.exited_with(0))?
  test.contains(unsupported_stderr.read_text()?, "unsupported schema version")?

  let section_stderr = test.temp_path(ctx, name: "system-report-invalid-section.stderr")
  let section_status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir.parent()}/core/system-report.xsh" -- --section hardware 2> $section_stderr
  test.ok(! section_status.exited_with(0))?
  test.ok(section_stderr.read_text()?.trim() != "")?
}

test test_system_report_live_collection_uses_explicit_root_and_redacts_by_default [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/sys/kernel", parents: true)?
  root.mkdir(p"proc/sys/kernel/random", parents: true)?
  root.mkdir(p"etc", parents: true)?
  root.write(
    p"proc/sys/kernel/osrelease",
    """fixture-release
""",
  )?
  root.write(
    p"proc/version",
    """Linux fixture version 1
""",
  )?
  root.write(
    p"proc/sys/kernel/hostname",
    """private-fixture-host
""",
  )?
  root.write(
    p"proc/sys/kernel/random/boot_id",
    """private-fixture-boot-id
""",
  )?
  root.write(
    p"proc/uptime",
    """73.5 12.0
""",
  )?
  root.write(
    p"etc/os-release",
    """ID=fixture
NAME=Fixture OS
PRETTY_NAME="Fixture Operating System"
VERSION_ID=1
""",
  )?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let share_safe = collector.collect_from_root(root, "fixture-arch", 65536, 250, "identity")?
  test.eq(share_safe.source_mode, report_model.SyntheticFixture)?
  test.eq(share_safe.identity.kernel_release, "fixture-release")?
  test.eq(share_safe.identity.architecture, "fixture-arch")?
  test.eq(share_safe.identity.uptime_seconds, 73)?
  test.eq(share_safe.identity.hostname.state, report_model.Redacted)?
  test.eq(share_safe.identity.hostname.value, null)?
  test.eq(share_safe.cpu.status.state, report_model.SectionNotRequested)?
  test.eq(share_safe.redacted, true)?

  let sensitive = collector.collect_from_root(root, "fixture-arch", 65536, 250, "identity", true)?
  test.eq(sensitive.identity.hostname.value, "private-fixture-host")?
  test.eq(sensitive.identity.hostname.state, report_model.Observed)?
}

test test_system_report_cpu_collection_does_not_invent_absent_cpu_zero [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu/cpu2", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu4", parents: true)?
  root.write(p"sys/devices/system/cpu/possible", "0-4")?
  root.write(p"sys/devices/system/cpu/present", "2,4")?
  root.write(p"sys/devices/system/cpu/online", "2")?
  root.write(p"sys/devices/system/cpu/offline", "4")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let snapshot = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.ok(snapshot.cpu.status.enumeration_succeeded)?
  test.eq(snapshot.cpu.possible, [0, 1, 2, 3, 4])?
  test.eq(snapshot.cpu.present, [2, 4])?
  test.eq(snapshot.cpu.cpus.len(), 2)?
  test.eq(snapshot.cpu.cpus[0].id, 2)?
  test.ok(snapshot.cpu.cpus[0].online == true)?
  test.eq(snapshot.cpu.cpus[1].id, 4)?
  test.ok(snapshot.cpu.cpus[1].online == false)?
}

test test_system_report_cpu_enumeration_requires_a_valid_present_list [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu", parents: true)?
  root.write(p"sys/devices/system/cpu/possible", "0")?
  root.write(p"sys/devices/system/cpu/present", "0-x")?
  root.write(p"sys/devices/system/cpu/online", "0")?
  root.write(p"sys/devices/system/cpu/offline", "")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?

  let malformed = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(malformed.cpu.possible, [0])?
  test.eq(malformed.cpu.present, [])?
  test.ok(! malformed.cpu.status.enumeration_succeeded)?
  test.eq((malformed.issues |> where .section == "cpu" and .field == "present").len(), 1)?

  root.write(p"sys/devices/system/cpu/present", "0")?
  root.remove(p"sys/devices/system/cpu/possible")?
  let present = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(present.cpu.possible, [])?
  test.eq(present.cpu.present, [0])?
  test.ok(present.cpu.status.enumeration_succeeded)?
  test.eq(present.cpu.status.state, report_model.Partial)?

  var padding = " "
  while padding.count_chars() < 65536 {
    padding = f"${padding}${padding}"
  }

  root.write(p"sys/devices/system/cpu/present", f"0${padding}")?
  let truncated = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(truncated.cpu.present, [])?
  test.ok(! truncated.cpu.status.enumeration_succeeded)?
  let present_issues = truncated.issues |> where .section == "cpu" and .field == "present"
  test.eq(present_issues.len(), 1)?
  test.ok(present_issues[0].state == report_model.Truncated)?
}

test test_system_report_cpu_present_symlinks_cannot_cycle_or_escape_the_source_root [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  let outside = fs.tempdir()?
  defer outside.close()?
  root.mkdir(p"sys/devices/system/cpu", parents: true)?
  root.write(p"sys/devices/system/cpu/possible", "0")?
  outside.write(p"present", "0")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?

  root.symlink(p"present", p"sys/devices/system/cpu/present")?
  let cycled = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(cycled.cpu.present, [])?
  test.ok(! cycled.cpu.status.enumeration_succeeded)?
  test.eq((cycled.issues |> where .section == "cpu" and .field == "present").len(), 1)?

  root.remove(p"sys/devices/system/cpu/present")?
  let outside_path = outside.host_path()?
  root.symlink(fp"${outside_path}/present", p"sys/devices/system/cpu/present")?
  let escaped = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(escaped.cpu.present, [])?
  test.ok(! escaped.cpu.status.enumeration_succeeded)?
  test.eq((escaped.issues |> where .section == "cpu" and .field == "present").len(), 1)?
}

test test_system_report_cpu_directory_failures_keep_source_issues [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu/cpu0", parents: true)?
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )?
  root.write(p"sys/devices/system/cpu/offline", "")?
  root.write(p"sys/devices/system/cpu/cpu0/cache", "not a directory")?
  root.write(p"sys/devices/system/cpu/cpu0/cpuidle", "not a directory")?
  root.write(p"sys/devices/system/cpu/vulnerabilities", "not a directory")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  for field in ["cpu0.cache", "cpu0.cpuidle", "vulnerabilities"] {
    let matches = value.issues |> where .section == "cpu" and .field == field
    test.eq(matches.len(), 1)?
    test.ok(matches[0].state == report_model.ReadFailure)?
  }

  test.eq(value.cpu.status.state, report_model.Partial)?

  root.remove(p"sys/devices/system/cpu/cpu0/cache")?
  root.remove(p"sys/devices/system/cpu/cpu0/cpuidle")?
  root.remove(p"sys/devices/system/cpu/cpu0", dir: true)?
  let disappeared = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  let enumeration_issues = disappeared.issues |> where .section == "cpu" and .field == "cpu0.enumeration"
  test.eq(enumeration_issues.len(), 1)?
  test.ok(enumeration_issues[0].state == report_model.Absent)?
}

test test_system_report_cpu_vulnerability_read_failures_keep_named_issues [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu/cpu0", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/vulnerabilities", parents: true)?
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )?
  root.write(p"sys/devices/system/cpu/offline", "\n")?
  root.write(
    p"sys/devices/system/cpu/vulnerabilities/spectre_v2",
    """Mitigation: fixture policy
""",
  )?
  root.symlink(p"missing", p"sys/devices/system/cpu/vulnerabilities/spectre_v1")?
  var oversized = "x"
  while oversized.count_chars() <= 16384 {
    oversized = f"${oversized}${oversized}"
  }

  root.write(p"sys/devices/system/cpu/vulnerabilities/mmio_stale_data", oversized)?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  let valid = value.cpu.vulnerabilities |> where .name == "spectre_v2"
  test.eq(valid.len(), 1)?
  test.eq(valid[0].description.value, "Mitigation: fixture policy")?
  let vanished = value.cpu.vulnerabilities |> where .name == "spectre_v1"
  test.eq(vanished.len(), 1)?
  test.ok(vanished[0].description.state == report_model.Absent)?
  let truncated = value.cpu.vulnerabilities |> where .name == "mmio_stale_data"
  test.eq(truncated.len(), 1)?
  test.ok(truncated[0].description.state == report_model.Truncated)?
  test.ok(
    value.issues |> any .section == "cpu" and .field == "vulnerabilities.spectre_v1" and .state == report_model.Absent,
  )?
  test.ok(
    value.issues
      |> any .section == "cpu" and .field == "vulnerabilities.mmio_stale_data" and .state == report_model.Truncated,
  )?
  test.ok(value.cpu.status.state == report_model.Partial)?
}

test test_system_report_effective_cpuset_rejects_a_truncated_source [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)?
  root.mkdir(p"sys/fs/cgroup", parents: true)?
  root.mkdir(p"sys/devices/system/cpu", parents: true)?
  root.write(
    p"proc/self/cgroup",
    """0::/
""",
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )?
  root.write(p"sys/devices/system/cpu/possible", "0")?
  root.write(p"sys/devices/system/cpu/present", "0")?
  root.write(p"sys/devices/system/cpu/online", "0")?
  root.write(p"sys/devices/system/cpu/offline", "")?
  var padding = " "
  while padding.count_chars() < 65536 {
    padding = f"${padding}${padding}"
  }

  root.write(p"sys/fs/cgroup/cpuset.cpus.effective", f"0${padding}")?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let snapshot = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(snapshot.cpu.effective_cpuset, [])?
  let cpuset_issues = snapshot.issues |> where .section == "cpu" and .field == "cgroup.effective_cpuset"
  test.eq(cpuset_issues.len(), 1)?
  test.ok(cpuset_issues[0].state == report_model.Truncated)?

  root.write(p"sys/fs/cgroup/cpuset.cpus.effective", "0")?
  root.write(
    p"proc/self/mountinfo",
    """30 20 0:25 /outside /sys/fs/cgroup/other rw - cgroup2 cgroup rw
31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )?
  let selected_mount = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(selected_mount.cpu.effective_cpuset, [0])?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup - cgroup2 cgroup rw
""",
  )?
  let malformed_mount = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(malformed_mount.cpu.effective_cpuset, [])?
  test.ok(
    malformed_mount.issues
      |> any .section == "cpu" and .field == "cgroup.effective_cpuset" and .state == report_model.Malformed,
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw cgroup2 cgroup rw
""",
  )?
  let missing_separator = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(missing_separator.cpu.effective_cpuset, [])?
  test.ok(
    missing_separator.issues
      |> any .section == "cpu" and .field == "cgroup.effective_cpuset" and .state == report_model.Malformed,
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw -
""",
  )?
  let incomplete_row = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(incomplete_row.cpu.effective_cpuset, [])?
  test.ok(
    incomplete_row.issues
      |> any .section == "cpu" and .field == "cgroup.effective_cpuset" and .state == report_model.Malformed,
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )?
  root.write(p"proc/self/cgroup", f"0::/${padding}")?
  let truncated_membership = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(truncated_membership.cpu.effective_cpuset, [])?
  let membership_issues = truncated_membership.issues |> where .section == "cpu" and .field == "cgroup.effective_cpuset"
  test.eq(membership_issues.len(), 1)?
  test.ok(membership_issues[0].state == report_model.Truncated)?

  root.write(
    p"proc/self/cgroup",
    """0::/
0::/other
""",
  )?
  let duplicate_membership = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(duplicate_membership.cpu.effective_cpuset, [])?
  test.ok(
    duplicate_membership.issues
      |> any .section == "cpu" and .field == "cgroup.effective_cpuset" and .state == report_model.Malformed,
  )?
}

test test_system_report_cpu_collection_keeps_sparse_models_policies_and_cpuset [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/sys/kernel/random", parents: true)?
  root.mkdir(p"proc/self", parents: true)?
  root.mkdir(p"etc", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu0/topology", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu0/cache/index7", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu0/node0", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu2/topology", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu2/cache/index7", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu2/node1", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpufreq/policy3", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpufreq/policy9", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/vulnerabilities", parents: true)?
  root.mkdir(p"sys/fs/cgroup/worker", parents: true)?
  root.write(
    p"proc/sys/kernel/osrelease",
    """fixture-release
""",
  )?
  root.write(
    p"proc/version",
    """Linux fixture version 1
""",
  )?
  root.write(
    p"proc/sys/kernel/hostname",
    """fixture-host
""",
  )?
  root.write(
    p"proc/sys/kernel/random/boot_id",
    """fixture-boot-id
""",
  )?
  root.write(
    p"proc/uptime",
    """1.0 0.0
""",
  )?
  root.write(
    p"etc/os-release",
    """ID=fixture
""",
  )?
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
    p"proc/self/status",
    """Cpus_allowed_list:	0,2
""",
  )?
  root.write(
    p"proc/cpuinfo",
    """processor: 0
vendor_id: GenuineIntel
model name: Intel fixture
flags: fpu sse

processor: 2
vendor_id: AuthenticAMD
model name: AMD fixture
Features: fp asimd
""",
  )?
  root.write(
    p"sys/devices/system/cpu/possible",
    """0-2
""",
  )?
  root.write(
    p"sys/devices/system/cpu/present",
    """0,2
""",
  )?
  root.write(
    p"sys/devices/system/cpu/online",
    """0,2
""",
  )?
  root.write(
    p"sys/devices/system/cpu/offline",
    """1
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/topology/physical_package_id",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/topology/die_id",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/topology/core_id",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/topology/thread_siblings_list",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu2/topology/physical_package_id",
    """1
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu2/topology/die_id",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu2/topology/core_id",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu2/topology/thread_siblings_list",
    """2
""",
  )?
  for cpu_path in [p"sys/devices/system/cpu/cpu0/cache/index7", p"sys/devices/system/cpu/cpu2/cache/index7"] {
    root.write(
      fp"${cpu_path}/level",
      """2
""",
    )?
    root.write(
      fp"${cpu_path}/type",
      """Unified
""",
    )?
    root.write(
      fp"${cpu_path}/size",
      """1M
""",
    )?
    root.write(
      fp"${cpu_path}/coherency_line_size",
      """64
""",
    )?
    root.write(
      fp"${cpu_path}/number_of_sets",
      """16384
""",
    )?
    root.write(
      fp"${cpu_path}/shared_cpu_list",
      """0,2
""",
    )?
  }

  for policy in [
    {
      path: p"sys/devices/system/cpu/cpufreq/policy3",
      related: "0",
      affected: "0",
      driver: "intel_pstate",
      governor: "powersave",
    },
    {
      path: p"sys/devices/system/cpu/cpufreq/policy9",
      related: "1 2",
      affected: "2",
      driver: "acme_cpufreq",
      governor: "unlisted-governor",
    },
  ] {
    root.write(
      fp"${policy.path}/related_cpus",
      f"""${policy.related}
""",
    )?
    root.write(
      fp"${policy.path}/affected_cpus",
      f"""${policy.affected}
""",
    )?
    root.write(
      fp"${policy.path}/scaling_driver",
      f"""${policy.driver}
""",
    )?
    root.write(
      fp"${policy.path}/scaling_governor",
      f"""${policy.governor}
""",
    )?
    root.write(
      fp"${policy.path}/scaling_available_governors",
      """powersave performance
""",
    )?
    root.write(
      fp"${policy.path}/cpuinfo_min_freq",
      """800000
""",
    )?
    root.write(
      fp"${policy.path}/cpuinfo_max_freq",
      """4000000
""",
    )?
    root.write(
      fp"${policy.path}/scaling_min_freq",
      """1000000
""",
    )?
    root.write(
      fp"${policy.path}/scaling_max_freq",
      """3000000
""",
    )?
    root.write(
      fp"${policy.path}/energy_performance_preference",
      """balance_performance
""",
    )?
    root.write(
      fp"${policy.path}/energy_performance_available_preferences",
      """performance balance_performance power
""",
    )?
  }

  root.write(
    p"sys/devices/system/cpu/vulnerabilities/spectre_v2",
    """Mitigation: fixture policy
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpufreq/boost",
    """1
""",
  )?
  let policies = root.children(p"sys/devices/system/cpu/cpufreq")?
  test.eq(policies.state, "complete")?
  test.eq((policies.children |> where .name().starts_with("policy")).len(), 2)?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(value.cpu.possible, [0, 1, 2])?
  test.eq(value.cpu.present, [0, 2])?
  test.eq(value.cpu.offline, [1])?
  test.eq(value.cpu.affinity, [0, 2])?
  test.eq(value.cpu.effective_cpuset, [0, 2])?
  test.eq(value.cpu.cpus[0].model, "Intel fixture")?
  test.eq(value.cpu.cpus[1].model, "AMD fixture")?
  test.eq(value.cpu.frequency_policies.len(), 2)?
  test.eq(value.cpu.cpus[0].policy, "policy3")?
  test.eq(value.cpu.cpus[1].policy, "policy9")?
  test.eq(value.cpu.frequency_policies.len(), 2)?
  test.eq(value.cpu.frequency_policies[1].governor, "unlisted-governor")?
  test.eq(value.cpu.frequency_policies[1].related_cpus, [1, 2])?
  test.eq(value.cpu.frequency_policies[1].affected_cpus, [2])?
  test.eq(value.cpu.frequency_policies[0].hardware_min_khz, 800000)?
  test.eq(value.cpu.frequency_policies[1].hardware_max_khz, 4000000)?
  test.eq(value.cpu.frequency_policies[0].scaling_min_khz, 1000000)?
  test.eq(value.cpu.frequency_policies[1].scaling_max_khz, 3000000)?
  test.eq(value.cpu.frequency_policies[0].energy_performance_preference, "balance_performance")?
  test.eq(
    value.cpu.frequency_policies[1].available_energy_performance_preferences,
    ["performance", "balance_performance", "power"],
  )?
  test.eq(value.cpu.frequency_policies[0].boost_allowed, true)?
  test.eq(value.cpu.frequency_policies[0].boost_supported, true)?
  test.eq(value.cpu.frequency_policies[0].boost_active, null)?
  test.eq(value.cpu.frequency_policies[1].boost_scope, "system")?
  test.eq(value.identity.kernel_release, "fixture-release")?
  test.eq(value.cpu.caches.len(), 1)?
  test.eq(value.cpu.caches[0].sysfs_index, 7)?
  test.eq(value.cpu.caches[0].level, 2)?
  test.eq(value.cpu.caches[0].shared_cpus, [0, 2])?
  test.eq(value.cpu.cpus[0].numa_node, 0)?
  test.eq(value.cpu.cpus[1].numa_node, 1)?
  test.eq(value.cpu.cpus[0].cache_ids, [0])?
  test.eq(value.cpu.cpus[1].cache_ids, [0])?
  test.eq(value.cpu.vulnerabilities[0].description.value, "Mitigation: fixture policy")?
  root.write(
    p"sys/devices/system/cpu/cpufreq/policy9/scaling_governor",
    """userspace
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpufreq/policy9/scaling_setspeed",
    """1900000
""",
  )?
  let userspace = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(userspace.cpu.frequency_policies[0].governor_requested_khz, null)?
  test.eq(userspace.cpu.frequency_policies[1].governor_requested_khz, 1900000)?
  root.write(
    p"sys/devices/system/cpu/cpufreq/policy9/scaling_setspeed",
    """invalid
""",
  )?
  let malformed = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(malformed.cpu.frequency_policies[1].governor_requested_khz, null)?
  test.ok(
    malformed.issues |> any .section == "cpu" and .field == "policy9.scaling_setspeed" and .state == report_model.Malformed,
  )?
}

test test_system_report_cpufreq_policy_rejects_truncated_field_prefixes [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  let policy_path = p"sys/devices/system/cpu/cpufreq/policy0"
  root.mkdir(policy_path, parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu0", parents: true)?
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )?
  root.write(p"sys/devices/system/cpu/offline", "\n")?
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"${padding}${padding}"
  }

  var long_padding = padding
  while long_padding.count_chars() < 65536 {
    long_padding = f"${long_padding}${long_padding}"
  }

  for field in [
    {
      name: "related_cpus",
      prefix: "0",
    },
    {
      name: "affected_cpus",
      prefix: "0",
    },
    {
      name: "scaling_driver",
      prefix: "fixture_driver",
    },
    {
      name: "scaling_governor",
      prefix: "performance",
    },
    {
      name: "scaling_min_freq",
      prefix: "1000",
    },
    {
      name: "scaling_available_frequencies",
      prefix: "1000 2000",
    },
    {
      name: "energy_performance_preference",
      prefix: "performance",
    },
  ] {
    let suffix = if field.name == "scaling_available_frequencies" { long_padding } else { padding }
    root.write(
      fp"${policy_path}/${field.name}",
      f"""${field.prefix}
${suffix}""",
    )?
  }

  root.write(
    p"sys/devices/system/cpu/cpufreq/boost",
    f"""1
${padding}""",
  )?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(value.cpu.frequency_policies.len(), 1)?
  let policy = value.cpu.frequency_policies[0]
  test.eq(policy.related_cpus, [])?
  test.eq(policy.affected_cpus, [])?
  test.eq(policy.driver, null)?
  test.eq(policy.governor, null)?
  test.eq(policy.scaling_min_khz, null)?
  test.eq(policy.available_frequencies_khz, [])?
  test.eq(policy.energy_performance_preference, null)?
  test.eq(policy.boost_supported, null)?
  test.eq(policy.boost_allowed, null)?
  test.eq(policy.boost_scope, null)?
  let truncated_fields = value.issues |> where .section == "cpu" and .state == report_model.Truncated
  for field in [
    "policy0.related_cpus",
    "policy0.affected_cpus",
    "policy0.driver",
    "policy0.governor",
    "policy0.scaling_min_freq",
    "policy0.scaling_available_frequencies",
    "policy0.energy_performance_preference",
    "boost",
  ] {
    test.ok(truncated_fields |> any .field == field)?
  }

  root.remove(fp"${policy_path}/scaling_min_freq")?
  root.mkdir(fp"${policy_path}/scaling_min_freq")?
  let unreadable = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(unreadable.cpu.frequency_policies[0].scaling_min_khz, null)?
  let failed_minimum = unreadable.issues |> where .section == "cpu" and .field == "policy0.scaling_min_freq"
  test.eq(failed_minimum.len(), 1)?
  test.eq(failed_minimum[0].state, report_model.ReadFailure)?
}

test test_system_report_affinity_rejects_duplicate_status_field [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu", parents: true)?
  root.mkdir(p"proc/self", parents: true)?
  root.write(
    p"sys/devices/system/cpu/possible",
    """0-1
""",
  )?
  root.write(
    p"sys/devices/system/cpu/present",
    """0-1
""",
  )?
  root.write(
    p"sys/devices/system/cpu/online",
    """0-1
""",
  )?
  root.write(p"sys/devices/system/cpu/offline", "\n")?
  root.write(
    p"proc/self/status",
    """Cpus_allowed_list:	0
Cpus_allowed_list:	1
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(value.cpu.affinity, [])?
  test.ok(
    value.issues
      |> any .section == "cpu" and .field == "affinity" and .state == report_model.Malformed and .error_kind == "duplicate_cpu_list",
  )?
}

test test_system_report_idle_governor_uses_read_only_source_when_writable_source_is_absent [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu/cpuidle", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu0", parents: true)?
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )?
  root.write(p"sys/devices/system/cpu/offline", "\n")?
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
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(value.cpu.global_idle_governor, "menu")?
  test.ok(! (value.issues |> any .field == "cpuidle.current_governor"))?
  root.write(
    p"sys/devices/system/cpu/cpuidle/current_governor",
    """teo
""",
  )?
  let writable = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(writable.cpu.global_idle_governor, "teo")?
}

test test_system_report_idle_and_affinity_reject_truncated_prefixes [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu0/cpuidle/state0", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu0/cpuidle/state1", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu0/cpuidle/state9007199254740992", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpuidle", parents: true)?
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )?
  root.write(p"sys/devices/system/cpu/offline", "\n")?
  var padding = " "
  while padding.count_chars() < 65536 {
    padding = f"${padding}${padding}"
  }

  root.write(
    p"proc/self/status",
    f"""Cpus_allowed_list:	0
${padding}""",
  )?
  for source_path in [
    p"sys/devices/system/cpu/cpuidle/current_driver",
    p"sys/devices/system/cpu/cpuidle/current_governor",
    p"sys/devices/system/cpu/cpuidle/available_governors",
    p"sys/devices/system/cpu/cpu0/cpuidle/state0/name",
    p"sys/devices/system/cpu/cpu0/cpuidle/state0/desc",
  ] {
    root.write(
      source_path,
      f"""complete-looking prefix
${padding}""",
    )?
  }

  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state0/disable",
    """2
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state0/latency",
    f"""12
${padding}""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state0/residency",
    """123
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state0/usage",
    """9007199254740992
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state0/time",
    f"""8
${padding}""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state1/name",
    """C1
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state1/disable",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state1/latency",
    """9
""",
  )?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(value.cpu.affinity, [])?
  test.eq(value.cpu.global_idle_driver, null)?
  test.eq(value.cpu.global_idle_governor, null)?
  test.eq(value.cpu.available_idle_governors, [])?
  let malformed = (value.cpu.idle_states
    |> where .name == "state0"
    |> first())?
  test.eq(malformed.state_index, 0)?
  test.eq(malformed.description, null)?
  test.eq(malformed.disable_setting, null)?
  test.eq(malformed.latency_us, null)?
  test.eq(malformed.residency_us, 123)?
  test.eq(malformed.usage_count, null)?
  test.eq(malformed.time_us, null)?
  let valid = (value.cpu.idle_states
    |> where .name == "C1"
    |> first())?
  test.eq(valid.state_index, 1)?
  test.eq(valid.disable_setting, 0)?
  test.eq(valid.latency_us, 9)?
  test.eq(value.cpu.idle_states.len(), 2)?
  test.ok(value.issues |> any .field == "cpu0.state9007199254740992" and .error_kind == "invalid_idle_state_index")?
  let truncated_fields = value.issues |> where .section == "cpu" and .state == report_model.Truncated
  for field in [
    "affinity",
    "cpuidle.current_driver",
    "cpuidle.current_governor",
    "cpuidle.available_governors",
    "cpu0.state0.name",
    "cpu0.state0.desc",
    "cpu0.state0.latency",
    "cpu0.state0.time",
  ] {
    test.ok(truncated_fields |> any .field == field)?
  }

  test.ok(value.issues |> any .field == "cpu0.state0.disable" and .state == report_model.Malformed)?
  test.ok(value.issues |> any .field == "cpu0.state0.usage" and .state == report_model.RangeFailure)?

  root.remove(p"sys/devices/system/cpu/cpu0/cpuidle/state1/latency")?
  root.mkdir(p"sys/devices/system/cpu/cpu0/cpuidle/state1/latency")?
  let unreadable = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  let idle_state = (unreadable.cpu.idle_states
    |> where .name == "C1"
    |> first())?
  test.eq(idle_state.disable_setting, 0)?
  test.eq(idle_state.latency_us, null)?
  let failed_latency = unreadable.issues |> where .section == "cpu" and .field == "cpu0.state1.latency"
  test.eq(failed_latency.len(), 1)?
  test.eq(failed_latency[0].state, report_model.ReadFailure)?
}

test test_system_report_cpuinfo_rejects_a_truncated_complete_looking_prefix [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu0", parents: true)?
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )?
  root.write(p"sys/devices/system/cpu/offline", "\n")?
  var padding = " "
  while padding.count_chars() < 8388608 {
    padding = f"${padding}${padding}"
  }

  root.write(
    p"proc/cpuinfo",
    f"""processor: 0
model name: complete-looking prefix
${padding}""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(value.cpu.cpus.len(), 1)?
  test.eq(value.cpu.cpus[0].model, null)?
  let matches = value.issues |> where .section == "cpu" and .field == "cpuinfo"
  test.eq(matches.len(), 1)?
  test.ok(matches[0].state == report_model.Truncated)?
}

test test_system_report_cpu_topology_rejects_truncated_scalar_prefixes [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  let topology = p"sys/devices/system/cpu/cpu0/topology"
  root.mkdir(topology, parents: true)?
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )?
  root.write(p"sys/devices/system/cpu/offline", "\n")?
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"${padding}${padding}"
  }

  for field in [
    {
      name: "physical_package_id",
      prefix: "7",
    },
    {
      name: "die_id",
      prefix: "2",
    },
    {
      name: "core_id",
      prefix: "3",
    },
    {
      name: "thread_siblings_list",
      prefix: "0",
    },
  ] {
    root.write(
      fp"${topology}/${field.name}",
      f"""${field.prefix}
${padding}""",
    )?
  }

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let collected = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(collected.cpu.cpus.len(), 1)?
  let cpu_item = collected.cpu.cpus[0]
  test.eq(cpu_item.package_id, null)?
  test.eq(cpu_item.die_id, null)?
  test.eq(cpu_item.core_id, null)?
  test.eq(cpu_item.thread_siblings, [])?
  for field in ["physical_package_id", "die_id", "core_id", "thread_siblings_list"] {
    test.ok(
      collected.issues |> any .section == "cpu" and .field == f"cpu0.topology.${field}" and .state == report_model.Truncated,
    )?
  }

  root.write(
    fp"${topology}/core_id",
    """9007199254740992
""",
  )?
  let unsafe_core = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(unsafe_core.cpu.cpus[0].core_id, null)?
  test.ok(unsafe_core.issues |> any .field == "cpu0.topology.core_id" and .state == report_model.RangeFailure)?
}

test test_system_report_cpu_cache_sizes_reject_scaled_overflow_and_truncation [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu/cpu0/cache/index7", parents: true)?
  root.mkdir(p"sys/devices/system/cpu/cpu0/cache/index8", parents: true)?
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )?
  root.write(p"sys/devices/system/cpu/offline", "")?
  root.write(
    p"sys/devices/system/cpu/cpu0/cache/index7/level",
    """2
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/cache/index7/type",
    """Unified
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/cache/index7/size",
    """8796093022208K
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/cache/index8/level",
    """3
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/cache/index8/type",
    """Unified
""",
  )?
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"${padding}${padding}"
  }

  root.write(p"sys/devices/system/cpu/cpu0/cache/index8/size", f"512K${padding}")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(value.cpu.caches.len(), 2)?
  for cache in value.cpu.caches {
    test.ok(cache.size_bytes == null)?
  }

  let overflow = value.issues |> where .section == "cpu" and .field == "cpu0.cache.index7.size"
  test.eq(overflow.len(), 1)?
  test.ok(overflow[0].state == report_model.RangeFailure)?
  let truncated = value.issues |> where .section == "cpu" and .field == "cpu0.cache.index8.size"
  test.eq(truncated.len(), 1)?
  test.ok(truncated[0].state == report_model.Truncated)?
  let incomplete_cache = p"sys/devices/system/cpu/cpu0/cache/index9"
  root.mkdir(incomplete_cache, parents: true)?
  root.write(
    fp"${incomplete_cache}/level",
    f"""4
${padding}""",
  )?
  root.write(
    fp"${incomplete_cache}/type",
    """Unified
""",
  )?
  root.write(
    fp"${incomplete_cache}/size",
    """1024K
""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/cache/index7/coherency_line_size",
    f"""64
${padding}""",
  )?
  root.write(
    p"sys/devices/system/cpu/cpu0/cache/index7/number_of_sets",
    f"""1024
${padding}""",
  )?
  let incomplete = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(incomplete.cpu.caches.len(), 2)?
  test.ok(incomplete.issues |> any .field == "cpu0.cache.index9.level" and .state == report_model.Truncated)?
  let first_cache = incomplete.cpu.caches |> where .sysfs_index == 7
  test.eq(first_cache.len(), 1)?
  test.eq(first_cache[0].line_size_bytes, null)?
  test.eq(first_cache[0].sets, null)?
  test.ok(
    incomplete.issues |> any .field == "cpu0.cache.index7.coherency_line_size" and .state == report_model.Truncated,
  )?
  test.ok(incomplete.issues |> any .field == "cpu0.cache.index7.number_of_sets" and .state == report_model.Truncated)?
  root.write(
    fp"${incomplete_cache}/level",
    """4
""",
  )?
  root.write(
    fp"${incomplete_cache}/type",
    f"""Unified
${padding}""",
  )?
  let incomplete_kind = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(incomplete_kind.cpu.caches.len(), 2)?
  test.ok(incomplete_kind.issues |> any .field == "cpu0.cache.index9.type" and .state == report_model.Truncated)?
}

test test_system_report_cpu_cache_rejects_ambiguous_shared_cpu_list [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  let cache_path = p"sys/devices/system/cpu/cpu0/cache/index7"
  root.mkdir(cache_path, parents: true)?
  root.write(
    p"sys/devices/system/cpu/possible",
    """0-1
""",
  )?
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/offline",
    """1
""",
  )?
  root.write(
    fp"${cache_path}/level",
    """2
""",
  )?
  root.write(
    fp"${cache_path}/type",
    """Unified
""",
  )?
  root.write(
    fp"${cache_path}/size",
    """1M
""",
  )?
  root.write(
    fp"${cache_path}/shared_cpu_list",
    """0,,1
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(value.cpu.caches.len(), 1)?
  test.eq(value.cpu.caches[0].shared_cpus, [])?
  let matching = value.issues |> where .section == "cpu" and .field == "cpu0.cache.index7.shared_cpu_list"
  test.eq(matching.len(), 1)?
  test.ok(matching[0].state == report_model.Malformed)?
}

test test_system_report_cpu_cache_keeps_distinct_kernel_ids_with_same_sharing [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu/cpu0/cache", parents: true)?
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )?
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )?
  root.write(p"sys/devices/system/cpu/offline", "\n")?
  for entry in [{index: 7, kernel_id: 9}, {index: 8, kernel_id: 10}] {
    let base = fp"sys/devices/system/cpu/cpu0/cache/index${entry.index}"
    root.mkdir(base)?
    root.write(
      fp"${base}/id",
      f"""${entry.kernel_id}
""",
    )?
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
      fp"${base}/shared_cpu_list",
      """0
""",
    )?
  }

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  test.eq(value.cpu.caches.len(), 2)?
  test.eq(value.cpu.caches[0].sysfs_index, 7)?
  test.eq(value.cpu.caches[1].sysfs_index, 8)?
  test.eq(value.cpu.cpus[0].cache_ids, [0, 1])?
}

test test_system_report_live_collection_rejects_linux_dry_run [fs, process, env, time, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  env XSH_LINUX_DRY_RUN=1 {
    match collector.collect_live() {
      Err(error) => test.contains(error.message, "dry-run mode")?
      Ok(_) => test.fail("live collection accepted dry-run mode")?
    }
  }
}

test test_system_report_storage_parses_mountinfo_escapes_and_stacked_mounts [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)?
  root.write(
    p"proc/self/mountinfo",
    """12 1 8:1 / /mnt/a\\040b rw,relatime - ext4 /dev/sda1 rw,errors=remount-ro,password=private-secret
13 1 8:1 / /mnt/alias rw - ext4 /dev/sda1 rw
14 1 0:2 / /mnt/remote rw - cifs //user:private-secret@server/share rw
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 65536, 250, "storage", true, true)?
  test.eq(value.storage.mounts.len(), 3)?
  let first = value.storage.mounts[0]
  test.eq(first.mount_id, 12)?
  test.eq(first.target.value, "/mnt/a b")?
  test.eq(first.filesystem, "ext4")?
  test.eq(first.source.value, "/dev/sda1")?
  test.eq(first.super_options, ["rw", "errors=remount-ro", "redacted"])?
  test.eq(first.usage_state, report_model.Disappeared)?
  test.eq(value.storage.mounts[1].mount_id, 13)?
  test.eq(value.storage.mounts[1].usage_state, report_model.Disappeared)?
  test.eq(value.storage.mounts[2].mount_id, 14)?
  test.eq(value.storage.mounts[2].usage_state, report_model.NotRequested)?
  test.eq(value.storage.mounts[2].source.state, report_model.Redacted)?
  test.eq(value.storage.mounts[2].source.value, null)?
  let fixture_only = collector.collect_from_root(root, "fixture-arch", 65536, 250, "storage", true)?
  test.eq(fixture_only.storage.mounts[0].usage_state, report_model.NotRequested)?
}

test test_system_report_storage_mounts_reject_truncated_complete_looking_prefix [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)?
  let valid = """12 1 8:1 / /mnt/data rw - ext4 /dev/sda1 rw
"""
  var padding = "#"
  while padding.count_chars() < 4194304 {
    padding = padding + padding
  }

  root.write(p"proc/self/mountinfo", valid + padding)?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(value.storage.mounts.len(), 0)?
  test.ok(value.issues |> any .section == "storage" and .field == "mounts" and .state == report_model.Truncated)?
}

test test_system_report_storage_usage_skips_shadowed_and_automount_descendants [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)?
  root.write(
    p"proc/self/mountinfo",
    """1 0 0:1 / / rw - tmpfs tmpfs rw
2 1 8:1 / /mnt/shared rw - ext4 /dev/sda1 rw
3 1 0:3 / /mnt/shared rw - tmpfs tmpfs rw
4 1 0:4 / /mnt/auto rw - autofs autofs rw
5 4 8:2 / /mnt/auto/local rw - ext4 /dev/sdb1 rw
6 1 0:6 / /mnt/safe rw - tmpfs tmpfs rw
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true, true)?
  test.eq(value.storage.mounts.len(), 6)?
  test.ok(value.storage.mounts[0].usage_state == report_model.Observed)?
  for index in [1, 2, 3, 4] {
    test.ok(value.storage.mounts[index].usage_state == report_model.NotRequested)?
    test.eq(value.storage.mounts[index].usage_total_bytes, null)?
  }

  test.ok(value.storage.mounts[5].usage_state == report_model.Disappeared)?
}

test test_system_report_storage_mount_rejects_json_unsafe_identity [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)?
  root.write(
    p"proc/self/mountinfo",
    """9007199254740992 0 0:1 / /oversized-id rw - tmpfs tmpfs rw
2 0 9007199254740992:1 / /oversized-device rw - tmpfs tmpfs rw
3 0 0:1 / /valid rw - tmpfs tmpfs rw
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true, true)?
  test.eq(value.storage.mounts.len(), 1)?
  test.eq(value.storage.mounts[0].mount_id, 3)?
  test.eq(value.storage.mounts[0].usage_state, report_model.NotRequested)?
  test.ok(value.issues |> any .field == "mounts.line.0" and .state == report_model.RangeFailure)?
  test.ok(value.issues |> any .field == "mounts.line.1" and .state == report_model.RangeFailure)?
}

test test_system_report_storage_links_block_devices_to_pci_controllers [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/sys/kernel/random", parents: true)?
  root.mkdir(p"proc/self", parents: true)?
  root.mkdir(p"etc", parents: true)?
  root.mkdir(p"sys/bus/pci/devices/0001:02:03.0", parents: true)?
  root.mkdir(p"sys/class/block", parents: true)?
  root.mkdir(p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue", parents: true)?
  root.mkdir(p"sys/devices/pci0001:02/0001:02:03.0/nvme0", parents: true)?
  root.mkdir(p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/holders", parents: true)?
  root.mkdir(p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/slaves", parents: true)?
  root.mkdir(p"sys/bus/pci/devices/0001:02:03.0/iommu_group", parents: true)?
  root.write(
    p"proc/sys/kernel/osrelease",
    """fixture-release
""",
  )?
  root.write(
    p"proc/version",
    """Linux fixture version 1
""",
  )?
  root.write(
    p"proc/sys/kernel/hostname",
    """fixture-host
""",
  )?
  root.write(
    p"proc/sys/kernel/random/boot_id",
    """fixture-boot-id
""",
  )?
  root.write(
    p"proc/uptime",
    """1.0 0.0
""",
  )?
  root.write(
    p"etc/os-release",
    """ID=fixture
""",
  )?
  root.write(
    p"proc/self/mountinfo",
    """12 1 259:0 / /mnt/data rw - ext4 /dev/nvme0n1 rw
""",
  )?
  root.write(
    p"sys/bus/pci/devices/0001:02:03.0/vendor",
    """0x1234
""",
  )?
  root.write(
    p"sys/bus/pci/devices/0001:02:03.0/device",
    """0xabcd
""",
  )?
  root.write(
    p"sys/bus/pci/devices/0001:02:03.0/subsystem_vendor",
    """0x1234
""",
  )?
  root.write(
    p"sys/bus/pci/devices/0001:02:03.0/subsystem_device",
    """0x0001
""",
  )?
  root.write(
    p"sys/bus/pci/devices/0001:02:03.0/class",
    """0x010802
""",
  )?
  root.write(
    p"sys/bus/pci/devices/0001:02:03.0/revision",
    """0x01
""",
  )?
  test.eq(root.children(p"sys/bus/pci/devices")?.children.len(), 1)?
  root.symlink(../../devices/pci0001:02/0001:02:03.0/block/nvme0n1, p"sys/class/block/nvme0n1")?
  root.symlink(../../nvme0, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/device")?
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/dev",
    """259:0
""",
  )?
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/size",
    """16
""",
  )?
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/logical_block_size",
    """512
""",
  )?
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/physical_block_size",
    """4096
""",
  )?
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/removable",
    """0
""",
  )?
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/rotational",
    """0
""",
  )?
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/ro",
    """0
""",
  )?
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/device/model",
    """Fixture NVMe
""",
  )?
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/device/firmware_rev",
    """1.0
""",
  )?
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/scheduler",
    """[none] mq-deadline
""",
  )?
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/read_ahead_kb",
    """128
""",
  )?
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/discard_granularity",
    """4096
""",
  )?
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/discard_max_bytes",
    """1048576
""",
  )?
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/stat",
    """1 0 8 1 2 0 16 2 0 3 4
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(value.pci.functions.len(), 1)?
  test.eq(value.pci.functions[0].domain, 1)?
  if value.storage.devices.len() == 0 {
    test.fail(
      (value.issues
        |> where .section == "storage"
        |> first())?.error_kind ?? "no storage issue",
    )?
  }

  test.eq(value.storage.devices.len(), 1)?
  test.eq(value.storage.devices[0].kind, "disk")?
  test.eq(value.storage.devices[0].size_bytes, 8192)?
  test.eq(value.storage.devices[0].parent_pci_function_index, 0)?
  test.eq(value.storage.mounts[0].block_device_index, 0)?
}

test test_system_report_block_scheduler_requires_one_selected_choice [fs, error] {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let selected = collectors.parse_block_scheduler("none [mq-deadline] kyber").require(BlockScheduler)?
  test.eq(selected.active, "mq-deadline")?
  test.eq(selected.available, ["none", "mq-deadline", "kyber"])?
  let tabbed = collectors.parse_block_scheduler("[none]\tfixture-scheduler").require(BlockScheduler)?
  test.eq(tabbed.active, "none")?
  test.eq(tabbed.available, ["none", "fixture-scheduler"])?
  for invalid in [
    "",
    "none mq-deadline",
    "[none] [mq-deadline]",
    "[] none",
    "[none] none",
    """[none]
kyber""",
  ] {
    test.ok(collectors.parse_block_scheduler(invalid) == null)?
  }
}

test test_system_report_storage_rejects_invalid_block_source_fields [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/block/loop0/queue", parents: true)?
  root.mkdir(p"sys/class/block/loop0/holders", parents: true)?
  root.mkdir(p"sys/class/block/loop0/slaves", parents: true)?
  root.mkdir(p"sys/class/block/loop0/device", parents: true)?
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"${padding}${padding}"
  }

  root.write(
    p"sys/class/block/loop0/dev",
    f"""7:0
${padding}""",
  )?
  root.write(
    p"sys/class/block/loop0/size",
    f"""16
${padding}""",
  )?
  root.write(
    p"sys/class/block/loop0/queue/logical_block_size",
    f"""512
${padding}""",
  )?
  root.write(
    p"sys/class/block/loop0/removable",
    f"""1
${padding}""",
  )?
  root.write(
    p"sys/class/block/loop0/queue/scheduler",
    f"""[none] mq-deadline
${padding}""",
  )?
  root.write(
    p"sys/class/block/loop0/device/model",
    f"""fixture-model
${padding}""",
  )?
  root.write(
    p"sys/class/block/loop0/device/firmware_rev",
    f"""fixture-revision
${padding}""",
  )?
  root.write(
    p"sys/class/block/loop0/device/rev",
    """fallback-revision
""",
  )?
  root.write(
    p"sys/class/block/loop0/stat",
    f"""1 0 8 1 2 0 16 2 0 3 4
${padding}""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(value.storage.devices.len(), 1)?
  let device = value.storage.devices[0]
  test.eq(device.major, null)?
  test.eq(device.minor, null)?
  test.eq(device.size_bytes, null)?
  test.eq(device.logical_sector_bytes, null)?
  test.eq(device.removable, null)?
  test.eq(device.active_scheduler, null)?
  test.eq(device.available_schedulers, [])?
  test.eq(device.io_counters, [])?
  test.eq(device.model.value, null)?
  test.eq(device.firmware.value, null)?
  test.ok(device.model.state == report_model.Truncated)?
  test.ok(device.firmware.state == report_model.Truncated)?
  for field in [
    "major_minor",
    "size",
    "logical_sector_bytes",
    "removable",
    "scheduler",
    "stat",
    "model",
    "firmware",
  ] {
    let matches = value.issues |> where .section == "storage" and .field == f"devices.loop0.${field}"
    test.eq(matches.len(), 1)?
    test.ok(matches[0].state == report_model.Truncated)?
  }

  root.write(
    p"sys/class/block/loop0/queue/scheduler",
    """none mq-deadline
""",
  )?
  let malformed_scheduler = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(malformed_scheduler.storage.devices[0].active_scheduler, null)?
  test.eq(malformed_scheduler.storage.devices[0].available_schedulers, [])?
  let scheduler_issues = malformed_scheduler.issues
    |> where .section == "storage" and .field == "devices.loop0.scheduler"
  test.eq(scheduler_issues.len(), 1)?
  test.eq(scheduler_issues[0].state, report_model.Malformed)?

  root.write(
    p"sys/class/block/loop0/queue/logical_block_size",
    """invalid
""",
  )?
  root.write(
    p"sys/class/block/loop0/queue/physical_block_size",
    """9007199254740992
""",
  )?
  root.write(
    p"sys/class/block/loop0/removable",
    """2
""",
  )?
  root.write(
    p"sys/class/block/loop0/queue/rotational",
    """-1
""",
  )?
  root.write(
    p"sys/class/block/loop0/ro",
    """invalid
""",
  )?
  root.write(
    p"sys/class/block/loop0/queue/read_ahead_kb",
    """-2
""",
  )?
  root.write(
    p"sys/class/block/loop0/queue/discard_granularity",
    """0x10
""",
  )?
  root.write(
    p"sys/class/block/loop0/queue/discard_max_bytes",
    """9007199254740992
""",
  )?
  let invalid_queue = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  let queue_device = invalid_queue.storage.devices[0]
  test.eq(queue_device.logical_sector_bytes, null)?
  test.eq(queue_device.physical_sector_bytes, null)?
  test.eq(queue_device.removable, null)?
  test.eq(queue_device.rotational, null)?
  test.eq(queue_device.read_only, null)?
  test.eq(queue_device.read_ahead_kb, null)?
  test.eq(queue_device.discard_granularity_bytes, null)?
  test.eq(queue_device.discard_max_bytes, null)?
  for expected in [
    {
      field: "logical_sector_bytes",
      state: report_model.Malformed,
    },
    {
      field: "physical_sector_bytes",
      state: report_model.RangeFailure,
    },
    {
      field: "removable",
      state: report_model.Malformed,
    },
    {
      field: "rotational",
      state: report_model.Malformed,
    },
    {
      field: "read_only",
      state: report_model.Malformed,
    },
    {
      field: "read_ahead_kb",
      state: report_model.Malformed,
    },
    {
      field: "discard_granularity_bytes",
      state: report_model.Malformed,
    },
    {
      field: "discard_max_bytes",
      state: report_model.RangeFailure,
    },
  ] {
    let matches = invalid_queue.issues |> where .section == "storage" and .field == f"devices.loop0.${expected.field}"
    test.eq(matches.len(), 1)?
    test.eq(matches[0].state, expected.state)?
  }

  root.write(
    p"sys/class/block/loop0/dev",
    """9007199254740992:0
""",
  )?
  let unsafe_identity = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(unsafe_identity.storage.devices[0].major, null)?
  let invalid = unsafe_identity.issues |> where .section == "storage" and .field == "devices.loop0.major_minor"
  test.eq(invalid.len(), 1)?
  test.ok(invalid[0].state == report_model.RangeFailure)?

  root.write(
    p"sys/class/block/loop0/dev",
    """7:0
""",
  )?
  root.write(
    p"sys/class/block/loop0/size",
    """17592186044415
""",
  )?
  let maximum = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(maximum.storage.devices[0].size_bytes, 9007199254740480)?
  root.write(
    p"sys/class/block/loop0/size",
    """17592186044416
""",
  )?
  let oversized = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(oversized.storage.devices[0].size_bytes, null)?
  let size_issues = oversized.issues |> where .section == "storage" and .field == "devices.loop0.size"
  test.eq(size_issues.len(), 1)?
  test.ok(size_issues[0].state == report_model.RangeFailure)?

  root.write(
    p"sys/class/block/loop0/size",
    """-0
""",
  )?
  let negative_zero = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(negative_zero.storage.devices[0].size_bytes, null)?
  let negative_size = negative_zero.issues |> where .section == "storage" and .field == "devices.loop0.size"
  test.eq(negative_size.len(), 1)?
  test.eq(negative_size[0].state, report_model.Malformed)?

  root.write(
    p"sys/class/block/loop0/size",
    """16
""",
  )?
  root.write(
    p"sys/class/block/loop0/stat",
    """9007199254740992 0 8 1 2 0 16 2 0 3 4
""",
  )?
  let counters = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.ok(! (counters.storage.devices[0].io_counters |> any .name == "read_ios"))?
  test.ok(counters.storage.devices[0].io_counters |> any .name == "read_sectors" and .value == 8)?
  let counter_issues = counters.issues |> where .section == "storage" and .field == "devices.loop0.stat.read_ios"
  test.eq(counter_issues.len(), 1)?
  test.ok(counter_issues[0].state == report_model.RangeFailure)?

  root.write(
    p"sys/class/block/loop0/stat",
    """1 0 8 1 2 0 16 2 0 3 4 5 6 7 8 9 10
""",
  )?
  let full_stats = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  let values = full_stats.storage.devices[0].io_counters
  test.eq(values.len(), 17)?
  test.ok(values |> any .name == "discard_ios" and .value == 5)?
  test.ok(values |> any .name == "discard_sectors" and .value == 7)?
  test.ok(values |> any .name == "flush_ios" and .value == 9)?
  test.ok(values |> any .name == "flush_ms" and .value == 10)?
  test.ok(! (full_stats.issues |> any .field == "devices.loop0.stat"))?

  root.write(
    p"sys/class/block/loop0/stat",
    """1 0 8 1 2 0 16 2 0 3 4 5 6
""",
  )?
  let incomplete_stats = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(incomplete_stats.storage.devices[0].io_counters, [])?
  let incomplete_issue = incomplete_stats.issues |> where .section == "storage" and .field == "devices.loop0.stat"
  test.eq(incomplete_issue.len(), 1)?
  test.ok(incomplete_issue[0].state == report_model.Malformed)?

  root.write(
    p"sys/class/block/loop0/stat",
    """1 0 8 1 2 0 16 2 0 3 4 5 6 7 8 9 10 11
""",
  )?
  let future_stats = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(future_stats.storage.devices[0].io_counters.len(), 17)?
  let unknown = future_stats.issues |> where .section == "storage" and .field == "devices.loop0.stat"
  test.eq(unknown.len(), 1)?
  test.ok(unknown[0].state == report_model.Unsupported)?

  root.remove(p"sys/class/block/loop0/queue/read_ahead_kb")?
  root.mkdir(p"sys/class/block/loop0/queue/read_ahead_kb")?
  let unreadable = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(unreadable.storage.devices[0].read_ahead_kb, null)?
  let failed_read_ahead = unreadable.issues |> where .section == "storage" and .field == "devices.loop0.read_ahead_kb"
  test.eq(failed_read_ahead.len(), 1)?
  test.eq(failed_read_ahead[0].state, report_model.ReadFailure)?
}

test test_system_report_storage_keeps_a_device_with_missing_numbers [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/block/mystery0", parents: true)?
  root.write(
    p"sys/class/block/mystery0/size",
    """16
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(value.storage.devices.len(), 1)?
  test.eq(value.storage.devices[0].name, "mystery0")?
  test.ok(value.storage.devices[0].major == null)?
  test.ok(value.storage.devices[0].minor == null)?
  let missing_numbers = value.issues |> where .field == "devices.mystery0.major_minor"
  test.eq(missing_numbers.len(), 1)?
  test.eq(missing_numbers[0].state, report_model.Absent)?
}

test test_system_report_storage_links_layered_block_devices_by_identity [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  for name in ["sda", "dm-0"] {
    root.mkdir(fp"sys/class/block/${name}/holders", parents: true)?
    root.mkdir(fp"sys/class/block/${name}/slaves", parents: true)?
    root.write(
      fp"sys/class/block/${name}/size",
      """16
""",
    )?
  }

  root.write(
    p"sys/class/block/sda/dev",
    """8:0
""",
  )?
  root.write(
    p"sys/class/block/dm-0/dev",
    """253:0
""",
  )?
  root.symlink(../../dm-0, p"sys/class/block/sda/holders/dm-0")?
  root.symlink(../../sda, p"sys/class/block/dm-0/slaves/sda")?
  root.mkdir(p"proc/self", parents: true)?
  root.write(p"proc/self/mountinfo", "")?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(value.storage.status.state, report_model.Complete)?
  test.eq(value.storage.devices.len(), 2)?
  let base = (value.storage.devices |> where .name == "sda")[0]
  let stacked = (value.storage.devices |> where .name == "dm-0")[0]
  test.eq(base.major, 8)?
  test.eq(stacked.major, 253)?
  test.eq(base.holder_indices.len(), 1)?
  test.eq(stacked.slave_indices.len(), 1)?
  test.eq(value.storage.devices[base.holder_indices[0]].name, "dm-0")?
  test.eq(value.storage.devices[stacked.slave_indices[0]].name, "sda")?
  test.eq(base.slave_indices, [])?
  test.eq(stacked.holder_indices, [])?
}

test test_system_report_storage_keeps_sparse_partition_numbers_and_parent_links [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/block", parents: true)?
  let disk_path = p"sys/devices/pci0000:00/0000:00:01.0/block/sda"
  root.mkdir(fp"${disk_path}/holders", parents: true)?
  root.mkdir(fp"${disk_path}/slaves", parents: true)?
  root.write(
    fp"${disk_path}/dev",
    """8:0
""",
  )?
  root.write(
    fp"${disk_path}/size",
    """1024
""",
  )?
  root.symlink(../../devices/pci0000:00/0000:00:01.0/block/sda, p"sys/class/block/sda")?
  for number in [1, 3] {
    let name = f"sda${number}"
    let partition_path = fp"${disk_path}/${name}"
    root.mkdir(fp"${partition_path}/holders", parents: true)?
    root.write(
      fp"${partition_path}/partition",
      f"""${number}
""",
    )?
    root.write(
      fp"${partition_path}/dev",
      f"""8:${number}
""",
    )?
    root.write(
      fp"${partition_path}/size",
      """128
""",
    )?
    root.symlink(fp"../../devices/pci0000:00/0000:00:01.0/block/sda/${name}", fp"sys/class/block/${name}")?
  }

  root.mkdir(p"proc/self", parents: true)?
  root.write(p"proc/self/mountinfo", "")?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(value.storage.status.state, report_model.Complete)?
  test.eq(value.storage.devices.len(), 3)?
  test.eq((value.storage.devices |> where .name == "sda2").len(), 0)?
  let disk = (value.storage.devices |> where .name == "sda")[0]
  test.eq(disk.kind, "disk")?
  test.eq(disk.size_bytes, 524288)?
  for name in ["sda1", "sda3"] {
    let partition = (value.storage.devices |> where .name == name)[0]
    test.eq(partition.kind, "partition")?
    test.eq(value.storage.devices[partition.parent_device_index ?? -1].name, "sda")?
    test.eq(partition.size_bytes, 65536)?
  }
}

test test_system_report_storage_keeps_holder_and_slave_enumeration_failures [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/block/fixture", parents: true)?
  root.write(
    p"sys/class/block/fixture/dev",
    """8:0
""",
  )?
  root.write(p"sys/class/block/fixture/holders", "not a directory")?
  root.write(p"sys/class/block/fixture/slaves", "not a directory")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  test.eq(value.storage.devices.len(), 1)?
  for field in ["devices.fixture.holders", "devices.fixture.slaves"] {
    let matches = value.issues |> where .section == "storage" and .field == field
    test.eq(matches.len(), 1)?
    test.ok(matches[0].state == report_model.ReadFailure)?
  }

  test.eq((value.issues |> where .section == "storage" and .field == "devices.fixture.sysfs_target").len(), 0)?

  root.write(p"sys/class/block/not-a-directory", "not a block device")?
  let failed_link = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  let link_issues = failed_link.issues
    |> where .section == "storage" and .field == "devices.not-a-directory.sysfs_target"
  test.eq(link_issues.len(), 1)?
  test.ok(link_issues[0].state == report_model.ReadFailure)?
  test.ok(link_issues[0].errno != null)?
}

test test_system_report_process_stat_parser_preserves_start_identity [fs, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let stat = collector.parse_proc_stat("123 (worker (pool)) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2")?
  test.eq(stat.pid, 123)?
  test.eq(stat.parent_pid, 1)?
  test.eq(stat.command, "worker (pool)")?
  test.eq(stat.thread_count, 2)?
  test.eq(stat.start_ticks, 100)?
  test.eq(stat.virtual_bytes, 8192)?
  test.eq(stat.resident_pages, 2)?
  let error_kind = f"${fs.cwd()?.display()}/core/lib/system_report.xsh.SystemReportError.InvalidProcStat"
  for invalid in [
    "0x7b (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2",
    "1_23 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2",
    "9007199254740992 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2",
    "123 (worker) S 9007199254740992 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2",
    "123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 9007199254740992 8192 2",
  ] {
    test.error_kind(collector.parse_proc_stat(invalid), error_kind)?
  }

  let oversized_optional = collector.parse_proc_stat(
    "123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 9007199254740992 0 100 9007199254740992 9007199254740992",
  )?
  test.eq(oversized_optional.thread_count, null)?
  test.eq(oversized_optional.virtual_bytes, null)?
  test.eq(oversized_optional.resident_pages, null)?
  test.ok(oversized_optional.field_issues |> any .field == "thread_count" and .state == report_model.RangeFailure)?
  test.ok(oversized_optional.field_issues |> any .field == "virtual_bytes" and .state == report_model.RangeFailure)?
  test.ok(oversized_optional.field_issues |> any .field == "resident_pages" and .state == report_model.RangeFailure)?
}

test test_system_report_process_statm_overflow_does_not_publish_stat_fallback [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)?
  root.mkdir(p"proc/9007199254740992", parents: true)?
  root.write(
    p"proc/123/stat",
    """123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2
""",
  )?
  root.write(
    p"proc/123/statm",
    """137438953472 137438953472 0 0 0 0 0
""",
  )?
  root.write(
    p"proc/123/status",
    """Uid:	1234	1234	1234	1234
""",
  )?
  root.write(
    p"proc/123/cgroup",
    """0::/fixture
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 65536, 250, "processes", true)?
  test.eq(value.processes.processes.len(), 1)?
  test.eq(value.processes.processes[0].virtual_bytes, null)?
  test.eq(value.processes.processes[0].resident_bytes, null)?
  test.ok(value.issues |> any .field == "123.statm.virtual_bytes" and .state == report_model.RangeFailure)?
  test.ok(value.issues |> any .field == "123.statm.resident_bytes" and .state == report_model.RangeFailure)?
  test.ok(value.issues |> any .field == "9007199254740992.pid" and .state == report_model.RangeFailure)?
}

test test_system_report_process_statm_requires_all_kernel_fields [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)?
  root.write(
    p"proc/123/stat",
    """123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2
""",
  )?
  root.write(
    p"proc/123/statm",
    """2 1
""",
  )?
  root.write(
    p"proc/123/status",
    """Uid:	1234	1234	1234	1234
""",
  )?
  root.write(
    p"proc/123/cgroup",
    """0::/fixture
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 65536, 250, "processes", true)?
  test.eq(value.processes.processes.len(), 1)?
  test.eq(value.processes.processes[0].virtual_bytes, null)?
  test.eq(value.processes.processes[0].resident_bytes, null)?
  test.ok(value.issues |> any .field == "123.statm" and .state == report_model.Malformed)?

  root.write(
    p"proc/123/statm",
    """2 1 malformed 0 0 0 0
""",
  )?
  let malformed = collector.collect_from_root(root, "fixture-arch", 65536, 250, "processes", true)?
  test.eq(malformed.processes.processes[0].virtual_bytes, null)?
  test.eq(malformed.processes.processes[0].resident_bytes, null)?
  test.ok(malformed.issues |> any .field == "123.statm" and .state == report_model.Malformed)?

  root.write(
    p"proc/123/statm",
    """2 1 9007199254740992 0 0 0 0
""",
  )?
  let unused_large = collector.collect_from_root(root, "fixture-arch", 65536, 250, "processes", true)?
  test.eq(unused_large.processes.processes[0].virtual_bytes, 131072)?
  test.eq(unused_large.processes.processes[0].resident_bytes, 65536)?
}

test test_system_report_process_cgroup_requires_one_absolute_v2_path [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)?
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
    p"proc/123/status",
    """Uid:	1234	1234	1234	1234
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?

  root.write(
    p"proc/123/cgroup",
    """0::relative
""",
  )?
  let relative = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  test.eq(relative.processes.processes[0].cgroup.value, null)?
  test.eq(relative.processes.processes[0].cgroup.state, report_model.Malformed)?
  test.ok(relative.issues |> any .field == "123.cgroup" and .state == report_model.Malformed)?

  root.write(
    p"proc/123/cgroup",
    """0::/first
0::/second
""",
  )?
  let duplicate = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  test.eq(duplicate.processes.processes[0].cgroup.value, null)?
  test.eq(duplicate.processes.processes[0].cgroup.state, report_model.Malformed)?

  root.write(
    p"proc/123/cgroup",
    """2:cpu:/legacy
""",
  )?
  let legacy = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  test.eq(legacy.processes.processes[0].cgroup.value, null)?
  test.eq(legacy.processes.processes[0].cgroup.state, report_model.Unsupported)?

  root.write(
    p"proc/123/cgroup",
    """0::/tenant/worker
2:cpu:/legacy
""",
  )?
  let valid = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  test.eq(valid.processes.processes[0].cgroup.value, "/tenant/worker")?
  test.eq(valid.processes.processes[0].cgroup.state, report_model.Observed)?
}

test test_system_report_process_uid_requires_one_complete_numeric_status_row [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)?
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
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?

  root.write(
    p"proc/123/status",
    """Name:	worker
Uid:	1234	1235	1235	1235
""",
  )?
  let valid = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  test.eq(valid.processes.processes[0].uid, 1234)?
  test.eq((valid.issues |> where .field == "123.uid").len(), 0)?

  root.write(
    p"proc/123/status",
    """Uid:	1234	1235	1235	1235
Uid:	2000	2000	2000	2000
""",
  )?
  let duplicate = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  test.eq(duplicate.processes.processes[0].uid, null)?
  test.ok(duplicate.issues |> any .field == "123.uid" and .state == report_model.Malformed)?

  root.write(
    p"proc/123/status",
    """Uid:	1234
""",
  )?
  let short = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  test.eq(short.processes.processes[0].uid, null)?
  test.ok(short.issues |> any .field == "123.uid" and .state == report_model.Malformed)?
}

test test_system_report_process_collection_scales_pages_and_omits_private_sources [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)?
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
    p"proc/123/status",
    """Name:	worker
Uid:	1234	1234	1234	1234
""",
  )?
  root.write(
    p"proc/123/cgroup",
    """0::/fixture/group
""",
  )?
  root.write(p"proc/123/environ", "PRIVATE_ENVIRONMENT_TOKEN=secret\0")?
  root.write(p"proc/123/cmdline", "private-command-argument\0")?
  test.eq(root.children(p"proc")?.children.len(), 1)?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 65536, 250, "processes", true)?
  test.eq(value.scope.page_size_bytes, 65536)?
  test.eq(value.scope.clock_ticks_per_second, 250)?
  if value.processes.processes.len() == 0 {
    test.fail(
      (value.issues
        |> where .section == "processes"
        |> first())?.error_kind ?? "no process issue",
    )?
  }

  test.eq(value.processes.processes.len(), 1)?
  let process_item = value.processes.processes[0]
  test.eq(process_item.pid, 123)?
  test.eq(process_item.parent_pid, 1)?
  test.eq(process_item.uid, 1234)?
  test.eq(process_item.command.value, "worker")?
  test.eq(process_item.start_ticks, 100)?
  test.eq(process_item.resident_bytes, 65536)?
  test.eq(process_item.virtual_bytes, 131072)?
  test.eq(process_item.cgroup.value, "/fixture/group")?
  test.eq(process_item.cgroup_resource_index, null)?
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let sensitive_json = model.encode_report_json(value, true, false)?
  test.ok("PRIVATE_ENVIRONMENT_TOKEN" not in sensitive_json)?
  test.ok("private-command-argument" not in sensitive_json)?
  test.ok("\"environment\"" not in sensitive_json)?
  test.ok("\"cmdline\"" not in sensitive_json)?
}

test test_system_report_process_collection_rejects_truncated_stat_and_field_prefixes [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)?
  let stat = """123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2
"""
  var padding = " "
  while padding.count_chars() < 16384 {
    padding = f"${padding}${padding}"
  }

  root.write(p"proc/123/stat", f"${stat}${padding}")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let truncated_stat = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  test.eq(truncated_stat.processes.processes, [])?
  let stat_issues = truncated_stat.issues |> where .section == "processes" and .field == "123.stat"
  test.eq(stat_issues.len(), 1)?
  test.ok(stat_issues[0].state == report_model.Truncated)?

  root.write(p"proc/123/stat", stat)?
  root.write(
    p"proc/123/statm",
    f"""9 8 0 0 0 0 0
${padding}""",
  )?
  root.write(
    p"proc/123/status",
    f"""Uid:	1234	1234	1234	1234
${padding}""",
  )?
  root.write(
    p"proc/123/cgroup",
    f"""0::/partial
${padding}""",
  )?
  let truncated_fields = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  test.eq(truncated_fields.processes.processes.len(), 1)?
  let item = truncated_fields.processes.processes[0]
  test.eq(item.virtual_bytes, 8192)?
  test.eq(item.resident_bytes, 8192)?
  test.eq(item.uid, null)?
  test.ok(item.cgroup.state == report_model.Truncated)?
  test.eq(item.cgroup.value, null)?
  test.ok(truncated_fields.issues |> any .field == "123.statm" and .state == report_model.Truncated)?
  test.ok(truncated_fields.issues |> any .field == "123.status" and .state == report_model.Truncated)?
  test.ok(truncated_fields.issues |> any .field == "123.cgroup" and .state == report_model.Truncated)?
}

test test_system_report_joins_visible_process_cgroups_to_resource_records [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)?
  root.mkdir(p"proc/self", parents: true)?
  root.mkdir(p"sys/fs/cgroup/fixture/group", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
MemFree: 4 kB
MemAvailable: 8 kB
VendorCounter: 12 widgets
""",
  )?
  root.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
""",
  )?
  root.write(
    p"proc/self/cgroup",
    """0::/fixture/group
""",
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw,nosuid,nodev - cgroup2 cgroup rw
""",
  )?
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
    p"proc/123/status",
    """Uid:	1234	1234	1234	1234
""",
  )?
  root.write(
    p"proc/123/cgroup",
    """0::/fixture/group
""",
  )?
  root.write(
    p"sys/fs/cgroup/fixture/group/memory.max",
    """1048576
""",
  )?
  root.write(
    p"sys/fs/cgroup/fixture/group/memory.current",
    """524288
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "", true)?
  let process_item = (value.processes.processes
    |> where .pid == 123
    |> first())?
  test.eq(process_item.cgroup.value, "/fixture/group")?
  if process_item.cgroup_resource_index == null {
    test.fail("process cgroup relationship was not resolved")?
  }

  let resource = value.memory.cgroup[process_item.cgroup_resource_index ?? -1]
  test.eq(resource.path.value, process_item.cgroup.value)?
}

test test_system_report_memory_collects_cgroup_v2_limits_and_visible_ancestors [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)?
  root.mkdir(p"sys/fs/cgroup/a/b", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
MemFree: 4 kB
MemAvailable: 8 kB
VendorCounter: 12 widgets
""",
  )?
  root.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
""",
  )?
  root.write(
    p"proc/self/cgroup",
    """0::/a/b
""",
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw,nosuid,nodev - cgroup2 cgroup rw
""",
  )?
  root.write(
    p"sys/fs/cgroup/a/b/memory.max",
    """max
""",
  )?
  root.write(
    p"sys/fs/cgroup/a/b/memory.current",
    """1024
""",
  )?
  root.write(
    p"sys/fs/cgroup/a/b/memory.swap.max",
    """262144
""",
  )?
  root.write(
    p"sys/fs/cgroup/a/b/memory.swap.current",
    """65536
""",
  )?
  root.write(
    p"sys/fs/cgroup/a/b/cpu.max",
    """50000 100000
""",
  )?
  root.write(
    p"sys/fs/cgroup/a/b/cpu.stat",
    """usage_usec 9000
user_usec 7000
system_usec 2000
nr_periods 12
nr_throttled 2
throttled_usec 450
""",
  )?
  root.write(
    p"sys/fs/cgroup/a/b/cpuset.cpus.effective",
    """0-1
""",
  )?
  root.write(
    p"sys/fs/cgroup/a/b/pids.max",
    """max
""",
  )?
  root.write(
    p"sys/fs/cgroup/a/b/pids.current",
    """8
""",
  )?
  root.write(
    p"sys/fs/cgroup/a/b/io.stat",
    """8:0 rbytes=4096 wbytes=2048 rios=2 wios=1
""",
  )?
  root.write(
    p"sys/fs/cgroup/a/memory.max",
    """8192
""",
  )?
  root.write(
    p"sys/fs/cgroup/a/memory.current",
    """2048
""",
  )?
  root.write(
    p"sys/fs/cgroup/a/cpu.max",
    """max 100000
""",
  )?
  root.write(
    p"sys/fs/cgroup/a/pids.max",
    """100
""",
  )?
  root.write(
    p"sys/fs/cgroup/a/pids.current",
    """12
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 65536, 250, "memory", true)?
  let memory_limits = value.memory.cgroup |> where .controller == "memory" and .resource == "memory.max"
  test.eq(memory_limits.len(), 2)?
  let current_limit = memory_limits[0]
  test.eq(current_limit.hierarchy_level, 0)?
  test.eq(current_limit.maximum_unlimited, true)?
  test.eq(current_limit.current_value, 1024)?
  test.eq(current_limit.unit, "bytes")?
  let cpu_limit = (value.memory.cgroup
    |> where .resource == "cpu.max"
    |> first())?
  test.eq(cpu_limit.quota, 50000)?
  test.eq(cpu_limit.period, 100000)?
  let swap_limit = (value.memory.cgroup
    |> where .resource == "memory.swap.max"
    |> first())?
  test.eq(swap_limit.maximum_value, 262144)?
  test.eq(swap_limit.current_value, 65536)?
  let cpu_usage = (value.memory.cgroup
    |> where .resource == "cpu.stat.usage_usec"
    |> first())?
  test.eq(cpu_usage.current_value, 9000)?
  test.eq(cpu_usage.unit, "microseconds")?
  let cpuset = (value.memory.cgroup
    |> where .resource == "cpuset.cpus.effective"
    |> first())?
  test.eq(cpuset.effective_cpus, [0, 1])?
  let io_bytes = (value.memory.cgroup
    |> where .resource == "io.stat.8:0.rbytes"
    |> first())?
  test.eq(io_bytes.current_value, 4096)?
  test.eq(io_bytes.unit, "bytes")?
  test.ok(value.memory.host.counters |> any .name == "VendorCounter" and .value == 12 and .unit == "widgets")?

  let invalid_root = fs.tempdir()?
  defer invalid_root.close()?
  invalid_root.mkdir(p"proc/self", parents: true)?
  invalid_root.write(
    p"proc/meminfo",
    """MemTotal: 16 MB
""",
  )?
  invalid_root.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
""",
  )?
  let invalid = collector.collect_from_root(invalid_root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(invalid.memory.host.total_bytes, null)?
  test.ok(invalid.issues |> any .field == "meminfo.MemTotal" and .state == report_model.Malformed)?
}

test test_system_report_memory_preserves_colons_in_cgroup_membership_path [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)?
  root.mkdir(p"sys/fs/cgroup/team:blue", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )?
  root.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
""",
  )?
  root.write(
    p"proc/self/cgroup",
    """0::/team:blue
""",
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )?
  root.write(
    p"sys/fs/cgroup/team:blue/memory.max",
    """4096
""",
  )?
  root.write(
    p"sys/fs/cgroup/team:blue/memory.current",
    """1024
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let limits = value.memory.cgroup |> where .resource == "memory.max" and .hierarchy_level == 0
  test.eq(limits.len(), 1)?
  test.eq(limits[0].path.value, "/team:blue")?
  test.eq(limits[0].maximum_value, 4096)?
  test.eq(limits[0].current_value, 1024)?

  root.mkdir(p"sys/fs/cgroup/selected", parents: true)?
  root.write(
    p"sys/fs/cgroup/selected/memory.max",
    """8192
""",
  )?
  root.write(
    p"sys/fs/cgroup/selected/memory.current",
    """2048
""",
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 /outside /sys/fs/cgroup/other rw - cgroup2 cgroup rw
32 20 0:25 /team:blue /sys/fs/cgroup/selected rw - cgroup2 cgroup rw
""",
  )?
  let selected = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let selected_limit = (selected.memory.cgroup
    |> where .resource == "memory.max" and .hierarchy_level == 0
    |> first())?
  test.eq(selected_limit.path.value, "/team:blue")?
  test.eq(selected_limit.maximum_value, 8192)?
  test.eq(selected_limit.current_value, 2048)?

  root.write(
    p"proc/self/mountinfo",
    """32 20 0:25 /team:blue /sys/fs/cgroup/selected - cgroup2 cgroup rw
""",
  )?
  let malformed_mount = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(malformed_mount.memory.cgroup, [])?
  test.ok(
    malformed_mount.issues |> any .section == "memory" and .field == "cgroup.mountinfo" and .state == report_model.Malformed,
  )?
  root.write(
    p"proc/self/mountinfo",
    """32 20 0:25 /team:blue /sys/fs/cgroup/selected rw cgroup2 cgroup rw
""",
  )?
  let missing_separator = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(missing_separator.memory.cgroup, [])?
  test.ok(
    missing_separator.issues
      |> any .section == "memory" and .field == "cgroup.mountinfo" and .state == report_model.Malformed,
  )?
  root.write(
    p"proc/self/mountinfo",
    """32 20 0:25 /team:blue /sys/fs/cgroup/selected rw - cgroup2 cgroup rw
""",
  )?

  root.write(
    p"proc/self/cgroup",
    """0::/team:blue
0::/other
""",
  )?
  let duplicate = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(duplicate.memory.cgroup, [])?
  test.ok(
    duplicate.issues |> any .section == "memory" and .field == "cgroup.membership" and .state == report_model.Malformed,
  )?
}

test test_system_report_memory_directory_failures_keep_source_issues [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.mkdir(p"sys/kernel/mm", parents: true)?
  root.mkdir(p"sys/devices/system", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
MemFree: 8 kB
""",
  )?
  root.write(p"sys/kernel/mm/hugepages", "not a directory")?
  root.write(p"sys/devices/system/node", "not a directory")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let top = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(top.memory.host.total_bytes, 16384)?
  for field in ["huge_pages.enumeration", "numa.enumeration"] {
    let matches = top.issues |> where .section == "memory" and .field == field
    test.eq(matches.len(), 1)?
    test.ok(matches[0].state == report_model.ReadFailure)?
  }

  test.eq(top.memory.status.state, report_model.Partial)?

  root.remove(p"sys/devices/system/node")?
  root.mkdir(p"sys/devices/system/node/node0", parents: true)?
  root.mkdir(p"sys/devices/system/node/nodebad", parents: true)?
  root.write(p"sys/devices/system/node/node0/hugepages", "not a directory")?
  let nested = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let nested_issues = nested.issues |> where .section == "memory" and .field == "numa.node0.huge_pages"
  test.eq(nested_issues.len(), 1)?
  test.ok(nested_issues[0].state == report_model.ReadFailure)?
  let meminfo_issues = nested.issues |> where .section == "memory" and .field == "numa.node0.meminfo"
  test.eq(meminfo_issues.len(), 1)?
  test.ok(meminfo_issues[0].state == report_model.Absent)?
  let invalid_node = nested.issues |> where .section == "memory" and .field == "numa.nodebad"
  test.eq(invalid_node.len(), 1)?
  test.ok(invalid_node[0].state == report_model.Malformed)?
}

test test_system_report_huge_page_pools_reject_unsafe_sizes_and_partial_counts [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )?
  root.mkdir(p"sys/kernel/mm/hugepages/hugepages-2048kB", parents: true)?
  root.mkdir(p"sys/kernel/mm/hugepages/hugepages-8796093022207kB", parents: true)?
  root.mkdir(p"sys/kernel/mm/hugepages/hugepages-8796093022208kB", parents: true)?
  root.mkdir(p"sys/kernel/mm/hugepages/hugepages-1024kB", parents: true)?
  root.write(
    p"sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages",
    """2
""",
  )?
  root.write(
    p"sys/kernel/mm/hugepages/hugepages-2048kB/free_hugepages",
    """0x1
""",
  )?
  root.write(
    p"sys/kernel/mm/hugepages/hugepages-8796093022207kB/nr_hugepages",
    """1
""",
  )?
  root.write(
    p"sys/kernel/mm/hugepages/hugepages-8796093022208kB/nr_hugepages",
    """1
""",
  )?
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"${padding}${padding}"
  }

  root.write(
    p"sys/kernel/mm/hugepages/hugepages-1024kB/nr_hugepages",
    f"""1
${padding}""",
  )?
  root.mkdir(p"sys/devices/system/node/node0/hugepages/hugepages-2048kB", parents: true)?
  root.write(
    p"sys/devices/system/node/node0/hugepages/hugepages-2048kB/nr_hugepages",
    """3
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(value.memory.huge_pages.len(), 3)?
  let global = (value.memory.huge_pages
    |> where .node_id == null and .page_size_bytes == 2097152
    |> first())?
  test.eq(global.page_size_bytes, 2097152)?
  test.eq(global.total, 2)?
  test.eq(global.free, null)?
  let safe_edge = (value.memory.huge_pages
    |> where .page_size_bytes == 9007199254739968
    |> first())?
  test.eq(safe_edge.total, 1)?
  let node = (value.memory.huge_pages
    |> where .node_id == 0
    |> first())?
  test.eq(node.page_size_bytes, 2097152)?
  test.eq(node.total, 3)?
  test.ok(
    value.issues |> any .field == "huge_pages.hugepages-8796093022208kB.page_size" and .state == report_model.RangeFailure,
  )?
  test.ok(value.issues |> any .field == "huge_pages.hugepages-1024kB.total" and .state == report_model.Truncated)?
  test.ok(value.issues |> any .field == "huge_pages.hugepages-2048kB.free" and .state == report_model.Malformed)?

  let empty_root = fs.tempdir()?
  defer empty_root.close()?
  empty_root.mkdir(p"proc", parents: true)?
  empty_root.mkdir(p"sys/kernel/mm/hugepages", parents: true)?
  empty_root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )?
  let empty = collector.collect_from_root(empty_root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(empty.memory.huge_pages.len(), 0)?
  test.ok(! (empty.issues |> any .field == "huge_pages.enumeration"))?
}

test test_system_report_numa_meminfo_requires_complete_rows_and_exact_bytes [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.mkdir(p"sys/devices/system/node/node0", parents: true)?
  root.mkdir(p"sys/devices/system/node/node1", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )?
  root.write(
    p"sys/devices/system/node/node0/meminfo",
    """Node 0 MemTotal: 8796093022207 kB
Node 0 MemFree: 8796093022208 kB
Node 0 Vendor: 9007199254740992 widgets
Node 1 Active: 4 kB
Node 0 Broken: 0x10 kB
""",
  )?
  var padding = " "
  while padding.count_chars() < 65536 {
    padding = f"${padding}${padding}"
  }

  root.write(
    p"sys/devices/system/node/node1/meminfo",
    f"""Node 1 MemTotal: 4 kB
${padding}""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(value.memory.numa.len(), 1)?
  test.eq(value.memory.numa[0].name, "node0.MemTotal")?
  test.eq(value.memory.numa[0].value, 9007199254739968)?
  test.eq(value.memory.numa[0].unit, "bytes")?
  test.ok(value.issues |> any .field == "numa.node0.MemFree" and .state == report_model.RangeFailure)?
  test.ok(value.issues |> any .field == "numa.node0.Vendor" and .state == report_model.RangeFailure)?
  test.ok(value.issues |> any .field == "numa.node0.Active" and .state == report_model.Malformed)?
  test.ok(value.issues |> any .field == "numa.node0.Broken" and .state == report_model.Malformed)?
  test.ok(value.issues |> any .field == "numa.node1.meminfo" and .state == report_model.Truncated)?
}

test test_system_report_pressure_keeps_complete_rows_and_unavailable_sources_distinct [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/pressure", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )?
  root.write(
    p"proc/pressure/cpu",
    """some avg10=0.00 avg60=1.50 avg300=2.25 total=9007199254740991
full avg10=nan avg60=0.00 avg300=0.00 total=0
""",
  )?
  root.write(
    p"proc/pressure/memory",
    """some avg10=0.00 avg60=0.00 avg300=0.00 total=9007199254740992
full avg10=0.10 avg60=0.20 avg300=0.30 total=4
full avg10=0.10 avg60=0.20 avg300=0.30 total=5
""",
  )?
  var padding = " "
  while padding.count_chars() < 16384 {
    padding = f"${padding}${padding}"
  }

  root.write(
    p"proc/pressure/io",
    f"""some avg10=0.00 avg60=0.00 avg300=0.00 total=1
${padding}""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(value.memory.pressure.len(), 2)?
  let cpu_pressure = (value.memory.pressure
    |> where .resource == "cpu"
    |> first())?
  test.eq(cpu_pressure.kind, "some")?
  test.eq(cpu_pressure.avg60, "1.50")?
  test.eq(cpu_pressure.total_us, 9007199254740991)?
  test.ok(value.issues |> any .field == "pressure.cpu.full" and .state == report_model.Malformed)?
  let memory = (value.memory.pressure
    |> where .resource == "memory"
    |> first())?
  test.eq(memory.kind, "full")?
  test.eq(memory.total_us, 4)?
  test.ok(value.issues |> any .field == "pressure.memory.some" and .state == report_model.RangeFailure)?
  test.ok(value.issues |> any .field == "pressure.memory.full" and .error_kind == "duplicate_psi_kind")?
  test.ok(value.issues |> any .field == "pressure.io" and .state == report_model.Truncated)?

  let unavailable_root = fs.tempdir()?
  defer unavailable_root.close()?
  unavailable_root.mkdir(p"proc", parents: true)?
  unavailable_root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )?
  let unavailable = collector.collect_from_root(unavailable_root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(unavailable.memory.pressure.len(), 0)?
  for resource in ["cpu", "memory", "io"] {
    test.ok(unavailable.issues |> any .field == f"pressure.${resource}" and .state == report_model.Absent)?
  }
}

test test_system_report_empty_pressure_file_is_malformed_beside_valid_memory_rows [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/pressure", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )?
  root.write(p"proc/pressure/cpu", "")?
  root.write(
    p"proc/pressure/memory",
    """some avg10=0.00 avg60=0.00 avg300=0.00 total=1
full avg10=0.00 avg60=0.00 avg300=0.00 total=0
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq((value.memory.pressure |> where .resource == "cpu").len(), 0)?
  test.eq((value.memory.pressure |> where .resource == "memory").len(), 2)?
  test.ok(
    value.issues
      |> any .section == "memory" and .field == "pressure.cpu" and .state == report_model.Malformed and .error_kind == "empty_psi_source",
  )?
}

test test_system_report_transparent_huge_page_policy_preserves_unknown_selection [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.mkdir(p"sys/kernel/mm/transparent_hugepage", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )?
  root.write(
    p"sys/kernel/mm/transparent_hugepage/enabled",
    """always [future_policy] never
""",
  )?
  root.write(
    p"sys/kernel/mm/transparent_hugepage/defrag",
    """always defer [madvise] never
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(
    value.memory.transparent_huge_pages,
    ["enabled=always [future_policy] never", "defrag=always defer [madvise] never"],
  )?

  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"${padding}${padding}"
  }

  root.write(
    p"sys/kernel/mm/transparent_hugepage/enabled",
    f"""always [future_policy] never
${padding}""",
  )?
  root.write(
    p"sys/kernel/mm/transparent_hugepage/defrag",
    """always defer never
""",
  )?
  let incomplete = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(incomplete.memory.transparent_huge_pages.len(), 0)?
  test.ok(incomplete.issues |> any .field == "transparent_huge_pages.enabled" and .state == report_model.Truncated)?
  test.ok(incomplete.issues |> any .field == "transparent_huge_pages.defrag" and .state == report_model.Malformed)?
}

test test_system_report_hwmon_identity_separates_duplicate_chip_names [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  for entry in ["hwmon0", "hwmon1"] {
    root.mkdir(fp"sys/class/hwmon/${entry}", parents: true)?
    root.write(
      fp"sys/class/hwmon/${entry}/name",
      """same_chip
""",
    )?
    root.write(
      fp"sys/class/hwmon/${entry}/temp1_input",
      """42000
""",
    )?
  }

  let pci_path = p"sys/devices/pci0000:00/0000:00:1f.3"
  root.mkdir(pci_path, parents: true)?
  root.mkdir(p"sys/bus/pci/devices", parents: true)?
  root.symlink(../../../devices/pci0000:00/0000:00:1f.3, p"sys/bus/pci/devices/0000:00:1f.3")?
  for field in [
    {
      name: "vendor",
      value: """0x1234
""",
    },
    {
      name: "device",
      value: """0xabcd
""",
    },
    {
      name: "subsystem_vendor",
      value: """0x1234
""",
    },
    {
      name: "subsystem_device",
      value: """0x0001
""",
    },
    {
      name: "class",
      value: """0x040300
""",
    },
    {
      name: "revision",
      value: """0x01
""",
    },
  ] {
    root.write(fp"${pci_path}/${field.name}", field.value)?
  }

  root.symlink(../../../devices/pci0000:00/0000:00:1f.3, p"sys/class/hwmon/hwmon0/device")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let snapshot = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  let channels = snapshot.sensors.channels
  test.eq(channels.len(), 2)?
  test.ok(channels |> any .chip_entry_name == "hwmon0")?
  test.ok(channels |> any .chip_entry_name == "hwmon1")?
  test.ok(channels |> all .chip == "same_chip")?
  let attached = (channels
    |> where .chip_entry_name == "hwmon0"
    |> first())?
  test.eq(attached.parent_pci_function_index, 0)?
  let sensitive = model.encode_report_json(snapshot, true, false)?
  let legacy = json.remove(json.decode(sensitive)?, ["sensors", "channels", 0, "chip_entry_name"])?
  let replay = model.decode_report_json(json.encode(legacy)?)?
  test.eq(replay.sensors.channels[0].chip_entry_name, null)?
  let invalid_parent = json.set(json.decode(sensitive)?, ["sensors", "channels", 0, "parent_pci_function_index"], 99)?
  test.error_kind(model.decode_report_json(json.encode(invalid_parent)?), "SystemReportError.InvalidJson")?
  root.write(
    p"sys/class/hwmon/hwmon0/inputfoo_input",
    """7
""",
  )?
  let with_unfamiliar_name = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  let unfamiliar = (with_unfamiliar_name.sensors.channels
    |> where .channel == "inputfoo"
    |> first())?
  test.eq(unfamiliar.kind, "unknown")?
  test.eq(unfamiliar.unit, "raw")?
}

test test_system_report_sensor_and_power_sources_keep_raw_units_and_partial_attributes [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/hwmon/hwmon0", parents: true)?
  root.mkdir(p"sys/class/power_supply/BAT0", parents: true)?
  root.mkdir(p"sys/class/powercap/intel-rapl:0", parents: true)?
  root.mkdir(p"sys/class/powercap/intel-rapl", parents: true)?
  root.write(
    p"sys/class/hwmon/hwmon0/name",
    """fixture_hwmon
""",
  )?
  root.write(
    p"sys/class/hwmon/hwmon0/temp1_input",
    """42000
""",
  )?
  root.write(
    p"sys/class/hwmon/hwmon0/temp1_label",
    """CPU Package
""",
  )?
  root.write(
    p"sys/class/hwmon/hwmon0/temp1_max",
    """100000
""",
  )?
  root.write(
    p"sys/class/hwmon/hwmon0/temp1_alarm",
    """0
""",
  )?
  root.write(
    p"sys/class/hwmon/hwmon0/mystery0_input",
    """17
""",
  )?
  root.write(
    p"sys/class/power_supply/BAT0/type",
    """Battery
""",
  )?
  root.write(
    p"sys/class/power_supply/BAT0/status",
    """Charging
""",
  )?
  root.write(
    p"sys/class/power_supply/BAT0/capacity",
    """68
""",
  )?
  root.write(
    p"sys/class/power_supply/BAT0/charge_now",
    """2000000
""",
  )?
  root.write(
    p"sys/class/power_supply/BAT0/charge_full",
    """3000000
""",
  )?
  root.write(
    p"sys/class/power_supply/BAT0/voltage_now",
    """12000000
""",
  )?
  root.write(
    p"sys/class/power_supply/BAT0/current_now",
    """-250000
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/name",
    """package-0
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/energy_uj",
    """123456
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/max_energy_range_uj",
    """999999
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_0_power_limit_uw",
    """45000000
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_0_name",
    """long_term
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_0_time_window_us",
    """1000000
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_1_power_limit_uw",
    """65000000
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_1_name",
    """short_term
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_1_time_window_us",
    """250000
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_2_power_limit_uw",
    """70000000
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_10_power_limit_uw",
    """80000000
""",
  )?
  root.mkdir(p"sys/class/powercap/intel-rapl:0/intel-rapl:0:0", parents: true)?
  root.write(
    p"sys/class/powercap/intel-rapl:0/intel-rapl:0:0/name",
    """core-0
""",
  )?
  root.symlink(p"intel-rapl:0/intel-rapl:0:0", p"sys/class/powercap/intel-rapl:0:0")?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let no_thermal = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  test.eq(no_thermal.sensors.thermal_zones, [])?
  test.eq(no_thermal.sensors.status.state, report_model.Complete)?
  test.eq(no_thermal.sensors.channels.len(), 2)?
  let temperature = (no_thermal.sensors.channels
    |> where .channel == "temp1"
    |> first())?
  test.eq(temperature.value, 42000)?
  test.eq(temperature.unit, "millidegrees_celsius")?
  test.eq(temperature.label.value, "CPU Package")?
  test.eq(temperature.maximum, 100000)?
  test.eq(temperature.alarm, false)?
  let unknown = (no_thermal.sensors.channels
    |> where .channel == "mystery0"
    |> first())?
  test.eq(unknown.kind, "unknown")?
  test.eq(unknown.value, 17)?
  test.eq(unknown.unit, "raw")?

  root.mkdir(p"sys/class/thermal/thermal_zone3", parents: true)?
  root.write(
    p"sys/class/thermal/thermal_zone3/type",
    """fixture_thermal
""",
  )?
  root.write(
    p"sys/class/thermal/thermal_zone3/temp",
    """41000
""",
  )?
  root.write(
    p"sys/class/thermal/thermal_zone3/trip_point_0_temp",
    """95000
""",
  )?
  root.write(
    p"sys/class/thermal/thermal_zone3/trip_point_0_type",
    """critical
""",
  )?
  root.write(
    p"sys/class/thermal/thermal_zone3/trip_point_0_hyst",
    """2000
""",
  )?
  root.write(
    p"sys/class/thermal/thermal_zone3/trip_point_2_temp",
    """85000
""",
  )?
  root.write(
    p"sys/class/thermal/thermal_zone3/trip_point_2_type",
    """passive
""",
  )?
  let thermal = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  test.eq(thermal.sensors.status.state, report_model.Complete)?
  test.eq(thermal.sensors.thermal_zones.len(), 1)?
  test.eq(thermal.sensors.thermal_zones[0].id, 3)?
  test.eq(thermal.sensors.thermal_zones[0].trips |> map .index, [0, 2])?
  test.eq(thermal.sensors.thermal_zones[0].temperature_millidegrees, 41000)?
  test.eq(thermal.sensors.thermal_zones[0].trips[0].temperature_millidegrees, 95000)?
  test.eq(thermal.sensors.thermal_zones[0].trips[0].hysteresis_millidegrees, 2000)?
  root.mkdir(p"sys/class/thermal/thermal_zone03", parents: true)?
  let malformed_zone = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  test.eq(malformed_zone.sensors.thermal_zones.len(), 1)?
  test.ok(malformed_zone.issues |> any .field == "thermal_zones.thermal_zone03" and .state == report_model.Malformed)?
  root.write(
    p"sys/class/thermal/thermal_zone3/trip_point_02_temp",
    """85000
""",
  )?
  let malformed_trip = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  test.eq(malformed_trip.sensors.thermal_zones[0].trips |> map .index, [0, 2])?
  test.ok(
    malformed_trip.issues
      |> any .field == "thermal_zones.thermal_zone3.trip_point_02_temp" and .state == report_model.Malformed,
  )?

  let power = collector.collect_from_root(root, "fixture-arch", 4096, 100, "power", true)?
  test.eq(power.power.status.state, report_model.Complete)?
  test.eq(power.power.supplies.len(), 1)?
  test.eq(power.power.supplies[0].capacity_percent, 68)?
  test.eq(power.power.supplies[0].charge_now_uah, 2000000)?
  test.eq(power.power.supplies[0].current_now_ua, -250000)?
  test.ok(power.power.supplies[0].energy_now_uwh == null)?
  test.eq(power.power.cap_zones.len(), 2)?
  let package_zone = (power.power.cap_zones
    |> where .name == "package-0"
    |> first())?
  test.eq(package_zone.entry_name, "intel-rapl:0")?
  test.ok(package_zone.parent == null)?
  test.eq(package_zone.energy_uj, 123456)?
  test.eq(package_zone.constraints.len(), 4)?
  test.eq(package_zone.constraints |> map .index, [0, 1, 2, 10])?
  let long_term = (package_zone.constraints
    |> where .index == 0
    |> first())?
  test.eq(long_term.name, "long_term")?
  test.eq(long_term.power_limit_uw, 45000000)?
  test.eq(long_term.time_window_us, 1000000)?
  let short_term = (package_zone.constraints
    |> where .index == 1
    |> first())?
  test.eq(short_term.name, "short_term")?
  test.eq(short_term.power_limit_uw, 65000000)?
  test.eq(short_term.time_window_us, 250000)?
  let core_zone = (power.power.cap_zones
    |> where .name == "core-0"
    |> first())?
  test.eq(core_zone.entry_name, "intel-rapl:0:0")?
  test.eq(core_zone.parent, "intel-rapl:0")?
}

test test_system_report_sensor_units_and_powercap_ranges_are_bounded [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/hwmon/hwmon0", parents: true)?
  root.mkdir(p"sys/class/power_supply/BAT0", parents: true)?
  root.mkdir(p"sys/class/powercap/intel-rapl:0", parents: true)?
  root.write(
    p"sys/class/hwmon/hwmon0/name",
    """fixture_hwmon
""",
  )?
  root.write(
    p"sys/class/hwmon/hwmon0/temp1_input",
    """42 C
""",
  )?
  root.write(
    p"sys/class/power_supply/BAT0/capacity",
    """101
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/name",
    """package-0
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/energy_uj",
    """9007199254740992
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/max_energy_range_uj",
    """999999999999999999999999
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_0_power_limit_uw",
    """-1
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_0_time_window_us",
    """1000000
""",
  )?
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_bad_power_limit_uw",
    """1000000
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let sensors = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  test.eq(sensors.sensors.channels.len(), 1)?
  test.ok(sensors.sensors.channels[0].value == null)?
  let sensor_issues = sensors.issues |> where .section == "sensors" and .field == "hwmon.hwmon0.temp1_input"
  test.eq(sensor_issues.len(), 1)?
  test.ok(sensor_issues[0].state == report_model.Malformed)?

  let power = collector.collect_from_root(root, "fixture-arch", 4096, 100, "power", true)?
  test.ok(power.power.supplies[0].capacity_percent == null)?
  let capacity_issues = power.issues |> where .section == "power" and .field == "supplies.BAT0.capacity"
  test.eq(capacity_issues.len(), 1)?
  test.ok(capacity_issues[0].state == report_model.Malformed)?
  test.eq(power.power.cap_zones.len(), 1)?
  test.ok(power.power.cap_zones[0].energy_uj == null)?
  test.ok(power.power.cap_zones[0].maximum_energy_range_uj == null)?
  test.eq(power.power.cap_zones[0].constraints.len(), 1)?
  test.ok(power.power.cap_zones[0].constraints[0].power_limit_uw == null)?
  test.eq(power.power.cap_zones[0].constraints[0].time_window_us, 1000000)?
  for field in ["cap_zones.intel-rapl:0.energy_uj", "cap_zones.intel-rapl:0.max_energy_range_uj"] {
    let matches = power.issues |> where .section == "power" and .field == field
    test.eq(matches.len(), 1)?
    test.ok(matches[0].state == report_model.RangeFailure)?
  }

  let negative_limit = power.issues
    |> where .section == "power" and .field == "cap_zones.intel-rapl:0.constraint_0_power_limit_uw"
  test.eq(negative_limit.len(), 1)?
  test.ok(negative_limit[0].state == report_model.Malformed)?
  let invalid_index = power.issues
    |> where .section == "power" and .field == "cap_zones.intel-rapl:0.constraint_bad_power_limit_uw"
  test.eq(invalid_index.len(), 1)?
  test.eq(invalid_index[0].error_kind, "invalid_constraint_index")?
}

test test_system_report_thermal_and_battery_reads_reject_truncated_prefixes [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/hwmon/hwmon0", parents: true)?
  root.mkdir(p"sys/class/thermal/thermal_zone0", parents: true)?
  root.mkdir(p"sys/class/power_supply/BAT0", parents: true)?
  root.write(
    p"sys/class/hwmon/hwmon0/temp1_input",
    """-5000
""",
  )?
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"${padding}${padding}"
  }

  root.write(p"sys/class/hwmon/hwmon0/name", f"chip-prefix${padding}")?
  root.write(p"sys/class/thermal/thermal_zone0/temp", f"41000${padding}")?
  root.write(p"sys/class/thermal/thermal_zone0/type", f"zone-prefix${padding}")?
  root.write(
    p"sys/class/thermal/thermal_zone0/trip_point_0_temp",
    """95000
""",
  )?
  root.write(p"sys/class/thermal/thermal_zone0/trip_point_0_type", f"critical${padding}")?
  root.write(p"sys/class/power_supply/BAT0/capacity", f"68${padding}")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let sensors = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  test.eq(sensors.sensors.channels[0].value, -5000)?
  test.eq(sensors.sensors.channels[0].chip, "hwmon0")?
  test.ok(sensors.sensors.thermal_zones[0].temperature_millidegrees == null)?
  test.ok(sensors.sensors.thermal_zones[0].kind == null)?
  test.eq(sensors.sensors.thermal_zones[0].trips[0].kind, "unknown")?
  for field in [
    "hwmon.hwmon0.name",
    "thermal_zones.thermal_zone0.type",
    "thermal_zones.thermal_zone0.trip_point_0_type",
  ] {
    let matches = sensors.issues |> where .section == "sensors" and .field == field
    test.eq(matches.len(), 1)?
    test.ok(matches[0].state == report_model.Truncated)?
  }

  let thermal_issues = sensors.issues |> where .section == "sensors" and .field == "thermal_zones.thermal_zone0.temp"
  test.eq(thermal_issues.len(), 1)?
  test.ok(thermal_issues[0].state == report_model.Truncated)?

  let power = collector.collect_from_root(root, "fixture-arch", 4096, 100, "power", true)?
  test.ok(power.power.supplies[0].capacity_percent == null)?
  let battery_issues = power.issues |> where .section == "power" and .field == "supplies.BAT0.capacity"
  test.eq(battery_issues.len(), 1)?
  test.ok(battery_issues[0].state == report_model.Truncated)?
}

test test_system_report_nested_sensor_and_power_directories_keep_issues [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/hwmon", parents: true)?
  root.mkdir(p"sys/class/thermal", parents: true)?
  root.mkdir(p"sys/class/powercap", parents: true)?
  root.write(p"sys/class/hwmon/hwmon0", "not a directory")?
  root.write(p"sys/class/thermal/thermal_zone0", "not a directory")?
  root.write(p"sys/class/powercap/intel-rapl:0", "not a directory")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let sensors = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  for field in ["hwmon.hwmon0.attributes", "thermal_zones.thermal_zone0.attributes"] {
    let matches = sensors.issues |> where .section == "sensors" and .field == field
    test.eq(matches.len(), 1)?
    test.ok(matches[0].state == report_model.ReadFailure)?
  }

  test.eq(sensors.sensors.status.state, report_model.Partial)?

  let power = collector.collect_from_root(root, "fixture-arch", 4096, 100, "power", true)?
  for field in ["cap_zones.intel-rapl:0.name", "cap_zones.intel-rapl:0.attributes"] {
    let matches = power.issues |> where .section == "power" and .field == field
    test.eq(matches.len(), 1)?
    test.ok(matches[0].state == report_model.ReadFailure)?
  }

  test.eq(power.power.status.state, report_model.Partial)?
}

test test_system_report_powercap_enumeration_failure_is_partial [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class", parents: true)?
  root.write(p"sys/class/powercap", "not a directory")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "power", true)?
  let matches = value.issues |> where .section == "power" and .field == "cap_zones"
  test.eq(matches.len(), 1)?
  test.ok(matches[0].state == report_model.ReadFailure)?
  test.eq(value.power.status.state, report_model.Partial)?
  test.eq(value.power.status.enumeration_succeeded, false)?
}

test test_system_report_powercap_rejects_truncated_names [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/powercap/intel-rapl:0", parents: true)?
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"${padding}${padding}"
  }

  root.write(p"sys/class/powercap/intel-rapl:0/name", f"package-prefix${padding}")?
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_0_power_limit_uw",
    """45000000
""",
  )?
  root.write(p"sys/class/powercap/intel-rapl:0/constraint_0_name", f"limit-prefix${padding}")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "power", true)?
  test.eq(value.power.cap_zones.len(), 1)?
  test.eq(value.power.cap_zones[0].name, "intel-rapl:0")?
  test.ok(value.power.cap_zones[0].constraints[0].name == null)?
  for field in ["cap_zones.intel-rapl:0.name", "cap_zones.intel-rapl:0.constraint_0_name"] {
    let matches = value.issues |> where .section == "power" and .field == field
    test.eq(matches.len(), 1)?
    test.ok(matches[0].state == report_model.Truncated)?
  }

  test.eq(value.power.status.state, report_model.Partial)?
}

test test_system_report_power_supply_rejects_truncated_text_attributes [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/power_supply/BAT0", parents: true)?
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"${padding}${padding}"
  }

  root.write(p"sys/class/power_supply/BAT0/type", f"Battery${padding}")?
  root.write(p"sys/class/power_supply/BAT0/status", f"Charging${padding}")?
  root.write(p"sys/class/power_supply/BAT0/health", f"Good${padding}")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "power", true)?
  test.eq(value.power.supplies.len(), 1)?
  test.ok(value.power.supplies[0].kind == null)?
  test.ok(value.power.supplies[0].status == null)?
  test.ok(value.power.supplies[0].health == null)?
  for field in ["supplies.BAT0.type", "supplies.BAT0.status", "supplies.BAT0.health"] {
    let matches = value.issues |> where .section == "power" and .field == field
    test.eq(matches.len(), 1)?
    test.ok(matches[0].state == report_model.Truncated)?
  }

  test.eq(value.power.status.state, report_model.Partial)?
}

test test_system_report_memory_reports_malformed_and_oversized_meminfo_fields [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.write(
    p"proc/meminfo",
    """UnframedField
MemTotal: 16 widgets
MemFree:
MemAvailable: 8 kB
Active: 8796093022207 kB
Cached: 8796093022208 kB
VendorCounter: 12 widgets
VendorHuge: 9007199254740992 widgets
""",
  )?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.ok(value.memory.host.total_bytes == null)?
  test.ok(value.memory.host.free_bytes == null)?
  test.ok(value.memory.host.cached_bytes == null)?
  test.eq(value.memory.host.available_bytes, 8192)?
  test.eq(value.memory.host.active_bytes, 9007199254739968)?
  let invalid_row = value.issues |> where .field == "meminfo.line.0"
  test.eq(invalid_row.len(), 1)?
  test.eq(invalid_row[0].state, report_model.Malformed)?
  let invalid_unit = value.issues |> where .field == "meminfo.MemTotal"
  test.eq(invalid_unit[0].error_kind, "invalid_byte_counter_unit")?
  let empty_value = value.issues |> where .field == "meminfo.MemFree"
  test.eq(empty_value[0].error_kind, "missing_integer")?
  let overflow = value.issues |> where .field == "meminfo.Cached"
  test.eq(overflow[0].state, report_model.RangeFailure)?
  let vendor_overflow = value.issues |> where .field == "meminfo.VendorHuge"
  test.eq(vendor_overflow.len(), 1)?
  test.eq(vendor_overflow[0].state, report_model.RangeFailure)?
  test.eq(vendor_overflow[0].error_kind, "json_integer_out_of_range")?
  test.ok(! (value.memory.host.counters |> any .name == "VendorHuge"))?
  let vendor = value.memory.host.counters |> where .name == "VendorCounter"
  test.eq(vendor.len(), 1)?
  test.eq(vendor[0].value, 12)?
  test.eq(vendor[0].unit, "widgets")?
}

test test_system_report_memory_accepts_tabbed_values_and_withholds_duplicate_fields [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal:	16	kB
MemTotal: 32 kB
MemFree:	4	kB
VendorCounter:	12	widgets
""",
  )?

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(value.memory.host.total_bytes, null)?
  test.eq(value.memory.host.free_bytes, 4096)?
  test.ok(! (value.memory.host.counters |> any .name == "MemTotal"))?
  let free = value.memory.host.counters |> where .name == "MemFree"
  test.eq(free.len(), 1)?
  test.eq(free[0].value, 4096)?
  test.eq(free[0].unit, "bytes")?
  let vendor = value.memory.host.counters |> where .name == "VendorCounter"
  test.eq(vendor.len(), 1)?
  test.eq(vendor[0].value, 12)?
  test.eq(vendor[0].unit, "widgets")?
  let duplicates = value.issues |> where .field == "meminfo.MemTotal" and .error_kind == "duplicate_field"
  test.eq(duplicates.len(), 1)?
}

test test_system_report_cgroup_inventory_rejects_partial_membership_and_mounts [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)?
  root.mkdir(p"sys/fs/cgroup/group", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )?
  root.write(
    p"sys/fs/cgroup/group/memory.max",
    """1048576
""",
  )?
  root.write(
    p"sys/fs/cgroup/group/memory.current",
    """512
""",
  )?
  var membership_padding = " "
  while membership_padding.count_chars() < 65536 {
    membership_padding = f"${membership_padding}${membership_padding}"
  }

  root.write(
    p"proc/self/cgroup",
    f"""0::/group
${membership_padding}""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let membership = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(membership.memory.cgroup.len(), 0)?
  test.ok(
    membership.issues |> any .section == "memory" and .field == "cgroup.membership" and .state == report_model.Truncated,
  )?

  root.write(
    p"proc/self/cgroup",
    """0::/group
""",
  )?
  var mount_padding = " "
  while mount_padding.count_chars() < 4194304 {
    mount_padding = f"${mount_padding}${mount_padding}"
  }

  root.write(
    p"proc/self/mountinfo",
    f"""31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
${mount_padding}""",
  )?
  let mount = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(mount.memory.cgroup.len(), 0)?
  test.ok(
    mount.issues |> any .section == "memory" and .field == "cgroup.mountinfo" and .state == report_model.Truncated,
  )?
}

test test_system_report_cgroup_limits_keep_source_and_numeric_failures [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)?
  root.mkdir(p"sys/fs/cgroup/group", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )?
  root.write(
    p"proc/self/cgroup",
    """0::/group
""",
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )?
  root.write(
    p"sys/fs/cgroup/group/memory.max",
    """1048576
""",
  )?
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"${padding}${padding}"
  }

  root.write(
    p"sys/fs/cgroup/group/memory.current",
    f"""512
${padding}""",
  )?
  root.write(
    p"sys/fs/cgroup/group/memory.swap.max",
    """0x10
""",
  )?
  root.write(
    p"sys/fs/cgroup/group/memory.swap.current",
    """65536
""",
  )?
  root.write(
    p"sys/fs/cgroup/group/pids.max",
    """9007199254740992
""",
  )?
  root.write(
    p"sys/fs/cgroup/group/pids.current",
    """8
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let memory_limit = (value.memory.cgroup
    |> where .resource == "memory.max"
    |> first())?
  test.eq(memory_limit.maximum_value, 1048576)?
  test.eq(memory_limit.current_value, null)?
  test.eq(memory_limit.state, report_model.Truncated)?
  let swap_limit = (value.memory.cgroup
    |> where .resource == "memory.swap.max"
    |> first())?
  test.eq(swap_limit.maximum_value, null)?
  test.eq(swap_limit.current_value, 65536)?
  test.eq(swap_limit.state, report_model.Malformed)?
  let pids_limit = (value.memory.cgroup
    |> where .resource == "pids.max"
    |> first())?
  test.eq(pids_limit.maximum_value, null)?
  test.eq(pids_limit.current_value, 8)?
  test.eq(pids_limit.state, report_model.RangeFailure)?
  test.ok(value.issues |> any .field == "cgroup.0.memory.current" and .state == report_model.Truncated)?
  test.ok(value.issues |> any .field == "cgroup.0.memory.swap.max" and .state == report_model.Malformed)?
  test.ok(value.issues |> any .field == "cgroup.0.pids.max" and .state == report_model.RangeFailure)?
}

test test_system_report_cgroup_cpu_and_io_counters_reject_partial_and_unsafe_values [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)?
  root.mkdir(p"sys/fs/cgroup/group", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )?
  root.write(
    p"proc/self/cgroup",
    """0::/group
""",
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )?
  root.write(
    p"sys/fs/cgroup/group/cpu.max",
    """50000 100000
""",
  )?
  root.write(
    p"sys/fs/cgroup/group/cpu.stat",
    """usage_usec 9007199254740992
user_usec 3
""",
  )?
  root.write(
    p"sys/fs/cgroup/group/io.stat",
    """8:0 rbytes=9007199254740992 wbytes=1024
""",
  )?
  var cpuset_padding = " "
  while cpuset_padding.count_chars() < 65536 {
    cpuset_padding = f"${cpuset_padding}${cpuset_padding}"
  }

  root.write(
    p"sys/fs/cgroup/group/cpuset.cpus.effective",
    f"""0-1
${cpuset_padding}""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let cpu_limit = (value.memory.cgroup
    |> where .resource == "cpu.max"
    |> first())?
  test.eq(cpu_limit.quota, 50000)?
  test.eq(cpu_limit.period, 100000)?
  test.ok(value.memory.cgroup |> any .resource == "cpu.stat.user_usec" and .current_value == 3)?
  test.ok(! (value.memory.cgroup |> any .resource == "cpu.stat.usage_usec"))?
  test.ok(value.memory.cgroup |> any .resource == "io.stat.8:0.wbytes" and .current_value == 1024)?
  test.ok(! (value.memory.cgroup |> any .resource == "io.stat.8:0.rbytes"))?
  test.ok(! (value.memory.cgroup |> any .resource == "cpuset.cpus.effective"))?
  test.ok(value.issues |> any .field == "cgroup.0.cpu.stat.usage_usec" and .state == report_model.RangeFailure)?
  test.ok(value.issues |> any .field == "cgroup.0.io.stat.8:0.rbytes" and .state == report_model.RangeFailure)?
  test.ok(value.issues |> any .field == "cgroup.0.cpuset.cpus.effective" and .state == report_model.Truncated)?

  root.write(
    p"sys/fs/cgroup/group/cpu.stat",
    """usage_usec 1
usage_usec 2
user_usec 3
""",
  )?
  root.write(
    p"sys/fs/cgroup/group/io.stat",
    """8:0 rbytes=1 rbytes=2 wbytes=4
""",
  )?
  let duplicate_fields = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.ok(! (duplicate_fields.memory.cgroup |> any .resource == "cpu.stat.usage_usec"))?
  test.ok(duplicate_fields.memory.cgroup |> any .resource == "cpu.stat.user_usec" and .current_value == 3)?
  test.ok(! (duplicate_fields.memory.cgroup |> any .resource == "io.stat.8:0.rbytes"))?
  test.ok(duplicate_fields.memory.cgroup |> any .resource == "io.stat.8:0.wbytes" and .current_value == 4)?
  test.ok(duplicate_fields.issues |> any .field == "cgroup.0.cpu.stat.usage_usec" and .state == report_model.Malformed)?
  test.ok(duplicate_fields.issues |> any .field == "cgroup.0.io.stat.8:0.rbytes" and .state == report_model.Malformed)?

  root.write(
    p"sys/fs/cgroup/group/io.stat",
    """8:0 rbytes=1
8:0 wbytes=2
9:0 rbytes=7
""",
  )?
  let duplicate_device = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.ok(! (duplicate_device.memory.cgroup |> any .resource.starts_with("io.stat.8:0.")))?
  test.ok(duplicate_device.memory.cgroup |> any .resource == "io.stat.9:0.rbytes" and .current_value == 7)?
  test.ok(duplicate_device.issues |> any .field == "cgroup.0.io.stat.8:0" and .state == report_model.Malformed)?

  root.write(
    p"sys/fs/cgroup/group/io.stat",
    "7:7 " + """
8:0 rbytes=17
""",
  )?
  let empty_device = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.ok(empty_device.memory.cgroup |> any .resource == "io.stat.8:0.rbytes" and .current_value == 17)?
  test.ok(! (empty_device.memory.cgroup |> any .resource.starts_with("io.stat.7:7.")))?
  test.ok(! (empty_device.issues |> any .field == "cgroup.0.io.stat"))?

  var cpu_padding = " "
  while cpu_padding.count_chars() < 4096 {
    cpu_padding = f"${cpu_padding}${cpu_padding}"
  }

  root.write(
    p"sys/fs/cgroup/group/cpu.max",
    f"""50000 100000
${cpu_padding}""",
  )?
  let truncated = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.ok(! (truncated.memory.cgroup |> any .resource == "cpu.max"))?
  test.ok(truncated.issues |> any .field == "cgroup.0.cpu.max" and .state == report_model.Truncated)?

  var cpu_stat_padding = " "
  while cpu_stat_padding.count_chars() < 16384 {
    cpu_stat_padding = f"${cpu_stat_padding}${cpu_stat_padding}"
  }

  var io_padding = " "
  while io_padding.count_chars() < 262144 {
    io_padding = f"${io_padding}${io_padding}"
  }

  root.write(
    p"sys/fs/cgroup/group/cpu.stat",
    f"""user_usec 3
${cpu_stat_padding}""",
  )?
  root.write(
    p"sys/fs/cgroup/group/io.stat",
    f"""8:0 wbytes=1024
${io_padding}""",
  )?
  let incomplete_counters = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.ok(! (incomplete_counters.memory.cgroup |> any .resource == "cpu.stat.user_usec"))?
  test.ok(! (incomplete_counters.memory.cgroup |> any .resource == "io.stat.8:0.wbytes"))?
  test.ok(incomplete_counters.issues |> any .field == "cgroup.0.cpu.stat" and .state == report_model.Truncated)?
  test.ok(incomplete_counters.issues |> any .field == "cgroup.0.io.stat" and .state == report_model.Truncated)?
}

test test_system_report_cgroup_hybrid_keeps_v2_values_and_v1_limitation [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)?
  root.mkdir(p"sys/fs/cgroup/group", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )?
  root.write(
    p"proc/self/cgroup",
    """0::/group
2:cpu:/legacy
""",
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
32 20 0:26 / /sys/fs/cgroup/cpu rw - cgroup cgroup rw,cpu
""",
  )?
  root.write(
    p"sys/fs/cgroup/group/memory.max",
    """1048576
""",
  )?
  root.write(
    p"sys/fs/cgroup/group/memory.current",
    """512
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let memory_limit = (value.memory.cgroup
    |> where .resource == "memory.max"
    |> first())?
  test.eq(memory_limit.maximum_value, 1048576)?
  test.eq(memory_limit.current_value, 512)?
  test.ok(value.issues |> any .field == "cgroup.v1" and .state == report_model.Unsupported)?

  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )?
  let hidden_v1_mount = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.ok(hidden_v1_mount.memory.cgroup |> any .resource == "memory.max" and .maximum_value == 1048576)?
  test.ok(hidden_v1_mount.issues |> any .field == "cgroup.v1" and .state == report_model.Unsupported)?
}

test test_system_report_memory_marks_an_empty_meminfo_file_malformed [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.write(p"proc/meminfo", "")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.ok(value.memory.host.total_bytes == null)?
  let empty_file = value.issues |> where .field == "meminfo"
  test.eq(empty_file.len(), 1)?
  test.eq(empty_file[0].state, report_model.Malformed)?
}

test test_system_report_memory_does_not_parse_truncated_meminfo_prefix [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  var padding = " "
  while padding.count_chars() < 1048576 {
    padding = f"${padding}${padding}"
  }

  root.write(
    p"proc/meminfo",
    f"""MemTotal: 16 kB
${padding}""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.ok(value.memory.host.total_bytes == null)?
  let matches = value.issues |> where .section == "memory" and .field == "meminfo"
  test.eq(matches.len(), 1)?
  test.ok(matches[0].state == report_model.Truncated)?
}

test test_system_report_swap_devices_keep_exact_bytes_and_reject_partial_sources [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )?
  root.write(
    p"proc/swaps",
    """Filename	Type	Size	Used	Priority
/dev/zram0	partition	8796093022207	1	42
/swap\\040file	file	4	0	-1
/dev/zram1	partition	8796093022208	0	10
/dev/broken partition 4
/dev/badprio	partition	4	0	invalid
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(value.memory.swaps.len(), 2)?
  test.eq(value.memory.swaps[0].name.value, "/dev/zram0")?
  test.eq(value.memory.swaps[0].size_bytes, 9007199254739968)?
  test.eq(value.memory.swaps[0].used_bytes, 1024)?
  test.eq(value.memory.swaps[0].priority, 42)?
  test.eq(value.memory.swaps[1].name.value, "/swap file")?
  test.eq(value.memory.swaps[1].kind, "file")?
  test.eq(value.memory.swaps[1].priority, -1)?
  let overflow = value.issues
    |> where .section == "memory" and .field == "swaps" and .state == report_model.RangeFailure
  test.eq(overflow.len(), 1)?
  test.ok(overflow[0].state == report_model.RangeFailure)?
  let malformed = value.issues |> where .section == "memory" and .field == "swaps" and .error_kind == "invalid_swap_row"
  test.eq(malformed.len(), 1)?
  let invalid_priority = value.issues
    |> where .section == "memory" and .field == "swaps" and .error_kind == "invalid_swap_priority"
  test.eq(invalid_priority.len(), 1)?

  var padding = " "
  while padding.count_chars() < 262144 {
    padding = f"${padding}${padding}"
  }

  root.write(
    p"proc/swaps",
    f"""Filename Type Size Used Priority
/dev/zram0 partition 4 1 42
${padding}""",
  )?
  let truncated = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(truncated.memory.swaps.len(), 0)?
  let incomplete = truncated.issues |> where .section == "memory" and .field == "swaps"
  test.eq(incomplete.len(), 1)?
  test.ok(incomplete[0].state == report_model.Truncated)?

  root.write(
    p"proc/swaps",
    """/dev/zram0 partition 4 1 42
""",
  )?
  let headerless = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(headerless.memory.swaps.len(), 0)?
  test.ok(headerless.issues |> any .field == "swaps" and .error_kind == "invalid_swap_header")?
}

test test_system_report_swap_devices_reject_duplicate_identity_and_impossible_usage [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )?
  root.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
/dev/zram0 partition 4 1 42
/dev/zram0 partition 4 0 42
/dev/bad partition 4 5 1
/swap\\040file file 8 0 -1
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  test.eq(value.memory.swaps |> map .name.value, ["/dev/zram0", "/swap file"])?
  let duplicates = value.issues
    |> where .section == "memory" and .field == "swaps" and .error_kind == "duplicate_swap_name"
  let overused = value.issues
    |> where .section == "memory" and .field == "swaps" and .error_kind == "invalid_swap_usage"
  test.eq(duplicates.len(), 1)?
  test.eq(overused.len(), 1)?
  test.ok(value.memory.status.state == report_model.Partial)?
}

test test_system_report_kernel_modules_reject_truncated_source_prefix [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.write(
    p"proc/cmdline",
    """quiet
""",
  )?
  var padding = " "
  while padding.count_chars() < 1048576 {
    padding = f"${padding}${padding}"
  }

  root.write(
    p"proc/modules",
    f"""example 4096 0 - Live 0x0
${padding}""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "kernel", true)?
  test.eq(value.kernel.modules, [])?
  test.ok(! value.kernel.status.enumeration_succeeded)?
  let matches = value.issues |> where .section == "kernel" and .field == "modules"
  test.eq(matches.len(), 1)?
  test.ok(matches[0].state == report_model.Truncated)?
}

test test_system_report_kernel_modules_keep_valid_rows_with_malformed_neighbor [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.write(
    p"proc/cmdline",
    """quiet
""",
  )?
  root.write(
    p"proc/modules",
    """example 4096 1 - Live 0x0
broken row
large 9007199254740992 0 - Live 0x0
busy 4096 9007199254740992 - Live 0x0
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "kernel", true)?
  test.ok(value.kernel.status.enumeration_succeeded)?
  test.ok(value.kernel.status.state == report_model.Partial)?
  test.eq(value.kernel.modules.len(), 1)?
  test.eq(value.kernel.modules[0].name, "example")?
  test.eq(value.kernel.modules[0].size_bytes, 4096)?
  let malformed = value.issues |> where .section == "kernel" and .field == "modules.line.1"
  test.eq(malformed.len(), 1)?
  test.ok(malformed[0].state == report_model.Malformed)?
  let oversized = value.issues |> where .section == "kernel" and .state == report_model.RangeFailure
  test.eq(oversized.len(), 2)?
  test.ok(oversized |> any .field == "modules.line.2")?
  test.ok(oversized |> any .field == "modules.line.3")?
}

test test_system_report_kernel_modules_accept_taint_flags_and_reject_extra_columns [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.write(
    p"proc/cmdline",
    """quiet
""",
  )?
  root.write(
    p"proc/modules",
    """tainted 4096 1 - Live 0x0 (OE)
extra 2048 0 - Live 0x1 (OE) unknown
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "kernel", true)?
  test.ok(value.kernel.status.enumeration_succeeded)?
  test.eq(value.kernel.modules.len(), 1)?
  test.eq(value.kernel.modules[0].name, "tainted")?
  test.eq(value.kernel.modules[0].users, 1)?
  let malformed = value.issues |> where .section == "kernel" and .field == "modules.line.1"
  test.eq(malformed.len(), 1)?
  test.ok(malformed[0].state == report_model.Malformed)?
}

test test_system_report_kernel_modules_preserve_unavailable_use_count [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.write(
    p"proc/cmdline",
    """quiet
""",
  )?
  root.write(
    p"proc/modules",
    """permanent 4096 - - Live 0x0
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "kernel", true)?
  test.ok(value.kernel.status.enumeration_succeeded)?
  test.eq(value.kernel.modules.len(), 1)?
  test.eq(value.kernel.modules[0].name, "permanent")?
  test.eq(value.kernel.modules[0].users, null)?
  test.eq((value.issues |> where .section == "kernel" and .field.starts_with("modules")).len(), 0)?
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  test.contains(model.render_text(value, true, true)?, "\"permanent\" size=4096 bytes users=unknown state=\"Live\"")?
}

test test_system_report_kernel_modules_reject_duplicate_identity_with_valid_neighbor [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  root.write(
    p"proc/cmdline",
    """quiet
""",
  )?
  root.write(
    p"proc/modules",
    """alpha 4096 1 - Live 0x0
alpha 4096 2 - Live 0x1
beta 8192 0 - Live 0x2
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "kernel", true)?
  test.ok(value.kernel.status.enumeration_succeeded)?
  test.ok(value.kernel.status.state == report_model.Partial)?
  test.eq(value.kernel.modules |> map .name, ["alpha", "beta"])?
  test.eq(value.kernel.modules[0].users, 1)?
  let duplicates = value.issues |> where .section == "kernel" and .field == "modules.line.1"
  test.eq(duplicates.len(), 1)?
  test.ok(duplicates[0].state == report_model.Malformed)?
  test.eq(duplicates[0].error_kind, "duplicate_module_name")?
}

test test_system_report_kernel_command_line_preserves_source_whitespace [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)?
  let source = "  root=UUID=private  quiet  " + "\n"
  root.write(p"proc/cmdline", source)?
  root.write(p"proc/modules", "")?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "kernel", true)?
  test.eq(value.kernel.command_line.state, report_model.Observed)?
  test.eq(value.kernel.command_line.value, source)?
  let redacted = report_model.redact_report(value)
  test.eq(redacted.kernel.command_line.state, report_model.Redacted)?
  test.eq(redacted.kernel.command_line.value, null)?
  test.eq(redacted.kernel.command_line.raw_bytes_base64, null)?
}

test test_system_report_source_text_preserves_exact_whitespace_when_requested [fs, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.write(
    p"cmdline",
    "  root=private  quiet  " + "\n",
  )?
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let normalized = collectors.read_source_text(root, p"cmdline")
  let exact = collectors.read_source_text(root, p"cmdline", 65536, true)
  test.eq(normalized.observation.value, "root=private  quiet")?
  test.eq(
    exact.observation.value,
    "  root=private  quiet  " + "\n",
  )?
}

test test_system_report_kernel_parameter_allowlist_keeps_values_and_absence [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/sys/kernel", parents: true)?
  root.mkdir(p"sys/module/usbcore/parameters", parents: true)?
  root.write(
    p"proc/cmdline",
    """quiet
""",
  )?
  root.write(p"proc/modules", "")?
  root.write(
    p"proc/sys/kernel/pid_max",
    """4194304
""",
  )?
  root.write(
    p"sys/module/usbcore/parameters/autosuspend",
    """2
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "kernel", true)?
  test.eq(value.kernel.sysctls.len(), 6)?
  test.eq(value.kernel.parameters.len(), 3)?
  test.eq(value.kernel.sysctls[0].name, "kernel.pid_max")?
  test.eq(value.kernel.sysctls[0].value.value, "4194304")?
  test.eq(value.kernel.sysctls[1].value.state, report_model.Absent)?
  test.eq(value.kernel.parameters[0].name, "usbcore.autosuspend")?
  test.eq(value.kernel.parameters[0].value.value, "2")?
  test.eq(value.kernel.parameters[1].value.state, report_model.Absent)?
}

test test_system_report_collects_swap_limit_without_memory_limit_files [fs, time, error] {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)?
  root.mkdir(p"sys/fs/cgroup/group", parents: true)?
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )?
  root.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
""",
  )?
  root.write(
    p"proc/self/cgroup",
    """0::/group
""",
  )?
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw,nosuid,nodev - cgroup2 cgroup rw
""",
  )?
  root.write(
    p"sys/fs/cgroup/group/memory.swap.max",
    """262144
""",
  )?
  root.write(
    p"sys/fs/cgroup/group/memory.swap.current",
    """65536
""",
  )?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let swap_limit = (value.memory.cgroup
    |> where .resource == "memory.swap.max"
    |> first())?
  test.eq(swap_limit.maximum_value, 262144)?
  test.eq(swap_limit.current_value, 65536)?
}

test test_system_report_smbios_parser_preserves_records_and_reports_bad_string_indexes [fs, time, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let table = b"\x01\x084\x12\x01\x02\x03\0Vendor\0Model\0Version\0\0\x7f\x04\0\0\0\0"
  let parsed = collector.parse_smbios_table(table)?
  test.eq(parsed.truncated, false)?
  test.eq(parsed.issues, [])?
  test.eq(parsed.records.len(), 2)?
  test.eq(parsed.records[0].record_type, 1)?
  test.eq(parsed.records[0].handle, 4660)?
  test.eq(parsed.records[0].strings.len(), 3)?
  test.eq(parsed.records[0].strings[1].value, "Model")?
  test.eq(parsed.records[1].record_type, 127)?

  let bad_index = b"\x01\x084\x12\x04\x02\x03\0Vendor\0Model\0Version\0\0\x7f\x04\0\0\0\0"
  let partial = collector.parse_smbios_table(bad_index)?
  test.eq(partial.records.len(), 2)?
  test.ok(partial.issues.len() > 0)?

  let invalid_length = collector.parse_smbios_table(b"\x01\x03\0\0")?
  test.eq(invalid_length.records.len(), 0)?
  test.eq(invalid_length.truncated, false)?
  test.eq(invalid_length.issues.len(), 1)?

  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/firmware/dmi/tables", parents: true)?
  root.write(p"sys/firmware/dmi/tables/DMI", bad_index)?
  let collected = collector.collect_from_root(root, "fixture-arch", 4096, 100, "firmware", true)?
  test.eq(collected.firmware.status.state, report_model.Partial)?
  let parser_issue = (collected.issues
    |> where .field == "smbios.issue.0"
    |> first())?
  test.ok(parser_issue.detail.value != null)?
}

test test_system_report_smbios_unknown_type_keeps_record_identity [fs, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let table = b"\x90\x06E#\xaa\xbb\0\0\x7f\x04\0\0\0\0"
  let parsed = collector.parse_smbios_table(table)?
  test.eq(parsed.issues, [])?
  test.eq(parsed.records.len(), 2)?
  test.eq(parsed.records[0].record_type, 144)?
  test.eq(parsed.records[0].handle, 9029)?
  test.eq(parsed.records[0].formatted_length, 6)?
  test.eq(parsed.records[0].fields.len(), 0)?
}

test test_system_report_smbios_type16_reads_device_count_from_short_form [fs, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let table = bytes.concat(
    [
      bytes.from_ints([16, 15, 1, 0])?,
      bytes.zero(9)?,
      bytes.from_ints([2, 0, 0, 0])?,
      bytes.from_ints([127, 4, 0, 0, 0, 0])?,
    ],
  )
  let parsed = collector.parse_smbios_table(table)?
  test.eq(parsed.issues, [])?
  test.eq(parsed.records.len(), 2)?
  test.eq(parsed.records[0].formatted_length, 15)?
  let count = (parsed.records[0].fields
    |> where .name == "number_of_devices"
    |> first())?
  test.eq(count.value, 2)?
  test.eq(count.unit, "count")?
}

test test_system_report_smbios_sentinel_size_requires_complete_formatted_field [fs, error] {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let short_table = bytes.concat(
    [
      bytes.from_ints([17, 30, 1, 0])?,
      bytes.zero(8)?,
      bytes.from_ints([255, 127])?,
      bytes.zero(14)?,
      bytes.from_ints([9, 8, 0, 0, 127, 4, 0, 0, 0, 0])?,
    ],
  )
  let short_record = collector.parse_smbios_table(short_table)?
  test.eq(short_record.issues, [])?
  test.eq(short_record.records.len(), 2)?
  test.eq(
    (short_record.records[0].fields
      |> where .name == "size_raw"
      |> first())?.value,
    32767,
  )?
  test.eq((short_record.records[0].fields |> where .name == "extended_size_raw").len(), 0)?

  let complete_table = bytes.concat(
    [
      bytes.from_ints([17, 32, 2, 0])?,
      bytes.zero(8)?,
      bytes.from_ints([255, 127])?,
      bytes.zero(14)?,
      bytes.from_ints([0, 0, 1, 0, 0, 0, 127, 4, 0, 0, 0, 0])?,
    ],
  )
  let complete_record = collector.parse_smbios_table(complete_table)?
  test.eq(complete_record.issues, [])?
  test.eq(
    (complete_record.records[0].fields
      |> where .name == "extended_size_raw"
      |> first())?.value,
    65536,
  )?
}
