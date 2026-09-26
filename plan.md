# Implement `system-report` as a native XSH systems-inspection program

## Mission

Implement a useful, production-quality XSH script called **`system-report`**, together with the reusable Linux standard-library extensions and differential-test harness needed to support it.

The command should answer: **What hardware does this Linux instance expose, how is it configured, what resources can this process actually use, and what is its current operating state?** Produce both an immediately readable overview and a detailed, versioned, typed snapshot suitable for programs and offline inspection.

This is a flagship XSH workload, not another Rust application with an XSH launcher. The application, collection orchestration, cross-subsystem joins, report model, selection, redaction, and rendering belong in XSH. Kernel interfaces and reusable typed host adapters belong in XSH's standard library. There must be **zero collector-initiated subprocesses**, including fallbacks and indirect library shellouts.

Implement the whole bounded scope below, not merely a prototype or a plan. Start with a vertical slice, then use measured coverage gaps to complete the required domains. Preserve unrelated working-tree changes. Do not push or publish anything unless separately instructed.

## 1. Establish the current repository contract

Read `AGENTS.md`, `docs/CHAPTER-01-why-xsh.md`, and the relevant parts of `docs/SPEC.md`, `docs/SPEC-TYPING.md`, `docs/SPEC-OS.md`, `docs/JSON.md`, `docs/ARCHITECTURE.md`, and `docs/TEST-MAP.md`. Use `xsht api` and the closest source/tests to establish actual signatures and syntax before implementing.

Relevant existing owners and behavior:

- `core/*.xsh` contains normal command scripts. `core/pstree.xsh` is a typed process-reporting example, not a template to copy blindly.
- `dev/release.xsh::core_sources`, `core_install_path`, and `package_core` already collect core scripts recursively, exclude `core/tests`, strip `.xsh` from executable commands, and retain `.xsh` under `core/lib/` for normal module loading.
- Public standard-module contracts belong to `crates/xsh-registry`. Inspect its signatures, records, runtime operation IDs, API documentation, and tests alongside the language-facing adapters.
- Existing Linux implementation owners include `src/modules/linux.rs`, `src/modules/linux/{api,block,kernel,process}.rs`, `src/modules/linux/real/*`, `src/modules/linux/unsupported.rs`, and evaluator dispatch/mode handling under `src/runtime/eval/`.
- Existing APIs include `system.uname`, `system.os_release`, `system.memory`, `process.list`, `linux.meminfo`, `linux.modules`, `linux.block_devices`, `linux.interfaces`, `linux.routes`, `linux.disk_usage`, `linux.sysctl_get`, and `linux.rfkill_list`. Inventory their real contracts, coverage, failure behavior, and call paths before extending them.
- Existing interface collection converts some errors into zero/empty values. Existing route collection reads procfs route files. These are not sufficient foundations for a loss-aware report or complete policy-routing inspection merely because they return records.
- The Linux API currently has `XSH_LINUX_REAL` and `XSH_LINUX_DRY_RUN` mode gates. Dry-run takes precedence and can emit canned values and write a diagnostic log. Preserve existing callers' documented behavior; never accidentally publish this data as a live system report.
- XSH already has named structural record schemas, optional types, tag unions, explicit effects, `Result`, typed paths, structured streams, and checked JSON boundaries. Use these. Do not invent unsupported user-defined generic syntax or add a new type-system project for this applet.
- The embedded standard library has an existing `Native`/`Script` binding architecture. Several parsers/helpers deliberately remained native after performance measurements. Respect that boundary; this task is not permission to restart a whole-stdlib migration.

Linux builds and tests must use the repository's pinned `Dockerfile.test`/`xsh-test` environment and existing development driver, with `aarch64-unknown-linux-musl` and `dev/targets.xsh::docker_test_env`. Do not substitute an arbitrary Ubuntu/glibc build and call it Linux verification. Preserve macOS builds and cross-platform fixture tests. Do not create a second CI system or unrelated workflows.

Follow the current prohibition on agent-run formatters/linters/autofixers and generated documentation churn. Use debug builds for ordinary tests; release builds only for actual measurement. The research-time `TEST-MAP` records a sibling `rustybench` compilation blocker: recheck it, but never count a command that fails before benchmarks execute as a performance pass.

## 2. Product and architecture boundaries

### The XSH application

Add `core/system-report.xsh`, installed as `system-report`, using the established `/bin/xsh` shebang and core packaging conventions. Put reusable application modules beneath `core/lib/`, for example a small `lib.system_report` facade with focused model/collection/rendering helpers. Adapt names to actual loader conventions. Keep imports static and source-visible.

Expose an importable typed collection interface and pure presentation functions. Another XSH script must be able to collect or accept the report, inspect CPU policies or device relationships, and make decisions without parsing text or JSON. Importing the library must not collect data, print, alter environment state, or run the CLI.

The XSH layer owns which collectors run, normalization into the report, joins, presentation, and privacy. Do not introduce a native `linux.system_report()` that does all the work. Conversely, do not force XSH to perform pointer arithmetic, own unsafe C layouts, or make thousands of scalar bridge calls when a reusable typed bulk API is appropriate.

### The standard library

Extend existing modules where their semantics fit. Add narrowly scoped, independently useful typed APIs for genuinely missing concepts: CPU topology/frequency policies, PCI, USB, mount information, sensors, power supplies, richer network dumps, and selected runtime state. Proposed names are not a mandate to create a new module hierarchy.

Implement ordinary composition and policy in XSH or embedded XSH when appropriate. Native Rust is permitted for reusable kernel ABI access, bounded binary decoding, descriptor ownership, and justified low-level parsers. Prefer existing `rustix`, filesystem, byte, collection, and serialization facilities. No Tokio, no daemon, no generic async runtime, no arbitrary `ioctl`/FFI escape hatch, and no wholesale dependency on another reporting application's model.

For every public addition, finish registry signatures/record shapes, effects, runtime dispatch, error mapping, platform stubs, native tests, and canonical API documentation. A function existing only in Rust or behind an untyped bridge is not complete.

Collection must be read-only in the operational sense: no system reconfiguration, module loading, device resets, interface changes, filesystem repair, privilege escalation, or external network requests. Local read-only kernel queries through netlink and narrowly defined ioctls are allowed; do not confuse socket use with Internet access or require every syscall to be literally `read`.

Use bounded synchronous collection initially. Respect cancellation checkpoints and resource cleanup. Do not add threads or an io_uring subsystem without measurements showing a specific need.

## 3. Small, useful command interface

Implement this behavior, fitting the existing CLI API:

```text
system-report
system-report --json
system-report --full
system-report --section cpu
system-report --from saved-report.json
system-report --sensitive --json
```

Also provide normal help and version/schema identification. Do not add an interactive dashboard, HTTP server, database, watch mode, multiple serialization formats, plugin registry, tuning commands, or package-management integration.

Default text is a compact overview with a clear completeness/scope summary. Group identical CPU policies rather than printing the same row hundreds of times. Summarize device families and show the important driver, storage, network, memory, and power configuration. Never imply identical policies when they differ.

`--json` emits the complete collected v1 report as one JSON document and a newline, with no status chatter on stdout. `--full` expands the text presentation; it does not silently enable invasive probes. `--section` restricts collection as well as rendering, apart from explicitly required dependencies such as device identity. Mark excluded sections as not requested.

`--from` validates and renders an already collected report using the same model and renderer. It performs no live inspection, enrichment, fallback lookups, or network queries. Support this offline path on macOS even though live collection is Linux-only. Reject unsupported schema versions and malformed input clearly.

Root must not be required. Expected permission/hardware gaps produce a useful partial report and structured issues, not a traceback or fake values. Use existing applet exit conventions: successful valid partial collection is distinguishable from invalid arguments, invalid replay input, unsupported live platform, and fatal collection failure. Document the exact contract. The acceptance harness, not a forest of production CLI flags, enforces required coverage.

Do not depend on `XSH_LINUX_REAL=1` being set in the user's shell just to make the applet work. A small lexical XSH environment scope enabling existing read-only calls is acceptable after rejecting active dry-run mode; it must not spawn `env` or a shell or change global gates. Preserve the low-level APIs' existing gate behavior. Live `system-report` must reject active synthetic/dry-run mode rather than silently mixing real and canned observations. Exercise dry-run API contracts separately from report correctness.

## 4. Typed report contract

Define a named `SystemReport` schema with a schema version, producer information, source mode, collection start/end, elapsed time, observation scope, section results, and structured issues. Keep known fields statically typed throughout application code. `Any` is allowed only at genuinely dynamic serialization/test boundaries, immediately narrowed with the normal schema mechanism.

Model the data according to its meaning:

- Named records for CPUs, CPUFreq policies, caches, NUMA nodes, PCI functions, USB devices/interfaces, block devices, mounts, network objects, sensors, and processes.
- Typed numeric IDs, quantities with explicit units, booleans, lists, optional values, and tag unions where appropriate. Kernel names such as driver names are legitimate text, not an excuse for a universal `Map[Any]`.
- Preserve unknown kernel enum values/attributes without treating every new value as corruption. Do not assert that a fixed list of governors or drivers is exhaustive.
- Keep raw hardware identifiers distinct from human database labels. Names are enrichment, never identity.
- CPU, interface, PID, mount, and bus addresses are scoped identities. Define their stability limits. Do not promise that USB device numbers or interface indices survive reboot or replugging.

Represent section enumeration success separately from its items. Use a compact consistent combination of typed optional fields, section state, and field-addressed issues, or an equivalent idiomatic typed approach. Distinguish absent, unsupported, permission denied, not requested, redacted, malformed, disappeared/raced, and truncated/limited. Preserve errno or an equivalent stable error classification; do not classify failures by matching English error messages.

An empty successful enumeration is not a failed enumeration. Zero is a real observation, not a missing-value placeholder. A single malformed device must not discard every valid device in the same section. Stream failures encountered during iteration must be handled as deliberately as failures opening the stream.

Keep amounts exact. Do not convert byte counts or counters to floats for convenience, assume all JSON consumers preserve large integers, saturate overflow silently, or reinterpret unsigned values as signed. Respect XSH's actual integer limits: use checked conversions and a documented lossless representation for opaque oversized identifiers, or report an explicit range failure. This is not a mandate to add arbitrary-precision arithmetic to XSH.

Use actual page size and clock-tick metadata where conversions require them. Test 64 KiB pages and non-default tick rates. Keep gauges and cumulative counters separate; do not invent instantaneous CPU utilization, I/O rates, or power consumption from one sample.

Preserve relationships, not just unrelated arrays: CPU-to-policy/cache/NUMA, USB-interface-to-device-to-hub/controller, netdev/block/GPU-to-parent-device, PCI-to-driver/IOMMU/NUMA, mount-to-block-device, and process-to-visible-cgroup. Use compact indexes and stable ordering. Missing or unresolved relationships must be representable without fabrication.

## 5. Required coverage

Deliver all domains below to the specified depth. This is the bounded v1 definition of comprehensive coverage, not a claim to dump every kernel variable or every register of every device.

### A. System identity and observation scope

Kernel release/build, architecture, OS release, uptime, boot context where available, process-visible CPU and memory scope, and firmware/platform identity. Support both DMI-oriented PCs and ARM systems whose identity comes from device tree. Do not source shell-formatted files as code.

Record the collector's namespace/cgroup context and readable source roots. Do not call a container-visible mixture of procfs/sysfs and current-netns data the complete physical host. A single set of filesystem reads is not an atomic snapshot: report its interval and any detected races.

### B. CPUs, topology, and operating policies

Enumerate actual online/offline/present/possible CPUs without assuming dense numbering, CPU 0 availability, or a 64-bit CPU mask. Preserve per-CPU model/features on heterogeneous machines. Report package/core/thread relationships, shared caches without double counting, NUMA membership, relevant affinity/cpuset constraints, and kernel-exposed vulnerability/mitigation descriptions without inventing a security verdict.

Enumerate **every** `/sys/devices/system/cpu/cpufreq/policy*`: related and affected CPUs, driver, governor/scaling algorithm, available governors, hardware bounds, configured bounds, available current/requested frequency observations, EPP where exposed, and boost controls with their scope. No `policy0` shortcut. Distinguish permitted boost from currently boosting. Distinguish absent CPUFreq in a VM from a broken collector.

Include CPUIdle state names, disable settings, latency/residency metadata and available counters, plus global idle-driver/governor metadata. Preserve kernel units and semantics. Driver-specific configuration is optional per host, not an excuse to omit generic policy collection.

### C. Memory and resource constraints

Typed meminfo fields, swap devices, huge-page sizes/counts, transparent-huge-page policy, NUMA memory where available, and pressure-stall observations. Expose relevant cgroup-v2 memory/CPU/pids/io constraints and available counters for the current visible group and visible ancestors. Detect v1/hybrid arrangements and identify unimplemented details explicitly.

Keep host-visible physical memory, cgroup limits/current use, CPU affinity, effective cpusets, and quota/period distinct. Do not infer unlimited host resources when ancestors are hidden, or reduce quota semantics to an unexplained integer CPU count.

### D. PCI

Enumerate PCI functions from sysfs with full domain:bus:device.function identity, vendor/device/subsystem IDs, class/subclass/programming-interface/revision, bound driver, parent bridge/controller path, NUMA node, IOMMU group, and exported current/maximum PCIe link speed and width. Preserve multifunction devices, nonzero domains, unbound devices, and unknown IDs.

Read exported metadata first. Do not read BAR memory, enable ROM access, rescan/remove/rebind devices, or blindly dump configuration space. Exhaustive PCI capability/register decoding is not required for v1. Keep that outside the scored baseline rather than counting it as implemented with empty fields.

### E. USB

Enumerate root hubs, hubs, devices, and active interfaces from sysfs, preserving topology, bus/device numbers, port paths, VID/PID, versions/classes, manufacturer/product when exposed, interface driver bindings, negotiated speed, and available power/runtime-PM configuration.

Parse exported descriptor blobs safely where they supply required configuration/interface/endpoint facts. Handle composite devices, repeated VID/PID pairs, alternate settings, interface-versus-device class, partial descriptors, and hot-unplug. Distinguish available descriptors from active configuration/alternate-setting observations. Validate all lengths, bounds, and version-dependent units; unknown descriptors must not break the whole inventory.

No USB resets, configuration changes, driver detachment, arbitrary control transfers, or external `lsusb` calls. A root hub may belong to a platform controller rather than a PCI function.

### F. Storage and mounts

Reuse and improve typed block enumeration: major/minor identity, physical/logical/virtual devices and partitions, capacity, logical/physical sector sizes, removable/rotational/read-only state, model/firmware when exposed, controller relationships, holders/slaves, active/available I/O schedulers, read-ahead and queue configuration, discard capability, and available cumulative I/O counters.

Parse mountinfo faithfully, including escaped path bytes, mount and parent IDs, filesystem/source, mount/superblock options, and propagation metadata. Preserve one-to-many relationships: multiple mounts are not a single mountpoint string, and stacked storage is not necessarily a tree.

Capacity/usage collection must avoid automatically traversing network/automount filesystems or waking every raw disk. Query bounded eligible local mounts and represent skipped usage separately from mount inventory. Do not use raw-device filesystem probing or SMART/NVMe admin commands by default. Exhaustive storage-health protocols are a later extension, not a prerequisite for this report.

### G. Network

Collect links, addresses, IPv4/IPv6 routes, and policy rules with typed native kernel interfaces. Include interface index/name/type, MTU, administrative/operational state, flags, master/lower-link relationships, addresses/prefixes, route table/metric/type/scope/protocol, nexthops where exposed, and available link counters. Add native read-only driver/link information where it can be implemented cleanly.

Use route netlink rather than pretending procfs route files expose the entire routing model. Validate multipart framing, attribute lengths/alignment, sequence/sender, acknowledgments/errors, interrupted dumps and truncated messages; use bounded retries and cancellation. No DNS resolution, address probing, device configuration, or `ip`/`ethtool` shellouts. Do not require NetworkManager or systemd.

### H. Sensors and power

Enumerate hwmon chips/channels, thermal zones/trips, power supplies and batteries, and kernel-exported powercap limits/counters. Preserve chip/channel identifiers, parent relationships, source units, available thresholds/alarms, charging status, capacities, cycle count and health-related observations where exposed.

Read only known informational attributes. Do not run `sensors-detect` or load modules. Default raw kernel sensor labels/scaling need not equal board-specific `libsensors` configuration. Identify that distinction. Missing thermometers are not zero-degree readings; an energy counter is not an instantaneous watt reading.

### I. Firmware, graphics, audio, and input inventory

Report available DMI identity and read-only SMBIOS system/board/processor/memory-array/DIMM records from kernel-exported tables, with optional per-field detail. Validate SMBIOS lengths, versions, string-table indices, end markers, and unknown/sentinel sizes. No `/dev/mem` fallback or `dmidecode` invocation. Where SMBIOS is not exposed, retain valid device-tree/platform identity and an explicit limitation.

Add lightweight kernel-exposed DRM/display, sound-card, and input-device inventory and parent links. Do not read input events or require GUI services, vendor GPU libraries, audio-server APIs, or a display session. Exhaustive EDID, codec, proprietary GPU and firmware-table decoding is not required.

### J. Kernel and processes

Kernel command-line policy under the privacy rules, loaded modules, selected relevant module parameters, typed curated sysctls, and basic visible-process identity/state/resource counters. Include PID/PPID/UID, command name, start identity, thread count, RSS/virtual memory and available accounting; handle exit/PID-reuse races.

Do not collect environments, credentials, arbitrary command lines, memory contents, logs, or open-file paths by default. Avoid NSS/name-service lookups; numeric identities are sufficient. Do not recursively dump all `/proc/sys`, debugfs, tracefs, efivars, or every process detail. Default text can summarize processes while JSON retains the bounded collected records and limitation metadata.

## 6. Safety, privacy, and predictable collection

Treat procfs/sysfs/device strings as untrusted input. Escape terminal controls, embedded newlines and bidi/control sequences in text output. Preserve legitimate raw data in the structured model using a documented lossless byte representation where UTF-8 is unavailable. Never build JSON with string concatenation or use display-form paths as a supposedly reversible identity.

Default to a share-safer report: redact unique machine/device identifiers and sensitive hostname/address/serial information consistently in text and JSON. `--sensitive` explicitly includes supported identifying fields. Never collect process environments or credentials even in that mode. State that redaction reduces exposure, not that an inventory is guaranteed anonymous. Redaction must include issue messages, source metadata, and replay output; it must not destroy structural device relationships or numeric hardware class/vendor/product IDs.

Use explicit read allowlists, length/count/depth bounds, and bounded descriptor lifetimes. Do not blindly walk/read all of `/sys` or `/proc`: informational files can invoke driver callbacks, disappear, or behave unlike ordinary disk files. Do not rely on file size to decide a procfs/sysfs file is empty.

Use the existing rooted-filesystem authority model for fixture roots and any new source context. No mutable process-global root variables. Correctly handle legitimate sysfs relative symlinks while preventing fixture symlink escapes, loops, and accidental fallback to the running host. Avoid path-check-then-open races where authority matters.

Document soft collection budgets honestly. A timer around a blocking driver or filesystem call is not a guarantee that the call can be cancelled. Avoid risky calls in the default path and use a test-supervisor timeout for hangs; do not invent subprocess workers to dodge the contract.

Local PCI/USB ID files are optional data, not executables. Load only relevant databases, at most once per collection, and tolerate missing/corrupt/outdated data. No network updates or runtime cache writes. Preserve provenance and licenses for any bundled data; keep large optional databases out of the XSH binary unless a measured justified design requires otherwise.

## 7. Build the coverage harness early

Implement a local XSH-driven harness integrated with existing `dev/` entrypoints. Prefer a focused command such as `xsh dev system-report-check`; choose the actual dispatch spelling idiomatically. External utilities are permitted **only in this harness**, using explicit argv execution. Rust test helpers may own privileges, syscall observation and other host boundaries. Do not introduce a second production collector in Python or Rust.

Create one machine-readable coverage manifest and generate human summaries from it. Each assertion should record a stable ID, domain/field/relation, requirement tier, source ABI, reference command/adapter, eligibility condition, equality/tolerance rule, and required fixture scenarios. Keep expectations reviewable; do not dynamically shrink the denominator to whatever the candidate outputs.

### Reference adapters

Use installed versions in the pinned test image. Record version, exact argv, locale, privileges, namespace/source scope, start/end time, output, exit status and ID-database identity where relevant. Pin/probe capabilities instead of assuming every version supports identical flags. Never download or install tools during a test run.

Build adapters for these comparisons:

| Domain | Reference | Comparison boundary |
| --- | --- | --- |
| PCI | `lspci -D -vmm -n -k`, supplemented by safe selected detail output | Numeric IDs, identities, classes, binding, NUMA/IOMMU and exposed links; not pretty strings |
| USB | `lsusb`, `lsusb -t`, selected `lsusb -v` captures | Device/interface topology, IDs, drivers and exported descriptors; bounded opt-in verbose live capture |
| CPU | `lscpu` JSON/explicit-column output; `cpupower frequency-info` and `idle-info` when available | Kernel CPU identities, topology, policies and idle metadata; account for logical versus physical ID presentation |
| Storage | `lsblk --json --bytes` with explicit columns; `findmnt --json` with explicit columns | Devices, sizes, layering and mounts; do not rely on defaults or truncate multi-mount relationships |
| Network | `ip -json -details link`, `ip -json address`, route dumps for both families/all tables, and policy-rule dumps | Typed kernel objects and relationships, not ordering or derived display text |
| Sensors/power | `sensors -j`, plus documented kernel ABI fixtures | Raw channel/unit comparisons separate from configuration-derived labels/scaling |
| Memory/process/kernel | selected `free`, `swapon`, `ps`, `lsmod`, `sysctl` output with explicit formats | Equivalent definitions, stable identities and appropriately bracketed observations |
| Firmware | `dmidecode` on captured tables or explicitly enabled live reads | The SMBIOS record types/fields actually in scope |
| Broad inventory | optionally `lshw -json` and selected HWall/siomon output | Gap discovery and corroboration, not sole ground truth |

Where a utility lacks a stable machine format, isolate a version-aware parser with its own saved-output tests. Never parse human table spacing in the production applet. Never use `lspci -xxx/-xxxx`, bus scans, or other aggressive reference modes in the default harness.

References are independent evidence, not infallible truth. Check mismatches against the kernel ABI and raw source. CPU cache aggregation, logical topology IDs, sensor configuration, udev enrichment, and privilege differences can legitimately disagree. A candidate/reference disagreement must get an explicit diagnosis, not a blanket allowlist.

### Scoring that cannot hide missing coverage

Report separately: required cases, eligible cases, exact matches, mismatches, candidate-missing fields/entities, not exercised, reference unavailable, permission-limited, unstable, and intentionally out of scope. Include per-domain entity precision/recall, field coverage, semantic agreement, and relationship correctness. A single global percentage is insufficient.

Eligibility must come from the manifest, independently captured sources, reference capability and environment—not from the candidate saying a field is unsupported. A null emitted for every hard field is failure, not graceful completeness.

Deterministic mandatory fixtures require 100% assertions passing. Required entity enumeration and identity/relationship assertions require exact agreement where observations are comparable. Missing/incorrect mandatory fields are release blockers regardless of an aggregate score. A supplementary >=95% field-agreement target can guide richer optional corroboration, but must not excuse mandatory failures or unexercised domains.

Changing a denominator, dropping a case, broadening a tolerance or classifying a failure as excluded requires an explicit reviewed rationale in the manifest. Record before/after counts for each improvement. Preserve adversarial and held-out cases rather than fitting to one workstation.

## 8. Three kinds of verification

### A. Deterministic parser and XSH integration fixtures

Separate byte/text decoding from acquisition. Feed live and fixture data through the same production parsers and typed collector assembly. Add the smallest reusable scoped source abstraction required; do not create a generic filesystem emulator or a public global `--root` shortcut. For netlink/ioctl data, capture or synthesize validated raw replies behind the relevant test boundary.

Use disk-backed native XSH tests by default for schemas, collection composition, failure handling, joins, redaction, rendering, and API behavior. Rust tests own unsafe/ABI decoding, actual errno/permission races, low-level source injection and syscall/process boundaries. A Rust-owned harness should execute native XSH tests when assertions concern XSH behavior.

Cover at least these scenarios across a compact corpus: x86 multi-policy Intel-like and AMD-like configurations; heterogeneous ARM without DMI; a VM without CPUFreq/sensors; nonzero PCI domains and repeated device IDs; USB hubs/composite devices/alternate settings; multi-NUMA/shared caches; stacked block devices and multiple mounts; IPv6 and policy routing; cgroup limits; permission-denied and hot-unplug cases.

Include malformed/truncated/unknown fields, empty files, oversized values, invalid numeric units, sparse CPU IDs, cache IDs not starting at zero, >64 CPUs, 64 KiB pages, unknown drivers/governors, unreadable optional attributes, symlink cycles/escapes, non-UTF-8 names, and terminal-control strings. Native permission tests must actually run as an identity denied access; chmod under root alone is not sufficient.

Keep synthetic fixtures clearly labeled. Never fabricate a real-machine capture or claim a fixture tested a physical hardware path.

### B. Reproducible paired capture/replay

The harness should capture allowlisted raw procfs/sysfs content, relevant reply bytes, source absence/error metadata, optional ID data, and reference outputs with provenance. Replay must run the **candidate collection logic from raw inputs**, not simply compare a saved candidate report to itself. Normal `--from` report rendering is a separate test.

Keep fixture expectation generation independent of the candidate's implementation. Inspect licenses and permissions before retaining third-party captures. Redact captured identifiers consistently across raw sources and oracle outputs. Raw local diagnostic bundles containing sensitive data must remain untracked and be created with restrictive permissions.

Synthetic, captured/replayed, container-live and physical-live results must remain distinguishable. Add historical bug captures as regression cases without turning the repository into a large archive of generated reports.

### C. Live differential and no-subprocess tests

Run available comparisons inside the pinned Linux environment with the same UID, privileges and namespace view. A Docker VM without PCI/USB passthrough is valid live evidence for the interfaces it exposes, not evidence of full physical-device coverage. Report unexercised capabilities explicitly and provide one actionable command for running the built report/harness on a suitable Linux machine.

For static facts, bracket candidate collection with reference observations when practical and require agreement on objects that remained stable. For counters/gauges, use timestamps and field-specific temporal semantics; do not blanket-ignore mismatches as races or use an arbitrary percentage tolerance. Controlled fixtures cover cases where live comparison is inherently unreliable. Check reference-parser failures separately from candidate failures.

Prove no subprocesses independently of XSH effects or a grep for `run`:

1. Audit the reachable application/stdlib/dependency paths for exec/spawn/fork helpers, command substitution, implicit DNS/NSS helpers and fallback utilities.
2. Run the real applet using an absolute XSH binary and script path, with an empty/nonfunctional `PATH`, minimal environment, and no installed helper commands in a minimal fixture image/root. Keep only the interpreter, source modules, and required passive data. Initial interpreter launch is outside the contract; everything it initiates is inside it.
3. Use the existing process/syscall tracing machinery or a focused `strace`/ptrace integration harness to assert no child processes, secondary execs, or attempted helper execution after the initial XSH image starts. Inspect `clone`/`clone3` flags rather than confusing threads with subprocesses. Exercise normal, permission-denied, missing-database, unsupported-device and malformed-input paths.
4. Add a deny-exec/process guard at an appropriate existing test boundary where feasible. Do not add an application-specific security framework. Absence from `PATH` alone is not proof: an absolute-path shellout would bypass it.

Trace and assert the no-system-mutation/no-external-network contract independently where practicable. Allow stdout/stderr writes and legitimate read-only kernel request traffic. Remember that dry-run logging would violate the live no-file-write contract. Test offline rendering for zero host collection too.

## 9. Performance and memory discipline

Use one collection pass shared by text/JSON renderers. Do not rescan all devices for every label or relationship, reopen and parse the same ID database per device, launch one collector per CPU when the kernel exports policies, build huge intermediate strings, or clone the whole report repeatedly. Index joins instead of quadratic nested scans.

Measure collection, joins/projection and rendering separately where instrumentation permits. Measure end-to-end startup too. Record wall time, CPU time, peak RSS, allocation count/bytes where available, file/syscall counts, descriptor peak and output size. Reuse `xsh-runtime-stats`, paired host RSS measurements and the existing performance tooling instead of inventing a metrics framework.

Benchmark ordinary fixtures and scaled corpora with thousands of devices/processes. Keep collected coverage and output fingerprints fixed between comparisons. Distinguish cold-start preparation from warm filesystem-cache runs. Do not compare a richer report with one narrow utility and claim a speedup or regression without accounting for work performed.

Protect unrelated XSH startup, minimal/default-feature binary size, and existing stdlib performance gates. Keep large data and application schemas out of preparation for unrelated scripts where possible. If XSH collection/model manipulation exposes a real runtime bug or pathological allocation behavior, fix the narrow underlying issue with regression tests. Do not hide the entire application in Rust or introduce AOT/JIT/frozen-image work to win the benchmark.

Report measured baseline and final numbers with environment/methodology. No invented performance targets, results, or guarantees. Establish any new regression budget from an initial reproducible baseline, retain its coverage, and explain the tradeoff.

## 10. Implementation sequence and definition of done

First inventory reusable APIs and required gaps. Add the minimal coverage manifest and no-subprocess harness before broad implementation. Build a vertical slice covering typed CPUFreq, PCI and USB collection plus JSON/text output and fixtures; these directly test the project's premise.

Then complete storage/network/memory/sensors/power/kernel/process/firmware/device-class coverage, strengthen relationship joins and partial-result semantics, and run the differential loop: classify gaps, implement, add regression cases, rerun, and retain measured evidence. Finish replay/redaction/packaging and scaled measurements. Do not stop after the vertical slice merely because it looks useful.

Completion requires:

- A real `core/system-report.xsh`, installed without the suffix, working from the packaged core tree outside the repository and from an unrelated working directory.
- An importable typed XSH report interface; native XSH tests demonstrate filtering policies and joining a device to its parent without JSON parsing or shellouts.
- Complete stdlib integration for every added API, with platform/error/resource-lifetime behavior tested. No fake successful stubs or placeholder zero/empty results for unimplemented required collectors.
- Every required deterministic fixture and mandatory coverage assertion passes. Live results identify what actually ran and what needs different hardware/privileges. Missing hardware is not a pass and not an excuse to omit the collector or its fixtures.
- Actual no-subprocess evidence for the production path and tested failure paths, and no accidental synthetic-mode report.
- Verified JSON schema/round-trip/replay behavior, deterministic ordering, privacy and terminal-safe text rendering, and complete partial/error metadata.
- Relevant native XSH gates, registry/API gates, filtered runtime gates, core packaging tests, pinned Linux verification and macOS non-regression tests run according to `TEST-MAP`. Run checker/compile-fail tests for incorrect field types and effect misuse. Identify owner-only formatter/linter gates without running them.
- Measured performance/memory evidence and no unexplained regressions. Report blocked gates honestly; do not repair unrelated sibling projects or claim they passed.

Update canonical docs and closest tests only. Keep a single concise implementation checklist where repository practice calls for one and one machine-readable coverage manifest; no parallel STATUS documents, progress diaries, proof receipts, or duplicate schemas.

The final implementation report should give exact run/test commands, added or improved public APIs, coverage counts by domain and evidence type, remaining environmental verification gaps, benchmark results, and any narrowly scoped follow-on coverage outside the stated v1. Do not claim literally complete Linux state or perfect physical hardware coverage from a container run.

## Reference material

Use primary source and ABI documentation. References provide semantics and test ideas, not permission to copy incompatible code. Reinspect current code before depending on behavior. Clone selected repositories only as implementation references outside the tracked tree; record revisions for any adapted code/data and honor licensing. In particular, treat `lemonrock/linux-support` as design reference unless its AGPL licensing is compatible with the intended reuse.

Repository and language baseline:

- https://github.com/laputa-systems/xsh
- https://github.com/laputa-systems/xsh/blob/master/AGENTS.md
- https://github.com/laputa-systems/xsh/blob/master/docs/ARCHITECTURE.md
- https://github.com/laputa-systems/xsh/blob/master/docs/TEST-MAP.md
- https://github.com/laputa-systems/xsh/blob/master/docs/JSON.md

Linux ABI:

- https://docs.kernel.org/admin-guide/pm/cpufreq.html
- https://docs.kernel.org/admin-guide/pm/cpuidle.html
- https://docs.kernel.org/PCI/sysfs-pci.html
- https://docs.kernel.org/filesystems/sysfs.html
- https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-bus-usb
- https://www.kernel.org/doc/Documentation/ABI/stable/sysfs-block
- https://docs.kernel.org/admin-guide/iostats.html
- https://docs.kernel.org/hwmon/sysfs-interface.html
- https://docs.kernel.org/admin-guide/cgroup-v2.html
- https://www.kernel.org/doc/html/latest/userspace-api/netlink/intro.html

Oracle implementations and useful prior art:

- https://github.com/pciutils/pciutils — machine-readable lspci contracts and PCI interpretation.
- https://github.com/gregkh/usbutils — lsusb enumeration, topology and descriptor interpretation.
- https://github.com/util-linux/util-linux — lscpu/lsblk/findmnt semantics, explicit columns and test fixtures.
- https://github.com/iproute2/iproute2 — route/link/rule object semantics and JSON presentation.
- https://github.com/lm-sensors/lm-sensors — raw versus configured sensor semantics.
- https://github.com/lyonel/lshw — broad hardware discovery and cross-source relationships.
- https://github.com/tuna-f1sh/cyme — USB structure and snapshot/presentation separation.
- https://github.com/pulpul-s/HWall — broad collection and explicit helper-disabled mode; not an authority for complete typing or zero helper use in every mode.
- https://github.com/level1techs/siomon — hardware-domain models and native probes; audit fallbacks instead of treating a no-runtime-dependencies claim as a no-exec guarantee.
- https://github.com/lemonrock/linux-support — aggregate typed diagnostics, scoped identities and unavailable observations; old code, not a dependency recommendation.

The intended result is one ordinary XSH command demonstrating that the language can compose a rich typed model of Linux directly, with reusable stdlib capabilities and independently measured coverage—not that XSH can launch all the old utilities and package their text as JSON.

## Handoff (2026-09-26)

`master` was already aligned with `origin/master` at `c038f4d`, so there were no upstream commits to reconcile. The current implementation is unfinished. The Rust library cross-check for `aarch64-unknown-linux-musl` passed inside the amd64 build of `Dockerfile.test` with the target Rust flags, but the required ARM64 `xsh-test` driver could not build or run: Docker BuildKit exposes only amd64 and the ARM64 image fails with `exec format error`. The focused `xsht test system-report` run failed on XSH parse/type diagnostics; the affected model, collector, live collector, CLI, and tests remain unverified after the partial fixes. Resume by clearing those diagnostics and rerunning the focused XSH tests, then the pinned ARM64 Linux driver when an ARM64-capable Docker worker is available. The coverage command still validates its manifest and process traces only; reference adapters, paired raw capture/replay, scoring, mutation/network audits, and measured performance evidence remain incomplete.
