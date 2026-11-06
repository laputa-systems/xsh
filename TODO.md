# TODO

Prefix: XSH
Next: 77

Apply the [design principles](docs/DESIGN.md) to each slice: prefer reusable
host boundaries and ordinary XSH policy over command-specific runtime growth.
[Laputa's queue](https://github.com/laputa-systems/laputa/blob/master/TODO.md)
owns distribution consumers; its [roadmap](https://github.com/laputa-systems/laputa/blob/master/docs/ROADMAP.md)
owns VM milestones. Cross-repository IDs below name prerequisites, not permission
to edit another checkout.

The systems-foundation items were reviewed against `9d893e0075d2`; findings
marked unproven need a reproducer, not an assumed defect. Existing acceptance
items remain open. The performance/deployment and Lean campaigns retain their
own pause/launch rules; this queue update resumes neither. The Deser
codec cutover was functionally qualified on an earlier Linux source; current-source
qualification is active. Native macOS qualification
is deferred by the owner, with no macOS host available. The [G0 release candidate](docs/ROADMAP.md#g0-release-candidate) decides
which items block a release; performance and resource items such as XSH-1 and
XSH-3 are deferred for G0. Do not make an unrelated full baseline an entry fee
for a bounded fix; use the narrow applicable gate. Real-hardware enablement is
out of scope.

### XSH-1 · P2 · Complete private and resource baseline acceptance

- **Area:** Performance
- **Where:** `dev/util-linux/resources.py:1632`, `dev/util-linux/acceptance.py:926`
- **Problem:** Final private/resource qualification remains open on current source. The historical scheduler comparison failed, so composed baseline close cannot proceed.
- **Recommendation:** After scheduler qualification, run the private and resource gates on the same source. Complete baseline close with its one-use independent review and readiness checks.
- **Fix with:** benchmark-checklist
- **Effort:** S

### XSH-35 · P2 · Qualify clock domains and deadline conversions

- **Area:** Runtime contracts
- **Where:** `src/modules/time.rs`, `src/runtime/eval/`, `crates/xsh-registry/`, `docs/templates/SPEC.md`
- **Problem:** Wall time is epoch milliseconds, `time.monotonic()` is nanoseconds from a process-local origin, and `time.boottime()` uses nanoseconds from a host clock. Linux boottime includes suspend; macOS uses its monotonic clock. All three use integers. Laputa xinit now converts monotonic nanoseconds to milliseconds for elapsed deadlines; other consumers and conversion boundaries remain unqualified.
- **Progress:** Pinned Linux host tests bracket wall, process-monotonic and boot-clock readings against their owning clocks. DHCP one-shot deadlines saturate without overflowing and reject unrepresentable CLI seconds before network setup; focused tests cover cancellation, zero budgets and existing wall/boot-clock jumps. The Linux inventory of `core/`, `core/lib/` and the Rust wait paths is complete. Rust waits (`Instant`, `poll`, process and netlink deadlines) are monotonic and take Durations of at most `u64` milliseconds, which cannot overflow `Instant` on 64-bit Linux; a `time.sleep` of the largest millisecond Duration is cancelled by SIGTERM rather than panicking. `syslogd` and DHCP use the boot clock deliberately. `lib.monotonic` gives `Reading` and `Deadline` nominal types so a wall value cannot meet a monotonic deadline, with tests for zero, negative, expired and saturating budgets; `udevadm`, `nc`, `traceroute`, `ping`, `curl`, `wget`, `dd` and `mdev` now use it. `iw` read wall milliseconds as nanoseconds and is fixed.
- **Remainder:** VM pause and physical suspend need a disposable guest and host (LAP-27) and stay unqualified. Native tests cannot step the wall clock inside a consumer, so wall-jump immunity of the migrated commands rests on the types, not an injected clock. `ping` still adds raw millisecond integers taken from `Reading.ms`. `traceroute` and `ping` round-trip times subtract wall readings from `SO_TIMESTAMP` receive stamps, which are `CLOCK_REALTIME`, so a step during a probe distorts that RTT. macOS is owner-deferred. Keep wall timestamps for calendar and security decisions; never persist a process-local instant as a restart-stable timestamp.
- **Acceptance:** Native tests separate wall and monotonic inputs, cover backward/forward wall jumps, zero/expired budgets, overflow and cancellation; host tests establish the units/origin. Qualify VM pause separately from physical suspend. Qualify xinit's conversion and any further consumer migration with unit-safe conversions; update authored docs before regeneration; no host clock changes.
- **Coordination:** Laputa LAP-27 owns xinit migration and disposable-guest clock tests.
- **Fix with:** make-invalid-states-unrepresentable
- **Effort:** M

### XSH-36 · P2 · Qualify descriptor and process ownership under failure

- **Area:** Resources
- **Where:** `src/modules/`, `src/runtime/process.rs`, `tests/xsh/process-failure-ownership.xsh`, `tests/xsh/unix-spawn-ownership.xsh`
- **Problem:** Linux qualification found no defect: repeated failed spawns, early consumer exit, partial input and output, and descriptor inheritance leave no owned children or leaked descriptors, and a released or stale handle cannot signal a recycled pid. macOS has no qualification, and no backend limitation is recorded for it.
- **Recommendation:** Run the same cases on macOS, where `/proc` is absent, so the child and descriptor inventories need another source. Record any backend that cannot enforce a contract instead of treating a no-op as enforcement.
- **Acceptance:** The cases in `tests/xsh/process-failure-ownership.xsh` pass on native macOS. Native macOS qualification is deferred by the owner.
- **Coordination:** Laputa LAP-34 exercises the service lifecycle; retain the host-process/bounded-stream concurrency model, not a new async runtime.
- **Fix with:** lifecycle-concurrency
- **Effort:** S

### XSH-37 · P2 · Qualify confined filesystem operations for privileged consumers

- **Area:** Host authority
- **Where:** `crates/xsh-root/`, `crates/xsh-archive/`, `src/modules/fs.rs`, `src/modules/archive/`, `tests/`
- **Problem:** `xsh-root` confines pathname resolution, but lexical Path validation and an `fs` effect are not filesystem authorization. Archive extraction (`archive.extract` and `FsRoot.extract`, one engine), `patch.apply`, `FsRoot.rename`/`fsync` and `fs.root_install_file` acquire one anchor and resolve beneath it; Path-taking extraction and publication acquire that anchor from the path. Rooted file reads in archive creation, patch and `FsRoot.fsync` open non-blocking and refuse a non-regular file, but `FsRoot.read` of a FIFO still blocks by design.
- **Recommendation:** Add `FsRoot` hard-link and special-file creation only if Laputa LAP-33 shows a need.
- **Status:** The Linux acceptance is met. `crates/xsh-archive/tests/interrupted.rs` (`test-archive-interrupted`) SIGKILLs a child mid-extraction (path and retained-root destinations, and over an existing destination), mid-creation, and during repeated publication. `tests/runtime/root_install.rs` SIGKILLs a script mid-`fs.root_install_file`, fresh and replacing. In every case staging is left behind and no partial destination or replaced file becomes visible. `rooted.rs` and `publication.rs` cover ancestor and symlink replacement of a retained root. Only the owner-deferred macOS work and the items conditional on Laputa LAP-33 (`FsRoot` hard-link and special-file creation) remain; the ticket stays open for those.
- **Acceptance:** Adversarial fixtures cover extraction into a retained root under ancestor and symlink replacement, and interrupted publication by process kill. Power-loss durability stays unclaimed; atomic visibility and durability stay tested as separate contracts. macOS enforcement is owner-deferred and must fail explicitly until qualified.
- **Coordination:** Laputa LAP-33 owns package adoption; LAP-28 owns build-process isolation; XSH-47 owns any process sandbox claim.
- **Fix with:** isolate-effects
- **Effort:** S

### XSH-43 · P2 · Stress process and stream lifetimes with signal storms

- **Area:** Runtime verification
- **Where:** `tests/runtime/signal_storm.rs`, `docs/TESTING.md`
- **Problem:** The Linux seed-replayable storm (`runtime::signal_storm`) found no orphan, double cleanup or unbounded cancellation. It does not cover SIGKILL of the runner, terminal-mode restoration, interruption inside spawn setup, or macOS. A second signal that lands after cleanup but before exit kills the runner with the default action, which the oracle accepts.
- **Recommendation:** Add terminal restoration for an interactive PTY session (`crates/xshi/`) and the macOS run when a host exists; reuse the plan generator and oracle.
- **Acceptance:** The storm passes on native macOS and checks terminal restoration. Native macOS qualification is deferred by the owner.
- **Coordination:** XSH-36 owns the resource contracts; Laputa LAP-36 supplies the xinit/PID 1 consumer, and LAP-34 owns its first hermetic lifecycle replay.
- **Fix with:** lifecycle-concurrency
- **Effort:** S

### XSH-55 · P2 · Qualify the native SSH client

- **Area:** Remote execution
- **Where:** `crates/xsh-ssh`, `src/modules/ssh.rs`, `docs/rfc/XSH-55-native-ssh-client.md`
- **Problem:** The native client is experimental on Linux x86-64. An internal protocol and security source review (2026-11-05) fixed five defects, and a second round fixed seven of its undemonstrated risks (rekey `NEWKEYS` filtering, key file permissions and size, FIFO opens, error wakeups, server requests during channel setup, close after exit status, zeroization). The `xsh-ssh` suite passed on glibc against OpenSSH 10.0; the second round was not rerun on musl. Open: native macOS qualification, an independent protocol and security review, a native signal-cancellation test, the native `tests/xsh/ssh-` suite on musl, the build-time gate (host busy), and the review section's remaining risks (lenient padding and AEAD length checks in the vendored decoder, no traffic served while a stalled output file is awaited, unwiped caller stdin and Graviola key state).
- **Recommendation:** Complete the remaining qualification in the RFC and obtain the independent review. Preserve mandatory host verification and typed execution certainty.
- **Acceptance:** The RFC's required tests pass against disposable OpenSSH servers, with ssh-build's OpenSSH client as oracle, on the executed Linux musl and macOS arm64 lanes; the client is labeled experimental. Native macOS qualification is deferred by the owner.
- **Coordination:** Laputa R6 and XSH-56 consume the client.
- **Fix with:** security
- **Effort:** L

### XSH-56 · P2 · Complete xsh-infra qualification

- **Area:** Remote execution consumer
- **Where:** `laputa-systems/xsh-infra`, `docs/rfc/XSH-56-xsh-infra.md`
- **Problem:** xsh-infra implements remote reconciliation over native SSH and has Linux Alpine container coverage, but the RFC's full qualification remains open. Its pyinfra comparison covers directory, file, symlink, user and group; packages, services, commands and failure comparisons remain open.
- **Recommendation:** Complete the remaining RFC scenarios and differential comparisons against the existing XSH implementation and native SSH transport.
- **Acceptance:** The RFC's per-resource and integration scenarios pass against disposable Alpine SSH hosts, with pyinfra as a differential reference on equivalent hosts; XSH defects found get minimal reproducers in XSH.
- **Coordination:** Remote execution uses the experimental XSH-55 client.
- **Fix with:** isolate-effects
- **Effort:** L

### XSH-3 · P3 · Finish the deferred consolidation acceptance matrix

- **Area:** Tests
- **Where:** `dev/consolidation/final_canonical_replay.py`, `dev/performance/check.py`, `dev/consolidation/winddown/gate-tools/`
- **Problem:** Canonical, supplemental, profile, observer, forced and comparative checks passed on R13 (`0dfebb001`); nothing is qualified on current source. The scheduler failures there are fixed (`e3179f233`, `a758be15e`, `2515907c8`, `0836935e3`). The frozen reference `perf/reference-9d-qualification` cannot continue: its pinned test image is gone and `Dockerfile.test` builds from `alpine:edge`; master pins the present `xsh-test` image in `bench/consolidation/versions.lock.json`. On `perf/reference-10-qualification` (`eb94b57d7` plus the fixes below) the tooling now runs: observer, supplemental-plan and owned-container pins are re-pinned or requalified, the replay mounts `.work/gate`, and root-run gate fixtures are handed to the native user. Bootstrap passes. The latest canonical replay (`replay-r10f` on `eb94b57d7`, 2026-11-05) passed every Rust stage and the full native suite finished with 15,270 passed, 54 failed, 181 skipped. Fixes since: 51 lint-fix and tooling failures came from the repository `xsht-config.ini` reaching fixtures under the native scratch (`8546e8fe5`), one from a stale API surface fixture (`05d1aef79`), and one from a formatter fixture that spliced a list into fixed parameters (`6551176d1`); each file passes in the pinned image. The replay has not been rerun on the fixed tree.
- **Recommendation:** On one frozen source, in order: `dev/performance/check.py functional --check`; a fresh normal profile (`profile-cost.py`) and `dev/perf/btop-profile.xsh` in a readiness-bracketed window; `final-observers.py --execute`; forced frames, the comparative profile and three scheduler pairs; util-linux `resources --check` and `close --check` (XSH-1). Bootstrap first with `final_canonical_replay.py bootstrap --repo SOURCE --head COMMIT --tree TREE --output .work/tmp/bootstrap-RUN --run-name bootstrap-RUN`, then `execute --bootstrap-receipt .work/tmp/bootstrap-RUN/bootstrap-receipt.json`. Validate each tool's `--plan` before an expensive run. Then publish the accepted baseline and retire the infrastructure under XSH-19.
- **Unmet:** (1) `tests/xsh/pattern-tests.xsh::test_pattern_predicate_dynamic_facets_retain_the_error_api` fails: the full IR verifier rejects `error_message(value)` after `value is NotFound` (`ground producer value disagrees with its checked contract`) because `tested_read_type` takes only types that `runtime_complete_check_type` calls complete, and that excludes `ErrorFamily`, `ErrorVariant` and `ErrorFacet`. Accepting them (`type_view.rs` decides all three completely) and letting a tested error type fit `Error` was not enough in two attempts; the draft patch and its tests are not committed. (2) The scheduler reference arm cannot be derived: the pinned `dev/consolidation/winddown/unfinished/475-native-scheduler-reference-*.patch` no longer applies to `crates/xsht/src/xsht/test.rs`, which has been rewritten since, and reverting the three reference scheduler commits conflicts in five regions. (3) `check.py functional --check` needs a skip review and a semantic review bundle (`ReviewEvidenceBundle`) bound to the source trees; none exists for this source, and every later step takes the functional-close record. (4) No supplemental, functional, profile, observer, forced, comparative, scheduler or util-linux step has run on this source. The checker also accepts a positional list splice into fixed parameters that docs/SPEC.md forbids (positional splices fill only the final rest parameter).
- **Fix with:** orchestrate
- **Effort:** M

### XSH-77 · P4 · Accept a record spread into a returned callable without a rest parameter

- **Area:** Checker
- **Where:** `src/sema/types.rs` (`TypedCallable`), `src/sema/check/typed_callable.rs` (`check_callable_contract_call_with_checked_argument`), `src/sema/check/call.rs`
- **Problem:** A local holding a declaration-derived callable keeps its parameter names, but `TypedCallable` does not record whether the value came from a declaration or from an authored fixed-arity type, so a call through any callable without a rest parameter rejects `...` spreads with `check.callable-mismatch`. Only callables with a rest parameter, which an authored type cannot have, take the record-spread path.
- **Recommendation:** Owner decision between two designs. (a) Drop the authored-type rejection in SPEC 9.4: authored callable types already carry labels and a function fits one only when its labels match, so a record spread can bind by label through any callable value; no new state, and the rest-parameter special case in `call.rs` goes away. (b) Add a `CallableOrigin` to `TypedCallable` with equality ignoring it; origin cannot live on the binding (`let g = f` copies an authored type) or be derived (an authored `pure(value: Int) -> Int` equals a declaration's type field for field), and about 25 IR and verifier construction sites plus `SchemeType::Callable` would need it, so the IR must encode it.
- **Acceptance:** `...{first: 7}` into a returned callable without a rest parameter checks and runs, and SPEC 6.3 and 9.4 state the chosen rule.
- **Fix with:** architect
- **Effort:** M

### XSH-78 · P4 · Resolve arithmetic in an erased proc returning an open spread through a generic call

- **Area:** Checker
- **Where:** `src/sema/constraints/operations.rs` (`SealedOperation::RecordOverride` selection), `src/runtime/eval/indexed/full/verify_schemes.rs`, `tests/xsh/full-inference-argument-closure.xsh`
- **Problem:** The failure is not arithmetic. The erased `pure reset(state) { same({...state, code: []}) }` generalizes with a `RecordOverride` requirement whose override type is `List[a]`; nothing ties `a` to the caller's `code` field, because `state` is an open row. At every call site (`let r = reset(start)`, with or without a `Parsed` expectation) the instantiated `RecordOverride` keeps `a` free, and the selection step skips an operation whose override types contain inference, so the instantiation fails with `sealed arithmetic operation remains unresolved`. Selecting once the receiver is concrete turns that into `type argument remains unresolved` for `reset(start).code.len()`, the same policy that rejects `same([])`, and into `generic call substitution has no independently checked argument evidence` from the Full verifier for `let r: Parsed = reset(start)`. The test's `reset(start).code.len()` form therefore cannot check without a policy decision.
- **Recommendation:** Decide whether an erased override of an existing field may leave the new value's type free and default it at the call site, or whether `reset` must keep its annotation; if the former, let `RecordOverride` select with a concrete receiver and give the Full verifier the checked argument evidence for the instantiated override, with a forge test beside `full_verifier_authenticates_erased_open_spread_record_arguments`.
- **Acceptance:** Either the test's `reset` drops its annotation and checks, lowers and runs with the same result, or the SPEC states that an erased proc returning an open-spread override needs a declared result and the checker names that cause.
- **Fix with:** architect
- **Effort:** M

### XSH-68 · P3 · Recover the Deser JSON roundtrip regression

- **Area:** Performance
- **Where:** `crates/deser`, `src/modules/json.rs`, the serializer event path
- **Problem:** On the same 20,000-record JSON, five alternating pinned Linux O2 pairs measured roundtrip median CPU of 59.065 ms on pre-Deser `e1b896dc` against 69.813 ms on `ad126a1e8`, above the 5% budget and the spread; decode alone stays within spread. Borrowed event guards later cut ten encodes by about 5%. Rejected without benefit above spread: small-record text vectors, direct compact-name accumulation, decoder sink inlining, an `#[inline]` encoder hint, per-container field emitters, and the parked primitive-handle prototype `fb3bc1712`. Profiles point at serializer event dispatch and field iteration. The products differ beyond the codec, so no single change is attributed.
- **Recommendation:** Profile the current encode path against `e1b896dc` on matched products and fix the dominant cost; S6 runtime, startup and build CPU-time qualification on current source closes with it.
- **Acceptance:** Roundtrip median CPU within `max(5%, paired spread)` of the pre-Deser baseline over five readiness-qualified alternating pairs with exact outputs; decode, JSONL and checker controls within budget.
- **Fix with:** make-it-fast
- **Effort:** M

### XSH-14 · P3 · Reuse import-graph checks across lint candidates

- **Area:** Performance
- **Where:** `crates/xsht/src/lint.rs`, `crates/xsht/src/cli`
- **Problem:** Each lint candidate probe parses, loads, and checks the unchanged import graph again. Large fix runs repeat the same work for every candidate.
- **Recommendation:** Measure shared checking, grouped candidates, and clean-corpus probe skipping. Keep the smallest design that preserves diagnostics and fix safety.
- **Unmet:** The batched-probe candidate (`97e361171`, merge `6300eb05b`) was reverted on 2026-11-01: guarded CPU timing put lint at 0.98 to 1.00 of the baseline on three workloads, inside the spread, for about 700 lines and 86,080 bytes of release `xsht`. Any further attempt must first show where per-candidate time goes on the current tree.
- **Fix with:** make-it-fast
- **Effort:** M

### XSH-19 · P3 · Retire consolidation-only gate infrastructure after acceptance

- **Area:** Build
- **Where:** `dev/consolidation/winddown/gate-tools/`, `dev/performance/check.py`, `dev/util-linux/acceptance.py`
- **Problem:** Baseline qualification currently depends on campaign-specific controllers, receipt adapters and historical pins. Leaving those dependencies in the everyday gate path makes subsequent changes require campaign history. The accepted baseline is still pending, so deletion must follow G1.
- **Recommendation:** Execute G5 immediately after G1. Trace current callers, move reusable checks behind ordinary developer commands, and delete obsolete adapters, duplicate entry points and historical-pin dependencies. Preserve required gates and their rejection controls. Update testing and baseline docs. Keep historical evidence outside the everyday command path.
- **Fix with:** dead-code-archaeology
- **Effort:** M

### XSH-38 · P3 · Establish one typed virtual-network library cohort

- **Area:** Systems libraries
- **Where:** `core/ip.xsh`, `core/ifup.xsh`, `core/lib/`, `src/modules/`, `crates/xsh-registry/`
- **Problem:** Interface/route observations and low-level network operations exist, but a shared typed observe/change surface for stateful VM administration is not qualified. More command spellings alone would not close that boundary.
- **Recommendation:** Start with links, IPv4/IPv6 addresses and routes on virtual Ethernet. Put policy and command rendering in shared XSH libraries; Rust owns narrow host operations and decoding. Make malformed/truncated replies, interrupted dumps and lost notifications visible so callers can re-observe. Add bridge/veth/namespace operations only when the next concrete fixture requires them; no Wi-Fi or replacement DHCP stack.
- **Acceptance:** A command and Laputa's controller use the same records/operations without rendering and reparsing text. Fixtures exercise malformed input and event loss; disposable Linux guests exercise actual mutations and convergence. Report unsupported families/backends separately; do not invent empty successful observations.
- **Coordination:** Laputa LAP-32 owns managed-resource identity, planning and reconciliation.
- **Fix with:** single-source-of-truth
- **Effort:** L

### XSH-39 · P3 · Qualify bounded Unix-socket I/O for VM control

- **Area:** Host I/O
- **Where:** `src/modules/`, `crates/xsh-registry/`, `core/lib/`, `tests/`
- **Problem:** `unix.connect_stream`, descriptor I/O and polling support Laputa's XSH QMP client, which owns and closes its socket and uses a monotonic deadline. Linux host tests cover short-write backpressure and retries, scoped closure while the interpreter remains alive, shutdown during a bounded poll, and a native line-protocol fixture on `lib.monotonic` deadlines: byte-fragmented and coalesced frames, EOF, a stalled peer, oversized unterminated and terminated frames, abrupt close mid-frame with a stable descriptor count, and SIGTERM mid-frame with cleanup and peer-visible closure. `unix.poll_fd` and `unix.poll_fds` both end at a handled signal, so Laputa's QMP client cancels promptly. macOS and Laputa LAP-35 corroboration remain unqualified.
- **Recommendation:** Keep framing and QMP policy in the XSH consumer.
- **Acceptance:** The same properties hold on the macOS host path, and Laputa LAP-35 demonstrates the same primitives against a real QEMU monitor; no futures runtime or callbacks are required.
- **Fix with:** isolate-effects
- **Effort:** S

### XSH-40 · P3 · Reduce one system failure to a hermetic native replay

- **Area:** Tests
- **Where:** `crates/xsht/`, `src/trace/`, `tests/`
- **Problem:** Laputa xinit has a native fake replay of a crashed leader with an owned descendant, corroborated by a disposable Linux x86-64 guest and a deliberately broken cleanup. The replay's input completeness, ordering, redaction and rejection of ambient-host fallback remain unqualified.
- **Recommendation:** Qualify the existing LAP-34 counterexample against an explicit ordered, complete and redacted input contract. Add a minimal reusable test helper only if existing fixtures cannot express the case; no generic replay platform or campaign controller.
- **Acceptance:** The reduced case reproduces offline, fails when the repaired invariant is deliberately broken, and rejects missing/mismatched observations rather than consulting the host. Test fixtures contain no secrets. Corroborate with the real guest; mock agreement does not establish OS conformance or power-loss durability.
- **Fix with:** root-cause
- **Effort:** M

### XSH-44 · P3 · Connect one Lean semantic scenario to actual XSH execution

- **Area:** Formal conformance
- **Where:** `xsh-lean.md` (G0b/G1b/G9 and the connected milestone), `formal/README.md`, `formal/Xsh/Conformance/`, `src/runtime/eval/indexed/full/`, native tests
- **Problem:** Lean models and finite bridge fixtures exist, but `formal/coverage.json` still has no source-identified Rust conformance cases. A theorem about the Lean model is not correspondence with the compiler or host.
- **Recommendation:** Only when the owner explicitly resumes the paused Lean campaign and its source-freeze/G1 prerequisites hold, implement its prescribed test-only source/IR bridge and the first stage of the connected milestone, with an independently specified actual-source witness (prefer ordered argv or Result propagation). Shape the case so a second implementation can submit its candidate facts and IR to the same checks. Replay the same bounded observation through the verified XSH execution path, record precise source/IR identity and expected observations, and minimize a mismatching case. Reuse G1b/G9; no new Lean campaign or product-visible exporter.
- **Acceptance:** One traceable ledger entry compares independent model expectations to actual Rust/XSH outcomes; altered source bindings, forged IR, omitted events and intentionally mutated behavior are rejected. The entry states its starting boundary (source bytes or supplied syntax). Record the limits of the finite witness, and make no full-language or Rust/kernel soundness claim. This TODO does not resume Lean work.
- **Fix with:** reference-parity
- **Effort:** M

### XSH-45 · P3 · Reuse deterministic host-observation replay in a second systems consumer

- **Area:** Host tests
- **Where:** `core/lib/btop_source.xsh`, `docs/BTOP.md`, `core/lib/`, `src/modules/`, `crates/xsht/`, `tests/`
- **Problem:** btop has a bounded rooted `/proc`/`/sys` capture-and-replay path and XSH-40 owns one reduced lifecycle failure, but reuse across a different real host-observation workload is unproven. A universal host-provider framework is not justified.
- **Recommendation:** After XSH-40 qualifies the replay contract, reuse its ordered, complete, secret-redacted observations for one additional system-report or Laputa PM/network consumer. Separate pure policy/model derivation from narrow live host reads only where that reduces existing machinery. Use existing `FsRoot`, typed mocks and trace boundaries; don't introduce a second runtime or generic host-injection framework.
- **Acceptance:** Fixed replay is deterministic without ambient-host fallback; omitted/reordered/stale observations fail explicitly, injected short reads and permission/clock errors reproduce a meaningful invariant, and a live Linux or disposable-guest corroboration covers the actual host boundary. Retain the smallest reusable fixture helper and report untested host behavior.
- **Coordination:** XSH-40 owns the first hermetic failure; Laputa LAP-37 owns the second operational consumer.
- **Fix with:** isolate-effects
- **Effort:** M

### XSH-46 · P3 · Pilot opt-in static signatures for external command flags

- **Area:** Tooling
- **Where:** `crates/xsht/`, `crates/xsh-registry/`, `docs/XSHT.md`, native CLI contracts and tooling tests
- **Problem:** XSH checks native and XSH-defined CLI contracts but does not statically validate arbitrary `run` targets' flags. An optional trusted signature could catch misspellings without constraining ordinary Unix interoperability.
- **Recommendation:** Start with one statically named XSH core applet whose CLI signature already comes from the canonical registry; optionally test one pinned external tool. Have `xsht` diagnose known incompatible flags/argument shapes, warn on incomplete or stale signatures, and skip dynamic command heads. Never execute tools during checking or add a mandatory external-signature database. A tool's CLI schema cannot prove its effects or authority.
- **Acceptance:** Valid invocations and unchecked dynamic commands retain their behavior; reviewed complete contracts reject known-invalid flags, partial contracts never cause false hard failures, signatures stay version-associated, and no filesystem/network/process execution is triggered by checking. Benchmark checking overhead and drop the experiment if maintenance outweighs benefit.
- **Coordination:** Laputa LAP-40 pilots a real recipe only if the opt-in tool contract proves useful.
- **Fix with:** make-invalid-states-unrepresentable
- **Effort:** M

### XSH-47 · P3 · Qualify one OS-enforced subprocess confinement boundary

- **Area:** Host authority
- **Where:** `src/runtime/process/`, `crates/xsh-root/`, `src/modules/`, `tests/`, `docs/SPEC.md`
- **Problem:** Effects and `without` statically constrain XSH calls; `FsRoot` confines selected pathname operations. Neither enforces authority over an external child: `without net` is not a kernel network sandbox, and no general confinement guarantee follows from its type.
- **Recommendation:** Use Laputa LAP-28's concrete closed-build cohort to first audit available kernel primitives and prove one optional constrained child execution path, reusing namespaces, Landlock, seccomp or existing container isolation only where each actually enforces the requested property. Keep static effects and OS authority distinct; prefer existing execution APIs and the smallest needed host primitive. No universal capability algebra, mandatory sandbox, or cross-platform pretend-success.
- **Acceptance:** Declared inputs/writes remain usable; forbidden file access, process/network operations and inherited-descriptor escapes fail under the actually supported profile. Tests prove enforcement inside a disposable Linux environment and reject unavailable kernels/features before execution. Document remaining privilege/host/child escapes and measure startup and dependency cost.
- **Coordination:** Laputa LAP-28 already owns the consumer and OS-enforced closed build; LAP-33 and XSH-37 separately own confined pathname operations.
- **Fix with:** isolate-effects
- **Effort:** M

### XSH-48 · P3 · Exercise pure XSH policy decisions without a new policy language

- **Area:** Systems architecture
- **Where:** `core/lib/`, `src/sema/`, existing callable/effect tests and one Laputa service/network policy consumer
- **Problem:** Restart, admission and reconciliation decisions tend to accumulate configuration flags or host-specific branches. XSH already has pure functions and effects, but no real consumer has established that an explicit policy callable simplifies one of these decisions.
- **Recommendation:** Start with Laputa LAP-38's selected restart/backoff or virtual-network decision. Pass an immutable typed observation to an ordinary XSH pure function and produce a typed decision, with execution and effects remaining in its caller. Compare against the existing straightforward implementation before introducing any callback field or public API. No speculative strategy DSL, component runtime or new language syntax.
- **Acceptance:** The selected consumer has deterministic policy fixtures, rejects transitive `fs`/`net`/`process`/`time` effects in pure code, preserves precedence and failure behavior, and demonstrably removes duplicated branches or policy state. If an ordinary helper is sufficient, keep it and explicitly reject a new abstraction.
- **Coordination:** Laputa LAP-38 owns the concrete policy and operational parity proof.
- **Fix with:** single-source-of-truth
- **Effort:** S

### XSH-66 · P3 · Compare practical joint views and close erased sort's CPU gap

- **Area:** Inference performance
- **Where:** `core/wc.xsh`, `core/sort.xsh`, `core/awk.xsh`, Laputa `pm/generation.xsh`, `docs/PERFORMANCE-INFERENCE.md`
- **Problem:** The four practical joint views pass, but their joint behavior and interfaces have not been compared with the annotated sources, and the host-consumer guest drill has not been rerun on them. Joint-erased `sort.xsh` produces identical output at about 2.3 times the annotated source's CPU on the faster sort build.
- **Recommendation:** Diff joint-view interfaces and outputs against the annotated sources, rerun the guest drill, then profile erased `sort` to find which inferred types cost the extra CPU.
- **Acceptance:** Interfaces and outputs match or each difference is explained; the CPU gap is attributed and either removed or recorded with its cause under the performance campaign's sampling rules.
- **Fix with:** make-it-fast, benchmark-checklist
- **Effort:** M

### XSH-23 · P4 · Evaluate bbtree as the ordered-map implementation

- **Deferred:** The owner is not interested in this evaluation at present. Exclude it from the active backlog run until the owner reopens it, even after a baseline becomes available.
- **Area:** Performance
- **Where:** `src/runtime/value.rs:172`, `src/runtime/value.rs:958`, `src/sema/schemes.rs`, `src/sema/constraints.rs`
- **Problem:** `laputa-systems/bbtree` claims about 5x faster single-thread lookups than std `BTreeMap` with `unsync::BBTreeMap` as a drop-in. XSH uses `BTreeMap` throughout the checker, verifier and runtime values, but most maps are probably tiny, where the gain may vanish. The measured campaign growth is generalization-map copying, which depends on clone cost. `unproven` that ordered-map operations matter at all.
- **Recommendation:** After the post-inference performance baseline, profile the 512-unit generalization witness and script startup to rank `BTreeMap` time. Only if it is a meaningful share, swap `unsync::BBTreeMap` in behind one alias at the hottest owner and compare interleaved CPU time. Vet the single maintainer, its `unsafe` and allocation code, `InlinkeKey` on our key types, and the build-time, binary-size and startup gates before widening.
- **Fix with:** make-it-fast
- **Effort:** M

### XSH-42 · P4 · Evaluate an optional SQLite boundary with one real consumer

- **Area:** Optional stdlib
- **Where:** `src/modules/`, `crates/xsh-registry/`, `examples/`, `Cargo.toml`
- **Problem:** Transactional local state may simplify maintenance history or small catalogs, but the benefit and dependency cost are unproven. This is not a requirement for PID 1, PM receipts or the default system.
- **Recommendation:** Select one concrete consumer and compare against ordinary files before implementation. If justified, prototype an optional established SQLite binding with parameterized statements, explicit transactions, owned connections/statements, bounded row iteration and existing value validation. No ORM, SQL type system, storage-engine rewrite or mandatory service.
- **Acceptance:** The consumer demonstrably loses bespoke persistence machinery; tests cover injection-safe parameters, rollback, busy/error handling, cancellation and resource release. Measure dependency, build, startup, binary-size and memory cost against the no-feature build. Stop at the evaluation if no consumer earns that cost; keep early boot and immutable receipts independent.
- **Fix with:** none
- **Effort:** M

### XSH-49 · P4 · Account for first-party complexity in one capability change

- **Area:** Design economy
- **Where:** `docs/DESIGN.md`, `docs/ARCHITECTURE.md`, one selected codec or host-capability change
- **Problem:** Binary-size/startup gates already exist, but they do not expose ownership proliferation, parallel representations, repeated semantics or Rust/native glue added for one XSH-facing capability. A raw line-count ceiling would punish reuse of trustworthy dependencies and tests.
- **Recommendation:** On one real accepted change (the Deser cutover or a bounded host API), record before/after first-party source ownership, runtime variants and operations, representations/conversions, duplicated invariants, required crates, product size and startup, and one end-to-end XSH consumer. Identify machinery retired and any complexity moved outward; use a small review table in the owning existing document, not a separate metrics framework.
- **Acceptance:** Independent reviewer can trace added, retained and deleted responsibilities to code and tests; no compatibility or verifier boundary is counted as removable merely because it duplicates a check. Record a concrete simplification or reject the abstraction on evidence, without blocking unrelated fixes on a new arbitrary LOC quota.
- **Coordination:** Laputa LAP-39 applies the same method to one userspace consumer; existing performance gates remain authoritative.
- **Fix with:** ultratriage
- **Effort:** S
