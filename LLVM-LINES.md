# LLVM Lines Reduction Plan

## Goal

Shrink the release-profile LLVM IR line count by avoiding unnecessary
monomorphization, without regressing XSH language semantics or the user-facing
benchmark suite.

The original target was a 10% reduction. The ceiling analysis below shows that
monomorphization avoidance on project-owned code alone caps well below that, so
the realistic low-risk scope is smaller; the path to ~10% requires hot-path
sort consolidation, which carries runtime risk and is documented here as a
follow-up rather than undertaken by default.

## Profile And Measurement

Always measure with the release profile. The `dist` profile is reserved for CI
release packaging and must not be used for this work.

```sh
cargo llvm-lines --release --no-default-features --features tools --lib > /tmp/xsh-llvm-lines.txt
```

The first data row after the `(TOTAL)` line is the grand total. The companion
tool analyzes the same capture:

```sh
# Per-offender table (project-owned only by default):
target/release/xsh tools/llvm-lines-repeat-offenders.xsh -- /tmp/xsh-llvm-lines.txt --limit 40 --examples 1

# Reclaimable-lines total as a share of the grand total:
target/release/xsh tools/llvm-lines-repeat-offenders.xsh -- /tmp/xsh-llvm-lines.txt --sum
target/release/xsh tools/llvm-lines-repeat-offenders.xsh -- /tmp/xsh-llvm-lines.txt --sum --all
target/release/xsh tools/llvm-lines-repeat-offenders.xsh -- /tmp/xsh-llvm-lines.txt --sum --all --filter slice::sort
```

The script can also generate and analyze the input directly:

```sh
target/debug/xsh tools/llvm-lines-repeat-offenders.xsh -- --generate --all --limit 40
```

`--sum` reports `duplicated` lines: the lines that would be reclaimed if every
monomorphized copy of each generic function collapsed into a single non-generic
inner function (i.e. `total_lines - max_instance_lines` summed over offenders).
That number is the ceiling for the monomorphization strategy in each scope or
category.

## Baseline (2026-07-02, release --lib)

| Metric | Value |
|---|---:|
| Total LLVM lines | 1,420,026 |
| Total copies | 27,984 |
| 10% reduction target | ~142,003 |

Treat this as historical context. Re-measure before choosing a target because
the runtime, crate layout, and generic call sites continue to change.

## Ceiling Analysis (reclaimable duplicated lines)

Scope `--all` (project + dependencies):

| Category | Duplicated | % of total | Notes |
|---|---:|---:|---|
| `slice::sort` machinery | 91,561 | 6.45% | 49 offenders, ~1400 instances; hot + cold |
| `btree` construction/clone | 21,148 | 1.49% | 105 offenders |
| `drop_in_place` | 17,866 | 1.26% | 4214 instances; ~irreducible (per-type) |
| project-owned generics | 6,281 | 0.44% | 42 offenders; safe inner-fn pattern |
| other (iter adapters, tokio, spawn, Vec collect) | ~127,851 | ~9.0% | long tail of small offenders |
| **total duplicated** | **264,707** | **18.64%** | ceiling if everything collapsed |

Scope project-owned only (`--sum` without `--all`): **6,281 lines (0.44%)**.

The key conclusion: the inner-non-generic-function pattern applied to
project-owned code can reclaim at most ~0.44% directly. De-monomorphizing a
project-owned generic also collapses some dependency machinery it instantiates
internally (drop_in_place, sort, BTree, iterator adapters), so the real yield is
higher than 0.44% but still modest, because the top project-owned offenders are
thin wrappers with limited internal machinery. Hitting ~10% requires attacking
the dependency-side sort machinery, which is mostly triggered by call sites in
our code.

## Iteration Methodology

For each candidate change:

1. **Re-measure the baseline** with the command above and record the `(TOTAL)`.
2. **Identify the target.** Use the table (`--limit N`) to find project-owned
   generics with many copies; use `--sum --all --filter <cat>` to size a
   dependency category. Prefer offenders with high `duplicated` and many
   `instances`.
3. **Apply one technique** (see below), scoped to a single function or call
   site. Keep the diff minimal.
4. **Rebuild and re-measure.** Re-run `cargo llvm-lines --release ...` and
   `--sum`. Compute the delta against the recorded baseline. The
   `duplicated-lines` drop should be visible immediately; the grand total drops
   by roughly the same amount minus any new wrapper codegen.
5. **Run `make bench`** when the change can affect a user-visible workflow.
6. **Run the behavior gate** for touched areas (see `docs/TEST-MAP.md`); update
   the nearest tests if behavior changed.
7. **Commit only the change**, not formatter churn (see `AGENTS.md`: never run
   `make lint`, `cargo fmt`, `cargo clippy --fix`, `xsht fmt`, or `xsht lint
   --fix`).

Techniques, safest first:

- **Inner non-generic function.** A generic `fn foo<T>(...)` with a substantial
  body becomes `fn foo<T>(...) { foo_inner(typed_args) }` where `foo_inner` is
  non-generic and takes type-erased inputs (raw pointers, `&[u8]`, indices, or
  `dyn`-typed values). The body is compiled once; the thin wrapper is inlined or
  emitted once per type. This is the canonical pattern for project-owned
  generics.
- **`#[inline]` on trivial generic wrappers.** For one-line generics such as
  `vec_capacity_bytes` / `range_slice` in `src/syntax/arena.rs`, adding
  `#[inline]` lets the body fold into each caller instead of emitting a separate
  copy per type. Lower and less predictable yield than the inner-fn pattern.
- **Runtime flag for const generics.** `fn f<const PROFILE: bool>(...)` produces
  one monomorphization per bool value (plus per-bool closures). Replacing the
  const generic with a runtime `profile: bool` collapses to one instantiation.
  Safe when the function is not on a hot path (e.g. constructors).
- **Type-erased call sites for dependency generics.** Reduce the number of
  distinct `(element type, comparator)` pairs that `slice::sort` /
  `BTreeMap::from_iter` are monomorphized for. See the follow-up below; this is
  where the runtime risk lives.

## Project-Owned Offenders (safe starting Set)

Top repeat offenders owned by the xsh crate, from `--sum` (project-owned):

| Duplicated | Inst | Function |
|---:|---:|---|
| 881 | 6 | `Evaluator::new_with_sources_and_command_inner::<const PROFILE: bool>` |
| 464 | 9 | `modules::json::raw_json_object::<T>` |
| 440 | 41 | `syntax::arena::vec_capacity_bytes::<T>` |
| 432 | 25 | `syntax::arena::range_slice::<T>` |
| 416 | 5 | `modules::archive::block_on_archive::<T, F>` |
| 402 | 6 | `Evaluator::install_compact_lowered::<const PROFILE: bool>` |
| 397 | 6 | `symbol::Name::intern::<T>` |
| 348 | 4 | `RuntimeError::new::<A, B>` |

These are the direct targets for the inner-fn / `#[inline]` / runtime-flag
techniques. Expected direct yield: ~6,281 lines (0.44%), plus indirect
dependency-machinery reduction of roughly the same order.

## Follow-Up: Hot-Path Sort Consolidation

This is the only lever large enough to approach the original 10% target. It is
documented as a follow-up because it risks regressing sort-heavy pipelines and
requires careful benchmark work. Do not undertake it without re-confirming the
user-facing benchmark suite.

### Why sort dominates

`core::slice::sort` is monomorphized once per `(element type, comparator)` pair.
Our code triggers ~59 distinct pairs, so the entire sort internals (small-sort,
quicksort, partition, merge, pivot selection) are emitted ~59 times each,
totaling 91,561 reclaimable lines (6.45%). Reducing the number of distinct
pairs is the only way to reclaim this; the std sort code itself cannot be
refactored.

### Call-site inventory

Hot (XSH `sort` / `sort-by` runtime — latency-sensitive, do not regress):

- `src/runtime/eval.rs`
- `src/runtime/eval/lowered_run.rs`

Medium (lowering time, not steady-state):

- `src/runtime/eval/lower.rs`

Cold (trace, diagnostics, archive, loader, linux modules — lower risk to
restructure):

- `src/trace.rs`
- `src/loader.rs`
- `src/diagnostic.rs`
- `src/modules/archive/tar.rs`, `src/modules/archive/cpio.rs`
- `src/modules/mod.rs`
- `src/modules/unix.rs`
- `src/modules/linux/kernel.rs`, `src/modules/linux/real/boot.rs`,
  `src/modules/linux/real/device.rs`, `src/modules/linux/real/net.rs`,
  `src/modules/linux/block.rs`, `src/modules/linux/process.rs`

Medium-cold (fs/process listing — only runs when those modules are invoked):

- `src/modules/fs.rs`
- `src/modules/process.rs`

### Techniques And Their Limits

- **Index-permutation sort.** Extract keys into a canonical
  `Vec<(K, usize)>` (e.g. `(&str, usize)` or `(u64, usize)`), sort that once,
  then reorder the original slice by the permutation. Collapses many distinct
  element types into one canonical sort. Limit: reordering non-`Copy` items in
  place requires moving them out of `&mut [T]`, which is not possible without
  `unsafe` or changing the caller to own a `Vec<T>` (so `std::mem::take` + write
  back). Most cold sorts are on non-`Copy` records, so this is invasive.
- **`Box<dyn FnMut>` comparator.** Wrapping comparators in
  `Box<dyn FnMut(&T, &T) -> Ordering>` collapses multiple comparators for the
  *same* `T` into one `sort_by` monomorphization. Limit: does not help across
  different element types, which is the common case here.
- **Canonical element type.** Where several modules sort their own record
  structs by the same kind of key, route them through a shared
  `sort_records_by_key` helper over a canonical `Vec<(String, Value)>` or
  `Vec<(String, usize)>`. Limit: only applies where the records can cheaply
  project to the canonical key without losing ordering fidelity.

### Constraints (Must Hold)

- **Stability.** Some call sites use stable `sort_by` / `sort_by_key` and rely
  on deterministic equal-key ordering for reproducible output (trace rows,
  diagnostics, module listings). Any consolidation must preserve
  stable-vs-unstable semantics, or tests in `tests/runtime/` will break.
- **Ordering fidelity.** String/path keys cannot be hashed to `u64` (hashing
  breaks total order). Use the original key type or a canonical ordered proxy.
- **No hot-path regression.** Re-run `make bench` before and after a change to
  any user-visible sort or `sort-by` path; any meaningful slowdown blocks the
  change.
- **No semantic change.** XSH `sort` / `sort-by` behavior, stability, and
  ordering are specified in `docs/SPEC.md` and `docs/STDLIB.md`; update those
  first if any visible behavior changes.

### Suggested Order Of Attack

1. Cold-path sorts in `src/trace.rs` (the largest single cold cluster).
   These render trace output only when tracing is enabled, so restructuring is
   lower risk. Measure the `slice::sort` `--sum` delta after each.
2. `src/loader.rs` and `src/diagnostic.rs` (diagnostic sorting, cold).
3. Archive and linux-module sorts (cold, module-scoped).
4. Only if the cold path banks enough headroom: revisit the hot-path sorts in
   `eval.rs` / `lowered_run.rs`, with a dedicated benchmark change and explicit
   sign-off, since those are the XSH language sort operations.

## Benchmark And Behavior Gates

```sh
make bench
cargo test --test runtime
cargo test --test sema
```

Record the `--sum` total and the benchmark numbers before starting, after each
change, and at the end. The change is done when the `--sum` delta is banked and
no gate regressed.
