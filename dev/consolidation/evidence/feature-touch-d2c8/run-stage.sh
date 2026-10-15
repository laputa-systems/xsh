#!/usr/bin/env bash
set -u
REPO_ROOT=/home/josh/d/laputa-systems/xsh
FEATURE_TREE=$REPO_ROOT/.work/wt/feature-touch-final-replay
SCRATCH_ROOT=$REPO_ROOT/.work/tmp/feature-touch-final-replay
PROBE_IMAGE=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
TARGET_RUSTFLAGS='-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol'
STAGE_NAME=$1
shift
set +e
docker run --rm --init --platform linux/amd64 --cpus=8 --memory=16g --memory-swap=16g \
  -v "$FEATURE_TREE:$FEATURE_TREE" \
  -v "$SCRATCH_ROOT:$SCRATCH_ROOT" \
  -v "$REPO_ROOT/.git:$REPO_ROOT/.git:ro" \
  -v "$REPO_ROOT/dev/musl-linker.sh:/usr/local/bin/xsh-mold-linker:ro" \
  -v /usr/local/bin/mold:/usr/local/bin/mold:ro \
  -v /home/josh/.cargo/bin/kache:/usr/local/bin/kache:ro \
  -v /usr/lib/libjemalloc.so.2:/usr/lib/libjemalloc.so.2:ro \
  -v xsh-cargo-registry:/root/.cargo/registry \
  -v xsh-consolidation-kache:/root/.cache/kache \
  -w "$FEATURE_TREE" \
  -e TARGET=x86_64-unknown-linux-musl \
  -e CARGO_BUILD_JOBS=8 \
  -e "CARGO_TARGET_DIR=$SCRATCH_ROOT/target" \
  -e "TMPDIR=$SCRATCH_ROOT/tmp" \
  -e RUSTC_WRAPPER=/usr/local/bin/kache \
  -e CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker \
  -e "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=$TARGET_RUSTFLAGS" \
  -e LD_PRELOAD=/usr/lib/libjemalloc.so.2 \
  -e LC_ALL=C -e TZ=UTC \
  "$PROBE_IMAGE" cargo check --message-format=json "$@" \
  >"$SCRATCH_ROOT/logs/$STAGE_NAME.json" 2>"$SCRATCH_ROOT/logs/$STAGE_NAME.stderr"
CHECK_STATUS=$?
printf '%s\n' "$CHECK_STATUS" > "$SCRATCH_ROOT/logs/$STAGE_NAME.exit"
printf 'stage=%s exit=%s\n' "$STAGE_NAME" "$CHECK_STATUS"
exit "$CHECK_STATUS"
