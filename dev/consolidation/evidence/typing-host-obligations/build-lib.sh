#!/usr/bin/env bash
set -Eeuo pipefail
root=/home/josh/d/laputa-systems/xsh
lane=$root/.work/wt/typing-host-obligations
scratch=$lane/.work/tmp/typing-host-obligations
out=$scratch/logs/compile-iteration
image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
target_dir=$lane/.work/target-typing-host-obligations
mkdir -p "$out" "$scratch/tmp"
docker run --rm --init --platform linux/amd64 --cpuset-cpus=0-3 --cpus=4 --memory=8g --memory-swap=8g --network=none --entrypoint /bin/bash \
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
  --env CARGO_BUILD_JOBS=4 \
  --env CARGO_NET_OFFLINE=true \
  --env RUSTC_WRAPPER=/usr/local/bin/kache \
  --env CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker \
  --env 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol' \
  --env LD_PRELOAD=/usr/lib/libjemalloc.so.2 \
  --env TMPDIR=/scratch/tmp \
  "$image" -c 'cargo test --locked --offline --target "$TARGET" -p xsh --lib --no-run --message-format=json > /scratch/logs/compile-iteration/cargo.stdout.jsonl 2> /scratch/logs/compile-iteration/cargo.stderr.log'
