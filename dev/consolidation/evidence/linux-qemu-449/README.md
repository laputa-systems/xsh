# Linux QEMU evidence for source 449

These receipts cover the privileged mount worker name regression and seven QEMU acceptance assertions against source commit `449320b1844cb5c7cc361df2fe27e387b37b12d1`, tree `b1f2632d9fc3b48ceb5747843a6631db6e0aeb1c`. The source worktree was clean for the runs.

## Source and products

| Product | Cargo-reported path | SHA-256 |
|---|---|---|
| Default XSH | `target/x86_64-unknown-linux-musl/verification/xsh` | `7fa9a4e517e7375af102b1e539429d1ba055201b2018f0028a8cbb079576567a` |
| Ordinary privileged-feature harness | `target/x86_64-unknown-linux-musl/verification/build/xsh/a67ed2905136c9b7/out/core_compat_extra-a67ed2905136c9b7` | `bab2830527c40a57a3a498ee11327fe9609c5a98d1f67a8537f7e4e943f96980` |
| Dedicated privileged harness | `target/x86_64-unknown-linux-musl/verification/build/xsh/cf73e16bb50a016e/out/core_compat_extra_privileged-cf73e16bb50a016e` | `fc7b7fdfab83fa2ea4cf92f5a7307a05ef31a8b12881d3fe6b47c0b5c1194e2b` |
| Read-only product disk | `.work/consolidation/qemu/source-products.ext4` | `60a6aa78b462893fbf3fa98555627605548e17c550ee4d4a291e8504bc835127` |

The QEMU launchers use q35 with TCG, two vCPUs, 2048 MiB, a fixed guest RTC, no network device, and the product disk attached read-only. The guest root runs from a fresh overlay for each boot. The GNU date reference is `/usr/local/bin/date` in the prepared guest.

## Passing checks

The ordinary and dedicated harnesses each passed `test_mount_worker_name_matches_current_test` once in container image `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`. Their exact libtest names differ because the dedicated harness wraps the crate name:

| Harness | Exact test name | Result |
|---|---|---|
| Ordinary | `cp::privileged::test_mount_worker_name_matches_current_test` | 1 passed |
| Dedicated | `core_compat_extra::cp::privileged::test_mount_worker_name_matches_current_test` | 1 passed |

The QEMU runs passed these seven assertions, each in a fresh guest. Each passing guest and its QEMU process exited 0, and the per-guest launcher status was 0. The three-guest controller's final status is described below.

| Guest | Assertions | Result |
|---|---|---|
| XSH date | `date_clock::test_date_set_valid`, `date_clock::test_date_set_valid_3`, `date_clock::test_date_set_valid_4` | 3 passed |
| GNU date reference | The same three `date_clock` cases | 3 passed |
| Reflink | `core_compat_extra::cp::privileged::test_cp_reflink_always_override` | 1 passed |

The three-guest launcher ran the date guests successfully, then exited 101 when its reflink guest failed on missing `mkfs.btrfs`. The reflink result above comes from a separate fresh guest using the same source, product disk, harnesses, and exact test selector. Its command-scoped exported `PATH` was `/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`; the guest serial log prints that value before running the harness. The test then passed 1/1, and the guest, QEMU process, and one-guest launcher all exited 0.

## Failed attempts

`reflink-selector-miss-ed908/` records an earlier run on source `ed90857bdd825439159b7d788ea5d57cd9652920`, tree `efc56a4a81c7993e35b2fd76a2549742eb8d8847`, with a different product disk and binaries. The outer test used the selector `cp::privileged::test_cp_reflink_always_override`; the wrapped test's actual name was `core_compat_extra::cp::privileged::test_cp_reflink_always_override`. Its assertion reported that the mount worker did not run. The guest exited 101 while QEMU exited 0. This is a failed selector attempt on the older source, not a result for source 449.

`reflink-path-failure-449/` records the first reflink attempt on source 449 and its exact products. The single test failed with `BLOCKED: fixture tool mkfs.btrfs: No such file or directory (os error 2)`. The guest exited 101 and QEMU exited 0. The guest shell had a synthesized `PATH` that it did not export to the harness child, so the child could not resolve the executable under `/sbin`. The passing retry exported the pinned guest tool directories only for the test command and logged the inherited value.

## Raw records

`date-xsh/` and `date-gnu/` contain serial and exit-status records. `reflink-path-failure-449/`, `reflink-path-pass-449/`, and `reflink-selector-miss-ed908/` preserve the reflink attempts and their distinct identities. `name-regression/` contains exact test names, list output, invocation status, and harness identity. `launchers/three-guest.sh` is the controller used for the date batch and initial reflink attempt; `launchers/reflink-one-guest.sh` is the corrected one-guest retry. Their hashes are respectively `d05c5fd5a4d28cd3aa8b14228b60350d0e10bcabd12a6ac21c6ea9fc9bc14e19` and `ee907a36dafffbd6bd2f2e2636b0c211afee8a09a70e3bea7cbf07e3ea2a515c`.
