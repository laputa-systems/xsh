#!/usr/bin/env bash
set -euo pipefail
[[ $# == 2 ]] || { printf 'usage: dd-block-fixture.sh XSHT XSH\n' >&2; exit 2; }
[[ $(id -u) == 0 ]] || { printf 'dd-block-fixture: requires root in a disposable privileged container\n' >&2; exit 2; }
[[ ! -e /dev/sda1 && ! -L /dev/sda1 ]] || { printf 'dd-block-fixture: /dev/sda1 already exists\n' >&2; exit 2; }
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
xsht="$(realpath "$1")"
xsh="$(realpath "$2")"
scratch="$(mktemp -d "${TMPDIR:?set TMPDIR to an owned scratch directory}/dd-block.XXXXXX")"
loop_device=''
created_node=0
cleanup() {
    status=$?
    trap - EXIT
    if (( created_node )); then rm -- /dev/sda1 || status=1; fi
    if [[ -n "$loop_device" ]] && ! losetup -d "$loop_device"; then
        printf 'dd-block-fixture: detach failed; retain %s\n' "$scratch" >&2
        exit 1
    fi
    rm -r -- "$scratch" || status=1
    exit "$status"
}
trap cleanup EXIT
truncate -s 4K "$scratch/device.img"
printf 'aBcD' | dd of="$scratch/device.img" conv=notrunc status=none
loop_device="$(losetup --find --show "$scratch/device.img")"
read -r major_hex minor_hex < <(stat -c '%t %T' "$loop_device")
mknod /dev/sda1 b "$((16#$major_hex))" "$((16#$minor_hex))"
created_node=1
cd "$repo_root"
export CARGO_BIN_EXE_xsh="$xsh" CARGO_BIN_EXE_xsht="$xsht"
"$xsht" test --jobs 1 --exact-id core/tests/test-uu-dd-2.xsh::test_uu_dd_seek_past_dev --exact-id core/tests/test-uu-dd-2.xsh::test_uu_dd_skip_past_dev
for conversion in plain converted; do
    options=()
    expected=Bc
    if [[ "$conversion" == converted ]]; then options+=(conv=ucase); expected=BC; fi
    "$xsh" core/dd.xsh -- bs=1 skip=1 count=2 status=none "${options[@]}" < /dev/sda1 > "$scratch/stdin.out"
    [[ $(cat "$scratch/stdin.out") == "$expected" ]]
    "$xsh" core/dd.xsh -- bs=1 skip=1 count=2 status=none if=/dev/sda1 "${options[@]}" > "$scratch/named.out"
    [[ $(cat "$scratch/named.out") == "$expected" ]]
    printf '%s valid stdin and named block skips pass\n' "$conversion"
done
