# XSH formal project

This maintenance project uses Lean 4.35.0, pinned in [`lean-toolchain`](lean-toolchain).
Cargo builds and XSH products do not depend on Lean. [`Xsh.lean`](Xsh.lean)
imports mathematical models of selected XSH behavior. A `SPEC_PROVED` theorem
proves a property of its Lean definitions; it does not establish correspondence
with XSH source, indexed IR, Rust runtime, or host behavior.

[`coverage.json`](coverage.json) is the authority for coverage. It records
the frozen source commit and tree. A universal obligation is planned or
closed. A row closes only when its theorem covers the whole frozen core for
that obligation; it then names a public theorem in a module reachable from
`Xsh` that is not a fragment root, and the axiom audit rejects a closed root
that is absent, not a theorem, or outside the project modules. Read the
`universal_theorems` rows for the current closed obligations. A module that
holds a closed root needs no fragment row. The fragment roots below close no
obligation; each row's boundary names its
limits. The finite Rust-conformance ledger holds only the named tests that ran
against binaries built from that commit. Each case is evidence for its own test
only. The `artifact_certificates` list holds `ARTIFACT_CERTIFIED` records, one
per kernel-checked certificate: the artifact kind, the Lean definitions that
hold the artifact's bytes, the certificate theorem, its supporting and
rejection theorems, the proposition, and what it does not show. A certificate
proves a closed proposition about those bytes; it is not a fragment root and
closes no universal obligation. The fixed source/IR corpus, semantic scenario
suite, and replay runner have not been introduced. The ledger's bridge cases are
the fourteen named arithmetic and facts tests; other private bridge fixtures
are not conformance cases.

The vertical raw-IR exchange carries the string pool and canonical record-label
table under `integer-calls-conditional-local-fixture-v4`. The record-local source
and raw-IR pair is certified for four recorded calls, including division by zero
and an unread overflowing entry. The Lean admission and lowering theorems cover
one immutable, nonempty integer-field record local in a one-parameter `Int`
function, with entry values over the parameter and literals and a return over
those atoms and fields of that local. Fields have distinct non-keyword names;
spreads, updates, nested records, other statements, and calls of record-local
functions are outside this shape. The pair certifies the identified documents;
Rust execution is observed in finite bridge tests, not modelled.

Record construction and projection stay in the independent
[`BaseMachine`](Xsh/Semantics/BaseMachine.lean). Scalarizing fields in both
readers could make source and IR agree through the same projection mistake.
The machine's integer-record tag describes an exact ordered field shape;
relating it to Core membership's record-width rule remains separate work.
Record entries use canonical positions in the exported `names` table, and
field reads use the `strings` table. Both tables carry spellings rather than
process-local interner words. The bridge's
`record_local_labels_are_canonical_across_interner_states` test checks that
unrelated interning leaves the exported document unchanged.

The [record-local pair certificate](Xsh/Conformance/Vertical/RecordLocalPairCertificate.lean)
rejects its entry-order mutation through exact lowering even though the
recorded fixture results agree. That mutation does not demonstrate a semantic
ordering difference. Record facts remain a separate extension. Supporting
`IntFieldSlot` reads requires those facts because Rust selects that row form
from checker type facts. Callers of record-local functions must retain
projection as a machine step when their admitted shape is extended.

[`IR/Raw/Structure.lean`](Xsh/IR/Raw/Structure.lean) models the structural
`xsh-raw-ir-v1` document of an indexed store (scope `indexed-store-structure-v1`):
instruction, pattern, stage, value and driver tables with their tags named by
ordinal, payload words, blocks, functions, function metadata, parameters,
captures, strings, bytes, locations, copy pools, driver programs and the counts
of the semantic pools. The payload shape of every tag, the closed domains and the
resource-transferring instructions in [`IR/Raw/Schema.lean`](Xsh/IR/Raw/Schema.lean)
are generated from the Rust schema macros and codecs by a test-only exporter; the
width of each closed enumeration is the number of words its codec decodes. A Rust
test requires the generated text and the exported documents in
[`IR/Raw/Exported.lean`](Xsh/IR/Raw/Exported.lean) and
[`IR/Raw/ExportedFeatures.lean`](Xsh/IR/Raw/ExportedFeatures.lean) to match.
`document_decode_iff` shows that decoding is exactly the inverse of encoding on
documents whose rows have their declared arity. Raw-IR documents hold whole
programs, so `decodeDocumentCodes` parses them without the 64 KiB limit of the
other exchange documents, and the Rust raw-IR decoder does the same.
`decode_encode` sets `simp` `maxSteps` to 4000000 because its goal unfolds all 47
document fields at once.

[`IR/Raw/Spec.lean`](Xsh/IR/Raw/Spec.lean) states structural well-formedness,
`Spec.WellFormed`, without the checker. It imports only the document and shape
types and has no fuel, job list, scope, owner segment or budget. Each payload is a
sentence of its tag's shape grammar over the words of the table, and the rules
follow the raw-IR plan: exact tags, payload widths, one-based identities and the
`u32::MAX` sentinel, ranges, pools, patterns and stages entered at descending
indexes, owners and blocks, driver programs and steps, locations, and single
ownership of every claimed entry. [`IR/Raw/Agreement.lean`](Xsh/IR/Raw/Agreement.lean)
proves `verify_iff_wellFormed`: for every `Regular` schema the structural checker
accepts exactly the documents that satisfy `Spec.WellFormed`. A regular schema has
distinct tag ordinals and recursive shape names, and its recursive shapes reach no
pattern, stage, text pattern or recursion inside a block. `rawSchema_regular`
holds by evaluation, so `verify_rawSchema_iff` holds for the generated schema.
`budget_suffices` shows that the entry budget `(patterns + stages + 1) * (words + 1)`
is enough for every payload that fits with any fuel; a schema that recurs inside a
block can need more, which is why the theorem requires regularity. `verify_iff`
relates the Bool checker to its own Prop transcription `StructuralWF`, and
`fitFuel_mono` shows that more fuel never changes an accepted payload. These are
fragment roots about the Lean model.

The model enforces these rules of Rust's `FullVerifier`: function metadata with
zero reserved bytes, defined flags and a callable kind; parameter flags below 8 and
capture slots and binding identities inside the owning function; a present
location on every driver step; resource-transfer marks only on the eight
transferring instructions; every instruction read by its owner, every block read
only by its owner and every owned block read; every driver program except the
root imported once, never the root; driver slot rows with zero reserved bytes,
defined flags, a slot below the step's slot count, the temporary and definition
constraints and distinct slots and names within a step; writer identities in range;
regions that partition each program's steps in order; sync rows that are exactly
each region's shared slot names in increasing order, with their type and read and
write flags; and every step, region, slot, writer and sync row owned exactly once. The model is also stricter than Rust on
properties of how the builder emits IR, which real exported programs satisfy and
Rust does not check:

- an operand precedes the instruction that reads it;
- every pattern and stage row is read;
- block and parameter reserved bytes are zero;
- an unowned block is read at most once, and a value reads only unowned blocks;
- a `Use` step imports a program whose steps all come before it, which also rules
  out driver-program cycles;
- every driver step is checked, also when the document has no driver root;
- the decoder requires the document fields in the exported order.

Rust accepts documents that break these, so each is a retained difference, not a
Rust rule. The model does not check the semantic tables (types, signatures,
schemes, prepared values, validations, slot type facts, checked call targets and
binding-identity contents), the semantic content of driver writers (payload names,
binding-identity owners and locations, and the slot a writer names), the effect
words of driver steps, regions and programs, source spans, or which runtime
operations a program may use.

[`IR/Raw/Certificate.lean`](Xsh/IR/Raw/Certificate.lean) certifies the exported
points fixture and rejects 27 changes to it; the changes are listed in the
`artifact.raw-ir-structure` record. [`IR/Raw/FeaturesCertificate.lean`](Xsh/IR/Raw/FeaturesCertificate.lean)
certifies the export of `tests/fixtures/raw-ir-features.xsh`, which imports a
module and has patterns with a text pattern, a pipeline stage, prepared values and a
`Use` step, and rejects 13 changes to it, listed with the rules each breaks in the
`artifact.raw-ir-features` record. Among them, exchanging two pattern rows with
the words that name them breaks only the descending-pattern rule, while the
pattern-cycle and driver-program-cycle witnesses each break several rules. An imported module is named by its canonical path, so the
exporter replaces the checkout directory with `/xsh` in that document. These
theorems set `maxRecDepth` to 10000 locally because the kernel evaluates the whole
document. The model is structural only: nothing here shows that Rust's
`FullVerifier` makes the same decisions beyond these documents, or that the IR
executes correctly. No obligation id names the raw-IR structure work, and these
results close no obligation. They supersede the earlier synthetic four-tag
decoder model.

The independent base machine models integer-field record construction in source
order and field reads through named bindings. Its proofs cover preservation,
termination, and exclusion of record shape faults for well-formed executions;
these results do not establish source lowering or Rust correspondence.

The shared data foundation in [`Core/Types.lean`](Xsh/Core/Types.lean) and
[`Core/Values.lean`](Xsh/Core/Values.lean) represents the non-recovery type
families and raw value candidates. It reuses the existing scalar and integer
validation domains. Its kinded substitutions preserve callable metadata and
module contracts. Values retain raw Float bits, typed semantic references,
recursive error causes and nominal identity. The data laws do not establish
admissibility, complete type membership, execution safety or Rust conformance;
the fragment machines have not yet migrated to this foundation.

[`Core/Membership.lean`](Xsh/Core/Membership.lean) defines membership for the
represented value-type families, with row, export and union judgments. It
retains the environment-path list view, exact and open exports, and semantic
nominal provenance. [`Core/MembershipDecision.lean`](Xsh/Core/MembershipDecision.lean)
constructs a terminating decision and proves its result agrees with those
judgments. Retained stream and callable contracts are explicit predicates with
caller-supplied decisions. The model controls cover acceptance and rejection;
they are not Rust conformance evidence. Type admissibility, valid semantic
worlds, establishing and preserving the retained contracts, and integration
with source and execution remain open. TYPE-MEMBER is still planned.

[`Core/SymbolicEffects.lean`](Xsh/Core/SymbolicEffects.lean) represents finite
symbolic effect expressions with an arbitrary number of parameters and a mask
for each parameter. Exact inclusion and permission coverage are proved
equivalent to their judgments for every assignment. Only direct permissions
receive `io` closure; variable masks retain exact membership. Substitution agrees
with replacing assignments and preserves both relations for known replacements.
Unknown contracts propagate through each live substituted dependency and remain
distinct from a known empty contract. An erased error-only dependency gives a
counterexample to unconditional capture/substitution commutation with unknown
replacements. These are laws of the model: a false comparison witnesses an
assignment that fails, not an immediate Rust predicate rejection. Source
extraction, agreement with the Rust predicate decision, source effect
instantiation, inference integration and execution correspondence remain open.
`EFF-LATTICE` proves the finite effect-set lattice and exact `io` implication
closure for the Lean model only; it does not prove Rust extraction, diagnostics
or runtime effects. The other universal effect obligations remain planned.

[`Core/EffectPredicates.lean`](Xsh/Core/EffectPredicates.lean) models the
three-result decision for equality, inclusion, permission coverage and exclusion
over arbitrary parameter scopes. Known inclusion and coverage are accepted
exactly when their judgments hold under every assignment. A known rejection
has a semantic counterexample. Exclusion distinguishes universally safe,
universally prohibited and varying assignments. Equality acceptance is sound;
a redundant coefficient witnesses unresolved equality despite all-assignment
agreement. Unknown contracts retain their separate policy cases. Predicate
exclusion uses inverse permission coverage, including explicit error masks;
lexical-guard admissibility remains independent. Model controls exercise all
decisions and outcomes, symbolic `io` masks and error capture. The manual
production comparison covers all closed-mask pairs, all single-parameter mask
pairs and selected multi-parameter cases in a seventeen-parameter scope. Its
133,888 rows exercise all three outcomes of every predicate. The Rust consumer
constructs production contracts and calls `EffectPredicate::satisfaction`.
This finite comparison is separate from the frozen conformance ledger. General
source binding, inference integration and execution correspondence remain open.
These component proofs close no universal obligation.

[`Core/DeadlinePolicy.lean`](Xsh/Core/DeadlinePolicy.lean) proves selection
for arbitrary finite related states: a host-time order is represented by natural
monotonic ticks preserving and reflecting order, and the state relation maps
each host deadline in order, preserving identity, metadata and delivery state.
Under that assumption the selected scope is undelivered and expired, has the
earliest deadline, and precedes every equally early eligible scope; a selection
exists exactly when an eligible scope exists. Eligibility persists as supplied
ticks advance. The model also proves its delivery, close, finish, rearm, shield
and restore equations. These are model proofs, conditional on the relation and
clock abstraction; they do not prove that Rust or the host clock satisfies them.

The production selector receives borrowed deadlines and an explicit `Instant`.
A finite conformance comparison calls that function and Lean's `choose` for all
2,925 inputs with zero to three scopes, ticks zero through three, both delivery
flags, and observation ticks zero through four. Inputs embed ticks as nanoseconds
after one common epoch and retain duplicate identities. Strict decoding and
unique-input accounting reject unsupported, missing or repeated cases. Rust
tests also check identity and order through rearming, cleanup shielding,
restoration and closing with supplied instants. These are test evidence, not
universal Rust refinement. Clock domain and unit correspondence, representable
instants and checked deadline addition, identity allocation, checkpoints, error
construction and resource execution remain host or implementation obligations.
CTRL-DEADLINE stays planned; no universal obligation is closed.

Shared type terms now carry an effect-parameter scope separately from type
variables. Callable policies retain symbolic effect bounds and keep pure,
procedure and producer kinds distinct. Type substitution preserves that scope;
effect substitution can change it. One traversal implements both operations.
Their general commutation law transforms the type replacements when applying
the effect substitution. Known effect substitution agrees with interpreting
the bound under the substituted assignment; unknown replacements propagate
through live dependencies. The existing membership decision uses zero effect
parameters and no type variables. Model controls check nested callable policies,
defaulted labels, error capture and unknown replacements. Type admissibility,
source binding, retained-contract authentication and machine integration remain
open. These data and substitution laws close no full-core obligation.

[`Core/ReferenceValidity.lean`](Xsh/Core/ReferenceValidity.lean) enumerates the
references carried by every shared value family, including nested error causes,
module exports and process-status segments. A supplied policy decides which
references are retained. [`Core/Store.lean`](Xsh/Core/Store.lean) instantiates the
existing typed-cell API with that policy and the shared membership judgment.
A write succeeds exactly for a mutable present cell whose candidate satisfies
both checks. An unretained reference rejects the write without changing any cell.
The proofs quantify over the shared value domain. Controls exercise every
reference kind, nested payloads and real store acceptance and rejection.
This is not a proof of reference ownership, liveness, heap closure, contract
authentication, type admissibility, machine integration or Rust conformance.
Those obligations remain open. TYPE-MEMBER remains planned.

[`Semantics/CoreWorld.lean`](Xsh/Semantics/CoreWorld.lean) is the semantic
foundation interface. Its types and values are the Core types and values, its
store is the Core store guarded by reference retention and Core membership, and
its environments resolve names through binders to activation cells. Assignment
and declaration return a committed store, a `type-error` failure with the old
store, or an internal fault for a missing, immutable or occupied cell. `try`
recovery catches only catchable failures and propagates `type-error`.
[`Semantics/Connected/CoreEmbedding.lean`](Xsh/Semantics/Connected/CoreEmbedding.lean)
embeds the connected machine's runtime types and values into Core, and a
connected value with a connected type is a Core member of that type's image.
The callable contract decision is noncomputable and uses classical proposition
decidability. The connected store in
[`Semantics/Connected/Store.lean`](Xsh/Semantics/Connected/Store.lean) is
guarded by `StoreTyped`: every cell stores the Core typing
(`CoreWorld.HasType`) of its embedded value at the embedded type, together with
the connected typing the machine's lemmas pull back from that stored proof.
The guard admits exactly the connected members, so connected steps are
unchanged.
[`Semantics/Connected/CoreBridge.lean`](Xsh/Semantics/Connected/CoreBridge.lean)
adds checker monotypes and shows that an accepted connected write commits in
the corresponding Core store from the stored Core typing. Connected nominal
types over non-record bases, and row or argument sorts, have no Core image.
Core record membership is wider than the connected check, so a connected
rejection is not a Core rejection. The bridge is a fragment root, not a
full-core obligation.

The `environments_and_typed_stores` fragment root is in
[`Proofs/Core/Env.lean`](Xsh/Proofs/Core/Env.lean). It quantifies over every
Core world, type and value: lexical and activation resolution, captures, the
lookup chain, the stored-member invariant, assignment and declaration
acceptance and effects, and `type-error` failures that keep the store and
escape `try`. Only whole-cell assignment is modelled; field and index path
writes, compound assignment and patterns are not. Cleanup during the unwind belongs to the control machine and is
not modelled here. The witnesses accept a `UInt` write, reject `-1` with the
store unchanged, keep NaN Float bits, and detect a store without its guard and
a mutated binder.

The `store_writing_forms` fragment root is in
[`Proofs/Core/Writes.lean`](Xsh/Proofs/Core/Writes.lean), over the forms of
[`Semantics/CoreWrites.lean`](Xsh/Semantics/CoreWrites.lean). It is stated on
the CoreWorld API over resolved targets with selectors and the right side
already evaluated, so it does not close BASE-ENV, which needs a meaning over
`Statement`, `AssignTarget` and `BindingTarget` syntax.

- `let`/`var` with a `BindingTarget` is `declareTarget`. SPEC 7: "all fields
  are selected before any name becomes visible"; the declaration commits every
  cell or none, and `_` binds nothing. `..` (`hasRest`) is ignored because
  records are width-compatible, which matches Rust.
- `target sel… op= right` is `assignStatement` and `assignPath`. Selectors
  evaluate in path order, and a negative key into `Map[UInt, V]` fails with the
  catchable schema error at its own selector, before any later selector and the
  right side run, as Rust does. The update
  then commits on the store the right side left, re-checks the whole cell, and
  a compound operator reads the target at commit (SPEC 7). An index outside
  the list, a missing key under a deeper selector or a compound operator, and
  an operator failure are runtime failures that `try` does not capture
  (SPEC 8.8). A `=` write to a missing key inserts in key order (SPEC 4.5). A
  field write on a filesystem entry publishes the entry's field record, which
  follows Rust; the SPEC does not say. A key write into a record held in a
  `Map[Str, V]` cell publishes a key-sorted map, and inserting into a
  key-sorted map keeps it sorted. A present, mutable member cell with a typed
  path never reaches an internal fault, a well-typed destructuring never
  reaches `malformedTarget`, and a field write through a nominal type, which the checker
  rejects (`check.assign-target`), is not a typed path. The operator meanings
  and the runtime error values are parameters; Rust's write path raises
  `missing-field` for a missing key. BASE-STEP owes the instantiation of
  `Operate`, including that list `+` is `concatenate` (SPEC 4.5), which
  VAL-STORE's `+=` clauses take as a premise.
- `e"NAME" = value` is `setEnvironment`; a NUL value is the uncaught
  `nul-argv` failure of argv conversion (SPEC 10.5, 11.4).
- `unwind` runs the cleanups of a block in reverse order of registration on
  the store the failure left; an earlier failure stays primary and, after a
  committed body, the first cleanup failure becomes primary; `try` catches only
  raised errors.

The witnesses commit a field write and read it back, reject a negative `UInt`
field, show that out-of-range indices, `/=` by zero and a missing key under
`+=` are not caught, that a negative `Map[UInt, V]` key fails at its own selector before
a later selector writes the store, that a missing key is inserted in key order,
that `{b, a}` held in a `Map[Str, Int]` cell takes `"c"` as `a, b, c`, that `x += e` reads
`x` after `e` while `x = x + e` reads it first, that an entry field write and
a record-in-map key write publish values, and that the last registered
cleanup runs first. Mutants that each falsify a conjunct: counting a negative
index from the end, defaulting a missing key, appending an inserted key, a
non-atomic destructuring, and a cleanup failure that replaces the primary one.

The `syntax_directed_store_writes` fragment root is in
[`Proofs/Core/Statements.lean`](Xsh/Proofs/Core/Statements.lean), over
[`Semantics/CoreStatements.lean`](Xsh/Semantics/CoreStatements.lean). It gives
the store-write meaning of `let`, `var` and `const` (also under `export`) and
of assignment over the frozen core's `Statement`, `BindingTarget` and
`AssignTarget` syntax. A declaration evaluates its initializer, resolves its
target to cells and checker types, commits through `declareTarget`, and binds
each name so that `Environment.cellOf` resolves it to its cell; a well-typed
destructuring never reaches `malformedTarget`. An assignment resolves its root
through `cellOf` and runs `assignStatement` on its flattened selector path, so
every `store_writing_forms` fact applies; `e"NAME" = value` converts the value
and sets the overlay. A block runs in a new lexical scope and leaving it
restores name resolution. The oracles are the name table's spellings,
BASE-STEP's expression and index evaluation, the checker's binder, cell and
type for each declared name, and argv conversion (SPEC 11.4). The witness runs
`var n: UInt = 0; n = 5; n = -1` as a block: the last write is a `type-error`
that keeps `5`, and after the block `n` no longer resolves; a block without
its scope exit still resolves `n`. BASE-ENV stays planned: `for`, `guard let`,
`match`, `if let` and `while let` bindings, comprehension and `retry` targets,
`with`, resource and `tempdir` scope bindings, parameter binding at call,
`use` aliases, declaration names and captures have no meaning here.

Three fragment roots cover the expression evaluator in
[`Semantics/CoreEval.lean`](Xsh/Semantics/CoreEval.lean):
`covered_expression_evaluation` in [`Proofs/Core/Eval.lean`](Xsh/Proofs/Core/Eval.lean),
`preservation` in [`Proofs/Core/EvalTyping.lean`](Xsh/Proofs/Core/EvalTyping.lean)
over the typing judgment in [`Typing/Core/Expressions.lean`](Xsh/Typing/Core/Expressions.lean),
and `evaluated_store_writes` in [`Proofs/Core/EvalStatements.lean`](Xsh/Proofs/Core/EvalStatements.lean).
The evaluator is fuel-bounded and covers `Syntax.Core` literals, names (locals,
then `Environment.cellOf` and the store), list and named-field record
literals, field reads, list, map and record indexing, `not` and negation,
short-circuit `and` and `or`, the Int, Float, Duration, Str and Bool operators
(with both `Duration * Int` and `Int * Duration`), list `+`, `if`/`else`, and
positional calls of let-free functions. A negative literal index is evaluated
by BASE-NUM negation and counts from the end of a `List` only, so `-0` is `0`
and a map receives the negative key (SPEC 6.6). A function body evaluates in
the environment the function was defined in. Operands and arguments evaluate
once, left to right. A finished result is the same at every larger fuel.
Every value is a member of the type the typing judgment gives, in a typing
context whose names hold members and with every function typed at its
signature in a context its own defining environment satisfies; the judgment is defined from the SPEC 6.2 operator table, not from
the evaluator. Every failure is a runtime failure, which `try` propagates
(SPEC 8.8), or the raised error of a `Result[Bool]` condition's `Err` in an
`if` expression, which `try` catches (SPEC 8.3). Operator results are BASE-NUM's `ArithmeticMeaning`,
`FloatMeaning` under the context's `NaNPolicy`, `ComparisonMeaning`, IEEE
equality, `IntegerExact` and `NegationMeaning`, the `DurationOps` results, and
list `concatenate`. `evaluated_store_writes` answers BASE-ENV's
`Oracles.evaluate` and `Operate` with the evaluator: declarations commit the
evaluated value or fail with its failure, a well-typed initializer never
reaches `malformedTarget`, and `+=` on lists is `concatenate`, discharging the
`store_writing_forms` deferral for that `Operate`. The witnesses give
`division-by-zero`, `integer-overflow`, `index-out-of-range`, `missing-field`
and `missing-key`, each propagated by `try`, and give `[7, 8][-0]` as `7`,
`m[-1]` as the `-1` entry, and `2 * 1s` as `2s`. Mutants each falsify a claim:
`(1 / 0) + [][0]` with the right operand first reports `index-out-of-range`, a
strict `and` fails on `false and [][0]`, the remainder BASE-NUM rejects gives
`0` for the minimum `Int` `% -1`, reading a negative literal as a positive
distance fails on `-0` and reads `m[1]`, and running a body in the caller's
environment makes `let x = 1; fn f() { x }; { let x = 2; f() }` give `2`
rather than `1`. BASE-STEP
stays planned. Method calls (so `IntToFloatMeaning`, `ToIntMeaning` and
`BitsetMeaning`), UInt arithmetic, `?`, `try` and raised failures, `??`, `in`,
sets, guarded `?.` and `?[]`, slices, map literals, record spread, shorthand,
computed and path fields, comprehensions, pipelines, closures, named, spread
and splice arguments and defaults, collection equality, host requests and the
`Map[UInt]` key Schema failure have no meaning here.

The `statement_control` fragment root is in
[`Proofs/Core/Control.lean`](Xsh/Proofs/Core/Control.lean), over the statement
machine in [`Semantics/CoreControl.lean`](Xsh/Semantics/CoreControl.lean). The
machine runs `let`, `var` and `const`, name and field-path assignment,
environment assignment, expression and bare-block statements, `if`/`else` and
`if let`, `while` and `while let`, `for` over a `List`, `loop`, `break`,
`continue`, `match` with literal, binding, wildcard, record and list patterns
and guards, `return`, `assert` and sugar through its expansion. A block function
body runs in its defining environment with its tail expression as its value.
Each block, branch, iteration and arm is a scope: leaving it restores the
environment and locals and drops the cells allocated inside it. Outcomes are
normal completion, `break`, `continue`, `return` with a value, a failure (raised,
runtime or type error, so `try` captures exactly the raised ones, SPEC 8.8), an
internal fault, stuck, unsupported, and out of fuel. The root proves that a
finished block or statement list is the same at every fuel, that every outcome
keeps the cells it started with at their types, that blocks and control
statements restore their scope and so keep a frame consistent with any typing
context it satisfied, that `while false` completes unchanged, that a loop ends
normally on `break` while a block passes it on, that `return` skips the rest
of a block and leaves a `for` loop, and that pattern captures and `for`
targets bind cells that resolve in the bound body. `if let` and `while let`
use the new `Expr.patternCondition`, mirroring `ArenaExprKind::PatternCondition`;
a pattern that cannot fail is an optional binding that rejects `null`. The
witnesses show that shadowing restores after a block, that a `for` variable
does not resolve after the loop, that `continue` skips the rest of the body
but advances, that a nested `break` ends only the inner loop, and that
`return` leaves a loop and a block function body. Three mutants are rejected:
a `for` that leaks its variable, a `break` that ends two loops, and a `return`
that only breaks. BASE-STEP and BASE-ENV stay planned: `Status` conditions,
`for` over streams, maps, strings and bytes, `break` with a value, `return`
without one, index-selector assignment, `defer`, `errdefer` and cleanup,
`guard let`, `with`, resource and `tempdir` scopes, `yield`, `exit`,
declarations, commands, and alternation, alias, constructor, type and text
patterns have no meaning here.

The `deferred_cleanup` fragment root, also in
[`Proofs/Core/Control.lean`](Xsh/Proofs/Core/Control.lean), covers SPEC 8.7.
`defer` and `errdefer` register their action, newest first, with the names it
resolves; leaving a block runs the pending actions on normal completion,
`break`, `continue`, `return` and failure, newest first and each once, on the
store the block left. An `errdefer` runs only when the block leaves with an
error: a failure, an `Err` returned through it, or one of its own later
actions failing. A failing action is a cleanup failure: an earlier failure
stays primary and the cleanup failure is reported, and otherwise the first
cleanup failure becomes primary and is never replaced. Each call runs in its
own activation, a declared name's cell being its binder in that activation, so
a recursive block body gets fresh cells; the callee cannot write its caller's
cells, and its cells end with the call. The witnesses compute `sum(2) = 3`
through a body that declares `var acc`, run a loop body's `defer` once per
iteration and on `break`, return `1` from `var x = 1; defer { x = 2 }; return x`
before the defer sets `x`, skip an `errdefer` on a normal exit and run it on a
failure, run two defers newest first, and keep the original failure primary
over a cleanup failure. Five mutants are rejected: calls sharing the caller's
activation, registration-order unwinding, skipping defers on `break`, running
`errdefer` on success, and a cleanup failure masking the original one.

The `error_handling` fragment root, in the same module, covers the error core
of SPEC 8. `?` and a conversion unwrap `Ok` and raise the error of `Err`, which
is the raised class together with failed assertions and the `Map[UInt]` key
`Schema` failure; a statement-position `Err` fails its statement. `try { ... }`
is a value block whose raised failure becomes `Err` and whose completion is
`Ok` of its tail; runtime and type failures, `return`, `break` and `continue`
pass through it, and because the failure leaves the `try` body, that body's
`errdefer` runs while the block containing the `try` continues. A value
block's tail is its value, so an `Err` tail leaves it normally and its
`errdefer` is not due. Each `retry` attempt is a value block: a raised failure
the `on` filter selects runs the attempt's defers, takes the next delay as an
observation in `Frame.waited`, and runs again, and the result is `Ok` of the
first success or `Err` of the last failure. A block function body has a
return kind: a `Result` function returns a raised failure, a primary cleanup
failure included, as its `Err` and wraps plain values in `Ok`, and any other
function fails the call. The witnesses show `?` skipping the rest of a `try`
body and running its `errdefer`, `try` capturing a failed assertion but not an
`index-out-of-range`, a three-attempt `retry` running its attempt `errdefer`
twice, a cleanup failure reaching a `Result` function's caller as `Err` and a
plain function's caller as a call failure that `try` captures, and a tail
`Err` running a function's `errdefer` but not a value block's. Four mutants
are rejected: a `try` that captures runtime failures, a tail `Err` treated as
a normal exit, a `retry` that skips attempt defers, and a `Result` function's
cleanup failure escaping as a call failure.

### BASE-ENV and BASE-STEP gap list

`xsh-lean.md` closes BASE-ENV and BASE-STEP "only when their theorems hold for
the frozen core's syntax, values and host request families"; BASE-STEP also
needs every numeric step result to meet the BASE-NUM meanings, including
`IntToFloatMeaning`, `ToIntMeaning` and `BitsetMeaning`. This list is every core
form the expression evaluator and statement machine still leave
`.unsupported` (P: pure, E: needs clocks, processes, the filesystem or the host
environment).

Statements:

- stream pipelines and stages, `.collect()` on a stream, comprehensions over
  one, and a `Stream[T]` from anything but a producer (E)
- `use` aliases, which need module loading (E)
- `cli main` declarations, which need the host argument model (E)
- `with` resource scopes, signal hooks and command statements (E)

Expressions:

- `and`, `or` and `!` over `Result[Bool]` operands in a value `if`
  condition (P)
- equality and membership of errors and handles, and `in` on a `Path` whose
  bytes are not valid UTF-8 (P)
- methods and value pipelines (P)
- the contextual `.require()` (P)
- `cd` and `within` scopes (E), `tempdir` (E),
  `spawn`, `wait`, run forms, pipelines and stream stages and their callbacks
  (E), `env.PATH` and the computed-name `env` readers (E), `$?` (E)


Writes and values: stream declarations, which need the stream model (E); `.Variant` constructors, which need a leading-dot
syntax form and the checker's expected type (P).

Binders: declared types still come from the resolution oracle (P); the
resolution pass is tied to the machine for statements and statement lists
(`statement_resolution`), for pattern captures and comprehension targets
(`capture_resolution`, with capture names from `pattern_names`), for
function bodies (`body_resolution`) and, from `runScript`, for the scopes at
each top-level function's declaration (`hoisted_scopes`,
`script_body_resolution`); that every closure a call finds is one hoisting
or a declaration bound holds by the machine's construction, not a theorem
(P).

The `resolution_and_captures` fragment root adds the syntax-directed
resolution pass (`staticResolve`, `staticUses`, `staticCaptures`), the
`Represents` link between the machine's lexical environment and the pass's
scopes, and writable captures: a call of a local function returns with the
caller's cells as the callee left them, and the callee may
write a caller-held cell only when it is one of the closure's captured mutable
cells (SPEC 9.2). Its witnesses are a counter closure, a write that reaches the
captured cell rather than the call site's shadowing one, and the pass on a
straight-line program; the mutants lose the write or write the call site's
cell.

Set literals, set comprehensions and comparison chains are rejected by the
parser (`parse.set-literal`, `parse.comparison-chain`), so the syntax model has
no form for them.

The `structural_equality_and_membership` fragment root, in
[`Proofs/Core/Equality.lean`](Xsh/Proofs/Core/Equality.lean), gives `==`,
`!=`, `in`, `not in` and the set operators their meaning on every operand
pair (SPEC 4.5, 6.2, owner decision 1). `==` is the structural equality
`valueEqual`: Floats compare by IEEE value at every depth, list items pairwise,
map entries and record fields by key whatever their order, set elements as
sets, `Ok` and `Err` payloads structurally, variants by declaration, name
and payload (a `Fault(1)` of another enum is unequal), and a `null` equals
only `null`.
`in` is substring containment in a `Str`, byte containment in `Bytes`, an equal
item in a `List`, a `Path` entry of `env.PATH`, key presence in a `Map` (a key
whose value is `null` is present), an element of a `Set`, a `Str` label of
a record, and a `Str` or `Path` in the display text of a `Path`, here its
bytes when they are valid UTF-8. `|`, `&` and `-` give the distinct elements of the union,
intersection and difference, and two sets with the same elements are equal
however they were built. Its witnesses agree with what the verification
binary prints for the same programs: `[1.0, -0.0] == [1.0, 0.0]`,
`[nan] == [nan]` and `nan in [nan]` are `true`, `false` and `false`; an `Int?`
holding 3 is not `null`; `{b: 1, a: 2} == {a: 2, b: 1}`; a `null`-valued map
key is present; and `s | t`, `s & t` and `s - t` for `s = set.from([3, 1, 2])`
and `t = set.from([2, 4])` are `[1,2,3,4]`, `[2]` and `[1,3]`. The mutants it
rejects are raw-bit Float equality, record equality in written field order,
membership that requires a non-`null` map value, and a union that
concatenates its operands, and variant equality by name alone. Errors and
handles are not compared, a
`Path` whose bytes are not valid UTF-8 is outside membership, and String containment is stated on
code points. The set order is map key order only if `keyBefore` is a total
order, and that is not proved.

The `match_values` fragment root, in
[`Proofs/Core/MatchValues.lean`](Xsh/Proofs/Core/MatchValues.lean), adds
`match` as an expression (SPEC 6.8). The subject is evaluated once and the
arms are tried in order: a failing pattern or guard passes to the next arm,
and the first arm that selects the subject gives its body's value in a scope
holding the pattern's captures, which do not resolve after the match. A match
with no such arm, statement or expression, fails with the runtime
`match-no-arm`, which `try` does not capture; before this root the statement
machine was stuck there. Its witnesses follow the verification binary: a
holding guard selects its arm, a failing one falls through to `else`, a
block body gives its tail, and a statement `match` over `3` with only a `1`
arm exits 3 with `match-no-arm`. The mutants it rejects ignore guards or bind
the captures in the enclosing scope. Exhaustiveness belongs to the checker
and is not modelled.

The `slices_and_guarded_hops` fragment root, in
[`Proofs/Core/Slices.lean`](Xsh/Proofs/Core/Slices.lean), adds slices
(SPEC 6.6) and the guarded `?.field`, `?[index]` and `?[low..high]`
(SPEC 6.7). A slice of a `List`, a `Str` by Unicode scalar values or `Bytes`
by bytes is a contiguous run of it; a negative bound counts from the end,
both bounds are clamped into the value, an omitted bound is its start or end,
and an end before the start gives an empty value. A guarded operation on a
`null` receiver is `null` without evaluating its index or bounds, a `Result`
receiver propagates as `?` does, and any other receiver runs the ordinary
operation. Its witnesses follow the verification binary: `xs[-3..]`,
`xs[2..1]`, `xs[-10..10]` and `xs[..-1]`, `"aé🦀"[1..3]` as `é🦀`,
`"hello"[..-1]` as `hell`, `b"abcdef"[2..5]`, and `null` receivers whose index
is a division by zero. The mutants it rejects evaluate the index before
testing the receiver, or read a negative start as zero. A guarded method call
is outside the model, as methods are.

The `brace_literals` fragment root, in
[`Proofs/Core/Records.lean`](Xsh/Proofs/Core/Records.lean), adds record
spread, shorthand, computed and path fields (SPEC 6.4). A brace literal with
a computed key is a map, its constant labels `Str` keys; otherwise it is a
record. Entries evaluate once in source order, a computed key before its
value; a later field or spread replaces an earlier field of that label; and a
path entry replaces the existing nested field it names and adds none. Its
witnesses follow the verification binary's output for `{...defaults,
...overrides}`, `{...defaults, jobs: 9}`, a functional update of
`build.flags.debug`, and `{[ext]: 1, other: 2}` and `{[ext]: 1, [ext]: 5}`. The
mutants it rejects evaluate a computed key's value first or let earlier
fields win. The checker's spread and update rules are not modelled.

The `control_conditions` fragment root, in
[`Proofs/Core/Conditions.lean`](Xsh/Proofs/Core/Conditions.lean), adds the
control positions of a condition (SPEC 8.3). An `if` or `while` condition,
and each operand of `!`, `and` and `or` within one, gives the `Bool` of a
`Result[Bool]`'s `Ok` and raises its `Err`'s error as `?` would; `and` with a
false left operand and `or` with a true one skip the right operand, which
then cannot fail. Its witnesses follow the verification binary on
`ok(false) and bad()`, `ok(true) or bad()`, `! ok(false)`,
`while ok(i < 2)` and an `Err` reached through `ok(true) and bad()`. The
mutants it rejects test both operands before combining them, or accept only
plain `Bool` operands. A value `if`, in both evaluators, takes the `Bool` of
a whole condition's `Ok` and raises its `Err`'s error, as the verification
binary gives `7 8` and then the `Err` for `if ok(true)`, `if ok(false)` and
`if bad()`; `and`, `or` and `!` over `Result` operands there remain outside
the model.

The `require_expression` fragment root, in
[`Proofs/Core/Requires.lean`](Xsh/Proofs/Core/Requires.lean), connects the
machine's `value.require(T)` to the validation model (SPEC 5.3). The value is
evaluated once and given to `Require.requireValue` with the `keep` field
policy, on the machine's membership world and contract decisions, for the
checker's `T`. So the closed VAL-REQUIRE theorem `recursive_require` applies
to what the machine gives: each `Ok` is a member of `T`, and each `Err` is
the schema error of the conversion's failure. The witnesses give
`7.require(Int)` as `Ok(7)` and `"x".require(Int)` as a schema error, and
reject a `.require` that passes its value through. The contextual
`.require()` needs the checker's expected type and is not modelled.

The `error_contexts` fragment root, in
[`Proofs/Core/Contexts.lean`](Xsh/Proofs/Core/Contexts.lean), adds
`ctx description { body }` (SPEC 8.8). The description is evaluated once and
the body runs as a value block. A raised or runtime failure that leaves the
body gains one context frame with the description, after the body's cleanup,
and keeps its family, variant, message, payload and earlier contexts; a value
the block gives, an `Err` included, is data and is not annotated. The
verification binary agrees: a schema error that `?` propagates out of a `ctx`
body reports `(ctx: outer step)`, and so does an index out of range, while a
`fail`, which returns its `Err`, is not annotated. The witnesses check a
propagated error and an `Err` value, and reject a `ctx` that annotates data.
The frame's span is not modelled.

The `exported_declarations` fragment root, in
[`Proofs/Core/Exports.lean`](Xsh/Proofs/Core/Exports.lean), runs `export` in a
script (SPEC 3.1, 3.3): an exported declaration runs, hoists and declares
names in the resolution pass exactly as the declaration it wraps, so an
exported `proc` or `pure` is hoisted with the script's other functions. The
verification binary prints `3` for an exported `pure` called above its
declaration and `14` for `twice(seven)` over an exported `let`; the witnesses
check both shapes and reject a script that does not hoist an exported
function. A module's export interface and loading are not modelled.

Function values travel as the verification binary's do. Nested declarations
are rejected (`check.nested-declaration`), so every callable is a hoisted
top-level function, and each call's closure table holds its caller's, so a
function value passed to, returned from or stored by another function is
found where it is called: `apply(twice, 7)`, `pick()(7)` and
`apply([twice][0], 7)` all give `14` (`function_values_travel`), where the
binary prints `14 8 2` for its three variants of these shapes. A callee table
holding only what its declaration saw cannot find the later `twice`. The
machine's `callableCell` oracle must map each callable identity back to its
declaration's cell; the witness world has one callable identity.

The `numeric_methods` fragment root, in
[`Proofs/Core/NumericMethods.lean`](Xsh/Proofs/Core/NumericMethods.lean),
gives the machine the numeric method calls whose values BASE-NUM describes
(SPEC 4.2, 4.3, 6.3). The receiver evaluates first, then the arguments in
order; `.float()` is `IntToFloatMeaning`'s conversion, `.floor()`, `.ceil()`
and `.round()` are `Ok` of the `ToIntMeaning` result under their bounds or
`Err` of the conversion error, and `.bit_and`, `.bit_or` and `.clear_bits` are
`BitsetMeaning`'s result, a negative operand failing with the runtime
`integer-bitset` that `try` does not capture; `.parse_float()` is `Ok` of
`parseFloatText`'s reading or `Err` of the parse error. Its witnesses follow the
verification binary's `true 3 -3 2 3`, `true` and bitset results, with Float
literals read by `decimalFloat`, and reject magnitude bitsets and ties-to-even
rounding.

The `environment_reads` fragment root, in
[`Proofs/Core/EnvironmentReads.lean`](Xsh/Proofs/Core/EnvironmentReads.lean),
adds `e"NAME"` (SPEC 10.5). A read takes the variable as the script's
assignments leave it, else as the process started with it, which the machine
takes as the input `hostEnvironment` and never writes; it is `Ok` of the text
when the bytes are UTF-8, `Err` of `invalid-utf8` when they are not, and `Err`
of `env-missing` when the variable is unset. The environment is evaluator
state, not a binding: a call now starts with its caller's assignments and its
caller continues with the call's, and `e"NAME" = value` runs inside a call,
where it was unsupported before. The witnesses follow the verification
binary on a script that reads an unset variable, assigns it in a called
`proc` and reads it back, assigns it at the top, and reads a preset and a
non-UTF-8 variable (`true`, `set-in-proc`, `top preset`, `true`); a call with
its own empty environment is rejected.

The `environment_scopes` root, in the same module, adds `env (overlay) { ... }`
(SPEC 10.4): the overlay's fields convert as argv items and are set in order,
the body runs as a value block, and the scope gives `Ok` of its tail; a NUL
value is the scope's own `Err`; and however the body ends, its frame returns
with the environment the scope entered with, so the overrides and the
body's own assignments are undone. The witnesses follow the verification
binary's `inner`, `true true` and `true`, and reject a scope that keeps its
overrides. `cd` and `within` scopes are not modelled.

The `script_exit` fragment root, in
[`Proofs/Core/Exits.lean`](Xsh/Proofs/Core/Exits.lean), adds `exit status`
(SPEC 8.9) as the machine outcome `exited`. The status is evaluated once and
must be from 0 to 255, else the runtime `exit-status` fails; the exit leaves
every scope through its deferred actions, `errdefer` among them only for a
nonzero status or after a cleanup failure; a cleanup failure is reported and
the exit stays primary; `try` does not capture it; and a call's `exit` ends
its caller. The verification binary prints `errdefer f`, `defer f`, `defer top`
and exits 3 for an `exit 3` inside `try`, runs only `defer f` for `exit 0`,
runs an `errdefer` after a failing `defer` and still exits 0, and fails an
out-of-range status with `exit-status`; the witnesses check each, with a
counter the actions add to, and reject a `try` that captures the exit.

The `retry_backoff` root, in the same module, adds `retry backoff first..cap
within limit` (SPEC 8.8). Its three durations evaluate once, in order, and the
form runs as a written list of the delays it takes: the schedule is `first`,
then twice the delay before, never longer than `cap`, and a delay is taken
only if it would end within `limit` of the first attempt's start. The clock
is the machine input `retryClock`, the milliseconds since the first attempt
began at the end of each attempt, a host observation the machine does not
produce. With attempts that take no time and `100ms..250ms within 1s` the
witness waits `100`, `200`, `250` and `250` milliseconds and ends with the
fifth attempt's `Err`; an uncapped schedule is rejected. A zero `first` is
outside the model, and the delays are observed, not slept.

The `stream_producers` fragment root, in
[`Proofs/Core/Producers.lean`](Xsh/Proofs/Core/Producers.lean), adds stream
producers consumed by a `for` loop (SPEC 9.5). A `stream` declaration is
hoisted as a `producer` closure; a `for` loop whose iterable calls one runs
the producer with the loop pushed on the frame's `consumers`, which calls
carry; each `yield` runs the loop's body on the item in the loop's own scope,
sharing the store and environment variables, then resumes the producer. A
body that completes or continues resumes it; one that breaks, returns or
fails stops it as the `consumed` outcome, which unwinds the producer's scopes
running its `defer`s but not its `errdefer`s, passes its `try`s, and then
leaves the loop as the body did. The verification binary prints `yield 0`,
`got 0`, `yield 1`, `got 1`, `producer defer`, `1` for a loop that breaks after
two items and three `yield`s, `producer defer`, `2` for a loop that returns;
the witnesses check both orders with a trace list and reject running the
producer to its end before the loop. `yield @list` yields each item in turn
and stops when the loop does: as the binary prints `before`, `got 7`,
`got 8`, `producer defer` for a loop that breaks at the second delegated item
and runs the rest after the delegation when the loop does not, so do the
witnesses. A producer call anywhere else creates a `Stream` value without
running the body, its arguments evaluated at the call: the frame's `streams`
table, which calls share, holds it as pending until a `for` loop or a `yield @`
consumes it, and a consumed stream yields nothing again, as streams are
one-pass. As the binary prints `created`, `yield 0`, `got 0`, `producer defer`
and nothing for a second loop over a stored stream, and pulls `yield
@count(2)` on demand inside another producer, closing it before its parent
goes on, so do the witnesses; a stream that restarts on each loop is rejected.

The `parameters_and_maps` fragment root adds default and rest parameters
(SPEC 9.2: defaults "are evaluated in the callee's declaration scope, cannot
see other parameters, and run once per call for each omitted argument, in
parameter order, after the supplied arguments") and map comprehensions, where
the later projection of a key wins and the key is projected before the value.

The `expression_store_threading` fragment root, in
[`Proofs/Core/Threading.lean`](Xsh/Proofs/Core/Threading.lean), gives the
statement machine its own frame-threading expression step. `evalExpr` takes
the current frame and returns an `Outcome`: a value with the frame it leaves,
or the outcome that leaves instead, so a failure keeps the writes made before
it. `threadStep` is `CoreEval.step` with the frame threaded and the same
`binary`, `unary`, `indexAt` and `fieldOf`: operands, list elements, record
fields, branch tests, call arguments, assignment selectors and omitted
defaults evaluate left to right, each in the frame the one before it left
(SPEC 1: "Evaluation order is source order"). A value block nested in an
expression runs as a block in the current frame, so `[{ x = 2; 1 }]` writes a
cell that existed before it. Every call that runs a body goes through
`callBlock`, which returns through `returnTo`, so a captured mutable cell
written by a call nested anywhere in an expression reaches the caller (SPEC
9.2: "A writable capture must refer to mutable storage with exactly that
type"), and a call in statement position is the same evaluation. On a frame
without local functions, wherever `CoreEval.evaluate` gives an `Evaluable`
expression (its forms, without `??`, and record literals of named fields
only) a meaning and every expression-bodied
plain function's body is evaluable, the machine gives the evaluator's result
with the frame unchanged, so `covered_expression_evaluation`, `preservation`
and `evaluated_store_writes` describe the machine's results there; with
defaults that leave the frame as it is, the threaded binder is
`bindArguments`, which `parameters_and_maps` describes. Finished results
agree at every fuel, and every result keeps the cells it started with at
their types and leaves the environment and locals as it found them. The
witnesses give `[counter(), counter()]` as `[1, 2]` with `count = 2`,
`counter() * 7 + counter()` as `9` with `count = 2`, `[defaulted(), count]`
as `[1, 1]` through a default that calls the counter, `[{ count = 2; 1 }]` as
`[1]` with `count = 2`, and the bridge's premises on `[7, 8][-0] + 2 * 2`.
Five mutants are rejected: a list step that drops nested writes gives `[1, 1]`
and leaves `count = 0`, right-to-left elements give `[2, 1]`, a right-first
`+` gives `15`, and a nested block run as a call or with the frame before it
loses the write. The bridge excludes local-function calls, `Ok` and `Err`
constructors, `??`, value blocks and every other form the evaluator leaves
unsupported.

The `statement_resolution` fragment root, in
[`Proofs/Core/Resolution.lean`](Xsh/Proofs/Core/Resolution.lean), adds scope
preservation to `Restoring`: every expression and value-block result leaves
with a value or another exit, never a bare normal completion, in a frame with
the environment and locals it started with. From it and the existing
`Represents` lemmas, under the canonical resolution, binding any target
declares its names in the innermost static scope, so destructuring targets
are tied to the pass; a statement that completes normally, a `guard let` of
either form included, leaves a lexical environment representing the pass's
scopes after it; and so does a statement list. The pass's `staticDeclare` now
declares binding and guard targets' names and what sugar's expansion
declares. The witness runs `let {key: a, value: b} = pair; guard let x = ok`
under the canonical resolution, where `a`, `b` and `x` hold `1`, `2` and `7`
and resolve to the pass's binders; the earlier pass, which declared only
single-name lets, is rejected.

The `callable_values` fragment root, in
[`Proofs/Core/Callables.lean`](Xsh/Proofs/Core/Callables.lean), covers SPEC
9.1 callable values. SPEC 3.1 allows declarations only at the top level
("inside a callable body or any block they are `check.nested-declaration`"),
so the machine refuses a function declaration in a block or a callable body
and every callable value names a top-level declaration. A name bound to a
local function evaluates to `Value.callable` with the runtime's identity for
its declaration, an oracle (`callableReference`, `callableCell`).
`value.call(...)` evaluates the receiver, then its arguments, and runs the
named declaration through `callBlock` and `returnTo`, so it writes captured
top-level `var`s through the caller's frame; a `Proc` value's call is the
proc's own `Result` or `Ok` of its value, and a `Pure` value's call is its
value. The witnesses run `var count = 0; proc bump() -> Int { count += 1;
count }; let fns = [bump]` and give `fns[0].call()?` as `1`, then `2`, with
`count = 2` (the verification binary prints the same), call `bump` through a
record and through a `twice(action)` parameter, and call a pure `double` for
`14`; a call that snapshots the store when the value is made gives `[1, 1]`
and is rejected, and a `proc` declared in a block is refused. Calls before a declaration are `declaration_hoisting`'s.

The `declared_constructors` fragment root, in
[`Proofs/Core/Constructors.lean`](Xsh/Proofs/Core/Constructors.lean), covers
SPEC 4.7 and 4.10 constructors. `type` and `error` declarations are top-level
only and bind no runtime name: the machine takes the program's declarations
as a parameter, visible throughout as the runtime makes them, and the
checker's declaration identity as the oracle `Machine.identity`. A record
constructor evaluates its arguments once in written order, places positional
ones in declaration order and named ones by label, fills a missing field from
its default and builds the record in declaration order, nominal for a nominal
schema. `Family.Variant(...)` builds an `Error` whose message is its one
optional positional argument or `Family.Variant` for a payload-free variant,
and its `message` field or `Family.Variant` for a payload variant. The
witnesses match the verification binary: `Point(x: 1)` before `type Point`
gives `{x: 1, y: 7}`, `Pair(right: bump(), left: bump())` gives left `2` and
right `1`, `Labelled("x", 2)` fills fields positionally, and the `Fetch`
errors carry `Fetch.Usage`, `custom`, `Fetch.Missing` with payload `where` and
facet `NotFound`, and `boom`, which a variant pattern and a facet pattern then
match. Declaration-order evaluation, label-free placement and a variant-only
default message are rejected.

The `call_arguments` fragment root, in
[`Proofs/Core/Arguments.lean`](Xsh/Proofs/Core/Arguments.lean), gives every
call of a local function, direct or through `.call(...)`, one argument binder
(`invokeWith`): arguments evaluate once in written order, an `@` splice
supplies a list's items positionally and a `...record` spread its fields by
label, positional arguments fill parameters in order and labelled ones bind
by name, then a default or a final rest parameter's `List`. A label given twice
or to a parameter already filled by position, an unknown label, a missing
required parameter and an extra positional argument are runtime failures
that `try` does not capture. The witnesses match the verification binary:
`sub(right: 2, left: 7)`, `sub(...pair)`, `sub(7, right: 2)`, `total(1,
@more)` and `total(1, 2, @more, 8)` give `5 7 5 10 20`; a local holding a
callable value, `let pick: Op = sub`, is called by name with the callee's own
result, so `pick(7, 2)` and `pick(right: 2, left: 7)` give `5 5`; `[sub][0].call(7, 2)?`
gives `5`, and `call(1, 2, 7)` and `call(1)` fail with `expected 2 arguments,
found 3` and `found 1`, escaping `try`. Rust does not compile a named argument
to a dynamic `.call` and rejects a spread or splice there, where SPEC 6.3
allows named arguments; the machine follows the SPEC. A mutant for each check
(dropping unknown labels, keeping the first of two, `null` for a missing
parameter, dropping extra arguments) is rejected. Constructors take their
arguments, puns and `...base` spreads through the same `threadArguments`.

The `capture_resolution` fragment root, in
[`Proofs/Core/Patterns.lean`](Xsh/Proofs/Core/Patterns.lean), extends the
pass into nested scopes: under the canonical resolution, pattern captures
bound in a fresh scope and comprehension and `for` targets declare their names
in a new innermost static scope, and a bound body's statements carry
`Represents` from there. A pass that declares captures in the enclosing scope
is rejected. The `pattern_names` root, in the same module, reads capture names
from the syntax: `patternNames` covers every pattern form, an alternation
taking its first alternative's names, and when every alternation's
alternatives bind the same names (SPEC 6.8) a successful match binds exactly
`patternNames pattern` up to order, so `capture_resolution` declares
`patternNames pattern` without the match's names. `[a, b] | [b, a, _]` binds
`b` then `a` on `[1, 2, 7]`, as the verification binary does (`21`); reading
an alternation as all its alternatives' names, or the names in order, is
rejected.

The `body_resolution` fragment root, in
[`Proofs/Core/Bodies.lean`](Xsh/Proofs/Core/Bodies.lean), ties function bodies
to the pass. A call's arguments bind exactly its parameter names, and its body
runs through `callBlock` as `runTail` on the declaration's environment entered
once, with the parameters as locals; `runTail` runs the statements before the
tail as `runStatements` does. Wherever the body's statements have completed
normally, the environment represents the pass's scopes from a fresh scope
above the declaration's, and the locals are exactly the parameters that scope
does not declare, since each declaration hides the locals it shadows
(`statement_hides`). `proc f(x) { let x = 2; return x }` gives `2` and a
shadowing `let` in a nested block leaves `x` as `7`, as on the verification
binary; a pass that resolves parameters above the body's own scope is
rejected. For a script's top-level functions, `script_body_resolution`
derives the premise on the declaration's scopes from `runScript`.

The `hoisted_scopes` and `script_body_resolution` fragment roots, in
[`Proofs/Core/Scripts.lean`](Xsh/Proofs/Core/Scripts.lean), derive those
scopes from `runScript` (SPEC 3.1). The pass's scopes where a script's
statements start (`scriptScopes`) declare its top-level functions and its
program's payload-free variants in the script's scope, and each statement
then declares its names in order. Under the canonical resolution, for a
script whose top-level declarations are `proc`s, `pure`s and `stream`s
(`modelled`; `cli main` stays outside), every closure hoisting binds belongs
to one of the script's declarations and is over an environment representing
the pass's scopes after that declaration: the hoisted functions and variants
and the top-level bindings declared before it, and none declared after it.
So is the closure a declaration binds when it runs. `script_body_resolution`
composes this with `body_resolution` for a script started in the empty
lexical environment: a call of either closure resolves its body as the pass
does (`CallsResolve`), with no premise on the scopes at the declaration.
Hoisting now follows the pass on every statement form it models: a top-level
`guard let` declares its target, sugar hoists as its expansion, and a
`proc` or `pure` the checker gives no declaration fails the hoisting as its
statement fails. `var count = 7; print (first()); proc first() -> Int {
second() + count }; proc second() -> Int { 1 }; let later = 2; print
(first())` prints `8` twice, as on the verification binary, and both of
`first`'s closures agree with the pass on `count`, `second`, `first`, the
variant `Info` and the unresolved `later`; a `guard let` binding read
through a function called above its declaration gives `7`, as on the
binary. A hoisting that closes every function over the environment after
all bindings, one that omits the functions declared after a closure, and
one that skips `guard let` are rejected. The verification binary fails
`let r = first()` above `proc first() -> Int { 2 }` with `unknown captured
name r`, though `first` does not read `r`; SPEC 3.1 makes only a read of a
binding before its statement runs a failure, and the machine follows the
SPEC.

The `declaration_hoisting` fragment root, in
[`Proofs/Core/Hoisting.lean`](Xsh/Proofs/Core/Hoisting.lean), models SPEC 3.1
hoisting ("Declarations are visible throughout their script or module,
including the statements before them"). `runScript` binds a script's
top-level functions and its enums' payload-free variants before any statement
runs, each closure over the environment at its declaration with the earlier
top-level `var`s writable, and every call's body sees every top-level
function, so `isEven` and `isOdd` call each other (a callee that sees only
the functions hoisted before it is rejected); payload variants are called
like functions (SPEC 4.8). So
a binding declared after a callable stays unresolved in its body, and reading
a captured top-level binding whose statement has not run is a runtime failure
(`unknown captured name`) that `try` does not capture. The witnesses follow
`tests/xsh/declaration-hoisting.xsh` on the verification binary: `early()`,
`double(7)` and `Point(x: 1).y` above their declarations give `1`, `14` and
`7`, `read()` before `var count` fails, and a `count` declared after `read`
stays unresolved; a hoisted `bump` writes `count`. An unhoisted machine and
mutants that read the uninitialized binding as stuck or as a default value
are rejected. Because an uninitialized read is now a failure rather than
stuck, `expression_store_threading`'s bridge assumes the frame's resolved
names, and those of every expression-bodied function's defining environment,
are initialized.

The `local_bindings` fragment root covers local function declarations, which
bind a closure over the declaring environment that reads captured cells at the
call (SPEC 9.2: "Stream producers observe current captured mutable storage
before each pull"), the implicit `.` of a handler, list comprehensions, `collect`
and `yield`, text, error-variant, facet and tuple patterns, `guard let`'s
`else { |failure| }` (the new `Statement.guardHandled`), and the canonical
resolver that replaces the oracle's binder and cell choice.

The `pure_forms_laws` fragment root and the witnesses in
[`Proofs/Core/Control.lean`](Xsh/Proofs/Core/Control.lean) cover what this
list no longer names: `Ok(...)` and `Err(...)`, return kinds for
expression-bodied functions, `??` with a named handler parameter (the new
`Expr.handlerBlock`), `Result[Bool]` conditions, `guard let` (the checker's
`Result` or optional form from an oracle), `loop` expressions and `break` with
a value, `return` without a value (`Unit`), index-selector assignment through
`evaluateSelectors` with the `Map[UInt]` key rule, `for` over `Map` in key
order, `Str` and `Bytes`, and alternation, alias, `Ok`/`Err` and variant
constructor and type patterns. Each binding form has a witness that its
binding is visible where SPEC says and a rejected mutant that leaks or hides
it: a block's `let`, `for` targets, `if let`, `while let`, `match` arms,
`guard let` and `??` handler parameters.

The core `Statement` syntax in [`Syntax/Core.lean`](Xsh/Syntax/Core.lean) now
has every `ArenaStmtKind` kind: `use`, `export`, type and error declarations,
the four function-definition kinds with parameters and defaults, signal hooks,
sugar through its expansion, and command statements with their run forms.
`TailBareIdent` is the existing `tailName`. Loader output, formatter-only
flags and spans stay outside.

BASE-NUM is closed by `numeric_semantics` in
[`Proofs/Base/Num.lean`](Xsh/Proofs/Base/Num.lean). It keeps the checked Int,
UInt and Duration results and IEEE Float equality. `%` fails with
`integer-overflow` whenever `/` on the same operands would, so the minimum
`Int` `% -1` fails rather than giving `0`; a remainder without that rule is
rejected. The theorem adds `Duration / Duration`, the `time.millis` and
`time.seconds` clamps from [`Values/DurationOps.lean`](Xsh/Values/DurationOps.lean),
and the bitset methods from [`Values/IntegerOps.lean`](Xsh/Values/IntegerOps.lean):
`.bit_and`, `.bit_or` and `.clear_bits` combine the operands bit by bit and
fail with `integer-bitset` on a negative operand, as the runtime does.

Float arithmetic, negation, comparisons and conversions come from
[`Values/FloatOps.lean`](Xsh/Values/FloatOps.lean). `FloatMeaning` decodes each
operand's raw bits to NaN, a signed infinity or a signed exact multiple of
2^-1074. It specifies `+`, `-`, `*` and `/` through an outcome: a quiet NaN, a
signed infinity, a signed zero, or the exact rational result with its sign. A
rounded outcome below the overflow midpoint must be `BinaryNearest`: no finite
binary64 encoding is closer, and a tie keeps the even encoding. At or above the
midpoint it is the signed infinity. The proof in
[`Values/FloatArith.lean`](Xsh/Values/FloatArith.lean) shows the selected
encoding decodes to the uniform-grid rounding at the value's own binade, that
every other finite candidate is on that grid or below the binade floor, and
that magnitude decoding is strictly monotone. `floatApply` meets the meaning
for every `NaNPolicy`, a policy being any choice of quiet NaN bits (bit 51
set). The meaning fixes every non-NaN result and leaves the payload free, which
the witnesses show with two policies; a signaling NaN result for 0/0 is
rejected. Unary minus flips only the sign bit, so NaN keeps its payload and
`-(+0)` is `-0` where `0 - 0` is `+0`. `<`, `<=`, `>` and `>=` are specified on
exact values with infinities at the ends and are false when either operand is
NaN; the implementation compares total-order keys and is proved to agree, and
the total order itself is rejected as a comparison. `.float()` rounds an Int
like arithmetic and is never infinite. `.floor()`, `.ceil()` and `.round()`
return the unique integer satisfying the floor, ceiling or
nearest-with-ties-away-from-zero bound, after the Int range check, and reject
NaN and infinities.

The special-value table (which operands give NaN, infinity or a signed zero) is
part of the meaning and shared with `floatApply`. The proved content is the
rounding, the overflow, the signed encodings, the comparison order and
determinism. The witnesses compute 0.1 + 0.2, both tie directions at
1 + 2^-53, the overflow midpoint at MAX + 2^970 against MAX + 2^969, subnormal
ties, the signed-zero and infinity cases, and the conversions of 2.5, -2.5 and
2^63. The rejected mutants are ties-away rounding, operand exponents decoded
one too high, unsigned zero results, a non-NaN result for 0/0, `.round()` with
ties to even, negation as `0 - x`, total-order comparison, and bitsets over
magnitudes. Float literals and the `Float` text holes `f`, `e` and `g` round
in [`Values/FloatLiterals.lean`](Xsh/Values/FloatLiterals.lean): the
`float_literal_rounding` root gives a literal the exact decimal value of its
digits and exponent, rounded to the nearest binary64 with ties to even and
overflow to infinity, shows `decimalFloat` computes it and nothing else meets
it, and keeps a text hole's reading finite. Its witnesses are the bits Rust's
`str::parse::<f64>` gives for `1.0`, `0.25`, `1.5e6`, `10e-3`, `0.1`,
`0.30000000000000004`, `1E+2`, `123.456e-2`, `1e400` (infinity), `5e-324` and
`2e-324`, the lexer's refusals `1.`, `1`, `1e` and `.5`, and the text hole's
`-1.5e3`, `7`, `1.`, `.5` and `-0` against its refusals `nan`, `inf`, `1e999`
and `1.5 `; truncation is rejected. A context whose `literals.float` is
`decimalFloat` gives Float literals that meaning; the expression contexts take
the lexer's reading as a parameter. `parseFloatText` reads `parse_float`'s
text as Rust's `parse_float_text` does: trimmed of Unicode whitespace, every
`_` removed, `inf`, `infinity` and `nan` in any case, otherwise a decimal
rounded as a literal is, overflowing to infinity; the witnesses check padded
and underscored text, the special words, overflow, `-0.0` and four refusals.
Outside the model: the Float math methods such as `atan2` and `format_number`, and the identity of the
`float-conversion` error, which the conversions collapse to `none`. The model
claims no Rust or hardware conformance. Witnesses that evaluate binary64
rounding with `decide` raise `maxRecDepth` to 10000, because `Nat.log2`
recursion follows the bit length of numerators up to 2^2200. They need no
`exponentiation.threshold` change: the kernel evaluates the large powers, so
the earlier `exponentiation.threshold 1100` settings were removed.

The validated-type obligations use the Core types directly.
[`Core/Views.lean`](Xsh/Core/Views.lean) names the four validation tags
(`NonEmpty`, `RelPath`, a nominal declaration and a bounded interval), each
base, and the base-view relation: a validated position at any depth of
`NonEmpty`, a list, map value, stream element, optional, result, callable
return, record field or union member may stand for its base. Record fields
and union members are matched in any order, with each target member paired
once. `RowsDistinct` requires distinct labels in every nested row, and
`eraseValidations` replaces validated positions with their bases. The erased
equivalence `≈` (`ErasedEquiv`) is the least equivalence that is closed under
the positions a view descends through and swaps adjacent record fields or
union members. Equivalent types view each other in both directions, and a
record keeps its field count. Core membership reads a retained
stream or callable contract upward-closed under views, because stream items
are checked against the retained type as they are produced.

VAL-WIDEN is closed by `validated_widening` in
[`Validation/Core/Widen.lean`](Xsh/Validation/Core/Widen.lean). Every view
carries members to members in every world, a view into a validated type starts
from the same tag with a base that views to the target's base, and no base
views as its validated type. A view between types whose rows have distinct
labels is precise: erasing validations on both sides gives equivalent types,
and equivalent types have the same members in every world. Such a view also
introduces no validation at any depth: `validationTags` lists the tag of every
validated position, and for each tag the target has at most as many
occurrences as the source. So `List[narrowPort]` does not view as
`List[widePort]`, and the witness shows that the tag clause alone rejects that
pair. Counts cannot place a tag, so the theorem also inverts every congruence:
a view between two lists, `NonEmpty`s, streams, optionals, maps with the same
key, results, callables with the same policy and parameters, records or unions
is a view between their matching parts, with record fields matched by label
and union members by the one-to-one pairing, and a view between two sets, two
modules or two nominal types relates only equal types. A validated source
viewed as a type with no validation tag is a view of its base, and a view from
a type with no validation tag keeps its outer constructor. These clauses take
every view apart from the outside in: the top-level tag rule settles a
validated target position, the validated-source rule moves a validated source
over an unvalidated target to its base, and the inversion for the shared outer
constructor handles every other pair; leaf shapes have no validated positions.
So each validated position of the target is matched with the same validation
in the source. The witnesses reject `Result[narrowPort, widePort]` as
`Result[widePort, Int]`, `NonEmpty[Result[narrowPort, widePort]]` as
`List[Result[widePort, Int]]`, a module with exports `{x: narrowPort, y:
widePort}` as one with `{x: widePort, y: Int}`, and a nominal type over `{p:
RelPath}` as the same declaration over `{p: Path}`. In each pair the erasures
are equal and the tag counts fit, yet each witness shows that any relation
with the clauses it names rejects its pair. The label premise mirrors Rust,
where a record type is a map keyed by label (`Type::Record` in
`src/sema/types.rs`) and cannot repeat one, and a schema that repeats a field
is rejected separately with `check.duplicate-record-field`; the premise is
assumed, not derived from that code. A witness shows that a record view can
drop a duplicate label, so the erased types differ in field count and are not
equivalent. Nested intervals are not views, even though the
interval-containment fragment relates them. A module contract entry must match
its kind and full signature exactly.

VAL-ERASE is closed by `validated_erasure` in
[`Validation/Core/Erase.lean`](Xsh/Validation/Core/Erase.lean): a member
read at its base is a base member, erasure is the identity for `NonEmpty`,
`RelPath` and bounded integers, removes exactly the declaration wrapper of a
nominal member, and keeps references and every base-view observation.

The `validated_store` fragment in
[`Validation/Core/Store.lean`](Xsh/Validation/Core/Store.lean) shows that a cell of a
validated type accepts exactly members, any other write is a `type-error`
that keeps the store, a validated value copied into a base-view cell keeps its
value and references, and later writes to the wider cell leave the validated
holder unchanged. The witness empties a `List[Str]` copy of a `NonEmpty[Str]`
and rejects the same write to the `NonEmpty` cell.

VAL-STORE is closed by `validated_store_writes` in the same module, over every
store-writing form of `store_writing_forms`: a field, key or index write or a
compound assignment commits exactly when the whole updated cell value is a
member, and otherwise raises `type-error` with the store unchanged, so no write
breaks a refinement or a nominal schema; a commit changes no other cell,
including a validated holder. `names[i] = v` on a `NonEmpty` cell commits
VAL-PRESERVE's `replaceAt` (SPEC 4.13). Where `operate .add` is list
concatenation, an explicit premise that BASE-STEP must discharge when it
instantiates `Operate` (SPEC 4.5), `+=` on a `NonEmpty` cell commits through
`add_assign_commits` and `+=` into a `NonEmpty` field of a well-formed record
keeps the field `NonEmpty`. The witness that empties a `NonEmpty` field is
model-level: Rust rejects that program at check time.

VAL-UNION is closed by `union_semantics` in
[`Validation/Core/Union.lean`](Xsh/Validation/Core/Union.lean). It shows that union
membership is membership in a listed member, runtime selection returns the
first accepting member in written order, and membership is preserved when each member views as one of the other's
in any order. General union fit and admissibility belong to G4/TYPE-*. Union admissibility, which
rejects overlapping members, is a checker judgment and is not assumed; the
overlap witness selects `UInt` for `5` and `Int` for `-5`, and a last-match
selector, which picks `Int` for `5`, is refuted.

The `narrowing` fragment root is in
[`Validation/Core/Narrow.lean`](Xsh/Validation/Core/Narrow.lean). It shows that a type
test is true exactly for members, a failed union member test narrows to the
remaining members, a non-null optional value is a member of its element type,
and cell facts learned from passing tests stay sound because assignment forgets
the assigned cell's fact. The witness shows that retaining a `NonEmpty` fact
after assigning `[]` is unsound.

VAL-REQUIRE is closed by `recursive_require` in
[`Validation/Core/Require.lean`](Xsh/Validation/Core/Require.lean), over every
Core type, value and world, for `.require`, `.require_exact` and
`.require_stripped`. `convert` is the boundary operation; `Converts` states the
SPEC conversion rule by rule, and the theorem proves that the published `Ok`
value is exactly `Converts` of the candidate, that it is unique and a member
of the target, and that every `Err` is a `schema` error whose structured
failure has no conversion and is located. The rules and their SPEC sources
(`docs/templates/SPEC.md`):

- Recursion: "Records are checked recursively, including inside lists, maps,
  and optional slots, and the result is published only if the whole value
  passes. Extra fields are kept. A `T?` field must still be present." The
  policies apply "through declared record fields, lists, map values, optional
  slots, ordered union alternatives, and validated bases. Maps keep their
  keys. `Any` and erased `Record` slots declare no field set and remain
  opaque." `Result`, streams, callables, error types, `RelPath` and bounded
  integers are checked by Core membership and published unchanged.
- Wire conversion: "Str-backed enum slots are converted from their wire
  strings" and "Unknown strings report the field or index path". The wire
  strings come from `Core.WireEnums`, Core's enum wire-declaration metadata.
- Map publication: "A `Map[Str, V]` slot accepts a map or a record and
  publishes a map of the record's fields ... maps with other key types accept
  only maps."
- Sets: "`.require(Set[T])` accepts a set, or an array whose elements are
  distinct values of `T`; an array that repeats an element fails with
  `schema`."
- Unions: "a value belongs to the first member, in the order written, that
  accepts it", and "If every union alternative fails, the error reports that
  union's path."
- Policies: "`value.require_exact(T)` rejects fields not declared by a
  structural record schema; `value.require_stripped(T)` returns only its
  declared fields", and "Rejection reports the unexpected field's path as a
  `schema` error."
- Modules: "A failed `.require(Contract)` on a module reports every violation
  in one error", with `MissingExport`, `MismatchedExport` and
  `UnexpectedExport` facets; `module_report` and `facets_report` prove both
  directions for each kind.

`Located` places each failure at a path into the candidate whose slot and
value show the reason: an unadmitted shape, a missing or unexpected field, an
unknown wire string, a rejected map key, a rejected or repeated set element, a
union no alternative of which converts, or the complete module violation list.
The premise `WellFormed` in
[`Validation/Core/WellFormed.lean`](Xsh/Validation/Core/WellFormed.lean) is the
type judgment that every record schema lists each field once (Rust
`check.duplicate-record-field`) and every module contract names each export
once (Rust `check.duplicate-name`); `WireWellFormed` requires each Str-backed
enum to list each variant and wire string once. Without `WellFormed` a
stripped publication of `{level: Level, level: Str}` is not a member.
`rowsDistinct_of_wellFormed` shows that a `WellFormed` type is `RowsDistinct`,
so it meets the label premise of VAL-WIDEN's precision and tag-count clauses.
A value export rejects a callable, because SPEC 4.9 reports a `MismatchedExport`
when "an export has another kind". A filesystem entry is converted as its field
record under every policy, because "Choosing a field policy does not change
which containers a slot admits". The error message renders the reported path,
which for an unexpected field is the field's own path ("schema check failed at
future"); its remaining wording is diagnostic.

Boundaries: map and set publication keep the candidate's order, while SPEC 4.5
orders maps by key and Rust sorts them; no Core operation reads that order
today. The lexical codec mode is reachable only from `decode_lexical`, never
from `.require`, and is not modelled. The expected and found export signatures
that SPEC 4.9 puts in module failure messages are not modelled.

The witnesses convert a wire string, report an unknown one at its field and
list index, publish a map from a record, follow the SPEC's `Config` example for
the three policies, report a union path, show that order decides enum
conversion, reject a repeated set element, convert a filesystem entry under
every policy, report three module violations with their facets, and report a
callable under an `Any` value export as `.mismatched "f"`. Mutants detected: a
check without conversion, a skipped enum conversion, a map published as a
record and a dropped field path.

The `nominal_provenance` fragment root is in
[`Validation/Core/Nominal.lean`](Xsh/Validation/Core/Nominal.lean). Core
membership alone admits a forged `.nominal d (.record [])`, so the theorem
adds a provenance judgment over value-forming expressions: scalar, list and
record literals, constructor calls, `.require`, store reads, field reads and
spreads. Every nominal identity in a produced value came from the store or was
named by a constructor or `.require` target in the expression, so a nominal
member was introduced there or read from the store. Constructors check their
schema; declarations that share a name and shape across modules stay distinct.
Calls, iteration and the other expression forms are not in this language; a
machine that adds them must show they are nominal-transparent. The witnesses
include a constructor resolved to another module's declaration, which the
theorem rejects.

VAL-PRESERVE is closed by `validated_operations` in
[`Validation/Core/Preserve.lean`](Xsh/Validation/Core/Preserve.lean): `push`,
`extend`, `+` with a `NonEmpty` on either side, a single-clause comprehension
without a filter and element replacement keep a `NonEmpty`; filtering, slicing
and comprehensions with several clauses return only the base list; `.extend`
returns a new `NonEmpty`, and `+=` on a `NonEmpty` variable is a store write
that commits and keeps the type; and `.parent()` and `.normalize()` keep a
`RelPath`. The path operations
read a path by its non-trivial components, as `strip_prefix` does, in
[`Validation/Core/Paths.lean`](Xsh/Validation/Core/Paths.lean), so `a//b` and
`a/./b` have parent `a`; `.normalize()` resolves `.` and `..` lexically, and
both return `.` when nothing is left. The witnesses show `relative_to`,
`with_ext`, `components` and `Port` arithmetic leaving their validated type,
including `p"...b".with_ext("")`, which gives `..` and fails
`.require(RelPath)`. A nominal spread `{...pkg, version: v}` gives a plain
structural record that is not a nominal member. The witnesses also kill a
result-type rule that keeps `NonEmpty` after filtering.
These definitions follow the SPEC and claim no Rust correspondence.

The `introduction_routes` fragment root is in
[`Validation/Core/Intro.lean`](Xsh/Validation/Core/Intro.lean). It shows that a judged list
literal of typed items is a `NonEmpty` and a rejected one is only plain
splices; path and bounded-integer literals are members exactly when the rule
or interval accepts them; an `fp` literal whose text passes the rule with each
`RelPath` interpolation read as `.` renders to a `RelPath`; constructors and
accepted `.require` results are members; and no view reaches a validated type
from anything else. Constants and field defaults use the same judgments.

Allocation now uses one generic typed-cell operation in
[`Semantics/Environments.lean`](Xsh/Semantics/Environments.lean). Both the
connected execution machine and shared-value store consume it. General laws
characterize acceptance, reject occupied cells and nonmembers, preserve every
existing cell, and retain the exact new value, type and mutability. Connected
binding, parameter and callable-store preservation proofs reuse these laws.
The shared adapter also rejects any unretained carried reference at allocation.
The connected machine still has its restricted value and membership domains;
this mechanism migration does not establish their correspondence with shared
membership. Type admissibility, valid worlds and complete syntax and execution
remain open, as do BASE-ENV and TYPE-MEMBER.

The experimental foreign-IR bridge also decodes candidate instruction rows,
binary-operation tables, locations, and string pools before passing a
reconstructed program to `FullVerifier`. Other semantic tables still come from
an in-process program. Source-to-IR validation and portable symbol ownership
remain open, so this bridge is not a certificate for foreign programs. The
instruction exchange also retains assignment operations and decodes assignment
path steps. Lean relates an assignment's decoded StoreValue ordinal and source
statement site to scoped checker facts.
An extended comparison reconstructs the replacement expression from raw rows
and from the checked node. It supports scalar literals, reads, binary operations,
records, projection, and conditional expressions, and value blocks that bind
temporaries outside the mapped slots before one value, as scheme lowering emits
for assignment right-hand sides. It rejects different same-type reads and
integer payloads. Its general theorem composes this comparison with
the scoped store typing theorem for every sorted solution. Name spellings come
from the admitted source's name table. The binder-to-slot map is derived rather
than supplied. The source parameter at position i takes slot i only when row i
of the owner function's range in the exported Rust parameter table carries the
parameter's spelling through the exported string pool and sets no rest, default
or reserved bits; the indexed IR's rule, which `FullVerifier` applies, is that a
slot below the parameter count holds the parameter row at that offset. Each
source local takes the slot of the StmtLet row that a test-only Rust export
claims for its declaration site. Lean accepts a claim only when the row lies in
the function's own statement blocks, its exported fact has the local's
mutability (a user `var` is thus told from a compiler temporary), the
initializer resolved through the
function's single-assignment temporaries equals the source initializer, and
binders and slots are injective. Corrupting a claim, a fact, a row slot, or a
store target is rejected for the exported scoped body, and swapped claims for the
exported two-local body. Swapped, renamed, flagged, missing and out-of-range
parameter rows are rejected for both bodies, and malformed parameter documents
fail to decode. The claim table, fact kinds, and the invariant that slots are
never reused remain assumptions of the Rust lowerer;
locals with equal initializers are told apart only by the uses a check covers.
Calls, annotated or destructuring declarations, other value blocks,
the older projected-row fixtures that still pass a map, and operational
correspondence remain open.
Assignment transfer exports retain the checker’s optional value-type override.
For the scoped fixture, Lean relates the emitted assignment fact to the outer
binding and checked projected row; altered transfer facts are rejected.
Resource-marked candidates remain unsupported by the Rust decoder; complete
source-to-IR and execution correspondence remain open.

The connected model represents nested value blocks with a Unit value and a
separate function-return context. Its contextual validators prove source and
node typing for checked blocks, including qualified instances. Block scope
restoration preserves writes to outer cells. Mutable bindings retain their
selected storage type, and assignments use structural compatibility while
preserving the stored value. The model checks qualified storage requirements
and relates them to runtime compatibility; source-derived unique rows and
runtime-relevant call-instance closure remain open. Source admission, lexical
effect bounds, translation validation, and whole-machine correspondence remain open.
The vertical source witness checker excludes adjacent identifier characters and
hyphens, decimal digits followed by identifier starts or dots, `E>`, `==`,
`=>`, `<=`, `>=`, `>>`, and `??`. It also requires each consecutive decimal
digit run to fit in a signed 64-bit integer, even inside an identifier. These
restrictions also exclude reserved words other than the six keywords used by
the vertical grammar, and exclude reserved function names and literal-let
bindings. They are
conservative restrictions on checker acceptance; they do not establish full
SPEC lexical agreement, parse-failure classification or tree-node spans.
The local recursion-depth cap in the record admission witnesses accommodates
kernel reduction of their concrete source byte lists.
The connected machine has conditional initialization, progress, step and
execution preservation proofs. Their premises include grounded function bodies,
storage and function-table certificates; deriving those premises from admitted
source and published Rust facts remains open in general. The call side is
derived: Lean computes the invoked declaration, type arguments, instantiated
parameter and result types, and integer or Bool literal arguments from the
admitted source and the decoded call and body facts, and proves the invocation
machine typed with integer-only faults and safe progress for any function entry
carrying the published scheme with a certified body. The scoped fixture's
derived machine equals its earlier hand-written one, and forged arguments,
instantiations, roles, sites, targets and indices are rejected. The body
candidate, store certificate and function-table entry remain hand-written. For the scoped
mutable-local fixture, its actual published Int call grounds the checked body
and closes its storage tag. Its source-selected literal argument initializes a
typed invocation whose finite model executions preserve typing, exclude internal
faults, and retain safe progress. Rust execution correspondence remains open.
A Result slice carries `Ok` construction, `Err` construction and `?` propagation
through the connected path for one test-only fixture of two single-return pure
functions over `Int`, `Str` and `Result`: `Ok(r?)` for a `Result[Int, Str]`
parameter and `Err("no")`. A test-only Rust export publishes the checker's
parameter and result types and the exported instruction, function and parameter rows.
Lean admits the source, validates hand-written candidate bodies against the
source and those types with a checked constraint derivation, and compares each
function's return row, read as a value tree with Ok, Err and propagation
forms, with the candidate's returned node. That comparison reads each
parameter at the slot whose row in the exported Rust parameter table carries
its spelling, as the derived slot map does. The connected machine executes the
validated nodes through construction, success propagation and propagation of
an error to the caller frame; it also unwinds a propagation inside the left
operand of an addition without evaluating the right operand, for a node the
fixture does not include. Controls reject forged signatures, swapped Ok and
Err in the candidate and in the rows, a dropped, misplaced or mistyped
propagation, and forged rows, function tables and parameter rows. A bare `Ok`
under `?`, in a binary operand, a field base, an unannotated declaration or a discarded
statement defaults its error type to `Error` (SPEC 8.5), while an `Ok` at the
function's return keeps the declared error type; witnesses reject the nested
`Ok(Ok(1)? + 1)` at `Result[Int, Str]` and accept it at `Result[Int, Error]`.
Call arguments and record fields are not yet expected positions in the model,
so agreement is shown only for the
fixture. The published facts also give the checker's type of every non-callee expression; Lean pairs them in pre-order with the validated nodes and requires the same types, and forged, shifted, dropped and extra entries are rejected.
A second fixture adds a call of a user function whose Result the caller
propagates: `wrap(n: Int)` returns `Ok(n)` and `lift()` returns `Ok(wrap(1)?)`.
Every function is in scope at the type of its published signature, so the
callee's signature feeds the caller's derivation. The direct call row reads as
a call of the callee's function entry, whose name must be the published one.
The checker publishes no type for a call-name site, so the export gives the
function the checker selected for each call instead, and Lean requires it to
be the function the validated node calls. Controls reject a forged callee
signature, a forged call-site or argument type, a forged, dropped or shifted
call target, a call of an unknown function, and call rows with another target,
argument or tag. The connected machine runs the certified caller node: it
enters the callee's frame with the parameter in a fresh cell, runs the
certified callee body, returns `Ok(1)` to the caller frame, and the caller's
`?` and `Ok` produce `Ok(1)`. With a callee body that returns an error instead,
the caller's `?` unwinds the pending `Ok` call and returns the error to the
caller's own caller frame. Those machine hypotheses follow from a certified
program (every fixture function bound at its published scheme with its
certified body, and `Ok` bound) together with the activation invariant at top
level; a concrete loaded machine satisfies both, and a callee loaded at a forged
scheme does not. Invoking the certified caller in that loaded machine runs its
whole body under the caller frame `callFunction` pushes and returns `Ok(1)` to
it. The typed-scope invariant (`ScopeStateAgrees`) holds over the certified
scopes at the caller's entry, at the callee's entry, where the parameter holds
the `Int` argument, and after the return; the certified scopes are shown equal
to those scopes. Both certified bodies satisfy `FunctionBodyCertified` over the
global scheme bindings (Ok, the callee and the caller), so the invoked machine
is a `TypedMachine`: every execution from it preserves the invariant, faults
only with an integer fault, and is terminal or can take a permitted step. Local declarations, recursion, the base machine and its lowerer, typed whole-function execution, and
source-site binding of propagation and call rows remain open. The source is kept short
because lean4lean exceeds its deterministic timeout when the kernel admits a
source of a few hundred characters.
A scoped fact certificate checks one Rust-exported generic mutable-local example
inside `without`, including source sites, storage requirements, call bindings,
and ten altered decoded facts. Its call check currently supports direct,
unshadowed top-level calls in equality assertions with literal arguments;
other call sites are reported as unsupported. Other effect regions and full
source-to-IR correspondence remain open; the older fact validator still reports
regions as unsupported.

Every claim here starts after parsing unless planned source-admission evidence
covers the source bytes. Rust is the only implementation compared so far. The
plan states the contract a second implementation must meet; no such
implementation or evidence about one exists.

[`Core/ContractSubstitution.lean`](Xsh/Core/ContractSubstitution.lean) characterizes
substitution of known and unknown effect contracts across arbitrary parameter
scopes. Given an initial exact inclusion or permission-coverage judgment, a
substitution preserves that judgment exactly when a resulting unknown contract
on the left also makes the right unknown. For known source expressions,
preservation under every substitution is equivalent to inclusion of their live
parameter dependencies. A parameter hidden by direct effects can therefore
break an otherwise valid inclusion when replaced by an unknown contract.
Source typing must establish the condition before using a substitution law.
Restricted-call permission keeps its separate known-bound rule.
The proof covers the shared effect domain, not source admission or Rust
conformance. It closes no universal obligation.

[`check-inventory.py`](scripts/check-inventory.py) and its
[tests](tests/inventory/test_check_inventory.py) validate inventory structure,
owner coverage, root reachability, and evidence records. They do not test Lean
theorem behavior or Rust conformance. [`xsh-lean.md`](../xsh-lean.md) is the
owner-managed plan and is parsed by the inventory checker; preserve its
G0–G10 structure when updating it.

## Source admission

[`Conformance/Vertical/SourceAdmission.lean`](Xsh/Conformance/Vertical/SourceAdmission.lean)
proves the fragment root `Xsh.Conformance.Vertical.SourceAdmission.source_admission`.
SRC-ADMIT stays planned: the root has no parse-failure judgment, its string
rejection clauses are the Lean scanner's relational reading, `D` admits no
string literals, comments or module inputs, and no Rust lexer correspondence is
shown. The relations `DeclaredString` and `EscapeError` relate the scanner's own
decisions; they are not an independent SPEC reading of source strings.
It decides, for arbitrary source bytes and no imported inputs, one of three
outcomes, each with an exact declarative range:

- `accepted functions scopes` holds exactly on the declared domain `D`. `D`
  fixes a module input list of `[]`, no conflicting adjacent bytes, decimal runs
  within the integer bound, distinct function names, supported names, a slot
  projection for every function, and a tokenization whose trees satisfy the
  independent grammar, ordered propagation, record-call exclusion, and the
  supported-word guard.
- `rejected (.invalidUtf8 span)` holds exactly where the bytes before the span
  are well-formed UTF-8 and no well-formed UTF-8 sequence starts at the span's
  start, stated against `ValidUtf8` and `StartsSeq` rather than the detector.
  The classifier's offset is the least such offset, and the span runs to the end
  of input.
- `rejected (.invalidEscape span)` and `rejected (.unterminatedString span)`
  hold exactly over the declared string envelope `E`, through the relational
  `DeclaredString` (whose invalid-escape case carries `EscapeError`): well-formed
  UTF-8 with no
  `#` comment byte, no `r"`, `b"`, `f"`, `fp"`, `g"`, `p"`, `rx"`, or `e"`
  opener, no `"""`, no `${` or `$(` interpolation marker, and no `'` byte.
  An invalid escape spans the backslash through the
  escape's syntax; a backslash before a line end spans the backslash and the
  line-end bytes. An unterminated string spans the opening quote through
  the line end, excluding the line-end bytes, or through the end of input when
  no line end follows.
- `outside` holds for everything else and makes no SPEC claim: valid UTF-8,
  outside `D`, and either outside `E` or scanning clean.

The accepted outcome also yields a `ScopesProjection` from the accepted
functions to the returned scopes, the positioned tokens of the exact bytes, the
grammar derivation, the decimal interpretation of every number token within the
bound, and a span for the token list that lies within the source. A node's span
is its own tokens' span by construction. `D` contains no quote byte, so an
input that fails the string scan is never in `D`; that disjointness makes the
ranges exact rather than merely sufficient. The classifier decides `D` through
`reconstruct`, so the accepted outcome names the very trees and scopes the
domain requires.

The boundary is the Lean model. It proves nothing about the Rust lexer,
parser, or diagnostics, and `E` and `D` are narrower than the full source
language: interpolation, triple-quoted layout, raw strings, and every
production outside the integer fragment are outside both. The SPEC's claimed
diagnostic codes and message text are not modelled.

## Checked Int arithmetic

`src/arith.rs::checked_int_binary` is the one decision for Int `+`, `-`, `*`,
`/` and `%`. Constant folding declines to fold on any error, and execution maps
the error to its `division-by-zero` or `integer-overflow` diagnostic.
`Core/Arith.lean` models it over mathematical integers. A zero divisor of `/`
or `%` is a division-by-zero error. Otherwise the result is the exact sum,
difference, product, truncating quotient (`Int.tdiv`) or truncating remainder
(`Int.tmod`) when its witness fits signed 64 bits, and an overflow otherwise.
The witness of `%` is its quotient, so `%` fails with `integer-overflow`
whenever `/` on the same operands would, and `MIN % -1` overflows as in Rust.
`checked_zero_divisor`, `checked_ok` and `checked_overflow` characterize the
outcome.

The corpus covers every operator over all pairs drawn from the i64 endpoints
and their neighbours, -7, -2, -1, 0, 1, 2, 3 and 7: 720 rows, SHA-256
`b29f28e51cad10be7c89517682a3a3eb57e15e8674077495e74c644ac180e6e6`. It assumes decimal values denote the same mathematical and Rust
integers. It is finite evidence, not universal Rust refinement, and it closes
none of the 77 universal obligations.

[IMPL.md](IMPL.md) records an `EXTRACTED_REFINEMENT` of this function and of
`src/duration.rs::checked_duration_binary`: Lean theorems over the Charon and
Aeneas translation of the production source, for every operator and every
signed or unsigned 64-bit operand, against `Core/Arith.lean` and
`Values/DurationOps.lean`. They are conditional on that pipeline and its
audited library models, taken from the laputa-systems Aeneas fork at
`6d7c51a5`, are not about the shipping binary, and leave `IMPL-REFINE`
planned.

```sh
LEAN_PATH="$PWD/.work/formal-build/lib/lean" lean --run formal/scripts/ArithCases.lean > "$TMPDIR/arith-cases.txt"
XSH_ARITH_CASES="$TMPDIR/arith-cases.txt" cargo test --locked --profile verification -p xsh --lib -- --ignored --exact runtime::eval::arith_conformance::production_matches_lean_int_arithmetic
```

Both exit 0. Five altered corpora, with a wrong remainder, a missing row, a
duplicate row, an out-of-domain operand and a malformed row, exit 101 with
`model/source disagreement`, `incomplete corpus`, `duplicate input`,
`out-of-domain input` and `malformed row`. The native constants,
typed-scalar route, operand-order, UInt, frame-span and Duration arithmetic
tests pass through `cargo dev gate`.

## Build and check

The [Lake config](lakefile.toml) keeps build and package output under the
repository's `.work/` directory. Before creating it, ensure `.work/` is ignored
through this checkout's Git exclude file. From the repository root:

```sh
repo_root=$(git rev-parse --show-toplevel)
exclude_path=$(git rev-parse --git-path info/exclude)
if ! rg -qxF '.work/' "$exclude_path"; then
  printf '.work/\n' >>"$exclude_path"
fi
mkdir -p "$repo_root/.work/tmp" "$repo_root/.work/lake-cache"
export TMPDIR="$repo_root/.work/tmp"
export LAKE_CACHE_DIR="$repo_root/.work/lake-cache"
lake --dir="$repo_root/.work" --file="$repo_root/formal/lakefile.toml" build
```

Use the pinned toolchain on `PATH`. From the repository root, check the
inventory:

```sh
python3 formal/scripts/check-inventory.py
python3 formal/tests/inventory/test_check_inventory.py
```

After building the model, generate the effect decision corpus from the
repository root:

```sh
LEAN_PATH="$repo_root/.work/formal-build/lib/lean" \
  lean --run formal/scripts/EffectPredicateCases.lean > "$TMPDIR/effect-predicate-cases.txt"
```

Run the manual production comparison in the pinned Linux test container, with
the target and linker environment from [Testing](../docs/TESTING.md). Set the
corpus path to its location inside that container:

```sh
XSH_EFFECT_PREDICATE_CASES="$TMPDIR/effect-predicate-cases.txt" \
  cargo test --locked --profile verification --target x86_64-unknown-linux-musl \
  -p xsh --lib sema::effects::effect_predicate_conformance::production_matches_lean_effect_predicates \
  -- --ignored --exact
```

The test rejects incomplete closed-mask coverage, malformed rows and a changed
expected result. Ordinary Rust tests do not require Lean or the corpus.

Generate and compare the deadline-selection corpus in the same environment:

```sh
LEAN_PATH="$repo_root/.work/formal-build/lib/lean" \
  lean --run formal/scripts/DeadlinePolicyCases.lean > "$TMPDIR/deadline-policy-cases.txt"
XSH_DEADLINE_POLICY_CASES="$TMPDIR/deadline-policy-cases.txt" \
  cargo test --locked --profile verification --target x86_64-unknown-linux-musl \
  -p xsh --lib runtime::eval::deadline_policy_conformance::production_matches_lean_deadline_policy \
  -- --ignored --exact
```

The deadline comparison requires every admitted input exactly once. Nine
altered corpora reject with the expected diagnostics, including a wrong decision,
a missing input, duplicates, and invalid or unsupported fields. Ordinary Rust
tests require neither Lean nor these generated corpora.

The membership comparison uses the Cartesian product of 33 types and 41
value fixtures in `MembershipCases.lean` and `membership_conformance.rs`.
The types include Any, Null, Bool, Int, UInt, Float, Str, Bytes, Duration,
Digest, Regex, Path, Unit, List[Int], NonEmpty[Int], Map[Str, Int],
Map[Int, Int], Set[Int], Optional[Int], a record requiring `x: Int`,
Int | Str, Result[Int, Str], List[Optional[Int]], Map[Str, record{x: Int}],
record{x: Int} | Bool, erased record, two enum tag types, Error,
ProcessError, an error family and variant, and an error facet.
Values include scalar representatives (including negative and zero integers),
empty and populated containers, mismatched elements and keys, and records with
missing, wrong, and extra fields, both result arms with matching and mismatched
payloads, nested valid and invalid containers, tags with and without fields,
structured errors of two families and variants, and a process error. The Result error payloads are
raw Str and Int candidates, not claims that source execution constructs them.
Records are open: extra fields are accepted.
String-keyed maps also accept records whose field values match. Map and set keys
are restricted to scalar key representations; this corpus exercises integer
and string keys. The precise fixtures are the declared finite domain, not all
values of these types. Record fields and map/set keys are unique; integers,
paths and durations satisfy their representation bounds. Fixture names identify
the same values and types on both sides; their preservation through execution
and source type admissibility are assumptions, not established here.

Nominal record identity is excluded: Rust has no runtime representation for it
and relies on the checker. Bounded integer types, other
unions and results, modules, filesystem entries, other key kinds and other
constructors are outside this corpus. Unknown and Invalid recovery types are excluded. Stream,
callable, scheme, generic and environment-path contracts are also excluded;
the Lean world sets retained predicates false and no admitted case consults
them. Rust static validation recognizes environment-path snapshots, streams by
presence and non-stream callables by kind, and rejects scheme and generic
contracts; runtime type checks also reject environment-path contracts. Prepared
validation instead authenticates callable, scheme and generic contracts and
stream item contracts, inspecting already stored items when no contract exists;
it recognizes environment-path snapshots. Correspondence, validity and
preservation of those retained facts remain assumptions and are not tested.

### Pilot 1A common domain and retained facts

The admitted inputs are exactly the named Cartesian product above, interpreted
as borrowed `ValueView::Runtime` and `ValueView::Lowered` values. The latter
are produced once per fixture by the production `lowered_value_from_runtime_any`
conversion; all admitted fixtures convert. A nested map or set key is borrowed
as `ValueView::Key`. Each fixture that is a key value (Str, Int, Bool, Bytes,
Path or Duration) is also checked directly as `ValueView::Key`, borrowed by the
production `MapKeyRef::from_value`, against each admitted key type (Any, Bool,
Int, UInt, Str, Bytes, Duration, Path): 56 rows, each agreeing with the Lean
decision. Key views are not compared against other types, which the checker
rejects as key types (`check.map-key-type`, `check.set-element-type`); the
production key interface rejects them. Computed-field views are not covered. The closed types are
exactly the 33 listed above, with no type variables, `Unknown`, or `Invalid`. The fixture
names and payloads in the Lean generator and Rust consumer define the finite
domain; the names are an assumed correspondence, not a serialization proof.

For these inputs Any accepts every fixture; each other scalar accepts its own
representation only, except UInt accepts Int values at least zero. In
particular Int accepts both negative and nonnegative integers and neither Int
nor UInt accepts Float or Bool. List[Int] accepts lists whose every item is Int,
including the empty list; NonEmpty[Int] additionally rejects the empty list.
Map accepts maps only when every key and value matches, including empty maps;
Map[Str, Int] also accepts records when every field value is Int, while
Map[Int, Int] rejects records. Set[Int] accepts sets whose keys are all Int,
including the empty set. In these fixtures the only key representations are
Str and Int; a mismatched key rejects a populated map or set. The production
key interface admits only Any, Bool, Int, UInt, Duration, Str, Bytes and Path
as non-recovery key types; other key types reject each key. Optional[Int]
accepts null or an Int. The record type requires an `x` field with an Int
value, accepts extra fields, and rejects a missing or mismatched `x` and all
non-record fixtures. Unions accept either named arm; Result checks the payload
against the corresponding success or failure type. Nested types apply those
same decisions to each contained value. The Lean row ends in `rowEmpty` and
Rust has no explicit open-row variable; both decisions accept extra fields.
Erased record accepts every record fixture. Each tag type accepts only tags of
its own enum identity, regardless of variant or fields. Error accepts structured
and process errors; ProcessError accepts only the process error. Family and
variant types require the corresponding structured error identity, and the facet
type requires the facet. Wrong identities, variants, and missing facets reject.
Record and key names are unique. Nominal record identity is unsupported: Lean
requires stored provenance, while Rust stores no such identity and checks the
underlying schema; constructing the requested same-shape identity contrast
would require a production representation change. Other values, constructors
and representations are unsupported by this finite claim, rather than inferred
from the table.

The shared decision receives a callback for retained facts. Its production
routes interpret those facts differently:

| Route | Stream | Callable | Scheme / generic | Environment path |
|---|---|---|---|---|
| Static argument/result checks | stream presence | matching non-stream kind | false | snapshot presence |
| Runtime type checks | stream presence | matching non-stream kind | false | false |
| Prepared argument and scheme-value checks | retained item contract matches; absent contract checks already stored items, and a live source without one errors | retained target type matches expected kind and signature | scheme instance and owning program accept the scheme; generic target has the requested kind and a quantified contract with no instance | snapshot presence |

Prepared checks can return an error while resolving a retained fact; that is
not a Boolean rejection correspondence. Scheme-value validation only invokes
the decision when the type contains a scheme callable. Omitted arguments and
other adapter control flow remain outside membership. Module authentication and
schema conversion are separate operations, not consequences of membership. Schema
decoding passes its immutable `&Evaluator` to module-contract authentication;
this shared reference alone does not establish which observations it needs.

For a future comparison involving retained references, Lean's
`ContractDecisions.stream`, `.callable`, `.scheme`, and `.generic` must decide
respectively the same route-specific facts as the Rust callback for the mapped
reference and requested type, signature, declaration, or kind. Callable and
generic Lean judgments additionally require kind equality. Environment paths
are a distinct Lean value constructor, not a `ContractDecisions` predicate;
its primitive judgment recognizes that constructor. Thus runtime's deliberate
false result has no such direct correspondence without restricting the domain
or choosing another model. The existing finite corpus has no such values or
contract types, so its false Lean predicates are never consulted.

Validity assumptions are that fixture mappings preserve representations and
identity, types and retained metadata are well formed, and each selected
callback fact describes the corresponding semantic object. Preservation
assumptions are that checking and execution preserve those mappings and facts,
including stream item contracts and callable ownership. Neither validity nor
preservation is proved by this comparison. Lean's `check_iff` proves only that
its decision agrees with its own judgment for the supplied world and decisions;
the finite test separately compares Rust outcomes for the stated fixtures.

The comparison calls the same `value_matches_static_type` used by argument
validation and its shared borrowed structural decision. Lean's `Membership.check`
is proved equivalent to `Membership.value` by `check_iff`; agreement with Rust
on this finite domain is evidence, not universal refinement. TYPE-MEMBER
remains planned, with 68 other whole-core obligations; eight of the 77 are
closed. Ordinary Rust tests do not need
the corpus or Lean.

Host verification with Lean 4.35.0 used these commands from the repository root:

```sh
export TMPDIR="$PWD/.work/tmp" LAKE_CACHE_DIR="$PWD/.work/lake-cache"
mkdir -p "$TMPDIR" "$LAKE_CACHE_DIR"
PATH="$HOME/.local/lib/lean4/v4.35.0-prebuilt/bin:$PATH" lake --dir="$PWD/.work" --file="$PWD/formal/lakefile.toml" build
python3 formal/scripts/check-inventory.py
python3 formal/tests/inventory/test_check_inventory.py
PATH="$HOME/.local/lib/lean4/v4.35.0-prebuilt/bin:$PATH" LEAN_PATH="$PWD/.work/formal-build/lib/lean" lean --run formal/scripts/MembershipCases.lean > "$TMPDIR/membership-cases.txt"
wc -l "$TMPDIR/membership-cases.txt"
sha256sum "$TMPDIR/membership-cases.txt"
XSH_MEMBERSHIP_CASES="$TMPDIR/membership-cases.txt" cargo test --locked --profile verification -p xsh --lib runtime::eval::membership_conformance::production_matches_lean_membership -- --ignored --exact
cargo test --locked --profile verification -p xsh --lib runtime::eval::type_view
```

Each exited 0: the build completed 290 jobs, inventory reported 77 obligations
and its 72 tests passed, the corpus contained 1353 rows plus its header (1354
lines), the comparison passed,
and all five type-view tests passed. The following controls generate six altered
corpora and run the same test against each:

```sh
python3 - <<'PYCONTROL'
from pathlib import Path
p = Path('.work/tmp')
rows = (p / 'membership-cases.txt').read_text().splitlines()
wrong = rows.copy(); a = wrong[1].split(); a[2] = '0' if a[2] == '1' else '1'; wrong[1] = ' '.join(a)
missing = rows[:-1]
duplicate = rows.copy(); duplicate[2] = duplicate[1]
outside = rows.copy(); outside[1] = 'outside null 1'
malformed = rows.copy(); malformed[1] = 'any null'
extra = rows + [rows[1]]
for name, content in [('wrong', wrong), ('missing', missing), ('duplicate', duplicate), ('outside', outside), ('malformed', malformed), ('extra', extra)]:
    (p / ('membership-' + name + '.txt')).write_text('\n'.join(content) + '\n')
PYCONTROL
for name in wrong missing duplicate outside malformed extra; do
  XSH_MEMBERSHIP_CASES="$TMPDIR/membership-$name.txt" cargo test --locked --profile verification -p xsh --lib runtime::eval::membership_conformance::production_matches_lean_membership -- --ignored --exact > "$TMPDIR/membership-$name.log" 2>&1
  status=$?
  printf '%s exit=%s\n' "$name" "$status"
  rg 'model/source disagreement|incomplete corpus|duplicate input|out-of-domain type|malformed row|corpus exceeds bound' "$TMPDIR/membership-$name.log"
done
```

Each test exited 101 with, respectively, `model/source disagreement`,
`incomplete corpus`, `duplicate input`, `out-of-domain type`, `malformed row`,
and `corpus exceeds bound`. The loop itself exited 0. The 1353-row corpus has SHA-256
`948d9cbdf1cf507fb39385f1c1ad675900299feef808216240a1646e0ac7fb30`.

From `formal/`, run the [project axiom audit](scripts/audit-axioms.sh), its
self-test, and the [fresh paranoid check](scripts/check-fresh.py):

```sh
/bin/bash scripts/audit-axioms.sh
/bin/bash scripts/audit-axioms.sh --self-test
python3 scripts/check-fresh.py
python3 -B tests/test_check_fresh.py
```

The audit rejects unsafe declarations, project axioms, `native_decide`, and
`sorry`; its permitted dependency list is defined in the linked script. The
paranoid kernel check is separate. It requires `bubblewrap` on `PATH` and
enabled user namespaces or a working setuid installation, and checks a
temporary workspace under `.work/tmp/`. Its sandbox passes the Lake thread
limit to con-leche's `--jobs` option as well as to Lean. Con-ron uses one worker
to fit its per-worker stack and scratch storage alongside the complete export
within the same 8 GiB address-space limit.
The fresh build limits [glibc malloc arenas](https://sourceware.org/glibc/manual/latest/html_node/Malloc-Tunable-Parameters.html)
to the configured worker count. The official prebuilt compiler otherwise fails
to create a thread under that limit on the qualification host. The same module
compiles with bounded arenas and the unchanged address-space cap.
If a build or checker exceeds its deadline, the command prints its captured
progress and terminates the owned process group. A timeout does not qualify a check.

## Pilot 2 effect algebra domain

The corpus enumerates all 128 effect sets in the order fs, net, process, env,
time, error, io. Each of the 16,384 ordered pairs checks union, exact inclusion,
and permission coverage. Lean uses `union`, `included`, and inclusion in
`permissionClosure`. Rust uses `EffectSet::union`, the finite `Included`
predicate decision, and `effect_covers`. The relation maps each bit to its
corresponding effect.

The contract domain contains unknown, known empty, three finite sets, two
variables, captured error, two symbolic unions, and a masked variable. The
121 ordered pairs compare contract union. Sixteen predicates cover equality,
inclusion, exclusion, and coverage with finite, unknown, and symbolic
contracts. Their 256 ordered pairs compare `EffectPredicate.implies` with
`effect_predicate_implies`. Both sides use the same variable identity mapping
and canonical nonzero masks. Unknown remains distinct from known empty.

Production implication is incomplete: it accepts identical predicates and
certain coverage implications with closed permission sets. The Lean soundness
theorem proves assignment-level coverage implication when all four contracts
are known; unknown contracts have no assigned denotation. Reflexivity holds for
every predicate. The comparison is finite implementation evidence, not universal Rust
refinement. It closes none of the 77 universal obligations.

Host verification used the build environment above:

```sh
LEAN_PATH="$PWD/.work/formal-build/lib/lean" lean --run formal/scripts/EffectCases.lean > "$TMPDIR/effect-cases.txt"
XSH_EFFECT_CASES="$TMPDIR/effect-cases.txt" cargo test --locked --profile verification -p xsh --lib sema::effects_conformance -- --ignored
```

Both exited 0. The corpus has 16,761 rows (SHA-256 `c74bc152a80e735d165fdd58d04e7ddc1d317a1e2b3512ab25695b96b6a231b3`).
Four altered corpora, with one flipped implication, the last row removed, a
duplicated row and an effect set outside the domain, each exit 101 with,
respectively, `model/source disagreement`, `incomplete corpus`,
`duplicate input` and `out-of-domain input`.
## Pilot 3 cleanup precedence

The model folds finite lists of errors with an identity and a forced flag. It
retains the first forced error, or the first error when none is forced, and
emits each displaced or secondary error immediately. The conservation theorem
relates the input multiset to the retained error and emitted sequence. The
model also states the managed-release recording rule and the final disposition
for supplied force and primary-failure facts. It assumes those facts and error
identities are supplied correctly; resource execution, traceback handling,
signal delivery, and frame scheduling remain outside the model. The Rust
adapter uses the same decisions; the finite corpus is conformance evidence,
not a universal proof of Rust refinement. This fixes a forced cleanup failure
being demoted behind an earlier non-forced failure. CLEAN-ORDER, CLEAN-ONCE,
and CLEAN-ERROR remain planned.

From the repository root, generate and compare the exhaustive corpus of all
four disposition inputs, four recording inputs, and sequences of at most four
managed/unmanaged plain/forced failures:

```sh
LEAN_PATH="$PWD/.work/formal-build/lib/lean" lean --run formal/scripts/CleanupPolicyCases.lean > "$TMPDIR/cleanup-cases.txt"
XSH_CLEANUP_CASES="$TMPDIR/cleanup-cases.txt" cargo test --locked --profile verification -p xsh --lib runtime::eval::cleanup_policy_conformance::production_matches_lean_cleanup_policy -- --ignored --exact
```

Both exit 0 over 349 rows. Four altered corpora, with a flipped report list,
a missing row, a duplicated row and a malformed row, exit 101 with
`model/source disagreement`, `incomplete corpus`, `duplicate input` and
`malformed row`. The regression
`explicit_run::tests::forced_cleanup_failure_overrides_earlier_failure` fails
before the fix with `forced cleanup must surface` and passes after it. The
defer, errdefer, within, resource, traceback, propagation and context-scope
native tests pass through `cargo dev gate` with unchanged counts.

The model and inventory checks run with:

```sh
(cd formal && lake build)
python3 formal/scripts/check-inventory.py
python3 formal/tests/inventory/test_check_inventory.py
(cd formal && /bin/bash scripts/audit-axioms.sh)
```

Each exits 0.

## Pilot 4 cleanup order and errdefer dueness

The model selects the next due action from a finite cleanup stack, then updates
its error flag when an executed action has a recorded failure. Its theorems
show that execution indices strictly decrease and occur once, that always and
managed-release actions execute, and that an errdefer executes exactly when
the initial flag is set or a later executed action has a recorded failure.
Resource execution, scheduling, tracebacks and host signal delivery remain
outside the model. The shared Rust selector has a finite comparison with the
model; this is conformance evidence, not universal Rust refinement. CLEAN-ORDER,
CLEAN-ONCE and CLEAN-ERROR remain planned.

From the repository root, generate and compare all stacks of at most four
actions over three kinds and three outcomes, with both initial flags:

```sh
LEAN_PATH="$PWD/.work/formal-build/lib/lean" lean --run formal/scripts/CleanupOrderCases.lean > "$TMPDIR/cleanup-order-cases.txt"
XSH_CLEANUP_ORDER_CASES="$TMPDIR/cleanup-order-cases.txt" cargo test --locked --profile verification -p xsh --lib runtime::eval::cleanup_order_conformance::production_matches_lean_cleanup_order -- --ignored --exact
```

Both exit 0 over 14,762 rows. An altered expected trace exits 101 with
`model/source disagreement`.

The model and inventory checks run with:

```sh
lake --dir="$PWD/.work" --file="$PWD/formal/lakefile.toml" build
(cd formal && bash scripts/audit-axioms.sh)
python3 formal/scripts/check-inventory.py
python3 -m unittest formal/tests/inventory/test_check_inventory.py
```

### Cleanup order and precedence composition

The composition accounts for every executed failure exactly once across the
recorded and direct reports, selects the first forced recorded failure or the
first recorded failure, and sets the final error flag when a recorded failure
executes. A non-forced managed-release failure cannot become primary. The
model covers finite action lists; the two existing Rust corpora provide finite
conformance evidence, not universal refinement. CLEAN-ORDER, CLEAN-ONCE and
CLEAN-ERROR remain planned.
