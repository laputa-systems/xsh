# Front-End Pipeline Design

The front end is a compact, Zig-aligned pipeline for parsing, checking,
lowering, and execution. The design mirrors Zig's `Ast.zig` and `Parse.zig`
(see `~/d/zig/lib/std/zig/` for reference).

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

## Architecture

### Lexer / Tokens
- Dense `TokenTable` with tag, byte-start, and compact-payload columns behind a
  shared `TokenTableData`
- Token ends reconstructed from source on demand
- CST constructed from `TokenTable` directly
- Lexer output is compact-only

### Parser / Arena
- `AstArena` stores statement, expression, and type-expression rows as
  tag/data/span columns
- Parser-direct construction covers all statement, expression, pattern,
  command, and run forms
- Parse path returns `ArenaProgram` without constructing recursive
  `Program`/`Stmt`/`Expr` syntax trees
- Arena-only expression parsing returns `ExprId` directly for all forms

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
- Auto-main invokes lowerable `proc main(...argv)` without `FunctionDef`
- Standard-module `use` declarations are compact-skippable
- Proc commands, cd/env/print with propagate, signal hooks, loop, grouped
  run capture, par-map, tee, table.print lowered and executed
- Implicit variable declaration on assignment (auto-declare)
- Alternation patterns in match

**Runner flow & the "compact lowering not available" fallback** (`src/runner.rs`).
A script runs as: arena-only parse → `Evaluator::try_eval_compact_lowered_only`
→ if it returns `Err(self)`, fall back to running the checker to surface
diagnostics; a clean check there prints "compact lowering not available" (exit 1)
— i.e. a genuine lowering/parse gap, not a user error. So that message has TWO
sources: (a) `try_eval_*` returned `Err` (install produced an incomplete
`lowered_program`, or auto-main args couldn't bind), or (b) arena parsing/checking
produced diagnostics. When a whole script is "not available", check the
arena-parse diagnostics in the runner first.
- **Large stack:** the lowered evaluator's recursive Rust fns have large frames
  (giant matches over `LoweredExpr`/`LoweredValue`), so eval runs on a worker
  thread with a 1 GiB stack (`run_eval_on_large_stack` in `eval.rs`). A panic in
  the worker is resumed on the main thread (not swallowed).
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
    pub spans: Vec<ArenaByteSpan>,
    pub span_source_overrides: Vec<ArenaSpanSource>,
    pub stmt_tags: Vec<ArenaStmtTag>,
    pub stmt_data: Vec<ArenaStmtData>,
    pub stmt_spans: Vec<ArenaByteSpan>,
    pub stmt_span_source_overrides: Vec<ArenaSpanSource>,
    pub expr_tags: Vec<ArenaExprTag>,
    pub expr_data: Vec<ArenaExprData>,
    pub expr_spans: Vec<ArenaByteSpan>,
    pub expr_span_source_overrides: Vec<ArenaSpanSource>,
    pub type_expr_tags: Vec<ArenaTypeExprTag>,
    pub type_expr_data: Vec<ArenaTypeExprData>,
    pub type_expr_spans: Vec<ArenaByteSpan>,
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
    starts: Vec<u32>,
    payloads: Vec<TokenPayload>,
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
- Store inline `ArenaByteSpan { start, len }` on statement, expression, and
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

- `stmt_spans`, `expr_spans`, `type_expr_spans` — inline byte-span columns for
  hot node rows
- `stmt_span_source_overrides`, `expr_span_source_overrides`,
  `type_expr_span_source_overrides` — sparse source overrides for those inline
  span columns
- `spans: Vec<ArenaByteSpan>` — shared byte-span table for side tables that
  store `SpanId`
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

## Compatibility Cleanup Guardrails

`src/syntax/ast.rs` is intentionally gone. Do not restore `syntax::ast`, a
recursive `Program`/`Stmt`/`Expr` tree, or recursive compatibility leaves such
as the former call-argument and type-expression wrappers. `src/syntax/node.rs`
is for small non-recursive node payloads and shared syntax-side enums that the
arena can store directly.

Parser entry points should return compact IDs or `ArenaProgram` directly. Avoid
bridge names such as `parse_*_with_arena`; those imply a second syntax path.
Builder APIs should expose shape-specific insertion methods (`push_*_type_expr`,
`build_*`, `finish_*`) instead of generic lowering hooks from recursive nodes.

Consumers that need to walk annotations should read `ArenaTypeExprTag` and
`ArenaTypeExprData` from the owning arena. If a local convenience enum is useful,
name it as an arena view and keep it private to that consumer; do not add a
public recursive syntax model or a `type_from_ast`/`raise_type_expr` bridge.

Loader/tooling APIs should describe what they load and check (`text`, `bytes`,
`file`, `entry`) rather than revive historical compatibility wording such as
`CheckedProgram` or `parse_load_check_program*`. `Program` remains fine where it
names a real compact or lowered program type, such as `ArenaProgram` or
`LoweredProgram`.

## Non-Negotiables

- **Be aggressive.** Break things and fix them.
- **Do not reintroduce recursive AST compatibility.** Prefer compact arena/CST/
  lowered-IR consumers over compatibility aliases or bridge reconstruction.
- **Avoid strings and stringly typed logic.** Use `Name`, typed IDs, enums,
  `RuntimeOp`.
- **Measure with the existing perf machinery.**
- **Do not run formatters or autofixers.**

## Measurement Gates

```sh
cargo check --no-default-features
cargo test --test runtime <filter> --no-default-features
RUST_MIN_STACK=16777216 cargo test --test runtime --no-default-features
git diff --check
cargo run --quiet --bin xsh-parse-corpus-report --no-default-features \
  --features "tools perf-metrics" -- --root . --repeat 1
make prof-baseline-frontend
```

`xsh-parse-corpus-report` is the boundary-readiness report. In addition to the
phase allocation/timing data, it emits:

- `per_file_counts`: bytes, tokens, statements, imports, exports, declarations,
  and executable top-level statements per file.
- `per_phase_file_summaries`: total, p50, p95, and max per-file timing for
  parse, declaration probe, body probe, lowering probes, and runtime declaration
  registration.
- `module_graph_readiness`: import edges, unique modules, qualified declaration
  count, duplicate diagnostics, and largest dependency component.
- `function_lowering_readiness`: attempted/lowered/blocked function counts,
  dependency edges, SCC count, blocker counts, and qualified vs unqualified call
  counts.
- `top_level_readiness`: lowered/skipped/blocked top-level statement counts and
  fallback reason counts.

`make prof-baseline-frontend` runs inside the Linux Docker profiling container
and writes `perf/*-baseline-$(uname -m).json`. On Apple Silicon that means the
`aarch64` baseline is a Linux baseline, not a macOS one. Compare baselines only
when target OS, target arch, profile, repeat count, and corpus root match.

The `unix::` spawn tests leak a child that holds the stdout pipe open, so a
foreground `cargo test --test runtime` can appear to hang AFTER the `test
result:` line prints. Wait for that line, then `pkill -f deps/runtime`.

Do not run formatters or autofixers.

## Improvement Opportunities (durable design, from deep internals work)

Opinionated, grounded in concrete friction. These concern the front-end's
durable design and are separate from any single cleanup work order. Roughly by
leverage.

1. **Shrink the eval frame instead of the 1 GiB stack band-aid.** Eval runs on a
   1 GiB-stack worker thread because each recursive frame in `eval_lowered_*` is
   ~1 MB (giant matches over ~100-variant enums), so the default stack overflows
   after a handful of recursion levels. That caps real recursion depth and is
   fragile. The durable fix is boxing large locals / `#[inline(never)]` on cold
   match arms (or an explicit work-stack / trampoline for the deeply recursive
   eval paths), so depth scales and the stack size stops being load-bearing.

2. **Guard that lowering threads every behavior-bearing arena field.** A
   recurring shape: the lowered IR omits something the arena carries, with no
   signal — fmt-string format specs (`${x:>4}`) were a `_`-ignored field;
   per-item stream errors and several trace events were simply never emitted;
   `exts:`/`--max-bytes`/named method args were dropped. These surface only as
   wrong *output* (not a crash), so a "did it lower" metric never catches them.
   Worth a lint or structured checklist that each behavior-bearing arena field is
   carried through lowering; the showcase scripts are the best detector for these
   regressions.

3. **Reduce span storage with Zig-style main-token derivation where it pays.**
   Zig's `Ast` stores node `tag`, `main_token`, and compact `data`, then derives
   first/last tokens and token slices from the token table and `extra_data`. XSH
   still stores inline byte spans for every statement, expression, and
   type-expression row. That is simpler and robust, but it is the largest visible
   gap from the Zig model. A measured next step is to add `main_token` or
   token-range fields only for hot node families where `xsh-layout-report` and
   corpus retained/allocation metrics show span columns are real cost, while
   preserving explicit spans for cooked text, diagnostics, and cross-source
   module composition.

4. **Close the remaining compact hot-path blockers.** The post-AST frontend
   baseline is compact-only, but the corpus still has a small blocked bucket
   (`compact_blocked_*` in `xsh-parse-corpus-report`) where lowering cannot
   construct every executable top-level/function body. As of the post-AST cleanup
   this is concentrated in user-module `use` declarations and a handful of
   function-body blockers in `core/getty.xsh`, `core/passwd.xsh`, `core/su.xsh`,
   and the qualified-helper perf scenarios. These are not recursive-AST fallbacks,
   but they are the next frontend completeness target.

5. **Make frontend perf output schema stable and compact-native.** The old
   parse-corpus report used to carry recursive-path phase names and misleading
   "old AST required" counters. Keep future report fields named for the compact
   pipeline state they actually measure (`compact_blocked_*`,
   `unconstructed_*`, `unsupported_*`) and avoid adding compatibility-era phase
   keys back into checked baselines.

6. **Watch builder size after adding direct arena construction APIs.**
   `ArenaProgramBuilder` is intentionally the parser's staging object, but it is
   easy to grow by adding per-kind staging vectors. When adding `begin_X`/
   `push_X`/`finish_X` groups, check both `xsh-layout-report` and the corpus
   retained/allocation metrics; split cold staging state out if the builder
   starts growing for constructs that are rare in ordinary scripts.
