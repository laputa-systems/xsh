# Dead-Code Lint Plan

Status: design plan. The current `lint.dead-code` rule is a deliberately small
first slice. It recognizes statements after an unconditional `return`, `break`,
or `continue`, and statements after an `if` or `match` whose branches all
return. It is not yet a complete control-flow or symbol-reachability analysis.

## Contract

Dead code is source that cannot execute on any valid path from an entry point of
its enclosing body. The detector must be proof-oriented: when control-flow or
dynamic dispatch is unknown, it must remain silent rather than guess.

Dead code is distinct from an unused value:

- `lint.dead-code` concerns unreachable execution.
- `lint.unused-local` concerns a binding that is never read.
- `lint.unused-type` concerns an unreferenced type declaration.
- Future unused-proc or unused-module diagnostics need a separate module and
  entry-point analysis; they must not be smuggled into local binding warnings.

The diagnostic remains a warning, uses the existing `lint.dead-code` code, and
has no automatic fix until source comments, deferred effects, and declaration
boundaries can be preserved safely. A dead-code warning must not be emitted
merely because an earlier operation can fail: `run`, `?`, `with`, `guard`, and
fallible module calls may still have a successful path.

## Analysis model

Replace the current last-statement predicates in `crates/xsht/src/lint.rs` with
a small control-flow summary for every statement and block. The summary should
preserve at least these exits:

- fall-through to the next statement;
- return from the enclosing proc or pure function;
- break from the nearest loop;
- continue in the nearest loop; and
- divergence, for a proven infinite path.

Runtime failure and cancellation are not ordinary dead-code exits. They should
be modeled only where the language guarantees that no normal path exists. A
possible error is not enough to make following source unreachable.

Analyze a sequence by joining its predecessor exits. Once the fall-through bit
is absent, report the next source statement as unreachable while still visiting
it for independent diagnostics. Join branch summaries rather than copying the
current `all arms return` check into each construct.

The summary must be context-aware for loop control. A `break` exits the current
loop and makes the remainder of that loop body unreachable, but it does not by
itself make code after the loop unreachable. A `continue` has the analogous
relationship to the loop body. `yield` and `defer` do not terminate the current
path; deferred expressions still execute while a block unwinds and must be
visited for diagnostics.

## Construct coverage

Implement the flow rules in stages, with each stage adding positive and negative
tests.

1. Sequences and unconditional exits: `return`, direct `break`, and direct
   `continue`, including the conditional `return when/unless`, `break
   when/unless`, and `continue when/unless` forms represented by
   `GuardedStmt`. Conditional forms retain a fall-through path.
2. Branches: `if` without `else` retains a fall-through path; `if` with `else`
   joins every branch. `match` joins every arm and accounts for guarded arms and
   the runtime `match-no-arm` path. Exhaustiveness facts from the checker should
   be reused instead of inferred a second time in the linter.
3. Loops: `while` and `for` may execute zero times, so their bodies do not make
   later code unreachable. `loop` executes at least once; prove divergence only
   when the body has no fall-through or loop exit and no other exit remains.
4. Structured control flow: model `with`, `guard`, `retry`, builder/task blocks,
   stream producer bodies, and signal-hook bodies as separate entry regions.
   Their failure and cleanup paths must not be confused with ordinary
   fall-through.
5. Known terminating APIs: only add calls such as `abort(...)` after a stable
   registry-level contract proves that the call cannot return. Do not infer this
   from names, effects, or an arbitrary user proc body.

## Declaration reachability

After statement reachability is sound, add a separate symbol graph for dead
declarations. The graph must be built across the checked program bundle, not one
source file at a time.

Roots include the script entry (`main` and implicit top-level execution), exported
module declarations, signal hooks, and native-test entry procs. A declaration
that is exported or reachable through a dynamic/`Any` boundary must be treated
as live unless the language later gains an explicit closed-world mode.

The first declaration candidates should be unexported `proc`, `pure`, and
`stream` definitions. Top-level values and imports need an effect model first:
an unread binding may still perform initialization, and importing a module may
run its initialization. Type declarations remain owned by the existing
`lint.unused-type` analysis until the graph can distinguish type-only use from
runtime reachability.

Do not report native test procs, implicit entry functions, exported API, or
module-contract declarations as unused. Preserve source order and module names
in diagnostics so a multi-file report is deterministic.

## Diagnostic and tooling decisions

- Keep `lint.dead-code` as the stable code for unreachable statements and use a
  precise message/label for the unreachable construct.
- Prefer one diagnostic per contiguous unreachable region unless a later
  machine-readable format needs statement-level records.
- Keep analysis-only behavior separate from `xsht lint --fix`; no deletion fix is
  sound until comments, `defer`, and effectful initializers have explicit rules.
- Run the detector after parsing and checking when checker facts are available,
  but continue to render independent diagnostics after recovery.
- If a future CLI mode distinguishes reachability from unused declarations, add
  that mode only after the library analysis has a stable contract; do not add a
  repository-specific command or selector.

## Test plan

The nearest owner is `crates/xsht/src/lint.rs`; keep focused behavior coverage in
`crates/xsht/tests/lint.rs` and CLI rendering/status coverage in the XSHT
integration tests.

Required cases:

- statements after `return`, `break`, and `continue`;
- conditional exits that remain reachable;
- all-returning and partially returning `if` and `match` branches;
- guarded match arms and non-exhaustive matches;
- zero-iteration loops, loop exits, nested loops, and proven divergence;
- `?`, fallible runs, `with`, `guard`, `retry`, `defer`, `yield`, and `abort`;
- stream/task blocks and signal hooks as independent regions;
- deterministic ordering, contiguous-region behavior, and no duplicate reports;
- exported, dynamic, test, entry, and unexported declarations across modules;
- comments and formatting stability when diagnostics have no fixes.

Start with the focused lint tests, then run the XSHT integration gate and the
runnable XSH corpus gate from `docs/TEST-MAP.md`. The canonical language and
diagnostic contract belongs in `docs/SPEC.md` when each stage is implemented;
this file remains the design and sequencing record.

## Delivery sequence

1. Freeze the terminology and soundness rules in this document.
2. Introduce the flow-summary types and replace the provisional predicates
   without changing the diagnostic code.
3. Add construct coverage and negative tests until the statement analysis is
   sound for all current AST statement forms.
4. Add checker-assisted loop, match, and known-terminator facts.
5. Build the cross-module declaration graph and add unused-proc diagnostics as a
   separately specified lint category.
6. Update `docs/SPEC.md`, `docs/XSHT.md`, test-map references, and any generated
   help only when the corresponding behavior is implemented.
7. Consider guarded autofixes only after the analysis and source-edit invariants
   are proven by fixtures.

## Completion criteria

The detector is ready to be called complete only when every current control-flow
construct has an explicit flow rule, every warning class has a soundness test and
a false-positive test, module entry points and dynamic boundaries are covered,
and the full relevant test gates pass without repository-specific CLI flags.
