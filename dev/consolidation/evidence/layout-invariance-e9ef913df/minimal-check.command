#!/usr/bin/env bash
set -Eeuo pipefail
root=/home/josh/d/laputa-systems/xsh
lane=$root/.work/wt/consolidation-layout-repair
scratch=$lane/.work/tmp/layout-repair/check-minimal
source=$lane/.work/tmp/layout-repair/narrow-source
pin=b1bc741fcd512d1c71b43116853278a4a3f1521a
products=$root/.work/consolidation/final-performance-gates/profile-artifacts-b1bc741f-20261017-r1/opt3/target/x86_64-unknown-linux-musl/release
harness=$root/.work/final-review-evidence/final-performance-gates/resources-7eb62cd9-20261017-r1/gate-batch-target/x86_64-unknown-linux-musl/release/build/xsht/8de7863f11fab919/out/integration-8de7863f11fab919
selector=lint_format_invariance::layout_perturbation_preserves_lints_on_the_repository_corpus
cleanup() {
  if [ -s "$scratch/container.cid" ]; then
    owned_cid=$(cat "$scratch/container.cid")
    docker rm --force "$owned_cid" > "$scratch/cleanup.stdout" 2> "$scratch/cleanup.stderr" || true
    docker ps --all --quiet --no-trunc --filter "id=$owned_cid" > "$scratch/cleanup.remaining"
    test ! -s "$scratch/cleanup.remaining"
  fi
}
trap cleanup EXIT
git -C "$lane" diff --exit-code "$pin" -- .
git -C "$lane" rev-parse HEAD > "$scratch/source-pin.txt"
sha256sum "$harness" "$products/xsh" "$products/xsht" > "$scratch/products-before.txt"
set +e
timeout -s TERM -k 5 240 docker run --rm --init \
  --cidfile "$scratch/container.cid" --platform linux/amd64 \
  --cpuset-cpus=0-11 --cpus=12 --memory=24g --memory-swap=24g --network=none \
  --volume "$source:/work:ro" \
  --volume "$harness:/harness/integration:ro" \
  --volume "$products:/work/target/x86_64-unknown-linux-musl/release:ro" \
  --volume "$scratch:/scratch:rw" --volume "$lane/.work/tmp/layout-repair/minimal:/fixtures:ro" --workdir /work --env TMPDIR=/scratch/tmp \
  xsh-test@sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c \
  /bin/sh -c 'for f in /fixtures/*.xsh; do echo "CASE $f"; /work/target/x86_64-unknown-linux-musl/release/xsht check "$f"; echo "STATUS $?"; done' \
  > "$scratch/stdout.log" 2> "$scratch/stderr.log"
status=$?
set -e
printf 'exact layout selector exit: %s\n' "$status" | tee "$scratch/status.txt"
sha256sum "$harness" "$products/xsh" "$products/xsht" > "$scratch/products-after.txt"
cmp "$scratch/products-before.txt" "$scratch/products-after.txt"
git -C "$lane" diff --exit-code "$pin" -- .
:
exit "$status"
