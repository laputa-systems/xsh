# QEMU evidence scope for b235a1a4

The seven Linux QEMU cases have not been rerun for source `b235a1a43d06e44580fde4c1b8c58ff27a17ca78`, tree `08402ddd079228e072f24642fdb82db5196fff11`. The prior QEMU proof remains pinned to source `0deeb9c7fb79e52740c82a264754dbb29d8be693`; see [its README](../linux-qemu-0deeb9c7/README.md) and guest receipts.

Phase A of the [campaign policy](../../CAMPAIGN.md#phase-a--finish-the-existing-campaign-without-expanding-it) allows earlier evidence when its semantic scope is unchanged. The source diff from `0deeb9c7` to `b235a1a4` changes only `core/tests/test-nc.xsh` and `core/tests/test-uu-tail-1.xsh` under `core`, plus campaign documentation and evidence. It does not change the `date` or `cp` applet sources, platform fixtures, or product build inputs used by the prior QEMU cases.

At the stable-build seam, the four b235 products had the same hashes as the products bound to the seven prior guest passes. This makes the old guest proof applicable only if the canonical run's terminal `products_after` hashes still match all four entries below.

| Product | Cargo-relative path | Stable-build SHA-256 |
|---|---|---|
| XSH | `target/x86_64-unknown-linux-musl/verification/xsh` | `02639f0ee23780371ddc38c0c241d412adc9d79ae32b4cb722c51d74c05fb2c4` |
| XSHT | `target/x86_64-unknown-linux-musl/verification/xsht` | `5ba0907b1b5222f5dab0efa19496b711bb4036b14c0ab2c344d1d8987f7e8836` |
| Ordinary harness | `target/x86_64-unknown-linux-musl/verification/build/xsh/409440b2a43657bc/out/core_compat_extra-409440b2a43657bc` | `66b35d1aeece51b28329abaa7cd8e652cbedfa68252a6af296d49842a1de739a` |
| Privileged-feature harness | `target/x86_64-unknown-linux-musl/verification/build/xsh/a67ed2905136c9b7/out/core_compat_extra-a67ed2905136c9b7` | `bab2830527c40a57a3a498ee11327fe9609c5a98d1f67a8537f7e4e943f96980` |

The canonical owner must confirm terminal `products_after` equality for XSH, XSHT, the ordinary harness, and the privileged-feature harness. Until then, b235 does not inherit the QEMU qualification. No products were staged and no b235 guest was launched. A matching comparison will reuse the old XSH date, GNU Coreutils 9.12 date, and BTRFS reflink receipts without changing their 0de source pin. This platform-scope result does not determine the canonical suite result.
