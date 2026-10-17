# Make XSH fast to run and dependable in Laputa

Status: active, 2026-10-17. The owner authorized bounded isolated edits,
local commits and integration, pinned fetches, and owned Docker/KVM operations.
Pushing remains forbidden.

Finish the remaining acceptance work, integrate the accepted inference capabilities,
and improve ordinary invocation and substantial workloads. Qualify complete Laputa
operations on the existing Linux platform. Add no syntax-feature batches.

The deliverables are:

1. One accepted compiler baseline with reproducible gates and a concise everyday reference.
2. Integrated inference, a classified retained-annotation corpus, and joint behavior parity on real utilities and Laputa code.
3. Reproducible measurements for startup, streaming, algorithms, and filesystem/process orchestration.
4. A measured decision on prepared shipped modules, plus useful improvements to shared runtime mechanisms.
5. Disposable Laputa package, process, rooted-filesystem, and real boot/shutdown acceptance workflows.
6. A bounded release candidate with all applicable gates passing and accurate formal-evidence claims.

## Charter

The coordinator owns integration, shared contracts, acceptance, and the final review.
Use the campaign and lane contracts in the installed `orchestrate` skill. Assign
component-sized work and keep disjoint ready work running within the limits below.
The coordinator writes no feature code while implementation lanes run.

Resolve technical choices inside this document's bounds. Record only decisions a
future reader needs. Stop an item when it needs forbidden authority, unavailable
inputs, or a verification loop that cannot establish a result. Continue independent
items. An incomplete mandatory item prevents a successful campaign close.

Run correctness and parity before optimization. Measurement preparation can overlap
inference work, but representation-changing optimization waits for its scheme/IR
contract. Do not keep reopening the retired consolidation campaign. Its finite
remaining acceptance work is the first capability of this campaign.

After two repetitions of a focused check over 30 seconds, assign loop repair before
more implementation width. Compiler rebuilds are recorded separately from test
execution. A focused command group has a 120-second execution ceiling. Preserve
existing intentional stress deadlines. A slow full gate is never the repair loop.

After three refuted hypotheses, recheck source, products, inputs, and ownership.
Bisect the cause before another repair. Never retry a flaky test to green. Keep its
smallest failing case and preserve failed full receipts.
Bound each failing item's active investigation to three repair attempts or two
hours, whichever comes first. A revised causal hypothesis counts as an attempt.
Park an unresolved item with its reproducer and continue independent work.
A fresh bounded review may reopen it once with a materially different hypothesis;
otherwise retain the hard stop and report the campaign incomplete.

Put all worktrees and scratch under the owning repository's ignored `.work/`.
Check `.git/info/exclude` before the first write. Set `TMPDIR` there. Use
`.work/wt/perf-<lane>` for XSH lanes. Create isolated Laputa worktrees under
the XSH campaign's `.work/wt/laputa-<lane>` and keep each checkout's derived
state under that checkout's `.work/` or its existing ignored build paths.
Never write the sibling's normal checkout or shared artifact store.

Only commit after a separate execution grant permits local commits. Use Conventional
Commits and put capability IDs in bodies. Never push, publish, run formatters,
change CI, or update the Rust nightly. Preserve existing branches and historical
evidence. Use the cleanup skill for completed worktrees and retain ambiguous work.

## Decisions

### Owner decisions

| Decision | Scope |
|---|---|
| Pursue performance and deployment after consolidation and inference integration. | Owner direction, 2026-10-17. Improve the existing language; no syntax-feature campaign. |
| Execute this campaign with local commits and integration. | Owner launch grant, 2026-10-17. Includes the bounded isolated edits, pinned fetches, and owned Docker/KVM operations in this document. No push. |
| Allow isolated Laputa edits and pinned input fetches. | Owner answer during planning, 2026-10-17. A separate execution grant is still required. No live roots, host installation, publishing, or pushes. |
| Allow `doas` for KVM access. | Owner direction during planning, 2026-10-17. Limit elevation to required device/container/VM operations using owned paths. Do not run arbitrary recipe code as host root. |
| Retain the thirteen exact `lsipc` reference-crash exceptions. | Existing owner decision in the util-linux campaign. Independent XSH assertions remain behavior evidence and receive no exact-parity credit. |

### Prescribed decisions

| Decision | Reason and reversal condition |
|---|---|
| Start at local master `3ed58895c0b1423c8f7ac4852bb9ec9a23922da2`. | It includes the merged util-linux work and preserves the accepted Lean tree. The supplied review's `7e8881d9` predates that merge. Reseal once if master advances before launch. |
| Keep x86-64 Linux musl and the existing pinned test image. | Reuse established product settings and isolation. No new OS or kernel substitution. |
| Audit inference by capability and patch, then merge only reviewed changes. | The accepted checkpoint is hundreds of commits divergent. A whole-branch merge can restore obsolete compiler or toolchain state. |
| Keep original accounting and practical acceptance separate. | Preserve 9,194 sites, 693 eligible locals, 2,190 eligible internal annotations, and the historical 95%/85% targets. Do not claim those targets pass unless they do. |
| Classify every retained eligible annotation. | Distinguish semantic meaning, unavailable information, and missing relationships. `{}` remains a Record; Map selection remains purposeful. |
| Preserve explicit capability restrictions. | Eliding a clause is allowed only when the inferred contract is equivalent under the existing effect rules. A smaller current body does not authorize weakening a declared restriction. |
| Evaluate immutable release-shipped modules first. | Their source, registry, compiler, and feature set are controlled. Arbitrary-script caches and persistent user formats are excluded. |
| Keep prepared artifacts private and exact-version matched. | No stable serialized ABI or migration promise is needed. Reject malformed or mismatched data before execution; do not use silent source fallback to pass acceptance. |
| Treat prepared modules as an experiment with a keep/reject gate. | Ship only when correctness and measured tradeoffs pass. A rejected experiment leaves no runtime feature flag or alternate executor. |
| Reuse current measurement and VM tooling. | Extend narrow selectors and phase counters where missing. Do not build another benchmark platform, VM supervisor, proof inventory, or provenance system. |
| Test immutable package/root generations. | Laputa has no mutable install/uninstall contract. Model upgrade as a replacement generation and removal as recomposition plus reachability-based collection. |
| Use the existing `qemu-dwl-foot` profile. | It is Laputa's supported image and guest proof. Add bounded shutdown assertions there; do not invent a second distribution or a handwritten package list. |
| Keep all test/build execution offline after explicit input acquisition. | Fetch only recipe/image-pinned inputs for the selected closure. No broad `make fetch`, package-catalog expansion, or moving dependency versions. |
| Preserve source-facing APIs and the single indexed executor. | New private types and scheme metadata may change. A new public contract, native backend, daemon, or compile-first workflow is outside this plan. |
| Start with no new production dependency. | Use existing codecs and tools. A concrete missing operation can justify a dependency only after license, maintenance, transitive-weight, build, size, and startup review. |

The proposed optimization thresholds below are prescribed engineering targets,
not measured wins. The owner can overrule them before execution.

### Delegated decisions

Choose fixtures and algorithms inside the fixed families below. Freeze workload
selection before candidate measurement. Prefer an existing fixture and the smaller
reversible design when benefits overlap. Limit each mechanism to three distinct,
profile-supported experiments before recording the result and moving on.

The scheme/IR owner chooses the private encoding, linker relocation, and bounds
after inspecting the integrated inference implementation. It must preserve the
existing semantic verifier's authority. Never serialize process-local pointers,
interned integer identities without an owner, or live resources.

## Baseline

### Source and acceptance state

The local planning source is `3ed58895c`. The normal Laputa checkout is clean at
`ca658ce1da9d89c55209c25268dfdf687ff4fdc6`. Neither is qualified by a fresh full
gate during planning. `DECISIONS.md` is absent. Reuse the decision tables in the
existing campaign documents instead of creating a parallel decision register.

The retired campaigns leave XSH-1, XSH-2, and XSH-3 in `TODO.md`: util-linux
resource/close acceptance, an intermittent timeout forwarding failure, and the
remaining consolidation matrix. Historical accepted source `24cc0d704` has
13,916 canonical passes, zero failures, 78 skips, and accepted resource evidence.
It is not current-master acceptance.

The retained `7e8881d96` normal-profile receipt records 13,857 passes, zero failures,
and 138 skips. The later observer attempt records 13,916 passes, one failure, and
78 skips. The timeout selector subsequently passed five focused runs. Its cause
remains unestablished; those reruns do not repair the failed full receipt.

The accepted inference corpus checkpoint is
`15243e97f6fd751016ab28690ada2cadb6fc973a`, on
`campaign/typing-next-corpus-1038-20261017`. It records 499/693 local removals
(72.0%) and 541/2,190 internal removals (24.7%), totaling 1,040. All 149 Map
annotations on `{}` and 202 eligible private effect clauses remain. These are
checkpoint counts, not current-master counts. Planning found master and this
checkpoint 483 and 281 commits beyond their common ancestor `b235a1a43`.
The checkpoint commit records evidence; implementation lives in its ancestry.

The original 95%/85% targets remain unmet. Preserve their exact denominators.
Publish a class-by-class attainable ceiling when semantic disambiguation makes a
target impossible without a contract change. Do not shrink the denominator or
change literal semantics. This campaign accepts useful inference by the practical
criteria below and reports the older numeric targets separately and honestly.

### Fresh planning probes

Source-qualified release products from the complete `7e8881d96` profile receipt
were available. Their recorded digests matched the actual files. `xsh` is
30,158,976 bytes; `xsht` is 40,737,768 bytes. These sizes are for those products,
not newly built master binaries.

Probes ran in the pinned Linux image with one CPU, 512 MiB memory, no extra swap,
no network, `LC_ALL=C`, and `TZ=UTC`. Each timing used seven fresh processes.
Filesystem caches were warm. Container startup was excluded. Parent wall time
includes the `/usr/bin/time` wrapper and output collection; child CPU includes
that wrapper. Exact output, empty stderr, and status zero were asserted.
`perf-gate-ready` and its immediate `-1` check passed for both batches.

| Workload | Median wall ms [range] | Median child CPU ms [range] | Median process peak bytes |
|---|---:|---:|---:|
| Existing `cold_trivial.xsh`, `hello` | 1.283 [1.073, 1.696] | 1.207 [1.035, 1.595] | 19,099,648 |
| Existing `cold_quote.xsh`, `'two words'` | 2.879 [2.713, 2.903] | 2.737 [2.643, 2.827] | 27,119,616 |
| Existing `cold_cli_parse.xsh`, `3` | 2.926 [2.734, 3.162] | 2.867 [2.692, 3.082] | 28,098,560 |
| `print template.render("hello {{.name}}", {name: "xsh"})?` | 16.242 [15.832, 19.579] | 15.841 [15.605, 19.405] | 32,595,968 |

Separate `xsht runtime-stats` probes reported these allocation counts and bytes:

| Workload | Construction allocations | Construction allocated bytes | Execution allocations | Execution allocated bytes |
|---|---:|---:|---:|---:|
| Trivial | 1,675 | 280,860 | 46 | 4,376 |
| Embedded template | 123,388 | 25,157,105 | 1,014 | 236,768 |

These are diagnostic observations. They identify construction as a candidate for
profiling, not a measured speedup or a bound on time saved. Allocation traffic is
not RSS. Phase-separated CPU attribution and substantial-workload throughput are
unmeasured. Establish them at the accepted combined source before optimization.

The stream probe failed before execution: `bench/stream-stage-cost.xsh:24` calls
removed `abort(2)` and receives `check.removed-abort`. The same stale form appears
in the terminal and producer memory probes. This is a known-red benchmark input,
not a streaming measurement. Repair its exit spelling in P0, preserving behavior.

The resource-controller loop passed 53 tests in 0.14 s wall, 0.12 s user, and
0.02 s system, with 25,404 KiB peak RSS. Readiness passed on both sides. Two
existing exact native selectors passed in the pinned container: template field
behavior (5 ms test-reported) and rooted traversal rejection (2 ms test-reported).
Those reported test durations are loop evidence, not performance acceptance.

Planning host: 32 logical CPUs, 64,899,153,920 bytes RAM, about 59.5 GB available
at survey time. Swap had about 1.17 GB occupied; do not infer ongoing pressure
from occupancy alone. Measure swap activity and cgroup events before setting width.
The sandbox hid `/dev/kvm`; authorized `doas` inspection found the character
device and successfully opened it, reporting KVM API version 12. No guest ran.
No ready Laputa guest bundle was established.

### Verified execution foundation

The pinned image exists locally:
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
Planning proved that the available source-qualified products can execute the
embedded-module path and the existing rooted-confinement test in this image.
It also proved that actual KVM access works through the granted elevation.

Artifact independence from build-machine state, inference merge correctness,
substantial-workload scaling, and real guest boot remain unproven. Their tests
are prerequisites below. No full build, suite, proof recheck, fetch, or VM launch
was performed during planning.

## Sequence

### Resource and ownership limits

Ordinary iteration allows at most two compiler builds, each at two jobs and
8 GiB. Inherited release-profile builds keep their 12 jobs, 12 CPUs, and 24 GiB
and run alone. A full native observer keeps its existing 12-CPU, 24-GiB allocation.
Each inherited scheduler pair keeps 32 CPUs and 40 GiB and runs alone. Do not
reduce those allocations to obtain incomparable timing. Park the affected gate
when its allocation cannot fit. Never run two full native suites together.
Lightweight independent test lanes get two CPUs and 4 GiB. A VM lane gets four
CPUs and 8 GiB host allowance, including guest memory.

Reserve 12 GiB for the host and coordinator. Admit another lane only when the
sum of enforced memory limits fits current available memory minus that reserve.
Reduce width on observed swap activity, cgroup memory events, or disk pressure.
Measure lane peaks after the first tranche and revise the admission calculation.
Timing windows exclude other builds, suites, VMs, and profiling.

| Phase | Maximum lanes, excluding coordinator | Floor while independent work is ready |
|---|---:|---|
| P0 foundation | 1 until G0 passes, then 4 | One per admitted ownership set |
| P1 completeness | 6 | One per admitted ownership set |
| P2 hardening | 5 | One per admitted ownership set |
| P3 optimization | 5 | One per admitted ownership set; pause measured competitors during timing |
| P4 close | 2 review/preparation lanes, then one measurement owner | All feature lanes stopped before final freeze |

Replace a stopped lane in the same turn when it has ready work. Overlap integration
and disjoint next slices. Run verification from a frozen scratch checkout, not the
moving integration tree. Split independent focused inputs; preserve the one-full-
suite lock. Width limits are ceilings, never a requirement to saturate timing runs.

### P0: Prove the loop and finish the baseline

| Item | Size and prerequisites | Exclusive ownership | Outcome and proof |
|---|---|---|---|
| G0: Qualify existing inputs and exact checks | S; execution grant | Coordinator; `bench/stream-*.xsh` only for demonstrated stale inputs, focused controller selectors | Reproduce the stale `abort` rejection, repair it, assert exact count output, and run the two native foundation selectors. No other implementation starts until these checks work. |
| G1: Close the inherited acceptance queue | L; G0; final qualification requires G2's functional validator | Acceptance lane; timeout fixture/owner and existing consolidation gate controllers; no inference code | Reproduce and fix XSH-2 at its origin. Run the finite current-source queue below, including util-linux resources/close. Zero unaccounted failures or skips; finish XSH-1 to XSH-3 only after their own proofs pass. |
| G2: Add narrow measurement adapters | M; G0 | Tooling lane; `dev/performance/`, existing benchmark selectors, `src/frontend_stats.rs`, `src/runner.rs` phase-observation seam | Extend existing reports with parse/load, check/solve, publish, lower, verify, execute, and first-output boundaries. Separate inclusive from exclusive time. No observer in production hot paths when disabled. Validate exact output and reject wrong products, unsupported workloads, and failed readiness. |
| G3: Audit isolated inference and freeze the practical cohort | M; G0 | Read-only implementation audit plus `dev/performance/` corpus subdirectory and inference evidence docs; tooling owner assigns disjoint files first | Produce a minimal reviewed capability dependency list, source diff, original/reduced cohort, and retained-site classes. No whole-history merge. Pin the actual implementation source behind the accepted evidence. |
| G4: Prove disposable Laputa inputs | M; G0 | Laputa foundation lane; isolated checkout's local test fixture adapter and owned output roots | Run synthetic PM controls, verify exact candidate-XSH injection, prove network-disabled execution and output confinement, inspect existing profile input closure and KVM path. No guest or recipe is credited by a host-only fixture. |

At G1 completion, publish `docs/PERFORMANCE-BASELINE.md` with the accepted source,
product locations, platform, limitations, and gate commands. Update that same page
after inference integration and final close. Keep the accepted code source explicit
when the documentation commit differs. Do not wait until campaign end to provide
the everyday baseline reference.

G1, G2, G3, and G4 can overlap after G0. G1 owns broad acceptance; other lanes
stop expensive work for its timed windows. The coordinator resolves any shared
controller or source-identity seam before assigning writers.

G2 creates `dev/performance/check.py` as a thin dispatcher over current tools.
Its commands listed below are **planned**, not existing interfaces. Add only
missing selection, attribution, and result checks. Reuse existing fixture makers,
paired sampling, stats, trace, locks, and process cleanup. Do not duplicate their
implementations. Adapter failures remain nonzero and retain their primary cause.

Validate the proof foundation before opening dependent work: a wrong-output
control, a mismatched product/source control, a real cleanup failure control, and
a before/after-readiness rejection must each fail the exact relevant command.
The new `self-test` owns those controls. Unit policy cases must avoid real clocks
and processes; bounded real adapters prove the OS boundary.

#### Run the finite inherited queue

Freeze a clean integrated source and use the existing gate tools. Adapt paths,
source pins, and product qualification in their existing owners only where the
merged source proves a mismatch. Never manufacture `functional_complete: true`
or copy source credit from an earlier receipt.

1. Revalidate current functional/structural obligations with the existing canonical
   owner and supplemental correctness launcher. Run the deep grammar proof once.
   G2's `functional --check` validates the results and issues the existing
   functional-close record for this exact source before any profile consumes it.
2. Run a fresh normal profile with `profile-cost.py`.
3. Run one timed canonical/resource observer with `final-observers.py --execute`.
4. Run the forced profile, final comparative profile, and three scheduler pairs.
5. Run util-linux `resources --check` and `close --check` through its release scope.
6. Check final metrics and document only required known limitations.

The source-qualified inherited receipt must satisfy its existing budgets,
including the isolated 15-second repository lint gate. Readiness is mandatory
for time credit; binary size and memory still run when timing is deferred.
Keep the existing exact `lsipc` and rooted-memory exceptions unchanged.

The inherited tools use absolute workspace paths and local `.work` prerequisites.
G1 must validate their `--plan` output and required files before an expensive run.
Fix relocation only when needed by a demonstrated failure. Do not replace the
controllers or add a second source/product receipt protocol.

`functional --check` is a missing qualification step that G2 must supply. It
consumes the actual canonical receipt, supplemental context/skip review,
structural report, and required semantic review rows for the clean source.
Use the current `metrics.xsh check-current` owner for those rows. Reject a bare
boolean, absent result, stale product, or unreviewed mandatory skip. Then write
the existing functional-close shape with its evidence bindings. Supplementary
`launch.py` success alone does not issue a close. For canonical bootstrap, adapt
the existing Cargo-JSON producer to `final_canonical_replay.py`'s exact
`validate_bootstrap_receipt` contract. Add no parallel build or receipt schema.

### P1: Integrate inference and complete workflow behavior

| Item | Size and prerequisites | Exclusive ownership | Outcome and proof |
|---|---|---|---|
| I1: Integrate the accepted inference capabilities | L; G1 and G3 | One compiler lane; `src/sema/`, corresponding syntax metadata, compact facts, lowerer, indexed schema/verifier, compiler-facing tests | Review and port/cherry-pick the minimal dependency set. Preserve master Lean, cleanup, and util-linux changes. Run ground-type, schemes, rows, host anchors, recursion, rollback, importer-order, diagnostics, frontend-drop, and forged-IR tests. Then run the combined regression boundary once. |
| I2: Remove missing inference relationships by class | L; I1; corpus preparation from G3 can precede it | Same compiler lane for implementation; a separate corpus lane owns transformed fixtures and accounting only | Classify 100% of retained eligible sites. Investigate all 202 effect clauses by cause. Fix shared propagation/constraint causes; keep intentional restrictions and all semantic Map annotations. Prove joint original/reduced behavior and useful inferred signatures. |
| D1: Complete immutable package lifecycle tests | L; G4; use accepted baseline compiler first, then I1 | Laputa PM lane; `tests/pm/`, narrow PM implementation owners, local recipe fixtures; never XSH compiler files | Local source validation, unpack/build/proof, root generation, version replacement, recomposition/removal, GC, and injected failure at each publication boundary. Check exact files, modes, links, ownership, receipts, and unchanged old generation on failure. |
| D2: Complete process and rooted-filesystem workflows | M; G4 | Boundary lane; `tests/runtime/process.rs`, `tests/xsh/within.xsh`, `tests/xsh/stdlib/fs*.xsh`, narrow host owners; no scheme/IR files | Bound process trees, partial output, signal/timeout outcomes, complete reaping, primary plus cleanup diagnostics, raw filenames, symlink swaps, hardlinks, permission errors, and interrupted publication. No escape from the declared root. |
| M1: Freeze representative measurements | M; G2 and G1; final checker baseline after I1 | Measurement lane; existing benchmark files and `dev/performance/` selection; no runtime optimization | Qualify all four families, exact outputs, common/demanding shapes, n/2n/4n scaling, completed-work and allocation/syscall counts. Establish the post-inference source/product baseline. |
| F1: Bind one transformation to applicable formal assumptions | M; G2; final subject after profiles | Formal lane; `formal/` maintenance/evidence and one optimization-specific finite validation fixture; compiler edits belong to I1/P3 owner | Select one measured transformation. Map actual checked input and emitted IR assumptions to the relevant formal result; mutation controls reject a lost assumption. Keep theorem statements separate from finite Rust evidence. |

All scheme, effect, compact publication, and IR changes stay with one compiler
owner. Do not run competing solver and effect-representation writers. Corpus,
Laputa, host boundaries, measurements, and formal work provide the remaining
disjoint lanes. If D2 needs an IR fix, return it to I1 instead of widening ownership.

I1's combined boundary includes the inference relationship suite, unchanged
annotated corpus, full util-linux parity, existing process/rooted tests, and one
full native suite. Run the existing changed-path Rust checks for shared compiler
changes. Do not infer correctness from corpus checking without execution.

For I2, use at least `core/wc.xsh`, `core/sort.xsh`, `core/awk.xsh`, and
`pm/generation.xsh` from the isolated Laputa checkout. Retain the original frozen
48-source corpus and its eligibility categories. New util-linux files are an
additional cohort, never an excuse to change the old denominator. A declaration's
inferred interface must be independent of importer order and unrelated callers.

The practical inference gate requires every selected program to check and run
with eligible implementation annotations removed jointly wherever supported.
Compare outputs, failures, cleanup, evaluation order, inferred signatures, and
negative diagnostics. Record each retained site with its class and a minimal
witness. Every missing relationship found in the selected practical cohort must
be repaired or be a visible incomplete item. Purposeful restrictions and
ambiguous literals are accepted retained sites. Do not rewrite algorithms to
improve the score. Reproduce at least the pinned 1,040-removal result on its
original corpus before assigning new credit at the merged source.

### P2: Harden the contracts before performance changes

| Item | Size and prerequisites | Exclusive ownership | Outcome and proof |
|---|---|---|---|
| H1: Qualify prepared-module behavior and format boundaries | M; I1 scheme/IR contract settled, G2 | Preparation lane; artifact tests, private loader/verification contract design; coordinate compiler seams with I1 | Source path is the oracle. Freeze module/linker corpus and failure controls before the experiment. Run all baseline cases. Artifact-specific rejection cases become the experiment's mandatory tests. |
| H2: Harden deployment and input acquisition | M; D1 and D2 | Same Laputa and boundary owners, disjoint slices | Corrupt/truncated inputs, duplicate concurrent publication, interruption between writes, partial output, cleanup failure, and idempotent rerun leave only valid generations. Fetch pinned inputs separately; test/build runs have network disabled. |
| H3: Maintain formal trust and documentation | M; F1, I1 | Formal lane plus coordinator-owned generated docs | Run existing Lake build, axiom self-tests, guarded paranoid check, inventory checks, and applicable finite controls. Update SPEC sources for inference behavior, registry docs for actual API changes, and deployment reproduction docs. |

H1 must cover module identity, imports, private bindings, nominal types, schemes,
labels/defaults/rest arguments, exact diagnostic spans, malformed table bounds,
unknown schema/version/features, resource exhaustion, and function evidence.
Run each behavior through source preparation and candidate loading on the same
inputs. Corrupt an instruction, substitution, and identity on purpose; rejection
must occur before a script side effect.

Prepare on one cwd/environment/filesystem and execute on another. Assert that
module initializers run once at ordinary runtime order, never during artifact
generation. Keep source locations and source-map ownership after parser, arena,
checker, solver, and compact frontend facts are actually dropped in a Rust test.
A subprocess-only test is insufficient. Raw external input never constructs
`FullProgram` or bypasses `FullVerifier` directly.

Freeze decoder limits before O1: at most 64 MiB encoded data, one million entries
in any table, and nesting no deeper than the current semantic limit of 256.
Check lengths, aggregate sizes, ranges, and arithmetic before allocation.
The tighter product-size and memory gates still apply. Exact-release data has
one canonical writer, an explicit format version, and no legacy readers.

### P3: Optimize the measured shared work

No P3 item starts until the affected workload is complete and passes its P1/P2
behavior corpus. Performance snapshots may precede complete inference, but artifact
schema work waits for I1 and H1. Other runtime mechanisms depend only on their
own stable checked interface and the accepted measurement source.

| Item | Size and prerequisites | Exclusive ownership | Outcome and proof |
|---|---|---|---|
| O1: Experiment with prepared shipped modules | L; H1, H3, M1 | Preparation lane; `src/stdlib.rs`, narrow loader/runner seams, private artifact implementation, indexed verification changes; no competing compiler writer | Build exact-release immutable data, link and verify it, then use the existing indexed executor. Evaluate one catalog module first, then broaden only after it passes. Ship or reject using the thresholds below. |
| O2: Remove stream materialization and copying | L; D2, M1, affected parity | Streaming lane; stream/stage implementation and byte/text library owners | Preserve pull timing, short-circuiting, errors, chunk boundaries, backpressure, cancellation, and cleanup. Prove bounded memory for unbounded input and compare useful bytes/records per CPU second. |
| O3: Improve measured script algorithms | L; I2 practical corpus, M1 | Algorithm lane; selected `core/lib/` algorithm and applet sources only, one coherent algorithm family at a time | Improve measured comparisons/copies/spill behavior. Preserve exact GNU/uutils/BusyBox behavior only on the tested overlapping corpus and keep unsupported differences explicit. No applet-named Rust acceleration. |
| O4: Remove redundant host and frame work | L; I1, D2, M1 | Host lane; selected syscall adapters and runtime value/frame owners; excludes O1 verifier/loader and O2 stages | Remove repeated metadata or argument/value work identified by profiles. Preserve host errors, stat/symlink policy, evaluation order, frame ownership, and cleanup. A new primitive must be reusable and bounded. |
| F2: Prove the selected transformation's actual boundary | M; F1 and selected O-item | Formal lane, with runtime probes supplied by its owning lane | Validate actual checked/IR assumptions, run the transformation, compare observable outcomes, and run a mutation negative. State exactly which property is theorem-backed and which is finite differential evidence. |

Profile before each experiment and after each retained win. Rank candidates by
the removable fraction of end-to-end work. Keep up to three supported experiments
per mechanism. Reject unmeasurable experiments and delete their unused paths.
Do not chase dispatch while avoidable I/O, materialization, or algorithms dominate.

#### Set numeric performance gates

M1 freezes primary workloads and common/demanding inputs before O1 to O4.
Use at least five alternating baseline/candidate pairs, with AB/BA reversal.
Retain existing larger sample counts where the reused runner already owns them.
Report median, range, minimum CPU, and paired spread per workload, never one
aggregate XSH speed ratio. Count failures, bytes/records, subprocesses, syscalls,
allocation traffic, process RSS, and cgroup peaks separately.

Treat a change as a regression only when it exceeds the permitted budget and
observed paired spread. A benefit must exceed spread as well as its numeric target.
Do not treat overlapping ranges as a reliable win without more independent pairs.

| Target | Pass condition |
|---|---|
| Prepared-module experiment | At least 20% lower median end-to-end startup on two shipped-module workloads, including template, with all H1 cases passing. Otherwise reject and remove the experiment. |
| Useful shared runtime improvements | At least 15% lower median CPU on two preselected substantial workloads from different non-startup families, each completing at least 50 ms of baseline CPU work. Mechanism counts explain the change. |
| Untouched startup controls | Retain the existing per-case increase limit `max(5% of baseline, 1 ms)` and require no new failure or helper process. |
| Substantial workload regressions | Candidate median CPU no more than baseline plus `max(5% of baseline, paired spread)` on every frozen unaffected workload. Preserve any stricter existing absolute budget. |
| Production binary sizes | Each release product at most 5% larger than the post-inference baseline. Count embedded artifact bytes in the product. |
| RSS and finalized retained memory | Each measured independently; at most baseline plus `max(10% of baseline, 2 MiB)` on unchanged source. Retain any stricter inherited gate. |
| Build cost | At most 10% additional median CPU for matched incremental and release builds, outside measured spread; identical flags, concurrency, cache state, and absolute staging path. |
| Solver scaling | Existing inference bounds and negative cases pass at n/2n/4n. Explain genuine output growth separately; no elevated limits, silent recovery, or new `Any` to meet time budgets. |
| Observation overhead | Disabled observers allocate nothing per operation. Measured production controls stay within their existing budgets. Profiling runs never supply timing samples. |

Define cold as a fresh process without XSH session/cache reuse. State filesystem
page-cache warmth separately. Do not flush host caches. Measure first useful
output separately from complete exit; both include load, link, verification,
and required initialization. Keep output comparison outside timing but consume
the actual output inside each measured process. Report helpers as part of the
workflow users pay for.

Run `perf-gate-ready` immediately before every timed measurement and
`perf-gate-ready -1` immediately after, including failed measurements. Discard
overlapped runs. When the host stays busy, report `perf gate deferred: host busy`.
Memory and size still run. Deferred timing never passes, and blocks full release.

### P4: Qualify deployment and close

| Item | Size and prerequisites | Exclusive ownership | Outcome and proof |
|---|---|---|---|
| R1: Boot and shut down the candidate userspace | L; D1/D2/H2, selected profile input closure, exact candidate compiler | Laputa VM lane; existing `system/qemu.xsh`, guest proof, profile sources, bounded fixtures | Run `make profile-test` in the isolated checkout with the candidate XSH substrate bound into its immutable package/root identity. Require actual guest marker, screenshot, intended userspace, bounded clean shutdown, zero surviving VM descendants, and valid unchanged old bundle on failure. |
| R2: Freeze and review the release candidate | M; all feature lanes stopped, R1 | Coordinator and independent reviewer | Run combined changed-area gates, one full native suite, applicable inherited gates, workload parity, the final serial performance window, and formal maintenance. Resolve P1/P2 review findings, or leave the campaign incomplete. |
| R3: Retire campaign machinery and publish local acceptance record | S; R2 | Coordinator | Remove dead experiment paths, duplicate temporary adapters, and integrated clean worktrees. Update only completed TODO tickets. Write one accepted-baseline record and deployment reproduction commands. No release upload, tag, or push. |

R1 reuses Laputa's existing 180-second guest proof deadline and shutdown
supervisor. Set a 15-minute bound on one supplied-source fixture package workflow.
Run a selected profile preparation/build stage at most 45 minutes per attempt,
then inspect the concrete limiting package and resume only its bounded stage.
Do not repeat a whole closure after a known package failure. A missing pinned
input, unusable KVM, or unsupported host parks R1 and prevents release credit.
No mock, screenshot-only host fixture, or software-acceleration substitution
can pass the real boot requirement.

## Acceptance commands

G2 supplies these **new** commands. Until its tested implementation exists,
their rows are requirements, not evidence. The dispatcher reads the explicit
source/product and Laputa checkout paths from arguments; no hidden normal-checkout
fallback is allowed. Keep one small configuration for those paths and selected
workloads, with validation before effects. Pass that configuration as `--config`.

| Deliverable | Command after G2 | Threshold |
|---|---|---|
| Proof foundation | `python3 -B dev/performance/check.py --config PATH self-test` | All real negative controls reject; exact positive fixtures pass; zero leaked processes. |
| Functional qualification | `python3 -B dev/performance/check.py --config PATH functional --check` | Actual required correctness/review results qualify the exact source before emitting the existing close record. |
| Accepted baseline | `python3 -B dev/performance/check.py --config PATH baseline --check` | Existing finite inherited gates pass for the named source and products, including util-linux resources and close. |
| Integrated inference | `python3 -B dev/performance/check.py --config PATH inference --check` | Full selected relationship and joint corpus matrix passes; all retained eligible sites classified; old denominators preserved. |
| Four measured families | `python3 -B dev/performance/check.py --config PATH measure --check` | Every selected input has exact output and completion checks, raw samples, CPU, RSS, and attribution; readiness accepted. |
| Prepared experiment and shared wins | `python3 -B dev/performance/check.py --config PATH optimize --check` | O1 ships only if its keep gate passes; shared workload wins and all non-regression budgets pass. |
| Deployment | `python3 -B dev/performance/check.py --config PATH deployment --check` | Four complete workflow families pass, including real guest boot and shutdown; no mandatory skip or leaked resource. |
| Release candidate | `python3 -B dev/performance/check.py --config PATH close --check` | Same-source applicable proofs pass. Reports distinguish experiment rejection, target misses, deferred time, and blocked work from success. |

Do not embed the inherited full suite in `measure`, `optimize`, or each focused
iteration. Run it only for G1's acceptance source, I1's combined integration, and
R2's final source. An existing mandatory close command can own that suite at the
same milestone. Deduplicate invocation, not its acceptance obligations. Widen
after a material uncovered integration risk and state the reason first.

Existing narrow commands remain the development loop:

```sh
python3 -B -m unittest discover -s dev/util-linux -p test_resources.py
cargo dev gate src/sema/types.rs
cargo dev gate PATH...
xsht test --exact tests/xsh/stdlib/template.xsh::test_template_fields_and_dot
xsht test --exact tests/xsh/stdlib/fs.xsh::test_fs_root_operations_reject_traversal
xsht runtime-stats --json .work/tmp/runtime.json SCRIPT
xsht trace --syscalls --trace-top-syscalls 10 SCRIPT
```

Use absolute source-qualified verification/release `xsh`, `xsht`, and helper
paths inside the pinned container. The spellings above abbreviate those paths.
Rust builds use kache; Linux musl links with mold through existing target settings.
`cargo dev gate` does not replace private root-lib scheme/verifier tests. Select
those tests by exact name in a reusable debug harness, then run them directly.

The inherited profile adapter invokes these existing interfaces with real values:

```sh
python3 -B dev/consolidation/winddown/gate-tools/final-correctness-gates/launch.py --plan
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/final-observers.py --plan
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/profile-cost.py --tree SOURCE --commit COMMIT --functional-close CLOSE_JSON --output OUTPUT
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/final-observers.py --execute --tree SOURCE --commit COMMIT --functional-close CLOSE_JSON --profile-receipt PROFILE_JSON --release-opt-level 3 --output OUTPUT
python3 -B dev/util-linux/check.py resources --check
python3 -B dev/util-linux/check.py close --check
```

Reuse `forced-frames.py`, `final-comparative.py`, and
`native-scheduler-comparison/compare.py` for their rows. The adapter validates
their current argument parsers, receipts, mandatory selector counts, accounted
skips, and unchanged source before invoking them. Do not write replacement suites.
The old ARM64-only `bench/stdlib-port/run.py --linux` route must not be used as
the x86-64 gate. Reuse its workloads through the current pinned x86-64 runner.

Within the isolated Laputa tree, bind `XSHT` and its sibling `xsh` to candidate
products and use existing exact selectors. Start with these current controls:

```sh
"$XSHT" test --exact tests/pm/pm_execute.xsh::test_execute_builds_dependency_levels_in_isolated_roots_and_reuses
"$XSHT" test --exact tests/pm/pm_execute.xsh::test_execute_proof_failure_and_corrupt_final_never_publish_replacement
"$XSHT" test --exact tests/pm/pm_root.xsh::test_root_failed_composition_leaves_completed_output_untouched
"$XSHT" test --exact tests/pm/pm_store.xsh::test_store_gc_keeps_named_artifacts_and_removes_the_rest
"$XSHT" test --exact tests/system/qemu.xsh::test_qemu_supervisor_retries_qmp_injects_once_and_escalates_shutdown
make profile-test
```

Those controls do not supply the complete new lifecycle by themselves. D1, D2,
and R1 extend existing tests with the missing observable transitions. Prove the
tested and booted compiler is the candidate, not an old seed or `PATH` binary.

For R1, pass the clean candidate checkout as `XSH_ROOT`, and pass absolute
candidate host-tool paths as `XSH_BIN_DIR`, `XSH`, `XSHT`, and `XSH_HOST` to
the isolated Laputa Makefile. Run `make seed ARCH=x86_64 CARGO_JOBS=2` with
those bindings after the pinned input acquisition stage. This uses Laputa's
existing offline seed build and its own recorded release profile. Qualify that
profile separately if its flags differ from the benchmark products.
Check `.out/seed/x86_64/manifest.json`: `xsh_commit` must equal the candidate,
`xsh_dirty` must be false, features/target must match, and every listed product
and `core.tar.xz` must match its actual bytes. Feed that seed through the existing
`packages/xsh/PKGBUILD.xsh` repository input into the root generation.
Check the completed generation's `/usr/bin/xsh`, `xshi`, `xsht`, and installed
core content against the seed before boot. Bind that generation to the existing
image/guest proof. Candidate host tests alone cannot satisfy this condition.
Do not add executor version or timing data to immutable package keys; seed
payload identity already belongs to the package's repository input.

Formal maintenance follows `formal/README.md`: use `.work` for Lake workspace,
build and cache outputs. From `formal/`, run `scripts/audit-axioms.sh`,
`scripts/audit-axioms.sh --self-test`, and `python3 scripts/check-fresh.py`.
Keep their existing 8-GiB, 180-second build and 1,080-second check bounds.
Maintain inventory structure checks separately. No fragment count closes a
universal obligation or Rust-correspondence claim.

## Execution boundary

The owner explicitly launched this campaign on 2026-10-17. That grant covers
isolated XSH and Laputa edits, local commits and integration, pinned fetches,
owned Docker/VM operations, and bounded `doas` use within this document.
The deployment-scope and KVM decisions above further bound the grant.
Keep test and package writes inside disposable owned roots.
Do not edit the sibling normal checkout, host packages, live services, devices
other than required virtual-machine access, boot configuration, or real mounts.

Public API and language semantic changes beyond the already accepted inference
scope are forbidden. Preserve diagnostics, effects, module visibility, path bytes,
process exit semantics, and publication boundaries. Report every actual contract
change. Internal representation changes require their own negative validation
tests. A dependency cannot bypass the measured build, size, memory, or startup
gates. Do not update pinned Lean or Rust tools as part of this campaign.

## Non-goals

- New syntax batches, general ownership syntax, exact-record syntax, or expanding the utility catalogue.
- A native backend, JIT, daemon, mandatory warmup, or user compilation ritual.
- Command-specific Rust implementations behind XSH command names.
- Persistent arbitrary-script caches or a public artifact format.
- Another kernel, OS port, package-catalog campaign, production deployment, release upload, or CI changes.
- Closing all 72 universal formal obligations or calling the Rust compiler verified from fragment theorems.

## Reference map

| Source | Disposition | Role |
|---|---|---|
| `docs/ARCHITECTURE.md:1`, `docs/ARCHITECTURE.md:60` | Reuse | One checked/indexed execution path and XSH policy/Rust mechanism boundary. |
| `src/stdlib.rs:51`, `src/stdlib.rs:183` | Change after profiling | Six embedded module sources and required-module selection. |
| `src/runner.rs:205`, `src/runner.rs:377` | Extend narrowly | Construction allocations and ordinary preparation boundary. |
| `src/frontend_stats.rs:37` | Extend narrowly | Existing stage memory/count report, not phase CPU timers. |
| `src/runtime/eval/indexed/full.rs:854`, `src/runtime/eval/indexed/full.rs:12874` | Reuse/change with one owner | Owned executable representation and semantic verifier. |
| `dev/consolidation/designs/inference.md:173`, `dev/consolidation/designs/inference.md:251` | Reuse accepted contracts | Immutable scheme authority, call substitutions, frontend disposal, verifier obligations. |
| `dev/consolidation/HANDOFF.md:71`, `dev/consolidation/CAMPAIGN.md:1494` | Reproduce | Pinned corpus result and unchanged historical accounting targets. |
| `dev/consolidation/winddown/gate-tools/` | Reuse | Finite inherited correctness, resource, forced, comparative, and scheduler gates. |
| `dev/util-linux/resources.py:1560`, `docs/CAMPAIGN-UTIL-LINUX.md:79` | Reuse | Same-source resource measurement and exact approved parity exceptions. |
| `bench/stdlib-port/run.py:25`, `bench/consolidation/README.md:1` | Reuse fixtures/sampling | Startup and complete glue workloads; ARM-only routes remain separate. |
| `docs/TESTING.md:494`, `crates/xsht/src/stats.rs:29` | Reuse | Benchmark policy, allocation reports, and syscall diagnostics. |
| `tests/xsh/stdlib/fs.xsh:309`, `tests/runtime/process.rs:303` | Extend | Actual confinement and process cleanup boundaries. |
| `../laputa/docs/PM.md:81`, `../laputa/tests/pm/pm_execute.xsh:294` | Reuse/extend in isolated tree | Immutable build, proof, and generation operations. |
| `../laputa/docs/QEMU.md:24`, `../laputa/tests/system/qemu.xsh:199` | Reuse/extend | Hardware-backed guest proof and supervised shutdown. |
| `formal/README.md:578`, `formal/README.md:595` | Maintain | Trust-check bounds and explicit absence of a general source/IR replay corpus. |
| `dev/performance/check.py` | New in G2 | Thin selection/qualification adapter for the existing mechanisms. |
| `docs/PERFORMANCE-BASELINE.md`, isolated Laputa deployment reproduction doc | New at G1, updated at close | Concise accepted source/products/platform/gates/limitations for everyday use. |

The old campaigns mixed long historical receipts with active requirements.
Another source-specific full failure then sent the executor back through many
checkpoints. Keep their archives, but make the new baseline page answer whether
the current revision is usable without requiring that history. Never preserve
duplicate per-commit copies of controllers solely to record provenance.

## Decision log

- 2026-10-17: Plan against local merged master; keep historical acceptance and current verification separate.
- 2026-10-17: Owner selected isolated Laputa edits plus pinned fetches, and permitted `doas` for KVM access.
- 2026-10-17: KVM API probe succeeded through `doas`; sandbox visibility is not a platform limitation.
- 2026-10-17: Keep 95%/85% historical targets visible without changing `{}` semantics or hiding retained restrictions.
- 2026-10-17: Prepared modules receive a keep/reject experiment; useful shared runtime gains remain a separate deliverable.
- 2026-10-17: Launch source is `c6d36683872e4d0dc9173a9c2c8ff09e8e6b9d23`, the owner's documentation-only plan commit over `3ed58895c`. No product-source change requires a new planning baseline.
- 2026-10-17: G2 may extend explicit `check-current --review-evidence` to call the existing review verifier. Preserve default checks and strict `final-close` behavior. G1 owns the canonical bootstrap producer seam in existing controllers.

## Campaign log

- Planning: source survey, seven-sample startup diagnostics, allocation probes, 53 resource-controller tests, two native selectors, and KVM API probe complete. No execution phase started.
- G0: Reproduced removed-abort failures in all three stream probes. Replaced the stale exit spelling and diagnosed redundant pipeline parentheses. Five exact-output cases, three invalid-mode cases, and both native foundation selectors pass in the pinned container with qualified `7e8881d96` tools. No current-source build or performance credit is claimed.
- P0 lanes from `b1ebd7fd8`: `.work/wt/perf-acceptance` on `campaign/performance-acceptance-20261017`; `.work/wt/perf-measurement` on `campaign/performance-measurement-20261017`; `.work/wt/perf-inference-audit` on `campaign/performance-inference-audit-20261017`. Root master remains the integration checkout.
- G4 checkout: `.work/wt/laputa-foundation` on Laputa branch `campaign/xsh-deployment-foundation-20261017`, at `bbe05e4`, the owner's documentation update after the planning survey. Its source remains isolated from the normal sibling checkout.

## For the owner afterwards

- Read the concise accepted baseline page and deployment reproduction commands.
- Review source-level behavior/API changes, inference classes and retained sites, individual workload results, and the artifact experiment decision.
- Review the exact finite/formal distinction for the selected transformation.
- Inspect local XSH and Laputa integration branches. Merge only reviewed accepted commits under the launch grant. Nothing is pushed by this campaign.
