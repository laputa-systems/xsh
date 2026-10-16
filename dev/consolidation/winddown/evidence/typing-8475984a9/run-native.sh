#!/usr/bin/env bash
set -Eeuo pipefail

readonly root=/home/josh/d/laputa-systems/xsh
readonly lane=$root/.work/wt/typing-pipeline-integration
readonly output=$root/.work/tmp/typing-native-four-8475984a9
readonly target_dir=$root/.work/target-typing-pipeline-products
readonly native_lock=$root/.work/consolidation/infra/native-suite.lock
readonly image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
readonly expected_head=8475984a9ce1e8dbc987a8cf409f0747583c4878
readonly expected_tree=2d2dfebdb892fa6dbae70db5fd38199489321fdf
readonly common_git=$(git -C "$lane" rev-parse --path-format=absolute --git-common-dir)

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

[[ $(git -C "$lane" rev-parse HEAD) == "$expected_head" ]] || fail 'source HEAD differs from approved pin'
[[ $(git -C "$lane" rev-parse 'HEAD^{tree}') == "$expected_tree" ]] || fail 'source tree differs from approved pin'
[[ -z $(git -C "$lane" status --porcelain=v1 --untracked-files=all) ]] || fail 'source checkout is not clean'
[[ -d $target_dir/x86_64-unknown-linux-musl/verification ]] || fail 'verification products missing'
[[ ! -e $output && ! -L $output ]] || fail 'output directory already exists'
mkdir -p "$output/tmp"

product_dir=$target_dir/x86_64-unknown-linux-musl/verification
sha256sum "$product_dir/xsh" "$product_dir/xsht" "$product_dir/xshi" >"$output/products-before.sha256"
snapshot "$output/source-before.txt"
printf 'head=%s\ntree=%s\nimage=%s\nplatform=linux/amd64\ncpus=4\nmemory=8g\nmemory_swap=8g (no swap)\nnetwork=none\ninvocations=4\nexpected selectors=21 (capability=7, rank1=12, size=2)\n' \
  "$expected_head" "$expected_tree" "$image" >"$output/environment-and-limits.txt"
printf '%s\n' "$(docker image inspect --format '{{.Id}} {{.Os}} {{.Architecture}}' "$image")" >"$output/image-identity.txt"
[[ $(cat "$output/image-identity.txt") == "$image linux amd64" ]] || fail 'pinned image identity mismatch'

container_script='set -eu
run_dir=/scratch/typing-native-four-8475984a9
product_dir=/target/x86_64-unknown-linux-musl/verification
{
  grep "^Cpus_allowed_list:" /proc/self/status || true
  for cap in /sys/fs/cgroup/cpuset.cpus.effective /sys/fs/cgroup/cpu.max /sys/fs/cgroup/memory.max /sys/fs/cgroup/memory.swap.max; do
    if [ -r "$cap" ]; then printf "%s=" "$cap"; cat "$cap"; fi
  done
} >"$run_dir/test-resource-caps.txt"
sha256sum "$product_dir/xsh" "$product_dir/xsht" "$product_dir/xshi" >"$run_dir/products-container-before.sha256"
unset LD_PRELOAD || true
export PATH="$product_dir:$PATH"
xsht=$product_dir/xsht
cases=(
  "file:tests/xsh/typing-capability-witnesses.xsh"
  "file:tests/xsh/rank1-inference.xsh"
  "exact:tests/xsh/size-literals.xsh::test_size_literal_is_a_plain_unsigned_integer"
  "exact:tests/xsh/size-literals.xsh::test_size_literal_binding_is_unsigned"
)
: >"$run_dir/summary.tsv"
index=0
for case in "${cases[@]}"; do
  index=$((index + 1))
  test_log=$(printf "%02d" "$index")
  case_kind=${case%%:*}
  test_selector=${case#*:}
  if [ "$case_kind" = file ]; then
    if "$xsht" test "$test_selector" >"$run_dir/$test_log.stdout" 2>"$run_dir/$test_log.stderr"; then status=0; else status=$?; fi
  else
    if "$xsht" test --exact "$test_selector" >"$run_dir/$test_log.stdout" 2>"$run_dir/$test_log.stderr"; then status=0; else status=$?; fi
  fi
  printf "%s\t%s\n" "$status" "$case" >>"$run_dir/summary.tsv"
done
sha256sum "$product_dir/xsh" "$product_dir/xsht" "$product_dir/xshi" >"$run_dir/products-container-after.sha256"
cmp -s "$run_dir/products-container-before.sha256" "$run_dir/products-container-after.sha256" || exit 3
'

exec 9>"$native_lock"
flock -w 1800 -x 9 || fail 'native-suite lock wait timed out'
set +e
docker run --rm --init --platform linux/amd64 --cpuset-cpus=12-15 --cpus=4 --memory=8g --memory-swap=8g --network=none --entrypoint /bin/bash \
  --volume "$lane:/work:ro" \
  --volume "$target_dir:/target:ro" \
  --volume "$output:/scratch/typing-native-four-8475984a9:rw" \
  --volume "$common_git:$common_git:ro" \
  --workdir /work \
  --env TMPDIR=/scratch/typing-native-four-8475984a9/tmp \
  --env LC_ALL=C --env TZ=UTC \
  "$image" -c "$container_script" >"$output/docker-tests.stdout" 2>"$output/docker-tests.stderr"
docker_status=$?
set -e
printf '%s\n' "$docker_status" >"$output/docker-tests.exit-status.txt"
snapshot "$output/source-after.txt"
cmp -s "$output/source-before.txt" "$output/source-after.txt" || fail 'source changed during native tests'
sha256sum "$product_dir/xsh" "$product_dir/xsht" "$product_dir/xshi" >"$output/products-after.sha256"
cmp -s "$output/products-before.sha256" "$output/products-after.sha256" || fail 'verification products changed during native tests'
if (( docker_status != 0 )); then exit "$docker_status"; fi
if awk -F '\t' '$1 != 0 { bad=1 } END { exit !bad }' "$output/summary.tsv"; then fail 'one or more native invocations failed'; fi
awk -F '\t' 'BEGIN { pass=0; fail=0 } $1 == 0 { pass++; next } { fail++ } END { printf "passed=%d failed=%d total=%d\n", pass, fail, pass+fail }' "$output/summary.tsv"
printf 'evidence=%s\n' "$output"
