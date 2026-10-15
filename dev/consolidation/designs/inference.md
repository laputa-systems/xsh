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

Put immutable scheme data in `src/sema/schemes.rs`. Use `TypeScheme` for
quantified binders and a `SchemeType` tree for its parameter and return types.
The open-record tail in `SchemeType` refers to a `SchemeRowParameterId`, never
a mutable `RowVariableId`. Use `CallInstantiation` for one call's solved type
and row substitutions. Use `RowEvidence` for required and retained record
fields. These records contain no mutable solver cell. `TypeScheme` construction
rejects `Inference`, `Unknown`, and `Invalid`. Only explicit `Any` can represent
a dynamic boundary. Keep all generalization, instantiation, and unification
operations in `TypeConstraints`.

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
Missing required annotations are represented by a fabricated unknown type
expression after reporting the parse error. Change that syntax fact to an
explicit optional annotation, preferably `Option<TypeExprId>`, and remove the
overlapping `ty_defaulted` bit. `None` means the author omitted the annotation.
It is distinct from explicit `Any`, `Unknown` used for recovery, and a solver
metavariable. A default remains separately represented by `default`, so
`None` with a default preserves default-derived parameter typing, while `None`
without a default enters declaration inference. Malformed annotation syntax
continues to be a parse error and must not become `None` recovery.

`ArenaFunctionDef.return_ty_defaulted` already distinguishes an omitted return
annotation from explicit `Any`. Retain that distinction: `infer_return.rs`
relies on the flag, and an omitted proc return has the existing `Result[Unit]`
default. Do not replace either case with `Any` or `Unknown`. The return-rule
change here is limited to the approved unique rank-1 pure schemes, including
exported identity. Unannotated recursive pure returns remain explicitly
annotated in this first checkpoint.

`parse_params_arena_only` is shared by function declarations and callable type
expressions. Function declarations may carry `None`. Callable type expressions
still require an explicit type and retain their current diagnostic. Update the
arena builders, AST child enumeration, parser and type-expression validation,
checker, CLI planner, frontend-size accounting, and lowering sites that read
`param.ty` or `param.ty_defaulted`: currently `src/syntax/arena.rs`,
`src/syntax/parser/{stmt,types}.rs`, `src/sema/{types,cli_entry}.rs`,
`src/sema/check/{decl,infer_param,infer_return,stmt,types}.rs`,
`src/runtime/eval/{lower.rs,lower/types.rs}`, and `src/frontend_stats.rs`.
Existing `CallableParamType::defaulted` continues to mean an argument default,
not an omitted annotation. CLI entry validation keeps its existing contract.

`src/sema/check/infer_param.rs::infer_default_parameter_types` resolves
defaulted parameter types from their defaults and signatures already in scope.
It does not infer relationships from a function body. The return checks in
`src/sema/check/infer_return.rs` reject unannotated exported and recursive
pure functions.

`src/sema/check/module_driver.rs::CheckSession` prepares reusable module
records. `src/sema/check/module_record.rs::CheckedModuleFacts` retains each
module's checked output and exported `UserModuleSig`. The compact path in
`src/sema/check/checked_declarations.rs` and
`src/sema/check/compact.rs` publishes declaration signatures and body facts.
`src/loader.rs::CompactModuleGraph` retains imported module signatures.

`src/runtime/eval/lower/calls.rs::lower_call` reads checked call targets,
argument bindings, and expression types. `src/runtime/eval/indexed/semantic.rs::SemanticPools`
encodes closed `Type` values and callable signatures. The verifier in
`src/runtime/eval/indexed/full.rs` checks those pools and the encoded calls.
`src/sema/types.rs::CallableType` carries parameter labels, concrete types,
default and rest flags, return type, and optional effects.
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
identity. `CheckedDeclarations` records function signatures. `CompactDeclOutput`
projects declarations, and `CompactBodyFacts` rekeys expression facts by
`ExprId`. `CompactModuleGraph` carries exported `CompactFunctionSig` values.
Store one immutable `TypeScheme` per declaration in `CheckedModuleFacts`, keyed
by declaration identity. `CompactModuleGraph` carries that checked scheme under
the qualified exported name. `CompactBodyFacts` stores one `CallInstantiation`
per call `ExprId`. The declaration cache owns the scheme. The importing root
owns its call instantiations. Keep `CompactFunctionSig` as a ground projection
for existing concrete contracts. Never use `Any` in that projection to stand
for a generic scheme.

`src/runtime/eval/lower/calls.rs::lower_call` already consumes
`selected_function_calls`, `argument_bindings`, and checked expression types.
Have it encode the published instantiation. It must not revisit the AST to
infer a type or select an overload. The generic function body is compiled once.
Do not specialize a function body from the set or order of visible callers.

The indexed semantic pool currently encodes closed types and callable
signatures. Add quantified type and row binders to the generic function
signature encoding, plus the call's checked substitution and row evidence.
`FullVerifier` validates binder bounds, unique row labels, required fields,
call target, argument slots, instantiated parameter and return types, and
function body facts before execution. The executor reads the verified
instruction stream and does no inference. Keep the existing monomorphic
encoding for functions without quantified parameters.

The native acceptance test runs each program through the ordinary loader,
checker, lowerer, and indexed verifier, then executes it through `xsh`. A
subprocess test proves execution after the compiler process has discarded its
parser and checker state. A focused indexed-IR test forges a call substitution
or row evidence and requires `FullVerifier` to reject it. Existing
`builtin_templates_execute_checked_calls_after_frontend_drop_on_both_routes`
is a useful source precedent for testing calls after frontend disposal. The
new witness must exercise user schemes on the same ordinary prepared-program
boundary. The Bool identity case must remain a Bool through the call and
assertion. Lowering cannot choose its result from a runtime tag.

Importer outputs do not prove that the loader cached a quantified scheme.
Add a checker test that inspects the cached export and asserts one quantified
type binder shared by identity's parameter and return. Instantiate that scheme
twice and assert that the fresh solver variables differ before unification.
Unify the instances with `Int` and `Str`, then assert that no inference
variable escapes into checked facts. Run the Bool case through ordinary
indexed execution after frontend disposal.

## Exact implementation lanes

Each lane owns its listed paths in an isolated worktree. The integrator owns
shared-path integration and conflict resolution.

| Lane | Owned paths | Starts when |
|---|---|---|
| Capability witnesses | `tests/xsh/typing-capability-witnesses.xsh`, plus `tests/xsh/modules/typing-capability-identity.xsh`, `typing-capability-identity-reordered.xsh`, `typing-capability-import-int.xsh`, `typing-capability-import-text.xsh`, `typing-capability-root-int.xsh`, `typing-capability-root-text.xsh`, `typing-capability-root-joint.xsh`, and `typing-capability-root-bool.xsh` | The test owner keeps the acceptance fixtures. The red baseline records two pass and four fail. Inference lanes do not edit these files. |
| Solver and solver tests | `src/sema/constraints.rs` only | Red baseline recorded. First replace aliases with ranked representatives, compression, provenance, and rollback. After the scheme record contract lands, extend this same `TypeConstraints` authority with row unification, levels, generalization, and instantiation. |
| Parser representation | `src/syntax/arena.rs`, `src/syntax/parser/{stmt,types}.rs`, parser tests, and arena tests | Baseline recorded. Can proceed alongside the solver lane. |
| Immutable scheme records | New `src/sema/schemes.rs` and its `src/sema/mod.rs` wiring | The role names in this design are reviewed. This lane can proceed alongside the alias prerequisite. It defines immutable scheme and call-evidence data only. It does not own mutable cells or unification. |
| Checker and module publication | `src/sema/check.rs`, `src/sema/check/{decl,infer_param,infer_return,call,expr,stmt,types,module_driver,module_record,checked_declarations,compact,public_result,local_inference}.rs`, `src/sema/{types,cli_entry}.rs`, `src/loader.rs`, and `src/frontend_stats.rs` | Parser, solver, and scheme interfaces reviewed. Own function constraints, SCC monomorphism, scheme publication, per-call facts, and direct publication tests. |
| Indexed lowering and verification | `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/{calls,types}.rs`, `src/runtime/eval/indexed/semantic.rs`, `src/runtime/eval/indexed/full.rs`, and `src/runtime/eval/indexed/full/{schema,decode}.rs` | Checker fact types reviewed. Encode checked schemes and call evidence, then verify them before execution. |
| Language contract docs | `docs/templates/SPEC.md`, `docs/templates/user-tour.md`, `docs/SPEC.md`, and `docs/user-tour.md` | Parser, checker, and indexed behavior accepted. Update templates before generated docs. |

The alias prerequisite, parser representation, and immutable scheme-record
lanes can proceed now in separate worktrees. The solver extends the same
authority after the scheme record contract lands. The checker lane follows the
parser and solver interfaces. The indexed lane follows the published checker
facts. Documentation follows accepted behavior.

Phase A currently edits some parser, checker, lowerer, and verifier paths.
Inference owners may implement in their isolated worktrees while Phase A runs.
They must not edit the consolidation worktree or shared master. The integrator
merges overlapping paths after Phase A closes and reviews each boundary. This
merge sequence does not block isolated implementation. The first inference
slice does not depend on Lean. Lean remains paused.

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
and closed-row missing-field rejection. Add the indexed-verifier test
`full_verifier_rejects_forged_generic_call_evidence`.
Record each solver lane's removed and retained production decisions with its
owner. Report added, removed, and net production lines at each checkpoint.
Source growth and touched-line counts remain advisory.

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
