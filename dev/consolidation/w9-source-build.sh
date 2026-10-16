#!/bin/bash
set -euo pipefail

if (($# < 3)); then
  echo "usage: w9-source-build.sh SOURCE_ROOT build|run COMMAND..." >&2
  exit 2
fi

TASK_SOURCE=$1
TASK_MODE=$2
shift 2
TASK_SCRIPT=$(realpath "$0")
TASK_BUILDER_ROOT=$(git -C "$(dirname -- "$TASK_SCRIPT")/../.." rev-parse --show-toplevel)
TASK_COMMON_GIT=$(git -C "$TASK_BUILDER_ROOT" rev-parse --path-format=absolute --git-common-dir)
TASK_CONTROLLER_ROOT=$(cd -- "$(dirname -- "$TASK_COMMON_GIT")" && pwd -P)
TASK_PROGRAM_SNAPSHOT_HELPER="$TASK_BUILDER_ROOT/dev/consolidation/w9_program_snapshot.py"
TASK_SOURCE=$(realpath "$TASK_SOURCE")
TASK_TMP="$TASK_CONTROLLER_ROOT/.work/tmp"
TASK_DOCKER_BIN=/usr/bin/docker
TASK_DOCKER_HOST=unix:///var/run/docker.sock
TASK_PLAN=0
TASK_ACTION=()
if [[ $TASK_MODE == plan-build || $TASK_MODE == plan-run ]]; then
  TASK_MODE=${TASK_MODE#plan-}
  TASK_PLAN=1
fi

if [[ ! -d $TASK_SOURCE ]]; then
  echo "source root must be a canonical directory" >&2
  exit 2
fi
case "$TASK_SOURCE" in
  "$TASK_CONTROLLER_ROOT"|"$TASK_CONTROLLER_ROOT"/*) ;;
  *) echo "source root is outside the qualified candidate checkout" >&2; exit 2 ;;
esac
git -C "$TASK_CONTROLLER_ROOT" check-ignore --quiet .work/ || {
  echo "candidate .work is not ignored" >&2
  exit 2
}
git -C "$TASK_SOURCE" check-ignore --quiet .work/ || {
  echo "source .work is not ignored" >&2
  exit 2
}
if [[ ! -d $TASK_TMP ]]; then
  mkdir -p -- "$TASK_TMP"
fi
if [[ $(realpath "$TASK_TMP") != "$TASK_TMP" ]]; then
  echo "repository .work/tmp is not canonical" >&2
  exit 2
fi

task_paths_overlap() {
  local left=$1 right=$2
  [[ $left == "$right" || $left == "$right/"* || $right == "$left/"* ]]
}

TASK_SOURCE_COMMIT=$(git -C "$TASK_SOURCE" rev-parse HEAD)
TASK_SOURCE_TREE=$(git -C "$TASK_SOURCE" rev-parse 'HEAD^{tree}')
if [[ ${W9_SOURCE_COMMIT-} != "$TASK_SOURCE_COMMIT" || ${W9_SOURCE_TREE-} != "$TASK_SOURCE_TREE" ]]; then
  echo "source root identity differs before builder launch" >&2
  exit 2
fi

task_check_source_changes() {
  local role=$1 overlay_sha256=$2 status observed_overlay
  status=$(git -C "$TASK_SOURCE" status --porcelain=v1 --untracked-files=all)
  if [[ $role == run ]]; then
    [[ -z $status ]] || { echo "source worktree is dirty before run" >&2; exit 2; }
    return
  fi
  [[ $status == ' M tests/sema.rs' ]] || {
    echo "source worktree has changes outside the parity overlay" >&2
    exit 2
  }
  git -C "$TASK_SOURCE" diff --cached --quiet || {
    echo "source index is not clean" >&2
    exit 2
  }
  observed_overlay=$(sha256sum "$TASK_SOURCE/tests/sema.rs" | cut -d ' ' -f 1)
  [[ $observed_overlay == "$overlay_sha256" ]] || {
    echo "source parity overlay hash differs" >&2
    exit 2
  }
}

task_require_regular_file() {
  local path=$1
  [[ -f $path && ! -L $path && $(realpath "$path") == "$path" ]] || {
    echo "generated adapter build input is missing or not a canonical regular file: $path" >&2
    return 2
  }
}

task_build_input_sha256() {
  local overlay_path="$TASK_SOURCE/tests/sema.rs"
  local adapter_helper_path="$TASK_SOURCE/.work/consolidation/w9-publication-adapter.rs"
  local owner_helper_path="$TASK_SOURCE/.work/consolidation/w9-publication-owner-helper.rs"
  local overlay_sha256 adapter_helper_sha256 owner_helper_sha256
  task_require_regular_file "$overlay_path" || return 2
  task_require_regular_file "$adapter_helper_path" || return 2
  task_require_regular_file "$owner_helper_path" || return 2
  overlay_sha256=$(sha256sum -- "$overlay_path" | cut -d ' ' -f 1) || return 2
  [[ $overlay_sha256 == "$TASK_OVERLAY_SHA256" ]] || {
    echo "source parity overlay hash differs" >&2
    return 2
  }
  adapter_helper_sha256=$(sha256sum -- "$adapter_helper_path" | cut -d ' ' -f 1) || return 2
  owner_helper_sha256=$(sha256sum -- "$owner_helper_path" | cut -d ' ' -f 1) || return 2
  TASK_BUILD_INPUT_SHA256=$(printf 'xsh-w9-build-inputs-v1\0overlay\0%s\0adapter\0%s\0owner\0%s\0' \
    "$overlay_sha256" "$adapter_helper_sha256" "$owner_helper_sha256" \
    | sha256sum | cut -d ' ' -f 1) || return 2
}

if [[ ! -x $TASK_DOCKER_BIN ]]; then
  echo "pinned local Docker executable is unavailable" >&2
  exit 2
fi
TASK_DOCKER=("$TASK_DOCKER_BIN" --host "$TASK_DOCKER_HOST")
if ((TASK_PLAN == 0)); then
  if [[ ! -S /var/run/docker.sock ]] || ! /usr/bin/env -i PATH=/usr/bin:/bin HOME=/nonexistent \
    "$TASK_DOCKER_BIN" --host "$TASK_DOCKER_HOST" info --format '{{.ServerVersion}}' >/dev/null; then
    echo "local Docker endpoint could not be validated" >&2
    exit 2
  fi
fi

TASK_RUSTFLAGS='-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol'
TASK_IMAGE='sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c'
TASK_KACHE=/home/josh/.cargo/bin/kache
TASK_MOLD=/usr/local/bin/mold
TASK_JEMALLOC=$(realpath /usr/lib/libjemalloc.so.2)
TASK_KACHE_SHA256=b826e99983057974663d6863eafbee4e1b25ca9373071406b15dd02961b2c37d
TASK_MOLD_SHA256=78e4b6aec99fa067b81326f4b540893b92ae78e5b4176ab32be0565dcbd049d3
TASK_JEMALLOC_SHA256=2db538572b752a3ed5b6b8d9a76cc2966f0670ad481892cb423617c9e4345284

case "$TASK_MODE" in
  build)
    TASK_COMMAND=("$@")
    TASK_ROLE=${W9_ROLE:?W9_ROLE is required}
    TASK_OVERLAY_SHA256=${W9_OVERLAY_SHA256:?W9_OVERLAY_SHA256 is required}
    [[ $TASK_ROLE == reference || $TASK_ROLE == candidate || $TASK_ROLE == derived-reference ]] || {
      echo "unknown adapter source role" >&2
      exit 2
    }
    [[ $TASK_OVERLAY_SHA256 =~ ^[0-9a-f]{64}$ ]] || {
      echo "malformed parity overlay hash" >&2
      exit 2
    }
    if task_paths_overlap "$TASK_SOURCE" "$TASK_TMP"; then
      echo "compiler source root overlaps writable build output" >&2
      exit 2
    fi
    task_check_source_changes "$TASK_ROLE" "$TASK_OVERLAY_SHA256"
    task_build_input_sha256
    TASK_CARGO_TARGET_DIR="$TASK_TMP/w9-target-${TASK_SOURCE_COMMIT}-${TASK_SOURCE_TREE}-${TASK_BUILD_INPUT_SHA256}-${TASK_ROLE}"
    TASK_LINKER=$(realpath "${W9_LINKER_PATH:?W9_LINKER_PATH is required}")
    TASK_LINKER_SHA256=${W9_LINKER_SHA256:?W9_LINKER_SHA256 is required}
    [[ -x $TASK_LINKER && $(sha256sum "$TASK_LINKER" | cut -d ' ' -f 1) == "$TASK_LINKER_SHA256" ]] || {
      echo "qualified musl linker identity differs" >&2
      exit 2
    }
    for tool in "$TASK_KACHE" "$TASK_MOLD" "$TASK_JEMALLOC"; do
      [[ -f $tool && ! -L $tool ]] || {
        echo "required host build tool is missing or not a regular file: $tool" >&2
        exit 2
      }
    done
    [[ $(sha256sum "$TASK_KACHE" | cut -d ' ' -f 1) == "$TASK_KACHE_SHA256" \
      && $(sha256sum "$TASK_MOLD" | cut -d ' ' -f 1) == "$TASK_MOLD_SHA256" \
      && $(sha256sum "$TASK_JEMALLOC" | cut -d ' ' -f 1) == "$TASK_JEMALLOC_SHA256" ]] || {
      echo "host build tool identity differs" >&2
      exit 2
    }
    expected=(cargo test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --test integration --no-run --message-format=json-render-diagnostics)
    if ((${#TASK_COMMAND[@]} != ${#expected[@]})); then
      echo "source-qualified build command differs from the frozen harness command" >&2
      exit 2
    fi
    for index in "${!expected[@]}"; do
      if [[ ${TASK_COMMAND[$index]} != "${expected[$index]}" ]]; then
        echo "source-qualified build command differs at argument $index" >&2
        exit 2
      fi
    done
    TASK_DOCKER+=(run --rm --init --platform linux/amd64 --cpus=12 --memory=24g --memory-swap=24g --network=none
      -v "$TASK_CONTROLLER_ROOT:$TASK_CONTROLLER_ROOT:ro"
      -v "$TASK_TMP:$TASK_TMP:rw"
      -v "$TASK_TMP:/scratch:rw"
      -v "$TASK_LINKER:/usr/local/bin/xsh-mold-linker:ro"
      -v "$TASK_KACHE:/usr/local/bin/kache:ro"
      -v "$TASK_MOLD:/usr/local/bin/mold:ro"
      -v "$TASK_JEMALLOC:/usr/lib/libjemalloc.so.2:ro"
      -v xsh-consolidation-kache:/root/.cache/kache
      -v xsh-cargo-registry:/root/.cargo/registry
      -w "$TASK_SOURCE"
      -e LC_ALL=C -e TZ=UTC -e TMPDIR=/scratch
      -e TARGET=x86_64-unknown-linux-musl -e DIST_PROFILE=verification
      -e CARGO_TARGET_DIR="$TASK_CARGO_TARGET_DIR" -e CARGO_BUILD_JOBS=12
      -e CARGO_BUILD_WARNINGS=deny
      -e RUSTC_WRAPPER=/usr/local/bin/kache
      -e CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker
      -e "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=$TASK_RUSTFLAGS"
      "$TASK_IMAGE" /usr/bin/env -u XSH_MODULE_PATH LD_PRELOAD=/usr/lib/libjemalloc.so.2 "${TASK_COMMAND[@]}")
    TASK_ACTION=(/usr/bin/env -i PATH=/usr/bin:/bin HOME=/nonexistent "${TASK_DOCKER[@]}")
    ;;
  run)
    TASK_CONTAINER_SCRATCH="$TASK_TMP/w9-container-scratch"
    if task_paths_overlap "$TASK_SOURCE" "$TASK_CONTAINER_SCRATCH"; then
      echo "compiler source root overlaps writable container scratch" >&2
      exit 2
    fi
    task_check_source_changes run ""
    TASK_INPUT_ROOT=
    TASK_PROGRAM_ROOT=
    TASK_OUTPUT_ROOT=
    TASK_CPU_SET=
    TASK_CONTAINER_NAME=
    TASK_COMMAND=()
    TASK_OUTPUT_PATHS=()
    for argument in "$@"; do
      case "$argument" in
        W9_INPUT_ROOT=*) TASK_INPUT_ROOT=${argument#W9_INPUT_ROOT=} ;;
        W9_PROGRAM_ROOT=*) TASK_PROGRAM_ROOT=${argument#W9_PROGRAM_ROOT=} ;;
        W9_OUTPUT_ROOT=*) TASK_OUTPUT_ROOT=${argument#W9_OUTPUT_ROOT=} ;;
        W9_CPU_SET=*) TASK_CPU_SET=${argument#W9_CPU_SET=} ;;
        W9_CONTAINER_NAME=*) TASK_CONTAINER_NAME=${argument#W9_CONTAINER_NAME=} ;;
        W9_RECEIPT_OUTPUT=*|W9_TIMING_OUTPUT=*|W9_ADAPTER_OUTPUT_DIR=*|W9_EXACT_OWNER_OUTPUT_DIR=*|W9_EXACT_OWNER_RECEIPT=*)
          TASK_COMMAND+=("$argument")
          TASK_OUTPUT_PATHS+=("${argument#*=}")
          ;;
        *) TASK_COMMAND+=("$argument") ;;
      esac
    done
    if ((${#TASK_OUTPUT_PATHS[@]})) && [[ -z $TASK_OUTPUT_ROOT ]]; then
      echo "W9 output root is required for capture output paths" >&2
      exit 2
    fi
    if [[ -n $TASK_CPU_SET && ! $TASK_CPU_SET =~ ^[0-9]+$ ]]; then
      echo "W9 CPU set must be one host CPU index" >&2
      exit 2
    fi
    if [[ -n $TASK_CONTAINER_NAME && ! $TASK_CONTAINER_NAME =~ ^(w9adapter-[0-9]+-(reference|candidate|derived-reference)-[A-D]-[0-9]+|w9adapter-[0-9]+-hash-(reference|candidate|derived-reference)|w9exact-[0-9]+-(reference|candidate)-([A-D]-[0-9]+|hash-[12]))$ ]]; then
      echo "W9 container name is malformed" >&2
      exit 2
    fi
    if [[ -n $TASK_INPUT_ROOT ]]; then
      TASK_INPUT_ROOT=$(realpath "$TASK_INPUT_ROOT")
      [[ -d $TASK_INPUT_ROOT ]] || { echo "W9 input root is not a directory" >&2; exit 2; }
      case "$TASK_INPUT_ROOT/" in
        "$TASK_CONTAINER_SCRATCH/"*)
          echo "W9 input root overlaps container scratch" >&2
          exit 2
          ;;
      esac
      case "$TASK_CONTAINER_SCRATCH/" in
        "$TASK_INPUT_ROOT/"*)
          echo "W9 input root overlaps container scratch" >&2
          exit 2
          ;;
      esac
    fi
    if [[ -n $TASK_OUTPUT_ROOT ]]; then
      [[ $TASK_OUTPUT_ROOT == /* && -d $TASK_OUTPUT_ROOT && ! -L $TASK_OUTPUT_ROOT ]] || {
        echo "W9 output root must be an existing canonical directory" >&2
        exit 2
      }
      [[ $(realpath "$TASK_OUTPUT_ROOT") == "$TASK_OUTPUT_ROOT" ]] || {
        echo "W9 output root must be an existing canonical directory" >&2
        exit 2
      }
      TASK_WORK_ROOT=$(realpath "$TASK_CONTROLLER_ROOT/.work")
      case "$TASK_OUTPUT_ROOT" in
        "$TASK_WORK_ROOT"/*) ;;
        *) echo "W9 output root is outside controller .work" >&2; exit 2 ;;
      esac
      TASK_PROTECTED_ROOTS=("$TASK_SOURCE" "$TASK_CONTAINER_SCRATCH")
      if [[ $TASK_BUILDER_ROOT != "$TASK_CONTROLLER_ROOT" ]]; then
        TASK_PROTECTED_ROOTS+=("$TASK_BUILDER_ROOT")
      fi
      for protected_root in "${TASK_PROTECTED_ROOTS[@]}"; do
        if task_paths_overlap "$TASK_OUTPUT_ROOT" "$protected_root"; then
          echo "W9 output root overlaps a protected source or scratch root" >&2
          exit 2
        fi
      done
    fi
    [[ -n $TASK_PROGRAM_ROOT ]] || {
      echo "W9 program root is required" >&2
      exit 2
    }
    TASK_PROGRAM_ROOT=$(realpath "$TASK_PROGRAM_ROOT")
    [[ -d $TASK_PROGRAM_ROOT ]] || { echo "W9 program root is not a directory" >&2; exit 2; }
    case "$TASK_PROGRAM_ROOT" in
      "$TASK_CONTROLLER_ROOT"/*) ;;
      *) echo "W9 program root is outside the controller checkout" >&2; exit 2 ;;
    esac
    case "$TASK_PROGRAM_ROOT/" in
      "$TASK_SOURCE/"*) echo "W9 program root overlaps compiler source root" >&2; exit 2 ;;
    esac
    case "$TASK_SOURCE/" in
      "$TASK_PROGRAM_ROOT/"*) echo "W9 program root overlaps compiler source root" >&2; exit 2 ;;
    esac
    if [[ -n $TASK_INPUT_ROOT ]]; then
      case "$TASK_PROGRAM_ROOT/" in
        "$TASK_INPUT_ROOT/"*) echo "W9 program root overlaps input root" >&2; exit 2 ;;
      esac
      case "$TASK_INPUT_ROOT/" in
        "$TASK_PROGRAM_ROOT/"*) echo "W9 program root overlaps input root" >&2; exit 2 ;;
      esac
    fi
    if [[ -n $TASK_OUTPUT_ROOT ]]; then
      if task_paths_overlap "$TASK_OUTPUT_ROOT" "$TASK_PROGRAM_ROOT"; then
        echo "W9 output root overlaps a protected source or input root" >&2
        exit 2
      fi
      if [[ -n $TASK_INPUT_ROOT ]] && task_paths_overlap "$TASK_OUTPUT_ROOT" "$TASK_INPUT_ROOT"; then
        echo "W9 output root overlaps a protected source or input root" >&2
        exit 2
      fi
    fi
    for output_path in "${TASK_OUTPUT_PATHS[@]}"; do
      [[ $output_path == /* ]] || {
        echo "W9 capture output path must be absolute" >&2
        exit 2
      }
      [[ $output_path != */ && ! -L $output_path ]] || {
        echo "W9 capture output path is not a canonical leaf" >&2
        exit 2
      }
      output_parent=${output_path%/*}
      [[ -n $output_parent ]] || output_parent=/
      output_name=${output_path##*/}
      [[ -n $output_name && $output_name != . && $output_name != .. ]] || {
        echo "W9 capture output path is not a canonical leaf" >&2
        exit 2
      }
      [[ -d $output_parent ]] || {
        echo "W9 capture output parent is not an existing directory" >&2
        exit 2
      }
      canonical_output_path="$(realpath "$output_parent")/$output_name"
      case "$canonical_output_path" in
        "$TASK_OUTPUT_ROOT"/*) ;;
        *) echo "W9 capture output path is outside W9 output root" >&2; exit 2 ;;
      esac
    done
    if task_paths_overlap "$TASK_PROGRAM_ROOT" "$TASK_CONTAINER_SCRATCH"; then
      echo "W9 program root overlaps writable container scratch" >&2
      exit 2
    fi
    python3 "$TASK_PROGRAM_SNAPSHOT_HELPER" validate "$TASK_PROGRAM_ROOT" >/dev/null
    if [[ -e $TASK_CONTAINER_SCRATCH || -L $TASK_CONTAINER_SCRATCH ]]; then
      [[ -d $TASK_CONTAINER_SCRATCH && ! -L $TASK_CONTAINER_SCRATCH \
        && $(realpath "$TASK_CONTAINER_SCRATCH") == "$TASK_CONTAINER_SCRATCH" ]] || {
        echo "container scratch path is not a canonical directory" >&2
        exit 2
      }
    elif ((TASK_PLAN == 0)); then
      mkdir -- "$TASK_CONTAINER_SCRATCH"
    fi
    TASK_DOCKER+=(run --rm --init --platform linux/amd64 --cpus=1 --memory=1g --memory-swap=1g --network=none
      --user "$(id -u):$(id -g)"
      -v "$TASK_CONTROLLER_ROOT:$TASK_CONTROLLER_ROOT:ro"
      -v "$TASK_PROGRAM_ROOT:$TASK_PROGRAM_ROOT:ro"
      -v "$TASK_CONTAINER_SCRATCH:/scratch:rw"
      -w "$TASK_SOURCE"
      -e LC_ALL=C -e TZ=UTC -e TMPDIR=/scratch
      -e HOME=/home/compat -e RUSTUP_HOME=/root/.rustup
      -e PATH=/usr/bin:/bin:/usr/local/bin:/root/.cargo/bin
    )
    if [[ -n $TASK_CPU_SET ]]; then
      TASK_DOCKER+=(--cpuset-cpus="$TASK_CPU_SET")
    fi
    if [[ -n $TASK_CONTAINER_NAME ]]; then
      TASK_DOCKER+=(--name "$TASK_CONTAINER_NAME")
    fi
    if [[ -n $TASK_INPUT_ROOT ]]; then
      TASK_DOCKER+=( -v "$TASK_INPUT_ROOT:$TASK_INPUT_ROOT:ro" )
      TASK_DOCKER+=( -e "W9_INPUT_ROOT=$TASK_INPUT_ROOT" )
    fi
    TASK_DOCKER+=( -e "W9_PROGRAM_ROOT=$TASK_PROGRAM_ROOT" )
    if [[ -n $TASK_OUTPUT_ROOT ]]; then
      TASK_DOCKER+=( -v "$TASK_OUTPUT_ROOT:$TASK_OUTPUT_ROOT:rw" )
    fi
    TASK_DOCKER+=("$TASK_IMAGE" /usr/bin/env -u XSH_MODULE_PATH "${TASK_COMMAND[@]}")
    TASK_ACTION=(/usr/bin/env -i PATH=/usr/bin:/bin HOME=/nonexistent "${TASK_DOCKER[@]}")
    ;;
  *)
    echo 'unknown source-qualified builder mode' >&2
    exit 2
    ;;
esac

if ((TASK_PLAN)); then
  printf '%s\0' "${TASK_ACTION[@]}"
else
  "${TASK_ACTION[@]}"
fi

TASK_AFTER_COMMIT=$(git -C "$TASK_SOURCE" rev-parse HEAD)
TASK_AFTER_TREE=$(git -C "$TASK_SOURCE" rev-parse 'HEAD^{tree}')
if [[ $TASK_AFTER_COMMIT != "$TASK_SOURCE_COMMIT" || $TASK_AFTER_TREE != "$TASK_SOURCE_TREE" ]]; then
  echo "source root identity changed during builder operation" >&2
  exit 2
fi
if [[ $TASK_MODE == build ]]; then
  task_check_source_changes "$TASK_ROLE" "$TASK_OVERLAY_SHA256"
else
  task_check_source_changes run ""
  python3 "$TASK_PROGRAM_SNAPSHOT_HELPER" validate "$TASK_PROGRAM_ROOT" >/dev/null
fi
