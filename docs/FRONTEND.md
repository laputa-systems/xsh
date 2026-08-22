# Compact Frontend and Indexed Runtime

XSH has one production frontend and one production executable representation.
Source becomes compact tokens, a lossless CST, and an `ArenaProgram`; checking
and lowering consume typed arena IDs; `FullBuilder::build_compact()` commits a
verified `FullProgram`; and `indexed_run` executes that program. The normal
path never reconstructs a recursive syntax tree or installs a second executable
program as a fallback.

This document is the durable architecture and change contract for that path.
[`FRONTEND-FOLLOWUPS.md`](../FRONTEND-FOLLOWUPS.md) records measured, non-blocking performance and memory
work. `docs/SPEC.md`, `docs/SPEC-TYPING.md`, and `docs/SPEC-OS.md` remain the
source-visible behavior contracts.

## Greppable Frontend Vocabulary

Use the following terms consistently when describing the implementation. The
name in the middle column is the search handle; the path and test in the last
column are the first places to read.

| Concept | Canonical symbols | Owner and coverage |
|---|---|---|
| compact lexical input | `Lexer::lex_compact`, `TokenTable`, `TokenTableData` | `src/syntax/lexer.rs`, `src/syntax/token.rs`; syntax fixtures in `tests/fixtures/syntax` |
| lossless source structure | `SyntaxTree::from_token_table`, `SyntaxTree`, `LazyCst` | `src/syntax/cst.rs`; formatter coverage in `tests/syntax.rs` |
| compact parsed program | `Parser::parse_source_arena_only`, `ArenaProgram`, `AstArena` | `src/syntax/parser.rs`, `src/syntax/arena.rs`; parser coverage in `tests/syntax.rs` |
| loaded module graph | `CompactFileUnit`, `CompactModuleGraph`, `parse_load_entry_source_compact_file_unit` | `src/loader.rs`; module fixtures in `tests/fixtures/runtime` |
| compact declaration checking | `Checker::check_compact_declarations`, `CompactDeclOutput` | `src/sema/check/compact.rs`; semantic coverage in `tests/sema.rs` |
| compact body probing | `Checker::probe_compact_bodies`, `CompactBodyProbe`, `check_compact_program`, `check_compact_expr` | `src/sema/check/compact.rs`; compact frontend fixtures in `tests/fixtures/frontend-indexed` |
| executable commit | `FullBuilder::build_compact`, `FullProgram`, `FullVerifier::verify` | `src/runtime/eval/lower.rs`, `src/runtime/eval/indexed/full.rs`; verifier tests under `runtime::eval::indexed::full::tests` |
| indexed execution | `Evaluator::prepare_compact_indexed_only`, `indexed_run`, `CallFrame` | `src/runtime/eval.rs`, `src/runtime/eval/lowered_run/indexed_run`; `tests/runtime/frontend_indexed.rs` and `tests/runtime/stack_depth.rs` |
| dynamic symbol ownership | `SymbolOwner`, `NameText`, `dynamic_symbol_stats` | `src/symbol.rs`; symbol lifetime tests in `src/symbol.rs::tests` |
| runtime allocation evidence | `xsh-runtime-stats`, `RuntimeAllocationStats`, `WorkerStage`, `WorkerAllocationScope` | `src/runtime_stats.rs`, `src/mem_track.rs`; `runtime_stats::tests` |

When adding a new implementation concept, document it in this table and in
the nearest owner section using the same exact spelling. Do not use a broad
description such as “the checker” when `Checker::check_compact_declarations`
or `Checker::probe_compact_bodies` is the relevant path.

## Pipeline

| Boundary | Primary objects | Owner code | Contract |
|---|---|---|---|
| lexical input | `TokenTable`, `TokenTableData` | `src/syntax/lexer.rs`, `src/syntax/token.rs` | Dense token columns keep tags, starts, and sparse payloads. Token ends and text are recovered from source when needed. |
| source structure | `SyntaxTree`, `ArenaProgram`, `AstArena` | `src/syntax/cst.rs`, `src/syntax/parser.rs`, `src/syntax/arena.rs` | The CST preserves formatting/source structure; the arena stores parser output as typed IDs and compact rows. |
| declarations and bodies | `CompactFileUnit`, `CompactModuleGraph`, `CompactDeclOutput`, `CompactBodyProbeOutput` | `src/loader.rs`, `src/sema/check/compact.rs` | Declaration and body checks consume `ArenaProgram` IDs directly. |
| executable commit | `FullBuilder`, `FullProgram`, `FullVerifier` | `src/runtime/eval/lower.rs`, `src/runtime/eval/indexed/full.rs` | A complete indexed program is encoded, finalized, and verified before installation. |
| execution | `Evaluator`, `FunctionHeader`, `indexed_run`, `CallFrame` | `src/runtime/eval.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs` | The evaluator reads verified payloads through borrowed indexed views and keeps call/control state in explicit frames. |

`ArenaParseOutput` is the parser result. `CompactFileUnit` makes one parsed
file and its source-facing metadata available without constructing runtime
state. `CompactModuleGraph` supplies resolved imports, declarations, exports,
and deterministic diagnostics before executable installation.

## Compact Source Representation

### Tokens and CST

`Lexer::lex_compact()` produces a `TokenTable` backed by shared
`TokenTableData`. Token tags and byte starts are columnar. Starts remain in a
small representation for ordinary files and promote only when a larger offset
is necessary; sparse payload rows avoid reserving payload storage for default
tokens. Source text remains the authority for token ends and spelling.

`SyntaxTree::from_token_table()` builds the lossless CST from that table. The
CST serves formatting and source-preserving tooling; it is not an executable
syntax tree. Do not add a recursive parser result or a CST-to-recursive-AST
bridge to make a consumer convenient.

### Arena Program

`Parser` writes directly into `ArenaProgramBuilder`, which finalizes an
`ArenaProgram` containing `AstArena`. IDs such as `StmtId`, `ExprId`,
`PatternId`, `BlockId`, and `RunFormId` identify rows; tags plus compact data
select their form; variable payloads live in side tables and ranges. Important
row types include `ArenaStmt`, `ArenaExpr`, `ArenaPattern`,
`ArenaTypeExprData`, and their corresponding tags in `src/syntax/arena.rs`.

Arena spans use compact byte-span storage and promote only when a file or an
interpolation needs wider offsets. Keep explicit spans where diagnostics,
cooked text, or cross-source composition require them. Do not replace spans
with token-derived recovery wholesale without retained-memory and diagnostic
evidence.

Parser APIs should return compact IDs or `ArenaProgram` directly. A consumer
that must retain type syntax should retain the owning `Arc<ArenaProgram>` plus
the ID it needs, rather than raising a recursive type-expression copy. Builder
APIs should name the compact form they create and keep rare staging state cold.

### Checking and Declaration State

`Checker::check_compact_declarations()` and
`Checker::probe_compact_bodies()` read arena rows directly. Declaration output
contains the information needed to build module/type/error metadata, while
body probing supplies checked type facts and lowering eligibility. Runtime
registration derives declaration metadata from those compact results.

Checking must not depend on an alternate syntax representation. When a compact
row gains behavior-bearing data, update every applicable checker, lowerer, and
parity test; an indexed program that silently drops a format specification,
stream error, trace event, method argument, or run option is incorrect even if
it still lowers successfully.

## Indexed Executable Representation

### Program Layout

`src/runtime/eval/indexed.rs` defines typed, one-based `u32` identities such as
`IrFunctionId`, `IrBlockId`, `IrStringId`, `IrBytesId`, `IrLocationId`,
`TypeId`, `SignatureId`, and `ShapeId`. `IR_NONE` is the reserved absent value.
Use these IDs and `IrRange` for persistent links and variable-length payloads;
do not put machine-width indexes, recursive children, strings, or `Type` values
in hot executable rows.

`FullStore` in `src/runtime/eval/indexed/full.rs` owns the finalized columns.
The common instruction representation is a one-byte tag plus eight-byte
`IrData`; compact location IDs are stored separately, and shared `u32` extra
storage carries variable payloads. Dedicated columns own blocks, patterns,
pipeline stages, functions, parameters, captures, driver steps, strings, bytes,
locations, runtime-operation metadata, and semantic pools.

`FullFunction` rows identify parameter, capture, body-block, and slot metadata.
`FullBlock` rows identify a function owner and instruction range. Each variable
sequence is a range into a shared table, not a nested allocation. A finalized
`FullProgram` is self-contained for execution: it does not retain CST, arena,
checker-output, or construction-body references.

### Semantic and Runtime Identities

`SemanticPoolBuilder` assigns program-owned IDs to executable types,
signatures, record shapes, and module shapes. Its canonical maps are build-time
state only. Finalization drops those maps, shrinks the pools, and verifies all
child IDs and ranges. Recovery-only checker facts must resolve to existing
runtime wildcard behavior before commit; `Unknown` and `Invalid` never become
executable semantic identities.

Executable `ShapeId` is program-owned. Runtime records use a separate
process-local shape identity because public/host values can outlive any one
`FullProgram`. Fixed runtime shapes store fields densely. The runtime shape
cache may retain preloaded shapes for steady-state reuse but must not retain
dropped dynamic-name shapes indefinitely.

### Dynamic Names and Sources

Preloaded `Name` spellings are static. Dynamic spellings are owned by
`SymbolOwner` in `src/symbol.rs`; `Name::as_str()` returns `NameText`, which
borrows static storage or owns a dynamic spelling as appropriate. Never claim a
process-lifetime `&'static str` for dynamic input.

`ArenaProgram` owns symbols introduced while parsing. `FullProgram` retains the
owner and its `SourceMap` so indexed execution, diagnostics, loaded modules,
workers, and runtime errors retain valid text and source identity. Dropping the
last relevant owner releases dynamic spellings instead of leaking session data.

## Commit, Verification, and Lifetime

`FullBuilder::build_compact()` is the executable commit boundary. It reserves
function identities, lowers checked compact functions into short-lived
construction bodies, encodes each admitted body into indexed columns, and
builds the root driver only when the complete program is representable. The
construction representation is scratch: it is dropped after encoding and is
not a second installed runtime.

The builder uses checkpoints around function and driver encoding. An error
rewinds every affected instruction, graph, literal, location, operation, and
semantic column. Unsupported behavior cannot become a runnable `Unit` or a
similar placeholder. Lowering blockers are structured diagnostics and coverage
data, not authority to mark an incomplete program executable.

`FullVerifier::verify()` runs before `FullProgram` is returned. It validates
tag/data schemas, ranges, instruction and block ownership, function
termination, slot bounds, IDs, locations, patterns, stages, literals, and
parameter/capture metadata. Runtime decoders may rely on this verified contract;
the verifier itself keeps checked decoding.

After a successful install, `Evaluator` retains the indexed program and drops
the parser/checker/constructor state that is no longer needed. The expected
lifetime is:

1. Source, tokens, CST, and arena exist while parsing and checking.
2. Compact declaration/body results and builder scratch exist while lowering.
3. `FullProgram`, its source map, and its `SymbolOwner` survive execution.
4. Explicit execution frames survive only active calls and control flow.

Do not keep a shadow executor, a whole-body recursive decoder, or a command
line switch that chooses arena execution. A source error or clean construction
gap is reported before installation; it cannot select another evaluator.

## Execution

`Evaluator::prepare_compact_indexed_only()` prepares the indexed program used by
the runner, native tests, direct calls, dynamic-function registration, module
loading, auto-main, and signal-hook setup. `indexed_run` executes function
blocks and driver ranges from borrowed `FullProgram` views. Shared process,
stream, filesystem, module, value-conversion, trace, and slot-frame behavior
lives in `src/runtime/eval/lowered_run.rs` rather than in a parallel expression
interpreter.

`explicit_run.rs` uses heap-backed `CallFrame`, `FrameWork`, and
`FrameContinuation` values. This makes XSH language call depth independent of
the former large native evaluation-stack reservation. Frame rows are active
runtime costs, not retained frontend program costs; change them only with
release latency, RSS, or stack-depth evidence.

Stateful and OS-facing behavior remains explicit in indexed instructions or
driver steps. Direct pure-call and verified-decoder fast paths may remove
redundant dispatch after verification, but must preserve tracebacks, tracing,
process boundaries, `cwd`/environment mutation, defers, signal hooks, streams,
and exact error spans.

## Structural Rules

- Keep the compact parser/arena path as the only source syntax representation.
- Keep one installed `FullProgram` per executable program; scratch and
  measurement structures must have clear drop points.
- Use typed IDs, compact tags/data, ranges, and side tables for persistent
  executable links.
- Verify before execution and rewind on failed construction.
- Preserve a direct, explicit representation for effects and host operations.
- Treat executable coverage as insufficient without value, output, error,
  source-span, trace, and stream parity.
- Do not add a bytecode VM merely because the indexed store exists. Reconsider
  only after broad indexed coverage and measured AST/dispatch bottlenecks remain,
  and only if all existing observability and OS contracts can stay exact.

## Measurement and Evidence

`src/frontend_stats.rs` provides deterministic structural accounting through
`xsh-frontend-stats`. For each input it measures `tokens`, `cst`, `ast_check`,
`lower`, and `after_drop`, reporting retained bytes, item counts, allocation
traffic, peak live bytes, blocker counts, dynamic-symbol ownership, and a
reconciliation delta. The binary in `src/entrypoints/frontend_stats.rs` alone
installs `mem_track::CountingAllocator`; product binaries do not.

The library reports structural counters without allocator tracking, marking the
lowered retained value as estimated when necessary. With tracking enabled,
lower-stage retained bytes are the live-byte delta across lowering. Worker
allocations are not represented by the ordinary controlling-thread columns.
`xsh-runtime-stats --json REPORT SCRIPT [-- ARGS...]` uses the ordinary runner
with `CountingAllocator` only in that profiling binary. Its report separates
construction, controller, and explicitly instrumented indexed `par-map` worker
traffic while preserving the script's stdout and stderr. Each worker report also
attributes allocation count and bytes to setup, `par_map_results`,
`par_map_item`, and `fused_reduce_item`; scoped figures intentionally omit live
bytes because values can cross worker boundaries. Worker peak totals are thread-local
allocation-pressure evidence, not process RSS or an exact concurrent-live total;
pair them with a host RSS check before making a memory claim.

The July 28, 2026 closeout corpus contained 287 files and 545,254 source bytes.
Its retained/peak comparison against the pre-redesign baseline was:

| Stage | Retained delta | Peak-live delta |
|---|---:|---:|
| tokens | +0.00% | +1.71% |
| CST | +0.00% | +2.44% |
| AST/check | -0.11% | +0.88% |
| lower | -77.81% | -14.33% |
| after drop | -1.49% | -20.70% |

The finalized corpus stored 43,589 instructions in 1,843,105 bytes, 57.11%
below the conservative recursive-row lower bound before counting that former
representation's nested allocations. These are historical closeout evidence,
not universal size budgets; remeasure the affected corpus before changing a
layout.

The JSON rollup follow-up is closed. On the Apple M1 measurement host, the regular
`xsh_json_log_rollup_10000_rows` median was 13.58 ms on July 27, 2026, compared
with 13.81 ms for the repeated pre-redesign measurement. The accepted path remains
the compact indexed program.

## Changing Indexed Execution

When changing interpreter-speed behavior, follow this path before adding an
indexed encoding:

1. Read `docs/SPEC.md` for source-visible semantics and this document for the
   compact executable contract.
2. Inspect `src/syntax/arena.rs` for the arena shape, `src/syntax/node.rs` for
   shared syntax, and `src/sema/check.rs` plus `src/modules/signature.rs` for
   checked signatures and operation identities.
3. Inspect the ordinary runtime behavior in `src/runtime/eval.rs`,
   `src/runtime/eval/modules.rs`, `src/runtime/eval/lowered_ops.rs`, or the
   focused runtime owner before changing indexed dispatch.
4. Add executable support only when the behavior has an exhaustive encoding,
   verifier coverage, and exact runtime parity. Keep stateful and OS-facing work
   behind explicit host/runtime operations.
5. Update `tools/xsh-ir-coverage.xsh` for accepted expansion coverage and add a
   user-visible benchmark workload only when the interaction affects latency.

The indexed representation is not a language layer. It is the verified
execution form of checked arena syntax. Imported modules participate in complete
program admission; processes, tracing-sensitive execution, and OS effects stay
explicit runtime boundaries rather than reasons to retain a source executor.

## Change and Verification Map

| Change | Read first | Focused verification |
|---|---|---|
| token, CST, or arena layout | `src/syntax/token.rs`, `src/syntax/cst.rs`, `src/syntax/arena.rs` | affected syntax/checker tests; `cargo test -p xsh --lib frontend_stats::tests` for retained accounting |
| declaration/checker metadata | `src/loader.rs`, `src/sema/check/compact.rs` | targeted semantic integration test and affected compact-lowering tests |
| indexed instruction or verifier | `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/lower.rs` | targeted `runtime::eval::indexed::full::tests::` library test |
| indexed execution or frames | `src/runtime/eval/lowered_run/indexed_run.rs`, `explicit_run.rs` | targeted runtime parity test; `runtime::stack_depth` for frame changes |
| retained frontend accounting | `src/frontend_stats.rs`, `tests/fixtures/frontend-indexed` | `cargo test -p xsh --lib frontend_stats::tests` and `cargo run --bin xsh-frontend-stats -- --json tests/fixtures/frontend-indexed` |
| runtime worker allocation accounting | `src/runtime_stats.rs`, `src/mem_track.rs`, `indexed_run.rs` | `cargo test -p xsh --lib runtime_stats::tests`, then `xsh-runtime-stats --json REPORT SCRIPT [-- ARGS...]` plus a paired release RSS check |
| user-visible performance | `docs/BENCHMARKING.md`, `docs/TEST-MAP.md` | focused benchmark first, then the applicable `cargo dev bench --fast` or `cargo dev bench` gate |

`tests/runtime/frontend_indexed.rs` keeps the frozen indexed fixtures on the
standard execution path. `tests/fixtures/frontend-indexed/README.md` documents
their exact coverage. The broader gate and commands belong to
`docs/TEST-MAP.md`; use debug builds for ordinary correctness work and release
measurements only for performance decisions. Do not run formatters or
autofixers as part of this workflow.

## Follow-Ups

The frontend redesign is complete. Remaining work is optional and must begin
with a measurable user-visible cost, not an attempt to recreate a previous
representation. [`FRONTEND-FOLLOWUPS.md`](../FRONTEND-FOLLOWUPS.md) contains the current queue, rejected
directions, and measurement rules.
