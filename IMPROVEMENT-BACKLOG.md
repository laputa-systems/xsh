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
  Close performance experiments with workload, baseline, parity, raw samples,
  memory evidence, and a keep/revert decision. Distinguish macOS from the
  pinned `Dockerfile.test` Linux route.
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

1. **B01:** complete the owner-run `xsht lint` tooling row on both hosts.
2. **E01–E02:** finish the formatter fixture and migrate layout families only
   when their policy changes.
3. **F09:** identify the intermittent inherited descriptor from the expanded
   helper report before changing socket or spawn behavior.
4. **J07, J11, J14, last:** finish compatibility and installer reintegration
   after the checked-out ARM64 profile build and QEMU proof.

## A. Correctness and test evidence

- **A02 · P1.** Use an owner-run `cargo dev coverage` report to select one real interactive or Linux workflow gap at a time; require an observable transition or invariant, not a branch-only test (`docs/COVERAGE.md`). The command-position editor gap now has a real-loop test. The coverage workflow runs unfiltered `cargo test`, including formatter and linter tests that agents cannot invoke.

## B. Embedded standard library and acceptance performance

`bench/stdlib-port/README.md` names the B0 baseline, the 18/24 macOS and
18/30 Linux failures, the fixed gate formulas, and the raw result files. This
section is the only remaining port task list.

- **B01 · P1.** Complete the owner-run `xsht lint` row in `bench/stdlib-port/tooling.py` on macOS and pinned Linux. `results-b01-tooling.json` records matched B0/B1/candidate release measurements, exact parity, and raw samples for `xsht api` and `xsht check` on both hosts. The candidate passes both rows; B1's macOS `xsht check` regression is visible. `AGENTS.md` forbids agents from running linters.

## E. `xsht`, diagnostics, and source fidelity

- **E01 · P1.** Finish the named/splice argument, stage-block, multiline-`?`, nested-control, and comment cases in `tests/fixtures/fmt/beauty.xsh` and its golden (`docs/XSHT-FMT.md`); include CST-backed cases for comments beside delimiters, authored blank lines, and `fmt: skip` with trailing comments. Output must reparse and remain idempotent.
- **E02 · P2.** Migrate one formatter construct family to `Doc`/`DocRenderer` when its layout changes; verify the beauty fixture and syntax gate rather than rewriting the formatter wholesale.
- **E04 · P1.** Complete owner-run `xsht lint` and `xsht fmt --check` parity on script-backed standard calls in static and dynamically loaded user modules. `tests/runtime/run.rs::copied_products_check_and_run_script_backed_calls_in_static_and_loaded_modules` now verifies copied `xsht check` and `xsh` execution without repository files on macOS and pinned Linux. Agents cannot invoke linters or formatters under `AGENTS.md`.
- **E07 · P2.** Improve `xsht` cold single-file check latency only after the paired tooling route attributes its preparation cost; retain checker equivalence with the runner.

## F. Linux, host operations, and network

- **F09 · P1/M.** Reproduce an intermittent inherited descriptor in `runtime::modules::native_xsh_net_runtime_descriptors_do_not_survive_exec` on macOS. One full filtered runtime gate reported fd 9 in the child; the exact case, later full gates, and 30 isolated reruns passed. `xsh-test-show-fds` now reports the inherited fd kind and socket ports. On Apple targets, `crates/xsh-net/src/lib.rs::async_connect_resolved_tcp` sets close-on-exec after socket creation, and `crates/xsh-net/src/runtime.rs::FileLane::submit` sets it after `UnixStream::pair`; either interval could overlap a spawn, but neither is yet identified as fd 9. Use the helper's next failure to identify the descriptor and make a deterministic fixture before changing socket or process-spawn policy.

## J. Reintegration with `../packages` and `../laputa` — final phase

These are downstream consumers of XSH and have their own `AGENTS.md` contracts.
Use read-only and static checks first; package builds, named Docker volumes,
QEMU, installer images, release artifacts, and publication are later gates.
Keep PM policy in `../packages` and image/QEMU policy in `../laputa`.

The checked-out macOS route uses `target/debug/{xsh,xshi,xsht}` with default
`native-tests net tools` features; the local Linux debug route uses the pinned
`xsh-test` ARM64 musl image with explicit `xsh/native-tests xsh/net xsh/tools
xsht/native-tests`. The package scratch image and Laputa package-tools image
still pin published `release-d09c6c3305ab8c650043bd8d32e03f2db6509e97`,
built with `net tools`; Laputa's host Makefile uses checked-out debug `xsh`.
The PM static check, Laputa strict check and native modules, and combined PM/
Laputa import test pass with the local build. Separate file identities and
captured `args` shadowing are covered in XSH native tests. Package local-binary
targets now select the owning Cargo packages and matching Linux flags. The local
Linux route passes 114 PM tests including the filesystem test. The published pin's
first PM suite runs 20/23 tests and fails three tagged-constructor cases; it is
not a passing comparison baseline for today's PM source. Laputa's local ARM64
plan writes byte-identical BuildPlan and generation-plan JSON across two runs;
the pinned runtime cannot load the new typed generation boundary.
The release workflow emits `xsh`, `xshi`, and `xsht` for both Linux musl
architectures with matching SHA-256 sidecars, plus a shared `core` archive.
The PM recipe and Laputa updater use those exact names; PM strips the archive's
single `core/` directory before installing scripts. Pins remain unchanged until
a compatible release is published. The installer QEMU harness now uses the
existing typed `stderr` command field.

The checked-out `qemu-dwl-foot` route built all 47 plan nodes in the pinned
native ARM64 Docker environment and atomically published a bundle with 35
runtime artifacts. `laputa.xsh -- test qemu-dwl-foot` then passed QMP readiness,
input, and screenshot checks; `console.log` contains
`LAPUTA_DWL_FOOT_PROOF_OK`, with no panic or failure marker, and the screenshot
is nonempty. The tested BuildPlan SHA-256 is
`01e778e8bfa12b08acc7b89b9bd14190e707324ec8a4876e62cb0891eef696ff`.
The independent `make installer-image-aarch64` route currently fails at its
first package installation: `build-installer-image.xsh::install_remote_packages`
calls `pm install`, which was removed from the final typed CLI. Its sequential
mutable root installation and local overlay need migration to saved plans,
verified artifacts, and immutable composition before installer QEMU can run.
A read-only PM plan for the eight installer package roots succeeds, but all 24
nodes select local builds because the checked-out releases exceed the mirror;
the old remote-only install sequence cannot be replaced by a download loop.

- **J07 · P1.** When a published binary compatible with the current PM source is available, compare PM plan JSON, canonical fingerprints, store receipt validation, and root-generation outputs across the two binaries on deterministic fixtures. Executor binary identity is part of a BuildPlan, so classify expected key changes separately from semantic drift.
- **J11 · P2.** Migrate the independent installer image builder from `pm install` to saved BuildPlans, verified artifacts, and immutable root composition; preserve its distinct installer and target overlays. Then run installer image and QEMU tests without routing them through `qemu-dwl-foot` policy.
- **J14 · P2.** Finish with a cross-repository compatibility report: exact commands, profiles, fixtures, observed differences, remaining failures, and reviewable changes in each owner's repository.

## Deliberately outside this queue

The explicit deferrals in `docs/SPEC-INTERACTIVE.md` remain deferrals: POSIX
compatibility, multi-job control, shell functions/arithmetic/arrays, process
substitution, `~user`, and persisted session mutations. `docs/SHOWCASE.md` also
rejects a generic plugin/package framework, application-runtime features, and
ports selected only for recognizability. They need a separate contract decision
supported by multiple real programs, not an unchecked backlog item.
