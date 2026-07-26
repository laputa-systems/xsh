# Architecture

XSH is implemented as a small compiler-style pipeline around a tree-walking
runtime:

1. `src/syntax` turns source text into the AST and lossless CST.
2. `src/sema` checks names, types, standard-module signatures, and lint rules.
3. `src/runtime` evaluates checked ASTs, runs host processes, manages cwd/env,
   and records the runtime graph as trace events.
4. `src/source.rs`, `src/diagnostic.rs`, and `src/trace.rs` provide shared
   source maps, spans, diagnostics, trace events, runtime graph payloads, and
   tracebacks across those stages.
5. `src/loader.rs` owns entry source ingestion, script/module loading, and the
   checked program bundle used by runtime and tooling. `src/runner.rs` owns
   plain script execution for `xsh`, and `crates/xsht/src/cli/mod.rs` wires the
   `xsht` tooling commands.

The workspace is split where a subsystem can have a stable Rust boundary
without depending on XSH source spans, runtime values, diagnostics, or evaluator
state. `crates/xsh-net` owns DNS resolution, HTTP/HTTPS transport, TLS
configuration, connection pools, redirects, and network error classification.
The main `xsh` crate keeps the language-facing adapters in `src/modules/dns.rs`,
`src/modules/net.rs`, and `src/runtime/eval/modules/net.rs`: those adapters
translate records and paths into plain Rust request structs, convert crate
results back into `Value`/`RuntimeError`, preserve source spans, honor test
mocks, and manage evaluator-owned pool state.

There is no bytecode format, instruction VM, JIT, green-thread scheduler, or
async task runtime in the execution path. The normalized AST remains the
executable representation for the full language: syntax desugaring rewrites
surface conveniences into simpler AST forms, the checker validates that tree,
and `src/runtime/eval.rs` walks it directly while coordinating host processes,
streams, cwd/env state, defers, signals, and trace events. A small lowered IR
exists only as an optional fast path for checked pure/effect-free regions that
stay inside a conservative value and control-flow subset. Unsupported functions
fall back to AST evaluation. This is intentional. XSH's performance boundary is the
Unix orchestration boundary, where most expensive work belongs in external
processes or focused host helpers, not in a hidden application runtime inside
the language. Runtime changes should preserve source-visible order, explicit
boundaries, and traceable failure paths before pursuing cleverness. The lowered
IR now also includes eligible restricted proc bodies and a statement-granular
top-level script cache for effect-free glue regions, but the AST remains the
semantic source of truth and unsupported statements still fall back
independently.

`docs/SPEC.md` is the language contract. `docs/SPEC-TYPING.md` covers
typechecking, `docs/SPEC-INTERACTIVE.md` covers `xshi`, and
`docs/SPEC-OS.md` covers OS-facing runtime behavior such as process groups,
signals, cancellation, and signal hooks. `docs/XSH-GUIDE.md` shows the intended
script shape, including target examples that may be ahead of the implementation.
`docs/FRONTEND.md` is the implementation guide for the compact frontend,
lowered IR plumbing, symbol identity, registry invariants, and benchmark
verification. `docs/COVERAGE.md` tracks the practical coverage plan for areas
that need larger harnesses rather than branch-only tests.

## Agent Routing

`docs/AGENT-ROUTING.md` is the compact task router. Use it before deep reading.
This table is the owner-module summary:

| Task | Owner docs | Owner code |
|---|---|---|
| syntax and formatting | `docs/SPEC.md` sections 2-9 | `src/syntax/*` |
| checking, typing, linting | `docs/SPEC-TYPING.md` | `src/sema/*` |
| runtime evaluation | `docs/SPEC.md` relevant section | `src/runtime/*` |
| process, cwd, env, signals, cancellation | `docs/SPEC-OS.md` | `src/runtime/run.rs`, `src/runtime/process.rs`, `src/runtime/cwd.rs` |
| standard modules and methods | `docs/STDLIB.md`, `src/modules/README.md` | `crates/xsh-registry/src/signature/*`, `src/modules/*`, `src/runtime/eval/modules.rs`, `src/runtime/eval/methods.rs` |
| structured streams | `docs/STREAMS.md` | `src/sema/check/stream.rs`, `src/runtime/eval/stream.rs` |
| indexed executable IR | `docs/FRONTEND.md` | `src/runtime/eval/indexed.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lowered_run.rs` |
| docs and examples | `docs/GENERATED-DOCS.md`, `docs-src/README.md` | `src/docs.rs`, `docs-src/*`, `examples/*` |

## Agent Map For IR Work

Use this path when changing interpreter-speed behavior:

1. Read `docs/SPEC.md` for source-visible semantics and `docs/FRONTEND.md` for the
   lowered fast-path contract.
2. Inspect `src/syntax/arena.rs` for the arena node or type shape, and
   `src/syntax/node.rs` for shared leaf syntax such as operators and type
   expressions.
3. Inspect `src/sema/check.rs` and `src/modules/signature.rs` for checked
   signatures and method/module operation IDs.
4. Inspect the normal runtime behavior in `src/runtime/eval.rs`,
   `src/runtime/eval/methods.rs`, `src/runtime/eval/modules.rs`, or
   `src/runtime/eval/stmt.rs`.
5. Add executable support only when it has an exhaustive indexed encoding,
   verifier coverage, and exact runtime behavior. Stateful or OS-facing work is
   represented by an explicit host/runtime operation referenced by indexed IR.
6. Update `tools/xsh-ir-coverage.xsh` for expansion coverage and add a
   user-visible workload to `crates/xsh-multicall/benches/bench.rs` only when
   the change affects an interaction users actually wait for.

The indexed IR is not a new language layer. It is the verified executable
representation derived from checked arena syntax after definitions are known.
Imported user modules are part of complete-program admission. Process forms,
stateful module calls, tracing-sensitive execution, and OS effects remain
explicit runtime boundaries in the indexed driver rather than reasons to retain
the source arena.

## Syntax

`src/source.rs` assigns source IDs, tracks UTF-8 source files, maps byte-offset
spans to line/column locations, and exposes original span text for diagnostics
and traces.

`src/syntax/lexer.rs` produces tokens, `parser.rs` builds `ArenaProgram` values
through `src/syntax/arena.rs`, and `cst.rs` retains lossless token/trivia
structure for tooling. Arena nodes carry `Span` values from `src/source.rs`, and
`ArenaParseOutput` carries both the arena program and CST. The active formatter
lives in `crates/xsht/src/format.rs`. Parser changes should usually come with
formatter and syntax fixture coverage so new syntax round-trips.

`docs/XSHT.md` describes the tooling architecture in more detail: command
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

`src/sema/check.rs` owns the main checker state: lexical scopes, function
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

`src/runtime/eval.rs` owns the evaluator state: scopes, indexed program,
stdout/stderr capture, cwd, env, last process status, trace events, call stack,
pending traceback, and stream item context.

Focused runtime behavior lives beside it:

- `src/runtime/eval/stream.rs` evaluates structured pipelines and parallel
  stream stages.
- `src/runtime/eval/modules.rs` dispatches standard-module calls that still
  need evaluator state.
- `src/runtime/process.rs` owns process invocation, redirection, argv/env
  conversion, and cancellation signals.
- `src/runtime/run.rs` executes `run` forms.
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
execution. The arena evaluator remains available only through the
`native-tests` force mode as a differential oracle. See `docs/FRONTEND.md`
before adding instructions, runtime operations, value kinds, or execution
shortcuts.

## Interactive

`xshi` has session builtins for shell state such as `cd`, `set`, aliases, jobs,
and listings. Core utility names are ordinary PATH commands, including
XSH-authored scripts under `core/` when that directory is on PATH. The
interactive shell-subset frontend lowers external commands to normal process
execution; it does not use a hidden compatibility-builtin registry or sudo shim.

## Tracing And Errors

`src/trace.rs` defines trace events, payloads, and traceback data. Together
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
