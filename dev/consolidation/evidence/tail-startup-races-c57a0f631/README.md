# Tail fixture startup races

The C profile exposed two test-fixture races. Neither failure came from `core/tail.xsh`.

## Reproductions

`test_gnu_tail_debug_log` started the XSH `timeout` wrapper before `strace`. A delayed `strace` startup let the 100 ms timeout expire before `strace` opened `strace.out`. The test then failed while reading that missing file. The controlled red run inserted a 200 ms delay before `exec strace`. Its failure names the same missing path. See [the raw test output](controlled-red/gnu-debug.stdout), [the injected source](controlled-red/controlled-red-source.diff), and [the runner](controlled-red/run-exact.sh).

`test_tail_follow_name_switches_banners_between_files` started its writer before tail. The writer appended after 100 ms and 200 ms. A controlled 300 ms delay before tail startup made both appends part of the initial read. The resulting output matches the profile failure: `log appended` appears under the first `log` banner, and no later `log` banner appears. See [the raw test output](controlled-red/follow-banners.stdout) and [the injected source](controlled-red/controlled-red-source.diff).

## Changes

`traced_timeout` now starts real `strace -f` around the existing XSH `timeout` launcher and tail launcher. `strace` opens `strace.out` before the XSH timeout starts its unchanged 100 ms limit. The syscall filter, status 124 assertion, and conditional notification-mode tests remain. The inotify-only companion test uses the same helper because it had the same startup ordering.

The banner test now owns a live tail `ProcessHandle` with a 2 second process safety limit. It waits for the exact initial output before appending to `other`. It waits for that output before appending to `log`, then waits for the full transcript. It cancels the follower after the final output arrives. The exact transcript and empty-stderr assertions remain.

The old status-124 assertion coupled transcript ordering to a fixed 500 ms timeout that started before tail was ready. The test now checks the banner behavior directly. `test_gnu_tail_wait_log` continues to check timeout status 124.

## Verification

The controlled red run failed both selectors with status 1. The first fixed-wrapper trial used the host `timeout` executable instead of the XSH launcher. Both GNU selectors then reported status 3 instead of 124, while the banner selector passed. That trial receives no fix credit. See its [raw output](external-timeout-trial/gnu-debug.stdout) and [source](external-timeout-trial/fixed-source.diff).

The final focused run passed all three selectors with no skips:

- `core/tests/test-gnu-tail.xsh::test_gnu_tail_debug_log`: 1 pass, 0 failures, 0 skips, 357 ms.
- `core/tests/test-gnu-tail.xsh::test_gnu_tail_inotify_only_regular_log`: 1 pass, 0 failures, 0 skips, 142 ms.
- `core/tests/test-tail.xsh::test_tail_follow_name_switches_banners_between_files`: 1 pass, 0 failures, 0 skips, 71 ms.

The tests used the protected C products and pinned Linux image. [The source and product hashes](final-green/source-products.sha256) record the tested files and binaries. [The checkout record](final-green/checkout.txt) pins base C. [The runner scripts](final-green/run-lease.sh) record the container flags, exact selectors, and lock acquisition. Native lock inode 62012470 matched before and after each test lease. Docker reported no such object for each `--rm` container after the run.

The first runner preflight exited 126 because `run-container.sh` lacked execute permission. It started no container and ran no test. The recorded preflight is in [runner-preflight](runner-preflight/description.txt). Its [container runner](runner-preflight/run-container.sh) preserves the missing execute bit.

This lane ran no full native suite, build, or Cargo check.
