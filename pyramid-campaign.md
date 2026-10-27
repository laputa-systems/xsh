# Scoped verification across XSH and Laputa

Status: winding down on master. The first slice is integrated; P0 and campaign
acceptance are open. The owner directs Pyramid before inference and Lean; this
order replaces the earlier post-inference plan. Qualify Pyramid on
current source and rerun affected checks when inference changes their inputs.
See [the roadmap](docs/ROADMAP.md).

Automatic discovery now reads staged, unstaged and untracked Git paths with
NUL delimiters and rejects non-UTF-8 output. The Docker launcher scopes Git's
safe-directory setting to the checkout. The selected gate-test module passed
49 tests in the replacement image. Native file filters are discovered and run
as exact ID sets within the case budget; required skips and empty selections
return incomplete. Timed child inspections check selected product digests and
modes, plus tracked and untracked Git inputs and the index, from host selection
through preparation and leaf execution. Changed or unavailable inputs return
incomplete. The Tokei
Edit case also passes in the replacement image. Ignored and external inputs,
harness preparation and atomic binding remain open; these
checks do not establish P0.

Source observation and automatic path discovery now discard inherited Git
overrides and ignore replacement refs. Discovery returns incomplete for
`assume-unchanged` or `skip-worktree` index entries that could hide edits;
explicit cases still work.
The source observer now binds staged entries and persistent index flags rather
than Git's cached stat data. A stat-only index refresh leaves the digest stable;
staged changes, intent-to-add, hidden flags, and conflicting stages still
change it. The host and Linux source-owner regressions pass. This remains an
observation across separate reads, not an atomic snapshot.
Host and Linux controls cover poisoned indexes, replacement refs, hidden index
flags and linked-worktree metadata.
An existing regular-file symlink target must also be in the observed source
set; otherwise inspection returns incomplete. The regression passes on the
host and in Linux. Linked directories reject unobserved entries and include
directory names and modes in the observation. Resolved symlink targets are
bound even when an intermediate link is ignored. Other ignored inputs remain
unbound.

The small Tokei fixture now checks the exact selected multilingual file set and
the complete table bytes, including ignored and unsupported inputs. Its focused
Linux test passes; the pinned upstream and substantial-corpus comparison remain
open.
Selected files that cannot be read now fail both Tokei output modes; a host
and a Linux regression cover the failure.

The Edit catalogue now selects three exact local-inference behavior tests for
`src/sema/check/local_inference.rs`; Change retains the broader checker gates.
The ownership control passes on the host and the selected tests pass in Linux.
`src/sema/schemes.rs` likewise selects three imported-callable tests at Edit
tier while retaining broader checker coverage at Change tier. Its ownership and
selected tests pass on the host and in Linux.
The Edit catalogue also selects the exact inference-work isolation and
instantiation-count Rust test. Its ownership control and the source-bound
Rust test pass in the replacement Linux image.
The frontend-retention Edit case selects the retained-column stability test;
its ownership control and prepared local Rust harness pass, as does the Linux
gate. Wider disposal measurements remain open.
The explicit frontend Edit scenario validates runtime output and frontend
evidence for annotated and reduced forms of two generated workloads. Its
source-bound Linux run passes. Disposal execution, scaling, and performance
remain unqualified for those workload pairs. Separate exact Linux scenarios
pass the imported-scheme and compact-entry execution controls after frontend
state is dropped. The shared process controller distinguishes observed
nonzero child exits from controller cancellation and incomplete timeouts.
A bounded host probe of the eight 64-unit Change candidates passed dependency
chain, generic stream, wide records, and finite overload. Alias chain and
higher-order composition exceeded the indexed type-materialization work bound;
reduced recursive-effect and module-diamond inputs failed checking. This probe
is not a source-bound Linux scenario. The wider frontend scenario remains open.
Inference workload generator and measurement changes now select their Python
contract tests instead of generic native developer tests. The selection control
and all 30 contract tests pass on the host and in the qualified Linux image.
Corpus tooling and witness paths now report unresolved instead of selecting
unrelated native developer tests. The selection regression passes on the host
and in a bounded Linux run. Explicit external-input ownership remains open.
Frozen-witness tooling now selects its 19 Python contracts, which pass on the
host and in the qualified Linux image. Its pinned Git reads scope checkout
trust to that invocation and ignore inherited Git overrides and replacement
refs. Other corpus ownership remains open.
The constraint-solver Edit case selects an independent row and rollback oracle;
its ownership control and prepared local Rust harness pass, as does the Linux
gate. Solver scaling measurements remain open.

The cat Edit case selects four exact byte-stream and error tests. Change selects
the full cat module and 66 identified external consumers instead of the broad
Core suite. The source-bound Edit and Change gates passed in the replacement
image. The permission-error fixture runs its command unprivileged when the
test runner is root; its locale-adjusted Change gate passed all 79 selected
tests in Linux after resource admission.

The uniq Edit case selects four record-grouping and streaming tests. The find
Edit case selects four traversal and path tests. Their ownership controls pass
on the host and both source-bound cases pass in the replacement Linux image;
the wider record/order and traversal workloads remain open.
The sort, xargs, AWK, JSON runtime and effect-inference Edit cases select
ordering and spill, process and stream, record, structured-data, and capability
tests respectively. Their ownership controls pass on the host and all 21
selected tests pass in Linux after resource admission. Wider workload coverage
remains open.
The explicit `performance/core-wc-small/correctness` scenario checks a 64 KiB
binary input against an exact line-count oracle. The focused Linux scenario and
fixture controls pass. Its resource diagnostic is incomplete because cgroup
pressure accounting is unavailable; performance remains unmeasured.
Medium and large `wc` fixtures also pass exact-output Linux diagnostics with
unchanged source and product observations. The medium scenario stopped before
execution when host admission was unavailable. Later source-bound Linux gates
passed both scenarios. Their pressure diagnostics remain incomplete.
Explicit small sort, traversal, and AWK scenarios also pass their exact output
oracles in Linux. The medium sort scenario passes as well; two large-sort
attempts stopped before execution because resource admission was unavailable.
The large sort output oracle passes in a bounded host diagnostic.
Their pressure diagnostics are incomplete, and performance remains unmeasured.
Medium and large AWK and traversal scenarios are registered. Their exact output
oracles pass in a bounded host diagnostic; the medium AWK Linux gate stopped
before execution when resource admission was unavailable. The other new Linux
scenarios remain unverified.
The pipeline fixture's trace emits segment completions without helper-start
events. The reader reconciles them against explicitly declared segment counts
and rejects incomplete or mixed traces and missing or duplicate process event IDs.
The small, medium, and large Linux
scenarios check exact output and four, 16, and 64 completed helpers respectively.
Pipeline performance and resource qualification remain open.
Host syscall-trace probes completed for small `wc` and sort. The sort JSONL
trace exceeded the previous 16 MiB reader limit; a bounded streaming reader now
reconciles it without loading the whole file. Its 128 MiB total and 1 MiB
per-record limits leave larger traces and core syscall scaling unqualified.
The explicit small `wc`, sort, AWK, and traversal trace scenarios pass their
exact outputs and reconcile zero helpers and syscall summaries in the qualified
Linux image. Their syscall counts include tracing machinery and do not establish
production performance.
Medium `wc` and medium and large traversal trace scenarios also pass in the
qualified Linux image, including exact output and zero-helper reconciliation.
Ordinary run attribution matches each completion to its start. A host trace
of the existing 128-helper workload reconciles all 128. A source-bound Linux
scenarios also check two, eight, and 32 spawned children, their successful
waits, and a no-op cancel after wait. Detached and live-cancellation traces and `wait_any` and
`wait_ready` remain incomplete. The trace reader requires root framing,
contiguous event IDs, valid parent references, and a final syscall summary.

P0 reference candidates are [upstream Tokei v14.0.0](https://github.com/XAMPPRocky/tokei/releases/tag/v14.0.0) at commit
`8cdd6fa3a54f8cd69442d2f00effb29aa3110353` and the local uutils/coreutils
commit `e7c9f3194280835c4487c2945c68d5f01ccacc8d`. The latter can be archived
without its live checkout's untracked file and has no tracked submodules or
`.tokeignore` files. Language selection, file reconciliation, worker affinity
and output policy still need qualification before these become P0 inputs.
The test image's existing offline cache lacked Tokei's `arbitrary` dependency.
Locked dependencies were fetched into disposable workspace storage and the
reference was built offline in the replacement image. A later build used a clean
checkout of the pinned commit, mounted read-only in that image, with the locked
dependencies offline. Its binary SHA-256 was
`53c96c158d369d7bf8ed775c801191a6e8947cff7f47d7155e9c39afc26ca3ba`, matching
the earlier reference. After the build, all 204 available registry source trees,
including the 122 compiled registry crates, matched lockfile-verified archives.
Another offline build used a root-owned, read-only archive of the pinned commit
and a read-only registry snapshot in the replacement image. Its 204 cached
archives matched the lockfile, and the corresponding source trees matched the
previously checked archive extractions; all 122 compiled registry crates came
from that snapshot. Its binary matched the retained reference byte for byte.
The build still used the host's mutable linker and compiler cache, and the
reference artifact and full P0 remain unqualified. An earlier failed rebuild
omitted the linker target and did not hold the shared build lease; its artifacts
were not used for this result.
A fresh offline build used read-only copies of the linker, linker wrapper, and
kache, with kache's cache disabled and a new target directory. The resulting
binary has SHA-256 `3dae78ab69cc013325c7509320d510530b6ec332565b6b97772f390e160036bf`,
which differs from the earlier cached build. A second cache-free build with a
fresh target directory produced the same bytes. The reason for the difference
from the cached build is not established. The corpus scenario initially stopped
before execution because host admission was unavailable. A later source-bound
Linux run passed file-set reconciliation with the new binary and reproduced the
counts below. The scenario does not qualify reference provenance or P0.
An archive projection containing 671 tracked, non-hidden Rust files was checked
with host XSH and the offline Linux reference: both selected exactly that set.
XSH counted 213,988 code, 18,298 comment and 30,443 blank lines; upstream
counted 214,731, 18,027 and 29,973 respectively. These are preliminary output
observations, not source-bound acceptance. An affinity-limited host probe also
confirmed that XSH reports the two available CPUs. Timing and memory remain
unqualified.
The integrated corpus helper also projected the pinned Git blobs directly and
reconciled real Linux XSH and upstream JSON reports: both selected all 671 files.
The explicit Tokei scenario now passes the same file-set reconciliation in the
replacement Linux image, binding XSH to the selected source and checking the
reference binary's stability. Its Linux report also records the line counts and
differences above. Reference source provenance and controlled
performance comparison remain open.

Native cases now reconcile XSHT's structured events against the selected IDs and
child status. Missing or contradictory results make a successful run incomplete;
an observed child failure retains failure precedence. Prepared Cargo harnesses
are inspected across execution and reuse, and their selected libtest results
are reconciled. Focused Linux controls cover pass, ignored, failed, missing,
and changed-harness cases; a real nightly libtest probe confirmed pass, ignored,
and failure outcomes. Other workload reports remain to be integrated.
XSHT now retains status 1 for an observed case or coverage failure even when
cleanup or final event writing also fails; the available secondary diagnostics
remain in the event reason or stderr. The Laputa adapter can retain an exact-scope
failed or timed-out event prefix when a final stream or source inspection is
unavailable. Focused Rust, native gate and Python controls pass in Linux.

Native discovery and exact-set execution now exchange JSON files, preserving
newlines and avoiding argument-size limits. The file reader rejects nonregular
or oversized inputs. Five targeted Rust CLI tests and the focused Linux gate
controls pass. The feature-disabled XSHT library now compiles in Linux and
returns its explicit unavailable-runner diagnostic; a focused regression and
the default-feature library check pass in the replacement image.

Laputa now exposes a pure `pm.make.plan_tasks` graph inventory that validates
structure and cycles without executing tasks or inspecting files. Six focused
Linux tests cover its inventory and the retained scheduler behavior. The
cross-repository plan case passed its two selected tests in the replacement
image, with observed source and product binding and reconciled events. Source
inspection now includes checked-out Git submodules; its regression covers nested
modified and untracked files. Atomic binding and other Laputa cases remain open.
An isolated PM executor regression now checks actual build-hook counts across a
fresh build, reuse, and a dependency change; it passes on the host. Its explicit
scenario prepares and binds XSHI as well as XSH and XSHT and passed in the
qualified Linux image. Broader world coverage remains open.
The isolated executor fixture also changes only the application recipe and
checks that its payload changes while both dependency receipts are reused.
The focused host test and Linux scenario pass. A real package rebuild remains
open.
The catalogue also exposes the existing synthetic Kbuild discovery test as an
explicit Laputa scenario. Its adapter and host selection controls pass, and its
selected test passed through the qualified Linux image.
The Kbuild graph-cache scenario now selects twenty-one isolated graph parity, cache
invalidation, EFI stub, plan-output, and selected-closure tests.
All twenty-one pass in the replacement Linux image after resource admission,
including the archive-plan and parallel snapshot controls.
The closure regression exposed a stale dependent after an upstream rebuild;
the scheduler now reruns declared dependents when a dependency ran. Its host
regression and the PM make tests pass.
The same closure fixture checks the exact `linux-kbuild-stopped` plan error and
the written plan report; it also passes in Linux.
PM make now compares nanosecond timestamps, fixing missed same-second input
changes; its regression passes on the host and in Linux, and the PM make tests
pass on the host.
An offline preliminary discovery of Laputa's checksum-pinned Linux 7.2.9
source with its x86 configuration found 716 directories and 1,477 objects, but
reported six unsupported items. The planner now handles the nested Make
functions and assignment replacement responsible for those diagnostics; the
same local probe reports no unsupported items. The real-kernel scenario and
its complete inventory remain unqualified.
A bounded host diagnostic on a separate copy of that source produced 3,064
archive tasks, 25 missing sources and one generated object without the recipe's
prepare stage. This checks planner execution only; it is not a real-kernel
inventory or qualification.
A subsequent isolated Linux probe used the checksum-pinned kernel and LLVM
23.1.0-rc2 archive, with an image CRT wrapper, to run the recipe's x86 prepare
stage and reach the exact plan stop. Its plan reports 3,114 tasks, no missing
sources, and no duplicate outputs. The planner now represents x86 EFI stub
objects as source compilation followed by the existing objcopy task; a
two-task `gop.stub.o` closure built, reused unchanged outputs, and rebuilt both
outputs after a controlled source edit. Its stub matched the pinned objcopy
transformation. The EFI copy helper now checks the stripped object's relocations
before copying and binds its digest into both the plan cache and task signature.
With pinned LLVM, a relative relocation was accepted and an absolute relocation
was rejected without changing the existing output. The probe bypassed the full
package staging context and has no independent complete-inventory oracle;
those remain unqualified.
After the relocation helper landed, a fresh prepared kernel copy built the
same two-task closure and reached the compile stop. An unchanged rerun preserved
both output timestamps. Adding a newline to the copied `gop.c` rebuilt both
outputs and reached the same stop. This remains a diagnostic outside the full
package staging context.
The local scan-record cache now uses format `linux-local-records-v4`, so records
created by the old planner are rebuilt instead of reused.
Compile-flag caches and discovered-plan fingerprints are likewise versioned
forward; focused host regressions show the default path rebuilds old entries.
Explicit plan-cache trust remains an override.
Default fingerprints now include the architecture, cached plan, reachable Kbuild files and active includes;
the public `kbuild.plan_fingerprint` requires the source architecture. Host
regressions cover changed nested and included inputs and pass in Linux.
Local scan records now carry digests of the Kbuild and active include text
consumed by each scanner and worker, and their cache keys bind the parsed
configuration. Plan-cache fingerprints now use those discovery records and the
configuration used by the workers. Compile-flags cache misses bind the text
actually parsed. Archive planning prepares flags and source selections once,
shares them with workers, and validates report and summary bytes before reuse.
These observations are not an atomic source-tree snapshot.
Process-pool discovery now shares one parsed configuration with every worker;
its public entry takes `Kconfig` and requires the `env` effect. The pool uses
`TMPDIR` for its scratch state. The two-worker regression passed in Linux.
The focused Make-semantics scenario, including the subdirectory-flags control,
passed five tests in the replacement Linux image after resource admission.
The explicit PM root-composition scenario selects five isolated inventory,
ownership, link, and collision tests. Its selection and adapter controls and
selected tests pass on the host, and the source-bound scenario passes in the
replacement Linux image.
The explicit PM package-inventory scenario selects four synthetic graph,
reuse, dependency-kind, and executor-provenance tests. Its adapter, selection,
and selected tests pass on the host; the source-bound scenario also passes in
the replacement Linux image.
The explicit world host-plan scenario checks selection and mirror environment
handling in an isolated fixture. Its adapter, selection, and selected host test
pass, as does the Linux scenario after resource admission. The real world
inventory remains open.
An offline diagnostic planned the Linux root from Laputa `32db65e` with the
historical `429f6ff` XSH seed in a disposable repository projection. The plan
contains seven build nodes: m4, musl, xsh, llvm-toolchain, bison, flex, and
linux. Their levels and dependency kinds match the recipe declarations and
bootstrap policy. This single-root diagnostic does not qualify the current seed,
the full world inventory, artifact reuse, or a package build.
Another offline diagnostic built and proved the real m4 and xsh recipes in a
disposable store with the same historical seed. An unchanged rerun returned
identical receipts. After adding a newline to the copied m4 source, a second
plan built a different m4 payload and reused the xsh receipt; both receipts
passed store verification. Xsh is a runtime-only dependency of m4. The probe
does not qualify a current-seed build or full-world reuse.
A separate disposable generation from those two real recipes passed package
ownership, the `usr/bin/sh` link, receipt, and root-content checks. Its m4
script produced the expected output using the container's mounted verification
XSH. The diagnostic uses the historical seed and does not prove a bootable root.

The `performance/json-n/correctness` gate passed its output oracle and cleanup
checks in the replacement image, bound to the selected source and prepared
product. Performance remains unmeasured. Its fresh-container diagnostic sampled
cgroup counters but reported incomplete because `cgroup.pressure` was unavailable.
The later rerun was incomplete because resource admission was unavailable.
Required resource measurements and performance qualification remain open.

The approved Linux test image `sha256:ef7c796588552777825857dc3b8ffca9efe533112a4c340687e625742d93da94`
is unavailable locally. A gate invocation rebuilt the default `xsh-test` tag
before the image was pinned; the approved image could not be found afterward.
The owner authorized qualifying the replacement image
`sha256:6e97d46216b8e3be9c7ca2ee1c28872c5eee5d7531667ff948fb7a7c67e82db9`.
It has the pinned nightly compiler and has passed the selected checks above.
Earlier results from the previous image do not qualify the replacement.
The gate resolves the configured image to an immutable ID before launch; the
Tokei Edit case passed through that path. Focused controls cover missing images,
identity mismatches and cancellation during image inspection.
An exploratory gate in a delegated worktree later rebuilt the shared
`xsh-test:latest` tag, leaving this approved image unavailable. The owner
authorized qualifying its replacement,
`sha256:dcb4231618e78bfbdd242360ce51c67fb64991eff9db3d80db4c1b476d8bc637`.
It has the pinned nightly and passed the five-test Tokei Edit gate and eleven
focused source, dispatch and compiler controls. It is retained under a separate
local tag and used by immutable ID for subsequent Linux checks.
Some subsequent launches checked load without separately checking resource
status with `perf-gate-ready -s`. Their behavior results are provisional for
campaign admission and must be rerun under both checks.

Landed from `campaign/pyramid-port`:

- `xsht test --exact-id ID` sets and `--events-jsonl PATH` schema-1 results.
- The component catalogue behind `cargo dev gate`: `--plan [--json]`,
  `--tier edit|change`, repeatable `--case ID`, bounded execution and the exit
  mapping below. Cases derive from the existing lane plans, plus
  `showcase/tokei/correctness`, `core/lsmem/correctness` and
  `core/lsmem/consumers`.
- `cargo dev gate --full` as the only Full entry. `--batch` and
  `internal gate-batch` are gone; Full validates its closure before Docker.

Parked on `campaign/pyramid-integration` (`c7b9b7b29`, built on an older
provisional inference compiler, so port by path rather than merge): product and
input binding, stale-evidence status, standalone host execution and mixed
host/native cases, lease and resource accounting, focused performance and
workload adapters, Tokei, core-workload, frontend and Laputa PM cases, and the
final-replay validation boundary. Each is the P1/P2 lane in the table below and
needs its own port against the current source.

Pilot, `core/lsmem.xsh` edit, 2 CPUs, in-container operation run on the host
because the pinned image was absent:

- Selection: `core/lsmem/correctness` (22 tests in `test-lsmem.xsh`) and
  `core/lsmem/consumers` (two exact IDs in other modules that run the applet).
  Both namespace-dependent consumers skip in either route, so they are excluded.
- Pyramid: 35 s warm (34.5, 35.3, 35.1) and 60 s cold including a 16 s product
  relink. The existing `gate PATH` selects `xsht test core/tests`: 9,473 tests
  in 515 files, 3,386 s including a 50 s product build, single run.
- Confidence: the 24 selected tests are every `core/tests` test that runs the
  applet and does not skip, and they pass in both routes. The existing run is red on this host (1,453
  environment failures, none in lsmem), so it gave no verdict either.
- Not established: `xsht check`/`lint` of the file, Rust integration tests,
  other applets, and the container itself. Neither route was run in Docker, and
  32-CPU and `--full` turnaround were not measured.

For a change, select the smallest declared verification scope, explain each
selection, prepare only its required products, and report what the result
establishes. Keep comprehensive verification behind explicit manual requests.

Deliver:

1. One component catalogue behind `cargo dev gate`, with inspectable ownership,
   exact selection, resource bounds, and enforced manual-only targets.
2. Exact-set native execution, versioned structured results, and deterministic
   reports across existing Cargo, XSHT, performance, and Laputa runners.
3. Independently valid workload measurements, immutable product/input binding,
   host reservations, contamination rules, and correct memory scopes.
4. End-to-end cases for inference, core command mechanisms, Tokei, PM/world,
   and kernel planning, with larger scenarios visible but never implicit.
5. A fast development loop, regression controls, migrated callers, and removal
   of superseded selection/reporting paths.

## Charter

Follow `orchestrate` and its campaign reference for workstreams, integration,
and worktree lifecycle. Keep four lanes active when four disjoint assignments
are ready and review and verification capacity can accept them. Keep a ceiling
of six including the coordinator. Replace or resume stopped lanes in the same
turn when that capacity is available. Overlap integration and the next tranche.
Parallelize verification only on disjoint inputs and output directories.

Apply the parent steering checkpoint after each validation wave, on resume,
and after repeated failures or queue growth. Record the accepted result,
provisional work, limiting stage and next substantive milestone here. Keep at
most one unqualified checkpoint per lane. Reduce active writers when review or
verification cannot consume their output. Revise agent-chosen methods within
the grant without weakening deliverables, acceptance or permission boundaries.

Use `.work/wt/pyramid-<lane>` and each checkout's `.work/tmp`; first confirm
`.work/` is ignored through `.git/info/exclude`. State that must survive the
session belongs in this document or the maintained test catalogue. Add no
second campaign tracker. Commit checked, coherent changes using Conventional
Commits. Never push. Do not change CI, formatters, Rust nightly,
package epochs, normal stores, or live installed systems.

Preserve the 12 GiB host reserve. Admit launches with `perf-gate-ready -s`:
ordinary Cargo builds use 2 CPUs, 2 jobs and 8 GiB, at most two on disjoint
outputs; focused executions use 2 CPUs and 4 GiB. Inherited final/release
operations use their existing allocation, at most 12 CPUs and 24 GiB alone.
Existing 32-CPU/40-GiB scheduler operations remain manual-only and exclusive.
Shared reservations must cover both repositories and worktrees, not just a
single container. Fail closed when the required allocation cannot be obtained.

Keep child handles until terminal status and cleanup are consumed. Poll owned
jobs within 60 seconds and lanes within 15 minutes. Test children that finish
before observation, failures, cancellation and cleanup failure. A terminal
notification must advance or fail the dependent stage, never leave it running.

Use the narrowest behavior check during implementation. Repair a prepared
single regression taking over 30 seconds, or an incremental focused group over
2 minutes, before relying on it repeatedly. Exclude the first necessary build,
measure compilation separately, and report expensive checks before launch.
An unchanged workload over twice its P0 CPU or memory baseline needs an
immediate bounded repair. After three refuted hypotheses or 90 minutes without
a discriminator, reassign the diagnosis with its minimal repro in the same
turn. Technical difficulty never waives a deliverable.

First complete selection, results and representative behavior. Then harden
failure boundaries, then optimize the driver. Before optimization, record
performance budgets without gating except for hard resource limits and the
loop defects above. Timing gates require `perf-gate-ready` immediately before
and `perf-gate-ready -1` immediately after. Defer contaminated timings without
claiming a pass; memory and binary-size checks still run.

## Decisions

Owner decisions, including the current ordering:

- Qualify Pyramid on the current source before inference. Repeat affected
  checks when inference changes their inputs, then proceed to Lean.
- The supplied proposal defines Edit, Change, Scenario and Full tiers. Full
  runs require an explicit owner request and cannot be an automatic fallback.
- Preserve Cargo, XSHT and the existing benchmark/VM mechanisms. Defer Buck2.

Prescribed decisions:

- Extend `dev/gates.xsh` as the catalogue/planner owner. Keep entries typed and
  discover individual cases through their existing runners. Do not duplicate
  thousands of test declarations or create a new build system or daemon.
- Keep artifact dependencies separate from verification ownership. A rebuilt
  compiler does not select all consumers. Unknown paths/import relationships
  produce a visible gap and incomplete scope, never a broad fallback.
- Default `cargo dev gate PATH...` to Change. Add `--plan`, `--tier edit|change`,
  `--case ID` and machine-readable plan output. Scenario requires an explicitly
  named case covered by the task. Preserve existing path invocation semantics;
  migrate `--batch` callers to the manual entry point in the same change.
- Provide `cargo dev gate --full` as the manual entry point. Validate the entire
  dependency/alias closure before launching anything. Automatic plans cannot
  construct a manual request, including through prerequisites, aliases,
  imported catalogue entries or budget overruns. A serialized plan cannot
  confer authority. Unknown tiers, cycles and manual dependencies fail planning.
- Implement XSHT exact sets as repeatable `--exact-id ID`, incompatible with
  FILTER and `--exact`. `--list` validates the same frozen set. Reject missing
  or duplicate requested IDs before execution. Keep current single-filter and
  single-exact behavior for existing callers. Add `--events-jsonl PATH`.
- Schema 1 events contain run/case IDs, integer durations with units, lifecycle
  outcomes and reasons. Use explicit passed, failed, skipped, timed_out,
  cancelled and not_executed variants. Reconcile every selected ID exactly once
  with one terminal outcome and one final summary. Missing mandatory results,
  required skips or truncated streams cannot pass. Reject unknown schemas and
  malformed/duplicate/out-of-order records. Version incompatible changes.
- Driver exits are 0 for selected scope passed, 1 for a failed required check,
  2 for invalid plans or incomplete scope, and 130 for user cancellation.
  Preserve raw child statuses and structured causes. A failed check takes
  precedence over an unrelated deferral; cancellation remains explicit.
- Edit execution has a 30-second budget, Change 300 seconds, excluding separately
  reported necessary compilation. Scenario requires an explicit per-case timeout
  declared before launch. Budget expiry cancels owned work, consumes cleanup and
  returns incomplete. Wait at most 60 seconds for reservations or quiet-host
  admission; then report the pending case and continue independent work.
- Keep catalogue, execution and verdict decisions in the existing XSH driver;
  Rust owns native runner mechanics. Reuse Python measurement owners through
  narrow adapters. No duplicated catalogue or independent readiness authority.
- Snapshot relevant staged, unstaged, untracked, generated, configuration and
  cross-repository inputs. Capture Git paths without newline or UTF-8 loss.
  Bind prepared products to that snapshot, build into owned paths, and keep
  performance products immutable. Never relabel old results after an edit.
  Follow symlink/import closure intentionally; reject escapes from owned roots.
- Focused measurement validates its own source/products, fixtures, outputs,
  resource scope and cleanup. It must not require util-linux close or whole
  campaign receipts. Existing full acceptance still requires its full evidence;
  focused reports cannot synthesize or deserialize a live acceptance capability.
- Preserve existing profile and platform rules. Linux uses the pinned image,
  kache and mold; native process checks use verification products, performance
  uses matching release products. Native macOS adapters retain explicit support
  and unavailable measurements remain unavailable, never estimated as zero.
- Keep existing justified budgets and sampling requirements. P0 calibrates new
  per-case thresholds from repeatability and workload needs, before looking at
  candidate results. New ordinary timing cases use at least 20 interleaved pairs,
  at most one additional predeclared confirmation batch. Report paired intervals,
  an absolute floor, raw pairs and rejection reasons. No precise p99 from small N.
  For new paired cases use a two-sided 95% bootstrap interval for the paired
  median ratio, 10,000 resamples and a fixed recorded seed. Keep existing
  justified statistical methods. Set per-case relative budgets and absolute
  floors from baseline repeatability and workload needs before candidate runs;
  never promote observed candidate noise into a looser threshold.
- Keep nearby and accepted long-lived baselines distinct. Promotion is explicit
  and tested; a candidate cannot approve itself or erase accumulated drift.
- Full target dispatch is qualified using real planner/runner code and bounded
  substitute commands. Completing this tooling campaign does not require a
  fresh world/kernel/platform qualification. Any actual Full run needs its own
  explicit request, even at campaign close. This supersedes generic advice to
  run the full repository suite at a phase end.

Delegated decisions:

- Lanes choose private file splits and precise runner adapters within the owners
  below. Prefer an existing module and one declaration over a generic framework.
- P0 pins an upstream Tokei release and a substantial corpus revision, selecting
  available supported-language coverage with fixed ignore/thread/output policy.
  Use local available inputs first; fetch pinned inputs only within the grant.
- Nextest is an experiment only if scheduling is measured as the bottleneck
  after parity. Keep it only with a representative end-to-end gain and intact
  process/global-state behavior; it is not a campaign dependency or deliverable.
- Avoid new dependencies. If necessary, permit development-only dependencies
  within measured P0 build-time/binary-size budgets; no production dependency,
  public language syntax or persisted program format is authorized here.

## Baseline and foundation

The planning source is local XSH `21ce1ed25` and Laputa `bbe05e466`. At P0,
freeze the current source, check intervening changes and rerun the foundation.
Do not transfer these observations into future acceptance.

Two exact native gate tests pass against matching verification products:
`dev/tests/test-gates.xsh::test_changed_paths_select_and_deduplicate_their_verification_owners`
and `dev/tests/test-gates.xsh::test_cargo_callers_select_their_exact_native_tests_and_products`.
Each reports 5 ms test duration; observed whole helper calls were 0.611 and
1.834 seconds. These are single operational observations during other work,
not controlled performance baselines. No compilation was needed.
`xsht test --list --exact` on an absent ID exited 0 while execution exited 2.
Closed: both exit 2, and the integration control
`cli::test_exact_filter_with_no_matching_tests_fails` keeps it so.
The existing Python regression also passes:
`python3 -B -m unittest dev.performance.test_check.PolicyTests.test_measure_without_live_close_report_fails_before_effects`.
It exercises the current live-close prerequisite and verifies rejection before
product qualification or Docker. The test reports less than 1 ms; this is
functional foundation evidence, not a workload performance result.

Source inspection confirms coarse Core/Showcase ownership, broad product
preparation, same-source util-linux close as the representative measurement
prerequisite, and kernel stop controls raising `linux-kbuild-stopped`.
No full suites, benchmark comparison, kernel build or VM run was performed for
this plan. P0 records existing failures without treating them as new regressions.

## Sequence and ownership

Every item delivers code, behavior tests, source docs and caller migration.
Sizes describe coherent lanes: M is bounded component work, L spans its owner.
Each item has the explicit executable proof in Acceptance below.

| Phase/item | Size | Prerequisites | Exclusive ownership and deliverable |
|---|---|---|---|
| P0 ground | M | Current sources; execution grant | Coordinator freezes both repos/products, measures narrow loop, records failures and pins references. Repair broken selectors before broad work. |
| P1 catalogue | L | P0 | `dev/gates.xsh`, `dev/main.xsh`, live canonical/manual dispatcher callers, gate tests and driver docs: typed catalogue, reasons, plan inspection, tier/manual closure, exact products, snapshot binding. |
| P1 native results | L | Early isolated implementation authorized; P0 before acceptance | `crates/xsht/src/commands.rs`, native test owner/tests: exact sets, JSONL, per-module preparation, accounting and cancellation. |
| P1 workload adapters | L | P0 | `dev/performance` and existing benchmark owners/tests: independent correctness-qualified cases and Cargo/performance result translation. Start on agreed event shape before native lane merges. |
| P1 Laputa cases | L | P0 | Isolated Laputa tests and adapters: PM/world/kernel inventory and stop-condition oracles. No normal-store or epoch mutation. Reuse package/VM owners. |
| P2 host resources | L | Catalogue interface; workload shape | Existing leases/readiness owner and adapter hooks: common reservations, telemetry, cgroup accounting, bounded waits and cleanup. Coordinator serializes overlapping workload-adapter files. |
| P2 complete catalogue | L | P1 all lanes; P2 resources | Catalogue lane adds all workload families below, exact ownership/import selection, reports and stale-evidence status. Other lanes own their respective fixtures. |
| P3 harden | L | All functional catalogue families complete | Original lanes fix race, cancellation, path, snapshot, schema and counterexample controls; update topic docs. Coordinator runs touched-area checks. |
| P4 optimize | M | P3 parity complete | Original owners measure and remove driver overhead, unnecessary preparation and repeated work; preserve useful workload evidence and budgets. |
| P5 close | M | P4 | Coordinator runs catalogue contracts and named Change cases, final independent review and fixes P1/P2 findings. Migrate callers/delete dead paths, retire integrated worktrees and report limits. Full dispatch remains manual. |

P1 has four disjoint lanes. P2 permits three to five once shared adapter edits
are serialized. P3/P4 retain all ready disjoint owners up to six; do not create
competing planners to fill slots. The coordinator owns public schema decisions,
shared registrations, manifests, integration and final verification. Prove
foundation again after each source-contract merge, without rebuilding unrelated
products. Keep the full-inference completion claims separate from this campaign.

## Acceptance

P1 first milestone: edit Tokei, one core command or `pm.make`; a plan explains
selected correctness, memory and timing checks, executes only those checks,
and returns a reproducible verdict. No default/fallback/transitive prerequisite
can launch Full. The following tests and cases are mandatory:

| Deliverable | Executable proof and observable conditions |
|---|---|
| Catalogue | Extend `dev/tests/test-gates.xsh`: changed tests, production/import ownership, deduplication, unknown impacts, exact products, compiler-wide representative set, cycles, missing ID, every tier/alias/prerequisite manual-bypass attempt. Prove forbidden commands never start using bounded process fixtures. |
| Native runner | Focused XSHT integration tests plus native fixtures: multi-ID set across modules, missing/duplicate/list validation, preparation once per module, expected/completed reconciliation, failure/skip/timeout/cancel/not-executed outcomes, final event exactly once, broken/truncated stream and structured cause. Existing filters remain compatible. |
| Results/reports | Existing driver adapter tests consume real Cargo/XSHT/Laputa outputs. Golden default report at most eight lines, no misleading total-green claim, selected scope verdict, unresolved reason, regression magnitude/uncertainty, host validity and smallest next command. Status view identifies stale evidence after relevant edits and keeps Full visibly not run. |
| Working tree | Staged/unstaged/untracked/generated/import/cross-repo edits, non-UTF-8/newline paths, symlink escapes, source edits during execution, mutated/replaced products and competing builds. Result remains bound to tested snapshot; new tree cannot inherit pass. No mandatory iteration commit. |
| Independent workloads | Existing `dev/performance/test_check.py` and adapter tests: exact output/state oracle before samples, malformed fixtures/output/cleanup reject, no live util-linux close required for focused case, and unchanged strict full-acceptance boundary. Dirty-source focused runs work without weakening full clean-source gates. |
| Host/memory | Existing lease/readiness tests plus controlled worker fixtures: exclusivity across repos/worktrees, bounded waits, stale-owner cleanup, cancel/failure at every acquired resource, pre/post readiness, per-window pressure, throttling/OOM and worker counts. Fresh per-run hierarchical cgroup peaks distinguish whole workload from leader RSS and allocation/retained bytes. Individual cases never borrow aggregate peaks. |
| Sample validity | Paired/interleaved baseline/candidate, cache/warmup/concurrency/output agreement, minimum sample counts, predeclared thresholds/confirmation, external contamination versus candidate-caused pressure, all three interval outcomes, missing telemetry and unsupported fields. No load-average correction, cherry-picking or silent rerun-to-pass. |
| Inference/core | Register solver/import/verification/frontend-disposal and preparation/evidence work cases; byte-stream, record/order/AWK, traversal, process/pipeline, and JSON/structured-data families. Small/medium/large shapes check useful completed work, memory growth, metadata/syscall counts and cleanup. Compare GNU/BusyBox only for overlapping semantics. |
| Tokei | Small multilingual fixture plus pinned substantial corpus Scenario. Preserve XSH byte-output oracle and deliberate approximate classification. Compare candidate/accepted XSH and pinned upstream separately; assert selected-file sets, report language/count differences, thread/ignore/output/cache policy, time and memory. Historical speed comments supply no evidence. |
| PM/world | Plan exact package/node/action inventory; reuse exact built/reused counts; force one selected fresh package in isolated store with reusable deps; root composition checks ownership/links/manifest/inventory. Executor-provenance changes alone must not masquerade as payload rebuilds. Fresh world is Full. |
| Kernel | Synthetic Kbuild fixtures; fixed graph/scheduler scaling; pinned real discovery/plan Scenario; predetermined one-output closure, no-op and controlled input-change reruns. Pin source/config/toolchain/all worker dimensions/cache state. Require exact `linux-kbuild-stopped` kind plus requested-stage artifacts; arbitrary nonzero exits fail. Full compile/link is Full. Report phases and actual CPU scopes, never elapsed-child subtraction. |
| Manual operations | Full native/Rust sweeps, fresh world, complete kernel and comprehensive VM/platform runs are catalogue-visible manual-only. Planner/dispatch tests exercise each request boundary with bounded substitutes and verify permissions. No real Full run is required or implied by these tests. |
| Close | Named locally relevant Change cases for all changed owners pass; review finds no unresolved P1/P2; no open behavior/parity item. Dead selectors/report writers disappear with migrated callers. Retain supported old result readers only for evidenced consumers; canonical writer is schema 1. Record performance/memory/size and unexecuted manual scope separately. |

Run a single regression with the existing Cargo named-test or XSHT exact-ID
selector. End an item with its named family, a phase with the changed areas'
Change collection. Use `cargo dev gate --plan --case ID` to inspect new cases,
then `cargo dev gate --case ID` for Change or an explicitly applicable Scenario.
`--plan`, `--tier`, `--case` and `--full` exist now; Scenario cases and the
workload families beyond the pilot remain implementation targets.
P0 records the exact existing commands and narrow filters alongside each new
case. A missed required check, budget overrun or mandatory timing deferral leaves
selected scope incomplete. Optional manual coverage does not invalidate an
otherwise complete selected Change scope. No full-project green is inferred.

## Execution boundary

The current owner request resumes Pyramid ahead of inference. Use existing
owned offline Docker operations under the shared allocation limits. The earlier
request authorized reviewed local checkpoints for the isolated XSH native
runner lane; it does not authorize other commits. Full-inference grants continue
to govern inference work. No pushes,
host installation, live roots, secret access, external messages, publication,
normal-store cleanup or production-service control. Register KVM-dependent
manual targets but do not execute them without a separate explicit scope grant.

Missing authority parks only its operation. Missing optional host counters are
reported unavailable; missing required timing/memory accounting leaves that
case incomplete. Missing pinned inputs block their Scenario, not pure planner
work. A mandatory deliverable cannot be declared done by dropping its scenario.
Successful campaign close means all tooling and bounded representative contracts
above pass; it does not certify every distro/platform workload.

Contract changes to surface: gate flags/default tiers, manual `--batch` migration,
exact-ID selectors/list validation, schema-1 event/result formats, exit/status
mapping, shared lease protocol, snapshot applicability and baseline promotion.
Keep source docs/tests and all installed callers consistent in each change.
Migrate live `dev/consolidation/final_canonical_replay.py` and internal Docker
dispatch callers with the manual boundary. Preserve historical evidence and
source-bound receipts as history; never rewrite them to imply new acceptance.

## Non-goals and deferred work

No Buck2 migration, new verification daemon, language syntax, native backend,
command-specific Rust shortcuts, CI changes, package catalogue expansion or
Tokei tokenization rewrite. No online service or arbitrary code execution in
catalogue discovery. Case argv is structured, inputs/outputs are owned paths,
and cleanup never follows untrusted links outside owned roots.

Nextest, remote execution and shared build-cache redesign require measured
residual orchestration bottlenecks after P4. Keep conclusions and a bounded
follow-up proposal if warranted; do not begin a second campaign here.

## Verified code map and reference

- `dev/gates.xsh` owns `Area`, `select`, `required_products`, `lane_plan`, the
  case catalogue, `plan_paths` and the manual `--full` sequence; `dev/main.xsh`
  dispatches `gate` and `internal gate-lane|gate-full`.
- `crates/xsht/src/xsht/test.rs` owns discovery, filtering, module preparation,
  cancellation and timeouts. `crates/xsht/src/commands.rs` defines current CLI.
- `dev/performance/check.py`, its tests, `inference_measurement.py`, and
  `dev/performance/README.md` own current measurement/acceptance distinctions.
- `dev/util-linux/leases.py` owns current common leases; extend that authority.
- `showcase/tokei.xsh` and showcase tests own saved output/classification rules.
- Laputa `../laputa/docs/TESTING.md`, `../laputa/docs/PM.md`,
  `../laputa/docs/MAKE.md`, `../laputa/pm/make.xsh`,
  `../laputa/packages/linux/PKGBUILD-shared.xsh`, `../laputa/seed/verify.xsh`
  and `../laputa/system/qemu.xsh`
  own focused checks, identity, stop points and full/VM operations.

The original proposal below retains workload details and rationale. The
executable decisions and acceptance above govern implementation. Its published
source review and historical figures are background, not current acceptance.
Its suggested full checks at close are superseded by explicit manual-only
scope; its optional Buck2/Nextest investigations remain deferred as stated above.

## Decision log

- 2026-10-21: Separate valid focused workload evidence from complete release
  qualification; reuse measurement mechanics while retaining strict full gates.
- 2026-10-21: Qualify manual dispatch through bounded real-runner fixtures;
  comprehensive real runs require a separate explicit owner request.

## Campaign log

- 2026-10-21: Planned after full inference. Two exact gate foundation controls
  pass; absent exact-ID listing gap reproduced.
- 2026-10-22: Owner requested staggered execution. Start one isolated native
  runner lane without increasing the shared six-lane or two-build limits.
  Parent takes inference patch review while the former reviewer owns this lane.
  Compiler repairs and combined inference verification retain build priority.
  No early Pyramid result advances post-inference acceptance.
- 2026-10-22: Native-runner checkpoint `22f94efc0` passes one final sixteen-control
  replay and independent source review. Exact sets, list validation, schema-1
  events, coverage failures, selected-source diagnostics, cancellation, timeout,
  cleanup and fair admission are covered. Native SIGTERM now drains and exits
  130; missing legacy exact listings exit 2. No Full suite or P0/performance
  qualification ran. Retain the clean lane and matching products for P0.

- 2026-10-24: Owner steered the smallest useful slice onto master. Ported
  `29b0bd974` (exact sets and events), `2fd3b92e5` (catalogue and tiers) and
  `4e98783bc` (manual Full) by cherry-pick; every other branch commit is parked.
  The ported driver needed current-checker repairs and two timing-tolerant
  controls (cancellation startup wait, descendant reaping wait). The lsmem
  Edit case was narrower than the Core directory run it replaced, so a Change
  request adds `core/lsmem/consumers`.

## For the owner afterwards

- Read the short report, explicit contract changes and any manual scope not run.
- Review the parked branch commit by commit before porting the next family
  onto the current source; `--batch` callers outside the repository must move
  to `--full`.
- Qualify Pyramid on current source, then repeat affected checks after inference.

---

# Design reference supplied by the owner

The central requirement should be:

> **For a particular change, select the smallest declared verification scope, explain why each check is selected, execute it under known conditions, and report exactly what the result establishes. Full verification is a separate, exclusively manual operation.**

That is achievable without turning every edit into a release qualification exercise.

I inspected published XSH at [`25592b9a`](https://github.com/laputa-systems/xsh/commit/25592b9a8f6d392143c101c55fc9bc955bef2631) and Laputa at [`bbe05e46`](https://github.com/laputa-systems/laputa/commit/bbe05e466e95f7ba8b26b17dea0bfcd5365510e9). This was a source review; I did not run their tests or benchmarks. Your unpublished inference work may be newer.

## 1. You already have much of the machinery

The current implementation provides a substantial starting point:

| Existing piece | What I would retain | What needs to change |
|---|---|---|
| `cargo dev gate PATH...` | Changed-path ownership and focused execution | Finer component selection; include relevant performance and memory cases |
| XSHT native runner | Discovery, filtering, bounded execution, cancellation | Select multiple exact test IDs together; emit structured results |
| Cargo test workflows | Profile rules, product preparation, host-boundary tests | Build only required products and harnesses; reuse them across selected checks |
| Performance adapters | Paired samples, product/fixture identity, output checks, cleanup | Allow individual workloads to qualify independently of full campaign acceptance |
| Laputa test/build entry points | Existing PM, Kbuild, package, and VM checks | Expose their costs and scopes through the same catalogue |
| Host readiness checks | Existing `perf-gate-ready` integration | Record comparable telemetry throughout measurement, with explicit contamination reasons |

There are concrete inefficiencies in today’s selector: a core source change selects all `core/tests`, a showcase source change selects all `showcase/tests`, and many selections prepare all products. These are reasonable initial mappings, but they are coarser than the workflow you want. [Current gate implementation](https://github.com/laputa-systems/xsh/blob/25592b9a8f6d392143c101c55fc9bc955bef2631/dev/gates.xsh).

The more consequential issue is that representative performance measurement currently requires a same-source, live util-linux close result and associated campaign evidence. That is appropriate for the existing acceptance claim, but far too expensive as the prerequisite for asking whether one operation got slower. [Performance adapter](https://github.com/laputa-systems/xsh/blob/25592b9a8f6d392143c101c55fc9bc955bef2631/dev/performance/README.md).

**I would separate “this workload was measured correctly” from “this entire compiler and distro revision is qualified.”** The latter can consume the former when you choose to run full verification.

## 2. Make tiers an enforced execution policy

I would use four tiers. These time ranges are proposed design budgets to calibrate on your machines, not claims about current runtimes.

| Tier | Purpose | Typical execution budget, excluding necessary compilation | Invocation |
|---|---|---:|---|
| **Edit** | Fast feedback while implementing | Seconds to roughly 30 seconds | Normal agent loop |
| **Change** | Complete the selected component’s verification | Roughly 1–5 minutes | Once a coherent change is ready |
| **Scenario** | Exercise a substantial real workload | Several minutes or more | Explicitly selected for the task |
| **Full** | Comprehensive language, command, distro, and platform qualification | Potentially hours | **Only when you request it** |

The tiers describe scope and cost. They should not form an automatic escalation ladder.

### Edit

Run the exact regression being developed, its immediate controls, and applicable cheap work/memory checks.

Examples:

- A solver repair: its Rust unit tests, a few native inference witnesses, and an existing scaling counter check.
- A `tail` repair: affected native cases and the relevant stream/process boundary.
- A Tokei scanner change: scanner fixtures and a small fixed corpus.
- A PM scheduling change: task-order, cancellation, stamp, and small graph tests.

### Change

Run the selected component’s complete contract: broader relevant correctness cases, representative memory checks, and a bounded performance comparison where applicable.

This is the normal point at which an agent says the change is verified within its declared scope. It should happen after a coherent edit, not mechanically after every commit.

### Scenario

Run a named substantial workload: real kernel planning, one kernel output closure, a selected package fresh build, or a large source-counting corpus.

An agent can run an applicable scenario when the task explicitly calls for that scope. A broad-looking diff should not silently trigger it.

### Full

Full native/Rust sweeps, complete kernel builds, fresh world builds, and comprehensive VM/platform verification belong behind explicit manual entry points.

Enforce this in three places:

- The catalogue marks these targets `manual_only`.
- The automatic planner cannot include them, including through prerequisites or aliases.
- Agent instructions prohibit selecting the manual entry point without your request.

**Unknown impact must produce an explicit gap, never an automatic full run.** A budget overrun must produce an incomplete result, never silently omit required checks and report success.

That policy matches Laputa’s existing instruction to reserve `make verify` for explicit full verification. Its published driver actually begins with `make clean` and proceeds through world construction, publication, images, VM proofs, and tests—so it particularly must not become a fallback. [Laputa testing policy](https://github.com/laputa-systems/laputa/blob/bbe05e466e95f7ba8b26b17dea0bfcd5365510e9/docs/TESTING.md), [verification driver](https://github.com/laputa-systems/laputa/blob/bbe05e466e95f7ba8b26b17dea0bfcd5365510e9/seed/verify.xsh).

## 3. Selection needs two different kinds of dependency

This is the most important architectural distinction, including for the Buck2 question.

### Artifact dependencies determine what must be rebuilt

A change to XSH’s checker may rebuild the XSH executable. Every native test and Laputa program depends on that executable.

That dependency must remain accurate for build correctness.

### Verification ownership determines what should be exercised now

The same checker change might need:

- A particular constraint-solving test family.
- Imported generic-callable witnesses.
- A frontend-disposal execution case.
- A representative preparation benchmark.
- A retained-memory/scaling check.

It does not follow that the edit loop should rebuild Linux.

A build graph alone cannot make that distinction: nearly everything is downstream of the language implementation. **The verification catalogue needs explicit knowledge of language and system contracts.**

### What the catalogue should contain

I would register **components and workload families**, discovering their individual tests from the existing runners. Avoid manually duplicating thousands of test declarations.

Each registered gate needs:

| Field | Purpose |
|---|---|
| Stable ID | Reproduction, historical comparison, reporting |
| Component/contracts | Why this gate exists |
| Source ownership | Paths and relevant dependencies that select it |
| Runner and exact selection | Cargo harness, XSHT IDs, existing benchmark adapter, Laputa operation |
| Required products and fixtures | Prepare only what execution actually needs |
| Tier and manual policy | Prevent accidental widening |
| Resource requirements | CPU slots, memory envelope, privilege/platform requirements, exclusivity |
| Correctness oracle | Expected output, state, diagnostic, or invariants |
| Measurements and budgets | Explicit units and accounting scope |
| Expected cost and timeout | Planning and bounded execution |

The planner should combine:

1. Changed tests themselves.
2. Explicit ownership mappings for changed production code.
3. Relevant imported-module dependencies.
4. Small cross-boundary checks required by the affected contract.

For example:

| Change | Focused correctness | Relevant performance/memory |
|---|---|---|
| Generic-call elaboration | Scheme, importer, verifier, frontend-disposal cases | Preparation work, retained evidence, repeated instantiation |
| Streaming implementation | Pull order, cancellation, error propagation | Throughput and memory growth with input size |
| Directory traversal | Symlinks, metadata/error handling, affected applets | Entries/sec, metadata calls, bounded traversal memory |
| Tokei scanner | Classification fixtures, output contract | Fixed source corpus, allocations, process memory |
| `pm.make` scheduler | Ordering, invalidation, cancellation, stamp publication | Graph scaling, scheduler work, process-tree resources |
| Kernel planning | Relevant Kbuild fixtures and plan checks | Real pinned discovery/plan workload |

Some compiler changes genuinely have broad effects. Give them a **bounded compiler-wide collection of representative checks**. Include a few programs that cross checking, lowering, verification, imports, and execution. Keep larger affected workloads visible as unexecuted scope.

### Coverage can improve selection, but cannot be its sole authority

Observed coverage and import graphs are useful for discovering relationships and ordering tests. They are weaker evidence for excluding tests: a changed parser, dispatch rule, or error path can affect behavior that previous executions never reached.

I would start with explicit component ownership and add measured coverage only after the basic system works.

Also, selection must be inspectable:

```text
Selected inference/imported-callable:
  src/... changed → callable elaboration contract
Selected inference/retained-evidence:
  same contract → mandatory memory control
```

An agent should be able to explain the plan before executing it.

## 4. Keep the existing runners; improve their interfaces

### Add exact-set selection and structured events to XSHT

The current XSHT test interface accepts one filter or one exact selector. Listing prints plain test IDs; `--cov-json` produces coverage rather than a structured test-result stream. An unmatched exact execution fails, but listing returns before that check. [Native runner](https://github.com/laputa-systems/xsh/blob/25592b9a8f6d392143c101c55fc9bc955bef2631/crates/xsht/src/xsht/test.rs), [CLI definition](https://github.com/laputa-systems/xsh/blob/25592b9a8f6d392143c101c55fc9bc955bef2631/crates/xsht/src/commands.rs).

I would add two small capabilities:

- **Run a frozen set of exact test IDs in one invocation.**
- **Emit versioned JSONL events and a final accounting record.**

Validate every requested ID. Missing selectors must fail planning or execution. Group selected cases by module so the runner can reuse preparation within its existing semantics.

The result stream should distinguish passed, failed, skipped, timed out, cancelled, and not executed. Expected and completed counts must reconcile. A mandatory test skipped for a missing capability leaves that scope incomplete.

Test durations belong in scheduling history. They do not automatically become performance measurements.

### Preserve profile discipline

XSH already distinguishes debug units, optimized `verification` process tests, release performance checks, and distribution builds. Keep those distinctions. [Testing and profiles](https://github.com/laputa-systems/xsh/blob/25592b9a8f6d392143c101c55fc9bc955bef2631/docs/TESTING.md).

My policy would be:

- During edits, build the smallest required harness/product set.
- Build a harness once and run its selected cases.
- Use cheap deterministic performance controls during repeated edits.
- Prepare matching release products for the selected performance comparison at the change boundary.
- Report compilation time separately from execution time.

That last point matters. If “12 seconds of tests” requires four minutes rebuilding unnecessary products, the workflow is not fast.

Nextest is worth a small Rust-only trial if execution scheduling becomes a bottleneck. Its filters and concurrency groups may help, but its process-per-test model needs validation against XSH’s process/global-state assumptions. It does not replace XSHT or coordinate all other agents on the host. [cargo-nextest](https://nexte.st/docs/filtersets/reference/?utm_source=chatgpt.com)

### Allow ordinary working-tree development

A focused run should not require committing every iteration.

Record the base revision plus the actual input snapshot, including relevant staged, unstaged, untracked, generated, and cross-repository inputs. Bind the built executable hashes to that snapshot. If inputs change during execution, preserve the result for the snapshot it tested; do not present it as evidence for the newer tree.

For performance runs, use immutable product paths. Another agent rebuilding `target/.../xsh` must not change the executable halfway through a comparison.

## 5. Performance needs both cheap controls and real measurements

I would maintain three complementary forms of evidence.

### Deterministic work and growth checks

These belong in the fast loop where appropriate:

- Solver representative hops and row visits.
- Function-body check/lower counts.
- Allocations or copied bytes for a fixed operation.
- Metadata calls per directory entry.
- Scheduler task/edge visits.
- Peak live objects or retained compiler evidence.
- Memory growth across increasing stream sizes.

These detect whole classes of regressions without requiring a perfectly quiet host.

They still need semantic assertions: doing less work because half the input was skipped is not an optimization.

### Controlled timing and resource comparisons

These establish whether the changed product became slower or more memory-hungry on a named workload.

Each case must define:

- Exact input and output contract.
- Timing boundary.
- Cache state.
- Concurrency.
- Required measurement scope.
- Baseline and candidate product identities.
- Allowed regression and hard resource envelope.

### Diagnostic profiling

Phase clocks, allocation instrumentation, syscall tracing, and detailed scheduler traces explain a result.

Keep them separate from ordinary timing samples. XSH already makes this distinction in its runtime statistics and performance adapter; preserve it. [Existing measurement contract](https://github.com/laputa-systems/xsh/blob/25592b9a8f6d392143c101c55fc9bc955bef2631/dev/performance/README.md).

This gives agents a sensible sequence: detect the issue cheaply, confirm it on the relevant workload, then profile the expensive mechanism.

## 6. Host conditions must be part of the result

I would extend the existing readiness owner rather than introduce another competing readiness script.

### Coordinate your own workload first

Use a shared host-level reservation across worktrees and agents:

- Ordinary tests consume bounded CPU/memory slots.
- Product builds have their own resource allocation.
- Qualified timing gets exclusive use of its declared measurement resources.
- Heavy package/kernel work cannot overlap an incompatible timing reservation.
- Reservations have bounded waits and reliable cleanup.

This can use the existing lock/lease machinery; it does not require a resident scheduling service.

An idle container is insufficient evidence that the physical machine is quiet.

### Record before, during, and after

Capture the information needed to explain comparability:

- Machine, architecture, kernel, and container image.
- Effective CPU set, CPU quota, and relevant topology.
- CPU utilization and runnable pressure.
- Memory availability, swap activity, and memory pressure.
- I/O pressure.
- Cgroup throttling and OOM events.
- Frequency/governor and thermal information where available.
- Every relevant worker/job count.

Linux PSI reports CPU, memory, and I/O stalls at both system and cgroup scope, including cumulative counters useful for measuring the actual execution window. Load average alone cannot describe those conditions. [The Linux Kernel documentation](https://docs.kernel.org/accounting/psi.html?utm_source=chatgpt.com)

For short samples, low-overhead boundary counters and a shared observer are preferable to spawning monitoring commands around every operation.

### Distinguish outside interference from the workload’s own behavior

This deserves explicit policy.

If another agent begins compiling and disrupts the measurement, mark the affected comparison window contaminated.

If the candidate itself creates excessive memory pressure or oversubscribes its worker pool, that is potentially the regression being measured. **Do not discard the candidate’s bad behavior as “host noise.”**

Define validity rules before examining speed results. Preserve raw measurements and rejection reasons. Never discard a sample simply because it is slow.

### A busy host should produce a short pending result

For example:

```text
Correctness: PASS
Memory controls: PASS
Timing: DEFERRED — external CPU contention
Next: rerun core/tail/throughput
```

Bound the wait for quiet conditions. Agents should continue other useful work instead of spending an hour polling or manufacturing a passing benchmark.

Do not “correct” timings by dividing by load average. Affinity, controlled conditions, pairing, and honest uncertainty are more defensible; CPU affinity and topology are also standard considerations in established benchmark tooling. [pyperf 2.10.0 documentation](https://pyperf.readthedocs.io/en/latest/system.html?utm_source=chatgpt.com)

## 7. Define memory and statistical claims precisely

### Memory must have a named scope

For XSH and Laputa, I would record these separately:

| Measurement | Question it answers |
|---|---|
| XSH allocation traffic | How much allocation/copying work did the interpreter perform? |
| XSH retained bytes | How much compiler/runtime state remained live? |
| XSH process peak RSS | What was this process’s resident-memory high-water mark? |
| Whole-workload cgroup peak | What memory envelope did the complete build/test workload require? |
| Sampled memory breakdown | Was growth associated with anonymous memory, file cache, or kernel accounting? |

A parallel kernel build requires whole-workload accounting. Linux’s child `ru_maxrss` reports the largest child, not the maximum simultaneous memory of the process tree. [Linux manual page](https://man7.org/linux/man-pages/man2/getrusage.2.html?utm_source=chatgpt.com)

Use a fresh per-run cgroup, or a supported correctly reset peak, for those measurements. Cgroup v2 provides hierarchical peak accounting; record its definition and cache policy rather than labelling it “RSS.” Direct cgroup use does not depend on systemd. [The Linux Kernel documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html?utm_source=chatgpt.com)

For Docker work, measure the workload’s container/cgroup—not merely the host-side Docker client. A container peak covering twenty different cases cannot become the individual peak for each case.

### Compare baseline and candidate in the same window

For short and medium cases:

1. Prepare both products and fixtures.
2. Validate their expected behavior.
3. Apply the same declared warmup/cache procedure.
4. Run paired, interleaved baseline/candidate samples.
5. Retain the raw pairs and validity observations.
6. Evaluate the declared budget.

Avoid running all baseline samples and then all candidate samples when temporal drift can affect the comparison.

### Use three outcomes for performance

A useful rule is:

- **Within budget:** the uncertainty interval remains within the allowed regression.
- **Regression:** the interval lies beyond the allowed regression.
- **Inconclusive:** the interval crosses the boundary.

Each case should combine a practical relative threshold with an absolute floor. A tiny percentage change in a sub-millisecond operation may be irrelevant or unresolvable.

Keep justified existing budgets. Calibrate new ones from repeatability and product requirements; do not apply one universal percentage to startup, directory scanning, and kernel construction.

A few samples can identify a gross slowdown. They cannot establish a precise p99. Expensive manual builds may initially provide observational results rather than statistically tight claims.

Predeclare sampling and confirmation limits. Avoid repeated reruns until one result passes.

### Prevent gradual regression

Maintain both:

- A nearby comparison baseline for diagnosing the current change.
- A longer-lived accepted reference for detecting accumulated drift.

Baseline promotion must be explicit. A candidate must not redefine its own acceptable memory or latency simply by becoming the newest commit.

## 8. Give Laputa several meaningful workloads

**The most valuable Laputa benchmark may be kernel planning, rather than a complete kernel build.** It exercises substantial XSH and `pm.make` behavior without every measurement paying for all compiler subprocesses.

### World construction

I would define separate cases:

| Case | What is measured | Essential assertion |
|---|---|---|
| World planning | Recipe loading, graph construction, fingerprints | Exact package/node/action inventory |
| World reuse | Store lookup and verified artifact reuse | Expected built/reused counts |
| Selected fresh package build | Actual execution of a representative recipe | Selected payload was genuinely rebuilt |
| Root composition | Materializing a runtime generation | Correct inventory, ownership, links, and manifest |
| Fresh world | Complete construction | **Manual** |

There is an important existing policy here: Laputa artifact keys exclude the XSH runner, PM implementation, and core applets. Executor identity is recorded as provenance. Consequently, a successful cached build after an executor change may rebuild nothing. [PM identity policy](https://github.com/laputa-systems/laputa/blob/bbe05e466e95f7ba8b26b17dea0bfcd5365510e9/docs/PM.md#identity-store-and-snapshots).

Use isolated benchmark stores to force the intended selected rebuild while retaining reusable dependencies. Do not routinely invalidate the user’s normal store or bump the global build epoch.

### Kernel construction

The kernel recipe already exposes stop points for `prepare`, `discover`, `plan`, `compile`, and `link`, plus selection of an output’s task closure. Those are excellent existing boundaries. [Kernel build controls](https://github.com/laputa-systems/laputa/blob/bbe05e466e95f7ba8b26b17dea0bfcd5365510e9/packages/linux/PKGBUILD-shared.xsh).

I would register:

1. Synthetic Kbuild contract fixtures.
2. Fixed-size graph construction and scheduling.
3. Real pinned kernel discovery/planning.
4. One predetermined output closure.
5. No-op rerun.
6. Controlled input-change rerun.
7. Complete kernel compile/link—manual.

The stop controls intentionally return named errors. The adapter must recognize the precise expected stopping condition and validate its artifacts; accepting arbitrary nonzero exits would conceal failures.

For every kernel comparison, pin source, configuration, compiler/linker, cache policy, and all concurrency dimensions. Package-level `JOBS` alone does not capture compilation, discovery, archive-analysis, and planner parallelism.

### Measure PM cost without misleading subtraction

Report:

- End-to-end wall time.
- XSH’s own CPU time and memory.
- Whole-workload CPU and memory.
- Planning/validation phases.
- Tasks executed and reused.
- Relevant scheduler events.

Do not calculate “PM overhead” by subtracting summed child elapsed times from total elapsed time. Parallel children overlap. Phase measurements and actual CPU accounting are more informative.

Similarly, “cold” needs a precise definition: cold plan cache, fresh objects, empty artifact store, and cold filesystem cache are different experiments.

## 9. Coreutils and Tokei need durable, representative contracts

### Coreutils

Organize performance cases by shared mechanism, while retaining applet-specific behavior tests:

| Family | Representative work | Useful bounds |
|---|---|---|
| Byte streaming | `cat`, `head`, `tail`, `wc`, `tr` | Throughput, startup, memory versus input size |
| Records and ordering | `cut`, `sort`, `uniq`, AWK | Scaling, comparisons, allocation, temporary storage |
| Filesystem traversal | `find`, `du`, `cp`, `rm` | Metadata calls, entries/sec, traversal memory |
| Process/pipeline behavior | Spawn, pipe, cancel, reap | Process count, descriptors, buffering, cleanup |
| Structured data | JSON and record/map processing | Completed records, preparation, allocation, retained memory |

Use small/medium/large shapes with distinct purposes. A change should select the smallest shape capable of exposing its relevant failure class.

For streaming commands, a memory-growth assertion across input sizes is often more valuable than one generous RSS ceiling. For traversal, syscall/work counts can explain a regression much more directly than a noisy elapsed time.

Compare against GNU, BusyBox, or another reference only where the declared semantics overlap. Keep compatibility differences explicit.

### Tokei

I would definitely restore a maintained upstream comparison.

However, the current script explicitly documents approximate line classification and uses its own saved output as its byte-level oracle. Its historical speed and count-gap comments are not current measurements. [Tokei implementation and contract](https://github.com/laputa-systems/xsh/blob/25592b9a8f6d392143c101c55fc9bc955bef2631/showcase/tokei.xsh).

Maintain two comparisons:

- **XSH candidate versus accepted XSH baseline:** regression detection.
- **XSH versus pinned upstream Tokei:** external performance and compatibility comparison.

Report speed, memory, selected-file differences, and classification differences separately.

Pin corpus revision, supported languages, ignore rules, thread policy, output mode, and filesystem-cache policy. Use a small multilingual fixture in ordinary development and a pinned substantial repository for explicitly selected measurements.

This prevents a misleading “faster than Tokei” result caused by silently scanning fewer files or changing classification work.

## 10. Reporting should be a deterministic product

I would make terse reporting an acceptance requirement of the campaign.

The default output should fit roughly eight lines. Detailed events, logs, samples, and provenance remain available behind a run ID.

**Illustrative output only:**

```text
REGRESSION · laputa/pm.make · 6 gates · 48s
Tests       37 passed; 0 failed; 0 skipped
Plan time   182 → 211 ms  +15.9%  [12%, 19%]; budget +5%
Memory      92 → 94 MiB; within budget
Work        12,000 tasks; identical plan and outputs
Host        valid; fixed 4 CPUs; no external contention
Manual      kernel/world/VM not run
Reproduce   cargo dev gate --case laputa/pm.make/plan
```

The command spelling above is proposed, not currently implemented.

A successful scoped run should say **“selected scope passed.”** An inconclusive required timing should keep that scope incomplete. Manual coverage should be shown as not run, without turning every ordinary edit into a demand to run it.

The report should answer:

1. Can this task’s verification be considered complete?
2. What failed or remains unresolved?
3. How large is the regression?
4. Was the comparison valid?
5. What is the smallest next command?

I would also provide a separate status view for the broader project: latest evidence per family, source revision, platform, and whether subsequent relevant changes have invalidated its applicability. A two-week-old full run remains useful history; it should not appear as current global green.

Generate verdicts and summaries directly from structured results. Agents can explain them, but should not reinterpret an inconclusive measurement into a pass.

## 11. Buck2: technically suitable, premature as the next step

XSH being a greenfield language is **not an obstacle**. Buck2 supports custom language rules and toolchains; an XSH rule could expose its module/fixture closure and invoke a pinned XSH/XSHT toolchain. [Buck2](https://buck2.build/docs/rule_authors/writing_toolchains/?utm_source=chatgpt.com)

But Buck2 does not supply the policy you most need. Its documentation separates the build engine from the test runner; the open-source runner is basic, while Meta’s TPX handles substantial discovery, orchestration, and reporting responsibilities. [Buck2](https://buck2.build/docs/rule_authors/test_execution/?utm_source=chatgpt.com)

Its reverse-dependency queries would help identify downstream targets. My inference from that graph model is that a shared compiler change would reach most XSH programs, leaving your component-specific verification policy necessary. [Buck2](https://buck2.build/docs/api/bxl/CqueryContext/?utm_source=chatgpt.com)

Native Rust integration also has a real migration cost. Reindeer translates Cargo dependency manifests into committed Buck targets, invokes `rustc` through Buck, and requires package-specific fixups where manifest information is insufficient. Wrapping Cargo in a single Buck action is simpler but retains much of Cargo’s existing granularity. [GitHub](https://github.com/facebookincubator/reindeer/blob/main/docs/MANUAL.md?utm_source=chatgpt.com)

I would reconsider Buck2 when your new reports demonstrate one of these:

- Repeated compilation dominates edit-to-verification time despite narrow product selection.
- Cross-worktree or cross-repository artifact reuse is a major source of waste.
- Maintaining the build dependency graph has become difficult.
- Remote execution or shared artifact caching would materially help.

At that point, pilot one representative build/test path and compare total latency, resource use, and maintenance burden. The verification catalogue and reporting contract should survive the change of executor.

## Reference implementation order

**First: make selection visible and enforce manual boundaries.** Extend `cargo dev gate` with plan output, component ownership, required products, explicit costs, and manual-only target validation. Include tests proving that ordinary paths and aggregate wrappers cannot accidentally launch full verification.

**Second: unify result collection.** Add exact-set execution and structured events to XSHT. Adapt existing Rust and Laputa outputs into the same result model. Produce the short report immediately.

**Third: make individual performance cases independently runnable.** Reuse the existing parity, sampling, readiness, and cleanup machinery. Preserve its provenance while removing unrelated full-acceptance prerequisites from focused measurements.

**Fourth: add host coordination and correct memory scopes.** Establish shared resource reservations, per-run telemetry, contamination rules, and whole-workload accounting.

**Fifth: expand the catalogue based on real changes.** Start with one compiler/inference change, one coreutils change, Tokei, and `pm.make`; then cover the remaining mechanisms.

The first milestone should be very concrete:

> **An agent edits Tokei, a core command, or `pm.make`; the driver explains the selected correctness/performance/memory checks, runs only those checks, and returns a concise, reproducible verdict. No default, fallback, or transitive prerequisite can start the full suite.**

Once that works well, extending coverage becomes ordinary maintenance. That is the foundation I would establish before considering a build-system migration.
