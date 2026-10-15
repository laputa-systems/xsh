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

The fresh strict W9 comparison on 7e93 passed for 1,690 inputs and 6,760
outcomes with zero mismatches. Both exact-owner proofs cover 1,690 inputs.
The earlier f105 pass remains historical. The C1 comparison failed with 24
mismatches before the narrow
derived-reference retry correction at `1fd9e5f2f` passed a direct build and
four positive and two negative fixture programs; its Rust unit build failed
because the musl host does not support the `mini-internal` proc-macro. The
correction-controller pin fix `19fcb0d91` passed eight tests and live source
validation. The fresh full comparison now qualifies the corrected reference;
the [handoff](dev/consolidation/HANDOFF.md#final-batch-status-2026-10-15)
records the direct unit and compiler-image qualification.

The source-qualified 8ca cache receipt is also historical. It records XSH check
1,675 and default lint 1,533, plus Laputa check 379 and default lint 316. No
shared key repeats. Candidate-probe caches are private and separate from those
shared session counts. See the [2026-10-15 consolidation handoff](dev/consolidation/HANDOFF.md#final-batch-status-2026-10-15)
for current W9 boundaries and receipts.

### Current consolidation acceptance gates

The repaired source passes the isolated clean lint preflight: 1,524 files,
zero findings, 113 notes, and 13.51 seconds within the 15-second budget.
Readiness passes before and after. Fresh W9 and structural qualification remain
required after the Rust repairs. The earlier 7e93 structural check covered 370
source files and satisfied all non-advisory metrics and required-zero targets.
The start-source scanner rerun and archived report are byte-identical.
The ten semantic rows and final-close verifier remain open. The current
successor scanner digest begins `352d27af`; its 39 focused policy tests pass.
Source totals remain advisory.
The 8ca feature-touch probe records
38 XSH and 18 XSHT compiler demands from isolated placeholder arms. It does not
prove runtime reachability or feature-touch coverage on the current source. The
source-qualified 0ab-to-8ca review found 25 of 27 selected blobs identical and
confirms the changed statement-flow sites are unchanged.

The 152-tag dispatch review classified four defensive error fallback paths and
found no concrete route divergence. The 74 authenticated route receipts cover
36 of 92 expression tags. The focused statement and typed-scalar suites cover
42 and 31 cases. No paired runtime run covers all tags.

The review of all 46 generated stage rows is complete. An allocation probe on
8ca is retained separately from the Rust allocation test filter, which remains
pending. The frozen d02 opt3 suite attempt failed correctness before the
focused repairs, so its timing is invalid. The handoff records the result and
the post-repair focused checks; the nine-suite run remains pending.

Sixty focused lint tests are source-qualified from an earlier run and do not
replace the final lint gate. These reviews do not close the ten acceptance
rows.

The canonical fixed-seed,
integration, normal and forced runtime, repository, documentation, Linux
privileged, size, memory, scheduler, comparative, and guarded performance gates
remain pending.
The canonical integration run owns the 22,768-candidate deep grammar proof; do
not run it twice. The batch gate (`cargo dev gate --batch`) owns fixed-seed and
tooling checks plus one full native suite. The source-qualified documentation
generator passed without changing files, but canonical docs, snippets, and
project checks remain pending. The final report must state cumulative campaign
fuzz seeds and upstream compatibility results. See the
[consolidation handoff](dev/consolidation/HANDOFF.md) for current receipts and
close requirements.

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
reproduce or disprove the earlier rewrite report. A bounded current-source audit
found no evidence of the alleged bug; the historical report remains unconfirmed
because no reproducer is available. The audit does not disprove that report.
Canonical current-source lint acceptance remains open.

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

The 59a6dfbc run under host load recorded 13,437 passes, 56 failures, and 138
skips. That result and the older 13,377 passed, 116 failed, and 138 skipped
count are historical. The frozen d02 attempt failed before the focused repairs;
the post-repair suite has not run. The focused GNU rm repair passes its exact
parity cases and isolated 400,000-entry case, but full stress acceptance remains
open. The handoff links the exact run log and qualification.

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

Use `cargo dev gate PATH...` for focused changes. It selects gate families for
changed paths, deduplicates them, and runs focused native tests with the
required hard checks. Cargo reuses up-to-date build artifacts. Use
`cargo dev gate --batch` when you need the full process suite, debug tests,
tooling, fixed-seed soundness, one full native suite, repository checks, and
documentation checks. It also runs the source-bound consolidation ratchet.
Linux also runs the privileged test plan. See
[Testing](docs/TESTING.md) for gate owners and commands.

Process and native tests use the optimized `verification` profile, which has
no LTO and 64 codegen units. Debug `--lib` tests retain debug-only assertions.
`cargo dev check` uses release products for repository checks. The root
package's `--lib` test target can trigger LLVM `ScalarEvolution` recursion in
release; select root integration targets with `--test`. Build only the needed
package and binary. The `dist` profile is for release packaging. On qualifying
musl hosts, `cargo dev` preloads jemalloc for compilation only; see
[Testing](docs/TESTING.md) for the exact policy.

The earlier `src/runtime/eval/lower.rs` and `crates/xsht/src/lint.rs` estimates
of 16,000 and 15,000 lines are historical. Those files are module roots with
implementation split across `src/runtime/eval/lower/` and `crates/xsht/src/lint_*.rs`.
File size does not establish current rebuild time.

The documented release lint gate has a 15-second budget. The earlier 17-second
standalone result and failures under concurrent integration load are
historical. Current standalone status is unmeasured, so keep the documented
gate authoritative. Final comparative performance results are pending, and
guarded performance gates have not started.

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
