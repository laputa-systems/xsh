# XSH typing inference: plan

Status: replanned on 2026-10-02. Nothing is in progress. Each work item below
starts only on request, after its open decisions are settled.

## Where things stand

The first campaign (`f9cec20c`..`6f25e76d`) was reverted to the pre-campaign
tree `d6f09bc5`. The full implementation remains in history at `6f25e76d`. If
parameter generalization is ever reconsidered, its inference core
(`src/sema/inference/`, about 5k lines) is the only part worth consulting.

Why it was reverted, measured at `6f25e76d`:

- `src/` grew from 144k to 239k lines in three days. About 21k of the 95k added
  lines were tests.
- `tests/xsh/stdlib` dropped from 335 passing tests to 96. 21 whole modules
  failed preparation, so about 277 of 381 test declarations never ran. The
  ergonomics suites (`ergonomics.xsh`, `ergonomics-extended.xsh`) failed
  outright.
- No annotation was removed from any maintained program. The campaign's own
  inference tests passed 56 and failed 30. `pure double(x) { x * 2 }` failed
  during preparation.
- Most runtime additions verified checker output against copies of itself:
  "receipts", "original authority", and owner and rewind checks. That
  vocabulary grew from 35 lines in 7 files to 5,074 lines in 255 files. The
  checks rejected valid programs whose operation had no prepared protocol yet.
- The new solver ran inside the old checker through about 150 bridge sites. The
  legacy adapters it was meant to retire were never removed.

Kept from the campaign: native tests that pass under baseline semantics.

- `tests/xsh/statement-value-contracts.xsh` covers the statement, value and
  Result decision matrix.
- Extended `local-inference`, `named-argument-spreading`, `parametric-records`,
  `private-proc-effects`, `typed-causes`, `stdlib/json`, and the
  command, constructor and schema boundary tests.

## Lessons that constrain this plan

- **Inferring polymorphic parameter types is what made it large.** Unannotated
  parameters were given polymorphic types (rank-1 schemes, structural rows,
  operator requirements). That turned generic execution and evidence passing
  into runtime problems, and most of the growth followed from that.
- **Checker and runtime run in one process.** In-process proof checking between
  them has no adversary, and it failed valid programs.
- **One contract asked for too many things at once.** The previous contract
  combined HM inference, rows, effect variables, sealed requirements, a
  reference solver, post-disposal evidence verification, and 95%/85% removal
  targets. Each looked defensible alone; together they multiplied.
- **The removal targets measured the wrong corpus.**
  - 69% of the "eligible" annotations were in agent-written system-report code.
  - 509 of the 692 eligible effect clauses sit on `test` declarations. Those
    clauses exist only because `lint.unannotated-effects` demands them; the
    checker treats an unannotated test as unrestricted.
- **Work that is not green at each step does not converge.** Per-operation
  "protocols" grew with operations × execution routes, and the remaining-work
  list grew faster than it shrank.

## Goal

Remove annotations that buy nothing in ordinary XSH glue code. Change only the
existing checker and lint, not the runtime architecture.

The rule is the one `docs/SPEC-TYPING.md` already states: signatures are written
at declaration boundaries, and everything inside a body is inferred.
Annotations stay on:

- required parameters of named declarations;
- public contracts (exported returns and effects);
- schema and validation boundaries;
- recursive declarations, unless item 3 lands.

All inference stays local and monomorphic. The checker publishes concrete types,
so lowering and both indexed executors need no generic evidence.

The baseline already infers:

- locals, including empty and null locals solved by later use;
- default parameter types;
- `.require()` targets;
- generic record constructor arguments;
- private pure returns;
- private proc effects.

`statement-value-contracts.xsh`, `local-inference.xsh`,
`private-proc-effects.xsh`, `default-parameters.xsh` and
`parametric-records.xsh` pin that behavior.

## Non-goals

- Inferring required parameter types, polymorphic generalization of
  unannotated declarations, row polymorphism, effect variables, or
  typeclass-like operation requirements.
- Runtime evidence passing, prepared generic execution, or checking checker
  output beyond the existing structural IR verifier.
- Inferred public contracts.
- Reference solvers, frozen annotation denominators, operation inventories,
  benchmark cohorts, or status ledgers.

## Work items

Each item is one vertical slice with a size budget. It lands green, with spec,
native tests, checker, lint and docs together. If an item exceeds twice its
budget, stop and re-plan rather than pushing through.

### 1. Stop the lint from demanding redundant annotations

Budget: about 100 lines of Rust plus a scoped source edit.

- Restrict `lint.unannotated-effects` and `lint.missing-effects`
  (`crates/xsht/src/lint.rs::lint_effect_annotation`) so they do not fire on
  `test` declarations. Those entries are already unrestricted. Clauses that
  are present stay upper bounds.
- Then delete the redundant effect clauses under `tests/`, `core/tests/`,
  `dev/tests/` and `showcase/tests/`. Because this is a large mechanical edit,
  it needs explicit approval of the method.
- Open: should CLI `main`, conventional `main` and `stream` entries get the
  same treatment?

### 2. Infer private proc success types that are currently errors

Budget: about 600 lines including tests.

- Today an unannotated private proc whose tail is, for example, `Int` fails
  with `expected Unit, found Int`.
- Proposed rule:
  - Tails that already have statement meaning keep it: `Unit`,
    `Result[Unit]`, `Bool` (asserts) and `Status`.
  - Any other tail type becomes the inferred success type `T`, and the proc
    returns `Result[T]`.
- Expression tails of these other types are rejected today, so no migration is
  needed for them. A proc returning `Bool` data still writes `-> Bool`.
- Value-producing `if`/`match` tails are the one compatible-but-visible change.
  Today they act as statements and discard their branch values (probe:
  `proc h(x: Int) { if x > 0 { x } else { 0 } }` checks). Under this rule they
  become values. Callers are unaffected, because a non-Unit result in statement
  position is discarded. Pin this case in the spec and in
  `statement-value-contracts.xsh`.
- Reuse the private pure return path (`src/sema/check/infer_return.rs`).
  Exported and recursive procs keep explicit returns.
- Open: accept the `Bool`/`Status` irregularity, or make value tails uniformly
  data with a one-time migration? The first campaign found 40 sites that would
  need `-> Result[Unit]`.

### 3. Optional: monomorphic recursive private returns

Budget: about 400 lines.

- Today recursive private functions need an explicit return
  (`check.required-return`).
- Allow omission when a non-recursive completion fixes the type, and keep
  rejecting cases that are genuinely underdetermined.
- This is rare in glue code. Do it only if real sources want it.

### 4. Optional: explicit generic functions

Budget to be decided; requires a design note first.

- Declared, rank-1 type parameters, for example `pure first[T](items:
  List[T]) -> T?`, instantiated per call by the checker.
- Bodies use `T` only in value-generic ways, so uniform tagged runtime values
  suffice and no evidence is needed. Inference of parameter types stays out of
  scope.
- Do it only if maintained code shows real demand.

## Pre-existing duplication that multiplies typing cost

These predate the campaign. Every typing feature currently has to be
implemented in several places, and the campaign's growth was partly this
multiplier:

- Two indexed executors run over the same IR:
  `lowered_run/indexed_run.rs` (10.1k lines) and
  `indexed_run/explicit_run.rs` (4.5k).
- The compact checker probe (`src/sema/check/compact.rs`, 2.9k) re-checks
  bodies alongside the full checker.
- `src/runtime/eval/lower.rs` (15.8k) re-derives types and overloads; there are
  117 `infer_*`/`infer_checked_*` uses.
- Checked types are stored both in span-keyed `CheckOutput` maps and in
  ExprId-keyed compact facts.

Consolidating any of these is its own task with its own budget. Doing one before
items 2–4 makes each later typing change cheaper. Whether and which one to do is
open.

## Process

- **Stay green.** Every commit keeps `xsht test tests/xsh/stdlib`, the
  ergonomics suites and `statement-value-contracts.xsh` green. Run the full
  native suite (`xsht test`) and the Rust gates in `docs/TEST-MAP.md` before an
  item lands.
- **Delete before adding.** A change that adds a path for an existing decision
  removes the old one in the same slice.
- **Specs and tests first.** Update `docs/SPEC-TYPING.md` first, and prefer
  native tests per `AGENTS.md`.
- **One owner per item.** Do not fan out parallel lanes on the checker. Keep
  bookkeeping to git history and the test suite.

## Known issues to check separately

- The first campaign's handoff reported that
  `tests/runtime/modules.rs::LocalHttpsHttp2Server` can block in `accept()`
  after its client fails. Confirm whether this happens on this tree, and bound
  the server's cleanup.
- The campaign's baseline profiling measured about 703 MB of retained checked
  facts for the native system-report module. That is a checker memory issue
  independent of inference.
- `"/opt/x" in env.PATH` passes the checker but is always false at runtime; the
  `p"/opt/x"` form works. `env.PATH.append("/opt/x")?` also checks, then fails
  at runtime with "expected Path, found Str". A likely cause is
  `is_path_like_arena_expr` in `src/sema/check/expr.rs` accepting a Str
  literal.
- Parametric record mismatches report "expected Record, found Record" instead of
  the type applications. `error Failure[T] = …` is rejected as
  `check.unresolved-proc-command` rather than with a generic-error-family
  diagnostic.
