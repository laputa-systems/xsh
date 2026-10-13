#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: dev/consolidation/w9-cache-measure.sh XSHT_BIN BUILD_RECEIPT LAPUTA_ROOT [OUTPUT_DIR]

Run check and default lint in normal and traced modes for this checkout and
Laputa. Create BUILD_RECEIPT with w9-cache-build.sh build using the pinned
runner. The receipt must match this clean source tree and XSHT_BIN. OUTPUT_DIR
must be fresh, under this checkout's .work/, and have an existing parent.
Trace validation requires every resolved module and entry key request to have
one prepare or reuse event and one check or cache-hit event. Repeated module
keys across shared cache owners fail; private per-worker repeats are reported.
USAGE
}

if [[ $# -lt 3 || $# -gt 4 ]]; then
  usage
  exit 2
fi

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
xsh_root=$(cd -- "$script_dir/../.." && pwd -P)
xsht_bin="$(cd -- "$(dirname -- "$1")" && pwd -P)/$(basename -- "$1")"
if [[ ! -x "$xsht_bin" ]]; then
  printf 'xsht binary is not executable: %s\n' "$xsht_bin" >&2
  exit 2
fi
build_receipt=$2
case "$build_receipt" in
  /*) ;;
  *) build_receipt="$xsh_root/$build_receipt" ;;
esac
laputa_root=$(cd -- "$3" && pwd -P)
if [[ -n "$(git -C "$xsh_root" status --porcelain)" ]]; then
  printf 'XSH source checkout must be clean before measurement\n' >&2
  exit 2
fi
if [[ -n "$(git -C "$laputa_root" status --porcelain)" ]]; then
  printf 'Laputa source checkout must be clean before measurement\n' >&2
  exit 2
fi
xsh_commit=$(git -C "$xsh_root" rev-parse HEAD)
laputa_commit=$(git -C "$laputa_root" rev-parse HEAD)
"$script_dir/w9-cache-build.sh" verify "$xsht_bin" "$build_receipt"
if [[ $# -eq 4 ]]; then
  output_dir=$4
  case "$output_dir" in
    /*) ;;
    *) output_dir="$xsh_root/$output_dir" ;;
  esac
else
  output_dir="$xsh_root/.work/w9-cache-measure-$(date -u +%Y%m%dT%H%M%SZ)-$$"
fi

work_dir=$(cd -- "$xsh_root/.work" && pwd -P)
output_parent=$(dirname -- "$output_dir")
output_name=$(basename -- "$output_dir")
if [[ ! -d "$output_parent" ]]; then
  printf 'output parent directory does not exist: %s\n' "$output_parent" >&2
  exit 2
fi
output_parent=$(cd -- "$output_parent" && pwd -P)
case "$output_parent/" in
  "$work_dir/"*) ;;
  *) printf 'output directory must be under %s\n' "$work_dir" >&2; exit 2 ;;
esac
output_dir="$output_parent/$output_name"
if [[ -e "$output_dir" ]]; then
  printf 'output directory already exists: %s\n' "$output_dir" >&2
  exit 2
fi
mkdir -p -- "$output_dir"

{
  printf 'xsh_root=%s\n' "$xsh_root"
  printf 'xsh_commit=%s\n' "$xsh_commit"
  printf 'laputa_root=%s\n' "$laputa_root"
  printf 'laputa_commit=%s\n' "$laputa_commit"
  printf 'xsht_bin=%s\n' "$xsht_bin"
  printf 'build_receipt=%s\n' "$build_receipt"
  printf 'commands=check .; lint .\n'
} > "$output_dir/run.txt"

run_command() {
  local project_name=$1
  local project_root=$2
  local command_name=$3
  local mode=$4
  local prefix="$output_dir/$project_name-$command_name-$mode"
  local status

  if [[ "$mode" == traced ]]; then
    if (cd -- "$project_root" && XSH_W9_CACHE_TRACE="$prefix.tsv" "$xsht_bin" "$command_name" .) > "$prefix.stdout" 2> "$prefix.stderr"; then
      status=0
    else
      status=$?
    fi
  else
    if (cd -- "$project_root" && "$xsht_bin" "$command_name" .) > "$prefix.stdout" 2> "$prefix.stderr"; then
      status=0
    else
      status=$?
    fi
  fi
  printf '%s\n' "$status" > "$prefix.status"
}

compare_outputs() {
  local project_name=$1
  local command_name=$2
  local normal="$output_dir/$project_name-$command_name-normal"
  local traced="$output_dir/$project_name-$command_name-traced"

  if [[ "$(cat "$normal.status")" != "$(cat "$traced.status")" ]]; then
    printf '%s %s status changed with tracing enabled\n' "$project_name" "$command_name" >&2
    return 1
  fi
  cmp "$normal.stdout" "$traced.stdout"
  sed -E '/^xsht (check|lint): [0-9]+ files in .* \(thread time by stage: .*\)$/d' \
    "$normal.stderr" > "$normal.diagnostics"
  sed -E '/^xsht (check|lint): [0-9]+ files in .* \(thread time by stage: .*\)$/d' \
    "$traced.stderr" > "$traced.diagnostics"
  cmp "$normal.diagnostics" "$traced.diagnostics"
  if [[ "$(cat "$traced.status")" != 0 ]]; then
    printf '%s %s returned status %s; outputs were captured for review\n' \
      "$project_name" "$command_name" "$(cat "$traced.status")" >&2
    return 1
  fi
}

for project_name in xsh laputa; do
  if [[ "$project_name" == xsh ]]; then
    project_root=$xsh_root
  else
    project_root=$laputa_root
  fi
  for command_name in check lint; do
    run_command "$project_name" "$project_root" "$command_name" normal
    run_command "$project_name" "$project_root" "$command_name" traced
    compare_outputs "$project_name" "$command_name"
    printf '%s %s\n' "$project_name" "$command_name" >> "$output_dir/summary.txt"
    awk -F '\t' -f "$script_dir/w9-cache-trace-validate.awk" \
      "$output_dir/$project_name-$command_name-traced.tsv" \
      | tee -a "$output_dir/summary.txt"
  done
done

printf 'W9 cache measurement complete: %s\n' "$output_dir"
