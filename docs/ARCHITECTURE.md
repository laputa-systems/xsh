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

Driver temporary type metadata supports annotation checks. Generic and callable
contracts require independently authenticated writer evidence. Missing optional
type information does not relax those contracts.

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

Generic comprehensions reuse the ordinary qualifier rows and iterable adaptation.
Every source must establish its actual item type before its immutable target
bindings enter a child lexical frame. Filters use the checked control-condition
role. Later qualifiers and the projection see those bindings; earlier sources
cannot read them. Source, filter, key, and value elaboration stays inside lazy
value blocks. Resource-bearing block results transfer through the existing
cleanup mechanism before iteration begins. Cold verification proves every
source and projection, checks propagated error families, and accepts `NonEmpty`
only for the canonical single-source unfiltered mapping.

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
the payload's unsigned and validated domains. Typed optional carriers use the
exact read site's active branch, short-circuit prefix, and preceding terminating
branch while retaining the original broad storage contract and every writer
proof. Completed structural ownership and return analysis populate the owner census
before callable fact validation. Both that validation and the producer proof borrow
its same charged ordered-prefix query. Physical optional carriers require the
presence proof even when their checked-expression facts are absent. Completed
presence evidence belongs to one physical gate in that owner. Reuse rechecks the
schema, source child, type identity, and exact optional payload; failed queries
never supply evidence. Source, storage, and callee proofs remain independent.
Prefix checks include nested condition writes and calls that can mutate
shared captured storage. Immediate shared reads, pure calls, and independent local
copies retain their checked behavior. Capture mutability must agree with the
binding's canonical defining driver payload.
Every destructuring source and guard envelope is proved, including targets that
bind no values. Fresh list and map arguments receive context only from selected
call parameters or physical checked-record fields. Their binding carriers retain
exact owner, source, pattern, and slot identities. Every proposing use must agree
on one concrete source contract. Stored collection contracts remain invariant.
Conditional producers derive their contract from independently typed completing
arms, then prove every condition and arm under that contract.

Every physical record entry retains its producer proof, including overwritten
fields and spreads. Checked storage proves the actual input through intermediate
wrappers before applying runtime validation. Matching record fields can defer
nonnominal predicates to that gate without changing stored collection variance.
Typed callable signatures and nominal payloads retain independent producer proof.
The exact optional-payload carrier preserves the original optional producer for
those incomplete runtime checks. Matching fresh collection and comprehension
inputs retain all element, spread, iterable, filter, and body proofs under the
actual checked gate. Every comprehension target proves its record nodes and
selected fields from the actual iterable item, including empty and unused targets. A fresh list receiver can obtain a set element contract only
when the selected registered method returns exactly the gate's set contract.
Stored collection contracts and nominal validation remain independent.
Generic shared-state restrictions remain separate.

The producer census collects pattern and pipeline instructions during its
mandatory instruction scan. Later scope registration visits those candidates
under the same work bound. Guard queries borrow the exact owner census and visit
only relevant ordered prefix subtrees. Pattern and target write membership streams
through the same borrowed visitors as write collection, charging before allocation
and stopping once an overlapping writer is established. Selected native arguments,
record entries, and match subjects can carry an owned, single-use declared-type
fact into their existing proof obligation. The fact
must match the exact owner and instruction. Contextual hints cannot supply it,
and consuming the fact retains all child, shape, storage, and effect proofs.

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
from nongeneralizing invocations. Nested committed transactions and speculative
trials share the same journal, including fresh scope ownership and panic rollback.
Direct source applications retain one declaration-and-expression identity across
constraint probes. Instantiated requirements remain separate activations.
A deferred Proc return envelope refers to an explicit application identity and
uses the existing ground Result/error join policy; type-cell equality alone
cannot establish application-result provenance.

Checked declaration effects use `CheckedEffects` and `CheckedEffectScope`.
`FunctionEffectFact.effective`, `FunctionEffectFact.required`, and the callable
effect projection retain these contracts. Concrete projection returns an unknown
contract separately from a finite set and rejects unresolved bound expressions.
Bound effect ordinals belong to one declaring callable. Graph edges substitute
the actual call's effect arguments before joining contributions from another
owner. Defaults belong to their declaring callable through the actual parameter
expression spans. A semantic body comparison uses the same retained callable
instance as its header comparison. Raw effect-term equality proves only equality
within one owner scope. Indexed symbolic effect serialization remains unavailable
until the physical store can retain and verify that scoped evidence.

`FullVerifier` proves spawn and wait result envelopes from actual handle and
command operands and checks command options against canonical registry contracts.

Indexed IDs refer to program-owned tables. `SymbolOwner` owns dynamic names.
The installed program retains its source map, but not the CST, arena program,
or checker output.

`src/stdlib.rs` embeds script-backed standard modules at build time, so an
installed XSH does not read a stdlib tree at runtime. The loader selects modules
required by source use and sends them through ordinary checking and lowering.
The registry can also bind an entry directly to a native runtime operation.

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
