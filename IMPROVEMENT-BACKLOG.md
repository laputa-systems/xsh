# XSH improvement backlog

This is the cross-project queue for improving XSH. It is intentionally broad,
but every entry points to an existing contract, code owner, test gap, measured
failure, or documented design pressure. It is a work queue, not a second
specification. `docs/SPEC.md`, the focused `docs/` contracts, `xsht api`, and
`docs/TEST-MAP.md` remain authoritative. The detailed acceptance work for the
standard-library port remains in `STDLIB-PORT.md`. The default outcome is a
smaller, clearer implementation of the existing language: fix defects, remove
duplication, narrow contracts, and improve measured behavior. New syntax or
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

1. **E01, A01–A02:** isolate the observed formatter failure, then make the
   main flaky or Linux-only failures reproducible with focused harnesses.
2. **B01–B05:** make the standard-library performance evidence reproducible on
   both supported hosts before optimizing against it.
3. **B06–B12, C01–C05:** improve the measured hot paths and cold preparation in
   small batches, preserving the original B0 gates and native controls.
4. **D01–D04, E01–E03, F01–F03:** make the interactive, tooling, and Linux test
   boundaries deterministic enough to support sustained work.
5. **H01–H10:** refine existing `dev/`, `core/`, and `showcase/` programs; let
   repeated friction select small runtime or tooling changes.
6. **J01–J14, last:** reintegrate the stable XSH build with `../packages` and
   `../laputa`, progressing from static checks to isolated Linux tests and QEMU.

## A. Correctness and test evidence

- **A01 · P0.** Give the ignored timing cases in `tests/runtime/os.rs` and `tests/runtime/modules.rs` deterministic barriers or controlled clocks where possible; preserve true PTY/OS tests for behavior that needs them, then remove justified ignores.
- **A02 · P0.** Reproduce the Linux corpus failures noted in `STDLIB-PORT.md` against current B0 and HEAD inside `Dockerfile.test`; record exact fixture, host state, and failure identity before changing production code.
- **A03 · P1.** Add a `Record` read-allocation scaling case beside `tests/runtime/collections.rs::map_reads_do_not_copy_the_map`; prove reads do not copy the whole record and keep construction separate from traversal.
- **A04 · P1.** Run `crates/xsht/tests/profile_parity.rs` across each supported feature/profile/platform combination available in CI; make missing binaries explicit skips with a reproducible reason.
- **A05 · P1.** Audit `#[ignore]` and `test.skip` uses by reason and platform; keep a machine-readable count of contract coverage that actually executed, with no blanket unignore of PTY or privileged tests.
- **A06 · P1.** Compare the native XSH and Rust integration suites against the ownership rule in `docs/TEST-MAP.md`; move language assertions to disk-backed native tests when the host harness adds no boundary.
- **A07 · P1.** Use `cargo dev coverage` to select one real interactive or Linux workflow gap at a time; require an observable transition or invariant, not a branch-only test (`docs/COVERAGE.md`).
- **A08 · P1.** Make flaky `xshi` denv source-appearance coverage deterministic (`tests/runtime/interactive.rs`); verify dirty-state and refresh transitions without wall-clock timing.

## B. Embedded standard library and acceptance performance

`STDLIB-PORT.md` names the original B0 baseline, the 18/24 macOS failures, all
five measured Linux R12 failures, and the fixed gate formulas. These items
break that remaining work into reviewable units; the detailed contract stays
there.

- **B01 · P1.** Make `bench/stdlib-port/run.py` preserve independent rounds, raw samples, order, dispersion, skips, and hashes; prove the budget arithmetic with its self-test.
- **B02 · P1.** Add an executable, pinned-container R12 benchmark route with the fixed 200-module input; a `--linux` result must represent real execution and report fixture identity.
- **B03 · P1.** Attach status, stdout/stderr, and side-effect parity evidence to each timed workload (`bench/stdlib-port/parity.py`), including expected nonzero results.
- **B04 · P1.** Put `xsht api summary`, `xsht check core/ls.xsh`, and `xsht lint core/ls.xsh` in a repeatable matched-binary route; preserve their old standalone numbers only as history.
- **B05 · P1.** Measure B0, B1, and candidate in repeated uninstrumented rounds on one host per platform; report each failing row's delta and original budget, with native controls in the same round.
- **B06 · P1/M.** Profile the three CLI batch workloads through `stdlib/cli.xsh` and `src/runtime/eval/lowered_run/indexed_run.rs`; choose a change only after attributing complete-workload cost.
- **B07 · P1/M.** Profile `text_wrap_unicode`, `text_pad_batch`, and `fmt_batch` separately; retain Unicode and ANSI behavior while measuring allocation, per-step dispatch, and scalar-operation costs.
- **B08 · P1/M.** Profile MIME, INI, JSON paths, environment, quoting, checksum, and `core_command` as separate cost shapes; avoid a single fix inferred from their shared gate failure.
- **B09 · P1/M.** Split cold CLI and dynamic-load startup into parse, declaration/body check, dependency discovery, lowering, verification, and execution; optimize the largest measured phase.
- **B10 · P1.** Tighten `src/stdlib.rs::required_modules` only with registry-resolved identity and conservative uncertainty; prove native-only zero-preparation and genuine `module.load` closure semantics.
- **B11 · P1.** Measure R12 acquisition, stream creation, first item, partial consumption, and full parsed rows on Linux; late malformed rows must still error when consumed.
- **B12 · P1/M.** Investigate bounded call, stage, argument, block, and frame overhead in the existing indexed runtime; keep the explicit-frame small-stack route, Result propagation, traces, and private bridge authority intact.
- **B13 · P1.** Qualify `hash.verify_file` over tiny, large, many-small, and error workloads; apply the G02 gate to its workload class, not only the existing large-file example.
- **B14 · P2/M.** Time `linux.rfkill_list` only with its own correct fixture before reconsidering its retained native disposition; the block-device result is not its measurement.
- **B15 · P1.** After each runtime optimization, run the exact stdlib architecture, copied-product, native corpus, API, feature, and Linux gates in `docs/TEST-MAP.md`; close the migration only when cumulative B0 gates pass.

## C. Runtime, streams, and memory

`docs/STREAMS.md` lists the remaining levers and records which earlier
experiments lost. Every item here starts with a complete workload and
allocation/RSS evidence; no new execution engine is implied.

- **C01 · P1/M.** Measure per-item stage dispatch on serial and worker paths; preserve lazy short-circuit, effects, and tracing before considering a verified block specialization.
- **C02 · P1/M.** Profile `fs.walk` record construction when `where` rejects most entries; test whether lazy fields save measurable work without changing metadata-error timing.
- **C03 · P2/M.** Profile scope-key allocation and hashing in repeated bindings; compare a narrow key-reuse change with the wider `Arc<str>` ownership change before choosing either.
- **C04 · P2/M.** Measure large flat directories before revisiting intra-directory work splitting; the earlier fused parallel walk lost on that shape (`docs/STREAMS.md`).
- **C05 · P1.** Add a bounded-memory and early-stop measurement for script producers feeding `par-map` and fused stages; distinguish the one-vector staging cost from observable semantics.
- **C06 · P1/M.** Identify whole-buffer scanners that can use line-state APIs; convert one real large-file workload at a time and preserve malformed-late-row behavior.
- **C07 · P1.** Keep structural allocation tests for list/map accumulation and add alias-preservation cases when storage reuse changes; no mutation may leak to an older value.
- **C08 · P1/M.** Pair `xsh-runtime-stats` worker traffic with host RSS on the same workload; thread-local allocation peaks alone cannot establish process memory improvement (`docs/FRONTEND.md`).
- **C09 · P1/M.** Measure trace-disabled work separately from trace-enabled output on call-heavy scripts; an optimization must preserve error-stack reconstruction and visible trace events.
- **C10 · P2/M.** Add a user-facing `xsh`/`xsht` frontend latency corpus only for workflows with repeated measured cost; `docs/BENCHMARKING.md` currently covers interactive `xshi` workloads.

## D. Interactive `xshi`

The contract is `docs/SPEC-INTERACTIVE.md`; `docs/COVERAGE.md` identifies the
stateful editor and completion harness as the central missing evidence.

- **D01 · P1.** Build a test-only key-event editor harness without a PTY or timing sleeps; cover buffer, cursor, completion, and repaint state transitions.
- **D02 · P1.** Cover ambiguous completion opening, navigation, preview, acceptance, cancellation, and grid clearing after edits through that harness.
- **D03 · P1.** Cover quoted paths, `~/` insertion, hidden paths, directory-only `cd`/`z`, and prefix-vs-substring fallback with deterministic directories.
- **D04 · P1.** Cover history search and autosuggestion as state machines: enter, move, accept, cancel, restore original input, and suppress ghost text in modal states.
- **D05 · P1.** Keep a small PTY gate for TTY startup, raw-mode restoration, Ctrl-C, bracketed paste, and external-command handoff; make each retained ignore's environment requirement explicit.
- **D06 · P1.** Test PATH and directory completion-cache invalidation after `set`, `unset`, denv, cwd changes, and directory mtime changes; no stale candidate may survive a documented refresh point.
- **D07 · P2.** Test remote `ssh` completion timeout, denial, malformed output, and missing program with a fake executable boundary; prompt input must stay responsive and quiet.
- **D08 · P2.** Add terminal-width model cases for wide characters, combining marks, pending wrap, multiline prompts, and narrow completion grids.
- **D09 · P1.** Make single-job background/stop/foreground transitions deterministic at the session state boundary, then retain PTY tests only for terminal process-group behavior.
- **D10 · P2/M.** Re-measure the five complete `xshi` workloads in `docs/BENCHMARKING.md` after editor or completion changes; reject regressions in latency and allocation traffic.
- **D11 · P2.** Audit `xshi` session startup and prompt submissions for embedded-stdlib preparation reuse, failed-input recovery, and program/symbol ownership (`crates/xshi/tests/stdlib_preparation.rs`).

## E. `xsht`, diagnostics, and source fidelity

- **E01 · P0.** Fix the observed formatter idempotence failure in `../packages/pm/cli.xsh`: `return RepoPlan({ ... })` changes layout on the second pass. First isolate the construct in a local fixture, then make the first and second pass identical without changing its parse. `cargo test --test integration syntax::formatter_is_idempotent_on_package_corpus -- --exact` currently fails at `tests/syntax.rs:2927`.
- **E02 · P1.** Finish the named/splice argument, stage-block, multiline-`?`, nested-control, and comment cases in `tests/fixtures/fmt/beauty.xsh` and its golden (`docs/XSHT-FMT.md`); include CST-backed cases for comments beside delimiters, authored blank lines, and `fmt: skip` with trailing comments. Output must reparse and remain idempotent.
- **E03 · P2.** Migrate one formatter construct family to `Doc`/`DocRenderer` when its layout changes; verify the beauty fixture and syntax gate rather than rewriting the formatter wholesale.
- **E04 · P2/M.** Measure actual source files with tabs, wide Unicode, or combining marks before changing display-column accounting; add boundary fixtures if the current character-count policy misformats them.
- **E05 · P1.** Keep `xsht check`, `lint`, `fmt --check`, and execution in agreement on script-backed standard calls, including loaded user modules and copied binaries (`src/loader.rs`).
- **E06 · P1.** Test diagnostic span/source attribution across embedded and user modules on both parse and lowering failures; internal namespace labels must not leak as user-callable names.
- **E07 · P2.** Audit `xsht api` examples against the canonical registry and native tests after API changes; the generated surface fixture remains a gate, not a second hand-edited signature list.
- **E08 · P2.** Improve `xsht` cold single-file check latency only after B04 attributes its preparation cost; retain checker equivalence with the runner.

## F. Linux, host operations, and network

- **F01 · P1.** Build the isolated privileged mount harness described in `docs/COVERAGE.md`, using the pinned `Dockerfile.test` image, private namespaces, temporary roots, and cleanup assertions.
- **F02 · P1.** Add safe mount and switch-root failure-path coverage through that harness; keep real host boot transitions behind dry-run or harness-specific boundaries.
- **F03 · P2.** Extend loop-device and parity tests only on runners that can allocate and release devices reliably; otherwise retain explicit dry-run fixture coverage.
- **F04 · P1.** Prove cancellation and cleanup when process work, a network job, and a stream worker coexist; assert trace parentage and no surviving owned handles.
- **F05 · P1.** Make the local network refill/transfer flakes in `tests/runtime/modules.rs` reproducible with controlled listeners and barriers; distinguish product errors from host overload.
- **F06 · P2.** Audit host error kind and byte preservation at file, path, environment, and process boundaries using native XSH tests for semantics and Rust tests for exact OS bytes.
- **F07 · P2.** Check Linux real-mode tests against the feature/profile matrix in `dev/targets.xsh`; a skipped privileged test must say which capability or fixture is missing.
- **F08 · P2/M.** Run syscall diagnostics for representative core commands and stream pipelines in the approved container before changing host adapters (`docs/BENCHMARKING.md`).
- **F09 · P2.** Document isolation and cleanup guarantees beside every new test that mutates mount or kernel state, as required by `docs/COVERAGE.md`.

## G. Build, CI, release, and repository hygiene

- **G01 · P1.** Add a non-release CI verification path for ordinary changes if the repository's hosting policy permits it; `.github/workflows/release.yml` is currently manual release-only.
- **G02 · P1.** Keep local and CI gates on the same `dev/main.xsh` operations, with explicit target/profile/feature identity and no second shell implementation.
- **G03 · P1.** Verify copied `xsh`, `xshi`, and `xsht` binaries and packaged core scripts from an unrelated directory with no source checkout; retain artifact hashes and surface checks.
- **G04 · P2.** Audit `Makefile` compatibility targets against `dev/main.xsh` callers; remove a duplicate only when the migration path and external consumers are known.
- **G05 · P1.** Keep deterministic fixture identity and regeneration in every benchmark or container test that relies on generated `/proc`, `/sys`, or text inputs.
- **G06 · P2.** Add a documentation-link check limited to repository-owned Markdown targets; the repaired `docs/FRONTEND.md` reference to a removed follow-up file shows the regression this should catch.
- **G07 · P2.** Track the reason, owner, and last attempted gate for ignored tests, so a growing ignore count cannot masquerade as improving coverage.
- **G08 · P2.** Compare distribution packaging on the declared target matrix through `dev/targets.xsh` and the release workflow; keep `dist` for CI packaging, not routine agent verification.

## H. Refine the existing systems corpus

`dev/`, `core/`, and `showcase/` already provide substantial XSH programs.
Improve those programs before proposing more. `docs/SHOWCASE.md` supplies the
selection and completion standards; the sibling package manager already owns
root composition, so a second composer here would be duplication.

- **H01 · P1.** Check the existing `dev/main.xsh` lifecycle against its tests and `Makefile` facade; remove duplicate orchestration only where the command contract remains clear.
- **H02 · P1.** Select two existing host-heavy `showcase/` programs for fault-injection review, using the `docs/SHOWCASE.md` selection test rather than size or recognizability.
- **H03 · P1.** Exercise partial-write, cancellation, and cleanup behavior in `showcase/backup-rotate.xsh` and `showcase/archive-unpack.xsh`; fix observed failures in their paired native tests.
- **H04 · P2.** Check `showcase/release-pack.xsh` and `showcase/bump-version.xsh` for atomic output and rollback behavior; require deterministic fixture results before editing their policy.
- **H05 · P2.** Check bounded timeouts, child cleanup, and output limits in `showcase/watch-run.xsh`, `run-retry.xsh`, and `wait-for.xsh` under controlled failure fixtures.
- **H06 · P2.** Check byte/path handling in `showcase/file-audit.xsh`, `path-audit.xsh`, and `git-digest.xsh`; preserve non-UTF-8 and symlink boundaries with exact tests where relevant.
- **H07 · P2.** Review `core/` applets as a set for duplicated argument parsing, error presentation, and host-boundary helpers; consolidate only repeated, stable XSH code.
- **H08 · P2.** Keep `showcase/jq.xsh` as the documented negative control; use any measured defect it reveals to improve the existing runtime without turning XSH into a general data language.
- **H09 · P2.** Record friction found across H01–H08 as program design, reusable XSH helper, diagnostic/tooling issue, or narrow host capability; add no new semantic category from one script.
- **H10 · P2.** Reconcile `docs/SHOWCASE.md` with the actual `dev/` and corpus state, marking completed infrastructure and deferring speculative new programs until there is a concrete consumer.

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
