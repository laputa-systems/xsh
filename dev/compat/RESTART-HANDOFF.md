# Restart handoff — 2026-10-11

Work stopped at the owner's request for a computer restart. The campaign is
incomplete. Resume this handoff before the historical notes in CAMPAIGN.md.

## Workspace and standing instructions

- Integration: /home/josh/d/laputa-systems/xsh-codex-compat, branch codex/compat-native.
- Integration HEAD before this handoff commit: 3dd2e310ea8027adc26b57f951687fe4601de50e.
- All lane worktrees: /home/josh/d/laputa-systems/xsh-codex-compat-lanes.
- Lane manifests, evidence, and handoffs: xsh-codex-compat-lanes/_scratch.
- Integration evidence and boundary handoffs: xsh-codex-compat/_scratch and .work.
- The original /home/josh/d/laputa-systems/xsh checkout belongs to the owner.
  Never reset, rebase, stash, edit, or move its HEAD. Specify the workdir on every
  command. An earlier accidental rebase was corrected; never restore an old
  recorded original-checkout hash because the owner has continued working there.
- Use exclusively gpt-6.1-sol, medium, fork_turns:none for new agents. No Luna.
  The owner requested maximum independent lanes; bound simultaneous builds and
  test memory independently of agent concurrency.
- No worktrees or persistent scratch in /tmp. Set TMPDIR to the integration
  .work/tmp for Python lane-generator tests. Disposable test fixtures in bounded
  container tmpfs are recreated by their committed tests and need not survive.
- No formatters, autofixers, pushes, or new product dependencies without asking.
- Licensing approved: original behavioral tests with GNU/BusyBox origin IDs;
  never copy GPL script text. Functional binary fixtures may be retained.

## Durable state and coverage

Everything necessary to resume is on disk outside /tmp: Git commits, uncommitted
lane source, original fixture backups, proof logs, compiler commands, reference
source copies, jail tools, and release binaries. Cleanup confirmations say no
campaign-owned test/compiler/container processes remain. Other users' processes
and containers were not stopped.

All 6,805 frozen origins have been authored or explicitly classified in lane
inventories. This does NOT mean the strict integration ratchet is complete.
The final read-only integration report is .work/origin-progress-restart.log:

- BusyBox: 638/638 mapped, all native and normative references validated.
- GNU: 455/514 mapped in the integration tree.
- uutils: 5,033/5,653 mapped in the integration tree.
- 679 still unmapped at HEAD, largely authored held ports and Rust mappings.

The retained origin checker is core/tests/origins/check_origins.py. Its freeze
and exception files must be finalized before harness deletion. The old
dev/compat/port copies remain while retirement is incomplete. Reviewed exceptions
are recorded in both locations (152 entries). They include 140 internal,
uutils-only, or empty-body scope decisions and older retained-boundary reasons.
The two proposed join exclusions were rejected: GNU 9.12 supports those cases
under C.utf8; their original test bytes/assertions now pass with explicit locale.

## Current binaries and reference environments

Last matched pinned Linux release pair is in
target/x86_64-unknown-linux-musl/release:

    xsh  1aa79bea1bf61d64259033674cb2efc07f54ec46558dd26e1f0f839fc6db3a5a
    xsht 47df8bffe609c02d47df7e03ccba94d6609e7ab57bc5cbaba2fce8a61c86af70

This pair includes the final SYSTEM namespace, locale facts, inherited exec cwd,
closed startup descriptors, and EAGAIN evaluation fallback. It PRECEDES the
integrated fallible allocation and seek-whence changes (5817a106). Rebuild before
testing those changes. Interpreted core script changes need no binary rebuild.
Never use target/release host artifacts as Linux evidence.

Linux campaign verification uses xsh-test, linux/amd64,
x86_64-unknown-linux-musl, the pinned compiler and these flags:

    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol

Build only xsh bins and xsht, with bounded RAM/jobs and reusable integration
target. See .work/system-codec-build.log and .work/native-api-build.log. Do not
run another Linux libc/toolchain as evidence. Preserve the original .git mount
read-only in Docker. The reference images persist across reboot:

- xsh-oracle-gnu-9.12: image 8aceca9f1278, glibc 2.43-7; real GNU tools are
  /usr/local/bin, NOT distro GNU 9.10. Includes strip, upstream getlimits, all
  date locales, and Swedish UTF-8/ISO locales.
- xsh-oracle-busybox-1.36.1: image a605b38b678eba7b0a16bc60d29b0299ccedcbf9999711d87e716cf406904e12;
  exact locked source, all 68 frozen applets, gzip levels enabled.
- Three BusyBox sed defects conflict with the established GNU sed corpus. Their
  unchanged assertions live in test-bb-sed-gnu.xsh and are validated against
  GNU sed; the other 77 sed cases use pinned BusyBox. No exclusions were added.

## Major integrated changes

Hundreds of accepted original native ports and fixtures are integrated. All
original 32 uutils scopes are accepted; the expanded manifest has 87 green
scopes at shutdown. All BusyBox ports and 55 of the GNU coordinator's 59 utility
scopes have passing references (initial GNU basenc/cat are separate).

Runtime changes include actual copy fallback errno fields, CPU feature facts,
preserved inaccessible inherited cwd, raw stderr bytes, exact new-file creation
mode, CLOEXEC FD duplication, closed-startup FD preservation, and evaluation on
the calling thread only when worker creation fails with EAGAIN.

IMPORTANT: the new long-double and locale operations are in EXISTING system:
parse_long_double, format_long_double, long_double_precision,
locale_numeric_info, locale_time_info. No public numeric/locale/text module was
added, because that would break ordinary existing variable and function names.
The private Rust numeric/locale modules remain. Numeric operations preserve the
native C long-double ABI; printf handles GNU x87 hexadecimal presentation.
Musl locale metadata uses verified generated language facts (23 locales, 55
aliases), preserving UTF-8, ISO, and GB18030 bytes, with no new dependency or
silent C fallback. See dev/reference/locale_metadata.py.

5817a106 adds unix.seek_fd whence=start/current/end and real fallible eager
range/repeat/bytes.zero allocation. Split consumer 2ed8495c catches ENOMEM and
prints GNU memory exhausted. These have compile checks but await rebuilt native
tests and exact GNU split resource gates. Preserve minimum-VM +6000 KiB budgets;
do not raise them, fabricate invalid-number failures, or weaken assertions.

## Immediate restart queue

1. Read child handoffs and manifests listed below; confirm branch/worktree paths.
   Do not redo proven ports or blindly reapply cherry-picked source hashes.
2. Integrate/review pending ready commits, then rebuild the matched release pair
   once native/API changes are stable. Run focused allocation, relative seek,
   numeric, locale, io, and unix tests first.
3. Finish pending production/fixture issues and accept the authored held ports.
4. Add exact retained Rust mappings only after meaningful assertions are verified.
   Complete strict origin coverage; regenerate docs and API summary fixture.
5. Run the relevant final native and Rust gates, repository ratchets and docs
   checks. The final full native gate has NOT been run on this integrated tree.
6. Only then tag compat-harness-final and delete dev/compat and remaining
   harness-only consumers. Nothing was pushed; no final retirement tag exists.

Pending commit/source details:

- SORT2 32011d6be1f44bc0e9c1b998e350039861e09a67 is accepted but unmerged:
  native 76 +4 existing locale skips, GNU 80. Only owned test file.
- SORT locale/field follow-up 5c587d222ff4a27b3eda88b809af83c35297a18f
  is pending integration. Several focused tests printed pass but Docker cleanup
  timed out, so do not claim completed gates. Unicode debug width fix remains
  uncommitted in fix-sort; reuse text_a2.character width, no new Std API needed.
- SORT1 Swedish port is wrong: it forces ASCII-space grouping while the real
  locale uses NBSP/narrow NBSP. Review original conditional guard and raw A0
  proof; preserve supported assertions and document any fixture correction.
- LS capability color 994fb767a6535385b27ef02b8cc220a6f57a382e is in
  fix-ls-capability-color. Original Rust case failed; focused Rust/native cases
  now pass. Full ls regressions/reference matrices remain unverified; do not
  claim full acceptance. Uses fs.xattr_get and typed capability decode.
- MORE 87a3d475 is ready but unmerged: pinned native Rust 7/7, BusyBox 7/7,
  closest native 13/13. Register core_compat_extra/more.rs and map seven shared
  origins. Three pure F/n/P origins and fifteen mixed unsupported clauses have
  exact BusyBox proofs; root must review scope reasons. Preserve meaningful XSH
  flag extensions rather than introducing BusyBox's ignored semantic flags.
- TIMEOUT fix is uncommitted in fix-timeout: remove argument normalization that
  hoists command options across DURATION; nearest 17/17 passed, GNU proof127,
  held 26 gate pending. Do not lose the saved regression.
- DD 9146eca7 fixes real stdout seek/zero-count pipe failure. Relative inherited
  position requires caller whence=current after rebuilding 5817a106; follow-up
  source/test work remains with the dd lane.
- KILL RTMIN+7 differs by libc: musl 42, glibc GNU 41. No policy decision made.
  Do NOT blindly hardcode GNU base34 or alter typed host process.signal semantics.
  Classify the target-ABI boundary with independently derived expectations.
- TAIL1 initial readiness and later delivery timing/reference problems remain.
  A bounded initial-output barrier is approved; preserve all later 50 ms checks.
- TAIL2 native69 and UNEXPAND native43 passed unchanged; GNU preparation was
  interrupted at wind-down. Neither final acceptance file is committed yet.
- GNU MISC/STAT/SORT wait final namespace/data rechecks; GNU SPLIT waits allocator
  rebuild and all14 unchanged cases. Others' accepted hashes are in manifests.

## Rust boundaries and safety

tests/core_compat_boundaries.rs: all21 passed native and GNU 9.12.
core_compat_extra fresh result: native60/61, real LS capability-color failure;
GNU all53 applicable owned origins passed (one shred timeout passed unchanged on
retry). The btrfs case has full retained code but lacks mkfs.btrfs/kernel support.
Three successful date clock-setting origins still require VM-only coverage;
containers cannot isolate realtime clocks. Never mutate the developer host clock.

Split lifecycle source/module is integrated, but its separate CPU quota gates
are pending. be20480b is a pending follow-up extending only startup readiness to
60 seconds before the preserved500 ms observation. Use --cpus=0.01, child AS
768 MiB, actual /dev/zero-open barrier, no chunks/output, and kill/reap ownership.
Ordinary extra gates must skip split:: and explicitly run the separate resource
gate. dev workflow routing is unfinished.

All block-device fixtures require a nodev /tmp tmpfs and a fail-closed statvfs
guard before node creation; 886d4688 integrates that guard. rm root-preservation
tests require private recursively readonly mount namespaces and dropped
credentials before any applet; strace --kill-on-exit prevents leaked tracees.
Never weaken or bypass these guards. Shred /proc/self/mem probes run only against
the disposable owned applet's own address space in isolated containers.

The pinned release-equivalent fixture compiler command is durably saved at
.work/boundary-extra-rustc-command.log; executable .work/core_compat_extra-release.
It avoids repeatedly rebuilding production code just to compile host fixtures.
Final registered Cargo release test targets still need their final relevant gate.

## Durable manifests and child handoffs

Under xsh-codex-compat-lanes/_scratch:

- ready-uutils-commits.json (87 expanded green scopes at shutdown)
- first-wave-ready-commits.json (32/32 original scopes)
- acceptance-current-status.json, all-uutils-status.json, accounted-uutils-origins.json
- uutils-restart-handoff.md, restart-unmerged-uutils.json
- remaining-fixes-handoff.md, remaining-fixes-ownership.json
- gnu-wave/ready-commits.json and .txt, ownership.json, audit-cp-head.md,
  audit-cut-split.md (preserve complete compound assertions)
- busybox-wave/integration-files.json (all638 origins, chronological hashes)
- fix-more-contract/scope.md and exact pinned proofs

Under integration _scratch:

- exception-audit/boundary-restart-handoff.md and boundary-extra-coverage.json
- exception-audit/ready-approved-candidates.json, all-proposals.json,
  boundary-coverage-gaps.json, unresolved-contracts.json, audit-corrections.json
- integrated-ready ledger/logs (source hashes often differ from cherry-pick hashes)

Child handoffs may call hashes unmerged that the root finished integrating during
wind-down: b2bee444, e11a1244, e0e0e2f7 are already integrated. Verify tree content.

## Retirement work already prepared

core/aliases.json is canonical; release packaging and native helper use it.
dev/reference/oracle.sh and regeneration callers are retained outside compat.
Frozen origins and repository invariants are retained under core/tests/origins
and dev/checks, invoked by xsht native wrappers. Packaging21 and lifecycle13
passed on the retirement lane; origin checker units5 and retained checks4 passed.

Before deleting the harness, finish refresh/mappings and resolve remaining
consumers: old stage/compat GNU tests, compatibility Python tool tests,
.claude/skills/xsh-compat-campaign, old dev/coreutils-parity.json and historical
links/.gitignore entries. Keep meaningful release archive tests. Update closest
docs, regenerate generated docs via make docs, and regenerate
tests/fixtures/modules/standard-api-surface.jsonl with final xsht.

Docker/corpus filesystem operations were slow near shutdown; this was not a
license to weaken deadlines, add retries, or stop other users' jobs. All owned
work was wound down. Source, unfinished patches and proofs survive reboot.
