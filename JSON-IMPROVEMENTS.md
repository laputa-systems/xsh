# JSON Improvements

## Status

JSON-heavy indexed execution remains a measured performance follow-up after the
frontend redesign. It is not a Stage 11 completion blocker, but it must not be
represented as solved by PGO.

On the July 27, 2026 Apple M1 campaign host, repeated release measurements for
`xsh_json_log_rollup_10000_rows` were:

| build | median |
|---|---:|
| Phase 0 regular | 13.81 ms |
| final regular | 23.13 ms |
| final PGO | 19.28 ms |

PGO improves the final indexed path by about 17%, but the PGO result remains
about 40% slower than the Phase 0 regular result. The final operation retains
409 benchmark-thread allocations and 53.77 KiB allocated, versus 299
allocations and 50.73 KiB in the repeated Phase 0 comparison. Divan does not
count allocations performed by the evaluation worker, so those columns are not
a complete runtime-allocation comparison.

## Findings

- The regression begins at the Phase 6 direct indexed execution cutover.
- Phase 9 explicit call frames are not the original regression point.
- PGO materially helps indexed expression dispatch but does not close the gap.
- Restoring the old stream-prefix shape alone did not improve the rollup, so the
  remaining cost is broader than one missing pipeline fusion.
- Sampling attributes most indexed runtime CPU to `eval_indexed_expr`; the old
  path spent the corresponding time in `eval_lowered_expr_inner` but completed
  the workload materially sooner.
- Repository-check latency was a separate repeated call-graph construction
  issue and is fixed independently; it should not be conflated with JSON
  execution work.

## Next Work

1. Add a benchmark that separates parse/check/lower time from execution time for
   the JSON rollup without creating a second performance authority.
2. Add worker-aware allocation or RSS accounting for indexed execution so
   runtime allocations are not hidden by the benchmark thread boundary.
3. Measure operand decoding, instruction dispatch, field projection, method
   dispatch, and pipeline-stage execution separately inside the existing
   workload.
4. Remove redundant runtime validation only if the whole-store verifier can
   make the trusted-execution invariant explicit and malformed-store tests stay
   at the verifier boundary.
5. Compare compact payload decoding with a non-recursive fixed-width execution
   cache. Do not reintroduce recursive lowered expression ownership.
6. Revisit streaming fusion only after the execution-only benchmark shows that
   collection or intermediate lists are still material.
7. Repeat regular and PGO measurements serially on the same host, preserving the
   Phase 0 fixture and benchmark setup.

## Acceptance

Close this follow-up only when repeated regular release measurements are within
the campaign's material-regression threshold, behavior and trace parity remain
exact, the compact program remains the sole production representation, and PGO
is an additional improvement rather than the mechanism that hides a regular
build regression.
