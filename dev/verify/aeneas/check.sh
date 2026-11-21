#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: dev/verify/aeneas/check.sh

Fetches the pinned Lean dependencies, applies the remainder-model patch to the
pinned Aeneas library, builds XshImpl, audits the transitive axioms of every
accepted theorem and control, and reruns the kernel on the whole import closure
with leanchecker --fresh. Run it with the pinned Lean 4.35.0 on PATH.
EOF
}
[[ $# -eq 0 ]] || { usage; exit 2; }

repo=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
here=$repo/dev/verify/aeneas
project=$here/lean
packages=$repo/.work/impl-packages
patch=$here/patches/signed-checked-rem.patch
toolbin=$repo/.work/impl/bin

exclude_path=$(git -C "$repo" rev-parse --git-path info/exclude)
rg -qxF '.work/' "$exclude_path" || printf '.work/\n' >>"$exclude_path"
mkdir -p "$repo/.work/tmp" "$repo/.work/lake-cache" "$toolbin"
export TMPDIR=$repo/.work/tmp LAKE_CACHE_DIR=$repo/.work/lake-cache
printf '#!/bin/sh\nunset LD_LIBRARY_PATH\nexec %s "$@"\n' "$(command -v curl)" >"$toolbin/curl"
chmod +x "$toolbin/curl"
export PATH=$toolbin:$PATH

[[ $(lean --version) == "Lean (version 4.35.0,"* ]]
cd "$project"
lake exe cache get
if ! git -C "$packages/aeneas" apply --reverse --check "$patch" 2>/dev/null; then
  git -C "$packages/aeneas" apply "$patch"
fi

lake build XshImpl

theorems=(
  XshImpl.Arith.checked_int_binary_refines
  XshImpl.Arith.production_refines
  XshImpl.Arith.decode_injective
  XshImpl.Duration.checked_duration_binary_refines
  XshImpl.Duration.production_refines
  XshImpl.Duration.decode_injective
  XshImpl.Controls.int_success_and_truncation
  XshImpl.Controls.int_error_branches
  XshImpl.Controls.int_diagnostics
  XshImpl.Controls.upstream_remainder_model_differs_at_min
  XshImpl.Controls.wrapping_overflow_rejected
  XshImpl.Controls.upstream_remainder_rejected
  XshImpl.Controls.floored_division_rejected
  XshImpl.Controls.swapped_zero_error_rejected
  XshImpl.Controls.admitted_comparison_rejected
  XshImpl.Controls.panicking_division_rejected
  XshImpl.Controls.always_overflow_rejected
  XshImpl.Controls.duration_success_and_truncation
  XshImpl.Controls.duration_error_branches
  XshImpl.Controls.duration_diagnostics
  XshImpl.Controls.wrapping_sum_rejected
  XshImpl.Controls.saturating_difference_rejected
  XshImpl.Controls.wrapped_factor_rejected
  XshImpl.Controls.wrapping_count_rejected
  XshImpl.Controls.always_type_error_rejected
)
audit_dir=$(mktemp -d "$TMPDIR/impl-axioms.XXXXXX")
trap 'rm -rf "$audit_dir"' EXIT
audit=$audit_dir/Audit.lean
{
  printf 'import XshImpl\n'
  printf '#print axioms %s\n' "${theorems[@]}"
} >"$audit"
report=$(lake env lean "$audit")
printf '%s\n' "$report"
[[ $(rg -c "depends on axioms" <<<"$report") == "${#theorems[@]}" ]]
if rg -v "depends on axioms: \[(propext|Classical\.choice|Quot\.sound)(, (propext|Classical\.choice|Quot\.sound))*\]$" <<<"$report"; then
  printf 'a theorem depends on an axiom outside propext, Quot.sound and Classical.choice\n' >&2
  exit 1
fi
if rg -n '\b(sorry|admit|native_decide|axiom|bv_decide)\b|maxHeartbeats [0-9]{8,}|maxRecDepth [0-9]{6,}' XshImpl; then
  printf 'a harness module uses a forbidden escape or an unbounded local cap\n' >&2
  exit 1
fi

/usr/bin/time -f 'leanchecker --fresh XshImpl: wall=%es user=%Us peak_rss=%MkB' \
  lake env leanchecker --fresh XshImpl
