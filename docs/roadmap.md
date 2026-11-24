# Roadmap

This is the single plan for this repository: all current and future work, and the handoff for resuming it. Campaigns are sections below, and their items interleave in **Sequence**. "Resume the roadmap" means continue from **Status**.

## Status

- **Active:** performance and profiling; M2 baseline B1 is next when the host is quiet.
- Last integrated revision: `5784eaec0` (XSH-191 frame changes); integration green at this revision is not recorded. The last explicitly recorded green slice revision is `ae7534c1a`.
- L3 merged at `c36fa4c61`; its Lean build, inventory, leanchecker, axiom audit and profiler gate passed as recorded in the commit.
- O2 counterfactual landed at `d82dc6eb9`; its performance conclusion is not recorded here.
- MG1 is parked at `.work/wt/mg2`, 19 of 52 rules: it competes with measurement gates. Resume after M2; run `cargo dev test linux` before pushing.
- B1 is parked at `.work/wt/b1-baselines` for host readiness; retry after `perf-gate-ready` succeeds. Laputa registry rows are B1b.
- The broad semantic-soundness campaign is paused since 2026-11-23; focused proofs continue when an optimization requires them.
- `xsh-lean.md` stays unchanged because the formal inventory checker and its tests read it.

## Charter

- Decide and keep going. Record a decision here only when a later slice
  depends on it. Park an item that needs authority outside the grant.
- Observation must not change results or execution routes. A profile of a
  parallel stage runs that stage in parallel.
- Extend what exists: `dev/performance/`, `xsht runtime-stats`,
  `xsht frontend-stats`, `xsht trace` and the native-test coverage collector.
  No second benchmark or trace framework.
- The supported profiling path needs no `perf`, `strace`, Massif or external
  flamegraph tool. External tools may cross-check a calibration fixture once;
  no acceptance command depends on them.
- Calibrate every instrument (coverage, profiler, oracle) on fixtures with
  known answers before trusting what it reports on the corpus.
- Parity before optimization. Each optimization lands with five things: what
  costs too much, why the change removes that cost, which behavior must be
  preserved, what verifies it, and interleaved before/after measurements.
  Keep it only if the win survives integration.
- No blanket speed target before attribution. End-to-end timings are reported
  next to internal attribution, so a faster evaluator cannot hide slower
  startup, more retained memory or costlier cleanup.
- Parallel lanes only for the disjoint sets named in **Sequence**; serial
  otherwise. Worktrees at `.work/wt/<lane>`, following `orchestrate`'s
  worktree lifecycle.
- Conventional Commits, scope `perf`, `profile`, `coverage` or `formal`, with
  the slice ID in the body. Code comments never cite this doc.
- Push each accepted slice to `origin/master` once its gate passes on the
  integrated tree (owner, 2026-11-23). Lane branches are never pushed.
- Fix egregious problems met on the way in their own commit: wrong results,
  route-dependent behavior, or measurement that misreports by more than 10%.
  File anything else in `TODO.md`.

## Sequence

| ID | Campaign and item | Owned files | Prerequisites | Acceptance command and condition |
|---|---|---|---|---|
| B1 | Performance: complete portfolio baselines and Laputa benchmarks | `dev/performance/`, Laputa `bench/` | M1 met; quiet host | `perf-gate-ready` then the [performance baseline commands](TESTING.md#performance-baseline), and `perf-gate-ready -1` immediately after each measurement; report each workload's median, spread and oracle, or inconclusive. Run Laputa registry rows under its testing instructions. |
| O2 | Performance: evaluate the Stats shape counterfactual | `dev/performance/stats_shape*` | O1; quiet host | `perf-gate-ready`, `python3 -B -m dev.performance.stats_shape`, then `perf-gate-ready -1`; establish whether a field-name cliff exists or retain a general replacement within 5%. |
| F6 | Performance: complete memory views, cross-thread frees and high-water marks | `src/mem_track.rs`, profiler | F4 | `cargo dev gate src/mem_track.rs` and the affected profiler paths; cross-thread-free fixture reconciles and worker peaks are not labelled RSS. |
| O4 / XSH-191 | Performance: investigate remaining helper call and frame cost | `src/runtime/eval/`, `dev/performance/` | B1; attribution and quiet host | `cargo dev gate` on changed paths and interleaved abstraction measurements; preserve call, pruning and traceback behavior. |
| IFC-1 | Functional cores: extend structural membership correspondence | `src/runtime/eval/type_view.rs`, production adapters, `formal/Xsh/Core/Membership*.lean` | finite domain and retained contract assumptions in `formal/README.md` | `cargo dev gate src/runtime/eval/type_view.rs` and `formal/scripts/check-inventory.py`; compare production acceptance and rejection with Lean. |
| IFC-2 | Functional cores: extend cleanup boundary only where it removes runner complexity | `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `formal/` | accepted pilot boundaries and deadline shielding | `cargo dev gate src/runtime/eval/lowered_run/indexed_run/explicit_run.rs` and `formal/scripts/check-inventory.py`; preserve cleanup ordering and failure precedence. |
| L2 | Performance / semantic soundness: bring forward `MODE-EQUIV`, `STREAM-CURSOR` or `CLEAN-ORDER` | `formal/` | dependent optimization slice | `formal/scripts/check-inventory.py` and `leanchecker --fresh`; close the relevant obligation in `formal/coverage.json`. |
| MG1 | Performance: migrate excluded lint rules | corpus `.xsh`, `xsht-config.ini` | after M2; parked lane | `xsht lint --only RULE` and `cargo dev test linux`; rule reports nothing and native suite passes. |

## Campaign: performance and profiling

### Goal and state

Make `xsht` profile real XSH and Laputa workloads faithfully without external profilers, then use that evidence to optimize natural, well-factored XSH. M1 is met. M2 needs complete B1 baselines and the O1 abstraction ranking. The broad Lean sweep stays paused; targeted proofs serve this campaign. See the [ticket backlog](../TODO.md) and [workload matrix](../dev/performance/MATRIX.md).

### Decisions

- **D1 (owner, 2026-11-23).** A `par-map` callback that writes a captured
  binding is rejected at check time (XSH-185). Writes inside a parallel
  callback then cannot depend on the route, and S0's reproducer is the first
  profiler fidelity fixture.
- **D2 (owner, 2026-11-23).** Bounded streaming parallelism (lazy `par-map`
  with a bounded window) is out of scope before M2. Today's eager
  `par-map` is specified behavior. Changing it alters upstream effects,
  failure selection, cleanup and early termination, so it needs a SPEC
  change, a migration and the relevant Lean obligations, not a transparent
  optimization.
- **D3 (owner, 2026-11-23).** The broad Lean universal-obligation sweep stays
  paused; Lean runs as this campaign's focused lane (L1 to L3). Its most
  productive output has been differential probes of the binary against the
  model, which found XSH-184 and XSH-185. Proofs come forward when an
  optimization needs them.
- **D4 (owner, 2026-11-23).** Performance and profiling succeeds the earlier performance and deployment campaign; focused Lean work supports it.
- **Prescribed.** One identity scheme: stable source-operation and branch IDs
  tied to source identity and mapped through lowering. Coverage and profiling
  both use it.
- **Prescribed.** `xsht` owns a versioned profile format: JSON with a
  completeness record (mode, overhead calibration, dropped data, opaque
  regions), plus folded-stack text for flame views. Rendering is deferred.
  Hooks do no allocation, string formatting or shared-lock work.
- **Prescribed.** Three collection modes. **Counters**: calls, items, copies,
  allocations and coarse phases. **Attribution**: function, stage and host
  boundary timing, plus task state transitions. **Events**: bounded traces.
  Exact branch coverage is a separate mode, never on during performance
  measurement.
- **Delegated, bounded.** Specializations such as `LoweredValue::Stats` and
  `StatsBlob` stay unless a counterfactual benchmark shows they are
  field-name cliffs. Replace one only with a general mechanism that keeps the
  win within 5%.

### Current baseline and remaining work

M1 profiled a warm Laputa source fetch with `python3 -B -m dev.performance.m1 --laputa-root ../laputa`, with identical summaries and progress, a complete profile and eight overlapping workers. The recorded five-pair medians were 2.80 s plain and 3.15 s profiled. B1a accounted for all 18 matrix rows, but only tooling-check had a timed baseline; the other rows were correctness-only or inconclusive. See [performance baseline](TESTING.md#performance-baseline) for qualification and reproduction.

O3 added dynamic claiming to plain `par-map`, preserving eager buffering; fused `par-map |> reduce-by` retains static partitioning. Its captured-state copying and nested oversubscription experiments were inconclusive. XSH-192 tracks repeated stage overhead. XSH-188's parsed and checked import reuse landed; imported body caching failed its gate, and further lowering gains require reworking the producer proof. XSH-191 remains open for call and frame costs after the integrated frame changes. L2 closed PAR-BOUND; nine of 77 universal obligations were then closed. L3's event-model theorems and profiler correction were integrated later.

The following is the historical pre-campaign baseline at `43e41840f`, not a claim about current behavior:

- `xsht trace` sends ordinary `par-map` down its sequential route
  (`serial_pipeline.rs:1307`, `!evaluator.trace_enabled && jobs > 1 &&
  item_count > 1`). Traced parallel stages are not parallel.
- XSH-185: tracing changes results, not just timing. A `par-map` callback's
  `b += 1` gives `b = 0` untraced and `3` traced.
- XSH-184: a producer suspended inside an `env` scope swaps its whole
  environment (`producer.rs:301-325`).
- Trace events go to an unbounded `Vec<TraceEvent>` (`src/runtime/eval.rs:135`)
  and are rendered after the run.
- Coverage collector (`crates/xsht/src/cli/coverage.rs`): line marks come from
  trace spans; there is no branch-edge coverage; the API `tests` field counts
  events, not distinct tests (`:129`); only `.xsh` paths count (`:308`), so
  Laputa's suffixless scripts are missed; no test-to-source matrix is kept.
- SPEC 13.6 (`docs/SPEC.md:6209`) advises built-in methods over small helpers
  in stage blocks for per-item cost: clean factoring costs speed today.
- `xsht-config.ini` excludes 52 default-on migration lints across the whole
  corpus until it is converted.
- Existing measurement: `dev/performance/` (runtime, inference and core case
  catalogues, interleaved comparison, an explicit inconclusive outcome) and
  Laputa's `bench/registry.xsh`. The 2026-11-23 external audit counted 21
  runtime, 52 inference and 21 core cases, plus 16 Laputa workloads. These
  counts have not been rechecked here, and historical results do not qualify
  the current HEAD.
- Corpus, per the same audit: 2,352 `.xsh` files in XSH (578 of `core/`'s 964
  under `core/tests/`), and 743 `.xsh` plus 12 suffixless shebang scripts in
  Laputa. Not rechecked here; C1 recounts them.

### Acceptance

- **M1, first milestone.** `xsht profile` profiles a real parallel Laputa
  workload (package-source fetching or Kbuild fingerprinting). The workload
  runs on its normal parallel route with unchanged output and effects. The
  profile identifies preparation cost and the critical path, explains
  allocation and buffering, and lists what it could not observe. It fails if
  the run serializes (the F1 rendezvous), if the results differ between
  traced and untraced runs, or if the profile claims completeness after
  dropping data.
- **M2.** B1's baselines exist for every C4 family, and O1's ranked
  abstraction penalties are published in `docs/TESTING.md#performance-baseline`.
- **Close.** Every slice above passes its acceptance, or is parked with its
  cause and named in **Status**. After M2, R1 proposals such as bounded streaming parallelism or deprecations require an RFC with corpus frequency, replacement, migration evidence and machinery removed; none lands without owner approval.

### Needs environment

- Laputa boot, publish and service-startup workloads need the Laputa VM
  harness. The owner runs the registry workloads per Laputa's
  `docs/TESTING.md`.
- SEC1 and SEC2 on aarch64: proven only where the host can cross-build and
  run the proof; otherwise named as unverified in the slice.
- macOS profiler clock and resource adapters are qualified on the m1-max host
  only. Physical suspend qualification for XSH-35 needs real hardware.

### Execution boundary and non-goals

- Authorized: edits and local commits in this repository, Laputa's
  benchmark registry and, for SEC1 and SEC2, Laputa's musl and zlib recipes, plus local containers through `dev/performance`'s owned
  containers.
- Authorized: pushing accepted slices to `origin/master`.
- Authorized (owner, 2026-11-24): passwordless `doas` for host maintenance the
  campaign needs, such as resetting swap for measurement readiness. Other
  projects' processes and VMs are not touched without asking.
- Forbidden: pushing lane branches or unverified work; posting outside
  `joshuarli` and `laputa-systems`; changing pinned toolchains.
- Non-goals: a native-symbol sampling profiler for third-party binaries
  (their invocations are attributed as opaque regions), a repository-wide
  line-coverage target, and syntax cleanup without measured or structural
  payoff.
- Contract changes for the owner: D1 (a new check diagnostic, or a SPEC 13.5
  rule); a versioned `xsht profile` interface and report format; corpus-wide
  rewrites from MG1; and any R1 language change.

## Campaign: semantic soundness (paused)

### Goal and state

The full-core universal-obligation sweep is paused since 2026-11-23. Focused Lean probes and proofs continue when performance work requires them. Nine of 77 obligations were closed at L2; model proofs and finite conformance evidence do not close other universal obligations. The [formal inventory](../formal/coverage.json), [formal README](../formal/README.md) and [implementation evidence](../formal/IMPL.md) own the current proof limits.

### Decisions, baseline and acceptance

The kept [semantic-soundness plan](../xsh-lean.md) contains the decisions, baseline, sequence G0–G10, connected milestone, acceptance commands and proof limits. It remains at its original path because the inventory checker and its tests parse it. Its status and links to the former campaign and roadmap are historical; this roadmap is the resume point. Resume the broad sweep only after its pause is lifted. Use the focused L2 item in **Sequence** for currently authorized proof work.

### Needs environment, execution boundary and non-goals

Use the proof and host-boundary requirements in the kept plan and formal README. Do not claim universal refinement from finite differential agreement. Preserve the existing language and host contracts; a proof or functional-core extraction does not authorize new syntax or a replacement runtime.

### Reference

[Formal README](../formal/README.md), [formal implementation](../formal/IMPL.md), [inventory](../formal/coverage.json), and [ticket backlog](../TODO.md).

## Campaign: internal functional cores

### Goal and state

**Status: active.** Make small semantic functions and explicit state
transitions an organizing principle inside the implementation. Semantic
functions receive their operands, relevant state, and observations explicitly.
Adapters acquire observations, perform host operations, and attach diagnostics
and source blame. Owned mutable storage remains an implementation choice.

Apply narrow gates to bounded fixes without requiring an unrelated full baseline.

The source basis is the owner's assessment of SX
[`9e9e4e5`](https://github.com/joshuarli/sx/tree/9e9e4e5bf3fcb834748eafedd227e22f5c95b527)
and XSH `ed917341e`, rechecked against the current implementation. SX's
[core/capability boundary](https://github.com/joshuarli/sx/blob/9e9e4e5bf3fcb834748eafedd227e22f5c95b527/docs/ROADMAP.md#rules-for-every-campaign)
and [request/response machine](https://github.com/joshuarli/sx/blob/9e9e4e5bf3fcb834748eafedd227e22f5c95b527/rust/sx-kernel/src/machine.rs)
are useful patterns. Its native cores remain trusted implementations, and
its mutation lowering admits fewer aliasing and escape cases than XSH.
Adopt explicit semantic dependencies while preserving XSH's language contract.

Use [`duration.rs`](../src/duration.rs) as the implementation template:
typed operands, explicit results and semantic errors, one decision shared by
constant preparation and execution, and thin runtime adapters. Keep components
internal modules. The [existing architecture](ARCHITECTURE.md#packages) governs
whether a later concrete consumer justifies a separate crate.

### Decisions and dependencies

Continue full Linux and Asterinas native qualification as a separate gate.
Start each pilot from an identified source freeze after its focused baseline
passes. Qualification repairs and test-cost work can continue independently;
partial runs cannot become a passing baseline. Performance, deployment, and
owner-deferred platform work keep their existing scope and gates.

Pilots 1A and 1B may proceed in parallel with separate ownership. Neither
requires completion of the other's implementation. Cleanup depends on their
accepted boundary discipline, including deadline shielding. It does not require
a blanket arithmetic or effect-system rewrite.

### Pilot 1A: structural membership

The first finite Rust/Lean comparison covers structural membership; its domain
and limits are recorded in [formal/README.md](../formal/README.md). This is
implementation evidence, not universal refinement. TYPE-MEMBER and the other
planned whole-core obligations remain planned.

Specify the admitted value views and closed, non-recovery type constructors
before changing code. Start with structural families already represented in
both implementations. State exact acceptance, rejection, and unsupported cases,
including scalar distinctions, key restrictions, record fields, and nominal
identity where admitted. Extend the domain deliberately after the first link.

Preserve borrowed `ValueView` access across runtime, lowered, and computed-field
representations. Keep the existing allocation boundary: no second generic value
tree, record materialization, value cloning, or stream pulling to make the
formal interface convenient. Production argument/result validation must use
the same decision being compared; a test-only reimplementation is insufficient.

Document each production route's interpretation of retained stream, callable,
scheme, generic, and environment-path contracts. The Rust callback and Lean
`ContractDecisions` must refer to corresponding facts. Name the validity and
preservation assumptions, and distinguish those assumptions from discharged
properties. Rust's permissive `Unknown` and `Invalid` recovery cases stay
outside the initial non-recovery correspondence domain.

Membership, schema conversion, and callable/module authentication retain
separate meanings. Inspect schema decoding's `&Evaluator` dependency and
narrow it only when its actual immutable requirements can be named. A broad
shared reference alone is not evidence of effects.

Relate the admitted Rust inputs and results to `Membership.value` and
`Membership.check_iff`. Compare acceptance and rejection through the production
operation, include rejected corruptions, and minimize counterexamples. State
unsupported cases explicitly. Finite differential agreement is implementation
evidence; the existing Lean decision theorem proves agreement with its own
judgment and does not by itself establish Rust correspondence or TYPE-MEMBER.

### Pilot 1B: supplied-time deadline selection

The production selector receives borrowed deadline state and a supplied
`Instant`; the evaluator acquires time and calls that selector. Lean proves
selection eligibility, earliest expiry and outermost tie order for arbitrary
finite states related to an order-preserving natural-tick clock abstraction.
It also proves the model's state update equations. Rust agreement is finite
conformance evidence over 2,925 inputs, supplemented by deterministic state
transition checks and the native `within` gate; it is not universal refinement.
Clock domain, units, representability and host behavior were qualified on Linux
and macOS (XSH-35) and remain assumptions of the formal model; physical suspend
needs real hardware.
No universal obligation is closed. See the [formal README](../formal/README.md)
for the relation and proof limits.

Retain native `within` tests for real sleeps, child cancellation, stream and
worker checkpoints, swallowed interruptions, and cleanup. Those tests establish
adapter behavior that supplied-time decision tests cannot. Expand into a full
state/event transition component only when selection extraction simplifies the
implementation. Do not add a universal request interpreter or serialized action
object to ordinary synchronous operations.

### Follow-up boundaries

Arithmetic and validation follow the duration pattern where two production
routes currently decide the same meaning. Checked Int `+ - * / %` now share
one decision across constant folding and execution, with a finite Lean
comparison in [formal/README.md](../formal/README.md#checked-int-arithmetic).
Effect work should preserve unknown
versus known-empty contracts, exact inclusion versus permission coverage,
concrete versus unresolved symbolic contributions, and error capture that
retains other contributions. The existing effect algebra may need no physical
extraction; connect its exact relations instead.
The finite effect algebra comparison and its proof limits are recorded in [formal/README.md](../formal/README.md#pilot-2-effect-algebra-domain).

For cleanup, first make the next action and failure disposition explicit.
Preserve reverse defer order, changing `errdefer` eligibility, managed-release
behavior, primary versus secondary errors, original exits and tracebacks,
forced cancellation, and deadline shielding. The runner still executes deferred
expressions and owns actual resources. Expand into frame scheduling or resource
transfer only when the initial boundary removes complexity. Pilot 3 extracts force-aware cleanup precedence, and Pilot 4 extracts next-action selection with `errdefer` dueness; each is checked against a Lean model and a finite conformance corpus ([formal README](../formal/README.md#pilot-4-cleanup-order-and-errdefer-dueness)). Frame scheduling, resource transitions and deadline shielding remain in the runner.

### Acceptance and proof limits

Each pilot must deliver all four outcomes:

1. Inputs and types expose a smaller dependency surface, with observations
   supplied at a visible boundary.
2. A real production path uses the semantic function, and any superseded
   decision is removed.
3. Record the formal connection in the existing [inventory](../formal/coverage.json)
   and its owner documentation: model, property, admitted domain, implementation
   relation, evidence, and remaining assumptions. Keep one proof inventory.
   Keep model proofs, artifact certificates, Rust evidence, and external host
   assumptions distinct. Partial pilots close none of the 77
   universal rows unless the existing whole-core closure rule is met.
4. A before/after comparison shows removed duplication, a narrower context,
   less fixture setup, or fewer route-specific cases. Moving files or adding
   another wrapper alone does not meet this requirement.

Keep Lean meaning independent of Rust layout and preserve the
[six-layer contract](../xsh-lean.md#layers-and-representation). State refinement
may permit several implementation steps per semantic step while preserving
observable outcomes. Independent verifier authentication remains a trust
boundary rather than duplicated decision logic to delete.

Preserve practical mutation and exact evaluation order. In particular, compound
assignment reads the target after evaluating its right-hand side and commits
nothing on a failed update. Account for failure and divergence when considering
rewrites. A proc's empty effect bound does not imply purity: `print` and `eprint`
remain observable despite requiring no effect. No persistent-collection mandate,
new language syntax, public API, or application-policy subsystem follows from
this implementation work. Pure administration and service policy stay in
ordinary XSH libraries over narrow host operations.

## Later

- XSH-66: remaining inference comparison drift; see [inference evidence](PERFORMANCE-INFERENCE.md#campaign-close).
- XSH-180: resolve remaining extrema ordering choices in the [draft RFC](rfc/XSH-180-min-max-ordering.md).
- LAP-20: remain parked by owner choice to retain gnu-stubs.
- LAP-52: owner-deferred platform work.
- LAP-87, LAP-88: complete the aarch64 musl and zlib security proofs where the host can run them.
- LAP-16: physical sound and wireless hardware enablement remains out of scope.
