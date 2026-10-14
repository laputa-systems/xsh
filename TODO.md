# TODO

What is left after the hardening and ergonomics work: items that are designed
and not built, known defects, and ideas that were turned down. `docs/SPEC.md`
is the contract for everything that is built; nothing here describes it.
The current consolidation acceptance snapshot is in
[`dev/consolidation/HANDOFF.md`](dev/consolidation/HANDOFF.md); failure counts
below are historical unless they name a later targeted run.

## Needs a design

- **Affine checking of runtime-owned handles.** Turn "already consumed" from
  a runtime error into a check error where ownership is statically visible:
  after `wait child`, a later `child.cancel()` on a consumed handle, a
  consumed `NetJob`, a closed `FsRoot`. Deliberately smaller than Rust: only
  checker-known runtime-owned types, moves, and consuming operations, starting
  with use after a consuming operation in straight-line code of one scope.
  Open: how a parameter says it borrows a handle or takes it.
- **`lint.prefer-inferred-proc-return`** is opt-in, with about 160 provable
  sites here. Decide whether it becomes a default. A failed
  `.require(Contract)` raises a plain `Error` with facets; a built-in
  `ContractError` family would let code match it by name.
- **Record and JSON exactness.** See "Deferred and rejected".

## Designed, not built

- **`FsRoot` operations take a `RelPath`.** They take `Path`; the opt-in note
  `lint.prefer-rel-path` lists 637 call sites here that pass a computed
  `Path`.
- **Byte sinks that need the Linux container:** `linux.mount` source and
  `linux.modinfo` still take text, and so does `applet.su_session`, whose
  three text parameters need a decision on which is a path.
- **Laputa's `pm/cli.xsh`** parses its subcommands by hand (eleven
  placeholder-and-`match` blocks, 850 lines). `cli main repo check(...)`
  entries replace it; they live only in an entry script, so the entries move
  to `pm.xsh`.
- **`cargo dev test linux --ci`** passes the host target into the container,
  so it works only on a Linux host; use `cargo dev test linux` on macOS. The
  Linux run shares `target/` with the host and overwrites `target/release`.

## Tests still in Rust

About 310 tests stay in Rust because they cross a boundary a native test
cannot: fixture servers in the test process, signals sent from outside at a
chosen moment, PTYs, privileges, non-UTF-8 argv, a copied or renamed binary,
allocation and preparation counters, small-stack runs, grammar proofs,
whole-corpus walks, and lint or checker runs without the facts the command
line always computes. Three things found while porting the rest:

- `xsht check` turns migration diagnostics on, so a program the in-process
  checker accepts can be rejected by the command for an unrelated code; and
  it rejects `proc main(...)` with parameters that are not the argument list
  (`compact.main-missing-spread`) where `xsht lint` accepts it.
- `use text` is `check.unknown-module` in process and `parse.module-read`
  from the command, which reads the module first.
- `examples::example_corpus_lints_without_warnings` strips lint's timing line
  before it checks for diagnostics. The standard XSHT test plan skips that
  corpus-wide pass; run the `examples::` target explicitly to execute it.

## Defects

The earlier 1,690-input, 6,760-outcome W9 parity run is historical evidence,
not final-source acceptance. Tracing on earlier sources counted one
preparation and check per full module-record key in each shared session: XSH
check 1,661 and default lint 1,518; Laputa check 379 and default lint 316.
Default lint also created 288 fresh candidate-probe caches in XSH (905
records) and 77 in Laputa (347 records). The latest [consolidation handoff](dev/consolidation/HANDOFF.md#current-closure-checks-2026-10-14)
records the pinned helper build and shared-import count probe. The frozen-source
1,690-input, four-profile publication run remains open.

### Current consolidation acceptance gates

The structural recount passed all four required zero metrics with no rises on
the `865fa955` source snapshot. Its synthetic feature-touch probe recorded 38
XSH and 18 XSHT exhaustive-match demands, then compiled after isolated
placeholder arms. That probe does not establish runtime reachability or replace
the original witness review. Final-source qualification remains open. The
current-source fixed-seed and XSHT integration gates, full native suite,
repository and documentation checks, and Linux privileged plan also remain
open. The final report must state cumulative campaign fuzz seeds and upstream
compatibility results. `cargo dev gate --batch` owns the fixed-seed and tooling
checks plus one full native suite. See the [consolidation handoff](dev/consolidation/HANDOFF.md)
for the source freeze and other close requirements.

### Lint migration

The broad lint migration still needs current-source acceptance. Its old
per-rule finding counts and merge exclusions predate the second pass and are
not a current inventory. Historical findings and targeted results are
recorded in the consolidation handoff.

An earlier report said `lint.prefer-collect` could offer a rewrite that the
checker rejected, but it preserved no source fixture. The retained
`test_prefer_collect_keeps_a_list_mutable_after_multiline_index_assignment`
initially failed before lint ran: a bare newline inside `kept[...]` produced
`parse.expected-expression`. The fixture now uses the parenthesized multiline
index form covered by
`crates/xsht/src/lint_prefer_collect.rs::multiline_index_assignment_keeps_the_list_mutable`.
The focused `xsht test --exact` run passes with the pinned verification pair,
and the program prints `99,2`. This fixes the malformed test but does not
reproduce or disprove the earlier rewrite report. Current-source lint
acceptance remains open.

Adjacent tests keep indexed mutations out of the rewrite. The Rust tests
`indexed_comparison_after_the_loop_is_not_a_reassignment` and
`indexed_comparison_does_not_hide_a_later_unrelated_assignment` check both
source and rewritten text. The runtime capture regression
`tests/xsh/stream-producers.xsh::test_stream_producer_mutations_refresh_consumer_captures`
covers captured mutations across collection, resumption, delegation, and
cancellation.

The historical findings for needless annotations, directory excludes,
conditional `as` conversions, multiline record-constructor fixes,
redundant-close indentation, and positional callable constructors have merged
fixes. Their evidence is recorded in the consolidation handoff.

The narrowed optional callable-field verifier gap is closed. Positive tests in
`src/runtime/eval/indexed/full.rs` cover calls through narrowed optional
records and guarded optional paths. Negative tests reject forged guards,
writes, loop backedges, and reordered evidence.

### Historical host-load failures and current targeted shards

The recorded full native counts predate the current source. The latest run in
the [consolidation handoff](dev/consolidation/HANDOFF.md) recorded 13,437
passes, 56 failures, and 138 skips on snapshot `59a6dfbc` under host load.
The handoff records later targeted passes and repairs, including the split
system-report checks, GNU cut, factor, and sort shards. Those runs do not
replace a full current-source suite. The older result of 13,377 passed, 116
failed, and 138 skipped is not a current failure total. Run the full suite from
the final source freeze before reporting current failures.

### Smaller open items

- Each lint candidate probe is a full parse, load and check of the import
  graph (introduced by `da40b6b4`); options are one shared check of the
  unchanged graph, grouped candidates, or skipping probes on a clean corpus.
- `third_party/regex-lite` is a vendored copy of regex-lite 0.1.9 whose capture
  name map is a `BTreeMap`, so compiling a regex literal does not need
  `getrandom` (`test_uu_shuf_getrandom_fail`); the owner chose this over
  leaving the test red. Build time and binary size were not measured; drop the
  patch if an upstream release removes the std `HashMap`.

## Laputa

Laputa (`../laputa`) is migrated through every default lint that has a fix,
one commit per rule, and its `xsht check` is clean. Its older lint findings
were never migrated, and 139 of its files have never been formatted.

Its test suite is a gate the lints cannot be: it found that
`write(data, mode:)` lost setuid bits, and that test-written `PKGBUILD.xsh`
and service strings used removed spellings. On an x86_64 Alpine host it is
317 passed and 9 failed. Seven are `packages/linux/tests/main.xsh` tests that
need to write `/var/cache/laputa/linux-kbuild`. Two fail the same way on a
toolchain and a Laputa tree from before this work:
`tests/pm/pm_execute.xsh::test_execute_hashes_each_payload_once_and_reuse_hashes_nothing`
and
`tests/pm/pm_execute.xsh::test_execute_parallel_builds_log_per_package_and_failures_name_their_log`,
whose failure message no longer carries the proof's own error text.

The owner-selected restoration of Laputa's four nominal Asterinas error
families is confirmed at `ca658ce` and has a red-to-green diagnostic-type
regression. Preserve those original error identities; this decision is closed.

A lint migration that rewrites a checksummed local source must update the
`sha256` its `PKGBUILD.xsh` declares (`packages/flex`, `packages/bison`,
`packages/ca-certificates`; the others declare `SKIP`) and regenerate
`tests/pm/fixtures/plans/basic-aarch64.json`, which records proof hashes: run
the round-trip test with `--keep-temp` and copy its
`plan-json-out-*/basic-aarch64.json`. Its lint-excluded fixtures and its
embedded scripts are not reached by any fix and need a script or a hand edit
when a spelling is removed.

## Iteration speed

Gates and builds are the slowest part of a change. On a 32-thread x86_64
Alpine machine the whole gate sequence takes about eight minutes: a minute
for the native suite (about 3,440 tests), three and a half for the `xsht`
integration target, and under a minute each for the release build, the root
integration target, and the unit tests. On a ten-core M1 Pro a release
rebuild alone takes about two minutes.

- On a musl host `rustc` uses musl's allocator and spends most of a thin-LTO
  build in the kernel; `cargo dev` preloads jemalloc into its build steps
  there (`XSH_DEV_BUILD_PRELOAD`). A plain `cargo build` does not: a release
  rebuild of `xsh` takes 163 s without the preload and 33 s with it.
- The `xsht` integration target is dominated by the corpus tests (lint and
  format invariance over this repository and Laputa) and competes with
  itself for the lint budget test.
- A build for running tests need not be the release profile (thin LTO,
  `opt-level = 3`): optimization without LTO, with more codegen units, would
  probably do. `src/runtime/eval/lower.rs` is 16,000 lines and
  `crates/xsht/src/lint.rs` 15,000, and a change to either recompiles its
  crate. The root package's unit tests cannot be built with `--release` at
  all (LLVM recursion).
- About 1,300 native tests run a whole script through a fresh `xsh` process;
  the few `system-report` tests take five to eight seconds each. Running a
  script test in process is undesigned.
- The `xsht` lint wall-budget test has historical failures under concurrent
  integration load and an older standalone `xsht lint` measurement of about
  17 s against its 15 s budget. The scheduler and lint sources have changed
  since those measurements; current standalone status is unmeasured, so keep
  the existing gate authoritative and rerun it on the current release binary.
- Run only the gates a change can affect: map changed paths to the rows of
  the table in `docs/TESTING.md`, as one `cargo dev` command.

## Deferred and rejected

Rejected, with reasons:
- `not` as unary negation: too mechanical, and `!` is readable.
- Infix text predicates (`starts with`, `ends with`, `matches`): too
  mechanical; the new words would not be reusable elsewhere, so the
  `starts_with`/`ends_with`/regex methods stay the API.
- `pkg with field: value` record updates: too esoteric; keep `{...pkg, field: v}`.
- Quantifier phrases (`any x in xs where c`, `every`, `count`): confusing.
- `first x in xs where c else ...`: a stretch; the explicit loop reads well.
- String test names (`test "a sentence" { }`): strings are bad identifiers.
- `for x in xs, N at a time`: too magical, and `par-map` must stay greppable.
- English collection phrases (`sorted by`, `grouped by`): `sort-by` and the
  other stage names must stay greppable.
- A delimiter that makes `{` literal in an interpolating string, for
  generated C, XSH, and config text: `$`-interpolated block strings
  (`${expr}`) overload `$`, which command words already use, and Swift-style
  hash delimiters (`f#"""...#{expr}..."""#`) are too noisy. `{{` and
  `template.render` stay.
- `/` as path join: keep `fp"..."`.
- Path and emptiness phrases (`if out is a directory`, `if p exists`,
  `when xs is empty`): `is` would gain a second meaning beside the type
  test, and a fallible filesystem call would read as a pure predicate. The
  methods `is_dir()`, `exists()`, and `is_empty()` are as short and can be
  searched for.
- Computed negative indexes: an off-by-one would read the last element
  instead of failing. Only a literal negative index counts from the end.
- A propagation rule based on the expected type (a `Result[T]` propagating
  wherever a `T` is expected): it would make every binding and argument a
  hidden exit.

From the hardening pass:

- **Record/JSON schema exactness is deferred.** Do not add `exact { ... }`
  yet. Exactness may belong at the dynamic validation operation instead
  (`json.require(..., exact: true)`, a strict `.require` mode, or a related
  schema API) so one schema can deliberately accept, strip, preserve, or reject
  unknown fields at different boundaries. Review Zod's TypeScript model
  (strict/passthrough/catchall-style choices) and similar schema systems before
  choosing how XSH composes exactness with width-compatible records, defaults,
  JSON decoding/encoding, and future schema evolution.

- **`assert let PATTERN = VALUE` is rejected.** Although it could combine a
  shape assertion with binding, the syntax makes assertion, pattern matching,
  and declaration read ambiguously at a glance. Keep ordinary `assert`,
  explicit patterns, and existing conditional binding as separate concepts.

- **Nominal scalar `type X wraps Str` is rejected for now.** Do not add a
  second scalar-wrapper mechanism while the accepted `text as T` conversion
  work is still defining explicit typed conversion boundaries. Revisit only if
  real corpus bugs remain that `as`, enums, bounded scalars, or nominal
  records cannot express cleanly.

Accepted safety boundaries:

- `lint.prefer-env-path-list` remains a non-applying note. Rewriting path
  strings can lose a trailing empty component for an unset variable, cannot
  preserve non-UTF-8 values, and can fail for a directory containing `:`.
  `tests/xsh/stdlib/env_path_list.xsh::test_prefer_env_path_list_lint_offers_the_list_without_applying_it`
  preserves this boundary.
- The 100,000-open-call `stack-overflow` limit is an intentional runtime
  bound, retained by `tests/xsh/call-depth.xsh`; stage-block recursion now
  reports the same diagnostic before native stack exhaustion.
