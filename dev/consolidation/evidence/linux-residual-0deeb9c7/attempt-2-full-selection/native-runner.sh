#!/bin/sh
set -u
: "${XSH_BIN:?set XSH_BIN to the pinned xsh binary}"
: "${XSHT_BIN:?set XSHT_BIN to the pinned xsht binary}"
: "${SELECTORS:?set SELECTORS to the exact selector list}"
: "${EXPECTED_COUNT:?set EXPECTED_COUNT to the exact selected count}"
: "${RESULTS:?set RESULTS to the raw result log path}"
: "${CURRENT:?set CURRENT to the current test output path}"
: "${TMPDIR:?set TMPDIR to the short owned scratch path}"
: "${COMMON_ROOT:?set COMMON_ROOT to the host repository root}"
: "${PRODUCT_XSH_SHA256:?set PRODUCT_XSH_SHA256 from the canonical qualification}"
: "${PRODUCT_XSHT_SHA256:?set PRODUCT_XSHT_SHA256 from the canonical qualification}"
: "${SOURCE_COMMIT:?set SOURCE_COMMIT to the frozen source commit}"
: "${SOURCE_TREE:?set SOURCE_TREE to the frozen source tree}"
test -x "$XSH_BIN" || exit 2
test -x "$XSHT_BIN" || exit 2
test -r "$SELECTORS" || exit 2
test -d "$TMPDIR" || exit 2
actual_xsh=$(sha256sum "$XSH_BIN" | awk '{print $1}') || exit 2
actual_xsht=$(sha256sum "$XSHT_BIN" | awk '{print $1}') || exit 2
printf 'qualified_product_xsh_sha256=%s actual=%s\n' "$PRODUCT_XSH_SHA256" "$actual_xsh"
printf 'qualified_product_xsht_sha256=%s actual=%s\n' "$PRODUCT_XSHT_SHA256" "$actual_xsht"
if [ "$actual_xsh" != "$PRODUCT_XSH_SHA256" ] || [ "$actual_xsht" != "$PRODUCT_XSHT_SHA256" ]; then
  printf 'qualified product hash mismatch inside container\n'
  exit 2
fi
git_common_dir=$(git -C /repo rev-parse --path-format=absolute --git-common-dir) || exit 2
source_commit=$(git -C /repo rev-parse HEAD) || exit 2
source_tree=$(git -C /repo rev-parse 'HEAD^{tree}') || exit 2
source_status=$(git -C /repo status --porcelain) || exit 2
printf 'source_commit=%s source_tree=%s source_status=%s git_common_dir=%s COMMON_ROOT=%s/.git\n' "$source_commit" "$source_tree" "${source_status:-clean}" "$git_common_dir" "$COMMON_ROOT"
if [ "$git_common_dir" != "$COMMON_ROOT/.git" ] || [ "$source_commit" != "$SOURCE_COMMIT" ] || [ "$source_tree" != "$SOURCE_TREE" ] || [ -n "$source_status" ]; then
  printf 'container source or Git common directory guard failed\n'
  exit 2
fi
export CARGO_BIN_EXE_xsh="$XSH_BIN"
export CARGO_BIN_EXE_xsht="$XSHT_BIN"
: > "$RESULTS"
printf 'identity='
id
awk '/^CapEff:|^CapBnd:/ { print }' /proc/self/status
printf 'TMPDIR=%s\n' "$TMPDIR"
count=0
passed=0
failed_selectors=0
actual_passed=0
actual_failed=0
skipped=0
unclassified=0
while IFS= read -r selector; do
  if [ -z "$selector" ]; then
    continue
  fi
  count=$((count + 1))
  printf '\n=== %s ===\n' "$selector" >> "$RESULTS"
  printf 'argv=%s test --jobs 1 --exact %s (default per-test timeout: 120s)\n' "$XSHT_BIN" "$selector" >> "$RESULTS"
  "$XSHT_BIN" test --jobs 1 --exact "$selector" > "$CURRENT" 2>&1
  status=$?
  cat "$CURRENT" >> "$RESULTS"
  printf 'runner_exit=%s\n' "$status" >> "$RESULTS"
  summary=$(grep '^test result:' "$CURRENT" | tail -n 1)
  one_passed=$(printf '%s\n' "$summary" | sed -n 's/^test result: .* \([0-9][0-9]*\) passed; [0-9][0-9]* failed; [0-9][0-9]* skipped$/\1/p')
  one_failed=$(printf '%s\n' "$summary" | sed -n 's/^test result: .* [0-9][0-9]* passed; \([0-9][0-9]*\) failed; [0-9][0-9]* skipped$/\1/p')
  one_skipped=$(printf '%s\n' "$summary" | sed -n 's/^test result: .* [0-9][0-9]* passed; [0-9][0-9]* failed; \([0-9][0-9]*\) skipped$/\1/p')
  if [ -z "$one_passed" ] || [ -z "$one_failed" ] || [ -z "$one_skipped" ]; then
    unclassified=$((unclassified + 1))
  else
    actual_passed=$((actual_passed + one_passed))
    actual_failed=$((actual_failed + one_failed))
    skipped=$((skipped + one_skipped))
  fi
  printf 'selector_summary=%s passed=%s failed=%s skipped=%s\n' "$summary" "${one_passed:-unknown}" "${one_failed:-unknown}" "${one_skipped:-unknown}" >> "$RESULTS"
  if [ "$status" -eq 0 ] && grep -Fq 'running 1 tests' "$CURRENT" && [ "$one_passed" = 1 ] && [ "$one_failed" = 0 ] && [ "$one_skipped" = 0 ]; then
    passed=$((passed + 1))
  else
    failed_selectors=$((failed_selectors + 1))
  fi
done < "$SELECTORS"
printf '\nselected=%s accepted_passes=%s failed_selectors=%s actual_passed_tests=%s actual_failed_tests=%s actual_skipped_tests=%s unclassified=%s\n' "$count" "$passed" "$failed_selectors" "$actual_passed" "$actual_failed" "$skipped" "$unclassified" >> "$RESULTS"
cat "$RESULTS"
if [ "$count" -ne "$EXPECTED_COUNT" ] || [ "$passed" -ne "$EXPECTED_COUNT" ] || [ "$failed_selectors" -ne 0 ] || [ "$actual_failed" -ne 0 ] || [ "$skipped" -ne 0 ] || [ "$unclassified" -ne 0 ]; then
  exit 1
fi
