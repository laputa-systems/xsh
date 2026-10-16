# Route receipt review: `routes-ed908`

## Finding

**HOLD for route credit; GO for the underlying 184 executions as recorded.** The receipt contains 92 expression tags in both normal and forced modes, and every artifact reviewed matches the pinned mapping. The existing matrix collector currently rejects all 184 steps because its executable-path suffix check does not accept the runner's actual pinned product path. No receipts should be credited until that compatibility issue is resolved without weakening product identity checks.

## Source, mapping, and products

- Run: `.work/final-review-evidence/final-correctness-gates/routes-ed908/receipt.json`; status `completed`, 184 unique steps, outer status 0 for every step, no timeouts.
- Source freeze: `ed90857bdd825439159b7d788ea5d57cd9652920`, tree `efc56a4a81c7993e35b2fd76a2549742eb8d8847`. All 368 per-step before/after source checks report this identity, clean status, and an empty worktree. The present checkout also reports this clean identity.
- Route policy: `evidence.json` pins child test files to `95488faf20621dd2fd182e840e4ff5fafdb0abfd`. The replay driver SHA-256 is `56d18314a2b040180d901f4c526acf830c6b793386bcc5941952675083851fff`; its 92-row input table SHA-256 is `d0f4084f9f3eccbdb7192cdc815ce47858156cd2eb00043bb99c9f0ac9f75e7b`, matching the driver's pin.
- Products: `xsh` verification SHA-256 `e0062e3280732626ebc3b2aa3eabe09ec3400156f112e0458e6694b9136ee9fa`; `xsht` verification SHA-256 `29c80afd1c5ca7243fc4dea60355a273cc5883ffa3d9f1d2949069f04e2093fa`. Both receipt before/after hashes match each other and the present on-disk files. The recorded environment is pinned Linux/amd64 image, 8 CPUs, 16 GiB, no swap, no runtime preload; each command uses network none.

## Artifact audit

I used the existing schema parser, route mapping validator, and artifact matcher as read-only validators; no tests, builds, or native commands were run. The 92 TSV rows match all 92 live Expr schema tags and ordinals, the official mapping's test IDs and normal/forced expectations, and the exact extracted child source bytes. Each extracted child SHA-256 matches both the table and configured child bytes. All 184 steps run exactly one `xsht test --exact <mapped-test-id>` using the pinned verification binaries; test-ID context and forced marker match the mode. Each step has one original captured artifact, and its retained copy is byte-identical. All 184 artifact digests and log digests match the receipt. The one record in each artifact matches mapped test ID, child source hash, tag, ordinal, expected route, and mode.

## Credit blocker

The existing consumer at `dev/consolidation/winddown/gate-tools/semantic-route-matrix/generate.py:4716-4723` only recognizes an `xsht` argument ending in `target/x86_64-unknown-linux-musl/verification/xsht`. Every recorded command instead executes `/repo/.work/target-final-canonical/x86_64-unknown-linux-musl/verification/xsht`, the exact product whose hash is pinned above. Calling the existing `receipt_step_matches` and `route_witness_receipts` validators accepts 0 of 184 steps/tuples for this path mismatch. This is a validator/launcher path-contract mismatch, not a failing route artifact. Keep the validated raw artifacts uncredited until the consumer accepts this exact frozen product path or the same products are executed through a path already accepted by the strict consumer.

These receipts cover only the 92 Expr tags and 184 route tuples. They do not cover the 60 unsupported-format non-Expr rows, source-order comparisons, or establish that the canonical/full-native suite passed. The separate canonical failure remains outside this route-only review.
