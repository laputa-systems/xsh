#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 2 ]; then
  echo "usage: run-native-final.sh WORKTREE build|lint|run|prepare-cargo|run-cargo COMMAND..." >&2
  exit 2
fi

TASK_TREE=$1
TASK_MODE=$2
shift 2
TASK_ROOT=/home/josh/d/laputa-systems/xsh
TASK_INFRA=$TASK_ROOT/.work/consolidation/infra
TASK_GATE_DIR=$TASK_ROOT/.work/final-review-evidence/final-correctness-gates
TASK_NATIVE_LOCK=$TASK_INFRA/native-suite.lock
TASK_NATIVE_CLEANUP_MARKER=$TASK_INFRA/native-suite.cleanup-unresolved
TASK_UID=$(id -u)
TASK_GID=$(id -g)
TASK_CIDFILE=${XSH_GATE_CIDFILE:?one unique container-id file is required}
TASK_TARGET_DIR=${XSH_GATE_TARGET_DIR:?one source-isolated Cargo target directory is required}
TASK_DISK_TMPDIR=$TASK_TREE/.work/tmp
TASK_CARGO_LOCK_TIMEOUT=${XSH_GATE_CARGO_LOCK_TIMEOUT:-900}
TASK_CARGO_HOME=${XSH_GATE_CARGO_HOME:-}
TASK_XDG_CACHE_HOME=${XSH_GATE_XDG_CACHE_HOME:-}

case "$TASK_TREE" in
  "$TASK_ROOT"|"$TASK_ROOT"/.work/wt/*) ;;
  *) echo "worktree is not the final main checkout" >&2; exit 2 ;;
esac
case "$TASK_TARGET_DIR" in
  "$TASK_GATE_DIR/"*/target/*) ;;
  *) echo "Cargo target must be isolated under this gate's ignored output directory" >&2; exit 2 ;;
esac
if [ -e "$TASK_NATIVE_CLEANUP_MARKER" ]; then
  echo "native container cleanup remains unresolved: $TASK_NATIVE_CLEANUP_MARKER" >&2
  cat "$TASK_NATIVE_CLEANUP_MARKER" >&2
  exit 75
fi

if [ -n "${XSH_GATE_NATIVE_LOCK_FD:-}" ]; then
  case "$XSH_GATE_NATIVE_LOCK_FD" in
    *[!0-9]*|'') echo 'inherited native-suite lock descriptor is invalid' >&2; exit 2 ;;
  esac
  TASK_NATIVE_LOCK_FD_PATH=$(readlink -f "/proc/self/fd/$XSH_GATE_NATIVE_LOCK_FD" 2>/dev/null || true)
  if [ "$TASK_NATIVE_LOCK_FD_PATH" != "$TASK_NATIVE_LOCK" ]; then
    echo 'inherited descriptor is not the canonical local native-suite lock' >&2
    exit 2
  fi
  flock -n "$XSH_GATE_NATIVE_LOCK_FD" || { echo 'canonical native-suite lock is not held' >&2; exit 75; }
else
  exec 8>"$TASK_NATIVE_LOCK"
  flock -n 8 || { echo 'canonical native-suite lock is held by another final gate or suite' >&2; exit 75; }
fi
if [ -e "$TASK_NATIVE_CLEANUP_MARKER" ]; then
  echo "native container cleanup remains unresolved: $TASK_NATIVE_CLEANUP_MARKER" >&2
  cat "$TASK_NATIVE_CLEANUP_MARKER" >&2
  exit 75
fi
mkdir -p "$TASK_TARGET_DIR"

TASK_DOCKER_BASE=(docker run --rm --init --cidfile "$TASK_CIDFILE" --platform linux/amd64 --cpus=12 --memory=24g --memory-swap=24g --network=none --tmpfs /tmp:rw,exec,nosuid,mode=1777
  -v "$TASK_TREE:$TASK_TREE"
  -v "$TASK_ROOT/.git:$TASK_ROOT/.git:ro"
  -v /home/josh/d/laputa-systems/laputa:/home/josh/d/laputa-systems/laputa:ro
  -v /home/josh/.cargo/bin/kache:/usr/local/bin/kache:ro
  -v /usr/local/bin/mold:/usr/local/bin/mold:ro
  -v "$TASK_INFRA/mold-linker.sh:/usr/local/bin/xsh-mold-linker:ro"
  -w "$TASK_TREE"
  -e LC_ALL=C -e TZ=UTC
  -e CARGO_TARGET_DIR="$TASK_TARGET_DIR"
  -e CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker
  -e CARGO_BUILD_JOBS=12
  -e XSH_LAPUTA_CORPUS=/home/josh/d/laputa-systems/laputa
  -e PATH=/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin
  -e 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=-C target-feature=+crt-static -C link-arg=-Wl,--defsym=__isoc23_sscanf=sscanf -C link-arg=-Wl,--defsym=__isoc23_strtol=strtol')

task_docker() {
  printf 'inner Docker argv:' >&2
  printf ' %q' "$@" >&2
  printf '\n' >&2
  "$@"
}

task_disk_tmpdir_args() {
  TASK_GIT_COMMON_DIR=$(git -C "$TASK_TREE" rev-parse --path-format=absolute --git-common-dir)
  TASK_GIT_EXCLUDE=$TASK_GIT_COMMON_DIR/info/exclude
  if [ ! -f "$TASK_GIT_EXCLUDE" ]; then
    echo 'repository .work exclusion file is missing' >&2
    exit 2
  fi
  if [ -L "$TASK_TREE/.work" ] || [ -L "$TASK_DISK_TMPDIR" ]; then
    echo 'repository .work and .work/tmp must not be symlinks' >&2
    exit 2
  fi
  TASK_WORK_EXCLUDED=0
  while IFS= read -r TASK_EXCLUDE_RULE; do
    case "${TASK_EXCLUDE_RULE%%#*}" in
      .work|.work/|/.work|/.work/) TASK_WORK_EXCLUDED=1 ;;
    esac
  done < "$TASK_GIT_EXCLUDE"
  if [ "$TASK_WORK_EXCLUDED" -ne 1 ] || ! git -C "$TASK_TREE" check-ignore --quiet --no-index .work/tmp; then
    echo 'repository .work/tmp must be excluded by .git/info/exclude before Cargo use' >&2
    exit 2
  fi
  mkdir -p "$TASK_DISK_TMPDIR"
  if [ ! -d "$TASK_TREE/.work" ] || [ -L "$TASK_TREE/.work" ] || [ ! -d "$TASK_DISK_TMPDIR" ] || [ -L "$TASK_DISK_TMPDIR" ]; then
    echo 'repository .work/tmp must be a real directory' >&2
    exit 2
  fi
  TASK_TMP_ARGS=(-v "$TASK_DISK_TMPDIR:$TASK_DISK_TMPDIR" -e "TMPDIR=$TASK_DISK_TMPDIR")
}

case "$TASK_MODE" in
  build|lint)
    if [ "$#" -eq 0 ]; then echo 'build/lint command is required' >&2; exit 2; fi
    case "$*" in
      'cargo test '* )
        case " $* " in *' --no-run '*) ;; *) echo 'test build needs --no-run' >&2; exit 2;; esac ;;
      'cargo build '*|'cargo check '*) ;;
      'cargo clippy '*) ;;
      *) echo 'build mode accepts only cargo build/check/test --no-run/clippy' >&2; exit 2 ;;
    esac
    task_disk_tmpdir_args
    exec 9>"$TASK_INFRA/cargo.lock"
    flock -w "$TASK_CARGO_LOCK_TIMEOUT" 9 || { echo 'cargo build lock timed out' >&2; exit 75; }
    task_docker "${TASK_DOCKER_BASE[@]}" "${TASK_TMP_ARGS[@]}" -v "$TASK_TARGET_DIR:$TASK_TARGET_DIR" -v xsh-consolidation-kache:/root/.cache/kache -v /usr/lib/libjemalloc.so.2:/usr/lib/libjemalloc.so.2:ro -v xsh-cargo-registry:/root/.cargo/registry -e RUSTC_WRAPPER=/usr/local/bin/kache -e LD_PRELOAD=/usr/lib/libjemalloc.so.2 sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c "$@"
    ;;
  run)
    if [ "$#" -eq 0 ]; then echo 'run command is required' >&2; exit 2; fi
    case " $* " in
      *" cargo test "*|*" cargo build "*|*" cargo check "*|*" cargo clippy "*) echo "compile with build/lint mode and --no-run where appropriate" >&2; exit 2;;
    esac
    task_docker "${TASK_DOCKER_BASE[@]}" -e TMPDIR=/tmp -v "$TASK_TARGET_DIR:$TASK_TARGET_DIR:ro" --user "$TASK_UID:$TASK_GID" -e HOME=/home/compat -e RUSTUP_HOME=/root/.rustup sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c "$@"
    ;;
  prepare-cargo)
    if [ "$#" -ne 0 ]; then echo 'prepare-cargo accepts no command' >&2; exit 2; fi
    : "${TASK_CARGO_HOME:?dedicated Cargo home is required}"
    : "${TASK_XDG_CACHE_HOME:?dedicated Kache home is required}"
    case "$TASK_CARGO_HOME" in "$TASK_GATE_DIR/"*) ;; *) echo 'Cargo home must stay in the ignored gate output' >&2; exit 2;; esac
    case "$TASK_XDG_CACHE_HOME" in "$TASK_GATE_DIR/"*) ;; *) echo 'Kache home must stay in the ignored gate output' >&2; exit 2;; esac
    task_disk_tmpdir_args
    exec 9>"$TASK_INFRA/cargo.lock"
    flock -w "$TASK_CARGO_LOCK_TIMEOUT" 9 || { echo 'cargo cache preparation lock timed out' >&2; exit 75; }
    task_docker "${TASK_DOCKER_BASE[@]}" --user 0:0 \
      "${TASK_TMP_ARGS[@]}" \
      -v "$TASK_TARGET_DIR:$TASK_TARGET_DIR" \
      -v "$TASK_CARGO_HOME:$TASK_CARGO_HOME" \
      -v xsh-cargo-registry:"$TASK_CARGO_HOME/registry" \
      -v "$TASK_XDG_CACHE_HOME:$TASK_XDG_CACHE_HOME" \
      -v xsh-consolidation-kache:"$TASK_XDG_CACHE_HOME/kache" \
      -e HOME=/home/compat -e RUSTUP_HOME=/root/.rustup \
      sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c \
      /bin/sh -ceu 'chown -R "$1:$2" "$3" "$4" "$5" "$6" "$7"; stat -c "%U:%G %n" "$3" "$4" "$5" "$6" "$7"' \
      sh "$TASK_UID" "$TASK_GID" "$TASK_TARGET_DIR" "$TASK_CARGO_HOME" "$TASK_CARGO_HOME/registry" "$TASK_XDG_CACHE_HOME" "$TASK_XDG_CACHE_HOME/kache"
    ;;
  run-cargo)
    if [ "$#" -eq 0 ]; then echo 'run-cargo command is required' >&2; exit 2; fi
    : "${TASK_CARGO_HOME:?dedicated Cargo home is required}"
    : "${TASK_XDG_CACHE_HOME:?dedicated Kache home is required}"
    : "${XSH_GATE_CARGO_FORWARDER:?nested Cargo forwarder is required}"
    : "${XSH_GATE_RUSTC_WRAPPER:?compiler wrapper is required}"
    case "$TASK_CARGO_HOME" in "$TASK_GATE_DIR/"*) ;; *) echo 'Cargo home must stay in the ignored gate output' >&2; exit 2;; esac
    case "$TASK_XDG_CACHE_HOME" in "$TASK_GATE_DIR/"*) ;; *) echo 'Kache home must stay in the ignored gate output' >&2; exit 2;; esac
    case "$XSH_GATE_CARGO_FORWARDER" in "$TASK_GATE_DIR/cargo-forwarder.sh") ;; *) echo 'nested Cargo forwarder is not the reviewed gate helper' >&2; exit 2;; esac
    case "$XSH_GATE_RUSTC_WRAPPER" in "$TASK_GATE_DIR/rustc-jemalloc.sh") ;; *) echo 'Rustc wrapper is not the reviewed gate helper' >&2; exit 2;; esac
    task_disk_tmpdir_args
    exec 9>"$TASK_INFRA/cargo.lock"
    flock -w "$TASK_CARGO_LOCK_TIMEOUT" 9 || { echo 'nested cargo lock timed out' >&2; exit 75; }
    task_docker "${TASK_DOCKER_BASE[@]}" \
      "${TASK_TMP_ARGS[@]}" \
      -v "$TASK_TARGET_DIR:$TASK_TARGET_DIR" \
      -v "$TASK_CARGO_HOME:$TASK_CARGO_HOME" \
      -v xsh-cargo-registry:"$TASK_CARGO_HOME/registry" \
      -v "$TASK_XDG_CACHE_HOME:$TASK_XDG_CACHE_HOME" \
      -v xsh-consolidation-kache:"$TASK_XDG_CACHE_HOME/kache" \
      -v "$XSH_GATE_CARGO_FORWARDER:/usr/local/bin/cargo:ro" \
      -v "$XSH_GATE_RUSTC_WRAPPER:/usr/local/bin/xsh-rustc-jemalloc:ro" \
      -v /usr/lib/libjemalloc.so.2:/usr/lib/libjemalloc.so.2:ro \
      --user "$TASK_UID:$TASK_GID" \
      -e HOME=/home/compat -e RUSTUP_HOME=/root/.rustup \
      -e CARGO_HOME="$TASK_CARGO_HOME" -e XDG_CACHE_HOME="$TASK_XDG_CACHE_HOME" \
      -e XSH_GATE_RUSTC_WRAPPER=/usr/local/bin/xsh-rustc-jemalloc \
      sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c "$@"
    ;;
  *) echo 'unknown mode' >&2; exit 2 ;;
esac
