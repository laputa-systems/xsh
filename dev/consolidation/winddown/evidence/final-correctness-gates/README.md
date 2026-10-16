# Final correctness gate failures

These files preserve the recorded failure receipts and raw failure logs. Each copied receipt remains unchanged and retains its original scratch paths.

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
