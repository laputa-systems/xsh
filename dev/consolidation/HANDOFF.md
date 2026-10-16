# Consolidation handoff

Status: resumed by the owner on 2026-10-16. Lean and Phase F stay paused.
Inference remains isolated until consolidation acceptance permits its merge.
The [stop handoff](CAMPAIGN.md#stop-handoff-2026-10-16) records the starting
state, parked branches, retained worktrees and completed stop audit.

## Active resume

The `bb809e1a6` closure attempt stopped on a stale Rust test expectation.
The schema correctly marks `ExprProcessCommandArgv` as `FrameApply`.
The expected cohort omitted it. Repair `21f8848b6` adds the missing row in
schema order, and the exact pinned musl debug test passes.
The [failed canonical run](evidence/canonical-bb809e1a/README.md) retains
747 passes, one failure, and three ignored tests. No canonical close exists.

Gate repairs `3c39d8271` and `6e930ee73` are integrated. The split context now
receives 0.01 CPU and 1 GiB. The block-device fixture mounts its actual
`.work/tmp` path as a bounded nodev tmpfs. All 43 launcher controls pass.
The immutable r6 manifest remains approved. Supplemental attempt
`correctness-bb809e1a-20261016-r4` passes products, three harness builds,
workspace warnings-denied Clippy, and all five Linux contexts. Its receipt
still requires review of three ignored Rust tests and two native skips.
Its source pin remains `bb809e1a6`.

The three fresh QEMU guests pass all seven required cases on `bb809e1a6`.
Evidence is preserved on `campaign/qemu-platform-20261016` at `6088167b4`.
The current-source route replay did not execute. The W9 input-0 probe passed
all five roles, native hashes, projections, and source audits. Its full capture
was cancelled gracefully for the corrected freeze. The reference adapter
completed 68 ranges, and the candidate remained partial. Its status is
incomplete. All owned workers and locks were released.

Seal a new clean controller and candidate after this handoff commit. Run fresh
supplemental, canonical, W9, route, and platform receipts for that source.
Preserve the old receipts without changing their source credit. Keep the next
controller fixed through functional and resource verification. The resource
queue contains ten full native suites: correctness canonical, profile cost,
forced frames, six scheduler runs, and the separate release resource observer.
The observer requires functional close and a qualified release profile.

The isolated composition checkpoint is accepted at `18df5d9b3`, with evidence
at [typing composition](winddown/evidence/typing-composition-18df5d9b3/README.md).
The separate development-module parity repair is committed at `5fade8ca1`
and awaits integrator acceptance. Inference still has no merge to main.
Accelerated cross-cutting typing work uses `gpt-6.1-sol` at `medium`.
Other active lanes retain `gpt-6-luna` at `xhigh`, without nested delegation.

The owner invoked `$accelerate` on 2026-10-16. The source inventory on
`fe674c14e` has zero source-inventory gaps across 152 rows. All 73 collector
controls and 15 replay controls pass, including exact native-output admission.
The final replay selects 214 unique IDs in both modes, for 428 invocations.
Freeze the integrated source and run supplemental, canonical, W9, route,
platform, and resource closure as independent lanes with shared build/native
locks. Fresh W9 receipts may use the current clean controller and its qualified
comparator optimization. Hold that controller fixed throughout the cohort.
Historical 56bf receipts remain unchanged. Inference stays isolated.

The integration tree was clean at `e048b33f2`. The integrator cancelled the
closeout guard before selecting work. The resume assignments are:

| Lane | Worktree below `.work/wt/` | Base and scope |
|---|---|---|
| Inference ownership | `typing-pipeline-integration` | `e2cf1fec2`; exact generic exports, retained values by binding identity, and required regressions |
| Acceptance | `resume-acceptance` | `e048b33f2`; approved supplemental and canonical gates |
| Loop speed | `resume-route-replay` | initial audit complete; current driver must admit distinct If/Match probes |
| W9 parity | `resume-w9` | `e048b33f2`; immutable capture admission and strict comparison |
| Routes and resources | `resume-routes`, `resume-route-tools` | frozen `e048b33f2` source; separate collector edits and current route matrix |
| Fallback review | `resume-fallback-review` | remaining expression dispatch inventory in the AST extractor |
| Operand order | `resume-order-review` | existing assertion map and focused missing order oracles |
| Route strategy | `resume-route-strategy` | truthful process-command apply witness and dispatch invariant |
| Process source order | `resume-process-order` | repair run operand evaluation to the user-selected SPEC contract |
| Semantic review | removed after integration | `e048b33f2`; ten acceptance rows and current-source qualification |
| Performance preparation | removed after integration | `e048b33f2`; reviewed existing post-functional gate inputs and controls |

Each resume worktree has its matching `campaign/resume-*-20261016` branch.
Inference retains `campaign/typing-pipeline-integration`. All lanes use
`gpt-6-luna` at `xhigh`, with no nested delegation. Acceptance owns the main
build slot. Inference may run one focused debug build in a separate target
directory. Only one full native suite may run at a time.

The loop review required no selector changes. The retained inference command
builds one library harness and executes exact tests directly. Its 286 selections
include a repeated frontend-disposal selection. The next native runner can
select the two complete inference files and two exact size-literal cases in
four invocations, preserving the earlier 20 selections. W9 comparison repeats
derived-record reads and capture-receipt parsing. Its owner is checking whether
those operations can share validated data without weakening mutation checks.

Accepted resume batch: `1c3fbeee1` repairs the scheduler derivation test fixture.
Synthetic Git repositories produce different full-index blob IDs from the
production overlay. The test now uses its synthetic diff digest, retains the
separate production digest and ancestry controls, and rejects independent drift
in each patched file. Production source is unchanged. The test touches 57 lines.
The integrator reran `test_reference_derivation.py` successfully after the
fast-forward merge. The clean lane worktree and merged branch were removed.

Performance preparation confirms that the existing final sequence requires nine
serial full native suites, including canonical, release, forced frames, and six
paired scheduler runs. The reference overlay, comparator inputs, locks and
resource controls were inspected. No functional-close record exists yet, and
no final performance run started.

The semantic review is integrated as `85699e799`. Its ten rows distinguish
current source inspection from historical executions and unrun gates. The
integrator independently confirmed unchanged checker, loader, project and
shared lint-walker paths, plus the W9 candidate's compiler-input equivalence.
The clean review worktree and merged branch were removed.

Launcher repair `c066957ea` separates the controller, gate tools and selected
source. Execution now requires `--source-root`. Source and tool commit, tree,
cleanliness and hashes remain checked independently. The integrator reran all
38 controls successfully. The three e048 supplemental attempts build products
and pass workspace Clippy. The first exposed a read-only special-context source
mount; `d8d6c57df` restores the historical writable source context with Git and
runtime artifacts read-only. The next two pass all 23 boundary checks, then fail
the PTY check because its Cargo harness resolves `xsht` beside its own `xsh`.
Mounting the product target (`6f31d46b4`) alone does not supply that sibling.
`80739cf35` stages the qualified product beside the harness before runtime.
Focused execution found receipt-shape mismatches in that helper; `cea9013a6`
and `8652e411c` repair them against the actual qualified receipt. The integrator
reran all 41 controls successfully. The exact PTY replay now passes (one test,
62 filtered) using the e048 artifacts with matching staged `xsht` hashes.
Earlier receipts stay unchanged. No full supplemental or canonical result is
accepted yet.

W9 comparison repair `fa58a4e54` shares each validated derived record between
correction-delta detection and parity. The integrator reran all 86 adapter-runner
controls successfully. For the frozen corpus, the removed second pass read
6,760 record groups containing 27,040 sidecar files. This is operation accounting,
not a measured speedup. The unchanged historical comparator completed all
1,690 inputs and 6,760 outcomes with zero mismatches on source `56bf3286c`.
A private user/mount namespace supplied its exact historical controller while
preserving strict admissions. Both 1,690-owner proofs, assignment-facts proof,
and all three raw-hash scans pass. [The completed evidence](evidence/w9-resume-56bf/README.md)
retains the receipts and namespace command. The optimized comparator was not
used in this historical replay. Current-source W9 acceptance remains open:
the process repair changes parser and checker traversal. Its new cohort receipt
requires fresh adapter, exact-owner, assignment, and derived-reference captures.
The pinned corpus and program snapshot remain reusable.

The strict matrix additionally needs 60 unique non-expression runtime selectors
in both modes. They are disjoint from the 92 expression probes and 32
source-order selectors. All 184 exact behavior executions and 184 route probes
pass on frozen e048. Integrated collector `020df10c9` consumes the strict
source-order receipts and source-validated emitter route IDs. The integrator
reran all 68 controls. The actual matrix credits 184 route tuples, 120
behavior-only paths and 64 source-order steps across 32 rows. It remains red:
63 fallback inventories, 87 operand-order gaps, and two distinct If/Match
in-place branches remain. Non-expression results earn no route artifacts.

Source review found a production metadata conflict: `ExprProcessCommandArgv`
declares `FrameControl` but executes through `indexed_frame_apply_tag` and
`push_apply_operands`. Its e048 artifact consequently reports the wrong deeper
route. Integrated `b920d3e92` changes that schema field to `FrameApply` and adds
an invariant over the actual non-eager apply-dispatch set, checking normal and
forced witness routes. The test fails on the old source and passes on the fix;
the integrator reran the pinned debug harness (one pass, 750 filtered).
Production accounting is one changed schema row, two deleted comment lines,
and 31 added test lines. The e048 freeze and receipts remain historical; final
builds and routes must qualify the repaired source. Full acceptance waits for
that freeze and the missing native order/branch assertions.

`b06f17e56` adds four native assertions for path options, reversed named module
arguments, pipeline callback sequencing, and recursive named calls. Each passes
in both modes against the e048 products; final-source receipts remain required.
`1a5a1ffed` adds a pure If probe. Collector `8a44e1489` retains distinct exact
proof identities `(tag, mode, route_id, test_id)` and aggregates branch coverage
by route. Distinct tests may corroborate the same forced route; repeated exact
claims and contradictory test/mode claims fail. All 69 collector controls pass
at integration. Integrated `3eb12bb42` and `c995ac87e` admit all 94 exact probes,
derive their IDs and source pin from validated configuration, and remove the
historical mapping-file authority. All ten replay controls pass.

Integrated `c8a02d67f`, `59fa1af8f`, and `fb1e7fe93` consume the expression
dispatch AST inventory. All 92 expression rows have source classification,
including seven defensive boundaries and one reachable in-place bypass.
Normal dispatch records eligibility and its selector predicate separately.
The integrator reran all 70 collector and ten replay controls successfully.
The pinned Linux musl extractor harness also passes both focused expression
tests, including forced-selector mutations. These are source and tool checks.
The current matrix has no exact-source runtime credit while its source lock
and receipts remain stale. Final runtime collection waits for the process fix.

The expanded order map covers 115 candidate rows. Four process forms still
await the source-order repair. Follow-up assertions observe the Bool initializer
before overwrite and ordered body effects for capture, context scope, and with.
Resource tests prove eventual release, without claiming release order.
Three authorized test/config commits landed directly on main after a lane
missed its worktree. The integrator stopped that lane's writes, reviewed the
three-file scope, and reran the affected collector and replay controls.

The order review exposed a contract conflict: run syntax writes options and
environment operands before the target, but the evaluator runs target,
arguments, environment, redirections, then options. Existing process-frame tests
preserve that phase order, contrary to the SPEC's source-order rule. The user
explicitly chose **follow source order** on 2026-10-16. The process-order lane
owns the failing regression, shared runtime/lowering repair, and correction of
the old assertion. Run capture, pipeline, spawn-run, and command-builder order
credit remains open until repaired. Final source qualification waits for this
behavior change. Other route and inference work continues independently.

The source-order repair is integrated as `68cbed2b4` and `877ae06a9`.
`106ef25fa` updates the SPEC template and regenerates its output with qualified
verification tools. The clean lane `50f54522c` passes nine process-frame tests
and two compatibility tests in each execution mode. The integrator reran the
corrected runtime regression in both modes and confirmed production-source
equivalence after integration. [Repair evidence](evidence/process-source-order-50f54522c/README.md)
retains its old-source failure, new-source passes, and bounded AST accounting.
One old test was wrong: `*${empty}` failed checking rather than exercising
runtime splice conversion. The corrected computed `@(...)` splice reaches the
runtime error and proves later operands are skipped. Collector field paths and
variant completeness controls must integrate before the final source freeze.

The isolated generic module export slice is accepted at `8475984a9`, tree
`2d2dfebdb892fa6dbae70db5fd38199489321fdf`. All 289 Rust selections
(288 distinct tests), 21 native tests, production `-D warnings`, and
verification-product builds pass on that source. Independent review found no
remaining issue in exact export authority and the reused-module writer lookup.
The [accepted evidence](winddown/evidence/typing-8475984a9/README.md) retains
the exact selectors, commands, outputs, source snapshots, and product hashes.
Main integration waits for consolidation closure. Broader inference and
performance goals remain open; test-only rigid/skolem checks do not establish
production support.

The accepted results remain those recorded below. Starting a gate or repairing
its iteration loop does not accept the source or close a remaining requirement.

## Latest stop checkpoint

The XSHT repair batch is integrated from `2226c0ba` (source qualification
`e55dc689`). Actual warning-denied package and workspace Clippy pass.
45 CLI, 34 rule, one integration and three followup tests passed in the lane.
The failed 56bf supplemental attempt and corrected Clippy commands/streams are
retained separately in [existing evidence](evidence/clippy-56bf-warning-deny-01/README.md).
This does not execute the five required contexts or close the canonical gate.

The collector source from `dc64ac3f5` describes all eight current production
inventory families. Lane checks pass 56 Rust and 65 Python tests; root reran
65 Python tests after integration. The earlier be0b actual routes pass 184/184
and source-order cases 64/64, but their exact source pins do not qualify a
new final matrix. Run the current routes only after the next final freeze.

W9 completed its held captures and admission proofs on controller/candidate
56bf. The original comparison failed after winddown cleanup removed a historical
source dependency; that exact source and patch were restored. A separately
named compare-only retry reached 225 inputs and 900 outcomes with no observed
mismatch, then stopped at the 15-minute winddown bound. Its exit -2 followed
signal 15. Comparison remains incomplete with zero completed ranges. No W9
parity is accepted and no replacement run starts during pause.
[The W9 stop evidence](evidence/w9-final-56bf/README.md) records the exact
commands, source pins, admissions, failed first comparison and bounded retry.

Inference is parked at `e2cf1fec2`, with implementation `e20914bb9`: 286 selected
Rust tests pass and the native capability group is 19/20. The source loader's
exact module type excludes quantified pure exports, producing a 1-vs-3 export
set mismatch when reading a module value. Shared static module initialization
is deduplicated, but its value materialization still uses names and needs
binding-identity ownership. [The parked handoff](winddown/typing-first-slice.md)
records the approved narrow repair and regression requirements. There is no
accepted first-slice matrix or general inference claim.

The rest of this document retains earlier source-specific observations. It
must not override the stop handoff or turn component results into acceptance.

## Production qualification

The last full canonical run used `7b1485310abdf2190ce4532d840114e74ff7c2cd`
(tree `0f3394902504cbe5271edd8916f504bd493f091a`). It failed with 13,902
native passes, 3 failures, and 78 skips; cleanup resolved and source stayed
clean. Root Rust passed 747 with 3 ignored, integration 223, XSHT 116 with 5
filtered, and soundness 14. The [complete evidence](evidence/canonical-7b148/README.md)
retains the failed receipt and logs.

The failures were GNU rm ext3perf (exit 3, empty stderr), uu-sort buffer_sizes
(240-second timeout), and PCI capture identity links (EAGAIN). A bounded
production fix retries the identical confined openat2 request at most eight
times for EAGAIN, preserving flags and final errors. The qualified be0b PCI
comparison passes under the corrected resource constraints while the old 7b
stress case fails; see [PCI evidence](evidence/rooted-path-openat2-9b2/README.md).

On be0b, exact rm and sort selectors each pass once. Rm's 38.144-second timing
is unqualified because a Cargo build overlapped despite passing readiness.
Sort's 139.973 seconds is one readiness-bracketed observation, not a speed
comparison. Neither pass establishes the full-suite failure cause or replaces
the required fresh full run. [Evidence and scheduler findings](evidence/rm-sort-isolated-be0b/README.md)
retain the initial launcher failures and corrected runs. No scheduler or timeout
budget was changed.

Be0b default products were qualified separately: bootstrap xsh was fresh=false;
product xsh was fresh=true with the same hash; xsht and xshi were fresh=false.
That products-only run earns no canonical credit. Earlier 7b focused semantic,
lint, source-order, diagnostic, module-cache, and structural results remain
source-specific evidence under the historical links below.

## W9: failed captures and corrected producer

All be0b producer captures completed for 1,690 inputs and 6,760 outcomes.
Initial consumers failed on an undefined path map and a missing program root
in a second harness listing. Repairs `81419c0e3` and `23ad103a2` correct those
contracts and bind helper authority to the validated controller root.

Full strict reference validation then failed at owner 0. The adapter sidecar
has statement rows but no AST-owner rows: `_derived_owner_helper_bytes` removed
the owner-emission loop while adding flush/sync. This was a producer defect,
not an invalid source span or an error to normalize. Repair `1c4962e62` retains
the loop. The Python gates pass 220 module tests plus 54 publication-run tests.
[Tracked failure evidence](evidence/w9-owner-row-derivation-be0b/README.md)
contains the source, both owner-0 sidecars, receipts, and failed validator log.
Full ignored captures remain intact. They cannot qualify the repaired producer.

Next: seal a fresh source cohort, build the required products, and exercise
input 0 through the actual adapter and exact-owner capture entries. Strictly
compare projections and independently validate source-audit bytes/hashes before
starting the full cohort. Assignment facts still require their owned proof;
no field may be removed to obtain parity. No W9 parity is currently accepted.

## Routes and supplemental correctness gates

Be0b actual runtime results are 184/184 route probes and 64/64 source-order
executions. Product hashes and source checks match before/after. Isolated
collector `2388c7010` closes reviewed call-input and shadowing holes (53 focused
Rust and 65 Python checks), but actual collection fails on the separate typed
scalar inventory's obsolete `match tag` assumption. Current production uses
`match tag.kind()` and decoder errors, rather than the old mismatched-view
wildcard labels. Repair must describe actual branches and keep strict coverage;
no matrix credit is recorded. The failed collection and old source lock remain
preserved. Collector work stays isolated while the next production freeze runs.

The owner approved the replacement gate plan: one verification-product build,
three harness builds, workspace Clippy, and five existing Linux contexts.
Attempts r1-r3 failed in plumbing. R4 built products/harnesses, then failed
Clippy at mdev.rs:1689. Integrated runner `82248fa7d` separates child stdout
and stderr artifacts, retains bounded cleanup, protects read-only source and
runtime targets, and fixes Cargo argument selection. All 26 runner controls
pass. The mdev borrow repair passes pinned-musl xsh-applets lib/test Clippy with
-D warnings. [Failure evidence](winddown/evidence/final-correctness-gates/README.md)
retains each earlier failure. The full replacement gate still needs a new
attempt; the five Linux contexts have no credit.

## Isolated inference checkpoint

`typing_scheme_encoding` remains sole coherence owner in
`.work/wt/typing-pipeline-integration`. At clean `e2ecbb906`, lib harness and
verification products build. The same-pin 286-selector Rust group is 261 pass /
25 fail; the native 20-selector group is 11 pass / 9 fail. Twenty-four Rust and
all nine native failures are new regressions in the driver-annotation proof:
it mistakes initializer read slots for the newly declared writer binding. The
remaining Rust failure is a repeated-call lowering fixture missing embedded
implementation facts. These failures remain failures and are being repaired.

The prepared-program frontend-disposal test passes on both indexed routes at
that pin. Five metadata tests, recursive monomorphism, 47 constraint tests,
nine scheme tests, and eight frontend-stats tests pass. These are component
results, not acceptance of the six named native requirements. The final named
matrix must bind all required results to one qualified isolated checkpoint.

The integrated solver counter source includes `714507b43` via `a730d4199`.
Repeated calls and retained-memory accounting have bounded tests and review,
but total body work across effect probes and historical cached modules is not
measured by the final-Checker count. Retained semantic bytes are stage estimates,
not one simultaneously live graph or RSS. Direct Cargo's CARGO_BUILD_WARNINGS
marker did not enforce rustc denial: non-test products still emit 15 dead-code
warnings at e2. An actual warnings-denied check remains open.

Isolated integration and acceptance are authorized now. Inference merging onto
main waits for consolidation closure. The next compositional witness follows
the closed first slice; broader rank-1, row, recursion, effect, corpus, and
performance goals remain in force. Optional Arc work is separate.

## Bounded closure queue

1. Seal the reviewed consolidation checkout and pass the W9 input-0 smoke.
2. Execute the approved replacement gate as a new attempt. Rerun the canonical
   full gate on the repaired freeze, with existing locks and resource checks.
3. Complete W9 fresh captures, exact/assignment proofs, and strict comparison.
4. Finish the isolated route collector, execute current routes as required by
   source qualification, and collect the actual matrix.
5. Repair inference's driver ownership and lowering fixture, then close the
   complete named first-slice/verifier group on one isolated source.
6. Close remaining semantic review/resource gates. Timing claims require the
   existing readiness and overlap rules; paired successor baselines still wait
   for the approved post-consolidation freeze.

## Historical evidence links

The links below are retained from earlier handoffs. They describe their own source pins and limits; none substitutes for the current-source gates above.

**Canonical, fixtures, and source checks:** [fixture repair](evidence/fixture-fixes-577b/driver.stdout), [failed 0d6 canonical receipt](evidence/canonical-0d6/canonical-replay.json), [0d6 product sidecar](evidence/canonical-0d6/actual-products.json), [cross-file AWK cause](evidence/awk-cross-file-0ea/README.md), [bounded AWK replay](evidence/awk-replay-0ea/README.md), [Clippy batch](evidence/clippy-4d7b/README.md), [Clippy scope outputs](evidence/clippy-ed908/README.md), [canonical e89 receipt](evidence/canonical-e89/canonical-replay.json), [e89 stdout](evidence/canonical-e89/stdout.log), [e89 stderr](evidence/canonical-e89/stderr.log), [e89 resource capture](evidence/canonical-e89/container-resources.json), [e89 bootstrap receipt](evidence/canonical-e89/bootstrap-receipt.json), [canonical 787](evidence/canonical-787/README.md), [canonical ed908](evidence/canonical-ed908/README.md), [canonical 45b4](evidence/canonical-45b4/README.md), [canonical 0d6 resources](evidence/canonical-0d6/actual-products.json), [structural scan](evidence/structural-787/README.md), [source review](evidence/source-review-6198/README.md), [assignment producer output](evidence/resume-assignment-producer.log), [merged project-test output](evidence/resume-a86d4-project-tests.log), [interrupted canonical output](evidence/resume-fff13082-canonical.log), [interrupted canonical resources](evidence/resume-fff13082-resources.json), [lint-flow evidence](evidence/lint-flow-d2c8/README.md), [feature-touch recount](evidence/feature-touch-d2c8/README.md), [replacement-label soundness evidence](evidence/fuzz-replacement-1d64/README.md).

**W9, routes, and Linux fixtures:** [W9 memory-format probe](evidence/w9-memory-format/README.md), [reference OOM observation](evidence/w9-reference-137/single-1655/cgroup-oom-observed.txt), [failed W9 comparison](evidence/w9-d20-final-failure/README.md), [W9 compact comparison](evidence/w9-compact/README.md), [W9 recovery tooling](evidence/w9-recovery-b111/README.md), [W9 builder preflight failures](evidence/w9-builder-preflight/README.md), [W9 artifact boundary](evidence/w9-artifact-boundary/README.md), [W9 preflight attempt](evidence/w9-preflight-6198/README.md), [W9 exact-import projection](evidence/w9-exact-imports/README.md), [route review](evidence/routes-449/README.md), [route matrix](evidence/routes-ed908/README.md), [residual Linux checks](evidence/linux-residual-449/README.md), [QEMU checks](evidence/linux-qemu-449/README.md), [procps exit-race replay](evidence/procps-exit-races/README.md), [ethtool fixture](evidence/ethtool-fixture-7d014/README.md).

**Typing, assignment, and acceptance reviews:** [typing witnesses](evidence/typing-witnesses-b6d4/test.stdout), [alias red baseline](evidence/typing-alias-red-6f53/constraints-module.stdout), [alias failed candidate](evidence/typing-alias-failed-3b7c/constraints-module.stdout), [alias green candidate](evidence/typing-alias-green-b556/constraints-module.stdout), [intermediate typing checkpoints](evidence/typing-intermediate-checkpoints/README.md), [typing failure detail](evidence/typing-intermediate-checkpoints/README.md#source-based-module-export-failure), [scheme checkpoints](evidence/typing-scheme-checkpoints/README.md), [row checkpoints](evidence/typing-row-checkpoints/README.md), [assignment evidence](evidence/assignment-52c0/README.md), [ten-row semantic review](evidence/acceptance-review-45b4/semantic.md), [open-defect review](evidence/acceptance-review-45b4/open-defects.md), [8d acceptance follow-up](evidence/acceptance-follow-up-8d/README.md), [915 acceptance follow-up](evidence/acceptance-follow-up-915/README.md), [lint rules](designs/lint-rules.md#assignment-safety-facts), [inference design](designs/inference.md).

**Binding and winddown records:** [binding-reader output](evidence/binding-reader-2cd/README.md), [binding-reader comparison](evidence/binding-reader-2cd-comparison/README.md), [binding-import repair](evidence/binding-import-5366/README.md), [retained winddown receipts](winddown/README.md), [historical lane review](winddown/lane-review.md), [original branch index](winddown/branches.tsv), [detached recovery tags](winddown/detached-refs.tsv), [imported source refs](winddown/standalone-source-refs.tsv), [unfinished patch index](winddown/unfinished/index.tsv), [standalone patch index](winddown/unfinished/standalone-index.tsv).

Read [the campaign](CAMPAIGN.md), [the typing and formal successor requirements](CAMPAIGN.md#typing-and-formal-successor-requirements), [the performance policy](CAMPAIGN.md#successor-performance-policy), [the item inventory](ITEMS.md), and [the test owner](../../docs/TESTING.md) for approved contracts and gate definitions. Older chronological entries in those documents are historical, not a live queue.
