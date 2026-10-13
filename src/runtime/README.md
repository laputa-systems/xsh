# Runtime

The runtime lowers checked programs into a verified indexed `FullProgram`
before execution. It owns values, scopes, host processes, cwd/env state, and
trace events. See [`Runtime design`](../../docs/ARCHITECTURE.md#runtime-design)
for the execution and ownership invariants.

## Routing

| Change | Start here |
|---|---|
| checked program lowering and indexed IR verification | `src/runtime/eval/lower.rs`, `src/runtime/eval/indexed/full.rs` |
| indexed expression and statement execution | `src/runtime/eval/lowered_run/indexed_run.rs` |
| function call dispatch | `src/runtime/eval/lowered_run/indexed_run/calls.rs`, `src/runtime/eval/call_plan.rs` |
| top-level drivers and explicit frame scheduling | `src/runtime/eval/lowered_run/indexed_run/driver.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs` |
| pipeline stages | `src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs`, `src/runtime/eval/lowered_run/indexed_run/pipeline.rs` |
| `run`, `spawn`, `wait`, captures, argv | `src/runtime/run.rs`, `src/runtime/process.rs` |
| namespace changes made by a spawned child (`linux.run_in_namespaces`) | `src/runtime/namespace.rs`, `src/runtime/eval/lowered_run/namespaces.rs` |
| stream ownership and values | `src/runtime/eval/stream.rs` |
| standard module operations needing evaluator state | `src/runtime/eval/lowered_run.rs`, `src/runtime/eval/lowered_run/module_*.rs` |
| value method dispatch and operations | `src/runtime/eval/lowered_run.rs`, `src/runtime/eval/lowered_run/indexed_run/values.rs`, `src/runtime/eval/lowered_ops.rs` |
| runtime values and error constructors | `src/runtime/value.rs` |

Preserve source-visible order, explicit process boundaries, and traceable
failure paths. Expected host failures should normally return `Result` values;
runtime errors are for behavior that cannot be represented as ordinary failure
data.

For process groups, signals, cancellation, cwd/env mutation, or signal hooks,
read `docs/SPEC.md` sections 11-12 before editing.
