# Frontend Follow-Ups

The compact frontend and indexed runtime are complete production architecture.
This document is a measured follow-up queue, not a migration plan. Each item
needs a concrete workload, an owned cost, exact semantic parity, and a release
A/B before it becomes implementation work. The durable architecture and owner
map are in `docs/FRONTEND.md`.

## Accepted Baseline

The July 28, 2026 closeout corpus measured 287 files and 545,254 source bytes.
The indexed program reduced lower-stage retained storage by 77.81%, lower-stage
peak live bytes by 14.33%, and post-drop peak live bytes by 20.70% relative to
the pre-redesign baseline. These results close the representation redesign; they do
not make all future runtime or construction costs acceptable by default.

The JSON indexed-execution regression is also closed. The regular
`xsh_json_log_rollup_10000_rows` median was 13.58 ms on July 27, 2026, versus
13.81 ms in the repeated pre-redesign measurement. Do not reopen it without a fresh
material regression or a parity failure in the specialized paths.

## Prioritized Work

### 1. Account for Worker Memory

Ordinary rustybench allocation columns cover the controlling benchmark thread, while
indexed execution and `par-map` can run on bounded workers. Add worker-aware
allocation or RSS evidence before treating a runtime-memory claim as complete.
The output should distinguish frontend construction, controller allocation, and
worker allocation without changing product behavior.

Start with the affected prepared execution benchmark and a host RSS check.
Keep the existing `xsh-frontend-stats` accounting separate: it measures
frontend ownership, not total concurrent runtime allocation.

### 2. Reduce Construction Traffic Only When It Reaches Users

The closeout retained a smaller executable program while lower-stage allocation
traffic remained 12.68% above the pre-redesign baseline. Investigate only if repository checking,
short-script startup, or another representative workflow shows a material
allocation or latency cost.

Likely areas are parser/token text churn, rare `ArenaProgramBuilder` staging
state, repeated standalone-file setup, and transient declaration/body graph
work. Preserve the current single compact path and explicit scratch drop points;
do not add a compatibility frontend to avoid construction work.

Measure `xsht_check_xsh_repository` end to end, then use
`xsh-frontend-stats` and `scripts/ir-layout.py` to identify the owned storage or
allocation source.

### 3. Reduce Broad Runtime Value Movement

The remaining Tokei showcase gap is interpreter and dynamic-value work rather
than a missing scanner primitive. The July 28, 2026 seven-run comparison placed
the XSH default table at 1.189 s and JSON at 1.145 s, versus native tokei at
0.722 s and 0.731 s on the same Apple M1 host. A single-run RSS check found
memory at parity for those paths; treat the remaining gap as CPU work until
worker-aware memory accounting says otherwise.

Prefer changes with broad value: cheaper small shaped records, fewer complete
record clones for field/method access, ownership-preserving collection updates,
or report assembly that does not retain extra script-visible graphs. Keep
specialized instructions general and verifier-backed. Reject a change that only
improves a synthetic showcase shape or makes general value semantics less clear.

Use the prepared/execution-only diagnostic benchmark to isolate evaluator work,
then run the ordinary workload to catch setup regressions. Preserve exact output
fingerprints and stream/error/trace behavior.

### 4. Keep Indexed Execution Frames Honest

Explicit `CallFrame`, `FrameWork`, and `FrameContinuation` rows replaced a large
native evaluation-stack reservation. Their active-depth memory and dispatch
costs may merit work only if stack depth, RSS, or release latency identifies a
material regression. Cold continuations can move out of hot frame state when
that improves a real workload; do not shrink rows based on `size_of` alone.

Use `tests/runtime/stack_depth` for behavior and a focused execution benchmark
for cost. Preserve tracebacks, match/require/method continuations, defers,
signals, and call-depth behavior.

### 5. Compact Spans and Builder Staging Selectively

`AstArena` already uses promoted compact spans and keeps rare arena tables cold.
Further work may reduce retained memory, but token-derived spans cannot replace
explicit spans needed for cooked text, diagnostics, or cross-source modules.
Similarly, rare builder features should not enlarge `ArenaProgramBuilder`'s hot
header without proving a corpus benefit.

Start with `scripts/ir-layout.py --only TYPE`, then confirm the affected
frontend or repository-check allocation/peak-live result. Preserve diagnostic
locations and parser throughput.

### 6. Close Real Lowering Gaps With Parity Tests

A `compact.unlowered-*` diagnostic on current real source is a completeness bug,
not a reason to restore recursive execution. Before adding an instruction,
reproduce the gap with `tools/xsh-ir-coverage.xsh`, locate the exact
`Arena*` field or checked operation, and add the narrowest parity test.

Every new executable form needs exhaustive encoding, verifier coverage, and
value/output/error/source-span/trace parity. If the behavior is stateful or
OS-facing, model it as an explicit host/runtime operation. Do not use runnable
placeholder instructions or a per-opcode arena fallback.

### 7. Maintain PGO After Regular Measurements

PGO is not required for the accepted JSON result. After a regular-build change
passes its direct behavior and benchmark gates, regenerate profiles only when a
fresh PGO comparison is useful. Treat short benchmark deltas as noise until
focused repetition confirms them.

## Rejected Directions

- Do not restore recursive AST or lowered-program execution as a production
  fallback, a shadow executor, or a diagnostic command-line switch.
- Do not retain build-time semantic canonical maps or dynamic name spellings for
  process lifetime merely to avoid ownership work.
- Do not add benchmark-shaped standard-library helpers or an opcode that lacks a
  general semantics, compact encoding, and verifier contract.
- Do not pursue a bytecode VM unless indexed coverage is broad, measured dispatch
  remains the bottleneck, and the VM can preserve tracing, errors, processes,
  streams, environment/cwd mutation, defers, and signals exactly.
- Do not treat debug timings, `size_of`, or controlling-thread allocations alone
  as acceptance evidence.

## Measurement Loop

1. Name the user-visible workload and the expected cost reduction.
2. Capture a regular-build baseline with the narrowest benchmark in
   `docs/TEST-MAP.md` or `docs/BENCHMARKING.md`.
3. Add direct behavior and indexed parity coverage before accepting a fast path.
4. Use `xsh-frontend-stats` for retained/peak frontend ownership,
   `scripts/ir-layout.py` for layout hypotheses, and execution-only diagnostics
   for evaluator hypotheses.
5. Run the applicable `make bench-fast` or `make bench` gate only after the
   focused result is credible.
6. Keep a change only when it preserves semantics and materially improves the
   stated workload, or document why a non-performance contract justifies it.

Do not run formatters or autofixers during this workflow. Use the direct
frontend, frame, runtime, and benchmark commands in `docs/TEST-MAP.md`.
