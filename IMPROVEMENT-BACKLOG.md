# XSH improvement backlog

This is the cross-project queue for improving XSH. It is intentionally broad,
but every entry points to an existing contract, code owner, test gap, measured
failure, or documented design pressure. It is a work queue, not a second
specification. `docs/SPEC.md`, the focused `docs/` contracts, `xsht api`, and
`docs/TEST-MAP.md` remain authoritative. Benchmark methods and acceptance
evidence live in `bench/stdlib-port/README.md` and its result files. The
default outcome is a smaller, clearer implementation of the existing language:
fix defects, remove duplication, narrow contracts, and improve measured
behavior. New syntax or
public APIs require a separate decision backed by repeated real use.

## How to work this queue

- **P0**: a known failing, ignored, or missing correctness boundary. **P1**:
  observed performance, reliability, or usability cost. **P2**: a bounded
  experiment or corpus investment. `M` means measure or reproduce before
  implementation; it is not a commitment to a proposed mechanism.
- Take one contract-sized item at a time. Record the baseline, the smallest
  regression test or measurement, the implementation, the focused check, and
  the full relevant gate from `docs/TEST-MAP.md`. Update the exact owner doc.
  Keep raw samples for performance claims and distinguish macOS from the pinned
  `Dockerfile.test` Linux route.
- Close an item with evidence, a measured rejection, or a precise reason it is
  blocked. Do not mark it done because code exists or a test was skipped.
  Re-rank after each batch; a newly found correctness defect outranks this list.
- Work through ready items continuously in small batches. When one item is
  blocked by an external runner or a contract decision, record the blocker and
  continue with an independent item. Prune tasks that measurements disprove;
  the target is a better XSH, not a larger completed checklist.
- Preserve the language's systems-orchestration focus. The corpus-first and
  three-strike rules in `docs/SHOWCASE.md` govern any proposed language growth;
  this queue assumes a language-feature moratorium. Do not
  add dependencies without asking the owner. Do not run formatters, autofixers,
  pre-commit hooks, or push; `AGENTS.md` owns those workflow limits.

## First execution sequence

1. **B01:** complete the paired `xsht` tooling evidence, then use the recorded
   B0 `xsh` failures to prioritize profiling on both supported hosts.
2. **B08, C01–C04, C06:** improve the measured hot paths and cold preparation in
   small batches, preserving the original B0 gates and native controls.
3. **E01–E03:** make the tooling and Linux test
   boundaries deterministic enough to support sustained work.
4. **H07–H09:** refine existing `dev/`, `core/`, and `showcase/` programs; let
   repeated friction select small runtime or tooling changes.
5. **J01–J14, last:** reintegrate the stable XSH build with `../packages` and
   `../laputa`, progressing from static checks to isolated Linux tests and QEMU.

## A. Correctness and test evidence

- **A01 · P1.** Continue the native/Rust ownership audit beyond the completed stream module and the `fs.walk` value checks now in `tests/xsh/stdlib/fs.xsh`; move language assertions to disk-backed native tests when the host harness adds no boundary.
- **A02 · P1.** Use `cargo dev coverage` to select one real interactive or Linux workflow gap at a time; require an observable transition or invariant, not a branch-only test (`docs/COVERAGE.md`).
- **A04 · P1/M.** Resolve the `reduce-by --jobs` execution mismatch and adjacent fusion policy. `FullStageTag::ReduceBy` is serial; an explicit `--jobs` runs once, validates a positive count, and prevents adjacent `par-map |> reduce-by` fusion. Fused workers now share the ordinary reducer's projected-record fast path. The paired release samples in `bench/stream-fusion-2026-09-24.json` show the 50,000-row fused case improving from about 284 to 99 ms while retaining a large memory advantage over the unfused path. Fusion remains slower on the default `showcase/tokei.xsh` workload over `src` and this repository, while `../packages` is near parity. Keep `--jobs` as an escape hatch; decide when fusion should engage after measuring a file corpus large enough to expose its memory tradeoff. Do not add reduce workers without deterministic error and effect semantics.

## B. Embedded standard library and acceptance performance

`bench/stdlib-port/README.md` names the B0 baseline, the 18/24 macOS and
18/30 Linux failures, the fixed gate formulas, and the raw result files. This
section is the only remaining port task list.

- **B01 · P1.** Complete the owner-run `xsht lint` row in `bench/stdlib-port/tooling.py` on macOS and pinned Linux. `results-b01-tooling.json` records matched B0/B1/candidate release measurements, exact parity, and raw samples for `xsht api` and `xsht check` on both hosts. The candidate passes both rows; B1's macOS `xsht check` regression is visible. `AGENTS.md` forbids agents from running linters.
- **B08 · P1/M.** Investigate bounded call, stage, argument, block, and frame overhead in the existing indexed runtime; keep the explicit-frame small-stack route, Result propagation, traces, and private bridge authority intact.

## C. Runtime, streams, and memory

`docs/STREAMS.md` lists the remaining levers and records which earlier
experiments lost. Every item here starts with a complete workload and
allocation/RSS evidence; no new execution engine is implied.

- **C01 · P1/M.** Measure per-item stage dispatch on serial and worker paths; preserve lazy short-circuit, effects, and tracing before considering a verified block specialization.
- **C02 · P1/M.** Profile `fs.walk` record construction when `where` rejects most entries; test whether lazy fields save measurable work without changing metadata-error timing. The `src` all-rejected walk allocated 811 KB with `stat: true` versus 101 KB with `stat: false`, but the latter skips metadata and cannot preserve the contract (`bench/fs-walk-rejection-c02-2026-09-24.json`). The probe also exposed a dynamic-boolean lowering bug, now covered in `tests/xsh/stdlib/fs.xsh`.
- **C03 · P2/M.** Profile scope-key allocation and hashing in repeated bindings; compare a narrow key-reuse change with the wider `Arc<str>` ownership change before choosing either.
- **C04 · P2/M.** Measure large flat directories before revisiting intra-directory work splitting; the earlier fused parallel walk lost on that shape (`docs/STREAMS.md`).
- **C06 · P1/M.** Identify whole-buffer scanners that can use line-state APIs; convert one real large-file workload at a time and preserve malformed-late-row behavior. The `showcase/loc.xsh` `Path.lines() |> count()` candidate lost to `read_text()?.count_lines()` on `src` (33.016 versus 22.883 ms median in 20 paired release runs), despite 1.8 MB lower median peak RSS, and changed the late invalid-UTF-8 diagnostic path, so it was rejected (`bench/stream-line-count-c06-2026-09-24.json`).

## E. `xsht`, diagnostics, and source fidelity

- **E01 · P1.** Finish the named/splice argument, stage-block, multiline-`?`, nested-control, and comment cases in `tests/fixtures/fmt/beauty.xsh` and its golden (`docs/XSHT-FMT.md`); include CST-backed cases for comments beside delimiters, authored blank lines, and `fmt: skip` with trailing comments. Output must reparse and remain idempotent.
- **E02 · P2.** Migrate one formatter construct family to `Doc`/`DocRenderer` when its layout changes; verify the beauty fixture and syntax gate rather than rewriting the formatter wholesale.
- **E03 · P2/M.** Measure actual source files with tabs, wide Unicode, or combining marks before changing display-column accounting; add boundary fixtures if the current character-count policy misformats them.
- **E04 · P1.** Keep `xsht check`, `lint`, `fmt --check`, and execution in agreement on script-backed standard calls, including loaded user modules and copied binaries (`src/loader.rs`).
- **E07 · P2.** Improve `xsht` cold single-file check latency only after the paired tooling route attributes its preparation cost; retain checker equivalence with the runner.

## F. Linux, host operations, and network

- **F05 · P2.** Audit host error kind and byte preservation at file, path, environment, and process boundaries using native XSH tests for semantics and Rust tests for exact OS bytes. The three product CLIs now reject non-UTF-8 argv with status 2 instead of panicking; other host boundaries remain to review.
- **F06 · P2.** Check Linux real-mode tests against the feature/profile matrix in `dev/targets.xsh`; a skipped privileged test must say which capability or fixture is missing.
- **F07 · P2/M.** Run syscall diagnostics for representative core commands and stream pipelines in the approved container before changing host adapters (`docs/BENCHMARKING.md`).
- **F08 · P2.** Document isolation and cleanup guarantees beside every new test that mutates mount or kernel state, as required by `docs/COVERAGE.md`.
- **F09 · P1/M.** Reproduce an intermittent inherited descriptor in `runtime::modules::native_xsh_net_runtime_descriptors_do_not_survive_exec` on macOS. One full filtered runtime gate reported fd 9 in the child; the exact case, a subsequent full gate, and 30 isolated reruns passed. Identify the descriptor and the creation/exec overlap with a deterministic fixture before changing socket or process-spawn policy.

## G. Build, CI, release, and repository hygiene

The proposed Markdown-link checker was pruned after an inventory found only
two repository-local links across 27 Markdown files, both in
`docs/FRONTEND.md` and both valid. A new checker and check-stage dependency
would cost more than the current link surface warrants; revisit if local links
become a regular documentation convention.

- **G07 · P2.** Compare distribution packaging on the declared target matrix through `dev/targets.xsh` and the release workflow; keep `dist` for CI packaging, not routine agent verification.

## H. Refine the existing systems corpus

`dev/`, `core/`, and `showcase/` already provide substantial XSH programs.
Improve those programs before proposing more. `docs/SHOWCASE.md` supplies the
selection and completion standards; the sibling package manager already owns
root composition, so a second composer here would be duplication.
The existing `showcase/*.xsh` programs are single-file tools, so none passes
the canonical corpus selection test's multi-module requirement. Review their
real failure boundaries without inflating them to satisfy that milestone.

- **H07 · P2.** Review `core/` applets as a set for duplicated argument parsing, error presentation, and host-boundary helpers; consolidate only repeated, stable XSH code.
- **H08 · P2.** Keep `showcase/jq.xsh` as the documented negative control; use any measured defect it reveals to improve the existing runtime without turning XSH into a general data language.
- **H09 · P2.** Record friction found across H03–H08 as program design, reusable XSH helper, diagnostic/tooling issue, or narrow host capability; add no new semantic category from one script.

## I. Documentation and contract upkeep

- **I01 · P1.** For every behavior change, update the canonical owner (`docs/SPEC.md`, `SPEC-OS.md`, `STREAMS.md`, `JSON.md`, or `xsht api`) in the same change; keep this queue at task level.
- **I02 · P2.** Keep architecture and roadmap descriptions consistent when implemented work closes an item; prefer removing stale future-tense prose to adding another guide.
- **I03 · P2.** Keep `docs/TEST-MAP.md` aligned with the commands that actually execute product binaries and platform fixtures; remove stale or ambiguous gates when found.
- **I04 · P2.** Close each performance experiment with workload, baseline, parity, raw samples, memory evidence, and a keep/revert decision in its benchmark owner rather than accumulating another narrative ledger.

## J. Reintegration with `../packages` and `../laputa` — final phase

These are downstream consumers of XSH and have their own `AGENTS.md` contracts.
Begin only after the XSH correctness and performance work above is stable.
Use read-only and static checks first; package builds, named Docker volumes,
QEMU, installer images, release artifacts, and publication are later gates.
Keep PM policy in `../packages` and image/QEMU policy in `../laputa`.

- **J01 · P1.** Inventory which checked-out or published `xsh`/`xshi`/`xsht` binaries each downstream route uses (`../packages/Makefile`, `../laputa/Makefile`, and `../laputa/update-xsh.xsh`); record exact versions and feature sets before comparing results.
- **J02 · P1.** Run `xsht check` on `../packages/pm.xsh` and `pm/*.xsh` with a matching local debug product; separate checker errors from the Linux runtime and fixture work in J06.
- **J03 · P1.** Check `../laputa/laputa.xsh`, `laputa/*.xsh`, and its native tests with `xsht check --strict`; keep installer modules in their separate route.
- **J04 · P1.** Test global exported-name and module-path collisions across both consumers; `../laputa/AGENTS.md` calls out the shared runtime symbol table explicitly.
- **J05 · P1.** Verify the `../packages/Makefile` local-binary build targets against current workspace package ownership; `xsh`, `xshi`, and `xsht` are owned by distinct packages in this tree.
- **J06 · P1.** Run the PM suite against local XSH in the approved Linux container, preserving `../packages/AGENTS.md` source mounts and scratch isolation; compare with its pinned published-release baseline.
- **J07 · P1.** Compare PM plan JSON, canonical fingerprints, store receipt validation, and root-generation outputs across the two binaries on deterministic fixtures; a compiler/runtime change must not silently change package identity.
- **J08 · P1.** Run Laputa's typed `qemu-dwl-foot` plan and focused native module tests before invoking its Docker build; compare profile and artifact paths with the existing baseline.
- **J09 · P1.** Run the Laputa native `linux/arm64` Docker build using the checked-out packages graph and named volumes; preserve the container-local staging and atomic final-copy boundary in `../laputa/AGENTS.md`.
- **J10 · P1.** After J09 passes, run QEMU/QMP proof and inspect its recorded markers; distinguish guest boot, package, image, and XSH runtime failures.
- **J11 · P2.** Verify installer image and installer QEMU tests as an independent product route after the core profile passes; do not route installer work through `qemu-dwl-foot` policy.
- **J12 · P1.** Reconcile XSH release artifact names, checksums, and core-script packaging among this repo's release workflow, `../packages/repo/xsh/PKGBUILD.xsh`, and `../laputa/update-xsh.xsh` before changing published pins.
- **J13 · P2/M.** Investigate the two `stderr: /dev/null` TODOs in `../laputa/installer-qemu-test.xsh` against current process APIs; prefer an existing structured redirection before proposing a public API addition.
- **J14 · P2.** Finish with a cross-repository compatibility report: exact commands, profiles, fixtures, observed differences, remaining failures, and reviewable changes in each owner's repository.

## Deliberately outside this queue

The explicit deferrals in `docs/SPEC-INTERACTIVE.md` remain deferrals: POSIX
compatibility, multi-job control, shell functions/arithmetic/arrays, process
substitution, `~user`, and persisted session mutations. `docs/SHOWCASE.md` also
rejects a generic plugin/package framework, application-runtime features, and
ports selected only for recognizability. They need a separate contract decision
supported by multiple real programs, not an unchecked backlog item.
