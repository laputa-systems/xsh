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

The primary retrieval path is symbol-first: search for the concrete type or
method named in this document, then open its owner file and nearest test. For
the complete frontend vocabulary, see `docs/FRONTEND.md`; use the routing
policy in `AGENTS.md` for task-specific reading and verification.

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
  continuation frames; it is the only recursive-language-call executor.
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
existing owners. Prepared constants provide contract values; a known plain
receiver and exact existing schema are required before offering an edit.

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
- `src/modules/signature.rs` declares the standard API registry used by the
  checker and runtime. The `RuntimeOp` enum here names every method and module
  function dispatched at runtime. Deprecated APIs should be removed from both
  this registry and `runtime/eval.rs` — no traversal files need touching.
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
  Concrete `record_constructor_instances` facts feed compact checking and
  lowering after inference completes. Defaults supply no inference evidence.
  Fully substituted instances reuse `Type::Record`; no runtime generic dispatch
  is introduced. `LiteralConstant` admits bounded
  literal trees and earlier immutable literal bindings; checker, parameter-default
  lowering, constructor lowering, and conservative constructor fixes share this
  analysis. Constructor lowering emits existing record and schema-check rows,
  preserving supplied field order and independent aggregate values.
- `src/sema/check/stream.rs` checks structured stream pipelines.
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

Callable result slots retain complete declared schemas through
`compact_function_return_type`. Checked or inferred signature facts take
precedence over syntactic recovery. Resolving a named record return before
field aliases are stored keeps Optional receivers distinguishable from Result
receivers when guarded postfix operations are lowered.

Private pure return inference is owned by
`src/sema/check/infer_return.rs::infer_local_pure_returns`. Declaration dependency
components are computed with iterative graph traversals; recursive unannotated
members require explicit signatures. `CheckOutput::function_return_types` and
`CompactDeclOutput::function_return_types` retain checked body return facts for
qualified calls, indexed return kinds, lint rechecks, and annotation rendering.
No caller supplies the inferred definition's return context.

Private proc effects are owned by `src/sema/check/infer_effects.rs`. A checker
collection pass records direct requirements and edges from resolved calls;
`EffectGraph::solve` computes finite transitive summaries before final bound
checking. `EffectDeclarationId` combines declaring namespace and body span so
separately parsed module arenas may reuse local spans safely.
`CheckOutput::function_effect_facts` and
`CompactDeclOutput::function_effect_facts` publish effective requirements and
inference provenance. Linting consumes these checked facts and uses equivalent
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
base schema and lower to `BuildExprRow::RecordUpdate`; its indexed payload
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

### Static argument expansion

`sema::arguments::expand_named_arguments` exposes only checked finite record
fields and retains each source entry's index. `bind_static_arguments` resolves
those fields and ordinary arguments to callable parameter slots before runtime
lowering; absent slots continue to select ordinary defaults. Expression calls
and structured stages share these facts. `lower_expanded_argument_values`
creates source-ordered hygienic slots and projects each spread before the next
entry. `wrap_argument_bindings` sequences the initialization around existing
call and operation rows. Compiler-generated projection IDs are transient and
leave the original source argument ranges and CST unchanged. A checked omitted
parameter of a loaded module call uses `LoweredCallArg::Default`, a parameter
index in the existing call argument codec. Execution selects that exact
prepared callable's immutable default; it is distinct from a caller frame slot
and introduces no runtime name binding.

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
plan pool, so execution does not normalize static descriptors again. Ordinary
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
