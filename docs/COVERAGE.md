# Coverage Plan

`cargo dev coverage` is the source of truth for combined Rust LLVM coverage and XSH API
coverage. Coverage work should stay behavior-oriented: prefer tests that prove
real workflows, host contracts, and safety boundaries over tests that exist only
to execute a branch.

For a focused XSH source-coverage report, run `xsht test --cov`. The report
registers source files discovered by the active `xsht-config.ini`, including
files that no test loads, and derives its source-line denominator from parsed
executable statements rather than counting every nonblank physical line. The
source-line numerator is still based on runtime source spans, so it is a useful
diagnostic rather than precise statement or branch coverage.

Use `[coverage]` `exclude` patterns in `xsht-config.ini` to remove known source
families from this denominator without removing them from `xsht check`, `xsht
fmt`, or `xsht lint` discovery.

The procedure metric is reported as `proc entries`: it answers whether a proc
or pure function was entered at least once. It does not prove that every
statement or branch in the procedure ran. Add `--api` when API-surface coverage
is the question; `--cov-json` remains the machine-readable source-coverage
output used by the combined coverage tool. Use `--api --cov-json` when the JSON
report should also include standard API hit data. JSON reports also include
`source_scope.files` and `source_scope.observed_files` so a result cannot
silently be mistaken for whole-repository coverage.

The standard XSH API surface is currently covered by the native suites. The
remaining meaningful LLVM gap is concentrated in two areas that need larger
harnesses, not scattered microtests.

## Interactive Editor And Completion

The deterministic unit tests cover core line editing, preview, autosuggestion,
completion replacement, grid movement, SSH host discovery, and render wrapping.
`crates/xshi/src/interactive/edit.rs::tests::ScriptedEditorInput` now feeds key
bytes and a fixed terminal size through the real editor loop, capturing the
rendered output without a PTY or timing sleeps. Its cases cover cursor edits,
ambiguous completion opening, arrow and Tab navigation, preview, acceptance,
Escape/Ctrl-C cancellation, filtering, and grid clearing when edits remove all
matches. Deterministic directory cases cover quoted paths, `~/` insertion,
hidden paths, directory-only `cd`/`z`, and prefix-before-substring fallback.
History search cases cover selection, acceptance, Escape/Ctrl-C cancellation,
and restoration of the saved line. Autosuggestion cases cover Right Arrow
acceptance and suppressing ghost text during completion and history search.
`app.rs::tests::path_completion_refreshes_after_set_unset_assignment_and_denv`
covers command candidates across session environment changes;
`complete.rs::tests::completion_refreshes_cwd_snapshot_and_non_cwd_directory_mtime`
covers cwd snapshot refresh and mtime-based directory cache replacement.

Useful next work:

- Cover command-position completion and remote path fallbacks.

The active `tests/runtime/interactive.rs` PTY gate covers prompt startup,
terminal-mode restoration on exit, Ctrl-C, bracketed paste, and cooked-mode
external-command handoff. The PTY helper forces `NO_COLOR` for stable prompt
matching; the opt-in ANSI-color case removes it. The 20 retained opt-in cases
name their controlling-terminal, scheduling, color, or signal requirement at
each `#[ignore]`.

## Linux Boot, Mount, And Parity Surfaces

The real Linux module coverage now includes safe container reads and temporary
filesystem/device workflows. Remaining low-coverage areas are mostly operations
that can alter global host state or require kernel capabilities that vary by
runner: boot transitions, destructive mount paths, loop/parity edge cases, and
kernel configuration writes.

Useful next work:

- Build a privileged Linux harness with isolated mount namespaces, temp roots,
  and explicit cleanup checks before enabling broader mount and switch-root
  tests.
- Add fake-root or namespace-backed coverage for boot helpers where possible,
  while keeping real `halt`, `poweroff`, `reboot`, and `switch_root` behavior
  guarded behind dry-run or harness-specific entry points.
- Extend loop-device and parity tests only when the runner can allocate and
  clean up devices reliably; otherwise keep these as dry-run parity fixtures.
- Document every test that mutates kernel or mount state with its isolation
  boundary and cleanup guarantee.

## Lower-Value Remainders

Some low LLVM files are acceptable to leave low unless their behavior changes:
benchmark-only entry points, generated/reference documentation paths, and
dangerous platform operations without a dedicated isolation harness. Chasing
those with branch-only tests would make coverage less useful.
