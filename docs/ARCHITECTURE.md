# Architecture

XSH is a compiler-style pipeline around one verified executable form. Source is
parsed once into a compact arena and a lossless CST, checked once, and the
checker publishes facts that every later consumer reads: lowering, the
formatter, lint, and the other `xsht` tools. Lowering commits a verified indexed
program, and a heap-frame executor runs it. There is no second frontend, no
alternate executable representation, and no fallback interpreter.

`docs/SPEC.md` is the language contract. This document is the contributor map:
where each stage lives, the invariants that hold the stages together, and the
checklist for changing them. Search for the exact symbol names below; each is a
retrieval handle for its owner file. Testing lives in `docs/TESTING.md`,
tooling in `docs/XSHT.md`, formatter policy in `docs/XSHT-FMT.md`.

## Packages

| Package | Owns |
|---|---|
| `xsh` (root) | the `libxsh` library, the `xsh` binary, the `xsh-test-*` helper binaries, and the `xsh-frontend-stats`/`xsh-runtime-stats` profiling binaries |
| `crates/xsht` | the `xsht` tooling binary: check, fmt, lint, test, api, trace, grep, refactor, grammar |
| `crates/xshi` | the `xshi` interactive shell |
| `crates/xsh-registry` | standard-module signatures, records, API docs, examples, language reference items, and runtime operation IDs |
| `crates/xsh-net` | DNS, the resolved TCP dialer, TLS, redirects, body limits, and network error classification |
| `crates/xsh-root` | kernel-enforced rooted file opening (Linux and macOS) |
| `crates/xsh-fuzz` | the soundness fuzzer: a seeded generator of well-typed programs, an independent reference evaluator, registry-derived call probes, corpus mutation, a sandboxed runner, and failure shrinking (`make fuzz`) |
| `crates/xsh-applets` | native applet support (`mdev`) |

A subsystem gets its own crate only when it has a stable Rust boundary that does
not depend on XSH source spans, runtime values, diagnostics, or evaluator state.
`xsh-net` is the model: `src/modules/net.rs` and `src/runtime/eval/modules/net.rs`
translate XSH records into plain Rust requests and translate results back into
`Value`/`RuntimeError`, keeping spans, test mocks, and evaluator state on the
XSH side.

### The `libxsh` façade

First-party consumers import the root library only through these modules:

| Concern | Path | Owner |
|---|---|---|
| loading, syntax, checking | `xsh::frontend::{load, syntax, check, source}` | `src/frontend.rs` over `src/loader.rs`, `src/syntax`, `src/sema`, `src/source.rs` |
| diagnostics | `xsh::diagnostic` | `src/diagnostic.rs` |
| script execution | `xsh::execution::script` | `src/execution.rs` over `src/runner.rs` |
| evaluator and values | `xsh::execution::{evaluator, value}` | `src/runtime/eval.rs`, `src/runtime/value.rs` |
| process lifecycle | `xsh::process` | `src/process.rs` over `src/runtime/process.rs` |
| structured traces | `xsh::trace::model` | `src/trace.rs` |
| reusable host adapters | `xsh::host` | `src/lib.rs` |

Script execution, source/diagnostic data, and structured traces are the
supported tier. Frontend, evaluator, value, and process types are first-party
tooling APIs whose representation is still coupled to the compiler. The
implementation roots (`runtime`, `sema`, `syntax`, `modules`, `runner`) are
private. Trace data belongs to `libxsh`; trace presentation belongs to
`xsht`. The library stays a static Rust library: no `cdylib`, and no split into
a separate core crate without a concrete consumer. `tests/libxsh_api.rs` guards
the façade.

## Pipeline

| Stage | Primary objects | Owner |
|---|---|---|
| grammar | `grammar()`, `BINARY_OPERATORS`, `STATEMENT_KEYWORDS`, `QUOTED_LITERALS`, `STREAM_STAGES`, `RUN_FORMS`, `line_continuation`, `grammar_tokens` | `src/syntax/grammar.rs` (productions in `src/syntax/grammar/productions.rs`) |
| lex | `Lexer::lex_compact`, `TokenTable` | `src/syntax/lexer.rs`, `src/syntax/token.rs` |
| source structure | `SyntaxTree::from_token_table` (CST), `Parser::parse_source_arena_only`, `ArenaProgram`, `AstArena` | `src/syntax/cst.rs`, `src/syntax/parser.rs`, `src/syntax/arena.rs` |
| load | `CompactFileUnit`, `CompactModuleGraph`, `CheckedEntry` | `src/loader.rs` |
| check | `Checker`, `Checker::check_compact_declarations`, `CheckOutput`, `CompactDeclOutput` | `src/sema/check.rs`, `src/sema/check/compact.rs` |
| publish facts | `CompactBodyFacts`, `CheckedApiCall`, `CheckedArguments`, `PreparedConstants` | `src/sema/check/compact.rs`, `src/sema/constants.rs` |
| lower | `FullBuilder::build_compact`, `BuildScratch` | `src/runtime/eval/lower.rs` |
| verify | `FullVerifier::verify`, `FullStore`, `FullProgram` | `src/runtime/eval/indexed/full.rs` |
| execute | `Evaluator::prepare_compact_indexed_only`, `indexed_run`, `CallFrame`, `FrameWork` | `src/runtime/eval.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs` |

**Grammar.** `src/syntax/grammar.rs` is the one definition of the syntax:
productions over the lexer's tokens plus the tables the lexer and parser
dispatch on (operators with precedence, associativity, and line continuation;
statement and primary keywords; quoted-literal prefixes; stream stages; run
forms and options; builder APIs). The parser keeps its own recursive descent,
recovery, and diagnostics but reads every such table from the grammar.
`make docs` renders the productions as `docs/reference/grammar.md` through
`xsht grammar --format json`. `grammar::earley` recognizes token streams
against the productions and `grammar::generate` produces sentences from them;
the grammar tests use both to hold the productions and the parser to the same
language (`docs/TESTING.md`).

**Syntax.** The lexer produces columnar token tags and starts; source text stays
the authority for token ends and spelling. The parser writes typed rows
(`StmtId`, `ExprId`, `PatternId`, `BlockId`) directly into `ArenaProgramBuilder`;
variable payloads live in side tables and ranges. The CST serves formatting and
source-preserving edits and is never executed. Do not add a recursive AST or a
CST-to-AST bridge for convenience. The parser decides language shape; later
stages do not recover from ambiguous trees the parser could have represented.

**Checking.** `Checker` owns lexical scopes, signatures, imports, return and
purity context, and stream item context. Focused rules live beside it:
`src/sema/constraints.rs::TypeConstraints` (bounded monomorphic inference),
`src/sema/check/infer_return.rs` and `src/sema/check/infer_effects.rs` (private
return and effect inference), `src/sema/check/proof.rs` (narrowing provenance),
`src/sema/check/stream.rs` (pipeline stage facts), and `src/sema/arguments.rs`
(static argument binding). Registry signatures from `crates/xsh-registry` are
adapted to semantic types in `src/modules/signature.rs`. The checker reports a
diagnostic and continues with an internal recovery type; public dynamic data is
`Type::Any`, and recovery types never leak into signatures or docs.

**Facts.** The checker publishes everything later stages need:
expression and binding types, the selected overload and argument slots for every
registered call (`CheckedApiCall`), argument bindings for user calls and stages
(`CheckedArguments`), statement positions, function return and effect facts,
prepared constants, and terminating calls. `CompactBodyFacts` re-keys them by
arena identity. Lowering, lint, and annotation all consume these facts instead
of re-deriving them.

**Lowering and verification.** `FullBuilder::build_compact` reserves function
identities, lowers each checked body into short-lived construction scratch,
encodes it into indexed columns, and builds the root driver only when the whole
program is representable. Checkpoints rewind every column on failure, so
unsupported behavior becomes a diagnostic and never a runnable placeholder.
`FullVerifier::verify` checks tag/data schemas, ranges, ownership, termination,
slot bounds, IDs, locations, patterns, stages, and literal and semantic pools
before a `FullProgram` exists. Runtime decoders rely on that contract.

**Execution.** `indexed_run` executes function blocks and driver ranges from
borrowed `FullProgram` views. Calls, work, and continuations live in heap-backed
frames (`CallFrame`, `FrameWork`), so XSH call depth never becomes native
stack depth; statement blocks nested in expressions run on lent block frames
over the caller's slots. After installation, parser, checker, and builder state
are dropped: only `FullProgram`, its `SourceMap`, and its `SymbolOwner` survive
execution. The runner, native tests, direct calls, module loading, auto-main,
and signal-hook setup all prepare through
`Evaluator::prepare_compact_indexed_only`.

### Program representation

`src/runtime/eval/indexed.rs` defines one-based `u32` identities (`IrFunctionId`,
`IrBlockId`, `IrStringId`, `TypeId`, `SignatureId`, `ShapeId`) with `IR_NONE` as
absence. Instructions are a one-byte tag plus eight-byte `IrData`; variable
payloads are ranges into shared tables. Hot rows hold no machine-width indexes,
recursive children, strings, or `Type` values. A finalized `FullProgram` holds no
CST, arena, or checker references. `src/runtime/eval/indexed/semantic.rs`
assigns program-owned type, signature, and shape identities and drops its
canonicalization maps at finalization.

Dynamic name spellings are owned by `SymbolOwner` in `src/symbol.rs`;
`Name::as_str()` returns `NameText`. Never claim a process-lifetime `&'static str`
for dynamic input. Dropping the last owner releases the spellings.

### Embedded standard library

A registry entry carries an `ImplBinding`: `Native` (a `RuntimeOp` body, the
default) or `Script` (a function in an embedded XSH module under `stdlib/`).
`src/stdlib.rs` embeds a fixed catalog with `include_str!`; nothing is read from
disk at run time. `stdlib::required_modules` selects modules syntactically from
the spellings a program mentions; selected modules are parsed into the same
arena under the reserved `<xsh-stdlib:IDENTITY>` namespace, checked with the
program, and called through ordinary `Call` instructions.
`every_catalog_module_parses_checks_and_lowers` validates every catalog module.
Whether an entry is native or script-backed is decided by measurement;
`bench/stdlib-port/README.md` records each disposition.

### Generated documentation

`dev/docs.xsh` renders each `docs/templates/REL` into `docs/REL` with the
`template` module (`make docs`, `cargo dev docs`), so the SPEC, the tour, and
the references come from code instead of copies of it. SPEC and tour code
blocks are the files in `docs/snippets/spec/` and `docs/snippets/tour/`
(`NN-name.xsh`; the tour also has the `project/` example with its own
`xsht-config.ini`). A snippet shows either the whole file or its
`# begin example` ... `# end example` regions, dedented, so a fragment is
checked inside a wrapper program the document leaves out. Snippets under
`rejected/` show code that must not check and name each expected diagnostic
with a `# error: CODE` comment on its line; other files in a snippet directory
are support modules the snippets import. A snippet runs, sandboxed in an empty
directory with only `PATH`, exactly when a template shows its `.output`, and a
`# platform: linux` snippet never runs, so generation is host-independent.
`docs/reference/stdlib.md`, `cli.md`, and `lints.md` are read from
`xsht api --format jsonl`, the binaries' help, and `xsht lint --list`. The
SPEC facet table is read from the `language:facet` API items, which come from
`xsh_registry::errors::ErrorFacet`, the one facet vocabulary the checker,
runtime, and built-in error families share.
`dev/docs.xsh::check` (the `check-docs` stage of `cargo dev check` and
`make check`, and `make docs-check`) re-renders and fails on any difference,
then runs `xsht check` on each snippet, requiring no diagnostics or exactly the
annotated ones, and `xsht test` in the project.

## Invariants

1. **Verify before execute.** Only a `FullProgram` that passed
   `FullVerifier::verify` is installed. A clean construction gap is a diagnostic;
   it cannot select another evaluator.
2. **One decision pipeline.** Each semantic decision (overload selection,
   argument binding, narrowing, inference, effects, constants) is made once, by
   the checker. Two routes that must agree (full and compact checking, the
   recursive and frame evaluators) share one implementation or are pinned by a
   parity test.
3. **Lowering consumes checker facts.** Lowering never checks a body again,
   selects an overload, or binds arguments. A call without a checked plan lowers
   only its positional arguments.
4. **Behavior-bearing data survives every stage.** An indexed program that drops
   a format spec, stream error, trace event, method argument, or run option is
   wrong even if it lowers.
5. **Effects stay explicit.** Processes, cwd/env, defers, signal hooks, streams,
   and host operations remain explicit instructions or driver steps. Fast paths
   may remove dispatch after verification but must preserve tracebacks, traces,
   and exact error spans.
6. **Formatter equivalence is a safety net.** `verify_formatted_output` compares
   canonical syntax walks (`crates/xsht/src/format_equivalence.rs`) of the input
   and output and refuses to write a regrouped program, for both `fmt` and
   `lint --fix`.
7. **Lint is invariant under formatting.** A file and its `xsht fmt` output
   produce the same diagnostics. Layout may decide only whether a fix is offered.
   `crates/xsht/tests/lint_format_invariance.rs` enforces this.
8. **No speculative machinery.** No JIT, green threads, async task runtime, or
   bytecode VM. Reconsider only with measured bottlenecks and only if every
   observability and OS contract stays exact.

## Adding a language feature

1. Specify it in `docs/templates/SPEC.md`, with examples in
   `docs/snippets/spec/` (first, or in the same change).
2. Add its productions, and any keyword, operator, or stage table rows, to
   `src/syntax/grammar.rs`; add arena storage and accessors in
   `src/syntax/arena.rs`, and parse it in `src/syntax/parser/` from those
   tables. Keep CST round-tripping exact, and run `make docs`.
3. Check it in `src/sema/check/` and publish whatever later stages need as a
   checked fact. Keep full and compact checking in agreement.
4. Lower it in `src/runtime/eval/lower.rs` from those facts. Add instruction
   tags and verifier rules in `src/runtime/eval/indexed/full.rs`; execute in
   `src/runtime/eval/lowered_run/indexed_run.rs` or the focused runtime owner.
5. Format it in `crates/xsht/src/format.rs`, give its operands contexts in
   `src/syntax/grouping.rs` (`child_context`, `needs_parens`, shared by the
   printer and `check.redundant-parens`), and extend
   `crates/xsht/src/format_equivalence.rs` if the canonical walk needs it.
6. There is no generic AST visitor. Update every arena/CST consumer that owns
   behavior for the surface, typically `crates/xsht/src/lint.rs`,
   `crates/xsht/src/grep.rs`, and `xsht check --annotate`.
7. Add native tests (see `docs/TESTING.md`) plus a verifier unit test for new
   instruction shapes, and update `tools/xsh-ir-coverage.xsh` if coverage
   accounting changes.

Change frame layouts, token/arena storage, or instruction encodings only with
retained-memory, RSS, latency, or stack-depth evidence from
`xsh-frontend-stats` (`src/frontend_stats.rs`) or `xsh-runtime-stats`
(`src/runtime_stats.rs`). Only those profiling binaries install
`mem_track::CountingAllocator`.

## Runtime design

The OS runtime coordinates tree-shaped evaluation with the host's graph of
processes, process groups, terminals, signals, and waits. `docs/SPEC.md` defines
what a script observes; the runtime preserves those observations when the host
interrupts, reorders, or outlives evaluation.

**Three layers, no smuggling.** Evaluation owns scopes, values, `$?`, `Result`
propagation, defers, hooks, handles, and trace parentage. The process substrate
(`src/runtime/process.rs`, `src/runtime/run.rs`) owns argv/env/cwd conversion,
redirections, process-group setup, terminal handoff, `waitpid` decoding,
timeouts, cancellation escalation, and detached reaping. The signal substrate
(`src/runtime/signal.rs`) owns handler installation, async-signal-safe recording,
and child disposition reset. The process substrate never decides XSH control
flow, handlers never inspect evaluator state, and the evaluator never calls
`waitpid` or `tcsetpgrp` directly.

**Ownership plus checkpoints.** Every process XSH starts is owned by a scope,
owned by an active wait, released to a background reaper on explicit detach, or
deliberately outside XSH's process group. Ownership answers who must reap or
cancel; checkpoints answer when a recorded signal may become XSH behavior. XSH
code never runs inside a signal handler, and shutdown is never deferred until
the script returns.

**The process group is the unit of cancellation.** A simple command gets a new
group; a byte pipeline shares one; a managed `spawn` handle owns one until
`wait`, `cancel`, lexical cleanup, or detach consumes it. Children that
double-fork, `setsid`, or create their own groups have left XSH's control.

**Signals are recorded globally and interpreted locally.** The handler records
the first primary signal and one escalation. At checkpoints the evaluator runs a
matching hook (once, with its own defers and its own process work), forwards to
active groups, cancels live handles, or skips cleanup after escalation.

**Status is data; failure is control.** The substrate returns structured
outcomes; the evaluator decides whether to set `$?`, wrap `Ok`, propagate `Err`,
or build a traceback. Cancellation policy returns `Forward`/`Escalate` decisions
rather than throwing from the wait loop.

**Lexical cleanup is ownership.** A live non-detached handle is canceled and
reaped when its owning scope exits unless it moved into a surviving value;
cleanup runs before user defers observe the completed scope. `NetJob`, stream
producers, and `par-map` workers follow the same rule. `cd`/`env` value scopes
select evaluator state without touching process-global cwd or environment and
restore it only after scope cleanup.

**Fork safety.** Once networking starts, XSH is multithreaded before it forks.
Post-fork child setup does only descriptor setup, signal reset, group/session
setup, and `exec`, with no allocation or locking. Internal descriptors are
close-on-exec.

**Networking.** Each evaluator owns at most one lazy `NetRuntimeOwner`: one
executor on a parked driver thread, bounded transport admission, and bounded
file workers. It receives only plain Rust data, never `Evaluator`, `Value`,
spans, or trace buffers, and cannot run XSH code. `NetJob` identity, ownership,
trace events, and signal decisions stay in `src/runtime/eval/net_job.rs`.

**Streams.** A `StreamValue` holds a materialized prefix plus an optional live
source or suspended script producer. Supported serial stages
(`src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs`) pull one source
item through every stage before the next, stop at bounded terminals, and
materialize before an unsupported stage. `yield @source` delegation and
cancellation are iterative
(`src/runtime/eval/lowered_run/indexed_run/producer.rs`), so delegation depth
never becomes native stack depth; child cleanup runs before parent cleanup.
Stages that need a complete input (`sort`, `collect`, `batch`) keep an explicit
materialization boundary, and `par-map` keeps its worker boundary.

**Errors and traces.** `RuntimeError` carries immutable typed causes
(`src/runtime/value/error_cause.rs`); traces snapshot them into a bounded flat
sequence (`src/trace/error_causes.rs`). `TraceEvent` and `TracePayload`
(`src/trace.rs`) are the runtime-graph projection: spans anchor nodes to source,
parent IDs preserve dynamic containment, and payloads carry structured argv,
cwd, env, handle IDs, signals, statuses, and errors rather than reconstructed
shell strings. Network events never carry bodies, headers, credentials, or URL
queries.

**Interactive.** `xshi` (`crates/xshi/src/interactive/`) uses the same process
substrate; its observable behavior matches the `ish` shell, held by differential
PTY tests. History (`crates/xshi/src/interactive/history.rs`) is its only
cross-process state.
Its session policy is part of the language reference.

A new host integration names its owner, checkpoint behavior, cleanup
responsibility, signal interaction, status/error shape, and trace evidence
before adding API surface.
