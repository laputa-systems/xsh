# Linux QEMU evidence for source bb809e1a

These three disposable guests completed the final Linux platform checks for
source HEAD `bb809e1a670b49c66d59f4e4cbca7ca3f6680e16`, tree
`f2f8094c8f18f4fc20fa51a53ea87fa311b853f3`. All seven selected tests passed.

## Source and products

The verification products used the pinned Linux build image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
Cargo reported these executable paths from the frozen source mounted at
`/work`.

| Product | Cargo path | SHA-256 |
|---|---|---|
| XSH | `target/x86_64-unknown-linux-musl/verification/xsh` | `c493d0324bcd66f15ace3f71be690248b832fecffb795a10f29ec223fad32b02` |
| Ordinary harness | `target/x86_64-unknown-linux-musl/verification/build/xsh/409440b2a43657bc/out/core_compat_extra-409440b2a43657bc` | `66b35d1aeece51b28329abaa7cd8e652cbedfa68252a6af296d49842a1de739a` |
| Privileged harness | `target/x86_64-unknown-linux-musl/verification/build/xsh/cf73e16bb50a016e/out/core_compat_extra_privileged-cf73e16bb50a016e` | `fc7b7fdfab83fa2ea4cf92f5a7307a05ef31a8b12881d3fe6b47c0b5c1194e2b` |

The raw ext4 product disk was staged from that source and those executables.
Its scratch path was `.work/tmp/qemu-current/source-products.ext4`.
Its SHA-256 was
`8abd2f8128b3f7471126399513a0719214236e0dc7dbbbb0dc2fbd346d5f447f`.

## Guest setup and results

Each guest used q35 with TCG, `-cpu max`, two vCPUs, and 2048 MiB. QEMU used
RTC `2020-01-01T00:00:00` with `clock=vm`. QEMU had no NIC. It attached the
product disk read-only and used a fresh qcow2 root overlay and 256-bit nonce
for each boot.

The guest kernel was `7.2.9-2-stable`. The prepared kernel, initramfs, and
guest-root hashes were respectively
`f12a4ff57b432fef1290c649c389f19f7b80a2d9a960ccce0d2492a55b3708e7`,
`db6d679337f3a2136a034052a0725e22435688dd07695f91e44aa4cad37e4a26`, and
`45e3c9219fd2d9ab20344943579ba5d1a5881eb85619e255229ff59f5e495a31`.
The launchers verified the guest-root hash before and after the runs.

| Guest | Selected tests | Result |
|---|---|---|
| XSH date | `date_clock::test_date_set_valid`, `date_clock::test_date_set_valid_3`, `date_clock::test_date_set_valid_4` | 3 passed |
| GNU reference | The same three tests, with GNU Coreutils 9.12 | 3 passed |
| BTRFS reflink | `cp::privileged::test_cp_reflink_always_override` | 1 passed |

Each guest printed `XSH_QEMU_GUARD_OK` with `rtc_cmos`, root UID 0, and an
ext4 root. The guest hash checks passed in all three serial logs. Every guest,
QEMU process, and launcher exited 0.

The guest root's `e2fsck -fn` check exited 0. It reported one-block and
one-inode free-count differences and a clean orphan-file flag. The pinned
guest-root hash remained unchanged.

## Receipts and launchers

Each receipt directory contains the serial log, launcher status, and guest
status. The status file records the nonce, product identity, and exact test
command.

- [XSH date receipts](date-xsh/status.txt), [serial log](date-xsh/serial.log)
- [GNU reference receipts](date-gnu/status.txt), [serial log](date-gnu/serial.log)
- [BTRFS reflink receipts](reflink/status.txt), [serial log](reflink/serial.log)

The [stager](launchers/stage-products.sh) copies the frozen source's `core/`
tree and the exact Cargo-reported products into the read-only image.
The [date launcher](launchers/qemu-date-only.sh) runs the two date guests.
The [reflink launcher](launchers/reflink-one-guest.sh) runs the BTRFS case.
These scripts record the paths and QEMU setup used for this result.

Their SHA-256 values are:

| Script | SHA-256 |
|---|---|
| Stager | `6e66dfcb1980e513740db155ed2fe881f51b9b9197c01af0cf0636830d4f5851` |
| Date launcher | `ab3333be6313ccfe312a67b725909f7e9a5d4ef6c94094e0c5e8e76898f10617` |
| Reflink launcher | `50053e988a6f51f09c7e1bb8385e98778df0617903bce5eeecd58e25a0498aa7` |
