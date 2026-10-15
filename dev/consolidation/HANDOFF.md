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
- Assignment parity fixtures pass against the old collector: five tests cover
  late writes, nested comparison candidates, target roots, excluded statement
  regions, original scope visibility, and erroneous checked output. The
  production traversal repair is pending. Its approved design is in
  [lint rules](designs/lint-rules.md#assignment-safety-facts).
- The checked assignment producer is integrated at `266421477`.
  Existing lexical preparation collects source-and-range-owned spelling sets.
  Private storage preserves the public `ResolvedBindings` layout. Ten focused
  tests pass, including error output, raw sugar regions, and owner separation.
  The consumer is still pending. The classifier repair at `7afa88e06` requires
  explicit decisions for future expression variants. Both existing latent
  stream tests pass. [Focused output](evidence/resume-assignment-producer.log)
  retains these checks. The frontend facade export is repaired at `f04cbfd6f`.
  No final campaign gate has run on this batch.
- Verification `xsh` and `xsht` products were rebuilt at `2688d52fc` for the
  assignment repair's output baseline. Repository and Laputa check and lint
  each exit zero. Preserve their complete outputs for candidate comparison.
  Compare diagnostics before the closing timing line, as the integration
  harness does. These runs do not establish performance acceptance.

### Current queue

The integrator owns `master`. The assignment-fact repair, current diagnostic
feature-touch probe, and route-evidence helper recovery have isolated worktrees
under `.work/wt/`. Their branches are not accepted implementation until
reviewed and verified. The feature scaffold is diagnostic source only and
must never merge as a language feature. Source changes require a new canonical
freeze after integration.

The assignment repair removes the independent lint prepass while preserving
source-only rules, late writes, and diagnostic/fix ordering. The feature-touch
audit has found a latent stream-error classifier with a compiling wildcard
that requires a separate exhaustive-classifier repair. Finish its manual-site
inventory before accepting the feature-touch recount. Keep the scheduler
comparison's original default workers and outcome checks.

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
