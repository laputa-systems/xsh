# Successor evidence for 19c9aded

These receipts bind the additive functional close and exact-source artifact
qualification to commit `19c9aded0ea42c979fbc6be32162320d29ded3b1`, tree
`956ea0056c53b5f006ede4fd980fd7ae364970e2`. The original receipts remain
unchanged at their `.work` paths.

## Functional close

[functional-close.json](functional-close.json) is root's additive delta close.
It reuses the accepted semantic, route, platform, and ownership scope from
`7eb62cd993e578c13bf6befd68a6a6ac15133c78`. It is not a fresh full canonical
pass.

The successor delta changes factor output accumulation and the lifecycle
fixture's child profile. Three exact checks passed after integration on
`3160bda38f5fb4cdb05e6b279efc57b296d4fcf8`: parallel factor, non-Unicode
factor, and lifecycle cleanup. The receipt preserves their source and product
identities.

The earlier R3 canonical attempt remains failed with 13,914 passes, 2 failures,
and 78 skips. The receipt preserves that result and does not claim contention
caused the timeout. It also claims no fresh QEMU run, W9 full-corpus run, or
structural recount. Inference changes remain excluded.

## Artifact qualification

[profile-artifacts.json](profile-artifacts.json) records a clean-source,
`artifacts-only` release build for the exact successor commit and tree. It
qualifies the recorded `xsh`, `xsht`, and `xsh-test-helper` artifacts for
`x86_64-unknown-linux-musl`. The receipt contains their paths and hashes, plus
the build profile, inputs, and tool identities. No binaries or build cache are
archived here.

The R3 canonical products are separate. The retained Cargo build outputs and
protected copies have identical bytes:

- `xsh`: 30,216,624 bytes, SHA-256
  `1d7690114501313ffc4706b7f78ab6154ecd668f79d3735700219a00501dbe33`
- `xsht`: 40,850,808 bytes, SHA-256
  `0cd37140bd43ca0ca5104d43d11bc1732084fc2342ee3bb69a73fda38452f1c4`

The retained Cargo outputs are under
`.work/final-review-evidence/final-performance-gates/resources-7eb62cd9-20261017-r1/gate-batch-target/x86_64-unknown-linux-musl/release/build/`,
in `xsh/27d36d63319bd1a5/out/xsh` and
`xsht/b85012ac7e8d215c/out/xsht`. Protected copies are at
`.work/protected-products/resources-r3-7eb/{xsh,xsht}`. I compared both
pairs byte-for-byte. The public Cargo `release/xsh` and `release/xsht`
aliases are mutable and now point to later R4 variants 6038 and ae36. They do
not identify the R3 products.

The artifact-only receipt records different `xsh` and `xsht` hashes:
`e79fd4820b23b840c0bbface75a8b24780eae8fb7aedc34a01d76045f9aeae6b` and
`54bb342ae14e77cdacac6db01974f60606ea42ad1a27a6ee3185cbcfe1b01b02`. Those
are exact-source profile-build artifacts for 19c9, not products from the R3
canonical attempt.

This build did not run a canonical suite or a timing measurement. The receipt
marks timing as discarded. The R4 canonical and resource retry is still
running and has no accepted result. Timing gates and the final metrics review
remain pending. The failed R3 receipt remains unchanged.
