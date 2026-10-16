#!/usr/bin/env bash
set -euo pipefail
TASK_ROOT=/home/josh/d/laputa-systems/xsh
TASK_WT=$TASK_ROOT/.work/wt/consolidation-xsht-clippy-integration
TASK_INFRA=$TASK_ROOT/.work/consolidation/infra
TASK_TARGET=$TASK_WT/.work/target-xsht-clippy-integration
TASK_TMP=$TASK_WT/.work/tmp/qualification-9f72af95/clippy-recheck-e55dc689
TASK_IMAGE=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
TASK_CIDFILE=$TASK_TMP/container.cid
TASK_SCRIPT='set +e
LD_PRELOAD=/usr/lib/libjemalloc.so.2 cargo clippy --locked --offline --target x86_64-unknown-linux-musl -p xsht --lib --tests --all-features --quiet -- -D warnings > "$TMPDIR/clippy-xsht.stdout.log" 2> "$TMPDIR/clippy-xsht.stderr.log"
clippy_xsht=$?
LD_PRELOAD=/usr/lib/libjemalloc.so.2 cargo clippy --locked --offline --target x86_64-unknown-linux-musl --workspace --all-targets --all-features --quiet -- -D warnings > "$TMPDIR/clippy-workspace.stdout.log" 2> "$TMPDIR/clippy-workspace.stderr.log"
clippy_workspace=$?
printf "clippy_xsht=%s\nclippy_workspace=%s\n" "$clippy_xsht" "$clippy_workspace" > "$TMPDIR/exit-status.txt"
[ "$clippy_xsht" -eq 0 ] && [ "$clippy_workspace" -eq 0 ]'
cleanup() {
  if [ -f "$TASK_CIDFILE" ]; then
    TASK_CID=$(cat "$TASK_CIDFILE")
    docker rm --force "$TASK_CID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT
printf '%s\n' "$TASK_SCRIPT" > "$TASK_TMP/container-script.txt"
set +e
flock -w 900 "$TASK_INFRA/cargo.lock" timeout -k 30 3600 docker run --rm --init --cidfile "$TASK_CIDFILE" --platform linux/amd64 --cpus=8 --memory=16g --memory-swap=16g --network=none --tmpfs /tmp:rw,exec,nosuid,mode=1777 \
  -v "$TASK_WT:$TASK_WT:ro" \
  -v "$TASK_ROOT/.git:$TASK_ROOT/.git:ro" \
  -v /home/josh/d/laputa-systems/laputa:/home/josh/d/laputa-systems/laputa:ro \
  -v /home/josh/.cargo/bin/kache:/usr/local/bin/kache:ro \
  -v /usr/local/bin/mold:/usr/local/bin/mold:ro \
  -v "$TASK_INFRA/mold-linker.sh:/usr/local/bin/xsh-mold-linker:ro" \
  -w "$TASK_WT" \
  -e LC_ALL=C -e TZ=UTC \
  -e CARGO_TARGET_DIR="$TASK_TARGET" \
  -e CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker \
  -e CARGO_BUILD_JOBS=8 \
  -e XSH_LAPUTA_CORPUS=/home/josh/d/laputa-systems/laputa \
  -e PATH=/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin \
  -e 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=-C target-feature=+crt-static -C link-arg=-Wl,--defsym=__isoc23_sscanf=sscanf -C link-arg=-Wl,--defsym=__isoc23_strtol=strtol' \
  -v "$TASK_TMP:$TASK_TMP:rw" -e "TMPDIR=$TASK_TMP" \
  -v "$TASK_TARGET:$TASK_TARGET:rw" \
  -v xsh-consolidation-kache:/root/.cache/kache \
  -v /usr/lib/libjemalloc.so.2:/usr/lib/libjemalloc.so.2:ro \
  -v xsh-cargo-registry:/root/.cargo/registry \
  -e RUSTC_WRAPPER=/usr/local/bin/kache \
  "$TASK_IMAGE" /bin/sh -ceu "$TASK_SCRIPT" > "$TASK_TMP/docker.stdout.log" 2> "$TASK_TMP/docker.stderr.log"
TASK_STATUS=$?
printf '%s\n' "$TASK_STATUS" > "$TASK_TMP/docker.exit-status.txt"
exit "$TASK_STATUS"
