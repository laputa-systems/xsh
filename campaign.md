Run the `TODO.md` backlog as a campaign: one integrator and up to eight
`xsh-campaign` lanes (Opus, medium effort; `.claude/agents/xsh-campaign.md`),
one lane per workstream, in waves.

`TODO.md` owns what to build and in which order inside a workstream. This file
owns how the work moves, and starts with where it stands.

## Handoff

The campaign paused on 2026-10-05 with no lane running. About two fifths of
the backlog is merged. Everything needed to resume is in two repositories and
this file; nothing lives only on the machine the first waves ran on.

### Where the work is

- **`master`** has every merged item, including the commits that were held
  back for migrations (the `abort` removal and the two check errors).
  `xsht check`, `xsht lint`, and `xsht fmt --check` are clean on it.
- **`campaign-utils`** is rebased onto `master`: its fifty commits, a commit
  that makes the typed fs primitives build on macOS, and a commit that
  brings its applets and collectors to the current language. Its history was
  rewritten, so it needs a forced push before another machine can use it.
- **Laputa `master`** is migrated through every lint the campaign added,
  one commit per rule, and checks clean (`TODO.md`, "Laputa").
- `campaign-harden` is fully merged. Lane worktrees and the local branches
  `worktree-agent-*`, `cmd-*`, and `err-*` are leftovers of the first
  machine and hold nothing that is not on `master`.

### Setting up

1. Check out `xsh` and `laputa` side by side: tests and migrations read
   Laputa at `../laputa`, or at `XSH_LAPUTA_CORPUS`.
2. Build with `cargo build --release -p xsh --bins -p xsht --bin xsht`; the
   toolchain is pinned in `rust-toolchain.toml`.
3. Run the gates once, before changing anything, so the machine has its own
   baseline (`docs/TESTING.md` has every command):
   `target/release/xsht check`, `lint`, `fmt --check`, and `test`;
   `cargo test --release -p xsht --no-fail-fast`;
   `cargo test --release --test integration --no-fail-fast -- --skip runtime::coverage:: --skip runtime::examples::`;
   `cargo test -p xsh --lib`; `cargo test -p xsh-registry`;
   `make docs-check`.
4. Record the timing lines `xsht check` and `xsht lint` print, here and in
   Laputa. No baseline was ever taken on a quiet machine; on a ten-core M1
   Pro `xsht check` takes about 3 s here and 7 s on Laputa, and `xsht lint`
   about 10 s here.
5. The lane agent is `.claude/agents/xsh-campaign.md`. On macOS, if
   `make docs` rewrites a doc you did not touch, a snippet depends on GNU
   tools: put GNU coreutils first on `PATH`. All Linux work goes through the
   `Dockerfile.test` container (`cargo dev test linux`).

### What is known to fail on `master`

On macOS, with nothing else running. Each is recorded in `TODO.md`,
`LINT-8`.

- Native suite: 2 of about 2,245,
  `tests/xsh/pattern-conditionals.xsh::test_pattern_conditional_lint_retains_comments_guards_and_error_bindings`
  and
  `tests/xsh/pattern-tests.xsh::test_pattern_predicate_lint_preserves_comments_and_bindings`.
- `xsht` integration target: 3 lint tests,
  `lint::fs_root_receiver_cli_fix_checks_an_isolated_fixture_and_converges`,
  `lint::linter_list_compound_assignment_reaches_every_argument_that_leaves_the_local_alone`,
  and `lint::linter_reports_dead_code_after_all_returning_match`; and
  `lint_performance::repository_lint_is_clean_within_wall_budget`, which
  passes alone and fails when the rest of the target runs beside it.
- The root integration target, the `xsh` unit tests, the registry tests,
  and `make docs-check` passed in the last full run; eight stale fixtures
  were fixed after it and rerun one by one, not as a whole.

### What `campaign-utils` still needs

- A Linux run. After the rebase it was built and tested only on macOS:
  `xsht check`, `lint`, and `fmt --check` are clean, and the native suite is
  2,444 passed and 11 failed. Two of the failures are the ones above; nine
  are tests of Linux behavior with no macOS skip (`tests/xsh/stdlib/fs_prims.xsh`,
  six tests; `core/tests/test-cat.xsh`, `test-tee.xsh`, and
  `test-hostname.xsh`, one each). Run them, and the uutils and GNU suites
  the branch tracks, in the container.
- A decision on the macOS stand-ins: `fs.mknod` fails there with `ENOTSUP`,
  and `statvfs` reports `nodev` and `noexec` as false.
- A look at the files where the rebase met the lint migrations
  (`core/lib/system_report_live.xsh`, `core/lib/system_report_collect.xsh`,
  `core/head.xsh`, `tail.xsh`, `tee.xsh`, `cat.xsh`, `rev.xsh`, `uname.xsh`).
  Conflicted hunks took the branch's side and were migrated again by
  autofix; one list literal that git merged wrongly without reporting a
  conflict was repaired by hand, and the checker would have caught another.

### How a merge is done

The wave loop below says when; this is how, in the order that worked.

1. Cherry-pick the lane's commits. A conflict where both sides only add
   lines (a `mod` line in `crates/xsht/src/lint.rs`, a registry row) keeps
   both. A conflict in a generated file takes either side; the file is
   regenerated in step 4. A conflict in XSH source between a lint migration
   and a real edit takes the edit; step 3 migrates it again.
2. `cargo check --workspace --all-targets`, then the release build. Compile
   errors here are usually one lane's code meeting another lane's change to
   a shared type.
3. `xsht check`, then `xsht lint --only RULE --fix` for each new rule, then
   `xsht fmt` on the files `xsht fmt --check` names. Fix what has no
   autofix by hand.
4. `make docs`, and
   `target/release/xsht api summary --format jsonl > tests/fixtures/modules/standard-api-surface.jsonl`.
5. Commit, run the gates, and send failures back to the lane.
6. Apply the same rules in Laputa, one commit per rule. Then recompute the
   `sha256` of every local source its `PKGBUILD.xsh` files declare under
   `packages/*/files/`, regenerate
   `tests/pm/fixtures/plans/basic-aarch64.json` from the output of
   `tests/pm/pm_plan.xsh::test_build_plan_json_round_trip_and_detects_corruption`,
   and run `xsht check` and `xsht test` there.

### Next

All with closed designs: `PROP-5` to `PROP-7`, then `PROP-8` and `PROP-9`;
`ERR-8`; `PATH-7` to `PATH-10`; `SCOPE-6` and `SCOPE-12`; `MATCH-2`,
`MATCH-3`, `MATCH-5`, `MATCH-6`; `TYPE-2`, `TYPE-3`; `CMD-5`, `CMD-6`;
`LINT-2` to `LINT-8`. `ITER` (iteration speed) has no design yet and is
worth doing before another wide wave: start with `ITER-1`.

### What the first waves taught

- The gate cycle is the bottleneck: fifteen minutes alone, 25 to 40 with
  lanes building. Five or six lanes kept the merge queue short; eight did
  not.
- Lanes that touch syntax conflict with each other. Merge those one at a
  time, and hold the branch still while one of them rebases.
- A lane cannot see another lane's change to a shared type until the merge,
  so it runs `cargo check --workspace --all-targets` before it reports.
- Applying each autofix to Laputa is a gate in its own right: it found a fix
  that built its replacement from the wrong file's text.
- A rule that turns a warning into an error ships in two commits, and the
  second waits for both corpora to be migrated.
- Git can merge a reformatted multi-line literal with a one-line edit inside
  it and report no conflict. `xsht check` after every merge is what catches
  it.
- `git pull --rebase` over a merge commit replays every merged commit one by
  one. Merge the upstream branch instead.

## Hardening principles

The campaign adds language surface; these keep it from adding ways for the
checker and the runtime to disagree. They apply to every slice.

- **Practical, not formal.** The goal is fewer opportunities for checker and
  runtime to disagree, invalid internal states caught earlier, and tests for
  features that interact. It is not a verified language, and nothing here
  claims soundness. A report says exactly what became harder to get wrong.
- **Decide once, validate before execution.** Each semantic decision is made
  by the checker and published as a fact. Lowering consumes it and never
  re-derives it; the verifier checks the consequences of a decision on the
  lowered program and never repeats the decision.
- **Keep the architecture.** No second frontend, competing inference engine,
  fallback interpreter, or replacement IR. Strengthen and connect what
  exists.
- **Two tests per invariant.** One shows the valid behavior; one would catch
  the violation: a valid lowered program, deliberately corrupted, must be
  rejected before it runs. Say whether a finding is a reachable bug or an
  internal assumption that needed defending.
- **No silent defaults.** Missing semantic information in a runnable program
  never becomes "no effects", a guessed type, or an executable placeholder.
  Erroneous source still gets ordinary diagnostics, not an internal crash.
- **Expected failure is not a broken promise.** Rejecting untrusted data at
  run time is ordinary behavior. A concrete operation receiving a value the
  checker ruled out is a defect; do not classify it away.
- **Performance is part of correctness here.** No second run of inference,
  no frontend structures kept after preparation, no recursive validation on
  hot paths. Expensive observation is opt-in. Measure the paths a change
  touches.
- **Be selective about new metadata.** Use the facts already published before
  adding one, and never carry a whole checker state for one assertion.
- **Name the evidence.** A production check, a test, a bounded exhaustive
  check, and a proof are different things, and a bound is stated with its
  result.

The same principles direct implementation hardening proper, wherever a slice
reaches it: precise, tested contracts for effects first; verifier rules for
what lowering assumes; an independent small model for the effect solver;
exhaustive or symbolic checks of small kernels such as effect-set operations
and index arithmetic; and opt-in runtime checks of static promises.

## Roles

The integrator is the main session. It picks each wave, writes the lane
briefs, reviews and merges every lane, runs the full gates and the corpus
migrations, keeps `TODO.md` current, and brings contract changes and open
decisions to the owner. It writes no feature code while lanes are running.

A lane owns one workstream for one wave. It works in its own git worktree and
commits there. Lanes never talk to each other; a lane that needs something
from another workstream reports it and the integrator sequences it.

`xsh-routine` runs corpus migrations: one lint rule, named directories, one
commit.

## What every lane relies on

- `xsht check` and `xsht lint` end stderr with their stage times, which are
  the campaign's performance evidence.
- A sugar form is one surface node that carries its operands and its
  expansion into core forms (`docs/ARCHITECTURE.md`, "Adding a sugar form").
  The formatter, lint, and `xsht grep` read the operands; the checker and
  everything after it read only the expansion; `xsht desugar` prints it. An
  item that is sugar by `docs/DESIGN.md` adds a form there and does not touch
  the checker or lowering.
- `TODO.md` "Decisions" records the closed design points and lists the open
  ones. An item with an open point is not assigned.

## The wave loop

1. **Select.** For each workstream take the next items in `TODO.md` order
   whose prerequisites have merged and whose design is closed. A lane gets one
   to three items, sized to finish in one session. A workstream with nothing
   ready sits the wave out; concurrency is a ceiling, not a target.
2. **Brief.** Each brief names the item IDs, the SPEC sections to change, the
   files the lane is expected to own, the files it must not touch, and
   anything earlier waves learned. It names no gates: a lane runs only the
   tests it wrote. The brief is the lane's only context beyond the
   repository.
3. **Launch.** Each lane runs with `isolation: "worktree"` and first
   fast-forwards its worktree to the campaign branch, because a new worktree
   can start several commits behind. Lanes build debug only; the release
   build and the gates are the integrator's, so they run once per merge
   instead of once per lane. A lane that reports is given its next items in
   the same session, after `git reset --hard` to the campaign branch, so it
   keeps what it learned.
4. **Review, merge, then gate.** Read the report and the diff; use
   `/code-review` for checker, lowering, verifier, and executor changes.
   Cherry-pick the lane's commits onto the campaign branch, one commit per
   item. Lanes that report while a gate run is in progress are merged
   together and gated once. After a merge the integrator builds release,
   applies each new lint to this repository (`xsht lint --only RULE --fix`),
   formats the files `xsht fmt --check` names, regenerates the API surface
   fixture and the docs (`make docs`, with GNU coreutils first on `PATH`),
   commits, and runs the gates (`docs/TESTING.md` "Broader" column).
   Gates run here, once, on the code that will ship, not in every lane on
   code that is about to be rebased. A failure goes back to the lane with
   the command and its output; the lane's commits stay merged unless the
   failure blocks other lanes, in which case they are reverted until fixed.
   A conflict in a registry (keyword table, diagnostic codes, arena kinds,
   lint registration, SPEC section list) is resolved by the integrator; a
   semantic conflict goes back to the lane that merges second.
5. **Close the wave** with every lane merged and none running:
   - `cargo build --release -p xsh --bins -p xsht --bin xsht`, then the full
     native suite, the `sema::`, `syntax::`, and filtered `runtime::`
     integration gates, `cargo test --release -p xsht`, the `--lib` unit
     tests, and `make docs-check` (`docs/TESTING.md` has the exact
     commands);
   - the `xsht check` and `xsht lint` timing lines on this repository and on
     Laputa against the baseline. A wave that slows either by more than a
     tenth is explained or fixed before the next wave.
6. **Migrate Laputa.** Apply the lints the wave added to Laputa, one rule
   per commit, leaving files with uncommitted changes alone. A lint that
   shipped opt-in because its fix is not automatic becomes a default once
   both corpora are clean, and its setting is deleted.
7. **Record.** Delete finished items from `TODO.md`, add defects the wave
   found, and update prerequisites. Report to the owner: what merged, every
   contract change, the timing lines, and the decisions the next wave needs.

## Lane contract

- **One worktree, one workstream.** Commit each finished item on the worktree
  branch; never push, merge, or rebase onto another lane.
- **SPEC first.** `docs/templates/SPEC.md` and `docs/snippets/spec/` change
  before or with the code, including a `rejected/` snippet for each new check
  error. Check a snippet with the debug `xsht check FILE`. Never edit
  generated files and do not run `make docs`, which builds release binaries:
  the integrator regenerates the docs after the merge.
- **A slice is finished** when it has the grammar productions, parser,
  checker facts, lowering, verifier rule, formatter and grouping support,
  `xsht grep` and annotation support where the surface needs them, native
  tests for the behavior and for each new diagnostic, and the migration lint
  with its autofix.
- **Hardening is part of the slice.** A new instruction shape gets a verifier
  rule and a unit test that corrupts a valid lowered program and requires
  rejection. A new checker fact is consumed by lowering, never re-derived
  there. A check that moves a runtime failure to check time keeps the
  runtime failure as the defended fallback.
- **Behavior changes are named.** An item that changes what existing code
  does (a default, an index rule, a severity, an ordering) ships the lint
  that makes old code explicit first, and its report lists the change.
- **Shared files.** Add to registries at the end of the relevant group and
  nowhere else. Put a new lint in its own file. Do not reorganize, rename, or
  reformat a shared file.
- **Lints.** On by default, unless the fix cannot be automatic. A lint
  visits only the module being linted, through the linter's existing
  statement and expression traversal; it never scans an arena table, which
  holds the whole workspace. The report gives the sites found and the sites
  with a fix in this repository and in Laputa.
- **API surface.** A change to the standard modules or methods regenerates
  `tests/fixtures/modules/standard-api-surface.jsonl` with the debug
  `xsht api summary --format jsonl`.
- **Debug builds only.** A lane never builds with `--release`; release
  builds, and so every measurement and every Rust test target that spawns a
  binary, belong to the integrator. Build with
  `cargo build -j 3 -p xsh --bins -p xsht --bin xsht` and use `cargo check`
  for compile checks.
- **Tests a lane runs.** Only the ones it wrote or changed, by exact name:
  `target/debug/xsht test FILE` for its native test files and debug
  `cargo test --lib ... NAME` for a unit test it added. A Rust integration
  test that spawns a binary refuses to run in debug: write it, and leave
  running it to the integrator. A lane does not run a test
  target or suite as a whole, the corpus tests (lint/format invariance,
  lowering agreement), `make docs-check`, or the Linux container. The
  integrator runs those after the merge, and the lane's report says which
  of its changes they should exercise.
- **No fuzz gate.** Neither lanes nor the integrator run `make fuzz` or the
  `xsh-fuzz` test targets as part of the campaign, and an item does not
  extend the fuzzer unless the item is about it.
- **Scope.** Do not migrate the corpus, settle an open design point, add a
  dependency, or fix another workstream's defect. Report those instead.

## Workstreams

Each is a section of `TODO.md` with ordered, numbered items.

| ID | Workstream | Starts from |
|---|---|---|
| `PROP` | propagation and failure flow | `src/sema/check/{stmt,expr}.rs`, SPEC 1 and 8 |
| `ERR` | error families and variants | `src/sema/check/{decl,pattern}.rs`, SPEC 4.10 and 5.5 |
| `PATH` | paths and the filesystem API | `crates/xsh-registry`, `src/modules/fs.rs`, SPEC 4.4 and 15 |
| `SCOPE` | scoped blocks, time, and resources | `src/syntax/parser/stmt.rs`, SPEC 8.7, 8.8, 11.8 |
| `MATCH` | matching, binding, and conversion | `src/sema/check/pattern.rs`, SPEC 5.4, 6.8, 6.10 |
| `TYPE` | type-level hardening | `src/sema/{types,constraints}.rs`, SPEC 4 and 5 |
| `MOD` | effects, modules, and inference | `src/sema/check/infer_{effects,return}.rs`, `src/loader.rs`, SPEC 3.3, 4.9, 9 |
| `CMD` | commands, lexer, CLI, and tooling | `src/syntax/lexer.rs`, `src/syntax/parser/command.rs`, SPEC 2, 3.2, 10, 11 |
| `LINT` | fixes to existing lints | `crates/xsht/src/lint.rs` |

Prerequisites that cross workstreams are written on the item in `TODO.md`.
