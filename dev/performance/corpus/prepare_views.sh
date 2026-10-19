#!/usr/bin/env bash
set -Eeuo pipefail

if (($# != 7)); then
  printf 'usage: %s XSH_ROOT XSH_REV LAPUTA_ROOT LAPUTA_REV INVENTORY SITE_IDS OUTPUT_DIR\n' "$0" >&2
  exit 2
fi

xsh_root=$(cd "$1" && pwd)
xsh_revision=$2
laputa_root=$(cd "$3" && pwd)
laputa_revision=$4
inventory=$(cd "$(dirname "$5")" && pwd)/$(basename "$5")
site_ids=$6
output=$(python3 -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).resolve())' "$7")
repo=$(git rev-parse --show-toplevel)
exclude=$(git -C "$repo" rev-parse --git-path info/exclude)
if ! awk '$0 == ".work/" || $0 == "/.work/" { found = 1 } END { exit !found }' "$exclude" || \
  ! git -C "$repo" check-ignore -q --no-index .work/tmp/performance-corpus-views; then
  printf '.work/ must be ignored through %s before the view preparation creates scratch\n' "$exclude" >&2
  exit 1
fi

case "$output" in
  "$repo"/.work/tmp/*) ;;
  *) printf 'output directory must be under %s/.work/tmp\n' "$repo" >&2; exit 1 ;;
esac
[[ ! -e $output && ! -L $output ]]
[[ -f $inventory ]]

verify_source() {
  [[ $(git -C "$xsh_root" rev-parse --show-toplevel) == "$xsh_root" ]]
  [[ $(git -C "$laputa_root" rev-parse --show-toplevel) == "$laputa_root" ]]
  [[ $(git -C "$xsh_root" rev-parse --verify HEAD^{commit}) == "$xsh_revision" ]]
  [[ $(git -C "$laputa_root" rev-parse --verify HEAD^{commit}) == "$laputa_revision" ]]
  [[ -z $(git -C "$xsh_root" status --porcelain) ]]
  [[ -z $(git -C "$laputa_root" status --porcelain) ]]
}

verify_source
mkdir -p "$output/original/xsh" "$output/original/laputa" \
  "$output/reduced/xsh" "$output/reduced/laputa" "$output/tmp"
export TMPDIR=$output/tmp
git -C "$xsh_root" archive "$xsh_revision" | tar -x -C "$output/original/xsh"
git -C "$laputa_root" archive "$laputa_revision" | tar -x -C "$output/original/laputa"
cp -a "$output/original/xsh/." "$output/reduced/xsh/"
cp -a "$output/original/laputa/." "$output/reduced/laputa/"
python3 "$repo/dev/performance/corpus/apply_spans.py" \
  "$inventory" "$site_ids" "$xsh_revision" "$laputa_revision" \
  "$output/reduced/xsh" "$output/reduced/laputa"
verify_source
