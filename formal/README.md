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
that is absent, not a theorem, or outside the project modules. No row is closed
yet. The finite Rust-conformance ledger holds only the named tests that ran
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
All universal effect obligations remain planned.

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
Those obligations remain open. BASE-ENV and TYPE-MEMBER remain planned.

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
conformance. EFF-LATTICE and all other universal obligations remain planned.

[`check-inventory.py`](scripts/check-inventory.py) and its
[tests](tests/inventory/test_check_inventory.py) validate inventory structure,
owner coverage, root reachability, and evidence records. They do not test Lean
theorem behavior or Rust conformance. [`xsh-lean.md`](../xsh-lean.md) is the
owner-managed plan and is parsed by the inventory checker; preserve its
G0–G10 structure when updating it.

## Checked Int arithmetic

`src/arith.rs::checked_int_binary` is the one decision for Int `+`, `-`, `*`,
`/` and `%`. Constant folding declines to fold on any error, and execution maps
the error to its `division-by-zero` or `integer-overflow` diagnostic.
`Core/Arith.lean` models it over mathematical integers. A zero divisor of `/`
or `%` is a division-by-zero error. Otherwise the result is the exact sum,
difference, product, truncating quotient (`Int.tdiv`) or truncating remainder
(`Int.tmod`) when its witness fits signed 64 bits, and an overflow otherwise.
The witness of `%` is its quotient, so `MIN % -1` overflows as in Rust.
`checked_zero_divisor`, `checked_ok` and `checked_overflow` characterize the
outcome.

The corpus covers every operator over all pairs drawn from the i64 endpoints
and their neighbours, -7, -2, -1, 0, 1, 2, 3 and 7: 720 rows, SHA-256
`b29f28e51cad10be7c89517682a3a3eb57e15e8674077495e74c644ac180e6e6`. It assumes decimal values denote the same mathematical and Rust
integers. It is finite evidence, not universal Rust refinement, and it closes
none of the 77 universal obligations.

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
on this finite domain is evidence, not universal refinement. TYPE-MEMBER and
all 77 whole-core obligations remain planned. Ordinary Rust tests do not need
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
