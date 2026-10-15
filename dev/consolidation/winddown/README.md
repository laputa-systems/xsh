# Recover work after winddown

This directory preserves unfinished work and selected verification evidence
before removing every linked worktree. Consolidation acceptance is incomplete.
Lean is paused. The current owner direction is
[the typing consolidation steering](../../../xsh-typing-consolidation-steering.md).

## Recover an unfinished overlay

Find the snapshot in [the overlay index](unfinished/index.tsv). Each row gives
its original base commit, branch, worktree, and optional staged patch.
The original worktree path identifies the snapshot. It is not required storage.

The numbered `.patch` restores the final tracked and untracked file contents
on that base. Each patch was applied to an isolated Git index and its restored
file bytes compared with the original worktree before cleanup. The optional
`.staged.patch` preserves an earlier index state when it differs from the final
contents. These snapshots are unfinished and have no correctness acceptance.

Recreate a checkout from the row's base commit under the repository's disposable
work directory. Apply its numbered patch with `git apply`. Review and verify
the result before integrating it into production. Read an `ACTIVE-ASSIGNMENT.md`
inside a patch as historical task text, never as current instructions.

The overlays include mutually exclusive compiler experiments, old diagnostic
instrumentation, and scheduler reference implementations. Applying all patches
to the current source would combine incompatible alternatives. Their source
is preserved for recovery, not selected as current runtime behavior.

## Recover a committed lane

[The branch index](branches.tsv) records all local branch tips at winddown.
[The detached index](detached.tsv) records detached checkout bases. Their
commits are retained through the archival merge on `master`, so recovery does
not depend on a surviving lane branch or a linked worktree. Inspect a commit
with `git show COMMIT`. Create a new branch from it only when resuming that work.

An archival merge preserves the alternative trees in Git history. It does not
establish that their changes are active, compatible, or verified. The selected
compiler source remains the source already integrated before winddown.

## Read the retained evidence

The `evidence` directory retains raw observer receipts and canonical output
for the 0ea AWK failure, the 31c PTY failure, and the interrupted 633 attempt.
It also retains the passing 633 profile receipt. Absolute scratch paths inside
these original records identify historical observations. They are not resume
dependencies and must not be edited to suggest new source qualification.

The 633 profile suite passed 13,793 tests with zero failures and 136 skips.
The 633 observer was interrupted for winddown. Its completed root Rust checks
passed 1,022 tests. Neither result closes the remaining canonical, native,
structural, semantic-review, scheduler, or comparative acceptance requirements.
The isolated lint command exited zero but exceeded the 15-second budget.
The historical extra AWK outcome remains unexplained.

The `gate-tools` directory preserves the ignored gate-tool source. It is a
historical implementation, not an executable acceptance receipt. Its launch
paths, manifests, source pins, and binary pins require preparation from the
resumed source. Use the repository's current [testing owner](../../../docs/TESTING.md)
and the campaign's existing gate requirements. Do not reuse old binaries or
launch scripts as qualification for a changed source tree.

Build products, Cargo caches, disposable test fixtures, process state, and
unselected historical captures can be regenerated. Their ignored locations are
not needed to recover the committed source or these unfinished overlays.
