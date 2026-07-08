# Inline XSH Test Footprint

This tracks Rust tests that still construct inline XSH scripts through helpers
such as `run_temp_script`, `run_temp_script_with_args`, `write_temp_script`, and
`run_cancelable_temp_script`.

## Current Count

Snapshot after the first native-test migration pass:

| File | Matches | Notes |
|---|---:|---|
| `tests/runtime/modules.rs` | 55 | Largest remaining target. Mix of module integration, generated fixture setup, imports, archive/json/hash/bytes workflows, and diagnostics. |
| `tests/runtime/process.rs` | 38 | Mostly process lifecycle, cancellation, signals, spawn behavior, and Rust-side control. |
| `tests/runtime/linux.rs` | 20 | Platform-gated and privileged/dry-run Linux behavior. |
| `tests/runtime/unix.rs` | 12 | Unix dry-run/process group/TTY style behavior. |
| `tests/runtime/streams.rs` | 12 | Remaining cases are mostly generated stress, signal/cancellation, or behavior where the Rust harness controls the process. |
| `tests/runtime/common.rs` | 10 | Harness helper definitions and internal helper use, not migration targets by themselves. |
| `tests/runtime/run.rs` | 8 | Mostly CLI/tooling, cgroup/platform, or direct `xsht`/trace-file behavior. |
| `tests/runtime/retry.rs` | 5 | Candidate for native migration if native tests get retry/state helpers. |
| `tests/runtime/coverage.rs` | 4 | Tooling/check/lint integration; likely belongs in Rust unless native tests can run `xsht` subcommands generally. |
| `tests/runtime/stack_depth.rs` | 2 | Stack-size environment and failure-mode coverage. |
| `tests/runtime/collections.rs` | 1 | Left because direct native migration exposed a laziness/evaluation-order mismatch. |
| `tests/runtime/interactive.rs` | 1 | Interactive harness behavior. |

Total current matches: 168, including the 10 helper-definition matches in
`tests/runtime/common.rs`.

## Migration Priority

Good native-test candidates:

- `tests/runtime/modules.rs` ordinary stdlib behavior with deterministic
  stdout/stderr/status.
- `tests/runtime/retry.rs` if the assertions do not need Rust-side sleeps,
  cancellation, or shared state.
- Small `tests/runtime/run.rs` cases that can be expressed with
  `test.run_script`, `test.run_xsh`, or `test.run_xsht_trace`.

Keep in Rust unless the native harness grows more facilities:

- cancellation and signal timing that requires Rust to spawn, wait, and send
  signals at specific checkpoints
- TTY, process group, cgroup, ptrace, platform-gated, or privilege-sensitive
  behavior
- generated stress scripts where Rust builds large input programs or argv sets
- `xsht` tool behavior that needs command-level file outputs, config roots, or
  subcommand-specific assertions
- cases where native migration changes observed semantics instead of just
  moving assertions

## Test-Only Ops Placement

The new helpers are currently language-facing `test` module APIs:

- `test.run_script`
- `test.run_xsh`
- `test.run_xsht_trace`

They are declared in `crates/xsh-registry`, adapted by `src/modules/signature.rs`,
checked by the main checker, lowered as `RuntimeOp`s, and executed inside the
main `Evaluator`.

Moving their implementation fully into `crates/xsht` is not currently a simple
file move. Native tests are parsed, checked, lowered, and evaluated by the main
`xsh` frontend/runtime. `xsht` owns discovery, scheduling, temp roots, reporting,
coverage aggregation, and example execution, but module call signatures and
runtime dispatch are not injectable per tool invocation.

Feasible directions:

1. Keep the public `test` module signatures in `xsh-registry`, but route
   test-only host behavior through an `Evaluator` host callback installed by
   `xsht`. This would move subprocess implementation details out of
   `lowered_run.rs` while preserving normal checking/lowering.
2. Add an overlay API spec for native-test mode. `xsht` would pass extra
   module signatures into parsing/checking/lowering. This is more invasive
   because the checker and lowerer currently read the global standard API spec.
3. Keep the helpers in the main runtime, but isolate their implementation in a
   small `src/runtime/eval/modules/test.rs` adapter. This does not move them
   into `crates/xsht`, but it reduces main-runtime clutter and keeps test-only
   code clearly fenced.

Most practical next step: option 1. Add a narrow `TestHost`/`NativeTestHost`
interface to `Evaluator` or `LoweredSharedState`, with default implementations
that return a structured unsupported error outside `xsht`. Then `xsht` can
install the subprocess runner for native tests without making the main runtime
know how to find `xsht` or spawn child scripts.
