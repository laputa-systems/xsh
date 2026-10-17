#!/usr/bin/env bash
set -Eeuo pipefail
root=/home/josh/d/laputa-systems/xsh
lane=$root/.work/wt/consolidation-layout-repair
scratch=$lane/.work/tmp/layout-repair/build-xsht
lock=$root/.work/consolidation/infra/cargo.lock
exec 9>"$lock"
flock -x 9
image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
target_dir=$root/.work/final-review-evidence/final-performance-gates/resources-7eb62cd9-20261017-r1/gate-batch-target
mkdir -p "$scratch/tmp"
cleanup() {
  if [ -s "$scratch/container.cid" ]; then
    owned_cid=$(cat "$scratch/container.cid")
    docker rm --force "$owned_cid" > "$scratch/cleanup.stdout" 2> "$scratch/cleanup.stderr" || true
    docker ps --all --quiet --no-trunc --filter "id=$owned_cid" > "$scratch/cleanup.remaining"
    test ! -s "$scratch/cleanup.remaining"
  fi
}
trap cleanup EXIT
docker run --rm --init --cidfile "$scratch/container.cid" --platform linux/amd64 --cpuset-cpus=0-11 --cpus=12 --memory=24g --memory-swap=24g --network=none --entrypoint /bin/bash \
  --volume "$lane:/work:ro" \
  --volume "$target_dir:/target:rw" \
  --volume "$scratch:/scratch:rw" \
  --volume xsh-cargo-registry:/root/.cargo/registry \
  --volume xsh-consolidation-kache:/root/.cache/kache \
  --volume /home/josh/.cargo/bin/kache:/usr/local/bin/kache:ro \
  --volume /usr/local/bin/mold:/usr/local/bin/mold:ro \
  --volume /usr/lib/libjemalloc.so.2:/usr/lib/libjemalloc.so.2:ro \
  --volume "$lane/dev/musl-linker.sh:/usr/local/bin/xsh-mold-linker:ro" \
  --workdir /work \
  --env CARGO_HOME=/root/.cargo \
  --env CARGO_TARGET_DIR=/target \
  --env TARGET=x86_64-unknown-linux-musl \
  --env CARGO_BUILD_JOBS=12 \
  --env CARGO_NET_OFFLINE=true \
  --env RUSTC_WRAPPER=/usr/local/bin/kache \
  --env CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker \
  --env 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol' \
  --env LD_PRELOAD=/usr/lib/libjemalloc.so.2 \
  --env TMPDIR=/scratch/tmp \
  "$image" -c 'cargo build --locked --offline --profile release --target "$TARGET" -p xsht --bin xsht > /scratch/stdout.log 2> /scratch/stderr.log'
