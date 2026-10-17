#!/usr/bin/env bash
set -Eeuo pipefail
root=/home/josh/d/laputa-systems/xsh
lane=$root/.work/wt/coverage-discovery-repair
pin=${1:?source pin required}
exec 9>"$root/.work/consolidation/infra/native-suite.lock"
flock -x 9
scratch=$lane/.work/tmp/discovery
out=$scratch/logs/native
private=$scratch/native-denied
mkdir -p "$out" "$private"
printf 'let value = 1\n' > "$private/hidden.xsh"
chmod 000 "$private"
trap 'chmod 755 "$private"' EXIT
git -C "$lane" diff --exit-code "$pin" -- . ':!dev/consolidation/evidence/coverage-discovery'
sha256sum "$lane/.work/target-discovery/x86_64-unknown-linux-musl/verification/xsh" \
  "$lane/.work/target-discovery/x86_64-unknown-linux-musl/verification/xsht" > "$out/products-before.txt"
for name in \
  test_runner_cov_list_does_not_execute_tests \
  test_runner_cov_exact_prints_coverage_sections \
  test_runner_cov_api_opt_in_prints_api_sections \
  test_runner_cov_json_out_writes_structured_report
do
  selector=tests/xsh/tooling-test.xsh::$name
  docker run --rm --init --platform linux/amd64 --user 1000:1000 \
    --cpuset-cpus=0-3 --cpus=4 --memory=8g --memory-swap=8g --network=none \
    --volume "$lane:/work:ro" \
    --volume "$root/.git:$root/.git:ro" \
    --volume "$lane/.work/target-discovery:/target:ro" \
    --volume "$scratch:/scratch:rw" \
    --volume "$lane/.work/tmp:/work/.work/tmp:rw" \
    --workdir /work --env TMPDIR=/scratch/tmp \
    --env PATH=/target/x86_64-unknown-linux-musl/verification:/usr/local/bin:/root/.cargo/bin:/usr/local/sbin:/usr/sbin:/usr/bin:/sbin:/bin \
    --entrypoint /bin/bash \
    xsh-test@sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c \
    -c 'set -Eeuo pipefail; test "$(id -u)" = 1000; test ! -r /scratch/native-denied; git check-ignore .work; exec xsht test --exact "$1"' _ "$selector" \
    > "$out/$name.stdout" 2> "$out/$name.stderr"
  rg -F "running 1 tests" "$out/$name.stdout"
  rg -F "$selector ... ok" "$out/$name.stdout"
done
git -C "$lane" diff --exit-code "$pin" -- . ':!dev/consolidation/evidence/coverage-discovery'
sha256sum "$lane/.work/target-discovery/x86_64-unknown-linux-musl/verification/xsh" \
  "$lane/.work/target-discovery/x86_64-unknown-linux-musl/verification/xsht" > "$out/products-after.txt"
cmp "$out/products-before.txt" "$out/products-after.txt"
