# XSH: unified type-and-effect inference execution campaign

User amendment, 2026-09-30: quiet product timing and exhaustive benchmarking are removed. Use lightweight regression measurements while prioritizing compiler implementation. Semantic, annotation-reduction, runtime, resource, and deterministic complexity contracts remain required.

Deliver a production compiler change, scoped source migration, and measured verification. Completion requires annotation-light real XSH programs executing through the normal verified indexed runtime, with the semantic, annotation-reduction, performance, and consolidation gates below satisfied.

This document is the execution contract for one resumable campaign. Follow the architecture and decisions here. Resolve routine implementation choices from the settled repository, its canonical specifications, and the evidence gathered during the campaign. Do not silently choose a weaker type system, postpone required runtime integration, or declare completion after implementing a solver kernel.

## 1. Mission, decisions, and boundaries

Replace piecemeal reconstruction with bidirectional constraint generation over a compact shared type graph, rank-1 polymorphic schemes, unique-label structural record rows, inferred callable effects, and explicit dynamic-data boundaries. Generate typing problems from source, solve declaration relationships, fix elaboration, publish one solved frontend representation, and let tooling and indexed lowering consume it.

Priority order:

1. Sound typing and faithful execution.
2. Low annotation burden and useful reusable inferred contracts.
3. Deterministic diagnostics close to the conflicting source contributions.
4. Cold-check speed, retained memory, and runtime performance.
5. Maintainable ownership and removal of duplicated semantic machinery.

All mandatory gates must pass. Priority order determines how to resolve conflicts; it does not waive a lower-priority acceptance condition. A sound implementation that misses the annotation or performance gates remains incomplete.

### Fixed decisions

| Question | Campaign decision |
|---|---|
| Declaration inference | Infer reusable schemes from definitions and dependencies; each call gets a fresh instantiation. Callers never train a declaration. |
| Public functions | Complete inferred exported schemes are allowed. Preserve existing deliberately written public promises during migration. |
| Generic statements | Require explicit discard or a sufficient contract when statement behavior cannot be fixed before generalization. Never defer assertion or propagation selection to instantiation. |
| Callable values | Preserve every statically justified signature through existing aliases, conditionals, fields, containers, parameters, and returns, within rank-1 restrictions. Explicit erasure remains dynamic. |
| Sealed requirements | Cover all applicable existing finite builtin operation families, including equality, arithmetic, ordering, membership, iteration, and Map key eligibility. Freeze the concrete inventory before implementation. |
| Internal annotation removal | Sound internal generalization may count as removal. Protect deliberate domain restrictions, semantic annotations, external boundaries, public promises, and explicit `[]` bounds. |
| Module reuse | Reuse checked dependencies within one bundle, including diamond imports. Across-edit incremental caching is deferred. |
| Maintained migration | Allow scoped production changes under `core/`, `dev/`, and relevant `examples/`; protect annotation-specific fixtures and keep showcase programs unchanged. |
| Performance | Use lightweight per-workload regression observations; no quiet-machine or exhaustive sampling prerequisite. Keep deterministic complexity and resource guards. |
| Unattended failure | Stop with a reproducible incomplete checkpoint after three complete attempts without meaningful progress on the same mandatory gate. Never relax budgets or change denominators to pass. |

The scope includes omitted required/default/rest parameter types, returns, producer item types, effects, locals, rank-1 callable relationships, inferred exports, row-based record helpers, contextual inference, ordinary monomorphic recursion, checked dynamic boundaries, and generic indexed execution. Make only the grammar changes necessary to omit those annotations and use already supported constructs.

The scope excludes higher-rank inference, independently polymorphic callback parameters or container elements, polymorphic recursion, general recursive structural types, dependent types, unrestricted union/intersection algebra, user-defined trait/instance search, effect handlers, numeric-coercion redesign, an ownership-language redesign, new closure or callback syntax, general async machinery, arbitrary compile-time execution, and an SMT/SAT solver. Do not introduce a public typeclass or inferred-predicate DSL.

Across-edit incremental reuse, a daemon, a persistent type cache, a cache database, a new background process, Linux/Docker verification, cross-compilation, CI-platform changes, and release packaging are deferred. Record useful future work briefly in the final report; these items cannot substitute for the mandatory cold and within-bundle gates.

### Environment and authorization

- The execution host is macOS aarch64. Use the repository's pinned Rust toolchain and actual supported feature configuration.
- Read applicable `AGENTS.md` instructions, `docs/CHAPTER-01-why-xsh.md`, `docs/ARCHITECTURE.md`, `docs/TEST-MAP.md`, and the closest code/tests before editing. Use `docs/SPEC.md`, `docs/SPEC-TYPING.md`, `docs/FRONTEND.md`, `docs/STREAMS.md`, `docs/JSON.md`, and OS contracts for their respective concerns.
- Begin implementation only after the concurrent source work has settled and an immutable handoff snapshot is available. Use an isolated local checkout/branch with separate baseline and candidate build directories. Preserve unrelated work and never reset or clean the shared checkout to manufacture a baseline.
- Routine builds and tests use targeted debug products. Targeted optimized builds are authorized for paired profiling and performance measurements. Do not use the `dist` profile.
- Local checkpoint commits in the isolated campaign branch are authorized; disable hooks for those commits. Do not run pre-commit hooks, merge into another checkout, push, or publish artifacts remotely.
- Add no dependencies. Use existing facilities or a small local implementation. A necessary dependency change is a contract blocker to report, not an unattended authorization to add it.
- Do not run formatter or linter CLI commands, including their non-fixing forms, or broad formatter/autofixer gates. Tooling library tests and annotation transformations confined to isolated campaign fixtures are authorized. Maintained-source migration uses scoped reviewed patches; review means checking the patch and evidence within this campaign, with no mandatory human approval between ordinary gates.
- Keep source comments self-contained. Do not mention this campaign, ergonomics documents, gate labels, planning paths, issues, or milestones in code comments/docstrings. Preserve local reasons and constraints in domain terms, and search touched comments before finishing.

## 2. Starting-point audit and durable owners

Do not assume the compiler still matches the earlier proposal's audit. At planning time, `Type` already contains `Type::Inference`; `TypeConstraints` supplies bounded monomorphic substitutions; builtin templates and dynamic-boundary consolidation already exist. Treat these as migration inputs. Re-audit the settled snapshot, record actual behavior, and choose reuse or replacement based on its invariants and measured costs.

The inspected tree has concrete consolidation opportunities: local inference clones and rechecks callable bodies; effect inference repeatedly scans its graph; checked facts retain recursive types in several span-indexed maps; lowering still reconstructs method signatures and argument binding. Confirm their current owners before changing them.

| Concern | Existing retrieval targets | Required ownership outcome |
|---|---|---|
| Semantic types and constraints | `src/sema/types.rs`, `src/sema/constraints.rs`, `src/sema/check.rs` | One type/row/scheme store, scoped inference state, and solved facts with compact identities. |
| Local/declaration inference | `src/sema/check/local_inference.rs`, `infer_param.rs`, `infer_return.rs` | One generated constraint problem per declaration; remove probe/recheck repair paths. |
| Effects | `src/sema/check/infer_effects.rs` | Finite summaries plus effect variables and dependency-driven inclusion solving. |
| Builtin relationships | `crates/xsh-registry/src/signature/`, `src/modules/signature.rs`, `src/sema/builtin_templates.rs` | Canonical templates and finite operation requirements, instantiated by the shared core. |
| Call binding | `src/sema/arguments.rs`, `src/sema/stage_arguments.rs`, `src/sema/check/call.rs` | One resolved argument/default/spread plan, separate from source evaluation order. |
| Callable values | `src/sema/check/callable_alias.rs`, module contracts, call checking | Signatures are type facts; unique declaration identity is separate optional metadata. |
| Rows, schemas, constants | `src/sema/records.rs`, `src/sema/constants.rs`, projections, require checking | Shared rows and application facts; explicit runtime decoding and existing bounded preparation. |
| Refinements | `src/sema/check/proof.rs`, nearest narrowing code/tests | Bounded versioned facts over shared types; no unification-based theorem proving. |
| Frontend/module facade | `src/frontend.rs`, `src/loader.rs`, compact declaration/body outputs | One solved representation owned by the checked bundle and reused by its consumers. |
| Indexed lowering and execution | `src/runtime/eval/lower.rs`, `indexed.rs`, `indexed/full.rs`, `indexed/semantic.rs`, indexed executors | Consume solved semantics; prepare and verify generic evidence; keep one executable representation. |
| Tooling | `crates/xsht/src/`, API/reveal/edit/check consumers, ordinary XSH execution in `xshi` | Query the same solved facts without rebuilding signatures or weakening boundaries. |
| Measurement | `src/frontend_stats.rs`, `src/entrypoints/frontend_stats.rs`, existing runtime stats/profiling owners | Comparable phase/counter/retention evidence, with instrumentation confined to profiling products. |

These are retrieval targets, not a requirement to create a particular new directory tree. Fit new interfaces to the real owners. Record changed ownership and the obsolete paths to remove in `bench/typing/README.md`; do not create a parallel ticket/status framework.

A temporary compatibility adapter must state its input/output, why it is needed, which facts remain authoritative, and the gate that removes it. An adapter may materialize a bounded view of solved facts; it may not independently decide types, overloads, effects, or elaboration. No production adapter may survive final consolidation merely because its consumers were not migrated.

## 3. Freeze the semantic contract before inference changes

Update the canonical language/typing specifications first or in the same change as the executable target contracts. Freeze historical observations and distinguish preserved behavior from intentional new acceptance. The old checker is a behavioral baseline for established programs, not a soundness oracle for the new language.

### 3.1 Statement/value and Result decision matrix

Specify and test this matrix before enabling omitted-return inference:

| Declaration/body context | Required meaning |
|---|---|
| Ordinary omitted-return function/proc tail with resolved Bool | A value. `false` is returned. |
| Ordinary omitted-return tail with quantified payload `T` | A value, with the same elaboration for every instantiation, including Bool, Unit, Optional, and Result. |
| Explicit Unit/Result[Unit] body or native test body | A statement-consuming context retaining established assertion and propagation rules. |
| Non-tail expression resolved as Bool from declaration constraints | An assertion under the existing statement contract. |
| Non-tail operation with an established implicit propagation rule | Preserve that rule, including its exact callable kind and Result shape. |
| Still-generic non-tail expression whose behavior depends on `T` | Reject with a local request for explicit discard or a sufficient type/operation contract. |
| `let _ = expression` | Explicitly discard the resulting value. Explicit `?`, effects, cleanup, and control transfers within its initializer retain their meanings. |
| Omitted-return body with normal payloads and outward `?` | Infer one success/error relationship and record the necessary Result elaboration once. |
| Normal return independently established as a Result | Preserve the checked Result interpretation; do not silently double-wrap it. |
| Generic payload later instantiated as a Result | Preserve the original payload wrapping/nesting; do not dynamically flatten it. |
| Explicit `Ok(result)` | Preserve explicitly requested nesting. |
| Ambiguous payload-versus-Result interpretation | Require an annotation at that boundary; do not search among control-flow meanings. |
| Empty/no-value ordinary proc | Retain the established Result[Unit] success convention, unless explicitly contracted otherwise. |
| Error-only or otherwise underconstrained success boundary | Require context or a targeted annotation; do not invent a success value or public Never type. |
| Unreachable/non-completing path | Contributes no manufactured Unit/value; retain an internal control-flow fact. |

For example, `pure identity(value) { value }` returns false when instantiated at Bool. Conversely, `pure helper(value) { value; 1 }` cannot carry a latent rule that asserts only at Bool. `pure helper(value) { let _ = value; 1 }` has fixed discard behavior.

Resolve statement use and Result elaboration after declaration constraints establish enough information and before generalization. Keep each decision in the solved frontend output. Candidate trials, caller instantiation, discarded-call results, optimizer liveness, and runtime values cannot change it.

Preserve the existing distinction between a proc's explicit data return and any out-of-band propagation channel. Do not force all explicitly annotated procs into a new Result API. For unannotated functions with outward propagation, infer the success type from normal completions and the error type from propagated failures; use existing nominal error joins. An explicit return must not be reinterpreted solely to make the inferred boundary fit.

Freeze examples for every combination that matters: explicit/omitted returns; early/tail returns; Bool/Unit/Optional/Result payloads; explicit Ok/Err; explicit and implicit propagation; try/retry capture; callback bodies; producer delegation; defers; cancellation; process Status; and missing/non-completing paths. Include exact observations and independent unsuccessful CLI witnesses.

### 3.2 Preserve existing boundaries and timing

- A `try` body retains its established one-layer capture and nesting contract. Retry attempts, explicit return, loop transfer, defer cleanup, and cancellation retain their lexical targets.
- Producer item inference uses reachable yield/delegation sites in the producer's definition. `stream` fixes producer kind. Creation stays lazy, and no inferred type materializes a producer.
- Supplied arguments evaluate in source order; omitted defaults evaluate once in parameter order in their existing lexical scope. Defaults do not gain access to other parameters. Producer defaults remain delayed until the existing first-pull boundary; CLI defaults retain preparation-only restrictions.
- Keep integer/float distinctions, UInt's nonnegative boundary, Optional representation, nominal tags/errors/resources, error-family identity, process Status, collection value semantics, field absence versus null, and literal classification.
- A bare `{}` keeps its existing record/map classification rules. Later convenience is not evidence that it meant a Map.
- `pure`, `proc`, and `stream` retain declaration meanings and existing kind compatibility. An empty inferred effect set does not promote a proc to pure.
- Imports remain namespace-only, prohibited import cycles remain prohibited, and embedded standard-module identities remain sealed.

### 3.3 Semantic stabilization before annotation removal

Freeze original source and behavior first. Use baseline checked facts to identify omitted proc defaults and annotations that select assertion, wrapping, validation, conversion, or overload behavior. In authorized maintained source, insert the smallest purposeful annotations necessary to preserve those meanings before enabling the new omitted-return rules.

Build a stabilized annotated compatibility cohort that the baseline compiler accepts with the original observations. Freeze it before measuring or optimizing the new compiler. Both baseline and candidate performance runs use this exact source. Keep the original source and the stabilization diff for audit; newly inserted stabilizers are protected and counted separately from original eligible annotations.

Do not use a source-history flag, filename heuristic, old/new execution mode, or generic AST fallback to preserve old defaults. Existing native test bodies remain Unit-consuming tests. Protected sources outside the migration allowlist receive no edits: use isolated stabilized fixture copies for comparison, and report any intentional cutover migration they would require. Such a report does not authorize removing that workload or hiding an unexpected regression.

## 4. Type graph, schemes, and solver contracts

### 4.1 Shared representation and ownership

Implement one inference context per checked bundle, owned through the frontend facade. Use compact typed IDs for type nodes, inference variables, rows, schemes, constraints, and reasons, backed by arenas/dense vectors and existing interned symbols.

Separate these concepts explicitly:

- Mutable unbound metavariable, with level, ownership, union-find state, dependencies, and provenance.
- Rigid quantified parameter bound by a scheme or checked generic body.
- Concrete constructor and nominal identity.
- Explicit dynamic Any and erased record/callable boundaries.
- Poisoned recovery fact that cannot certify execution.
- Internal non-completion/control-flow fact.

Use union-find with rank/size, representative lookup, occurs checks, level lowering, and iterative deep traversals. Type equality/unification, directional assignability, row requirements, callable application, finite operation eligibility, effect inclusion, constructor instantiation, and Result propagation may have specialized handlers. They share variable identities, reasons, and scheduling. Runtime schema validation remains an explicit operation, not a unification success.

Share immutable structure and instantiate only quantified portions, preserving ground sharing. Do not clone wide types for every binding, call, alias, diagnostic, or interface. Do not use changing union-find state as an unversioned interning/cache key. Count unique graph allocations and auxiliary storage, rather than recursively charging every reference as if it owned a separate tree.

Expression/statement/call identities come from arena nodes together with their owning source/module identity. Spans serve diagnostics and source-edit projections; equal or overlapping spans are not semantic identity. Exported interface handles cannot outlive or refer into a dropped mutable inference store. Keep explicit ownership for immutable finalized graphs and their consumer views.

### 4.2 Worklist and transactional obligations

Generate the declaration's typing constraints once. Index suspended constraints by the variables/rows/effects they depend on; wake them only when relevant facts change. Merge watcher state safely when roots union, deduplicate queued work, and avoid global rescans on each binding or effect growth.

Overload probes must be reversible. Trail or isolate bindings, lowered levels, row lacks facts, effect facts, watcher registrations, reasons, and pending semantic choices. Path compression and rollback must agree. A failed candidate cannot leave a binding, diagnostic, elaboration choice, or stale wakeup in the surviving problem. Do not clone the entire checker or solve the whole graph per candidate.

Keep compact provenance edges and render failure explanations on demand. Preserve both conflicting contributions for incompatible mutable writes. Continue invalid-program checking using poisoned facts where useful, with bounded cascades; do not run a permissive salvage checker or permit poison to reach execution.

### 4.3 Rank-1 generalization and recursion

Resolve names and declaration dependencies first. Solve strongly connected declaration components in dependency order. Recursive placeholders are monomorphic within a component, and permitted variables generalize after that component is solved. Support ordinary monomorphic recursion where constraints determine it; reject polymorphic recursion and infinite structural equations promptly.

A scheme quantifies the type, row, and effect variables permitted by its ownership/level dependencies and may retain approved finite builtin requirements. Requirements must refer to variables connected to the quantified signature; floating ambiguity is an error. Quantified variables are valid relationships, not unresolved concrete answers.

Use a conservative value restriction with these observable rules:

- Named callable definitions may generalize variables independent of persistent captured state. Function-local scratch state is fresh per invocation; it does not globally tie all callers together.
- Safe immutable values, named callable aliases, and inert aggregates/projections of safe values may generalize variables independent of the environment and shared resource/mutable state.
- Ordinary call results, live producer/handle creation, and initializers that may retain fresh mutable state are monomorphic at the receiving local binding unless a documented sound existing analysis proves safe generalization. Purity syntax alone is insufficient evidence.
- A `var` has one monomorphic lifetime type. Aliasing it through `let` does not freshen its variables. Signature preservation for mutable callables never implies polymorphic storage.
- Captured/environment variables and nominal identities obey scope escape checks. Variables reachable from shared resource-bearing or mutable captured state cannot be freshly generalized as independent.
- Callable parameters and container elements are monomorphic within an instantiation. Quantifiers do not appear inside parameter, field, or element types. A named/let-bound enclosing scheme may quantify a relationship over an entire aggregate.
- A callable returned from a function may have an ordinary monomorphic callable type whose variables belong to the enclosing rank-1 scheme. This does not give a returned callback independent higher-rank instantiation.

Test both permitted generalization and rejection: identity at multiple types; safe empty immutable collections; aliases; captured variables; factories returning stateful callables; mutable callable assignments; returned callables; recursive components; and independently polymorphic callbacks that exceed rank-1 scope.

No arbitrary default to Any, Str, Int, or a nominal type repairs an underconstrained variable. Safe parametric relationships may generalize; executable operation ambiguity, error-only success ambiguity, and required non-reifiable schema information need local annotations.

### 4.4 Structural record rows

Infer `pure name(entry) { entry.name }` as a relationship equivalent to `forall T,R. {name:T | R} -> T`: the required field has type T, and additional fields remain permitted and preserved where returned or updated.

Use unique labels, normalized row operations, and sufficient occurs/lacks checks to prevent duplicate or impossible rows. Support existing field access, nested requirements, destructuring, constant-key projection, record construction, and functional update contracts. Share rows and avoid sorting/copying a complete wide record for every projection or update.

Keep row equality distinct from existing width assignability. A wider record passed to a narrower promised schema must not lose its actual fields by unification. Preserve literal excess/missing-field checks, container invariance, schema-owned defaults, contextual conversions, and nominal boundaries. Row inference does not add field-extension/removal or type-changing update semantics that the language does not already support.

Distinguish closed empty records, open inferred rows, and explicitly erased Record/Any. Optional field values and optional field presence are separate. A function that deliberately handles absence must not accidentally infer unconditional presence. Preserve an explicit/dynamic boundary where a richer optional-row relationship is outside the supported fragment.

### 4.5 Effects and callable relationships

Represent the finite concrete effect vocabulary with compact bitsets and effect variables only where relationships require them. Preserve `io` coverage, pure/proc/stream kinds, and the distinction between a closed empty summary and unknown effects.

Infer summaries through solved calls, callable values, higher-order use, defaults at their actual evaluation sites, lexical propagation, and latent producer work. Selecting, storing, forwarding, or returning a callable preserves its latent effects without executing or charging those effects to the enclosing operation. Invocation contributes them; defaults contribute at their established invocation boundary. Solve monotone inclusions with dependency wakeups, including recursive components. Explicit effect lists are caller-visible upper bounds and cannot be widened; explicit `[]` remains a protected restriction.

Dynamic erased callables have unknown effects. Restricted consumers require a checked contract. Constructing Result data does not itself propagate an error; local try/retry capture removes only its outward propagation channel, not filesystem/network/process/etc. effects. Creating a producer does not execute its latent work, and resource ownership remains a separate analysis.

Use existing nominal error-family joins and Error for unrelated supported families. Do not generate anonymous unions of all observed errors. Keep error data/payload identity independently checked.

### 4.6 Independent reference fragment

Implement a deliberately simple, test-only reference solver for a precisely listed core: finite concrete constructors, arrows, rank-1 schemes, equality/unification, and unique-label open/closed rows with the required lacks/occurs rules. Include the value-restriction cases represented in that fragment. Use a straightforward representation and solving strategy sufficiently different from the production union-find/worklist implementation to catch shared algorithm mistakes.

Compare accept/reject outcomes and normalized relationships modulo quantified-variable renaming and label order. Generate bounded terms and constraint permutations with fixed seeds. Include infinite equations, row conflicts, environment/capture escape, and failed-probe isolation in production tests.

The reference does not implement the full language, runtime, effects, overload search, refinement logic, or unrestricted assignability. Document its boundary honestly. Independent negative programs, contract tests, and runtime observations supply evidence outside that fragment; no test corpus proves full-language soundness.

## 5. Finite operation requirements and bounded overload resolution

### 5.1 Freeze an exhaustive operation inventory

Before implementing generalized builtin obligations, create `bench/typing/operations.json` from the settled registry and language-operation owners. Registry methods alone do not enumerate operators, membership, iteration, or stage contracts. Every existing operation entry must be classified as monomorphic, ordinary parametric template, eligible sealed requirement, explicit dynamic boundary, or a documented non-generalizable boundary.

The inventory must cover these applicable families:

| Family | Required relationship/decision |
|---|---|
| Equality and inequality | Admissible operand relationships, nullable comparisons, nominal restrictions, and Bool result. |
| Arithmetic and compound arithmetic | Existing operand/result domains, UInt boundaries, numeric distinctions, and Duration/collection relationships where supported. |
| Ordering and comparison chains | Existing finite ordered domains, compatible operands, Bool result, and once-only operand evaluation. |
| Membership | Container/domain relationships and the existing canonical membership semantics. |
| Iteration, comprehensions, and delegation | List/Map/Stream/Str/Bytes item relationships, producer kind/effects, and existing Result handling. |
| Collection operations | Element/key/value relationships for construction, append, extend, get, set, fold/reduction, and other existing generic methods. |
| Map keys | The fixed supported scalar key domains and their existing ordering/representation rules. |
| Receiver methods and operators with varying results | Finite receiver/result relationships, labels/defaults, conversion behavior, effects, and runtime operation IDs. |
| Callable application and existing higher-order consumers | Parameter/result relationships, callable kind, labels/default/rest metadata, latent effects, and existing stage identity restrictions. |
| Constructors, constant keys, and descriptors | Existing parametric construction, preparation-time facts, and contextual schema/application constraints. |
| Validation operations | Independently justified reifiable schema contracts; no inferred trust from desired field accesses. |

For each entry record its authority symbol/path, stable operation identity, operand/receiver heads, parameter labels and modes, relationship template, result/error/effect behavior, retained-requirement eligibility, evidence needs, and positive/negative coverage owner. Classification must follow the actual operation contract. Do not mark a genericizable family non-generalizable solely because it is expensive to implement.

Freeze this inventory at Gate A. Sound support for its applicable existing families is mandatory. Unknown/user-defined receiver search, unrestricted conversion ranking, and higher-rank relationships remain outside scope. If an entry needs an excluded language feature, retain its explicit boundary and explain that independently of campaign difficulty; do not invent an instance system to support it.

### 5.2 Solving and coherence

Resolve lexical/module identities before type solving. Index candidates by operation spelling/ID, arity, labels, and known head families. Apply cheap filters, propagate facts common to remaining candidates, and prune when dependencies become known. Expected result types constrain choices only through the declared relationship.

A genuinely generic named function may retain an approved requirement in its scheme. At a concrete call the requirement must discharge uniquely; an enclosing generalized helper may forward the same checked requirement. No floating variable or ambiguity unrelated to the signature can be hidden by generalization.

Do not recursively explore combinations of overload choices across a large expression. No recursive instance search, blanket instances, global backtracking, speculative assertion/wrapping choices, or runtime overload lookup. Ordinary Int/Float/Duration/etc. literals retain their defined defaults; they cannot default unconstrained parameters.

Overload selection must be invariant under candidate/declaration order and transparent refactors. Discarding a caller's result does not change operation meaning. If a concrete executable operation remains ambiguous, report its small remaining candidate set and the local annotation that would distinguish it.

Freeze deterministic work/node/depth/diagnostic limits before performance tuning. Charge all attempted candidate work, including failed probes. A limit failure identifies the responsible constraint/source region and suggests a local boundary. It never permits an Any fallback, an unresolved operation, an unmeasured retry, or runtime inference. Regular cohort/scaling cases must pass without hitting guards.

## 6. Bidirectional checking and one solved frontend result

Synthesize types where expressions provide information and check against types where consumers provide it. Expected information must reach nested literals, constructors, callbacks, returns, branches, yields, defaults, named/spread arguments, and independently anchored `.require()` targets.

Generate typing constraints once per declaration solve. Dependency/name collection, bounded constant preparation, and flow analysis may have their own justified traversals; instrument them separately. The once-only requirement prohibits regenerating/rechecking the body to repair newly known receiver or local types. Candidate tests inspect generated obligations, not fresh AST traversals, and cannot duplicate diagnostics or evaluation.

Return one solved frontend result containing at least:

- Expression, binding, parameter, return, and producer-item TypeIds; declaration/immutable-binding SchemeIds; instantiated call signatures and evidence substitutions.
- Resolved symbol/operation identities and explicit supplied/default/rest/spread binding plans.
- Source evaluation order distinct from parameter-slot order, including receiver and spread evaluation.
- Field projections, constructor application identities, required conversions, UInt boundaries, and explicit schema-validation operations.
- Assertion/value/discard use, Result/Ok wrapping, propagation/capture decisions, and lexical control targets.
- Effective versus required effects, callable kind, non-completion/reachability, and bounded versioned refinement evidence.
- Owning source/module identities and compact diagnostic provenance, including failed-program facts that are unsuitable for execution.

Use the existing arena and facade owners; do not create a second AST just to carry facts. Lowering, native test preparation, check/reveal/API, annotation edits, grep/refactor, tooling library validation, and ordinary XSH execution in xshi must consume the same answers.

Normalize argument binding once. Preserve original supplied evaluation order and default timing even when named arguments map to different parameter slots. A spread receiver evaluates once; projections/default slots do not reevaluate it. Optional calls retain skipped-argument laziness. Retain constructor/default/schema rules through aliases and imports.

A verifier independently checks indexed layouts, instruction/evidence validity, and ownership. It does not reconstruct source signatures or choose overloads. Temporary views for consumers must query solved identities. Remove compact body probes, lowerer type reconstruction, and additional argument binders when their semantic work is duplicated.

### 6.1 Callable values beyond transparent aliases

Treat callable signatures as type relationships, separate from an optional unique declaration identity. Preserve static signatures through:

- Direct names, imports, and transparent immutable aliases.
- Compatible conditional selections and record fields/projections.
- Compatible homogeneous callable containers and their checked retrieval operations.
- Callable parameters, existing callback/block uses, and callable returns.
- Mutable callable bindings with one compatible monomorphic lifetime signature.

Compatibility includes kind, labels, required/default/rest slot shape, parameter/result relationships, effects, and established data-versus-propagation conventions. When branch effects differ, use a sound existing finite upper summary rather than dropping them. Preserve actual defaults on the selected callable handle; differing default values are not permission to execute an arbitrary branch's defaults.

A computed callable need not identify one declaration. Do not fabricate navigation/effect edges or a direct-call identity for it. Existing stage descriptors that require a statically named identity retain that boundary; use the already supported explicit block form to invoke a computed typed callable. No new callback syntax or changed descriptor semantics are required.

Instantiate named schemes before placing callables in parameter/field/element monotypes. Preserve the whole enclosing rank-1 relationship without storing nested forall values. Conditional compatibility cannot broaden incompatible signatures to Any. Genuinely erased `Pure`/`Proc`, dynamic module exports without a checked contract, and dynamic selections retain explicit dynamic invocation rules and unknown effects.

For each existing invocation form, freeze its wrapping/propagation semantics. A more precise signature must not silently change the Result envelope of a dynamic `.call` operation or the channel of a direct proc invocation.

### 6.2 Tooling and annotation output

Display inferred schemes faithfully in reveal/API/structured output, including quantifiers, rows, effects, and approved requirements. Use deterministic naming/order independent of internal allocation IDs. Annotation insertion/rendering must emit only parser-supported syntax and decline to invent an equivalent source contract when none is printable. Eligible annotation removal may leave source unannotated with a sound inferred scheme even when that scheme has no printable source spelling. Protected promises stay written. Never print Any as a stand-in for polymorphism.

Keep editor/failed-source diagnostics usable using the same constraint semantics and bounded poisoned facts. Execution still requires a fully valid solved program. Explicit contract compatibility, module validators, edit rechecking, and ordinary runners enforce one dynamic policy.

## 7. Refinements, constants, schemas, and resource boundaries

### 7.1 Bounded flow facts

Keep flow-sensitive reasoning separate from equality solving, over the same TypeIds and binding/path identities. Track existing null/variant/type/presence predicates, stable record projections, and immutable Boolean aliases with versioned provenance. Refine branch/guard/return continuations, short-circuit operands, pattern guards, and successful assertions only according to their established contracts.

Writes, calls, capture, shadowing, and joins invalidate or intersect facts using existing alias/path overlap rules. An old true Boolean does not prove the present state of a changed record. Loops use a conservative bounded fixed point with counted visits. Unknown calls invalidate the facts they may affect conservatively; no arbitrary Boolean implication or theorem prover is needed.

Filesystem observations, deadlines, host-resource liveness, and cancellation state are not immortal type refinements. Preserve user assertions; do not remove them as incidental proof hints.

### 7.2 Constants and descriptors

Use existing bounded preparation-time constant analysis. Inline, local/imported constant, transparent alias, and constant projection forms must retain equivalent descriptor-derived types and validation. Normalize the contract representation where runtime interpretation and static shape inference duplicate policy.

Do not interpret user functions/defaults during inference or add general CTFE. Preserve constructor application identity, phantom/unused schema arguments, schema-owned defaults, nominal wire decoding, and preparation before runtime effects. Constructor defaults do not invent missing independent type evidence.

### 7.3 Dynamic validation and nominal/resource checks

Metavariables describe missing compile-time information. Any, erased Record, and erased callables describe explicit runtime uncertainty. Do not let either substitute for the other.

Concrete-to-Any erasure remains allowed. Any-to-concrete flow requires an established explicit schema/type-pattern/module-contract validation boundary. Expected types alone do not validate data. Existing checked dynamic operations remain usable and dynamic.

An omitted `.require()` target needs an independently anchored, concrete reifiable contract. Ordinary later property access, an open row, or an unconstrained generic variable cannot invent a JSON schema. A schema witness is usable only if its reifiability and ownership are checked before execution under the existing construct's contract. Otherwise require the explicit named/concrete schema.

Retain resource ownership/escape checks, path validity, process protocol checks, nominal constructors, UInt storage constraints, and schema validation. These are independent contracts, not duplicate inference. Test generic values containing resources and context-scoped producers against these boundaries; quantified types cannot bypass them.

Confirm whether any permissive/strict divergence or legacy descriptor adapter still exists in the settled snapshot. Consolidation already present must remain present. Remove only actual remaining divergence/duplicate representations and required migration switches. Do not recreate a removed `--strict` mode or spend this campaign removing unrelated names.

## 8. Generic execution on the existing indexed engine

Generalized parameters are statically quantified, not erased to Any. Carry rigid generic parameters and prepared evidence through the solved representation. Share one indexed body where the uniform value representation permits it.

An operation that depends on a record layout, sealed operation choice, callable evidence, or reifiable schema receives compact private prepared evidence: slot, operation, type/schema, and substitution identities. Verify its scope, arity, type relationship, and lifetime before execution. Forward evidence through ordinary recursive/higher-order calls and captured callable handles as needed, without building a new metadata graph per invocation.

A generic row projection must work on different narrow/wide record shapes through the same body. Avoid a runtime field-name scan or type lookup disguised as a witness. A known monomorphic operation retains its direct fast path.

No ordinary invocation may parse source, generate constraints, infer types, select overloads by runtime search, compile/specialize a body, or fall back to an AST evaluator. Dynamic module loading remains its explicit existing compilation boundary. Embedded modules stay sealed and do not gain a per-call solver/parser.

Do not generate a body for every incidental caller or record shape. Optional specialization requires a measured hot path, canonical identities, preparation before execution, bounded code/evidence size, and sharing as the default. It cannot compensate for missing sound generic execution.

Executable artifacts contain no unresolved metavariables or poison. Quantified variables are bound by verified generic/evidence scopes. Type-independent operations may omit unnecessary evidence, but their quantified relationships remain valid; do not invent a concrete type just to fill a runtime slot.

Prove lifetime behavior after the frontend and inference context are dropped. Prepared bodies/evidence keep precisely their required immutable ownership. Test malformed evidence, wrong row slots, absent requirements, invalid quantifier scopes, and rewind/checkpoint behavior in the indexed verifier.

No runtime source inference is a hard rule. Evidence passing and prepared indirect operations can have measurable cost; report and gate that cost with generic-versus-monomorphic comparisons. They are not assumed free.

## 9. Within-bundle modularity and reuse

Compute each module's interface from its implementation and already checked dependencies. Publish normalized exported value/callable schemes, constant/descriptor facts, callable/default metadata, nominal identities, and relevant effects. Importers instantiate that interface; their actual clients cannot change it.

Preserve explicit module contracts and existing exact compatibility requirements where specified. An inferred minimal effect summary and an explicit promised upper bound have distinct roles. Public/private nominal identities cannot escape as unresolved names or foreign graph handles.

Within one checked bundle, use the loader's real module identity and resolution rules to load/resolve/solve each dependency once. Diamond imports reuse the same immutable interface and solved module facts, with correct source/namespace ownership. Avoid accidental duplication through import aliases and equal local spans. Deduplicate failures without losing the source/import provenance needed to explain them.

Assert module load, declaration solve, interface instantiation, and preparation counts in generated diamond fixtures. Changing a referenced dependency in a separate fresh bundle must produce fresh correct answers, so a process/session cannot accidentally reuse stale facts. This is correctness isolation, not a requirement to implement a cross-edit incremental cache.

Across-edit interface-diff invalidation, retained failure caches between edits, and private-body-only incremental rechecking are follow-up work. Do not introduce a cache design or daemon to pass cold measurements. Existing session facilities may retain finalized unchanged programs according to their current contract; they do not expand this campaign's scope.

## 10. Annotation eligibility, source migration, and observations

### 10.1 Freeze eligibility before solving for percentages

Count annotations structurally in the representative real-program cohort. Separate local binding annotations from internal parameter/return/effect annotations. Count one explicit syntactic site once; count an effect clause once, not once per effect name. Schema field declarations are contracts and have their own protected count. Source strings containing fixture programs do not become annotations of the enclosing native test module.

Record each site by original source identity, declaration/binding identity, token span, category, eligibility, reason, and observation/contract owner. Keep correspondence through transformations. Freeze the manifest and both denominators before the new solver/migration results are known.

Use these classification rules:

| Class | Rule |
|---|---|
| Ordinary internal/local annotations | Eligible unless positive evidence establishes a protected purpose. Current inference difficulty is not a reason to exclude. |
| Sound internal scheme broadening | Eligible: replacing a concrete identity/relationship with a more general sound scheme is allowed when it does not erase domain intent or alter execution. |
| Deliberate public/API/module promises | Protected, even if implementation could infer a broader contract. Inferred new public declarations receive separate acceptance fixtures/reporting. |
| Explicit `[]` effect clauses | Protected restrictions, including internal declarations. |
| Nonempty internal effect clauses | Eligible summaries unless docs/tests/call boundaries establish an intentional upper bound. Preserve demonstrated bounds. |
| Domain/resource/nominal intent | Protected when narrowing matters independently of inferred implementation, including deliberately promised UInt/domain restrictions. An annotation already determined by operations is not automatically protected merely because it names Path or a resource. |
| Schemas/validation targets/wire contracts | Protected external or runtime contracts, including independently selected `.require()` targets. |
| Assertion/Result/conversion/overload selection | Protected when the annotation selects semantics; compare checked elaboration, not only acceptance. |
| Annotation-specific parser/checker/tooling fixtures | Protected coverage; do not erase the contract those tests exercise. |
| Added semantic stabilizers | Protected and reported separately; do not inflate the eligible denominator. |

Classify deliberate intent using canonical contracts, local comments, tests, boundary use, and independently demonstrated semantic differences. Do not protect every old concrete type as an intentional promise. If a factually incorrect classification is discovered later, record it and require a separately authorized denominator revision; until then report against the original denominator, including any resulting failure. Never silently reclassify hard cases.

Targets on the frozen internal cohort, removed jointly:

- At least 95% of eligible local binding annotations.
- At least 85% of eligible internal parameter/return/effect annotations.

For each target, report frozen eligible count, jointly removed count, retained eligible count with reasons, protected/excluded counts by reason, and added stabilizers. Report parameter, return, and effect subcounts as well as the combined internal target. Do not call broad inferred Any acceptance or erased effects a successful removal.

Joint success is checked on the fully reduced variant, including its imports. Individual removability only informs diagnosis. No new Any, unintended weakened public/domain promise, extra validation, changed cleanup/order/error handling, or resource escape may be introduced to reach the percentages.

### 10.2 Migration allowlist

| Surface | Authorized changes |
|---|---|
| `core/**/*.xsh` | Scoped semantic stabilization and verified redundant-annotation removal; explicitly includes `core/system-report.xsh` and `core/lib/system_report*.xsh`. |
| `dev/**/*.xsh` | Scoped production/helper migration, including system-report corroboration modules; preserve host/platform and entrypoint contracts. |
| Relevant `examples/**/*.xsh` | Migrate substantial examples affected by inference, preserving their purpose and catalog/API boundaries. |
| `tests/xsh/**`, Rust/tooling tests, intentional fixtures | Add focused new coverage and adjust obsolete acceptance/rejection expectations; retain annotated-form coverage. No blanket annotation-removal migration. |
| `showcase/**` and pre-existing benchmark programs | Freeze, measure, and test without source migration. Use isolated variants where needed; changes require separate scope authorization. |
| Canonical `docs/*.md` | Update language/typing/frontend/architecture/test ownership. Do not rebuild generated docs. |
| `docs/snippets/**`, generated API content, catalogs/data | Preserve unless a directly affected canonical example or inventory entry requires a scoped correctness update; no annotation-erasure sweep. |
| New `bench/typing/**` artifacts and fixtures | Create the frozen cohort, transformations, harness, measurements, and final report for this campaign. |

The measured frozen variants may include native modules/showcase sources beyond the maintained migration allowlist. Achieving the score in an isolated variant does not authorize rewriting protected source. Source migration and annotation measurement are separate outputs.

### 10.3 Equivalence and metamorphic tests

Compare more than exit success. Observe returned values and Result nesting, stdout/stderr, exact process status, filesystem fixture changes, nominal failures, explicit assertion outcomes, call/default ordering, lazy evaluation, cleanup/cancellation, and resource lifetime. Use isolated temp fixtures and existing mocks; no live host discovery or destructive operations are needed for an equivalence witness.

For sound internal generalization, compare the inferred relationship rather than demanding the old narrower scheme's textual identity. Preserve intentional public/domain promises. Normalize scheme snapshots modulo quantified names, field order, and stable operation IDs.

Use AST/CST-aware isolated transformations for individual and joint annotation removal, alpha renaming, independent declaration reordering, transparent alias insertion/removal, constant/descriptor extraction, and inert record-field reordering. Honor lexical visibility, effects, preparation restrictions, and source order. Every transformed variant must be rechecked normally; reject unsafe transformations without partial source writes.

Add adversarial mutations and independent CLI rejection witnesses. The new assertion/inference implementation cannot be the only judge of its own rejection behavior. Compare reference-core outcomes, process status, verified artifacts, and runtime observations together.

## 11. Campaign artifacts and gate protocol

Use the nearest existing benchmark owner, normally `bench/typing/`, with this small artifact set:

| Artifact | Required contents |
|---|---|
| `README.md` | Current gate, last verified checkpoint, exact failing condition if any, next action; final outcome, commands, measurements, scope limits, and deletion inventory. |
| `manifest.json` | Schema version, immutable source/cohort hashes, dependency closures, baseline binary hashes and build settings, workload IDs/commands/inputs/expected observations, platform eligibility, sampling/resource-limit policy, and phase/accounting definitions. |
| `annotations.json` | Frozen original/stabilized site correspondence, protected reasons, eligible denominators, stabilizers, joint-removal results, and retained-site explanations. |
| `operations.json` | Frozen exhaustive finite operation classification, relationship/effect/evidence contracts, authority identities, and coverage owners. |
| `results.json` | Appendable run records with source/binary/config hashes, gate/check status, raw samples, per-workload comparisons, counters/scaling, normalized interface facts, and observation/rejection results. |
| `cohort/` and generated fixture ownership | Immutable original/stabilized sources with their import closures; reduced/metamorphic/negative/scaling variants generated reproducibly from manifests and fixed seeds. |

Keep machine-readable fields simple and versioned. Identify a run by source, binary, settings, workload, variant, and measurement kind. Use explicit units, null for unavailable instrumentation, and passed/failed/not-run/not-applicable status with a reason. Never turn an unavailable measurement or skipped assertion into a pass.

Do not store host secrets or unrelated source in artifacts. Do not invent another receipt/proof/status framework. Canonical docs own language semantics; the benchmark README owns execution status and measured evidence.

### Gate rules

Run A through G sequentially. Within a gate, delegate independent bounded work only after interfaces/ownership are clear; avoid overlapping edits to central checker/runtime contracts. Integration and acceptance remain one responsibility.

A gate passes only when its required artifacts exist, its mandatory checks actually ran and passed, its retirement obligations are met, and prior applicable gates remain green. A command example is not evidence it ran. Record exit status, relevant result counts, feature configuration, source/binary identities, and skipped/non-applicable cases.

Capture a local compiling checkpoint at each passed gate. When a later attempt fails, retain its failing regression and evidence, identify the last green checkpoint, and keep changes in the isolated branch. Do not hide the failure by deleting tests, reverting unrelated work, or editing reported samples. Follow the stop/resume policy below.

## 12. Sequential delivery gates

### Gate A: baseline, semantic freeze, and scope inventory

**Inputs:** Settled immutable source handoff; applicable instructions; existing architecture/specifications/Test Map; pinned macOS toolchain and feature set.

**Work:**

1. Confirm exact package/binary/test ownership and the starting-point audit. Read the nearest semantic, native, loader, indexed, and tooling tests. Record known baseline failures and actual removed compatibility paths.
2. Create an isolated candidate checkout and an immutable baseline source/build location. Use separate build output directories and exact executable paths so concurrent agents or stale target products cannot contaminate runs.
3. Freeze a stratified real-program cohort: small startup scripts; substantial `core/`/`dev/` tools; complete system-report dependency graphs; stream-heavy programs; broad stdlib consumers; import-heavy/diamond-shaped code; largest relevant native modules; representative valid and invalid annotated programs. Include the largest type-heavy cases found by the existing stats facilities. Count unique source sites once across import closures.
4. Fix workload selection criteria before inference work. Include required classes even when inconvenient. Declare macOS runtime-inapplicable host cases up front, retain their valid frontend checks where possible, and use existing isolated host fixtures for observable semantics. No live Linux execution is authorized.
5. Capture original behavior, exact observations, declared/inferred signature snapshots, byte/source identities, and semantically significant annotations. Produce the stabilization diff and baseline-compatible stabilized annotated cohort.
6. Freeze annotation eligibility, denominators, finite operation inventory, required acceptance cases, resource limits, phase definitions, and measurement/sample protocol. Resolve ordinary intent from evidence using the rules above; do not reserve denominator choices for the eventual score.
7. Build the exact baseline product and profiling binaries. Preserve the unmodified baseline product for timing. If missing observability requires an instrumented baseline copy, retain that minimal instrumentation diff and verify observational equivalence; do not attribute its overhead to product timings.
8. Write target semantic tests/specification changes before implementing inference. Historical baseline rejection of newly intended programs is expected and explicitly classified. Existing valid compatibility programs and baseline-observation tests must pass or have an independently documented pre-existing failure.
9. Record baseline product observations and retained-byte accounting. Preserve source/product identities and raw results. Extensive pilot/final sample sets and quiet-machine calibration are not prerequisites for implementation.

**Owner focus:** Specifications, loader/frontend, existing constraints/types, parameter/return/effect inference, registry/operation owners, current profiling products, native behavior tests, and host/process witnesses.

**Deliverables:** All frozen manifests/cohort artifacts; runnable observation and measurement harness; semantic decision matrix with executable target cases; current owner/deletion inventory; raw baseline measurements and toolchain/binary identities.

**Pass conditions:** The snapshot and cohort are reproducible; stabilized sources preserve baseline observations; every annotation has a frozen classification; every operation entry has a disposition; denominator totals reconcile; baseline runs include all applicable required workloads; noise/skip/failure reporting is honest. Future-language target tests may still reject on the old compiler and are listed as pending, rather than marked passed.

**Retirement/failure:** Gate A makes no new inference mechanism authoritative. If the settled handoff or runnable baseline is unavailable, stop before compiler changes with the specific missing prerequisite. Do not benchmark a moving shared checkout or continue on invented baseline values.

### Gate B: shared core and an early indexed execution slice

**Inputs:** Passed A; frozen semantics/operations/limits; baseline builds and compatibility fixtures.

**Work:**

1. Build/extend the type graph, scoped variables, union-find/levels/occurs checks, rows/lacks, schemes, compact reasons, effect inclusions, and dependency worklist. Replace the existing monomorphic core where necessary while retaining its proven rollback/boundary contracts.
2. Convert registry/operation relationships to canonical templates, with bounded isolated candidate trials and verified requirement/evidence identities. Add the small independent reference fragment and deterministic generated tests.
3. Specify the minimum solved-fact/evidence interface at the existing frontend facade and indexed owners. Support only the necessary omitted-annotation grammar in the existing parser for the slice; no new source DSL or alternate parser.
4. Execute these ordinary source fixtures through the normal checker, lowerer, verifier, and runtime:
   - Generalized identity at Int, Str, and false in the same program.
   - A row-projection helper used on distinct narrow/wide shapes and distinct field types through one generic body.
   - One genuinely generic sealed operation, with at least two distinct supported concrete discharges and one rejection.
   - An enclosing helper forwarding the generic operation/row evidence through an ordinary call.
5. Run the slice after dropping frontend/inference state on both existing indexed execution routes. Inspect prepared artifacts to establish shared generic body count and evidence scope; a runtime value search is not evidence preparation.
6. Add verifier rejection for malformed/foreign/missing evidence and ensure temporary builder/checkpoint rewind retains no invalid evidence.
7. Run focused old-constraint, builtin-template, local-inference, assertion, Result, and indexed tests plus an initial compatibility/counter pilot. Resolve representation/lifetime failures before broad language integration.

**Owner focus:** Semantic graph/constraints, registry adapters, frontend fact facade, parser omission support, indexed semantic/store/verifier/executors, native slice fixtures, reference-core Rust tests.

**Deliverables:** Shared solver contracts and counters; passing reference-core comparisons; early source-to-runtime fixtures; prepared artifact/lifetime evidence; explicit temporary-adapter inventory with C/D/G removal targets.

**Pass conditions:** All slice programs have correct observations; every generic call is statically resolved or supplied approved prepared evidence; both indexed routes execute after frontend disposal; bad evidence is rejected; production and reference core agree on bounded supported cases; no Any repair, per-call inference, AST fallback, or per-shape body cloning. Existing compatibility tests applicable to touched boundaries remain green.

**Retirement/failure:** New slice semantics have one authoritative core. Legacy consumer views may remain temporarily, but cannot make independent decisions for the slice. Do not advance with checker-only inferred signatures or a test-only evaluator. Representation/evidence failures block C until resolved or checkpointed incomplete.

### Gate C: declaration inference, callable flows, and solved consumers

**Inputs:** Passed B, especially the normal-runtime generic slice; frozen operation/semantic contracts.

**Work:**

1. Infer omitted required/default/rest parameters, returns, effects, producer items, constructor arguments, and local holes from definitions and expected contexts. Replace private-only/concrete-only inference exceptions with scheme generation.
2. Solve declaration SCCs with monomorphic recursion, level-based generalization, scope checks, and the value restriction. Add inferred exports independent of importer/caller order.
3. Implement signature preservation through all required statically justified callable flows: aliases/imports, compatible conditionals, fields/containers, parameters/returns, and monomorphic mutable callable slots. Keep unique identity and explicit erasure separate.
4. Publish the full solved frontend representation, including binding plans, fixed assertion/Result/conversion choices, descriptor/schema/application facts, required/effective effects, and diagnostic provenance.
5. Route full/compact outputs, annotation/reveal/API, native test preparation, normal runners, and ordinary XSH execution in xshi through the shared result. Preserve existing source/tooling contracts and parser-supported annotation output.
6. Exercise every frozen applicable operation family with positive and negative concrete discharges, generalized requirements, forwarded requirements, effect facts, and deterministic candidate-order tests. Do not cover only the early slice's one operation.
7. Test ordinary recursion, underconstrained success errors, safe immutable generalization, captured/stateful factory rejection, labels/defaults/spreads, missing fields, container invariance, and unsupported higher-rank/polymorphic recursion.

**Owner focus:** Checker declarations/expressions/calls, existing local/parameter/return/effect inference owners, callable aliases, registry/operation inventory, frontend/loader/facade, tooling library consumers.

**Deliverables:** Complete declaration/interface scheme facts; callable-flow tests and normalized signatures; binding/elaboration/effect plans; operation coverage mapping; adapted tool/native/runtime consumers.

**Pass conditions:** All required declaration/callable/operation-family cases check and expose their solved contracts; inferred exports are definition-owned; fixed elaboration matches the matrix; unsafe mutable/captured/higher-rank cases reject; checker/tooling consumers enforce the same boundaries. Already integrated runtime witnesses, including the entire B slice, execute normally. List remaining exhaustive indexed coverage explicitly for D, with no fallback executing unsupported cases. Consumer parity compares normalized answers, not duplicate independent rechecking.

**Retirement/failure:** Remove local cloned-body constraint collection and repeated private-return/default repairs where the new generator replaces them. Remove effect graph whole-program scan solving. No new per-method/type-name repair branches are permitted. If a consumer still needs a temporary solved-fact adapter, name its exact remaining D/G removal task; no consumer may keep an independent semantic checker.

### Gate D: full indexed integration, refinements, boundaries, and module reuse

**Inputs:** Passed C; complete solved-fact interfaces; early generic/evidence verifier contracts.

**Work:**

1. Finish generic execution/evidence for every applicable frozen operation family, callable flow, row operation, producer use, and existing reifiable schema construct. Preserve monomorphic direct paths.
2. Integrate existing versioned refinement, constant/descriptor preparation, schema validation, nominal/UInt/resource boundaries, and producer/default timing with TypeIds and solved decisions. Preserve working bounded analyses rather than rebuilding them gratuitously.
3. Prove within-bundle module/diamond reuse with source ownership, interface/solve/preparation counts, imported schemes, private nominal identities, and independently changed fresh-bundle dependencies.
4. Remove lowerer overload/signature selection, semantic argument rebinding, compact probes, and duplicated type reconstruction. Lowering consumes operation/binding/elaboration plans and treats missing facts as a frontend error.
5. Run compatibility observation pairs on the frozen stabilized annotated cohort. Test both indexed routes after frontend disposal, including imported modules, typed callable handles, resources, defaults, defers, cancellation, and schema failures.
6. Verify unsupported dynamic-to-concrete flows and unknown effects remain rejected. Confirm already-removed modes/adapters stay absent; remove actual remaining semantic-policy divergence only.
7. Run the relevant semantic, native stdlib, loader/module, indexed verifier, tooling, and ordinary xshi execution gates. Check phase/accounting and generic execution for obvious regressions before annotation migration.

**Owner focus:** Indexed lowerer/store/executors, proof/schema/constants owners, loader/interface reuse, resource lifecycles, complete solved-fact consumers.

**Deliverables:** Fully executable static schemes/evidence; correct refined/validated boundary facts; counted within-bundle reuse; compatibility/lifetime observations; source inventory proving semantic rebinding/probes removed.

**Pass conditions:** The annotated compatibility cohort retains its prescribed observations; every required family/flow has indexed execution coverage; no source inference or type search runs during invocation; stale proofs and Any laundering reject; modules solve once per bundle dependency identity; executable artifacts verify after frontend drop. All affected full relevant gates pass, with pre-existing/platform exclusions distinguished from new failures.

**Retirement/failure:** Retire duplicate lowering binders/signature helpers and all compact semantic probes. Keep only representation/layout verification and necessary host/protocol/resource validation. Across-edit caching is not an escape hatch for a failed cold or reuse test. Any observed execution/cleanup/validation change blocks E until explained by the frozen contract and verified.

### Gate E: joint annotation removal, refactors, and adversarial evidence

**Inputs:** Passed D; frozen annotation denominators/operation inventory; fully executable candidate; original/stabilized/reduced fixture correspondence.

**Work:**

1. Generate individual-removal variants to diagnose dependencies, then one jointly reduced variant per cohort graph. Keep the fixed denominator and site mapping even when a group cannot be removed.
2. Run all reduced sources through normal checking, verified preparation, and applicable observation workloads. Record generalized internal schemes and retained intentional/public promises; validate protected annotations remain intact.
3. Generate the permitted alias/rename/reordering/constant/descriptor transformations and compare normalized schemes, selected operations, elaboration, runtime observations, and diagnostics where appropriate. Recheck the entire affected import graph.
4. Mutate valid programs into independent invalid ones: incompatible mutable writes, wrong nested fields, missing rows, invariant-container mismatch, nominal mismatch, unchecked dynamic flow, stale proof use, bad effect bounds, alias erasure, incompatible labels/spreads, infinite type/row equations, captured-variable generalization, mutable callable polymorphism, escaped identities, unsupported higher-rank use, concrete overload ambiguity, and altered Result propagation.
5. Include wrong-error/wrong-schema/UInt/resource cases and effect timing failures. A program rejected only by a parser error unrelated to its intended mutation does not demonstrate type rejection.
6. Verify selected negative cases independently through CLI exit status and absence of pre-validation host effects. Use native tests for language behavior and Rust for exact process/byte/verifier boundaries. Compare the supported generated fragment against the independent reference.
7. Exercise transformation refusal and stable repeated application without weakening types, losing comments/source provenance, or writing partial maintained patches.

**Owner focus:** Native inference/boundary modules, source-edit library APIs and fixtures, process/verifier witnesses, annotation manifest/harness.

**Deliverables:** Reproducible reduced/metamorphic/negative corpus; per-site joint-removal results and protected counts; normalized scheme/elaboration/operation comparisons; actual soundness/runtime witness results.

**Pass conditions:** Joint 95% local and 85% internal targets pass on the frozen denominators; every applicable runtime observation matches the prescribed contract; protected promises remain; all required adversarial mutations reject for the intended boundary; supported reference comparisons pass; transparent refactors preserve relationships/selection. No Any/effect erasure, denominator adjustment, extra validation, changed control flow, or weakened public/domain promise buys the score.

**Retirement/failure:** Successful transformations are candidates for G's allowlisted patches, not blanket source authorization. Keep hard eligible sites in the denominator. A failing percentage or observation is a real campaign failure to fix or checkpoint, not a reason to enlarge exclusions or accept individual-only removability.

### Gate F: lightweight performance regression and deterministic scaling checks

**Inputs:** Passed E; immutable annotated/reduced pairs; exact baseline and candidate builds; frozen measurement/guard policy.

**Work:**

1. Run lightweight comparisons below: baseline versus candidate on the identical stabilized annotated source; candidate annotated versus jointly reduced; unchanged monomorphic runtime; generic/evidence versus handwritten monomorphic equivalents.
2. Attribute failures to measured parse/load, resolution, generation, solving, generalization/interface, finalization, lowering/verification, or execution work. Include allocations/retention and evidence/code size. Do not move work outside the measured interval.
3. Improve shared mechanisms: level/occurs traversal, row representation, watcher deduplication, requirement filtering, ground sharing, interface reuse, or unnecessary consumer materialization. No workload-name fast paths or selective relaxed checking.
4. Run all regular scaling families and adversarial termination families. Count failed probes, diagnostic work, dependency edges, and module solves. A smaller runtime number does not compensate for superlinear graph work.
5. After a substantive change, rerun correctness and the full affected counter/scaling cohort. Final timed reports require a consistent final binary/configuration for every workload; incompatible runs cannot be assembled into a fictitious passing build.
6. Apply the finite no-progress rule. Preserve complete failing results and the closest verified implementation if the mandatory budgets cannot be met.

**Owner focus:** Shared solver/row/requirement machinery, frontend finalization, indexed evidence/fast paths, profiling owners and benchmark harness.

**Deliverables:** Small raw timing sets with ordinary medians and observed ranges; production RSS and unique retained-byte accounting; allocation/operation counts; regular/adversarial scaling results; generic evidence cost and code-size report.

**Pass conditions:** Required observations match and no unexplained material performance regression remains; all required regular scaling and deterministic termination conditions pass; within-bundle reuse counts are correct; instrumentation/product distinctions are explicit. Small timing differences are investigation context, not precise percentile gates.

**Retirement/failure:** Remove measurement-only production overhead or keep it behind existing profiling configuration. Do not raise limits, change workload inputs, reinterpret cold, specialize by fixture identity, or relax budgets. Three complete attempts without meaningful progress on the same condition end the run as incomplete with exact measurements.

### Gate G: consolidation, allowlisted migration, and final report

**Inputs:** Passed F; proven joint removals; semantic stabilization patches; remaining adapter/deletion inventory.

**Work:**

1. Apply only allowlisted, reviewed semantics-preserving maintained-source removals and purposeful stabilizers. Keep externally meaningful schemas, validation, domain/public/effect promises, and annotated-form tests. Review the complete patch and normal checked/elaborated observations; no whole-tree lint-fix pass.
2. Remove obsolete concrete/private-only inference repair paths, cloned checker probes, semantic lowerer binders, duplicate effect/signature logic, actual permissive modes, temporary adapters/switches, and repeated retained Type trees. Keep necessary schema/ABI/protocol/ownership verification.
3. Inspect callers of retired symbols and remaining type-reconstruction helpers. A renamed helper that still rediscovers semantics is not a deletion. Bounded display/serialization views of solved facts may remain with explicit ownership and measurement.
4. Update canonical specifications, architecture/frontend docs, Test Map, API/reveal output, and relevant existing examples. Document intentional remaining annotations and supported ambiguity/generalization boundaries. Do not create a second authoritative typing manual or rebuild generated docs.
5. Search touched source comments/docstrings for document names/paths/URLs and planning/gate/milestone labels. Preserve their durable reason/invariant in self-contained domain terms.
6. Build the exact final debug products and run all affected relevant gates. Rebuild profiling products and rerun affected measurement/scaling cohorts after changes that can alter binaries, embedded modules, semantics, or retained layouts. Final evidence must identify the delivered source/binary state; document-only changes need no invented timing rerun.
7. Reconcile all final artifacts, protected counts, observed platform limitations, removed mechanisms, tested feature configurations, and final runtime/performance/annotation results. Leave a clear local checkpoint and an honest complete or incomplete report.

**Owner focus:** All solved-fact consumers and remaining legacy adapters; allowlisted XSH production sources; canonical docs; final measurement/report owner.

**Deliverables:** One production inference architecture; executable generalized programs; allowlisted migration; canonical contracts; no temporary semantic duplication; final coherent test/measurement/counter/annotation report.

**Pass conditions:** All mandatory A-F evidence still describes the final relevant implementation and remains green; migrations preserve the frozen semantics; deletion searches/caller review confirm one typing truth; final products/tests pass; annotation targets and per-workload budgets remain satisfied. The report distinguishes actual passes, owner-run omissions, platform exclusions, and remaining intentional annotations.

**Retirement/failure:** No permanent old/new checker mode, generic fallback, speculative body probe, dynamic operation search, or redundant semantic binder remains. Unresolved consolidation or migration failures leave the campaign incomplete. Do not mark G passed just because remaining cleanup appears small.

## 13. Performance, memory, and deterministic work

Union-find does not make full-language inference linear. Inferred type/output size, rows, instantiation, operation requirements, diagnostics, and bounded dataflow can dominate. Measure the complete path and claim only observed properties.

### 13.1 Phase and memory definitions

Instrument these boundaries using existing profiling facilities where possible:

1. Source/import load and parse.
2. Name/declaration dependency resolution.
3. Constraint generation, including source visits.
4. Type/row/operation/effect solving.
5. Generalization and immutable module interface production.
6. Finalization and consumer-view materialization.
7. Indexed lowering, evidence preparation, and verification.
8. Program execution, including preparation-independent runtime evidence handling.

Frontend check time includes all applicable work through finalized checked facts, not only unification. Startup through verified preparation includes every step needed before the first ordinary instruction. Runtime workloads separately measure execution and total invocation/startup so work cannot migrate between categories unnoticed. Record the old pipeline's corresponding phase ownership explicitly; a phase absent in the old compiler is not a license to omit it from the candidate total.

Track allocated type/row/scheme/constraint/reason nodes; unifications; occurs/level visits; instantiations; row visits; candidate tests including rejected probes; queue/wakeup/revisit counts; effect edge visits; source/body visits; module loads/solves/interface instantiations; flow fixed-point visits; and diagnostic rendering. Record both total work and family-specific counters so redistributing work cannot fool one headline counter.

Record allocation count/bytes and peak live bytes in profiling products, peak process RSS in production products, retained solved/interface data, and prepared instruction/evidence size. Count unique graph nodes plus capacities/backing storage, watcher/trail/reason tables, retained views, and duplicated snapshots. Distinguish temporary construction from retained frontend and after-frontend-drop memory.

Use the same accounting definitions for both implementations. Keep instrumented allocation traffic distinct from product timings/RSS; worker-local pressure is not process RSS. Record collector units and feature state. Missing counters are unavailable, never zero by assumption. Ordinary products must not install a new global allocator or collect expensive per-operation diagnostic/profiling data by default.

### 13.2 Lightweight sampling protocol

- Baseline source/product observations already collected are sufficient to begin implementation.
- For subsequent timing comparisons, use five measured fresh-process observations per build/workload and one warmup. Alternate build order when comparing two builds.
- Record the ordinary median and observed range. Do not claim p95 or statistical proof from the small sample.
- Preserve raw samples, output fingerprints, source/binary/config identities, and available host conditions. Keep slow valid samples and explicit failures visible.
- Concurrent builds or unavailable thermal/power/process observations do not prevent execution. A quiet machine is not required.
- Repeat only the affected measurements when a concrete regression or implementation change warrants it; no complete 200-sample reruns or noise-calibration campaign is required.
- Each cold check still uses a fresh process; within-bundle reuse is allowed and required. Filesystem caches may be warm and are not claimed flushed.
- Keep production timing/RSS separate from allocator-instrumented profiling. Frozen runtime inputs, exact observations, process cleanup, and resource guards remain unchanged.

### 13.3 Performance regression comparisons

Compare baseline and candidate on identical stabilized annotated sources, candidate annotated and jointly reduced sources, and generic helpers with handwritten monomorphic equivalents. Include cold frontend, preparation/startup, representative runtime, peak RSS, and retained solved/interface memory. Use the frozen workloads; report unavailable attribution honestly.

Treat measurements as practical regression signals. Investigate large or reproducible slowdowns and unexpected memory growth. Precise percentage/absolute timing ceilings and mandatory p95 acceptance are removed; small timing differences do not block semantic implementation. Do not advertise an improvement percentage without matching source/product observations.

Generic comparators retain the same algorithm, data shapes, loop/call counts, effects, and observations. Keep prepared instruction/evidence counts and bytes visible. Evidence scales with canonical requirements/necessary distinct instantiations, without one body per incidental shape or metadata allocation proportional to runtime items/calls. No invocation-time inference and bounded code remain hard requirements independently of timing.

### 13.4 Regular scaling and adversarial limits

Generate fixed-seed regular families at 1k/2k/4k/8k source units, keeping grammar and declared relationships comparable:

- Long immutable binding/alias chains.
- Repeated generic instantiations and forwarded requirements.
- Wide records with repeated projection/update and preserved rows.
- Nested container constraints.
- Independent declarations and ordinary recursive effect/type components.
- Module diamonds with measured once-per-bundle dependency solving.
- Locally resolvable overload chains and valid nearly-matching candidates.

Record input bytes/nodes, unavoidable normalized output DAG size, every relevant work counter, and retained graph/constraint bytes. Do not normalize by avoidable duplication or choose generators whose useful output is artificially constant while claiming broad linearity.

For the final two doublings of each regular family:

- Total and applicable family-specific solver work must grow by at most 2.6x.
- Retained type/constraint memory must grow by at most 2.5x.
- Investigate wall-time growth above 2.8x; it cannot be dismissed if counters moved work into an uncounted phase.

Explain necessary output growth explicitly and retain raw counts. Regular families stay below frozen resource guards; hitting a guard fails the family. Effect convergence cannot perform whole-program rescans per call edge. Module diamonds must expose actual dependency counts, not a stale cache hit.

Separate adversarial families cover unresolved overload chains, huge inferred expansion, deep rows/containers, recursive aliases/equations, invalid almost-matching calls, captured-state traps, and diagnostic fanout. Each must terminate deterministically under frozen node/work/depth/output limits, with a local reason, no stack exhaustion, no unbounded retry/printing, and no Any acceptance. Not every pathological source must be accepted or fully printed.

## 14. Required acceptance witnesses

Create self-contained fixtures in the supported grammar, with declaration omission enabled by this campaign. Prefer native behavior tests named after the relationship or boundary. Reuse nearest modules for existing behavior; add a focused inference module for new coordinated cases. Use Rust only for solver/reference internals, exact compiler/process/byte boundaries, profiling, and indexed verifier/lifetime contracts that native tests cannot own.

These witnesses are mandatory; their generated variants augment them rather than replacing them:

| Witness | Positive/negative evidence and observable contract |
|---|---|
| Generic identity | Int, Str, false, Optional, and nested Result instantiations coexist; no caller changes another signature; false is a returned value. |
| Generic statement use | Unresolved non-tail `value` requests an explicit discard/contract; `let _ = value` has fixed discard behavior; explicit `?` still propagates. |
| Row projection | Distinct narrow/wide records and field types share one body; missing/wrong nested fields reject; unrelated fields remain preserved. |
| Row equality versus width | Width-compatible argument acceptance does not collapse actual rows or bypass invariant containers; duplicate/impossible/infinite rows reject. |
| Optional/presence | Nullability and absence remain distinct; a deliberate absence handler does not gain an unconditional required field. |
| Unique builtin inference | `pure parse(value) { value.parse_int()? }` obtains its independently supported receiver, success/error, and pure contract. |
| Explicit dynamic schema | `proc read_manifest(path) { json.read(path)?.require(Manifest)? }` infers operations/return/effects while Manifest remains explicit validation. |
| Empty/nullable accumulation | Empty List/Map and null-initialized mutable locals solve from consistent body constraints; earlier reads share the same fixed lifetime type; incompatible writes show both origins. |
| Safe versus unsafe generalization | Safe named/immutable relationships and empty immutable collections generalize; mutable storage, captured metavariables, and stateful callable factories remain monomorphic where required. |
| Concrete ambiguity | Unresolved executable overloads, non-reifiable validation, and underconstrained error-only success boundaries request local annotations rather than guessing. |
| Defaults/rest/context | Checked defaults, body constraints, required/rest slots, nested expected types, labels, and spreads preserve exact scope/order/laziness and parameter relationships. |
| Constructors/constants/descriptors | Generic record construction, phantom arguments, imported constant keys/descriptors, aliases, and inline forms retain checked schema/application facts; defaults do not manufacture evidence. |
| Callable value flows | Compatible conditions, fields, containers, parameters, returns, and mutable slots retain monomorphic signatures; incompatible contracts reject; explicit erasure stays dynamic. |
| Rank-1 restriction | No callback parameter or element gets independent forall instantiation; attempted higher-rank/polymorphic-recursive use rejects with a local explanation. |
| Latent callable effects | Selection/storage/return does not execute callable host work/defaults; invocation contributes the retained effects and obeys explicit bounds. |
| Callable identity | Computed callable signature precision does not invent a unique declaration; static stage descriptor restrictions remain, while an existing block can invoke a typed computed callable. |
| Effects and kind | Transitive/recursive/effect-variable relationships work; `[]` rejects host effects; unknown dynamic effects are not empty; empty inferred proc effects do not make it pure. |
| Producer lifecycle | Item and latent-effect inference preserve creation/first pull/default/yield/delegation/cancellation/cleanup timing; no hidden collection/materialization. |
| Result elaboration | Tail/early/explicit/omitted returns, implicit Ok, explicit nested Ok/Err, propagated failures, generic Result payloads, and try/retry targets follow the matrix. |
| Control and cleanup | Native tests/explicit Unit bodies retain assertions; lexical return/break/continue/defer/cancellation and process Status observations remain unchanged. |
| Refinement provenance | Nested paths, immutable Boolean aliases, snapshots, short circuits, continuation joins, loops, sibling writes, capture, shadowing, and unknown calls cannot revive stale proof. |
| Dynamic laundering | Any/erased Record/callables cannot establish concrete rows, nominal/domain types, resource promises, or restricted effects without the appropriate explicit boundary. |
| Nominal/UInt/resource | Generic helpers cannot conflate imported nominal families, erase UInt storage checks, leak private identities, or escape handles/producers past their established lifetime. |
| Module schemes and reuse | Exported inference is independent of client types/order; diamond dependencies solve once; equal spans/aliases have correct ownership; a fresh bundle sees changed dependencies. |
| Indexed evidence | Generic bodies execute after frontend disposal on both routes; malformed scopes/slots/requirements/evidence reject; prepared pool rewind and lifetime remain correct. |
| Refactor invariance | Transparent aliases, local renaming, safe independent declaration ordering, and descriptor extraction preserve relationships, overloads, elaboration, and observations. |
| Tooling contract | Reveal/API display schemes honestly; source annotation output parses; removal accepts eligible generalization; rechecking rejects unsafe/partial edits. |
| Deterministic complexity | Regular scaling stays within work/retention gates; pathological equations/overloads/diagnostics terminate locally without fallback or host-stack failure. |

For example, the inferred declaration fixtures should include:

```xsh
pure identity(value) {
  value
}

pure name(entry) {
  entry.name
}

pure parse(value) {
  value.parse_int()?
}

type Manifest = {
  name: Str,
  version: Str,
}

proc read_manifest(path) {
  json.read(path)?.require(Manifest)?
}
```

Run them in fixtures that actually call both useful and deliberately invalid instantiations. Check full inferred relationships/effects and execute applicable cases. Do not replace their explicit external Manifest contract with an inferred row.

Existing nearest native owners include `tests/xsh/local-inference.xsh`, `builtin-templates.xsh`, `dynamic-boundaries.xsh`, `private-pure-inference.xsh`, `private-proc-effects.xsh`, `inferred-require.xsh`, `proof-provenance.xsh`, `assertions.xsh`, `value-blocks.xsh`, `default-parameters.xsh`, `record-update.xsh`, `typed-map-keys.xsh`, `stage-functions.xsh`, `yield-delegation.xsh`, and context/Result/stdlib modules. Re-audit names in the settled snapshot. Preserve coverage of written annotations when an old annotation-required rejection becomes a valid inference example; retain a genuinely invalid replacement for the original boundary.

## 15. Verification commands and permitted checks

Use the nearest hard judge first: compiler build, focused solver/reference test, native behavior test, exact semantic fact assertion, or indexed verifier witness. Broaden to the complete relevant gates after focused checks pass. Once those gates pass, repeat/broaden only after changes or unresolved evidence justify it.

Typical targeted debug products:

```sh
cargo build -p xsh --bin xsh
cargo build -p xsht --bin xsht
cargo build -p xshi --bin xshi
```

Typical syntax/checker/registry/API and indexed gates, adjusted to actual ownership and authorized test contents:

```sh
cargo test -p xsh --test integration syntax::
cargo test -p xsh --test integration sema::
cargo test -p xsh-registry --lib
cargo test -p xsh --lib runtime::eval::indexed::full::tests --features native-tests
cargo test -p xsht --test api
cargo build -p xsh --lib --no-default-features
```

Start native checks with the touched modules and exact freshly built xsht path, for example:

```sh
target/debug/xsht test --jobs 1 tests/xsh/local-inference.xsh
target/debug/xsht test --jobs 1 tests/xsh/builtin-templates.xsh
target/debug/xsht test --jobs 1 tests/xsh/dynamic-boundaries.xsh
```

Then run the relevant native stdlib and broader runtime groups. Preserve owner-run exclusions:

```sh
cargo test -p xsh --test integration runtime:: -- \
  --skip runtime::coverage:: \
  --skip runtime::examples::example_corpus_is_formatted \
  --skip runtime::examples::example_corpus_lints_without_warnings \
  --test-threads=1
```

This runtime command is a starting filter, not permission to execute prohibited formatter/linter CLI cases hidden inside tests. Inspect selected groups and add exact exclusions for such cases, recording them as owner-run. Likewise, run focused xsht library-backed check/API/edit/grep/refactor/lint/format tests only after confirming they stay within authorized library/isolated fixture behavior. Do not use an unfiltered xsht integration suite as a shortcut around CLI restrictions.

Build exact helper binaries that the selected native/process tests require. Set/record absolute product paths and build output ownership when tests assume `target/debug`. Do not accidentally run the shared checkout's stale binaries. Cover default features and supported feature-disabled core contracts; record which registry inventory is enabled in each configuration.

For profiling only, build exact optimized products, such as:

```sh
cargo build -p xsh --release --bin xsh
cargo build -p xsht --release --bin xsht
cargo build -p xsh --release --bin xsh-frontend-stats
cargo build -p xsh --release --bin xsh-runtime-stats
```

Integrate the existing stats products or their nearest equivalents; ordinary runtime timing uses production binaries, not the counting allocator. No bare release build, dist profile, Linux image/toolchain substitution, or release-packaging task is part of this campaign.

Record every actually run command, configuration, exit status, and relevant executed counts. Include full relevant gates chosen from the settled `docs/TEST-MAP.md`, not only the representative commands above. Explicitly name what was not run and why. Pre-existing failures need frozen evidence; new failures cannot be relabeled pre-existing.

## 16. Stop, checkpoint, resume, and completion

### 16.1 Finite failure policy

Do not stop merely because work is substantial or an early implementation failed. Use focused evidence to fix ordinary failures and continue. The fixed architecture and explicit semantic rules eliminate the need to ask the user to choose a solver or redefine the target halfway through.

A complete recovery/optimization attempt consists of:

1. Identifying a specific failing mandatory condition and recording its current evidence.
2. Applying a reasoned change or testing a concrete alternative within the fixed contract.
3. Running focused correctness checks and the full affected workload/counter/scaling cohort needed to assess the change.
4. Recording the outcome, regressions, and whether the same condition improved.

Meaningful progress means a previously failing mandatory condition passes, or its remaining measurable failure is reduced reproducibly beyond observed noise without introducing an equivalent new failure. Logging, speculative edits, new exclusions, an isolated best time, and an unverified belief are not progress.

After three consecutive complete attempts without meaningful progress on the same mandatory condition, stop. Preserve the failing regression/results, the smallest coherent implementation, and a compiling checkpoint that passes the checks already established for its completed gates. Record separately any unfinished attempt that cannot yet compile; identify the last green checkpoint and leave a reviewable patch rather than destructive reset/cleanup.

Unavailable stable source, missing required environment/build inputs, a necessary dependency authorization, or a semantic requirement that contradicts the fixed contract can block work before three useful attempts exist. Report that exact prerequisite instead of inventing progress, continuing dependent changes, or choosing unauthorized semantics. An explicit external time/compute limit also requires a truthful checkpoint when reached. No default total duration is imposed; the no-progress rule provides a finite stop for a stalled gate.

An incomplete checkpoint must state:

- Current gate and last passed gate/checkpoint.
- Source/binary/config identities and exact command reproducing failure.
- Expected versus actual behavior or statistic, including the budget/denominator and raw evidence.
- Attempts made and what each established.
- Which artifacts/tests/migrations are complete and which remain pending.
- The next concrete action or required external decision/environment change.

Never raise limits, loosen budgets, reclassify eligibility, remove a frozen workload, silently defer required runtime/family coverage, or mark completion to end a difficult run. Follow-up resumes this checkpoint rather than restarting the baseline or discarding prior evidence.

### 16.2 Completion checklist

Completion requires all of these:

- One production bidirectional type/row/callable/effect inference architecture over compact shared data, with valid generalization and explicit dynamic/recovery distinctions.
- Declaration-owned rank-1 schemes, row helpers, inferred exports/effects/items, local lifetime inference, and required callable flows across the frozen operation inventory.
- Fixed checked assertion/Result/conversion/default elaboration, bounded sound refinements, and preserved nominal/resource/dynamic validation contracts.
- Shared generic indexed execution/evidence on both normal routes, verified after frontend disposal, with no invocation-time inference/search/compilation or AST fallback.
- Mandatory within-bundle module reuse with counted correct identities; across-edit caching remains explicitly deferred.
- Frozen original/stabilized/reduced cohorts, independent reference/negative/runtime/refactor witnesses, honest joint 95%/85% reduction results, and allowlisted maintained migration.
- Lightweight performance regression observations plus passing regular/adversarial work/scaling gates, with raw data and exact product identities.
- Removal of temporary semantic adapters, checker/body probes, lowerer signature/binding reconstruction, whole-graph repair scans, and actual redundant modes/representations.
- Canonical documentation and honest executed-test/feature/platform/owner-run accounting.

Finish with `bench/typing/README.md` as the campaign checkpoint/final report. Lead with complete or incomplete status and the concrete result. Include per-gate status, frozen denominator counts, intentional remaining annotations, semantic stabilization, scope exclusions, exact verification, per-workload measurements, code/evidence size, deletion inventory, and any blocking condition. Use machine-readable artifacts for the raw detail.

Fewer annotations alone, accepted parser syntax, permissive Any, checker-only schemes, a permanent second checker, unmeasured speed claims, or a passing cohort average do not satisfy this deliverable.

## 17. Reference reading

Adopt mechanisms that support the fixed architecture; do not import whole languages or broaden the campaign while reading.

- [Dunfield and Krishnaswami, Bidirectional Typing](https://research.cs.queensu.ca/home/jana/papers/bidir-survey/): synthesis/checking discipline, expected-type propagation, and error locality; no higher-rank feature request.
- [Kiselyov, Efficient and Insightful Generalization](https://okmij.org/ftp/ML/generalization.html): levels, environment dependency/escape, sharing, and avoiding environment-wide scans. Start with the simplest sound implementation and optimize traversal with measured evidence.
- [Leijen, Extensible Records with Scoped Labels](https://www.microsoft.com/en-us/research/publication/extensible-records-with-scoped-labels/): row inference background; XSH retains unique labels and its existing record/update semantics.
- [Leijen, Koka: Programming with Row-Polymorphic Effect Types](https://www.microsoft.com/en-us/research/publication/koka-programming-with-row-polymorphic-effect-types/): type/effect relationships; no effect handlers or new runtime model.
- [Dolan and Mycroft, Polymorphism, Subtyping, and Type Inference in MLsub](https://www.repository.cam.ac.uk/items/b7009a90-9bc6-4538-a388-90a2d5617de0): context for richer subtyping; unrestricted union/intersection inference is outside this campaign.
- Swift compiler authors, [Roadmap for Improving the Type Checker](https://forums.swift.org/t/roadmap-for-improving-the-type-checker/82952) and [Recent Improvements to the Type Checker](https://forums.swift.org/t/recent-improvements-to-the-type-checker/87048): finite candidate pruning, invalid-expression behavior, deterministic operation counts, and scaling tests. Do not adopt general backtracking overload search.
