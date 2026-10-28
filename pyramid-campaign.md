# Scoped verification across XSH and Laputa

**Status:** active on `master`, resumed 2026-10-28. Finish-line items 1 to 3 are
complete. Item 4 (P0 baselines) is open and needs a quiet measurement window.
Item 5 (Close) is in progress: the independent reviews of 2026-10-28 found no P1,
and every P2 and the P3s they listed are fixed on `master`.

Review findings and their fixes:
- partial lane exclusion was silent: `8b596e5ae`, `26e941fb7`;
- `--status` could not resolve product paths recorded in the container:
  `3d820dcec`;
- Laputa evidence ignored the Laputa checkout: `538c9a83f`;
- the privileged lane was over-broad: `56a8138f9` splits it into
  `lane/root-native` (default confinement, no network, `27b1a0edc`) and
  `lane/namespace-native` (only a vendored seccomp profile changes);
- cancellation did not remove the gate's containers or let adapters clean up:
  `56ccc45b7`, `5a822f3ca`, `ddec7c02a`, `429e54223`, `2c6c0c80a`, `3db8235fb`;
  `02cb08ece` runs the built interpreter as the container command, because
  `cargo run` forwarded no signal. A cancelled core trace case that holds the
  native lease exits 130 in 1.0 s, runs the adapter's own cleanup and leaves no
  cleanup-unresolved marker;
- ownership repair did not run on cancellation: `247f182af`;
- smaller findings: `6b7f553a8`, `aa3bfce9f`, `25f1c4cb5` (container removal and
  registry), `c938434ac` (evidence binds target and profile), `97f4a411d`,
  `a3511e707`, `9d44ac759` (dead code), `ccd4046f8` (image ID from the build),
  `2d1a6ed75`, `4ea819051`, `769318909`, `f3e5d92ce`, `db256243a`, `530b79dee`,
  `aaf16ba69`, `5edf1236a`.

Driver optimization beyond the finish line is out of scope. The G0 candidate is
qualified separately from its own pinned checkout; see
[G0](docs/ROADMAP.md#g0-release-candidate).

**Next:** record the P0 timing, memory and size baselines in a quiet
measurement window, then close item 5. The owner waived the quiet-host
requirement on 2026-10-28 so correctness work can proceed on a busy host.
Correctness cases admit on resources alone. Timings taken under the waiver are
diagnostic, not baselines, and item 4 stays open for a quiet window.

**Resume:** read `AGENTS.md`, `TODO.md`, and this document. Run the open cases
with `cargo dev gate --plan --case ID`, then `cargo dev gate --case ID`, one
case per run. After inference changes a Pyramid input or contract, rerun the
affected checks.

## Goal

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

## Finish line

Pyramid closes when all of these hold:

1. Every catalogue Change case completes on clean `master` in the pinned
   Linux image, with no incomplete or timeout caused by the harness itself.
   **Complete.**
2. Every parked branch and worktree has been integrated, ported by path, or
   deleted, with the decision recorded here. **Complete.**
3. Each workload family in Acceptance has a correctness-qualified case with an
   exact oracle, run through the gate. **Complete.**
4. P0 references are pinned and P0 timing, memory and size baselines are
   recorded in a quiet measurement window. Timing gates may stay deferred
   while the host is busy, but that leaves this item open. **Open.**
5. The Close row of Acceptance passes, including an independent review.
   **In progress.**

## Results that later work depends on

- **Item 1.** `native-core/tests` is no longer a Change case. Native cases run
  as the host user. Tests the unprivileged lane cannot run are declared in
  `dev/gate_lane.xsh` and run by the explicit `lane/root-native` and
  `lane/namespace-native` scenarios. Corpus-wide checks (core suite, desugar
  corpus, lint-format corpus, `test-awk.xsh`, `test-gnu-sort.xsh`) are Scenario
  cases. On `5046fd6b9`, `native-dev/tests` passed 487 of 487 in the pinned
  image, and `core/cat/external-consumers` and `verification/performance-check`
  passed three of three. The roughly 990 per-file native cases run inside their
  suites, not one case per run.
  Clean verdict table on `aaf16ba69`, one case per run: all 51 listed Change
  cases pass, as do `lane/root-native`, `lane/namespace-native`,
  `verification/erase-sites` and `core/lsmem/heavy-inventory`. Three runs carry a
  reduced verdict because tests the unprivileged lane cannot run are declared in
  `dev/gate_lane.xsh`: `native-tests/xsh/stdlib` (29 tests outside the lane),
  `native-tests/xsh/tooling-` (1) and `native-dev/tests` (2). A suite that runs
  reduced exits 0 and says so in its last verdict line; a single file partly
  outside the lane is incomplete. `native-dev/tests` is the one load-sensitive
  case: at host load 40 to 47 a heavy system-report test can hit its 2 minute
  per-test limit, and the rerun passed.
- **Item 3.** All 49 workload-family correctness cases passed through
  `cargo dev gate --case`, one case per run, on `ba8102de0`, `e18c2e5e1` or
  `22451afb0`. Those revisions differ only in gate, container and test-harness
  repairs. The Laputa cases passed against Laputa `9485988`.
- **Combined selections.** A combined Change selection shares one 300 s
  budget, so it cannot finish under heavy load. Per-case runs each get the full
  budget.
- **Stale-evidence view.** It reads the latest focused report per case from the
  checkout's `.work/gate/`, bound to the source and product identities the
  report ran on, and reports stale or not run when they no longer match. The
  reports are disposable local evidence and are not committed.
- **Paired timing comparison** from the integration line waits for the quiet
  window in item 4.
- **Item 2 branch decisions.** Deleted branches are restorable from their tips
  until Git prunes them. `campaign/pyramid-integration` (`c7b9b7b29`) was built
  on an older provisional inference compiler. Only its Full-catalogue validation
  before Docker starts (`dc84bfc57`) and owned Docker cleanup (`f0227139b`)
  were ported. Its other modules were superseded by `dev/gates.xsh`,
  `dev/gate_source.py`, `dev/gate_product.xsh` and master's adapters.

  | Disposition | Branches (tip) |
  |---|---|
  | Deleted as superseded | `campaign/pyramid-native-results` (`22f94efc0`), `scratch/pyramid-pilot-baseline` (`e61982c5d`), `scratch/pyramid-pilot-lsmem` (`4f745f4a3`), `campaign/pyramid-caller-migration` (`9614fc7c5`), `campaign/pyramid-core-cases` (`ce2a3b836`), `campaign/pyramid-integration` (`c7b9b7b29`), `park/pyramid-tokei-scout` (`3164f644b`), `park/pyramid-resources` (`c5a161317`), `park/pyramid-core-cases` (`1be0bb781`), `park/pyramid-kbuild-case` (`06e485aca`), `park/pyramid-laputa-case` (`d66eed09d`), `park/pyramid-native-set` (`fcce250d7`), `campaign/pyramid-catalogue` (`37bf94b65`), `campaign/pyramid-resources` (`74aba1609`), `campaign/pyramid-workloads` (`bf30b227b`) |
  | Merged, then deleted | `campaign/pyramid-port`, `campaign/pyramid-tokei-scout` |
  | Landed on `master` | native set (`ed8193131`, `9b0d52484`), JSON slice from `campaign/pyramid-json-structured` (`76d36cc4f`) as `d51dfcc39`, stale-evidence view (`86bfe6a0c`, `56be6dce4`) |
  | Kept | `.work/wt/pyramid-tokei-upstream`, the pinned upstream Tokei reference |

## Pinned inputs

- **Linux test image:** `sha256:dcb4231618e78bfbdd242360ce51c67fb64991eff9db3d80db4c1b476d8bc637`,
  authorized by the owner as the replacement for earlier approved images. Use it
  by immutable ID. A gate run in a delegated worktree can rebuild the shared
  `xsh-test:latest` tag and orphan an image that other cases pin.
- **Tokei reference:** [upstream v14.0.0](https://github.com/XAMPPRocky/tokei/releases/tag/v14.0.0)
  at `8cdd6fa3a54f8cd69442d2f00effb29aa3110353`. Reference binary SHA-256
  `53c96c158d369d7bf8ed775c801191a6e8947cff7f47d7155e9c39afc26ca3ba`, built
  offline from a read-only archive. A cache-free build with a fresh target
  directory produced `3dae78ab69cc013325c7509320d510530b6ec332565b6b97772f390e160036bf`.
  The cause of the difference is not established, so reference source
  provenance stays unqualified.
- **uutils/coreutils:** `e7c9f3194280835c4487c2945c68d5f01ccacc8d`, archived
  without the live checkout's untracked file. It has no tracked submodules or
  `.tokeignore` files. The Tokei uutils scenario passes: both sides select the
  same 671 tracked Rust files. XSH counts 213,988 code, 18,298 comment and
  30,443 blank lines. Upstream counts 214,731, 18,027 and 29,973. The
  differences are recorded, not explained.

## Known limits

- Source observation is a series of separate reads, not an atomic snapshot.
  Ignored and external inputs are not bound, except resolved symlink targets.
  Harness preparation and atomic binding are open.
- Pressure diagnostics are incomplete in the Linux scenarios because
  `cgroup.pressure` is unavailable. Scenario performance is unmeasured.
- The syscall-trace reader accepts at most 128 MiB per trace and 1 MiB per
  record. Larger traces and core syscall scaling are unqualified. Syscall counts
  include tracing machinery. Detached and live-cancellation traces, `wait_any`
  and `wait_ready` remain unreconciled.
- The Kbuild planner reports 3,114 tasks, no missing sources and no duplicate
  outputs for the pinned Linux 7.2.9 x86 source. No independent oracle confirms
  the inventory, and the real-kernel scenario is unqualified.
- The PM diagnostics (seven-node Linux root plan, m4 and xsh builds, generation
  composition) used the historical `429f6ff` XSH seed. They do not qualify the
  current seed, the full world inventory, artifact reuse, or a bootable root.
- The wider frontend scenarios are partial. Alias chain and higher-order
  composition at 64 units exceeded the indexed type-materialization work bound
  in a host probe, and reduced recursive-effect and module-diamond inputs failed
  checking. Disposal execution, scaling and performance remain unqualified.

## Charter

Follow `orchestrate` and its campaign reference for workstreams, integration,
and worktree lifecycle. Use `.work/wt/pyramid-<lane>` and each checkout's
`.work/tmp`. State that must survive the session belongs in this document or the
maintained test catalogue. Add no second campaign tracker. Never push. Do not
change CI, formatters, Rust nightly, package epochs, normal stores, or live
installed systems.

Preserve the 12 GiB host reserve. Admit launches with `perf-gate-ready -s`:
ordinary Cargo builds use 2 CPUs, 2 jobs and 8 GiB, at most two on disjoint
outputs; focused executions use 2 CPUs and 4 GiB. Inherited final/release
operations use their existing allocation, at most 12 CPUs and 24 GiB alone.
Existing 32-CPU/40-GiB scheduler operations remain manual-only and exclusive.
Shared reservations cover both repositories and worktrees. Fail closed when the
required allocation cannot be obtained.

Repair a prepared single regression taking over 30 seconds, or an incremental
focused group over 2 minutes, before relying on it repeatedly. An unchanged
workload over twice its P0 CPU or memory baseline needs an immediate bounded
repair. After three refuted hypotheses or 90 minutes without a discriminator,
reassign the diagnosis with its minimal repro. Technical difficulty never waives
a deliverable. Before optimization, record performance budgets without gating
except for hard resource limits and the loop defects above.

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

Measurement rules:

- Name every memory scope: allocation traffic, retained bytes, process peak RSS,
  and whole-workload cgroup peak. Child `ru_maxrss` reports the largest child,
  not the process tree. Docker work measures the workload's container cgroup.
- Define sample validity before looking at speed. Mark a window contaminated
  when outside work interferes. Candidate-caused memory pressure or worker
  oversubscription is the regression being measured, not host noise. Never drop
  a sample for being slow, and never correct timings by load average.
- Report within budget, regression, or inconclusive, by where the interval
  falls against the allowed regression. A busy host yields a short pending
  result with timing deferred and continues other work.
- "Cold" names its exact meaning: plan cache, fresh objects, artifact store, or
  filesystem cache.
- Do not compute PM overhead by subtracting summed child elapsed times from
  total elapsed time. Report phases and actual CPU accounting.

## Sequence

P0 grounds the work: freeze both repositories and products, measure the narrow
loop, record failures, and pin references. P1 to P4 deliver the catalogue,
native results, workload adapters, Laputa cases, host resources, hardening, and
driver optimization. P5 closes: the coordinator runs the catalogue contracts and
named Change cases, an independent review finds no unresolved P1 or P2, dead
selectors and report writers disappear with migrated callers, and integrated
worktrees retire. Full dispatch remains manual. Keep all scheme, catalogue and
public schema decisions with one owner; do not create competing planners.

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

Reconsider Buck2 only if the new reports show that repeated compilation
dominates edit-to-verification time despite narrow product selection, that
cross-worktree artifact reuse is a major source of waste, that the build graph
is hard to maintain, or that remote execution or a shared cache would
materially help. Then pilot one representative path and compare total latency,
resource use, and maintenance burden. The catalogue and reporting contract
should survive a change of executor.


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

## Log

- 2026-10-21: Separate valid focused workload evidence from complete release qualification. Reuse measurement mechanics while retaining strict full gates.
- 2026-10-21: Qualify manual dispatch through bounded real-runner fixtures. A comprehensive real run needs a separate explicit owner request.
- 2026-10-24: The owner steered the smallest useful slice onto `master`. Exact sets and events (`29b0bd974`), the catalogue and tiers (`2fd3b92e5`) and manual Full (`4e98783bc`) were cherry-picked. The lsmem Edit case was narrower than the Core directory run it replaced, so a Change request adds `core/lsmem/consumers`.
- 2026-10-28: The owner waived the quiet-host requirement (see Status).

## For the owner afterwards

- Read the short report, explicit contract changes and any manual scope not run.
- `--batch` callers outside the repository must move to `--full`.
- Qualify Pyramid on current source, then repeat affected checks after inference.
