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
- **Opaque `Any`.** `Any` can be navigated with fields, indexes, method
  calls, and iteration, which defers shape errors to run time. Make
  `.require(T)` or a type pattern the way in, and keep a local, greppable
  escape hatch for schema-less code. Open: the spelling of that escape hatch.
- **`lint.prefer-inferred-proc-return`** is opt-in, with about 160 provable
  sites here. Decide whether it becomes a default. A failed
  `.require(Contract)` raises a plain `Error` with facets; a built-in
  `ContractError` family would let code match it by name.
- **Record and JSON exactness.** See "Deferred and rejected".

## Designed, not built

- **The legacy `set` module.** `set.empty()` and `set.from(items)` return a
  `Set[T]` only where one is expected and the old `Map[Str, Bool]` otherwise,
  and `set.add` / `set.remove` work on that map. Both corpora are migrated to
  `Set[T]`; what remains is to make the two constructors always return a set
  and remove the two functions, with a `check.removed-set-function`
  diagnostic that names the method.
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

Language and checker:

- A `yield` in a stage block inside a stream producer passes the checker. A
  `stream` definition is rejected at a test file's top level.
- A later binding of a `with` that reads an earlier binding sharing a
  top-level `const`'s name reads the constant.
- An `Any` receiver takes no named arguments, so a required label cannot be
  required there. `FsRoot.symlink(target, path)` is still positional (39
  sites). A plain `.field` or `[i]` on an optional reports a generic type
  error instead of naming the optional.
- A typed call whose callee is a local, a field, or a call result is tied to
  its callable type only by a check at run time; the verifier ties parameter
  callees.
- A module command other than `json.write` checks and then fails at run time
  (`fs.fsync $p`: `unsupported-proc-command`).
- Diagnostics print an imported enum's type as a file path
  (`/…/kinds.xsh.Kind`). A diagnostic prints its source line whole, however
  long.
- Output that `print` has buffered is lost when `unix.exec` replaces the
  process.

Limits that are a crash or a wait instead of a diagnostic:

- Recursion through a stage block aborts at 100 to 150 levels. A function
  call is bounded at 100,000 open calls (`stack-overflow`).
- A spread of 16,000 fields takes half a minute: the checker is quadratic in
  its width. Lowering a pipeline clones the program once per stage.
- `xshi` has no thread with a sized stack.

Grammar and parser, outside the seeds the grammar tests generate:

- The grammar accepts `while let [.. a-b] = ...`; the parser rejects it. The
  parser accepts `xs |> first .name` and a spaced `(...) ?`; the grammar
  rejects both.
- Generated sentences that failed to parse in one-off deeper runs: block
  parameters split across lines (`{ ; | c ,` then a line break), `is 10ms`
  as a pattern, `y is a .. ]` in a slice, and a statement starting
  `$? * $? >`.

Formatter and lint:

- `xsht fmt` prints a bare-block match arm body as `({ ... })`, misindents a
  `"""` string that is the only argument of a nested multi-line call, and
  misindents a list that holds a raw string with an unbalanced `(`.
- A lint fix formats at the default width, not the project's `[format]`
  width. `lint.prefer-inferred-pure-return` is silent in a file that has a
  `use`.
- `lint.redundant-propagation` is silent on `let x = (run.text a ?)` and
  `let x = (run.text a)?`, whose `?` is removable.
- `lint.prefer-fail` changes what an uncaught failure reports, from
  `Family.Variant` to `validation`, and reads only the linted file, so a test
  in another file that names the kind goes stale.
- `lint.prefer-env-path-list` (a note) has 18 Laputa sites with no automatic
  fix: the list drops the trailing empty entry of an unset variable, keeps a
  value that is not UTF-8, and fails on a directory that contains `:`.

Tests:

- `lint_performance::repository_lint_is_clean_within_wall_budget` passes
  alone and fails when the rest of the `xsht` integration target runs beside
  it. `xsht lint` on this repository takes about 12 s of its 15 s budget.
- `showcase/tests/test-px.xsh::test_px_default_search_matches_executable_substrings`
  once failed to spawn a file it had just written (`Text file busy`), under
  load.

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
- Check a module once per workspace. `xsht lint` rechecks shared modules
  under every root that imports them (about 37 s of thread time here, and
  the same check again after every fix round); a per-module checked-interface
  cache is the fix, and `xsh` startup would use the same cache.
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
