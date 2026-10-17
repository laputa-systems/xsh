# Functional close for successor b1bc741f

This archive records the additive functional close for source commit
`b1bc741fcd512d1c71b43116853278a4a3f1521a`, tree
`6d2a7d2fc8506ca5de8e312ded27b5548385116e`. The [root-issued receipt](functional-close.json)
marks this narrow delta complete. It inherits the accepted functional scope
from `19c9aded0ea42c979fbc6be32162320d29ded3b1`, including the explicitly
inherited `7eb62cd993e578c13bf6befd68a6a6ac15133c78` scope. It does not record a
new full canonical pass.

## Successor delta

The source delta moves fifteen capability fixtures byte-for-byte into the
existing excluded evidence directory. Two README files update their links and
fixture classification. The original typing-cohort inventory is restored
unchanged. The delta changes no Rust, Cargo, core, native-test, configuration,
or tooling code.

The exact formatter/lint repository invariant failed on the parent at
`bytes-reduced.xsh:1:17`. On corpus candidate
`b2f3a0624205e68d06f9a439ffe2dea5e7dc4541`, the same selector passed with one
selected test, one pass, and 121 filtered tests, using the same harness and
products. The close records the audit of 1,835 observable XSH and configuration
inputs. This result covers the fixture-discovery change only.

The [inventory check script](inventory-check.py) and its
[captured output](inventory-check.stdout.log) are byte-identical copies of the
original check. The script loaded the real `cohort.inventory`, checked the
restored inventory blob, and compared the original input digest before and
after. Its output records 9,194 sites across 48 sources and the frozen category
counts. It does not change the inventory or its denominator.

The close reuses the accepted factor and lifecycle checks from the parent
source. Their product and source identities remain in the receipt.

## Exact-source artifacts

The [artifact receipt](profile-artifacts.json) records a successful
artifacts-only release build from the clean source commit and tree above. The
build exited zero and its owned container cleanup resolved. The target is
`x86_64-unknown-linux-musl`, with opt level 3, thin LTO, and 16 codegen units.
The receipt records these artifact hashes:

- `xsh`: `e79fd4820b23b840c0bbface75a8b24780eae8fb7aedc34a01d76045f9aeae6b`
- `xsht`: `54bb342ae14e77cdacac6db01974f60606ea42ad1a27a6ee3185cbcfe1b01b02`
- `xsh-test-helper`: `cf8005649749a43cc22aa5e40092a8f90365efc370b857fce5de2004adb2e621`

The build used 12 CPUs on CPUs 0 through 11, a 24 GiB memory limit, and no
swap. The receipt does not report peak memory. Its timing fields are null, and
artifacts-only mode grants no timing or resource-gate result.

The proof-reproduction supplement commit
`00754f5e11ef411d1dd04bac63b2036134340162` remains separate and unmerged. This
archive does not duplicate its files.

## Remaining gates

The R3 canonical attempt remains failed with 13,914 passes, two failures, and
78 skips. R4 stopped after the formatter corpus invariant failed and before a
full native suite began. The original receipts remain unchanged.

No fresh full canonical, resource, timing, QEMU, W9 full-corpus, or structural
recount is recorded here. The R5 full canonical and resource retry, timing
gates, and final metrics evidence bundle remain pending. Inference changes
remain excluded.
