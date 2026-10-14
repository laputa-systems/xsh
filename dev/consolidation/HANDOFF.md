# Consolidation restart handoff

Status: active 2026-10-14.

## Full publication acceptance, 2026-10-14

W9's strict full comparison completed successfully: 1,690 inputs across four
profiles, 6,760 outcomes, and zero mismatches. Both exact-owner proofs cover
all 1,690 inputs. The mismatch file is empty. Evidence is in
`.work/consolidation/w9-derived-reference-f105-compare-a1/`. The separate
original-to-derived-reference report scans all 6,760 outcomes and records its
first publication difference at input 36, `bench/stdlib-port/native_control.xsh`.
It does not count all reference differences; earlier descriptions of 36 as a
difference count were incorrect. The approved reference repairs and original
evidence remain preserved.

The captured compiler source is clean `f105936cd`. The comparison uses that
source's controller with the reviewed Python decoders from `5c59c0d8e`, whose
integration checkpoint is `43250d7a0`. The parser and subsequent campaign-doc
changes leave all eleven final review source owners unchanged. Original
captures, the fresh candidate capture, both fresh exact-owner captures, and
the fresh corrected-reference capture remain preserved with their receipts.

W4's accepted 0ab evidence remains 34 compiler, 242 native, and 62 integration
tests. The later statement and named-callback publication repairs have their
own regressions and the full W9 comparison above. Final current-source
correctness, normal and forced-frame suites, documentation, platform,
structural, and performance acceptance remain open. Consolidation is not done.

The owner retained the existing verification profile: opt3, no LTO, and 64
codegen units. Alternative-profile selection measurement is waived; no
fastest-profile claim is made. Final release acceptance uses opt3 with thin
LTO and 16 codegen units. Removing unused release opt1/opt2 runs leaves nine
serialized full native suites. The narrowed launchers passed focused receipt,
signal-cleanup, and observer checks, plus independent review. Their active
artifact pins and inventory validate.

The comparison controller now uses at most three forked workers with three
inputs in flight. The parent validates captures and proofs, orders results,
and publishes receipts. Workers read the inherited context without copying
it into each task. Serial/parallel mismatch and ordering tests, real-process
failure and signal-cleanup tests, and one saved real captured pair pass.
The integrated five-module Python check passed all 188 tests.
Independent review accepts the corrected implementation. Existing claimed
signal handling is preserved. The completed full W9 run above used the serial
controller; no parallel full-corpus timing or speedup is claimed.

## Current publication repair batch, 2026-10-14

The f105 smoke exposed one further parser error: Rust publishes
`stream_stage_types` as `(Option<Name>, Span) -> CheckedStreamStage{input,output}`,
while the new fixtures had assumed `StreamStageId` keys. The reviewed repair
from `e67f4f3a6` uses namespace/span keys, rejects duplicate normalized spans,
checks source bounds and value shape, and joins callbacks by their exact span.
The exact AST stage table retains its separate contiguous integer IDs.

All 24 selected outcomes now match under the corrected Python modules using
the existing f105 smoke data. A separate hash-only pass verifies all 12 exact
sidecars against their original BLAKE3 receipt digests. These are selected
smoke results, not full-corpus acceptance. Evidence is in
`.work/consolidation/w9-f105-smoke-replay-a2/` and
`.work/consolidation/w9-f105-sidecar-hash-a1/`. Full candidate capture is active
against frozen master f105. Keep master unchanged through captures and strict
comparison; the reviewed parser integration remains isolated until that boundary.

A bounded profile of one large current-schema projection found duplicate
whole-map scans. The reviewed `815f8e46d` repair validates and splits maps in
one pass. Old/new parsers agree on values and exact malformed-input errors;
the actual 873 expression-type map preserves all 70,974 ordered entries and
their rendered bytes. The integrated tooling passes 217 focused Python tests.
Profile timings are instrumentation attribution, not performance evidence.
Use the corrected, reviewed Python modules for strict full comparison while
retaining f105's compiler and capture identities. No Rust or helper protocol
changed in these parser repairs, so unchanged capture artifacts remain usable.

The repaired source at `47a691832` completed all 18 selected smoke harness
invocations, but strict projection stopped at input 873. The parsed `drop(1)`
span at SourceId(0), offset 11499, length 7 belongs to the exact stream-stage
table; the frozen generic owner projection contains pipeline-wrapper spans
instead. The independently reviewed follow-up from `4fe16f456` validates
stream-stage ownership against its exact AST table, retaining source bounds,
contiguous IDs, unique spans, argument/callback graph checks, and unchanged
generic projection equality. The removed generic-owner assertion was wrong
for this AST class. The integrated tooling passes 210 focused Python tests.
The parsed Rust regression still requires execution on
both fresh exact-owner roles. No full capture started after this smoke failure.

The 47a cache qualification passes all eight normal/traced XSH and Laputa
check/lint commands with matching output and diagnostics and zero duplicate
shared module keys. XSH check records 1,675 checks and 10,738 reuses; lint
records 1,533 shared checks and 1,264 reuses. Private lint contexts retain 524
checks and 58 repeated keys. Laputa check records 379 checks and 3,653 reuses;
lint records 316 shared checks and 1,389 reuses, with 372 private checks and
60 repeated keys. These are counts, not timing claims. The mixed migration
fixture passes repair, fresh import-graph recheck, execution, and convergence.
Evidence remains in `.work/wt/w9-cache-47a/.work/consolidation/w9-cache-47a-a1/`.
The proof-only follow-up leaves production Rust and cache commands unchanged.

Generated documentation renders successfully with the source-qualified 47a
verification binaries and produces no changed files. The scoped Markdown
checker was interrupted after 180 seconds without output and did not pass.
Canonical generated-doc, snippet, and project checks remain final gates.

The full strict W9 c1 comparison completed with 6,760 outcomes and 24
mismatches across inputs 873, 877, 893, 1162, 1516, and 1681. It failed;
neither its complete status nor its earlier zero-mismatch progress is acceptance.
Reports remain in
`.work/consolidation/w9-derived-reference-bb-compare-c1/comparison/`.
Original-to-derived-reference comparison records 36 changed outcomes. The
original reference, corrected reference, corpus, captures, and receipts remain
unchanged.

The accepted producer repair from `0ccc24d7f` restores named stream callbacks'
implicit call facts: result type, statement position, and static resolution
when the call adds no diagnostic. This explains the first four failing inputs.
The pinned Linux regression failed before the repair and passed afterward.
Each/Tee retain their existing statement and Result[Unit] propagation behavior.
This publishes existing semantics rather than changing the language contract.

The exact-owner protocol now records RunKind and complete ordered stage
arguments and callback edges. A plain command inside try can automatically
propagate without an explicit run flag; captured, status, and stream forms
cannot use that proof. Each/Tee implicit callback statements require exact
stage edges in both roles. Candidate callbacks additionally require matching
checked types, positions, targets, static-call facts, and identifier or field
names. The field-name repair from `66f67ed83` rejects a forged alias whose
exact module field names a different function. These repairs passed independent
review. Both exact-owner roles must be rebuilt and recaptured; old exact-owner
proofs do not match the new protocol.

The integrated tooling passed 209 focused Python tests after the final
field-name repair; its isolated adapter suite passed 39 tests. Integration
regressions cover reference proof forwarding through both rendered and parsed
comparison paths, including missing-edge rejection. The scanner and per-input
parsed handoff reduce repeated parsing while preserving diagnostics-first
errors and profile outcomes. No measured runtime speedup is claimed.

Next, qualify the final integrated source, build the candidate and exact-owner
harnesses, replay the six failing inputs, capture the new candidate and both
exact-owner roles, and complete a fresh strict 6,760-outcome comparison. The
original publication capture can be reused. Regenerate the correction receipt
against the fresh reference proof; the current controller requires a fresh
derived capture.
Final correctness, generated docs, platform, and performance gates remain
pending until functional W4/W9 acceptance. The prepared final launch plan
serializes the required suites and shares one canonical correctness batch.

The current 8ca cache measurement, full structural scan, and semantic reviews
are retained evidence with their source qualifications. Five previously
uncovered XSHT library tests now pass individually in pinned Linux. Feature
compiler-demand evidence transfers to the repair batch by unchanged match
owners; this establishes no new runtime parity. Final-source qualifications
and canonical receipts remain required.

Safe disk cleanup removed inactive compiler targets while preserving source
worktrees, captures, proof files, and receipt-linked binaries. The latest two
rounds increased observed free space by 5,103,865,856 and 2,024,652,800 bytes.
The paused coverage baseline remains about 128.5 GiB: its final LLVM/API
aggregate was never produced, so partial reports do not justify deleting raw
profiles. ModuleRecord reference evidence, Laputa2, and Goose remain untouched.

## Prior W4 and W9 closure checkpoint, 2026-10-14

Master integrates the accepted W4 source at `0ab236324`, the statement-fact
repair from `213585aac`, and the reviewed W9 controller and correction pipeline.
The campaign remains incomplete. Full publication parity and final acceptance
are pending.

W4 passes 34 focused compiler tests, 242 native tests, and 62 integration tests.
The native count includes 121 tests in each scheduler mode across eight files.
The changed-path scan covers ten Rust files and finds identical structural
metric values and evidence. The four required-zero metrics remain zero.
Production lines across those files fall by 23. The final full scan remains open.

The statement-fact repair moves span publication after automatic Result[Unit]
propagation. Its new regression fails before the repair and passes afterward.
The 28 existing statement-value tests also pass on the prebuilt 56580 binaries.
Those native results establish the existing behavior, not the repaired metadata.

The merged publication tooling passes 179 focused Python tests. The failed-profile
cache preserves each profile's raw checks and mismatch row while parsing an
identical pair once per input. The derived-reference controller passes independent
review, including a real caller-path delta-file test. Updated source-pin tests
pass 50 cases. No measured speedup is claimed.

The original e240 source, binaries, captures, receipts, and input manifest remain
unchanged. Both original roles capture 1,690 inputs across four profiles.
Their exact-owner proofs and all 6,760 raw hashes per role pass validation.
The a4 comparison stops intentionally with exit 130 after finding known differences.
It is incomplete and supplies no passing parity receipt.

The differences have three source causes. The approved iteration-propagation
repair adds the missing error effect for Result list, set, and stream sources.
The approved tail-match repair combines resource facts already present on arm
blocks. AST-only captures map both resource witnesses to their exact source blocks.
The statement-span difference is the refactor regression repaired in this batch.
A complete diagnostic-byte comparison finds four differences, all profiles of
input 1678. That original wait-until fixture lacks the success-type annotation
already added by `9791736eb`.

The separately derived reference at `bb29697265b29540f7d3a944a2454d2a4152dcc7`
backports the approved iteration, resource, and no-success flow corrections.
The original corpus remains fixed. Independent review accepts the source changes.
Eleven focused flow tests pass in the pinned Linux environment. The added iteration
and resource fixtures still require execution on the combined reference.
The reference proof distinguishes captured error values from automatic propagation
and excludes effectful or fallible literals from guaranteed control transfers.
The correction pipeline compares all 6,760 outcomes and diagnostics strictly.
Its source, patch, and parent pins live in `w9_publication_reference_correction.py`.
Follow `w9-reference-correction.md` for the build, capture, and comparison sequence.

The 56580 cache measurement passes all eight normal and traced repository and
Laputa check and lint runs. Diagnostics agree and repeated shared module keys
are zero. Final-source cache qualification remains required after W4 integration.
The feature probe at 0ab records 38 XSH and 18 XSHT compiler demands. The original
15 witnesses resolve to 13 compiler-demanded and two catalog-derived decisions.
Two test expectations require updates when a real expression kind is added.
The probe uses synthetic placeholders and proves no new runtime behavior.

Do not start stricter performance gates before functional W4 and W9 acceptance.
The final launcher preflight passes after fixing executable mode, scratch-directory
symlink checks, disk-backed build temporaries, and retry-path validation.
The canonical correctness batch, full forced-frame suite, profile matrix,
scheduler comparison, comparative workloads, docs, memory, size, and final
structural and semantic qualification remain open. Lean still lacks a frozen
source identity and Rust conformance evidence.

## Earlier closure checks, 2026-10-14

Master integrates the reviewed closure batch from `8c0352f02` and the
corrections through `97fb9b8ee`. Final acceptance remains pending.
The pinned x86_64 Linux musl image and platform exclusions remain unchanged.

The batch's products and required unit, integration, and soundness
harnesses built successfully. Capture regressions pass in both scheduler
modes, including parallel callbacks, producer yield/resume, and cancellation
defers. The forced-discard unit preserves child captures while skipping
parent defers. The optional-field verifier regression passes, and its
1,500-seed runtime property passes after the earlier 41 failures.

The DNS integration passes with the test harness's nested Cargo build removed.
Process tests now require explicitly prebuilt sibling products. Narrow gate
selection, product selection, removal of the root XSHT dev dependency, and
the shared musl linker contract are included. Timing speedups are not measured.

The optimized scanner reproduces the archived start report over 306 sources,
13 metrics, and 45 allowances exactly. The definitive scan at `865fa955`
covers 370 sources and passes all four required zero metrics: manual payload
calls, optional raw calls, blanket allowance lines, and wide arena wildcards.
No structural metric rises. Source totals remain advisory. Later corrections
still need qualification against the final freeze.

The current synthetic compiler probe records 38 XSH and 18 XSHT demanded
matches and a successful combined check after isolated placeholder arms.
The placeholders are not production code or runtime proof. Final source
qualification and the original 15 manual-witness review remain required.

Route lane `0db33401` passes 228 tests in each mode and produces 74 authenticated
receipts. Separate regressions retain the prepared-constant value assertion
and nominal checked-error payload coverage after isolating repeated tag
records. The validator remains strict. Direct-IR If selection now checks
both branch values and the actual normal and forced routes.

W9's corrected helper harness builds and lists exactly one parity test.
Its strict comparison found the approved schema changes: retired constant
indexes and new selected-callee and flow facts. A versioned adapter is under
independent review. The original helper, reference pins, and mismatch remain
preserved. Parser-derived owner metadata and focused flow-fact corruption
oracles are separate lanes. Final publication still needs all 1,690 inputs
and four profiles.

Current-source cache traces have zero repeated shared module keys. XSH check
prepares and checks 1,675 keys; default lint 1,533. Laputa check has 379 and
default lint 316. Fresh private probe caches remain separate. The official
measurement stopped on two XSH diagnostics, both since corrected; the
remaining captures are partial evidence, not a passing measurement receipt.

The combined pinned check at `0384ffc1` passes 1,521 files. Its lint run
returns zero with six warnings, all subsequently fixed and covered by three
focused development tests. Earlier lint fixes pass 99 contract tests in normal
mode and 110 route-related tests in each scheduler mode. The collect mutation fixture
used invalid multiline syntax before lint; the corrected supported form
passes its exact test. Full final-source acceptance remains required.

Architecture and Testing now describe compiler-demanded feature updates.
The generated CLI reference includes `xsht ir-tags`. The scoped Markdown
check did not retain a completion status; generated freshness belongs to the
final canonical gate.
Six semantic review rows have supporting evidence. The remaining rows,
normal and forced full-suite acceptance, generated docs, supplemental Linux
checks, size, memory, and isolated performance gates remain open.

Lean remains at 685 fragment roots across 89 owners. All 72 universal
obligations are planned, with no source identity or conformance cases.

## Earlier checkpoint — 2026-10-14 (`56079e577`, superseded)

Consolidation remains active. Integration is on `master`; final acceptance is
pending. Platform acceptance uses x86_64 Linux musl in the pinned image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
ARM64 and macOS remain excluded.

Master now includes the corrected pipeline-input regressions, authenticated
route-witness descriptor transport, and the Docker `.work` context exclusion.
Focused pinned transport checks passed. The broader runtime gate remains
90/91 because the DNS client exposes a verifier completion-analysis defect.
The functional candidate contains its proposed fix and retained regressions.

The formal inventory has 685 fragment roots across 89 owners. All 72 universal
obligations remain planned; `source_identity` is null and conformance cases are
empty. The latest validation/storage integration passed the inventory's 51
tests, full 82-job Lake build, fresh kernel check, 12,948-declaration axiom
audit, and all three negative controls. These fragments establish no Rust
conformance or whole-language soundness.

The functional candidate is in `.work/wt/closure-batch-integration`. Its
current check revision is `045ec245f2465b3a48ef0554b8d6db5962a7a8e9`.
It combines the reviewed closure batch, narrower gate selection, root XSHT
dev-dependency removal, generated instruction listing, mark-driven evaluator,
single pipeline path, route oracles, and consistent kache/mold build policy.
The product and debug harness builds passed. Focused tests exposed a
depth-test fixture decoder defect; its correction and the remaining checks
are pending. This candidate is not accepted yet.

Iteration latency now has a dedicated owner under both campaign skills.
The canonical campaign wrapper is `.work/tmp/run-native-canonical.sh`; it
mounts `dev/musl-linker.sh`, keeps one target/flag contract, and uses disk-backed
`.work/tmp` scratch. Do not restart old launchers with a different linker or
rebuild broad products for each exact filter.

Next gates are the combined candidate's focused tests and route receipts,
normal/deep grammar proofs, current structural scan and ten review rows, W9's
fresh 1,690-input/four-profile publication run, and final correctness, Linux,
documentation, size, memory, and performance acceptance. Older parity and
timing evidence does not close these gates. The corrected blanket baseline is
47,061 lines. Source totals and strict shrink remain advisory; the four
required zero metrics and other structural bounds remain binding.

## Earlier checkpoint — 2026-10-14 (`9b88622c6`, superseded)

This section records the acceptance snapshot at `9b88622c6` and is superseded
by the checkpoint above. At that checkpoint, consolidation remained active
and was not accepted or complete. Development and
final platform gates are x86_64 Linux musl in the pinned `Dockerfile.test`
image `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`;
ARM64 and macOS are excluded.

The current master includes the checker/lowerer control-flow alignment and
flow-fact reuse (`98b297356` and its preceding fixes), replacing the old note
that control-flow work remained isolated. Named-spread lowering now avoids
arena clones (`7ecb14310`). The W9 publication runner distinguishes BLAKE3
receipt digests from SHA-256 manifest digests (`7099b2a6d`); this correction
does not close W9 acceptance.

Formal foundations have advanced through the observed-machine and
occurrence-labeled binding-identity batches, integrated at `3630b99e4` and
`9b88622c6`, with additional bounded fragments for loops, paths, JSON,
collections, raw-IR decoding, and effects. The latest recorded formal gate for
the 637-root binding-identity batch passed the full `Xsh` Lake build, 51
inventory tests, the guarded fresh kernel check, and an axiom audit over
12,096 declarations with all three negative controls rejected. The provisional
inventory has 637 fragment roots across 89 owner sets; all 72 universal
obligations remain planned. Its source identity is still null and there are no
finite conformance cases. These bounded models do not establish Rust
conformance or whole-language soundness.

The route-oracle acceptance matrix remains open; current master does not
contain the unaccepted route-oracle integration work. W9 also remains open
until its pinned real build, 1,690-input four-profile publication/corpus run,
and final source-matched cache measurements pass. Earlier 1,690-input /
6,760-outcome parity belongs to an older source snapshot.

The verifier, applet-lint, and observer changes remain integrated. The
recorded observer checks passed 6 tests and 38 ratchets at their recorded
snapshot. Its archived start-source comparison produced identical legacy and
successor reports over 306 sources and 13 metrics (report SHA-256
`322ea03664f2478abb57362fbd0bda714715ff3020dd6316f827f2b441e891d3`). That
evidence covers the archived start snapshot only; a current full-source scan
and final structural close remain open. The recorded verification-binary size
snapshot at `0107c8158` is 31,005,944 bytes for `xsh` and 38,002,304 bytes for
`xsht`; final release size and memory gates remain open.

The final-correctness launcher remains prepared under the real-main
`.work/final-review-evidence/final-correctness-gates` tree and has not run. It
does not establish a source freeze. Full correctness and privileged Linux
acceptance, structural and reliability closure, G0b post-consolidation source
freeze, final campaign close, and comparative performance remain pending. No
final comparative timings are recorded. Strict source shrink remains advisory
under the 2026-10-11 owner update; structural, correctness, ownership,
documentation, platform, and performance acceptance remain required.

## Historical resume checkpoint at `59a6dfbc` — 2026-10-13 (superseded)

Resume began at `59a6dfbc`. Main through `2495fee8` includes the stream
comprehension repair, shared annotation/resource proofs, `.work` corpus
exclusion, deterministic shared-module lint ownership and fallback tests,
the mdev caller-umask repair, and the df/find/wc/layout/collect/GNU-rm fixture
repairs. The first-party Rust `Type::may_hold_resource` predicate is public;
lint uses the checker's resource classification. Nothing has been pushed.

The documentation refresh is complete, including canonical-owner routing,
runtime architecture, privacy, packaging, and verification commands. Generated
docs and `make docs-check` passed on the pinned release pair, then again with
the fresh optimized pair for the exported-callable shadowing description.
Public parameter labels are preserved while private/local shadowing remains
diagnosed. The single mdtool pass retained template-relative links and
generated-source markers plus example/path false positives; it was not clean.

Pre-edit verification on immutable `59a6dfbc` passed repository check (1,506
files), root runtime integration (94), the other selected root integration
(183; one corpus-walker failure), ambient/core boundary gates, and soundness
(14). The full native suite recorded 13,437 passed, 56 failed, and 138 skipped.
Failures are retained rather than classified collectively as flakes. Confirmed
causes include long socket paths, inherited setgid scratch directories,
hash-bound observer drift (restored), scratch-corpus traversal (fixed), a lint
fixture inheriting disabled configuration (fixed), and a GNU rm shell quote
(fixed, exact workload passes). The mdev umask leak is repaired and its isolated
mode tests pass. Debug library verification finished: 620 passed, 3 ignored.

After quarantining only owned scratch leftovers, XSHT integration finished
41 passed / 4 failed: prefer-collect layout invariance, a separate invalid
layout byte range, three post-desugar fuser fixtures, and the lint performance
gate. The latter is not valid timing evidence under load or before lint is
clean. Layout and collect regressions now pass. The fuser originals and
desugared sources passed 39 narrow checks; the full desugar failure remains
unreproduced and retained for the final gate with its actual failure output.

The owner requested wider staggered lanes. Ten disjoint core/test/helper
cleanup lanes landed after focused checks and independent review. The immutable
`61b125c2` snapshot built a fresh verification pair, passed 73 selected native
tests, and checked 1,507 files with one redundant-parens error in the current
observer (its lane has corrected it). Its full lint inventory was 455 findings
and 108 notes; later AWK/dd/sed, wc/fixture, and lint-defect commits are not
included in that count. The bare `cd` local-use defect and unitless builtin
range-size false positives are fixed, including a shadowed-range regression.

W9 tracing on `59a6dfbc` confirms each shared-session full key is prepared and
checked once: repository check 1,661, default lint 1,518; Laputa check 379,
default lint 316. Default lint additionally uses fresh candidate-probe caches:
288 caches / 905 records in XSH, 77 / 347 in Laputa. Every `(cache, key)` is
checked once, but repeated physical keys across probes remain work to measure
and optimize. Diagnostic ordering is now deterministic with retained fallback
tests; its batching performance still needs measurement. Laputa was clean at
`ef53ce3559432a80599606f848b1b4ac792b65d0` during the count run. The owner chose
to restore nominal Asterinas errors: Laputa `ca658ce` contains the restoration
and a red-to-green diagnostic-type regression; the five changed files check.

Active exclusive lanes: `observer-close` owns current observer/ratchet cleanup
and the real archived-start-source comparison before any successor-hash update;
`verifier-positional` owns the callable constructor fix, held at `2df51282`
after review found a forged-signature proof hole; `lint-control-flow` owns the
checker loop/return analysis behind four necessary AWK fallback warnings;
`lint-app-final` owns the remaining applet/native-test cleanup. Preserve explicit
`let _: Command` return-type assertions. The formal toolchain lane is closing
a post-spawn cancellation window in its guarded fresh-check runner.

Lean preflight is ready and its authorized staggered campaign is active.
Scalar and environment/store fragments, strict private Rust bridge fixtures,
and a provisional inventory are integrated. The root imports both theorem
modules; its fresh kernel check and 571-declaration axiom audit pass. Inventory
checks 77 named owner sets, its 13 mutation tests pass, and all 72 complete
theorem obligations remain planned. No Rust conformance, certified artifact,
whole-language theorem, G0b source freeze, or full G1 closure is claimed.

Next: finish focused failures, manual lint batches, merged-source correctness,
privileged Linux acceptance, structural/semantic closure, and release A/B
performance and vendored-regex impact. Performance gates require quiet-host
pre/post checks; final acceptance is still open. The readiness check exited 75:
perf gate deferred: host busy. The snapshot verification binaries are
31,005,944 bytes (xsh) and 38,002,304 bytes (xsht); final release size/memory
and timing gates remain. Lean source freezing, Float semantic repair,
conformance, and dependent proof closure wait for consolidation acceptance.

## Earlier pause checkpoint — 2026-10-13 (superseded)

**Resume:** "Resume the XSH consolidation campaign from
`dev/consolidation/HANDOFF.md`." First command:
`git -C /home/josh/d/laputa-systems/xsh log --oneline -12 && sed -n 1,80p dev/consolidation/HANDOFF.md`.
Then run the gates below in the pinned image through
`.work/consolidation/infra/run-native.sh` (it accepts worktrees under
`.work/wt/*`); builds are `cargo build --profile verification --target
x86_64-unknown-linux-musl -p xsh --bins -p xsht --bin xsht`.

**State:** campaign branch `master` in `/home/josh/d/laputa-systems/xsh`
carries everything below; nothing is pushed. Laputa `master` in
`/home/josh/d/laputa-systems/laputa` is at `28af5e6` (27 lint commits, check
clean, `xsht lint` at 1 warn plus 67 notes, pm/system/xinit/linux suites and
`bash tests/system/asterinas_host.sh` and `make asterinas-test` passed on it).

Merged this session:
- Verifier: top-level callable bindings are proven by their writers, pipeline
  stage slots are proven by item type, refined receivers (null-narrowed
  optional, record membership, union error pattern), exported callable aliases.
  Independent review found no forgery; its completeness gaps are in TODO.md.
- Checker performance: the resource analysis rescanned whole tables per
  statement (introduced by `1f6c6db1`); check stage on `core/awk.xsh` is back
  from 7.4 s to 0.4 s, whole-repo check thread time from 55,000 s to 323 s,
  diagnostics byte-identical over 473 files.
- Runtime: `return Err` in a `par-map` callback no longer panics on an unowned
  block frame; `sort` evaluates its options after collecting its input;
  `dd` sparse zero test is native.
- Tooling and fixtures: stale constants, dead diagnostic code, required-label
  count, rustix close, fuzz and desugar printer/fixture repairs, `as` effect
  note, grammar accepts an empty parameter list on a test declaration,
  vendored `third_party/regex-lite` (owner-approved).
- The xsh-wide lint pass (`lane/lintfix`, about 70 per-rule commits, 733
  files) and regenerated docs.

**Known red / unverified:** the lint pass landed without a test run on the
merged tree. What did run on the pre-lint tree: debug lib 611/611, root
integration 278/278, soundness 14/14, xsht integration all but the desugar
gate (since fixed and passed alone), native 13,377/13,493 with every failure
re-run alone passing except four slow or timing-bound tests listed in TODO.md.
On the merged tree only `ambient_fs_policy`, `core_compat_boundaries` and
`core_compat_extra` ran (green); the stopped run did not reach integration or
native. Not run at all: release-profile comparisons, privileged Linux tests,
`lint_performance` (cannot pass before lint reaches zero), `make docs-check`.

**Open lanes:** none. Every `lane/*` branch of this session is merged
(`git cherry` shows the g4 and g6 commits applied as cherry-picks). Lint-lane
leftovers are TODO.md items, not branches.

**Not started, in order:**
1. On `master`: build, then run `xsht check` over the repository, the native
   suite, and the Rust gates `rust` (integration, ambient_fs_policy,
   core_compat_*), `xsht` integration (`desugar::`, `lint_format_invariance::`),
   `-p xsh-fuzz --test soundness`, and the debug `-p xsh --lib` run. Bisect any
   failure to its `chore(lint): apply ...` commit and revert that commit.
2. Finish lint to zero (TODO.md "The xsh-wide lint pass"), then the same pass
   in laputa for the one remaining warn.
3. Release-profile performance comparison against
   `.work/consolidation/infra/baseline-bin` (btop and awk startup, lint,
   native wall time); startup on `core/awk.xsh` should be near 0.5 s CPU.
4. Per-workspace module check counts to close W9.

**Decisions owed:** keep or revert `refactor(asterinas)` in laputa (message-only
`return Err(X.Failed(...))` became `fail "..."`, four error declarations
deleted); how lint candidate probes should share the import-graph check;
whether the vendored `regex-lite` stays; the flaky-test follow-up in TODO.md.

**Verified at pause:** no campaign process, container or scheduled job runs;
this session's worktrees are merged and removed; laputa's
`.work/wt/lint-pass` directory still exists on disk with root-owned container
output (7.9 GB, git no longer tracks it); remove it with
`sudo rm -rf /home/josh/d/laputa-systems/laputa/.work/wt/lint-pass`.
The older worktrees under `/home/josh/d/laputa-systems/xsh-consolidation-lanes`
and `xsh-consolidation` belong to earlier campaign sessions, were left
intact, and were not audited.

## Private integration, complete owner parity and alias fix — 2026-10-12

Before this checkpoint main is `fdf8d41b`, with no pending tracked edits.
Root assembled `integration/consolidation-final-rehearsal` in
`/home/josh/d/laputa-systems/xsh-consolidation-lanes/root-final-integration`.
Its current head is `3e2b444d`; main source remains unchanged. The assembly
contains W4 through `17db51da`, exhaustive witnesses `0b33906c`, runtime/counter
closure `556d8d75`, W9 port `4b990e07`, nominal correction `40c9d792`, publication
fix `7dc881ed`, context/scope fixes `14de6072` and `3f39188c`, and canonical
documentation preparation `94409522`. Root resolved overlapping lowering
imports, preserved checked import provenance with selected module lookup,
and retained checked pattern registration with selected root statements.
Pinned production-library and XSHT compile checks pass at the earlier
`8b130232` snapshot; this is not final optimized verification.

W9 owner parity is complete at sealed `7dc881ed`: all 1,690 frozen inputs and
6,760 outcomes match. The final controller invocation initially failed because
its reference directory contained `receipt.tsv` while the verifier globbed
`*.receipt`. Original status, diagnostics and receipts remain unchanged.
An isolated symlink adapter and the unchanged verifier pass, independently
rerun by Root without rerunning cases. Real-main evidence is
`.work/final-review-evidence/w9-owner-publication-7dc-proof`; copy/root receipt
SHA-256 `e15dc5bd25b7caf17cb7b2b6c362144dcaf19c2f9ac0b633bf09478a12a8181a`.
Independent owner review SHA-256 is
`8219eca5adebd5eb4762b54f85d1ee7d6c7c1b785feeb2676a3b65142aa8f0dd`.
Final combined source and check/preparation counts remain unverified.

Contextual Error test seal `70e1f322` reproduces a direct import-alias schema
mismatch while transitive calls pass. Narrow lowering correction `3e2b444d`
canonicalizes annotated local and top-level runtime checks, retaining source
diagnostic names and inferred facts. Independent review finds no defect,
report SHA-256 `233108b7db0cfd3e81bfc2c534a143298b01fb1a96d85a00926506aebbb73993`.
CLI root tests pass 3/3; callables 36 and resources 19 pass in each mode,
as do five selected nominal/structured/imported Error cases in each mode.
Supplemental receipt SHA-256 is
`4e0631b804c7c6fc61874bab10434891ada3cf4be23c48798d44caa5dbab5da1`.
The new top-level red was observed but its redirected log was overwritten;
the original local red is retained. These are focused, pre-generic-W4 gates.

The generic Method owner remains mutable on `17db51da`. Its corrected mutation
swaps a same-owner Label parameter into List.push/Map.set without changing
slot or source facts: disabling parameter proof accepts the forgery; restoring
it rejects it. Root also reproduced two checker-valid baseline successes that
the provisional candidate rejects: a dynamic formatted-string parse receiver,
and an empty annotated Map containing typed callables. Both now have native
regressions and narrow intrinsic/empty-container fixes under verification.
Do not dismiss the empty Map as an invalid fixture or integrate mutable WIP.
After the next seal, reactivate independent verifier review and the exhaustive
witness port, including the new literal-key query. No scalar Path proof is
authorized beyond the established callable/producer contract.

Final performance tooling is now copied and reconciled under real-main
`.work/final-review-evidence/final-performance-gates`. Its revised launch plan
separates opt-level costs, the frozen seven-tool baseline, and the same-source
release scheduler reverse-overlay comparison. Original tools/fixtures remain
byte-identical in `original/`. No final builds, fresh lists, suites or timings
have run. Generic W4 review/port, final integration, generated docs, structural
and semantic evidence, full correctness and release performance still remain.

## Selected-context fix and latest constructor publication delta — 2026-10-12

Main is `b3e7d965` before this checkpoint; W4/W9 remain private and unaccepted.
The owner asked whether the campaign is nearly finished: implementation is
mostly in place, but exact correctness closure and final integrated/performance
gates still remain. Continue unattended rather than treating focused tests as
final acceptance.

W4 specialized Result producer/lowering correction is sealed as `17db51da`,
parent `fb770640`, diff SHA-256
`25c9a8dda59e3d76d704ddab4251617a41ee03e56e6111c1e857a5524ff7976e`.
It removes duplicate FsFiles/FsWalk Try lowering that bypassed source facts;
normal lowering attaches facts to the actual producer row. Registry-derived
result shapes include conditional wrappers. Private Rust checks and native
callables 36/36 in both modes pass; fresh private discovery has 13,648 IDs.
Final review identifies generic registered Method results as a remaining
producer gap (Path methods, scalar fallible methods, container getters and
Result.context). The author owns receiver-specific registry template
instantiation and exact callable/Error provenance, with no blanket scalar Path
contract. Generic source remains mutable; do not integrate that WIP.

Verifier feature-witness slice is privately sealed as `0b33906c`, with
`17db51da` ported as `f36b46d5`. Six actual semantic queries enumerate all 92
expression views, and the narrow function-reference source query is shared by
its two clients. Private source check, corruption checks and callables 36/36
both modes pass. Receipt SHA-256 is
`69ad67a75d70874960410857f4294d0c351b0068aa96be9d5588aeea2bee3e54`.
This seal predates the generic Method follow-up; it needs that port and a
new-helper feature audit before final acceptance.

W9 nominal probe correction `40c9d792` is sealed, independently reviewed,
and reproduces/fixes the missing nominal index panic. All 62 checker units,
seven constant units and source 1075 in four profiles pass. Fresh four-worker
owner-v2 attempt 1 stopped on a controller helper omission, cleaned all owned
jobs and was preserved. Attempt 2 proves 47 complete ranges (1,175 inputs /
4,700 outcomes), then finds a real publication mismatch at source 1631,
`tests/xsh/stdlib/rel_path.xsh`, check-cli profile. Diagnostics match. The
reference publishes constructor instances Defaults, Entry, Inventory; candidate
publishes Inventory, Entry. All three are root-owned SourceId(0); Defaults
exists in checked call facts but is missing from the published instance cache.
Captured original input/helper/reference/candidate identities and both outputs
remain under `w9-publication-mismatch-1631`. No proof jobs remain. The owner
is adding a red regression in isolated
`module-record-instance-publication-followup` before repairing cache admission
and reference ordering. Keep the declaration-only nominal fix; do not sort
rendered outputs or drop publication fields to hide the difference.

Discovery context/cancellation fixes are privately sealed as `14de6072` and
`3f39188c` on top of master port `4b990e07`. The first creates context-qualified
internal module keys while preserving physical paths and namespaces, and adds
a cancellation-aware loader API alongside the existing never-cancel API.
The second reproduces second-context enum/Error construction failure and
confines alias-owner lookup to selected module identities and root statement
ranges. Preparation is capped at 32 independently of execution --jobs;
execution fairness and its per-file eight-case bound are unchanged. Focused
CLI cases and optimized builds pass; the owner did not retain follow-up raw
logs. Independent source/artifact review finds no remaining defect, report
SHA-256 `db1c2c328fbfce4d6b046b22945fefb02f3cbe9ad3e2c22d9e32cba540fcf6d5`.
The earlier broad global-function collision claim is explicitly withdrawn:
function definitions already use selected scope. Final combined W4 canonical
Error identity and scoped-import tests are still required.

The final correctness launcher is prepared under real-main
`.work/final-review-evidence/final-correctness-gates`, not executed. Independent
review caught and corrected hidden runtime Cargo builds, feature-overwritten
products, manifest trust and signal cleanup. Distinct product/harness/runtime/
privileged targets preserve exact feature variants; runtime tests keep their
existing nested Cargo contract in a managed writable target, UID 1000, pinned
compiler/cache/lock and compiler-only jemalloc. No tracked test seam was added.
Final static review SHA-256 is
`8a2a1a1fc7ff375b72b002415673999e9c09ac15819d99ffe6bef20ff0ee9cb2`.
Launcher SHA-256 `a8281383228289b12eb5494784a9a4a46e784287ce1ed5a0f11c46f9e9c59e68`;
runner `fcfb41b49fc28818ac90b1a6809bd941f56bcbce85cc6e528125c08bc66866af`;
manifest `993d3f49dfb70ff02cb8c76324c82f6348e30850e722124e5ba24c55c4016b58`.
W4/W9 integration, final source freeze, complete publication parity, generated
docs, observers/review bundle and release performance remain pending.

## Result producer cohort and nominal-domain regression — 2026-10-12

Main `0769da30` retains the frozen historical ratchet exclusion and current
coverage. W4 is not accepted yet: final review of `fb770640` identifies a
missing fixed Result producer family, including specialized Path and FS rows
used as immediate Try sources inside With initializers. The author owns a
coherent registry-derived result/error proof plus valid-source and corruption
coverage. Conditional Result wrappers and encoded operations must agree with
the registry shape. This does not authorize scalar Path operand proofs.
The independent verifier witness lane adds exhaustive semantic query matches
in a separate branch and will port the final producer correction.

W9 latest-owner proof `functional-full-owner-v1` is incomplete. It validates
1,000 original inputs across all four profiles, then the 1000–1099 range
panics at source 1075, `docs/snippets/spec/70-nominal-type.xsh`, in the entry
argument-domain probe: nominal signature lookup lacks the constructor
nominal-name index. Removing redundant Str preparation exposed this missing
probe prerequisite. The failed log and incomplete attempt status remain;
there is no complete latest-owner parity claim. No parity jobs remain.
The owner is adding root/import nominal probe regressions and factoring one
declaration-only constructor-index pass, retaining existing full-bundle
default collection afterward. Independent review follows its exact seal.
A fresh immutable latest-candidate matrix will use four bounded workers over
disjoint ranges; the original reference, absolute input root and strict
6,760-outcome validator remain unchanged. All children must be joined/cleaned
on failure; this functional concurrency is not performance evidence.

The private combined runtime slice `4c5cf26e` (W4 `fb770640` plus runtime
`556d8d75`) passes optimized lowering coverage 10/10 in both modes. A disposable
single-row schema probe at ordinal 158 produces E0004 at the actual semantic
dispatcher; its scratch worktree is removed and evidence copied unchanged into
real-main `.work/final-review-evidence/runtime-counter-closure-final-private`.
This scoped probe and private pair predate the new W4 producer correction;
final-source combined review and the complete AST feature experiment remain.

## Historical ratchet discovery migration — 2026-10-12

The frozen `dev/tests/test-consolidation-ratchets.xsh` is excluded from
ordinary directory-based discovery by the root configuration. Its bytes and
hash attestations are unchanged; explicit file invocation remains available.
Independent source review identifies two stale fixtures that invoke the
current driver without successor observer/evidence inputs, explaining the
legacy 36/38 result. Its current counterpart retains all 38 declarations,
including corrected fixtures and structural-zero, provenance and final-close
coverage. This retires historical executable discovery rather than silently
filtering failing IDs or changing the frozen original oracle. Final fresh
paired full-suite lists must include all current counterparts and no frozen
legacy-file IDs.

W4 correction `fb770640` is sealed, but final review identifies one remaining
required producer gap: PathResolve/PathReadlink Result rows marked by With
initializer Try lack fixed registry producer arms. The author owns focused
red-first correction and exact registry Result/Error checks, without operand
Path validation. The separate compiler-witness lane proceeds on its own
branch and will port that final correction before acceptance.

## Narrowed verifier and sealed module port — 2026-10-12

Main remains `e40235db`; the following private slices are not yet accepted.
W4's original producer-proof commit `ec5057d5` and follow-ups `7575dd6c`
and `84a3f178` correct the discovery failures recorded below. A further
two-file correction removes incidental Path operand validation from
registry-fixed Result/Error producers. Independent review establishes that
blanket scalar Path provenance is outside the approved callable/Error proof
contract; required callable source identity and canonical Error checks remain
strict. The broader uncommitted Path proof was archived with source hashes
before reversible rollback. Valid computed, indexed, mutable and direct-record
Path receiver regressions remain. The exact optimized pair passes 35 callable
tests in both modes, the original focused gates, current ratchets 38/38,
and fresh discovery: 13,647 valid IDs, zero invalid file-only entries and empty
stderr. Final corrective seal and independent acceptance remain pending.
Seven verifier semantic queries still need compiler-demanded extension closure;
that is a separate lane, not authorization for broader scalar provenance.

W9 owner `39763a66` is sealed and removes the private ignored parity-helper
include from tracked tests. Independent source review finds no remaining owner
contract defect. A fresh immutable candidate-only comparison reuses the fully
validated original reference and exact original absolute input root; it has
passed 300 of 1,690 inputs in all four profiles at this checkpoint. Earlier
complete v9 evidence applies to the pre-cleanup source and cannot certify the
latest owner. The obsolete old-source v7 rerun was stopped and preserved as
superseded, not counted as a semantic failure or completed latest proof.

Private master port `4b990e07` combines owner `39763a66` with main `e40235db`,
preserving parallel checking, default 32 workers, fair file admission and its
eight-case bound. Focused parallel-discovery CLI tests and tooling-test 17/17
pass. Independent review finds two port defects: path-only shared loader
caching can reuse nested imports under a different nearest configuration,
and synchronous workspace preparation delays cancellation. A separate
follow-up lane owns regression tests and both fixes; owner proof source stays
immutable. Port acceptance waits for those fixes and complete owner parity.

Runtime/counter slice `556d8d75` is sealed and independently reviewed, with
actual exhaustive semantic dispatch and one counter catalogue. Its earlier
lowering-coverage failure requires rerun after corrected W4 integration.
Final combined correctness, generated documentation, source/feature observers,
semantic review bundle and comparative release performance remain pending.
Source caps and strict shrink are advisory; ARM64/macOS remain excluded;
fourfold native speedup is aspirational and performance nonregression remains
the acceptance requirement.

Disk cleanup is complete. The additional inactive main `.work/target` removal
freed 6,754,529,280 bytes by statvfs after reference/process checks. All seven
retained executable hashes matched. The real-main ignored cleanup ledger
records exact paths, reasons and outcomes; source, Git worktrees, original
oracles and evidence were preserved.

## Stage fast paths and verifier discovery — 2026-10-12

Master `9c6d4eb0` accepts the restored direct-field Map/Where/SortBy/GroupBy
paths and precise cached-field error spans. Independent review found an
initial span regression; retained red/green evidence and the follow-up fix
preserve the field expression's range. Stage-engine tests passed 9/9 in
normal and forced-frame modes, plus four focused stream cases in each mode.
The allocation comparison is one 256-row point (candidate 4,552 allocations,
parent 4,551); it proves neither an allocation slope nor a speedup. Final
isolated comparative timings remain pending.

W4 seals its reviewed owned port as private `ec5057d5`, but fresh discovery
exposes 53 invalid file-only entries versus zero on its source-matched abdd
base. These are preparation failures, not headers or successful test IDs.
The first reports full-IR verification failure; the author is reproducing
and correcting the common cause before integration. Focused passing gates
do not qualify this broader discovery failure as green.

W9's strict common-root publication matrix is exact through source 699 of
1,690, then fails at source 703 in every profile: the candidate publishes an
extra statically resolved call span for `unix.write_fd(fd, part)` in the
original DD fixture. Input and diagnostic digests match. Both captures and
the failed attempt are retained; removing that fact for localization is not
an oracle allowance. The reference fact-admission condition is under review
before an owned fix and fresh complete matrix.

## Extension and lint closure lanes — 2026-10-12

Master `a149123f` accepts the formatted-interpolation error-location oracle
(`4d56f0dd`): a checked-valid runtime failure must identify line 2, column 17
inside the interpolation rather than the outer string. It passed normal and
forced-frame on the frozen source-equivalent abdd pair. Final-source rerun is
still pending.

Resource-scope callable reachability (`54f3404d`, private `7385a12c`) fixes
a reproduced lint omission: expression-form resource bindings shadow an
unrelated top-level callable after their initializers, so that callable is
now correctly reported unused. The shared visitor enters/restores scope,
processes initializers left-to-right and excludes `_`; no second full lint
traversal was added. Independent source/receipt review found no blocker;
private optimized lint-dead-code 20/20 and with-resource 19/19 passed.
A direct lint-specific `_` shadow oracle was not added; the source guard and
existing unnamed-resource test cover the boundary. The red/control fixtures
and logs are preserved in real-main final-review evidence.

Checker statement extension closure (`a149123f`, private `88947f35`) makes
six expression context/control decisions and two statement policies exhaustive
without changing current positive cases or fallback results. Independent
review verifies the exact source; five focused files passed 65 tests in each
mode, with zero skips. The file named source-before records an intermediate
edit before the first compiler check, not the commit parent: its duplicate
ValueBlock arm was removed after an unreachable-pattern warning. Original
evidence is preserved with an explicit provenance clarification. This is not
the final synthetic compiler experiment or a final zero-site claim. Source
growth is advisory under the owner's performance-first policy.

The expanded feature-touch audit found remaining runtime routing/dispatch and
compact counter-size obligations. Their implementation waits for W4 to
release overlapping runtime files. W4 also owns exhaustive schema-expectation
classification and fixes legal imported-error/stream-signature proof failures.
The stream regression explicitly declares Stream[Int]; lowered Stream[Any]
storage fallback must not erase that declaration. Error-family aliases need
canonical declaration identity, never suffix-only verifier equality.

A separate stage-engine lane restores the approved Map/Where fast checks and
historical SortBy/GroupBy field projections. It is sealed privately and under
independent review; no timing acceptance is claimed. The cached Map plan adds
one allocation at the sampled 256-row size; no slope is established. Review is
checking list/stream errors against both current engine and frozen materializer.

W9 retains the true cache/constructor-order fixes and all failed attempts. Its
v4 comparison used separate absolute input mirrors, producing source-qualified
Name differences; the exact core/arch replay proves that comparison condition
invalid, not a candidate semantic difference. A fresh full v5 uses one common
frozen absolute input root for both arms and preserves strict live Name
spellings. Focused captures use distinct index/profile/side/binary-hash paths,
refuse overwrite, and validate actual headers, input identities and terminals.
No complete green publication matrix or W9 master port exists yet.

Read-only final preparation now includes the ten semantic rows, all 152 live
instruction routes and six retired ordinal holes, 46 stage routes, lint descent
inventory, source-observer 900s plan, and final comparative/profile launch plan.
These are qualified inventories, not an accepted review bundle. The fresh
native nonregression reference will use final shared runtime/corpus/helper
with only both scheduler patches and default-width policy reversed, giving
the old contiguous queue and default eight workers. Original frozen baselines
and interrupted runs remain unchanged; expected final IDs require fresh paired
listing, not the invalid historical preparation-error manifests.

## Deterministic fixtures, GNU regressions and build tooling — 2026-10-12

Master `724c53ba` includes deterministic DD FIFO production (`abdd589c`):
the test-only child opens a write-only FIFO rendezvous and acknowledges chunk
consumption before the existing pauses. Inputs, assertions and the 15-second
DD deadline remain unchanged. All five callers passed three rounds privately;
this repairs a fixture race rather than altering runtime IO contracts.

The frozen `abdd589c` combined optimized source passed all 43 focused gates:
121 indexed Rust units and 710 native test executions, zero skips. Twenty
files ran in both execution modes, plus both exact session-leader LS PTY
cases. The real-main receipt is
`.work/final-review-evidence/master-abdd-kache-combined-gate.md`; original
source/binary manifests and raw logs remain retained. This is focused
integration evidence, not final acceptance or full-suite throughput evidence.

GNU regression fixes `57800e04` and `724c53ba` preserve resource ownership for
implicit tail-Match returns and suppress automatic main dispatch after signal
shutdown during initialization. A Root returned through an arm remains live
through borrowed calls; interrupted initializers cannot leave main running
with unbound captured globals. The four GNU cases passed normal and serial
privately, resource lifetime tests passed, and signal hooks passed 14/14 in
both modes. The deterministic startup regression reproduces status 3 before
the fix and the requested hook status 143 afterwards. Canonical signal and
architecture prose now describe these contracts; generated docs remain pending.
The Match publication delta is an intentional bug fix separate from W9's
exact frozen-reference refactor parity proof.

All three central build wrappers now use the host static kache 1.0.0 and mold
3.0.0 through read-only mounts, with the pinned image's Clang driver, CRT
search path and symbol aliases. The compiler, libc and image remain pinned
to `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
The corrected wrapper passed compile checks and the combined build/gate above.
The first direct-mold driver attempt rejected GCC-style arguments before
compiling XSH and is retained as a failed attempt. Jemalloc remains build-only.

The requested Luna xhigh disk cleanup removed inactive Cargo targets while
preserving source worktrees, dirty source, original oracles, raw receipts and
seven required executables with matching hashes. About 196 GiB of disposable
targets were removed; available space was about 415 GiB at completion. Exact
paths, pre-delete sizes, reasons and outcomes are in the real-main ignored
`.work/final-review-evidence/disk-cleanup-ledger.jsonl`. Concurrent builds mean
free-space deltas are not exclusive accounting.

W4's owned port passes typed-callable and signal gates but is not accepted:
an imported-module fixture exposed a verifier failure under investigation.
W9's repaired CLI cache merge retains equal shared plans and rejects conflicts;
fresh source-matched b2sum and lspci captures pass all four profiles. Its new
full 1,690-input/four-profile pair is running with explicit executable hashes.
Previous failed, stale and partial captures remain preserved and qualified.
Full publication parity, W9 master port, final structural/semantic bundle,
full relevant correctness and comparative performance gates remain pending.

## Accepted broad component batch and corrected publication evidence — 2026-10-12

Master `7205a0b6` accepts configured-width lint (`05aad7be`, `6b273089`),
including loud errors for invalid nearest imported configurations. Independent
review confirms dependent roots are excluded before lint work; private units
41/41, integration 22/22 and focused native regressions passed.

Indexed blanket retirement (`cc59c5d1`) and metadata closure (`51c82f32`,
`2ac19edd`) remove redundant comparison-chain assertion metadata, validate
and consume the encoded filesystem-children operation, and replace the driver
defer trigger's hand-indexed read with its typed view. Comparison-chain tag 32
is unchanged; its internal payload loses the unused Boolean. A test-only slot
count helper remains for existing Rust tests. Focused corruption/ordinal/defer
checks and comparison-chain 6/6 in both modes passed. The only production
warning is the W4-owned typed-call signature reader; final warning-deny feature
matrix remains pending that integration.

Canonical stage-call checking (`413d1982`, `eefe8ba8`) removes per-callback
arena clones while sharing callback-result, Unit-tail and argument-binding
rules. It fixes a reproduced invalid Bool assertion edit on a callee token.
Int no-fix coverage already passes baseline and is an invariant, not a claimed
regression. Focused stage/tail files passed 15/15, 28/28 and 4/4 in both modes;
the allocation regression passed. At 96 unrelated declarations, residual
named-vs-inline growth is 36,764 bytes and 123 allocations; the byte guard is
bounded regression evidence, not proof of zero growth.

Feature-on XSHI (`09159c8b`, `97d3bec6`) runs session/checker/evaluator work on
a 12 MiB thread while argument parsing and signal guards stay on the joining
thread. Existing IO workers keep their original behavior. The tiny-main-stack
regression, actual pthread stack probe and CLI 10/10 passed privately.

Bounded fair scheduler (`3a6c3daa`, `7205a0b6`) limits parallel admission to
eight cases per file and rotates ready files. Default workers remain capped
at 32; explicit jobs remain an override. Serial behavior, stable IDs/temp
indices and completion-arrival parallel reporting remain intact. The real
multi-file progress gate and default-worker BusyBox AWK 49/49 passed. The
bound counts admitted cases; preexisting uncooperative timed-out evaluator
threads can outlive leases. No full-suite nonregression claim is made yet.

The completed W9 original-corpus matrix has 6,760 valid rows per arm, identical
input and diagnostic digests, but 256 publication mismatches across 64 paths
in all four profiles. The earlier live-prefix comparator ignored output rows
that preceded input rows, so every earlier live-publication-match claim is
withdrawn. Independent receipt audit validates the complete corpus and all
mismatches. A fresh first-mismatch capture isolates the missing imported
`PreparedConstants.cli_plans` cache; the lane owns a red-first fix and renewed
parity proof. W9 is not accepted or ported.

Combined master verification for this broad batch is pending. W4 full producer
proof, W9 publication, deterministic DD FIFO producer and GNU resource/capture
regressions remain active. The frozen-source metrics scan hit its 240-second
cap, not a reported scanner error; nine observer tests pass, but no full report
was produced. One mutable-source scan was explicitly invalidated and retained.
All original baseline files remain unchanged.

## Throughput requirement clarified and combined gate green — 2026-10-12

The owner explicitly made 4x speedup aspirational; performance nonregression
is the acceptance requirement, and further optimizations may follow campaign
closure. Earlier hard-4x statements in this log are superseded. The scheduler
lane now closes reliability and honest nonregression evidence rather than
chasing a speed target.

The frozen `7feee1d9` combined optimized source passed all 37 focused gates:
120 indexed Rust units and 616 native test executions, with zero skips. This
includes 17 files in both modes and both exact LS PTY cases as a session
leader. Source and binary manifests, raw logs and the real-main review receipt
are retained. This is focused integration evidence, not a full-suite gate.

Sixteen native-file preparation repairs are accepted from `3a2e3dd3`:
fourteen descriptor helper effect closures and two PID snapshots before
consuming wait. Both frozen benchmark arms prepare all 16 files, listing 800
IDs each. Runtime spot coverage passes 16/17 each; the retained partial-block
DD FIFO case fails independently on both arms, sometimes at its existing 15s
deadline. Its dedicated follow-up lane owns root-cause investigation and
preserves the original assertions and deadlines.

## Wider component closure and scheduler failures — 2026-10-12

Master `c031b1f3` includes final call readers (`01ec3b91`, `edf269e2`),
checksum sharding with per-input output checks (`987bc1e4`, `af7b1897`),
AWK sharding (`65af4019`), and indexed named-spread parameter lookup
(`0d60d207`). All 4,000 checksum inputs and 2,112 AWK fixtures retain their
original assertions and origins. Private checksum 49/49 and AWK 307/307 gates
passed; their overlapping timings are not performance acceptance.

Reviewed nested comprehension target binding (`c031b1f3`, private `cf6ba78e`)
removes three allocations and 336 bytes per item in normal and forced-frame
probes. Its private record/collection/map gates and atomic binding test passed;
CPU latency remains unmeasured. PTY redirection fix `aa0c8f28` adds `O_NOCTTY`
after a session-leader reproduction terminated XSHT with SIGHUP. The retained
Rust boundary regression and 34 process native cases passed privately. The
three stale musl applet failure TODOs were removed at `c06788f7` only after
strict matching-fixture reproductions passed.

Frozen `master-af7` combined binaries passed 120 indexed Rust units and all
14 selected native files in both execution modes. Its first focused PTY case
then exited with SIGHUP; the second was not run. That batch is qualified, not
fully green. The new PTY and comprehension commits still need a combined gate.

The default-32 full candidate correctness run was interrupted after 1,402.6
seconds with 4,058 passes, 121 failures/timeouts and 92 skips; 8,557 manifest
IDs remained unobserved. It is neither a complete suite nor a timing baseline.
Sixteen preparation errors reproduce serially (fourteen missing helper `net`
effects and two reads of child PID after consuming `wait`). A separate repair
lane preserves the assertions. Isolated BusyBox AWK passes 49/49 at one worker
but fails 34 cases at 32 workers because of existing five-second child
deadlines. Fair scheduling and bounded per-file admission now belong to one
complete scheduler lane; deadlines and coverage remain fixed. Other runtime
failures need exact isolated diagnostics. The host has 16 physical cores and
32 hardware threads. No fourfold speedup has been demonstrated.

Wide lanes remain active: complete typed producer provenance and owned-master
port; all 6,760 original module-publication comparisons and owned-master port;
canonical stage-call checking without arena clones; configured-width lint
closure including loud imported-config errors; compiler-driven blanket
retirement; and feature-on XSHI thread stack bounds. Independent review is
required before acceptance. Original baselines and historical artifacts remain
unchanged; final review artifacts reside in the real master worktree.

## Wider closure lanes and accepted boundaries — 2026-10-12

Master `4a14be26` accepts the runner preparation (`8e336d8d`), library-owned
XSHT binary entry (`11daa6a0`), 32-worker default (`aa5aca3a`), and final
structural-zero enforcement (`f6bf743d`). The schema observer accepts generated
eager classification and increasing retired ordinal gaps (`33946059`,
`0d2e6ef0`); all 152 actual instruction rows remain counted. Original scanners,
tests and baselines remain byte-preserved. Private focused gates passed;
this combined source has not yet been built or accepted as final.

Lint boundary closure (`1924987f`, `df5a82d1`) uses one reusable expression
worklist per body proof. Semantic declaration closure (`c36d336d`) removes
three wide wildcards and the separate module allowlist. The old flat-map
positive fixture failed identically on matching fb04: `84c6ef94` uses explicit
propagation and retains a full/compact rejection regression for bare Result.
Canonical SPEC/checker corrections (`f4134bf3`) preserve the inferred-Int
positive and explicitly reject opaque Any sort keys. Generated docs remain
pending. Performance harness `4a14be26` fixes per-round medians and records
actual container resources; its 35/35 comparator result is readiness-only,
using the frozen original release binary, with no timing samples.

The first full eight-worker reference was interrupted after 945.472 seconds:
1,730 successes, 29 timeouts and 33 skips, with only 1,792 of 12,538 IDs
terminal. This is incomplete failure evidence, not a walltime baseline. The
timeouts expose coarse declarations: 2,112 AWK fixtures in 26 sequential
groups and 4,000 checksum inputs in four sequential groups. AWK and checksum
sharding now have independent owners and must preserve every original input
and assertion. Both performance arms must share those test repairs. No
candidate full-suite run or 4x speedup is claimed. Coordinated timing locks
were released; automatic build queues and correctness work may proceed.

W1 final call readers are sealed at `4fad095a` plus `b88cc72f` and under complete
independent review. Exact-cc512 allocation slopes match for direct/method calls
and improve module calls in normal and forced modes; all three frame layouts
match. W4 owns complete callable producer provenance and its owned-master port;
W9 owns publication corrections, normalized full parity and its later port.
Compiler-driven blanket retirement runs independently with shared-file
requests directed to those owners. None of these pending lanes is accepted.

The fb04 frozen xsh was accidentally rewritten by an objcopy section-dump
command after the 86 native log outcomes. Its exact original hash was restored
from the untouched main build, the transformed ELF was preserved, and frozen
artifacts are now read-only. The recovery receipt qualifies affected later
observer evidence; the native logs predate the mutation. Of those 86 files,
85 passed and an overbroad serial whole-core-ls gate timed out; its two changed
PTY cases need a focused combined-source gate instead.

## Owned frames and runner preparation — 2026-10-12

Integration is on master `cc512718`. Accepted owned execution port
`ed6fb5e5` retains the c746 typed readers and removes every legacy recursive
strategy; resolution also retains the schema-generated eager-atom predicate
from `d7e730f5`. The private port passed 664 tests across 48 files per mode,
136 module/stream/signal tests per mode and 120 indexed Rust units. Independent
source review found no integration defect. These are private-lane gates;
combined master verification remains pending. Call-argument cursors are now
ready and have an exclusive private lane from cc512.

The low-stack matching verification control fails with the engine's 32-machine
limit (exit 1); the owned candidate passes. The older release control's native
stack overflow is supplemental, mismatched-profile evidence. Do not combine
those controls. Scalar views, schema-generated eager membership and ownership
will be verified together at the next frozen master build.

The valid frozen f884 pilot has 48 concrete test IDs. Listing alone took
10.18 s; the existing default eight workers took 12.38 s and 32 workers took
12.13 s, both 48/48 passing. This is about 1.02x, not the required 4x. The
runner lane is implementing bounded parallel file preparation, preserving
sorted discovery, deterministic errors and fresh per-case evaluator state.
The earlier 117-case pilot was rejected before timings: quoted fixture tests
inflated static counts and a core ls helper lacked its `net` effect. Master
`ad02581f` fixes both fd-reading helpers; their exact two PTY cases pass.

W9's corrected resource scope avoids repeated imported-owner analysis; the
heavy lspci input has exact symbol-rendered publication in all four profiles.
The broad `structured-full-v3` attempt was stopped and preserved: diagnostic
hashes matched its 800 terminal outcomes, but source 15 exposes a real surplus
record-constructor publication difference. That lane is fixing the producer
boundary before rebuilding normalized receipts and restarting the matrix.
No full W9 parity or final structural/performance acceptance is claimed.

## Native runner performance and accepted follow-ups — 2026-10-11

The owner explicitly added full `xsht test` parallelization, test-pollution
repairs and a hard measured target of at least 4x lower wall time. The
`native_test_parallelism` Luna xhigh lane is reading the runner and designing
its isolated baseline/candidate full-suite comparison; no speedup is claimed.
Use the existing default run as the baseline, identical test inputs and
resource limits, and preserve correctness and isolation. This extends the
older exclusion of native runner changes; it does not authorize skipping
slow tests or substituting a deliberately single-worker baseline.

Master f884's full relevant integration subset is green: 84 normal files,
35 forced-frame files and 119 indexed verifier units. The frozen pair/source
and both command manifests remain under `master-f884-*`. Accepted scalar
control/defer views `08f5acf0` as `e1e43cb7` reduce 33 manual payload calls;
private 74/74 tests pass per mode, with unchanged allocation slopes. Accepted
resource/opaque-Any documentation `a5e8718a` as `34fd393e`: 19 edited snippets
pass the frozen optimized checker; generated documentation remains pending.
The canonical XSHT guide also replaces the removed with-scope lint guidance.
These later source changes still need their combined build/gates.

Cross-program CPU0 measurements use matching d5/candidate verification
binaries, hold all three compile locks and exit 0. Docker multiplexes labels
and timings from separate streams, so their ordered sequences are paired,
not neighboring log lines. A previously prepared wrong release-baseline
script was never run and is retained as invalid. Common and dynamic CPU are
comparable; no stricter or full-host-idle claim is made. Independent review
finds the early external-kind lookup safe under fixed stdlib linkage.
Typed-callee loop-slot corruption now reproduces a real verifier acceptance
hole; its coordinated fact/signature mutation stays as a regression while
producer provenance is completed. W9's imported resource traversal has a
red counter test (two owner visits instead of one); its root-only entry pass
must preserve every profile's published facts before acceptance.

## Master integration checks and remaining reviews — 2026-10-11

Master `dc0ad171` passes the pinned x86_64 musl verification build. Its
runtime/API/time/system gates pass; initial btop and TUI failures exposed a
missing core module search path and three positional symlink calls. Accepted
`a458238f` as `0065d41c`: those calls now retain their original operands and
assertions with `target:` / `path:` labels. With `XSH_MODULE_PATH` set to the
master core directory, all 121 btop tests and 15 TUI tests pass using the
frozen dc0 pair (`master-0065-core-gates/manifest.json`).

Statement-block cursors `5e6b65b0` are accepted as `f884f8ad`; merge conflicts
retain the newer typed imports and remove both obsolete statement and branch
pools. The lane removes pool-counter assertions because the pool is gone,
keeps the 200-iteration observable result, and adds foreign-store cursor
validation. Master f884's pinned optimized build passes; its matching frozen
pair/source manifest are `master-f884-bin` / `master-f884-source.json`.
All 119 indexed full-program Rust units pass, including the unchanged height
boundary assertions and the corrected checked-slot diagnostic regression.
The broader normal/forced native integration gates are running, not yet
claimed green. Build output identifies one obsolete `BLOCK_STATEMENTS`
import, reserved for the next source cleanup batch.

The isolated CPU0 W9 reference/candidate/candidate/reference run holds all
three compile locks and passes all four profiles with repeat-identical
publication and diagnostic receipts. Default checker CPU is about 4.31 s
reference versus 7.37 s candidate; serialization costs are similar. Resource
analysis visits each imported module twice. A root-owner traversal fix must
retain cached module facts and exact publication before the full 6,760
comparisons restart. Raw logs and command manifest are `w9-isolated-*`.

Cross-program ownership and the Root-compatible typed-callee port remain
unsealed. Independent review is checking early external-kind resolution
before argument effects and missing loop/record callable-slot producer
proofs; both lanes retain their prior green tests and are adding specific
regressions where reachable. Residual scalar/control/defer typed readers are
active from f884; call-argument cursors wait for program ownership. Schema
authority for eager-atom membership has a separate read-only design task.
Performance samples taken under unrelated load remain unqualified. ARM64
and macOS remain excluded; all final platform work is native x86_64 Linux
musl in the pinned image. No final structural/performance closure is claimed.

## Integration moved to master — 2026-10-11, `e4261855`

The owner requested merging consolidation into updated master and continuing
the campaign on master. Integration now uses `/home/josh/d/laputa-systems/xsh`
on branch `master`; all future accepted changes land there. Merge parents are
master `58d321d9` (completed btop) and consolidation `a376c097`. The old
consolidation worktree stays at its preserved checkpoint, with all private
lanes and frozen binaries intact. `.work/consolidation` in master points to
the existing artifact directory; the three pinned build wrappers now accept
the master path, retain their original locks, and mount the artifact directory.
A private writable copy of the prior dependency/build cache seeds master.

Conflicts retained the consolidation lowering partition, with btop’s seven
new RuntimeOps ported to `lower/api.rs`; the API snapshot combines btop’s
functions with consolidation’s nominal FsLock representation (108 standard
records, 953 queryable/documented items). Linux verification instructions keep
verification/release profiles and the explicit x86_64-only campaign scope.
Merged-source build and btop/API/runtime integration gates remain pending.

Before the move, Root36’s optimized lint build passed 18 native files plus
256 lint, 41 CLI, and one catalog Rust unit. The current observer passed its
6 tests and 38 current ratchets; original scanner/test bytes remain preserved.
Accepted after Root36: observer `0cca7cd5` / `2b0f2313` as `9db514d8` /
`c9e9561e`, height fixture `79623ce4` as `827d1250`, sequence cursors and
one-box assertion `5bf5895c` / `ae3bc8fa` as `c46f924a` / `7e5951f1`, and
assignment/comprehension cursors `0f8279b2` as `a376c097`. These later runtime
slices still need the combined master gate. Statement-block cursors are sealed
privately and await review; cross-program ownership, Root-compatible typed
proof and W9 correctness/performance remain active. ARM64 and macOS are out
of scope; source ceilings are advisory and performance remains the priority.

## Combined cursor/schema batch and lint integration — 2026-10-11

Root's pinned optimized build at `fb813cc0` passes. The frozen matching pair
and immutable source hashes are `joint-fb813-bin` / `joint-fb813-source.json`.
All **56 normal files and 32 forced-frame files pass**, with exact commands,
source/log/binary hashes and results in `joint-fb813-gates/manifest.json` and
`joint-fb813-forced-gates/manifest.json` under the restart artifact directory.

Accepted since `431677d2`: deferred cleanup allocation improvements
`11ba983a` / `10034afd` as `bf342e30` / `1c2de715`; value schema `ead22bc2`
as `097142c1`; pattern/driver schema `057e8a7c` as `de838125`; expression If
cursor `24dbf636` as `05e8e41d`; aggregate cursors `0c58051e` / `da9c98c8`
as `a1cf88d3` / `ab963186`; pipeline borrowed readers `09306f4c` as
`3de9204c`; statement branches `9037fbc5` as `2798ea91`. `fb813cc0` deletes
the callerless branch-descriptor pool after both If readers migrated. Import
conflicts were resolved by retaining both reviewed cohorts, with no fallback
restoration. `23a1d265` documents the shared wire authorities.

The combined indexed-program Rust suite passes 116 tests and fails two:
the height-bound fixture incorrectly uses PathFrom after that row became
Scheduled, and a callable corruption expects a later generic type diagnostic
instead of the earlier checked-slot mismatch. The failing log and exact harness
hash are preserved as `joint-fb813-full-units.log` / `joint-fb813-lib-source.json`.
`86d0cafc` keeps the callable corruption and updates its expected diagnostic;
a separate lane is preserving the exact 128/129 height assertions using an
operand instruction. These changes still need their green integration gate.

W6's five commits are accepted as `9433adc2`, `cef6151e`, `dbbbbd04`,
`0d447d2d`, and `da40b6b4`. The complete table/shared traversal/fact-demand
slice passed its private compiler, Rust and native gates; independent static
review found no defect in the requested scope. It preserves existing strict
bundle diagnostic policies and fixes source-owned TryCapture facts and Tempdir
diagnostics. This later tooling source is not qualified by fb813's frozen pair.

The sequence cursor's first combined compiler probe shows common continuation
growth from 208 to 232 bytes. Integration waits for its one-box assertion-chain
follow-up and measured layout/allocation gates. Block-statement cursors and
assignment/comprehension cursors are independent active lanes. Cross-program
ownership now has meaningful private red/green evidence: 10,000 non-tail mutual
runtime module callbacks overflow the old route under a 1 MiB stack and pass
both new routes. Private module and streams suites pass; final source and
performance review remain pending.

W9's heavy index231 publication now matches all four profiles after the narrowly
authorized Symbol-to-spelling rendering, preserving all raw captures, IDs,
ordered vectors and diagnostics. Fresh-check CPU remains slower and is being
attributed before restarting all 6,760 comparisons. The versioned observer has
same-baseline proof; its current full scan and restoration of immutable legacy
ratchet test bytes remain pending. No final structural or performance closure
is claimed. ARM64 and macOS remain excluded by the owner's explicit scope.

## Owner platform scope and integration checkpoint — 2026-10-11

The owner explicitly excluded ARM64 and macOS. Development and final platform
acceptance are host-native x86_64 Linux musl in the pinned Docker environment.
This supersedes every older ARM final-gate requirement in this log. Preserve
historical artifacts; clean up only campaign-owned runtime registrations.

Root accepted through `431677d2`: cold builder/retry state `d5e24a24`, process
stream ownership repair `13ff7089`, scalar statement views `0526ad9b`, singleton
assignment reader `509a19d6`, active deferred cleanup `0bb30154`, cold process
state `bc71a8c6`, scalar expression views `ffc6bb56`, and dead statement fallback
retirement `431677d2`. The original two stream failures remain archived; the
repair restores registration at the active lexical owner before continuation.

The combined optimized build at `0bb30154` passed **48 normal files and 24
forced-frame files**, including both repaired stream cases. Frozen matching
binaries, tracked-source hashes, exact commands and manifests are in the restart
directory's `joint-0bb-bin`, `joint-0bb-source.json`, `joint-0bb-gates`, and
`joint-0bb-forced-gates`. This evidence qualifies 0bb only; later accepted source
needs the next combined build. No final campaign acceptance is claimed.

Sealed slices awaiting parent integration include deferred-cleanup allocation
improvements, statement branches, expression sequence/If/aggregate cursors,
pipeline borrowed readers, value/pattern/driver schema authorities, and the
complete lint fact-demand walk. Aggregate cursor slope checks use 72 pinned
Docker runs with revision-matched linked runtimes: ordinary allocation slopes
match the baseline, while forced-frame slopes improve. Initial per-operand
boxing is superseded by the same-continuation-box fix; integrate both commits.

Typed-callee corruption gates pass 11/11 and the native suite passes 21/21 on
the private source. Cross-program ownership remains active. W9's corrected
publication comparison recovered generic instantiation facts; annotation vector
ordering is under exact comparison. Its fresh-check CPU regression remains
unresolved. Source budgets remain advisory, with performance prioritized.

## Combined frame verification — 2026-10-11, `55d16b01`

The pinned optimized build passes. Frozen binaries/source hashes are in
`joint-55d-bin` / `joint-55d-source.json`. The 38-file combined gate has
**37 passing files and one failing file**, not a green integration receipt:
`tests/xsh/stream-producers.xsh` fails two existing process-stream ownership
cases (discarded streams stop early; retained streams outlive scope defers).
Both cases passed at 43d58034. The process lane is reproducing and repairing
this regression on the refreshed source; preserve the original failing log.
All 11 additional forced-frame files pass, including full retry, wait-until,
within, context, error/resource ownership, defer, propagation, process/cache
oracles and Tokei. Exact commands and hashes are in `joint-55d-gates` and
`joint-55d-forced-gates`. No full final or performance acceptance is claimed.

## Integrated typed readers and process frames — 2026-10-11, `0f2eb080`

Accepted scanner `896b1c59` / `5834c235` as `1ef86555` / `43d58034`.
ScanLines/ScanBytes retain synchronous optimized loops, borrowed input storage,
signal cadence, and canonical nested codec declarations. Root's pinned optimized
build at `43d58034` passed; all 34 focused files passed. The immutable pair and
tracked-source manifest are `joint-43d-bin` / `joint-43d-source.json`; the exact
commands, modes and hashes are in `joint-43d-gates/manifest.json` under the
restart artifact directory. This evidence qualifies that revision only.

Subsequent accepted slices require a new combined build and focused gates:

- Driver `113183fc` / `aa29e8ba` as `c95e2a86` / `11b74a36`: one row schema,
  eager borrowed-list corruption checks, 45 manual driver reads removed;
  checker-only hook-slot metadata remains outside encoded fields. Private
  compiler, driver, signal-hook, module and frontend gates pass.
- Atoms `46736a99` as `1a927fad`: 22 manual reads removed, canonical source
  delta -47; 49 focused normal/forced runs pass. The eager-tag partition is
  explicitly remaining dispatch debt, not a completed structural gate.
- Cache `c6f01b57` as `71cf8700`: same-program ordinary misses use the active
  machine; hits retain early completion. Eight native cases pass in each mode,
  including callback depth, resource transfer and trace lifecycle. Cross-program
  callbacks and stream producers still need their owning execution boundary.
- Process execution `16a73ee1` as `9bf36c70`: six expression rows and StmtRun
  use typed frame operands; canonical production delta +468. Private pinned
  build, schema units and focused process/timeout/trace/redirection gates pass.
  Static independent review found no concrete defect; it is not runtime proof.
- `0f2eb080` corrects ExprPipeline's strategy to its actual frame handler and
  restores the empty-spliced-command invariant comment. ExprExternalCall is
  the remaining true LegacyRecursive row.

Compiler measurements compare e3b37fc6 to ae0c1a6f: FrameContinuation grows
208 to 568 bytes, FrameWork 272 to 1,120, CallFrame 656 to 1,016 and boundary
policy 64 to 552. The independent report retains exact revision/probe hashes.
The private cold-state lane boxes command-builder and retry state once per
expression and restores those earlier sizes at its 11b74a36 base. Allocation
probes are still being finalized; this is not a throughput acceptance claim.
Root's newer inline ProcessExecutionState needs its own follow-up measurement
and fix. Defer cleanup state must also stay off the common frame layout.

Remaining active lanes: typed-callee verifier-only import TypeId provenance
(with no wire expansion), all-rule lint fact-demand audit and optimized gates,
active-machine deferred cleanup, pipeline identity-preserving borrowed readers,
and scalar statement views. Cross-program ownership planning must resolve
FullExecution lifetimes without unsafe, leaks, or copied executable IR.
W9's corrected ordered publication oracle remains red on record-constructor
instance order after missing instances were recovered. Preserve all previous
attempts; do not normalize away publication fields or vector order. Check-only
CPU overhead on the heavy module probes still needs repair before the complete
6,760-pair restart. Source budgets are advisory; these correctness, ownership,
structural and performance requirements remain hard.

## Error and statement-match frame slices — 2026-10-11, `919f34c9`

Accepted `3ecd88f5` as `23a127e9`: Error/Fail/Abort use typed borrowed shared
apply and remove their recursive/manual readers. The structured error payload
has only its live variant; retired tag-zero handling is not restored. Immediate
operand conversion preserves validation-before-later-effects behavior. Private
pinned build, two schema units, error-families 22, fail 19 and exit 14 cases pass;
new forced child regressions pass. A forced resource probe stops earlier at
unmigrated SpawnRun; its normal case passes. Canonical source receipt is pending.
The useful exit-unwinds comment is restored with this checkpoint.

Accepted `506903de` as `919f34c9`: statement Str/Tag matches consume borrowed
arm lists on the active machine, with original block scopes/control flow.
Canonical production delta is +34. Private pinned build, default match-else 9,
string-patterns 13, forced exact string/tag cases and default/forced escaping
resource oracle pass. Combined compilation/gates for these new slices remain
pending, not covered by earlier retry/source-matched binaries.

Scanner `896b1c59` and driver `113183fc` remain private pending codec follow-ups:
new borrowed nested payload readers must share authoritative order with their
existing FullCodec encoder/decoder. Scanner uses existing schema macros;
driver scalar exports can reuse their allocation-free decoder, while hook slots
need a precise checker-only field initialization clause. Do not drop eager
corruption validation or duplicate driver-slot metadata while resolving these.

## Canonical documentation refresh — 2026-10-11, `2698cb7c`

Accepted private `a61832d3` as `2698cb7c`, updating only SPEC source prose and
architecture against integrated code/tests. It clarifies scope ownership and
`process.spawn` reaping, flat-map's `check.flat-map` rejection of implicitly
returned Result values, compact/pretty JSON sorting, declaration/type-expression
facts and the current indexed stage/frame architecture. It explicitly retains
unfinished LegacyRecursive coverage instead of claiming fallback closure.
`git diff --check` passes. Generated documentation is intentionally pending;
`make docs` and `make docs-check` run at final documentation verification.
Their development path builds release tools, so it is deferred until final
verification rather than triggering a release build during active frame work.

The full corrected W9 run is still stopped while exact publication projection
is repaired. Preserved check-only thread-CPU measurements on lspci/lsusb show
roughly 50–65% candidate overhead; investigate repeated checking/preparation
before restarting the long pair. These two probes are not an overall benchmark.
Frame enum sizes and cold continuation state allocations have a separate
compiler-measured review; no layout or speed conclusion is yet claimed.

## Retry boundary repair — 2026-10-11, `ae0c1a6f`

Accepted `8b4cfefe` as `ae0c1a6f`: include Retry in the nearest expression-value
boundary search. Existing failures reproduce before the one-line repair and
pass afterward. The pinned private optimized build and focused suites pass:
retry 24, wait-until 15, value-blocks 20, propagation 13, context-scopes 25,
error-resource ownership 16, defers 17 and within 20. Forced loop-transfer and
Bool-callback retry pass; broader forced retry still hits unmigrated ExprError.

The repair lane and Root have identical tracked trees except this handoff.
Exact binary hashes and that difference are recorded in
`joint-retry-fixed-reuse.json`; the source-matched pair is copied to
`joint-retry-fixed-bin`. This reuses verified compilation instead of repeating
an identical build. Root's ordinary target directory still holds the earlier
failing pair: use the explicitly frozen repaired pair for ad-hoc probes.
No full final suite or performance pass is claimed.

Independent lanes now own remaining Error/Fail/Abort, six process-execution
forms, statement Str/Tag matches, and line/byte scanners. Driver typed readers,
typed-callee verification, W9 records and lint transport remain separate.
W9 diagnostic replay has a focused regression for entry/import ordering after
moving cache state outside PreparedConstants; exact first-source publication
parity must pass before another full paired run.

## Performance priority and runtime integration — 2026-10-11, `3a2f292a`

The owner relaxed source budgets and prioritizes performance. Production and
physical-touch caps, temporary source ceilings and final strict shrink are now
advisory reporting. Original scanners, baselines and provenance remain intact.
Structural zeros, ownership, correctness, documentation, platform and measured
performance gates remain required. This policy supersedes older source-budget
blockers below. `39f75ea9` records the policy; `e3b37fc6` applies it to the CLI
without removing source rows or raw rise audits. Its successor provenance keeps
historical artifacts immutable. Focused policy 2, ratchet 38 and gate-plan 7
native tests pass on the frozen optimized pair. Structural rises still fail.

Accepted lint baseline-fact repair `b7ccfa01` (-21 canonical production lines),
shared Env conversion `b48979cb` (+2), callerless helper deletion `01e7207c`
(-46), and both statement-frame commits `766ac3b3`/`8b95a363`. Statements use
borrowed list positions rather than copied future operands. Their complete
cohort, including cursor and Env prerequisites once, is +225 canonical lines.
Branch/host commits `14c7c367`/`29530987`/`a5e81ca2` add +190; filesystem
`c12460e7`/`29a1dda4` adds +135. `328cf3a0` preserves branch leaf support in
shared operand iteration and reconciles the surviving operand-class inventory.
Private focused gates and independent reviews qualify those sealed slices.

The combined `328cf3a0` build passes; 19 of 20 focused native files pass.
Wire-enum Print propagation fails with the wrong return destination. The
minimal regression and root-cause fix are accepted in `57deaf44`; its private
normal and forced regression, 11 wire-enum and 13 normal propagation cases pass.
The old combined binaries and immutable source hashes are frozen as
`joint-328-bin`/`joint-328-source.json`; do not claim the old run was all green.

Process-command construction `70a16f66` is integrated as `f35741ef`, and
loop/retry `266ea41a` as `3a2f292a`, preserving statement/branch handlers in
shared executor conflicts. Their source deltas are +335 and +158 respectively.
Private pinned focused/native gates pass; whole forced exploratory sweeps still
hit unrelated unmigrated handlers. Combined optimized compilation is running
under `joint-process-loop-build.log`; combined gates and independent retry
review are pending. No comparative performance result is claimed.

The combined build is green, but its native sweep exposes an integration defect:
retry passes 17/24, value-blocks 19/20, and propagation/wait-until have related
failures. The expression-value boundary selector omitted Retry while the
completion dispatcher handles it. Preserve these failing receipts and the
`joint-process-loop-bin` pair; an isolated repair lane starts at `2fe799a3`.
The first twenty files, process, Unix process, within and stack bounds pass.
The main driver accidentally names nonexistent `unix-process.xsh`; this entry
is a missing-file failure. Supplemental `unix_process.xsh` passes separately.
No combined runtime acceptance or performance pass is claimed.

The exact isolated `982bd755` whole-tree scan is 192,988 src / 49,714 xsht.
The runtime reader batch replay is -222 canonical production lines and 3,361
physical Rust touches excluding its separately counted cursor prerequisite.
The earlier additive baseline was one line high; frozen canonical scans settle
it. The concurrently edited c29 whole-tree scan is explicitly invalid.

W9's old parity hashes sorted individual Debug lines and cannot prove fact
associations or ordering. Corrected structural hashing preserves vectors,
diagnostics and key/value associations. Its paired run has matching diagnostics
but a publication difference from index zero: candidate PreparedConstants adds
wire_diagnostics. The run is stopped with all partial receipts preserved;
move replay cache state outside published output and prove exact first-source
parity before restarting. Completed old shards are provisional only. Early
candidate timings are slower; source-specific CPU profiling remains necessary.
Typed-callee coverage currently passes 20 of 21 native cases; module-export slot
proof remains open. Complete lint transport and typed driver readers are private.

## Display/cursor integration and combined gates — 2026-10-11, `982bd755`

Accepted borrowed display `9225089c` as `d75d942e`: scalar output shares one
borrowed view operation and error fields use canonical data accessors. The
current lowered executor retains its Status/Error/Result policies; the Runtime
helper remains scalar-only for native assertion diagnostics. Canonical growth
is one production line, covered only by the concrete negative reader batch,
not a claim that its individual zero-growth estimate was met. Source-matched
private optimized verification passes its display unit and eight focused cases.

Accepted `0a10e7bf` as `982bd755`, only the checked typed list cursor API and
its wrong-list boundary regression. The cursor carries store/block identity,
offset and remaining count; runtime continuations keep the program's Arc owner
stable through producer suspension. Its 56 production lines belong to the
statement cohort's total, counted once. Print/Cd/Env itself remains private:
its measured +291 exceeds the +200 grant, and genuine duplication removal is
required before acceptance. Branch PatternIf follows this sealed API to remove
its copied future-branch descriptors while retaining current capture cleanup.

The combined pre-display/cursor source at `e708d1b5` passes all 18 focused
native file gates, including forced frame/stage oracles. Its matching binaries
and 2,206 source hashes are frozen under `joint-e708-bin` and
`joint-e708-source.json`. The source-matched root harness also passes ten
focused serializer/frame/retired-flag/layout units. The first stdin launch did
not execute tests because the wrapper does not forward stdin; its empty output
is not evidence. The corrected file-based driver and manifest are preserved.
These gates do not qualify later display/cursor source or final acceptance.

A root whole-tree recount started at `c29ac5a7` was invalidated by concurrent
source integration and is explicitly marked invalid. A fresh isolated checkout
at `982bd755` is being measured; no new whole-tree count is yet claimed.
W9's repaired first paired shard passes 100 sources in all four option modes;
full 1,690-source parity remains running. Its original candidate panic artifact
was overwritten on retry and the retained failure note is explicitly
reconstructed, not a source-matched original receipt. Subsequent attempts must
retain distinct output directories. All final structural/source/platform/
documentation/performance gates remain open.

## Borrowed JSON and indexed subtraction — 2026-10-11, `e708d1b5`

Accepted binary retirement `b414d80a` as `29027228`: general ExprBinary
evaluation reuses its existing frame handler, including in-place nodes, and
the duplicate recursive handler/work stack is removed. The assertion-specific
path remains. Its source-matched private optimized build and focused native
gates pass 34 cases with one fixture skip; ten manual payload reads are removed.
The three-file production deletion is 122 retained physical lines; its durable
canonical per-file receipt is being finalized.

Accepted indexed dead state `088911c7` as `3e6c8342`: callerless owned ID/tag
inspection readers and the unused defer-presence metadata producer/reader chain
are deleted. Canonical production reduction is 255 lines. Function metadata
now retains only its live kind bit, and a focused corruption oracle rejects
the retired bit. All instruction/stage rows and ordinals remain intact; defer
execution is unchanged. The pinned all-feature compile/harness builds and six
focused layout/header/function/corruption units pass. Blanket removal remains
open.

Accepted JSON `6c2bfe3c`, diagnostic repair `2ac50b9a` and test-only `31a3b88d`
as `9fbd9249`, `b885a74e` and `10ad216f`. One borrowed projection and traversal
serve tree/direct sinks; write paths avoid owned LoweredValue conversion.
Compact RecordVec keys now sort as SPEC requires. Compact map encoding checks
all keys before values; pretty encoding checks in traversal order. This also
settles compact writes' former legacy precedence difference. Filesystem entry
values are uniformly unsupported, with the public `Record` diagnostic rather
than their internal storage name. Well-shaped wire tags missing a declared
mapping entry share the specific diagnostic. No caller compatibility profiles
remain. Independent review's public-name defect was reproduced before repair;
exact-message parity covers both representations/formats. Native JSON 18,
wire enum 11 and Tokei 2 cases pass, plus three serializer units. Tokei covers
exact Stats/StatsBlob output. Canonical production delta is -165, while the
physical-file delta is -63; do not confuse those counts. Durable scanner runner,
snapshots and hashes are in the JSON lane receipt. Buffer/allocation performance
is not measured; the no-owned-child/key-copy invariant is reviewed by inspection.

The root combined optimized build is queued under
`.work/consolidation/restart/joint-e708-build.log`; these private gates do not
claim a combined final root pass. The latest exact whole-tree count remains
the earlier checkpoint; additive accepted deltas are not a fresh scan.

Private lint follow-up is sealed at `a280a307` (-11 production lines against
`738203ba`), but cumulative W6 is still +1,233 excluding the identical selected-
module prerequisite and is not accepted. Its 289 selected library units,
focused native 17/10/10/14 and catalog gates pass. Five extra integration
failures reproduce identically in the source-matched reference at `032cf2dc`
with identical test hash `cd75083d...`: both runs pass 17 and fail five of 22.
Reference evidence is `w6-reference-xsht-manifest.json`; the initial wrong
root-package filter matched zero and is not a pass. Those failures remain open
for final verification. The private table still needs root refresh, proof
transport, source offsets and full end-state closure.

FS/Path remains private at +137 after eliminating PathWrite's duplicated data
conversion; its normal/forced focused oracles pass. Print/Cd/Env's sealed
`b3b12fba` is not accepted because it reintroduced owned operand descriptors;
a checked opaque cursor follow-up is required. Branch/host leaves are underway.
Typed-callee native legal-expression coverage is still red despite focused
corruption successes; synthetic wrapper/capture transport must be correct.
W9 parity's full 1,690-source/four-option reference attempt timed out at 900s
without a receipt and is not a pass. Sharded verification retains the entire
input set and all four modes, with incremental receipts. No corpus reduction
or final source/structural/platform/performance waiver is authorized.

## Additional upstream interactive update — 2026-10-11, `e553e259`

Frozen master `76bc2de8` is merged cleanly in `ee6e12b2`. Its restored
interactive implementation remains opt-in through `xshi/laputa-asterinas`;
ordinary builds retain the disabled stub. The evaluator helper's removed cfg
was already absent in the refactored root, so this merge introduces no source
growth in either ratcheted Rust root. `8db3dfe8` migrates its new CLI target to
the existing strict verification/release binary helper. `e553e259` corrects
the documentation's feature-specific gates and marks missing PTY scenario
drivers explicitly rather than claiming those historical goldens are runnable.

Pinned native optimized feature-on builds and gates pass: 290 xshi units,
seven CLI checks (including two binary-profile helper checks), and one repeated
input/embedded preparation regression. Expected division-by-zero and blocked
directory-environment messages in the latter log do not represent test failures.
The feature-off build also passes and returns exit 1 with its exact disabled
diagnostic. Frozen feature-on binary/harnesses, source hashes and gate receipts
are under `.work/consolidation/restart/upstream-xshi-feature-on*`; the default
receipt is `upstream-xshi-default-manifest.json`. The feature-on xsh library
build has one existing dead-code warning for the native-test cancellation
variant. Neither gate proves end-to-end terminal parity. Existing development
product builds still select the default stub; release smoke expectations need
to respect that optional feature contract at final verification.

The root `adc08a9f` explicit-frame harness now passes all five focused units.
Its build and run logs are `joint-adc08-unit-{build,run}.log`. This closes the
pending root harness check in the previous checkpoint, not the remaining
consolidation workstreams or final gates.

## Frame/stage integration — 2026-10-11, `adc08a9f`

Accepted `ef7b317c` as `1819fa28`: borrowed frame apply and the single
StageEngine cover the sealed operand cohort and all 46 scheduled stages.
The surviving instruction inventory is 152, with stable ordinal holes;
pattern ordinal 14 remains a hole. The measured production delta is +246
src and zero xsht against `ca11c451`, including upstream allocation failures.
Independent review found no demonstrated defect in this slice. Its two
requested regressions are accepted from `3e97ea44` as `adc08a9f`: distinct
resource roots held by suspended Fold and parent continuation state, and
Range conversion failure before a later operand's side effects. Five focused
explicit-frame units pass on the private source-matched candidate.

The root optimized binary build passes. Focused root native checks pass for
frame apply, stage execution, typed callables, patterns, collections, streams,
resource ownership, producers, error cleanup, context scopes, module commands
and stage result contracts; frame and stage oracles also pass with forced
frames. The first driver incorrectly named two nonexistent test files;
those entries are missing-file failures, not passes. The supplemental driver
runs the actual files successfully. Both manifests remain preserved under
`.work/consolidation/restart/joint-adc08*`. Frozen root binaries and the source
manifest are `joint-adc08-bin` and `joint-adc08-source.json`; a root focused
library harness build is pending. Generated SPEC documentation remains pending.

Filesystem/Path and Print/Cd/Env cohorts, full module records, typed-callee
verification and the complete lint table/walk remain private. Source budgets
are total non-test Rust across both roots, not each root separately. The W9
candidate's imported-command regression is reproduced and fixed privately;
fresh parity and its deletion offset remain required. JSON consolidation now
has an independent lane: the published sorted-record-key contract governs
compact encoding as well as pretty encoding. This corrects an existing compact
RecordVec mismatch; it does not introduce a compatibility profile. Final
source shrink, structural zeros and all final acceptance gates remain open.

## Completed compatibility integration — 2026-10-11, `ca11c451`

At the owner's request, master `4b0b4e7c` is merged in `654bfad3` rather than
rewriting the consolidation history. Master includes the completed native
compatibility corpus and retirement of `dev/compat`; retained repository checks
now live under `dev/checks` and `core/tests/origins`. The original consolidation
baseline, corrected blanket companion, source-bound historical profiles and
private lanes are preserved. Upstream source additions require separate exact
accounting; they are not consolidation growth or permission to weaken final
source shrink. No final gate is claimed.

Twelve conflicts were reconciled with the campaign's moved owners. Upstream
Path-method dispatch guards are ported to `lower/calls.rs`, native operation
support to `lower/api.rs`, and optional Unix exec working directories to
`lowered_run/module_unix.rs`. The rooted-children ordered option retains the
current capability and resource owners. Catchable eager range/repeat allocation
failures are ported to the current pipeline implementation; retired legacy
bodies were not restored. Development workflows retain target-specific
verification profiles and add the ordinary/privileged core boundary targets
and separately bounded split runs. `ca11c451` restores the required enum
import, adapts new fixtures to result-returning APIs and target-specific product
paths, and migrates new process-test callers to the existing strict
verification/release binary helper.

The combined optimized binary build and three targeted Rust harness builds
pass. Focused native verification passes 174 cases with three capability skips:
collections, Unix process, numeric, locale, bytes, filesystem primitives and
rooted children, runtime type names, development gate/cargo/target/lifecycle
plans and all five retained repository-contract checks. Rust CLI/startup and
two cancellation filters pass 19 cases. The first scoped cancellation run was
interrupted during the test helper's implicit workspace tool build; it is not a
pass. The explicit prebuild and separately recorded retry complete, but that
retry also rebuilt workspace tools and is not performance evidence. Initial
merge failures and corrected results are all retained under root
`.work/consolidation/restart/upstream-4b0*`. The binary-only `lint_files` warning
remains. Imported byte fixtures and upstream whitespace are preserved.

Frozen current binaries and source hashes are in `joint-ca11-bin` and
`joint-ca11-source.json`; the original native gate manifest binds the initial
merged binary pair separately. Docker was stopped after interruption and was
restarted; the exact protected native image still exists at `3b8b0133...`.
Do not substitute the mutable image tag or host toolchain.

Private frame/stage joint integration now starts at `ca11c451`, including the
complete sealed frame cohort, scalar borrowed readers and all 46 scheduled
stages. It must retain the newly merged allocation regressions and strong
stream ownership. Obsolete compatibility corpus exclusions are removed in `9650c103`; lane
verification passes two corpus-walker units, one fuzz walker unit and the
checker-publication integration case. Directory links remain excluded, file
links remain readable and nested development sources participate normally.
Exact upstream accounting measures 22 changed Rust files with the same
canonical scanner: `src` +575 and `crates/xsht/src` -1. Per-file counts and source
hashes are recorded in `upstream-source-delta.json`; the derived
`envelope-upstream.json` carries precisely those deltas into interim ceilings
193,749 / 50,367. The original envelope and both baseline files remain unchanged;
final closure still rejects all envelopes and compares against original source
counts. Detailed runner/binary/image receipts remain in the private
`upstream-source-count` worktree's `.work/upstream-source-accounting/`. W9 review found that
Entry records are not yet cached and Bytes entries check Str-domain modules
first; its expanded bounded grant in ITEMS requires closing both gaps and
removing actual dead preparation state, without waiving final shrink. Typed
callee review found lost exact checker expression facts behind the partial
initializer proof; a narrow authoritative fact view is under review rather
than accepting a second general type checker or rejecting legal expressions. The exact `c495971b` whole-tree recount reports 193,228 src and 49,714
xsht lexical lines, below the derived interim bounds. It also reports 155
instructions, 46 stages, 844 manual payload calls, 42 optional raw calls, five
wide arena matches, five machine factories and 10,736 blanket lines. Its 364
source hashes, per-file counts and 48 item allowances are preserved in the
private `upstream-source-current-c495` worktree under
`.work/current-production-accounting/`; report SHA is
`a9b5f9ca9b2a6a2d5c79b25ed9fc97490c800e59860f99c3add083d3d91e8347`.
This is an exact checkpoint, not a final gate or a claim about later sources.

The frame/stage joint slice is sealed privately at `ef7b317c` with 257 focused
native executions and the layout/schema/corruption/frame filters passing.
Its actual same-root delta is +246 src / zero xsht: frame +387, stage -140 and
scalar readers -1. Four lines are overlapping removals counted only once;
three preserve upstream allocation failures. Independent review found no
proven defect but requested distinct suspended-stage resource retention and
conversion-before-later-side-effect regressions; those test-only follow-ups
are required before parent acceptance. Filesystem and Print/Cd/Env statement
cohorts continue on that private base with explicit ownership and budgets.

The prior module,
lint-table, proof transport, verifier and filesystem candidates remain on disk;
none was reset or implicitly accepted during this merge. Full structural,
semantic, final platform, documentation and performance closure remains open.

## Reader integration and native image recovery — 2026-10-11, `a5ad6e9f`

Accepted since the last checkpoint: isolated command decoder coverage
(`4e77164b`), shared borrowed type naming (`0922848b`, -2 production lines),
dead archive/command/simple-error representations (`08a9fc5a`, -271), and
borrowed pattern matching (`ed805ec8`, -143). Surviving instruction ordinals
retain holes 97, 98 and 147; pattern ordinal 14 is also a hole. Type naming
uses the executor's existing `Error` label for process-error values. Pattern
lane native gates pass 74/74 and schema/store/corruption units 20/20. Three
existing negative fixtures now have the missing match-arm commas and retain
their original checker assertions; their parser failures were reproduced on
the source-matched frozen reference before repair.

`a5ad6e9f` fixes CoreAssert grouping at its actual cause: a selected module
can have statements while the workspace's global root is empty. The existing
grouping normalizer now receives the selected source identity. The original
fixture and recheck, a selected-module unit, and a complex-expression/source-
order native regression pass. The complete lint table remains private and
still requires deletion offsets; this repair does not accept that table.

The decoder closure ledger now reconciles all original 115 rows, the final
41/72/2 canonical counts, finalized pattern and StmtProc overlays, and exact
writer/caller proofs for every remaining zero row. It explicitly supersedes
the earlier 110-row prose. There is no unresolved source-reachable row in
that frozen inventory. It qualifies reader migration, not final current-tree
coverage. Ledger JSON SHA is
`8d4127de341932898b6a94499492c4c574c33c5dd6f5abf55fb2c4ab2cbf6d82`,
in the preserved coverage-baseline worktree's
`.work/consolidation/coverage/profile-aggregate/decoder-closure-ledger/`.
Original failures, corrupt profiles and the external dependency mismatch
remain recorded. Metadata preflight confirms all 10,996 original raw files
and 115,716,334,536 bytes remain present at their expected sizes.

External Docker cleanup removed native index `65dc744f...`. Recovery from
cached layers produced protected tag `xsh-test:consolidation-native` with
index `3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
Its AMD64 manifest
`e8459b82618dff6d193055ecb58e9988382beccd4fb8f54f8e6082836f90b6cf`
and all 15 rootfs layers are exactly the old payload; only the outer
provenance index differs. Compiler, LLVM, Clang, musl, CRT hashes and alias
smoke checks match. The three ignored native wrappers now pin that index
and set `RUSTUP_HOME=/root/.rustup` for unprivileged compiler probes. Receipts
are under root `.work/consolidation/infra/recovery-native/`. Do not substitute
the mutable `latest` tag or change the ARM image.

Private work remains active: complete module records pass unchanged
1,415-source/four-mode checker parity but are over their source budget;
the full stage engine and borrowed views are undergoing final metric/gate
checks; typed/frame and filesystem cohorts await combined integration;
typed callee proof must preserve indexed and composite callable initializers;
the lint walker still needs genuine graph/collector deletion offsets. Final
source shrink, zero structural counts, documentation, platform, semantic
review and performance acceptance remain required.

## Recount and reader preparation — 2026-10-11, `78aaa3af`

The exact `6db4c917` production recount is complete. `src` has 193,062
lexical lines against 189,274 at the start, only 112 below its interim
193,174 ceiling. `crates/xsht/src` has 49,710 against 49,968, below its
50,368 interim ceiling. Positive runtime and module work therefore waits for
a genuinely source-deleting batch. The full private lint table and proof
transport also need additional deletion before their combined batch fits the
xsht envelope. Final strict shrink remains required for both roots.

The same source-bound scan reports zero lowering type resolvers, one static
type predicate, two storage mappings, 916 manual payload calls, 42 optional
raw calls, five remaining wide arena matches, five machine factory sites,
and 10,770 lines under the remaining indexed blanket. The live inventory at
this revision is still 158 instructions, 18 patterns and 46 stages. Recount
and provenance artifacts are in the private `root6db4c917` worktree under
`.work/consolidation/recount-6db4c917/`; scanner SHA is
`da2c3e70cf6cc08e79ff7eba3ce5ebaf05848203cd7f21797150141155f35e79`.

`c78a8031` accepts the focused pattern mismatch test. Its isolated frozen
profile proves the previously zero Facet and Tag completion reads; the
coverage overlay is separate from the original aggregate. Exact writer/caller
audits qualify unreachable specialized error-pattern, archive and recursive
pattern-control paths. The frozen StmtProc gap now has an isolated native
profile with nonzero payload and completion reads; its new fixture still
needs the current opaque-Any validation migration before integration.

The CoreAssert migration fixture fails identically under three frozen pairs.
Its retained recheck assertion also rejects the fixer's redundant parentheses,
so this is an existing invalid-fix bug, not permission to change the expected
text and accept unchecked fixes. The repair remains private. Independent
frame review found that operand-classified rows could enter a new machine
through an enclosing in-place selection; the private fix marks those rows
scheduled and retains a test-first verifier regression. Neither candidate is
accepted yet.

The campaign skill now records the owner's Luna xhigh policy. Stopped
accepted-lane incremental caches were removed with receipts under root
`.work/consolidation/restart/`; binaries, harnesses, source evidence and all
original coverage data remain preserved. No final gate is claimed.

## Integration checkpoint — 2026-10-11, `bfb18460`

The campaign remains active under the owner's unattended completion instruction.
All subagents use Luna xhigh. The historical checkpoints below retain their
source-qualified results and do not describe the current tree.

Since `9ff2c334`, the integrator accepted stage order/drain oracles
(`b6c737e4`), exhaustive lint and semantic policy cases (`1ae2c671`,
`cb5ea4fc`), borrowed captured-resource traversal and balanced failure cleanup
(`fd27eb38`, `f6fe4479`), and strong script-stream ownership with removal of
the global producer registry and statement sweeps (`a5f8b89f`). The latter's
follow-up `08479519` clears an unstarted cancelled producer's frame; a
test-first host regression reproduces the retained-resource cycle without
that fix. Ownership, producer and error-cleanup lane gates pass 26/26, 6/6
and 16/16, with eight capture lifecycle units passing. The last joint frozen
pair is `joint-cb5e-bin`, before strong ownership; its 48 combined native
cases pass. A source-matched joint build after the next accepted batch remains
required.

Structural scope correction is integrated in `381e06b6`, with documentation
clarified in `bfb18460`. Original baseline bytes are unchanged. The verified
companion measures blanket scope with the same canonical scanner on both
revisions: start 47,061 lines and `9ff2c334` 10,770 lines. Item allowances are
inventoried separately; they are not a replacement for blanket removal.
Provenance and 38 ratchet / seven gate cases pass. The latest root has not
yet been recounted. Final source shrink and zero structural targets remain
unchanged.

Frozen baseline coverage completed 6,122 outcomes in 545 groups, plus 18
isolated exact-ID replays. Original 107.77 GiB raw evidence, original failures
and invalid/truncated profiles are preserved. Bounded aggregation and exact
object attribution are complete; the sole final xsh mismatch is an external
`ignore` dependency function from mixed-domain data, with its exact attribution
recorded. The source-owned decoder inventory initially has 41 covered and 72
zero rows in 21 groups; a separate targeted pattern profile proves two more
rows. Missing rows still require source-reachable tests or exact writer and
caller closure. Passing stage tests did not hit the old indexed block decoder
rows and are not coverage acceptance. Artifacts live under
`/home/josh/d/laputa-systems/xsh-consolidation-lanes/coverage-baseline/.work/consolidation/coverage/profile-aggregate/`.

Private work remains unaccepted: the full lint table, source-owned lint proof
transport, complete per-module records, typed-callee verifier follow-ups,
typed frame cohorts and the full 46-row stage engine. W9 may retain one
file-local effect probe and final pass, as its approved design specifies;
preparation reuse around repeated whole-bundle probes is not its end state.
Stage scheduling, runtime reader migration, final dead-code and classifier
closure, docs, final platform and performance gates remain outstanding.

## Integration checkpoint — 2026-10-11, `9ff2c334`

The campaign remains active and unattended, with Luna xhigh subagents.
The latest joint optimized build passes in the pinned native image. Frozen
binaries, source hashes and the build log are under
`.work/consolidation/restart/joint-9ff2-*`. Combined native gates pass context
scopes 25/25, typed callables 20/20, formatter layout 65/65 and modules 31/31.
The existing binary-only `lint_files` warning remains; no blanket was added.

Since the previous checkpoint, accepted changes repair imported error-facet
canonicalization (`051a828d`), remove unused semantic helpers (`0d46d837`),
share guarded storage conversions (`66ce7b80`), generate borrowed stage views
(`8187bd11`), publish nested checked RunForm types (`35a132d5`), preserve string
contents during broken-call formatting (`aa262c64`), and stabilize example and
process-fixture verification (`45f5af63`). Resolver and storage-authority
metrics now classify their actual source shapes (`e6aa1c0d`, `9ff2c334`).
Borrowed views are preparation, not completed runtime-reader migration.

Baseline canonical coverage has completed all 545 groups and 6,122 outcomes.
Original failures and profile gaps remain explicit. Supplemental exact-ID
replays are running; the original 107.77 GiB raw evidence remains untouched.
Complete combined counter coverage and missing-arm tests still gate runtime
reader acceptance. The captured-resource traversal is being rebased and
verified against this joint root before strong producer ownership proceeds.

Full module-session checking, shared lint proof/fix validation, the complete
lint rule authority, typed callee verification, remaining exhaustive policies,
frame/stage scheduling, dead-code closure, generated documentation, final
platform and performance gates remain in progress. Private lane results do
not substitute for integration acceptance or final campaign completion.

## Current integration checkpoint — 2026-10-11, `65594365`

Follow-up checkpoint `ffabb4c7`: the joint `65594365` build passes in the
pinned native environment. Its frozen binaries and source manifest are in
`.work/consolidation/restart/joint-6559-bin` and `joint-6559-source.json`.
Focused combined native gates pass 15 pattern conditionals, six module-command
contracts and 16 test declarations. The initial invocation incorrectly passed
multiple filters; its rejection is preserved, and the corrected sequential
invocation ran all 37 cases. Borrowed pattern views and bridge fixes are now
integrated (`602ccd51`, `f7df0fe8`); the rebuilt root static harness passes
five actual schema/store/corruption cases. Linux module allowances are removed
in `45e70000`; this warning-only change has Linux compiler evidence and no
claim of non-Linux validation. `ffabb4c7` removes temporary child vectors
from annotation linting, with 60 focused lane tests passing.

Baseline coverage is through group 400, 4,400/6,122 outcomes. Group 385 has
three instrumented parallel-map timeout failures with valid profiles; those
failures are retained. Existing profile gaps remain groups 30 and 181.
The full W9 driver needs a real owning multi-entry session and complete
module output projection; earlier source-aware plumbing was not such a
session. Necessary temporary budgets are expanded in ITEMS for that driver
and the full lint table/enabled-state consolidation, without relaxing final
source reduction or structural acceptance.

The owner’s unattended completion instruction remains active. All subagents
use Luna xhigh. The historical checkpoints below are retained; they are not
the current branch state.

Since `d15511a3`, accepted slices include checked constructor facts, shared
stage authority, grammar ownership repairs and the fixed-seed CLI grammar
regression, the first forced frame cohort, exhaustive formatter/checker/lint
and runtime policies, resource escape lint repairs, transfer telemetry, and
script-backed JSON profile publication proof. The latest batch integrates
runtime policies (`a63b00ef`), mutation recognition (`4aec944a`), required
replacement argument printing (`176061b0`), generated stage metrics
(`3392c25b`), and immutable borrowed workspace entry views (`65594365`).
The borrowed view adds public `CheckBundleView` entry-range APIs; the arena
remains immutable and site IDs remain stable. Its lane gates passed the
two-root isolation proof, module/cache integration and optimized checker tests.
The fuzz lane passed the unchanged 1,500 grammar and runtime seeds and 1,000
mutants; the runtime gate took 13.41 seconds. These remain lane results until
the latest joint candidate is verified.

The joint optimized build is running. Its initial attempt encountered
read-only cached compiler outputs; the integrator detached 521 read-only
target artifacts without modifying their linked cache originals and restarted
the same pinned build. Both logs are preserved under
`.work/consolidation/restart/joint-6559-build*`.

Baseline canonical coverage has passed group 372 of 545, with 4,088 of
6,122 outcomes recorded. Groups 30 and 181 retain explicit profile gaps;
the original 107.77 GiB raw evidence remains untouched. Runtime reader
acceptance still waits for complete combined coverage and missing-arm tests.
The captured-resource fix, typed frame continuation cohort and borrowed
pattern views are private review candidates, not accepted root changes.

Full module record reuse across entries, the complete lint rule authority,
all frame/stage scheduling and borrowed runtime reader migrations, remaining
wide policies and dead-code blankets, generated docs, final platform gates,
semantic reviews and comparative performance acceptance remain outstanding.
The imported error-facet regression is bracketed between `b7e5c5f4` and
`edac89d0`, and is being repaired with the preserved assertion.

## Current integration batch — 2026-10-11

Latest root batch ends at `d15511a3`: ListRest codec validation ordering is
repaired (`8d7ca885`), ten unused checker impl allowances and their dead arity
helper are removed (`ce633f7b`), nullable storage mapping is shared
(`5accb8c4`), lint facts share one immutable checked output (`bc1708cb`), and
redundant-close uses are collected during the existing walk (`10ac1f4a`).
The reachable module stream fixture is corrected separately (`41c4a0f4`),
and immutable preparation is reused across effect probes (`d15511a3`).
The ListRest lane passed 15 pattern-conditionals, 24 pattern tests, and the
retained verifier corruption unit. Lint lane gates passed 244 lint units,
38 CLI lint units and five redundant-close units. Module preparation passed
its reuse/domain unit, shared-arena integration case, module/stream native
gates and all 46 sema integration tests. These are lane results; the joint
optimized build and focused combined gates remain in progress.

Joint `d15511a3` optimized build now passes. Its frozen pair is in
`.work/consolidation/restart/joint-d155-bin` (xsh SHA-256
`a10e1bdd27d12a5a6e065a2518360a8f3b8478e7f4b85b255b4ab4fe5ce40dde`,
xsht `39271709b4eb79388d70b6d4a46ca2398823d253ca796c2193ba9f6c75ff17b1`).
Combined native gates pass pattern conditionals 15/15, compact declarations
4/4, module-command contracts 6/6 and resource ownership 22/22. Both the
ListRest corruption unit and module preparation/domain unit pass one actual
case each using the rebuilt static test harness. The initial timeout argument
`30` was invalid, and an initial wrong module filename ran zero cases; those
logs are retained and excluded. Corrected runs use `30s` and the actual file.
Thirteen lint-proven redundant FsRoot defers are removed in `78052d1d`;
all 13 nearest native cases and the retained child/parent lifecycle case pass.
The intentionally retained child defer still has its lint finding.

The public lint Rust options now accept shared `checked_output` rather than
21 separately copied fact maps; synthetic maps remain test-only. Cross-root
module reuse, the complete lint declaration/dispatch table, all frame and
runtime-reader migrations remain unfinished. A returned producer stream still
closes a captured FsRoot too early; the retained failing native test owns the
capture-transfer fix before reachability sweep removal. Grammar repair must
preserve existing parser ownership behavior rather than change the language.

The campaign remains active under the owner's unattended-completion instruction,
using Luna xhigh subagents. The older reboot pause below is historical.
Current implementation includes the complete BindingId adapter (`498e02e0`,
`57aa1abb`), R3 checked resource facts (`1f6c6db1`), full explicit annotation
facts (`01f786b9`), named checker profiles and unconditional warning publication
(`c8fc9e26`, `530d957c`), module namespace isolation (`5a4d87ff`), eight further
lint hooks (`7d384613`, `d873b984`), and the error-family fixture correction
(`5d2661a0`). `CheckOptions::migration_diagnostics` is removed; named execution,
check CLI, and interactive factories choose entry policy without hiding warnings.
Execution gates on errors, while checker-only entry constructs remain rejected.

The latest accepted slices are resource runtime fixture coverage (`b5f9a685`),
opaque FsLock/API and missing-module fixtures (`62d67705`), grouping traversal
and retained slice-range regression (`4b2363ba`), stream ownership/worker cleanup
(`f10506cb`), and exact local/capture callable verification (`d325c86c`). Each
has its lane's focused green gates; the joint all-feature XSH/XSHT check passes.
Combined optimized native gates are pending. Neither stream cleanup nor grouping
closes the larger resource or traversal workstream. The R3 runtime bridge awaits
budget and strict missing-fact review; the Any checker remains unintegrated.

Baseline canonical coverage is running under the shared suite lock with the
sealed instrumented pair explicitly on PATH and XSH_BIN, unprivileged UID 1000,
and tmpfs test storage. Original 107.77 GiB raw evidence remains untouched.
New group profiles are hashed and validated before bounded LLVM aggregation;
only validated new raw inputs are deleted, retaining aggregate evidence rather
than claiming lossless individual raw archival. Earlier wrong-environment and
wrong-top-level-binary phases are preserved as superseded supplemental evidence.
Coverage and final runtime-reader acceptance remain incomplete.

Further integration: `c49c05e5` replaces `lint.prefer-with-scope` with
default-on `lint.redundant-close` and shares checked output through lint workers;
its focused lint gate passes 282/282. `349124ad` corrects the whole-initializer
run propagation fixture while proving guarded hops and runtime output unchanged.
W9's borrowed reachable view and per-root isolation proof are `a8cd70a2` /
`a963a005`; repeated probe preparation remains for the next slice. Any opacity
and its nine hidden-fixture migrations are now integrated (`4a09be2d`,
`2be723b9`, `c2676b8b`). The joint optimized build passes; focused opacity
102/102 and migration 155/155 gates pass. Known shell recovery and With
initializer failures remain assigned to the binding owner. A newly exposed
`fs.dirs` owner-registration regression is assigned to the stream owner;
do not claim the full candidate gate green.

`1e35ab66` removes the inherited CLI dead-code blanket and unused Rust
convenience APIs `check_script`, `check_script_with_options`,
`check_paths_with_options`, `check_paths_with_summary_options`, and
`CliOutput::success`. CLI dispatch still uses `check_paths_timed`; its focused
check/coverage gates pass. Legitimate exported coverage constructors and syscall
summary data retain item-level allowances. `4a7447b9` removes four lint-proven
redundant FsRoot defers in cp/mv/curl/split, preserving explicit unlocks;
focused core gates pass with two TLS fixture skips. The SPEC template now
records affine local checks and stream cleanup; generated docs are still pending.

R3 bridge integration is now `e513f574`, `1d452ec1`, and `eb70eb1a`, with strict
`ResourceTransferSite::Assignment` facts: locals carry explicit false; missing
outer facts block lowering. The complete bridge uses the expanded +340 interim
allowance and still requires final source deletion offsets. Binding recovery
and in-place With error handling are `ae9269bd` / `9c95f599`. The joint optimized
pair at 9c95 passes resource ownership 21/21, With 19/19, mistakes 5/5, nine
resource units, the missing-fact blocker unit, and three callable units. An
initial exact unit filter ran zero cases; it is retained but excluded, and the
corrected filter passes one actual case.

The latest integration batch includes shared field lookup (`b7e5c5f4`), two
pattern traversal helpers (`7aeeceee`, `bdd01efb`), trace/table blanket removal
and unused helper deletion (`213f5a08`), checker declaration allowance removal
(`c0fe54a7`), checked type-fact resolver contraction (`edac89d0`), checker-side
main spread diagnostics (`6d42c588`), and the generated 18-pattern codec plus
coverage-tool schema repair (`a99aba94`). Undocumented Rust trace helpers
`TraceSink`, `MemoryTraceSink`, and `TraceNormalizer` are removed; normalization
needed by tests is test-local. The coverage report now includes
`pattern_variants`. R4 delegated-owner and fs.dirs registration fixes are
`2147695c` / `a514c417`; their lane gates pass foreign workers 6/6, filesystem
24/24, and stream producers 5/5. Current joint compile/build gates are queued.
Reachability sweep removal and one expression-pattern success scan still need
their typed lifecycle proof; neither is silently deleted.

W9's canonical no-argument baseline lint completed on 1,160 discovered files;
the contended 160.97-second run and same-process profile are setup attribution,
not comparative performance acceptance. ARM compiler/environment recovery is
verified, but final ARM product and release gates have not run. Generated docs,
structural closure, fixed-seed reliability, semantic review rows, and comparative
timings remain required before goal completion.

## Resume update — 2026-10-11

The owner resumed the campaign from this checkpoint and changed all subagents
to **gpt-6-luna at xhigh effort**. This supersedes the older Sol-only policy
below and in the campaign skill. The preserved checkpoint remains historical
evidence; interrupted gates still require rerunning. Integration resumed at
`e8533c12`. Docker was stopped after reboot; the integrator restarted the
daemon, but the pinned image is not currently listed. Builds remain held
while the preserved test environment is recovered.

Recovery update: the original native image is absent from Docker's store.
The compatibility session rebuilt the byte-identical `Dockerfile.test`
(SHA-256 `7b8017b69d3ecc51f04ac2fe481af7e5059e0bcf0f1494fb39abecadcfb0e5ff`).
Resumed native gates use immutable image
`sha256:65dc744f6c560f189716ea193bd765149f4691c80392663e630813033ebc53a3`.
The base and rustup installer layers match; rustc commit `574ff7d98`,
LLVM 23.1.1, clang 23.1.3-r0, and musl/musl-dev 1.2.6-r5 match retained
historical evidence. Rootfs hashes differ, so this is regenerated environment
evidence, not byte identity with the old image. Primary/secondary/third native
wrappers pin the resumed image; their original copies remain on disk.
Image metadata and compiler/CRT/libc hashes are saved under
`.work/consolidation/restart/resumed-native-*`. Focused lane gates are released.

Active independent lanes: W1 schema classes, module hooks, checked declaration
facts, source-aware check session and telemetry, BindingId adapter, grammar,
fuzz grammar, lint rule inventory, and opaque Any discovery. Each uses its
existing worktree and preserves unfinished diffs. Source-aware session plumbing
now has an assigned owner; shared-file edits are scoped to disjoint hunks.

### Resumed integration and outstanding gates

Implementation now includes W1 `1867ad25`, class column `8bccf75f`, module
hooks `562bb656`, stage constructor reconciliation `052e6192`, named argument
indexing `6154d0d1`, shared lazy-consumer descent `2b83fc91`, lint selection
`8f68cf1f`, schema-aware metrics `ba614735`, and declaration/type facts
`4b0ebefc`. These are prerequisite slices, not closed workstreams.

The class test passed 1/1 after resume. Joint all-feature compilation passed
after the two stage constructors adopted `call_plan`; the optimized product
build passed. Six actual focused files passed 54 native cases: module commands,
checker commands, command operation boundaries, stdlib CLI, named argument
spreading, and stage functions. Two initially mistyped filters ran zero tests;
those logs are retained but excluded from evidence, and the correct files
were run. The lint selector's literal `lint::` gate passed 282/282. Scanner
tests passed 15/15 and the current generated schema counts 158 instructions.
Generated documentation remains pending; canonical R2 cleanup prose is updated
in `c6b4d97c`. No full integration or ARM gate has passed.

E's focused declaration/fact units and neighboring native cases passed. Its
signature-CLI unresolved-name failure reproduced on exact before-E `d561e2c4`;
it was not introduced by E. A new source-session joint worktree at
`xsh-consolidation-lanes/check-session-joint` carries the required helper and
all loaded XSHT/runner callers. The original dirty session draft is retained.
The BindingId joint worktree is `xsh-consolidation-lanes/lower-bindings-joint`;
it preserves the integrated stage lowering and drops only its two obsolete
pipeline lookup edits. Source-aware checking is needed for imported command
word identities; the adapter fixes the broader unresolved command-splice sites.
The adapter's fold-layout leak has a failing reproduction and cleanup fix.
Do not delete legacy constant maps until the joint adapter gates pass.

Opaque Any broad discovery ran twice with changed source between runs. The
preserved first candidate recorded 3,304 passed / 21 failed / 32 skipped;
the refreshed candidate recorded 2,812 / 54 / 32. Nine hidden fixture files
were migrated in `f632c945` and `ed43ecac`, retaining behavior and rejected
program coverage; their original opacity-pair gates passed 62 and 93 cases.
The refreshed candidate's six original focused files passed 102/102. Most
remaining failures are the known command-site identity regression; R3/R4 and
other distinct failures remain recorded. Any is not integrated or accepted.
Raw logs and classifications remain under the Any lane's `.work/any/logs/`.

The Linux errno test's arbitrary family 9999 returns 1 instead of 97 with
frozen baseline, Any, and current integration binaries in the resumed
environment. Both frozen and current binaries return 97 for `c.AF_UNSPEC`.
Commit `c9a0c980` uses that standard unsupported family and retains the exact
errno assertion; the focused test passes. This repairs an environment-sensitive
test input without changing the socket wrapper or security settings. Evidence
is under `.work/consolidation/restart/net-errno/`; no full Linux gate is claimed.

Source-session/telemetry has a bounded interim +450 production / 2,200 touched
allowance for mandatory source plumbing and retained fact/index accounting,
offset by the prerequisite batch's source reductions. This does not waive
the final reduction requirement. R3 facts use explicit true/false entries at
typed transfer sites; absent facts cannot silently mean scalar values. R4,
W3 fact consumers, coverage resumption, and reveal publication versus entry
legality have assigned preparation/implementation owners. The owner explicitly
requested unattended continuation through completion; no further pause exists.

Further integrated prerequisites: reveal publication/entry legality
`b1b4e99f`, source-aware checking and telemetry `3e7e2164`, and derived
in-place instruction proofs `d51149a5`. The current combined all-feature
compile check passes. Source-session units passed 5/5 and module tests 30/30;
W2's focused verifier and native gates pass, with allocation scaling improving
from a baseline delta of 64 to 32. These do not close their workstreams.
The adapter's current joint tree is `lower-bindings-final` on `d51149a5`;
imported `cd` binding resolution still needs correction before acceptance.
Its approved stream trace fixture migration removes redundant parentheses
and retains the trace assertions. E12/E13 lint hooks are committed in the lane
but await the joint adapter gate before integration.

Baseline coverage continuation is authorized for the remaining 5,586 tests
across 538 invocations, preserving 14 failures and three timeouts already
recorded. Expected runtime is several hours; the shared suite lock and bounded
resources remain required. All 103 flagged raw profiles were accepted by the
pinned LLVM tool; one empty profile contributes no coverage. Final aggregation
and API counter completion are still pending. W2 follow-up, W4 session closure,
W5 behavior traversal, W9 module reuse, R3 runtime transfer transport, and dead
code inventories now have independent preparation owners.

Further accepted structural slices: W3 annotation storage facts `b4d5ad37`
(-103 production; seven focused native cases and corpus lowering consistency
proof pass), formatter dead-code contraction `7b231e21` (-39 production and
removal of the formatter's inherited blanket allowance), and generated stage
tags `153d8595` (-43 production; three stage proofs pass, all 46 ordinals
preserved). The unused public `Formatter::format_program_with_source` method
was removed with no repository callers. Test-only source-session option
qualification is fixed in `feaf24df`. The formatter proof exposes a preexisting
slice-range grouping defect; the grouping owner is retaining its per-case
assertions and repairing that defect, not claiming the full proof green yet.

R3 checker/type/transfer facts are ready in lane commit `76fa3df4`, based on
`4b0ebefc`, not yet integrated. Strict units and reassignment pass with the
sealed adapter assignment-root hunk in an isolated joint probe. The runtime
bridge lane uses separate prerequisite commits and a typed instruction sidecar
without payload/tag changes. The full adapter has a sealed 14-file prerequisite
patch (`b57c11f0...9ec526`); its final source-aware arena text and fixture fixes
are outside that earlier artifact. Imported module tests now pass 30/30 and
all 18 expanded native suites pass; the final lint gate remains running.

ARM infrastructure has been recovered from the exact Dockerfile. Final gates
will use image `sha256:7aec577a12163bc2aee78e759dde9ae571c692632183a83a564ae1bb88384026`
and the host-visible `xsh-consolidation-aarch64-5e66b6b8` binfmt registration.
The bounded compiler/QEMU identity probe passes; no ARM product gate has run.
Evidence is `.work/consolidation/infra/arm-environment-current.md`.

Coverage continuation preserves an observed `dmesg` failure and profiler
permission errors rather than normalizing them. The latter contaminate child
stderr and require an explicitly linked retry with writable disk-backed profile
storage. Noninstrumented frozen/current `dmesg` cases both pass; instrumentation
is being investigated before changing that test. Original statuses and profiles
remain intact. Primary compiler launches now have 12-CPU/24-GiB limits;
secondary/third retain their separate bounded queues. An in-place launcher
update caused EOF errors after some successful compiles; affected commands
must rerun incrementally to obtain a valid launch return code.

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
   behavior coverage. Reveal-type publication versus entry legality is resolved
   by `b1b4e99f` and `c8fc9e26`: checker diagnostics are profile-independent,
   reveal notes are separately published, and execution rejects `reveal_type`
   outside `xsht check` per SPEC §5.4. Existing Rust and native tests cover the
   diagnostic parity, note publication, and rejection behavior.
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

The original scanner reports 189,274 src and 49,968 xsht non-test lines.
The corrected blanket baseline is 47,061 lines; 47,705 was a scope-count error.
Source totals, intermediate envelopes, and strict shrink are advisory under
the 2026-10-11 owner update. Final closure requires zero blanket allowances and
the other structural end states. Ten semantic review rows need verified
evidence consumed by the check. The verification opt3/no-LTO profile remains
provisional; profile-cost measurement follows functional close.

Final feature probe (manual sites zero), reliability/fixed-seed gates, pinned
seven-tool comparative parity/timings, complete candidate x86_64 Linux verification,
cleanup and the closing report all remain. No comparative timings were taken;
prepared suite has 35 parity cases and eight harness tests green. Continue to
campaign completion only after the owner's resume request.
