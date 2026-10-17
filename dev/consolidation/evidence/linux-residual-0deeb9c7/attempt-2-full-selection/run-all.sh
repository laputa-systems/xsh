#!/bin/bash
set -euo pipefail
: "${SOURCE_ROOT:?set SOURCE_ROOT to the frozen source worktree}"
: "${PRODUCT_DIR:?set PRODUCT_DIR to the qualified product directory}"
: "${SCRATCH_ROOT:?set SCRATCH_ROOT to worktree .work/tmp}"
: "${IMAGE:?set IMAGE to the pinned test image digest}"
: "${SOURCE_COMMIT:?set SOURCE_COMMIT to the frozen source commit}"
: "${SOURCE_TREE:?set SOURCE_TREE to the frozen source tree}"
: "${COMMON_ROOT:?set COMMON_ROOT to the repository root}"
lock="$COMMON_ROOT/.work/consolidation/infra/native-suite.lock"
test -f "$lock"
export SOURCE_ROOT PRODUCT_DIR SCRATCH_ROOT IMAGE SOURCE_COMMIT SOURCE_TREE COMMON_ROOT PRODUCT_XSH_SHA256 PRODUCT_XSHT_SHA256
if ! flock -n "$lock" bash "$SCRATCH_ROOT/linux-residual-run-all-locked.sh"; then
  printf 'native suite lock unavailable or residual run failed\n' >&2
  exit 1
fi
