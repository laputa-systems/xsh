# Canonical run on 3c0c01206

The pinned Linux canonical batch ran against clean source `3c0c01206db71443662242167ac5f0a4bfafc62b`, tree `ddfc7e60f70f0de26b618b3c0eabefd8a2b36122`. The xsh verification bootstrap passed with the locked musl build. XSHT integration took 1,854.26 seconds and then failed with exit 3. The receipt grants no acceptance credit.

The failing case was `desugar::the_desugared_corpus_checks_and_tests_like_the_corpus`. It compared 198 written and desugared files. Written `core/tests/test-uu-tail-1.xsh` passed 79 tests. The desugared run passed 78 and failed `test_uu_tail_follow_name_hash_table_stress`: actual stderr had an additional `tail : stderr tail: 9: file truncated` line before the expected inaccessible and appeared messages. The output difference is recorded in `stdout.log`; this run does not establish its cause.

The canonical runner did not reach the full native suite, the four batch checks, or the documentation gate. It therefore produced neither a canonical pass nor a functional close. The source remained clean at the pinned commit and tree, and container cleanup resolved.

Products at the end of the run were stable after the runner exited:

| Product | Cargo-relative path | SHA-256 | Size |
|---|---|---|---:|
| xsh | `.work/target-final-canonical/x86_64-unknown-linux-musl/verification/xsh` | `2576641bd019ea1ba46fa555e351668407dbbacabdf4d87cabcf63de32b5b017` | 31,645,840 B |
| xsht | `.work/target-final-canonical/x86_64-unknown-linux-musl/verification/xsht` | `5ba0907b1b5222f5dab0efa19496b711bb4036b14c0ab2c344d1d8987f7e8836` | 38,738,400 B |
| xshi | `.work/target-final-canonical/x86_64-unknown-linux-musl/verification/xshi` | `5121c56402853320e396695d6854336067fa9754fcc957bdcdd98ffe74fcea53` | 587,320 B |
| ordinary `core_compat_extra` | `.work/target-final-canonical/x86_64-unknown-linux-musl/verification/build/xsh/409440b2a43657bc/out/core_compat_extra-409440b2a43657bc` | `66b35d1aeece51b28329abaa7cd8e652cbedfa68252a6af296d49842a1de739a` | 2,062,576 B |

The privileged harness was not built because the earlier XSHT failure stopped the Cargo plan before that step. No binaries are retained here.

`canonical-replay.json` records the exact container command, source checks, products, resource capture, and failure status. `bootstrap-receipt.json` and `cargo-build.jsonl` bind the successful bootstrap to this source. The accompanying stdout, stderr, and resource files preserve the run output unchanged.
