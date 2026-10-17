# Lifecycle cleanup profile reproduction

The R3 canonical native run failed
`dev/tests/test-lifecycle.xsh::test_docker_container_failure_runs_target_ownership_cleanup`
because the test expected a verification Cargo command while the canonical
container selected release. The test fixture expected `DIST_PROFILE` to be
verification but did not set it in its child environment. Commit
`f4a9fb92f0976ed8fa6680f4ed8597180cc93f32`, based on
`7eb62cd993e578c13bf6befd68a6a6ac15133c78`, adds only the explicit child
environment value `DIST_PROFILE=verification`.

The RED run used source `7eb62cd993e578c13bf6befd68a6a6ac15133c78` and its
test file. It selected one test and exited 1. The test observed the release
Cargo command, `StageError.Failed`, and both the fake-Cargo and cleanup marker
files. The copied marker paths are in
[`red-cargo-markers.txt`](red-cargo-markers.txt) and
[`red-cleanup-markers.txt`](red-cleanup-markers.txt). Raw output is in
[`red-release`](red-release/).

The two GREEN runs used the same source plus the one-line test change. Each
selected one test and passed with zero failures or skips, once with outer
`DIST_PROFILE=release` and once with `DIST_PROFILE=verification`. The passing
test also verifies that both marker files exist. Its child environment pins
`DIST_PROFILE=verification` in both runs. Raw output is in
[`green-release`](green-release/) and
[`green-verification`](green-verification/).

The exact in-container command was
`setpriv --reuid=1000 --regid=1000 --clear-groups xsht test --jobs 1 --exact --keep-temp dev/tests/test-lifecycle.xsh::test_docker_container_failure_runs_target_ownership_cleanup`.
The pinned image was
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
The container had four CPUs, 8 GiB of memory, and no swap. It started as root
and ran the test as UID/GID 1000. The source and products were read-only; the
test scratch directory was writable. The relevant inherited R3 environment
was `CARGO_BUILD_JOBS=12`, `TARGET=x86_64-unknown-linux-musl`,
`CARGO_TARGET_DIR=/work/target/x86_64-unknown-linux-musl`, `HOST_UID=501`, and
`HOST_GID=20`, with `DIST_PROFILE` set to the outer profile named above.

The reused release products were `xsh` at SHA-256
`1d7690114501313ffc4706b7f78ab6154ecd668f79d3735700219a00501dbe33` and
`xsht` at SHA-256
`0cd37140bd43ca0ca5104d43d11bc1732084fc2342ee3bb69a73fda38452f1c4`.
No build ran. The runs held the shared native-suite lock with device number
and inode `66307:62012470`; the lock was free afterward. Run-owned containers
were absent after cleanup.

An initial Docker launch exited 125 before native execution because its target
mount point was missing. That attempt's stderr was overwritten during setup,
so this archive contains no raw output for it. It is recorded as a startup
failure only, not as a test result.
