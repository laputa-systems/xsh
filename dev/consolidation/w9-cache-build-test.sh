#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  printf 'Usage: dev/consolidation/w9-cache-build-test.sh PINNED_RUNNER\n' >&2
  exit 2
fi

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
source "$script_dir/w9-cache-build-config.sh"
xsh_root=$(cd -- "$script_dir/../.." && pwd -P)
build_tool="$script_dir/w9-cache-build.sh"
measure_tool="$script_dir/w9-cache-measure.sh"
test_dir=$(mktemp -d "$xsh_root/.work/tmp/w9-cache-build-test.XXXXXX")
trap 'rm -rf -- "$test_dir"' EXIT

binary="$test_dir/fake-xsht"
runner="$test_dir/pinned-runner"
receipt="$test_dir/receipt.tsv"
output="$test_dir/measurement-output"
cp -- "$1" "$runner"
chmod +x "$runner"
printf 'synthetic XSHT executable\n' > "$binary"
chmod +x "$binary"

source_commit=$(git -C "$xsh_root" rev-parse HEAD)
source_tree=$(git -C "$xsh_root" rev-parse 'HEAD^{tree}')
builder_sha256=$(sha256sum -- "$runner" | awk '{print $1}')

write_receipt() {
  local commit=${1:-$source_commit}
  local tree=${2:-$source_tree}
  local image=${3:-$W9_CACHE_BUILD_IMAGE}
  local runner_hash=${4:-$builder_sha256}
  {
    printf 'schema\t%s\n' "$W9_CACHE_BUILD_SCHEMA"
    printf 'source_commit\t%s\n' "$commit"
    printf 'source_tree\t%s\n' "$tree"
    printf 'binary_path\t%s\n' "$binary"
    printf 'binary_sha256\t%s\n' "$binary_sha256"
    printf 'target\t%s\n' "$W9_CACHE_BUILD_TARGET"
    printf 'profile\t%s\n' "$W9_CACHE_BUILD_PROFILE"
    printf 'builder_image\t%s\n' "$image"
    printf 'builder_path\t%s\n' "$runner"
    printf 'builder_sha256\t%s\n' "$runner_hash"
  } > "$receipt"
}

expect_rejected() {
  local description=$1
  shift
  if "$@" >/dev/null 2>&1; then
    printf 'expected rejection: %s\n' "$description" >&2
    exit 1
  fi
}

binary_sha256=$(sha256sum -- "$binary" | awk '{print $1}')
write_receipt
"$build_tool" verify "$binary" "$receipt"

printf 'stale bytes\n' >> "$binary"
expect_rejected 'stale binary hash' "$build_tool" verify "$binary" "$receipt"
expect_rejected 'driver rejects stale binary before creating output' \
  "$measure_tool" "$binary" "$receipt" "$xsh_root" "$output"
[[ ! -e "$output" ]] || { printf 'invalid measurement created output directory\n' >&2; exit 1; }

printf 'synthetic XSHT executable\n' > "$binary"
chmod +x "$binary"
binary_sha256=$(sha256sum -- "$binary" | awk '{print $1}')

write_receipt deadbeef "$source_tree"
expect_rejected 'source commit mismatch' "$build_tool" verify "$binary" "$receipt"
write_receipt "$source_commit" deadbeef
expect_rejected 'source tree mismatch' "$build_tool" verify "$binary" "$receipt"
write_receipt "$source_commit" "$source_tree" sha256:0000000000000000000000000000000000000000000000000000000000000000
expect_rejected 'unpinned image' "$build_tool" verify "$binary" "$receipt"
write_receipt "$source_commit" "$source_tree" "$W9_CACHE_BUILD_IMAGE" deadbeef
expect_rejected 'unpinned build wrapper hash' "$build_tool" verify "$binary" "$receipt"

write_receipt
printf '\n# changed after receipt\n' >> "$runner"
expect_rejected 'changed build runner' "$build_tool" verify "$binary" "$receipt"

printf 'W9 cache build receipt checks passed\n'
