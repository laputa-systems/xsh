# Route receipt review: `routes-ed908-target-layout`

## Finding

**GO for the 184 expression route tuples in this receipt.** The existing collector accepts all 184 after the runner used the canonical target path; no validator changes or relaxed checks were needed. The prior `routes-ed908` receipt remains a historical HOLD: its `/repo/.work/target-final-canonical/.../xsht` argv did not match the collector’s required `target/x86_64-unknown-linux-musl/verification/xsht` suffix. The fresh run uses `/artifacts/target/x86_64-unknown-linux-musl/verification/xsht` and its sibling `xsh`, mounted read-only.

## Independent checks

- Receipt: `.work/final-review-evidence/final-correctness-gates/routes-ed908-target-layout/receipt.json`; status `completed`, 184 unique exact-test steps, each exit 0 and not timed out. There are 368 before/after source checks.
- Freeze: HEAD `ed90857bdd825439159b7d788ea5d57cd9652920`, tree `efc56a4a81c7993e35b2fd76a2549742eb8d8847`; current checkout matches and is clean. Every source check binds that commit/tree and reports clean.
- Products: receipt before/after hashes agree with current files: `xsh` `e0062e3280732626ebc3b2aa3eabe09ec3400156f112e0458e6694b9136ee9fa`; `xsht` `29c80afd1c5ca7243fc4dea60355a273cc5883ffa3d9f1d2949069f04e2093fa`.
- The pinned schema has 152 tags and the reviewed mapping has 152 unique tags. Exactly 92 Expr mappings have reviewed child source, each bound to the configured pinned source commit and current whole-file bytes; each extracted child equals its configured bytes and source hash.
- For each of 92 tags in normal and forced mode, I checked the unique exact `xsht test --exact <ID>` step, forced marker, successful status, artifact reference and digest, log digest, and the artifact’s one record against ID, child hash, tag, ordinal, expected route, and mode. This yields 184 distinct tuple claims and artifacts.
- The retained collector `route_witness_receipts` in `dev/consolidation/winddown/gate-tools/semantic-route-matrix/generate.py` returns exactly 184 tuples for this checkout and product hash. It scans the receipt corpus, so this confirms acceptance while the earlier path-mismatched receipt remains present and uncredited.
- Driver SHA-256: `9b76d6503a5bbd9653d0bb57de0f71151ba5c945a4fb97659eb70cdad40ddafe`. Mapping TSV SHA-256: `d0f4084f9f3eccbdb7192cdc815ce47858156cd2eb00043bb99c9f0ac9f75e7b`.

## Scope

This closes route-witness evidence only for the 92 Expr tags in normal and forced modes. It does not credit the 60 non-Expr mappings, establish source-order behavior, or establish canonical/full-native acceptance. No builds, tests, or native commands were run for this review; validation used the existing pure receipt and artifact validators plus read-only checks.
