# XSH steering: close consolidation, then deliver general inference

## Mission

Continue unattended in `laputa-systems/xsh`. Preserve the substantial compiler consolidation, native core-command corpus, and independent Lean work. Do not start another ergonomics or language-surface campaign.

Deliver three separately judged outcomes:

1. Close the already approved consolidation acceptance on a qualified source freeze.
2. Complete the general type-inference capability promised by the unified typing campaign, rather than equating improved concrete inference with that capability.
3. Establish a connected formal-verification milestone for an implemented compiler slice, without representing fragment counts or empirical comparisons as universal soundness.

The review baseline was `31c07c71f871428d255fe48e042725e703126b58`. Inspect the actual current checkout; do not reset to this commit. Reuse existing campaign ownership, lane/model policy, source-qualification procedures, and worktrees. Preserve unrelated work and recorded failed receipts.

Read current `AGENTS.md`, `docs/ARCHITECTURE.md`, `docs/TESTING.md`, `docs/DESIGN.md`, the relevant SPEC source templates, `dev/consolidation/{CAMPAIGN,HANDOFF,ITEMS}.md`, `formal/README.md`, `formal/coverage.json`, and the original unified typing campaign when available. Do not resurrect obsolete documentation paths or test commands from earlier briefs.

Honor the latest approved platform scope: the active consolidation campaign selects host-native x86_64 Linux musl in the pinned `Dockerfile.test` environment. Do not replace it with arbitrary Linux/glibc builds, revive the old macOS/ARM requirement, or mix receipts from unlike hosts, profiles, binaries, or resource allocations. Follow the repository's current `verification`, debug-unit, and release-performance profile rules.

The present consolidation campaign prohibits language behavior changes. Close it first. General parameter/scheme/row inference is a successor milestone, explicitly separate from behavior-preserving consolidation. This steering authorizes that originally requested inference capability in the successor, not unrelated syntax additions. Do not claim output parity for programs whose acceptance or inferred interface deliberately changes.

## What the review actually established

These are source observations, not newly executed build or performance results:

- `src/syntax/parser/stmt.rs::parse_params_arena_only` still emits `ParseExpectedParamType` for a required parameter with neither an annotation nor a default.
- `src/sema/check/infer_param.rs` infers parameter types from defaults, not general relationships inferred from unannotated function bodies.
- `src/sema/check/infer_return.rs` still requires explicit return annotations for exported and recursive pure functions.
- `src/sema/types.rs` has transient inference IDs, concrete structural records, typed callables, validated types, and builtin template parameters. These must not be confused with generalized user-function schemes and open row variables.
- `src/sema/constraints.rs` uses variable bindings into recursive `Type` values. Representative traversal follows aliases with a visited set; it counts alias hops against `MAX_TYPE_DEPTH`. This is not the requested level-based, path-compressed inference graph.
- `infer_default_parameter_types` can repeat statement-wide passes; `EffectGraph::solve_with` repeats scans of its nodes and edges. These are profiling/scaling targets, not permission to assert an unmeasured slowdown.
- `CheckedDeclarations`, published call/type facts, shared module preparation, verifier-owned execution routing, and shared record-type storage are valuable foundations. Keep them.
- The consolidation handoff retains passing historical native suites and focused repair evidence but explicitly leaves final acceptance open. In particular, do not erase the unexplained extra AWK desugar outcome because a later combined comparison passes.
- `formal/README.md` explicitly distinguishes its fragment results from Rust correspondence and universal obligations. Preserve that honesty and the existing full-core target.

Revalidate these observations before changing code. When an observation no longer applies, record the current owner and its executable evidence; do not implement a duplicate repair.

## Phase A — finish the existing campaign without expanding it

Follow the existing handoff's next actions and canonical gate owners. Do not invent another acceptance framework or multiply full-suite runs outside the existing policy.

Freeze the relevant implementation and script state. Bind reports to the actual source, binaries, profile, runner configuration, and test manifest. Reuse earlier evidence only when the existing qualification policy establishes that its semantic scope is unchanged. A documentation-only edit need not invalidate unrelated executable evidence; an applet repair does require its affected script evidence.

Close the outstanding functional, normal/forced-route, structural, semantic-review, documentation, resource, scheduler, and comparative-performance requirements already specified by the campaign. Use only one full native suite at a time. Run timed gates alone with both readiness checks. Never report timing from a run that failed correctness, was interrupted, or failed readiness.

For the unexplained AWK outcome, retain exact before/after source, environment, selected test identities, stdout/stderr/status, and worker configuration. Obtain a discriminating reproducer or bounded evidence identifying whether the cause is source semantics, fixture assumptions, harness scheduling, or environmental state. A later green run does not explain the earlier discrepancy. Do not invent a cause, weaken the oracle, or suppress a flaky-looking test. Apply the existing explicit nonclosure policy if a cause remains unestablished.

Keep upstream compatibility accounting separate from aggregate native-suite totals. Report pinned GNU/uutils/BusyBox origins, native ports, host-boundary exceptions, exclusions, skips, and actual upstream execution results as separate quantities. Do not manufacture a compatibility percentage from all native tests combined. Preserve the existing frozen denominators and explain every changed exclusion.

Deliver a concise current-source acceptance summary in the existing handoff. Preserve raw evidence and hashes, but make the summary interpretable without treating a chain of local `.work` paths as the conclusion itself. No new orchestration framework, extra report hierarchy, or broad unrelated cleanup.

## Phase B — establish a red/green capability contract for general inference

Before redesigning the solver, turn the original user-facing promises into executable acceptance cases. Keep their initial current-checker outcomes as baseline evidence. Use the ordinary loader/checker/indexed execution path, not a special demonstration mode.

At minimum, establish these positive witnesses:

```xsh
pure identity(value) {
  value
}

pure name_of(value) {
  value.name
}

export pure public_identity(value) {
  value
}

test inferred_relationships {
  identity(42) == 42
  identity("demo") == "demo"
  identity(false) == false

  name_of({name: "package", version: "1.0"}) == "package"
  name_of({name: 42, enabled: true}) == 42
}
```

`identity` must have a reusable relationship equivalent to `forall T. T -> T`; it must not be checked separately according to its first caller, retyped from all visible callers, or implemented through `Any`. `name_of` requires a record with `name: T` and permits other fields through a row relationship. A record without `name` must fail statically.

Test the exported scheme from two independent importer roots that instantiate it differently. Adding or removing one importer must not change another importer's inferred interface. Add transparent aliases, independent declaration reordering, alpha-renaming, nested records, optional fields, and mutation-invalidated refinements.

Also cover unannotated parameters anchored by established host signatures; contextual return/constructor/schema inference; monomorphic mutable accumulators; annotated and unannotated ordinary recursive declaration components; higher-order callable/effect relationships within the approved rank-1 design; and explicit public annotations that intentionally restrict an interface.

Negative witnesses must include incompatible writes to the same mutable binding, overgeneralization of captured mutable/resource-bearing state, stale field proofs, unsafe `Any`-to-concrete flows, wrong callable effects, missing required record fields, nominal-type confusion, recursive type equations, and ambiguous concrete overloads. Assert the relevant failure, not merely any nonzero exit code.

Preserve semantics selected by annotations. A generic identity instantiated at Bool returns false; a test/Unit-consuming statement asserts a false Bool. Result wrapping, propagation destinations, body-tail classification, and storage conversions must be fixed in elaboration, never chosen by runtime tags or whichever caller is checked last. Keep or insert purposeful annotations in old code whose meaning depends on them before removing redundant annotations elsewhere.

## Phase C — complete one inference core behind existing compiler interfaces

Build on current checked facts and registry templates. Keep the public/diagnostic `Type` representation as a projection if useful; do not force an unrelated façade rewrite merely to use compact solver IDs internally.

Implement genuinely distinct representations for inference metavariables, generalized scheme parameters, rigid annotated parameters where needed, row variables, and dynamic `Any`. An unsolved metavariable is neither an unknown runtime value nor a recovery type.

Use representative-based unification with rank/size, levels, occurs checks, and rollback-safe mutation. Share ground type structure and avoid cloning wide recursive types in every constraint, candidate, or publication step. Preserve current provenance and fail-closed behavior. Generalize safe declaration values at the proper boundary; instantiate quantified variables freshly at each use; keep environment-reachable and mutable variables monomorphic. Solve ordinary recursive components monomorphically before generalizing; do not introduce polymorphic recursion.

Support unique-label structural record rows for required fields and retained extra structure. Preserve existing nominal, validated, union, container, optional, and schema-validation contracts. Do not turn this into unrestricted subtyping, structural recursion, user-defined trait search, or automatic dynamic validation.

Generate constraints once where feasible and process dependency-indexed worklists. Changes should wake affected obligations, not recheck every unrelated body or module. Builtin templates, typed aliases, argument binding, and overload evidence have one semantic owner. Do not leave a new general solver beside permanent per-method or per-syntax inference repairs that repeat its decisions.

For effects, preserve explicit contract edges, inferred-summary edges, captured-error behavior, unresolved-work facts, delayed-work provenance, and lexical bounds. A dependency worklist and compact concrete fact sets should replace repeated whole-graph scanning where measurement supports it. Generic callable relationships may require effect variables; dynamic unresolved effects must never become the empty set.

Keep finite overload candidates indexed and pruned by known facts. Do not add global exponential backtracking. Residual sealed operation requirements may be generalized only as the approved design permits; concrete executable ambiguity must be diagnosed rather than guessed. Use deterministic complexity budgets without turning ordinary shallow programs into arbitrary depth failures.

Publish solved schemes, call instantiations, selected operations, argument layouts, row evidence, effects, and elaboration decisions. The indexed lowerer consumes this data without resolving types or checking bodies again. Prepared generic evidence must not become per-call inference, dynamic overload searches, or an AST interpreter fallback. Preserve fast monomorphic paths and verify any generic instruction/evidence representation before execution.

### Required solver regression candidate

The reviewed alias representation can accumulate long chains independently of semantic type nesting. Add this test directly at the solver boundary, initially against the existing implementation; it is a review-derived candidate, not a claim that a current XSH source reproducer was executed:

```rust
let mut constraints = TypeConstraints::default();
let span = Span::at(SourceId::new(0), 0);
let variables = (0..400)
    .map(|_| constraints.fresh(span))
    .collect::<Vec<_>>();
for index in (1..variables.len()).rev() {
    constraints
        .constrain(&variables[index], &variables[index - 1], span)
        .unwrap();
}
constraints.constrain(&variables[0], &Type::Int, span).unwrap();
assert_eq!(constraints.resolve(&variables[399]), Ok(Type::Int));
```

Adapt imports to the actual owner. Record the observed result before changing the algorithm. A chain of equal variables bound to Int is not a 400-level user type. Separate semantic type-depth limits, total-work budgets, alias-cycle detection, and representative traversal. Preserve rejection of true cycles, foreign variables, and malformed/deep types; do not fix this by removing safety limits or merely increasing 256. Include failed-unification rollback tests after compression and branch-local probes, so the performance repair cannot corrupt independent inference states.

## Phase D — use the enlarged command corpus as the inference benchmark

Freeze a representative cohort containing small glue scripts and substantial command implementations, not only synthetic identities. Include `core/wc.xsh`, other text/byte utilities, command parsers, structured-data transformations, library helpers, and representative tooling scripts. Include external consumer modules where already in scope. Select the cohort and annotation categories before seeing removal results.

Keep domain/wire schemas, intentional interface restrictions, and semantically necessary annotations separate from redundant implementation annotations. Record their counts and rationale; do not quietly shrink the denominator until a percentage passes.

Remove eligible annotations jointly in isolated fixture/worktree copies, then check and execute the resulting programs. Verify their outputs, failures, call order, cleanup, and selected operations against the annotated originals. Individual-removal success is insufficient: several annotations can each be redundant only while another remains.

Retain the original campaign's 95% eligible-local and 85% eligible-internal parameter/return/effect targets as targets, not claims. Exported inferred interfaces get an explicit separate comparison. No new `Any`, erased-record fallbacks, inserted per-call runtime checks, or caller-dependent signature inference may be used to improve the score.

A compatibility test corpus does not by itself test generic inference: maintain the dedicated relationship/refactoring/adversarial witnesses from Phase B as well. Conversely, synthetic generic examples do not substitute for the corpus. Require both.

Do not rewrite coreutils algorithms, add native speed shortcuts, or alter their compatibility contracts to make a typing benchmark easier. Their application schemas and corner cases are the workload, not obstacles to delete.

## Phase E — measure solver performance, not just repository-lint time

Keep the existing isolated 15-second repository-lint budget and the active campaign's performance policy. A passing end-to-end lint run is valuable but is not a measurement of solver latency or proof of a relative speedup.

On the same pinned host/profile/resource allocation, separately measure parsing/loading, constraint generation, solving/generalization, checked-fact publication, lowering/verifying, end-to-end cold preparation, retained typing memory, peak process memory, and any separately observed cgroup peak. Label these metrics accurately.

Retain the original typing campaign's paired frontend/preparation/runtime budgets. Freeze a fresh, justified successor baseline only after current consolidation closes; do not erase older failed receipts or silently rebase away regressions. Compare the new checker on unchanged annotated source, then annotation-reduced equivalents. Measure cold performance independently of session reuse; warm workspace checking is an additional result, not the substitute.

Instrument deterministic counts: generated constraints, representative hops, unifications, occurs visits, row-field visits, scheme instantiations, candidate tests, watcher wakeups, solver passes, and per-module/body check counts. Compare several sizes of alias chains, wide/deep records, repeated generic instantiations, reversed declaration chains, module diamonds, and effect-call chains. Genuine growth in output type size must be distinguished from repeated work on the same type graph.

Preserve session-cache isolation and source/option/interface key validity. Attribute cost from lint candidate rechecks separately; do not camouflage it as solver cost or hide it behind warm caches. Optimize measured dominant work, not every BTreeMap by principle.

Performance repairs must preserve solution/rejection under variable renaming, legal constraint reordering, module-root changes, and rollback. Raising budgets, deleting adversarial tests, or returning recovery types on exhaustion is not an improvement.

## Phase F — turn Lean fragments into a connected compiler milestone

Keep Lean an independent specification/verification layer. No production dependency on Lean. Preserve the existing formal campaign's core scope and all universal obligations; coreutils, system-report, btop, and other application algorithms do not become proof targets here. They may supply minimized semantic counterexamples.

Honor the post-consolidation source-freeze gate before claiming Rust correspondence. Do not optimize for number of roots, touched files, or fragment coverage. Choose a connected path with immediate relevance to this campaign and complete it before opening more disconnected model families.

A suitable first path is resolved lexical names -> generated constraints -> solved substitutions/schemes -> elaborated typed core -> verified indexed instructions -> observable execution for an explicitly defined core fragment containing variables, functions/application, immutable and mutable local bindings, records/projection, Bool/Result control, and the relevant effect boundaries. Stage it if necessary, but make each stage's exact domain explicit.

For that domain, state and prove the actual typing/substitution/generalization/elaboration obligations. Prove or independently check the translation from source constructs to modeled constraints; do not assume the checker selected the right operation or classified every effect and then describe a theorem about that assumed graph as checker soundness. Existing effect-lattice/solver fragments are useful inputs, not the missing translation proof itself.

Bind executable correspondence witnesses to actual emitted checked facts/raw IR and the active schema. Add negative mutations that the independent checker rejects, not just round trips through the same encoder. Distinguish universally quantified Lean theorems, validated finite certificates, empirical differential tests, and unproved host assumptions in the existing ledger. Finite conformance witnesses do not prove universal Rust implementation correctness.

Run the existing pinned Lean build, axiom/trust audit, kernel or external rechecking where required, and negative controls. Absence of `sorry` alone does not establish the desired theorem or implementation connection. Do not introduce compiler-trusting/native-evaluation axioms to evade the existing trust policy. Record necessary assumptions precisely rather than hiding them in bridge inputs.

Complete one connected slice as an intermediate milestone while leaving broader obligations honestly open. Do not shrink the original full-core theorem inventory or close the whole formal campaign because the intermediate slice passed.

## Execution and completion

Use existing current repository gate commands and changed-path selection. Process tests use verification/release products from the same target; debug-only unit assertions still run in their proper profile. Respect the no-source-tree-formatter/autofixer rule. Focused migration acceptance in isolated temporary fixtures is in scope; whole-tree mechanical churn is not.

Continue through independently ready work without asking routine design questions. Use bounded agent/worktree ownership and separate compilation/test/performance concurrency budgets. Do not weaken or quarantine a failing test to maintain momentum. If a genuine blocker remains, report the exact reproducer, constraint, failed gate, and dependent work; do not mark the milestone complete or replace the implementation with a report.

The final report must answer, in order:

1. Which existing consolidation gates are now closed on which qualified source, and what remains open?
2. Which formerly rejected unannotated programs now have principled inferred schemes, including independent imports and negative controls?
3. How many eligible annotations were removed jointly on the frozen real-program cohort, without weaker boundaries?
4. What do cold phase timings, retained memory, runtime comparisons, and scaling counters show?
5. Which Lean statements now form a connected proof/correspondence milestone, which are finite empirical evidence, and which universal obligations remain unproved?

The desired outcome is not another large volume of syntax, formal fragments, or acceptance paperwork. It is an annotation-light, fast, soundly constrained compiler whose existing corpus and formal layer test the same semantic contracts.
