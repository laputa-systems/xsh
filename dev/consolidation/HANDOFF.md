# Consolidation handoff

Status: active frozen source `b235a1a43d06e44580fde4c1b8c58ff27a17ca78`,
tree `08402ddd079228e072f24642fdb82db5196fff11`. Main remains on this source.
Lean and Phase F stay paused. Inference remains isolated. The 2026-10-16 stop
handoff below is historical.

## Active candidate

The active source is `b235a1a43d06e44580fde4c1b8c58ff27a17ca78`, tree
`08402ddd079228e072f24642fdb82db5196fff11`. The canonical replay is running
from `.work/tmp/canonical-b235a1a43-20261017-r1` in the frozen checkout.
Integration (223), symbol (1), boundary (23), extra (58 passed, 3 ignored),
debug (748 passed, 3 ignored), XSHT (117), soundness (14), privileged extra
(82 passed, 4 ignored, 4 filtered), and Linux privileged (10) checks passed.
The 13,994-case full native selection is still running. It has no terminal
result or acceptance credit.

The candidate includes the XSHT linked-worktree discovery repair at
`c30aed3bc`, the same-size tail fixture at `afe2998e4`, the batch admission
change at `2d882cf6`, the W9 producer repair at `283dcdeab`, and the native
`nc` fixture repair at `94b772ac0`. The root-reviewed tail fixture repair is
`16abfbed3`. It preserves same-size stress-test writes and both truncation
controls. No production Rust or XSHT source changed after `0deeb9c7`.
Scheduler behavior, selected cases, privileged plans, and deadlines remain
unchanged.

### Integrated evidence

| Lineage | Pinned result and limit |
|---|---|
| Coverage discovery `fd008a789` | On its lane pin, 18 Rust tests, four native cases, Clippy, and product builds pass. These results do not qualify the assembled source. See [coverage discovery](evidence/coverage-discovery/README.md). |
| Route expectation `1c0187f40` | The 0dc9 replay completed 428 invocations before the expectation correction. It admitted 187 of 188 route proofs and left two normal `ExprIf` gaps. A fresh strict replay is required. See [route evidence](winddown/evidence/route-if-expectation-0dc9cff7/README.md). |
| Failed canonical `9080e505f` | The 0dc9 run reports 13,906 passes, 9 failures, and 78 skips. Four coverage failures and one tail-fixture failure are reproduced and repaired. Four timeout/rm causes remain unproven. See [canonical evidence](evidence/canonical-0dc9/README.md). |
| Canonical `0deeb9c7` | The complete attempt reports 13,915 passes, one failed `nc` TELNET fixture test, 78 skips, and exit status 3. The root-reviewed fixture repair is `94b772ac0`; its exact selector and focused file run pass, but the full canonical gate has not been rerun or accepted. The terminal receipt and logs are preserved in [QEMU evidence](evidence/linux-qemu-0deeb9c7/README.md). See also [fixture repair evidence](evidence/native-nc-repair/README.md). |
| Failed canonical `3c0c01206` | XSHT integration passed 116 cases, then failed the desugared `test_uu_tail_follow_name_hash_table_stress` after 1,854.26 seconds. The written file passed 79 tests; the desugared file passed 78 and emitted an extra `tail : stderr tail: 9: file truncated` line. Exit status 3 stopped the gate before the full native suite, privileged harness, four batch checks, and docs gate. The source was clean; this run predates the accepted fixture repair and earns no credit. See [failed 3c canonical evidence](evidence/canonical-3c0c01206/README.md). |
| Tail fixture repair `16abfbed3`, proof `845cc84f9` | The controlled probe observed the truncation window. All four exact selectors passed once in both written and desugared form. The test-only repair preserves the real truncation controls. The full canonical suite has not completed on b235. See [tail fixture evidence](evidence/tail-hash-table-stress-3c0c0120/README.md). |
| Canonical `b235a1a43` | The listed Rust, XSHT, soundness, and privileged checks passed. The 13,994-case full native selection remains active; terminal receipt and acceptance are pending. |
| `nc` fixture `94b772ac0` | The empty-stdin case could half-close before TELNET negotiation, leaving the old fixture spinning on empty reads. The test now keeps its write side open through negotiation and fails on early EOF. The exact selector and 27-case focused file pass. No production Rust or XSHT source changed. See [native fixture evidence](evidence/native-nc-repair/README.md). |
| Earlier R6 `957405a4` | All 13 commands exited zero on 0dc9, and root reviewed two traceroute skips. Reuse is blocked because XSHT source changed and workspace Clippy uncovered. Run a fresh R6 on b235 after canonical. See [0dc9 R6 evidence](winddown/evidence/final-correctness-gates/correctness-0dc9cff7-20261016-r1/README.md). |
| QEMU `5bdd8dd43` | Seven platform cases passed on 0dc9. This historical result does not qualify a later source. See [QEMU evidence](evidence/linux-qemu-0dc9cff7/README.md). |
| QEMU `b94c8ce1e`, b235 scope `64f426a71` | All seven guest cases passed on exact source `0deeb9c7`. The four b235 products currently match all four qualified hashes; reuse remains conditional on the canonical terminal `products_after` values. No b235 guest ran and no new guest credit is claimed. See [0deeb9 QEMU evidence](evidence/linux-qemu-0deeb9c7/README.md) and [b235 reuse scope](evidence/linux-qemu-b235a1a4/README.md). |
| Batch admission `2d882cf6` | Its focused regression passes. The batch gate caps its outer native workers at four. This change does not establish the cause of the earlier timeouts. See [admission evidence](evidence/canonical-admission-0dc9/README.md). |
| Compute `39175cc7` and filesystem `56e2f137` | Factor, sort, rm, and mv exact selectors pass individually on frozen 0dc9 products. These results do not explain the full-run failures. See [compute evidence](evidence/native-compute-0dc9/README.md) and [filesystem evidence](evidence/native-filesystem-repair-20261017/README.md). |
| Tail `a45771ea` | The corrected same-size fixture and both truncation controls pass on frozen 0dc9 products. The earlier unqualified proof is withdrawn. See [tail evidence](evidence/tail-same-size-fixture-afe2998e4/README.md). |
| W9 producer `283dcdeab` | The accepted source/test change keeps the three-worker cap and reviewed GC/lifecycle fixes. Its 64 focused controls pass; the integrated runner passes 91 tests. These checks do not qualify a full W9 cohort on this source. See [focused controls and width diagnostic](evidence/w9-comparator-width/README.md). |
| W9 parity `d92ab2aff` | Root accepted strict parity for exact source `0dc9cff7c2b16e4c201adb3abca11d7ebda8e1f4` and tree `b4f9db3f8ee2f38a6c62fea876976619db424ed7`: 1,690 inputs, 6,760 outcomes, zero mismatches. This proof does not transfer to the assembled candidate. See [durable W9 proof](evidence/w9-current-0dc9/README.md). |
| W9 parity `7be37796b` | Root accepted strict parity for exact source `0deeb9c7fb79e52740c82a264754dbb29d8be693` and tree `ffb7736e1c141bbacde71c7a036ef6bcf1433c34`: 1,690 inputs, 6,760 outcomes, zero mismatches, and verified assignment proof. The b235 diff changes no `src/`, `crates/`, `stdlib/`, or W9 publication tool. [Phase A](CAMPAIGN.md#phase-a--finish-the-existing-campaign-without-expanding-it) permits reuse for the unchanged W9 scope. See [durable W9 proof](evidence/w9-current-0deeb9/README.md). |
| Interrupted W9 repeat `d0e5ac4a2`, correction `a83c77655` | The 3c comparison stopped after 250 of 1,690 inputs and 1,000 of 6,760 outcomes. It observed zero mismatches in that prefix. Signal 15 left the attempt incomplete, with no parity credit. Root removed three raw payload directories after the reuse decision and freed 22.52 GiB. Payload-level verification is no longer replayable. See [interrupted comparison evidence](evidence/w9-current-3c0c-interrupted/README.md). |
| W9 scope check and input-0 probe `342ae4108` | The source-seal receipt binds b235. The input-0 probe passed for reference, derived-reference, and candidate adapters and both exact-owner roles. No b235 full-corpus capture ran because the shared publication lock was held by the 3c comparison. The README records Phase A reuse; the probe is not a full-corpus result. See [b235 W9 scope evidence](evidence/w9-current-b235a1a43/README.md). |
| W9 width diagnostic `f63b64baf` | The rejected eight-worker memory diagnostic reached the 16 GiB limit using frozen 0dc9 captures. The original three-worker cap remains. This diagnostic grants no parity or resource acceptance. See [diagnostic record](evidence/w9-comparator-width/README.md). |
| Compatibility inventory `b235a1a43` | Frozen denominators remain 638 BusyBox IDs across 68 utilities, 514 GNU IDs across 61 groups, and 5,653 uutils IDs across 106 utilities. Native-origin counts are 638, 513, and 5,419. Exception entries are 0, 3, and 235. They overlap native-origin counts at 0, 2, and 1 IDs. The union is 6,570 native-origin IDs plus 235 exception-only IDs. The 282 exclusions are disjoint and grouped as 137 `clap-wording`, 89 `gnu-semantics`, 17 `help-text`, 3 `thread-model`, 34 `uutils-extension`, and 2 `uutils-internal`. Origin, exclusion, and offline-inventory assertions passed. Historical upstream reports retain failures; no b235 full upstream pass is claimed. See [compatibility accounting](evidence/compatibility-close-b235a1a43/README.md). |
| Structural and ten-row review `097192fd7` | `metrics.xsh check-current` passed at the exact b235 pin and the four hard-zero structural counts are zero. This is structural/source-review evidence only; route replay, current native execution, functional close, and timing remain open. See [b235 closure evidence](evidence/closure-b235a1a43/README.md). |

The 73 route mapping tests and 91 W9 adapter tests are earlier focused checks.
The accepted 0deeb9 W9 cohort remains reusable for b235 under Phase A because
the compiler and W9 scope are unchanged. The b235 input-0 probe produced no
full-corpus outcomes. The interrupted 3c repeat earns no W9 credit. Fresh
strict route coverage remains required.

Phase D whole-corpus typing work remains isolated in
`.work/wt/typing-corpus-b235`, currently at checkpoint
`17be3ff8fc8740868612ba94cc0ff0a187ce1e27` on freeze `a8d5bd910`. After 615
joint removals, 236 of 693 local annotations
(34.05%) and 379 of 2,190 internal annotations (17.31%) are removed. Eleven
root checks and the lower checks pass; the Phase D targets are not met. The
26-case paired witness runner is prepared but awaits the residual slot. Host
anchor and indirect host-obligation work remain isolated. Do not merge
inference to main before consolidation closes.

### Current queue

1. Finish the active b235 canonical replay and review its terminal result. Its four product hashes currently match the qualified 0deeb9 QEMU hashes; confirm terminal equality before reusing those guest results.
2. Run a fresh R6. The 0dc9 result cannot transfer because XSHT sources changed and workspace Clippy uncovered.
3. Run the strict route matrix for 214 IDs in normal and forced modes, 428 invocations total.
4. Run the 36-case root residual: 35 ordinary cases and one `SYS_ADMIN` case. Run the two `NET_RAW` loopback positives in a separate root context with `CAP_NET_RAW`; `tests.native` drops root and cannot qualify them.
5. Complete the remaining semantic, structural, documentation, and functional-close review. The active canonical replay alone closes none of these gates.
6. After functional close, run the campaign's ten full-suite resource and performance sequence, counting the active canonical replay as one. Root owns release readiness and main promotion.

The active canonical, R6, strict route, root residual, QEMU reuse, and
performance gates remain open. W9 parity is accepted at 0deeb9 and reusable
for the unchanged b235 W9 scope under Phase A. No functional close exists.
Do not merge inference work from typing branches.

## Historical resume record (superseded)

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
