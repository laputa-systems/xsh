#!/usr/bin/env bash
set -Eeuo pipefail

readonly root=/home/josh/d/laputa-systems/xsh
readonly lane=$root/.work/wt/native-tail-repair
readonly output=$lane/.work/tmp/tail-native-qualified-afe2998e4
readonly products=$root/.work/wt/consolidation-final-0dc9cff7c/.work/target-final-canonical/x86_64-unknown-linux-musl/verification
readonly product_dir=$products
readonly native_lock=$root/.work/consolidation/infra/native-suite.lock
readonly common_git=$(git -C "$lane" rev-parse --path-format=absolute --git-common-dir)
readonly image=xsh-test@sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
readonly expected_xsh=c137d1d1ccff63c3ae492af1106aebb1872ae23b3d0e2af1d017968a4aa19a6c
readonly expected_xsht=5dccc9f21f30b4b26e8f96a7815303dd3b7a13c94b132d421ad02cee3fada7e1
readonly expected_fixture_blob=69bd9994fc81a5f1822b9da28636a5677658f1ce
readonly base=0dc9cff7c2b16e4c201adb3abca11d7ebda8e1f4

fail() { printf '%s\n' "$*" >&2; exit 1; }

source_snapshot() {
  local destination=$1
  {
    printf 'head='; git -C "$lane" rev-parse HEAD
    printf 'tree='; git -C "$lane" rev-parse 'HEAD^{tree}'
    printf 'status='; git -C "$lane" status --porcelain=v1 --untracked-files=all
    printf 'base='; git -C "$lane" rev-parse "$base"
    printf 'fixture_blob='; git -C "$lane" hash-object core/tests/test-uu-tail-1.xsh
    printf 'tail_blob='; git -C "$lane" hash-object core/tail.xsh
    printf 'write_helper_blob='; git -C "$lane" hash-object core/tests/support/uu.xsh
    printf 'write_path_blob='; git -C "$lane" hash-object src/modules/fs.rs
  } >"$destination"
}

[[ -d $lane && -d $products && -d $lane/.work/tmp ]] || fail 'source, products, or lane scratch missing'
git -C "$lane" check-ignore -q .work/tmp || fail 'lane scratch is not ignored'
[[ -z $(git -C "$lane" status --porcelain=v1 --untracked-files=all) ]] || fail 'source checkout is not clean'
[[ $(git -C "$lane" hash-object core/tests/test-uu-tail-1.xsh) == "$expected_fixture_blob" ]] || fail 'fixture source differs from the reviewed repair'
[[ $(git -C "$lane" hash-object core/tail.xsh) == "$(git -C "$lane" rev-parse "$base:core/tail.xsh")" ]] || fail 'tail implementation differs from the canonical base'
[[ $(git -C "$lane" hash-object core/tests/support/uu.xsh) == "$(git -C "$lane" rev-parse "$base:core/tests/support/uu.xsh")" ]] || fail 'test write helper differs from the canonical base'
[[ $(git -C "$lane" hash-object src/modules/fs.rs) == "$(git -C "$lane" rev-parse "$base:src/modules/fs.rs")" ]] || fail 'Path.write implementation differs from the canonical base'
[[ -x $product_dir/xsh && -x $product_dir/xsht ]] || fail 'qualified verification products missing'
[[ ! -e $output && ! -L $output ]] || fail 'output directory already exists'

mkdir -p "$output/tmp"
source_snapshot "$output/source-before.txt"
sha256sum "$product_dir/xsh" "$product_dir/xsht" >"$output/products-before.sha256"
printf '%s  %s\n%s  %s\n' "$expected_xsh" "$product_dir/xsh" "$expected_xsht" "$product_dir/xsht" >"$output/products-expected.sha256"
sha256sum --check "$output/products-expected.sha256"
printf '%s\n' "$(docker image inspect --format '{{.Id}} {{.Os}} {{.Architecture}}' "$image")" >"$output/image-identity.txt"
[[ $(cat "$output/image-identity.txt") == 'sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c linux amd64' ]] || fail 'pinned image identity mismatch'
{
  printf 'source_head=%s\n' "$(git -C "$lane" rev-parse HEAD)"
  printf 'source_tree=%s\n' "$(git -C "$lane" rev-parse 'HEAD^{tree}')"
  printf 'base=%s\n' "$base"
  printf 'fixture_blob=%s\n' "$expected_fixture_blob"
  printf 'qualified_products=%s\n' "$product_dir"
  printf 'xsh_sha256=%s\n' "$expected_xsh"
  printf 'xsht_sha256=%s\n' "$expected_xsht"
  printf 'image=%s\nplatform=linux/amd64\nuser=1000:1000\ncpus=2\nmemory=4g\nmemory_swap=4g (no swap)\nnetwork=none\n' "$image"
  printf 'native_lock=%s\n' "$native_lock"
} >"$output/environment-and-pins.txt"

args=(run --rm --init --platform linux/amd64 --cpus=2 --memory=4g --memory-swap=4g --network=none --user=1000:1000 --entrypoint /bin/bash
  --volume "$lane:/work:ro"
  --volume "$lane/.work/tmp:/work/.work/tmp:rw"
  --volume "$product_dir:/target:ro"
  --volume "$output:/scratch:rw"
  --volume "$common_git:$common_git:ro"
  --workdir /work
  --env PATH=/target:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
  --env TMPDIR=/scratch/tmp
  --env LC_ALL=C --env TZ=UTC
  "$image" /work/dev/consolidation/evidence/tail-same-size-fixture-afe2998e4/qualified-container-command.sh)
{
  printf 'docker'
  printf ' %q' "${args[@]}"
  printf '\n'
} >"$output/docker.argv.txt"

exec 9>"$native_lock"
flock -w 1800 -x 9 || fail 'native-suite lock wait timed out'
if docker "${args[@]}" >"$output/docker.stdout" 2>"$output/docker.stderr"; then docker_status=0; else docker_status=$?; fi
printf '%s\n' "$docker_status" >"$output/docker.exit-status.txt"
source_snapshot "$output/source-after.txt"
cmp -s "$output/source-before.txt" "$output/source-after.txt" || fail 'source changed during native tests'
sha256sum "$product_dir/xsh" "$product_dir/xsht" >"$output/products-after.sha256"
cmp -s "$output/products-before.sha256" "$output/products-after.sha256" || fail 'qualified products changed during native tests'
cat "$output/summary.tsv"
if (( docker_status != 0 )); then fail "native test container exited with status $docker_status"; fi
awk -F '\t' 'BEGIN { pass=0; fail=0 } $1 == 0 { pass++; next } { fail++ } END { printf "passed=%d failed=%d total=%d\n", pass, fail, pass+fail }' "$output/summary.tsv"
printf 'evidence=%s\n' "$output"
