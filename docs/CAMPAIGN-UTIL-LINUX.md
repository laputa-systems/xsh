# Extend core compatibility to util-linux

Status: active, 2026-10-17. The owner authorized execution alongside consolidation.

Deliver the first complete Linux util-linux cohort: `lslocks`, `lsipc`, `lsmem`, `mountpoint`, and `namei`.
Implement commands in native XSH. Reuse the existing collectors and host APIs.
Calling the installed utility is permitted in reference tests, never as the
command implementation. This campaign does not claim complete util-linux parity.

The deliverables are:

1. A runnable, qualified util-linux reference and behavior corpus for all five commands.
2. A complete `lslocks` capability.
3. A complete `lsipc` capability.
4. Complete `lsmem`, `mountpoint`, and `namei` capabilities.
5. Regression coverage for the existing lock, storage, CPU, namespace, and process commands.
6. Integrated native tests, host-boundary tests, command documentation, and measured resource acceptance.

## Charter

Run the coordinator and four implementation lanes in isolated worktrees.
The coordinator owns shared host APIs, registries, gate selectors, integration,
and final verification. It writes no feature code while implementation lanes run.
Apply the local `orchestrate` skill and its campaign lane contract.

Decide technical details inside the bounds below, record the reason, and continue.
Park only an item that requires forbidden authority, an unavailable execution
environment, or a verification loop that cannot establish any result.
Resolve shared-code blockers in the coordinator. Never invent a silent fallback.
After three distinct failed fixes, preserve the reproducer and assign a fresh
bounded root-cause investigation. Continue independent work. A parked mandatory
item prevents campaign completion.

Parity and complete tests precede optimization. Preserve the existing native
origin accounting. Never turn a skip, unexecuted case, or missing feature into a pass.
Do not run formatters, change CI, change the Rust nightly, or push.
Local commits are permitted only when the owner's execution grant includes them.
Use Conventional Commits when permitted. Put item IDs in commit bodies.

Rooted `lsmem` fixtures use `--sysroot` and include missing and malformed
attributes, sparse blocks, and symlinks. A missing fixture attribute must never
fall back to live host sysfs. Exercise that invariant with a conflicting host
value or a fixture-only root.

All worktrees and scratch files live under `.work/`. Before any write, ensure
`.work/` is ignored through `.git/info/exclude`. Set `TMPDIR` to an existing
`.work/tmp` directory. Do not add durable evidence unrelated to acceptance.
Follow orchestrate's worktree lifecycle. Without commit authority, retain
uncommitted worktrees and report them. Never force-remove a worktree or branch.

Fix a broken loop before repeating it. A single behavior selector must finish
within 30 seconds, excluding an explicitly timed lock-wait case capped at 60 seconds.
A focused command group must finish within 120 seconds. If it exceeds that
budget twice, the coordinator narrows the selector or fixes the harness before
resuming that group. Do not remove behavior to meet the budget.

## Decisions

### Owner direction

The owner requested a util-linux campaign extending coreutils compatibility,
named `lslocks` as an example, and requested about four implementation lanes
alongside consolidation. On 2026-10-17 the owner authorized launching this
campaign after receiving the launch prompt with local commits and no push.

### Prescribed decisions

| Decision | Reason and reversal condition |
|---|---|
| Use x86-64 Linux musl in the pinned test image. | Match the active campaign's qualified platform. Change only on owner direction. |
| Complete five new commands, beginning with `lslocks`. | These commands form four disjoint workstreams and reuse existing lock, process, memory, and mount foundations. Add another cohort only after this cohort closes. |
| Use util-linux 2.42.4 from the pinned image as the executable reference. | Planning verified that version. P0 fails a version mismatch and records enabled features. Never move the denominator to a different installed release. |
| Complete every behavior available in that reference for each selected command. | Existing partial option support is the starting point, not the target. Do not redefine the target as today's XSH flags. |
| Preserve XSH identity in `--version`. | Existing `gnu.version` prints `NAME (XSH core) VERSION`. Record this narrow provenance exception. The util-linux 2.42.4 string pins the oracle, never the XSH banner. |
| Implement policy, parsing, selection, and rendering in XSH. | Preserve the project's architecture. Add a host primitive only for an actual missing OS operation. |
| Keep shared source changes with the coordinator. | Registry, runtime, and gate ownership overlap consolidation. Feature lanes consume a reviewed seam. |
| Use private fixtures for process, lock, namespace, and device cases. | Observe the same objects in reference and XSH without changing host state. Privilege cases use the existing pinned container isolation. |
| Add no dependency by default. | Existing Linux and descriptor APIs are the starting point. A dependency proposal requires coordinator review, maintenance and license evidence, and measured build and binary-size impact before adoption. |

### Delegated decisions

The coordinator chooses internal record and error types from observed kernel
contracts. Keep units explicit and optional values distinct from empty text.
Keep pure parsing and rendering separate from effects. Expose only the narrow
host primitive the command needs. Update the registry, API tests, and API docs
together when adding an exported API.

Lanes choose the smallest reversible internal design inside their owned files.
When upstream behavior depends on an unavailable kernel or build feature,
record the exact condition and a test for the supported failure behavior.
A conditional case remains a skip unless its positive path actually runs.
The campaign covers Linux behavior of the qualified reference build, including
its documented unavailable-feature failures. It does not claim other build configurations.
The coordinator must qualify mandatory positive paths before closing the command.
Do not label an available upstream behavior unsupported merely to avoid implementing it.

## Baseline

Planning source: `0097a8b9ac0a98e46dc0a66a10cdeb61af929043`.
The planning worktree does not mutate the running consolidation source.
The Linux image is
`xsh-test@sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.

The five cohort commands have no applets or dedicated native tests.
Existing neighboring commands include `flock`, `lsblk`, `findmnt`, `lscpu`,
`lsns`, `chrt`, `setsid`, and `rev`. Six dedicated files for those commands
contain 35 native cases. Storage has 44 shared cases, including rooted
`lsblk` and `findmnt` fixtures. These counts establish reusable coverage, not parity.

Known limitations observed directly in source include inherited-descriptor,
timeout, no-fork, and OFD locking in `flock`; cache-table and hexadecimal-mask
output in `lscpu`; namespace-ID operands in `lsns`; and a small option and
column set in `lsblk` and `findmnt`.
The baseline has no qualified five-command differential suite.
The consolidation campaign retains its own known-red results. Its aggregate
pass count supplies no util-linux compatibility credit.

Planning probes ran in the pinned image with util-linux 2.42.4 and kernel
`7.2.9-2-stable`. Checking the missing `lslocks` applet exited 2 with ENOENT.
Ordinary source checks for `flock`, `findmnt`, `lsblk`, and `lsns` passed.
One readiness-qualified loop sample recorded 0.20 seconds wall time and
0.25 seconds CPU time. This single sample is loop guidance, not a benchmark gate.
P0 repeats the selectors against its actual implementation base.

A live owned BSD flock proved the key `lslocks` foundation. Reference
`lslocks` observed PID 21, device `103:03`, inode 62138326, and range `0 EOF`.
XSH `fs.stat` observed device 66307 and the same inode. Existing
`linux.open_files()` and `lsof` matched the holder and fixture path.
The hexadecimal device pair and encoded `st_dev` describe the same device.
The probe cleaned up its holder and containers. This proves identity joining
is possible. It does not prove POSIX, OFD, lease, blocker, or race parity.
Those remain P0 fixture and P1 behavior work.

The probes reused release products from
`.work/consolidation/final-performance-gates/profile-cost-57a0f63145a4ea3b1db6cfc38b4744a5c7ab02c1/opt3/target/x86_64-unknown-linux-musl/release/`.
They did not build the planning source or qualify an exact-source campaign gate.
The probe scripts were removed after cleanup. Reproduce the source-check loop
inside the pinned image with that directory as `PRODUCTS`:

```sh
/usr/bin/time -f 'wall_seconds=%e user_seconds=%U sys_seconds=%S' "$PRODUCTS/xsht" check core/flock.xsh core/findmnt.xsh core/lsblk.xsh core/lsns.xsh
```

For the identity proof, create an owned scratch file and hold it with
`flock -F FILE /bin/sleep 45`. Wait until the saved holder PID appears in
`/proc/locks`. In XSH, call `fs.stat(Path(args[0]), follow_symlinks: true)`
and `linux.open_files(holder_pid)`. Require an entry with the same `dev` and
`inode`. Compare `lslocks --pid HOLDER` and `xsh core/lsof.xsh -- FILE`
against that identity and path. Bound the container to 120 seconds and kill,
wait, and remove owned resources in an exit trap. P0 turns this recipe into
an assertion-based native fixture covering all required lock classes.

## Sequence

### Workstream ownership

The owner authorized sixteen concurrent source workstreams. Twelve util-linux
workers share the coordinator, independent reviewer, consolidation gate owner
and disk owner. Source width does not increase execution width.

The four original workers own command implementations and command documentation.
Four parity workers own their dedicated command tests, fixtures and coverage maps:
`lslocks`, `lsipc`, `lsmem`, and the paired `mountpoint` and `namei` corpus.
The path corpus uses `dev/util-linux/coverage/paths.md`.

Two regression workers own new storage and process regression files and their
own fixtures. The storage worker covers `lsblk`, `findmnt`, `lscpu` and `lsns`.
The process worker covers `flock`, `chrt`, `setsid` and `rev`.
The private-context worker owns `support/util_linux_contexts.xsh`, its dedicated
test file and context fixtures. The acceptance worker owns `acceptance.py` and
its unit tests. The coordinator retains the shared native helper, thin CLI,
host primitives, integration and final verification.

All execution still uses the existing global guards and resource caps.
The coordinator serializes native checks. Source workers do not launch competing
containers, builds or native suites. Consolidation timing retains priority.

### P0: qualify the reference and prove the loop

Width: coordinator plus up to four read-only or corpus-preparation lanes.
Feature implementation starts per lane after common U0.1 and U0.2 acceptance
and that lane's required positive U0.3 and baseline U0.4 proofs pass.
Every P0 proof remains required before its dependent behavior and campaign closure.
Independent rooted memory and path work does not wait for live IPC or lock fixtures.

| Item | Size | Prerequisites | Owned files | Acceptance |
|---|---|---|---|---|
| U0.1 reference and inventory | One coordinator slice | Execution grant | `dev/util-linux/`, this document | `python3 dev/util-linux/check.py inventory --check`: zero missing cohort reference commands, version conflicts, or unclassified installed commands. |
| U0.2 behavior proof harness | One coordinator slice | U0.1 | `dev/util-linux/`, new `core/tests/fixtures/util-linux/`, shared test registration | `python3 dev/util-linux/check.py harness --self-test`: zero failed controls and at least one rejected output, stderr, exit-status, side-effect, and over-normalization mutant. |
| U0.3 shared OS seams and gate mapping | One coordinator slice | U0.2 | Required `src/modules/`, registry, API docs and tests, gate mappings | `cargo dev gate` with the exact changed shared paths: zero new failures. The fixture proofs must acquire, observe, release, and reap an owned lock-holder; create and remove every IPC resource class; read a rooted memory fixture; and identify a private bind mount. Zero leaked resources. Freeze each lane's required contracts before that lane starts P1. |
| U0.4 baseline and resource schedule | One coordinator slice | U0.2 | `dev/util-linux/` and this document | `python3 dev/util-linux/check.py baseline --check`: five command results, selector timings, resource samples, and explicit missing behaviors. Zero unaccounted skips or stale-product results. |

The commands under `dev/util-linux/` are planned P0 deliverables. They do not
exist at planning time. Implement their documented interface first. Until
they pass their self-tests, do not use their results to declare parity.

U0.1 inventories util-linux 2.42.4 install targets and enabled build
features. Classify every command as complete, partial, absent, or explicitly
outside this cohort. Read the release's manuals, help output, and tests.
For every selected command, enumerate flags, operands, defaults, output modes,
columns, ordering, escaping, locale, terminal behavior, errors, exit statuses,
side effects, environment, and interactions. Each behavior gets a named case.
Import relevant upstream tests as native behavioral tests, preserving origin
identifiers. Use Rust only where native tests cannot express the boundary.

Reuse the scene, byte capture, and applet invocation conventions in
`core/tests/support/uu.xsh`. Add a small util-linux support module only where
reference selection and synchronization differ. Keep the Python checker a
lock-aware dispatcher and result checker. Do not add a second test runtime,
benchmark framework, origin database, or generic fixture language.

U0.2 runs reference and XSH against the same fixture inputs. Compare stdout
and stderr as bytes, statuses and signals, and fixture state after execution.
Normalize only identified nondeterminism such as actual PIDs and inode IDs.
Map aliases by fixture identity. Preserve distinct owners, holders, blockers,
and inodes. Reject swapped-holder and wrong-inode mutants in U0.2.
Do not normalize whitespace, JSON types, nulls, ordering, errors, or escaped paths.
Record one named intentional difference, `version-provenance`, in the behavior
inventory. For `--version`, compare success status and command identity across
implementations. Separately assert the exact XSH banner produced by
`gnu.version_text(command)` plus a newline. At this baseline that banner is
`NAME (XSH core) 0.0.1`. Assert the reference's actual util-linux 2.42.4 banner
separately. Preserve each implementation's provenance instead of replacing text.
Do not normalize other help, option, version-option parsing, or diagnostic differences.
No other intentional difference is preapproved. A newly discovered contract
difference remains open until resolved inside the stated reference contract.
Keep raw comparison beside parsed assertions where structured output matters.
Use fixture synchronization rather than sleeps for lock and process readiness.
Preserve raw path bytes until rendering. Name each operation's symlink policy.
Report expected disappearance and permission failures according to the reference.
Propagate unexpected I/O failures with their structured cause. Do not silently
drop rows or convert inaccessible values into zero. A fixture owner releases
resources on every exit path. Cleanup diagnostics cannot mask the primary failure.
Bound every child and clean it up on assertion failure, cancellation, and timeout.

Capture reference goldens once for stable fixture outputs. Keep a small
behavior table with the case selector, reference origin, and disposition.
Do not create a second provenance ledger. Existing GNU, uutils, and BusyBox
denominators stay separate. Use `jaq` for JSON and YAML inspection or updates.

U0.3 first proves that the real host API can observe a held lock through
the same process and inode identities as the reference. Cover POSIX, BSD flock,
and OFD lock classes, blocked requests, byte ranges, deleted paths, and disappearing
owners. Default and all-row `lslocks` comparisons run inside a private PID
namespace with freshly mounted procfs, so concurrent host locks cannot enter
the listing. Prove that an outside holder stays invisible to both commands.
Use owned fixture IDs to select rows for narrower shared-namespace probes. Keep `/proc` and `/sys` access in the collectors or host boundary.
No direct path literals enter top-level applets.

Before creating a new host API, test whether the existing collectors and
descriptor APIs suffice. Coordinate the exact shared paths with consolidation.
If consolidation has frozen those paths, build the native fixture and XSH
parsing independently, then integrate the host seam after the freeze lease ends.

### P1: complete the command cohort

Width: four implementation lanes and four disjoint parity lanes. Begin independent corpus work as soon as
U0.2 lands. Begin each lane's feature work after common U0.1 and U0.2 acceptance
and its required positive U0.3 and baseline U0.4 proofs pass.
`lslocks` is the first capability the coordinator accepts. Other lanes may
work concurrently, but no first-cohort close substitutes for complete `lslocks`.

| Lane and item | Size and prerequisites | Exclusive feature ownership | Capability and proof |
|---|---|---|---|
| A: U1 `lslocks` | One lock-inspection slice. U0.1–U0.4. | `core/lslocks.xsh`, new `core/lib/locks.xsh`, `core/tests/test-lslocks.xsh`, lock fixtures | Every reference lock class, column, filter, output mode, byte range, blocker and holder behavior. `python3 dev/util-linux/check.py parity --commands lslocks`: zero undecided mismatches or open behaviors. |
| B: U2 `lsipc` | One IPC-inspection slice. U0.1–U0.4. | `core/lsipc.xsh`, new `core/lib/ipc_report.xsh`, `core/tests/test-lsipc.xsh`, IPC fixtures | System V and POSIX objects, summaries, ID and name selection, permissions, timestamps, every column and output mode. `python3 dev/util-linux/check.py parity --commands lsipc`: zero undecided mismatches or open behaviors. |
| C: U3 `lsmem` | One memory-inventory slice. U0.1–U0.4. | `core/lsmem.xsh`, new `core/lib/memory_report.xsh`, `core/tests/test-lsmem.xsh`, rooted memory fixtures | Memory block ranges, online and configured states, range merging and splitting, NUMA nodes, zones, summaries, sysroot, every output mode. `python3 dev/util-linux/check.py parity --commands lsmem`: zero undecided mismatches or open behaviors. |
| D: U4 `mountpoint` and `namei` | One path-inspection slice. U0.1–U0.4. | `core/mountpoint.xsh`, `core/namei.xsh`, their dedicated tests, path and mount fixtures | File and directory mount detection, filesystem and block device numbers, symlink policy, quiet and show modes, path components, symlink chains and loops, file modes, owners, vertical alignment, error and status behavior. `python3 dev/util-linux/check.py parity --commands mountpoint,namei`: zero undecided mismatches or open behaviors. |

Existing collectors and shared libraries remain coordinator-owned, including
`core/lib/storage.xsh`, `core/lib/hardware.xsh`, `core/lib/sys_block.xsh`,
`core/lib/sys_mount.xsh`, `core/lib/system_report.xsh`, `core/lib/system_report_live.xsh`,
`core/lib/system_control.xsh`, and `core/lib/proc_launch.xsh`.
Lanes request a shared change rather than expanding ownership. No lane edits
Rust, Cargo files, command catalogs, shared gate infrastructure, this campaign
doc, or another lane's fixtures.

Use `linux.open_files()` for available descriptor identities. It does not
provide lock class, range, mode, or owner. Keep lock snapshots in a dedicated
collector and add only the missing host operations.
The existing `linux.is_mountpoint` compares a path and its parent devices
and inodes (`src/modules/linux/real/fs.rs:258`). That misses same-device bind
mounts and follows symlinks. Do not use it as a complete mountpoint oracle.
Reuse rooted mount identities or add a narrow coordinator-owned host seam
for the exact missing behavior, with bind-mount and `--nofollow` controls.
IPC fixtures require a parent-owned creator
and cleanup helper for real System V and POSIX objects. Reading `/proc/sysvipc`
alone cannot prove complete POSIX IPC behavior. Build that seam in P0. Use private IPC and mount namespaces, a private
mqueue mount, and private `/dev/shm` backing. Create at most 16 owned objects
per resource class, with at most 1 MiB per object. Track exact owned IDs and
names. Remove only those objects on every exit path. Never scan-delete objects.
Run permission cases under a real unprivileged UID and a second owner UID,
with capabilities dropped. A root-only permission probe cannot qualify them.

Each implementation lane ships command-specific documentation text with its
slice. Its parity worker ships the dedicated native corpus and fixtures.
The coordinator applies shared documentation and registry changes.
The lane report names contract changes, exact checks, remaining behaviors,
new shared-API needs, and checks integration must run.

Replace or resume a stopped lane in the same coordinator turn while disjoint
ready work remains. Integrate one accepted slice at a time. Assign its next
hardening slice immediately when disjoint work exists. Run checks on an immutable
verification worktree, so integration can continue without mutating tested source.

### P2: harden and document the completed behavior

Width: up to four disjoint hardening lanes, with shared work in the coordinator.
Prerequisite: all five P1 commands have no open ordinary behavior.

U5 is one hardening slice per lane, limited to its P1 files.
It owns race, permission, malformed input, non-UTF-8 path, closed descriptor,
short-write, broken-pipe, locale, and large-fixture cases discovered during P1.
Run `python3 dev/util-linux/check.py parity --all --require-positive-boundaries`.
Threshold: zero undecided mismatches, zero missing cases, and all mandatory positive
host boundaries executed in their qualified contexts.

U6 is one coordinator slice after U5. It owns shared documentation, API sources,
and inventory registration. Update `docs/CORE.md` and generated API sources
when needed. Run the changed-path gates and
`python3 dev/checks/check_inventory.py --check`.
Threshold: zero failures and exact dispositions for every new command.
Run the repository-contract test only when its shared gate lease is available.
Do not alter an existing upstream exclusion without evidence and a stated reason.

### P3: measure resources and optimize only demonstrated regressions

Width: one measurement owner. Independent documentation review may continue.
Prerequisite: U5 and U6 pass. Use final release products from one frozen source.

U7 is one coordinator slice owning only the acceptance adapter and resource
fixtures. Measure binary size, build cost, peak RSS, and command CPU time on the
same fixtures used for reference comparisons. Add no unrelated speed target.
For existing regression targets, compare baseline and candidate from matching source,
profile, target, image, and allocation. For new commands, compare against the
qualified reference and report XSH runtime overhead separately.

Use five interleaved baseline/candidate samples and report CPU-time minimum,
median, and spread. Check the result and completed work before accepting each
sample. Profile outside the reported timing to identify the limiter.
Reject an existing-command regression larger than both 10% and
the observed spread. Startup CPU time has an additional 5 ms absolute allowance.
Candidate peak RSS may grow by at most the larger of 10% or 8 MiB.
The three product binaries may grow by at most 1% each.
Native applet source size is reported separately from product binary size.
A larger change needs
a cause and a smaller implementation before closure, not a changed threshold.
Measure build CPU cost with the same warmed caches and compile concurrency.
Reject growth above both 10% and observed spread.

Run time-based gates only when `perf-gate-ready` exits zero, then run
`perf-gate-ready -1` immediately after each measurement, even after failure.
Discard overlapped runs. If the host stays busy, report
"perf gate deferred: host busy". Deferred gates prevent completion but do
not block functional implementation. Always run memory and binary-size checks.

Proof: `python3 dev/util-linux/check.py resources --check` exits zero with all
thresholds met and no deferred required measurement. The checker must reject
incomparable builds and discarded readiness results.

### P4: close

Width: coordinator, with no feature lane running.
U8 is one coordinator slice after U7. It owns integration, cleanup, and the
final report. Rerun all cohort comparisons and the existing focused tests affected by
shared changes on the integrated source. Run one full native suite because
shared collectors, command registration, and host primitives cross lane boundaries.
Run the applicable Rust host-boundary groups, repository contracts, and docs checks.
Do not run unrelated workspace-wide builds by default.

Run an adversarial review of the integrated diff and fix all P1 and P2 findings.
Prove unused helpers are dead before removing them. Keep the reference runner
as the executable compatibility oracle. Remove temporary implementation shims.
Finish with no running owned process and no lost uncommitted work.

Proof: `python3 dev/util-linux/check.py close --check` exits zero only when
deliverables 1–6 pass, all mandatory behaviors are covered, all required gates
executed on the final source, and no required performance measurement is deferred.
Report upstream cases, native ports, host cases, conditional skips, exclusions,
and executed reference comparisons separately. Do not publish an aggregate
util-linux compatibility percentage.

## Acceptance commands

Native `xsht test` cases and support fixtures own behavior comparisons.
P0 implements the thin checker interface above and the following selectors:

```sh
python3 dev/util-linux/check.py parity --command lslocks --case NAME
python3 dev/util-linux/check.py parity --commands lslocks,lsipc
python3 dev/util-linux/check.py parity --all --require-positive-boundaries
python3 dev/util-linux/check.py resources --check
python3 dev/util-linux/check.py close --check
```

Acceptance permits only the named `version-provenance` difference above.
Zero undecided mismatches means all other comparisons match exactly and that
exception passes both implementations' independent identity assertions.
Every selector fails when no behavior matches. Every parity result requires
an executed reference and candidate, matching fixture inputs, and exact
comparison rules. A missing reference, killed child, timeout, stale executable,
or fixture setup failure fails the case. It cannot produce an empty passing run.

Use existing native selectors for iteration, for example:

```sh
target/x86_64-unknown-linux-musl/verification/xsht test --exact core/tests/test-lscpu.xsh::test_lscpu_fixture_preserves_cpu_identity_and_offline_selection
```

Run Linux commands inside the pinned image, using the target flags from
`dev/targets.xsh::docker_test_env` and `kache`. The coordinator runs
`cargo dev gate PATH...` after integrating shared source or API changes.
Do not let each feature lane rebuild all products.

## Coexist with consolidation

Use `.work/wt/util-linux-integration` for integration and
`.work/wt/util-linux-{locks,ipc,memory,mountpoint}` for lanes.
Use a separate `.work/wt/util-linux-verify` checkout for frozen checks.
The planning worktree is `.work/wt/util-linux-campaign-plan`.
Never edit frozen consolidation source while its gates run.

Keep four implementation lanes active when ready work exists and the measured
headroom supports them. Give each lane at most one CPU and 1 GiB RSS.
Cap the util-linux functional cohort at four CPUs and 4 GiB total memory.
Reserve at least 8 GiB available memory for consolidation and the host.
Reduce execution width when headroom falls below that reserve, not by allowing
swap or OOM. Keep read-only design work active in the paused feature lanes.
Resample lane and host memory after each first check and integration.
The 4 GiB cap covers the four feature lanes. The coordinator alone builds
with a two-CPU and 8 GiB memory cap after acquiring the shared Cargo lease.
Keep the same 8 GiB host reserve. Compiler parallelism is at most two jobs.
Do not overlap that build with feature process execution or borrow unmeasured
memory. If that cap cannot build the scoped products, stop the build, diagnose
its peak, and reduce the selected targets before widening resources.

All util-linux native execution is serialized under the original repository's
`.work/consolidation/infra/native-suite.lock`, including bounded focused cases.
All builds use its `.work/consolidation/infra/cargo.lock`.
Timed measurements additionally use its `final-performance.lock` exclusively.
Resolve these paths against the original repository, never the lane worktree.
Open the existing lock files without replacement or truncation. Keep the same
physical lock inodes across both campaigns. Never create a private competing lease.
Use the existing controller's lock acquisition interface after inspecting it.
The coordinator owns any necessary lock-aware wrapper and registration change.
Acquire leases in this order: performance, native, then Cargo, taking only
those needed. Ordinary builds and process execution hold a shared performance
guard throughout their work. Timed gates hold it exclusively. Inspect the
existing controllers before choosing the wrapper so both campaigns enforce
that policy atomically. Never check the lease and release it before starting.
P0 tests cross-campaign exclusion: an ordinary util-linux worker cannot start
under the consolidation exclusive guard, and the exclusive guard cannot start
while that worker runs. Also prove native and Cargo serialization.

Consolidation final gates have priority. Do not queue a util-linux timed gate
ahead of them. During a consolidation performance lease, suspend util-linux
process execution and builds. Continue only work allowed by its quiet-host rule.
Parallel source reading and independent nonexecuting checks are allowed outside
that lease. Never count a contended performance attempt as accepted.

## Execution boundary

The owner authorized execution and local commits on 2026-10-17 by accepting
the launch prompt. That grant covers this campaign in its isolated worktrees.
This document does not grant authority beyond the owner’s request.
Pushing and releasing remain forbidden. Keep the primary consolidation
checkout frozen while its source-bound gates run.

An execution grant covers repository implementation, isolated local reference
fixtures, and local verification within the resource and platform limits above.
It does not cover host disks, host mounts, host scheduler changes, global clocks,
package installation on the host, publication, deployment, or pushing.
Use owned child processes for scheduling tests. Use private namespaces and
disposable fixture devices for cases requiring privileges. Park a case whose
required isolation cannot be established and continue independent work.

Surface added command interfaces, previously unsupported options, exported
host APIs and types, error and status changes, and resource ownership changes
in the final report. Update code, tests, and documentation together.

## Non-goals and follow-up inventory

Do not expand this first cohort to all util-linux commands. P0 still inventories
the full selected release so the next cohort has an honest starting point.
Defer `lsfd`: its descriptor-type and column surface is much larger than
`mountpoint`, and existing `linux.open_files()` does not establish full parity.
The initial source survey yields these follow-up groups:

| Group | Existing applets to assess for full parity | Absent capabilities to inventory |
|---|---|---|
| Storage | `blkid`, `blockdev`, `fdisk`, `findmnt`, `fsfreeze`, `fstrim`, `losetup`, `lsblk`, `mkswap`, `mount`, `partx`, `sfdisk`, `swapon`, `swapoff`, `umount`, `wipefs` | Further release install targets from U0.1. |
| Processes and namespaces | `chrt`, `flock`, `ionice`, `lsns`, `nsenter`, `pivot_root`, `prlimit`, `setpriv`, `setsid`, `switch_root`, `taskset`, `unshare` | `lsfd`, `setarch`. |
| Hardware and memory | `lscpu`, `rfkill` | `chmem`. |
| Terminals and accounts | `more`, `su` | `lslogins`, `script`. |
| IPC and miscellaneous | `dmesg`, `hwclock`, `rev` | `ipcs`, `ipcrm`, `ipcmk`, `getopt`, `logger`, `uuidgen`, `uuidparse`. |

This table is a broad source inventory, not the release denominator.
Do not count `lsof`, `fuser`, `partprobe`, `lsmod`, `lsattr`, `chattr`,
`lspci`, `lsusb`, or `free` as util-linux coverage. Their upstream owners differ.
Do not equate an applet file with parity.

Do not change macOS or ARM support, consolidation acceptance, compiler design,
interactive behavior, packaging, CI, or pinned nightly. Do not optimize before
the complete cohort passes. Do not forward command behavior to installed tools.

## Verified code map

| Starting point | Treatment |
|---|---|
| `core/lsblk.xsh:1`, `core/findmnt.xsh:1` | Reuse as storage regression targets. |
| `core/lib/storage.xsh:514`, `core/lib/storage.xsh:635` | Reuse rooted storage integration regressions. |
| `core/lib/sys_block.xsh:21`, `core/lib/sys_block.xsh:120` | Reuse block identities and rooted collection. |
| `core/lib/sys_mount.xsh:14`, `core/lib/sys_mount.xsh:261` | Reuse mount identities and rooted collection. |
| `core/lscpu.xsh:1`, `core/lib/hardware.xsh:306` | Reuse CPU inventory and regression targets. |
| `core/flock.xsh:1`, `core/lib/system_control.xsh:1` | Reuse lock acquisition and regression targets. |
| `core/lsns.xsh:1`, `src/modules/linux/namespace.rs:7` | Reuse process and namespace identity boundaries. |
| `core/chrt.xsh:1`, `core/lib/proc_launch.xsh:1`, `core/setsid.xsh:1` | Reuse process launch and scheduler regression targets. |
| `core/rev.xsh:1`, `core/lib/textio_a1.xsh:1` | Reuse existing text-command regression coverage. |
| `core/tests/test-storage.xsh:63`, `core/tests/test-storage.xsh:130` | Reuse rooted storage fixtures. |
| `core/tests/test-lscpu.xsh:4`, `core/tests/test-flock.xsh:7` | Reuse CPU identity and lock conflict proofs. |
| `core/lib/gnu.xsh:13`, `core/lib/gnu.xsh:375`, `core/lib/gnu.xsh:412` | Reuse XSH version identity and the exact banner helper. |
| `dev/checks/check_inventory.py:1`, `dev/checks/surface.json` | Extend live inventory through coordinator-owned changes. |
| `dev/consolidation/final_canonical_replay.py:24` | Reuse original shared leases. |

The reference source is the [util-linux v2.42.4 tag](https://github.com/util-linux/util-linux/tree/v2.42.4).
Its upstream tests provide a starting corpus, not complete coverage:

| Command | Upstream evidence and gaps |
|---|---|
| `lslocks` | [15 upstream subcases](https://github.com/util-linux/util-linux/blob/v2.42.4/tests/ts/lslocks/lslocks) cover lock classes and duplicated holders. Add JSON, filters, inaccessible paths, blockers, and races. The reference exposes 13 columns. |
| `lsmem` | [Rooted dump tests](https://github.com/util-linux/util-linux/blob/v2.42.4/tests/ts/lsmem/lsmem) exercise nine invocations per dump, including split modes, raw output, JSON, and all columns. Extend boundary fixtures. |
| `lsipc` | [Six resource modes](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/lsipc.c) include System V and POSIX resources. The release has no dedicated `tests/ts/lsipc` suite. P0 must create the missing behavior corpus. |
| `mountpoint` | [Three upstream cases](https://github.com/util-linux/util-linux/blob/v2.42.4/tests/ts/mountpoint/mountpoint) cover symlink and option conflicts. Add bind mounts, device numbers, errors, and show behavior. |
| `namei` | [The upstream logic script](https://github.com/util-linux/util-linux/blob/v2.42.4/tests/ts/namei/logic) covers plain fixture paths. Add every flag, relative and absolute symlinks, loops, permission failures, and owner rendering. |

Read [core behavior](CORE.md), [testing](TESTING.md),
[architecture](ARCHITECTURE.md), and the
[active consolidation campaign](../dev/consolidation/CAMPAIGN.md) before execution.

## Decision log

- 2026-10-17: Preserve the existing XSH `gnu.version` banner as the sole intentional provenance difference from the reference.
- 2026-10-17: Prescribe a five-command Linux cohort to provide four ownership lanes and a complete first `lslocks` capability.

## Campaign log

- 2026-10-17: The owner authorized execution alongside consolidation. Launch four lanes on reference corpus and fixture preparation; feature implementation waits for P0 acceptance.
- 2026-10-17: Admit lanes independently after the common guarded reference controls and their own positive boundary proofs pass. This preserves each proof while allowing rooted memory and path work to proceed independently of IPC activity fixtures. Common inventory, quarantine controls, typed signal capture, two unprivileged identities, rooted no-fallback memory, private same-device bind and nofollow, and five explicit missing-command baselines passed. Memory and path feature work may proceed. Lock and IPC fixture lifetime qualification remains open.

## For the owner afterwards

- Read the command-level compatibility summary and the exact skipped boundaries.
- Review added command and host API contracts.
- Review integrated commits only when local commits were authorized.
- Use the full-release inventory to choose a later cohort after this one closes.
