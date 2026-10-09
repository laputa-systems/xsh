# Lane claims (campaign-utils)

This run has one integrator. Every lane agent uses GPT-6 Luna at xhigh effort.
The campaign runs on native Linux x86_64, with no Docker, the workspace-pinned
`nightly-2026-09-15`, and mold 3.0.0 from its verified GitHub release. See
[`README.md`](README.md#setup) and [`native-env.sh`](native-env.sh).

The local integration head is `aa8a0183`; origin is still at `5de29706` while
the regression repairs are being validated. A full uutils run after cp/mv-ln
integration completed at 2,813/5,974 but exposed 187 regressions. The shared
`fs.stat`/`Path.read_bytes` issues and the cp/mv-ln utility regressions are
fixed locally; a new full run is pending. Lane result files and shared
baselines remain integrator-owned. Before starting or resuming a lane, check
this table, fetch `origin/campaign-utils`, and render its brief with
`lanes.py brief`.

| Lane | State |
|---|---|
| text-b1, text-b2 | merged in an earlier campaign run |
| ls, native-proc-tty, tty-misc | merged in an earlier campaign run |
| proc-a (`kill`, `nice`) | merged; `nohup`, `timeout`, `stdbuf` remain free |
| date | merged as `243c5bd9`; 183/185, two documented locale-data differences |
| fs-misc | merged into `1d7eff31`; 295/304, nine GNU-verified diagnostic gaps |
| cp | merged locally; regression fix at `f06c924b`, focused slice 238/386 |
| mv-ln | merged locally; same-file and prompt fixes at `e4efc7ed`, focused mv/ln slice 126/207 |
| fs-basic | merged at `aa8a0183`; 188/203, rm `/dev/full` reporting fixed; remaining gaps are being added |
| perm | active in `/workspace/xsh-lanes/perm` |
| stat-du-df | active in `/workspace/xsh-lanes/stat-du-df` |
| printf-env | active in `/workspace/xsh-lanes/printf-env`; based on `e4efc7ed` |

All other Wave 1 lanes are free unless they are already listed as merged in
[`LANES.md`](LANES.md). A lane already merged on `campaign-utils` is done; the
integrator records its final counts and removes its worktree after the merge
gate passes.
