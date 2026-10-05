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

## Wave 0: before any lane starts

1. Frontend cost. `xsht check` and `xsht lint` print their stage times; a lane
   runs them hundreds of times, so the measured waste (the whole import graph
   checked four times per entry, serially) is fixed first. Record the timing
   lines of `xsht check` and `xsht lint` on this repository and on Laputa when
   this closes. They are the campaign's performance baseline.
2. Decisions. `TODO.md` "Decisions" records the closed design points and
   lists the open ones by item. An item with an open point is not assigned.
3. Sugar expansion. A sugar form has been a first-class arena statement
   that the checker, lowering, the verifier, and the executor each handle
   (`GuardedStmt` appears in nine files), and about a dozen backlog items
   are sugar by the `docs/DESIGN.md` definition. Before the first wave, the
   parser learns to write a sugar form as one surface node that carries its
   operands and its expansion into core forms. The formatter, lint, and
   `xsht grep` read the operands; the checker and everything after it read
   only the expansion. That makes "desugars trivially" an implementation
   fact, gives sugar the core forms' verifier and fuzz coverage, and keeps
   ergonomics items out of `src/sema/check/stmt.rs` and
   `src/runtime/eval/lower.rs`. `docs/ARCHITECTURE.md` owns the mechanism;
   `SCOPE-2` is its first form. `CMD-12` and `CMD-11` follow as soon as it
   merges: the existing sugar moves onto it, and `xsht desugar` prints it.
4. Lint placement. A new rule lives in its own `crates/xsht/src/lint_NAME.rs`
   (as `lint_try_capture.rs` does) and adds only its registration to
   `lint.rs`, so eight lanes do not edit one 15,000-line file.

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
3. **Launch.** Start the wave's lanes together, each with
   `isolation: "worktree"`. Start with four lanes in the first wave and widen
   to eight once build and test times under load are known: every lane builds
   Rust, a release rebuild takes about two minutes of all ten cores, and a
   loaded machine turns timing-sensitive tests into noise.
4. **Review, merge, then gate**, one lane at a time as they report. Read the
   diff; use `/code-review` for checker, lowering, verifier, and executor
   changes. Rebase the lane onto the campaign branch and fast-forward, one
   commit per item. Then the integrator runs the gates for the areas the
   lane touched (`docs/TESTING.md` "Broader" column) on the merged tree.
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
     tests, `cargo test --release -p xsh-fuzz --test soundness`, and
     `make docs-check` (`docs/TESTING.md` has the exact commands);
   - `make fuzz` once;
   - the `xsht check` and `xsht lint` timing lines on this repository and on
     Laputa against the baseline. A wave that slows either by more than a
     tenth is explained or fixed before the next wave.
6. **Migrate.** For each lint the wave added, in dependency order: apply it
   to this repository with `xsht lint --only RULE --fix`, run the gates it
   can affect, commit; then the same in Laputa. Then make the rule a default
   and delete its opt-in setting. Migrations run only here, between waves,
   because they rewrite the files lanes read and test against.
7. **Record.** Delete finished items from `TODO.md`, add defects the wave
   found, and update prerequisites. Report to the owner: what merged, every
   contract change, the timing lines, and the decisions the next wave needs.

## Lane contract

- **One worktree, one workstream.** Commit each finished item on the worktree
  branch; never push, merge, or rebase onto another lane.
- **SPEC first.** `docs/templates/SPEC.md` and `docs/snippets/spec/` change
  before or with the code, including a `rejected/` snippet for each new check
  error. Run `make docs`; never edit generated files.
- **A slice is finished** when it has the grammar productions, parser,
  checker facts, lowering, verifier rule, formatter and grouping support,
  `xsht grep` and annotation support where the surface needs them, native
  tests for the behavior and for each new diagnostic, and the migration lint
  with its autofix.
- **Hardening is part of the slice.** A new instruction shape gets a verifier
  rule and a unit test that corrupts a valid lowered program and requires
  rejection. A new checker fact is consumed by lowering, never re-derived
  there. A form the fuzzer's generator could produce gets generator and
  reference-evaluator support, or a line in the report saying why not. A
  check that moves a runtime failure to check time keeps the runtime failure
  as the defended fallback.
- **Behavior changes are named.** An item that changes what existing code
  does (a default, an index rule, a severity, an ordering) ships the lint
  that makes old code explicit first, and its report lists the change.
- **Shared files.** Add to registries at the end of the relevant group and
  nowhere else. Put a new lint in its own file. Do not reorganize, rename, or
  reformat a shared file.
- **Tests a lane runs.** Only the ones it wrote or changed, by exact name:
  `target/release/xsht test FILE` for its native test files and
  `cargo test ... NAME` for a Rust test it added. Use debug `cargo check`
  for compile checks and build with `-j 3`. A lane does not run a test
  target or suite as a whole, the corpus tests (lint/format invariance,
  lowering agreement), the soundness tests, `make docs-check`, `make fuzz`,
  or the Linux container. The integrator runs those after the merge, and
  the lane's report says which of its changes they should exercise.
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

## First wave

Items that need no decision, start their workstream, and overlap least:

| Lane | Items |
|---|---|
| `PROP` | `PROP-1`, `PROP-2`, then the `par-map` question in `PROP-3` and `PROP-4` |
| `ERR` | `ERR-1`, `ERR-2` |
| `PATH` | `PATH-1` to `PATH-4` |
| `SCOPE` | `SCOPE-1`, `SCOPE-2` |
| `MATCH` | `MATCH-1`, `MATCH-2`, `MATCH-3` |
| `TYPE` | `TYPE-1` |
| `MOD` | `MOD-1` to `MOD-4` |
| `CMD` | `CMD-1`, `CMD-2`, `CMD-9`, `CMD-10` |

`LINT-1` (why existing lints under-report on Laputa) runs in wave 0 or takes
a slot in the first wave: every later migration depends on the lints finding
their sites.

Items that change the behavior of existing code, or that depend on the most
other work, go last in their workstream: `PROP-8`, `PROP-9`, `PATH-11`,
`MATCH-7`, `TYPE-7`, `SCOPE-9`.
