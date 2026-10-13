# TODO

What is left after the hardening and ergonomics work: items that are designed
and not built, known defects, and ideas that were turned down. `docs/SPEC.md`
is the contract for everything that is built; nothing here describes it.

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
- `runtime::examples::example_corpus_lints_without_warnings` asserts an empty
  stderr, and lint now prints a timing line; the runtime gate skips that
  module, so nothing has said so.

## Defects

The W9 module publication gap is closed for parity: the sealed owner matches
the frozen reference on all 1,690 inputs and 6,760 profile outcomes, with
matching diagnostics. No current per-workspace module check or preparation
counts have been recorded. Close W9 only with that count evidence.

### The xsh-wide lint pass is landed but unverified

`lane/lintfix` (about 70 per-rule commits, 733 files) is merged without a test
run on the merged tree; the final gate battery was stopped. Resume by running
the native suite, `xsht check` over the repository, and the Rust gates that
walk the corpus (`syntax::grammar::`, `desugar::`, `lint_format_invariance::`),
then bisect any failure to the per-rule commit and revert that commit; a
failure after a lint rewrite is a lint bug. Lint is not at zero. Remaining
(count from before the second pass): `lint.shadowing` 158 (err),
`lint.unused-local` 152, `lint.organize-top-level-consts` 126,
`lint.prefer-slice` 158, notes `lint.empty-sentinel` 65 and
`lint.prefer-non-empty-argv` 56, and a handful of `dollar-in-expression-string`,
`unused-callable`, `unused-type`, `dead-code`. `crates/xsht/tests/lint_performance.rs`
cannot pass until warn/err/info reach zero. `core/dd.xsh`,
`core/tests/test-uu-tail-1.xsh` and `tests/xsh/stdlib/linux_storage.xsh` kept
master's text in the merge and were not linted.

Lint bugs found, with reproducers in the commit bodies of the reverts or the
lane report: `lint.prefer-positional-constructor` (bulk `xsht check` then fails
`compact.indexed-build` at `core/arch.xsh`; reverted); `lint.needless-annotation`
suggests dropping `List[Any]` that the checker then rejects; 
`lint.prefer-as-conversion` prints an unparenthesised `if ... else {} as Path`
(`core/cp.xsh:711`); `lint.prefer-collect` rewrites rejected by the checker;
`lint.prefer-record-constructor` joins long literals onto one line;
`lint.redundant-close` leaves the removed line's indentation on the next line;
directory-argument discovery ignores `xsht-config.ini` excludes (the excluded
`core/tests/data/patch/tree.xsh` was rewritten once and restored).

### Host-load flakes and slow tests in the full native suite

One full `xsht test` on the merged tree under host load 35 to 75 (other jobs
on the machine, 1.5 h) ended `13377 passed; 116 failed; 138 skipped`.
Re-running the 72 `FAILED` IDs one at a time passed 71; the remaining one
(`test_uu_tail_follow_descriptor_vs_rename1`) is a startup race fixed in the
test. Re-running the 44 `TIMEOUT 2m` IDs one at a time passed 40; four still
time out alone, and two of them also time out at 2 minutes on the frozen
abdd589c build, so they are slow tests and not regressions:
`test-gnu-cut.xsh::test_gnu_cut_cut_log`,
`test-gnu-factor.xsh::test_gnu_factor_factor_parallel_log`,
`test-uu-sort-2.xsh::test_uu_sort_merge_write_error_does_not_panic`,
`dev/tests/test-system-report-check.xsh::test_system_report_traced_live_and_replay_paths_keep_host_effect_...`.
Which of the other 111 are test pollution and which are timing-gated is not
settled. Known pollution: a test that closes a descriptor and asserts the
error from the same number races any concurrent test that opens a file
(`linux_storage.xsh::test_real_ioctls_on_a_regular_file...`, fixed by probing a
never-open number); other tests with that shape were not searched for. Known
timing gates: 3 s and 5 s child deadlines in `core/tests` (awk, dd, tail,
find, sed, ls) and the btop PTY tests' 10 s waits against a 10 to 25 s
verification-profile applet startup. To separate them, run the suite at
`-j 1`, `-j 8` and the default, and diff the failing sets; a failure that
follows the worker count and moves between runs is pollution, one that
follows host load alone is timing.

### Smaller open items

- `[e for e in fs.children(dir)?]` yields an empty list while
  `for e in fs.children(dir)? { }` visits both entries (found by the laputa
  lint pass, `guest/asterinas/viability/overlay/usr/lib/asterinas/boot-proof.xsh:117`;
  `lint.prefer-list-comp` fires on that shape and its rewrite changed behavior).
- Each lint candidate probe is a full parse, load and check of the import
  graph (introduced by `da40b6b4`); options are one shared check of the
  unchanged graph, grouped candidates, or skipping probes on a clean corpus.
- The full IR verifier derives a field receiver's type from the slot's
  declared type; a narrowed optional record is peeled one layer, but a callable
  field read or call through a narrowed optional record, and an error pattern
  on a narrowed `FetchError?`, may still be rejected. The soundness fuzzer
  does not reach them.
- `tag` pattern verification (`verify_tag_pattern_subject`) has the same
  declared-union shape as the error-pattern subject fix.
- `third_party/regex-lite` is a vendored copy of regex-lite 0.1.9 whose capture
  name map is a `BTreeMap`, so compiling a regex literal does not need
  `getrandom` (`test_uu_shuf_getrandom_fail`); the owner chose this over
  leaving the test red. Build time and binary size were not measured; drop the
  patch if an upstream release removes the std `HashMap`.
- The `Return` continuation fix for an unowned block frame was verified for
  error payloads only; a returned resource through that path has no test.
- `tests/xsh/list-pattern.xsh` lines 142 and 144 use the one-line `=> last _ =>`
  arm form that the parser now rejects without a separator; they are
  error-expectation fixtures and may pass for the wrong reason.

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
- Measure module-check reuse with current per-workspace pass counts. An older
  source snapshot reported repeated checks of shared modules (about 37 s of
  thread time and another check after each fix round); that timing is
  historical, and no current pass-count measurement is recorded.
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
