# Factor and sort timeout evidence

The canonical native gate on source commit `0dc9cff7c2b16e4c201adb3abca11d7ebda8e1f4` timed out in two existing tests. The frozen run reported a two-minute timeout for `test_uu_factor_parallel` and a four-minute timeout for `test_uu_sort_buffer_sizes`. The exact records and final failure summary are in [the canonical excerpt](canonical-failure-excerpt.log).

The canonical container had a 12-CPU quota. Its resource receipt records 5,316 throttled periods out of 36,985, 2,111,336,856 throttled microseconds, a 5,092,859,904-byte memory peak, and no OOM events. [The raw receipt](canonical-container-resources.json) contains the full cgroup counters.

## Exact selectors

Both selectors pass alone with the frozen 0dc9 verification products. The runner selected one test for each command. `test_uu_factor_parallel` passed with exit status 0 under a 150-second external cutoff. `test_uu_sort_buffer_sizes` passed with exit status 0 under a 270-second external cutoff. The raw output and elapsed time are in the [factor logs](factor.stdout.log), [factor stderr](factor.stderr.log), [sort logs](sort.stdout.log), and [sort stderr](sort.stderr.log). The [factor status](factor.exit-status.txt) and [sort status](sort.exit-status.txt) files record each runner exit code.

The runs used the pinned Linux image, UID 1000, a 12-CPU limit, an 8-GiB memory limit, and no network. [The run configuration](run-configuration.txt) records the selectors and cutoffs. [The source pin](source-pin.txt) and [product hashes before](products-pre.sha256) and [after](products-post.sha256) show the clean source and unchanged frozen products.

These exact selections do not reproduce cross-test scheduling. XSHT runs one worker when a command selects one case. The canonical logs record completion order, not test start times, so they do not establish which expensive cases overlapped. Nested CPU work in the factor file and concurrent sort tests are consistent with resource contention, but the overlap remains unproven.

The four factor and sort source and test files have identical bytes at `7b1485310abdf2190ce4532d840114e74ff7c2cd` and `0dc9cff7c2b16e4c201adb3abca11d7ebda8e1f4`. This evidence does not support an applet algorithm change. The remaining check belongs to the canonical runner: reduce the number of concurrent tests, then verify the affected workload without changing test timeouts or assertions.
