# Compact Lowerability Gap Guide

This guide is for closing `compact.unlowered-*` and `runtime.unlowered-*`
failures without weakening compact lowerability. The goal is strict detection:
`xsht check` should report every construct that cannot run in the compact
runtime, and `xsh` should not silently accept a source shape by falling back to
an unsupported path.

## North Star

`xsht check` should pass, with no output and exit status 0, for every `.xsh`
script in:

- this repository: `./target/debug/xsht check .`
- Laputa scripts: `./target/debug/xsht check ../laputa`
- package scripts: `./target/debug/xsht check ../packages`

That means parse errors, checker errors, unsupported compact-lowering
diagnostics, and runtime-lowering gaps all count against the same goal. Do not
call the lowerability work complete while any of those broad gates fail.

## Operating Rule

Treat every unlowered diagnostic as one of these cases:

1. The compact runtime can already express the behavior, but the lowerer is
   missing type information or a construction case.
2. The compact runtime needs a new `LoweredExpr`, `LoweredStmt`, type, or
   runtime operation path.
3. The source genuinely should remain unsupported, and the diagnostic should say
   what construct blocked lowering.

Do not make `Any`, `Unknown`, records, modules, or dynamic calls permissive just
to pass a corpus check. If the receiver or callee is dynamic, keep it rejected
unless there is a concrete checked type that proves the lowered method is valid.

## Fast Loop

Start with the smallest failing command:

```sh
cargo build --bin xsh --bin xsht
./target/debug/xsht check --summary ../laputa
cargo test --test runtime SOME_TEST -- --nocapture
```

For local corpus work, prefer one script or one test first:

```sh
./target/debug/xsh examples/archive.xsh
./target/debug/xsht check --summary tools/xsh-ir-coverage.xsh
cargo test --test runtime coverage::ir_coverage_scans_multiline_top_level_regions_once -- --nocapture
```

Use `--summary` on broad directory checks. It preserves the normal diagnostics
and appends counts by diagnostic code plus the first observed location:

```sh
for root in . ../laputa ../packages; do
  ./target/debug/xsht check --summary "$root" || true
done
```

The summary is a triage aid only. The normal diagnostics above it remain the
source of truth for spans, messages, and reduction work.

When the public diagnostic only points at a top-level statement, reduce the
script by deleting surrounding statements until one expression remains. If that
is still unclear, add a temporary env-gated trace near the `lower_expr` blocker
path in `src/runtime/eval/lower.rs`, run with `XSH_DEBUG_LOWER=1`, then remove
the trace before finishing.

## Definition Of Done

A compact-lowerability tranche is done only when all of these are true:

- [ ] Every targeted command in the tranche either passes or has fewer
  unsupported-lowering diagnostics than the tranche baseline.
- [ ] Every remaining unsupported construct has a specific diagnostic that names
  the blocking construct, receiver type, module op, method, or function body
  dependency. Do not leave a broad `compact.unlowered-statement` diagnostic
  when the lowerer knows a more precise blocker.
- [ ] New lowering support is strict: `Type::Any`, `Type::Unknown`, dynamic
  calls, and unchecked record/module shapes remain rejected unless a concrete
  checked type proves the operation is valid.
- [ ] The closest runtime, sema, or `xsht` tests cover the newly supported or
  newly diagnosed construct.
- [ ] `Current Useful Corpus Targets` below reflects the new state, including
  removing fixed targets and adding the next smallest blockers found by the
  broad corpus check.

## Tranche Roadmap

Current baseline captured with the debug `xsht`:

```sh
./target/debug/xsht check --summary .
./target/debug/xsht check --summary tools/xsh-ir-coverage.xsh
./target/debug/xsht check --summary ../laputa
./target/debug/xsht check --summary ../packages
cargo test --test runtime coverage::ir_coverage_scans_multiline_top_level_regions_once -- --nocapture
```

Current broad failure shape:

- `./target/debug/xsht check --summary .`: `compact.unlowered-main: 25` and
  `compact.unlowered-statement: 2`.
- `./target/debug/xsht check --summary ../laputa`: `compact.unlowered-main: 5`
  diagnostics blocking `main`.
- `./target/debug/xsht check --summary ../packages`:
  `compact.unlowered-main: 11` and `compact.unlowered-statement: 64`.
  The previous `../packages/pm.xsh:1124` parse typo is no longer present in the
  current corpus; `../packages/pm.xsh` now reaches
  `compact.unlowered-main: 1` at `world_plan_repo`.
- `coverage::ir_coverage_scans_multiline_top_level_regions_once`: fails because
  `tools/xsh-ir-coverage.xsh:1800` is not lowerable.

Complete tranches in order. Within a tranche, each checkbox should be small
enough for one agent turn.

### Tranche 0: Baseline And Check Harness

- [x] Add or update a cheap way to summarize `xsht check` failures by
  diagnostic code and first blocking construct across `.`, `../laputa`, and
  `../packages`. Gate: `xsht check --summary PATH` produces counts after the
  original diagnostics.
- [x] Keep parse/check/lowerability failures distinct in the summary. Gate:
  `check_summary_groups_directory_failures_by_code` covers parse and compact
  lowerability diagnostics in one directory check and verifies separate counts.
- [x] Add regression coverage for directory `xsht check` failures returning a
  nonzero exit status when any child file has parse or lowerability diagnostics.
  Gate: `check_directory_lowerability_failure_exits_nonzero` covers a directory
  whose only failing child is a compact-lowerability failure.
- [x] Re-run the north-star gates and paste the new counts into this section
  before starting Tranche 1.

### Tranche 1: Parse Errors And Top-Level Declarations

- [x] Confirm the `../packages/pm.xsh:1124` parse error is gone from the package
  corpus. Gate: `./target/debug/xsht check --summary ../packages/pm.xsh` now
  reports `compact.unlowered-main: 1`, not `parse.expected-expression`.
- [ ] Make top-level `use` statements behave like declaration/import markers for
  lowerability checking. Gate:
  `./target/debug/xsht check --summary ../packages/pm/local.xsh` no longer fails at
  `use remote` solely because the `use` statement itself is unlowered.
- [ ] Cover aliased imports such as `use pm.make as make`,
  `use pm.util as pm_util`, and relative package imports such as
  `use PKGBUILD-shared as PKGBUILD_shared`. Gate: targeted tests cover all
  three source shapes.
- [ ] Re-run `./target/debug/xsht check --summary ../packages` and update the baseline.
  The expected progress is that package diagnostics move past top-level `use`
  lines to real body, call, method, module, or record blockers.

### Tranche 2: Precise Blocker Diagnostics

- [ ] Add or improve blocker reporting so the
  `tools/xsh-ir-coverage.xsh:1800` failure names the first unsupported
  construct inside the `CoverageReport` literal. Gate:
  `./target/debug/xsht check --summary tools/xsh-ir-coverage.xsh` must no longer report
  only a generic `compact.unlowered-statement` for that line.
- [ ] Reduce the `CoverageReport` failure to the exact unsupported expression or
  method, then classify it under the three `Operating Rule` cases. Add a small
  fixture or runtime test that reproduces that construct without scanning the
  full corpus. Gate: the new targeted test fails before the lowering or
  diagnostic change and passes after it.
- [ ] If the `CoverageReport` blocker is a legitimate missing compact lowering
  case, implement the smallest strict lowerer/runtime support for it. Gate:
  `cargo test --test runtime coverage::ir_coverage_scans_multiline_top_level_regions_once -- --nocapture`
  passes, or the failure advances to a later, more specific blocker.
- [ ] If the `CoverageReport` blocker should remain unsupported, keep it
  rejected but replace the broad diagnostic with a precise one. Gate:
  `cargo test --test runtime coverage::ir_coverage_scans_multiline_top_level_regions_once -- --nocapture`
  either passes because the script no longer needs that construct or fails with
  the precise unsupported construct.
- [ ] Replace direct `unsupported statement in body` reports with the first
  nested blocker when the construct probe has a more specific sample span. Gate:
  at least one failing `core/` script and one failing `../laputa` script name
  the nested expression, method, call, module op, or statement kind.
- [ ] Add an `xsht check` diagnostic snapshot or targeted test for a nested
  blocker inside a function dependency of `main`. Gate: the test asserts both
  the dependency function name and the nested blocker detail.

### Tranche 3: Shared Core And Showcase Body Gaps

- [ ] Reduce and fix the shared `common_int` blocker used by
  `core/head.xsh`, `core/seq.xsh`, `core/shuf.xsh`, `core/split.xsh`,
  `core/strings.xsh`, `core/tail.xsh`, and `core/tar.xsh`. Gate: those scripts
  either pass `xsht check` or advance to later blockers.
- [ ] Reduce and fix direct unsupported `main` bodies in small `core/` and
  `showcase/` scripts such as `core/basename.xsh`, `core/sort.xsh`,
  `showcase/archive-unpack.xsh`, `showcase/batch-rename.xsh`,
  `showcase/bytes-inspect.xsh`, `showcase/json-diff.xsh`,
  `showcase/loc.xsh`, and `showcase/secret-scan.xsh`. Gate: each fixed script
  has a targeted runtime or `xsht check` regression case when the lowering gap
  is not already covered.
- [ ] Reduce and fix larger showcase helper blockers:
  `showcase/hyperfine.xsh::print_summary`,
  `showcase/perf-collapse.xsh::clean_symbol`, and
  `showcase/run-retry.xsh::run_attempt`. Gate: each script either passes or
  reports a later, more specific blocker.
- [ ] Keep `showcase/tokei.xsh` in the tranche even if it needs multiple
  passes. Gate: `./target/debug/xsht check --summary showcase/tokei.xsh` eventually
  passes and no public `Value` behavior changes to make that happen.

### Tranche 4: Laputa Main Dependencies

- [ ] For `../laputa/boot.xsh`, reduce `dotenv_lookup` to its first unsupported
  statement and add a targeted regression test. Gate:
  `./target/debug/xsht check --summary ../laputa/boot.xsh` either passes or reports a
  later blocker than `dotenv_lookup`.
- [ ] For `../laputa/linux-iteration.xsh`, reduce `cache_report` to its first
  unsupported statement and add a targeted regression test. Gate:
  `./target/debug/xsht check --summary ../laputa/linux-iteration.xsh` either passes or
  reports a later blocker than `cache_report`.
- [ ] For `../laputa/proof-dwl-foot-minimal.xsh` and
  `../laputa/proof-waterfox.xsh`, reduce `verify_chroot_command` once and share
  the same lowering or diagnostic fix for both scripts. Gate:
  `./target/debug/xsht check --summary ../laputa/proof-dwl-foot-minimal.xsh` and
  `./target/debug/xsht check --summary ../laputa/proof-waterfox.xsh` either pass or report
  later blockers.
- [ ] For `../laputa/update-xsh.xsh`, reduce `update_pkgbuild` to its first
  unsupported statement and add a targeted regression test. Gate:
  `./target/debug/xsht check --summary ../laputa/update-xsh.xsh` either passes or reports
  a later blocker than `update_pkgbuild`.
- [ ] Run `./target/debug/xsht check --summary ../laputa`. Gate: it exits 0
  with no diagnostics, or the only remaining failures are later blockers
  discovered after the five named dependencies above.

### Tranche 5: Package Manager And PKGBUILD Modules

- [ ] After Tranche 1 moves past top-level `use`, reduce the first package
  manager module blocker in `../packages/pm/*.xsh`. Gate:
  `./target/debug/xsht check --summary ../packages/pm` either passes or reports fewer,
  more specific lowerability diagnostics.
- [ ] Fix strict lowering for package scripts that depend on package-manager
  module return records, lists, checksums, source arrays, and metadata. Gate:
  one reduced fixture covers each added record or method path.
- [ ] Reduce representative `PKGBUILD.xsh` failures for `pm.make`,
  `pm.util`, Linux `kbuild`, and aliased shared package modules. Gate: at least
  one script in each import family moves past module/import setup to body
  blockers or passes.
- [ ] Keep module shape strict. Gate: tests assert dynamic module calls or
  unknown imported members are still rejected with precise diagnostics.

### Tranche 6: Package Proof Scripts

- [ ] Reduce and fix proof scripts with direct unsupported `main` bodies, such
  as `bison/proof-stack.xsh`, `build-essential-native/proof-image.xsh`,
  `cmake/proof.xsh`, `dropbear/proof.xsh`, `flex/proof.xsh`,
  `m4/proof.xsh`, `muon/proof.xsh`, `musl/proof.xsh`, and
  `tmux/proof.xsh`. Gate: each script either passes or reports a later,
  specific blocker.
- [ ] Reduce and fix proof helper blockers such as
  `ca-certificates/proof.xsh::verify_package_metadata`. Gate: helper
  dependency diagnostics name the nested blocker and the script advances or
  passes.
- [ ] Re-run `./target/debug/xsht check --summary ../packages`. Gate: it exits
  0 with no diagnostics, or the remaining failures are listed as the next
  tranche with exact files, function names, and blockers.

### Tranche 7: Final Coverage Sweep

- [ ] Run `./target/debug/xsht check .`,
  `./target/debug/xsht check ../laputa`, and
  `./target/debug/xsht check ../packages`. Gate: all three exit 0 with no
  output.
- [ ] Run `cargo test --test runtime` and the closest `xsht` CLI tests. Gate:
  runtime tests pass, except for unrelated pre-existing ignored tests.
- [ ] Run `tools/xsh-ir-coverage.xsh` through the relevant runtime test and
  direct `xsht check`. Gate:
  `coverage::ir_coverage_scans_multiline_top_level_regions_once` passes and
  `./target/debug/xsht check --summary tools/xsh-ir-coverage.xsh` exits 0.
- [ ] Update this file by checking off completed tranches, deleting stale
  per-file blockers, and recording any remaining gap as a new tranche with a
  concrete gate.

Do not mark a checkbox complete just because the diagnostic changed shape. Mark
it complete when the named gate demonstrates progress against that exact target.

## Where To Look

- `crates/xsht/src/app.rs`: `xsht check` option parsing and help text.
- `crates/xsht/src/cli/check.rs`: file/directory check loop, parse/check
  diagnostic rendering, compact lowerability invocation, `--summary` counts,
  duplicate diagnostic filtering, and annotation writes.
- `crates/xsht/tests/cli.rs`: CLI regression coverage for directory checks,
  summary output, compact lowerability diagnostics, and config-sensitive
  discovery.
- `src/runtime/eval/lower.rs`: AST to compact lowered IR construction, type
  inference used by lowering, blocker counters, top-level lowerability.
- `src/runtime/eval/lowered_run.rs`: execution of lowered expressions,
  statements, methods, and module runtime ops.
- `src/runtime/eval.rs`: compact install, `xsht check` lowerability diagnostics,
  runtime execution of installed compact plans.
- `src/sema/check/compact.rs`: compact probe checker used before lowering.
- `crates/xsh-registry/src/signature/*`: standard API signatures and runtime
  operation ids.
- `crates/xsh-registry/src/records.rs`: standard record schemas that should be
  preserved when a lowered API returns structured data.

## Common Failure Classes

**Concrete type lost as `Any` or `Unknown`**

Symptoms: a valid method such as `.len()`, `.trim()`, `.keys()`, or `.cancel()`
is rejected because the local slot type is dynamic.

Fix the compact lowerer inference, not the method gate. Check:

- local binding type selection;
- `Try(...)` ok-type inference;
- top-level slot metadata;
- module/method return schemas;
- env pseudo-fields such as `env.Str.NAME?` and `env.Path.NAME?`;
- standard records returned by APIs such as `metadata`, `fs.files`, archive
  list APIs, and byte-copy helpers.

**Known method missing receiver-specific return type**

Symptoms: a chain lowers until a method result is used as the next receiver, for
example `path.read_text()?.trim()` or `bytes.slice(...).dump(...)`.

Add checked return inference for the method on the concrete receiver type. Keep
receiver support strict: `Type::Any` and `Type::Unknown` should not pass method
support checks.

**Known module op missing compact construction**

Symptoms: the checker accepts the API and `RuntimeOp` exists, but expression
lowering records a call blocker.

Check `lowered_module_op_supported`, `lowered_module_call_args`, special
construction in `lower_call`, and execution in `lowered_run.rs`. Add the
smallest exact lowered form. Reuse registry signatures and existing runtime
helpers.

**Top-level statement lowers in pieces but is rejected**

Symptoms: subexpressions lower, but `xsht check` reports the whole statement.

The construct probe substitutes placeholders while counting blockers. The real
top-level lowerer rejects any statement that produced blocker events. Find the
first blocker event, not the final placeholder.

**Checker-only forms**

`reveal_type(...)` is checker-only. `xsht check` may skip it for lowerability,
but normal `xsh` execution must still reject it. Keep that distinction explicit
in `compact_top_level_stmt_is_skippable`.

## Diagnostics Work

`compact.unlowered-statement` is often too broad. Prefer diagnostics that name
the blocking construct:

- unsupported statement kind;
- unsupported call or method label;
- dynamic receiver type (`Any` or `Unknown`) when strict method lowering blocks;
- unsupported module op;
- unsupported function body dependency for `main`.

The construct probe already records blocker counters and sample spans. Prefer
threading those details into diagnostics over adding ad hoc string matching.

## Methodology Notes

- Start every broad pass with `xsht check --summary PATH`. The summary makes
  code-level regressions visible while preserving the original diagnostics.
- Reduce from the first diagnostic in each code family, not necessarily the
  first file alphabetically. A single shared helper such as `common_int` can
  cover many scripts.
- Treat directory behavior as part of the contract. A directory check must exit
  nonzero if any child file has parse, check, or lowerability diagnostics.
- Keep parse/check/lowerability counts separate. A parse error blocks later
  compact-lowering evidence for that file, so do not compare lowerability counts
  across baselines until parse errors are cleared.
- Update this guide after each tranche with the new summary counts and first
  blocker locations. Stale counts are worse than no counts because they send the
  next agent to the wrong layer.

## Verification

Use the narrowest command that proves the fix, then widen:

```sh
cargo test --test runtime TARGET -- --nocapture
cargo test -p xsht --test cli TARGET -- --nocapture
cargo test --test sema
cargo test --test runtime
./target/debug/xsht check --summary ../laputa
```

Do not run formatters or autofixers for this work. Do not change tests to avoid
strict lowerability unless the test source itself uses intentionally removed
language behavior. If a test asserts old behavior such as removed error fields,
update the source to the current language contract separately from compact
lowering changes.

## Current Useful Corpus Targets

As of the current baseline, the north-star gates are:

- [ ] `./target/debug/xsht check .`
- [ ] `./target/debug/xsht check ../laputa`
- [ ] `./target/debug/xsht check ../packages`

Narrow gates that are currently useful while closing the first tranches:

- [ ] `./target/debug/xsht check --summary tools/xsh-ir-coverage.xsh`
- [ ] `./target/debug/xsht check --summary ../packages/pm.xsh`
- [ ] `./target/debug/xsht check --summary ../packages/pm/local.xsh`
- [ ] `./target/debug/xsht check --summary ../laputa/boot.xsh`
- [ ] `./target/debug/xsht check --summary ../laputa/linux-iteration.xsh`
- [ ] `./target/debug/xsht check --summary ../laputa/proof-dwl-foot-minimal.xsh`
- [ ] `./target/debug/xsht check --summary ../laputa/proof-waterfox.xsh`
- [ ] `./target/debug/xsht check --summary ../laputa/update-xsh.xsh`
- [ ] `cargo test --test runtime coverage::ir_coverage_scans_multiline_top_level_regions_once -- --nocapture`
- [ ] `cargo test --test runtime collections::fs_files_recurses_with_raw_walk_and_preserves_entry_ext -- --nocapture`
- [ ] `cargo test --test runtime streams::reduce_by_parallel_jobs_matches_serial -- --nocapture`
- [ ] `cargo test --test runtime coverage::showcase_standalone_scripts_are_self_testable -- --nocapture`

Expect this list to change as gaps close. Keep the broad gates until they pass;
replace narrow gates whenever a script advances to a later blocker or a cluster
is fully resolved.
