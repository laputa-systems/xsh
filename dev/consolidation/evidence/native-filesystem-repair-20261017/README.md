# Native filesystem failure evidence

This evidence covers source commit `0dc9cff7c2b16e4c201adb3abca11d7ebda8e1f4` and its frozen x86_64 musl verification products. It preserves the original full-run failures, qualified single-selector reruns, and the bounded descriptor probe. No production or test source changed.

## Exact reruns

The canonical full run reported `test_gnu_mv_leak_fd_log` timed out after two minutes and `test_gnu_rm_ext3_perf_log` failed after 1m14s with status 3 and empty stderr. The [canonical failure excerpt](canonical-failure-excerpt.txt) and [full replay receipt](rm-canonical-replay.json) preserve those results.

I reran each selector alone with the pinned image, 12 CPUs, 8 GiB memory, UID/GID 1000, no network, and the frozen products. Both commands checked the source commit/tree and source/product hashes before and after execution. `mv` passed with one selected test in 1m10s; `rm` passed with one selected test in 43.6s. See the [mv command](mv-exact-frozen.command), [mv output](mv-exact-frozen.stdout.log), [mv status](mv-exact-frozen.status), and [mv pins before](mv-exact-frozen.before.txt) and [after](mv-exact-frozen.after.txt); and the corresponding [rm command](rm-exact-frozen.command), [output](rm-exact-frozen.stdout.log), [status](rm-exact-frozen.status), and [pins before](rm-exact-frozen.before.txt) and [after](rm-exact-frozen.after.txt).

These isolated passes do not explain the earlier full-run failures. There is no per-test CPU or memory attribution, and no measured evidence separating host load, resource admission, or a command defect. The rm test creates 400,000 files, with an inner timeout of max(60 seconds, setup duration) and a 120-second outer timeout. A prior pass of the same-byte rm selector at `be0b79a48403d74b11186166ba739a81dcbed702` in 38.144s is [historical evidence](../rm-sort-isolated-be0b/README.md), not current qualification. The exact rerun durations above are outcome records, not performance measurements.

An earlier isolated mv pass used a different main-root cache instead of the frozen products and is unqualified. Its command, output, status, and post-run hashes are preserved in the [unqualified receipt](mv-exact-unqualified.command), [output](mv-exact.stdout.log), [status](mv-exact-unqualified.status), and [pin record](source-and-product-pins.txt). It receives no pass credit.

## Descriptor probe

The bounded probe made 96 one-file directories on the lane filesystem and moved them to `/dev/shm` with `RLIMIT_NOFILE=64`. XSH exited 0. The trace has no `EMFILE`; a representative `O_PATH` root open returned descriptor 6 and a later `close(6)` succeeded before the next directory transaction. The [probe script](mv-low-fd-probe.sh), [command](mv-low-fd.command), [status](mv-low-fd.status), and [full trace](mv-low-fd.trace) preserve the run. The trace includes only `openat`, `openat2`, and `close`, so it is not a full descriptor census and does not establish an `EMFILE` boundary.

The observed close refutes the proposed unclosed `FsRoot` hypothesis for this probed path. No explicit close, production fix, or replacement test was added. A prior Docker-stats attribution to mv was withdrawn because those figures came from a concurrent Cargo build container; no per-test CPU or memory measurement exists.

The source and frozen product pins are summarized in [source-and-product-pins.txt](source-and-product-pins.txt). This lane does not claim the full native gate passes or identify the cause of its two recorded failures.
