# Historical lane review

This inventory compares the pre-winddown lane tips with `e0ede796e`.
There were 824 non-ancestor branches. Of these, 716 had no unique non-merge
patch according to `git cherry`. The remaining 108 contained historical
alternatives requiring archival preservation. Path overlap does not prove
semantic equivalence. All committed tips retain named branch or recovery-tag refs.
The feature-probe instrumentation contains intentional `todo!()` arms and
must remain diagnostic source. Existing Lean feature modules are already
represented in master; the dirty base-equality overlay is separately archived.

## Unique-patch branch inventory: commit IDs, subjects, paths

Each `+` commit below is unique relative to current `master` by `git cherry`; `-` commits are omitted. “Overlap” means `master` has changed that path since the branch merge base, so the patch needs content review for supersession. “No same-path overlap” is only a candidate for live integration, not a claim that it still fits. Repeated SHAs across branches are the same committed source.

Inventory totals: 108 branches, 206 branch/commit occurrences, 173 distinct `+` commit IDs. Exact current-tree matches on all changed paths: 8 branches; branches with changed current content: 100. Per-commit occurrences whose changed path has later `master` overlap: 205; with no same-path overlap: 1. The path-overlap check cannot establish that content is superseded. Subject categories by occurrence: Share 11, test(runtime) 10, feat(formal) 10, test(xsh) 9, Borrow 7, Schedule 7, WIP 7, test(eval) 6, Use 6, Publish 6, test(hash) 5, Route 5, Run 5, Drive 5, Classify 5, Keep 5, refactor(core) 5, Add 4, Prove 4, Preserve 4, Verify 4, wip 4, test(route) 3, test(evaluator) 3, Visit 3, Generate 3, fix(w9) 2, Register 2, Retain 2, Apply 2, Read 2, schema 2, Unify 2, fix 2, test(xsht) 2, Own 2, Match 2, feat(runtime) 2, docs(testing) 1, refactor(types) 1, fix(types) 1, perf(types) 1, test(consolidation) 1, Fix 1, Remove 1, test(sort) 1, bb-patch 1, native-fs 1, fix(dev) 1, perf(metrics) 1, perf 1, Count 1, Cover 1, Collect 1, Import 1, check 1, perf(sema) 1, Execute 1, Check 1, transfer 1, Transfer 1, Decode 1, fix(formal) 1, test 1, Port 1, Require 1, Retire 1, feat(test) 1, Restore 1, refactor 1, expr 1, nl 1, tr 1, uniq 1.
### `audit/system-report-slow` — 1 unique commit(s); overlap/review
Master has since touched: `TODO.md`
- `8d19a84d8597` — docs(testing): retire stale system report timeout — `TODO.md`

### `campaign/closure-record-callers` — 2 unique commit(s); overlap/review
Master has since touched: `src/modules/cli.rs`, `src/modules/signature.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/semantic.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/type_view.rs`, `src/sema/arguments.rs`, `src/sema/builtin_templates.rs`, `src/sema/check/call.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/expr.rs`, `src/sema/check/infer_return.rs`, `src/sema/check/proof.rs`, `src/sema/check/record_require.rs`, `src/sema/check/stmt.rs`, `src/sema/check/stream.rs`, `src/sema/check/types.rs`, `src/sema/constants.rs`, `src/sema/types/resource.rs`, `src/sema/validated.rs`, `src/symbol.rs`, `tests/sema.rs`
- `19150f8af731` — refactor(types): share record type maps — `src/modules/cli.rs`, `src/modules/signature.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/semantic.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/type_view.rs`, `src/sema/arguments.rs`, `src/sema/builtin_templates.rs`, `src/sema/check/call.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/expr.rs`, `src/sema/check/infer_return.rs`, `src/sema/check/proof.rs`, `src/sema/check/record_require.rs`, `src/sema/check/stmt.rs`, `src/sema/check/stream.rs`, `src/sema/check/types.rs`, `src/sema/constants.rs`, `src/sema/types/resource.rs`, `src/sema/validated.rs`, `src/symbol.rs`, `tests/sema.rs`
- `c22697edc887` — fix(types): adapt remaining record map callers — `src/modules/cli.rs`, `src/sema/check/call.rs`

### `campaign/closure-record-core` — 1 unique commit(s); overlap/review
Master has since touched: `src/sema/constraints.rs`, `src/sema/types.rs`
- `6b04a6096fef` — perf(types): share record payloads across type clones — `src/sema/constraints.rs`, `src/sema/types.rs`

### `campaign/feature-probe-208` — 1 unique commit(s); no same-path overlap
- `448fbd6588d4` — test(consolidation): probe feature-touch compiler demand — `crates/xsht/src/format.rs`, `crates/xsht/src/format_equivalence.rs`, `crates/xsht/src/grep.rs`, `crates/xsht/src/lint.rs`, `crates/xsht/src/lint_annotations.rs`, `crates/xsht/src/lint_flow.rs`, `crates/xsht/src/lint_prefer_tempdir.rs`, `crates/xsht/src/lint_safety.rs`, `crates/xsht/src/lint_try_capture.rs`, `crates/xsht/src/lint_walk.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/patterns.rs`, `src/runtime/eval/lower/values.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/sema/check/expr.rs`, `src/sema/check/removed_fs.rs`, `src/sema/check/resource.rs`, `src/sema/check/scopecheck.rs`, `src/sema/check/stmt.rs`, `src/sema/constants.rs`, `src/syntax/arena.rs`, `src/syntax/grouping.rs`

### `campaign/w9-propagating-run-owner-fix-8ca` — 2 unique commit(s); overlap/review
Master has since touched: `dev/consolidation/test_w9_publication_adapter.py`, `dev/consolidation/test_w9_publication_adapter_runner.py`, `dev/consolidation/test_w9_publication_exact_owner_runner.py`, `dev/consolidation/test_w9_publication_reference_facts.py`, `dev/consolidation/test_w9_publication_sources.py`, `dev/consolidation/w9-publication-exact-owners.rs`, `dev/consolidation/w9_publication_adapter.py`, `dev/consolidation/w9_publication_adapter_runner.py`, `dev/consolidation/w9_publication_exact_owner_runner.py`, `dev/consolidation/w9_publication_reference_facts.py`, `dev/consolidation/w9_publication_sources.py`
- `a012be063758` — fix(w9): validate implicit callback propagation owners — `dev/consolidation/test_w9_publication_adapter.py`, `dev/consolidation/test_w9_publication_adapter_runner.py`, `dev/consolidation/test_w9_publication_exact_owner_runner.py`, `dev/consolidation/test_w9_publication_sources.py`, `dev/consolidation/w9-publication-exact-owners.rs`, `dev/consolidation/w9_publication_adapter.py`, `dev/consolidation/w9_publication_adapter_runner.py`, `dev/consolidation/w9_publication_exact_owner_runner.py`, `dev/consolidation/w9_publication_sources.py`
- `e67f4f3a6560` — fix(w9): parse checked stream stages by span — `dev/consolidation/test_w9_publication_adapter.py`, `dev/consolidation/test_w9_publication_reference_facts.py`, `dev/consolidation/w9_publication_adapter.py`, `dev/consolidation/w9_publication_reference_facts.py`

### `codex-boundary-extra-main` — 2 unique commit(s); overlap/review
Master has since touched: `Cargo.toml`, `dev/internal.xsh`, `dev/test_workflows.xsh`, `docs/TESTING.md`, `tests/core_compat_extra.rs`
- `7dbb1b7023bd` — Register bounded core applet host boundary test target — `Cargo.toml`, `dev/internal.xsh`, `dev/test_workflows.xsh`, `docs/TESTING.md`, `tests/core_compat_extra.rs`
- `b2bee444ce0c` — Register split lifecycle tests with their controlled resource gate — `docs/TESTING.md`, `tests/core_compat_extra.rs`

### `codex/boundary-retirement-restart` — 1 unique commit(s); overlap/review
Master has since touched: `dev/docker.xsh`, `dev/test_workflows.xsh`, `dev/tests/test-lifecycle.xsh`, `dev/tests/test-targets.xsh`
- `7039136f97a6` — Fix split resource gate routing — `dev/dist.xsh`, `dev/docker.xsh`, `dev/test_workflows.xsh`, `dev/tests/test-lifecycle.xsh`, `dev/tests/test-targets.xsh`

### `codex/compat-retirement-cleanup` — 1 unique commit(s); overlap/review
Master has since touched: `.claude/agents/xsh-compat-lane.md`, `.claude/skills/xsh-compat-campaign/SKILL.md`, `.claude/skills/xsh-compat-campaign/scripts/lane.py`, `.gitignore`, `core/README.md`, `dev/consolidation/CAMPAIGN.md`, `dev/coreutils-parity.json`, `dev/tests/test-compat-gnu.py`, `dev/tests/test-compat-stage.xsh`, `docs/TESTING.md`, `tools/test_compat_lanes.py`, `tools/test_compat_port_lanes.py`
- `477aa4a5f15c` — Remove compatibility harness consumers outside dev/compat — `.claude/agents/xsh-compat-lane.md`, `.claude/skills/xsh-compat-campaign/SKILL.md`, `.claude/skills/xsh-compat-campaign/scripts/lane.py`, `.gitignore`, `core/README.md`, `dev/consolidation/CAMPAIGN.md`, `dev/coreutils-parity.json`, `dev/tests/test-compat-gnu.py`, `dev/tests/test-compat-stage.xsh`, `docs/TESTING.md`, `tools/test_compat_lanes.py`, `tools/test_compat_port_lanes.py`

### `codex/date-clock-vm` — 1 unique commit(s); overlap/review
Master has since touched: `docs/TESTING.md`, `tests/core_compat_extra/cp.rs`, `tests/core_compat_extra/date_clock.rs`
- `99aacd2bdee5` — Add disposable VM clock boundary cases — `docs/TESTING.md`, `tests/core_compat_extra/cp.rs`, `tests/core_compat_extra/date_clock.rs`

### `codex/fix-sort` — 1 unique commit(s); overlap/review
Master has since touched: `core/tests/data/uutils/sort/empty.txt`, `core/tests/data/uutils/sort/ext_sort.txt`, `core/tests/data/uutils/sort/merge_ints_interleaved_1.txt`, `core/tests/data/uutils/sort/merge_ints_interleaved_2.txt`, `core/tests/data/uutils/sort/merge_ints_interleaved_3.txt`, `core/tests/data/uutils/sort/merge_ints_reversed_1.txt`, `core/tests/data/uutils/sort/merge_ints_reversed_2.txt`, `core/tests/data/uutils/sort/merge_ints_reversed_3.txt`, `core/tests/data/uutils/sort/merge_stable_1.txt`, `core/tests/data/uutils/sort/merge_stable_2.txt`, `core/tests/data/uutils/sort/multiple_files1.txt`, `core/tests/data/uutils/sort/multiple_files2.txt`, `core/tests/test-uu-sort-3.xsh`
- `0c0924947817` — test(sort): add shared fixtures and third uutils chunk — `core/tests/data/uutils/sort/empty.txt`, `core/tests/data/uutils/sort/ext_sort.txt`, `core/tests/data/uutils/sort/merge_ints_interleaved_1.txt`, `core/tests/data/uutils/sort/merge_ints_interleaved_2.txt`, `core/tests/data/uutils/sort/merge_ints_interleaved_3.txt`, `core/tests/data/uutils/sort/merge_ints_reversed_1.txt`, `core/tests/data/uutils/sort/merge_ints_reversed_2.txt`, `core/tests/data/uutils/sort/merge_ints_reversed_3.txt`, `core/tests/data/uutils/sort/merge_stable_1.txt`, `core/tests/data/uutils/sort/merge_stable_2.txt`, `core/tests/data/uutils/sort/multiple_files1.txt`, `core/tests/data/uutils/sort/multiple_files2.txt`, `core/tests/test-uu-sort-3.xsh`

### `ext/bb-patch` — 1 unique commit(s); overlap/review
Master has since touched: `core/patch.xsh`, `core/tests/test-patch.xsh`
- `8dd2e64acb4c` — bb-patch: patch applies unified diffs in XSH with GNU hunk handling (busybox patch: 0 -> 7 pass, 7 fixed, 0 regressed) — `core/patch.xsh`, `core/tests/test-patch.xsh`

### `ext/shuf-random-seed` — 1 unique commit(s); overlap/review
Master has since touched: `core/shuf.xsh`, `core/tests/test-shuf.xsh`
- `1635362d325e` — native-fs: shuf --random-seed (ChaCha12 keyed by SHA3-256) and full-width u64 input ranges (uutils shuf: 69 -> 75 pass, 6 fixed, 0 regressed) — `core/shuf.xsh`, `core/tests/test-shuf.xsh`

### `fix/docker-musl-link-flags` — 1 unique commit(s); overlap/review
Master has since touched: `dev/tests/test-cargo-steps.xsh`, `dev/tests/test-targets.xsh`
- `8b0bb101fe36` — fix(dev): pass musl defsym flags through clang driver — `dev/targets.xsh`, `dev/tests/test-cargo-steps.xsh`, `dev/tests/test-targets.xsh`

### `fix/structural-scan-speed-current` — 1 unique commit(s); overlap/review
Master has since touched: `dev/consolidation/metric-observer-successor.json`, `dev/consolidation_metrics_current.xsh`
- `b68745e6bc7a` — perf(metrics): reduce structural scan exclusion overhead — `dev/consolidation/metric-observer-successor.json`, `dev/consolidation_metrics_current.xsh`

### `integration/route-oracles` — 10 unique commit(s); overlap/review
Master has since touched: `tests/xsh/expression-route-oracles.xsh`, `tests/xsh/fs-path-frame-oracles.xsh`, `tests/xsh/operand-order-collections.xsh`, `tests/xsh/operand-order-control-oracles.xsh`, `tests/xsh/operand-order-host.xsh`, `tests/xsh/operand-order-scalars.xsh`, `tests/xsh/operand-order-simple-statements.xsh`, `tests/xsh/statement-route-oracles.xsh`, `tests/xsh/stdlib/archive.xsh`, `tests/xsh/stdlib/hash.xsh`, `tests/xsh/stdlib/process.xsh`, `tests/xsh/typed-scalar-route-oracles.xsh`
- `10b2e8f10a5c` — test(eval): add statement route oracles — `tests/xsh/statement-route-oracles.xsh`
- `969d83f8e45a` — test(runtime): add expression route oracles — `tests/xsh/expression-route-oracles.xsh`
- `0691c1cc7b86` — test(route): prove scalar operand order and laziness — `tests/xsh/operand-order-scalars.xsh`
- `b39c8a36e292` — test(evaluator): add typed scalar route oracles — `tests/xsh/typed-scalar-route-oracles.xsh`
- `7268ace86f0a` — test(eval): add collection operand-order oracles — `tests/xsh/operand-order-collections.xsh`
- `bc3d4eddecea` — test(runtime): cover operand and payload order — `tests/xsh/operand-order-control-oracles.xsh`
- `15d0d34cef1b` — test(runtime): add statement operand-order oracles — `tests/xsh/operand-order-simple-statements.xsh`
- `b2c1cacecda6` — test(xsh): add host operand order oracles — `tests/xsh/operand-order-host.xsh`
- `265bb581955d` — test(xsh): cover specialized named-call order — `tests/xsh/fs-path-frame-oracles.xsh`, `tests/xsh/stdlib/archive.xsh`, `tests/xsh/stdlib/process.xsh`
- `c1e24f94abb5` — test(hash): cover verify-file operand order — `tests/xsh/stdlib/hash.xsh`

### `integration/route-oracles-v2` — 11 unique commit(s); overlap/review
Master has since touched: `tests/xsh/expression-route-oracles.xsh`, `tests/xsh/fs-path-frame-oracles.xsh`, `tests/xsh/operand-order-collections.xsh`, `tests/xsh/operand-order-control-oracles.xsh`, `tests/xsh/operand-order-host.xsh`, `tests/xsh/operand-order-scalars.xsh`, `tests/xsh/operand-order-simple-statements.xsh`, `tests/xsh/statement-route-oracles.xsh`, `tests/xsh/stdlib/archive.xsh`, `tests/xsh/stdlib/hash.xsh`, `tests/xsh/stdlib/process.xsh`, `tests/xsh/typed-scalar-route-oracles.xsh`
- `ed1b4e3718d1` — test(eval): add statement route oracles — `tests/xsh/statement-route-oracles.xsh`
- `a9c72d663b22` — test(runtime): add expression route oracles — `tests/xsh/expression-route-oracles.xsh`
- `f20c30619c1e` — test(route): prove scalar operand order and laziness — `tests/xsh/operand-order-scalars.xsh`
- `8b822469977f` — test(evaluator): add typed scalar route oracles — `tests/xsh/typed-scalar-route-oracles.xsh`
- `f278fd678b3c` — test(eval): add collection operand-order oracles — `tests/xsh/operand-order-collections.xsh`
- `372b59e52be4` — test(runtime): cover operand and payload order — `tests/xsh/operand-order-control-oracles.xsh`
- `c454fa0f5f22` — test(runtime): add statement operand-order oracles — `tests/xsh/operand-order-simple-statements.xsh`
- `1571f5fba37a` — test(xsh): add host operand order oracles — `tests/xsh/operand-order-host.xsh`
- `a4f26830c49b` — test(xsh): cover specialized named-call order — `tests/xsh/fs-path-frame-oracles.xsh`, `tests/xsh/stdlib/archive.xsh`, `tests/xsh/stdlib/process.xsh`
- `1c9eae5308c6` — test(hash): cover verify-file operand order — `tests/xsh/stdlib/hash.xsh`
- `6000ee14f3f1` — test(xsh): cover non-in-place if selection — `tests/xsh/expression-route-oracles.xsh`

### `integration/special-order-current` — 2 unique commit(s); overlap/review
Master has since touched: `tests/xsh/fs-path-frame-oracles.xsh`, `tests/xsh/stdlib/archive.xsh`, `tests/xsh/stdlib/hash.xsh`, `tests/xsh/stdlib/process.xsh`
- `d50e1d438d4a` — test(xsh): cover specialized named-call order — `tests/xsh/fs-path-frame-oracles.xsh`, `tests/xsh/stdlib/archive.xsh`, `tests/xsh/stdlib/process.xsh`
- `d02f11d681ab` — test(hash): cover verify-file operand order — `tests/xsh/stdlib/hash.xsh`

### `lane/assignment-comprehension-cursors` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/operands.rs`
- `0f8279b25aab` — Use borrowed cursors for assignment and comprehension rows — `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/operands.rs`

### `lane/branch-host-frames` — 2 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/patterns.rs`, `tests/xsh/pattern-conditionals.xsh`
- `9b9cbe43d3a9` — Route branch and host leaf expressions through frames — `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/patterns.rs`
- `df415a70d1d8` — Retain ExprPatternIf branch cursors in frames — `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `tests/xsh/pattern-conditionals.xsh`

### `lane/cold-frame-state` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`
- `10e29a4f54f0` — perf: box cold evaluator continuation state — `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`

### `lane/consolidation-checker-foundation` — 1 unique commit(s); overlap/review
Master has since touched: `src/sema/check.rs`, `src/sema/check/checked_declarations.rs`, `src/sema/check/compact.rs`, `src/sema/check/decl.rs`, `src/sema/check/local_inference.rs`, `src/sema/check/stmt.rs`, `src/sema/check/types.rs`, `tests/xsh/compact-declarations.xsh`
- `fadd2b7fbe9e` — Publish checker declaration facts directly — `src/sema/check.rs`, `src/sema/check/checked_declarations.rs`, `src/sema/check/compact.rs`, `src/sema/check/decl.rs`, `src/sema/check/local_inference.rs`, `src/sema/check/stmt.rs`, `src/sema/check/types.rs`, `tests/xsh/compact-declarations.xsh`

### `lane/consolidation-defer-frames-current` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/lowered_run/indexed_run/context.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `tests/xsh/nested-machine-bounds.xsh`
- `e353c6ca1c58` — Run deferred callbacks on active frames — `src/runtime/eval/lowered_run/indexed_run/context.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `tests/xsh/nested-machine-bounds.xsh`

### `lane/consolidation-driver-row-codec` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/driver.rs`
- `113183fc5ad8` — Borrow indexed driver payload rows — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/driver.rs`

### `lane/consolidation-driver-row-views` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/driver.rs`
- `113183fc5ad8` — Borrow indexed driver payload rows — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/driver.rs`

### `lane/consolidation-feature-probe` — 2 unique commit(s); overlap/review
Master has since touched: `dev/consolidation/metrics.xsh`, `dev/consolidation_metrics.xsh`, `dev/tests/test-consolidation-ratchets.xsh`
- `3d92b525e4a9` — Count generated instruction schema rows in metrics — `dev/consolidation/metrics.xsh`, `dev/consolidation_metrics.xsh`, `dev/tests/test-consolidation-ratchets.xsh`
- `98741da7e30e` — Cover instruction schema class metadata — `dev/tests/test-consolidation-ratchets.xsh`

### `lane/consolidation-frame-apply-next` — 5 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/values.rs`, `tests/xsh/frame-apply-oracles.xsh`, `tests/xsh/test-declarations.xsh`
- `250dd2bab4d4` — Schedule typed scalar evaluation on frames — `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/values.rs`, `tests/xsh/test-declarations.xsh`
- `0f642b68a54c` — Prove typed apply routes under forced frames — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `tests/xsh/frame-apply-oracles.xsh`
- `aad685da584c` — Visit retained values in typed frame states — `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`
- `19ec74548946` — Preserve instruction ordinals after dead-row deletion — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`
- `814efb94bb3d` — Schedule remaining eager expression apply frames — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `tests/xsh/frame-apply-oracles.xsh`

### `lane/consolidation-fs-path-frame-apply` — 8 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/values.rs`, `src/runtime/eval/lowered_run/module_archive.rs`, `tests/xsh/frame-apply-oracles.xsh`, `tests/xsh/fs-path-frame-oracles.xsh`, `tests/xsh/test-declarations.xsh`
- `5f5c29526f77` — Schedule typed scalar evaluation on frames — `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/values.rs`, `tests/xsh/test-declarations.xsh`
- `d43a453ae9d6` — Prove typed apply routes under forced frames — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `tests/xsh/frame-apply-oracles.xsh`
- `b9c301e93b3a` — Visit retained values in typed frame states — `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`
- `48aaaab88f32` — Preserve instruction ordinals after dead-row deletion — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`
- `4a79d67d1f1a` — Schedule remaining eager expression apply frames — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `tests/xsh/frame-apply-oracles.xsh`
- `494d46c5b593` — Add filesystem frame apply candidate — `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `tests/xsh/fs-path-frame-oracles.xsh`
- `3136dfe0e1a3` — Route filesystem host rows through explicit frames — `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `tests/xsh/fs-path-frame-oracles.xsh`
- `2f94946caf5a` — Borrow operands in filesystem frame apply — `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/module_archive.rs`, `tests/xsh/fs-path-frame-oracles.xsh`

### `lane/consolidation-lint-probe-transport` — 1 unique commit(s); overlap/review
Master has since touched: `crates/xsht/src/cli/check.rs`, `crates/xsht/src/cli/lint.rs`, `src/frontend.rs`, `src/loader.rs`, `src/sema/check.rs`, `src/sema/check/call.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/effect_bounds.rs`, `src/sema/check/expr.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/types.rs`
- `fa11b1bd8cb4` — Share checked module facts across workspace entries — `crates/xsht/src/cli/check.rs`, `crates/xsht/src/cli/lint.rs`, `src/frontend.rs`, `src/loader.rs`, `src/sema/check.rs`, `src/sema/check/call.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/effect_bounds.rs`, `src/sema/check/expr.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/types.rs`

### `lane/consolidation-lint-rule-table` — 1 unique commit(s); overlap/review
Master has since touched: `crates/xsht/src/app.rs`, `crates/xsht/src/lint.rs`, `crates/xsht/src/lint_annotations.rs`, `crates/xsht/src/lint_prefer_inferred_proc_return.rs`, `crates/xsht/src/lint_redundant_close.rs`, `crates/xsht/src/lint_redundant_propagation.rs`, `crates/xsht/src/lint_rules.rs`, `crates/xsht/src/lint_sequence.rs`, `crates/xsht/src/lint_walk.rs`
- `3612276fb1a9` — Drive lint callbacks from complete rule table — `crates/xsht/src/app.rs`, `crates/xsht/src/lint.rs`, `crates/xsht/src/lint_annotations.rs`, `crates/xsht/src/lint_prefer_inferred_proc_return.rs`, `crates/xsht/src/lint_redundant_close.rs`, `crates/xsht/src/lint_redundant_propagation.rs`, `crates/xsht/src/lint_rules.rs`, `crates/xsht/src/lint_sequence.rs`, `crates/xsht/src/lint_walk.rs`

### `lane/consolidation-lint-rule-table-joint` — 1 unique commit(s); overlap/review
Master has since touched: `crates/xsht/src/app.rs`, `crates/xsht/src/lint.rs`, `crates/xsht/src/lint_annotations.rs`, `crates/xsht/src/lint_prefer_inferred_proc_return.rs`, `crates/xsht/src/lint_redundant_close.rs`, `crates/xsht/src/lint_redundant_propagation.rs`, `crates/xsht/src/lint_rules.rs`, `crates/xsht/src/lint_sequence.rs`, `crates/xsht/src/lint_walk.rs`
- `7a5ae236a80d` — Drive lint callbacks from complete rule table — `crates/xsht/src/app.rs`, `crates/xsht/src/lint.rs`, `crates/xsht/src/lint_annotations.rs`, `crates/xsht/src/lint_prefer_inferred_proc_return.rs`, `crates/xsht/src/lint_redundant_close.rs`, `crates/xsht/src/lint_redundant_propagation.rs`, `crates/xsht/src/lint_rules.rs`, `crates/xsht/src/lint_sequence.rs`, `crates/xsht/src/lint_walk.rs`

### `lane/consolidation-lint-table-currentroot` — 2 unique commit(s); overlap/review
Master has since touched: `crates/xsht/src/app.rs`, `crates/xsht/src/cli/lint.rs`, `crates/xsht/src/lint.rs`, `crates/xsht/src/lint_annotations.rs`, `crates/xsht/src/lint_prefer_inferred_proc_return.rs`, `crates/xsht/src/lint_reachability.rs`, `crates/xsht/src/lint_redundant_close.rs`, `crates/xsht/src/lint_redundant_propagation.rs`, `crates/xsht/src/lint_rules.rs`, `crates/xsht/src/lint_sequence.rs`, `crates/xsht/src/lint_walk.rs`, `tests/xsh/lint-collections.xsh`, `tests/xsh/lint-dead-code.xsh`, `tests/xsh/lint2-expressions.xsh`
- `26886462d1dd` — Drive lint callbacks from complete rule table — `crates/xsht/src/app.rs`, `crates/xsht/src/lint.rs`, `crates/xsht/src/lint_annotations.rs`, `crates/xsht/src/lint_prefer_inferred_proc_return.rs`, `crates/xsht/src/lint_redundant_close.rs`, `crates/xsht/src/lint_redundant_propagation.rs`, `crates/xsht/src/lint_rules.rs`, `crates/xsht/src/lint_sequence.rs`, `crates/xsht/src/lint_walk.rs`
- `a280a307445f` — Collect lint reachability facts during shared walk — `crates/xsht/src/cli/lint.rs`, `crates/xsht/src/lint.rs`, `crates/xsht/src/lint_reachability.rs`, `crates/xsht/src/lint_sequence.rs`, `crates/xsht/src/lint_walk.rs`, `tests/xsh/lint-collections.xsh`, `tests/xsh/lint-dead-code.xsh`, `tests/xsh/lint2-expressions.xsh`

### `lane/consolidation-lint-table-root8187` — 1 unique commit(s); overlap/review
Master has since touched: `crates/xsht/src/app.rs`, `crates/xsht/src/lint.rs`, `crates/xsht/src/lint_annotations.rs`, `crates/xsht/src/lint_prefer_inferred_proc_return.rs`, `crates/xsht/src/lint_redundant_close.rs`, `crates/xsht/src/lint_redundant_propagation.rs`, `crates/xsht/src/lint_rules.rs`, `crates/xsht/src/lint_sequence.rs`, `crates/xsht/src/lint_walk.rs`
- `5fe285eda2db` — Drive lint callbacks from complete rule table — `crates/xsht/src/app.rs`, `crates/xsht/src/lint.rs`, `crates/xsht/src/lint_annotations.rs`, `crates/xsht/src/lint_prefer_inferred_proc_return.rs`, `crates/xsht/src/lint_redundant_close.rs`, `crates/xsht/src/lint_redundant_propagation.rs`, `crates/xsht/src/lint_rules.rs`, `crates/xsht/src/lint_sequence.rs`, `crates/xsht/src/lint_walk.rs`

### `lane/consolidation-lint-table-rootbfb` — 1 unique commit(s); overlap/review
Master has since touched: `crates/xsht/src/app.rs`, `crates/xsht/src/lint.rs`, `crates/xsht/src/lint_annotations.rs`, `crates/xsht/src/lint_prefer_inferred_proc_return.rs`, `crates/xsht/src/lint_redundant_close.rs`, `crates/xsht/src/lint_redundant_propagation.rs`, `crates/xsht/src/lint_rules.rs`, `crates/xsht/src/lint_sequence.rs`, `crates/xsht/src/lint_walk.rs`
- `504de1114075` — Drive lint callbacks from complete rule table — `crates/xsht/src/app.rs`, `crates/xsht/src/lint.rs`, `crates/xsht/src/lint_annotations.rs`, `crates/xsht/src/lint_prefer_inferred_proc_return.rs`, `crates/xsht/src/lint_redundant_close.rs`, `crates/xsht/src/lint_redundant_propagation.rs`, `crates/xsht/src/lint_rules.rs`, `crates/xsht/src/lint_sequence.rs`, `crates/xsht/src/lint_walk.rs`

### `lane/consolidation-local-typed-callee` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lower/commands.rs`, `src/runtime/eval/lower/control.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/patterns.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/runtime/eval/lower/types.rs`, `src/runtime/eval/lower/values.rs`, `src/sema/check.rs`, `src/sema/check/command.rs`, `src/sema/check/scopecheck.rs`, `tests/xsh/constant-resolution.xsh`
- `a5956727e674` — Apply frozen BindingId SlotScope adapter — `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lower/commands.rs`, `src/runtime/eval/lower/control.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/patterns.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/runtime/eval/lower/types.rs`, `src/runtime/eval/lower/values.rs`, `src/sema/check.rs`, `src/sema/check/command.rs`, `src/sema/check/scopecheck.rs`, `tests/xsh/constant-resolution.xsh`

### `lane/consolidation-lower-bindings-final` — 2 unique commit(s); overlap/review
Master has since touched: `crates/xsht/src/cli/lint.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lower/commands.rs`, `src/runtime/eval/lower/control.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/patterns.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/runtime/eval/lower/types.rs`, `src/runtime/eval/lower/values.rs`, `src/sema/check.rs`, `src/sema/check/command.rs`, `src/sema/check/scopecheck.rs`, `src/syntax/arena.rs`, `tests/xsh/constant-resolution.xsh`, `tests/xsh/stdlib/streams.xsh`, `tests/xsh/tooling-lint.xsh`
- `3bbeb7f0f97a` — Use checked binding ids for lowering adapters — `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lower/commands.rs`, `src/runtime/eval/lower/control.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/patterns.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/runtime/eval/lower/types.rs`, `src/runtime/eval/lower/values.rs`, `src/sema/check.rs`, `src/sema/check/command.rs`, `src/sema/check/scopecheck.rs`, `src/syntax/arena.rs`, `tests/xsh/constant-resolution.xsh`, `tests/xsh/stdlib/streams.xsh`, `tests/xsh/tooling-lint.xsh`
- `362816c4221a` — Import check options in lint unit tests — `crates/xsht/src/cli/lint.rs`

### `lane/consolidation-main-spread` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval.rs`, `src/sema/check.rs`, `tests/xsh/tooling-check-session.xsh`
- `ec49f15a6f76` — check invalid proc main argument binding earlier — `src/runtime/eval.rs`, `src/sema/check.rs`, `tests/xsh/tooling-check-session.xsh`

### `lane/consolidation-module-bundle-view` — 1 unique commit(s); overlap/review
Master has since touched: `src/sema/check.rs`, `src/sema/check/decl.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/infer_return.rs`, `src/sema/check/local_inference.rs`, `src/sema/constants.rs`, `src/sema/wire_enums.rs`
- `ad20ae13fe40` — perf(sema): scope preparation to reachable modules — `src/sema/check.rs`, `src/sema/check/decl.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/infer_return.rs`, `src/sema/check/local_inference.rs`, `src/sema/constants.rs`, `src/sema/wire_enums.rs`

### `lane/consolidation-module-record-driver` — 6 unique commit(s); overlap/review
Master has since touched: `crates/xsht/src/cli/check.rs`, `crates/xsht/src/cli/lint.rs`, `crates/xsht/src/xsht/test.rs`, `src/frontend.rs`, `src/loader.rs`, `src/modules/cli.rs`, `src/modules/cli/gnu.rs`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/sema/check.rs`, `src/sema/check/call.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/effect_bounds.rs`, `src/sema/check/expr.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/infer_return.rs`, `src/sema/check/local_inference.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/resource.rs`, `src/sema/check/scopecheck.rs`, `src/sema/check/types.rs`, `src/sema/constants.rs`, `src/sema/wire_enums.rs`, `tests/sema.rs`, `tests/sema_bundle_view.rs`, `tests/xsh/checker-modules.xsh`, `tests/xsh/context-scopes.xsh`
- `dc4af304d8f8` — Share checked module facts across workspace entries — `crates/xsht/src/cli/check.rs`, `crates/xsht/src/cli/lint.rs`, `src/frontend.rs`, `src/loader.rs`, `src/sema/check.rs`, `src/sema/check/call.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/effect_bounds.rs`, `src/sema/check/expr.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/types.rs`
- `d8ef181afc2b` — Publish checked RunForm types for transfer facts — `src/sema/check/expr.rs`, `src/sema/check/resource.rs`, `tests/xsh/context-scopes.xsh`
- `63d79c2f2bf5` — Classify declaration statement variants — `src/sema/check/decl.rs`
- `d00ccabf93ec` — WIP: complete session-owned module records — `crates/xsht/src/cli/check.rs`, `crates/xsht/src/xsht/test.rs`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/sema/check.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/infer_return.rs`, `src/sema/check/local_inference.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/scopecheck.rs`, `src/sema/constants.rs`, `src/sema/wire_enums.rs`
- `95e0e041ac7f` — Share checked module records across entries — `src/modules/cli.rs`, `src/modules/cli/gnu.rs`, `src/sema/check.rs`, `src/sema/check/decl.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/resource.rs`, `src/sema/constants.rs`, `tests/sema.rs`, `tests/sema_bundle_view.rs`, `tests/xsh/checker-modules.xsh`
- `39763a661547` — Keep parity helper out of permanent tests — `tests/sema.rs`

### `lane/consolidation-partition-lower` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`
- `1cd0e625dbbf` — Generate indexed instruction rows and borrowed views from one schema — `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`

### `lane/consolidation-partition-modules` — 2 unique commit(s); overlap/review
Master has since touched: `src/frontend.rs`, `src/runtime/eval.rs`, `src/runtime/eval/call_plan.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/api.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lower/commands.rs`, `src/runtime/eval/lower/control.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/operands.rs`, `src/sema/check.rs`, `src/sema/check/args.rs`, `src/sema/check/command.rs`, `src/sema/check/compact.rs`, `tests/xsh/module-command-contracts.xsh`
- `2191aeec9bf9` — Generate indexed instruction rows and borrowed views from one schema — `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`
- `96ddf70f10b1` — Execute checked module commands through prepared module call plans — `src/frontend.rs`, `src/runtime/eval/call_plan.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/api.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lower/commands.rs`, `src/runtime/eval/lower/control.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/operands.rs`, `src/sema/check.rs`, `src/sema/check/args.rs`, `src/sema/check/command.rs`, `src/sema/check/compact.rs`, `tests/xsh/module-command-contracts.xsh`

### `lane/consolidation-process-execution-frames` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/pipeline.rs`, `src/runtime/eval/lowered_run/indexed_run/process.rs`, `tests/xsh/stdlib/process_frames.xsh`
- `16a73ee14c5c` — Run process operations on typed frames — `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/pipeline.rs`, `src/runtime/eval/lowered_run/indexed_run/process.rs`, `tests/xsh/stdlib/process_frames.xsh`

### `lane/consolidation-r3-resource-bridge` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lower/commands.rs`, `src/runtime/eval/lower/control.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/patterns.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/runtime/eval/lower/types.rs`, `src/runtime/eval/lower/values.rs`, `src/sema/check.rs`, `src/sema/check/command.rs`, `src/sema/check/scopecheck.rs`, `tests/xsh/constant-resolution.xsh`
- `5c5372f2ad9f` — Apply BindingId SlotScope adapter prerequisite — `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lower/commands.rs`, `src/runtime/eval/lower/control.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/patterns.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/runtime/eval/lower/types.rs`, `src/runtime/eval/lower/values.rs`, `src/sema/check.rs`, `src/sema/check/command.rs`, `src/sema/check/scopecheck.rs`, `tests/xsh/constant-resolution.xsh`

### `lane/consolidation-resources` — 1 unique commit(s); overlap/review
Master has since touched: `src/diagnostic/code.rs`, `src/frontend.rs`, `src/sema/check.rs`, `src/sema/check/compact.rs`, `src/sema/check/resource.rs`, `src/sema/types.rs`, `src/sema/types/resource.rs`, `tests/xsh/resource-ownership.xsh`
- `76fa3df42220` — Check use-after-release for exact local resources — `src/diagnostic/code.rs`, `src/frontend.rs`, `src/sema/check.rs`, `src/sema/check/compact.rs`, `src/sema/check/resource.rs`, `src/sema/types.rs`, `src/sema/types/resource.rs`, `tests/xsh/resource-ownership.xsh`

### `lane/consolidation-stage-engine-cb5` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/pipeline.rs`, `src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs`, `src/sema/check/stream.rs`, `tests/xsh/stdlib/stage-engine-oracles.xsh`, `tests/xsh/stdlib/stage-result-contracts.xsh`, `tests/xsh/stdlib/streams.xsh`
- `67eeb7448caa` — Run pipeline stages on explicit frames — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/pipeline.rs`, `src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs`, `src/sema/check/stream.rs`, `tests/xsh/stdlib/stage-engine-oracles.xsh`, `tests/xsh/stdlib/stage-result-contracts.xsh`, `tests/xsh/stdlib/streams.xsh`

### `lane/consolidation-stream-sweep-cutover` — 2 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval.rs`, `src/runtime/eval/lowered_run.rs`, `src/runtime/eval/lowered_run/indexed_run/calls.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/producer.rs`, `src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs`, `src/runtime/eval/stream.rs`, `src/runtime/value.rs`, `tests/fixtures/runtime/lazy-stream-producers.xsh`, `tests/xsh/resource-ownership.xsh`, `tests/xsh/stream-producers.xsh`
- `6f888ad40452` — transfer captured resources with owned streams — `src/runtime/eval.rs`, `src/runtime/eval/lowered_run/indexed_run/calls.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/producer.rs`, `src/runtime/eval/stream.rs`, `src/runtime/value.rs`, `tests/fixtures/runtime/lazy-stream-producers.xsh`, `tests/xsh/resource-ownership.xsh`, `tests/xsh/stream-producers.xsh`
- `a4aa62a58eb6` — Transfer resources from retained producer work — `src/runtime/eval.rs`, `src/runtime/eval/lowered_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/producer.rs`, `src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs`, `src/runtime/eval/stream.rs`

### `lane/consolidation-type-fact-roots` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/types.rs`
- `222d10697f2f` — Read annotation roots from checked type facts — `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/types.rs`

### `lane/consolidation-type-fact-tail` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lower/patterns.rs`, `src/runtime/eval/lower/types.rs`
- `5b42beb19ccf` — Read resolved type facts during lowering — `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lower/patterns.rs`, `src/runtime/eval/lower/types.rs`

### `lane/consolidation-typed-apply-oracles` — 2 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/values.rs`, `tests/xsh/frame-apply-oracles.xsh`, `tests/xsh/test-declarations.xsh`
- `4dacf0c04c55` — Schedule typed scalar evaluation on frames — `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/values.rs`, `tests/xsh/test-declarations.xsh`
- `29dd20076f69` — Prove typed apply routes under forced frames — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `tests/xsh/frame-apply-oracles.xsh`

### `lane/consolidation-typed-call-verifier` — 7 unique commit(s); overlap/review
Master has since touched: `docs/ARCHITECTURE.md`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/indexed/semantic.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/driver.rs`, `tests/xsh/typed-callables.xsh`
- `5a2734ca4384` — Verify typed call callee types structurally — `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/semantic.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/calls.rs`, `tests/xsh/typed-callables.xsh`
- `dbbf6286c4f6` — Verify typed-call metadata against runtime representation — `src/runtime/eval/indexed/full.rs`
- `a3f4606a7491` — wip: retain typed callable proof regressions — `src/runtime/eval/indexed/full.rs`, `tests/xsh/typed-callables.xsh`
- `4ee6ebb91ba8` — WIP preserve typed callable verifier before driver row integration — `docs/ARCHITECTURE.md`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/semantic.rs`, `src/runtime/eval/lower.rs`
- `c4a38400c3bf` — schema: add generated driver payload row codecs — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/driver.rs`
- `dbeaf75738c9` — Unify borrowed driver element codecs — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/lowered_run/indexed_run/driver.rs`
- `dbd030c572af` — fix: collect borrowed module exports — `src/runtime/eval/indexed/full.rs`

### `lane/consolidation-typed-scalar` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/values.rs`, `tests/xsh/test-declarations.xsh`
- `82168e886ee1` — Schedule typed scalar evaluation on frames — `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/values.rs`, `tests/xsh/test-declarations.xsh`

### `lane/consolidation-w1-scalar-readers` — 5 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/values.rs`, `tests/xsh/frame-apply-oracles.xsh`, `tests/xsh/test-declarations.xsh`
- `d050bb698d8e` — Schedule typed scalar evaluation on frames — `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/values.rs`, `tests/xsh/test-declarations.xsh`
- `376d4399d459` — Prove typed apply routes under forced frames — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `tests/xsh/frame-apply-oracles.xsh`
- `dd34ae2e2633` — Preserve instruction ordinals after dead-row deletion — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`
- `20b9293d80d6` — Visit retained values in typed frame states — `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`
- `743c7ffcca48` — Decode typed scalar frame operands through borrowed views — `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`

### `lane/discovery-context-cancellation-fix` — 6 unique commit(s); overlap/review
Master has since touched: `crates/xsht/src/cli/check.rs`, `crates/xsht/src/cli/lint.rs`, `crates/xsht/src/xsht/test.rs`, `src/frontend.rs`, `src/loader.rs`, `src/modules/cli.rs`, `src/modules/cli/gnu.rs`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/sema/check.rs`, `src/sema/check/call.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/effect_bounds.rs`, `src/sema/check/expr.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/infer_return.rs`, `src/sema/check/local_inference.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/resource.rs`, `src/sema/check/scopecheck.rs`, `src/sema/check/types.rs`, `src/sema/constants.rs`, `src/sema/wire_enums.rs`, `tests/sema.rs`, `tests/sema_bundle_view.rs`, `tests/xsh/checker-modules.xsh`, `tests/xsh/context-scopes.xsh`
- `dc4af304d8f8` — Share checked module facts across workspace entries — `crates/xsht/src/cli/check.rs`, `crates/xsht/src/cli/lint.rs`, `src/frontend.rs`, `src/loader.rs`, `src/sema/check.rs`, `src/sema/check/call.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/effect_bounds.rs`, `src/sema/check/expr.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/types.rs`
- `d8ef181afc2b` — Publish checked RunForm types for transfer facts — `src/sema/check/expr.rs`, `src/sema/check/resource.rs`, `tests/xsh/context-scopes.xsh`
- `63d79c2f2bf5` — Classify declaration statement variants — `src/sema/check/decl.rs`
- `d00ccabf93ec` — WIP: complete session-owned module records — `crates/xsht/src/cli/check.rs`, `crates/xsht/src/xsht/test.rs`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/sema/check.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/infer_return.rs`, `src/sema/check/local_inference.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/scopecheck.rs`, `src/sema/constants.rs`, `src/sema/wire_enums.rs`
- `95e0e041ac7f` — Share checked module records across entries — `src/modules/cli.rs`, `src/modules/cli/gnu.rs`, `src/sema/check.rs`, `src/sema/check/decl.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/resource.rs`, `src/sema/constants.rs`, `tests/sema.rs`, `tests/sema_bundle_view.rs`, `tests/xsh/checker-modules.xsh`
- `39763a661547` — Keep parity helper out of permanent tests — `tests/sema.rs`

### `lane/discovery-context-lowering-fix` — 6 unique commit(s); overlap/review
Master has since touched: `crates/xsht/src/cli/check.rs`, `crates/xsht/src/cli/lint.rs`, `crates/xsht/src/xsht/test.rs`, `src/frontend.rs`, `src/loader.rs`, `src/modules/cli.rs`, `src/modules/cli/gnu.rs`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/sema/check.rs`, `src/sema/check/call.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/effect_bounds.rs`, `src/sema/check/expr.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/infer_return.rs`, `src/sema/check/local_inference.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/resource.rs`, `src/sema/check/scopecheck.rs`, `src/sema/check/types.rs`, `src/sema/constants.rs`, `src/sema/wire_enums.rs`, `tests/sema.rs`, `tests/sema_bundle_view.rs`, `tests/xsh/checker-modules.xsh`, `tests/xsh/context-scopes.xsh`
- `dc4af304d8f8` — Share checked module facts across workspace entries — `crates/xsht/src/cli/check.rs`, `crates/xsht/src/cli/lint.rs`, `src/frontend.rs`, `src/loader.rs`, `src/sema/check.rs`, `src/sema/check/call.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/effect_bounds.rs`, `src/sema/check/expr.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/types.rs`
- `d8ef181afc2b` — Publish checked RunForm types for transfer facts — `src/sema/check/expr.rs`, `src/sema/check/resource.rs`, `tests/xsh/context-scopes.xsh`
- `63d79c2f2bf5` — Classify declaration statement variants — `src/sema/check/decl.rs`
- `d00ccabf93ec` — WIP: complete session-owned module records — `crates/xsht/src/cli/check.rs`, `crates/xsht/src/xsht/test.rs`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/sema/check.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/infer_return.rs`, `src/sema/check/local_inference.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/scopecheck.rs`, `src/sema/constants.rs`, `src/sema/wire_enums.rs`
- `95e0e041ac7f` — Share checked module records across entries — `src/modules/cli.rs`, `src/modules/cli/gnu.rs`, `src/sema/check.rs`, `src/sema/check/decl.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/resource.rs`, `src/sema/constants.rs`, `tests/sema.rs`, `tests/sema_bundle_view.rs`, `tests/xsh/checker-modules.xsh`
- `39763a661547` — Keep parity helper out of permanent tests — `tests/sema.rs`

### `lane/eager-classification-closure` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`
- `621ce05d470d` — Generate eager atom classification from schema — `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`

### `lane/lean-call-order-integration` — 1 unique commit(s); overlap/review
Master has since touched: `formal/README.md`, `formal/Xsh.lean`, `formal/coverage.json`, `formal/tests/inventory/test_check_inventory.py`, `xsh-lean.md`
- `ab916098e3b1` — feat(formal): integrate call argument ordering fragment — `formal/README.md`, `formal/Xsh.lean`, `formal/coverage.json`, `formal/tests/inventory/test_check_inventory.py`, `xsh-lean.md`

### `lane/lean-iteration-snapshot` — 1 unique commit(s); overlap/review
Master has since touched: `formal/Xsh/Iteration/Snapshot.lean`
- `7202ab29f79f` — feat(formal): model finite loop snapshots — `formal/Xsh/Iteration/Snapshot.lean`

### `lane/lean-json-encoding` — 1 unique commit(s); overlap/review
Master has since touched: `formal/Xsh/Json/Encoding.lean`
- `fa5073974592` — feat(formal): model bounded compact JSON encoding — `formal/Xsh/Json/Encoding.lean`

### `lane/lean-json-path` — 1 unique commit(s); overlap/review
Master has since touched: `formal/Xsh/Json/Path.lean`
- `f01938e39473` — feat(formal): model JSON path updates — `formal/Xsh/Json/Path.lean`

### `lane/lean-key-order-integration` — 1 unique commit(s); overlap/review
Master has since touched: `formal/README.md`, `formal/Xsh.lean`, `formal/coverage.json`, `formal/tests/inventory/test_check_inventory.py`, `xsh-lean.md`
- `8e4672c99cb6` — feat(formal): integrate primitive key ordering — `formal/README.md`, `formal/Xsh.lean`, `formal/coverage.json`, `formal/tests/inventory/test_check_inventory.py`, `xsh-lean.md`

### `lane/lean-loop-control` — 1 unique commit(s); overlap/review
Master has since touched: `formal/Xsh/Loops/ControlFlow.lean`
- `2240fabf66eb` — feat(formal): add finite loop control model — `formal/Xsh/Loops/ControlFlow.lean`

### `lane/lean-path-fields` — 1 unique commit(s); overlap/review
Master has since touched: `formal/Xsh/Paths/Fields.lean`
- `878ad20ae213` — feat(formal): model lexical path fields — `formal/Xsh/Paths/Fields.lean`

### `lane/lean-path-normalize` — 2 unique commit(s); overlap/review
Master has since touched: `formal/Xsh/Paths/Normalize.lean`
- `a7229fc835d8` — feat(formal): model lexical path normalization — `formal/Xsh/Paths/Normalize.lean`
- `8e78cf3706d9` — fix(formal): strengthen path normalization witnesses — `formal/Xsh/Paths/Normalize.lean`

### `lane/lean-result-fallback` — 1 unique commit(s); overlap/review
Master has since touched: `formal/Xsh/Results/Fallback.lean`
- `2fd9d978b27a` — feat(formal): model result fallback semantics — `formal/Xsh/Results/Fallback.lean`

### `lane/lean-unicode-integration` — 1 unique commit(s); overlap/review
Master has since touched: `formal/README.md`, `formal/Xsh.lean`, `formal/coverage.json`, `formal/tests/inventory/test_check_inventory.py`, `xsh-lean.md`
- `961a5c95cea0` — feat(formal): inventory Unicode slice proofs — `formal/README.md`, `formal/Xsh.lean`, `formal/coverage.json`, `formal/tests/inventory/test_check_inventory.py`, `xsh-lean.md`

### `lane/loop-retry-frames` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/context.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `tests/xsh/retry.xsh`
- `266ea41a4f16` — Run loop and retry expressions in frames — `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/context.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `tests/xsh/retry.xsh`

### `lane/module-check-trace-evidence` — 1 unique commit(s); overlap/review
Master has since touched: `crates/xsht/tests/cli.rs`
- `74fc3747d95a` — test(xsht): validate real shared module cache trace — `crates/xsht/tests/cli.rs`

### `lane/module-record-instance-publication-followup` — 6 unique commit(s); overlap/review
Master has since touched: `crates/xsht/src/cli/check.rs`, `crates/xsht/src/cli/lint.rs`, `crates/xsht/src/xsht/test.rs`, `src/frontend.rs`, `src/loader.rs`, `src/modules/cli.rs`, `src/modules/cli/gnu.rs`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/sema/check.rs`, `src/sema/check/call.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/effect_bounds.rs`, `src/sema/check/expr.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/infer_return.rs`, `src/sema/check/local_inference.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/resource.rs`, `src/sema/check/scopecheck.rs`, `src/sema/check/types.rs`, `src/sema/constants.rs`, `src/sema/wire_enums.rs`, `tests/sema.rs`, `tests/sema_bundle_view.rs`, `tests/xsh/checker-modules.xsh`, `tests/xsh/context-scopes.xsh`
- `dc4af304d8f8` — Share checked module facts across workspace entries — `crates/xsht/src/cli/check.rs`, `crates/xsht/src/cli/lint.rs`, `src/frontend.rs`, `src/loader.rs`, `src/sema/check.rs`, `src/sema/check/call.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/effect_bounds.rs`, `src/sema/check/expr.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/types.rs`
- `d8ef181afc2b` — Publish checked RunForm types for transfer facts — `src/sema/check/expr.rs`, `src/sema/check/resource.rs`, `tests/xsh/context-scopes.xsh`
- `63d79c2f2bf5` — Classify declaration statement variants — `src/sema/check/decl.rs`
- `d00ccabf93ec` — WIP: complete session-owned module records — `crates/xsht/src/cli/check.rs`, `crates/xsht/src/xsht/test.rs`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/sema/check.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/infer_return.rs`, `src/sema/check/local_inference.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/scopecheck.rs`, `src/sema/constants.rs`, `src/sema/wire_enums.rs`
- `95e0e041ac7f` — Share checked module records across entries — `src/modules/cli.rs`, `src/modules/cli/gnu.rs`, `src/sema/check.rs`, `src/sema/check/decl.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/resource.rs`, `src/sema/constants.rs`, `tests/sema.rs`, `tests/sema_bundle_view.rs`, `tests/xsh/checker-modules.xsh`
- `39763a661547` — Keep parity helper out of permanent tests — `tests/sema.rs`

### `lane/module-record-owner-port` — 6 unique commit(s); overlap/review
Master has since touched: `crates/xsht/src/cli/check.rs`, `crates/xsht/src/cli/lint.rs`, `crates/xsht/src/xsht/test.rs`, `src/frontend.rs`, `src/loader.rs`, `src/modules/cli.rs`, `src/modules/cli/gnu.rs`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/sema/check.rs`, `src/sema/check/call.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/effect_bounds.rs`, `src/sema/check/expr.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/infer_return.rs`, `src/sema/check/local_inference.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/resource.rs`, `src/sema/check/scopecheck.rs`, `src/sema/check/types.rs`, `src/sema/constants.rs`, `src/sema/wire_enums.rs`, `tests/sema.rs`, `tests/sema_bundle_view.rs`, `tests/xsh/checker-modules.xsh`, `tests/xsh/context-scopes.xsh`
- `dc4af304d8f8` — Share checked module facts across workspace entries — `crates/xsht/src/cli/check.rs`, `crates/xsht/src/cli/lint.rs`, `src/frontend.rs`, `src/loader.rs`, `src/sema/check.rs`, `src/sema/check/call.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/effect_bounds.rs`, `src/sema/check/expr.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/types.rs`
- `d8ef181afc2b` — Publish checked RunForm types for transfer facts — `src/sema/check/expr.rs`, `src/sema/check/resource.rs`, `tests/xsh/context-scopes.xsh`
- `63d79c2f2bf5` — Classify declaration statement variants — `src/sema/check/decl.rs`
- `d00ccabf93ec` — WIP: complete session-owned module records — `crates/xsht/src/cli/check.rs`, `crates/xsht/src/xsht/test.rs`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/names.rs`, `src/runtime/eval/lower/pipeline.rs`, `src/sema/check.rs`, `src/sema/check/command.rs`, `src/sema/check/decl.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/infer_return.rs`, `src/sema/check/local_inference.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/scopecheck.rs`, `src/sema/constants.rs`, `src/sema/wire_enums.rs`
- `95e0e041ac7f` — Share checked module records across entries — `src/modules/cli.rs`, `src/modules/cli/gnu.rs`, `src/sema/check.rs`, `src/sema/check/decl.rs`, `src/sema/check/infer_effects.rs`, `src/sema/check/module_driver.rs`, `src/sema/check/module_record.rs`, `src/sema/check/resource.rs`, `src/sema/constants.rs`, `tests/sema.rs`, `tests/sema_bundle_view.rs`, `tests/xsh/checker-modules.xsh`
- `39763a661547` — Keep parity helper out of permanent tests — `tests/sema.rs`

### `lane/owned-execution-frames` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/calls.rs`, `src/runtime/eval/lowered_run/indexed_run/driver.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/operands.rs`, `src/runtime/eval/lowered_run/indexed_run/patterns.rs`, `src/runtime/eval/lowered_run/indexed_run/producer.rs`, `tests/xsh/stdlib/module.xsh`
- `6ec2f1fb6eec` — Own indexed frames across program calls — `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/calls.rs`, `src/runtime/eval/lowered_run/indexed_run/driver.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/operands.rs`, `src/runtime/eval/lowered_run/indexed_run/patterns.rs`, `src/runtime/eval/lowered_run/indexed_run/producer.rs`, `tests/xsh/stdlib/module.xsh`

### `lane/owned-execution-root-integration` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/calls.rs`, `src/runtime/eval/lowered_run/indexed_run/driver.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/operands.rs`, `src/runtime/eval/lowered_run/indexed_run/patterns.rs`, `src/runtime/eval/lowered_run/indexed_run/producer.rs`, `tests/xsh/stdlib/module.xsh`
- `6ec2f1fb6eec` — Own indexed frames across program calls — `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/calls.rs`, `src/runtime/eval/lowered_run/indexed_run/driver.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/operands.rs`, `src/runtime/eval/lowered_run/indexed_run/patterns.rs`, `src/runtime/eval/lowered_run/indexed_run/producer.rs`, `tests/xsh/stdlib/module.xsh`

### `lane/pattern-view` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`
- `9d7b1938a391` — Add borrowed pattern schema views — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`

### `lane/port-busybox-gunzip` — 1 unique commit(s); overlap/review
Master has since touched: `core/tests/data/busybox/gunzip/foo.gz`
- `9520825f00de` — test: match default gzip input header — `core/tests/data/busybox/gunzip/foo.gz`

### `lane/port-gnu-pr` — 1 unique commit(s); overlap/review
Master has since touched: `core/pr.xsh`, `core/tests/test-pr.xsh`
- `357106523545` — Match GNU pr page range diagnostics — `core/pr.xsh`, `core/tests/test-pr.xsh`

### `lane/port-gnu-split` — 3 unique commit(s); overlap/review
Master has since touched: `core/tests/test-gnu-split.xsh`
- `1af9f02c651e` — Port frozen GNU split chunk, suffix, and input guard cases — `core/tests/test-gnu-split.xsh`
- `0d18e2657d71` — Match GNU split resource and overflow proofs — `core/tests/test-gnu-split.xsh`
- `1277ac966a16` — Require the split memory probe to launch — `core/tests/test-gnu-split.xsh`

### `lane/process-command-frames` — 3 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/process.rs`, `src/runtime/eval/lowered_run/module_archive.rs`, `tests/xsh/fs-path-frame-oracles.xsh`, `tests/xsh/stdlib/process.xsh`
- `1e74dafbc835` — Route filesystem host rows through explicit frames — `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `tests/xsh/fs-path-frame-oracles.xsh`
- `1a1749a067de` — Borrow operands in filesystem frame apply — `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/module_archive.rs`, `tests/xsh/fs-path-frame-oracles.xsh`
- `70a16f66120c` — Retire recursive process command evaluation — `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/process.rs`, `tests/xsh/stdlib/process.xsh`

### `lane/reader-batch-accounting` — 3 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/patterns.rs`, `src/runtime/eval/lowered_run/module_archive.rs`, `tests/xsh/fs-path-frame-oracles.xsh`, `tests/xsh/pattern-conditionals.xsh`
- `c2688561401b` — Route branch and host leaf expressions through frames — `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/patterns.rs`
- `de2721401654` — Retain ExprPatternIf branch cursors in frames — `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `tests/xsh/pattern-conditionals.xsh`
- `194ccd850740` — Borrow operands in filesystem frame apply — `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/module_archive.rs`, `tests/xsh/fs-path-frame-oracles.xsh`

### `lane/resume-lint` — 5 unique commit(s); overlap/review
Master has since touched: `core/lib/awk.xsh`, `core/lib/sed.xsh`
- `c349b0f6e860` — refactor(core): rename shadowed sed block binding — `core/lib/sed.xsh`
- `62ec21a1558f` — refactor(core): use pattern conditionals in awk and sed — `core/lib/awk.xsh`, `core/lib/sed.xsh`
- `82f1bf948c64` — refactor(core): use a pattern test in awk — `core/lib/awk.xsh`
- `b9d1833f7221` — refactor(core): combine equivalent AWK match arms — `core/lib/awk.xsh`
- `0536857cdec2` — refactor(core): iterate AWK regex bytes directly — `core/lib/awk.xsh`

### `lane/route-expression-oracles` — 1 unique commit(s); overlap/review
Master has since touched: `tests/xsh/expression-route-oracles.xsh`
- `00a9f03ebcc0` — test(runtime): add expression route oracles — `tests/xsh/expression-route-oracles.xsh`

### `lane/route-matrix-scalar-oracles` — 1 unique commit(s); overlap/review
Master has since touched: `tests/xsh/typed-scalar-route-oracles.xsh`
- `7fbbc7f8265f` — test(evaluator): add typed scalar route oracles — `tests/xsh/typed-scalar-route-oracles.xsh`

### `lane/route-operand-collections` — 1 unique commit(s); overlap/review
Master has since touched: `tests/xsh/operand-order-collections.xsh`
- `0e4443967cf5` — test(eval): add collection operand-order oracles — `tests/xsh/operand-order-collections.xsh`

### `lane/route-operand-control` — 1 unique commit(s); overlap/review
Master has since touched: `tests/xsh/operand-order-control-oracles.xsh`
- `7a6e8114ee86` — test(runtime): cover operand and payload order — `tests/xsh/operand-order-control-oracles.xsh`

### `lane/route-operand-host` — 1 unique commit(s); overlap/review
Master has since touched: `tests/xsh/operand-order-host.xsh`
- `4d6f541ff6ee` — test(xsh): add host operand order oracles — `tests/xsh/operand-order-host.xsh`

### `lane/route-operand-scalars` — 1 unique commit(s); overlap/review
Master has since touched: `tests/xsh/operand-order-scalars.xsh`
- `a4ee1ec6348c` — test(route): prove scalar operand order and laziness — `tests/xsh/operand-order-scalars.xsh`

### `lane/route-operand-statements` — 1 unique commit(s); overlap/review
Master has since touched: `tests/xsh/operand-order-simple-statements.xsh`
- `6ed7aff96174` — test(runtime): add statement operand-order oracles — `tests/xsh/operand-order-simple-statements.xsh`

### `lane/route-statement-oracles` — 1 unique commit(s); overlap/review
Master has since touched: `tests/xsh/statement-route-oracles.xsh`
- `db9febed5cf0` — test(eval): add statement route oracles — `tests/xsh/statement-route-oracles.xsh`

### `lane/route-trace-current` — 1 unique commit(s); overlap/review
Master has since touched: `crates/xsht/tests/cli.rs`
- `b65c13e85426` — test(xsht): validate real shared module cache trace — `crates/xsht/tests/cli.rs`

### `lane/route-witness-collector` — 2 unique commit(s); overlap/review
Master has since touched: `crates/xsht/Cargo.toml`, `crates/xsht/src/xsht/test.rs`, `crates/xsht/tests/cli.rs`, `src/execution.rs`, `src/runner.rs`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/modules/host.rs`, `src/runtime/eval/route_witness.rs`
- `427d1801d647` — feat(runtime): add private route witness capture — `src/runtime/eval.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/route_witness.rs`
- `ce1eb8e18e00` — feat(test): collect native child route witnesses — `crates/xsht/Cargo.toml`, `crates/xsht/src/xsht/test.rs`, `crates/xsht/tests/cli.rs`, `src/execution.rs`, `src/runner.rs`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run.rs`, `src/runtime/eval/modules/host.rs`, `src/runtime/eval/route_witness.rs`

### `lane/route-witness-runtime` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/route_witness.rs`
- `70b627f3e15d` — feat(runtime): add private route witness capture — `src/runtime/eval.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/route_witness.rs`

### `lane/special-call-order-oracles` — 2 unique commit(s); overlap/review
Master has since touched: `tests/xsh/fs-path-frame-oracles.xsh`, `tests/xsh/stdlib/archive.xsh`, `tests/xsh/stdlib/hash.xsh`, `tests/xsh/stdlib/process.xsh`
- `8e11ef0e65a7` — test(xsh): cover specialized named-call order — `tests/xsh/fs-path-frame-oracles.xsh`, `tests/xsh/stdlib/archive.xsh`, `tests/xsh/stdlib/process.xsh`
- `13bba79e01d1` — test(hash): cover verify-file operand order — `tests/xsh/stdlib/hash.xsh`

### `lane/special-order-integration` — 2 unique commit(s); overlap/review
Master has since touched: `tests/xsh/fs-path-frame-oracles.xsh`, `tests/xsh/stdlib/archive.xsh`, `tests/xsh/stdlib/hash.xsh`, `tests/xsh/stdlib/process.xsh`
- `11fe6758fb8e` — test(xsh): cover specialized named-call order — `tests/xsh/fs-path-frame-oracles.xsh`, `tests/xsh/stdlib/archive.xsh`, `tests/xsh/stdlib/process.xsh`
- `93fd77f50aa3` — test(hash): cover verify-file operand order — `tests/xsh/stdlib/hash.xsh`

### `lane/stage-fast-path-closure` — 2 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/lowered_run/indexed_run/pipeline.rs`, `src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs`, `tests/xsh/stdlib/stage-engine-oracles.xsh`
- `5e39409c6507` — Restore indexed stage field fast paths — `src/runtime/eval/lowered_run/indexed_run/pipeline.rs`, `src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs`, `tests/xsh/stdlib/stage-engine-oracles.xsh`
- `fe1cfd22fc77` — Preserve field spans in stage fast paths — `src/runtime/eval/lowered_run/indexed_run/pipeline.rs`, `src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs`, `tests/xsh/stdlib/stage-engine-oracles.xsh`

### `lane/statement-block-cursors` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/producer.rs`
- `5e6b65b05c7e` — Use statement block cursors in frame machine — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/producer.rs`

### `lane/statement-frame-cohort` — 2 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/context.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `tests/xsh/statement-frame-oracles.xsh`
- `b3b12fba133b` — Route print cd and env statements through frames — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/context.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `tests/xsh/statement-frame-oracles.xsh`
- `0a10e7bf22d9` — Add typed list cursor resumption — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`

### `lane/stmt-scan-frames` — 1 unique commit(s); overlap/review
Master has since touched: `showcase/tests/test-tokei.xsh`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `tests/xsh/lowered-scanners.xsh`
- `896b1c5907ab` — Run scanner statements on borrowed frame views — `showcase/tests/test-tokei.xsh`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `tests/xsh/lowered-scanners.xsh`

### `lane/typed-callee-owned-master-port` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full.rs`, `tests/xsh/typed-callables.xsh`
- `ba9cab12d19b` — test(runtime): checkpoint direct with success regression — `src/runtime/eval/indexed/full.rs`, `tests/xsh/typed-callables.xsh`

### `lane/typed-callee-root-integration` — 7 unique commit(s); overlap/review
Master has since touched: `docs/ARCHITECTURE.md`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/indexed/semantic.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/calls.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/driver.rs`, `tests/xsh/typed-callables.xsh`
- `5a2734ca4384` — Verify typed call callee types structurally — `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/semantic.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/arguments.rs`, `src/runtime/eval/lower/calls.rs`, `tests/xsh/typed-callables.xsh`
- `dbbf6286c4f6` — Verify typed-call metadata against runtime representation — `src/runtime/eval/indexed/full.rs`
- `a3f4606a7491` — wip: retain typed callable proof regressions — `src/runtime/eval/indexed/full.rs`, `tests/xsh/typed-callables.xsh`
- `4ee6ebb91ba8` — WIP preserve typed callable verifier before driver row integration — `docs/ARCHITECTURE.md`, `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/semantic.rs`, `src/runtime/eval/lower.rs`
- `c4a38400c3bf` — schema: add generated driver payload row codecs — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/driver.rs`
- `dbeaf75738c9` — Unify borrowed driver element codecs — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/lowered_run/indexed_run/driver.rs`
- `dbd030c572af` — fix: collect borrowed module exports — `src/runtime/eval/indexed/full.rs`

### `lane/w1-aggregate-cursors` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/operands.rs`
- `0c58051e5fe4` — refactor: consume aggregate operands through borrowed cursors — `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/operands.rs`

### `lane/w1-borrowed-shape-readers` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/lowered_run/indexed_run/pipeline.rs`, `src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs`
- `09306f4c67c3` — Borrow indexed pipeline shape readers — `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/decode.rs`, `src/runtime/eval/lowered_run/indexed_run/pipeline.rs`, `src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs`

### `lane/w1-expression-sequence-cursors` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/operands.rs`
- `5bf5895cb989` — Use borrowed cursors for expression sequences — `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`, `src/runtime/eval/lowered_run/indexed_run/operands.rs`

### `lane/w1-pattern-runtime-reader` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/patterns.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/patterns.rs`, `tests/xsh/decoder-pattern-coverage.xsh`, `tests/xsh/list-pattern.xsh`, `tests/xsh/pattern-aliases.xsh`
- `4b52fcd04470` — Use borrowed views for indexed pattern matching — `src/runtime/eval.rs`, `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/lower.rs`, `src/runtime/eval/lower/patterns.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/patterns.rs`, `tests/xsh/decoder-pattern-coverage.xsh`, `tests/xsh/list-pattern.xsh`, `tests/xsh/pattern-aliases.xsh`

### `lane/w1-statement-branches` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`
- `9037fbc5fa97` — Borrow statement branch lists in indexed runs — `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs`

### `lane/w1-typed-atom-readers` — 1 unique commit(s); overlap/review
Master has since touched: `src/runtime/eval/lowered_run/indexed_run.rs`
- `46736a99e3b8` — Use typed views for eager atom readers — `src/runtime/eval/lowered_run/indexed_run.rs`

### `wip/bb-misc` — 1 unique commit(s); overlap/review
Master has since touched: `core/rev.xsh`, `core/tests/test-tr.xsh`, `core/tests/test-xargs.xsh`, `core/tr.xsh`, `core/xargs.xsh`
- `ec6429a03048` — wip: bb-misc (stopped at wind-down, ungated) — `core/rev.xsh`, `core/tests/test-rev.xsh`, `core/tests/test-tr.xsh`, `core/tests/test-which.xsh`, `core/tests/test-xargs.xsh`, `core/tr.xsh`, `core/which.xsh`, `core/xargs.xsh`

### `wip/lang-diag` — 1 unique commit(s); overlap/review
Master has since touched: `src/sema/check/expr.rs`, `src/sema/check/stmt.rs`, `src/syntax/parser.rs`, `src/syntax/parser/pattern.rs`
- `80f466f7d94d` — wip: lang-diag (stopped at wind-down, ungated) — `src/sema/check/expr.rs`, `src/sema/check/stmt.rs`, `src/syntax/parser.rs`, `src/syntax/parser/pattern.rs`

### `wording/expr` — 1 unique commit(s); overlap/review
Master has since touched: `core/expr.xsh`, `core/tests/test-expr.xsh`
- `f24673d9eeb6` — expr: report stdout write failures with the host error text — `core/expr.xsh`, `core/tests/test-expr.xsh`

### `wording/nl` — 1 unique commit(s); overlap/review
Master has since touched: `core/nl.xsh`, `core/tests/test-nl.xsh`
- `95f15691858b` — nl: name the rejected text in invalid numeric and format option errors — `core/nl.xsh`, `core/tests/test-nl.xsh`

### `wording/tr` — 1 unique commit(s); overlap/review
Master has since touched: `core/tests/test-tr.xsh`, `core/tr.xsh`
- `40222cf95b3b` — tr: warn on invalid UTF-8 operands and use uutils class-alignment wording — `core/tests/test-tr.xsh`, `core/tr.xsh`

### `wording/uniq` — 1 unique commit(s); overlap/review
Master has since touched: `core/tests/test-uniq.xsh`, `core/uniq.xsh`
- `9f058b6773bf` — uniq: match uutils adapter diagnostics for option-value and conflict errors — `core/tests/test-uniq.xsh`, `core/uniq.xsh`

