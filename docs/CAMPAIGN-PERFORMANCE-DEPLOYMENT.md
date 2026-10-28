# Make XSH fast to run and dependable in Laputa

**Status:** paused 2026-10-25 on `master`. No campaign loop, lane or
qualification retry is scheduled. The [G0 release
candidate](ROADMAP.md#g0-release-candidate) defers this campaign's performance
obligations. Deser has since been cut over. No push grant exists.

**Resume:** the owner's prompt is "Resume this campaign from its 2026-10-25
handoff on master. Prioritize feature completeness and focused evidence; leave
XSH-23 deferred." No owner decision is needed to preserve the paused state.
From the repository root run `git status --short && git log -5 --oneline`, then
read `AGENTS.md`, `TODO.md`, and this document. Recheck the limiting stage and
owner authority. Do not restart the old queue or rebaseline unchanged inputs.

## State

Twenty-seven backlog tickets are closed. XSH-1, XSH-3, XSH-14 and XSH-19 remain
open in `TODO.md`, and XSH-23 is owner-deferred. Accepted feature delivery ends
at `25639900b`, including typed CLI entries and the Laputa migration `c8cdb73`.
Winddown also integrated the lint candidate `97e361171` (merge `6300eb05b`) and
the lsmem fallback simplification `f60a72f41`. The lint change preserves
diagnostics and passes its focused tests, RSS-only pairs, binary-size checks and
the xsht compiler check. Its CPU acceptance is open: **perf gate deferred: host
busy**. No speedup or full qualification is claimed.

No current source has G1 qualification. The R13 baseline `0dfebb001` is the
last source that passed most gates, and old passes do not qualify later source.

- **Passed on R13:** canonical, supplemental, functional-close, normal opt3
  profile, timed observer, forced-frame and comparative gates.
- **Failed or unrun:** the three scheduler pairs failed validation, and focused
  repairs (`a758be15e`, `2b6fcfc66`, `2515907c8`) were not requalified. UDP
  failures and full-corpus timeouts remain unexplained. Private and resource
  close (`dev/util-linux/check.py resources|close`) is unrun.
- **Limiting stage:** current-source acceptance and the remaining XSH-1, XSH-3
  and XSH-19 delivery, not a shortage of lanes. On resume, choose the next
  substantive delivery from those, preserve focused evidence, and steer again
  after the first accepted wave or two failed attempts.
- **Environment:** the owner-approved ef7 image is the environment.
- **Laputa world:** the isolated world build, store verification, rooted
  functionality and unchanged rebuild passed with the R13 seed (85 artifacts
  reused offline). Actual KVM smoke passes with `ASTERINAS_PVH_BOOT_OK`. Full
  Asterinas guest acceptance is open. The run with the R13 seed stopped at 159
  passes, one failure and four skips. The failing AWK case requires
  `systime() > 1000000000`. RTC device failures and the pinned kernel's epoch
  fallback suggest a clock cause, which is unproven.
- **Inference:** the accepted capabilities are merged on `master`. The
  [inference campaign](../full-inference-campaign.md) owns the remainder.
- **Lean:** H3 maintenance passed on the unchanged Lean 4.35 project. See
  [the Lean campaign](../xsh-lean.md).

## Retained inputs

The `.work` checkouts below are disposable. Recreate any of them from its
revision with `git worktree add`.

| Input | Revision | Use |
|---|---|---|
| `backlog-finish-lint` | `97e361171` | Matched release baseline and candidate, RSS evidence, and pending guarded CPU acceptance for XSH-14 |
| Detached R13 baseline | `0dfebb001` | Release baseline products and receipts for the remaining comparative gates |
| Detached ef7 reference | `57a0f6314` | Protected reference products built with the approved ef7 image |
| Laputa qualification corpus | `f201300d` | Common comparison corpus, at `.work/wt/qualification-corpus-f201300` in Laputa |

Superseded lane snapshots live only in Git history and in local branches that
`git branch --no-merged master` lists. Never merge them wholesale. They hold
obsolete intermediate implementations and lack later integration corrections.
Retry one only when a focused source comparison finds a useful change missing
from `master`.

## Goal

Make the existing language faster to run and dependable in Laputa, without a
syntax-feature campaign. Correctness and parity come before optimization.
Representation-changing optimization waits for its scheme and IR contract. An incomplete
mandatory item prevents a successful close.

| Item | Outcome and proof |
|---|---|
| G0 qualify exact checks | Stale `abort` benchmark inputs repaired and the two native foundation selectors pass. **Done.** |
| G1 close the inherited acceptance queue | Reproduce and fix XSH-2 at its origin, then run the finite current-source queue (below) with zero unaccounted failures or skips. Final qualification needs G2's functional validator. |
| G2 narrow measurement adapters | `dev/performance/check.py` dispatches over current tools and separates parse/load, check/solve, publish, lower, verify, execute and first-output time. `self-test`, `functional`, `baseline` and `measure` exist. **Done.** |
| G3 audit isolated inference | Reviewed capability dependency list, source diff, original and reduced cohorts, and retained-site classes. **Done.** Evidence: [inference evidence](PERFORMANCE-INFERENCE.md). |
| G4 disposable Laputa inputs | Synthetic PM controls, exact candidate-XSH injection, network-disabled execution, output confinement. **Done.** |
| G5 retire consolidation infrastructure | Complete XSH-19 after G1. Keep reusable checks under ordinary developer commands and remove campaign adapters, duplicate entry points and historical pins. Prove replacement commands reject invalid inputs before deleting their predecessors. |
| G6 Laputa world and Asterinas | Full x86-64 world build with the frozen XSH candidate, root functionality, unchanged rebuild reuse, then the real Asterinas smoke and full guest acceptance. No mock, filtered package subset, stale compiler pin or host-only proof qualifies. |
| I1 integrate inference | Merged. The combined regression boundary runs once at the accepted source. |
| I2 remove missing inference relationships | Owned by the inference campaign. |
| D1 immutable package lifecycle | Source validation, build and proof, root generation, version replacement, recomposition and removal, GC, and injected failure at each publication boundary. |
| D2 process and rooted-filesystem workflows | Bounded process trees, signal and timeout outcomes, reaping, raw filenames, symlink swaps, hardlinks, permission errors, interrupted publication. No escape from the declared root. Focused workflows are integrated. |
| M1 representative measurements | Four workload families with exact outputs, common and demanding shapes, n/2n/4n scaling, and completed-work counts, on the post-inference source. Wiring is integrated. |
| F1, F2 formal binding | Bind one measured transformation to its formal assumptions with mutation controls. Separate theorem-backed properties from finite differential evidence. |
| H1 prepared-module behavior | The source path is the oracle. Freeze the module and linker corpus and the failure controls before the experiment. Decoder limits: at most 64 MiB encoded data, one million entries in any table, nesting at most 256. Check lengths, ranges and arithmetic before allocation. Raw input never constructs `FullProgram` or bypasses `FullVerifier`. |
| H2 deployment hardening | Corrupt or truncated inputs, concurrent publication, interruption, partial output and cleanup failure leave only valid generations. Fetch pinned inputs separately. Test and build runs have no network. |
| H3 formal and documentation maintenance | Lake build, axiom audit and self-test, guarded paranoid check, inventory checks. SPEC sources, registry docs and reproduction docs updated. |
| O1 prepared shipped modules | Experiment with exact-release immutable data, linked and verified, run by the existing indexed executor. Ship only if it meets its keep gate, otherwise remove it without a runtime flag or alternate executor. |
| O2 stream materialization | Remove copying while preserving pull timing, short-circuiting, errors, backpressure, cancellation and cleanup. Prove bounded memory for unbounded input. |
| O3 script algorithms | Improve measured comparisons, copies and spill behavior in `core/lib/`. Preserve GNU, uutils and BusyBox behavior only on the tested overlap. No applet-named Rust acceleration. |
| O4 host and frame work | Remove repeated metadata or argument work identified by profiles. A new primitive must be reusable and bounded. |
| R1 boot the candidate | `make profile-test` in the isolated Laputa checkout with the candidate bound into its package identity: real guest marker, screenshot, bounded clean shutdown, zero surviving VM descendants. |
| R2 freeze and review | Combined gates, one full native suite, inherited gates, workload parity, one serial performance window, formal maintenance, and an independent review with no P1 or P2 open. |
| R3 retire machinery | Remove dead experiment paths and temporary adapters, update completed tickets, and write one accepted-baseline record (`docs/PERFORMANCE-BASELINE.md`, not yet written) with deployment reproduction commands. No release upload, tag or push. |

Publish `docs/PERFORMANCE-BASELINE.md` at G1 completion with the accepted
source, product locations, platform, limitations and gate commands. Update it
after the inference integration and at close.

Ordering: G1, G2, G3 and G4 overlap after G0, and G1 owns the timed windows.
G5 runs immediately after G1. G6, I1 and M1 start in parallel with
consolidation. No P3
item starts until its workload completes the P1 and P2 behavior corpus.
Artifact schema work waits for I1 and H1.

### The finite inherited queue (G1)

Freeze a clean integrated source and use the existing gate tools. Never
manufacture `functional_complete: true` or copy source credit from an earlier
receipt.

1. Revalidate the functional and structural obligations with the canonical
   owner and the supplemental launcher. Run the deep grammar proof once. Then
   `dev/performance/check.py functional --check` issues the functional-close
   record for this exact source.
2. Run a fresh normal profile with `profile-cost.py`. With its release products,
   run `dev/perf/btop-profile.xsh` in an isolated, readiness-bracketed window.
3. Run one timed canonical and resource observer with
   `final-observers.py --execute`.
4. Run the forced profile, the final comparative profile, and three scheduler
   pairs.
5. Run util-linux `resources --check` and `close --check`.
6. Check final metrics and document only required known limits.

The source-qualified receipt must meet its existing budgets, including the
isolated 15-second repository lint gate. Readiness is mandatory for time
credit. Binary size and memory still run when timing is deferred. Keep the
exact `lsipc` and rooted-memory exceptions unchanged. The inherited tools use
absolute workspace paths, so validate their `--plan` output before an expensive
run, and fix relocation only for a demonstrated failure. Run the canonical
bootstrap stage before `execute`:

```sh
python3 -B dev/consolidation/final_canonical_replay.py bootstrap --repo SOURCE --head COMMIT --tree TREE --output .work/tmp/bootstrap-RUN --run-name bootstrap-RUN
```

`execute --bootstrap-receipt` consumes `.work/tmp/bootstrap-RUN/bootstrap-receipt.json`.

## Decisions in force

### Owner decisions

| Decision | Scope |
|---|---|
| Pursue performance and deployment after consolidation and inference integration. Improve the existing language. | 2026-10-17 |
| Execute with local commits and integration, isolated Laputa edits, pinned fetches and owned Docker/KVM operations. No push, live roots, host installation or publishing. A separate execution grant is required on resume. | 2026-10-17 |
| Allow `doas` for KVM access, limited to required device, container and VM operations on owned paths. | 2026-10-17 |
| Allow one build of the existing Laputa host-tools image, then use the saved image offline. Other moving inputs are not authorized. | 2026-10-18 |
| Retain the thirteen exact `lsipc` reference-crash exceptions. Independent XSH assertions give no exact-parity credit (see `dev/util-linux/README.md`). | Util-linux campaign |
| Start Laputa world and Asterinas acceptance in parallel with consolidation, and inference integration alongside G1 and G6. Preserve exclusive G1 timing windows. Final G6 acceptance needs the same XSH source to qualify under G1. | 2026-10-18 |
| Defer XSH-23 (bbtree evaluation). | Owner-deferred |
| Waive the performance thresholds of the Deser cutover. | [ROADMAP](ROADMAP.md) |

### Prescribed decisions

| Decision | Reason and reversal condition |
|---|---|
| Keep x86-64 Linux musl and the pinned test image for XSH qualification. G6 also qualifies Laputa's pinned Asterinas backend. | Reuse established settings and isolation. No new backend. |
| Merge inference by capability and patch, never a whole branch. | A whole-branch merge can restore obsolete compiler or toolchain state. |
| Keep original inference accounting and practical acceptance separate. Do not claim the 95%/85% targets unless they pass. | Preserves the 9,194 sites, 693 eligible locals and 2,190 eligible internal annotations. See [inference evidence](PERFORMANCE-INFERENCE.md). |
| Preserve explicit capability restrictions. Elide a clause only when the inferred contract is equivalent. | A smaller current body does not authorize weakening a declared restriction. |
| Evaluate immutable release-shipped modules first. | Their source, registry, compiler and features are controlled. Arbitrary-script caches and persistent user formats are excluded. |
| Keep prepared artifacts private and exact-version matched. Reject malformed or mismatched data before execution with no silent source fallback. | No stable ABI or migration promise is needed. |
| Treat prepared modules as an experiment with a keep or reject gate. | A rejected experiment leaves no runtime flag or alternate executor. |
| Reuse current measurement and VM tooling. Extend narrow selectors and phase counters where missing. | No new benchmark platform, VM supervisor, proof inventory or provenance system. |
| Test immutable package and root generations. Model upgrade as a replacement generation and removal as recomposition plus reachability-based collection. | Laputa has no mutable install contract. |
| Use the existing `qemu-dwl-foot` profile, adding bounded shutdown assertions. | It is Laputa's supported image and guest proof. |
| Run test and build execution offline after explicit input acquisition. Fetch only recipe-pinned inputs for the selected closure. | No broad `make fetch`, new catalogue entries or moving versions. |
| Preserve source-facing APIs and the single indexed executor. | A new public contract, native backend, daemon or compile-first workflow is outside this plan. |
| Add no production dependency by default. A concrete missing operation needs license, maintenance, transitive-weight, build, size and startup review. | |
| Keep compiler qualification separate from receipt-reader repair. Name both revisions. | A reader-only correction gives no compiler credit to the newer tool revision or `master`. |
| Retain the vendored `regex-lite` 0.1.9 fork in `third_party/regex-lite`. | Upstream's `std` build uses a randomly seeded capture-name map. With `getrandom` returning `EIO` the upstream consumer panics with status 101, while the fork completes and makes no `getrandom` call. Upstream 0.1.9 rejects disabling `std`. A standalone consumer (release opt3, thin LTO, five alternating cold builds) showed build CPU 5.24 s against 5.21 s, peak RSS 298,180 against 299,848 KiB, and stripped size 529,600 against 538,416 bytes, within the spread. Whole-XSH qualification stays with G1. The fork keeps upstream's MIT or Apache-2.0 licensing and adds no dependencies. Its two differences are the capture-name map and a `usize::MAX` spelling for warning-deny builds. Reconsider when upstream compiles regexes without an entropy dependency in a supported configuration. |

Delegated: choose fixtures and algorithms inside the fixed families, freeze
workload selection before candidate measurement, and prefer the smaller
reversible design. Limit each mechanism to three profile-supported experiments.
The scheme/IR owner chooses the private encoding, linker relocation and bounds,
preserving the semantic verifier's authority. Never serialize process-local
pointers, interned integer identities without an owner, or live resources.

### Numeric performance gates

These are prescribed engineering targets, not measured wins. M1 freezes the
primary workloads and shapes before O1 to O4. Use at least five alternating
baseline and candidate pairs, with AB/BA reversal, and keep larger counts where
the reused runner owns them. Report median, range, minimum CPU and paired
spread per workload, never one aggregate speed ratio. A regression must exceed
the budget and the observed spread, and so must a benefit.

| Target | Pass condition |
|---|---|
| Prepared-module experiment | At least 20% lower median end-to-end startup on two shipped-module workloads, including template, with all H1 cases passing. Otherwise reject and remove it. |
| Shared runtime improvements | At least 15% lower median CPU on two preselected substantial workloads from different non-startup families, each with at least 50 ms of baseline CPU work. Mechanism counts explain the change. |
| Untouched startup controls | Existing per-case limit `max(5% of baseline, 1 ms)`, and no new failure or helper process. |
| Substantial workload regressions | Candidate median CPU at most baseline plus `max(5% of baseline, paired spread)` on every frozen unaffected workload, and any stricter existing budget. |
| Binary sizes | Each release product at most 5% larger than the post-inference baseline, counting embedded artifact bytes. |
| RSS and retained memory | Each at most baseline plus `max(10% of baseline, 2 MiB)` on unchanged source. |
| Build cost | At most 10% additional median CPU for matched incremental and release builds, outside the spread. |
| Solver scaling | Existing inference bounds and negative cases pass at n/2n/4n. No elevated limits, silent recovery or new `Any`. |
| Observation overhead | Disabled observers allocate nothing per operation. Profiling runs never supply timing samples. |

"Cold" means a fresh process without session or cache reuse. State page-cache
warmth separately and do not flush host caches. Measure first useful output
separately from complete exit. Time gates run only when `perf-gate-ready` passes
before and `perf-gate-ready -1` passes after, and a busy host reports `perf gate
deferred: host busy`. Memory and size still run. Deferred timing never passes and
blocks a full release.

## Resource and ownership limits

Ordinary iteration allows at most three compiler builds, each at two jobs and
8 GiB. The isolated Laputa Deno link may use 12 GiB with two CPUs and no extra
swap, because its 8 GiB container OOM reproduced and the 12 GiB build passed.
Inherited release builds keep 12 jobs, 12 CPUs and 24 GiB and run alone. A full
native observer keeps 12 CPUs and 24 GiB. Each inherited scheduler pair keeps
32 CPUs and 40 GiB and runs alone. Do not reduce those allocations to obtain
incomparable timing. Park a gate whose allocation cannot fit. Lightweight test
lanes get two CPUs and 4 GiB. A VM lane gets four CPUs and 8 GiB including guest
memory. Never run two full native suites together.

Reserve 12 GiB for the host. Admit another lane only when the sum of enforced
memory limits fits current available memory minus that reserve, and reduce width
on swap activity, cgroup memory events or disk pressure. Timing windows exclude
other builds, suites, VMs and profiling. Run verification from a frozen scratch
checkout, not the moving integration tree. Use `.work/wt/perf-<lane>` for XSH
lanes and `.work/wt/laputa-<lane>` under the XSH campaign for isolated Laputa
checkouts, and never write the sibling's normal checkout or shared artifact
store. Do not run formatters, change CI, or update the Rust nightly or the pinned
Lean.

After two repetitions of a focused check over 30 seconds, repair the loop before
adding implementation width. A focused command group has a 120-second ceiling,
and a slow full gate is never the repair loop. After three refuted hypotheses,
recheck source, products, inputs and ownership, and bisect before another
repair. Never retry a flaky test to green. Bound each failing item to three
repair attempts or two hours, then park it with its reproducer.

## Acceptance commands

`dev/performance/check.py --config PATH` reads explicit source, product and
Laputa checkout paths, with no hidden fallback to the normal checkout.
`inference`, `optimize`, `deployment` and `close` are unavailable and cannot
supply acceptance.

| Deliverable | Command | Threshold |
|---|---|---|
| Proof foundation | `python3 -B dev/performance/check.py --config PATH self-test` | All negative controls reject, positive fixtures pass, zero leaked processes. |
| Functional qualification | `python3 -B dev/performance/check.py --config PATH functional --check` | Required correctness and review results qualify the exact source before the functional-close record is emitted. |
| Accepted baseline | `python3 -B dev/performance/check.py --config PATH baseline --check` | The finite inherited gates pass for the named source and products, including util-linux resources and close. |
| Measured families | `python3 -B dev/performance/check.py --config PATH measure --check` | Every selected input has exact output, completion checks, raw samples, CPU, RSS and attribution, with readiness accepted. |

Run the inherited full suite only for G1's acceptance source, I1's combined
integration, and R2's final source. Widen after a material uncovered
integration risk and state the reason first.

Development loop:

```sh
python3 -B -m unittest discover -s dev/util-linux -p test_resources.py
cargo dev gate src/sema/types.rs
cargo dev gate PATH...
xsht test --exact tests/xsh/stdlib/template.xsh::test_template_fields_and_dot
xsht test --exact tests/xsh/stdlib/fs.xsh::test_fs_root_operations_reject_traversal
xsht runtime-stats --json .work/tmp/runtime.json SCRIPT
xsht trace --syscalls --trace-top-syscalls 10 SCRIPT
```

The inherited profile adapter invokes these existing interfaces:

```sh
python3 -B dev/consolidation/winddown/gate-tools/final-correctness-gates/launch.py --plan
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/final-observers.py --plan
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/profile-cost.py --tree SOURCE --commit COMMIT --functional-close CLOSE_JSON --laputa-corpus LAPUTA --output OUTPUT
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/final-observers.py --execute --tree SOURCE --commit COMMIT --functional-close CLOSE_JSON --profile-receipt PROFILE_JSON --release-opt-level 3 --output OUTPUT
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/forced-frames.py --tree SOURCE --commit COMMIT --functional-close CLOSE_JSON --laputa-corpus LAPUTA --profile-receipt PROFILE_JSON --output OUTPUT
python3 -B dev/util-linux/check.py resources --check --laputa-corpus LAPUTA
python3 -B dev/util-linux/check.py close --check --laputa-corpus LAPUTA
```

Reuse `forced-frames.py`, `final-comparative.py` and
`native-scheduler-comparison/compare.py` for their rows. The old ARM64-only
`bench/stdlib-port/run.py --linux` route is not the x86-64 gate. The scheduler
comparator keeps networking disabled and supplies a local `xsh-test` hostname
mapping. Its read-only mounts expose the source at `/work` and its original
absolute path, the shared Git directory, and `/usr/local/bin/mold`. The launch
owner must match the `runner_artifacts` map's mold bytes to the passed profile's
mold binding before comparing.

Use absolute source-qualified verification or release `xsh`, `xsht` and helper
paths inside the pinned container. `cargo dev gate` does not replace private
root-lib scheme and verifier tests. Select those by exact name in a reusable
debug harness and run them directly.

Within the isolated Laputa tree, bind `XSHT` and its sibling `xsh` to candidate
products and start with these controls:

```sh
"$XSHT" test --exact tests/pm/pm_execute.xsh::test_execute_builds_dependency_levels_in_isolated_roots_and_reuses
"$XSHT" test --exact tests/pm/pm_execute.xsh::test_execute_proof_failure_and_corrupt_final_never_publish_replacement
"$XSHT" test --exact tests/pm/pm_root.xsh::test_root_failed_composition_leaves_completed_output_untouched
"$XSHT" test --exact tests/pm/pm_store.xsh::test_store_gc_keeps_named_artifacts_and_removes_the_rest
"$XSHT" test --exact tests/system/qemu.xsh::test_qemu_supervisor_retries_qmp_injects_once_and_escalates_shutdown
make profile-test
```

D1, D2 and R1 extend these with the missing observable transitions. Prove that
the tested and booted compiler is the candidate, not an old seed or a `PATH`
binary. For R1, pass the clean candidate checkout as `XSH_ROOT` and absolute
candidate paths as `XSH_BIN_DIR`, `XSH`, `XSHT` and `XSH_HOST` to the isolated
Laputa Makefile, then run `make seed ARCH=x86_64 CARGO_JOBS=2`. In
`.out/seed/x86_64/manifest.json`, `xsh_commit` must equal the candidate,
`xsh_dirty` must be false, features and target must match, and every listed
product and `core.tar.xz` must match its bytes. Feed that seed through
`packages/xsh/PKGBUILD.xsh` into the root generation, and check the generation's
`/usr/bin/xsh`, `xshi`, `xsht` and installed core content against the seed before
boot. Do not add executor version or timing data to immutable package keys.

Formal maintenance follows `formal/README.md`: put the Lake workspace and caches
under `.work`, and from `formal/` run `scripts/audit-axioms.sh`,
`scripts/audit-axioms.sh --self-test` and `python3 scripts/check-fresh.py`, within
the 8 GiB, 180-second build and 1,080-second check bounds. No fragment count
closes a universal obligation.

## Boundaries

The 2026-10-17 grant covers the work above and no more. Do not edit the sibling
normal checkout, host packages, live services, devices other than required
virtual-machine access, boot configuration, or real mounts. Public API and
language semantic changes beyond the accepted inference scope are forbidden.
Preserve diagnostics, effects, module visibility, path bytes, process exit
semantics and publication boundaries, and report every contract change.

Non-goals: new syntax, a native backend, JIT, daemon or mandatory warmup,
command-specific Rust behind XSH command names, persistent arbitrary-script
caches or a public artifact format, another kernel or OS port or package-catalog
campaign, any production deployment or release upload, and closing all 77
universal formal obligations.

## Log

- 2026-10-17: Launch source `c6d36683872e4d0dc9173a9c2c8ff09e8e6b9d23`. KVM access through `doas` works, so sandbox visibility is not a platform limit. G2 may extend `check-current --review-evidence` to call the existing review verifier while preserving default checks and strict `final-close`.
- 2026-10-18: The scheduler bridge derives the shared namespace from the common Git directory, using the unchanged launcher from its reviewed tool checkout. Compiler qualification stays with R13.
- 2026-10-24: Paused at the owner's request. Completed slices were integrated, three incomplete branches parked, and 312 checkouts reclaimed.
- 2026-10-25: Paused on `master`. Lint and lsmem source integrated, obsolete lane snapshots committed and their checkouts retired, full qualification explicitly pending.
- 2026-10-28: G0 defers this campaign's performance obligations. The inference log lines moved to [the inference campaign](../full-inference-campaign.md#log).
