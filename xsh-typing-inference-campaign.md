# XSH typing inference: plan

Status: decisions settled on 2026-10-02. Execute the work items in order
(1 → 6). Items with disjoint files may run as parallel lanes (see Orchestration).

## Where things stand

The first campaign (`f9cec20c`..`6f25e76d`) was reverted to the pre-campaign
tree `d6f09bc5` in `c4570242`. The full implementation remains in history at
`6f25e76d`. Do not resurrect it. Its inference core (`src/sema/inference/`) is
the only part worth consulting, and only if parameter generalization is
reopened.

Why it was reverted, measured at `6f25e76d`:

- `src/` grew from 144k to 239k lines in three days. About 21k of the 95k added
  lines were tests.
- `tests/xsh/stdlib` dropped from 335 passing tests to 96. 21 whole modules
  failed preparation, so about 277 of 381 test declarations never ran. The
  ergonomics suites failed outright.
- No annotation was removed from any maintained program. The campaign's own
  inference tests passed 56 and failed 30.
- Most runtime additions verified checker output against copies of itself:
  "receipts", "original authority", and owner and rewind checks. That
  vocabulary grew from 35 lines in 7 files to 5,074 lines in 255 files. The
  checks rejected valid programs whose operation had no prepared protocol yet.
- The new solver ran inside the old checker through about 150 bridge sites, and
  the legacy adapters it was meant to retire stayed.

Native tests now pin every behavior from the implemented ergonomics proposals
(`308d0b28`). `tests/xsh/statement-value-contracts.xsh` pins the current
statement, value and Result matrix.

## Lessons that constrain this plan

- **Inferring polymorphic parameter types is what made it large.** It turned
  generic execution and evidence passing into runtime problems, and most of the
  growth followed from that.
- **Checker and runtime run in one process.** In-process proof checking between
  them has no adversary, and it failed valid programs.
- **One contract asked for too many things at once.** Each requirement looked
  defensible alone; together they multiplied.
- **The removal targets measured the wrong corpus.**
  - 69% of the "eligible" annotations were in agent-written system-report code.
  - 509 of the 692 eligible effect clauses sit on `test` declarations, demanded
    only by `lint.unannotated-effects`.
- **Work that is not green at each step does not converge.**

## Goal and rules

Remove annotation and assertion noise from ordinary XSH glue code. Change only
the existing checker, lint and tooling, not the runtime architecture.

- **Signatures at declaration boundaries; inference inside bodies.** Required
  parameters of named declarations, public contracts (exported returns and
  effects), schema and validation boundaries, and recursive declarations
  (unless item 6 lands) stay annotated.
- **Inference stays local and monomorphic.** The checker publishes concrete
  types, so lowering and execution need no generic evidence.
- **Bool values are always data; assertions are explicit.** A Bool expression
  statement is an error, and `assert` is the only assertion form (item 4).
  Process `Status` statement semantics are unchanged.

The baseline already infers locals (including empty and null locals), default
parameter types, `.require()` targets, generic record constructor arguments,
private pure returns, and private proc effects.

## Non-goals

- Inferring required parameter types, polymorphic generalization, row
  polymorphism, effect variables, or typeclass-like operation requirements.
- Explicit generic functions (`f[T](...)`). Not planned; revisit only with
  concrete demand from maintained code.
- Runtime evidence passing, prepared generic execution, or checking checker
  output beyond the existing structural IR verifier.
- Inferred public contracts.
- Reference solvers, frozen annotation denominators, operation inventories,
  benchmark cohorts, or status ledgers.

## Work items

Each item is one or more vertical slices with a size budget. Each slice lands
green, with spec, native tests, checker, lint and docs together. If a slice
exceeds twice its budget, stop and re-plan rather than pushing through.

### 1. Stop the lint from demanding redundant effect clauses

Budget: about 100 lines of Rust, plus a scoped migration.

- In `crates/xsht/src/lint.rs::lint_effect_annotation`, stop firing
  `lint.unannotated-effects` and `lint.missing-effects` on `test` declarations,
  CLI `main` and conventional `main`. Those entries are already unrestricted,
  and nothing restricted calls them.
- Keep the rule for exported procs and streams. Without a clause they are
  unrestricted, so restricted callers cannot use them, and for exported procs
  the clause is the public contract.
- Clauses that are present remain upper bounds.
- Migration: one scoped rule removes redundant clauses from `test` declarations
  under `tests/`, `core/tests/`, `dev/tests/` and `showcase/tests/`.
  - Removal is safe because a body that checks under its clause also checks
    unrestricted.
  - Keep explicit `[]` clauses: they deliberately assert that a test body is
    effect-free.
  - Review the diff, then run the full native suite before committing.

### 2. Fix the known bugs

Budget: about 200 lines per bug, each with a regression test.

- `"/opt/x" in env.PATH` checks but is always false at runtime; `p"/opt/x"`
  works. `env.PATH.append("/opt/x")?` checks, then fails at runtime with
  "expected Path, found Str". The likely cause is `is_path_like_arena_expr` in
  `src/sema/check/expr.rs` accepting a Str literal. Make the checker and
  runtime agree: either convert or reject.
- Parametric record mismatches report "expected Record, found Record". Show the
  type applications, for example `Observation[Int]`.
- `error Failure[T] = …` is rejected as `check.unresolved-proc-command`. Give
  it a generic-error-family diagnostic.
- The first campaign reported that
  `tests/runtime/modules.rs::LocalHttpsHttp2Server` can block in `accept()`
  after its client fails. Reproduce it on this tree, then bound the server's
  cleanup.

### 3. Consolidate the layers that multiply typing cost

Each slice should remove lines on net. These predate the campaign: today every
typing feature is implemented two or three times.

- **3a. Lowering consumes checked types.**
  - Today `src/runtime/eval/lower.rs` re-derives types and overloads; there are
    117 `infer_*` and `infer_checked_*` uses.
  - Read the checker's published types and resolved operations instead, and
    delete the re-derivation.
  - Gate: full native suite plus the indexed and runtime Rust gates.
- **3b. Remove the compact body probe.**
  - Today `src/sema/check/compact.rs` re-checks bodies with fresh constraints
    alongside the full checker (`SPEC-TYPING.md`: "Full checking, compact body
    facts, and lowering infer the same result type").
  - Derive compact facts from full checker output, and keep one
    statement-position and one type store.
- **3c. One dispatch path per operation.**
  - Today `lowered_run/indexed_run.rs` (10.1k) and its heap-frame child
    `indexed_run/explicit_run.rs` (4.5k) each implement many of the same
    operations.
  - Make each operation's semantics live in one place. The heap-frame executor
    is already the only recursive-call executor, so prefer it.
  - Check stack-depth tests (`tests/runtime/stack_depth.rs`) and run the
    existing benchmarks in `docs/BENCHMARKING.md` before and after.

3a and 3b touch the checker and lowerer, so they run sequentially and before
item 4. 3c is runtime-only and can run in parallel with items 1, 2 and 3a/3b.

### 4. Explicit assertions and data tails

This replaces the implicit-Bool design.

- **4a. Make `assert` sufficient (parser and runtime).** Budget: about 300
  lines.
  - `assert condition` with an optional message.
  - On failure, report the same detail a bare Bool statement reports today: the
    expression, `left:`/`right:` operands of a failed comparison, and the
    reached pair of an ordering chain. Today `assert` prints
    `<List> == <List>` without values.
  - Reuse the bare-assertion diagnostics rather than adding a second
    formatter.
- **4b. Migrate.** One scoped rule prepends `assert ` at every statement in
  `CheckOutput::assertion_spans`, across `tests/`, `core/`, `dev/`,
  `showcase/` and `examples/`.
  - The checker's spans, not a regex, decide what is an assertion, so
    multi-line, parenthesized and tail forms are covered.
  - Expect about 6.6k test lines and about 200 production lines.
  - This lands while bare Bools are still valid, so the tree stays green.
  - Apply the same rule to the sibling repositories `../packages` (about 160
    sites) and `../laputa` (about 100 sites) before 4c lands. Make separate local
    commits in each repo, verified with that repo's native tests; never push.
  - `showcase/` is migrated too, because it would otherwise stop checking.
- **4c. Flip the language (spec first).** Budget: net negative.
  - A Bool expression statement, or a Bool tail of a `Unit` or `Result[Unit]`
    body, becomes a checker error that suggests `assert` or `let _ =`. Every
    callable tail is data.
  - Remove the Unit-tail assertion rule and `lint.prefer-bare-assertion`.
    Repoint `lint.core-assert` and the assertion-helper migrations at `assert`.
  - Update `docs/SPEC.md` (statement assertions, around "Boolean expression
    statements assert") and `docs/SPEC-TYPING.md` ("Checked Statement and Value
    Positions").
  - Intentionally rewrite the tests that pin today's behavior:
    `assertions.xsh`, `statement-value-contracts.xsh`, `ergonomics.xsh`,
    `assertion-tail-context.xsh`, `value-blocks.xsh`.
- **4d. Infer private proc returns.** Budget: about 400 lines including tests.
  - With implicit assertions gone, an unannotated private proc infers its
    success type from its value tails, like a private pure function, and
    returns `Result[T]`. A proc with no value tail keeps `Result[Unit]`.
  - Value-producing `if`/`match` tails, which are statements today, become
    values. Callers are unaffected, because non-Unit results in statement
    position are discarded.
  - Reuse `src/sema/check/infer_return.rs`. Exported and recursive procs keep
    explicit returns.

### 5. Make the test suites fast enough to run routinely

The full native suite takes 20+ minutes and saturates the machine. Many
`tests/xsh/system-report.xsh` cases take 50–70 s each in debug builds.

- Rank the slowest tests from one full run's timings.
- Remove repeated whole-module-graph checking and subprocess re-parsing where a
  shared prepared module or an in-process call would do.
- Merge redundant system-report cases.
- Cap default `xsht test` parallelism at a reasonable share of cores.
- Killing `xsht test` currently leaves the `xsh` children it spawned (e.g.
  system-report subprocesses from `test.run_script`) running as orphans, each
  pinning a core. Run test children in the runner's process group and terminate
  them when the runner exits or is interrupted.
- Target: the full native suite under 5 minutes in debug, without dropping
  coverage.

### 6. Optional: monomorphic recursive private returns

Budget: about 400 lines.

- Today recursive private functions need an explicit return
  (`check.required-return`).
- Allow omission when a non-recursive completion fixes the type; keep
  rejecting cases that are genuinely underdetermined.
- Do it only if real sources want it after item 4.

## Orchestration

This campaign runs in Claude Code.

- **The primary session is the integrator.** It alone owns:
  - shared facades and registrations: `src/runtime/eval/lower.rs`,
    `src/sema/check.rs`, `src/runtime/eval/indexed/full.rs`, and the dispatch
    entry points;
  - canonical docs and this plan;
  - broad test gates and commits.
- **Implementation lanes use the `xsh-lane` agent** (`.claude/agents/xsh-lane.md`:
  Opus 5.5, medium effort).
  - Run up to 8 concurrently, each on one slice with an exclusive file set, a
    named gate and a stop condition.
  - Lanes that compile run with `isolation: "worktree"` and their own target
    directory. Remove each worktree after merging.
- **Routine work uses the `xsh-routine` agent** (`.claude/agents/xsh-routine.md`:
  Sonnet 5.5, medium effort). Prefer it whenever the work is economical to
  specify and verify, to keep cost down. This covers:
  - the scoped migrations in items 1 and 4b;
  - adding tests from an explicit list;
  - reference and doc-path updates;
  - searches and inventories.
- **Parallel start:** items 1, 2, 3c and 4a have disjoint owners. Then 3a → 3b →
  4b → 4c → 4d run in sequence, because they share the checker and lowerer.
- **Bound machine load.** Lanes never run the full native suite. They run
  targeted files and `xsht test tests/xsh/stdlib --jobs 2`, one test process at
  a time. Only the integrator runs the full suite, and only one full suite runs
  on the machine at a time. Docker/Linux verification is deferred to final
  verification.
- **Keep bookkeeping out of the repo.** Keep at most one Cargo process per
  target directory. Do not create tracking files, ledgers, or agent-written
  plans; the integrator keeps the ownership list in context.

## Process

- **Stay green.** Every commit keeps these passing:
  - `xsht test tests/xsh/stdlib`;
  - the ergonomics suites;
  - `statement-value-contracts.xsh`, except where item 4c rewrites it on
    purpose;
  - the read-only `make check` lint gate.
- **Run the full gates before an item lands.** That means the full native suite
  (`xsht test`, 20+ minutes; 1,783 passed, 0 failed, 37 skipped on the reset
  tree) and the Rust gates in `docs/TEST-MAP.md`.
- **Delete before adding.** A change that adds a path for an existing decision
  removes the old one in the same slice.
- **Specs and tests first.** Update `docs/SPEC-TYPING.md` / `docs/SPEC.md` first,
  and prefer native tests per `AGENTS.md`.
- **Mass edits are scoped.** They are automated rewrites limited to one named
  rule and the named directories, with a reviewed diff and the full native
  suite before commit. Never use whole-tree `xsht fmt`, `cargo fmt`, or
  unfiltered `xsht lint --fix`.

## Other observations

- The campaign's baseline profiling measured about 703 MB of retained checked
  facts for the native system-report module. That is a checker memory issue
  independent of this plan.
