# Integrated factor candidate qualification

This run qualified integrated candidate `3160bda38f5fb4cdb05e6b279efc57b296d4fcf8`, tree `7bb5fb42a1dd437cf8be1e8d1d49137425d2a0b7`. The candidate worktree was mounted read-only. Source, test, product, candidate tree, and shared-lock guards matched before and after every selector.

The exact parallel factor selector timed out at the existing two-minute test limit. The mixed non-Unicode factor selector passed, and the lifecycle cleanup selector passed. Raw stdout, stderr, statuses, and container state are beside this note. The candidate is not qualified while the parallel selector fails.

The retained partial output is 4,075,380 bytes and 250,005 lines: five identical complete child outputs of 815,076 bytes and 50,001 lines each. All eight child stderr files were empty. The case timed out while waiting for its factor children and had not reached the subsequent sort and hash assertion. `factor-parallel.partial-output.gz` decompresses byte-for-byte to the retained output.

The parallel container's cgroup v2 `memory.peak` was 260,632,576 bytes. `memory.events` reported `oom 0`, `oom_kill 0`, and `oom_group_kill 0`. Docker reported `OOMKilled=false`. This rules out observed cgroup memory exhaustion for this run.

`overlapping-host-work.txt` records two pinned Cargo containers running during the factor interval. A four-CPU build container started at 08:20:54Z, 51 seconds before the factor test's timeout; another four-CPU Cargo container started about three seconds before the timeout. A host load sample at 08:22:11Z reported 20.79 on 32 CPUs. This shows overlapping CPU work; it does not prove that contention caused the timeout. A qualification rerun after those builds drain is needed before deciding whether another factor algorithm change is required.

The run used the frozen R3 release products and pinned image with Linux x86_64 musl, cpuset 0–11, 12 CPU quota, 24 GiB memory and no swap, no network, UID/GID 1000, `DIST_PROFILE=release`, `-j 1`, `--keep-temp`, and a 180-second outer cutoff. The original shared lock file (device 66307, inode 62012470) was held exclusively and its inode and metadata remained unchanged.
