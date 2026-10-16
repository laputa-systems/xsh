# Canonical attempt at ed908

The pinned Linux musl replay binds clean source
`ed90857bdd825439159b7d788ea5d57cd9652920`, tree
`efc56a4a81c7993e35b2fd76a2549742eb8d8847`, before and after.
It exits 3 with no timeout or OOM kill. No timing or full-batch acceptance
credit is claimed.

The replay passes 305 process tests and 741 library tests. XSHT integration
passes 115 tests and fails the full desugar comparison. The written procps file
passes 39 tests; the desugared file passes 38 and fails
`test_iostat_selects_blocks_and_units_with_options`. Its `mebibytes.stdout` is
empty where the test expects the device table header. The logs retain the
assertion and both reports. This observation does not establish the cause.

Soundness, later privileged checks, and the normal full native suite are not
reached. The repaired soundness oracle therefore has no fresh canonical result
from this attempt. Product hashes in the receipt identify actual built products,
not a passing canonical batch.
