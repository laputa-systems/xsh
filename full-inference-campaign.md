# XSH: complete general inference across the language

**Status:** active on `master`. Execution launched 2026-10-19 from `9134db7e7`
under the owner's grant for bounded isolated edits, local commits and
integration, and owned offline Docker operations. No pushing. The G3 matrix
rows pass and the G3 gaps are closed on `master` (2026-10-28). Two G3 items stay
open. G4, the corpus reductions, has started. G5 and G6 have not. The
[ROADMAP](docs/ROADMAP.md) orders this campaign after Pyramid.

**Resume:** read `AGENTS.md`, `TODO.md`, and this document. The checkable G3
status and the G4 measurements are in
[the inference evidence](docs/PERFORMANCE-INFERENCE.md). Recheck the limiting
stage on current `master` before choosing work, and rerun the affected checks
after any change to a compiler input or contract.

**Open items:**

- G3: the metamorphic programs and the independent solver comparison exist
  (see the slice 14 and 17 log lines). Nothing in G3 is open.
- G3: the row-override wording is done (see the slice 15 log line); the
  rejection itself remains a documented limit.
- G4: erase annotations on the practical cohort with the open candidate classes
  listed in the log (22 `ambiguous-overload` parameters, 22
  `compact.indexed-build` failures, stream return provenance, two
  `check.conversion` cases, and the `Set[Str]` generic-argument case).
- G4: the `supports_generalized_body` whitelist is replaced by the result of
  lowering the scheme body (done; see the slice 11 log line).
- G4: the frozen views need a source-qualified compiler pair (see the evidence
  page).
- G5, G6, and the F0 completion-ownership item below.

Deliver these results in order:

1. One rank-1 type, row, and effect authority for pure, proc, and stream declarations.
2. Ordinary bodies, recursion, safe callable composition, and prepared execution with no runtime solver.
3. Every capability and safety case below, including independent exports and delayed effects.
4. Joint annotation migration and behavioral comparison on the frozen and practical corpora.
5. Qualified cold, memory, runtime, and scaling results, followed by removal of superseded machinery.

## Execute within this charter

Use this executable core for sequencing. The numbered completion specification
below remains mandatory. Its G0–G6 checkpoints map to the work items here. An
intermediate checkpoint never waives a later requirement. These are inference
checkpoints. The [deployment campaign](docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md)
keeps its own G1 baseline and G6 deployment gates.

The coordinator owns integration, representation contracts, and acceptance, and
keeps the log at the end of this document. Follow the installed `orchestrate`
skill for lane contracts, integration, and worktree lifecycle. Do not create
another campaign tracker.

Decide inside the bounds below, record the reason, and continue. Park only the
affected item when authority or a required input is unavailable. A parked
mandatory item means the campaign remains incomplete. Technical difficulty,
unimplemented inference, and three unsuccessful experiments are reasons to
reassign diagnosis, not reasons to waive the deliverable.

The core compiler owner takes the coupled scheme/checker/lowerer/verifier
changes end to end. Other lanes own independent fixture files, corpus tooling,
diagnostics consumers, or loop repairs. A lane receives explicit files before
writing. The coordinator owns shared test registrations, generated output,
manifests, and lockfiles. Never create competing signature redesigns to fill
slots.

Use `.work/wt/full-inference-<lane>` and scratch under that checkout's
`.work/tmp`. Resource allocations, admission with `perf-gate-ready -s`, and the
lease owners are those of the deployment campaign's resource and ownership
limits. Run one full native suite at a time, and verify immutable merged
sources in a scratch worktree so later integration cannot alter a running check.

F0 must also enforce completion ownership in the existing launcher path: retain
the child handle until status and cleanup are consumed, and chain a dependent
stage only from an observed successful terminal result. Reproduce the past
mistake with a test child that finishes before the next observation. The owner
must consume it and advance or fail, never leave it recorded as running. Test
failure and interruption too. Use the existing process and launcher tests, not
a new polling service or durable job registry. A prose polling rule alone does
not complete F0.

Repair the loop before repeating a check that takes over two minutes for a
single semantic repro, or over five minutes for an incremental focused check.
Target 30 seconds for a prepared repro and two minutes for its focused group,
excluding the first necessary build. An unchanged workload over twice its P0
CPU or memory baseline is egregious and gets a bounded repair immediately.
Ordinary performance tuning waits until functional and parity completion.
Measure the smaller budgets in section 12 during development and gate them at
P4. Memory limits, correct cleanup, and fail-closed resource exhaustion always
apply.

After three distinct refuted hypotheses or 90 minutes without a discriminating
result, park the current approach with its minimal repro and refuted premise.
The coordinator reassigns diagnosis in the same turn and continues independent
work. Do not rerun a full gate to discover the same local failure. A baseline
red that blocks the item is repaired at its origin in a separate change.

## Decisions

### Owner intent recorded on 2026-10-19

The supplied completion brief expands inference beyond the first pure-only
slice. Sections 1–16 define the requested acceptance changes: inferred proc
and stream interfaces, rank-1 callable values, ordinary generic bodies,
monomorphic recursion, and compositional effects. They retain explicit
validation, intentional contracts, and the existing execution semantics.
No further product choice is required to implement those stated rules.
Execution effects still require a launch grant covering this expanded scope.

### Prescribed decisions

| Decision | Reason and reversal condition |
|---|---|
| Begin from the latest reviewed local master, freezing it once at launch. | `d489d8fcf` is the planning anchor, not a qualified new baseline. If master advances, inspect the intervening diff and repeat the two foundation probes before freezing. |
| Permit inference implementation alongside inherited G1 qualification; require matching final-source acceptance before close. | The owner's parallel-work direction already supersedes an implementation wait for G1. This does not transfer old receipts to a changed compiler or cancel deployment/cleanup obligations. |
| Keep one coupled compiler owner through G1/G2 representation work. | `SchemeType`, scheme facts, call evidence, indexed metadata, and verification share invariants. Split later work only after that owner publishes a compiling contract. |
| Keep schemes rank-1; solve recursive SCCs monomorphically before generalization. | This admits ordinary recursion and factories without higher-rank or polymorphic-recursion inference. Reject uses requiring those excluded capabilities with a specific diagnostic. |
| Preserve declared callable kinds, bounds, defaults, and parameter-label evidence. | Omitted types infer relationships; they do not authorize pure/proc coercion or caller-selected defaults. For incompatible selected defaults require supplied arguments or an existing purposeful annotation. |
| Generalize an empty producer's item only when no value is fabricated and no state escapes. | This chooses the sound generalized option in section 7. Otherwise diagnose the missing evidence; never choose Any. |
| Treat concrete overload ambiguity separately from a well-scoped residual sealed requirement. | Resolve grounded calls uniquely. Retain representable relationships in generalized schemes; do not use a caller to recheck or train the body. |
| Add no production dependencies, persisted cache format, or new public syntax. | Existing solver, registry, CLI, and benchmark owners cover this scope. A design requiring one is parked and redesigned within these bounds. |
| Build migration support as source transformations only. | Preserve old statement/value meaning explicitly before changing omission defaults. Delete migration-only compiler paths at close; introduce no legacy runtime mode. |
| Treat the cross-type overloads `byte_at` (`Str`, `Bytes`), `get` (`List`, `Map`) and `replace` (`Str`, `Regex`) on unresolved receivers as section 9 sealed requirements, not registry method families. | A caller must not train a callee, so a concrete call without a unique signature still fails; a generic helper retains the finite requirement and each use discharges it. Until implemented these sites are open work, not retained annotations. |
| Keep historical and current cohorts separate. | The accepted 1,040 view uses `15243e97`, while current corpus scripts describe 877. Neither mutable tables nor fresh diagnostics replace frozen eligibility. |
| Complete parity and capability coverage before optimization. | Section 12's numerical budgets remain unchanged but become release gates only at P4. Correctness, memory safety, and egregious loop regressions are never deferred. |

### Delegated decisions

The compiler owner chooses private graph and evidence encodings within the
single-authority and verifier invariants. Prefer extending the current encoding
over adding a new table. When two encodings meet correctness, choose fewer
independent owners, then less retained data. The corpus lane chooses additional
witnesses from changed call paths before running the reduced view. The loop
lane chooses selectors and safe artifact reuse after measuring what they select.
Neither lane may weaken acceptance or edit another lane's semantic code.

## Baseline and fast verification

Use these two loop speeds:

```sh
# Run inside the pinned environment with matching verification products.
"$XSHT_BIN" test --exact "$SELECTOR"
cargo test --locked --offline --target x86_64-unknown-linux-musl \
  -p xsh --lib "$UNIT_FILTER" -- --exact

# Integrator: changed-area checks after a coherent slice.
cargo dev gate PATH...
```

Use debug only for non-process Rust units. Process and native tests need
matching verification or release products and `XSH_BIN`/`XSHT_BIN` bindings.
Discover selectors with `xsht test --list FILTER` and require exactly the
intended nonempty set. An exact positive must report one pass and zero skips.
Inspect the generated gate plan in `dev/gates.xsh`: the broad checker mapping
does not select every inference-specific native file, so pass those paths too.

Before any full candidate freeze, run the warning boundary with the existing
pinned launcher and Cargo lease:

```sh
dev/consolidation/winddown/gate-tools/final-correctness-gates/run-native-final.sh \
  "$SOURCE" lint cargo clippy --locked --offline --workspace \
  --target x86_64-unknown-linux-musl --all-targets --all-features --quiet -- -D warnings
```

Bind the launcher's source, output, Laputa corpus, and resource environment
through its existing owner, rather than borrowing another source's target. Run
the focused changed crate first. The workspace command is the pre-freeze
integration boundary, not the per-edit loop. No autofix or formatter.

## Sequence and ownership

An item ends on its focused proofs. A phase ends on its touched-area checks.
Only P2 and P5 run the full integration boundary, for complete semantic-path
coverage and final combined qualification respectively. Re-run it earlier
only for a concrete integration risk that focused tests cannot cover.

| Phase/item; size | Prerequisites | Owned files and deliverable | Acceptance proof |
|---|---|---|---|
| P0/F0 ground and loop; M | Launch grant | Coordinator: the log below. Loop lane: `dev/gates.xsh`, relevant `dev/tests`, existing launchers only as needed. Freeze sources/products, baseline reds, commands, and corpus inputs. | Both foundation probes; minimal reds for each frontier; warning boundary repaired and checked; measure focused loop and show every selected test. No receipt credit from older sources. |
| P0/F1 freeze witnesses; M | F0 source freeze; overlaps loop repair | Fixture lane: new inference fixtures and separate native test modules. Corpus lane: `dev/performance/corpus/` and copied source views. No compiler edits. | Map every section 13 row and every section 4 node family to named positive/negative witnesses. Freeze changed-path coverage, classification, operation/effect snapshots, and paired behavior selection before removals. |
| P1/F2 canonical path; L | F0; F1 supplies witnesses continuously | One compiler owner: `src/sema/constraints.rs`, `schemes.rs`, checker, lowering, indexed schema/verifier/execution. Add nested callable monotypes, effects, SCC inference, and prepared evidence through one path. Coordinate SPEC, snippet and grammar updates in the same accepted slice. | G1 then G2: imported generic value → higher-order proc → generic Result → producer → post-disposal execution; own negative controls, unchanged existing controls, matching specification, and compiling mainline. |
| P2/F3 complete the language path; L | F2 contract; F1 matrix | Same compiler owner: remaining checker/elaboration/runtime files. Disjoint fixture and diagnostic lanes consume its published contract; no parallel signature redesign. Land each behavior change with its SPEC, snippet and grammar updates. | G3: every matrix row, existing expression/statement family, all delayed callable positions, rank-1 captures/recursion, grounded operations, and normal/forced disposal routes pass. Full changed-compiler integration boundary once, including grammar generators when affected. |
| P2/F4 complete corpus parity; L | F2 for probes; final reductions require F3 | Corpus lane: views, classification and existing cohort adapters. Compiler causes return to F3 owner. Tooling lane: `crates/xsht/` consumers with coordinator-assigned files. | G4: original/reduced historical eleven roots and practical roots check, lower, execute selected affected paths, compare operations/effects/interfaces, and reject deliberate mutations. Every retained in-scope missing relationship resolved. |
| P3/F5 harden; M | F3 and F4 | Compiler owner: remaining safety/error defects. Tooling/docs lane: remaining documentation, scheme rendering and safe fixes. Coordinator generates derived docs for each accepted slice, not only at F5. | Section 13 metamorphisms, independent test-only solver comparison, valid-before-invalid IR tests, safe-fix idempotence, diagnostic specificity, affected documentation checks. No missing mandatory positive or negative. |
| P4/F6 measure and optimize; L | F5 functional and parity completion | Measurement lane: existing `dev/performance/`, benchmark fixtures and counters. Compiler owner fixes measured mechanisms only. | G5: all section 12 budgets, deterministic scaling below, build/size gates from the deployment campaign, exact-output controls, readiness before/after each timed window. Record tradeoffs; no moving tolerances or benchmark denominator. |
| P5/F7 close; M | F6; inherited same-source qualification | Coordinator plus independent reviewer. Delete proved-dead paths, update canonical docs and this document, integrate reviewed local changes only within grant. | G6, final independent adversarial review with no P1/P2 open, full same-source canonical/supplemental/functional and inherited baseline acceptance; clean owned-process shutdown and worktree closeout. Mandatory parked items mean incomplete, never successful close. |

P0 starts with one foundation owner until its exact checks work, then can
sustain four lanes: loop, capability fixtures, corpus inventory, and
read-only contract audit. P1–P3 can sustain four: compiler, disjoint fixtures,
corpus, and tooling/docs. Give shared-file repairs to their current owner.
At P4 suspend competing timed/build work, but keep read-only review and report
preparation moving. At P5 integrate continuously while independent review and
disjoint documentation checks run. The coordinator runs slow qualification only
after assigning every ready independent item.

## Acceptance commands and boundaries

The section 13 matrix is a minimum, not a replacement for the detailed
requirements in sections 1–12 and 14–15. F1 records exact test selectors beside
each existing row in the G3 table of `docs/PERFORMANCE-INFERENCE.md`; add
fixture tests in ordinary native modules, not a new campaign runner. Existing rejection tests such as
`rank1_generic_function_cannot_be_stored_as_typed_callable` become positive
regressions for the newly specified case. Keep separately named higher-rank,
bad-effect, bad-capture, and dynamic-signature rejection controls. Never retain
an obsolete rejection as a reason to narrow this contract.

The low-level focused units include these existing names. Verify their module
paths on the frozen source before running a filter:

| Owner | Required controls |
|---|---|
| `src/sema/constraints.rs` | `alias_chain_of_400_variables_resolves_to_ground_type`; `failed_unification_rolls_back_rank_binding_and_path_compression`; row occurs/lacks, levels, instantiation and generalization controls |
| `src/sema/check/module_driver.rs` | `captured_mutable_bindings_are_not_generalized`; independent imported schemes and reordered import roots |
| `src/runtime/eval/indexed/full.rs` | `user_schemes_execute_after_frontend_drop_on_both_routes`; new callable/effect/binder/layout mutation controls with valid original fixtures |
| `tests/xsh/` | `rank1-inference`, `typing-capability-witnesses`, `typing-generic-body`, `inferred-proc-return`, `typed-callables`, `typed-callable-verifier`, `private-proc-effects`, `stream-producers`, `local-inference`, and affected lexical/exported effect files |

Extend the existing independent soundness/reference evaluator under
`crates/xsh-fuzz` with a test-only core solver oracle. It may not call production
unification or copy its algorithm. Compare finite generated type/row/effect
problems at fixed seeds, including negative cases and rollback. Mutation
controls must demonstrate that the comparison detects a wrong result.

For F4 reconstruct the accepted views with the exact `15243e97` retained table
and commands in `docs/PERFORMANCE-INFERENCE.md`. Include its witness-fixture
correction. Keep the 48-file, eleven-root and 9,194-site accounting unchanged.
Do not use the mutable current 877 table or current witness list as 1,040 evidence.
Use `dev/performance/corpus/inventory.sh`, `prepare_views.sh`, and
`check_views.sh` for a separately frozen current practical cohort over `wc`,
`sort`, `awk`, and isolated Laputa `pm/generation.xsh`; also run the existing
`check.sh` utility/plan/generation behavior boundary. Read each usage contract
and supply real clean revisions, exact selectors, products and hashes. Extend
these adapters only where they cannot compare newly affected paths. Their
current selected witnesses are a starting point, not sufficient completion
coverage. Include error, stream-pull and cleanup paths when those bodies change.

Freeze the retained classification before final erasures; diagnostic labels
alone are not classifications. Prove each claimed semantic selector, ambiguity,
unavailable input, or excluded higher-rank case with a minimal witness. The
758 Proc restrictions and 202 effect clauses all require review. For effects,
compare the whole selected dependency component jointly, not a sequence of
single-site lint suggestions. Report annotation-only erasure separately from
source rewrites. Require the old concrete contract to instantiate the new
private scheme, not identical printed types. Intentional public promises stay.

At F0 choose fixed scaling inputs of 64, 128, 256 and 512 units for every
section 12 family, retaining the 400-alias control and the existing semantic
depth limit separately. For fixed-size output per unit, no normalized work
counter may grow above `3 * previous + 128` per doubling. Normalize only by
independently counted emitted type/evidence size; show both raw and normalized
counts. Cases with genuinely growing output keep that growth visible and
still bound repeat visits per output node. Exceedance is a diagnosed F6
failure, not an excuse to omit a family. Preserve current deterministic
resource limits until a tested, stricter bounded representation replaces them.

Reuse the deployment campaign's benchmark owners and source/product qualification.
Add only the missing inference workloads/counters. Use at least 100 interleaved
completed baseline/candidate invocations per cold distribution for median/p95,
and at least five interleaved runs per runtime workload with median, min, and
range. Define cold as a fresh process with no in-process frontend/module cache;
record OS page-cache state separately. Do not drop host caches. Profile outside
timed samples. Check exact outputs and completed work, both readiness checks,
memory, retained data, binary sizes, and CPU/build costs. A difference inside
observed spread is inconclusive. If a tight budget cannot be resolved, collect
up to three independent admitted windows; unresolved results block acceptance.
On host load report `perf gate deferred: host busy` and continue non-timed work.

Final qualification uses the existing owners, in their documented order:
canonical bootstrap/execution in `dev/consolidation/final_canonical_replay.py`,
supplemental `dev/consolidation/winddown/gate-tools/final-correctness-gates/launch.py`,
then `dev/performance/check.py --config CONFIG functional --check` and the
deployment campaign's normal profile, timed observer, forced, comparative,
scheduler, util-linux resource/private-namespace/close prerequisites. Use the
real independent-review context for baseline close; the bare CLI does not
manufacture it. Never use the unavailable `inference`, `optimize`, `deployment`,
or `close` adapter commands as acceptance. Inspect the current CLI contracts
instead of copying an old invocation. Preserve failed attempts and resolved
cleanup; a changed compiler needs new same-source receipts.

## Execution authority and exclusions

Conventional commit subjects name the area. Item IDs belong in the body. Mark
contract changes as breaking where applicable and surface the omission, export,
callable and elaboration changes in the final report. Never push.

Keep execution offline using already available pinned inputs. If an input is
missing, park its dependent item instead of fetching moving versions. Do not
edit ordinary sibling Laputa, live roots, host packages, CI, Rust toolchain
pins, kernels, or release artifacts. This plan adds no KVM, privilege, or
network requirement. Deployment effects stay with the deployment campaign, and
this plan does not close its obligations.

Delete superseded restrictions only after proving their replacements cover
callers, tools, tests, imports, generated inputs and prepared execution. Search
for every removed path and classify survivors. Preserve historical evidence;
delete temporary dual authorities and runtime compatibility paths. Worktree
cleanup follows `orchestrate`: stop the owner, inspect modified/untracked/ignored
data, preserve needed work, remove only the recorded clean integrated worktree,
and delete a branch only when ancestry proves integration. Never force removal.

No higher-rank or polymorphic-recursion inference, new syntax batches, public
effect handlers, user traits, dynamic inference, secondary executor, native
backend, persistent arbitrary-script cache, daemon, new package catalogue,
Lean theorem expansion, or new orchestration/provenance platform belongs here.

## Reference: mandatory completion specification

Implement the original general-inference ambition in `laputa-systems/xsh`. This is the completion campaign, not another pure-function pilot, annotation-removal exercise, or proposal for future work. Preserve the integrated solver, canonical schemes, checked facts, verified indexed execution, and existing runtime repairs. Extend their authority until ordinary XSH implementations need annotations only to state intentional contracts, supply genuinely missing information, or resolve genuine ambiguity.

Read `AGENTS.md`, `docs/PERFORMANCE-INFERENCE.md`, `docs/ARCHITECTURE.md`, `docs/TESTING.md`, the relevant SPEC templates, and the existing inference design and capability tests. Do not reopen the retired consolidation campaign or create another orchestration/provenance system. Preserve historical evidence and accounting.

Follow the current pinned x86_64 Linux musl environment, toolchain, build profiles, resource admission, isolated-worktree policy, and source/product qualification. Do not revive obsolete macOS/ARM commands. Keep ordinary Laputa checkouts and live systems untouched; use the approved isolated source views and local fixtures. No publishing, new kernel work, or release upload. Lean stays in its current maintenance scope; full theorem development is not a prerequisite or a parallel expansion of this task.

## 1. Scope decision: implementation limits are not permanent language contracts

This brief explicitly supersedes the first-slice restrictions that permit generalized schemes only on pure declarations, prohibit ordinary constructs in quantified bodies, reject generic callables as statically typed values, require ordinary recursive returns to be annotated, and require proc/stream parameters merely because their declaration kind is not pure.

The requested product scope includes the necessary language-acceptance changes for omitted annotations, inferred callable interfaces, supported generic bodies, and statically typed callable composition. Execution still requires the separate grant described above. The scope excludes new unrelated syntax, implicit host operations, a dynamic runtime inference fallback, new trait/operator-extension APIs, and weakened resource/validation rules.

Deliver one coherent rank-1 type-and-effect system with structural record rows. Infer required/default/rest parameter types where the program determines them, return and yielded-item types, safe let-polymorphism, callable relationships, and effects for private and exported definitions. Support ordinary monomorphic recursion before generalization. Do not require users to spell type variables, effect variables, type applications, dictionaries, or compiler evidence.

An annotation may still select Record versus Map for `{}`, identify an external schema, deliberately restrict a public interface or capability set, distinguish a nominal domain, or resolve an operation for which the program supplies no unique evidence. Those are legitimate reasons. “This is a proc,” “this callable is stored in a local,” “this generic body has a branch,” “this result contains a type parameter,” and “this return contains null” are not sufficient reasons.

In particular, the 758 historically retained Proc annotations are an implementation-frontier inventory, not a permanent annotation floor. Reclassify their causes under the completed system without altering their historical counts. Keep the separate purposeful Map-selection cases separate. Do not relabel unsupported inference as semantic necessity to meet a completion claim.

## 2. What completion means

Every mandatory capability below must run through the ordinary parser, checker, importer, lowerer, verifier, and indexed executor. Positive examples must have concrete assertions about inferred relationships and runtime behavior; negative cases must fail for the intended reason. Parser acceptance, isolated solver tests, a successful `xsht check` followed by a lowering blocker, or a fallback to Any is not completion.

Target forms include these illustrative declarations:

```xsh
pure identity(value) { value }

pure choose(flag, left, right) {
  if flag { left } else { right }
}

pure name_of(record) { record.name }

pure apply(transform, value) { transform(value) }

proc read_text(file) {
  file.read_text()?
}

proc apply_checked(action, value) {
  action(value)?
}

stream repeat_item(value, count) {
  repeat count times {
    yield value
  }
}
```

The stream fixture uses the existing repeat statement; infer its item and count requirements without adding range or generator syntax. Supply self-contained definitions for other illustrative helpers and all required module/export documentation.

The intended relationships include:

- identity relates its argument and result without dynamic erasure.
- choose uses the existing condition contract (Bool in its concrete witnesses) and one compatible branch type; retain a sealed condition requirement when appropriate rather than guessing among existing condition domains. Its scheme must not depend on which caller is checked first.
- name_of accepts a record containing a name field and preserves the field's type while permitting unrelated fields.
- apply accepts a statically typed pure callable and preserves the relationship between its input and output.
- read_text derives a Path parameter, a Result success type, and the existing host/error obligations from the selected method.
- apply_checked derives a Result-returning callable requirement, relates its payload to the proc's result, and carries the callback's effects rather than replacing them with an unrestricted or empty contract.
- repeat_item derives its item type from yield, while stream construction, first pull, aliases, and cleanup keep their current runtime behavior.

Each example is a target, not a claim that current HEAD accepts it. No production special cases keyed to fixture names, body text, or known callers.

## 3. One inference representation for callables of every supported kind

At the review anchor, `SchemeType` can represent parameterized containers/rows, but a callable is not a parameterized nested scheme-type node, and `TypeScheme` rejects quantified non-pure callables. Complete the existing representation instead of removing that check without replacing the necessary invariants.

Represent callable monotypes with ordered parameter/label information, required/default/rest shape, argument/schema expectations, return or producer-item types, callable-kind constraints, and effect relationships. Their component types can refer to enclosing quantified type, row, and effect parameters. Keep schemes as quantification over monotypes and constraints; do not put arbitrary nested `forall` binders into every field.

Use distinct IDs/roles for unification metavariables, quantified parameters, rigid annotation variables, record-row variables, effect variables, and genuinely dynamic types. Error-recovery states must never become successful schemes. Extend the canonical `CallableScheme`/`TypeScheme` ownership and its ordinary ground projection; do not add a parallel proc-inference table or higher-order signature cache.

Preserve declaration identity, visibility, source provenance, parameter labels, defaults, rest behavior, and schema expectations through the pending-to-checked transition. Inferred exports publish complete closed schemes and obligations independently of importing callers. Public author-written clauses and types remain enforceable contracts; no handwritten annotation is required merely to export an otherwise well-determined definition. A written public Result annotation keeps its existing explicit-error-type rule. An inferred public Result publishes its complete inferred error contract without making the author repeat it.

Infer module/declaration dependencies by resolved binding identity. Ordinary recursive components use provisional monotypes and shared effect variables internally, solve the component, then generalize eligible variables. Do not instantiate a function polymorphically at its own recursive call. Remove the blanket explicit-return requirement for recursive functions. Reject genuine infinite types and requirements for polymorphic recursion; do not mistake a safe, generalized unconstrained variable for an error solely because it is not concrete.

Use the same instantiated signature and argument-layout evidence for direct calls, aliased calls, imported calls, pipeline calls, and typed indirect calls. Their typing must not depend on textual spelling when resolved identity and behavior are equivalent.

## 4. Generic bodies use the language, not a whitelist of trivial forms

Remove the separate quantified-body subset. Every existing expression and statement form must either participate in ordinary constraint generation/elaboration or have a specific semantic incompatibility diagnosed. “Unsupported generic node” is not an acceptable final restriction for ordinary well-typed control/data flow.

Cover literals and constants, nested blocks, immutable and mutable locals, records/maps/lists/sets, Optional/Result values, field/index/slice operations, operators, match/patterns, if/guards, loops/comprehensions, defaults and rest arguments, source-order named/spread/spliced calls, resource scopes, defers, local try/retry, and applicable stream operations. Preserve the real semantics of all admitted constructs; do not rewrite difficult source into synthetic helper functions to bypass the generic checker.

Constructed `Result[T, E]`, `Optional[T]`, and containers of parameters must lower and verify. `pure wrap(value) { Ok(value) }` must not pass checking and then fail because its success slot is quantified. Preserve the language's defined defaults for unconstrained error types; otherwise retain a well-kinded generalized error parameter where allowed. Nominal errors stay errors, not arbitrary unconstrained type arguments.

Allow loops and mutation inside a generic callable when they are sound: a local accumulator is monomorphic within one activation, but its element type may be a parameter of the enclosing callable, freshly instantiated at another call. Mutation is not a reason to ban the whole generic function. Captured shared mutable state requires stricter generalization, described below.

At every existing node family, test the same operation with ground types and with a quantified enclosing parameter that is legitimately supported. Document missing implementation coverage as open work until it is implemented; do not narrow the final language specification to match the current body classifier.

## 5. First-class callable composition within rank-1 inference

These must work without `Pure`, `Proc`, or repeated callable-type annotations:

```xsh
use utilities

let identity = utilities.identity
let number = identity(42)
let text = identity("demo")

let selected = if choose_first { normalize_a } else { normalize_b }
let normalized = selected(input)

let first = apply(utilities.identity, 42)
let second = apply(utilities.identity, "demo")
```

Preserve a reusable scheme through immutable aliases to known generic callables, including imported exports. A module's constant-key field/index/get projection must not reject a generic export solely because it is being obtained as a value. Keep private-export checks and distinguish known export selection from arbitrary dynamic lookup.

Support ordinary higher-order parameters, returned callable values, existing closure/capture forms, and callables in inferred record/list structures where rank-1 typing permits them. A record or collection can have a generalized outer binding whose callable fields share those binders. Do not require impredicative polymorphism just to represent that case. At a use of a generalized binding, instantiate its outer scheme freshly; within one monomorphic aggregate/activation, keep related types related.

A higher-order parameter is monomorphic within that invocation. Passing identity to apply at Int in one call and Str in another is required. Requiring one parameter to be instantiated independently at Int and Str inside the same invocation is higher-rank polymorphism and outside the original rank-1 scope. Diagnose that distinction clearly; do not fake support with Any. This exclusion cannot be used to reject ordinary rank-1 callbacks, generic aliases, or generic callable factories.

For a runtime-selected callable, infer a compatible shared call contract and a sound effect summary from the possible values. Do not erase it just because selection is conditional or the binding is mutable. A mutable callable binding has one compatible monomorphic contract; incompatible reassignment is rejected. Do not infer a union of unrelated parameter signatures or let the selected runtime handle choose static typing rules.

Keep parameter-label/default/rest semantics deliberate. A higher-order requirement created only by positional calls must not invent a required source parameter name. Named calls and explicit callable contracts constrain labels; actual callee labels and defaults remain in prepared evidence. Known aliases preserve their declaration metadata. Indirect calls must use the actual callee's applicable defaults at the established point, or supply all arguments where the existing callable contract requires that. Complete static argument/splice binding where enough information exists; do not silently materialize or reorder arguments to support the shorthand. When different selected functions cannot supply one unambiguous omitted-argument contract, require the argument or a purposeful callable annotation rather than inventing a default.

Typed invocation remains ordinary `f(args)`/`record.f(args)`. Do not require `.call(...)`, adapters, wrapper lambdas, or a new function-type syntax merely because a function is generic. Existing deliberately dynamic callable APIs stay explicit and conservative. Do not permit runtime `.require`/type patterns to certify opaque callable signatures unless the existing runtime contract actually carries independently valid evidence.

## 6. Generalize safe values; never generalize shared state by accident

Remove the pure-declaration-only generalization policy. A proc or producer definition is a callable value whose body is not executed when its scheme is generalized. A fresh call can have its own type instantiation even when invoking the body has effects. The body's effects must be represented, not used as a blanket reason to prohibit all polymorphism.

Use levels and environment reachability plus a justified value/generalization restriction. Variables reachable from captured mutable cells, resource-bearing state, and active monomorphic bindings must not be generalized. Fresh mutable locals allocated inside each activation do not automatically prevent generalizing independent parameters of the enclosing callable. Bindings produced by an effectful computation require a sound check for escaping state; immutable `let` alone is not proof.

Pure, non-expansive aliases and structural values should generalize when safe. Returned callable values should generalize at a new immutable binding when the existing purity/capture evidence justifies it. If a factory has allocated shared mutable state, do not let two uses choose incompatible types for that state. Keep producer instances one-shot and monomorphic even when the producer factory has a generalized scheme.

Do not introduce a global pure/proc coercion. Pure call contexts exclude forbidden callable kinds/host effects, even when a proc currently has an empty summary. Effectful contexts may invoke the supported callable kinds using checked evidence. Keep explicit declarations meaningful.

Require negative tests for shared mutable captures, references hidden inside records/containers, aliases to resource handles, a single instantiated producer reused at incompatible types, and effects hidden by returning a callable. These restrictions prevent unsoundness, not ordinary extraction of stateless helpers.

## 7. Infer return and yielded types without letting callers select behavior

Use bidirectional expected-type propagation and declaration-local constraints, not use-site monomorphization or global guesses. Infer pure returns, proc Result success/error relationships, and stream item types consistently for private and exported definitions. A stream with no annotation derives its item type from yields and applicable expected constraints; a stream with no usable item evidence may expose a sound generalized item parameter if no values of that type are fabricated, or report genuine ambiguity according to the declared rules. Do not simply default every unknown item to Any.

Proc defaults require a carefully pinned elaboration rule. Build on the current private-proc value inference: a successful value body ordinarily yields the proc's Result envelope, a Result tail preserves its existing envelope, and a statement-only body returns Result[Unit]. Extend that rule to generic and exported procs. Do not add an extra Result layer automatically or unwrap a Result merely because a consumer expects its payload. Explicit nested-result annotations remain meaningful.

Collect explicit returns, reachable tails, failure-only exits, and lexical propagation obligations before finalizing the return contract. Mixed returns must unify or have a justified existing join; they must not collapse to Unit or Any. An all-error body with no payload evidence may retain the established Unit fallback where the language specifies it. Do not use that fallback to swallow an actual value-producing path. Existing `fail` semantics follow their actual current expansion/target, not an older brief's definition.

A final Bool in an inferred value-producing function is a value, including false. A test or explicitly Unit-consuming block retains assertion semantics. A generic tail that is later instantiated to Bool cannot become an assertion at runtime. Non-tail Bool statements remain assertions. Fix all statement/value, implicit-Ok, argument/storage-conversion, and error-destination choices during checked elaboration, before emitting code.

Inventory old code whose missing annotations selected statement semantics or whose explicit annotation supplied a conversion/validation/nominal choice. Preserve its meaning with the smallest purposeful annotation or explicit source form before applying the new default rule. No version pragma, long-lived legacy inference mode, runtime tag switch, or per-caller body reinterpretation.

For null-bearing functions, distinguish legitimate Null data, Optional payload holes, and failed inference. A branch returning null and another returning a concrete T should infer the existing Optional[T] join when no observable distinction is lost. Empty collections and unused success/error parameters may remain generalized rather than forcing concrete types. No blanket `return_type_is_concrete` rejection may discard a valid generalized shape merely because it contains Null, Optional, a container, or an error parameter. A function returning only null does not thereby reveal the name or structure of a domain schema.

## 8. Infer callable effects and latent obligations compositionally

Extend the existing EffectGraph and canonical callable scheme to represent effect variables and relationships, not only `Option<Arc<[Effect]>>` concrete summaries. Retain compact concrete sets for resolved operations; use symbolic variables and inclusion/union/subtraction constraints only where necessary. Do not add a public effect-language syntax, handler runtime, or scheduler.

For an effectful higher-order helper, its inferred scheme must express the callback's effect contribution plus the helper's own effects. Instantiate that relationship at the call site. A callback parameter that is known from a checked relationship is not an opaque unrestricted Proc. Conversely, a genuinely dynamic callable with no trusted contract remains unknown, never the empty effect set.

Preserve explicit upper-bound clauses and their `io` implication/`without` rules. Calling through aliases, records, conditional selections, module exports, or parameters must not launder effects. Publish complete inferred exported relationships; importing clients cannot alter the declaration's scheme. Omitted clauses are inferable, written clauses stay enforceable promises.

Keep error propagation distinct from returning Err as data. Local try/retry removes only the outward captured error obligation; it does not remove fs/net/process work or an error that escapes via another path. Preserve causes, ctx metadata, source attribution, and runtime failure/cancellation distinctions. A callback-returned Result is not automatically propagated unless the consuming form has that established behavior.

Preserve delayed-work causality. Producer defaults execute at first pull, not creation or checking. Producer obligations remain charged at the creation site under XSH's current contract, even when a consumer pulls inside another `without` scope. Callback-stage obligations remain at their existing operation sites. Retain origin/body/creation-site evidence through aliases, returned producers, containers, branches, and delegation. Inferring a Stream[T] alone is not enough if it drops its associated latent obligations or captured resources.

Default arguments, deferred actions, signal hooks, stream stage callbacks, and all other existing delayed callable positions participate in the same effect evidence. Do not infer an effect away because its code executes later. Pure default expressions are not pre-evaluated just to obtain their type. Scope restoration and cleanup failures remain part of the real control-flow contract.

Solve recursive effect dependencies to a bounded fixed point, including function-valued edges. Preserve explicit contract edges versus inferred implementation-summary edges. For annotation migration, compare a whole selected component with and without eligible clauses; single-declaration redundancy proofs do not justify a mutually dependent joint deletion. Keep intentional restrictions even when their current body needs less.

## 9. Preserve builtin operation relationships as sealed constraints

Generic bodies using existing overloaded operators/methods need a principled contract, not caller-dependent declaration checking. Use the canonical registry signatures to express sealed operation requirements relating receiver/operand types, result types, effects, labels, and any checked conversion evidence.

When a generic operation has several supported domains, retain the finite relational obligation in the inferred scheme when that obligation is coherent, well-scoped, and representable. At a concrete use, discharge it from the known type arguments and emit the selected operation evidence. Do not check the source body again. A helper can be polymorphic over applicable byte-access or arithmetic domains without inventing user-definable traits.

Distinguish residual polymorphic requirements from genuine ambiguity. An overloaded generic helper is not automatically an ambiguous concrete call. But a concrete call for which no unique admitted signature is established must fail, with the remaining candidates and missing fact. Do not search unrelated imports, choose the first candidate, default Any, or add coercions to make a choice.

Operation selection and effects must remain tied: a residual requirement cannot carry the effects of one implementation while executing another. Preserve nominal/schema expectations, numeric representations, argument order, conversions, defaults, and failure behavior. When registry signatures are insufficient, improve their shared expressiveness and delete local repairs instead of adding a source-spelling exception.

Use candidate indexing, dependency-triggered pruning, canonicalized constraints, and memoization of fully grounded evidence. No global exponential overload backtracking, open instance search, or extensible trait solver. Generalization must reject escaping or semantically unresolvable operation requirements rather than hiding them.

## 10. Complete local/row inference and preserve explicit dynamic boundaries

Local inference uses one static type for each mutable binding, constraints from all statically relevant writes, and ordinary control-flow refinement for reads. Empty List/Set/explicit-Map constructors and null-initialized scratch variables can defer their type variables until resolved or generalized safely. Do not change `{}` into a Map because later code happens to index it as one.

Record requirements are unique-label rows, including nested fields, destructuring, typed updates, and pattern facts. Preserve extras and lacks-label constraints through calls and returns. Keep container invariance, validated types, nominal enum identity, error-family rules, and typed Map keys. Do not implement rows by erasing records or adding repeated runtime validation.

Expected types propagate through bindings, defaults, branches, returns, callable parameters/results, constructors, Optional/Result wrappers, and constant-known field access. A named/imported constant retains the same descriptor facts as its inline equivalent. Do not execute arbitrary code to infer descriptor contents.

Flow refinements are a bounded dataflow analysis over resolved/versioned bindings and paths, not an SMT theorem prover. Invalidate affected facts on mutation and potentially mutating calls, retain sound disjoint facts, merge conservatively, and respect loops and handled failures. An earlier successful assertion does not prove a state after it was mutated or after failure was caught on another path.

Unannotated compile-time parameters may acquire a required record shape. External Any does not. Runtime schema validation remains explicit through `.require(Schema)` or `.require()` with a genuinely grounded, reifiable static target. Later field usage alone is not permission to invent an external schema. A source cast/annotation cannot fabricate trustworthy static evidence. Preserve separately documented dynamic operations instead of disabling Any wholesale.

## 11. Prepared generic execution with no runtime solver

Keep one indexed executor and one verified program ownership model. Lower a generalized body once with binder-scoped types and sealed-operation/call evidence. Calls instantiate prepared evidence, not source bodies. Where a runtime-selected callable is necessary, use a function handle plus program-owned environment and validated evidence; runtime dispatch of a value is allowed, runtime inference or overload search is not.

Preserve ground fast paths. Do not require all calls to allocate fresh maps of types or revalidate recursively every argument merely because polymorphism now exists. Use compact immutable descriptors, stable program-owned IDs, shared evidence, and bounded lexical environments. Existing defensive checks at genuinely dynamic boundaries remain.

The verifier must independently validate binder scopes, row lacks/required fields, argument layouts, substitutions, effects, captures, and the types of actual indexed producers/consumers. Do not let two identically forged metadata records certify each other. Existing registration-time signal-hook snapshots, callable writer proofs, and alias/resource ownership must survive the new representation.

No solver IDs, borrow of checker state, global symbol lifetime, unresolved schema, or runtime-owned resource may escape into immutable generic metadata. Frontend disposal tests must drop parsed programs, CST, checker/solver state, and compact declarations before verifying/executing the prepared program. Cover ordinary and forced indexed routes.

A typed callable selected or returned at runtime must carry enough prepared evidence to call it safely after that disposal. Immutable modules publish schemes independent of clients; client instantiations do not mutate cached declarations. Program/module caches key all relevant source/config/feature/interface/default/constant information. No daemon, persistent arbitrary-script cache, mandatory warmup, JIT, or native backend in this campaign.

## 12. Fast solver requirements

Extend the current TypeConstraints authority rather than layering another general solver beside it. Use compact IDs, shared ground structure, union-find representatives with rank/size and levels, separate row/effect cells, occurs checks, and rollback-safe trails. Path compression must remain correct under failed candidate probes and cloned constraint problems. Preserve the already repaired shallow 400-alias case and true recursive/deep-type rejection.

Generate body constraints once per required source owner where feasible. Use dependency-indexed worklists: waking an unrelated whole module or cloning a wide type graph per candidate is not acceptable as the normal strategy. Keep scheme instantiation proportional to the quantified/reachable shape actually needed, sharing immutable structure. Effect solving is monotone and bounded over the declared finite effect universe plus constrained variables.

Keep diagnostic provenance compact. Record why constraints exist without eagerly copying/rendering complete types at every edge. Render useful conflicts at failure, naming the relevant source contributions. Canonicalize scheme display modulo binder renaming so legal source reordering does not perturb interfaces or overwhelm diffs.

Instrument invocation-local counts: constraints, representative traversals/compressions, occurs visits, level adjustments, row-label visits, instantiations, candidate tests, watcher wakeups, effect-edge processing, source/body checks, evidence nodes, and diagnostic work. These counters must distinguish legitimate growth in output type size from repeated work on the same graph.

Keep resource limits deterministic and fail closed with a local explanation. No Any/recovery fallback on exhaustion. Do not call the complete solver linear merely because union-find is efficient; inferred type size and qualified obligations matter. Test several sizes of alias chains, dependency chains, wide/deep records, higher-order composition, generic streams, recursive effect components, module diamonds, and finite-overload chains.

Use the current accepted baseline's pinned environment and source-qualified product pair. Record both the same annotated source and its annotation-reduced equivalent. Preserve the original campaign budgets unless the owner has explicitly superseded a particular one:

| Metric | Acceptance budget relative to the frozen reference |
|---|---|
| Cold frontend/preparation median, unchanged annotated source | Reference + max(5% of reference, 1 ms) |
| Cold frontend/preparation p95 | Reference + max(10% of reference, 2 ms) |
| Peak process memory and retained typing data, reported separately | Reference + max(10% of reference, 2 MiB) |
| New compiler: reduced versus annotated equivalent, median frontend cost | Annotated cost + max(20% of annotated cost, 2 ms) |
| Existing monomorphic runtime median | Reference + max(5% of reference, 1 ms) |

These are acceptance targets, not claims of measurements already obtained. Keep phase CPU/wall times, total preparation, runtime, retained memory, allocation traffic, and peak RSS/cgroup data separate. Pair samples, bracket readiness, and exclude other heavy work from timed windows. Warm workspace results are additional, not a substitute for cold performance. Calibrate very short workloads with batching and deterministic counters without hiding per-invocation costs.

Measure equivalent statically typed higher-order/generic programs against their existing explicit-annotation forms. No blanket runtime exemption for “now generic.” Genuine indirect dispatch may have a different cost from a direct call; expose that cost rather than labeling runtime inference as unavoidable. Preserve code size and preparation-memory accounting, and bound specialization if any targeted optimization is justified. Do not solve regressions by widening tolerances, removing hard fixtures, or changing the benchmark denominator.

## 13. Capability and safety acceptance matrix

Keep and extend existing native capability tests. For each row, require declaration metadata, end-to-end execution, failure-specific negatives, and normal tool invocation—not only synthetic solver output.

| Mandatory family | Positive witnesses | Essential negatives/invariants |
|---|---|---|
| Callable kinds | Unannotated private/exported pure, proc, stream; host-anchored parameters; inferred returns/items | Explicit incompatible bounds/types; malformed written annotations; producer kinds remain distinct |
| Body composition | Branches, patterns, loops, accumulators, constructors, Optional/Result, ordinary calls, defaults/rest | Unsupported operation domains reject normally; no generic-body or lowering-only blockers |
| Callable values | Imported aliases, conditional selection, parameters, returns, records/lists, existing captures | Private exports, incompatible mutable callables, unknown dynamic signature, no hidden per-call checking |
| Recursion | Ordinary single/mutual recursive components; inferred returns; independent calls after generalization | Infinite types, polymorphic recursion, premature instantiation inside SCC, declaration-order dependence |
| Effects | Higher-order callback effects, explicit bounds, joint inferred recursion, try/retry, defaults/defers | Effect laundering, unknown-as-empty, wrong source charge, dropped unresolved/delayed obligation |
| Streams | Generic producers, producer aliases/returns, delegated streams, early consumer stop | Reinstantiating a live cursor, eager defaults/pulls, lost captured effect/resource, duplicate cleanup |
| Rows/locals | Nested projections, updates/destructuring, null/concrete joins, empty accumulators | Missing labels, duplicate/lacks violations, invalidation after writes, incompatible shared state |
| Operation constraints | Reusable builtin-domain relationships, fixed evidence at concrete calls | True concrete ambiguity, wrong selected operation/effect, forged conversions, unbounded candidate search |
| Dynamic boundaries | Explicit reifiable require target, named/inline descriptor parity | Any becoming a record/callable contract without evidence; phantom external schemas |
| Prepared programs | Frontend disposal, client-independent schemes, one lowered body, bounded evidence | Forged binders/substitutions/effects/captures; stale program ownership; runtime solver dependency |

Include representative examples where the old pure-only path succeeds but extracting a callable, adding a branch, making the helper effectful, or returning a generic Result currently fails. Those are regression targets for generalization, not grounds to rewrite the example into the original limited subset.

Run metamorphic tests for alpha-renaming, import-root order, reordering independent declarations, transparent aliases, named/default argument equivalence, inline versus constant descriptors, helper extraction that preserves capture/cleanup, and equivalent explicit/inferred types. Restrict transformations to actual semantic equivalence. An annotation may select a conversion or narrow a public contract; removing it blindly is not a valid metamorphism.

Use the existing soundness fuzzer/reference evaluator where appropriate, plus a small independent test-only solver for the supported type/row/effect core. Compare success/failure and schemes modulo renaming on generated problems. Include rollback, generalization, effect substitution, and row constraints. This is test evidence, not a claim of full-language formal soundness. Lean maintenance remains separate.

For every malformed-IR test, prove the unmodified fixture valid, mutate exactly the intended invariant, and check the relevant rejection. Unrelated stale pool IDs or malformed fixtures must not stand in for semantic-verifier coverage.

## 14. Corpus acceptance without gaming the goal

Preserve the historical 9,194-site inventory, 693 eligible locals, 2,190 eligible internal annotations, and the exact historical 1,040-removal source view and products. Preserve the old 95%/85% goals as historical goals; the Map/Record literal distinction can genuinely prevent the old local percentage. Do not change old evidence, silently swap in a different retained table, or mix isolated compiler results with main.

Freeze the completion-campaign classification before applying final removals. Every retained site must name one of: intentional external/domain/capability contract; source meaning or elaboration selector; genuinely absent information; genuinely ambiguous operation; unsupported higher-rank/recursive type requirement; or missing in-scope inference. The last class is open work, not a legitimate completion exclusion. “Proc,” “stream,” “generic body,” or a diagnostic family alone is not a cause.

Revisit all 758 Proc restrictions under the newly authorized inference rules, all 202 effect clauses jointly where source evidence permits, and the remaining checker-restored site classes. Build minimal source witnesses that expose their actual cause. Cascading diagnostic counts are not counts of independent blockers. Do not use a no-suggestion lint result as proof of necessity.

Evaluate all selected erasures jointly in isolated source views with one source-qualified compiler pair. Compare source checks, inferred interfaces, chosen operations/elaborations, errors, output, side-effect order, stream pulls, and cleanup on actual coreutils and Laputa witnesses. Broaden the existing paired runtime selection to exercise changed call paths; a check/lowering pass plus one targeted test cannot establish whole-corpus behavioral equivalence.

A formerly concrete private function may acquire a strictly more general inferred scheme. That is expected. Require the old contract to be a valid instance and preserve behavior at the existing calls, rather than requiring identical printed types and thereby forbidding generalization. Intentional public contracts remain constrained unless their migration was explicitly selected and documented. Do not weaken error/effect bounds or add runtime validation to make erased annotations pass.

Keep annotation-only erasure distinct from reviewed source improvements that remove a semantic choice. For example, changing `{}` to an explicit existing Map constructor is a source rewrite, not evidence that deleting its type annotation alone was valid. Report both honestly when useful, but do not conflate their denominators.

Completion requires the mandatory capability matrix to pass and every remaining in-scope failure class to be resolved. Do not call an implementation complete because the easier percentage passed. Conversely, do not corrupt type/literal semantics to reach a mathematically impossible historical percentage. Publish absolute counts, historical percentages, the predeclared completion classification, and concrete examples of purposeful annotations left behind.

## 15. Tooling, migration, and execution discipline

Update SPEC templates and grammar together with each accepted behavior change, including any oracle/formatter/parser assumptions about mandatory proc/stream annotations. Do not merely change expected tests: retain the old controls as negatives where still semantically valid and record the new inference semantics where deliberately changed.

The checker is the sole type/signature/overload/argument authority. Lowering, highlighting, completion, reveal-type/annotation output, linting, refactoring, and API rendering consume published facts. They must render general relationships rather than erase them to Pure/Proc/Any. No new source syntax is necessary for printing complete inferred schemes in diagnostic/API notation.

Safe source fixes use resolved identities, checked facts, and CST spans. Preserve comments, lexical order, error propagation, source bytes, and import-source deduplication. Recheck the complete candidate and require idempotence. Never insert annotations into a capability witness just to preserve the original implementation restriction. Keep meaningful explicit annotations in real code when they state a contract.

Maintain one compileable integration owner across scheme, checker, lowerer, verifier, and runtime contract changes. Delegate disjoint solver, tests, corpus, diagnostics, and performance work through existing ownership rules. Do not parallelize conflicting redesigns of the same signature representation. Active performance/deployment work may continue when it does not contend for the same paths/resources, but this full inference contract is no longer optional follow-up after deployment.

Qualify the latest integrated baseline through the existing finite regression gate. Isolated inference work can overlap baseline qualification under current owner rules. Never claim old receipts qualify a changed compiler; never use an unrelated failing harness to justify unsound shortcuts. Keep failing evidence and bounded repairs. No wholesale merge of old divergent branches or revival of superseded machinery.

Use `cargo dev gate` and the current test-map owners; do not paste historical test commands blindly. Core semantics use native XSH tests, while Rust owns low-level solver/verifier, byte/handle, disposal, counters, and process-boundary witnesses. Run source-qualified verification products in the pinned environment and reserve release for appropriate performance/final gates. One full native suite at a time. Focused fake/local fixtures need no external network or live host mutation.

Do not run source-tree formatters or broad autofixers. Temporary fixture formatting/fix tests follow the repository's explicit acceptance authorization. Use current generated-document procedures only for edited templates/reference metadata. Keep old campaigns historical and the active status concise.

## 16. Staged gates and stopping condition

G0 — freeze current source, supported behavior, unresolved frontier tests, representative corpus classes, and measurement contracts. Preserve meaningful old semantics before enabling broader omission rules.

G1 — extend the canonical callable/type/row/effect graph and argument evidence; keep the project compiling and all established inference controls passing. No duplicated semantic authority.

G2 — demonstrate one complete path containing an imported generic callable value, an unannotated higher-order proc, generic Result flow, and an inferred producer, with correct effects, rejection controls, and execution after frontend disposal. This is an intermediate checkpoint, not the campaign's endpoint.

G3 — complete ordinary body forms, recursion, safe let-generalization/captures, operation obligations, locals/rows, and all declared/delayed effect positions through the same path. Close the entire capability matrix; no pure-only or whitelist-only final profile.

G4 — perform the jointly reduced frozen and current practical corpora with meaningful paired behavior coverage; resolve every remaining missing-in-scope inference class. Retain justified annotations and accurate accounting.

G5 — pass cold, memory, runtime, and deterministic-scaling gates on the combined compiler; preserve fast monomorphic execution and diagnose actual resource exhaustion. Complete necessary optimization without broadening the language or creating a second runtime.

G6 — delete superseded per-kind restrictions, duplicate passes, temporary repair paths, and migration-only compatibility execution; finish canonical documentation and current combined acceptance. Publish a concise exact-source completion report.

Intermediate successes do not waive later gates. Continue autonomously through ready work under existing safety/resource constraints. A genuine blocker requires its exact failing source, relevant constraint/evidence, and dependency status; it does not permit a report to replace unfinished implementation. Do not restart Lean, introduce new features, or shrink the ambition to the accepted first slice.

The final report must show: which declaration/callable/body restrictions were removed; inferred examples for all callable kinds and higher-order effects; independent exported scheme evidence; remaining purposeful annotations and their reasons; joint corpus results; phase/scaling/runtime costs; deleted duplicate authorities; actual gate commands and source/product identities; and any unresolved mandatory item. If a mandatory item remains, state that the original ambition is not yet fully delivered.

**Success is one integrated, fast, annotation-light XSH typing system across ordinary systems code—not excellent inference confined to a specially supported subset.**

## Verified source map

Read the current implementation before changing each owner. These paths were
checked at the planning anchor; the earlier first-slice designs are historical
context, not an override of this completion contract.

| Area | Current owner |
|---|---|
| Quantification, callable schemes, ground projection and registered-call evidence | [schemes.rs](src/sema/schemes.rs) |
| Type/row solving, rollback, counters and generalization | [constraints.rs](src/sema/constraints.rs) |
| Declaration return inference and recursion restriction | [infer_return.rs](src/sema/check/infer_return.rs) |
| Checked body facts and nonmaterial constructor evidence | [scheme_body.rs](src/sema/check/scheme_body.rs) |
| Callable signature fitting | [typed_callable.rs](src/sema/check/typed_callable.rs) |
| Effect graph and concrete fixed point | [infer_effects.rs](src/sema/check/infer_effects.rs) |
| Module-level scheme and capture checks | [module_driver.rs](src/sema/check/module_driver.rs) |
| Indexed lowering, independent verifier and execution | [lower.rs](src/runtime/eval/lower.rs), [full.rs](src/runtime/eval/indexed/full.rs), `src/runtime/eval/indexed/full/`, `src/runtime/eval/lowered_run/indexed_run/` |
| Canonical language text and generated outputs | [SPEC template](docs/templates/SPEC.md), [testing procedures](docs/TESTING.md) |
| Historical inference design and frozen corpus design | [first slice](dev/consolidation/designs/inference.md), [cohort](dev/consolidation/designs/typing-cohort.md) |
| Accepted source views, reproduction, retained classes and practical adapters | [inference evidence](docs/PERFORMANCE-INFERENCE.md) |
| Inherited gates and deployment obligations | [performance and deployment campaign](docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md) |

## Log

One line per judgment a reader would otherwise miss. This document's
prescribed decisions are the starting rules.

- 2026-10-19: Execution launched from `9134db7e7`. Inference implementation runs alongside the deployment campaign's G1 qualification. Close still needs matching final-source acceptance.
- 2026-10-28, G3 gaps: closed on `master` by one-case gates. A recursive pure function that indexes a generic receiver binds its wrapper return before generalizing (`6841a2107`). A nullary proc that tail-calls a generic proc applying its callback expands the application when the argument cannot hold a resource (`b4a5620c7`). A pure-effect rejection and the follow-on errors after a body error or a failed call report one diagnostic (`f248649ea` and the cascade commit after it). Dynamic-boundary, prepared-program and metamorphic rows produced no defect with ad-hoc witnesses.
- 2026-10-28, reduced composition: the quadratic checker output of the reduced higher-order composition workload (453, 941, 2685 and 9245 output nodes at 8, 16, 32 and 64) stays. The earlier diagnosis that each scheme carries applications differing only in span was wrong. The inherited applications are chained, one callback invocation per level with distinct argument and result parameters, so merging them modulo span is a no-op and the measured output did not change. Unifying them would reject callbacks whose parameter is `Any`, and sharing a callee's requirement set by reference would change the scheme representation in the checker, verifier and runtime. The scaling comparison normalizes work by output size, and verifier work stays near a tenth of its bound up to 256 units, so the output size is accepted as inherent to rank-1 copying. No blame span is user-visible.
- 2026-10-28, G4 start: `dev/performance/corpus/erase_sites.py` measures the practical corpus (usage and counts in the evidence page). Twenty of the 23 proc-return `type-mismatch` failures come from an explicit non-Result return annotation that selects the plain value contract. That is source meaning, not missing inference. The frozen views need a source-qualified compiler pair and are not migrated. Open candidate classes by first diagnostic: 22 `ambiguous-overload` parameters not yet checked for genuine ambiguity, 22 `compact.indexed-build` failures (generic argument temporaries without a unique path, incomplete storage contracts, condition control roles), stream return provenance, and two `check.conversion` cases. A `Set[Str]` local from `set.empty()` passed to a generic callee fails verification with "generic call argument has no closed checked type".
- 2026-10-28, generic-body whitelist: over the 4,731 private parameter annotations in `core/*.xsh`, `supports_generalized_body` itself rejects 25 sites and the lowering's "cannot be compiled yet" limits reject 38, against roughly 200 verifier or lowering failures behind the checker's acceptance. Removing the whitelist before those gaps close would turn a clear rejection into an IR verification failure. The proposal is to replace the predicate with the result of lowering the scheme body, closing the verifier classes first. It is a separate item and has not started.
- 2026-10-28, core-wide erasure: 4,225 of 4,731 (89.3%) private parameter annotations in the 250 top-level `core/*.xsh` commands are erasable alone (4,141 before this round of fixes). The 506 failures by first diagnostic are 124 `check.ambiguous-overload`, 53 `check.type-mismatch`, 36 "registered method receiver needs an annotation", 25 whitelist rejections, 21 "generic call argument has no closed checked type", 19 "ground index has no collection contract", 16 `callable generalization failed: Build(RecoveryType)`, 16 "generic body is outside the supported parameter projection", 15 "generic argument temporary is not bound on a unique path", 14 "call argument has no checked type", and a tail of smaller classes. Fixes since the practical-corpus line: `Set`/`Map` empty locals, pattern-capture and comprehension-variable arguments, enum-typed row fields, null-tested optional reads, `exit` at branch or arm tails, loops that cannot break, implicit `Result[Bool]` condition propagation, `is_empty` conditions, slice and comparison arguments.
- 2026-10-28, cross-type overloads: the 124 `ambiguous-overload` sites (`byte_at` on `Str`/`Bytes`, `replace` on `Str`/`Regex`, `get` on `List`/`Map`) are section 9 work, as the decision above records. They are scheduled after the count-order verifier classes and before the whitelist item.
- 2026-10-28, verifier classes closed since the core-wide line: an ordered read of a generic index in a condition, a comprehension variable as a generic call argument, comparison and boolean arguments, `exit` at a value-returning proc tail and at a completion-block tail, and a literal argv list in `process.command_argv` (checker now records its type). Judgment: the 12 `record override result has no independent remainder` sites (`{...state, mode: x}` returning an annotated closed record from an unannotated receiver) are not a verifier gap. Closing the output while the receiver row stays open would let a caller's extra fields leak past the declared type, and closing the receiver would reject callers passing the full record. Both need a row lacks/override constraint, so they stay with the row-override limit recorded for G3 rather than being forced. A later rerun against a stale `src/xsh` archive shows 4,212 passing and a new `unknown-module-api` class from APIs removed on master since the archive; re-baseline from current master before comparing counts.
- 2026-10-28, re-baselined core-wide erasure: both compilers run on one `core/*.xsh` tree archived from `310ec48ab` (251 commands, 4,724 private parameter sites after the DHCP lane's `linux.dhcp_*` migration). Baseline is `d7693cfb1`, the master tip before the first G3 fix; candidate is `310ec48ab`. Baseline erases 3,916 alone, candidate 4,255 (90.1%). All 251 commands check unmodified with the candidate. The baseline compiler fails three commands (`dhcpc`, `ifdown`, `ifup`, 92 of the sites, all stdlib changes that landed after `d7693cfb1`), so the baseline figure is understated by up to those sites and no command regresses under the candidate. Remaining candidate failures by first diagnostic: 124 `ambiguous-overload`, 53 `type-mismatch`, 36 receiver-needs-annotation, 26 whitelist, 21 no-closed-checked-type, 16 `RecoveryType` generalization, 15 temporary-not-bound, 13 generic command options.
- 2026-10-29, cross-type overload requirement (first slice): `byte_at` (`Str`/`Bytes`) and `replace` (`Str`/`Regex`) on an unannotated receiver are retained as a sealed method requirement keyed by the method name. The member set is read from the registry's method table (names registered on more than one receiver) with no copy of the receiver domains in the checker or IR; a concrete use discharges it by selecting the unique member for its receiver and arguments, so a receiver or argument without one still fails. The IR stores the requirement kind, and the verifier rejects a name outside the table, a kind swapped for another name or a family, and a binding to a member the kind does not admit. The type-to-receiver-domain mapping now has one owner shared by requirement selection, the verifier, and registered-method lookup (it covered six domains and missed `Regex`). Two verifier gaps the change exposed are fixed separately: a regex literal as a generic call argument, and a `??` over an Optional method result in a generic body that also declares an empty local (24 sites that previously stopped at the ambiguity). Same-tree rerun: 4,351/4,724 (92.1%) erasable alone, from 4,255; `ambiguous-overload` falls from 124 to 23 (`get` 10, `find` 4, `keys` 3, `push` 3, `lines` 2, `trim` 1). The rest differ in result type per receiver, so they need a result chosen by the selected member (ground returns for `find`/`lines`/`trim`, instantiated returns for `get`/`keys`/`push`); that is the next slice. Observations: after one failed instantiation of a method requirement the next failing call reports the first call's receiver (family requirements behave the same); and `core/lib/syslog_render.xsh:48` fails `xsht check` with `check.redundant-parens` on master, which `xsht check core` surfaces although the 251 commands themselves check.
- 2026-10-29, cross-type overload requirement (result slice): a requirement whose members disagree on the result now keeps a result variable that each concrete use fixes from the selected member, instantiated for the receiver by the registry's builtin-parameter instantiation; arguments are compared against that same instantiation. This covers `find` (`Str`/`Regex`), `trim` (`Str`/`Bytes`), and `get`/`keys`/`push` on the collections. Result-based candidate pruning stays ahead of the requirement, so a call whose result already fixes the receiver still resolves to a ground function (the indirect-anchor test caught the first version deferring it). Same-tree rerun: 4,368/4,724 (92.4%) erasable alone, from 4,255 at the start of the overload work; `ambiguous-overload` is down to 2 `lines` sites, which stay ambiguous on purpose: `Path.lines` reads the filesystem while `Str.lines` and `Bytes.lines` are pure, so the call has no single effect row. Next in count order: 55 `type-mismatch`, 36 receiver-needs-annotation, 28 whitelist. A narrower class this exposed (7 sites, `check.result-fallback`): `a ?? 0` after `return null when a == null` in a generic body fails with `UnsupportedSource(Int)` because the fallback requirement records the null-narrowed payload as its source; it reproduces without any method (`let a = row.n` over `{n: Int?}`), so it is a separate fix.
- 2026-10-29, narrowed fallback, `guard let`, one-arity module overload, single-receiver methods: (1) The 7 `check.result-fallback` sites are not a fallback bug. `a ?? 0` after `return null when a == null` fails because the statement-level early-exit narrowing of a local is applied as a plain type refinement (the intersection of the reaching branches) outside any guard scope, so the guard view leaks and the read has no guard membership: plain `a` after the early exit fails IR verification with "generic body expression disagrees with its checked type", while `if a != null { ... }` and field reads narrow fine. A checker-only change moved the failure to a lowering blocker, so I reverted it; the real fix is to open a guard scope for the continuation after an early exit, which needs a decision on re-registering the guard (its source and comparison spans are already registered by the branch) and is left for its own item. (2) `guard let` over an unresolved subject now binds as an optional like `if let` (4 sites erasable). (3) A standard-API call with an unresolved argument now binds that argument to the only overload its arity allows, so `process.threads(pid)` erases (2 sites); `process.which` over `Str`/`Path` stays ambiguous and would need a module-overload requirement. (4) A method with a single registered receiver (`bit_and`, `bit_or`, `clear_bits`, `float`, `pow`, `sqrt`) is retained by name like the overloaded ones; the registry table now lists every method name and the requirement kind is `Named`. 36 receiver-needs-annotation sites drop to a handful, 29 more sites erase, none regress. Same-tree rerun: 4,404/4,724 (93.2%) erasable alone, from 4,255. The 55 `type-mismatch` sites are mostly purposeful annotations: an `Int?` or `Path?` parameter whose erased form infers the non-null use and then meets a nullable argument, a closed record field set, or an `Error` parameter. They are not inference gaps.
- 2026-10-29, early-exit guard scope: code after `return -1 when at == null`, `if at == null { return }` or `guard at != null else { ... }` now runs in a guard scope that reuses the branch's own guard (the id is read from the path that continues, so no second registration of its spans is made), so a narrowed local read, including a bare-identifier tail, has guard membership; a `??` over a value the guard proves non-null is lowered as the value with its fallback kept as an unreachable branch. A forged-IR test moves the narrowed read before the exit and the verifier rejects it ("guard-local body fact has no dominating non-null source proof"). The 7 `check.result-fallback` sites and 5 more erase; none regress. Same-tree rerun: 4,416/4,724 (93.5%) erasable alone. Correction to the receiver-class line above: that class is 0 sites, not "a handful". 29 sites still stop at the generic-body whitelist.
- 2026-10-29, whitelist and scheme lowering (slice 1, pipeline placeholders): bypassing `supports_generalized_body` showed it hides only 2 of the 29 sites it blocks; the other 26 stop at scheme lowering ("generic expression is outside the supported parameter projection"): `x |> f(_)` placeholders, `spawn`, type-namespaced calls such as `Path.parse_bytes(...)`, fallback blocks, and a few others. The decision is to close those lowering gaps one slice at a time, each deleting its whitelist clause in the same commit, and to delete the predicate when it has no clauses left. Slice 1: a value pipeline placeholder in a generic body now lowers (the input is bound to a local, the hole reads it) and the whitelist accepts it; a forged-IR test moves the input binding after its read. 8 sites erase, 0 regress; 4,424/4,724 (93.6%) erasable alone. 21 sites still stop at the whitelist.
- 2026-10-29, whitelist and scheme lowering (slice 2, spawn): `spawn run ...`, `spawn plan` and `wait` now lower in a generic body (spawn through the command domain scheme `run` uses, wait from its target), the verifier types them from the actual operands, and the whitelist admits them for non-pure bodies. A command-argv list that mixes a literal with a generic value (`["sh", "-c", command]`) is a separate gap ("call argument has no checked type") and stays for the remainder slice. 4 sites erase, 0 regress; 4,428/4,724 (93.7%) erasable alone; 15 sites still stop at the whitelist.
- 2026-10-29, whitelist and scheme lowering (slice 3, type-namespaced calls): `Path.parse_bytes(value)` publishes no registered call plan, so the checker now records it as a checked builtin call (the fact the whitelist already reads) and scheme lowering emits the module call the ground lowering emits. 4 sites erase, 0 regress; 4,432/4,724 (93.8%) erasable alone; 11 sites still stop at the whitelist. Remainder slice: fallback blocks (`?? { |failure| ... }`), `.collect()` on a stream, a `sort-by` stream pipeline, and the mixed command-argv list.
- 2026-10-29, whitelist and scheme lowering (slice 4, stream collect): `.collect()` on a stream receiver in a generic body was excluded in two places, the registered-call executability list (no Stream domain) and the indexed dispatch of registered methods (pure value methods only). The domain is accepted and a collect on a stream value drains it through the evaluator's stream collection. 1 site erases, 0 regress; 4,433/4,724 (93.8%); 9 sites still stop at the whitelist. Remainder, in order: fallback handler blocks, dot-variant constructors, `.context(...)`, the mixed command-argv list, then the three IR verification failures, then the predicate.
- 2026-10-29, whitelist and scheme lowering (slice 5, fallback handler blocks): a `?? { |failure| ... }` handler over a closed result lowers for any source (it needed a wrapper requirement before), and a handler branch ending in `return Err(failure)` is accepted: the whitelist admitted a tail `return` only as the function result and scheme lowering would have made it the branch value, so a branch that ends in `return` now lowers as a value block ending in that statement, typed as the join of its siblings, and the verifier accepts a value block that ends in a return. The native test runs both the missing-path and the returned-failure paths. 2 sites erase, 0 regress; 4,435/4,724 (93.9%); 8 sites still stop at the whitelist.
- 2026-10-29, whitelist and scheme lowering (slice 6, payload variant constructors): a dot-variant call such as `.Once(delay * 1000)` publishes no registered plan; the checker records it in its inferred-variant table, so the whitelist saw an unchecked call. The recorded spans now count as checked operations, scheme lowering builds the tag row from the positional arguments (arity or unsigned-constraint mismatches still stop), and the generic tag verifier now types every payload field against the declared variant layout instead of requiring a nullary tag. A forged variant name with a different field type is rejected. 1 site erases, 0 regress; 4,436/4,724 (93.9%); 6 sites still stop at the whitelist.
- 2026-10-29, whitelist and scheme lowering (slice 7, Result.context): `Result.context(kind, message)` carries its own argument check in the registry, and the registered-call executability test accepted only standard checks, so the call had no plan in a generic body. The test now admits exactly the registered `ResultContext` method (Result receiver, Str parameters, receiver-typed return); the existing generic method verification already types the receiver and arguments, so no verifier change was needed. The native test runs the success and the contextualized failure. 1 site erases, 0 regress; 4,437/4,724 (93.9%); 5 sites still stop at the whitelist.
- 2026-10-29, whitelist and scheme lowering (slice 8, mixed command argv list): `process.command_argv("sh", ["sh", "-c", command], ...)` with a generic `command` had no list type, because the argv checker records a literal list's type only when every item type is equal and a generic word is neither equal to nor unifiable with a literal Str (it may be Str or Path at a call, and a mixed Str/Path list is `List[Any]`, so unifying would reject callers that pass a Path today). A quantified body now types such a literal argv list `List[Any]` as a body fact; the generic list verifier accepts any item under an `Any` item contract, and the command verifier, in both the scheme-body and the ground-producer passes, replaces that type by an elementwise check: every word must satisfy the owner's recorded ProcessWord (or ArgvList for a splice) conversion, resolved through the single immutable declaration of the temporary that holds the list. A forged conversion mask is rejected. The native test runs the same generic proc with a Str and a Path word. 2 sites erase (split `run_filter` and `start_filter_pipe`), 0 regress; 4,439/4,724 (94.0%); 5 sites still stop at the whitelist, 6 in lowering.
- 2026-10-29, whitelist and scheme lowering (slice 8 rework, precise argv list type): the first slice 8 typed a literal argv list `List[Any]` and widened the generic list verifier, which substitutes `Any` for what the checker knows. The quantified body now records `List[Str | Path]`, the argv word contract the command already demands (`process_command_argv_item_type`); the generic list verifier gains one rule, that an item satisfying the owner's recorded ProcessWord conversion matches that exact union (ProcessWord acceptance is defined as matching it), and nothing else is widened. The command verifier's own conversion check is unchanged, and the `Any` stand-in, the elementwise command check and the single-declaration lookup are removed. A caller whose word is not a process word now fails at the call with a specific message (`Int cannot be used here: ... used as a command word, which must be Str or Path`) instead of `inferred types disagree`; the same wording covers every recorded value conversion. A forged conversion mask is rejected by the list rule; a native negative covers the Int caller. Erasure unchanged: 4,439/4,724, 0 regress.
- 2026-10-29, whitelist and scheme lowering (slice 9, the "outside the supported parameter projection path" sites): three causes behind the one message, found by tracing the failing line of each site. (1) A callable whose only requirements are guards (no type, row or effect parameter) is quantified (`CallableScheme::is_quantified` counts guards) and gets scheme body facts, but the checker decided whether to publish registered-call plans with its own parameter-only test, so calls such as `text.find(",")` had no plan; both now use the same predicate. (2) The ground-caller lowering recorded a call instantiation only when the callee had type or row arguments, while the verifier requires one for every call to a scheme-scoped callee, so a ground caller of a guard-only callee failed verification (`generic invocation effects have no call substitution`); it now records every checked invocation. (3) A generic record literal returned under the declared `Record` type (`ErasedRecord`) did not match the return contract, although ground code lets any record flow into it; the body-return match now accepts a record row there. No verifier rule changed. Native tests cover a guard-only callee called from ground code and the erased-record return. 9 sites erase (ifconfig, mdev, nc, numfmt, pinky, stat, curl x2, route), 0 regress; 4,448/4,724 (94.2%); 5 sites still stop at the whitelist, 0 stop in lowering.
- 2026-10-29, whitelist and scheme lowering (slice 10, the three IR verification failures): with the whitelist bypassed, the five remaining sites stop at three distinct defects, plus one shared defect that also blocked sixteen other sites. (1) `source ?? null` over a generic source registered its fallback as a fresh wrapper variable that a null literal never binds, so the sealed fallback relationship (`fallback disagrees with its wrapper relationship`) could not match the body; a null literal fallback is now registered as `Null`, so an instantiation whose element is not optional fails at the call. (2) A binding made by a real match arm pattern (for example a variant payload) was treated as a hoisted temporary when passed to a generic call, giving `generic argument temporary is not bound on a unique path to its use`; the verifier now recognises slots bound by any match-expression arm pattern except the single unguarded bind arm that lowering uses to hoist a temporary. (3) A prepared constant inside a receiver list literal had no checked type fact, so scheme lowering returned without a diagnostic; it now takes the prepared constant's own type and is not counted as a typed expression. Forged-IR negative for (2): redirecting an argument to the slot bound by a different arm is rejected; native tests cover (1) and (2). 16 sites erase (cpio call_error, curl x6, du, find, ifconfig report, mdev build_alias, passwd, ss fail, stty, traceroute x2), 0 regress; 4,464/4,724 (94.5%); 4 sites still stop at the whitelist (cp, install, ss x2), which pass with it bypassed.
- 2026-10-29, whitelist and scheme lowering (slice 11, the predicate is deleted): `supports_generalized_body`, `is_body_expression` and their helpers (about 750 lines), the capture-access classification and checked-operation set that only fed them, and the literal-capture payload they consulted are removed; a generic body is now accepted exactly when scheme lowering and the verifier accept it. The verifier already owned the capture rules (`generic body captures resource state`, `generic record capture has a nonliteral payload`, `generic namespace capture escapes its callable projection`), so the checker no longer duplicates them; the native tests pin those messages. Behavior that changes, all intended: a generic body is no longer rejected at check time for a form it supports, so unannotated receivers whose method families agree (`byte_at`, `trim`, `keys`, `float`) are retained as sealed method requirements and fail at the call that violates them (`has no canonical result for Int`) instead of reporting `AmbiguousOverload` at the definition; a generic proc that calls an unknown-effect `Proc` reports `generic effectful callable requires a known checked effect contract` instead of the whitelist message; a constant payload variant can be captured by a generic body. Pinned-message tests updated: the dynamic `Proc` receiver test in `module_driver.rs`, `tests/xsh/diagnostic-cascades.xsh` (two assertions), the ambiguity pins in `infer_param.rs`, `host_obligations.rs` and `module_driver.rs`, the nullary-variant and namespace-capture pins in `scheme_body.rs`, and the imported-pure read pins in `decl.rs`; the whitelist capture test in `recursive_components.rs` is replaced by one asserting the published read bindings. Open: the verifier capture rejections report at the start of the file under `compact.indexed-build`; a located, specific diagnostic is open work. 4 sites erase (cp, install, ss x2), 0 regress; 4,468/4,724 (94.6%); no site stops at a whitelist or in lowering. Full native suite on the host build: 3,055 pass; the 8 non-passing are 3 environment-dependent failures that also fail on the previous build (getent DNS, env -S config, a descriptor-limit sort) and 5 load timeouts that pass alone.
- 2026-10-29, whitelist and scheme lowering (slice 12, located capture rejections): the three capture rejections the verifier owns now land on the source. The lowering records, for each captured top-level binding, the first read of it inside the function body; the IR keeps that location on the capture row (`FullCapture` grows by one word) and the verifier raises `generic body captures resource state` and `generic namespace capture escapes its callable projection` at it. `generic record capture has a nonliteral payload` carries the same location through the literal-payload obligation. `IrVerifyError` gained an optional span and is boxed, so a verification result stays one word wide (a wider error overflowed the stack of the 128-deep in-place height test). The verifier stays the only authority; the checker adds no classification. Native tests assert line and column for each of the three.
- 2026-10-29, whitelist and scheme lowering (slice 13, one receiver-domain authority): `registered_call_is_executable` kept its own list of method receiver domains next to the evaluator's dispatch over the same domains, so a domain added to one could reach the other's `unreachable!`. The list now lives beside the dispatch (`registered_method_receiver_executes`, including the stream `collect` and `FsRoot` module-call cases that are lowered elsewhere) and the checker asks it. The other clauses of the predicate (native binding, standard argument check plus the two named exceptions, slot layout, registry membership) describe what a plan can mean, not the evaluator, and stay. Not run into by the gates: `builtin_templates::tests::symbolic_registered_plans_match_canonical_ground_method_domains` fails under a plain `cargo test -p xsh --lib` (17 domains against 15 expected) before and after this change; the gate cases do not run it.
- 2026-10-29, G3 evidence (slice 14, metamorphic programs and the independent solver): the independent test-only solver comparison already exists and passes in the sema gate: `crates/xsh-fuzz/tests/support/inference_oracle.rs` (type and row unification with levels, lacks, occurs, rollback, generalization) and `inference_effect_oracle.rs` (finite effect assignments) are included by `src/sema/constraints/reference_tests.rs`, which compares success, resolved outputs and schemes modulo renaming against the production solver on 128 fixed seeds, covers failed-trial rollback and generalization continuations, and carries comparator controls that reject mutated terms (14 tests pass). New: `crates/xsh-fuzz/tests/support/metamorphic.rs`, run by the `test-soundness` gate. Generated programs (40 fixed seeds from the soundness generator) must keep the reference evaluator's output after reversing function order, renaming generator identifiers, and making every pure return type explicit. Hand-built generic programs (relay, method families, callbacks, row fields, fallbacks, effectful relay, defaults, transparent aliases) must keep their output under renaming, declaration reordering, named arguments, helper extraction, explicit defaults and an alias chain. Controls: a rewrite that changes meaning is rejected or changes the output, so the comparison can fail. Writing the reorder showed that a function name is part of a generated call by index, so the reorder works on printed declarations. Not covered, so G3 stays open for them: import-root order, inline versus constant descriptors, and helper extraction that must preserve capture and cleanup.
- 2026-10-29, G3 wording (slice 15, row override): `{n: 0, ...r}` and field assignment on a local copy of an unannotated parameter are still rejected, because a row override needs a lacks constraint. The two messages now say why and what to write: an unknown-type spread after named fields explains that those fields could override fields the compiler cannot see and points to `{...value, field: 1}` or an annotated record type; a field assignment through an unresolved value says the record type must be known and points to `{...value, field: new}`. Spreads of a value that is known not to be a record keep `spread must be a record`. Native tests assert the cause, the fix and the line and column, and that the override-after-spread form runs.
- 2026-10-29, gate catalogue (slice 16, unowned sema unit tests): `test-sema` ran only the `integration` test target filtered by `sema::` (55 tests); the 930 library unit tests under `src/sema` ran in no case, so eleven of them had been failing silently after the whitelist deletion and `symbolic_registered_plans_match_canonical_ground_method_domains` had been failing since the stream `collect` slice (a Stream domain) and the `Result.context` slice (a Result domain) made two more receiver domains executable: its expectation of 15 domains was stale, and now lists 17. A new case `test-sema-unit` runs `-p xsh --lib sema::` (debug, like `test-lowering`; the manual `w9_resource_scope_attribution` test is skipped by name so the case reports complete) and is selected for the same files as `test-sema`. `dev/tests/test-gates.xsh` asserts the selection. Gate harness note: a single Cargo command may name only one test harness, so the unit tests are a separate case rather than a flag on `test-sema`.
- 2026-10-29, G3 closed (slice 17, the three remaining metamorphisms): native tests in `tests/xsh/full-inference-bodies.xsh` compare variants of one program against a stated output. Import-root order: two module roots each exporting a generic function, run with `XSH_MODULE_PATH` in both orders, identical output. Inline against constant descriptors: a generic reader called with an inline row, a local constant and an imported constant gives one output. Helper extraction: a generic proc that captures a module binding and defers a cleanup prints the same lines when the print is moved into a helper proc. With slice 14 this covers alpha-renaming, import-root order, declaration order, transparent aliases, named and default arguments, constant descriptors, helper extraction and explicit versus inferred return types; each test states the expected output, so a variant that diverges fails.
- 2026-10-29, G4 verifier class (slice 18, slice of a mutable list as a generic argument, and located verifier errors): `d(operands[1..])`, where `operands` is a `var` list that is appended to, passed to a generic callee failed with `generic call argument has no closed checked type`: the ground argument derivation handled the mutable local on its own but not a slice of it. `derived_argument_type` now derives a slice from its base (List, Str or Bytes), so the slice is proved by the same writer-by-writer storage derivation as the local. A first attempt trusted the slot type fact for every multiply-written slot; the existing forged-IR test that redirects a write to a guard binding rejected that (it would have accepted a write of the wrong type), so the fact is not trusted and the derivation stays the authority. The verifier also records what it was verifying (function, or call instruction) in a thread-local scope, and a verification error without its own location now reports at the enclosing function instead of the start of the file. 6 sites erase (arp x2, expr x2, factor, sort), 0 regress; 4,474/4,724 (94.7%). Remaining verifier classes seen by argument form when erased: a conditional expression as a generic call argument (chrt, pr, shred, udevadm), a destructured or mutable scalar local (numfmt, tree, nc, patch), and a field of a loop variable (patch).
- 2026-10-29, G4 verifier classes (slice 19, conditional and destructured arguments): two more argument forms reached the generic-call argument check without a derivable type. A conditional expression whose branches include a loop variable (`show(if flag { target } else { 0 }, "new")`) is now derived by deriving each branch (the checked type of a literal or the writer-derived type of a binding) and requiring them to agree; a record-destructured binding (`let {value: whole, ..} = p`, `for {value: v} in items`) is now accepted by the argument derivation, which projects the field from the source record through the same destructuring machinery the ground checker uses. Forged-IR negatives: a branch swapped for a literal of another type, and a destructuring source swapped for a list, are rejected. Native tests run both forms with several argument types. 9 sites erase (chrt, pr, shred, udevadm, nc, numfmt, patch x2, tree), 0 regress; 4,483/4,724 (94.9%).
- 2026-10-29, G4 verifier classes (slice 20, command environment and record override remainders): (1) `process.command_argv(..., env: record)` where a field of the record is a generic value failed `generic command options disagree with their native contract`: the option parameter is `Union[ErasedRecord, Map[Str, Bytes]]`, and the command verifier required a closed ground type for every option. Options are now matched with the body-type rule that already accepts a record row for a record expectation, extended with the two ground rules it lacked: any record row satisfies `ErasedRecord`, and a type satisfies a union when it satisfies a member. (2) `{...state, count: state.count + 1}` over an unannotated record failed `record override result has no independent remainder`: the verifier demanded a fresh row remainder for every override. It now accepts the same remainder when every overridden field is already a known field of the receiver (a known field is lacked by the remainder), and a closed result view over exactly the derived known fields when the result is declared with a closed record type (the extras stay in the value, as for a wider record passed to a narrower annotation). Forged-IR negatives: the environment record swapped for a generic parameter, and a new-field override given the receiver's remainder, are rejected. Native tests run both. 16 sites erase (ifdown x6, ifup x6, nc port, split x2, uniq), 0 regress; 4,499/4,724 (95.2%).
- 2026-10-29, G4 verifier class (slice 21, complete storage contracts): a typed local initialised in a generic body is stored through a checked value whose runtime check must be complete. The completeness rule treated `Any` as incomplete everywhere and consulted the source type only for an identical source, so `var block: List[Member] = [...]` (a record with an `Any` field) and a nested `null` for an `Optional` field failed `generic checked value is not a complete storage contract`. The rule now consults the closed source type position by position: an `Any` position is complete when the source position is already `Any` (nothing is lost), an `Optional` position is complete when the source position is `Null`, and the existing `Tag` rule is unchanged. The unchanged negatives (a Set retargeted to another element type) still reject. Native test covers the dynamic-field list and the null optional. 8 sites erase (mdev rules, smartctl x7), 0 regress; 4,507/4,724 (95.4%). Gates: all eight cases including `test-sema-unit` pass; `test-sema` and `test-sema-unit` also pass on the previous master (7544cb16d). `json_document` with `options` erased stays a justified exception for now: calling `j.object` with a `List[Member]` argument from a generic caller needs a call substitution containing `Any`, which the verifier forbids (the successor requirement keeps schemes free of `Any` substitution); to be recorded on the evidence page.
- 2026-10-29, G4 checker class (slice 22, splice of an unresolved value): `[@ancestors, key]` over an unannotated parameter reported `list literal splice requires List` because only a resolved list type was accepted. An unresolved spliced value now gets a `List` of a fresh item (as the process argv checker already did), so the parameter becomes a list-shaped parameter and a caller that passes a non-list fails at the call. 9 sites erase (chmod, cpio x2, diff, fmt, iw, smartctl argv, tar, xargs), 0 regress; 4,516/4,724 (95.6%). All eight gate cases pass.
- 2026-10-29, G4 and two owner-reported defects (slice 23): (1) `proc v(n: Int) [fs, error] { let n = 2; p"x".mkdir() }` and `pure v(n: Int) { let n = 2; n }` were rejected as `check.duplicate-name`: the body of a callable whose return is inferred was checked in the parameters' scope, while an annotated one gets a block scope. The inferring check now runs in its own scope (new `sema::` unit tests, including that a true duplicate in the body is still rejected). (2) `proc probe(target: Path?) { let _ = target?.lines() }` failed `checked method has no exact receiver type`: the temporary for the unwrapped receiver of a null-safe call had no checked slot type, and the inner call carried the optional type of the whole expression; the temporary now has the payload type and the guarded inner call no longer records the outer optional type. The xsht `path_method_lowering_tests::every_registry_path_method_lowers` and the native tests pass again. (3) A fallback handler that cannot complete (`?? { |_| exit 255 }`) in a generic body failed `callable generalization failed: Build(RecoveryType)`: the handler block's type is `Unknown` (it is also the divergence witness the body-contract code reads, so it stays), but the body fact now takes the join of the fallback; lowering builds the handler as a diverging branch block and the verifier accepts a block that ends in an `Abort`. Forged-IR negative: the abort swapped for a literal is rejected. 12 sites erase (arp, cpio x5, ifconfig, sort x4, tar x2), 0 regress; 4,528/4,724 (95.8%). Gates: the eight cases plus `test-tooling-unit` all exit 0. One new lowering stop: an inferred error variant (`fail .Fatal("...")`) in a generic body (cpio `copy_in`), open.
- 2026-10-29, G4 checker panic (slice 24): `let end = make(items, 1).e` where `make` is a generic callee aborted `xsht check` with `a call instantiation is recorded once`. `check_generic_module_alias_initializer` checked the base of a `let x = base.field` initializer to learn whether it names a module, and `check_field_arena` then checked it again, so a generic call as the base recorded its instantiation twice. The probe now runs only when the base is a module path (identifier or field chain). New `sema::` unit test (fails without the fix with the panic) and a native test. Master's `test-lowering` was red after the archive consolidation (`instruction_schema_preserves_all_tag_ordinals_and_effects` still expected the removed `ExprArchiveTarCreate`, 154 tags); fixed as its own commit. Four sites erase (ptx `trim_span` x2, shuf `wide_add` x2), 0 regress outside `core/tar.xsh`; The measurement snapshot's `core/tar.xsh` called the replaced `archive.*` API, so the first re-measure failed all of its sites; the snapshot now carries the migrated `core/tar.xsh` with its 97 parameter sites regenerated, and the full corpus is 4,533/4,725 (95.9%), tar included (95 of its 97 erase). Also adds the 196-site classification to the evidence page. Gates: the eight cases plus `test-tooling-unit` all exit 0.
- 2026-10-29, G4 inferred error variants (slice 25): `fail .Fatal("...")` in a generic body stopped lowering with `generic expression is outside the supported parameter projection path`, because the scheme call arm lowered only an inferred enum `Tag`. The explicit `Family.Variant(args)` constructor already had a scheme path; it is now one method, and the inferred error variant (family and variant from the checker's `InferredVariant::Error`) goes through it with scheme-lowered arguments. No verifier change. Native test fails before (`cannot be compiled yet`) and passes after. 2 sites erase (cpio `copy_in` patterns, batch_names), 0 regress; 4,535/4,725 (96.0%); no site stops at lowering any more. Gates: the eight cases plus `test-tooling-unit` all exit 0.
- 2026-10-29, G4 record override into a closed result (slice 26): `pure mark(state) -> Run { {...state, failed: true} }` was rejected by the verifier with `record override result has no independent remainder`. The verifier accepted an open receiver row with a closed result only when the result held exactly the derived fields. A closed result with more fields takes them from the receiver, and the call site already proves that: its row argument is the actual argument's extras, and the selection check compares the receiver with the overrides applied against the instantiated result (a wider actual still fails there with `sealed operation result disagrees with its input signature`). The arm no longer requires the equal count. Forged-IR negative: the unmodified fixture verifies, and renaming the call's row-argument field is rejected. Native test for the runtime values. 12 sites erase (cpio x4, expr, ls, more, sort, split, stty, tail x2), 0 regress; 4,547/4,725 (96.2%). Gates: the eight cases plus `test-tooling-unit` all exit 0. Open: a wider actual argument (`mark({..., extra: 5})`) passes the checker and is refused only by the verifier, so its diagnostic is a verification message and not a type error.
- 2026-10-29, G4 record override width (slice 27): the open gap recorded in slice 26 (`mark({..., extra: 5})` accepted by the checker and refused by the verifier) is a verifier defect, not a checker one. SPEC section on records says they are width-compatible, and `take(wide)` against a `Run` parameter already runs; the checker's constraint `Run` against the receiver with the overrides applied is compatible by width, so the checker is right to accept. The verifier's selection check compared the receiver with the overrides applied for equality with the result. It now requires every field of the closed result to be present with the same type in the selection, so extra fields fit and a missing field is still refused. Forged-IR: the wide fixture verifies, and a required result field renamed out of the call's row argument is rejected. Native test runs the wide call. No site changes; gates: the eight cases plus `test-tooling-unit` all exit 0.
- 2026-10-29, G4 generic call argument, record literal (slice 28): a record literal passed to a generic callee with a computed field (`text_of({neg: a.neg != b.neg, mag: a.mag})`) failed verification with `generic call argument has no closed checked type`. `derived_argument_type` returned at once for any expression that is not arithmetic or a comprehension and has no binding path, which includes every record literal the checker had no argument fact for; and the record derivation read each field only from recorded contexts, which comparison and boolean operators do not have. A record literal now takes the derivation path, and a field with no recorded context is derived from its own form. Forged-IR: the fixture verifies, and the comparison swapped for an addition (no derivable type) is rejected. Native test. 3 sites erase (expr `whole_text`, mdev `create_device`, patch `apply_file`), 0 regress; 4,550/4,725 (96.3%). Gates: the eight cases plus `test-tooling-unit` all exit 0. Remaining in this class: fmt `layout_buffer`, join `join_line`, lsipc `row_for` x2.
- 2026-10-29, G4 proc completion without a value (slice 29): `proc v(argv) { let raw = argv[0] }` failed `callable generalization failed: Build(UnresolvedWrapper)`. Indexing an unresolved receiver in a proc registers a key-validation wrapper whose boundary is the proc's completion group (the receiver may be a `Map[UInt, _]`), and the completion group was finished only for procs that may return a value, so a proc with no value return left it `Pending` at generalization. After the first body check, a proc whose body created a completion group now takes the same inference path as a value-returning proc (`infer_proc_return`), which finishes the group and gives the scheme its `Result[Unit, E]` envelope; a proc without a group is unchanged. Behavior: with a list the proc completes, with `Map[UInt, Str]` and a negative key it fails with the schema error (native test, both). 5 sites erase (df and du `validate_block_sizes`, chroot and setpriv `launch`, lsipc `missing_resource`), 0 regress; 4,555/4,725 (96.4%). Gates: the eight cases plus `test-tooling-unit` all exit 0. Seven `UnresolvedWrapper` sites remain with another cause (arp, cp, dd, ethtool x2, patch x2). Open defect found while reducing them: a no-value proc that has a non-tail statement call to an `[error]` proc and also indexes an unresolved receiver (`proc v(items) [io, error] { say("a"); let raw = items[0]; print $raw }`) now generalizes but fails verification with `proc completion contributions differ: actual 2 [PropagatedError, KeyValidation], expected 1 [KeyValidation]`: the return inference records no statement propagation for that body (`propagated 0`), while the same body with a value tail records it. Cause not yet found; the patch `announce` sites hit it.
- 2026-10-29, G4 proc completion with propagation, and empty list branches (slice 30): the defect recorded after slice 29 was that slice 29's second return inference was a no-op. The first, non-inferring body check marks the body checked, so `infer_proc_return` re-entered `check_function_arena`, returned at once, and finished the completion group from no candidates and no propagations (`propagated 0`); the lowering then carried the statement `PropagatedError` the checker's group lacked. A no-value proc whose first check left a completion group and no error now reopens its body and runs `infer_proc_return` for real, with the pending call instantiations set aside for that second check. Two gaps in that path surfaced and are fixed: the group's `Result[Ok, E]` was never constrained to the proc's own return (`RecoveryType`), and a propagated error was compared with an unresolved error variable by shape instead of unified. Separate root cause found on the way (`pure v(flag, x) { let a = if flag { [x] } else { [] }; a.len() }` failed `RecoveryType` in any generic body): the empty list literal in a branch of an `if` expression keeps the type `List[Unknown]` as a body fact, since only the join receives the other branch's item type; the body fact now takes the join's type. The tail `if` statement form still fails the same way and is open. An experiment that routed every proc through the inference path was rejected: four existing tests failed (annotation facts of exported procs, cleanup error joins, a retry contract), so the reopen is limited to procs with a group. Native tests for both; 6 sites erase (cp `finish_parents`, ethtool x3, patch x2), 0 regress; 4,561/4,725 (96.5%). Gates: the eight cases plus `test-tooling-unit` all exit 0.
- 2026-10-30, G4 conditional generic call argument (slice 31): join `join_line(if side == 0 { row } else { [] }, ..., layout)` failed `generic call argument has no closed checked type`; the erased parameter is only the last argument, the failing one is the first. The conditional argument's arms are value blocks, and the derivation read each arm as an expression without peeling the block, and an empty list arm has no type of its own. Arms are now read through their block tail, and an empty list arm takes the type the other arms agree on (a non-list join is still refused). Forged-IR: the fixture verifies; the empty arm replaced by a string literal is rejected. Native test. 1 site erases (join `join_line`), 0 regress; 4,562/4,725 (96.6%). The `var layout_out = layout` shape I suspected from the site text was not the cause (the same program with a `let` failed identically). Gates: the eight cases plus `test-tooling-unit` all exit 0.
- 2026-10-30, G4 structurally typed generic call arguments (slice 32): `lay(first.byte_len(), goal)` with a generic `lay` failed `generic call argument has no closed checked type`: `derived_argument_type` demanded a binding path for any form other than arithmetic, comprehension and record literal, and a byte length or string predicate has none although its type is fixed by its form. `StrByteLen` and `StrPredicate` now take the derivation path. Forged-IR: the fixture verifies; the byte length replaced by a string literal is rejected. Native test. 2 sites erase (fmt `layout_buffer`, dd `render_operand_error`), 0 regress; 4,564/4,725 (96.6%). Gates: the eight cases plus `test-tooling-unit` all exit 0. Remaining in the generic-argument class: lsipc `row_for` x2 (`{values: values, elements: []}`: the empty list field gets its type only from the callee parameter, which the derivation does not see).
- 2026-10-30, G4 compound assignment over an unresolved local (slice 33): `pure f(a) { var n = a; n /= 2; n }` failed `/= is not defined for <type needs an annotation>` while `n = n / 2` was accepted. A compound assignment to a plain local whose type is still a variable now registers the same sealed binary operation a binary expression does (keyed by the statement, with the right operand's expression as its fact key); it is not registered for a list append, which already had its rule, nor for field or index targets. The scheme lowering writes it as `Assign{Set}` of a `Binary` whose left operand is the local read and which carries the operation, so the existing binary-operation proof authenticates it; no new verifier rule and no desugaring in the arena: an index target keeps `Assign{op}` and its single evaluation, and stays unsupported over a type parameter. The owner pinned the old limitation in `list_append_preserves_validation_and_numeric_ambiguity` (`output *= 2` on an unresolved local is an operator error); that expectation is replaced: the form is accepted, runs for Int and Duration, and an ill-typed `output *= "x"` is still `check.operator-type`. Forged-IR: the fixture verifies; the operator changed to another is rejected and a removed operation fact is rejected. Native test for Int, Float, Str and List. 7 sites erase (cat, ls, lsipc, od, patch, pr, shuf), 0 regress; 4,571/4,725 (96.7%). Gates now run as two combined runs: A (`test-lowering`, `test-sema`, the bodies and checker native files) and B (`test-sema-unit`, `test-tooling-unit`, `test-type-agreement`, `test-runtime`, `lowering-coverage`); all nine exit 0, 10 minutes with a cold compile.
- 2026-10-30, G4 reductions of the largest remaining verifier refusals (no code change): (1) btop `limited_observations` and its four siblings: `pure f(o: List[Int], count) -> Bool { count == null or count >= o.len() }` verifies the second read of `count` as `Int` (the checker's per-occurrence fact, pinned by the ordering with an `Int`) against the parameter slot `T`, so the generic body expression disagrees; the null test refines the occurrence but not the parameter, which should become `Int?`. A checker decision about null-tested parameters, not a verifier gap. (2) dd `write_fd_all` and three siblings: `proc w(fd: Int, data) [process, net, error] { var n = 0; while n < 3 { let written = unix.write_fd(fd, data[1..])?; n += written } }` fails `ground slice has no collection contract`; the same body with `n += 1`, or without the loop, verifies, so the ground producer proof that chases the compound assignment's writer into the `let` initializer reaches the slice base without its parameter context. Instrumented: the owner is the scheme function (parameter `data` is a type parameter, `ty None` for its header read), and the slice base is the argument temporary slot copied from it, so the ground proof asks for a collection contract of a type parameter; the chase from `n += written` should stay on the scheme proof. Open.
- 2026-10-30, G4 ground slot proofs for generic owners (slice 34): `proc w(fd: Int, data) [process, net, error] { var n = 0; while n < 3 { let written = unix.write_fd(fd, data[1..])?; n += written } }` failed `ground slice has no collection contract`. The ground producer proof queues a slot obligation for a local whose writers it has to prove, and that obligation walked the writer `n += written` back into the `let` initializer and into `data[1..]`, whose base is a type parameter of the scheme function. A scheme function's locals are proved by the scheme body proof (`assignment_replacement` checks every writer against the declaration), so the ground slot obligation is not queued for a scheme owner. Instrumented first (the owner was the scheme function, `ty None` for the type-parameter read). The same cause held for eleven more sites. Forged-IR: the fixture verifies, and the counter's value replaced by a string literal is rejected by the scheme proof. Native test. 12 sites erase (dd and mdev `write_fd_all`, expr x2, ls `url_escape`, lsns, nsenter, sort x2, split, tsort, wget), 0 regress; 4,583/4,725 (97.0%). Gates: both combined runs exit 0.

## For the owner afterwards

- Read the exact-source completion report and contract changes in the active handoff.
- Review purposeful remaining annotations, joint behavior coverage, and performance tradeoffs.
- Integrate only reviewed commits from the named campaign branch within the execution grant; never copy an entire historical inference branch.
- Treat unresolved mandatory work, deferred performance gates, or mismatched source acceptance as incomplete. No owner review is needed to decide that result.
