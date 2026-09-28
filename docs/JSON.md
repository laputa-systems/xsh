# JSON Boundaries

JSON is a boundary format in XSH, not the internal language of a script. Decode
it at the edge, check the shape you intend to trust, and keep the rest of the
program in typed XSH values.

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

Prefer this whenever the script knows what it needs. It produces better errors,
keeps field access ordinary, and avoids scattering dynamic checks through the
program.

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
source states as closed tag unions. JSON v1 uses the `SystemReportJson` wire
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
symlink identities when `/proc/self/ns` exposes them. `fs.root_readlink_result`
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
`collect_sensors` keeps unfamiliar `*_input` channels with `kind: "unknown"`
and `unit: "raw"` so their integer values are not silently assigned a known
physical unit. A missing thermal class does not erase observed hwmon channels.
Truncated hwmon chip names fall back to the class entry name. Truncated thermal
zone and trip names remain unavailable, with field issues rather than
publishing a valid-looking prefix.
`collect_power` leaves battery fields null when a supply exports charge
measurements but no energy measurements; it does not infer one from the other.
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
CPU section enumeration succeeds only when the complete `present` source
parses; a valid `possible` list alone does not establish which CPUs were
enumerated. CPU records follow `present`, including when CPU 0 is only
possible and is absent from the current system. Missing, malformed, or
truncated CPU-list sources leave a field-addressed issue. Effective cgroup CPU
sets require complete membership, mount, and `cpuset.cpus.effective` reads
before a parsed list is reported as observed.
CPU model, vendor, and feature enrichment from `/proc/cpuinfo` requires a
complete read. An incomplete source leaves these fields unavailable and keeps
a `cpuinfo` issue without changing the independently enumerated CPU ID set.
`Cpu.numa_node` identifies each logical CPU's observed node; `Cpu.cache_ids`
links to deduplicated cache records by shared CPU set, so one cache can remain
shared across CPUs assigned to different NUMA nodes.
CPU directory enumeration keeps failures and truncation as issues for each
present CPU, its cache and idle-state directories, and the vulnerability
directory. Absent optional cache, idle-state, and vulnerability directories do
not imply a failed read; an absent directory for a CPU named by `present` does.
Once a vulnerability entry is listed, an absent, failed, or truncated content
read retains that entry and adds a `vulnerabilities.<name>` issue.
Present CPUFreq and CPUIdle attributes retain field issues when their reads
fail; a failed optional value remains unavailable beside valid neighboring
policy or idle-state values.
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
`/proc/swaps` is also parsed only after a complete read. Its tab-separated rows
carry KiB counters, which must fit the exact JSON integer range after byte
conversion; an out-of-range or malformed row produces a `swaps` issue without
adding a device. The expected column header must be present before any rows
are accepted. Swap names decode the kernel's octal path escapes so spaces
and other escaped characters retain their original identity.
`KernelSection.modules` is populated only after a complete `/proc/modules`
read. An incomplete source leaves enumeration unsuccessful and records a
`modules` issue; a complete-looking prefix is not published as an inventory.
Malformed rows and module sizes or user counts outside the exact JSON integer
range remain row issues while valid neighboring modules stay available.
`KernelSection.command_line` preserves the complete bounded `/proc/cmdline`
source in sensitive reports, including whitespace that separates or surrounds
tokens. Invalid UTF-8 is retained as base64 with a `malformed` state and a
field issue. Default reports redact either payload form. The live
`dev/system_report_check.xsh::compare_live_kernel_command_line` assertion
compares both output modes with bracketed raw source bytes.
`KernelSection.sysctls` has six fixed keys: `kernel.pid_max`,
`kernel.threads-max`, `vm.swappiness`, `vm.overcommit_memory`,
`net.ipv4.ip_forward`, and `net.ipv6.conf.all.forwarding`.
`KernelSection.parameters` has `usbcore.autosuspend`,
`nvme_core.default_ps_max_latency_us`, and `intel_pstate.no_turbo`.
Each entry keeps its observation state when the source is absent or fails;
an unreadable sysctl keeps its key with a null `permission_denied` value and a
field-addressed issue while readable neighboring keys remain observed.
default reports redact observed values. The live
`dev/system_report_check.xsh::compare_live_kernel_parameters` assertion
brackets sensitive output with named `sysctl -n` and bounded module-file
readings, and requires the full fixed entry set.
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
than producing a zero measurement.
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
following the visible v2 hierarchy. Memory, swap, and PID maximum/current
values are checked independently for complete decimal observations within
the exact JSON integer range; `max` is retained as an explicit unlimited
maximum. The resource state reflects an invalid or incomplete field, and a
field issue explains the withheld value. CPU quota/period and selected CPU
and I/O counters use the same bound. Truncated `cpu.stat`, `io.stat`, or
effective cpuset reads cannot publish complete-looking prefix records.
When v1 controllers share the visible mount view, observed v2 resources remain
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
Incomplete local release files do not trigger fallback or contribute parsed
fields. Uptime and device-tree compatible values also require complete reads;
failed optional firmware sources retain field issues without publishing a
complete-looking prefix.
Present DMI placeholder strings remain observed raw values in
`IdentitySection.firmware`. The collector does not interpret vendor-specific
placeholder spellings as absent hardware identity.
`IdentitySection.uptime_seconds` is a whole-second observation from a single
read. Live checking brackets it with independent readings before and after
collection; a changing second is accepted only when the reported value lies
within that interval.
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
`PciFunction.address` includes the function number, so functions of one PCI
device remain separate even when their numeric vendor and device IDs agree.
Numeric PCI identity comes from the rooted sysfs attributes and remains
available without an installed label database or helper command.
Absent optional PCIe link files leave their fields null; a present malformed
link width also records a field issue and makes the PCI section partial.
An invalid or non-UTF-8 bus entry name records an address issue while valid
neighboring functions remain in the inventory.
`core/lib/system_report_collect.xsh::collect_pci` owns those source states.
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
Capacity reads through `fs.root_filesystem_stats` are limited to local
filesystems whose mount ancestors are also local and whose target paths are
unique in the visible mount inventory. A shadowed target can resolve to a
different mount, and a local child beneath an automount can trigger its
ancestor during path traversal. These entries retain `not_requested` usage
with null byte values. The live reference in
`dev/system_report_check.xsh::compare_live_mount_usage` selects safe mount IDs
from an independent `findmnt` inventory before querying capacity by ID; it
checks stable total bytes and brackets changing used and available bytes.
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
the inventory.
USB power-control and autosuspend files may be absent. A present file that
cannot be read leaves the value null, records a field issue, and makes the USB
section partial.
`UsbAlternateSetting.configuration_value` identifies the descriptor
configuration that owns the setting. Endpoint descriptors attach to the most
recent interface setting, so matching interface and alternate numbers in
different configurations remain distinct from the kernel's active
configuration observation. The decoder rejects configuration lengths that
leave a child descriptor outside its owning configuration.
The route-netlink report keeps unknown numeric enum values in explicit
`*_N` strings and stores each attribute payload as base64 in
`NetworkAttribute.data`. Routes preserve multipath entries as typed
`NetworkNexthop` records with interface index, kernel flags, raw hop weight, and
an optional gateway observation; a single output interface stays in
`output_ifindex` and does not create a synthetic nexthop. Addresses attach to
links by interface index, retaining each link's source order, and IPv6 policy
rules retain their source prefix, table, mark/mask, and interface relation.
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
The USB descriptor parser bounds input to 1 MiB, validates every descriptor
length, and preserves unknown descriptor bytes for later typed interpretation.

`system-report --from FILE` uses the same decoder and renderer for saved
snapshots. It reads at most 16 MiB. Replay does not query the current host;
`--section` projects the decoded report before rendering, and `--json` emits
one v1 JSON document. Default text includes source scope and section status,
groups CPUFreq policies only when their observed settings match, and reports
how many mount capacity queries ran or were skipped. `--full` adds per-item
detail without changing which observations were collected.
