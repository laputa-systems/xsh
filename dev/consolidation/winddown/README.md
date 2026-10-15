# Recover work after winddown

This directory preserves unfinished work and selected verification evidence
before removing every linked worktree. Consolidation acceptance is incomplete.
Lean is paused. The current owner direction is
[the typing and formal successor requirements](../CAMPAIGN.md#typing-and-formal-successor-requirements).

## Recover an unfinished overlay

Find the snapshot in [the linked overlay index](unfinished/index.tsv) or
[the standalone overlay index](unfinished/standalone-index.tsv). Each row gives
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
commits remain on their named local branches. Additional detached tips have
explicit tags in [the recovery-tag index](detached-refs.tsv). Recovery does
not depend on any linked worktree. Inspect a commit with `git show COMMIT`.
Create a new checkout only when resuming that work.

[The standalone repository index](standalone-repositories.tsv) records another
79 self-contained repositories and 11 of their linked checkouts found inside
the disposable directory. [The imported-ref index](standalone-source-refs.tsv)
names 92 additional source tips retained under explicit parked branches.
Their histories include 136 commits previously absent from the primary
repository. Local fetches retain the source without selecting it for execution.

Automatic approval review rejected a mass archival merge because it would mark
unintegrated alternatives as merged. The 824 original non-ancestor branches
and 92 imported source-tip branches remain visibly parked. They are not
accepted implementation. The selected compiler
source remains the source already integrated before winddown.

The stale `strict-rust-0107` partial checkout contains no unique source: all
1,817 regular source files match `0107c8158bf8576a6d28953143cab302d7abb17e`.
The stale uutils checkout matches its external repository at
`e7c9f3194280835c4487c2945c68d5f01ccacc8d`, apart from an empty generated ZIP.
Neither partial tree is required storage.

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

The ignored `.campaign-evidence/consolidation` directory outside the disposable
work directory holds the large historical captures. These captures are optional
evidence; source recovery and resume do not require them. Keep them out of Git.

`w9-raw.tar.zst.part-00` and `w9-raw.tar.zst.part-01` retain the 208 candidate,
derived reference, comparison receipts, and both exact-owner captures. Join
the parts in name order and decode with `zstd --long=28 -d` before reading the
tar archive. All 61,825 archived files were compared byte for byte with their
original captures. `w9-input-corpus.tar.zst` retains the 1,690 frozen input
files and their original manifest. Each input matched that manifest before
archiving. These are historical finite observations, not current-source or
universal correctness claims. The original receipt paths remain historical.

The `gate-tools` directory preserves the ignored gate-tool source. It is a
historical implementation, not an executable acceptance receipt. Its launch
paths, manifests, source pins, and binary pins require preparation from the
resumed source. Use the repository's current [testing owner](../../../docs/TESTING.md)
and the campaign's existing gate requirements. Do not reuse old binaries or
launch scripts as qualification for a changed source tree.

Build products, Cargo caches, disposable test fixtures, process state, and
unselected historical captures can be regenerated. Their ignored locations are
not needed to recover the committed source or these unfinished overlays.
