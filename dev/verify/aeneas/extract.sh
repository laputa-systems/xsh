#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: dev/verify/aeneas/extract.sh [--check]

Extracts src/arith.rs and src/duration.rs through the slice crate with the
pinned Charon and Aeneas, then installs the Lean translation at
dev/verify/aeneas/lean/XshImpl/XshArithSlice.lean. With --check, fails when
the committed translation differs from a fresh extraction.
EOF
}

mode=install
case ${1-} in
  '') ;;
  --check) mode=check ;;
  *) usage; exit 2 ;;
esac
[[ $# -le 1 ]] || { usage; exit 2; }

repo=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
here=$repo/dev/verify/aeneas
work=$repo/.work/impl
release=nightly-2026.11.09-5a392f4
archive=aeneas-linux-x86_64.tar.gz
archive_sha256=54982f0f6bb8ec7080ef9d2a23932aebe6e3c032f32015963bbccc43103f723a
charon_version='0.1.315 (454586e9b770231e66d2062993e632dfc06463e2)'
image=xsh-aeneas-extract:nightly-2026-09-17
installed=$here/lean/XshImpl/XshArithSlice.lean

exclude_path=$(git -C "$repo" rev-parse --git-path info/exclude)
rg -qxF '.work/' "$exclude_path" || printf '.work/\n' >>"$exclude_path"
mkdir -p "$work/release" "$work/context"

if [[ ! -x $work/release/aeneas ]]; then
  curl -fsSL -o "$work/$archive" \
    "https://github.com/AeneasVerif/aeneas/releases/download/$release/$archive"
  printf '%s  %s\n' "$archive_sha256" "$work/$archive" | sha256sum -c - >/dev/null
  tar -xzf "$work/$archive" -C "$work/release" aeneas charon charon-driver
fi
[[ $("$work/release/aeneas" -version) == "aeneas $release" ]]

docker build -q -t "$image" -f "$here/extract.Dockerfile" "$work/context" >/dev/null
in_image() {
  docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp \
    -e CARGO_TARGET_DIR="$work/target" -v "$repo:$repo" \
    -w "$here/slice" "$image" "$@"
}
[[ $(in_image "$work/release/charon" version) == "$charon_version" ]]

rm -rf "$work/out"
mkdir -p "$work/out/lean"
in_image "$work/release/charon" cargo --preset=aeneas --sysroot default \
  --start-from 'crate::arith' --start-from 'crate::duration' \
  --include 'core::convert::num::{impl core::convert::TryFrom<_> for _}' \
  --include 'core::num::error::TryFromIntError' \
  --dest-file "$work/out/xsh_arith_slice.llbc" -- --locked --quiet
"$work/release/aeneas" -backend lean -dest "$work/out/lean" -abort-on-error \
  -no-progress-bar "$work/out/xsh_arith_slice.llbc" >"$work/out/aeneas.log"

generated=$work/out/lean/XshArithSlice.lean
for kind in PosOverflow NegOverflow; do
  pattern="core.result.Result.Err core.num.error.IntErrorKind.$kind)"
  [[ $(rg -cF "$pattern" "$generated") == 1 ]]
  sed -i "s/core\.result\.Result\.Err core\.num\.error\.IntErrorKind\.$kind)/core.result.Result.Err ())/" "$generated"
done
if rg -n '^(axiom|opaque) |sorry|noncomputable' "$generated"; then
  printf 'the translation leaves an axiom, opaque item or noncomputable section\n' >&2
  exit 1
fi

if [[ $mode == check ]]; then
  diff -u "$installed" "$generated"
else
  cp "$generated" "$installed"
fi
