#!/usr/bin/env bash
set -euo pipefail
TASK_ROOT=/home/josh/d/laputa-systems/xsh
TASK_TREE=$TASK_ROOT/.work/wt/typing-pipeline-integration
TASK_SCRATCH=$TASK_ROOT/.work/tmp/typing-ownership-debug
TASK_IMAGE=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
TASK_STAGE=$1
shift
set +e
docker run --rm --init --platform linux/amd64 --cpus=8 --memory=16g --memory-swap=16g --network=none \
  -v "$TASK_TREE:$TASK_TREE" \
  -v "$TASK_ROOT/.work/target-typing-pipeline:$TASK_ROOT/.work/target-typing-pipeline" \
  -v "$TASK_SCRATCH:$TASK_SCRATCH" \
  -v "$TASK_ROOT/.git:$TASK_ROOT/.git:ro" \
  -v "$TASK_ROOT/dev/musl-linker.sh:/usr/local/bin/xsh-mold-linker:ro" \
  -v /usr/local/bin/mold:/usr/local/bin/mold:ro \
  -v /home/josh/.cargo/bin/kache:/usr/local/bin/kache:ro \
  -v /usr/lib/libjemalloc.so.2:/usr/lib/libjemalloc.so.2:ro \
  -v xsh-cargo-registry:/root/.cargo/registry \
  -v xsh-consolidation-kache:/root/.cache/kache \
  -w "$TASK_TREE" \
  -e TARGET=x86_64-unknown-linux-musl \
  -e CARGO_TARGET_DIR="$TASK_ROOT/.work/target-typing-pipeline" \
  -e CARGO_BUILD_JOBS=8 \
  -e CARGO_BUILD_WARNINGS=deny \
  -e TMPDIR="$TASK_SCRATCH/tmp" \
  -e RUSTC_WRAPPER=/usr/local/bin/kache \
  -e CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker \
  -e 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol' \
  -e LD_PRELOAD=/usr/lib/libjemalloc.so.2 \
  -e LC_ALL=C -e TZ=UTC \
  "$TASK_IMAGE" "$@" \
  >"$TASK_SCRATCH/logs/$TASK_STAGE.stdout" 2>"$TASK_SCRATCH/logs/$TASK_STAGE.stderr"
TASK_STATUS=$?
printf '%s\n' "$TASK_STATUS" >"$TASK_SCRATCH/logs/$TASK_STAGE.exit"
printf 'stage=%s exit=%s\n' "$TASK_STAGE" "$TASK_STATUS"
exit "$TASK_STATUS"
