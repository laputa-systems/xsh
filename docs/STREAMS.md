# Structured Streams — Architecture & Performance

How the `|>` structured pipeline executes, and the performance model behind it.
`examples/streams.xsh` is the curated composition showcase;
`tests/xsh/stdlib/streams.xsh` owns focused behavior coverage.

## 1. Surface

A structured pipeline is `source |> stage |> … |> stage`. Value stages use the
same `|>` spelling but are lowered to ordinary calls: a bare method name takes
the preceding value as its receiver, while a qualified call takes it as its
first argument. A trailing `?` remains an explicit Result propagation step.
Three result shapes:

- ends in a non-terminal stage → a **`List[T]`** (items collected at the boundary);
- ends in `collect()` → a **`List[T]`** (explicit materialization);
- ends in another **terminal** stage → a **scalar** (`count`/`sum`/`min`/`max`/
  `first`/`last`/`any`/`all`/`fold`/`reduce`, and `reduce-by` → a `Map`,
  `table.print` → `Unit`);
- consumed by **`for x in pipeline { … }`** → supported serial stages hand each
  output row to the loop body before pulling the next source row. Stages that
  require materialization keep their ordinary expression boundary. A raw
  script producer is also pulled by the loop one item at a time.

The verified indexed pipeline is executed by `FullTag::ExprPipeline` in
`src/runtime/eval/lowered_run/indexed_run.rs`. Live serial prefixes are driven
by `src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs`.
`src/runtime/eval/stream.rs` owns source pulls and script-producer cancellation.

## 2. Live execution in the indexed runtime

A `StreamValue` carries any already materialized prefix plus an optional live
source or suspended script producer. `stream_next` pulls one value; a script
producer resumes in the current evaluator. `stream_cancel` closes a script
producer that a consumer stops early and runs its defers once.

For a live source, `serial_pipeline.rs` runs supported serial stages on each
source item before pulling the next. `flat-map` sends each expanded value through
the remaining stages in order, so `take` can stop within an expansion. The
serial path covers `tee`, `where`, `map`, `flat-map`, `drop`, and `enumerate`; it
stops at `take`, `first`, `any`, or `all`, and collects at an explicit `collect()`
or expression boundary. An unsupported stage receives the materialized serial
prefix, then follows its indexed handler. This preserves effects and late-error
timing for the supported prefix, including producer cleanup and trace exits on
failure.
The ordinary indexed handlers also close their `stream.stage` trace when a
stage errors or propagates a value early.

`fold`, `reduce-by`, `each`, keyed `count`, `group-by`, `unique-by`, `sum`,
`last`, `min`, and `max` also consume live input one row at a time and cancel
their producers on errors. A non-`Int` item in `sum` or a key error in keyed
`count`/`group-by`/`unique-by` fails before the next source pull. `last`, `min`,
and `max` retain one candidate value while they drain the input. `zip(other)`
collects its right list or stream first, then pulls only paired left items and
cancels the left producer if the right side ends first. Its result is still a list.
Stages that need a complete result (`sort`, `sort-by`, `shuffle`, `collect`,
`table.print`, positive `repeat`, and `batch`) retain their materialization
boundary. `par-map` retains its worker boundary.
`sort-by --desc=expr` evaluates that option before draining a live source and
before running key projections.
The size-limited `batch` handlers consume live input one item at a time;
`batch --max-bytes` closes the producer on an oversized item without pulling
the following item.
`repeat(0)` cancels a live source without pulling an item.
`FullTag::StmtFor` in `indexed_run/explicit_run.rs` keeps a supported serial
pipeline in `FrameWork::ForPipeline`, with its input evaluated once and its
stage counters and flat-map expansion retained across loop-body executions.
`break`, `continue`, errors, and bounded `take` use that cursor's cleanup path.
An unsupported stage follows the ordinary expression path; a raw script
producer uses `FrameWork::ForStream`. A script producer that returns another
stream delegates later pulls and cancellation to that returned stream in
`indexed_run/producer.rs`. An unsupported loop stage follows its ordinary
expression boundary. Parallel stages keep their separate materialization and
worker boundaries described below.

## 3. The filesystem walk

`fs.walk`/`files`/`dirs` →
`walk_filesystem(root, gitignore, stat, hidden, emit)`
(`src/modules/fs.rs`). `WalkEmit::{All,Files,Dirs}` gates which records leave the
producer; visible directories are descended by default, while dot-prefixed child
entries are skipped unless `hidden: true` is set. `stat: false` skips the
per-entry `stat`; stat-derived fields are unavailable and reading them returns
a `metadata-unavailable` runtime error. `fs.files(..., exts: [...])` filters
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

The indexed `reduce-by` handler folds serially. For a live source, it reduces
each row before pulling the next one, uses O(distinct) group storage, and closes
the producer when reduction fails. The accepted `--jobs=N` option is currently
evaluated once and validated before the fold, but it does not start reduce
workers. Adjacent `par-map |> reduce-by` may fuse into worker-local aggregation
when the `par-map` stage supplies the workers; an explicit `reduce-by --jobs`
keeps the ordinary reduction stage.

### Parallelism boundaries

The indexed `group-by` and keyed `count { block }` handlers also run serially;
they and plain `count` reject `--jobs`. On live input they evaluate each key
before the next pull; `group-by` retains its grouped items, while keyed `count`
retains one count per key. The stages below are serial as well:

- **Order-sensitive** (`take`/`drop`/`first`/`last`/`enumerate`/`unique-by`/`zip`/
  `batch`) — splitting changes the result.
- **`fold`/`reduce`** — a sequential user combine with no merge function.
- **Side-effecting** (`each`/`tee`) — their effects follow input order.
- **`map`/`where`/`flat-map`** — per-item independent, but mid-pipeline they'd have
  to materialize (can't partition a live stream) and the per-item work is usually
  too cheap to beat coordination overhead. Use `par-map` for the heavy-item case.
- **`sum`/`count`/`min`/`max` with no block** — per-item work is nil; the cost is
  an upstream `map`, not the terminal.

`par-map` is the explicit worker stage. The filesystem walk starts its own
parallel traversal when pulled.

## 5. `par-map` and adapters

- **`par-map`** (`--jobs=N` optional) materializes the lazy source, then maps
  items on bounded workers. It defaults to the available CPU count capped at
  `DEFAULT_PAR_MAP_WORKERS`; `--jobs=N` overrides that limit. Output retains
  input order. `each` runs serially and rejects `--jobs`. Use
  `par-map` for heavy independent per-item work.
- **Result handling.** `par-map` does not unwrap `Result` return values — the
  block's return type flows through unchanged. Use `?` inside the block for
  short-circuit-on-first-error semantics (errors propagate out-of-band). Omit `?`
  for collect-all semantics (`Result` values, including `Err`, stay in-band in the
  output stream). This mirrors how Rust's rayon, Go, and Haskell separate
  parallelism from error handling.
- **Aggregation fusion.** With tracing disabled, adjacent `par-map |> reduce-by`
  fuses into worker-local partial maps. Its simple record sums use the same
  field projection as ordinary `reduce-by`. A measured attempt to carry
  `where`/`map`/`flat-map` suffix stages into that fusion regressed the
  `showcase/tokei.xsh` workload, so non-adjacent shapes keep the ordinary
  materialized path for now. An explicit `reduce-by --jobs` keeps the ordinary
  reduction stage, so its option expression runs once at that boundary.
- **Adapters** (`text.lines`/`bytes.chunks`/`json.lines`/`json.stream`) are valid
  only as the first stage; they convert a value into the stream the rest consumes.

## 6. Performance model

The pipeline is a **single-threaded tree-walking interpreter over boxed heap
`Value`s** unless an explicit `--jobs`/parallel-walk path engages. The cost is
interpreter dispatch + heap traffic, **not** memory bandwidth or cache layout —
there is no contiguous columnar buffer to vectorize. Levers applied (all landed):

- **`Value` is 48 bytes** on the supported 64-bit targets (was 216).
  `Error`/`RunError`/`Command` payloads are
  boxed; every value move/clone was `memmove`ing the largest variant.
- **Shaped records share a dense field slice** via `Arc<[Value]>` — clone = a
  refcount bump, not a copy. Their field labels are interned `Name` identities
  in a process-local shape cache; borrowed lookup and updates to existing fields
  retain the dense representation, while adding a new field deliberately moves
  the record to the dynamic map path.
- **Function defs are `Arc<FunctionDef>`** — a call no longer deep-clones the body
  AST; also makes evaluator forks cheap.
- **String literals are `Arc<str>` in the AST** — evaluating a literal is a bump,
  not a fresh allocation (a `where` predicate's `"file"`/`""` no longer allocate
  per item).
- **Scope maps are pooled** — `push_scope` recycles a cleared `HashMap`.
- **Cheaper hot helpers**: zero-alloc directory sort (compare borrowed bytes, not
  `sort_by_key` re-running an allocating key fn); `translate`/`Str.lower` ASCII
  byte scan with no per-call `Vec<char>`.

The live terminal memory probe is `bench/stream-terminal-memory.xsh`. On macOS
ARM64 with the debug `xsh`, one million `Int` values from a script producer gave
these single-run `/usr/bin/time -l` peak RSS samples (bytes). `count` is the
unchanged streaming control; all four runs returned the expected value.

| Terminal | Before bounded fold | After bounded fold |
|---|---:|---:|
| `count` | 54,050,816 | 54,444,032 |
| `last` | 88,260,608 | 54,493,184 |
| `min` | 88,195,072 | 54,509,568 |
| `max` | 88,145,920 | 54,525,952 |

`bench/stream-producer-memory.xsh` isolates a script producer whose loop
allocates no input list. The raw macOS and pinned Linux debug RSS samples at
100,000 and 300,000 rows are in `bench/stream-producer-memory-2026-09-24.json`.
Direct `first`, `take(1) |> par-map |> first`, and `count` stay near their
per-process baseline as the row count grows. `par-map |> first` and unfused
`par-map |> reduce-by` retain a row-count-sized intermediate; fused reduction
still stages all input rows but avoids the mapped output list. Put `take` before
`par-map` when the producer must stop early. The producer cleanup and pull
counts are asserted by `tests/fixtures/runtime/worker-stage-producers.xsh`.

### Choosing a parallel strategy

An adjacent `par-map |> reduce-by` folds mapped records on worker threads.
A *fused* parallel walk (walk workers run the pipeline + fold inline)
was built and **measured slower on flat trees** — one huge directory is processed
by a single worker while the rest idle — so it was removed. Record-partitioning
avoids that tree-shape problem; intra-directory work-splitting (batching a large directory's
entries onto the work-stack) would be required before per-directory parallelism
could win on flat trees. See §7 pitfalls.

## 7. Pitfalls (and the user-facing guidance)

- **`group-by` then aggregate buffers everything (O(N)).** For a per-key
  count/sum, use `reduce-by` (O(distinct)) or `count { key }`.
- **Order isn't free.** Recursive walks are unordered/parallel; `take`/`first`/
  `last` over them are nondeterministic. Add `|> sort-by` when order matters.
  `sort-by`/`sort` are stable and order `Int`/`Str`/`Bool`/`Path` keys and
  records (field by field in sorted field-name order); unsupported key types
  fail loudly instead of silently leaving the stream unsorted.
- **Don't wrap trivial work in a `pure`.** A per-item user-function call pays
  scope + dispatch overhead; prefer a builtin (`.lower()` over a `translate`
  helper) or an inline block.
- **`par-map` is for heavy items only.** For cheap aggregation use `reduce-by`;
  for cheap mapping, plain `map` (the parallel coordination would lose).
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
