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
2. **B08, C02:** improve the measured hot paths and cold preparation in
   small batches, preserving the original B0 gates and native controls.
3. **E01–E02:** finish the formatter fixture and migrate layout families only
   when their policy changes.
4. **J01–J14, last:** reintegrate the stable XSH build with `../packages` and
   `../laputa`, progressing from static checks to isolated Linux tests and QEMU.

## A. Correctness and test evidence

- **A02 · P1.** Use an owner-run `cargo dev coverage` report to select one real interactive or Linux workflow gap at a time; require an observable transition or invariant, not a branch-only test (`docs/COVERAGE.md`). The command-position editor gap now has a real-loop test. The coverage workflow runs unfiltered `cargo test`, including formatter and linter tests that agents cannot invoke.

## B. Embedded standard library and acceptance performance

`bench/stdlib-port/README.md` names the B0 baseline, the 18/24 macOS and
18/30 Linux failures, the fixed gate formulas, and the raw result files. This
section is the only remaining port task list.

- **B01 · P1.** Complete the owner-run `xsht lint` row in `bench/stdlib-port/tooling.py` on macOS and pinned Linux. `results-b01-tooling.json` records matched B0/B1/candidate release measurements, exact parity, and raw samples for `xsht api` and `xsht check` on both hosts. The candidate passes both rows; B1's macOS `xsht check` regression is visible. `AGENTS.md` forbids agents from running linters.
- **B08 · P1/M.** Investigate bounded stage, argument, block, and remaining frame overhead in the existing indexed runtime; keep the explicit-frame small-stack route, Result propagation, traces, and private bridge authority intact. For fully supplied non-rest user calls, moving the evaluated argument vector into frame slots removed 99,997 allocations and 3.20 MB of allocation traffic across 100,000 calls, with lower paired release medians on macOS and pinned Linux (`bench/call-slot-ownership-b08-2026-09-25.json`). This one call shape does not establish a general runtime speedup.

## C. Runtime, streams, and memory

`docs/STREAMS.md` lists the remaining levers and records which earlier
experiments lost. Every item here starts with a complete workload and
allocation/RSS evidence; no new execution engine is implied.

- **C02 · P1/M.** Test whether lazy `fs.walk` fields help a real rejection-heavy workload without changing record equality or metadata-error timing. Moving the emitted `ignore::DirEntry` path removed one allocation per entry on a 20,000-file flat walk, but did not establish a throughput or peak-RSS gain (`bench/fs-walk-path-ownership-c02-2026-09-24.json`). `stat: false` skips metadata and remains a non-equivalent upper bound (`bench/fs-walk-rejection-c02-2026-09-24.json`).

## E. `xsht`, diagnostics, and source fidelity

- **E01 · P1.** Finish the named/splice argument, stage-block, multiline-`?`, nested-control, and comment cases in `tests/fixtures/fmt/beauty.xsh` and its golden (`docs/XSHT-FMT.md`); include CST-backed cases for comments beside delimiters, authored blank lines, and `fmt: skip` with trailing comments. Output must reparse and remain idempotent.
- **E02 · P2.** Migrate one formatter construct family to `Doc`/`DocRenderer` when its layout changes; verify the beauty fixture and syntax gate rather than rewriting the formatter wholesale.
- **E04 · P1.** Complete owner-run `xsht lint` and `xsht fmt --check` parity on script-backed standard calls in static and dynamically loaded user modules. `tests/runtime/run.rs::copied_products_check_and_run_script_backed_calls_in_static_and_loaded_modules` now verifies copied `xsht check` and `xsh` execution without repository files on macOS and pinned Linux. Agents cannot invoke linters or formatters under `AGENTS.md`.
- **E07 · P2.** Improve `xsht` cold single-file check latency only after the paired tooling route attributes its preparation cost; retain checker equivalence with the runner.

## F. Linux, host operations, and network

- **F09 · P1/M.** Reproduce an intermittent inherited descriptor in `runtime::modules::native_xsh_net_runtime_descriptors_do_not_survive_exec` on macOS. One full filtered runtime gate reported fd 9 in the child; the exact case, a subsequent full gate, and 30 isolated reruns passed. Identify the descriptor and the creation/exec overlap with a deterministic fixture before changing socket or process-spawn policy.

## G. Build, CI, release, and repository hygiene

The proposed Markdown-link checker was pruned after an inventory found only
two repository-local links across 27 Markdown files, both in
`docs/FRONTEND.md` and both valid. A new checker and check-stage dependency
would cost more than the current link surface warrants; revisit if local links
become a regular documentation convention.

- **G07 · P2.** Compare distribution packaging on the declared target matrix through `dev/targets.xsh` and the release workflow; keep `dist` for CI packaging, not routine agent verification.

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
