# Coverage Plan

`cargo dev coverage` is the source of truth for combined Rust LLVM coverage and XSH API
coverage. Coverage work should stay behavior-oriented: prefer tests that prove
real workflows, host contracts, and safety boundaries over tests that exist only
to execute a branch.

On ARM hosts, the automatic Docker backend uses the pinned `linux/arm64` image
and `aarch64-unknown-linux-musl` target. An explicit `TARGET` or `--target`
selection is honored.

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
output used by the combined coverage tool. Use `--cov --api --cov-json` when the JSON
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

`edit.rs::tests::scripted_editor_completes_commands_after_shell_operators`
checks command candidates through the editor loop after pipes, `&&`, `||`,
and semicolons, with an argument-position control.

`complete.rs::tests::remote_completion_rejects_denial_malformed_output_and_missing_executable`
and `remote_completion_passes_host_as_one_argument_and_bounds_timeout` use
fake `ssh` executables to cover remote results, failures, quoting, and timeouts
without network access or process-wide environment changes.
`render.rs::tests` covers wide-character pre-wrap in prompts, editor lines,
multiline input, and history search, plus combining marks, pending wrap, and
narrow completion grids.

The active `tests/runtime/interactive.rs` PTY gate covers prompt startup,
terminal-mode restoration on exit, Ctrl-C, bracketed paste, and cooked-mode
external-command handoff. The PTY helper forces `NO_COLOR` for stable prompt
matching; the opt-in ANSI-color case removes it. The 17 retained opt-in cases
name their controlling-terminal, scheduling, color, or signal requirement at
each `#[ignore]`.

`app.rs::tests::single_background_job_rejects_second_slot_and_reaps_without_changing_prompt_status`
and `stopped_background_job_resumes_then_foregrounds_to_its_exit_status`
exercise the one-job state transitions without a PTY. The retained PTY job
cases cover foreground process-group signals and Ctrl-Z handoff.

## Linux Boot, Mount, And Parity Surfaces

The real Linux module coverage now includes safe container reads and temporary
filesystem/device workflows. Remaining low-coverage areas are mostly operations
that can alter global host state or require kernel capabilities that vary by
runner: boot transitions, destructive mount paths, loop/parity edge cases, and
kernel configuration writes.

`tests/linux_priv.rs::linux_priv_tmpfs_mount_is_mountpoint_disk_usage_and_cleanup`
runs the real mount in a child mount namespace with private propagation and a
temporary root. It checks the mounted filesystem inside that child, verifies
no mount remains visible in the parent after the child exits, and checks root
removal. The overlapping inherited-namespace runtime test was removed.
`tests/linux_priv.rs::linux_priv_mount_and_switch_root_fail_within_private_namespace`
binds a test `/etc/fstab` only inside the child namespace. It checks that
rejected `mount` and `mount_all` calls leave the target unmounted, that a
missing root produces an inspectable `linux-switch-root` error, and that the
parent's `/etc/fstab` is unchanged.
`tests/linux_priv.rs::linux_priv_loop_attach_list_and_detach_release_device`
uses the pinned image's BusyBox `losetup` to supply a real loop device, then
checks XSH's list, detach, attach, and eventual release operations. The host
guard detaches any loop still backed by the temporary image if the test fails.
The focused lifecycle passed 20 consecutive runs in the privileged image;
`tests/xsh/stdlib/linux.xsh` retains the dry-run parity case.
`tests/linux_priv.rs::linux_priv_mknod_creates_character_device_when_permitted`
creates its character device only inside a temporary directory in the pinned
container. The directory guard removes the node even if the assertion fails.
`tests/linux_priv.rs::linux_priv_kill_all_signals_contained_new_session_process`
relies on the pinned container's PID namespace to contain the process-wide
signal. Its helper runs in a new session, is killed and reaped by a drop guard,
and writes its readiness marker in a temporary directory. The harness also
keeps generated XSH scripts in temporary directories, so panic paths remove
them.

Useful next work:

- Add fake-root or namespace-backed coverage for boot helpers where possible,
  while keeping real `halt`, `poweroff`, `reboot`, and `switch_root` behavior
  guarded behind dry-run or harness-specific entry points.

## Lower-Value Remainders

Some low LLVM files are acceptable to leave low unless their behavior changes:
benchmark-only entry points, generated/reference documentation paths, and
dangerous platform operations without a dedicated isolation harness. Chasing
those with branch-only tests would make coverage less useful.
