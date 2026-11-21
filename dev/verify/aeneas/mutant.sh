#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: dev/verify/aeneas/mutant.sh

Copies the slice and the Lean harness under .work/impl/mutant. In the copies,
checked addition in src/arith.rs becomes wrapping addition and checked
Duration subtraction in src/duration.rs becomes saturating subtraction. Both
copies are extracted with the pinned Charon and Aeneas. Exits 0 only when the
unchanged proofs of XshImpl.Arith.checked_int_binary_refines and
XshImpl.Duration.checked_duration_binary_refines both fail against the
mutants, and Lean proves that each extracted mutant violates its refinement
property, at i64::MAX + 1 and at 0ms - 1ms. Production source is not modified.
Run dev/verify/aeneas/extract.sh and dev/verify/aeneas/check.sh first, with
Lean 4.35.0 on PATH.
EOF
}
[[ $# -eq 0 ]] || { usage; exit 2; }

repo=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
here=$repo/dev/verify/aeneas
work=$repo/.work/impl
mutant=$work/mutant
image=xsh-aeneas-extract:nightly-2026-09-17

mutate() {
  local file=$1 original=$2 replacement=$3
  [[ $(rg -cF "$original" "$file") == 1 ]]
  python3 -I -c 'import sys, pathlib
path = pathlib.Path(sys.argv[1])
path.write_text(path.read_text().replace(sys.argv[2], sys.argv[3]))' "$file" "$original" "$replacement"
  [[ $(rg -cF "$replacement" "$file") == 1 ]]
}

[[ -x $work/release/aeneas ]] || { printf 'run extract.sh first\n' >&2; exit 2; }
rm -rf "$mutant"
mkdir -p "$mutant/root/src/syntax" "$mutant/root/dev/verify/aeneas" "$mutant/out/lean"
cp "$repo/src/arith.rs" "$repo/src/duration.rs" "$mutant/root/src/"
cp "$repo/src/syntax/node.rs" "$mutant/root/src/syntax/"
cp -r "$here/slice" "$mutant/root/dev/verify/aeneas/slice"
mutate "$mutant/root/src/arith.rs" \
  'BinaryOp::Add => left.checked_add(right),' \
  'BinaryOp::Add => Some(left.wrapping_add(right)),'
mutate "$mutant/root/src/duration.rs" \
  'DurationResult::Millis(a.checked_sub(b).ok_or(E::Underflow)?)' \
  'DurationResult::Millis(a.saturating_sub(b))'

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
rg -q 'U64.saturating_sub a b' "$generated"

mkdir -p "$mutant/lean/XshImpl"
cp "$here/lean/lean-toolchain" "$here/lean/lake-manifest.json" "$mutant/lean/"
cp "$here/lean/XshImpl/ArithSpec.lean" "$here/lean/XshImpl/Arith.lean" \
  "$here/lean/XshImpl/DurationSpec.lean" "$here/lean/XshImpl/Duration.lean" "$mutant/lean/XshImpl/"
cat >"$mutant/lean/XshImpl/Rejected.lean" <<'LEAN'
import XshImpl.ArithSpec
import XshImpl.DurationSpec
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

theorem saturating_subtraction_mutant_rejected :
    ¬ XshImpl.Duration.Refines duration.checked_duration_binary := by
  intro claim
  obtain ⟨out, run, decoded⟩ := claim .Sub (.Millis (U64.ofNat 0)) (.Millis (U64.ofNat 1))
  have saturated : core.num.U64.saturating_sub (U64.ofNat 0) (U64.ofNat 1) = U64.ofNat 0 := by
    decide
  simp [duration.checked_duration_binary, saturated, lift] at run
  subst run
  revert decoded
  decide

#print axioms wrapping_addition_mutant_rejected
#print axioms saturating_subtraction_mutant_rejected
LEAN
cp "$generated" "$mutant/lean/XshImpl/XshArithSlice.lean"
sed -e "s|^buildDir = .*|buildDir = \"$mutant/build\"|" \
    -e "s|^packagesDir = .*|packagesDir = \"$repo/.work/impl-packages\"|" \
    -e "s|^path = .*|path = \"$repo/formal\"|" \
    "$here/lean/lakefile.toml" >"$mutant/lean/lakefile.toml"

export TMPDIR=$repo/.work/tmp LAKE_CACHE_DIR=$repo/.work/lake-cache PATH=$work/bin:$PATH
cd "$mutant/lean"
for module in XshImpl.Arith XshImpl.Duration; do
  if lake build "$module" >"$mutant/$module.log" 2>&1; then
    printf 'negative control failed: %s accepted its mutant\n' "$module" >&2
    exit 1
  fi
  rg -n '^error' "$mutant/$module.log" | head -5
done
lake build XshImpl.ArithSpec XshImpl.DurationSpec
refutation=$(lake env lean XshImpl/Rejected.lean)
printf '%s\n' "$refutation"
expected="'wrapping_addition_mutant_rejected' depends on axioms: [propext, Classical.choice, Quot.sound]
'saturating_subtraction_mutant_rejected' depends on axioms: [propext, Classical.choice, Quot.sound]"
[[ $refutation == "$expected" ]]
printf 'rejected: both extracted mutants violate their refinement properties\n'
