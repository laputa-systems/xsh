# Consolidation restart handoff

Paused at the owner's request on 2026-10-11 for a computer reboot. The campaign
is **incomplete**. Do not resume implementation, tests, or profiling until the
owner resumes it. This checkpoint supersedes older progress rows; approved
contracts in `CAMPAIGN.md` and `designs/` remain authoritative.

## Location and preservation

- Integration: `/home/josh/d/laputa-systems/xsh-consolidation`, branch
  `consolidation`. Implementation HEAD at pause:
  `5d1c1623802be11bb3b297f43caeb652570a9f18`.
- Original XSH worktree remains on the campaign start commit
  `5e66b6b896a47d0c692739135d51200b007a2a67`; compatibility worktrees are
  separate and must not be reset, merged, cleaned, or have their jobs stopped.
- XSH lanes: `/home/josh/d/laputa-systems/xsh-consolidation-lanes/`.
- Laputa integration: `/home/josh/d/laputa-systems/laputa-consolidation`, branch
  `consolidation`, at `048592b0473bcee57b5dd0573297115ab78ef633`. The symlink
  migration lane is under `laputa-consolidation-lanes/symlink`; no Laputa edits
  were required. Original Laputa remains untouched.
- All worktrees, Git objects, unfinished diffs, binaries, build caches, logs,
  profiles, launchers, and required evidence are on disk. Nothing required to
  resume is stored in `/tmp` or `/dev/shm`.
- An extra snapshot of all 32 campaign worktrees is saved in integration
  `.work/consolidation/restart/state/manifest.json`. Each tree has staged and
  unstaged binary patches, a status file, and copies of untracked source files.
  These are backups, not instructions to overwrite the live worktrees.
  `.work/consolidation/restart/snapshot.py` recreates the snapshot.
- The six dirty lanes are checker-foundation, coverage-baseline, fuzz-grammar,
  grammar, lower-bindings, and partition-lower. Preserve their current files.
  The baseline coverage lane contains measurement-only overlays, not candidate
  implementation to merge wholesale.
- `.work/consolidation/restart/temp-link-audit.json` records 25,942 examined
  entries under campaign evidence directories and no temporary-directory
  symlink dependencies. Container temporary test inputs can be regenerated
  from the retained source/fixtures/seeds.

The goal is paused, not complete. All active agents were instructed to stop.
Owned test/build containers, queued lock waiters, and lane processes were
stopped/reaped. Unrelated compatibility and other user processes were left
alone. Interrupted gates must be rerun; cancellation is not verification.

## Owner decisions to retain

- Implement the entire approved campaign. Feature and structural completeness
  precede stricter performance gates. Do not treat a partial implementation as
  completion after restart.
- Parallelize ready, disjoint lanes using **gpt-6.1-sol medium** for routine
  work and **high** for complex work. No Luna. Use the orchestrate skill and
  exact source ownership; do not create worktrees or build caches in `/tmp`.
- Approved contracts override contradicted old tests. Retain coverage, update
  the incorrect expectations, and record each migration. Approval is already
  given; do not ask again for the recorded conflicts.
- Development verification uses native **x86_64 musl in the exact
  `Dockerfile.test` image**. Final verification requires aarch64 musl in that
  same environment. Do not substitute a host toolchain or libc.
- Exclude **dev/compat from check and lint while it is WIP**. Preserve its
  explicit tooling/tests. XSH and sibling Laputa otherwise remain in scope;
  avoid heavy Laputa checks.
- No formatter/autofixer, push, new dependency, or broad automatic checks.
  Existing tests that format temporary corpus copies are retained as behavior
  coverage. Test process boundaries with optimized binaries; debug is allowed
  for root library units and narrow compile checks.
- Generated docs are not hand edited. Edit templates/snippets and regenerate
  with `make docs` once the implementation and contracts settle.

## Integrated implementation

The integration branch contains the approved campaign and skill, verification
profile/gate tooling, four pure partitions, child enumeration, structural
scanner/baseline/envelope, and starting feature/comparative evidence. These
prerequisites do not close their semantic workstreams.

Accepted behavior slices include bounded diagnostic excerpts, imported enum
display, buffered exec regressions, boundary contexts, lexical BindingId
facts, the 32 nested-machine limit, socket net effects with clause migrations,
ordinary Optional/named-argument diagnostics, required FsRoot.symlink labels,
opaque FsLock/evaluator tokens, scope-owned root/lock transfer and cleanup,
one borrowed static-type predicate, direct checked stage lowering, common
lint/traversal removals, and the shared user-module graph loader.

Recent integration commits:

| Commit | Slice / completed focused evidence |
|---|---|
| `771ad5a4` | R2 scope ancestry, descendant resource transfer, roots/locks released after defers; 135 unique native cases and eight host invariants |
| `46b87fc6` | One borrowed runtime static-type predicate; 120 native passes, one DNS fixture skip, two units, zero allocation/no-pull invariant |
| `12925062` | Checked stage lowering without whole-program/body clones; 29 native cases; bounded allocation improvement recorded with profile caveat |
| `5d1c1623` | One user-module graph loader for runtime/lint; missing import and invalid UTF-8 contracts; 38 native and 13 units across accepted loader slices |
| `9af93baf` | Lexical identities; 120 native cases and five units |
| `b160fecd` | Optional guidance, label insertion fixes, 97 symlink caller migrations; 35 native cases, four Rust tests, caller/light Laputa checks |
| `1cec6df9` | Eleven ordered lint expression hooks; cumulative removal work continued in the unintegrated lint commit below |

All 116 directly checked Any migration sites are integrated across tokei,
collections, stdlib, NVMe/EFI and system-report tests. The **Any checker is
not yet integrated**; broad discovery of embedded-script conflicts remains.
Do not count migration checks as a whole integration gate. The full system
report dev harness remains unresolved: 223 passed, one powercap replay failed;
that case then passed individually with both baseline and candidate. Preserve
the failure and investigate/recheck rather than calling the module green.

## Ready commits and unfinished lanes

Paths below are relative to the XSH lane directory unless specified. Read the
durable lane handoff before cherry-picking, rebasing, or resuming a build.

| Lane | Preserved state | Resume detail |
|---|---|---|
| partition-lower | **`1cd0e625` unintegrated**: authoritative 158-instruction schema, generated rows/codecs/effects and borrowed views; warning-free all-feature library build, 95/95 units; production net -1,105, 4,369 touched. Separate 86 Operands/72 Scheduled class draft is uncommitted and its queued gate was cancelled. Module companion hooks remain uncommitted deliberately. | `.work/consolidation/w1/REBOOT-HANDOFF.md`, `classes-paused.patch`, layout proof |
| partition-modules | **`96ddf70f` unintegrated**: checked module-command plans/conversions; warning-free optimized build, 30 native and 95 schema units; production +181 within +250. | `.work/module-command/REBOOT-HANDOFF.md`. Order: core `910bd163` (already integrated as `38e92981`) → W1 `1cd0e625` → hooks `96ddf70f`. Local W1 equivalent is `2191aeec`. |
| partition-lint | **`bf62c20e` unintegrated**: removes lazy visitor recursion, preserves finding order; before observable oracle green, after 281 lint units green; cumulative W6 -393 non-test lines. Clean. RuleSelection facade patch is unapplied and unapproved. | Integration `.work/consolidation/infra/lint-rules/PAUSE-HANDOFF.md` and `rule-selection-facade.patch` |
| lowering-costs | **`012a18c5` unintegrated**: parameter name indexing/checked argument grouping; 19 native cases; production -3. Clean. Initial 16,000-field guard timeout retained; exact retry passed in 1.7s, threshold unchanged. | `.work/lowering-costs/`; stage commit `9917f3cf` is already integrated as `12925062`. |
| any | **`020685bd` unintegrated** opaque Any checker; focused 102/102 native cases, production +32/487 touched. The broad run was queued then cancelled before starting. | `.work/any/HANDOFF.md`; `.work/any/run-broad-checker.sh` acquires the shared suite lock. Candidate binaries and hashes retained. |
| checker-foundation | Uncommitted E: checked declarations/type-expression facts, removal of CompactDeclCollector and duplicate revalidation. Earlier compile green; final-source unit/native checks incomplete. Six modified/two new files, production-path net -118/1,062 touched. | `.work/checker-foundation/reboot-handoff.md`; source-aware helper/caller plumbing remains unassigned. |
| lower-bindings | Uncommitted BindingId slot adapter; pinned optimized build green and real argv-splice regression passes after frozen baseline failed at four sites. Wider focused gate never ran: timeout option error corrected in script, not rerun. Net -195/905 touched. | `.work/lower-bindings/HANDOFF.md`, `paused-draft.patch`, `checker-sites.patch`, metrics. Preserve checker facade additions when combining E/module hooks. |
| grammar | Uncommitted repair of deep counterexamples and generator rollback. Two final guards applied but uncompiled at pause. Last normal proof ~88.26% had seed1419 failure; deep proof previously 58.323% failed the unchanged 80% threshold. Net +47/919 touched. | `.work/grammar/PAUSE-HANDOFF.md`, `pause-handoff.json`, `pause-remediation.patch`; focused/normal/native/deep gates pending. |
| fuzz-grammar | Uncommitted W7B recognizer/corpus traversal draft; three library units and 11/13 soundness tests green; five mutant disagreements wait grammar repairs; 1,500-seed runtime property had timed out at 180s. | `.work/fuzz-grammar/`; authorize one bounded longer retry after grammar integration, without reducing seeds. |
| in-place-scheduler | Clean, no implementation started. Exact grants, class/proof design, reachability removals and tests saved. Frozen scalar execution allocations: 143 for one pass, 207 for 17 passes, delta64. | `.work/in-place/HANDOFF.md`; refresh on W1/module/R2 integration and seed an immutable disk cache after its writer stops. |
| coverage-baseline | Frozen production with documented measurement overlays. Canonical and early phase containers cancelled and removed; full LLVM baseline is incomplete. | `.work/consolidation/coverage/REBOOT-HANDOFF.md`, manifest, phase logs, source seals, raw profiles, preliminary reports. Do not restart an old whole driver blindly or duplicate completed phases. |

## Next coordination decisions and ownership

1. Reconcile W1 and module hooks together; retain registered signature pointer,
   scalar conversion order, default/flag validation and corrupted-plan tests.
   Do not cherry-pick companion hooks from two lanes. Runtime manual readers
   have not been migrated yet; starting count is **923 calls across 922 lines**
   plus 43 optional reads. Update scanner counting for the macro schema before
   relying on current instruction counts; otherwise it can falsely report zero.
2. Give source-aware CheckSession/helper plumbing one owner. Required helper
   signature is `check_compact_declarations(program, sources: &SourceMap,
   entry_source_id: SourceId)`. Check each module against its own source ID and
   text; no empty-text/root-source fallback. Update Evaluator, stdlib catalog,
   loader, telemetry and unit callers together. The empty-source helper currently
   classifies a real Boolean command flag as a word. E owns declaration/type
   facts; session adapter ownership was **not assigned before pause**.
3. E's exact four-line constraint_probe publication-history take/restore was
   approved immediately before pause. Semantic tables must stay available.
   Telemetry must count CheckedDeclarations and annotation/template/instance
   type facts. Do not use Arc::get_mut uniqueness: Checker derives Clone.
   Lower-bindings merges already-selected command sites once at publication.
4. Complete the BindingId adapter's focused gate, reconcile its two pipeline
   lookups with integrated stage lowering, then coordinate deletion of old
   PreparedConstants global/tail maps. Deletion was cancelled at pause and has
   **not** happened; do not remove fields while old readers still exist.
5. Apply and test the shared-loader's two stale project-module-path expectation
   migrations from check-session `.work/check-session/`, retaining missing-import
   behavior coverage. Resolve reveal_type publication versus entry legality
   explicitly against the approved same-diagnostics contract; do not invent a
   runtime spelling or silence a diagnostic to bridge the conflict.
6. Resume W2 instance marks using generated operand traversal and the single
   schema class column. Pure tag classification alone is insufficient: mark
   only instances whose transitive operands cannot run script code, with proof
   height 128. Preserve the independent verifier/cache/corruption oracles.
   Shared apply functions, scheduling all unmarked instructions and the single
   pipeline engine remain substantial unfinished work.
7. Resource R3 local affine checks/MayHoldResource/clone-on-return and R4 stream
   ownership/worker teardown remain **unstarted**. R2 releases explicit_run.rs
   to W2. Coordinate metadata with W1, type facts with E, and stream files with
   the evaluator owner. Remove reachability sweep only after owned streams work.
8. W6 still needs the complete rule table, scope facts, selection/listing, shared
   probe/check session and all remaining private descents. Review its prepared
   RuleSelection patch before granting facade edits. W5 still has remaining wide
   behavior matches and traversal consumers. W3 storage/type-resolution
   consolidation waits checked facts; its borrowed predicate is only one slice.
9. W9 per-module records/isolation/reuse and bundle view remain unfinished beyond
   shared loading and stage/spread lowering. Reuse is within the resolved graph;
   fresh fix-round arenas recheck affected bundles, never reuse an older arena.
10. Integrate Any only after broad embedded-script discovery/migrations and a
    focused candidate gate. Current direct migrations are not proof of hidden
    fixture completeness. Preserve all approved expected-error coverage.

## Verification and coverage limitations

Frozen release binaries are integration `.work/consolidation/infra/baseline-bin/`.
Initial native baseline: **5,996 passed, zero failed, 118 fixture/capability
skips**. Compression focused 52/0/3; initial XSH source check 1,162 clean and
lightweight Laputa 305 clean. These results precede subsequent implementation.
No full final integration or ARM pass exists.

The instrumented Rust baseline is not all green. It retained stale label/API
counts/policy offsets, a signal fixture race, corpus symlink/read failures and
grammar disagreement. Candidate fixes have their own focused regressions.
XSHT retained actual core formatter/desugar failures (`test-stat.xsh`,
`test-ln.xsh`), large layout/redundant-parens findings and a 15s lint-budget
failure under instrumentation. Layout copied discovery also included ignored
measurement `.work` scripts; record that contamination. Do not call instrumented
wall times performance evidence, skip failed coverage oracles, or rerun every
long phase automatically.

Canonical fuzz and native API phases were interrupted for reboot. The native
API suite had discovered 6,122 cases and recorded 519 successes, 14 failures and
three timeouts before interruption; final API JSON is empty, not a passing report.
Fuzz printed 10 passed/one failed during
shutdown, with generated `.replace` positional-argument rejections through
seed1482; retain the terminal failure evidence separately from the interrupted
phase. Linux privilege, final API/report and drift retry phases did not complete.
All 10,996 raw profiles (107.77 GiB) remain on disk; the inventory marks 103
shutdown-window/empty profiles for explicit validation before merging. Both
preliminary decode-gap reports remain on disk. They are
provisional upper bounds, not final runtime-reader coverage acceptance.

Corpus measurement used temporary read-only masks for the known directory
symlink loop and WIP compatibility before Rust corpus phases; masks were removed
before native tests and disappeared with stopped containers. Production walkers
have retained symlink/WIP/error regressions. Never claim an unmasked baseline.

## Environment after reboot

Native image expected ID:
`sha256:99e195582ba55dc427b1551022fbe56f8f344ed7838e971ecba86ce43a7733e9`.
Docker image/registry storage is on disk. Recheck that ID before using launchers.
Required target flags:
`-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol`.
Jemalloc is preloaded for compilation only, never for test execution.

Integration `.work/consolidation/infra/run-native.sh TREE build|run COMMAND...`
is the primary wrapper (12 compiler jobs, cargo.lock). Secondary and third
wrappers have separate locks and eight-job bounds; no shared writable targets.
Use a candidate's own PATH/XSH_BIN for behavior gates. Nightly artifact paths are
recursive `target/TRIPLE/PROFILE/build/.../out/`, not conventional deps paths.
`xsht test` accepts one filter. One full native suite at a time is guarded by the
hard-linked integration and coverage `native-suite.lock`; do not replace that
inode. Lock holders are gone, but retain the files.

ARM prep logs exist, but the **actual pause-time Docker state lacks**
`xsh-test:consolidation-aarch64` and `xsh-consolidation-aarch64-target`. Recreate
them using the unchanged `Dockerfile.test` when final ARM verification is due;
do not rely on earlier preparation text as current image evidence. QEMU is the
disk file `/usr/bin/qemu-aarch64`, not a temporary artifact. Binfmt registration
is kernel state and must be recreated after reboot, using the own-name helper
`.work/consolidation/infra/binfmt.py` and preserved `binfmt-register.log`.
Register only `xsh-consolidation-aarch64-5e66b6b8`, binding the host QEMU path and
the helper into the privileged native container. Preserve other registrations.
`run-arm.sh` and `cleanup.sh` are saved on disk. Revalidate registration/image
before final builds; do not launch ARM work while paused.

## Closing work still required

Generated SPEC/tour/reference docs are deliberately not regenerated yet. Update
R2 canonical cleanup prose (old text still says roots/locks are not scope-owned),
type/fact/session architecture, Optional/label guidance and resource API docs,
then `make docs`. Update TODOs only after each defect is actually closed.

The unchanged baseline scanner reports 189,274 src and 49,968 xsht non-test
lines; 47,705 lines fall under inherited blanket dead-code scopes. Intermediate
envelope ceilings are 193,174 / 50,368 / 52,005. Final closure requires net source
reduction, zero blankets and no envelope. Ten semantic review rows remain
pending; establish verified evidence consumed by the ratchet rather than
manually erasing Reviews. Current verification opt3/no-LTO profile is provisional;
measure opt1/2/3 build-plus-native cost in isolation after functional close.

Final feature probe (manual sites zero), reliability/fixed-seed gates, pinned
seven-tool comparative parity/timings, complete candidate Linux/ARM verification,
cleanup and the closing report all remain. No comparative timings were taken;
prepared suite has 35 parity cases and eight harness tests green. Continue to
campaign completion only after the owner's resume request.
