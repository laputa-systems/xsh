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
source. Their product and source identities remain in the receipt. The
functional close does not qualify artifacts for the exact successor source.

## Remaining gates

The R3 canonical attempt remains failed with 13,914 passes, two failures, and
78 skips. R4 stopped after the formatter corpus invariant failed and before a
full native suite began. The original receipts remain unchanged.

No fresh full canonical, resource, timing, QEMU, W9 full-corpus, or structural
recount is recorded here. Exact-source artifact qualification, the R5 full
canonical and resource retry, timing gates, and the final metrics evidence
bundle remain pending. Inference changes remain excluded.
