#!/usr/bin/env bash
set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
qemu_root="/home/josh/d/laputa-systems/xsh/.work/consolidation/qemu"
scratch_root="$repo_root/.work/tmp/qemu-current/runs"
guest_root="$qemu_root/guest-root.ext4"
kernel="$qemu_root/boot/vmlinuz-stable"
initramfs="$qemu_root/boot/initramfs-stable"
export TMPDIR="$scratch_root"

fail() {
    printf '%s\n' "$*" >&2
    exit 2
}

if [[ $# -ne 9 ]]; then
    fail "usage: $0 READ_ONLY_SOURCE_PRODUCTS.ext4 SOURCE_HEAD SOURCE_TREE_OID PRODUCT_DISK_SHA256 XSH_SHA256 CORE_COMPAT_EXTRA_PATH CORE_COMPAT_EXTRA_SHA256 CORE_COMPAT_EXTRA_PRIVILEGED_PATH CORE_COMPAT_EXTRA_PRIVILEGED_SHA256"
fi

product_disk=$(readlink -f "$1")
source_head=$2
source_tree_oid=$3
product_disk_sha256=$4
xsh_sha256=$5
core_extra_rel=$6
core_extra_sha256=$7
core_extra_privileged_rel=$8
core_extra_privileged_sha256=$9

case "$product_disk" in
    "$repo_root"/.work/*) ;;
    *) fail "product disk must be under .work/: $product_disk" ;;
esac
[[ -f "$product_disk" ]] || fail "product disk is not a regular file: $product_disk"
[[ "$product_disk" != "$guest_root" ]] || fail "product disk cannot be the guest root image"

validate_harness_path() {
    local path=$1
    local binary_name=$2
    [[ "$path" =~ ^target/x86_64-unknown-linux-musl/verification/build/xsh/[[:xdigit:]]{16}/out/${binary_name}-[[:xdigit:]]{16}$ ]] ||
        fail "harness path must be the supplied Cargo artifact inside the x86_64 musl verification profile: $path"
}

validate_oid() {
    local value=$1
    local label=$2
    [[ "$value" =~ ^[0-9a-f]{40,64}$ ]] || fail "$label must be a full lowercase Git object ID"
}

validate_sha256() {
    local value=$1
    local label=$2
    [[ "$value" =~ ^[0-9a-f]{64}$ ]] || fail "$label must be a full lowercase SHA-256"
}

validate_oid "$source_head" "source HEAD"
validate_oid "$source_tree_oid" "source tree OID"
validate_sha256 "$product_disk_sha256" "source/products disk SHA-256"
validate_sha256 "$xsh_sha256" "verification xsh SHA-256"
validate_sha256 "$core_extra_sha256" "ordinary harness SHA-256"
validate_sha256 "$core_extra_privileged_sha256" "privileged harness SHA-256"
validate_harness_path "$core_extra_rel" core_compat_extra
validate_harness_path "$core_extra_privileged_rel" core_compat_extra_privileged

actual_head=$(git rev-parse HEAD)
actual_tree_oid=$(git rev-parse 'HEAD^{tree}')
[[ "$actual_head" == "$source_head" ]] || fail "current checkout HEAD does not match the supplied final source HEAD"
[[ "$actual_tree_oid" == "$source_tree_oid" ]] || fail "current checkout tree does not match the supplied final source tree OID"
[[ -z "$(git status --porcelain --untracked-files=all)" ]] || fail "current checkout must be clean before acceptance"

for command in qemu-system-x86_64 qemu-img expect e2fsck sha256sum od tr readlink df tail cat grep mktemp; do
    command -v "$command" >/dev/null 2>&1 || fail "required host tool is missing: $command"
done

[[ -f "$kernel" && -r "$kernel" && -r "$initramfs" && -r "$guest_root" ]] || fail "prepared guest kernel, initramfs, or root image is missing"
[[ -x "$qemu_root/rootfs/usr/local/bin/date" && -x "$qemu_root/rootfs/usr/local/bin/cp" ]] || fail "qualified GNU date/cp are missing from the prepared guest root"
[[ -x "$qemu_root/rootfs/sbin/mkfs.btrfs" && -x "$qemu_root/rootfs/bin/mount" ]] || fail "BTRFS fixture tools are missing from the prepared guest root"
[[ -x "$qemu_root/rootfs/bin/busybox" ]] || fail "guest BusyBox is required for staged product hash checks"

read -r -a scratch_df <<< "$(df -T "$scratch_root" | tail -n 1)"
[[ "${scratch_df[1]:-}" != tmpfs && "${scratch_df[1]:-}" != ramfs ]] || fail "host scratch must be disk-backed: $scratch_root"

verify_hash() {
    local expected=$1
    local path=$2
    printf '%s  %s\n' "$expected" "$path" | sha256sum -c - >/dev/null
}

verify_hash f12a4ff57b432fef1290c649c389f19f7b80a2d9a960ccce0d2492a55b3708e7 "$kernel" || fail "prepared guest kernel hash changed"
verify_hash db6d679337f3a2136a034052a0725e22435688dd07695f91e44aa4cad37e4a26 "$initramfs" || fail "prepared initramfs hash changed"
verify_hash 45e3c9219fd2d9ab20344943579ba5d1a5881eb85619e255229ff59f5e495a31 "$guest_root" || fail "prepared guest root hash changed"
verify_hash "$product_disk_sha256" "$product_disk" || fail "source/products disk hash does not match the supplied staging identity"
e2fsck -fn "$guest_root" >/dev/null || fail "prepared guest root ext4 check failed"
e2fsck -fn "$product_disk" >/dev/null || fail "source/products disk ext4 check failed"
qemu_img_info=$(qemu-img info "$product_disk")
grep -Fq 'file format: raw' <<< "$qemu_img_info" || fail "source/products disk must be raw ext4"

verify_root_unchanged() {
    verify_hash 45e3c9219fd2d9ab20344943579ba5d1a5881eb85619e255229ff59f5e495a31 "$guest_root"
}
trap 'exit_code=$?; if ! verify_root_unchanged; then printf "%s\n" "guest root backing image changed during QEMU run" >&2; exit 1; fi; exit "$exit_code"' EXIT

run_guest() {
    local label=$1
    local harness_rel=$2
    local filter=$3
    local reference=$4
    local expected_passed=$5
    local run_dir nonce root_overlay guest_command test_args expect_status applet_source

    run_dir=$(mktemp -d "$scratch_root/qemu-final-$label.XXXXXXXX")
    mkdir -p "$run_dir/tmp"
    root_overlay="$run_dir/guest-root.qcow2"
    qemu-img create -q -f qcow2 -F raw -b "$guest_root" "$root_overlay"
    nonce=$(od -An -v -N32 -tx1 /dev/urandom | tr -d ' \n')
    [[ "$nonce" =~ ^[0-9a-f]{64}$ ]] || fail "could not make a fresh 256-bit lowercase nonce"
    if [[ "$label" == reflink-* ]]; then
        applet_source=cp.xsh
    else
        applet_source=date.xsh
    fi

    if [[ -n "$reference" ]]; then
        test_args="/work/$harness_rel --ignored --test-threads=1 --nocapture $filter"
        guest_command=$(cat <<EOF
set +e
cd /work || exit 125
printf 'XSH_QEMU_STAGING_ID source_head=%s source_tree_oid=%s product_disk_sha256=%s\\n' '$source_head' '$source_tree_oid' '$product_disk_sha256'
if /bin/busybox sha256sum -c <<HASHES
$xsh_sha256  /work/target/x86_64-unknown-linux-musl/verification/xsh
$core_extra_sha256  /work/$core_extra_rel
$core_extra_privileged_sha256  /work/$core_extra_privileged_rel
HASHES
then staged_hashes=1; else staged_hashes=0; fi
if [ ! -x "/work/$harness_rel" ] || [ ! -x /work/target/x86_64-unknown-linux-musl/verification/xsh ] || [ ! -r "/work/core/$applet_source" ]; then test_status=125; else
    if [ "\$staged_hashes" -ne 1 ]; then test_status=125; else
      /usr/local/bin/date --version | grep -Fq '9.12' && /usr/local/bin/cp --version | grep -Fq '9.12'
      if [ "\$?" -ne 0 ]; then test_status=125; else
        XSH_CORE_COMPAT_REFERENCE_DIR=$reference /work/$harness_rel --ignored --test-threads=1 --nocapture $filter > /tmp/$label.test.log 2>&1
        test_status=\$?
        cat /tmp/$label.test.log
        if [ "\$test_status" -eq 0 ] && ! grep -Fq 'test result: ok. $expected_passed passed; 0 failed;' /tmp/$label.test.log; then test_status=125; fi
      fi
    fi
fi
printf 'XSH_QEMU_ACCEPTANCE_EXIT_${label}=%s\\n' "\$test_status"
sync
/sbin/poweroff -f
EOF
)
    else
        test_args="/work/$harness_rel --ignored --test-threads=1 --nocapture $filter"
        guest_command=$(cat <<EOF
set +e
cd /work || exit 125
unset XSH_CORE_COMPAT_REFERENCE_DIR
printf 'XSH_QEMU_STAGING_ID source_head=%s source_tree_oid=%s product_disk_sha256=%s\\n' '$source_head' '$source_tree_oid' '$product_disk_sha256'
if /bin/busybox sha256sum -c <<HASHES
$xsh_sha256  /work/target/x86_64-unknown-linux-musl/verification/xsh
$core_extra_sha256  /work/$core_extra_rel
$core_extra_privileged_sha256  /work/$core_extra_privileged_rel
HASHES
then staged_hashes=1; else staged_hashes=0; fi
if [ ! -x "/work/$harness_rel" ] || [ ! -x /work/target/x86_64-unknown-linux-musl/verification/xsh ] || [ ! -r "/work/core/$applet_source" ]; then test_status=125; else
    if [ "\$staged_hashes" -ne 1 ]; then test_status=125; else
        /work/$harness_rel --ignored --test-threads=1 --nocapture $filter > /tmp/$label.test.log 2>&1
        test_status=\$?
        cat /tmp/$label.test.log
        if [ "\$test_status" -eq 0 ] && ! grep -Fq 'test result: ok. $expected_passed passed; 0 failed;' /tmp/$label.test.log; then test_status=125; fi
    fi
fi
printf 'XSH_QEMU_ACCEPTANCE_EXIT_${label}=%s\\n' "\$test_status"
sync
/sbin/poweroff -f
EOF
)
    fi

    printf 'label=%s\nsource_head=%s\nsource_tree_oid=%s\nproduct_disk=%s\nproduct_disk_sha256=%s\nxsh_sha256=%s\ncore_extra_path=%s\ncore_extra_sha256=%s\ncore_extra_privileged_path=%s\ncore_extra_privileged_sha256=%s\nnonce=%s\nroot_overlay=%s\ntest_command=%s\n' \
        "$label" "$source_head" "$source_tree_oid" "$product_disk" "$product_disk_sha256" \
        "$xsh_sha256" "$core_extra_rel" "$core_extra_sha256" "$core_extra_privileged_rel" \
        "$core_extra_privileged_sha256" "$nonce" "$root_overlay" "$test_args" > "$run_dir/status.txt"
    export QEMU_RUN_DIR="$run_dir"
    export QEMU_ROOT_OVERLAY="$root_overlay"
    export QEMU_PRODUCT_DISK="$product_disk"
    export QEMU_NONCE="$nonce"
    export QEMU_SERIAL_LOG="$run_dir/serial.log"
    export QEMU_GUEST_STATUS="$run_dir/guest-status.txt"
    export QEMU_GUEST_COMMAND="$guest_command"
    export QEMU_GUEST_LABEL="$label"
    export QEMU_BIN=$(command -v qemu-system-x86_64)

    if expect <<'EXPECT'
proc stop_qemu {pid} {
    catch {exec /bin/kill -TERM $pid}
    after 2000
    catch {exec /bin/kill -KILL $pid}
}
set timeout 120
log_user 0
log_file -a -noappend $env(QEMU_SERIAL_LOG)
spawn -noecho env TMPDIR=$env(QEMU_RUN_DIR)/tmp $env(QEMU_BIN) \
    -no-user-config -nodefaults -no-reboot \
    -machine q35,accel=tcg -cpu max -smp 2 -m 2048 \
    -rtc base=2020-01-01T00:00:00,clock=vm \
    -kernel $env(QEMU_KERNEL) -initrd $env(QEMU_INITRAMFS) \
    -append "console=ttyS0,115200n8 xsh_disposable_nonce=$env(QEMU_NONCE)" \
    -drive file=$env(QEMU_ROOT_OVERLAY),if=none,id=root,format=qcow2 \
    -device virtio-blk-pci,drive=root \
    -drive file=$env(QEMU_PRODUCT_DISK),if=none,id=work,format=raw,readonly=on \
    -device virtio-blk-pci,drive=work \
    -nic none -display none -monitor none -serial stdio
set qemu_pid [exp_pid]
expect {
    -re {XSH_QEMU_GUARD_OK[^\r\n]*[[:space:]]ext4[[:space:]]} {}
    -re {XSH_QEMU_(INIT|GUARD)_FAIL[^\r\n]*} { puts stderr "guest rejected QEMU guard"; stop_qemu $qemu_pid; exit 1 }
    timeout { puts stderr "guest guard timed out"; stop_qemu $qemu_pid; exit 124 }
    eof { puts stderr "QEMU exited before the guest guard"; exit 125 }
}
after 1000
set guest_input [string map [list "\n" "\r"] "$env(QEMU_GUEST_COMMAND)\n"]
send -- $guest_input
set timeout 300
set result_pattern [format {XSH_QEMU_ACCEPTANCE_EXIT_%s=([0-9]+)} $env(QEMU_GUEST_LABEL)]
expect {
    -re $result_pattern {
        set guest_exit $expect_out(1,string)
        set status_file [open $env(QEMU_GUEST_STATUS) w]
        puts $status_file "guest_exit=$guest_exit"
        close $status_file
    }
    timeout { puts stderr "guest acceptance timed out"; stop_qemu $qemu_pid; exit 124 }
    eof { puts stderr "QEMU exited before guest status was captured"; exit 125 }
}
set timeout 30
expect {
    eof {}
    timeout { puts stderr "QEMU did not stop after guest poweroff"; stop_qemu $qemu_pid; exit 124 }
}
set wait_result [wait]
set qemu_exit [lindex $wait_result 3]
set status_file [open $env(QEMU_GUEST_STATUS) a]
puts $status_file "qemu_exit=$qemu_exit"
close $status_file
if {$guest_exit != 0} { exit $guest_exit }
if {$qemu_exit != 0} { exit 126 }
exit 0
EXPECT
    then
        expect_status=0
    else
        expect_status=$?
    fi

    printf 'launcher_exit=%s\n' "$expect_status" >> "$run_dir/status.txt"
    [[ -f "$QEMU_GUEST_STATUS" ]] || printf 'guest_exit=unavailable\n' > "$run_dir/guest-status.txt"
    printf 'evidence=%s\n' "$run_dir" >> "$run_dir/status.txt"
    if [[ $expect_status -ne 0 ]]; then
        printf 'QEMU acceptance run failed: %s; evidence: %s\n' "$label" "$run_dir" >&2
        return "$expect_status"
    fi
    printf 'QEMU acceptance run passed: %s; evidence: %s\n' "$label" "$run_dir"
}

export QEMU_KERNEL="$kernel"
export QEMU_INITRAMFS="$initramfs"
run_guest date-xsh "$core_extra_rel" date_clock:: "" 3
run_guest date-gnu "$core_extra_rel" date_clock:: /usr/local/bin 3
