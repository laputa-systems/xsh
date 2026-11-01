#!/usr/bin/env bash
set -Eeuo pipefail

if (($# != 7)); then
  printf 'usage: %s COMPILER_SOURCE COMPILER_REV XSH_ROOT XSH_REV LAPUTA_ROOT LAPUTA_REV OUTPUT\n' "$0" >&2
  exit 2
fi

compiler_source=$(cd "$1" && pwd)
compiler_revision=$2
xsh_root=$(cd "$3" && pwd)
xsh_revision=$4
laputa_root=$(cd "$5" && pwd)
laputa_revision=$6
output=$(python3 -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).resolve())' "$7")
repo=$(git rev-parse --show-toplevel)
exclude=$(git -C "$repo" rev-parse --git-path info/exclude)
if ! awk '$0 == ".work/" || $0 == "/.work/" { found = 1 } END { exit !found }' "$exclude" || \
  ! git -C "$repo" check-ignore -q --no-index .work/tmp/performance-corpus-inventory; then
  printf '.work/ must be ignored through %s before the inventory creates scratch\n' "$exclude" >&2
  exit 1
fi

verify_source() {
  [[ $(git -C "$compiler_source" rev-parse --show-toplevel) == "$compiler_source" ]]
  [[ $(git -C "$xsh_root" rev-parse --show-toplevel) == "$xsh_root" ]]
  [[ $(git -C "$laputa_root" rev-parse --show-toplevel) == "$laputa_root" ]]
  [[ $(git -C "$compiler_source" rev-parse --verify HEAD^{commit}) == "$compiler_revision" ]]
  [[ -z $(git -C "$compiler_source" status --porcelain) ]]
  [[ $(git -C "$xsh_root" rev-parse --verify HEAD^{commit}) == "$xsh_revision" ]]
  [[ $(git -C "$laputa_root" rev-parse --verify HEAD^{commit}) == "$laputa_revision" ]]
  [[ -z $(git -C "$xsh_root" status --porcelain) ]]
  [[ -z $(git -C "$laputa_root" status --porcelain) ]]
  for path in core/wc.xsh core/sort.xsh core/awk.xsh; do
    [[ -f $xsh_root/$path ]]
  done
  [[ -f $laputa_root/pm/generation.xsh ]]
}

verify_source
case "$output" in
  "$repo"/.work/tmp/*) ;;
  *) printf 'inventory output must be under %s/.work/tmp\n' "$repo" >&2; exit 1 ;;
esac
[[ ! -e $output ]]
mkdir -p "$repo/.work/tmp"
scratch=$(mktemp -d "$repo/.work/tmp/performance-corpus-inventory.XXXXXXXX")
trap 'rm -rf "$scratch"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -p "$scratch/src" "$scratch/tmp"
cat >"$scratch/Cargo.toml" <<EOF
[package]
name = "performance-corpus-inventory"
version = "0.0.0"
edition = "2024"

[workspace]

[profile.verification]
inherits = "release"
opt-level = 3
lto = false
codegen-units = 64

[dependencies]
xsh = { path = "$compiler_source", default-features = false }
EOF
cp "$repo/dev/performance/corpus/inventory.rs" "$scratch/src/main.rs"
cp "$compiler_source/Cargo.lock" "$scratch/Cargo.lock"
python3 - "$compiler_source" >>"$scratch/Cargo.toml" <<'PY'
import json
import sys
import tomllib

root = sys.argv[1]
with open(f"{root}/Cargo.toml", "rb") as stream:
    manifest = tomllib.load(stream)
for registry, patches in manifest.get("patch", {}).items():
    print(f"\n[patch.{registry}]")
    for name, spec in patches.items():
        path = json.dumps(root + "/" + spec["path"])
        print(f"{name} = {{ path = {path} }}")
PY
export TMPDIR=$scratch/tmp
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
target_dir=$(python3 -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).resolve())' "${XSH_CARGO_TARGET_DIR:-$scratch/target}")
case "$target_dir" in
  "$repo"/.work/*) ;;
  *) printf 'inventory target must stay under %s/.work\n' "$repo" >&2; exit 1 ;;
esac
export CARGO_TARGET_DIR=$target_dir
profile=${XSH_CARGO_PROFILE:-dev}
target=${XSH_CARGO_TARGET:-}
cargo_args=(run --offline --quiet --profile "$profile")
if [[ -n $target ]]; then
  cargo_args+=(--target "$target")
fi
cargo "${cargo_args[@]}" --manifest-path "$scratch/Cargo.toml" -- \
  xsh "$xsh_root" "$xsh_revision" core/wc.xsh \
  xsh "$xsh_root" "$xsh_revision" core/sort.xsh \
  xsh "$xsh_root" "$xsh_revision" core/awk.xsh \
  laputa "$laputa_root" "$laputa_revision" pm/generation.xsh \
  >"$scratch/practical-sites.tsv"
verify_source
mkdir -p "$(dirname "$output")"
mv "$scratch/practical-sites.tsv" "$output"
