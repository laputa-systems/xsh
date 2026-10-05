# Lane claims

Refreshed 2026-10-05 at `37df79b6` after the compatibility branch was merged
into `master`. The local checkout has no lane worktrees or `lane/*` branches.
No active local ownership was verified; remote worker state was not checked.
The old session claims are retired and do not reserve work.

| Lane | Owner | Base revision | State |
|---|---|---|---|
| — | — | — | No verified active local claims |

Before starting implementation, check with any current collaborators, inspect
existing worktrees and branches, then record the lane, owner, exact base
revision and exclusive file set here. Use `master` as the integration base.
Follow the current session's authorization for commits and publishing; a
claim never requires an automatic push. Clear a claim when it lands or is
released, and retain the result and remaining gaps in the handoff/report.

`lanes.json` describes ownership boundaries and dependencies, not completion.
The old claims for `cp`, `text-a2`, `sort`, `fs-basic`, `printf-env`,
`native-bytes-hash`, `bytes-enc`, `checksums`, `perm`, and `stat-du-df` were
session plans, not evidence of completion. Check current code, reports and
`gaps.json` before assigning them. The earlier `proc-a` report covered only
`kill` and `nice`; verify `nohup`, `timeout`, and `stdbuf` separately.

Full-suite reports and the parity manifest remain integrator-owned. Preserve
the committed baseline before a new run; resolve result conflicts by rerunning
the suite, not by combining incompatible reports. See
[`CAMPAIGN.md`](CAMPAIGN.md) for the audited state and next steps.
