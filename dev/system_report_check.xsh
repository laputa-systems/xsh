##! Fixed-denominator coverage manifest validation and report generation.
use context

error SystemReportCheckError = Invalid(message: Str)

pure check_failure(message: Str) -> SystemReportCheckError {
  return SystemReportCheckError.Invalid(message: message)
}

# The manifest keeps every comparison case independent of candidate output.
type CoverageAssertion = {
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

type FixtureCase = {scenario: Str, tests: List[Str]}
type FixtureRun = {passed: Int, failed: Int}

type CoverageManifest = {
  schema_version: Int,
  producer: Str,
  assertions: List[CoverageAssertion],
  fixture_cases: List[FixtureCase],
  macos_fixture_cases: List[FixtureCase],
  fixture_scenarios: List[Str],
}

type LscpuExtendedCpu = {cpu: Int, online: Bool}
type LscpuExtended = {cpus: List[LscpuExtendedCpu]}
## Records exact CPU identity-set agreement and both directions of missing identities.
export type CpuIdSetComparison = {
  reference_count: Int,
  candidate_count: Int,
  matched_count: Int,
  missing_ids: List[Int],
  unexpected_ids: List[Int],
  exact: Bool,
}

## Holds four independently parsed kernel CPU identity sets.
export type CpuSetReference = {
  possible: List[Int],
  present: List[Int],
  online: List[Int],
  offline: List[Int],
}

## Scores every identity set in the CPU-set assertion separately.
export type CpuSetComparison = {
  possible: CpuIdSetComparison,
  present: CpuIdSetComparison,
  online: CpuIdSetComparison,
  offline: CpuIdSetComparison,
  exact: Bool,
}

## Holds one independently reported swap area with byte-valued counters.
export type SwapReferenceDevice = {name: Str, kind: Str, size_bytes: Int, used_bytes: Int, priority: Int}

## Preserves each procfs memory field with its exact normalized unit.
export type MeminfoReferenceCounter = {name: Str, value: Int, unit: Str}

## Scores counters and their host-memory projection only where raw gauges are stable.
export type MeminfoReferenceComparison = {
  reference_count: Int,
  candidate_count: Int,
  matched_count: Int,
  stable_count: Int,
  changed_count: Int,
  missing_names: List[Str],
  unexpected_names: List[Str],
  mismatched_names: List[Str],
  scalar_mismatches: List[Str],
  exact_scored: Bool,
}

## Holds one raw transparent huge-page policy from a named kernel control.
export type ThpReferencePolicy = {name: Str, value: Str}

## Scores the complete available transparent huge-page policy set.
export type ThpReferenceComparison = {
  reference_count: Int,
  candidate_count: Int,
  matched_count: Int,
  missing_names: List[Str],
  unexpected_names: List[Str],
  mismatched_names: List[Str],
  exact: Bool,
}

## Holds one raw kernel vulnerability description without interpreting its verdict.
export type VulnerabilityReference = {name: Str, description: Str}

## Scores the complete stable set of named kernel vulnerability descriptions.
export type VulnerabilityReferenceComparison = {
  reference_count: Int,
  candidate_count: Int,
  matched_count: Int,
  missing_names: List[Str],
  unexpected_names: List[Str],
  mismatched_names: List[Str],
  exact: Bool,
}

## Scores swap identities and values separately from the presence of the candidate field.
export type SwapReferenceComparison = {
  reference_count: Int,
  candidate_count: Int,
  matched_count: Int,
  candidate_field_missing: Bool,
  missing_names: List[Str],
  unexpected_names: List[Str],
  field_mismatches: List[Str],
  kind_mismatches: Int,
  size_mismatches: Int,
  used_mismatches: Int,
  priority_mismatches: Int,
  exact: Bool,
}

type CandidateSwapName = {state: Str, value: Str?}
type CandidateSwapDevice = {name: CandidateSwapName, kind: Str, size_bytes: Int?, used_bytes: Int?, priority: Int?}
type CandidateIssueField = {section: Str, field: Str}

## Holds numeric PCI identity and only the optional fields emitted by lspci's verbose machine format.
export type PciReference = {
  address: Str, domain: Int, bus: Int, device: Int, function: Int,
  vendor_id: Int, device_id: Int, class_code: Int, prog_if: Int?, revision: Int?,
  subsystem_vendor_id: Int?, subsystem_device_id: Int?, driver: Str?,
  numa_node: Int?, iommu_group: Str?,
}
type CandidatePciFunction = {
  address: Str?, domain: Int?, bus: Int?, device: Int?, function: Int?,
  vendor_id: Int?, device_id: Int?, class_code: Int?, revision: Int?,
  subsystem_vendor_id: Int?, subsystem_device_id: Int?, driver: Str?,
  numa_node: Int?, iommu_group: Str?,
}
type CandidatePciStatus = {state: Str, enumeration_succeeded: Bool}
type CandidatePciSection = {status: CandidatePciStatus, functions: List[CandidatePciFunction]}

## Counts PCI functions by BDF and compares the numeric fields available from the reference.
export type PciReferenceComparison = {
  reference_count: Int, candidate_count: Int, matched_count: Int,
  missing_addresses: List[Str], unexpected_addresses: List[Str],
  field_mismatches: List[Str], candidate_field_missing: Bool, exact_static: Bool,
}

## Keeps the static link facts independently emitted by iproute2 JSON.
export type IpLinkReference = {ifindex: Int, name: Str, mtu: Int, admin_up: Bool, operstate: Str?, kind: Str?, master_name: Str?, lower_name: Str?, lower_index: Int?}
type CandidateNetworkTextObservation = {state: Str, value: Str?}
type CandidateIpLink = {ifindex: Int, name: CandidateNetworkTextObservation, mtu: Int?, admin_up: Bool?, operational_state: Str?, kind: Str?, master_ifindex: Int?, lower_ifindex: Int?}

## Counts static link identity and field agreement without treating dynamic counters as stable.
export type IpLinkComparison = {
  reference_count: Int,
  candidate_count: Int,
  matched_count: Int,
  missing_ids: List[Int],
  unexpected_ids: List[Int],
  field_mismatches: List[Str],
  candidate_field_missing: Bool,
  exact: Bool,
}

## Keeps one independently reported address attached to its interface index.
export type IpAddressReference = {
  ifindex: Int,
  family: Str,
  address: Str,
  prefix_length: Int,
  scope: Str,
  broadcast: Str?,
  valid_lifetime_seconds: Int?,
  preferred_lifetime_seconds: Int?,
}
type CandidateIpAddress = {family: Str, address: CandidateNetworkTextObservation, prefix_length: Int, scope: Str?, broadcast: CandidateNetworkTextObservation}
type CandidateAddressLink = {ifindex: Int, addresses: List[CandidateIpAddress]}

## Scores static address identity and fields while retaining lifetime observations separately.
export type IpAddressComparison = {
  reference_count: Int,
  candidate_count: Int,
  matched_count: Int,
  missing_keys: List[Str],
  unexpected_keys: List[Str],
  field_mismatches: List[Str],
  candidate_field_missing: Bool,
  exact_static: Bool,
}

## Keeps static policy selectors from one explicitly chosen iproute2 address family.
export type IpRuleReference = {
  family: Str,
  priority: Int,
  source: Str?,
  source_prefix_length: Int,
  destination: Str?,
  destination_prefix_length: Int,
  fwmark: Int?,
  fwmask: Int?,
  table: Int,
  action: Str,
  input_name: Str?,
  output_name: Str?,
}
type CandidateIpRule = {
  family: Str, priority: Int?, source: CandidateNetworkTextObservation, source_prefix_length: Int,
  destination: CandidateNetworkTextObservation, destination_prefix_length: Int,
  fwmark: Int?, fwmask: Int?, table: Int?, action: Str,
  input_ifindex: Int?, output_ifindex: Int?,
}
type CandidateRuleLink = {ifindex: Int, name: CandidateNetworkTextObservation}

## Scores static selectors; other iproute2 selectors remain outside this partial comparison.
export type IpRuleComparison = {
  reference_count: Int,
  candidate_count: Int,
  matched_count: Int,
  missing_keys: List[Str],
  unexpected_keys: List[Str],
  candidate_field_missing: Bool,
  exact_static: Bool,
}

## Keeps independently reported static route identity from one address family.
export type IpRouteReference = {
  family: Str, destination: Str, prefix_length: Int, table: Int,
  source: Str?, source_prefix_length: Int, preferred_source: Str?,
  metric: Int?, route_type: Str, scope: Str, protocol: Str,
  gateway: Str?, output_name: Str?, flags: Int, nexthops: List[IpRouteNexthopReference],
}
## Identifies one multipath gateway by output link, effective weight, and kernel flags.
export type IpRouteNexthopReference = {output_name: Str, gateway: Str?, weight: Int, flags: Int}
type CandidateIpRouteNexthop = {ifindex: Int, hops: Int, flags: Int, gateway: CandidateNetworkTextObservation}
type CandidateIpRoute = {
  family: Str, destination: CandidateNetworkTextObservation, prefix_length: Int,
  source: CandidateNetworkTextObservation, source_prefix_length: Int,
  preferred_source: CandidateNetworkTextObservation,
  table: Int, metric: Int?, route_type: Str, scope: Str?, protocol: Str?,
  gateway: CandidateNetworkTextObservation, output_ifindex: Int?, flags: Int, nexthops: List[CandidateIpRouteNexthop],
}

## Scores route identity and known flags while leaving extensions unscored.
export type IpRouteComparison = {
  reference_count: Int, candidate_count: Int, matched_count: Int,
  missing_keys: List[Str], unexpected_keys: List[Str],
  candidate_field_missing: Bool, exact_static: Bool,
}

## Keeps explicit-column lsblk device facts separate from candidate block-device records.
export type BlockReferenceDevice = {
  name: Str,
  major: Int,
  minor: Int,
  kind: Str,
  size_bytes: Int,
  logical_sector_bytes: Int,
  physical_sector_bytes: Int,
  removable: Bool,
  rotational: Bool,
  read_only: Bool,
}

## Identifies a partition parent or a block-layer dependency in an lsblk tree.
export type BlockReferenceEdge = {parent_name: Str, child_name: Str, partition: Bool}
## Holds unique kernel block identities and all observed tree edges.
export type BlockReference = {devices: List[BlockReferenceDevice], edges: List[BlockReferenceEdge]}

## Scores block identities, scalar values, and independently reported tree edges.
export type BlockReferenceComparison = {
  reference_count: Int,
  candidate_count: Int,
  matched_count: Int,
  matched_edges: Int,
  missing_edges: Int,
  unexpected_edges: Int,
  missing_names: List[Str],
  unexpected_names: List[Str],
  field_mismatches: List[Str],
  major_minor_mismatches: Int,
  size_mismatches: Int,
  sector_mismatches: Int,
  flag_mismatches: Int,
  partition_mismatches: Int,
  candidate_field_missing: Bool,
  exact: Bool,
}

## Holds the queue and model fields actually available in explicit-column lsblk JSON.
export type BlockQueueReference = {
  name: Str,
  scheduler: Str?,
  read_ahead_kb: Int?,
  discard_granularity_bytes: Int?,
  discard_max_bytes: Int?,
  model: Str?,
  revision_hint: Str?,
}

## Scores the lsblk-visible queue fields without treating REV as firmware revision.
export type BlockQueueFieldComparison = {
  reference_count: Int,
  candidate_count: Int,
  matched_count: Int,
  missing_names: List[Str],
  unexpected_names: List[Str],
  field_mismatches: List[Str],
  scheduler_mismatches: Int,
  read_ahead_mismatches: Int,
  discard_mismatches: Int,
  model_mismatches: Int,
  candidate_field_missing: Bool,
  exact: Bool,
}

type CandidateBlockQueueDevice = {
  name: Str?,
  active_scheduler: Str?,
  read_ahead_kb: Int?,
  discard_granularity_bytes: Int?,
  discard_max_bytes: Int?,
  model: CandidateTextObservation,
}

## Holds one bounded raw sysfs counter with its documented unit.
export type BlockQueueCounter = {name: Str, value: Int, unit: Str}
## Holds the firmware revision and counter set read independently for one block device.
export type BlockQueueSources = {name: Str, firmware: Str?, counters: List[BlockQueueCounter]}
## Scores firmware and I/O counters against bracketed raw sysfs observations.
export type BlockQueueSourceComparison = {
  reference_count: Int,
  candidate_count: Int,
  matched_count: Int,
  missing_names: List[Str],
  unexpected_names: List[Str],
  firmware_mismatches: Int,
  counter_mismatches: Int,
  unstable: Bool,
  candidate_field_missing: Bool,
  exact: Bool,
}

type CandidateQueueCounter = {name: Str, value: Int, unit: Str}
type CandidateQueueSourceDevice = {
  name: Str?,
  firmware: CandidateTextObservation,
  io_counters: List[CandidateQueueCounter],
}

## Holds one flat kernel mount row from findmnt without collapsing repeated targets.
export type MountReference = {
  mount_id: Int,
  parent_id: Int,
  major: Int,
  minor: Int,
  root: Str,
  target: Str,
  filesystem: Str,
  source: Str,
  mount_options: List[Str],
  super_options: List[Str],
  propagation: Str,
}

## Scores mount identities, parent relationships, values, options, and redaction.
export type MountComparison = {
  reference_count: Int,
  candidate_count: Int,
  matched_count: Int,
  missing_ids: List[Int],
  unexpected_ids: List[Int],
  field_mismatches: List[Int],
  parent_mismatches: Int,
  identity_mismatches: Int,
  path_mismatches: Int,
  filesystem_mismatches: Int,
  source_mismatches: Int,
  option_mismatches: Int,
  propagation_mismatches: Int,
  candidate_field_missing: Bool,
  exact: Bool,
}

## Holds byte-valued capacity for one mount ID selected before invoking findmnt.
export type MountUsageReference = {mount_id: Int, total_bytes: Int?, used_bytes: Int?, available_bytes: Int?}

## Scores every mount's capacity state, including mounts that must remain unqueried.
export type MountUsageComparison = {
  eligible_count: Int,
  skipped_count: Int,
  matched_count: Int,
  mismatched_ids: List[Int],
  unstable: Bool,
  exact: Bool,
}

type CandidateMountUsage = {
  mount_id: Int,
  usage_state: Str,
  usage_total_bytes: Int?,
  usage_used_bytes: Int?,
  usage_available_bytes: Int?,
}

type CandidateTextObservation = {state: Str, value: Str?}
type CandidateRawTextObservation = {state: Str, value: Str?, raw_bytes_base64: Str?}
type CandidateVulnerability = {name: Str, description: CandidateRawTextObservation}

## Checks source fidelity in sensitive output and complete redaction in default output.
export type KernelCommandLineComparison = {
  sensitive_exact: Bool,
  redacted_exact: Bool,
  candidate_field_missing: Bool,
  exact: Bool,
}

## Holds one curated kernel value or an explicit source observation state.
export type KernelParameterReference = {
  name: Str,
  source: Str,
  state: Str,
  value: Str?,
  raw_bytes_base64: Str?,
}

## Scores the fixed sysctl and module-parameter inventory by source and name.
export type KernelParameterComparison = {
  reference_count: Int,
  candidate_count: Int,
  matched_count: Int,
  missing_names: List[Str],
  unexpected_names: List[Str],
  field_mismatches: List[Str],
  candidate_field_missing: Bool,
  exact: Bool,
}

type CandidateKernelParameter = {name: Str, value: CandidateRawTextObservation}
type CandidateNamedKernelParameter = {source: Str, name: Str, value: CandidateRawTextObservation}
type CandidateMount = {
  mount_id: Int,
  parent_id: Int,
  major: Int?,
  minor: Int?,
  root: CandidateTextObservation,
  target: CandidateTextObservation,
  filesystem: Str,
  source: CandidateTextObservation,
  mount_options: List[Str],
  super_options: List[Str],
  optional_fields: List[Str],
}

type PendingLsblkNode = {node: Record, parent_name: Str?}
type CandidateBlockDevice = {
  name: Str?,
  major: Int?,
  minor: Int?,
  kind: Str,
  size_bytes: Int?,
  logical_sector_bytes: Int?,
  physical_sector_bytes: Int?,
  removable: Bool?,
  rotational: Bool?,
  read_only: Bool?,
  parent_device_index: Int?,
  holder_indices: List[Int],
  slave_indices: List[Int],
}

## Holds module facts from lsmod together with the state exposed only by procfs.
export type KernelModuleReference = {name: Str, size_bytes: Int, users: Int, state: Str}

## Scores kernel module identities and every declared module field separately.
export type KernelModuleComparison = {
  reference_count: Int,
  candidate_count: Int,
  matched_count: Int,
  missing_names: List[Str],
  unexpected_names: List[Str],
  field_mismatches: List[Str],
  size_mismatches: Int,
  users_mismatches: Int,
  state_mismatches: Int,
  candidate_field_missing: Bool,
  exact: Bool,
}

type CandidateKernelModule = {name: Str, size_bytes: Int, users: Int, state: Str}

## Scores kernel release and architecture independently, including missing candidate fields.
export type IdentityComparison = {
  candidate_release: Str?,
  candidate_architecture: Str?,
  release_missing: Bool,
  architecture_missing: Bool,
  release_exact: Bool,
  architecture_exact: Bool,
  exact: Bool,
}

## Scores one whole-second uptime observation against two source observations.
export type UptimeComparison = {
  candidate_seconds: Int?,
  before_seconds: Int,
  after_seconds: Int,
  candidate_missing: Bool,
  bracketed: Bool,
}

## Identifies one process-visible namespace symlink target from an independent readlink.
export type NamespaceReference = {field: Str, target: Str}

## Scores every requested namespace identity without treating redaction as agreement.
export type NamespaceComparison = {
  reference_count: Int,
  matched_count: Int,
  missing_fields: List[Str],
  mismatched_fields: List[Str],
  exact: Bool,
}

## Holds the release identity read independently from an os-release source file.
export type OsReleaseReference = {id: Str, version_id: Str?}

## Scores required OS identity and optional version without filling candidate gaps.
export type OsReleaseComparison = {
  id_missing: Bool,
  version_id_missing: Bool,
  id_exact: Bool,
  version_id_exact: Bool,
  exact: Bool,
}

## Holds exact device-tree strings decoded independently from bounded od bytes.
export type DeviceTreeReference = {model: Str?, compatible: List[Str]}

## Scores model, ordered compatible values, and the candidate's source identity.
export type DeviceTreeComparison = {
  source_exact: Bool,
  model_exact: Bool,
  compatible_exact: Bool,
  exact: Bool,
}

## Holds the mandatory procfs identity fields without requiring resource counters to fit JSON.
export type ProcStatIdentityReference = {
  pid: Int,
  parent_pid: Int,
  command: Str,
  state: Str,
  start_ticks: Int,
}

## Keeps the thread count independently of unrelated stat memory counters.
export type ProcStatThreadReference = {pid: Int, start_ticks: Int, thread_count: Int}

type ProcStatReferenceFields = {identity: ProcStatIdentityReference, fields: List[Str]}

## Gives procfs page counts in exact byte units without floating point conversion.
export type ProcStatmReference = {virtual_bytes: Int, resident_bytes: Int}

## Identifies one process across a capture bracket using PID and start ticks.
export type ProcessIdentityReference = {
  pid: Int,
  start_ticks: Int,
  parent_pid: Int,
  uid: Int,
  command: Str,
  state: Str,
}

## Scores static fields for identities present in both raw reference snapshots.
export type ProcessIdentityComparison = {
  stable_count: Int,
  unstable_count: Int,
  candidate_count: Int,
  matched_count: Int,
  missing_pids: List[Int],
  mismatched_pids: List[Int],
  state_unscored_count: Int,
  exact_static: Bool,
}


## Keeps skipped process sources visible instead of reducing the reference silently.
export type ProcessIdentitySnapshot = {processes: List[ProcessIdentityReference], skipped_count: Int}

## Holds byte-valued per-process resources from one bounded procfs read.
export type ProcessResourceReference = {
  pid: Int,
  start_ticks: Int,
  thread_count: Int,
  resident_bytes: Int,
  virtual_bytes: Int,
  cgroup: Str?,
}

## Keeps the processes omitted by resource source reads in the reference result.
export type ProcessResourceSnapshot = {processes: List[ProcessResourceReference], skipped_count: Int}

## Keeps changing gauges out of the scored field denominator.
export type ProcessResourceComparison = {
  stable_count: Int,
  unstable_count: Int,
  candidate_count: Int,
  matched_count: Int,
  scored_fields: Int,
  missing_pids: List[Int],
  unscored_fields: List[Str],
  mismatched_fields: List[Str],
  exact_scored: Bool,
}

type CandidateProcessIdentity = {
  pid: Int,
  parent_pid: Int,
  uid: Int?,
  command: CandidateTextObservation,
  state: Str,
  start_ticks: Int?,
}

type CandidateProcessResource = {
  pid: Int,
  start_ticks: Int?,
  thread_count: Int?,
  resident_bytes: Int?,
  virtual_bytes: Int?,
  cgroup: CandidateTextObservation,
}

type CheckOptions = {
  manifest: Str,
  no_subprocess: Bool,
  run_fixtures: Bool,
  run_macos_fixtures: Bool,
  compare_cpu: Bool,
  compare_vulnerabilities: Bool,
  compare_meminfo: Bool,
  compare_thp: Bool,
  compare_swaps: Bool,
  compare_pci: Bool,
  compare_network_links: Bool,
  compare_network_addresses: Bool,
  compare_network_rules: Bool,
  compare_network_routes: Bool,
  compare_storage: Bool,
  compare_queue: Bool,
  compare_mounts: Bool,
  compare_mount_usage: Bool,
  compare_modules: Bool,
  compare_command_line: Bool,
  compare_parameters: Bool,
  compare_identity: Bool,
  compare_namespaces: Bool,
  compare_processes: Bool,
  capture_cpu_bundle: Str,
  replay_cpu_bundle: Str,
  capture_memory_bundle: Str,
  replay_memory_bundle: Str,
  xsh_bin: Str,
  xsht_bin: Str,
  cargo_bin: Str,
  script: Str,
}

type UnameObservation = {value: Str, started: Int, ended: Int}
type UptimeReferenceObservation = {seconds: Int, started: Int, ended: Int}
type DeviceTreeRawObservation = {data: Bytes?, state: Str, started: Int, ended: Int}
type NamespaceLinkObservation = {target: Str, started: Int, ended: Int}
type KernelModuleObservation = {modules: List[KernelModuleReference], started: Int, ended: Int}
type ThpObservation = {policies: List[ThpReferencePolicy], started: Int, ended: Int}
type VulnerabilityObservation = {descriptions: List[VulnerabilityReference], started: Int, ended: Int}
type KernelCommandLineObservation = {data: Bytes, started: Int, ended: Int}
type KernelParameterSource = {name: Str, source: Str, path: Path}
type KernelParameterObservation = {values: List[KernelParameterReference], started: Int, ended: Int}
type MeminfoObservation = {counters: List[MeminfoReferenceCounter], started: Int, ended: Int}

type CpuSetSourceObservation = {
  name: Str,
  state: Str,
  truncated: Bool,
  errno: Int?,
  error_kind: Str?,
  byte_count: Int,
  sha256_hex: Str?,
}

type CpuSetCapture = {
  schema_version: Int,
  origin: Str,
  captured_unix_ms: Int,
  reference_adapter: Str,
  stable: Bool,
  sources: List[CpuSetSourceObservation],
  reference: CpuSetReference?,
}

type MemorySourceObservation = {
  path: Str,
  state: Str,
  truncated: Bool,
  errno: Int?,
  error_kind: Str?,
  byte_count: Int,
  sha256_hex: Str?,
}

## Holds independently parsed memory facts from a bounded raw source bundle.
export type MemoryBundleReference = {
  meminfo: List[MeminfoReferenceCounter],
  thp: List[ThpReferencePolicy],
}

## Scores the production memory collector against both saved raw-source oracles.
export type MemoryBundleComparison = {
  meminfo: MeminfoReferenceComparison,
  thp: ThpReferenceComparison,
}

type MemoryCapture = {
  schema_version: Int,
  origin: Str,
  captured_unix_ms: Int,
  reference_adapter: Str,
  stable: Bool,
  sources: List[MemorySourceObservation],
  reference: MemoryBundleReference?,
}

type SystemReportLiveCollector = module {
  export proc collect_from_root(root: FsRoot, architecture: Str, page_size_bytes: Int, clock_ticks_per_second: Int, selected: Str = "", sensitive: Bool = false, include_local_mount_usage: Bool = false) [fs, time, error] -> Result[Record]
}

## Counts required assertions by their declared domain without consulting a report.
export pure summary(assertions: List[CoverageAssertion]) -> Str {
  let groups = assertions |> group-by .domain |> sort-by .key
  var lines = ""
  var mandatory = 0
  var supplemental = 0

  for domain_group in groups {
    var group_mandatory = 0
    var group_supplemental = 0

    for assertion in domain_group.items {
      if assertion.tier == "mandatory" {
        group_mandatory += 1
        mandatory += 1
      } else {
        group_supplemental += 1
        supplemental += 1
      }
    }

    lines = f"${lines}  ${domain_group.key}: ${group_mandatory} mandatory, ${group_supplemental} supplemental\n"
  }

  return f"${lines}total: ${mandatory} mandatory, ${supplemental} supplemental"
}

## Reads CPU identities from util-linux's explicit-column JSON output.
export pure parse_lscpu_online_cpu_ids(output: Str) -> Result[List[Int]] {
  let raw = json.decode(output)?
  let data = raw.require(LscpuExtended)?
  var seen: List[Int] = []
  var online: List[Int] = []
  for cpu_item in data.cpus {
    if cpu_item.cpu < 0 or cpu_item.cpu in seen {
      return Err(check_failure("lscpu JSON has a negative or duplicate CPU ID"))
    }
    seen = seen.push(cpu_item.cpu)
    if cpu_item.online {
      online = online.push(cpu_item.cpu)
    }
  }
  if online.len() == 0 {
    return Err(check_failure("lscpu JSON reported no online CPUs"))
  }
  return online |> sort-by .
}

pure reference_cpu_number(value: Str) -> Result[Int] {
  if value == "" {
    return Err(check_failure("CPU set contains an empty identifier"))
  }
  for character in value.split("") {
    if !"0123456789".contains(character) {
      return Err(check_failure("CPU set contains a non-decimal identifier"))
    }
  }
  match value.parse_int() {
    Ok(number) => return Ok(number)
    Err(_) => return Err(check_failure("CPU set identifier is outside the supported integer range"))
  }
}

## Parses independent sysfs CPU-set references without using the report parser.
export pure parse_reference_cpu_list(output: Str, allow_empty: Bool) -> Result[List[Int]] {
  let source = output.trim()
  if source == "" {
    if allow_empty {
      return Ok([])
    }
    return Err(check_failure("CPU set is empty"))
  }
  var ids: List[Int] = []
  var seen = set.empty()
  for term in source.split(",") {
    let bounds = term.split("-")
    if bounds.len() == 0 or bounds.len() > 2 {
      return Err(check_failure("CPU set contains an invalid range"))
    }
    let first = reference_cpu_number(bounds[0])?
    let last = if bounds.len() == 2 {reference_cpu_number(bounds[1])?} else {first}
    if last < first {
      return Err(check_failure("CPU set range ends before it begins"))
    }
    let width = last - first
    if width >= 65536 or ids.len() > 65535 - width {
      return Err(check_failure("CPU set range is reversed or exceeds 65536 identifiers"))
    }
    var id = first
    while id <= last {
      let key = f"${id}"
      if set.has(seen, key) {
        return Err(check_failure("CPU set contains a duplicate identifier"))
      }
      seen = set.add(seen, key)
      ids = ids.push(id)
      if id == last {
        break
      }
      id += 1
    }
  }
  return ids |> sort-by .
}

proc captured_cpu_set_text(root: FsRoot, name: Str) [fs, error] -> Result[Str] {
  let raw = fs.root_read_result(root, fp"sys/devices/system/cpu/${name}", max_bytes: 65536)?
  if raw.state != "observed" or raw.truncated or raw.data == null {
    return Err(check_failure(f"CPU set capture has no complete ${name} source"))
  }
  match (raw.data ?? b"").utf8() {
    Ok(value) => return Ok(value)
    Err(_) => return Err(check_failure(f"CPU set capture has non-UTF-8 ${name} source"))
  }
}

proc captured_cpu_set_reference(root: FsRoot) [fs, error] -> Result[CpuSetReference] {
  return {
    possible: parse_reference_cpu_list(captured_cpu_set_text(root, "possible")?, false)?,
    present: parse_reference_cpu_list(captured_cpu_set_text(root, "present")?, false)?,
    online: parse_reference_cpu_list(captured_cpu_set_text(root, "online")?, false)?,
    offline: parse_reference_cpu_list(captured_cpu_set_text(root, "offline")?, true)?,
  }
}

## Saves bounded raw CPU-set sources and an independently parsed oracle for collector replay.
export proc capture_cpu_set_bundle(source: FsRoot, bundle: FsRoot, origin: Str) [fs, time, error] -> Result[Unit] {
  if origin != "synthetic_fixture" and origin != "live_capture" {
    return Err(check_failure("CPU set capture origin must be synthetic_fixture or live_capture"))
  }
  if fs.root_exists(bundle, p"capture.json")? {
    return Err(check_failure("CPU set capture already exists"))
  }
  for name in ["possible", "present", "online", "offline"] {
    if fs.root_exists(bundle, fp"sys/devices/system/cpu/${name}")? {
      return Err(check_failure("CPU set capture destination contains a source file"))
    }
  }
  fs.root_mkdir(bundle, p"sys/devices/system/cpu", mode: 0o700, parents: true)?
  var observations: List[CpuSetSourceObservation] = []
  var complete = true
  var saved_bytes: List[Bytes?] = []
  for name in ["possible", "present", "online", "offline"] {
    let relative = fp"sys/devices/system/cpu/${name}"
    let raw = fs.root_read_result(source, relative, max_bytes: 65536)?
    if raw.state != "observed" or raw.truncated or raw.data == null {
      complete = false
    }
    let byte_count = if raw.data == null {0} else {(raw.data ?? b"").len()}
    if raw.data != null {
      fs.root_write(bundle, relative, raw.data ?? b"")?
    }
    saved_bytes = saved_bytes.push(raw.data)
    var sha256_hex: Str? = null
    if raw.data != null {
      sha256_hex = hash.sha256(raw.data ?? b"").hex()
    }
    observations = observations.push({
      name: name, state: raw.state, truncated: raw.truncated,
      errno: raw.errno, error_kind: raw.error_kind, byte_count: byte_count,
      sha256_hex: sha256_hex,
    })
  }
  var stable = true
  let names = ["possible", "present", "online", "offline"]
  for index in range(4) {
    let again = fs.root_read_result(source, fp"sys/devices/system/cpu/${names[index]}", max_bytes: 65536)?
    let first = observations[index]
    if again.state != first.state or again.truncated != first.truncated or
        again.errno != first.errno or again.error_kind != first.error_kind or
        again.data != saved_bytes[index] {
      stable = false
    }
  }
  var reference: CpuSetReference? = null
  if complete and stable {
    match captured_cpu_set_reference(bundle) {
      Ok(parsed) => reference = parsed
      Err(_) => {}
    }
  }
  let capture: CpuSetCapture = {
    schema_version: 2,
    origin: origin,
    captured_unix_ms: time.now(),
    reference_adapter: "xsh-dev-sysfs-cpu-list-v1",
    stable: stable,
    sources: observations,
    reference: reference,
  }
  let wire: Any = capture
  fs.root_write_atomic(bundle, p"capture.json", json.encode(wire, pretty: true)?)?
  return Ok()
}

## Validates captured bytes and independently reparses the saved CPU-set reference.
export proc validate_cpu_set_bundle(bundle: FsRoot) [fs, error] -> Result[CpuSetReference] {
  let metadata = fs.root_read_result(bundle, p"capture.json", max_bytes: 65536)?
  if metadata.state != "observed" or metadata.truncated or metadata.data == null {
    return Err(check_failure("CPU set capture metadata is missing or incomplete"))
  }
  let metadata_text = (metadata.data ?? b"").utf8()?
  let capture = json.decode(metadata_text)?.require(CpuSetCapture)?
  if capture.schema_version != 2 or capture.origin not in ["synthetic_fixture", "live_capture"] or
      capture.reference_adapter != "xsh-dev-sysfs-cpu-list-v1" or capture.sources.len() != 4 {
    return Err(check_failure("CPU set capture metadata has an unsupported contract"))
  }
  if !capture.stable {
    return Err(check_failure("CPU set capture changed while sources were read"))
  }
  let names = ["possible", "present", "online", "offline"]
  for index in range(4) {
    let expected = capture.sources[index]
    let name = names[index]
    if expected.name != name or expected.state != "observed" or expected.truncated {
      return Err(check_failure(f"CPU set capture cannot score ${name} source"))
    }
    let raw = fs.root_read_result(bundle, fp"sys/devices/system/cpu/${name}", max_bytes: 65536)?
    if raw.state != "observed" or raw.truncated or raw.data == null or (raw.data ?? b"").len() != expected.byte_count or
        expected.sha256_hex == null or hash.sha256(raw.data ?? b"").hex() != (expected.sha256_hex ?? "") {
      return Err(check_failure(f"CPU set capture ${name} bytes differ from metadata"))
    }
  }
  if capture.reference == null {
    return Err(check_failure("CPU set capture has no usable reference observation"))
  }
  let reference = captured_cpu_set_reference(bundle)?
  if reference != (capture.reference ?? reference) {
    return Err(check_failure("CPU set capture reference differs from raw sources"))
  }
  return reference
}

## Re-runs the production collector on captured raw files and checks an independent oracle.
export proc replay_cpu_set_bundle(bundle: FsRoot) [fs, time, error] -> Result[CpuSetComparison] {
  let reference = validate_cpu_set_bundle(bundle)?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let candidate = collector.collect_from_root(bundle, "captured-architecture", 4096, 100, "cpu", true)?
  let possible = compare_cpu_id_sets(candidate.cpu.possible, reference.possible)?
  let present = compare_cpu_id_sets(candidate.cpu.present, reference.present)?
  let online = compare_cpu_id_sets(candidate.cpu.online, reference.online)?
  let offline = compare_cpu_id_sets(candidate.cpu.offline, reference.offline)?
  return {
    possible: possible, present: present, online: online, offline: offline,
    exact: possible.exact and present.exact and online.exact and offline.exact,
  }
}

pure memory_bundle_paths() -> List[Str] {
  return [
    "proc/meminfo",
    "sys/kernel/mm/transparent_hugepage/enabled",
    "sys/kernel/mm/transparent_hugepage/defrag",
  ]
}

proc memory_bundle_text(bundle: FsRoot, relative: Str, bound: Int) [fs, error] -> Result[Str] {
  let raw = fs.root_read_result(bundle, fp"${relative}", max_bytes: bound)?
  if raw.state != "observed" or raw.truncated or raw.data == null {
    return Err(check_failure(f"memory bundle has no complete ${relative} source"))
  }
  match (raw.data ?? b"").utf8() {
    Ok(value) => return Ok(value)
    Err(_) => return Err(check_failure(f"memory bundle ${relative} source is not UTF-8"))
  }
}

proc memory_bundle_reference(bundle: FsRoot) [fs, error] -> Result[MemoryBundleReference] {
  let meminfo = parse_meminfo_reference(memory_bundle_text(bundle, "proc/meminfo", 1048576)?)?
  var thp: List[ThpReferencePolicy] = []
  for name in ["enabled", "defrag"] {
    let relative = f"sys/kernel/mm/transparent_hugepage/${name}"
    let raw = fs.root_read_result(bundle, fp"${relative}", max_bytes: 4096)?
    if raw.state == "absent" {
      continue
    }
    thp = thp.push({name: name, value: parse_thp_reference(memory_bundle_text(bundle, relative, 4096)?)?})
  }
  return {meminfo: meminfo, thp: thp}
}

## Captures bounded raw memory sources with exact digests and an independent oracle.
export proc capture_memory_bundle(source: FsRoot, bundle: FsRoot, origin: Str) [fs, time, error] -> Result[Unit] {
  if origin not in ["synthetic_fixture", "live_capture"] {
    return Err(check_failure("memory capture origin must be synthetic_fixture or live_capture"))
  }
  if fs.root_exists(bundle, p"capture.json")? {
    return Err(check_failure("memory capture already exists"))
  }
  let paths = memory_bundle_paths()
  for relative in paths {
    if fs.root_exists(bundle, fp"${relative}")? {
      return Err(check_failure("memory capture destination contains a source file"))
    }
  }
  fs.root_mkdir(bundle, p"proc", mode: 0o700, parents: true)?
  fs.root_mkdir(bundle, p"sys/kernel/mm/transparent_hugepage", mode: 0o700, parents: true)?
  var observations: List[MemorySourceObservation] = []
  var saved_bytes: List[Bytes?] = []
  var complete = true
  for index in range(paths.len()) {
    let relative = paths[index]
    let bound = if index == 0 {1048576} else {4096}
    let raw = fs.root_read_result(source, fp"${relative}", max_bytes: bound)?
    if raw.state != "observed" and (index == 0 or raw.state != "absent") {
      complete = false
    }
    if raw.truncated {
      complete = false
    }
    var sha256_hex: Str? = null
    var byte_count = 0
    if raw.data != null {
      let data = raw.data ?? b""
      fs.root_write(bundle, fp"${relative}", data)?
      sha256_hex = hash.sha256(data).hex()
      byte_count = data.len()
    }
    saved_bytes = saved_bytes.push(raw.data)
    observations = observations.push({
      path: relative, state: raw.state, truncated: raw.truncated,
      errno: raw.errno, error_kind: raw.error_kind,
      byte_count: byte_count, sha256_hex: sha256_hex,
    })
  }
  var stable = true
  for index in range(paths.len()) {
    let bound = if index == 0 {1048576} else {4096}
    let again = fs.root_read_result(source, fp"${paths[index]}", max_bytes: bound)?
    let first = observations[index]
    if again.state != first.state or again.truncated != first.truncated or
        again.errno != first.errno or again.error_kind != first.error_kind or again.data != saved_bytes[index] {
      stable = false
    }
  }
  var reference: MemoryBundleReference? = null
  if complete {
    match memory_bundle_reference(bundle) {
      Ok(parsed) => reference = parsed
      Err(_) => {}
    }
  }
  let capture: MemoryCapture = {
    schema_version: 1, origin: origin, captured_unix_ms: time.now(),
    reference_adapter: "xsh-dev-memory-raw-v1", stable: stable,
    sources: observations, reference: reference,
  }
  let wire: Any = capture
  fs.root_write_atomic(bundle, p"capture.json", json.encode(wire, pretty: true)?)?
  return Ok()
}

## Requires captured bytes and the saved independent memory oracle to agree.
export proc validate_memory_bundle(bundle: FsRoot) [fs, error] -> Result[MemoryBundleReference] {
  let metadata = fs.root_read_result(bundle, p"capture.json", max_bytes: 2097152)?
  if metadata.state != "observed" or metadata.truncated or metadata.data == null {
    return Err(check_failure("memory capture metadata is missing or incomplete"))
  }
  let capture = json.decode((metadata.data ?? b"").utf8()?)?.require(MemoryCapture)?
  let paths = memory_bundle_paths()
  if capture.schema_version != 1 or capture.origin not in ["synthetic_fixture", "live_capture"] or
      capture.reference_adapter != "xsh-dev-memory-raw-v1" or capture.sources.len() != paths.len() {
    return Err(check_failure("memory capture metadata has an unsupported contract"))
  }
  for index in range(paths.len()) {
    let expected = capture.sources[index]
    let relative = paths[index]
    if expected.path != relative or expected.truncated {
      return Err(check_failure("memory capture source identity or completeness differs from metadata"))
    }
    if expected.state == "absent" and index > 0 {
      if expected.byte_count != 0 or expected.sha256_hex != null or fs.root_exists(bundle, fp"${relative}")? {
        return Err(check_failure(f"memory capture ${relative} absent source differs from metadata"))
      }
      continue
    }
    if expected.state != "observed" or expected.sha256_hex == null {
      return Err(check_failure(f"memory capture cannot score ${relative} source"))
    }
    let bound = if index == 0 {1048576} else {4096}
    let raw = fs.root_read_result(bundle, fp"${relative}", max_bytes: bound)?
    if raw.state != "observed" or raw.truncated or raw.data == null or
        (raw.data ?? b"").len() != expected.byte_count or
        hash.sha256(raw.data ?? b"").hex() != (expected.sha256_hex ?? "") {
      return Err(check_failure(f"memory capture ${relative} bytes differ from metadata"))
    }
  }
  if capture.reference == null {
    return Err(check_failure("memory capture has no usable reference observation"))
  }
  let reference = memory_bundle_reference(bundle)?
  if reference != (capture.reference ?? reference) {
    return Err(check_failure("memory capture reference differs from raw sources"))
  }
  return reference
}

## Runs the production memory collector on captured sources and checks both oracles.
export proc replay_memory_bundle(bundle: FsRoot) [fs, time, error] -> Result[MemoryBundleComparison] {
  let reference = validate_memory_bundle(bundle)?
  let collector = module.load(p"core/lib/system_report_live.xsh")?.require(SystemReportLiveCollector)?
  let candidate = collector.collect_from_root(bundle, "captured-architecture", 4096, 100, "memory", true)?
  let wire: Any = candidate
  let candidate_json = json.encode(wire)?
  return {
    meminfo: compare_meminfo(candidate_json, reference.meminfo, reference.meminfo)?,
    thp: compare_thp(candidate_json, reference.thp, reference.thp)?,
  }
}

pure compare_cpu_id_sets(candidate: List[Int], reference: List[Int]) -> Result[CpuIdSetComparison] {
  var reference_seen = set.empty()
  for cpu_id in reference {
    let key = f"${cpu_id}"
    if cpu_id < 0 or set.has(reference_seen, key) {
      return Err(check_failure("reference CPU set has a negative or duplicate ID"))
    }
    reference_seen = set.add(reference_seen, key)
  }
  var candidate_seen = set.empty()
  var matched_count = 0
  var unexpected_ids: List[Int] = []
  for cpu_id in candidate {
    let key = f"${cpu_id}"
    if cpu_id < 0 or set.has(candidate_seen, key) {
      return Err(check_failure("candidate report has a negative or duplicate CPU ID"))
    }
    candidate_seen = set.add(candidate_seen, key)
    if set.has(reference_seen, key) {
      matched_count += 1
    } else {
      unexpected_ids = unexpected_ids.push(cpu_id)
    }
  }
  var missing_ids: List[Int] = []
  for cpu_id in reference {
    if !set.has(candidate_seen, f"${cpu_id}") {
      missing_ids = missing_ids.push(cpu_id)
    }
  }
  return {
    reference_count: reference.len(),
    candidate_count: candidate.len(),
    matched_count: matched_count,
    missing_ids: missing_ids |> sort-by .,
    unexpected_ids: unexpected_ids |> sort-by .,
    exact: missing_ids.len() == 0 and unexpected_ids.len() == 0,
  }
}

## Compares the independently observed online CPU identities as sets.
export pure compare_cpu_online_ids(candidate_json: Str, reference_json: Str) -> Result[CpuIdSetComparison] {
  let reference = parse_lscpu_online_cpu_ids(reference_json)?
  let candidate_data = json.decode(candidate_json)?
  let candidate = json.get(candidate_data, ["cpu", "online"])?.require(List[Int])?
  return compare_cpu_id_sets(candidate, reference)
}

## Compares all four CPU identity sets without consulting candidate status flags.
export pure compare_cpu_sets(candidate_json: Str, reference: CpuSetReference) -> Result[CpuSetComparison] {
  let data = json.decode(candidate_json)?
  let possible = compare_cpu_id_sets(json.get(data, ["cpu", "possible"])?.require(List[Int])?, reference.possible)?
  let present = compare_cpu_id_sets(json.get(data, ["cpu", "present"])?.require(List[Int])?, reference.present)?
  let online = compare_cpu_id_sets(json.get(data, ["cpu", "online"])?.require(List[Int])?, reference.online)?
  let offline = compare_cpu_id_sets(json.get(data, ["cpu", "offline"])?.require(List[Int])?, reference.offline)?
  return {
    possible: possible,
    present: present,
    online: online,
    offline: offline,
    exact: possible.exact and present.exact and online.exact and offline.exact,
  }
}

pure meminfo_reference_number(raw: Str, max_value: Int) -> Result[Int] {
  if raw == "" {
    return Err(check_failure("meminfo reference has an empty number"))
  }
  for character in raw.split("") {
    if !"0123456789".contains(character) {
      return Err(check_failure("meminfo reference has a nondecimal number"))
    }
  }
  var number = 0
  match raw.parse_int() {
    Ok(parsed) => number = parsed
    Err(_) => return Err(check_failure("meminfo reference number cannot be represented"))
  }
  if number > max_value {
    return Err(check_failure("meminfo reference number exceeds its exact JSON range"))
  }
  return Ok(number)
}

pure meminfo_byte_field(name: Str) -> Bool {
  return name in ["MemTotal", "MemFree", "MemAvailable", "Buffers", "Cached", "Active", "Inactive", "Dirty", "Writeback", "SwapTotal", "SwapFree"]
}

## Parses bounded procfs memory rows without borrowing the collector's parser.
export pure parse_meminfo_reference(output: Str) -> Result[List[MeminfoReferenceCounter]] {
  var counters: List[MeminfoReferenceCounter] = []
  var seen = set.empty()
  for line in output.lines() {
    if line.trim() == "" {
      continue
    }
    let pair = line.split(":", maxsplit: 1)
    if pair.len() != 2 {
      return Err(check_failure("meminfo reference has a row without a field separator"))
    }
    let name = pair[0].trim()
    if name == "" or set.has(seen, name) {
      return Err(check_failure("meminfo reference has an empty or duplicate field name"))
    }
    let fields = pair[1].replace("\t", " ").split(" ") |> where .trim() != ""
    if fields.len() == 0 or fields.len() > 2 {
      return Err(check_failure("meminfo reference has an invalid value column count"))
    }
    let source_unit = fields.get(1, "")
    if meminfo_byte_field(name) and source_unit != "kB" {
      return Err(check_failure("meminfo reference byte field has an invalid unit"))
    }
    let kib = source_unit == "kB"
    let source_value = meminfo_reference_number(fields[0], if kib {8796093022207} else {9007199254740991})?
    counters = counters.push({
      name: name, value: if kib {source_value * 1024} else {source_value},
      unit: if kib {"bytes"} else if source_unit == "" {"count"} else {source_unit},
    })
    seen = set.add(seen, name)
  }
  if !set.has(seen, "MemTotal") {
    return Err(check_failure("meminfo reference is missing MemTotal"))
  }
  return Ok(counters |> sort-by .name)
}

pure meminfo_host_field(name: Str) -> Str? {
  match name {
    "MemTotal" => return "total_bytes"
    "MemFree" => return "free_bytes"
    "MemAvailable" => return "available_bytes"
    "Buffers" => return "buffers_bytes"
    "Cached" => return "cached_bytes"
    "Active" => return "active_bytes"
    "Inactive" => return "inactive_bytes"
    "Dirty" => return "dirty_bytes"
    "Writeback" => return "writeback_bytes"
    "SwapTotal" => return "swap_total_bytes"
    "SwapFree" => return "swap_free_bytes"
    _ => return null
  }
}

## Compares all source names while withholding value scores for changed gauges.
export pure compare_meminfo(
  candidate_json: Str, before: List[MeminfoReferenceCounter], after: List[MeminfoReferenceCounter],
) -> Result[MeminfoReferenceComparison] {
  let data = json.decode(candidate_json)?
  let candidates = json.get(data, ["memory", "host", "counters"])?.require(List[MeminfoReferenceCounter])?
  var before_by_name: Map[Int] = {}
  var after_by_name: Map[Int] = {}
  var candidate_by_name: Map[Int] = {}
  for index in range(before.len()) {
    let item = before[index]
    if item.name == "" or item.unit == "" or item.value < 0 or item.value > 9007199254740991 or before_by_name.has(item.name) {
      return Err(check_failure("before meminfo reference contains an invalid or duplicate field"))
    }
    before_by_name = before_by_name.set(item.name, index)
  }
  for index in range(after.len()) {
    let item = after[index]
    if item.name == "" or item.unit == "" or item.value < 0 or item.value > 9007199254740991 or after_by_name.has(item.name) {
      return Err(check_failure("after meminfo reference contains an invalid or duplicate field"))
    }
    after_by_name = after_by_name.set(item.name, index)
  }
  if before.len() != after.len() {
    return Err(check_failure("meminfo reference field set changed around candidate collection"))
  }
  for item in before {
    if !after_by_name.has(item.name) or after[after_by_name.get(item.name, -1)].unit != item.unit {
      return Err(check_failure("meminfo reference field or unit changed around candidate collection"))
    }
  }
  for index in range(candidates.len()) {
    let item = candidates[index]
    if item.name == "" or item.unit == "" or item.value < 0 or item.value > 9007199254740991 or candidate_by_name.has(item.name) {
      return Err(check_failure("candidate meminfo contains an invalid or duplicate counter"))
    }
    candidate_by_name = candidate_by_name.set(item.name, index)
  }
  var matched_count = 0
  var stable_count = 0
  var changed_count = 0
  var missing_names: List[Str] = []
  var unexpected_names: List[Str] = []
  var mismatched_names: List[Str] = []
  var scalar_mismatches: List[Str] = []
  for first in before {
    if !candidate_by_name.has(first.name) {
      missing_names = missing_names.push(first.name)
      continue
    }
    matched_count += 1
    let last = after[after_by_name.get(first.name, -1)]
    let candidate = candidates[candidate_by_name.get(first.name, -1)]
    if candidate.unit != first.unit {
      mismatched_names = mismatched_names.push(first.name)
      continue
    }
    if first.value != last.value {
      changed_count += 1
      continue
    }
    stable_count += 1
    if candidate.value != first.value {
      mismatched_names = mismatched_names.push(first.name)
    }
    let scalar_field = meminfo_host_field(first.name)
    if scalar_field != null {
      let scalar = json.get(data, ["memory", "host", scalar_field], null).require(Int?)?
      if scalar != first.value {
        scalar_mismatches = scalar_mismatches.push(first.name)
      }
    }
  }
  for candidate in candidates {
    if !before_by_name.has(candidate.name) {
      unexpected_names = unexpected_names.push(candidate.name)
    }
  }
  return Ok({
    reference_count: before.len(), candidate_count: candidates.len(), matched_count: matched_count,
    stable_count: stable_count, changed_count: changed_count,
    missing_names: missing_names |> sort-by ., unexpected_names: unexpected_names |> sort-by .,
    mismatched_names: mismatched_names |> sort-by ., scalar_mismatches: scalar_mismatches |> sort-by .,
    exact_scored: stable_count > 0 and missing_names.len() == 0 and unexpected_names.len() == 0 and
      mismatched_names.len() == 0 and scalar_mismatches.len() == 0,
  })
}

## Validates a raw sysfs policy line without assuming a fixed policy vocabulary.
export pure parse_thp_reference(output: Str) -> Result[Str] {
  if output.lines().len() != 1 {
    return Err(check_failure("THP reference must contain one policy line"))
  }
  let value = output.trim()
  let words = value.replace("\t", " ").split(" ") |> where .trim() != ""
  if words.len() == 0 {
    return Err(check_failure("THP reference is empty"))
  }
  var selected_count = 0
  var names = set.empty()
  for word in words {
    let bracketed = word.starts_with("[") and word.ends_with("]") and word.count_chars() >= 3
    let name = if bracketed {(word.split("") |> drop(1) |> take(word.count_chars() - 2)).join("")} else {word}
    if name == "" or name.contains("[") or name.contains("]") or set.has(names, name) {
      return Err(check_failure("THP reference has an invalid or duplicate policy name"))
    }
    names = set.add(names, name)
    if bracketed {
      selected_count += 1
    }
  }
  if selected_count != 1 {
    return Err(check_failure("THP reference must select exactly one policy"))
  }
  return Ok(value)
}

## Compares available named policies after rejecting a changed sysfs reference.
export pure compare_thp(
  candidate_json: Str, before: List[ThpReferencePolicy], after: List[ThpReferencePolicy],
) -> Result[ThpReferenceComparison] {
  var before_by_name: Map[Int] = {}
  var after_by_name: Map[Int] = {}
  for index in range(before.len()) {
    let item = before[index]
    if item.name not in ["enabled", "defrag"] or before_by_name.has(item.name) or parse_thp_reference(item.value)? != item.value {
      return Err(check_failure("before THP reference has an invalid policy"))
    }
    before_by_name = before_by_name.set(item.name, index)
  }
  for index in range(after.len()) {
    let item = after[index]
    if item.name not in ["enabled", "defrag"] or after_by_name.has(item.name) or parse_thp_reference(item.value)? != item.value {
      return Err(check_failure("after THP reference has an invalid policy"))
    }
    after_by_name = after_by_name.set(item.name, index)
  }
  if before.len() != after.len() {
    return Err(check_failure("THP reference field set changed around collection"))
  }
  for item in before {
    if !after_by_name.has(item.name) or after[after_by_name.get(item.name, -1)].value != item.value {
      return Err(check_failure("THP reference policy changed around collection"))
    }
  }
  let data = json.decode(candidate_json)?
  let candidate_values = json.get(data, ["memory", "transparent_huge_pages"])?.require(List[Str])?
  var candidate_by_name: Map[Str] = {}
  for entry in candidate_values {
    let parts = entry.split("=", maxsplit: 1)
    if parts.len() != 2 or parts[0] not in ["enabled", "defrag"] or candidate_by_name.has(parts[0]) {
      return Err(check_failure("candidate THP policy has an invalid or duplicate name"))
    }
    candidate_by_name = candidate_by_name.set(parts[0], parts[1])
  }
  var missing_names: List[Str] = []
  var unexpected_names: List[Str] = []
  var mismatched_names: List[Str] = []
  var matched_count = 0
  for item in before {
    if !candidate_by_name.has(item.name) {
      missing_names = missing_names.push(item.name)
      continue
    }
    matched_count += 1
    if candidate_by_name.get(item.name, "") != item.value {
      mismatched_names = mismatched_names.push(item.name)
    }
  }
  for name in candidate_by_name.keys() {
    if !before_by_name.has(name) {
      unexpected_names = unexpected_names.push(name)
    }
  }
  return Ok({
    reference_count: before.len(), candidate_count: candidate_values.len(), matched_count: matched_count,
    missing_names: missing_names |> sort-by ., unexpected_names: unexpected_names |> sort-by .,
    mismatched_names: mismatched_names |> sort-by .,
    exact: before.len() > 0 and missing_names.len() == 0 and unexpected_names.len() == 0 and mismatched_names.len() == 0,
  })
}

pure valid_vulnerability_name(name: Str) -> Bool {
  return name != "" and name != "." and name != ".." and name.count_chars() <= 255 and
    !name.contains("/") and !name.contains("\n") and !name.contains("\r") and !name.contains("\x00")
}

## Compares named descriptions only when both raw sysfs observations agree.
export pure compare_vulnerabilities(
  candidate_json: Str, before: List[VulnerabilityReference], after: List[VulnerabilityReference],
) -> Result[VulnerabilityReferenceComparison] {
  var before_by_name: Map[Str] = {}
  var after_by_name: Map[Str] = {}
  for item in before {
    if !valid_vulnerability_name(item.name) or before_by_name.has(item.name) {
      return Err(check_failure("before vulnerability reference has an invalid or duplicate name"))
    }
    before_by_name = before_by_name.set(item.name, item.description)
  }
  for item in after {
    if !valid_vulnerability_name(item.name) or after_by_name.has(item.name) {
      return Err(check_failure("after vulnerability reference has an invalid or duplicate name"))
    }
    after_by_name = after_by_name.set(item.name, item.description)
  }
  if before.len() != after.len() {
    return Err(check_failure("vulnerability reference field set changed around collection"))
  }
  for item in before {
    if !after_by_name.has(item.name) or after_by_name.get(item.name, "") != item.description {
      return Err(check_failure("vulnerability reference description changed around collection"))
    }
  }
  let data = json.decode(candidate_json)?
  let candidate_values = json.get(data, ["cpu", "vulnerabilities"])?.require(List[CandidateVulnerability])?
  var candidate_by_name: Map[CandidateRawTextObservation] = {}
  for item in candidate_values {
    if !valid_vulnerability_name(item.name) or candidate_by_name.has(item.name) {
      return Err(check_failure("candidate vulnerability has an invalid or duplicate name"))
    }
    candidate_by_name = candidate_by_name.set(item.name, item.description)
  }
  var missing_names: List[Str] = []
  var unexpected_names: List[Str] = []
  var mismatched_names: List[Str] = []
  var matched_count = 0
  for item in before {
    if !candidate_by_name.has(item.name) {
      missing_names = missing_names.push(item.name)
      continue
    }
    matched_count += 1
    let candidate = candidate_by_name.get(item.name, {state: "", value: null, raw_bytes_base64: null})
    if candidate.state != "observed" or candidate.value != item.description or candidate.raw_bytes_base64 != null {
      mismatched_names = mismatched_names.push(item.name)
    }
  }
  for name in candidate_by_name.keys() {
    if !before_by_name.has(name) {
      unexpected_names = unexpected_names.push(name)
    }
  }
  return Ok({
    reference_count: before.len(), candidate_count: candidate_values.len(), matched_count: matched_count,
    missing_names: missing_names |> sort-by ., unexpected_names: unexpected_names |> sort-by .,
    mismatched_names: mismatched_names |> sort-by .,
    exact: before.len() > 0 and missing_names.len() == 0 and unexpected_names.len() == 0 and mismatched_names.len() == 0,
  })
}

pure reference_swap_number(value: Str, signed: Bool) -> Result[Int] {
  let digits = if signed and value.starts_with("-") {(value.split("") |> drop(1)).join("")} else {value}
  if digits == "" {
    return Err(check_failure("swapon reference contains an empty number"))
  }
  for character in digits.split("") {
    if !"0123456789".contains(character) {
      return Err(check_failure("swapon reference contains a nondecimal number"))
    }
  }
  let parsed = value.parse_int()?
  if parsed > 9007199254740991 or parsed < -9007199254740991 {
    return Err(check_failure("swapon reference number exceeds the exact JSON integer range"))
  }
  return parsed
}

## Parses util-linux's raw byte-valued swap table and rejects rows without unambiguous columns.
export pure parse_swapon_raw(output: Str) -> Result[List[SwapReferenceDevice]] {
  let lines = output.trim().split("\n")
  if lines.len() == 0 or lines[0].trim() != "NAME TYPE SIZE USED PRIO" {
    return Err(check_failure("swapon reference has an unexpected header"))
  }
  var devices: List[SwapReferenceDevice] = []
  var seen = set.empty()
  for line in lines |> drop(1) {
    let columns = line.replace("\t", " ").split(" ") |> where .trim() != ""
    if columns.len() != 5 or columns[0] == "" or set.has(seen, columns[0]) {
      return Err(check_failure("swapon reference has ambiguous or duplicate swap identity"))
    }
    let size = reference_swap_number(columns[2], false)?
    let used = reference_swap_number(columns[3], false)?
    let priority = reference_swap_number(columns[4], true)?
    if used > size {
      return Err(check_failure("swapon reference reports used bytes above size"))
    }
    seen = set.add(seen, columns[0])
    devices = devices.push({name: columns[0], kind: columns[1], size_bytes: size, used_bytes: used, priority: priority})
  }
  return devices
}

## Compares swap areas by observed path, preserving missing and redacted candidate identities.
export pure compare_swap_devices(candidate_json: Str, reference: List[SwapReferenceDevice]) -> Result[SwapReferenceComparison] {
  let data = json.decode(candidate_json)?
  var reference_seen = set.empty()
  for device in reference {
    if device.name == "" or set.has(reference_seen, device.name) or device.size_bytes < 0 or
        device.size_bytes > 9007199254740991 or device.used_bytes < 0 or
        device.used_bytes > device.size_bytes or device.priority < -9007199254740991 or
        device.priority > 9007199254740991 {
      return Err(check_failure("swap reference contains an invalid or duplicate device"))
    }
    reference_seen = set.add(reference_seen, device.name)
  }
  let raw_swaps = json.get(data, ["memory", "swaps"], null)
  if raw_swaps == null {
    return {
      reference_count: reference.len(), candidate_count: 0, matched_count: 0,
      candidate_field_missing: true, missing_names: reference |> map .name |> sort-by .,
      unexpected_names: [], field_mismatches: [],
      kind_mismatches: 0, size_mismatches: 0, used_mismatches: 0, priority_mismatches: 0,
      exact: false,
    }
  }
  let candidates = raw_swaps.require(List[CandidateSwapDevice])?
  var candidate_seen = set.empty()
  var candidate_field_missing = false
  var matched_count = 0
  var unexpected_names: List[Str] = []
  var field_mismatches: List[Str] = []
  var kind_mismatches = 0
  var size_mismatches = 0
  var used_mismatches = 0
  var priority_mismatches = 0
  for device in candidates {
    if device.name.state != "observed" or device.name.value == null or device.name.value == "" {
      candidate_field_missing = true
      continue
    }
    let name = device.name.value ?? ""
    if set.has(candidate_seen, name) {
      return Err(check_failure("candidate swap report contains a duplicate identity"))
    }
    candidate_seen = set.add(candidate_seen, name)
    if !set.has(reference_seen, name) {
      unexpected_names = unexpected_names.push(name)
      continue
    }
    matched_count += 1
    for source in reference {
      if source.name == name {
        if device.kind != source.kind {kind_mismatches += 1}
        if device.size_bytes != source.size_bytes {size_mismatches += 1}
        if device.used_bytes != source.used_bytes {used_mismatches += 1}
        if device.priority != source.priority {priority_mismatches += 1}
        if device.kind != source.kind or device.size_bytes != source.size_bytes or
            device.used_bytes != source.used_bytes or device.priority != source.priority {
          field_mismatches = field_mismatches.push(name)
        }
      }
    }
  }
  var missing_names: List[Str] = []
  for device in reference {
    if !set.has(candidate_seen, device.name) {
      missing_names = missing_names.push(device.name)
    }
  }
  return {
    reference_count: reference.len(), candidate_count: candidates.len(), matched_count: matched_count,
    candidate_field_missing: candidate_field_missing,
    missing_names: missing_names |> sort-by ., unexpected_names: unexpected_names |> sort-by .,
    field_mismatches: field_mismatches |> sort-by .,
    kind_mismatches: kind_mismatches, size_mismatches: size_mismatches,
    used_mismatches: used_mismatches, priority_mismatches: priority_mismatches,
    exact: !candidate_field_missing and missing_names.len() == 0 and unexpected_names.len() == 0 and field_mismatches.len() == 0,
  }
}

## Checks order-independent stability of swap identities and byte-valued gauges.
export pure swap_reference_stable(before: List[SwapReferenceDevice], after: List[SwapReferenceDevice]) -> Bool {
  return (before |> sort-by .name) == (after |> sort-by .name)
}

type PciReferenceBdf = {address: Str, domain: Int, bus: Int, device: Int, function: Int}

pure pci_reference_hex(value: Str, width: Int) -> Result[Int] {
  if value.byte_len() != width or width < 1 {
    return Err(check_failure("lspci reference has an invalid hexadecimal field width"))
  }
  for digit in value.lower().split("") {
    if !"0123456789abcdef".contains(digit) {
      return Err(check_failure("lspci reference has a nonhexadecimal field"))
    }
  }
  return f"0x${value}".parse_int()
}

pure pci_reference_bdf(value: Str) -> Result[PciReferenceBdf] {
  let parts = value.split(":")
  if parts.len() != 3 {
    return Err(check_failure("lspci reference has an invalid slot address"))
  }
  let device_function = parts[2].split(".")
  if device_function.len() != 2 {
    return Err(check_failure("lspci reference has an invalid device or function address"))
  }
  let domain = pci_reference_hex(parts[0], 4)?
  let bus = pci_reference_hex(parts[1], 2)?
  let device = pci_reference_hex(device_function[0], 2)?
  let function = pci_reference_hex(device_function[1], 1)?
  if device > 31 or function > 7 {
    return Err(check_failure("lspci reference has an out-of-range slot address"))
  }
  return {address: value.lower(), domain: domain, bus: bus, device: device, function: function}
}

pure pci_reference_optional_hex(fields: Map[Str], key: Str, width: Int) -> Result[Int?] {
  if !fields.has(key) {return Ok(null)}
  return Ok(pci_reference_hex(fields.get(key, ""), width)?)
}

pure pci_reference_argv() -> List[Str] {
  return ["lspci", "-D", "-vmm", "-n", "-k"]
}

## Parses numeric lspci -D -vmm -n -k records without using the collector's PCI decoder.
export pure parse_lspci_vmm_numeric(output: Str) -> Result[List[PciReference]] {
  if output.byte_len() > 16777216 {
    return Err(check_failure("lspci reference exceeds the bounded output size"))
  }
  var references: List[PciReference] = []
  var seen = set.empty()
  for block in output.replace("\r\n", "\n").split("\n\n") {
    if block.trim() == "" {continue}
    var fields: Map[Str] = {}
    for line in block.trim().lines() {
      let parts = line.split(":\t")
      if parts.len() != 2 or parts[0] == "" {
        return Err(check_failure("lspci reference has a malformed field line"))
      }
      let key = parts[0]
      let value = parts[1].trim()
      if key in ["Slot", "Class", "Vendor", "Device", "SVendor", "SDevice", "Rev", "ProgIf", "Driver", "NUMANode", "IOMMUGroup"] {
        if fields.has(key) or value == "" {
          return Err(check_failure("lspci reference has a duplicate or empty identity field"))
        }
        fields = fields.set(key, value)
      }
    }
    if !fields.has("Slot") or !fields.has("Class") or !fields.has("Vendor") or !fields.has("Device") or
        fields.has("SVendor") != fields.has("SDevice") {
      return Err(check_failure("lspci reference is missing a required numeric identity"))
    }
    let bdf = pci_reference_bdf(fields.get("Slot", ""))?
    if set.has(seen, bdf.address) {
      return Err(check_failure("lspci reference has a duplicate slot address"))
    }
    seen = set.add(seen, bdf.address)
    let class_base = pci_reference_hex(fields.get("Class", ""), 4)?
    let prog_if = pci_reference_optional_hex(fields, "ProgIf", 2)?
    let revision = pci_reference_optional_hex(fields, "Rev", 2)?
    var numa_node: Int? = null
    if fields.has("NUMANode") {
      let numa_text = fields.get("NUMANode", "")
      for digit in numa_text.split("") {
        if !"0123456789".contains(digit) {
          return Err(check_failure("lspci reference has a nondecimal NUMA node"))
        }
      }
      let parsed = numa_text.parse_int()?
      if parsed < 0 or parsed > 9007199254740991 {
        return Err(check_failure("lspci reference has an invalid NUMA node"))
      }
      numa_node = parsed
    }
    var driver: Str? = null
    if fields.has("Driver") {driver = fields.get("Driver")?}
    var iommu_group: Str? = null
    if fields.has("IOMMUGroup") {iommu_group = fields.get("IOMMUGroup")?}
    references = references.push({
      address: bdf.address, domain: bdf.domain, bus: bdf.bus, device: bdf.device, function: bdf.function,
      vendor_id: pci_reference_hex(fields.get("Vendor", ""), 4)?,
      device_id: pci_reference_hex(fields.get("Device", ""), 4)?,
      class_code: class_base * 256 + (prog_if ?? 0), prog_if: prog_if, revision: revision,
      subsystem_vendor_id: pci_reference_optional_hex(fields, "SVendor", 4)?,
      subsystem_device_id: pci_reference_optional_hex(fields, "SDevice", 4)?,
      driver: driver, numa_node: numa_node, iommu_group: iommu_group,
    })
    if references.len() > 65536 {
      return Err(check_failure("lspci reference contains too many functions"))
    }
  }
  return Ok(references)
}

## Requires the independently reported PCI identities to survive the candidate collection interval.
export pure pci_reference_stable(before: List[PciReference], after: List[PciReference]) -> Bool {
  return (before |> sort-by .address) == (after |> sort-by .address)
}

## Compares BDF-scoped numeric PCI identity and independently exposed optional fields.
export pure compare_lspci_identity(candidate_json: Str, reference: List[PciReference]) -> Result[PciReferenceComparison] {
  let candidate_data = json.decode(candidate_json)?
  let section = json.get(candidate_data, ["pci"])?.require(CandidatePciSection)?
  var candidate_field_missing = section.status.state != "complete" or !section.status.enumeration_succeeded
  var candidates: Map[CandidatePciFunction] = {}
  for function in section.functions {
    if function.address == null or function.address == "" {
      candidate_field_missing = true
      continue
    }
    let address = function.address ?? ""
    if candidates.has(address) {
      return Err(check_failure("candidate PCI inventory has a duplicate slot address"))
    }
    candidates = candidates.set(address, function)
  }
  var references = set.empty()
  var missing_addresses: List[Str] = []
  var field_mismatches: List[Str] = []
  var matched_count = 0
  for item in reference {
    if set.has(references, item.address) {
      return Err(check_failure("lspci reference has a duplicate slot address"))
    }
    references = set.add(references, item.address)
    if !candidates.has(item.address) {
      missing_addresses = missing_addresses.push(item.address)
      continue
    }
    let function = candidates.get(item.address)?
    matched_count += 1
    if function.domain == null or function.bus == null or function.device == null or function.function == null or
        function.vendor_id == null or function.device_id == null or function.class_code == null or
        item.revision != null and function.revision == null or
        item.subsystem_vendor_id != null and function.subsystem_vendor_id == null or
        item.subsystem_device_id != null and function.subsystem_device_id == null or
        item.numa_node != null and function.numa_node == null or
        item.iommu_group != null and function.iommu_group == null {
      candidate_field_missing = true
    }
    if function.domain != item.domain {field_mismatches = field_mismatches.push(f"${item.address}.domain")}
    if function.bus != item.bus {field_mismatches = field_mismatches.push(f"${item.address}.bus")}
    if function.device != item.device {field_mismatches = field_mismatches.push(f"${item.address}.device")}
    if function.function != item.function {field_mismatches = field_mismatches.push(f"${item.address}.function")}
    if function.vendor_id != item.vendor_id {field_mismatches = field_mismatches.push(f"${item.address}.vendor_id")}
    if function.device_id != item.device_id {field_mismatches = field_mismatches.push(f"${item.address}.device_id")}
    let candidate_class = function.class_code ?? -1
    if function.class_code == null or candidate_class - candidate_class.bit_and(255) != item.class_code - item.class_code.bit_and(255) {
      field_mismatches = field_mismatches.push(f"${item.address}.class_code")
    }
    if item.prog_if != null and candidate_class.bit_and(255) != (item.prog_if ?? -1) {
      field_mismatches = field_mismatches.push(f"${item.address}.prog_if")
    }
    if item.revision != null and function.revision != item.revision {field_mismatches = field_mismatches.push(f"${item.address}.revision")}
    if item.subsystem_vendor_id != null and function.subsystem_vendor_id != item.subsystem_vendor_id {
      field_mismatches = field_mismatches.push(f"${item.address}.subsystem_vendor_id")
    }
    if item.subsystem_device_id != null and function.subsystem_device_id != item.subsystem_device_id {
      field_mismatches = field_mismatches.push(f"${item.address}.subsystem_device_id")
    }
    if function.driver != item.driver {field_mismatches = field_mismatches.push(f"${item.address}.driver")}
    if item.numa_node != null and function.numa_node != item.numa_node {field_mismatches = field_mismatches.push(f"${item.address}.numa_node")}
    if item.iommu_group != null and function.iommu_group != item.iommu_group {
      field_mismatches = field_mismatches.push(f"${item.address}.iommu_group")
    }
  }
  var unexpected_addresses: List[Str] = []
  for address in candidates.keys() {
    if !set.has(references, address) {unexpected_addresses = unexpected_addresses.push(address)}
  }
  return {
    reference_count: reference.len(), candidate_count: section.functions.len(), matched_count: matched_count,
    missing_addresses: missing_addresses |> sort-by ., unexpected_addresses: unexpected_addresses |> sort-by .,
    field_mismatches: field_mismatches |> sort-by ., candidate_field_missing: candidate_field_missing,
    exact_static: !candidate_field_missing and missing_addresses.len() == 0 and unexpected_addresses.len() == 0 and field_mismatches.len() == 0,
  }
}

pure ip_link_operstate(value: Str) -> Result[Str] {
  match value {
    "UNKNOWN" => return Ok("unknown")
    "NOTPRESENT" => return Ok("not_present")
    "DOWN" => return Ok("down")
    "LOWERLAYERDOWN" => return Ok("lower_layer_down")
    "TESTING" => return Ok("testing")
    "DORMANT" => return Ok("dormant")
    "UP" => return Ok("up")
    _ => return Err(check_failure("ip link reference has an unknown operational state"))
  }
}

## Parses the ifindex-scoped static facts from iproute2 link JSON.
export pure parse_ip_link_json(output: Str) -> Result[List[IpLinkReference]] {
  let rows = json.decode(output)?.require(List[Record])?
  var links: List[IpLinkReference] = []
  var ids = set.empty()
  var names = set.empty()
  if rows.len() > 65536 {
    return Err(check_failure("ip link reference contains too many interfaces"))
  }
  for row in rows {
    let ifindex = json.get(row, ["ifindex"])?.require(Int)?
    let name = json.get(row, ["ifname"])?.require(Str)?
    let mtu = json.get(row, ["mtu"])?.require(Int)?
    let flags = json.get(row, ["flags"])?.require(List[Str])?
    let state = json.get(row, ["operstate"], null).require(Str?)?
    let state_index = json.get(row, ["operstate_index"], null).require(Int?)?
    let kind = json.get(row, ["linkinfo", "info_kind"], null).require(Str?)?
    let master_name = json.get(row, ["master"], null).require(Str?)?
    let lower_name = json.get(row, ["link"], null).require(Str?)?
    let lower_index = json.get(row, ["link_index"], null).require(Int?)?
    if ifindex <= 0 or ifindex > 9007199254740991 or name == "" or
        mtu < 0 or mtu > 9007199254740991 or state != null and state_index != null or kind == "" or master_name == "" or
        lower_name == "" or lower_name != null and lower_index != null or lower_index != null and ((lower_index ?? -1) <= 0 or (lower_index ?? -1) > 9007199254740991) {
      return Err(check_failure("ip link reference has an invalid identity or state"))
    }
    let id_key = f"${ifindex}"
    if set.has(ids, id_key) or set.has(names, name) {
      return Err(check_failure("ip link reference has a duplicate interface identity"))
    }
    ids = set.add(ids, id_key)
    names = set.add(names, name)
    var flag_seen = set.empty()
    for flag in flags {
      if flag == "" or set.has(flag_seen, flag) {
        return Err(check_failure("ip link reference has an invalid flag list"))
      }
      flag_seen = set.add(flag_seen, flag)
    }
    var operstate: Str? = null
    if state != null {
      operstate = ip_link_operstate(state)?
    } else if state_index != null {
      if state_index < 0 or state_index > 255 {
        return Err(check_failure("ip link reference has an invalid operational state index"))
      }
      operstate = f"operstate_${state_index}"
    }
    links = links.push({ifindex: ifindex, name: name, mtu: mtu, admin_up: "UP" in flags, operstate: operstate, kind: kind, master_name: master_name, lower_name: lower_name, lower_index: lower_index})
  }
  return Ok(links)
}

## Requires the static interface facts to remain unchanged around candidate collection.
export pure ip_link_reference_stable(before: List[IpLinkReference], after: List[IpLinkReference]) -> Bool {
  return (before |> sort-by .ifindex) == (after |> sort-by .ifindex)
}

pure network_candidate_enumerated(state: Str, enumeration_succeeded: Bool) -> Bool {
  return enumeration_succeeded and state in ["complete", "partial"]
}

## Compares the stable, independently available link fields by interface index.
export pure compare_ip_links(candidate_json: Str, reference: List[IpLinkReference]) -> Result[IpLinkComparison] {
  let candidate_data = json.decode(candidate_json)?
  let state = json.get(candidate_data, ["network", "status", "state"])?.require(Str)?
  let enumeration_succeeded = json.get(candidate_data, ["network", "status", "enumeration_succeeded"])?.require(Bool)?
  let candidate = json.get(candidate_data, ["network", "links"])?.require(List[CandidateIpLink])?
  var candidate_by_id: Map[Int] = {}
  var position = 0
  while position < candidate.len() {
    let ifindex = candidate[position].ifindex
    if ifindex <= 0 or ifindex > 9007199254740991 or candidate_by_id.has(f"${ifindex}") {
      return Err(check_failure("candidate network links have an invalid or duplicate interface index"))
    }
    candidate_by_id = candidate_by_id.set(f"${ifindex}", position)
    position += 1
  }
  var reference_ids = set.empty()
  var reference_by_name: Map[Int] = {}
  for link in reference {
    if reference_by_name.has(link.name) {
      return Err(check_failure("ip link reference has a duplicate interface name"))
    }
    reference_by_name = reference_by_name.set(link.name, link.ifindex)
  }
  var missing_ids: List[Int] = []
  var field_mismatches: List[Str] = []
  var matched_count = 0
  for link in reference {
    let id_key = f"${link.ifindex}"
    if set.has(reference_ids, id_key) {
      return Err(check_failure("ip link reference has a duplicate interface index"))
    }
    reference_ids = set.add(reference_ids, id_key)
    if !candidate_by_id.has(id_key) {
      missing_ids = missing_ids.push(link.ifindex)
      continue
    }
    matched_count += 1
    let observed = candidate[candidate_by_id.get(id_key, 0)]
    if observed.name.state != "observed" or observed.name.value != link.name {
      field_mismatches = field_mismatches.push(f"${link.ifindex}.name")
    }
    if observed.mtu != link.mtu {
      field_mismatches = field_mismatches.push(f"${link.ifindex}.mtu")
    }
    if observed.admin_up != link.admin_up {
      field_mismatches = field_mismatches.push(f"${link.ifindex}.admin_up")
    }
    if observed.operational_state != link.operstate {
      field_mismatches = field_mismatches.push(f"${link.ifindex}.operational_state")
    }
    if observed.kind != link.kind {
      field_mismatches = field_mismatches.push(f"${link.ifindex}.kind")
    }
    var expected_master: Int? = null
    if link.master_name != null {
      let master_name = link.master_name ?? ""
      if !reference_by_name.has(master_name) {
        return Err(check_failure("ip link reference has an unresolved master interface"))
      }
      expected_master = reference_by_name.get(master_name)?
    }
    if observed.master_ifindex != expected_master {
      field_mismatches = field_mismatches.push(f"${link.ifindex}.master_ifindex")
    }
    var expected_lower = link.lower_index
    if link.lower_name != null {
      let lower_name = link.lower_name ?? ""
      if !reference_by_name.has(lower_name) {
        return Err(check_failure("ip link reference has an unresolved lower interface"))
      }
      expected_lower = reference_by_name.get(lower_name)?
    }
    if observed.lower_ifindex != expected_lower {
      field_mismatches = field_mismatches.push(f"${link.ifindex}.lower_ifindex")
    }
  }
  var unexpected_ids: List[Int] = []
  for link in candidate {
    if !set.has(reference_ids, f"${link.ifindex}") {
      unexpected_ids = unexpected_ids.push(link.ifindex)
    }
  }
  let candidate_field_missing = !network_candidate_enumerated(state, enumeration_succeeded)
  return {
    reference_count: reference.len(), candidate_count: candidate.len(), matched_count: matched_count,
    missing_ids: missing_ids, unexpected_ids: unexpected_ids, field_mismatches: field_mismatches,
    candidate_field_missing: candidate_field_missing,
    exact: !candidate_field_missing and missing_ids.len() == 0 and unexpected_ids.len() == 0 and field_mismatches.len() == 0,
  }
}

pure ip_address_scope(value: Str) -> Result[Str] {
  if value in ["global", "site", "link", "host", "nowhere"] {
    return Ok(value)
  }
  if value == "" {
    return Err(check_failure("ip address reference has an empty scope"))
  }
  for character in value.split("") {
    if !"0123456789".contains(character) {
      return Err(check_failure("ip address reference has an unknown scope"))
    }
  }
  let number = value.parse_int()?
  if number < 0 or number > 255 {
    return Err(check_failure("ip address reference has an invalid scope number"))
  }
  return Ok(f"scope_${number}")
}

pure ip_address_key(ifindex: Int, family: Str, address: Str, prefix_length: Int) -> Str {
  return f"${ifindex}|${family}|${address}|${prefix_length}"
}

## Parses the interface-indexed IPv4 and IPv6 address inventory from iproute2 JSON.
export pure parse_ip_address_json(output: Str) -> Result[List[IpAddressReference]] {
  let rows = json.decode(output)?.require(List[Record])?
  var addresses: List[IpAddressReference] = []
  var interfaces = set.empty()
  var identities = set.empty()
  if rows.len() > 65536 {
    return Err(check_failure("ip address reference contains too many interfaces"))
  }
  for row in rows {
    let ifindex = json.get(row, ["ifindex"])?.require(Int)?
    let name = json.get(row, ["ifname"])?.require(Str)?
    let id_key = f"${ifindex}"
    if ifindex <= 0 or ifindex > 9007199254740991 or name == "" or set.has(interfaces, id_key) {
      return Err(check_failure("ip address reference has an invalid or duplicate interface identity"))
    }
    interfaces = set.add(interfaces, id_key)
    let rows_for_link = json.get(row, ["addr_info"])?.require(List[Record])?
    for address_row in rows_for_link {
      if addresses.len() >= 65536 {
        return Err(check_failure("ip address reference contains too many addresses"))
      }
      let raw_family = json.get(address_row, ["family"])?.require(Str)?
      let family = if raw_family == "inet" {"ipv4"} else if raw_family == "inet6" {"ipv6"} else {""}
      if family == "" {
        return Err(check_failure("ip address reference has an unsupported family"))
      }
      let local = json.get(address_row, ["local"])?.require(Str)?
      let prefix_length = json.get(address_row, ["prefixlen"])?.require(Int)?
      let scope = ip_address_scope(json.get(address_row, ["scope"])?.require(Str)?)?
      let broadcast = json.get(address_row, ["broadcast"], null).require(Str?)?
      let valid_lifetime = json.get(address_row, ["valid_life_time"], null).require(Int?)?
      let preferred_lifetime = json.get(address_row, ["preferred_life_time"], null).require(Int?)?
      let maximum_prefix = if family == "ipv4" {32} else {128}
      if local == "" or local.contains("|") or prefix_length < 0 or prefix_length > maximum_prefix or broadcast == "" {
        return Err(check_failure("ip address reference has an invalid address field"))
      }
      if valid_lifetime != null {
        if valid_lifetime < 0 or valid_lifetime > 4294967295 {
          return Err(check_failure("ip address reference has an invalid lifetime"))
        }
      }
      if preferred_lifetime != null {
        if preferred_lifetime < 0 or preferred_lifetime > 4294967295 {
          return Err(check_failure("ip address reference has an invalid lifetime"))
        }
      }
      let key = ip_address_key(ifindex, family, local, prefix_length)
      if set.has(identities, key) {
        return Err(check_failure("ip address reference has a duplicate address identity"))
      }
      identities = set.add(identities, key)
      addresses = addresses.push({
        ifindex: ifindex, family: family, address: local, prefix_length: prefix_length,
        scope: scope, broadcast: broadcast,
        valid_lifetime_seconds: valid_lifetime, preferred_lifetime_seconds: preferred_lifetime,
      })
    }
  }
  return Ok(addresses)
}

## Treats changing address lifetimes as unscored while requiring a stable static inventory.
export pure ip_address_reference_stable(before: List[IpAddressReference], after: List[IpAddressReference]) -> Bool {
  var before_static: List[Str] = []
  for address in before {
    let broadcast = address.broadcast ?? ""
    let key = ip_address_key(address.ifindex, address.family, address.address, address.prefix_length)
    before_static = before_static.push(f"${key}|${address.scope}|${broadcast}")
  }
  var after_static: List[Str] = []
  for address in after {
    let broadcast = address.broadcast ?? ""
    let key = ip_address_key(address.ifindex, address.family, address.address, address.prefix_length)
    after_static = after_static.push(f"${key}|${address.scope}|${broadcast}")
  }
  return (before_static |> sort-by .) == (after_static |> sort-by .)
}

## Compares static address identity, link membership, scope, and broadcast state.
export pure compare_ip_addresses(candidate_json: Str, reference: List[IpAddressReference]) -> Result[IpAddressComparison] {
  let candidate_data = json.decode(candidate_json)?
  let state = json.get(candidate_data, ["network", "status", "state"])?.require(Str)?
  let enumeration_succeeded = json.get(candidate_data, ["network", "status", "enumeration_succeeded"])?.require(Bool)?
  let candidate_links = json.get(candidate_data, ["network", "links"])?.require(List[CandidateAddressLink])?
  var candidate_by_key: Map[CandidateIpAddress] = {}
  var candidate_links_seen = set.empty()
  var candidate_count = 0
  var candidate_field_missing = !network_candidate_enumerated(state, enumeration_succeeded)
  for link in candidate_links {
    let link_key = f"${link.ifindex}"
    if link.ifindex <= 0 or link.ifindex > 9007199254740991 or set.has(candidate_links_seen, link_key) {
      return Err(check_failure("candidate network addresses have an invalid or duplicate interface index"))
    }
    candidate_links_seen = set.add(candidate_links_seen, link_key)
    for address in link.addresses {
      candidate_count += 1
      if address.address.state != "observed" or address.address.value == null {
        candidate_field_missing = true
        continue
      }
      let key = ip_address_key(link.ifindex, address.family, address.address.value ?? "", address.prefix_length)
      if candidate_by_key.has(key) {
        return Err(check_failure("candidate network addresses have a duplicate address identity"))
      }
      candidate_by_key = candidate_by_key.set(key, address)
    }
  }
  var reference_keys = set.empty()
  var missing_keys: List[Str] = []
  var field_mismatches: List[Str] = []
  var matched_count = 0
  for address in reference {
    let key = ip_address_key(address.ifindex, address.family, address.address, address.prefix_length)
    if set.has(reference_keys, key) {
      return Err(check_failure("ip address reference has a duplicate address identity"))
    }
    reference_keys = set.add(reference_keys, key)
    if !candidate_by_key.has(key) {
      missing_keys = missing_keys.push(key)
      continue
    }
    matched_count += 1
    let observed = candidate_by_key.get(key)?
    if observed.scope != address.scope {
      field_mismatches = field_mismatches.push(f"${key}.scope")
    }
    if address.broadcast == null {
      if observed.broadcast.state != "absent" or observed.broadcast.value != null {
        field_mismatches = field_mismatches.push(f"${key}.broadcast")
      }
    } else if observed.broadcast.state != "observed" or observed.broadcast.value != address.broadcast {
      field_mismatches = field_mismatches.push(f"${key}.broadcast")
    }
  }
  var unexpected_keys: List[Str] = []
  for key in candidate_by_key.keys() {
    if !set.has(reference_keys, key) {
      unexpected_keys = unexpected_keys.push(key)
    }
  }
  return {
    reference_count: reference.len(), candidate_count: candidate_count, matched_count: matched_count,
    missing_keys: missing_keys |> sort-by ., unexpected_keys: unexpected_keys |> sort-by .,
    field_mismatches: field_mismatches |> sort-by ., candidate_field_missing: candidate_field_missing,
    exact_static: !candidate_field_missing and missing_keys.len() == 0 and unexpected_keys.len() == 0 and field_mismatches.len() == 0,
  }
}

pure ip_rule_table(value: Str) -> Result[Int] {
  match value {
    "local" => return Ok(255)
    "main" => return Ok(254)
    "default" => return Ok(253)
    _ => {}
  }
  if value == "" {
    return Err(check_failure("ip rule reference has an empty table"))
  }
  for character in value.split("") {
    if !"0123456789".contains(character) {
      return Err(check_failure("ip rule reference has an unknown table name"))
    }
  }
  let number = value.parse_int()?
  if number < 0 or number > 4294967295 {
    return Err(check_failure("ip rule reference has an invalid table number"))
  }
  return Ok(number)
}

pure ip_rule_prefix(value: Str?, length: Int?, family: Str) -> Result[Str?] {
  let maximum = if family == "ipv4" {32} else {128}
  let prefix_length = length ?? (if value == null or value == "all" {0} else {maximum})
  if prefix_length < 0 or prefix_length > maximum {
    return Err(check_failure("ip rule reference has an invalid prefix length"))
  }
  if value == null or value == "all" {
    if prefix_length != 0 {
      return Err(check_failure("ip rule reference has a prefix length without an address"))
    }
    return Ok(null)
  }
  if value == "" {
    return Err(check_failure("ip rule reference has an empty prefix address"))
  }
  return Ok(value)
}

pure ip_rule_hex(value: Str?) -> Result[Int?] {
  if value == null {return Ok(null)}
  let raw = value ?? ""
  if raw != "0" and (!raw.starts_with("0x") or raw.byte_len() <= 2) {
    return Err(check_failure("ip rule reference has an invalid hex selector"))
  }
  let parsed = raw.parse_int()?
  if parsed < 0 or parsed > 4294967295 {
    return Err(check_failure("ip rule reference has an out-of-range hex selector"))
  }
  return Ok(parsed)
}

pure ip_rule_key(rule: IpRuleReference) -> Result[Str] {
  return json.encode([
    rule.family, f"${rule.priority}", rule.source ?? "", f"${rule.source_prefix_length}",
    rule.destination ?? "", f"${rule.destination_prefix_length}", f"${rule.fwmark ?? -1}",
    f"${rule.fwmask ?? -1}", f"${rule.table}", rule.action,
    rule.input_name ?? "", rule.output_name ?? "",
  ])
}

pure network_rule_argv(family: Str) -> List[Str] {
  let family_option = if family == "ipv4" {"inet"} else {"inet6"}
  return ["ip", "-json", "-family", family_option, "rule", "show"]
}

pure network_route_argv(family: Str) -> List[Str] {
  let family_option = if family == "ipv4" {"inet"} else {"inet6"}
  return ["ip", "-json", "-family", family_option, "route", "show", "table", "all"]
}

## Parses one family at a time because iproute2 rule JSON omits the address family.
export pure parse_ip_rule_json(output: Str, family: Str) -> Result[List[IpRuleReference]] {
  if family != "ipv4" and family != "ipv6" {
    return Err(check_failure("ip rule reference needs an explicit IPv4 or IPv6 family"))
  }
  let rows = json.decode(output)?.require(List[Record])?
  if rows.len() > 65536 {
    return Err(check_failure("ip rule reference contains too many rules"))
  }
  var rules: List[IpRuleReference] = []
  var keys = set.empty()
  for row in rows {
    let priority = json.get(row, ["priority"])?.require(Int)?
    let source = json.get(row, ["src"], null).require(Str?)?
    let source_length = json.get(row, ["srclen"], null).require(Int?)?
    let destination = json.get(row, ["dst"], null).require(Str?)?
    let destination_length = json.get(row, ["dstlen"], null).require(Int?)?
    let mark = ip_rule_hex(json.get(row, ["fwmark"], null).require(Str?)?)?
    let mask = ip_rule_hex(json.get(row, ["fwmask"], null).require(Str?)?)?
    let table_name = json.get(row, ["table"], "0").require(Str)?
    let input_name = json.get(row, ["iif"], null).require(Str?)?
    let output_name = json.get(row, ["oif"], null).require(Str?)?
    let action_name = json.get(row, ["action"], null).require(Str?)?
    let goto_value = json.get(row, ["goto"], null)
    let action = if action_name != null {action_name ?? ""} else if goto_value != null {"goto"} else if "nop" in row.keys() {"nop"} else {"to_table"}
    let source_address = ip_rule_prefix(source, source_length, family)?
    let destination_address = ip_rule_prefix(destination, destination_length, family)?
    let maximum = if family == "ipv4" {32} else {128}
    let source_prefix_length = source_length ?? (if source_address == null {0} else {maximum})
    let destination_prefix_length = destination_length ?? (if destination_address == null {0} else {maximum})
    if priority < 0 or priority > 4294967295 or action == "" or input_name == "" or output_name == "" {
      return Err(check_failure("ip rule reference has an invalid selector"))
    }
    let rule: IpRuleReference = {
      family: family, priority: priority, source: source_address, source_prefix_length: source_prefix_length,
      destination: destination_address, destination_prefix_length: destination_prefix_length,
      fwmark: mark, fwmask: if mask == 4294967295 {null} else {mask}, table: ip_rule_table(table_name)?, action: action,
      input_name: input_name, output_name: output_name,
    }
    let key = ip_rule_key(rule)?
    if set.has(keys, key) {
      return Err(check_failure("ip rule reference has duplicate static selectors"))
    }
    keys = set.add(keys, key)
    rules = rules.push(rule)
  }
  return Ok(rules)
}

## Requires the static rule tuple to be unchanged across the capture bracket.
export pure ip_rule_reference_stable(before: List[IpRuleReference], after: List[IpRuleReference]) -> Result[Bool] {
  var before_keys: List[Str] = []
  for rule in before {
    before_keys = before_keys.push(ip_rule_key(rule)?)
  }
  var after_keys: List[Str] = []
  for rule in after {
    after_keys = after_keys.push(ip_rule_key(rule)?)
  }
  return Ok((before_keys |> sort-by .) == (after_keys |> sort-by .))
}

## Compares static rule tuples after resolving candidate interface indices to names.
export pure compare_ip_rules(candidate_json: Str, reference: List[IpRuleReference]) -> Result[IpRuleComparison] {
  let candidate_data = json.decode(candidate_json)?
  let state = json.get(candidate_data, ["network", "status", "state"])?.require(Str)?
  let enumeration_succeeded = json.get(candidate_data, ["network", "status", "enumeration_succeeded"])?.require(Bool)?
  let links = json.get(candidate_data, ["network", "links"])?.require(List[CandidateRuleLink])?
  let candidate = json.get(candidate_data, ["network", "rules"])?.require(List[CandidateIpRule])?
  var names_by_index: Map[Str] = {}
  var candidate_field_missing = !network_candidate_enumerated(state, enumeration_succeeded)
  for link in links {
    if link.ifindex <= 0 or names_by_index.has(f"${link.ifindex}") {
      return Err(check_failure("candidate network rules have an invalid or duplicate link index"))
    }
    if link.name.state == "observed" and link.name.value != null {
      names_by_index = names_by_index.set(f"${link.ifindex}", link.name.value ?? "")
    } else {
      candidate_field_missing = true
    }
  }
  var candidate_keys = set.empty()
  var candidate_key_list: List[Str] = []
  for rule in candidate {
    if rule.source.state not in ["observed", "absent"] or rule.destination.state not in ["observed", "absent"] {
      candidate_field_missing = true
      continue
    }
    if rule.source.state == "observed" and rule.source.value == null or rule.destination.state == "observed" and rule.destination.value == null or
        rule.source.state == "absent" and rule.source.value != null or rule.destination.state == "absent" and rule.destination.value != null {
      candidate_field_missing = true
      continue
    }
    let input_index = rule.input_ifindex ?? 0
    let output_index = rule.output_ifindex ?? 0
    if input_index != 0 and !names_by_index.has(f"${input_index}") or output_index != 0 and !names_by_index.has(f"${output_index}") {
      candidate_field_missing = true
      continue
    }
    var input_name: Str? = null
    var output_name: Str? = null
    if input_index != 0 {input_name = names_by_index.get(f"${input_index}")?}
    if output_index != 0 {output_name = names_by_index.get(f"${output_index}")?}
    if rule.input_ifindex != null and input_name == null or rule.output_ifindex != null and output_name == null or rule.table == null {
      candidate_field_missing = true
      continue
    }
    let normalized: IpRuleReference = {
      family: rule.family, priority: rule.priority ?? 0,
      source: rule.source.value, source_prefix_length: rule.source_prefix_length,
      destination: rule.destination.value, destination_prefix_length: rule.destination_prefix_length,
      fwmark: rule.fwmark, fwmask: if rule.fwmask == 4294967295 {null} else {rule.fwmask}, table: rule.table ?? 0, action: rule.action,
      input_name: input_name, output_name: output_name,
    }
    let key = ip_rule_key(normalized)?
    if set.has(candidate_keys, key) {
      return Err(check_failure("candidate network rules have duplicate static selectors"))
    }
    candidate_keys = set.add(candidate_keys, key)
    candidate_key_list = candidate_key_list.push(key)
  }
  var reference_keys = set.empty()
  var missing_keys: List[Str] = []
  var matched_count = 0
  for rule in reference {
    let key = ip_rule_key(rule)?
    if set.has(reference_keys, key) {
      return Err(check_failure("ip rule reference has duplicate static selectors"))
    }
    reference_keys = set.add(reference_keys, key)
    if set.has(candidate_keys, key) {matched_count += 1} else {missing_keys = missing_keys.push(key)}
  }
  var unexpected_keys: List[Str] = []
  for key in candidate_key_list {
    if !set.has(reference_keys, key) {unexpected_keys = unexpected_keys.push(key)}
  }
  return {
    reference_count: reference.len(), candidate_count: candidate.len(), matched_count: matched_count,
    missing_keys: missing_keys |> sort-by ., unexpected_keys: unexpected_keys |> sort-by .,
    candidate_field_missing: candidate_field_missing,
    exact_static: !candidate_field_missing and missing_keys.len() == 0 and unexpected_keys.len() == 0,
  }
}

pure ip_route_destination(value: Str, family: Str) -> Result[List[Str]] {
  let maximum = if family == "ipv4" {32} else {128}
  if value == "default" {
    return Ok([if family == "ipv4" {"0.0.0.0"} else {"::"}, "0"])
  }
  let parts = value.split("/")
  if parts.len() < 1 or parts.len() > 2 or parts[0] == "" {
    return Err(check_failure("ip route reference has an invalid destination"))
  }
  var length = maximum
  if parts.len() == 2 {
    if parts[1] == "" {
      return Err(check_failure("ip route reference has an empty prefix length"))
    }
    for character in parts[1].split("") {
      if !"0123456789".contains(character) {
        return Err(check_failure("ip route reference has a nondecimal prefix length"))
      }
    }
    length = parts[1].parse_int()?
  }
  if length < 0 or length > maximum {
    return Err(check_failure("ip route reference has an out-of-range prefix length"))
  }
  return Ok([parts[0], f"${length}"])
}

pure ip_route_protocol(value: Str) -> Str {
  match value {
    "ra" => return "router_advertisement"
    "unspec" => return "unspecified"
    _ => return value
  }
}

pure ip_route_flags(names: List[Str], route_level: Bool) -> Result[Int] {
  var flags = 0
  for name in names {
    let bit = match name {
      "dead" => 1
      "pervasive" => 2
      "onlink" => 4
      "offload" => 8
      "linkdown" => 16
      "unresolved" => 32
      "trap" => 64
      "notify" => if route_level {256} else {-1}
      "rt_offload" => if route_level {16384} else {-1}
      "rt_trap" => if route_level {32768} else {-1}
      "rt_offload_failed" => if route_level {536870912} else {-1}
      _ => -1
    }
    if bit < 0 {
      return Err(check_failure("ip route reference has an unknown next-hop flag"))
    }
    if flags.bit_and(bit) != 0 {
      return Err(check_failure("ip route reference has a duplicate next-hop flag"))
    }
    flags = flags.bit_or(bit)
  }
  return Ok(flags)
}

# The iproute2 route flags array omits several kernel bits, so those candidates cannot be scored exactly.
pure ip_route_has_only_reported_flags(flags: Int) -> Bool {
  return flags >= 0 and flags.bit_and(536920447) == flags
}

pure ip_route_nexthop_key(hop: IpRouteNexthopReference) -> Result[Str] {
  return json.encode([hop.output_name, hop.gateway ?? "", f"${hop.weight}", f"${hop.flags}"])
}

pure ip_route_key(route: IpRouteReference) -> Result[Str] {
  var hop_keys: List[Str] = []
  for hop in route.nexthops {
    hop_keys = hop_keys.push(ip_route_nexthop_key(hop)?)
  }
  let hops_key = json.encode(hop_keys |> sort-by .)?
  return json.encode([
    route.family, route.destination, f"${route.prefix_length}", f"${route.table}",
    f"${route.metric ?? -1}", route.route_type, route.scope, route.protocol,
    route.gateway ?? "", route.output_name ?? "", route.source ?? "",
    f"${route.source_prefix_length}", route.preferred_source ?? "", f"${route.flags}", hops_key,
  ])
}

## Parses one family at a time because iproute2 route JSON omits the address family.
export pure parse_ip_route_json(output: Str, family: Str) -> Result[List[IpRouteReference]] {
  if family != "ipv4" and family != "ipv6" {
    return Err(check_failure("ip route reference needs an explicit IPv4 or IPv6 family"))
  }
  let rows = json.decode(output)?.require(List[Record])?
  if rows.len() > 65536 {
    return Err(check_failure("ip route reference contains too many routes"))
  }
  var routes: List[IpRouteReference] = []
  var keys = set.empty()
  for row in rows {
    let destination_text = json.get(row, ["dst"])?.require(Str)?
    let destination_parts = ip_route_destination(destination_text, family)?
    let source_text = json.get(row, ["from"], null).require(Str?)?
    let zero_source = json.get(row, ["src"], null).require(Str?)?
    if source_text != null and zero_source != null {
      return Err(check_failure("ip route reference has conflicting source selectors"))
    }
    var source: Str? = null
    var source_prefix_length = 0
    if source_text != null {
      let parts = ip_route_destination(source_text, family)?
      source = parts[0]
      source_prefix_length = parts[1].parse_int()?
    } else if zero_source != null {
      let parts = ip_route_destination(zero_source, family)?
      if parts[0] != "0" {
        return Err(check_failure("ip route reference has an invalid zero-address source selector"))
      }
      source_prefix_length = parts[1].parse_int()?
    }
    let preferred_source = json.get(row, ["prefsrc"], null).require(Str?)?
    let table_name = json.get(row, ["table"], "main").require(Str)?
    let metric = json.get(row, ["metric"], null).require(Int?)?
    let route_type = json.get(row, ["type"], "unicast").require(Str)?
    let scope = json.get(row, ["scope"], "global").require(Str)?
    let protocol = json.get(row, ["protocol"], "boot").require(Str)?
    let gateway = json.get(row, ["gateway"], null).require(Str?)?
    let output_name = json.get(row, ["dev"], null).require(Str?)?
    let flags = ip_route_flags(json.get(row, ["flags"], []).require(List[Str])?, true)?
    if route_type == "" or scope == "" or protocol == "" or gateway == "" or output_name == "" or preferred_source == "" {
      return Err(check_failure("ip route reference has an empty static field"))
    }
    let hop_rows = json.get(row, ["nexthops"], []).require(List[Record])?
    if hop_rows.len() > 4096 or json.get(row, ["via"], null) != null {
      return Err(check_failure("ip route reference has unsupported or excessive next-hop data"))
    }
    var nexthops: List[IpRouteNexthopReference] = []
    for hop_row in hop_rows {
      if json.get(hop_row, ["via"], null) != null {
        return Err(check_failure("ip route reference has an unsupported cross-family next hop"))
      }
      let hop_name = json.get(hop_row, ["dev"])?.require(Str)?
      let hop_gateway = json.get(hop_row, ["gateway"], null).require(Str?)?
      let weight = json.get(hop_row, ["weight"])?.require(Int)?
      let flags = ip_route_flags(json.get(hop_row, ["flags"])?.require(List[Str])?, false)?
      if hop_name == "" or hop_gateway == "" or weight < 1 or weight > 256 {
        return Err(check_failure("ip route reference has an invalid next hop"))
      }
      nexthops = nexthops.push({output_name: hop_name, gateway: hop_gateway, weight: weight, flags: flags})
    }
    if metric != null and ((metric ?? -1) < 0 or (metric ?? -1) > 4294967295) {
      return Err(check_failure("ip route reference has an invalid metric"))
    }
    let route: IpRouteReference = {
      family: family, destination: destination_parts[0], prefix_length: destination_parts[1].parse_int()?,
      source: source, source_prefix_length: source_prefix_length, preferred_source: preferred_source,
      table: ip_rule_table(table_name)?, metric: metric,
      route_type: if route_type == "xresolve" {"external_resolve"} else {route_type},
      scope: scope, protocol: ip_route_protocol(protocol), gateway: gateway, output_name: output_name, flags: flags,
      nexthops: nexthops,
    }
    let key = ip_route_key(route)?
    if set.has(keys, key) {
      return Err(check_failure("ip route reference has duplicate static identities"))
    }
    keys = set.add(keys, key)
    routes = routes.push(route)
  }
  return Ok(routes)
}

## Requires route identity and selected static fields to be unchanged across the bracket.
export pure ip_route_reference_stable(before: List[IpRouteReference], after: List[IpRouteReference]) -> Result[Bool] {
  var before_keys: List[Str] = []
  for route in before {before_keys = before_keys.push(ip_route_key(route)?)}
  var after_keys: List[Str] = []
  for route in after {after_keys = after_keys.push(ip_route_key(route)?)}
  return Ok((before_keys |> sort-by .) == (after_keys |> sort-by .))
}

## Compares selected static routes after resolving interface indices to names.
export pure compare_ip_routes(candidate_json: Str, reference: List[IpRouteReference]) -> Result[IpRouteComparison] {
  let candidate_data = json.decode(candidate_json)?
  let state = json.get(candidate_data, ["network", "status", "state"])?.require(Str)?
  let enumeration_succeeded = json.get(candidate_data, ["network", "status", "enumeration_succeeded"])?.require(Bool)?
  let links = json.get(candidate_data, ["network", "links"])?.require(List[CandidateRuleLink])?
  let candidate = json.get(candidate_data, ["network", "routes"])?.require(List[CandidateIpRoute])?
  var names_by_index: Map[Str] = {}
  var candidate_field_missing = !network_candidate_enumerated(state, enumeration_succeeded)
  for link in links {
    if link.ifindex <= 0 or names_by_index.has(f"${link.ifindex}") {
      return Err(check_failure("candidate network routes have an invalid or duplicate link index"))
    }
    if link.name.state == "observed" and link.name.value != null {
      names_by_index = names_by_index.set(f"${link.ifindex}", link.name.value ?? "")
    } else {
      candidate_field_missing = true
    }
  }
  var candidate_keys = set.empty()
  var candidate_key_list: List[Str] = []
  for route in candidate {
    if route.destination.state != "observed" or route.destination.value == null or
        route.source.state == "observed" and route.source.value == null or
        route.source.state == "absent" and route.source.value != null or
        route.source.state not in ["observed", "absent"] or
        route.preferred_source.state == "observed" and route.preferred_source.value == null or
        route.preferred_source.state == "absent" and route.preferred_source.value != null or
        route.preferred_source.state not in ["observed", "absent"] or
        route.gateway.state == "observed" and route.gateway.value == null or
        route.gateway.state == "absent" and route.gateway.value != null or
        route.gateway.state not in ["observed", "absent"] or route.scope == null or route.protocol == null or
        !ip_route_has_only_reported_flags(route.flags) {
      candidate_field_missing = true
      continue
    }
    let output_index = route.output_ifindex ?? 0
    if output_index != 0 and !names_by_index.has(f"${output_index}") {
      candidate_field_missing = true
      continue
    }
    var output_name: Str? = null
    if output_index != 0 {output_name = names_by_index.get(f"${output_index}")?}
    var nexthops: List[IpRouteNexthopReference] = []
    var hop_missing = false
    for hop in route.nexthops {
      let hop_index = hop.ifindex
      if hop_index <= 0 or !names_by_index.has(f"${hop_index}") or hop.hops < 0 or hop.hops > 255 or
          hop.flags < 0 or hop.flags > 127 or
          hop.gateway.state == "observed" and hop.gateway.value == null or
          hop.gateway.state == "absent" and hop.gateway.value != null or
          hop.gateway.state not in ["observed", "absent"] {
        hop_missing = true
        continue
      }
      nexthops = nexthops.push({
        output_name: names_by_index.get(f"${hop_index}")?,
        gateway: hop.gateway.value, weight: hop.hops + 1, flags: hop.flags,
      })
    }
    if hop_missing {
      candidate_field_missing = true
      continue
    }
    let normalized: IpRouteReference = {
      family: route.family, destination: route.destination.value ?? "", prefix_length: route.prefix_length,
      source: route.source.value, source_prefix_length: route.source_prefix_length,
      preferred_source: route.preferred_source.value,
      table: route.table, metric: route.metric, route_type: route.route_type,
      scope: route.scope ?? "", protocol: route.protocol ?? "",
      gateway: route.gateway.value, output_name: output_name, flags: route.flags, nexthops: nexthops,
    }
    let key = ip_route_key(normalized)?
    if set.has(candidate_keys, key) {
      return Err(check_failure("candidate network routes have duplicate static identities"))
    }
    candidate_keys = set.add(candidate_keys, key)
    candidate_key_list = candidate_key_list.push(key)
  }
  var reference_keys = set.empty()
  var missing_keys: List[Str] = []
  var matched_count = 0
  for route in reference {
    let key = ip_route_key(route)?
    if set.has(reference_keys, key) {
      return Err(check_failure("ip route reference has duplicate static identities"))
    }
    reference_keys = set.add(reference_keys, key)
    if set.has(candidate_keys, key) {matched_count += 1} else {missing_keys = missing_keys.push(key)}
  }
  var unexpected_keys: List[Str] = []
  for key in candidate_key_list {
    if !set.has(reference_keys, key) {unexpected_keys = unexpected_keys.push(key)}
  }
  return {
    reference_count: reference.len(), candidate_count: candidate.len(), matched_count: matched_count,
    missing_keys: missing_keys |> sort-by ., unexpected_keys: unexpected_keys |> sort-by .,
    candidate_field_missing: candidate_field_missing,
    exact_static: !candidate_field_missing and missing_keys.len() == 0 and unexpected_keys.len() == 0,
  }
}

pure parse_major_minor_reference(value: Str, adapter: Str) -> Result[List[Int]] {
  let parts = value.split(":")
  if parts.len() != 2 {
    return Err(check_failure(f"${adapter} reference has an invalid major:minor identity"))
  }
  var numbers: List[Int] = []
  for part in parts {
    if part == "" {
      return Err(check_failure(f"${adapter} reference has an empty major:minor component"))
    }
    for character in part.split("") {
      if !"0123456789".contains(character) {
        return Err(check_failure(f"${adapter} reference has a nondecimal major:minor component"))
      }
    }
    let number = part.parse_int()?
    if number > 9007199254740991 {
      return Err(check_failure(f"${adapter} reference has a JSON-unsafe major:minor component"))
    }
    numbers = numbers.push(number)
  }
  return numbers
}

pure block_edge_key(edge: BlockReferenceEdge) -> Str {
  return f"${edge.parent_name.count_chars()}:${edge.parent_name}${edge.child_name.count_chars()}:${edge.child_name}:${edge.partition}"
}

## Parses explicit-column lsblk JSON without losing repeated tree nodes or their relationships.
export pure parse_lsblk_json(output: Str) -> Result[BlockReference] {
  let data = json.decode(output)?
  let roots = json.get(data, ["blockdevices"], null)
  if roots == null {
    return Err(check_failure("lsblk reference lacks blockdevices"))
  }
  var pending: List[PendingLsblkNode] = []
  for node in roots.require(List[Record])? {
    pending = pending.push({node: node, parent_name: null})
  }
  var devices: List[BlockReferenceDevice] = []
  var edges: List[BlockReferenceEdge] = []
  var by_name: Map[Int] = {}
  var by_major_minor: Map[Str] = {}
  var edge_seen = set.empty()
  var cursor = 0
  while cursor < pending.len() {
    if pending.len() > 65536 {
      return Err(check_failure("lsblk reference contains too many tree nodes"))
    }
    let item = pending[cursor]
    cursor += 1
    let node = item.node
    for field in ["name", "kname", "maj:min", "size", "type", "ro", "rm", "rota", "log-sec", "phy-sec"] {
      if json.get(node, [field], null) == null {
        return Err(check_failure(f"lsblk reference lacks ${field}"))
      }
    }
    let display_name = json.get(node, ["name"])?.require(Str)?
    let name = json.get(node, ["kname"])?.require(Str)?
    let major_minor = json.get(node, ["maj:min"])?.require(Str)?
    let numbers = parse_major_minor_reference(major_minor, "lsblk")?
    let size = json.get(node, ["size"])?.require(Int)?
    let kind = json.get(node, ["type"])?.require(Str)?
    let parent = json.get(node, ["pkname"], null).require(Str?)?
    let read_only = json.get(node, ["ro"])?.require(Bool)?
    let removable = json.get(node, ["rm"])?.require(Bool)?
    let rotational = json.get(node, ["rota"])?.require(Bool)?
    let logical_sector = json.get(node, ["log-sec"])?.require(Int)?
    let physical_sector = json.get(node, ["phy-sec"])?.require(Int)?
    if display_name == "" or name == "" or kind == "" or
        size < 0 or size > 9007199254740991 or
        logical_sector <= 0 or logical_sector > 9007199254740991 or
        physical_sector <= 0 or physical_sector > 9007199254740991 {
      return Err(check_failure("lsblk reference has an invalid or JSON-unsafe device value"))
    }
    let device: BlockReferenceDevice = {
      name: name, major: numbers[0], minor: numbers[1], kind: kind,
      size_bytes: size, logical_sector_bytes: logical_sector,
      physical_sector_bytes: physical_sector, removable: removable,
      rotational: rotational, read_only: read_only,
    }
    let numeric_identity = f"${numbers[0]}:${numbers[1]}"
    if by_name.has(name) {
      if devices[by_name.get(name)?] != device {
        return Err(check_failure("lsblk reference has conflicting duplicate device facts"))
      }
    } else {
      if by_major_minor.has(numeric_identity) {
        return Err(check_failure("lsblk reference has duplicate major:minor identity"))
      }
      by_name = by_name.set(name, devices.len())
      by_major_minor = by_major_minor.set(numeric_identity, name)
      devices = devices.push(device)
    }
    if item.parent_name != null {
      let parent_name = item.parent_name ?? ""
      let edge: BlockReferenceEdge = {parent_name: parent_name, child_name: name, partition: kind == "part"}
      let key = block_edge_key(edge)
      if !set.has(edge_seen, key) {
        edge_seen = set.add(edge_seen, key)
        edges = edges.push(edge)
      }
      if parent != null and parent != parent_name {
        return Err(check_failure("lsblk reference parent identity conflicts with its tree edge"))
      }
    }
    let children = json.get(node, ["children"], []).require(List[Record])?
    for child in children {
      pending = pending.push({node: child, parent_name: name})
    }
  }
  return {devices: devices, edges: edges}
}

## Compares observed block identities and tree edges by kernel name, independent of row order.
export pure compare_block_devices(candidate_json: Str, reference: BlockReference) -> Result[BlockReferenceComparison] {
  let data = json.decode(candidate_json)?
  let raw = json.get(data, ["storage", "devices"], null)
  if raw == null {
    return {
      reference_count: reference.devices.len(), candidate_count: 0, matched_count: 0,
      matched_edges: 0, missing_edges: reference.edges.len(), unexpected_edges: 0,
      missing_names: reference.devices |> map .name |> sort-by .,
      unexpected_names: [], field_mismatches: [], major_minor_mismatches: 0,
      size_mismatches: 0, sector_mismatches: 0, flag_mismatches: 0,
      partition_mismatches: 0, candidate_field_missing: true, exact: false,
    }
  }
  let candidates = raw.require(List[CandidateBlockDevice])?
  var reference_by_name: Map[Int] = {}
  for index in range(reference.devices.len()) {
    let name = reference.devices[index].name
    if name == "" or reference_by_name.has(name) {
      return Err(check_failure("block reference contains duplicate or empty device identity"))
    }
    reference_by_name = reference_by_name.set(name, index)
  }
  var candidate_by_name: Map[Int] = {}
  var candidate_field_missing = false
  var matched_count = 0
  var unexpected_names: List[Str] = []
  var field_mismatches: List[Str] = []
  var major_minor_mismatches = 0
  var size_mismatches = 0
  var sector_mismatches = 0
  var flag_mismatches = 0
  var partition_mismatches = 0
  for index in range(candidates.len()) {
    let device = candidates[index]
    if device.name == null or device.name == "" {
      candidate_field_missing = true
      continue
    }
    let name = device.name ?? ""
    if candidate_by_name.has(name) {
      return Err(check_failure("candidate block report contains a duplicate device identity"))
    }
    candidate_by_name = candidate_by_name.set(name, index)
    if !reference_by_name.has(name) {
      unexpected_names = unexpected_names.push(name)
      continue
    }
    matched_count += 1
    let source = reference.devices[reference_by_name.get(name)?]
    let major_minor_bad = device.major != source.major or device.minor != source.minor
    let size_bad = device.size_bytes != source.size_bytes
    let sector_bad = device.logical_sector_bytes != source.logical_sector_bytes or
      device.physical_sector_bytes != source.physical_sector_bytes
    let flag_bad = device.removable != source.removable or device.rotational != source.rotational or
      device.read_only != source.read_only
    let partition_bad = (device.kind == "partition") != (source.kind == "part")
    if major_minor_bad {major_minor_mismatches += 1}
    if size_bad {size_mismatches += 1}
    if sector_bad {sector_mismatches += 1}
    if flag_bad {flag_mismatches += 1}
    if partition_bad {partition_mismatches += 1}
    if major_minor_bad or size_bad or sector_bad or flag_bad or partition_bad {
      field_mismatches = field_mismatches.push(name)
    }
  }
  var missing_names: List[Str] = []
  for device in reference.devices {
    if !candidate_by_name.has(device.name) {
      missing_names = missing_names.push(device.name)
    }
  }
  var matched_edges = 0
  var missing_edges = 0
  var unexpected_edges = 0
  var reference_edges = set.empty()
  for edge in reference.edges {
    reference_edges = set.add(reference_edges, block_edge_key(edge))
  }
  var candidate_edges = set.empty()
  var candidate_edge_keys: List[Str] = []
  for child_index in range(candidates.len()) {
    let child = candidates[child_index]
    if child.name == null or child.name == "" {
      continue
    }
    let child_name = child.name ?? ""
    var slave_seen = set.empty()
    var holder_seen = set.empty()
    if child.parent_device_index != null {
      let parent_index = child.parent_device_index ?? -1
      if parent_index < 0 or parent_index >= candidates.len() or parent_index == child_index or candidates[parent_index].name == null {
        return Err(check_failure("candidate block report has an invalid parent index"))
      }
      let parent_name = candidates[parent_index].name ?? ""
      let edge_key = block_edge_key({parent_name: parent_name, child_name: child_name, partition: true})
      if !set.has(candidate_edges, edge_key) {
        candidate_edges = set.add(candidate_edges, edge_key)
        candidate_edge_keys = candidate_edge_keys.push(edge_key)
      }
    }
    for parent_index in child.slave_indices {
      let index_key = f"${parent_index}"
      if parent_index < 0 or parent_index >= candidates.len() or parent_index == child_index or
          candidates[parent_index].name == null or set.has(slave_seen, index_key) {
        return Err(check_failure("candidate block report has an invalid slave index"))
      }
      slave_seen = set.add(slave_seen, index_key)
      let parent_name = candidates[parent_index].name ?? ""
      let edge_key = block_edge_key({parent_name: parent_name, child_name: child_name, partition: false})
      if !set.has(candidate_edges, edge_key) {
        candidate_edges = set.add(candidate_edges, edge_key)
        candidate_edge_keys = candidate_edge_keys.push(edge_key)
      }
    }
    for holder_index in child.holder_indices {
      let index_key = f"${holder_index}"
      if holder_index < 0 or holder_index >= candidates.len() or holder_index == child_index or
          candidates[holder_index].name == null or set.has(holder_seen, index_key) {
        return Err(check_failure("candidate block report has an invalid holder index"))
      }
      holder_seen = set.add(holder_seen, index_key)
      let holder_name = candidates[holder_index].name ?? ""
      let edge_key = block_edge_key({parent_name: child_name, child_name: holder_name, partition: false})
      if !set.has(candidate_edges, edge_key) {
        candidate_edges = set.add(candidate_edges, edge_key)
        candidate_edge_keys = candidate_edge_keys.push(edge_key)
      }
    }
  }
  for candidate_edge in candidate_edge_keys {
    if !set.has(reference_edges, candidate_edge) {
      unexpected_edges += 1
    }
  }
  for edge in reference.edges {
    if !candidate_by_name.has(edge.parent_name) or !candidate_by_name.has(edge.child_name) {
      missing_edges += 1
      continue
    }
    let parent_index = candidate_by_name.get(edge.parent_name)?
    let child_index = candidate_by_name.get(edge.child_name)?
    let parent = candidates[parent_index]
    let child = candidates[child_index]
    let found = if edge.partition {
      child.parent_device_index == parent_index
    } else {
      parent_index in child.slave_indices and child_index in parent.holder_indices
    }
    if found {matched_edges += 1} else {missing_edges += 1}
  }
  return {
    reference_count: reference.devices.len(), candidate_count: candidates.len(), matched_count: matched_count,
    matched_edges: matched_edges, missing_edges: missing_edges, unexpected_edges: unexpected_edges,
    missing_names: missing_names |> sort-by ., unexpected_names: unexpected_names |> sort-by .,
    field_mismatches: field_mismatches |> sort-by .,
    major_minor_mismatches: major_minor_mismatches, size_mismatches: size_mismatches,
    sector_mismatches: sector_mismatches, flag_mismatches: flag_mismatches,
    partition_mismatches: partition_mismatches, candidate_field_missing: candidate_field_missing,
    exact: !candidate_field_missing and missing_names.len() == 0 and unexpected_names.len() == 0 and
      field_mismatches.len() == 0 and missing_edges == 0 and unexpected_edges == 0,
  }
}

## Checks whether two lsblk observations agree on device facts and tree relationships.
export pure block_reference_stable(before: BlockReference, after: BlockReference) -> Bool {
  let before_edges = before.edges |> sort-by { |edge| block_edge_key(edge) }
  let after_edges = after.edges |> sort-by { |edge| block_edge_key(edge) }
  return (before.devices |> sort-by .name) == (after.devices |> sort-by .name) and before_edges == after_edges
}

## Parses the lsblk columns that correspond directly to static queue attributes.
export pure parse_lsblk_queue_json(output: Str) -> Result[List[BlockQueueReference]] {
  let data = json.decode(output)?
  let raw = json.get(data, ["blockdevices"], null)
  if raw == null {return Err(check_failure("lsblk queue reference lacks blockdevices"))}
  var devices: List[BlockQueueReference] = []
  var seen = set.empty()
  for row in raw.require(List[Record])? {
    let name = json.get(row, ["kname"])?.require(Str)?
    let scheduler = json.get(row, ["sched"])?.require(Str?)?
    let read_ahead = json.get(row, ["ra"])?.require(Int?)?
    let discard_granularity = json.get(row, ["disc-gran"])?.require(Int?)?
    let discard_max = json.get(row, ["disc-max"])?.require(Int?)?
    let model = json.get(row, ["model"])?.require(Str?)?
    let revision = json.get(row, ["rev"])?.require(Str?)?
    let safe_read_ahead = read_ahead ?? 0
    let safe_discard_granularity = discard_granularity ?? 0
    let safe_discard_max = discard_max ?? 0
    if name == "" or set.has(seen, name) or
        safe_read_ahead < 0 or safe_read_ahead > 9007199254740991 or
        safe_discard_granularity < 0 or safe_discard_granularity > 9007199254740991 or
        safe_discard_max < 0 or safe_discard_max > 9007199254740991 {
      return Err(check_failure("lsblk queue reference has a duplicate identity or unsafe value"))
    }
    seen = set.add(seen, name)
    devices = devices.push({
      name: name,
      scheduler: if scheduler == null {null} else {scheduler.trim()},
      read_ahead_kb: read_ahead,
      discard_granularity_bytes: discard_granularity,
      discard_max_bytes: discard_max,
      model: if model == null {null} else {model.trim()},
      revision_hint: if revision == null {null} else {revision.trim()},
    })
  }
  return devices
}

## Compares only fields whose lsblk columns have the same units and source meaning.
export pure compare_block_queue_fields(candidate_json: Str, reference: List[BlockQueueReference]) -> Result[BlockQueueFieldComparison] {
  let data = json.decode(candidate_json)?
  let raw = json.get(data, ["storage", "devices"], null)
  if raw == null {
    return {
      reference_count: reference.len(), candidate_count: 0, matched_count: 0,
      missing_names: reference |> map .name |> sort-by ., unexpected_names: [], field_mismatches: [],
      scheduler_mismatches: 0, read_ahead_mismatches: 0, discard_mismatches: 0,
      model_mismatches: 0, candidate_field_missing: true, exact: false,
    }
  }
  let candidates = raw.require(List[CandidateBlockQueueDevice])?
  var reference_by_name: Map[Int] = {}
  for index in range(reference.len()) {
    let name = reference[index].name
    if name == "" or reference_by_name.has(name) {
      return Err(check_failure("queue reference has duplicate or empty identity"))
    }
    reference_by_name = reference_by_name.set(name, index)
  }
  var candidate_seen = set.empty()
  var candidate_field_missing = false
  var matched_count = 0
  var unexpected_names: List[Str] = []
  var field_mismatches: List[Str] = []
  var scheduler_mismatches = 0
  var read_ahead_mismatches = 0
  var discard_mismatches = 0
  var model_mismatches = 0
  for device in candidates {
    if device.name == null or device.name == "" {
      candidate_field_missing = true
      continue
    }
    let name = device.name ?? ""
    if set.has(candidate_seen, name) {
      return Err(check_failure("candidate queue report has duplicate identity"))
    }
    candidate_seen = set.add(candidate_seen, name)
    if !reference_by_name.has(name) {
      unexpected_names = unexpected_names.push(name)
      continue
    }
    matched_count += 1
    let source = reference[reference_by_name.get(name)?]
    let scheduler_bad = device.active_scheduler != source.scheduler
    let read_ahead_bad = device.read_ahead_kb != source.read_ahead_kb
    let discard_bad = device.discard_granularity_bytes != source.discard_granularity_bytes or
      device.discard_max_bytes != source.discard_max_bytes
    let model_bad = if source.model == null {
      device.model.state != "absent" or device.model.value != null
    } else {
      device.model.state != "observed" or device.model.value != source.model
    }
    if scheduler_bad {scheduler_mismatches += 1}
    if read_ahead_bad {read_ahead_mismatches += 1}
    if discard_bad {discard_mismatches += 1}
    if model_bad {model_mismatches += 1}
    if scheduler_bad or read_ahead_bad or discard_bad or model_bad {
      field_mismatches = field_mismatches.push(name)
    }
  }
  var missing_names: List[Str] = []
  for device in reference {
    if !set.has(candidate_seen, device.name) {
      missing_names = missing_names.push(device.name)
    }
  }
  return {
    reference_count: reference.len(), candidate_count: candidates.len(), matched_count: matched_count,
    missing_names: missing_names |> sort-by ., unexpected_names: unexpected_names |> sort-by .,
    field_mismatches: field_mismatches |> sort-by .,
    scheduler_mismatches: scheduler_mismatches, read_ahead_mismatches: read_ahead_mismatches,
    discard_mismatches: discard_mismatches, model_mismatches: model_mismatches,
    candidate_field_missing: candidate_field_missing,
    exact: !candidate_field_missing and missing_names.len() == 0 and unexpected_names.len() == 0 and field_mismatches.len() == 0,
  }
}

## Requires static lsblk queue facts to remain stable around candidate collection.
export pure block_queue_reference_stable(before: List[BlockQueueReference], after: List[BlockQueueReference]) -> Bool {
  return (before |> sort-by .name) == (after |> sort-by .name)
}

pure parse_block_queue_stat(output: Str) -> Result[List[BlockQueueCounter]] {
  let words = output.replace("\t", " ").trim().split(" ") |> where .trim() != ""
  if words.len() not in [11, 15, 17] {
    return Err(check_failure("block stat reference has an unsupported field count"))
  }
  let names = ["read_ios", "read_merges", "read_sectors", "read_ms", "write_ios", "write_merges", "write_sectors", "write_ms", "in_flight", "io_ms", "weighted_io_ms", "discard_ios", "discard_merges", "discard_sectors", "discard_ms", "flush_ios", "flush_ms"]
  let units = ["requests", "requests", "sectors", "milliseconds", "requests", "requests", "sectors", "milliseconds", "requests", "milliseconds", "milliseconds", "requests", "requests", "sectors", "milliseconds", "requests", "milliseconds"]
  var counters: List[BlockQueueCounter] = []
  for index in range(words.len()) {
    let word = words[index]
    if word == "" {
      return Err(check_failure("block stat reference has an empty counter"))
    }
    for character in word.split("") {
      if !"0123456789".contains(character) {
        return Err(check_failure("block stat reference has a nondecimal counter"))
      }
    }
    let value = word.parse_int()?
    if value > 9007199254740991 {
      return Err(check_failure("block stat reference counter exceeds the exact JSON integer range"))
    }
    counters = counters.push({name: names[index], value: value, unit: units[index]})
  }
  return counters
}

proc bounded_block_reference_text(root: FsRoot, source_path: Path) [fs, error] -> Result[Str?] {
  let source = fs.root_read_result(root, source_path, max_bytes: 4096)?
  if source.state == "absent" {return null}
  if source.state != "observed" or source.truncated or source.data == null {
    return Err(check_failure(f"block reference source ${source_path} is incomplete"))
  }
  match (source.data ?? b"").utf8() {
    Ok(value) => return value.trim()
    Err(_) => return Err(check_failure(f"block reference source ${source_path} is not UTF-8"))
  }
}

## Reads allowlisted firmware and stat files for the identities independently reported by lsblk.
export proc read_block_queue_sources(root: FsRoot, queue: List[BlockQueueReference]) [fs, error] -> Result[List[BlockQueueSources]] {
  var sources: List[BlockQueueSources] = []
  var seen = set.empty()
  for device in queue {
    let name = device.name
    if name == "" or name in [".", ".."] or name.contains("/") or set.has(seen, name) {
      return Err(check_failure("block reference has an unsafe or duplicate kernel name"))
    }
    seen = set.add(seen, name)
    let firmware_primary = bounded_block_reference_text(root, fp"sys/class/block/${name}/device/firmware_rev")?
    let firmware = if firmware_primary == null {
      bounded_block_reference_text(root, fp"sys/class/block/${name}/device/rev")?
    } else {
      firmware_primary
    }
    let stat = bounded_block_reference_text(root, fp"sys/class/block/${name}/stat")?
    sources = sources.push({
      name: name, firmware: firmware,
      counters: if stat == null {[]} else {parse_block_queue_stat(stat ?? "")?},
    })
  }
  return sources
}

## Compares monotonic counters inside their source bracket and requires a stable in-flight gauge.
export pure compare_block_queue_sources(candidate_json: Str, before: List[BlockQueueSources], after: List[BlockQueueSources]) -> Result[BlockQueueSourceComparison] {
  let data = json.decode(candidate_json)?
  let raw = json.get(data, ["storage", "devices"], null)
  if raw == null {
    return {
      reference_count: before.len(), candidate_count: 0, matched_count: 0,
      missing_names: before |> map .name |> sort-by ., unexpected_names: [],
      firmware_mismatches: 0, counter_mismatches: 0, unstable: false,
      candidate_field_missing: true, exact: false,
    }
  }
  let candidates = raw.require(List[CandidateQueueSourceDevice])?
  var before_by_name: Map[Int] = {}
  var after_by_name: Map[Int] = {}
  for index in range(before.len()) {
    let name = before[index].name
    if name == "" or before_by_name.has(name) {
      return Err(check_failure("block source reference has duplicate identity"))
    }
    before_by_name = before_by_name.set(name, index)
  }
  for index in range(after.len()) {
    let name = after[index].name
    if name == "" or after_by_name.has(name) {
      return Err(check_failure("block source reference has duplicate identity"))
    }
    after_by_name = after_by_name.set(name, index)
  }
  var unstable = before.len() != after.len()
  for source in before {
    if !after_by_name.has(source.name) {unstable = true}
  }
  var candidate_seen = set.empty()
  var candidate_field_missing = false
  var matched_count = 0
  var unexpected_names: List[Str] = []
  var firmware_mismatches = 0
  var counter_mismatches = 0
  for device in candidates {
    if device.name == null or device.name == "" {
      candidate_field_missing = true
      continue
    }
    let name = device.name ?? ""
    if set.has(candidate_seen, name) {
      return Err(check_failure("candidate block source report has duplicate identity"))
    }
    candidate_seen = set.add(candidate_seen, name)
    if !before_by_name.has(name) {
      unexpected_names = unexpected_names.push(name)
      continue
    }
    matched_count += 1
    let first = before[before_by_name.get(name)?]
    if !after_by_name.has(name) {
      unstable = true
      continue
    }
    let last = after[after_by_name.get(name)?]
    if first.firmware != last.firmware or first.counters.len() != last.counters.len() {
      unstable = true
      continue
    }
    let firmware_bad = if first.firmware == null {
      device.firmware.state != "absent" or device.firmware.value != null
    } else {
      device.firmware.state != "observed" or device.firmware.value != first.firmware
    }
    if firmware_bad {firmware_mismatches += 1}
    var candidate_by_counter: Map[Int] = {}
    for index in range(device.io_counters.len()) {
      let counter = device.io_counters[index]
      if counter.name == "" or candidate_by_counter.has(counter.name) {
        return Err(check_failure("candidate block report has duplicate or empty I/O counter"))
      }
      candidate_by_counter = candidate_by_counter.set(counter.name, index)
    }
    if device.io_counters.len() != first.counters.len() {
      counter_mismatches += 1
    }
    for index in range(first.counters.len()) {
      let earlier = first.counters[index]
      let later = last.counters[index]
      if earlier.name != later.name or earlier.unit != later.unit {
        unstable = true
        continue
      }
      if !candidate_by_counter.has(earlier.name) {
        counter_mismatches += 1
        continue
      }
      let candidate_counter = device.io_counters[candidate_by_counter.get(earlier.name)?]
      if candidate_counter.unit != earlier.unit {
        counter_mismatches += 1
        continue
      }
      if earlier.name == "in_flight" {
        if earlier.value != later.value or candidate_counter.value != earlier.value {
          unstable = true
        }
      } else if later.value < earlier.value {
        unstable = true
      } else if candidate_counter.value < earlier.value or candidate_counter.value > later.value {
        counter_mismatches += 1
      }
    }
  }
  var missing_names: List[Str] = []
  for source in before {
    if !set.has(candidate_seen, source.name) {missing_names = missing_names.push(source.name)}
  }
  return {
    reference_count: before.len(), candidate_count: candidates.len(), matched_count: matched_count,
    missing_names: missing_names |> sort-by ., unexpected_names: unexpected_names |> sort-by .,
    firmware_mismatches: firmware_mismatches, counter_mismatches: counter_mismatches,
    unstable: unstable, candidate_field_missing: candidate_field_missing,
    exact: !unstable and !candidate_field_missing and missing_names.len() == 0 and
      unexpected_names.len() == 0 and firmware_mismatches == 0 and counter_mismatches == 0,
  }
}

pure reference_mount_decimal(value: Str, octal: Bool = false) -> Bool {
  if value == "" {return false}
  let digits = if octal {"01234567"} else {"0123456789"}
  for character in value.split("") {
    if !digits.contains(character) {return false}
  }
  return true
}

pure reference_mount_option_safe(option: Str) -> Bool {
  if option in [
    "ro", "rw", "nosuid", "suid", "nodev", "dev", "noexec", "exec",
    "sync", "async", "dirsync", "relatime", "norelatime", "strictatime",
    "noatime", "nodiratime", "lazytime", "nolazytime", "mand", "nomand",
  ] {return true}
  let parts = option.split("=")
  if parts.len() != 2 {return false}
  let key = parts[0]
  let value = parts[1]
  match key {
    "errors" => return value in ["continue", "remount-ro", "panic"]
    "lowerdir" | "upperdir" | "workdir" => return value.starts_with("/")
    "uid" | "gid" | "rsize" | "wsize" | "size" => return reference_mount_decimal(value)
    "mode" => return reference_mount_decimal(value, true)
    "vers" => {
      let components = value.split(".")
      if components.len() == 0 or components.len() > 3 {return false}
      for component in components {
        if !reference_mount_decimal(component) {return false}
      }
      return true
    }
    _ => return false
  }
}

pure reference_mount_sanitized_options(options: List[Str]) -> List[Str] {
  var output: List[Str] = []
  for option in options {
    output = output.push(if reference_mount_option_safe(option) {option} else {"redacted"})
  }
  return output
}

pure reference_mount_source_sensitive(source: Str) -> Bool {
  let lower = source.lower()
  return source.contains("@") or lower.contains("password=") or lower.contains("token=") or lower.contains("secret=")
}

pure reference_mount_propagation(fields: List[Str]) -> Str {
  var shared = false
  var slave = false
  var unbindable = false
  for field in fields {
    if field == "redacted" {continue}
    if field == "unbindable" {
      unbindable = true
      continue
    }
    let parts = field.split(":")
    if parts.len() != 2 or !reference_mount_decimal(parts[1]) {
      return "unknown"
    }
    match parts[0] {
      "shared" => shared = true
      "master" => slave = true
      "propagate_from" => {}
      _ => return "unknown"
    }
  }
  if unbindable {return "unbindable"}
  if shared and slave {return "shared,slave"}
  if shared {return "shared"}
  if slave {return "slave"}
  return "private"
}

## Parses every flat findmnt row by mount ID, retaining mounts that share a target or source.
export pure parse_findmnt_json(output: Str) -> Result[List[MountReference]] {
  let data = json.decode(output)?
  let raw = json.get(data, ["filesystems"], null)
  if raw == null {return Err(check_failure("findmnt reference lacks filesystems"))}
  let rows = raw.require(List[Record])?
  var mounts: List[MountReference] = []
  var seen = set.empty()
  for row in rows {
    for field in ["id", "parent", "maj:min", "fsroot", "target", "fstype", "source", "vfs-options", "fs-options", "propagation"] {
      if json.get(row, [field], null) == null {
        return Err(check_failure(f"findmnt reference lacks ${field}"))
      }
    }
    let mount_id = json.get(row, ["id"])?.require(Int)?
    let parent_id = json.get(row, ["parent"])?.require(Int)?
    let numbers = parse_major_minor_reference(json.get(row, ["maj:min"])?.require(Str)?, "findmnt")?
    let root = json.get(row, ["fsroot"])?.require(Str)?
    let target = json.get(row, ["target"])?.require(Str)?
    let filesystem = json.get(row, ["fstype"])?.require(Str)?
    let source = json.get(row, ["source"])?.require(Str)?
    let vfs_options = json.get(row, ["vfs-options"])?.require(Str)?
    let fs_options = json.get(row, ["fs-options"])?.require(Str)?
    let propagation = json.get(row, ["propagation"])?.require(Str)?
    let identity = f"${mount_id}"
    if mount_id <= 0 or mount_id > 9007199254740991 or parent_id < 0 or parent_id > 9007199254740991 or
        set.has(seen, identity) or root == "" or !target.starts_with("/") or
        filesystem == "" or source == "" or vfs_options == "" or fs_options == "" or
        propagation not in ["private", "shared", "slave", "shared,slave", "unbindable"] {
      return Err(check_failure("findmnt reference has an invalid or duplicate mount row"))
    }
    seen = set.add(seen, identity)
    mounts = mounts.push({
      mount_id: mount_id, parent_id: parent_id, major: numbers[0], minor: numbers[1],
      root: root, target: target, filesystem: filesystem, source: source,
      mount_options: vfs_options.split(","), super_options: fs_options.split(","),
      propagation: propagation,
    })
  }
  return mounts
}

## Scores mount IDs and fields without using a target or device path as the identity.
export pure compare_mounts(candidate_json: Str, reference: List[MountReference]) -> Result[MountComparison] {
  let data = json.decode(candidate_json)?
  let raw = json.get(data, ["storage", "mounts"], null)
  if raw == null {
    return {
      reference_count: reference.len(), candidate_count: 0, matched_count: 0,
      missing_ids: reference |> map .mount_id |> sort-by ., unexpected_ids: [], field_mismatches: [],
      parent_mismatches: 0, identity_mismatches: 0, path_mismatches: 0,
      filesystem_mismatches: 0, source_mismatches: 0, option_mismatches: 0,
      propagation_mismatches: 0, candidate_field_missing: true, exact: false,
    }
  }
  let candidates = raw.require(List[CandidateMount])?
  var reference_by_id: Map[Int] = {}
  for index in range(reference.len()) {
    let id = reference[index].mount_id
    let key = f"${id}"
    if id <= 0 or reference_by_id.has(key) {return Err(check_failure("mount reference has invalid or duplicate ID"))}
    reference_by_id = reference_by_id.set(key, index)
  }
  var candidate_seen = set.empty()
  var matched_count = 0
  var unexpected_ids: List[Int] = []
  var field_mismatches: List[Int] = []
  var parent_mismatches = 0
  var identity_mismatches = 0
  var path_mismatches = 0
  var filesystem_mismatches = 0
  var source_mismatches = 0
  var option_mismatches = 0
  var propagation_mismatches = 0
  for mount in candidates {
    let key = f"${mount.mount_id}"
    if mount.mount_id <= 0 or set.has(candidate_seen, key) {
      return Err(check_failure("candidate mount report has invalid or duplicate ID"))
    }
    candidate_seen = set.add(candidate_seen, key)
    if !reference_by_id.has(key) {
      unexpected_ids = unexpected_ids.push(mount.mount_id)
      continue
    }
    matched_count += 1
    let source = reference[reference_by_id.get(key)?]
    let parent_bad = mount.parent_id != source.parent_id
    let identity_bad = mount.major != source.major or mount.minor != source.minor
    let path_bad = mount.root.state != "observed" or mount.root.value != source.root or
      mount.target.state != "observed" or mount.target.value != source.target
    let filesystem_bad = mount.filesystem != source.filesystem
    let source_bad = if reference_mount_source_sensitive(source.source) {
      mount.source.state != "redacted" or mount.source.value != null
    } else {
      mount.source.state != "observed" or mount.source.value != source.source
    }
    let option_bad = mount.mount_options != reference_mount_sanitized_options(source.mount_options) or
      mount.super_options != reference_mount_sanitized_options(source.super_options)
    let propagation_bad = reference_mount_propagation(mount.optional_fields) != source.propagation
    if parent_bad {parent_mismatches += 1}
    if identity_bad {identity_mismatches += 1}
    if path_bad {path_mismatches += 1}
    if filesystem_bad {filesystem_mismatches += 1}
    if source_bad {source_mismatches += 1}
    if option_bad {option_mismatches += 1}
    if propagation_bad {propagation_mismatches += 1}
    if parent_bad or identity_bad or path_bad or filesystem_bad or source_bad or option_bad or propagation_bad {
      field_mismatches = field_mismatches.push(mount.mount_id)
    }
  }
  var missing_ids: List[Int] = []
  for mount in reference {
    if !set.has(candidate_seen, f"${mount.mount_id}") {
      missing_ids = missing_ids.push(mount.mount_id)
    }
  }
  return {
    reference_count: reference.len(), candidate_count: candidates.len(), matched_count: matched_count,
    missing_ids: missing_ids |> sort-by ., unexpected_ids: unexpected_ids |> sort-by .,
    field_mismatches: field_mismatches |> sort-by .,
    parent_mismatches: parent_mismatches, identity_mismatches: identity_mismatches,
    path_mismatches: path_mismatches, filesystem_mismatches: filesystem_mismatches,
    source_mismatches: source_mismatches, option_mismatches: option_mismatches,
    propagation_mismatches: propagation_mismatches, candidate_field_missing: false,
    exact: missing_ids.len() == 0 and unexpected_ids.len() == 0 and field_mismatches.len() == 0,
  }
}

## Requires the bracketed mount inventory to keep all IDs, values, and parents stable.
export pure mount_reference_stable(before: List[MountReference], after: List[MountReference]) -> Bool {
  return (before |> sort-by .mount_id) == (after |> sort-by .mount_id)
}

pure mount_usage_local_filesystem(filesystem: Str) -> Bool {
  return filesystem in ["btrfs", "exfat", "ext2", "ext3", "ext4", "f2fs", "ntfs", "ntfs3", "overlay", "tmpfs", "vfat", "xfs"]
}

## Selects mounts whose target cannot resolve through a remote, automount, or shadowed ancestor.
export pure mount_usage_eligible_ids(mounts: List[MountReference]) -> List[Int] {
  var by_id: Map[Int] = {}
  var target_counts: Map[Int] = {}
  for index in range(mounts.len()) {
    let mount = mounts[index]
    by_id = by_id.set(f"${mount.mount_id}", index)
    target_counts = target_counts.set(mount.target, target_counts.get(mount.target, 0) + 1)
  }
  var eligible: List[Int] = []
  for mount in mounts {
    var current_id = mount.mount_id
    var seen = set.empty()
    var safe = true
    var depth = 0
    while depth < mounts.len() {
      let key = f"${current_id}"
      if set.has(seen, key) or !by_id.has(key) {safe = false; break}
      seen = set.add(seen, key)
      let current = mounts[by_id.get(key, -1)]
      if !mount_usage_local_filesystem(current.filesystem) or target_counts.get(current.target, 0) != 1 {
        safe = false
        break
      }
      if current.parent_id == 0 or !by_id.has(f"${current.parent_id}") {break}
      if current.parent_id == current_id {safe = false; break}
      current_id = current.parent_id
      depth += 1
    }
    if depth == mounts.len() {safe = false}
    if safe {eligible = eligible.push(mount.mount_id)}
  }
  return eligible |> sort-by .
}

## Parses a single ID-filtered df observation and rejects missing or unsafe byte counts.
export pure parse_findmnt_usage_json(output: Str, mount_id: Int) -> Result[MountUsageReference] {
  let data = json.decode(output)?
  let raw = json.get(data, ["filesystems"], null)
  if raw == null {return Err(check_failure("findmnt usage reference lacks filesystems"))}
  let rows = raw.require(List[Record])?
  if rows.len() != 1 {return Err(check_failure("findmnt usage reference must contain exactly one mount"))}
  let row = rows[0]
  let id = json.get(row, ["id"])?.require(Int)?
  for field in ["size", "used", "avail"] {
    if !row.has(field) {return Err(check_failure("findmnt usage reference lacks a capacity field"))}
  }
  let total = json.get(row, ["size"], null).require(Int?)?
  let used = json.get(row, ["used"], null).require(Int?)?
  let available = json.get(row, ["avail"], null).require(Int?)?
  let all_unavailable = total == null and used == null and available == null
  let all_observed = total != null and used != null and available != null
  if id != mount_id or id <= 0 or (!all_unavailable and !all_observed) or
      (all_observed and ((total ?? -1) < 0 or (used ?? -1) < 0 or (available ?? -1) < 0 or
      (total ?? -1) > 9007199254740991 or (used ?? -1) > (total ?? -1) or (available ?? -1) > (total ?? -1))) {
    return Err(check_failure("findmnt usage reference has an invalid mount or byte count"))
  }
  return {mount_id: id, total_bytes: total, used_bytes: used, available_bytes: available}
}

pure mount_usage_between(value: Int?, first: Int, last: Int) -> Bool {
  let lower = if first < last {first} else {last}
  let upper = if first > last {first} else {last}
  return value != null and (value ?? -1) >= lower and (value ?? -1) <= upper
}

## Compares all mount IDs and requires explicit skipped states for ineligible targets.
export pure compare_mount_usage(
  candidate_json: Str, mounts: List[MountReference], before: List[MountUsageReference], after: List[MountUsageReference],
) -> Result[MountUsageComparison] {
  let eligible = mount_usage_eligible_ids(mounts)
  var before_by_id: Map[Int] = {}
  var after_by_id: Map[Int] = {}
  for index in range(before.len()) {
    let key = f"${before[index].mount_id}"
    if before_by_id.has(key) {return Err(check_failure("duplicate before mount usage ID"))}
    before_by_id = before_by_id.set(key, index)
  }
  for index in range(after.len()) {
    let key = f"${after[index].mount_id}"
    if after_by_id.has(key) {return Err(check_failure("duplicate after mount usage ID"))}
    after_by_id = after_by_id.set(key, index)
  }
  if before.len() != eligible.len() or after.len() != eligible.len() {
    return Err(check_failure("mount usage reference does not cover every eligible mount"))
  }
  for id in eligible {
    if !before_by_id.has(f"${id}") or !after_by_id.has(f"${id}") {
      return Err(check_failure("mount usage reference lacks an eligible mount"))
    }
  }
  let data = json.decode(candidate_json)?
  let candidates = json.get(data, ["storage", "mounts"])?.require(List[CandidateMountUsage])?
  var expected_ids = set.empty()
  for mount in mounts {expected_ids = set.add(expected_ids, f"${mount.mount_id}")}
  var seen = set.empty()
  var mismatched_ids: List[Int] = []
  var matched_count = 0
  var unstable = false
  for candidate in candidates {
    let key = f"${candidate.mount_id}"
    if !set.has(expected_ids, key) or set.has(seen, key) {
      return Err(check_failure("candidate mount usage has unexpected or duplicate ID"))
    }
    seen = set.add(seen, key)
    var matches = false
    if candidate.mount_id in eligible {
      let first = before[before_by_id.get(key, -1)]
      let last = after[after_by_id.get(key, -1)]
      let first_available = first.total_bytes != null
      let last_available = last.total_bytes != null
      if first_available != last_available or first.total_bytes != last.total_bytes {unstable = true}
      if first_available and last_available {
        matches = candidate.usage_state == "observed" and first.total_bytes == last.total_bytes and
          candidate.usage_total_bytes == first.total_bytes and
          mount_usage_between(candidate.usage_used_bytes, first.used_bytes ?? -1, last.used_bytes ?? -1) and
          mount_usage_between(candidate.usage_available_bytes, first.available_bytes ?? -1, last.available_bytes ?? -1)
      } else {
        matches = candidate.usage_state in ["disappeared", "permission_denied", "read_failure"] and
          candidate.usage_total_bytes == null and candidate.usage_used_bytes == null and candidate.usage_available_bytes == null
      }
    } else {
      matches = candidate.usage_state == "not_requested" and candidate.usage_total_bytes == null and
        candidate.usage_used_bytes == null and candidate.usage_available_bytes == null
    }
    if matches {matched_count += 1} else {mismatched_ids = mismatched_ids.push(candidate.mount_id)}
  }
  for mount in mounts {
    if !set.has(seen, f"${mount.mount_id}") {mismatched_ids = mismatched_ids.push(mount.mount_id)}
  }
  return {
    eligible_count: eligible.len(), skipped_count: mounts.len() - eligible.len(), matched_count: matched_count,
    mismatched_ids: mismatched_ids |> sort-by ., unstable: unstable,
    exact: mismatched_ids.len() == 0 and !unstable,
  }
}

pure reference_module_number(value: Str) -> Result[Int] {
  if value == "" {
    return Err(check_failure("module reference contains an empty number"))
  }
  for character in value.split("") {
    if !"0123456789".contains(character) {
      return Err(check_failure("module reference contains a nondecimal number"))
    }
  }
  let number = value.parse_int()?
  if number > 9007199254740991 {
    return Err(check_failure("module reference number exceeds the exact JSON integer range"))
  }
  return number
}

pure module_reference_words(line: Str) -> List[Str] {
  return line.replace("\t", " ").split(" ") |> where .trim() != ""
}

## Joins lsmod's size and use count with the state exposed by the same procfs snapshot.
export pure parse_lsmod_reference(formatted: Str, raw: Str) -> Result[List[KernelModuleReference]] {
  let lines = formatted.trim().split("\n")
  if lines.len() == 0 or module_reference_words(lines[0]) != ["Module", "Size", "Used", "by"] {
    return Err(check_failure("lsmod reference has an unexpected header"))
  }
  var formatted_by_name: Map[List[Int]] = {}
  var formatted_count = 0
  for line in lines |> drop(1) {
    let words = module_reference_words(line)
    if words.len() < 3 or words[0] == "" or formatted_by_name.has(words[0]) {
      return Err(check_failure("lsmod reference has an ambiguous or duplicate row"))
    }
    let size = reference_module_number(words[1])?
    let users = reference_module_number(words[2])?
    formatted_by_name = formatted_by_name.set(words[0], [size, users])
    formatted_count += 1
  }
  var modules: List[KernelModuleReference] = []
  var raw_seen = set.empty()
  if raw.trim() != "" {
    for line in raw.trim().split("\n") {
      let words = module_reference_words(line)
      if words.len() < 6 or words[0] == "" or words[4] == "" or set.has(raw_seen, words[0]) or
          !formatted_by_name.has(words[0]) {
        return Err(check_failure("proc module reference has an ambiguous, duplicate, or unmatched row"))
      }
      let size = reference_module_number(words[1])?
      let users = reference_module_number(words[2])?
      if formatted_by_name.get(words[0])? != [size, users] {
        return Err(check_failure("lsmod and proc module facts disagree"))
      }
      raw_seen = set.add(raw_seen, words[0])
      modules = modules.push({name: words[0], size_bytes: size, users: users, state: words[4]})
    }
  }
  if modules.len() != formatted_count {
    return Err(check_failure("lsmod and proc module identities disagree"))
  }
  return modules
}

## Compares the complete module set and declared scalar fields by module name.
export pure compare_kernel_modules(candidate_json: Str, reference: List[KernelModuleReference]) -> Result[KernelModuleComparison] {
  let data = json.decode(candidate_json)?
  let raw = json.get(data, ["kernel", "modules"], null)
  if raw == null {
    return {
      reference_count: reference.len(), candidate_count: 0, matched_count: 0,
      missing_names: reference |> map .name |> sort-by ., unexpected_names: [], field_mismatches: [],
      size_mismatches: 0, users_mismatches: 0, state_mismatches: 0,
      candidate_field_missing: true, exact: false,
    }
  }
  let candidates = raw.require(List[CandidateKernelModule])?
  var reference_by_name: Map[Int] = {}
  for index in range(reference.len()) {
    let name = reference[index].name
    if name == "" or reference_by_name.has(name) {
      return Err(check_failure("module reference has duplicate or empty identity"))
    }
    reference_by_name = reference_by_name.set(name, index)
  }
  var candidate_seen = set.empty()
  var matched_count = 0
  var unexpected_names: List[Str] = []
  var field_mismatches: List[Str] = []
  var size_mismatches = 0
  var users_mismatches = 0
  var state_mismatches = 0
  for module_item in candidates {
    if module_item.name == "" or set.has(candidate_seen, module_item.name) {
      return Err(check_failure("candidate module report has duplicate or empty identity"))
    }
    candidate_seen = set.add(candidate_seen, module_item.name)
    if !reference_by_name.has(module_item.name) {
      unexpected_names = unexpected_names.push(module_item.name)
      continue
    }
    matched_count += 1
    let source = reference[reference_by_name.get(module_item.name)?]
    let size_bad = module_item.size_bytes != source.size_bytes
    let users_bad = module_item.users != source.users
    let state_bad = module_item.state != source.state
    if size_bad {size_mismatches += 1}
    if users_bad {users_mismatches += 1}
    if state_bad {state_mismatches += 1}
    if size_bad or users_bad or state_bad {
      field_mismatches = field_mismatches.push(module_item.name)
    }
  }
  var missing_names: List[Str] = []
  for module_item in reference {
    if !set.has(candidate_seen, module_item.name) {
      missing_names = missing_names.push(module_item.name)
    }
  }
  return {
    reference_count: reference.len(), candidate_count: candidates.len(), matched_count: matched_count,
    missing_names: missing_names |> sort-by ., unexpected_names: unexpected_names |> sort-by .,
    field_mismatches: field_mismatches |> sort-by .,
    size_mismatches: size_mismatches, users_mismatches: users_mismatches,
    state_mismatches: state_mismatches, candidate_field_missing: false,
    exact: missing_names.len() == 0 and unexpected_names.len() == 0 and field_mismatches.len() == 0,
  }
}

## Ignores reference row order while requiring every module fact to remain unchanged.
export pure kernel_module_reference_stable(before: List[KernelModuleReference], after: List[KernelModuleReference]) -> Bool {
  return (before |> sort-by .name) == (after |> sort-by .name)
}

## Requires the complete procfs command line in sensitive output and no payload in default output.
export pure compare_kernel_command_line(
  sensitive_json: Str, redacted_json: Str, reference: Bytes,
) -> Result[KernelCommandLineComparison] {
  let sensitive_data = json.decode(sensitive_json)?
  let redacted_data = json.decode(redacted_json)?
  let sensitive_raw = json.get(sensitive_data, ["kernel", "command_line"], null)
  let redacted_raw = json.get(redacted_data, ["kernel", "command_line"], null)
  if sensitive_raw == null or redacted_raw == null {
    return {sensitive_exact: false, redacted_exact: false, candidate_field_missing: true, exact: false}
  }
  let sensitive = sensitive_raw.require(CandidateRawTextObservation)?
  let redacted = redacted_raw.require(CandidateRawTextObservation)?
  let sensitive_exact = match reference.utf8() {
    Ok(text) => sensitive.state == "observed" and sensitive.value == text and sensitive.raw_bytes_base64 == null
    Err(_) => sensitive.state == "malformed" and sensitive.value == null and sensitive.raw_bytes_base64 == reference.base64()
  }
  let redacted_exact = redacted.state == "redacted" and redacted.value == null and redacted.raw_bytes_base64 == null
  return {
    sensitive_exact: sensitive_exact, redacted_exact: redacted_exact,
    candidate_field_missing: false, exact: sensitive_exact and redacted_exact,
  }
}

## Requires the exact allowlisted value set and preserves absent and failed observations.
export pure compare_kernel_parameters(
  candidate_json: Str, reference: List[KernelParameterReference],
) -> Result[KernelParameterComparison] {
  var reference_by_key: Map[Int] = {}
  for index in range(reference.len()) {
    let source = reference[index]
    let key = f"${source.source}:${source.name}"
    let observed = source.state == "observed" and source.value != null and source.raw_bytes_base64 == null
    let absent = source.state == "absent" and source.value == null and source.raw_bytes_base64 == null
    let malformed = source.state == "malformed" and source.value == null and source.raw_bytes_base64 != null
    if source.name == "" or source.source not in ["sysctl", "module"] or reference_by_key.has(key) or
        (!observed and !absent and !malformed) {
      return Err(check_failure("kernel parameter reference has an invalid or duplicate observation"))
    }
    reference_by_key = reference_by_key.set(key, index)
  }
  let data = json.decode(candidate_json)?
  let raw_sysctls = json.get(data, ["kernel", "sysctls"], null)
  let raw_parameters = json.get(data, ["kernel", "parameters"], null)
  if raw_sysctls == null or raw_parameters == null {
    return {
      reference_count: reference.len(), candidate_count: 0, matched_count: 0,
      missing_names: reference |> map .name |> sort-by ., unexpected_names: [], field_mismatches: [],
      candidate_field_missing: true, exact: false,
    }
  }
  let sysctls = raw_sysctls.require(List[CandidateKernelParameter])?
  let parameters = raw_parameters.require(List[CandidateKernelParameter])?
  var candidates: List[CandidateNamedKernelParameter] = []
  for value in sysctls {candidates = candidates.push({source: "sysctl", name: value.name, value: value.value})}
  for value in parameters {candidates = candidates.push({source: "module", name: value.name, value: value.value})}
  var seen = set.empty()
  var matched_count = 0
  var unexpected_names: List[Str] = []
  var field_mismatches: List[Str] = []
  for candidate in candidates {
    let key = f"${candidate.source}:${candidate.name}"
    if candidate.name == "" or set.has(seen, key) {
      return Err(check_failure("candidate kernel parameters contain an empty or duplicate identity"))
    }
    seen = set.add(seen, key)
    if !reference_by_key.has(key) {
      unexpected_names = unexpected_names.push(key)
      continue
    }
    matched_count += 1
    let source = reference[reference_by_key.get(key, -1)]
    if candidate.value.state != source.state or candidate.value.value != source.value or
        candidate.value.raw_bytes_base64 != source.raw_bytes_base64 {
      field_mismatches = field_mismatches.push(key)
    }
  }
  var missing_names: List[Str] = []
  for source in reference {
    let key = f"${source.source}:${source.name}"
    if !set.has(seen, key) {missing_names = missing_names.push(key)}
  }
  return {
    reference_count: reference.len(), candidate_count: candidates.len(), matched_count: matched_count,
    missing_names: missing_names |> sort-by ., unexpected_names: unexpected_names |> sort-by .,
    field_mismatches: field_mismatches |> sort-by ., candidate_field_missing: false,
    exact: missing_names.len() == 0 and unexpected_names.len() == 0 and field_mismatches.len() == 0,
  }
}

## Compares stable kernel identity fields against independently observed uname values.
export pure compare_identity(candidate_json: Str, reference_release: Str, reference_architecture: Str) -> Result[IdentityComparison] {
  let data = json.decode(candidate_json)?
  let release = json.get(data, ["identity", "kernel_release"])?.require(Str?)?
  let architecture = json.get(data, ["identity", "architecture"])?.require(Str?)?
  let release_exact = release != null and release == reference_release
  let architecture_exact = architecture != null and architecture == reference_architecture
  return {
    candidate_release: release,
    candidate_architecture: architecture,
    release_missing: release == null,
    architecture_missing: architecture == null,
    release_exact: release_exact,
    architecture_exact: architecture_exact,
    exact: release_exact and architecture_exact,
  }
}

pure parse_reference_os_release_value(raw: Str) -> Result[Str] {
  let value = raw
  if value == "" {
    return Ok("")
  }
  if value.starts_with("'") {
    if !value.ends_with("'") or value.count_chars() < 2 {
      return Err(check_failure("os-release reference has an unterminated single-quoted value"))
    }
    let content = (value.split("") |> drop(1) |> take(value.count_chars() - 2)).join("")
    if content.contains("'") {
      return Err(check_failure("os-release reference has an unescaped single quote"))
    }
    return Ok(content)
  }
  let quoted = value.starts_with("\"")
  if quoted and (!value.ends_with("\"") or value.count_chars() < 2) {
    return Err(check_failure("os-release reference has an unterminated double-quoted value"))
  }
  if !quoted and (value.ends_with("'") or value.ends_with("\"")) {
    return Err(check_failure("os-release reference has a trailing quote"))
  }
  let content = if quoted {(value.split("") |> drop(1) |> take(value.count_chars() - 2)).join("")} else {value}
  var decoded = ""
  var escaped = false
  for character in content.split("") {
    if escaped {
      if character in ["$", "`", "\"", "\\"] {
        decoded = decoded + character
      } else if !quoted and !"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._-".contains(character) {
        return Err(check_failure("os-release reference has an unescaped special character"))
      } else {
        decoded = decoded + "\\" + character
      }
      escaped = false
    } else if character == "\\" {
      escaped = true
    } else if character in ["$", "`", "\""] or (!quoted and !"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._-".contains(character)) {
      return Err(check_failure("os-release reference has an unescaped special character"))
    } else {
      decoded = decoded + character
    }
  }
  if escaped {
    return Err(check_failure("os-release reference ends with an escape"))
  }
  return Ok(decoded)
}

pure reference_os_release_id(value: Str) -> Bool {
  if value == "" {return false}
  for character in value.split("") {
    if !"0123456789abcdefghijklmnopqrstuvwxyz._-".contains(character) {return false}
  }
  return true
}

## Parses ID and VERSION_ID as data; neither variable nor command syntax is evaluated.
export pure parse_reference_os_release(source: Str) -> Result[OsReleaseReference] {
  var id: Str? = null
  var version_id: Str? = null
  for line in source.lines() {
    let trimmed = line.trim()
    if trimmed == "" or trimmed.starts_with("#") {
      continue
    }
    let assignment = line.split("=", maxsplit: 1)
    if assignment.len() != 2 {
      return Err(check_failure("os-release reference has a line without an assignment"))
    }
    let key = assignment[0]
    if key == "" or !"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_".contains(key.split("")[0]) {
      return Err(check_failure("os-release reference has an invalid assignment key"))
    }
    for character in key.split("") {
      if !"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_0123456789".contains(character) {
        return Err(check_failure("os-release reference has an invalid assignment key"))
      }
    }
    if key == "ID" {
      let value = parse_reference_os_release_value(assignment[1])?
      if !reference_os_release_id(value) {
        return Err(check_failure("os-release reference has an invalid ID"))
      }
      id = value
    } else if key == "VERSION_ID" {
      version_id = parse_reference_os_release_value(assignment[1])?
    }
  }
  if id == null or (id ?? "") == "" {
    return Err(check_failure("os-release reference has no ID"))
  }
  return {id: id ?? "", version_id: version_id}
}

## Compares candidate OS identity to the decoded source values, including absent versions.
export pure compare_os_release(candidate_json: Str, reference: OsReleaseReference) -> Result[OsReleaseComparison] {
  let data = json.decode(candidate_json)?
  let candidate_id = json.get(data, ["identity", "os_release", "id"], null).require(Str?)?
  let candidate_version_id = json.get(data, ["identity", "os_release", "version_id"], null).require(Str?)?
  let id_exact = candidate_id != null and candidate_id == reference.id
  let version_id_exact = candidate_version_id == reference.version_id
  return {
    id_missing: candidate_id == null,
    version_id_missing: candidate_version_id == null and reference.version_id != null,
    id_exact: id_exact,
    version_id_exact: version_id_exact,
    exact: id_exact and version_id_exact,
  }
}

## Decodes fixed-width od output and rejects non-byte tokens and oversized sources.
export pure parse_reference_od_bytes(output: Str, max_bytes: Int) -> Result[Bytes] {
  if max_bytes <= 0 {
    return Err(check_failure("device-tree reference has an invalid byte bound"))
  }
  let tokens = output.replace("\n", " ").replace("\t", " ").split(" ") |> where .trim() != ""
  if tokens.len() > max_bytes {
    return Err(check_failure("device-tree reference exceeds its byte bound"))
  }
  var values: List[Int] = []
  for token in tokens {
    if token.count_chars() != 2 {
      return Err(check_failure("device-tree od reference has a non-byte token"))
    }
    for character in token.split("") {
      if character not in ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f", "A", "B", "C", "D", "E", "F"] {
        return Err(check_failure("device-tree od reference has a non-hex byte"))
      }
    }
    values = values.push(f"0x${token}".parse_int()?)
  }
  return bytes.from_ints(values)?
}

pure parse_reference_device_tree_strings(raw: Bytes) -> Result[List[Str]] {
  var values: List[Str] = []
  var start = 0
  var index = 0
  while index < raw.len() {
    if raw.byte_at(index) == 0 {
      if index == start {
        return Err(check_failure("device-tree reference has an empty string"))
      }
      let decoded = raw.slice(start, index - start).utf8()
      match decoded {
        Ok(_) => {}
        Err(_) => return Err(check_failure("device-tree reference contains invalid UTF-8"))
      }
      values = values.push(decoded?)
      start = index + 1
    }
    index += 1
  }
  if values.len() == 0 or start != raw.len() {
    return Err(check_failure("device-tree reference has an unterminated string"))
  }
  return Ok(values)
}

## Parses a single model and ordered compatible values without using the collector decoder.
export pure parse_reference_device_tree(model_raw: Bytes?, compatible_raw: Bytes?) -> Result[DeviceTreeReference] {
  if model_raw == null and compatible_raw == null {
    return Err(check_failure("device-tree reference has no source files"))
  }
  var model: Str? = null
  if model_raw != null {
    let values = parse_reference_device_tree_strings(model_raw)?
    if values.len() != 1 {
      return Err(check_failure("device-tree model reference has more than one string"))
    }
    model = values[0]
  }
  var compatible: List[Str] = []
  if compatible_raw != null {
    compatible = parse_reference_device_tree_strings(compatible_raw)?
  }
  return {model: model, compatible: compatible}
}

## Compares available device-tree values in source order and detects malformed candidates.
export pure compare_device_tree(candidate_json: Str, reference: DeviceTreeReference) -> Result[DeviceTreeComparison] {
  let data = json.decode(candidate_json)?
  let candidate_source = json.get(data, ["identity", "firmware", "source"], null).require(Str?)?
  let model_state = json.get(data, ["identity", "firmware", "device_tree_model", "state"], null).require(Str?)?
  let model_value = json.get(data, ["identity", "firmware", "device_tree_model", "value"], null).require(Str?)?
  let candidate_compatible = json.get(data, ["identity", "firmware", "device_tree_compatible"], []).require(List[Record])?
  let source_exact = candidate_source == "device-tree"
  let model_exact = if reference.model == null {model_state == "absent" and model_value == null} else {model_state == "observed" and model_value == reference.model}
  var compatible_exact = candidate_compatible.len() == reference.compatible.len()
  if compatible_exact {
    var index = 0
    while index < reference.compatible.len() {
      let row = candidate_compatible[index]
      let state = json.get(row, ["state"], null).require(Str?)?
      let value = json.get(row, ["value"], null).require(Str?)?
      if state != "observed" or value != reference.compatible[index] {
        compatible_exact = false
      }
      index += 1
    }
  }
  let candidate_issues = json.get(data, ["issues"], []).require(List[Record])?
  for candidate_issue in candidate_issues {
    let field = json.get(candidate_issue, ["field"], null).require(Str?)?
    if field == "firmware.device_tree_compatible" {
      compatible_exact = false
    }
  }
  return {
    source_exact: source_exact,
    model_exact: model_exact,
    compatible_exact: compatible_exact,
    exact: source_exact and model_exact and compatible_exact,
  }
}

## Parses the uptime gauge from the kernel's two-column decimal source.
export pure parse_reference_uptime_seconds(output: Str) -> Result[Int] {
  let columns = output.trim().split(" ")
  if columns.len() < 2 {
    return Err(check_failure("/proc/uptime is missing its two decimal columns"))
  }
  let parts = columns[0].split(".")
  if parts.len() != 2 {
    return Err(check_failure("/proc/uptime has an invalid uptime decimal"))
  }
  let seconds = reference_cpu_number(parts[0])?
  let _ = reference_cpu_number(parts[1])?
  let idle_parts = columns[1].split(".")
  if idle_parts.len() != 2 {
    return Err(check_failure("/proc/uptime has an invalid idle decimal"))
  }
  let _ = reference_cpu_number(idle_parts[0])?
  let _ = reference_cpu_number(idle_parts[1])?
  return Ok(seconds)
}

pure process_reference_number(value: Str) -> Result[Int] {
  if value == "" {
    return Err(check_failure("process reference contains an empty number"))
  }
  for character in value.split("") {
    if !"0123456789".contains(character) {
      return Err(check_failure("process reference contains a nondecimal number"))
    }
  }
  var number = 0
  match value.parse_int() {
    Ok(parsed) => number = parsed
    Err(_) => return Err(check_failure("process reference number cannot be represented"))
  }
  if number > 9007199254740991 {
    return Err(check_failure("process reference number exceeds the exact JSON integer range"))
  }
  return Ok(number)
}

pure process_reference_stat_fields(output: Str) -> Result[ProcStatReferenceFields] {
  let leading = output.trim().split(" (", maxsplit: 1)
  if leading.len() != 2 {
    return Err(check_failure("process stat reference has no command opener"))
  }
  let pid = process_reference_number(leading[0])?
  let command_and_fields = leading[1].split(") ")
  if command_and_fields.len() < 2 {
    return Err(check_failure("process stat reference has no command terminator"))
  }
  let command = (command_and_fields |> take(command_and_fields.len() - 1)).join(") ")
  let fields = command_and_fields[command_and_fields.len() - 1].replace("\t", " ").split(" ") |> where .trim() != ""
  if pid == 0 or fields.len() < 22 or fields[0].count_chars() != 1 {
    return Err(check_failure("process stat reference has invalid identity or field count"))
  }
  let parent_pid = process_reference_number(fields[1])?
  let start_ticks = process_reference_number(fields[19])?
  return Ok({
    identity: {pid: pid, parent_pid: parent_pid, command: command, state: fields[0], start_ticks: start_ticks},
    fields: fields,
  })
}

## Parses mandatory procfs stat identity even when optional resource counters are unsafe.
export pure parse_proc_stat_identity_reference(output: Str) -> Result[ProcStatIdentityReference] {
  return Ok(process_reference_stat_fields(output)?.identity)
}

## Parses threads without requiring the stat virtual and resident counters to fit JSON.
export pure parse_proc_stat_thread_reference(output: Str) -> Result[ProcStatThreadReference] {
  let parsed = process_reference_stat_fields(output)?
  let thread_count = process_reference_number(parsed.fields[17])?
  if thread_count == 0 {
    return Err(check_failure("process stat reference has no threads"))
  }
  return Ok({pid: parsed.identity.pid, start_ticks: parsed.identity.start_ticks, thread_count: thread_count})
}

## Reads the real numeric UID from the one complete status row.
export pure parse_proc_status_uid_reference(output: Str) -> Result[Int] {
  var found: Int? = null
  for line in output.lines() {
    if !line.starts_with("Uid:") {
      continue
    }
    if found != null {
      return Err(check_failure("process status reference has duplicate UID rows"))
    }
    let columns = line.split(":", maxsplit: 1).get(1, "").replace("\t", " ").split(" ") |> where .trim() != ""
    if columns.len() != 4 {
      return Err(check_failure("process status reference has an incomplete UID row"))
    }
    var uids: List[Int] = []
    for column in columns {
      uids = uids.push(process_reference_number(column)?)
    }
    found = uids[0]
  }
  if found == null {
    return Err(check_failure("process status reference has no UID row"))
  }
  return Ok(found ?? 0)
}

## Converts the two required statm gauges using the process-visible page size.
export pure parse_proc_statm_reference(output: Str, page_size_bytes: Int) -> Result[ProcStatmReference] {
  if page_size_bytes <= 0 or page_size_bytes > 9007199254740991 {
    return Err(check_failure("process page size is outside the exact byte range"))
  }
  let fields = output.trim().replace("\t", " ").split(" ") |> where .trim() != ""
  if fields.len() != 7 {
    return Err(check_failure("process statm reference does not have seven fields"))
  }
  for field in fields |> drop(2) {
    if field == "" {
      return Err(check_failure("process statm reference has an empty field"))
    }
    for character in field.split("") {
      if !"0123456789".contains(character) {
        return Err(check_failure("process statm reference has a nondecimal field"))
      }
    }
  }
  let virtual_pages = process_reference_number(fields[0])?
  let resident_pages = process_reference_number(fields[1])?
  if virtual_pages > 9007199254740991 / page_size_bytes or
      resident_pages > 9007199254740991 / page_size_bytes {
    return Err(check_failure("process statm reference exceeds the exact byte range"))
  }
  return Ok({
    virtual_bytes: virtual_pages * page_size_bytes,
    resident_bytes: resident_pages * page_size_bytes,
  })
}

## Selects the unified cgroup path without treating v1 membership as a v2 path.
export pure parse_proc_cgroup_reference(output: Str) -> Result[Str?] {
  if output == "" {
    return Err(check_failure("process cgroup reference is empty"))
  }
  var unified_path: Str? = null
  for line in output.lines() {
    let fields = line.split(":", maxsplit: 2)
    if fields.len() != 3 {
      return Err(check_failure("process cgroup reference has an invalid row"))
    }
    let hierarchy = process_reference_number(fields[0])?
    if !fields[2].starts_with("/") or (hierarchy == 0 and fields[1] != "") or
        (hierarchy != 0 and fields[1] == "") {
      return Err(check_failure("process cgroup reference has an invalid hierarchy or path"))
    }
    if hierarchy == 0 {
      if unified_path != null {
        return Err(check_failure("process cgroup reference has duplicate unified rows"))
      }
      unified_path = fields[2]
    }
  }
  return Ok(unified_path)
}

proc process_reference_text(root: FsRoot, source_path: Path, max_bytes: Int) [fs, error] -> Result[Str?] {
  let source = fs.root_read_result(root, source_path, max_bytes: max_bytes)?
  if source.state != "observed" or source.truncated or source.data == null {
    return Ok(null)
  }
  match (source.data ?? b"").utf8() {
    Ok(value) => return Ok(value)
    Err(_) => return Ok(null)
  }
}

## Captures only complete per-process sources whose PID/start identity survives both stat reads.
export proc read_process_identity_snapshot(root: FsRoot) [fs, error] -> Result[ProcessIdentitySnapshot] {
  let listing = fs.root_children(root, p"proc", max_entries: 8192)?
  if listing.state != "complete" {
    return Err(check_failure("process reference enumeration is incomplete"))
  }
  var processes: List[ProcessIdentityReference] = []
  var skipped_count = 0
  for process_path in listing.children {
    let pid_text = process_path.name()
    if pid_text == "" {
      continue
    }
    var decimal = true
    for character in pid_text.split("") {
      if !"0123456789".contains(character) { decimal = false }
    }
    if !decimal {
      continue
    }
    let parsed_pid = process_reference_number(pid_text)
    guard let pid = parsed_pid else |_| {
      skipped_count += 1
      continue
    }
    if pid == 0 {
      skipped_count += 1
      continue
    }
    let first_source = process_reference_text(root, fp"${process_path}/stat", 16384)?
    if first_source == null {
      skipped_count += 1
      continue
    }
    guard let first = parse_proc_stat_identity_reference(first_source ?? "") else |_| {
      skipped_count += 1
      continue
    }
    let status_source = process_reference_text(root, fp"${process_path}/status", 16384)?
    if status_source == null {
      skipped_count += 1
      continue
    }
    guard let uid = parse_proc_status_uid_reference(status_source ?? "") else |_| {
      skipped_count += 1
      continue
    }
    let last_source = process_reference_text(root, fp"${process_path}/stat", 16384)?
    if last_source == null {
      skipped_count += 1
      continue
    }
    guard let last = parse_proc_stat_identity_reference(last_source ?? "") else |_| {
      skipped_count += 1
      continue
    }
    if first.pid != pid or last.pid != pid or first.start_ticks != last.start_ticks or
        first.parent_pid != last.parent_pid or first.command != last.command {
      skipped_count += 1
      continue
    }
    processes = processes.push({
      pid: first.pid, start_ticks: first.start_ticks, parent_pid: first.parent_pid,
      uid: uid, command: first.command, state: first.state,
    })
  }
  return Ok({processes: processes |> sort-by .pid, skipped_count: skipped_count})
}

## Reads stat, statm, and cgroup inside a per-PID start-identity bracket.
export proc read_process_resource_snapshot(root: FsRoot, page_size_bytes: Int) [fs, error] -> Result[ProcessResourceSnapshot] {
  if page_size_bytes <= 0 or page_size_bytes > 9007199254740991 {
    return Err(check_failure("process resource reference has an invalid page size"))
  }
  let listing = fs.root_children(root, p"proc", max_entries: 8192)?
  if listing.state != "complete" {
    return Err(check_failure("process resource reference enumeration is incomplete"))
  }
  var processes: List[ProcessResourceReference] = []
  var skipped_count = 0
  for process_path in listing.children {
    let pid_text = process_path.name()
    if pid_text == "" {
      continue
    }
    var decimal = true
    for character in pid_text.split("") {
      if !"0123456789".contains(character) { decimal = false }
    }
    if !decimal {
      continue
    }
    guard let pid = process_reference_number(pid_text) else |_| {
      skipped_count += 1
      continue
    }
    if pid == 0 {
      skipped_count += 1
      continue
    }
    let first_source = process_reference_text(root, fp"${process_path}/stat", 16384)?
    if first_source == null {
      skipped_count += 1
      continue
    }
    guard let first = parse_proc_stat_thread_reference(first_source ?? "") else |_| {
      skipped_count += 1
      continue
    }
    let statm_source = process_reference_text(root, fp"${process_path}/statm", 4096)?
    let cgroup_source = process_reference_text(root, fp"${process_path}/cgroup", 16384)?
    if statm_source == null or cgroup_source == null {
      skipped_count += 1
      continue
    }
    guard let memory = parse_proc_statm_reference(statm_source ?? "", page_size_bytes) else |_| {
      skipped_count += 1
      continue
    }
    guard let cgroup = parse_proc_cgroup_reference(cgroup_source ?? "") else |_| {
      skipped_count += 1
      continue
    }
    let last_source = process_reference_text(root, fp"${process_path}/stat", 16384)?
    if last_source == null {
      skipped_count += 1
      continue
    }
    guard let last = parse_proc_stat_thread_reference(last_source ?? "") else |_| {
      skipped_count += 1
      continue
    }
    if first.pid != pid or last.pid != pid or first.start_ticks != last.start_ticks or
        first.thread_count != last.thread_count {
      skipped_count += 1
      continue
    }
    processes = processes.push({
      pid: pid, start_ticks: first.start_ticks, thread_count: first.thread_count,
      resident_bytes: memory.resident_bytes, virtual_bytes: memory.virtual_bytes,
      cgroup: cgroup,
    })
  }
  return Ok({processes: processes |> sort-by .pid, skipped_count: skipped_count})
}

## Counts only identities that survive the reference bracket with unchanged static fields.
export pure compare_process_identity(
  candidate_json: Str, before: List[ProcessIdentityReference], after: List[ProcessIdentityReference],
) -> Result[ProcessIdentityComparison] {
  let data = json.decode(candidate_json)?
  let candidates = json.get(data, ["processes", "processes"])?.require(List[CandidateProcessIdentity])?
  var before_by_pid: Map[Int] = {}
  var after_by_pid: Map[Int] = {}
  var candidate_by_pid: Map[Int] = {}
  for index in range(before.len()) {
    let item = before[index]
    let key = f"${item.pid}"
    if item.pid <= 0 or item.pid > 9007199254740991 or item.parent_pid < 0 or
        item.parent_pid > 9007199254740991 or item.start_ticks < 0 or
        item.start_ticks > 9007199254740991 or item.uid < 0 or item.uid > 9007199254740991 or
        before_by_pid.has(key) {
      return Err(check_failure("before process reference contains an invalid or duplicate identity"))
    }
    before_by_pid = before_by_pid.set(key, index)
  }
  for index in range(after.len()) {
    let item = after[index]
    let key = f"${item.pid}"
    if item.pid <= 0 or item.pid > 9007199254740991 or item.parent_pid < 0 or
        item.parent_pid > 9007199254740991 or item.start_ticks < 0 or
        item.start_ticks > 9007199254740991 or item.uid < 0 or item.uid > 9007199254740991 or
        after_by_pid.has(key) {
      return Err(check_failure("after process reference contains an invalid or duplicate identity"))
    }
    after_by_pid = after_by_pid.set(key, index)
  }
  for index in range(candidates.len()) {
    let key = f"${candidates[index].pid}"
    if candidates[index].pid <= 0 or candidate_by_pid.has(key) {
      return Err(check_failure("candidate process report contains an invalid or duplicate PID"))
    }
    candidate_by_pid = candidate_by_pid.set(key, index)
  }
  var stable_count = 0
  var unstable_count = 0
  var matched_count = 0
  var missing_pids: List[Int] = []
  var mismatched_pids: List[Int] = []
  for first in before {
    let key = f"${first.pid}"
    if !after_by_pid.has(key) {
      unstable_count += 1
      continue
    }
    let last = after[after_by_pid.get(key, -1)]
    if first.start_ticks != last.start_ticks or first.parent_pid != last.parent_pid or
        first.uid != last.uid or first.command != last.command {
      unstable_count += 1
      continue
    }
    stable_count += 1
    if !candidate_by_pid.has(key) {
      missing_pids = missing_pids.push(first.pid)
      continue
    }
    let candidate = candidates[candidate_by_pid.get(key, -1)]
    if candidate.start_ticks != first.start_ticks {
      missing_pids = missing_pids.push(first.pid)
      continue
    }
    matched_count += 1
    if candidate.parent_pid != first.parent_pid or candidate.uid != first.uid or
        candidate.command.state != "observed" or candidate.command.value != first.command {
      mismatched_pids = mismatched_pids.push(first.pid)
    }
  }
  return Ok({
    stable_count: stable_count, unstable_count: unstable_count, candidate_count: candidates.len(),
    matched_count: matched_count, missing_pids: missing_pids |> sort-by .,
    mismatched_pids: mismatched_pids |> sort-by ., state_unscored_count: stable_count,
    exact_static: stable_count > 0 and missing_pids.len() == 0 and mismatched_pids.len() == 0,
  })
}

pure valid_process_resource_reference(item: ProcessResourceReference) -> Bool {
  return item.pid > 0 and item.pid <= 9007199254740991 and
    item.start_ticks >= 0 and item.start_ticks <= 9007199254740991 and
    item.thread_count > 0 and item.thread_count <= 9007199254740991 and
    item.resident_bytes >= 0 and item.resident_bytes <= 9007199254740991 and
    item.virtual_bytes >= 0 and item.virtual_bytes <= 9007199254740991 and
    (item.cgroup == null or (item.cgroup ?? "").starts_with("/"))
}

## Compares fields only when both raw references agree for the same PID/start identity.
export pure compare_process_resources(
  candidate_json: Str, before: List[ProcessResourceReference], after: List[ProcessResourceReference],
) -> Result[ProcessResourceComparison] {
  let data = json.decode(candidate_json)?
  let candidates = json.get(data, ["processes", "processes"])?.require(List[CandidateProcessResource])?
  var before_by_pid: Map[Int] = {}
  var after_by_pid: Map[Int] = {}
  var candidate_by_pid: Map[Int] = {}
  for index in range(before.len()) {
    let item = before[index]
    let key = f"${item.pid}"
    if !valid_process_resource_reference(item) or before_by_pid.has(key) {
      return Err(check_failure("before process resource reference has an invalid or duplicate identity"))
    }
    before_by_pid = before_by_pid.set(key, index)
  }
  for index in range(after.len()) {
    let item = after[index]
    let key = f"${item.pid}"
    if !valid_process_resource_reference(item) or after_by_pid.has(key) {
      return Err(check_failure("after process resource reference has an invalid or duplicate identity"))
    }
    after_by_pid = after_by_pid.set(key, index)
  }
  for index in range(candidates.len()) {
    let key = f"${candidates[index].pid}"
    if candidates[index].pid <= 0 or candidate_by_pid.has(key) {
      return Err(check_failure("candidate process resource report has an invalid or duplicate PID"))
    }
    candidate_by_pid = candidate_by_pid.set(key, index)
  }
  var stable_count = 0
  var unstable_count = 0
  var matched_count = 0
  var scored_fields = 0
  var missing_pids: List[Int] = []
  var unscored_fields: List[Str] = []
  var mismatched_fields: List[Str] = []
  for first in before {
    let key = f"${first.pid}"
    if !after_by_pid.has(key) or after[after_by_pid.get(key, -1)].start_ticks != first.start_ticks {
      unstable_count += 1
      continue
    }
    stable_count += 1
    if !candidate_by_pid.has(key) or candidates[candidate_by_pid.get(key, -1)].start_ticks != first.start_ticks {
      missing_pids = missing_pids.push(first.pid)
      continue
    }
    let last = after[after_by_pid.get(key, -1)]
    let candidate = candidates[candidate_by_pid.get(key, -1)]
    matched_count += 1
    if first.thread_count == last.thread_count {
      scored_fields += 1
      if candidate.thread_count != first.thread_count {
        mismatched_fields = mismatched_fields.push(f"${first.pid}.thread_count")
      }
    } else {
      unscored_fields = unscored_fields.push(f"${first.pid}.thread_count")
    }
    if first.resident_bytes == last.resident_bytes {
      scored_fields += 1
      if candidate.resident_bytes != first.resident_bytes {
        mismatched_fields = mismatched_fields.push(f"${first.pid}.resident_bytes")
      }
    } else {
      unscored_fields = unscored_fields.push(f"${first.pid}.resident_bytes")
    }
    if first.virtual_bytes == last.virtual_bytes {
      scored_fields += 1
      if candidate.virtual_bytes != first.virtual_bytes {
        mismatched_fields = mismatched_fields.push(f"${first.pid}.virtual_bytes")
      }
    } else {
      unscored_fields = unscored_fields.push(f"${first.pid}.virtual_bytes")
    }
    if first.cgroup == last.cgroup {
      scored_fields += 1
      let expected_state = if first.cgroup == null {"unsupported"} else {"observed"}
      if candidate.cgroup.state != expected_state or candidate.cgroup.value != first.cgroup {
        mismatched_fields = mismatched_fields.push(f"${first.pid}.cgroup")
      }
    } else {
      unscored_fields = unscored_fields.push(f"${first.pid}.cgroup")
    }
  }
  return Ok({
    stable_count: stable_count, unstable_count: unstable_count,
    candidate_count: candidates.len(), matched_count: matched_count,
    scored_fields: scored_fields, missing_pids: missing_pids |> sort-by .,
    unscored_fields: unscored_fields |> sort-by .,
    mismatched_fields: mismatched_fields |> sort-by .,
    exact_scored: scored_fields > 0 and missing_pids.len() == 0 and mismatched_fields.len() == 0,
  })
}

## Preserves a missing candidate uptime separately from a value outside its source bracket.
export pure compare_uptime(candidate_json: Str, before_seconds: Int, after_seconds: Int) -> Result[UptimeComparison] {
  if before_seconds < 0 or after_seconds < before_seconds {
    return Err(check_failure("reference uptime bracket is invalid"))
  }
  let data = json.decode(candidate_json)?
  let candidate_seconds = json.get(data, ["identity", "uptime_seconds"])?.require(Int?)?
  let observed_seconds = candidate_seconds ?? -1
  let bracketed = candidate_seconds != null and observed_seconds >= before_seconds and observed_seconds <= after_seconds
  return {
    candidate_seconds: candidate_seconds,
    before_seconds: before_seconds,
    after_seconds: after_seconds,
    candidate_missing: candidate_seconds == null,
    bracketed: bracketed,
  }
}

## Compares scoped namespace symlink identities against independent readlink results.
export pure compare_namespace_scope(candidate_json: Str, references: List[NamespaceReference]) -> Result[NamespaceComparison] {
  if references.len() == 0 {
    return Err(check_failure("namespace comparison has no reference identities"))
  }
  let data = json.decode(candidate_json)?
  var seen: List[Str] = []
  var matched_count = 0
  var missing_fields: List[Str] = []
  var mismatched_fields: List[Str] = []
  for reference in references {
    if reference.field == "" or reference.target == "" or reference.field in seen {
      return Err(check_failure("namespace reference has an empty or duplicate identity"))
    }
    seen = seen.push(reference.field)
    let state = json.get(data, ["scope", reference.field, "state"], null).require(Str?)?
    let value = json.get(data, ["scope", reference.field, "value"], null).require(Str?)?
    if state != "observed" or value == null {
      missing_fields = missing_fields.push(reference.field)
    } else if value == reference.target {
      matched_count += 1
    } else {
      mismatched_fields = mismatched_fields.push(reference.field)
    }
  }
  return {
    reference_count: references.len(),
    matched_count: matched_count,
    missing_fields: missing_fields,
    mismatched_fields: mismatched_fields,
    exact: missing_fields.len() == 0 and mismatched_fields.len() == 0,
  }
}

## Keeps live differential scores tied to a real Linux collection mode.
export pure require_live_linux_report(candidate_json: Str) -> Result[Unit] {
  let data = json.decode(candidate_json)?
  let source_mode = json.get(data, ["source_mode"])?.require(Str?)?
  if source_mode != "live_linux" {
    return Err(check_failure("candidate is not a live Linux report"))
  }
  return Ok()
}

pure trace_audit_argv(binary: Str, trace_file: Str, xsh_bin: Str, script: Str, applet_args: List[Str]) -> List[Str] {
  return [
    binary, "-f", "-qq", "-s", "4096", "-e",
    "trace=process,network,file,init_module,finit_module,delete_module,swapon,swapoff,write,writev,pwrite64,pwritev,pwritev2",
    "-o", trace_file, "--", xsh_bin, script, "--",
  ].extend(applet_args)
}

## Rejects a manifest whose expected denominator can be accidentally reduced.
export pure validate(manifest: CoverageManifest) -> Result[Unit] {
  if manifest.schema_version != 4 {
    return Err(check_failure("unsupported coverage manifest schema"))
  }

  if manifest.producer != "system-report" or manifest.assertions.len() == 0 {
    return Err(check_failure("coverage manifest has no producer or assertions"))
  }

  var seen: List[Str] = []
  var has_mandatory = false
  for assertion in manifest.assertions {
    if assertion.id.trim() == "" or assertion.domain.trim() == "" or assertion.field.trim() == "" {
      return Err(check_failure("coverage assertion has an empty identity or field"))
    }
    if assertion.tier != "mandatory" and assertion.tier != "supplemental" {
      return Err(check_failure(f"unsupported tier for '${assertion.id}'"))
    }
    if assertion.tier == "mandatory" {
      has_mandatory = true
    }
    let rooted_reference = assertion.reference_adapter == "procfs-rooted-v1"
    if assertion.source_abi.trim() == "" or assertion.reference_adapter.trim() == "" or
        (assertion.reference_commands.len() == 0 and !rooted_reference) or
        (rooted_reference and (assertion.domain != "process" or assertion.reference_commands.len() != 0)) {
      return Err(check_failure(f"assertion '${assertion.id}' has an invalid source ABI or reference invocation"))
    }
    var seen_commands: List[List[Str]] = []
    for command in assertion.reference_commands {
      if command.len() == 0 or command[0].trim() == "" or command in seen_commands {
        return Err(check_failure(f"assertion '${assertion.id}' has an empty or duplicate reference command"))
      }
      for argument in command {
        if argument == "" {
          return Err(check_failure(f"assertion '${assertion.id}' has an empty reference argument"))
        }
      }
      seen_commands = seen_commands.push(command)
    }
    if assertion.id == "network.routes" and assertion.reference_commands != [network_route_argv("ipv4"), network_route_argv("ipv6")] or
        assertion.id == "network.rules" and assertion.reference_commands != [network_rule_argv("ipv4"), network_rule_argv("ipv6")] or
        assertion.id in ["pci.identity", "pci.binding"] and
          (assertion.reference_adapter != "lspci" or assertion.reference_commands != [pci_reference_argv()]) or
        assertion.id in ["privacy.no-process-data", "safety.no-child", "safety.no-mutation"] and
          (assertion.reference_adapter != "strace" or assertion.reference_commands != [trace_audit_argv("strace", "<trace file>", "<xsh binary>", "<system-report script>", ["--json"])]) {
      return Err(check_failure(f"assertion '${assertion.id}' must declare the executed reference commands"))
    }
    if assertion.eligibility.trim() == "" or assertion.equality_rule.trim() == "" or assertion.fixture_scenarios.len() == 0 {
      return Err(check_failure(f"assertion '${assertion.id}' is missing eligibility, comparison, or fixture policy"))
    }
    if assertion.id in seen {
      return Err(check_failure(f"duplicate assertion id '${assertion.id}'"))
    }
    seen = seen.push(assertion.id)
  }

  if ! has_mandatory {
    return Err(check_failure("coverage manifest declares no mandatory assertions"))
  }

  var seen_scenarios: List[Str] = []
  for scenario in manifest.fixture_scenarios {
    if scenario.trim() == "" {
      return Err(check_failure("coverage manifest has an empty fixture scenario"))
    }
    if scenario in seen_scenarios {
      return Err(check_failure(f"duplicate fixture scenario '${scenario}'"))
    }
    seen_scenarios = seen_scenarios.push(scenario)
  }

  for assertion in manifest.assertions {
    for scenario in assertion.fixture_scenarios {
      if scenario not in manifest.fixture_scenarios {
        return Err(check_failure(f"assertion '${assertion.id}' references undeclared fixture scenario '${scenario}'"))
      }
    }
  }

  var covered_scenarios: List[Str] = []
  for fixture_case in manifest.fixture_cases.extend(manifest.macos_fixture_cases) {
    if fixture_case.scenario not in manifest.fixture_scenarios {
      return Err(check_failure(f"fixture case '${fixture_case.scenario}' is not declared"))
    }
    var required_by_assertion = false
    for assertion in manifest.assertions {
      if fixture_case.scenario in assertion.fixture_scenarios {
        required_by_assertion = true
      }
    }
    if !required_by_assertion {
      return Err(check_failure(f"fixture case '${fixture_case.scenario}' is not required by an assertion"))
    }
    if fixture_case.scenario in covered_scenarios {
      return Err(check_failure(f"duplicate executable fixture case '${fixture_case.scenario}'"))
    }
    covered_scenarios = covered_scenarios.push(fixture_case.scenario)
    if fixture_case.tests.len() == 0 {
      return Err(check_failure(f"fixture case '${fixture_case.scenario}' has no tests"))
    }
    var seen_tests: List[Str] = []
    for test_name in fixture_case.tests {
      let native_case = test_name.starts_with("tests/xsh/system-report.xsh::test_system_report_")
      let reference_case = test_name.starts_with("dev/tests/test-system-report-check.xsh::test_system_report_")
      let rust_netlink_case = test_name.starts_with("src/modules/linux/real/netlink.rs::")
      let rust_privilege_case = test_name.starts_with("tests/linux_priv.rs::system_report_")
      if (!native_case and !reference_case and !rust_netlink_case and !rust_privilege_case) or test_name.split("::").len() != 2 or test_name in seen_tests {
        return Err(check_failure(f"fixture case '${fixture_case.scenario}' has an invalid or duplicate test"))
      }
      seen_tests = seen_tests.push(test_name)
    }
  }

  return Ok()
}

## Requires a fixture name to match a declared test procedure in its source.
export pure fixture_test_definition_exists(source: Str, test_name: Str) -> Bool {
  let declaration = f"proc ${test_name}("
  for line in source.lines() {
    if line.trim().starts_with(declaration) {
      return true
    }
  }
  return false
}

## Requires a Rust fixture to name a function with a test attribute in its owner source.
export pure rust_fixture_test_definition_exists(source: Str, test_name: Str) -> Bool {
  let declaration = f"fn ${test_name}("
  var test_attribute = false
  var block_comment = false
  for line in source.lines() {
    let trimmed = line.trim()
    if block_comment {
      if trimmed.contains("*/") { block_comment = false }
      continue
    }
    if trimmed.starts_with("/*") {
      if !trimmed.contains("*/") { block_comment = true }
      continue
    }
    if trimmed == "#[test]" {
      test_attribute = true
    } else if test_attribute and (trimmed == "" or trimmed.starts_with("//") or trimmed.starts_with("#[")) {
      continue
    } else if test_attribute {
      if trimmed.starts_with(declaration) { return true }
      test_attribute = false
    }
  }
  return false
}

proc validate_fixture_test_definitions(root: Path, fixture_cases: List[FixtureCase]) [fs, error] -> Result[Unit] {
  let native_source = context.repo_path(root, "tests/xsh/system-report.xsh").read_text()?
  let reference_source = context.repo_path(root, "dev/tests/test-system-report-check.xsh").read_text()?
  let rust_netlink_source = context.repo_path(root, "src/modules/linux/real/netlink.rs").read_text()?
  let rust_privilege_source = context.repo_path(root, "tests/linux_priv.rs").read_text()?
  for fixture_case in fixture_cases {
    for test_name in fixture_case.tests {
      let parts = test_name.split("::")
      var exists = false
      if parts[0] == "src/modules/linux/real/netlink.rs" {
        exists = rust_fixture_test_definition_exists(rust_netlink_source, parts[1])
      } else if parts[0] == "tests/linux_priv.rs" {
        exists = rust_fixture_test_definition_exists(rust_privilege_source, parts[1])
      } else if parts[0] == "tests/xsh/system-report.xsh" {
        exists = fixture_test_definition_exists(native_source, parts[1])
      } else {
        exists = fixture_test_definition_exists(reference_source, parts[1])
      }
      if !exists {
        return Err(check_failure(f"fixture case '${fixture_case.scenario}' names missing test '${test_name}'"))
      }
    }
  }
  return Ok()
}

## Flags child-process creation and secondary execution in a process trace.
export pure process_trace_violations(trace: Str) -> List[Str] {
  var violations: List[Str] = []
  var initial_execs = 0

  for line in trace.lines() {
    if traced_syscall(line, "execve") or traced_syscall(line, "execveat") {
      initial_execs += 1
      if initial_execs > 1 {
        violations = violations.push("secondary exec syscall")
      }
    }

    if traced_syscall(line, "fork") {
      violations = violations.push("fork syscall")
    }
    if traced_syscall(line, "vfork") {
      violations = violations.push("vfork syscall")
    }
    if traced_syscall(line, "clone") or traced_syscall(line, "clone3") {
      if ! line.contains("CLONE_THREAD") {
        violations = violations.push("process clone syscall")
      }
    }
  }

  if initial_execs == 0 {
    violations = violations.push("initial XSH exec was not traced")
  }

  return violations
}

pure traced_syscall(line: Str, name: Str) -> Bool {
  let parts = line.split("(", maxsplit: 1)
  if parts.len() != 2 { return false }
  let before_call = parts[0].trim()
  return before_call == name or before_call.ends_with(f" ${name}")
}

# Extracts syscall arguments without splitting commas inside payloads or quoted strings.
pure traced_call_arguments(line: Str, name: Str) -> List[Str] {
  let call = line.split(f"${name}(", maxsplit: 1)
  if call.len() != 2 {
    return []
  }
  var arguments: List[Str] = []
  var current: List[Str] = []
  var braces = 0
  var brackets = 0
  var parentheses = 0
  var quoted = false
  var escaped = false
  for character in call[1].split("") {
    if quoted {
      current = current.push(character)
      if escaped {
        escaped = false
      } else if character == "\\" {
        escaped = true
      } else if character == "\"" {
        quoted = false
      }
      continue
    }
    if character == "\"" {
      quoted = true
      current = current.push(character)
    } else if character == "{" {
      braces += 1
      current = current.push(character)
    } else if character == "}" {
      braces -= 1
      current = current.push(character)
    } else if character == "[" {
      brackets += 1
      current = current.push(character)
    } else if character == "]" {
      brackets -= 1
      current = current.push(character)
    } else if character == "(" {
      parentheses += 1
      current = current.push(character)
    } else if character == ")" and braces == 0 and brackets == 0 and parentheses == 0 {
      arguments = arguments.push(current.join("").trim())
      return arguments
    } else if character == ")" {
      parentheses -= 1
      current = current.push(character)
    } else if character == "," and braces == 0 and brackets == 0 and parentheses == 0 {
      arguments = arguments.push(current.join("").trim())
      current = []
    } else {
      current = current.push(character)
    }
    if braces < 0 or brackets < 0 or parentheses < 0 {
      return []
    }
  }
  return []
}

# Requires every decoded route-netlink message outside quoted data to be a known query.
pure route_netlink_query_only(message: Str) -> Bool {
  if (!message.starts_with("[{") and !message.starts_with("{")) or message.contains("...") {
    return false
  }
  var visible: List[Str] = []
  var quoted = false
  var escaped = false
  for character in message.split("") {
    if quoted {
      if escaped {
        escaped = false
      } else if character == "\\" {
        escaped = true
      } else if character == "\"" {
        quoted = false
      }
      continue
    }
    if character == "\"" {
      quoted = true
    } else {
      visible = visible.push(character)
    }
  }
  if quoted {
    return false
  }
  let messages = visible.join("").split("nlmsg_type=")
  if messages.len() < 2 {
    return false
  }
  for encoded in messages |> drop(1) {
    let message_type = encoded.split(",").get(0, "").split("}").get(0, "").split("]").get(0, "").trim()
    if message_type not in ["RTM_GETLINK", "RTM_GETADDR", "RTM_GETROUTE", "RTM_GETRULE"] {
      return false
    }
  }
  return true
}

## Flags system mutations and network operations outside route queries.
export pure host_effect_trace_violations(trace: Str) -> List[Str] {
  var violations: List[Str] = []
  for line in trace.lines() {
    for name in [
      "creat", "rename", "renameat", "renameat2", "unlink", "unlinkat",
      "mkdir", "mkdirat", "rmdir", "link", "linkat", "symlink", "symlinkat",
      "mknod", "mknodat", "chmod", "fchmod", "fchmodat", "chown", "lchown",
      "fchown", "fchownat", "truncate", "ftruncate", "mount", "umount2",
      "swapon", "swapoff", "init_module", "finit_module", "delete_module",
      "setxattr", "lsetxattr", "fsetxattr", "removexattr", "lremovexattr", "fremovexattr",
    ] {
      if traced_syscall(line, name) {
        violations = violations.push(f"system mutation syscall ${name}")
      }
    }
    let open_name = if traced_syscall(line, "open") {"open"} else if traced_syscall(line, "openat") {"openat"} else if traced_syscall(line, "openat2") {"openat2"} else {""}
    if open_name != "" {
      let arguments = traced_call_arguments(line, open_name)
      let flags = if open_name == "open" {arguments.get(1, "")} else {arguments.get(2, "")}
      if flags.contains("O_WRONLY") or flags.contains("O_RDWR") or flags.contains("O_CREAT") or flags.contains("O_TRUNC") or flags.contains("O_APPEND") {
        violations = violations.push("writable file open")
      }
    }
    for name in ["write", "writev"] {
      if traced_syscall(line, name) {
        let descriptor = line.split(f"${name}(", maxsplit: 1).get(1, "").split(",").get(0, "").trim()
        if descriptor != "1" and descriptor != "2" {
          violations = violations.push("write to non-output descriptor")
        }
      }
    }
    for name in ["pwrite64", "pwritev", "pwritev2"] {
      if traced_syscall(line, name) {
        violations = violations.push("positioned write syscall")
      }
    }
    if traced_syscall(line, "socket") {
      let arguments = traced_call_arguments(line, "socket")
      let family = arguments.get(0, "")
      let socket_type = arguments.get(1, "")
      let protocol = arguments.get(2, "")
      if family == "AF_NETLINK" and protocol not in ["NETLINK_ROUTE", "0"] {
        violations = violations.push("unexpected netlink protocol")
      } else if family == "AF_NETLINK" and !socket_type.starts_with("SOCK_RAW") and !socket_type.starts_with("SOCK_DGRAM") {
        violations = violations.push("unexpected netlink socket type")
      } else if family in ["AF_INET", "AF_INET6", "AF_PACKET"] {
        violations = violations.push("external network socket")
      } else if family != "AF_NETLINK" {
        violations = violations.push("unexpected socket family")
      }
    }
    if traced_syscall(line, "socketpair") {
      violations = violations.push("unexpected socket pair")
    }
    if traced_syscall(line, "listen") {
      violations = violations.push("unexpected network listener")
    }
    if traced_syscall(line, "accept") or traced_syscall(line, "accept4") {
      violations = violations.push("unexpected network accept")
    }
    if traced_syscall(line, "connect") {
      let destination = traced_call_arguments(line, "connect").get(1, "")
      if destination.starts_with("{sa_family=AF_INET") or destination.starts_with("{sa_family=AF_PACKET") {
        violations = violations.push("external network syscall")
      } else if destination.starts_with("{sa_family=AF_UNIX") {
        violations = violations.push("local socket connection")
      } else {
        violations = violations.push("unexpected network connection")
      }
    }
    if traced_syscall(line, "bind") {
      let destination = traced_call_arguments(line, "bind").get(1, "")
      if destination.starts_with("{sa_family=AF_INET") or destination.starts_with("{sa_family=AF_PACKET") {
        violations = violations.push("external network syscall")
      } else if !destination.starts_with("{sa_family=AF_NETLINK") and !destination.starts_with("{nl_family=AF_NETLINK") {
        violations = violations.push("unexpected network bind")
      }
    }
    if traced_syscall(line, "sendto") {
      let arguments = traced_call_arguments(line, "sendto")
      let destination = arguments.get(4, "")
      if destination.starts_with("{sa_family=AF_INET") or destination.starts_with("{sa_family=AF_PACKET") {
        violations = violations.push("external network syscall")
      } else if destination.starts_with("{sa_family=AF_NETLINK") or destination.starts_with("{nl_family=AF_NETLINK") {
        if !route_netlink_query_only(arguments.get(1, "")) {
          violations = violations.push("non-query netlink request")
        }
      } else {
        violations = violations.push("unexpected network send")
      }
    } else if traced_syscall(line, "send") or traced_syscall(line, "sendmsg") or traced_syscall(line, "sendmmsg") {
      violations = violations.push("unexpected network send")
    }
  }
  return violations
}

pure process_path_identity(value: Str) -> Bool {
  if value == "self" or value == "thread-self" {
    return true
  }
  if value == "" {
    return false
  }
  for character in value.split("") {
    if !"0123456789".contains(character) {
      return false
    }
  }
  return true
}

# Resolves lexical path components for a trace launched with the root directory as its cwd.
pure normalized_traced_path(argument: Str) -> List[Str] {
  if !argument.starts_with("\"") {return []}
  let traced_path = argument.split("\"").get(1, "")
  var components: List[Str] = []
  for component in traced_path.split("/") {
    if component == "" or component == "." {continue}
    if component == ".." {
      if components.len() > 0 {components = components |> take(components.len() - 1)}
    } else {
      components = components.push(component)
    }
  }
  return components
}

## Rejects reads of per-process environments, command lines, memory, and open paths.
export pure forbidden_process_read_violations(trace: Str) -> List[Str] {
  var violations: List[Str] = []
  for line in trace.lines() {
    for name in ["open", "openat", "openat2", "readlink", "readlinkat"] {
      if traced_syscall(line, name) {
        let path_index = if name in ["openat", "openat2", "readlinkat"] {1} else {0}
        let argument = traced_call_arguments(line, name).get(path_index, "")
        let path_parts = normalized_traced_path(argument)
        if path_parts.len() < 3 or path_parts[0] != "proc" or !process_path_identity(path_parts[1]) {
          continue
        }
        var field = path_parts[2]
        if field == "task" and path_parts.len() >= 5 and process_path_identity(path_parts[3]) {
          field = path_parts[4]
        }
        if field in ["environ", "cmdline", "mem", "maps", "smaps", "smaps_rollup", "auxv", "fd", "fdinfo", "map_files", "cwd", "root", "exe"] {
          violations = violations.push("forbidden process source read")
        }
      }
    }
  }
  return violations
}

## Rejects process details that disclose environment, command-line, memory, or open-path data.
export pure forbidden_process_field_violations(candidate_json: Str) -> Result[List[Str]] {
  let data = json.decode(candidate_json)?
  let processes = json.get(data, ["processes", "processes"])?.require(List[Record])?
  var violations: List[Str] = []
  for process_item in processes {
    for field in process_item.keys() {
      if field in ["environment", "environ", "credentials", "cmdline", "command_line", "open_paths", "open_files", "fds", "fdinfo", "maps", "cwd", "exe", "memory"] {
        violations = violations.push(f"forbidden process output field ${field}")
      }
    }
  }
  return Ok(violations)
}

pure replay_live_source_path(source_path: Str) -> Bool {
  let components = normalized_traced_path(source_path)
  return components.len() > 0 and components[0] in ["proc", "sys", "etc"]
}

## Flags live-source file and metadata reads during saved-report replay.
export pure replay_host_read_violations(trace: Str) -> List[Str] {
  var violations: List[Str] = []
  for line in trace.lines() {
    for name in [
      "open", "openat", "openat2", "readlink", "readlinkat", "stat", "lstat",
      "newfstatat", "statx", "access", "faccessat", "faccessat2",
    ] {
      if traced_syscall(line, name) {
        let path_index = if name in ["openat", "openat2", "readlinkat", "newfstatat", "statx", "faccessat", "faccessat2"] { 1 } else { 0 }
        let source_path = traced_call_arguments(line, name).get(path_index, "")
        if replay_live_source_path(source_path) {
          violations = violations.push("saved-report replay read a live source")
        }
      }
    }
  }
  return violations
}

# Traces one production command path with an empty command search path.
proc audit_no_subprocess_case(
  xsh_bin: Str,
  script: Str,
  applet_args: List[Str],
  expected_success: Bool,
  label: Str,
  scratch: FsRoot,
) [fs, process, error] -> Result[Unit] {
  let trace_name = fp"trace-${label}"
  let stdout_name = fp"stdout-${label}"
  let stderr_name = fp"stderr-${label}"
  fs.root_write(scratch, trace_name, "")?
  fs.root_write(scratch, stdout_name, "")?
  fs.root_write(scratch, stderr_name, "")?
  let scratch_path = fs.root_path(scratch)?
  let trace_path = fp"${scratch_path}/${trace_name}"
  let stdout_path = fp"${scratch_path}/${stdout_name}"
  let stderr_path = fp"${scratch_path}/${stderr_name}"
  let strace = "/usr/bin/strace"
  let argv = trace_audit_argv(strace, trace_path.display(), xsh_bin, script, applet_args)
  let command = process.command_argv(
    strace,
    argv,
    cwd: p"/",
    env: {
      HOME: "/nonexistent",
      PATH: "/nonexistent",
      LANG: "C",
      LC_ALL: "C",
      TERM: "dumb",
      XSH_LINUX_REAL: "1",
    },
    stdout: stdout_path,
    stderr: stderr_path,
  )
  let status = process.run(command)?
  let trace = fs.root_read_text(scratch, trace_name)?
  let output = fs.root_read_text(scratch, stdout_name)?
  let candidate_error = fs.root_read_text(scratch, stderr_name)?
  let violations = process_trace_violations(trace)
    .extend(host_effect_trace_violations(trace))
    .extend(forbidden_process_read_violations(trace))

  if status.exited_with(0) != expected_success {
    return Err(check_failure(f"${label} command had unexpected exit status: ${candidate_error.trim()}"))
  }
  if violations.len() > 0 {
    return Err(check_failure(violations.join(", ")))
  }
  if expected_success and output.trim() == "" {
    return Err(check_failure(f"${label} emitted no output"))
  }
  if expected_success and applet_args.contains("--json") {
    let forbidden_fields = forbidden_process_field_violations(output)?
    if forbidden_fields.len() > 0 {
      return Err(check_failure(forbidden_fields.join(", ")))
    }
  }
  if !expected_success and output.trim() != "" {
    return Err(check_failure(f"${label} wrote stdout before failing"))
  }

  return Ok()
}

## Traces live, saved-report, and failure paths with an unusable command search path.
export proc audit_no_subprocess(xsh_bin: Str, script: Str) [fs, process, error] -> Result[Unit] {
  if ! xsh_bin.starts_with("/") or ! script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }

  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  fs.root_write(scratch, p"malformed.json", "{invalid")?
  fs.root_write(scratch, p"unsupported.json", "{\"schema_version\":2}")?
  fs.root_write(scratch, p"invalid-utf8.json", b"\xff")?
  let scratch_path = fs.root_path(scratch)?
  let malformed_path = fp"${scratch_path}/malformed.json"
  let unsupported_path = fp"${scratch_path}/unsupported.json"
  let invalid_utf8_path = fp"${scratch_path}/invalid-utf8.json"
  let missing_path = fp"${scratch_path}/missing.json"

  audit_no_subprocess_case(xsh_bin, script, ["--json"], true, "live-json", scratch)?
  audit_no_subprocess_case(xsh_bin, script, [], true, "overview-text", scratch)?
  audit_no_subprocess_case(xsh_bin, script, ["--full"], true, "full-text", scratch)?
  audit_no_subprocess_case(xsh_bin, script, ["--section", "cpu", "--json"], true, "cpu-json", scratch)?
  audit_no_subprocess_case(xsh_bin, script, ["--sensitive", "--json"], true, "sensitive-json", scratch)?
  let saved_path = fp"${scratch_path}/stdout-live-json"
  audit_no_subprocess_case(xsh_bin, script, ["--from", saved_path.display(), "--json"], true, "offline-replay", scratch)?
  let live_json = fs.root_read_text(scratch, p"stdout-live-json")?
  let replay_json = fs.root_read_text(scratch, p"stdout-offline-replay")?
  if replay_json != live_json {
    return Err(check_failure("saved-report replay changed the JSON snapshot"))
  }
  audit_no_subprocess_case(xsh_bin, script, ["--version"], true, "version", scratch)?
  audit_no_subprocess_case(xsh_bin, script, ["--section", "invalid"], false, "invalid-section", scratch)?
  audit_no_subprocess_case(xsh_bin, script, ["--from", malformed_path.display()], false, "malformed-replay", scratch)?
  audit_no_subprocess_case(xsh_bin, script, ["--from", unsupported_path.display()], false, "unsupported-schema", scratch)?
  audit_no_subprocess_case(xsh_bin, script, ["--from", invalid_utf8_path.display()], false, "invalid-utf8-replay", scratch)?
  audit_no_subprocess_case(xsh_bin, script, ["--from", missing_path.display()], false, "missing-replay", scratch)?
  for label in ["offline-replay", "malformed-replay", "unsupported-schema", "invalid-utf8-replay", "missing-replay"] {
    let replay_trace = fs.root_read_text(scratch, fp"trace-${label}")?
    let replay_reads = replay_host_read_violations(replay_trace)
    if replay_reads.len() > 0 {
      let reasons = replay_reads.join(", ")
      return Err(check_failure(f"${label}: ${reasons}"))
    }
  }
  return Ok()
}

proc read_reference_cpu_sets() [fs, error] -> Result[CpuSetReference] {
  return {
    possible: parse_reference_cpu_list(p"/sys/devices/system/cpu/possible".read_text()?, false)?,
    present: parse_reference_cpu_list(p"/sys/devices/system/cpu/present".read_text()?, false)?,
    online: parse_reference_cpu_list(p"/sys/devices/system/cpu/online".read_text()?, false)?,
    offline: parse_reference_cpu_list(p"/sys/devices/system/cpu/offline".read_text()?, true)?,
  }
}

proc read_uname_reference(flag: Str, scratch: FsRoot, name: Str) [fs, process, time, error] -> Result[UnameObservation] {
  let scratch_path = fs.root_path(scratch)?
  let output_path = fp"${scratch_path}/${name}"
  fs.root_write(scratch, fp"${name}", "")?
  let started = time.now()
  let status = process.run(process.command_argv(
    "/bin/uname", ["uname", flag], cwd: p"/",
    env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"}, stdout: output_path,
  ))?
  let ended = time.now()
  if !status.exited_with(0) {
    return Err(check_failure(f"uname ${flag} reference command failed"))
  }
  let value = fs.root_read_text(scratch, fp"${name}")?.trim()
  if value == "" {
    return Err(check_failure(f"uname ${flag} reference command returned an empty value"))
  }
  return {value: value, started: started, ended: ended}
}

proc read_uptime_reference(scratch: FsRoot, name: Str) [fs, process, time, error] -> Result[UptimeReferenceObservation] {
  let scratch_path = fs.root_path(scratch)?
  fs.root_write(scratch, fp"${name}", "")?
  let started = time.now()
  let status = process.run(process.command_argv(
    "/bin/cat", ["cat", "/proc/uptime"], cwd: p"/",
    env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
    stdout: fp"${scratch_path}/${name}",
  ))?
  let ended = time.now()
  if !status.exited_with(0) {
    return Err(check_failure("cat /proc/uptime reference command failed"))
  }
  let output = fs.root_read_text(scratch, fp"${name}")?
  return {seconds: parse_reference_uptime_seconds(output)?, started: started, ended: ended}
}

proc read_os_release_reference(scratch: FsRoot, source_path: Str, name: Str) [fs, process, time, error] -> Result[UnameObservation] {
  let scratch_path = fs.root_path(scratch)?
  fs.root_write(scratch, fp"${name}", "")?
  let started = time.now()
  let status = process.run(process.command_argv(
    "/bin/cat", ["cat", source_path], cwd: p"/",
    env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
    stdout: fp"${scratch_path}/${name}",
  ))?
  let ended = time.now()
  if !status.exited_with(0) {
    return Err(check_failure(f"cat ${source_path} reference command failed"))
  }
  let value = fs.root_read_text(scratch, fp"${name}")?
  return {value: value, started: started, ended: ended}
}

## Reads one device-tree source through bounded od output and retains absence explicitly.
export proc read_device_tree_raw_reference(scratch: FsRoot, source_path: Path, max_bytes: Int, name: Str) [fs, process, time, error] -> Result[DeviceTreeRawObservation] {
  let started = time.now()
  if !source_path.exists()? {
    return {data: null, state: "absent", started: started, ended: time.now()}
  }
  if !p"/usr/bin/od".exists()? {
    return Err(check_failure("device-tree comparison needs /usr/bin/od"))
  }
  let scratch_path = fs.root_path(scratch)?
  fs.root_write(scratch, fp"${name}", "")?
  let status = process.run(process.command_argv(
    "/usr/bin/od", ["od", "-An", "-tx1", "-v", "-N", f"${max_bytes + 1}", source_path.display()],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
    stdout: fp"${scratch_path}/${name}",
  ))?
  let ended = time.now()
  if !status.exited_with(0) {
    return Err(check_failure(f"device-tree od reference failed for ${source_path}"))
  }
  let output = fs.root_read_result(scratch, fp"${name}", max_bytes: max_bytes * 4 + 4096)?
  if output.state != "observed" or output.truncated or output.data == null {
    return Err(check_failure(f"device-tree od reference output is incomplete for ${source_path}"))
  }
  let decoded = (output.data ?? b"").utf8()
  match decoded {
    Ok(_) => {}
    Err(_) => return Err(check_failure("device-tree od reference output is not UTF-8"))
  }
  return {data: parse_reference_od_bytes(decoded?, max_bytes)?, state: "observed", started: started, ended: ended}
}

proc read_namespace_reference(scratch: FsRoot, kernel_name: Str, output_name: Str) [fs, process, time, error] -> Result[NamespaceLinkObservation] {
  let scratch_path = fs.root_path(scratch)?
  fs.root_write(scratch, fp"${output_name}", "")?
  fs.root_write(scratch, fp"${output_name}-error", "")?
  let source_path = f"/proc/self/ns/${kernel_name}"
  let started = time.now()
  let status = process.run(process.command_argv(
    "/usr/bin/readlink", ["readlink", source_path], cwd: p"/",
    env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
    stdout: fp"${scratch_path}/${output_name}",
    stderr: fp"${scratch_path}/${output_name}-error",
  ))?
  let ended = time.now()
  if !status.exited_with(0) {
    return Err(check_failure(f"namespace reference ${source_path} is unavailable"))
  }
  let target = fs.root_read_text(scratch, fp"${output_name}")?.trim()
  if target == "" {
    return Err(check_failure(f"namespace reference ${source_path} returned an empty target"))
  }
  return {target: target, started: started, ended: ended}
}

proc read_reference_tool_version(binary: Str, argv_name: Str, scratch: FsRoot, name: Str) [fs, process, error] -> Result[Str] {
  let scratch_path = fs.root_path(scratch)?
  let output_name = fp"${name}-version"
  let error_name = fp"${name}-version-error"
  fs.root_write(scratch, output_name, "")?
  fs.root_write(scratch, error_name, "")?
  let status = process.run(process.command_argv(
    binary, [argv_name, "--version"], cwd: p"/",
    env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
    stdout: fp"${scratch_path}/${output_name}",
    stderr: fp"${scratch_path}/${error_name}",
  ))?
  let output = fs.root_read_text(scratch, output_name)?
  let error_output = fs.root_read_text(scratch, error_name)?
  let busybox_lines = error_output.lines() |> where .starts_with("BusyBox")
  if status.exited_with(0) {
    return output.lines().get(0, "unavailable")
  }
  return busybox_lines.get(0, "unavailable")
}

# Brackets a candidate report with independent kernel identity observations.
proc compare_live_identity(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Int] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  if !p"/bin/uname".exists()? {
    return Err(check_failure("identity comparison needs /bin/uname"))
  }
  if !p"/bin/cat".exists()? or !p"/proc/uptime".exists()? {
    return Err(check_failure("identity comparison needs /bin/cat and /proc/uptime"))
  }
  let os_release_path = if p"/etc/os-release".exists()? {"/etc/os-release"} else if p"/usr/lib/os-release".exists()? {"/usr/lib/os-release"} else {""}
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let before_release = read_uname_reference("-r", scratch, "release-before")?
  let before_architecture = read_uname_reference("-m", scratch, "architecture-before")?
  let before_uptime = read_uptime_reference(scratch, "uptime-before")?
  let before_os_release = if os_release_path == "" {{value: "", started: 0, ended: 0}} else {read_os_release_reference(scratch, os_release_path, "os-release-before")?}
  let before_device_tree_model = read_device_tree_raw_reference(scratch, p"/sys/firmware/devicetree/base/model", 4096, "device-tree-model-before")?
  let before_device_tree_compatible = read_device_tree_raw_reference(scratch, p"/sys/firmware/devicetree/base/compatible", 16384, "device-tree-compatible-before")?
  let scratch_path = fs.root_path(scratch)?
  fs.root_write(scratch, p"candidate", "")?
  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "identity", "--json"],
    cwd: p"/",
    env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate identity collection failed"))
  }
  let after_os_release = if os_release_path == "" {{value: "", started: 0, ended: 0}} else {read_os_release_reference(scratch, os_release_path, "os-release-after")?}
  let after_device_tree_model = read_device_tree_raw_reference(scratch, p"/sys/firmware/devicetree/base/model", 4096, "device-tree-model-after")?
  let after_device_tree_compatible = read_device_tree_raw_reference(scratch, p"/sys/firmware/devicetree/base/compatible", 16384, "device-tree-compatible-after")?
  let after_uptime = read_uptime_reference(scratch, "uptime-after")?
  let after_release = read_uname_reference("-r", scratch, "release-after")?
  let after_architecture = read_uname_reference("-m", scratch, "architecture-after")?
  if before_release.value != after_release.value or before_architecture.value != after_architecture.value {
    print "identity: unstable (uname changed around candidate collection)"
    return Err(check_failure("identity could not be scored because the reference changed"))
  }
  if os_release_path != "" and before_os_release.value != after_os_release.value {
    print "identity.os.release: unstable (source changed around candidate collection)"
    return Err(check_failure("OS release could not be scored because the reference changed"))
  }
  if before_device_tree_model.state != after_device_tree_model.state or before_device_tree_model.data != after_device_tree_model.data or
      before_device_tree_compatible.state != after_device_tree_compatible.state or before_device_tree_compatible.data != after_device_tree_compatible.data {
    print "firmware.device-tree: unstable (source changed around candidate collection)"
    return Err(check_failure("device-tree identity could not be scored because the reference changed"))
  }
  let uname_version = read_reference_tool_version("/bin/uname", "uname", scratch, "uname")?
  let cat_version = read_reference_tool_version("/bin/cat", "cat", scratch, "cat")?
  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let compared = compare_identity(candidate, before_release.value, before_architecture.value)?
  let uptime_compared = compare_uptime(candidate, before_uptime.seconds, after_uptime.seconds)?
  var os_release_exact = true
  var os_release_scored = 0
  var os_release_line = "identity.os.release: reference unavailable (no os-release source)"
  var os_release_provenance = "os-release=unavailable"
  if os_release_path != "" {
    let os_release_compared = compare_os_release(candidate, parse_reference_os_release(before_os_release.value)?)?
    let os_release_state = if os_release_compared.exact {"exact"} else if os_release_compared.id_missing {"candidate ID missing"} else {"mismatch"}
    os_release_line = f"identity.os.release: ${os_release_state}; id_exact=${os_release_compared.id_exact}, version_id_exact=${os_release_compared.version_id_exact}, candidate_version_id_missing=${os_release_compared.version_id_missing}"
    os_release_provenance = f"os-release-argv=/bin/cat ${os_release_path}; before-os-release=${before_os_release.started}..${before_os_release.ended} ms; after-os-release=${after_os_release.started}..${after_os_release.ended} ms"
    os_release_exact = os_release_compared.exact
    os_release_scored = 1
  }
  var device_tree_exact = true
  var device_tree_scored = 0
  var device_tree_line = "firmware.device-tree: reference unavailable (no device-tree identity source)"
  var device_tree_provenance = "device-tree=unavailable"
  if before_device_tree_model.data != null or before_device_tree_compatible.data != null {
    let device_tree_reference = parse_reference_device_tree(before_device_tree_model.data, before_device_tree_compatible.data)?
    let device_tree_compared = compare_device_tree(candidate, device_tree_reference)?
    let device_tree_state = if device_tree_compared.exact {"exact"} else {"mismatch"}
    device_tree_line = f"firmware.device-tree: ${device_tree_state}; source_exact=${device_tree_compared.source_exact}, model_exact=${device_tree_compared.model_exact}, compatible_exact=${device_tree_compared.compatible_exact}"
    var device_tree_argv: List[List[Str]] = []
    if before_device_tree_model.data != null {
      device_tree_argv = device_tree_argv.push(["od", "-An", "-tx1", "-v", "-N", "4097", "/sys/firmware/devicetree/base/model"])
    }
    if before_device_tree_compatible.data != null {
      device_tree_argv = device_tree_argv.push(["od", "-An", "-tx1", "-v", "-N", "16385", "/sys/firmware/devicetree/base/compatible"])
    }
    let od_version = read_reference_tool_version("/usr/bin/od", "od", scratch, "od")?
    device_tree_provenance = f"device-tree-od=${od_version}; argv=${json.encode(device_tree_argv)?}; before-model=${before_device_tree_model.started}..${before_device_tree_model.ended} ms; before-compatible=${before_device_tree_compatible.started}..${before_device_tree_compatible.ended} ms; after-model=${after_device_tree_model.started}..${after_device_tree_model.ended} ms; after-compatible=${after_device_tree_compatible.started}..${after_device_tree_compatible.ended} ms"
    device_tree_exact = device_tree_compared.exact
    device_tree_scored = 1
  }
  let release_state = if compared.release_exact {"exact"} else if compared.release_missing {"candidate missing"} else {"mismatch"}
  let architecture_state = if compared.architecture_exact {"exact"} else if compared.architecture_missing {"candidate missing"} else {"mismatch"}
  let release_display = compared.candidate_release ?? "null"
  let architecture_display = compared.candidate_architecture ?? "null"
  let reference_release_display = json.encode(before_release.value)?
  let reference_architecture_display = json.encode(before_architecture.value)?
  let candidate_release_display = json.encode(release_display)?
  let candidate_architecture_display = json.encode(architecture_display)?
  let uptime_state = if uptime_compared.bracketed {"bracketed"} else if uptime_compared.candidate_missing {"candidate missing"} else {"mismatch"}
  let candidate_uptime_number = uptime_compared.candidate_seconds ?? -1
  let candidate_uptime_display = if uptime_compared.candidate_missing {"null"} else {f"${candidate_uptime_number}"}
  let candidate_data = json.decode(candidate)?
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let host_claim_display = json.encode(host_claim)?
  print f"identity.kernel.release: ${release_state}; reference=${reference_release_display}; candidate=${candidate_release_display}"
  print f"identity.kernel.architecture: ${architecture_state}; reference=${reference_architecture_display}; candidate=${candidate_architecture_display}"
  print f"identity.uptime: ${uptime_state}; before=${before_uptime.seconds}, candidate=${candidate_uptime_display}, after=${after_uptime.seconds} seconds"
  print f"${os_release_line}"
  print f"${device_tree_line}"
  print f"reference: uname=${uname_version}; cat=${cat_version}; argv=/bin/uname -r, /bin/uname -m, /bin/cat /proc/uptime; ${os_release_provenance}; ${device_tree_provenance}; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${host_claim_display}; before-release=${before_release.started}..${before_release.ended} ms; before-architecture=${before_architecture.started}..${before_architecture.ended} ms; before-uptime=${before_uptime.started}..${before_uptime.ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after-uptime=${after_uptime.started}..${after_uptime.ended} ms; after-release=${after_release.started}..${after_release.ended} ms; after-architecture=${after_architecture.started}..${after_architecture.ended} ms"
  if !compared.exact or !uptime_compared.bracketed or !os_release_exact or !device_tree_exact {
    return Err(check_failure("mandatory identity assertions failed"))
  }
  return Ok(3 + os_release_scored + device_tree_scored)
}

# Brackets namespace symlink identities around one sensitive, identity-only candidate snapshot.
proc compare_live_namespaces(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  if !p"/usr/bin/readlink".exists()? {
    return Err(check_failure("namespace comparison needs /usr/bin/readlink"))
  }
  let specs = [
    {kernel: "mnt", field: "mount_namespace"},
    {kernel: "net", field: "network_namespace"},
    {kernel: "pid", field: "pid_namespace"},
    {kernel: "cgroup", field: "cgroup_namespace"},
    {kernel: "uts", field: "uts_namespace"},
    {kernel: "ipc", field: "ipc_namespace"},
    {kernel: "user", field: "user_namespace"},
    {kernel: "time", field: "time_namespace"},
  ]
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  var before: List[NamespaceReference] = []
  var reference_argv: List[List[Str]] = []
  let before_started = time.now()
  for spec in specs {
    reference_argv = reference_argv.push(["readlink", f"/proc/self/ns/${spec.kernel}"])
    let observed = read_namespace_reference(scratch, spec.kernel, f"${spec.kernel}-before")?
    before = before.push({field: spec.field, target: observed.target})
  }
  let before_ended = time.now()
  let scratch_path = fs.root_path(scratch)?
  fs.root_write(scratch, p"candidate", "")?
  let candidate_started = time.now()
  let status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "identity", "--sensitive", "--json"],
    cwd: p"/",
    env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate",
  ))?
  let candidate_ended = time.now()
  if !status.exited_with(0) {
    return Err(check_failure("candidate namespace collection failed"))
  }
  let after_started = time.now()
  var index = 0
  for spec in specs {
    let observed = read_namespace_reference(scratch, spec.kernel, f"${spec.kernel}-after")?
    if observed.target != before[index].target {
      print f"identity.scope.namespaces: unstable (${spec.kernel} changed around candidate collection)"
      return Err(check_failure("namespace identities changed around candidate collection"))
    }
    index += 1
  }
  let after_ended = time.now()
  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let compared = compare_namespace_scope(candidate, before)?
  let version = read_reference_tool_version("/usr/bin/readlink", "readlink", scratch, "readlink")?
  let candidate_data = json.decode(candidate)?
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let host_claim_display = json.encode(host_claim)?
  let missing = json.encode(compared.missing_fields)?
  let mismatched = json.encode(compared.mismatched_fields)?
  let argv_display = json.encode(reference_argv)?
  let agreement = if compared.exact {"exact"} else {"mismatch"}
  print f"identity.scope.namespaces: ${agreement}; reference=${compared.reference_count}, matched=${compared.matched_count}, candidate_missing=${missing}, mismatched=${mismatched}"
  print f"reference: ${version}; executable=/usr/bin/readlink; argv_each_bracket=${argv_display}; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${host_claim_display}; before=${before_started}..${before_ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after_started}..${after_ended} ms"
  if !compared.exact {
    return Err(check_failure("identity.scope.namespaces mandatory assertion failed"))
  }
  return Ok()
}

proc print_cpu_id_set_result(name: Str, result: CpuIdSetComparison) [io, error] -> Result[Unit] {
  let missing = json.encode(result.missing_ids)?
  let unexpected = json.encode(result.unexpected_ids)?
  let agreement = if result.exact {"exact"} else {"mismatch"}
  print f"  ${name}: ${agreement}; reference=${result.reference_count}, candidate=${result.candidate_count}, matched=${result.matched_count}, missing=${missing}, unexpected=${unexpected}"
  return Ok()
}

# Reads a bounded raw host-memory snapshot with its observation interval.
proc read_meminfo_reference(scratch: FsRoot, name: Str) [fs, process, time, error] -> Result[MeminfoObservation] {
  let scratch_path = fs.root_path(scratch)?
  fs.root_write(scratch, fp"${name}", "")?
  fs.root_write(scratch, fp"${name}-error", "")?
  let started = time.now()
  let status = process.run(process.command_argv(
    "/bin/cat", ["cat", "/proc/meminfo"], cwd: p"/",
    env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
    stdout: fp"${scratch_path}/${name}", stderr: fp"${scratch_path}/${name}-error",
  ))?
  let ended = time.now()
  if !status.exited_with(0) {
    return Err(check_failure("meminfo raw reference command failed"))
  }
  let raw = fs.root_read_result(scratch, fp"${name}", max_bytes: 1048576)?
  if raw.state != "observed" or raw.truncated or raw.data == null {
    return Err(check_failure("meminfo raw reference is incomplete"))
  }
  var output = ""
  match (raw.data ?? b"").utf8() {
    Ok(value) => output = value
    Err(_) => return Err(check_failure("meminfo raw reference is not UTF-8"))
  }
  return Ok({counters: parse_meminfo_reference(output)?, started: started, ended: ended})
}

# Brackets one memory report with raw procfs fields and exact byte conversion.
proc compare_live_meminfo(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  if !p"/bin/cat".exists()? or !p"/proc/meminfo".exists()? {
    return Err(check_failure("meminfo comparison needs /bin/cat and /proc/meminfo"))
  }
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let before = read_meminfo_reference(scratch, "meminfo-before")?
  let scratch_path = fs.root_path(scratch)?
  fs.root_write(scratch, p"candidate", "")?
  fs.root_write(scratch, p"candidate-error", "")?
  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "memory", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate", stderr: fp"${scratch_path}/candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate memory collection failed"))
  }
  let after = read_meminfo_reference(scratch, "meminfo-after")?
  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let data = json.decode(candidate)?
  let issues = json.get(data, ["issues"])?.require(List[CandidateIssueField])?
  for item in issues {
    if item.section == "memory" and item.field.starts_with("meminfo") {
      return Err(check_failure("candidate meminfo source has a collection issue"))
    }
  }
  let compared = compare_meminfo(candidate, before.counters, after.counters)?
  let host_claim = json.get(data, ["scope", "host_claim"])?.require(Str)?
  let agreement = if compared.exact_scored {"exact_scored"} else {"mismatch"}
  print f"memory.meminfo: ${agreement}; reference=${compared.reference_count}, candidate=${compared.candidate_count}, matched=${compared.matched_count}, stable=${compared.stable_count}, changed=${compared.changed_count}, missing=${compared.missing_names.len()}, unexpected=${compared.unexpected_names.len()}, counter_mismatches=${compared.mismatched_names.len()}, scalar_mismatches=${compared.scalar_mismatches.len()}"
  print f"reference: adapter=proc-meminfo-raw; argv=/bin/cat /proc/meminfo; bound=1048576 bytes; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${json.encode(host_claim)?}; before=${before.started}..${before.ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after.started}..${after.ended} ms"
  if !compared.exact_scored {
    return Err(check_failure("meminfo scored fields differ from the raw reference"))
  }
  return Ok()
}

# Reads available sysfs policy files through an explicit raw cat reference.
proc read_thp_reference(scratch: FsRoot, label: Str) [fs, process, time, error] -> Result[ThpObservation] {
  let scratch_path = fs.root_path(scratch)?
  var policies: List[ThpReferencePolicy] = []
  let started = time.now()
  for name in ["enabled", "defrag"] {
    let source_path = fp"/sys/kernel/mm/transparent_hugepage/${name}"
    if !source_path.exists()? {
      continue
    }
    let output_name = f"thp-${label}-${name}"
    fs.root_write(scratch, fp"${output_name}", "")?
    fs.root_write(scratch, fp"${output_name}-error", "")?
    let status = process.run(process.command_argv(
      "/bin/cat", ["cat", source_path.display()], cwd: p"/",
      env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
      stdout: fp"${scratch_path}/${output_name}", stderr: fp"${scratch_path}/${output_name}-error",
    ))?
    if !status.exited_with(0) {
      return Err(check_failure(f"THP ${name} raw reference command failed"))
    }
    let raw = fs.root_read_result(scratch, fp"${output_name}", max_bytes: 4096)?
    if raw.state != "observed" or raw.truncated or raw.data == null {
      return Err(check_failure(f"THP ${name} raw reference is incomplete"))
    }
    var output = ""
    match (raw.data ?? b"").utf8() {
      Ok(value) => output = value
      Err(_) => return Err(check_failure(f"THP ${name} raw reference is not UTF-8"))
    }
    policies = policies.push({name: name, value: parse_thp_reference(output)?})
  }
  return Ok({policies: policies, started: started, ended: time.now()})
}

# Brackets one sensitive memory report with named transparent huge-page policies.
proc compare_live_thp(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Bool] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  if !p"/bin/cat".exists()? {
    return Err(check_failure("THP comparison needs /bin/cat"))
  }
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let before = read_thp_reference(scratch, "before")?
  let scratch_path = fs.root_path(scratch)?
  fs.root_write(scratch, p"candidate", "")?
  fs.root_write(scratch, p"candidate-error", "")?
  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "memory", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate", stderr: fp"${scratch_path}/candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate memory collection failed"))
  }
  let after = read_thp_reference(scratch, "after")?
  if before.policies.len() == 0 and after.policies.len() == 0 {
    print "memory.thp: reference unavailable (no policy files are exported)"
    return Ok(false)
  }
  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let data = json.decode(candidate)?
  let issues = json.get(data, ["issues"])?.require(List[CandidateIssueField])?
  for item in issues {
    if item.section == "memory" and item.field.starts_with("transparent_huge_pages") {
      return Err(check_failure("candidate THP policy has a collection issue"))
    }
  }
  let compared = compare_thp(candidate, before.policies, after.policies)?
  let host_claim = json.get(data, ["scope", "host_claim"])?.require(Str)?
  let agreement = if compared.exact {"exact"} else {"mismatch"}
  var reference_argv: List[Str] = []
  for item in before.policies {
    reference_argv = reference_argv.push(f"/bin/cat /sys/kernel/mm/transparent_hugepage/${item.name}")
  }
  print f"memory.thp: ${agreement}; reference=${compared.reference_count}, candidate=${compared.candidate_count}, matched=${compared.matched_count}, missing=${compared.missing_names.len()}, unexpected=${compared.unexpected_names.len()}, mismatched=${compared.mismatched_names.len()}"
  print f"reference: adapter=thp-sysfs-v1; argv=${json.encode(reference_argv)?}; exit_status=0; output=${json.encode(before.policies)?}; bound=4096 bytes per file; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${json.encode(host_claim)?}; before=${before.started}..${before.ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after.started}..${after.ended} ms"
  if !compared.exact {
    return Err(check_failure("THP policies differ from stable sysfs references"))
  }
  return Ok(true)
}

# Captures every visible named vulnerability through a bounded raw cat read.
proc read_vulnerability_reference(scratch: FsRoot, label: Str) [fs, process, time, error] -> Result[VulnerabilityObservation] {
  let source_root = fs.open_root(p"/sys/devices/system/cpu")?
  defer fs.close_root(source_root)?
  let started = time.now()
  let listing = fs.root_children(source_root, p"vulnerabilities", max_entries: 256)?
  if listing.state == "absent" {
    return Ok({descriptions: [], started: started, ended: time.now()})
  }
  if listing.state != "complete" {
    return Err(check_failure("vulnerability reference directory enumeration is incomplete"))
  }
  let scratch_path = fs.root_path(scratch)?
  var descriptions: List[VulnerabilityReference] = []
  for index in range(listing.children.len()) {
    let name = listing.children[index].name()
    if !valid_vulnerability_name(name) {
      return Err(check_failure("vulnerability reference has an unsafe file name"))
    }
    let output_name = f"vulnerability-${label}-${index}"
    fs.root_write(scratch, fp"${output_name}", "")?
    fs.root_write(scratch, fp"${output_name}-error", "")?
    let source_path = fp"/sys/devices/system/cpu/vulnerabilities/${name}"
    let status = process.run(process.command_argv(
      "/bin/cat", ["cat", source_path.display()], cwd: p"/",
      env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
      stdout: fp"${scratch_path}/${output_name}", stderr: fp"${scratch_path}/${output_name}-error",
    ))?
    if !status.exited_with(0) {
      return Err(check_failure(f"vulnerability ${name} raw reference command failed"))
    }
    let raw = fs.root_read_result(scratch, fp"${output_name}", max_bytes: 16384)?
    if raw.state != "observed" or raw.truncated or raw.data == null {
      return Err(check_failure(f"vulnerability ${name} raw reference is incomplete"))
    }
    var output = ""
    match (raw.data ?? b"").utf8() {
      Ok(value) => output = value
      Err(_) => return Err(check_failure(f"vulnerability ${name} raw reference is not UTF-8"))
    }
    descriptions = descriptions.push({name: name, description: output.trim()})
  }
  return Ok({descriptions: descriptions, started: started, ended: time.now()})
}

# Brackets one CPU report with complete, independently read kernel descriptions.
proc compare_live_vulnerabilities(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Bool] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  if !p"/bin/cat".exists()? {
    return Err(check_failure("vulnerability comparison needs /bin/cat"))
  }
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let before = read_vulnerability_reference(scratch, "before")?
  let scratch_path = fs.root_path(scratch)?
  fs.root_write(scratch, p"candidate", "")?
  fs.root_write(scratch, p"candidate-error", "")?
  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "cpu", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate", stderr: fp"${scratch_path}/candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate CPU collection failed"))
  }
  let after = read_vulnerability_reference(scratch, "after")?
  if before.descriptions.len() == 0 and after.descriptions.len() == 0 {
    print "cpu.vulnerabilities: reference unavailable (no vulnerability files are exported)"
    return Ok(false)
  }
  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let data = json.decode(candidate)?
  let issues = json.get(data, ["issues"])?.require(List[CandidateIssueField])?
  for item in issues {
    if item.section == "cpu" and (item.field == "vulnerabilities" or item.field.starts_with("vulnerabilities.")) {
      return Err(check_failure("candidate vulnerability source has a collection issue"))
    }
  }
  let compared = compare_vulnerabilities(candidate, before.descriptions, after.descriptions)?
  let host_claim = json.get(data, ["scope", "host_claim"])?.require(Str)?
  let agreement = if compared.exact {"exact"} else {"mismatch"}
  var reference_argv: List[List[Str]] = []
  for item in before.descriptions {
    reference_argv = reference_argv.push(["/bin/cat", f"/sys/devices/system/cpu/vulnerabilities/${item.name}"])
  }
  print f"cpu.vulnerabilities: ${agreement}; reference=${compared.reference_count}, candidate=${compared.candidate_count}, matched=${compared.matched_count}, missing=${compared.missing_names.len()}, unexpected=${compared.unexpected_names.len()}, mismatched=${compared.mismatched_names.len()}"
  print f"reference: adapter=kernel-vulnerability-files; argv=${json.encode(reference_argv)?}; output=${json.encode(before.descriptions)?}; bound=16384 bytes per file,256 entries; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${json.encode(host_claim)?}; before=${before.started}..${before.ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after.started}..${after.ended} ms"
  if !compared.exact {
    return Err(check_failure("CPU vulnerability descriptions differ from stable sysfs references"))
  }
  return Ok(true)
}

proc compare_live_swaps(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  let binary = if p"/sbin/swapon".exists()? {"/sbin/swapon"} else if p"/usr/sbin/swapon".exists()? {"/usr/sbin/swapon"} else {""}
  if binary == "" {
    print "memory.swap: reference unavailable (util-linux swapon is absent)"
    return Err(check_failure("swap comparison needs util-linux swapon"))
  }
  let argv = ["swapon", "--show=NAME,TYPE,SIZE,USED,PRIO", "--raw", "--bytes"]
  let reference_env = {PATH: "/usr/sbin:/sbin:/usr/bin:/bin", LANG: "C", LC_ALL: "C"}
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let scratch_path = fs.root_path(scratch)?
  let before_started = time.now()
  let before_status = process.run(process.command_argv(
    binary, argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/swapon-before", stderr: fp"${scratch_path}/swapon-before-error",
  ))?
  let before_ended = time.now()
  if !before_status.exited_with(0) {
    return Err(check_failure("swapon reference failed before candidate collection"))
  }
  let before = parse_swapon_raw(fs.root_read_text(scratch, p"swapon-before")?)?

  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "memory", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate", stderr: fp"${scratch_path}/candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate memory collection failed"))
  }

  let after_started = time.now()
  let after_status = process.run(process.command_argv(
    binary, argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/swapon-after", stderr: fp"${scratch_path}/swapon-after-error",
  ))?
  let after_ended = time.now()
  if !after_status.exited_with(0) {
    return Err(check_failure("swapon reference failed after candidate collection"))
  }
  let after = parse_swapon_raw(fs.root_read_text(scratch, p"swapon-after")?)?
  if !swap_reference_stable(before, after) {
    print "memory.swap: unstable (reference swap set or gauges changed around candidate collection)"
    return Err(check_failure("memory.swap could not be scored because the reference changed"))
  }

  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let candidate_data = json.decode(candidate)?
  let issues = json.get(candidate_data, ["issues"])?.require(List[CandidateIssueField])?
  for issue in issues {
    if issue.section == "memory" and issue.field == "swaps" {
      return Err(check_failure("candidate swap source has a collection issue"))
    }
  }
  let compared = compare_swap_devices(candidate, before)?
  let version = read_reference_tool_version(binary, "swapon", scratch, "swapon")?
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let host_claim_display = json.encode(host_claim)?
  let argv_display = json.encode(argv)?
  let missing_count = compared.missing_names.len()
  let unexpected_count = compared.unexpected_names.len()
  let mismatch_count = compared.field_mismatches.len()
  let agreement = if compared.exact {"exact"} else {"mismatch"}
  print f"memory.swap: ${agreement}; reference=${compared.reference_count}, candidate=${compared.candidate_count}, matched=${compared.matched_count}, missing=${missing_count}, unexpected=${unexpected_count}, field_mismatches=${mismatch_count}, kind=${compared.kind_mismatches}, size_bytes=${compared.size_mismatches}, used_bytes=${compared.used_mismatches}, priority=${compared.priority_mismatches}, candidate_field_missing=${compared.candidate_field_missing}"
  print f"reference: ${version}; executable=${binary}; argv=${argv_display}; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${host_claim_display}; before=${before_started}..${before_ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after_started}..${after_ended} ms"
  if !compared.exact {
    return Err(check_failure("memory.swap mandatory assertion failed"))
  }
  return Ok()
}

proc lspci_reference_binary() [fs, error] -> Result[Str] {
  let binary = if p"/usr/sbin/lspci".exists()? {"/usr/sbin/lspci"} else if p"/sbin/lspci".exists()? {"/sbin/lspci"} else if p"/usr/bin/lspci".exists()? {"/usr/bin/lspci"} else if p"/bin/lspci".exists()? {"/bin/lspci"} else {""}
  if binary == "" {
    return Err(check_failure("PCI comparison needs pciutils lspci"))
  }
  return Ok(binary)
}

proc read_lspci_reference(scratch: FsRoot, output_name: Str) [fs, error] -> Result[Str] {
  let raw = fs.root_read_result(scratch, fp"${output_name}", max_bytes: 16777216)?
  if raw.state != "observed" or raw.truncated or raw.data == null {
    return Err(check_failure("lspci reference output is incomplete"))
  }
  match (raw.data ?? b"").utf8() {
    Ok(value) => return Ok(value)
    Err(_) => return Err(check_failure("lspci reference output is not UTF-8"))
  }
}

# Brackets one sensitive PCI report with numeric pciutils machine-readable records.
proc compare_live_pci_identity(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  let binary = lspci_reference_binary()?
  let argv = pci_reference_argv()
  let reference_env = {PATH: "/usr/sbin:/sbin:/usr/bin:/bin", LANG: "C", LC_ALL: "C"}
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let scratch_path = fs.root_path(scratch)?
  let version_status = process.run(process.command_argv(
    binary, ["lspci", "--version"], cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/lspci-version", stderr: fp"${scratch_path}/lspci-version-error",
  ))?
  if !version_status.exited_with(0) {
    return Err(check_failure("pciutils lspci version probe failed"))
  }
  let version = read_lspci_reference(scratch, "lspci-version")?.trim()

  let before_started = time.now()
  let before_status = process.run(process.command_argv(
    binary, argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/lspci-before", stderr: fp"${scratch_path}/lspci-before-error",
  ))?
  let before_ended = time.now()
  if !before_status.exited_with(0) {
    return Err(check_failure("lspci numeric reference failed before candidate collection"))
  }
  let before = parse_lspci_vmm_numeric(read_lspci_reference(scratch, "lspci-before")?)?

  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "pci", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate", stderr: fp"${scratch_path}/candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate PCI collection failed"))
  }

  let after_started = time.now()
  let after_status = process.run(process.command_argv(
    binary, argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/lspci-after", stderr: fp"${scratch_path}/lspci-after-error",
  ))?
  let after_ended = time.now()
  if !after_status.exited_with(0) {
    return Err(check_failure("lspci numeric reference failed after candidate collection"))
  }
  let after = parse_lspci_vmm_numeric(read_lspci_reference(scratch, "lspci-after")?)?
  if !pci_reference_stable(before, after) {
    return Err(check_failure("PCI functions changed during reference capture"))
  }
  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let compared = compare_lspci_identity(candidate, before)?
  let candidate_data = json.decode(candidate)?
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let agreement = if compared.exact_static {"exact_static"} else {"mismatch"}
  print f"pci.identity.static: ${agreement}; reference=${compared.reference_count}, candidate=${compared.candidate_count}, matched=${compared.matched_count}, missing=${compared.missing_addresses.len()}, unexpected=${compared.unexpected_addresses.len()}, field_mismatches=${compared.field_mismatches.len()}, candidate_field_missing=${compared.candidate_field_missing}"
  print f"reference: ${version}; executable=${binary}; argv=${json.encode(argv)?}; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${json.encode(host_claim)?}; before=${before_started}..${before_ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after_started}..${after_ended} ms"
  if !compared.exact_static {
    return Err(check_failure("PCI numeric identity comparison failed"))
  }
  return Ok()
}

proc read_ip_json_reference(scratch: FsRoot, output_name: Str) [fs, error] -> Result[Str] {
  let raw = fs.root_read_result(scratch, fp"${output_name}", max_bytes: 16777216)?
  if raw.state != "observed" or raw.truncated or raw.data == null {
    return Err(check_failure("ip JSON reference output is incomplete"))
  }
  let decoded = (raw.data ?? b"").utf8()
  match decoded {
    Ok(_) => {}
    Err(_) => return Err(check_failure("ip JSON reference output is not UTF-8"))
  }
  return decoded?
}

proc ip_reference_binary() [fs, error] -> Result[Str] {
  let binary = if p"/usr/sbin/ip".exists()? {"/usr/sbin/ip"} else if p"/sbin/ip".exists()? {"/sbin/ip"} else if p"/usr/bin/ip".exists()? {"/usr/bin/ip"} else if p"/bin/ip".exists()? {"/bin/ip"} else {""}
  if binary == "" {
    return Err(check_failure("network comparison needs iproute2 ip with JSON output"))
  }
  return Ok(binary)
}

proc ip_reference_version(binary: Str, scratch: FsRoot) [fs, process, error] -> Result[Str] {
  let scratch_path = fs.root_path(scratch)?
  let status = process.run(process.command_argv(
    binary, ["ip", "-Version"], cwd: p"/",
    env: {PATH: "/usr/sbin:/sbin:/usr/bin:/bin", LANG: "C", LC_ALL: "C"},
    stdout: fp"${scratch_path}/ip-version", stderr: fp"${scratch_path}/ip-version-error",
  ))?
  if !status.exited_with(0) {
    return Err(check_failure("network comparison needs iproute2 ip, not a non-JSON substitute"))
  }
  let version_stdout = fs.root_read_text(scratch, p"ip-version")?.trim()
  let version_stderr = fs.root_read_text(scratch, p"ip-version-error")?.trim()
  return Ok(if version_stdout != "" {version_stdout} else {version_stderr})
}

# Brackets one network report with independently decoded iproute2 link JSON.
proc compare_live_ip_links(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  let binary = ip_reference_binary()?
  let argv = ["ip", "-json", "-details", "link", "show"]
  let reference_env = {PATH: "/usr/sbin:/sbin:/usr/bin:/bin", LANG: "C", LC_ALL: "C"}
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let scratch_path = fs.root_path(scratch)?
  let version = ip_reference_version(binary, scratch)?

  let before_started = time.now()
  let before_status = process.run(process.command_argv(
    binary, argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/ip-before", stderr: fp"${scratch_path}/ip-before-error",
  ))?
  let before_ended = time.now()
  if !before_status.exited_with(0) {
    return Err(check_failure("ip link JSON reference failed before candidate collection"))
  }
  let before = parse_ip_link_json(read_ip_json_reference(scratch, "ip-before")?)?

  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "network", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate", stderr: fp"${scratch_path}/candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate network collection failed"))
  }

  let after_started = time.now()
  let after_status = process.run(process.command_argv(
    binary, argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/ip-after", stderr: fp"${scratch_path}/ip-after-error",
  ))?
  let after_ended = time.now()
  if !after_status.exited_with(0) {
    return Err(check_failure("ip link JSON reference failed after candidate collection"))
  }
  let after = parse_ip_link_json(read_ip_json_reference(scratch, "ip-after")?)?
  if !ip_link_reference_stable(before, after) {
    return Err(check_failure("network links changed during reference capture"))
  }
  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let compared = compare_ip_links(candidate, before)?
  let candidate_data = json.decode(candidate)?
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let host_claim_display = json.encode(host_claim)?
  let argv_display = json.encode(argv)?
  let agreement = if compared.exact {"exact_static"} else {"mismatch"}
  print f"network.links.static: ${agreement}; reference=${compared.reference_count}, candidate=${compared.candidate_count}, matched=${compared.matched_count}, missing=${compared.missing_ids.len()}, unexpected=${compared.unexpected_ids.len()}, field_mismatches=${compared.field_mismatches.len()}, candidate_field_missing=${compared.candidate_field_missing}"
  print f"reference: ${version}; executable=${binary}; argv=${argv_display}; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${host_claim_display}; before=${before_started}..${before_ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after_started}..${after_ended} ms"
  if !compared.exact {
    return Err(check_failure("network link static comparison failed"))
  }
  return Ok()
}

# Brackets one network report with independently decoded iproute2 address JSON.
proc compare_live_ip_addresses(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  let binary = ip_reference_binary()?
  let argv = ["ip", "-json", "address", "show"]
  let reference_env = {PATH: "/usr/sbin:/sbin:/usr/bin:/bin", LANG: "C", LC_ALL: "C"}
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let scratch_path = fs.root_path(scratch)?
  let version = ip_reference_version(binary, scratch)?

  let before_started = time.now()
  let before_status = process.run(process.command_argv(
    binary, argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/ip-address-before", stderr: fp"${scratch_path}/ip-address-before-error",
  ))?
  let before_ended = time.now()
  if !before_status.exited_with(0) {
    return Err(check_failure("ip address JSON reference failed before candidate collection"))
  }
  let before = parse_ip_address_json(read_ip_json_reference(scratch, "ip-address-before")?)?

  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "network", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate", stderr: fp"${scratch_path}/candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate network collection failed"))
  }

  let after_started = time.now()
  let after_status = process.run(process.command_argv(
    binary, argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/ip-address-after", stderr: fp"${scratch_path}/ip-address-after-error",
  ))?
  let after_ended = time.now()
  if !after_status.exited_with(0) {
    return Err(check_failure("ip address JSON reference failed after candidate collection"))
  }
  let after = parse_ip_address_json(read_ip_json_reference(scratch, "ip-address-after")?)?
  if !ip_address_reference_stable(before, after) {
    return Err(check_failure("network addresses changed during reference capture"))
  }
  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let compared = compare_ip_addresses(candidate, before)?
  let candidate_data = json.decode(candidate)?
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let host_claim_display = json.encode(host_claim)?
  let argv_display = json.encode(argv)?
  let agreement = if compared.exact_static {"exact_static"} else {"mismatch"}
  print f"network.addresses.static: ${agreement}; reference=${compared.reference_count}, candidate=${compared.candidate_count}, matched=${compared.matched_count}, missing=${compared.missing_keys.len()}, unexpected=${compared.unexpected_keys.len()}, field_mismatches=${compared.field_mismatches.len()}, candidate_field_missing=${compared.candidate_field_missing}, lifetimes_unscored=yes"
  print f"reference: ${version}; executable=${binary}; argv=${argv_display}; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${host_claim_display}; before=${before_started}..${before_ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after_started}..${after_ended} ms"
  if !compared.exact_static {
    return Err(check_failure("network address static comparison failed"))
  }
  return Ok()
}

proc read_ip_rule_family(binary: Str, scratch: FsRoot, output_name: Str, family: Str) [fs, process, error] -> Result[List[IpRuleReference]] {
  if family != "ipv4" and family != "ipv6" {
    return Err(check_failure("ip rule reference needs an explicit family"))
  }
  let scratch_path = fs.root_path(scratch)?
  let status = process.run(process.command_argv(
    binary, network_rule_argv(family), cwd: p"/",
    env: {PATH: "/usr/sbin:/sbin:/usr/bin:/bin", LANG: "C", LC_ALL: "C"},
    stdout: fp"${scratch_path}/${output_name}", stderr: fp"${scratch_path}/${output_name}-error",
  ))?
  if !status.exited_with(0) {
    return Err(check_failure(f"ip rule JSON reference failed for ${family}"))
  }
  return parse_ip_rule_json(read_ip_json_reference(scratch, output_name)?, family)
}

# Brackets one network report with separate IPv4 and IPv6 iproute2 rule captures.
proc compare_live_ip_rules(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  let binary = ip_reference_binary()?
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let scratch_path = fs.root_path(scratch)?
  let version = ip_reference_version(binary, scratch)?
  let before_started = time.now()
  let before_ipv4 = read_ip_rule_family(binary, scratch, "ip-rule-ipv4-before", "ipv4")?
  let before_ipv6 = read_ip_rule_family(binary, scratch, "ip-rule-ipv6-before", "ipv6")?
  let before_ended = time.now()
  let before = before_ipv4.extend(before_ipv6)

  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "network", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate", stderr: fp"${scratch_path}/candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate network collection failed"))
  }

  let after_started = time.now()
  let after_ipv4 = read_ip_rule_family(binary, scratch, "ip-rule-ipv4-after", "ipv4")?
  let after_ipv6 = read_ip_rule_family(binary, scratch, "ip-rule-ipv6-after", "ipv6")?
  let after_ended = time.now()
  if !ip_rule_reference_stable(before, after_ipv4.extend(after_ipv6))? {
    return Err(check_failure("network rules changed during reference capture"))
  }
  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let compared = compare_ip_rules(candidate, before)?
  let candidate_data = json.decode(candidate)?
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let host_claim_display = json.encode(host_claim)?
  let reference_commands = [network_rule_argv("ipv4"), network_rule_argv("ipv6")]
  let agreement = if compared.exact_static {"exact_static"} else {"mismatch"}
  print f"network.rules.static: ${agreement}; reference=${compared.reference_count}, candidate=${compared.candidate_count}, matched=${compared.matched_count}, missing=${compared.missing_keys.len()}, unexpected=${compared.unexpected_keys.len()}, candidate_field_missing=${compared.candidate_field_missing}"
  print f"reference: ${version}; executable=${binary}; argv=${json.encode(reference_commands)?}; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${host_claim_display}; before=${before_started}..${before_ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after_started}..${after_ended} ms"
  if !compared.exact_static {
    return Err(check_failure("network rule static comparison failed"))
  }
  return Ok()
}

proc read_ip_route_family(binary: Str, scratch: FsRoot, output_name: Str, family: Str) [fs, process, error] -> Result[List[IpRouteReference]] {
  if family != "ipv4" and family != "ipv6" {
    return Err(check_failure("ip route reference needs an explicit family"))
  }
  let scratch_path = fs.root_path(scratch)?
  let status = process.run(process.command_argv(
    binary, network_route_argv(family), cwd: p"/",
    env: {PATH: "/usr/sbin:/sbin:/usr/bin:/bin", LANG: "C", LC_ALL: "C"},
    stdout: fp"${scratch_path}/${output_name}", stderr: fp"${scratch_path}/${output_name}-error",
  ))?
  if !status.exited_with(0) {
    return Err(check_failure(f"ip route JSON reference failed for ${family}"))
  }
  return parse_ip_route_json(read_ip_json_reference(scratch, output_name)?, family)
}

# Brackets one network report with separate IPv4 and IPv6 iproute2 route captures.
proc compare_live_ip_routes(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  let binary = ip_reference_binary()?
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let scratch_path = fs.root_path(scratch)?
  let version = ip_reference_version(binary, scratch)?
  let before_started = time.now()
  let before_ipv4 = read_ip_route_family(binary, scratch, "ip-route-ipv4-before", "ipv4")?
  let before_ipv6 = read_ip_route_family(binary, scratch, "ip-route-ipv6-before", "ipv6")?
  let before_ended = time.now()
  let before = before_ipv4.extend(before_ipv6)

  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "network", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate", stderr: fp"${scratch_path}/candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate network collection failed"))
  }

  let after_started = time.now()
  let after_ipv4 = read_ip_route_family(binary, scratch, "ip-route-ipv4-after", "ipv4")?
  let after_ipv6 = read_ip_route_family(binary, scratch, "ip-route-ipv6-after", "ipv6")?
  let after_ended = time.now()
  if !ip_route_reference_stable(before, after_ipv4.extend(after_ipv6))? {
    return Err(check_failure("network routes changed during reference capture"))
  }
  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let compared = compare_ip_routes(candidate, before)?
  let candidate_data = json.decode(candidate)?
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let host_claim_display = json.encode(host_claim)?
  let reference_commands = [network_route_argv("ipv4"), network_route_argv("ipv6")]
  let agreement = if compared.exact_static {"exact_static"} else {"mismatch"}
  print f"network.routes.static: ${agreement}; reference=${compared.reference_count}, candidate=${compared.candidate_count}, matched=${compared.matched_count}, missing=${compared.missing_keys.len()}, unexpected=${compared.unexpected_keys.len()}, candidate_field_missing=${compared.candidate_field_missing}"
  print f"reference: ${version}; executable=${binary}; argv=${json.encode(reference_commands)?}; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${host_claim_display}; before=${before_started}..${before_ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after_started}..${after_ended} ms"
  if !compared.exact_static {
    return Err(check_failure("network route static comparison failed"))
  }
  return Ok()
}

# Brackets one storage report with explicit-column util-linux block inventories.
proc compare_live_storage(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  let binary = if p"/usr/bin/lsblk".exists()? {"/usr/bin/lsblk"} else if p"/bin/lsblk".exists()? {"/bin/lsblk"} else {""}
  if binary == "" {
    print "storage.devices: reference unavailable (util-linux lsblk is absent)"
    return Err(check_failure("storage comparison needs util-linux lsblk"))
  }
  if !p"/sys/class/block".exists()? {
    return Err(check_failure("storage comparison needs mounted block sysfs"))
  }
  let argv = ["lsblk", "--all", "--json", "--bytes", "--output=NAME,KNAME,MAJ:MIN,SIZE,TYPE,PKNAME,RO,RM,ROTA,LOG-SEC,PHY-SEC"]
  let reference_env = {PATH: "/usr/sbin:/sbin:/usr/bin:/bin", LANG: "C", LC_ALL: "C"}
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let scratch_path = fs.root_path(scratch)?
  let before_started = time.now()
  let before_status = process.run(process.command_argv(
    binary, argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/lsblk-before", stderr: fp"${scratch_path}/lsblk-before-error",
  ))?
  let before_ended = time.now()
  if !before_status.exited_with(0) {
    return Err(check_failure("lsblk reference failed before candidate collection"))
  }
  let before = parse_lsblk_json(fs.root_read_text(scratch, p"lsblk-before")?)?

  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "storage", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate", stderr: fp"${scratch_path}/candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate storage collection failed"))
  }

  let after_started = time.now()
  let after_status = process.run(process.command_argv(
    binary, argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/lsblk-after", stderr: fp"${scratch_path}/lsblk-after-error",
  ))?
  let after_ended = time.now()
  if !after_status.exited_with(0) {
    return Err(check_failure("lsblk reference failed after candidate collection"))
  }
  let after = parse_lsblk_json(fs.root_read_text(scratch, p"lsblk-after")?)?
  if !block_reference_stable(before, after) {
    print "storage.devices: unstable (reference block inventory changed around candidate collection)"
    return Err(check_failure("storage.devices could not be scored because the reference changed"))
  }

  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let candidate_data = json.decode(candidate)?
  let issues = json.get(candidate_data, ["issues"])?.require(List[CandidateIssueField])?
  for issue in issues {
    if issue.section == "storage" and
        (issue.field == "devices" or issue.field.ends_with(".major_minor") or
         issue.field.ends_with(".size") or issue.field.ends_with(".sysfs_target") or
         issue.field.ends_with(".logical_sector_bytes") or issue.field.ends_with(".physical_sector_bytes") or
         issue.field.ends_with(".removable") or issue.field.ends_with(".rotational") or
         issue.field.ends_with(".read_only") or issue.field.ends_with(".holders") or
         issue.field.ends_with(".slaves")) {
      return Err(check_failure("candidate block source has a collection issue"))
    }
  }
  let compared = compare_block_devices(candidate, before)?
  let version = read_reference_tool_version(binary, "lsblk", scratch, "lsblk")?
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let host_claim_display = json.encode(host_claim)?
  let argv_display = json.encode(argv)?
  let agreement = if compared.exact {"exact"} else {"mismatch"}
  print f"storage.devices: ${agreement}; reference=${compared.reference_count}, candidate=${compared.candidate_count}, matched=${compared.matched_count}, edges=${compared.matched_edges}/${before.edges.len()}, missing=${compared.missing_names.len()}, unexpected=${compared.unexpected_names.len()}, field_mismatches=${compared.field_mismatches.len()}, missing_edges=${compared.missing_edges}, unexpected_edges=${compared.unexpected_edges}, major_minor=${compared.major_minor_mismatches}, size_bytes=${compared.size_mismatches}, sectors=${compared.sector_mismatches}, flags=${compared.flag_mismatches}, partitions=${compared.partition_mismatches}, candidate_field_missing=${compared.candidate_field_missing}"
  print f"reference: ${version}; executable=${binary}; argv=${argv_display}; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${host_claim_display}; before=${before_started}..${before_ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after_started}..${after_ended} ms"
  if !compared.exact {
    return Err(check_failure("storage.devices mandatory assertion failed"))
  }
  return Ok()
}

# Brackets queue fields with lsblk and firmware and I/O counters with bounded sysfs reads.
proc compare_live_queue(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  let binary = if p"/usr/bin/lsblk".exists()? {"/usr/bin/lsblk"} else if p"/bin/lsblk".exists()? {"/bin/lsblk"} else {""}
  if binary == "" or !p"/sys/class/block".exists()? {
    print "storage.queue: reference unavailable (lsblk or block sysfs is absent)"
    return Err(check_failure("queue comparison needs lsblk and mounted block sysfs"))
  }
  let argv = ["lsblk", "--all", "--list", "--json", "--bytes", "--output=KNAME,SCHED,RA,DISC-GRAN,DISC-MAX,MODEL,REV"]
  let reference_env = {PATH: "/usr/sbin:/sbin:/usr/bin:/bin", LANG: "C", LC_ALL: "C"}
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let source = fs.open_root(p"/")?
  defer fs.close_root(source)?
  let scratch_path = fs.root_path(scratch)?
  let before_started = time.now()
  let before_status = process.run(process.command_argv(
    binary, argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/lsblk-queue-before", stderr: fp"${scratch_path}/lsblk-queue-before-error",
  ))?
  if !before_status.exited_with(0) {
    return Err(check_failure("lsblk queue reference failed before candidate collection"))
  }
  let before = parse_lsblk_queue_json(fs.root_read_text(scratch, p"lsblk-queue-before")?)?
  let before_sources = read_block_queue_sources(source, before)?
  let before_ended = time.now()

  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "storage", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate", stderr: fp"${scratch_path}/candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate queue collection failed"))
  }

  let after_started = time.now()
  let after_status = process.run(process.command_argv(
    binary, argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/lsblk-queue-after", stderr: fp"${scratch_path}/lsblk-queue-after-error",
  ))?
  if !after_status.exited_with(0) {
    return Err(check_failure("lsblk queue reference failed after candidate collection"))
  }
  let after = parse_lsblk_queue_json(fs.root_read_text(scratch, p"lsblk-queue-after")?)?
  let after_sources = read_block_queue_sources(source, after)?
  let after_ended = time.now()
  if !block_queue_reference_stable(before, after) {
    print "storage.queue: unstable (reference queue fields changed around candidate collection)"
    return Err(check_failure("storage.queue could not be scored because lsblk values changed"))
  }

  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let candidate_data = json.decode(candidate)?
  let issues = json.get(candidate_data, ["issues"])?.require(List[CandidateIssueField])?
  for issue in issues {
    if issue.section == "storage" and
        (issue.field == "devices" or issue.field.ends_with(".scheduler") or
         issue.field.ends_with(".read_ahead_kb") or issue.field.ends_with(".discard_granularity_bytes") or
         issue.field.ends_with(".discard_max_bytes") or issue.field.ends_with(".model") or
         issue.field.ends_with(".firmware") or issue.field.ends_with(".stat") or
         issue.field.contains(".stat.")) {
      return Err(check_failure("candidate queue source has a collection issue"))
    }
  }
  let fields = compare_block_queue_fields(candidate, before)?
  let sources = compare_block_queue_sources(candidate, before_sources, after_sources)?
  let version = read_reference_tool_version(binary, "lsblk", scratch, "lsblk-queue")?
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let host_claim_display = json.encode(host_claim)?
  let argv_display = json.encode(argv)?
  let agreement = if fields.exact and sources.exact {"exact"} else if sources.unstable {"unstable"} else {"mismatch"}
  print f"storage.queue: ${agreement}; reference=${fields.reference_count}, candidate=${fields.candidate_count}, matched=${fields.matched_count}, missing=${fields.missing_names.len()}, unexpected=${fields.unexpected_names.len()}, static_field_mismatches=${fields.field_mismatches.len()}, scheduler=${fields.scheduler_mismatches}, read_ahead=${fields.read_ahead_mismatches}, discard=${fields.discard_mismatches}, model=${fields.model_mismatches}, firmware=${sources.firmware_mismatches}, counters=${sources.counter_mismatches}, counter_unstable=${sources.unstable}"
  print f"reference: ${version}; executable=${binary}; argv=${argv_display}; raw_sysfs=firmware_rev|rev,stat for each KNAME with 4096-byte bounds; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${host_claim_display}; before=${before_started}..${before_ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after_started}..${after_ended} ms"
  if sources.unstable {
    return Err(check_failure("storage.queue could not be scored because an I/O gauge or counter source changed incompatibly"))
  }
  if !fields.exact or !sources.exact {
    return Err(check_failure("storage.queue mandatory assertion failed"))
  }
  return Ok()
}

# Brackets one storage report with a flat kernel mount inventory from findmnt.
proc compare_live_mounts(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  let binary = if p"/usr/bin/findmnt".exists()? {"/usr/bin/findmnt"} else if p"/bin/findmnt".exists()? {"/bin/findmnt"} else {""}
  if binary == "" or !p"/proc/self/mountinfo".exists()? {
    print "storage.mountinfo: reference unavailable (findmnt or mountinfo is absent)"
    return Err(check_failure("mount comparison needs findmnt and readable mountinfo"))
  }
  let argv = ["findmnt", "--kernel=mountinfo", "--all", "--list", "--json", "--notruncate", "--nofsroot", "--nocanonicalize", "--output=ID,PARENT,MAJ:MIN,FSROOT,TARGET,FSTYPE,SOURCE,VFS-OPTIONS,FS-OPTIONS,PROPAGATION"]
  let reference_env = {PATH: "/usr/sbin:/sbin:/usr/bin:/bin", LANG: "C", LC_ALL: "C"}
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let scratch_path = fs.root_path(scratch)?
  let before_started = time.now()
  let before_status = process.run(process.command_argv(
    binary, argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/findmnt-before", stderr: fp"${scratch_path}/findmnt-before-error",
  ))?
  let before_ended = time.now()
  if !before_status.exited_with(0) {
    return Err(check_failure("findmnt reference failed before candidate collection"))
  }
  let before = parse_findmnt_json(fs.root_read_text(scratch, p"findmnt-before")?)?
  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "storage", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate", stderr: fp"${scratch_path}/candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate mount collection failed"))
  }
  let after_started = time.now()
  let after_status = process.run(process.command_argv(
    binary, argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/findmnt-after", stderr: fp"${scratch_path}/findmnt-after-error",
  ))?
  let after_ended = time.now()
  if !after_status.exited_with(0) {
    return Err(check_failure("findmnt reference failed after candidate collection"))
  }
  let after = parse_findmnt_json(fs.root_read_text(scratch, p"findmnt-after")?)?
  if !mount_reference_stable(before, after) {
    print "storage.mountinfo: unstable (reference mount inventory changed around candidate collection)"
    return Err(check_failure("storage.mountinfo could not be scored because the reference changed"))
  }
  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let candidate_data = json.decode(candidate)?
  let issues = json.get(candidate_data, ["issues"])?.require(List[CandidateIssueField])?
  for issue in issues {
    if issue.section == "storage" and (issue.field == "mounts" or issue.field.starts_with("mounts.line.")) {
      return Err(check_failure("candidate mount source has a collection issue"))
    }
  }
  let compared = compare_mounts(candidate, before)?
  let version = read_reference_tool_version(binary, "findmnt", scratch, "findmnt")?
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let host_claim_display = json.encode(host_claim)?
  let argv_display = json.encode(argv)?
  let agreement = if compared.exact {"exact"} else {"mismatch"}
  print f"storage.mountinfo: ${agreement}; reference=${compared.reference_count}, candidate=${compared.candidate_count}, matched=${compared.matched_count}, missing=${compared.missing_ids.len()}, unexpected=${compared.unexpected_ids.len()}, field_mismatches=${compared.field_mismatches.len()}, parents=${compared.parent_mismatches}, major_minor=${compared.identity_mismatches}, paths=${compared.path_mismatches}, filesystem=${compared.filesystem_mismatches}, source=${compared.source_mismatches}, options=${compared.option_mismatches}, propagation=${compared.propagation_mismatches}, candidate_field_missing=${compared.candidate_field_missing}"
  print f"reference: ${version}; executable=${binary}; argv=${argv_display}; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${host_claim_display}; before=${before_started}..${before_ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after_started}..${after_ended} ms"
  if !compared.exact {
    return Err(check_failure("storage.mountinfo mandatory assertion failed"))
  }
  return Ok()
}

# Queries only mount IDs selected from the independent inventory before any capacity lookup.
proc read_mount_usage_references(
  binary: Str, scratch: FsRoot, name: Str, mounts: List[MountReference],
) [fs, process, error] -> Result[List[MountUsageReference]] {
  let scratch_path = fs.root_path(scratch)?
  var observations: List[MountUsageReference] = []
  for id in mount_usage_eligible_ids(mounts) {
    let output_name = f"${name}-${id}"
    let argv = ["findmnt", "--kernel=mountinfo", "--id", f"${id}", "--df", "--all", "--list", "--json", "--bytes", "--output=ID,SIZE,USED,AVAIL"]
    let status = process.run(process.command_argv(
      binary, argv, cwd: p"/", env: {PATH: "/usr/sbin:/sbin:/usr/bin:/bin", LANG: "C", LC_ALL: "C"},
      stdout: fp"${scratch_path}/${output_name}", stderr: fp"${scratch_path}/${output_name}-error",
    ))?
    if !status.exited_with(0) {return Err(check_failure(f"findmnt capacity reference failed for mount ID ${id}"))}
    observations = observations.push(parse_findmnt_usage_json(fs.root_read_text(scratch, fp"${output_name}")?, id)?)
  }
  return observations
}

# Brackets one storage report with capacity queries for independently safe mount IDs.
proc compare_live_mount_usage(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  let binary = if p"/usr/bin/findmnt".exists()? {"/usr/bin/findmnt"} else if p"/bin/findmnt".exists()? {"/bin/findmnt"} else {""}
  if binary == "" or !p"/proc/self/mountinfo".exists()? {
    print "storage.mount-usage: reference unavailable (findmnt or mountinfo is absent)"
    return Err(check_failure("mount usage comparison needs findmnt and readable mountinfo"))
  }
  let inventory_argv = ["findmnt", "--kernel=mountinfo", "--all", "--list", "--json", "--notruncate", "--nofsroot", "--nocanonicalize", "--output=ID,PARENT,MAJ:MIN,FSROOT,TARGET,FSTYPE,SOURCE,VFS-OPTIONS,FS-OPTIONS,PROPAGATION"]
  let capacity_argv = ["findmnt", "--kernel=mountinfo", "--id", "<eligible mount ID>", "--df", "--all", "--list", "--json", "--bytes", "--output=ID,SIZE,USED,AVAIL"]
  let reference_env = {PATH: "/usr/sbin:/sbin:/usr/bin:/bin", LANG: "C", LC_ALL: "C"}
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let scratch_path = fs.root_path(scratch)?
  let before_started = time.now()
  let before_status = process.run(process.command_argv(
    binary, inventory_argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/usage-inventory-before", stderr: fp"${scratch_path}/usage-inventory-before-error",
  ))?
  if !before_status.exited_with(0) {return Err(check_failure("findmnt inventory failed before mount usage collection"))}
  let before_mounts = parse_findmnt_json(fs.root_read_text(scratch, p"usage-inventory-before")?)?
  let before_usage = read_mount_usage_references(binary, scratch, "usage-before", before_mounts)?
  let before_ended = time.now()
  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "storage", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/usage-candidate", stderr: fp"${scratch_path}/usage-candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {return Err(check_failure("candidate mount usage collection failed"))}
  let after_started = time.now()
  let after_status = process.run(process.command_argv(
    binary, inventory_argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/usage-inventory-after", stderr: fp"${scratch_path}/usage-inventory-after-error",
  ))?
  if !after_status.exited_with(0) {return Err(check_failure("findmnt inventory failed after mount usage collection"))}
  let after_mounts = parse_findmnt_json(fs.root_read_text(scratch, p"usage-inventory-after")?)?
  if !mount_reference_stable(before_mounts, after_mounts) {
    return Err(check_failure("storage.mount-usage could not be scored because the mount inventory changed"))
  }
  let after_usage = read_mount_usage_references(binary, scratch, "usage-after", after_mounts)?
  let after_ended = time.now()
  let candidate = fs.root_read_text(scratch, p"usage-candidate")?
  require_live_linux_report(candidate)?
  let candidate_data = json.decode(candidate)?
  let issues = json.get(candidate_data, ["issues"])?.require(List[CandidateIssueField])?
  for issue in issues {
    if issue.section == "storage" and (issue.field == "mounts" or issue.field.starts_with("mounts.line.")) {
      return Err(check_failure("candidate mount source has a collection issue"))
    }
  }
  let compared = compare_mount_usage(candidate, before_mounts, before_usage, after_usage)?
  let version = read_reference_tool_version(binary, "findmnt", scratch, "findmnt-usage")?
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let agreement = if compared.exact {"exact"} else if compared.unstable {"unstable"} else {"mismatch"}
  print f"storage.mount-usage: ${agreement}; eligible=${compared.eligible_count}, skipped=${compared.skipped_count}, matched=${compared.matched_count}, mismatched=${compared.mismatched_ids.len()}"
  print f"reference: ${version}; executable=${binary}; inventory_argv=${json.encode(inventory_argv)?}; capacity_argv=${json.encode(capacity_argv)?}; eligible_ids=${json.encode(mount_usage_eligible_ids(before_mounts))?}; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${json.encode(host_claim)?}; before=${before_started}..${before_ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after_started}..${after_ended} ms"
  if compared.unstable {return Err(check_failure("storage.mount-usage could not be scored because total capacity changed"))}
  if !compared.exact {return Err(check_failure("storage.mount-usage mandatory assertion failed"))}
  return Ok()
}

proc read_kernel_module_reference(binary: Str, scratch: FsRoot, name: Str) [fs, process, time, error] -> Result[KernelModuleObservation] {
  let scratch_path = fs.root_path(scratch)?
  let started = time.now()
  let formatted_status = process.run(process.command_argv(
    binary, ["lsmod"], cwd: p"/", env: {PATH: "/usr/sbin:/sbin:/usr/bin:/bin", LANG: "C", LC_ALL: "C"},
    stdout: fp"${scratch_path}/${name}-lsmod", stderr: fp"${scratch_path}/${name}-lsmod-error",
  ))?
  if !formatted_status.exited_with(0) {
    return Err(check_failure("lsmod reference failed"))
  }
  let raw_status = process.run(process.command_argv(
    "/bin/cat", ["cat", "/proc/modules"], cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
    stdout: fp"${scratch_path}/${name}-proc", stderr: fp"${scratch_path}/${name}-proc-error",
  ))?
  let ended = time.now()
  if !raw_status.exited_with(0) {
    return Err(check_failure("proc module state reference failed"))
  }
  return {
    modules: parse_lsmod_reference(
      fs.root_read_text(scratch, fp"${name}-lsmod")?,
      fs.root_read_text(scratch, fp"${name}-proc")?,
    )?,
    started: started, ended: ended,
  }
}

# Brackets a module report with lsmod and the procfs state field it omits.
proc compare_live_modules(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  let binary = if p"/sbin/lsmod".exists()? {"/sbin/lsmod"} else if p"/usr/sbin/lsmod".exists()? {"/usr/sbin/lsmod"} else {""}
  if binary == "" or !p"/bin/cat".exists()? or !p"/proc/modules".exists()? {
    print "kernel.modules: reference unavailable (lsmod, cat, or /proc/modules is absent)"
    return Err(check_failure("module comparison needs lsmod and readable /proc/modules"))
  }
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let before = read_kernel_module_reference(binary, scratch, "modules-before")?
  let scratch_path = fs.root_path(scratch)?
  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "kernel", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate", stderr: fp"${scratch_path}/candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate module collection failed"))
  }
  let after = read_kernel_module_reference(binary, scratch, "modules-after")?
  if !kernel_module_reference_stable(before.modules, after.modules) {
    print "kernel.modules: unstable (reference module set or values changed around candidate collection)"
    return Err(check_failure("kernel.modules could not be scored because the reference changed"))
  }
  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let candidate_data = json.decode(candidate)?
  let issues = json.get(candidate_data, ["issues"])?.require(List[CandidateIssueField])?
  for issue in issues {
    if issue.section == "kernel" and (issue.field == "modules" or issue.field.starts_with("modules.line.")) {
      return Err(check_failure("candidate module source has a collection issue"))
    }
  }
  let compared = compare_kernel_modules(candidate, before.modules)?
  let version = read_reference_tool_version(binary, "lsmod", scratch, "lsmod")?
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let host_claim_display = json.encode(host_claim)?
  let agreement = if compared.exact {"exact"} else {"mismatch"}
  print f"kernel.modules: ${agreement}; reference=${compared.reference_count}, candidate=${compared.candidate_count}, matched=${compared.matched_count}, missing=${compared.missing_names.len()}, unexpected=${compared.unexpected_names.len()}, field_mismatches=${compared.field_mismatches.len()}, size=${compared.size_mismatches}, users=${compared.users_mismatches}, state=${compared.state_mismatches}, candidate_field_missing=${compared.candidate_field_missing}"
  print f"reference: ${version}; executable=${binary}; argv=[\"lsmod\"] and [\"cat\",\"/proc/modules\"]; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${host_claim_display}; before=${before.started}..${before.ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after.started}..${after.ended} ms"
  if !compared.exact {
    return Err(check_failure("kernel.modules mandatory assertion failed"))
  }
  return Ok()
}

# Reads the reference command line as bounded bytes so whitespace and invalid UTF-8 remain visible.
proc read_kernel_command_line_reference(
  scratch: FsRoot, name: Str,
) [fs, process, time, error] -> Result[KernelCommandLineObservation] {
  let scratch_path = fs.root_path(scratch)?
  let started = time.now()
  let status = process.run(process.command_argv(
    "/bin/cat", ["cat", "/proc/cmdline"], cwd: p"/",
    env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
    stdout: fp"${scratch_path}/${name}", stderr: fp"${scratch_path}/${name}-error",
  ))?
  let ended = time.now()
  if !status.exited_with(0) {return Err(check_failure("proc command-line reference failed"))}
  let read = fs.root_read_result(scratch, fp"${name}", max_bytes: 65536)?
  if read.state != "observed" or read.truncated or read.data == null {
    return Err(check_failure("proc command-line reference is incomplete"))
  }
  return {data: read.data ?? b"", started: started, ended: ended}
}

# Checks both source fidelity and the default sharing-safe report path.
proc compare_live_kernel_command_line(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  if !p"/bin/cat".exists()? or !p"/proc/cmdline".exists()? {
    print "kernel.command-line: reference unavailable (cat or /proc/cmdline is absent)"
    return Err(check_failure("kernel command-line comparison needs cat and readable /proc/cmdline"))
  }
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let scratch_path = fs.root_path(scratch)?
  let before = read_kernel_command_line_reference(scratch, "cmdline-before")?
  let sensitive_started = time.now()
  let sensitive_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "kernel", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/cmdline-sensitive", stderr: fp"${scratch_path}/cmdline-sensitive-error",
  ))?
  let sensitive_ended = time.now()
  if !sensitive_status.exited_with(0) {return Err(check_failure("sensitive candidate kernel collection failed"))}
  let redacted_started = time.now()
  let redacted_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "kernel", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/cmdline-redacted", stderr: fp"${scratch_path}/cmdline-redacted-error",
  ))?
  let redacted_ended = time.now()
  if !redacted_status.exited_with(0) {return Err(check_failure("default candidate kernel collection failed"))}
  let after = read_kernel_command_line_reference(scratch, "cmdline-after")?
  if before.data != after.data {
    return Err(check_failure("kernel.command-line could not be scored because the source changed"))
  }
  let sensitive = fs.root_read_text(scratch, p"cmdline-sensitive")?
  let redacted = fs.root_read_text(scratch, p"cmdline-redacted")?
  require_live_linux_report(sensitive)?
  require_live_linux_report(redacted)?
  let compared = compare_kernel_command_line(sensitive, redacted, before.data)?
  let version = read_reference_tool_version("/bin/cat", "cat", scratch, "cat-cmdline")?
  let sensitive_data = json.decode(sensitive)?
  let host_claim = json.get(sensitive_data, ["scope", "host_claim"])?.require(Str)?
  let agreement = if compared.exact {"exact"} else {"mismatch"}
  print f"kernel.command-line: ${agreement}; source_exact=${compared.sensitive_exact}, default_redacted=${compared.redacted_exact}, candidate_field_missing=${compared.candidate_field_missing}"
  print f"reference: ${version}; executable=/bin/cat; argv=[\"cat\",\"/proc/cmdline\"]; max_bytes=65536; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${json.encode(host_claim)?}; before=${before.started}..${before.ended} ms; sensitive=${sensitive_started}..${sensitive_ended} ms; default=${redacted_started}..${redacted_ended} ms; after=${after.started}..${after.ended} ms"
  if !compared.exact {return Err(check_failure("kernel.command-line mandatory assertion failed"))}
  return Ok()
}

pure kernel_parameter_sources() -> List[KernelParameterSource] {
  return [
    {name: "kernel.pid_max", source: "sysctl", path: p"/proc/sys/kernel/pid_max"},
    {name: "kernel.threads-max", source: "sysctl", path: p"/proc/sys/kernel/threads-max"},
    {name: "vm.swappiness", source: "sysctl", path: p"/proc/sys/vm/swappiness"},
    {name: "vm.overcommit_memory", source: "sysctl", path: p"/proc/sys/vm/overcommit_memory"},
    {name: "net.ipv4.ip_forward", source: "sysctl", path: p"/proc/sys/net/ipv4/ip_forward"},
    {name: "net.ipv6.conf.all.forwarding", source: "sysctl", path: p"/proc/sys/net/ipv6/conf/all/forwarding"},
    {name: "usbcore.autosuspend", source: "module", path: p"/sys/module/usbcore/parameters/autosuspend"},
    {name: "nvme_core.default_ps_max_latency_us", source: "module", path: p"/sys/module/nvme_core/parameters/default_ps_max_latency_us"},
    {name: "intel_pstate.no_turbo", source: "module", path: p"/sys/module/intel_pstate/parameters/no_turbo"},
  ]
}

# Reads only the fixed informational keys; an absent optional key remains an explicit reference fact.
proc read_kernel_parameter_reference(
  sysctl_binary: Str, scratch: FsRoot, name: Str,
) [fs, process, time, error] -> Result[KernelParameterObservation] {
  let scratch_path = fs.root_path(scratch)?
  let started = time.now()
  var values: List[KernelParameterReference] = []
  for item in kernel_parameter_sources() |> enumerate() {
    let source = item.value
    if !source.path.exists()? {
      values = values.push({name: source.name, source: source.source, state: "absent", value: null, raw_bytes_base64: null})
      continue
    }
    let output_name = f"${name}-${item.index}"
    let binary = if source.source == "sysctl" {sysctl_binary} else {"/bin/cat"}
    let argv = if source.source == "sysctl" {["sysctl", "-n", source.name]} else {["cat", source.path.display()]}
    let status = process.run(process.command_argv(
      binary, argv, cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C"},
      stdout: fp"${scratch_path}/${output_name}", stderr: fp"${scratch_path}/${output_name}-error",
    ))?
    if !status.exited_with(0) {
      return Err(check_failure(f"kernel parameter reference failed for ${source.name}"))
    }
    let read = fs.root_read_result(scratch, fp"${output_name}", max_bytes: 4096)?
    if read.state != "observed" or read.truncated or read.data == null {
      return Err(check_failure(f"kernel parameter reference is incomplete for ${source.name}"))
    }
    let data = read.data ?? b""
    match data.utf8() {
      Ok(text) => values = values.push({
        name: source.name, source: source.source, state: "observed", value: text.trim(), raw_bytes_base64: null,
      })
      Err(_) => values = values.push({
        name: source.name, source: source.source, state: "malformed", value: null, raw_bytes_base64: data.base64(),
      })
    }
  }
  return {values: values, started: started, ended: time.now()}
}

# Brackets one sensitive kernel report with sysctl and module-parameter observations.
proc compare_live_kernel_parameters(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  let sysctl_binary = if p"/sbin/sysctl".exists()? {"/sbin/sysctl"} else if p"/usr/sbin/sysctl".exists()? {"/usr/sbin/sysctl"} else {""}
  if sysctl_binary == "" or !p"/bin/cat".exists()? {
    print "kernel.parameters: reference unavailable (sysctl or cat is absent)"
    return Err(check_failure("kernel parameter comparison needs sysctl and cat"))
  }
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let before = read_kernel_parameter_reference(sysctl_binary, scratch, "parameters-before")?
  let scratch_path = fs.root_path(scratch)?
  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "kernel", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/parameters-candidate", stderr: fp"${scratch_path}/parameters-candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {return Err(check_failure("candidate kernel parameter collection failed"))}
  let after = read_kernel_parameter_reference(sysctl_binary, scratch, "parameters-after")?
  if before.values != after.values {
    return Err(check_failure("kernel.parameters could not be scored because a reference value changed"))
  }
  let candidate = fs.root_read_text(scratch, p"parameters-candidate")?
  require_live_linux_report(candidate)?
  let compared = compare_kernel_parameters(candidate, before.values)?
  let version = read_reference_tool_version(sysctl_binary, "sysctl", scratch, "sysctl")?
  let candidate_data = json.decode(candidate)?
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let agreement = if compared.exact {"exact"} else {"mismatch"}
  print f"kernel.parameters: ${agreement}; reference=${compared.reference_count}, candidate=${compared.candidate_count}, matched=${compared.matched_count}, missing=${compared.missing_names.len()}, unexpected=${compared.unexpected_names.len()}, field_mismatches=${compared.field_mismatches.len()}, candidate_field_missing=${compared.candidate_field_missing}"
  print f"reference: ${version}; sysctl_executable=${sysctl_binary}; sysctl_argv=[\"sysctl\",\"-n\",KEY]; module_argv=[\"cat\",PATH]; keys=${json.encode(kernel_parameter_sources() |> map .name)?}; max_bytes=4096 each; locale=C; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${json.encode(host_claim)?}; before=${before.started}..${before.ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after.started}..${after.ended} ms"
  if !compared.exact {return Err(check_failure("kernel.parameters mandatory assertion failed"))}
  return Ok()
}

# Brackets one candidate observation with sysfs CPU sets and util-linux JSON.
proc compare_live_cpu_sets(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if ! xsh_bin.starts_with("/") or ! script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  if !p"/usr/bin/lscpu".exists()? {
    print "cpu.sets: reference unavailable (/usr/bin/lscpu is absent)"
    return Err(check_failure("CPU set comparison needs util-linux lscpu"))
  }
  for source_path in [
    p"/sys/devices/system/cpu/possible", p"/sys/devices/system/cpu/present",
    p"/sys/devices/system/cpu/online", p"/sys/devices/system/cpu/offline",
  ] {
    if !source_path.exists()? {
      print "cpu.sets: reference unavailable (a kernel CPU set file is absent)"
      return Err(check_failure("CPU set comparison needs four sysfs CPU set files"))
    }
  }

  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  for name in ["lscpu-before", "lscpu-after", "candidate", "lscpu-version"] {
    fs.root_write(scratch, fp"${name}", "")?
  }
  let scratch_path = fs.root_path(scratch)?
  let reference_argv = ["lscpu", "--json", "--extended=CPU,ONLINE,NODE,SOCKET,CORE"]
  let reference_env = {PATH: "/usr/bin:/bin", LANG: "C", LC_ALL: "C"}
  let before_sysfs_started = time.now()
  let before_sets = read_reference_cpu_sets()?
  let before_sysfs_ended = time.now()
  let before_started = time.now()
  let before_status = process.run(process.command_argv(
    "/usr/bin/lscpu", reference_argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/lscpu-before",
  ))?
  let before_ended = time.now()
  if !before_status.exited_with(0) {
    return Err(check_failure("lscpu reference command failed before candidate collection"))
  }

  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "cpu", "--json"],
    cwd: p"/",
    env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate CPU collection failed"))
  }

  let after_started = time.now()
  let after_status = process.run(process.command_argv(
    "/usr/bin/lscpu", reference_argv, cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/lscpu-after",
  ))?
  let after_ended = time.now()
  if !after_status.exited_with(0) {
    return Err(check_failure("lscpu reference command failed after candidate collection"))
  }
  let after_sysfs_started = time.now()
  let after_sets = read_reference_cpu_sets()?
  let after_sysfs_ended = time.now()

  let version_status = process.run(process.command_argv(
    "/usr/bin/lscpu", ["lscpu", "--version"], cwd: p"/", env: reference_env,
    stdout: fp"${scratch_path}/lscpu-version",
  ))?
  let version = if version_status.exited_with(0) {
    fs.root_read_text(scratch, p"lscpu-version")?.trim()
  } else {
    "unavailable"
  }
  let before = fs.root_read_text(scratch, p"lscpu-before")?
  let after = fs.root_read_text(scratch, p"lscpu-after")?
  let before_ids = parse_lscpu_online_cpu_ids(before)?
  let after_ids = parse_lscpu_online_cpu_ids(after)?
  if before_sets.possible != after_sets.possible or before_sets.present != after_sets.present or before_sets.online != after_sets.online or before_sets.offline != after_sets.offline or before_ids != after_ids {
    print "cpu.sets: unstable (reference CPU sets changed around candidate collection)"
    return Err(check_failure("cpu.sets could not be scored because the reference changed"))
  }
  if before_ids != before_sets.online or after_ids != after_sets.online {
    print "cpu.sets: reference disagreement (lscpu and sysfs online IDs differ)"
    return Err(check_failure("cpu.sets independent references disagree"))
  }

  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let compared = compare_cpu_sets(candidate, before_sets)?
  let candidate_data = json.decode(candidate)?
  let source_mode = json.get(candidate_data, ["source_mode"])?.require(Str)?
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let source_mode_display = json.encode(source_mode)?
  let host_claim_display = json.encode(host_claim)?
  let agreement = if compared.exact {"exact"} else {"mismatch"}
  print f"cpu.sets: ${agreement}"
  print_cpu_id_set_result("possible", compared.possible)?
  print_cpu_id_set_result("present", compared.present)?
  print_cpu_id_set_result("online", compared.online)?
  print_cpu_id_set_result("offline", compared.offline)?
  print f"reference: ${version}; argv=lscpu --json --extended=CPU,ONLINE,NODE,SOCKET,CORE; sysfs=/sys/devices/system/cpu/possible,present,online,offline; locale=C; euid=${applet.current_euid()}; source_mode=${source_mode_display}; host_claim=${host_claim_display}; before-sysfs=${before_sysfs_started}..${before_sysfs_ended} ms; before-lscpu=${before_started}..${before_ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after-lscpu=${after_started}..${after_ended} ms; after-sysfs=${after_sysfs_started}..${after_sysfs_ended} ms"
  if !compared.exact {
    return Err(check_failure("cpu.sets mandatory assertion failed"))
  }
  return Ok()
}

# Checks comparable process identity and resource values across raw procfs snapshots.
proc compare_live_processes(xsh_bin: Str, script: Str) [fs, process, time, io, error] -> Result[Unit] {
  if !xsh_bin.starts_with("/") or !script.starts_with("/") {
    return Err(check_failure("--xsh-bin and --script must be absolute paths"))
  }
  let source = fs.open_root(p"/")?
  defer fs.close_root(source)?
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let scratch_path = fs.root_path(scratch)?
  let units = system.execution_units()?
  let before_started = time.now()
  let before_identity = read_process_identity_snapshot(source)?
  let before_resources = read_process_resource_snapshot(source, units.page_size_bytes)?
  let before_ended = time.now()
  fs.root_write(scratch, p"candidate", "")?
  fs.root_write(scratch, p"candidate-error", "")?
  let candidate_started = time.now()
  let candidate_status = process.run(process.command_argv(
    xsh_bin, [xsh_bin, script, "--", "--section", "processes", "--sensitive", "--json"],
    cwd: p"/", env: {PATH: "/nonexistent", LANG: "C", LC_ALL: "C", XSH_LINUX_REAL: "1"},
    stdout: fp"${scratch_path}/candidate", stderr: fp"${scratch_path}/candidate-error",
  ))?
  let candidate_ended = time.now()
  if !candidate_status.exited_with(0) {
    return Err(check_failure("candidate process collection failed"))
  }
  let after_started = time.now()
  let after_resources = read_process_resource_snapshot(source, units.page_size_bytes)?
  let after_identity = read_process_identity_snapshot(source)?
  let after_ended = time.now()
  let candidate = fs.root_read_text(scratch, p"candidate")?
  require_live_linux_report(candidate)?
  let compared = compare_process_identity(candidate, before_identity.processes, after_identity.processes)?
  let resources = compare_process_resources(candidate, before_resources.processes, after_resources.processes)?
  if compared.stable_count == 0 or resources.scored_fields == 0 {
    return Err(check_failure("process reference has no stable identity to compare"))
  }
  let candidate_data = json.decode(candidate)?
  let candidate_page_size = json.get(candidate_data, ["scope", "page_size_bytes"])?.require(Int)?
  if candidate_page_size != units.page_size_bytes {
    return Err(check_failure("candidate process page size differs from the reference"))
  }
  let host_claim = json.get(candidate_data, ["scope", "host_claim"])?.require(Str)?
  let host_claim_display = json.encode(host_claim)?
  let agreement = if compared.exact_static {"exact"} else {"mismatch"}
  let resource_agreement = if resources.exact_scored {"exact_scored"} else {"mismatch"}
  print f"process.identity.static: ${agreement}; stable=${compared.stable_count}, unstable=${compared.unstable_count}, candidate=${compared.candidate_count}, matched=${compared.matched_count}, missing=${compared.missing_pids.len()}, mismatched=${compared.mismatched_pids.len()}, state_unscored=${compared.state_unscored_count}, before_skipped=${before_identity.skipped_count}, after_skipped=${after_identity.skipped_count}"
  print f"process.resources: ${resource_agreement}; stable=${resources.stable_count}, unstable=${resources.unstable_count}, candidate=${resources.candidate_count}, matched=${resources.matched_count}, scored_fields=${resources.scored_fields}, unscored_fields=${resources.unscored_fields.len()}, missing=${resources.missing_pids.len()}, mismatched_fields=${resources.mismatched_fields.len()}, before_skipped=${before_resources.skipped_count}, after_skipped=${after_resources.skipped_count}"
  print f"reference: adapter=procfs-rooted-v1; sources=/proc/[pid]/stat,status,statm,cgroup; bounds=16384 bytes per text source,4096 statm bytes,8192 entries; page_size_bytes=${units.page_size_bytes}; euid=${applet.current_euid()}; source_mode=live_linux; host_claim=${host_claim_display}; before=${before_started}..${before_ended} ms; candidate=${candidate_started}..${candidate_ended} ms; after=${after_started}..${after_ended} ms"
  if !compared.exact_static or !resources.exact_scored {
    return Err(check_failure("process reference comparison failed"))
  }
  return Ok()
}

## Accepts only the exact one-test success summary from an xsht invocation.
export pure fixture_single_test_passed(output: Str) -> Bool {
  if output.trim() == "" {
    return false
  }
  let lines = output.trim().lines()
  return output.contains("running 1 tests\n") and
    lines.get(lines.len() - 1, "") == "test result: ok. 1 passed; 0 failed; 0 skipped"
}

## Requires Cargo to report exactly one successful Rust fixture test.
export pure rust_fixture_single_test_passed(output: Str) -> Bool {
  if output.trim() == "" { return false }
  let lines = output.trim().lines()
  return output.contains("running 1 test\n") and
    lines.get(lines.len() - 1, "").starts_with("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;")
}

## Selects one Rust host-boundary test without running unrelated tests.
export pure rust_fixture_argv(cargo_bin: Str, test_name: Str) -> Result[List[Str]] {
  let parts = test_name.split("::")
  if parts.len() != 2 or parts[1] == "" {
    return Err(check_failure("Rust fixture has an invalid test name"))
  }
  if parts[0] == "src/modules/linux/real/netlink.rs" {
    return Ok([
      cargo_bin, "test", "--offline", "-p", "xsh", "--lib", "--target", "aarch64-unknown-linux-musl",
      f"modules::linux::real::netlink::tests::${parts[1]}", "--", "--exact", "--test-threads=1",
    ])
  }
  if parts[0] == "tests/linux_priv.rs" {
    return Ok([
      cargo_bin, "test", "--offline", "-p", "xsh", "--test", "linux_priv", "--features", "linux-priv-tests",
      "--target", "aarch64-unknown-linux-musl", parts[1], "--", "--exact", "--test-threads=1",
    ])
  }
  return Err(check_failure("Rust fixture has an unsupported owner"))
}

pure rust_fixture_name(test_name: Str) -> Bool {
  return test_name.starts_with("src/modules/linux/real/netlink.rs::") or
    test_name.starts_with("tests/linux_priv.rs::")
}

# Runs explicitly mapped fixtures and counts only cases whose tests all passed.
proc run_fixture_cases(root: Path, xsh_bin: Str, xsht_bin: Str, cargo_bin: Str, fixture_cases: List[FixtureCase]) [fs, process, io, error] -> Result[FixtureRun] {
  if !xsh_bin.starts_with("/") or !xsht_bin.starts_with("/") {
    return Err(check_failure("--xsh-bin and --xsht-bin must be absolute paths"))
  }
  var needs_cargo = false
  for fixture_case in fixture_cases {
    for test_name in fixture_case.tests {
      if rust_fixture_name(test_name) { needs_cargo = true }
    }
  }
  if needs_cargo and !cargo_bin.starts_with("/") {
    return Err(check_failure("Rust fixtures require --cargo-bin with an absolute path"))
  }
  let scratch = fs.tempdir()?
  defer fs.close_root(scratch)?
  let scratch_path = fs.root_path(scratch)?
  let stdout_path = fp"${scratch_path}/stdout"
  let stderr_path = fp"${scratch_path}/stderr"
  var passed_cases = 0
  var failed_cases = 0
  # One test may cover several scenarios; reuse its result within this fixture run.
  var test_results: Map[Bool] = {}
  var test_failures: Map[Str] = {}
  for fixture_case in fixture_cases {
    var case_passed = true
    for test_name in fixture_case.tests {
      if test_results.has(test_name) {
        if !test_results.get(test_name, false) {
          case_passed = false
          print f"fixture ${fixture_case.scenario}: failed ${test_name} (${test_failures.get(test_name, "cached failure")})"
        }
        continue
      }
      fs.root_write(scratch, p"stdout", "")?
      fs.root_write(scratch, p"stderr", "")?
      let rust_case = rust_fixture_name(test_name)
      let status = if rust_case {
        process.run(process.command_argv(
          cargo_bin, rust_fixture_argv(cargo_bin, test_name)?,
          cwd: root, stdout: stdout_path, stderr: stderr_path,
        ))?
      } else {
        process.run(process.command_argv(
          xsht_bin, [xsht_bin, "test", "--exact", "--jobs", "1", test_name],
          cwd: root,
          env: {
            PATH: "/nonexistent", LANG: "C", LC_ALL: "C", TERM: "dumb",
            CARGO_BIN_EXE_xsh: xsh_bin, CARGO_BIN_EXE_xsht: xsht_bin,
          },
          stdout: stdout_path,
          stderr: stderr_path,
        ))?
      }
      let output = fs.root_read_text(scratch, p"stdout")?
      let one_test_passed = if rust_case {rust_fixture_single_test_passed(output)} else {fixture_single_test_passed(output)}
      let passed = status.exited_with(0) and one_test_passed
      test_results = test_results.set(test_name, passed)
      if !passed {
        case_passed = false
        let output_lines = output.trim().lines()
        let result_line = output_lines.get(output_lines.len() - 1, "no test output")
        test_failures = test_failures.set(test_name, result_line)
        print f"fixture ${fixture_case.scenario}: failed ${test_name} (${result_line})"
      }
    }
    if case_passed {
      passed_cases += 1
      print f"fixture ${fixture_case.scenario}: passed"
    } else {
      failed_cases += 1
    }
  }
  print f"fixture execution: ${passed_cases} passed, ${failed_cases} failed"
  return Ok({passed: passed_cases, failed: failed_cases})
}

## Validates and summarizes the checked-in comparison denominator.
export proc validate_and_run(ctx: context.Context, args: List[Str]) [fs, process, env, time, error, io] -> Result[Unit] {
  let parsed = cli.parse(args, {
    manifest: {
      form: "--manifest FILE",
      default: "dev/system-report-coverage.json",
    },
    no_subprocess: {
      form: "--no-subprocess",
      default: false,
    },
    run_fixtures: {
      form: "--run-fixtures",
      default: false,
    },
    run_macos_fixtures: {
      form: "--run-macos-fixtures",
      default: false,
    },
    compare_cpu: {
      form: "--compare-cpu",
      default: false,
    },
    compare_vulnerabilities: {
      form: "--compare-vulnerabilities",
      default: false,
    },
    compare_meminfo: {
      form: "--compare-meminfo",
      default: false,
    },
    compare_thp: {
      form: "--compare-thp",
      default: false,
    },
    compare_swaps: {
      form: "--compare-swaps",
      default: false,
    },
    compare_pci: {
      form: "--compare-pci",
      default: false,
    },
    compare_network_links: {
      form: "--compare-network-links",
      default: false,
    },
    compare_network_addresses: {
      form: "--compare-network-addresses",
      default: false,
    },
    compare_network_rules: {
      form: "--compare-network-rules",
      default: false,
    },
    compare_network_routes: {
      form: "--compare-network-routes",
      default: false,
    },
    compare_storage: {
      form: "--compare-storage",
      default: false,
    },
    compare_queue: {
      form: "--compare-queue",
      default: false,
    },
    compare_mounts: {
      form: "--compare-mounts",
      default: false,
    },
    compare_mount_usage: {
      form: "--compare-mount-usage",
      default: false,
    },
    compare_modules: {
      form: "--compare-modules",
      default: false,
    },
    compare_command_line: {
      form: "--compare-command-line",
      default: false,
    },
    compare_parameters: {
      form: "--compare-parameters",
      default: false,
    },
    compare_identity: {
      form: "--compare-identity",
      default: false,
    },
    compare_namespaces: {
      form: "--compare-namespaces",
      default: false,
    },
    compare_processes: {
      form: "--compare-processes",
      default: false,
    },
    capture_cpu_bundle: {
      form: "--capture-cpu-bundle DIR",
      default: "",
    },
    replay_cpu_bundle: {
      form: "--replay-cpu-bundle DIR",
      default: "",
    },
    capture_memory_bundle: {
      form: "--capture-memory-bundle DIR",
      default: "",
    },
    replay_memory_bundle: {
      form: "--replay-memory-bundle DIR",
      default: "",
    },
    xsh_bin: {
      form: "--xsh-bin FILE",
      default: "",
    },
    xsht_bin: {
      form: "--xsht-bin FILE",
      default: "",
    },
    cargo_bin: {
      form: "--cargo-bin FILE",
      default: "",
    },
    script: {
      form: "--script FILE",
      default: "",
    },
  })?
  let options: CheckOptions = {
    manifest: parsed.manifest,
    no_subprocess: parsed.no_subprocess,
    run_fixtures: parsed.run_fixtures,
    run_macos_fixtures: parsed.run_macos_fixtures,
    compare_cpu: parsed.compare_cpu,
    compare_vulnerabilities: parsed.compare_vulnerabilities,
    compare_meminfo: parsed.compare_meminfo,
    compare_thp: parsed.compare_thp,
    compare_swaps: parsed.compare_swaps,
    compare_pci: parsed.compare_pci,
    compare_network_links: parsed.compare_network_links,
    compare_network_addresses: parsed.compare_network_addresses,
    compare_network_rules: parsed.compare_network_rules,
    compare_network_routes: parsed.compare_network_routes,
    compare_storage: parsed.compare_storage,
    compare_queue: parsed.compare_queue,
    compare_mounts: parsed.compare_mounts,
    compare_mount_usage: parsed.compare_mount_usage,
    compare_modules: parsed.compare_modules,
    compare_command_line: parsed.compare_command_line,
    compare_parameters: parsed.compare_parameters,
    compare_identity: parsed.compare_identity,
    compare_namespaces: parsed.compare_namespaces,
    compare_processes: parsed.compare_processes,
    capture_cpu_bundle: parsed.capture_cpu_bundle,
    replay_cpu_bundle: parsed.replay_cpu_bundle,
    capture_memory_bundle: parsed.capture_memory_bundle,
    replay_memory_bundle: parsed.replay_memory_bundle,
    xsh_bin: parsed.xsh_bin,
    xsht_bin: parsed.xsht_bin,
    cargo_bin: parsed.cargo_bin,
    script: parsed.script,
  }
  let manifest_path = context.repo_path(ctx.root, options.manifest)
  let raw = json.read(manifest_path)?
  let manifest = raw.require(CoverageManifest)?
  validate(manifest)?
  validate_fixture_test_definitions(ctx.root, manifest.fixture_cases.extend(manifest.macos_fixture_cases))?
  if options.run_fixtures and options.run_macos_fixtures {
    return Err(check_failure("choose one fixture platform gate"))
  }
  var bundle_mode_count = 0
  for value in [options.capture_cpu_bundle, options.replay_cpu_bundle, options.capture_memory_bundle, options.replay_memory_bundle] {
    if value != "" {
      bundle_mode_count += 1
    }
  }
  if bundle_mode_count > 1 {
    return Err(check_failure("choose one capture or replay bundle operation"))
  }
  if bundle_mode_count > 0 and
      (options.run_fixtures or options.run_macos_fixtures or options.no_subprocess or options.compare_cpu or options.compare_vulnerabilities or options.compare_meminfo or options.compare_thp or options.compare_swaps or options.compare_pci or options.compare_network_links or options.compare_network_addresses or options.compare_network_rules or options.compare_network_routes or options.compare_storage or options.compare_queue or options.compare_mounts or options.compare_mount_usage or options.compare_modules or options.compare_command_line or options.compare_parameters or options.compare_identity or options.compare_namespaces or options.compare_processes) {
    return Err(check_failure("bundle operations cannot be combined with fixture, live comparison, or trace operations"))
  }
  if options.capture_cpu_bundle != "" {
    let destination = path.absolute(fp"${options.capture_cpu_bundle}")?
    let parent = fs.open_root(destination.parent())?
    defer fs.close_root(parent)?
    let leaf = fp"${destination.name()}"
    if fs.root_exists(parent, leaf)? {
      return Err(check_failure("CPU set capture destination already exists"))
    }
    fs.root_mkdir(parent, leaf, mode: 0o700)?
    let bundle = fs.open_root(destination)?
    defer fs.close_root(bundle)?
    let source = fs.open_root(p"/")?
    defer fs.close_root(source)?
    capture_cpu_set_bundle(source, bundle, "live_capture")?
    let capture = json.decode(fs.root_read_text(bundle, p"capture.json")?)?.require(CpuSetCapture)?
    let scoreable = if capture.reference != null {"yes"} else {"no"}
    print f"CPU set raw capture saved at ${destination}; origin=live_capture; stable=${capture.stable}; scoreable=${scoreable}"
    print f"Replay with --replay-cpu-bundle ${destination}"
    return Ok()
  }
  if options.replay_cpu_bundle != "" {
    let bundle = fs.open_root(fp"${options.replay_cpu_bundle}")?
    defer fs.close_root(bundle)?
    let result = replay_cpu_set_bundle(bundle)?
    let capture = json.decode(fs.root_read_text(bundle, p"capture.json")?)?.require(CpuSetCapture)?
    let state = if result.exact {"exact"} else {"mismatch"}
    print f"CPU set raw replay: ${state}; origin=${capture.origin}; captured=${capture.captured_unix_ms} ms"
    print_cpu_id_set_result("possible", result.possible)?
    print_cpu_id_set_result("present", result.present)?
    print_cpu_id_set_result("online", result.online)?
    print_cpu_id_set_result("offline", result.offline)?
    if !result.exact {
      return Err(check_failure("CPU set raw replay does not match its independent reference"))
    }
    return Ok()
  }
  if options.capture_memory_bundle != "" {
    let destination = path.absolute(fp"${options.capture_memory_bundle}")?
    let parent = fs.open_root(destination.parent())?
    defer fs.close_root(parent)?
    let leaf = fp"${destination.name()}"
    if fs.root_exists(parent, leaf)? {
      return Err(check_failure("memory capture destination already exists"))
    }
    fs.root_mkdir(parent, leaf, mode: 0o700)?
    let bundle = fs.open_root(destination)?
    defer fs.close_root(bundle)?
    let source = fs.open_root(p"/")?
    defer fs.close_root(source)?
    capture_memory_bundle(source, bundle, "live_capture")?
    let capture = json.decode(fs.root_read_text(bundle, p"capture.json")?)?.require(MemoryCapture)?
    let scoreable = if capture.reference != null {"yes"} else {"no"}
    print f"memory raw capture saved at ${destination}; origin=live_capture; stable=${capture.stable}; scoreable=${scoreable}"
    print f"Replay with --replay-memory-bundle ${destination}"
    return Ok()
  }
  if options.replay_memory_bundle != "" {
    let bundle = fs.open_root(fp"${options.replay_memory_bundle}")?
    defer fs.close_root(bundle)?
    let result = replay_memory_bundle(bundle)?
    let capture = json.decode(fs.root_read_text(bundle, p"capture.json")?)?.require(MemoryCapture)?
    let meminfo_state = if result.meminfo.exact_scored {"exact"} else {"mismatch"}
    let thp_state = if result.thp.reference_count == 0 {"unavailable"} else if result.thp.exact {"exact"} else {"mismatch"}
    print f"memory raw replay: meminfo=${meminfo_state}; thp=${thp_state}; origin=${capture.origin}; captured=${capture.captured_unix_ms} ms; meminfo_fields=${result.meminfo.reference_count}; thp_fields=${result.thp.reference_count}"
    if !result.meminfo.exact_scored or (result.thp.reference_count > 0 and !result.thp.exact) {
      return Err(check_failure("memory raw replay differs from its independent reference"))
    }
    return Ok()
  }
  var mandatory_count = 0
  var cpu_sets_declared = false
  var vulnerabilities_declared = false
  var meminfo_declared = false
  var thp_declared = false
  var swaps_declared = false
  var pci_identity_declared = false
  var network_links_declared = false
  var network_addresses_declared = false
  var network_rules_declared = false
  var network_routes_declared = false
  var storage_devices_declared = false
  var storage_queue_declared = false
  var storage_mountinfo_declared = false
  var storage_mount_usage_declared = false
  var kernel_modules_declared = false
  var kernel_command_line_declared = false
  var kernel_parameters_declared = false
  var release_declared = false
  var architecture_declared = false
  var uptime_declared = false
  var os_release_declared = false
  var namespaces_declared = false
  var processes_declared = false
  for assertion in manifest.assertions {
    if assertion.tier == "mandatory" {
      mandatory_count += 1
    }
    if assertion.id == "cpu.sets" and assertion.tier == "mandatory" {
      cpu_sets_declared = true
    }
    if assertion.id == "cpu.vulnerabilities" and assertion.tier == "mandatory" {
      vulnerabilities_declared = true
    }
    if assertion.id == "memory.meminfo" and assertion.tier == "mandatory" {
      meminfo_declared = true
    }
    if assertion.id == "memory.thp" and assertion.tier == "mandatory" {
      thp_declared = true
    }
    if assertion.id == "memory.swap" and assertion.tier == "mandatory" {
      swaps_declared = true
    }
    if assertion.id == "pci.identity" and assertion.tier == "mandatory" {
      pci_identity_declared = true
    }
    if assertion.id == "network.links" and assertion.tier == "mandatory" {
      network_links_declared = true
    }
    if assertion.id == "network.addresses" and assertion.tier == "mandatory" {
      network_addresses_declared = true
    }
    if assertion.id == "network.rules" and assertion.tier == "mandatory" {
      network_rules_declared = true
    }
    if assertion.id == "network.routes" and assertion.tier == "mandatory" {
      network_routes_declared = true
    }
    if assertion.id == "storage.devices" and assertion.tier == "mandatory" {
      storage_devices_declared = true
    }
    if assertion.id == "storage.queue" and assertion.tier == "mandatory" {
      storage_queue_declared = true
    }
    if assertion.id == "storage.mountinfo" and assertion.tier == "mandatory" {
      storage_mountinfo_declared = true
    }
    if assertion.id == "storage.mount-usage" and assertion.tier == "mandatory" {
      storage_mount_usage_declared = true
    }
    if assertion.id == "kernel.modules" and assertion.tier == "mandatory" {
      kernel_modules_declared = true
    }
    if assertion.id == "kernel.command-line" and assertion.tier == "mandatory" {
      kernel_command_line_declared = true
    }
    if assertion.id == "kernel.parameters" and assertion.tier == "mandatory" {
      kernel_parameters_declared = true
    }
    if assertion.id == "identity.kernel.release" and assertion.tier == "mandatory" {
      release_declared = true
    }
    if assertion.id == "identity.kernel.architecture" and assertion.tier == "mandatory" {
      architecture_declared = true
    }
    if assertion.id == "identity.uptime" and assertion.tier == "mandatory" {
      uptime_declared = true
    }
    if assertion.id == "identity.os.release" and assertion.tier == "mandatory" {
      os_release_declared = true
    }
    if assertion.id == "identity.scope.namespaces" and assertion.tier == "mandatory" {
      namespaces_declared = true
    }
    if assertion.id == "process.identity" and assertion.tier == "mandatory" {
      processes_declared = true
    }
  }
  let mapped_fixture_cases = manifest.fixture_cases.len() + manifest.macos_fixture_cases.len()
  print f"system-report coverage manifest v${manifest.schema_version}: ${manifest.assertions.len()} declared assertions, ${manifest.fixture_scenarios.len()} declared fixture scenarios, ${mapped_fixture_cases} executable fixture cases"
  print summary(manifest.assertions)

  var scored = 0
  var partial_compared = false
  var reference_unavailable = false
  var fixture_cases_passed = 0
  if options.run_fixtures {
    if system.uname()?.sysname != "Linux" {
      return Err(check_failure("--run-fixtures requires Linux"))
    }
    if options.xsh_bin.trim() == "" or options.xsht_bin.trim() == "" {
      return Err(check_failure("--run-fixtures requires --xsh-bin and --xsht-bin"))
    }
    if manifest.fixture_cases.len() == 0 {
      return Err(check_failure("coverage manifest has no executable fixture cases"))
    }
    let fixture_run = run_fixture_cases(ctx.root, options.xsh_bin, options.xsht_bin, options.cargo_bin, manifest.fixture_cases)?
    fixture_cases_passed = fixture_run.passed
    print f"fixture coverage: ${fixture_run.passed} passed, ${fixture_run.failed} failed, ${manifest.fixture_scenarios.len() - manifest.fixture_cases.len()} declared scenarios not executed"
    if fixture_run.failed > 0 {
      return Err(check_failure("one or more mapped fixture scenarios failed"))
    }
  }
  if options.run_macos_fixtures {
    if system.uname()?.sysname != "Darwin" {
      return Err(check_failure("--run-macos-fixtures requires macOS"))
    }
    if options.xsh_bin.trim() == "" or options.xsht_bin.trim() == "" {
      return Err(check_failure("--run-macos-fixtures requires --xsh-bin and --xsht-bin"))
    }
    if manifest.macos_fixture_cases.len() == 0 {
      return Err(check_failure("coverage manifest has no macOS fixture cases"))
    }
    let fixture_run = run_fixture_cases(ctx.root, options.xsh_bin, options.xsht_bin, options.cargo_bin, manifest.macos_fixture_cases)?
    fixture_cases_passed = fixture_run.passed
    print f"macOS fixture coverage: ${fixture_run.passed} passed, ${fixture_run.failed} failed, ${manifest.fixture_scenarios.len() - manifest.macos_fixture_cases.len()} declared scenarios not executed on macOS"
    if fixture_run.failed > 0 {
      return Err(check_failure("one or more macOS fixture scenarios failed"))
    }
  }
  if options.no_subprocess {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--no-subprocess requires --xsh-bin and --script"))
    }

    audit_no_subprocess(options.xsh_bin, options.script)?
    print "process, file-mutation, and network-request traces passed for overview, full text, live JSON, CPU JSON, sensitive JSON, offline replay, version, invalid-section, and missing, malformed, unsupported-schema, and invalid-UTF-8 replay paths"
  }
  if options.compare_cpu {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-cpu requires --xsh-bin and --script"))
    }
    if !cpu_sets_declared {
      return Err(check_failure("coverage manifest lacks mandatory cpu.sets assertion"))
    }
    compare_live_cpu_sets(options.xsh_bin, options.script)?
    scored += 1
  }
  if options.compare_vulnerabilities {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-vulnerabilities requires --xsh-bin and --script"))
    }
    if !vulnerabilities_declared {
      return Err(check_failure("coverage manifest lacks mandatory cpu.vulnerabilities assertion"))
    }
    if compare_live_vulnerabilities(options.xsh_bin, options.script)? {
      scored += 1
    } else {
      reference_unavailable = true
    }
  }
  if options.compare_meminfo {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-meminfo requires --xsh-bin and --script"))
    }
    if !meminfo_declared {
      return Err(check_failure("coverage manifest lacks mandatory memory.meminfo assertion"))
    }
    compare_live_meminfo(options.xsh_bin, options.script)?
    partial_compared = true
  }
  if options.compare_thp {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-thp requires --xsh-bin and --script"))
    }
    if !thp_declared {
      return Err(check_failure("coverage manifest lacks mandatory memory.thp assertion"))
    }
    if compare_live_thp(options.xsh_bin, options.script)? {
      scored += 1
    } else {
      reference_unavailable = true
    }
  }
  if options.compare_swaps {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-swaps requires --xsh-bin and --script"))
    }
    if !swaps_declared {
      return Err(check_failure("coverage manifest lacks mandatory memory.swap assertion"))
    }
    compare_live_swaps(options.xsh_bin, options.script)?
    scored += 1
  }
  if options.compare_pci {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-pci requires --xsh-bin and --script"))
    }
    if !pci_identity_declared {
      return Err(check_failure("coverage manifest lacks mandatory pci.identity assertion"))
    }
    compare_live_pci_identity(options.xsh_bin, options.script)?
    partial_compared = true
  }
  if options.compare_network_links {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-network-links requires --xsh-bin and --script"))
    }
    if !network_links_declared {
      return Err(check_failure("coverage manifest lacks mandatory network.links assertion"))
    }
    compare_live_ip_links(options.xsh_bin, options.script)?
    partial_compared = true
  }
  if options.compare_network_addresses {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-network-addresses requires --xsh-bin and --script"))
    }
    if !network_addresses_declared {
      return Err(check_failure("coverage manifest lacks mandatory network.addresses assertion"))
    }
    compare_live_ip_addresses(options.xsh_bin, options.script)?
    partial_compared = true
  }
  if options.compare_network_rules {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-network-rules requires --xsh-bin and --script"))
    }
    if !network_rules_declared {
      return Err(check_failure("coverage manifest lacks mandatory network.rules assertion"))
    }
    compare_live_ip_rules(options.xsh_bin, options.script)?
    partial_compared = true
  }
  if options.compare_network_routes {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-network-routes requires --xsh-bin and --script"))
    }
    if !network_routes_declared {
      return Err(check_failure("coverage manifest lacks mandatory network.routes assertion"))
    }
    compare_live_ip_routes(options.xsh_bin, options.script)?
    partial_compared = true
  }
  if options.compare_storage {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-storage requires --xsh-bin and --script"))
    }
    if !storage_devices_declared {
      return Err(check_failure("coverage manifest lacks mandatory storage.devices assertion"))
    }
    compare_live_storage(options.xsh_bin, options.script)?
    scored += 1
  }
  if options.compare_queue {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-queue requires --xsh-bin and --script"))
    }
    if !storage_queue_declared {
      return Err(check_failure("coverage manifest lacks mandatory storage.queue assertion"))
    }
    compare_live_queue(options.xsh_bin, options.script)?
    scored += 1
  }
  if options.compare_mounts {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-mounts requires --xsh-bin and --script"))
    }
    if !storage_mountinfo_declared {
      return Err(check_failure("coverage manifest lacks mandatory storage.mountinfo assertion"))
    }
    compare_live_mounts(options.xsh_bin, options.script)?
    scored += 1
  }
  if options.compare_mount_usage {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-mount-usage requires --xsh-bin and --script"))
    }
    if !storage_mount_usage_declared {
      return Err(check_failure("coverage manifest lacks mandatory storage.mount-usage assertion"))
    }
    compare_live_mount_usage(options.xsh_bin, options.script)?
    scored += 1
  }
  if options.compare_modules {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-modules requires --xsh-bin and --script"))
    }
    if !kernel_modules_declared {
      return Err(check_failure("coverage manifest lacks mandatory kernel.modules assertion"))
    }
    compare_live_modules(options.xsh_bin, options.script)?
    scored += 1
  }
  if options.compare_command_line {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-command-line requires --xsh-bin and --script"))
    }
    if !kernel_command_line_declared {
      return Err(check_failure("coverage manifest lacks mandatory kernel.command-line assertion"))
    }
    compare_live_kernel_command_line(options.xsh_bin, options.script)?
    scored += 1
  }
  if options.compare_parameters {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-parameters requires --xsh-bin and --script"))
    }
    if !kernel_parameters_declared {
      return Err(check_failure("coverage manifest lacks mandatory kernel.parameters assertion"))
    }
    compare_live_kernel_parameters(options.xsh_bin, options.script)?
    scored += 1
  }
  if options.compare_identity {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-identity requires --xsh-bin and --script"))
    }
    if !release_declared or !architecture_declared or !uptime_declared or !os_release_declared {
      return Err(check_failure("coverage manifest lacks mandatory identity assertions"))
    }
    scored += compare_live_identity(options.xsh_bin, options.script)?
  }
  if options.compare_namespaces {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-namespaces requires --xsh-bin and --script"))
    }
    if !namespaces_declared {
      return Err(check_failure("coverage manifest lacks mandatory identity.scope.namespaces assertion"))
    }
    compare_live_namespaces(options.xsh_bin, options.script)?
    scored += 1
  }
  if options.compare_processes {
    if options.xsh_bin.trim() == "" or options.script.trim() == "" {
      return Err(check_failure("--compare-processes requires --xsh-bin and --script"))
    }
    if !processes_declared {
      return Err(check_failure("coverage manifest lacks mandatory process.identity assertion"))
    }
    compare_live_processes(options.xsh_bin, options.script)?
    partial_compared = true
  }
  if scored > 0 {
    print f"coverage execution: ${scored} mandatory live assertions scored, ${mandatory_count - scored} mandatory assertions not exercised"
  }
  if partial_compared {
    print "partial comparisons passed; mandatory assertions remain unscored where reference fields are incomplete or unstable"
  }
  if !options.no_subprocess and scored == 0 and fixture_cases_passed == 0 and !partial_compared and !reference_unavailable {
    print "No candidate or reference cases were run by manifest validation."
  }

  return Ok()
}
