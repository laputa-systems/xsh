# ExprIf direct-IR route gap on 7eb62cd9

The exact route replay completed all 428 invocations. The strict semantic route matrix remains red because one normal `ExprIf` direct-IR route proof is pending.

## Replay result

The replay used clean source commit `7eb62cd993e578c13bf6befd68a6a6ac15133c78` and tree `736b4bc16b1bcf07a0250b97e849506c19fb0753`. Its receipt records 428 successful normal and forced invocations, 856 source checks, and unchanged product hashes before and after execution.

The plan ran 94 artifact IDs, 60 behavior IDs, and 60 source-order IDs in both modes. The replay admitted all 188 supported route artifacts. It also ran 120 behavior steps and 120 source-order steps. The matrix reports 252 source-order links across 119 rows because one exact selector can support several tag rows.

The only remaining matrix gap is `ExprIf` normal `verifier_approved_in_place_recursive_selection`. The XSH `test_expr_if_pure_selection` artifact correctly records `explicit_frame_control_path` in both modes. It does not prove the separate in-place route taken by qualified direct-IR input. A Rust direct-IR receipt must close that gap. The matrix does not grant that route credit here.

## Product pair

The replay used the source-qualified R6 products. The `xsh` hash is `d817ed7e8e43e5b7db5670068eb6fe76186178416233360aa535a5d07b5917af`. The `xsht` hash is `4e0322037add1138f0e3a67f6696e3b27e7ae60ea794f198e0c5ea0d6bae8a6e`.

The route driver requires products under the frozen root's `.work/target-final-canonical` path. The pair was staged there as regular byte-identical copies. The replay receipt records the hashes before and after execution. This staging does not count as a canonical build.

`product-qualification-receipt.json` preserves the R6 product receipt unchanged. Its recorded status is `commands-passed-needs-privilege-skip-and-close-review`. The receipt's exact steps and product hashes remain available for review.

## Archived files

`route-replay/` contains the unchanged replay receipt, logs, probe records, container IDs, and all 188 route artifacts. The receipt retains paths from the original ignored run directory. `matrix-7eb62cd99-20261017.json` preserves the red matrix. `source-lock.json` and `evidence.json` preserve the source and selector inputs. The TSV files preserve the 428-step plan and 120 source-order steps. All archived copies match their source bytes.

The replay receipt SHA-256 is `e331f41877a3a3d9b7e0d43c46a6bb1a9abaede0003e67888fe450c9d44d5530`. The product receipt SHA-256 is `22ebd31a344c5e3977310340d2b45459ff932c0ff729ba3566f009aa949a6fbd`. The matrix SHA-256 is `a8dc7d7ca727ce0030edcdac3f8226afce68a33c6a847d6144cd52047039efc7`.

## AST extractor used for the matrix

The official `generate.py` wrote the matrix with the pinned standalone AST extractor from the frozen source tree. Cargo and rustc were `1.100.0-nightly (7941be6fb 2026-09-11)` and `1.100.0-nightly (574ff7d98 2026-09-14)`. The helper was built offline for `x86_64-unknown-linux-musl` with Kache and mold.

The reusable helper is at:

```text
/home/josh/d/laputa-systems/xsh/.work/wt/consolidation-final-7eb62cd99/.work/tmp/semantic-route-matrix/ast-extractor/7eb62cd993e5-736b4bc16b1b-2b1661fb59781965/x86_64-unknown-linux-musl/debug/semantic-route-ast-extractor
```

Its SHA-256 is `25c1e75dad46d147bf430fd00114ce2edb107272052b5a9396ea0d9ca4462c03`. Its size is 57,661,072 bytes. The helper target was absent before this run. Kache did not report whether it reused cached compilations.

The helper source is pinned by the frozen commit. Its files have these hashes:

- `Cargo.toml`: `b9d90ee1d3656ea0ab0e7d6e2d207ad626767030485a0d22648a3f60b23f395a`
- `Cargo.lock`: `043ea06cc21bbde96705397224c71f8b8e061ff9be6c8eb9ba009da7c355f182`
- `src/lib.rs`: `8695b4efa159ec5a19d0628d669f9907d4b5d586fcf7aeeb709502b5278fc736`
- `src/main.rs`: `b6c87b909eae719e0815fa2228e60af8986a6f7acd0519443ce6840c8f9ceb89`
