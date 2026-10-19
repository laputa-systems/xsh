#!/usr/bin/env bash
set -Eeuo pipefail

run_check() {
  local view=$1
  local project=$2
  local expected_files=$3
  shift 3
  local root=$views/$view/$project
  local output=$scratch/check.$view.$project.stdout
  local error=$scratch/check.$view.$project.stderr
  if ! (cd "$root" && "$xsht" check "$@") >"$output" 2>"$error"; then
    cat "$output" "$error" >&2
    return 1
  fi
  if ! grep -Eq "^xsht check: $expected_files files? .*check [^,]+, lower " "$error"; then
    cat "$output" "$error" >&2
    return 1
  fi
}

run_exact() {
  local view=$1
  local project=$2
  local selector=$3
  local name=${4:-${selector//[^[:alnum:]]/_}}
  local root=$views/$view/$project
  local output=$scratch/test.$view.$name.stdout
  local error=$scratch/test.$view.$name.stderr
  if ! (cd "$root" && "$xsht" test --exact "$selector") >"$output" 2>"$error"; then
    cat "$output" "$error" >&2
    return 1
  fi
  local matches
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

frozen_main() {
  if (($# < 13)); then
    printf 'usage: %s --frozen COMPILER_SOURCE COMPILER_REV XSH_SOURCE XSH_REV LAPUTA_SOURCE LAPUTA_REV XSH XSHT XSH_SHA256 XSHT_SHA256 RETAINED VIEWS [--selector PROJECT SELECTOR]...\n' "$0" >&2
    return 2
  fi
  compiler_source=$(cd "$1" && pwd)
  compiler_revision=$2
  xsh_source=$(cd "$3" && pwd)
  xsh_revision=$4
  laputa_source=$(cd "$5" && pwd)
  laputa_revision=$6
  xsh=$7
  xsht=$8
  expected_xsh_sha256=$9
  expected_xsht_sha256=${10}
  retained=$(python3 -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).resolve())' "${11}")
  views=$(python3 -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).resolve())' "${12}")
  shift 12

  local -a selector_projects=()
  local -a selectors=()
  while (($#)); do
    if (($# < 3)) || [[ $1 != --selector ]]; then
      printf 'expected repeated --selector PROJECT SELECTOR arguments\n' >&2
      return 2
    fi
    case $2 in
      xsh|laputa) selector_projects+=("$2") ;;
      *) printf 'unknown selector project: %s\n' "$2" >&2; return 2 ;;
    esac
    [[ $3 == *::* ]] || { printf 'selector must name a test: %s\n' "$3" >&2; return 2; }
    for existing in "${selectors[@]}"; do
      [[ $existing != "$3" ]] || { printf 'duplicate exact selector: %s\n' "$3" >&2; return 2; }
    done
    selectors+=("$3")
    shift 3
  done
  if ((${#selectors[@]} == 0)); then
    printf 'at least one exact paired selector is required\n' >&2
    return 2
  fi

  repo=$(git rev-parse --show-toplevel)
  exclude=$(git -C "$repo" rev-parse --git-path info/exclude)
  if ! awk '$0 == ".work/" || $0 == "/.work/" { found = 1 } END { exit !found }' "$exclude" || \
    ! git -C "$repo" check-ignore -q --no-index .work/tmp/performance-corpus-frozen; then
    printf '.work/ must be ignored through %s before the frozen paired check creates scratch\n' "$exclude" >&2
    return 1
  fi
  case "$retained" in
    "$repo"/.work/tmp/*) ;;
    *) printf 'retained table must be under %s/.work/tmp\n' "$repo" >&2; return 1 ;;
  esac
  case "$views" in
    "$repo"/.work/tmp/*) ;;
    *) printf 'views must be under %s/.work/tmp\n' "$repo" >&2; return 1 ;;
  esac
  [[ -f $retained ]]
  [[ -d $views/original/xsh && -d $views/original/laputa ]]
  [[ -d $views/reduced/xsh && -d $views/reduced/laputa ]]

  verify_checkout() {
    local root=$1
    local revision=$2
    [[ $(git -C "$root" rev-parse --show-toplevel) == "$root" ]]
    [[ $(git -C "$root" rev-parse --verify HEAD^{commit}) == "$revision" ]]
    [[ -z $(git -C "$root" status --porcelain) ]]
  }
  verify_source() {
    verify_checkout "$compiler_source" "$compiler_revision"
    verify_checkout "$xsh_source" "$xsh_revision"
    verify_checkout "$laputa_source" "$laputa_revision"
  }
  verify_products() {
    for product in "$xsh" "$xsht"; do
      [[ -f $product && -x $product ]]
      case "$product" in
        "$compiler_source"/.work/*) ;;
        *) printf 'product must be under the pinned compiler checkout .work/: %s\n' "$product" >&2; return 1 ;;
      esac
    done
    [[ $(sha256sum "$xsh" | cut -d ' ' -f 1) == "$expected_xsh_sha256" ]]
    [[ $(sha256sum "$xsht" | cut -d ' ' -f 1) == "$expected_xsht_sha256" ]]
  }
  verify_source
  verify_products

  scratch=$(mktemp -d "$repo/.work/tmp/performance-corpus-frozen.XXXXXXXX")
  cleanup_frozen() {
    local status=$?
    trap - EXIT INT TERM
    verify_source || status=1
    verify_products || status=1
    rm -rf "$scratch" || status=1
    exit "$status"
  }
  trap cleanup_frozen EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  mkdir -p "$scratch/archive/original/xsh" "$scratch/archive/original/laputa"
  git -C "$xsh_source" archive "$xsh_revision" | tar -x -C "$scratch/archive/original/xsh"
  git -C "$laputa_source" archive "$laputa_revision" | tar -x -C "$scratch/archive/original/laputa"
  local frozen_retained=$scratch/retained-1040.tsv
  git -C "$repo" show 15243e97f6fd751016ab28690ada2cadb6fc973a:dev/consolidation/typing-corpus/retained.tsv >"$frozen_retained"
  if ! python3 "$repo/dev/consolidation/typing-corpus/cohort.py" \
    "$scratch/archive/original" --reduced "$scratch/archive/reduced-1040" \
    --retain "$frozen_retained" >"$scratch/accepted-cohort.log" 2>&1; then
    cat "$scratch/accepted-cohort.log" >&2
    return 1
  fi
  if ! python3 "$repo/dev/consolidation/typing-corpus/apply_witness_fixture.py" \
    "$scratch/archive/original/xsh" "$scratch/archive/reduced-1040/xsh" \
    >"$scratch/fixture.log" 2>&1; then
    cat "$scratch/fixture.log" >&2
    return 1
  fi
  if ! python3 "$repo/dev/consolidation/typing-corpus/validate.py" \
    "$scratch/archive/original" "$scratch/archive/reduced-1040" \
    --retain "$frozen_retained" >"$scratch/accepted-validation.log" 2>&1; then
    cat "$scratch/accepted-validation.log" >&2
    return 1
  fi
  local witness_rows=$scratch/frozen-witnesses.tsv
  PYTHONDONTWRITEBYTECODE=1 python3 "$repo/dev/performance/corpus/frozen_witnesses.py" \
    "$scratch/archive" >"$witness_rows"
  python3 - "$frozen_retained" "$retained" "$witness_rows" <<'PY'
import csv
import sys

def site_rows(path):
    with open(path, newline='') as source:
        rows = list(csv.reader(source, delimiter='\t'))
    if not rows or rows[0] != ['site', 'reason']:
        raise SystemExit(f'invalid retained table header: {path}')
    values = {}
    for row in rows[1:]:
        if len(row) != 2 or not row[0].isdecimal() or not row[1]:
            raise SystemExit(f'invalid retained row in {path}: {row}')
        site = int(row[0])
        if site in values:
            raise SystemExit(f'duplicate retained site {site} in {path}')
        values[site] = row
    return values

accepted_rows = site_rows(sys.argv[1])
candidate_rows = site_rows(sys.argv[2])
accepted = set(accepted_rows)
candidate = set(candidate_rows)
with open(sys.argv[3], newline='') as source:
    rows = csv.DictReader(source, delimiter='\t')
    proc = {
        int(row['site'])
        for row in rows
        if row['classification'] in {'proc-parameter-contract', 'proc-return-contract'}
    }
removed = accepted - candidate
if candidate - accepted or removed - proc or any(
    candidate_rows[site] != accepted_rows[site] for site in candidate
):
    raise SystemExit('candidate must preserve the frozen retained set except selected Proc contracts')
PY

  mkdir -p "$scratch/expected"
  if ! python3 "$repo/dev/consolidation/typing-corpus/cohort.py" \
    "$scratch/archive/original" --reduced "$scratch/expected/reduced" \
    --retain "$retained" >"$scratch/candidate-cohort.log" 2>&1; then
    cat "$scratch/candidate-cohort.log" >&2
    return 1
  fi
  if ! python3 "$repo/dev/consolidation/typing-corpus/validate.py" \
    "$scratch/archive/original" "$scratch/expected/reduced" \
    --retain "$retained" >"$scratch/candidate-validation.log" 2>&1; then
    cat "$scratch/candidate-validation.log" >&2
    return 1
  fi
  for project in xsh laputa; do
    python3 "$repo/dev/performance/corpus/compare_tree.py" \
      "$scratch/archive/original/$project" "$views/original/$project"
    python3 "$repo/dev/performance/corpus/compare_tree.py" \
      "$scratch/expected/reduced/$project" "$views/reduced/$project"
  done

  export XSH_BIN=$xsh XSHT_BIN=$xsht
  export PATH="$(dirname "$xsh"):$(dirname "$xsht"):$PATH"
  local -a xsh_roots=(
    core/wc.xsh core/cut.xsh core/tr.xsh core/date.xsh core/lib/date_parse.xsh
    core/system-report.xsh dev/gates.xsh dev/internal.xsh dev/docker.xsh dev/main.xsh
  )
  for view in original reduced; do
    run_check "$view" xsh 10 "${xsh_roots[@]}"
    run_check "$view" laputa 1 pm/plan_json.xsh
    for ((index = 0; index < ${#selectors[@]}; index++)); do
      local project=${selector_projects[$index]}
      local selector=${selectors[$index]}
      local test_path=${selector%%::*}
      case "$test_path" in
        /*|../*|*/../*|*/..|.|./*) printf 'selector path escapes its source root: %s\n' "$selector" >&2; return 1 ;;
      esac
      [[ -f $views/$view/$project/$test_path ]]
      run_exact "$view" "$project" "$selector" "$index"
    done
  done

  for project in xsh laputa; do
    python3 "$repo/dev/performance/corpus/compare_tree.py" \
      "$scratch/archive/original/$project" "$views/original/$project"
    python3 "$repo/dev/performance/corpus/compare_tree.py" \
      "$scratch/expected/reduced/$project" "$views/reduced/$project"
  done
  verify_source
  verify_products
  printf 'paired frozen corpus: 11 roots and %s exact selectors passed in both views\n' "${#selectors[@]}"
}

if [[ ${1:-} == --frozen ]]; then
  shift
  frozen_main "$@"
  exit $?
fi

if (($# != 13)); then
  printf 'usage: %s COMPILER_SOURCE COMPILER_REV XSH_SOURCE XSH_REV LAPUTA_SOURCE LAPUTA_REV XSH XSHT XSH_SHA256 XSHT_SHA256 INVENTORY SITE_IDS VIEWS\n' "$0" >&2
  exit 2
fi

compiler_source=$(cd "$1" && pwd)
compiler_revision=$2
xsh_source=$(cd "$3" && pwd)
xsh_revision=$4
laputa_source=$(cd "$5" && pwd)
laputa_revision=$6
xsh=$7
xsht=$8
expected_xsh_sha256=$9
expected_xsht_sha256=${10}
inventory=$(python3 -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).resolve())' "${11}")
site_ids=${12}
views=$(python3 -c 'from pathlib import Path; import sys; print(Path(sys.argv[1]).resolve())' "${13}")
repo=$(git rev-parse --show-toplevel)
exclude=$(git -C "$repo" rev-parse --git-path info/exclude)

if ! awk '$0 == ".work/" || $0 == "/.work/" { found = 1 } END { exit !found }' "$exclude" || \
  ! git -C "$repo" check-ignore -q --no-index .work/tmp/performance-corpus-paired; then
  printf '.work/ must be ignored through %s before the paired check creates scratch\n' "$exclude" >&2
  exit 1
fi

verify_checkout() {
  local root=$1
  local revision=$2
  [[ $(git -C "$root" rev-parse --show-toplevel) == "$root" ]]
  [[ $(git -C "$root" rev-parse --verify HEAD^{commit}) == "$revision" ]]
  [[ -z $(git -C "$root" status --porcelain) ]]
}

verify_source() {
  verify_checkout "$compiler_source" "$compiler_revision"
  verify_checkout "$xsh_source" "$xsh_revision"
  verify_checkout "$laputa_source" "$laputa_revision"
}

verify_products() {
  for product in "$xsh" "$xsht"; do
    [[ -f $product && -x $product ]]
    case "$product" in
      "$compiler_source"/.work/*) ;;
      *) printf 'product must be under the pinned compiler checkout .work/: %s\n' "$product" >&2; return 1 ;;
    esac
  done
  [[ $(sha256sum "$xsh" | cut -d ' ' -f 1) == "$expected_xsh_sha256" ]]
  [[ $(sha256sum "$xsht" | cut -d ' ' -f 1) == "$expected_xsht_sha256" ]]
}

case "$inventory" in
  "$repo"/.work/tmp/*) ;;
  *) printf 'inventory must be under %s/.work/tmp\n' "$repo" >&2; exit 1 ;;
esac
case "$views" in
  "$repo"/.work/tmp/*) ;;
  *) printf 'views must be under %s/.work/tmp\n' "$repo" >&2; exit 1 ;;
esac
[[ -f $inventory ]]
[[ -d $views/original/xsh && -d $views/original/laputa ]]
[[ -d $views/reduced/xsh && -d $views/reduced/laputa ]]
if [[ $site_ids =~ (^|,)[0-9]+(,|$) ]]; then
  printf 'paired views require exact source selectors, not inventory row numbers\n' >&2
  exit 1
fi
if [[ $site_ids == *'xsh|core/wc.xsh|parameter|94|pure continuation parameter data'* || \
      $site_ids == *'xsh|core/wc.xsh|parameter|99|pure within parameter data'* || \
      $site_ids == *'xsh|core/wc.xsh|parameter|105|pure sequence_width parameter data'* || \
      $site_ids == *'xsh|core/wc.xsh|parameter|214|pure byte_words parameter data'* ]]; then
  printf 'paired removal set includes a reserved wc diagnostic control\n' >&2
  exit 1
fi
verify_source
verify_products

mkdir -p "$repo/.work/tmp"
scratch=$(mktemp -d "$repo/.work/tmp/performance-corpus-paired.XXXXXXXX")
restore_probes() {
  if [[ ! -f $scratch/probe-restore-required ]]; then
    return
  fi
  for view in original reduced; do
    for path in core/wc.xsh core/sort.xsh core/awk.xsh; do
      if [[ -f $scratch/expected/$view/xsh/$path ]]; then
        cp "$scratch/expected/$view/xsh/$path" "$views/$view/xsh/$path"
      fi
    done
    if [[ -f $scratch/expected/$view/laputa/pm/generation.xsh ]]; then
      cp "$scratch/expected/$view/laputa/pm/generation.xsh" "$views/$view/laputa/pm/generation.xsh"
    fi
  done
}
cleanup() {
  restore_probes
  rm -rf "$scratch"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -p "$scratch/expected/original/xsh" "$scratch/expected/original/laputa" "$scratch/expected/reduced/xsh" "$scratch/expected/reduced/laputa" "$scratch/tmp"
export TMPDIR=$scratch/tmp
git -C "$xsh_source" archive "$xsh_revision" | tar -x -C "$scratch/expected/original/xsh"
git -C "$laputa_source" archive "$laputa_revision" | tar -x -C "$scratch/expected/original/laputa"
cp -a "$scratch/expected/original/xsh/." "$scratch/expected/reduced/xsh/"
cp -a "$scratch/expected/original/laputa/." "$scratch/expected/reduced/laputa/"
python3 "$repo/dev/performance/corpus/apply_spans.py" \
  "$inventory" "$site_ids" "$xsh_revision" "$laputa_revision" \
  "$scratch/expected/reduced/xsh" "$scratch/expected/reduced/laputa"
for view in original reduced; do
  for project in xsh laputa; do
    if ! python3 "$repo/dev/performance/corpus/compare_tree.py" \
      "$scratch/expected/$view/$project" "$views/$view/$project"; then
      printf '%s %s view differs from its pinned source and selected spans\n' "$view" "$project" >&2
      exit 1
    fi
  done
done

export XSH_BIN=$xsh XSHT_BIN=$xsht
export PATH="$(dirname "$xsh"):$(dirname "$xsht"):$PATH"

run_negative_control() {
  local selector=$1
  local name=$2
  local root=$scratch/negative/$name/xsh
  local laputa_root=$scratch/negative/$name/laputa
  local output=$scratch/negative.$name.stdout
  local error=$scratch/negative.$name.stderr
  local status=0
  mkdir -p "$root" "$laputa_root"
  cp -a "$scratch/expected/original/xsh/." "$root/"
  python3 "$repo/dev/performance/corpus/apply_spans.py" \
    "$inventory" "$selector" "$xsh_revision" "$laputa_revision" "$root" "$laputa_root"
  if (cd "$root" && "$xsht" check core/wc.xsh) >"$output" 2>"$error"; then
    printf '%s unexpectedly passed after its data parameter annotation was removed\n' "$name" >&2
    return 1
  else
    status=$?
  fi
  local expected_codes=check.ambiguous-overload,check.result-fallback
  local codes
  codes=$(sed -nE 's/^err\[([^]]+)\].*/\1/p' "$error" | sort -u | paste -sd, -)
  if [[ $status != 2 ]] || ! grep -q 'core/wc.xsh' "$error" || [[ $codes != "$expected_codes" ]]; then
    cat "$output" "$error" >&2
    printf '%s emitted diagnostic codes %s; expected %s in core/wc.xsh\n' \
      "$name" "${codes:-none}" "$expected_codes" >&2
    return 1
  fi
  printf '%s: %s\n' "$name" "$codes"
}

for view in original reduced; do
  run_check "$view" xsh 12 \
    core/wc.xsh core/sort.xsh core/awk.xsh \
    core/cut.xsh core/tr.xsh core/date.xsh core/lib/date_parse.xsh \
    core/system-report.xsh dev/gates.xsh dev/internal.xsh dev/docker.xsh dev/main.xsh
  run_check "$view" laputa 3 pm/plan_json.xsh pm/generation.xsh pm/generation_adapter.xsh
  run_exact "$view" xsh core/tests/test-wc.xsh::test_wc_characters_and_invalid_bytes
  run_exact "$view" xsh core/tests/test-sort.xsh::test_sort_general_numeric_orders_past_double_precision
  run_exact "$view" xsh core/tests/test-awk.xsh::test_awk_arrays_functions_and_loops
  run_exact "$view" laputa tests/pm/pm_generation.xsh::test_generation_plan_and_receipt_are_deterministic_for_root_order_and_duplicates
  run_exact "$view" laputa tests/pm/pm_generation.xsh::test_generation_plan_json_round_trips_and_rejects_changed_identity
done

run_negative_control 'xsh|core/wc.xsh|parameter|94|pure continuation parameter data' continuation_data
run_negative_control 'xsh|core/wc.xsh|parameter|99|pure within parameter data' within_data
run_negative_control 'xsh|core/wc.xsh|parameter|214|pure byte_words parameter data' byte_words_data

touch "$scratch/probe-restore-required"
python3 "$repo/dev/performance/corpus/append_probes.py" \
  "$inventory" "$xsh_revision" "$laputa_revision" \
  "$views/original/xsh" "$views/original/laputa"
python3 "$repo/dev/performance/corpus/append_probes.py" \
  "$inventory" "$xsh_revision" "$laputa_revision" \
  "$views/reduced/xsh" "$views/reduced/laputa"

for view in original reduced; do
  for project in xsh laputa; do
    if [[ $project == xsh ]]; then
      run_check "$view" xsh 3 core/wc.xsh core/sort.xsh core/awk.xsh
      expected_notes=4
    else
      run_check "$view" laputa 1 pm/generation.xsh
      expected_notes=2
    fi
    grep 'revealed type:' "$scratch/check.$view.$project.stderr" >"$scratch/signatures.$view.$project"
    if [[ $(wc -l <"$scratch/signatures.$view.$project") != "$expected_notes" ]]; then
      cat "$scratch/check.$view.$project.stdout" "$scratch/check.$view.$project.stderr" >&2
      printf '%s %s view emitted an unexpected reveal_type note count\n' "$view" "$project" >&2
      exit 1
    fi
  done
done
cmp "$scratch/signatures.original.xsh" "$scratch/signatures.reduced.xsh"
cmp "$scratch/signatures.original.laputa" "$scratch/signatures.reduced.laputa"
restore_probes
rm -f "$scratch/probe-restore-required"
for view in original reduced; do
  for project in xsh laputa; do
    if ! python3 "$repo/dev/performance/corpus/compare_tree.py" \
      "$scratch/expected/$view/$project" "$views/$view/$project"; then
      printf '%s %s view changed during paired checking\n' "$view" "$project" >&2
      exit 1
    fi
  done
done

verify_source
verify_products
printf 'paired practical corpus: retained diagnostics, five runtime selectors, and six signatures passed\n'
