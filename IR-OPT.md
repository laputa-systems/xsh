# IR Optimization Plan

This file tracks focused front-end and lowered-IR allocation work. Keep it
short: record the current measurement command, the next good ideas, and the
latest decision points.

## Measurement

Use the frontend Criterion group for parse, desugar, semantic check, lowering,
and evaluator setup before top-level execution:

```sh
cargo bench --bench bench frontend -- --sample-size 10 --warm-up-time 0.5 --measurement-time 1
```

The benchmark prints an allocation audit before Criterion timing. Treat
allocation counts and allocated bytes as the stable signal; local Criterion
means are directional.

Verification gate for front-end/lowered setup changes:

```sh
cargo check --benches
cargo test --test syntax
cargo test --test sema
cargo test --test runtime
```

## Current Shape

The 2026-06-15 baseline after the string-churn cleanup leaves empty
`parse/check/lower` near the fixed floor at about 68 allocations. The remaining
interesting `+lower` allocation counts are concentrated in:

- `pure_call_chain_20k`
- `mixed_glue_2k`
- `stream_callback_pure_5k`

Small bookkeeping cleanups are unlikely to move the suite much from here.

## Tranches

1. Reduce boxed lowered expression/statement representation where it duplicates
   the checked AST. This is the most likely IR-local win, but it needs a careful
   node-table or arena-shaped design.
2. Audit and shrink small lowered vectors and maps. `smallvec` is already in
   the resolved dependency graph via net dependencies and is MIT/Apache-2.0.
   Use it for non-recursive metadata first, such as function params and
   top-level sync slots. Direct inline `SmallVec<LoweredExpr>` fields do not
   work inside `LoweredExpr`; call args, method args, tag fields, and record
   fields need a different representation if we want to remove those
   allocations.
3. Avoid top-level slot metadata where impossible. Some lowered top-level
   statements may not need sync slots at all.
4. Consider compact token payload storage only if identifier/string-heavy
   scenarios still allocate meaningfully after IR-local work.
5. Treat arena-backed source AST storage as a larger parser/checker project. It
   can pay off, but crosses formatter, diagnostics, checker, and runtime
   ownership.

Lazy cwd is low priority: the fixed floor is already small, and cwd/path/process
semantics depend on a concrete current directory.

## Notes

2026-06-15 small-vector slice:

- Added `smallvec` as a direct dependency at the already-resolved `1.15.2`.
- Kept `SmallVec` only on non-recursive lowered metadata: function parameter
  names, function parameter kinds, top-level sync slots, and tag-pattern slots.
- Do not put `SmallVec<LoweredExpr>` or `SmallVec` of any type containing
  `LoweredExpr` directly inside `LoweredExpr`. That makes the recursive enum
  layout infinite unless another indirection is added, and adding a `Box` would
  usually give back the allocation we are trying to remove.

Frontend allocation audit after this slice:

| Scenario | Previous +lower allocs | Current +lower allocs | Delta |
| --- | ---: | ---: | ---: |
| empty | 68 | 68 | 0 |
| loop_10k | 102 | 99 | -3 |
| pure_call_chain_20k | 269 | 259 | -10 |
| stream_callback_pure_5k | 220 | 215 | -5 |
| mixed_glue_2k | 351 | 345 | -6 |
| import_disk | 396 | 392 | -4 |

2026-06-15 top-level sync-slot attempt:

- Tried tracking referenced top-level slots while lowering so
  `LoweredTopLevelStmt` would sync only bindings actually used by the lowered
  statement.
- An `FxHashSet` reference tracker reduced some sync metadata but added more
  allocation and clear timing overhead; it was not kept.
- A cheaper top-level-only inline reference log avoided the timing regression,
  but allocation counts were neutral except `mixed_glue_2k`, which regressed
  from 345 to 349 `+lower` allocations. It was also not kept.
- Conclusion: after `LoweredTopLevelSlot` moved to `SmallVec`, tranche 3 has no
  good standalone win in the current frontend suite. Revisit only with a
  broader lowered representation change that can know used slots without extra
  tracking.
