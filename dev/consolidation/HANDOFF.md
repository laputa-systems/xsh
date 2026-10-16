# Consolidation handoff

Status: resumed by the owner on 2026-10-16. Lean and Phase F stay paused.
Inference remains isolated until consolidation acceptance permits its merge.
The [stop handoff](CAMPAIGN.md#stop-handoff-2026-10-16) records the starting
state, parked branches, retained worktrees and completed stop audit.

## Active resume

The integration tree was clean at `e048b33f2`. The integrator cancelled the
closeout guard before selecting work. The resume assignments are:

| Lane | Worktree below `.work/wt/` | Base and scope |
|---|---|---|
| Inference ownership | `typing-pipeline-integration` | `e2cf1fec2`; exact generic exports, retained values by binding identity, and required regressions |
| Acceptance | `resume-acceptance` | `e048b33f2`; approved supplemental and canonical gates |
| Loop speed | removed after read-only review | `e048b33f2`; completed the selector and repeated-validation audit |
| W9 parity | `resume-w9` | `e048b33f2`; immutable capture admission and strict comparison |
| Routes and resources | `resume-routes` | `e048b33f2`; current route matrix and remaining resource gates |
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
38 controls successfully. The supplemental attempt
`correctness-e048-20261016-source-root` is active using frozen tools in
`resume-acceptance` and source in detached `consolidation-final-e048` at
`e048b33f2`. It preserves the five contexts and their existing resource limits.
No supplemental result is accepted yet.

W9 comparison repair `fa58a4e54` shares each validated derived record between
correction-delta detection and parity. The integrator reran all 86 adapter-runner
controls successfully. For the frozen corpus, the removed second pass read
6,760 record groups containing 27,040 sidecar files. This is operation accounting,
not a measured speedup. Full parity remains open. Its retained receipt pins the
historical controller at the primary checkout path, so current main fails exact
controller admission. The W9 owner is checking a private historical replay
namespace with unchanged receipts and admission code before another attempt.

The strict matrix additionally needs 60 unique non-expression runtime selectors
in both modes. They are disjoint from the 92 expression probes and 32
source-order selectors. The route owner prepared 184 exact behavior executions
alongside the 184 route probes. Non-expression results earn no route artifacts.

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
