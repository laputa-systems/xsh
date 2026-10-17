#!/usr/bin/env bash
set -Eeuo pipefail
root=/home/josh/d/laputa-systems/xsh
lane=$root/.work/wt/coverage-permission-lint
scratch=$lane/.work/tmp/permission-lint
pin=7eb62cd993e578c13bf6befd68a6a6ac15133c78
target_dir=$root/.work/final-review-evidence/final-correctness-gates/correctness-b235a1a43-20261017-r1/target/products
mkdir -p "$scratch/tmp"
exec 9>"$root/.work/consolidation/infra/cargo.lock"
flock -x 9
git -C "$lane" diff --exit-code "$pin" -- crates/xsht/src/cli/files.rs
set +e
docker run --rm --init --platform linux/amd64 --cpus=4 --memory=8g --memory-swap=8g --network=none \
  --volume "$lane:$lane:ro" \
  --volume "$root/.git:$root/.git:ro" \
  --volume /home/josh/d/laputa-systems/laputa:/home/josh/d/laputa-systems/laputa:ro \
  --volume /home/josh/.cargo/bin/kache:/usr/local/bin/kache:ro \
  --volume /usr/local/bin/mold:/usr/local/bin/mold:ro \
  --volume "$root/.work/consolidation/infra/mold-linker.sh:/usr/local/bin/xsh-mold-linker:ro" \
  --volume "$target_dir:$target_dir:rw" \
  --volume "$scratch/tmp:$scratch/tmp:rw" \
  --volume xsh-consolidation-kache:/root/.cache/kache \
  --volume /usr/lib/libjemalloc.so.2:/usr/lib/libjemalloc.so.2:ro \
  --volume xsh-cargo-registry:/root/.cargo/registry \
  --workdir "$lane" \
  --env LC_ALL=C --env TZ=UTC \
  --env CARGO_TARGET_DIR="$target_dir" \
  --env CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker \
  --env CARGO_BUILD_JOBS=4 \
  --env XSH_LAPUTA_CORPUS=/home/josh/d/laputa-systems/laputa \
  --env PATH=/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin \
  --env 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=-C target-feature=+crt-static -C link-arg=-Wl,--defsym=__isoc23_sscanf=sscanf -C link-arg=-Wl,--defsym=__isoc23_strtol=strtol' \
  --env RUSTC_WRAPPER=/usr/local/bin/kache \
  --env LD_PRELOAD=/usr/lib/libjemalloc.so.2 \
  --env TMPDIR="$scratch/tmp" \
  sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c \
  cargo clippy --locked --offline -p xsht --target x86_64-unknown-linux-musl --all-targets --all-features --quiet -- -D warnings \
  > "$scratch/green.stdout" 2> "$scratch/green.stderr"
clippy_status=$?
set -e
printf 'focused Clippy exit: %s\n' "$clippy_status" | tee "$scratch/green.status"
git -C "$lane" diff --exit-code "$pin" -- crates/xsht/src/cli/files.rs
exit "$clippy_status"
