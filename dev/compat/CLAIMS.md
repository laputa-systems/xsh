# Lane claims (campaign-utils)

This run has one integrator. Every lane agent uses GPT-6 Luna at xhigh effort.
The campaign runs on native Linux x86_64, with no Docker, the workspace-pinned
`nightly-2026-09-15`, and mold 3.0.0 from its verified GitHub release. See
[`README.md`](README.md#setup) and [`native-env.sh`](native-env.sh).

The local integration head is `8a880adf`; the post-`cp`/`mv-ln` full uutils
regression run is in progress. Lane result files and the shared baseline remain
integrator-owned. Before starting or resuming a lane, check this table, fetch
`origin/campaign-utils`, and render its current brief with `lanes.py brief`.

| Lane | State |
|---|---|
| text-b1, text-b2 | merged in an earlier campaign run |
| ls, native-proc-tty, tty-misc | merged in an earlier campaign run |
| proc-a (`kill`, `nice`) | merged; `nohup`, `timeout`, `stdbuf` remain free |
| date | merged as `243c5bd9`; 183/185, two documented locale-data differences |
| fs-misc | merged into `1d7eff31`; 295/304, nine GNU-verified diagnostic gaps |
| cp | merged locally as `80231c6a`; 233/386, full regression comparison pending |
| mv-ln | merged locally as `8a880adf`; 246/343, full regression comparison pending |
| fs-basic | active in `/workspace/xsh-lanes/fs-basic`; prompt fix and final slice pending |
| perm | claimed for the next lane start |
| stat-du-df | claimed for the next lane start |

All other Wave 1 lanes are free unless they are already listed as merged in
[`LANES.md`](LANES.md). A lane already merged on `campaign-utils` is done; the
integrator records its final counts and removes its worktree after the merge
gate passes.
