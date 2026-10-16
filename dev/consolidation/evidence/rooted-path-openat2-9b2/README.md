# Linux openat2 EAGAIN in PCI capture

The exact PCI selector fails with the 7b `xsht` product while a separate directory tree is renamed under `/scratch`. Its trace records `openat2` returning EAGAIN for the PCI BDF path. The be0b selector passes under the same rename worker. Its trace also records EAGAIN.

The [retry helper](../../../../crates/xsh-root/src/linux.rs#L94) retries the same `openat2` operation for raw EAGAIN, up to eight attempts. It returns the eighth EAGAIN unchanged. This run does not show success under a rename storm that exhausts all eight attempts.

## Source and products

Both product sets ran against the be0b source tree mounted read-only at `/src`. The selected test and its two directly imported modules match their 7b versions byte for byte.

| Product source | Commit | Tree | `xsh` SHA-256 | `xsht` SHA-256 | Product directory |
| --- | --- | --- | --- | --- | --- |
| 7b | `7b1485310abdf2190ce4532d840114e74ff7c2cd` | `0f3394902504cbe5271edd8916f504bd493f091a` | `7e66422d21b480da24308aac719df7ff1fbf806f6e285e26739a544a46f1fd0a` | `e9671817dce3efb6d5bcfc74ed343be9b7ac3f8403888d6b03f5ef91315d7951` | `.work/target-final-canonical-previous-run-final-be0b-products/x86_64-unknown-linux-musl/verification` |
| be0b | `be0b79a48403d74b11186166ba739a81dcbed702` | `cc80212504507d34c7d07413ce33f42ea68b6028` | `1a4b3857c6bd0b989cf0f159a6745ddf6a846b6d6467a282f96e8737d1196a7b` | `8e9c3f5aa24e5933daf74294ec54772623ab819594244a02646a05c8fbd4429f` | `.work/target-final-canonical/x86_64-unknown-linux-musl/verification` |

The selector was `dev/tests/test-system-report-check.xsh::test_system_report_pci_capture_replays_raw_identity_links_and_rejects_tampering`. Its source SHA-256 is `d1408244a204563f57b357ef3515f805db152f88eab7d7a15939b714f5d72834`. The imported files `dev/system_report_check.xsh` and `dev/system_report_smbios_check.xsh` have SHA-256 values `f39d42e3caea6186d74fc1e03b9cf3132b0676379fec55316b61e03678979b88` and `e814f2a611be6de688a93612fa0f51c161993cd21263c3541325ae66661e991d`.

## Runs

The original run did not preserve raw stdout, stderr, or per-run status files. The counts, durations, and statuses below are observed tool output.

| Product | Rename worker | Selector result | Runner status |
| --- | --- | --- | --- |
| be0b | none | 1 passed, 0 failed, 0 skipped in 635 ms | 0 |
| 7b | active | 0 passed, 1 failed; `SystemReportCheckError.Invalid: PCI function attributes are incomplete` | 1 |
| be0b | active | 1 passed, 0 failed, 0 skipped in 652 ms | 0 |

The stress runner renamed only directories under its separate `/scratch/<run>/unrelated` tree. It stopped after the selector returned. The worker reported ready and not done in both stress runs.

## Original attempt

Commit `7d825b285` retains this first comparison. Its Docker command set `--cpus=4` and `--memory=8g`. It omitted `--memory-swap=8g`, `--network=none`, and `--platform linux/amd64`. Keep its results as diagnostic observations. The corrected attempt below adds all three flags and stores raw selector output.

The old trace records this failed call:

```text
33    openat2(3, "sys/bus/pci/devices/0000:00:1f.0", {flags=O_RDONLY|O_CLOEXEC|O_DIRECTORY, resolve=RESOLVE_NO_MAGICLINKS|RESOLVE_BENEATH}, 24) = -1 EAGAIN (Resource temporarily unavailable)
```

The be0b trace records these failed calls while the exact selector passes:

```text
33    openat2(3, "sys/bus/pci/devices/0000:00:1f.0", {flags=O_RDONLY|O_CLOEXEC|O_DIRECTORY, resolve=RESOLVE_NO_MAGICLINKS|RESOLVE_BENEATH}, 24) = -1 EAGAIN (Resource temporarily unavailable)
33    openat2(3, "sys/bus/pci/devices/0000:00:1f.0/class", {flags=O_RDONLY|O_CLOEXEC, resolve=RESOLVE_NO_MAGICLINKS|RESOLVE_BENEATH}, 24) = -1 EAGAIN (Resource temporarily unavailable)
```

Strace used `-e status=failed`, so these traces retain failed syscalls only. They do not show the later successful retry calls. The full captured traces are [7b stress](7b-stress.strace), [be0b control](be0b-control.strace), and [be0b stress](be0b-stress.strace).

## Original command and environment

The pinned image was `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`. Each container had limits of 4 CPUs and 8 GiB. It used `--init`, `--pids-limit=128`, and an 8-second `timeout` with a 2-second kill grace. The source and product bind mounts were read-only. The scratch bind mount was writable. The container command unset `LD_PRELOAD` and `XSH_DEV_BUILD_PRELOAD` and set `TMPDIR=/scratch`.

The run-specific container name, product directory, runner, and output directory were:

| Run | Container name | Product directory | Runner | Output directory |
| --- | --- | --- | --- | --- |
| be0b control | `xsh-rooted-path-race-be0b-control` | `$repo/.work/target-final-canonical/x86_64-unknown-linux-musl/verification` | `native-control-run.sh` | `be0b-control` |
| 7b stress | `xsh-rooted-path-race-7b-stress` | `$repo/.work/target-final-canonical-previous-run-final-be0b-products/x86_64-unknown-linux-musl/verification` | `native-run.sh` | `7b-stress` |
| be0b stress | `xsh-rooted-path-race-be0b-stress` | `$repo/.work/target-final-canonical/x86_64-unknown-linux-musl/verification` | `native-run.sh` | `be0b-stress` |

The host variables were `repo=/home/josh/d/laputa-systems/xsh` and `scratch=/home/josh/d/laputa-systems/xsh/.work/wt/consolidation-rooted-path-race/.work/tmp/rooted-path-race/product-comparison`. Each row gives the exact `name`, `products`, `runner`, and `label` values used in this host invocation:

```sh
mkdir -p "$scratch/$label"
cleanup() { docker rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT INT TERM
set +e
timeout -k 2 8s docker run --rm --init --cpus=4 --memory=8g --pids-limit=128 --name "$name" \
  --mount "type=bind,src=$repo,dst=/src,readonly" \
  --mount "type=bind,src=$products,dst=/products,readonly" \
  --mount "type=bind,src=$scratch,dst=/scratch" \
  --workdir /src --entrypoint /usr/bin/env \
  sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c \
  -u LD_PRELOAD -u XSH_DEV_BUILD_PRELOAD TMPDIR=/scratch \
  /bin/sh "/scratch/$runner" "/scratch/$label"
status=$?
set -e
printf '%s_container_status=%s\n' "$label" "$status"
cleanup
trap - EXIT INT TERM
exit "$status"
```

Each runner invoked the selector through:

```sh
/usr/bin/strace -f -qq -e trace=openat2,readlinkat -e status=failed -o "$scratch/native.trace" /products/xsht test --exact dev/tests/test-system-report-check.xsh::test_system_report_pci_capture_replays_raw_identity_links_and_rejects_tampering
```

The stress runner launched the worker with a 10,000,000-iteration cap:

```sh
python3 /scratch/rename-worker-ready.py "$scratch/unrelated" 10000000 "$scratch/renamer.ready" "$scratch/renamer.done" >"$scratch/renamer.log" 2>&1 &
```

It waited for the worker's ready marker before starting XSHT. Its exit trap killed and waited for the worker. The host wrapper also trapped exit and signals to remove the named container with `docker rm -f`. `docker ps -a` showed no named containers after the runs.

## Corrected attempt

The corrected rerun used the same be0b source tree, products, selector, runners, and rename worker. The Docker image inspected as `linux/amd64`. The command added `--memory-swap=8g`, `--network=none`, and `--platform linux/amd64`; all three invocations completed before the 8-second timeout. The source commit, tree, three source-file hashes, and four product hashes matched byte for byte before and after. Captured identity outputs are [source before](corrected/source.before.txt), [source after](corrected/source.after.txt), [products before](corrected/products.before.txt), and [products after](corrected/products.after.txt).

| Run | Product | Rename worker | Selector output | Status | Captures |
| --- | --- | --- | --- | --- | --- |
| be0b control | be0b | none | 1 passed, 0 failed, 0 skipped in 704 ms | 0 | [stdout](corrected/runs/be0b-control/stdout.log), [stderr](corrected/runs/be0b-control/stderr.log), [trace](corrected/runs/be0b-control/native.trace) |
| 7b stress | 7b | active | 0 passed, 1 failed; `fs-root-readlink: Resource temporarily unavailable (os error 11)` | 1 | [stdout](corrected/runs/7b-stress/stdout.log), [stderr](corrected/runs/7b-stress/stderr.log), [trace](corrected/runs/7b-stress/native.trace) |
| be0b stress | be0b | active | 1 passed, 0 failed, 0 skipped in 712 ms | 0 | [stdout](corrected/runs/be0b-stress/stdout.log), [stderr](corrected/runs/be0b-stress/stderr.log), [trace](corrected/runs/be0b-stress/native.trace) |

The corrected 7b stress trace records one EAGAIN from `openat2` on `sys/bus/pci/devices/0000:00:1f.0`. The passing be0b stress trace records 18 EAGAIN results, including repeated opens of that BDF, while the selector passes. The trace command captures failed syscalls only, so it does not include successful calls after retries.

All three stress/control containers were absent after cleanup. For both stress runs the worker reached its ready marker and had not reached its done marker when the selector returned; the runner trap stopped and waited for it, and container removal ended the remaining container processes. Per-run worker markers and the empty container-query outputs are saved alongside each run's logs. No native run, product build, or source edit ran against the main tree during this comparison.

The corrected invocation used `repo=/home/josh/d/laputa-systems/xsh`, `wt=/home/josh/d/laputa-systems/xsh/.work/wt/consolidation-rooted-path-race`, `scratch=$wt/.work/tmp/rooted-path-race/product-comparison-corrected`, and `evidence=$wt/dev/consolidation/evidence/rooted-path-openat2-9b2/corrected/runs`. `runner` values refer to the [control runner](corrected/native-control-run.sh), [stress runner](corrected/native-run.sh), and [rename worker](corrected/rename-worker-ready.py). The per-run arguments were:

| Label | Container name | Product directory | Runner |
| --- | --- | --- | --- |
| `be0b-control` | `xsh-rooted-path-race-be0b-control` | `$repo/.work/target-final-canonical/x86_64-unknown-linux-musl/verification` | `native-control-run.sh` |
| `7b-stress` | `xsh-rooted-path-race-7b-stress` | `$repo/.work/target-final-canonical-previous-run-final-be0b-products/x86_64-unknown-linux-musl/verification` | `native-run.sh` |
| `be0b-stress` | `xsh-rooted-path-race-be0b-stress` | `$repo/.work/target-final-canonical/x86_64-unknown-linux-musl/verification` | `native-run.sh` |

The host wrapper ran each row sequentially with this command shape and capture logic:

```sh
out=$evidence/$label
mkdir -p "$scratch/$label" "$out"
cleanup() { docker stop "$name" >/dev/null 2>&1 || true; docker rm "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT INT TERM
set +e
timeout -k 2 8s docker run --rm --init --cpus=4 --memory=8g --memory-swap=8g --network=none --platform linux/amd64 --pids-limit=128 --name "$name" \
  --mount "type=bind,src=$repo,dst=/src,readonly" \
  --mount "type=bind,src=$products,dst=/products,readonly" \
  --mount "type=bind,src=$scratch,dst=/scratch" \
  --workdir /src --entrypoint /usr/bin/env \
  sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c \
  -u LD_PRELOAD -u XSH_DEV_BUILD_PRELOAD TMPDIR=/scratch \
  /bin/sh "/scratch/$runner" "/scratch/$label" > "$out/stdout.log" 2> "$out/stderr.log"
status=$?
set -e
printf '%s\n' "$status" > "$out/status"
cleanup
trap - EXIT INT TERM
cp "$scratch/$label/native.trace" "$out/native.trace"
docker ps -a --filter "name=^/$name$" --format '{{.Names}}' > "$out/container-after-cleanup"
```

The output directories also retain the renamer log, ready/done observations, and exit status. The corrected artifacts are separate from the original trace files and do not replace the observations committed in `7d825b285`.
