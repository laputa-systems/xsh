#!/bin/sh
set -eu
failure_status=2
[ "$#" -eq 2 ] || { printf 'usage: dd-block-fixture.sh XSHT XSH\n' >&2; exit "$failure_status"; }
uid=$(id -u)
[ "$uid" = 0 ] || { printf 'dd-block-fixture: requires root in a disposable privileged container\n' >&2; exit "$failure_status"; }
[ ! -e /dev/sda1 ] && [ ! -L /dev/sda1 ] || { printf 'dd-block-fixture: /dev/sda1 already exists\n' >&2; exit "$failure_status"; }
repo_root=$(cd "$(dirname "$0")/../.." && pwd -P)
xsht="$(realpath "$1")"
xsh="$(realpath "$2")"
scratch="$(mktemp -d "${TMPDIR:?set TMPDIR to an owned scratch directory}/dd-block.XXXXXX")"
loop_device=''
created_node=0
cleanup() {
    status=$?
    trap - EXIT
    if [ "$created_node" -eq 1 ]; then rm /dev/sda1 || status=1; fi
    if [ -n "$loop_device" ] && ! losetup -d "$loop_device"; then
        printf 'dd-block-fixture: detach failed; retain %s\n' "$scratch" >&2
        exit 1
    fi
    rm -r -- "$scratch" || status=1
    exit "$status"
}
trap cleanup EXIT
signal_status=0
trap 'signal_status=130; exit "$signal_status"' 2
trap 'signal_status=143; exit "$signal_status"' 15
truncate -s 4K "$scratch/device.img"
printf 'aBcD' | dd of="$scratch/device.img" conv=notrunc status=none
loop_device="$(losetup --find --show "$scratch/device.img")"
device_numbers=$(python3 "$(dirname "$0")/device_numbers.py" "$loop_device")
major=${device_numbers%% *}
minor=${device_numbers#* }
mknod /dev/sda1 b "$major" "$minor"
created_node=1
cd "$repo_root"
export CARGO_BIN_EXE_xsh="$xsh" CARGO_BIN_EXE_xsht="$xsht"
"$xsht" test --jobs 1 --exact-id core/tests/test-uu-dd-2.xsh::test_uu_dd_seek_past_dev --exact-id core/tests/test-uu-dd-2.xsh::test_uu_dd_skip_past_dev
for conversion in plain converted; do
    expected=Bc
    if [ "$conversion" = converted ]; then
        "$xsh" core/dd.xsh -- bs=1 skip=1 count=2 status=none conv=ucase < /dev/sda1 > "$scratch/stdin.out"
        expected=BC
        "$xsh" core/dd.xsh -- bs=1 skip=1 count=2 status=none if=/dev/sda1 conv=ucase > "$scratch/named.out"
    else
        "$xsh" core/dd.xsh -- bs=1 skip=1 count=2 status=none < /dev/sda1 > "$scratch/stdin.out"
        "$xsh" core/dd.xsh -- bs=1 skip=1 count=2 status=none if=/dev/sda1 > "$scratch/named.out"
    fi
    stdin_output=$(cat "$scratch/stdin.out")
    [ "$stdin_output" = "$expected" ]
    named_output=$(cat "$scratch/named.out")
    [ "$named_output" = "$expected" ]
    printf '%s valid stdin and named block skips pass\n' "$conversion"
done
