# Linux QEMU evidence for source 0deeb9c7

Three disposable guests ran the selected Linux platform checks for source HEAD
`0deeb9c7fb79e52740c82a264754dbb29d8be693`, tree
`ffb7736e1c141bbacde71c7a036ef6bcf1433c34`. All seven selected tests passed.
After the terminal canonical run, all four staged product hashes matched the
terminal product hashes. The QEMU platform slice is qualified for this source
and these products. The separate canonical gate remains red and has no
acceptance credit; its receipt and logs are preserved below.

## Source products

The products were built with the pinned Linux image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
They came from the frozen checkout's `.work/target-final-canonical`, mounted as
`/work/target`. The stage helper checked each source product before copying,
checked each staged copy, then rechecked all four source hashes after writing
the disk. The canonical owner rehashed all four products after the terminal
run and confirmed that they still matched the staged hashes. The canonical
receipt independently lists matching terminal XSH and XSHT hashes. The staging
output is preserved in [stage receipt](stage-receipt.txt).

| Product | Cargo path | SHA-256 |
|---|---|---|
| XSH | `target/x86_64-unknown-linux-musl/verification/xsh` | `02639f0ee23780371ddc38c0c241d412adc9d79ae32b4cb722c51d74c05fb2c4` |
| XSHT | `target/x86_64-unknown-linux-musl/verification/xsht` | `5ba0907b1b5222f5dab0efa19496b711bb4036b14c0ab2c344d1d8987f7e8836` |
| Ordinary harness | `target/x86_64-unknown-linux-musl/verification/build/xsh/409440b2a43657bc/out/core_compat_extra-409440b2a43657bc` | `66b35d1aeece51b28329abaa7cd8e652cbedfa68252a6af296d49842a1de739a` |
| Privileged-feature harness | `target/x86_64-unknown-linux-musl/verification/build/xsh/a67ed2905136c9b7/out/core_compat_extra-a67ed2905136c9b7` | `bab2830527c40a57a3a498ee11327fe9609c5a98d1f67a8537f7e4e943f96980` |

The privileged-feature Cargo artifact is named `core_compat_extra`; the
canonical owner confirmed its test list contains `cp::privileged::*` and the
ordinary harness does not. The raw ext4 product disk is preserved in the frozen
checkout at `.work/tmp/qemu-current/source-products.ext4`. Its SHA-256 is
`90a07c74c5373529d9ae3cbe019576331631db20583c6d46f0b56f4fb24d763a`.

## Guest setup and results

Each guest used q35 with TCG, `-cpu max`, two vCPUs, and 2048 MiB. QEMU used
RTC `2020-01-01T00:00:00` with `clock=vm` and no NIC. Each boot had a fresh
qcow2 root overlay and 256-bit nonce. The kernel was `7.2.9-2-stable`; the
kernel, initramfs, and prepared guest-root hashes were respectively
`f12a4ff57b432fef1290c649c389f19f7b80a2d9a960ccce0d2492a55b3708e7`,
`db6d679337f3a2136a034052a0725e22435688dd07695f91e44aa4cad37e4a26`, and
`45e3c9219fd2d9ab20344943579ba5d1a5881eb85619e255229ff59f5e495a31`.

| Guest | Selected tests | Result |
|---|---|---|
| XSH date | `date_clock::test_date_set_valid`, `date_clock::test_date_set_valid_3`, `date_clock::test_date_set_valid_4` | 3 passed |
| GNU reference | The same three tests, using GNU Coreutils 9.12 | 3 passed |
| BTRFS reflink | `cp::privileged::test_cp_reflink_always_override` | 1 passed |

All three serial logs report `XSH_QEMU_GUARD_OK` with `rtc_cmos`, root UID 0,
and an ext4 root. They include the source, tree, disk, and four staged-product
hash identities. Every launcher, guest, and QEMU process exited 0. Their raw
serial and status receipts bind the seven passing cases to the staged disk.

## Canonical terminal receipt

The canonical batch ended with exit 3 and status `failed-or-incomplete`. The
native suite reported 13,915 passed, one failed, and 78 skipped. Its failure was
`core/tests/test-nc.xsh::test_nc_answers_telnet_negotiation`, which timed out
after two minutes. The source checks were clean before and after the run. The
receipt records terminal XSH and XSHT hashes equal to the staged hashes; the
canonical owner also rehashed the ordinary and privileged harnesses after the
run and confirmed both hashes matched.

- [Canonical replay receipt](canonical-replay.json), SHA-256 `8b4b5b4c56638c393dff0f0007d5e6a8558dab69b973fca303d5dd93698f1a83`
- [Canonical stdout](canonical/stdout.log)
- [Canonical stderr](canonical/stderr.log)
- [Canonical resource record](canonical/container-resources.json)

## Receipts and launchers

Each guest directory contains `serial.log`, `status.txt`, and
`guest-status.txt`; status records include its nonce and exact test command.
The launcher stdout files preserve the host-side run output.

- [XSH date receipts](date-xsh/status.txt) and [serial log](date-xsh/serial.log)
- [GNU date receipts](date-gnu/status.txt) and [serial log](date-gnu/serial.log)
- [BTRFS reflink receipts](reflink/status.txt) and [serial log](reflink/serial.log)
- [Date launcher stdout](date-launcher.stdout.log)
- [Reflink launcher stdout](reflink-launcher.stdout.log)

The scratch launchers validate the exact qualified Cargo artifact paths and
check all four product hashes in each guest. Their SHA-256 values are:

| Script | SHA-256 |
|---|---|
| [Stager](launchers/stage-products.sh) | `7bf7579354185c5fa733195c0137902271da196662c90d8a29012754166819c4` |
| [Date launcher](launchers/qemu-date-only.sh) | `c2e7e5b3265b0dd860035f9fa4b075e36dc65d5d3fadb673581d31131ae5c4c2` |
| [Reflink launcher](launchers/reflink-one-guest.sh) | `2c1dd70b58f0d758960e967ed341b8aa877705171470e8674e932569ae97f2c4` |
