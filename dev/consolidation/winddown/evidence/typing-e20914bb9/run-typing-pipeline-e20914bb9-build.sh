#!/usr/bin/env bash
set -Eeuo pipefail
readonly root=/home/josh/d/laputa-systems/xsh
readonly lane=$root/.work/wt/typing-pipeline-integration
readonly scratch=$root/.work/tmp
readonly run_name=typing-pipeline-e20914bb9
readonly output=$scratch/$run_name
readonly target_dir=$root/.work/target-typing-c2d8f87
readonly lock_path=$root/.work/consolidation/infra/cargo.lock
readonly image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
readonly rustflags='-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol'
readonly common_git=$(git -C "$lane" rev-parse --path-format=absolute --git-common-dir)
readonly expected_head=e20914bb9c9091cab36bfe219184e2f9dbb5dca1
readonly expected_tree=1cae8ed274160dcb0df3e4168e28012ff1681647
fail() { printf '%s\n' "$*" >&2; exit 1; }
snapshot() {
  local destination=$1
  {
    printf 'head='; git -C "$lane" rev-parse HEAD
    printf 'tree='; git -C "$lane" rev-parse 'HEAD^{tree}'
    printf 'status='; git -C "$lane" status --porcelain=v1 --untracked-files=all
    sha256sum "$lane/Cargo.lock" "$lane/rust-toolchain.toml"
  } >"$destination"
}
[[ -d $lane && -d $lane/.work/tmp && -d $scratch && -f $lock_path ]] || fail 'source, scratch, or primary Cargo lock missing'
git -C "$lane" check-ignore -q .work/tmp || fail 'source scratch is not ignored'
[[ $(git -C "$lane" rev-parse HEAD) == "$expected_head" ]] || fail 'source HEAD differs from approved pin'
[[ $(git -C "$lane" rev-parse 'HEAD^{tree}') == "$expected_tree" ]] || fail 'source tree differs from approved pin'
[[ -z $(git -C "$lane" status --porcelain=v1 --untracked-files=all) ]] || fail 'source checkout is not clean'
[[ -d $target_dir && -x /home/josh/.cargo/bin/kache && -x /usr/local/bin/mold ]] || fail 'target cache or linker tools missing'
[[ ! -e $output && ! -L $output ]] || fail 'output directory already exists'
mkdir "$output" "$output/tmp"
snapshot "$output/source-before.txt"
printf 'head=%s\ntree=%s\nimage=%s\nplatform=linux/amd64\ncpus=8\nmemory=16g\nmemory_swap=16g (no swap)\nnetwork=none\nlib harness=locked offline cargo test --target x86_64-unknown-linux-musl -p xsh --lib --no-run\nproducts=locked offline cargo build --profile verification --target x86_64-unknown-linux-musl -p xsh --bin xsh -p xsht --bin xsht -p xshi --bin xshi\nCARGO_BUILD_WARNINGS=deny\nCARGO_BUILD_JOBS=8\nLD_PRELOAD=jemalloc during build only\n' "$expected_head" "$expected_tree" "$image" >"$output/environment-and-limits.txt"
printf '%s\n' "$(docker image inspect --format '{{.Id}} {{.Os}} {{.Architecture}}' "$image")" >"$output/image-identity.txt"
[[ $(cat "$output/image-identity.txt") == "$image linux amd64" ]] || fail 'pinned image identity mismatch'
for volume in xsh-cargo-registry xsh-consolidation-kache; do docker volume inspect --format '{{.Name}} {{.Driver}}' "$volume" >>"$output/cache-volumes.txt"; done
container_script='set -eu
run_dir=/scratch/typing-pipeline-e20914bb9
{
  printf "LD_PRELOAD=%s\n" "$LD_PRELOAD"
  printf "RUSTC_WRAPPER=%s\n" "$RUSTC_WRAPPER"
  printf "CARGO_TARGET_DIR=%s\n" "$CARGO_TARGET_DIR"
  printf "CARGO_BUILD_JOBS=%s\n" "$CARGO_BUILD_JOBS"
  printf "CARGO_BUILD_WARNINGS=%s\n" "$CARGO_BUILD_WARNINGS"
  printf "TMPDIR=%s\n" "$TMPDIR"
  grep "^Cpus_allowed_list:" /proc/self/status || true
  for cap in /sys/fs/cgroup/cpuset.cpus.effective /sys/fs/cgroup/cpu.max /sys/fs/cgroup/memory.max /sys/fs/cgroup/memory.swap.max; do
    if [ -r "$cap" ]; then printf "%s=" "$cap"; cat "$cap"; fi
  done
} >"$run_dir/build-resource-caps.txt"
set +e
cargo test --locked --offline --target "$TARGET" -p xsh --lib --no-run --message-format=json >"$run_dir/cargo-lib.stdout.jsonl" 2>"$run_dir/cargo-lib.stderr.log"
lib_status=$?
printf "%s\n" "$lib_status" >"$run_dir/cargo-lib.exit-status.txt"
if [ "$lib_status" -ne 0 ]; then exit "$lib_status"; fi
cargo build --locked --offline --profile verification --target "$TARGET" -p xsh --bin xsh -p xsht --bin xsht -p xshi --bin xshi --message-format=json >"$run_dir/cargo-products.stdout.jsonl" 2>"$run_dir/cargo-products.stderr.log"
product_status=$?
printf "%s\n" "$product_status" >"$run_dir/cargo-products.exit-status.txt"
exit "$product_status"'
start=$(date +%s)
exec 9>"$lock_path"
flock -w 1800 -x 9 || fail 'primary Cargo lock wait timed out'
printf 'wait_seconds=%s\n' "$(( $(date +%s) - start ))" >"$output/cargo-lock-wait.txt"
set +e
docker run --rm --init --platform linux/amd64 --cpuset-cpus=4-11 --cpus=8 --memory=16g --memory-swap=16g --network=none --entrypoint /bin/bash \
  --volume "$lane:/work:ro" \
  --volume "$lane/.work/tmp:/work/.work/tmp:rw" \
  --volume "$target_dir:/target:rw" \
  --volume "$scratch:/scratch:rw" \
  --volume xsh-cargo-registry:/root/.cargo/registry \
  --volume xsh-consolidation-kache:/root/.cache/kache \
  --volume "$common_git:$common_git:ro" \
  --volume /home/josh/.cargo/bin/kache:/usr/local/bin/kache:ro \
  --volume /usr/local/bin/mold:/usr/local/bin/mold:ro \
  --volume /usr/lib/libjemalloc.so.2:/usr/lib/libjemalloc.so.2:ro \
  --volume "$lane/dev/musl-linker.sh:/usr/local/bin/xsh-mold-linker:ro" \
  --workdir /work \
  --env RUN_NAME="$run_name" \
  --env CARGO_HOME=/root/.cargo \
  --env RUSTUP_HOME=/root/.rustup \
  --env XDG_CACHE_HOME=/root/.cache \
  --env CARGO_TARGET_DIR=/target \
  --env TARGET=x86_64-unknown-linux-musl \
  --env CARGO_BUILD_JOBS=8 \
  --env CARGO_BUILD_WARNINGS=deny \
  --env CARGO_NET_OFFLINE=true \
  --env RUSTC_WRAPPER=/usr/local/bin/kache \
  --env CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker \
  --env "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=$rustflags" \
  --env LD_PRELOAD=/usr/lib/libjemalloc.so.2 \
  --env TMPDIR=/scratch/$run_name/tmp \
  "$image" -c "$container_script" >"$output/docker-build.stdout" 2>"$output/docker-build.stderr"
build_status=$?
set -e
printf '%s\n' "$build_status" >"$output/docker-build.exit-status.txt"
snapshot "$output/source-after-build.txt"
cmp -s "$output/source-before.txt" "$output/source-after-build.txt" || fail 'source changed during build'
exit "$build_status"
