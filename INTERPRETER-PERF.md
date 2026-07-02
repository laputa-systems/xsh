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

1. **`fork_for_par_map()` clones the Evaluator per worker thread (10×).**
   Each clone copies `sources` (SourceMap with script text), `scopes` (variable
   bindings), `lowered_pures/procs` maps, `module_value_cache`, `env`, and ~30
   other fields. This is the single worst design choice — see below.

2. **`Vec<u8>` file read buffers.** Each `std::fs::read(path)` allocates a
   `Vec<u8>` for the file contents. Over the run, 625MB of Vecs are allocated
   and freed. jemalloc retains ~50% of those pages (~200MB). mimalloc and the
   system allocator behave similarly.

3. **Pipeline intermediate Vecs.** Despite the streaming prefix optimization
   (lazy map/where), we still collect one `Vec<LoweredValue>` of 18K items
   (~5MB) for `par-map`, plus `all_results` (~5MB).

4. **Thread stacks.** 10 threads × 8MB reserved = 80MB virtual; ~1MB committed
   each = ~10MB RSS.

Items 2–4 account for ~220MB. The remaining ~120MB is jemalloc page retention
from the evaluator clones and temporary allocations during scanning.

### Why native tokei is 44MB

Native tokei uses `ignore::Walk::parallel()` + `rayon` which fuse walk, read,
and scan into one parallel operation. There are no evaluator clones, no
intermediate item Vecs, no `BTreeMap` per file entry. Each file's bytes are
read, scanned, and dropped before the next file is pulled. Peak memory is
`max(file_size) × thread_count` + fixed overhead ≈ 44MB.

## Priority: remove `fork_for_par_map`

`fork_for_par_map()` (in `eval.rs`) clones the entire `Evaluator` — 47 fields —
for every worker thread. This is lazy design. A parallel worker needs only:

- **Immutable (shared via `Arc`):** lowered function definitions, sources, env,
  cwd, module caches — set up once and never mutated during evaluation.
- **Mutable (per-thread):** stdout/stderr buffers, slot pool, signal state,
  trace state.

The fix: extract the immutable state into a `LoweredSharedState` struct wrapped
in `Arc`, and create a `LoweredWorker` that holds `Arc<LoweredSharedState>` +
per-thread mutable fields. `eval_lowered_expr` / `eval_lowered_stmts` / etc.
move from `impl Evaluator` to `impl LoweredWorker`. The `Evaluator` methods
become thin wrappers that delegate to `self.worker.eval_lowered_expr(...)`.

This eliminates 10 evaluator clones (~10–50MB), reduces fork overhead from
~hundreds-of-µs to ~an-Arc-clone, and makes the design general: any lowered
expression evaluation in a background context (stream workers, signal hooks,
test runners) benefits from not needing a full Evaluator.

**Scope:** ~1015 `self.` accesses across `lowered_run.rs` need to be channeled
through the shared state. Mechanical but tedious — a sed script handles most
of it. The struct extraction is straightforward; the volume of field access
changes is the only reason it wasn't done in this session.

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
| — | **speed target** | — | **≤ 840ms** | **≤ 1.20×** |
| — | **memory target** | — | **≤ 66MB RSS** | **≤ 1.50×** |

## Optimizations applied

1. **Parallel `ParMapBlock`** (the key win). The tokei script uses
   `ParMapBlock` (block bodies with `let` statements), not `ParMap` (simple
   expressions). Only `ParMap` was parallelized; `ParMapBlock` was a sequential
   `for` loop. Now uses `std::thread::scope` + atomic counter work distribution
   with per-thread evaluator forks. CPU/wall ratio: 1.1× → 4.4×.

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

10. **Slim `fork_for_par_map`.** Removed unnecessary clones for fields workers
    don't need (`tag_variants`, `error_families`, `net_pool_options`,
    `test_mocks`, etc.). The real fix is to eliminate the fork entirely — see
    the priority section above.

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
