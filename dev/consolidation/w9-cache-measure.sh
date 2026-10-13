#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: dev/consolidation/w9-cache-measure.sh XSHT_BIN LAPUTA_ROOT [OUTPUT_DIR]

Run check and default lint in normal and traced modes for this checkout and
Laputa. Build XSHT_BIN from this checkout's frozen source with the pinned
runner. OUTPUT_DIR must be fresh, under this checkout's .work/, and have an
existing parent directory.
USAGE
}

if [[ $# -lt 2 || $# -gt 3 ]]; then
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
laputa_root=$(cd -- "$2" && pwd -P)
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
if [[ $# -eq 3 ]]; then
  output_dir=$3
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

validate_trace() {
  local trace_path=$1
  awk -F '\t' '
    function fail(message) {
      print "invalid W9 cache trace: " message > "/dev/stderr"
      failed = 1
    }
    {
      if (NF != 13 || $1 != "W9CACHE" || $2 != "1") {
        fail("malformed row " NR)
        next
      }
      if (NR > 1 && $4 <= previous_sequence) fail("event sequence is not increasing")
      previous_sequence = $4
      owner = $3 SUBSEP $5
      if ($6 != "shared" && $6 != "private") fail("unknown cache kind " $6)
      if (owner_kind[owner] && owner_kind[owner] != $6) fail("owner kind changed")
      owner_kind[owner] = $6
      if ($7 == "cache") {
        owner_rows[owner]++
        next
      }
      if ($7 != "prepared" && $7 != "reuse" && $7 != "checked" && $7 != "check_hit") {
        fail("unknown event " $7)
        next
      }
      key = owner SUBSEP $8 SUBSEP $9 SUBSEP $10 SUBSEP $11 SUBSEP $12 SUBSEP $13
      full_key = $8 SUBSEP $9 SUBSEP $10 SUBSEP $11 SUBSEP $12 SUBSEP $13
      observed[key] = 1
      owner_for_full_key[full_key SUBSEP owner] = 1
      if ($7 == "prepared") {
        prepared[key]++
        if (prepared[key] > 1) fail("duplicate prepared record for one cache key")
        prepared_by_kind[$6]++
      } else if ($7 == "reuse") {
        reused[key]++
        reused_by_kind[$6]++
      } else if ($7 == "checked") {
        checked[key]++
        if (checked[key] > 1) fail("duplicate check for one cache key")
        checked_by_kind[$6]++
      } else {
        check_hits[key]++
        check_hits_by_kind[$6]++
      }
    }
    END {
      if (NR == 0) fail("empty trace")
      for (owner in owner_kind) {
        if (owner_rows[owner] != 1) fail("cache owner must have one declaration row")
        owner_counts[owner_kind[owner]]++
      }
      for (key in observed) {
        if (prepared[key] != 1) fail("observed key must be prepared once in its owner")
        if (checked[key] != 1) fail("observed key must be checked once in its owner")
      }
      for (identity in owner_for_full_key) {
        split(identity, parts, SUBSEP)
        full_identity = parts[1] SUBSEP parts[2] SUBSEP parts[3] SUBSEP parts[4] SUBSEP parts[5] SUBSEP parts[6]
        full_key_owner_count[full_identity]++
      }
      for (full_identity in full_key_owner_count) {
        if (full_key_owner_count[full_identity] > 1) repeated_full_keys++
      }
      for (kind in owner_counts) {
        print kind " caches=" owner_counts[kind] \
          " records=" prepared_by_kind[kind] + 0 \
          " cache_reuses=" reused_by_kind[kind] + 0 \
          " checks=" checked_by_kind[kind] + 0 \
          " check_hits=" check_hits_by_kind[kind] + 0
      }
      print "identical full keys across cache owners=" repeated_full_keys + 0
      if (failed) exit 1
    }
  ' "$trace_path"
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
    validate_trace "$output_dir/$project_name-$command_name-traced.tsv" \
      | tee -a "$output_dir/summary.txt"
  done
done

printf 'W9 cache measurement complete: %s\n' "$output_dir"
