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
  export pure parse_cpu_list(text: Str) -> Result[List[Int], Error]
  export pure select_report_section(report: Record, selected: Str) -> Result[Record, Error]
  export pure encode_report_json(report: Record, sensitive: Bool, pretty: Bool) -> Result[Str, Error]
  export pure decode_report_json(text: Str) -> Result[report_model.SystemReport, Error]
  export pure render_text(report: Record, full: Bool, sensitive: Bool) -> Result[Str, Error]
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
  export pure parse_pci_address(value: Str) -> Result[PciAddress, Error]
  export pure parse_pci_hex_value(value: Str) -> Result[Int, Error]
  export pure pci_parent_address(target: Path, child_address: Str) -> Str?
  export pure parse_usb_descriptor_stream(data: Bytes) -> Result[List[UsbDescriptorRecord], Error]
  export proc read_source_text(root: FsRoot, path: Path, max_bytes: Int = 65536, preserve_whitespace: Bool = false) [fs, error] -> SourceRead
  export pure bounded_number(source: SourceRead, nonnegative: Bool) -> BoundedNumericObservation
  export pure parse_uptime_seconds(source: SourceRead) -> BoundedNumericObservation
  export pure bounded_size_bytes(source: SourceRead) -> BoundedNumericObservation
  export pure valid_psi_average(value: Str) -> Bool
  export pure parse_thp_policy(value: Str) -> Result[TransparentHugePagePolicy, Error]
  export pure decode_os_release_value(raw: Str) -> Str?
  export pure valid_os_release_key(key: Str) -> Bool
  export pure valid_os_release_id(value: Str) -> Bool
  export pure parse_block_scheduler(value: Str) -> BlockScheduler?
  export pure decode_device_tree_strings(raw: Str) -> List[Str]?
  export proc collect_pci(root: FsRoot) [fs, error] -> PciCollection
}

type BlockScheduler = {active: Str, available: List[Str]}

type SystemReportLiveCollector = module {
  export proc parse_usb_alternates(data: Bytes) [error] -> Result[List[UsbDescriptorAlternateFixture], Error]
  export proc collect_from_root(root: FsRoot, architecture: Str, page_size_bytes: Int, clock_ticks_per_second: Int, selected: Str = "", sensitive: Bool = false, include_local_mount_usage: Bool = false) [fs, time, error] -> Result[report_model.SystemReport, Error]
  export proc collect_live(selected: Str = "", sensitive: Bool = false) [fs, process, env, time, error] -> Result[report_model.SystemReport, Error]
  export pure assemble_network_dump(value: LinuxNetworkDump) -> NetworkCollection
  export proc link_network_device_sources(root: FsRoot, assembled: NetworkCollection, pci_functions: List[report_model.PciFunction], usb_devices: List[report_model.UsbDevice]) [fs, error] -> NetworkCollection
  export proc optional_driver_name(root: FsRoot, source_path: Path) [fs, error] -> SourceRead
  export proc usb_controller_address(root: FsRoot, device_path: Path) [fs, error] -> UsbControllerObservation
  export proc class_parent_target(root: FsRoot, entry: Path) [fs, error] -> ClassParentObservation
  export pure parse_smbios_table(data: Bytes) -> Result[SmbiosParseResult, Error]
  export pure parse_proc_stat(text: Str) -> Result[ProcStat, Error]
  export pure usb_parent_address(target: Path) -> Str?
  export pure pci_address_in_target(target: Path) -> Str?
  export pure link_usb_parents(devices: List[report_model.UsbDevice]) -> List[report_model.UsbDevice]
}

test test_system_report_checker_keeps_cpu_policy_members_typed { |ctx|
  let _ = test.expect(
    ctx,
    r"""use core.lib.system_report as model
pure cpu_policy_members(policy: model.CpuFreqPolicy) -> Str {
  return policy.related_cpus
}
""",
    status: 2,
    stderr: ["expected Str, found List[Int]"],
    args: [],
    env: {XSH_MODULE_PATH: ctx.core_dir.parent()},
  )?
}

test test_system_report_checker_rejects_live_collection_in_pure_code { |ctx|
  let _ = test.expect(
    ctx,
    r"""use core.lib.system_report_live as collector
pure forbidden_live_collection() -> Result[Unit] {
  let _ = collector.collect_live()?
  return Ok()
}
""",
    status: 2,
    stderr: ["effectful proc is not allowed in pure functions"],
    args: [],
    env: {XSH_MODULE_PATH: ctx.core_dir.parent()},
  )?
}

test test_system_report_class_parent_retains_independent_fallback_and_link_failure {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/drm/card0", parents: true)

  let directory = collector.class_parent_target(root, p"sys/class/drm/card0")
  assert directory.state == report_model.Observed
  assert directory.target == null

  root.symlink(../../devices/pci0000:00/0000:03:00.0/drm/card1, p"sys/class/drm/card1")
  let fallback = collector.class_parent_target(root, p"sys/class/drm/card1")
  assert fallback.state == report_model.Observed
  assert collector.usb_parent_address(fallback.target.require()?) == "0000:03:00.0"

  root.write(p"sys/class/drm/card0/device", "not a symlink")
  let failed = collector.class_parent_target(root, p"sys/class/drm/card0")
  assert failed.state == report_model.ReadFailure
  assert failed.errno != null

  let disappeared = collector.class_parent_target(root, p"sys/class/drm/card2")
  assert disappeared.state == report_model.Disappeared
}

test test_system_report_device_classes_reject_truncated_names_and_attributes {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/input/input0", parents: true)
  root.mkdir(p"sys/class/sound/card0", parents: true)
  root.mkdir(p"sys/class/drm/card0", parents: true)
  var padding = " "
  while padding.count_chars() < 16384 {
    padding = f"{padding}{padding}"
  }

  root.write(
    p"sys/class/input/input0/name",
    f"""private keyboard
{padding}""",
  )
  root.write(
    p"sys/class/sound/card0/id",
    f"""private card
{padding}""",
  )
  root.write(
    p"sys/class/drm/card0/status",
    f"""connected
{padding}""",
  )
  root.write(
    p"sys/class/drm/card0/enabled",
    """enabled
""",
  )

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "devices", true)?
  assert value.devices.devices.len() == 3
  let input = value.devices.devices
    |> where .class == "input"
    |> first()?
  let sound = value.devices.devices
    |> where .class == "sound"
    |> first()?
  let drm = value.devices.devices
    |> where .class == "drm"
    |> first()?
  assert input.name.value == "input0"
  assert sound.name.value == "card0"
  assert drm.name.value == "card0"
  assert drm.attributes |> any .name == "enabled" and .value.value == "enabled"
  assert ! (drm.attributes |> any .name == "status")
  for field in ["input.input0.name", "sound.card0.id", "drm.card0.status"] {
    assert value.issues |> any .section == "devices" and .field == field and .state == report_model.Truncated
  }

  assert value.devices.status.state == report_model.Partial
}

test test_system_report_device_classes_keep_sound_and_input_without_drm {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/sound/card0", parents: true)
  root.mkdir(p"sys/class/input/input0", parents: true)
  root.write(
    p"sys/class/sound/card0/id",
    """fixture sound
""",
  )
  root.write(
    p"sys/class/sound/card0/number",
    """0
""",
  )
  root.write(
    p"sys/class/input/input0/name",
    """fixture keyboard
""",
  )
  root.symlink(../../../devices/virtual/sound/card0, p"sys/class/sound/card0/device")
  root.symlink(../../../devices/virtual/input/input0, p"sys/class/input/input0/device")

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "devices", true)?
  assert value.devices.status.state == report_model.Complete
  assert value.devices.status.enumeration_succeeded
  assert value.devices.devices.len() == 2
  let sound = value.devices.devices
    |> where .class == "sound"
    |> first()?
  let input = value.devices.devices
    |> where .class == "input"
    |> first()?
  assert sound.name.value == "fixture sound"
  assert input.name.value == "fixture keyboard"
  assert sound.parent_pci_function_index == null
  assert input.parent_usb_device_index == null
  assert ! (value.devices.devices |> any .class == "drm")
}

test test_system_report_device_class_entry_identity_survives_duplicate_labels_and_replay {
  let root = fs.tempdir()?
  defer root.close()?
  for entry in ["card0", "card1"] {
    root.mkdir(fp"sys/class/sound/{entry}", parents: true)
    root.write(
      fp"sys/class/sound/{entry}/id",
      """Shared card label
""",
    )
  }

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "devices", true)?
  let sound = value.devices.devices |> where .class == "sound"
  assert sound.len() == 2
  assert sound[0].name.value == sound[1].name.value
  assert sound |> any .entry_name.value == "card0"
  assert sound |> any .entry_name.value == "card1"
  let sensitive = model.encode_report_json(value, true, false)?
  assert json.decode(sensitive)?.devices.devices[0].entry_name.value == sound[0].entry_name.value
  assert json.decode(sensitive)?.devices.devices[1].entry_name.value == sound[1].entry_name.value
  let redacted = model.encode_report_json(value, false, false)?
  assert json.decode(redacted)?.devices.devices[0].entry_name.state == "observed"
  let legacy = json.remove(json.decode(sensitive)?, ["devices", "devices", 0, "entry_name"])?
  let replay = model.decode_report_json(json.encode(legacy)?)?
  assert replay.devices.devices[0].entry_name.state == report_model.Unsupported
  let repeated = json.set(
    json.decode(sensitive)?,
    ["devices", "devices", 1, "entry_name"],
    json.decode(sensitive)?.devices.devices[0].entry_name,
  )?
  test.error_kind(model.decode_report_json(json.encode(repeated)?), "SystemReportError.InvalidJson")
}

test test_system_report_usb_controller_link_distinguishes_directories_disappearance_and_failures {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/bus/usb/devices/1-2", parents: true)

  let directory = collector.usb_controller_address(root, p"sys/bus/usb/devices/1-2")
  assert directory.state == report_model.Observed
  assert directory.address == null

  root.symlink(../../../devices/pci0000:00/0000:04:00.4/usb4/4-2, p"sys/bus/usb/devices/4-2")
  let linked = collector.usb_controller_address(root, p"sys/bus/usb/devices/4-2")
  assert linked.state == report_model.Observed
  assert linked.address == "0000:04:00.4"

  let disappeared = collector.usb_controller_address(root, p"sys/bus/usb/devices/4-3")
  assert disappeared.state == report_model.Disappeared

  root.write(p"sys/bus/usb/devices/4-4", "not a directory")
  let failed = collector.usb_controller_address(root, p"sys/bus/usb/devices/4-4")
  assert failed.state == report_model.ReadFailure
  assert failed.errno != null
}

test test_system_report_driver_link_distinguishes_unbound_and_unreadable_devices {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/example", parents: true)

  let unbound = collector.optional_driver_name(root, p"sys/devices/example/driver")
  assert unbound.observation.state == report_model.Absent
  assert unbound.observation.value == null

  root.symlink(p"example-driver", p"sys/devices/example/driver")
  let bound = collector.optional_driver_name(root, p"sys/devices/example/driver")
  assert bound.observation.state == report_model.Observed
  assert bound.observation.value == "example-driver"
  root.remove(p"sys/devices/example/driver")

  root.write(p"sys/devices/example/driver", "not a symlink")
  let failed = collector.optional_driver_name(root, p"sys/devices/example/driver")
  assert failed.observation.state == report_model.ReadFailure
  assert failed.observation.value == null
  assert failed.errno != null
}

test test_system_report_network_device_links_keep_absence_separate_from_failures {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let snapshot = model.decode_report_json(json.encode(json_report_fixture())?)?
  let source = NetworkCollection(
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
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/net/eth0", parents: true)

  let absent = collector.link_network_device_sources(root, source, [], [])
  assert absent.status.state == report_model.Complete
  assert absent.issues == []
  assert absent.links[0].parent_pci_function_index == null

  root.symlink(../../../devices/pci0000:00/0001:02:03.0, p"sys/class/net/eth0/device")
  let linked = collector.link_network_device_sources(root, source, snapshot.pci.functions, [])
  assert linked.status.state == report_model.Complete
  assert linked.links[0].parent_pci_function_index == 1
  root.remove(p"sys/class/net/eth0/device")
  root.write(p"sys/class/net/eth0/device", "not a symlink")
  let failed = collector.link_network_device_sources(root, source, [], [])
  assert failed.status.state == report_model.Partial
  let parent_issues = failed.issues |> where .field == "links.2.parent"
  assert parent_issues.len() == 1
  assert parent_issues[0].state == report_model.ReadFailure
  assert parent_issues[0].errno != null
}

test test_system_report_usb_descriptors_keep_configuration_and_endpoint_ownership {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let descriptors = b"\t\x02\x19\0\x01\x01\0\x802\t\x04\0\0\x01\xff\0\0\0\x07\x05\x81\x02@\0\0\t\x02\x19\0\x01\x02\0\x802\t\x04\0\0\x01\x08\x06P\0\x07\x05\x82\x02\0\x02\0"
  let alternates = collector.parse_usb_alternates(descriptors)?
  assert alternates.len() == 2
  assert alternates[0].configuration_value == 1
  assert alternates[1].configuration_value == 2
  assert alternates[0].endpoints.len() == 1
  assert alternates[1].endpoints.len() == 1
  assert alternates[0].endpoints[0].address == 129
  assert alternates[1].endpoints[0].address == 130
}

test test_system_report_usb_descriptor_parser_rejects_truncated_and_orphan_records {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  test.error_kind(collector.parse_usb_alternates(b"\x02\x01"), "validation")
  test.error_kind(
    collector.parse_usb_alternates(b"\t\x02\x08\0\x01\x01\0\x802"),
    "validation",
  )
  test.error_kind(
    collector.parse_usb_alternates(b"\t\x02\xff\xff\x01\x01\0\x802"),
    "validation",
  )
  test.error_kind(collector.parse_usb_alternates(b"\x04\x04\0\0"), "validation")
  test.error_kind(collector.parse_usb_alternates(b"\x07\x05\x81\x02@\0\0"), "validation")
}

test test_system_report_usb_descriptor_parser_enforces_configuration_total_length {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  test.error_kind(
    collector.parse_usb_alternates(b"\t\x02\t\0\x01\x01\0\x802\t\x04\0\0\0\xff\0\0\0"),
    "validation",
  )
  test.error_kind(
    collector.parse_usb_alternates(b"\t\x02\x12\0\x01\x01\0\x802\t\x04\0\0\x01\xff\0\0\0\x07\x05\x81\x02@\0\0"),
    "validation",
  )
}

test test_system_report_identity_uses_vendor_os_release_only_when_local_file_is_absent {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"usr/lib", parents: true)
  root.mkdir(p"proc/self/ns", parents: true)
  root.symlink(p"uts:[1001]", p"proc/self/ns/uts")
  root.symlink(p"ipc:[1002]", p"proc/self/ns/ipc")
  root.symlink(p"user:[1003]", p"proc/self/ns/user")
  root.symlink(p"time:[1004]", p"proc/self/ns/time")
  root.write(
    p"usr/lib/os-release",
    """ID=vendor
PRETTY_NAME="Vendor \\"Linux\\""
VERSION="v\\$token"
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let fallback = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let fallback_os = fallback.identity.os_release.require(report_model.OsRelease)?
  assert fallback_os.id == "vendor"
  assert fallback_os.pretty_name == "Vendor \"Linux\""
  assert fallback_os.version == "v$token"
  assert fallback.scope.uts_namespace.value == "uts:[1001]"
  assert fallback.scope.ipc_namespace.value == "ipc:[1002]"
  assert fallback.scope.user_namespace.value == "user:[1003]"
  assert fallback.scope.time_namespace.value == "time:[1004]"
  root.mkdir(p"etc", parents: true)
  root.write(
    p"etc/os-release",
    """ID=local
""",
  )
  let local = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let local_os = local.identity.os_release.require(report_model.OsRelease)?
  assert local_os.id == "local"
  assert local_os.pretty_name == null
}

test test_system_report_identity_withholds_malformed_os_release_values_without_vendor_fallback {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"etc", parents: true)
  root.mkdir(p"usr/lib", parents: true)
  root.write(
    p"usr/lib/os-release",
    """ID=vendor
VERSION_ID=99
""",
  )
  root.write(
    p"etc/os-release",
    """# local source
ID=local
VERSION_ID="unterminated
PRETTY_NAME="Local System"
NAME=Unquoted Name
not-an-assignment
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let report = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let os = report.identity.os_release.require(report_model.OsRelease)?
  assert os.id == "local"
  assert os.version_id == null
  assert os.pretty_name == "Local System"
  assert os.name == null
  assert report.issues |> any .field == "os_release.VERSION_ID" and .state == report_model.Malformed
  assert report.issues |> any .field == "os_release.NAME" and .state == report_model.Malformed
  assert report.issues |> any .field == "os_release.line.5" and .state == report_model.Malformed

  root.write(
    p"etc/os-release",
    """ID=first
ID=second
VERSION_ID=2
""",
  )
  let repeated = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let repeated_os = repeated.identity.os_release.require(report_model.OsRelease)?
  assert repeated_os.id == "second"
  assert repeated_os.version_id == "2"
  assert ! (repeated.issues |> any .field.starts_with("os_release."))

  root.write(
    p"etc/os-release",
    """ID=first
ID=bad value
VERSION_ID=2
""",
  )
  let malformed_duplicate = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let retained_os = malformed_duplicate.identity.os_release.require(report_model.OsRelease)?
  assert retained_os.id == "first"
  assert retained_os.version_id == "2"
  assert malformed_duplicate.issues |> any .field == "os_release.ID" and .state == report_model.Malformed
}

test test_system_report_identity_marks_os_release_without_id_partial {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/sys/kernel/random", parents: true)
  root.mkdir(p"etc", parents: true)
  root.mkdir(p"usr/lib", parents: true)
  root.write(
    p"proc/sys/kernel/osrelease",
    """fixture-release
""",
  )
  root.write(
    p"proc/version",
    """Linux fixture build
""",
  )
  root.write(
    p"proc/sys/kernel/hostname",
    """fixture-host
""",
  )
  root.write(
    p"proc/sys/kernel/random/boot_id",
    """fixture-boot-id
""",
  )
  root.write(
    p"proc/uptime",
    """73.5 12.0
""",
  )
  root.write(
    p"etc/os-release",
    """NAME="Local System"
PRETTY_NAME="Local Test System"
""",
  )
  root.write(
    p"usr/lib/os-release",
    """ID=vendor
""",
  )

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let collected = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let os = collected.identity.os_release.require(report_model.OsRelease)?
  assert os.id == null
  assert os.name == "Local System"
  assert os.pretty_name == "Local Test System"
  assert collected.identity.status.state == report_model.Partial
  assert collected.issues
    |> any .section == "identity" and .field == "os_release.ID" and .state == report_model.Malformed

  root.write(
    p"etc/os-release",
    """ID=first
ID=
NAME="Local System"
""",
  )
  let malformed_duplicate = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let retained = malformed_duplicate.identity.os_release.require(report_model.OsRelease)?
  assert retained.id == "first"
  assert malformed_duplicate.issues
    |> any .section == "identity" and .field == "os_release.ID" and .state == report_model.Malformed

  root.write(
    p"etc/os-release",
    """ID=first
ID="Not A Distro"
""",
  )
  let invalid_spelling = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  assert invalid_spelling.identity.os_release.require(report_model.OsRelease)?.id == "first"
  assert invalid_spelling.issues
    |> any .section == "identity" and .field == "os_release.ID" and .state == report_model.Malformed
}

test test_system_report_os_release_value_parser_rejects_malformed_assignments {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  assert collectors.decode_os_release_value("\"Local \\\"System\\\"\"") == "Local \"System\""
  assert collectors.decode_os_release_value("\"v\\$token\"") == "v$token"
  assert collectors.decode_os_release_value("'literal $value'") == "literal $value"
  assert collectors.decode_os_release_value("plain") == "plain"
  assert collectors.decode_os_release_value("v1.2-release_3") == "v1.2-release_3"
  assert collectors.decode_os_release_value("\"https://example.test/path?query=yes\"") == "https://example.test/path?query=yes"
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
    assert collectors.decode_os_release_value(malformed) == null
  }

  assert collectors.valid_os_release_key("VERSION_ID")
  assert collectors.valid_os_release_key("VENDOR_FIELD2")
  for valid_id in ["linux", "my_os-2.3", "0"] {
    assert collectors.valid_os_release_id(valid_id)
  }

  for invalid_id in ["", "Ubuntu", "with space", "has/slash", "with:colon", "é"] {
    assert ! collectors.valid_os_release_id(invalid_id)
  }

  for malformed_key in ["", "1ID", "ID-NAME", "ID "] {
    assert ! collectors.valid_os_release_key(malformed_key)
  }
}

test test_system_report_device_tree_strings_require_complete_terminated_values {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  assert collectors.decode_device_tree_strings("ARM Test Board\0") == ["ARM Test Board"]
  assert collectors.decode_device_tree_strings("vendor,board\0vendor,soc\0") == ["vendor,board", "vendor,soc"]
  for malformed in ["", "ARM Test Board", "vendor,board\0vendor,soc", "vendor,board\0\0", "\0"] {
    assert collectors.decode_device_tree_strings(malformed) == null
  }
}

test test_system_report_arm_identity_preserves_heterogeneous_cpus_without_dmi {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  root.mkdir(p"sys/firmware/devicetree/base", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu0", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu1", parents: true)
  root.write(p"sys/firmware/devicetree/base/model", "ARM Example Board\0")
  root.write(p"sys/firmware/devicetree/base/compatible", "vendor,example\0arm,v8\0")
  root.write(
    p"sys/devices/system/cpu/possible",
    """0-1
""",
  )
  root.write(
    p"sys/devices/system/cpu/present",
    """0-1
""",
  )
  root.write(
    p"sys/devices/system/cpu/online",
    """0-1
""",
  )
  root.write(p"sys/devices/system/cpu/offline", "\n")
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
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let identity = collector.collect_from_root(root, "aarch64", 65536, 100, "identity", true)?
  let platform = identity.identity.firmware.require(report_model.FirmwareIdentity)?
  assert identity.identity.architecture == "aarch64"
  assert platform.source == "device-tree"
  assert platform.device_tree_model.value == "ARM Example Board"
  assert platform.device_tree_compatible.len() == 2
  assert platform.device_tree_compatible[0].value == "vendor,example"
  assert platform.device_tree_compatible[1].value == "arm,v8"
  assert platform.vendor == null
  let cpus = collector.collect_from_root(root, "aarch64", 65536, 100, "cpu", true)?
  assert cpus.cpu.cpus.len() == 2
  assert cpus.cpu.cpus[0].model_id == "0xd05"
  assert cpus.cpu.cpus[0].model == "Cortex-A55"
  assert cpus.cpu.cpus[1].model_id == "0xd0b"
  assert cpus.cpu.cpus[1].model == "Cortex-A76"
  let firmware = collector.collect_from_root(root, "aarch64", 65536, 100, "firmware", true)?
  assert firmware.firmware.source == "device-tree"
  assert firmware.firmware.records.is_empty()

  root.remove(p"sys/firmware/devicetree/base/model")
  let compatible_only = collector.collect_from_root(root, "aarch64", 65536, 100, "firmware", true)?
  assert compatible_only.firmware.source == "device-tree"
  assert compatible_only.firmware.records.is_empty()

  root.write(p"sys/firmware/devicetree/base/model", "ARM Example Board")
  root.write(p"sys/firmware/devicetree/base/compatible", "vendor,example\0arm,v8")
  let invalid = collector.collect_from_root(root, "aarch64", 65536, 100, "identity", true)?
  let invalid_platform = invalid.identity.firmware.require(report_model.FirmwareIdentity)?
  assert invalid_platform.source == "unavailable"
  assert invalid_platform.device_tree_model.state == report_model.Malformed
  assert invalid_platform.device_tree_compatible.is_empty()
  assert invalid.issues |> any .field == "firmware.device_tree_model" and .state == report_model.Malformed
  assert invalid.issues |> any .field == "firmware.device_tree_compatible" and .state == report_model.Malformed
  let invalid_firmware = collector.collect_from_root(root, "aarch64", 65536, 100, "firmware", true)?
  assert invalid_firmware.firmware.source == "unavailable"
  assert invalid_firmware.issues |> any .field == "device_tree_model" and .state == report_model.Malformed
  assert invalid_firmware.issues |> any .field == "device_tree_compatible" and .state == report_model.Malformed
}

test test_system_report_identity_rejects_truncated_source_prefixes {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/sys/kernel", parents: true)
  root.mkdir(p"etc", parents: true)
  root.mkdir(p"usr/lib", parents: true)
  root.mkdir(p"sys/class/dmi/id", parents: true)
  root.mkdir(p"sys/firmware/devicetree/base", parents: true)
  root.write(
    p"proc/sys/kernel/osrelease",
    """fixture-release
""",
  )
  root.write(
    p"proc/version",
    """Linux fixture build
""",
  )
  root.write(
    p"usr/lib/os-release",
    """ID=vendor
""",
  )
  var padding = " "
  while padding.count_chars() < 65536 {
    padding = f"{padding}{padding}"
  }

  root.write(
    p"etc/os-release",
    f"""ID=partial
{padding}""",
  )
  root.write(
    p"proc/uptime",
    f"""73.5 12.0
{padding}""",
  )
  root.write(
    p"sys/class/dmi/id/sys_vendor",
    f"""Acme
{padding}""",
  )
  root.write(p"sys/firmware/devicetree/base/compatible", f"acme,board\0{padding}")

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let partial = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  assert partial.identity.os_release == null
  assert partial.identity.uptime_seconds == null
  let firmware = partial.identity.firmware.require(report_model.FirmwareIdentity)?
  assert firmware.vendor == null
  assert firmware.device_tree_compatible == []
  assert firmware.source == "unavailable"
  for field in ["os_release", "uptime", "firmware.vendor", "firmware.device_tree_compatible"] {
    assert partial.issues |> any .section == "identity" and .field == field and .state == report_model.Truncated
  }

  assert partial.identity.status.state == report_model.Partial

  root.write(p"sys/firmware/devicetree/base/compatible", "acme,board\0acme,soc\0")
  let complete_dt = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let observed = complete_dt.identity.firmware.require(report_model.FirmwareIdentity)?
  assert observed.source == "device-tree"
  assert observed.device_tree_compatible.len() == 2
  assert observed.device_tree_compatible[0].value == "acme,board"
}

test test_system_report_identity_retains_dmi_placeholder_text_as_raw_values {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/dmi/id", parents: true)
  root.write(
    p"sys/class/dmi/id/sys_vendor",
    """To Be Filled By O.E.M.
""",
  )
  root.write(
    p"sys/class/dmi/id/product_name",
    """Default string
""",
  )
  root.write(
    p"sys/class/dmi/id/board_name",
    """Not Specified
""",
  )
  root.write(
    p"sys/class/dmi/id/product_serial",
    """System Serial Number
""",
  )

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let collected = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  let firmware = collected.identity.firmware.require(report_model.FirmwareIdentity)?
  assert firmware.source == "dmi"
  assert firmware.vendor == "To Be Filled By O.E.M."
  assert firmware.product == "Default string"
  assert firmware.board_product == "Not Specified"
  assert firmware.serial.state == report_model.Observed
  assert firmware.serial.value == "System Serial Number"
  assert ! (collected.issues |> any .field.starts_with("firmware."))
}

test test_system_report_scope_keeps_all_process_visible_namespace_identities {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self/ns", parents: true)
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
    root.symlink(fp"{namespace.target}", fp"proc/self/ns/{namespace.name}")
  }

  root.write(
    p"proc/self/cgroup",
    """0::/container.slice/workload
""",
  )

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let collected = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  assert "process-visible" in collected.scope.host_claim
  assert collected.scope.mount_namespace.value == "mnt:[101]"
  assert collected.scope.network_namespace.value == "net:[102]"
  assert collected.scope.pid_namespace.value == "pid:[103]"
  assert collected.scope.cgroup_namespace.value == "cgroup:[104]"
  assert collected.scope.uts_namespace.value == "uts:[105]"
  assert collected.scope.ipc_namespace.value == "ipc:[106]"
  assert collected.scope.user_namespace.value == "user:[107]"
  assert collected.scope.time_namespace.value == "time:[108]"
  assert collected.scope.visible_cgroup.value == "0::/container.slice/workload"
  assert ! (collected.issues |> any .section == "scope")
}

test test_system_report_identity_preserves_namespace_link_failures {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self/ns", parents: true)
  root.write(p"proc/self/ns/mnt", "not a symlink")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let snapshot = collector.collect_from_root(root, "fixture-arch", 4096, 100, "identity", true)?
  assert snapshot.scope.mount_namespace.state == report_model.ReadFailure
  assert snapshot.scope.mount_namespace.value == null
  let failures = snapshot.issues |> where .section == "scope" and .field == "mount_namespace"
  assert failures.len() == 1
  assert failures[0].state == report_model.ReadFailure
  assert failures[0].errno != null
  assert snapshot.scope.network_namespace.state == report_model.Absent
  let selected = report_model.select_report_section(snapshot, "identity")?.require(report_model.SystemReport)?
  assert (selected.issues |> where .section == "scope" and .field == "mount_namespace").len() == 1
}

pure cpu_policy(name: Str, related_cpus: List[Int], affected_cpus: List[Int]) -> FixtureCpuFreqPolicy {
  {
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
  {
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
  {state: state, value: value, raw_bytes_base64: null}
}

pure json_section(state: Str) -> Record {
  {state: state, enumeration_succeeded: true}
}

pure json_report_fixture() -> Record {
  {
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

test test_system_report_model_relationships {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let policies = [
    cpu_policy("policy0", [0, 2], [0]),
    cpu_policy("policy1", [1, 3], [1, 3]),
  ]

  let offline_member_policy = model.frequency_policies_for_cpu(policies, 2)
  assert offline_member_policy.len() == 1
  let selected_policy = offline_member_policy[0]
  assert selected_policy.name == "policy0"
  assert model.frequency_policies_for_cpu(policies, 4).is_empty()

  let functions = [
    pci_function("0000:00:01.0", null),
    pci_function("0001:02:03.0", 0),
    pci_function("0002:04:05.0", 9),
  ]
  let child = functions[1]
  let parent = model.pci_parent_function(functions, child)
  if parent != null {
    assert parent.address == "0000:00:01.0"
  } else {
    test.fail("indexed PCI parent did not resolve")
  }

  let root_function = functions[0]
  assert model.pci_parent_function(functions, root_function) == null

  let unresolved_child = functions[2]
  assert model.pci_parent_function(functions, unresolved_child) == null
}

test test_system_report_cpu_list_parser_handles_sparse_and_large_ids {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let sparse = model.parse_cpu_list("2-4,66,129-130")?
  assert sparse == [2, 3, 4, 66, 129, 130]

  let many = model.parse_cpu_list("0-127")?
  assert many.len() == 128
  assert 65 in many
  assert 127 in many

  test.error_kind(model.parse_cpu_list(""), "SystemReportError.InvalidCpuList")
  test.error_kind(model.parse_cpu_list("4,,8"), "SystemReportError.InvalidCpuList")
  test.error_kind(model.parse_cpu_list("5-2"), "SystemReportError.InvalidCpuList")
  test.error_kind(model.parse_cpu_list("1,1"), "SystemReportError.InvalidCpuList")
  test.error_kind(model.parse_cpu_list("0-65536"), "SystemReportError.InvalidCpuList")
}

test test_system_report_cpu_collection_preserves_128_present_ids {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu", parents: true)
  for cpu_id in range(128) {
    root.mkdir(fp"sys/devices/system/cpu/cpu{cpu_id}")
  }

  root.write(
    p"sys/devices/system/cpu/possible",
    """0-127
""",
  )
  root.write(
    p"sys/devices/system/cpu/present",
    """0-127
""",
  )
  root.write(
    p"sys/devices/system/cpu/online",
    """0-127
""",
  )
  root.write(p"sys/devices/system/cpu/offline", "\n")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert value.cpu.status.enumeration_succeeded
  assert value.cpu.present.len() == 128
  assert value.cpu.cpus.len() == 128
  assert value.cpu.cpus[0].id == 0
  assert value.cpu.cpus[127].id == 127
  assert value.cpu.online.len() == 128
}

test test_system_report_cpu_collection_keeps_absent_cpufreq_unavailable {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu/cpu0", parents: true)
  root.mkdir(p"proc/sys/kernel", parents: true)
  root.write(
    p"proc/sys/kernel/osrelease",
    """fixture-vm-release
""",
  )
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )
  root.write(p"sys/devices/system/cpu/offline", "\n")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert value.cpu.status.enumeration_succeeded
  assert value.cpu.cpus.len() == 1
  assert value.cpu.frequency_policies.is_empty()
  assert value.cpu.cpus[0].policy == null
  assert value.cpu.idle_states.is_empty()
  assert value.cpu.global_idle_driver == null
  assert value.identity.kernel_release == "fixture-vm-release"
}

test test_system_report_uptime_parser_requires_complete_two_column_decimal {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let observed = SourceRead(
    observation: {
      state: report_model.Observed,
      value: "73.50 12.34",
      raw_bytes_base64: null,
    },
    errno: null,
    error_kind: null,
  )
  assert collectors.parse_uptime_seconds(observed).value == 73
  assert collectors.parse_uptime_seconds(
    {...observed, observation.value: "9007199254740991.99 0.00"},
  ).value == 9007199254740991
  let unsafe = collectors.parse_uptime_seconds(
    {...observed, observation.value: "9007199254740992.00 0.00"},
  )
  assert unsafe.value == null
  assert unsafe.state == report_model.RangeFailure
  for malformed in ["73", "73 0.00", "73.50", "-1.00 0.00", "73. 0.00", "73.50 0.x", "73.50 0.00 extra"] {
    let parsed = collectors.parse_uptime_seconds(
      {...observed, observation.value: malformed},
    )
    assert parsed.value == null
    assert parsed.state == report_model.Malformed
  }

  let truncated = collectors.parse_uptime_seconds(
    {...observed, observation.state: report_model.Truncated},
  )
  assert truncated.value == null
  assert truncated.state == report_model.Truncated
  let absent = collectors.parse_uptime_seconds(
    {...observed, observation.state: report_model.Absent, observation.value: null},
  )
  assert absent.value == null
  assert absent.state == report_model.Absent
}

test test_system_report_bounded_number_respects_source_state_and_json_range {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let observed = SourceRead(
    observation: {
      state: report_model.Observed,
      value: "-5000",
      raw_bytes_base64: null,
    },
    errno: null,
    error_kind: null,
  )
  let signed = collectors.bounded_number(observed, false)
  assert signed.value == -5000
  assert signed.state == null
  let unsigned = collectors.bounded_number(observed, true)
  assert unsigned.value == null
  assert unsigned.state == report_model.Malformed
  assert unsigned.error_kind == "negative_integer"
  assert collectors.bounded_number({...observed, observation.value: "-0"}, true).state == report_model.Malformed
  let maximum = collectors.bounded_number(
    {...observed, observation.value: "9007199254740991"},
    true,
  )
  assert maximum.value == 9007199254740991
  let unsafe_json = collectors.bounded_number(
    {...observed, observation.value: "9007199254740992"},
    true,
  )
  assert unsafe_json.value == null
  assert unsafe_json.state == report_model.RangeFailure
  let overflow = collectors.bounded_number(
    {...observed, observation.value: "999999999999999999999999"},
    true,
  )
  assert overflow.state == report_model.RangeFailure
  let invalid = collectors.bounded_number({...observed, observation.value: "42 C"}, false)
  assert invalid.state == report_model.Malformed
  for text_value in ["0x2a", "1_000", "+42"] {
    let nondecimal = collectors.bounded_number(
      {...observed, observation.value: text_value},
      false,
    )
    assert nondecimal.value == null
    assert nondecimal.state == report_model.Malformed
  }

  assert collectors.bounded_number({...observed, observation.value: "007"}, true).value == 7
  let truncated = collectors.bounded_number(
    {...observed, observation.state: report_model.Truncated, observation.value: "68"},
    true,
  )
  assert truncated.value == null
  assert truncated.state == report_model.Truncated
  let absent = collectors.bounded_number(
    {...observed, observation.state: report_model.Absent, observation.value: null},
    true,
  )
  assert absent.value == null
  assert absent.state == null
  let denied = collectors.bounded_number(
    {
      ...observed,
      observation.state: report_model.PermissionDenied,
      observation.value: null,
      errno: 13,
      error_kind: "permission_denied",
    },
    true,
  )
  assert denied.state == report_model.PermissionDenied
  assert denied.errno == 13
}

test test_system_report_bounded_size_bytes_checks_scaled_json_range {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let observed = SourceRead(
    observation: {
      state: report_model.Observed,
      value: "8796093022207K",
      raw_bytes_base64: null,
    },
    errno: null,
    error_kind: null,
  )
  assert collectors.bounded_size_bytes(observed).value == 9007199254739968
  let kilobyte_overflow = collectors.bounded_size_bytes(
    {...observed, observation.value: "8796093022208K"},
  )
  assert kilobyte_overflow.value == null
  assert kilobyte_overflow.state == report_model.RangeFailure
  assert collectors.bounded_size_bytes({...observed, observation.value: "8589934591M"}).value == 9007199253692416
  assert collectors.bounded_size_bytes({...observed, observation.value: "8589934592M"}).state == report_model.RangeFailure
  assert collectors.bounded_size_bytes({...observed, observation.value: "8388607G"}).value == 9007198180999168
  assert collectors.bounded_size_bytes({...observed, observation.value: "8388608G"}).state == report_model.RangeFailure
  assert collectors.bounded_size_bytes({...observed, observation.value: "9007199254740991"}).value == 9007199254740991
  assert collectors.bounded_size_bytes({...observed, observation.value: "9007199254740992"}).state == report_model.RangeFailure
  assert collectors.bounded_size_bytes({...observed, observation.value: "1T"}).state == report_model.Malformed
  assert collectors.bounded_size_bytes({...observed, observation.value: "0x10K"}).state == report_model.Malformed
  assert collectors.bounded_size_bytes({...observed, observation.value: "-1K"}).state == report_model.Malformed
  let truncated = collectors.bounded_size_bytes(
    {...observed, observation.state: report_model.Truncated, observation.value: "512K"},
  )
  assert truncated.value == null
  assert truncated.state == report_model.Truncated
}

test test_system_report_psi_average_parser_rejects_invalid_percentages {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  for value in ["0.00", "1.50", "99.99", "100.00"] {
    assert collectors.valid_psi_average(value)
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
    assert ! collectors.valid_psi_average(value)
  }
}

test test_system_report_thp_policy_parser_keeps_unknown_selected_value {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let policy = collectors.parse_thp_policy("always [future_policy] never")?
  assert policy.selected == "future_policy"
  assert policy.available == ["always", "future_policy", "never"]
  for value in [
    "always future_policy never",
    "[always] [never]",
    "[always] always",
    "[[]",
    """always [future_policy] never
extra""",
  ] {
    test.error_kind(collectors.parse_thp_policy(value), "SystemReportSourceError.InvalidThpPolicy")
  }
}

test test_system_report_pci_and_usb_source_parsers {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?

  let address: PciAddress = collectors.parse_pci_address("0001:af:1f.7")?
  assert address == {domain: 1, bus: 175, device: 31, function: 7}
  assert collectors.parse_pci_hex_value("0x10DE")? == 4318
  assert collectors.parse_pci_hex_value("10de")? == 4318
  test.error_kind(collectors.parse_pci_address("0000:00:20.0"), "SystemReportSourceError.InvalidPciAddress")
  test.error_kind(collectors.parse_pci_address("0000:00:01.8"), "SystemReportSourceError.InvalidPciAddress")
  test.error_kind(collectors.parse_pci_address("00:00:01.0"), "SystemReportSourceError.InvalidPciAddress")
  test.error_kind(collectors.parse_pci_hex_value("0x10xz"), "SystemReportSourceError.InvalidPciId")
  assert collectors.pci_parent_address(../../../devices/pci0001:02/0001:02:01.0/0001:02:03.0, "0001:02:03.0") == "0001:02:01.0"
  assert collectors.pci_parent_address(../../../devices/pci0001:02/0001:02:03.0, "0001:02:03.0") == null
  assert collectors.pci_parent_address(../../../devices/pci0001:02/0001:02:01.0, "0001:02:03.0") == null

  let descriptors = collectors.parse_usb_descriptor_stream(b"\x03\x99B\x02\xfe")?
  assert descriptors.len() == 2
  assert descriptors[0].offset == 0
  assert descriptors[0].descriptor_type == 153
  assert descriptors[0].raw == b"\x03\x99B"
  assert descriptors[1].offset == 3
  assert descriptors[1].descriptor_type == 254
  test.error_kind(collectors.parse_usb_descriptor_stream(b"\x01\x02"), "SystemReportSourceError.InvalidUsbDescriptor")
  test.error_kind(collectors.parse_usb_descriptor_stream(b"\x04\x01x"), "SystemReportSourceError.InvalidUsbDescriptor")
  test.error_kind(collectors.parse_usb_descriptor_stream(b"\t"), "SystemReportSourceError.InvalidUsbDescriptor")
}

test test_system_report_pci_collection_links_a_child_to_its_bridge {
  let root = fs.tempdir()?
  defer root.close()?
  let parent_path = p"sys/devices/pci0001:02/0001:02:01.0"
  let child_path = p"sys/devices/pci0001:02/0001:02:01.0/0001:02:03.0"
  root.mkdir(child_path, parents: true)
  root.mkdir(p"sys/bus/pci/devices", parents: true)
  root.symlink(../../../devices/pci0001:02/0001:02:01.0, p"sys/bus/pci/devices/0001:02:01.0")
  root.symlink(../../../devices/pci0001:02/0001:02:01.0/0001:02:03.0, p"sys/bus/pci/devices/0001:02:03.0")
  for device_path in [parent_path, child_path] {
    root.write(
      fp"{device_path}/vendor",
      """0x1234
""",
    )
    root.write(
      fp"{device_path}/device",
      """0xabcd
""",
    )
    root.write(
      fp"{device_path}/subsystem_vendor",
      """0x1234
""",
    )
    root.write(
      fp"{device_path}/subsystem_device",
      """0x0001
""",
    )
    root.write(
      fp"{device_path}/class",
      """0x060400
""",
    )
    root.write(
      fp"{device_path}/revision",
      """0x01
""",
    )
  }

  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let collection = collectors.collect_pci(root)
  assert collection.status.enumeration_succeeded
  assert collection.functions.len() == 2
  assert collection.functions[0].vendor_id == collection.functions[1].vendor_id
  assert collection.functions[0].device_id == collection.functions[1].device_id
  assert collection.functions[0].address != collection.functions[1].address
  assert collection.functions[0].driver == null
  assert collection.functions[1].driver == null
  assert collection.functions[0].parent_function_index == null
  assert collection.functions[1].parent_function_index == 0
}

test test_system_report_pci_collection_reports_non_utf8_names_without_losing_valid_functions {
  if system.uname()?.sysname == "Darwin" {
    test.skip("macOS filesystems reject non-UTF-8 filenames")
    return
  }

  let root = fs.tempdir()?
  defer root.close()?
  let valid = p"sys/bus/pci/devices/0000:00:01.0"
  let invalid = b"sys/bus/pci/devices/raw\xffname" as Path
  root.mkdir(valid, parents: true)
  root.mkdir(invalid, parents: true)
  for source in ["vendor", "device", "subsystem_vendor", "subsystem_device", "class", "revision"] {
    root.write(
      fp"{valid}/{source}",
      """0x0001
""",
    )
  }

  assert root.children(p"sys/bus/pci/devices")?.children.len() == 2

  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let collected = collectors.collect_pci(root)
  assert collected.functions.len() == 1
  assert collected.functions[0].address == "0000:00:01.0"
  let invalid_issues = collected.issues |> where .error_kind == "invalid_pci_address"
  assert invalid_issues.len() == 1
  assert collected.status.state == report_model.Partial
}

test test_system_report_pci_multifunction_keeps_optional_link_sources_distinct {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/bus/pci/devices", parents: true)
  let first_path = p"sys/devices/pci0000:01/0000:01:02.0"
  let second_path = p"sys/devices/pci0000:01/0000:01:02.1"
  for source_path in [first_path, second_path] {
    root.mkdir(source_path, parents: true)
    root.write(
      fp"{source_path}/vendor",
      """0x1234
""",
    )
    root.write(
      fp"{source_path}/device",
      """0xabcd
""",
    )
    root.write(
      fp"{source_path}/subsystem_vendor",
      """0x1234
""",
    )
    root.write(
      fp"{source_path}/subsystem_device",
      """0x0001
""",
    )
    root.write(
      fp"{source_path}/class",
      """0x020000
""",
    )
    root.write(
      fp"{source_path}/revision",
      """0x01
""",
    )
  }

  root.symlink(../../../devices/pci0000:01/0000:01:02.0, p"sys/bus/pci/devices/0000:01:02.0")
  root.symlink(../../../devices/pci0000:01/0000:01:02.1, p"sys/bus/pci/devices/0000:01:02.1")
  root.write(
    fp"{first_path}/current_link_speed",
    """8.0 GT/s PCIe
""",
  )
  root.write(
    fp"{first_path}/current_link_width",
    """8
""",
  )
  root.write(
    fp"{first_path}/max_link_speed",
    """16.0 GT/s PCIe
""",
  )
  root.write(
    fp"{first_path}/max_link_width",
    """16
""",
  )
  root.write(
    fp"{first_path}/numa_node",
    """-1
""",
  )

  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let collection = collectors.collect_pci(root)
  assert collection.functions.len() == 2
  assert collection.functions[0].address == "0000:01:02.0"
  assert collection.functions[0].function == 0
  assert collection.functions[1].address == "0000:01:02.1"
  assert collection.functions[1].function == 1
  assert collection.functions[0].current_link_speed == "8.0 GT/s PCIe"
  assert collection.functions[0].current_link_width == 8
  assert collection.functions[0].maximum_link_speed == "16.0 GT/s PCIe"
  assert collection.functions[0].maximum_link_width == 16
  assert collection.functions[0].numa_node == null
  assert ! (collection.issues |> any .field == "functions.0000:01:02.0.numa_node")
  assert collection.functions[1].current_link_speed == null
  assert collection.functions[1].current_link_width == null
  assert collection.functions[1].maximum_link_speed == null
  assert collection.functions[1].maximum_link_width == null
  assert ! (collection.issues |> any .field.starts_with("functions.0000:01:02.1.current_link"))
  assert ! (collection.issues |> any .field.starts_with("functions.0000:01:02.1.maximum_link"))

  root.write(
    fp"{second_path}/current_link_width",
    """invalid
""",
  )
  let malformed = collectors.collect_pci(root)
  assert malformed.status.state == report_model.Partial
  assert malformed.issues
    |> any .field == "functions.0000:01:02.1.current_link_width" and .state == report_model.Malformed
  assert malformed.functions[1].current_link_width == null
  root.write(
    fp"{second_path}/current_link_width",
    """0x8
""",
  )
  let radix = collectors.collect_pci(root)
  assert radix.issues |> any .field == "functions.0000:01:02.1.current_link_width" and .state == report_model.Malformed
  assert radix.functions[1].current_link_width == null
  root.write(fp"{first_path}/driver", "not-a-symlink")
  root.write(fp"{first_path}/iommu_group", "not-a-symlink")
  let failed_links = collectors.collect_pci(root)
  assert failed_links.issues |> any .field == "functions.0000:01:02.0.driver" and .state == report_model.ReadFailure
  assert failed_links.issues
    |> any .field == "functions.0000:01:02.0.iommu_group" and .state == report_model.ReadFailure
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"{padding}{padding}"
  }

  root.write(
    fp"{first_path}/current_link_speed",
    f"""8.0 GT/s PCIe
{padding}""",
  )
  root.write(
    fp"{first_path}/max_link_speed",
    f"""16.0 GT/s PCIe
{padding}""",
  )
  let truncated_links = collectors.collect_pci(root)
  assert truncated_links.functions[0].current_link_speed == null
  assert truncated_links.functions[0].maximum_link_speed == null
  assert truncated_links.issues
    |> any .field == "functions.0000:01:02.0.current_link_speed" and .state == report_model.Truncated
  assert truncated_links.issues
    |> any .field == "functions.0000:01:02.0.maximum_link_speed" and .state == report_model.Truncated
}

test test_system_report_usb_controller_path_handles_pci_and_platform_roots {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  assert collector.usb_parent_address(../../../devices/pci0000:00/0000:00:08.1/0000:04:00.4/usb4/4-2) == "0000:04:00.4"
  assert collector.usb_parent_address(../../../devices/platform/soc/usb1/1-2) == null
}

test test_system_report_usb_parent_join_handles_root_hubs_sorted_last {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let decoded = model.decode_report_json(json.encode(json_report_fixture())?)?
  let source = decoded.usb.devices[0]
  let child = {...source, sysfs_name: "4-2", bus_number: 4, parent_device_index: null, is_root_hub: false}
  let grandchild = {...source, sysfs_name: "4-2.3", bus_number: 4, parent_device_index: null, is_root_hub: false}
  let root_hub = {...source, sysfs_name: "usb4", bus_number: 4, parent_device_index: null, is_root_hub: true}
  let linked = collector.link_usb_parents([child, grandchild, root_hub])
  assert linked[0].vendor_id == linked[1].vendor_id
  assert linked[0].product_id == linked[1].product_id
  assert linked[0].sysfs_name != linked[1].sysfs_name
  assert linked[0].parent_device_index == 2
  assert linked[1].parent_device_index == 0
  assert linked[2].parent_device_index == null
}

test test_system_report_usb_keeps_a_device_with_missing_numeric_identity {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/bus/usb/devices/1-2", parents: true)
  root.mkdir(p"sys/bus/usb/devices/1-3", parents: true)
  root.mkdir(p"sys/bus/usb/devices/1-4", parents: true)
  root.write(
    p"sys/bus/usb/devices/1-2/idVendor",
    """1234
""",
  )
  root.write(
    p"sys/bus/usb/devices/1-2/idProduct",
    """5678
""",
  )
  root.write(
    p"sys/bus/usb/devices/1-2/busnum",
    """1
""",
  )
  root.write(
    p"sys/bus/usb/devices/1-3/idProduct",
    """5678
""",
  )
  root.write(
    p"sys/bus/usb/devices/1-3/busnum",
    """1
""",
  )
  root.write(
    p"sys/bus/usb/devices/1-4/idVendor",
    """zzzz
""",
  )
  root.write(
    p"sys/bus/usb/devices/1-4/idProduct",
    """5678
""",
  )
  root.write(
    p"sys/bus/usb/devices/1-4/busnum",
    """1
""",
  )

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "usb", true)?
  assert value.usb.devices.len() == 3
  assert value.usb.devices[0].vendor_id == 4660
  assert value.usb.devices[1].vendor_id == null
  let missing_vendor = value.issues |> where .field == "devices.1-3.vendor_id"
  assert missing_vendor.len() == 1
  assert missing_vendor[0].state == report_model.Absent
  assert value.usb.devices[2].vendor_id == null
  let malformed_vendor = value.issues |> where .field == "devices.1-4.vendor_id"
  assert malformed_vendor.len() == 1
  assert malformed_vendor[0].state == report_model.Malformed
}

test test_system_report_usb_power_read_failures_make_section_partial {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/bus/usb/devices/1-2/power/control", parents: true)
  root.mkdir(p"sys/bus/usb/devices/1-2/power/autosuspend_delay_ms", parents: true)
  root.mkdir(p"sys/bus/usb/devices/1-2/power/runtime_status", parents: true)
  root.write(
    p"sys/bus/usb/devices/1-2/idVendor",
    """1234
""",
  )
  root.write(
    p"sys/bus/usb/devices/1-2/idProduct",
    """5678
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "usb", true)?
  assert value.usb.devices.len() == 1
  assert value.usb.devices[0].power_control == null
  assert value.usb.devices[0].autosuspend_delay_ms == null
  assert value.usb.devices[0].runtime_status == null
  assert value.usb.status.state == report_model.Partial
  for field in ["devices.1-2.power_control", "devices.1-2.autosuspend_delay_ms", "devices.1-2.runtime_status"] {
    let matches = value.issues |> where .section == "usb" and .field == field
    assert matches.len() == 1
    assert matches[0].state == report_model.ReadFailure
  }
}

test test_system_report_usb_identity_read_failures_keep_field_issues {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/bus/usb/devices/1-2/bDeviceClass", parents: true)
  root.mkdir(p"sys/bus/usb/devices/1-2/manufacturer", parents: true)
  root.write(
    p"sys/bus/usb/devices/1-2/idVendor",
    """1234
""",
  )
  root.write(
    p"sys/bus/usb/devices/1-2/idProduct",
    """5678
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "usb", true)?
  assert value.usb.devices.len() == 1
  assert value.usb.devices[0].class_code == null
  assert value.usb.devices[0].manufacturer.value == null
  assert value.usb.status.state == report_model.Partial
  for field in ["devices.1-2.class_code", "devices.1-2.manufacturer"] {
    let matches = value.issues |> where .section == "usb" and .field == field
    assert matches.len() == 1
    assert matches[0].state == report_model.ReadFailure
  }

  root.remove(p"sys/bus/usb/devices/1-2/bDeviceClass", dir: true)
  root.write(
    p"sys/bus/usb/devices/1-2/bDeviceClass",
    """9
""",
  )
  let malformed = collector.collect_from_root(root, "fixture-arch", 4096, 100, "usb", true)?
  assert malformed.usb.devices[0].class_code == null
  assert malformed.issues
    |> any .section == "usb" and .field == "devices.1-2.class_code" and .state == report_model.Malformed
  root.write(
    p"sys/bus/usb/devices/1-2/busnum",
    """invalid
""",
  )
  let invalid_bus = collector.collect_from_root(root, "fixture-arch", 4096, 100, "usb", true)?
  assert invalid_bus.usb.devices[0].bus_number == null
  assert invalid_bus.issues
    |> any .section == "usb" and .field == "devices.1-2.bus_number" and .state == report_model.Malformed
  root.write(
    p"sys/bus/usb/devices/1-2/busnum",
    """9007199254740992
""",
  )
  let unsafe_bus = collector.collect_from_root(root, "fixture-arch", 4096, 100, "usb", true)?
  assert unsafe_bus.usb.devices[0].bus_number == null
  assert unsafe_bus.issues
    |> any .section == "usb" and .field == "devices.1-2.bus_number" and .state == report_model.Malformed
}

test test_system_report_usb_truncated_scalar_sources_do_not_publish_prefixes {
  let root = fs.tempdir()?
  defer root.close()?
  let device = p"sys/bus/usb/devices/1-2"
  root.mkdir(device, parents: true)
  root.mkdir(p"sys/bus/usb/devices/1-2/power", parents: true)
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"{padding}{padding}"
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
      fp"{device}/{field.name}",
      f"""{field.prefix}
{padding}""",
    )
  }

  root.write(
    p"sys/bus/usb/devices/1-2/idProduct",
    """5678
""",
  )

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "usb", true)?
  assert value.usb.devices.len() == 1
  let observed = value.usb.devices[0]
  assert observed.vendor_id == null
  assert observed.product_id == 22136
  assert observed.bus_number == null
  assert observed.speed_mbps == null
  assert observed.power_control == null
  assert observed.runtime_status == null
  for field in ["vendor_id", "bus_number", "speed_mbps", "power_control", "runtime_status"] {
    assert value.issues
      |> any .section == "usb" and .field == f"devices.1-2.{field}" and .state == report_model.Truncated
  }
}

test test_system_report_block_class_path_identifies_its_pci_controller {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  assert collector.pci_address_in_target(../../devices/pci0000:00/0000:00:01.2/0000:01:00.0/nvme/nvme0/nvme0n1) == "0000:01:00.0"
  assert collector.pci_address_in_target(../../devices/virtual/block/loop0) == null
}

test test_system_report_assembles_network_links_addresses_routes_and_rules {
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
  assert result.status.state == report_model.Partial
  assert result.status.enumeration_succeeded == true
  assert result.links[0].name.value == "eth0"
  assert result.links[0].mac.value == "02:00:00:00:00:01"
  assert "lower_up" in result.links[0].flags
  assert result.links[0].counters[0].value == 4096
  assert result.links[0].addresses[0].address.value == "192.0.2.10"
  assert result.links[0].addresses[0].family == "ipv4"
  assert result.links[0].addresses[1].family == "ipv6"
  assert result.links[0].addresses[1].address.value == "2001:db8::10"
  assert result.links[0].addresses[1].prefix_length == 64
  assert result.links[0].addresses[1].preferred_lifetime_seconds == 120
  assert result.links[0].addresses[1].valid_lifetime_seconds == 240
  assert result.links[0].addresses.len() == 2
  assert result.links[1].name.value == "eth0.42"
  assert result.links[1].kind == "vlan"
  assert result.links[1].lower_ifindex == 2
  assert result.links[1].addresses.len() == 1
  assert result.links[1].addresses[0].address.value == "2001:db8:42::5"
  assert result.links[0].attributes[0].data.value == "AgA="
  assert result.routes[0].destination.value == "0.0.0.0"
  assert result.routes[0].route_type == "route_type_222"
  assert result.routes[0].protocol == "protocol_77"
  assert result.routes[0].nexthops.is_empty()
  assert result.routes[1].family == "ipv6"
  assert result.routes[1].nexthops[0].ifindex == 9
  assert result.routes[1].nexthops[0].flags == 2
  assert result.routes[1].nexthops[0].hops == 3
  assert result.routes[1].nexthops[0].gateway.value == "2001:db8::1"
  assert result.rules[0].input_ifindex == 2
  assert result.rules[0].action == "action_50"
  assert result.rules[1].family == "ipv6"
  assert result.rules[1].source.value == "2001:db8::"
  assert result.rules[1].source_prefix_length == 64
  assert result.rules[1].input_ifindex == 3
  assert result.rules[1].output_ifindex == 2
  assert result.rules[1].table == 1000
  assert result.rules[1].fwmark == 7
  assert result.rules[1].fwmask == 255
  assert result.rules[1].action == "to_table"
  assert result.rules[2].action == "goto"
  assert result.rules[2].attributes[0].kind == 4
  assert result.rules[2].attributes[0].data.value == "ewAAAA=="
  assert result.issues[0].state == report_model.Malformed
  assert result.issues[1].field == "netlink.links.2.rx_bytes"
  assert result.issues[1].state == report_model.RangeFailure

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
  assert denied.status.state == report_model.SectionPermissionDenied
  assert denied.status.enumeration_succeeded == false
  assert denied.issues[0].errno == 13
}

test test_system_report_section_selection_marks_excluded_domains {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let source = json_report_fixture().require(report_model.SystemReportJson)?
  let report = model.decode_report_json(json.encode(source)?)?
  let cpu_only = model.select_report_section(report, "cpu")?.require(report_model.SystemReport)?
  let cpu_only_wire = json.decode(model.encode_report_json(cpu_only, true, false)?)?.require(report_model.SystemReportJson)?
  let cpu_only_text = model.render_text(cpu_only, true, false)?

  assert cpu_only.identity.hostname.value == "workstation-name"
  assert cpu_only_wire.cpu.status.state == "complete"
  assert cpu_only_wire.memory.status.state == "not_requested"
  assert ! cpu_only_wire.memory.status.enumeration_succeeded
  assert cpu_only_wire.memory.host.total_bytes == null
  assert cpu_only_wire.pci.status.state == "not_requested"
  assert cpu_only_wire.pci.functions.is_empty()
  assert cpu_only_wire.network.status.state == "not_requested"
  assert cpu_only.issues.len() == 1
  assert "PCI: not requested" in cpu_only_text
  assert "PCI functions:" not in cpu_only_text

  let usb_only = model.select_report_section(report, "usb")?
  assert usb_only.pci.status.state == report_model.Complete
  assert usb_only.usb.status.state == report_model.Complete
  assert usb_only.storage.status.state == report_model.SectionNotRequested
  assert usb_only.network.status.state == report_model.SectionNotRequested

  let network_only = model.select_report_section(report, "network")?
  assert network_only.pci.status.state == report_model.Complete
  assert network_only.usb.status.state == report_model.Complete
  assert network_only.network.status.state == report_model.Complete
  assert network_only.storage.status.state == report_model.SectionNotRequested

  let sensors_only = model.select_report_section(report, "sensors")?
  assert sensors_only.pci.status.state == report_model.Complete
  assert sensors_only.usb.status.state == report_model.Complete
  assert sensors_only.sensors.status.state == report_model.Complete
  assert sensors_only.devices.status.state == report_model.SectionNotRequested

  let processes_only = model.select_report_section(report, "processes")?
  assert processes_only.processes.status.state == report_model.Complete
  assert processes_only.processes.processes[0].cgroup.value == "/user.slice/private"
  assert processes_only.processes.processes[0].cgroup_resource_index == null
  assert processes_only.memory.status.state == report_model.SectionNotRequested

  test.error_kind(model.select_report_section(report, "hardware"), "SystemReportError.InvalidSection")
}

test test_system_report_json_round_trip_and_redaction {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let source = json_report_fixture().require(report_model.SystemReportJson)?
  let encoded_source = json.encode(source)?
  let decoded = model.decode_report_json(encoded_source)?
  let decoded_wire = json.decode(model.encode_report_json(decoded, true, false)?)?.require(report_model.SystemReportJson)?

  assert decoded.schema_version == 1
  assert decoded_wire.source_mode == "synthetic_fixture"
  assert decoded.identity.hostname.value == "workstation-name"
  assert decoded_wire.identity.kernel_build == "Linux version 6.12-test (builder@private-build-host)"
  assert decoded.pci.status.enumeration_succeeded
  assert decoded.pci.functions.len() == 2

  let safe_json = model.encode_report_json(decoded, false, false)?
  let safe = model.decode_report_json(safe_json)?
  let safe_wire = json.decode(safe_json)?.require(report_model.SystemReportJson)?
  let safe_text = model.render_text(decoded, true, false)?
  assert safe.redacted
  assert safe_wire.identity.status.state == "complete"
  assert safe_wire.identity.hostname.state == "redacted"
  assert safe_wire.identity.kernel_build == null
  assert "private-build-host" not in safe_json
  assert "private-build-host" not in safe_text
  assert safe.issues |> any .section == "identity" and .field == "kernel_build" and .state == report_model.Redacted
  assert model.encode_report_json(safe, false, false)? == safe_json
  assert safe.identity.hostname.value == null
  assert safe_wire.scope.network_namespace.state == "redacted"
  assert safe_wire.scope.uts_namespace.state == "redacted"
  assert safe_wire.scope.ipc_namespace.state == "redacted"
  assert safe_wire.scope.user_namespace.state == "redacted"
  assert safe_wire.scope.time_namespace.state == "redacted"
  assert safe_wire.scope.source_roots == ["redacted", "redacted"]
  assert safe.cpu.online == [0, 1, 2]
  assert safe_wire.pci.functions[0].address == null
  assert safe_wire.pci.functions[0].domain == null
  assert safe_wire.pci.functions[0].vendor_id == 32902
  assert safe_wire.pci.functions[1].parent_function_index == 0
  assert "0000:00:1f.6" not in safe_json
  assert "0000:00:1f.6" not in safe_text
  assert safe_wire.issues[1].field == "functions.redacted.vendor_id"
  assert "private PCI source path" not in safe_json
  if let Ok(vulnerability) = safe.cpu.vulnerabilities.get(0) {
    assert vulnerability.description.value == "mitigation active"
  } else {
    test.fail("CPU vulnerability fixture did not round-trip")
  }

  if let Ok(_) = safe.memory.swaps.get(0) {
    assert safe_wire.memory.swaps[0].name.state == "redacted"
  } else {
    test.fail("swap fixture did not round-trip")
  }

  if let Ok(_) = safe.usb.devices.get(0) {
    assert safe_wire.usb.devices[0].sysfs_name == null
    assert safe_wire.usb.devices[0].port_path == null
    assert safe_wire.usb.devices[0].bus_number == null
    assert safe_wire.usb.devices[0].controller_pci_index == 1
    assert safe_wire.usb.devices[0].vendor_id == 4660
    assert safe_wire.usb.devices[0].interfaces[0].name == null
    assert safe_wire.usb.devices[0].interfaces[0].alternate_settings[0].configuration_value == 1
    assert safe_wire.usb.devices[0].interfaces[0].alternate_settings[1].configuration_value == 2
    assert safe_wire.usb.devices[0].serial.state == "redacted"
    assert "1-2.3" not in safe_json
    assert "1-2.3" not in safe_text
  } else {
    test.fail("USB fixture did not round-trip")
  }

  if let Ok(block_device) = safe.storage.devices.get(0) {
    assert safe_wire.storage.devices[0].name == null
    assert safe_wire.storage.devices[0].major == null
    assert block_device.parent_pci_function_index == 1
    assert safe_wire.storage.devices[0].parent_pci_function_index == 1
    assert safe_wire.storage.devices[0].model.state == "redacted"
    assert "nvme0n1" not in safe_json
    assert "nvme0n1" not in safe_text
  } else {
    test.fail("block device fixture did not round-trip")
  }

  if let Ok(_) = safe.storage.mounts.get(0) {
    assert safe_wire.storage.mounts[0].major == null
    assert safe_wire.storage.mounts[0].minor == null
    assert safe_wire.storage.mounts[0].block_device_index == 0
    assert safe_wire.storage.mounts[0].target.state == "redacted"
    assert safe_wire.storage.mounts[0].mount_options == ["rw", "relatime", "redacted"]
    assert safe_wire.storage.mounts[0].optional_fields == ["shared:42", "redacted"]
    assert safe_wire.storage.mounts[0].super_options == ["rw", "redacted", "redacted", "redacted"]
    assert "private-mount-label" not in safe_json
    assert "private-mount-field" not in safe_json
    assert "/private/host/snapshot" not in safe_json
    assert "mount-secret" not in safe_json
  } else {
    test.fail("mount fixture did not round-trip")
  }

  if let Ok(link) = safe.network.links.get(0) {
    assert safe_wire.network.links[0].mac.state == "redacted"
    assert safe_wire.network.links[0].attributes[0].data.state == "redacted"
    if let Ok(_) = link.addresses.get(0) {
      assert safe_wire.network.links[0].addresses[0].address.state == "redacted"
    } else {
      test.fail("network address fixture did not round-trip")
    }
  } else {
    test.fail("network link fixture did not round-trip")
  }

  if let Ok(safe_route) = safe.network.routes.get(0) {
    assert safe_route.output_ifindex == 2
    assert safe_route.nexthops[0].ifindex == 2
    assert safe_route.gateway.state == report_model.Redacted
    assert safe_route.nexthops[0].gateway.state == report_model.Redacted
    assert safe_wire.network.routes[0].attributes[0].data.state == "redacted"
  } else {
    test.fail("network route fixture did not round-trip")
  }

  assert safe_wire.sensors.channels[0].label.state == "redacted"
  assert "private-sensor-label" not in safe_json
  if let Ok(firmware_record) = safe.firmware.records.get(0) {
    if let Ok(_) = firmware_record.strings.get(0) {
      assert safe_wire.firmware.records[0].strings[0].state == "redacted"
    } else {
      test.fail("firmware string fixture did not round-trip")
    }
  } else {
    test.fail("firmware record fixture did not round-trip")
  }

  if let Ok(_) = safe.kernel.parameters.get(0) {
    assert safe_wire.kernel.parameters[0].value.state == "redacted"
  } else {
    test.fail("kernel parameter fixture did not round-trip")
  }

  if let Ok(process_item) = safe.processes.processes.get(0) {
    assert process_item.command.value == "worker"
    assert safe_wire.processes.processes[0].cgroup.state == "redacted"
  } else {
    test.fail("process fixture did not round-trip")
  }

  if let Ok(device) = safe.devices.devices.get(0) {
    if let Ok(_) = device.attributes.get(0) {
      assert safe_wire.devices.devices[0].attributes[0].value.state == "redacted"
    } else {
      test.fail("device attribute fixture did not round-trip")
    }
  } else {
    test.fail("device fixture did not round-trip")
  }

  if let Ok(_) = safe.issues.get(0) {
    assert safe_wire.issues[0].detail.state == "redacted"
  } else {
    test.fail("issue fixture did not round-trip")
  }

  let sensitive_json = model.encode_report_json(decoded, true, false)?
  assert "/private/host/snapshot" in sensitive_json
  assert "mount-secret" not in sensitive_json
  assert "private-mount-field" not in sensitive_json
  assert json.decode(sensitive_json)?.storage.mounts[0].super_options[2] == "redacted"
  assert json.decode(sensitive_json)?.storage.mounts[0].super_options[3] == "redacted"
  assert json.decode(sensitive_json)?.storage.mounts[0].optional_fields == ["shared:42", "redacted"]
  assert "private-sensor-label" in sensitive_json
  let sensitive = model.decode_report_json(sensitive_json)?
  assert ! sensitive.redacted
  assert sensitive.identity.hostname.value == "workstation-name"
  assert sensitive.pci.functions[0].address == "0000:00:1f.6"
  assert sensitive.usb.devices[0].sysfs_name == "1-2.3"
  assert sensitive.storage.devices[0].name == "nvme0n1"
  assert sensitive.network.links[0].attributes[0].data.value == "eA=="
  assert sensitive.network.routes[0].nexthops[0].gateway.value == "192.0.2.1"

  let route_text = model.render_text(sensitive, true, true)?
  assert "nexthop ifindex=2 flags=1 hops=0 gateway=\"192.0.2.1\"" in route_text

  let unsupported_version = json.encode({...source, schema_version: 2})?
  test.error_kind(model.decode_report_json(unsupported_version), "SystemReportError.UnsupportedSchema")

  let unknown_state = json.encode({...source, source_mode: "unknown-mode"})?
  test.error_kind(model.decode_report_json(unknown_state), "SystemReportError.InvalidJson")

  let invalid_cpu_status = {...source.cpu.status, enumeration_succeeded: false}
  let invalid_cpu = {...source.cpu, status: invalid_cpu_status}
  let invalid_section = {...source, cpu: invalid_cpu}
  test.error_kind(model.decode_report_json(json.encode(invalid_section)?), "SystemReportError.InvalidJson")
}

test test_system_report_thermal_trip_indexes_round_trip_and_legacy_unknown {
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
  let trip_indexes = decoded.sensors.thermal_zones[0].trips |> map .index
  let expected_trip_indexes: List[Int?] = [0, 2]
  assert trip_indexes == expected_trip_indexes
  let encoded = model.encode_report_json(decoded, true, false)?
  assert json.get(json.decode(encoded)?, ["sensors", "thermal_zones", 0, "trips", 1, "index"])?.require(Int)? == 2
  assert "trip 2 \"passive\"" in model.render_text(decoded, true, false)?
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
  assert legacy.sensors.thermal_zones[0].trips[0].index == null
  let duplicate = {...zone, trips: [zone.trips[0], {...zone.trips[1], index: 0}]}
  test.error_kind(
    model.decode_report_json(json.encode(json.set(source, ["sensors", "thermal_zones"], [duplicate])?)?),
    "SystemReportError.InvalidJson",
  )
  let mixed = json.set(indexed, ["sensors", "thermal_zones", 0, "trips", 1, "index"], null)?
  test.error_kind(model.decode_report_json(json.encode(mixed)?), "SystemReportError.InvalidJson")
  let negative = json.set(indexed, ["sensors", "thermal_zones", 0, "trips", 0, "index"], -1)?
  test.error_kind(model.decode_report_json(json.encode(negative)?), "SystemReportError.InvalidJson")
}

test test_system_report_cpufreq_scaling_current_replays_legacy_requested_name {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let encoded_fixture = json.encode(json_report_fixture())?
  let source = json.decode(encoded_fixture)?
  let decoded = model.decode_report_json(json.encode(source)?)?
  assert decoded.cpu.frequency_policies[0].scaling_current_khz == 1800000
  let saved = json.decode(model.encode_report_json(decoded, true, false)?)?
  assert json.get(saved, ["cpu", "frequency_policies", 0, "scaling_current_khz"])?.require(Int)? == 1800000
  assert json.get(saved, ["cpu", "frequency_policies", 0, "requested_current_khz"], null) == null
  var legacy = json.remove(source, ["cpu", "frequency_policies", 0, "scaling_current_khz"])?
  legacy = json.set(legacy, ["cpu", "frequency_policies", 0, "requested_current_khz"], 1800000)?
  assert model.decode_report_json(json.encode(legacy)?)?.cpu.frequency_policies[0].scaling_current_khz == 1800000
  let redundant = json.set(source, ["cpu", "frequency_policies", 0, "requested_current_khz"], 1800000)?
  assert model.decode_report_json(json.encode(redundant)?)?.cpu.frequency_policies[0].scaling_current_khz == 1800000
  let legacy_absent = json.set(
    json.remove(source, ["cpu", "frequency_policies", 0, "scaling_current_khz"])?,
    ["cpu", "frequency_policies", 0, "requested_current_khz"],
    null,
  )?
  assert model.decode_report_json(json.encode(legacy_absent)?)?.cpu.frequency_policies[0].scaling_current_khz == null
  let conflicting = json.set(source, ["cpu", "frequency_policies", 0, "requested_current_khz"], 1700000)?
  test.error_kind(model.decode_report_json(json.encode(conflicting)?), "SystemReportError.InvalidJson")
}

test test_system_report_usb_runtime_status_replays_legacy_absence {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let encoded_fixture = json.encode(json_report_fixture())?
  let source = json.decode(encoded_fixture)?
  let decoded = model.decode_report_json(json.encode(source)?)?
  assert decoded.usb.devices[0].runtime_status == "active"
  let old = json.remove(source, ["usb", "devices", 0, "runtime_status"])?
  assert model.decode_report_json(json.encode(old)?)?.usb.devices[0].runtime_status == null
  let malformed = json.set(source, ["usb", "devices", 0, "runtime_status"], 7)?
  test.error_kind(model.decode_report_json(json.encode(malformed)?), "SystemReportError.InvalidJson")
}

test test_system_report_v1_replay_marks_unrecorded_namespaces_unsupported {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let encoded_fixture = json.encode(json_report_fixture())?
  var old = json.decode(encoded_fixture)?
  for field in ["uts_namespace", "ipc_namespace", "user_namespace", "time_namespace"] {
    old = json.remove(old, ["scope", field])?
  }

  let restored = model.decode_report_json(json.encode(old)?)?
  assert restored.scope.uts_namespace.state == report_model.Unsupported
  assert restored.scope.ipc_namespace.state == report_model.Unsupported
  assert restored.scope.user_namespace.state == report_model.Unsupported
  assert restored.scope.time_namespace.state == report_model.Unsupported

  let malformed = json.set(old, ["scope", "uts_namespace"], null)?
  test.error_kind(model.decode_report_json(json.encode(malformed)?), "SystemReportError.InvalidJson")
}

test test_system_report_v1_replay_keeps_unrecorded_idle_state_index_unknown {
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
  assert restored.cpu.idle_states.len() == 1
  assert restored.cpu.idle_states[0].state_index == null
  let current = json.decode(model.encode_report_json(restored, true, false)?)?
  assert json.get(current, ["cpu", "idle_states", 0, "state_index"])?.require(Int?)? == null
  let invalid = json.set(old, ["cpu", "idle_states", 0, "state_index"], -1)?
  test.error_kind(model.decode_report_json(json.encode(invalid)?), "SystemReportError.InvalidJson")
  let indexed_state = {...legacy_state, state_index: 0}
  let duplicate = json.set(old, ["cpu", "idle_states"], [indexed_state, indexed_state])?
  test.error_kind(model.decode_report_json(json.encode(duplicate)?), "SystemReportError.InvalidJson")
}

test test_system_report_v1_replay_restores_legacy_powercap_constraint {
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
  assert restored.power.cap_zones.len() == 1
  assert restored.power.cap_zones[0].entry_name == "package-0"
  assert restored.power.cap_zones[0].constraints.len() == 1
  assert restored.power.cap_zones[0].constraints[0].index == 0
  assert restored.power.cap_zones[0].constraints[0].power_limit_uw == 45000000
  let wire = json.decode(model.encode_report_json(restored, true, false)?)?
  assert json.get(wire, ["power", "cap_zones", 0, "constraints", 0, "power_limit_uw"])?.require(Int)? == 45000000
  test.error_kind(json.get(wire, ["power", "cap_zones", 0, "power_limit_uw"]), "json-path")

  let invalid = json.set(old, ["power", "cap_zones", 0, "power_limit_uw"], "invalid")?
  test.error_kind(model.decode_report_json(json.encode(invalid)?), "SystemReportError.InvalidJson")
}

test test_system_report_powercap_constraints_round_trip_and_render {
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
  assert decoded.power.cap_zones[0].entry_name == "intel-rapl:0"
  assert decoded.power.cap_zones[0].constraints.len() == 2
  let text = model.render_text(decoded, true, true)?
  assert "constraint 0 \"long_term\" limit=45000000 uW window=1000000 us" in text
  assert "constraint 1 \"short_term\" limit=65000000 uW window=250000 us" in text
  let encoded = model.encode_report_json(decoded, true, false)?
  let restored = model.decode_report_json(encoded)?
  assert restored.power.cap_zones[0].constraints == decoded.power.cap_zones[0].constraints
  let duplicate = json.set(source, ["power", "cap_zones", 0, "constraints", 1, "index"], 0)?
  test.error_kind(model.decode_report_json(json.encode(duplicate)?), "SystemReportError.InvalidJson")
}

test test_system_report_replay_withholds_mount_credentials_in_sensitive_json {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let raw = json.set(
    json_report_fixture(),
    ["storage", "mounts", 0, "source"],
    json_observation("observed", "smb://user:private-secret@host/share"),
  )?
  let decoded = model.decode_report_json(json.encode(raw)?)?
  assert decoded.storage.mounts[0].source.state == report_model.Redacted
  let sensitive_json = model.encode_report_json(decoded, true, false)?
  assert "private-secret" not in sensitive_json
  assert json.decode(sensitive_json)?.storage.mounts[0].source.state == "redacted"
}

test test_system_report_text_output_escapes_untrusted_controls {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let source = json_report_fixture().require(report_model.SystemReportJson)?
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

  assert "\u{202e}" not in rendered
  assert "\u{1b}" not in rendered
  assert "\\u{202e}" in rendered
  assert "6.12\\nattack" in rendered
  assert """6.12
attack""" not in rendered
  assert "does not guarantee anonymity" in rendered
  assert "Uptime: unknown seconds" in rendered
}

test test_system_report_full_text_renders_numeric_relationship_lists {
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let source = json_report_fixture().require(report_model.SystemReportJson)?
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
  let disk = source.storage.devices[0]
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
  assert "shared CPUs 0,2" in rendered
  assert "holders=1 slaves=" in rendered
  assert "holders= slaves=0" in rendered
}

test test_system_report_command_replays_saved_json_offline { |ctx|
  let report_path = test.temp_path(ctx, name: "system-report-v1.json")
  report_path.write(json.encode(json_report_fixture())?)

  let projected = run.text ${ctx.xsh_bin} fp"{ctx.core_dir.parent()}/core/system-report.xsh" -- --from $report_path \
    --section cpu --json ?
  let decoded = json.decode(projected)?
  assert decoded.schema_version == 1
  assert decoded.identity.hostname.state == "redacted"
  assert decoded.cpu.status.state == "complete"
  assert decoded.memory.status.state == "not_requested"
  assert "workstation-name" not in projected
  let json_full = run.text ${ctx.xsh_bin} fp"{ctx.core_dir.parent()}/core/system-report.xsh" -- --from $report_path \
    --section cpu --json --full ?
  assert json_full == projected

  let sensitive = run.text ${ctx.xsh_bin} fp"{ctx.core_dir.parent()}/core/system-report.xsh" -- --from $report_path \
    --sensitive --json ?
  assert "workstation-name" in sensitive
  let default_json = run.text ${ctx.xsh_bin} fp"{ctx.core_dir.parent()}/core/system-report.xsh" -- --from $report_path \
    --json ?
  assert "/private/host/snapshot" not in default_json
  assert "mount-secret" not in default_json
  assert "private-sensor-label" not in default_json

  let overview = run.text ${ctx.xsh_bin} fp"{ctx.core_dir.parent()}/core/system-report.xsh" -- --from $report_path ?
  assert "XSH system report v1" in overview
  assert "2 identical policy group on CPUs 0,1" in overview
  assert "1 identical policy group on CPUs 2" in overview
  assert "3 identical policy group" not in overview

  let version = run.text ${ctx.xsh_bin} fp"{ctx.core_dir.parent()}/core/system-report.xsh" -- --version ?
  assert version == """system-report schema v1
"""

  let help = run.text ${ctx.xsh_bin} fp"{ctx.core_dir.parent()}/core/system-report.xsh" -- --help ?
  assert "--from FILE" in help
  assert "--section NAME" in help
}

test test_system_report_command_usage_retains_invalid_section_cause { |ctx|
  let outcome = run.capture --text --accept=[3] ${ctx.xsh_bin} fp"{ctx.core_dir.parent()}/core/system-report.xsh" -- \
    --section hardware ?
  assert outcome.status.exited_with(3)
  assert outcome.stdout == ""
  assert "err: SystemReportCliError.Usage:" in outcome.stderr
  assert "caused by: SystemReportError.InvalidSection:" in outcome.stderr
  assert outcome.stderr.split("unknown report section 'hardware'").len() == 3
}

test test_system_report_command_rejects_malformed_replay { |ctx|
  let report_path = test.temp_file(ctx, name: "system-report-invalid.json", contents: b"{invalid")?
  let stderr = test.temp_path(ctx, name: "system-report-invalid.stderr")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir.parent()}/core/system-report.xsh" -- --from $report_path \
    2> $stderr
  assert ! status.exited_with(0)
  assert "invalid replay report" in stderr.read_text()?

  let invalid_utf8 = test.temp_file(ctx, name: "system-report-invalid-utf8.json", contents: b"\xff")?
  let utf8_stderr = test.temp_path(ctx, name: "system-report-invalid-utf8.stderr")
  let utf8_status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir.parent()}/core/system-report.xsh" -- --from \
    $invalid_utf8 2> $utf8_stderr
  assert ! utf8_status.exited_with(0)
  assert "not valid UTF-8" in utf8_stderr.read_text()?

  let unsupported_path = test.temp_path(ctx, name: "system-report-unsupported-schema.json")
  unsupported_path.write(
    json.encode({...json_report_fixture().require(report_model.SystemReportJson)?, schema_version: 99})?,
  )
  let unsupported_stderr = test.temp_path(ctx, name: "system-report-unsupported-schema.stderr")
  let unsupported_status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir.parent()}/core/system-report.xsh" -- --from \
    $unsupported_path 2> $unsupported_stderr
  assert ! unsupported_status.exited_with(0)
  assert "unsupported schema version" in unsupported_stderr.read_text()?

  let section_stderr = test.temp_path(ctx, name: "system-report-invalid-section.stderr")
  let section_status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir.parent()}/core/system-report.xsh" -- --section \
    hardware 2> $section_stderr
  assert ! section_status.exited_with(0)
  assert section_stderr.read_text()?.trim() != ""
}

test test_system_report_live_collection_uses_explicit_root_and_redacts_by_default {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/sys/kernel", parents: true)
  root.mkdir(p"proc/sys/kernel/random", parents: true)
  root.mkdir(p"etc", parents: true)
  root.write(
    p"proc/sys/kernel/osrelease",
    """fixture-release
""",
  )
  root.write(
    p"proc/version",
    """Linux fixture version 1
""",
  )
  root.write(
    p"proc/sys/kernel/hostname",
    """private-fixture-host
""",
  )
  root.write(
    p"proc/sys/kernel/random/boot_id",
    """private-fixture-boot-id
""",
  )
  root.write(
    p"proc/uptime",
    """73.5 12.0
""",
  )
  root.write(
    p"etc/os-release",
    """ID=fixture
NAME=Fixture OS
PRETTY_NAME="Fixture Operating System"
VERSION_ID=1
""",
  )

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let share_safe = collector.collect_from_root(root, "fixture-arch", 65536, 250, "identity")?
  assert share_safe.source_mode == report_model.SyntheticFixture
  assert share_safe.identity.kernel_release == "fixture-release"
  assert share_safe.identity.architecture == "fixture-arch"
  assert share_safe.identity.uptime_seconds == 73
  assert share_safe.identity.hostname.state == report_model.Redacted
  assert share_safe.identity.hostname.value == null
  assert share_safe.cpu.status.state == report_model.SectionNotRequested
  assert share_safe.redacted == true

  let sensitive = collector.collect_from_root(root, "fixture-arch", 65536, 250, "identity", true)?
  assert sensitive.identity.hostname.value == "private-fixture-host"
  assert sensitive.identity.hostname.state == report_model.Observed
}

test test_system_report_cpu_collection_does_not_invent_absent_cpu_zero {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu/cpu2", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu4", parents: true)
  root.write(p"sys/devices/system/cpu/possible", "0-4")
  root.write(p"sys/devices/system/cpu/present", "2,4")
  root.write(p"sys/devices/system/cpu/online", "2")
  root.write(p"sys/devices/system/cpu/offline", "4")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let snapshot = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert snapshot.cpu.status.enumeration_succeeded
  assert snapshot.cpu.possible == [0, 1, 2, 3, 4]
  assert snapshot.cpu.present == [2, 4]
  assert snapshot.cpu.cpus.len() == 2
  assert snapshot.cpu.cpus[0].id == 2
  assert snapshot.cpu.cpus[0].online == true
  assert snapshot.cpu.cpus[1].id == 4
  assert snapshot.cpu.cpus[1].online == false
}

test test_system_report_cpu_enumeration_requires_a_valid_present_list {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu", parents: true)
  root.write(p"sys/devices/system/cpu/possible", "0")
  root.write(p"sys/devices/system/cpu/present", "0-x")
  root.write(p"sys/devices/system/cpu/online", "0")
  root.write(p"sys/devices/system/cpu/offline", "")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?

  let malformed = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert malformed.cpu.possible == [0]
  assert malformed.cpu.present == []
  assert ! malformed.cpu.status.enumeration_succeeded
  assert (malformed.issues |> where .section == "cpu" and .field == "present").len() == 1

  root.write(p"sys/devices/system/cpu/present", "0")
  root.remove(p"sys/devices/system/cpu/possible")
  let present = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert present.cpu.possible == []
  assert present.cpu.present == [0]
  assert present.cpu.status.enumeration_succeeded
  assert present.cpu.status.state == report_model.Partial

  var padding = " "
  while padding.count_chars() < 65536 {
    padding = f"{padding}{padding}"
  }

  root.write(p"sys/devices/system/cpu/present", f"0{padding}")
  let truncated = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert truncated.cpu.present == []
  assert ! truncated.cpu.status.enumeration_succeeded
  let present_issues = truncated.issues |> where .section == "cpu" and .field == "present"
  assert present_issues.len() == 1
  assert present_issues[0].state == report_model.Truncated
}

test test_system_report_cpu_present_symlinks_cannot_cycle_or_escape_the_source_root {
  let root = fs.tempdir()?
  defer root.close()?
  let outside = fs.tempdir()?
  defer outside.close()?
  root.mkdir(p"sys/devices/system/cpu", parents: true)
  root.write(p"sys/devices/system/cpu/possible", "0")
  outside.write(p"present", "0")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?

  root.symlink(p"present", p"sys/devices/system/cpu/present")
  let cycled = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert cycled.cpu.present == []
  assert ! cycled.cpu.status.enumeration_succeeded
  assert (cycled.issues |> where .section == "cpu" and .field == "present").len() == 1

  root.remove(p"sys/devices/system/cpu/present")
  let outside_path = outside.host_path()?
  root.symlink(fp"{outside_path}/present", p"sys/devices/system/cpu/present")
  let escaped = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert escaped.cpu.present == []
  assert ! escaped.cpu.status.enumeration_succeeded
  assert (escaped.issues |> where .section == "cpu" and .field == "present").len() == 1
}

test test_system_report_cpu_directory_failures_keep_source_issues {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu/cpu0", parents: true)
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )
  root.write(p"sys/devices/system/cpu/offline", "")
  root.write(p"sys/devices/system/cpu/cpu0/cache", "not a directory")
  root.write(p"sys/devices/system/cpu/cpu0/cpuidle", "not a directory")
  root.write(p"sys/devices/system/cpu/vulnerabilities", "not a directory")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  for field in ["cpu0.cache", "cpu0.cpuidle", "vulnerabilities"] {
    let matches = value.issues |> where .section == "cpu" and .field == field
    assert matches.len() == 1
    assert matches[0].state == report_model.ReadFailure
  }

  assert value.cpu.status.state == report_model.Partial

  root.remove(p"sys/devices/system/cpu/cpu0/cache")
  root.remove(p"sys/devices/system/cpu/cpu0/cpuidle")
  root.remove(p"sys/devices/system/cpu/cpu0", dir: true)
  let disappeared = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  let enumeration_issues = disappeared.issues |> where .section == "cpu" and .field == "cpu0.enumeration"
  assert enumeration_issues.len() == 1
  assert enumeration_issues[0].state == report_model.Absent
}

test test_system_report_cpu_vulnerability_read_failures_keep_named_issues {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu/cpu0", parents: true)
  root.mkdir(p"sys/devices/system/cpu/vulnerabilities", parents: true)
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )
  root.write(p"sys/devices/system/cpu/offline", "\n")
  root.write(
    p"sys/devices/system/cpu/vulnerabilities/spectre_v2",
    """Mitigation: fixture policy
""",
  )
  root.symlink(p"missing", p"sys/devices/system/cpu/vulnerabilities/spectre_v1")
  var oversized = "x"
  while oversized.count_chars() <= 16384 {
    oversized = f"{oversized}{oversized}"
  }

  root.write(p"sys/devices/system/cpu/vulnerabilities/mmio_stale_data", oversized)

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  let valid = value.cpu.vulnerabilities |> where .name == "spectre_v2"
  assert valid.len() == 1
  assert valid[0].description.value == "Mitigation: fixture policy"
  let vanished = value.cpu.vulnerabilities |> where .name == "spectre_v1"
  assert vanished.len() == 1
  assert vanished[0].description.state == report_model.Absent
  let truncated = value.cpu.vulnerabilities |> where .name == "mmio_stale_data"
  assert truncated.len() == 1
  assert truncated[0].description.state == report_model.Truncated
  assert value.issues
    |> any .section == "cpu" and .field == "vulnerabilities.spectre_v1" and .state == report_model.Absent
  assert value.issues
    |> any .section == "cpu" and .field == "vulnerabilities.mmio_stale_data" and .state == report_model.Truncated
  assert value.cpu.status.state == report_model.Partial
}

test test_system_report_effective_cpuset_rejects_a_truncated_source {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)
  root.mkdir(p"sys/fs/cgroup", parents: true)
  root.mkdir(p"sys/devices/system/cpu", parents: true)
  root.write(
    p"proc/self/cgroup",
    """0::/
""",
  )
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )
  root.write(p"sys/devices/system/cpu/possible", "0")
  root.write(p"sys/devices/system/cpu/present", "0")
  root.write(p"sys/devices/system/cpu/online", "0")
  root.write(p"sys/devices/system/cpu/offline", "")
  var padding = " "
  while padding.count_chars() < 65536 {
    padding = f"{padding}{padding}"
  }

  root.write(p"sys/fs/cgroup/cpuset.cpus.effective", f"0{padding}")

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let snapshot = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert snapshot.cpu.effective_cpuset == []
  let cpuset_issues = snapshot.issues |> where .section == "cpu" and .field == "cgroup.effective_cpuset"
  assert cpuset_issues.len() == 1
  assert cpuset_issues[0].state == report_model.Truncated

  root.write(p"sys/fs/cgroup/cpuset.cpus.effective", "0")
  root.write(
    p"proc/self/mountinfo",
    """30 20 0:25 /outside /sys/fs/cgroup/other rw - cgroup2 cgroup rw
31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )
  let selected_mount = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert selected_mount.cpu.effective_cpuset == [0]
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup - cgroup2 cgroup rw
""",
  )
  let malformed_mount = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert malformed_mount.cpu.effective_cpuset == []
  assert malformed_mount.issues
    |> any .section == "cpu" and .field == "cgroup.effective_cpuset" and .state == report_model.Malformed
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw cgroup2 cgroup rw
""",
  )
  let missing_separator = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert missing_separator.cpu.effective_cpuset == []
  assert missing_separator.issues
    |> any .section == "cpu" and .field == "cgroup.effective_cpuset" and .state == report_model.Malformed
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw -
""",
  )
  let incomplete_row = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert incomplete_row.cpu.effective_cpuset == []
  assert incomplete_row.issues
    |> any .section == "cpu" and .field == "cgroup.effective_cpuset" and .state == report_model.Malformed
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )
  root.write(p"proc/self/cgroup", f"0::/{padding}")
  let truncated_membership = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert truncated_membership.cpu.effective_cpuset == []
  let membership_issues = truncated_membership.issues |> where .section == "cpu" and .field == "cgroup.effective_cpuset"
  assert membership_issues.len() == 1
  assert membership_issues[0].state == report_model.Truncated

  root.write(
    p"proc/self/cgroup",
    """0::/
0::/other
""",
  )
  let duplicate_membership = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert duplicate_membership.cpu.effective_cpuset == []
  assert duplicate_membership.issues
    |> any .section == "cpu" and .field == "cgroup.effective_cpuset" and .state == report_model.Malformed
}

test test_system_report_cpu_collection_keeps_sparse_models_policies_and_cpuset {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/sys/kernel/random", parents: true)
  root.mkdir(p"proc/self", parents: true)
  root.mkdir(p"etc", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu0/topology", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu0/cache/index7", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu0/node0", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu2/topology", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu2/cache/index7", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu2/node1", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpufreq/policy3", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpufreq/policy9", parents: true)
  root.mkdir(p"sys/devices/system/cpu/vulnerabilities", parents: true)
  root.mkdir(p"sys/fs/cgroup/worker", parents: true)
  root.write(
    p"proc/sys/kernel/osrelease",
    """fixture-release
""",
  )
  root.write(
    p"proc/version",
    """Linux fixture version 1
""",
  )
  root.write(
    p"proc/sys/kernel/hostname",
    """fixture-host
""",
  )
  root.write(
    p"proc/sys/kernel/random/boot_id",
    """fixture-boot-id
""",
  )
  root.write(
    p"proc/uptime",
    """1.0 0.0
""",
  )
  root.write(
    p"etc/os-release",
    """ID=fixture
""",
  )
  root.write(
    p"proc/self/cgroup",
    """0::/tenant/worker
""",
  )
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 /tenant /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )
  root.write(
    p"sys/fs/cgroup/worker/cpuset.cpus.effective",
    """0,2
""",
  )
  root.write(
    p"proc/self/status",
    """Cpus_allowed_list:	0,2
""",
  )
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
  )
  root.write(
    p"sys/devices/system/cpu/possible",
    """0-2
""",
  )
  root.write(
    p"sys/devices/system/cpu/present",
    """0,2
""",
  )
  root.write(
    p"sys/devices/system/cpu/online",
    """0,2
""",
  )
  root.write(
    p"sys/devices/system/cpu/offline",
    """1
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/topology/physical_package_id",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/topology/die_id",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/topology/core_id",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/topology/thread_siblings_list",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu2/topology/physical_package_id",
    """1
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu2/topology/die_id",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu2/topology/core_id",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu2/topology/thread_siblings_list",
    """2
""",
  )
  for cpu_path in [p"sys/devices/system/cpu/cpu0/cache/index7", p"sys/devices/system/cpu/cpu2/cache/index7"] {
    root.write(
      fp"{cpu_path}/level",
      """2
""",
    )
    root.write(
      fp"{cpu_path}/type",
      """Unified
""",
    )
    root.write(
      fp"{cpu_path}/size",
      """1M
""",
    )
    root.write(
      fp"{cpu_path}/coherency_line_size",
      """64
""",
    )
    root.write(
      fp"{cpu_path}/number_of_sets",
      """16384
""",
    )
    root.write(
      fp"{cpu_path}/shared_cpu_list",
      """0,2
""",
    )
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
      fp"{policy.path}/related_cpus",
      f"""{policy.related}
""",
    )
    root.write(
      fp"{policy.path}/affected_cpus",
      f"""{policy.affected}
""",
    )
    root.write(
      fp"{policy.path}/scaling_driver",
      f"""{policy.driver}
""",
    )
    root.write(
      fp"{policy.path}/scaling_governor",
      f"""{policy.governor}
""",
    )
    root.write(
      fp"{policy.path}/scaling_available_governors",
      """powersave performance
""",
    )
    root.write(
      fp"{policy.path}/cpuinfo_min_freq",
      """800000
""",
    )
    root.write(
      fp"{policy.path}/cpuinfo_max_freq",
      """4000000
""",
    )
    root.write(
      fp"{policy.path}/scaling_min_freq",
      """1000000
""",
    )
    root.write(
      fp"{policy.path}/scaling_max_freq",
      """3000000
""",
    )
    root.write(
      fp"{policy.path}/energy_performance_preference",
      """balance_performance
""",
    )
    root.write(
      fp"{policy.path}/energy_performance_available_preferences",
      """performance balance_performance power
""",
    )
  }

  root.write(
    p"sys/devices/system/cpu/vulnerabilities/spectre_v2",
    """Mitigation: fixture policy
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpufreq/boost",
    """1
""",
  )
  let policies = root.children(p"sys/devices/system/cpu/cpufreq")?
  assert policies.state == "complete"
  assert (policies.children |> where .name().starts_with("policy")).len() == 2

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert value.cpu.possible == [0, 1, 2]
  assert value.cpu.present == [0, 2]
  assert value.cpu.offline == [1]
  assert value.cpu.affinity == [0, 2]
  assert value.cpu.effective_cpuset == [0, 2]
  assert value.cpu.cpus[0].model == "Intel fixture"
  assert value.cpu.cpus[1].model == "AMD fixture"
  assert value.cpu.frequency_policies.len() == 2
  assert value.cpu.cpus[0].policy == "policy3"
  assert value.cpu.cpus[1].policy == "policy9"
  assert value.cpu.frequency_policies.len() == 2
  assert value.cpu.frequency_policies[1].governor == "unlisted-governor"
  assert value.cpu.frequency_policies[1].related_cpus == [1, 2]
  assert value.cpu.frequency_policies[1].affected_cpus == [2]
  assert value.cpu.frequency_policies[0].hardware_min_khz == 800000
  assert value.cpu.frequency_policies[1].hardware_max_khz == 4000000
  assert value.cpu.frequency_policies[0].scaling_min_khz == 1000000
  assert value.cpu.frequency_policies[1].scaling_max_khz == 3000000
  assert value.cpu.frequency_policies[0].energy_performance_preference == "balance_performance"
  assert value.cpu.frequency_policies[1].available_energy_performance_preferences == [
    "performance",
    "balance_performance",
    "power",
  ]
  assert value.cpu.frequency_policies[0].boost_allowed == true
  assert value.cpu.frequency_policies[0].boost_supported == true
  assert value.cpu.frequency_policies[0].boost_active == null
  assert value.cpu.frequency_policies[1].boost_scope == "system"
  assert value.identity.kernel_release == "fixture-release"
  assert value.cpu.caches.len() == 1
  assert value.cpu.caches[0].sysfs_index == 7
  assert value.cpu.caches[0].level == 2
  assert value.cpu.caches[0].shared_cpus == [0, 2]
  assert value.cpu.cpus[0].numa_node == 0
  assert value.cpu.cpus[1].numa_node == 1
  assert value.cpu.cpus[0].cache_ids == [0]
  assert value.cpu.cpus[1].cache_ids == [0]
  assert value.cpu.vulnerabilities[0].description.value == "Mitigation: fixture policy"
  root.write(
    p"sys/devices/system/cpu/cpufreq/policy9/scaling_governor",
    """userspace
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpufreq/policy9/scaling_setspeed",
    """1900000
""",
  )
  let userspace = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert userspace.cpu.frequency_policies[0].governor_requested_khz == null
  assert userspace.cpu.frequency_policies[1].governor_requested_khz == 1900000
  root.write(
    p"sys/devices/system/cpu/cpufreq/policy9/scaling_setspeed",
    """invalid
""",
  )
  let malformed = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert malformed.cpu.frequency_policies[1].governor_requested_khz == null
  assert malformed.issues
    |> any .section == "cpu" and .field == "policy9.scaling_setspeed" and .state == report_model.Malformed
}

test test_system_report_cpufreq_policy_rejects_truncated_field_prefixes {
  let root = fs.tempdir()?
  defer root.close()?
  let policy_path = p"sys/devices/system/cpu/cpufreq/policy0"
  root.mkdir(policy_path, parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu0", parents: true)
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )
  root.write(p"sys/devices/system/cpu/offline", "\n")
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"{padding}{padding}"
  }

  var long_padding = padding
  while long_padding.count_chars() < 65536 {
    long_padding = f"{long_padding}{long_padding}"
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
      fp"{policy_path}/{field.name}",
      f"""{field.prefix}
{suffix}""",
    )
  }

  root.write(
    p"sys/devices/system/cpu/cpufreq/boost",
    f"""1
{padding}""",
  )

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert value.cpu.frequency_policies.len() == 1
  let policy = value.cpu.frequency_policies[0]
  assert policy.related_cpus == []
  assert policy.affected_cpus == []
  assert policy.driver == null
  assert policy.governor == null
  assert policy.scaling_min_khz == null
  assert policy.available_frequencies_khz == []
  assert policy.energy_performance_preference == null
  assert policy.boost_supported == null
  assert policy.boost_allowed == null
  assert policy.boost_scope == null
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
    assert truncated_fields |> any .field == field
  }

  root.remove(fp"{policy_path}/scaling_min_freq")
  root.mkdir(fp"{policy_path}/scaling_min_freq")
  let unreadable = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert unreadable.cpu.frequency_policies[0].scaling_min_khz == null
  let failed_minimum = unreadable.issues |> where .section == "cpu" and .field == "policy0.scaling_min_freq"
  assert failed_minimum.len() == 1
  assert failed_minimum[0].state == report_model.ReadFailure
}

test test_system_report_affinity_rejects_duplicate_status_field {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu", parents: true)
  root.mkdir(p"proc/self", parents: true)
  root.write(
    p"sys/devices/system/cpu/possible",
    """0-1
""",
  )
  root.write(
    p"sys/devices/system/cpu/present",
    """0-1
""",
  )
  root.write(
    p"sys/devices/system/cpu/online",
    """0-1
""",
  )
  root.write(p"sys/devices/system/cpu/offline", "\n")
  root.write(
    p"proc/self/status",
    """Cpus_allowed_list:	0
Cpus_allowed_list:	1
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert value.cpu.affinity == []
  assert value.issues
    |> any .section == "cpu" and .field == "affinity" and .state == report_model.Malformed and .error_kind == "duplicate_cpu_list"
}

test test_system_report_idle_governor_uses_read_only_source_when_writable_source_is_absent {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu/cpuidle", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu0", parents: true)
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )
  root.write(p"sys/devices/system/cpu/offline", "\n")
  root.write(
    p"sys/devices/system/cpu/cpuidle/current_driver",
    """intel_idle
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpuidle/current_governor_ro",
    """menu
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert value.cpu.global_idle_governor == "menu"
  assert ! (value.issues |> any .field == "cpuidle.current_governor")
  root.write(
    p"sys/devices/system/cpu/cpuidle/current_governor",
    """teo
""",
  )
  let writable = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert writable.cpu.global_idle_governor == "teo"
}

test test_system_report_idle_and_affinity_reject_truncated_prefixes {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu0/cpuidle/state0", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu0/cpuidle/state1", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu0/cpuidle/state9007199254740992", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpuidle", parents: true)
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )
  root.write(p"sys/devices/system/cpu/offline", "\n")
  var padding = " "
  while padding.count_chars() < 65536 {
    padding = f"{padding}{padding}"
  }

  root.write(
    p"proc/self/status",
    f"""Cpus_allowed_list:	0
{padding}""",
  )
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
{padding}""",
    )
  }

  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state0/disable",
    """2
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state0/latency",
    f"""12
{padding}""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state0/residency",
    """123
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state0/usage",
    """9007199254740992
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state0/time",
    f"""8
{padding}""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state1/name",
    """C1
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state1/disable",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/cpuidle/state1/latency",
    """9
""",
  )

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert value.cpu.affinity == []
  assert value.cpu.global_idle_driver == null
  assert value.cpu.global_idle_governor == null
  assert value.cpu.available_idle_governors == []
  let malformed = value.cpu.idle_states
    |> where .name == "state0"
    |> first()?
  assert malformed.state_index == 0
  assert malformed.description == null
  assert malformed.disable_setting == null
  assert malformed.latency_us == null
  assert malformed.residency_us == 123
  assert malformed.usage_count == null
  assert malformed.time_us == null
  let valid = value.cpu.idle_states
    |> where .name == "C1"
    |> first()?
  assert valid.state_index == 1
  assert valid.disable_setting == 0
  assert valid.latency_us == 9
  assert value.cpu.idle_states.len() == 2
  assert value.issues |> any .field == "cpu0.state9007199254740992" and .error_kind == "invalid_idle_state_index"
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
    assert truncated_fields |> any .field == field
  }

  assert value.issues |> any .field == "cpu0.state0.disable" and .state == report_model.Malformed
  assert value.issues |> any .field == "cpu0.state0.usage" and .state == report_model.RangeFailure

  root.remove(p"sys/devices/system/cpu/cpu0/cpuidle/state1/latency")
  root.mkdir(p"sys/devices/system/cpu/cpu0/cpuidle/state1/latency")
  let unreadable = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  let idle_state = unreadable.cpu.idle_states
    |> where .name == "C1"
    |> first()?
  assert idle_state.disable_setting == 0
  assert idle_state.latency_us == null
  let failed_latency = unreadable.issues |> where .section == "cpu" and .field == "cpu0.state1.latency"
  assert failed_latency.len() == 1
  assert failed_latency[0].state == report_model.ReadFailure
}

test test_system_report_cpuinfo_rejects_a_truncated_complete_looking_prefix {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu0", parents: true)
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )
  root.write(p"sys/devices/system/cpu/offline", "\n")
  var padding = " "
  while padding.count_chars() < 8388608 {
    padding = f"{padding}{padding}"
  }

  root.write(
    p"proc/cpuinfo",
    f"""processor: 0
model name: complete-looking prefix
{padding}""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert value.cpu.cpus.len() == 1
  assert value.cpu.cpus[0].model == null
  let matches = value.issues |> where .section == "cpu" and .field == "cpuinfo"
  assert matches.len() == 1
  assert matches[0].state == report_model.Truncated
}

test test_system_report_cpu_topology_rejects_truncated_scalar_prefixes {
  let root = fs.tempdir()?
  defer root.close()?
  let topology = p"sys/devices/system/cpu/cpu0/topology"
  root.mkdir(topology, parents: true)
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )
  root.write(p"sys/devices/system/cpu/offline", "\n")
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"{padding}{padding}"
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
      fp"{topology}/{field.name}",
      f"""{field.prefix}
{padding}""",
    )
  }

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let collected = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert collected.cpu.cpus.len() == 1
  let cpu_item = collected.cpu.cpus[0]
  assert cpu_item.package_id == null
  assert cpu_item.die_id == null
  assert cpu_item.core_id == null
  assert cpu_item.thread_siblings == []
  for field in ["physical_package_id", "die_id", "core_id", "thread_siblings_list"] {
    assert collected.issues
      |> any .section == "cpu" and .field == f"cpu0.topology.{field}" and .state == report_model.Truncated
  }

  root.write(
    fp"{topology}/core_id",
    """9007199254740992
""",
  )
  let unsafe_core = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert unsafe_core.cpu.cpus[0].core_id == null
  assert unsafe_core.issues |> any .field == "cpu0.topology.core_id" and .state == report_model.RangeFailure
}

test test_system_report_cpu_cache_sizes_reject_scaled_overflow_and_truncation {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu/cpu0/cache/index7", parents: true)
  root.mkdir(p"sys/devices/system/cpu/cpu0/cache/index8", parents: true)
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )
  root.write(p"sys/devices/system/cpu/offline", "")
  root.write(
    p"sys/devices/system/cpu/cpu0/cache/index7/level",
    """2
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/cache/index7/type",
    """Unified
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/cache/index7/size",
    """8796093022208K
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/cache/index8/level",
    """3
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/cache/index8/type",
    """Unified
""",
  )
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"{padding}{padding}"
  }

  root.write(p"sys/devices/system/cpu/cpu0/cache/index8/size", f"512K{padding}")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert value.cpu.caches.len() == 2
  for cache in value.cpu.caches {
    assert cache.size_bytes == null
  }

  let overflow = value.issues |> where .section == "cpu" and .field == "cpu0.cache.index7.size"
  assert overflow.len() == 1
  assert overflow[0].state == report_model.RangeFailure
  let truncated = value.issues |> where .section == "cpu" and .field == "cpu0.cache.index8.size"
  assert truncated.len() == 1
  assert truncated[0].state == report_model.Truncated
  let incomplete_cache = p"sys/devices/system/cpu/cpu0/cache/index9"
  root.mkdir(incomplete_cache, parents: true)
  root.write(
    fp"{incomplete_cache}/level",
    f"""4
{padding}""",
  )
  root.write(
    fp"{incomplete_cache}/type",
    """Unified
""",
  )
  root.write(
    fp"{incomplete_cache}/size",
    """1024K
""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/cache/index7/coherency_line_size",
    f"""64
{padding}""",
  )
  root.write(
    p"sys/devices/system/cpu/cpu0/cache/index7/number_of_sets",
    f"""1024
{padding}""",
  )
  let incomplete = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert incomplete.cpu.caches.len() == 2
  assert incomplete.issues |> any .field == "cpu0.cache.index9.level" and .state == report_model.Truncated
  let first_cache = incomplete.cpu.caches |> where .sysfs_index == 7
  assert first_cache.len() == 1
  assert first_cache[0].line_size_bytes == null
  assert first_cache[0].sets == null
  assert incomplete.issues |> any .field == "cpu0.cache.index7.coherency_line_size" and .state == report_model.Truncated
  assert incomplete.issues |> any .field == "cpu0.cache.index7.number_of_sets" and .state == report_model.Truncated
  root.write(
    fp"{incomplete_cache}/level",
    """4
""",
  )
  root.write(
    fp"{incomplete_cache}/type",
    f"""Unified
{padding}""",
  )
  let incomplete_kind = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert incomplete_kind.cpu.caches.len() == 2
  assert incomplete_kind.issues |> any .field == "cpu0.cache.index9.type" and .state == report_model.Truncated
}

test test_system_report_cpu_cache_rejects_ambiguous_shared_cpu_list {
  let root = fs.tempdir()?
  defer root.close()?
  let cache_path = p"sys/devices/system/cpu/cpu0/cache/index7"
  root.mkdir(cache_path, parents: true)
  root.write(
    p"sys/devices/system/cpu/possible",
    """0-1
""",
  )
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/offline",
    """1
""",
  )
  root.write(
    fp"{cache_path}/level",
    """2
""",
  )
  root.write(
    fp"{cache_path}/type",
    """Unified
""",
  )
  root.write(
    fp"{cache_path}/size",
    """1M
""",
  )
  root.write(
    fp"{cache_path}/shared_cpu_list",
    """0,,1
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert value.cpu.caches.len() == 1
  assert value.cpu.caches[0].shared_cpus == []
  let matching = value.issues |> where .section == "cpu" and .field == "cpu0.cache.index7.shared_cpu_list"
  assert matching.len() == 1
  assert matching[0].state == report_model.Malformed
}

test test_system_report_cpu_cache_keeps_distinct_kernel_ids_with_same_sharing {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/devices/system/cpu/cpu0/cache", parents: true)
  root.write(
    p"sys/devices/system/cpu/possible",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/present",
    """0
""",
  )
  root.write(
    p"sys/devices/system/cpu/online",
    """0
""",
  )
  root.write(p"sys/devices/system/cpu/offline", "\n")
  for entry in [{index: 7, kernel_id: 9}, {index: 8, kernel_id: 10}] {
    let base = fp"sys/devices/system/cpu/cpu0/cache/index{entry.index}"
    root.mkdir(base)
    root.write(
      fp"{base}/id",
      f"""{entry.kernel_id}
""",
    )
    root.write(
      fp"{base}/level",
      """2
""",
    )
    root.write(
      fp"{base}/type",
      """Unified
""",
    )
    root.write(
      fp"{base}/size",
      """1M
""",
    )
    root.write(
      fp"{base}/shared_cpu_list",
      """0
""",
    )
  }

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "cpu", true)?
  assert value.cpu.caches.len() == 2
  assert value.cpu.caches[0].sysfs_index == 7
  assert value.cpu.caches[1].sysfs_index == 8
  assert value.cpu.cpus[0].cache_ids == [0, 1]
}

test test_system_report_storage_parses_mountinfo_escapes_and_stacked_mounts {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)
  root.write(
    p"proc/self/mountinfo",
    """12 1 8:1 / /mnt/a\\040b rw,relatime - ext4 /dev/sda1 rw,errors=remount-ro,password=private-secret
13 1 8:1 / /mnt/alias rw - ext4 /dev/sda1 rw
14 1 0:2 / /mnt/remote rw - cifs //user:private-secret@server/share rw
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 65536, 250, "storage", true, true)?
  assert value.storage.mounts.len() == 3
  let first = value.storage.mounts[0]
  assert first.mount_id == 12
  assert first.target.value == "/mnt/a b"
  assert first.filesystem == "ext4"
  assert first.source.value == "/dev/sda1"
  assert first.super_options == ["rw", "errors=remount-ro", "redacted"]
  assert first.usage_state == report_model.Disappeared
  assert value.storage.mounts[1].mount_id == 13
  assert value.storage.mounts[1].usage_state == report_model.Disappeared
  assert value.storage.mounts[2].mount_id == 14
  assert value.storage.mounts[2].usage_state == report_model.NotRequested
  assert value.storage.mounts[2].source.state == report_model.Redacted
  assert value.storage.mounts[2].source.value == null
  let fixture_only = collector.collect_from_root(root, "fixture-arch", 65536, 250, "storage", true)?
  assert fixture_only.storage.mounts[0].usage_state == report_model.NotRequested
}

test test_system_report_storage_mounts_reject_truncated_complete_looking_prefix {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)
  let valid = """12 1 8:1 / /mnt/data rw - ext4 /dev/sda1 rw
"""
  var padding = "#"
  while padding.count_chars() < 4194304 {
    padding = padding + padding
  }

  root.write(p"proc/self/mountinfo", valid + padding)
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert value.storage.mounts.is_empty()
  assert value.issues |> any .section == "storage" and .field == "mounts" and .state == report_model.Truncated
}

test test_system_report_storage_usage_skips_shadowed_and_automount_descendants {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)
  root.write(
    p"proc/self/mountinfo",
    """1 0 0:1 / / rw - tmpfs tmpfs rw
2 1 8:1 / /mnt/shared rw - ext4 /dev/sda1 rw
3 1 0:3 / /mnt/shared rw - tmpfs tmpfs rw
4 1 0:4 / /mnt/auto rw - autofs autofs rw
5 4 8:2 / /mnt/auto/local rw - ext4 /dev/sdb1 rw
6 1 0:6 / /mnt/safe rw - tmpfs tmpfs rw
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true, true)?
  assert value.storage.mounts.len() == 6
  assert value.storage.mounts[0].usage_state == report_model.Observed
  for index in [1, 2, 3, 4] {
    assert value.storage.mounts[index].usage_state == report_model.NotRequested
    assert value.storage.mounts[index].usage_total_bytes == null
  }

  assert value.storage.mounts[5].usage_state == report_model.Disappeared
}

test test_system_report_storage_mount_rejects_json_unsafe_identity {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)
  root.write(
    p"proc/self/mountinfo",
    """9007199254740992 0 0:1 / /oversized-id rw - tmpfs tmpfs rw
2 0 9007199254740992:1 / /oversized-device rw - tmpfs tmpfs rw
3 0 0:1 / /valid rw - tmpfs tmpfs rw
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true, true)?
  assert value.storage.mounts.len() == 1
  assert value.storage.mounts[0].mount_id == 3
  assert value.storage.mounts[0].usage_state == report_model.NotRequested
  assert value.issues |> any .field == "mounts.line.0" and .state == report_model.RangeFailure
  assert value.issues |> any .field == "mounts.line.1" and .state == report_model.RangeFailure
}

test test_system_report_storage_links_block_devices_to_pci_controllers {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/sys/kernel/random", parents: true)
  root.mkdir(p"proc/self", parents: true)
  root.mkdir(p"etc", parents: true)
  root.mkdir(p"sys/bus/pci/devices/0001:02:03.0", parents: true)
  root.mkdir(p"sys/class/block", parents: true)
  root.mkdir(p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue", parents: true)
  root.mkdir(p"sys/devices/pci0001:02/0001:02:03.0/nvme0", parents: true)
  root.mkdir(p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/holders", parents: true)
  root.mkdir(p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/slaves", parents: true)
  root.mkdir(p"sys/bus/pci/devices/0001:02:03.0/iommu_group", parents: true)
  root.write(
    p"proc/sys/kernel/osrelease",
    """fixture-release
""",
  )
  root.write(
    p"proc/version",
    """Linux fixture version 1
""",
  )
  root.write(
    p"proc/sys/kernel/hostname",
    """fixture-host
""",
  )
  root.write(
    p"proc/sys/kernel/random/boot_id",
    """fixture-boot-id
""",
  )
  root.write(
    p"proc/uptime",
    """1.0 0.0
""",
  )
  root.write(
    p"etc/os-release",
    """ID=fixture
""",
  )
  root.write(
    p"proc/self/mountinfo",
    """12 1 259:0 / /mnt/data rw - ext4 /dev/nvme0n1 rw
""",
  )
  root.write(
    p"sys/bus/pci/devices/0001:02:03.0/vendor",
    """0x1234
""",
  )
  root.write(
    p"sys/bus/pci/devices/0001:02:03.0/device",
    """0xabcd
""",
  )
  root.write(
    p"sys/bus/pci/devices/0001:02:03.0/subsystem_vendor",
    """0x1234
""",
  )
  root.write(
    p"sys/bus/pci/devices/0001:02:03.0/subsystem_device",
    """0x0001
""",
  )
  root.write(
    p"sys/bus/pci/devices/0001:02:03.0/class",
    """0x010802
""",
  )
  root.write(
    p"sys/bus/pci/devices/0001:02:03.0/revision",
    """0x01
""",
  )
  assert root.children(p"sys/bus/pci/devices")?.children.len() == 1
  root.symlink(../../devices/pci0001:02/0001:02:03.0/block/nvme0n1, p"sys/class/block/nvme0n1")
  root.symlink(../../nvme0, p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/device")
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/dev",
    """259:0
""",
  )
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/size",
    """16
""",
  )
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/logical_block_size",
    """512
""",
  )
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/physical_block_size",
    """4096
""",
  )
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/removable",
    """0
""",
  )
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/rotational",
    """0
""",
  )
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/ro",
    """0
""",
  )
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/device/model",
    """Fixture NVMe
""",
  )
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/device/firmware_rev",
    """1.0
""",
  )
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/scheduler",
    """[none] mq-deadline
""",
  )
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/read_ahead_kb",
    """128
""",
  )
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/discard_granularity",
    """4096
""",
  )
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/queue/discard_max_bytes",
    """1048576
""",
  )
  root.write(
    p"sys/devices/pci0001:02/0001:02:03.0/block/nvme0n1/stat",
    """1 0 8 1 2 0 16 2 0 3 4
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert value.pci.functions.len() == 1
  assert value.pci.functions[0].domain == 1
  if value.storage.devices.is_empty() {
    test.fail(
      value.issues
        |> where .section == "storage"
        |> first()?.error_kind ?? "no storage issue",
    )
  }

  assert value.storage.devices.len() == 1
  assert value.storage.devices[0].kind == "disk"
  assert value.storage.devices[0].size_bytes == 8192
  assert value.storage.devices[0].parent_pci_function_index == 0
  assert value.storage.mounts[0].block_device_index == 0
}

test test_system_report_block_scheduler_requires_one_selected_choice {
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let selected = collectors.parse_block_scheduler("none [mq-deadline] kyber").require(BlockScheduler)?
  assert selected.active == "mq-deadline"
  assert selected.available == ["none", "mq-deadline", "kyber"]
  let tabbed = collectors.parse_block_scheduler("[none]\tfixture-scheduler").require(BlockScheduler)?
  assert tabbed.active == "none"
  assert tabbed.available == ["none", "fixture-scheduler"]
  for invalid in [
    "",
    "none mq-deadline",
    "[none] [mq-deadline]",
    "[] none",
    "[none] none",
    """[none]
kyber""",
  ] {
    assert collectors.parse_block_scheduler(invalid) == null
  }
}

test test_system_report_storage_rejects_invalid_block_source_fields {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/block/loop0/queue", parents: true)
  root.mkdir(p"sys/class/block/loop0/holders", parents: true)
  root.mkdir(p"sys/class/block/loop0/slaves", parents: true)
  root.mkdir(p"sys/class/block/loop0/device", parents: true)
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"{padding}{padding}"
  }

  root.write(
    p"sys/class/block/loop0/dev",
    f"""7:0
{padding}""",
  )
  root.write(
    p"sys/class/block/loop0/size",
    f"""16
{padding}""",
  )
  root.write(
    p"sys/class/block/loop0/queue/logical_block_size",
    f"""512
{padding}""",
  )
  root.write(
    p"sys/class/block/loop0/removable",
    f"""1
{padding}""",
  )
  root.write(
    p"sys/class/block/loop0/queue/scheduler",
    f"""[none] mq-deadline
{padding}""",
  )
  root.write(
    p"sys/class/block/loop0/device/model",
    f"""fixture-model
{padding}""",
  )
  root.write(
    p"sys/class/block/loop0/device/firmware_rev",
    f"""fixture-revision
{padding}""",
  )
  root.write(
    p"sys/class/block/loop0/device/rev",
    """fallback-revision
""",
  )
  root.write(
    p"sys/class/block/loop0/stat",
    f"""1 0 8 1 2 0 16 2 0 3 4
{padding}""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert value.storage.devices.len() == 1
  let device = value.storage.devices[0]
  assert device.major == null
  assert device.minor == null
  assert device.size_bytes == null
  assert device.logical_sector_bytes == null
  assert device.removable == null
  assert device.active_scheduler == null
  assert device.available_schedulers == []
  assert device.io_counters == []
  assert device.model.value == null
  assert device.firmware.value == null
  assert device.model.state == report_model.Truncated
  assert device.firmware.state == report_model.Truncated
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
    let matches = value.issues |> where .section == "storage" and .field == f"devices.loop0.{field}"
    assert matches.len() == 1
    assert matches[0].state == report_model.Truncated
  }

  root.write(
    p"sys/class/block/loop0/queue/scheduler",
    """none mq-deadline
""",
  )
  let malformed_scheduler = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert malformed_scheduler.storage.devices[0].active_scheduler == null
  assert malformed_scheduler.storage.devices[0].available_schedulers == []
  let scheduler_issues = malformed_scheduler.issues
    |> where .section == "storage" and .field == "devices.loop0.scheduler"
  assert scheduler_issues.len() == 1
  assert scheduler_issues[0].state == report_model.Malformed

  root.write(
    p"sys/class/block/loop0/queue/logical_block_size",
    """invalid
""",
  )
  root.write(
    p"sys/class/block/loop0/queue/physical_block_size",
    """9007199254740992
""",
  )
  root.write(
    p"sys/class/block/loop0/removable",
    """2
""",
  )
  root.write(
    p"sys/class/block/loop0/queue/rotational",
    """-1
""",
  )
  root.write(
    p"sys/class/block/loop0/ro",
    """invalid
""",
  )
  root.write(
    p"sys/class/block/loop0/queue/read_ahead_kb",
    """-2
""",
  )
  root.write(
    p"sys/class/block/loop0/queue/discard_granularity",
    """0x10
""",
  )
  root.write(
    p"sys/class/block/loop0/queue/discard_max_bytes",
    """9007199254740992
""",
  )
  let invalid_queue = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  let queue_device = invalid_queue.storage.devices[0]
  assert queue_device.logical_sector_bytes == null
  assert queue_device.physical_sector_bytes == null
  assert queue_device.removable == null
  assert queue_device.rotational == null
  assert queue_device.read_only == null
  assert queue_device.read_ahead_kb == null
  assert queue_device.discard_granularity_bytes == null
  assert queue_device.discard_max_bytes == null
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
    let matches = invalid_queue.issues |> where .section == "storage" and .field == f"devices.loop0.{expected.field}"
    assert matches.len() == 1
    assert matches[0].state == expected.state
  }

  root.write(
    p"sys/class/block/loop0/dev",
    """9007199254740992:0
""",
  )
  let unsafe_identity = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert unsafe_identity.storage.devices[0].major == null
  let invalid = unsafe_identity.issues |> where .section == "storage" and .field == "devices.loop0.major_minor"
  assert invalid.len() == 1
  assert invalid[0].state == report_model.RangeFailure

  root.write(
    p"sys/class/block/loop0/dev",
    """7:0
""",
  )
  root.write(
    p"sys/class/block/loop0/size",
    """17592186044415
""",
  )
  let maximum = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert maximum.storage.devices[0].size_bytes == 9007199254740480
  root.write(
    p"sys/class/block/loop0/size",
    """17592186044416
""",
  )
  let oversized = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert oversized.storage.devices[0].size_bytes == null
  let size_issues = oversized.issues |> where .section == "storage" and .field == "devices.loop0.size"
  assert size_issues.len() == 1
  assert size_issues[0].state == report_model.RangeFailure

  root.write(
    p"sys/class/block/loop0/size",
    """-0
""",
  )
  let negative_zero = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert negative_zero.storage.devices[0].size_bytes == null
  let negative_size = negative_zero.issues |> where .section == "storage" and .field == "devices.loop0.size"
  assert negative_size.len() == 1
  assert negative_size[0].state == report_model.Malformed

  root.write(
    p"sys/class/block/loop0/size",
    """16
""",
  )
  root.write(
    p"sys/class/block/loop0/stat",
    """9007199254740992 0 8 1 2 0 16 2 0 3 4
""",
  )
  let counters = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert ! (counters.storage.devices[0].io_counters |> any .name == "read_ios")
  assert counters.storage.devices[0].io_counters |> any .name == "read_sectors" and .value == 8
  let counter_issues = counters.issues |> where .section == "storage" and .field == "devices.loop0.stat.read_ios"
  assert counter_issues.len() == 1
  assert counter_issues[0].state == report_model.RangeFailure

  root.write(
    p"sys/class/block/loop0/stat",
    """1 0 8 1 2 0 16 2 0 3 4 5 6 7 8 9 10
""",
  )
  let full_stats = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  let values = full_stats.storage.devices[0].io_counters
  assert values.len() == 17
  assert values |> any .name == "discard_ios" and .value == 5
  assert values |> any .name == "discard_sectors" and .value == 7
  assert values |> any .name == "flush_ios" and .value == 9
  assert values |> any .name == "flush_ms" and .value == 10
  assert ! (full_stats.issues |> any .field == "devices.loop0.stat")

  root.write(
    p"sys/class/block/loop0/stat",
    """1 0 8 1 2 0 16 2 0 3 4 5 6
""",
  )
  let incomplete_stats = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert incomplete_stats.storage.devices[0].io_counters == []
  let incomplete_issue = incomplete_stats.issues |> where .section == "storage" and .field == "devices.loop0.stat"
  assert incomplete_issue.len() == 1
  assert incomplete_issue[0].state == report_model.Malformed

  root.write(
    p"sys/class/block/loop0/stat",
    """1 0 8 1 2 0 16 2 0 3 4 5 6 7 8 9 10 11
""",
  )
  let future_stats = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert future_stats.storage.devices[0].io_counters.len() == 17
  let unknown = future_stats.issues |> where .section == "storage" and .field == "devices.loop0.stat"
  assert unknown.len() == 1
  assert unknown[0].state == report_model.Unsupported

  root.remove(p"sys/class/block/loop0/queue/read_ahead_kb")
  root.mkdir(p"sys/class/block/loop0/queue/read_ahead_kb")
  let unreadable = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert unreadable.storage.devices[0].read_ahead_kb == null
  let failed_read_ahead = unreadable.issues |> where .section == "storage" and .field == "devices.loop0.read_ahead_kb"
  assert failed_read_ahead.len() == 1
  assert failed_read_ahead[0].state == report_model.ReadFailure
}

test test_system_report_storage_keeps_a_device_with_missing_numbers {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/block/mystery0", parents: true)
  root.write(
    p"sys/class/block/mystery0/size",
    """16
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert value.storage.devices.len() == 1
  assert value.storage.devices[0].name == "mystery0"
  assert value.storage.devices[0].major == null
  assert value.storage.devices[0].minor == null
  let missing_numbers = value.issues |> where .field == "devices.mystery0.major_minor"
  assert missing_numbers.len() == 1
  assert missing_numbers[0].state == report_model.Absent
}

test test_system_report_storage_links_layered_block_devices_by_identity {
  let root = fs.tempdir()?
  defer root.close()?
  for name in ["sda", "dm-0"] {
    root.mkdir(fp"sys/class/block/{name}/holders", parents: true)
    root.mkdir(fp"sys/class/block/{name}/slaves", parents: true)
    root.write(
      fp"sys/class/block/{name}/size",
      """16
""",
    )
  }

  root.write(
    p"sys/class/block/sda/dev",
    """8:0
""",
  )
  root.write(
    p"sys/class/block/dm-0/dev",
    """253:0
""",
  )
  root.symlink(../../dm-0, p"sys/class/block/sda/holders/dm-0")
  root.symlink(../../sda, p"sys/class/block/dm-0/slaves/sda")
  root.mkdir(p"proc/self", parents: true)
  root.write(p"proc/self/mountinfo", "")

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert value.storage.status.state == report_model.Complete
  assert value.storage.devices.len() == 2
  let base = (value.storage.devices |> where .name == "sda")[0]
  let stacked = (value.storage.devices |> where .name == "dm-0")[0]
  assert base.major == 8
  assert stacked.major == 253
  assert base.holder_indices.len() == 1
  assert stacked.slave_indices.len() == 1
  assert value.storage.devices[base.holder_indices[0]].name == "dm-0"
  assert value.storage.devices[stacked.slave_indices[0]].name == "sda"
  assert base.slave_indices == []
  assert stacked.holder_indices == []
}

test test_system_report_storage_keeps_sparse_partition_numbers_and_parent_links {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/block", parents: true)
  let disk_path = p"sys/devices/pci0000:00/0000:00:01.0/block/sda"
  root.mkdir(fp"{disk_path}/holders", parents: true)
  root.mkdir(fp"{disk_path}/slaves", parents: true)
  root.write(
    fp"{disk_path}/dev",
    """8:0
""",
  )
  root.write(
    fp"{disk_path}/size",
    """1024
""",
  )
  root.symlink(../../devices/pci0000:00/0000:00:01.0/block/sda, p"sys/class/block/sda")
  for number in [1, 3] {
    let name = f"sda{number}"
    let partition_path = fp"{disk_path}/{name}"
    root.mkdir(fp"{partition_path}/holders", parents: true)
    root.write(
      fp"{partition_path}/partition",
      f"""{number}
""",
    )
    root.write(
      fp"{partition_path}/dev",
      f"""8:{number}
""",
    )
    root.write(
      fp"{partition_path}/size",
      """128
""",
    )
    root.symlink(fp"../../devices/pci0000:00/0000:00:01.0/block/sda/{name}", fp"sys/class/block/{name}")
  }

  root.mkdir(p"proc/self", parents: true)
  root.write(p"proc/self/mountinfo", "")

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert value.storage.status.state == report_model.Complete
  assert value.storage.devices.len() == 3
  assert (value.storage.devices |> where .name == "sda2").is_empty()
  let disk = (value.storage.devices |> where .name == "sda")[0]
  assert disk.kind == "disk"
  assert disk.size_bytes == 524288
  for name in ["sda1", "sda3"] {
    let partition = (value.storage.devices |> where .name == name)[0]
    assert partition.kind == "partition"
    assert value.storage.devices[partition.parent_device_index ?? -1].name == "sda"
    assert partition.size_bytes == 65536
  }
}

test test_system_report_storage_keeps_holder_and_slave_enumeration_failures {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/block/fixture", parents: true)
  root.write(
    p"sys/class/block/fixture/dev",
    """8:0
""",
  )
  root.write(p"sys/class/block/fixture/holders", "not a directory")
  root.write(p"sys/class/block/fixture/slaves", "not a directory")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  assert value.storage.devices.len() == 1
  for field in ["devices.fixture.holders", "devices.fixture.slaves"] {
    let matches = value.issues |> where .section == "storage" and .field == field
    assert matches.len() == 1
    assert matches[0].state == report_model.ReadFailure
  }

  assert (value.issues |> where .section == "storage" and .field == "devices.fixture.sysfs_target").is_empty()

  root.write(p"sys/class/block/not-a-directory", "not a block device")
  let failed_link = collector.collect_from_root(root, "fixture-arch", 4096, 100, "storage", true)?
  let link_issues = failed_link.issues
    |> where .section == "storage" and .field == "devices.not-a-directory.sysfs_target"
  assert link_issues.len() == 1
  assert link_issues[0].state == report_model.ReadFailure
  assert link_issues[0].errno != null
}

test test_system_report_process_stat_parser_preserves_start_identity {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let stat = collector.parse_proc_stat("123 (worker (pool)) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2")?
  assert stat.pid == 123
  assert stat.parent_pid == 1
  assert stat.command == "worker (pool)"
  assert stat.thread_count == 2
  assert stat.start_ticks == 100
  assert stat.virtual_bytes == 8192
  assert stat.resident_pages == 2
  let error_kind = "SystemReportError.InvalidProcStat"
  for invalid in [
    "0x7b (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2",
    "1_23 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2",
    "9007199254740992 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2",
    "123 (worker) S 9007199254740992 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2",
    "123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 9007199254740992 8192 2",
  ] {
    test.error_kind(collector.parse_proc_stat(invalid), error_kind)
  }

  let oversized_optional = collector.parse_proc_stat(
    "123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 9007199254740992 0 100 9007199254740992 9007199254740992",
  )?
  assert oversized_optional.thread_count == null
  assert oversized_optional.virtual_bytes == null
  assert oversized_optional.resident_pages == null
  assert oversized_optional.field_issues |> any .field == "thread_count" and .state == report_model.RangeFailure
  assert oversized_optional.field_issues |> any .field == "virtual_bytes" and .state == report_model.RangeFailure
  assert oversized_optional.field_issues |> any .field == "resident_pages" and .state == report_model.RangeFailure
}

test test_system_report_process_statm_overflow_does_not_publish_stat_fallback {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)
  root.mkdir(p"proc/9007199254740992", parents: true)
  root.write(
    p"proc/123/stat",
    """123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2
""",
  )
  root.write(
    p"proc/123/statm",
    """137438953472 137438953472 0 0 0 0 0
""",
  )
  root.write(
    p"proc/123/status",
    """Uid:	1234	1234	1234	1234
""",
  )
  root.write(
    p"proc/123/cgroup",
    """0::/fixture
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 65536, 250, "processes", true)?
  assert value.processes.processes.len() == 1
  assert value.processes.processes[0].virtual_bytes == null
  assert value.processes.processes[0].resident_bytes == null
  assert value.issues |> any .field == "123.statm.virtual_bytes" and .state == report_model.RangeFailure
  assert value.issues |> any .field == "123.statm.resident_bytes" and .state == report_model.RangeFailure
  assert value.issues |> any .field == "9007199254740992.pid" and .state == report_model.RangeFailure
}

test test_system_report_process_statm_requires_all_kernel_fields {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)
  root.write(
    p"proc/123/stat",
    """123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2
""",
  )
  root.write(
    p"proc/123/statm",
    """2 1
""",
  )
  root.write(
    p"proc/123/status",
    """Uid:	1234	1234	1234	1234
""",
  )
  root.write(
    p"proc/123/cgroup",
    """0::/fixture
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 65536, 250, "processes", true)?
  assert value.processes.processes.len() == 1
  assert value.processes.processes[0].virtual_bytes == null
  assert value.processes.processes[0].resident_bytes == null
  assert value.issues |> any .field == "123.statm" and .state == report_model.Malformed

  root.write(
    p"proc/123/statm",
    """2 1 malformed 0 0 0 0
""",
  )
  let malformed = collector.collect_from_root(root, "fixture-arch", 65536, 250, "processes", true)?
  assert malformed.processes.processes[0].virtual_bytes == null
  assert malformed.processes.processes[0].resident_bytes == null
  assert malformed.issues |> any .field == "123.statm" and .state == report_model.Malformed

  root.write(
    p"proc/123/statm",
    """2 1 9007199254740992 0 0 0 0
""",
  )
  let unused_large = collector.collect_from_root(root, "fixture-arch", 65536, 250, "processes", true)?
  assert unused_large.processes.processes[0].virtual_bytes == 131072
  assert unused_large.processes.processes[0].resident_bytes == 65536
}

test test_system_report_process_cgroup_requires_one_absolute_v2_path {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)
  root.write(
    p"proc/123/stat",
    """123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2
""",
  )
  root.write(
    p"proc/123/statm",
    """2 1 0 0 0 0 0
""",
  )
  root.write(
    p"proc/123/status",
    """Uid:	1234	1234	1234	1234
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?

  root.write(
    p"proc/123/cgroup",
    """0::relative
""",
  )
  let relative = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  assert relative.processes.processes[0].cgroup.value == null
  assert relative.processes.processes[0].cgroup.state == report_model.Malformed
  assert relative.issues |> any .field == "123.cgroup" and .state == report_model.Malformed

  root.write(
    p"proc/123/cgroup",
    """0::/first
0::/second
""",
  )
  let duplicate = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  assert duplicate.processes.processes[0].cgroup.value == null
  assert duplicate.processes.processes[0].cgroup.state == report_model.Malformed

  root.write(
    p"proc/123/cgroup",
    """2:cpu:/legacy
""",
  )
  let legacy = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  assert legacy.processes.processes[0].cgroup.value == null
  assert legacy.processes.processes[0].cgroup.state == report_model.Unsupported

  root.write(
    p"proc/123/cgroup",
    """0::/tenant/worker
2:cpu:/legacy
""",
  )
  let valid = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  assert valid.processes.processes[0].cgroup.value == "/tenant/worker"
  assert valid.processes.processes[0].cgroup.state == report_model.Observed
}

test test_system_report_process_uid_requires_one_complete_numeric_status_row {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)
  root.write(
    p"proc/123/stat",
    """123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2
""",
  )
  root.write(
    p"proc/123/statm",
    """2 1 0 0 0 0 0
""",
  )
  root.write(
    p"proc/123/cgroup",
    """0::/tenant
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?

  root.write(
    p"proc/123/status",
    """Name:	worker
Uid:	1234	1235	1235	1235
""",
  )
  let valid = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  assert valid.processes.processes[0].uid == 1234
  assert (valid.issues |> where .field == "123.uid").is_empty()

  root.write(
    p"proc/123/status",
    """Uid:	1234	1235	1235	1235
Uid:	2000	2000	2000	2000
""",
  )
  let duplicate = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  assert duplicate.processes.processes[0].uid == null
  assert duplicate.issues |> any .field == "123.uid" and .state == report_model.Malformed

  root.write(
    p"proc/123/status",
    """Uid:	1234
""",
  )
  let short = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  assert short.processes.processes[0].uid == null
  assert short.issues |> any .field == "123.uid" and .state == report_model.Malformed
}

test test_system_report_process_collection_scales_pages_and_omits_private_sources {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)
  root.write(
    p"proc/123/stat",
    """123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2
""",
  )
  root.write(
    p"proc/123/statm",
    """2 1 0 0 0 0 0
""",
  )
  root.write(
    p"proc/123/status",
    """Name:	worker
Uid:	1234	1234	1234	1234
""",
  )
  root.write(
    p"proc/123/cgroup",
    """0::/fixture/group
""",
  )
  root.write(p"proc/123/environ", "PRIVATE_ENVIRONMENT_TOKEN=secret\0")
  root.write(p"proc/123/cmdline", "private-command-argument\0")
  assert root.children(p"proc")?.children.len() == 1
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 65536, 250, "processes", true)?
  assert value.scope.page_size_bytes == 65536
  assert value.scope.clock_ticks_per_second == 250
  if value.processes.processes.is_empty() {
    test.fail(
      value.issues
        |> where .section == "processes"
        |> first()?.error_kind ?? "no process issue",
    )
  }

  assert value.processes.processes.len() == 1
  let process_item = value.processes.processes[0]
  assert process_item.pid == 123
  assert process_item.parent_pid == 1
  assert process_item.uid == 1234
  assert process_item.command.value == "worker"
  assert process_item.start_ticks == 100
  assert process_item.resident_bytes == 65536
  assert process_item.virtual_bytes == 131072
  assert process_item.cgroup.value == "/fixture/group"
  assert process_item.cgroup_resource_index == null
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let sensitive_json = model.encode_report_json(value, true, false)?
  assert "PRIVATE_ENVIRONMENT_TOKEN" not in sensitive_json
  assert "private-command-argument" not in sensitive_json
  assert "\"environment\"" not in sensitive_json
  assert "\"cmdline\"" not in sensitive_json
}

test test_system_report_process_collection_rejects_truncated_stat_and_field_prefixes {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)
  let stat = """123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2
"""
  var padding = " "
  while padding.count_chars() < 16384 {
    padding = f"{padding}{padding}"
  }

  root.write(p"proc/123/stat", f"{stat}{padding}")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let truncated_stat = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  assert truncated_stat.processes.processes == []
  let stat_issues = truncated_stat.issues |> where .section == "processes" and .field == "123.stat"
  assert stat_issues.len() == 1
  assert stat_issues[0].state == report_model.Truncated

  root.write(p"proc/123/stat", stat)
  root.write(
    p"proc/123/statm",
    f"""9 8 0 0 0 0 0
{padding}""",
  )
  root.write(
    p"proc/123/status",
    f"""Uid:	1234	1234	1234	1234
{padding}""",
  )
  root.write(
    p"proc/123/cgroup",
    f"""0::/partial
{padding}""",
  )
  let truncated_fields = collector.collect_from_root(root, "fixture-arch", 4096, 100, "processes", true)?
  assert truncated_fields.processes.processes.len() == 1
  let item = truncated_fields.processes.processes[0]
  assert item.virtual_bytes == 8192
  assert item.resident_bytes == 8192
  assert item.uid == null
  assert item.cgroup.state == report_model.Truncated
  assert item.cgroup.value == null
  assert truncated_fields.issues |> any .field == "123.statm" and .state == report_model.Truncated
  assert truncated_fields.issues |> any .field == "123.status" and .state == report_model.Truncated
  assert truncated_fields.issues |> any .field == "123.cgroup" and .state == report_model.Truncated
}

test test_system_report_joins_visible_process_cgroups_to_resource_records {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/123", parents: true)
  root.mkdir(p"proc/self", parents: true)
  root.mkdir(p"sys/fs/cgroup/fixture/group", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
MemFree: 4 kB
MemAvailable: 8 kB
VendorCounter: 12 widgets
""",
  )
  root.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
""",
  )
  root.write(
    p"proc/self/cgroup",
    """0::/fixture/group
""",
  )
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw,nosuid,nodev - cgroup2 cgroup rw
""",
  )
  root.write(
    p"proc/123/stat",
    """123 (worker) S 1 1 1 0 -1 4194304 0 0 0 0 10 20 0 0 20 0 2 0 100 8192 2
""",
  )
  root.write(
    p"proc/123/statm",
    """2 1 0 0 0 0 0
""",
  )
  root.write(
    p"proc/123/status",
    """Uid:	1234	1234	1234	1234
""",
  )
  root.write(
    p"proc/123/cgroup",
    """0::/fixture/group
""",
  )
  root.write(
    p"sys/fs/cgroup/fixture/group/memory.max",
    """1048576
""",
  )
  root.write(
    p"sys/fs/cgroup/fixture/group/memory.current",
    """524288
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "", true)?
  let process_item = value.processes.processes
    |> where .pid == 123
    |> first()?
  assert process_item.cgroup.value == "/fixture/group"
  if process_item.cgroup_resource_index == null {
    test.fail("process cgroup relationship was not resolved")
  }

  let resource = value.memory.cgroup[process_item.cgroup_resource_index ?? -1]
  assert resource.path.value == process_item.cgroup.value
}

test test_system_report_memory_collects_cgroup_v2_limits_and_visible_ancestors {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)
  root.mkdir(p"sys/fs/cgroup/a/b", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
MemFree: 4 kB
MemAvailable: 8 kB
VendorCounter: 12 widgets
""",
  )
  root.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
""",
  )
  root.write(
    p"proc/self/cgroup",
    """0::/a/b
""",
  )
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw,nosuid,nodev - cgroup2 cgroup rw
""",
  )
  root.write(
    p"sys/fs/cgroup/a/b/memory.max",
    """max
""",
  )
  root.write(
    p"sys/fs/cgroup/a/b/memory.current",
    """1024
""",
  )
  root.write(
    p"sys/fs/cgroup/a/b/memory.swap.max",
    """262144
""",
  )
  root.write(
    p"sys/fs/cgroup/a/b/memory.swap.current",
    """65536
""",
  )
  root.write(
    p"sys/fs/cgroup/a/b/cpu.max",
    """50000 100000
""",
  )
  root.write(
    p"sys/fs/cgroup/a/b/cpu.stat",
    """usage_usec 9000
user_usec 7000
system_usec 2000
nr_periods 12
nr_throttled 2
throttled_usec 450
""",
  )
  root.write(
    p"sys/fs/cgroup/a/b/cpuset.cpus.effective",
    """0-1
""",
  )
  root.write(
    p"sys/fs/cgroup/a/b/pids.max",
    """max
""",
  )
  root.write(
    p"sys/fs/cgroup/a/b/pids.current",
    """8
""",
  )
  root.write(
    p"sys/fs/cgroup/a/b/io.stat",
    """8:0 rbytes=4096 wbytes=2048 rios=2 wios=1
""",
  )
  root.write(
    p"sys/fs/cgroup/a/memory.max",
    """8192
""",
  )
  root.write(
    p"sys/fs/cgroup/a/memory.current",
    """2048
""",
  )
  root.write(
    p"sys/fs/cgroup/a/cpu.max",
    """max 100000
""",
  )
  root.write(
    p"sys/fs/cgroup/a/pids.max",
    """100
""",
  )
  root.write(
    p"sys/fs/cgroup/a/pids.current",
    """12
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 65536, 250, "memory", true)?
  let memory_limits = value.memory.cgroup |> where .controller == "memory" and .resource == "memory.max"
  assert memory_limits.len() == 2
  let current_limit = memory_limits[0]
  assert current_limit.hierarchy_level == 0
  assert current_limit.maximum_unlimited == true
  assert current_limit.current_value == 1024
  assert current_limit.unit == "bytes"
  let cpu_limit = value.memory.cgroup
    |> where .resource == "cpu.max"
    |> first()?
  assert cpu_limit.quota == 50000
  assert cpu_limit.period == 100000
  let swap_limit = value.memory.cgroup
    |> where .resource == "memory.swap.max"
    |> first()?
  assert swap_limit.maximum_value == 262144
  assert swap_limit.current_value == 65536
  let cpu_usage = value.memory.cgroup
    |> where .resource == "cpu.stat.usage_usec"
    |> first()?
  assert cpu_usage.current_value == 9000
  assert cpu_usage.unit == "microseconds"
  let cpuset = value.memory.cgroup
    |> where .resource == "cpuset.cpus.effective"
    |> first()?
  assert cpuset.effective_cpus == [0, 1]
  let io_bytes = value.memory.cgroup
    |> where .resource == "io.stat.8:0.rbytes"
    |> first()?
  assert io_bytes.current_value == 4096
  assert io_bytes.unit == "bytes"
  assert value.memory.host.counters |> any .name == "VendorCounter" and .value == 12 and .unit == "widgets"

  let invalid_root = fs.tempdir()?
  defer invalid_root.close()?
  invalid_root.mkdir(p"proc/self", parents: true)
  invalid_root.write(
    p"proc/meminfo",
    """MemTotal: 16 MB
""",
  )
  invalid_root.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
""",
  )
  let invalid = collector.collect_from_root(invalid_root, "fixture-arch", 4096, 100, "memory", true)?
  assert invalid.memory.host.total_bytes == null
  assert invalid.issues |> any .field == "meminfo.MemTotal" and .state == report_model.Malformed
}

test test_system_report_memory_preserves_colons_in_cgroup_membership_path {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)
  root.mkdir(p"sys/fs/cgroup/team:blue", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )
  root.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
""",
  )
  root.write(
    p"proc/self/cgroup",
    """0::/team:blue
""",
  )
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )
  root.write(
    p"sys/fs/cgroup/team:blue/memory.max",
    """4096
""",
  )
  root.write(
    p"sys/fs/cgroup/team:blue/memory.current",
    """1024
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let limits = value.memory.cgroup |> where .resource == "memory.max" and .hierarchy_level == 0
  assert limits.len() == 1
  assert limits[0].path.value == "/team:blue"
  assert limits[0].maximum_value == 4096
  assert limits[0].current_value == 1024

  root.mkdir(p"sys/fs/cgroup/selected", parents: true)
  root.write(
    p"sys/fs/cgroup/selected/memory.max",
    """8192
""",
  )
  root.write(
    p"sys/fs/cgroup/selected/memory.current",
    """2048
""",
  )
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 /outside /sys/fs/cgroup/other rw - cgroup2 cgroup rw
32 20 0:25 /team:blue /sys/fs/cgroup/selected rw - cgroup2 cgroup rw
""",
  )
  let selected = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let selected_limit = selected.memory.cgroup
    |> where .resource == "memory.max" and .hierarchy_level == 0
    |> first()?
  assert selected_limit.path.value == "/team:blue"
  assert selected_limit.maximum_value == 8192
  assert selected_limit.current_value == 2048

  root.write(
    p"proc/self/mountinfo",
    """32 20 0:25 /team:blue /sys/fs/cgroup/selected - cgroup2 cgroup rw
""",
  )
  let malformed_mount = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert malformed_mount.memory.cgroup == []
  assert malformed_mount.issues
    |> any .section == "memory" and .field == "cgroup.mountinfo" and .state == report_model.Malformed
  root.write(
    p"proc/self/mountinfo",
    """32 20 0:25 /team:blue /sys/fs/cgroup/selected rw cgroup2 cgroup rw
""",
  )
  let missing_separator = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert missing_separator.memory.cgroup == []
  assert missing_separator.issues
    |> any .section == "memory" and .field == "cgroup.mountinfo" and .state == report_model.Malformed
  root.write(
    p"proc/self/mountinfo",
    """32 20 0:25 /team:blue /sys/fs/cgroup/selected rw - cgroup2 cgroup rw
""",
  )

  root.write(
    p"proc/self/cgroup",
    """0::/team:blue
0::/other
""",
  )
  let duplicate = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert duplicate.memory.cgroup == []
  assert duplicate.issues
    |> any .section == "memory" and .field == "cgroup.membership" and .state == report_model.Malformed
}

test test_system_report_memory_directory_failures_keep_source_issues {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  root.mkdir(p"sys/kernel/mm", parents: true)
  root.mkdir(p"sys/devices/system", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
MemFree: 8 kB
""",
  )
  root.write(p"sys/kernel/mm/hugepages", "not a directory")
  root.write(p"sys/devices/system/node", "not a directory")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let top = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert top.memory.host.total_bytes == 16384
  for field in ["huge_pages.enumeration", "numa.enumeration"] {
    let matches = top.issues |> where .section == "memory" and .field == field
    assert matches.len() == 1
    assert matches[0].state == report_model.ReadFailure
  }

  assert top.memory.status.state == report_model.Partial

  root.remove(p"sys/devices/system/node")
  root.mkdir(p"sys/devices/system/node/node0", parents: true)
  root.mkdir(p"sys/devices/system/node/nodebad", parents: true)
  root.write(p"sys/devices/system/node/node0/hugepages", "not a directory")
  let nested = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let nested_issues = nested.issues |> where .section == "memory" and .field == "numa.node0.huge_pages"
  assert nested_issues.len() == 1
  assert nested_issues[0].state == report_model.ReadFailure
  let meminfo_issues = nested.issues |> where .section == "memory" and .field == "numa.node0.meminfo"
  assert meminfo_issues.len() == 1
  assert meminfo_issues[0].state == report_model.Absent
  let invalid_node = nested.issues |> where .section == "memory" and .field == "numa.nodebad"
  assert invalid_node.len() == 1
  assert invalid_node[0].state == report_model.Malformed
}

test test_system_report_huge_page_pools_reject_unsafe_sizes_and_partial_counts {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )
  root.mkdir(p"sys/kernel/mm/hugepages/hugepages-2048kB", parents: true)
  root.mkdir(p"sys/kernel/mm/hugepages/hugepages-8796093022207kB", parents: true)
  root.mkdir(p"sys/kernel/mm/hugepages/hugepages-8796093022208kB", parents: true)
  root.mkdir(p"sys/kernel/mm/hugepages/hugepages-1024kB", parents: true)
  root.write(
    p"sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages",
    """2
""",
  )
  root.write(
    p"sys/kernel/mm/hugepages/hugepages-2048kB/free_hugepages",
    """0x1
""",
  )
  root.write(
    p"sys/kernel/mm/hugepages/hugepages-8796093022207kB/nr_hugepages",
    """1
""",
  )
  root.write(
    p"sys/kernel/mm/hugepages/hugepages-8796093022208kB/nr_hugepages",
    """1
""",
  )
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"{padding}{padding}"
  }

  root.write(
    p"sys/kernel/mm/hugepages/hugepages-1024kB/nr_hugepages",
    f"""1
{padding}""",
  )
  root.mkdir(p"sys/devices/system/node/node0/hugepages/hugepages-2048kB", parents: true)
  root.write(
    p"sys/devices/system/node/node0/hugepages/hugepages-2048kB/nr_hugepages",
    """3
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert value.memory.huge_pages.len() == 3
  let global = value.memory.huge_pages
    |> where .node_id == null and .page_size_bytes == 2097152
    |> first()?
  assert global.page_size_bytes == 2097152
  assert global.total == 2
  assert global.free == null
  let safe_edge = value.memory.huge_pages
    |> where .page_size_bytes == 9007199254739968
    |> first()?
  assert safe_edge.total == 1
  let node = value.memory.huge_pages
    |> where .node_id == 0
    |> first()?
  assert node.page_size_bytes == 2097152
  assert node.total == 3
  assert value.issues
    |> any .field == "huge_pages.hugepages-8796093022208kB.page_size" and .state == report_model.RangeFailure
  assert value.issues |> any .field == "huge_pages.hugepages-1024kB.total" and .state == report_model.Truncated
  assert value.issues |> any .field == "huge_pages.hugepages-2048kB.free" and .state == report_model.Malformed

  let empty_root = fs.tempdir()?
  defer empty_root.close()?
  empty_root.mkdir(p"proc", parents: true)
  empty_root.mkdir(p"sys/kernel/mm/hugepages", parents: true)
  empty_root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )
  let empty = collector.collect_from_root(empty_root, "fixture-arch", 4096, 100, "memory", true)?
  assert empty.memory.huge_pages.is_empty()
  assert ! (empty.issues |> any .field == "huge_pages.enumeration")
}

test test_system_report_numa_meminfo_requires_complete_rows_and_exact_bytes {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  root.mkdir(p"sys/devices/system/node/node0", parents: true)
  root.mkdir(p"sys/devices/system/node/node1", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )
  root.write(
    p"sys/devices/system/node/node0/meminfo",
    """Node 0 MemTotal: 8796093022207 kB
Node 0 MemFree: 8796093022208 kB
Node 0 Vendor: 9007199254740992 widgets
Node 1 Active: 4 kB
Node 0 Broken: 0x10 kB
""",
  )
  var padding = " "
  while padding.count_chars() < 65536 {
    padding = f"{padding}{padding}"
  }

  root.write(
    p"sys/devices/system/node/node1/meminfo",
    f"""Node 1 MemTotal: 4 kB
{padding}""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert value.memory.numa.len() == 1
  assert value.memory.numa[0].name == "node0.MemTotal"
  assert value.memory.numa[0].value == 9007199254739968
  assert value.memory.numa[0].unit == "bytes"
  assert value.issues |> any .field == "numa.node0.MemFree" and .state == report_model.RangeFailure
  assert value.issues |> any .field == "numa.node0.Vendor" and .state == report_model.RangeFailure
  assert value.issues |> any .field == "numa.node0.Active" and .state == report_model.Malformed
  assert value.issues |> any .field == "numa.node0.Broken" and .state == report_model.Malformed
  assert value.issues |> any .field == "numa.node1.meminfo" and .state == report_model.Truncated
}

test test_system_report_pressure_keeps_complete_rows_and_unavailable_sources_distinct {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/pressure", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )
  root.write(
    p"proc/pressure/cpu",
    """some avg10=0.00 avg60=1.50 avg300=2.25 total=9007199254740991
full avg10=nan avg60=0.00 avg300=0.00 total=0
""",
  )
  root.write(
    p"proc/pressure/memory",
    """some avg10=0.00 avg60=0.00 avg300=0.00 total=9007199254740992
full avg10=0.10 avg60=0.20 avg300=0.30 total=4
full avg10=0.10 avg60=0.20 avg300=0.30 total=5
""",
  )
  var padding = " "
  while padding.count_chars() < 16384 {
    padding = f"{padding}{padding}"
  }

  root.write(
    p"proc/pressure/io",
    f"""some avg10=0.00 avg60=0.00 avg300=0.00 total=1
{padding}""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert value.memory.pressure.len() == 2
  let cpu_pressure = value.memory.pressure
    |> where .resource == "cpu"
    |> first()?
  assert cpu_pressure.kind == "some"
  assert cpu_pressure.avg60 == "1.50"
  assert cpu_pressure.total_us == 9007199254740991
  assert value.issues |> any .field == "pressure.cpu.full" and .state == report_model.Malformed
  let memory = value.memory.pressure
    |> where .resource == "memory"
    |> first()?
  assert memory.kind == "full"
  assert memory.total_us == 4
  assert value.issues |> any .field == "pressure.memory.some" and .state == report_model.RangeFailure
  assert value.issues |> any .field == "pressure.memory.full" and .error_kind == "duplicate_psi_kind"
  assert value.issues |> any .field == "pressure.io" and .state == report_model.Truncated

  let unavailable_root = fs.tempdir()?
  defer unavailable_root.close()?
  unavailable_root.mkdir(p"proc", parents: true)
  unavailable_root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )
  let unavailable = collector.collect_from_root(unavailable_root, "fixture-arch", 4096, 100, "memory", true)?
  assert unavailable.memory.pressure.is_empty()
  for resource in ["cpu", "memory", "io"] {
    assert unavailable.issues |> any .field == f"pressure.{resource}" and .state == report_model.Absent
  }
}

test test_system_report_empty_pressure_file_is_malformed_beside_valid_memory_rows {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/pressure", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )
  root.write(p"proc/pressure/cpu", "")
  root.write(
    p"proc/pressure/memory",
    """some avg10=0.00 avg60=0.00 avg300=0.00 total=1
full avg10=0.00 avg60=0.00 avg300=0.00 total=0
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert (value.memory.pressure |> where .resource == "cpu").is_empty()
  assert (value.memory.pressure |> where .resource == "memory").len() == 2
  assert value.issues
    |> any .section == "memory" and .field == "pressure.cpu" and .state == report_model.Malformed and .error_kind == "empty_psi_source"
}

test test_system_report_transparent_huge_page_policy_preserves_unknown_selection {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  root.mkdir(p"sys/kernel/mm/transparent_hugepage", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )
  root.write(
    p"sys/kernel/mm/transparent_hugepage/enabled",
    """always [future_policy] never
""",
  )
  root.write(
    p"sys/kernel/mm/transparent_hugepage/defrag",
    """always defer [madvise] never
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert value.memory.transparent_huge_pages == [
    "enabled=always [future_policy] never",
    "defrag=always defer [madvise] never",
  ]

  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"{padding}{padding}"
  }

  root.write(
    p"sys/kernel/mm/transparent_hugepage/enabled",
    f"""always [future_policy] never
{padding}""",
  )
  root.write(
    p"sys/kernel/mm/transparent_hugepage/defrag",
    """always defer never
""",
  )
  let incomplete = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert incomplete.memory.transparent_huge_pages.is_empty()
  assert incomplete.issues |> any .field == "transparent_huge_pages.enabled" and .state == report_model.Truncated
  assert incomplete.issues |> any .field == "transparent_huge_pages.defrag" and .state == report_model.Malformed
}

test test_system_report_hwmon_identity_separates_duplicate_chip_names {
  let root = fs.tempdir()?
  defer root.close()?
  for entry in ["hwmon0", "hwmon1"] {
    root.mkdir(fp"sys/class/hwmon/{entry}", parents: true)
    root.write(
      fp"sys/class/hwmon/{entry}/name",
      """same_chip
""",
    )
    root.write(
      fp"sys/class/hwmon/{entry}/temp1_input",
      """42000
""",
    )
  }

  let pci_path = p"sys/devices/pci0000:00/0000:00:1f.3"
  root.mkdir(pci_path, parents: true)
  root.mkdir(p"sys/bus/pci/devices", parents: true)
  root.symlink(../../../devices/pci0000:00/0000:00:1f.3, p"sys/bus/pci/devices/0000:00:1f.3")
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
    root.write(fp"{pci_path}/{field.name}", field.value)
  }

  root.symlink(../../../devices/pci0000:00/0000:00:1f.3, p"sys/class/hwmon/hwmon0/device")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  let snapshot = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  let channels = snapshot.sensors.channels
  assert channels.len() == 2
  assert channels |> any .chip_entry_name == "hwmon0"
  assert channels |> any .chip_entry_name == "hwmon1"
  assert channels |> all .chip == "same_chip"
  let attached = channels
    |> where .chip_entry_name == "hwmon0"
    |> first()?
  assert attached.parent_pci_function_index == 0
  let sensitive = model.encode_report_json(snapshot, true, false)?
  let legacy = json.remove(json.decode(sensitive)?, ["sensors", "channels", 0, "chip_entry_name"])?
  let replay = model.decode_report_json(json.encode(legacy)?)?
  assert replay.sensors.channels[0].chip_entry_name == null
  let invalid_parent = json.set(json.decode(sensitive)?, ["sensors", "channels", 0, "parent_pci_function_index"], 99)?
  test.error_kind(model.decode_report_json(json.encode(invalid_parent)?), "SystemReportError.InvalidJson")
  root.write(
    p"sys/class/hwmon/hwmon0/inputfoo_input",
    """7
""",
  )
  let with_unfamiliar_name = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  let unfamiliar = with_unfamiliar_name.sensors.channels
    |> where .channel == "inputfoo"
    |> first()?
  assert unfamiliar.kind == "unknown"
  assert unfamiliar.unit == "raw"
}

test test_system_report_sensor_and_power_sources_keep_raw_units_and_partial_attributes {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/hwmon/hwmon0", parents: true)
  root.mkdir(p"sys/class/power_supply/BAT0", parents: true)
  root.mkdir(p"sys/class/powercap/intel-rapl:0", parents: true)
  root.mkdir(p"sys/class/powercap/intel-rapl", parents: true)
  root.write(
    p"sys/class/hwmon/hwmon0/name",
    """fixture_hwmon
""",
  )
  root.write(
    p"sys/class/hwmon/hwmon0/temp1_input",
    """42000
""",
  )
  root.write(
    p"sys/class/hwmon/hwmon0/temp1_label",
    """CPU Package
""",
  )
  root.write(
    p"sys/class/hwmon/hwmon0/temp1_max",
    """100000
""",
  )
  root.write(
    p"sys/class/hwmon/hwmon0/temp1_alarm",
    """0
""",
  )
  root.write(
    p"sys/class/hwmon/hwmon0/mystery0_input",
    """17
""",
  )
  root.write(
    p"sys/class/power_supply/BAT0/type",
    """Battery
""",
  )
  root.write(
    p"sys/class/power_supply/BAT0/status",
    """Charging
""",
  )
  root.write(
    p"sys/class/power_supply/BAT0/capacity",
    """68
""",
  )
  root.write(
    p"sys/class/power_supply/BAT0/charge_now",
    """2000000
""",
  )
  root.write(
    p"sys/class/power_supply/BAT0/charge_full",
    """3000000
""",
  )
  root.write(
    p"sys/class/power_supply/BAT0/voltage_now",
    """12000000
""",
  )
  root.write(
    p"sys/class/power_supply/BAT0/current_now",
    """-250000
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/name",
    """package-0
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/energy_uj",
    """123456
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/max_energy_range_uj",
    """999999
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_0_power_limit_uw",
    """45000000
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_0_name",
    """long_term
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_0_time_window_us",
    """1000000
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_1_power_limit_uw",
    """65000000
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_1_name",
    """short_term
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_1_time_window_us",
    """250000
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_2_power_limit_uw",
    """70000000
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_10_power_limit_uw",
    """80000000
""",
  )
  root.mkdir(p"sys/class/powercap/intel-rapl:0/intel-rapl:0:0", parents: true)
  root.write(
    p"sys/class/powercap/intel-rapl:0/intel-rapl:0:0/name",
    """core-0
""",
  )
  root.symlink(p"intel-rapl:0/intel-rapl:0:0", p"sys/class/powercap/intel-rapl:0:0")

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let no_thermal = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  assert no_thermal.sensors.thermal_zones == []
  assert no_thermal.sensors.status.state == report_model.Complete
  assert no_thermal.sensors.channels.len() == 2
  let temperature = no_thermal.sensors.channels
    |> where .channel == "temp1"
    |> first()?
  assert temperature.value == 42000
  assert temperature.unit == "millidegrees_celsius"
  assert temperature.label.value == "CPU Package"
  assert temperature.maximum == 100000
  assert temperature.alarm == false
  let unknown = no_thermal.sensors.channels
    |> where .channel == "mystery0"
    |> first()?
  assert unknown.kind == "unknown"
  assert unknown.value == 17
  assert unknown.unit == "raw"

  root.mkdir(p"sys/class/thermal/thermal_zone3", parents: true)
  root.write(
    p"sys/class/thermal/thermal_zone3/type",
    """fixture_thermal
""",
  )
  root.write(
    p"sys/class/thermal/thermal_zone3/temp",
    """41000
""",
  )
  root.write(
    p"sys/class/thermal/thermal_zone3/trip_point_0_temp",
    """95000
""",
  )
  root.write(
    p"sys/class/thermal/thermal_zone3/trip_point_0_type",
    """critical
""",
  )
  root.write(
    p"sys/class/thermal/thermal_zone3/trip_point_0_hyst",
    """2000
""",
  )
  root.write(
    p"sys/class/thermal/thermal_zone3/trip_point_2_temp",
    """85000
""",
  )
  root.write(
    p"sys/class/thermal/thermal_zone3/trip_point_2_type",
    """passive
""",
  )
  let thermal = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  assert thermal.sensors.status.state == report_model.Complete
  assert thermal.sensors.thermal_zones.len() == 1
  assert thermal.sensors.thermal_zones[0].id == 3
  let thermal_trip_indexes = thermal.sensors.thermal_zones[0].trips |> map .index
  let expected_trip_indexes: List[Int?] = [0, 2]
  assert thermal_trip_indexes == expected_trip_indexes
  assert thermal.sensors.thermal_zones[0].temperature_millidegrees == 41000
  assert thermal.sensors.thermal_zones[0].trips[0].temperature_millidegrees == 95000
  assert thermal.sensors.thermal_zones[0].trips[0].hysteresis_millidegrees == 2000
  root.mkdir(p"sys/class/thermal/thermal_zone03", parents: true)
  let malformed_zone = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  assert malformed_zone.sensors.thermal_zones.len() == 1
  assert malformed_zone.issues |> any .field == "thermal_zones.thermal_zone03" and .state == report_model.Malformed
  root.write(
    p"sys/class/thermal/thermal_zone3/trip_point_02_temp",
    """85000
""",
  )
  let malformed_trip = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  let malformed_trip_indexes = malformed_trip.sensors.thermal_zones[0].trips |> map .index
  assert malformed_trip_indexes == expected_trip_indexes
  assert malformed_trip.issues
    |> any .field == "thermal_zones.thermal_zone3.trip_point_02_temp" and .state == report_model.Malformed

  let power = collector.collect_from_root(root, "fixture-arch", 4096, 100, "power", true)?
  assert power.power.status.state == report_model.Complete
  assert power.power.supplies.len() == 1
  assert power.power.supplies[0].capacity_percent == 68
  assert power.power.supplies[0].charge_now_uah == 2000000
  assert power.power.supplies[0].current_now_ua == -250000
  assert power.power.supplies[0].energy_now_uwh == null
  assert power.power.cap_zones.len() == 2
  let package_zone = power.power.cap_zones
    |> where .name == "package-0"
    |> first()?
  assert package_zone.entry_name == "intel-rapl:0"
  assert package_zone.parent == null
  assert package_zone.energy_uj == 123456
  assert package_zone.constraints.len() == 4
  assert (package_zone.constraints |> map .index) == [0, 1, 2, 10]
  let long_term = package_zone.constraints
    |> where .index == 0
    |> first()?
  assert long_term.name == "long_term"
  assert long_term.power_limit_uw == 45000000
  assert long_term.time_window_us == 1000000
  let short_term = package_zone.constraints
    |> where .index == 1
    |> first()?
  assert short_term.name == "short_term"
  assert short_term.power_limit_uw == 65000000
  assert short_term.time_window_us == 250000
  let core_zone = power.power.cap_zones
    |> where .name == "core-0"
    |> first()?
  assert core_zone.entry_name == "intel-rapl:0:0"
  assert core_zone.parent == "intel-rapl:0"
}

test test_system_report_sensor_units_and_powercap_ranges_are_bounded {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/hwmon/hwmon0", parents: true)
  root.mkdir(p"sys/class/power_supply/BAT0", parents: true)
  root.mkdir(p"sys/class/powercap/intel-rapl:0", parents: true)
  root.write(
    p"sys/class/hwmon/hwmon0/name",
    """fixture_hwmon
""",
  )
  root.write(
    p"sys/class/hwmon/hwmon0/temp1_input",
    """42 C
""",
  )
  root.write(
    p"sys/class/power_supply/BAT0/capacity",
    """101
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/name",
    """package-0
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/energy_uj",
    """9007199254740992
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/max_energy_range_uj",
    """999999999999999999999999
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_0_power_limit_uw",
    """-1
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_0_time_window_us",
    """1000000
""",
  )
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_bad_power_limit_uw",
    """1000000
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let sensors = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  assert sensors.sensors.channels.len() == 1
  assert sensors.sensors.channels[0].value == null
  let sensor_issues = sensors.issues |> where .section == "sensors" and .field == "hwmon.hwmon0.temp1_input"
  assert sensor_issues.len() == 1
  assert sensor_issues[0].state == report_model.Malformed

  let power = collector.collect_from_root(root, "fixture-arch", 4096, 100, "power", true)?
  assert power.power.supplies[0].capacity_percent == null
  let capacity_issues = power.issues |> where .section == "power" and .field == "supplies.BAT0.capacity"
  assert capacity_issues.len() == 1
  assert capacity_issues[0].state == report_model.Malformed
  assert power.power.cap_zones.len() == 1
  assert power.power.cap_zones[0].energy_uj == null
  assert power.power.cap_zones[0].maximum_energy_range_uj == null
  assert power.power.cap_zones[0].constraints.len() == 1
  assert power.power.cap_zones[0].constraints[0].power_limit_uw == null
  assert power.power.cap_zones[0].constraints[0].time_window_us == 1000000
  for field in ["cap_zones.intel-rapl:0.energy_uj", "cap_zones.intel-rapl:0.max_energy_range_uj"] {
    let matches = power.issues |> where .section == "power" and .field == field
    assert matches.len() == 1
    assert matches[0].state == report_model.RangeFailure
  }

  let negative_limit = power.issues
    |> where .section == "power" and .field == "cap_zones.intel-rapl:0.constraint_0_power_limit_uw"
  assert negative_limit.len() == 1
  assert negative_limit[0].state == report_model.Malformed
  let invalid_index = power.issues
    |> where .section == "power" and .field == "cap_zones.intel-rapl:0.constraint_bad_power_limit_uw"
  assert invalid_index.len() == 1
  assert invalid_index[0].error_kind == "invalid_constraint_index"
}

test test_system_report_thermal_and_battery_reads_reject_truncated_prefixes {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/hwmon/hwmon0", parents: true)
  root.mkdir(p"sys/class/thermal/thermal_zone0", parents: true)
  root.mkdir(p"sys/class/power_supply/BAT0", parents: true)
  root.write(
    p"sys/class/hwmon/hwmon0/temp1_input",
    """-5000
""",
  )
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"{padding}{padding}"
  }

  root.write(p"sys/class/hwmon/hwmon0/name", f"chip-prefix{padding}")
  root.write(p"sys/class/thermal/thermal_zone0/temp", f"41000{padding}")
  root.write(p"sys/class/thermal/thermal_zone0/type", f"zone-prefix{padding}")
  root.write(
    p"sys/class/thermal/thermal_zone0/trip_point_0_temp",
    """95000
""",
  )
  root.write(p"sys/class/thermal/thermal_zone0/trip_point_0_type", f"critical{padding}")
  root.write(p"sys/class/power_supply/BAT0/capacity", f"68{padding}")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let sensors = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  assert sensors.sensors.channels[0].value == -5000
  assert sensors.sensors.channels[0].chip == "hwmon0"
  assert sensors.sensors.thermal_zones[0].temperature_millidegrees == null
  assert sensors.sensors.thermal_zones[0].kind == null
  assert sensors.sensors.thermal_zones[0].trips[0].kind == "unknown"
  for field in [
    "hwmon.hwmon0.name",
    "thermal_zones.thermal_zone0.type",
    "thermal_zones.thermal_zone0.trip_point_0_type",
  ] {
    let matches = sensors.issues |> where .section == "sensors" and .field == field
    assert matches.len() == 1
    assert matches[0].state == report_model.Truncated
  }

  let thermal_issues = sensors.issues |> where .section == "sensors" and .field == "thermal_zones.thermal_zone0.temp"
  assert thermal_issues.len() == 1
  assert thermal_issues[0].state == report_model.Truncated

  let power = collector.collect_from_root(root, "fixture-arch", 4096, 100, "power", true)?
  assert power.power.supplies[0].capacity_percent == null
  let battery_issues = power.issues |> where .section == "power" and .field == "supplies.BAT0.capacity"
  assert battery_issues.len() == 1
  assert battery_issues[0].state == report_model.Truncated
}

test test_system_report_nested_sensor_and_power_directories_keep_issues {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/hwmon", parents: true)
  root.mkdir(p"sys/class/thermal", parents: true)
  root.mkdir(p"sys/class/powercap", parents: true)
  root.write(p"sys/class/hwmon/hwmon0", "not a directory")
  root.write(p"sys/class/thermal/thermal_zone0", "not a directory")
  root.write(p"sys/class/powercap/intel-rapl:0", "not a directory")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let sensors = collector.collect_from_root(root, "fixture-arch", 4096, 100, "sensors", true)?
  for field in ["hwmon.hwmon0.attributes", "thermal_zones.thermal_zone0.attributes"] {
    let matches = sensors.issues |> where .section == "sensors" and .field == field
    assert matches.len() == 1
    assert matches[0].state == report_model.ReadFailure
  }

  assert sensors.sensors.status.state == report_model.Partial

  let power = collector.collect_from_root(root, "fixture-arch", 4096, 100, "power", true)?
  for field in ["cap_zones.intel-rapl:0.name", "cap_zones.intel-rapl:0.attributes"] {
    let matches = power.issues |> where .section == "power" and .field == field
    assert matches.len() == 1
    assert matches[0].state == report_model.ReadFailure
  }

  assert power.power.status.state == report_model.Partial
}

test test_system_report_powercap_enumeration_failure_is_partial {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class", parents: true)
  root.write(p"sys/class/powercap", "not a directory")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "power", true)?
  let matches = value.issues |> where .section == "power" and .field == "cap_zones"
  assert matches.len() == 1
  assert matches[0].state == report_model.ReadFailure
  assert value.power.status.state == report_model.Partial
  assert value.power.status.enumeration_succeeded == false
}

test test_system_report_powercap_rejects_truncated_names {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/powercap/intel-rapl:0", parents: true)
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"{padding}{padding}"
  }

  root.write(p"sys/class/powercap/intel-rapl:0/name", f"package-prefix{padding}")
  root.write(
    p"sys/class/powercap/intel-rapl:0/constraint_0_power_limit_uw",
    """45000000
""",
  )
  root.write(p"sys/class/powercap/intel-rapl:0/constraint_0_name", f"limit-prefix{padding}")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "power", true)?
  assert value.power.cap_zones.len() == 1
  assert value.power.cap_zones[0].name == "intel-rapl:0"
  assert value.power.cap_zones[0].constraints[0].name == null
  for field in ["cap_zones.intel-rapl:0.name", "cap_zones.intel-rapl:0.constraint_0_name"] {
    let matches = value.issues |> where .section == "power" and .field == field
    assert matches.len() == 1
    assert matches[0].state == report_model.Truncated
  }

  assert value.power.status.state == report_model.Partial
}

test test_system_report_power_supply_rejects_truncated_text_attributes {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/class/power_supply/BAT0", parents: true)
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"{padding}{padding}"
  }

  root.write(p"sys/class/power_supply/BAT0/type", f"Battery{padding}")
  root.write(p"sys/class/power_supply/BAT0/status", f"Charging{padding}")
  root.write(p"sys/class/power_supply/BAT0/health", f"Good{padding}")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "power", true)?
  assert value.power.supplies.len() == 1
  assert value.power.supplies[0].kind == null
  assert value.power.supplies[0].status == null
  assert value.power.supplies[0].health == null
  for field in ["supplies.BAT0.type", "supplies.BAT0.status", "supplies.BAT0.health"] {
    let matches = value.issues |> where .section == "power" and .field == field
    assert matches.len() == 1
    assert matches[0].state == report_model.Truncated
  }

  assert value.power.status.state == report_model.Partial
}

test test_system_report_memory_reports_malformed_and_oversized_meminfo_fields {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
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
  )

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert value.memory.host.total_bytes == null
  assert value.memory.host.free_bytes == null
  assert value.memory.host.cached_bytes == null
  assert value.memory.host.available_bytes == 8192
  assert value.memory.host.active_bytes == 9007199254739968
  let invalid_row = value.issues |> where .field == "meminfo.line.0"
  assert invalid_row.len() == 1
  assert invalid_row[0].state == report_model.Malformed
  let invalid_unit = value.issues |> where .field == "meminfo.MemTotal"
  assert invalid_unit[0].error_kind == "invalid_byte_counter_unit"
  let empty_value = value.issues |> where .field == "meminfo.MemFree"
  assert empty_value[0].error_kind == "missing_integer"
  let overflow = value.issues |> where .field == "meminfo.Cached"
  assert overflow[0].state == report_model.RangeFailure
  let vendor_overflow = value.issues |> where .field == "meminfo.VendorHuge"
  assert vendor_overflow.len() == 1
  assert vendor_overflow[0].state == report_model.RangeFailure
  assert vendor_overflow[0].error_kind == "json_integer_out_of_range"
  assert ! (value.memory.host.counters |> any .name == "VendorHuge")
  let vendor = value.memory.host.counters |> where .name == "VendorCounter"
  assert vendor.len() == 1
  assert vendor[0].value == 12
  assert vendor[0].unit == "widgets"
}

test test_system_report_memory_accepts_tabbed_values_and_withholds_duplicate_fields {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal:	16	kB
MemTotal: 32 kB
MemFree:	4	kB
VendorCounter:	12	widgets
""",
  )

  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert value.memory.host.total_bytes == null
  assert value.memory.host.free_bytes == 4096
  assert ! (value.memory.host.counters |> any .name == "MemTotal")
  let free = value.memory.host.counters |> where .name == "MemFree"
  assert free.len() == 1
  assert free[0].value == 4096
  assert free[0].unit == "bytes"
  let vendor = value.memory.host.counters |> where .name == "VendorCounter"
  assert vendor.len() == 1
  assert vendor[0].value == 12
  assert vendor[0].unit == "widgets"
  let duplicates = value.issues |> where .field == "meminfo.MemTotal" and .error_kind == "duplicate_field"
  assert duplicates.len() == 1
}

test test_system_report_cgroup_inventory_rejects_partial_membership_and_mounts {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)
  root.mkdir(p"sys/fs/cgroup/group", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )
  root.write(
    p"sys/fs/cgroup/group/memory.max",
    """1048576
""",
  )
  root.write(
    p"sys/fs/cgroup/group/memory.current",
    """512
""",
  )
  var membership_padding = " "
  while membership_padding.count_chars() < 65536 {
    membership_padding = f"{membership_padding}{membership_padding}"
  }

  root.write(
    p"proc/self/cgroup",
    f"""0::/group
{membership_padding}""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let membership = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert membership.memory.cgroup.is_empty()
  assert membership.issues
    |> any .section == "memory" and .field == "cgroup.membership" and .state == report_model.Truncated

  root.write(
    p"proc/self/cgroup",
    """0::/group
""",
  )
  var mount_padding = " "
  while mount_padding.count_chars() < 4194304 {
    mount_padding = f"{mount_padding}{mount_padding}"
  }

  root.write(
    p"proc/self/mountinfo",
    f"""31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
{mount_padding}""",
  )
  let mount = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert mount.memory.cgroup.is_empty()
  assert mount.issues |> any .section == "memory" and .field == "cgroup.mountinfo" and .state == report_model.Truncated
}

test test_system_report_cgroup_limits_keep_source_and_numeric_failures {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)
  root.mkdir(p"sys/fs/cgroup/group", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )
  root.write(
    p"proc/self/cgroup",
    """0::/group
""",
  )
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )
  root.write(
    p"sys/fs/cgroup/group/memory.max",
    """1048576
""",
  )
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"{padding}{padding}"
  }

  root.write(
    p"sys/fs/cgroup/group/memory.current",
    f"""512
{padding}""",
  )
  root.write(
    p"sys/fs/cgroup/group/memory.swap.max",
    """0x10
""",
  )
  root.write(
    p"sys/fs/cgroup/group/memory.swap.current",
    """65536
""",
  )
  root.write(
    p"sys/fs/cgroup/group/pids.max",
    """9007199254740992
""",
  )
  root.write(
    p"sys/fs/cgroup/group/pids.current",
    """8
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let memory_limit = value.memory.cgroup
    |> where .resource == "memory.max"
    |> first()?
  assert memory_limit.maximum_value == 1048576
  assert memory_limit.current_value == null
  assert memory_limit.state == report_model.Truncated
  let swap_limit = value.memory.cgroup
    |> where .resource == "memory.swap.max"
    |> first()?
  assert swap_limit.maximum_value == null
  assert swap_limit.current_value == 65536
  assert swap_limit.state == report_model.Malformed
  let pids_limit = value.memory.cgroup
    |> where .resource == "pids.max"
    |> first()?
  assert pids_limit.maximum_value == null
  assert pids_limit.current_value == 8
  assert pids_limit.state == report_model.RangeFailure
  assert value.issues |> any .field == "cgroup.0.memory.current" and .state == report_model.Truncated
  assert value.issues |> any .field == "cgroup.0.memory.swap.max" and .state == report_model.Malformed
  assert value.issues |> any .field == "cgroup.0.pids.max" and .state == report_model.RangeFailure
}

test test_system_report_cgroup_cpu_and_io_counters_reject_partial_and_unsafe_values {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)
  root.mkdir(p"sys/fs/cgroup/group", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )
  root.write(
    p"proc/self/cgroup",
    """0::/group
""",
  )
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )
  root.write(
    p"sys/fs/cgroup/group/cpu.max",
    """50000 100000
""",
  )
  root.write(
    p"sys/fs/cgroup/group/cpu.stat",
    """usage_usec 9007199254740992
user_usec 3
""",
  )
  root.write(
    p"sys/fs/cgroup/group/io.stat",
    """8:0 rbytes=9007199254740992 wbytes=1024
""",
  )
  var cpuset_padding = " "
  while cpuset_padding.count_chars() < 65536 {
    cpuset_padding = f"{cpuset_padding}{cpuset_padding}"
  }

  root.write(
    p"sys/fs/cgroup/group/cpuset.cpus.effective",
    f"""0-1
{cpuset_padding}""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let cpu_limit = value.memory.cgroup
    |> where .resource == "cpu.max"
    |> first()?
  assert cpu_limit.quota == 50000
  assert cpu_limit.period == 100000
  assert value.memory.cgroup |> any .resource == "cpu.stat.user_usec" and .current_value == 3
  assert ! (value.memory.cgroup |> any .resource == "cpu.stat.usage_usec")
  assert value.memory.cgroup |> any .resource == "io.stat.8:0.wbytes" and .current_value == 1024
  assert ! (value.memory.cgroup |> any .resource == "io.stat.8:0.rbytes")
  assert ! (value.memory.cgroup |> any .resource == "cpuset.cpus.effective")
  assert value.issues |> any .field == "cgroup.0.cpu.stat.usage_usec" and .state == report_model.RangeFailure
  assert value.issues |> any .field == "cgroup.0.io.stat.8:0.rbytes" and .state == report_model.RangeFailure
  assert value.issues |> any .field == "cgroup.0.cpuset.cpus.effective" and .state == report_model.Truncated

  root.write(
    p"sys/fs/cgroup/group/cpu.stat",
    """usage_usec 1
usage_usec 2
user_usec 3
""",
  )
  root.write(
    p"sys/fs/cgroup/group/io.stat",
    """8:0 rbytes=1 rbytes=2 wbytes=4
""",
  )
  let duplicate_fields = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert ! (duplicate_fields.memory.cgroup |> any .resource == "cpu.stat.usage_usec")
  assert duplicate_fields.memory.cgroup |> any .resource == "cpu.stat.user_usec" and .current_value == 3
  assert ! (duplicate_fields.memory.cgroup |> any .resource == "io.stat.8:0.rbytes")
  assert duplicate_fields.memory.cgroup |> any .resource == "io.stat.8:0.wbytes" and .current_value == 4
  assert duplicate_fields.issues |> any .field == "cgroup.0.cpu.stat.usage_usec" and .state == report_model.Malformed
  assert duplicate_fields.issues |> any .field == "cgroup.0.io.stat.8:0.rbytes" and .state == report_model.Malformed

  root.write(
    p"sys/fs/cgroup/group/io.stat",
    """8:0 rbytes=1
8:0 wbytes=2
9:0 rbytes=7
""",
  )
  let duplicate_device = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert ! (duplicate_device.memory.cgroup |> any .resource.starts_with("io.stat.8:0."))
  assert duplicate_device.memory.cgroup |> any .resource == "io.stat.9:0.rbytes" and .current_value == 7
  assert duplicate_device.issues |> any .field == "cgroup.0.io.stat.8:0" and .state == report_model.Malformed

  root.write(
    p"sys/fs/cgroup/group/io.stat",
    "7:7 " + """
8:0 rbytes=17
""",
  )
  let empty_device = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert empty_device.memory.cgroup |> any .resource == "io.stat.8:0.rbytes" and .current_value == 17
  assert ! (empty_device.memory.cgroup |> any .resource.starts_with("io.stat.7:7."))
  assert ! (empty_device.issues |> any .field == "cgroup.0.io.stat")

  var cpu_padding = " "
  while cpu_padding.count_chars() < 4096 {
    cpu_padding = f"{cpu_padding}{cpu_padding}"
  }

  root.write(
    p"sys/fs/cgroup/group/cpu.max",
    f"""50000 100000
{cpu_padding}""",
  )
  let truncated = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert ! (truncated.memory.cgroup |> any .resource == "cpu.max")
  assert truncated.issues |> any .field == "cgroup.0.cpu.max" and .state == report_model.Truncated

  var cpu_stat_padding = " "
  while cpu_stat_padding.count_chars() < 16384 {
    cpu_stat_padding = f"{cpu_stat_padding}{cpu_stat_padding}"
  }

  var io_padding = " "
  while io_padding.count_chars() < 262144 {
    io_padding = f"{io_padding}{io_padding}"
  }

  root.write(
    p"sys/fs/cgroup/group/cpu.stat",
    f"""user_usec 3
{cpu_stat_padding}""",
  )
  root.write(
    p"sys/fs/cgroup/group/io.stat",
    f"""8:0 wbytes=1024
{io_padding}""",
  )
  let incomplete_counters = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert ! (incomplete_counters.memory.cgroup |> any .resource == "cpu.stat.user_usec")
  assert ! (incomplete_counters.memory.cgroup |> any .resource == "io.stat.8:0.wbytes")
  assert incomplete_counters.issues |> any .field == "cgroup.0.cpu.stat" and .state == report_model.Truncated
  assert incomplete_counters.issues |> any .field == "cgroup.0.io.stat" and .state == report_model.Truncated
}

test test_system_report_cgroup_hybrid_keeps_v2_values_and_v1_limitation {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)
  root.mkdir(p"sys/fs/cgroup/group", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )
  root.write(
    p"proc/self/cgroup",
    """0::/group
2:cpu:/legacy
""",
  )
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
32 20 0:26 / /sys/fs/cgroup/cpu rw - cgroup cgroup rw,cpu
""",
  )
  root.write(
    p"sys/fs/cgroup/group/memory.max",
    """1048576
""",
  )
  root.write(
    p"sys/fs/cgroup/group/memory.current",
    """512
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let memory_limit = value.memory.cgroup
    |> where .resource == "memory.max"
    |> first()?
  assert memory_limit.maximum_value == 1048576
  assert memory_limit.current_value == 512
  assert value.issues |> any .field == "cgroup.v1" and .state == report_model.Unsupported

  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw - cgroup2 cgroup rw
""",
  )
  let hidden_v1_mount = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert hidden_v1_mount.memory.cgroup |> any .resource == "memory.max" and .maximum_value == 1048576
  assert hidden_v1_mount.issues |> any .field == "cgroup.v1" and .state == report_model.Unsupported
}

test test_system_report_memory_marks_an_empty_meminfo_file_malformed {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  root.write(p"proc/meminfo", "")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert value.memory.host.total_bytes == null
  let empty_file = value.issues |> where .field == "meminfo"
  assert empty_file.len() == 1
  assert empty_file[0].state == report_model.Malformed
}

test test_system_report_memory_does_not_parse_truncated_meminfo_prefix {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  var padding = " "
  while padding.count_chars() < 1048576 {
    padding = f"{padding}{padding}"
  }

  root.write(
    p"proc/meminfo",
    f"""MemTotal: 16 kB
{padding}""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert value.memory.host.total_bytes == null
  let matches = value.issues |> where .section == "memory" and .field == "meminfo"
  assert matches.len() == 1
  assert matches[0].state == report_model.Truncated
}

test test_system_report_swap_devices_keep_exact_bytes_and_reject_partial_sources {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )
  root.write(
    p"proc/swaps",
    """Filename	Type	Size	Used	Priority
/dev/zram0	partition	8796093022207	1	42
/swap\\040file	file	4	0	-1
/dev/zram1	partition	8796093022208	0	10
/dev/broken partition 4
/dev/badprio	partition	4	0	invalid
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert value.memory.swaps.len() == 2
  assert value.memory.swaps[0].name.value == "/dev/zram0"
  assert value.memory.swaps[0].size_bytes == 9007199254739968
  assert value.memory.swaps[0].used_bytes == 1024
  assert value.memory.swaps[0].priority == 42
  assert value.memory.swaps[1].name.value == "/swap file"
  assert value.memory.swaps[1].kind == "file"
  assert value.memory.swaps[1].priority == -1
  let overflow = value.issues
    |> where .section == "memory" and .field == "swaps" and .state == report_model.RangeFailure
  assert overflow.len() == 1
  assert overflow[0].state == report_model.RangeFailure
  let malformed = value.issues |> where .section == "memory" and .field == "swaps" and .error_kind == "invalid_swap_row"
  assert malformed.len() == 1
  let invalid_priority = value.issues
    |> where .section == "memory" and .field == "swaps" and .error_kind == "invalid_swap_priority"
  assert invalid_priority.len() == 1

  var padding = " "
  while padding.count_chars() < 262144 {
    padding = f"{padding}{padding}"
  }

  root.write(
    p"proc/swaps",
    f"""Filename Type Size Used Priority
/dev/zram0 partition 4 1 42
{padding}""",
  )
  let truncated = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert truncated.memory.swaps.is_empty()
  let incomplete = truncated.issues |> where .section == "memory" and .field == "swaps"
  assert incomplete.len() == 1
  assert incomplete[0].state == report_model.Truncated

  root.write(
    p"proc/swaps",
    """/dev/zram0 partition 4 1 42
""",
  )
  let headerless = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  assert headerless.memory.swaps.is_empty()
  assert headerless.issues |> any .field == "swaps" and .error_kind == "invalid_swap_header"
}

test test_system_report_swap_devices_reject_duplicate_identity_and_impossible_usage {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )
  root.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
/dev/zram0 partition 4 1 42
/dev/zram0 partition 4 0 42
/dev/bad partition 4 5 1
/swap\\040file file 8 0 -1
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let swap_names = value.memory.swaps |> map .name.value
  let expected_swap_names: List[Str?] = ["/dev/zram0", "/swap file"]
  assert swap_names == expected_swap_names
  let duplicates = value.issues
    |> where .section == "memory" and .field == "swaps" and .error_kind == "duplicate_swap_name"
  let overused = value.issues
    |> where .section == "memory" and .field == "swaps" and .error_kind == "invalid_swap_usage"
  assert duplicates.len() == 1
  assert overused.len() == 1
  assert value.memory.status.state == report_model.Partial
}

test test_system_report_kernel_modules_reject_truncated_source_prefix {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/cmdline",
    """quiet
""",
  )
  var padding = " "
  while padding.count_chars() < 1048576 {
    padding = f"{padding}{padding}"
  }

  root.write(
    p"proc/modules",
    f"""example 4096 0 - Live 0x0
{padding}""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "kernel", true)?
  assert value.kernel.modules == []
  assert ! value.kernel.status.enumeration_succeeded
  let matches = value.issues |> where .section == "kernel" and .field == "modules"
  assert matches.len() == 1
  assert matches[0].state == report_model.Truncated
}

test test_system_report_kernel_modules_keep_valid_rows_with_malformed_neighbor {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/cmdline",
    """quiet
""",
  )
  root.write(
    p"proc/modules",
    """example 4096 1 - Live 0x0
broken row
large 9007199254740992 0 - Live 0x0
busy 4096 9007199254740992 - Live 0x0
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "kernel", true)?
  assert value.kernel.status.enumeration_succeeded
  assert value.kernel.status.state == report_model.Partial
  assert value.kernel.modules.len() == 1
  assert value.kernel.modules[0].name == "example"
  assert value.kernel.modules[0].size_bytes == 4096
  let malformed = value.issues |> where .section == "kernel" and .field == "modules.line.1"
  assert malformed.len() == 1
  assert malformed[0].state == report_model.Malformed
  let oversized = value.issues |> where .section == "kernel" and .state == report_model.RangeFailure
  assert oversized.len() == 2
  assert oversized |> any .field == "modules.line.2"
  assert oversized |> any .field == "modules.line.3"
}

test test_system_report_kernel_modules_accept_taint_flags_and_reject_extra_columns {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/cmdline",
    """quiet
""",
  )
  root.write(
    p"proc/modules",
    """tainted 4096 1 - Live 0x0 (OE)
extra 2048 0 - Live 0x1 (OE) unknown
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "kernel", true)?
  assert value.kernel.status.enumeration_succeeded
  assert value.kernel.modules.len() == 1
  assert value.kernel.modules[0].name == "tainted"
  assert value.kernel.modules[0].users == 1
  let malformed = value.issues |> where .section == "kernel" and .field == "modules.line.1"
  assert malformed.len() == 1
  assert malformed[0].state == report_model.Malformed
}

test test_system_report_kernel_modules_preserve_unavailable_use_count {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/cmdline",
    """quiet
""",
  )
  root.write(
    p"proc/modules",
    """permanent 4096 - - Live 0x0
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "kernel", true)?
  assert value.kernel.status.enumeration_succeeded
  assert value.kernel.modules.len() == 1
  assert value.kernel.modules[0].name == "permanent"
  assert value.kernel.modules[0].users == null
  assert (value.issues |> where .section == "kernel" and .field.starts_with("modules")).is_empty()
  let model = module.load(p"core/lib/system_report.xsh")?.require(SystemReportModel)?
  assert "\"permanent\" size=4096 bytes users=unknown state=\"Live\"" in model.render_text(value, true, true)?
}

test test_system_report_kernel_modules_reject_duplicate_identity_with_valid_neighbor {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/cmdline",
    """quiet
""",
  )
  root.write(
    p"proc/modules",
    """alpha 4096 1 - Live 0x0
alpha 4096 2 - Live 0x1
beta 8192 0 - Live 0x2
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "kernel", true)?
  assert value.kernel.status.enumeration_succeeded
  assert value.kernel.status.state == report_model.Partial
  assert (value.kernel.modules |> map .name) == ["alpha", "beta"]
  assert value.kernel.modules[0].users == 1
  let duplicates = value.issues |> where .section == "kernel" and .field == "modules.line.1"
  assert duplicates.len() == 1
  assert duplicates[0].state == report_model.Malformed
  assert duplicates[0].error_kind == "duplicate_module_name"
}

test test_system_report_kernel_command_line_preserves_source_whitespace {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc", parents: true)
  let source = """  root=UUID=private  quiet  
"""
  root.write(p"proc/cmdline", source)
  root.write(p"proc/modules", "")
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "kernel", true)?
  assert value.kernel.command_line.state == report_model.Observed
  assert value.kernel.command_line.value == source
  let redacted = report_model.redact_report(value)
  assert redacted.kernel.command_line.state == report_model.Redacted
  assert redacted.kernel.command_line.value == null
  assert redacted.kernel.command_line.raw_bytes_base64 == null
}

test test_system_report_source_text_preserves_exact_whitespace_when_requested {
  let root = fs.tempdir()?
  defer root.close()?
  root.write(
    p"cmdline",
    "  root=private  quiet  " + "\n",
  )
  let collectors = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCollectors)?
  let normalized = collectors.read_source_text(root, p"cmdline")
  let exact = collectors.read_source_text(root, p"cmdline", 65536, true)
  assert normalized.observation.value == "root=private  quiet"
  assert exact.observation.value == "  root=private  quiet  " + "\n"
}

test test_system_report_kernel_parameter_allowlist_keeps_values_and_absence {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/sys/kernel", parents: true)
  root.mkdir(p"sys/module/usbcore/parameters", parents: true)
  root.write(
    p"proc/cmdline",
    """quiet
""",
  )
  root.write(p"proc/modules", "")
  root.write(
    p"proc/sys/kernel/pid_max",
    """4194304
""",
  )
  root.write(
    p"sys/module/usbcore/parameters/autosuspend",
    """2
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "kernel", true)?
  assert value.kernel.sysctls.len() == 6
  assert value.kernel.parameters.len() == 3
  assert value.kernel.sysctls[0].name == "kernel.pid_max"
  assert value.kernel.sysctls[0].value.value == "4194304"
  assert value.kernel.sysctls[1].value.state == report_model.Absent
  assert value.kernel.parameters[0].name == "usbcore.autosuspend"
  assert value.kernel.parameters[0].value.value == "2"
  assert value.kernel.parameters[1].value.state == report_model.Absent
}

test test_system_report_collects_swap_limit_without_memory_limit_files {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"proc/self", parents: true)
  root.mkdir(p"sys/fs/cgroup/group", parents: true)
  root.write(
    p"proc/meminfo",
    """MemTotal: 16 kB
""",
  )
  root.write(
    p"proc/swaps",
    """Filename Type Size Used Priority
""",
  )
  root.write(
    p"proc/self/cgroup",
    """0::/group
""",
  )
  root.write(
    p"proc/self/mountinfo",
    """31 20 0:25 / /sys/fs/cgroup rw,nosuid,nodev - cgroup2 cgroup rw
""",
  )
  root.write(
    p"sys/fs/cgroup/group/memory.swap.max",
    """262144
""",
  )
  root.write(
    p"sys/fs/cgroup/group/memory.swap.current",
    """65536
""",
  )
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let value = collector.collect_from_root(root, "fixture-arch", 4096, 100, "memory", true)?
  let swap_limit = value.memory.cgroup
    |> where .resource == "memory.swap.max"
    |> first()?
  assert swap_limit.maximum_value == 262144
  assert swap_limit.current_value == 65536
}

test test_system_report_smbios_parser_preserves_records_and_reports_bad_string_indexes {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let table = b"\x01\x084\x12\x01\x02\x03\0Vendor\0Model\0Version\0\0\x7f\x04\0\0\0\0"
  let parsed = collector.parse_smbios_table(table)?
  assert parsed.truncated == false
  assert parsed.issues == []
  assert parsed.records.len() == 2
  assert parsed.records[0].record_type == 1
  assert parsed.records[0].handle == 4660
  assert parsed.records[0].strings.len() == 3
  assert parsed.records[0].strings[1].value == "Model"
  assert parsed.records[1].record_type == 127

  let bad_index = b"\x01\x084\x12\x04\x02\x03\0Vendor\0Model\0Version\0\0\x7f\x04\0\0\0\0"
  let partial = collector.parse_smbios_table(bad_index)?
  assert partial.records.len() == 2
  assert ! partial.issues.is_empty()

  let invalid_length = collector.parse_smbios_table(b"\x01\x03\0\0")?
  assert invalid_length.records.is_empty()
  assert invalid_length.truncated == false
  assert invalid_length.issues.len() == 1

  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"sys/firmware/dmi/tables", parents: true)
  root.write(p"sys/firmware/dmi/tables/DMI", bad_index)
  let collected = collector.collect_from_root(root, "fixture-arch", 4096, 100, "firmware", true)?
  assert collected.firmware.status.state == report_model.Partial
  let parser_issue = collected.issues
    |> where .field == "smbios.issue.0"
    |> first()?
  assert parser_issue.detail.value != null
}

test test_system_report_smbios_unknown_type_keeps_record_identity {
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let table = b"\x90\x06E#\xaa\xbb\0\0\x7f\x04\0\0\0\0"
  let parsed = collector.parse_smbios_table(table)?
  assert parsed.issues == []
  assert parsed.records.len() == 2
  assert parsed.records[0].record_type == 144
  assert parsed.records[0].handle == 9029
  assert parsed.records[0].formatted_length == 6
  assert parsed.records[0].fields.is_empty()
}

test test_system_report_smbios_type16_reads_device_count_from_short_form {
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
  assert parsed.issues == []
  assert parsed.records.len() == 2
  assert parsed.records[0].formatted_length == 15
  let count = parsed.records[0].fields
    |> where .name == "number_of_devices"
    |> first()?
  assert count.value == 2
  assert count.unit == "count"
}

test test_system_report_smbios_sentinel_size_requires_complete_formatted_field {
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
  assert short_record.issues == []
  assert short_record.records.len() == 2
  assert short_record.records[0].fields
    |> where .name == "size_raw"
    |> first()?.value == 32767
  assert (short_record.records[0].fields |> where .name == "extended_size_raw").is_empty()

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
  assert complete_record.issues == []
  assert complete_record.records[0].fields
    |> where .name == "extended_size_raw"
    |> first()?.value == 65536
}
