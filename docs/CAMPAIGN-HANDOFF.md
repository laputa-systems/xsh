# Linux campaign handoff, 2026-11-04

The owner requested a wind-down, then resumed the goal on 2026-11-04. The
campaign remains incomplete. Retain unfinished source and useful fixtures.
macOS remains excluded. This handoff records the wind-down snapshot and the
resume updates below, not release or production qualification.

## Resume updates

The repeated graphical launch and idle-memory pairs now pass with the LTO
font package. Laputa's `docs/FONT-STACK.md` records five samples per side,
their spread, and the launch-to-screenshot upper-bound interpretation.

The parked primitive-handle prototype `fb3bc1712` has now completed matched
timing pairs. No CPU benefit exceeds observed spread. It remains unmerged;
the owning performance campaign records the neutral comparison. The earlier
description of unrun measurements below records the wind-down state.

The fresh connected check at `09447fa09` now passes its build and all six
kernels under the unchanged limits. The axiom audit passes for 27,044 project
declarations and zero closed roots, and all six negative controls reject.
This establishes the previously missing maintenance check, not the connected
milestone or final matching Rust/source freeze. Private branch `c6ff5be9f`
records the result. The complete kernel gate is still running and has no
final acceptance credit.

The generic imported-callable storage failure is repaired on `master` in
`bbc421a2d`. The verifier authenticates the imported scheme at the erased
callable producer boundary and retains concrete materialization checks. Six
Rust import-contract tests, all 33 native callable tests, and all nine native
opaque-Any tests pass in pinned Linux. The new negative control rejects a
substituted export. The full `94c13418a` reference replay failed in the debug
library tests: its older callable materializer lacked the guarded imported-field
path already present on `master`. The current production repair also passes its
focused debug test. Reference-only `28eb488d7` backports that five-line path and
passes all six debug import-contract tests. Its tree is
`e04676f5212f9bfc6d2df93552c1db4d989ba6ac`; a new bootstrap is running. Preserve
the failed receipt. No full reference replay has passed.

Private branch `318d2ac4e` extends the raw store check to the replacement
expression: Lean reconstructs the replacement from raw rows and from the checked
node and requires agreement. Scheme lowering emits assignment right-hand sides as
let-bound temporaries followed by one value, so the reader substitutes them and
rejects temporaries that rebind mapped source slots. The Rust control changes an
executed store literal and observes the assertion failure; its earlier harness
discarded propagated errors and could not fail. The binder-to-slot map, calls,
Result forms, and execution correspondence remain open.

The Codex session stopped at 17:38 UTC and ended the kernel `2252e24b6` full
gate in recovery round 8 of 10, with all earlier cases passing on both backends.
That run has no credit. The unchanged rerun then passed: baseline, device,
clock, all network-state and network cases, and ten of ten recovery rounds on
both backends (`full-qualify-105-rerun.log` in the kernel worktree's
`.work/tmp`). The A0 preservation and resource checks against build 105 remain
before integration. The `28eb488d7`
reference bootstrap had already passed. Its first full replay passed all 2,774
library tests, including the repaired callable case, then failed one xsht
integration case: `test_gnu_cp_sparse_extents_2_log` timed out inside the
desugared-corpus comparison while concurrent builds loaded the host and the disk
briefly reached 99 percent. The unchanged rerun (`reference-replay-28e-r2`,
fresh bootstrap) failed the same comparison at a load average near 13: in the
desugared corpus, `test_uu_tail_follow_single` saw no output 200 ms after
spawning `tail -f`. Neither receipt has credit. `master` now polls in that test
(`ddf203804`); the frozen reference keeps its fixed sleeps, so its next replay
must run on a quiet host.

Ghostty's font-filtered tests now pass inside the composed Laputa runtime:
206 pass, 13 skip, zero fail. The skips include existing disabled complex-script
cases, other platform backends, and unavailable font-specific fixtures. Laputa's
consumer contract records the exact accounting; this is not a full Ghostty
suite result.

## Source and publication state

| Checkout | Source | State |
|---|---|---|
| XSH `master` | `0d9af7bd9` before this handoff | Includes checker timeout output retention and the guarded field repair. No push. |
| Laputa `master` | `b1877cd` | Shared font packages, qualification tools, corrected roadmap, and wind-down limits committed. No push. |
| Ghostty `main` | `4cab0f7a9` | Linux GPU release uses system font libraries. Static software configuration remains separate. No release published. |
| Play `main` | `43ae445` at wind-down | Inspected at `2b48605`; a subsequent owner commit remains untouched. No Play migration or qualification performed here. |
| Kernel `work/successor-icmp` | `2252e24b6` | Local committed candidate in Laputa's `.work/wt/asterinas-icmp`. Not integrated or pushed. |

Laputa's checked-in kernel pin remains `d4c2e3877`. Its production Ghostty
package still pins published `f2024d5d2`. Do not replace either pin with an
unpublished or unqualified candidate.

The existing kernel-fork publication grant applies only after its required
qualification. It does not authorize pushing the other repositories. The owner
authorized publishing upstream liblzma and liblzma-sys 0.4.9 to the shared
registry earlier in the campaign. That work is recorded in the performance
campaign's log.

## Shared native font stack

Laputa packages FreeType 2.15.0 revision 3, HarfBuzz 15.0.0, Fontconfig 2.18.3,
and libpng 1.6.59. The existing Expat 2.8.5 and zlib 1.3.2 provide XML and
compression. [The consumer contract](../../laputa/docs/FONT-STACK.md) owns
SONAMEs, headers, pkg-config interfaces, font policy, measurements, and limits.

The real package build and international font proofs passed on x86_64. The
local shared-font Ghostty archive passed `make profile-test` under Linux/KVM,
Tinyland, and virgl after the FreeType LTO revision. Keyboard, screenshot,
application restart, and compositor recovery proofs remain intact. The
composed-root audit passed for 19 packages and 26 ELF files. It rejected a
duplicate font provider and an undeclared dependency in negative checks.

Reproduce local archive qualification from Laputa with
`qualification/ghostty-shared/run.sh /absolute/path/to/ghostty.tar.gz`.
Run `xsh qualification/ghostty-shared/closure.xsh -- ROOT` on the composed
candidate root. The retained archive is
`.work/tmp/ghostty-profile-repo/packages/ghostty-custom/files/ghostty-local.tar.gz`.
Its SHA-256 is
`a7245e40dc52777d3efa13b92577ee4c4dcc179d99da0fd0017620217d1d2595`.
The archive contains no shared font libraries.

The stripped executable falls from 30,787,208 to 27,179,848 bytes. The unique
installed closure rises from 172,913,946 to 175,776,899 bytes. FreeType LTO
reduces the same-version shaping/rasterization workload cost and library bytes.
The older bundled stack remains faster in that workload. The contract records
the measured samples and does not attribute that gap solely to shared linkage.

Graphical startup pairs and final repeated idle-memory measurements were
prepared but not run. The queued quiet-window driver was terminated during
wind-down. Older overlapping timing trials are invalid. ARM shared-library
cross-build and qemu-user evidence is bounded: native ARM packages, ARM Ghostty,
and an ARM graphical guest remain unqualified. The attempted Ghostty cross-build
failed at native-helper SDK and xkbcommon include configuration.

The owner's next sequence is shared-stack qualification, Play SRT/Core Text
migrations, [LAP-52 shared rust-fontconfig adoption](../../laputa/docs/rfc/LAP-52-rust-fontconfig.md),
then [LAP-51 C-free userspace work](../../laputa/docs/rfc/LAP-51-c-free-userspace.md).
The latter campaigns have not started. Preserve native Core Text without
running macOS work in this Linux campaign.

The owner's final correction excludes FFmpeg and codec replacement. Keep
`ffmpeg-build`, Mesa, musl, and Linux. Crabc evaluation or adoption and allocator
work are explicitly out of scope. The revised roadmap prioritizes existing
text and desktop interfaces, distribution compression, and later
HarfRust/Swash/Skrifa/Zeno qualification. It no longer proposes a media decoder
replacement or treats already-adopted OxiSH and KBVM as missing infrastructure.

## Kernel qualification

Candidate `2252e24b6` fixes limited-broadcast DHCP packets being addressed to a
gateway after route selection. `5da429919` replaces an obsolete Asterinas-only
wildcard-bind expectation with a real paired UDP test. The unchanged paired
DHCP lifecycle check passes after the kernel repair.

The build-105 `all --backend both` run passed baseline, device, clock ABI,
clock, 16 network-state cases, and all ten network cases. Five complete
recovery rounds passed. Wind-down interrupted round six after its paired boot
case. The orchestrator exited 143 and its guest container was removed. This is
not a complete `all` pass.

The retained log is
`../laputa/.work/wt/asterinas-icmp/.work/tmp/full-qualify-105-final.log`.
The candidate configuration is `candidate105-no-net.json` in that worktree's
`.work/tmp`. Re-run the full unchanged gate before accepting this source.
Repeat applicable A0 preservation and resource checks against build 105. The
earlier A0 and 30 resource runs used the previous kernel and do not qualify
this final repair. Only then integrate the kernel, exercise public-input
verification, and update Laputa's lock and gitlink together.

Three unrelated untracked kernel C drafts remain untouched. Keep the kernel
and successor worktrees, pinned tools, and guest inputs for resumption.

## XSH reference and optimization

The frozen reference is `51bd312d9` on `perf/reference-9d-qualification`, tree
`e8498e51977c47f8a5245a661b7a537a6483f10b`. Its canonical replay was stopped
during native testing. The receipt says `failed-or-incomplete`, exit 130,
unchanged clean source, and resolved container cleanup. Rust and XSHT groups
completed, but native accounting is partial. Observed failures include the
whole `opaque-any.xsh` module and the generic callable storage case in
`full-inference-callables.xsh`. Recheck both on repaired source before the next
full replay.

The opaque-Any failure also reproduced on current production source. Repair
`0d9af7bd9` is integrated into `master`. Its regression reads a record field
after a runtime type test and rejects substituting an unguarded binding. Three
focused Rust checks and all nine native opaque-Any tests pass in pinned Linux.
The generic callable storage case still fails with a field producer error on
this source. Its failure is retained in
`.work/wt/guarded-field/.work/tmp/guarded-field-native-final.log`. Full
qualification remains open; this repair does not turn the interrupted replay
into a pass.

The Rust checks used `cargo test --locked --offline --profile verification
--target x86_64-unknown-linux-musl -p xsh --lib FILTER -- --nocapture` for
`type_tested_field_reads_authenticate_the_guarded_binding`,
`dynamic_field_equality_authenticates_both_receivers`, and
`checked_optional_loop_fields_preserve_runtime_predicates`.
`xsht check tests/xsh/opaque-any.xsh` and
`xsht test tests/xsh/opaque-any.xsh` also passed. The exact generic-callable
native test command still failed. All used the pinned Linux image and
verification products, not host debug binaries.

The primitive serialization-handle prototype is committed and parked on
`perf/finish-linux` at `fb3bc1712` in `.work/wt/performance`. Its 44 focused
codec checks passed. Matched O2 products are retained under that worktree's
`.work/tmp/primitive-baseline` and `primitive-handles`. The candidate shrinks
`xsh` by 39,528 bytes and `xsht` by 51,584 bytes. Timing pairs were not run,
so the prototype has not earned integration. Preserve that branch and
do not compare against the unmatched older `spanless-codec` products.

Historical codec regression, the two-family 15% target, build-cost gates,
and final integrated-source qualification remain open in
[the performance campaign](CAMPAIGN-PERFORMANCE-DEPLOYMENT.md).

## Connected formal work

The private integration branch is `qualify/connected-machine` at `09447fa09`,
in `.work/wt/connected-qualification`. Its inventory checks 965 roots and
77 universal obligations, with zero universal obligations closed. The six
connected bridge tests and three raw bridge tests passed in pinned Linux.
Targeted Lean builds, axiom audit, and negative audit controls passed before
the final small proof-reuse change. That change's targeted build also passed.

Fresh whole-project qualification remains open. One loaded-host run passed
its build but exceeded the unchanged 1080-second paranoid-check deadline.
Another exceeded the 180-second fresh-build deadline. Timeout diagnostics now
retain partial checker output. No fresh six-kernel pass is claimed for the
current source. Re-run `formal/scripts/check-fresh.py` with the pinned Lean
4.35.0 tools and bubblewrap on `PATH`, using `/usr/bin/python3`. Retain the
8 GiB address-space guard and existing wall limits.

Raw RHS, binder-to-slot provenance, transfer marks, broader call admission,
and source-to-execution correspondence remain open. Fixture certificates do
not establish general correspondence. Do not merge the private formal branch
or refresh source-bound conformance records until matching checks pass.

## Retained work and unavailable review

Additional reviewer lanes became unavailable because the service usage quota
was exhausted. No substitute provider was used. Required independent review
remains open where the candidate lacks it.

Keep incomplete, dirty, and parked worktrees. Earlier cleanup reclaimed stale
fixtures, but rejected deletions were left alone. Git unregistered the completed
docs-refresh worktree, then root-owned generated files prevented full deletion.
Its remaining directory is retained and its committed changes are already
integrated. Do not describe that directory as an intact registered worktree or
claim its files were reclaimed.

No current campaign is complete. Resume from these source branches and their
owning documents, rechecking retained scratch before relying on it.
