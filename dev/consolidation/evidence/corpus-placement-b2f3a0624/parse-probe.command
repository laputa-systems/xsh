#!/usr/bin/env bash
set -Eeuo pipefail
root=/home/josh/d/laputa-systems/xsh
lane=$root/.work/wt/consolidation-corpus-repair
scratch=$lane/.work/tmp/corpus-repair
products=$root/.work/consolidation/final-performance-gates/profile-artifacts-19c9aded-20261017-r1/opt3/target/x86_64-unknown-linux-musl/release
fixture=dev/consolidation/typing-corpus/capability-witnesses/bytes-reduced.xsh
pin=19c9aded0ea42c979fbc6be32162320d29ded3b1
cleanup() {
  if [ -s "$scratch/check-byte.cid" ]; then
    owned_cid=$(cat "$scratch/check-byte.cid")
    docker rm --force "$owned_cid" > "$scratch/cleanup.stdout" 2> "$scratch/cleanup.stderr" || true
    docker ps --all --quiet --no-trunc --filter "id=$owned_cid" > "$scratch/cleanup.remaining"
    test ! -s "$scratch/cleanup.remaining"
  fi
}
trap cleanup EXIT
git -C "$lane" diff --exit-code "$pin" -- .
git -C "$lane" rev-parse HEAD > "$scratch/context.txt"
sha256sum "$lane/$fixture" "$products/xsh" "$products/xsht" > "$scratch/inputs-before.txt"
set +e
timeout -s TERM -k 5 30 docker run --rm --init \
  --cidfile "$scratch/check-byte.cid" \
  --platform linux/amd64 --cpuset-cpus=28-31 --cpus=4 \
  --memory=8g --memory-swap=8g --network=none \
  --volume "$lane:/source:ro" --volume "$products:/products:ro" \
  --volume "$scratch:/scratch:rw" --workdir /source \
  --env TMPDIR=/scratch/tmp \
  xsh-test@sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c \
  /products/xsht check "$fixture" > "$scratch/check-byte.stdout" 2> "$scratch/check-byte.stderr"
status=$?
set -e
printf 'xsht check exit: %s\n' "$status" | tee "$scratch/check-byte.status"
sha256sum "$lane/$fixture" "$products/xsh" "$products/xsht" > "$scratch/inputs-after.txt"
cmp "$scratch/inputs-before.txt" "$scratch/inputs-after.txt"
git -C "$lane" diff --exit-code "$pin" -- .
test "$status" -eq 2
