# Architecture

XSH has one compiler-style execution path. The parser builds an arena program
and a lossless concrete syntax tree (CST); the checker publishes semantic facts;
lowering builds an indexed program; and the verifier checks it before the
evaluator can run it. The CST serves formatting and source-preserving tools;
execution uses only the verified indexed program.

[`SPEC.md`](SPEC.md) defines language behavior. This page maps the major
components and their invariants. Syntax rationale is in [`DESIGN.md`](DESIGN.md),
build and test procedures in [`TESTING.md`](TESTING.md), and tool behavior in
[`XSHT.md`](XSHT.md) and [`XSHT-FMT.md`](XSHT-FMT.md).

## Packages

| Package | Owns |
|---|---|
| `xsh` | The `libxsh` library, `xsh` executable, and integration-test helper process. |
| `crates/xsht` | The `xsht` tooling binary: checking, formatting, linting, tests, API docs, tracing, search, refactoring, and grammar tools. |
| `crates/xshi` | The interactive shell. |
| `crates/xsh-registry` | Standard-module signatures, runtime operation IDs, API documentation, and examples. |
| `crates/xsh-net` | DNS, TCP, TLS, redirects, response limits, and network error classification. |
| `crates/xsh-root` | Kernel-enforced rooted file access on Linux and macOS. |
| `crates/xsh-fuzz` | The soundness fuzzer and its reference evaluator and sandboxed runner. |
| `crates/xsh-applets` | Native applet support, including `mdev`. |

A subsystem belongs in its own crate only when its stable Rust boundary does
not depend on XSH source spans, values, diagnostics, or evaluator state.
`xsh-net` is the model: the XSH adapter converts records to plain Rust requests
and translates results and errors back, while spans, mocks, and evaluator state
stay on the XSH side.

## The `libxsh` façade

First-party library consumers use the root crate's public modules:

| Concern | Public module | Implementation owner |
|---|---|---|
| Loading, syntax, and checking | `xsh::frontend` | `src/frontend.rs`, `src/loader.rs`, `src/syntax/`, `src/sema/` |
| Diagnostics | `xsh::diagnostic` | `src/diagnostic.rs` |
| Script execution and values | `xsh::execution` | `src/execution.rs`, `src/runner.rs`, `src/runtime/` |
| Process lifecycle | `xsh::process` | `src/process.rs`, `src/runtime/process.rs` |
| Structured traces | `xsh::trace` | `src/trace.rs` |
| Reusable host adapters | `xsh::host` | `src/runtime/eval/modules/host.rs` |

Script execution, source and diagnostic data, and structured traces are the
supported library tier. Frontend, evaluator, value, and process types serve
first-party tooling and remain coupled to the implementation. The compiler,
runtime, syntax, semantic checker, and standard modules stay private behind the
façade. `tests/libxsh_api.rs` protects this boundary. `libxsh` remains the root
Rust library; a separate core crate or foreign-function ABI needs a concrete
consumer.

Language and command behavior lives in XSH libraries under `core/lib/`. Rust
provides host operations such as syscalls, descriptors, codecs, and byte-level
work. A native performance exception needs an XSH implementation and
measurements that identify the smallest operation needing acceleration.

Module search order belongs to `src/loader.rs`: beside the importing file,
then `XSH_MODULE_PATH`, then project module roots. `src/project.rs` reads the
nearest `xsht-config.ini` to find those roots. Loaders share this resolution
order; the `xsh` executable does not read the rest of the `xsht` configuration.

## Pipeline

| Stage | Main owner |
|---|---|
| Grammar and syntax tables | `src/syntax/grammar.rs` |
| Lexing | `src/syntax/lexer.rs`, `src/syntax/token.rs` |
| CST and arena program | `src/syntax/cst.rs`, `src/syntax/parser.rs`, `src/syntax/arena.rs` |
| Module loading | `src/loader.rs`, `src/project.rs` |
| Semantic checking and published facts | `src/sema/check.rs`, `src/sema/check/`, `src/sema/check/module_driver.rs` |
| Lowering | `src/runtime/eval/lower.rs` |
| Indexed schema and verification | `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/indexed/full.rs` |
| Execution and stage scheduling | `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/` |

**Grammar and syntax.** `src/syntax/grammar.rs` owns syntax productions and the
tables shared by the lexer and parser. The parser writes typed rows into
`ArenaProgram`; the CST preserves source structure for tools and is not an
execution representation. A sugar form keeps its source operands beside its
core expansion: semantic passes use the expansion; tools that preserve or
search the written surface form use the operands.

**Checking.** `Checker` owns type and signature decisions and validates import
bindings. It publishes the facts required by lowering and tooling, including
selected overloads, argument bindings, expression and binding types, and
effects. Later stages consume these facts instead of repeating semantic
decisions. `CheckSession` retains the immutable workspace identities and shares
prepared module records across entry-root views. Interactive command checks use
their own module records.

**Lowering and verification.** `FullBuilder` turns the checked arena program
into indexed columns and pools. A construction gap produces a diagnostic, not
a runnable placeholder. `FullVerifier::verify` checks the finalized
representation's schemas, ranges, ownership, IDs, and references before a
`FullProgram` exists. The instruction schema owns tag metadata, payload codecs,
views, and execution strategy. Verification derives the per-instruction marks
that admit in-place evaluation up to a nesting height of 128 and installs them
on the program; deeper instructions use the explicit frame scheduler. Runtime
decoders rely on that verification contract.

**Execution.** The indexed executor runs functions and top-level driver steps
from `FullProgram` views. Verifier-marked instructions may use the bounded
in-place route; other instructions use heap-backed frames and explicit
continuations. A loaded program stays alive while its calls or suspended
producers use it; the parser, checker, and builder are not the runtime
representation.

**Program data.** Indexed IDs refer to program-owned tables, and variable-sized
data lives in shared payload ranges. Dynamic names are owned by `SymbolOwner`;
they are not process-lifetime string references. The finalized program does
not retain the CST, arena, or checker output.

**Standard library.** `src/stdlib.rs` embeds the script-backed modules at build
time, so an installed XSH does not read a stdlib tree at runtime. The loader
selects required modules from source use, checks them with the program, and
lowers them through the ordinary indexed path. Registry entries can also bind
directly to native runtime operations.

## Invariants

1. **Verify before execute.** Only a `FullProgram` accepted by
   `FullVerifier::verify` can be installed. Unsupported or malformed indexed
   data fails preparation.
2. **One decision pipeline; routes preserve parity.** The checker makes each
   semantic decision once. Routes that must agree, such as full and compact
   checking or in-place and frame execution for eligible instructions, share
   an implementation or are covered by parity tests. Lowering consumes checked
   facts; it does not re-check bodies, select overloads, or bind arguments.
3. **Preserve source-visible behavior.** Lowering and fast paths retain
   evaluation order, error kinds and spans, cleanup, status, and trace events.
4. **Keep one meaning for each construct.** Syntax tables have one owner; sugar
   has one core expansion.
5. **Keep host effects at their boundary.** Process and signal operations stay
   in the host substrate; the evaluator decides how their structured outcomes
   affect XSH control flow.

## Adding a language feature

1. Define observable behavior in the SPEC template and snippets, then render
   generated docs as described in [`TESTING.md`](TESTING.md).
2. Add grammar, an arena variant, and parsing under `src/syntax/`. Preserve the
   CST and add the node's logical children to the exhaustive `AstArena` child
   enumerator. Update each owning traversal for binding order, callable edges,
   command boundaries, and scope transitions; a generic child walk cannot
   preserve those rules.
3. Check the behavior in `src/sema/check/` and publish any facts later stages
   need.
4. If the feature has an indexed representation, add its row to
   `instruction_schema!` in `src/runtime/eval/indexed/full/schema.rs`. The row
   generates its tag, view, and payload codecs; choose its in-place class and
   frame strategy. Implement lowering and any `FullVerifier` checks required
   by the representation's payload invariants. Add behavior-specific arms for
   every compiler-reported `ArenaExprKind`, `ExprView`, and `ExprViewKind`
   match. Add a `compact_expr_kind_catalog!` row when the expression needs a
   compact blocker category; the catalog owns its indices and labels and
   derives their count.
5. Update formatting and linting in `crates/xsht/`, and add focused native
   behavior and verifier coverage. Use the staged library-check sequence in
   [`TESTING.md`](TESTING.md) to find missing compiler arms, then run the
   relevant behavior and verifier gates.

## Adding a sugar form

A form is sugar only when an existing core construct supplies its complete
meaning. The checker and runtime must not special-case the surface spelling.
A form that declares a name in the enclosing scope, such as `guard let`, needs
to remain a core statement because declaration discovery does not inspect
inside sugar.

The parser stores both the source operands and the core expansion in the arena.
Semantic passes follow the expansion; tools that preserve or search the written
surface form, including formatter, grep, and spelling-based lint, follow the
operands. `xsht desugar` renders the expansion. Add one parser expansion,
preserve source spans for diagnostics, and cover it with a hand-written core
equivalent and the desugared-corpus parity check in `crates/xsht/`.

## Adding a validated type

A validated type is a base type plus a property tracked by the checker. The
mechanism lives in `src/sema/validated.rs`; its rules must stay consistent
across checking, constants, indexed type storage, and runtime tests.

- A validated value may widen to its base, but a base value does not widen to
  the validated type. This widening applies at stored positions in containers;
  those positions are otherwise invariant.
- Ordinary operations see the base type. Only constructors and checks that
  establish the property produce a validated type.
- The verifier checks that the indexed type and schema use a valid base and
  validation. Runtime checks use the matching validator in
  `src/runtime/eval/validated.rs`.
- For a property determined by the value alone, share one predicate across
  literal, constant, and runtime checks. A declaration-identity type such as a
  nominal type instead gets its identity from its declaration.

Adding a validation variant requires defining its base, display and error
forms, static literal and constant acceptance, runtime acceptance, and any
registry methods it guarantees or operations that preserve the property. Add
its spelling to the type-expression resolvers and indexed type representation
as needed.

## Runtime design

The evaluator owns values, scopes, result propagation, defers, handles, and
trace parentage. The process substrate (`src/runtime/process.rs`,
`src/runtime/run.rs`) owns process creation, redirections, process groups,
terminal handoff, waits, cancellation, and detached reaping. The signal
substrate (`src/runtime/signal.rs`) records signals in the handler; the
evaluator interprets them at checkpoints. Script code never runs in a signal
handler, and process groups are the unit of cancellation.

Post-fork setup applies a precomputed child plan using only async-signal-safe
operations: descriptor and signal setup; process-group and session changes;
namespace, mount, root/cwd, and credential setup; then `exec`. It performs no
allocation or locking. Internal descriptors are close-on-exec.

Process operations return structured outcomes. The evaluator decides status,
`Result` propagation, and runtime errors. Trace events retain structured argv,
cwd, environment, handle, status, and error data alongside source spans and
parent IDs.

Resources belong to their creating scope until a value transfers them to an
enclosing scope. Scope exit stops owned activities before user defers; roots
and locks remain usable during defers and are released afterward. Explicit
`close` and `unlock` calls preserve their result, while automatic cleanup does
not replace the script's result with a release error.

Network jobs cross into `NetRuntimeOwner` as plain Rust requests. The network
worker does not run XSH code or own evaluator values, source spans, or trace
buffers. Job ownership and trace events stay with the evaluator.

`IndexedSerialPipeline` owns item-at-a-time pulls. For-loop sources use it when
no materialized or terminal boundary applies. Ordinary value pipelines use
`IndexedStageEngine` to join serial groups with `IndexedMaterializedStage`,
which handles stages that materialize input and keyed work. `par-map` is the
worker boundary. Stages resume through evaluator continuations. Yield
delegation and cancellation remain iterative so producer depth does not become
native stack depth.

A new host integration names its owner, checkpoint behavior, cleanup
responsibility, signal interaction, result shape, and trace evidence before it
adds API surface.
