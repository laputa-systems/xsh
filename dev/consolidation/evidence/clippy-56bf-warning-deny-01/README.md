# XSHT workspace Clippy failure

`correctness-56bf-20261016-r1` is the original receipt ID. It was the fifth overall correctness-gate attempt, after four attempts for `be0b`.

The receipt and three `006-warning-deny-01` logs are byte-for-byte copies of that attempt. The step ran workspace Clippy for `x86_64-unknown-linux-musl`, all targets, and all features with `-D warnings`. It exited 101 on source commit `56bf3286c0034c183515c0dc4adbf041f398a776`, tree `2d4cf52e64d4dbf06daa5e169c88d6eef8bccdd5`.

## Corrected isolated qualification

Source `e55dc6890245965961fc020a611da6d0cb537125`, tree
`21ae968ac3aac575e738b93ffb01eb9dcd4992be`, passes XSHT lib/test/all-feature
Clippy and workspace all-target/all-feature Clippy with actual `-D warnings`.
The exact commands, source pins, statuses and streams are retained in
[corrected-e55dc689](corrected-e55dc689/run.sh). These are historical commands;
recreate the source worktree before replaying with new output paths.
The pinned image, 8 CPU / 16 GiB, no swap and no network constraints are in
the command. Both child statuses and the Docker status are zero.

The owner reported 45 CLI, 34 rule, one integration and three added-fix tests
passing. Those counts are supporting lane reports, not a new canonical run.
The winddown integration has identical workspace source/build inputs to this
qualification; the additional route collector is a separate Cargo workspace.
No five-context result, canonical pass, timing, or resource acceptance follows.
