# Compact Front-End and Lowered Runtime Pipeline

The active XSH pipeline is compact-first: source is parsed into dense token,
CST, and arena tables; checked directly from arena IDs; lowered from those same
IDs into a verified indexed executable store; and executed after the CST and
arena are dropped.
There is no `src/syntax/ast.rs`, recursive syntax tree, or compatibility
frontend in the normal execution path. Valid scripts do not fall back to arena
evaluation when indexed construction fails.

The front-end data layout is deliberately Zig-aligned in spirit: compact token
columns, typed node indexes, side-table ranges for variable payloads, and lazy
source-location recovery. The runtime is still language-first rather than
VM-first: indexed IR is an in-process executable representation, not a
serialized bytecode format or a second language.

[`FRONTEND-CAMPAIGN.md`](../FRONTEND-CAMPAIGN.md) owns the benchmark-gated
migration from the current lowered runtime representation to a Zig-inspired
compact indexed executable IR. This document describes current behavior; the
campaign records proposed structures, dependency-ordered phases, progress,
gates, and decisions.

## Measurement

`xsh-frontend-stats` measures the production frontend in five stages: tokens,
CST, arena parse/check, compact lowering, and the installed state after earlier
frontend ownership is dropped. It reports deterministic per-file and corpus
JSON or text over the campaign roots, including retained components, allocator
traffic and peak live bytes, lowerer blocker counts, dynamic symbols, and a
reconciliation delta.

The binary alone installs `mem_track::CountingAllocator`; product binaries do
not. With that allocator, lowered retained bytes are the live-byte delta for the
lower stage. Library callers still receive all structural counters, with the
current shallow lowered fallback explicitly marked as estimated. Run the full
Phase 0 protocol with `scripts/frontend-campaign-phase0`; its machine-specific
evidence stays under `target/frontend-campaign/phase-0/`.

## Indexed IR History

Phase 1 added a test-only indexed executable store in
`src/runtime/eval/indexed.rs`. It is not installed or constructed by product
binaries. The prototype flattens the frozen vertical slice from the current
lowered semantic oracle into parallel `IrTag`, eight-byte `IrData`, and compact
location columns, with variable payloads in one `u32` extra array. Functions,
blocks, parameters, captures, patterns, strings, bytes, and source locations
live in separately measured pools. Finalization shrinks and verifies every
column before the test executor accepts the program.

The common instruction row is 13 retained bytes: one tag byte, eight data
bytes, and a four-byte optional location ID. The frozen slice uses 3.675 extra
bytes per instruction; the corpus-weighted prototype subset uses 3.475. No
persistent instruction or pattern recursively owns another IR node.

The deliberate Phase 1 deviation from the eventual pipeline is construction
from `LoweredFunctionUnit` rather than directly from compact AST and semantic
IDs. This keeps the phase focused on proving storage, verification,
transactional failure, self-contained execution, and difficult semantic parity
without prematurely taking on Phase 2 dependency/SCC migration. The finalized
store contains no lowered nodes and executes after the token table, CST, arena,
checker output, construct probe, and lowering scratch are dropped. Phase 2 owns
moving construction to the compact-ID dependency graph.

### Dependency And Transaction Model

Phase 2 builds every compact function identity and its parameter/capture
metadata before emitting a body. Dependency discovery is a separate pass that
combines the current function-unit edges with an independent scan of committed
lowered bodies, then computes Tarjan SCCs once over compact `IrFunctionId`s.
The dependency-first SCC order is construction scratch and is dropped after
finalization.

An SCC emits beyond an explicit commit watermark. Its function bodies remain
pending until the partial verifier accepts instruction schemas, ownership,
slots, calls, locations, and every nested block in the SCC. Verification moves
the watermark; any blocker or verification failure clears pending bodies and
rewinds every store column to the SCC checkpoint. Unsupported constructs never
become `Unit` or another executable instruction.

Blockers are structured records containing function identity, blocker kind and
label, detail, compact source location, and optional callee. A separate compact
coverage report aggregates counts, callees, and bounded sample locations. The
indexed path does not inspect construct-probe blocker counters to decide whether
rows are safe.

The Phase 1–3 store remains as the narrow historical prototype and graph/SCC
evidence. Phase 4 adds the complete executable-body store described below
rather than widening that frozen vertical-slice format.

### Interned Semantic Identities

Phase 3 adds finalized semantic pools to the indexed executable store.
`TypeId`, `SignatureId`, and `ShapeId` are checked one-based `u32` identities.
Types use parallel one-byte tag and eight-byte data columns plus shared `u32`
extra storage. Signatures use eight-byte range rows over shared `u32` payloads.
Shapes are ranges over ordered `Name` identities. The finalized pools contain no
owned `Type`, callable, record, or module trees.

Canonical maps exist only in `SemanticPoolBuilder` construction state. A
transaction checkpoint covers every semantic column and rewinding also removes
canonical entries whose IDs were discarded. Finalization drops the maps,
shrinks the columns, and verifies all type children, signature parameters,
shape ranges, module exports, and recovery boundaries before execution.
`Unknown` and `Invalid` are rejected rather than assigned executable IDs.

Indexed function rows refer to a `SignatureId`; parameter and capture rows carry
`TypeId`. Record requirement instructions carry one `TypeId` instead of copied
field-name/type pairs. Semantic record and module types obtain `ShapeId` from
one canonical ordered-field pool. Runtime records still use their current map
representation; Phase 7 reuses the semantic shape identity when dense runtime
records land.

Semantic pools are cold metadata beside the frozen instruction store. The
common instruction remains the Phase 1 thirteen-byte
tag/data/optional-location row, and Phase 2 dependency ordering, SCC commit
watermarks, rollback, and verification-before-commit are unchanged.

### Full Indexed Function Bodies

Phase 4 adds a test-only complete indexed representation in
`src/runtime/eval/indexed/full.rs`. Its exhaustive encoders cover every current
`LoweredExpr`, `LoweredStmt`, typed integer/boolean expression,
`LoweredPattern`, `LoweredPipelineStage`, process/run form, and persistable
compile-time value variant. The finalized program owns no recursive lowered
node, `Type`, runtime `Value`, AST, token, or CST reference.

All hot instructions use a one-byte tag and eight-byte `IrData`. Variable
payloads use shared `u32` extra storage. Strings and arbitrary bytes have
separate program-owned blobs; source spans become compact location IDs; direct
calls contain `IrFunctionId`; and types and signatures use the Phase 3 semantic
pools. Recovery-only checker types are translated at the commit boundary to
their existing runtime wildcard meaning, `Any`, so `Unknown` and `Invalid`
never become executable semantic identities.

Every `Vec`-backed variable sequence has an explicit compact block row.
Statement blocks are distinguished from ordinary payload lists, record their
function owner, and cannot be shared across parents. A function row is 32 bytes
and names exact parameter, capture, body-block, and slot metadata. Parameter
rows are 12 bytes; optional defaults and validations occupy sparse 12-byte cold
rows; captures are 12 bytes; and blocks are 20 bytes. Function metadata that is
cold during execution is stored in a separate eight-byte row.

Finalization shrinks all columns and verifies tag/data parity, every payload
schema, instruction and block ownership, function termination, slot bounds,
type/signature/function/pattern/stage/value/string/byte/location IDs, dense
function instruction ranges, and parameter/capture metadata. Failed body
construction rewinds every graph, literal, location, operation, and semantic
column to one checkpoint. The verifier must succeed before a `FullProgram` is
returned.

The compact entry lowers and freezes one function at a time into the indexed
store instead of retaining a program-wide collection of recursive function
bodies. The runner drops each temporary function body after encoding and drops
the arena, checker outputs, and canonical maps before execution; product
execution retains only `FullProgram`.

Phase 6 execution reads verified instruction, block, function, driver,
pattern, stage, and literal payloads through borrowed views. Scalar, list,
record, formatting, field/method, result, direct user-call, typed
integer/boolean, binding, assignment, branch, loop, iteration, print, return,
yield, common collection-pipeline stages, and ordinary top-level driver forms
execute without reconstructing a recursive function or driver body. Direct
pipeline stages include text/JSON adapters, map/filter, parallel and block
stages, sorting/grouping, predicates, aggregates, collection, and range/count
transforms. Process execution, spawning and waiting,
filesystem/path/archive/hash/JSON host operations, match, defer, imports,
signal hooks, recursive calls, and every cold opcode execute from borrowed
indexed payloads as well. There is no opcode preflight or recursive decode
fallback: a verified function or driver step stays indexed for the entire
execution region.

The Phase 4 corpus evidence covers 837 committed functions and 43,589
instructions across 204 wholly executable files. Finalized storage is
1,843,105 bytes, 57.11% below a conservative 4,297,136-byte recursive-row lower
bound that excludes the old representation's nested heap storage. The report
also records all opcode frequencies and 13.560 extra bytes per instruction.
Exact vertical-slice differential tests compare returned values, stdout,
runtime errors and locations, and normalized traces after all frontend and
adapter scratch is dropped. Run the full protocol with
`scripts/frontend-campaign-phase4`.

### Top-Level Driver And Effect Boundary

Phase 5 selects coherent top-level regions inside an honestly admitted complete
program. A file receives an indexed driver only when every executable
top-level statement and every required function body lowers; otherwise the
complete driver is rejected. Declarations that require no runtime work become
explicit `Skip` steps. A committed region can therefore never jump back to the
arena evaluator or substitute a placeholder instruction.

The finalized driver is compact executable metadata, not an arena-owned
orchestration shell. `FullDriverStep` rows record one source-ordered operation,
its exact instruction range, compact location, slot count, slot range, and
effect bitset. `FullDriverRegion` groups adjacent effect-free steps and isolates
every import, cwd/env mutation, process boundary, signal/cancellation boundary,
trace-sensitive operation, dynamic call, defer, propagation, or host operation.
`FullDriverProgram` owns exact step and region ranges; imported module programs
are referenced by checked IDs. Admission recurses through every loaded module:
if any executable module statement lacks a lowered row, the root driver is
rejected. Compact locations carry their originating `SourceId`, so imported
module errors and traces continue to identify the module source.

State synchronization is explicit at both levels. A 16-byte
`FullDriverSlot` maps one runtime binding name and semantic type to a dense
step-local slot, with separate read, write-back, and mutability flags.
`FullDriverSync` rows are the deterministic union of those reads and writes for
one coherent region. Binding definitions and direct assignments are effects of
their driver step rather than implicit slot write-back. This preserves the
current distinction between loading captured bindings, writing mutations made
inside a lowered control statement, and directly defining or assigning a
top-level binding.

Final verification recomputes every step, region, program, effect, and
synchronization union. It rejects invalid locations, slot/type bounds, payload
schemas, effect bits, overlapping or unreachable program/region/sync rows,
overlapping or unreachable slot rows, step/program cycles or sharing,
cross-owner instructions and blocks, and gaps in the dense instruction ranges.
Tests use the arena mode as an independent semantic oracle and execute the
indexed store directly; the whole-function and whole-driver compatibility
decoders have been removed. Values, stdout/stderr, status, cwd/env effects,
process boundaries, errors, and normalized traces compare after the arena and
lowering scratch have been dropped.

The corpus comparison admits 283 of 287 checked files as complete programs.
Those programs contain 1,647 driver steps, 705 coherent regions, and 2,919
region synchronization rows. Driver metadata retains 175,280 bytes. Retaining
the arena orchestration representation for the same admitted files would keep
2,935,921 arena bytes in addition to the executable store. The four rejected
files contain unlowered function bodies; no file is partially committed.

Whole-program single-region lowering has the same honest admission but hides
effect scheduling inside one broad region. Arena orchestration covers every
file today but permanently retains a general equal interpreter and the arena.
Coherent regions were selected because production timing and allocation are
identical while the Phase 5 store remains test-only, and the representation
makes each runtime boundary independently verifiable without retaining AST
nodes. Run the complete comparison with `scripts/frontend-campaign-phase5`.
Phase 6 installs the verified `FullProgram` and driver as the only retained
executable program in the script runner. The runner verifies the complete
program before installation, drops the CST and arena, and reads source-ordered
driver steps from indexed storage. Function lookup resolves against indexed
function identity and kind. Coverage tracing uses the same indexed entry rather
than a separate arena execution path.

The test-only `XSH_TEST_EXECUTION_MODE` switch selects `arena` as the migration
oracle or `ir` for the production path. It is compiled only with the
`native-tests` feature. The normal binary always selects indexed execution.

Dynamic method calls whose checked receiver remains `Any` or `Unknown` are
explicit indexed method operations. Runtime method dispatch validates the
actual value and arity; it is not a lowering fallback. This admits the former
`sources.len()` campaign blocker, and complete-program admission reaches every
loaded file in the checked campaign corpus.

## North Star

- Dense token tables with token tags, byte starts, and compact payloads for data
  that would otherwise force source rescans or stringly lookups
- Typed indexes: `TokenId`, `ExprId`, `StmtId`, `PatternId`, `BlockId`, `RunFormId`
- Compact node rows with small tags, `u32` payloads, and byte spans; move toward
  main-token-derived spans only where measurement shows it reduces storage or
  work
- Variable-length child data in side tables or shared extra-data buffers
- Lazy line/column and end-span recovery from byte offsets and token tables
- Parser functions that return compact IDs directly
- Checker/desugar/lower/runtime consumers that read compact IDs, not recursive AST
- Runtime data structures that stay compact internally while preserving public
  `Value` behavior at language boundaries

## Architecture

### Lexer / Tokens
- Dense `TokenTable` with tag, byte-start, and compact-payload columns behind a
  shared `TokenTableData`
- Token starts use a promoted column: `u16` for ordinary small files, promoted
  to `u32` only when a byte offset needs it
- Payload rows are sparse; tokens with the default zero payload do not reserve a
  `TokenPayload` slot
- Token ends reconstructed from source on demand
- CST constructed from `TokenTable` directly
- Lexer output is compact-only

### Parser / Arena
- `AstArena` stores statement, expression, and type-expression rows as
  tag/data/span columns
- Span columns use promoted `(u16, u16)` rows for small files and `(u32, u32)`
  rows only when source length or interpolation offsets require it
- Parser-direct construction covers all statement, expression, pattern,
  command, and run forms
- Parse path returns `ArenaProgram` without constructing recursive
  `Program`/`Stmt`/`Expr` syntax trees
- Arena-only expression parsing returns `ExprId` directly for all forms
- Parser reserve heuristics are tuned to avoid retaining large unused compact
  arena capacity on ordinary small scripts; do not add a post-parse
  `shrink_to_fit` pass without proving the hot small-corpus frontend does not
  regress

### Declarations / Checking
- `CompactFileUnit` is the file-scoped frontend boundary around one parsed
  source/module file. It owns the `ArenaParseOutput` and exposes source identity,
  display path, CST, token table, parse diagnostics, root statements, module
  statements, import/export summaries, and declaration counts without requiring
  checker state, runtime state, or evaluator construction.
- `CompactModuleGraph` is the resolved declaration/export layer that can be
  built before evaluator installation. It carries import edges, module aliases,
  qualified `proc`/`pure`/stream/error declarations, type metadata, exported
  top-level binding names, source ordering, and deterministically sorted
  diagnostics.
- Type and error definitions in compact arena rows with side-table ranges
- `check_compact_declarations` validates from `ArenaProgram`
- `probe_compact_bodies` walks all function bodies from compact IDs
- Runtime declaration registration derives tag arity maps and error-family
  metadata from compact declaration state
- Checker entry points are arena-backed; declaration/type consumers should read
  `ArenaTypeExprTag`/`ArenaTypeExprData` directly rather than reconstructing
  recursive type-expression leaves
- Type metadata that must outlive a checker borrow should keep its owning
  `Arc<ArenaProgram>` plus the relevant typed ID, not a raised syntax copy

### Lowering / IR
- Compact lowered-IR construction produces `LoweredPureFunction` and
  `LoweredProgram` from compact IDs
- Function lowering now has an explicit per-function metadata boundary:
  `LoweredFunctionUnit` keyed by `LoweredFunctionKey`. A unit records pure/proc
  kind, source span, owning module, parameter/capture/slot counts, dependency
  edges, structured blocker reason, lowered body when available, and SCC
  metadata before runnable bodies are committed into the lowered function maps.
- Covers: match, list/map comprehensions, structured pipeline stages,
  source-backed fmt strings, string/tag match, guards, type patterns,
  alternation patterns
- `RuntimeOp` module-call nodes for all standard library calls
- First-class lowered scalars: `Float`, `Duration`, `Null`
- Top-level IR uses `Name` and `LoweredType`
- Nested block construction uses flat statement stacks (one allocation per
  block, not per node)
- Top-level lowering remains statement-granular. Unsupported executable
  statements fall back independently, declaration/import markers remain
  skippable, and lowered statement execution synchronizes tracked runtime slots
  before and after each lowered statement.
- Lowered records are compact internally: ordinary lowered record literals use
  vector-backed records keyed by interned `Name` values, while host/runtime
  records keep the public `RecordMap` shape at boundaries.
- Hot `Stats` records have lowered-only compact inline/boxed representations and
  still behave as records for projection, destructuring, indexing, JSON
  serialization, type checks, equality, spreads, and public conversion.
- `stat:false` filesystem entries use lazy `FsEntry` values. They report as
  records, but direct `path`, `name`, `ext`, and `kind` field reads project from
  the stored path/kind without materializing a record map.
- Large lowered slot lists freeze in place as `SharedList(Arc<Vec<_>>)` when
  read as parameters, so repeated reads share one retained list instead of deep
  cloning it.
- Cold or wide lowered value payloads are boxed, and string/byte views use
  compact `u32` offsets with owned fallbacks for pathologically large buffers.
  The current retained `LoweredValue` target is 32 bytes while public
  `runtime::value::Value` remains unchanged.
- `json.encode` on lowered values validates JSON compatibility, then serializes
  through a borrowed view instead of building a second JSON object graph.
- `bytes.concat` and list/method/index operations must treat `SharedList` as
  equivalent to an owned lowered `List`.

**Probe-then-commit, and the permissive `Unit` fallback (load-bearing).**
Lowering runs as a *measurement probe* (`CompactLowerConstructProbe` in
`src/runtime/eval/lower.rs`) that **never fails** — on any sub-node it cannot
lower it records a blocker metric and substitutes `LoweredExpr::Unit` /
`LoweredStmt::Expr{Unit}` so it can finish traversing and tally what's missing.
The same code paths produce the *real* lowering, so the probe must not commit
those `Unit` placeholders as runnable code: `lower_function_with_blocker` refuses
to commit a function whose body produced any `blocker_events`, so the fixpoint
retries once dependencies lower (or it falls back honestly). A construct that
slips through as `Unit` poisons everything downstream (forward refs, mutual
recursion, "cannot display Unit", "pipeline input expected List"); when chasing a
bug, suspect an upstream node that lowered to `Unit`.
- `CompactLowerConstructProbe` is the real lowering probe. Body lowerability is
  derived from its output with `CompactLowerBodyProbeOutput::from_construct`,
  replacing the former hand-synced `can_lower_*` body gate.
- Top-level `let`/`var` bindings are gated by `top_level_binding_kind` (must infer
  a `LoweredType` for the initializer) before the statement is committed.
- `XSH_LOWER_DEBUG=1` prints per-function `DBG fn …` lowerability lines, but ONLY
  for function bodies — top-level statement lowering is invisible to it; bisect
  top-level scripts by hand.

### Execution
- Compact-only execution via `try_eval_compact_lowered_only`
- Auto-main invokes lowerable `proc main(...argv)` from compact declarations
  without reconstructing a recursive function definition
- Standard-module `use` declarations are compact-skippable
- Proc commands, cd/env/print with propagate, signal hooks, loop, grouped
  run capture, par-map, tee, table.print lowered and executed
- Implicit variable declaration on assignment (auto-declare)
- Alternation patterns in match
- `run_script` first attempts the compact lowered runner unless tracing/coverage
  disables that fast path. It parses arena-only, drops the CST before preparing
  the lowered plan, installs the compact program, then drops the arena before
  runtime work. The fallback path exists to surface parse/check diagnostics; a
  clean fallback diagnostic path means a real compact lowering gap.
- `XSH_ALLOW_LEGACY_FALLBACK` is removed. Do not reintroduce an old frontend
  execution path under a compatibility flag.
- Lowered `fs.files`/`fs.walk` preserve live streams where possible, and the
  common `Stream |> map |> where |> par-map { ... }` shape feeds lowered
  `par-map` workers through bounded queues instead of first materializing the
  candidate list.
- Lowered identity `flat-map` immediately followed by `reduce-by` validates the
  outer list first, then reduces nested rows directly without building a
  flattened transient list. Empty-body `reduce-by --sum` projections of the form
  `{key: item.field, value: {out: item.field, ...}}` also skip the transient
  outer reducer record and update occupied record accumulators field-by-field.
  Stable `RecordVec` item layouts cache source-field indexes inside a reduce
  stage so projected reducers can avoid repeated name scans. Tracing keeps the
  ordinary per-stage path.
- When the same projected identity-`flat-map`/`reduce-by` shape follows lowered
  streaming `par-map`, the runner drains ready contiguous worker results into the
  reducer in encounter order. Completed nested result lists are dropped as soon
  as their earlier siblings are ready, so the live-stream path avoids retaining
  the whole post-`par-map` result graph. The shared ordered result buffer keeps
  absolute indices for worker writes and compacts drained prefixes after enough
  contiguous results have been consumed.
- Lowered `for item in fs.files/fs.walk(...) |> map/where |> par-map { ... } |>
  map/where` can stream ordered `par-map` results directly into simple loop
  bodies that contain no explicit control-flow statements. This keeps the JSON
  report fold from retaining the full post-`par-map` result list and uses the
  same compacting ordered result buffer.
- Lowered `Map.len()` is a direct cardinality read, avoiding the allocation-heavy
  `map.keys().len()` idiom when only the key count is needed.
- Lowered self-assignment recognizes `xs = xs.push(item)`,
  `map = map.set(key, value)`, `map = map.push(key, value)`, and
  `map = map.remove(key)` and mutates the target slot in place when aliasing
  rules permit. This avoids clone-heavy method dispatch for common collection
  update idioms.
- Lowered record literals append or replace fields during construction, then
  sort once before producing the final `RecordVec` or inline stats value. This
  preserves duplicate-field replacement semantics without sorting after every
  inserted field.
- Lowered compact `json.encode` uses a validating writer with an estimated
  output capacity. Pretty JSON still converts through the ordinary JSON value
  path so formatting behavior stays centralized.
- Parallel lowered `par-map` uses one `Arc<LoweredSharedState>` plus per-thread
  `LoweredWorker`s. Shared state holds immutable sources, lowered function maps,
  module caches, function-module maps, cwd, and env; workers own mutable stdout,
  stderr, slots, signal, and trace state.
- Lowered `par-map` writes worker results directly into indexed result slots
  instead of returning per-worker chunk vectors.
- Lowered `par-map` caps default workers at 6. Release worker stacks are 1 MiB;
  debug workers use 64 MiB. The outer lowered evaluator runs on a scoped 64 MiB
  worker stack so recursive XSH calls do not depend on the main-thread stack.
- Lowered evaluation has a `compact.stack-depth` guard shared by the main
  lowered evaluator, fast lowered expression paths, statement execution,
  function calls, and lowered `par-map` workers. Thread stack size is a safety
  margin, not the correctness mechanism: deeply recursive XSH code must either
  complete or fail with the structured stack-depth diagnostic before the native
  Rust stack is at risk. Hot recursive Int/Result[Int] call series and nested
  integer expression trees use explicit loop stacks, so package-manager-shaped
  recursion and lowered worker recursion do not depend on native call depth.
  `XSH_TEST_SMALL_EVAL_STACK=1 cargo test --test runtime stack_depth -- --ignored`
  forces reduced 16 MiB debug stacks for the main evaluator and lowered workers
  to keep that boundary reproducible.
- The collection self-assignment specialization is split out behind a guarded
  `Set`-method helper so ordinary lowered assignments stay on the compact main
  statement path. The outer evaluator stack reservation is now 64 MiB; the
  frame-shrink work is complete, though this did not by itself close the tokei
  wall/RSS gap.
- `showcase/tokei.xsh`'s default table path returns final `SummaryRow` reduce
  rows from `par-map`, then does only `flat-map { |rows| rows } |> reduce-by`.
  Keep this script-level shape; it removed a large nested scan/report graph
  without adding evaluator fusion complexity.

**Runner flow & strict lowerability** (`src/runner.rs`).
A script runs as: arena-only parse, drop CST, prepare/install compact lowered
plan, drop arena, then `Evaluator::eval_installed_compact_lowered_only`. If
`--strict-lower` is not set, normal script execution allows supported dynamic
lowered operations on `Any`/`Unknown` receivers and lets runtime dispatch decide
whether the actual value supports the method. `xsh --strict-lower` and
`xsht test --strict-lower` preserve the stricter diagnostic behavior:
after parse/check passes, a compact-lowerability gap reports the compact
diagnostic instead of permitting those dynamic lowered operations.
- **Eval stack:** the lowered evaluator still runs on a scoped worker thread so
  recursive XSH calls do not depend on the main-thread stack. The worker stack
  is 64 MiB (`run_eval_on_large_stack` in `eval.rs`), but native stack
  reservation is only a margin. Lowered expression, statement, typed fast-path,
  and call evaluators share depth accounting and report `compact.stack-depth`
  before the native stack is at risk. The hot recursive Int call and nested Int
  expression paths run on explicit stacks first. A panic in the worker is
  resumed on the main thread (not swallowed).
- **Auto-main:** `compact_root_proc_main_requires_auto_call` decides whether a
  script of only `proc main` is auto-invoked; `compact_auto_main_args` supplies
  the CLI args and coerces each positional `Str` arg to its declared `main` param
  type where lossless (notably `Str`→`Path`).

## Compact Node and Tag Design

The compact arena (`AstArena`) stores syntax as parallel column vectors keyed
by typed `u32` indexes. Statement, expression, and type-expression nodes each
have a tag column, a data column, and an inline byte-span column:

```rust
pub struct AstArena {
    pub span_source_id: Option<SourceId>,
    pub spans: ArenaByteSpans,
    pub span_source_overrides: Vec<ArenaSpanSource>,
    pub stmt_tags: Vec<ArenaStmtTag>,
    pub stmt_data: Vec<ArenaStmtData>,
    pub stmt_spans: ArenaByteSpans,
    pub stmt_span_source_overrides: Vec<ArenaSpanSource>,
    pub expr_tags: Vec<ArenaExprTag>,
    pub expr_data: Vec<ArenaExprData>,
    pub expr_spans: ArenaByteSpans,
    pub expr_span_source_overrides: Vec<ArenaSpanSource>,
    pub type_expr_tags: Vec<ArenaTypeExprTag>,
    pub type_expr_data: Vec<ArenaTypeExprData>,
    pub type_expr_spans: ArenaByteSpans,
    pub type_expr_span_source_overrides: Vec<ArenaSpanSource>,
    // ... blocks, patterns, definitions, literals, side tables ...
    pub extra: Vec<u32>,
}
```

`ArenaStmtData` and `ArenaExprData` are compact `u32` pairs:

```rust
#[repr(C)]
pub struct ArenaStmtData {
    pub lhs: u32,
    pub rhs: u32,
}

#[repr(C)]
pub struct ArenaExprData {
    pub lhs: u32,
    pub rhs: u32,
}
```

The tag column determines the payload interpretation:

- whether `rhs` is a child ID, optional child, token ID, or extra-data range
- which accessor method to use when reading the node back
- whether desugar/check/lower treat the node as expression, statement, pattern,
  type, command, or run form

Typed IDs are public at API boundaries and are `NonZeroU32` wrappers:

- `StmtId`, `ExprId`, `PatternId`, `BlockId`, `RunFormId`, `TypeExprId`
- `BindingTargetId`, `AssignTargetId`, `FunctionDefId`, `CommandStmtId`
- `IntLiteralId`, `FloatLiteralId`, `DurationLiteralId`, `StringLiteralId`, etc.
- `SpanId` — index into the shared span column

Variable-length children (call args, record fields, match arms, pipeline
stages, etc.) are stored as `ArenaRange { start: u32, len: u32 }` pointing
into side-table vectors. Accessor methods decode ranges into typed slices
(`arena.call_args(range)`, `arena.match_arms(range)`, etc.).

## Token Table

Tokens are stored as dense columns, not as rich objects:

```rust
pub struct TokenTableData {
    tags: Vec<TokenTag>,
    starts: TokenStarts,              // U16 until a source needs U32 offsets
    payloads: Vec<TokenPayloadEntry>, // sparse nonzero payload rows
}
```

End offsets are not stored. Compute them from:

- the next token start for fixed-width trivia-free cases
- token tag lexeme length for punctuation and keywords
- source re-scan for string-like or path-like tokens that need exact end
- CST trivia boundaries where tooling requires lossless reconstruction

Line and column do not belong in tokens. Keep byte offsets and compute
locations through `SourceMap`. The CST (`SyntaxTree`) is constructed from
`TokenTable` directly — syntax tooling no longer needs rich token spans as
its construction input. Unlike Zig's token table, XSH keeps a payload column
because interned identifiers, keywords, dollar identifiers, and string flags are
hot enough to justify avoiding repeated decoding.

## Source Locations

Source position policy:

- Store byte offsets as `u32`
- Store `SourceId` once per file/program where possible
- Store promoted `ArenaByteSpans` columns on statement, expression, and
  type-expression rows today
- Store shared `SpanId` references for side-table payloads that need spans
- Avoid full per-node `Span { source_id, start, len }`
- Compute `line`, `column`, and source line lazily for diagnostics
- Compute `end` lazily from syntax shape and token table
- Cache line-start tables in `SourceFile`, not in every token or AST node

`SourceFile::location` currently binary-searches cached line starts. That is
fine for ordinary diagnostics; if hot dumps start doing repeated nearby lookups,
copy Zig's idea of passing a previous line-start cursor through the dump path
rather than storing line/column on syntax nodes.

Span representation in `AstArena`:

- `stmt_spans`, `expr_spans`, `type_expr_spans` — promoted byte-span columns for
  hot node rows
- `stmt_span_source_overrides`, `expr_span_source_overrides`,
  `type_expr_span_source_overrides` — sparse source overrides for those inline
  span columns
- `spans: ArenaByteSpans` — shared promoted byte-span table for side tables
  that store `SpanId`
- `span_source_overrides` — sparse source overrides for shared spans
- Source IDs resolve from the relevant override table or the arena's default
  `span_source_id`

## Extra Data

Variable-length and uncommon payloads live in side tables keyed by `ArenaRange`:

```rust
pub struct ArenaRange {
    pub start: u32,
    pub len: u32,
}
```

The arena uses `Vec<u32>` as the shared extra-data buffer (`arena.extra`).
Typed accessor methods decode ranges into meaningful views.

Extra-data categories:

- Statement lists (`arena.stmt_ids(range)`)
- Block params, function params, schema fields, module contract entries
- Tag variants, error variants, error fields
- Call args, record fields, match arms, if branches, if-expr branches
- Pipeline stages, stream stage options
- Command args, run segments, redirections, env assignments
- Builder entries, word parts, fmt parts
- Destructure fields, pattern fields (record patterns)

Per-kind side tables complement the shared extra buffer for data that needs
typed struct storage rather than `u32` encoding (e.g., `Vec<ArenaBlock>`,
`Vec<ArenaCallArg>`, `Vec<ArenaPattern>`).

## Arena Builder Staging

For nested parser constructs, `ArenaProgramBuilder` must stage child inputs in a
side `Vec` and drain them into the permanent arena table only at `finish_X`.
Directly appending to the permanent table and recording `(mark, current_len)` is
wrong whenever another instance of the same construct can be parsed before the
outer `finish_X`.

Known staged groups include `fmt_part_inputs`, `word_part_inputs`,
`command_arg_inputs`, `env_assignment_inputs`, `redirection_inputs`,
`destructure_field_inputs`, `builder_entry_inputs`, and
`stream_stage_option_inputs`. `call_arg_inputs` is the reference template.

Before adding or changing any `begin_X`/`push_X`/`finish_X`/`discard_X` group,
ask whether an expression can be parsed while the group is open, and whether that
expression can contain another instance of the same construct. If yes, stage in a
side buffer and drain at finish.

## Compact-Only Guardrails

Parser entry points should return compact IDs or `ArenaProgram` directly. Avoid
bridge names such as `parse_*_with_arena`; those imply a second syntax path.
Builder APIs should expose shape-specific insertion methods (`push_*_type_expr`,
`build_*`, `finish_*`) instead of generic lowering hooks from recursive nodes.

Consumers that need to walk annotations should read `ArenaTypeExprTag` and
`ArenaTypeExprData` from the owning arena. If a local convenience enum is useful,
name it as an arena view and keep it private to that consumer; do not add a
public recursive syntax model or a `type_from_ast`/`raise_type_expr` bridge.

Loader/tooling APIs should describe the compact source boundary they own
(`text`, `bytes`, `file`, `entry`) and avoid generic names that imply a hidden
recursive syntax path. `Program` remains fine where it names a real compact or
lowered program type, such as `ArenaProgram` or `LoweredProgram`.

## Non-Negotiables

- **Avoid strings and stringly typed logic.** Use `Name`, typed IDs, enums,
  `RuntimeOp`.
- **Measure user-visible effects with the suite in `docs/BENCHMARKING.md`.**
- **Do not run formatters or autofixers.**

## Improvement Opportunities

These are durable compact/lowered-pipeline work items, not a backlog of
showcase-specific tweaks. Prefer improvements that benefit the representative
workloads in `docs/BENCHMARKING.md`.
Prefer changes that reduce parser/token churn, compact install cost, body/function
   probing allocation, or broad runtime value movement across ordinary XSH programs.

1. **Make project checking lean.** Good candidates are reducing token/text
   churn, shrinking hot parser staging objects, avoiding repeated module/setup
   allocation per standalone file, and making body/function probing reuse
   compact structures instead of rebuilding transient maps. Measure the complete
   `xsht_check_xsh_repository` operation, including its allocation signals.

2. **Keep shrinking hot lowered frames only when it buys measurable runtime
   wins.** The 64 MiB outer eval-stack work is complete, so future work should
   not chase frame splitting as a standalone
   milestone. Continue splitting cold match arms, boxing unusually large locals,
   or moving deeply recursive paths to an explicit work stack only when release
   measurements show lower RSS, fewer instructions, or simpler broad evaluator
   behavior. Measure stack size, release RSS, and instruction deltas; do not
   rely on debug timing.

3. **Reduce lowered value movement and retained report graphs.** The remaining
   tokei gap is dominated by dynamic `LoweredValue` records/maps/lists, ordered
   `par-map` coordination, and final report/JSON construction rather than one
   missing scanner optimization. Good candidates are representation changes that
   apply to many scripts: cheaper small records, lower-clone method dispatch,
   more in-place collection updates when aliasing permits, or streaming JSON
   emission that does not retain extra script-visible state. Treat each as a
   measured release A/B over a representative user-facing workload.

4. **Guard that lowering threads every behavior-bearing arena field.** A common
   failure mode is lowered IR silently omitting something the arena carries:
   fmt-string format specs, per-item stream errors, trace events, `exts:`,
   `--max-bytes`, or named method args. These usually produce wrong output
   rather than crashes, so "did it lower" coverage is not enough. Add targeted
   lowering parity tests or a structured review checklist around arena fields
   whenever adding or changing compact syntax rows.

5. **Reduce span storage only where retained metrics justify it.** XSH stores
   inline byte spans for every statement, expression, and type-expression row.
   A Zig-style `main_token`/token-range scheme could reduce frontend retained
   memory, but it must be applied selectively and measured with
   `scripts/ir-layout.py` plus Divan's allocation and `max alloc` metrics on the
   repository check workload. Preserve explicit spans for cooked text,
   diagnostics, and cross-source module composition.

6. **Close real compact construction gaps without recreating compatibility
   paths.** Compact-only is the baseline. Any remaining
   `compact.unlowered-*` diagnostics from `xsht check` are frontend completeness
   bugs, not a reason to revive recursive AST bridges. Recheck the real
   repository corpus before naming specific blocked files; older examples go
   stale.

7. **Keep builders compact-native.** When adding direct arena construction
   APIs, watch `ArenaProgramBuilder` with `scripts/ir-layout.py`; split cold
   staging state out if rare constructs grow the parser's hot staging object,
   and confirm the effect on the complete repository check benchmark.

## Lowered IR Guide
This is the focused map for interpreter-speed work. The lowered IR is an
acceleration layer for eligible pure functions and selected whole-script
regions. It is not a bytecode format, not a serialized program representation,
and not a replacement for the tree-walking evaluator.

### Lowerability Diagnostics And Corpus Work

The compact runtime should reject unsupported source shapes explicitly. `xsht
check` reports compact lowerability diagnostics without executing the script,
and `xsh` must not silently accept a source shape by falling back to an
unsupported path.

Treat every `compact.unlowered-*` or `runtime.unlowered-*` diagnostic as one of
three cases:

1. The compact runtime can already express the behavior, but the lowerer lost
   type information or is missing a construction case.
2. The compact runtime needs a new `LoweredExpr`, `LoweredStmt`, `LoweredType`,
   `LoweredValue`, or runtime operation path.
3. The source shape should remain unsupported, and the diagnostic should name
   the blocking construct, receiver type, module operation, method, or function
   dependency.

Keep lowering strict. Do not make `Any`, `Unknown`, records, modules, dynamic
calls, or unchecked receiver shapes permissive to pass a corpus check. Dynamic
receivers remain rejected unless a concrete checked type proves the lowered
method or operation is valid. The narrow exceptions are explicit dynamic
introspection surfaces such as supported `Any.has(name)` and single-argument
`Any.get(name)`, whose result remains dynamic unless an annotation supplies a
concrete type.

Use `xsht check --summary PATH` for broad passes. The summary preserves normal
diagnostics and appends counts by diagnostic code plus the first observed
location, which makes parse, check, compact-lowerability, and
runtime-lowerability failures visible as separate buckets:

```sh
for root in . ../laputa ../packages; do
  ./target/debug/xsht check --summary "$root" || true
done
```

Start with the smallest failing command, then widen:

```sh
cargo build --bin xsh --bin xsht
./target/debug/xsht check --summary path/to/script.xsh
cargo test -p xsht --test cli TARGET -- --nocapture
cargo test --test runtime TARGET -- --nocapture
```

When the public diagnostic points only at a top-level statement, reduce the
script by deleting surrounding statements until one expression remains. If that
is still unclear, add a temporary env-gated trace near the `lower_expr` blocker
path in `src/runtime/eval/lower.rs`, run with `XSH_LOWER_DEBUG=1` or a local
one-off trace, then remove the trace before finishing. The construct probe
records blocker counters and sample spans; prefer threading those details into
diagnostics over ad hoc string matching.

A lowerability change is done when the targeted command passes or has fewer
unsupported-lowering diagnostics than its baseline, remaining unsupported
constructs have specific diagnostics, strict dynamic behavior is preserved, and
the closest runtime, sema, or `xsht` tests cover the newly supported or newly
diagnosed construct.

Common failure classes:

- A concrete type is lost as `Any` or `Unknown`. Fix local slot, `Try(...)`
  ok-type, top-level slot metadata, method return, module return, env
  pseudo-field, or standard-record propagation instead of weakening method
  gates.
- A known method is missing receiver-specific return inference. Add the
  concrete checked return shape and keep `Type::Any` and `Type::Unknown`
  rejected.
- A known module operation is missing compact construction. Check
  `lowered_module_op_supported`, argument construction in lowering, and
  execution in `lowered_run.rs`; reuse registry signatures and existing runtime
  helpers.
- A top-level statement lowers in pieces but is rejected. The probe substitutes
  placeholders while counting blockers, but real lowering must reject any
  statement with blocker events. Find the first blocker event rather than the
  final placeholder.
- Checker-only forms such as `reveal_type(...)` may be skipped by `xsht check`
  lowerability, but normal `xsh` execution must still reject them.

Broad compact-lowerability gates currently expected to pass are:

```sh
./target/debug/xsht check .
./target/debug/xsht check ../laputa
./target/debug/xsht check ../packages
./target/debug/xsht check --summary tools/xsh-ir-coverage.xsh
cargo test --test runtime coverage::ir_coverage_scans_multiline_top_level_regions_once -- --nocapture
cargo test -p xsht --test cli
```

### Architecture

XSH executes through an AST-first pipeline:

1. `src/syntax` reads source into the AST and applies syntax-level
   normalization.
2. `src/sema` checks names, types, purity, module APIs, records, errors, and
   stream contexts.
3. `src/runtime/eval.rs` registers checked `proc`, `pure`, type, and error
   definitions.
4. Function lowering first produces explicit `LoweredFunctionUnit` values keyed
   by `LoweredFunctionKey`. Each unit records its function kind, dependency
   edges, SCC metadata, and blocker reason before runnable bodies are committed
   into the lowered function maps.
5. `Evaluator::refresh_lowered_pures` repeatedly lowers eligible `pure`
   functions and restricted effect-free `proc` bodies after currently known
   definitions are registered, then refreshes the script-level lowered cache
   against the current lowered function set.
6. `eval_use`, `import_user_module`, and `loaded_module_record` register module
   exports, then retry lowering so newly importable helper functions can
   unblock callers and later script statements.
7. `src/runtime/eval/call.rs` attempts the lowered call path.
8. `Evaluator::eval_maybe_lowered_stmt` attempts the top-level lowered
   statement path.
9. Unsupported functions, unsupported statements, and gated runtime contexts
   fall back to AST evaluation.

The current shape is effectively:

```text
source -> parse/desugar AST -> semantic check -> register functions
       -> lower eligible functions to LoweredFunctionUnit records
       -> commit runnable per-function IR bodies
       -> evaluate `use` statements and retry lowering with module exports
       -> lower eligible restricted proc bodies to the same internal shape
       -> call lowered functions when runtime gates allow it
       -> execute eligible top-level script statements through lowered IR,
          including calls to lowered restricted procs
       -> otherwise evaluate checked AST
```

The script or module is already loaded as checked AST before lowering. The
first lowering unit is one `pure` function body. Restricted `proc` bodies with
no host effects other than `error` can lower to the same representation for use
from script-level IR. The script-level lowering unit is one eligible top-level
statement, synchronized against normal runtime bindings. This keeps OS effects,
cwd/env state, process forms, tracing, defers, signal hooks, and stream
callbacks on the normal source-order evaluator while letting pure,
effect-free-proc, and script-level glue code skip repeated AST dispatch where
semantics are already represented.

Think of the lowered IR as a checked-AST acceleration cache. XSH still reads the
whole script/module graph into memory as AST before execution. There is no
streaming compiler, bytecode image, instruction dispatch loop, or serialized
intermediate format.

### Symbol Identity And Registry Source Of Truth

XSH uses compact interned symbols for identifier-like names. Source text stays
human-readable, but checked AST, semantic maps, runtime binding maps, and
lowered IR paths compare `Name` values by integer identity instead of repeatedly
hashing or comparing UTF-8 strings. Qualified identities use
`QualifiedName { namespace, member }`; IR, checker, and runtime code should keep
that binary identity through semantic paths and reserve formatted
`module.member` strings for diagnostics, docs, and other display boundaries.

`src/symbol.rs` defines:

- `Symbol`: a compact `u32` raw id.
- `Name`: a copyable wrapper around `Symbol`.
- `QualifiedName`: a pair of `Name`s for `namespace.member` identities.

The first ids are the fixed core builtin prefix. Their raw values are stable and
must not be reordered because constants such as `Name::INT` depend on them.
`Name::is_builtin()` only checks that fixed prefix.

Additional standard names are preloaded at build time. `build.rs` asks the
language-facing registry in `crates/xsh-registry` for the ordered preloaded
symbol list, then writes `OUT_DIR/preloaded_symbols.rs` with:

- one 64-byte-aligned ASCII byte blob for all preloaded symbol text;
- one compact range table indexed by `Symbol::raw()`;
- counts for the fixed builtin prefix and the full preloaded table.

The runtime interner initializes an `FxHashMap<&'static str, Symbol>` from
slices into that static blob. Resolving a preloaded `Name` slices the blob by
range. The generated table is ASCII-validated in `build.rs`; ASCII is valid
UTF-8, so the resolver keeps the conversion from bytes to `&str` in one small
checked boundary.

Dynamic symbols still support user-defined names and names that are not known at
build time. They keep the original safe lifetime model: text is copied into a
boxed string and leaked for process lifetime. Dynamic symbol ids follow the
preloaded range, and the interner stores direct `&'static str` references for
those dynamic symbols. This is less compact than an arena, but it avoids custom
unsafe storage while preserving the `Name::as_str() -> &'static str` contract.

The north-star invariant is:

> Adding a language-facing API name happens in exactly one place, and the
> checker, runtime, generated docs, coverage data, and preloaded symbol table
> all observe it automatically.

`crates/xsh-registry` is the authoritative language-facing registry. It owns
typed metadata for fixed core builtin symbol names, fixed semantic names such as
`Ok`, `Err`, `args`, `ARGV`, and `main`, standard module/function/method
signatures, parameter definitions, standard record schemas, builtin error
families, builtin type names, and runtime operation ids. The registry lives
outside the main `xsh` crate so Cargo build scripts and runtime/checker/docs
code can consume the same definitions without creating a build cycle.

The registry should remain Rust-native unless a stronger reason appears. XSH's
API metadata includes type expressions, runtime operation ids,
receiver-specific method behavior, command-callability, and special argument
checks. Keeping those definitions typed gives normal Rust compile-time checking
and avoids replacing one fragile string layer with another.

The invariant is achieved only when drift is mechanically impossible or caught
by tests. A migration is not done just because data moved from `src/modules` to
`crates/xsh-registry`; it is done when the old copy no longer exists and the
remaining consumers either read the registry directly or are covered by exact
adapter-equivalence tests.

The registry proof must cover these surfaces:

- preloaded symbols are generated from the registry and include every registry
  API, record, error, and builtin type name;
- checker and runtime API adapters expose exactly the registry metadata after
  converting from registry types into local runtime/checker types;
- standard record values produced at runtime match the registry schemas they are
  advertised to implement, enforced by strict debug/test return validation at
  standard module and method boundaries;
- builtin type-name decisions are made through a registry-owned typed enum, so
  the type checker and runtime cannot silently diverge through string literals;
- build-time symbol generation does not parse source files or maintain parallel
  source lists.

String output for diagnostics, docs, parser tokens, and runtime record field
values is still a boundary concern. Stringly-typed semantic matching inside the
type system is not acceptable: matching `Int`, `Path`, `Result`, `Error`, or a
standard record by raw string in more than one place reintroduces the same class
of drift that the registry is meant to remove.

Current registry migration status:

- fixed core and semantic symbol names live in `crates/xsh-registry`;
- builtin process error family metadata lives in the registry and the checker
  registers error definitions from it;
- standard record schema definitions live in the registry and
  `record_schemas()` is derived from that data;
- standard module and method names, parameters, signatures, and `RuntimeOp`
  values live in the registry while `src/modules/signature.rs` remains the
  checker/runtime adapter;
- debug/test runtime module and method calls validate successful values against
  their selected registry return signatures, including exact field sets for
  non-empty record types;
- `build.rs` generates `preloaded_symbols.rs` from
  `xsh_registry::symbols::preloaded_symbol_names()`, with no source extraction;
- `docs/SPEC.md` names the registry as the authoritative source for
  language-facing API metadata.

Future work should keep reducing adapter code where it no longer carries useful
semantic boundaries, but it must not recreate parallel hand-maintained name
lists. Keep `docs/SPEC.md` in sync with the implemented registry contract
whenever language-facing behavior changes.

### Runtime Gates

A lowered body can exist and still not run. `call_lowered_pure` uses the fast
path only when:

- tracing is disabled;
- the call is not an entry call through module context that changes behavior;
- arguments convert into the `LoweredValue` set for the function signature;
- the function successfully lowered.

Lowered bodies may still call other lowerable functions, including qualified
helper pures such as `auth.invalid_option`, when the callee body lowered
successfully. That nested call uses the already-lowered callee body; it does not
bypass the entry gate for module-owned calls from normal runtime dispatch.

Restricted procs are lowerable only when their effect annotation is `[]` or
contains only `error`. Unrestricted procs and procs with `fs`, `io`, `net`,
`process`, `env`, or `time` remain on the AST path. Ordinary proc dispatch still
uses the AST evaluator; lowered proc bodies are currently used only by lowered
script regions, with tracing disabled and no module-context gate active.

The fallback contract is strict: unsupported syntax, types, methods, or runtime
conditions must return `None` from lowering or opt out at the call gate. They
must not partially interpret different semantics.

Script-level lowered statements use the same conservative contract. The current
cache lowers typed top-level bindings, assignments, expressions, and lowerable
`if`/`while`/`for`/`match` statements. Explicitly typed top-level bindings and
untyped bindings with unambiguous lowered initializer types become
synchronization slots for later lowered script statements. Each lowered
top-level statement reloads slot values from normal runtime bindings before
execution and writes back only tracked mutable slots after execution. Tracing
disables the script-level fast path, and lowered loops service the same signal
checkpoints as AST loops.

### Critical Files

`src/runtime/eval.rs`
: The hub: defines the `Lowered*` IR types (the shared vocabulary), the
  `Evaluator`, the lowering registry/bridge (`refresh_lowered_pures`,
  `call_lowered_pure`), the `LOWERED_METHOD_NAMES` whitelist, and AST-evaluator
  glue. The lowered-IR subsystem was split out of here into the sibling modules
  below; the IR types stay here so all of them can reach the types (and their
  private fields) via `super::`.

`src/runtime/eval/lower.rs`
: The AST→lowered-IR lowering pass: `LoweredPureFunction::lower`,
  `LoweredProgram::lower`, `lower_stmt`, `lower_expr`, the top-level lowering, and
  `SlotScope` (which owns the dense slot-allocation, scope enter/exit, and
  `\0`-sentinel high-water invariants).

`src/runtime/eval/lowered_run.rs`
: The lowered evaluator — `eval_lowered_function`/`_stmt`/`_expr`/`_typed_int`/…
  as a second `impl Evaluator` block. The lowered `Call` errors
  (`unresolved-lowered-call`) rather than falling back per-call, so callees must
  be lowered before callers (see mutual-recursion note under Open performance work).

`src/runtime/eval/lowered_ops.rs`
: Pure value operations and method dispatch over `LoweredValue`
  (`lowered_*_method_value`, `lowered_value_matches`, binary/conversion helpers) —
  no `Evaluator`/`self`.

`src/runtime/eval/call.rs`
: Contains `call_registered_function` and `call_lowered_pure`, the dispatch gate
  between lowered pure calls and AST function calls. Ordinary proc calls stay on
  the AST path; eligible restricted proc bodies are invoked only from lowered
  script regions.

`src/runtime/eval/stmt.rs`
: Imports user modules, evaluates module exports, registers qualified exported
  procs/pures, and retries lowering after those exports are known.

`src/runtime/value.rs`
: Defines public `Value`. Boundary conversion must preserve this representation
  exactly, including `Result`, `Error`, `Path`, records, lists, and maps.

`src/symbol.rs`
: Defines `Symbol`, `Name`, and `QualifiedName`, initializes the generated
  preloaded symbol table, and owns the dynamic symbol interner. IR/checker/runtime
  semantic paths should carry `Name` or `QualifiedName`, not reconstructed
  dotted strings.

`src/syntax/arena.rs` and `src/syntax/node.rs`
: Define the source surface. `arena.rs` stores statement, expression, pattern,
  command, run, and call forms as compact arena IDs and arena node kinds.
  `node.rs` holds shared leaf syntax such as stream stage kinds, operators, and
  type-expression leaves.

`build.rs`
: Generates `OUT_DIR/preloaded_symbols.rs` from
  `xsh_registry::symbols::preloaded_symbol_names()`. It must stay a registry
  consumer, not a source scraper or parallel semantic-name list.

`crates/xsh-registry/src/signature/*` and `crates/xsh-registry/src/runtime_op.rs`
: Define standard API signatures and `RuntimeOp` identifiers. Use these to find
  normal method semantics before adding a lowered method.

`crates/xsh-registry/src/symbols.rs` and `crates/xsh-registry/src/types.rs`
: Define registry-derived symbol collection and builtin type-name identities.
  Type-name decisions in checker/runtime code should route through these typed
  registry representations.

`src/runtime/eval/methods.rs` and `src/runtime/eval/modules.rs`
: Contain normal runtime dispatch for methods and stateful module operations.
  Lowered methods should mirror this behavior or stay unsupported.

`crates/xsh-registry/src/records.rs`
: Defines standard record schemas. The Rust lowerer treats standard records as
  lowered record values when their fields are representable.

`tools/xsh-ir-coverage.xsh`
: Programmatic expansion map. It scans pure functions, restricted proc bodies,
  and top-level script statements in this repository, `../packages`, and
  `../laputa`, extracts the lowered surface from Rust source, and reports real
  fallback reasons.

### Frontend Benchmarking

Use the shared Divan suite when changing parse, desugar, semantic check,
lowering, or evaluator setup:

```sh
make bench
```

The `xsht_check_xsh_repository`, `xsht_format_check_xsh_repository`, and
`xsht_lint_xsh_repository` benchmarks exercise the frontend over the checked-in
XSH corpus through complete tool operations. XSH execution benchmarks cover
parse/check/lower/evaluate together. Treat latency as primary and Divan's
allocation count and allocated bytes as explanatory signals. See
`docs/BENCHMARKING.md` for the focused interpreter/IR diagnostic loop, workload,
and PGO policy.

Good remaining ideas:

- Reduce boxed lowered expression/statement representation where it duplicates
  the checked AST. This is the most likely IR-local win.
- Audit lowered vectors and maps for empty or single-item cases before adding a
  dependency; prefer existing local structures unless the benchmark shows a
  clear win.
- Consider compact token payload storage only if identifier/string-heavy
  scenarios still allocate meaningfully after IR-local work.
- Treat arena-backed AST storage as a larger parser/checker project. It can pay
  off, but it crosses formatter, diagnostics, checker, and runtime ownership.

Lazy cwd remains a runtime-startup question rather than a front-end lowering
question; cwd/path/process semantics still need a concrete current directory
when a script actually executes.

### Core Structures

`LoweredPureFunction`
: The lowered unit used for pure functions and eligible restricted proc bodies.
  Stores parameter names and kinds, return kind, slot count, and lowered body.

`LoweredProgram`
: The script-level lowered cache. Stores one optional lowered representation per
  top-level statement, so unsupported statements can fall back independently.

`LoweredTopLevelStmt`
: A lowered top-level statement plus the runtime binding slots it needs to sync
  before and after execution.

`LoweredType`
: Small type-kind set accepted at IR boundaries: unit, `Int`, `Bool`, `Str`,
  `Regex`, `Status`, path, error, record, list, map, and tag-union shapes.
  Named user types resolve through the runtime type-definition table; qualified
  type expressions resolve through `QualifiedName` keys in
  `qualified_type_defs`, not by synthesizing flattened `module.Type` names.

`LoweredReturnKind`
: Distinguishes plain returns from `Result[T]` returns so implicit result
  wrapping and error propagation match AST evaluation.

`LoweredStmt`
: Slot-oriented statement IR for `let`, `var`, assignment, `if`, `while`,
  `for`, `match`, `return`, `break`, and `continue`.

`LoweredStmtFlow`
: Internal statement-control result used by the lowered evaluator. Extend it
  only when the matching source control flow is implemented end to end.

`LoweredExpr`
: Expression IR for scalars, slots, binary operations, `if`, `match`, records,
  lists, tag constructors, simple list comprehensions, selected list
  pipelines, field/index access, selected methods, `Ok`/`Err`, `?`, `??`, and
  lowerable function calls. Dotted calls lower as qualified function calls only
  when the qualified name is in the lowered function set; otherwise dotted
  calls remain receiver methods and must pass `lowered_method_name`.
  Qualified type/tag lookup uses the same binary `QualifiedName` identity as
  function calls when the source type definition is qualified.

`LoweredPattern`
: Pattern subset for lowered matches. It supports wildcard and literal patterns,
  plus tag-union constructor patterns with simple binding or tuple-of-binding
  payload fields.

`LoweredPipelineStage`
: List-pipeline IR. Current stages are the `text.lines` adapter, `where`,
  `map`, `group-by`, `enumerate`, `take`, `drop`, `sort`, and `sort-by`;
  item-expression stages use a synthetic item slot for `.`, and `map` can also
  lower a simple one-parameter block with local `let`/assignment setup before
  its tail value.

`LoweredValue`
: Runtime value set used by the lowered evaluator. It intentionally avoids the
  full `Value` enum in hot paths. Boundary conversion is centralized in
  `lowered_value_from_runtime`, `lowered_return_value`, and
  `LoweredValue::into_value`.

### Extension Points

`LoweredPureFunction::lower`
: Entry point for one checked pure definition or eligible restricted proc body.
  Rejects unsupported signatures or bodies by returning `None`.

`LoweredProgram::lower`
: Entry point for the top-level script cache. Keep it statement-granular and
  fallback-friendly; do not require the whole script to lower before using
  lowered regions.

`lower_stmts`, `lower_stmt`, `lower_expr`
: The lowering pass. Keep these conservative and structural. If a construct is
  not completely represented, return `None`.

`lower_type_expr`
: Converts checked source type syntax into `LoweredType`. Standard records and
  user records are represented as `LoweredType::Record` when field values are
  lowerable. Qualified source types must be resolved through the qualified
  type-definition table, with text formatting reserved for diagnostics and
  user-visible output.

`lowered_method_name`
: Whitelist used by the lowerer and coverage tool. Add a method here only after
  `lowered_method_value` or `lowered_method_ref` implements exact behavior.

`lowered_method_value`, `lowered_method_ref`
: Receiver-method dispatch. Use `_ref` when slot-backed receivers can avoid
  cloning large lists, maps, or records.

`eval_lowered_function`, `eval_lowered_stmts`, `eval_lowered_expr`
: General lowered evaluator. Keep spans, `Result` propagation, type errors, and
  error values aligned with the AST evaluator.

`eval_lowered_fast_plain_return`, `eval_lowered_fast_return`
: Specialized fast paths for simple returns. If a new expression cannot be
  safely handled here, return `None` and let the general lowered evaluator run.

### Implemented Behavior

The lowered path currently covers the main pure glue shapes: scalar arithmetic
and comparisons, local variables, loops, branches, matches, records, lists,
maps, tag unions, simple list comprehensions, selected list pipelines including
`text.lines`, `group-by`, and filter/map block normalization, selected string/
regex/status/path/record/list/map/result methods, `regex.compile`, error
construction, result propagation including `Result[Unit]`, implicit pure tail
expressions, expression-only pure tail `match` statements, and calls between
lowerable pure functions. Recent value-method coverage includes string
`reverse()` and map `values()` in addition to existing collection helper
methods such as `extend()` and `keys()`. Tight text scanners also have
specialized lowered expressions for `Str.byte_len()` and positional
`Str.byte_at(...)`, plus a typed integer node for slot-backed
`Str.count_lines()`, so byte-offset and line-count loops do not pay the generic
lowered method dispatch cost on every hot call. Unary `-` and `!` lower into equivalent primitive
expressions, which keeps scanner initializers such as `var delim = -1` from
blocking the whole function. Scanner-shaped `Int` and `Bool` lets, assignments,
branches, and while conditions can use typed lowered statement variants when
the value shape proves the primitive type; ambiguous bare-slot `=` assignments
fall back to the generic lowered statement path. Typed boolean lowering also
uses direct slot-backed string predicates for `s.starts_with(...)`,
`s.contains(...)`, `s.trim() == ""`, and `s.trim().starts_with(...)` /
`s.trim().ends_with(...)` with literal needles. Repeated lowered pure/helper
calls recycle slot-frame allocations to reduce allocation pressure in helper
pures called inside loops. Qualified helper calls and qualified source type
annotations keep their binary identities through lowering and runtime
validation; the lowerer should not rebuild `module.member` names except at
display boundaries. `for line in text.lines()` can lower to a streaming
statement form that evaluates the receiver once and iterates Rust string lines
without first materializing the full `List[Str]`; ordinary `.lines()` calls and
pipeline stages still return lists when the source program asks for a list.
Lowered string slots can also hold internal string views, so line-loop items and
`trim()` results can borrow ranges from the original text and materialize owned
`Str` values only when they escape the lowered string path.

Restricted `proc` bodies with `[]` or `[error]` effects can lower when their
signatures and bodies fit the same value/control-flow subset. These lowered proc
bodies are callable from lowered script regions and can return plain values or
`Result[T]`, including result propagation with `?`. They are not used for normal
proc dispatch, tracing, module-context entry calls, unrestricted procs, or procs
with host effects.

At script scope, the lowered path covers top-level scalar/list/map/record
bindings with explicit lowerable types or obvious inferred types, assignments,
expressions, and lowerable branch/loop/match statements. Untyped bindings are
exposed as slots only when their initializer has an unambiguous lowered type,
such as a literal, simple collection, compatible branch expression, or a plain
return from an already-lowered pure or restricted proc.

It intentionally does not cover process forms, effectful module calls, tracing
behavior, stream callbacks with block parameters, arbitrary lambdas, ambient
cwd/env mutation, broad untyped script binding inference, `Bytes`, or host
objects such as `Command` and `Duration`. Current `Bytes` fallback counts come
from effectful `fs`/`io` procs, so adding a lowered byte value alone would not
make those bodies eligible for the fast path.

Run the coverage tool for exact current numbers:

```sh
target/release/xsh tools/xsh-ir-coverage.xsh -- --json target/ir-coverage.json
```

The pure-function percentage remains separate from the top-level script
lowerability percentage, and restricted proc-body coverage is a third number.
All three are expansion maps, not whole-language coverage.

### Whole-Script Coverage

For XSH, "whole-script IR coverage" does not mean every language feature runs
through IR. It means every executable script region is either lowered or has an
explicit, measured, documented fallback reason.

The coverage boundary is:

1. Executable top-level script regions: top-level bindings, assignments,
   expressions, and control statements are counted independently.
2. Pure/effect-free bodies: pure functions and restricted proc bodies are
   reported separately because their call gates and source contracts differ.
3. Imports and modules: `use` statements are runtime boundaries that load AST
   modules and refresh lowering after exports are registered; they are measured
   as script fallback points, not hidden IR work.
4. Runtime gates: tracing, module-context entry calls, unsupported argument or
   return values, and unsupported body shapes keep the AST evaluator in charge.
5. Benchmarks: IR expansion requires a corpus hit and an affected operation in
   the shared Divan suite. Add a workload only when a user-visible path is not
   already represented.

The coverage tool is conservative. It scans source shape and lowered capability
lists; it does not prove that a specific dynamic execution used IR, and it does
not model every checker fact. Top-level script scanning groups obvious
continuation regions such as multiline structured pipelines, list literals, and
argument lists, and it skips triple-quoted string contents and proc/pure
definition bodies before assigning fallback reasons. Treat false negatives as
acceptable prompts for manual inspection, not as language failures.

Each pure, restricted-proc, and top-level script report includes both raw
fallback reasons and grouped summaries. The groups separate import boundaries,
explicit runtime boundaries, proc effect boundaries, methods, types,
expressions, statements, and uncategorized leftovers so intentional AST-first
regions are not mixed with actionable lowering candidates.

### Active Roadmap

The lowered IR should remain serious, lean, and inspectable before considering a
bytecode VM.

1. Keep the AST as the semantic source of truth. Lowered IR must be a runtime
   acceleration cache with precise fallback.
2. Broaden typed value, method, and statement coverage only when there is a
   corpus hit, benchmark case, and exact parity with the AST evaluator.
3. *(Done.)* `src/runtime/eval.rs` was split into focused modules — `lower.rs`
   (lowering + `SlotScope`), `lowered_run.rs` (lowered evaluator), `lowered_ops.rs`
   (value ops + method dispatch), and `tests.rs` — dropping it from ~13.8k to
   ~5.0k lines, with the IR types kept in the hub. See Critical Files.
4. Improve programmatic coverage precision for top-level regions and proc
   bodies, especially around imported module exports, effect annotations, and
   method false positives from line-based scanning.
5. Expand the curated suite only for missing user-visible workflows. Do not
   revive internal glue loops or a separate lowered-IR corpus.

### Performance Methodology

The lowered IR exists to make ordinary XSH shapes — local scalar loops, string
and byte predicates, `for line in text.lines()`, small pure helpers, and
`par-map |> reduce-by` aggregation — cheap enough for file-oriented and
text-scanning workloads, without growing the standard library with
benchmark-shaped primitives. The AST stays the semantic source of truth; lowered
IR is an acceleration cache with precise fallback.

Decisions are made on release measurements, never debug timing. Debug timing is
directional — enough to find a likely bottleneck, never enough to keep or reject
a change. Every accepted change should name the exact cost it reduces, preserve
output parity (and computed-value parity against any reference implementation),
carry direct lowered/runtime tests with exact AST parity, and report a release
A/B delta for the affected command or fixture. Flat or regressing changes should
be reverted, or kept only with a note explaining why the representation is still
worthwhile (coverage or consistency rather than speed). See **Benchmark Loop**
for the mechanical loop.

#### Diagnostic workflow

`docs/BENCHMARKING.md` owns the interpreter and IR measurement loop. Start from
one affected user-facing Divan operation, use raw Divan allocation and peak-live
measurements plus `scripts/ir-layout.py` or `tools/xsh-ir-coverage.xsh` to
attribute the cost, and return to the complete suite for the decision. Native
comparisons such as `showcase/tokei.xsh` remain useful case studies, but they
are not a second benchmark gate or PGO workload.

New lowered behavior needs direct tests with exact AST parity, normally in
`src/runtime/eval/tests.rs`. Mirror normal `RuntimeOp` semantics from
`src/runtime/eval/methods.rs` and `src/modules/`; do not change a showcase's
observable output merely to match an external reference implementation.

#### Current baseline and gap

Treat the historical values below as context, not as a fresh completion claim.

Release `showcase/tokei.xsh` over `/Users/josh/dev/sentry` vs native release
tokei (warm, hyperfine, 7 runs):

| path | XSH wall | native wall | gap | XSH user CPU |
| --- | --- | --- | --- | --- |
| default table | ~0.59 s | ~0.77 s | **~1.31× faster** | ~1.23 s |
| `--json` | ~0.83 s | ~0.83 s | **~parity** | ~1.31 s |

Trajectory of the default path: pre-lowering it was ~1.65 s wall / ~8.6 s user
(~2.3× native); after the `map.empty()`/block-scope scanner-lowering fixes it was
~1.03 s / ~2.34 s (~1.35× slower). After **co-lowering the mutually-recursive
scanner cluster** (SCC co-lowering + `bytes.concat` + match-arm block-scoping +
bare-ident match arms; item 3 below) plus memoizing `record_schemas` (item 5), it
is ~0.59 s wall / ~1.23 s user — **now faster than native release tokei** on the
default path. Aggregate counts still differ from native tokei
(language-detection/ignore differences, compared separately); XSH-vs-previous-XSH
JSON parity is preserved (canonicalized).

The `--json` gap closed at the same time: the heavy TS/JS/HTML/Markdown scanners
run on both paths and were the dominant `--json` cost too, so lowering them (with
cheaper per-file record materialization from the `record_schemas` memoization)
brought `--json` from ~3.83× to **roughly at parity** with native — without
touching the report-assembly path itself (the planned parallel report-list reducer
is therefore no longer justified by the benchmark; re-measure before pursuing it,
see item 4 below). The remaining XSH self-time is now value movement (alloc/free +
value-drop/clone + btree/record), not scanning or scope hashing.

#### Open performance work

Historical CPU self-time attribution of a serial `showcase/tokei.xsh` scan over
the Sentry corpus reshaped the priorities below. The *pre-fix* compute-only
split was roughly:
allocation/free ~27%, semantic type/schema work ~27%, generic AST eval ~15%,
scope management ~6%, string/byte scan ~7%, and **lowered IR eval ~0%** — the hot
scanner path was *not lowered at all*, so the documented lowered predicate/`for
line` fast paths never fired. Item 1 addressed that; item 3 then co-lowered the
remaining mutually-recursive scanners. The current split (regenerated baseline):
`other` ~22%, value-drop ~12%, alloc/free ~12%, ast-eval ~12%, stream/pipeline
~12%, **lowered-eval ~11%**, str/byte-scan ~7%, btree/record ~6%, value-clone ~4%,
**sema/type-intern ~1%** (was ~27%), scope-mgmt ~0.4% (was ~6%). The sema/SipHash
and scope buckets effectively vanished once the scanners lowered (they used
integer slots instead of `HashMap<String, Binding>`), and `record_schemas` is now
memoized. The dominant remaining cost is value movement (item 6 below). In
priority order:

1. **(Largely done — `map.empty()` lowering.)** The root cause of `lowered-eval`
   ~0% was that **every `count_*` scanner was rejected from lowering by its single
   `blobs: map.empty()` call**: that nil-ary builtin constructor was not lowerable,
   so `LoweredPureFunction::lower` rejected the whole function and the AST↔lowered
   bridge (`call_lowered_pure`, wired from `call.rs`) fell back to
   `eval_call`/`eval_stmt`/`eval_expr` with full `push_scope`/`pop_scope`. Bisection
   confirmed `Bytes` params/returns and Map-field record params all lower fine (the
   IR-coverage tool's `type.param.Bytes`/`type.return.Bytes` reasons are stale);
   only `map.empty()` blocked. Fixed by lowering `map.empty()` to `LoweredExpr::EmptyMap`
   (empty list literals already lower), plus a latent bug: `lowered_value_matches`
   had no `Bytes` arm at all, so any Bytes param/return through a lowered call
   errored "expected Bytes, found Bytes" once scanners started lowering — added
   `Bytes`/`BytesView` arms mirroring `Str`. Release A/B on the parallel Sentry
   scan, with byte-for-byte identical XSH totals: **user CPU ~8.6 s → ~2.4 s (~3.5×), wall
   ~1.65 s → ~1.0 s (~1.65×)**; native release tokei is ~0.75 s, so XSH went from
   ~2.3× to ~1.3× slower. Remaining: 6 scanners (`count_json`, `count_html`,
   `count_language`, `count_markdown`, `count_slash_language`, `join_lines`) still
   fall back — see item 2.
2. **(Done — block-scoped re-binding.)** `lower_stmt` rejected any `let`/`var`
   whose name was already in the slot map (`if slots.contains_key(name) { return
   None }`), banning the scanners' sibling-scope re-`let` (early-return `if` body
   declares `let stats`, the function body declares it again). Fixed by scoping
   nested blocks: `lower_block_stmts` snapshots the name→slot map on block entry
   and restores it on exit (so block-local bindings drop and a sibling can re-`let`
   the name), keeping every slot index reserved via a `\0scope.N` placeholder so
   `slots.len()` stays the high-water count. Applied to `if`/`else`/`while` bodies
   and the `for` loop var + body; the function-body statement list stays unscoped
   because its tail expression depends on its prefix bindings. The `let` rejection
   is *kept* on purpose: nested shadowing of a still-in-scope name (verified
   block-scoped at runtime) conservatively falls back to AST rather than risk a
   miscompile, since the scanners only need sibling scopes. `count_json` now
   lowers; `lowered-eval` self-time rose to ~9–10%. (NB: a parallel rebuild
   replaced the pre-fix release binary, so a clean isolated A/B for this lever
   alone wasn't possible; correctness is covered by unit tests + tokei parity.)
3. **(Done — co-lowered the mutually-recursive scanner cluster.)** Five scanners
   ran on the AST evaluator because they form a **mutual-recursion cycle**
   (`count_markdown` ↔ `count_slash_language`, both call `count_html`, and
   `count_language` dispatches to all of them). The single-candidate fixpoint
   (`refresh_lowered_pures`) could not bootstrap a cycle. Fixed with **SCC
   co-lowering** in `colower_pure_sccs` (`src/runtime/eval.rs`): when the
   single-candidate sweep stalls, build the un-lowered pure call graph, Tarjan-SCC
   it, and co-lower each component atomically (all members added to the candidate
   set via `LowerableFunctions::pures_with_candidates`; commit only if every member
   lowers). Three more leaf blockers fell out during bring-up: (a) `bytes.concat`
   (`join_lines`'s tail) now lowers to `LoweredExpr::BytesConcat`; (b) statement-
   and tag-match **arm bodies are now block-scoped** (`lower_block_stmts`, like
   if/else) so sibling arms can re-`let` the same name (`let scan`/`let body` per
   `count_markdown` fence arm) — they previously used un-scoped `lower_stmts` and
   collided; (c) a **bare-identifier match arm** (`_ => empty`, parsed as
   `ArenaStmtKind::TailBareIdent`, not `ArenaStmtKind::Expr`) now lowers via
   `lower_arm_value_expr`/`lower_bare_ident`, which `count_language`'s `_ => empty`
   needs. All five (`count_html`, `count_markdown`, `count_slash_language`,
   `count_language`, `join_lines`) now lower; only `lang_for_name_ext` still falls
   back, on **guard patterns** (`e if e == "ts" or …`), which the lowerer does not
   yet support and which are out of scope here (a cold per-file dispatch helper).
   Release A/B: default path went ~1.03 s → ~0.59 s wall (now faster than native);
   `--json` ~3.07 s → ~0.83 s (parity). Covered by direct lowered tests
   (`bytes_concat_lowers_and_matches_ast`, `mutually_recursive_pures_colower_atomically`,
   `statement_match_arms_relet_sibling_names`, `match_expr_bare_ident_fallback_lowers`)
   and tokei JSON parity.
4. **(Resolved as a side effect of item 3 — re-measure before pursuing.)** The
   `--json` report path was the biggest headline gap (~3.83×). The scanners are
   identical on both paths and were the dominant `--json` cost; lowering them (item
   3) plus the `record_schemas` memoization (item 5) brought `--json` to ~parity
   with native (~0.83 s vs ~0.83 s) **without** touching report assembly. A
   parallel list-merging `reduce-by` reducer (to group per-language report lists in
   parallel, deleting the serial ~18k-file dispatch loop in `--json`) was the
   planned next lever; it is no longer justified by the benchmark. If a future
   change regresses `--json`, add or adapt a representative benchmark before
   reaching for it.
5. **(Done — `record_schemas` memoized.)** `sema::records::standard_record_type`
   rebuilt the whole `record_schemas()` `BTreeMap` (every schema `Type`) on each
   call; it is now memoized via a `LazyLock` (`src/sema/records.rs`), so the hot
   per-record-construction lookup is a map read. The rest of the old
   "type/schema/SipHash" bucket was a *consequence* of item 3 — the AST evaluator
   hashes scope names via SipHash+RandomState, work the lowered scanners now avoid
   with integer slots — and dropped from ~27% to ~1% once they lowered.
6. Cut allocation and value movement (still the largest single bucket). The
   Gate-2 axis behind local record-accumulator mutation is described in
   `LANG.md`; track volume through Divan's allocation metrics.
7. The string/byte scan itself (`memcmp`/memchr) is near-irreducible; do not chase
   it before 3–6.

See `docs/BENCHMARKING.md` for the current attribution, allocation, layout, and
coverage workflow.

#### Case study: tokei showcase

`showcase/tokei.xsh` was the forcing benchmark for most of the above and ruled
out several tempting directions. Durable lessons worth not relitigating:

- Byte-offset rewrites of line scanners (`byte_at`/`find`) regressed against
  idiomatic `for line in text.lines()`; the win was removing per-line helper
  calls, not abandoning `lines()`.
- Specialized standard-library summary APIs (e.g. a native `line_stats`) were
  removed in favor of internal lowered string views for idiomatic loops plus
  view-aware predicates. Do not add benchmark-shaped public text APIs.
- On large corpora, report accumulation can dominate scanner throughput. Avoid
  per-file `List.push` into per-language buckets; collect records once and derive
  reports in batches.
- Summary-only XSH-level scanners preserved totals but regressed badly.
- Worker-count tuning moved wall time only at noise level; scanner-loop runtime
  cost and intermediate `Value` movement are the higher-value targets.
- The biggest single win was getting the heavy mutually-recursive scanners
  (TS/JS, HTML, Markdown) onto the lowered path at all. The cluster is one SCC
  (`count_markdown` ↔ `count_slash_language`, both into `count_html`, dispatched by
  `count_language`), so it had to be co-lowered atomically — the one-candidate-at-a-time
  fixpoint cannot bootstrap a cycle (a lowered call to a not-yet-lowered callee
  errors rather than falling back). It also closed the `--json` gap as a side
  effect, because the same scanners are the bulk of report-record cost — i.e. lower
  the shared hot code before reworking a path-specific serial loop.
- When a whole function refuses to lower, the blocker is usually one leaf
  construct, not the obvious one. Bisect empirically (env-gated dump of
  lowered/rejected pures); here the scanner cluster was gated behind, in order,
  `bytes.concat`, un-block-scoped match arms colliding on sibling `let`s, and a
  bare-identifier match arm (`ArenaStmtKind::TailBareIdent`) — none of which is the
  recursion the cluster appears to be about.
- The default table intentionally keeps a tokei-like presentation: embedded
  ("`|- Child`") breakdown and per-language `(Total)` rows, heavy/light rules
  (via `tui` glyphs), fixed column right-edges (28/41/54/67/80, 80-wide rows /
  81-wide rules), and stable language ordering. The byte-for-byte gate for this
  showcase is XSH output against saved XSH output for the same corpus and
  options, not XSH against native tokei. Native output is useful for spotting
  accuracy gaps, but real-corpus differences in line classification,
  child-language treatment, JSON field order, and report ordering do not by
  themselves fail the interpreter performance objective. The MDX prose-only
  scanner change is an example of an intentional accuracy improvement that still
  uses XSH-vs-XSH saved output as the regression gate. The per-`(parent,
  child)` breakdown is aggregated in the stream by expanding each file into one
  parent record plus one child record per embedded language
  (`par-map |> flat-map |> reduce-by --sum`, children keyed `parent\tchild`);
  `|- Child` rows use the child's *deep* total (recursively including its own
  nested blobs, e.g. a TOML fence inside a Rust doc-comment Markdown blob). The
  `flat-map` breaks the `par-map |> reduce-by` fusion and the child expansion
  adds work, so it remains one of the hot value-movement paths.
- The corpus count gap splits into two axes. **File selection** was closed to exact
  (Δfiles = 0 per language over the Sentry tree) cheaply: `.pyi`/`.pot` extensions,
  and `#!`-shebang detection for extension-less scripts (`lang_for_shebang`; the
  pipelines keep extension-less files past the language filter so par-map can read
  the first line, then drop the still-unknown ones). **Line classification** is the
  other axis: matching tokei's per-line code/comment/blank split byte-for-byte needs
  each language's own string/comment tokenizer (Python `"""`/`'''`, JS/TS backtick
  template literals, HTML `<style>`→CSS embedding, blank-inside-block-comment, …).
  Those were prototyped and reached ~0.12% of total lines, but they require
  char-level scanning of every string/comment-bearing line — and an interpreter
  (even lowered) doing per-character work over ~5M lines runs ~1.65× *slower* than
  native tokei, forfeiting the speed lead. Decision: **keep the cheap file-selection
  parity, revert the char-scanning tokenizers**, leaving the shared approximate
  counters (`count_hash_language`, `count_slash_plain`, single-pass `count_html`)
  for line classification. So the showcase keeps cheap file-selection parity and
  a tokei-like output format, while line-classification counts remain a deliberate
  approximation rather than a byte-for-byte tokenizer port.

The `par-map |> flat-map |> reduce-by` default-table path and the borrowed
`for line in text.lines()` representation are in place, and `Bytes` is a
first-class zero-allocation byte-scanning surface.

### Deferred VM Considerations

A bytecode VM is not justified by the current architecture. Reconsider only if
the lowered IR becomes broad enough that a compact instruction set is obvious,
benchmark results show AST dispatch remains the bottleneck after lowered-region
coverage is high, and the VM can preserve exact tracing, process boundaries,
cwd/env mutation, defers, signal hooks, stream behavior, and fallback
independence. If that happens, this guide becomes the boundary spec for bytecode
work rather than disposable scaffolding.

### Benchmark Loop

Use the interpreter and IR diagnostic workflow in `docs/BENCHMARKING.md`.
Lowerability work starts with a frequent real fallback from
`tools/xsh-ir-coverage.xsh`; representation work starts with allocation,
peak-live, and `scripts/ir-layout.py` evidence from an affected real operation.
Both finish with direct AST-parity tests and `make bench`. The useful priority
order is corpus frequency, semantic safety, benchmark coverage, then
implementation size.
