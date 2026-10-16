#!/usr/bin/env bash
set -euo pipefail
umask 077

ROOT=$(git rev-parse --show-toplevel)
if (($# != 5)); then
  printf 'usage: %s MAIN_HEAD MAIN_TREE CANDIDATE_ROOT ATTEMPT prepare|capture|assignment|compare\n' "$0" >&2
  exit 2
fi
MAIN_HEAD=$1
MAIN_TREE=$2
CANDIDATE_ROOT=$(realpath "$3")
ATTEMPT=$4
PHASE=$5
case "$ATTEMPT" in
  ''|*[!A-Za-z0-9._-]*) printf 'invalid attempt name: %s\n' "$ATTEMPT" >&2; exit 2 ;;
esac
case "$PHASE" in
  prepare|capture|assignment|compare) ;;
  *) printf 'invalid phase: %s\n' "$PHASE" >&2; exit 2 ;;
esac

REFERENCE_ROOT="$ROOT/.work/wt/w9-adapter-ref-e240"
PROGRAM_ROOT="$ROOT/.work/wt/w9-program-e240"
DERIVED_ROOT="$ROOT/.work/wt/w9-derived-api-imports"
INPUT_ROOT="$ROOT/.work/tmp/w9-corpus-recovery/input-corpus"
INPUT_MANIFEST="$ROOT/.work/tmp/w9-corpus-recovery/input-files.sha256.tsv"
SOURCE_RECEIPT="$CANDIDATE_ROOT/.work/consolidation/source-cohort-$ATTEMPT.receipt"
RUN_ROOT="$ROOT/.work/consolidation/$ATTEMPT"
CARGO_LOCK="$ROOT/.work/consolidation/infra/cargo.lock"
EXPECTED_MANIFEST_SHA256=1ec4e8eb67681591d25c69b395387ba4acbb4bfe1e4ab514a858e60ff1a56305
EXPECTED_E240_HEAD=e240efad273fe35d66d91f4037b6a8193527da2e
EXPECTED_E240_TREE=a1ef04ac7b6f759c272e8f841acee3a3b3a6ff08
EXPECTED_DERIVED_HEAD=74f1551ee9e3066b653ae190779e16d511c90293
EXPECTED_DERIVED_TREE=3162d2f33076a8c9261d0ffb7a89a9952662f03f
TMPDIR="$ROOT/.work/tmp"
export TMPDIR

fail() {
  printf '%s\n' "$*" >&2
  exit 2
}

assert_identity() {
  local path=$1 expected_head=$2 expected_tree=$3 label=$4 actual_head actual_tree status
  [[ -d $path && ! -L $path && $(realpath "$path") == "$path" ]] || fail "$label root is not canonical: $path"
  actual_head=$(git -C "$path" rev-parse HEAD)
  actual_tree=$(git -C "$path" rev-parse 'HEAD^{tree}')
  [[ $actual_head == "$expected_head" && $actual_tree == "$expected_tree" ]] || \
    fail "$label identity differs: $actual_head:$actual_tree"
  status=$(git -C "$path" status --porcelain=v1 --untracked-files=all)
  [[ -z $status ]] || fail "$label worktree is dirty: $path"
}

assert_inputs() {
  [[ -d $INPUT_ROOT && ! -L $INPUT_ROOT && $(realpath "$INPUT_ROOT") == "$INPUT_ROOT" ]] || fail "input root is not canonical"
  [[ -f $INPUT_MANIFEST && ! -L $INPUT_MANIFEST && $(realpath "$INPUT_MANIFEST") == "$INPUT_MANIFEST" ]] || fail "input manifest is not canonical"
  [[ $(sha256sum "$INPUT_MANIFEST" | awk '{print $1}') == "$EXPECTED_MANIFEST_SHA256" ]] || fail "input manifest hash differs"
  [[ $(wc -l < "$INPUT_MANIFEST" | tr -d ' ') == 1690 ]] || fail "input manifest does not contain 1690 rows"
  python3 "$ROOT/dev/consolidation/w9_program_snapshot.py" validate "$PROGRAM_ROOT"
}

assert_sources() {
  assert_identity "$ROOT" "$MAIN_HEAD" "$MAIN_TREE" controller
  assert_identity "$CANDIDATE_ROOT" "$MAIN_HEAD" "$MAIN_TREE" candidate
  assert_identity "$REFERENCE_ROOT" "$EXPECTED_E240_HEAD" "$EXPECTED_E240_TREE" reference
  assert_identity "$PROGRAM_ROOT" "$EXPECTED_E240_HEAD" "$EXPECTED_E240_TREE" program
  assert_identity "$DERIVED_ROOT" "$EXPECTED_DERIVED_HEAD" "$EXPECTED_DERIVED_TREE" derived-reference
  assert_inputs
}

snapshot_sources() {
  local destination=$1 path head tree
  [[ ! -e $destination && ! -L $destination ]] || fail "refusing to overwrite $destination"
  : > "$destination"
  for path in "$ROOT" "$CANDIDATE_ROOT" "$REFERENCE_ROOT" "$PROGRAM_ROOT" "$DERIVED_ROOT"; do
    head=$(git -C "$path" rev-parse HEAD)
    tree=$(git -C "$path" rev-parse 'HEAD^{tree}')
    printf '%s\t%s\t%s\n' "$path" "$head" "$tree" >> "$destination"
  done
}

run_logged() {
  local name=$1
  shift
  local log="$RUN_ROOT/$name.log"
  local command_file="$RUN_ROOT/$name.command"
  [[ ! -e $log && ! -L $log && ! -e $command_file && ! -L $command_file ]] || fail "refusing to overwrite $log"
  printf '%q ' "$@" > "$command_file"
  printf '\n' >> "$RUN_ROOT/$name.command"
  local status
  set +e
  "$@" > "$log" 2>&1
  status=$?
  set -e
  cat "$log"
  ((status == 0)) || fail "$name failed with status $status; output preserved in $log"
}

log_field() {
  local file=$1 key=$2
  awk -F '=' -v key="$key" '$1 == key { count++; value = substr($0, index($0, "=") + 1) } END { if (count != 1) exit 1; print value }' "$file"
}

receipt_field() {
  local file=$1 key=$2
  awk -F '\t' -v key="$key" '$1 == key { count++; value = substr($0, index($0, "\t") + 1) } END { if (count != 1) exit 1; print value }' "$file"
}

require_fresh_cargo_artifact() {
  local receipt=$1 label=$2 binary cargo_json rows
  binary=$(receipt_field "$receipt" binary) || fail "$label receipt has no unique binary field: $receipt"
  cargo_json="$binary.cargo.jsonl"
  [[ -f $cargo_json && ! -L $cargo_json ]] || fail "$label raw Cargo JSON is missing: $cargo_json"
  rows=$(jaq -r 'select(.reason == "compiler-artifact" and .target.name == "integration") | if .fresh == false then "false" else "not-fresh-false" end' "$cargo_json") || fail "$label Cargo JSON is invalid: $cargo_json"
  [[ -n $rows ]] || fail "$label Cargo JSON has no integration artifact row: $cargo_json"
  if grep -Fvxq 'false' <<< "$rows"; then
    printf '%s\n' "$rows" >&2
    fail "$label integration artifact is not freshly compiled; see $cargo_json"
  fi
  printf 'fresh=false %s binary=%s cargo_json=%s\n' "$label" "$binary" "$cargo_json"
}

assert_output_root() {
  local output=$1
  case "$output" in
    "$ROOT/.work/consolidation/"*) ;;
    *) fail "runtime output is outside controller .work/consolidation: $output" ;;
  esac
  [[ $output != "$CANDIDATE_ROOT"* && $output != "$REFERENCE_ROOT"* && $output != "$PROGRAM_ROOT"* && $output != "$DERIVED_ROOT"* && $output != "$INPUT_ROOT"* && $output != "$ROOT/.work/tmp/w9-container-scratch"* ]] || fail "runtime output overlaps a protected root: $output"
}

assert_sources
[[ $(realpath "$ROOT") == "$ROOT" ]] || fail "controller root is not canonical"
for writable_parent in "$ROOT/.work" "$ROOT/.work/consolidation" "$ROOT/.work/tmp"; do
  [[ -d $writable_parent && ! -L $writable_parent && $(realpath "$writable_parent") == "$writable_parent" ]] || fail "writable parent is not canonical: $writable_parent"
done
[[ -f $CARGO_LOCK && ! -L $CARGO_LOCK && $(realpath "$CARGO_LOCK") == "$CARGO_LOCK" ]] || fail "primary Cargo lock is missing or noncanonical"

if [[ $PHASE == prepare ]]; then
  [[ ! -e $RUN_ROOT && ! -L $RUN_ROOT ]] || fail "attempt output already exists: $RUN_ROOT"
  mkdir -m 700 "$RUN_ROOT"
  mkdir -p "$CANDIDATE_ROOT/.work/consolidation"
  git -C "$CANDIDATE_ROOT" check-ignore --quiet "$CANDIDATE_ROOT/.work/consolidation" || fail "candidate consolidation output is not ignored"
  snapshot_sources "$RUN_ROOT/sources.before"
  run_logged source-seal python3 "$ROOT/dev/consolidation/w9-publication-run.py" seal-source \
    --candidate-root "$CANDIDATE_ROOT" --input-root "$INPUT_ROOT" \
    --input-manifest "$INPUT_MANIFEST" --receipt "$SOURCE_RECEIPT"
  run_logged adapter-build flock -x "$CARGO_LOCK" env TMPDIR="$TMPDIR" python3 \
    "$ROOT/dev/consolidation/w9_publication_adapter_runner.py" build-harnesses \
    --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
    --reference-source-root "$REFERENCE_ROOT"
  REFERENCE_ADAPTER_BUILD=$(log_field "$RUN_ROOT/adapter-build.log" reference_build_receipt)
  CANDIDATE_ADAPTER_BUILD=$(log_field "$RUN_ROOT/adapter-build.log" candidate_build_receipt)
  require_fresh_cargo_artifact "$REFERENCE_ADAPTER_BUILD" reference-adapter
  require_fresh_cargo_artifact "$CANDIDATE_ADAPTER_BUILD" candidate-adapter
  run_logged exact-build-dry-run python3 "$ROOT/dev/consolidation/w9_publication_exact_owner_runner.py" build-harnesses \
    --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
    --reference-source-root "$REFERENCE_ROOT" --dry-run
  run_logged exact-build flock -x "$CARGO_LOCK" env TMPDIR="$TMPDIR" python3 \
    "$ROOT/dev/consolidation/w9_publication_exact_owner_runner.py" build-harnesses \
    --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
    --reference-source-root "$REFERENCE_ROOT"
  REFERENCE_EXACT_BUILD=$(log_field "$RUN_ROOT/exact-build.log" reference_build_receipt)
  CANDIDATE_EXACT_BUILD=$(log_field "$RUN_ROOT/exact-build.log" candidate_build_receipt)
  require_fresh_cargo_artifact "$REFERENCE_EXACT_BUILD" reference-exact-owner
  require_fresh_cargo_artifact "$CANDIDATE_EXACT_BUILD" candidate-exact-owner
  run_logged derived-reference-build flock -x "$CARGO_LOCK" env TMPDIR="$TMPDIR" python3 \
    "$ROOT/dev/consolidation/w9_publication_adapter_runner.py" build-derived-reference \
    --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
    --derived-reference-source-root "$DERIVED_ROOT" --derivation-schema 3
  DERIVED_BUILD=$(log_field "$RUN_ROOT/derived-reference-build.log" derived_reference_build_receipt)
  require_fresh_cargo_artifact "$DERIVED_BUILD" derived-reference
  ADAPTER_OUTPUT="$RUN_ROOT/adapter-capture"
  assert_output_root "$ADAPTER_OUTPUT"
  run_logged adapter-capture-dry-run python3 "$ROOT/dev/consolidation/w9_publication_adapter_runner.py" run \
    --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
    --reference-source-root "$REFERENCE_ROOT" --reference-build-receipt "$REFERENCE_ADAPTER_BUILD" \
    --candidate-build-receipt "$CANDIDATE_ADAPTER_BUILD" --capture-only \
    --output "$ADAPTER_OUTPUT" --dry-run
  snapshot_sources "$RUN_ROOT/sources.after-prepare"
  cmp -s "$RUN_ROOT/sources.before" "$RUN_ROOT/sources.after-prepare" || fail "source identities changed during prepare"
  assert_sources
  printf 'prepare complete: source_receipt=%s\nrun_root=%s\n' "$SOURCE_RECEIPT" "$RUN_ROOT"
  printf 'reference_adapter_build=%s\ncandidate_adapter_build=%s\n' "$REFERENCE_ADAPTER_BUILD" "$CANDIDATE_ADAPTER_BUILD"
  printf 'reference_exact_build=%s\ncandidate_exact_build=%s\nderived_reference_build=%s\n' "$REFERENCE_EXACT_BUILD" "$CANDIDATE_EXACT_BUILD" "$DERIVED_BUILD"
  printf 'adapter_capture_plan=%s\n' "$RUN_ROOT/adapter-capture-dry-run.log"
  exit 0
fi

[[ -d $RUN_ROOT && ! -L $RUN_ROOT && $(realpath "$RUN_ROOT") == "$RUN_ROOT" ]] || fail "attempt root is missing or noncanonical: $RUN_ROOT"
[[ -f $SOURCE_RECEIPT && ! -L $SOURCE_RECEIPT ]] || fail "source receipt is missing: $SOURCE_RECEIPT"
REFERENCE_ADAPTER_BUILD=$(log_field "$RUN_ROOT/adapter-build.log" reference_build_receipt)
CANDIDATE_ADAPTER_BUILD=$(log_field "$RUN_ROOT/adapter-build.log" candidate_build_receipt)
REFERENCE_EXACT_BUILD=$(log_field "$RUN_ROOT/exact-build.log" reference_build_receipt)
CANDIDATE_EXACT_BUILD=$(log_field "$RUN_ROOT/exact-build.log" candidate_build_receipt)
DERIVED_BUILD=$(log_field "$RUN_ROOT/derived-reference-build.log" derived_reference_build_receipt)
ADAPTER_OUTPUT="$RUN_ROOT/adapter-capture"
REFERENCE_CAPTURE="$ADAPTER_OUTPUT/reference/capture.receipt"
CANDIDATE_CAPTURE="$ADAPTER_OUTPUT/candidate/capture.receipt"
REFERENCE_EXACT_OUTPUT="$RUN_ROOT/exact-reference"
CANDIDATE_EXACT_OUTPUT="$RUN_ROOT/exact-candidate"
REFERENCE_EXACT_CAPTURE="$REFERENCE_EXACT_OUTPUT/capture.receipt"
CANDIDATE_EXACT_CAPTURE="$CANDIDATE_EXACT_OUTPUT/capture.receipt"
DERIVED_OUTPUT="$RUN_ROOT/derived-reference-capture"
DERIVED_CAPTURE="$DERIVED_OUTPUT/derived-reference/capture.receipt"
DERIVATION_RECEIPT="$DERIVED_OUTPUT/derivation.receipt"
ASSIGNMENT_BUILD_LOG="$RUN_ROOT/assignment-build.log"
ASSIGNMENT_OUTPUT="$RUN_ROOT/assignment-facts"
ASSIGNMENT_CAPTURE="$ASSIGNMENT_OUTPUT/assignment-capture.receipt"

if [[ $PHASE == capture ]]; then
  [[ -f $RUN_ROOT/adapter-capture-dry-run.log ]] || fail "adapter dry-run evidence is missing"
  [[ -f $RUN_ROOT/sources.after-prepare ]] || fail "prepare source snapshot is missing"
  cmp -s "$RUN_ROOT/sources.before" "$RUN_ROOT/sources.after-prepare" || fail "source identities changed during prepare"
  [[ ! -e $ADAPTER_OUTPUT && ! -L $ADAPTER_OUTPUT ]] || fail "adapter output already exists"
  snapshot_sources "$RUN_ROOT/sources.before-capture"
  run_logged adapter-capture python3 "$ROOT/dev/consolidation/w9_publication_adapter_runner.py" run \
    --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
    --reference-source-root "$REFERENCE_ROOT" --reference-build-receipt "$REFERENCE_ADAPTER_BUILD" \
    --candidate-build-receipt "$CANDIDATE_ADAPTER_BUILD" --capture-only --output "$ADAPTER_OUTPUT"
  [[ -f $REFERENCE_CAPTURE && -f $CANDIDATE_CAPTURE ]] || fail "adapter capture receipts are incomplete"
  run_logged reference-exact-dry-run python3 "$ROOT/dev/consolidation/w9_publication_exact_owner_runner.py" capture \
    --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
    --reference-source-root "$REFERENCE_ROOT" --role reference --build-receipt "$REFERENCE_EXACT_BUILD" \
    --adapter-capture-receipt "$REFERENCE_CAPTURE" --output "$REFERENCE_EXACT_OUTPUT" --dry-run
  run_logged candidate-exact-dry-run python3 "$ROOT/dev/consolidation/w9_publication_exact_owner_runner.py" capture \
    --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
    --reference-source-root "$REFERENCE_ROOT" --role candidate --build-receipt "$CANDIDATE_EXACT_BUILD" \
    --adapter-capture-receipt "$CANDIDATE_CAPTURE" --output "$CANDIDATE_EXACT_OUTPUT" --dry-run
  run_logged reference-exact-capture python3 "$ROOT/dev/consolidation/w9_publication_exact_owner_runner.py" capture \
    --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
    --reference-source-root "$REFERENCE_ROOT" --role reference --build-receipt "$REFERENCE_EXACT_BUILD" \
    --adapter-capture-receipt "$REFERENCE_CAPTURE" --output "$REFERENCE_EXACT_OUTPUT"
  run_logged candidate-exact-capture python3 "$ROOT/dev/consolidation/w9_publication_exact_owner_runner.py" capture \
    --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
    --reference-source-root "$REFERENCE_ROOT" --role candidate --build-receipt "$CANDIDATE_EXACT_BUILD" \
    --adapter-capture-receipt "$CANDIDATE_CAPTURE" --output "$CANDIDATE_EXACT_OUTPUT"
  run_logged derived-reference-capture python3 "$ROOT/dev/consolidation/w9_publication_adapter_runner.py" capture-derived-reference \
    --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
    --derived-reference-source-root "$DERIVED_ROOT" --derivation-schema 3 \
    --derived-reference-build-receipt "$DERIVED_BUILD" --original-capture-receipt "$REFERENCE_CAPTURE" \
    --original-exact-build-receipt "$REFERENCE_EXACT_BUILD" \
    --original-exact-capture-receipt "$REFERENCE_EXACT_CAPTURE" --output "$DERIVED_OUTPUT"
  run_logged assignment-build-dry-run python3 "$ROOT/dev/consolidation/w9_publication_exact_owner_runner.py" build-assignment-facts-harness \
    --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
    --adapter-capture-receipt "$CANDIDATE_CAPTURE" --exact-build-receipt "$CANDIDATE_EXACT_BUILD" \
    --exact-capture-receipt "$CANDIDATE_EXACT_CAPTURE" --dry-run
  run_logged assignment-build flock -x "$CARGO_LOCK" env TMPDIR="$TMPDIR" python3 \
    "$ROOT/dev/consolidation/w9_publication_exact_owner_runner.py" build-assignment-facts-harness \
    --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
    --adapter-capture-receipt "$CANDIDATE_CAPTURE" --exact-build-receipt "$CANDIDATE_EXACT_BUILD" \
    --exact-capture-receipt "$CANDIDATE_EXACT_CAPTURE"
  ASSIGNMENT_BUILD=$(log_field "$ASSIGNMENT_BUILD_LOG" candidate_build_receipt)
  require_fresh_cargo_artifact "$ASSIGNMENT_BUILD" assignment-facts
  ASSIGNMENT_BINARY=$(receipt_field "$ASSIGNMENT_BUILD" binary)
  TEST_DOCKER=(
    docker --host unix:///var/run/docker.sock run --rm --init --platform linux/amd64
    --cpus=4 --cpuset-cpus=0-3 --memory=8g --memory-swap=8g --network=none
    --user "$(id -u):$(id -g)"
    -v "$ROOT:$ROOT:ro" -v "$PROGRAM_ROOT:$PROGRAM_ROOT:ro"
    -v "$INPUT_ROOT:$INPUT_ROOT:ro" -v "$ROOT/.work/tmp:/scratch:rw"
    -w "$ROOT" -e LC_ALL=C -e TZ=UTC -e TMPDIR=/scratch
    -e "W9_PROGRAM_ROOT=$PROGRAM_ROOT" -e "W9_INPUT_ROOT=$INPUT_ROOT"
    -e "W9_SOURCE_COMMIT=$MAIN_HEAD" -e "W9_SOURCE_TREE=$MAIN_TREE"
  )
  run_logged assignment-test-list "${TEST_DOCKER[@]}" sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c \
    /usr/bin/env -u XSH_MODULE_PATH -u LD_PRELOAD "$ASSIGNMENT_BINARY" --list
  selectors=(
    sema::w9_source_audit_rejects_loaded_bytes_that_differ_from_the_file
    sema::w9_source_audit_keeps_roots_bound_to_the_config_bytes_it_resolved
    sema::tests::legacy_target_roots_collapse_field_index_and_duplicate_assignments
    sema::tests::legacy_collection_follows_raw_sugar_and_skips_expression_and_defer_bodies
    sema::tests::source_backed_empty_root_keeps_an_explicit_empty_owner
    sema::tests::imported_module_names_remain_separate_from_the_root_owner
    sema::tests::source_backed_empty_imported_module_keeps_an_explicit_empty_owner
  )
  for selector in "${selectors[@]}"; do
    count=$(grep -Fxc -- "$selector: test" "$RUN_ROOT/assignment-test-list.log" || true)
    [[ $count == 1 ]] || fail "assignment harness listing contains $count entries for $selector"
    safe_name=${selector//:/_}
    run_logged "assignment-selector-$safe_name" "${TEST_DOCKER[@]}" \
      sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c \
      /usr/bin/env -u XSH_MODULE_PATH -u LD_PRELOAD "$ASSIGNMENT_BINARY" --exact "$selector" --nocapture
    grep -Fq "test $selector ... ok" "$RUN_ROOT/assignment-selector-$safe_name.log" || fail "selector did not report one passing test: $selector"
    grep -Fq '1 passed; 0 failed' "$RUN_ROOT/assignment-selector-$safe_name.log" || fail "selector summary is not exactly one passing test: $selector"
  done
  assert_output_root "$ASSIGNMENT_OUTPUT"
  run_logged assignment-capture-dry-run python3 "$ROOT/dev/consolidation/w9_publication_exact_owner_runner.py" capture-assignment-facts \
    --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
    --build-receipt "$ASSIGNMENT_BUILD" --adapter-capture-receipt "$CANDIDATE_CAPTURE" \
    --exact-build-receipt "$CANDIDATE_EXACT_BUILD" --exact-capture-receipt "$CANDIDATE_EXACT_CAPTURE" \
    --output "$ASSIGNMENT_OUTPUT" --dry-run
  [[ ! -e $ASSIGNMENT_OUTPUT && ! -L $ASSIGNMENT_OUTPUT ]] || fail "assignment output exists after dry-run"
  snapshot_sources "$RUN_ROOT/sources.after-pre-assignment-capture"
  cmp -s "$RUN_ROOT/sources.before-capture" "$RUN_ROOT/sources.after-pre-assignment-capture" || fail "source identities changed during capture preparation"
  assert_sources
  printf 'pre-assignment-capture complete: adapter=%s exact_reference=%s exact_candidate=%s derived=%s\n' \
    "$CANDIDATE_CAPTURE" "$REFERENCE_EXACT_CAPTURE" "$CANDIDATE_EXACT_CAPTURE" "$DERIVED_CAPTURE"
  printf 'assignment_build=%s\nassignment_capture_plan=%s\n' "$ASSIGNMENT_BUILD" "$RUN_ROOT/assignment-capture-dry-run.log"
  exit 0
fi

[[ -f $RUN_ROOT/assignment-capture-dry-run.log ]] || fail "assignment capture dry-run evidence is missing"
[[ -f $REFERENCE_CAPTURE && -f $CANDIDATE_CAPTURE && -f $REFERENCE_EXACT_CAPTURE && -f $CANDIDATE_EXACT_CAPTURE && -f $DERIVED_CAPTURE && -f $DERIVATION_RECEIPT ]] || fail "required adapter/exact/derived receipts are incomplete"
ASSIGNMENT_BUILD=$(log_field "$ASSIGNMENT_BUILD_LOG" candidate_build_receipt)
if [[ $PHASE == assignment ]]; then
  [[ ! -e $ASSIGNMENT_OUTPUT && ! -L $ASSIGNMENT_OUTPUT ]] || fail "assignment output already exists"
  run_logged assignment-facts-capture python3 "$ROOT/dev/consolidation/w9_publication_exact_owner_runner.py" capture-assignment-facts \
    --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
    --build-receipt "$ASSIGNMENT_BUILD" --adapter-capture-receipt "$CANDIDATE_CAPTURE" \
    --exact-build-receipt "$CANDIDATE_EXACT_BUILD" --exact-capture-receipt "$CANDIDATE_EXACT_CAPTURE" \
    --output "$ASSIGNMENT_OUTPUT"
  [[ -f $ASSIGNMENT_CAPTURE ]] || fail "assignment facts receipt is missing"
  snapshot_sources "$RUN_ROOT/sources.after-assignment-capture"
  cmp -s "$RUN_ROOT/sources.after-pre-assignment-capture" "$RUN_ROOT/sources.after-assignment-capture" || fail "source identities changed during assignment capture"
  assert_sources
  printf 'assignment_facts_capture=%s\n' "$ASSIGNMENT_CAPTURE"
  exit 0
fi

[[ -f $ASSIGNMENT_CAPTURE ]] || fail "assignment facts capture receipt is missing"
[[ ! -e $RUN_ROOT/compare-derived && ! -L $RUN_ROOT/compare-derived ]] || fail "compare output already exists"
[[ ! -e $RUN_ROOT/compare-derived.log && ! -L $RUN_ROOT/compare-derived.log ]] || fail "comparison was already attempted"
snapshot_sources "$RUN_ROOT/sources.before-compare"
run_logged compare-derived python3 "$ROOT/dev/consolidation/w9_publication_adapter_runner.py" compare-derived-captures \
  --candidate-root "$CANDIDATE_ROOT" --candidate-source-receipt "$SOURCE_RECEIPT" \
  --reference-source-root "$REFERENCE_ROOT" --derived-reference-source-root "$DERIVED_ROOT" \
  --derived-reference-derivation-receipt "$DERIVATION_RECEIPT" \
  --original-capture-receipt "$REFERENCE_CAPTURE" \
  --reference-exact-build-receipt "$REFERENCE_EXACT_BUILD" \
  --candidate-exact-build-receipt "$CANDIDATE_EXACT_BUILD" \
  --candidate-capture-receipt "$CANDIDATE_CAPTURE" \
  --reference-exact-capture-receipt "$REFERENCE_EXACT_CAPTURE" \
  --candidate-exact-capture-receipt "$CANDIDATE_EXACT_CAPTURE" \
  --assignment-facts-capture-receipt "$ASSIGNMENT_CAPTURE" --output "$RUN_ROOT/compare-derived"
grep -Fxq 'corpus=1690 outcomes=6760 mismatches=0 assignment_facts_proof=verified' "$RUN_ROOT/compare-derived.log" || fail "comparison did not report complete zero-mismatch proof"
grep -Fxq 'status=complete' "$RUN_ROOT/compare-derived/attempt-status.txt" || fail "comparison status is not complete"
awk -F '\t' '$1 == "outcome_count" && $2 == "6760" { found++ } END { exit found != 1 }' \
  "$RUN_ROOT/compare-derived/comparison/derived-reference-comparison.receipt" || fail "derived-reference comparison receipt outcome count differs"
[[ ! -s $RUN_ROOT/compare-derived/comparison/mismatches.tsv ]] || fail "comparison mismatch file is nonempty"
snapshot_sources "$RUN_ROOT/sources.after-compare"
cmp -s "$RUN_ROOT/sources.before-compare" "$RUN_ROOT/sources.after-compare" || fail "source identities changed during comparison"
assert_sources
printf 'final parity verified: %s\n' "$RUN_ROOT/compare-derived.log"
