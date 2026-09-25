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
stops at `take`, `first`, `any`, `all`, or `count`, and collects at an explicit
`collect()` or expression boundary. `count` keeps only its running total and
enters its trace before pulling the source. An unsupported stage receives the
materialized serial prefix, then follows its indexed handler. This preserves
effects and late-error timing for the supported prefix, including producer
cleanup and trace exits on failure.
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

- **Serial, lazy.** Recursive walks use `ignore::WalkBuilder` and a single
  `ignore::Walk` iterator. `gitignore: true` enables `.gitignore`, `.ignore`,
  `.fdignore`, global gitignore, and git exclude files without requiring the
  root to be inside a git worktree. Traversal starts on first `next()`.
  Records follow filesystem traversal order, which is not sorted.

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
workers. `par-map |> reduce-by` and `par-map |> flat-map |> reduce-by` with an
identity flattening block may fuse into worker-local aggregation when `par-map`
supplies the workers; an explicit `reduce-by --jobs` keeps the ordinary stage.

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

`par-map` is the explicit worker stage. The filesystem walk starts serial
traversal when pulled.

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
- **Aggregation fusion.** With tracing disabled, direct `par-map |> reduce-by`
  and `par-map |> flat-map |> reduce-by` with an identity flattening block
  fuse into worker-local partial maps. Simple record sums use the same field
  projection as ordinary `reduce-by`. A measured attempt to carry other
  `where`/`map`/`flat-map` suffix stages into that fusion regressed the
  `showcase/tokei.xsh` workload, so those shapes keep the ordinary materialized
  path. An explicit `reduce-by --jobs` keeps the ordinary reduction stage, so
  its option expression runs once at that boundary.
  Keep eligible fusion as the default: on a 20,000-file flat corpus it used
  about 26% less peak RSS on macOS and 33% less on pinned Linux. Ten paired
  release runs showed about 2.5% slower median wall time on macOS and a tie
  within Linux's 10 ms timer resolution. `--jobs` remains the opt-out when
  throughput matters more than peak memory. Raw samples and exact output
  parity are in `bench/stream-fusion-large-corpus-a04-2026-09-24.json`.
- **Adapters** (`text.lines`/`bytes.chunks`/`json.lines`/`json.stream`) are valid
  only as the first stage; they convert a value into the stream the rest consumes.

## 6. Performance model

The pipeline is a **single-threaded tree-walking interpreter over boxed heap
`Value`s** unless an explicit `par-map` or `reduce-by --jobs` stage engages. The
cost is interpreter dispatch + heap traffic, **not** memory bandwidth or cache layout —
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
- **Indexed lexical bindings use slots** — the remaining evaluator scope map
  stores `Name` symbol IDs in `FxHashMap<Name, Binding>`, avoiding a string-key
  allocation and SipHash on each bind.
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

`bench/stream-stage-cost.xsh` compares direct live `count` with identity `map`,
a cheap `where`, their combination, and a `par-map` control. On one million
items, moving `count` into the live serial cursor reduced median `map` peak
RSS from 47.3 to 13.2 MB on macOS and 80.3 to 15.2 MB on pinned Linux. Median
wall time was 862 to 862 ms on macOS and 895 to 880 ms on Linux.
`where` and `map |> where` saw similar memory reductions; the worker control
was unchanged. Eight alternating release pairs per mode and host, exact output
parity, and raw samples are in `bench/stream-stage-cost-c01-2026-09-24.json`.
These small expressions bound total per-item cost but do not isolate dispatch
from expression evaluation, so they do not justify a compiled stage path.

`showcase/loc.xsh` is a whole-file scanner, but replacing
`Path.read_text()?.count_lines()` with `Path.lines()? |> count()` did not help
on `src` (128 Rust files, 123,720 lines). Twenty alternating macOS ARM64
release pairs had 22.883 ms versus 33.016 ms median wall time. Empty, CRLF,
unterminated, and Unicode files kept the same counts. A late invalid UTF-8 byte
still failed with status 3 and no output, but changed the diagnostic operation
from `result.propagate` to `runtime.error`. Keep the whole-file count for this
workload; `bench/stream-line-count-c06-2026-09-24.json` holds raw samples and
the exact candidate expression. The candidate did lower median peak RSS from
17.7 MB to 15.8 MB in five paired runs. Revisit line-state scanning only for a
measured large-file workload where bounded memory offsets per-line stream
overhead and the error path can be preserved.

`showcase/perf-collapse.xsh` was tested separately on a 10.79 MB perf-script
input with 30,000 samples. Passing `Path.lines()` to the fold produced identical
valid output, but raised median wall time from 1587.886 to 1599.695 ms and
median peak RSS from 36.29 to 36.93 MB. A late invalid UTF-8 byte still
produced no output and status 3, but changed the error from a propagated
file-read result to a stream runtime error. The candidate was rejected;
`bench/perf-collapse-lines-c06-2026-09-24.json` has the raw samples. Other
whole-file readers retain useful boundaries: `core/rg.xsh` emits matches as it
goes, so a live read could print partial results before a late decode failure;
the per-file scanner tasks in `showcase/secret-scan.xsh` and
`showcase/todo-scan.xsh` discard a file's hits when its read fails; and
`showcase/csv-query.xsh` materializes rows for sorting and grouping. These
two measured rejections close the current scanner-conversion experiment.

### Choosing a parallel strategy

An adjacent `par-map |> reduce-by` folds mapped records on worker threads.
A *fused* parallel walk (walk workers run the pipeline + fold inline)
was built and measured slower on flat trees, where one large directory kept
only one worker busy, so it was removed. The current serial `fs.walk` traversed
20,000 empty files with `stat: true` faster in one flat directory than in 100
directories on both hosts: 124 versus 131 ms median on macOS, and 100 versus
120 ms on pinned Linux (10 paired release runs per host). Peak RSS was about
83 MB for both shapes on macOS and 83 versus 85 MB on Linux. Exact counts and
raw samples are in `bench/fs-walk-shape-c04-2026-09-24.json`. Keep the serial
walker until a real workload demonstrates a traversal bottleneck; this shape
comparison does not justify intra-directory work splitting.

## 7. Pitfalls (and the user-facing guidance)

- **`group-by` then aggregate buffers everything (O(N)).** For a per-key
  count/sum, use `reduce-by` (O(distinct)) or `count { key }`.
- **Order isn't free.** Recursive walks follow unsorted traversal order; `take`/`first`/
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
  when `where` discards it. `raw_walk_entry` now moves the owned
  `ignore::DirEntry` path into the walk item instead of cloning it. This removed
  20,001 execution-thread allocations and about 1.36 MB of allocation traffic
  on a 20,000-file flat walk. Peak RSS did not move; paired latency was mixed
  on both hosts, so this is an allocation result, not a throughput claim. Raw
  samples are in `bench/fs-walk-path-ownership-c02-2026-09-24.json`. On the
  rejecting `src` walk in
  `bench/fs-walk-rejection-c02-2026-09-24.json`, `stat: true` took 18.471 ms and
  allocated 811 KB in the execution thread, versus 16.917 ms and 101 KB with
  `stat: false` (15 paired release timings, three allocation runs). The latter
  also skips metadata reads and changes field errors, so it is an upper bound
  rather than an equivalent replacement. An eager metadata read followed by
  lazy field construction would need separate evidence on a larger tree and
  must preserve metadata-error timing.
