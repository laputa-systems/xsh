# Linux QEMU evidence for source 0dc9cff7

Three disposable guests completed the Linux platform checks for source HEAD
`0dc9cff7c2b16e4c201adb3abca11d7ebda8e1f4`, tree
`b4f9db3f8ee2f38a6c62fea876976619db424ed7`. All seven selected tests passed.

## Source products

The verification products used the pinned Linux build image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
The raw ext4 product disk was staged from this frozen source and the following
Cargo artifacts. The guest attached it read-only.

| Product | Cargo path | SHA-256 |
|---|---|---|
| XSH | `target/x86_64-unknown-linux-musl/verification/xsh` | `24136a2854e7b9c06dda7f2f008f169bb7eac36fa062660facd58aa771556a10` |
| Ordinary harness | `target/x86_64-unknown-linux-musl/verification/build/xsh/409440b2a43657bc/out/core_compat_extra-409440b2a43657bc` | `f1605b9dadc41b3beb89c65a777b0295bc260fcb1f29f415b79614e017c2c892` |
| Privileged harness | `target/x86_64-unknown-linux-musl/verification/build/xsh/cf73e16bb50a016e/out/core_compat_extra_privileged-cf73e16bb50a016e` | `57f977a8382840ef64184be4234b5d9cba208cf0536c9831dcce736df4c6d8eb` |

The staged disk was `.work/tmp/qemu-current/source-products.ext4` in the frozen
checkout and its SHA-256 was
`951eb22d8d8d15058745f893d8f0d5ffd3fdb99a0740d38530274864973ea87e`.

## Guest setup and results

Each guest used q35 with TCG, `-cpu max`, two vCPUs, and 2048 MiB. QEMU used
RTC `2020-01-01T00:00:00` with `clock=vm` and no NIC. Every boot used a fresh
qcow2 root overlay and 256-bit nonce. The kernel was `7.2.9-2-stable`; the
kernel, initramfs, and prepared guest-root hashes were respectively
`f12a4ff57b432fef1290c649c389f19f7b80a2d9a960ccce0d2492a55b3708e7`,
`db6d679337f3a2136a034052a0725e22435688dd07695f91e44aa4cad37e4a26`, and
`45e3c9219fd2d9ab20344943579ba5d1a5881eb85619e255229ff59f5e495a31`.
The launchers checked the guest-root hash before and after each run.

| Guest | Selected tests | Result |
|---|---|---|
| XSH date | `date_clock::test_date_set_valid`, `date_clock::test_date_set_valid_3`, `date_clock::test_date_set_valid_4` | 3 passed |
| GNU reference | The same three tests, using GNU Coreutils 9.12 | 3 passed |
| BTRFS reflink | `cp::privileged::test_cp_reflink_always_override` | 1 passed |

All guests reported `XSH_QEMU_GUARD_OK` with `rtc_cmos`, root UID 0, and an ext4
root. Their serial logs contain the expected source, tree, and disk IDs, all
staged product hash checks, and the test output. Each launcher, guest, and QEMU
process exited 0.

## Receipts and launchers

Each receipt directory contains `serial.log`, `status.txt`, and
`guest-status.txt`; the status records include the per-boot nonce and exact
test command.

- [XSH date receipts](date-xsh/status.txt) and [serial log](date-xsh/serial.log)
- [GNU date receipts](date-gnu/status.txt) and [serial log](date-gnu/serial.log)
- [BTRFS reflink receipts](reflink/status.txt) and [serial log](reflink/serial.log)

The launchers are the approved source-qualified stager and the two guest
launchers. Their SHA-256 values are:

| Script | SHA-256 |
|---|---|
| [Stager](launchers/stage-products.sh) | `6e66dfcb1980e513740db155ed2fe881f51b9b9197c01af0cf0636830d4f5851` |
| [Date launcher](launchers/qemu-date-only.sh) | `ab3333be6313ccfe312a67b725909f7e9a5d4ef6c94094e0c5e8e76898f10617` |
| [Reflink launcher](launchers/reflink-one-guest.sh) | `50053e988a6f51f09c7e1bb8385e98778df0617903bce5eeecd58e25a0498aa7` |
