# JSON Improvements

## Status

Accepted on July 27, 2026.

The regular release `xsh_json_log_rollup_10000_rows` median is now 13.58 ms on
the Apple M1 campaign host, compared with the repeated Phase 0 median of
13.81 ms. The final result is 1.67% faster than Phase 0 and is within the
campaign's material-regression threshold. The compact indexed program remains
the sole production representation.

The Stage 11 PGO result predates these changes and is no longer the current JSON
comparison. PGO is not needed to reach regular-build parity; regenerating the
profile is ordinary future benchmark maintenance.

## Measurements

All measurements use the existing curated Divan workload and fixture.

| milestone | regular median |
|---|---:|
| Phase 0 | 13.81 ms |
| Stage 11 final | 23.13 ms |
| borrowed indexed field chains | 17.40 ms |
| compiled field/literal `where` predicates | 15.71 ms |
| retained record-key ownership | 14.35 ms |
| direct shaped JSON object construction | **13.58 ms** |

An ignored diagnostic benchmark separates preparation from execution without
changing the curated suite or PGO workload. Before optimization, execution
alone measured 22.63 ms while the complete workload measured 23.13 ms, showing
that parse/check/lower contributed less than 1 ms and the regression belonged
to indexed execution.

The ignored execution benchmark reports four allocations and 2.296 KiB on the
controlling benchmark thread. Evaluation still runs on its bounded worker, so
Divan's allocation columns do not represent total execution allocations. The
normal curated workload continues to report 409 controlling-thread allocations
and 53.77 KiB.

## Changes

- Added an ignored execution-only form of the existing JSON rollup benchmark.
  Input generation prepares and verifies the compact program outside measured
  time, while the measured closure executes the same installed indexed plan.
- Restored borrowed field-chain projection over indexed instruction IDs. A
  field read from a slot now clones only the selected leaf instead of cloning
  and dropping the complete record first.
- Reused verified direct-field shapes in `sort-by`, `group-by`, and `map`
  stages, retaining the general indexed expression fallback for all other
  expressions and value kinds.
- Compiled direct string field comparisons and their `and`/`or` composition
  once per `where` stage. Per-item execution no longer constructs binary work
  stacks or re-decodes the same predicate tree.
- Preserved owned dynamic `NameText` spellings while converting shaped runtime
  records to lowered records. Dynamic keys now clone the existing `Arc<str>`
  instead of allocating a replacement from `&str`.
- Built JSON object records directly from `(Name, Value)` rows instead of first
  constructing a temporary `BTreeMap<Arc<str>, Value>` and then converting it
  into the shaped record representation.

## Findings

- The original regression began at the Phase 6 direct indexed execution
  cutover, not at Phase 9 explicit frames.
- Preparation was not material; nearly the entire Stage 11 median was indexed
  execution.
- The largest cost was lost borrowing semantics for field reads. General field
  evaluation cloned complete JSON records for expressions such as `.level`,
  `.service`, and `.duration_ms`.
- Repeated generic expression dispatch inside pipeline stages was the second
  material cost. Verified projection and predicate shapes recover the old cheap
  path without adding benchmark-specific opcodes.
- Phase 7 shaped records exposed an ownership conversion cost: interned dynamic
  keys were converted back through borrowed strings and reallocated.
- Restoring the old stream-prefix shape alone did not improve the rollup and was
  not retained.

## Verification

Passing focused gates include:

- prepared benchmark execution parity with normal `run_script` execution;
- compact indexed JSON rollup runner coverage;
- JSON runtime behavior;
- structured stream integration tests;
- runtime record-value tests;
- repeated regular execution-only and complete JSON rollup benchmarks.

## Remaining Work

Worker-aware allocation or RSS accounting would make future runtime-memory
investigations more complete. It is not required for the accepted latency
result and remains ordinary benchmarking maintenance. Future changes should
preserve the ignored execution-only diagnostic, rerun regular measurements
first, and regenerate PGO only after the ordinary gate remains clean.
