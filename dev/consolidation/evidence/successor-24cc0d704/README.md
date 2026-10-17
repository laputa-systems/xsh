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

This close is an additive functional qualification, not a full canonical pass.
The close record alone grants no resource, timing, QEMU, W9, or structural
recount credit. Inference remains isolated. A fresh exact-source artifacts-only
build is recorded below; the separate R6 canonical/resource run is recorded
after it. No timing gate has run.

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

## Failed timed-profile attempts

Two earlier `opt-level=3` attempts are preserved with their raw receipts and
logs. R1's [receipt](profile-attempts-r1/receipt.json), [build log](profile-attempts-r1/build.log),
and [wrapper stderr](profile-attempts-r1/wrapper-stderr.log) are archived under
`profile-attempts-r1/`; its wrapper stdout is empty. R2's
[receipt](profile-attempts-r2/receipt.json), [build log](profile-attempts-r2/build.log),
[native-suite log](profile-attempts-r2/native-suite.log), and
[wrapper stderr](profile-attempts-r2/wrapper-stderr.log) are archived under
`profile-attempts-r2/`; its wrapper stdout is empty. The receipts retain
container IDs and cleanup results; no standalone CID files remained. Both
used source `24cc0d704209a8c0aa0eb27d5cfea1434929dc20` and tree
`b6efac064ed074c9ee03d8f183bff9b41d353c91`. Their original commands and
container records remain unchanged. The recorded profile tool is the original
revision `39948e7c9f6ca85fb06c33ee4fab8a777e53b1f4b40733ac66776a8473739556`.

R1's build exited zero, but the post-run readiness check reported host load
12.91 on 12 CPUs. An outer `taskset` made host CPU discovery report 12 CPUs;
the Docker build already had a 12-CPU limit. The tool discarded the timing and
stored a null duration. R1 did not run the native suite.

R2 removed the outer `taskset`; host discovery reported 32 CPUs, while Docker
remained limited to CPUs 0–11, 12 CPUs, and 24 GiB of memory with no swap.
Both readiness checks passed. The build succeeded and produced the same three
binary hashes as the accepted artifact build. Its native suite failed with
13,853 passed, 2 failed, and 139 skipped: GNU `rm`'s ext3 case returned status
3, and the `uu` sort buffer-size case timed out after four minutes and received
SIGTERM. The attempt remains failed; its stage measurements do not establish a
profile result or speed comparison.

The R2 wrapper's temporary-directory cleanup initially hit a permission error.
The [additive cleanup resolution](profile-attempts-r2/cleanup-resolution.txt)
records permission-aware cleanup and confirms the temporary path and owned
container are absent. It also confirms the original receipt and logs are
unchanged; the cleanup does not change the run's failed result. All containers
started by the attempts are absent. The Cargo, native-suite, and
final-performance locks remain free at their original inodes.

## R6 canonical and resource run

The [R6 receipt](resources-r6/receipt.json) has SHA-256
`b7a31410ba6ee2eb828f0b23e6b3966a90d2ef4463dd32b4d364f99c20ce887d`. Both
stages completed with exit status zero against source
`24cc0d704209a8c0aa0eb27d5cfea1434929dc20`, tree
`b6efac064ed074c9ee03d8f183bff9b41d353c91`; the source was clean before and
after. The canonical `cargo dev gate --batch` stage reported 13,916 passed,
zero failed, and 78 skipped native tests. Metrics, documentation, and diff
checks also passed. Its [stdout](resources-r6/canonical-gate-batch/stdout.log)
and [stderr](resources-r6/canonical-gate-batch/stderr.log) are preserved, as
is the [container resource capture](resources-r6/canonical-gate-batch/container-resources.json).
Container cleanup resolved.

The separate repository lint stage exited zero with two findings and 113
notes, so it is not lint-clean. Its [stderr](resources-r6/isolated-repository-lint/stderr.log)
and [resource capture](resources-r6/isolated-repository-lint/container-resources.json)
are preserved.

The canonical container was limited to CPUs 0–11, 24 GiB of memory, and zero
swap. Its cgroup memory peak was 5,179,297,792 bytes and sampled process RSS
peak was 4,966,969,344 bytes; OOM and swap events were zero. The observer's
timing fields are null and discarded. This run supports no timing claim.

The exact [observer artifact manifest](observer-tools/final-artifact-manifest.json)
and its six referenced files are preserved beside the R6 run. The manifest
SHA-256 is `de31460e1232ddfcb118f314e59d27109ca204758ba74e9952129576fa1d27f5`.
Each referenced file matches the hash recorded in that manifest.

R5's stale marker cleanup is preserved in the
[additive cleanup reconciliation](r5-cleanup-reconciliation/reconciliation.json).
It records that the owned container and process were absent, cleanup resolved,
and the native cleanup marker was removed; the original R5 receipt remained
unchanged. This reconciliation does not change R5's failed status.

## Scheduler reference qualification

The [static qualification](scheduler-static.log) passes for the exact 24cc source.
The reference overlay modifies only the two approved scheduler files and matches
the pinned patch in both recorded diff forms. Its derived Git tree is
`e0df45e193a75774a1f95fb07ee4b41599ff2129`.
No reference build, native suite, or timing run follows from this static check.

## Non-timing review scope

The `src` and `tests` Git trees match 7eb exactly. The prior source reviews for
rows 1–5 and 8 therefore carry to 24cc. Their execution receipts remain pinned
to their original sources. Row 5's prior paired lint executions remain historical
because the lint implementation changed.

The shared lint walker, flow, and sequence files are unchanged. The three
changed lint files alter diagnostic filtering and tests without adding a descent.
The module driver and module record files are unchanged. These comparisons
preserve rows 6 and 7's implementation scope. The prior cache trace remains
historical. Row 9's TODO, open-defects, and W9 review inputs also match exactly.

The scanner policy is unchanged, and changed XSHT hunks do not affect its
detectors. The four hard-zero structural counts carry by source comparison.
The b235 metrics report remains the measurement. No fresh 24cc scan, current
source-line count, or complete ten-row evidence bundle is claimed. Source-line
budgets remain advisory. Timing evidence remains required.
