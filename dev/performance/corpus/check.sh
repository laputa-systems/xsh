#!/usr/bin/env bash
set -Eeuo pipefail

if (($# != 3)); then
  printf 'usage: %s XSHT XSH_ROOT LAPUTA_ROOT\n' "$0" >&2
  exit 2
fi

xsht=$1
xsh_root=$(cd "$2" && pwd)
laputa_root=$(cd "$3" && pwd)
repo=$(git rev-parse --show-toplevel)
scratch=$repo/.work/tmp/performance-corpus
mkdir -p "$scratch"
export TMPDIR=$scratch

for path in "$xsh_root/core/wc.xsh" "$xsh_root/core/sort.xsh" \
  "$xsh_root/core/awk.xsh" "$laputa_root/pm/plan_json.xsh" \
  "$laputa_root/pm/generation.xsh" "$laputa_root/pm/generation_adapter.xsh" \
  "$laputa_root/tests/pm/pm_generation.xsh"; do
  [[ -f $path ]]
done

(
  cd "$xsh_root"
  "$xsht" check \
    core/wc.xsh core/sort.xsh core/awk.xsh \
    core/cut.xsh core/tr.xsh core/date.xsh core/lib/date_parse.xsh \
    core/system-report.xsh dev/gates.xsh dev/internal.xsh dev/docker.xsh dev/main.xsh
  "$xsht" test --exact core/tests/test-wc.xsh::test_wc_characters_and_invalid_bytes
  "$xsht" test --exact core/tests/test-sort.xsh::test_sort_general_numeric_orders_past_double_precision
  "$xsht" test --exact core/tests/test-awk.xsh::test_awk_arrays_functions_and_loops
)

(
  cd "$laputa_root"
  "$xsht" check pm/plan_json.xsh pm/generation.xsh pm/generation_adapter.xsh
  "$xsht" test --exact tests/pm/pm_generation.xsh::test_generation_plan_and_receipt_are_deterministic_for_root_order_and_duplicates
  "$xsht" test --exact tests/pm/pm_generation.xsh::test_generation_plan_json_round_trips_and_rejects_changed_identity
)
