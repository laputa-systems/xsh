# Failed b235 R6 attempt

This archive preserves the first R6 run on source and tool commit `b235a1a43d06e44580fde4c1b8c58ff27a17ca78`, tree `08402ddd079228e072f24642fdb82db5196fff11`. The receipt, six combined step logs, and each step's separate stdout and stderr logs are byte-identical copies from the ignored output directory. Their filenames match the receipt references. No binaries or Cargo targets are archived.

## Result

The approved 13-step manifest passed its pinned image check, verification product build, and three Rust harness builds. Workspace warning-denied Clippy then exited 101. No staging or Linux context step ran.

Clippy reported `clippy::non_octal_unix_permissions` at `crates/xsht/src/cli/files.rs:671` and `:764`. Both calls used `fs::Permissions::from_mode(0)`. The next source commit changed both literals to `0o0`; its fresh R6 result is recorded in the adjacent 7eb62 archive.

The raw receipt keeps its original `failed-or-incomplete` status. It records the original source and tool pins, all six step exit codes, and the failed Clippy output. Its exact manifest is preserved beside it.

## Built artifact identities

| Artifact | SHA-256 |
|---|---|
| `xsh-verification` | `d817ed7e8e43e5b7db5670068eb6fe76186178416233360aa535a5d07b5917af` |
| `xsht-verification` | `4e0322037add1138f0e3a67f6696e3b27e7ae60ea794f198e0c5ea0d6bae8a6e` |
| `xshi-verification` | `5121c56402853320e396695d6854336067fa9754fcc957bdcdd98ffe74fcea53` |
| `xsh-fuzz-verification` | `919ce65d2945e7bce288fbef7191c6e299516154d1c6dcbd7bc8362658b14d19` |
| `xsh-test-helper-verification` | `5687f4a562fe6c35e76d4921032d8293303f896bdfc94cd0ae542fe73f0a653d` |
| `core_compat_boundaries` harness | `36df20d6c4337de379917c4f5323e345bc867c3656cbc52fbfbbce2ac7075f77` |
| `core_compat_extra` harness | `93a1f87dd2d77c803aa6d5f4b6d4f96b3e0d8fa88637c7a56e977e3246c3803b` |
| `core_compat_extra_privileged` harness | `284a214f3f36aa877b50bcf9f0cde174e3d56e9a7559c6bd1793c80af623f1ab` |

Clippy had already exited when the runner sent `docker stop`. Docker reported that the container no longer existed. `docker rm --force` and the exact-container absence check succeeded. The step failed on Clippy diagnostics, not container cleanup.

## Preserved files

- [Unchanged receipt](receipt.json)
- [Approved R6 manifest](manifest-correctness-bb809e1a-20261016-nodev-tmpdir-r6.json)
- [Image check log](logs/001-verify-pinned-image-id.log)
- [Verification product build log](logs/002-build-product-01-xsh-xsht-xshi-xsh-fuzz.log)
- [Boundary harness build log](logs/003-build-harness-core-compat-boundaries.log)
- [Extra harness build log](logs/004-build-harness-core-compat-extra.log)
- [Privileged harness build log](logs/005-build-harness-core-compat-extra-privileged.log)
- [Failed Clippy log and cleanup output](logs/006-warning-deny-01.log)

The corresponding `.stdout` and `.stderr` files for all six recorded steps are preserved in `logs/` beside each combined log.
