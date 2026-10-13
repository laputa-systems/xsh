# Standard-library port measurements

`run.py` compares a reference and candidate `xsh` binary on the same frozen
workload scripts. It checks exit status, output, and persistent filesystem
effects before measuring; each workload interleaves fresh reference and
candidate processes. The JSON report retains raw samples, binary and fixture
hashes, parity results, and the budget decision. Historical reports are kept
beside the runner; their measurements apply only to their recorded binaries,
fixtures, and host.

The gate allows a candidate-minus-reference increase of at most
`max(5% of reference, 1 ms)` for cold-start workloads and
`max(10% of reference, 2 ms)` for end-to-end workloads. The runner exits
nonzero when a workload fails its budget or parity check.

## Host run

Build release binaries from equivalent revisions. Keep the reference worktree
and its separate build output under `.work/`. Before creating `.work/`, make
sure it is ignored through `.git/info/exclude`:

```sh
mkdir -p .work/wt .work/tmp .work/target
git worktree add .work/wt/stdlib-reference BASELINE
CARGO_TARGET_DIR="$PWD/.work/target/stdlib-reference" \
  cargo build --manifest-path .work/wt/stdlib-reference/Cargo.toml \
  --release -p xsh --bin xsh
cargo build --release -p xsh --bin xsh
python3 -B bench/stdlib-port/run.py \
  --reference "$PWD/.work/target/stdlib-reference/release/xsh" \
  --candidate "$PWD/target/release/xsh" \
  --rounds 3 --out .work/tmp/stdlib-port-host.json
```

The command assumes the repository root as its working directory. The runner
executes workloads from `bench/stdlib-port`, so relative inputs resolve there.
Binary paths must be absolute.
Three rounds are the usual comparison; `json_lines_batch` takes ten samples per
round because each process is long-running. Other sample counts are fixed by
`WORKLOADS` in `run.py`. Results report medians of round medians. The
`failed`, `parity_failed`, and `skipped` fields distinguish budget failures,
behavior mismatches, and workloads not run.

## Linux run

The runner's `--linux` route is pinned to `linux/arm64` and
`aarch64-unknown-linux-musl`. Build both matched release binaries inside the
`Dockerfile.test` environment using the target settings in
`dev/targets.xsh::docker_test_env`, then pass their absolute paths:

```sh
python3 -B bench/stdlib-port/run.py \
  --reference /absolute/path/to/reference/release/xsh \
  --candidate /absolute/path/to/candidate/release/xsh \
  --rounds 3 --linux --out .work/tmp/stdlib-port-linux.json
```

`XSH_TEST_IMAGE` selects the Docker image; it defaults to `xsh-test`. Linux
workloads read committed fixtures mounted at fixed `/proc` paths in the
container's private mount namespace. `linux-runner.xsh` measures each child,
excluding container startup and Python aggregation. This ARM64 route is
separate from the repository's current x86_64 Linux verification target.

## Fixed-path `system.os_release` test

The Linux integration test
`stdlib_port::os_release_entry_reads_the_fixed_paths` exercises the public
`system.os_release` API at `/etc/os-release` and `/usr/lib/os-release`. It is
skipped unless `XSH_OS_RELEASE_SCENARIO` is set. The fixtures are staged as
symlink targets in the container's writable layer; no host release file or
`/proc` or `/sys` path is changed. Run the three scenarios from the repository
root:

```sh
mkdir -p .work/target .work/tmp
run() {
  scenario=$1
  setup=$2
  docker run --rm --privileged --platform linux/arm64 \
    -v "$PWD:/work:ro" -v "$PWD/.work/target:/work/.work/target" \
    -v "$PWD/.work/tmp:/work/.work/tmp" \
    -v xsh-cargo-registry:/root/.cargo/registry -w /work \
    -e TARGET=aarch64-unknown-linux-musl \
    -e CARGO_TARGET_DIR=/work/.work/target \
    -e TMPDIR=/work/.work/tmp \
    -e XSH_OS_RELEASE_SCENARIO="$scenario" xsh-test \
    sh -c "$setup; cargo test --profile verification \
      --target aarch64-unknown-linux-musl --features linux-priv-tests \
      --test integration stdlib_port::os_release_entry_reads_the_fixed_paths \
      -- --exact"
}

FIX=/work/tests/fixtures/stdlib/os_release/fixed-path
run etc "rm -f /etc/os-release; ln -s $FIX/etc-os-release.txt /etc/os-release"
run fallback "rm -f /etc/os-release; ln -s $FIX/no-such-file.txt /etc/os-release; \
  rm -f /usr/lib/os-release; ln -s $FIX/usr-lib-os-release.txt /usr/lib/os-release"
run neither "rm -f /etc/os-release; ln -s $FIX/no-such-file.txt /etc/os-release; \
  rm -f /usr/lib/os-release; ln -s $FIX/not-utf8.txt /usr/lib/os-release"
```

The scenarios check the first path, fallback to the second path, and reporting
the second path's invalid-UTF-8 error. Use a private `.work/` directory for
build output; do not commit capture or measurement output unless it is intended
as a reviewable result.
