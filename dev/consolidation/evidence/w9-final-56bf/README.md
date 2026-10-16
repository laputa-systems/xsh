# W9 final attempt at 56bf

## Result

The source-sealed run completed both adapter roles, both exact-owner roles, the derived reference, and assignment-facts proofs for all 1,690 inputs. Each adapter capture and the derived-reference capture covers 68 ranges and 6,760 outcomes; both exact-owner captures and the assignment-facts proof cover all 1,690 inputs. The seven focused assignment-harness selectors each passed exactly once. The attempt did not produce a complete comparison and is not parity acceptance.

The first compare command stopped before creating output because cleanup had removed the derivation receipt's retry-context source worktree. The error is retained in `commands/compare-first.log`. Root restored the exact source worktree and ignored patch named by the receipt. The compare-only retry used the same capture and proof receipts and changed only the output path. At the 15-minute bound it was stopped with SIGTERM. Its runner handled signal 15 and returned `-2`; `comparison-r2/compare.log` records 225 of 1,690 inputs and 900 of 6,760 outcomes with zero mismatches observed so far. The attempt status remains incomplete, so the partial count does not establish parity.

## Source pins

The controller and candidate were commit `56bf3286c0034c183515c0dc4adbf041f398a776`, tree `2d4cf52e64d4dbf06daa5e169c88d6eef8bccdd5`. The reference compiler and shared program source were e240 commit `e240efad273fe35d66d91f4037b6a8193527da2e`, tree `a1ef04ac7b6f759c272e8f841acee3a3b3a6ff08`. The derived-reference producer was commit `74f1551ee9e3066b653ae190779e16d511c90293`, tree `3162d2f33076a8c9261d0ffb7a89a9952662f03f`.

The derivation receipt also binds retry-context base e240 and source commit `1fd9e5f2f366833b786e57d1fc54bcd9a238731e`, tree `1d1661f02f3126c05fb5a7d9487f4fa0c3845556`. Its patch was read from `.work/wt/w9-derived-retry-context-bb/.work/tmp/derived-reference-retry-full.patch`, SHA-256 `5f5de52a036b2410b93ead022d81b14f2ce5076b3263e581b18931cafcba1ad5`; the exact bytes are retained as `retry-context.patch`.

The frozen corpus had 1,690 inputs and manifest SHA-256 `1ec4e8eb67681591d25c69b395387ba4acbb4bfe1e4ab514a858e60ff1a56305`. The schema-4 source receipt and all build and capture receipts used by the run are retained here. The full raw captures remain unchanged under `.work/consolidation/w9-final-56bf`.

## Commands

The exact prepare, capture, and assignment phase command template is retained in `w9-final-closure.sh`. The phases used:

```sh
bash .work/tmp/w9-final-closure.sh 56bf3286c0034c183515c0dc4adbf041f398a776 2d4cf52e64d4dbf06daa5e169c88d6eef8bccdd5 .work/wt/consolidation-final-56bf w9-final-56bf prepare
bash .work/tmp/w9-final-closure.sh 56bf3286c0034c183515c0dc4adbf041f398a776 2d4cf52e64d4dbf06daa5e169c88d6eef8bccdd5 .work/wt/consolidation-final-56bf w9-final-56bf capture
bash .work/tmp/w9-final-closure.sh 56bf3286c0034c183515c0dc4adbf041f398a776 2d4cf52e64d4dbf06daa5e169c88d6eef8bccdd5 .work/wt/consolidation-final-56bf w9-final-56bf assignment
```

The first comparison used the `compare-derived.command` saved with the original output path. The bounded retry argv is retained in `commands/compare-r2.command`; it reuses the same seven capture/proof receipts and directs output to `compare-r2`.

## Retained evidence

`build/` and `captures/` contain the source, build, and completed capture receipts. `build-logs/` and `native-tests/` retain builder and selector results. `status/` records completion ranges and source snapshots. `comparison-r2/` contains both exact-owner proof receipts, the bounded comparison log, and its incomplete status; no complete comparison receipt was produced. No raw sidecars or full comparison products were duplicated into the repository.
