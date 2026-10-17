# Host obligation native six-case qualification

All six exact native cases pass on the protected source and product pair.
The integrator admitted this run after the main residual work and release of
the corpus 26-case slot. The corpus run was red and has no acceptance credit
here. Broader inference and campaign acceptance remain separate.

The runner adapts the existing `typing-successor-d6a70a237/run-native-53.sh`
qualification style. It selects only these six cases from
`tests/xsh/typing-capability-witnesses.xsh`:

- `test_typing_host_result_anchor_and_nested_loops`
- `test_typing_host_constraints_follow_chains_and_body_order`
- `test_typing_host_replacement_result_keeps_receiver_ambiguity`
- `test_typing_host_byte_receiver_keeps_ambiguity_after_bytes_caller`
- `test_typing_host_inference_preserves_argument_and_effect_errors`
- `test_typing_host_untemplated_receiver_domains_remain_candidates`

The protected source is `.work/wt/typing-host-obligations`, pinned to
`5277f62b7b8b81f9805cb0d8e33172db5c1d8c9f`, tree
`c5ffd8a209d8e6b59c7012c571d64ed5436f5290`. Its compiler code is
`a972c8d139ecbb43174271842988eefb2c8638cc`, tree
`6b329848a07a2e685c8f1a89797a3fa490ac6bbc`. The later source commit adds
documentation and retained proof. Preparation confirms that all six named
tests exist and that the protected checkout is clean.

The read-only product directory is
`.work/wt/typing-host-obligations/.work/target-typing-host-obligations/x86_64-unknown-linux-musl/verification`.
Preparation verifies both assigned hashes:

```text
xsh  ee306b82df028f4b2d0bc83699bf41de27c5c7ac56862de10135461e6bbca154
xsht c7c12a7af5e26edee94ae12bba7196a9a3bffd6bbe90129d29a5a923b73e831c
```

The admitted outer command is:

```sh
bash /home/josh/d/laputa-systems/xsh/.work/wt/typing-host-native6/dev/consolidation/evidence/typing-host-native6/run-native6.sh
```

Each inner invocation is `/target/xsht test --exact SELECTOR -j 1`.
The existing 120-second per-test timeout stays at its default. Each command
must exit zero, select one test, pass that exact test, and report one pass,
zero failures, and zero skips. Missing or zero-count output fails admission.

The runner acquires the original
`.work/consolidation/infra/native-suite.lock` with `flock -n -x` before Docker.
It rejects the existing unresolved-cleanup marker. A busy lock exits with
status 75 without starting a container; there is no waiter or retry loop.
The attempt requires a fresh owned scratch directory, so a rejected or failed
attempt cannot be silently overwritten.

The container uses pinned image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`,
UID/GID 1000, CPUs 12–15 with a four-CPU cap, 8 GiB with no extra swap,
and no network. Source, products, and Git metadata are read-only. Writable
temporary directories are under the proof owner's `.work/` and mounted over
the container's scratch and source temporary paths.

Host and container guards check source identity, cleanliness, and both
product hashes before and after runtime. Exit traps retain those checks on
test or cleanup failure. The host owns one uniquely named container and its
CID file, removes only that recorded container when necessary, and records
exact CID and name absence before releasing its lock. Cleanup or guard
failure cannot turn a failed run into a pass.

Both scripts pass `bash -n`. Only the six named exact native selectors run;
there is no build, full native suite, alternate execution mode, release, or
performance measurement.

## Results and retained failure

The first admitted invocation stops during host preflight, before acquiring
the native lock or starting Docker. This host uses BusyBox `sha256sum`, which
rejects `--check`. The [raw preflight error](preflight-busybox/products-before.stderr),
source and product readings, and original scripts are retained under
[`preflight-busybox`](preflight-busybox/). That invocation exits one and runs
zero tests. The portability repair changes only `--check` to `-c` in both
guards; `-c` works with GNU and BusyBox implementations. No runtime failure
is retried or discarded.

The corrected invocation uses a fresh scratch directory under the same named
admission. The [summary](run-5277/summary.tsv) records six command exits of zero.
Each adjacent native stdout file reports `running 1 tests`, the exact selected
test as `ok`, and `test result: ok. 1 passed; 0 failed; 0 skipped`.
There are six passes, zero failures, and zero skips in total.

The [outer command](run-5277/docker.command) records the actual container
arguments. [Runner](run-5277/runner.status), [Docker](run-5277/docker.status),
[container cleanup](run-5277/cleanup.status), and
[final guard](run-5277/final-guards.status) statuses are all zero.
The [host source before](run-5277/source-before.txt) and
[after](run-5277/source-after.txt) files match, as do
[product hashes before](run-5277/products-before.sha256) and
[after](run-5277/products-after.sha256). The independent
[inside-before](run-5277/inside-before.txt) and
[inside-after](run-5277/inside-after.txt) checks also pass.

The [recorded CID](run-5277/container.cid) is absent from the successful
[post-run ID query](run-5277/container-ids-after.txt), and the
[exact unique-name query](run-5277/container-name-after.txt) is empty.
The owning runner exited and released its lock; no waiter remains.
Git printed its existing untracked-cache warning during the read-only checks;
it did not change the source or turn any guard into a failure.

Only this evidence directory changes. The protected `typing-host-obligations`
checkout, its original products, main, and frozen b235 remain untouched.
