# Semantic and structural closure evidence: b235 scan, 7eb62 reuse

## Pin and method

This read-only source qualification is for commit `b235a1a43d06e44580fde4c1b8c58ff27a17ca78`, tree `08402ddd079228e072f24642fdb82db5196fff11`. It adds no production source or tests. I ran no build, test, native suite, or timing command.

## Current-source reuse at 7eb62cd

The current product commit is `7eb62cd993e578c13bf6befd68a6a6ac15133c78`, tree `736b4bc16b1bcf07a0250b97e849506c19fb0753`. The only source/test diff from b235 is in `crates/xsht/src/cli/files.rs`: two `#[cfg(test)]` permission-mode literals change from `from_mode(0)` to `from_mode(0o0)`. This does not change production code, test selection, line count, or any runtime contract. Root reports the focused XSHT all-targets/features `-D warnings` checks green at 7eb; the accepted full canonical is the b235 run, and it was not repeated. Thus the b235 semantic source reviews carry to 7eb; the changed XSHT fixture has its separate focused receipt.

I reran official `check-current` in input mode against the saved b235 report and the current 7eb checkout. It exited 0. The generated report and audit were byte-identical to the saved b235 files (report SHA-256 `d14b1644bb2538b3a50ef82d894131a2fc669ba8f604904a1c8683d7280b3a07`; audit SHA-256 `76879db307cc633ea443b1cb36fdb1f6bf5078569e087f922e6ea9e265514aea`) and were removed from scratch. This validates policy on the saved report; it is not a fresh 7eb source scan. The zero metrics therefore retain their measured b235 source pin and carry to 7eb by the production-source identity above. The reported source totals likewise remain b235 measurements and stay advisory; no current-source total is claimed.

The final-close schema fixes eleven source paths. These are the exact `ReviewSource.oid` values at 7eb and at the rebased evidence-branch HEAD; only the aggregate `crates` tree OID changes from b235:

| Review source path | OID at 7eb62cd |
|---|---|
| `src` | `b7d70f65ffe209347742bb27f631f801613fb273` |
| `crates` | `65b8037a95430d265aaae324e147c8cbe035d047` |
| `tests` | `f833d0d156785b1d6fda5a06efb8ce797a2fe2ce` |
| `core` | `9ac7fd05cbfe24d2679fb860a6d8b9a3d3109565` |
| `examples` | `046719960b39fb48b30e9be24de21efc24be53f9` |
| `showcase` | `79cae4996a771fba9b7f599a3537d19c636d296c` |
| `tools` | `a63177a1755dc42f35ee649cb7b28fcd5df8b0aa` |
| `dev/tests` | `a37e594ac099e353305fa67db778a0855d1ebe54` |
| `dev/consolidation_metrics.xsh` | `3ff0ede0c9e2e09645aa3b118e8bb84d4339b741` |
| `dev/consolidation_metrics_current.xsh` | `89aca84fd486800b168b4b53d52df6d7608755f9` |
| `dev/consolidation/metrics.xsh` | `bdd5e7712634f23e3fe640c7b33b5e6f21f944b3` |

The fixed review-source paths are clean and have those same OIDs at HEAD. The evidence branch is based on 7eb, so `source_commit` can be `7eb62cd993e578c13bf6befd68a6a6ac15133c78` in the eventual `ReviewEvidenceBundle`.

The fresh 7eb R6 supplemental run is recorded at `.work/final-review-evidence/final-correctness-gates/correctness-7eb62cd99-20261017-r1/receipt.json` (SHA-256 `22ebd31a344c5e3977310340d2b45459ff932c0ff729ba3566f009aa949a6fbd`). Its 13 command steps all exited 0 without timing out; the receipt binds both source and tool to the 7eb commit/tree and records clean source checks. Root reviewed the source, product guards, and all contexts. The receipt status remains `commands-passed-needs-privilege-skip-and-close-review`; this is supplemental R6 evidence, not the accepted b235 canonical-suite receipt.

The command was `metrics.xsh measure --root <checkout> --source-root src --xsht-source-root crates/xsht/src --output <report> --item-inventory <inventory>`. It read both source roots from the pinned checkout, writing [the report](metrics-report.json) (SHA-256 `d14b1644bb2538b3a50ef82d894131a2fc669ba8f604904a1c8683d7280b3a07`) and [item inventory](item-allowances.json) (SHA-256 `744ecb34c47b33f87ba2bd91836873df2f40d75d11eb0a9a39ef2eb2ff5ad3f1`). The runner was `/home/josh/d/laputa-systems/xsh/.work/consolidation/resume-candidate-5366/xsh`, SHA-256 `83ffe690b3a7feb215ef7cb5fb5800d8818c1fb71ef67bd8ef712753b5c733a7`; it ran in pinned Linux AMD64 image `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`, with one CPU, 4 GiB memory, no network, and `.work/tmp` for scratch. The official observer inputs (`metrics.xsh`, `dev/consolidation_metrics_current.xsh`, and `dev/consolidation/current_source_policy.xsh`) are byte-identical between product pin `5366d950ad644fec8652b2a409148c75261a1358` and b235.

The measured report contains 372 source files. It reports these structural values: `manual_payload_calls=0`, `optional_raw_calls=0`, `wide_arena_wildcards=0`, `blanket_dead_code_lines=0`, and `lowering_type_resolver_definitions=0`. Other measured values include 152 instruction variants, 17 pattern variants, 46 stage variants, five nested-machine factory calls, `source_lines_src=215122`, and `source_lines_xsht=53234`.

The command `metrics.xsh check-current --root <checkout> --source-root src --xsht-source-root crates/xsht/src --input metrics-report.json --output <scratch-current-report> --baseline dev/consolidation/baseline-blanket-scope.json --baseline-provenance dev/consolidation/blanket-metric-provenance-source-advisory.json --envelope dev/consolidation/envelope-upstream.json --audit check-current-audit.json` exited 0. The scratch current report was removed after the check. Its [audit](check-current-audit.json) (SHA-256 `76879db307cc633ea443b1cb36fdb1f6bf5078569e087f922e6ea9e265514aea`) records only the approved source-total rises. The current metric policy explicitly filters the two source-line totals from its structural ratchet and retains the four zero requirements for final close. No source-total waiver or tool change was made.

## Ten semantic rows

This qualification carries the named historical evidence only across the source paths shown unchanged below; current source review is identified separately from executed results. The prior [45b4 review](../acceptance-review-45b4/semantic.md), [e048 review](../semantic-resume-e048/README.md), [dispatch review](../fallback-review-e048/dispatch-review.md), [4d7 source review](../clippy-4d7b/source-review.md), [frame-machine review](../clippy-4d7b/factory-review.md), and [6198 source inventory](../source-review-6198/source-review.md) remain pinned historical artifacts.

| Row | b235 qualification and exact remaining gate |
|---|---|
| `operand_behavior_differences` | Current source-order implementation is covered by the accepted `6aa12613251127a8c3765ae5e43cee84b08f23cd` source review; its process evaluator/producer files are unchanged through b235. Current parser, lowering, and sema changes implement the ordered option, environment, mandatory-target, and interleaved argument/redirection operands. The final 7eb route matrix is accepted: 428 normal/forced runtime invocations and 856 source checks passed; 188 XSH proofs and 120 source-order outcomes are retained. See the [strict route matrix](../../winddown/evidence/route-replay-7eb62cd99/direct-ir-admission/README.md). Historical 184-route/64-source-order results are not substituted. |
| `duplicated_per_item_stages` | Stage execution/lowering, `producer.rs`, the two pipeline implementations, and `stage-list-stream-parity.xsh`/`stage-engine-oracles.xsh` are unchanged from 4d7. The two list/live-stream differential selectors have historical 4d7 passes. The current canonical run owns current native/Rust executions; no additional lane replay is needed. |
| `nested_machine_bounds` | Current source review confirms five production `ExplicitFrames::new` sites each use an entry guard; the depth limit remains 32, rejected entry rolls its increment back, and guard drop restores depth. `producer.rs` and the nested-machine native test file are unchanged from 4d7; `explicit_run.rs` changed later and was checked directly at b235. The eight exact native tests and later depth-restoration unit selector remain historical executions; the current canonical run owns their current receipt. |
| `instruction_fallback_arms` | The accepted dispatch inventory still applies: direct expression dispatch is explicit, frame dispatch routes through the explicit apply groups or named `ExprViewKind` arms, and unsupported kinds return `frame_strategy_error`; there is no catch-all recursive expression fallback. The current schema declares `ExprProcessCommandArgv` as `Scheduled; FrameApply`, and its host-apply inventory includes it. The accepted 7eb strict matrix has 152 rows, zero gaps, zero unmapped paths, all 188 XSH artifacts, and one separate normal direct-IR `ExprIf` proof. |
| `name_resolution_authorities` | The only `src/sema` changes since e048 are `check/builder.rs` and `check/command.rs`; direct review found these implement ordered process-operand checks and do not move lookup/binding authority. Loader, project, and authority paths are unchanged. The prior feature-touch review and eight paired repository/Laputa checks remain scoped evidence; compiler-demand sites are not counted as reachability. |
| `lint_recursive_descents` | The recursive-descent paths (`lint_walk.rs`, `lint_flow.rs`, `lint_sequence.rs`) preserve the shared walk/fact-composition design. I also checked the process-tail migrations in `lint_context_scope.rs`, `lint_prefer_within.rs`, `lint_rules.rs`, and `lint_safety.rs`: they consume ordered options/tail operands and add no second general flow traversal. The general flow walk visits run options, environment, target, then the interleaved argument/redirection tail; flow composition reads child facts rather than recursing a second time. Specialized recursive queries remain the explicitly inventoried exception set in `source-review-6198`. Historical mutation and paired-diagnostic passes are not b235 runs; current lint/native execution belongs to canonical. |
| `module_checks_per_pass` | `src/sema/check/module_driver.rs` and `module_record.rs` are unchanged from 4d7. The retained 4d7 trace records 8 requests, 1 prepare, 7 reuses, 1 checked record, and 7 check hits. It counts cache events, not body traversals, and remains a historical trace rather than a b235 measurement. No new trace is an assigned current gate. |
| `whole_program_stage_copies` | Current stage-lowering paths retain the bounded 4d7/45b4 source review: one compact stage-descriptor copy, with no whole `FullProgram` or executable stage-program clone. Named allocation/execution probes remain pinned to their original runs; they do not establish zero allocation. Current host/native executions are owned by canonical. |
| `open_defects` | `TODO.md` is byte-identical to 45b4, so the prior [open-defects dispositions](../acceptance-review-45b4/open-defects.md) carry forward. The AWK/curl cross-file cause is reproduced on original inputs; unavailable historical child bytes remain unavailable. The old `lint.prefer-collect` report remains unconfirmed, not a current demonstrated defect. Deferred designs/tool-context differences remain deferred. W9 is qualified for b235 under Phase A reuse of accepted proof `7be37796b` (1,690 inputs, 6,760 outcomes, zero mismatches, assignment verified) because compiler and W9-scoped inputs are unchanged from source `0deeb9c7`. The b235 source/hash/input-0 proof `342ae410859d5df3d8c91911454a3ca598a8c41b` is scoped evidence and adds no full-corpus credit. Preserve the incomplete 56bf result as historical. The redundant `3c` comparison stopped at 250 inputs/1,000 outcomes with zero observed mismatches; it is incomplete and has no parity credit. Retain proof `d0e5ac4` and cleanup correction `a83c77655`. No new full W9 comparison is pending. |
| `timings` | No timing evidence was produced here. This is the named post-functional timing gate, not a prerequisite for functional close. Run the existing isolated timing commands only after correctness, with `perf-gate-ready` successful before and with `-1` immediately after; discard overlap or failed readiness. This row must be evidenced before final metric review close. |

## Admission boundary

The structural scan and `check-current` result qualify the four hard-zero structural requirements at b235 under the current policy. They do not claim W9, canonical, resource, or timing execution. Separate accepted route evidence is pinned to 7eb; no older receipt is relabeled.

`metrics.xsh final-close` was not run. Its existing contract requires `--review-evidence`, rejects a temporary envelope, and verifies each of the ten canonical rows exactly once as `review_verified`, with named reviewer and hash-checked artifacts. It also requires exact OIDs for every fixed review-source path at both the review commit and current HEAD, source cleanliness, and final structural zeros. This evidence directory is not itself a final-close bundle. Functional structural qualification can use this report and audit; final metrics close waits for the remaining resource/timing receipts and the source-bound review bundle.

Root accepted functional close for exact source `7eb62cd993e578c13bf6befd68a6a6ac15133c78` and tree `736b4bc16b1bcf07a0250b97e849506c19fb0753`. The immutable receipt is [archived here](functional-close-7eb62cd993e578c13bf6befd68a6a6ac15133c78.json), SHA-256 `349b90176125d95b2c6a550d079f50918680b0ddde251cfcf918f9e6cb594da4`. The accepted [strict route matrix](../../winddown/evidence/route-replay-7eb62cd99/direct-ir-admission/README.md) has 152 rows, zero gaps, zero unmapped paths, 188 XSH proofs, and one separate direct-IR proof. All 428 runtime invocations and source checks passed.

The [root residual evidence](../linux-residual-0deeb9c7/README.md) records 36
unique passes across 37 executions. The standard
context passed 34 cases, then the skipped Unix-socket `sun_path` length case
passed through an owned alias. The namespace case and both positive `NET_RAW`
cases passed. Root reviewed the raw summary, source and pair guards, and
cleanup. The initial Git-guard failure ran zero tests and remains excluded.
The archive retains the initial Git-guard failure, which selected zero tests.

QEMU has no remaining gate. The platform owner accepted Phase A reuse of the
seven guest passes pinned to 0de because all four protected canonical products
were already shown identical at b235. The only 7eb source delta is two
test-only zero-mode spellings. The reuse preserves b235 product pins and 0de
guest pins, with no new 7eb product build. See platform decision commit
`eafce9201`. The b235 canonical pass is accepted and must not be repeated.

W9 is qualified for b235 under the accepted Phase A reuse proof described in
`open_defects`. The 7eb-only test-literal change is outside W9 compiler inputs.
Timing remains post-functional and is required for the final metrics bundle.

The final ten-row metrics bundle and post-functional resource/performance receipts remain pending. No timed run has started: `perf-gate-ready` returned zero, but shared-host load prevents an exclusive measurement window. **perf gate deferred: host busy**. The `performance_prep` artifacts-only build/resources-only observer split is in progress and unreviewed; it needs distinct artifact-qualified state and must discard timing from the resource-only observer. No release resource or binary-size gate has run. The final bundle must list all ten rows once, with actual reviewer names and workspace-relative artifact paths, hashes, commands, methods, and `verified` results. The accepted functional-close receipt does not close these remaining gates.

## Change accounting

Production source delta from b235 to 7eb: **0 lines**. The accepted 7eb source change is two test-only literal spellings. Evidence/docs delta: this README, the immutable 7eb functional-close receipt, the official b235 metrics report, item inventory, and check-current audit. The measured source counts remain pinned to b235; historical evidence pins remain unchanged.
