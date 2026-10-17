# Cached child kinds in recursive `rm`

This is a source-only improvement separate from the still-unresolved R2
`test_gnu_rm_ext3_perf_log` failure. The original profile reported status 3
with empty stderr; the observation below does not identify its cause or claim
to fix it.

## Change and behavior checks

Commit `a745248e2a65b683b219c54229e0e7e806f82fec` uses the no-follow kind
already returned by `fs.children(stat: false)` for quiet `rm -rf` leaf entries.
It avoids one pre-unlink metadata lookup for those leaves. Directory entries,
one-file-system, interactive, verbose, or progress modes keep the prior path.
On an unlink error, a fresh `stat` returns to recursive removal only when it
proves the entry became a directory; otherwise the original unlink error
follows the existing force-missing/reporting policy.

The candidate source was tested from the dirty worktree before commit, based
on the frozen `24cc0d704209a8c0aa0eb27d5cfea1434929dc20`. It was not tested as
that commit's HEAD. Candidate source SHA-256 values at test time and after
execution were identical to the committed blobs:

- `core/rm.xsh`: `3d655a686043960a9e20592dfc97ea88956b271612a17c22c206619223b92df2`
- `core/tests/test-gnu-rm.xsh`: `fd6accc08b60b98bcc0df01aa9d8899273415dda91517eafce76f22196692a12`

The unchanged protected native products were `xsh`
`e79fd4820b23b840c0bbface75a8b24780eae8fb7aedc34a01d76045f9aeae6b`, `xsht`
`daa6c47667f65e8a2fe06a04ff179ee99e6286496c6d441b193889b3b9fec7fa`, and
`xsh-test-helper`
`cf8005649749a43cc22aa5e40092a8f90365efc370b857fce5de2004adb2e621` before
and after the runs. Runs used pinned image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`,
4 CPUs 28–31, 8 GiB memory with no extra swap, and native lock inode `62012470`.

After adding the explicit `Removal` result type, all ten focused selectors
passed (1 passed, 0 failed, 0 skipped each): the new recursive symlink test,
recursive removal, mixed denied/removable siblings, EACCES, verbose recursion,
one-file-system, interactive recursion/always, and force-missing controls.
`controls/stdout` retains exact test output; stderr is empty and exit status is
0. The mixed-sibling case verifies best-effort removal with a denied leaf.

The initial candidate attempt is retained in `initial-type-error.stdout` with
its exit status. The selectors failed type checking because the conditional
inferred a nullable `write_error`; no candidate count from that attempt is
valid. The explicit annotation fixed the checker error before the focused
selectors and candidate count below.

## 10,000-leaf operation counts

Baseline and candidate used the same `measure.sh`: create 10,000 regular files
outside tracing, then run the unchanged `uu-launch.xsh` wrapper and
`xsh core/rm.xsh -- -rf d` under `strace --seccomp-bpf -f -qq -c`, counting
`newfstatat,lstat,unlink,unlinkat`. Raw summaries are in `counts/`; the
candidate's script was byte-identical to the retained baseline script. The
derived count file reads the `calls` column from those raw summaries:

| Source | `lstat` | `unlink` | rm status | Scene |
|---|---:|---:|---:|---|
| Frozen `24cc` baseline | 20,004 | 10,000 | 0 | tree removed |
| Candidate source | 10,004 | 10,000 | 0 | tree removed |

This is an operation-count observation, not a wall-time or speedup claim.
Strace setup/summary time is not used. The first count report parser read the
wrong table column because strace omits the zero-valued `errors` column; its
controller returned 1 after the raw runs. Counts were corrected from the
retained raw `calls` column without rerunning either measurement. The controller
result and corrected count are retained.
Both exact cleanup checks returned `remove_status=0 list_status=0`; scene and
scene outputs confirm the test trees were removed. No native full-suite rerun
was performed here.

After commit, the unchanged 400,000-entry
`test_gnu_rm_ext3_perf_log` selector passed on the candidate: 1 passed,
0 failed, 0 skipped; selector and Docker status were both 0. Its exact output,
source/product guards, and container cleanup result are in `ext3-400k/`. The
fixture's displayed elapsed time is retained only as raw output; no performance
gate was run. Passing this isolated selector does not identify or establish a
fix for the original R2 failure, and no R2 causal or acceptance claim is made.
