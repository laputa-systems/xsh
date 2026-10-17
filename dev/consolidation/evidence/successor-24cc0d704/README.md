# Functional close for 24cc0d7

Root accepted a narrow additive functional close for commit
`24cc0d704209a8c0aa0eb27d5cfea1434929dc20`, tree
`b6efac064ed074c9ee03d8f183bff9b41d353c91`. The immutable record is
[functional-close.json](functional-close.json), SHA-256
`fa53df523e59ef877211d550fb88061aa9e7553c759f8947be14dd0613eb8492`.

The close inherits the narrow `b1bc` close and its explicitly inherited `19c9`
and `7eb` scopes. Three private XSHT lint files preserve checked proofs across
`CheckRedundantParens`; semantic diagnostic vetoes remain. The focused
regressions and controls pass, and the full exact layout invariant passes on the
qualified `e9ef` inputs. The record binds the exact inputs and products used.

This close is not a fresh full canonical pass. It grants no resource, timing,
QEMU, W9, or structural recount credit. Inference remains isolated. A fresh
exact-source artifacts-only build is recorded below. It does not replace the
fresh canonical, resource, or timing gates.

## Artifacts-only release build

The [profile receipt](profile-artifacts.json) has SHA-256
`b755cdefcd6fa60cb72dcf0570ef5be52c74addc42a42e7d5c5571ad7d2dfff8`. It is
bound to source `24cc0d704209a8c0aa0eb27d5cfea1434929dc20` and tree
`b6efac064ed074c9ee03d8f183bff9b41d353c91`.

The release build used `opt-level=3`, thin LTO, and 16 codegen units. Docker
limited it to 12 CPUs on 0–11 and 24 GiB of memory. The cgroup swap limit was
zero.

The build produced these release products:

- `xsh`: 30,158,976 bytes.
- `xsht`: 40,737,768 bytes.
- `xsh-test-helper`: 657,104 bytes.

The receipt records each product hash. The build exited zero and container
cleanup resolved. No full native suite ran. The receipt records the memory cap,
not a measured peak. Timing fields are null and discarded, so this run supports
no timing claim.

## Scheduler reference qualification

The [static qualification](scheduler-static.log) passes for the exact 24cc source.
The reference overlay modifies only the two approved scheduler files and matches
the pinned patch in both recorded diff forms. Its derived Git tree is
`e0df45e193a75774a1f95fb07ee4b41599ff2129`.
No reference build, native suite, or timing run follows from this static check.
