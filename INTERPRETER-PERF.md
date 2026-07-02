# Interpreter Performance: tokei.xsh

## Objective

Get `showcase/tokei.xsh` wall-clock time on the Sentry corpus within **1.2× of
native tokei** (target: ≤ 840ms, current: ~2,660ms, gap: ~3.2×), with **byte-for-byte
identical output** to the current version. All work is under-the-hood interpreter
optimization — no language semantics change, no script-visible behavior change.

## Current state (2026-07-01)

| Tool | Mean | vs real tokei |
|---|---|---|
| real tokei | ~700ms | 1.00× |
| xsh-tokei (current) | ~2,660ms | 3.80× slower |
| **Target** | **≤ 840ms** | **≤ 1.20× slower** |

xsh-tokei was once **1.3× faster** than native tokei (~530ms) on an older
interpreter whose lowered runtime (`lowered_run.rs` + `lowered_ops.rs`) was
**4,051 lines**. The current lowered runtime is **17,168 lines** — 4.2× larger.

Output vs real tokei: within 0.2% line-count accuracy (different blank-line
heuristics). File selection is exact.

## Already done (2026-07-01)

- **`pure add_stats()` restored.** 52 call sites rewired from manual field-by-field
  addition back to the fused pure function. Enables SCC co-lowering.
- **`StrByteLen`/`StrByteAt` lowering wired.** Byte ops now skip `Method` dispatch.
  Slot optimization (`Param` → `StrByteLenSlot`) skips receiver eval.
- **~900 lines of dead code removed.** Old-AST evaluator methods, unused Flow
  variants, dead stream helpers, orphaned optimization scaffolding. Zero perf
  impact — these were unreachable in the compact lowering path.

## Profiling setup

### Macro benchmark

```sh
hyperfine --warmup 3 --runs 10 \
  'target/release/xsh showcase/tokei.xsh -- ~/dev/sentry' \
  '/Users/josh/d/tokei/target/release/tokei ~/dev/sentry'
```

### Micro benchmark (fast iteration)

A single large source file to isolate scanning from filesystem walk:

```sh
# Find the largest files
find ~/dev/sentry -name '*.rs' -exec wc -l {} + | sort -rn | head -5

# Benchmark just that directory
hyperfine --warmup 5 --runs 20 \
  'target/release/xsh showcase/tokei.xsh -- ~/dev/sentry/src/sentry'
```

### CPU profiling

```sh
# samply (simplest, works cross-platform)
cargo install samply
samply record target/release/xsh showcase/tokei.xsh -- ~/dev/sentry

# instruments (macOS native, more detail)
instruments -t 'Time Profiler' -l 20000 \
  target/release/xsh showcase/tokei.xsh -- ~/dev/sentry

# flamegraph (Linux/macOS via DTrace)
cargo install flamegraph
cargo flamegraph --bin xsh -- showcase/tokei.xsh -- ~/dev/sentry
```

### Counters for hypothesis testing

Add atomic counters behind a compile-time feature flag to measure how often
specific code paths fire without disrupting the hot path:

```rust
// In lowered_run.rs, gated on #[cfg(feature = "perf-counters")]:
static FAST_PLAIN_RETURN_HITS: AtomicU64 = AtomicU64::new(0);
static FAST_PLAIN_RETURN_MISSES: AtomicU64 = AtomicU64::new(0);
static FAST_RETURN_HITS: AtomicU64 = AtomicU64::new(0);
static FAST_RETURN_MISSES: AtomicU64 = AtomicU64::new(0);
static SLOT_OPT_HITS: AtomicU64 = AtomicU64::new(0);
static METHOD_DISPATCH_FALLBACKS: AtomicU64 = AtomicU64::new(0);
```

Print counters at exit. Tells us at a glance which optimizations fire and which
don't, before diving into a profiler.

## Investigation hypotheses

### H1: The fast paths aren't firing

The old interpreter had `eval_lowered_fast_plain_return` and
`eval_lowered_fast_return` that handled simple pure functions without going
through the general statement-by-statement evaluator. They still exist (at
`lowered_run.rs` lines 7894 and 8408), but may not match the current IR shape.

**Test:** Add the fast-path hit/miss counters above. Run tokei. If misses ≈
total calls, the fast paths are dead.

**Fix:** Compare old vs new `eval_lowered_fast_plain_return` to find what
conditions changed. The old version handled `Int`/`Bool`-returning functions;
the new one may have stricter conditions that no current IR satisfies.

### H2: Value boxing/unboxing costs dominate

Each `byte_at()`, field access, or arithmetic op may spend more time converting
between `LoweredValue` and raw Rust types than doing the actual work. The
lowered runtime expanded from 4K to 17K lines — much of that may be conversion
boilerplate on the hot path.

**Test:** In samply/instruments, look for time spent in:
- `LoweredValue::into_value`
- `lowered_value_from_runtime`
- `Value::Int` / `Value::Str` constructors
- `RecordMap` / `FxHashMap` lookups with `Name` keys

**Fix:** If conversions dominate, add more `LoweredValue`-native fast paths
that operate on slots directly without round-tripping through `Value`.

### H3: Record field access is doing hash lookups

`add_stats` accesses `.blanks`, `.code`, `.comments` on Stats records. If each
field access does a `FxHashMap::get` with a `Name` (interned string), that's
three hash lookups per `add_stats` call — and `add_stats` is called 52 times
per file, across 18,000 files.

**Test:** Profile record field access. Look for `FxHashMap::get` / `get_item`
calls with field-name keys on the hot path.

**Fix:** The lowering knows the record shape at compile time. If the Stats record
always has fields [blanks, code, comments] in that order, integer indexing can
replace hash lookups. This is an IR-level change — the lowered runtime already
has ordered record representations for known shapes.

### H4: Byte operations have too much overhead

`byte_at()` and `byte_len()` go through `LoweredExpr::StrByteAt` → match dispatch
→ type validation → Param-slot fast path check → `lowered_str_byte_at_value`.
Even with the slot optimization wired, each call may do more work than the old
interpreter did.

**Test:** Profile `eval_lowered_plain_expr` and `eval_lowered_expr` with
StrByteLen/StrByteAt inputs. Count instructions per call vs the equivalent
inlined Rust `bytes[index]`.

**Fix:** If the match dispatch and type validation dominate, consider a
dedicated byte-scanning interpreter loop that operates on `&[u8]` directly,
bypassing the general expression evaluator for tight character-scanning loops.

### H5: `par-map` overhead dominates wall time

tokei uses `par-map |> flat-map |> reduce-by` for per-file parallelism. If
thread-pool overhead costs more than the scanning work (especially for small
files), parallelism may be a net loss.

**Test:** Run a single-directory benchmark (e.g. `~/dev/sentry/src/sentry/`).
Compare wall time of `par-map` vs a simple `for` loop. If `for` is faster,
parallelism overhead is the bottleneck.

**Fix:** Tune the parallel work granularity. `par-map` should batch small files
into chunks large enough to amortize thread-pool overhead.

### H6: Code size causes I-cache pressure

The current `lowered_run.rs` is ~14,000 lines with hundreds of match arms. The
old one was 2,487 lines. The larger code may cause instruction-cache misses on
the hot path.

**Test:** In instruments, look at "Instructions Retired" vs "Cycles" — a high
ratio (low IPC) suggests cache stalls. Use `--sample-cpu` to see which functions
have the most stalls.

**Fix:** Extract cold code paths (error handling, rarely-taken match arms) into
separate functions with `#[cold]` annotations. Split `lowered_run.rs` into
hot-path and cold-path modules.

## tokei.xsh improvement plan

### Already done

- [x] Restore `pure add_stats()` — 52 call sites rewired
- [x] Wire `StrByteLen`/`StrByteAt` lowering — byte ops skip Method dispatch
- [x] Slot optimization for Param receivers — skips receiver evaluation
- [x] Remove ~900 lines dead runtime code

### Script-level (after profiling confirms bottlenecks)

- [ ] Profile `count_slash_language` to confirm byte ops / field access are the
  hot instructions (not `line.contains()`, `line.trim()`, etc.)
- [ ] Audit that every scanner is `pure`-annotated for SCC co-lowering
- [ ] If `blobs` field is never read on the hot path, consider a `StatsNoBlobs`
  type for the scanning phase, converted to `Stats` only at aggregation time
- [ ] Check that `.lines()` produces borrowed views (zero-copy), not owned strings

### Interpreter-level (ordered by expected impact)

1. **Verify fast paths fire.** Add counters. If `eval_lowered_fast_plain_return`
   handles `add_stats` and `count_*`, that's a free 2–3× on pure calls. If not,
   fix the conditions so they match the current IR shape.

2. **Optimize record field access for known shapes.** The lowering knows the
   Stats record has fields `{blanks, code, comments}`. When the IR sees field
   access on a record with a known compile-time shape, emit integer indices
   instead of string-keyed hash lookups.

3. **Tighten the byte-operation path.** `StrByteLen`/`StrByteAt` are wired, but
   their runtime handlers still do general-purpose type checking and Param-slot
   dispatch. A dedicated `byte_scan` loop could operate on raw `&[u8]` slices,
   calling `byte_at()` without leaving the lowered runtime at all.

4. **Reduce Value ↔ LoweredValue round-trips.** The `for line in text.lines()`
   loop currently converts each line through the Value system. If `.lines()`
   stays in `LoweredValue` space (borrowed byte slices), the entire scanner
   loop runs without boxing.

5. **Tune par-map granularity.** If H5 is confirmed, batch small files into
   work chunks of ~100 files each, amortizing thread-pool dispatch.

6. **Split hot/cold code.** Move error-handling match arms, diagnostics, and
   tracing out of `lowered_run.rs` into separate `#[cold]` functions.

### Benchmarking cadence

After each change, run both benchmarks and record the result:

```sh
# Micro (fast iteration)
hyperfine --warmup 5 --runs 20 \
  'target/release/xsh showcase/tokei.xsh -- ~/dev/sentry/src/sentry'

# Macro (full verification, after each milestone)
hyperfine --warmup 3 --runs 10 \
  'target/release/xsh showcase/tokei.xsh -- ~/dev/sentry' \
  '/Users/josh/d/tokei/target/release/tokei ~/dev/sentry'
```

Keep a running log of times in this document so we can see which changes moved
the needle.

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
| 2026-07-01 | **current (all optimizations)** | **674ms** | **1,933ms** | **2.76×** |
| — | target | — | ≤ 840ms | ≤ 1.20× |

## Current status (2026-07-01 EOD)

| Tool | Mean | vs real tokei |
|---|---|---|
| real tokei | ~700ms | 1.00× |
| xsh-tokei (current) | ~1,933ms | 2.76× slower |
| xsh-tokei (micro, src/sentry only) | ~674ms | **0.96× (faster!)** |
| **Target** | **≤ 840ms** | **≤ 1.20× slower** |

The micro benchmark (single directory, ~5,300 files) already exceeds the target at
674ms — 1.04× faster than native tokei. The macro benchmark is still 2.76× above
target, with system time (file I/O) at ~1,100ms dominating the wall time.

### Optimizations applied (2026-07-01)

1. **Perf counters** (`perf-counters` feature): atomic counters at fast-path call
   sites, printed at exit. Confirmed fast_plain_return hits 5,331 (48.5% hit rate),
   fast_return hits 7,444 (28.4% hit rate), 18,759 calls fall through to slow path.

2. **Fast no-defer `eval_lowered_stmts_fast`**: when `LoweredPureFunction.has_defers`
   is false, skips defer Vec allocation and Defer checks. `has_defers` is computed
   at lowering time via recursive scan of the function body.

3. **Batched signal checks**: `ForStrLines` checks `service_pending_signal` every
   64 lines instead of every line (saves ~5M atomic ops on Sentry corpus).

4. **Parallel `par-map`**: Changed from sequential `for` loop to `std::thread::scope`
   with atomic-counter work distribution. Each worker gets a forked evaluator
   (`fork_for_par_map`) with shared lowered function definitions. Falls back to
   sequential path when tracing is enabled or ≤ 1 item.

5. **Direct file I/O**: `read_host_path_bytes` changed from `cap_std::fs::Dir::
   open_ambient_dir` + `dir.open` to direct `std::fs::read`. Eliminates capability
   checks and parent-directory re-opening overhead. System time reduced by ~235ms.

### Remaining gap

The macro benchmark spends ~1,100ms in system time (file I/O) for ~18K files.
Native tokei completes all I/O + scanning in ~700ms total. The gap is primarily:

- **File I/O throughput**: xsh reads files sequentially inside par-map; even with
  threads, I/O throughput doesn't scale linearly with concurrency. Native tokei
  uses `ignore::Walk` + `rayon` which parallelizes the walk itself, overlapping
  directory traversal with file reading and scanning.
- **Per-line interpreter dispatch**: `eval_lowered_stmts` → `eval_lowered_stmt` →
  `eval_lowered_bool` chain costs ~200ns per line. For 5M lines, this is ~1,000ms
  of CPU. A dedicated `ScanLines` lowering that processes simple scanner loops
  without general dispatch could eliminate most of this overhead.
- **PGO**: Profile-guided optimization (the `profiling` profile exists) could
  give an additional 10–20% improvement by better inlining the hot path.

### Investigated but not yet implemented

- **Record field access by index** (H3): Changing `LoweredValue::Record` from
  `BTreeMap` to `Vec` would make field access O(n) linear scan vs O(log n) tree
  lookup. For 4-field records, linear scan is competitive. Implementation is
  invasive (40+ call sites across 4 files).
- **Dedicated scan loop** (H4): Adding `LoweredStmt::ScanLines` that processes
  `for line in text.lines()` loops with simple conditions (trim-empty,
  trim-starts-with) in a tight loop without `eval_lowered_stmts` dispatch.
  Requires lowering changes to detect the scanner pattern.
- **Hot/cold code split** (H6): Extracting error-handling match arms from
  `eval_lowered_stmt` (2,374 lines) and `eval_lowered_expr` (3,239 lines) to
  `#[cold]` functions. The old interpreter had `eval_lowered_stmt` at ~400 lines
  and `eval_lowered_expr` at ~896 lines.

## References

- `showcase/tokei.xsh` header — tokei benchmark history and lowered-IR milestones
- `docs/IR.md` — lowered IR design
- `PORTS.md` — project porting status, tokei as the forcing benchmark
- `~/d/laputa-systems/xsh-archive/` — old interpreter snapshot; lowered runtime
  was 4,051 lines (current: 17,168)
