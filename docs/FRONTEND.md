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
reconciliation delta. Dynamic-symbol counters come from the parsed program's
owner, so concurrent files do not affect each other's measurements.

The binary alone installs `mem_track::CountingAllocator`; product binaries do
not. With that allocator, lowered retained bytes are the live-byte delta for the
lower stage. Library callers still receive all structural counters, with the
current shallow lowered fallback explicitly marked as estimated. For current
evidence, run the focused frontend-stat tests and command in
`docs/TEST-MAP.md`, followed by `make bench-fast`; historical campaign capture
scripts are not part of the production verification path.

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
one canonical ordered-field pool. Runtime records and modules use a separate
process-local shape identity keyed by the same ordered interned names:
executable `ShapeId` is program-owned, while host and public values may exist
without that program. Fixed runtime shapes store fields densely; the runtime
shape cache retains only all-preloaded shapes for steady-state reuse and does
not retain dropped dynamic-name shapes.

### Dynamic Name Ownership

Preloaded names continue to use generated static spellings. Dynamic names are
interned only while a `SymbolOwner` is active; the owner holds their `Arc<str>`
spellings and releases them when its last clone is dropped. `Name::as_str()`
returns a `NameText` value that borrows static storage or owns a dynamic spelling
as appropriate, rather than promising `&'static str` for session data.

`ArenaProgram` owns the symbols created by parsing, and a completed
`FullProgram` retains that owner for indexed execution. Lexer output, generated
interactive shell programs, evaluation workers, native-test workers, runtime
errors, and dynamic record shapes carry or re-enter the applicable owner. This
keeps borrowed spelling lookup valid without a process-lifetime name leak.

Semantic pools are cold metadata beside the frozen instruction store. The
common instruction remains the Phase 1 thirteen-byte
tag/data/optional-location row, and Phase 2 dependency ordering, SCC commit
watermarks, rollback, and verification-before-commit are unchanged.

### Full Indexed Function Bodies

`src/runtime/eval/indexed/full.rs` defines the production executable
representation. Its exhaustive encoders cover every checked expression,
statement, pattern, pipeline stage, process/run form, and persistable
compile-time value used by an admitted program. The finalized program owns no
recursive executable node, semantic `Type`, runtime `Value`, AST, token, or CST
reference.

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
store instead of retaining a program-wide collection of construction bodies.
The runner drops each temporary body after encoding and drops the arena,
checker outputs, and canonical maps before execution; product execution retains
only `FullProgram`.

Execution reads verified instruction, block, function, driver, pattern, stage,
and literal payloads through borrowed views. Scalar, list,
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

The Phase 4 migration evidence covered 837 committed functions and 43,589
instructions across 204 wholly executable files. Finalized storage is
1,843,105 bytes, 57.11% below a conservative 4,297,136-byte recursive-row lower
bound that excludes the old representation's nested heap storage. The report
also records all opcode frequencies and 13.560 extra bytes per instruction.
Current direct tests freeze returned values, output, runtime errors and
locations, and normalized trace behavior after all frontend and construction
scratch is dropped.

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
Tests execute the indexed store directly and freeze values, stdout/stderr,
status, cwd/env effects, process boundaries, errors, source locations, and
normalized traces. The arena runner and whole-function/whole-driver
compatibility decoders have been removed.

The corpus comparison admits 283 of 287 checked files as complete programs.
Those programs contain 1,647 driver steps, 705 coherent regions, and 2,919
region synchronization rows. Driver metadata retains 175,280 bytes. Retaining
the arena orchestration representation for the same admitted files would keep
2,935,921 arena bytes in addition to the executable store. The four rejected
files contain unlowered function bodies; no file is partially committed.

Coherent regions keep each runtime boundary independently verifiable without
retaining AST nodes. The script runner installs the verified `FullProgram` and
driver as its only executable program, drops the CST and arena, and reads
source-ordered driver steps from indexed storage. Function lookup resolves
against indexed function identity and kind. Coverage and native-test execution
use the same indexed entry; there is no execution-mode environment switch.

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

### Lowering / Indexed IR

`FullBuilder::build_compact` is the executable commit boundary. It lowers one
checked function at a time, encodes it immediately into `FullProgram`, and
drops the function-local construction scratch before moving to the next
function. Top-level statements are encoded only when the complete root and
imported-module program is representable. A clean lowering gap rejects the
complete commit; executable statements are never replaced by runnable
placeholders.

`FullProgram` is the only lowered executable representation. Its instruction,
data, block, pattern, stage, function, parameter, capture, driver, semantic,
string, byte, and location columns own everything needed at runtime. Direct
calls contain indexed function identities. Stateful and OS-facing behavior is
an explicit host operation referenced by an instruction or driver step.
Finalization verifies every range, owner, payload schema, location, terminator,
slot bound, and semantic identity before the program can be installed.

The lowerer writes short-lived expression, statement, pattern, typed-fast-path,
and top-level rows into one indexed construction arena. Child relationships use
typed four-byte IDs; no construction row recursively owns another row. The
encoder resolves those IDs while committing one function or the complete
driver, then drops the arena. Measurement probes count blockers but retain
neither construction programs nor function bodies.

### Execution

`src/runner.rs`, `Evaluator::eval`, native-test setup, loaded modules, dynamic
function dispatch, auto-main, signal hooks, and direct calls all install and
execute verified indexed programs. The runner drops the CST, checked arena,
semantic probe state, and construction scratch before the first driver step.

`src/runtime/eval/lowered_run/indexed_run.rs` interprets borrowed indexed
payloads. `src/runtime/eval/lowered_run.rs` contains shared host operations,
runtime value helpers, process/stream plumbing, and call-frame support used by
that interpreter; it contains no recursive expression, statement, or pattern
interpreter. Runtime function metadata uses a body-free
`FunctionHeader`.

There is no arena execution mode, shadow execution, whole-body decoder, or
per-opcode fallback. Parse/check diagnostics can prevent installation, but once
a `FullProgram` is committed the entire function and driver remain indexed.
Dynamic modules are independently lowered and verified, then registered by
qualified name with their owning indexed program.

A clean construction gap is always a diagnostic. The checker diagnostic path
distinguishes invalid source from a missing indexed representation; it does not
run an arena evaluator.

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

The frontend statistics report ranks every arena table by retained capacity,
item count, and number of source files that use it. Only four completed-table
rows were cold in the 287-file corpus: `with_bindings`,
`destructure_fields`, `builder_blocks`, and `builder_entries` occurred in at
most two files. They use `ArenaColdVec`, an 8-byte optional vector owner that
allocates only on its first insertion. Its reported retained bytes include both
the deferred vector header and its capacity. Other uncommon-looking tables,
including match arms, schemas, errors, and stream stages, remain ordinary
vectors because their corpus frequency makes an extra indirection a worse
trade-off.

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

This applies to the parser staging buffers, not just the permanent arena rows.
`builder_blocks` and `builder_entries` can be cold after a completed construct,
but their nested input stacks must remain separate until the corresponding
`finish_*` call.

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
recursive syntax path. `Program` remains fine where it names a real compact
program type, such as `ArenaProgram` or `FullProgram`.

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
   wins.** Indexed calls and structured control use heap-backed execution
   frames, replacing the former 64 MiB evaluation-stack reservation with a
   12 MiB bounded worker stack (8 MiB for stack-stress tests). Continue
   splitting cold match arms or boxing unusually large locals only when release
   measurements show lower RSS, fewer instructions, or simpler broad evaluator
   behavior. Measure stack size, release RSS, and instruction deltas; do not
   rely on debug timing.

3. **Reduce lowered value movement and retained report graphs.** After restoring
   actual `par-map` workers, the remaining tokei gap is dominated by dynamic
   `LoweredValue` records/maps/lists, worker setup, and final report/JSON
   construction rather than one missing scanner optimization. Good candidates
   are representation changes that apply to many scripts: cheaper small records,
   lower-clone method dispatch, more in-place collection updates when aliasing
   permits, or streaming JSON emission that does not retain extra script-visible
   state. Treat each as a measured release A/B over a representative user-facing
   workload.

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

## Indexed Execution Guide

### Critical Files

`src/runtime/eval/indexed/full.rs`
: Owns the finalized column layout, builder checkpoints, exhaustive payload
  codecs, verifier, function/driver views, and retained-byte accounting.

`src/runtime/eval/lower.rs`
: Lowers checked arena IDs into function-local builder scratch and records
  structured blockers. Production hands each completed function directly to
  the indexed builder.

`src/runtime/eval/lowered_run/indexed_run.rs`
: Executes verified function blocks and driver instruction ranges directly
  from borrowed payloads.

`src/runtime/eval/lowered_run.rs`
: Supplies shared runtime operations used by indexed instructions, including
  process, stream, filesystem, module, value-conversion, trace, and slot-frame
  behavior.

`src/runtime/eval.rs`
: Owns evaluator state, indexed installation, driver planning, native-test
  preparation, and dynamic indexed function registration.

`src/runner.rs`
: Owns the indexed-only script entry path and diagnostic fallback before
  installation.

`tools/xsh-ir-coverage.xsh`
: Reports source-surface coverage and counts finalized `FullTag` statement and
  expression opcodes rather than deleted recursive IR variants.

### Runtime Contract

- A complete verified `FullProgram` must exist before execution starts.
- A committed program retains no CST, arena, checker output, or recursive
  function body.
- Function and driver lookup use indexed identities and borrowed views.
- Host effects remain explicit instructions or driver boundaries.
- Indexed execution preserves values, output, status, cwd/env changes, errors,
  source locations, propagation, and trace frames.
- Adding language behavior requires an exhaustive encoding, verification, and
  direct execution case. There is no compatibility interpreter to absorb a
  missing opcode.

`LoweredValue` remains the compact runtime value set used inside indexed
execution. Public `Value` conversion stays centralized at call, host-operation,
and user-visible result boundaries. Fixed public records and modules use a
process-local `RecordShape` cache keyed by ordered `Name` identities plus a
dense `Arc<[Value]>` field slice. The runtime shape is deliberately distinct
from program-local semantic `ShapeId`: host values can outlive or arrive without
an executable program. All-preloaded shapes remain strongly cached for
steady-state reuse; dynamic-name entries are weak, and each live shape owns its
field spellings and relevant symbol owner. Dropping the last dynamic record or
shape releases its field names. Open records retain a dynamic map only after
adding a field outside their shape. Indexed calls keep function, slot, work,
defer, return, and trace state in explicit heap-backed frames. On the campaign's
64-bit release target, an ordinary call frame is 336 bytes, a work frame is 136
bytes, and a continuation is 96 bytes; the same layout capture accompanies the
runtime benchmark allocation columns.

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

Measured July 28, 2026 with the release `xsh` working tree over
`/Users/josh/dev/sentry` versus `/Users/josh/d/tokei/target/release/tokei`.
Each command was warmed up twice and measured for seven runs with `hyperfine`;
stdout was redirected to `/dev/null`. The default path uses the host's ten
parallel workers; traced execution intentionally remains serial for ordered
trace events.

| path | XSH wall | native wall | gap | XSH user CPU | native user CPU |
| --- | --- | --- | --- | --- | --- |
| default table | 1.418 s | 0.698 s | **~2.03× slower** | 4.307 s | 1.730 s |
| `--json` | 1.326 s | 0.695 s | **~1.91× slower** | 2.449 s | 1.736 s |

The latest pass combines direct lowered-function target caching, the release
shallow-call fast path, `par-map |> identity flat-map |> reduce-by` worker-local
fusion, ownership-preserving reducer extraction, and worker-local JSON report
aggregation. Aggregate counts still differ from native tokei
(language-detection/ignore differences), while both paired runs report the same
43-line table shape and JSON language set. A single-run RSS check is at parity:
~42.9 MiB vs ~48.1 MiB default and ~56.3 MiB vs ~56.3 MiB JSON.

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
   Historical release A/B: the default path went ~1.03 s → ~0.59 s wall and
   `--json` went ~3.07 s → ~0.83 s (parity at that time). Those measurements are
   superseded by the July 27, 2026 baseline above. Covered by direct lowered tests
   (`bytes_concat_lowers_and_matches_ast`, `mutually_recursive_pures_colower_atomically`,
   `statement_match_arms_relet_sibling_names`, `match_expr_bare_ident_fallback_lowers`)
   and tokei JSON parity.
4. **(Historical result — re-measure before pursuing.)** The `--json` report path
   was the biggest headline gap (~3.83×). The scanners were identical on both
   paths and were the dominant `--json` cost; lowering them (item 3) plus
   `record_schemas` memoization (item 5) brought `--json` to ~parity with native
   (~0.83 s vs ~0.83 s) **without** touching report assembly. The current baseline
   no longer supports treating the parallel list-merging `reduce-by` reducer as
   unnecessary; re-profile before deciding whether to pursue it. If a future
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
