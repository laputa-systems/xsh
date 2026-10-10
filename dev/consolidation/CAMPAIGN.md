# XSH Consolidation Campaign

Status: planned, not started (drafted 2026-10-10; the owner decided the
questions under "Decisions" the same day). Nothing in this document has been
implemented.

## Start condition and sequence

1. The compatibility campaign (`dev/compat/CAMPAIGN.md`) finishes its
   correctness work: parity, the native port, and harness retirement.
2. This campaign runs.
3. Compatibility performance work (utility throughput, speed-only native
   primitives, moving policy out of Rust) runs after this campaign closes, on
   the instruction set and value representation this campaign leaves behind.

The figures below were read from the tree on 2026-10-10 while compatibility
lanes were still landing. They size the work; they are not the baseline. The
baseline is measured at the start commit (see "Setup").

## Goal

The language design stays as it is. The implementation is brought to the
standard its own architecture document states: each semantic decision made
once, one executable form, one implementation of each instruction, and a
checker whose facts every later stage reads instead of re-deriving.

The campaign answers an external review of the repository. That review judged
the language philosophy sound and the internals not yet minimal, and set two
tests this plan adopts as its own:

- **Subtraction.** A change is an improvement when it removes code and states
  the compiler must reason about, not when it adds a better abstraction.
- **Structural agreement.** Two stages that must agree share one definition,
  so a disagreement fails to compile. A test that catches the disagreement
  afterwards is a stopgap, not the fix.

## Rules

Lane process, roles, and review follow the compatibility campaign unless a
rule below says otherwise.

**Roles and models.** One coordinating session, on its own model, is the
integrator and the only one that merges. Implementation lanes are `xsh-lane`
agents on Sonnet 5.5 at high effort. Routine lanes (scoped lint migrations,
pure file moves, test additions from an explicit list, inventories) are
`xsh-routine` agents on Haiku 5.5 at high effort, never Sonnet or Opus. Pass
`model` and `effort` explicitly on every spawn. A routine lane that meets a
design decision or a checker, lowering, or runtime change stops and reports;
the integrator re-issues the item as an `xsh-lane` item.

**Isolation.** Each lane works in its own worktree and branch over an
exclusive file set the integrator assigns. Shared paths (facades,
registrations, `docs/`, `dev/`, `Cargo.toml`, this directory) belong to the
integrator; a lane that needs one files a `Requests:` line. Most of this work
lands in a few large files, so isolation is scheduled, not free: Setup
partitions those files, and items that still collide run in series.

**Tests are the oracle.** No lane weakens, skips, deletes, or rewrites the
expected text of an existing test to pass. The only way a test stops counting
is an exact-ID entry with a reason in `dev/consolidation/exceptions.json`,
proposed by the integrator and approved by the owner.

**Acceptance.** The same mechanical gate accepts a lane and its merge:

1. Changed paths are inside the lane's file set.
2. The gate the item names is green on release binaries.
3. No test was removed or weakened, outside approved exceptions.
4. No ratchet rose, and the item's target metric reached its stated value.
5. Net non-test Rust lines are within the item's budget. The default budget
   is zero or negative: a lane that adds a path for an existing decision
   deletes the old path in the same slice.
6. Generated documentation is fresh when a contract changed.

A behavior-preserving item additionally changes nothing in
`docs/templates/SPEC.md`, the registry signatures, or any expected text in a
native test. A defect fix starts from a failing native test that stays in the
tree.

**No new surface.** No new syntax, keyword, stage, or standard-module entry
during the campaign, except the one spelling decision D4 allows. A gap a
lane finds is recorded in `TODO.md`.

**Performance boundary.** In scope: redundant work in the compiler and tools.
Out of scope: how fast any utility in `core/` runs, and any native primitive
added for speed. A lane that finds one records it for the later
compatibility performance phase.

**Contract changes** (APIs, types, diagnostics, SPEC wording, gate commands)
go to the owner before a lane starts. The integrator drafts each design;
lanes implement the approved result. Nothing is pushed.

## Planning figures

Each row is a ratchet: Setup records its value at the start commit, and the
value may only fall.

| Measure | 2026-10-10 | End state |
|---|---|---|
| Instructions whose behavior on operand values is written separately in each evaluator | not yet counted; 27 expression and 2 statement tags have an arm in both | 0 |
| Stream stages written both in the materializing pipeline arm and in `serial_pipeline.rs` | 17 of 46, not yet classified | 0 with separate per-item behavior |
| Places where the recursive evaluator starts a nested frame evaluator | more than 20 | 0 |
| Wildcard arms that hand an instruction from the frame evaluator to the recursive one | 2 | 0 |
| Hand-ordered payload reads in the executors (`indexed_raw`, `indexed_decode`, `indexed_finish`) | about 930 | 0 outside generated code |
| Definitions of the static type test | 3, with observable differences | 1 |
| Type-expression resolvers in lowering | 3 | 0 |
| Mappings between `Type` and `LoweredType` | 4 | 2 |
| Name-resolution models | 3 | 1 authority |
| `match` over arena kinds that names 8 or more variants and ends in a wildcard | 56 | 0 |
| Separate recursive descents in lint | at least 7 | 1 |
| Checks of a shared module in one `xsht lint` run | once per importing root, again per fix round | 1 |
| Whole-program copies made to lower one stage that names a callable | 1 | 0 |
| `xsht lint` on this repository | about 17 s against a 15 s budget | inside the budget, measured alone |
| Wall time of the full gate sequence | about 8 minutes on the 32-thread machine, before the two targets Setup adds | lower than the start value measured with those targets included |
| Lines under a blanket `allow(dead_code)` | about 15,800 | 0 |
| Entries under "Defects" in `TODO.md` | all open | closed, or moved out with a reason |
| Non-test Rust lines, `src/` and `crates/xsht/src` | 205,864 and 60,041 | lower |

## Setup

Run by the integrator, in series, before any workstream.

- **Baseline and ratchets.** One XSH program under `dev/` measures every row
  above and writes `dev/consolidation/baseline.json`; `cargo dev check` fails
  when a measure rises. Lane tooling for this campaign is XSH, not Python.
- **Lane gate.** `cargo dev` gains a command that maps changed paths to the
  rows of the gate table in `docs/TESTING.md` and runs them. The full
  sequence the integrator runs once per batch adds two targets the default
  test command omits today: the `xsht` integration target and the fixed-seed
  soundness fuzz.
- **Feature-touch probe.** On a scratch branch that is then discarded, add
  one inert expression kind and one inert instruction, and record two counts:
  the sites the compiler demands, and the sites that need an edit the
  compiler does not demand. The second count is the review's "coordinated
  changes" cost made measurable; it is taken again at close.
- **Partition the large files.** `lower.rs`, `indexed_run.rs`,
  `lowered_run.rs`, and `lint.rs` are divided by concern into sibling modules
  so that lanes can own files. Where a function is one large `match`
  (`eval_indexed_expr_inner`, `eval_lowered_module_call_values`,
  `lower_call`), arm bodies move into functions grouped by family and the
  dispatcher stays. These are pure moves with no behavior change, verified by
  the full gate and by the allocation counts of `xsht runtime-stats`.
  Afterwards, retry a release build of the root unit tests: the LLVM
  recursion that blocks it is likely tied to those functions.
- **Test build profile.** Add a Cargo profile for running tests: optimized,
  without LTO, with more codegen units (D10). `docs/TESTING.md`, `AGENTS.md`,
  and `release_bin!` change with it, since they name the release profile
  today. The root crate is not split.
- **Coverage before refactoring.** Measure Rust line coverage of the
  executor, lowering, and verifier files under the full gate
  (`cargo dev coverage`). An arm or decode path no test reaches gets a native
  test before any lane rewrites it; a behavior-preserving rewrite of
  unexercised code has no oracle.
- **Classify the paired arms.** For each instruction and stream stage with an
  arm in both evaluators, record whether the two arms share their value-level
  helper or carry separate behavior. This sets the first two rows of the
  table and sizes workstream 2.
- **Item list.** This document is a charter, not yet a lane plan. Once the
  files are partitioned, the integrator writes the items for each workstream
  with a file set, a gate, and a size budget, in `dev/consolidation/ITEMS.md`.
- **Dead code.** Remove the blanket `allow(dead_code)` over
  `src/runtime/eval/indexed.rs` and its children and delete what the compiler
  then reports.
- **Stale statements.** Correct the ones that would mislead a lane: the
  module comment of `lowered_run.rs` (it describes an evaluator and names
  functions that no longer exist), the benchmark section of
  `docs/TESTING.md` and `dev/bench.xsh` (they name a bench target that is not
  in the tree), and the line counts in `TODO.md`.
- **Skill and exceptions file.** Write
  `.claude/skills/xsh-consolidation-campaign/SKILL.md` from this document and
  create an empty `exceptions.json`.

## Workstreams

Each names the evidence, the end state, and the check that closes it. Symbol
names are search handles; line numbers are left out because they move.

### 1. One instruction table

**Evidence.** `impl_node_codec!` in `indexed/full.rs` already generates the
encoder and the per-tag verifier rule from one table entry per instruction.
The executor is outside it: it reads each payload by hand, in encode order,
so the field order of `ExprBinary` is known to one table entry and six
executor sites. The row enums (`BuildExprRow`, `BuildStmtRow`, ...) and the
tag enum `FullTag` are separate hand-written lists that happen to be
one-to-one. `instruction_effects` is a hand-written match that ends in a
wildcard. Return analysis exists twice, once per representation
(`lowered_body_can_return`, `indexed_stmt_can_return`). The executor
redeclares nine payload structs that lowering already declares
(`RunArg` beside `LoweredRunArg`, and so on). `lowered_module_op_supported`
lists 365 runtime operations by name and the executor match names 387; the
lists differ in 24 names. `tools/xsh-ir-coverage.xsh` reads the tag enum by
scraping Rust source.

**End state.** One table entry per instruction yields the row, the tag, the
encoder, the verifier rule, a typed decoder the executor calls, and the
effect bit. Pattern rows join the table. One list says which runtime
operations have a lowered body. The coverage tool reads a generated listing.
The encoding itself does not change.

**Closes when** the hand-ordered read count is zero outside generated code,
the duplicate return analysis and mirror structs are deleted, and paired
runs of the existing loop and call micro-workloads under `bench/` show no
regression.

### 2. One definition of each instruction's behavior

**Evidence.** Two evaluators run the same `FullProgram`: the frame evaluator
(`ExplicitFrames` in `explicit_run.rs`) and a recursive one
(`eval_indexed_expr_inner`, `eval_indexed_stmt_inner` in `indexed_run.rs`).
Each falls through to the other. Twenty-seven expression tags and two
statement tags have an arm in both, and seventeen stream stages exist both in
the materializing pipeline arm and in `serial_pipeline.rs`.

The paired arms are not copies. Each is a different way of evaluating
operands, scheduled on frames with continuations or in place by recursion,
around helpers that are only partly shared. `ExprBinary` and `ExprRecord`
share theirs (`lowered_binary_value`, `finish_record_entries`). `ExprField`
does not: the frame arm calls `lowered_record_field_value`, and the recursive
arm has its own match over value shapes with its own errors. Whether an
operand can be evaluated in place is a property of that operand, not of the
instruction's tag, and the frame evaluator already tests it per operand
(`leaf_operand`). Only three pairs have been read; Setup classifies the rest.

No test pins the pairs against each other, which invariant 2 of
`docs/ARCHITECTURE.md` requires of two routes that must agree. The open
defect "recursion through a stage block aborts at 100 to 150 levels" follows
from the structure: the recursive evaluator starts a nested frame evaluator
on the native stack.

**End state** (D1).

- An instruction's behavior on operand values is one function, called by
  whichever evaluator scheduled the operands.
- An operand is evaluated in place only when the verifier has marked that
  instruction instance as unable to run script code. The in-place evaluator
  therefore never starts a frame evaluator, and script recursion depth never
  becomes native stack depth.
- The frame evaluator hands an operand over by that mark, explicitly. Neither
  evaluator has a wildcard arm.
- A stream stage's per-item behavior is one function, whether the pipeline
  materializes or pulls one item at a time.

**Closes when** the three counts in the table and the wildcard count are
zero, the stage-block recursion limit is a `stack-overflow` diagnostic with a
native test, and `docs/ARCHITECTURE.md` describes the result. Follows
workstream 1.

### 3. One reading of a value and of a type

**Evidence.** The static type test exists three times
(`value_matches_static_type`, `lowered_value_matches_static_type`,
`test_value_matches_type`) and the copies disagree on streams, maps,
`EnvPathList`, modules, and errors. JSON encoding, display, type naming, and
error messages each exist once per value representation. Seven functions
resolve a type expression; three of them are in lowering, which states that
it "interprets binding annotations itself", and a debug-only drift recorder
plus a corpus test exist to catch lowering disagreeing with the checker.
Four functions map between `Type` and `LoweredType`, and the two reverse
mappings disagree on `Set` and `Tag`.

**End state.** One type test, written once over a view both representations
provide. The checker publishes the resolved type of every type expression as
a fact and lowering reads it, so lowering resolves none and the drift
recorder is deleted because nothing is left to drift. One mapping in each
direction between `Type` and `LoweredType`. Each of the paired operations has
one definition.

**Closes when** the three counts in the table reach their end state. Any
observable difference between today's copies is settled by the SPEC, with a
native test, before the copies merge. Follows workstreams 2 and 4.

### 4. Checker structure and the open defects

Each defect below is the visible end of a structural cause. The item fixes
the cause; the failing test for the defect comes first and stays.

| Defect (`TODO.md`) | Structural cause | End state |
|---|---|---|
| A `yield` in a stage block inside a stream producer passes the checker | Context that belongs to a callable boundary is a set of separate `Checker` fields, and each boundary saves and restores a subset chosen by hand | One boundary context value, pushed and popped whole at every boundary |
| A later `with` binding reads a top-level `const` of the same name | `PreparedConstants` keeps its own model of scopes, by span containment, and both checker and lowering consult it before lexical lookup | Constants read the checker's resolution; `SlotScope` reads the same binding identities |
| A typed call through a local, a field, or a call result is tied to its type only at run time | The IR records a declared callable type for parameters only | The row carries the callee's declared type and the verifier ties every callee |
| A module command other than `json.write` checks and then fails at run time | The command form of a module call has its own executor arm, written for one operation, with a wildcard that fails at run time for the rest | The command form binds its arguments and enters the ordinary module-call path |
| `xsht check`, the in-process check, and `xsht lint` disagree | Five entry points build the check from three option sets and two loaders; one diagnostic is raised during lowering preparation | One check-session constructor with named profiles and one loader |
| The compact declaration pass re-reports declaration diagnostics | `CompactDeclCollector` is a second declaration pass | It reads the checked declarations |

The remaining entries under "Language and checker" in `TODO.md` (named
arguments on an `Any` receiver, the optional-access message, an imported
enum printed as a path, unbounded source lines in diagnostics, buffered
output lost at `unix.exec`, positional `FsRoot.symlink`) are ordinary
fixes in the same file sets. The `Any` receiver entry disappears under
opaque `Any` (D4), which leaves no method call on an `Any` receiver.

**Closes when** every listed defect has a kept test and its cause is gone.

### 5. Traversal that cannot skip a node

**Evidence.** 409 `match` expressions range over arena statement and
expression kinds; 380 have a wildcard arm. Fifty-six of those name eight or
more variants: they are walkers, and a new kind passes through them
silently. Lint alone has at least seven separate recursive descents. A few
child enumerators exist (`grouping::for_each_child`, lint's
`expr_child_exprs` and `expr_child_blocks`) with one or two callers each,
and the first has a wildcard arm.

**End state.** One exhaustive child enumerator per node family (statement,
expression, pattern), defined beside the arena encoders, which are already
exhaustive. Every walker that only recurses calls it. A walker that owns
behavior for a kind keeps its own exhaustive match. This is enumeration, not
a visitor framework: no trait, no callbacks beyond the child closure.

**Closes when** the wide-wildcard count is zero, lint has one descent, and
the line in `docs/ARCHITECTURE.md` that says there is no generic visitor
describes the enumerators instead.

### 6. Lint as a rule table

**Evidence.** Lint is about 32,000 lines in `lint.rs` and 47 rule files.
There is no rule table: a rule is a function called by hand from one of six
places, and selection filters diagnostics after the fact. The rule list for
a program's top level and the list for a block are written separately and
differ. Checker facts are copied field by field twice on the way in. Ten or
more rules re-parse and re-check rewritten text through their own code.
`lint.prefer-fail` changes the kind an uncaught failure reports, which
breaks the rule in `docs/DESIGN.md` that an autofix never changes behavior.

**End state.** Each rule is declared once with its code, the scope it runs
at, and the facts it reads; one walker dispatches; the top-level and block
lists are one list. Rules read the check output directly. Probe re-checks go
through one helper. A fix that cannot prove it preserves behavior is not
offered and the lint explains the manual rewrite: `lint.prefer-fail` first.

**Closes when** the rule table drives `xsht lint --list`, the six call
places are one, and every entry under "Formatter and lint" in `TODO.md` is
closed. Any rule that starts or stops firing because the two lists merged is
a behavior change reported to the owner. Follows workstream 5.

### 7. Grammar and parser

**Evidence.** The parser reads the grammar's tables but restates every
sequence by hand, and agreement is held by seeded generation and by
recognition of the corpus. The three known disagreements are each a rule
written twice: the name after `..` in a list pattern, adjacency of a member
access after a stage, and a spaced trailing `?` inside a group. Four more
sentences failed in deeper runs.

**End state.** Each of those rules has one definition both sides read, in
the way operators, keywords, and stages already do. The seven cases are
permanent grammar tests. Rewriting the parser as a generated one is out of
scope.

One direction is checked today only on the corpus: that a source the parser
accepts is a sentence of the grammar. Two of the three known disagreements
are in that direction. The soundness fuzz gains that property for every
mutant that parses without diagnostics.

Agreement stays held by tests for every rule that is not shared. That is
weaker than the structural standard in "Goal", and it is the stated limit of
this workstream.

**Closes when** the "Grammar and parser" entries in `TODO.md` are closed and
a deeper generation run, at a depth and seed count recorded here at start,
finds no new disagreement.

### 8. Contracts: resources, `Any`, effects

The directions are decided (D3 to D6). Each still changes a contract: the
integrator drafts the detailed design, the owner approves it, and only then
do lanes start.

**Evidence.** SPEC 11.8 states one ownership rule and the implementation has
several. `ProcessHandle` and `NetJob` are tracked by owner scope and
released at exit. Streams are described as scope-owned but are not in the
owner tables; they are released by three other mechanisms. `FsRoot` and
`FsLock` are released only by `with`, `defer`, or by hand. `FsLock` is a
structural record, so any record with its three fields is accepted by
`fs.unlock`. Repeating a consuming operation has four different outcomes
across the types, and only one has a stated reason. Raw descriptors are
`Int`. `ManagedResource` lists two types. Two statements are wrong today:
SPEC 11.8 and 18 say there is no wait-any while `process.wait_any` is
registered, and the registry text for `process.spawn` describes lexical
ownership the code does not give. Consumption is never checked statically.

The split between released-at-exit and released-by-hand is the largest
difference and the corpus pays for it. Outside test files, `core/`, `dev/`,
`showcase/`, and Laputa open a root about 465 times and call `.close()` about
as often, and take a lock 8 times; none of those sites uses a `with` scope
(text counts, 2026-10-10). The reason the SPEC gives for the split is that
closing a root and releasing a lock can fail, not a host constraint.

`Any` can be navigated by field, index, slice, method, and iteration without
validation. Effects are static claims only, which SPEC 9.6 states. One gap
is inside that claim: every `linux` module call is charged `process`, so
`linux.socket` and `linux.connect` pass under `without net`.

**End state.**

- One resource table in the registry, extended from `ManagedResource`, that
  lists every runtime-owned type with its owner rule, consuming operations,
  release at scope exit, and repeat outcome. The checker, the runtime, and
  the SPEC section read it. A row that departs from the common rule states
  the host constraint that forces it.
- `FsLock` is an opaque type like the other handles.
- Repeating a consuming operation has one outcome for every type, with a
  process handle's `cancel` the stated exception.
- Streams enter the owner tables, so the code does what SPEC 11.8 says.
- `FsRoot` and `FsLock` are released when their owner scope exits, like the
  other handles, subject to the count D12 requires first. Every runtime-owned
  type then follows one rule, and raw descriptors are the one exception.
- Raw descriptors stay `Int`; the table states the exception and `TODO.md`
  records a typed descriptor as future work.
- The wait-any statements in the SPEC and the `process.spawn` registry text
  are corrected.
- Use after a consuming operation is a check error where ownership is
  visible, starting with straight-line code in one scope, as `TODO.md`
  describes.
- `Any` is opaque: `.require(T)` or a type pattern is the way in, with one
  greppable escape for schema-less code, and a migration lint with an
  autofix wherever the rewrite is provable.
- Socket operations in `linux` are charged `net`. Effects stay static
  claims; no runtime enforcement is added.
- Record and JSON exactness stays deferred.

**Closes when** each item is specified, built, tested, and migrated across
this repository and Laputa. Laputa is a separate repository: its migrations
are routine lanes there, one commit per rule, gated by its own test suite,
and they are the integrator's to schedule.

### 9. Redundant work

**Evidence.** The checker runs the whole bundle at least twice per check,
redoing bundle-wide preparation each pass, and one preparation step walks
the entire arena once per module. `xsht lint` checks the bundle once per
root and again in every fix round, so a shared module is checked many times;
about 37 s of thread time here. No cache of any kind exists. A stage that
names a callable copies the whole program and all checked facts to lower it;
the named-spread path had the same cost and already shares one copy. A
named spread scans the parameter list once per field. About 1,300 native
tests start a fresh process, while an in-process `run_script` exists and is
not wired to them.

**End state.** A module is checked once per workspace within one process,
and a fix round rechecks only the changed module and its importers. The
checker prepares a bundle once. Lowering a stage copies nothing. Parameter
lookup in a spread is by name. The limits listed in `TODO.md` are
diagnostics. An on-disk cache and in-process script tests are not part of
this campaign (D7, D8).

Checking a module once is the most invasive checker change in this document
and has no design yet. A module's check depends on its bundle today in four
ways: type definitions are indexes into the bundle's arena, effect inference
is one fixed point over every proc in the bundle, the type of the builtin
`args` follows the entry's `main`, and each entry point passes different
options. The integrator drafts the design and the owner approves it before
any lane starts.

**Closes when** the two counts and the lint time in the table reach their
end state and the 16,000-field spread has a native test with a time bound.
Follows workstream 4, which supplies the single check session.

### 10. Evidence and close

- **Feature-touch probe**, repeated. The end state is zero sites that need
  an edit the compiler does not demand in checking, lowering, verification,
  and execution. `docs/ARCHITECTURE.md`, "Adding a language feature", is
  rewritten from the result.
- **Reliability.** The fixed-seed soundness fuzz and the `xsht` integration
  target are part of the default gate. The report states cumulative fuzz
  seeds run during the campaign and the upstream compatibility results as
  evidence from an oracle this project did not write.
- **Comparative workloads.** A small pinned suite of glue workloads drawn
  from Laputa (startup, a spawn-heavy loop, a pipeline over a large text
  file, a directory walk, a JSON transform), run in the test image against
  dash, Bash, Nushell, YSH, Elvish, and Python at pinned versions, with the
  method and raw samples committed. This is measurement, published as it
  falls; nothing is tuned to it in this campaign.
- **Reconcile** `TODO.md`, `docs/ARCHITECTURE.md`, and `docs/TESTING.md`
  with what was built, and hand the measured state to the compatibility
  performance phase.

One part of the review cannot be met by engineering: independent users. The
campaign can make the claim checkable; it cannot supply adoption.

## Order

| Phase | Work | Runs |
|---|---|---|
| 0 | Setup | integrator, in series |
| 1 | Workstreams 1, 4, 5, 7 in parallel over disjoint files; designs for 8 drafted and decided | lanes |
| 2 | Workstream 2 after 1; workstream 6 after 5 | lanes |
| 3 | Workstreams 3 and 9; approved parts of 8 | lanes |
| 4 | Workstream 10 | integrator |

Workstreams 1 and 2 share the executor files and run in series. Workstream 3
touches lowering, the checker, and the executor, so it waits for the lanes
that hold those files.

## Decisions

Decided by the owner on 2026-10-10. D1 was decided twice: the first wording
was withdrawn and the corrected one approved the same day. D12 was added and
decided after the others.

| | Question | Decision |
|---|---|---|
| D1 | End state of the two evaluators | One function per instruction for its behavior on operand values; two ways of scheduling operands, on frames or in place; an in-place operand only where the verifier has marked that instruction instance as unable to run script code. Decided on the corrected proposal, after the first wording (a frame or leaf class per tag) proved impossible: `ExprBinary` is in place for some operands and scheduled for others. Not frames only |
| D2 | Shared child enumerators, reversing "no generic AST visitor" | Yes, as enumeration only |
| D3 | Resource table | `FsLock` becomes opaque; one outcome for a repeated consuming operation, `cancel` excepted; streams enter the owner tables |
| D4 | Affine checking and opaque `Any` during the freeze on new surface | Both are built. Affine checking starts without parameter syntax. The escape hatch for `Any` is the one new spelling |
| D5 | Raw descriptors | Stay `Int`, as a stated exception |
| D6 | Runtime enforcement of effects | None |
| D7 | On-disk checked-interface cache | Not in this campaign; in-process only |
| D8 | In-process script tests | Deferred; reconsider only if gate time is still the bottleneck after workstream 9 |
| D9 | Comparators for the workload suite | dash, Bash, Nushell, YSH, Elvish, Python |
| D10 | Test build profile; splitting the root crate | Profile yes; no crate split |
| D11 | Coordinator model | The session model |
| D12 | Whether `FsRoot` and `FsLock` are released when their owner scope exits, as process handles and network jobs are | Yes. Ownership moves outward on return and outer assignment as it does for the other handles, and `with` stays as the way to observe a failed release. A lint with an autofix removes the hand-written `.close()` and `fs.unlock` calls this makes redundant. Condition: the design draft first counts the sites where a root or lock is left open on purpose while only a `Path` from it escapes (a `fs.tempdir` root closed that way removes a directory the caller still uses). If that count is not near zero, the decision falls back to a check that every root and lock is released, escapes, or is bound by `with`, and returns to the owner |

Still open, each settled in its design draft before lanes start:

- the spelling of the `Any` escape hatch;
- the error a repeated consuming operation reports, and whether the existing
  kinds (`net-job-not-live`, `fs-root`, `fs-lock`, `ProcessError.Unknown`)
  remain as names for it;
- how a parameter says it borrows a handle or takes it, which affine checking
  needs once it goes past one scope and is not part of this campaign;
- the settings of the test build profile;
- the design for checking a module once (workstream 9);
- the site count that D12 is conditional on.

## Review traceability

| Review point | Workstream |
|---|---|
| Implementation too large for its core; no generic visitor; lowering and execution maintain a handwritten correspondence | 1, 2, 3, 5, 6, Setup |
| Resource ownership unsettled; `Any` too permissive; effects are not authority | 8 |
| Checker, grammar, and runtime disagree; `lint.prefer-fail` changes behavior | 4, 6, 7 |
| Repeated module checking, per-stage clone, quadratic wide records, lint over budget, build and test cost | 9, Setup |
| Maturity and optimality not demonstrated | 10 |
| Freeze on expressive surface | Rules |

## Handoff log

Empty. Dated entries are added at the top of this section once the campaign
starts.
