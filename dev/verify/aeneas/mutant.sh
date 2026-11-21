#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: dev/verify/aeneas/mutant.sh

Copies the slice and the Lean harness under .work/impl/mutant, replaces the
checked addition in the copy of src/arith.rs with a wrapping addition, and
extracts the copy with the pinned Charon and Aeneas. Exits 0 only when the
unchanged proof of XshImpl.Arith.checked_int_binary_refines fails against the
mutant and Lean proves that the extracted mutant violates the same refinement
property at i64::MAX + 1. Production source is not modified.
Run dev/verify/aeneas/extract.sh and dev/verify/aeneas/check.sh first.
EOF
}
[[ $# -eq 0 ]] || { usage; exit 2; }

repo=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
here=$repo/dev/verify/aeneas
work=$repo/.work/impl
mutant=$work/mutant
image=xsh-aeneas-extract:nightly-2026-09-17
original='BinaryOp::Add => left.checked_add(right),'
replacement='BinaryOp::Add => Some(left.wrapping_add(right)),'

[[ -x $work/release/aeneas ]] || { printf 'run extract.sh first\n' >&2; exit 2; }
rm -rf "$mutant"
mkdir -p "$mutant/root/src/syntax" "$mutant/root/dev/verify/aeneas" "$mutant/out/lean"
cp "$repo/src/arith.rs" "$repo/src/duration.rs" "$mutant/root/src/"
cp "$repo/src/syntax/node.rs" "$mutant/root/src/syntax/"
cp -r "$here/slice" "$mutant/root/dev/verify/aeneas/slice"
[[ $(rg -cF "$original" "$mutant/root/src/arith.rs") == 1 ]]
sed -i "s/BinaryOp::Add => left.checked_add(right),/$replacement/" "$mutant/root/src/arith.rs"
[[ $(rg -cF "$replacement" "$mutant/root/src/arith.rs") == 1 ]]

docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp -e CARGO_TARGET_DIR="$mutant/target" \
  -v "$work:$work" -w "$mutant/root/dev/verify/aeneas/slice" "$image" \
  "$work/release/charon" cargo --preset=aeneas --sysroot default \
  --start-from 'crate::arith' --start-from 'crate::duration' \
  --include 'core::convert::num::{impl core::convert::TryFrom<_> for _}' \
  --include 'core::num::error::TryFromIntError' \
  --dest-file "$mutant/out/xsh_arith_slice.llbc" -- --locked --quiet
"$work/release/aeneas" -backend lean -dest "$mutant/out/lean" -abort-on-error \
  -no-progress-bar "$mutant/out/xsh_arith_slice.llbc" >"$mutant/out/aeneas.log"
generated=$mutant/out/lean/XshArithSlice.lean
for kind in PosOverflow NegOverflow; do
  sed -i "s/core\.result\.Result\.Err core\.num\.error\.IntErrorKind\.$kind)/core.result.Result.Err ())/" "$generated"
done
rg -q 'I64.wrapping_add left right' "$generated"

mkdir -p "$mutant/lean/XshImpl"
cp "$here/lean/lean-toolchain" "$here/lean/lake-manifest.json" "$mutant/lean/"
cp "$here/lean/XshImpl/ArithSpec.lean" "$here/lean/XshImpl/Arith.lean" "$mutant/lean/XshImpl/"
cat >"$mutant/lean/XshImpl/Rejected.lean" <<'LEAN'
import XshImpl.ArithSpec
open Aeneas Aeneas.Std Result xsh_arith_slice

theorem wrapping_addition_mutant_rejected : ¬ XshImpl.Arith.Refines arith.checked_int_binary := by
  intro claim
  obtain ⟨out, run, decoded⟩ := claim .Add (I64.ofInt 9223372036854775807) (I64.ofInt 1)
  have wrapped : core.num.I64.wrapping_add (I64.ofInt 9223372036854775807) (I64.ofInt 1) =
      I64.ofInt (-9223372036854775808) := by
    decide
  simp [arith.checked_int_binary, wrapped, lift] at run
  subst run
  revert decoded
  decide

#print axioms wrapping_addition_mutant_rejected
LEAN
cp "$generated" "$mutant/lean/XshImpl/XshArithSlice.lean"
sed -e "s|^buildDir = .*|buildDir = \"$mutant/build\"|" \
    -e "s|^packagesDir = .*|packagesDir = \"$repo/.work/impl-packages\"|" \
    -e "s|^path = .*|path = \"$repo/formal\"|" \
    "$here/lean/lakefile.toml" >"$mutant/lean/lakefile.toml"

export TMPDIR=$repo/.work/tmp LAKE_CACHE_DIR=$repo/.work/lake-cache PATH=$work/bin:$PATH
cd "$mutant/lean"
if lake build XshImpl.Arith >"$mutant/build.log" 2>&1; then
  printf 'negative control failed: the refinement proof accepted the wrapping addition\n' >&2
  exit 1
fi
rg -n 'error' "$mutant/build.log" | rg -v 'remote URL' | head -20
lake build XshImpl.ArithSpec
refutation=$(lake env lean XshImpl/Rejected.lean)
printf '%s\n' "$refutation"
[[ $refutation == "'wrapping_addition_mutant_rejected' depends on axioms: [propext, Classical.choice, Quot.sound]" ]]
printf 'rejected: the extracted wrapping-addition mutant violates the refinement property\n'
