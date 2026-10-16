# W9 owner projection failure at be0b

## Result

The be0b reference exact-owner validator stopped at input 0, `bench/consolidation/xsh/directory.xsh`. Its exact owner sidecar contains the AST owner for `StmtId(1)`. The adapter sidecar has the same statement row but no `owner` rows. The strict statement-to-AST-owner check rejects the adapter sidecar, so no reference proof receipt was written. Candidate proof validation and assignment capture did not start.

The native exact-owner hash selector passed once over all 1,690 inputs in the one-CPU, 1 GiB worker. The full validator then failed while parsing owner 0. The failed status and complete validator log are retained in `proof-attempt-status.txt` and `proof-validation.log`.

## Pinned inputs

The schema-4 source receipt pins controller and candidate commit `be0b79a48403d74b11186166ba739a81dcbed702`, tree `cc80212504507d34c7d07413ce33f42ea68b6028`. The reference compiler and shared program snapshot use e240 commit `e240efad273fe35d66d91f4037b6a8193527da2e`, tree `a1ef04ac7b6f759c272e8f841acee3a3b3a6ff08`.

The receipt pins the 1,690-input manifest SHA-256 `1ec4e8eb67681591d25c69b395387ba4acbb4bfe1e4ab514a858e60ff1a56305`. Its source receipt SHA-256 is `00b4b4d2c94d832212ae5e94428fec08363771c470373e5fc820bc73739ebf53`.

The failed validator used the be0b exact-owner consumer fix `6979f16880469d3751e37410092672bcfdeabcbe` with the unchanged be0b main bootstrap. Both role dry-runs passed for 1,690 owners and 68 ranges. The reference exact capture receipt is SHA-256 `15f2cef64db7182731a2d519936656d5b3af50720de4d97d2d9bb46991d8c88d`. The reference adapter capture receipt is SHA-256 `fcf9504b8da402ccb6726c0e4e872568b00f08d03ac2511c40569f662b622110`.

The source receipt, both reference build receipts, both capture receipts, and their raw owner-0 sidecars are copied here. The ignored full captures remain unchanged under `.work/consolidation/w9-final-be0b-20261016T1711Z`.

## Owner 0 evidence

`directory.xsh` is 337 bytes. `StmtId(1)` spans bytes 25 through 51 and has kind `stmt:LetExprNoTy`. `exact-owner-0.tsv` has 115 lines and includes `owner Span { source_id: SourceId(0), start: 25, len: 27 } stmt:LetExprNoTy`. `adapter-owner-0.tsv` has 17 lines and includes the `StmtId(1)` statement row, but no `owner` rows. Their SHA-256 hashes are `96928040fc73deaf1aced63273c95ca0c48d18d9849691eb15c9e08f5ce3135e` and `249e2bb21da20f29731f9fa6b0a560f951ab9107ec23fb6dfce88d9fd024e363`, respectively.

The omission comes from `_derived_owner_helper_bytes` in `w9_publication_adapter_runner.py`. The transformation introduced at `f2e6cd2ca` replaced the `owner_rows` write loop with only `flush` and `sync_all`. The pinned raw owner helper still collects AST owners and emits those rows. This repair keeps that loop in the derived helper before flushing and syncing the output.

## Retained files

- `source-cohort.receipt`
- `reference-adapter-build.receipt`
- `reference-exact-build.receipt`
- `reference-adapter-capture.receipt`
- `reference-exact-capture.receipt`
- `adapter-owner-0.tsv`
- `exact-owner-0.tsv`
- `directory.xsh`
- `proof-attempt-status.txt`
- `proof-validation.log`

The incomplete be0b attempt is historical. The corrected helper changes the producer bytes, so these captures cannot qualify a later source pin.
