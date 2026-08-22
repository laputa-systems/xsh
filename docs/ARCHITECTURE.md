# Architecture

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
state. `crates/xsh-net` owns DNS resolution, XSH's capability-resolved TCP
dialer, TLS configuration, redirects, body limits, and network error
classification. `h12tiny-client` owns HTTP framing, TLS handshakes, ALPN,
protocol selection, and its bounded connection pools. The main `xsh` crate keeps
the language-facing adapters in `src/modules/dns.rs`,
`src/modules/net.rs`, and `src/runtime/eval/modules/net.rs`: those adapters
translate records and paths into plain Rust request structs, convert crate
results back into `Value`/`RuntimeError`, preserve source spans, honor test
mocks, and manage evaluator-owned pool state.

`net.request`, `net.download`, and `net.upload` are blocking host calls backed
by a persistent HTTP/1.1 h12 client in each evaluator-owned named pool.
`net.request_many` and `net.download_many` are bounded transport capabilities:
the former buffers response bodies, while the latter streams directly to caller
destinations. Each batch owns a fresh h12 client and can select HTTP/2 only when
HTTPS ALPN negotiates `h2`; otherwise it uses HTTP/1.1. XSH supplies its
nonblocking capability-resolved streams through h12tiny's TCP dialer hook, then
drives the client on the host-call executor without exposing futures, callbacks,
`await`, a process-wide event loop, or evaluator worker threads. Tokio,
`hyper-util`, and `hyper-rustls` are intentionally absent from this boundary.
The relevant grep targets are `request_many`, `download_many`, `h12_client`,
`CapTcpDialer`, `native_xsh_net_single_calls_force_https_http1`, and
`net_module_request_many_negotiates_local_https_http2`.

There is no JIT, green-thread scheduler, or async task runtime in the execution
path. The checked arena is lowered into a compact verified indexed store before
execution. `src/runtime/eval.rs` and its focused runtime modules execute borrowed
instruction and driver payloads while coordinating host processes, streams,
cwd/env state, defers, signals, and trace events. Process forms and other
OS-facing operations remain explicit indexed host-operation boundaries. The
normal script runner and native-test harness execute the same verified indexed
representation. There is no arena execution mode or compatibility interpreter.
Runtime changes should preserve source-visible order, explicit boundaries, and
traceable failure paths before pursuing cleverness.

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
verification. `../FRONTEND-FOLLOWUPS.md` records evidence-based performance and
memory work that remains after the architecture closeout. `docs/COVERAGE.md`
tracks the practical coverage plan for areas that need larger harnesses rather
than branch-only tests.

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

**Block parameter conventions.** XSH has two syntactic positions for block
parameters — inside the block (`{ |x| ... }`) for stream stages and lambdas,
and before the block (`|e| { ... }`) for `else` clauses in `with` and
`guard let`. These are documented in `docs/SPEC.md` §8. The parser's
`parse_block()` reads params as the first token inside `{`, so `else`-clause
params must be extracted before calling `parse_block()` (as `parse_with` and
`parse_guard` do).

## Semantics

`Checker` in `src/sema/check.rs` owns the main checker state: lexical scopes, function
signatures, imported modules, current return type, purity context, `$?`
availability, and stream item context.

Focused semantic rules live beside it:

- `src/modules/signature.rs` declares the standard API registry used by the
  checker and runtime. The `RuntimeOp` enum here names every method and module
  function dispatched at runtime. Deprecated APIs should be removed from both
  this registry and `runtime/eval.rs` — no traversal files need touching.
- `src/modules` contains shared host helpers for standard modules.
- `src/sema/records.rs` contains shared record schemas.
- `src/sema/check/stream.rs` checks structured stream pipelines.
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

`xshi` has session builtins for shell state such as `cd`, `set`, aliases, jobs,
and listings. Core utility names are ordinary PATH commands, including
XSH-authored scripts under `core/` when that directory is on PATH. The
interactive shell-subset frontend lowers external commands to normal process
execution; it does not use a hidden compatibility-builtin registry or sudo shim.

## Tracing And Errors

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
