# Architecture

XSH checks a complete program before execution. Parsing produces an arena
program and a lossless concrete syntax tree (CST). The checker publishes
semantic facts, lowering builds indexed IR, and verification gates execution.
The CST serves formatting and tools that preserve source. The evaluator runs
only the verified indexed program.

[`SPEC.md`](SPEC.md) owns language behavior. This page maps the implementation
and its invariants. [`DESIGN.md`](DESIGN.md) explains syntax choices;
[`TESTING.md`](TESTING.md) owns build and test procedures; [`XSHT.md`](XSHT.md)
and [`XSHT-FMT.md`](XSHT-FMT.md) own tool behavior.

## Packages

The Cargo workspace and its members are listed in `Cargo.toml`. Put a Rust
subsystem in a separate crate when its stable boundary does not depend on XSH
spans, values, diagnostics, or evaluator state. `xsh-net` is the example: its
adapter converts XSH records to Rust requests and turns results back into XSH
data.

## The `libxsh` facade

`src/lib.rs` defines the public library surface. `tests/libxsh_api.rs` protects
it. The supported tier is script execution, diagnostics, traces, and the
narrow host adapters in `xsh::host`. Frontend, IR, and process types also serve
first party tools and remain coupled to the implementation. The compiler,
runtime, syntax, checker, and standard modules stay private behind the facade.
A separate core crate or foreign-function interface needs a concrete consumer.

Core command behavior lives in XSH under `core/`; shared XSH libraries live in
`core/lib/`. Standard-module signatures live in `crates/xsh-registry`, host
operations live in `src/modules/`, and operations that need evaluator state
live under `src/runtime/eval/`. Keep command policy and behavior in XSH. Rust
implements host operations and representation; command-specific performance
shortcuts do not move policy into Rust. A native performance exception needs
an XSH implementation and measurements that identify the smallest operation
needing acceleration.

Module resolution is owned by `src/loader.rs` and `src/project.rs`. The loader
searches beside the importing file, then `XSH_MODULE_PATH`, then project module
roots. The executable does not read the rest of the `xsht` configuration.

## Pipeline

| Stage | Main owner |
|---|---|
| Grammar and syntax tables | `src/syntax/grammar.rs` |
| Lexing, CST, and arena program | `src/syntax/lexer.rs`, `src/syntax/cst.rs`, `src/syntax/parser.rs`, `src/syntax/arena.rs` |
| Module loading and project roots | `src/loader.rs`, `src/project.rs` |
| Semantic checking | `src/sema/check/` |
| Lowering | `src/runtime/eval/lower.rs` |
| Indexed schema and verification | `src/runtime/eval/indexed/full/` |
| Execution and stage scheduling | `src/runtime/eval/lowered_run/indexed_run/` |

The parser keeps source structure for tools. Semantic passes use the arena
program and checker facts. `Checker` owns type, signature, and import-binding
decisions; it publishes selected overloads, argument bindings, types, and
effects for lowering and tools. Later stages consume these facts instead of
repeating semantic decisions.

`FullBuilder` encodes the checked program as indexed rows and pools. A
construction gap is a diagnostic. The builder runs `FullVerifier::verify`
before it returns a program. Verification checks schemas, ranges, ownership,
IDs, and references, then derives the marks used by the in-place evaluator.
Other instructions use the explicit frame scheduler. Runtime decoders rely on
the verified representation.

Driver lowering selects immutable input bindings from resolved reads before
reserving slots. Each step allocates a dense frame for those inputs and its
locals; unused known bindings allocate no slot or type-fact row. Every mutable
binding remains available for root synchronization, and signal hooks retain all
known lexical slots. Resolved expression, shorthand, command, and tail reads
preserve binding identity when selecting inputs. Missing binding identities
conservatively retain their slots. The indexed schema and runtime slot checks
use this same dense frame layout.

Immutable driver writer aliases are authenticated iteratively. Each link keeps
its lexical owner and physical input header, and exact type comparison unwinds
after the terminal writer is proved. The shared cycle guard is released on
success and error, so later proofs can reuse the decoder safely.

Driver temporary type metadata supports annotation checks. Generic and callable
contracts require independently authenticated writer evidence. Missing optional
type information does not relax those contracts.
An explicitly annotated Optional record argument may retain its closed source
type when a null field prevents standalone constructor derivation. Its owned
writer still requires the ordinary producer proof or a complete declaration
validator. The validator runs before binding publication and before the call;
a call's expected result cannot supply this source authority.

Each prepared generic local declaration carries its checked storage type and
an immutable or mutable role, tied to its actual owner and declaration
instruction. The verifier authenticates the initializer before making the
binding visible, checks every assignment against that storage type, and
requires explicit mutable permission. Header and pattern bindings are
intrinsically immutable. Missing, duplicate, foreign, or escaped declaration
evidence is rejected; multiple writers never grant mutation permission.
The lowering slot scope stores one canonical `SchemeType` per resolved binding.
Ordinary bindings use `Ground`; generalized locals retain their published
storage relationship. A ground use-site fact cannot replace an open storage
contract. Field and indexed assignment elaborate checks from that same contract,
so a closed unsigned leaf remains checked inside an open record.

Generic assignment uses the ordinary assignment-path rows. Each selector and
the right side retain their elaboration inside separate lazy value blocks.
Execution evaluates selectors before the right side and commits through the
ordinary update mechanism. Cold verification starts from the actual mutable
declaration, projects each field or collection domain, and proves every selector
and replacement. Compound replacements preserve the storage domain; an unsigned
check cannot substitute for proving the actual operand and target.

Generic body verification uses an explicit obligation stack and an arena of
lexical frames linked by parent indices. Child obligations finish before a
parent continuation can publish later bindings. The existing indexed return
analysis supplies reachable loop exits, including bare `Unit` exits, even
when ordinary producer demand is absent. Preparation retains no frontend tree
for execution and performs no runtime type solving.

Named record constructors in generic bodies reuse ordinary lowering's checked
instance identity, field order, and defaults. Supplied source entries evaluate
before defaults. A prepared constructor schema must match its exact checked type;
cold proof independently derives every physical record field before accepting
the result or its nominal identity. A body result annotation cannot supply that
source evidence or authorize a constructor from another function.
Only checker-selected constructor callees use a static namespace role. Their
arguments and independent module-value reads retain ordinary capture rules.
Checked generic arguments retain exact closed null-bearing shapes through the
existing fresh invocation owner. Ordinary nullable storage seeds still require
non-null payload evidence. A ground checked result context relates to that same
instance before source argument checking through unresolved input variables
bound to actual fresh list literals. Static argument binding selects those
inputs, and only compatible result projections contribute early constraints. Computed arithmetic result variables retain operand-selected
signatures, including when aliased into an input. A checked UInt context constrains
an unresolved intrinsic arithmetic result to Int; existing storage and return
boundaries retain UInt validation. Journaled watcher inheritance follows result
aliases across representative unions before queued operations run. The existing expression completion owner retains the complete result relation
after argument solving and wrapper elaboration. This context does not bind declaration or
recursive-component monotypes. Contextual list literals retain their actual
checker domains as owned producer contexts. A shared context-role classifier admits
closed opaque data only for physical list literals or list builders with an outer
List domain. The literal-argument reader selects that role; other context readers
retain static substitution rules. Cold proof checks actual members, writers,
ownership, and context use before accepting strict generic binder evidence.
Existing Any and Optional widening use checked-value argument elaborations rather than treating
a formal target as the actual producer type.
Error variant constructors in generic bodies reuse the canonical checked
family, variant, and source-ordered payload binding from ordinary lowering.
Generated default-message and unsigned-check rows retain actual producer types
and immutable scope ownership. Scoped proof authenticates the exact error
identity and complete unique payload against its canonical declaration before
accepting the result; constructor namespaces remain static only at selected
constructor sites.
Constructor instance completion closes schema arguments; surrounding callable
generalization and final fact resolution own the supplied expressions. A closed
constructor field does not close the open row from which its value was projected.
Generic schema applications retain canonical declaration identity and scoped
scheme arguments alongside their actual field contracts. The callable's existing
generalization mapping owns these arguments; phantom arguments without input
evidence remain unresolved errors. Indexed schema metadata uses the same binder
and owner checks as other scheme types. Runtime prepared schemas and validation
targets require closed, reifiable types.
Parametric construction uses the checked-value record-constructor case. Its
fresh scheme-schema root belongs to that exact instruction, including for local
and discarded results. A validated template handle selects one normalized
canonical declaration template per used definition. Existing scheme substitution
links the actual type arguments to that template's field and nominal contracts;
omitted fields retain their canonical literal defaults. The declaration's
canonical nested schema context also retains phantom alias arguments erased by
field-type expansion. Field provenance follows authenticated immutable
initializers and projections, declared parameter schemas, and generic call
return schemas under their actual substitution. A scoped field context without
independent producer provenance blocks lowering. The existing indexed binder-use
census identifies phantom positions before context arguments are visited; only
those positions require exact context equality. Actual field proofs continue to
own ordinary structural compatibility. Cold verification checks
those relationships and the fresh record's actual field producers before
accepting its declared constructor contract. Ground callable return schemas
also retain their canonical checked declaration contract through compact
lowering. Their roots belong to the exact ground function; arguments are
verified under an empty binder scope. A field supplied by a ground call uses
that selected callee's return schema, never the caller's expected annotation.
Ground body verification still proves the actual physical return type and
nominal validation. Phantom arguments absent from that representation remain
part of the authoritative checked declaration contract; indexed ownership
checks do not prove correspondence back to the source declaration. Ground storage and
nominal construction retain their ordinary schema checks. The constructor case
passes through only in verified indexed execution and rejects owned decoding.

Argument expansion shares one source-entry traversal for ordinary `Type` facts
and generalized `SchemeType` facts. Each spread requires a checked finite closed
record. The existing binding map selects destination slots; lowering reads a
borrowed ground or scheme parameter layout and preserves source evaluation order.
Registered argument checks remain attached to their checked source entries.

Generic default expressions join the declaration's existing type, selected-call,
and capture census before generalization. They lower to the ordinary lazy
`DefaultParameter` prefix, including literal defaults. Omitted interior slots
carry the ordinary default token to the callee. Every supplied argument evaluates
before that prefix, and each supplied slot skips its default. Default expression
scopes expose the declaration's captures; formal names enter scope for the body.

Local empty `map.empty()` and `set.empty()` constructors retain their declaration's
actual inference cells in expression and binding facts. Map key and Set element
cells carry the finite `MapKey` requirement defined by `Type::is_map_key`.
Unification journals and propagates that requirement; generalization retains it
on callable type parameters, and every invocation enforces it on the actual
substitution. Imported schemes and callable contracts preserve the same bound.
Cold verification requires it for symbolic Map and Set domains and rejects
missing or malformed parameter metadata. These rows belong to the in-memory
Full IR; they do not define a serialized program format.

Callable application requirements distinguish invocations from known argument
relationships. An invocation owns its actual callee, kind, result and effects.
A known argument relationship retains the real formal layout and the original
argument types in the same scoped worklist and transaction journal. Nullable
formals defer assignability while an input remains unresolved; an independent
field requirement can establish a present record. Remaining relationships
are generalized without changing the input to an Optional. Cold call proof
uses the exact owner and binder and preserves the actual callee's schema and
unsigned runtime guards. Argument relationships cannot supply result, effect,
field-presence or already-validated-value evidence.

Known `FsRoot` receiver calls inside generalized bodies retain their selected
registry signature and source argument-to-parameter mapping. They use the same
receiver-first native operation as ordinary rooted calls, including omitted
optional arguments. The closed receiver, result, effects, resource owner and
native producer contract remain independently checked; no method spelling or
caller supplies the receiver type.

Generic native `json.encode` uses the registry's `JsonCompatible` argument role
for its value. The shared rule checks the actual closed input type with
`Type::is_json_compatible_with`. Wire tags require a retained declaring mapping.
Selected JSON calls retain those mappings once per build. Publication scans the
encoded call census once and preserves existing mapping indices.
Explicit `Any` keeps the ordinary fallible runtime encoder. The optional Boolean
and `Result[Str, Error]` use the existing template relationship. Cold proof retains
every input producer and charges input and mapping traversal.

Pipeline key requirements use `SealedOperation::TypePredicate` in the existing
operation authority. A predicate carries its actual subject type and has no
result, invocation, effect, or computational use-site. `SortKey` accepts `Int`,
`Str`, `Bool`, `Path`, and records of supported keys. `CountKey` accepts `Str`,
`Int`, `UInt`, and `Bool`. These requirements remain distinct from `MapKey`.
Generalization retains the whole subject, including row tails. Invocation
substitution checks every actual field. Cold pipeline proof derives the key
from its input or callback and requires the matching scoped predicate.
Computational operations retain their existing use-site ownership checks.
Sum keeps its Int input and result. Min and Max reuse the existing selection rows
with a distinct `MinMaxItem` predicate on the actual item. That predicate rejects
only outer Any, preserving opaque fields and nested collections under the
ordinary typed ordering behavior. The result retains the item under Result.

Cold generic verification proves actual fixed arguments, rest items and splices
against the selected callable contract. Default prefixes require unique matching
parameter entries, concrete checked parameter representations, actual value and
check compatibility, and independent expression and effect proof. Their lexical
frames exclude formal slots. Resource-bearing default block results use the same
transfer and cleanup authority as other lazy body values.

A generic constant-key projection follows actual immutable initializer
instructions in their lexical creation scopes. It does not infer a key from the
claimed result type. Generalization replaces monomorphic projection residues
with the same canonical scheme-body facts used for other expressions.
Concatenation checks each real operand's payload guarantee before accepting its
result claim; an ordinary complete unsigned check remains a separate boundary.
The checker and verifier share the canonical concatenation validation rule.

Generic constructors reuse the ordinary format parts, list builders, record
entries, and map entries. Format operands retain their individual value blocks;
all constructors preserve source evaluation order. Verification proves every
actual operand, including overwritten fields, before accepting the final
ordered fields or collection payload. Display and map-key domains come from
the checked operand types. Generated set conversion retains the canonical
selected registry method and uses the same native-call proof as a source call.

Inferred pipeline inputs use the existing iteration relationship with a narrowed
container policy. Its five states distinguish ordinary iterable error bounds,
pipeline error bounds, and pipeline error transport. Intersections retain both
the pipeline domain and any enclosing iteration restriction. Full stores the
policy in the existing parameter row and rejects unknown tags or mismatched
error payloads. Pipeline stream transport retains actual source-owner and
producer proofs derived from the physical pipeline instruction.

Generic buffer adapters reuse the ordinary TextLines, BytesChunks, and JsonLines
rows. Their first stage establishes the actual Str or Bytes source and item
type before iterable adaptation. Verification authenticates that source, the
chunk-size expression, stage position, and result under the same function owner;
the ordinary materializing engine retains parsing and cleanup order.
JSON forwarding returns retain the authored `List[Any]` boundary required for
opaque outputs; concrete results such as item counts remain inferred.

Inferred table items use the existing open-row relationship with no required
fields. This constrains only the record head and preserves opaque field types.
The existing table stage rows retain checked column argument mapping, while
cold body proof checks the actual record item and `List[Str]` configuration
under its source owner before accepting the terminal `Unit` result.

Generic shape stages reuse the existing indexed stage rows. Batch derives a
list item, Zip derives both item fields, and Range replaces the item with Int.
Stage configuration follows its checked argument slots and source order.
Range configuration bindings remain visible only to that stage's bounds.
Byte-bounded batches use the existing `Argv` conversion requirement. A disabled
argv limit requires proof of the actual immutable false value before omitting
that requirement. Zip stream transport authenticates its actual side producer
against the same physical pipeline and source owner.

Generic Each and Tee stages reuse ordinary statement blocks. Their immutable
item slot belongs to one callback scope, and the body completes without a value
tail. Named callbacks retain their actual Unit or Result[Unit] return contract;
ordinary statement consumption propagates failures and preserves cleanup.
Typed named callbacks retain their canonical call substitutions even when the
return type is concrete. Concrete item arguments derive from the scoped
pipeline prefix and retain its source and writer proof obligations.

Arguments read from ordinary line loops use the existing iteration-binding proof.
Their item type comes from the actual text or byte source and registered `lines`
contract; source and writer proof obligations remain mandatory.

Generic comprehensions reuse the ordinary qualifier rows and iterable adaptation.
Every source must establish its actual item type before its immutable target
bindings enter a child lexical frame. Filters use the checked control-condition
role. Later qualifiers and the projection see those bindings; earlier sources
cannot read them. Source, filter, key, and value elaboration stays inside lazy
value blocks. Resource-bearing block results transfer through the existing
cleanup mechanism before iteration begins. Cold verification proves every
source and projection, checks propagated error families, and accepts `NonEmpty`
only for the canonical single-source unfiltered mapping.
A fresh list comprehension used as a generic argument derives its closed item
from the actual qualifier bindings and projection. The ordinary argument demand
then verifies the complete source, targets, filters, and projection; an expected
parameter type does not supply the comprehension type.
Guard-bound arguments derive their actual payload from the guard source and
target, including the error binding inside a terminating handler. The canonical
completion query proves that handler cannot continue before a ground payload is
published. Lexical binding, complete writer census, and ordinary argument proofs
remain mandatory.

Shorthand fields contribute their resolved binding identities to the same
body-read and capture census as explicit reads. Their checked constructor
context supplies elaboration types; physical slot proof independently
authenticates each read. Static record updates reuse the ordinary indexed
update row and its canonical nonempty/disjoint path checks. Generic field
projection borrows through checked records and materializes only the selected
leaf under the existing budget. Actual replacements must preserve that field's
guarantees; complete typed storage boundaries remain explicit executable checks.

Generic binary operations prove their actual operand domains before accepting
their result facts. Equality retains checked parameter relationships; ordering
uses the same scalar-domain rule as ordinary checking. Membership reuses the
canonical key or comparison contract, and set results require both operands to
preserve their element guarantees. Boolean right operands keep all elaboration
inside a lazy value block. Comparison chains store the first operand and an
ordered list of operator, right operand, and span tuples. Both lowering paths retain each actual operand
once. Execution carries each reached right value into the next comparison;
cold verification proves every operand, including unreached links. Nested chains
therefore do not duplicate their middle operand subtrees.

Indexing and slicing share the journaled sealed-operation lifecycle with
arithmetic. Their requirements retain the receiver, key where present, result,
and source operand form. Inputs determine the domain; an integer key or expected
result cannot choose a list instead of an integer-keyed map. Constant record and
module keys retain the actual selected name. Independently known open records
project required constant fields through the existing journaled row constraint;
a string key cannot establish a record domain. Slice sites prove their ordered
bounds as integers and authenticate which bounds were present.

A leading open-record spread and finite named or shorthand overrides retain a
`RecordOverride` requirement in the same sealed-operation lifecycle. The
requirement owns the receiver, ordered override names and types, result, and a
distinct output row remainder. The output remainder removes overridden labels
from the input remainder. Input rows may contain those labels. Cold verification
proves the actual spread and each ordered value, derives known output fields,
and checks the complete result when the input closes. Duplicate overrides retain
all value evaluations and use the last value's type. Shorthand lowering uses each
source override's type rather than the final record field type. Existing record
rows execute the construction without a new opcode or runtime solver.

The indexed `Index` operand distinguishes an ordinary expression from a nonzero
negative-literal distance. Cold verification authenticates that form against the
owned requirement and actual producer contracts. A generic negative literal
uses the existing invocation substitution to select list-from-end access or a
negative map key before the receiver value is read. Binding-dependent sites and
their ancestors use scheduled execution, preserving the active substitution on
both evaluator routes. At body closure, a registered source operation with a
resolved List receiver publishes its owned negative-literal distance for ordinary
lowering. A closed Map keeps its ordinary negative key. Computed
negative indices retain ordinary evaluation; there is no runtime overload
search or value-tag selection of literal meaning.

Unsigned map keys use the canonical UInt validator and existing lexical
propagation flow. At erased function returns, the authenticated substituted
outer envelope determines whether propagated errors cross a plain boundary or
remain Result data, matching grounded returns without inspecting payload values.
The cold verifier relates the original Int key to the lowered immutable
UInt-validation temporary and checks the conditional error contribution against
the callable's own return boundary. Caller-side capture cannot replace that
callee contract; an already-UInt key has no validation contribution.

The focused ordinary/generic UInt-key parity control observes the same outcomes
on both evaluator routes:

| Map item type | Valid `Int` key holding the payload | Negative `Int` key |
| --- | --- | --- |
| `Str` | original string | propagated Schema failure |
| `Result[Int, Error]` | original error payload | Schema error as Result data |
| `Result[Int, Str]` | rejected for an `Int` key | rejected for an `Int` key |
| `Result[Int, Str]?` | rejected for an `Int` key | rejected for an `Int` key |

The last two rows are rejected because any `Int` key can fail UInt validation,
and a Schema error cannot satisfy the declared `Str` error payload. A separately
validated `UInt` key preserves a stored `Result[Int, Str]` error as data.

Pending registered methods constrain their result cell when every viable
candidate has the same closed concrete return type. Receiver selection remains
pending, and no selected call evidence is published until a unique operation
is established.

Finite registered method families retain their receiver, source-ordered
arguments, parameter slots and result in the checked scheme. Each physical
method site identifies its owned requirement; call evidence either selects a
canonical registry member or forwards a caller-owned requirement. Path parent
and normalization retain a separate result binder, solved from the selected
signature after the actual receiver is known. Method dependencies share the
journaled application fixed point and the binder-evidence reachability graph;
consumer expectations do not supply actual result types. An authored Path
return can accept a direct method result binder only when every canonical
member returns Path or RelPath. The same-owner header and returned producer
remain independently verified, including a Proc's Result success payload.
A pending method
result passed to a known formal retains its exact assignability edge until
invocation evidence discharges it. Cold
verification maps source arguments through those slots to the actual physical
operands. Execution replays the selected identity using the existing invocation
substitution and host operation path. It performs no runtime type solving or
value-based overload selection. Lowering evaluates arguments in written order
before arranging their temporary reads in parameter order.

Checked native `Path` host methods lower to `ModuleCall` with the receiver in
the first argument slot. The selected registry method must match the source
name, checked receiver, ordered operands, and result. The executable operation
catalog and canonical native contracts authenticate that physical ABI. Ordinary
method rows forward only when every registered overload shares one executable
native operation and host contract. The native argument handlers preserve the
checked operand layout and defaults. Pure Path transforms retain method rows.
Path line streams keep the existing reader, resource registration, and
cancellation path.

Nullable scheme types use the ordinary boxed physical representation while
retaining their explicit Optional contract and parameter validation. Guarded
postfix lowering evaluates the receiver once and places operand elaboration in
the present arm. The verifier derives null exclusion from actual preceding
patterns and permits a proved payload read to retain a checked Optional type.
Selected native signatures determine the present operation's result before
nullable lifting; an already nullable result keeps its existing envelope.
Result guards prove the success producer and its error relation to the owning
Result boundary independently. Unsigned key requirements retain their existing
checked propagation path.

Branch checking keeps a provisional candidate separate from a required
context. Null supplies no contextual type to the opposite producer; the
checked branch types determine the nullable envelope. The null-envelope rule
is shared with return-shape joining without sharing recursive container or
error joins. Checked value-producing statement tails retain their joined type
in the same generalized tail facts as binding reads. Preparation uses that
type and independently verifies every actual arm. Closed nullable returns
serialize as ground types; unresolved nullable scheme returns use the existing
erased return boundary so a Result payload stays data without runtime type
inspection.
Generic lowering and cold verification share `T` and `null` assignability to
`Optional[T]` with the payload's exact binder identity.

`execution::evaluator::checked_body_contracts` consumes one check, captures its
semantic selections, and prepares the program once without running initialization.
It extracts actual validation rows and parameter and return gates from the verified
program. The opaque result retains checks, schemas, source-role keys, multiplicity,
and their symbol owner, without retaining the AST or prepared program. Comparison
uses the canonical callable instance relation for interface gates; local, creation,
assignment, requirement, and default checks remain exact. Prepared schemas compare
structurally, including conversion order, nominal names, wire mappings, and error
names. Preparation failures retain their original diagnostic.

Inferred return schemas come from checked constructor, requirement, callable,
and immutable binding provenance. Scoped bindings retain contextual schema
expectations separately from the schema proven by their initializer. Checked
expression and tail origins are rebuilt in probes, then carried with the
existing return candidates. Completing branches and early returns must agree
on schema identity; equal record shapes do not supply that identity. The
ordinary proc return envelope places a plain constructor's schema under
Success. Mutable reads without an authenticated origin do not establish a
return schema.

The checker publishes successful binary selections at their executable source
identities. Comparison-chain pairs have canonical ordinal paths and use the
preceding actual right operand as the next left domain. Constant preparation
publishes its own successful fold domains, including the raw result before a
storage annotation applies. Body comparison consumes these facts without
folding or resolving operations. It compares operation identity and all operand
and result domains, specializing with the same retained callable-instance
evidence used for the interface. Unchanged schemes use the existing bound-type
equivalence anchored to their declaration signatures. Missing or unmapped
published selections fail capture.

Private embedded-module bridges carry their native callable contract in the
stdlib catalog. Prepared bridge declarations must match that contract and remain
inside their declared module. The decoder derives bridge results from this
trusted host contract. Public native overloads use registry contracts, including
argument types when one operation serves overloads with different results.
Prepared CLI descriptor plans supply the shape the same executable plan returns.
Repeated calls share the plan's derived values and full-result shapes, retaining
that shape's symbol owner for later materialization. Open typed callable parameters retain their declared contract; their argument and
result proofs do not require a caller to supply a concrete function identity.

Closed Result constructor contracts use the existing ground producer proof for
payloads, including contextual collections and lexical writers. Verification
derives producer demand from selected operations, constructor facts, and mutable
storage reads. Its owner indexes and loop analysis share that transient demand.
Context proofs preserve body values, completing tail-less Unit bodies, and
return transfers; noncompleting blocks do not acquire a Unit result. Process
capture proofs share the checker's RunKind result shapes;
a pipeline takes its capture shape from the head segment.

The same bounded producer census authenticates control predicates even when a
body has no generic calls or Result constructors. Source roles distinguish
Bool-or-Status control from strict Bool guards; scalar Bool slots independently
prove their truth-converting storage. Membership shares the checker’s operand
contract: list and set membership compare values, while map keys retain their
checked domains. Scalar literal evidence borrows pooled text and bytes instead
of allocating copies during verification. Scalar producer slots project their
canonical lexical binding; native result projection uses argument shape only
when all compatible signatures agree, while supplied operands are verified
independently.

Quantified destructuring and loop targets use the same projected-target visitor
as ground declarations. Verification proves the source before publishing every
binding with its actual field type and declaration mutability. Loop bindings
live in a child lexical frame. The iterable's actual item type determines those
bindings, including the success payload of one Result envelope. Result iterable
adaptation uses the ordinary lowering decision, while stream error provenance
uses the existing bounded producer walk and the enclosing checked result
contract. All source producers and loop bodies remain mandatory proof roots.

Explicit discard bindings consume values without propagating Result errors.
Static storage annotations use checked-value validation; a failed annotation is
a fatal type error. Explicit schema requirements retain their Result transport.
Module storage checks include the selected callable export contracts.

Monomorphic driver and capture reads use one storage proof per binding and
contract. Typed mutable callees use this authenticated storage contract without
requiring one immutable function identity. The proof anchors the contract in the defining writer, then checks replacement and
partial writers across driver steps and functions. Destructuring projects the
binding's contract from the actual source and target. Driver aliases and unused
destructuring outputs authenticate their defining binding identities even without
optional producer contexts. Optional aggregate payloads retain every
field and element proof; their raw shape cannot establish a validated domain.
Direct slot/null comparisons establish an immutable pattern read in an active
`if` branch or `while` body, or after a matching `if` branch that cannot complete,
only when the exact regional binding has no writers. Null exclusion preserves
the payload's unsigned and validated domains. Closed record presence proofs retain
opaque fields at their exact Any type while proving typed fields independently;
a null guard cannot refine an opaque field into a typed domain. Typed optional
carriers use the
exact read site's active branch, short-circuit prefix, and preceding terminating
branch while retaining the original broad storage contract and every writer
proof. Completed structural ownership and return analysis populate the owner census
before callable fact validation. Both that validation and the producer proof borrow
its same charged ordered-prefix query. Completed lexical binding queries belong to
that transient census and retain the executable owner, physical read, and slot.
Census and producer proofs reuse successful exact-site binding identities while
writer enumeration and presence refinements keep their independent proofs. Each
query and retained identity remains charged to the unchanged work bound.
Physical optional carriers require the
presence proof even when their checked-expression facts are absent. Completed
presence evidence belongs to one physical gate in that owner. Reuse rechecks the
schema, source child, type identity, and exact optional payload; failed queries
never supply evidence. Source, storage, and callee proofs remain independent.
Checker null-comparison facts distinguish known replacement types from symbolic
non-null source variables. Symbolic facts retain their binding identity, path,
mutation revision, source read, and comparison through Boolean proof composition.
They establish a private guard-local view without choosing an Optional input.
Callable requirements partition every symbolic type, row, and effect parameter
between the root and one guard scope. A scope can reference the root and its
ancestors; sibling and forward references are invalid. Body facts retain that
lexical scope. Existing root binding and unused-parameter checks remain total.
The wrapper worklist and transaction journal also own guarded read relationships.
A closed source selects its identity type, its exact Optional payload, or Null
absence. Only live scopes allocate local requirement arguments. Null carries no
local payload, row, or effect arguments, and unresolved sources remain pending.
Reads of other ancestor bindings use local identity relationships; sharing a type
variable never transfers another value's non-null proof.
Full retains one callable-local binder census with disjoint root and guard ranges.
Type-graph verification records each node's deepest required guard, so cached
nodes cannot escape through root or sibling references. Existing body fact rows
record guard membership. Cold guard metadata identifies the physical source,
comparison, and identity reads. Each live call packet owns only its local dense
arguments and refers to its authenticated ancestor packet. Null supplies no
local arguments. Root argument evidence remains independently total.
Prepared packets retain the callable identity as well as their guard identity.
Runtime consumers select the verified local packet from the physical instruction's
guard membership; they do not infer bindings during execution. Closed record input
evidence preserves opaque fields in row data and proves every required typed field
independently. Static row binding still rejects opaque substitution domains.
Callable contract comparison preserves each guard's source, view, local domains,
and nested requirements under a separate ancestor-visible binder map. Physical
source positions do not determine contract equivalence. A target correspondence
that carries only root binders rejects distinct guarded packs.
Generic calls retain residual guarded requirements under a distinct call ordinal.
Each residual identifies the actual call or materialization, its checked callee,
and that callee's physical guard or residual call. It reuses the same call's root
substitution; callee-local binders never enter the caller's binder census. Cold
verification follows this bounded chain to classify the actual source and prove
the complete live pack or exact Null absence. Prepared residual packets retain
the caller, lexical scope, call ordinal, and physical source alongside the final
callee and guard. Forwarding checks both origins before transferring a packet.
Pending source relationships rediscover canonical type, row, and effect watchers
through the existing wrapper journal after aliases resolve.
Prefix checks include nested condition writes and calls that can mutate
shared captured storage. Immediate shared reads, pure calls, and independent local
copies retain their checked behavior. Capture mutability must agree with the
binding's canonical defining driver payload.
Immutable record destructuring copies prove each field's presence at the source
read before retaining its payload type. Later source writes do not change the copy.
Snapshot reads retain condition and loop evidence while omitting mutation scans
only when the binding has no shared storage or applicable writers.
Every destructuring source and guard envelope is proved, including targets that
bind no values. An optional guard refines the original binding only in
its successful continuation. Its evaluated source, value-block tail, and immutable
temporary initializers prove the same visible binding; the handler retains the
original scope. A mutable local also qualifies when the existing owner and
lexical writer census proves its declaration is its only writer and it has no
shared storage. Captures and written locals receive no snapshot refinement.
This refinement does not change mutable storage contracts.
Generalized record reads in an optional-binding conditional prove every removed
optional field through the same owner, lexical binding, and guard-prefix authority
as ordinary record reads. The physical null-rejecting match and its evaluated
value-block tail identify the guarded field; immutable temporary aliases retain
their exact lexical writer. Closed generic header records supply their actual
type only for the same owner and binding-path prefix. Optional payloads must
match exactly, and all nonoptional fields retain their original types. Mutable
sources retain the ordinary write and capture checks between the condition and
the read. A temporary alias of a mutable source also proves that the source
path is preserved between its initializer and the condition read within the
same statement block; aliases across blocks require a protected snapshot.
The complete condition body still executes and is verified.
Optional guard continuations use that same guard ledger with a distinct presence
role. The actual guard must unwrap an optional and its failure handler must not
complete. The evaluated source and successful continuation preserve the same
binding path; handler reads keep their original scope. Result guards and Boolean
type tests gain no optional-presence authority from this role.
Fresh list and map arguments receive context only from selected
call parameters or physical checked-record fields. Their binding carriers retain
exact owner, source, pattern, and slot identities. Every proposing use must agree
on one concrete source contract. Stored collection contracts remain invariant.
Fresh `NonEmpty` list literals prove a written element or an independently proved
`NonEmpty` splice. Every element and splice retains its item contract proof.
A validated comprehension retains its source validation only through the canonical
single-`for`, unfiltered mapping rule. The actual iterable, target, and body retain
their producer proofs.
Conditional producers derive their contract from independently typed completing
arms, then prove every condition and arm under that contract.
Actual null arms can lift an independently derived concrete arm to an optional
contract. Actual record arms can share an erased record contract while retaining
every field proof. Checked destinations cannot supply either contract.
Initializer-anchored scalar and self-slice storage proves every replacement writer
under the original initializer contract. A null initializer supplies no such anchor.
Direct error propagation from a physical `Err` proves its error payload and cause
without inventing a success contract. Every direct transfer has a mandatory
producer proof, including transfers with no completing return. Uncaught transfers
must fit the function's declared error boundary.
Mutable capture headers require exact equality with the independently derived
storage declaration. Ordinary reads retain their compatibility relation. The
same storage obligation cache distinguishes both relations, and every writer
preserves the declaration type. Producers refresh their identity-bound captures
before every pull and before cleanup of a started body. Unstarted cancellation
drops the frame without executing defaults, body, or defers.

Presence queries retain the exact binding and field path through ordered guards.
They check later fallthrough conditions, completing branches, intervening writes,
and earlier expression operands in execution order. Immutable snapshot aliases
can name an actual null predicate without authorizing shared mutable roots.
Composite predicates retain that snapshot authority. Mutable paths require the
same ordered preservation checks inside each condition.
Completed positive predicates belong to one owner, block, guard branch, binding,
path, and outcome. Reuse retains guard selection, source, writer, and payload proofs.

Every physical record entry retains its producer proof, including overwritten
fields and spreads. Checked storage proves the actual input through intermediate
wrappers before applying runtime validation. Matching record fields can defer
nonnominal predicates to that gate without changing stored collection variance.
Typed callable signatures and nominal payloads retain independent producer proof.
The exact optional-payload carrier preserves the original optional producer for
those incomplete runtime checks. Closed storage-backed parameter and field reads
retain the stored declaration type when flow checking removes only optional
layers. Closed record reads containing typed callables retain their stored
optional type. Ordinary record reads retain their presence carriers. Field
projection proves that an optional receiver is non-null before reading its payload.
A value consumer that needs a stronger record proves each removed optional layer
at the actual binding path, including intervening writes.
Other exact optional-payload reads retain their checked gate, including callable,
nominal, and validated payloads.
Checked expression facts and producer contexts use the same physical read contract.
Guard proofs authenticate each carrier's original binding and field path. Matching fresh collection and comprehension
inputs retain all element, spread, iterable, filter, and body proofs under the
actual checked gate. Every comprehension target proves its record nodes and
selected fields from the actual iterable item, including empty and unused targets.
A comprehension can derive an unannotated local list from its lexical empty-list
declaration and complete append-writer census. Every nonempty append must
independently produce the same closed list type. The proof charges traversal of
that source type and retains every writer obligation. The loop variable uses the
actual qualifier target and source item. Checked slot facts do not establish that
relationship. A fresh list receiver can obtain a set element contract only
when the selected registered method returns exactly the gate's set contract.
Stored collection contracts and nominal validation remain independent.
Generic shared-state restrictions remain separate.
Checked storage authenticates a generated collect accumulator through its fresh
scoped empty-list initializer, inner value block, and matching final slot read.
Every writer must append an independently proved list inside that block; empty
outputs use the vacuous empty-list rule. Runtime predicates remain at the checked
gate, and the inner body's return and effect proofs remain mandatory.
Generalized collect expressions use that same accumulator and lexical writer
contract. The checker retains the nearest collect target across ordinary blocks
and resets it at callable boundaries. A no-result `CollectItem` predicate
checks each inferred single-item substitution; delegated yields retain their
invariant list relationship. Cold verification authenticates the physical
accumulator, writers, items, and owners, including any present predicate. A
single-item yield and delegation of a singleton list can have identical physical
rows, so those rows do not independently certify which source form produced them.
A contextual unsigned collector retains the actual raw writer type until its
whole-list guard runs after inner cleanup. Generic proof requests drain through
the same fresh accumulator proof as ordinary checked storage, including for an
uncalled owner without captures; ordinary unsigned producer demand is unchanged.

The producer census collects pattern and pipeline instructions during its
mandatory instruction scan. Later scope registration visits those candidates
under the same work bound.
Owners without callback or unreachable roots can collect writers and returns from
the complete physical statement index. The index must match the independently
counted statements, with finished ownership, valid ranges, and unique positions.
Other owners retain the full flow traversal. Writer census lookup reuses a completed inherited binding
for an owner-local block and slot only after checking region overrides and earlier
local declarations at the actual writer. Read and presence queries retain their
lexical lookup. The owned operand walk retains each instruction's enclosing
statement, and the charged census copies that metadata beside its parent links.
Owners without expression-region bindings can skip expression-only ancestry.
Owners with region bindings retain the physical parent walk. Exact slot overrides
and ordered declaration lookup still establish visibility. This evidence contains
no type or refinement.

Generic body facts distinguish typed expressions from nonmaterial constructor producers.
Empty lists, empty maps, and closed record wrappers retain their actual physical
instructions and ordered field writers. The cold verifier reconstructs their
producer shapes; an unresolved empty payload must be discarded or overwritten
before the shape reaches typed storage, a call, projection, or return. Constructor
facts carry no semantic type or scheme binder.

Guard queries borrow the exact owner census and visit
only relevant ordered prefix subtrees. Pattern and target write membership streams
through the same borrowed visitors as write collection, charging before allocation
and stopping once an overlapping writer is established. Selected native arguments,
record entries, and match subjects can carry an owned, single-use declared-type
fact into their existing proof obligation. The fact
must match the exact owner and instruction. Contextual hints cannot supply it,
and consuming the fact retains all child, shape, storage, and effect proofs.
Actual parameter derivation can retain its resolved binding in the same fact.
Consumption checks the physical slot before reusing that binding for the original
all-writer proof. Checked-type facts cannot supply this binding witness.

Quantified proc effects must be finite and include each selected callee's
checked effect contract. `FullVerifier` derives ground owner requirements from
physical operations and selected native contracts. Operations with different
overload effects require proof of their actual operands. For dynamic calls through
module fields, `FullVerifier` proves arguments, results, and effects against the
actual module's required export contract. Unknown callee contracts
cannot satisfy finite effect bounds. Callable kind must match the prepared entry
and return boundary. Pure body restrictions remain independent of capability
sets, and captured errors remove only the Error requirement within their actual
runtime boundary.

`CallableType.effects` uses the opaque `CallableEffects` contract. Unknown,
known empty, and symbolic effects remain distinct. Concrete projection rejects
unresolved symbolic terms. Effect expressions retain raw capability identity,
including `io`, while permission and exclusion checks share one implication
relation. `TypeConstraints` owns effect cells, predicates, dependency queues,
and callable-application requirements. Their fresh identities, bindings, levels,
watchers, and queue changes use the same rollback journal as type and row cells.
Application requirements refer to the actual callable type variable and use
the canonical argument binder, preserving authored labels, defaults, and rest
layout through aliases and returned values.
`CallableScheme` owns the bound effect scope and application requirements.
Generalization includes obligation-only result binders before freezing the body
scope. Instantiation allocates one type, row, and effect substitution and retains
the selected argument layouts. An unresolved nested invocation instead retains
a reference to its actual caller-owned application requirement. Ground invocation
evidence still requires a selected layout. Physical verification must compare the
substituted application relationship against that caller requirement independently
of constructor range checks. Declaration-owned requirements remain distinct
from nongeneralizing invocations. Strict `process.command_argv` word requirements
use the same conversion requirements for inference, substitution, and cold body
proof; argv retains an invariant list item binder. Environment overlay entries
use the distinct `Env` conversion, with the same scalar and exact `List[Path]`
domain as concrete environment values. Its bit occupies the existing conversion
set and uses the same journal, generalization and call substitution. The Full
scope consumer proves every owned record field or string-keyed map value against
that conversion; the ordinary expression proof independently authenticates the
actual keys, values and binders. Other context-scope inputs retain their closed
type checks. Nested committed transactions
and speculative trials share the same journal, including fresh scope ownership
and panic rollback.
Direct source applications retain one declaration-and-expression identity across
constraint probes. Instantiated requirements remain separate activations.
A deferred Proc return envelope refers to an explicit application identity and
uses the existing ground Result/error join policy; type-cell equality alone
cannot establish application-result provenance.

Stream return origins specialize a selected callback through its actual
application identity and settled argument layout. Unique formal type identities
locate the supplied argument origins; known callable origins select the target
summary and preserve its exact projections. Equal callable signatures do not
identify a target. Imported normalized application ordinals remain independent
of local pending declarations. Explicitly typed arguments retain their original
expression origins; imported bare formal arguments use the exact resolved
binding and declaring body. Ambiguous or foreign identities fail closed. A
checker-local derived source index shares immutable maps across probes and is
invalidated when resolved bindings change. Mixed or unsupported origins retain
their unresolved relationship. Settled rest layouts prefix individual item
origins once and retain the element projection of spliced lists. Bare splice
arguments use their exact AST operand binding. Omitted arguments use the exact
selected target's checked default origins; missing or unresolved default
relationships leave the application unresolved. Default evaluation stays in its
existing runtime position.

Checked declaration effects use `CheckedEffects` and `CheckedEffectScope`.
`FunctionEffectFact.effective`, `FunctionEffectFact.required`, and the callable
effect projection retain these contracts. Concrete projection returns an unknown
contract separately from a finite set and rejects unresolved bound expressions.
Bound effect ordinals belong to one declaring callable. Graph edges substitute
the actual call's effect arguments before joining contributions from another
owner. Defaults belong to their declaring callable through the actual parameter
expression spans. A semantic body comparison uses the same retained callable
instance as its header comparison. Raw effect-term equality proves only equality
within one owner scope.

A defaulted parameter of a ground callable keeps its actual signature as an
argument producer contract. The canonical default flag and lexical header role
select this proof; the same producer proof checks every incoming argument,
default initializer and subsequent writer before accepting the parameter read.

A ground invocation validates its complete immutable binder substitution once
for its argument group. A private validated token reuses the canonical bound-type
instantiator for each argument; conversion, MapKey and iteration checks still
cover every binder, including binders absent from that argument's type. Every
argument retains its independent physical producer and binder evidence checks.
The shared materialization budget also bounds each instantiated argument type.

Frontend inference-work metrics count generalization and instantiation type
function entries, schema argument slots visited by mapping, scoped constructor
generalizations, and constructor template binder normalization iterations.
`generalization_binding_clones` counts entries copied from scope maps and
parameter vectors, including nested row-lacks sets. These counters are active
only in measurement and test mode; they measure neither CPU time nor allocations.
Output type and evidence sizes retain their independent emitted-fact census.
Artifacts without the new metric fields are unavailable for this measurement.

Pending recursive invocation permissions constrain the actual shared effect
cells in their declaration's inference scope before generalization. An invocation
rejected during argument checking retains its owned permission check without
publishing a call instantiation, return flow, or Stream flow. Deferred validation
does not repeat a permission check that already reported its failure.

Indexed schemes retain owned effect binders, expressions, predicates, and call
arguments alongside their type and row substitutions. Preparation authenticates
one scope per scheme and reuses it for physical effect and producer verification.
Concrete projection distinguishes unknown contracts from unresolved expressions;
scoped substitution and retained predicates prove symbolic permission checks.
Capture removes only outward Error inside its actual lexical boundary.

Stored generic callable values retain a finite set of actual function targets
and one complete checked contract. Relationship identities belong to their
semantic table. Full IR maps each target's type, row and effect binders to the
canonical contract and authenticates the physical producer's target set.
Aggregate storage retains an owned conversion plan with source and destination
contracts. Cold proof validates actual structural paths, callable relationships,
substitutions and complete evidence ownership. Scoped field producers reuse
the completed body proof without grounding opaque sibling fields. Unchanged
subtrees remain shared; conversion visits only affected branches. Checks that
need invocation bindings use scheduled execution on both evaluator routes.
Body facts use cold-validated instruction/kind order for bounded lookup.
Scoped membership requirements retain both actual operand contracts. Cold
verification binds each physical membership to its own selected key check and
operation ordinal, then proves its operands independently. The check occupies
extra payload without growing hot function or parameter rows. Execution reads
only the outer container domain and map key from the same immutable invocation
bindings; it does not clone unrelated record fields. Unsigned keys use the
ordinary Require/Try path after computing the needle and before evaluating the
container. Shared key-validation requirements preserve conditional Error across
Index and Membership captures, forwarding and Proc completion.
Invocation and value materialization retain distinct checked evidence, including
when a factory expression needs both at the same source span. Both execution
routes use the same checked materialization operation. Dynamic value calls have
their own effect authority; direct calls retain their physical target checks.
Imported signatures retain their immutable relationship snapshot. Each checker
rebases imported handles into its own table, and final module publication maps
all checked outputs into one fresh owner. Type, scheme, schema, invocation and
value evidence move together. Cached source snapshots keep their original identities.
Retained-byte accounting includes the relationship pool and counts shared
allocations once within that pool.

Prepared applications retain their actual callee, ordered argument domains,
result, kind, effects, argument layout, and Proc return relationships. A selected
layout can still have pending argument constraints. The solver revisits those
constraints after type or row changes and resolves composite actual arguments
before constraining them. Publication forwards pending relationships through
the caller's existing requirement ordinal. Settled applications retain selected
layouts and must match the actual callable contract. A forwarded application
must match the caller-owned requirement under the same substitution, including
its ordinal and all domains. Argument producers establish binder evidence first.
For a structural callable, an independently supported actual argument can also
establish its matching parameter binders through the canonical argument layout.
The existing argument-evidence rules exclude Null, Any and missing defaults
when they do not establish a payload type.
An application can then establish result binders only after its callee and input
binders are established; cyclic requirements cannot establish themselves.
Fresh empty collection arguments receive a transient closed-domain obligation
from that proof. The existing producer verifier checks the exact physical call
operand, literal family, aliases, and writers before accepting the program.
Capture bodies use ordinary lazy block lowering and independently verify actual
success and error producers after frontend disposal.
Named callable values materialized inside generic bodies retain their actual
target and expression type. Quantified targets require their owned fresh value
instantiation and complete substituted contract. A ground target instead proves
its exact kind and signature against its selected declaration; it has no invented
instantiation. Inferred immutable local aliases use the same materialization
authority. Pipeline descriptors retain the actual callable value and environment,
using a scheme value call or a checked ground signature as appropriate. Their
synthetic invocation evidence remains separate from the source value selection.
Ground and driver Capture/Retry instructions retain required checked Result
contracts, including explicit Any payloads. Scheme instructions use their owned
body facts and prepared substitution. Cold verification joins the actual error
contributions; an empty contribution set does not constrain a declared phantom
error type. Execution normalizes propagated payloads only when the checked outer
error type is Error, preserving nested Result data and existing error identity.

Finite wrapper requirements retain guarded fields, fallbacks, handlers and
captures. Calls select a checked Optional or Result mode, or forward the exact
caller-owned requirement; execution obtains that mode from the existing type
substitution. Cold proof authenticates actual physical operands and ordered
capture contributions. Dynamic projected binders require evidence from their
exact source projection. Closed fallback envelopes use their actual source
type without a generalized wrapper requirement; Optional fallbacks match null
without unwrapping a Result payload, and Result fallbacks project one layer.
Result constructor arguments derive their selected
payload from actual producers, using owned context only for the absent branch;
the ordinary producer proof also checks aliases and causes. Constructor
recursion observes the existing argument-depth bound before descending.

Generalized bodies retain their actual callable kind for cleanup permission.
An unresolved propagation operand acquires a Result shape through the existing
transactional constraint solver, with fresh children at the operand's level.
The checker, application solver and cold verifier share the inferred Proc
envelope rule: nonempty propagated errors determine the error type; an empty
set retains the default Error. Execution uses the prepared error substitution.
Proc cleanup uses the ordinary deferred-action rows and failure policy; inferred
return envelopes include errors from actual statement and cleanup propagation.
Generic assertions retain lazy message operands. Cold proof checks their Boolean
condition and optional Str message; the separate effect pass requires Error.
Checked `error.fail` calls lower to the existing failure constructor, whose
message producer and Result[Unit, Error] contract are independently verified.
A quantified Proc with an explicit plain or Any return can propagate through
its checked direct Error permission. Cold proof binds that boundary to the
actual owner and scheme, and independently proves the Result operand. Other
callback effect terms retain their separate permission checks. Captured direct
generic calls use the instruction's checked invocation substitution to classify
error flow; Ground calls retain their verified signature authority, including
embedded script calls without an auxiliary target row.
A terminal Proc command completes with Unit through the ordinary Result envelope.
With initializers see only preceding success bindings. Its handler sees the
outer scope and its failure binding. A Guard handler must diverge before success
bindings become visible. The body and producer verifiers preserve these same
regions. Argument queries sample actual checked guards and closed conditional
arms; incomplete runtime guards still require independent proof of their inputs.
A closed List type from an actual arm can supply another arm's missing producer
context, including erased record items. The producer proof still authenticates
every arm, condition, and writer.
A plain Result consumer can check a fresh `Err(...).context(...)` through its
verified argument bindings. Its proof authenticates the registered
method, error payload, cause, and context arguments; it publishes no success
type for the absent payload.

Generalized captured runs, retry attempts, error contexts, and restored context
scopes use the existing command and scoped-block executor. Retry preserves an
actual Result body and wraps other completing values; its delays remain outside
the attempt's error boundary. Error contexts preserve the body's value type.
Context scopes wrap the body in Result without flattening nested Results. They
retain the ordinary static escape rule and check symbolic values before resource
transfer and context restoration.

Generalized Proc `tempdir` scopes use the ordinary `MatchExpr` and `ValueBlock`
rows and deferred cleanup. They retain the `fs` requirement and the `error`
requirement for failures that are not lexically captured. Entry failure arms
reconstruct `Err` from the actual filesystem error payload. Generated
expressions and immutable locals carry owned type facts. Body returns and
errors keep their ordinary destinations.

Selected native calls retain their registry identity, argument layout, and
ordinary effect contract in generalized bodies. Native selection does not imply
purity: every owner still proves the operation's actual effects and invocation
permission. Equality reads resolved operands from the same constraint problem
before applying the ordinary compatibility rules; this does not change invariant
storage writes or nominal error identity.

Each invocation retains the checked application layouts selected by its canonical
call substitution. Forwarded entries share the caller's actual layout instead of
building a chain of unresolved applications. Argument evaluation retains source
entry boundaries through splice expansion. After all supplied entries finish,
those layouts place values and actual declaration defaults in formal slots. The
runtime consumes prepared slot indices and preserves actual callee header checks.
Declaration-derived callable signatures retain defaults and rest metadata; typed
call verification checks their layout and every argument against both the callable
contract and its actual target writers.

Proc application returns retain their actual owner, return instruction,
application ordinal, and return-envelope ordinal. The body verifier traces the
actual immutable producers and all completing branch values before accepting
that descriptor. During Proc return inference, terminal statement `if` and `match`
arms and structured `return if` and `return match` arms retain separate raw
completions. Their control nodes have no material type; conditions, guards, and
value leaves retain their checked facts. The checker normalizes those completions
after collecting the body's propagated errors. Lowering emits one return per arm
with its authenticated envelope. Ground defaulted procs use the same arm-value
completion rule when their checked return has non-Unit success; statement-only
Unit bodies retain ordinary fallthrough completion. Bare procedure returns use an actual Unit producer and the same authenticated completion boundary as value returns. Generic fixed-field projections derive their result from the actual closed receiver through the canonical fixed-field relation; records retain their declared field map. When a structured return has no material control
fact, resource transfer requires the checked facts of every completing value leaf.
Missing leaf evidence blocks publication. Values checked against settled return types retain their contextual
facts, schemas, and ordinary joins. Branches used as values keep their raw type join. A shared descriptor must
prove the same raw result and normalized return contract across those branches;
its representative is the minimum actual application ordinal.
Invocation bindings project the result's outer constructor from the same checked
type substitution to select wrapping or preservation. The executor does not
inspect the returned payload to select an envelope, and formal values keep their
ordinary data wrapping.

An inferred Proc completion group retains each returned callable application's
raw result type. A `Result` source contributes its error type to the owner join.
The cold body verifier ties that source to the application's result row at the
owning return. The deferred application envelope receives the owner join and
supplies the Proc return directly, so lowering does not wrap it again.

Proc completion normalizes a non-Error outer failure only when the checked
return contract requires `Error`. Ground functions derive this from their
checked return and signature. Scheme functions retain a defaulted-Proc flag
in existing metadata and consult the actual prepared invocation substitution,
including application-result parameters. Cold verification authenticates the
flag against the scheme and limits callback error evidence to actual owned
return sites. Normalization preserves existing structured errors and updates
only a matching pending traceback's error identity, retaining its source span
and frames. Nested Result values remain data.

Checked callable contracts and aliases retain the canonical `CallableKind`.
The Rust `TypedCallable.kind` field replaces its former purity flag.
`Type::Function(kind)` represents an opaque callable and replaces `Type::Pure`
and `Type::Proc`. `Type::Stream(item)` continues to represent a produced stream.
The semantic codec retains the kind for opaque and typed callables and rejects
invalid kind tags. Authored callable annotations retain the existing Pure and
Proc syntax. Stream callable contracts have no authored annotation syntax.

The public Rust module export contract uses
`ModuleExportType::Callable { kind, sig, optional }` for ground Pure, Proc, and
Stream declarations. This replaces the separate `Pure` and `Proc` export
variants. `GenericCallable` identifies contracts with type, row, or effect
binders. Callable-valued exports retain their actual lexical binding proof.
The compact export codec records the canonical kind and rejects retired tags,
reserved bits, foreign owners, and missing signatures. Runtime module contracts
use retained executable signatures or checked load-time signatures. A callable
kind alone does not establish a Stream signature.

A generic body may borrow a root-owned immutable static import namespace to
read a nonoptional callable export. Ground declarations and callable-valued
exports retain their registered signatures. Generic declarations retain their
owned callable relationship and fresh invocation evidence. The namespace keeps
its original root import lifetime and cannot escape as a value. Cold body
verification checks the physical field read, immutable writer, and capture
binding. Producer verification follows the import's actual export storage or
child-owned declaration and matches the selected target. The selected target
cannot capture mutable or resource state. Resource-valued module exports remain
rejected. Unselected callable exports retain their existing root resources and
shared state. Calls preserve the real callable environment, defaults, kind,
and canonical invocation effects.

The public Rust `Value::Callable` variant replaces `Value::Pure` and `Value::Proc`.
Its opaque `CallableValue` carries the canonical `CallableKind`; named values
require an explicit kind, and prepared values derive it from the authenticated
executable target. Declaration identity remains available through `identity()`.
Equality compares kind and identity, excluding invocation evidence.
The runtime uses `CallableKind` in place of its former `LoweredFunctionKind`.
Prepared invocation context belongs to the callable's executable program and
remains private. Function references and typed calls encode the three kinds
strictly; a stream factory is distinct from its produced stream value.
A lazy producer owns the same checked invocation bindings at creation, first
pull, default evaluation, suspension, and cleanup. Declared creation effects
remain charged at the call site; executing defaults and the body waits for the
first pull. Extracting a declaration name provides no
substitute for checking the actual target and its call-site substitution.
Each contextual reference or immutable alias read retains its own checked
substitution; materializing one use does not specialize the stored declaration
for later clients. Materializations prove their actual producer and signature
without counting as invocations. Only an authenticated Pure target projects its
top-level invocation effects to empty; nested callable contracts retain their
own effects, labels, schemas, defaults, and rest layout.

Prepared invocation bindings retain the complete closed type, row, and effect
substitution using the same bounded decoder as cold verification. Opaque host
calls validate supplied values against those instantiated parameter contracts;
nested callable checks use the actual executable target and the canonical
callable compatibility rule. Cache lookup follows this validation. Cache keys
retain the executable program, target, normalized substitution, checked layouts,
and argument encoding. The retained program prevents address reuse from merging
unrelated executions, while equal substitutions share entries across call sites.
These runtime-owned bindings do not add static frontend evidence nodes.

Stream item evidence retains the owning program, actual script declaration or
checked factory instruction, and the same closed invocation substitution. It
survives storage, aggregates, and clones without pulling the producer. Ground
script headers are checked against actual yields and delegated item producers
through the existing producer proof. UInt item boundaries retain their runtime
check at first pull; a positive signed literal alone cannot certify a UInt
producer contract. Native factory facts must agree with the actual selected
operation's solved result before they can supply stream item evidence.

Bare Range uses the native owned-values Stream carrier after eager checked
allocation. The same range materializer constructs runtime values directly for
bare Range and lowered values for the existing pipeline path. Range and captured
Stream-result facts activate the existing producer proof for actual bounds,
writers, success values, and captured errors. Callable declaration kind decides
whether a call suspends a producer; a Pure function returning a Stream executes
its body immediately.

Environment path reads retain owned snapshot entries with sealed provenance.
The public Rust `Value::EnvPathList` variant carries an opaque
`EnvPathListSnapshot` instead of a unit marker. Runtime-to-lowered conversion and
clones retain that provenance; an ordinary path list cannot supply an internal
checked view contract. The native zero-argument operation returns this checked
view, while the one-argument `env.path_list` operation returns its ordinary
`Result[List[Path]]` contract. Actual arity selects the native contract.

Language type tests and `.require` observe snapshot entries as material list
data, independently of internal view authentication. Successful list validation
materializes an ordinary list, and validated list consumers use the same
one-way projection. Membership and equality read captured entries; environment
mutation methods evaluate their receiver and change the active environment.
Snapshot values remain runtime-owned and cannot enter the prepared constant pool.

`FullVerifier` proves spawn and wait result envelopes from actual handle and
command operands and checks command options against canonical registry contracts.

Indexed IDs refer to program-owned tables. `SymbolOwner` owns dynamic names.
The installed program retains its source map, but not the CST, arena program,
or checker output.

`src/stdlib.rs` embeds script-backed standard modules at build time, so an
installed XSH does not read a stdlib tree at runtime. The loader selects modules
required by source use and sends them through ordinary checking and lowering.
The registry can also bind an entry directly to a native runtime operation.
Loaded modules link script-backed calls to the loader's prepared implementations.
Verification resolves each external callee's reserved module identity and member
against the registry; all matching bindings must agree on purity and effects
before that contract can justify a finite caller's permissions.
Reserved implementation declarations must satisfy that same contract before
they can execute locally or be published to another prepared program.

## Invariants

- Verify the indexed program before installation or execution. Malformed or
  unsupported data fails preparation.
- Make each semantic decision once. Routes that must agree share an
  implementation or have parity coverage. Lowering consumes checker facts.
- Preserve order visible to XSH programs, error kinds and spans, cleanup,
  status, and trace events through lowering and runtime routes.
- Give each construct one core meaning. A sugar form has one core expansion.
- Keep host effects at their boundary. The evaluator interprets structured
  host outcomes as XSH control flow.

## Adding a language feature

1. Define observable behavior in the SPEC template and its snippets. Follow
   [`TESTING.md`](TESTING.md) to regenerate derived documentation.
2. Add syntax in `src/syntax/`. Preserve the CST, update the exhaustive
   `AstArena` child enumerator, and update traversals that depend on binding
   order, callable edges, command boundaries, or scope transitions.
3. Add checker behavior in `src/sema/check/` and publish the facts later stages
   need.
4. Add indexed rows in `src/runtime/eval/indexed/full/schema.rs` when the
   feature needs a new representation. Implement lowering, verifier checks,
   and evaluator behavior.
5. Update `crates/xsht/` tools that handle the new form. Add focused language
   and verifier coverage, then run the narrow gates in [`TESTING.md`](TESTING.md).

## Adding a sugar form

A form is sugar only when existing core forms provide its complete meaning,
including evaluation order, effects, and failures. The parser retains the
written operands and a core expansion. Semantic passes use the expansion;
source-aware tools use the operands. A form that binds in the enclosing scope
or owns resource cleanup stays core. See [`DESIGN.md`](DESIGN.md) for the
syntax rationale.

## Adding a validated type

`src/sema/validated.rs` owns validation identity and base-type rules.
Value-based validators such as `IntRange::contains` and `is_rel_path` are
shared by literal, constant, and runtime checks. `src/runtime/eval/validated.rs`
uses the runtime value view. `FullVerifier` checks the indexed type and schema.
The SPEC owns the observable behavior.

## Runtime design

The evaluator owns values, scopes, result propagation, cleanup, resource
handles, and trace parentage. `src/runtime/process.rs` and
`src/runtime/run.rs` own process creation, process groups, terminal handoff,
waits, cancellation, and detached reaping. `src/runtime/signal.rs` records
signals in the handler; the evaluator interprets them at checkpoints. Process
groups are the unit of cancellation.

Linux post-fork setup applies a plan prepared in the parent. Child setup uses
async-signal-safe operations and performs no allocation or locking. Keep XSH
code out of the signal handler and the post-fork child path.

Resources belong to their creating scope until transferred to an enclosing
scope. Scope exit stops owned activities before user defers; handles remain
usable during defers and are released afterward. Explicit `close` and `unlock`
results flow to the script. Automatic cleanup does not replace the script
result with a release error.

Network workers receive plain Rust requests. They do not own evaluator values,
source spans, or trace buffers. The evaluator owns job lifetimes and trace
events.

The serial pipeline pulls one item at a time. Materializing stages and
`par-map` cross into the evaluator's stage engine. Stages resume through
continuations, and yield delegation and cancellation stay iterative so producer
depth does not become native stack depth.

Before adding a host integration, name its owner, checkpoint behavior, cleanup
responsibility, signal interaction, result shape, and trace evidence.
