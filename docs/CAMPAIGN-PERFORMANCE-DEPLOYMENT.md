# Make XSH fast to run and dependable in Laputa

**Status:** resumed by the owner after the 2026-11-04 wind-down. See the
[handoff](CAMPAIGN-HANDOFF.md) for retained candidates and incomplete checks.
The previous qualification image is unavailable locally; the current image is
pinned below. No current-source G1 qualification has passed. One cause of the
scheduler-pair failures is fixed and measured. No push grant exists.

**Resume scope:** Linux performance and G1 qualification include
the time gates previously waived for Deser S6. Use the whole host through
coordinated readiness windows. macOS remains outside this run. Freeze the
integrated source before source-bound qualification; earlier passing receipts
do not qualify it.

## State

Twenty-seven backlog tickets are closed. XSH-1, XSH-3, XSH-14 and XSH-19 remain
open in `TODO.md`, and XSH-23 is owner-deferred. Accepted feature delivery ends
at `25639900b`, including typed CLI entries and the Laputa migration `c8cdb73`.
Winddown integrated the lsmem fallback simplification `f60a72f41`. The lint
candidate `97e361171` (merge `6300eb05b`) was reverted by `d71eca39c` after its
CPU measurements showed no benefit outside the spread. See XSH-14 and the
historical measurements below. No current-source full qualification is claimed.

No current source has G1 qualification. The R13 baseline `0dfebb001` is the
last source that passed most gates, and old passes do not qualify later source.

- **Passed on R13:** canonical, supplemental, functional-close, normal opt3
  profile, timed observer, forced-frame and comparative gates.
- **Scheduler pairs:** all three R13 pairs failed validation. The reference
  arm (8 workers) failed one test on a read-only mount (`2b6fcfc66`). The
  candidate arm (32 workers) failed 19 to 24 per round. Four to six were
  fixture races: the UDP error-queue test raced a concurrent fork that still
  held the closed port (`e3179f233`), and the descriptor, tail and tee
  fixtures raced under load (`a758be15e`, `2515907c8`). The other 15 to 18
  were timeouts of tests that take milliseconds alone. Those were fork cost:
  every child started through a `pre_exec` closure, so each spawn forked the
  shared runner under its address-space lock and stalled the other threads'
  page faults (330 s window: 589 s system against 53 s user CPU, threads
  blocked in `lock_mm_and_find_vma`, and no wall-time gain over 8 workers).
  `0836935e3` starts children with `posix_spawn` unless a signal needs
  resetting. On matched verification builds a 330 s window of the native suite
  at 32 workers completed 2,839 and 2,883 tests before and 7,941 and 8,214
  after, with five timeouts falling to one; at 8, 16 and 32 workers it now
  completes 4,437, 6,793 and about 8,000, so the 32-worker default scales. A
  whole-suite run at 32 workers finished in 17.7 min with 38 failures, all of
  which fail identically on the unpatched build in isolation (docker, read-only
  mount and typing-fixture cases of this container, not qualification
  failures) except the cat splice test that `5d2f6e63b` repairs; the unpatched
  build cannot finish that run, because a wedged test thread aborts it after
  3 min. `c5fd576e1` splits the 106 s numfmt check that remained. The long
  poles are `test_gnu_sort_sort_buffer_size_log` (676 s) and
  `test_gnu_sort_sort_compress_hang_log` (395 s), which start late in
  declaration order. Verification-profile builds on the `xsh-test:latest`
  image, not the pinned image; the pairs are not requalified.
- **Unrun:** private and resource close (`dev/util-linux/check.py
  resources|close`), and every pinned-image stage below.
- **Environment:** the earlier `8ddba911` and `b123366b` images are no longer
  available; a 2026-11-04 rebuild replaced the `xsh-test` tag.
  `versions.lock.json` now pins the local amd64 `Dockerfile.test` image
  `sha256:822035d36ef6da33bdd09a2f2a8b1d8b11c544b7bf0376e8327790c680599d2b`,
  built from the unchanged Dockerfile with Rust `nightly-2026-09-15`
  (`rustc 1.100.0-nightly 2026-09-14`). Earlier image measurements and
  receipts do not qualify new runs.
  The launch, observer and comparison tools keep
  their controller root, locks and evidence under the main checkout's `.work/`,
  derived from the git common directory, so every lane worktree shares one set
  of locks; that is approved, not relocated.
- **Limiting stage:** current-source acceptance and the remaining XSH-1, XSH-3
  and XSH-19 delivery on the re-pinned image, not a shortage of lanes. Steer
  again after the first accepted wave or two failed attempts.
- **Laputa world:** the isolated world build, store verification, rooted
  functionality and unchanged rebuild passed with the R13 seed (85 artifacts
  reused offline). Actual KVM smoke passes with `ASTERINAS_PVH_BOOT_OK`. Full
  Asterinas guest acceptance is open. The run with the R13 seed stopped at 159
  passes, one failure and four skips. The failing AWK case requires
  `systime() > 1000000000`. RTC device failures and the pinned kernel's epoch
  fallback suggest a clock cause, which is unproven.
- **Inference:** the accepted capabilities are merged on `master`. Its
  residuals are XSH-61, XSH-65 and XSH-66 in [TODO](../TODO.md).
- **Lean:** H3 maintenance passed on the unchanged Lean 4.35 project. See
  [the Lean campaign](../xsh-lean.md).

## Retained inputs

The `.work` checkouts below are disposable. Recreate any of them from its
revision with `git worktree add`.

| Input | Revision | Use |
|---|---|---|
| `backlog-finish-lint` | `97e361171` | Candidate branch of XSH-14; its release pair was rebuilt on 2026-11-01 from `25639900b` and `97e361171` |
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
| G1 close the inherited acceptance queue | XSH-2 closed on historical canonical and observer evidence. Run the finite current-source queue (below) with zero unaccounted failures or skips. Final qualification needs G2's functional validator. |
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
| Reopen the Deser cutover performance thresholds for Linux qualification; the earlier waiver is not a pass. | [ROADMAP](ROADMAP.md) |

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
python3 -B dev/consolidation/winddown/gate-tools/final-correctness-gates/generate_manifest.py --output "$PWD/.work/tmp/supplemental-plan.json"
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

- 2026-11-04: The primitive serialization-handle prototype `fb3bc1712` did not demonstrate a CPU benefit. Five alternating pinned Linux O2 pairs with readiness checks and exact outputs measured decode-plus-ten-encodes at 180.202 ms median CPU (177.424–184.230) versus 178.852 ms (174.341–182.552), paired ratio 0.993 (0.967–1.017). Roundtrip and constructed-value ratios were 1.028 (0.982–1.061) and 0.997 (0.954–1.030). Decode, JSONL, small/pretty, and checker controls overlap in spread. The 44 focused codec checks pass, and matched `xsh`/`xsht` products shrink 39,528/51,584 bytes, but this candidate does not resolve the historical roundtrip regression or the two-family target. Keep it off `master`; the parked branch retains the experiment. Earlier profiles identified serializer dispatch as a cost; no reduction in that cost is established by this neutral timing comparison.
- 2026-11-04: Selecting concrete private field emitters once per container did not improve substantial workloads after the event-guard and span refactors. Five alternating, readiness-qualified pinned Linux O2 pairs with exact outputs measured one decode plus ten encodes at 177.304 ms median CPU (174.891–183.322) versus 175.403 ms (172.491–176.373), paired ratio 0.983 (0.962–1.003). Roundtrip and constructed-record differences also fit spread. RSS and unaffected controls fit budgets, but `xsh`/`xsht` grew 35,984/25,600 bytes without demonstrated benefit. The typed-emitter prototype and its added test were reverted; no speedup is claimed.
- 2026-11-04: Borrowed serialization event guards preserve XSH's encoding-limit policy before transforming layers. Five alternating, readiness-qualified pinned Linux O2 pairs against `ad126a1e8` performed one decode and ten exact-checked encodes of the same 20,000-record JSON: median CPU fell from 192.750 ms (191.421–196.953) to 184.635 ms (181.151–187.308), with paired ratio 0.951 (0.946–0.970). Roundtrip and constructed-value differences remain within spread. Decode, JSONL, small/pretty and checker controls fit their budgets; RSS fits, and `xsh`/`xsht` grow 0.339%/0.283%. The 35 Deser serialization, 43 XSH host codec and 59 native codec checks pass, including the permission case run unprivileged. Independent review found no defect. The historical roundtrip regression, 15% two-family target and matched build-cost qualification remain open.
- 2026-11-04: Five alternating, readiness-qualified pinned Linux O2 pairs compared pre-Deser `e1b896dc` with `ad126a1e8` on the same 20,000-record JSON. Decode median CPU was 52.556 ms (51.360–55.092) versus 55.327 ms (54.131–57.524), within paired spread. Roundtrip was 59.065 ms (58.559–59.667) versus 69.813 ms (67.172–70.258), above the 5% budget and spread. Products differ beyond the codec. Small-record text vectors, direct compact-name accumulation, and inlining the decoder sink did not improve the large workloads and were reverted.
- 2026-11-04: With the same products and fixture, five alternating pairs performed ten exact-checked encodes after one decode: median CPU was 137.240 ms (131.295–138.909) versus 190.683 ms (190.181–201.140). Constructing the same records in XSH before ten encodes took 232.396 ms (223.499–241.000) versus 280.642 ms (276.927–298.981). Both include setup. Profiles identify serializer event dispatch and field iteration as prominent current costs, but do not attribute the historical difference to one change. A controlled use of Deser’s private no-finish hook was reverted: five paired decoded-value encode samples overlapped in spread, although constructed-value samples improved. It is not a supported integration interface.
- 2026-11-04: Integrated the XZ workspace repair. Rebuilt XZ candidate `91f319204` through the configured mirror after publishing upstream `liblzma` and `liblzma-sys` 0.4.9. Embedded registry paths changed the earlier direct-registry products, so timing was repeated. The matched `ad126a1e8` baseline rebuild was byte-identical to its earlier products. Both use pinned Linux, O2 release, musl and tracked registry configuration. Each of five alternating pairs passed readiness checks before and after measurement and reproduced exact 32 MiB outputs. Source-text median CPU fell from 173.178 ms (169.413–174.238) to 122.761 ms (119.632–125.760). Incompressible-data CPU fell from 60.766 ms (58.544–61.095) to 28.967 ms (26.752–29.547). Startup medians for 200 processes were 173.860 and 174.775 ms CPU, with ranges 171.029–187.982 and 167.187–186.554: no difference above spread. Untimed peak-RSS medians fell from 34,276 to 33,800 KiB for source text and 34,272 to 33,780 KiB for incompressible data. Release `xsh` shrinks 78,320 bytes and `xsht` 87,864 bytes. The three previously skipped compression-oracle tests pass without skips. Together with the earlier 52 passes, all 55 compression cases are accounted for across two runs. Cold workspace build cost, S6 and final integrated-source qualification remain open.
- 2026-11-04: Five alternating clean-target, warm-kache `xsh-archive` build pairs (pinned Linux, four CPUs, readiness brackets) measured median container CPU of 279.547 ms (274.901–280.298) for `ad126a1e8` and 296.926 ms (295.608–298.616) for XZ candidate `808d82aa4`: a 6.2% increase above spread. Both arms were warmed and every artifact was checked. This is cached archive-crate cost, not a cold workspace build. The candidate also passed 52 core compression tests; three comparisons skipped because their separate Docker-oracle context was unavailable.
- 2026-11-04: Unmerged XZ candidate `91f319204` passes 74 archive Rust tests and 17 native tests on pinned Linux. Against `ad126a1e8`, five readiness-qualified alternating O2 pairs decoded exact 32 MiB outputs. Median process CPU for source-like text fell from 173.568 ms (169.522–173.651) to 123.717 ms (121.213–124.047); deterministic incompressible data fell from 60.260 ms (59.501–61.249) to 29.575 ms (28.722–30.077). The warm-filesystem path includes opening, decoding and publication; separate profiles identify decoding, CRC64 and copying as the dominant work. Five startup pairs of 200 fresh processes showed no difference above spread: 171.967 ms CPU (168.638–173.556) versus 170.742 ms (167.921–187.983). Untimed memory checks found no growth; release `xsh` and `xsht` shrink 77,888 and 87,080 bytes. Build CPU cost and configured-registry availability remain open. This does not close S6 or qualify final integrated source.
- 2026-11-04: The frozen pre-Deser reference `d68814446` completed canonical Linux verification in the pinned image. Rust and XSHT groups passed; native tests reported 14,423 passed, 58 failed and 151 skipped. Counterexamples include older generic producer proofs, incorrect stream error annotations and system-report path serialization. Five semantic cases replayed successfully on current migrated source. The reference repair lane must close its remaining failures before a new full replay; neither this failed run nor historical passes qualify final integrated source.
- 2026-11-04: Rejected an `#[inline]` hint on the borrowed JSON encoder against `ad126a1e8`. Five alternating pinned Linux O2 pairs, with readiness checks before and after and identical output, measured median CPU for ten encodes of the 20,000-record value at 195.956 ms (195.778–197.750) before and 200.983 ms (200.108–202.508) after. The 2.6% regression exceeds the observed spread; the hint was removed. A first batch overlapping a history search was discarded.
- 2026-11-03: `cf8e2cd40` skips the source-wide list-join scan when a function has no empty list to refine. Against `085a35e79`, five alternating matched O2 pairs in the pinned Linux image passed both readiness checks and retained all 107 lint diagnostics. Median CPU for the inferred-return lint on `core/sort.xsh` fell from 31.548 s to 25.690 s (paired ratio 0.818, range 0.806–0.832); `xsht check core/sort.xsh` fell from 264.0 ms to 206.5 ms (ratio 0.787, range 0.777–0.795). Peak RSS was 199,260 versus 199,156 KiB in one untimed pair, and `xsht` shrank 2,784 bytes. These results qualify this workload and change, not the remaining G1 gates or the broader XSH-14 import-graph work.
- 2026-11-03: Matched O2 products from pre-Deser production `9d893e007` with qualification-only harness repairs and current `085a35e79` reproduced the same 20,000-record JSON output. Five alternating pairs with readiness checks measured median CPU of 59.949 versus 93.745 ms for the roundtrip and 51.029 versus 77.153 ms for decode alone. JSONL improved from 34.565 to 26.988 ms. These sources differ in more than the codec, so the ancestry comparison does not attribute the entire change to Deser. S6 time gates and current-source qualification remain open.
- 2026-10-17: Launch source `c6d36683872e4d0dc9173a9c2c8ff09e8e6b9d23`. KVM access through `doas` works, so sandbox visibility is not a platform limit. G2 may extend `check-current --review-evidence` to call the existing review verifier while preserving default checks and strict `final-close`.
- 2026-10-18: The scheduler bridge derives the shared namespace from the common Git directory, using the unchanged launcher from its reviewed tool checkout. Compiler qualification stays with R13.
- 2026-10-24: Paused at the owner's request. Completed slices were integrated, three incomplete branches parked, and 312 checkouts reclaimed.
- 2026-10-25: Paused on `master`. Lint and lsmem source integrated, obsolete lane snapshots committed and their checkouts retired, full qualification explicitly pending.
- 2026-10-28: G0 defers this campaign's performance obligations. Inference evidence is maintained in [Inference evidence](PERFORMANCE-INFERENCE.md).
- 2026-10-30: Resumed on `perf/g1-next`. Did not repin the qualification image or write under the main checkout's `.work/`, since either changes the approved environment or the lane's write boundary; both wait for the owner. Attributed the scheduler-pair timeouts to fork under the shared address-space lock and fixed them at the spawn (`0836935e3`). Timing gates were not run: the lane's own suites kept the host busy, so perf gate deferred: host busy.
- 2026-11-01: The owner reported a quiet host and released the CPU acceptance deferred above. Every pair below ran only after `perf-gate-ready` exited 0 and was kept only if `perf-gate-ready -1` exited 0 right after; an earlier set of lint pairs overlapped this lane's own build and was discarded and rerun. Release builds (`cargo build --release --locked`, `opt-level = 2`, thin LTO, x86_64 musl host) of `25639900b` (baseline, the candidate's parent) and `97e361171`; each pair ran both arms with alternating order; CPU is user plus system time of the whole process tree; outputs were identical across arms. Lint candidate, five pairs per workload, none discarded, on a frozen checkout of `25639900b`: six annotation-heavy files (`system_report_live`, `awk`, `sort`, `locks`, `storage`, `ls`) 25.75 s baseline median (25.05 to 26.21) against 25.70 s (24.22 to 26.37), paired ratio median 0.984 (0.962 to 1.016); `xsht lint core` 57.29 s (54.32 to 57.56) against 55.33 s (52.12 to 57.30), ratio 0.996 (0.960 to 0.997); `xsht lint` over the 1,664-file repository 73.75 s (71.50 to 76.93) against 71.74 s (70.63 to 85.99), ratio 1.003 (0.952 to 1.118). Peak RSS 864,128 against 855,576 KiB (six files) and 1,195,136 against 1,193,696 KiB (`core`). Release `xsht` grows 86,080 bytes (0.15%); release `xsh` shrinks 14,880 bytes. Verdict: no regression above the budget and no benefit above the spread, so the 15% target is unmet and no speedup is claimed; whether a 700-line change without a measured benefit stays is the owner's call (XSH-14). Check time of the inference and verifier commits since `550fb436a` (master `e2c2e4736`), ten pairs, 15 discarded for the gate: `xsht check core` (928 files) 274.83 s baseline median (264.70 to 287.75) against 271.55 s (262.64 to 284.77), ratio 0.991 (0.958 to 1.018); ten runs of `xsht check core/sort.xsh` 2.675 s against 2.659 s, ratio 0.996 (0.909 to 1.023); diagnostics identical; peak RSS 2,920,912 against 2,952,840 KiB. Release `xsh` and `xsht` are 39,488 and 40,392 bytes smaller. No other item the log marks as deferred for a busy host (the G1 pinned-image observers, forced frames, comparative profile and scheduler pairs) was run, because they need the re-pinned qualification image and a frozen qualified source.
- 2026-11-01: `core/sort.xsh` throughput (O3). Perf profiles of `sort -n`, `-k` and `-S` runs showed three costs: a `for ... in range(a, b)` filled all its items before the first was read (the chunk scanner paid a 65,536-item fill per record), every call through a function naming an imported module rebuilt that module's namespace, and each key byte went through a helper call. Three commits, each gated with `cargo dev gate` on its paths (the first two also the new runtime tests, which fail on the old build), cut median CPU over five interleaved pairs with readiness brackets and identical output in every arm: `sort -n` over 10,000 lines 7.75 s to 3.81 s, `sort -S 1k` over 10,000 lines (spill and merge) 5.84 s to 2.52 s, `sort --parallel=1 -k2,2` over 2,000 lines 2.80 s to 1.62 s. The two long-pole GNU tests, one run each, old and new side by side with readiness brackets passed: `test_gnu_sort_sort_buffer_size_log` 365.4 s to 170.3 s CPU and `test_gnu_sort_sort_compress_hang_log` 157.9 s to 70.6 s (the 676 s and 395 s above came from a 32-worker run on a loaded host). What remains is interpreter cost: about 4 microseconds per function call, 22 to 25% of samples in `memcpy` of 336 to 400 byte frame records (`FrameContinuation` and `FrameWork`), and per-record reparsing of key specs and numeric prefixes in the script. A copy of `sort.xsh` with its 298 private parameter annotations erased gives the same output at 2.29 times the CPU of the annotated source (paired ratio range 2.26 to 2.31, five pairs, `sort -n` over 2,000 lines, fixed build); its profile has the same flat shape and shows no module-namespace hydration, so the erased gap is not one of the paths fixed here and is left to the inference campaign. The newest master build rejects that erased copy (`concrete guard call retains a symbolic root type`).
