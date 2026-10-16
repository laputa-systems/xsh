# Canonical bb809e1a failure

The pinned canonical run used source `bb809e1a670b49c66d59f4e4cbca7ca3f6680e16`, tree `f2f8094c8f18f4fc20fa51a53ea87fa311b853f3`.
Its first Rust unit-test stage ended with 747 passes, one failure, and three ignored tests.
The runner exited 3 without a timeout, signal, OOM, or swap event.
The source stayed clean at its pinned identity.

`frame_apply_strategy_covers_the_ordered_value_cohorts` had a stale expected list.
The schema declares `ExprProcessCommandArgv` as `FrameApply`, but the test omitted that row.
Repair `21f8848b65f66d841250d4069800872e03219e59` adds the row between `ExprArchiveTarCreate` and `ExprAbort`.
The [focused repaired test](repaired-cohort.log) passes in the pinned musl debug environment, selecting exactly one test.
An earlier placement after `ExprError` failed the focused check and was corrected before integration.

The [canonical receipt](canonical-replay.json), [stdout](stdout.log), [stderr](stderr.log), and [resource observation](container-resources.json) retain the failed run.
This result gives no canonical acceptance credit.
The repaired freeze requires a fresh full canonical run and source-qualified receipts.
