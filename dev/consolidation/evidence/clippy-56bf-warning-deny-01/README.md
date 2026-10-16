# XSHT workspace Clippy failure

`correctness-56bf-20261016-r1` is the original receipt ID. It was the fifth overall correctness-gate attempt, after four attempts for `be0b`.

The receipt and three `006-warning-deny-01` logs are byte-for-byte copies of that attempt. The step ran workspace Clippy for `x86_64-unknown-linux-musl`, all targets, and all features with `-D warnings`. It exited 101 on source commit `56bf3286c0034c183515c0dc4adbf041f398a776`, tree `2d4cf52e64d4dbf06daa5e169c88d6eef8bccdd5`.
