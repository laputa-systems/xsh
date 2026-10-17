#!/usr/bin/env bash
set -Eeuo pipefail

if (($# != 8)); then
  printf 'usage: %s XSH_SOURCE LAPUTA_SOURCE XSH XSHT XSH_SHA256 XSHT_SHA256 XSH_ROOT LAPUTA_ROOT\n' "$0" >&2
  exit 2
fi

expected_xsh_source=$1
expected_laputa_source=$2
xsh=$3
xsht=$4
expected_xsh_sha256=$5
expected_xsht_sha256=$6
xsh_root=$(cd "$7" && pwd)
laputa_root=$(cd "$8" && pwd)
repo=$(git rev-parse --show-toplevel)
scratch=$repo/.work/tmp/performance-corpus
mkdir -p "$scratch"
export TMPDIR=$scratch

for product in "$xsh" "$xsht"; do
  [[ -f $product && -x $product ]]
done

[[ $(git -C "$xsh_root" rev-parse --verify HEAD^{commit}) = "$expected_xsh_source" ]]
[[ $(git -C "$laputa_root" rev-parse --verify HEAD^{commit}) = "$expected_laputa_source" ]]
[[ -z $(git -C "$xsh_root" status --porcelain) ]]
[[ -z $(git -C "$laputa_root" status --porcelain) ]]
[[ $(sha256sum "$xsh" | cut -d ' ' -f 1) = "$expected_xsh_sha256" ]]
[[ $(sha256sum "$xsht" | cut -d ' ' -f 1) = "$expected_xsht_sha256" ]]

for path in \
  "$xsh_root/core/wc.xsh" "$xsh_root/core/sort.xsh" "$xsh_root/core/awk.xsh" \
  "$xsh_root/core/tests/test-wc.xsh" "$xsh_root/core/tests/test-sort.xsh" \
  "$xsh_root/core/tests/test-awk.xsh" "$xsh_root/core/cut.xsh" \
  "$xsh_root/core/tr.xsh" "$xsh_root/core/date.xsh" \
  "$xsh_root/core/lib/date_parse.xsh" "$xsh_root/core/system-report.xsh" \
  "$xsh_root/dev/gates.xsh" "$xsh_root/dev/internal.xsh" \
  "$xsh_root/dev/docker.xsh" "$xsh_root/dev/main.xsh" \
  "$laputa_root/pm/plan_json.xsh" "$laputa_root/pm/generation.xsh" \
  "$laputa_root/pm/generation_adapter.xsh" "$laputa_root/tests/pm/pm_generation.xsh"; do
  [[ -f $path ]]
done

run_exact() {
  selector=$1
  name=${selector//[^[:alnum:]]/_}
  output=$scratch/$name.stdout
  error=$scratch/$name.stderr
  if ! "$xsht" test --exact "$selector" >"$output" 2>"$error"; then
    cat "$output" "$error" >&2
    return 1
  fi
  matches=$(awk -v prefix="$selector ... ok " '
    index($0, prefix) == 1 && substr($0, length(prefix) + 1) ~ /^[^[:space:]]+s$/ { count++ }
    END { print count + 0 }
  ' "$output")
  if [[ $(grep -c '^running 1 tests$' "$output") != 1 ]] || \
    [[ $matches != 1 ]] || \
    [[ $(grep -c '^test result: ok\. 1 passed; 0 failed; 0 skipped$' "$output") != 1 ]]; then
    cat "$output" "$error" >&2
    return 1
  fi
}

run_check() {
  expected_files=$1
  shift
  output=$scratch/check.$expected_files.stdout
  error=$scratch/check.$expected_files.stderr
  if ! "$xsht" check "$@" >"$output" 2>"$error"; then
    cat "$output" "$error" >&2
    return 1
  fi
  if ! grep -Eq "^xsht check: $expected_files files? .*check [^,]+, lower " "$error"; then
    cat "$output" "$error" >&2
    return 1
  fi
}

(
  cd "$xsh_root"
  export XSH_BIN=$xsh XSHT_BIN=$xsht
  export PATH="$(dirname "$xsh"):$(dirname "$xsht"):$PATH"
  run_check 12 \
    core/wc.xsh core/sort.xsh core/awk.xsh \
    core/cut.xsh core/tr.xsh core/date.xsh core/lib/date_parse.xsh \
    core/system-report.xsh dev/gates.xsh dev/internal.xsh dev/docker.xsh dev/main.xsh
  run_exact core/tests/test-wc.xsh::test_wc_characters_and_invalid_bytes
  run_exact core/tests/test-sort.xsh::test_sort_general_numeric_orders_past_double_precision
  run_exact core/tests/test-awk.xsh::test_awk_arrays_functions_and_loops
)

(
  cd "$laputa_root"
  export XSH_BIN=$xsh XSHT_BIN=$xsht
  export PATH="$(dirname "$xsh"):$(dirname "$xsht"):$PATH"
  run_check 3 pm/plan_json.xsh pm/generation.xsh pm/generation_adapter.xsh
  run_exact tests/pm/pm_generation.xsh::test_generation_plan_and_receipt_are_deterministic_for_root_order_and_duplicates
  run_exact tests/pm/pm_generation.xsh::test_generation_plan_json_round_trips_and_rejects_changed_identity
)
