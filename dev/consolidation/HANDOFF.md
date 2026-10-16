# Consolidation handoff

Status: active and incomplete at main commit
`7b1485310abdf2190ce4532d840114e74ff7c2cd`, tree
`0f3394902504cbe5271edd8916f504bd493f091a`. No functional close or performance
credit is recorded. The live checkpoint below supersedes older status summaries
in this file and in `CAMPAIGN.md` and `ITEMS.md`.

## Latest canonical run

The full native run on main source `7b1485310abdf2190ce4532d840114e74ff7c2cd`
started at 2026-10-16 15:21 UTC and terminated with receipt status
`failed-or-incomplete`, exit 3. Cleanup resolved, and the source was clean
through the end. The native suite reports 13,902 passed, 3 failed, and 78
skipped. The run does not pass canonical acceptance.

Terminal stage results:

- Root Rust: 747 passed, 3 ignored.
- Root integration: 223 passed.
- XSHT: 116 passed, 5 filtered.
- Soundness: 14 passed.

The root Rust, root integration, XSHT, and soundness stage counts above pass.
Three exact failures remain in the terminal report:

- GNU rm ext3perf exited 3 with empty stderr. Its failure entry is at stdout
  line 9,121; the run does not establish that this was a timeout.
- uu-sort `buffer_sizes` timed out after 4 minutes and was canceled with signal
  15.
- `dev/tests/test-system-report-check.xsh::test_system_report_pci_capture_replays_raw_identity_links_and_rejects_tampering`
  failed with EAGAIN (errno 11) in 9 ms; its failure entry is at stdout line
  15,061.

The [canonical 7b evidence](evidence/canonical-7b148/README.md) preserves the
receipt, full logs, and resource record.

The default-product restore passed on the 7b source, with the `xsh` bootstrap
matching. It supports downstream focused gates and does not change the failed
canonical result. Three hostname cases pass on source-identical fix
`63734367c`. Driver review reports all 184 route outputs pass on 7b with stable
hashes, along with 32 source-order checks. The route collector is blocked
because it expects one top-level AST match tag while the reviewed dispatch uses
`StmtView`. Driver review owns the collector repair. These runtime results do
not earn route-matrix credit.

The current source check passed on this main tree. It reports
`blanket_dead_code_lines=0`, `lowering_type_resolver_definitions=0`,
`manual_payload_calls=0`, `optional_raw_calls=0`, and
`wide_arena_wildcards=0`; 152 instructions, 17 patterns, 46 stages, 5
factories, 1 predicate, and 2 storage mappings. The source count is 268,495
lines; growth is advisory.

## Other active evidence

- **W9 final comparison:** the 7b final attempt stopped at helper compile error
  E0061. Isolated repair commits `f1e1d08` and `8932aaed` have reviewed
  reference, candidate, and derived builds (`fresh=false`), one passing
  structured fixture per role, and 270 passing Python tests. Final integration,
  fresh 1,690-input capture, and 6,760-outcome parity remain pending. These
  checks qualify only the isolated pins.
- **Profile-cost fallback:** cleanup commit `322889` is isolated and not integrated.
- **Diagnostic baseline:** qualification remains open because the source of
  older `27ec`/`0f204` binaries is unknown. This is separate from fallback
  cleanup `322889`.
- **Correctness-input plan:** current proposal commit `c6616fad` has 15 passing
  Python tests. The owner approved its replacement gates: one verification
  product build, three harnesses, five Linux contexts, and workspace Clippy.
  The gates have not run. Independent review, native-environment approval
  recording, and pin recomputation are pending. Commit `8dae13e` is superseded
  as the current proposal.
- **Arc storage comparison:** root accepted the isolated shared-storage change
  after source, evidence, and ownership review. The capture integration worktree
  now fast-forwards from `46ce` to source commit
  `e4d282457672a1fc87464c051820ebb64a9eb857`, tree
  `aa7eda1ae791a622cf831937a3f45cb2f94c3a02`. Sixteen candidate controls and
  five matched pairs pass at that pin, with lower candidate allocation traffic.
  Evidence commit `cfcdd2737a941468d5d680d4e2418f289c0065bf` records the source
  pins, checks, allocation pairs, and binary sizes in
  `dev/consolidation/evidence/typing-capture-memory-e4/`. This is separate from
  generic inference source `7a7418b5`, remains off main, and gives no time, RSS,
  or general performance result. The fast-forward adds no new runtime pass and
  this optional change is excluded from the first slice.

## First isolated typing slice

The owner authorized closing one coherent slice in isolation while
consolidation continues. `typing_scheme_encoding` is its sole coherence owner.
The target is the existing `typing-pipeline-integration` worktree: integrate
mandatory capture identity source `46ce` into pipeline source `7a7418b5`. Do not
include the optional Arc storage change.

The combined source still needs the existing [six-case capability matrix]
(evidence/typing-witnesses-b6d4/test.stdout), verifier checks, native tests,
the frontend-drop check, and both route gates. These gates are pending on the
combined source. Integration is in progress with six conflicts. Pipeline source
`7a7418b5` passed 17 Rust and 20 native
selectors; evidence commit `903004f98320ac5b5aef80ce8ff7144beb971a8b` records
them at `dev/consolidation/evidence/typing-pipeline-7a741/README.md`. Capture
source `46ce549a` passed 15 Rust selectors; evidence commit
`cdeeebcd7aaf23d8821d4f4107b357021ffafdc2` records them at
`dev/consolidation/evidence/typing-capture-46ce/README.md`. These remain
component evidence only and do not accept the combined feature.

Isolated integration, convergence, and acceptance work are authorized now,
including lowerer work. Main integration remains blocked until consolidation
closes. After this first slice closes, continue with the next composition in
the existing approved goals. Lean and Phase F remain paused.

## Next acceptance steps

1. Preserve the terminal canonical receipt. Triage the three failures, review
   repairs separately, qualify a new source freeze, then rerun affected gates.
   The default products are restored and `xsh` bootstrap-matched on 7b.
2. Close the first isolated typing slice on combined source using its named
   capability, verifier, native, frontend-drop, and two-route gates. Keep main
   integration blocked until consolidation closes; compose the next slice after
   this one closes.
3. Resolve the route collector mismatch and qualify the route matrix. The 184
   route outputs and 32 source-order checks pass on 7b, but earn no matrix
   credit while collection is blocked. Then finish final W9 capture and
   comparison on qualified common program inputs. Require all 1,690 inputs and
   6,760 outcomes to pass before claiming parity.
4. Close the ten semantic review rows and remaining functional acceptance
   work. Keep evidence tied to its exact source commit.
5. Only after functional close, run the required resource, profile,
   forced-frame, and scheduler comparisons. Apply readiness checks before and
   after time-based measurements.
6. Complete independent review and record native-environment approval for the
   owner-approved correctness-input gates, then recompute their pins. The gates
   are approved but not executed. Phase F stays paused unless the owner resumes
   Lean.

Run only one full native suite at a time. Development and final platform gates remain x86_64 Linux musl in the pinned test image. The campaign has no accepted performance result yet.

## Historical evidence links

The links below are retained from earlier handoffs. They describe their own source pins and limits; none substitutes for the live 7b gates above.

**Canonical, fixtures, and source checks:** [fixture repair](evidence/fixture-fixes-577b/driver.stdout), [failed 0d6 canonical receipt](evidence/canonical-0d6/canonical-replay.json), [0d6 product sidecar](evidence/canonical-0d6/actual-products.json), [cross-file AWK cause](evidence/awk-cross-file-0ea/README.md), [bounded AWK replay](evidence/awk-replay-0ea/README.md), [Clippy batch](evidence/clippy-4d7b/README.md), [Clippy scope outputs](evidence/clippy-ed908/README.md), [canonical e89 receipt](evidence/canonical-e89/canonical-replay.json), [e89 stdout](evidence/canonical-e89/stdout.log), [e89 stderr](evidence/canonical-e89/stderr.log), [e89 resource capture](evidence/canonical-e89/container-resources.json), [e89 bootstrap receipt](evidence/canonical-e89/bootstrap-receipt.json), [canonical 787](evidence/canonical-787/README.md), [canonical ed908](evidence/canonical-ed908/README.md), [canonical 45b4](evidence/canonical-45b4/README.md), [canonical 0d6 resources](evidence/canonical-0d6/actual-products.json), [structural scan](evidence/structural-787/README.md), [source review](evidence/source-review-6198/README.md), [assignment producer output](evidence/resume-assignment-producer.log), [merged project-test output](evidence/resume-a86d4-project-tests.log), [interrupted canonical output](evidence/resume-fff13082-canonical.log), [interrupted canonical resources](evidence/resume-fff13082-resources.json), [lint-flow evidence](evidence/lint-flow-d2c8/README.md), [feature-touch recount](evidence/feature-touch-d2c8/README.md), [replacement-label soundness evidence](evidence/fuzz-replacement-1d64/README.md).

**W9, routes, and Linux fixtures:** [W9 memory-format probe](evidence/w9-memory-format/README.md), [reference OOM observation](evidence/w9-reference-137/single-1655/cgroup-oom-observed.txt), [failed W9 comparison](evidence/w9-d20-final-failure/README.md), [W9 compact comparison](evidence/w9-compact/README.md), [W9 recovery tooling](evidence/w9-recovery-b111/README.md), [W9 builder preflight failures](evidence/w9-builder-preflight/README.md), [W9 artifact boundary](evidence/w9-artifact-boundary/README.md), [W9 preflight attempt](evidence/w9-preflight-6198/README.md), [W9 exact-import projection](evidence/w9-exact-imports/README.md), [route review](evidence/routes-449/README.md), [route matrix](evidence/routes-ed908/README.md), [residual Linux checks](evidence/linux-residual-449/README.md), [QEMU checks](evidence/linux-qemu-449/README.md), [procps exit-race replay](evidence/procps-exit-races/README.md), [ethtool fixture](evidence/ethtool-fixture-7d014/README.md).

**Typing, assignment, and acceptance reviews:** [typing witnesses](evidence/typing-witnesses-b6d4/test.stdout), [alias red baseline](evidence/typing-alias-red-6f53/constraints-module.stdout), [alias failed candidate](evidence/typing-alias-failed-3b7c/constraints-module.stdout), [alias green candidate](evidence/typing-alias-green-b556/constraints-module.stdout), [intermediate typing checkpoints](evidence/typing-intermediate-checkpoints/README.md), [typing failure detail](evidence/typing-intermediate-checkpoints/README.md#source-based-module-export-failure), [scheme checkpoints](evidence/typing-scheme-checkpoints/README.md), [row checkpoints](evidence/typing-row-checkpoints/README.md), [assignment evidence](evidence/assignment-52c0/README.md), [ten-row semantic review](evidence/acceptance-review-45b4/semantic.md), [open-defect review](evidence/acceptance-review-45b4/open-defects.md), [8d acceptance follow-up](evidence/acceptance-follow-up-8d/README.md), [915 acceptance follow-up](evidence/acceptance-follow-up-915/README.md), [lint rules](designs/lint-rules.md#assignment-safety-facts), [inference design](designs/inference.md).

**Binding and winddown records:** [binding-reader output](evidence/binding-reader-2cd/README.md), [binding-reader comparison](evidence/binding-reader-2cd-comparison/README.md), [binding-import repair](evidence/binding-import-5366/README.md), [retained winddown receipts](winddown/README.md), [historical lane review](winddown/lane-review.md), [original branch index](winddown/branches.tsv), [detached recovery tags](winddown/detached-refs.tsv), [imported source refs](winddown/standalone-source-refs.tsv), [unfinished patch index](winddown/unfinished/index.tsv), [standalone patch index](winddown/unfinished/standalone-index.tsv).

Read [the campaign](CAMPAIGN.md), [the typing and formal successor requirements](CAMPAIGN.md#typing-and-formal-successor-requirements), [the performance policy](CAMPAIGN.md#successor-performance-policy), [the item inventory](ITEMS.md), and [the test owner](../../docs/TESTING.md) for approved contracts and gate definitions. Older chronological entries in those documents are historical, not a live queue.
