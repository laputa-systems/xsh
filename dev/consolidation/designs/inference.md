# First rank-1 inference slice

This design covers the first end-to-end rank-1 inference checkpoint in the
approved successor work. It adds no syntax beyond omitted parameter and
return annotations. It does not add polymorphic recursion, open effects,
overload search, inferred mutable polymorphism, or automatic checks at dynamic
boundaries.

Capability baseline `b6d4a6d1a` has outer `xsht check` status 0 and six focused
native tests: two pass, four fail. Inferred positives and both inferred
negatives fail at `[parse.expected-param-type]`. Import tests also report
`[parse.module-load]` because the shared module has an unannotated parameter.
The annotated missing-field control and existing `Any`-to-`Int`
`[check.dynamic-boundary]` case pass. Solver baseline `6f53397b8` has a
separate exact red: `alias_chain_of_400_variables_resolves_to_ground_type`
returns `Err(TypeDepth)` instead of `Ok(Int)` on tree
`840e8864f1350cae01ac82549883daa713e81c36`. The other fifteen constraint
unit tests pass. Neither result makes a timing claim or supplies the post-
consolidation performance baseline.

The user has authorized isolated inference implementation while Phase A
consolidation continues. This supersedes the original phase ordering for
implementation only. The integrator owns shared-path merges after the
consolidation owner reviews the boundary. The post-consolidation performance
baseline still waits for Phase A acceptance.

## Accepted behavior

The checker accepts a private identity function at each independent use:

```xsh
pure identity(value) { value }

test identity_is_polymorphic {
  identity(42) == 42
  identity("demo") == "demo"
  identity(false) == false
}
```

Its scheme is `forall T. T -> T`. The checker infers the declaration before
checking callers. Each call gets fresh type variables. A caller cannot change
the scheme seen by another call or importing root.
The `forall` notation describes checked metadata. It adds no source syntax.
This first slice supports direct calls to rank-1 pure declarations. It does not
make generic function values, aliases, or nested generic calls.

The checker also accepts an open record projection:

```xsh
pure name_of(value) { value.name }

test projection_requires_name_and_keeps_other_fields {
  name_of({name: "package", version: "1.0"}) == "package"
  name_of({name: 42, enabled: true}) == 42
}
```

Its scheme is `forall T, R. {name: T | R} -> T`. `R` retains every additional
field. A closed record without `name` fails during checking with
`check.unknown-field`. The checker must not infer `Any` for the receiver.

An exported identity declaration is checked once and imported by two distinct
entry roots. One root calls it with `Int`. The other calls it with `Str`. Each
root also checks after the other importer is removed or reordered. The
remaining root's signature and result do not change.

The first slice rejects `identity("demo") + 1` with
`check.type-mismatch`, and rejects `name_of({version: "1.0"})` with
`check.unknown-field`. Calling identity separately with values of different
types is a positive case, not a rejection case. Keep the existing
`Any`-to-`Int` dynamic-boundary witness as the dynamic negative.

The first implementation handles pure function schemes. It does not infer
effect variables or generalize proc/stream declarations. Existing annotation,
effect, public `Result` error, argument-label, default, rest-parameter, and
callable-kind rules remain in force. In particular, an omitted parameter
annotation is new inference input, while an explicit `Any` remains a dynamic
contract and must not become a unification variable.

The first checkpoint leaves the approved broader acceptance in force. Later
inference coverage includes aliases, declaration reordering,
alpha-renaming, nested and optional record fields, mutation-invalidated
refinements, host-signature anchors, contextual inference, monomorphic mutable
accumulators, recursive components, higher-order callable and effect
relationships, and explicit public annotations. Overload candidates stay with
their existing owner. Ambiguous concrete operations remain errors. The corpus
and Phase E scaling gates remain separate acceptance work.

## One inference authority

`src/sema/types.rs::Type` already distinguishes `Inference`, `Any`, `Unknown`,
and `Invalid`. Keep those meanings separate. `Inference` is a temporary solver
reference. `Any` is a dynamic value. `Unknown` and `Invalid` are checker
recovery values. Never publish an unsolved inference reference as a checked
fact.

Replace the binding-and-alias algorithm behind
`src/sema/constraints.rs::TypeConstraints`. Do not add a second solver beside
it. The existing implementation stores each variable as an optional
recursive `Type` binding, chooses the lower variable ID when it joins two
variables, and follows bindings during resolution. Its `MAX_TYPE_DEPTH` limit
also counts alias hops. The solver test module already covers provenance,
occurs rejection, rollback, and foreign-variable rejection.

The first solver change replaces alias links without changing the
`TypeConstraints::{constrain,resolve,resolve_in_place}` signatures. The solver
owner owns all of `src/sema/constraints.rs`, including its tests. Keep
`Type::Inference(TypeVariableId)` as the transient adapter. Give each solver
cell a parent, rank, origin, contribution, annotation authority, and
representative-only structural binding. Do not add levels to this alias
prerequisite. Add levels to these same cells when scheme generalization starts.
Union by rank and path-compress parent links. Preserve the canonical exposed
identity if callers depend on the current lower-ID result. If both union roots
carry bindings, send them through the same directional unifier.
Keep `ConstraintResolutionError` variants unchanged. Detect representative
cycles separately from semantic-depth exhaustion, even if both map to the
existing public error.

`resolve(&self)` needs compression without changing its callers. A parent
`Cell` and transaction-local undo journal allow read-only resolution to
compress. Resolution outside a constraint need not retain undo entries.
During `constrain`, journal every parent, rank, binding, annotation, and
provenance change after the operation marker. Failure restores every entry,
including compression writes. Success discards those entries. Journal levels
when the solver adds them. Clone cells and an idle journal by value so
`constraint_probe` copies remain independent. Capture conflict provenance
before mutation using the existing expected-first rule. Preserve the earliest
origin, first establishing contribution, and annotation authority regardless
of root rank.

Keep `MAX_TYPE_DEPTH = 256` for semantic structure. Representative traversal
must detect parent cycles separately and must not count alias hops as nested
type structure. Do not raise or remove the existing limit. This narrow change
fixes the observed 400-variable correctness failure. It establishes no speedup
or performance budget. Later generalization adds levels to these same cells.
Do not add a parallel solver.

Keep the existing `TypeConstraints` entry point. Extend its solver-owned graph
after the alias prerequisite. Project solved ground types into the existing
public and diagnostic `Type`. The first slice needs these distinct forms:

- A type metavariable has a representative, rank, level, origin span, and
  contribution span. Representative lookup compresses paths.
- A structural record node stores sorted unique fields and a closed or open
  row tail. A row metavariable has its own representative, rank, level, and
  lacks-label set.
- A quantified type or row parameter is an immutable binder in a solved
  scheme. It is never a mutable solver cell.
- A rigid variable is a skolem used when checking a quantified scheme. It
  cannot unify with a concrete type or another distinct rigid variable.
- `Any` remains the existing dynamic type. It cannot anchor a type or row
  metavariable without an explicit dynamic-boundary operation.

The checker remains the sole owner of syntax-directed constraint generation.
The solver owns representative mutation, unification, provenance, generalize,
and fresh-instantiation operations. Scheme binders are immutable IDs scoped to
one scheme. Rigid variables are read-only during checking. Only metavariables
and row variables participate in representative mutation. None of these
internal IDs enters a compact fact as `Type::Inference`.

Keep the roles visible in types and storage. `TypeVariableId` identifies a
mutable type cell owned by `TypeConstraints`. `RowVariableId` identifies a
mutable row cell. `SchemeTypeParameterId` and `SchemeRowParameterId` are
immutable binders owned by a checked scheme and its call substitution.
`RigidTypeId` and `RigidRowId` exist only in a temporary skolem-checking scope.
The checker owns that scope. `TypeConstraints` may compare rigid IDs but never
mutates them. They are not union-find cells. `Any` remains the ground
`Type::Any` value. No conversion between these roles is implicit.

Put immutable scheme data in `src/sema/schemes.rs`. `TypeScheme` is the one
callable contract. It owns quantified binders, ordered parameter labels,
parameter and return `SchemeType`s, schema expectations, default and rest
flags, and the callable effect contract. `FunctionSig` in `src/sema/check.rs`
owns declaration identity and checking metadata plus one `Arc<TypeScheme>`.
It does not keep another parameter, return, or effect list. The existing
`CallableType` and `CompactFunctionSig` remain ground projections. They do not
represent generic functions.

`CheckSession::check_modules` passes a checked module's `UserModuleSig` to
later modules. Keep that as the only checker import handoff. Its concrete
`FunctionSig` carries the immutable scheme used at the declaration.
`CheckedModuleFacts` keeps the existing `UserModuleSig` cache. Do not add a
second checker-side scheme table. For lowering, `CompactDeclOutput` publishes
the same `Arc<TypeScheme>` under each qualified export.
`CompactModuleGraph::from_file_unit` copies that reference into its qualified
scheme index. It must not rebuild a generic contract from `CompactFunctionSig`
or use `Any` as a placeholder.

`SchemeType` has ground types, type binders, and record rows. Its open-row tail
refers to a `SchemeRowParameterId`, never a mutable `RowVariableId`. Row
binders retain their lacks-label constraints. `TypeScheme` construction rejects
`Inference`, `Unknown`, and `Invalid`. An explicit `Any` may remain a ground
contract type. It cannot create or anchor a quantified binder. Use
`CallInstantiation` for one direct call's solved type and row substitutions.
Use `RowEvidence` for required and retained record fields. These values contain
no mutable solver cell. Keep generalization, instantiation, and unification in
`TypeConstraints`.

Publish generic function body types in an immutable `SchemeBodyFacts` record.
Key `CompactDeclOutput.scheme_bodies` by the declaration's existing body
`Span`. Each record holds the same `Arc<TypeScheme>` and maps that function's
local `ExprId`s to `SchemeType`s under its binders. The checker abstracts
solved representatives into those binders before it drops solver state. These
facts do not replace ground `CompactBodyFacts.expr_types` for importing-root
expressions. Publish them only for generalized declarations. They cannot
contain solver IDs, rigid IDs, `Any`, `Unknown`, or `Invalid`.

Store ground substitutions and row evidence in
`CompactBodyFacts.call_instantiations`, keyed by call `ExprId`. Keep call
targets and argument slots in the existing `selected_function_calls` and
`argument_bindings` facts. Do not copy those decisions into
`CallInstantiation`. For this slice, generic bodies contain parameter reads,
required-label projections, and a tail returning one of those values.

The lowerer consumes each `SchemeBodyFacts` once and emits the generic body
once. Encode its binder-scoped types with the function in `FullProgram`.
`FullVerifier` checks binder scope, body expression types, field projections,
and the declared return scheme once under that binder environment. At each call
it checks the existing target and argument slots, then validates the concrete
type and row substitution. Reject an out-of-scope binder, missing required
field, inconsistent parameter or return type, or escaped inference ID. The
executor reads only the verified program and performs no inference.

Unification operates on representatives and shared structural nodes. It checks
the occurs condition before binding a type or row variable. Binding lowers the
levels of reachable variables. Each public constraint starts a trail marker.
Failure restores every parent, rank, level, binding, and lacks-label update
after that marker, including path-compression writes. Alias traversal and
semantic type nesting get separate limits. Do not raise `MAX_TYPE_DEPTH` to
make a variable chain pass.

Unifying a metavariable with a ground `Type` still follows the checker's
directional assignability rules. Preserve annotation and context authority,
invariant containers, optional values, validated types, nominal types, unions,
and `Result` error compatibility. Do not widen one ground type to another
because a later contribution happens to fit.

Row unification matches common labels by unifying field types. It places labels
found on one side into the other side's open tail. It reports a conflict when a
closed row lacks a required label or a row violates a lacks-label condition.
The row retains unmatched fields. This implements required-field projection,
not record subtyping or structural recursion.

Existing `Type`, `CallableType`, and module signatures remain ground
projections where the public diagnostic or runtime contract needs one. Do not
make `Type::Any` or `Type::Unknown` stand in for a quantified variable.
Generate each body's obligations once. Use a dependency worklist so a changed
representative wakes only the obligations that depend on it. The current
default-parameter pass in `src/sema/check/infer_param.rs` can revisit every
statement up to `count + 1` times. Do not extend that pass pattern to general
inference.

## Current source hooks

`src/syntax/parser/stmt.rs::parse_params_arena_only` currently emits
`ParseExpectedParamType` for a required unannotated parameter.
`ArenaParam` currently stores a mandatory `TypeExprId` plus `ty_defaulted`.
Change the syntax fact to `annotation: Option<TypeExprId>` and remove
`ty_defaulted`. `None` means the author omitted the annotation. It is distinct
from explicit `Any`, parser recovery, and a solver metavariable. A default
remains separately represented by `default`. `None` with a default keeps the
existing default-derived typing. `None` without a default enters declaration
inference only for pure definitions in this slice.

Pass the known declaration context into `parse_params_arena_only`. Accept a
required omission without a parser diagnostic for pure definitions. Keep
`ParseExpectedParamType` for proc and stream declarations, module contracts,
and callable type expressions. Store `None` in the syntax tree even when that
diagnostic is emitted. Do not fabricate an `Unknown` type expression for an
omitted annotation. A malformed written annotation remains a parse error and
does not become `None` recovery. Preserve `tests/xsh/mistakes.xsh`'s
`proc add(a, b)` diagnostic.

`ArenaFunctionDef.return_ty_defaulted` distinguishes an omitted return
annotation from explicit `Any`. Keep it because `infer_return.rs` reads it. Do
not replace either case with `Unknown` or an inference metavariable.

Keep omitted return behavior by declaration kind. An omitted pure return may be
inferred for the acyclic rank-1 schemes in this slice. An omitted proc return
continues to mean `Result[Unit]`. Stream returns and module-contract returns
keep their existing requirements. An unannotated recursive pure return still
requires an explicit annotation. `ArenaFunctionDef.return_ty_defaulted`
continues to distinguish omission from explicit `Any`.

Migrate every syntax-parameter reader in the same compileable change:
`src/syntax/arena.rs`, `src/syntax/parser/stmt.rs`,
`src/syntax/parser/types.rs`, `src/sema/types.rs`,
`src/sema/cli_entry.rs`, `src/sema/check/{decl,infer_param,stmt,types}.rs`,
`src/runtime/eval/lower.rs`, `src/runtime/eval/lower/types.rs`,
`src/frontend_stats.rs`, and `tests/sema.rs`. Keep
`CallableParamType::defaulted` as the argument-default fact. It does not encode
an omitted annotation. CLI entry validation keeps its existing contract.

`src/sema/check/infer_param.rs::infer_default_parameter_types` resolves
defaulted parameter types from their defaults and signatures already in scope.
It does not infer relationships from a function body. The new pure declaration
pass supplies body relationships for omitted required parameter types. Keep
recursive pure functions monomorphic and explicitly returned in this slice.

`src/sema/check/module_driver.rs::CheckSession` prepares reusable module
records and passes each checked module's `UserModuleSig` to its importers.
`UserModuleSig` remains the sole checker handoff. Its concrete `FunctionSig`
contains the declaration's `Arc<TypeScheme>`. `CheckedModuleFacts` keeps that
existing signature cache. Do not add another scheme cache.

`src/sema/check/checked_declarations.rs` and
`src/sema/check/compact.rs` publish the same scheme reference and
`SchemeBodyFacts`. `CompactDeclOutput` indexes schemes by qualified export and
body facts by body `Span`. `src/loader.rs::CompactModuleGraph::from_file_unit`
copies those references for lowering. It keeps `CompactFunctionSig` as the
existing ground signature projection.

`src/runtime/eval/lower/calls.rs::lower_call` reads checked call targets,
argument bindings, expression types, and call instantiations.
`src/runtime/eval/indexed/semantic.rs::SemanticPools` encodes closed `Type`
values and callable signatures. The verifier in
`src/runtime/eval/indexed/full.rs` checks those pools and encoded calls.
`src/sema/types.rs::CallableType` remains a concrete projection with parameter
labels, types, default and rest flags, return type, and optional effects.
`Checker::check_function_arena` also records `result_unit_function_tail` and
compact `statement_positions`. These facts determine whether a body tail is a
result value or a statement. Keep them fixed by the checked declaration and
call instantiation. The Bool
identity call must lower and verify as Bool, including a false value consumed
by a test assertion. Generic lowering must not choose `Unit`, result wrapping,
propagation, or storage conversion from a runtime tag or from a different
caller's instantiation.

## Generalization and calls

At a pure declaration boundary, generalize only type and row representatives
above the environment level that are not reachable from the environment. The
first slice generalizes pure function declarations, not local values. Keep
`var` bindings and captured mutable or resource-bearing state monomorphic. A
mutable accumulator therefore has one type across all reads and writes. Proc,
stream, and effect-variable generalization remain outside this first slice.

Use `src/sema/check/infer_return.rs::ReturnInferenceIndex` to identify
declaration dependencies and recursive components. Do not build a second
permanent declaration graph. When recursive scheme inference is enabled, give
each strongly connected component one provisional monomorphic signature.
Check every body and recursive call against those same representatives. Solve
the whole component before generalizing any member. Reject a recursive type
equation with the normal occurs diagnostic. Do not instantiate a member of its
own component at a recursive call. The first witness set is acyclic and keeps
the current explicit-return requirement for recursive pure declarations.
Monomorphic SCC solving is still a core invariant. Add a checker test that a
recursive component publishes no quantified binder and checks every recursive
call against the same provisional signature. Do not broaden recursive return
inference in this slice.

Instantiate each imported or local scheme once per call. Use one substitution
for all occurrences of each quantified type and row parameter. Publish the
target declaration, argument slots, instantiated argument and result types,
and row evidence for that call. Keep overload candidates and effect contracts
under their existing owners. This slice does not generalize either one.

Explicit annotations remain fixed contracts. They constrain the inferred
monotype before generalization. An exported annotation therefore restricts
the exported scheme instead of being replaced by a caller-specific signature.
The first slice supports inferred schemes for exported definitions only when
the declaration body has a unique, closed rank-1 scheme.

## Publish facts for lowering

The loader and checker already separate reusable module declarations from
entry-specific body facts. `CheckSession` caches `CheckedModuleFacts` by module
identity and passes the checked `UserModuleSig` to importers. `FunctionSig` in
that signature carries the declaration's scheme. `CompactDeclOutput` publishes
the same scheme reference under the qualified exported name and keeps each
generic body fact under its source-qualified body `Span`. `CompactModuleGraph`
copies the scheme reference for lowering. It does not reconstruct the scheme
from `CompactFunctionSig`. The importing root owns one `CallInstantiation`
per call `ExprId` in `CompactBodyFacts`. `CompactFunctionSig` remains the ground
projection for existing concrete contracts. Never use `Any` in that projection
to stand for a generic scheme.

`src/runtime/eval/lower/calls.rs::lower_call` already consumes
`selected_function_calls`, `argument_bindings`, and checked expression types.
Have it encode the published instantiation. It must not revisit the AST to
infer a type or select an overload. The generic function body is compiled once.
Do not specialize a function body from the set or order of visible callers.

The indexed semantic pool currently encodes closed types and callable
signatures. Add quantified type and row binders to the generic function
signature encoding, plus the call's checked substitution and row evidence.
`FullVerifier` validates binder scope and lacks-label constraints, unique row
labels, required fields, call target, argument slots, instantiated parameter
and return types, and function body facts before execution. The executor reads
the verified instruction stream and does no inference. Keep the existing
monomorphic encoding for functions without quantified parameters.

The native acceptance test runs each program through the ordinary loader,
checker, lowerer, and indexed verifier, then executes it through `xsh`. Add an
in-process prepared-program test using the `fixture` pattern in
`src/runtime/eval/indexed/full.rs`, following
`builtin_templates_execute_checked_calls_after_frontend_drop_on_both_routes`.
Explicitly drop parsed and compact declaration values before the helper returns
`FullProgram`. Let checker and solver state leave scope before the test receives
it. Call
`FullVerifier::verify` on the returned program. Execute user identity and
projection calls through both indexed invocation routes. This proves that the
prepared program carries the required scheme and body facts after frontend
state is gone. A subprocess test remains an ordinary-path acceptance test, not
the disposal proof. A focused indexed-IR test forges a call substitution, row
evidence, and scheme body fact and requires `FullVerifier` to reject each one.
The Bool identity case must remain Bool through the call and assertion.
Lowering cannot choose its result from a runtime tag.

Importer outputs do not prove that the loader cached a quantified scheme.
Add a checker test that inspects the cached export and asserts one quantified
type binder shared by identity's parameter and return. Instantiate that scheme
twice and assert that the fresh solver variables differ before unification.
Unify the instances with `Int` and `Str`, then assert that no inference
variable escapes into checked facts. Run the Bool case through ordinary
indexed execution after frontend disposal.

## Exact implementation ownership

Keep two inference lanes before acceptance. The representations cross parser,
checker, lowerer, and verifier boundaries. One end-to-end owner keeps those
interfaces compileable and gives every published scheme fact a consumer. The
integrator owns shared-path merges.

**Union-find prerequisite.** Own `src/sema/constraints.rs` only. Start after the
frozen 400-alias red baseline is recorded. Preserve public methods and error
variants, ranked representatives, rollback, provenance, and the 256 semantic-
depth limit.

**Rank-1 compiler slice.** Own these paths together:

- Syntax and existing parser controls: `src/syntax/arena.rs`,
  `src/syntax/parser.rs`, `src/syntax/parser/stmt.rs`,
  `src/syntax/parser/types.rs`, `tests/sema.rs`, and
  `tests/xsh/mistakes.xsh`.
- Scheme and checker: `src/sema/mod.rs`, `src/sema/schemes.rs`,
  `src/sema/check.rs`, `src/sema/types.rs`, `src/sema/cli_entry.rs`, and
  `src/sema/check/{args,call,callable_alias,checked_declarations,command,compact,decl,expr,infer_effects,infer_param,infer_return,local_inference,module_driver,module_record,public_result,stmt,stream,typed_callable,types}.rs`.
- Import and size accounting: `src/loader.rs` and `src/frontend_stats.rs`.
- Lowering and verification: `src/runtime/eval/lower.rs`,
  `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lower/types.rs`,
  `src/runtime/eval/indexed/semantic.rs`,
  `src/runtime/eval/indexed/full.rs`,
  `src/runtime/eval/indexed/full/schema.rs`, and
  `src/runtime/eval/indexed/full/decode.rs`.

Start after the union-find prerequisite is accepted and the completed Clippy
changes below are integrated or rebased. Own the full optional-annotation
migration, scheme production and import, scheme body facts, call
instantiation, lowering, verification, and focused Rust tests together. Do not
split out parser-only or scheme-record-only lanes.

**Language contract docs.** Own `docs/templates/SPEC.md`,
`docs/templates/user-tour.md`, `docs/SPEC.md`, and `docs/user-tour.md` after the
end-to-end behavior is accepted. Update templates before generated docs.

The completed source-only Clippy commits own these paths:

- `0015554d9`: `src/project.rs`, `src/syntax/grammar.rs`, and
  `src/syntax/parser/{expr.rs,without.rs}`.
- `b895269c6`: `src/sema/check.rs`,
  `src/sema/check/{checked_declarations,expr,infer_effects,module_driver,module_record,resource,stmt,stream}.rs`,
  and `src/sema/constants.rs`.
- `fe6039eda`: `src/runtime/eval.rs`, `src/runtime/eval/lower.rs`,
  `src/runtime/eval/lower/{patterns,pipeline,types}.rs`,
  `src/runtime/eval/lowered_run.rs`,
  `src/runtime/eval/lowered_run/module_net.rs`,
  `src/runtime/eval/net_job.rs`, and `src/runtime/eval/route_witness.rs`.

Integrate or rebase those commits before the rank-1 compiler lane starts.
Preserve their changes and do not repeat their Clippy cleanup.

The union-find lane can proceed independently while Phase A runs. The rank-1
compiler lane follows its accepted API and the Clippy integration, then runs in
an isolated worktree while Phase A continues. The integrator merges overlapping
paths after Phase A closes and reviews each boundary. The first inference slice
does not depend on Lean. Lean remains paused.

## Focused acceptance and instrumentation

The native witnesses must assert successful output and exact diagnostic
codes. Keep the six baseline cases in
`tests/xsh/typing-capability-witnesses.xsh` as the first acceptance group:
`test_typing_identity_and_open_record_projection`,
`test_typing_identity_rejects_a_wrong_context`,
`test_typing_open_record_projection_requires_the_projected_field`,
`test_typing_annotated_projection_missing_field_remains_rejected`,
`test_typing_dynamic_values_do_not_gain_a_concrete_type`, and
`test_typing_exported_scheme_is_independent_of_other_importers`. The annotated
missing-field control confirms `check.unknown-field`. The inferred missing-
field and wrong-context cases must assert their exact checker codes after
parser acceptance. Reuse the existing `Any`-to-`Int` dynamic-boundary negative.
Add checker tests named `checked_identity_scheme_is_rank1_and_reuses_one_binder`,
`scheme_instantiation_is_fresh_and_does_not_escape`,
`recursive_component_remains_monomorphic`, and
`captured_mutable_bindings_are_not_generalized`. Add solver tests for a
400-variable alias chain, type and row occurs failures, failed-unification
rollback after compression, isolated clone probes, rigid-variable rejection,
and closed-row missing-field rejection. Add verifier tests named
`full_verifier_rejects_forged_generic_call_evidence` and
`full_verifier_rejects_forged_generic_body_facts`. Add the in-process test
`user_schemes_execute_after_frontend_drop_on_both_routes`.

Each implementation owner records the production decisions it removes and the
owner of each retained decision. Report added, removed, and net production
lines at each checkpoint. Source growth and touched-line counts remain
advisory.

`TypeConstraints` has no operation counters in the current source. Record the
pre-repair 400-variable result before replacing its algorithm. Add
invocation-local counters for fresh variables, generated constraints,
representative hops and compression writes, unifications, occurs visits,
row-field visits, scheme instantiations, worklist wakeups, and rollbacks.
Count checked bodies and module roots in the checker. These counters must be
deterministic.

Count the 400-variable alias chain before and after the solver change. Count
repeated calls to one scheme. Record row visits for rows with 1, 4, 16, and 64
fields. Compare counts, not elapsed time, in the first checkpoint. These counts
show whether body constraints repeat and whether row work follows fields
visited. They do not waive Phase E's workload families. Preserve the 256
semantic depth limit and report a checking failure on exhaustion. Do not use a
dynamic or recovery type as a fallback.

Report retained bytes for solver cells, structural nodes, row fields, solved
schemes, and published call evidence. Reuse the `retained_bytes` pattern in
`CallableType`, `SemanticPools`, and `FullStore`. Keep retained compiler memory
separate from peak process memory. Do not make timing claims in this first
checkpoint. Phase E owns cold phase timing and scaling gates.

## Contract changes and requests

The parser currently reports `ParseExpectedParamType` for an unannotated
required parameter. The SPEC's rule that callers and bodies do not supply
types to each other remains in force: declarations are inferred independently
and calls instantiate the declaration scheme. This slice permits omitted
parameter annotations and extends inferred pure returns to the approved
exported identity case. It retains the explicit-return requirement for
recursive pure declarations in this checkpoint. Update `docs/templates/SPEC.md`
§5.1, §§9.2–9.3, and the current exclusion of generic functions. Regenerate the
user-facing docs after behavior is frozen.

The frontend exposes `CompactFunctionSig`, `CompactModuleGraph`, and checked
body facts. Generic schemes and call instantiations add compiler facts to
these boundaries. Keep solver metavariables private. If a public Rust struct
must expose new fields, surface that API change and update its callers and
documentation in the same change.

**Requests:** None for the first-slice contract. The recorded 400-variable
baseline fails before repair with `Err(TypeDepth)`. The solver owner preserves
that result while fixing representative traversal. The proposed internal types
are `TypeScheme`, `SchemeType`, `CallInstantiation`, and `RowEvidence`. No new
language syntax or performance-budget waiver is requested. Lean remains
paused.
