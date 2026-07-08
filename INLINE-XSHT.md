# Inline XSH Test Footprint

This tracks Rust tests that still construct inline XSH scripts through helpers
such as `run_temp_script`, `run_temp_script_with_args`, `write_temp_script`, and
`run_cancelable_temp_script`.

## Current Count

Snapshot after the next native-test migration pass:

| File | Matches | Notes |
|---|---:|---|
| `tests/runtime/modules.rs` | 15 | Remaining cases use Rust-side fixture generation, helper binaries, network servers, invalid UTF-8 process state, or archive inode checks. |
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

Total current matches: 128, including the 10 helper-definition matches in
`tests/runtime/common.rs`.

## Migration Priority

Good native-test candidates:

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

The native helpers are language-facing `test` module APIs:

- `test.run_script`
- `test.run_xsh`
- `test.run_xsht_trace`

The implemented split keeps native test typechecking/lowering in `xsh`, but
behind a non-default `native-tests` feature:

- `crates/xsh-registry` now gates `RuntimeOp::Test*`, `TestContext`,
  `TestCall`, `TestScriptOutput`, and the `test` module signatures behind
  `native-tests`.
- `xsh` exposes a matching `native-tests` feature and gates native test
  evaluator APIs and dispatch.
- `crates/xsht` depends on `xsh` with explicit features:
  `native-tests`, `net`, and `tools`.
- The subprocess implementation for `test.run_script`, `test.run_xsh`, and
  `test.run_xsht_trace` lives in `crates/xsht` as an `Evaluator` native-test
  host callback. Core `xsh` keeps only argument conversion, temp-path
  allocation, and callback dispatch.

This avoids a Cargo cycle. `xsh` cannot directly call into `xsht`, because
`xsht` already depends on `xsh`; the dependency direction remains:

```text
xsht -> xsh -> xsh-registry
```
