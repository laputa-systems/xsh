# Lane claims (campaign-utils)

The current run has one integrator. All spawned lane agents use GPT-6 Luna at
xhigh effort. The old parallel integrator's reservations are released: at
resume, the only lane worktrees were `date` and `fs-misc`, and no `lane/*`
branches were present on `origin`.

Before starting a lane, fetch `origin/campaign-utils`, check this table, then
commit and push the claim. Do not start a claimed lane. A lane already merged
on `campaign-utils` is done.

| Lane | State |
|---|---|
| text-b2, text-b1 | merged in an earlier campaign run |
| ls, native-proc-tty, tty-misc | merged in an earlier campaign run |
| proc-a (`kill`, `nice`) | merged; `nohup`, `timeout`, `stdbuf` free |
| cp | released; no lane commit exists |
| mv-ln | released; no lane commit exists |
| date | active on `lane/date`, claimed 2026-10-09 (GPT-6 Luna, xhigh) |
| fs-misc | active on `lane/fs-misc`, claimed 2026-10-09 (GPT-6 Luna, xhigh) |

All other lanes are free. Result JSON and baselines under
`dev/compat/results/` are integrator-owned and must be regenerated after a
merged lane changes behavior.
