# JSON Boundaries

JSON is a boundary format in XSH, not the internal language of a script. Decode
it at the edge, check the shape you intend to trust, and keep the rest of the
program in typed XSH values.

JSON object names remain strings. Encoding `Map[K, V]` requires `K = Str` and
returns `json-compatible` for other key domains. Convert application-owned keys explicitly
with a comprehension when a textual wire format is intended; Path display text
can lose native byte identity.

`examples/json.xsh` is the curated persistence and JSON-lines composition
showcase. `tests/xsh/stdlib/json.xsh` owns focused acceptance and error cases.

## The Default Pattern

Use a named schema when later code depends on fields having a stable shape.

```xsh
type Package = {name: Str, version: Str, files: List[Str]}

let raw = json.read(manifest_path)?
let package = raw.require(Package)?

for file in package.files {
  print f"${package.name}-${package.version}: ${file}"
}
```

`json.read` and `json.decode` return `Any` because valid JSON only proves that
the text parsed. `.require(Package)?` is the trust boundary: it checks the
runtime value, including nested named records inside collections, and gives the
checker a concrete type for the rest of the script.
Validation preserves additional object fields, including inside nested records;
encoding the checked value retains them. Required fields use the schema's
prepared order for typed access without changing JSON's object-name semantics.

Prefer this whenever the script knows what it needs. It produces better errors,
keeps field access ordinary, and avoids scattering dynamic checks through the
program.

## Enum Wire Strings

```xsh
enum State: Str { Ready = "ready", Empty = "" }
type Packet = {state: State, history: List[State]}
let packet = json.decode(input)?.require(Packet)?
json.write(output, packet)?
```

A Str-backed enum declares one unique constant string per payload-free variant.
JSON encoding and writing use those strings, including inside records and
collections. Raw decoding still returns strings. Explicit `.require(Packet)`
converts only enum slots, checks the whole value before returning it, reports
unknown strings with field and index paths, and never fills missing defaults.
Type patterns check existing enum values without conversion. Typed Map values
retain their declared key domain during conversion, including UInt checks. Raw
JSON objects can supply Str-keyed maps; numeric or other key domains require
already typed maps. JSON encoding still rejects non-Str keys, and enums do not
become a supported key domain. Ordinary enums remain incompatible with JSON.
`tests/xsh/wire-enums.xsh` covers these boundaries.

## Do Not Schema Every Temporary Value

Do not invent a named type for every throwaway JSON fragment. Add a schema where
data crosses a boundary or where later code needs stable fields.

```xsh
let event = {
  service: "worker",
  event: "done",
  ok: status.ok,
}

json.write(log_path, event)?
```

`json.write_lines` also accepts an existing concrete list of JSON-compatible
records. Serialization reads its values without converting the list to
`List[Any]`; container assignments retain their invariant element domains.

A list literal supplied directly to a declared JSON `List[Any]` parameter can
contain heterogeneous values. Checking retains each child's original type and
JSON eligibility instead of unifying the children into one inferred item type.
Splices retain their own element domain. Dynamic children are validated during
encoding; statically incompatible children fail checking. This admission belongs
to the canonical JSON parameter contract and does not widen an ordinary local
list or an explicit `List[Int]`. `check_graph_special_module_call` and
`validate_json_literal_boundaries` own this distinction.

The record is already typed in XSH. A separate `Event` type is useful only if
the script will read the value back, accept it from another process, or pass it
through an API that depends on that shape.

## Dynamic JSON Tools

Some programs are about unknown JSON itself: formatters, filters, validators,
diff tools, recursive walkers, and compatibility adapters. Those programs need
to branch on runtime shape because there is no single schema to require.

Use type-pattern matching on `Any` for that case:

```xsh
pure scalar_label(v: Any) -> Result[Str] {
  match v {
    n is Null => return Ok("null")
    b is Bool => return Ok(if b { "true" } else { "false" })
    i is Int => return Ok(f"integer ${i}")
    f is Float => return Ok(f"float ${f}")
    s is Str => return Ok(f"string ${s.count_chars()}")
    _ => return Err(Error(kind: "json-type", message: "expected scalar JSON"))
  }
}
```

This is different from a `type_name()` string helper. A type pattern both tests
the runtime value and narrows the binding inside the arm. That keeps the dynamic
case explicit without turning ordinary typed code into string comparisons.

Prefer `.require(Type)?` for known shapes and keep generic dynamic code
localized behind helper functions.

## Dynamic Fields

For known object shapes, require a schema before accessing fields.

```xsh
type User = {id: Int, name: Str}

let user = json.decode(input)?.require(User)?
print f"${user.id}: ${user.name}"
```

For genuinely dynamic object access, keep the dynamic operation visible.

```xsh
let value = json.decode(input)?
let name = json.get(value, ["name"], null)
```

That says the field name is data. If the field is part of the contract, use a
schema instead.

## Practical Rule

Use `.require(Type)?` when the program knows the shape it needs. Use dynamic
matching only when the program is intentionally operating on unknown JSON
shapes. Treat `Any` as a short-lived boundary value, not as the normal way to
model application data.

## System Report Snapshots

`core/lib/system_report.xsh::SystemReport` keeps observation, section, and
source states as Str-backed nominal enums. JSON v1 uses the `SystemReportJson` wire
schema and stable lower snake case strings for those union values. Use
`encode_report_json` and `decode_report_json` at this boundary; the decoder
rejects unknown state spellings and schema versions. The encoder applies the
same default redaction to JSON that the report renderer uses unless the caller
explicitly selects sensitive output.
`SectionStatus.enumeration_succeeded` describes the section's primary entity
source, such as the `present` CPU list or top-level class directory. A nested
directory can fail after that source succeeds; the section then retains its
primary enumeration result while `state` becomes `partial` and an issue names
the failed nested source.

Live collection currently reports `live_linux` and describes the
process-visible source view and namespaces in `ObservationScope`; it does not
claim that the process sees a physical host or the host's outer namespaces.
The scope records mount, network, PID, cgroup, UTS, IPC, user, and time namespace
symlink identities when `/proc/self/ns` exposes them. `FsRoot.readlink_result`
preserves absence, permission failure, and other read failures as distinct
observation states with field-addressed issues. Default output redacts the
symlink targets while retaining each observation state.
The v1 decoder accepts earlier reports that lack the four newer namespace
fields and restores them as `unsupported`; a present malformed field is rejected.
`container_live` and `physical_live` remain distinct source modes for captures
whose provenance establishes those boundaries.

`render_text` applies that redaction by default and escapes terminal controls,
bidi controls, and line separators in untrusted text. The default redaction
also removes source-root paths and sensitive observation payloads while
preserving numeric relationship indexes. It removes PCI bus addresses, USB
port and device addresses, block-device names and numbers, and their copied
model or firmware labels. Numeric PCI/USB vendor, product, and class IDs and
indexed parent, holder, slave, and mount relationships remain available.
`IdentitySection.kernel_build` is withheld by default because `/proc/version`
may contain a builder user and hostname; a field-addressed `redacted` issue
distinguishes withholding it from an absent observation.
Mount options retain only a fixed set of generic flag names; option values,
unknown flags, and unknown optional mount fields become `redacted` because they
can carry host paths, credentials, and policy labels. Numeric propagation tags
remain available in the default report to preserve mount relationships.
Collection and replay also remove unknown or credential-bearing mount options
before sensitive output; a small allowlist retains validated common values
such as `lowerdir` and `errors`, plus numeric mount propagation tags.
`--sensitive` never restores removed options.
Mount sources containing userinfo or credential-like option text are withheld
at collection and replay boundaries even in sensitive output.
Sensor channel labels are redacted while numeric readings and channel kinds
remain available.
`SensorChannel.chip_entry_name` identifies the hwmon class entry; `chip` is
the kernel's display name and can repeat across devices. Earlier v1 reports
without the entry replay with null identity. `parent_pci_function_index` and
`parent_usb_device_index` resolve its class device link against inventories
retained by a sensors-only report; older v1 reports replay with null links.
Known entry and channel pairs must be unique, and the generic class entry
remains visible when labels are redacted. Present parent indexes must resolve
inside the retained PCI and USB arrays.
`--compare-hwmon` brackets a sensitive sensors report with
`read_hwmon_reference`, independently reading bounded raw class attributes
and device links. It compares input values, kernel labels, units, thresholds,
alarms, and indexed PCI/USB parents only when both surrounding reads agree.
An input value observed only during collection is recorded as unstable even
when the surrounding reads happen to match;
configured `sensors -j` labels and scaling are separate corroboration.
The opt-in `--compare-sensors-json --sensors-bin ABSOLUTE_FILE` checker runs
`sensors -j -c /dev/null` around a sensitive sensors report. It compares only
uniquely mapped raw `*_input` names after libsensors unit conversion, keeps
changed, duplicate, or differing live readings partial, and reports command version,
timing, and output hashes. It does not add to the mandatory raw hwmon score.
`--capture-thermal-bundle NEW_DIRECTORY` saves bounded zone and indexed trip
source bytes, absence states, digests, an independent parsed reference, and a
stability observation in a private directory. `--replay-thermal-bundle DIRECTORY`
validates those bytes and runs `collect_from_root` with the sensors section
before comparing the zones. A changing live zone tree is marked unstable, but
complete saved bytes remain replayable. Observed sources with error metadata,
absent classes, and incomplete sources are unscoreable. Replay revalidates the
saved bytes after production collection.
`collect_sensors` keeps unfamiliar `*_input` channels with `kind: "unknown"`
and `unit: "raw"` so their integer values are not silently assigned a known
physical unit. A missing thermal class does not erase observed hwmon channels.
Truncated hwmon chip names fall back to the class entry name. Truncated thermal
zone and trip names remain unavailable, with field issues rather than
publishing a valid-looking prefix.
`ThermalTrip.index` preserves the `trip_point_N` identity, including sparse
indexes, and trips are ordered by that index. Duplicate, descending, negative,
or JSON-unsafe indexes are rejected at the v1 boundary. Older v1 reports with
unnamed trip positions replay with null indexes; a zone cannot mix known and
legacy-unknown trip indexes. Text output shows each trip's index, type,
temperature, and hysteresis. Noncanonical thermal-zone and trip-point entry
names remain field issues instead of merging distinct paths under one index.
`dev/system_report_check.xsh::read_thermal_zone_reference` separately reads
bounded thermal sysfs attributes and compares stable zone and trip identities;
changing temperatures prevent an exact live score.
`collect_power` leaves battery fields null when a supply exports charge
measurements but no energy measurements; it does not infer one from the other.
`--compare-power-supplies` brackets a sensitive power report with
`read_power_supply_reference` and `compare_power_supplies`. The reference
reads each optional sysfs attribute independently and preserves charge in
microampere-hours, energy in microwatt-hours, voltage in microvolts, and signed
current in microamperes. An absent attribute remains absent; a failed read or
changing measurement leaves only that field unscored. A powercap enumeration
issue does not invalidate a complete supply comparison.
`--compare-powercap` uses `read_powercap_reference` to identify zones by their
class entry and constraints by their numeric index. `compare_powercap` checks
parent links, names, limits, windows, and the maximum counter range across a
before/report/after bracket. It scores `energy_uj` only inside a nonwrapping
counter interval; a wrap, incomplete read, or changing setting leaves that
field partial. An unrelated power-supply issue does not prevent powercap
scoring.
Sensor inputs, thresholds, thermal readings, battery measurements, and
power-cap counters reject incomplete reads, invalid numeric text, and integers
outside the exact JSON range. Power-cap values are nonnegative; signed sensor
temperatures and supply currents retain their sign. A capacity percentage
outside 0–100 is malformed. Invalid fields remain null with a field-addressed
issue carrying the source or range state, rather than treating a valid-looking
prefix as a measurement.
`core/lib/system_report_collect.xsh::bounded_number` owns this source-aware
numeric boundary. It accepts decimal digits and an optional minus sign only
for signed fields; radix prefixes, plus signs, and separators are malformed.
The other procfs/sysfs decimal fields use the same lexical restriction;
PCI's explicitly hexadecimal identifiers use a separate parser.
A top-level power-cap zone has no parent; a nested zone
records its containing zone's sysfs entry name in `parent`; `entry_name`
identifies the zone independently of its display name. Class entries without
a zone `name` are control types and do not become fake zones. Class symlink
targets identify parent zones; repeated class and hierarchy paths produce one
zone record. `constraints` retains every indexed power limit in ascending
order with its optional name and time window. Duplicate, descending, negative,
or JSON-unsafe indexes are rejected at the report boundary. A missing or
invalid reading leaves that field null and adds an issue without removing
other constraints. A truncated zone name falls back to `entry_name`; truncated
constraint and power-supply text stays null with field issues. Older v1 reports
with single constraint fields replay as one index-0 constraint. Their missing
kernel entry name falls back to the saved display name, so an old parent link
may remain unresolved. Failed power-cap class enumeration makes the section
partial and retains a `cap_zones` issue.
Issue details and dynamic device identifiers in field paths receive the same
redaction. Redaction reduces exposure but does not guarantee anonymity.
`ObservationScope.page_size_bytes` and
`clock_ticks_per_second` record the runtime units used when interpreting
process page and clock-tick counters. Cgroup resource values carry a unit and
keep maximum, current, quota, and period observations separate; an unlimited
maximum is represented explicitly. Mount inventory is independent of usage:
`usage_state` is `not_requested` when filesystem capacity was skipped by the
fixture policy or local-filesystem eligibility policy, and exact byte counters
are present only when a rooted `statvfs` query succeeded. `parse_cpu_list`
expands sparse Linux ranges, sorts the identifiers, rejects duplicates and
malformed ranges, and limits the expanded list to 65,536 identifiers.
`core/lib/system_report_collect.xsh::bounded_size_bytes` accepts complete
cache-size observations with K, M, or G suffixes and checks the scaled byte
value against the exact JSON integer range. Truncated or oversized cache sizes
remain null with a field issue.
`core/lib/system_report_collect.xsh::read_source_text` retains the state and
error classification of a bounded read, but exposes decoded text only when the
source was completely observed. A truncated prefix cannot supply a scalar
field even if the prefix parses as a valid value.
CPU section enumeration succeeds only when the complete `present` source
parses; a valid `possible` list alone does not establish which CPUs were
enumerated. CPU records follow `present`, including when CPU 0 is only
possible and is absent from the current system. Missing, malformed, or
truncated CPU-list sources leave a field-addressed issue. Effective cgroup CPU
sets require complete membership, mount, and `cpuset.cpus.effective` reads
before a parsed list is reported as observed. Malformed or duplicate unified
membership rows leave the effective set unavailable with a field issue.
The independent checker parses one `Cpus_allowed_list` from bounded
`/bin/cat /proc/self/status` reads around a full sensitive report.
`--compare-cpu-scope` also resolves the current unified cgroup from bounded
membership and mountinfo reads, then reads `cpu.max` and
`cpuset.cpus.effective` for the current group and each visible ancestor.
The checker recognizes a cgroup2 mount by its mountinfo filesystem field;
an incomplete cgroup2 row fails the reference instead of appearing absent.
A complete mount outside the membership path leaves the cgroup2 comparison
ineligible because it exposes no current-group resource files.
It scores `memory.cpu-scope` only when affinity and both current-group CPU
controller files are inspectable, all stable reference values agree with
typed CPU and cgroup resources, and the visible ancestor set is unchanged.
No hidden ancestor is inferred.
Repeated `Cpus_allowed_list` fields in a complete status source make
`cpu.affinity` unavailable with a malformed issue; the collector does not
select one of the conflicting values.
CPU model, vendor, and feature enrichment from `/proc/cpuinfo` requires a
complete read. An incomplete source leaves these fields unavailable and keeps
a `cpuinfo` issue without changing the independently enumerated CPU ID set.
Package, die, and core IDs require complete reads within the exact JSON integer
range. Truncated or malformed thread-sibling lists cannot establish a sibling
relationship. Each failed topology source retains its field issue.
`Cpu.numa_node` identifies each logical CPU's observed node; `Cpu.cache_ids`
links to deduplicated cache records by observed kernel cache ID and sharing
membership, so one cache can remain shared across CPUs assigned to different
NUMA nodes.
An observed but malformed cache `shared_cpu_list` leaves `shared_cpus` empty
and adds a field issue. That cache is linked only to its owner CPU; the report
does not claim a wider sharing relationship from an ambiguous source.
Cache level and type must be complete and valid before a cache instance enters
the typed inventory; incomplete values retain field issues and leave valid
neighboring caches intact. Cache line size and set count may be null when their
sources are absent, truncated, malformed, or outside the exact JSON range.
The `cpu.cache-sharing` live comparison reads each present CPU's cacheinfo
directory through `read_cpu_cache_reference`. `compare_cpu_cache_sharing`
deduplicates matching shared instances, checks their sizes and metadata, and
requires every participating CPU's `cache_ids` link. Aggregate `lscpu --caches`
output does not expose those per-instance CPU relationships, so it cannot
establish exact sharing agreement by itself.
Kernel cacheinfo `id` distinguishes cache instances of the same level and
type, even when their shared CPU lists match. The collector uses this source
when present to keep those instances separate; report `CpuCache.id` remains a
snapshot-local link identifier. Exact live cache sharing requires observed
kernel IDs for every reference entry. Without them, the comparison reports an
incomplete reference instead of assuming that equal CPU maps prove identity.
The `cpu.topology` live comparison uses `lscpu`'s explicit CPU, socket, core,
and node columns, restricted by an independent kernel `present` mask.
`compare_lscpu_topology` compares package and core membership groups instead
of numeric socket/core labels, and checks each
reported thread-sibling set against its reference core group. It scores only
when every reference CPU has the topology columns needed for that comparison.
CPU directory enumeration keeps failures and truncation as issues for each
present CPU, its cache and idle-state directories, and the vulnerability
directory. Absent optional cache, idle-state, and vulnerability directories do
not imply a failed read; an absent directory for a CPU named by `present` does.
Once a vulnerability entry is listed, an absent, failed, or truncated content
read retains that entry and adds a `vulnerabilities.<name>` issue.
`--capture-vulnerabilities-bundle NEW_DIRECTORY` saves the bounded named
kernel files, source states, digests, and an independent description snapshot
in a private directory. Exact replay requires at least one complete file and
at most 256 KiB of combined raw description bytes across the 256-entry,
16 KiB-per-file limits. `--replay-vulnerabilities-bundle DIRECTORY` validates
the complete saved file set, recollects the CPU section from those raw files,
and compares the descriptions. An absent class or incomplete file remains
unscoreable. Replay checks the saved bytes and capture metadata again after
production collection.
Present CPUFreq and CPUIdle attributes retain field issues when their reads
fail; a failed optional value remains unavailable beside valid neighboring
policy or idle-state values.
`CpuIdleState.state_index` is the numeric `stateN` directory identity for one
CPU; the displayed state name is metadata and need not identify a state.
Older v1 saved reports lacking this field replay with a null index instead of
an inferred directory identity. Current reports reject duplicate or invalid
indexed states at the JSON boundary. A malformed or inexact `stateN` directory
name is skipped with a field issue, so one bad name cannot invalidate the
entire report.
`global_idle_governor` reads `current_governor_ro` when `current_governor` is
absent, preserving the kernel's read only governor interface.
The `--compare-cpuidle` checker reads each present CPU's `stateN` directories
independently through `read_cpuidle_reference`. `compare_cpuidle` checks state
identity, names, configured controls, and microsecond latency/residency values
against stable surrounding reads. The cumulative `usage_count` and `time_us`
must lie within the surrounding kernel counter values. Changing metadata or
state presence leaves the mandatory assertion unscored and identifies the
unstable field or state.
CPUFreq `related_cpus` and `affected_cpus` use the kernel's space-separated
CPU IDs; the policy retains offline members in `related_cpus` while
`affected_cpus` names online members. The live checker reads policy directories
independently through `read_cpufreq_policy_reference` and scores exact policy
membership through `compare_cpufreq_policies`. Configured bounds are compared
only where both surrounding reference reads agree. `scaling_current_khz` is
the value of `scaling_cur_freq`, which may describe the last requested P-state
rather than a measured hardware frequency. V1 replay accepts the former
`requested_current_khz` key and emits `scaling_current_khz`. The checker reads
`cpuinfo_cur_freq`, `scaling_cur_freq`, and `cpuinfo_avg_freq` around collection.
When a policy uses the `userspace` governor, it also reads `scaling_setspeed`
into `governor_requested_khz`; other governors leave that field null because
the kernel does not provide a functional `scaling_setspeed` value for them.
Current gauges score only when both reads are complete and agree with the
candidate. A changed or unreadable gauge, including a temporary `EAGAIN`,
leaves `cpu.freq-bounds` partial; an absent gauge is distinct from an incomplete
read. The reference also reads each policy's EPP value and available choices
plus the global boost control,
preferring `cpufreq/boost` over `intel_pstate/no_turbo`. EPP and boost count as
exact only when at least one control is exposed and the surrounding reads
agree. `boost_allowed` reflects the control setting; `boost_active` stays null
because these controls do not measure active boosting.
The opt-in `--compare-cpupower --cpupower-bin ABSOLUTE_FILE` checker invokes
versioned `cpupower` commands for CPU 0 only. It selects the candidate policy
by `related_cpus` membership, so a sparse policy directory index is valid.
It corroborates policy driver
and hardware limits plus the global CPUIdle driver, governor, and ordered state
names. It does not treat its changing usage or duration counters as exact
reference values, and it contributes only a supplemental assertion.
Block-device `holders` and `slaves` relationships retain a field issue when
their directories cannot be completely enumerated, including after a device
disappears. Partial child names remain available without claiming a complete
relationship set. A block class entry's symlink observation distinguishes a
disappeared device from a failed link read; a directly rooted class directory
is accepted without inventing a parent target.
Block major/minor identity, size, scheduler selection, and I/O counters are
parsed only from complete source reads. An incomplete source leaves the derived
field unavailable with a field issue. Major/minor identifiers and byte sizes
outside the exact JSON integer range are rejected with range issues. Sector
counts are bounded before multiplying by 512, and individual I/O counters
outside that range are omitted while valid neighboring counters remain.
Block sector counts require nonnegative decimal source text, including when
their numeric value would otherwise round to zero.
An observed scheduler row must have exactly one bracketed active choice and no
duplicate choices. Malformed rows leave `active_scheduler` null and
`available_schedulers` empty with a `storage.devices.<name>.scheduler` issue;
unknown scheduler names remain valid when their selection is unambiguous.
The block `stat` reader retains the available read, write, discard, and flush
fields in kernel order through field 17. `in_flight` is a gauge of requests
currently active; the other exported values are cumulative counters. If a
future kernel appends fields, the known prefix remains available with an
unsupported-field issue rather than a false truncated-read classification.
Optional numeric block attributes also withhold incomplete prefixes rather
than publishing them as observed values. Failed reads of present attributes
retain field issues, while absent optional attributes remain unavailable.
Readable queue and block-state attributes use nonnegative decimal integers
within the exact JSON range;
`removable`, `rotational`, and `read_only` accept only `0` or `1`.
Malformed or unsafe values leave their typed fields null and add field issues.
Memory collection keeps failed or truncated global huge-page, NUMA-node, and
per-node huge-page directory observations as issues. Missing optional
directories remain absent capabilities rather than empty successful scans. A
NUMA node that was enumerated but has no readable `meminfo` also retains its
source state as an issue. Malformed `node*` names do not create synthetic
numeric node identities.
Sensor chips, thermal zones, and power-cap zones keep attribute-directory
failures as issues even when their top-level class directory was enumerated
successfully. Partial entries remain in the report.
An empty `/proc/meminfo` file, malformed rows, and missing numeric values produce field-addressed
`CollectionIssue` records; invalid byte units and values beyond the safe integer
range used for JSON interchange leave the corresponding host-memory field unavailable.
The bound is applied after KiB-to-byte conversion, including for vendor counters;
unscaled counters have the same exact JSON integer bound. A truncated meminfo
read is not parsed, even when its prefix contains complete-looking rows.
Space and tab separators are accepted between `/proc/meminfo` values and units.
If a field name occurs more than once, none of its rows or named host-memory
value is published; a `duplicate_field` issue identifies the ambiguous field.
Valid fields beside it remain available.
`validate_cpu_set_bundle` and `validate_memory_bundle` reject saved sources
marked observed while also carrying an error number or error kind. Their replay
paths verify raw digests and independent reference values again after the
production collector runs, so a bundle changed during replay cannot score.
The memory bundle permits a changing live meminfo source at capture time
because its saved raw snapshot remains independently replayable.
`/proc/swaps` is also parsed only after a complete read. Its tab-separated rows
carry KiB counters, which must fit the exact JSON integer range after byte
conversion; an out-of-range or malformed row produces a `swaps` issue without
adding a device. The expected column header must be present before any rows
are accepted. Swap names decode the kernel's octal path escapes so spaces
and other escaped characters retain their original identity. A duplicate
decoded name or used count above the size is a malformed row; valid neighboring
devices remain available.
`dev/system_report_check.xsh::compare_swap_devices` requires a requested memory
section and no `swaps` source issue before scoring even an empty inventory.
`capture_proc_swaps_bundle` retains one bounded raw source with its read state,
digest, and an independent KiB-to-byte oracle. `replay_proc_swaps_bundle`
validates the saved bytes and reruns the production memory collector; an absent,
malformed, truncated, or changed source remains unscoreable. The mandatory live
assertion still compares against the bracketed byte-valued `swapon` reference.
`KernelSection.modules` is populated only after a complete `/proc/modules`
read. An incomplete source leaves enumeration unsuccessful and records a
`modules` issue; a complete-looking prefix is not published as an inventory.
Malformed rows and module sizes or user counts outside the exact JSON integer
range remain row issues while valid neighboring modules stay available.
When the kernel exports `-` because module unloading is disabled,
`KernelModule.users` is null; the module remains in the inventory and text
output displays the count as unknown. A numeric zero remains distinct from an
unavailable count.
An optional seventh `/proc/modules` word contains module taint flags; it does
not change the modeled name, size, use count, or state. Rows with further
words are malformed. `dev/system_report_check.xsh::parse_proc_modules_raw_reference`
and `parse_lsmod_reference` apply the same row bound before comparing modules.
Duplicate module names produce a row issue and do not add a second identity;
valid neighboring modules remain available. The raw reference rejects duplicate
names before scoring a capture.
`KernelSection.command_line` preserves the complete bounded `/proc/cmdline`
source in sensitive reports, including whitespace that separates or surrounds
tokens. Invalid UTF-8 is retained as base64 with a `malformed` state and a
field issue. Default reports redact either payload form. The live
`dev/system_report_check.xsh::compare_live_kernel_command_line` assertion
compares both output modes with bracketed raw source bytes.
`dev/system_report_check.xsh::capture_kernel_command_line_bundle` saves the
bounded source bytes, read state, digest, and an exact byte reference in a
private directory because the raw command line can contain secrets.
`replay_kernel_command_line_bundle` verifies the saved bytes and reruns the
production kernel collector in both output modes before scoring fidelity and
redaction. An observed source is scoreable only when its saved error number and
error kind are both absent.
`KernelSection.sysctls` has six fixed keys: `kernel.pid_max`,
`kernel.threads-max`, `vm.swappiness`, `vm.overcommit_memory`,
`net.ipv4.ip_forward`, and `net.ipv6.conf.all.forwarding`.
`KernelSection.parameters` has `usbcore.autosuspend`,
`nvme_core.default_ps_max_latency_us`, and `intel_pstate.no_turbo`.
Each entry keeps its observation state when the source is absent or fails;
an unreadable sysctl keeps its key with a null `permission_denied` value and a
field-addressed issue while readable neighboring keys remain observed.
Default reports redact observed values. The live
`dev/system_report_check.xsh::compare_live_kernel_parameters` assertion
brackets sensitive output with named `sysctl -n` and bounded module-file
readings, and requires the full fixed entry set.
`capture_kernel_parameters_bundle` saves only those nine bounded source paths
with absence/error states, byte digests, and an independently decoded reference.
`replay_kernel_parameters_bundle` validates the saved sources and invokes the
production kernel collector before comparing all nine entries. Sources that
change between capture reads, are tampered with after capture, or are truncated
or unreadable do not receive an exact replay score.
Global and NUMA huge-page pools use the same reader. Their directory size in
KiB is bounded before byte conversion, the total count requires a complete
nonnegative source observation, and invalid optional counts remain null with
field issues. An invalid size or total omits the pool.
NUMA `meminfo` rows are accepted only from a complete node source and only
when their embedded node ID matches the directory. Their `kB` counters use
the same exact byte bound as host `meminfo`; malformed or oversized rows keep
field issues without entering `MemorySection.numa`.
Pressure stall rows require a complete `/proc/pressure/{cpu,memory,io}` read,
one `some` or `full` row per kind, all three fixed two-decimal percentage
averages, and a nonnegative JSON-safe cumulative microsecond total. Malformed
rows remain issues. An absent pressure source is recorded as absent rather
than producing a zero measurement. A present but empty or whitespace-only
source is malformed and contributes no pressure row.
`capture_pressure_bundle` saves one bounded read of each pressure source in a
private directory, with source states, byte digests, capture time, and an
independently parsed row snapshot. Cumulative totals can change between live
reads, so the saved snapshot is replayed without a live stability claim.
`validate_pressure_bundle` requires every present source to be complete and
parseable and rejects contradictory observed/error metadata; absent resources
remain absent. `replay_pressure_bundle` reruns the production memory collector
on those saved files, checks their bytes again afterward, and compares every
row and absence state. No exported pressure rows means no exact score.
`transparent_huge_pages` keeps the complete source choice string in JSON v1.
`core/lib/system_report_collect.xsh::parse_thp_policy` exposes the selected and
available values without assuming a fixed kernel vocabulary. The live
collector accepts only a complete read with exactly one bracketed selected
value; an incomplete or malformed source contributes an issue instead of a
policy string.
`ProcessRecord.cgroup` contains the parsed unified cgroup path rather than the
raw `/proc` row; `cgroup_resource_index` resolves an exact path match in the
collected cgroup resource list and remains null when no match was collected or
when section projection removes the memory section. A v1-only membership is
`unsupported`; a relative or duplicate unified path is `malformed` and does
not become an observed path.
`ProcessRecord.uid` is the real numeric UID, the first value in the complete
four-column `Uid:` status row; effective and saved UIDs are not represented by
this field. A missing, duplicate, or malformed row leaves `uid` null with a
field issue. Space and tab separators are both accepted.
Process records never include per-process environment or command-line sources,
even in sensitive JSON. The observed command name comes from `stat`; the
collector does not read `/proc/[pid]/environ` or `/proc/[pid]/cmdline`.
Process identity requires complete `stat` reads on both sides of collection.
PID, parent PID, and start ticks must fit the exact JSON integer range;
oversized identities omit the process and retain an issue. Invalid optional
`stat` counters remain null with field issues. The [kernel's seven-field
`statm` row](https://docs.kernel.org/filesystems/proc.html) must be complete
with decimal fields before its first two page counts are used; unused fields
may exceed the exact JSON integer range. The first two page counts use the
reported page size and the exact byte bound. An observed but malformed or
overflowing `statm` field stays null even when a separate `stat` fallback
exists. When `statm` is incomplete, independently observed `stat` counters
may supply fallback values; the source issue still records the incomplete
read. Incomplete `status` and `cgroup` reads leave their derived fields
unavailable and retain source issues; complete-looking prefixes do not become
observed UIDs or cgroup paths.
The cgroup inventory requires complete membership and mountinfo reads before
following the visible v2 hierarchy.
`system_report_collect.xsh::parse_unified_cgroup_path` splits the two
membership separators without dropping colons in the cgroup pathname.
Malformed or duplicate unified rows prevent resource collection. A legacy
membership row retains the v1 limitation even if its mount is hidden. Memory,
CPU scope, and process collection use this same parsed membership path.
`system_report_collect.xsh::select_cgroup_mount` chooses the longest visible
cgroup v2 mount root containing that path, so an unrelated earlier mount
cannot hide the applicable resource files. A mountinfo row without its
separator or required fields makes the cgroup inventory malformed and cannot
supply resource or effective CPU-set values.
Memory, swap, and PID maximum/current values are checked independently for
complete decimal observations within the exact JSON integer range. `max` is
retained as an explicit unlimited maximum. The resource state reflects an
invalid or incomplete field, and a field issue explains the withheld value.
CPU quota/period and selected CPU and I/O counters use the same bound.
Truncated `cpu.stat`, `io.stat`, or effective cpuset reads cannot publish
complete-looking prefix records. Duplicate `cpu.stat` names and duplicate
`io.stat` device or counter identities are withheld with malformed issues;
independent neighboring counters remain available.
An `io.stat` row containing only a valid device ID has no reported counters;
it creates neither a resource nor a malformed issue. The independent
`--compare-cgroup-v2` checker reads the visible hierarchy through bounded
rooted sources, compares resource identities and stable limits, brackets
cumulative CPU and I/O counters, and leaves changing current-use gauges
unscored. It does not turn an unscored gauge into an exact mandatory match.
When v1 membership or mounts are visible, observed v2 resources remain
available and an `unsupported` issue identifies the v1 limitation.
`IdentitySection.os_release` reads `/etc/os-release`, falling back to
`/usr/lib/os-release` only when the first file is absent. Quoted shell-style
escapes are decoded by `core/lib/system_report_collect.xsh::decode_os_release_value`
as data; variables and commands are never evaluated.
Unquoted values accept only ASCII letters, digits, dot, underscore, and dash;
other punctuation and spaces require a quoted value. Leading or trailing
assignment whitespace is not silently trimmed into a different value.
Malformed recognized values remain unavailable with field issues while valid
neighbors are retained; malformed assignment lines produce line issues.
Comments and unknown keys are ignored. Repeated valid keys use the later
value, as required for readers of the [os-release format](https://github.com/systemd/systemd/blob/main/man/os-release.xml).
A malformed duplicate retains the last valid value and records a field issue.
An observed file without a nonempty valid `ID` keeps its other decoded fields
but records `os_release.ID` as malformed and leaves the identity section
partial. `ID` permits only lowercase ASCII letters, digits, dot, underscore,
and dash after decoding, including when the source value is quoted. An invalid
later `ID` does not erase an earlier valid value. The report does not insert
the format's optional generic `linux` default because that value was not
observed in the selected source.
`dev/system_report_check.xsh::capture_os_release_bundle` retains both source
paths and their bounded read states in a private raw bundle. Its saved
`selected_path` follows the same local-file precedence. Validation checks
source digests, rejects observed/error metadata contradictions, and reparses
`ID` and `VERSION_ID`; replay then calls
`SystemReportLiveCollector.collect_from_root` on the saved files and compares
the product's identity to that independent parse. It validates the saved
sources again after collection. Raw bundle replay paths also require exact
`capture.json` bytes to remain unchanged during collection, so a changed
origin or source observation cannot pass when its decoded oracle is the same.
A changed capture or an
unreadable selected source is not an exact identity observation.
Incomplete local release files do not trigger fallback or contribute parsed
fields. Uptime and device-tree compatible values also require complete reads;
failed optional firmware sources retain field issues without publishing a
complete-looking prefix.
Present DMI placeholder strings remain observed raw values in
`IdentitySection.firmware`. The collector does not interpret vendor-specific
placeholder spellings as absent hardware identity.
`--compare-identity` uses `read_dmi_identity_reference` and
`compare_dmi_identity` to bracket raw DMI vendor, product, board, BIOS,
serial, and UUID attributes around one sensitive identity report. Missing
attributes stay absent and unreadable attributes leave their fields partial.
`capture_dmi_identity_bundle` saves only those eight bounded class attributes
with source states, digests, capture time, and an independent raw-value
reference in a private directory because serial and UUID can identify a host.
`replay_dmi_identity_bundle` checks the saved files again after running the
production identity collector in sensitive and default modes. Exact replay
requires stable, complete source reads, at least an observed vendor or product,
and default redaction of present serial and UUID values. Missing optional
attributes remain absent.
When DMI vendor or product is exposed alongside device tree strings, the
firmware source remains `dmi`; the device tree values are still compared by
`compare_device_tree`. A device tree-only host can satisfy
`identity.firmware` through that bounded comparison. Source selection uses
the independently read DMI vendor and product, and candidate firmware values
must agree with their presence or absence. A model or compatible
field issue prevents an exact match even when the field value looks absent.
`capture_device_tree_bundle` saves the bounded `model` and `compatible` bytes,
source states, digests, capture time, and an independently decoded value oracle.
`replay_device_tree_bundle` validates the saved sources before and after running
the production identity collector. Exact replay requires stable, complete reads
of both paths, with at least one observed valid source. An absent model is
allowed when a valid compatible list is present. The saved compatible order is
part of the comparison.
`IdentitySection.uptime_seconds` is a whole-second observation from a single
read. Live checking brackets it with independent readings before and after
collection; a changing second is accepted only when the reported value lies
within that interval.
`capture_uptime_bundle` saves one bounded `/proc/uptime` read, its source state,
digest, capture time, and an independently parsed whole-second value. The raw
reference requires exactly two decimal columns and a JSON-safe uptime second.
`replay_uptime_bundle` verifies the saved bytes before and after invoking the
production identity collector and compares the candidate second to the saved
one. A missing, malformed, truncated, or changed source is unscoreable; live
uptime is not required to remain constant across separate reads.
Device-tree `model` is one NUL-terminated string and `compatible` is an
ordered list of NUL-terminated strings, following the
[DeviceTree specification](https://devicetree-specification.readthedocs.io/en/latest/chapter2-devicetree-basics.html).
`core/lib/system_report_collect.xsh::decode_device_tree_strings`
rejects missing terminators and empty elements; the identity and firmware
collectors retain malformed field issues. A valid compatible list establishes
device-tree firmware identity when the model file is absent and no DMI table is
exported.
SMBIOS records retain unknown type numbers and handles. Type 17's size
sentinel remains a raw field; an extended size is present only when all four
bytes belong to the formatted record, so following strings cannot supply
missing size bytes. `core/lib/system_report_live.xsh::parse_smbios_table`
owns this boundary.
Type 16's device count occupies bytes 13 and 14 even in the 15-byte form of
the record; the collector reads it without requiring later optional fields.
`--compare-smbios` brackets a sensitive firmware report with bounded reads
of the exported DMI table. `dev/system_report_smbios_check.xsh::parse_smbios_reference`
separately walks length-delimited structures and their double-null string
sets, then `compare_smbios` checks record types, handles, formatted lengths, selected
raw fields, and string bytes. A changed table, incomplete record, or invalid
string index remains partial. The raw comparison follows the
[DMTF SMBIOS structure rules](https://www.dmtf.org/sites/default/files/standards/documents/DSP0134_3.9.0.pdf);
`dmidecode` interpretation is separate corroboration.
`xsh dev system-report-check --capture-smbios-bundle NEW_DIRECTORY` saves the
bounded kernel-exported DMI table in a private directory with source state,
SHA-256 digest, capture origin, and an independently parsed reference. It also
retains the optional `smbios_entry_point` bytes and their source state and
digest. An observed table or entry point cannot carry read-error metadata;
the optional entry point may be absent while the table remains scoreable.
`dev/system_report_smbios_check.xsh::craft_dmidecode_dump` relocates a
valid SMBIOS 2 or 3 entry point and places the table at offset `0x20`, matching
the [dmidecode saved-dump format](https://manpages.debian.org/testing/dmidecode/dmidecode.8.en.html).
`--corroborate-smbios-bundle DIRECTORY --dmidecode-bin ABSOLUTE_FILE` explicitly
runs the selected utility on that dump inside the development harness. Its
bounded hexadecimal output is compared with raw record identities, selected
formatted fields, and string bytes. The private bundle retains version, argv,
locale, UID, timing, exit status, output digests, and the output itself.
Corroboration validates the saved bundle again after the utility exits and
rejects changes to its raw sources or capture metadata.
This supplemental result does not alter the mandatory raw SMBIOS score.
`--replay-smbios-bundle DIRECTORY` checks the saved bytes and reruns
`core/lib/system_report_live.xsh::collect_from_root` on that root before
comparing records. An absent, changing, truncated, or malformed table remains
unscored. Raw SMBIOS strings can contain serial numbers and other identifiers,
so captured bundles must stay outside the tracked tree.
Replay validates the saved table, optional entry point, independent oracle,
and capture metadata again after production collection. A changed bundle
cannot receive an exact score.
`PciFunction.address` includes the function number, so functions of one PCI
device remain separate even when their numeric vendor and device IDs agree.
Numeric PCI identity comes from the rooted sysfs attributes and remains
available without an installed label database or helper command.
Absent PCI driver and IOMMU links mean unbound or ungrouped functions. Failed
link reads and failed bus-entry parent reads retain field issues, so null does
not hide a source failure. `-1` in `numa_node` means unknown and becomes null.
Absent optional PCIe link files leave their fields null; a present malformed
link width also records a field issue and makes the PCI section partial.
Truncated current or maximum link-speed reads retain field issues and cannot
publish their complete-looking prefixes as link speeds.
PCIe widths are unsigned decimal integers within JSON's exact integer range;
the unknown NUMA sentinel does not create a field issue.
An invalid or non-UTF-8 bus entry name records an address issue while valid
neighboring functions remain in the inventory.
`core/lib/system_report_collect.xsh::collect_pci` owns those source states.
`dev/system_report_check.xsh::read_pci_link_reference` independently reads the
four exported link attributes for every visible PCI function. A link comparison
requires stable before and after values, checks nulls for unexposed fields, and
scores `pci.link` only when at least one function exposes a link attribute.
`dev/system_report_check.xsh::read_pci_binding_reference` independently reads
the PCI bus-entry, driver, and IOMMU links plus NUMA values. It compares the
candidate's parent indexes by resolving them to BDFs and scores `pci.binding`
only when the surrounding source snapshots are stable and complete.
`BlockDevice.parent_device_index` describes block stacking, while
`parent_pci_function_index` points to an enumerated PCI controller when the
block class-entry symlink path resolves to one. The block device's own `device`
link can point to an intermediate subsystem object such as an NVMe controller.
Default redaction preserves both indexed
relationships.
An enumerated block device remains in `StorageSection.devices` when its `dev`
number is absent or malformed; its major/minor fields are null and a
field-addressed issue explains the missing identity.
Block model and firmware text require complete observations. A truncated or
failed source retains its state and a field issue without publishing a prefix;
the firmware fallback is used only when the primary revision path is absent.
Block I/O counters accept only complete 11, 15, or 17-field kernel `stat`
layouts. An incomplete layout publishes no counters and records a `stat`
issue; a longer future layout preserves the known 17-field prefix and records
an unsupported-field issue.
`StorageSection.mounts` is populated only from a complete
`/proc/self/mountinfo` observation. A truncated read can end after valid rows;
publishing that prefix would make a partial mount namespace look complete, so
the collector retains a `storage.mounts` issue and withholds the mount list.
Mount IDs, parent IDs, and device major/minor numbers must fit the exact JSON
integer range; invalid rows retain field issues and cannot enter the mount
list. Any malformed mount row also disables capacity path traversal for that
inventory, since an omitted ancestor could change which filesystem a target
resolves through.
`--capture-mountinfo-bundle NEW_DIRECTORY` saves up to 4 MiB of raw
`/proc/self/mountinfo` bytes, source state, digest, origin, and a separately
decoded mount oracle in a private directory. Bundle schema v2 retains exact
optional propagation fields after credential-safe sanitization; unknown future
fields remain redacted entries.
`--replay-mountinfo-bundle DIRECTORY` validates the source and oracle, runs the
production storage collector on those bytes without capacity traversal, and
compares mount IDs, relationships, paths, options, propagation class and group
IDs. The live `findmnt` comparison can score only propagation class. A changing live namespace is
recorded in `stable` but does not invalidate complete saved bytes. Empty,
incomplete, malformed, or oversized-oracle captures remain unscoreable; replay
rechecks both raw bytes and exact capture metadata after collection.
Capacity reads through `FsRoot.filesystem_stats` are limited to local
filesystems whose mount ancestors are also local and whose target paths are
unique in the visible mount inventory. A shadowed target can resolve to a
different mount, and a local child beneath an automount can trigger its
ancestor during path traversal. These entries retain `not_requested` usage
with null byte values. The live reference in
`dev/system_report_check.xsh::compare_live_mount_usage` selects safe mount IDs
from an independent `findmnt` inventory before querying capacity by ID; it
checks stable total bytes and brackets changing used and available bytes.
The rooted stats operation accepts both directory mounts and regular file bind
mounts; its Linux descriptor is opened with `O_PATH` so a capacity query does
not read target data or block on a special file.
When `findmnt` reports all three capacity fields as null for an inaccessible
local target, the reference expects a non-observed usage state with null bytes;
a transition between available and unavailable observations is unscored.
`select_report_section` retains identity,
the selected section, and PCI/USB sections required to preserve the selected
USB, network, or device-class relationships. It clears other sections and
their issues and marks them `not_requested` with unsuccessful enumeration
states.
`core/lib/system_report_collect.xsh::parse_pci_address` preserves the PCI
domain, bus, device, and function components during collection. Default
redaction clears the address and components; sensitive output retains them.
`collect_pci` resolves `parent_function_index` from the PCI bus-entry symlink
target; the child function's `device` attribute is its numeric device ID.
`collect_usb` resolves `controller_pci_index` from the USB bus-entry symlink
target and leaves it null when the controller is platform-attached. It resolves
`parent_device_index` after enumerating all devices because a root hub's sysfs
name may sort after its children. Missing or malformed numeric vendor/product
IDs leave those fields null with a field-addressed issue; the device remains in
the inventory. Scalar USB fields such as numeric IDs, bus numbers, speed, and
power controls are published only from complete observed source reads. A
truncated prefix retains its field issue but cannot become a complete value.
The `--compare-usb-topology` checker independently enumerates USB device names
through `read_usb_topology_reference`, skipping interface entries. It derives
root-hub and parent identities from kernel device names and checks the report's
indexed parents, port paths, bus and device numbers, and speed only when the
surrounding sysfs readings agree. A device added, removed, or renumbered during
collection leaves `usb.topology` partial. This check requires sensitive output
because default redaction removes USB location identifiers.
The `--compare-usb-ids` checker reads fixed-width hexadecimal vendor,
product, device-class, subclass, and protocol attributes independently by
device name. It also compares the raw `bcdDevice` string and observed or absent
manufacturer and product strings. Changing or unreadable sources remain
unscored. The collector records a field issue for a failed read of any USB
identity attribute and for malformed fixed-width IDs or decimal device fields,
so an unreadable optional label cannot look absent.
USB power-control and autosuspend files may be absent. A present file that
cannot be read leaves the value null, records a field issue, and makes the USB
section partial. `UsbDevice.runtime_status` holds the optional
`power/runtime_status` value; v1 replay inserts null when an older report lacks
the field. `--compare-usb-power` independently brackets power control,
autosuspend delay in signed milliseconds, runtime status, and configuration
values. Changing runtime state or an unreadable source leaves the assertion
partial.
`UsbAlternateSetting.configuration_value` identifies the descriptor
configuration that owns the setting. Endpoint descriptors attach to the most
recent interface setting, so matching interface and alternate numbers in
different configurations remain distinct from the kernel's active
configuration observation. The decoder rejects configuration lengths that
leave a child descriptor outside its owning configuration.
`--compare-usb-interfaces` uses `read_usb_interface_reference` to read each
active interface's decimal alternate setting, hexadecimal
interface/class/endpoint-count attributes, and driver link. Its separate
`parse_usb_interface_descriptors` decoder checks exported configuration,
interface, and endpoint descriptors by device and interface number.
`compare_usb_interfaces` checks available settings independently of the active
setting and counts a changed or unreadable source as partial. Exact scoring
requires at least one visible interface and stable descriptor reads.
The opt-in `--compare-lsusb --lsusb-bin ABSOLUTE_FILE` checker reads numeric
device IDs from `lsusb`, displayed devices and interface drivers from
`lsusb -t`, and numeric descriptor fields from one bounded `lsusb -v -s`
capture selected from the utility's own device list. A changing utility
inventory is unscored. This supplement does not replace the raw sysfs and
descriptor assertions.
The route-netlink report keeps unknown numeric enum values in explicit
`*_N` strings and stores each attribute payload as base64 in
`NetworkAttribute.data`. Routes preserve multipath entries as typed
`NetworkNexthop` records with interface index, kernel flags, raw hop weight, and
an optional gateway observation; a single output interface stays in
`output_ifindex` and does not create a synthetic nexthop. Addresses attach to
links by interface index, retaining each link's source order, and IPv6 policy
rules retain their source prefix, table, mark/mask, and interface relation.
For a `goto` rule, `NetworkRule.attributes` retains the four-byte `FRA_GOTO`
priority as kind 4; sensitive JSON exposes its base64 bytes and the independent
rule checker decodes that target before comparing rule identity. The v1 named
rule fields do not include a separate jump-target field.
Link kind retains the kernel's exported name, while master and lower-link
relationships use interface indexes; an unfamiliar kind does not erase either
relationship.
Link byte counters above the exact JSON integer range are withheld while the
link remains visible, with a field-addressed `range_failure` issue.
Default redaction preserves attribute kinds and
relationship indices while marking payload observations `redacted`; sensitive
JSON retains their base64 bytes. Network section states distinguish denied,
unsupported, malformed, raced, and truncated dumps from empty successful
enumerations. `enumeration_succeeded` requires all four netlink dumps to finish
and every entity to decode; a reported field issue can still make the section
partial without losing enumeration success.
`link_network_device_sources` joins interface device links to
PCI and USB inventories; absent optional links leave the network section
complete, while denied and failed link reads retain a field issue and errno.
USB interface and device-class driver bindings use `optional_driver_name`:
an absent link means unbound, while failed reads retain their state and errno.
`usb_controller_address` preserves a disappeared bus entry and a failed
controller lookup separately; a directly rooted USB device directory remains
valid without an inferred PCI controller.
`class_parent_target` uses a class-entry link when a `device` link is absent;
if the `device` link fails, the report keeps any independently observed parent
and records the link failure as a field issue.
Input and sound class names fall back to their stable class entry when an
optional name read is incomplete. Allowlisted class attributes are published
only from complete reads; failed reads remain field issues.
`DeviceClassRecord.entry_name` preserves the sysfs class entry separately
from `name`, because different sound or input entries may share a display
label. It remains visible in the default report while the label is redacted;
indexed PCI and USB parents retain their meaning. Earlier v1 documents without
`entry_name` replay with an `unsupported` observation. Observed class and
entry pairs must be unique, and present parent indexes must resolve within
the same report. `--compare-device-classes` brackets a sensitive
device report with `read_device_class_reference` and checks stable entries,
labels, and indexed PCI/USB parents against class sysfs links. Incomplete or
changing source reads leave the affected comparison partial.
The USB descriptor parser bounds input to 1 MiB, validates every descriptor
length, and preserves unknown descriptor bytes for later typed interpretation.

`system-report --from FILE` uses the same decoder and renderer for saved
snapshots. It reads at most 16 MiB. Replay does not query the current host;
`--section` projects the decoded report before rendering, and `--json` emits
one v1 JSON document. Default text includes source scope and section status,
groups CPUFreq policies only when their observed settings match, and reports
how many mount capacity queries ran or were skipped. `--full` adds per-item
detail without changing which observations were collected.
