#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
repo_root=$(cd -- "$script_dir/../.." && pwd -P)
validator="$script_dir/w9-cache-trace-validate.awk"
test_dir="$repo_root/.work/tmp/w9-cache-trace-test-$$"
mkdir -p -- "$repo_root/.work/tmp"
mkdir -- "$test_dir"
trap 'rm -rf -- "$test_dir"' EXIT

cache_row() {
  local path=$1 sequence=$2 owner=$3 kind=$4
  printf 'W9CACHE\t2\t123\t%s\t%s\t%s\tcache\t-\t-\t-\t-\t-\t-\n' \
    "$sequence" "$owner" "$kind" >> "$path"
}

record_row() {
  local path=$1 sequence=$2 owner=$3 kind=$4 event=$5 module=$6 source=$7
  local start=$8 len=$9 role=${10} domain=${11}
  printf 'W9CACHE\t2\t123\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$sequence" "$owner" "$kind" "$event" "$module" "$source" \
    "$start" "$len" "$role" "$domain" >> "$path"
}

validate() {
  awk -F '\t' -f "$validator" "$1"
}

expect_rejected() {
  local description=$1 path=$2
  if validate "$path" >/dev/null 2>&1; then
    printf 'expected rejection: %s\n' "$description" >&2
    exit 1
  fi
}

ordering_fixture() {
  local path=$1 role=$2 ordering=$3
  local module source start len sequence=0 requests event index
  local -a events
  if [[ "$role" == "Entry" ]]; then
    module='"<entry>"'
    source='SourceId(1)'
    start=0
    len=1
  else
    module='"module"'
    source='SourceId(2)'
    start=1
    len=1
  fi
  cache_row "$path" "$sequence" 1 shared
  ((sequence += 1))

  if [[ "$role" != "Entry" ]]; then
    record_row "$path" "$sequence" 1 shared requested '"<entry>"' 'SourceId(1)' 0 1 Entry Str
    ((sequence += 1))
    record_row "$path" "$sequence" 1 shared prepared '"<entry>"' 'SourceId(1)' 0 1 Entry Str
    ((sequence += 1))
    record_row "$path" "$sequence" 1 shared checked '"<entry>"' 'SourceId(1)' 0 1 Entry Str
    ((sequence += 1))
  fi

  case "$ordering" in
    concurrent-reorder) requests=3; events=(reuse checked reuse check_hit prepared check_hit) ;;
    check-hit-before-checked) requests=2; events=(prepared reuse check_hit checked) ;;
    check-hit-before-reuse) requests=2; events=(prepared checked check_hit reuse) ;;
    *) printf 'unknown ordering fixture: %s\n' "$ordering" >&2; exit 1 ;;
  esac
  for ((index = 0; index < requests; index++)); do
    record_row "$path" "$sequence" 1 shared requested "$module" "$source" \
      "$start" "$len" "$role" Str
    ((sequence += 1))
  done
  for event in "${events[@]}"; do
    record_row "$path" "$sequence" 1 shared "$event" "$module" "$source" \
      "$start" "$len" "$role" Str
    ((sequence += 1))
  done
}

valid="$test_dir/valid.tsv"
cache_row "$valid" 0 1 shared
record_row "$valid" 1 1 shared requested '"<entry>"' 'SourceId(1)' 0 3 Entry Str
record_row "$valid" 2 1 shared prepared '"<entry>"' 'SourceId(1)' 0 3 Entry Str
record_row "$valid" 3 1 shared checked '"<entry>"' 'SourceId(1)' 0 3 Entry Str
record_row "$valid" 4 1 shared requested '"shared"' 'SourceId(2)' 4 2 User Str
record_row "$valid" 5 1 shared prepared '"shared"' 'SourceId(2)' 4 2 User Str
record_row "$valid" 6 1 shared checked '"shared"' 'SourceId(2)' 4 2 User Str
record_row "$valid" 7 1 shared requested '"<entry>"' 'SourceId(3)' 0 3 Entry Str
record_row "$valid" 8 1 shared prepared '"<entry>"' 'SourceId(3)' 0 3 Entry Str
record_row "$valid" 9 1 shared checked '"<entry>"' 'SourceId(3)' 0 3 Entry Str
record_row "$valid" 10 1 shared requested '"shared"' 'SourceId(2)' 4 2 User Str
record_row "$valid" 11 1 shared reuse '"shared"' 'SourceId(2)' 4 2 User Str
record_row "$valid" 12 1 shared check_hit '"shared"' 'SourceId(2)' 4 2 User Str
record_row "$valid" 13 1 shared requested '"shared"' 'SourceId(2)' 4 2 User Bytes
record_row "$valid" 14 1 shared prepared '"shared"' 'SourceId(2)' 4 2 User Bytes
record_row "$valid" 15 1 shared checked '"shared"' 'SourceId(2)' 4 2 User Bytes

summary=$(validate "$valid")
rg -q '^shared caches=1 requests=5 records=4 cache_reuses=1 checks=4 check_hits=1$' \
  <<< "$summary"
rg -q '^shared module keys across multiple owners=0$' <<< "$summary"

for role in User Entry; do
  concurrent_order="$test_dir/$role-concurrent-reorder.tsv"
  ordering_fixture "$concurrent_order" "$role" concurrent-reorder
  validate "$concurrent_order" >/dev/null

  malformed_order="$test_dir/$role-check-hit-before-checked.tsv"
  ordering_fixture "$malformed_order" "$role" check-hit-before-checked
  expect_rejected "$role check_hit before checked" "$malformed_order"

  malformed_order="$test_dir/$role-check-hit-before-reuse.tsv"
  ordering_fixture "$malformed_order" "$role" check-hit-before-reuse
  expect_rejected "$role check_hit before reuse" "$malformed_order"
done

old_schema="$test_dir/old-schema.tsv"
printf 'W9CACHE\t1\t123\t0\t1\tshared\tcache\t-\t-\t-\t-\t-\t-\n' > "$old_schema"
expect_rejected 'unsupported trace schema' "$old_schema"

missing_lookup="$test_dir/missing-lookup.tsv"
cache_row "$missing_lookup" 0 1 shared
record_row "$missing_lookup" 1 1 shared requested '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$missing_lookup" 2 1 shared prepared '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$missing_lookup" 3 1 shared checked '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$missing_lookup" 4 1 shared requested '"module"' 'SourceId(2)' 1 1 User Str
expect_rejected 'requested key has no cache lookup' "$missing_lookup"

missing_check="$test_dir/missing-check.tsv"
cache_row "$missing_check" 0 1 shared
record_row "$missing_check" 1 1 shared requested '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$missing_check" 2 1 shared prepared '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$missing_check" 3 1 shared checked '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$missing_check" 4 1 shared requested '"module"' 'SourceId(2)' 1 1 User Str
record_row "$missing_check" 5 1 shared prepared '"module"' 'SourceId(2)' 1 1 User Str
expect_rejected 'requested key is not checked' "$missing_check"

missing_hit="$test_dir/missing-hit.tsv"
cache_row "$missing_hit" 0 1 shared
record_row "$missing_hit" 1 1 shared requested '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$missing_hit" 2 1 shared prepared '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$missing_hit" 3 1 shared checked '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$missing_hit" 4 1 shared requested '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$missing_hit" 5 1 shared reuse '"<entry>"' 'SourceId(1)' 0 1 Entry Str
expect_rejected 'reused key has no check hit' "$missing_hit"

orphan_event="$test_dir/orphan-event.tsv"
cache_row "$orphan_event" 0 1 shared
record_row "$orphan_event" 1 1 shared requested '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$orphan_event" 2 1 shared prepared '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$orphan_event" 3 1 shared checked '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$orphan_event" 4 1 shared prepared '"module"' 'SourceId(2)' 1 1 User Str
record_row "$orphan_event" 5 1 shared checked '"module"' 'SourceId(2)' 1 1 User Str
expect_rejected 'cache event has no requested key' "$orphan_event"

shared_duplicate="$test_dir/shared-duplicate.tsv"
cache_row "$shared_duplicate" 0 1 shared
record_row "$shared_duplicate" 1 1 shared requested '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$shared_duplicate" 2 1 shared prepared '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$shared_duplicate" 3 1 shared checked '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$shared_duplicate" 4 1 shared requested '"module"' 'SourceId(2)' 1 1 User Str
record_row "$shared_duplicate" 5 1 shared prepared '"module"' 'SourceId(2)' 1 1 User Str
record_row "$shared_duplicate" 6 1 shared checked '"module"' 'SourceId(2)' 1 1 User Str
cache_row "$shared_duplicate" 7 2 shared
record_row "$shared_duplicate" 8 2 shared requested '"<entry>"' 'SourceId(3)' 0 1 Entry Str
record_row "$shared_duplicate" 9 2 shared prepared '"<entry>"' 'SourceId(3)' 0 1 Entry Str
record_row "$shared_duplicate" 10 2 shared checked '"<entry>"' 'SourceId(3)' 0 1 Entry Str
record_row "$shared_duplicate" 11 2 shared requested '"module"' 'SourceId(2)' 1 1 User Str
record_row "$shared_duplicate" 12 2 shared prepared '"module"' 'SourceId(2)' 1 1 User Str
record_row "$shared_duplicate" 13 2 shared checked '"module"' 'SourceId(2)' 1 1 User Str
expect_rejected 'shared key checked by multiple cache owners' "$shared_duplicate"

private_workers="$test_dir/private-workers.tsv"
cache_row "$private_workers" 0 1 private
record_row "$private_workers" 1 1 private requested '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$private_workers" 2 1 private prepared '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$private_workers" 3 1 private checked '"<entry>"' 'SourceId(1)' 0 1 Entry Str
record_row "$private_workers" 4 1 private requested '"module"' 'SourceId(2)' 1 1 User Str
record_row "$private_workers" 5 1 private prepared '"module"' 'SourceId(2)' 1 1 User Str
record_row "$private_workers" 6 1 private checked '"module"' 'SourceId(2)' 1 1 User Str
cache_row "$private_workers" 7 2 private
record_row "$private_workers" 8 2 private requested '"<entry>"' 'SourceId(3)' 0 1 Entry Str
record_row "$private_workers" 9 2 private prepared '"<entry>"' 'SourceId(3)' 0 1 Entry Str
record_row "$private_workers" 10 2 private checked '"<entry>"' 'SourceId(3)' 0 1 Entry Str
record_row "$private_workers" 11 2 private requested '"module"' 'SourceId(2)' 1 1 User Str
record_row "$private_workers" 12 2 private prepared '"module"' 'SourceId(2)' 1 1 User Str
record_row "$private_workers" 13 2 private checked '"module"' 'SourceId(2)' 1 1 User Str
private_summary=$(validate "$private_workers")
rg -q '^shared module keys across multiple owners=0$' <<< "$private_summary"
rg -q '^private repeated module keys across owners=1$' <<< "$private_summary"

empty="$test_dir/empty.tsv"
: > "$empty"
expect_rejected 'empty trace' "$empty"

printf 'W9 cache trace fixtures passed\n'
