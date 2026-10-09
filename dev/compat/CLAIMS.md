# Integration status and claims

Updated 2026-10-09 on `campaign-utils`. One integrator owns shared results,
baselines, campaign notes, merges, and pushes. There are no active lane claims;
the previous lane commits are merged into the integration branch. Clean lane
worktrees under `/workspace/xsh-lanes` are archived and do not indicate active
work.

## Setup

The campaign runs directly on native Linux x86_64. Docker is outside scope.
The workspace uses `nightly-2026-09-15` and mold 3.0.0 from its x86_64 GitHub
release; `dev/compat/native-env.sh` checks mold and configures the native build
environment. The release archive SHA-256, bootstrap commands, host
prerequisites, GNU 9.12 setup, and the host-specific `CARGO_PROFILE_RELEASE_LTO=false`
workaround are in [`README.md`](README.md#setup). These instructions have been
exercised with the release XSH dispatcher, pinned uutils harness, and GNU
suite.

## Current evidence

The current head has a fresh full uutils run from 2026-10-09 with **5,166 /
5,974 passing**, 808 failing, and 4 excluded. Comparison with the preceding
merged report found no regressions. Rerun the full suite after the next
integration batch.
The current focused `tty` slice is 11/11; GNU 9.12's `tests/tty/tty.sh`
passes. The remaining `who` failures expect uutils' extra-operand and
write-error wording. Exact residuals are in `gaps.json` and
`results/uutils-integration.json`.

The GNU differential is 300 / 273 / 6 / 57 across uutils-pass/XSH-pass,
uutils-pass/XSH-fail, uutils-fail/XSH-pass, and both-fail. The focused GNU tty
run moved the merged XSH records to 306 passed, 115 skipped, 273 failed, and
25 harness errors.

The branch is still in Wave 1. Do not treat the setup being complete or the
106-utility ownership table being covered as campaign completion: remaining
utility behavior, Wave 2/3 families, GNU differential blockers, and final
smoke/performance gates are listed in [`CAMPAIGN.md`](CAMPAIGN.md) and
[`LANES.md`](LANES.md).

## Excluded scratch work

The detached `/tmp/bytes-enc-followup.IOzYGs` worktree has uncommitted `dd` and
`od` edits. Its focused run scored 142/232 across those two utilities and
regressed 13 previously passing test IDs, so those edits are not part of the
integration branch or scoreboard.
