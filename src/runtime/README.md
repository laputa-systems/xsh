# Runtime

The runtime evaluates checked ASTs, runs host processes, manages cwd/env state,
and records trace events. `src/runtime/eval.rs` owns evaluator state; focused
behavior lives in `src/runtime/eval/*`.

## Routing

| Change | Start here |
|---|---|
| expression evaluation | `src/runtime/eval/expr.rs` |
| statement evaluation | `src/runtime/eval/stmt.rs` |
| proc, pure, or call dispatch | `src/runtime/eval/call.rs` |
| command forms | `src/runtime/eval/command.rs` |
| `run`, `spawn`, `wait`, captures, argv | `src/runtime/run.rs`, `src/runtime/process.rs` |
| structured streams | `src/runtime/eval/stream.rs` |
| standard module dispatch needing evaluator state | `src/runtime/eval/modules.rs` |
| value methods | `src/runtime/eval/methods.rs` |
| lowered IR | `src/runtime/eval/lower.rs`, `lowered_ops.rs`, `lowered_run.rs` |
| runtime values and error constructors | `src/runtime/value.rs` |

Preserve source-visible order, explicit process boundaries, and traceable
failure paths. Expected host failures should normally return `Result` values;
runtime errors are for behavior that cannot be represented as ordinary failure
data.

For process groups, signals, cancellation, cwd/env mutation, or signal hooks,
read `docs/SPEC-OS.md` before editing.
