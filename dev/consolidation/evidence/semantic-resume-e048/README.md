# Semantic and static review at e048b33f2

## Scope

This review qualifies source on `campaign/resume-semantic-20261016` at `e048b33f2`. The worktree was clean when the review began. The [last ten-row review](../acceptance-review-45b4/semantic.md) and the [current stop handoff](../../HANDOFF.md) own prior evidence and open gates.

I inspected the diffs from 45b4 to e048 for runtime dispatch, Linux process sampling, XSHT lint flow, and current test fixtures. I compared unchanged ownership paths against the retained 4d7 and d2 reviews. I reread `TODO.md`, `open-defects.md`, the evaluator, instruction-table, lint-rules, module-check, and traversal designs. I ran no builds, tests, native suites, or timings.

## Source changes since 45b4

`indexed_run.rs` changes the direct evaluator from a tag match to an exhaustive `ExprViewKind` match. Existing handled cases retain their expression bodies. Every remaining view kind has an explicit unsupported result. The schema's route capability mapping is unchanged.

`explicit_run.rs` refactors expression and statement dispatch around view kinds and extracts helpers for statement `if`, `yield`, and `for`. The frame path records route witnesses before dispatch. Its view-kind match ends in an explicit `frame_strategy_error` group. The helper paths preserve the earlier block, yield, and pipeline decisions by source inspection. The route oracle fixture also changed after 45b4, so its older receipts do not qualify e048.

`process.rs` adds handling for Linux stat rows with state `X` and for processes that disappear during sampling. Stat reads skip `NotFound`, `ESRCH`, and permission denial. Rows marked `X` are skipped before live-field validation. Malformed live rows and unexpected stat-read errors still fail. Metadata reads skip only `NotFound` and `ESRCH`. The six focused process tests passed at `3646630b`, and `src/modules/process.rs` is byte-identical from that commit through e048. The bounded 787 integration probe also passed. Those receipts use identical process source. They are not a full suite at e048.

XSHT changes after 45b4 are in lint configuration, hook registration, tests, and CLI batching. `src/sema`, `src/loader.rs`, `src/project.rs`, and `crates/xsht/src/lint_walk.rs` are unchanged from the reviewed source. The changed CLI branch that checks module ownership preserves the old predicate and `continue` path. `lint_sequence.rs` folds nested conditionals into the same short-circuit condition. Lint-rule function pointers replace equivalent closures. These source changes do not alter the reviewed name-resolution or shared-walk authorities.

## Ten semantic rows

| Row | Qualification at e048 | Remaining acceptance boundary |
|---|---|---|
| `operand_behavior_differences` | The schema still classifies all expression tags by in-place and frame strategy. Both current evaluators have explicit dispatch. | Run the 184 mapped route cases and 64 source-order cases on the final freeze. The results of 184 of 184 and 64 of 64 on be0b are historical. |
| `duplicated_per_item_stages` | `serial_pipeline.rs`, `pipeline.rs`, and stage decoding are unchanged from 45b4. The two list and stream differential probes passed at 4d7. | The probes were not run on e048. Preserve 4d7 as the executable pin. |
| `nested_machine_bounds` | The five production `ExplicitFrames::new` paths still acquire five guards through `enter_frame_machine`. The limit remains 32 with rollback on rejected entry and RAII decrement on drop. | Run `tests/xsh/nested-machine-bounds.xsh` and `nested_machine_depth_is_restored_after_an_execution_error` on the final freeze. The retained 4d7 passes are historical. |
| `instruction_fallback_arms` | The direct evaluator names every `ExprViewKind`. The frame evaluator names supported groups and explicitly errors on unsupported groups. There is no catch-all expression fallback in either dispatch. | Runtime selection still needs the same 184 route receipts at e048. A source match does not prove each route was taken. |
| `name_resolution_authorities` | `src/sema`, loader, and project paths are unchanged from 45b4. The XSHT CLI changes do not alter name resolution. | Bind the earlier authority review and eight paired checks to final close source and product identities. |
| `lint_recursive_descents` | `lint_walk.rs` is unchanged from d2 through e048. The general flow walk still records child summaries; sequence hooks read those summaries. The CLI and rule-table edits preserve the selected hooks by source inspection. | d2 probes and paired diagnostics remain historical executions. Lint acceptance for e048 is open. |
| `module_checks_per_pass` | `src/sema/check` is unchanged from 4d7 through e048. The CLI batching edits box the ready record and preserve the module ownership predicate. The retained trace records 8 requests, 1 prepare, 7 reuses, 1 checked record, and 7 check hits. | The trace ran on 4d7, not e048. It counts cache events and does not count body traversals. |
| `whole_program_stage_copies` | Stage lowering is unchanged from 45b4. The prior source review found one compact stage descriptor copy and no cloned `FullProgram` or executable stage program. | The named allocation and execution probes are 45b4 executions. They do not establish zero allocation or an e048 runtime result. |
| `open_defects` | `TODO.md` is unchanged from 45b4. The AWK/curl cause across files is reproduced on original source, while historical child output remains unavailable. The process sampling race repair has focused receipts on identical source. | The historical `lint.prefer-collect` report remains unconfirmed. The full native suite after repair and canonical gate remain open. |
| `timings` | No timing claim is made for e048. | Run current timing gates only under the readiness policy after correctness closes. |

## TODO dispositions

The current [`TODO.md`](../../../../TODO.md) bytes match 45b4. Its design and migration entries remain deferred work or accepted tool-context differences, not demonstrated product defects. Its 7e93 W9 comparison is a historical pass. The latest 56bf attempt is incomplete and supersedes any current parity claim. The earlier [open-defects review](../acceptance-review-45b4/open-defects.md) records those dispositions.

The original-source AWK/curl replay establishes the cause across files. It does not recover the missing historical child streams or environment. The collected fixes have focused passes, but the full canonical run remains open. The `lint.prefer-collect` claim still lacks an original fixture. The retained audit found no current evidence of the alleged bug, which does not disprove it. GNU rm and uu-sort focused results do not replace the full suite after repair. The 787 process-sampling integration probe and six focused module tests qualify their exact process source only.

The 8ca cache counts and private probe counts remain historical. The `module_checks_per_pass` row has the separate 4d7 executed trace. Old clean-lint and performance numbers are not e048 measurements. The campaign's current full canonical, route matrix, mandatory resource gates, and guarded timing acceptance remain open.

## W9 scoped source equivalence

The tracked diff from 56bf to e048 has no changes to `Cargo.toml`, `Cargo.lock`, `src`, `core/lib`, `crates/xsh-registry`, `crates/xsh-root`, `crates/xsh-applets`, `crates/xsh-net`, `third_party`, or the tracked W9 builder, linker, runner, and helper sources. The remaining tracked changes are XSHT lint files, one `xsh-fuzz` file, campaign records, and the independent route matrix and typing winddown tools. The W9 build command targets `-p xsh --test integration`; `xsh-fuzz` and XSHT are separate targets.

The 56bf source receipt also pins the x86_64 Linux musl verification profile, rustflags, image, builder, linker, runner, and helper hashes. The leading stop handoff in `CAMPAIGN.md` supersedes the historical queue below it and directs a new identified strict-comparison attempt using retained immutable inputs and existing admission rules. It does not request fresh captures. By inference, a completed comparison could qualify the same xsh product at e048 if an audit confirms the exact overlay and retained admissions still apply. The d20-to-449 qualification is a precedent for that limited claim. This review does not substitute source equivalence for the required audit and complete comparison.

The actual 56bf comparison remains incomplete: 225 of 1,690 inputs and 900 of 6,760 outcomes, with zero completed ranges. No W9 parity is accepted. Source equivalence cannot complete the missing comparison.

## Current executable results and open gates

No e048 executable result was produced by this review. The latest complete canonical run remains 7b with 13,902 passes, 3 failures, and 78 skips. The focused 184 of 184 route and 64 of 64 source order results are pinned to be0b. The 56bf W9 captures and proofs completed, but comparison r2 stopped at the partial counts above. The e048 canonical, routes, resource gates, and timing gates remain unrun.

## Change accounting

This review changes no production source. It adds this evidence document only. No build, test, native suite, or timing command ran. Source comparisons were read-only.
