# Architecture

Checked callable aliases retain optional original declaration identities; validated
module contracts supply signatures without inventing effect graph edges or
navigation targets. They are binding metadata, independent of erased
`Type::Pure`/`Type::Proc` values. `Checker::resolve_callable_alias_target`
resolves lexical and module signatures once; `StaticCallableAlias` facts
retain the original definition span and complete `CallableType`.
`CompactDeclOutput::static_callable_aliases` supplies the existing argument
binder with those facts. Indexed calls execute the retained callable handle,
using existing prepared default slots and capture hydration. Dynamic module
exports register an alias handle against the original indexed definition;
they do not create a forwarding body.

`generic/callable_receivers.rs::OriginalCallableReceiver` retains a saved `.call`
receiver independently of supplied arguments. Captured immutable aliases retain
their original checked binding, receiving capture allocation and exact
`UserCallableContract`. A compiler temporary cannot supply local binding authority
or substitute another capture with the same signature.

`runtime::eval::RuntimeCallableValue` carries an owning `Arc<FullProgram>`, a
validated `CallableValueId`, and the original lexical capture environment.
Immutable captures retain values; supported mutable captures retain live cells.
`Value::Callable` and `LoweredValue::Callable` preserve that same
creation through the host bridge and nested containers; neither conversion
replaces it with a function name. Creation validates the prepared program owner,
capture order, storage kinds, and original callable kind. Clones preserve
creation identity, while separate creations remain distinct. Persistent value
encoding refuses these handles. Mutable captures require an authenticated live
cell for the original program, `BindingIdentity`, definition owner and type;
an immutable value snapshot cannot supply that authority.
stream callable activation is not part of its current prepared contract.

`generic/lexical_captures.rs` seals declaration capture allocations independently
of their original reads. `full/lexical_capture_prepare.rs` checks the original
`BindingIdentity`, definition owner, declaration header, scoped type root and
`CapturedBinding` producer provenance. `indexed_run/live_capture_cells.rs`
binds supported ground captures to their original allocation and defining
activation. Nested calls, supplied arguments, defaults and defers share those
cells; reads refresh the receiving slots and assignments publish immediately.
The evaluator's shared environment retains the cell registry. Completion
refreshes slots and retires the receiving activation before slot reuse;
`CompletedLiveCaptureFrame` authenticates publication to external scopes while
local captures keep their defining cells. Resource and callable captures need
their own ownership transport; the plain ground cell protocol does not retain
those values.

Captured scalar and aggregate writes retain the receiving declaration's
`LexicalCaptureId` separately from the defining binding and its real versioned
producer flow. Aggregate writes also authenticate their original path selectors
and checked UInt boundaries. Specialized scalar reads preserve their actual
`IntSlot` or `BoolSlot` encoding in the lexical capture receipt.

Producer suspension detaches the physical slot registration while retaining an
opaque `SuspendedLiveCaptureFrame` for the original cells and receiving frame.
Resume authenticates that frame and refreshes slots before defaults, body work
or cancellation defers. Completion retires the activation before publication or
slot reuse. Empty Driver slot vectors do not register an allocation: nested
imports may share their dangling pointer without sharing any binding cells.

A scoped native method keeps its original pending candidate family in the
definition and forwarding declarations. Each ground instantiation retains a
separate selected witness; unused definitions retain the family without
instances. `generic/native_callables/scoped_methods.rs` validates the original
Str/Bytes `starts_with` family, source ancestry, and declaration frame.
`full/native_callable_prepare/scoped_methods.rs` seals the authored receiver and
argument packets before frontend disposal. Execution selects the stored witness.

`RuntimeNativeCallableValue` retains a separate `NativeCallableValueId` and
owning `Arc<FullProgram>`. `NativeCallableRef` encodes no registry name or
frontend ID; its original checked expression supplies the sealed registry
authority. `generic/native_callables.rs` owns the creation and invocation
receipts, and `full/native_callable_prepare.rs` validates their source, owner,
signature, permissions, admission guards, producer roles, operands, and omission
masks. A declared `Any` formal preserves each original operand descriptor and
its checked eligibility.
Native handles use `Value::NativeCallable` and `LoweredValue::NativeCallable`
through the host bridge and remain distinct from user declarations. Persistent
encoding refuses them. The current prepared execution supports a single
closed native module authority with static argument slots; conditional
families and scoped native invocations require their own prepared protocols.
`indexed_run/native_callable_run.rs` checks the owning program and value receipt
before constructing the formal native argument packet. Saved bindings evaluate
named operands in source order; absent default slots stay absent for the
existing numeric native backend, including its trace and error handling.

Framed user invocation keeps the prepared `InvocationPlanId` through callee and
argument continuations. A typed value must match that plan's owning program and
callable contract before entry by numeric function target. Its retained captures
hydrate the callee slots before defaults execute; the ordinary current-binding
hydration path does not overwrite that environment. Typed values with no prepared
invocation authority are refused. This entry path covers prepared monomorphic
user Pure/Proc callables, independently of conditional/native/stream protocols.

Original immutable local callable `let` bindings retain a `BindingIdentity`
through lowering. `BuildScratch::callable_binding_origins` pairs that identity
with the original statement, emitted `Let` row and slot, actual initializer row
and expression identity, and checked binding root with its lexical scheme.
`SlotScope::callable_binding_authorities` follows lexical declaration and
shadowing; `BuildScratch::callable_binding_uses` links original identifier reads
to the visible definition before expression lowering discards names. Equal
signatures do not identify a creation environment. Missing checked definition
or initializer facts refuse lowering; explicit kind erasure, mutable bindings,
constants, and compiler argument temporaries do not acquire this local receipt.

Original immutable closed-data `let` bindings use the separate
`BuildScratch::value_binding_origins` and `value_binding_uses` ledgers.
`lower/value_binding.rs` retains the qualified binding and statement, actual
emitted `Let` and initializer rows, original initializer expression, and both
the binding and initializer scoped graph roots. Groundness only determines
whether this transport is supported; a tree view cannot replace either root.
`SlotScope::value_binding_authorities` restores the original definition after
lexical shadows and excludes mutable and callable bindings. Missing source
facts refuse supported data lowering. Ordinary `Let`, specialized `LetInt` and
`LetBool`, and successful `Guard` continuations retain their actual physical
allocation and lexical reads. `full/value_prepare.rs` validates those rows;
equal storage kinds never replace the original binding identity.

Original simple `for` item bindings retain the selected iteration operation,
qualified statement, binding and iterator identities, and independently checked
input, item and binding roots. `generic::OriginalIterationBinding` seals the
encoded loop, iterator, item slot and body in a program-owned original receipt;
`OriginalIterationUse` seals each original read's relationship to that binding.
Cold verification checks actual body ancestry, excludes iterator and sibling
reads, rejects mutable item slots, and verifies the original iterator producer.
The prepared item type supplies operand evidence without searching for a
same-typed local slot. Direct closed Lists, including Path items, and scalar
Bytes iteration retain original item authority. Fallible Bytes sources also
retain their actual propagation carrier. `full/iteration_prepare.rs` verifies
each supported producer and physical item read before execution.

Saved named call arguments retain a separate compiler binding receipt in
`BuildScratch::argument_binding_origins`. Each generated slot read names the
original call, authored expansion ordinal and `SolvedArgumentSource`, actual
initializer, slot, and emitted match wrapper with its bind pattern. Parameter
reordering changes destinations without changing this evaluation sequence.
Generated reads never acquire the original initializer's expression identity.

Prepared ground native calls retain their selected registry operation, signature,
argument recipes, omitted slots, result carrier and closed effect roles in the
owning `GenericEvidenceStore`. Both indexed execution routes consult
`FullExecution::ground_native_call` before dispatch, validating the exact program,
body, instruction and decoded operation against the prepared authority. Calls
without an activated proof remain explicit legacy boundaries until their full
operand and protocol authority can be prepared.
Removing a proof for an originally prepared native call or method is rejected
before dispatch by `FullExecution::ground_native_call` and
`FullExecution::ground_native_method`.
`FullStore::original_generic_owner` preserves the original evidence owner
outside the optional evidence store. Verification and both worker entry paths
reject removal or replacement of that complete store.

Generated Result postfix receivers retain a separate
`generic/result_receivers.rs::PreparedResultReceiver` inside the original
native call receipt. `full/result_receiver_prepare.rs` checks the carrier's
authored identity, Result success/error domains and executed `ExprTry` before
admitting the success value as a method receiver. The generated `ExprTry`
never acquires an authored expression identity.

`full/language_result_prepare.rs::verify_language_result_operand` consumes the
original selected primitive operation's operand and result domains separately.
A string equality result remains `Bool` when supplied to a generic function;
its operand domain remains `Str`. `operation_prepare/error_field.rs` applies
the same original-operation boundary to concrete Error `.message` projections,
including family, variant, facet and ProcessError receivers.

`indexed/native_methods.rs::nondefault_method_spelling` shares the bounded
Str and Bytes operation mapping between preparation and execution. The backend
spelling comes from the selected numeric operation and actual receiver domain;
source spelling remains checked by the original sealed method receipt. These
nondefault operations reuse the existing text and byte implementations while
retaining their selected result domains and effects.

`Str.parse_int` carries its selected `TextParseInt` operation and closed
`Result[Int, Error]` producer type through native receipts. Both workers use
the selected operation when producing success values or parser errors.

Materialized `Str.lines` and `Bytes.lines` retain their selected item domains.
`List.collect` retains a separate List receiver contract even though its
selected operation is `StreamCollect`; the worker returns those list items
without introducing a lazy stream carrier.

`Stream.collect` retains its selected Stream receiver, item type and producer
effect roles separately from `List.collect`. Both workers drain it through
the existing evaluator stream driver, preserving suspended script ownership,
producer cancellation and cleanup.

`Str.words` and `Str.split` retain their original selected text operations.
The split packet records an omitted trailing limit as absence, checked against
the original default slot; its backend receives that absence directly. Named
arguments retain authored evaluation order separately from formal slots.

`List.join` retains its concrete string-list receiver and an omitted separator
as absence in the original two-formal method packet. The selected default slot
authorizes the existing backend's empty separator behavior; preparation does
not synthesize a string operand. A named separator retains its authored source
recipe and formal destination separately.

`Str.byte_slice` retains its byte offset and an omitted trailing length as
absence in the original method packet. Its selected backend preserves UTF-8
boundary errors and does not reinterpret those bounds as character offsets.

`fs.children` retains its selected `FsChildren` authority, omitted stat/order
markers and original `Result[Stream[FsEntry], Error]` producer. Specialized
`ExprFsList` and module call packets each retain an exact operand decoder;
named source order and omitted defaults remain part of the original call
receipt. Both workers validate native authority before evaluating its operands
and preserve the existing filesystem stream lifecycle.

`Digest.hex` and `Digest.base64` retain the nominal Digest producer and selected
encoding operation. Their native receipts admit that closed receiver directly
without converting it into a byte or record boundary.

Finite native spread fields retain `PreparedNativeRecordFieldArgument` in the
same sealed `NativeCallSource`. Each receipt keeps the authored record entry,
expansion ordinal and formal destination, exact field membership and closed
record type, and both compiler allocations. Cold verification checks the actual
projection and saved read scopes against the original record producer; a
generated field never acquires an invented expression identity.

Native `Result` success records retain their canonical selected schema in the
sealed native source receipt. `FullExecution::materialize_native_result_record`
uses that schema on `Ok` values before numeric projection. The existing `Err`
value and opaque capability identity remain intact through the host bridge.

Native List and Stream record items retain their selected carrier and record
layout in the same receipt. Materialized items receive that layout immediately;
live streams apply it only to reached items. `lowered_run/native_record_items.rs`
preserves buffering, cancellation and producer ownership without draining the
stream. Filesystem entries retain canonical numeric field slots while metadata
reads remain lazy; prepared layout does not change their logical equality.

Scoped Display eligibility retains its original published requirement and exact
`TypeRef` in the declaring scheme. Concrete witnesses use the checker’s Display
policy for the instantiated type. Generalization may replace source graph type
handles with rigid binders; preparation compares both references within the
original scheme, preserving the source requirement identity.

XSH is implemented as a small compiler-style pipeline around a verified
indexed runtime:

1. `src/syntax` turns source text into the AST and lossless CST.
2. `src/sema` checks names, types, standard-module signatures, and lint rules.
3. `src/runtime` executes verified indexed programs, runs host processes,
   manages cwd/env, and records the runtime graph as trace events.
4. `src/source.rs`, `src/diagnostic.rs`, and `src/trace.rs` provide shared
   source maps, spans, diagnostics, trace events, runtime graph payloads, and
   tracebacks across those stages.
5. `src/loader.rs` owns entry source ingestion, script/module loading, and the
   checked program bundle used by runtime and tooling. `src/runner.rs` owns
   plain script execution for `xsh`, and `crates/xsht/src/cli/mod.rs` wires the
   `xsht` tooling commands.

Declaration inference uses `sema/inference.rs::InferenceContext` for scoped
type and row identities, level-based generalization, operation requirements,
and latent effect relationships. `check/generic.rs` generates constraints;
`check/solved.rs::SolvedTypes` freezes the graph once and attaches declaration,
expression, call-binding, projection, and return-elaboration facts to source
arena identities. `Type::Graph` preserves unresolved generic relationships
inside checking instead of replacing them with `Any`. Ground tree views remain
bounded adapters for consumers that do not yet consume graph identities.
Graph publication validates each resolved node in its exact member scope and
shares the validation cache across retained roots. Cached subtree heights preserve
the structural-depth guard when the same graph is reached through a longer path.
Scope authorization and publication walks charge the solver work budget. The
finite effect inclusion graph is closed before publication; retained arrows,
scheme endpoints, operation evidence, and external callable/substitution facts
preserve quantified effect identities or their solved closed sets.
Source operation requirements retain their exact declaration or value scheme.
Publication checks their templates, selected certificates, substitutions,
producer outputs, and dependency certificates in that source scope. A catalog
candidate does not authorize foreign binders. Shared proof walks charge the work
budget once per requirement and scope; immutable consumers validate membership
with `SolvedGraph::validate_requirement_scoped`.

`RequirementTemplate::CallableInvocation` retains an `InvocationCall` with the
callable type, ordered positional/named/splice arguments, result, computed output
effects, and exact Pure or Pure/Proc/Stream domain. `inference/invocations.rs`
keeps Meta and Rigid callables residual instead of manufacturing Arrow labels,
defaults, rest slots, or kind. A concrete Arrow supplies `InvocationEvidence`
through the shared argument binder, including default timing derived from its
actual kind. `SolvedTypes::invocations` attaches the source requirement and caller
to the actual expression identity through `SolvedInvocation`; publication checks
that requirement in its source scheme. `frontend/query.rs` normalizes the retained
relationship and its actual argument modes without rechecking or guessing a
signature. Computed invocation effects remain distinct from the caller's
permission budget.

`SolvedTypes::argument_sources` retains each checked call's ordered supplied
sources independently of graph handles. `SolvedArgumentSource` preserves the
original entry index, label, span, and expression, record-field projection, or
positional splice. Fields from one finite spread share an entry index and source
record so consumers evaluate that record once. Static supplied slots, dynamic
segment argument indices, conditional branch plans, and constructor supplied
values correspond to this same vector order. Receivers and omitted defaults
remain separate; zero supplied arguments have an explicit empty recipe. Producer
argument transfers use this retained ledger rather than a transient source cache.

`SolvedQuery::schema_validation` and `constructor_application` project frozen
source application facts. Applied phantom arguments and qualified declaration,
default and member owners remain distinct from structural record layouts.
Caller scopes supply alpha normalization; original expression and record-spread
identities supply argument provenance. Exact per-source application certificates
and recorded counts reject substitutions and omissions before projection, without
replaying syntax or validating unrelated sources. Shared type, scope and default
roots use one bounded normalization budget. Assignment and projection indexes
locate local certificates and do not define application equivalence. Conditional
invocation views retain every branch plan; error joins retain independent ordered
input roots, their result and any written completion bound.
`Checker::compact_declarations_from_checked` borrows the existing `CheckOutput`,
retains the same `Arc<SolvedTypes>`, and adapts its prepared constants, constructors,
wire enums, CLI entry plan, and checked callable facts. The tool's single-file and
workspace check paths use this adapter instead of checking the original arena
again. The structural declaration collector remains a ground-view adapter until
compact lowering consumes solved declarations directly; it neither validates
source nor generates constraints.
`Linter::lint_with_checked` and `lint_module_with_checked` borrow that same checked
bundle for effect facts and source-migration baselines. Standalone lint checks
share one lazy original-source check. Different checker options and rewritten
candidate text retain their own checks. Annotation rendering uses `SolvedQuery`
for ground graph types and preserves the conservative source-syntax exclusions.
Annotation writes are a new source revision: the tooling checks the rewritten
text and its compact lowerability with the entry's source identity, module roots,
and checker options before writing. A rejected rewrite leaves the file unchanged.

`indexed/generic.rs::GenericEvidenceStore` owns declaration scopes, canonical
type templates, concrete instantiations, physical record layouts, and sealed
operation witnesses. `indexed/full/generic_prepare.rs` translates solved facts
into these artifacts before verification. Forwarding relationships are derived
from declaration-scoped substitutions, including for unused declarations;
concrete edges are prepared once for reachable call instantiations. Execution
selects an existing edge and uses numeric field slots and fixed operations.
Both indexed evaluators carry the active instantiation through calls and
suspended frames. Their return and cleanup behavior uses the declaration's
fixed return plan, so a returned `Err` payload is distinguished from a declared
Result failure channel.

`indexed/generic/callable_types.rs` materializes type templates directly into
semantic pool identities. Callable templates retain their kind, parameter
labels, positional or named mode, default and rest markers, result, and closed
effects through nested data types. Instance and forwarding validation compares
that complete structure. Signature parameter flags use default bit 0, rest bit
1, and named-only bit 2; function body parameter flags have a separate encoding.
Exporting a typed callable through the legacy `Type` representation remains an
explicit refusal. Materializing a signature alone does not authorize invoking
an unresolved generic callback; invocation needs its own prepared source proof.

`indexed/full/scoped_callable_prepare.rs` reads the original frozen callable
obligation and its exact contextual requirement correspondence for a concrete
instance. `indexed/generic/scoped_invocations.rs` retains the protected source
receipt separately from its public proof view. A signature-only invocation
witness supplies the actual parameter binding; it does not invent a declaration
target. The value handle retains its numeric target and creation environment.
Forwarded obligations retain each enclosing scheme's immediate requirement ID
and the exact ancestry to that original body obligation. Concrete frame
preparation composes those correspondences before reading the frozen invocation
evidence, so repeated calls of the same generic body remain distinct and binder
reordering cannot substitute another invocation's proof. The protected body
receipt owns these correspondences alongside the original argument recipes.
The initial scoped protocol admits Pure callbacks with closed empty effects and
static supplied/default slots. Native alternatives, conditional plans, splice
binding, and effect-quantifier activation require their own prepared protocol.

`indexed/full/generic_operation_prepare.rs` prepares the original checked
`Ok(value)` constructor requirement and its concrete contextual certificates.
`indexed/generic/operation_requirements.rs` retains the constructor's source
instruction and operand separately from the instance witness. Forwarding keeps
the exact source requirement ancestry and each declaration's binder mapping;
it does not create a second constructor instruction. Cold verification checks
the canonical language authority, complete signature, result shell, empty
effects, and original operand before either evaluator selects the stored numeric
constructor operation. Other operation requirements remain explicitly unavailable
until their own source and witness protocols are prepared.

Recursive declaration groups share a canonical binder owner while each member
retains only its reachable type and effect binders. `inference/components.rs`
generalizes after all member constraints and restrictions are solved.
`generic::register_graph_declaration` includes default-only headers with written
returns. Mutable local collection seeds use the graph directly and retain the
same variable in early alias expression facts. Indexed template preparation uses
`scheme_binder_index` to map a member's actual binder identities to local evidence
slots; the canonical owner's ordinal is not a member's ordinal.

The graph path registers pure, proc, native-test, and stream definitions,
including omitted parameters, defaults, and returns. Callable expression facts
preserve complete signatures through aliases, aggregates, conditionals, and
returned values. Mutable bindings keep one shared instantiation. Remaining
adapters are explicit integration work:

- Declaration, local collection, default, and return inference use the shared
  source graph. `return_join::unify_inferred_returns` joins ordinary body
  completions without another declaration walker. Constructor named spreads
  expand original argument sources and check each supplied value once; checker
  cloning and fabricated projection expressions are removed. A bounded legacy
  constructor-variable group still bridges null/empty fields to independently
  checked anchors before graph publication. Its deferred type imports remain
  temporary work for replacing constructor groups with graph variables.
- One checker traversal generates type constraints, finite effect inclusions,
  callable obligations, and producer flows. Declaration generalization retains
  latent effect relationships; graph solving and publication establish the
  summaries. Whole-program effect scans and a second source checker are removed.
- Full and compact consumers project the same immutable solved facts. Source
  registry, operator, and stage requirements retain canonical candidate authority,
  original binding plans, and exact producer paths. Schema validation records
  independently established targets and qualified applications. Constructor and
  process source plans and complete native callable families remain integration
  work; unsupported new execution evidence still fails preparation.
- `lower.rs::CompactLowerConstructProbe::solved_type` projects immutable graph
  container shells into old storage views. Expression and call type helpers
  retain tree/slot/shape reconstruction outside solved expression coverage;
  statement/assertion consumers retain span facts outside exact solved identities.
  `lower_function_with_blocker` obtains parameter and result storage kinds from
  `lower/storage.rs::checked_storage_view` using the original declaration's
  Arrow roots and scheme. A generic `List[T]` keeps List storage, while a bare
  type parameter uses Generic storage. `FunctionBuild::solved_declaration`
  preserves the semantic signature independently of those physical kinds.
  Record destructuring publishes each original leaf `BindingIdentity` while
  `define_binding_target_arena` checks that leaf. Top-level storage and capture
  consumers use those facts directly. Metadata collection returns the first
  unsupported statement, and function prefixes refuse that failure instead of
  constructing a partial binding environment.
- `lower/call_argument_plan.rs::lower_function_call_args` reads original supplied
  sources and checked slot plans from `SolvedTypes`. Script implementations use
  the selected registry candidate's binding rather than an overload search.
  Supplied values stay in source order, while interior omitted slots retain
  declaration-owned default markers. `lower_named_spread_call` consumes original
  call and invocation argument recipes directly, evaluates authored entries
  once, and calls the existing dispatch with unchanged syntax and saved values.
  It does not expand fields again, rebind parameters, or clone an arena for those
  plans. `lower_legacy_named_spread_call` keeps temporary argument projections
  only for native operation and constructor adapters outside that active path.
  Saved finite record entries retain separate original record roots and real
  initialization wrappers in `BuildScratch::argument_record_binding_origins`;
  generated record reads and field projections do not acquire syntax identities.
- `check_stream_stage_arena` keeps temporary stage-call AST nodes outside graph
  publication and accepts only a resolved ground target signature through its
  stage adapter. Generic stage calls require a plan tied to the original stage
  descriptor; generated arena identities cannot establish evidence ownership.
- `lower/iteration.rs` reads original statement and comprehension operation
  facts, their selected language authority, and exact iterator endpoints.
  Item graph identities remain available alongside bounded storage views;
  destructured leaves use their own checked binding facts. Generator identities
  preserve relative qualifier ordinals across filters. Materialized map, string,
  and byte Result sources retain lexical propagation, while list and stream
  adapters retain runtime failure transport. Missing or pending source evidence
  refuses preparation instead of reconstructing an iterable relationship.
- `FullBuilder::predeclare` retains coarse storage-to-signature construction
  for non-scoped headers. Exact ground, nested container, nominal, and producer
signatures must come from solved declarations before this adapter is removed.
  Graph-backed ground declarations now supply exact checked types directly.
  `FunctionBuild::legacy_checked_signature` transfers already checked ground
  parameter/result trees for declarations outside graph ownership, preserving
  precise container contracts across generic calls. It rejects unresolved
  holes and disappears when those declarations use solved graph signatures.

These adapters do not supply missing evidence for generic execution. Unsupported
generic bindings, sources, or operation domains fail preparation.
`lower.rs::CompactFunctionIndex` retains definition keys for lookup and
enumeration. `FullBuilder` predeclares the function units before encoding their
bodies, so mutual recursion and declaration order need no additional lowering
dependency graph. Source-owned checked declaration identities and instance
receipts preserve each generic call's relationship through preparation.
Effect quantifiers currently require prepared effect evidence that the indexed
scope format does not yet carry. Extra row lacks constraints must be represented
by the signature's checked record prefixes; unrepresented constraints fail
preparation. Multiple solved row extensions are flattened before projection
obligations are emitted.

The primary retrieval path is symbol-first: search for the concrete type or
method named in this document, then open its owner file and nearest test. For
the complete frontend vocabulary, see `docs/FRONTEND.md`; use the routing
policy in `AGENTS.md` for task-specific reading and verification.

Boolean statement use is owned by `Checker` and recorded in
`CheckOutput::assertion_spans`. `CompactBodyProbeOutput` carries those facts
into `lower_statement_expr`; `BuildExprRow::Assert` and `FullTag::ExprAssert`
preserve them through indexed verification and both indexed evaluators.
Comparison operands remain available until assertion completion, so failure
rendering can explain evaluated values without repeating evaluation or copying
collection backing on the passing path. Core `AssertionError` uses the nominal
error machinery independently of `native-tests`.

## Prepared scalar map keys

`map_key.rs::MapKey` retains owned scalar identity for ordered map storage;
`MapKeyRef` compares borrowed text/native byte slices without allocation.
Numeric and Duration keys order by value, Bool orders false before true, and
Bytes/Path order by their unchanged raw bytes. String keys use ordinary text
order. UInt retains the existing Int representation and is validated at its
nonnegative typed boundary. `runtime/map.rs` owns conversions to and from
runtime values; constant preparation can use the key representation directly
without depending on the evaluator.

Explicit enum declarations use `ArenaTypeDefBody::TagUnion` and the existing
nominal tag constructor tables in the full and compact checkers.
`parser/stmt.rs::parse_enum_def_arena_only` registers the same type definition
rows as aliases and schemas; indexed preparation and execution reuse tag values
and constructor patterns. Legacy type-union recovery emits
`parse.enum-migration` with token edits and remains a parse failure for execution.
`sema/wire_enums.rs::PreparedWireEnums` shares each validated Str mapping by
canonical declaring identity. Indexed tag constructors retain that identity and
mapping after frontend drop. Explicit require lowers a cached `PreparedSchema`
walk; both execution routes validate and convert a private value before returning
it. Schemas and prepared constants register in the same declaring mapping pool
as constructors; the verifier rejects independently altered mapping copies and
checks schema and constructor metadata before execution.
Record validation uses `lowered_record_field_value`, the owned field view shared
with ordinary record methods. Synthesized fields in compact Stats values remain
visible to the same schema contract as stored record fields.

## Checked key projections

`sema/projection.rs::CheckedProjection` retains a visible checked field selected
by a prepared Str key. Full checking stores it by source span and compact
checking by expression ID. Module export facts retain callable signatures;
unknown keys and hidden fields do not gain checked types. Lowering consumes the
selected expression type while retaining the original receiver, key, and
fallible get/index operation. `xsht` removes a selected-value schema require
only when the guaranteed value already has the exact validated shape.

## Producer suspension and delegation

`indexed_run/explicit_run.rs::ProducerStep` returns either an item, a delegated
source with the saved frame, or completion. `indexed_run/producer.rs` retains
that frame, List cursor, and Stream handle. `runtime/eval/stream.rs` drives
`ScriptStreamStep::Delegate` iteratively, releasing each producer lock before
entering its child while retaining ancestor ownership scopes. Cancellation
removes delegation links and runs child cleanup before parent cleanup. The
same evaluator and indexed frame engine execute every producer; delegation
depth does not become native call depth.

## Process completion policies

`runtime/process.rs::AcceptedExitCodes` validates a bounded ordinary exit-code
set once and stores it in `ProcessInvocation`, `CommandPlan`, and `ManagedChild`.
`completion_error` selects the first rejected segment while retaining the actual
`ProcessStatus`; `runtime/run.rs::run_completion_error` adds invocation context.
No policy preserves each existing run mode's status contract.

Policy-bearing process streams use `ProcessStream` as the child/stdout owner and
`indexed_run/producer.rs::ProcessProducer` as the evaluator stream cursor. Each
pull feeds Bytes stdin while draining stdout and checking timeout/cancellation;
EOF applies the completion policy. The existing producer sweep cancels unreachable
cursors, and the child owner kills and reaps on drop. Suspended trace frames retain
the run's identity without leaving it on the active evaluator event stack.

## `libxsh` Rust façade

The root `xsh` package also provides the shared Rust library consumed by the
`xsh`, `xshi`, and `xsht` products. Its canonical first-party import paths are
the façade modules below:

| Concern | Canonical path | Owner |
|---|---|---|
| source loading, syntax, checking | `xsh::frontend::{load, syntax, check, source}` | `src/frontend.rs`, backed by `src/loader.rs`, `src/syntax`, `src/sema`, and `src/source.rs` |
| diagnostics | `xsh::diagnostic` | `src/diagnostic.rs` |
| ordinary script execution | `xsh::execution::script` | `src/execution.rs`, backed by `src/runner.rs` |
| evaluator/session and runtime values | `xsh::execution::{evaluator, value}` | `src/runtime/eval.rs` and `src/runtime/value.rs` |
| process lifecycle and cancellation | `xsh::process` | `src/process.rs`, backed by `src/runtime/process.rs` |
| structured trace data | `xsh::trace::model` | `src/trace.rs` |
| narrow reusable host adapters | `xsh::host` | `src/lib.rs`, backed by the host adapter implementation |

Frontend AST/CST, checker, evaluator/session, value, and process-group types
are currently first-party tooling APIs: `xshi` and `xsht` need them, but their
representation and lifecycle are still coupled to the compiler/runtime. The
script execution, source/diagnostic, and structured trace contracts are the
initial supported library tier. The former `xsh::runtime`, `xsh::modules`,
`xsh::sema`, `xsh::syntax`, and `xsh::runner` roots are private implementation
owners; new consumers must use the façade instead. The `xsh::app` CLI entrypoint
is owned by the binary target and is not part of the library façade.

This Rust boundary is separate from the XSH language API. Standard module
signatures, records, docs, examples, and runtime operation IDs remain owned by
`crates/xsh-registry` and its language-facing adapters.

Cargo target ownership follows the product boundary: the root `xsh` package
owns the `libxsh` library and `xsh` binary, while `crates/xshi` and
`crates/xsht` own the `xshi` and `xsht` binaries. The root integration harness
resolves those package-owned binaries from the active Cargo profile so the
cross-product runtime tests do not require duplicate root targets.

The workspace is split where a subsystem can have a stable Rust boundary
without depending on XSH source spans, runtime values, diagnostics, or evaluator
state. `crates/xsh-net` owns DNS resolution and XSH's explicitly resolved TCP
dialer, TLS configuration, redirects, body limits, and network error
classification. `h12tiny-client` owns HTTP framing, TLS handshakes, ALPN,
protocol selection, and its bounded connection pools. The main `xsh` crate keeps
the language-facing adapters in `src/modules/dns.rs`,
`src/modules/net.rs`, and `src/runtime/eval/modules/net.rs`: those adapters
translate records and paths into plain Rust request structs, convert crate
results back into `Value`/`RuntimeError`, preserve source spans, honor test
mocks, and manage evaluator-owned pool state.

Each evaluator owns at most one lazy `NetRuntimeOwner` in
`src/runtime/eval.rs`. It owns one `async_executor::Executor`, a named parked
driver thread, bounded transport admission, terminal completions, cancellation,
and two lazy bounded network-file workers. It receives only plain Rust request,
download, upload, client, and completion data; it never receives `Evaluator`,
`Value`, scopes, source spans, trace buffers, or signal hooks. The networking
driver advances transport work only; it cannot execute XSH code.

`NetAgent` is a pool-policy bundle, not an executor owner. Every
`NetAgentKey` has persistent H1-only and auto H1/H2 h12 clients sharing its
evaluator runtime. `net.request`, `net.download`, and `net.upload` submit an
internal H1 operation and wait through evaluator checkpoints. Batches and
`net.start` submit auto-protocol operations; batches retain only their active
completion-driven window and reuse the persistent auto client across calls.
File-backed request bodies, upload sources, and download destinations complete
their bounded file-lane preparation before entering active transport admission.
This keeps a blocked filesystem operation admitted but out of the 32 scarce
DNS/socket/TLS permits. `NetJob` IDs, lexical ownership, result-capacity
reservations, trace events, and signal decisions remain in
`src/runtime/eval/net_job.rs`, on the evaluator side of the boundary. The
runtime records only safe timestamps, status, byte counts, and terminal error
kinds; the evaluator materializes `net.job.*` and `net.transport.*` events, so
the driver never mutates trace storage or retains request secrets.

`h12tiny-client` receives one `RequestOptions` value per dispatch, never in a
pool key. It owns TLS/ALPN and response-header phase races; XSH's
`ResolvedTcpDialer` receives the same options and owns platform DNS plus the
aggregate resolved-address TCP race. `timeout` remains an XSH driver deadline
from admission onward, including file preparation and scheduler queueing.

The runtime wake socket and file-completion sockets are nonblocking and
close-on-exec. Owner teardown cancels remaining jobs, rejects further work,
joins the driver and file workers, and drops agents. The relevant grep targets
are `NetRuntimeOwner`, `NetOperation`, `request_many_with_runtime`,
`NetJobTask`, `h12_client`, `ResolvedTcpDialer`, and
`native_xsh_net_single_calls_force_https_http1`. Tokio, `hyper-util`, and
`hyper-rustls` are intentionally absent from this boundary.

Core assertions retain `ArenaStmtKind::Assert` condition/message expressions.
`BuildStmtRow::Assert` carries an optional message so bare Bool statements and
explicit contextual assertions share `FullTag::StmtAssert`, codec verification,
propagation, and cleanup. `eval_indexed_assertion` uses a work stack for logical
conditions and retains reached comparison values once; its diagnostic renderer
bounds scalar text and reports container types without materializing them. The
message remains an indexed expression and executes only on a false condition.

Guarded control statements use `ArenaStmtKind::GuardedStmt` around ordinary
return/break/continue/yield statements. `Checker::check_condition_arena` checks
the Bool/Status guard before applying selected-branch narrowing to the payload.
The checker restores the skipped branch's lexical bindings and complementary
condition proof after a valid exiting payload; resumable yields establish no
continuation proof.
`CompactLowerConstructProbe::lower_stmt_with_blocker_guard` lowers this wrapper
through ordinary conditional statement rows, preserving lazy payload evaluation
and lexical cleanup ownership.
The parser retains ungrouped run argv boundaries; grouped expressions own their
closing delimiter so a run-valued payload can precede a postfix guard.

There is no JIT, green-thread scheduler, or async task runtime in the execution
path. The checked arena is lowered into a compact verified indexed store before
execution. `src/runner.rs` shares the owned parsed arena between the full
checker and compact lowering, avoiding a second arena copy during startup.
`src/runtime/eval.rs` and its focused runtime modules execute borrowed
instruction and driver payloads while coordinating host processes, streams,
cwd/env state, defers, signals, and trace events. Process forms and other
OS-facing operations remain explicit indexed host-operation boundaries. The
normal script runner and native-test harness execute the same verified indexed
representation. There is no arena execution mode or compatibility interpreter.

Brace literal entries reuse `ArenaRecordFieldKind` with explicit `Computed`
key/value children. Computed entries and contextual `Map[T]` facts select
`BuildExprRow::MapLiteral`; record spreads retain their separate interpretation.
`CompactBodyProbe::check_compact_expr_expected` and `apply_compact_expected`
preserve Map classification across bindings, returns, nested containers, and
resolved call parameters. Both indexed routes evaluate each key before its
value and populate one canonical Map, retaining source spans and alias values.

List literals retain typed `ArenaListElementRange` entries with a child expression
and optional splice span. Traversal-only owners use `list_element_exprs`; owners
that interpret elements use `list_elements` so scalar nesting cannot be lost.
Mixed literals lower to `BuildExprRow::ListBuild` and `FullTag::ExprListBuild`.
The indexed executor appends each evaluated scalar or List into one output vector,
checks capacity before extending, and stops before subsequent elements on failure.
Ordinary literals keep their existing indexed representation and singleton update
optimization.

List and map comprehensions retain one ordered `ArenaCompQualifier` range.
`LoweredCompQualifiers` verifies that the sequence starts with a loop;
`explicit_run.rs::ListCompState` holds nested iterators and resumes clauses in
textual order. Active stream ownership is shared with a work-stack cleanup
entry, so propagation and runtime failure cancel suspended producers without
collecting their remaining values.

Optional postfix receiver absence and Optional fallback lower through verified `MatchExpr` instructions
with a hidden receiver slot. The null arm skips the whole selected operation;
the present arm retains Result values. Result postfix receivers instead lower
through one `Try` instruction before the ordinary operation.
Runtime changes should preserve source-visible order, explicit boundaries, and
traceable failure paths before pursuing cleverness. List compound assignment
uses the ordinary indexed assignment route. A singleton list right side is
executed directly as one item, then appended through the existing ownership
aware list primitive. General extension evaluates its right side before taking
the target container; alias backing is copied only when shared. Failed right
side evaluation leaves the target intact.
`src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs` handles live
serial stage prefixes, pulling one source row through all supported stages
before the next. It stops at bounded terminals, collects at a value boundary,
and materializes before an unsupported stage. Other indexed pipeline shapes
remain in `src/runtime/eval/lowered_run/indexed_run.rs`.

Nested assignment paths lower to `BuildStmtRow::AssignPath` with verified
`LoweredAssignStep` field/index selectors. Indexed frames evaluate selectors and
RHS before `apply_indexed_path_assignment` observes the current root, validates
the complete path, and descends through ownership-aware record/map/list storage.
`lowered_ops.rs::lowered_record_field_mut` is the shared record COW primitive.
Unique backing is retained; shared ancestors copy only when mutable descent
reaches them. The top-level driver publishes operand mutations before surfacing
an enclosing assignment failure, so cleanup observes those effects.

Half-open slices reuse `ArenaExprKind::Slice` and the verified `ExprSlice` row.
`check_slice_arena` checks List/Str/Bytes receivers and Int bounds; indexed dispatch
evaluates the receiver and supplied bounds once in source order.
`lowered_slice_value` normalizes bounds and preserves Unicode scalar indexing
for text while converting the selected boundaries into internal UTF-8 views.
Bytes views retain their original backing allocation across nested slices; list
slices retain independent value semantics.

## Embedded Standard Library

Some public standard-module entries execute embedded XSH instead of a native
operation. The public contract is unchanged and still declared once in
`crates/xsh-registry`; what changes is where the entry's behavior comes from.

Each entry or overload carries an `ImplBinding`:

- `Native` — the existing `RuntimeOp` body. This is the default and covers
  everything not explicitly migrated.
- `Script` — an implementation function in an embedded module.

**Sources.** Maintained implementations live under `stdlib/` and are embedded
through the compile-time catalog in `src/stdlib.rs`. `include_str!` embeds each
file and makes Cargo rebuild tracking cover it. The catalog is a fixed table: it
never scans a directory, reads the environment, or consults the filesystem when
the executable runs, and no installed stdlib directory is required. Retained
sources and diagnostics ship in normal binaries.

`bytes.human` and `time.duration_compact` retain native operations after their
per-call B0 regressions were measured, so they have no embedded source module.
`tui.left_pad` and `tui.right_pad` also retain their native visible-width scan;
the TUI escape-sequence producers remain in `stdlib/tui.xsh`. Their measured
dispositions are in `bench/stdlib-port/README.md`.
The `cli` argument policy is native in `src/modules/cli.rs`; the script policy's
repeated calls and record conversions missed the B0 CLI batch budgets. The
public signatures and argument policy remain in `crates/xsh-registry` and
`docs/SPEC.md`.
`shlex.quote` and `shlex.join` use `src/modules/shlex.rs` after their embedded
implementations missed both fixed quoting batch budgets. The quoting contract
and native XSH tests remain unchanged.
`ini.encode` and `ini.write` use `src/modules/ini.rs` after the embedded encoder
missed the 1,000-key B0 workload on both hosts. The decoder was already native;
the public INI contract and native XSH tests remain unchanged.
`mime.lookup_ext`, `mime.lookup_path`, and `mime.parse` use
`src/modules/mime.rs` after the embedded implementation missed the 500-lookup
B0 workload on both hosts. The host overlay is read at each lookup, including
each candidate suffix of a path; no persistent table is introduced.
`json.get`, `json.set`, and `json.remove` use `src/modules/json.rs` after the
embedded path policy missed the 400-round B0 workload on both hosts.
`json.encode_lines` remains in the small `stdlib/json.xsh` module because its
bulk composition already passes B0 by a large margin.
`env.get_or`, `env.bool`, and `env.int` use their native scoped-overlay path
after the embedded conversions missed the macOS B0 batch. The environment
module has no embedded source. `hash.parse_check_line` uses
`src/modules/hash.rs` after its embedded parser missed the macOS B0 batch;
`hash.verify_file` retains the separate, passing `stdlib/hash.xsh` policy.
`Str.wrap` and `Str.fields` use `src/modules/text.rs` after the complete
Unicode wrapping workload exceeded B0 with embedded policy. Their public
signatures and native XSH tests are unchanged; `stdlib/text.xsh` and the
unused script-method selection table were removed. `wrap_line` iterates word
slices and tracks scalar columns once per piece, retaining the native greedy
wrapping contract without per-word chunk vectors.

**Preparation.** `src/loader.rs` extends ordinary preparation:

1. Parse the entry and its statically loaded user-module graph.
2. `stdlib::required_modules` selects embedded modules from the public
   spellings the parsed arena mentions. Selection is syntactic and
   conservative: over-selection prepares an implementation the program never
   calls, while under-selection would be a preparation defect. A referenced
   user-code loading route (`module.load`) selects the complete applicable set.
   The set comes from the registry's bindings for the current target; catalog
   sources that have no binding on this target are not prepared. The catalog
   still embeds those sources so builds for other targets can use them.
   All current script bindings are module functions. Text methods are native,
   so receiver fields do not trigger embedded preparation.
3. Each selected module is parsed at most once into the same arena as an
   *internal* module and checked with the program.
4. `lower_script_module_call` / `lower_script_method_call` in
   `src/runtime/eval/lower.rs` bind a script-backed public call to the prepared
   implementation function and emit an ordinary `Call`, so execution uses the
   normal frame engine.

Catalog ingestion seals original native bridge declarations and formal rows
from that module's existing parse. The checker publishes the exact native
signature and effects without assigning expression or completion facts to its
linkage body. Lowering rewrites only original checked invocations in the
declaring catalog module. `full/bridge_prepare.rs` and `generic/bridges.rs`
retain the original caller, argument recipe, formal and actual types, result,
and program receipt. Both execution routes consume that authority after the
frontend has been disposed. Authored functions with matching names or internal
module flags acquire no catalog authority.

A dynamically loaded user module never reparses embedded source: it lowers its
standard calls to `BuildExprRow::ExternalCall`, and the runtime resolves them
through the evaluator's dynamic function table to the implementations the
loading program already prepared.

**Platform-specific bindings.** `linux_uptime_entry` in
`crates/xsh-registry/src/signature/modules.rs` selects the script binding for
`unix.uptime_seconds` on Linux and the native body elsewhere. The R12 text
readers `linux.meminfo`, `system.memory`, and `system.os_release` use native
operations after their embedded implementations exceeded the cumulative B0
budget. Their parsers live in `src/modules/linux/real/kernel.rs` and
`src/modules/system.rs`. `linux.modules` retains its native stream on every
target after its script producer failed the full-scan B0 gate. The gated Linux
prototypes that were measured and reverted remain native; their dispositions are in
`bench/stdlib-port/README.md`.

**Namespace integrity.** `ArenaProgram::modules` entries carry an `internal`
flag. Internal modules use the reserved namespace `<xsh-stdlib:IDENTITY>`, a
spelling no XSH identifier can produce, so user source, `use` paths, module
search roots, and dynamic modules cannot name them. Their helpers are excluded
from the unqualified declaration tables, from the global top-level name set, and
from user-module collection. User modules use their resolved file key as the
internal function namespace, so equal basenames in different directories do not
share captured bindings. `xsh::frontend::stdlib_preparation` exposes
test-only preparation counters behind the existing `native-tests` feature.
`src/stdlib.rs::every_catalog_module_parses_checks_and_lowers` validates every
bundled implementation, including modules unused on the current target. The
`xsht check` CLI tests assert that user-module parse and call-lowering failures
retain user source locations and do not present internal namespaces as callable
names.

Frontend checker-lowering and lint workers use a fixed 16 MiB stack through
`xsht::cli::FRONTEND_WORKER_STACK_BYTES`. Recursive named schema construction
can exceed the platform's small default worker stack while remaining valid;
explicit `thread::Builder::spawn_scoped` stacks keep tooling independent of
`RUST_MIN_STACK` and bound the reservation for each worker.

The private `BridgeTypeName` operation remains restricted by verifier
provenance and belongs to JSON Lines. The CLI policy returned to
`src/modules/cli.rs` after the measured script path failed the B0 batch gate.
`bench/stdlib-port/README.md` owns measured dispositions and points to raw
results in that directory.

## Executable IR Ownership

The executable frontend has stable owners rather than a migration path:

- `src/runtime/eval/indexed.rs` owns compact IR identities, ranges, and build
  errors; `indexed/full.rs` owns the immutable store, builder checkpoints, and
  store verifier because the verifier validates that exact layout.
- `src/runtime/eval/lower.rs` owns checked-arena-to-build-scratch construction.
  `BuildScratch`, `ProgramBuild`, and `FunctionBuild` are construction-only and
  are dropped after `FullProgram` commits.
- `src/runtime/eval/indexed/semantic.rs` owns semantic pool construction and
  finalized canonical identities.
- `src/runtime/eval/lowered_run/indexed_run.rs` owns instruction decoding and
  execution. Its `explicit_run.rs` child owns the heap-backed call, work, and
  continuation frames; it is the only recursive-language-call executor. Field
  bases, index operands, and native module arguments are scheduled on these
  frames before dispatch. Calls nested in projections therefore do not retain
  recursive operand evaluation on the native stack. Native argument holes and
  source order are preserved, and both dispatch paths share module tracing.
  Selected `ExprMatch` arm chains in `eval_indexed_match_expr` advance
  iteratively. Named-argument constructor preparation can therefore bind a
  wide record without adding a native evaluator frame for each field; subject
  and guard evaluation still follows source order.
- `src/runtime/eval.rs` owns installation, dynamic-function registration, slot
  pooling, and evaluator/session lifetime. It never owns a second executable
  representation.

Regex literal occurrences in `src/syntax/arena.rs` retain their raw source text,
span, and a shared preparation cell. `src/modules/regex.rs::prepare_literal`
uses the same compiler as dynamic `regex.compile`, and checked preparation
visits every occurrence, including unreachable bodies. Arena clones and
frontend passes share the cell. Lowering reads completed cells and carries
immutable engines into the verified `FullStore` regex pool; indexed execution
clones engine handles. The owning source/program bounds the cache lifetime.
Builder rewind truncates the pool and the verifier rejects invalid pool indices.

`IrBuildError::span` retains the source ID with its byte range. Imported user
modules and embedded modules share one arena, so `Evaluator` must render a build
failure against the span's source rather than reconstructing it in the entry
file.

`FunctionHeader`, `StmtFlow`, `BuildScratch`, and the other final runtime types
describe their role without migration-version names. A clean construction gap
is rendered as a diagnostic; it cannot select another evaluator.

`docs/SPEC.md` is the language contract. `docs/SPEC-TYPING.md` covers
typechecking, `docs/SPEC-INTERACTIVE.md` covers `xshi`, and
`docs/SPEC-OS.md` covers OS-facing runtime behavior such as process groups,
signals, cancellation, and signal hooks. The `AGENTS.md` routing policy chooses
the smallest useful reading set for a change. `docs/FRONTEND.md` is the
implementation guide for the compact frontend,
indexed runtime plumbing, symbol identity, registry invariants, and benchmark
verification. `docs/COVERAGE.md` tracks the practical coverage limits for areas
that need larger harnesses rather than branch-only tests.

structure for tooling. Arena nodes carry `Span` values from `src/source.rs`, and
`ArenaParseOutput` carries both the arena program and CST. The active formatter
lives in `crates/xsht/src/format.rs`. Parser changes should usually come with
formatter and syntax fixture coverage so new syntax round-trips.

`docs/XSHT.md` describes the tooling architecture in more detail, while
`docs/XSHT-FMT.md` describes formatter design and layout policy: command
ownership, `xsht-config.ini`, AST-vs-CST responsibilities, formatter comment
policy, and CST-backed source edits for autofixes.

`Linter::lint_internal` enters the arena program's `SymbolOwner` for constructor
facts and rule traversal. Qualified enum names created after checking remain
owned by that program, including in workspace worker threads.

Tooling traverses `ArenaProgram`/`AstArena` directly, or the CST when exact token
and trivia placement matters. There is no recursive AST visitor layer; adding new
syntax requires updating each arena/CST consumer that owns behavior for that
surface.

**Adding a new arena node.** When you add a variant to `ArenaExprKind`,
`ArenaStmtKind`, or another arena enum:

1. Add the arena storage and accessor shape in `src/syntax/arena.rs`.
2. Parse it through the arena builder in `src/syntax/parser/*`.
3. Format it in `crates/xsht/src/format.rs`.
4. Type-check it in `src/sema/check/*`.
5. Lower/evaluate it in `src/runtime/eval/lower.rs`,
   `src/runtime/eval/lowered_run.rs`, or the relevant runtime module.
6. Handle it in `crates/xsht/src/lint.rs` and `crates/xsht/src/grep.rs` when the
   new surface affects lint or grep behavior.

**Formatter stage intent.** Which pipeline stages get `()` when they have no
args is declared on the `StreamStageKind` enum itself via
`canonical_parens_when_empty()`, not via a hardcoded list in the formatter. When
adding a new stage, set this intentionally.

The parser keeps language shape decisions local. Avoid teaching later stages to
recover from ambiguous ASTs when the parser can represent the construct
directly.

Block parameters have one syntax representation: `ArenaBlock.params`.
`Parser::parse_block_arena_only` reads the shared header, and each owner checks
its arity and supplied input type. Error handlers use
`Checker::check_error_handler_block_arena`; ordinary statement blocks reject
headers. `ArenaStmtKind::With` and `Guard` retain only the handler block id.
Indexed lowering resolves header names to immutable lexical slots, without a
callable frame. `BuildStmtRow::With` evaluates sequential bindings once and
selects its handler on the first Result or propagated initializer error;
its owned scope releases successful prefix bindings on every exit.
Rejected outside-brace headers are recovered only with a parser error and
precise edit hints. Lint tooling accepts only those diagnosed edits and parses
and checks the rewritten source normally before writing it.

## Semantics

`src/sema/check/record_require.rs` owns the removed `record.require` diagnostic
and identity migration metadata. Its finite scalar migration table never enters
runtime dispatch. The record module and its private string-contract parser are
absent; `.require(Type)` uses the ordinary schema checker and indexed schema
validation. Record receiver methods and typed module contracts retain their
existing owners. `SolvedModuleProjection` retains the original receiver, access
carrier, and exact export type against a core `ModuleProjection` constraint
contribution. Cold validation checks that promise without recovering a record
row or an implementation declaration. Prepared constants provide contract values; a known plain
receiver and exact existing schema are required before offering an edit.

`InvocationPlan` retains either a unique selected signature and binding or every
possible callable branch with its own binding, default timing, and effects.
Source callable joins use `InferenceContext::join_callable_values` while each
branch is checked independently. Computed invocation arguments reuse the original
checked expression endpoints and prove compatibility with their checked views;
list splices keep the original list endpoint and source flow before branch binding. Ordinary callable producer references retain
the checked expression instance as their immutable origin; their principal
declaration signature remains separate navigation and scope metadata. Frozen
source validation rejects replacing that instance with the principal signature.

`ProducerFlowKind::OptionalLift` retains nullable completion independently of
type equality. It transfers original producer and callable locations into one
`OptionalPayload` layer, preserving inputs that already carry that layer.
Declaration completion creates this flow after its return relationship is solved.
Local error captures separately retain reached failure projections from their
original operands; nested captures own separate lists, and consuming a failure
payload retains its pull and cleanup permissions.

`Checker` in `src/sema/check.rs` owns the main checker state: lexical scopes, function
signatures, imported modules, current return type, purity context, `$?`
availability, and stream item context.

Focused semantic rules live beside it:

- `src/sema/constraints.rs::TypeConstraints` owns bounded monomorphic
  substitutions for one checked inference problem. `Type::Inference` carries
  a fresh identity distinct from dynamic and recovery types. Alias constraints
  preserve that identity; transactional constraints retain initializer and
  contribution spans and roll back failed nested substitutions. Partial
  substitution keeps unresolved identities until the owning checker requires
  a concrete contract. Indexed type pools reject unresolved identities, and
  runtime type tests cannot satisfy them.
- `src/sema/check/local_inference.rs` connects local empty collections and
  mutable nullable accumulators to the enclosing declaration's shared graph.
  Initializers, aliases, and later contributions retain the same monomorphic
  identities; immutable value schemes obey the graph's level and capture
  restrictions. Defaults and original argument sources are checked once.
  Checker cloning, speculative body probes, and spread-generated projection
  expressions are removed. A bounded constructor-variable adapter temporarily
  retains checked original-expression types and callers while null/empty fields
  await independently checked anchors, then publishes each source fact once.
  Checked binding types supply indexed slot metadata, so a null initializer
  cannot erase a solved Optional contract. Full and compact publication use
  the same solved expression and callable facts; unresolved material facts
  diagnose rather than become dynamic types.
- `crates/xsh-registry/src/signature/` owns standard callable contracts and
  runtime operation identities; `src/modules/signature.rs` adapts them to
  semantic types and effects. `src/sema/builtin_templates.rs` instantiates
  receiver and parameter relationships through the shared type constraints.
  Registry semantic-rule metadata identifies schema validation, constant-key
  projection, and CLI descriptor facts separately from structural substitution.

- `src/modules` contains shared host helpers for standard modules.
- `src/sema/records.rs` contains shared record schemas.
- `src/sema/constants.rs::RecordConstructors` resolves user schema constructors
  and aliases in lexical module namespaces. Its checked application resolver
  caches instances by defining schema and resolved arguments, substitutes
  declaration-owned fields, and rejects recursive applications before caching.
  `begin_constructor_inference` and `finish_constructor_inference` share the
  constraint solver between ordinary checking and bounded constants.
  `SchemaExpectation` retains independently declared application arguments
  through expected fields and container slots without making records nominal.
  Selected builtin method templates project independently declared receiver
  applications into argument slots through `parameter_schema_contexts`;
  structural receiver fields never invent unused application arguments.
  Concrete `record_constructor_instances` facts feed compact checking and
  lowering after inference completes. Defaults supply no inference evidence.
  `src/sema/check/constructor_application.rs` publishes original expression and
  spread-record field identities, qualified applied aliases and underlying
  default owners, phantom arguments, fixed supplied/default slots, and exact
  graph assignment and projection origins. `constructor_defaults` shares
  declaration-owned prepared constants; `constructor_nominals` retains exact
  member payload slots. Tag and error-variant plans retain an operation
  requirement whose canonical declaration candidate includes the qualified
  source, namespace, declaration, and member identity. Equal payload signatures
  cannot substitute a different member after syntax disposal. Frozen validation
  compares those authorities and source
  endpoints without syntax or name-based member discovery. Scoped roots include
  application arguments, formals, actual values, and results; retained storage
  accounts nested contexts, vector capacities, and shared literal allocations.
  The remaining constructor-group `TypeConstraints` adapter contributes
  independently checked operands before graph import, and defers original
  null/empty/nested expression facts until the group resolves. It never rechecks
  a source expression or chooses a type from observed callers.
  Fully substituted instances reuse `Type::Record`; no runtime generic dispatch
  is introduced. `LiteralConstant` admits bounded
  literal trees and earlier immutable literal bindings; checker, parameter-default
  lowering, constructor lowering, and conservative constructor fixes share this
  analysis. Constructor lowering emits existing record and schema-check rows,
  preserving supplied field order and independent aggregate values.
  `RecordConstructors::source_id` indexes each declaration's source while its
  lexical namespace is collected. `check/schema_validation.rs` publishes
  explicit and contextual `require` decisions in `SolvedTypes::schema_validations`.
  Each expression identity retains its original input expression and type,
  independently selected target and Result type, lexical caller, and recursive
  qualified application identities with concrete argument handles. Phantom
  arguments remain present when record layouts are equal. Collection charges
  bounded node and edge work; publication roots and validation preserve the
  handles and private declaration owners after AST disposal.
  `SolvedGraph` retains a complete immutable `ScopedApplicationRoot` ledger.
  Each certificate binds an original source expression, typed nested schema
  path, alias ordinal, qualified type declaration, and original argument
  handles. `SolvedTypes::source_application_roots` projects current schema and
  record-constructor facts for comparison with that ledger, rejecting changed
  phantom arguments and missing applications even when record layouts agree.
  Publication validates constructor arguments in their exact source scheme;
  validation targets still require concrete reifiable arguments. Ledger path
  and argument storage is included in retained graph accounting.
- `src/sema/check/stream.rs` checks structured stream pipelines.
  `CheckedStreamStage` retains canonical input and output types per stage;
  namespace and source spans distinguish equal offsets in separate modules.
  The ordinary inference solver resolves these facts before
  `CompactDeclOutput.stream_stage_types` publishes them. Compact traversal and
  lowering consume the same input context and output contract, including
  terminal accumulators and adapter item types, without reconstructing stage
  results independently. JSON adapter items remain unchecked `Any` values.
  Stage configuration lives in ordinary `ArenaCallArg` lists and uses
  `sema::arguments::expand_named_arguments` and `bind_static_arguments`, with
  fixed parameter contracts in `xsh_registry::stream_parameters`. Lowering
  evaluates supplied entries in source order into checked temporary slots at
  the existing stage boundary; indexed stage opcodes retain their specialized
  configuration and worker machinery.
  Static unary callable descriptors use the same argument binder with a
  `block` role. `stage_callable_argument` separates the descriptor from fixed
  configuration; `append_stage_callable_block` creates a private temporary
  ordinary call for checking and lowering. Compact `stage_callable_types`
  retain return types without erasing the descriptor into a function value.
  The existing verified call and stage rows execute per item, while checker
  `statically_resolved_call_spans` authorize exact transparent wrapper fixes.
- `crates/xsht/src/lint.rs` reports non-fatal quality issues. Its `LintExprVisitor`
  implements `syntax::visitor::Visitor`; add new lint rules by adding methods
  there, not by expanding the traversal switch.
- `crates/xsht/src/grep.rs` implements structural pattern matching over the AST using
  the `Visitor` trait. Adding new AST nodes requires no changes here.

The checker should report diagnostics and continue with an internal recovery
type where possible. Public dynamic data is `Type::Any`; recovery types should
not leak into generated docs or user-facing signatures.

Dynamic boundaries use `Type::matches_expected` and
`Checker::expect_type` for both source checking and execution preparation.
`Type::ErasedRecord` carries builtin `Record` erasure without certifying fields;
`Type::Record` retains known fields, including an exact empty literal shape.
Explicit schema validation and type patterns establish concrete facts. Expected
types alone do not validate dynamic data. The checker options retain tooling
controls such as reveal output, without a separate dynamic compatibility policy.

## Runtime

`Evaluator` in `src/runtime/eval.rs` owns the evaluator state: scopes, indexed program,
stdout/stderr capture, cwd, env, last process status, trace events, call stack,
pending traceback, and stream item context.

Focused runtime behavior lives beside it:

- `Evaluator::collect_stream_values` in `src/runtime/eval/stream.rs` materializes structured
  stream values and drains live sources.
- `src/runtime/eval/modules.rs` dispatches standard-module calls that still
  need evaluator state.
- `src/runtime/process.rs` owns process invocation, redirection, argv/env
  conversion, and cancellation signals.
  `ProcessRedirection::Input` and `CommandRedirection::Input` retain immutable
  byte input. `InputDelivery` feeds a nonblocking child pipe in bounded writes
  under the capture, pipeline, and managed-child owners; evaluator checkpoints
  advance owned spawn input without introducing a public scheduler.
- `execute_run` in `src/runtime/run.rs` executes `run` forms.
- `src/runtime/value.rs` defines runtime values and error constructors.

Standard module API signatures and runtime operation IDs live in
`src/modules/signature.rs`. Host helpers that do not need evaluator state live
under `src/modules`, while stateful dispatch stays under `src/runtime/eval/*`.
Network host implementation is the first extracted helper crate: keep reusable
DNS and HTTP transport code in `crates/xsh-net`, and keep XSH-specific record
parsing, source spans, test-host interception, effect behavior, and evaluator
state in the main crate adapters. Do not widen evaluator fields just to share
code.

`src/runtime/eval/indexed/full.rs` owns the finalized function store and
source-ordered effect driver. A `FullProgram` is installed only after whole-store
verification; the script runner then drops parser and lowering ownership before
execution. Native tests prepare and call the same indexed program. See
`docs/FRONTEND.md` before adding instructions, runtime operations, value kinds,
or execution shortcuts.

## Interactive

`xshi` is an interactive frontend: the terminal UI (`crates/xshi/src/interactive/`
`repl`, `input`, `line`, `render`, `complete`, `prompt`) and a shell-language
layer (`shell/` lexing, parsing, globbing; `app.rs` execution). Its observable
behavior is that of the `ish` shell, held in place by differential PTY tests
(`docs/SPEC-INTERACTIVE.md`, `docs/TEST-MAP.md`). External commands run through
the same process substrate as `run` in scripts (`src/runtime/process.rs`);
`xshi` adds no compatibility-builtin registry or sudo shim. Core utility names
are ordinary PATH commands, including XSH-authored scripts under `core/` when
that directory is on PATH.

History is `xshi`'s one piece of cross-process state. `history.rs` owns the
in-memory entries and search; `history/store.rs` owns the log, cache, lock, and
reset marker, and compaction is a disk-to-disk merge under an exclusive lock, so
no shell's memory is authoritative. Directory environments (`denv.rs`) evaluate
`.envrc` files in a child interpreter and record only the environment
difference.

## Tracing And Errors

`RuntimeError` and `RunError` retain immutable diagnostic causes through
`ErrorCause` in `src/runtime/value/error_cause.rs`. Each link owns a shared typed
`Value`; attachment copies only the outer metadata and replaces its immediate
cause. Link destruction consumes uniquely owned suffixes iteratively. Payload
fields, nominal matching, Result typing, and internal abort transfers remain
separate from this metadata. `Value::resource_reachable_values` in
`src/runtime/value/resource_values.rs` borrows ordinary containers, error payloads,
and typed causes iteratively, visiting shared descendants once. Owned host
resource transfers use this complete traversal rather than diagnostic depth limits.
Explicit capture transfers from every discarded statement scope to the surviving
catcher scope before cleanup; recursive blocks promote checked failures to their
parent. Runtime-error transport borrows payload/cause roots directly and preserves
these resources through callee cleanup. Context scope rejection covers propagated
values and checked runtime failures before restoration.

`TraceError::from_value` snapshots causes into a flat bounded sequence of
`TraceErrorDetail` values in `src/trace/error_causes.rs`. Rendering never walks an
unbounded recursive diagnostic tree, and process status and lexical context spans
remain structured. Checked error boundaries restore the original `RunError`
including its cause when runtime-error transport was required.

`TraceEvent` and `TracePayload` in `src/trace.rs` define trace events, payloads,
and traceback data. Together
these events are the runtime graph projection: source spans anchor nodes back to
the tree-shaped program, parent ids preserve dynamic containment, and payloads
record process, stream, cwd/env, resource, and failure relationships. Public
trace rendering is owned by `xsht trace`; `xsh` keeps only the traceback
rendering needed for runtime failures and a private minimal coverage event
writer.
Runtime code should preserve structured relationships between source spans,
calls, process boundaries, stream stages, scoped ambient state, and propagated
errors.
Runtime code should preserve the distinction between status-as-data and
propagated errors:

- statement-position plain `run` asserts success by default;
- value-position plain `run` and `run.status` return inspectable status data;
- `?` unwraps `Result` values and remains available as an explicit success
  assertion for process forms;
- module APIs generally return `Result` values instead of throwing runtime
  errors for expected host failures.

## Tests And Examples

Runtime fixtures live under `tests/fixtures/runtime`. Syntax and semantic
fixtures live under `tests/fixtures/sema` and `tests/fixtures/syntax`.
Executable tutorial examples live in `examples/`. Larger standalone programs
live as `.xsh` scripts in `showcase/`, with native tests in `showcase/tests/`.
Both corpora are checked by `tests/runtime.rs` and `xsht fmt --check`.

`tests/syntax.rs` includes a formatter idempotency test that runs every cataloged
example through format → reparse → format again and asserts: no parse errors, and
the two formatted outputs are identical. This catches two classes of formatter
regression — output that cannot be reparsed, and output that is not stable under
repeated formatting — without needing to run the binary. Add examples to
`examples/catalog.json` so they are covered.

When adding language behavior, update the closest combination of: parser,
visitor.rs (traversal), checker, runtime, formatter (`canonical_parens_when_empty`
if adding a stream stage), guide, examples, and TODO status. Small features
should still leave the roadmap and examples in a state that describes what is
actually implemented.

Checked statement/value positions are explicit facts shared by lowering and tooling.
`CheckOutput::statement_positions` retains source spans; compact facts retain
`StmtId` and inferred `block_types`. `ArenaExprKind::ValueBlock` lowers to an
ordinary indexed scope, with a distinct value flow consumed by that expression.
The selected value is held before defers and host-resource cleanup, while lexical
return, propagation, and loop transfers pass to their established owners.

Record binding targets retain field selectors separately from recursively nested
binding targets in `ArenaDestructureField`. The checker resolves each selected
field against its enclosing schema. Indexed execution shares
`LoweredCompTarget` across declaration, iteration, comprehension, and guard
bindings; it selects every required field before writing any slot or exposing
any top-level name. Mutable selections are ordinary local values. Tooling uses
the same recursive target to resolve bound names and preserve renamed fields.

Direct map iteration uses `Type::iteration_item_type` to retain the structural
entry shape in both checker paths and lowering. `LoweredMapCursor` holds an
`Arc` to the evaluated map storage and a key-range position, constructing only
the next entry. Ordinary loop frames and comprehension qualifier cursors share
that entry representation. `Result[Map]` iterable sources lower through one
existing propagation operation before cursor creation, preserving nominal errors
and lexical cleanup. Pipeline map-source conversion has its own owner.

Direct Str/Bytes iteration shares `lowered_run.rs::LoweredScalarCursor` between
ordinary and heap frame execution. The cursor retains source storage and byte
bounds; Str steps create one scalar `LoweredStrView`, while Bytes steps produce
one Int. `FrameWork::ForScalars` and `CompIterator::Scalars` preserve suspension
and nested clause positions without storing an adapter List. Checked
`Type::iteration_item_type` facts feed both binding paths; `lower_direct_iterable`
adds the existing propagation operation for supported outer Results.

Deferred blocks reuse `ArenaExprKind::ValueBlock` under `ArenaStmtKind::Defer`.
`lower_deferred_expr` lowers every body statement in statement position. Indexed
execution saves a defer offset for each live lexical scope; suspension carries
those offsets alongside the slot scopes. Cleanup evaluates against live slots
before releasing its registering scope, preserving mutable capture reads and
nested cleanup order. `run_indexed_defers` executes every registered action,
retains the first failure, and reports secondary failures without replacing the
primary traceback.

Eager recursive calls and lexical statement blocks use
`indexed_run.rs::finish_indexed_statement_scope`. It retains escaping values and
checked failures in the parent, releases unused process handles before deferred
actions, and closes the resource scope even after an error or forced abort.
Explicit calls use `explicit_run.rs::cleanup_deferred_call_body` and
`exit_block_scope` for the same body-before-defer resource ordering while
preserving escaping handles and the scope that registers each deferred action.
Parameter defaults share the function's resource scope. Suspended stream bodies
retain their own continuation lifecycle.

Callable result slots retain complete declared schemas through
`compact_function_return_type`. Checked or inferred signature facts take
precedence over syntactic recovery. Resolving a named record return before
field aliases are stored keeps Optional receivers distinguishable from Result
receivers when guarded postfix operations are lowered.

Callable return inference is owned by `src/sema/check/generic.rs` and
`src/sema/inference/components.rs`. Structural declaration dependencies establish
recursive groups; each group is constrained before its member schemes are
published. An omitted return keeps its principal relationship, including in
exports and recursive declarations. `CheckOutput::function_return_types` and
`CompactDeclOutput::function_return_types` retain checked body return facts for
qualified calls, indexed return kinds, lint rechecks, and annotation rendering.
No caller supplies the inferred definition's return context.

Private proc effects are owned by `src/sema/check/infer_effects.rs`. A checker
records raw execution requirements in the shared graph independently of its
checked permission budget. Declared clauses remain caller-visible bounds;
missing permissions still reject the body without truncating its required
summary. `EffectDeclarationId` combines declaring namespace and body span so
separately parsed module arenas may reuse local spans safely.
`CheckOutput::function_effect_facts` and
`CompactDeclOutput::function_effect_facts` publish effective requirements and
inference provenance. Calculated finite diagnostic summaries remain available
when a permission failure or unrelated type error prevents publishing
`SolvedTypes`; unresolved and unrestricted summaries remain unknown. This
projection permits a safe permission edit while preserving the rejected program
and its other diagnostics. Linting consumes these checked facts and uses equivalent
rechecks for opt-in private-clause removal; it has no syntax-based effect solver.

Explicit field labels share `TokenTable::label_text_at` and the parser's
`current_label_name`, `peek_label_name`, and `expect_label_name` readers. The
token reader returns owned spelling without interning; the parser interns into
its source's symbol owner. Binding parsers retain `expect_ident`, and shorthand
sites call `require_label_binding_name` before creating lexical captures. Labels
remain ordinary field Names in schemas, literals, accessors, constructor calls,
and patterns, so checking and indexed execution preserve their existing type
and key contracts. Tooling reads the same label vocabulary for safe unquoting.

Nested functional record updates retain `ArenaRecordFieldKind::Path` selectors
separately from their replacement expressions. Checked updates preserve the
complete base row in `SolvedTypes::record_updates`, including fields in open
row tails. Each leaf retains its exact projection chain, replacement source
identity and mutable version, producer flow, and original assignability
constraint index. Frozen validation checks those endpoints without re-solving
or reconstructing names. Updates lower to `BuildExprRow::RecordUpdate`; its indexed payload
verifies nonempty, disjoint static paths. Both execution routes evaluate the
base and replacements before `lowered_record_update_batch` rebuilds a private
snapshot. A path trie groups shared ancestors and uses
`lowered_record_field_mut` for copy on write, preserving untouched storage.
Local Result capture uses `ArenaExprKind::Capture`, `BuildExprRow::Capture`,
and verified `FullTag::ExprCapture` instructions. `Checker::begin_error_boundary`
and `Checker::end_error_boundary` collect errors at the nearest propagation
boundary. `eval_indexed_error_boundary_block` shares retry's lexical scope
and defer execution while preserving `StmtFlow::Return`, `Break`, and
`Continue` separately from `Propagate`. Checked cleanup propagation carries
an internal origin marker across runtime-error transport; defects and abort
never acquire that marker.
Explicit value pipeline arguments retain `ArenaExprKind::ValuePipelineCall`
with the input, ordinary call, and sole immediate hole. Full and compact checkers
bind the hole to the checked input type while checking that ordinary call.
`CompactLowerConstructProbe::lower_expr` reserves a temporary slot and emits an
existing `MatchExpr` binding before the call. Hole reads use the exact `ExprId`,
so the temporary cannot collide with a user name. Formatter and structural-tool
visitors retain the pipeline's written argument position; indexed execution and
verification use the ordinary match and call instructions.

Native test declarations retain `ArenaFunctionDef::test_declaration` and the
normal typed proc frame representation. `ArenaBlock::params` owns the source
header; preparation derives the zero or one `TestContext` frame parameter.
`Checker::collect_definitions_arena` retains declaration collision checks without
adding tests to the callable namespace. `xsht::test::discover_native_tests`
registers explicit declarations while the evaluator prepares and calls the same
verified indexed program used by scripts.


Selective retry retains `ArenaExprKind::Retry` and `BuildExprRow::Retry`, with an
optional shared pattern ID. `FullTag::ExprRetry` evaluates delays once, runs each
attempt through the ordinary block cleanup boundary, then tests the failure
without publishing bindings. `RetryStopReason` extends the existing
`TracePayload::RetryAttempt` rather than establishing another event stream.

`push_lowered_native_fmt_value` appends Path fragments directly from native
storage and converts other displayable fragments to UTF-8. Both indexed
execution routes use it for `BuildExprRow::PathFmtString`; `lower_run_arg`
uses that same row for compound process words, covering stored plans and
redirection operands. Generic command arguments and f-strings retain their
human text construction path.

Block string preparation is owned by `src/syntax/literal.rs::block_string_chunks`
and `src/syntax/parser/literals.rs::quoted_text_chunks`. Layout produces slices
into the original source, rather than rewriting a buffer of interpolation code.
Text slices decode with their original offsets; interpolation expressions parse
unchanged and shift arena and diagnostic spans back into the enclosing source.
The command-word reader consumes the same chunks while retaining shorthand
versus braced interpolation. Formatter serialization escapes a leading value
newline to avoid accidentally turning value bytes into structural layout.

`sema/constants.rs::PreparedConstants` owns lexical preparation for `const`.
Preparation includes every declaration in the configured entry and module
sources, including unused function bodies, while excluding unrelated sources
retained in a shared workspace arena. `ConstantScopeIndex` batches lexical
containment queries by source and start position; an end-position prefix index
selects the shortest containing block and excludes equal spans for parents.
The index preserves block identities and runtime-binding lookup barriers while
bounding scope lookup work by the number of active blocks and queried nodes.
`LiteralConstant` is shared with schema defaults, preserving the separate rule
that earlier immutable literal `let` bindings may supply those defaults.
`CompactDeclOutput::prepared_constants` supplies values, concrete types, and
constant origins to full checking and indexed lowering. `BuildScratch` caches
converted values by origin; `FullStore` retains a verified immutable constant
pool, including shared List/Map/Record backing and prepared regex handles.
Constant reads create no runtime initializer evaluations or parameter captures.


Bare braces reuse `ArenaExprKind::ValueBlock`. The parser's
`brace_starts_record_value` selects field-shaped literals from source tokens;
expected types never choose the grammar. Full statement positions supply Unit
consumption, and `CompactBodyProbeOutput::value_block_types` retains independent
value inference while contextual statement positions select indexed tail rows.
`Writer::write_block_contents` groups an initial identifier to preserve the
record/block distinction. `lint_lexical_block` requires checked statement
position and reparses the retained body before offering a CST prefix deletion.
Structural grep distinguishes value blocks from literal records and local Result
capture. Expression-only block replacements substitute captures at child source
spans, retaining parentheses and surrounding block trivia.

Imported module initializers use separate lexical binding maps keyed by their
original module owner. `Evaluator::indexed_module_bindings` retains private
values after the initializer scope is removed; capture hydration uses the
callee's qualified owner. Import aliases select the exposed namespace without
changing ownership. Nested imports and equally spelled private bindings therefore
cannot replace each other's captures or leak into root bindings.

`SolvedTypes::patterns` retains original pattern topology, capture identities,
checked joins, structural shape, literal values, and canonical constructor
owners. A shared immutable original receipt authenticates cold projections.
Repeated success-branch lexical visits reuse that receipt; unresolved recovery
children leave ancestor authority unavailable. Lowering transports pattern rows,
allocated capture slots, and lexical uses before cleanup, including bare terminal
identifier statements with their own statement identities.
Pattern tests retain both original arms' receipts, including the wildcard false
arm, against the single checked subject type.

`SolvedTypes::nominal_members` independently retains ground enum and error
member declarations, including unused and private members. Original registration
publishes qualified declaration identities, canonical families and members,
ordered payload fields, and facets. `checked_nominal_member` authenticates a
projection against its shared immutable original receipt. Import aliases retain
that owner; an error pattern's surface tested type stays separate from its
canonical declaration member. Cold preparation consumes these checked roots
without resolving field annotations or treating a pattern as its own declaration
authority. Non-ground declaration member receipts remain unavailable.
`checked_pattern_nominal` authenticates the input and tested roots' original
nominal owners after declaration solving and import publication. Surface error
aliases retain their original qualified family and member owners without changing
type equality. Missing provenance for a nominal root stays unavailable;
`checked_pattern_scope` separately projects the authenticated source scope.

### Static argument expansion

`sema::arguments::expand_named_arguments` exposes only checked finite record
fields and retains each source entry's index. `bind_static_arguments` resolves
those fields and ordinary arguments to callable parameter slots before runtime
lowering; absent slots continue to select ordinary defaults. Expression calls
and structured stages share these facts. Original calls and invocations retain
ordered `SolvedArgumentSource` recipes in `SolvedTypes::argument_sources`.
`lower_named_spread_call` consumes those recipes and their checked binding
without repeating expansion or changing the source argument range.
An unresolved Pure callback may instead retain its exact original invocation
in the caller's declaration scheme. Its supplied values remain in authored
order until the instance proof selects formal destinations; missing evidence
outside that original declaration scope still refuses lowering.
`lower_source_argument_values` creates source-ordered hygienic slots, saves each
spread record once, and projects its fields before the next entry.
`wrap_argument_bindings` sequences the initialization around existing call and
operation rows. Saved records retain their original scoped graph root separately
from the argument receipts; generated reads and projections have no invented
syntax identities. Selected standard native operations also consume these
recipes directly through `lower_original_native_named_call`, using the original
registry authority and checked slot/default masks. Their finite spreads create
no projection syntax nodes and repeat neither expansion nor binding.
A saved argument read cannot carry its initializer's source operation identity;
`checked_saved_argument_read` validates the original initializer receipt before
suppressing source stamping. `lower_legacy_named_spread_call` still uses temporary
projection expressions for specialized native boundaries and constructor
adapters. A separate adapter map validates those projections against the exact
record-field recipe and original saved-record receipt; temporary syntax has no
source authority. A checked interior
omission uses `LoweredCallArg::Default`, the callee's parameter index in the
existing call argument codec. `IndexedCallArguments` carries evaluated supplied
values separately from omitted indices; a supplied null remains a value.
`LoweredParamDefault` distinguishes absent, prepared constant, and expression
defaults. Callee binding installs constants and retains expression defaults in
`IndexedCallSlots::pending_defaults`. Both evaluators execute the verified
declaration prefix after capture hydration, once in parameter order. Suspended
producer frames retain pending defaults until first pull. No default expression
is evaluated in the caller or reconstructed from source syntax.

Native `ModuleCall` argument vectors retain optional expression slots in static
parameter order. `NativeArgumentValues` exposes an omitted slot as absence to
the operation's existing default accessors; a supplied null remains a value.
The checker probes finite field sets without committing flow changes, then
checks each spread operand at its original source position alongside ordinary
arguments and their expected type contexts.

Heap execution uses `FrameWork::ExpressionBoundary` with
`ExpressionBoundaryPolicy::Capture`. Normal values and empty completion wrap
in Ok; propagation consumes the nearest capture after lexical cleanup. Checked
runtime-error transport searches the current frame and then caller frames only
after callee defers finish. Ordinary lexical return and loop transfers retain
their targets. These boundaries survive producer suspension and cancellation;
recursive calls inside capture remain on the heap frame stack.

The mount usage graph's `MountUsageIndex.by_id` stores numeric mount IDs directly.
The index is used only for lookup, never serialized or traversed for presentation;
its negative sentinel is a value, and target counts and the string Set used for
cycle detection retain their textual contracts. This migration removes an
internal decimal encoding without changing graph traversal or output order.

## Rooted filesystem receivers

`FsRoot` is a concrete opaque runtime type. `FsRootValue` carries a private slot
identity and evaluator owner token; `Evaluator::fs_roots` owns the existing
confined directory handles. Receiver methods lower to indexed `ModuleCall` with
the same `RuntimeOp` IDs as the former module calls. Argument bindings retain
source evaluation order before host slots are arranged. Closing an alias clears
its shared slot; independently opened children retain their own handles.

`legacy_fs_root_method` maps removed spellings for checker diagnostics and
`lint.fs-root-receiver` only. It adds no executable module alias.
Signature CLI entries use `ArenaStmtKind::CliMain` and the ordinary proc body,
parameter, return, effect, and indexed frame machinery. They are excluded from
callable declaration tables. `sema::cli_entry::validate_cli_entry` resolves
parameter shapes and consumes `PreparedConstants` for defaults;
`RecordConstructors::cli_parser_type` retains unsigned parsing through aliases.
`modules::cli::PreparedSignatureCli` derives the existing strict schema and
parser bindings, preserving declaration order for positional arguments while
ordinary explicit schemas retain sorted order. `CompactIndexedRunPlan` carries
the prepared schema, and the evaluator parses argv before executing any driver
step, including imported module initialization. Help and usage errors use the
existing CLI stop handling before an entry frame is invoked.

`modules/cli.rs::CliDescriptorPlan` owns normalized descriptor entries and the
strict-versus-applet policy. `PreparedConstants::cli_descriptor_plan` resolves
only admitted constant data, retains declaration spans, and caches plans by
origin and policy. Full and compact checking derive their result shapes from
that plan. `ModuleFnSig.semantic_rule` identifies this descriptor relation.
`BuildExprRow::ModuleCall::cli_plan` retains the same Arc in a verified indexed
plan pool, so execution does not normalize static descriptors again. Builder
checkpoints rewind this pool together with speculative instructions. Ordinary
argument entries still evaluate once in source order; missing slots remain
separate from supplied null values. Dynamic descriptors use the same normalizer
at the runtime boundary.
Record projection and Boolean alias provenance is owned by
`src/sema/check/proof.rs::BindingProof` and `ConditionNarrowings`. Full and compact
checkers share subject identities, bounded mutation stamps, path overlap rules,
and continuation intersections. Immutable aliases retain shared proof sets;
`condition_proofs` records when predicates were checked so later mutations cannot
revive stale evidence. Both routes publish precise expression types and proved
Optional fallback receivers. Indexed lowering reads those facts and inserts no
casts or runtime proof checks.
`SolvedTypes::checked_refined_read` retains the original null predicate,
immutable Boolean aliases, exiting guard, subject binding and projection path
for a refined read. Its sealed receipt preserves the invariant binding type
separately from the narrowed read type and records accepted disjoint writes.
`full/mutable_prepare.rs::prepare_mutable_refinements` checks that relationship
against the actual prepared statements before the frontend is discarded.
Removed compatibility vocabulary has no executable registry entry or lowering
mode. `Checker::removed_compatibility_name` records fatal diagnostics and exact
edits after ordinary name/receiver resolution; the parser recovers canonical run
heads with fatal diagnostics. `migration_lint_code` and
`migrate_workspace_syntax` combine those edits across each loaded source and
validate the entire overlay with ordinary preparation before publishing changes.
### Cwd and environment expression boundaries

`ArenaExprKind::ContextScope` retains the input, body, and whether the body is
consumed as a value. The checked type is `Result[T, Error]`; lowering emits
`BuildExprRow::ContextScope` with ordinary tail-value rows. Both indexed routes
enter evaluator state once, finish scoped cleanup, and restore before wrapping
normal completion. `ExpressionBoundaryPolicy::Scope` unwinds transparently for
lexical transfers and propagation. Suspended producers retain
`ScopedProducerContext` while their frame owns a scope boundary; pulls and
cancellation swap it with the consumer context, including delegated children.

Inferred `.require()` targets are checked schema facts.
`sema/check/expected.rs::RequirementTarget` preserves the concrete validation
type and named application identities; compact body facts carry that target
into `runtime/eval/lower.rs`. Both explicit and inferred forms intern the same
`PreparedSchema` and emit the existing validation row. Indexed execution needs
neither annotations nor frontend arenas to validate the receiver.
`Type::has_unsigned_constraint` identifies typed storage and callable boundaries
whose Int representation must remain nonnegative. `lower.rs` retains checks on
assignment rows and primitive parameters. `BuildExprRow::CheckedValue` guards
collection and branch creation, concrete builtin results, nominal constructor
payloads, and receiver arguments using the instantiated
canonical method signature. Receiver and operand bindings retain source order
before these guards run; precise checked return types remain
in the indexed semantic signature. `checked_indexed_assignment` and
`checked_lowered_return_value` validate before storage or outward return.
Checked assignments disable consuming receiver shortcuts until validation
succeeds, so failure preserves the current root and shared aliases.
`ScriptProducer` retains the checked item type; `stream.rs::pull_script_state`
validates each reached item against its producer and retained delegation ancestors.
It uses the existing iterative cancellation path after failure, preserving child
before parent cleanup without buffering or materializing future output.
