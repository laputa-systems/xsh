#!/usr/bin/env bash
set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
scratch_root="$repo_root/.work/tmp/qemu-current"
target_root="$repo_root/.work/target-final-canonical"
qemu_root="/home/josh/d/laputa-systems/xsh/.work/consolidation/qemu"
disk="$scratch_root/source-products.ext4"
target=x86_64-unknown-linux-musl

fail() {
    printf '%s\n' "$*" >&2
    exit 2
}

if [[ $# -ne 8 ]]; then
    fail "usage: $0 SOURCE_HEAD SOURCE_TREE XSH_SHA256 XSHT_SHA256 ORDINARY_HARNESS_STAGE_PATH ORDINARY_HARNESS_SHA256 PRIVILEGED_HARNESS_STAGE_PATH PRIVILEGED_HARNESS_SHA256"
fi

source_head=$1
source_tree=$2
xsh_sha256=$3
xsht_sha256=$4
ordinary_rel=$5
ordinary_sha256=$6
privileged_rel=$7
privileged_sha256=$8

[[ "$source_head" =~ ^[0-9a-f]{40}$ ]] || fail "source HEAD must be a full lowercase Git object ID"
[[ "$source_tree" =~ ^[0-9a-f]{40}$ ]] || fail "source tree must be a full lowercase Git object ID"
[[ "$(git rev-parse HEAD)" == "$source_head" ]] || fail "current checkout HEAD does not match the staging source"
[[ "$(git rev-parse 'HEAD^{tree}')" == "$source_tree" ]] || fail "current checkout tree does not match the staging source"
[[ -z "$(git status --porcelain=v1 --untracked-files=all)" ]] || fail "source checkout must be clean"
[[ -d "$target_root" && ! -L "$target_root" ]] || fail "canonical target directory is missing or unsafe: $target_root"
[[ -d "$qemu_root" && ! -L "$qemu_root" ]] || fail "prepared QEMU directory is missing or unsafe: $qemu_root"
[[ ! -e "$disk" && ! -L "$disk" ]] || fail "source/products image already exists: $disk"
git check-ignore --quiet --no-index .work || fail ".work must be ignored through .git/info/exclude"

validate_sha256() {
    local value=$1
    local label=$2
    [[ "$value" =~ ^[0-9a-f]{64}$ ]] || fail "$label must be a full lowercase SHA-256"
}

validate_sha256 "$xsh_sha256" "xsh SHA-256"
validate_sha256 "$xsht_sha256" "xsht SHA-256"
validate_sha256 "$ordinary_sha256" "ordinary harness SHA-256"
validate_sha256 "$privileged_sha256" "privileged harness SHA-256"
[[ "$ordinary_rel" =~ ^target/x86_64-unknown-linux-musl/verification/build/xsh/[[:xdigit:]]{16}/out/core_compat_extra-[[:xdigit:]]{16}$ ]] ||
    fail "ordinary harness path must be the exact Cargo JSON path inside the verification profile: $ordinary_rel"
[[ "$privileged_rel" == target/x86_64-unknown-linux-musl/verification/build/xsh/a67ed2905136c9b7/out/core_compat_extra-a67ed2905136c9b7 ]] ||
    fail "privileged harness path must be the qualified Cargo artifact: $privileged_rel"

for command in cp install mke2fs sha256sum e2fsck truncate; do
    command -v "$command" >/dev/null 2>&1 || fail "required host tool is missing: $command"
done

verify_hash() {
    local expected=$1
    local path=$2
    printf '%s  %s\n' "$expected" "$path" | sha256sum -c - >/dev/null
}

xsh_path="$target_root/$target/verification/xsh"
xsht_path="$target_root/$target/verification/xsht"
ordinary_path="$target_root/${ordinary_rel#target/}"
privileged_path="$target_root/${privileged_rel#target/}"
for path in "$xsh_path" "$xsht_path" "$ordinary_path" "$privileged_path"; do
    [[ -f "$path" && -x "$path" && ! -L "$path" ]] || fail "staged product is missing, unsafe, or not executable: $path"
done
verify_hash "$xsh_sha256" "$xsh_path" || fail "xsh hash changed before staging"
verify_hash "$xsht_sha256" "$xsht_path" || fail "xsht hash changed before staging"
verify_hash "$ordinary_sha256" "$ordinary_path" || fail "ordinary harness hash changed before staging"
verify_hash "$privileged_sha256" "$privileged_path" || fail "privileged harness hash changed before staging"

stage_root=$(mktemp -d "$scratch_root/qemu-products.XXXXXXXX")
trap 'rm -rf -- "$stage_root"' EXIT
mkdir -p "$stage_root/target/$target/verification"
cp -a "$repo_root/core" "$stage_root/core"
install -D -m 0755 "$xsh_path" "$stage_root/target/$target/verification/xsh"
install -D -m 0755 "$xsht_path" "$stage_root/target/$target/verification/xsht"
install -D -m 0755 "$ordinary_path" "$stage_root/$ordinary_rel"
install -D -m 0755 "$privileged_path" "$stage_root/$privileged_rel"
verify_hash "$xsh_sha256" "$stage_root/target/$target/verification/xsh" || fail "staged xsh copy hash mismatch"
verify_hash "$xsht_sha256" "$stage_root/target/$target/verification/xsht" || fail "staged xsht copy hash mismatch"
verify_hash "$ordinary_sha256" "$stage_root/$ordinary_rel" || fail "staged ordinary harness copy hash mismatch"
verify_hash "$privileged_sha256" "$stage_root/$privileged_rel" || fail "staged privileged harness copy hash mismatch"

truncate -s 256M "$disk"
mke2fs -q -F -t ext4 -d "$stage_root" "$disk"
e2fsck -fn "$disk" >/dev/null || fail "staged source/products ext4 image failed its read-only filesystem check"
disk_sha256=$(sha256sum -- "$disk" | cut -d ' ' -f 1)
verify_hash "$xsh_sha256" "$xsh_path" || fail "xsh hash changed during staging"
verify_hash "$xsht_sha256" "$xsht_path" || fail "xsht hash changed during staging"
verify_hash "$ordinary_sha256" "$ordinary_path" || fail "ordinary harness hash changed during staging"
verify_hash "$privileged_sha256" "$privileged_path" || fail "privileged harness hash changed during staging"

[[ "$(git rev-parse HEAD)" == "$source_head" ]] || fail "source HEAD changed during staging"
[[ "$(git rev-parse 'HEAD^{tree}')" == "$source_tree" ]] || fail "source tree changed during staging"
[[ -z "$(git status --porcelain=v1 --untracked-files=all)" ]] || fail "source checkout became dirty during staging"

printf 'source_head=%s\nsource_tree=%s\ndisk=%s\ndisk_sha256=%s\nxsh_sha256=%s\nxsht_sha256=%s\nordinary_path=%s\nordinary_sha256=%s\nprivileged_path=%s\nprivileged_sha256=%s\n' \
    "$source_head" "$source_tree" "$disk" "$disk_sha256" "$xsh_sha256" "$xsht_sha256" \
    "$ordinary_rel" "$ordinary_sha256" "$privileged_rel" "$privileged_sha256"
printf 'launch: bash /home/josh/d/laputa-systems/xsh/.work/wt/qemu-platform/.work/tmp/qemu-next/launchers/qemu-date-only.sh %q %s %s %s %s %s %q %s %q %s\n' \
    "${disk#"$repo_root/"}" "$source_head" "$source_tree" "$disk_sha256" "$xsh_sha256" "$xsht_sha256" \
    "$ordinary_rel" "$ordinary_sha256" "$privileged_rel" "$privileged_sha256"
