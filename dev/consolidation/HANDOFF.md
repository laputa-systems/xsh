# Consolidation restart handoff

Status: resumed 2026-10-15 by the owner for consolidation acceptance followed
by the typing successor. Consolidation acceptance is incomplete. Lean and
Phase F remain paused. Complete consolidation before changing language
acceptance or starting general-inference implementation.

## Current resume

The owner explicitly resumed the typing consolidation campaign after the
documentation refresh at `bfffd3af5e0002668e685e1711d7e6d9ee90ad51`.
The checkout was clean on `master`. The pinned x86_64 Linux image is present,
and no campaign containers or linked worktrees were running. At that initial freeze, product Rust, stdlib, applet, and test sources retained
the pre-winddown implementation.
Historical gate helpers require fresh paths and source qualification.

Acceptance launch, structural/semantic review, loop selection, typing witness
readiness, and historical AWK evidence were inventoried independently. No Lean
work is authorized by this resume.

### Verified since resume

- Locked verification products and root process-test artifacts built in the
  pinned image with Kache and mold. Cargo reused the existing target tree.
- The `fff13082f` structural scan passed without an envelope: manual payload
  calls, optional raw calls, wide arena wildcards, and blanket dead-code lines
  are zero. Static type predicates are one, type-storage mappings two,
  instruction rows 152, patterns 17, and stages 46. Five factory-call sites
  still require their semantic bound review. Source totals are advisory.
- An eight-root lint fixture recorded eight requests, one prepare, seven
  reuses, one checked record, and seven check hits for its shared user module.
  This is a record-cache observation, not one body traversal. It is preliminary
  evidence and does not close the source-pinned semantic review row.
- The historically extra AWK shard passed in written and desugared forms with
  one worker in a disposable source copy. The historical individual case,
  transformed source, and child diagnostics remain unavailable. This passing
  pair does not explain or close the old mismatch.
- A [bounded replay of the original 0ea source](evidence/awk-replay-0ea/README.md)
  selected twelve stream/process shards under root with eight workers. Written
  and freshly desugared forms both passed shard 001 and failed only the separate
  root-sensitive shard 008. The historical extra failure remains unexplained.
  AWK acceptance stays open. The replay is not a full-suite acceptance pass.
- The first canonical batch at `fff13082f` passed 305 root integration cases,
  with four ignored. Its debug unit target passed 715, failed two, and ignored
  three. Both failures inherited `/work/xsht-config.ini` through disk-backed
  temporary paths. The batch stopped before the remaining canonical stages.
  Timing was discarded. [The failed output](evidence/resume-fff13082-canonical.log)
  and [resource capture](evidence/resume-fff13082-resources.json) retain the
  actual observations. No normal full native suite ran in this attempt.
- Reviewed repair `a86d4a7123bed5aafde302b170342dfd4fb4a2da` is integrated.
  Config selection now has a private controlled filesystem predicate for
  unit tests. Public discovery behavior is unchanged. Both original failures
  reproduced before the repair; all five project tests pass against the
  merged source, including the added relative-entry case. [Merged-tree output](evidence/resume-a86d4-project-tests.log)
  preserves that focused pass. No test coverage was dropped.
- Scheduler reference qualification is integrated through `6fc1478a1`.
  The comparison helper validates the retained aggregate overlay against
  the three original scheduler commits. Its derivation and resource-snapshot
  checks pass. No scheduler comparison or speedup is established.
- Route-evidence helper recovery is integrated through `fe4986b6e`.
  Reconstructed inventory and operand-policy inputs are source review data.
  The merged helpers pass 60 semantic-matrix and 45 eager-atom selftests.
  Mapping validation finds 114 uncredited candidates and one artifact-capability
  mapping. These checks credit zero executed route tuples. Source locks require
  an explicit command and remain under `.work/`. EmptyMap child bytes are pinned
  separately from its complete test file. The pipeline verifier oracle covers
  a present input with the wrong category. Missing or truncated input remains
  a separate evidence gap.
- Seven assignment parity fixtures pass against the old collector. They cover
  late writes, nested comparison candidates, target roots, and excluded regions.
  They also cover original scope visibility, error output, diagnostic order,
  and statement-pair movement. The approved design is in
  [lint rules](designs/lint-rules.md#assignment-safety-facts).
- The checked assignment producer is integrated at `266421477`.
  Existing lexical preparation collects source-and-range-owned spelling sets.
  Private storage preserves the public `ResolvedBindings` layout. Ten focused
  tests pass, including error output, raw sugar regions, and owner separation.
  The classifier repair at `7afa88e06` requires
  explicit decisions for future expression variants. Both existing latent
  stream tests pass. [Focused output](evidence/resume-assignment-producer.log)
  retains these checks. The frontend facade export is repaired at `f04cbfd6f`.
  No final campaign gate has run on this batch.
- The lint consumer is integrated at `e5bd34e50`. It borrows checked summaries
  and collects SourceOnly assignments during the existing descent. The separate
  recursive assignment prepass is deleted. The follow-ups at `1f7b795b6` and
  `52c0f5399` preserve exhaustive statement-region and dynamic-require decisions.
  At the clean `52c0f5399` freeze, all ten merged-source XSHT unit selections and
  four exact native lint regressions pass. Actual abort status-23 children retain
  one authenticated route record in each mode. Baseline and candidate check and
  lint diagnostics match on the same repository and Laputa source cohorts.
  [The focused evidence](evidence/assignment-52c0/README.md) retains full output.
  These checks leave canonical and performance acceptance open.
- Route-capture repair `84668c961` preserves valid artifacts from nonzero child
  exits. Its regression fails before the repair. Three capture and twelve
  authentication unit tests pass afterward. Nonzero children without artifacts
  retain their original result and supply no route evidence.
- Native expression oracles at `67dc22efd` add 37 missing canonical cases.
  The PreparedConstant oracle at `21f20fea6` checks its record-default value.
  Baseline probes pass for 36 new tags and all twelve scalar expression tags
  in both modes. Abort's candidate check passes as recorded above. These probes
  do not close the source-pinned route matrix.
- The original expression-oracle migration is integrated at `95488faf2`.
  Forty-one cases now capture direct children. Their original behavior
  assertions remain where a different child is needed for one target record.
  All 82 baseline probes pass with one authenticated record each. The existing
  Match child also passes in both modes. Final candidate replay remains open.
- Assignment identity repair `0957cb97d` selects the shared binding at the
  assignment site. Context ownership, latent-stream updates, and projection
  invalidation retain that identity. Three decoy-stack invariants fail before
  the repair and pass afterward. Two existing assignment and transfer tests
  also pass. The nearest active refinement remains the scope owner. Remaining
  AST, proof, call, and command consumers are under review.
- Route-matrix recovery is integrated through `088da08c5`. Mapping validation
  requires all 152 schema tags and reviewed child sources for all 92 expression
  tags. The child sources match the completed probe table at `95488faf2`.
  The 184 supported mode expectations credit zero receipts. The remaining
  60 tags retain unsupported artifact mappings. Main and eager helper
  selftests pass 61 and 46 cases on the merged source. The eager helper now
  selects Rust commands for Rust verifier oracles. Pipeline runtime behavior,
  wrong-category rejection, and the absent-or-truncated gap remain distinct.
- Pipeline payload tests at `382381547` reject an absent input word and a
  payload truncated after the input ID. Both begin with a verified fixture
  and assert the exact decoder rejection after changing its payload length.
  The two new tests and the existing wrong-category test each pass. Evidence
  metadata at `52fc6694a` records all three rejection oracles separately from
  the native child and removes the covered malformed-payload gap. Merged main
  and eager helper selftests pass 62 and 47 cases. Route receipts remain zero.
- Disposable QEMU preparation passes its guard smoke. The guest uses the pinned
  image's userspace, matching Linux 7.2.9-2-stable kernel and modules, TCG/q35,
  two CPUs, 2 GiB, fixed RTC, and no network. Its temporary files stay on ext4.
  Guest-only BTRFS tools are installed. GNU 9.12 `date` and `cp` were rebuilt
  from the checksum-and-signature-verified official source for the clock and
  reflink probes. The pinned image has neither GNU tool, as corrected in
  `a7298ffa3`. Disposable assets remain under `.work/consolidation/qemu` and
  `.work/consolidation/gnu-reference`. Product tests await the final source and
  matching harnesses. The smoke establishes no privileged acceptance result.
- Binding-reader repair `2cd2baa86` is integrated. AST expressions and tails
  consume their published identities. Missing identities report existing
  unresolved-name errors before procedure or literal-word fallback. Command
  capture preserves its first selected ID. Proof creation retains the chosen
  binding. Stream classification checks that binding's scope owner. Recovery,
  constructor precedence, and the documented `error` module exception remain.
  All 18 focused unit selections pass. Fresh verification products pass four
  existing native item tests and match the constructor probe's baseline output.
  [Retained output](evidence/binding-reader-2cd/README.md) records the checks,
  source qualification, product hashes, and sizes. Canonical gates remain open.
- The fresh 2cd repository/Laputa comparison fails diagnostic parity.
  Imported standard modules receive lexical runtime IDs but no checker value
  bindings, so the identity lookup rejects valid namespace calls. Baseline
  2688 accepts the same source. [All eight command outputs](evidence/binding-reader-2cd-comparison/README.md)
  retain the failed observation. A minimal imported `system`/`unix` source
  reproduces it. Repair the namespace-only import facts before acceptance.
- Verification `xsh` and `xsht` products were rebuilt at `2688d52fc` for the
  assignment repair's output baseline. Repository and Laputa check and lint
  each exit zero. Preserve their complete outputs for candidate comparison.
  Compare diagnostics before the closing timing line, as the integration
  harness does. These runs do not establish performance acceptance.

### Current queue

The integrator owns `master`. Binding-identity repairs and W9 receipt recovery
have isolated worktrees under `.work/wt/`. Each lane
requires review and focused verification before integration. Source changes
require a new canonical freeze.

The checker audit's assignment and reader repairs are integrated. The imported
standard-module regression above is under repair in the isolated checker lane.
Requalify the repaired facts and readers on the final source. The invariants establish authority failures in
controlled checker states, not demonstrated user-visible failures. Existing
nearest-visible-name proof filters retain their identity checks. Changing
those filters could replace a real shadow and is outside this repair.

The diagnostic compiler replay at `52c0f5399` found 35 XSH and 19 XSHT source
sites requiring explicit decisions for a synthetic expression variant. The
two earlier classifier survivors now require compiler decisions. Replay the
probe after the checker repairs. Its synthetic source must never merge.

The archived W9 corpus is recovered. Its 1,690 file paths, bytes, modes, and
manifest match the pinned e240 source. The original legacy capture is missing.
Fresh captures must bind the same source and cohort before exact-owner proof
and candidate comparison. Historical ownership receipts cannot replace the
missing outcomes. Receipt recovery is in progress and establishes no parity.

The migrated expression children require explicit matrix requalification.
Keep the scheduler comparison's original default workers and outcome checks.

General inference implementation, its initial red/green witnesses, and the
400-variable solver observation have not started. Consolidation acceptance
remains the prerequisite. Lean and Phase F remain paused.

## Resume

Prompt: "Resume consolidation acceptance from this handoff. Keep Lean paused.
Follow the campaign's successor direction only after its prerequisites close."
For a typing-only resume, require consolidation acceptance first, then judge
Phases B–E independently of deferred Phase F. The explicit owner resume above authorizes consolidation and typing only.

First command, from the repository root:

```sh
git status --short
```

Read [the campaign](CAMPAIGN.md), [the successor requirements](CAMPAIGN.md#typing-and-formal-successor-requirements),
[the performance policy](CAMPAIGN.md#successor-performance-policy),
[the items](ITEMS.md), and [the testing owner](../../docs/TESTING.md).
All numerical typing targets and their current hard or aspirational status are
in the campaign. No deleted brief or old work directory is required.

## Winddown state

Integration remains on `master`. The executable source and test roots remain
unchanged from `633e6394df7e7492226a7c622cd3bad9f4eb2f86` during winddown.
The completed PTY-frame repair, root permission fixtures, desugar worker bound,
and six-digit PID separator repair were already integrated before this stop.
The owner steering is incorporated in the campaign; its standalone file is
removed. These documentation changes do not claim a new executable pass.

The 633 selected-profile native suite passed 13,793 cases, with zero failures
and 136 skips. Its observer completed 1,022 root Rust tests, then was
interrupted for winddown. Canonical acceptance did not complete. Isolated lint
exited zero but exceeded the existing 15-second budget. The historical extra
AWK desugar outcome remains unexplained. No scheduler or comparative speedup
is claimed. [Retained receipts and recovery instructions](winddown/README.md)
state the evidence limits.

## Winddown lanes

No lane or verification job remains in flight. The 824 original non-ancestor
branches and 92 additionally imported source-tip branches are parked. Their
source is recoverable through [the original branch index](winddown/branches.tsv),
[the detached recovery tags](winddown/detached-refs.tsv), and
[the imported source refs](winddown/standalone-source-refs.tsv).
[The historical lane review](winddown/lane-review.md) records changed scopes.

Automatic approval review rejected a mass archival merge because it would
mark competing, unintegrated alternatives as merged. Preserve the parked refs;
these histories are not accepted implementation. On resume, compare the
relevant source with current owners before choosing any unfinished patch.

All 50 dirty overlays, including staged alternatives and untracked fixtures,
are committed as recovery patches in the [linked index](winddown/unfinished/index.tsv)
and [standalone index](winddown/unfinished/standalone-index.tsv).
Each final patch was applied against its original tree in an isolated Git
index and the restored bytes compared with the original files. Diagnostic
probes and old scheduler baselines remain explicitly archival alternatives.

## Retained worktrees and disposable state

Only the primary repository checkout remains. All 557 linked worktrees,
79 standalone repositories, their 11 additional linked checkouts, and two
stale partial checkouts were removed. Both sibling consolidation directories
are gone. No worktree needs recreation until explicit resume.

The repository work directory is disposable, including caches, binaries,
old launch pins, and test fixtures. No required source or resume instruction
lives only there. Optional large historical captures are locally ignored in
`.campaign-evidence/consolidation`, outside that directory. They are not
checked into Git and are not required to recover source or resume work.
The committed small receipts retain historical path strings as observations,
not as files that must still exist.

## Not started

1. After explicit consolidation resume, qualify the source freeze and prepare
   current launch pins using the existing gate owners. Complete the unreached
   canonical, native, forced-route, privileged Linux, and soundness checks.
2. Apply the existing nonclosure policy to the unexplained AWK discrepancy;
   preserve the failed observation without inventing a cause.
3. Finish structural metrics, ten semantic review rows, docs, resource/size,
   scheduler, and comparative acceptance. Time-based gates require readiness
   before and after. Keep one full native suite at a time. The owner waived
   alternative profile-selection measurement, not correctness or other gates.
4. After consolidation closes and typing explicitly resumes, deliver the
   first end-to-end identity/projection/import/rejection slice through normal
   indexed execution. Retain solver counters and the observed alias-chain
   regression result. Then widen Phases B–E. No such new tests ran in winddown.
5. Phase F stays deferred until the owner explicitly resumes Lean. It does
   not block typing acceptance or authorize more disconnected proof lanes.

## Verified

- Git worktree pruning and listing retain only the primary checkout.
- All standalone source-ref histories and checkout states were checked before
  removal. Imported refs preserve 136 commits previously absent from the
  primary object database. Git connectivity checking passes.
- Recovery patches restore all 50 unfinished overlays. The two stale source
  trees have no unique source relative to their identified existing commits.
- Large historical captures were byte-verified, moved to local ignored
  storage, and removed from the unpublished commit that briefly included them.
- No campaign build/test processes or Docker containers remain. All team lanes
  are completed or interrupted. No user crontab or scheduling tool is present;
  the host has no `systemctl` for a systemd timer audit.
- The disposable work directory is on ext4 and excluded through local Git
  info/exclude. Optional historical capture storage is also locally excluded.
- The configured trusted Stop command was inspected and its closeout hook
  passed directly. The interactive hooks UI is unavailable in this session.
- Clean-environment resume and link checks passed. No new runtime tests,
  builds, performance measurements, or formal verification ran for winddown.

Historical detailed handoffs remain in Git history. Use this document for the
current stop; do not launch commands from obsolete chronological entries.
