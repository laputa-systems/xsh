#!/bin/bash
set -u
: "${SCRATCH_ROOT:?set SCRATCH_ROOT to worktree .work/tmp}"
set +e
bash "$SCRATCH_ROOT/linux-residual-run-container.sh" standard
standard_status=$?
bash "$SCRATCH_ROOT/linux-residual-run-container.sh" sysadmin
sysadmin_status=$?
printf 'standard_exit=%s sysadmin_exit=%s\n' "$standard_status" "$sysadmin_status"
if [[ "$standard_status" -ne 0 || "$sysadmin_status" -ne 0 ]]; then
  exit 1
fi
