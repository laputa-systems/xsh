# XSH typing inference: remaining work

Paused at the user's request on 2026-10-01. Resume only when requested. This is
an outstanding-work contract; completed implementation and historical evidence
belong in the canonical documentation and `bench/typing/`.

The immediate blocker is runtime integration, not a new solver kernel. The
frozen integration run has 56 failing library tests. Exact failures, commands,
source fingerprints, and logs are recorded in `bench/typing/wind-down.json`.
Fix these before annotation migration or declaring full inference acceptance.
The primary checkout uses the existing `master` branch. Historical worktree
paths in archived measurements describe past runs and are not live dependencies.

## 1. Finish runtime integration (remaining Gate D)

### Existing-program regressions

- Resolve every failure listed in `bench/typing/wind-down-failures.json`, keeping
  existing programs, expected observations, precise types, and proof refusal
  controls intact. Distinguish an invalid synthetic verifier fixture from an
  actual checker/preparation/runtime regression before changing a test.
- Repair heterogeneous JSON-native input checking. The unchanged
  `json.encode_lines([1, "two", null, true])` is accepted by the baseline but
  currently trains the list item to Int. Preserve the canonical declared
  `List[Any]` JSON admission boundary, each original child type, and its JSON
  eligibility proof. Do not widen ordinary inferred lists or user annotations.
  The source seam is `check_graph_special_module_call` in
  `sema/check/registry_boundaries.rs`.
- Complete saved `.call` receiver activation. The existing
  `callable_alias_method_omissions_keep_prepared_defaults_after_frontend_drop`
  still fails lowering. Authenticate the original immutable binding, saved
  receiver initializer, wrapper, read, slot, and owner independently of supplied
  argument recipes; preserve captured defaults and callee-before-argument order.
- Finish direct-native declared-erasure integration. The wire-enum fixture now
  reaches `full_ir_verification` after native argument admission. Preserve exact
  candidate guards/relations, canonical nominal/wire owners, and record layout
  established by validation; keep schema failures before host effects.
- Finish cold initializer proof for the unchanged builtin template fixture.
  Actual executed CheckedValue/compiler argument wrappers and the original
  material expression have separate identities. Verify the sealed wrapper
  lineage without giving generated reads or synthetic projections source IDs.
- Repair the remaining ordinary function/block, CLI descriptor, run/context,
  assignment/UInt, stage, producer/default/defer, embedded-stdlib, runner, and
  Tokei regressions captured by the frozen library run. Do not hide them behind
  exclusions, Any, relaxed effects, alternate execution modes, or AST fallback.

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

### Integration evidence required before proceeding

Run the complete frozen annotated compatibility cohort, native stdlib and loader
owners, both indexed routes, ordinary xshi execution, tooling/registry/API gates,
and affected feature configurations. Check inside the actual execution worker
which route ran. Dispose of the AST/checker/inference bundle before execution;
retain only the immutable authority needed by prepared artifacts. Reject altered,
missing, foreign, stale, rewound, cross-scope, and jointly rewritten proof data.

Record exact source/product identities, independent CLI rejection witnesses,
output/default/defer/pull/cancellation/resource observations, and counted module
reuse. Re-run affected earlier route claims whose forcing flag did not originally
reach the execution worker. Gate D requires all applicable runtime obligations
and existing-program regressions to pass; a focused green slice is insufficient.

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
  route forcing in the worker, frontend disposal, prepared execution, mutation
  controls, and exact product provenance. Keep fixtures isolated from unrelated
  module failures. Native language tests remain the default; host verifier,
  byte/process, and lifetime boundaries justify Rust tests.
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
  concise reproducible evidence; quiet-machine timing, exhaustive sampling,
  redundant product rebuilding, and precise percentile gates are not required.

## 3. Joint annotation removal and soundness (remaining Gate E)

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

## 4. Lightweight performance and deterministic resource gates (remaining Gate F)

Use `bench/typing/benchmark-amendment.json` and the frozen resource/scaling policy.
Recreate a baseline checkout only when a comparison needs it, from immutable
revision `d6f09bc54305515b4c34d3b872d7f9f13b874061`; historical cohort identity
remains `877a114d6dc403db32c50f035b38aab7f193147a`.

- Compare identical stabilized annotated baseline/candidate programs, candidate
  annotated/reduced programs, unchanged monomorphic runtime, and generic helpers
  against equivalent monomorphic helpers. Use one warmup and five fresh-process
  observations with ordinary median/range. Investigate material repeatable
  regressions; no quiet-machine requirement, mandatory p95, or exhaustive sampling.
- Count parse/load, generation/solving, finalization, lowering/verification,
  execution, allocation, unique retained graph/interface/evidence bytes, RSS,
  and code size honestly. Do not move work outside the measured interval.
- Finish fixed-seed 1k/2k/4k/8k scaling for aliases, instantiation/forwarding,
  wide rows, nested containers, recursive components/effects, module diamonds,
  and overload probes. For the final two doublings, work grows at most 2.6x and
  retained type/constraint memory at most 2.5x; investigate wall growth above
  2.8x. Include unavoidable output size, failed probes, and diagnostic work.
- Finish deterministic adversarial termination under frozen node/work/depth/
  output limits. Reject pathological ambiguity/expansion/equations/fanout with
  local diagnostics, no stack exhaustion, unbounded retry, or Any acceptance.
- Preserve the requested whole-repository read-only `xsht lint` gate through
  `make check`: no diagnostics and less than 60 seconds, never `--fix`. Repair
  remaining diagnostics without weakening types or changing exclusions/budget.

No invocation may infer, parse, specialize a body, search overloads, allocate a
new metadata graph per item/call, or fall back to an AST evaluator. Share generic
bodies; any optional specialization needs bounded measured preparation.

## 5. Consolidation, scoped migration, and final acceptance (remaining Gate G)

After the preceding gates pass:

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
- Reconcile frozen inventories, jointly removed/protected sites, runtime and
  negative observations, deterministic counters, lightweight measurements,
  source/product identities, and honest platform/exclusion limits. Commit local
  completed work with hooks disabled; never push without explicit instruction.

Keep definition-owned rank-1 inference: callers do not train declarations;
mutable/captured/resource state does not gain unsound generalization; pure/proc/
stream kinds and closed versus unknown effects remain distinct. Preserve exact
schema/nominal/UInt/ownership boundaries and lexical return/defer/cancellation.

Do not declare completion from a solver, catalog audit, annotation percentage,
or focused test pass alone. If the same mandatory condition has three complete
attempts without meaningful progress, leave a reproducible incomplete checkpoint
instead of expanding indefinitely or relaxing the contract.
