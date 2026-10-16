# Inference iteration review

Reviewed lane head `6a3b8f462fb7ffd20d3d1729a1c42cc1db22fa0b` and the retained build and test receipts. This review is read-only. It makes no timing claim.

## Current build selection

`build-typing-rank1-inference-6a3.sh` pins the clean lane head and tree, then runs these commands in the pinned Linux musl image:

```sh
cargo test --locked --offline --target "$TARGET" -p xsh --lib --no-run --message-format=json
cargo check --locked --offline --target "$TARGET" -p xsht --all-targets --message-format=json
```

The `6a3` receipts show both commands exit 0. The before and after source snapshots match. The Cargo JSON reports one rebuilt `xsh --lib` test harness, one rebuilt `xsh` library, and six rebuilt `xsht` target variants. The build-script artifacts are fresh. The harness lists 775 unit tests and no benchmarks. The XSHT check compiles test targets but runs no XSHT tests.

The `25be` and `6a3` builds share `.work/target-typing-regressions`. Cargo reports the same harness path, and the harness digest changes between commits. Each build and runtime receipt records the full source commit, tree, clean status, and harness digest before and after use. These checks bind each result to its own source revision. Existing log durations are not performance evidence because the runs were not bracketed by the readiness gate and the host has overlapping campaign work.

## Focused private tests

The runtime driver runs six exact `sema::constraints` tests, three exact type and scheme tests, then the whole `sema::constraints::tests` module. The module selects 41 tests. The six exact constraint tests repeat within that module, so this runs 50 cases across 44 unique test names.

At `6a3`, `constraints-module` exits 101 with 40 passed and one failed. `union_lowers_levels_of_inherited_open_record_descendants` expects level 0 but resolves level 1. The exact run for that test also exits 101. The set and module recovery tests also exit 101. The scheme rejection test passes. The other five exact constraint tests pass.

These results are the intentional pre-fix red checkpoint at commit `6a3b8f462fb7ffd20d3d1729a1c42cc1db22fa0b`. Comparing that snapshot with a later working tree would create a false source and behavior discrepancy. The integrator reports that fix `121d39242b9e7066a5a4dde8dbb15fb1336d022a` changes the shared `Type` recovery traversal and union level handling, and passes all 41 constraint tests and all nine exact tests. The reviewer returned GO. This review did not inspect the later receipt.

## Changed-path gate

`src/sema/*` maps to the `Checker` gate. `cargo dev gate src/sema/types.rs` selects `cargo test --profile verification --target "$TARGET" -p xsh --test integration sema::` and the native `tests/xsh/checker-` filter. The current source has 52 Rust tests in `tests/sema.rs` and 104 native tests across eight `checker-*.xsh` files.

That gate does not select the private `xsh --lib` tests in `src/sema`. The owner’s separate harness run is needed for those tests. The changed-path gate remains the broader checker and native behavior check.

## Narrow iteration queue

For each `src/sema` edit, build one `xsh --lib` harness and run the exact tests for the changed behavior. Run the full constraint module when solver behavior changes. Do not repeat the XSHT all-target check for every private-test edit. The migration compiled at `25be`, and the `25be..6a3` diff changes no XSHT source. Repeat the XSHT check when an API or consumer changes, then keep the changed-path gate and final campaign acceptance.

The single `xsh --lib` harness build is unavoidable with the current layout. These tests live in private `#[cfg(test)]` modules in the root library. `--exact` narrows execution but Cargo still compiles the complete 775-test harness. Avoid a new crate or production test seam for this lane.

## Revision-safe artifact use

The build driver reuses one target directory across revisions. That keeps Cargo's compiled dependencies available. The shared harness path changes contents between commits, so each run must keep the full source commit, tree, clean status, and matching harness digest in its before and after snapshots. The `25be` and `6a3` receipts record those values for their own runs.

Keep the shared target directory. When an old checkpoint must remain rerunnable after another build replaces the shared path, copy only its harness to a revision-specific scratch path and retain its digest. Do not copy the full target tree. Bind each runtime receipt to the source commit, tree, target, profile, features, Rust toolchain, flags, and harness digest.

## Timing status

No iteration-latency comparison is qualified. Keep timing out of the acceptance report until a run passes both `perf-gate-ready` checks without overlapping host load.
