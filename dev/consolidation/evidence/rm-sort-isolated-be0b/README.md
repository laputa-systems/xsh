# Isolated GNU rm and sort checks on be0b

## Outcomes

I ran each exact native selector once on source commit `be0b79a48403d74b11186166ba739a81dcbed702` with its pinned verification products.

| Selector | Result | Duration | Test budget | Readiness | Timing status |
|---|---|---:|---|---|---|
| `core/tests/test-gnu-rm.xsh::test_gnu_rm_ext3_perf_log` | 1 passed, 0 failed, 0 skipped | 38.144 s | Inner timeout is at least 60 s. Outer test timeout is 120 s. | 0 before, 0 after | Correctness only. Cargo build overlapped. |
| `core/tests/test-uu-sort-1.xsh::test_uu_sort_buffer_sizes` | 1 passed, 0 failed, 0 skipped | 139.973 s | 240 s for the whole test | 0 before, 0 after | Single readiness-bracketed observation. |

The runs used separate containers. The rm test created and removed 400,000 files. The sort test checked all nine buffer-size forms and expected output. Both receipts report zero CPU throttling, OOM, and swap. Sampled process RSS peaks were 317,440,000 bytes for rm and 153,538,560 bytes for sort. These are single-run observations, not a performance or memory comparison.

The rm pass is valid correctness evidence. Its time is not qualified because the pre-run snapshot shows an active Cargo test build for the semantic route matrix AST extractor. `perf-gate-ready` returned 0 before and after, but did not exclude that observed overlap. The sort pre-run snapshot shows no Cargo build. Raw snapshots and readiness codes are in each case directory.

## Earlier full-suite failures

The canonical run at commit `7b1485310abdf2190ce4532d840114e74ff7c2cd` reported rm status 3 with empty stderr after 1 minute 13 seconds. It timed out sort at 4 minutes. The [canonical receipt](../canonical-7b148/README.md) and [resource receipt](../canonical-7b148/container-resources.json) preserve those results.

The two test files and `crates/xsht/src/xsht/test.rs` have identical bytes at `7b148` and `be0b`. The focused runs therefore use the same test bodies and scheduler code as the earlier run.

The rm test creates its fixture, then runs rm under external BusyBox `timeout`. The inner timeout is 60 seconds unless fixture setup takes longer. Its outer `shell_run` timeout is 120 seconds. A pinned-image probe timed out a long-running XSH child with status 3 and empty streams. The same probe returned 143 for an external timeout around `/bin/sleep`, while `core/timeout.xsh` returned 124 on expiry. This supports the inner timeout as the status-3 mechanism. The 7b run did not record the signal boundary, so the probe cannot prove that this run expired the timer. The [probe script](timeout-status-probe/probe-cancel.sh), inputs, and [status summary](timeout-status-probe/results.txt) preserve the observations.

The sort test gives its nine buffer-size cases a shared 240-second budget. Its isolated pass took 139.973 seconds. The full-suite timeout shows the budget can be exceeded there, but does not identify why.

## Scheduler evidence

Commit `aa5aca3acbd2dafd03bf3b5a4fb2cb63f399f896` changed the default from half the visible CPUs, capped at eight, to all visible CPUs, capped at 32. The source uses `available_parallelism`. Commit `3a6c3daacf226f8f2c6049dbadf200dd38756c86` added fair admission across files. Commit `7205a0b6afce7d9a0072b6727cae36a0b8301963` capped active tests at eight per file.

The canonical container had a 12-CPU quota. If `available_parallelism` reported that quota, the current default allowed 12 workers there, compared with six under the former rule. The per-file cap does not cap total workers at eight. These tests are in separate files. Exact one-case selections run serially, so these focused runs did not exercise the worker policy.

The 7b whole-container receipt reports 5,072 throttled periods and 2,380,437,750 microseconds throttled over a 59-minute batch. It spans build and test stages, so it does not attribute throttling to either test. The isolated containers used the same quota and recorded no throttling.

The worker increase can raise suite concurrency. Both isolated cases passed on be0b under the recorded conditions, so neither has an unconditional failure there. This evidence does not show whether the scheduler change caused the full-suite overruns or whether other suite load or timing variation caused them. No worker policy, timeout, assertion, or full suite changed in this lane.

## Run records

The host held `.work/consolidation/infra/native-suite.lock`. The pinned image was `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`. Each container used 12 CPUs, 24 GiB memory, no swap, no network, and Docker user `1000:1000`. Source and product mounts were read-only. `TMPDIR` used host-backed ext4 scratch. `LD_PRELOAD` was unset.

[`source-identity.txt`](source-identity.txt) and [`source-after.txt`](source-after.txt) record the same clean source commit and tree before and after. [`product-identity.txt`](product-identity.txt) records the xsh and xsht hashes. The [rm output](rm/stdout.log), [rm resource receipt](rm/container-resources.json), [rm readiness](rm/readiness.txt), and [rm cleanup check](rm/cleanup.txt) preserve its run. The [sort output](sort/stdout.log), [sort resource receipt](sort/container-resources.json), [sort readiness](sort/readiness.txt), and [sort cleanup check](sort/cleanup.txt) preserve the other run. Docker and selector commands and host process snapshots sit beside those files. Both containers were absent after the runs.

Two launcher attempts did not start XSHT. The [first launcher error](launcher-attempts/timeout-options.stderr.log) records BusyBox rejecting GNU-style timeout options. The [second spawn receipt](launcher-attempts/missing-setpriv-rm-container-resources.json) records missing `/usr/bin/setpriv` in the pinned image. Neither attempt is a test result. The successful launch used Docker `--user=1000:1000`, matching the canonical runner's user mode.
