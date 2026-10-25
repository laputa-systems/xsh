#!/bin/bash
set -euo pipefail
TASK_ROOT=/home/josh/d/laputa-systems/xsh
TASK_SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
TASK_IMAGE_ID=$(python3 "$TASK_SCRIPT_DIR/../../../native_image.py")
if [ "$#" -eq 1 ] && [ "$1" = "--image-id" ]; then
  printf '%s\n' "$TASK_IMAGE_ID"
  exit 0
fi
if [ "$#" -lt 3 ]; then
  echo "usage: run-native.sh WORKTREE build|run COMMAND..." >&2
  exit 2
fi
TASK_TREE=$1
TASK_MODE=$2
shift 2
TASK_INFRA=$TASK_ROOT/.work/consolidation/infra
TASK_BASELINE=$TASK_INFRA/baseline-bin
case "$TASK_TREE" in
  "$TASK_ROOT"|"$TASK_ROOT"/.work/wt/*) ;;
  *) echo "worktree outside this checkout" >&2; exit 2;;
esac
TASK_UID=$(id -u)
TASK_GID=$(id -g)
TASK_DOCKER=(docker run --rm --init --platform linux/amd64 --cpus=12 --memory=24g --tmpfs /tmp:rw,exec,nosuid,mode=1777
  -v "$TASK_TREE:$TASK_TREE"
  -v "$TASK_ROOT/.git:$TASK_ROOT/.git:ro"
  -v "$TASK_ROOT/dev/consolidation/winddown/gate-tools/infra/container-build.sh:/usr/local/bin/xsh-container-build.sh:ro"
  -v /home/josh/d/laputa-systems/laputa:/home/josh/d/laputa-systems/laputa:ro
  -v "$TASK_BASELINE:$TASK_BASELINE:ro"
  -w "$TASK_TREE"
  -e LC_ALL=C -e TZ=UTC -e TMPDIR=/tmp
  -e CARGO_TARGET_DIR="$TASK_TREE/target"
  -v /home/josh/.cargo/bin/kache:/usr/local/bin/kache:ro
  -v /usr/local/bin/mold:/usr/local/bin/mold:ro
  -v "$TASK_INFRA/mold-linker.sh:/usr/local/bin/xsh-mold-linker:ro"
  -v xsh-consolidation-kache:/root/.cache/kache
  -e RUSTC_WRAPPER=/usr/local/bin/kache
  -e CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker
  -e CARGO_BUILD_JOBS=12
  -e XSH_LAPUTA_CORPUS=/home/josh/d/laputa-systems/laputa
  -e XSH_BIN="$TASK_BASELINE/xsh"
  -e "PATH=$TASK_BASELINE:/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin"
  -e 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=-C target-feature=+crt-static -C link-arg=-Wl,--defsym=__isoc23_sscanf=sscanf -C link-arg=-Wl,--defsym=__isoc23_strtol=strtol')
case "$TASK_MODE" in
  build)
    case "$*" in
      'cargo test '* )
        case " $* " in *' --no-run '*) ;; *) echo 'test build needs --no-run' >&2; exit 2;; esac ;;
      'cargo build '*|'cargo check '*) ;;
      *) echo 'build mode accepts only compile commands' >&2; exit 2;;
    esac
    exec 9>"$TASK_INFRA/cargo.lock"
    flock 9
    "${TASK_DOCKER[@]}" --stop-timeout 60 -v /usr/lib/libjemalloc.so.2:/usr/lib/libjemalloc.so.2:ro -v xsh-cargo-registry:/root/.cargo/registry -e TASK_UID="$TASK_UID" -e TASK_GID="$TASK_GID" "$TASK_IMAGE_ID" /bin/bash /usr/local/bin/xsh-container-build.sh --output "$TASK_TREE/target" -- /usr/bin/env LD_PRELOAD=/usr/lib/libjemalloc.so.2 "$@"
    ;;
  run)
    case " $* " in
      *" cargo test "*|*" cargo build "*|*" cargo check "*) echo "compile with build mode and --no-run; run the emitted static harness" >&2; exit 2;;
    esac
    exec "${TASK_DOCKER[@]}" --user "$TASK_UID:$TASK_GID" -e HOME=/home/compat -e RUSTUP_HOME=/root/.rustup "$TASK_IMAGE_ID" "$@"
    ;;
  *) echo 'unknown mode' >&2; exit 2;;
esac
