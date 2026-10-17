# Final correctness gate evidence

This index preserves failed attempts and source-qualified supplemental records. Receipts and raw logs remain unchanged and retain their original scratch paths.

- `correctness-be0b-20261016` failed preflight before a command ran. Its receipt has no steps and the original `logs` directory was empty.
- `correctness-be0b-20261016-r2` failed the pinned-image check. Its image-inspect log is preserved.
- `correctness-be0b-20261016-r3` failed Cargo argument parsing while selecting products. Its image-inspect and product-build logs are preserved.
- `correctness-be0b-20261016-r4` failed warning-deny Clippy at `crates/xsh-applets/src/mdev.rs:1689` on a needless borrow. The receipt and that step's stdout, stderr, and combined log are preserved.

The r4 receipt records production source `be0b79a48403d74b11186166ba739a81dcbed702`, image `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`, and manifest body `939f078f7f572c81b0cb188617991c32f0ce23d7acb1e8685170af8ec271e5ad`. The receipt also records the runner and tool hashes. This directory keeps only r4's failing step logs, not its successful build logs or target files. The complete ignored attempt remains under `.work/final-review-evidence/final-correctness-gates/` while that scratch storage exists.

## Focused mdev Clippy check

`mdev-applets-clippy-f8f8190b8` preserves the package-scoped check after removing the needless borrow. The source was `f8f8190b894ef6b2d55382dba02b83904baa2f38`, tree `755164b3190b36ba5e04745455cda20112e0ede4`. That commit changes the mdev test on top of tool/source base `6299a9725da1ccc208792ffa7f6e99e69dd17f62`.

The check used the tracked `run-native-final.sh` at SHA-256 `14700ccc0e657d03c00e9efef052bdfbf67228a98d9cd38c2decfa066ca58858`, the pinned image above, and r4's existing `target/products` directory. It exited zero:

```sh
cargo clippy --locked --offline --target x86_64-unknown-linux-musl -p xsh-applets --lib --tests --all-features --quiet -- -D warnings
```

The runner's stdout was empty. Its stderr contains the Docker invocation banner. Quiet Clippy emitted no diagnostic. This focused result does not change or replace the failed r4 receipt.

## Frozen e048 supplemental attempts

The supplemental attempts all selected source commit `e048b33f2c716ecff4e8d38428693e49466eef35`, tree `bc4ddf25718010ed086778372e820dda4992c0a6`, on the pinned `x86_64-unknown-linux-musl` image `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`. Each attempt passed image verification, the verification product build, all three harness builds, and warning-deny Clippy before its Linux context failure. The copied receipts retain the full runner and source/tool identity records; each adjacent log is the failing context's raw output.

- `correctness-e048-20261016-source-root` used tool commit `c066957ea159117da0debadb94da4a590f297798`, tree `95e8ea2cf8f88787f9d67848888e8643722b4935`. Its first context ran 23 boundary tests: 14 passed and 9 failed with `EROFS` while creating `$CARGO_MANIFEST_DIR/target/core-compat-boundaries`; the frozen source bind had accidentally been read-only. See its [receipt](correctness-e048-20261016-source-root/receipt.json) and [failure log](correctness-e048-20261016-source-root/logs/007-linux-special-01-core-compat-boundaries.log).
- `correctness-e048-20261016-source-root-r2` used tool commit `d8d6c57dff852d9d7ed7f6ca4016b629e2e8929f`, tree `d4541afc6b5d294d1c83b1410afa8ce93eb3ac73`. The boundary context passed. The next context reported 57 passed, 1 failed, 3 ignored, and 2 filtered; `pty::native_terminal_redirection_does_not_hang_up_its_session_leader` could not spawn its sibling `xsht` and got `ENOENT`. See its [receipt](correctness-e048-20261016-source-root-r2/receipt.json) and [failure log](correctness-e048-20261016-source-root-r2/logs/008-linux-special-02-core-compat-extra.log).
- `correctness-e048-20261016-source-root-r3` used tool commit `6f31d46b4806edd9f6e3c2724e711b055c55fce6`, tree `3cf682ef93a72ddd85c0ed5e929447982aa5b3d7`. Its boundary context passed; its next context had the same `57 passed, 1 failed, 3 ignored, 2 filtered` result and the same missing sibling `xsht`. The copied [receipt](correctness-e048-20261016-source-root-r3/receipt.json) is byte-identical to the scratch receipt: SHA-256 `7a80ad1260518f347c395e6c0b42447494e428254af00dee152c989f67423e82`. The corresponding [failure log](correctness-e048-20261016-source-root-r3/logs/008-linux-special-02-core-compat-extra.log) is preserved.

## Focused PTY retry

`correctness-e048-20261016-core-compat-extra-pty-focus-r2` records the single focused retry against the same frozen source, using tool commit `0f1f17f23f465e4f04cebf85fe21562dc4f2f301`, tree `b02fde7012692ff23a1d370dcaf954fdadfe2b46`. Its source-qualified verification product `target/products/x86_64-unknown-linux-musl/verification/xsht` was staged beside the existing `core-compat-extra` harness executable at `target/harness/x86_64-unknown-linux-musl/verification/xsht`. Source and destination SHA-256 both equal `2d44630fdebed670d52354f33f0a39a71aba17ae9ae34ab7013892ce861f5345`. The sibling harness `xsh` is the source-qualified verification harness artifact with features `default,native-tests,net,tools` and SHA-256 `c22e68d4e33c23669361186e7f77ef669b2f1b6207b53ebdd551a792f73bca6b`. The [receipt](correctness-e048-20261016-core-compat-extra-pty-focus-r2/receipt.json), [staging log](correctness-e048-20261016-core-compat-extra-pty-focus-r2/logs/001-stage-core-compat-extra-xsht.log), and [focused test log](correctness-e048-20261016-core-compat-extra-pty-focus-r2/logs/002-linux-special-focus-core-compat-extra-pty.log) record both successful steps. The exact PTY test passed: 1 passed, 0 failed, 0 ignored, 62 filtered out. This used the already-qualified r3 artifacts and did not rebuild them.

The five-context acceptance matrix remains open: `core-compat-boundaries` passed in r2 and r3, the non-split `core-compat-extra` context failed as above, and the split, privileged cp, and uid-1000 traceroute contexts were not reached. The QEMU-only ignored date-clock and BTRFS cases also remain open. The focused PTY pass does not close the supplemental or canonical gate.

## Frozen 0dc9 supplemental

The [reference-mode record](correctness-0dc9cff7-20261016-r1/README.md) covers the approved replacement protocol on source and tool commit `0dc9cff7c2b16e4c201adb3abca11d7ebda8e1f4`, tree `b4f9db3f8ee2f38a6c62fea876976619db424ed7`. The successful product and harness builds, warning-denied Clippy, staging step, and all five Linux contexts are documented there. Earlier failed receipts remain unchanged. Root's separate skip and QEMU review accepts this supplemental protocol result; canonical, W9, routes, and resource closure remain open.

## Fresh b235 and 7eb62 R6 runs

The [b235 record](correctness-b235a1a43-20261017-r1/README.md) preserves a failed attempt on source and tool commit `b235a1a43d06e44580fde4c1b8c58ff27a17ca78`. The image check, product build, and three harness builds passed. Workspace Clippy then rejected two non-octal permission literals. The receipt remains failed, and no later steps are credited.

The [7eb62 record](correctness-7eb62cd99-20261017-r1/README.md) preserves the fresh attempt on source and tool commit `7eb62cd993e578c13bf6befd68a6a6ac15133c78`, tree `736b4bc16b1bcf07a0250b97e849506c19fb0753`. All thirteen R6 steps exited zero. Root separately reviewed the three ignored date tests against the QEMU evidence. The two UID-1000 traceroute capability skips still require positive Linux residual coverage. The R6 and canonical product variants have distinct binary hashes.
