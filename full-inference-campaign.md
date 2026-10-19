# XSH: complete general inference across the language

Status: execution-ready plan, reviewed on 2026-10-19 against local master
`d489d8fcf`. Planning does not start this work or authorize its effects.

Deliver these results in order:

1. One rank-1 type, row, and effect authority for pure, proc, and stream declarations.
2. Ordinary bodies, recursion, safe callable composition, and prepared execution with no runtime solver.
3. Every capability and safety case below, including independent exports and delayed effects.
4. Joint annotation migration and behavioral comparison on the frozen and practical corpora.
5. Qualified cold, memory, runtime, and scaling results, followed by removal of superseded machinery.

## Execute within this charter

Use this executable core for sequencing. The numbered completion specification
below remains mandatory. Its G0–G6 checkpoints map to the work items here;
an intermediate checkpoint never waives a later requirement.
Those are inference checkpoints. The active performance campaign's inherited
G1 baseline gate and G6 deployment gate retain their own scopes and results.

The coordinator owns integration, representation contracts, acceptance, and
the existing progress owner in `docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md`.
Follow the installed `orchestrate` skill and its campaign reference for
lane contracts, integration, and worktree lifecycle. Do not create another
campaign tracker. Add only decisions and phase results a future reader needs
to that progress owner during execution. This document specifies the work.

Decide inside the bounds below, record the reason, and continue. Park only the
affected item when authority or a required input is unavailable. A parked
mandatory item means the campaign remains incomplete. Technical difficulty,
unimplemented inference, and three unsuccessful experiments are reasons to
reassign diagnosis, not reasons to waive the deliverable.

Keep at least four lanes active when four disjoint assignments are ready,
with a ceiling of six. When fewer are ready, use every disjoint assignment
and have the coordinator resolve their dependencies. Never create competing
signature redesigns to fill slots. The core compiler owner takes the coupled
scheme/checker/lowerer/verifier changes end to end. Other lanes own independent
fixture files, corpus tooling, diagnostics consumers, or loop repairs. A lane
must receive explicit files before writing; the coordinator owns shared test
registrations, generated output, manifests, and lockfiles.

Use `.work/wt/full-inference-<lane>` and scratch under that checkout's
`.work/tmp`. Confirm `.work/` is ignored in `.git/info/exclude` before writing.
Keep a 12 GiB host reserve and never exceed 32 assigned CPUs. Admit each launch
with `perf-gate-ready -s`. Preserve the active campaign's allocations:

| Work | CPU/jobs limit | Memory limit | Concurrency |
|---|---|---|---|
| Ordinary compiler build | 2 CPUs, 2 jobs | 8 GiB | At most two, with disjoint Cargo outputs and available leases |
| Lightweight focused check | 2 CPUs | 4 GiB | Up to the admitted disjoint lane count |
| Inherited final/release build or native observer | 12 CPUs, 12 build jobs | 24 GiB | Alone |
| Inherited scheduler pair | 32 CPUs | 40 GiB | Alone |

Use observed peaks to reduce width, never to override these gate allocations
or the host reserve. Park a gate whose required allocation does not fit.
Use existing common Cargo/native/performance leases through
`dev/util-linux/leases.py`; serialize builds sharing Cargo state and run one
full native suite at a time. A timed window excludes competing work.

Replace or resume a stopped lane in the same coordinating turn. Integrate
accepted slices while the next disjoint tranche runs. Verify immutable merged
sources in a scratch worktree, so later integration cannot alter a running
check. Split verification only across disjoint inputs and output directories.
Poll owned jobs at least every 60 seconds while active, consume completions
immediately, and inspect every lane at least every 15 minutes. Do not end a
coordinating turn with an unattended job and no active completion owner.
F0 must also enforce completion ownership in the existing launcher path:
retain the child handle until status and cleanup are consumed, and chain a
dependent stage only from an observed successful terminal result. Reproduce
the past mistake with a test child that finishes before the next observation;
the owner must consume it and advance or fail, never leave it recorded as
running. Test failure and interruption too. Use the existing process/launcher
tests, not a new polling service or durable job registry. A prose polling rule
alone does not complete F0.

Repair the loop before repeating a check that takes over two minutes for a
single semantic repro, or over five minutes for an incremental focused check.
Target 30 seconds for a prepared repro and two minutes for its focused group,
excluding the first necessary build. An unchanged workload over twice its P0
CPU or memory baseline is egregious and gets a bounded repair immediately.
Ordinary performance tuning waits until functional and parity completion.
Measure the smaller budgets in section 12 during development; gate them at P4.
Memory limits, correct cleanup, and fail-closed resource exhaustion always apply.

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

Current source already has quantified type and row parameters, open rows,
rollback-safe union-find, registered-call evidence, private proc value returns,
and generic pure identity/projection with immutable aliases. It still rejects
quantified proc/stream schemes, typed storage of generic callables, ordinary
recursive inferred returns, and generic bodies outside a narrow classifier.
Effects are concrete graph summaries rather than symbolic callback relations.
The verified code map appears at the end of this document.

Frozen combined source `55032a7ee2c4d340a6e5df9d4f362b0074d8a9e0`
passes canonical correctness, including 14,077 native passes, zero failures,
and 150 skips. Its supplemental warning-denied Clippy check fails with 53
errors. Later supplemental stages and functional/baseline close do not pass
by implication. Preserve the actual failed evidence under
`.work/final-review-evidence/final-correctness-gates/correctness-i1-r12-55032a7ee`.
P0 reproduces and repairs that warning boundary before another full run.

Two real pinned-image probes passed during planning using the frozen R12
canonical products, mounted read-only, with 2 CPUs, 4 GiB, no network, and a
30-second deadline:

| Exact native selector | Result | Observed outer elapsed time |
|---|---|---|
| `tests/xsh/typing-capability-witnesses.xsh::test_typing_identity_and_open_record_projection` | 1 pass, 0 failures, 0 skips | 0.148 s |
| `tests/xsh/rank1-inference.xsh::rank1_generic_function_cannot_be_stored_as_typed_callable` | 1 pass, 0 failures, 0 skips; the old rejection remains | 0.147 s |

These single observations establish a cheap executable foundation and a real
frontier. They are not performance baselines or proof of current-master full
qualification. No full suite or build was run during preflight.

Use these two loop speeds during execution:

```sh
# Run inside the pinned environment with matching verification products.
"$XSHT_BIN" test --exact "$SELECTOR"
cargo test --locked --offline --target x86_64-unknown-linux-musl \
  -p xsh --lib "$UNIT_FILTER" -- --exact

# Integrator: changed-area checks after a coherent slice.
cargo dev gate PATH...
```

Use debug only for non-process Rust units. Process/native tests need matching
verification or release products and `XSH_BIN`/`XSHT_BIN` bindings. Discover
selectors with `xsht test --list FILTER` and require exactly the intended
nonempty set. An exact positive must report one pass and zero skips. Inspect
the generated gate plan in `dev/gates.xsh`: the broad checker mapping does not
select every inference-specific native file. Explicitly pass those paths too.

Before any full candidate freeze run the warning boundary using the existing
pinned launcher and Cargo lease:

```sh
dev/consolidation/winddown/gate-tools/final-correctness-gates/run-native-final.sh \
  "$SOURCE" lint cargo clippy --locked --offline --workspace \
  --target x86_64-unknown-linux-musl --all-targets --all-features --quiet -- -D warnings
```

Bind the launcher's source, output, Laputa corpus, and resource environment
through its existing owner, rather than borrowing another source's target.
Run the focused changed crate first; the displayed workspace command is the
pre-freeze integration boundary, not the per-edit loop. No autofix or formatter.

## Sequence and ownership

An item ends on its focused proofs. A phase ends on its touched-area checks.
Only P2 and P5 run the full integration boundary, for complete semantic-path
coverage and final combined qualification respectively. Re-run it earlier
only for a concrete integration risk that focused tests cannot cover.

| Phase/item; size | Prerequisites | Owned files and deliverable | Acceptance proof |
|---|---|---|---|
| P0/F0 ground and loop; M | Launch grant | Coordinator: existing progress owner. Loop lane: `dev/gates.xsh`, relevant `dev/tests`, existing launchers only as needed. Freeze sources/products, baseline reds, commands, and corpus inputs. | Both foundation probes; minimal reds for each frontier; warning boundary repaired and checked; measure focused loop and show every selected test. No receipt credit from older sources. |
| P0/F1 freeze witnesses; M | F0 source freeze; overlaps loop repair | Fixture lane: new inference fixtures and separate native test modules. Corpus lane: `dev/performance/corpus/` and copied source views. No compiler edits. | Map every section 13 row and every section 4 node family to named positive/negative witnesses. Freeze changed-path coverage, classification, operation/effect snapshots, and paired behavior selection before removals. |
| P1/F2 canonical path; L | F0; F1 supplies witnesses continuously | One compiler owner: `src/sema/constraints.rs`, `schemes.rs`, checker, lowering, indexed schema/verifier/execution. Add nested callable monotypes, effects, SCC inference, and prepared evidence through one path. Coordinate SPEC, snippet and grammar updates in the same accepted slice. | G1 then G2: imported generic value → higher-order proc → generic Result → producer → post-disposal execution; own negative controls, unchanged existing controls, matching specification, and compiling mainline. |
| P2/F3 complete the language path; L | F2 contract; F1 matrix | Same compiler owner: remaining checker/elaboration/runtime files. Disjoint fixture and diagnostic lanes consume its published contract; no parallel signature redesign. Land each behavior change with its SPEC, snippet and grammar updates. | G3: every matrix row, existing expression/statement family, all delayed callable positions, rank-1 captures/recursion, grounded operations, and normal/forced disposal routes pass. Full changed-compiler integration boundary once, including grammar generators when affected. |
| P2/F4 complete corpus parity; L | F2 for probes; final reductions require F3 | Corpus lane: views, classification and existing cohort adapters. Compiler causes return to F3 owner. Tooling lane: `crates/xsht/` consumers with coordinator-assigned files. | G4: original/reduced historical eleven roots and practical roots check, lower, execute selected affected paths, compare operations/effects/interfaces, and reject deliberate mutations. Every retained in-scope missing relationship resolved. |
| P3/F5 harden; M | F3 and F4 | Compiler owner: remaining safety/error defects. Tooling/docs lane: remaining documentation, scheme rendering and safe fixes. Coordinator generates derived docs for each accepted slice, not only at F5. | Section 13 metamorphisms, independent test-only solver comparison, valid-before-invalid IR tests, safe-fix idempotence, diagnostic specificity, affected documentation checks. No missing mandatory positive or negative. |
| P4/F6 measure and optimize; L | F5 functional and parity completion | Measurement lane: existing `dev/performance/`, benchmark fixtures and counters. Compiler owner fixes measured mechanisms only. | G5: all section 12 budgets, deterministic scaling below, build/size gates from the active campaign, exact-output controls, readiness before/after each timed window. Record tradeoffs; no moving tolerances or benchmark denominator. |
| P5/F7 close; M | F6; inherited same-source qualification | Coordinator plus independent reviewer. Delete proved-dead paths, update canonical docs and existing handoff, integrate reviewed local changes only within grant. | G6, final independent adversarial review with no P1/P2 open, full same-source canonical/supplemental/functional and inherited baseline acceptance; clean owned-process shutdown and worktree closeout. Mandatory parked items mean incomplete, never successful close. |

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
each existing row in the active progress owner; add fixture tests in ordinary
native modules, not a new campaign runner. Existing rejection tests such as
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

Reuse the active campaign's benchmark owners and source/product qualification.
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
active campaign's normal profile, timed observer, forced, comparative,
scheduler, util-linux resource/private-namespace/close prerequisites. Use the
real independent-review context for baseline close; the bare CLI does not
manufacture it. Never use the unavailable `inference`, `optimize`, `deployment`,
or `close` adapter commands as acceptance. Inspect the current CLI contracts
instead of copying an old invocation. Preserve failed attempts and resolved
cleanup; a changed compiler needs new same-source receipts.

## Execution authority and exclusions

This preflight authorizes only editing this draft and bounded foundation checks.
It authorizes no implementation, commits, merging, acquisition, or deployment.
A later launch grant must explicitly cover isolated implementation and any local
commits/integration. Within that grant, conventional commit subjects name the
area; item IDs belong in the body. Mark contract changes as breaking where
applicable and surface the omission/export/callable/elaboration changes in the
final report. Never push.

Keep execution offline using already available pinned inputs. If an input is
missing, park its dependent item instead of fetching moving versions. Do not
edit ordinary sibling Laputa, live roots, host packages, CI, Rust toolchain
pins, kernels, or release artifacts. This plan adds no KVM, privilege, or
network requirement; deployment effects remain with their separately authorized
campaign owner. Preserve the active deployment and post-consolidation cleanup
obligations rather than claiming this inference plan closes them.

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

Read `AGENTS.md`, `docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md`, `docs/PERFORMANCE-INFERENCE.md`, `docs/ARCHITECTURE.md`, `docs/TESTING.md`, the relevant SPEC templates, and the existing inference design and capability tests. Incorporate this completion contract into the active campaign and its existing progress owner; do not reopen the retired consolidation campaign or create another orchestration/provenance system. Preserve historical evidence and accounting.

Follow the current pinned x86_64 Linux musl environment, toolchain, build profiles, resource admission, isolated-worktree policy, and source/product qualification. Do not revive obsolete macOS/ARM commands. Keep ordinary Laputa checkouts and live systems untouched; use the campaign's approved isolated source views and local fixtures. No pushing, publishing, new kernel work, or release upload. Lean stays in its current maintenance scope; full theorem development is not a prerequisite or a parallel expansion of this task.

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
| Current generic-body classifier and checked body facts | [scheme_body.rs](src/sema/check/scheme_body.rs) |
| Callable signature fitting | [typed_callable.rs](src/sema/check/typed_callable.rs) |
| Effect graph and concrete fixed point | [infer_effects.rs](src/sema/check/infer_effects.rs) |
| Module-level scheme and capture checks | [module_driver.rs](src/sema/check/module_driver.rs) |
| Indexed lowering, independent verifier and execution | [lower.rs](src/runtime/eval/lower.rs), [full.rs](src/runtime/eval/indexed/full.rs), `src/runtime/eval/indexed/full/`, `src/runtime/eval/lowered_run/indexed_run/` |
| Canonical language text and generated outputs | [SPEC template](docs/templates/SPEC.md), [testing procedures](docs/TESTING.md) |
| Historical inference design and frozen corpus design | [first slice](dev/consolidation/designs/inference.md), [cohort](dev/consolidation/designs/typing-cohort.md) |
| Accepted source views, reproduction, retained classes and practical adapters | [inference audit](docs/PERFORMANCE-INFERENCE.md) |
| Active campaign, inherited gates and deployment obligations | [performance/deployment campaign](docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md) |

## Decision and campaign logs

Use the existing append-only decision and phase log in
`docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md`. Add one line per judgment a reader
would otherwise miss and one line per completed phase, with the source and
actual result. This document's prescribed decisions are the starting rules.
Do not maintain a second progress log here. A stale completion status is a
defect for the coordinator to fix when consuming the result.

## For the owner afterwards

- Read the exact-source completion report and contract changes in the active handoff.
- Review purposeful remaining annotations, joint behavior coverage, and performance tradeoffs.
- Integrate only reviewed commits from the named campaign branch within the execution grant; never copy an entire historical inference branch.
- Treat unresolved mandatory work, deferred performance gates, or mismatched source acceptance as incomplete. No owner review is needed to decide that result.
