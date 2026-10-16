#!/usr/bin/env bash
set -Eeuo pipefail

readonly root=/home/josh/d/laputa-systems/xsh
readonly lane=$root/.work/wt/typing-pipeline-integration
readonly source_tmp=$lane/.work/tmp
readonly scratch=$root/.work/tmp
readonly output=$scratch/typing-native-products-8475984a9
readonly target_dir=$root/.work/target-typing-pipeline-products
readonly lock_path=$root/.work/consolidation/infra/cargo.lock
readonly image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
readonly target=x86_64-unknown-linux-musl
readonly expected_head=8475984a9ce1e8dbc987a8cf409f0747583c4878
readonly expected_tree=2d2dfebdb892fa6dbae70db5fd38199489321fdf
readonly rustflags='-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol'
readonly common_git=$(git -C "$lane" rev-parse --path-format=absolute --git-common-dir)
readonly kache=/home/josh/.cargo/bin/kache
readonly mold=/usr/local/bin/mold
readonly jemalloc=/usr/lib/libjemalloc.so.2
readonly linker=$lane/dev/musl-linker.sh

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

[[ -d $lane && -d $source_tmp && -d $scratch && -f $lock_path ]] || fail 'source, source scratch, output scratch, or Cargo lock missing'
git -C "$lane" check-ignore -q .work/tmp || fail 'source scratch is not ignored'
[[ $(git -C "$lane" rev-parse HEAD) == "$expected_head" ]] || fail 'source HEAD differs from approved pin'
[[ $(git -C "$lane" rev-parse 'HEAD^{tree}') == "$expected_tree" ]] || fail 'source tree differs from approved pin'
[[ -z $(git -C "$lane" status --porcelain=v1 --untracked-files=all) ]] || fail 'source checkout is not clean'
[[ -x $kache && -x $mold && -x $linker && -f $jemalloc ]] || fail 'required build tools missing'
[[ ! -e $output && ! -L $output ]] || fail 'output directory already exists'

mkdir -p "$target_dir" && mkdir "$output" "$output/tmp"
snapshot "$output/source-before.txt"
printf 'head=%s\ntree=%s\nimage=%s\nplatform=linux/amd64\ncpus=8\nmemory=16g\nmemory_swap=16g (no swap)\nnetwork=none\nTARGET=%s\nlocked offline cargo build --profile verification --target x86_64-unknown-linux-musl -p xsh --bin xsh -p xsht --bin xsht -p xshi --bin xshi\nCARGO_BUILD_WARNINGS=warn\nCARGO_BUILD_JOBS=8\nLD_PRELOAD=jemalloc during build only\n' "$expected_head" "$expected_tree" "$image" "$target" >"$output/environment-and-limits.txt"
printf '%s\n' "$(docker image inspect --format '{{.Id}} {{.Os}} {{.Architecture}}' "$image")" >"$output/image-identity.txt"
[[ $(cat "$output/image-identity.txt") == "$image linux amd64" ]] || fail 'pinned image identity mismatch'
for volume in xsh-cargo-registry xsh-consolidation-kache; do
  docker volume inspect --format '{{.Name}} {{.Driver}}' "$volume" >>"$output/cache-volumes.txt"
done

container_script='set -eu
run_dir=/scratch/typing-native-products-8475984a9
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
cargo build --locked --offline --profile verification --target "$TARGET" \
  -p xsh --bin xsh -p xsht --bin xsht -p xshi --bin xshi \
  --message-format=json >"$run_dir/cargo.stdout.jsonl" 2>"$run_dir/cargo.stderr.log"
status=$?
set -e
printf "%s\n" "$status" >"$run_dir/cargo.exit-status.txt"
exit "$status"'

start=$(date +%s)
exec 9>"$lock_path"
flock -w 1800 -x 9 || fail 'primary Cargo lock wait timed out'
printf 'wait_seconds=%s\n' "$(( $(date +%s) - start ))" >"$output/cargo-lock-wait.txt"
set +e
docker run --rm --init --platform linux/amd64 --cpuset-cpus=4-11 --cpus=8 --memory=16g --memory-swap=16g --network=none --entrypoint /bin/bash \
  --volume "$lane:/work:ro" \
  --volume "$source_tmp:/work/.work/tmp:rw" \
  --volume "$target_dir:/target:rw" \
  --volume "$output:/scratch/typing-native-products-8475984a9:rw" \
  --volume xsh-cargo-registry:/root/.cargo/registry \
  --volume xsh-consolidation-kache:/root/.cache/kache \
  --volume "$common_git:$common_git:ro" \
  --volume "$kache:/usr/local/bin/kache:ro" \
  --volume "$mold:/usr/local/bin/mold:ro" \
  --volume "$jemalloc:/usr/lib/libjemalloc.so.2:ro" \
  --volume "$linker:/usr/local/bin/xsh-mold-linker:ro" \
  --workdir /work \
  --env CARGO_HOME=/root/.cargo \
  --env RUSTUP_HOME=/root/.rustup \
  --env XDG_CACHE_HOME=/root/.cache \
  --env CARGO_TARGET_DIR=/target \
  --env TARGET="$target" \
  --env CARGO_BUILD_JOBS=8 \
  --env CARGO_BUILD_WARNINGS=warn \
  --env CARGO_NET_OFFLINE=true \
  --env RUSTC_WRAPPER=/usr/local/bin/kache \
  --env CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker \
  --env "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=$rustflags" \
  --env LD_PRELOAD=/usr/lib/libjemalloc.so.2 \
  --env TMPDIR=/scratch/typing-native-products-8475984a9/tmp \
  "$image" -c "$container_script" >"$output/docker-build.stdout" 2>"$output/docker-build.stderr"
build_status=$?
set -e
printf '%s\n' "$build_status" >"$output/docker-build.exit-status.txt"
snapshot "$output/source-after-build.txt"
cmp -s "$output/source-before.txt" "$output/source-after-build.txt" || fail 'source changed during product build'
if (( build_status != 0 )); then exit "$build_status"; fi

build_success=$(jaq -r 'select(.reason == "build-finished") | .success' "$output/cargo.stdout.jsonl")
[[ $build_success == true ]] || fail "Cargo build-finished success was not true: $build_success"
jaq -r 'select(.reason == "compiler-artifact" and (.target.kind | index("bin")) != null and (.target.name == "xsh" or .target.name == "xsht" or .target.name == "xshi") and .executable != null) | [.target.name, .executable, (.fresh // false), .package_id] | @tsv' "$output/cargo.stdout.jsonl" >"$output/product-events.tsv"
for product_name in xsh xsht xshi; do
  count=$(awk -F '\t' -v name="$product_name" '$1 == name { n++ } END { print n+0 }' "$output/product-events.tsv")
  [[ $count == 1 ]] || fail "expected one Cargo artifact event for $product_name, got $count"
done
while IFS=$'\t' read -r product_name container_path fresh package_id; do
  [[ $container_path == /target/* ]] || fail "Cargo path for $product_name is outside /target"
  product_path=$target_dir/${container_path#/target/}
  [[ -f $product_path && -x $product_path && ! -L $product_path ]] || fail "Cargo-reported product missing or unsafe: $product_name"
  printf '%s\t%s\t%s\t%s\n' "$product_name" "$product_path" "$fresh" "$package_id"
done <"$output/product-events.tsv" >"$output/products.tsv"
while IFS=$'\t' read -r product_name product_path fresh package_id; do sha256sum "$product_path"; done <"$output/products.tsv" >"$output/products.sha256"
printf 'status=0 products=3 evidence=%s target=%s\n' "$output" "$target_dir"
