# Interpreter Performance: tokei.xsh

## Objective

Get `showcase/tokei.xsh` wall-clock time on the Sentry corpus within **1.2× of
native tokei** (speed target: ≤ 840ms), and peak RSS within **1.5× of native
tokei** (memory target: ≤ 66MB). Byte-for-byte identical output. No language
semantics change, no script-visible behavior change.

## Current state (2026-07-01 EOD)

| Tool | Wall | RSS | vs real tokei |
|---|---|---|---|
| real tokei | ~700ms | ~44MB | 1.00× |
| xsh-tokei (baseline) | ~2,660ms | ~338MB | 3.80× slower, 7.7× more memory |
| xsh-tokei (optimized) | **~932ms** | ~340MB | 1.33× slower, 7.7× more memory |
| xsh-tokei (micro only) | **262ms** | ~152MB | **2.67× faster** |
| **Speed target** | **≤ 840ms** | — | ≤ 1.20× slower |
| **Memory target** | — | **≤ 66MB** | ≤ 1.50× |

The micro benchmark already crushes the speed target (262ms = 2.67× faster than
native tokei). The macro benchmark is 92ms (11%) from the speed target and 274MB
from the memory target.

xsh-tokei was once **1.3× faster** than native tokei (~530ms) on an older
interpreter whose lowered runtime was **4,051 lines**. The current lowered
runtime is **17,168 lines** — 4.2× larger.

## Where the 340MB RSS comes from

The Sentry corpus is ~625MB of source code across 18,419 files. During the run:

1. **Resolved 2026-07-02: `fork_for_par_map()` cloned the Evaluator per worker
   thread (10×).** Each clone copied `sources` (SourceMap with script text),
   `scopes` (variable bindings), `lowered_pures/procs` maps,
   `module_value_cache`, `env`, and ~30 other fields. The fork path is now
   removed; par-map workers are built from `Arc<LoweredSharedState>` and the
   large lowered/source/module maps are shared.

2. **`Vec<u8>` file read buffers.** Each `std::fs::read(path)` allocates a
   `Vec<u8>` for the file contents. Over the run, 625MB of Vecs are allocated
   and freed. jemalloc retains ~50% of those pages (~200MB). mimalloc and the
   system allocator behave similarly.

3. **Pipeline intermediate Vecs.** Despite the streaming prefix optimization
   (lazy map/where), we still collect one `Vec<LoweredValue>` of 18K items
   (~5MB) for `par-map`, plus `all_results` (~5MB).

4. **Thread stacks.** In the last measured run, 10 threads × 8MB reserved =
   80MB virtual; ~1MB committed each = ~10MB RSS. After the 2026-07-02 worker
   refactor, lowered par-map workers request 64MiB stacks to avoid stack
   overflow in lowered proc calls; rerun RSS before updating the measured table.

Items 2–4 accounted for ~220MB in the last measured run. The remaining ~120MB
was attributed to jemalloc page retention from the evaluator clones and
temporary allocations during scanning. Rerun the macro/RSS benchmark before
updating the headline memory table.

### JSON library and encoding follow-up

`showcase/tokei.xsh --json` is currently an encoding workload, not a parsing
workload: it builds a large XSH result value and then serializes it. Swapping
JSON parsers will not directly fix the peak RSS for this benchmark. The
high-leverage JSON change is to avoid building a second JSON object graph during
`json.encode`, especially on the lowered compact path.

For later JSON parsing work, benchmark small and fast alternatives before
changing dependencies. `jiter` is the first candidate to try because it exposes
iterator-style parsing and a value API without being a SIMD-heavy dependency.
`simd-json` and `sonic-rs` may be faster on large payloads, but they are larger
dependencies and need careful vetting against XSH's portability and dependency
budget. `serde_json` remains the conservative reliability baseline.

Also revisit whether XSH's internal value model can better accommodate
miniserde's `Serialize` API. Today XSH JSON compatibility is fallible at runtime:
`Path`, `Bytes`, `Status`, `Result`, and errors must produce an XSH
`json-compatible` error, not silently serialize or panic. miniserde's serializer
shape is easiest when traversal cannot fail with a language-level error. A
future design could introduce a JSON-compatible borrowed view/wrapper over
`Value`/`LoweredValue` that validates before serialization, or a small internal
fallible writer trait that mirrors the parts of `Serialize` XSH needs while
preserving exact error behavior.

2026-07-02 trial: the lowered compact `json.encode` path now validates
`LoweredValue` for JSON compatibility, then serializes through a borrowed
miniserde `Serialize` view instead of building a `miniserde::json::Value` tree.
This reduced the Sentry `tokei --json` normal-release RSS sample range from
~780-853MB to ~720-758MB. Allocation counters improved from
`allocation_calls=5,660,383`, `allocation_bytes=4,483,316,312`,
`peak_rss=709,312,512` to `allocation_calls=5,251,521`,
`allocation_bytes=4,419,649,330`, `peak_rss=643,956,736`. This confirms the
view-layer design is worthwhile, but the remaining peak is still dominated by
the large XSH result object graph and benchmark/script-level retention.

2026-07-02 follow-up: `showcase/tokei.xsh --json` now clears the intermediate
`scanned_files` list and clears each per-language report accumulator immediately
after its contents have been copied into the final output object and
`Total.children`. The JSON-only `ScannedFile` record also no longer stores a
separate `stats` field because `report.stats` already contains the same value.
This does not change the JSON shape, and the targeted showcase tokei test still
passes. Normal-release RSS samples improved again to ~510-526MB. Perf counters
improved from `allocation_calls=5,251,521`, `allocation_bytes=4,419,649,330`,
`peak_rss=643,956,736` after the miniserde view change to
`allocation_calls=5,008,719`, `allocation_bytes=4,018,650,173`,
`peak_rss=391,839,744`. The result confirms retained duplicate report graphs
are a major component of the JSON peak, but the benchmark is still far from the
66MB RSS target.

2026-07-02 rejected trial: lowered `List` clones were changed to share large
lists through an immutable copy-on-write representation, targeting the duplicate
per-language `reports` lists stored in both each language object and
`Total.children`. This reduced allocation counters to roughly
`allocation_calls=4,859,384` and `allocation_bytes=3,755,961,002`, but
allocator peak RSS regressed to ~473-481MB and normal-release RSS samples were
no better (~496-622MB). Do not pursue this narrow shared-list clone design
without a broader ownership/liveness change; it improves allocation volume but
does not improve the benchmark's peak memory.

2026-07-02 structural follow-up: the shared-list idea was kept only after
changing where sharing happens. Instead of copying a `Vec` into an `Arc` during
`Clone`, the lowered `Param` read path now freezes large slot lists in place:
the existing `Vec` is moved into `SharedList(Arc<Vec<_>>)` once, left in the
slot, and subsequent clones share it. This specifically removes the deep clone
of per-language `reports` when `showcase/tokei.xsh --json` stores those lists in
both each language object and `Total.children`. The JSON path also now iterates
the par-map result directly instead of retaining a `scanned_files` slot, and
clears the per-file `text` buffer after scanning. Normal-release RSS samples are
now ~177MB for both `--json` and table output on the Sentry corpus; perf-metrics
JSON counters are roughly `allocation_calls=1,378,818`,
`allocation_bytes=816,701,477`, and `peak_rss=190,808,064`. This is a real
structural reduction from ~510-526MB normal RSS and ~392MB perf peak, but the
remaining gap to 66MB is now mostly the file-read/scan memory shape rather than
duplicated JSON report lists.

2026-07-02 fs streaming trial: lowered `fs.files`/`fs.walk` now preserve the
live `Stream` instead of eagerly collecting it into a `List`, `count()` drains a
live stream without materializing entries, and the common
`Stream |> map |> where |> par-map { ... }` shape feeds par-map workers through
a bounded queue instead of building the candidate list first. The live
`IgnoreWalkStream` also uses a sequential pull walker so `fs.files` and
`par-map` do not create nested filesystem-walk parallelism. This fixed a stream
prefix ordering bug exposed by `where |> map` and keeps behavior covered by the
runtime stream tests.

The RSS result is directionally better but not enough. On a generated local
14,340-file / 56MB corpus, no-read `fs.files(root, stat:false)? |> count()`
still peaks at ~122MB, while `showcase/tokei.xsh --json` peaks at ~119-120MB
and table output at ~116-117MB. That means the remaining structural floor is
not JSON encoding and not retained file text; even a discarded filesystem entry
allocates a full runtime record (`path`, `name`, `ext`, shaped record wrapper)
for every file. The next serious memory design should make fs entries lowered
and lazy, so `count()` can count paths without constructing records and tokei's
`entry.path`/`entry.name`/`entry.ext` accesses allocate only the fields the
script actually reads.

2026-07-02 lazy fs-entry follow-up: `stat:false` filesystem entries now use a
compact runtime/lowered fs-entry value that reports as a `Record` but only
materializes fields on demand. Direct field access for `path`, `name`, `ext`,
and `kind` projects from the stored path/file kind without first building a
`RecordMap` or lowered `BTreeMap`; generic record consumers still materialize
the old record shape. On the same generated 14,340-file / 56MB corpus, no-read
`fs.files(root, stat:false)? |> count()` dropped from ~122MB to ~17MB RSS.
`showcase/tokei.xsh --json` dropped from ~119-120MB to ~84-87MB, and table
output from ~116-117MB to ~81-82MB.

Scaling now points at file-content bytes, not metadata. On a larger generated
57,348-file / 225MB corpus, no-read walk is only ~29MB RSS, but
`showcase/tokei.xsh --json` is ~274-280MB and table output is ~269MB. The next
memory lever is avoiding heap-owned `std::fs::read` buffers for read-and-scan
workloads, likely with a lowered-only mapped/borrowed byte storage that still
behaves as `Bytes` at language boundaries.

2026-07-02 rejected trial: lowered-only anonymous mmap backing for
`Path.read_bytes()` preserved byte behavior in focused tests but did not reduce
release RSS on the generated corpora (`--json` stayed around ~86MB on the
14,340-file corpus and ~273MB on the 57,348-file corpus). Do not keep that
complexity as a standalone change; the remaining peak appears to be retained
report/result shape, not heap allocator retention from transient file buffers.

2026-07-02 Sentry rerun: the real checkout is currently much larger than the old
note assumed (`~/dev/sentry` is ~3.1GB / 140,909 files; `fs.files(...,
exts: source_exts())` sees 20,101 relevant files after ignore/extension
filtering). Current XSH release completes on that corpus: `--json` is ~231MB
RSS / ~1.05s and table output is ~237MB RSS / ~1.10s. Native tokei samples are
~45MB RSS / ~1.52s for JSON and ~45MB RSS / ~0.75s for table output. No-read
`fs.files(root, stat:false)? |> count()` is only ~19MB RSS, so the remaining
Sentry gap is not traversal or entry metadata.

The ignore-aware selected source is ~18.4K files / ~186MB. The largest selected
files are the locale `.po` files and `CHANGES` at ~1.5-1.7MB each; there are no
single huge selected files setting the floor. A temporary `par-map --jobs=N`
sweep on the table path did not materially move RSS (`N=1` ~234MB, `N=2`
~236MB, `N=4` ~233MB, `N=8` ~229MB), so the gap is not primarily worker-count
or per-thread cache retention. With `perf-metrics`, Sentry table output still
does ~3.5GB of heap allocation and JSON ~2.6GB. That scale points at the
lowered value/record shape during scan/report construction, not just full-file
byte buffers.

2026-07-02 rejected trial: a narrow streaming fusion for
`par-map |> where |> flat-map |> reduce-by` on the table path reduced Sentry RSS
only slightly (~239MB to ~235MB) and regressed wall time (~1.1s to ~1.9s). Do
not keep that evaluator complexity without a stronger streaming aggregation
design.

2026-07-02 table-path follow-up: the default table path now has the `par-map`
worker return final `SummaryRow` reduce rows directly instead of returning
`{language, label, scan}` and expanding that nested scan record later. The
script then does only `flat-map { |rows| rows } |> reduce-by`. This preserves
the table output exactly (verified against the previous XSH table sample) and
keeps JSON untouched. On Sentry, normal release table RSS improved from
~229-237MB to ~127-140MB; a clean single run sampled ~1.05s / ~140MB. With
`perf-metrics`, table allocation volume improved from roughly
`allocation_calls=4,649,345`, `allocation_bytes=3,470,099,917`,
`peak_rss=269,549,568` to `allocation_calls=3,942,983`,
`allocation_bytes=2,259,806,545`, `peak_rss=206,684,160`.

2026-07-02 rejected trial: after the `SummaryRow` script change, a narrow
runtime fusion for `par-map |> flat-map(identity) |> reduce-by` aggregated rows
inside worker-local maps. It reduced instruction count but did not reduce RSS
beyond the script change (~138MB either way), and it added too much evaluator
complexity for no memory win. Do not revive that exact fusion; if aggregation
fusion is revisited, it needs to attack the remaining file-byte/value allocation
floor rather than only remove the post-`par-map` row list.

## Handoff (2026-07-02)

The active objective is still incomplete. Current best Sentry samples are:

| Path | XSH release | Native tokei | Target |
|---|---:|---:|---:|
| table | ~1.05s / ~140MB RSS | ~0.75-1.17s / ~44MB RSS | ≤1.2× wall, ≤66MB RSS |
| JSON | ~1.05-2.0s / ~228-234MB RSS | ~1.2-1.5s / ~45-55MB RSS | ≤1.2× wall, ≤66MB RSS |

Retained changes in the worktree:

- shared lowered state for `par-map` workers;
- borrowed/miniserde JSON serialization over `LoweredValue`;
- `SharedList` slot freezing for large lowered lists;
- live lowered `fs.files`/`fs.walk` streams and bounded streaming
  `Stream |> map |> where |> par-map`;
- lazy `FsEntry` values for `stat:false` entries;
- `bytes.concat` compatibility with `SharedList`;
- `showcase/tokei.xsh` table path returns `SummaryRow` rows directly from
  `par-map`, cutting table RSS by roughly 90-100MB.

Recent verification:

- `cargo check --lib`
- `cargo build --bin xsh && cargo run -p xsht -- test showcase/tests/test-tokei.xsh`
- `cargo test --test runtime par_map_reduce_by_fuses_to_local_worker_aggregation`
- `cmp -s /tmp/xsh-table-final-serial.out /tmp/xsh-table-direct-rows-final3.out`

Recommended next investigation:

1. Rebuild a normal release binary after any `perf-metrics` run; that feature
   installs a different allocator and changes RSS/timing.
2. Keep the table `SummaryRow` script change; it is simple and measured.
3. Do not spend more time on narrow `par-map`/`flat-map`/`reduce-by` fusion
   unless the design also reduces the remaining file-scan allocation floor.
4. Profile the lowered scanner/value path. The latest table allocation counters
   still show ~2.26GB allocated to process ~186MB of selected source. The next
   memory win is likely reducing per-line/per-file `LoweredValue` record/map/list
   churn in the scanner path, or introducing a script-visible-neutral host
   scanner primitive for the repeated `Stats` shapes.

### Why native tokei is 44MB

Native tokei uses `ignore::Walk::parallel()` + `rayon` which fuse walk, read,
and scan into one parallel operation. There are no evaluator clones, no
intermediate item Vecs, no `BTreeMap` per file entry. Each file's bytes are
read, scanned, and dropped before the next file is pulled. Peak memory is
`max(file_size) × thread_count` + fixed overhead ≈ 44MB.

## Completed priority: remove `fork_for_par_map`

`fork_for_par_map()` has been removed from `eval.rs`. Parallel lowered
`par-map` now takes one `Arc<LoweredSharedState>` snapshot and constructs
per-thread `LoweredWorker`s from that shared state. The largest immutable
runtime fields (`sources`, lowered function maps, lowered program, dynamic
module cache, and function-module maps) are stored behind `Arc`, so worker setup
does not clone those maps or source text per thread.

A parallel worker needs only:

- **Immutable (shared via `Arc`):** lowered function definitions, sources, env,
  cwd, module caches — set up once and never mutated during evaluation.
- **Mutable (per-thread):** stdout/stderr buffers, slot pool, signal state,
  trace state.

The current implementation keeps the existing lowered evaluator methods on
`Evaluator` and uses `LoweredWorker` as the worker-facing wrapper, rather than
moving all `eval_lowered_*` methods in one large mechanical change. That keeps
the behavioral diff small while eliminating the fork path and the heaviest
per-thread clones. A later cleanup can still move the methods fully onto
`LoweredWorker` if that improves maintainability.

## Performance log

| Date | Change | Micro | Macro | vs tokei |
|---|---|---|---|---|
| 2026-07-01 | baseline (before fixes) | — | 2,569ms | 3.62× |
| 2026-07-01 | +StrByte lowering wired | — | 2,767ms | 4.03× |
| 2026-07-01 | +add_stats restored | — | 2,660ms | 3.80× |
| 2026-07-01 | +has_defers fast stmts path | — | 2,656ms | 3.79× |
| 2026-07-01 | +batched signal checks (64 lines) | — | 2,604ms | 3.72× |
| 2026-07-01 | +fat LTO (dist profile) | — | 2,522ms | 3.60× |
| 2026-07-01 | +dir cache for file reads | — | 2,353ms | 3.36× |
| 2026-07-01 | +parallel par-map (atomic counter) | — | 2,231ms | 3.19× |
| 2026-07-01 | +std::fs::read (replaces cap_std) | — | 1,933ms | 2.76× |
| 2026-07-01 | +ScanLines dedicated scan loop | — | 1,886ms | 2.69× |
| 2026-07-01 | +parallel **ParMapBlock** (true parallelism) | **262ms** | **948ms** | **1.35×** |
| 2026-07-01 | +stream collection (eliminate intermediate Vec) | — | 921ms | 1.32× |
| 2026-07-01 | +lazy map/where streaming + bounded fs channel | — | 932ms | 1.33× |
| 2026-07-01 | +slim fork_for_par_map | — | 932ms | 1.33× |
| 2026-07-02 | +remove fork_for_par_map shared worker state | not rerun | not rerun | not rerun |
| — | **speed target** | — | **≤ 840ms** | **≤ 1.20×** |
| — | **memory target** | — | **≤ 66MB RSS** | **≤ 1.50×** |

## Optimizations applied

1. **Parallel `ParMapBlock`** (the key win). The tokei script uses
   `ParMapBlock` (block bodies with `let` statements), not `ParMap` (simple
   expressions). Only `ParMap` was parallelized; `ParMapBlock` was a sequential
   `for` loop. Now uses `std::thread::scope` + atomic counter work distribution
   with lowered workers. CPU/wall ratio: 1.1× → 4.4×.

2. **Streaming pipeline prefix.** When a pipeline stage list starts with
   Map → Where from a `Stream` input (e.g. `fs.files()`), items are pulled
   lazily and transformed inline instead of collecting into intermediate Vecs.
   Eliminates 2 `Vec<LoweredValue>` allocations (map output, where output).

3. **Bounded fs walk channel.** `IgnoreWalkStream` changed from unbounded to
   `bounded(1024)` channel, providing natural backpressure — the filesystem
   walker blocks when the consumer falls behind.

4. **`std::fs::read` replaces cap_std.** `read_host_path_bytes` uses
   `std::fs::read` directly instead of `cap_std::fs::Dir::open_ambient_dir`
   + `dir.open`. Eliminates capability checks and parent-directory re-opening
   overhead. Saved ~235ms system time.

5. **`ScanLines` dedicated scan loop.** When a `ForStrLines` loop body matches
   the simple scanner pattern (single `IfBool` with counter increments), the
   lowering emits `LoweredStmt::ScanLines` instead. The runtime processes lines
   in a tight loop without `eval_lowered_stmts` dispatch. Hits 5,039 times on
   the Sentry corpus.

6. **Perf counters** (`perf-counters` feature). Fast-path hit/miss counters
   printed at exit. Confirmed `fast_plain_return` 48.5% hit rate, `fast_return`
   28.4% hit rate, 18,759 slow-path falls-through.

7. **Fast no-defer `eval_lowered_stmts_fast`.** When `LoweredPureFunction.
   has_defers` is false (computed at lowering time via recursive scan), skips
   the `defers` Vec allocation and `Defer` check per statement.

8. **Batched signal checks.** `ForStrLines` and `ScanLines` check
   `service_pending_signal` every 64 lines instead of every line.

9. **Inline stream collection.** `collect_lowered_stream_values` converts
   stream items directly to `LoweredValue` instead of collecting into
   `Vec<Value>` first, eliminating one intermediate allocation.

10. **Removed `fork_for_par_map`.** Parallel lowered `par-map` now uses
    `LoweredSharedState` + `LoweredWorker`, with the large immutable lowered
    runtime fields shared through `Arc` instead of cloned per worker. Worker
    threads also use an explicit lowered-runtime stack size so lowered proc
    calls inside par-map do not overflow the default thread stack.

11. **`has_defers` recursive scan.** `lowered_body_has_defers` checks nested
    statements (inside `If` branches, `Retry` bodies, `While` loops, etc.) so
    the fast no-defer path is used correctly for all pure functions.

## Syscall analysis

xsh processes 18,419 files. Each `std::fs::read` does 4 syscalls (openat,
fstat, read, close) = ~73,676 syscalls. At ~15μs per syscall on SSD, this
accounts for ~1,100ms of system time.

Native tokei also does 4 syscalls/file via `File::open` + `read_to_string`.
Its 700ms total is achievable because `ignore::Walk::parallel()` + `rayon` fuse
walk, read, and scan into a single parallel operation — directory traversal,
I/O, and computation all overlap across threads. System time is 1,722ms but
wall time is 688ms (5× parallelism factor). Our 4.4× parallelism factor nearly
matches this.

## References

- `showcase/tokei.xsh` header — tokei benchmark history and lowered-IR milestones
- `docs/IR.md` — lowered IR design
- `PORTS.md` — project porting status, tokei as the forcing benchmark
- `~/d/laputa-systems/xsh-archive/` — old interpreter snapshot; lowered runtime
  was 4,051 lines (current: 17,168)
