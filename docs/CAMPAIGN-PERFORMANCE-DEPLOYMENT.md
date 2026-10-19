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
| Allow one build of the existing Laputa host-tools image. | Owner answer, 2026-10-18. Its pinned Alpine base installs seven unversioned APK packages. Build only this exact image, save it in the isolated checkout, and use that saved image offline afterwards. This does not authorize other moving inputs. |
| Retain the thirteen exact `lsipc` reference-crash exceptions. | Existing owner decision in the util-linux campaign. Independent XSH assertions remain behavior evidence and receive no exact-parity credit. |
| Start Laputa world and Asterinas acceptance in parallel with consolidation. | Latest owner direction, 2026-10-18, supersedes waiting for cleanup before starting G6. Preserve exclusive G1 timing windows. Laputa must remain functional. Use an isolated checkout of the sibling's named source, the frozen XSH candidate, the existing world recipe set and existing Asterinas backend. Final G6 acceptance requires that same XSH source to qualify under G1. |
| Start inference integration and performance work alongside G1 and G6. | Owner direction, 2026-10-18. Stagger compiler integration, corpus work, measurement preparation and deployment. Preserve exclusive timing windows, real close prerequisites and matching source/product qualification. |

### Prescribed decisions

| Decision | Reason and reversal condition |
|---|---|
| Start at local master `3ed58895c0b1423c8f7ac4852bb9ec9a23922da2`. | It includes the merged util-linux work and preserves the accepted Lean tree. The supplied review's `7e8881d9` predates that merge. Reseal once if master advances before launch. |
| Keep x86-64 Linux musl and the existing pinned test image for XSH qualification. | Reuse established product settings and isolation. G6 additionally qualifies Laputa's existing pinned Asterinas backend under the owner's explicit direction. No replacement of the Linux baseline or new backend. |
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
| Keep all test/build execution offline after explicit input acquisition. | Fetch only recipe/image-pinned inputs for the selected closure. G6 includes the existing full x86-64 world plan and Asterinas inputs. No broad `make fetch`, new package catalogue entries, or moving dependency versions. |
| Preserve source-facing APIs and the single indexed executor. | New private types and scheme metadata may change. A new public contract, native backend, daemon, or compile-first workflow is outside this plan. |
| Start with no new production dependency. | Use existing codecs and tools. A concrete missing operation can justify a dependency only after license, maintenance, transitive-weight, build, size, and startup review. |
| Keep compiler qualification separate from a receipt-reader repair. | The existing adapter configuration binds compiler source, products and receipts independently of its tool checkout. Freeze and name both revisions. A reader-only correction gives no compiler credit to the newer tool revision or master. |

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
8 GiB. The isolated Laputa Deno link may use 12 GiB with two CPUs and no extra
swap: its 8-GiB container OOM was reproduced, and the 12-GiB node build passed. Inherited release-profile builds keep their 12 jobs, 12 CPUs, and 24 GiB
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
| G5: Retire consolidation infrastructure | M; G1 | Tooling owner; `dev/consolidation/`, `dev/performance/`, `dev/util-linux/`, testing and baseline docs | Complete XSH-19. Trace current callers, retain reusable checks under ordinary developer commands, and remove obsolete campaign adapters, duplicate entry points and historical-pin dependencies. Preserve required acceptance behavior and prove replacement commands reject invalid inputs before deleting their predecessors. |
| G6: Qualify Laputa world and Asterinas | L; G4 to start, G1 for final source qualification | Laputa acceptance lane; isolated sibling-source checkout, existing world and Asterinas owners, pinned fork for demonstrated kernel defects | Start in parallel with consolidation. Run the full existing x86-64 world build with the frozen XSH candidate, prove root functionality and unchanged rebuild reuse, then require the actual Asterinas smoke and full guest acceptance. Resolve failures at their owning boundary. No mock, filtered package subset, stale compiler pin or host-only proof supplies acceptance. |

At G1 completion, publish `docs/PERFORMANCE-BASELINE.md` with the accepted source,
product locations, platform, limitations, and gate commands. Update that same page
after inference integration and final close. Keep the accepted code source explicit
when the documentation commit differs. Do not wait until campaign end to provide
the everyday baseline reference.

Run G5 immediately after G1. Start G6, I1 and M1 in parallel with consolidation.
Stagger builds and diagnostics within the resource limits. Accepted M1 results
still require their real close prerequisites and the post-inference source.
During exclusive timing windows, other lanes may prepare source and review
results without running builds, suites, VMs or profiling. Schedule G6
builds and VM execution outside those exclusive windows.
This is infrastructure cleanup, separate from deleting disposable worktrees and
build products. Keep checks required by the remaining campaign until ordinary
commands own them. Archive historical evidence outside the everyday command path.
Use focused owner tests and rejection controls for the cleanup. The next combined
regression boundary remains I1's responsibility. R3 removes machinery introduced
later in this campaign.

At G6 launch, name a clean Laputa source from `../laputa` and test it in an
isolated checkout. Keep the normal checkout and its shared artifact store intact.
Bind the frozen XSH source and products into the host tools, seed and package
build. Asterinas currently uses its own XSH commit pin. Update that isolated pin
and its existing source-cache selection to the same frozen local revision.
An older locked XSH build cannot qualify the current candidate.
If G1 requires a different source, refresh the affected G6 products and results.

Use `make plan` and `make build ARCH=x86_64` without `PKGS` or `STOP` to cover the
existing world. Fetch only the frozen plan's pinned missing inputs, then execute
offline. Run the existing Laputa checks and suites, root inspection and execution
proof, and an unchanged rebuild. Keep publication local to an owned loopback
mirror when the existing root workflow requires it. No external publication.

Run `make asterinas-setup` for its real KVM smoke marker, then
`make asterinas-test` for the PM-built root, xinit, XSH, filesystem and process
proofs. Require the existing guest markers, successful result and complete owned
VM cleanup. Smoke alone does not establish full Laputa acceptance. Keep the
existing pinned fork and input policy. Required syscall defects belong in that
fork with kernel regressions, never userspace workarounds or skips. Bound build
attempts by the concrete stage and resume that stage after a diagnosed failure.
Rerun applicable G6 checks after I1 and later shared-path changes. A broken world
or Asterinas acceptance blocks progression and final campaign closure.

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
2. Run a fresh normal profile with `profile-cost.py`. With its qualified release
   products, run the existing `dev/perf/btop-profile.xsh` scaling gate in an
   isolated readiness-bracketed window; the ordinary native suite retains its
   clock-free join correctness checks.
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
`launch.py` success alone does not issue a close. Run the canonical owner's
bootstrap stage before `execute`:

```sh
python3 -B dev/consolidation/final_canonical_replay.py bootstrap --repo SOURCE --head COMMIT --tree TREE --output .work/tmp/bootstrap-RUN --run-name bootstrap-RUN
```

`execute --bootstrap-receipt` consumes
`.work/tmp/bootstrap-RUN/bootstrap-receipt.json`. The stage records the exact
singleton Cargo build and writes the existing `validate_bootstrap_receipt`
schema.

### P1: Integrate inference and complete workflow behavior

| Item | Size and prerequisites | Exclusive ownership | Outcome and proof |
|---|---|---|---|
| I1: Integrate the accepted inference capabilities | L; G3 to start; matching G1/G6 qualification before close | One compiler lane; `src/sema/`, corresponding syntax metadata, compact facts, lowerer, indexed schema/verifier, compiler-facing tests | Review and port/cherry-pick the minimal dependency set. Preserve master Lean, cleanup, and util-linux changes. Run ground-type, schemes, rows, host anchors, recursion, rollback, importer-order, diagnostics, frontend-drop, and forged-IR tests. Then run the combined regression boundary once. |
| I2: Remove missing inference relationships by class | L; I1; corpus preparation from G3 can precede it | Same compiler lane for implementation; a separate corpus lane owns transformed fixtures and accounting only | Classify 100% of retained eligible sites. Investigate all 202 effect clauses by cause. Fix shared propagation/constraint causes; keep intentional restrictions and all semantic Map annotations. Prove joint original/reduced behavior and useful inferred signatures. |
| D1: Complete immutable package lifecycle tests | L; G4; use accepted baseline compiler first, then I1 | Laputa PM lane; `tests/pm/`, narrow PM implementation owners, local recipe fixtures; never XSH compiler files | Local source validation, unpack/build/proof, root generation, version replacement, recomposition/removal, GC, and injected failure at each publication boundary. Check exact files, modes, links, ownership, receipts, and unchanged old generation on failure. |
| D2: Complete process and rooted-filesystem workflows | M; G4 | Boundary lane; `tests/runtime/process.rs`, `tests/xsh/within.xsh`, `tests/xsh/stdlib/fs*.xsh`, narrow host owners; no scheme/IR files | Bound process trees, partial output, signal/timeout outcomes, complete reaping, primary plus cleanup diagnostics, raw filenames, symlink swaps, hardlinks, permission errors, and interrupted publication. No escape from the declared root. |
| M1: Freeze representative measurements | M; G2 to start; real close prerequisites and post-I1 products for accepted baseline | Measurement lane; existing benchmark files and `dev/performance/` selection; no runtime optimization | Qualify all four families, exact outputs, common/demanding shapes, n/2n/4n scaling, completed-work and allocation/syscall counts. Establish the post-inference source/product baseline. |
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
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/profile-cost.py --tree SOURCE --commit COMMIT --functional-close CLOSE_JSON --laputa-corpus LAPUTA --output OUTPUT
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/final-observers.py --execute --tree SOURCE --commit COMMIT --functional-close CLOSE_JSON --profile-receipt PROFILE_JSON --release-opt-level 3 --output OUTPUT
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/forced-frames.py --tree SOURCE --commit COMMIT --functional-close CLOSE_JSON --laputa-corpus LAPUTA --profile-receipt PROFILE_JSON --output OUTPUT
python3 -B dev/util-linux/check.py resources --check --laputa-corpus LAPUTA
python3 -B dev/util-linux/check.py close --check --laputa-corpus LAPUTA
```

Reuse `forced-frames.py`, `final-comparative.py`, and
`native-scheduler-comparison/compare.py` for their rows. The adapter validates
their current argument parsers, receipts, mandatory selector counts, accounted
skips, and unchanged source before invoking them. Do not write replacement suites.
The old ARM64-only `bench/stdlib-port/run.py --linux` route must not be used as
the x86-64 gate. Reuse its workloads through the current pinned x86-64 runner.

The scheduler comparator keeps networking disabled and supplies a local
`xsh-test` hostname mapping. Its read-only mounts expose the source at `/work`
and its original absolute path, the shared Git directory, and
`/usr/local/bin/mold`. The existing `runner_artifacts` map binds mold's bytes;
each stage checks that executable before and after execution. The launch owner
must match it to the passed profile's mold binding before starting comparison.

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
| [Architecture](ARCHITECTURE.md#the-libxsh-facade) | Reuse | One checked/indexed execution path and XSH policy/Rust mechanism boundary. |
| `src/stdlib.rs:51`, `src/stdlib.rs:183` | Change after profiling | Six embedded module sources and required-module selection. |
| `src/runner.rs:205`, `src/runner.rs:377` | Extend narrowly | Construction allocations and ordinary preparation boundary. |
| `src/frontend_stats.rs:37` | Extend narrowly | Existing stage memory/count report, not phase CPU timers. |
| `src/runtime/eval/indexed/full.rs:854`, `src/runtime/eval/indexed/full.rs:12874` | Reuse/change with one owner | Owned executable representation and semantic verifier. |
| `dev/consolidation/designs/inference.md:173`, `dev/consolidation/designs/inference.md:251` | Reuse accepted contracts | Immutable scheme authority, call substitutions, frontend disposal, verifier obligations. |
| `dev/consolidation/HANDOFF.md:71`, `dev/consolidation/CAMPAIGN.md:1494` | Reproduce | Pinned corpus result and unchanged historical accounting targets. |
| `dev/consolidation/winddown/gate-tools/` | Reuse | Finite inherited correctness, resource, forced, comparative, and scheduler gates. |
| `dev/util-linux/resources.py:1560`, `docs/CAMPAIGN-UTIL-LINUX.md:79` | Reuse | Same-source resource measurement and exact approved parity exceptions. |
| `bench/stdlib-port/run.py:25`, `bench/consolidation/README.md:1` | Reuse fixtures/sampling | Startup and complete glue workloads; ARM-only routes remain separate. |
| [Benchmarking](TESTING.md#benchmarking), `crates/xsht/src/stats.rs:29` | Reuse | Benchmark policy, allocation reports, and syscall diagnostics. |
| `tests/xsh/stdlib/fs.xsh:309`, `tests/runtime/process.rs:303` | Extend | Actual confinement and process cleanup boundaries. |
| `../laputa/docs/PM.md:81`, `../laputa/tests/pm/pm_execute.xsh:294` | Reuse/extend in isolated tree | Immutable build, proof, and generation operations. |
| `../laputa/docs/QEMU.md:24`, `../laputa/tests/system/qemu.xsh:199` | Reuse/extend | Hardware-backed guest proof and supervised shutdown. |
| [Formal scope](../formal/README.md), [trust checks](../formal/README.md#build-and-check) | Maintain | Trust-check bounds and explicit absence of a general source/IR replay corpus. |
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
- G2 foundation integrated at `4c13a8b7f`: twelve adapter policy controls and the exact native functional-versus-final review control pass. Functional qualification omits only the timing review row; final qualification still requires every row. No functional-close record has been issued.
- G1 timeout repair integrated at `85e39ab0b`, with the retained deterministic proof at `b51357241`. The old shell fixture can fork after SIGINT forwarding and defer its trap beyond cancellation grace; the repaired fixture blocks in a builtin read. Both deterministic controls and the exact INT/TERM native selector pass with the matched candidate tools. Runtime cancellation behavior is unchanged; full acceptance remains pending. The review checkout is `.work/wt/perf-timeout-review` on `campaign/performance-timeout-review-20261017`.
- G4 tool preparation uses read-only detached XSH checkout `.work/wt/perf-products` at `429f6ff080efac12e5706ab45ec05539a24e57e6`. The bounded offline build produced matched `xsh`, `xshi`, and `xsht` under the isolated Laputa checkout's `.out/xsh-target/x86_64-unknown-linux-musl/release`. Both package execution/publication selectors and explicit executor-digest provenance pass. These seed-feature products are not the canonical singleton bootstrap receipt or a boot qualification.
- 2026-10-18: Phase diagnostics integrated at `d12c360a4`; four stats tests, the runner's three-scenario observation test, and actual success/type-error/parse-error JSON probes pass. The coordinator reran the five Rust tests with the source-identical emitted harness and frozen integration checkout. First useful output and performance qualification remain separate work.
- 2026-10-18: Inference audit and corpus adapter integrated through `d53440e8a`; all five retained adapter controls pass. The audit reconstructs the historical 1,040 view without crediting the later 877-view recipe or broader behavior witnesses. Compiler capability integration has not started.
- 2026-10-18: `.work/wt/perf-baseline` freezes clean `d53440e8a`. Its first canonical bootstrap failed before Cargo because Docker could not create `/work/target` under the read-only source bind. The failed attempt remains under that checkout's `.work/tmp/canonical-bootstrap-d53440e8-20261018`; cleanup resolved. Repair the existing controller's private target mount before a fresh freeze and attempt; no build or acceptance credit attaches to this failure.
- 2026-10-18: Isolated Laputa integration is `.work/wt/laputa-integration`, branch `campaign/performance-deployment-20261018`, at `639184e`. Its exact executor-provenance selector passes with the matched tools. Profile planning stops at the absent `.out/seed/x86_64`; the transitive input closure and real boot remain unqualified. The normal sibling checkout is untouched.
- G1 follow-up: `.work/wt/perf-acceptance-mount` on `campaign/performance-acceptance-bootstrap-mount-20261018` owns the mount and host-boundary repairs. Clean freeze `.work/wt/perf-baseline-r2` at `69091f57c` passed the actual singleton bootstrap. Its canonical run then stopped at nine host-boundary tests writing under read-only source. Repair `760877c45` uses the configured temp directory; the retained red and all nine focused greens pass. Neither failed full attempt supplies acceptance credit.
- G2 completion: the original Rust allocation result and JSON are preserved at `c28e0170d`; a separate tooling entrypoint supplies phase JSON. Five focused stats tests and three actual CLI probes pass. The integrated hostpipe sampler passes eighteen tests, and all fifteen frozen workload cases pass with immutable seed products and null timing fields. An earlier tranche using shared products remains unqualified because those files changed during the run.
- Inference preparation: `.work/wt/perf-inference-audit` now uses `campaign/performance-inference-i2-prep-20261018`; `.work/wt/perf-inference-port` uses `campaign/performance-inference-port-20261018`. The reviewed source delta applies textually without conflict and includes the accepted host, Set, Optional, unsigned, and constant-authentication capabilities. Implementation was waiting for G1 acceptance at this checkpoint. The source audit classifies 911 retained sites and leaves 932 pending; protected contracts make the historical percentage targets unattainable.
- Formal preparation: `.work/wt/perf-formal` on `campaign/performance-formal-20261018` corrected the provisional inventory at `c1adbbd36`. Inventory, 51 inventory tests, three isolation controls, the Lake build, axiom audit and negatives, and all six kernel checks pass. Lean sources are unchanged; the 91-owner inventory retains 72 planned obligations, null source identity, and no conformance cases. These are maintenance results, not Rust verification or F1 completion.
- G4 inputs: the existing publisher produced a validated seed from frozen `429f6ff0`. Offline planning resolves 47 packages; all 37 pinned archives are fetched and SHA-verified, totaling 368,424,255 bytes. Two PM selectors pass against the read-only product snapshot `.work/tmp/perf-products-429-seed-r1`. The required Laputa host-tools image and saved tar are absent; its Dockerfile installs unpinned APK packages, outside the pinned-fetch boundary. Package builds and real boot remain blocked on that input, and no guest qualification is claimed.
- G1 follow-up: clean freeze `.work/wt/perf-baseline-r3` at `b31bebc73` passed its fresh singleton bootstrap. Canonical execution stopped at the documentation code scanner: 751 library tests passed, one failed, three were ignored; the preceding integration group passed 58 with three ignored. The scanner mistook the documented basename of `dev/performance/corpus/check.sh` for a diagnostic code. Repair `42771860e` resolves actual referenced source paths while retaining unknown-code checks. The minimal red and all six diagnostic-catalog greens pass; the failed full receipt remains preserved and supplies no acceptance credit.
- G2 baseline adapter integrated at `922b2591c`; nineteen policy tests pass. Optional explicit receipt paths compose the existing functional, profile, observer, forced-frame, comparative, and scheduler owners. Util-linux close still requires its actual one-use independent review and its distinct private-namespace suite; standalone review delivery remains incomplete. No completed receipt chain or current-source baseline is claimed.
- D2 focused workflows integrated at `a2c375153` and `3725c5bff`. The default process-stream path buffered despite its live-stream contract; the existing producer now serves default text and bytes while retaining zero-only completion and explicit acceptance policies. The coordinator replayed six native stream/filesystem selectors as UID 1000 and the exact Rust process-tree cancellation/cleanup test with byte-checked products; all pass. Two existing direct-pipeline selectors also pass. Rooted checks cover raw names, hardlinks, symlink confinement, failed replacement, and permission denial; deterministic mid-write interruption remains separate H2 work.
- G2 CPU diagnostics integrated at `eca7a41c6`; all eight focused controls pass in the coordinator's replay, and three CLI probes preserve output and error behavior. The additive tooling report measures process CPU across its threads, excluding descendants and waiting. The old allocation report remains unchanged; these observations carry no performance credit. The retained-effect review at `a8586ffa4` leaves all 202 clauses in place and identifies the missing facts for 60 non-findings.
- G1 invocation repairs integrated at `c2138ba25` and `4b8a1ec22`. The existing owners accept exact owned frozen checkouts while retaining common archive pins and locks; profile and forced-frame commands now require an explicit clean Laputa checkout. The adapter at `1942b831b` validates those bindings, and twenty policy tests pass. Focused owner tests and an actual main/frozen checkout, isolated corpus, scratch, pinned-helper, and target-path preflight pass. No full gate has run since the stream repair. Isolated Laputa integration `263a650` passes the composed replacement/publication/removal/GC fixture with historical seed tools; candidate qualification remains pending.
- G4 input update: the exact digest-pinned Alpine base is now fetched. The absent host-tools image still requires seven unversioned APK inputs; none were resolved or fetched, and no package build or VM run has started.
- G1 follow-up: frozen `.work/wt/perf-baseline-r5` at `a857527a7` passed its fresh singleton bootstrap after an earlier bootstrap was interrupted for unrelated host memory pressure. The canonical attempt has recorded a corpus layout-invariance failure; preserve its receipt and isolate the offending input before another full run. No current-source acceptance is claimed.
- Independent close review found selector reconstruction, callback working-directory, private-runner deadline, candidate diagnostic, and process-stream deadline defects. Controller repairs integrated at `71c34e1ab` and `8d885aa72`; the coordinator replayed 57 resource tests, 21 adapter tests, and two cancellation/cleanup controls successfully. The rooted-memory selector fails with the old matcher and passes with the tightened matcher using snapshotted products rebuilt from the frozen source. The thirteen `lsipc` crash exceptions remain unchanged. The process-stream deadline repair and the separate layout failure remain open.
- The existing structural scanner completed bounded small- and large-file diagnostic probes with matching source/product hashes and resolved container cleanup. A full-source measurement exceeded its initial 120-second bound without a report; this supplies no acceptance or performance credit. The functional adapter now selects the historical 900-second full-scan budget explicitly, while its focused self-test retains 60 seconds. The required full structural report remains pending.
- Isolated Laputa integration advanced to `cf61f4b`, adding concurrent publication retries with both workers acknowledging readiness before release. The coordinator replayed the composed replacement/publication/removal/GC selector successfully with the immutable historical seed. Candidate qualification and real boot remain pending. The normal sibling checkout is untouched.
- Follow-up checkouts are `.work/wt/perf-util-close` on `campaign/performance-util-close-20261018` for the integrated controller repairs and `.work/wt/perf-layout` on `campaign/performance-layout-20261018` for the failed layout investigation. The boundary and measurement owners retain their existing isolated checkouts. Keep these checkouts and failed-run evidence until integration and acceptance resolve.
- The `a857527a7` canonical run finished unsuccessfully with cleanup resolved. Its layout harness rejected existing lint errors when perturbation added redundant-parentheses errors. Harness repair `630e064ba` preserves all diagnostics and rejects unrelated checker and parse errors. The minimal fixture failed before the repair; the coordinator replayed all five focused controls successfully with unchanged harness and tool hashes. A fresh full run remains required.
- Supplemental and util-linux gates now require an explicit clean Laputa corpus. Supplemental receipts retain the executed Docker arguments, and the reader checks the selected read-only mount and environment. Local policy checks pass; no supplemental acceptance receipt has been issued.
- The authorized host-tools image is saved under `.work/wt/laputa-foundation/.cache/images/laputa-host-tools-x86_64-faf126e1d76d709e.tar`. Reload preserved image ID `sha256:0f1d14f60e28ec442566326db92872916b2ba61fc437ac07fa0e32c9a4fde371`; the coordinator checked the tar digest and loaded image independently. Reuse the saved image offline. Package-closure execution and real boot remain pending.
- Offline preparation subsequently built the package-tools image and all 47 planned artifacts for the eight profile roots plus Linux. The coordinator checked every selected artifact key, package ID, and built origin against `.work/wt/laputa-foundation/.out/world/x86_64/plan.json`. The build used the historical `429f6ff` seed and the 37 validated cached URL inputs. Its container had networking disabled, two CPUs, and 8 GiB memory; cleanup resolved and both source checkouts remained clean. Keep the isolated artifact cache. Current-candidate, profile, boot, and timing qualification remain pending.
- Deadline repair `d674b26a1` adds an owned timeout watcher and shared child-status authority. Paused-consumer and cancellation regressions fail on pre-watcher products and pass on the candidate; the worker-failure regression also has an executable red and green. Independent source review found no remaining P1/P2 findings. Two focused Rust controls and fifteen native selectors pass; the coordinator retained a replay of all fifteen after integration under `.work/tmp/runtime-integration-r2`, with unchanged product hashes and resolved cleanup. Timed streams now report `HostIo` before spawning when child status is discarded; restoring status collection does not revive an invalidated identity. SPEC source and generated output record this contract. Full acceptance remains pending.
- Deployment readiness found that the historical seed's `xshi` is disabled and the old headless profile proof does not detect that. Isolated Laputa repair `0ce56a3` adds an exact-output shell command to the graphical proof and routes capture failures through the existing guest failure protocol. The disabled historical shell fails the probe as expected; source checking and the existing overlay control pass. Enabled-candidate and guest execution remain pending. Keep it on the foundation branch while G1 binds the unchanged `cf61f4b` integration corpus. The existing `make seed-asterinas-shell` producer enables `xshi/laputa-asterinas` for the Linux shell payload; this selects no replacement kernel.
- G1 freezes clean `c77bb05b3` in `.work/wt/perf-baseline-r6`. Its fresh singleton bootstrap passed. The canonical run then failed at XSHT integration: 119 passed, two failed, five filtered; the earlier library group passed 757 with three ignored. Layout perturbation lost default-parameter and redundant-close diagnostics in three file/code groups. The written FIFO polling test missed a hangup while its desugared version passed. The failed receipt and logs remain under that freeze's `.work/tmp/canonical-replay-c77bb05b-r6`; cleanup resolved and shared locks are free. No later gate ran and no baseline is accepted. Keep the frozen source and Laputa corpus unchanged for diagnosis.
- Independent inference review identified nested-schema ownership ranges and unsigned literal validation without optional facts as cases to reproduce before I1 acceptance. The pending controls and constructor-origin coverage limit are recorded in `docs/PERFORMANCE-INFERENCE.md`. No inference implementation or executable qualification is claimed.
- Follow-up writers are `.work/wt/perf-layout-r6` on `campaign/performance-layout-r6-20261018` and `.work/wt/perf-unix-poll` on `campaign/performance-unix-poll-20261018`. Each must establish a minimal causal failure before repair; no unchanged full-run retry is authorized by a focused pass. Genuine r6 products are byte-checked copies under `.work/tmp/perf-products-c77-r6`; the older integration target is not that product set.
- M1 wiring is staged in `.work/wt/perf-measurement-m1` on `campaign/performance-measurement-m1-20261018`. Independent source review passed after fixing diagnostic-file collisions. The additive tooling contract accepts explicit fixture/output paths, returns and validates the existing in-process close report, and distinguishes owned-leader CPU/RSS, batch memory, first-line latency, and instrumented-process syscall diagnostics. Focused execution remains required; no measurement or performance acceptance is claimed.
- Actual mid-write interruption fixtures are staged separately in `.work/wt/perf-boundaries-publication` on `campaign/performance-boundaries-publication-20261018` and isolated Laputa `.work/wt/laputa-publication` on `campaign/performance-publication-interruption-20261018`. They use child-only file-size limits and SIGXFSZ to check old-state preservation, confined partial writes, and Laputa retry cleanup. Neither fixture has executed; the G1-bound Laputa `cf61f4b` remains unchanged.
- M1 wiring integrated at `247bbaa4b` after independent source review and 29 selected passing controls. Six integration controls also pass across `.work/tmp/m1-integration-r1` and `r2`; the first attempt's missing writable sampler-fixture mount is retained. This establishes adapter behavior, not an executed workload baseline. The typed close report remains a trusted in-process owner result, not a serialized acceptance token.
- Rooted interruption coverage integrated at `ac49f64eb`; FIFO polling isolation and an inherited-writer barrier control integrated at `c23fecba3`. The original FIFO test sampled a shared process descriptor table without waiting; isolation removes interference from parallel test forks. The barrier proves the inherited-writer topology, not a particular historical fork timing. The coordinator replayed the new filesystem case and both FIFO cases successfully with unchanged r6 products; logs are under `.work/tmp/native-integration-c23-r1`.
- Lint repair `6f1906b1b` permits only redundant-parentheses diagnostics through the existing semantic proof gates. All diagnostics remain visible and other checker errors still veto fixes. Independent source review and three focused controls pass, including autofix/recheck; the coordinator replayed both proof-safety controls. On the three actual affected c23 files, all 86 original diagnostics remain under grouped operands, with only four redundant-parentheses errors added. An explicit formatter probe was rejected by automatic approval review under the repository's no-formatter rule; the completed file comparison used lint only.
- Bootstrap retention repairs `4a03de9d7` and `1a73cac8d` address an actual r6 reread failure: later Cargo stages replaced the executable named by its bootstrap receipt. The existing schema now records exact retained bytes beside the receipt; preflight and prelaunch still require the mutable target to match. Independent review and all 36 preflight controls pass in the pinned container. Copy failures retain both the primary error and independent close failures. The failed r6 receipt remains untouched; this repair supplies no build or gate credit.
- Isolated Laputa `e516312` adds actual mid-copy SIGXFSZ interruption and retry to the lifecycle fixture. Final-source checking and the exact lifecycle selector pass with matched c77 r6 products, preserving old index/generation, checking the 4096-byte partial file, and verifying retry content and cleanup. This is focused fixture evidence, not an accepted compiler or guest result. Keep this branch and graphical-proof `0ce56a3` separate from the unchanged G1-bound `cf61f4b` until its receipt chain closes.
- Clean freeze `.work/wt/perf-baseline-r7` at `598af3a52` passed fresh bootstrap and the full layout-invariance corpus check. Retained bootstrap validation also passed after Cargo replaced the mutable executable. The desugared-corpus check then stalled in `dev/tests/test-system-report-check.xsh`; a bounded futex trace and executable symbol mapping identify the global symbol interner as the shared wait address. JSON record-key sorting nests two reads of that lock, contrary to the callback's contract. The coordinator cancelled the run through its owner after preserving diagnostics. Its receipt is failed-or-incomplete with SIGINT, not an assertion failure or owner timeout; cleanup resolved and shared locks are free. No G1 acceptance credit follows. `.work/wt/perf-json-symbol-lock` on `campaign/performance-json-symbol-lock-20261018` owns a focused reproducer and repair; no unchanged full-run retry is planned.
- JSON repair `cc03ec1ea` replaces that nested callback with the existing lexical `Name` comparison. A bounded subprocess reproducer stalled on the old comparator and was killed and reaped; all four JSON tests pass after the one-line repair. Independent review passed, and the coordinator replayed all four against the source-identical integrated checkout with unchanged harness bytes and resolved cleanup. Logs are under `.work/tmp/json-integration-cc-r1`. This establishes the exercised concurrency defect and exact output preservation; the schedule-sensitive reproducer does not identify the canonical process's precise blocked callsite. A fresh canonical run remains required.
- Freeze `.work/wt/perf-baseline-r8` at `100e2551e` passed fresh bootstrap, 758 library tests, 122 XSHT integration tests, 14 soundness tests and ten Linux privilege tests. The full native suite selected 14,152 tests and emitted twenty failures before coordinator cancellation. The receipt is failed-or-incomplete with SIGINT, not a timeout; cleanup resolved and shared locks are free. Focused probes found a runnable fixture discovered as a test, missing private-context admission, privileged device/owner operations under an unprivileged UID, direct kernel reads in `lsipc`, and stale active metric-observer pins. The btop scaling failure's assertion detail was lost; one subsequent isolated readiness-qualified diagnostic passed, leaving the original cause inconclusive. No current baseline or performance credit follows.
- Repairs integrated at `7a57b181f`, `607dc1360`, `b7232833e` and `3c717fe96` retain the timing threshold in the existing release profile, move the observer fixture into the existing excluded data hierarchy, distinguish ordinary unsupported contexts from strict advertised private contexts, and route IPC observations through a typed XSH domain. Raw limit text and command policy remain unchanged. The current metric observer reproduced the exact frozen start-source report and item inventory before only its two active pins changed; historical evidence is unchanged. Independent review passed after correcting a missing Linux guard and an unwanted limit-validation narrowing. The coordinator replayed nineteen integrated tests, including seven genuine private positives with zero skips, under `.work/tmp/native-integration-3c-r1`; source bytes matched the reviewed lanes, product hashes remained unchanged, and cleanup resolved. A fresh canonical run and the readiness-qualified release btop gate remain mandatory. The four repair branches and their `.work/wt/perf-{btop-scaling,native-context,ipc-domain,observer-provenance}` checkouts retain the failed and focused evidence.

- Freeze `.work/wt/perf-baseline-r9` at `2a1cd7176` passed fresh bootstrap, 758 library tests, 122 XSHT integration tests, both deep grammar checks, 14 soundness tests and ten Linux privilege tests. Its native suite emitted one failure in `test_process_stream_timeout_preserves_unobservable_child_identity` before controlled cancellation. The retained receipt is failed-or-incomplete with SIGINT, not a timeout; final cleanup resolved. No assertion body was emitted, so the historical failure's exact cause remains unconfirmed. Repair `f0687f324` adds a child-ready barrier after its successful initial write, preserving identity invalidation, survival assertions and the five-second timeout. A forced late-write schedule reproduces a 5.3-second failure, and a PIPE-trap control confirms Broken pipe before the survival marker. Independent source review passed. The coordinator replayed both negative controls and the repaired selector plus three neighbors against the frozen integrated checkout; all expected results held, product hashes were unchanged and cleanup resolved. Separate logs are under `.work/tmp/stream-integration-f068-r1`. Keep the repair checkout `.work/wt/perf-stream-identity-r9`, frozen integration checkout `.work/wt/perf-stream-integration`, and failed r9 evidence. A fresh canonical run remains required; no supplemental gate or current-source acceptance follows from these focused results.

- Freeze `.work/wt/perf-baseline-r10` at `dfee5f276` passed fresh bootstrap, 758 library tests, 122 XSHT integration tests, 14 soundness tests and ten Linux privilege tests. The repaired identity selector passed in the full native run. That run later failed `test_api_surface_matches_the_recorded_reference`; controlled cancellation retained a failed-or-incomplete SIGINT receipt with resolved cleanup and unchanged products. The snapshot omitted the three Linux IPC observation functions and record already introduced by `c772562b9`. Fixture-only repair `c48f7db47` adds exactly those entries and their counts; all other summary content is unchanged. The coordinator replayed the old-fixture negative control, all 67 API-tool tests, and the byte-exact live summary against `.work/wt/perf-api-integration`. The existing documentation check passed, including three tour-project tests, after a retained setup failure targeting a nonexistent read-only mountpoint. Repository check, lint, diff and the full current structural ratchet also passed with unchanged products and resolved cleanup. Evidence is under `.work/tmp/api-integration-c48-r1`, `.work/tmp/api-docs-tail-c48-r2` and `.work/tmp/canonical-tail-c48-r1`. The structural report retains zero lowering resolvers and 46 generated stages; source-line growth remains advisory under the existing policy, not a source-reduction result. Keep the failed r10 freeze and `.work/wt/perf-api-surface-r10` repair branch. The next canonical run must finish its already-running native suite after any failure marker so the existing runner can emit complete assertions and counts; its deadline and no-credit-on-failure rules remain unchanged. No full G1 acceptance follows from these diagnostics.

- Freeze `.work/wt/perf-baseline-r11` at `89408f371` passed the canonical gate: 758 library tests, 122 XSHT integration tests, both deep grammar checks, 14 soundness tests, ten Linux privilege tests, and 14,008 native tests. Native failures were zero, with 150 skips reviewed separately. Repository, documentation and structural checks passed. The retained bootstrap remained valid after Cargo replaced its mutable executable. Cleanup resolved, products stayed unchanged, and the structural report matched the preceding focused run. This untimed run supplies no performance credit. The supplemental run built its products and three harnesses, then stopped at seven warning-denied Clippy diagnostics. Its five Linux contexts did not run. Keep both receipts and products unchanged. The Unix-socket selector that skipped for excessive scratch-path length subsequently passed with scratch mounted at `/t`, using the same read-only source and products. Its exact result was one pass and zero skips, with resolved cleanup, under `.work/tmp/socket-r11-r1`. G1 remains open. Repair the Clippy diagnostics, then run supplemental correctness before another fresh canonical qualification.

- Warning repair `44de8d71c` preserves portable IPC widening and simplifies six equivalent expressions. Independent source review found no P1 or P2 issue. The scoped IPC Clippy exemption records why the conversion remains necessary. Freeze `.work/wt/perf-baseline-r12` passed the full supplemental gate, including warning-denied Clippy and all five Linux contexts. Those contexts passed 23, 58, two, two, and 18 tests, respectively. Three ignored date-clock tests require a disposable guest. Two traceroute skips require raw-network capabilities absent from their unprivileged context. Filtered tests receive no coverage credit. The short-path Unix-socket selector also passed with zero skips using this source and its supplemental products, under `.work/tmp/socket-r12-r1`. Source and Laputa stayed clean, products stayed unchanged, and cleanup resolved. The fresh canonical qualification is pending. No functional close, performance acceptance, inference integration, or guest boot follows from supplemental success.

- The r12 canonical gate passed at `44de8d71c`: 14,008 native passes, zero failures and 150 reviewed skips. Both earlier failing selectors passed. Library, XSHT, deep grammar, soundness, privilege, repository, structural and documentation checks passed. Cleanup resolved and retained-bootstrap validation passed after the mutable target changed. The structural values retain zero lowering resolvers and 46 stages. Its production-source count is one line lower; shifted evidence positions make the report different from r11. This is not a source-reduction result. Final-product socket coverage passed with zero skips. The existing shared-import trace recorded eight requests, one preparation, one check and seven reuses for one shared module owner. Evidence is under `.work/tmp/socket-r12-canonical-r2`, `.work/tmp/cache-r12-r1` and the frozen r12 checkout. Functional validation stopped before effects because its reader rejected the producer's extended environment metadata and copy-only harness staging step. The isolated reader repair is under review in `.work/wt/perf-functional-reader-r12`. Preserve the passed receipts and qualified compiler source. Run the corrected reader from a separately named frozen tool checkout; do not transfer canonical credit to that checkout. Functional publication and the remaining G1 queue are still pending.

- The corrected reader is integrated through `2444bef65` and frozen separately in `.work/wt/perf-validation-r2`. It accepts the producer's extended environment and exact copy-only stage, rejects staging roots overlapping the Laputa corpus, and runs the structural container as the caller. Fourteen policy tests and the caller-identity regression pass. The first full scan failed Git's ownership check as root; a bounded read-only Docker contrast reproduced status 128 as root and exact-source success as the caller. The corrected full functional check passed and published `.work/final-review-evidence/functional-close-44de8d71c04f155034a7c1e8cb8e7edddd9da15f.json` under the unchanged r12 freeze. Its canonical and supplemental receipts, structural report, semantic review and skip dispositions are bound by the existing validator. Both failed tool attempts remain preserved. The normal opt3 profile is next; G1, inference integration, timing acceptance and guest boot remain incomplete.

- The r12 normal opt3 build passed, but its completed native suite failed: 13,946 passed, one failed and 211 skipped. `tests/xsh/stdlib/env.xsh::test_env_functions_and_path_list` reported `spawn: Text file busy` while executing its freshly written helper. The failed receipt remains under the freeze's `.work/consolidation/final-performance-gates/profile-cost-44de8d71c04f155034a7c1e8cb8e7edddd9da15f`; both readiness checks passed and container cleanup resolved. The 61-skip increase is accounted: the caller-UID context adds 64 namespace/capability skips, while the short scratch path and group setup allow three canonical skips to run. Both runs enumerate 14,158 tests. No failed profile acceptance or speed claim follows. `.work/wt/perf-env-exec-r12` owns bounded reproduction; preserve the qualified source, products and failed receipt unchanged.

- Fixture repair `0dfebb001` creates the executable in a completed child and retains the original scoped-parent environment and PATH execution assertions. The inherited-writer control demonstrates `ETXTBSY`; it does not establish the historical fork schedule. Independent review found no P1/P2 issue. The coordinator's integrated env-file check passed all 20 tests with unchanged r12 diagnostic products and resolved cleanup, under `.work/wt/perf-validation-r2/.work/tmp/env-integration-r13`. Freeze `.work/wt/perf-baseline-r13` at this commit requires fresh qualification; r12 receipts do not qualify it. Its supplemental correctness run precedes fresh bootstrap and canonical execution. Inference remains unintegrated.

- R13 passed fresh supplemental correctness, singleton bootstrap and canonical execution. Canonical native results are 14,008 passed, zero failed and 150 skipped; the repaired env selector passed. Compiler/runtime, corpus-invariance, soundness, privilege, repository, structural and documentation checks passed. All 150 native skip selectors/reasons and 11 Rust ignore instances match the reviewed r12 dispositions, apart from one run-specific capability fixture path. Cleanup resolved, shared locks are free, source stayed clean and retained-bootstrap validation passed after mutable-target replacement. These are current-source correctness results, with no timing credit. On the owner's acceleration request, the remaining baseline queue has four workstream owners: serialized performance producers, functional-close assembly and controls, skip/context review, and final private/resource close. Functional publication, the normal profile, observers, forced/comparative/scheduler runs and actual util-linux close remain required before G1 closes.

- R13 functional qualification passed and published `.work/final-review-evidence/functional-close-0dfebb0019e4bfc5bb577e26d7bca2b8832634f7.json` under the frozen checkout. The validator checked the current canonical and supplemental results, structural report, nine semantic review rows and six skip contexts. The reviewer rechecked all 39 evidence bindings and 11 source identities. Current-product socket and shared-import controls passed. The first attempt completed validation but could not publish because the output directory was absent. That setup failure remains recorded. The unchanged-input rerun passed after directory creation, with resolved cleanup, free locks, unchanged products and clean source. The serialized performance lane now owns the remaining profile, btop, observer, forced-frame, comparative and scheduler runs without intermediate approval pauses. Actual private/resource close follows those runs. G1 remains open.

- R13's normal opt3 profile passed with 13,947 native passes, zero failures and 211 reviewed skips. Both build and suite readiness brackets passed. Cleanup resolved and products stayed unchanged. The skip set matches the preceding profile context. Compared with canonical, 64 namespace/capability selectors skip, while two group-dependent selectors and the short-path socket selector pass. The separate btop profile passed with both readiness checks successful. The observer then failed during isolated repository lint while traversing a generated `.work/` long-path fixture. Its failed receipt and stderr remain under `.work/final-review-evidence/final-performance-gates/observers-0dfebb001-r13` in the freeze. Cleanup resolved, but the observer batch did not run. The repair lane is investigating the controller's relocated source mount and inaccessible linked-worktree Git metadata. Forced, comparative, scheduler and private/resource acceptance remain pending. Preserve the qualified compiler source and its passed results.

- Observer repair `82b3ca7b4` preserves the source's absolute mount path and exposes the shared Git metadata read-only. The old mount hides linked-worktree exclusions. A minimal actual Docker control reproduces `ENAMETOOLONG` with the old command and lints exactly one visible file with the fix. Existing observer and temporary-directory tests pass, and independent review found no P1/P2 issue. Validation tool `.work/wt/perf-lint-scratch-r13` is frozen separately at `9b65865e542f1b00b161d8b93a0aa7a5991253bd`. Compiler source, products and qualification remain R13 at `0dfebb001`. The retained historical artifact snapshot and its pins stay unchanged. A fresh timed observer attempt uses `observers-0dfebb001-r13-r2`; the failed first receipt remains a failure. Focused controls supply no full observer acceptance.

- The corrected R13 observer attempt `observers-0dfebb001-r13-r2` passed with clean source and unchanged selected release products. Isolated repository lint and the canonical resource batch completed successfully, with accepted readiness brackets and resolved cleanup. Native results are 14,008 passed, zero failed and 150 skipped. The native skip selectors and reasons exactly match canonical. All Rust ignores and filters retain their reviewed context restrictions. The independent review binds the final receipt and logs. The executing validation tool remains `9b65865e542f1b00b161d8b93a0aa7a5991253bd`; the qualified compiler remains R13. The first failed observer attempt supplies no acceptance credit. Forced frames, comparative profiling, scheduler pairs and private/resource close remain required.

- The first R13 forced-frame gate failed with 13,942 passes, five failures and 211 skips. Its receipt and logs remain under `forced-frames-0dfebb0019e4bfc5bb577e26d7bca2b8832634f7`. All skip selectors and reasons match the normal profile. Three hostname tests failed DNS resolution. Two Docker-tooling tests failed with `StageError.MissingTool`. The owned offline container had no hosts entry for its own hostname and lacked `/usr/local/bin/mold`. Cleanup resolved and inputs stayed unchanged. A four-way causal control reproduced all five failures with the qualified products. Adding only a local host entry fixed only the three hostname tests. Adding only read-only mold fixed only the two tooling tests. Both changes passed all five. Source and products remained unchanged, networking stayed disabled, and every control cleaned up. Evidence is under `.work/wt/perf-forced-context-r13/.work/tmp/context-controls`. The repair belongs to the forced-frame controller and its strict receipt reader. A fresh full forced-frame pass remains required before comparative and scheduler execution.

- Forced-context repair `1e0f2cf77` retains offline networking, supplies a local self-host mapping, and mounts the profile-qualified mold executable read-only. Producer and reader now require matching `mold` snapshots alongside `source` and `binaries` in schema-1 inputs. Incomplete older forced receipts are rejected. Reader controls reject conflicting Docker option aliases before the pinned image. The repaired command passed all five affected selectors. Existing controller tests, eleven baseline-reader tests and independent review passed. Clean validation tool `.work/wt/perf-forced-context-r13` is frozen at `cef1a4fa801aac50604d9539e59e8c9e4874a0bf`, separate from qualified compiler R13. Its fresh full run uses `forced-frames-0dfebb0019e4bfc5bb577e26d7bca2b8832634f7-attempt-context-r2`. The prior failed receipt stays unchanged. Final baseline validation uses this corrected tool with tool-local scratch and the original R13 source, product and functional bindings.

- The corrected full R13 forced-frame attempt passed with 13,947 passes, zero failures and 211 reviewed skips. All five former failures passed. The skip selectors and reasons exactly match the normal profile. Source, Laputa corpus, selected binaries and the qualified mold snapshot stayed unchanged. Process/container cleanup resolved and all three shared locks were free after the run. The independent reviewer bound the completed receipt and native log. This qualifies forced execution behavior, without a speedup or private-parity claim. Comparative profiling, scheduler pairs and private/resource close remain pending.

- Comparative attempt `comparative-0dfebb001-r13-r2` passed on clean R13 source with the qualified opt3 product. This was the first actual comparative measurement; the unused original output name remains absent. All 35 parity cases matched across five workloads and seven tools, and all 3,780 samples completed. Readiness passed before and after, timing was accepted, and cleanup resolved. The independent reviewer bound the final report and receipt. These finite benchmark cases do not extend util-linux exact-parity credit or establish the later four-family performance objective. Three scheduler pairs and private/resource close remain required before G1.

- Scheduler preflight stopped before any reference build because the bridge selected a source-local target outside the launcher's shared namespace and omitted explicit Laputa/capture bindings. The genuine source-local candidate receipt and pinned two-file reference overlay remain unchanged. Repair `dd87fb9f9` derives the shared namespace from the common Git directory, uses the unchanged launcher from its reviewed tool checkout, binds the passed profile's clean Laputa corpus, and validates captured inner Docker argv. Namespace, environment, capture and corpus rejection tests pass, including launcher preflight from main and linked checkouts. Independent review passed. Clean bridge tool `.work/wt/perf-scheduler-bridge-r13` is frozen at `e59008ec1a5ed0425290a860c55bb1112a9e3d98`. It owns the reference build for run `r13-0dfebb001`. The comparison receipt uses `.work/consolidation/native-scheduler-comparison/r13-0dfebb001/receipt.json` under R13. Final baseline validation still uses tool `cef1a4fa801aac50604d9539e59e8c9e4874a0bf`; compiler qualification remains R13. No scheduler execution credit follows from preflight tests.

- Reference build run `r13-0dfebb001` exited 101 before compilation because its existing Cargo argument encoded LTO's `thin` value without TOML quotes. The process record, logs and exact Docker capture remain preserved. No reference artifact or receipt was produced. Container cleanup resolved and locks were free. Repair `a81ba825c` quotes that value. A regression parses all three emitted profile settings and fails on the old argument. Existing lightweight bridge tests pass. Clean tool `.work/wt/perf-scheduler-cargo-r13` is frozen at `5fbca8d82d394c3c39edc1251b00c12cbcde08b3`. Fresh run `r13-0dfebb001-r2` uses the existing qualify, candidate, reference-build and comparison owners. Its final receipt selection is `.work/consolidation/native-scheduler-comparison/r13-0dfebb001-r2/receipt.json` under R13. The earlier failed run and tools remain unchanged. No scheduler pair or reference-build acceptance follows from the syntax regression.

- Scheduler run `r13-0dfebb001-r2` produced valid candidate and reference build receipts. Its comparison stopped after reference listing: the cgroup writer appended command status to the resource snapshot, which the unchanged parser rejected. No scheduler pair ran. Repair `d45ef379d` separates those outputs and passes real-writer success and failure controls. A subsequent environment review independently reproduced missing mold and linked-worktree Git access. Read-only mounts, a fixed local hostname and disabled networking passed all five affected selectors on both arms. The reviewed environment repair is integrated at `f68b1f312`; clean executing tool `.work/wt/perf-scheduler-env-r13` is frozen at `b53ef3d0fa0c7af8439bb92a1e3bcd972366b253`. The r3 launch stopped before Docker because its PATH omitted `perf-gate-ready`; its failed receipt remains unchanged and supplies no acceptance credit. The corrected launcher resolves its external commands before execution. Reuse the genuine r2 builds with fresh comparison output `.work/consolidation/native-scheduler-comparison/r13-0dfebb001-r2-attempt-cgroup-r4/receipt.json` under R13. Failed attempts remain unchanged. Three scheduler pairs and actual private/resource close are still required. Compiler qualification remains R13; final baseline validation still uses tool `cef1a4fa801aac50604d9539e59e8c9e4874a0bf`.

- Final-consumer review found that the scheduler reader expected all three binaries in the reference build receipt, although the reference producer rebuilds and receipts only `xsht`. Repair `641a77c02` matches that existing producer contract. Full binary comparison and shared-child integrity checks remain mandatory. Eleven focused tests pass, including coordinated receipt tampering and child-binary mutations on both arms. Final baseline validation now uses clean tool `.work/wt/perf-scheduler-reader-r13` at `549466bd8c99f051c797fc85078da0b3bb6d3490`. This supersedes the previous consumer tool selection, without changing R13 or rerunning scheduler measurements. Parallel Laputa acceptance owns `.work/wt/laputa-world-acceptance`, based on clean sibling source `bbe05e466e95f7ba8b26b17dea0bfcd5365510e9`. Its expensive work remains outside exclusive G1 timing windows.

- Scheduler R4 completed all three pairs, but pair validation rejected the results. Each reference suite recorded 13,944 passes, one failure and 213 skips. Candidate suites recorded 23, 19 and 24 failures. Every suite failed the same `uu-ls` fixture when scratch creation reached read-only `/work/.work/tmp`. Candidate logs also retain output mismatches, descriptor/network failures and timeouts. All readiness and cgroup checks passed, inputs stayed unchanged and cleanup resolved. These runs supply diagnostics, not scheduler acceptance. Repair work owns `.work/wt/perf-scheduler-diagnosis-r13`; preserve R4 unchanged and keep final private/resource close blocked. Laputa acceptance now owns the heavy build and VM window. Its source preparation found and reproduced a separate snapshot bug: ordinary Git staging omitted an ignored but tracked test fixture. Force-adding the trusted archive preserves the source tree and passes the focused regression. Full world and guest acceptance remain pending.

- Focused scheduler-failure repairs are integrated locally. `a758be15e` isolates the TTY test's stale-descriptor assertions in a child, preventing legal descriptor reuse in another test from turning a second close into a foreign close. `2b6fcfc66` moves the `ls` socket fixture into writable temporary storage and preserves its short relative address. `2515907c8` replaces three tail fixtures' startup sleeps with output and diagnostic barriers, and starts tee's byte deadline after application readiness. Controlled old fixtures fail and repaired fixtures pass. The parent replay passed seven exact selectors on the integrated source using unchanged R13 products; source and product qualification remain distinct. Long scratch-path and withheld-output controls preserve the socket limit and original tee deadline. One mistyped short-read filter selected no test; its corrected invocation passed. UDP passes alone and with its 22-test file, while bounded system-report groups also pass. Their full-corpus failures remain unexplained. Preserve R13 and failed R4; no new freeze, scheduler acceptance or private/resource close follows from these focused results.

- Owner accelerated parallel I1/M1 launch on 2026-10-18 after the documentation refresh. Compiler integration owns `.work/wt/perf-inference-integration`; corpus work owns `.work/wt/perf-inference-corpus`; measurement work owns `.work/wt/perf-measurement-launch`. Independent compiler review and G1 failure diagnosis run alongside the existing isolated Laputa lane. The Deno diagnostic retries only its failed node under the same 2-CPU, 8-GiB, offline limits. This subset supplies no full-world acceptance. Timing remains exclusive and no synthetic close report supplies measurement credit.

- Inference compiler candidate `4233e34ee` was reviewed and merged locally at `ed7077a9c`. Its focused boundary passed 126 root controls, 14 tooling controls and 226 native tests with zero failures or skips; one unchanged autofix test was excluded. Both prior audit defects and a ground `with` binding producer-proof defect have red/green controls. Generated SPEC rendering, snippet diagnostics and all three tour-project tests passed after integration. Formal inventory maintenance and the combined regression boundary remain required.
- The R13 performance cohort completed all 1,530 timed samples and 30 parity checks with both readiness checks passing and cleanup resolved. These are provisional pre-inference observations, not an accepted baseline or speedup claim. Candidate corpus checking reproduces the frozen 1,040-removal result; removing all additional 730 checker-restored rows fails. Four practical pure-return removals pass paired behavior and signature checks. All 202 effect clauses remain retained.
- The isolated Deno node built successfully offline under the 12-GiB exception. Its prerequisite artifacts and the full-world plan are retained. Full-world completion, rooted functionality, unchanged rebuild reuse and actual Asterinas acceptance remain required; the deployment lane resumes that full remainder.

- Combined source `7851fd22e` passed singleton bootstrap, then failed canonical root integration: 221 passes and four failures before the full native stage. The failures are map/record per-read allocation budgets, signal-hook registration snapshot semantics and pure-parameter grammar correspondence. Source stayed clean and cleanup resolved. Preserve that failed receipt under `.work/wt/perf-i1-combined-freeze`; focused repairs use `.work/wt/perf-i1-combined-fixes`, with the original freeze and prior qualified products unchanged. No canonical, G1 or I1 combined acceptance follows from this attempt.

- Laputa's isolated world build, store verification, rooted functionality and unchanged rebuild had passed before an accidental `make verify` deleted its derived outputs. Its driver implicitly called `make clean`. Ordinary Laputa and persistent input caches were untouched. Fix `294e848` removes that implicit clean and is integrated into the acceptance checkout. A real-driver regression first reproduced deletion, then passed with exact sentinel bytes preserved and no Docker cleanup. All 18 seed tests passed, followed by the exact regression on the integrated checkout. Recovery uses the supported package publisher and remote importer for matching cached artifacts. Current world, root, reuse and Asterinas acceptance remain pending.

- The measurement selection now appends six demanding JSON and directory cases at package ranges 16,384, 32,768 and 65,536. The original fifteen cases and their provisional observations remain unchanged. Three focused generator, selection and measurement-failure controls pass on the integrated source. The new cases have no timing or acceptance credit yet.

- Combined source `63130a3d5` passed bootstrap, 228 root integration tests and the previously failing allocation, hook and grammar controls. Canonical execution then stopped in the debug library: 1,040 passed, one failed and three were ignored. The documentation scanner mistook the member expression `source.observation.state` for a diagnostic code. The corpus consistency check passed. A focused scanner regression reproduces the false positive; the token-boundary fix passes all six diagnostic checks while retaining actual unknown-code controls. Preserve the failed receipt in `.work/wt/perf-i1-combined-freeze-r2`. Native and supplemental stages did not run.

- Combined source `257c2d52a` passed the formerly failing documentation check and all 1,041 debug-library tests, with three existing ignores. Its tooling target then failed the contextual nominal-import test. The exact selector reproduced immediately: local slot facts used the resolved error-family owner while emitted annotations retained the import alias. The producer fix applies the existing scope-aware canonicalization to three local annotation paths. Four accepted declaration cases, two unchanged const rejections, all three contextual-root selectors and two verifier-authority controls pass with matching rebuilt products. Preserve the failed r3 receipt; native and supplemental stages remain unrun.

- Laputa acceptance now contains the reviewed lifecycle and XSHI-proof port at `636113a`. The exact lifecycle and runner tests pass with the current R13 seed; the guest proof has only passed typechecking. Deno recovery verified all eight closure artifacts. The subsequent full-world build stopped on remote metapackage import: publication correctly omits its archive, but planning substitutes a legacy archive URL. The container was removed and partial artifacts remain reusable. The isolated fix owns `.work/wt/laputa-remote-meta-import`; no full-world, rooted-profile or guest acceptance follows from this failed stage.

- H3 maintenance passed on the unchanged Lean 4.35 formal project: Lake build, axiom audit and self-test, guarded fresh paranoid check, sandbox self-tests, inventory check and all 51 inventory tests. The proof inventory still has 72 planned implementation obligations, no frozen Rust source identity and no conformance cases. These checks do not establish Rust compiler verification.

- Combined source `6957e88f4` passed 228 root integration tests, 1,042 debug-library tests with three existing ignores, and all 122 XSHT integration tests with five existing exclusions. Soundness then failed its unchanged minimum probe-count guard: five scalar receiver families were omitted after their registry types became explicit. The adapter repair exposed a separate checker panic on an unknown entry-parameter type. Reviewed repairs `03569f728` and `e91c777f8` restore scalar probe coverage and retain ordinary diagnostics for invalid entry signatures. The nine-source, three-mode diagnostic matrix and all 14 unchanged soundness tests pass; both fixes are integrated locally. Preserve the failed r4 receipt; native and supplemental stages did not run. Commit `4105a3823` moves soundness ahead of the debug and XSHT corpus targets without changing commands or exclusions. The ordering regression failed before the change and both focused workflow tests pass afterward.

- Laputa fix `5b016b43` is reviewed and integrated into the isolated acceptance checkout. Remote retrieval now distinguishes archives from metadata-only packages; existing index and plan wire versions remain unchanged. Import validates metadata before creating the canonical metapackage marker. All 229 package-manager tests pass with unchanged seed products. The run also corrected an incomplete response mock and a scratch login shell that discarded the configured runner path. Full-world recovery, rooted functionality, rebuild reuse and guest acceptance remain pending.

- Laputa acceptance is clean at `17211df05ee51a8e9698d5179b4c044f2908ea3b`. Firecracker installation now checks the pinned binary hash before executing it. The host regression first observed premature execution, then passed both rejected-hash and accepted-installation cases after the fix. The integrated host test passes. Offline full-world reuse verifies all 85 artifacts without starting a package build. The current profile plan composes eight runtime roots, and XSH executes inside that root. Root validation checks 1,482 files, 58 ELF files and 20 dynamic ELF files with zero loader failures. Pinned Asterinas Cargo inputs and Firecracker are fetched. Kernel build and actual guest acceptance remain pending. These results use the unchanged R13 seed, not the combined inference candidate.

- Combined source `c24ce1759` passes bootstrap, 228 root integration tests, all 14 soundness tests, 1,043 debug-library tests and all 122 XSHT integration tests. Existing ignores and exclusions remain unchanged. Full native execution found a file-check failure in `core/tests/test-btop-sources.xsh`. A standalone check reproduces `compact.indexed-build` on the `process_stat` call that supplies a named defaulted argument. Repair work owns `.work/wt/perf-call-default-r5`. The full suite continues to collect remaining results. No combined acceptance follows from this attempt.

## For the owner afterwards

- Read the concise accepted baseline page and deployment reproduction commands.
- Review source-level behavior/API changes, inference classes and retained sites, individual workload results, and the artifact experiment decision.
- Review the exact finite/formal distinction for the selected transformation.
- Inspect local XSH and Laputa integration branches. Merge only reviewed accepted commits under the launch grant. Nothing is pushed by this campaign.
