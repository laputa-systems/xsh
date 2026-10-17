# Canonical gate evidence for b235a1a43

## Source and run

The canonical gate ran against clean source commit
`b235a1a43d06e44580fde4c1b8c58ff27a17ca78`, tree
`08402ddd079228e072f24642fdb82db5196fff11`. The receipt records a clean
source check before and after the run.

The official attempt was `canonical-b235a1a43-20261017-r1`, run by
`dev/consolidation/final_canonical_replay.py` in the pinned Linux musl image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
The container had 12 CPUs, 24 GiB memory, no swap, and no network. The runner
exited 0 and recorded `status: passed`. Its `acceptance_credit` remains
`none from this runner; the canonical gate result must be reviewed separately`.
Root independently reviewed and accepted the functional result. Other closure
gates remain open.

The [canonical receipt](canonical-replay.json), [stdout](stdout.log),
[stderr](stderr.log), and [container resource record](container-resources.json)
are byte-for-byte copies of the run output. The receipt's cleanup state is
resolved. The stop command found the container already absent, removal
succeeded, and the final container query returned no match.

## Bootstrap and terminal products

The source-bound bootstrap passed in attempt 3. Its
[receipt](bootstrap/bootstrap-receipt.json) records the exact Cargo command,
the pinned image, and clean source identity. The copied Cargo JSON stream has
172 records, including a successful `build-finished` event. The exit record is
zero.

At bootstrap completion, `verification/xsh` had SHA-256
`c493d0324bcd66f15ace3f71be690248b832fecffb795a10f29ec223fad32b02` and size
31,552,464 bytes. The original artifact remains at
`build/xsh/600f79e93b66ed79/out/xsh` in the canonical target. Later canonical
Cargo feature builds replaced the stable `verification/xsh` path. At the end
of the run, that path had SHA-256
`02639f0ee23780371ddc38c0c241d412adc9d79ae32b4cb722c51d74c05fb2c4` and size
31,645,688 bytes. The official `products_after` records this terminal hash,
which is the product used at the stable path during native testing. The
bootstrap receipt remains unchanged and records the earlier hash.

The official `products_after` contains the three CLI products. The separate
harness hashes and sizes below were re-read from the frozen canonical target
after the gate. All five terminal products match the identities checked by
root and the stable-build seam.

| Product | Path below `target-final-canonical` | Size | SHA-256 |
|---|---|---:|---|
| `xsh` | `x86_64-unknown-linux-musl/verification/xsh` | 31,645,688 | `02639f0ee23780371ddc38c0c241d412adc9d79ae32b4cb722c51d74c05fb2c4` |
| `xsht` | `x86_64-unknown-linux-musl/verification/xsht` | 38,738,400 | `5ba0907b1b5222f5dab0efa19496b711bb4036b14c0ab2c344d1d8987f7e8836` |
| `xshi` | `x86_64-unknown-linux-musl/verification/xshi` | 587,320 | `5121c56402853320e396695d6854336067fa9754fcc957bdcdd98ffe74fcea53` |
| ordinary `core_compat_extra` | `x86_64-unknown-linux-musl/verification/build/xsh/409440b2a43657bc/out/core_compat_extra-409440b2a43657bc` | 2,062,576 | `66b35d1aeece51b28329abaa7cd8e652cbedfa68252a6af296d49842a1de739a` |
| `core_compat_extra` with `linux-priv-tests` | `x86_64-unknown-linux-musl/verification/build/xsh/a67ed2905136c9b7/out/core_compat_extra-a67ed2905136c9b7` | 2,187,512 | `bab2830527c40a57a3a498ee11327fe9609c5a98d1f67a8537f7e4e943f96980` |

The two harness executables share the Cargo target name. Their build directories
and hashes distinguish the default and `linux-priv-tests` variants. Raw
terminal outputs are in [harness-products.sha256](harness-products.sha256) and
[harness-products.sizes](harness-products.sizes).

## Gate results

| Stage | Result |
|---|---|
| `xsh --test integration` | 223 passed, 0 failed, 1 filtered |
| `ambient_fs_policy` | 0 passed, 1 ignored |
| `symbol_plateau` | 1 passed, 0 failed |
| `core_compat_boundaries` | 23 passed, 0 failed |
| ordinary `core_compat_extra` | 58 passed, 0 failed, 3 ignored, 2 filtered |
| XSH library tests | 748 passed, 0 failed, 3 ignored |
| XSHT integration | 117 passed, 0 failed, 5 filtered |
| soundness | 14 passed, 0 failed |
| `core_compat_extra` with `linux-priv-tests` | 82 passed, 0 failed, 4 ignored, 4 filtered |
| `linux_priv` | 10 passed, 0 failed |
| `tests.native` | 13,916 passed, 0 failed, 78 skipped |
| project docs tests | 3 passed, 0 failed, 0 skipped |

The runner completed `batch-check`, `batch-lint`, `batch-diff`, and
`batch-ratchets`. `xsht lint` reported one finding and 113 notes, then returned
successfully. The ratchet output records approved source-total changes from
189,274 to 215,122 lines in `src` and from 49,968 to 53,234 lines in XSHT. The
configured ceilings were 193,749 and 50,367 respectively. The docs project
tests passed 3 of 3.

The 11 ignored Rust cases are recorded in stdout. They cover the ambient
filesystem allowlist test, three date-setting tests that require a disposable
QEMU guest in each compatibility harness, the netlink capture/replay test, the
manual W9 resource-attribution test, the cold-start profile test, and the
privileged BTRFS reflink test. The QEMU lane records its reuse boundary
separately; this run does not claim a new b235 guest run.

## Native skips

The [78 extracted rows](native-skips.log) retain each selector and the reason
printed by the native run. The [36 residual-owned rows](native-skips-residual.log)
match the immutable residual selector list: 32 root, identity, ownership, or
supplementary-group cases; one raw ICMP socket case; one Unix socket case whose
scratch path exceeds `sun_path`; and two traceroute raw-networking cases. These
remain uncovered by this native context and belong to the residual Linux gate.

The remaining [42 rows](native-skips-environment.log) record unavailable
fixtures, tools, locales, devices, filesystem features, and platform cases,
plus explicit skip or already-covered test behavior. Examples include absent
DNS, TLS, and HTTP fixtures; missing oracle tooling; unavailable locales or
`/dev/sda1`; the unavailable file-capability fixture; and unsupported reflink
behavior. Skips are not passes. The raw list preserves the exact rows for
residual and acceptance review.

## Interpretation

This run supplies the canonical functional result for the pinned source. It
does not close the residual Linux coverage or other campaign gates. The raw
duration in the receipt is not performance evidence because other approved
campaign work overlapped the run.
