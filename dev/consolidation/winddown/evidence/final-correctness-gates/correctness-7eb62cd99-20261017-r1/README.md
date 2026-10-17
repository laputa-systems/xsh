# Successful 7eb62 supplemental run

This archive preserves the fresh R6 run on source and tool commit `7eb62cd993e578c13bf6befd68a6a6ac15133c78`, tree `736b4bc16b1bcf07a0250b97e849506c19fb0753`. The receipt and thirteen combined step logs are byte-identical copies from the ignored output directory. Their paths still point to that original run. No binaries or Cargo targets are archived.

## Result

All thirteen approved R6 steps exited zero. The run verified the pinned Linux image, built the verification products and three harnesses, passed workspace warning-denied Clippy, staged `xsht`, and passed all five Linux contexts. The final tracked diff check passed. Source and tool checks remained clean at the expected commit and tree before and after the run.

The receipt keeps the runner's original `commands-passed-needs-privilege-skip-and-close-review` status. Root separately reviewed the command results and current skip coverage. The receipt was not edited after that review.

| Linux context | Passed | Failed | Ignored, skipped, or filtered |
|---|---:|---:|---|
| `core-compat-boundaries` | 23 | 0 | 0 |
| `core-compat-extra` | 58 | 0 | 3 ignored, 2 filtered |
| `split::` under 0.01 CPU and 1 GiB | 2 | 0 | 61 filtered |
| Privileged recursive block-device `cp` | 2 | 0 | 88 filtered |
| UID-1000 traceroute | 18 | 0 | 2 capability skips |

The ignored tests are `date_clock::test_date_set_valid`, `date_clock::test_date_set_valid_3`, and `date_clock::test_date_set_valid_4`. Root accepted reuse of the [seven-case QEMU proof for source 0deeb9c7](../../../../evidence/linux-qemu-0deeb9c7/README.md) under [Phase A's scope rule](../../../../CAMPAIGN.md#phase-a-finish-the-existing-campaign-without-expanding-it). Three XSH date cases, three GNU date cases, and one BTRFS reflink case passed. Those guest results used the canonical XSH product with SHA-256 `02639f0ee23780371ddc38c0c241d412adc9d79ae32b4cb722c51d74c05fb2c`.

The UID-1000 skips are `test_traceroute_gateway_adds_a_source_route`, which needs `CAP_NET_RAW` for `IP_OPTIONS`, and `test_traceroute_tcp_syn_reaches_open_and_closed_ports`, which needs a raw ICMP socket. These skips earn no pass credit. The queued Linux residual lane owns positive coverage for both cases.

## Product identities

The R6 products and the canonical products are different build variants. Root reviewed the variants separately. The hashes below identify the artifacts used by this R6 run. They do not claim byte equality with canonical products.

| Artifact | SHA-256 |
|---|---|
| `xsh-verification` | `d817ed7e8e43e5b7db5670068eb6fe76186178416233360aa535a5d07b5917af` |
| `xsht-verification` | `4e0322037add1138f0e3a67f6696e3b27e7ae60ea794f198e0c5ea0d6bae8a6e` |
| `xshi-verification` | `5121c56402853320e396695d6854336067fa9754fcc957bdcdd98ffe74fcea53` |
| `xsh-fuzz-verification` | `0bae1ce255bbbee5462c9a82b399d01b1f6198a353c8ba0e5c9268fdee3d455d` |
| `xsh-test-helper-verification` | `5687f4a562fe6c35e76d4921032d8293303f896bdfc94cd0ae542fe73f0a653d` |
| `core_compat_boundaries` harness | `5cf6a4618c55c16faf882fdd565da0d95137af114481e91f5d286c9b82c3fe2e` |
| `core_compat_extra` harness | `ad40026e2165b98fa3521a9b59e08045bbe82ef9709819cdcbec018058b64a47` |
| `core_compat_extra_privileged` harness | `fc544b139f92dcc4e98d23d80e62d46af2021f4abf7d2ed8cba037c1a3e822a9` |

The product hashes stayed unchanged from the initial build through harness builds and Linux contexts. The canonical `xsh-verification` and `xsht-verification` hashes are `02639f0ee23780371ddc38c0c241d412adc9d79ae32b4cb722c51d74c05fb2c4` and `5ba0907b1b5222f5dab0efa19496b711bb4036b14c0ab2c344d1d8987f7e8836`. They belong to the canonical product variant. The QEMU proof uses that canonical XSH product, not the R6 XSH product.

## Preserved files

- [Unchanged receipt](receipt.json)
- [Approved R6 manifest](manifest-correctness-bb809e1a-20261016-nodev-tmpdir-r6.json)
- [Pinned image check](logs/001-verify-pinned-image-id.log)
- [Verification product build](logs/002-build-product-01-xsh-xsht-xshi-xsh-fuzz.log)
- [Boundary harness build](logs/003-build-harness-core-compat-boundaries.log)
- [Extra harness build](logs/004-build-harness-core-compat-extra.log)
- [Privileged harness build](logs/005-build-harness-core-compat-extra-privileged.log)
- [Workspace warning-denied Clippy](logs/006-warning-deny-01.log)
- [`xsht` staging](logs/007-stage-core-compat-extra-xsht.log)
- [Boundary context](logs/008-linux-special-01-core-compat-boundaries.log)
- [Extra context](logs/009-linux-special-02-core-compat-extra.log)
- [Split context](logs/010-linux-special-03-core-compat-extra.log)
- [Privileged `cp` context](logs/011-linux-special-04-core-compat-extra-privileged.log)
- [UID-1000 traceroute context](logs/012-linux-special-05-traceroute-unprivileged.log)
- [Final tracked diff check](logs/013-tracked-diff-check.log)

After the launcher exited, its container-ID directory was empty and the unresolved-cleanup marker was absent. A nonblocking post-run probe acquired both shared locks.
