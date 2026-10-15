# Factory semantic review at `4d7b0d0d1`

Read-only source review of the `nested_machine_bounds` requirement. I did not run Cargo, tests, or builds. The current metrics source defines exactly ten review rows (`dev/consolidation_metrics_current.xsh:57-67`): `operand_behavior_differences`, `duplicated_per_item_stages`, `nested_machine_bounds`, `instruction_fallback_arms`, `name_resolution_authorities`, `lint_recursive_descents`, `module_checks_per_pass`, `whole_program_stage_copies`, `open_defects`, and `timings`. This review contributes only to `nested_machine_bounds`; the metric scan and other rows remain requirements, not evidence.

## GO on the five production machine factories

The retained structural review (`dev/consolidation/evidence/source-review-6198/source-review.md:34`) enumerates five production `ExplicitFrames::new` calls plus a test-only direct construction. I checked the current `4d7b0d0d1` source and each call path:

- `explicit_run.rs:2951-2962`: `eval_indexed_with_frames` acquires `enter_frame_machine` before `ExplicitFrames::new`, then prepares argument slots and runs the frame machine.
- `explicit_run.rs:2965-2976`: `eval_indexed_with_frame_slots` likewise acquires the guard before construction and `push_call_with_slots`.
- `explicit_run.rs:3035-3062`: `eval_indexed_work_with_frames` guards before checking the selected program or constructing the statement/block frame. Early errors unwind the same RAII guard.
- `producer.rs:332-418`: each active pull enters a guard at line 347 before delegated-source polling and suspended-frame reconstruction. The later `IndexedProgramGuard` only swaps `indexed_program` on the same `Evaluator` (`producer.rs:25-57`); `CallFrame::from_state` rebuilds the suspended call (`explicit_run.rs:3543-3580`) and is run by the already-counted machine.
- `producer.rs:476-531`: cancellation enters a guard at line 484 only for a started producer. Unstarted producers return before the factory; started producers reconstruct state and run cleanup under the guard.

The unguarded `ExplicitFrames::new` at `explicit_run.rs:556` is inside a unit test. `ExplicitFrames::new` itself initializes a `SmallVec` scheduler (`explicit_run.rs:3582-3595`); it does not execute script code. Calls/slots are prepared into `CallFrame`s inside that scheduler (`4944-5032`) and use the separate 100,000-call check (`5045-5058`), not another machine factory. `CallFrame::from_state` is likewise payload reconstruction, not a hidden factory. `PreparedConstantValue`/`ExprPreparedConstant` are data payload variants (`src/runtime/eval/indexed/full/schema.rs:853-854`, `src/runtime/eval/indexed/full/decode.rs:264`), not frame-machine construction.

`enter_frame_machine` permits at most 32 open machines, rolls back the refused increment, and returns an `Arc`-owning RAII guard; `Drop` decrements it (`explicit_run.rs:2833-2844,2926-2936`). This provides the bound through these entry paths. The test contract is stated in the evaluator design (`designs/evaluator.md:94-99,129-131`) and `docs/SPEC.md:3795-3799`. `tests/xsh/nested-machine-bounds.xsh` exercises stage-block, loop, deferred-call recursion, exact 32/33 producer collection and pull boundaries, and started-producer cleanup (`1-18,21-72,75-162,169+`). The host unit `nested_machine_depth_is_restored_after_an_execution_error` verifies counter restoration and evaluator reuse (`explicit_run.rs:1034-1080`). The independent heap-call limit remains documented by `TODO.md:305-307` and `tests/xsh/call-depth.xsh`.

## Evidence limits

`CAMPAIGN.md:964-966` records an earlier pass of 66 native tests and the host lifecycle test. The durable `source-review-6198` artifact explicitly says it did not rerun bound tests (`source-review.md:34`), and I found no retained bound-test stdout/status artifact in current campaign evidence. Treat the row as source-reviewed, not freshly executed at this freeze. The campaign's current-source native verification must select `tests/xsh/nested-machine-bounds.xsh` and the `nested_machine_depth_is_restored_after_an_execution_error` unit test before final acceptance; no additional test or checklist authority is proposed here.
