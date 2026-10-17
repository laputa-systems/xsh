# Canonical native admission on 0dc9

The canonical container has a 12-CPU quota and exposes the host's 32-CPU
affinity mask. A standalone Rust probe, built with the pinned toolchain in the
pinned test image, reports the actual `available_parallelism()` seen by the
unprivileged native test identity. The exact source, command, and stdout are
retained beside this note.

The probe reports 12. In the frozen XSHT source, `test_jobs` uses that value
for the default outer case pool, while the queue can admit 8 cases from one
file. The factor file contains four long cases that each run one factor child
and `test_uu_factor_parallel`, which starts eight factor children. Those five
cases can therefore account for twelve CPU-bound children, while other files
may occupy the remaining outer case slots. This is an admission possibility;
the canonical log records completion order, not case start times, so it does
not establish that this overlap caused any particular deadline failure.

The canonical batch now limits only its outer native case pool to four. It
keeps the full native selection, existing timeouts, each test's own child
process behavior, all Rust and privileged plans, and the separate 32-versus-8
scheduler comparison. The earlier full run recorded 5,316 throttled periods
of 36,985, aggregate `throttled_usec=2,111,336,856`, a pids peak of 218, and
memory peak 5,092,859,904 bytes of 24 GiB with no OOM. These aggregate counters
do not identify which test was throttled. No timing or individual
failure-causation claim is made.
