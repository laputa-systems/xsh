Run the `TODO.md` backlog as a campaign: one integrator and up to eight
`xsh-campaign` lanes (Opus, medium effort; `.claude/agents/xsh-campaign.md`),
one lane per workstream, in waves.

`TODO.md` owns what to build and in which order inside a workstream. This file
owns how the work moves. `harden.md` is the tone: small finished increments,
contracts with a test that passes and a test that would catch the violation,
checks at boundaries, measured performance, and no claim of soundness.

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

## Foundations

Three things were put in place before the first lane and every lane relies
on them.

- `xsht check` and `xsht lint` end stderr with their stage times. The
  integrator compares them after each merge; they are the campaign's
  performance evidence.
- A sugar form is one surface node that carries its operands and its
  expansion into core forms (`docs/ARCHITECTURE.md`, "Adding a sugar form").
  The formatter, lint, and `xsht grep` read the operands; the checker and
  everything after it read only the expansion. An item that is sugar by
  `docs/DESIGN.md` adds a form there and does not touch the checker or
  lowering.
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
