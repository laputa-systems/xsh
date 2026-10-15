# Assignment and route checks at 52c0

Source: `52c0f5399ac9e488063cf6649e6e3d6a4cfbe54f`.
Tree: `1afd793931e2f07d44834af3e4e761027e9b75a5`.
Laputa: `ca658ce1da9d89c55209c25268dfdf687ff4fdc6`.
Both source checkouts were clean during these checks.

All runs used pinned Linux image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`,
eight CPUs, 16 GiB memory, and no swap. Products target x86_64 Linux musl.
Compilation used Kache, mold, static CRT, and the `sscanf` and `strtol`
isoc23 linker aliases. Jemalloc preload applied only during compilation.

## Focused results

| Check | Result | Output |
|---|---|---|
| Merged-source XSHT unit tests | 7 assignment, 1 SourceOnly, 2 CLI tests pass | `unit-*.log` |
| Actual abort child, expected status 23 | Normal and forced pass with one authenticated record each | `abort-*` |
| Native lint regressions | Four exact tests pass | `lint2-*` |
| Repository check and lint | Baseline and candidate exit zero with identical diagnostics | `*-repository-*` |
| Laputa check and lint | Baseline and candidate exit zero with identical diagnostics | `*-laputa-*` |

The unit logs retain exact commands, source identity, filter lists, and results.
Their scratch path was the repository's ignored `.work/tmp`.
The native runs held the shared native-suite lock and used `/scratch`, bound
to the same physical scratch directory.

The native lint cases cover value-pipeline fixes, comparison-chain convergence,
comparison-chain safety, and require-fix boundaries. Abort artifacts contain
`ExprAbort` ordinal 102. Normal uses `explicit_frame_apply_path`.
Forced uses `generic_operand_apply_path`. Both artifacts identify the child
source hash `fb909919356c2ff4fdac9f21727dada66117712029320420e5fbcb6f52f76034`.

The paired commands are `xsht check .` and `xsht lint .` from `/work` and
`/laputa`, mounted read-only. Each product ran against the same current source
cohort. All stdout files are empty. Stderr diagnostics match byte for byte
after removing the validated closing timing line, using the integration
harness's `stderr_before_timing_line` policy. Complete stderr files retain
those timing lines. This comparison establishes diagnostic parity.

## Product identities

| Product | SHA-256 |
|---|---|
| Baseline 2688 xsh | `aff1428185e1072a4a68747d30834ad6c81d9586c19a952c52757eb5682f9ada` |
| Baseline 2688 xsht | `a355ab671ce00844eff4a942c34dd092b0cf77a8e17a0425130828f2d11f7576` |
| Candidate 52c0 xsh | `c5031d38d0c1f6c041eb7547369425d2d103c802f6ad7f275bc2446bb383e496` |
| Candidate 52c0 xsht | `184fcc644f49f82f2750f7e8d5400f9a3e86116a4fcfe152e91238be38d15c90` |

This focused batch leaves canonical suites, source-pinned semantic reviews,
route-matrix completion, and performance acceptance open. The historical AWK
discrepancy remains unexplained.
