# Final freeze 6198: current structural and source review

## Freeze and measurement

- Tracked source before and after: `HEAD=619810fb96ca5dbcb4b521012e6fe5a43d0eb0c2`, tree `43822131f809f792feb33c8b794872a77fadb896`; `git status --short --branch` was clean both times (`master...origin/master`).
- The metric scanner files `dev/consolidation/metrics.xsh`, `dev/consolidation_metrics_current.xsh`, and `dev/consolidation/current_source_policy.xsh` compare identical from source product commit `5366d950ad644fec8652b2a409148c75261a1358` to this freeze. `src/runtime/eval/` compares identical from `d2c842812f87ddc13a84ab77b151b2a3ef0bd2ea` to this freeze.
- Product: `.work/consolidation/resume-candidate-5366/xsh`, SHA-256 `83ffe690b3a7feb215ef7cb5fb5800d8818c1fb71ef67bd8ef712753b5c733a7`. Scanner completed with exit 0; the first wrapper attempt exited 1 before Docker because BusyBox `timeout` rejected GNU `--kill-after`; the corrected invocation ran the scanner.
- Exact successful invocation:

  ```sh
  timeout -k 10 900 docker run --rm --init --platform linux/amd64 --cpus=8 --memory=16g --memory-swap=16g --network=none \
    -v /home/josh/d/laputa-systems/xsh:/home/josh/d/laputa-systems/xsh \
    -v /home/josh/d/laputa-systems/xsh/.work/tmp:/scratch \
    -w /home/josh/d/laputa-systems/xsh --user 1000:1000 -e HOME=/home/compat \
    sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c \
    /usr/bin/env -u LD_PRELOAD TMPDIR=/scratch \
    XSH_BIN=/home/josh/d/laputa-systems/xsh/.work/consolidation/resume-candidate-5366/xsh \
    PATH=/home/josh/d/laputa-systems/xsh/.work/consolidation/resume-candidate-5366:/usr/local/bin:/usr/bin:/bin \
    /home/josh/d/laputa-systems/xsh/.work/consolidation/resume-candidate-5366/xsh \
    /home/josh/d/laputa-systems/xsh/dev/consolidation/metrics.xsh measure \
    --root /home/josh/d/laputa-systems/xsh --source-root src --xsht-source-root crates/xsht/src \
    --output /home/josh/d/laputa-systems/xsh/.work/tmp/consolidation-6198-current-report.json \
    --item-inventory /home/josh/d/laputa-systems/xsh/.work/tmp/consolidation-6198-current-item-allowances.json
  ```

- Docker image ID is `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c` (`linux/amd64`). `/scratch` maps to disk-backed ext4 `.work/tmp`; `--memory-swap=16g` equals the 16-GiB memory cap; `LD_PRELOAD` was unset. No baseline, envelope, review-evidence, build, test, or timing action was run.

## Fresh scan

Report and allowances are `.work/tmp/consolidation-6198-current-report.json` and `.work/tmp/consolidation-6198-current-item-allowances.json`. The report contains 372 files (268 under `src/`, 104 under `crates/xsht/src/`). Production retained source counts: `source_lines_src=215300`, `source_lines_xsht=53179`, total 268479. Physical lines: 249765 + 65776 = 315541.

Hard-zero rows: `manual_payload_calls=0`, `optional_raw_calls=0`, `wide_arena_wildcards=0`, `blanket_dead_code_lines=0`; the current-source policy's lowering resolver count is `lowering_type_resolver_definitions=0`. Static counts: instruction variants 152, pattern variants 17, generated stage variants 46, static type-predicate definitions 1, type-storage mappings 2, and `nested_machine_factory_calls=5`.

The five production `ExplicitFrames::new` sites are `explicit_run.rs:2960,2974,3061` and `producer.rs:418,528`; `explicit_run.rs:556` is one additional direct unit-test construction. Separately, the five source calls to `enter_frame_machine` are `explicit_run.rs:2959,2973,3044` and `producer.rs:347,485`. `MAX_FRAME_MACHINE_DEPTH=32` is checked before frame-machine construction, with failed increments rolled back; `FrameMachineGuard::drop` releases successful increments. These are source/call-site facts, not dynamic nesting or scheduling counts. This audit does not rerun the bound tests.

## Lint source review

The ordinary expression descent is `LintWalker::visit_expr` / `walk_immediate_children` in `crates/xsht/src/lint_walk.rs:70,150`; selected dead-code linting records `FlowFacts` at expression, statement, block, run, command, assignment-target, and builder hooks. `lint_sequence.rs:662-683` composes the recorded summaries. `FlowFacts` is enabled only when the dead-code rule is selected (`lint.rs:669,1203`). Flow equations read child facts rather than recursively walking the syntax tree.

This supports one shared general lint descent for flow. It does not support a repo-wide claim that no independent recursive analyses remain. Rule-specific recursive queries still include `expr_may_assign_local`, `expr_references_name`, and `expr_may_have_effects` in `lint_safety.rs:122,279,570`; expected-type dependence in `lint_annotations.rs:655`; expression ownership in `lint_prefer_tempdir.rs:475`; and stable-read analysis in `lint_write_mode.rs:147`. `lint_try_capture.rs::closed_scalar_expression` uses its own bounded explicit stack. These remain intentional, specialized proofs/queries to inventory; historical lint probes were not rerun here.

## Ten qualification rows

The freshly measured report lists these rows as requirements only; it contains no verified status or evidence. None is closed by `measure` alone.

| Row | Remaining qualification evidence |
|---|---|
| `operand_behavior_differences` | Verify per-instruction behavior comparison and retain oracle tests; arm counts do not establish equivalence. |
| `duplicated_per_item_stages` | Review stage execution ownership; verify generated stage tables and pipeline differential probes. |
| `nested_machine_bounds` | Review every entry and constructor for native-stack nesting and the accepted counter bound; lexical calls do not prove scheduling. |
| `instruction_fallback_arms` | Verify generated instruction dispatch inventory and recursive fallback routes. |
| `name_resolution_authorities` | Review authority boundaries and feature-touch evidence; definitions do not prove semantic authority. |
| `lint_recursive_descents` | Verify traversal inventory and rule probes that identify independent recursive descents. |
| `module_checks_per_pass` | Measure single-pass module-check counts with the shared-import fixture. |
| `whole_program_stage_copies` | Review stage-lowering ownership and allocation probes; generic clone-call counts are not semantic counts. |
| `open_defects` | Review TODO dispositions with regression coverage and recorded reasons. |
| `timings` | Isolated repository-lint and full-gate wall-clock measurements; no timing claim was made here. |

This is a source/structure refresh, not final qualification or a final-close artifact.
