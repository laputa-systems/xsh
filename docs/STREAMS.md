# Structured Streams — Architecture & Performance

How the `|>` structured pipeline executes, and the performance model behind it.
This is the maintainer reference; the user-facing guide is
`docs/CHAPTER-08-structured-streams.md`.

## 1. Surface

A structured pipeline is `source |> stage |> … |> stage`. Three result shapes:

- ends in a non-terminal stage → a **`List[T]`** (items collected at the boundary);
- ends in `collect()` → a **`List[T]`** (explicit materialization);
- ends in another **terminal** stage → a **scalar** (`count`/`sum`/`min`/`max`/
  `first`/`last`/`any`/`all`/`fold`/`reduce`, and `reduce-by` → a `Map`,
  `table.print` → `Unit`);
- consumed by **`for x in pipeline { … }`** → iterated, never materialized.

The driver is `eval_structured_pipeline` → `build_pipeline` → `eval_stream_stage`
per stage (`src/runtime/eval/stream.rs`).

## 2. Two execution paths: eager vs lazy

`StreamPipelineValue` has three states: `Stream(StreamValue)` (materialized),
`Lazy(LazyPipeline)` (a live source + fused per-item ops, not yet run), and
`Value` (a terminal scalar / adapter input).

- **Eager.** A materialized source (a `List`, or an already-collected stream)
  flows through each stage's vec handler, producing a new `Vec<StreamItem>`.
- **Lazy.** A *live* source (`is_live()`: `fs.walk`/`files`/`dirs`, `fs.mounts`,
  file/text/byte line streams, `run.stream`, archive listings, process and
  Linux snapshot streams, user `stream` producers, and device/uevent streams)
  is wrapped in `LazyPipeline { source, ops }`. Lazy-class stages **fuse** onto
  `ops` doing no work; the first materializing/terminal stage drains the pipeline
  once.

`value_to_pipeline` decides which: a live `Stream` → `Lazy`, else → `Stream`.

### Lazy machinery

- `LazyOp` — the fused per-item forwarding ops, with per-pipeline state:
  `Where`/`Map`/`FlatMap`/`Tee` (own a cloned `StreamStage`), `Enumerate{count}`,
  `Take{remaining}`, `Drop{remaining}`.
- `drive_lazy(ops, item, sink)` — **push-based**: recursively pushes one source
  item through `ops`, invoking `LazySink::accept` per survivor. Returns
  `PullControl::Stop` to halt the source pull (how `take`/`first` short-circuit).
- `drain_lazy(pipe, sink, span)` — pulls the source (materialized prefix, then the
  live tail via `next_live`) through `drive_lazy` into a sink, stopping on `Stop`.
- Sinks (`LazySink`): `MaterializeSink` (→ `Vec`), `ForLoopSink` (runs the loop
  body; `break` → `Stop`), and the terminal sinks below.

This is **consumer-driven**: every block runs inside `&mut Evaluator`, so there is
no second copy of stage logic — the per-item helpers (`where_keep`, `stage_block_
value`, `flat_map_values`, `reduce_by_step`, `fold_step`, …) are shared by the
eager vec handlers and the lazy sinks.

### Terminals on a lazy pipeline

- **Short-circuit** (`take`/`first`/`any`/`all`): drive the source and stop early
  (`FirstSink`, `AnyAllSink`). For an infinite live source this is the only
  correct path — never materialize.
- **Folding** (`count`/`sum`/`min`/`max`/`last`/`fold`/`reduce`): fold one item at
  a time into O(1) state (`CountSink`, `SumSink`, …) — no materialization. So
  `fs.walk |> where … |> count()` is O(1) live memory.
- **Explicit materialization** (`collect()`): drain to a `List[T]`, equivalent
  to the automatic collection that happens when a non-terminal pipeline reaches
  an expression boundary.
- **Materializing** (`group-by`/`sort-by`/`unique-by`/`shuffle`, and `batch`/`zip`
  semi-lazy, and `par-map`): drain to a `Vec` first, then run the eager handler.
  `flat-map` can consume a live stream returned by its block, but that nested
  stream is drained for the current input item before the outer stream advances.

### Materialize-on-bind invariant

A `LazyPipeline` **never escapes as a value**. `pipeline_into_value` materializes
any still-lazy pipeline to a `List` at the expression boundary, and the for-loop
consumer (`eval_pipeline_for`) drives it in place. So `let x = <pipeline>` is a
plain reusable value; laziness only applies to consume-in-place. Live *sources*
(walk, uevent) are single-use, like every live stream.

## 3. The filesystem walk

`fs.walk`/`files`/`dirs` →
`walk_filesystem(root, gitignore, stat, hidden, emit)`
(`src/modules/fs.rs`). `WalkEmit::{All,Files,Dirs}` gates which records leave the
producer; visible directories are descended by default, while dot-prefixed child
entries are skipped unless `hidden: true` is set. `stat: false` skips the
per-entry `stat` (zeroes size/mode/time). `fs.files(..., exts: [...])` filters
child files by raw extension before `stat` and record construction, while still
traversing directories so matching files deeper in the tree can be reached.

- **Parallel, unordered.** Recursive walks use `ignore::WalkBuilder` /
  `WalkParallel`, including its per-thread depth-first deques and cross-thread
  stealing. `gitignore: true` enables `.gitignore`, `.ignore`, `.fdignore`,
  global gitignore, and git exclude files without requiring the root to be inside
  a git worktree. **Lazy-start:** the worker pool starts on first `next()`.
  Records arrive in **completion order, not sorted**.

Consumers needing deterministic order use `|> sort-by .path`.

## 4. `reduce-by` — streaming grouped aggregate

`… |> reduce-by --sum|--min|--max [--jobs=N] { |item| {key: K, value: V} }` →
a `Map` of key → reduced value. It keeps **one accumulator per key**
(O(distinct) live), unlike `group-by` which buffers every item per group (O(N)).
`--sum` adds `Int`s/`Float`s or two records **field-wise**, so a count+size
aggregate is one pass:

```
|> reduce-by --sum { |e| {key: e.ext.lower(), value: {count: 1, size: e.size}} }
```

The fold partitions the materialized items into chunks, folds each on a worker fork
into a private map, and merges with the same (associative) reducer. It is
**parallel by default** (one worker per CPU, like the walk); `--jobs=N` overrides
the worker count and `--jobs=1` forces the serial fold. The merge is
order-independent (exact for `Int`; `Float` sum reorders, a known caveat). Below
`PARALLEL_FOLD_MIN_ITEMS` it stays serial regardless.

Caveat for the parallel default: over a *live* source (the walk) the fold first
materializes the post-`where` items into a vec, so memory is O(N) there rather than
the serial fold's O(distinct) streaming — `--jobs=1` restores O(distinct).

### What parallelizes by default — and what can't

Default-parallel is reserved for **associative folds** (partition → private
accumulator → merge), where it neither changes results nor ordering: `reduce-by`,
`group-by`, and keyed `count { block }`. Each partitions into contiguous chunks and
merges in chunk order, so the result — including `group-by`'s first-seen key order
and the encounter order of items within each group — is identical to serial.
Deliberately *not* parallel by default:

- **Order-sensitive** (`take`/`drop`/`first`/`last`/`enumerate`/`unique-by`/`zip`/
  `batch`) — splitting changes the result.
- **`fold`/`reduce`** — a sequential user combine with no merge function.
- **Side-effecting** (`each`/`tee`) — parallel runs interleave output.
- **`map`/`where`/`flat-map`** — per-item independent, but mid-pipeline they'd have
  to materialize (can't partition a live stream) and the per-item work is usually
  too cheap to beat coordination overhead. Use `par-map` for the heavy-item case.
- **`sum`/`count`/`min`/`max` with no block** — per-item work is nil; the cost is
  an upstream `map`, not the terminal.

Threads are cheap to spawn, but coordination/merge, determinism, and
streaming-memory are not — so parallelism is a default only where it's a clean win.

## 5. `par-map` and adapters

- **`par-map`** (`--jobs=N` optional) and **`each --jobs=N`**: a materializing boundary that drains
  the lazy source to a vec, then runs the block on a **fixed pool of N long-lived
  workers** pulling work by atomic ordinal, results collected by index for
  deterministic ordering. Bare `par-map` uses one worker per CPU; `--jobs=N`
  overrides that. Bare `each` remains serial, so side-effect parallelism stays
  explicit. These stages are for *heavy* per-item work (e.g. spawning subprocesses)
  — not cheap functions over a large stream. (The original per-item thread-spawn
  was ~9× slower; the worker pool fixed that.)
- **Result handling.** `par-map` does not unwrap `Result` return values — the
  block's return type flows through unchanged. Use `?` inside the block for
  short-circuit-on-first-error semantics (errors propagate out-of-band). Omit `?`
  for collect-all semantics (`Result` values, including `Err`, stay in-band in the
  output stream). This mirrors how Rust's rayon, Go, and Haskell separate
  parallelism from error handling.
- **Aggregation fusion.** With tracing disabled, adjacent `par-map |> reduce-by`
  fuses into worker-local partial maps. A measured attempt to carry
  `where`/`map`/`flat-map` suffix stages into that fusion regressed the
  `showcase/tokei.xsh` workload, so non-adjacent shapes keep the ordinary
  materialized path for now. Explicit `reduce-by --jobs` also keeps the ordinary
  path.
- **Adapters** (`text.lines`/`bytes.chunks`/`json.lines`/`json.stream`) are valid
  only as the first stage; they convert a value into the stream the rest consumes.

## 6. Performance model

The pipeline is a **single-threaded tree-walking interpreter over boxed heap
`Value`s** unless an explicit `--jobs`/parallel-walk path engages. The cost is
interpreter dispatch + heap traffic, **not** memory bandwidth or cache layout —
there is no contiguous columnar buffer to vectorize. Levers applied (all landed):

- **`Value` is 56 bytes** (was 216). `Error`/`RunError`/`Command` payloads are
  boxed; every value move/clone was `memmove`ing the largest variant.
- **Shaped records share their value vector** via `Arc<Vec<Value>>` — clone = a
  refcount bump, not a copy (records flow through `where`/`map`/fold).
- **Function defs are `Arc<FunctionDef>`** — a call no longer deep-clones the body
  AST; also makes evaluator forks cheap.
- **String literals are `Arc<str>` in the AST** — evaluating a literal is a bump,
  not a fresh allocation (a `where` predicate's `"file"`/`""` no longer allocate
  per item).
- **Scope maps are pooled** — `push_scope` recycles a cleared `HashMap`.
- **Cheaper hot helpers**: zero-alloc directory sort (compare borrowed bytes, not
  `sort_by_key` re-running an allocating key fn); `translate`/`Str.lower` ASCII
  byte scan with no per-call `Vec<char>`.

### Choosing a parallel strategy

`reduce-by --jobs` partitions **records** evenly, so it parallelizes regardless of
tree shape. A *fused* parallel walk (walk workers run the pipeline + fold inline)
was built and **measured slower on flat trees** — one huge directory is processed
by a single worker while the rest idle — so it was removed. Record-partitioning is
the robust default; intra-directory work-splitting (batching a large directory's
entries onto the work-stack) would be required before per-directory parallelism
could win on flat trees. See §7 pitfalls.

## 7. Pitfalls (and the user-facing guidance)

- **`group-by` then aggregate buffers everything (O(N)).** For a per-key
  count/sum, use `reduce-by` (O(distinct)) or `count { key }`.
- **Order isn't free.** Recursive walks are unordered/parallel; `take`/`first`/
  `last` over them are nondeterministic. Add `|> sort-by` when order matters.
- **Don't wrap trivial work in a `pure`.** A per-item user-function call pays
  scope + dispatch overhead; prefer a builtin (`.lower()` over a `translate`
  helper) or an inline block.
- **`par-map` is for heavy items only.** For cheap aggregation use `reduce-by
  --jobs`; for cheap mapping, plain `map` (the parallel coordination would lose).
- **`let`-binding a pipeline materializes it.** Laziness/short-circuit only apply
  to consume-in-place (`for`, or a terminal in the same expression). Use
  `collect()` when that materialization should be explicit in the pipeline.
- **Nested streams in `flat-map` are per-item drains.** This composes correctly
  with live streams, but it is not a fully interleaved nested lazy pipeline.
- **Whole-buffer scanners stay whole-buffer.** `Path.bytes_lines()` gives scripts
  a byte-safe file line source, but existing scanners written around `Bytes`
  prechecks such as `.contains()` and `.count_lines()` still read the whole file
  until they are refactored to line-state APIs.

## 8. Remaining levers (not done)

- **Lazy/columnar walk records** — the walk builds a full record per entry even
  when `where` discards it; only `path` (a `Vec<u8>`) is the wasted allocation for
  the kept-everything case. Needs a lazy-field record representation.
- **Per-bind scope key allocation + SipHash** — needs `Arc<str>` param names
  (a wider AST change) or a faster hasher.
- **Intra-directory parallel walk splitting** — to make a parallel/fused walk win
  on flat trees (see §6).
- **Bytecode/compiled stage blocks** — cut per-item dispatch; helps the serial
  path and every parallel worker, with no determinism cost.
