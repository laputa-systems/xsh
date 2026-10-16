# Rank-1 lowerer and verifier plan

Source pin: `campaign/typing-rank1-integration` at `e63689e`. Lowering work
starts only after that pin compiles and the source-based export regression
passes.

## Encoding and ownership

Keep `TypeId` limited to closed ground `Type`. Add distinct `SchemeTypeId` and
`FullSchemeId` newtypes. `FullStore` owns one compact scheme-type node pool,
one row-lacks pool, per-function scheme records, call-substitution rows, and
scheme body facts. A scheme node is either a ground `TypeId`, a scoped type
binder, a closed constructor with child `SchemeTypeId`s, or a record with
sorted fields and a closed or binder row tail. No scheme binder is encoded as
`Type::Any`, `Type::Inference`, or a ground `TypeId`.

Each generic `FullFunction` carries one tagged callable contract: either the
existing ground `SignatureId` or one `FullSchemeId`. Its scheme record owns
parameter `SchemeTypeId`s, a return `SchemeTypeId`, type binder IDs, row binder
IDs and immutable lacks labels, callable kind, finalized effects, and
parameter/return schema expectations. Those scheme nodes are the sole static
parameter and return types for a generic function. `FullParam` owns only
runtime names, defaults, and rest flags; it does not carry a competing static
type row. The scheme record is the encoded function signature itself, not a
second scheme cache. Encode each source `Arc<TypeScheme>` once per function;
do not reconstruct it from `CompactFunctionSig` or maintain a separate
runtime scheme registry. Ground functions keep their current `SignatureId`
form.

Attach each call's `CallInstantiation` to its existing checked target row by
instruction. Store sorted mappings from the target's type and row binder IDs
to closed type arguments and validated `RowEvidence`. Keep target selection
and argument-slot binding in their current facts; neither is copied into the
instantiation. Store `SchemeBodyFacts` as function-owned instruction-to-
`SchemeTypeId` facts. The lowerer translates body `ExprId`s while it encodes
the function, preserving binder scope and emitting one body for all callers.

The canonical handoff remains `FunctionSig`'s `Arc<TypeScheme>` to
`CompactDeclOutput`, then `CompactModuleGraph`, then the lowerer. Exported
schemes remain keyed by qualified export; private generic bodies use their
existing body-facts entry. `lower_function_with_blocker` attaches the
declaration's scheme and body facts to `FunctionBuild`; expression lowering
records binder-scoped facts in `BuildScratch`. `FullBuilder` encodes those
facts beside the owning `FullFunction`. A call uses the checker-produced
`CompactBodyFacts.call_instantiations` entry and attaches it to the existing
checked target in `BuildScratch`. `FullProgram` owns the encoded scheme and
evidence after parsed syntax, compact declarations, checker state, and solver
state are dropped.

For the existing `let bool: Bool = identity(false)` witness, the checker
records `T -> Bool` in the call's `CallInstantiation` and ground `Bool` in
`CompactBodyFacts.expr_types`. Lowering stores both: the target row carries
the substitution, and the call expression interns `Bool` as its ground
`TypeId`. The generic body's parameter read and tail each carry scheme binder
`T`. Before execution, the verifier substitutes `Bool` for `T`, checks the
return against that call substitution, and confirms the call's ground result
is `Bool`. `FullParam` owns runtime names, defaults, and rest flags; its
generic physical slot uses an explicit erased execution kind derived from
the scheme binder and carries the ordinary value unchanged. The following
`assert bool == false` is lowered as a Bool comparison and assertion. No
runtime tag selects the result type or an Any/dynamic fallback.

## Verifier obligations

Before execution, `FullVerifier` checks that each scheme binder is unique and
in scope; each body fact belongs to its function; parameter reads and record
projections match their scheme types; row labels are unique; required fields
are present; and the body result satisfies the declared return scheme. For a
call, verify the checked target, exact argument slots, complete and
non-duplicated type/row substitutions, closed ground type arguments, row
lacks, required fields, and substituted parameter and return types against
the existing checked expression fact. Reject missing, extra, out-of-scope,
forged, or escaping inference IDs. No inference or scheme reconstruction runs
in the executor.

Add forged-program negatives for a wrong type argument, omitted or duplicate
binder mapping, a row substitution that violates lacks, missing projected
field, mismatched checked call result, body fact owned by another function,
and an escaped solver ID. Add one accepted open-row projection and the
`Bool` identity call with `false`. The prepared-program test must drop
front-end state before verifying and execute through both indexed invocation
routes.

## Consumers and gates

Runtime consumers are `src/runtime/eval.rs`, `src/runtime/eval/lower.rs`,
`src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lower/arguments.rs`,
`src/runtime/eval/lower/types.rs`, `src/runtime/eval/lowered_run.rs`,
`src/runtime/eval/indexed/semantic.rs`, and
`src/runtime/eval/indexed/full.rs` including its schema and decode modules.
The encoding extends `BuildScratch`, `FunctionBuild`, `FullBuilder`,
`FullStore`, and `FullVerifier`; it does not add a checker cache or public API.
Focused tests belong with scheme construction and the indexed verifier.

1. First qualify `e63689e`: exact `xsh --lib --no-run`, then the
   source-based exported-scheme regression and focused constraints/schemes
   modules. Do not edit lowerer code before this passes.
2. At each compileable lowerer checkpoint, use the pinned debug no-run build
   and run only the changed `xsh --lib` tests by exact name, including the
   verifier forgeries and prepared-program route test. Include the entire
   constraints module when solver code changes.
3. After the coherent path passes, build matching verification products and
   run the six typing capability witnesses, including exact diagnostics and
   the `Bool` identity assertion. Use the prepared-program test for proof that
   execution no longer depends on front-end state. Do not run the full suite.

The final inference change includes parser, checker, import, lowering, and
verification behavior together. These runtime files are the planned added
consumers; the existing ground signature and caller-selection paths remain
owned by their current representations.
