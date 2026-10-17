#!/usr/bin/env bash
set -Eeuo pipefail
root=/home/josh/d/laputa-systems/xsh
lane=$root/.work/wt/consolidation-corpus-repair
scratch=$lane/.work/tmp/corpus-repair
products=$root/.work/consolidation/final-performance-gates/profile-artifacts-19c9aded-20261017-r1/opt3/target/x86_64-unknown-linux-musl/release
harness=$root/.work/final-review-evidence/final-performance-gates/resources-7eb62cd9-20261017-r1/gate-batch-target/x86_64-unknown-linux-musl/release/build/xsht/8de7863f11fab919/out/integration-8de7863f11fab919
selector=lint_format_invariance::formatting_preserves_lints_on_the_repository_corpus
old=19c9aded0ea42c979fbc6be32162320d29ded3b1
candidate=b2f3a0624205e68d06f9a439ffe2dea5e7dc4541
phase=${1:?phase required}
case "$phase" in
  old) source=$root; pin=$old; expected_status=101 ;;
  candidate) source=$lane; pin=$candidate; expected_status=0 ;;
  *) exit 2 ;;
esac
mount_source=$source
if [ "$phase" = candidate ]; then
  mount_source=$scratch/candidate-source
fi
out=$scratch/invariance-$phase
mkdir -p "$out/tmp"
cleanup() {
  if [ -s "$out/container.cid" ]; then
    owned_cid=$(cat "$out/container.cid")
    docker rm --force "$owned_cid" > "$out/cleanup.stdout" 2> "$out/cleanup.stderr" || true
    docker ps --all --quiet --no-trunc --filter "id=$owned_cid" > "$out/cleanup.remaining"
    test ! -s "$out/cleanup.remaining"
  fi
}
trap cleanup EXIT
git -C "$source" diff --exit-code "$pin" -- .
git -C "$lane" diff --exit-code "$old" "$candidate" -- src crates Cargo.toml Cargo.lock rust-toolchain.toml .cargo
sha256sum "$harness" "$products/xsh" "$products/xsht" > "$out/products-before.txt"
git -C "$source" rev-parse HEAD > "$out/source.txt"
set +e
timeout -s TERM -k 5 180 docker run --rm --init \
  --cidfile "$out/container.cid" --platform linux/amd64 \
  --cpuset-cpus=28-31 --cpus=4 --memory=8g --memory-swap=8g --network=none \
  --volume "$mount_source:/work:ro" \
  --volume "$harness:/harness/integration:ro" \
  --volume "$products:/work/target/x86_64-unknown-linux-musl/release:ro" \
  --volume "$out:/scratch:rw" --workdir /work --env TMPDIR=/scratch/tmp \
  xsh-test@sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c \
  /harness/integration "$selector" --exact --nocapture --test-threads=1 \
  > "$out/stdout.log" 2> "$out/stderr.log"
status=$?
set -e
printf 'exact selector exit: %s\n' "$status" | tee "$out/status.txt"
sha256sum "$harness" "$products/xsh" "$products/xsht" > "$out/products-after.txt"
cmp "$out/products-before.txt" "$out/products-after.txt"
git -C "$source" diff --exit-code "$pin" -- .
rg -F 'running 1 test' "$out/stdout.log"
if [ "$expected_status" -eq 0 ]; then
  rg -F 'test result: ok. 1 passed; 0 failed;' "$out/stdout.log"
else
  rg -F 'test result: FAILED. 0 passed; 1 failed;' "$out/stdout.log"
  rg -F 'capability-witnesses/bytes-reduced.xsh' "$out/stderr.log"
fi
test "$status" -eq "$expected_status"
