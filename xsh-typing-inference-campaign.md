# XSH typing inference: remaining work

Implementation remains paused until requested. This document lists the remaining
functional work. Fix existing-program regressions first, complete runtime
integration and annotation removal, then consolidate the implementation.
Benchmark only after all functional work and migration are complete.

## Handoff — 2026-10-02

The user requested a wind-down: finish existing integrations, leave this handoff,
commit, and stop. Implementation remains paused after that commit. Resume only
on request. Work is integrated into the existing `master` checkout. This
campaign used the shared checkout without creating task branches; no additional
worktrees are present. The functional campaign is unfinished.

### Integrated behavior

The current changes preserve original checked source, scoped roots, declaration
and allocation owners through preparation and both indexed workers. Focused
execution and refusal tests cover frontend disposal, missing/foreign authority,
changed packets, lexical dominance and mutation. Completed slices include:

- Saved callable receivers and captured defaults; module receiver allocation,
  captures, result transport, and post-selection implementation pinning.
- Native argument lineage, declared JSON admission, Record.get, collection
  methods, Regex, Digest, Bytes.dump/strings, Record/Module keys, Map.push,
  optional FsRoot receivers, native result record projections and Result indexes.
- Immutable field membership refinement, lexical and live mutable captures,
  UInt mutation/creation, Tag construction/equality, pattern and iteration
  transport, line scanners, original fused pipeline callbacks and nominal
  callback capture.
- Dynamic CLI full-result carriers, run/context packets, computed Duration
  operands, finite mixed Str/Path command argv, typed UtilsCache callbacks, and
  public Hash argument adaptation to the private implementation ABI.

`lower/native_arguments.rs` replaces the old module argument AST rebinder.
`lower/hash_arguments.rs` replaces the legacy Hash named-argument rebinder;
Hash algorithm authority comes from the original registry boundary. Other
legacy adapters remain where a genuine source protocol is missing. In particular,
Tag adapter retirement is incomplete; the imported nullary producer test fails
before any legacy maps are erased. No constructor identity is fabricated.

### Verification at settlement

Exact debug `xsh`, `xsht`, and `xshi` products build successfully. The library
build uses `cargo test -p xsh --lib --offline --no-run`; execute its reported test
binary with `RUST_MIN_STACK=16777216 XSH_MODULE_PATH=$PWD`, `--test-threads=1`,
`--skip runtime::coverage`, and `--skip runtime::examples`. The final broad result
is **1,615 passed, 5 failed, 1 ignored**. The failures are the three Tokei slice
cases, imported nullary Tag applications, and static CLI postfix context.
Running the test binary without the stack setting aborts at Tokei.

`target/debug/xsht test --jobs 1 tests/xsh/stdlib` reports **96 passed, 25 failed,
4 skipped**. `tests/xsh/empty-list-equality.xsh` passes 2/2 and
`tests/xsh/tag-constructors.xsh` passes 3/3. Focused current integrations pass,
including Hash 4/4, module 6/6, field presence 4/4, fused stages 23/23,
ListText 2/2, capture/fallback controls 29/29, value binding controls 43/43, and
process/Duration 14/14. These counts overlap the broader library suite.

The filtered host runtime command was
`cargo test -p xsh --test integration runtime:: --offline -- --skip runtime::coverage:: --skip runtime::examples::example_corpus_is_formatted --skip runtime::examples::example_corpus_lints_without_warnings --test-threads=1`.
It completed **248 passing and 21 failing tests out of 400 selected**, then was
terminated at `runtime::modules::native_xsh_net_download_many_over_https_http2`.
`tests/runtime/modules.rs::LocalHttpsHttp2Server` blocks indefinitely in
`listener.accept()` after its client fails checking. The gate is incomplete;
131 selected tests have no result. Add bounded server cleanup on client failure
before rerunning this gate. This host run establishes no Linux support result.

Completed runtime failures cover collection accumulation/Map reads, example
execution/traces, lazy streams/worker producers, one Ctrl-P screen parity case,
six Unix process cases, three archive cases, Path command interpolation,
raw argv/environment helper coverage, DNS transport and two Net batch cases.
Keep those existing tests and observations; do not skip them to report a pass.

The final capture regression and all 29 focused capture/fallback controls pass.
The static CLI checker regression fails at its intended coarse Result boundary.
Formatting, linting, hooks, benchmarks, Docker/Linux support verification,
release builds, annotation migration and `make check` were not run in this
wind-down. No dependencies were added and nothing was pushed.

### First remaining boundaries

- `tokei_showcase_contains_direct_pure_calls`,
  `tokei_showcase_contains_scan_bytes_fast_paths`, and
  `tokei_showcase_contains_scan_lines_fast_paths` fail at original Bytes slice
  Expr1244, physical instruction 1264, Function 26. Fused callback ports now pass; the slice needs
  its own original operand/result protocol. Owner: `full/index_prepare.rs` and
  `generic/indexing.rs`; retain the original slice roots and packet.
- `original_tag_constructor_import_aliases_do_not_require_legacy_payload_or_wire_declarations`
  expects three checked constructor applications but receives one. Qualified
  nullary fields `second.Empty` and `second.Seen` need actual checker receipts
  before removing their lowerer fallback. Keep the original-member refusal
  control and imported alias test intact.
- Fold callback Result accumulators still fail the checker at nominal
  `CombineError.Stop` versus `CombineError`. The original capture receipt and
  data-error forwarding regressions pass independently; do not weaken their
  join or effect contracts to address the accumulator mismatch.
- Auth `render_shadow` reads Expr427 from formal parameter 0,
  `List[ShadowRecord]`. The original parameter proof exists, but Record formal
  iteration has no prepared item producer. Temporary diagnostics were removed.
- Module publication before handle selection can substitute a same-signature
  implementation in the mutable publication table. Allocation receipts and
  post-selection pinning do not establish independent publication identity.
- Static CLI postfix contexts still reject a descriptor-refined Result under
  coarse Record context. `cli_static_descriptor_postfix_preserves_original_refined_boundary`
  and the native `cli_static_descriptor_postfix_preserves_the_refined_result`
  preserve the genuine regression. The attempted legacy-context helper was
  unreachable: graph checking routes every canonical CLI candidate through
  `registry_boundaries.rs` before that path. The unused helper and its hook were
  removed. Fix the original graph/compact descriptor boundary without widening
  ordinary user Results.
- Native checker failures remain in Args, CLI, JSON Result[Any] validation,
  nested Map methods, Net/Process contextual validation, and Streams inferred
  callbacks. Native preparation failures remain in Auth, DNS record indexes,
  Env/Fs record bindings, process Result carriers, Linux fields, module fields,
  text producers, Test failure values, and Record field refinement.

The stdlib failing modules are `args`, `auth`, `bytes`, `cli`,
`cli_commands_constants`, `cli_constants`, `dns`, `env`, `fold-callbacks`, `fs`,
`fs_root_methods`, `group`, `json`, `linux`, `map`, `methods`, `module`, `net`,
`path`, `process`, `record`, `streams`, `test`, `text`, and `user`. Rerun each
native module to obtain its first authentic diagnostic; fixing a focused
boundary does not establish whole-module compatibility.

The frozen `bench/typing/wind-down-failures.json` is historical input, not the
current failure count. Recheck its remaining tests together with the current
regressions. The 730-operation inventory has not been fully activated or audited.
The joint annotation targets (740/778 locals and 2,386/2,807 internal sites) have
not been attempted; all 5,529 protected sites and 40 stabilizers remain required.
Migration under `core/`, `dev/`, and existing `examples/`, remaining adapter
retirement, complete relevant gates, final performance work, and read-only
`make check` under 60 seconds remain outstanding.

Keep bookkeeping minimal: ordinary regression tests and one concise status
update are enough. Keep a failure diagnostic only while it helps resolve an
outstanding problem. Do not create checkpoint ledgers, hash manifests, source
snapshots, build archives, historical plans, or per-run evidence directories.
Git and the test suite preserve implementation history.

## Campaign-specific lane orchestration

This strategy supersedes general orchestration guidance for this campaign.
Every campaign agent, including the integrator, uses `gpt-6.1-sol` with
`medium` reasoning. Do not substitute another model or reasoning level.
The primary agent remains the integrator and is the only agent that starts,
reassigns, or integrates lane work. This plan does not resume implementation.

### Keep independent work running

On resume, start eight bounded implementation lanes where their write sets are
independent. Use up to 15 worker lanes when there are additional difficult tasks
with disjoint files and settled interfaces. Refill a finished lane immediately;
do not wait for a whole wave to finish. Occupied agents are not progress if they
are waiting on the same interface or repeating the same investigation. Use slots nine through fifteen for ready
CLI, UInt, stage, producer/default, loader, diagnostics, and tooling slices
with exclusive owners; replace these choices as dependencies change.

For each assignment, send only the concrete failing behavior or missing
protocol, exclusive files, relevant tests, required invariants, and a clear stop
condition. A task should deliver one complete source-to-runtime slice, not
"finish callables", an inventory audit, or a report. Use existing failing tests
first; add a small regression and an independent refusal test when needed.

Keep one short ownership list in the integrator's working context: task,
agent, exclusive files, and current dependency. No tracking files, checkpoint
ledgers, separate acceptance reports, or agent-written planning documents.

### Initial bounded lanes

Paths below are relative to `src/`. They identify starting owners, not permission
to edit all nearby files. Confirm the exact write set before starting each lane.
Each lane owns its bug diagnosis, implementation, and focused regression tests;
do not assign another agent to independently solve or test the same slice.

| Lane | Bounded task and stop condition | Exclusive starting owners |
|---|---|---|
| JSON admission | Restore heterogeneous declared `List[Any]` JSON inputs while retaining child types and rejecting ordinary incompatible lists; stop when the source regression and refusal tests pass. | `sema/check/registry_boundaries.rs`; a dedicated checker/native regression module |
| Saved receiver | Make saved `.call` aliases preserve original receiver identity and captured defaults through preparation and both execution routes; stop at the existing omitted-default regression plus foreign/missing receiver refusal. | `runtime/eval/lower/callable_binding.rs`; `runtime/eval/indexed/generic/callable_receivers.rs`; `runtime/eval/indexed/full/callable_receiver_prepare.rs`; `runtime/eval/lowered_run/indexed_run/omitted_argument_tests.rs` |
| Direct native call | Complete one wire-enum direct-native admission/preparation path without overload rediscovery or erased evidence; stop at the wire regression and invalid argument refusal before host effects. | `runtime/eval/indexed/full/native_prepare.rs`; a dedicated native preparation regression module |
| Initializer wrappers | Repair the builtin-template cold initializer lineage through actual argument wrappers without fabricated source IDs; stop at the builtin regression and altered-wrapper refusal. | `runtime/eval/lower/argument_binding.rs`; `runtime/eval/indexed/full/argument_prepare.rs`; a dedicated initializer regression module |
| Validated record layout | Preserve nested validated record layout and extra fields through a host round trip and prepared projection; stop at both-route execution and schema rejection tests. | `runtime/eval/require.rs`; `runtime/eval/indexed/full/projection_prepare.rs`; a dedicated record-layout regression module |
| Ordinary value bindings | Activate one missing scalar Let or Guard-success binding form with lexical dominance; stop at execution, shadow/write/failure-body refusal, and frontend-disposal tests. | `runtime/eval/lower/value_binding.rs`; `runtime/eval/indexed/generic/value_bindings.rs`; `runtime/eval/indexed/full/value_prepare.rs` |
| Pattern transport | Repair one failing enum alias/alternation or nominal pattern transport path while retaining original owner and lexical capture identity; stop at its existing regression and cross-scope refusal. | `runtime/eval/lower/pattern_admission.rs`; `runtime/eval/indexed/full/pattern_prepare.rs`; `runtime/eval/lower/pattern_transport_tests.rs` |
| Iteration proof | Repair the scalar iteration regression and reject an item read attributed to its own iterator; stop at both-route execution and the existing ancestry refusal. | `runtime/eval/lower/iteration.rs`; `runtime/eval/indexed/generic/iterations.rs`; `runtime/eval/indexed/full/iteration_prepare.rs` and its tests |

The native-call and record-layout lanes share a behavior boundary, not files.
The integrator settles their admission/layout interface first. If two failures
share one root cause, combine them under one owner and refill the freed lane.
If diagnosis shows an initial task depends on an unsettled shared contract,
replace it with another independent regression task rather than launching
competing fixes.

### Shared interfaces and integration stay with the primary agent

The integrator exclusively owns shared facades and registrations, especially
`runtime/eval/indexed/full.rs`, `runtime/eval/indexed/generic.rs`,
`runtime/eval/lower.rs`, and the common checker and execution dispatch files.
This includes shared IDs, receipt fields, store/checkpoint registration, module
wiring, broad test runners, canonical docs, and the campaign document.
Agents may read these files and request a concrete signature or small wiring
change; they do not edit them concurrently. The integrator publishes that change
once and moves dependent lanes forward. Do not duplicate a missing helper or
weaken a contract to avoid a dependency.

Use a shared checkout for disjoint files. Create a temporary isolated worktree
only when a task needs an independently compiling intermediate state. After
merging into `master`, the integrator verifies that all useful work is integrated,
removes the worktree from disk, and deletes its merged task branch before closing
the lane. Preserve any useful uncommitted changes before removal; do not leave
merged worktrees or build caches behind. Do not recreate baseline repositories
or archived worktrees. Lane commits contain code and tests, not generated logs.

A lane returns a short message: changed behavior, focused test result, relevant
contract decision, and any remaining blocker. The integrator reviews and
integrates each complete patch, resolves shared wiring, and runs the affected
cross-lane tests. Only the integrator runs broad suites or shared product builds;
lanes run narrow checks without overlapping build storms. Keep at most one
Cargo build/test process per target directory. A queued compiler does not
justify more rebuilds, extra targets, or new evidence machinery.

### Refill by dependencies, then migrate

After a root cause is fixed, assign remaining failures by owning mechanism:
CLI descriptors, run/context, UInt assignment, stages, producer/default/defer,
embedded stdlib, and loader/interface reuse. A lane stops after one named
protocol and its positive/negative observations; split larger categories.

Then feed independent protocol slices from the remaining operation inventory:
conditional user calls, native families, scoped calls, stream callbacks,
live mutable captures, record constructors, and nominal constructors. Allocate
disjoint files before dispatch. Conditional calls and captures wait for the
shared call protocol; native families wait for direct-native authority;
constructors wait for the canonical layout/default interface. Keep ready
refinement, loader, diagnostics, and tooling tasks running beside those chains.

Once runtime compatibility passes, run separate annotation-removal lanes for
`core/`, `dev/`, and relevant `examples/`, preserving protected sites. The
integrator checks the jointly reduced graph. Adapter retirement, canonical docs,
and tooling can run in parallel only after their final interfaces settle, with
one owner per file. Fix cross-lane failures under the existing responsible owner;
do not start duplicate repair agents.

Functional acceptance and consolidation precede all benchmarking. At the end,
the integrator assigns bounded performance investigations only for actual
repeatable regressions, using the same model and reasoning setting.

## 1. Finish runtime integration

### Existing-program regressions

Use the current handoff and rerun the named gates before assigning a repair.
The frozen failure file remains useful for identifying unchanged programs;
several listed failures are now resolved. Keep existing observations, precise
types, original-source refusal controls and authentic checker roots intact.
Distinguish invalid synthetic fixtures from actual checker, preparation or
runtime regressions before changing a test. Restore compatibility before
starting annotation migration. Do not hide failures with exclusions, `Any`,
relaxed effects, alternate execution modes or AST fallback.

### Unactivated source and execution protocols

- Complete every applicable row in the frozen `bench/typing/operations.json`
  inventory: 730 entries, covering module, method, schema, error, stage, and
  language operations. Source catalog coverage is not runtime coverage.
- Expand prepared callable execution beyond the current bounded slices:
  conditional native/user authority sets, native families and overload members,
  scoped native invocation, proc/stream callbacks, inferred effect relationships,
  producer roles, dynamic splice/rest binding, fields/containers/returns, and
  mutable captures with actual live binding cells. Preserve complete kind,
  labels/modes, defaults/rest, signature, effects, guards, and ownership.
- Expand scoped operation evidence beyond Ok and the currently activated cases.
  Validate original requirement ancestry, binder scopes, contextual selections,
  operand/result relationships, forwarding, and active-frame ownership. Never
  supply a concrete type for an unused quantified relationship.
- Complete ordinary binding proof for Guard success continuations, specialized
  scalar Let rows, and remaining supported binding forms. A Guard binding is a
  real BindingIdentity, not a fabricated PatternId. Use actual lexical
  dominance; exclude its failure body, sibling scopes, shadowed slots, and writes.
- Extend iteration admission beyond direct simple-name `List[Str]` and its
  supported source recipes. Preserve original iterable/producer provenance,
  item types, target structure, body visibility, stream laziness, cleanup,
  cancellation, and resource ownership.
- Finish patterns/refinements: principal versus lexical schemes, distinct generic
  instances, imported facet owners, Path/Regex literals, unequal symbolic
  conditional joins, noncompleting branches and broader completing tails,
  source-independent fallback arms, writes/capture/shadowing invalidation,
  loop joins, immutable Boolean aliases, and bounded continuation facts.
- Finish row helpers and schema/constructor boundaries with original qualified
  nominal identities, phantom application arguments, defaults, wire decoding,
  UInt storage, reifiability, module promises, and resource escape checks.
  Retain extra record fields and validated physical layout through host bridges.
- Remove the remaining semantic native/constructor adapters. The next record
  constructor slice should consume original `ConstructorAuthority::Record`,
  supplied `ConstructorValueSource`/formal slots, ordered argument recipes, and
  canonical owner-qualified literal defaults. Emit existing record/schema
  operations directly; nominal/tag/error constructors need their own authority.
  Retire cloned arenas, invented projection ExprIds, overload rediscovery,
  argument rebinding, checked-body probes, and duplicated type reconstruction.
- Finish loader/interface correctness beyond the existing bounded reuse tests:
  each real dependency solves once per bundle, imports instantiate exported
  schemes, private owners remain distinct, and a changed dependency in a fresh
  bundle yields fresh answers. Across-edit caches remain outside this task.

### Verify runtime integration

Run the complete frozen annotated compatibility cohort, native stdlib and loader
owners, both indexed routes, ordinary xshi execution, tooling/registry/API gates,
and affected feature configurations. Check inside the actual execution worker
which route ran. Dispose of the AST/checker/inference bundle before execution;
retain only the immutable authority needed by prepared artifacts. Reject altered,
missing, foreign, stale, rewound, cross-scope, and jointly rewritten proof data.

Check output, default ordering, defer/pull/cancellation behavior, resource
cleanup, and module reuse in ordinary regression tests. Correct any affected
route tests whose forcing flag did not reach the execution worker. Runtime
integration is complete only when the affected existing programs and supported
protocols pass; a focused green slice is insufficient.

## 2. Simplify the architecture and language contracts while closing these gaps

These improvements are explicitly in scope. Use complete vertical slices and
observable regressions to choose the smallest useful changes; do not begin a
second unrelated redesign or add dependencies.

- **One semantic decision pipeline.** Remove the remaining parallel checker,
  compact probe, signature/effect reconstruction, and lowerer policy paths.
  Source checking should publish operation, binding, elaboration, and authority
  decisions once; lowering and tooling consume them. Representation validation
  and genuine dynamic/host/resource checks remain independent.
- **Explicit identity domains.** Model authored expressions, bindings,
  declarations, compiler temporaries, material operations, initializer wrappers,
  argument entry/ordinal, formal slot, and hidden receiver offset separately.
  Provide typed transport for generated instructions instead of fake AST nodes
  or row-kind-dependent origin publication.
- **A common authority lifecycle.** Make protected original receipts, derived
  indexes, owner/scope checks, atomic checkpoint validation, retirement,
  retained-byte accounting, and shrink behavior consistent across child stores.
  New child arenas must participate in prevalidation before any rewind mutation.
  Agreeing mutable copies are not independent evidence of the original choice.
- **Structured lexical binding analysis.** Replace residual instruction-number,
  top-level-let, or same-typed-slot guesses with a shared index of actual lexical
  definitions, continuations, reads, shadows, and writes. Reuse it for values,
  callables, iteration items, patterns, and Guard success boundaries.
- **A coherent call protocol.** Represent native/user/conditional/scoped
  authority and source-order evaluation separately from formal-slot packets and
  default timing. Keep semantic erasure/admission distinct from ordinary type
  equality. Avoid independently reimplementing this policy in each executor.
- **Validated record representation.** Centralize the conversion between dynamic
  records, prepared schema order, fixed numeric layouts, and host/container
  bridges. Preserve nested conversions and extra fields; never compensate for
  missing preparation with runtime name lookup on generic projections.
- **Actionable preparation diagnostics.** Preserve typed proof/owner/source
  failure context through `IrBuildError` and public diagnostics. Generic
  `full_ir_verification` without the failing obligation forced repeated temporary
  instrumentation and obscured whether a failure was source, codec, or runtime.
- **Reusable phase/lifetime test fixtures.** Standardize checked-source creation,
  route forcing in the worker, frontend disposal, prepared execution, and mutation
  controls. Keep fixtures isolated from unrelated module failures. Native language
  tests remain the default; host verifier, byte/process, and lifetime boundaries
  justify Rust tests.
- **Clarify language inference boundaries.** Keep Boolean assertion versus data,
  discarded Result versus propagation, omitted proc return inference, nullable
  joins, and schema reifiability fixed before generalization. Improve local
  diagnostics and examples instead of caller-dependent elaboration or arbitrary
  defaults. Consider narrowly scoped structural type syntax in existing type
  positions if named aliases are merely parser workarounds; preserve rank-1 and
  external schema intent.
- **Reduce integration contention and evidence overhead.** Assign shared header
  mutation to one owner, keep child interfaces coherent before parallel edits,
  and validate representative real programs after each vertical slice. Preserve
  useful regression tests and current failure diagnostics. Do not maintain
  historical evidence archives or benchmark during implementation.

## 3. Joint annotation removal and soundness

Use the frozen original/stabilized cohort, site correspondence, and
`bench/typing/annotations.json`. Do not change eligibility because a hard site
still needs an annotation. The joint targets remain:

| Category | Frozen eligible sites | Minimum removed together |
|---|---:|---:|
| Local bindings | 778 | 740 (95%) |
| Internal parameter/return/producer/effect annotations | 2,807 | 2,386 (85%) |

Preserve all 5,529 protected original sites and the 40 separately counted
semantic stabilizers. Public/domain/nominal/resource/schema/module promises,
independent validation targets, explicit `[]`, and annotation-specific coverage
remain protected. Test independent removals for diagnosis, then one jointly
reduced graph with normal checked preparation and runtime observations.

Finish alias/rename/independent reorder/constant/descriptor metamorphic cases and
independent negative witnesses for incompatible mutable writes, missing/nested
rows, invariant containers, nominal mismatch, dynamic laundering, stale facts,
wrong labels/spreads/defaults, effect bounds and timing, infinite equations,
captured-state generalization, mutable polymorphism, escaping identities,
higher-rank misuse, ambiguous overloads, error/schema/UInt/resource failures, and
changed Result behavior. Rejection must reach its intended boundary and occur
before forbidden host effects. Compare the supported fragment with the
independent reference. Tooling must reveal honest schemes, preserve comments,
refuse unsafe/partial edits, and converge on repeat application.

## 4. Consolidation, scoped migration, and functional acceptance

After runtime integration and annotation-removal tests pass:

- Apply proven semantics-preserving removals/stabilizers only under `core/`,
  `dev/`, and relevant existing `examples/`. Keep showcase and protected
  annotation fixtures unchanged. Preserve sibling-repository types and explicit
  dynamic/validation boundaries; do not run whole-tree autofix or formatting.
- Remove obsolete inference repairs, cloned probes, lowerer binders, duplicate
  effect/signature logic, permissive fallbacks, and temporary policy adapters.
  Verify actual callers are retired rather than merely renaming the helpers.
- Update canonical specifications, frontend/architecture/Test Map and relevant
  API/reveal/example owners. Keep comments self-contained; do not generate
  documentation churn or create a second authoritative typing guide.
- Build exact final debug products and run every affected relevant gate. Use
  release products only for justified profiling. Linux/Docker, cross-compilation,
  release packaging, remote publishing, daemons, new dependencies, and
  across-edit caching remain outside this campaign.
- Check operation coverage, jointly removed/protected annotations, existing
  program behavior, and negative tests. Keep one concise account of remaining
  failures or limitations. Commit completed work with hooks disabled; never
  push without explicit instruction.

Keep definition-owned rank-1 inference: callers do not train declarations;
mutable/captured/resource state does not gain unsound generalization; pure/proc/
stream kinds and closed versus unknown effects remain distinct. Preserve exact
schema/nominal/UInt/ownership boundaries and lexical return/defer/cancellation.

Functional completion requires existing-program compatibility, sound inferred
programs, annotation-removal targets, and the affected test suites to pass.
Do not relax types or execution contracts to make a test pass.

Prepared execution must consume solved decisions without runtime inference,
parsing, overload search, body specialization, or AST fallback. Share generic
bodies and authority metadata across calls. Pathological equations, ambiguity,
and expansion must reject within bounded work and depth with local diagnostics;
termination and soundness remain correctness requirements. Large scaling and
memory measurements wait until the final performance check.

## 5. Performance checks at the very end

Do not benchmark, profile, collect performance counters, rebuild baseline
products, or maintain measurement reports during functional implementation.
Only begin this work after runtime integration, annotation removal, migration,
and consolidation are complete and the relevant correctness tests pass.

Then compare representative real programs with the pre-inference baseline
`d6f09bc54305515b4c34d3b872d7f9f13b874061` and investigate repeatable regressions.
Include startup/preparation, unchanged monomorphic programs, inferred generic
helpers, module-heavy programs, and memory use. The retained scaling generator
is available if a suspected complexity problem needs an isolated reproduction.
Keep measurements practical; do not require quiet machines, fixed sample counts,
percentile targets, hash manifests, or exhaustive workload matrices.

Finally verify the requested whole-repository read-only `xsht lint` gate through
`make check`: no diagnostics and less than 60 seconds, never `--fix`. Preserve
types and the whole-repository scope.
