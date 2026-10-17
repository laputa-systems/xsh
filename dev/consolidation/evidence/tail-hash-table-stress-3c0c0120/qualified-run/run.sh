#!/usr/bin/env bash
set -Eeuo pipefail
readonly root=/home/josh/d/laputa-systems/xsh
readonly lane=$root/.work/wt/tail-hash-table-repair
readonly output=$lane/.work/tmp/tail-hash-table-repair-verify
readonly products=$root/.work/wt/consolidation-final-3c0c01206/.work/target-final-canonical/x86_64-unknown-linux-musl/verification
readonly lock=$root/.work/consolidation/infra/native-suite.lock
readonly image=xsh-test@sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
readonly expected_head=16abfbed3975da8dabb4e955fd182c99dabd22dd
readonly expected_tree=126ccda5c8b49f47cb20103d87d354f43308929a
readonly expected_xsh=2576641bd019ea1ba46fa555e351668407dbbacabdf4d87cabcf63de32b5b017
readonly expected_xsht=5ba0907b1b5222f5dab0efa19496b711bb4036b14c0ab2c344d1d8987f7e8836
fail() { printf '%s\n' "$*" >&2; exit 1; }
source_snapshot() {
  local destination=$1
  {
    printf 'head='; git -C "$lane" rev-parse HEAD
    printf 'tree='; git -C "$lane" rev-parse 'HEAD^{tree}'
    printf 'status=\n'; git -C "$lane" status --porcelain=v1 --untracked-files=all
    printf 'test1_blob='; git -C "$lane" hash-object core/tests/test-uu-tail-1.xsh
    printf 'test2_blob='; git -C "$lane" hash-object core/tests/test-uu-tail-2.xsh
    printf 'tail_blob='; git -C "$lane" hash-object core/tail.xsh
    printf 'uu_blob='; git -C "$lane" hash-object core/tests/support/uu.xsh
    printf 'fs_blob='; git -C "$lane" hash-object src/modules/fs.rs
  } >"$destination"
}
mkdir -p "$output/tmp"
[[ $(git -C "$lane" rev-parse HEAD) == "$expected_head" ]] || fail 'source HEAD mismatch'
[[ $(git -C "$lane" rev-parse 'HEAD^{tree}') == "$expected_tree" ]] || fail 'source tree mismatch'
[[ -z $(git -C "$lane" status --porcelain=v1 --untracked-files=all) ]] || fail 'source is dirty'
[[ $(readlink -f "$products") == "$products" ]] || fail 'qualified product path did not resolve exactly'
[[ -x $products/xsh && -x $products/xsht ]] || fail 'qualified products are missing'
[[ $(docker image inspect --format '{{.Id}} {{.Os}} {{.Architecture}}' "$image") == 'sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c linux amd64' ]] || fail 'pinned image identity mismatch'
printf '%s  /target/xsh\n%s  /target/xsht\n' "$expected_xsh" "$expected_xsht" >"$output/products-container-expected.sha256"
printf '%s  %s\n%s  %s\n' "$expected_xsh" "$products/xsh" "$expected_xsht" "$products/xsht" >"$output/products-host-expected.sha256"
sha256sum -c "$output/products-host-expected.sha256"
source_snapshot "$output/source-before.txt"
sha256sum "$products/xsh" "$products/xsht" >"$output/products-before.sha256"
printf 'head=%s\ntree=%s\ntest1_blob=%s\ntest2_blob=%s\nxsh_sha256=%s\nxsht_sha256=%s\nproducts=%s\nimage=%s\nplatform=linux/amd64\nuid=1000:1000\ncpus=2\nmemory=4g\nmemory_swap=4g\nnetwork=none\nnative_lock=%s\n' \
  "$expected_head" "$expected_tree" "$(git -C "$lane" hash-object core/tests/test-uu-tail-1.xsh)" "$(git -C "$lane" hash-object core/tests/test-uu-tail-2.xsh)" \
  "$expected_xsh" "$expected_xsht" "$products" "$image" "$lock" >"$output/environment-and-pins.txt"
args=(run --rm --init --platform linux/amd64 --cpus=2 --memory=4g --memory-swap=4g --pids-limit=128 --network=none --user=1000:1000 --entrypoint /bin/bash
  --volume "$lane:/work:ro"
  --volume "$products:/target:ro"
  --volume "$output:/scratch:rw"
  --workdir /work
  --env PATH=/target:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
  --env TMPDIR=/scratch/tmp
  --env LC_ALL=C --env TZ=UTC
  "$image" /scratch/container.sh)
{
  printf docker
  printf ' %q' "${args[@]}"
  printf '\n'
} >"$output/docker.argv.txt"
exec 9>"$lock"
flock -w 1800 -x 9 || fail 'native suite lock wait timed out'
if docker "${args[@]}" >"$output/docker.stdout" 2>"$output/docker.stderr"; then status=0; else status=$?; fi
printf '%s\n' "$status" >"$output/docker.exit-status.txt"
source_snapshot "$output/source-after.txt"
cmp -s "$output/source-before.txt" "$output/source-after.txt" || fail 'source changed during verification'
sha256sum "$products/xsh" "$products/xsht" >"$output/products-after.sha256"
cmp -s "$output/products-before.sha256" "$output/products-after.sha256" || fail 'qualified products changed during verification'
if (( status != 0 )); then fail "focused verification container exited $status"; fi
cat "$output/probe.stdout" "$output/summary.tsv"
