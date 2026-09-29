use core.lib.system_report as report_model

type UnifiedCgroupPath = {state: report_model.ObservationState, path: Str?, has_v1: Bool}
type CgroupMount = {root: Str, point: Str}
type SourceRead = {observation: report_model.TextObservation, errno: Int?, error_kind: Str?}
type SystemReportCgroupParser = module {
  export proc read_source_text(root: FsRoot, source_path: Path, max_bytes: Int, preserve_whitespace: Bool) [fs, error] -> SourceRead
  export pure parse_unified_cgroup_path(value: Str) -> UnifiedCgroupPath
  export pure select_cgroup_mount(group_path: Str, mounts: List[CgroupMount]) -> Result[CgroupMount?]
  export pure parse_cpufreq_members(value: Str) -> Result[List[Int]]
  export pure parse_cache_shared_cpus(value: Str) -> Result[List[Int]]
  export pure parse_idle_state_index(name: Str) -> Result[Int]
  export pure parse_pci_decimal_value(value: Str) -> Result[Int]
}

proc test_system_report_bounded_text_reader_withholds_truncated_prefix() [fs, error] {
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  var padding = " "
  while padding.count_chars() < 4096 {
    padding = f"${padding}${padding}"
  }
  fs.root_write(root, p"source", f"42\n${padding}")?
  let parser = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCgroupParser)?
  let read = parser.read_source_text(root, p"source", 4096, false)
  test.eq(read.observation.state, report_model.Truncated)?
  test.eq(read.observation.value, null)?
}

proc test_system_report_pci_decimal_attribute_rejects_nondecimal_and_inexact_values() [fs, error] {
  let parser = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCgroupParser)?
  test.eq(parser.parse_pci_decimal_value("8")?, 8)?
  test.eq(parser.parse_pci_decimal_value("9007199254740991")?, 9007199254740991)?
  for invalid in ["", "-0", "-1", "+8", "0x8", "1_0", "9007199254740992"] {
    test.error_kind(parser.parse_pci_decimal_value(invalid), "SystemReportSourceError.InvalidPciId")?
  }
}

proc test_system_report_idle_state_index_requires_canonical_json_safe_directory_name() [fs, error] {
  let parser = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCgroupParser)?
  test.eq(parser.parse_idle_state_index("state0")?, 0)?
  test.eq(parser.parse_idle_state_index("state42")?, 42)?
  for invalid in ["state", "state00", "state-1", "state+1", "state0x10", "state9007199254740992"] {
    test.error_kind(parser.parse_idle_state_index(invalid), "SystemReportSourceError.InvalidIdleStateIndex")?
  }
}

proc test_system_report_cpufreq_members_accept_kernel_space_separated_ids() [fs, error] {
  let parser = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCgroupParser)?
  test.eq(parser.parse_cpufreq_members("0 2 7")?, [0, 2, 7])?
  test.eq(parser.parse_cpufreq_members("4")?, [4])?
  test.error_kind(parser.parse_cpufreq_members("0 0"), "SystemReportSourceError.InvalidCpuFreqMembers")?
  test.error_kind(parser.parse_cpufreq_members("0 x"), "SystemReportSourceError.InvalidCpuFreqMembers")?
  test.error_kind(parser.parse_cpufreq_members("0-0"), "SystemReportSourceError.InvalidCpuFreqMembers")?
}

proc test_system_report_cache_shared_cpu_list_rejects_ambiguous_membership() [fs, error] {
  let parser = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCgroupParser)?
  test.eq(parser.parse_cache_shared_cpus("0,2-3")?, [0, 2, 3])?
  test.error_kind(parser.parse_cache_shared_cpus("0,,2"), "SystemReportSourceError.InvalidCacheSharing")?
  test.error_kind(parser.parse_cache_shared_cpus(""), "SystemReportSourceError.InvalidCacheSharing")?
}

proc test_system_report_unified_cgroup_path_preserves_name_and_rejects_ambiguous_rows() [fs, error] {
  let parser = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCgroupParser)?
  let observed = parser.parse_unified_cgroup_path("0::/team:blue\n")
  test.eq(observed.state, report_model.Observed)?
  test.eq(observed.path, "/team:blue")?
  test.eq(observed.has_v1, false)?
  let hybrid = parser.parse_unified_cgroup_path("2:cpu:/legacy\n0::/team:blue\n")
  test.eq(hybrid.path, "/team:blue")?
  test.eq(hybrid.has_v1, true)?
  for malformed in ["", "0::relative\n", "0::/team:blue\n0::/other\n", "0:cpu:/group\n", "x::/group\n", "9007199254740992::/group\n", "999999999999999999999999999999::/group\n"] {
    let parsed = parser.parse_unified_cgroup_path(malformed)
    test.eq(parsed.state, report_model.Malformed)?
    test.eq(parsed.path, null)?
  }
  let v1 = parser.parse_unified_cgroup_path("2:cpu:/legacy\n")
  test.eq(v1.state, report_model.Unsupported)?
  test.eq(v1.path, null)?
  test.eq(v1.has_v1, true)?
}

proc test_system_report_cgroup_mount_selection_uses_longest_visible_root() [fs, error] {
  let parser = module.load(p"core/lib/system_report_collect.xsh")?.require(SystemReportCgroupParser)?
  let mounts: List[CgroupMount] = [
    {root: "/other", point: "/sys/fs/cgroup/other"},
    {root: "/", point: "/sys/fs/cgroup"},
    {root: "/tenant", point: "/sys/fs/cgroup/tenant"},
  ]
  test.eq(parser.select_cgroup_mount("/tenant/job", mounts)?, mounts[2])?
  test.eq(parser.select_cgroup_mount("/tenantx", mounts)?, mounts[1])?
  test.eq(parser.select_cgroup_mount("/other/job", mounts)?, mounts[0])?
  test.eq(parser.select_cgroup_mount("/missing", [mounts[0]])?, null)?
  test.error_kind(parser.select_cgroup_mount("relative", mounts), "SystemReportSourceError.InvalidCgroupMount")?
  test.error_kind(parser.select_cgroup_mount("/tenant/job", [{root: "tenant", point: "/sys/fs/cgroup"}]), "SystemReportSourceError.InvalidCgroupMount")?
  test.error_kind(parser.select_cgroup_mount("/tenant/job", [{root: "/tenant", point: "relative"}]), "SystemReportSourceError.InvalidCgroupMount")?
}
