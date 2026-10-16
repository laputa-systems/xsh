# XSH Consolidation Campaign

Status: active and incomplete at main commit
`7b1485310abdf2190ce4532d840114e74ff7c2cd`, tree
`0f3394902504cbe5271edd8916f504bd493f091a`.

The full canonical native run on main source `7b1485310abdf2190ce4532d840114e74ff7c2cd`
terminated with receipt status `failed-or-incomplete`, exit 3. Cleanup resolved,
and the source was clean through the end. The native suite reports 13,902
passed, 3 failed, and 78 skipped. Root Rust (747 passed, 3 ignored), root
integration (223 passed), XSHT (116 passed, 5 filtered), and soundness (14
passed) stages passed.

The three native failures are GNU rm ext3perf (exit 3, empty stderr; failure
entry at stdout line 9,121), uu-sort `buffer_sizes` (timeout at 4 minutes,
canceled with signal 15), and PCI capture identity links (EAGAIN, errno 11, 9
ms; failure entry at stdout line 15,061). The run does not establish that the
GNU rm failure was a timeout. Default-product restoration remains available as
a separate step for downstream focused gates and does not change the failed
canonical result. Three hostname cases pass on source-identical fix
`63734367c`. The source check passes on this tree:
`blanket_dead_code_lines=0`, `lowering_type_resolver_definitions=0`,
`manual_payload_calls=0`, `optional_raw_calls=0`, and
`wide_arena_wildcards=0`; counts are 152 instructions, 17 patterns, 46 stages,
5 factories, 1 predicate, and 2 storage mappings. The 268,495-line source
count is advisory.

W9 final parity is not qualified. The 7b attempt stopped at helper compile
error E0061. Isolated repair commits `f1e1d08` and `8932aaed` have reviewed
reference, candidate, and derived builds (`fresh=false`), three passing
structured fixtures, and 270 passing Python tests; integration, fresh capture
of 1,690 inputs, and comparison of 6,760 outcomes remain pending. The
profile-cost fallback cleanup `322889` is isolated. Correctness-input plan
`8dae13e` is on HOLD pending owner approval because the original ignored
manifest has not been recovered.

Isolated inference pipeline `7a7418b5` passed 17 Rust and 20 native selectors;
capture source `46ce549a` passed its Rust selectors. Root accepted the isolated
Arc storage change after source, evidence, and ownership review. Its capture
integration worktree now points to `e4d282457672a1fc87464c051820ebb64a9eb857`,
tree `aa7eda1ae791a622cf831937a3f45cb2f94c3a02`; 16 candidate controls and 5
matched pairs pass with lower candidate allocation traffic. It remains off
main and distinct from the generic inference pipeline. No time, RSS, or general
performance result is established. Evidence commit
`cfcdd2737a941468d5d680d4e2418f289c0065bf` records the checks and allocation
pairs in `dev/consolidation/evidence/typing-capture-memory-e4/`. The fast-forward
adds no new runtime pass. Consolidation has no functional close or performance
credit. Lean and Phase F remain paused. See the [current handoff](HANDOFF.md)
for scoped evidence and the remaining gates.

## Owner steering, 2026-10-15

The owner adopted [the typing and formal successor requirements](#typing-and-formal-successor-requirements)
as the direction for resumed work. Winddown and cleanup are complete. The owner resumed consolidation and typing
after the documentation refresh. Lean remains paused.

Judge these outcomes separately:

1. Close the existing behavior-preserving consolidation acceptance on a
   qualified source freeze. Retain failed receipts and the original-source
   AWK/curl reproducer. Its cross-file fixture mutation explains the failure
   mechanism without reconstructing missing historical child outputs.
2. Deliver general rank-1 inference in a successor milestone. Establish
   red/green capability witnesses before solver changes: reusable function
   schemes, independent importer instantiation, open record rows, recursive
   components, mutable-state restrictions, and callable/effect relationships.
   Preserve checked facts, verifier-owned execution, and current contracts.
   Measure joint annotation removal on a frozen real-program cohort and cold
   solver phases, memory, and scaling. The 95% eligible-local and 85%
   eligible-internal targets remain targets, not achieved results.
3. Resume Lean toward one connected compiler slice from lexical resolution
   through constraints, schemes, elaboration, verified indexed instructions,
   and observable execution. State each stage's domain and assumptions.
   Distinguish universal theorems, finite certificates, empirical comparisons,
   and host assumptions. Preserve the full-core obligations and require the
   post-consolidation source freeze before claiming Rust correspondence.

Use the existing acceptance owners and pinned x86_64 Linux musl environment.
General inference changes language acceptance. Its isolated implementation may
overlap consolidation under the owner's 2026-10-15 and 2026-10-16 steering,
including independent lowerer work without waiting for consolidation close.
Shared integration, convergence acceptance, and the post-consolidation
performance baseline retain their gates. Do not put inference implementation
on main before consolidation acceptance closes.
Do not add another language-surface campaign or expand consolidation's scope.
The [restart handoff](HANDOFF.md) owns the current stop and remaining gates.

The implementation and focused
repairs are integrated on `master`. The 31c selected-profile native suite
passes. Its canonical batch stopped on a btop PTY assertion. The latest
test-only repairs require the expected screen state and a completed frame
after each input or resize. Focused regressions pass. The observer can reuse
a separate Cargo cache while retaining fresh receipts and the complete gate.
The exact 197-file desugar comparison also passes after the earlier root
permission and six-digit PID repairs. The later original-source AWK/curl replay
establishes the cross-file mechanism. Historical child output remains missing,
and final acceptance is still open. Fresh W9 parity on `2083821ed` passed
1,690 inputs and 6,760 outcomes with zero mismatches. Independent review
qualifies that result across the route-witness repair `ae7cf8490`.
The earlier selected-profile native suite and clean 13.65-second lint pass
remain bound to 208. Its canonical Rust batch found the now-repaired libc
hygiene violation. Final acceptance must run on the new freeze.
The 88 native attempt exposed a closed-descriptor test race. The assertion now
runs in an isolated child, retaining its exact errno contract. Focused checks pass.
The current acceptance and remaining gates are in [`HANDOFF.md`](HANDOFF.md).
The 122 selected-profile suite passes. Its canonical batch exposed a collect
rewrite separator bug and a timed lint test running alongside corpus work.
Both repairs are integrated with focused regressions and manual corpus
migrations. Acceptance must resume on the repaired freeze.
The owner approved the entire campaign and all six designs on 2026-10-11,
including the Codex model policy and completeness-first sequence. The start
commit remains `5e66b6b896a47d0c692739135d51200b007a2a67`. The owner moved
integration to `master` on 2026-10-11. Final launch pins must bind the new
functional-close record before acceptance runs start.
Implementation and gate evidence are tracked in `ITEMS.md` and the handoff log.

The campaign runs unattended: once started it never waits for the owner.
Every question it would otherwise ask is answered here or in a design file
before the affected item starts, or it is parked under the rules in
"Unattended operation".

## Start condition and sequence

1. This campaign starts Setup from a recorded commit with green consolidation
   gates. Completion of compatibility correctness work (parity, the native
   port, frozen-origin accounting, and harness retirement) is not a
   prerequisite. Record its current evidence and unfinished work alongside
   the consolidation baseline.
2. The campaign began on a separate `consolidation` branch. The owner moved
   integration to `master` on 2026-10-11; current approved work continues
   there. Compatibility correctness work may continue on a separate branch
   and worktree. Changes between campaigns are integrated explicitly at batch
   boundaries, with design assumptions revalidated and affected gates rerun; a
   moving compatibility head never silently replaces the consolidation
   baseline.
3. Complete the consolidation behavior and structural end states first.
   Stricter performance gates follow functional completion. Compatibility
   performance work (utility throughput, speed-only native primitives, moving
   policy out of Rust) runs after both consolidation and compatibility
   correctness close, on the instruction set and value representation this
   campaign leaves behind.

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

This campaign's lane process, roles, and review are defined by the rules
below. It sets its own model policy, integration branch, scheduling, and
gates; verification follows `docs/TESTING.md`.

**Roles and models.** One coordinating session, on its own model, is the
integrator and the only one that merges. `xsh-lane` and `xsh-routine` name
roles; Codex receives their instructions in the lane brief rather than
relying on Claude agent definitions.

| Role | Codex model | Reasoning effort |
|---|---|---|
| Routine: scoped migrations, pure file moves, tests from an explicit list, inventories | `gpt-6-luna` | `xhigh` |
| Implementation: checker, lowering, verifier, runtime, ownership, or other complex changes | `gpt-6-luna` | `xhigh` |
| Review: contract, invariant, and integration review of complex changes | `gpt-6-luna` | `xhigh` |

On resume the owner changed every Codex lane to `gpt-6-luna` at `xhigh` effort.
This overrides the previous Sol-only policy, including the campaign skill.
Pass `model`
and `reasoning_effort` explicitly on every spawn. With the session's
`spawn_agent` tool, use `fork_turns: "none"` for these explicit settings and
provide a self-contained brief. Lanes report unapproved contract decisions to
the integrator. This overrides the compatibility campaign's Codex model policy.

Claude Code sessions retain `xsh-lane` on Sonnet 5.5 at high effort and
`xsh-routine` on Haiku 5.5 at high effort, with `model` and `effort` explicit.
The Codex model and effort choices are supported by the
[OpenAI model documentation](https://developers.openai.com/api/docs/models/gpt-6.1-sol);
the delegation mechanics follow the
[Codex subagent guidance](https://learn.chatgpt.com/docs/agent-configuration/subagents).

**Parallel scheduling.** Use as many available agent slots as ready,
independent items permit. There is no fixed four-lane cap or barrier between
whole workstreams. Setup creates a dependency graph and exclusive file sets;
the integrator immediately fills a free slot with the next ready item,
prioritizing items that unblock others. Read-only inventories can run during
Setup; editing lanes start after their ownership and prerequisites are set.
Independent review can overlap implementation on other files. Lanes do not
delegate further unless the integrator assigns an explicit nested scope.

Agent concurrency and machine load are separate budgets. Queue compilation
and linking according to host memory, and run only one full native suite at
a time across both campaigns. Give Rust lanes separate build directories;
script-only and inventory lanes reuse binaries from their recorded base.
Run timing measurements alone. A lane waiting for a build slot does not
prevent another agent from doing independent code or review work.

**Lane brief.** Each spawn names the worktree, branch and base commit, exact
owned paths, approved design and item, observable end state, structural
metric and size budget, exact gates, and model and effort. It states whether
the lane may commit, forbids merging or pushing, and requires a report under
200 words with changed behavior, verification, metrics, blockers, and
`Requests:` for shared paths. The integrator reviews the diff and accepts
gates before merging; agent completion alone is not acceptance.

**Isolation.** Each lane works in its own worktree and branch over an
exclusive file set the integrator assigns. Shared paths (facades,
registrations, `docs/`, `dev/`, `Cargo.toml`, this directory) belong to the
integrator; a lane that needs one files a `Requests:` line. Most of this work
lands in a few large files, so isolation is scheduled, not free: Setup
partitions those files, and items that still collide run in series.

**Tests are the oracle.** No lane weakens, skips, or deletes coverage to
make a patch pass. On 2026-10-11 the owner explicitly approved updating
existing expectations that contradict the newly approved contracts, while
retaining their coverage and recording every migration in the handoff log.
The concrete starting conflicts are listed in `ITEMS.md`; each replacement
asserts the approved behavior. Other suspected incorrect tests are reported
with evidence. Design-specified migrations remain authorized.

**Acceptance.** The same mechanical gate accepts a lane and its merge:

1. Changed paths are inside the lane's file set.
2. The gate the item names is green on release binaries.
3. No test was removed or weakened.
4. No structural ratchet rose, and the item's functional and structural
   targets reached their stated values. Performance measurements are reported
   under the performance policy below.
5. Report exact non-test Rust and touched-line deltas. Source budgets are
   advisory under the owner update; remove replaced decisions and assess
   performance without using line count as a substitute.
6. Generated documentation is fresh when a contract changed.

A behavior-preserving item additionally changes nothing in
`docs/templates/SPEC.md`, the registry signatures, or any expected text in a
native test. One in the checker, the loader, or lint must also leave the
output of `xsht check` and `xsht lint` over this repository and Laputa
byte-identical to the output of the binaries built from the batch's base. A
defect fix starts from a failing native test that stays in the tree.

**No new surface.** No new syntax, keyword, stage, or standard-module entry
during the campaign, with no exception. A gap a lane finds is recorded in
`TODO.md`.

**Performance boundary.** In scope: redundant work in the compiler and tools.
Out of scope: how fast any utility in `core/` runs, and any native primitive
added for speed. A lane that finds one records it for the later
compatibility performance phase.

**Completeness before stricter performance gates.** Correctness, structural
agreement, resource and stack bounds, and existing regression tests remain
acceptance gates. Record latency, allocation, build time, and gate wall time
as evidence, but do not add tighter performance thresholds or park otherwise
complete items for noisy timing samples. Functional completion of
workstreams 1 to 9 comes first; stricter performance gates and tuning follow
on the completed implementation. This does not authorize weakening an
existing test or removing a safety bound.

**Contract changes.** Every change this campaign makes to an API, a type, a
diagnostic, SPEC wording, or a gate command is written down before the
affected item starts, in "Decisions" and in the design files. Approving those
approves the changes.
A lane makes no other contract change. One that turns out to be needed parks
the item. Nothing is pushed.

## Unattended operation

- **Branch.** The owner moved integration to `master` on 2026-10-11. The
  integrator owns `master`; isolated lanes merge there at accepted batch
  boundaries. Nothing is pushed. Laputa migrations use their owner-selected
  branch and remain separate from XSH integration.
- **Revalidate the designs.** Each design file lists the facts about the code
  it relies on, as symbols and behaviors a search can confirm. Before a
  workstream starts, the integrator checks those facts at its recorded base
  commit. It repeats the check after integrating a change from compatibility.
  A design whose facts no longer hold is not repaired: its workstream is
  parked whole.
- **Park, never ask.** An item is parked, with its evidence, when it would
  need a contract change that was not approved, when an unapproved test
  correction is needed, when it reaches twice its size budget, or when its gate cannot be
  made green. The lane's branch is kept and the worktree removed. Work that
  does not depend on a parked item continues. Three parked items in one
  workstream park the rest of that workstream.
- **Batches.** After each batch of merges the integrator runs the full gate
  sequence. If it fails, the integrator finds the merge responsible, reverts
  it, parks that item, and runs the sequence again.
- **Halt.** The campaign stops and writes its report when the gate is not
  green at the start commit, when the branch cannot be returned to green by
  reverting the last batch, or when no unparked item remains.
- **Resumable.** The integrator adds a dated entry to "Handoff log" after
  every batch: what merged, the ratchet values, what is parked and why. A new
  session resumes from that entry alone.
- **Close.** The report lists each workstream as closed or parked with its
  closing check, the start and end value of every ratchet, and each parked
  item with the decision it needs.

## Owner update — source budgets, 2026-10-11

The owner relaxed source budgets and made performance the priority. Per-item
production-line and touched-line limits, temporary source ceilings, and final
strict source shrink are now advisory measurements rather than acceptance
blockers. Record exact source deltas and review volume; preserve original
baseline artifacts. This supersedes older hard source-budget language below
and in item grants. Structural end states, correctness, ownership contracts,
documentation, platform gates and performance verification remain required.
Remove genuine duplication and unused code; do not compress layout to improve
counts. A passing source count is not performance evidence.

## Owner update — continue on master, 2026-10-11

The owner merged the completed btop work into master and requested merging
consolidation back into master, then continuing this campaign there. Master's
`58d321d9` and consolidation's `a376c097` are merged as `e4261855` in
`/home/josh/d/laputa-systems/xsh`. This supersedes older integration-branch
instructions. The integrator owns master; private lanes stay isolated and
retain their exact baselines and artifacts. No push is authorized.

## Owner update — platform scope, 2026-10-11

ARM64 and macOS are outside this campaign. Development and final platform
verification use host-native x86_64 Linux musl in the pinned `Dockerfile.test`
environment. This supersedes all older ARM final-verification requirements;
Linux correctness and performance gates remain required. Preserve historical
ARM artifacts and remove only campaign-owned runtime registrations at cleanup.

## Owner update — full native test throughput, 2026-10-11

The owner added parallelizing the complete `xsht test` suite and repairing
any test pollution that obstructs it, with at least a fourfold reduction in
wall time. This explicitly extends the earlier test-runner exclusions and
performance sequence. Compare the full unchanged test manifest using the
same pinned x86_64 Linux image, compiler profile and CPU/memory allocation;
record current default-run baseline and candidate elapsed time, outcomes,
worker count, source/binary hashes and raw logs. Merely selecting a slow
single-worker baseline does not establish the requested speedup. Preserve
per-test isolation, cancellation, timeout, cleanup, coverage and reporting
contracts. Coordinate full-suite runs and timings with the integrator so
only one full native suite runs at once and campaign builds do not compete.

## Owner update — throughput acceptance, 2026-10-12

The owner clarified that a fourfold full native-suite speedup is aspirational,
not an acceptance threshold. Performance must not regress; further
optimizations can follow once this campaign is settled. This supersedes the
hard fourfold requirement in the earlier full-native-throughput update.
Reliable parallel execution, isolation, cancellation, cleanup, coverage and
reporting remain required. Use comparable corpus and resource allocation for
nonregression evidence; preserve failed and incomplete run qualifications.
Do not prolong functional closure to pursue optional speed tuning.

## Planning figures

Setup records every row at the start commit. Structural counts are ratchets
whose values may only fall. Timing measurements are evidence for the later
performance gates; they do not introduce new acceptance thresholds here.

| Measure | 2026-10-10 | End state |
|---|---|---|
| Instructions whose behavior on operand values differs in substance between the two evaluators | 5 kinds, of 27 expression and 2 statement tags with an arm in both | 0 |
| Stream stages with separate per-item code in the materializing pipeline arm and in `serial_pipeline.rs` | 17 of 46 | 0 |
| Places that start a nested frame machine on the native stack | about 20 kinds of site | only those `designs/evaluator.md` leaves, each bounded by a counter |
| Wildcard arms that hand an instruction from the frame evaluator to the recursive one | 2 | 0 |
| Hand-ordered payload reads in the executors (`indexed_raw`, `indexed_decode`, `indexed_finish`) | about 930 | 0 outside generated code |
| Definitions of the static type test | 3, with observable differences | 1 |
| Type-expression resolvers in lowering | 3 | 0 |
| Mappings between `Type` and `LoweredType` | 4 | 2 |
| Name-resolution models | 3 | 1 authority |
| `match` over arena kinds that names 8 or more variants and ends in a wildcard | 56 | 0 |
| Separate recursive descents in lint | at least 7 | 1 |
| Checks of a shared module in one `xsht lint` run without `--fix` | once per importing root: 778 module checks per pass for 90 modules | once per module |
| Whole-program copies made to lower one stage that names a callable | 1 | 0 |
| `xsht lint` on this repository | about 17 s against a 15 s budget | inside the budget, measured alone |
| Wall time of the full gate sequence | about 8 minutes on the 32-thread machine, before the two targets Setup adds | measured at functional close with those targets included; stricter target set afterwards |
| Lines under a blanket `allow(dead_code)` | 47,061 under the corrected scope definition (the original report recorded 47,705) | 0 |
| Entries under "Defects" in `TODO.md` | all open | closed, or moved out with a reason |
| Non-test Rust lines, `src/` and `crates/xsht/src` | 205,864 and 60,041 | lower |

The lowering type-expression resolver metric counts lowerer functions that take
a `TypeExprId`, dispatch on `ArenaTypeExprTag`, and construct a semantic `Type`
or `LoweredType`. Syntax-name renderers and wrappers around already checked
types do not resolve type expressions.

The blanket dead-code measure counts retained production lines covered by
file or module inner allowances and outer module/impl/trait/extern-block
scopes, including such scopes written inside macro expansions. Attributes on a
macro definition do not propagate to its expanded items, and an attribute on a
macro invocation is ignored, so both are listed as item allowances instead.
Narrow allowances on individual functions, types, variants, or fields are
listed in separate start and current inventories, including inherited module
coverage. The original baseline files remain unchanged; the companion ratchet
report changes only the blanket metric row and records its checked source and
scanner hashes.

The same scanner accepts both stage-table forms present at the frozen start
and current source trees. At the start commit, `full.rs` used a handwritten
`FullStageTag` enum and `impl_stage_codec!` table; the previous scanner rejected
that layout because it required the generated table. The scanner validates the
legacy tag and codec rows together but continues to count variants only from
`LoweredPipelineStage`, avoiding double counting. The generated table now has
seven context names, including four borrowed-view contexts, so the scanner
accepts the complete supported sequence and rejects partial or overlapping
authority layouts.

## Setup

The integrator owns Setup and serializes changes to shared files before
implementation fan-out. Delegate independent read-only inventories in
parallel while that work proceeds.

- **Baseline and ratchets.** One XSH program under `dev/` measures every row
  above and writes `dev/consolidation/baseline.json`; `cargo dev check` fails
  when a structural measure rises. The original baseline and metadata remain
  immutable, while `baseline-blanket-scope.json` supplies the corrected scope
  row after both pinned source trees are measured by the same scanner. Its
  provenance enumerates every original-to-remeasured metric difference and
  binds the companion to the original start commit, source manifest, scanner,
  and ratchet fixtures. Timing samples are recorded without a new ratchet.
  Lane tooling for this campaign is XSH, not Python.
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
- **Recount the paired arms.** `designs/evaluator.md` classifies the
  instructions and stages with an arm in both evaluators as of 2026-10-10.
  Repeat the count at the start commit for the table.
- **Item list.** This document is a charter, not yet a lane plan. Once the
  files are partitioned, the integrator writes the items for each workstream
  with a file set, a gate, and a size budget, in `dev/consolidation/ITEMS.md`.
  Include dependencies and ownership status. Split independent slices into
  separate items so available lanes can stay occupied; items that share a
  mutable definition or invariant remain ordered.
- **Dead code.** Remove the blanket `allow(dead_code)` over
  `src/runtime/eval/indexed.rs` and its children and delete what the compiler
  then reports.
- **Stale statements.** Correct the ones that would mislead a lane: the
  module comment of `lowered_run.rs` (it describes an evaluator and names
  functions that no longer exist), the benchmark section of
  `docs/TESTING.md` and `dev/bench.xsh` (they name a bench target that is not
  in the tree), and the line counts in `TODO.md`.
- **Skill.** Write a Codex-discoverable
  `.agents/skills/xsh-consolidation-campaign/SKILL.md` from this document,
  including explicit delegation instructions and the model policy above.
  If Claude Code is used, its
  `.claude/skills/xsh-consolidation-campaign/SKILL.md` follows the same
  ownership, scheduling, and acceptance rules with its own model settings.

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
the duplicate return analysis and mirror structs are deleted, and the
correctness gates pass. Paired runs of the existing loop and call
micro-workloads under `bench/` are evidence collected after functional
completion; stricter latency gates follow later.

### 2. One definition of each instruction's behavior

Design: `designs/evaluator.md`.

**Evidence.** Two evaluators run the same `FullProgram`: the frame evaluator
(`ExplicitFrames` in `explicit_run.rs`) and a recursive one
(`eval_indexed_expr_inner`, `eval_indexed_stmt_inner` in `indexed_run.rs`).
The frame evaluator hands every tag it has no arm for to the recursive one.
The recursive one cannot push work onto the machine that called it, so when
it reaches a call, a block, or a stage callback it builds a new frame machine
and runs it on the native stack. Script recursion through anything the
recursive evaluator owns therefore uses native stack per level; the open
defect "recursion through a stage block aborts at 100 to 150 levels" is one
case of about twenty.

Twenty-seven expression tags and two statement tags have an arm in both.
Most pairs already share one value-level helper and differ only in how
operands are scheduled; five kinds differ in substance. Seventeen stream
stages are written twice with no shared per-item code, and `flat-map`
behaves differently on a list and on a live stream. No test forces one route
or compares the two, which invariant 2 of `docs/ARCHITECTURE.md` requires of
routes that must agree.

**End state** (D1).

- An instruction's behavior on operand values is one function, called by
  whichever scheduler evaluated the operands.
- An operand is evaluated in place only when the verifier has marked that
  instruction instance as unable to run script code. The in-place scheduler
  therefore never starts a frame machine.
- Neither scheduler has a wildcard arm.
- One pipeline engine, with one function per stage for its per-item
  behavior.
- A place that still starts a nested machine is bounded by a counter and
  fails with `stack-overflow`.

**Closes when** the three counts in the table and the wildcard count reach
their end state, recursion through a stage block is a `stack-overflow`
diagnostic with a native test, the native suite passes under both
schedulers, and `docs/ARCHITECTURE.md` describes the result. Follows
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

For the mapping count, a definition owns a full match-based conversion table
for one direction. Entry guards and functions that delegate to a table are
not counted as mappings.

**End state.** One type test, written once over a view both representations
provide. The checker publishes the resolved type of every type expression as
a fact and lowering reads it, so lowering resolves none and the drift
recorder is deleted because nothing is left to drift. One mapping in each
direction between `Type` and `LoweredType`. Each of the paired operations has
one definition.

**Closes when** the three counts in the table reach their end state. Any
observable difference between today's copies is settled before the copies
merge, with a native test: by the SPEC where it speaks, and otherwise by the
copy the executor runs today, since that is the behavior programs have.
Follows workstreams 2 and 4.

**Internal record storage for the final lint repair.** CPU profiles show
substantial copying and destruction of record type maps. `Type::Record` will
share its immutable ordered fields through `Arc`, as module types already do.
The first-party frontend Rust constructor changes from a `BTreeMap` payload to
`Arc<BTreeMap>`; callers convert owned maps with `.into()`. XSH types, runtime
values, and wire formats stay the same. Field order, structural equality,
display, and debug output stay the same. A mutation must obtain unique fields
through copy on write. Constraint
resolution must preserve unresolved identities, foreign-variable failures, and
the existing depth limit; ground records must not copy fields during resolution.

**Source-span lookup for the final lint repair.** The remaining profile places
the largest cost in binary search over source overrides. Preserve the ordered
override vectors and their source identities. Probe the offset from the first
override directly, accept it only when its stored span index matches, and use
the existing search otherwise. This removes searches for dense entries without
adding cached state or changing sparse lookup behavior. Verify expression and
statement spans across dense and sparse source layouts, then rerun shared-bundle
diagnostic tests and the existing isolated 15-second lint gate.
Tests cover independent refinement of clones and unchanged failure behavior.
Keep the representation only if scoped checks and the existing release lint,
memory, and size gates justify it; no speedup is presumed from the profiles.

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
closed. The merged list is the union of the two, so a rule that ran at only
one level now runs at both; a finding that appears in this repository or
Laputa is fixed in the same lane. Follows workstream 5.

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

The directions are decided (D3 to D6, D12). The detailed contracts are in
`designs/resources.md`, `designs/any.md`, and `designs/effects.md`.

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

The ownership that does exist loses handles and costs every call. A handle
created in a middle block and assigned outward from a deeper one is released
while still reachable, as is a handle a producer yields. A process handle or
lock used in a `par-map` worker indexes the worker's own table. And every
successful function return copies the returned value into the host
representation to look for handles, whatever its type.

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
and each fix round checks its freshly parsed affected bundle without reusing
records from older arenas, as specified in the approved module-check design. The
checker prepares a bundle once. Lowering a stage copies nothing. Parameter
lookup in a spread is by name. The limits listed in `TODO.md` are
diagnostics. An on-disk cache and in-process script tests are not part of
this campaign (D7, D8).

Checking a module once is the most invasive checker change in this document
and requires approval of its draft design. A module's check depends on its
bundle today in four ways: type definitions are indexes into the bundle's arena, effect inference
is one fixed point over every proc in the bundle, the type of the builtin
`args` follows the entry's `main`, and each entry point passes different
options. The design is `designs/module-check.md`.

**Closes when** the two structural counts in the table reach their end state,
existing lint gates pass, and the 16,000-field spread has a native test with a
safety timeout. Lint timing is recorded; tighter performance bounds follow
functional completion.
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

| Work | Prerequisite | Runs |
|---|---|---|
| Setup | recorded start commit with green gates | integrator; independent inventories in parallel |
| Workstreams 1, 4, 5, 7 | Setup; each item's required design approved | as many disjoint lanes as ready items permit |
| Workstream 2 | 1; evaluator design approved | lanes |
| Workstream 6 | 5 | lanes |
| Workstream 3 | 2 and 4 | lanes |
| Workstream 9 | 4; module-check design approved | lanes |
| Workstream 8 | its designs approved and each item's checker/runtime prerequisites complete | lanes, alongside other ready work |
| Workstream 10 | functional completion of all unparked implementation items | integrator, with independent evidence lanes |

These are dependencies, not global phase barriers. An item starts as soon as
its prerequisites are accepted and its files are free; it does not wait for
unrelated items in another workstream. Stricter performance gates follow the
functional milestone.

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
| D4 | Affine checking and opaque `Any` during the freeze on new surface | Both are built. Affine checking starts without parameter syntax. The escape hatch for `Any` turned out to exist already (`json.get`), so neither adds a spelling |
| D5 | Raw descriptors | Stay `Int`, as a stated exception |
| D6 | Runtime enforcement of effects | None |
| D7 | On-disk checked-interface cache | Not in this campaign; in-process only |
| D8 | In-process script tests | Deferred; reconsider only if gate time is still the bottleneck after workstream 9 |
| D9 | Comparators for the workload suite | dash, Bash, Nushell, YSH, Elvish, Python |
| D10 | Test build profile; splitting the root crate | Profile yes; no crate split. Ordinary verification retains opt3/no LTO/64 codegen units. |
| D11 | Coordinator model | The session model |
| D12 | Whether `FsRoot` and `FsLock` are released when their owner scope exits, as process handles and network jobs are | Yes, after the scope's defers, so a defer can still use them; `with` stays as the way to observe a failed release. The condition was a count of sites where a root or lock is left open while only a value derived from it escapes: of 731 sites on 2026-10-10 there are none. The integrator recounts at the start commit; above five, the campaign builds the alternative without asking: a check that every root and lock is released, escapes, or is bound by `with` |

## Designs

Files in `dev/consolidation/designs/`. A design states its contract precisely
enough that a lane needs no judgment about it, lists the facts about the code
it relies on, and names the tests that prove it. A measurement that can only
be taken at the start commit is written as a rule with its threshold, not
left open.

Four change what a user of the language or its tools sees, and two are
internal but costly to get wrong. All six were approved by the owner on 2026-10-11. Setup and
implementation proceed after their facts and prerequisites are checked.

| File | Settles | Status |
|---|---|---|
| `any.md` | What an `Any` permits and the migration. The escape hatch is the existing `json.get`, so no spelling is added | approved 2026-10-11 |
| `check-session.md` | One set of check diagnostics for every tool; which stage reports what; one code for an unresolved import | approved 2026-10-11 |
| `resources.md` | The one resource rule and its table; `FsLock` as a type; scope ownership of roots, locks, and streams; moving ownership by type; `check.use-after-release`; the migration | approved 2026-10-11 |
| `effects.md` | Socket calls in `linux` and `unix` need `net` | approved 2026-10-11 |
| `module-check.md` | Checking a module once: what a module's check depends on, the three defects that make it depend on its importer, the per-module driver, reuse | approved 2026-10-11 |
| `evaluator.md` | The in-place mark and what the verifier proves; apply functions; the order in which nested machines are removed | approved 2026-10-11 |

Three are internal shapes with no language contract or decision left for
the owner. The integrator established these designs in Setup on 2026-10-11,
within each workstream's stated end state:

| File | Settles |
|---|---|
| `instruction-table.md` | The table entry and what is generated from it |
| `lint-rules.md` | The rule declaration and the single walker |
| `traversal.md` | The child enumerators |

Decided here and needing no file: affine checking does not go past one scope
in this campaign, so no parameter syntax is designed. On 2026-10-14, the owner
waived the test-profile selection measurement for final acceptance. Ordinary
verification retains opt3 with LTO disabled and 64 codegen units. Final release
evidence uses opt3 with thin LTO and 16 codegen units. No measured profile
selection or fastest-profile claim is required.

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

### 2026-10-16 — main 7b checkpoint

Main is frozen at `7b1485310abdf2190ce4532d840114e74ff7c2cd`, tree
`0f3394902504cbe5271edd8916f504bd493f091a`. The full canonical native run
started at 15:21 UTC and terminated `failed-or-incomplete`, exit 3. Cleanup
resolved and the source was clean through the end. The native suite reports
13,902 passed, 3 failed, and 78 skipped. Root Rust (747 passed, 3 ignored),
root integration (223 passed), XSHT (116 passed, 5 filtered), and soundness
(14 passed) stages passed.

The failures are GNU rm ext3perf (exit 3, empty stderr; failure entry at stdout
line 9,121; not established to be a timeout), uu-sort `buffer_sizes` (timeout
after 4 minutes, canceled with signal 15), and PCI capture identity links
(EAGAIN, errno 11, 9 ms; failure entry at stdout line 15,061). Three hostname
cases pass on source-identical fix `63734367c`. The source check passes with
`blanket_dead_code_lines=0`, `lowering_type_resolver_definitions=0`,
`manual_payload_calls=0`, `optional_raw_calls=0`, and `wide_arena_wildcards=0`.
Current instruction, pattern, stage, factory, predicate, storage, and advisory
source counts are in the [handoff](HANDOFF.md).

The final W9 attempt stopped at E0061; reviewed isolated repairs have focused
build, fixture, and Python evidence, but final capture and comparison remain
pending. The diagnostic baseline is unqualified because the source of older
`27ec`/`0f204` binaries is unknown; it is separate from fallback cleanup
`322889`. The correctness-input plan is on hold for owner approval. Isolated
inference selectors pass on their named sources. Root accepted the isolated
Arc storage change at `e4d282457672a1fc87464c051820ebb64a9eb857`; evidence
commit `cfcdd2737a941468d5d680d4e2418f289c0065bf` records its matched
allocation results, without a timing or RSS claim. No functional close or
performance credit is claimed. This checkpoint replaces earlier live-status
summaries below.

### 2026-10-11 — resumed with Luna xhigh

The owner resumed from `HANDOFF.md` and selected `gpt-6-luna` at `xhigh` for
all subagents. Integration remains at the preserved checkpoint `e8533c12`;
dirty lane trees and incomplete evidence are retained. Independent schema,
module, declaration, source-session, BindingId, grammar, fuzz, lint, and Any
work has resumed. The source-aware helper/session adapter now has an owner.
The Docker daemon was restarted; its missing pinned image is being recovered
before any build or test runs. No interrupted gate has been counted as passing.

### 2026-10-11 — paused for reboot

The owner paused implementation and requested a durable restart handoff. See
[`HANDOFF.md`](HANDOFF.md) for the integration checkpoint, unmerged commits,
unfinished diffs, completed versus interrupted gates, environment recovery,
and remaining campaign work. All owned jobs are stopped. Required artifacts
and extra source snapshots are on disk outside `/tmp`; resume only on the
owner's request. The campaign is not complete.

### 2026-10-11 — approval and Setup started

- Owner approved the entire campaign and all six designs, and requested
  implementation through completion without stopping.
- Start commit: `5e66b6b896a47d0c692739135d51200b007a2a67`. Consolidation has
  its own integration worktree and branch; compatibility remains separate.
- Fifteen independent scouts revalidated code facts and planned exclusive
  implementation slices. Sol-only lane settings are enforced explicitly.
- Owner selected native x86_64 musl development verification in the same
  `Dockerfile.test` environment, with aarch64 final verification. Baseline
  gates and coverage are pending; the ARM image is being prepared separately.
- Owner resolved the concrete oracle conflicts in `ITEMS.md`: approved
  contracts take precedence over prior expectations, preserving coverage and
  logging each migration.
- D12 recount found zero roots and zero locks in the derived-value hazard
  row across this start commit and Laputa `048592b0`; scope ownership may
  proceed. The draft's 731-site denominator was not reproduced.
- Structural starting counts from source: 158 instructions, 18 patterns,
  46 stages, 923 hand-ordered read/finish calls across 922 lines plus 43 optional reads,
  57 wide wildcard matches including aliased pattern matches. Raw Rust
  physical lines include inline tests and are not the non-test baseline.
- Stale design examples are corrected against source evidence without
  inventing APIs. Independent Setup and implementation preparation continue.

### 2026-10-11 — baseline and parallel implementation

- Native release products rebuilt in the pinned Dockerfile environment;
  full native baseline: 5,996 passed, 0 failed, 118 capability/fixture skips.
  Rust full-gate and baseline coverage remain pending.
- Manually repaired five pre-existing redundant-parentheses findings in
  compression code (`44c7567d`). Focused compression tests: 52 passed,
  0 failed, 3 skips. XSH check: 1,162 files clean; initial lightweight
  Laputa check: 305 files clean.
- D3 buffered exec regressions integrated as `b6f1af4f`; focused Unix process
  gate: 21 passed, 0 failed, 2 privilege skips. No runtime change required.
- Diagnostic and imported-enum defects reproduced against frozen baseline
  products. The diagnostic lane's native gates pass; remaining focused unit
  and compiler checks are queued before acceptance.
- Four pure partition drafts retain every moved body and comment under
  visibility/path normalization. Compiler checks are queued; full coverage
  and allocation evidence remain required before acceptance.
- Established the three internal designs under `designs/`. Their schema,
  child-order, semantic-walker, lint-fact, and fix-proof invariants constrain
  later implementation slices.
- Owner requested disk-backed worktrees and build artifacts outside `/tmp`.
  All campaign worktrees use `xsh-consolidation-lanes`; logs, launchers,
  frozen binaries, and scratch evidence use `.work/consolidation`.
- Owner excluded `dev/compat` check/lint while compatibility is WIP.
  Consolidation project discovery and batch respect this exclusion.
  Laputa remains in migration scope with focused lightweight verification;
  automatic heavy sibling-corpus checks are excluded.
- Native build queue is serialized independently of ready agent lanes.
  No formatter, autofixer, push, or unrelated worktree change has run.


### 2026-10-11 — construction prerequisites and contract slices

- Integrated all four pure partitions after source-body preservation proofs
  and pinned musl compiler checks. Combined coverage and allocation evidence
  remain required; partitioning is not a completed semantic workstream.
- D1 and D2 focused gates passed and their TODOs closed; D3 retained the
  exact buffered-output/exit/defer process regressions.
- Boundary contexts passed 104 native tests. The prior value-block outer-loop
  transfer expectation contradicted the approved callable-boundary rule:
  `abe26163` retains tests for rejecting outer break/continue and for valid
  callback-local loops (30 focused tests).
- Bounded nested machines passed 66 native tests and an actual-overflow host
  lifecycle test proving reuse of the same evaluator. The opaque lock and
  evaluator-token foundation passed 24 native tests, two foreign-handle host
  tests and two registry tests. Roots/locks are not yet scope-owned until R2.
- Lock test migrations retain release-error precedence and resource liveness
  coverage: foreign opaque handles replace forged records; marker files and
  independent nonblocking reacquisition replace inspection of private IDs.
- Twenty socket APIs now require process and net. The registry's starting
  required-label test was already contradicted by set_capabilities: coverage
  now checks label/binding invariants, positional rejection and labeled calls.
  Clause-only migrations changed 17 library and 39 applet/test clauses; all
  affected source checks and 21 linux_net tests passed (one capability skip).
- First grammar sharing passed 23 grammar and 50 native tests. The deeper
  22,768-candidate proof found nine parser disagreements and 58.323% validity
  against the retained 80% threshold. Repair is active; no deep-proof pass
  or full rule coverage is claimed.
- Baseline LLVM coverage records pre-existing stale/corpus assertions instead
  of treating aggregate failures as green. Its frozen runtime/lowering source
  hashes still match the starting commit. Signal-fixture readiness and LLVM
  fuzz-worker flush handling have independent host regressions. Corpus phases
  temporarily mask the known directory-symlink cycle and WIP compatibility
  subtree; those mounts are removed before native tests. Production corpus
  helpers are being fixed with retained symlink/WIP regressions.
- The source scanner was remeasured against the exact starting tree: 189,274 /
  49,968 non-test Rust lines and 47,061 lines under blanket dead-code scopes.
  Its earlier report recorded 47,705 because it treated narrow item allowances
  as scopes; those items are now separately inventoried. The same scanner
  measures the current source tree and retains the complete start report as a
  diagnostic artifact. Semantic/performance rows remain unmeasured. Batch
  ratchets use a recorded, bounded intermediate envelope. This campaign uses
  the corrected companion at final close, without an envelope; the CLI also
  preserves original-baseline checks and independently enforces zero blankets
  and source reduction.
- Lexical identities, authoritative instructions, resource scope ownership,
  opaque Any, borrowed value/type predicates, common lint descent and module
  command plans now run on independent file sets. Remaining dependencies are
  scheduled at slice boundaries, with one queued primary compiler and one
  bounded coverage compiler. Timing remains deferred until functional close.
- Comparative workload preparation passed 35 exact seven-tool parity cases,
  eight harness tests and five XSH source checks. No timing result is claimed.
  Separate Laputa consolidation and migration worktrees were created on disk;
  symlink audit found no required Laputa caller migration. Heavy sibling
  corpus checks remain excluded.

### 2026-10-14 — current master status at `9b88622c6`

- Master now includes checker/lowerer control-flow alignment and flow-fact
  reuse (`98b297356`), named-spread lowering without arena clones
  (`7ecb14310`), and the W9 receipt-digest correction (`7099b2a6d`). The
  current formal foundation includes observed-machine coverage and
  occurrence-labeled binding identity. The latest recorded formal gate for
  that inventory passed with 637 fragment roots, 89 owners, 51 inventory
  tests, and all 72 universal obligations still planned.
- The route-oracle acceptance matrix is still open; unaccepted route-oracle
  integration work is not part of current master. W9's pinned real build,
  1,690-input four-profile publication/corpus run, and final source-matched
  cache measurements are also pending. Full correctness and privileged Linux
  acceptance, structural and reliability closure, G0b source freeze, and
  comparative performance remain open. Strict source shrink is advisory under
  the 2026-10-11 owner update. Platform gates remain x86_64 Linux musl only.
- [`HANDOFF.md`](HANDOFF.md) has the current acceptance snapshot and the
  exact evidence boundaries; this campaign remains active and incomplete.

### 2026-10-13 — W9 trace hardening and formal checkpoint at `aaa590c8e`

- Integrated `e090f124e`, `ebecb6d9f`, `2aceba100`, `7ddd56dd4`,
  `7cf61a3ae`, `4662992cd`, `50856659e`, `9b200e3dd`, `07397436b`,
  `5599ddd5e`, `ff0a9f729`, and `aaa590c8e`. W9 reference digests are bound to
  manifest inputs, complete cache-trace coverage is validated, and 36
  synthetic runner cases plus trace checks pass. The command inventory prefers
  native runner invocations while keeping its deterministic fallback.
- The formal gate passed at exact `aaa590c8e`: `lake build Xsh`, axiom audit
  over 5,410 declarations with negative controls, 82 rooted fragments, 89
  owner sets, all 72 complete theorem obligations still planned, 29/29
  inventory tests, and the guarded fresh kernel check. Runner, raw-decoder,
  and delayed-effect fragments do not establish Rust conformance or close
  whole-language obligations. The generated lint reference was refreshed.
- The active pinned image is `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`;
  development and final platform gates are x86_64 Linux musl only. The
  verifier, applet-lint, and observer work are integrated. The archived
  observer comparison has equal legacy/successor reports for its start-source
  snapshot, but does not establish a current full-source scan or final close.
  The recorded verification sizes at `0107c8158` are 31,005,944 bytes for
  `xsh` and 38,002,304 bytes for `xsht`.
- Laputa restored the four nominal Asterinas error families at `ca658ce` with
  a red-to-green diagnostic-type regression. Control-flow follow-up remains
  isolated through `cb45f3e54`; the
  dead-tail cleanup is pending integration and its merged-tree gate. The
  final-correctness launcher is prepared but not executed. W9 real publication
  and corpus evidence, full correctness, source freeze, final close, and
  comparative performance remain open; consolidation is not complete.

### 2026-10-15 — final-batch repair and acceptance status

- The frozen d02 suite attempt failed correctness before the focused repairs;
  its detailed counts and raw log are not retained in the current handoff, and
  its timing is invalid. The focused GNU rm repair passes 14 exact parity cases
  and the isolated 400,000-entry case; full stress acceptance and the nine
  serial native suites remain open.
- Fresh W9 parity passed on 7e93: 1,690 inputs, 6,760 outcomes, zero
  mismatches, and both exact-owner proofs complete. The three-worker controller
  passed. Preserve the historical f105 pass and 47a c1 failure separately.
- The start-source scanner rerun matches its archived report and allowance
  inventory byte for byte. The 7e93 structural check passed every non-advisory
  metric and all four hard zeros. Final semantic review remains open. Frozen-
  source LLVM and API measurements meet the pre-refactor coverage prerequisite;
  no current-source coverage claim is made.
- The source-qualified documentation generator passed without changing files;
  the scoped Markdown check was interrupted. Canonical documentation, snippet,
  and project checks and all ten CLOSE review rows remain open.
- The btop report-only 20-sample run uses a standalone profile. A separate full
  20-sample run and both readiness checks exited 0; cleanup succeeded. The
  `core/tests/test-btop-performance.xsh` file passed 3/3 tests under the
  explicitly qualified `04eb` runtime. This does not qualify a new CLOSE build;
  no performance change or range claim is made.
- The Lean inventory remains separate from Rust conformance. The owner waived
  profile-selection measurement; ordinary verification stays opt3 without LTO
  and with 64 codegen units, while release acceptance uses opt3 with thin LTO
  and 16. Full correctness, platform, final structural acceptance,
  memory, size, scheduler, and comparative gates remain open.
  Consolidation is not complete.

### 2026-10-15 — paused after winddown

Preserved 50 reviewed unfinished overlays as byte-restorable recovery patches,
retained detached source tips, and imported otherwise unretained standalone
clone histories onto parked refs. Removed all 557 linked worktrees, the
remaining standalone checkouts, and both sibling consolidation directories.
The primary checkout is the only retained worktree. Large historical captures
are locally ignored outside the disposable work directory.

The owner steering is incorporated below, including the typing-only acceptance
boundary, exact performance targets and their later waiver, and the first
end-to-end inference slice. Lean stays paused. Final acceptance is incomplete;
the last observer was interrupted for this stop. See the replacement
[restart handoff](HANDOFF.md). No implementation or verification resumes
merely because an agent reads these requirements.

## Typing and formal successor requirements

The owner adopted these requirements on 2026-10-15. They govern resumed
acceptance and successor work. The pause after winddown remains
in force until explicit resume, with Lean requiring its own explicit resume. General inference stays outside the
behavior-preserving consolidation milestone.

### Mission

After an explicit resume, continue unattended in `laputa-systems/xsh`. Preserve the substantial compiler consolidation, native core-command corpus, and independent Lean work. Do not start another ergonomics or language-surface campaign.

Deliver three separately judged outcomes:

1. Close the already approved consolidation acceptance on a qualified source freeze.
2. Complete the general type-inference capability promised by the unified typing campaign, rather than equating improved concrete inference with that capability.
3. Establish a connected formal-verification milestone for an implemented compiler slice, without representing fragment counts or empirical comparisons as universal soundness.

The review baseline was `31c07c71f871428d255fe48e042725e703126b58`. Inspect the actual current checkout; do not reset to this commit. Reuse existing campaign ownership, lane/model policy, and source-qualification procedures. Recreate worktrees under the repository work directory only after explicit resume. Preserve unrelated work and recorded failed receipts.

Read current `AGENTS.md`, `docs/ARCHITECTURE.md`, `docs/TESTING.md`, `docs/DESIGN.md`, the relevant SPEC source templates, `dev/consolidation/{CAMPAIGN,HANDOFF,ITEMS}.md`, `formal/README.md`, and `formal/coverage.json`. The successor performance policy below owns every required numerical target and sampling rule. The original typing campaign is historical context; its availability is not an acceptance dependency. Do not resurrect obsolete documentation paths or test commands from earlier briefs.

Honor the latest approved platform scope: the active consolidation campaign selects host-native x86_64 Linux musl in the pinned `Dockerfile.test` environment. Do not replace it with arbitrary Linux/glibc builds, revive the old macOS/ARM requirement, or mix receipts from unlike hosts, profiles, binaries, or resource allocations. Follow the repository's current `verification`, debug-unit, and release-performance profile rules.

The present consolidation campaign prohibits language behavior changes. Close it first. General parameter/scheme/row inference is a successor milestone, explicitly separate from behavior-preserving consolidation. This steering authorizes that originally requested inference capability in the successor, not unrelated syntax additions. Do not claim output parity for programs whose acceptance or inferred interface deliberately changes.

### What the review actually established

These are source observations, not newly executed build or performance results:

- `src/syntax/parser/stmt.rs::parse_params_arena_only` still emits `ParseExpectedParamType` for a required parameter with neither an annotation nor a default.
- `src/sema/check/infer_param.rs` infers parameter types from defaults, not general relationships inferred from unannotated function bodies.
- `src/sema/check/infer_return.rs` still requires explicit return annotations for exported and recursive pure functions.
- `src/sema/types.rs` has transient inference IDs, concrete structural records, typed callables, validated types, and builtin template parameters. These must not be confused with generalized user-function schemes and open row variables.
- `src/sema/constraints.rs` uses variable bindings into recursive `Type` values. Representative traversal follows aliases with a visited set; it counts alias hops against `MAX_TYPE_DEPTH`. This is not the requested level-based, path-compressed inference graph.
- `infer_default_parameter_types` can repeat statement-wide passes; `EffectGraph::solve_with` repeats scans of its nodes and edges. These are profiling/scaling targets, not permission to assert an unmeasured slowdown.
- `CheckedDeclarations`, published call/type facts, shared module preparation, verifier-owned execution routing, and shared record-type storage are valuable foundations. Keep them.
- The consolidation handoff retains passing historical native suites and focused repair evidence but explicitly leaves final acceptance open. Preserve the extra AWK desugar failure and its original-source cross-file reproducer. A later passing comparison does not recover missing historical child output or environment.
- `formal/README.md` explicitly distinguishes its fragment results from Rust correspondence and universal obligations. Preserve that honesty and the existing full-core target.

Revalidate these observations before changing code. When an observation no longer applies, record the current owner and its executable evidence; do not implement a duplicate repair.

### Phase A — finish the existing campaign without expanding it

Follow the existing handoff's next actions and canonical gate owners. Do not invent another acceptance framework or multiply full-suite runs outside the existing policy.

Freeze the relevant implementation and script state. Bind reports to the actual source, binaries, profile, runner configuration, and test manifest. Reuse earlier evidence only when the existing qualification policy establishes that its semantic scope is unchanged. A documentation-only edit need not invalidate unrelated executable evidence; an applet repair does require its affected script evidence.

Close the outstanding functional, normal/forced-route, structural, semantic-review, documentation, resource, scheduler, and comparative-performance requirements already specified by the campaign. Use only one full native suite at a time. Run timed gates alone with both readiness checks. Never report timing from a run that failed correctness, was interrupted, or failed readiness.

The original-source AWK/curl reproducer identifies a cross-file fixture mutation. Retain its source, environment, selected tests, streams, statuses, and harness-order evidence. Missing historical child outputs remain missing. The fresh canonical batch must qualify the integrated curl and private-fixture repairs. The earlier green AWK-only replay remains insufficient evidence by itself.

Keep upstream compatibility accounting separate from aggregate native-suite totals. Report pinned GNU/uutils/BusyBox origins, native ports, host-boundary exceptions, exclusions, skips, and actual upstream execution results as separate quantities. Do not manufacture a compatibility percentage from all native tests combined. Preserve the existing frozen denominators and explain every changed exclusion.

Deliver a concise current-source acceptance summary in the existing handoff. Preserve raw evidence and hashes, but make the summary interpretable without treating a chain of local `.work` paths as the conclusion itself. No new orchestration framework, extra report hierarchy, or broad unrelated cleanup.

### Phase B — establish a red/green capability contract for general inference

Before redesigning the solver, turn the original user-facing promises into executable acceptance cases. Keep their initial current-checker outcomes as baseline evidence. Use the ordinary loader/checker/indexed execution path, not a special demonstration mode.

The first implementation checkpoint is one end-to-end slice: polymorphic
identity, record projection, two independent importing modules, and their
corresponding rejection cases. Execute the accepted programs through normal
verified indexed instructions after frontend disposal. Preserve reusable
schemes without `Any` substitution, caller-trained signatures, or runtime
inference. Retain the solver counters and the observed 400-variable alias-chain
regression result, including its source and exact outcome before the repair.
This checkpoint precedes wider language coverage and corpus migration.
The owner resumed this checkpoint and authorized isolated implementation
alongside consolidation on 2026-10-15. The initial capability and solver
outcomes are recorded in the current handoff.

At minimum, establish these positive witnesses:

```xsh
pure identity(value) {
  value
}

pure name_of(value) {
  value.name
}

export pure public_identity(value) {
  value
}

test inferred_relationships {
  identity(42) == 42
  identity("demo") == "demo"
  identity(false) == false

  name_of({name: "package", version: "1.0"}) == "package"
  name_of({name: 42, enabled: true}) == 42
}
```

`identity` must have a reusable relationship equivalent to `forall T. T -> T`; it must not be checked separately according to its first caller, retyped from all visible callers, or implemented through `Any`. `name_of` requires a record with `name: T` and permits other fields through a row relationship. A record without `name` must fail statically.

Test the exported scheme from two independent importer roots that instantiate it differently. Adding or removing one importer must not change another importer's inferred interface. Add transparent aliases, independent declaration reordering, alpha-renaming, nested records, optional fields, and mutation-invalidated refinements.

Also cover unannotated parameters anchored by established host signatures; contextual return/constructor/schema inference; monomorphic mutable accumulators; annotated and unannotated ordinary recursive declaration components; higher-order callable/effect relationships within the approved rank-1 design; and explicit public annotations that intentionally restrict an interface.

Negative witnesses must include incompatible writes to the same mutable binding, overgeneralization of captured mutable/resource-bearing state, stale field proofs, unsafe `Any`-to-concrete flows, wrong callable effects, missing required record fields, nominal-type confusion, recursive type equations, and ambiguous concrete overloads. Assert the relevant failure, not merely any nonzero exit code.

Preserve semantics selected by annotations. A generic identity instantiated at Bool returns false; a test/Unit-consuming statement asserts a false Bool. Result wrapping, propagation destinations, body-tail classification, and storage conversions must be fixed in elaboration, never chosen by runtime tags or whichever caller is checked last. Keep or insert purposeful annotations in old code whose meaning depends on them before removing redundant annotations elsewhere.

### Phase C — complete one inference core behind existing compiler interfaces

Build on current checked facts and registry templates. Keep the public/diagnostic `Type` representation as a projection if useful; do not force an unrelated façade rewrite merely to use compact solver IDs internally.

Implement genuinely distinct representations for inference metavariables, generalized scheme parameters, rigid annotated parameters where needed, row variables, and dynamic `Any`. An unsolved metavariable is neither an unknown runtime value nor a recovery type.

Use representative-based unification with rank/size, levels, occurs checks, and rollback-safe mutation. Share ground type structure and avoid cloning wide recursive types in every constraint, candidate, or publication step. Preserve current provenance and fail-closed behavior. Generalize safe declaration values at the proper boundary; instantiate quantified variables freshly at each use; keep environment-reachable and mutable variables monomorphic. Solve ordinary recursive components monomorphically before generalizing; do not introduce polymorphic recursion.

Support unique-label structural record rows for required fields and retained extra structure. Preserve existing nominal, validated, union, container, optional, and schema-validation contracts. Do not turn this into unrestricted subtyping, structural recursion, user-defined trait search, or automatic dynamic validation.

Generate constraints once where feasible and process dependency-indexed worklists. Changes should wake affected obligations, not recheck every unrelated body or module. Builtin templates, typed aliases, argument binding, and overload evidence have one semantic owner. Do not leave a new general solver beside permanent per-method or per-syntax inference repairs that repeat its decisions.

For effects, preserve explicit contract edges, inferred-summary edges, captured-error behavior, unresolved-work facts, delayed-work provenance, and lexical bounds. A dependency worklist and compact concrete fact sets should replace repeated whole-graph scanning where measurement supports it. Generic callable relationships may require effect variables; dynamic unresolved effects must never become the empty set.

Keep finite overload candidates indexed and pruned by known facts. Do not add global exponential backtracking. Residual sealed operation requirements may be generalized only as the approved design permits; concrete executable ambiguity must be diagnosed rather than guessed. Use deterministic complexity budgets without turning ordinary shallow programs into arbitrary depth failures.

Publish solved schemes, call instantiations, selected operations, argument layouts, row evidence, effects, and elaboration decisions. The indexed lowerer consumes this data without resolving types or checking bodies again. Prepared generic evidence must not become per-call inference, dynamic overload searches, or an AST interpreter fallback. Preserve fast monomorphic paths and verify any generic instruction/evidence representation before execution.

#### Required solver regression candidate

The reviewed alias representation can accumulate long chains independently of semantic type nesting. Add this test directly at the solver boundary, initially against the existing implementation; it is a review-derived candidate, not a claim that a current XSH source reproducer was executed:

```rust
let mut constraints = TypeConstraints::default();
let span = Span::at(SourceId::new(0), 0);
let variables = (0..400)
    .map(|_| constraints.fresh(span))
    .collect::<Vec<_>>();
for index in (1..variables.len()).rev() {
    constraints
        .constrain(&variables[index], &variables[index - 1], span)
        .unwrap();
}
constraints.constrain(&variables[0], &Type::Int, span).unwrap();
assert_eq!(constraints.resolve(&variables[399]), Ok(Type::Int));
```

Adapt imports to the actual owner. Record the observed result before changing the algorithm. A chain of equal variables bound to Int is not a 400-level user type. Separate semantic type-depth limits, total-work budgets, alias-cycle detection, and representative traversal. Preserve rejection of true cycles, foreign variables, and malformed/deep types; do not fix this by removing safety limits or merely increasing 256. Include failed-unification rollback tests after compression and branch-local probes, so the performance repair cannot corrupt independent inference states.

### Phase D — use the enlarged command corpus as the inference benchmark

Freeze a representative cohort containing small glue scripts and substantial command implementations, not only synthetic identities. Include `core/wc.xsh`, other text/byte utilities, command parsers, structured-data transformations, library helpers, and representative tooling scripts. Include external consumer modules where already in scope. Select the cohort and annotation categories before seeing removal results.

Keep domain/wire schemas, intentional interface restrictions, and semantically necessary annotations separate from redundant implementation annotations. Record their counts and rationale; do not quietly shrink the denominator until a percentage passes.

Remove eligible annotations jointly in isolated fixture/worktree copies, then check and execute the resulting programs. Verify their outputs, failures, call order, cleanup, and selected operations against the annotated originals. Individual-removal success is insufficient: several annotations can each be redundant only while another remains.

Retain the original campaign's 95% eligible-local and 85% eligible-internal parameter/return/effect targets as targets, not claims. Exported inferred interfaces get an explicit separate comparison. No new `Any`, erased-record fallbacks, inserted per-call runtime checks, or caller-dependent signature inference may be used to improve the score.

A compatibility test corpus does not by itself test generic inference: maintain the dedicated relationship/refactoring/adversarial witnesses from Phase B as well. Conversely, synthetic generic examples do not substitute for the corpus. Require both.

Do not rewrite coreutils algorithms, add native speed shortcuts, or alter their compatibility contracts to make a typing benchmark easier. Their application schemas and corner cases are the workload, not obstacles to delete.

### Phase E — measure solver performance, not just repository-lint time

Keep the existing isolated 15-second repository-lint budget and the active campaign's performance policy. A passing end-to-end lint run is valuable but is not a measurement of solver latency or proof of a relative speedup.

On the same pinned host/profile/resource allocation, separately measure parsing/loading, constraint generation, solving/generalization, checked-fact publication, lowering/verifying, end-to-end cold preparation, retained typing memory, peak process memory, and any separately observed cgroup peak. Label these metrics accurately.

Use the exact successor performance policy below. Freeze a fresh, justified successor baseline only after current consolidation closes; do not erase older failed receipts or silently rebase away regressions. Compare the new checker on unchanged annotated source, then annotation-reduced equivalents. Measure cold performance independently of session reuse; warm workspace checking is an additional result, not the substitute.

Instrument deterministic counts: generated constraints, representative hops, unifications, occurs visits, row-field visits, scheme instantiations, candidate tests, watcher wakeups, solver passes, and per-module/body check counts. Compare several sizes of alias chains, wide/deep records, repeated generic instantiations, reversed declaration chains, module diamonds, and effect-call chains. Genuine growth in output type size must be distinguished from repeated work on the same type graph.

Preserve session-cache isolation and source/option/interface key validity. Attribute cost from lint candidate rechecks separately; do not camouflage it as solver cost or hide it behind warm caches. Optimize measured dominant work, not every BTreeMap by principle.

Performance repairs must preserve solution/rejection under variable renaming, legal constraint reordering, module-root changes, and rollback. Raising budgets, deleting adversarial tests, or returning recovery types on exhaustion is not an improvement.

#### Successor performance policy

This section is the durable numerical owner. The original table survives in
`f9cec20c35385686fe4ea0a1d64621fde1bbbf8b` at
`bench/typing/history/877a114d/xsh-typing-inference-campaign.md`, section 13.3.
The same commit's current `xsh-typing-inference-campaign.md`, sections 13.2–13.4,
and `bench/typing/measurement-policy.json` supersede its hard percentage
ceilings and 200-sample p95 acceptance with lightweight observations. Preserve
that later waiver. The table copies the original numbers as reference targets;
it does not reinstate them as hard gates. Large or reproducible regressions
still require explanation. No new tighter target is introduced here.

Freeze baseline `B` from the accepted post-consolidation source before typing
changes. Compare candidate `C` on the identical stabilized annotated cohort.
For reduced-source comparisons, `B` instead denotes that same candidate on its
annotated source. For generic comparisons, `B` denotes its handwritten
monomorphic equivalent with the same algorithm, inputs, calls, and effects.
Freeze those comparator identities and eligible workloads before measurement.
The old macOS baseline is historical and cannot qualify current Linux results.

Profile `P` is the current release profile: opt3, thin LTO, 16 codegen units,
host-native x86_64 Linux musl in the pinned `Dockerfile.test` environment.
Use identical features, toolchain, worker configuration, and recorded resource
allocation for both arms. Profile `I` is a separately identified instrumented
profiling product with the same release settings for retained-byte accounting.
Its timings and allocator traffic do not substitute for product timing or RSS.
Ordinary verification remains opt3 without LTO and 64 codegen units.

Sampling rule `S` is one fresh-process warmup per arm and workload, then five
measured fresh-process pairs with alternating arm order. Report each arm's
ordinary median and full observed range, preserving every raw sample. Cold
checks have no process-local solver/interface reuse; filesystem caches may be
warm. Keep within-bundle reuse. Retain invalid pairs and their reasons; replace
the entire pair, with at most ten invalid pairs per workload. A changed source,
binary, or configuration starts a new set; do not pool incompatible sets.
Collect memory in the same product observations or separately under `I`, with
matching units and collector definitions. Five observations establish no p95.

Time-based observations require `perf-gate-ready` to exit zero before and with
`-1` immediately afterward. Discard timing overlapped by load. If the host stays
busy, record `perf gate deferred: host busy`; a deferred gate is not a pass.
Memory and binary-size checks still run. Record CPU time as well as wall time;
use the minimum CPU observation for primary comparisons and retain medians and
ranges for context. Do not fail on a timing difference within observed spread.
This current host-readiness rule supersedes the historical no-quiet-host rule.

In the table, `B50` and `B95` are the matching baseline median and historical
p95, and `Bmem` is the matching baseline memory quantity. Percentages apply to
that statistic; absolute memory tolerances use binary MiB. Historical p95
targets are recorded for completeness and remain unmeasured under rule `S`.

| Metric and comparison | Median or memory reference target | Historical p95 reference target | Profile | Sampling | Status |
|---|---|---|---|---|---|
| Cold frontend, identical annotated source | `C50 <= B50 + max(5% * B50, 1 ms)` | `C95 <= B95 + max(10% * B95, 2 ms)` | P | S | Aspirational reference |
| Production peak RSS, identical annotated source | `Cmem <= Bmem + max(10% * Bmem, 2 MiB)` | None | P | S, record process peaks | Aspirational reference |
| Retained typing/interface bytes, identical annotated source | `Cmem <= Bmem + max(10% * Bmem, 2 MiB)` | None | I | Separate matched retained-byte observations | Aspirational reference |
| Startup through verified preparation, identical annotated source | `C50 <= B50 + max(5% * B50, 1 ms)` | `C95 <= B95 + max(10% * B95, 2 ms)` | P | S | Aspirational reference |
| Unchanged monomorphic runtime | `C50 <= B50 + max(5% * B50, 1 ms)` | `C95 <= B95 + max(10% * B95, 2 ms)` | P | S | Aspirational reference |
| Jointly reduced versus candidate annotated frontend | `C50 <= B50 + max(20% * B50, 2 ms)` | `C95 <= B95 + max(25% * B95, 3 ms)` | P | S | Aspirational reference |
| Reduced versus annotated peak RSS and retained bytes, measured separately | `Cmem <= Bmem + max(15% * Bmem, 2 MiB)` | None | P for RSS; I for retained bytes | S for RSS; matched retained-byte observations | Aspirational reference |
| Generic/evidence runtime versus monomorphic equivalent | `C50 <= B50 + max(5% * B50, 1 ms)` | `C95 <= B95 + max(10% * B95, 2 ms)` | P | S | Aspirational reference |
| Generic/evidence startup versus monomorphic equivalent | `C50 <= B50 + max(10% * B50, 1 ms)` | `C95 <= B95 + max(15% * B95, 2 ms)` | P | S | Aspirational reference |
| Generic/evidence peak RSS versus monomorphic equivalent | `Cmem <= Bmem + max(10% * Bmem, 2 MiB)` | None | P | S, record process peaks | Aspirational reference |

These existing numerical gates and investigation thresholds remain separate:

| Metric | Comparison baseline | Limit and absolute tolerance | Profile | Sampling | Status |
|---|---|---|---|---|---|
| Repository lint, including startup, discovery, checking, and linting | Absolute elapsed wall time | 15 seconds; no added tolerance | P | Existing isolated `repository_lint_is_clean_within_wall_budget` test, readiness before and after | Hard gate |
| Total and applicable family-specific solver work | Previous size in fixed-seed 1k/2k/4k/8k families | At most 2.6x at both 2k→4k and 4k→8k; no absolute allowance | Same identified counter product in both arms | Exact deterministic counts for every size and family | Hard gate |
| Retained type/constraint memory scaling | Previous size in the same families | At most 2.5x at both final doublings; no absolute allowance | I | Matched retained-byte accounting for every size and family | Hard gate |
| Wall-time scaling | Previous size in the same families | Investigate growth above 2.8x; no absolute allowance | P | S and readiness before and after | Investigation threshold |

Separate unavoidable output growth from repeated work. Ordinary families must
remain below their frozen resource guards; hitting a guard fails that family.
No invocation-time inference, per-incidental-shape bodies, input-proportional
metadata graphs, or erased unresolved effects are allowed, regardless of timing.
Record phase costs, memory, prepared evidence/code size, and finite resource
guards without inventing additional numerical thresholds. Existing consolidation
performance requirements retain their own owner and remain prerequisites.

### Phase F — connected Lean compiler milestone, deferred

Keep Lean an independent specification/verification layer. No production dependency on Lean. Preserve the existing formal campaign's core scope and all universal obligations; coreutils, system-report, btop, and other application algorithms do not become proof targets here. They may supply minimized semantic counterexamples.

Honor the post-consolidation source-freeze gate before claiming Rust correspondence. Do not optimize for number of roots, touched files, or fragment coverage. Choose a connected path with immediate relevance to this campaign and complete it before opening more disconnected model families.

A suitable first path is resolved lexical names -> generated constraints -> solved substitutions/schemes -> elaborated typed core -> verified indexed instructions -> observable execution for an explicitly defined core fragment containing variables, functions/application, immutable and mutable local bindings, records/projection, Bool/Result control, and the relevant effect boundaries. Stage it if necessary, but make each stage's exact domain explicit.

For that domain, state and prove the actual typing/substitution/generalization/elaboration obligations. Prove or independently check the translation from source constructs to modeled constraints; do not assume the checker selected the right operation or classified every effect and then describe a theorem about that assumed graph as checker soundness. Existing effect-lattice/solver fragments are useful inputs, not the missing translation proof itself.

Bind executable correspondence witnesses to actual emitted checked facts/raw IR and the active schema. Add negative mutations that the independent checker rejects, not just round trips through the same encoder. Distinguish universally quantified Lean theorems, validated finite certificates, empirical differential tests, and unproved host assumptions in the existing ledger. Finite conformance witnesses do not prove universal Rust implementation correctness.

Run the existing pinned Lean build, axiom/trust audit, kernel or external rechecking where required, and negative controls. Absence of `sorry` alone does not establish the desired theorem or implementation connection. Do not introduce compiler-trusting/native-evaluation axioms to evade the existing trust policy. Record necessary assumptions precisely rather than hiding them in bridge inputs.

Complete one connected slice as an intermediate milestone while leaving broader obligations honestly open. Do not shrink the original full-core theorem inventory or close the whole formal campaign because the intermediate slice passed.

### Execution and completion

Typing convergence is judged on Phases B–E, after the existing consolidation
prerequisite closes. The owner has authorized independent, staggered Phase B–E
implementation in isolated worktrees while consolidation acceptance proceeds;
this supersedes the earlier wait-before-lowerer sequence. Shared integration
and convergence acceptance remain gated on consolidation close. Phase F remains
deferred until the owner explicitly resumes Lean. A typing-only resume does not
authorize formal work, and the paused formal milestone does not block typing
acceptance.

Use existing current repository gate commands and changed-path selection. Process tests use verification/release products from the same target; debug-only unit assertions still run in their proper profile. Respect the no-source-tree-formatter/autofixer rule. Focused migration acceptance in isolated temporary fixtures is in scope; whole-tree mechanical churn is not.

Continue through independently ready work without asking routine design questions. Use bounded agent/worktree ownership and separate compilation/test/performance concurrency budgets. Do not weaken or quarantine a failing test to maintain momentum. If a genuine blocker remains, report the exact reproducer, constraint, failed gate, and dependent work; do not mark the milestone complete or replace the implementation with a report.

The final report must answer, in order:

1. Which existing consolidation gates are now closed on which qualified source, and what remains open?
2. Which formerly rejected unannotated programs now have principled inferred schemes, including independent imports and negative controls?
3. How many eligible annotations were removed jointly on the frozen real-program cohort, without weaker boundaries?
4. What do cold phase timings, retained memory, runtime comparisons, and scaling counters show?
5. For a typing-only report, state that Lean is deferred by the owner and does not block typing acceptance. Only after an explicit Lean resume, report which statements form a connected proof/correspondence milestone, which are finite empirical evidence, and which universal obligations remain unproved.

The desired outcome is not another large volume of syntax, formal fragments, or acceptance paperwork. It is an annotation-light, fast, soundly constrained compiler whose existing corpus and formal layer test the same semantic contracts.
