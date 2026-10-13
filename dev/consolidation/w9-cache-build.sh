#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
source "$script_dir/w9-cache-build-config.sh"
xsh_root=$(cd -- "$script_dir/../.." && pwd -P)

usage() {
  cat >&2 <<'USAGE'
Usage:
  dev/consolidation/w9-cache-build.sh build PINNED_RUNNER RECEIPT
  dev/consolidation/w9-cache-build.sh verify XSHT_BIN RECEIPT

The build receipt must be a new file under this checkout's .work/. Build
uses the x86_64 musl verification profile and the campaign's pinned image.
USAGE
}

fail() {
  printf 'W9 build receipt: %s\n' "$1" >&2
  exit 1
}

sha256_file() {
  sha256sum -- "$1" | awk '{print $1}'
}

canonical_existing_file() {
  local path=$1 parent name
  [[ "$path" == /* ]] || path="$PWD/$path"
  [[ -f "$path" && ! -L "$path" ]] || fail "expected a regular non-symlink file: $path"
  parent=$(cd -- "$(dirname -- "$path")" && pwd -P)
  name=$(basename -- "$path")
  printf '%s/%s\n' "$parent" "$name"
}

canonical_receipt_path() {
  local path=$1 parent name work_dir
  [[ "$path" == /* ]] || path="$xsh_root/$path"
  parent=$(dirname -- "$path")
  name=$(basename -- "$path")
  [[ -d "$parent" ]] || fail "receipt parent directory does not exist: $parent"
  parent=$(cd -- "$parent" && pwd -P)
  work_dir=$(cd -- "$xsh_root/.work" && pwd -P)
  case "$parent/" in
    "$work_dir/"*) ;;
    *) fail "receipt must be under $work_dir" ;;
  esac
  [[ "$name" != . && "$name" != .. ]] || fail "receipt path must name a file"
  [[ ! -e "$parent/$name" && ! -L "$parent/$name" ]] || fail "receipt path already exists: $parent/$name"
  printf '%s/%s\n' "$parent" "$name"
}

require_clean_source() {
  [[ -z "$(git -C "$xsh_root" status --porcelain)" ]] || fail "XSH source checkout must be clean"
}

runner_image() {
  local runner=$1 images
  images=$(rg --only-matching 'sha256:[0-9a-f]{64}' "$runner" | sort -u || true)
  [[ "$images" == "$W9_CACHE_BUILD_IMAGE" ]] || fail "runner does not use the pinned build image"
  printf '%s\n' "$images"
}

write_receipt() {
  local runner=$1 receipt=$2 binary=$3 head=$4 tree=$5 image=$6
  local runner_sha binary_sha temporary
  runner_sha=$(sha256_file "$runner")
  binary_sha=$(sha256_file "$binary")
  temporary=$(mktemp "$(dirname -- "$receipt")/.w9-cache-build.XXXXXX")
  trap 'rm -f -- "$temporary"' EXIT
  {
    printf 'schema\t%s\n' "$W9_CACHE_BUILD_SCHEMA"
    printf 'source_commit\t%s\n' "$head"
    printf 'source_tree\t%s\n' "$tree"
    printf 'binary_path\t%s\n' "$binary"
    printf 'binary_sha256\t%s\n' "$binary_sha"
    printf 'target\t%s\n' "$W9_CACHE_BUILD_TARGET"
    printf 'profile\t%s\n' "$W9_CACHE_BUILD_PROFILE"
    printf 'builder_image\t%s\n' "$image"
    printf 'builder_path\t%s\n' "$runner"
    printf 'builder_sha256\t%s\n' "$runner_sha"
  } > "$temporary"
  ln -- "$temporary" "$receipt" || fail "receipt path appeared during build: $receipt"
  rm -f -- "$temporary"
  trap - EXIT
}

read_receipt() {
  local receipt=$1 line key value
  schema=
  source_commit=
  source_tree=
  binary_path=
  binary_sha256=
  target=
  profile=
  builder_image_value=
  builder_path=
  builder_sha256=

  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ "$line" == *$'\t'* ]] || fail "malformed receipt row"
    key=${line%%$'\t'*}
    value=${line#*$'\t'}
    [[ -n "$key" && -n "$value" && "$value" != *$'\t'* ]] || fail "malformed receipt row"
    case "$key" in
      schema) [[ -z "$schema" ]] || fail "duplicate schema"; schema=$value ;;
      source_commit) [[ -z "$source_commit" ]] || fail "duplicate source_commit"; source_commit=$value ;;
      source_tree) [[ -z "$source_tree" ]] || fail "duplicate source_tree"; source_tree=$value ;;
      binary_path) [[ -z "$binary_path" ]] || fail "duplicate binary_path"; binary_path=$value ;;
      binary_sha256) [[ -z "$binary_sha256" ]] || fail "duplicate binary_sha256"; binary_sha256=$value ;;
      target) [[ -z "$target" ]] || fail "duplicate target"; target=$value ;;
      profile) [[ -z "$profile" ]] || fail "duplicate profile"; profile=$value ;;
      builder_image) [[ -z "$builder_image_value" ]] || fail "duplicate builder_image"; builder_image_value=$value ;;
      builder_path) [[ -z "$builder_path" ]] || fail "duplicate builder_path"; builder_path=$value ;;
      builder_sha256) [[ -z "$builder_sha256" ]] || fail "duplicate builder_sha256"; builder_sha256=$value ;;
      *) fail "unknown receipt field: $key" ;;
    esac
  done < "$receipt"

  for value in "$schema" "$source_commit" "$source_tree" "$binary_path" \
      "$binary_sha256" "$target" "$profile" "$builder_image_value" \
      "$builder_path" "$builder_sha256"; do
    [[ -n "$value" ]] || fail "receipt is missing a required field"
  done
}

verify_receipt() {
  local binary_arg=$1 receipt_arg=$2 binary receipt current_head current_tree work_dir
  local actual_binary_sha actual_builder_sha builder actual_image
  require_clean_source
  binary=$(canonical_existing_file "$binary_arg")
  [[ -x "$binary" ]] || fail "XSHT binary is not executable"
  receipt=$(canonical_existing_file "$receipt_arg")
  work_dir=$(cd -- "$xsh_root/.work" && pwd -P)
  case "$(dirname -- "$receipt")/" in
    "$work_dir/"*) ;;
    *) fail "receipt must be under $work_dir" ;;
  esac
  read_receipt "$receipt"

  current_head=$(git -C "$xsh_root" rev-parse HEAD)
  current_tree=$(git -C "$xsh_root" rev-parse 'HEAD^{tree}')
  [[ "$source_commit" == "$current_head" ]] || fail "receipt source commit does not match HEAD"
  [[ "$source_tree" == "$current_tree" ]] || fail "receipt source tree does not match HEAD"
  [[ "$schema" == "$W9_CACHE_BUILD_SCHEMA" ]] || fail "unsupported receipt schema"
  [[ "$binary_path" == "$binary" ]] || fail "receipt binary path does not match"
  [[ "$target" == "$W9_CACHE_BUILD_TARGET" ]] || fail "receipt target does not match"
  [[ "$profile" == "$W9_CACHE_BUILD_PROFILE" ]] || fail "receipt profile does not match"
  [[ "$builder_image_value" == "$W9_CACHE_BUILD_IMAGE" ]] || fail "receipt image is not the pinned build image"

  actual_binary_sha=$(sha256_file "$binary")
  [[ "$binary_sha256" == "$actual_binary_sha" ]] || fail "binary hash does not match receipt"
  builder=$(canonical_existing_file "$builder_path")
  [[ -x "$builder" ]] || fail "recorded build runner is not executable"
  [[ "$builder" == "$builder_path" ]] || fail "recorded build runner path is not canonical"
  actual_builder_sha=$(sha256_file "$builder")
  [[ "$builder_sha256" == "$actual_builder_sha" ]] || fail "build runner hash does not match receipt"
  [[ "$actual_builder_sha" == "$W9_CACHE_BUILD_RUNNER_SHA256" ]] || fail "build runner is not the pinned wrapper"
  actual_image=$(runner_image "$builder")
  [[ "$actual_image" == "$builder_image_value" ]] || fail "build runner image does not match receipt"
}

if [[ $# -lt 1 ]]; then
  usage
  exit 2
fi

case "$1" in
  build)
    if [[ $# -ne 3 ]]; then usage; exit 2; fi
    require_clean_source
    runner=$(canonical_existing_file "$2")
    [[ -x "$runner" ]] || fail "pinned runner is not executable: $runner"
    [[ "$(sha256_file "$runner")" == "$W9_CACHE_BUILD_RUNNER_SHA256" ]] || fail "runner is not the pinned wrapper"
    image=$(runner_image "$runner")
    receipt=$(canonical_receipt_path "$3")
    head=$(git -C "$xsh_root" rev-parse HEAD)
    tree=$(git -C "$xsh_root" rev-parse 'HEAD^{tree}')
    binary="$xsh_root/target/$W9_CACHE_BUILD_TARGET/$W9_CACHE_BUILD_PROFILE/xsht"
    "$runner" "$xsh_root" build cargo build --profile "$W9_CACHE_BUILD_PROFILE" \
      --target "$W9_CACHE_BUILD_TARGET" -p xsht --bin xsht
    require_clean_source
    [[ "$(git -C "$xsh_root" rev-parse HEAD)" == "$head" ]] || fail "source HEAD changed during build"
    [[ "$(git -C "$xsh_root" rev-parse 'HEAD^{tree}')" == "$tree" ]] || fail "source tree changed during build"
    [[ -f "$binary" && ! -L "$binary" ]] || fail "pinned build did not produce $binary"
    write_receipt "$runner" "$receipt" "$binary" "$head" "$tree" "$image"
    printf 'W9 XSHT build receipt: %s\n' "$receipt"
    ;;
  verify)
    if [[ $# -ne 3 ]]; then usage; exit 2; fi
    verify_receipt "$2" "$3"
    ;;
  *)
    usage
    exit 2
    ;;
esac
