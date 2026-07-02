# Compact Front-End and Lowered Runtime Pipeline

The active XSH pipeline is compact-first: source is parsed into dense token,
CST, and arena tables; checked directly from arena IDs; lowered from those same
IDs where the behavior is supported; and executed by the compact lowered runtime.
There is no `src/syntax/ast.rs`, recursive syntax tree, or compatibility
frontend in the normal execution path.

The front-end data layout is deliberately Zig-aligned in spirit: compact token
columns, typed node indexes, side-table ranges for variable payloads, and lazy
source-location recovery. The runtime is still language-first rather than VM-
first: lowered IR is an execution cache for the supported compact subset, not a
serialized bytecode format or a second language.

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
  Tracing keeps the ordinary per-stage path.
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
  debug workers use 8 MiB. The outer lowered evaluator still runs on a
  large-stack worker because the recursive eval frames remain large.
- The collection self-assignment specialization is split out behind a guarded
  `Set`-method helper so ordinary lowered assignments stay on the compact main
  statement path. This starts the frame-shrink direction, but it has not yet
  removed the outer evaluator's large-stack dependency or produced a clear
  tokei RSS win.
- `showcase/tokei.xsh`'s default table path returns final `SummaryRow` reduce
  rows from `par-map`, then does only `flat-map { |rows| rows } |> reduce-by`.
  Keep this script-level shape; it removed a large nested scan/report graph
  without adding evaluator fusion complexity.

**Runner flow & the "compact lowering not available" fallback** (`src/runner.rs`).
A script runs as: arena-only parse, drop CST, prepare/install compact lowered
plan, drop arena, then `Evaluator::eval_installed_compact_lowered_only`. If
preparation or evaluation returns the evaluator instead of output, the runner
falls back to the checker only to surface diagnostics. A clean check there
prints "compact lowering not available" (exit 1), which means a genuine lowering
gap rather than a user error. When a whole script is "not available", check the
arena-parse diagnostics in the runner first.
- **Large stack:** the lowered evaluator's recursive Rust fns have large frames
  (giant matches over `LoweredExpr`/`LoweredValue`), so eval runs on a worker
  thread with a 1 GiB stack (`run_eval_on_large_stack` in `eval.rs`). A panic in
  the worker is resumed on the main thread (not swallowed). The first scoped
  frame-shrink step moved collection self-assignment method updates out of the
  main statement match, but broader expression/statement splitting is still
  needed before reducing the outer stack reservation is defensible.
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
- **Measure with the existing perf machinery.**
- **Do not run formatters or autofixers.**

## Improvement Opportunities

These are durable compact/lowered-pipeline work items, not a backlog of
showcase-specific tweaks. The tokei objective is now past the easy wins; prefer
changes that reduce core value movement, retained runtime state, or frontend
memory across ordinary XSH programs.

1. **Shrink lowered eval frames.** Eval still runs on a 1 GiB-stack worker
   thread because recursive `eval_lowered_*` frames are large enough for the
   default stack to overflow after only a few levels. The durable fix is to make
   the hot recursive frames smaller: split cold match arms into `#[inline(never)]`
   helpers, box unusually large locals, or move deeply recursive paths to an
   explicit work stack. This is the item most likely to unlock broad runtime
   memory improvements because it could make stack sizing stop being
   load-bearing for the outer evaluator and future worker paths. Measure stack
   size, release RSS, and instruction deltas; do not rely on debug timing.

2. **Reduce lowered value movement and retained report graphs.** The remaining
   tokei gap is dominated by dynamic `LoweredValue` records/maps/lists, ordered
   `par-map` coordination, and final report/JSON construction rather than one
   missing scanner optimization. Good candidates are representation changes that
   apply to many scripts: cheaper small records, lower-clone method dispatch,
   more in-place collection updates when aliasing permits, or streaming JSON
   emission that does not retain extra script-visible state. Treat each as a
   measured release A/B; several narrower variants have already been rejected in
   `INTERPRETER-PERF.md`.

3. **Guard that lowering threads every behavior-bearing arena field.** A common
   failure mode is lowered IR silently omitting something the arena carries:
   fmt-string format specs, per-item stream errors, trace events, `exts:`,
   `--max-bytes`, or named method args. These usually produce wrong output
   rather than crashes, so "did it lower" coverage is not enough. Add targeted
   lowering parity tests or a structured review checklist around arena fields
   whenever adding or changing compact syntax rows.

4. **Reduce span storage only where retained metrics justify it.** XSH stores
   inline byte spans for every statement, expression, and type-expression row.
   A Zig-style `main_token`/token-range scheme could reduce frontend retained
   memory, but it must be applied selectively and measured with
   `xsh-layout-report` plus corpus retained/allocation metrics. Preserve explicit
   spans for cooked text, diagnostics, and cross-source module composition.
   This is more likely to improve frontend corpus metrics than the current
   runtime-heavy tokei benchmark.

5. **Close real compact construction gaps without recreating compatibility
   paths.** Compact-only is the baseline. Any remaining
   `compact_blocked_*`/`unconstructed_*` buckets from parse-corpus reporting are
   frontend completeness bugs, not a reason to revive recursive AST bridges.
   Recheck current reports before naming specific blocked files; older examples
   here have gone stale.

6. **Keep perf reports and builders compact-native.** Future perf schemas should
   name the compact pipeline state they actually measure and avoid
   compatibility-era phase keys. When adding direct arena construction APIs,
   watch `ArenaProgramBuilder` size with layout and corpus metrics; split cold
   staging state out if rare constructs grow the parser's hot staging object.
