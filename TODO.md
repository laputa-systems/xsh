# TODO

Prefix: XSH
Next: 58

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
codec cutover is accepted; its correctness and platform qualification remain
open. The [G0 release candidate](docs/ROADMAP.md#g0-release-candidate) decides
which items block a release; performance and resource items such as XSH-1 and
XSH-3 are deferred for G0. Do not make an unrelated full baseline an entry fee
for a bounded fix; use the narrow applicable gate. Real-hardware enablement is
out of scope.

### XSH-1 · P2 · Complete private and resource baseline acceptance

- **Area:** Performance
- **Where:** `dev/util-linux/resources.py:1632`, `dev/util-linux/acceptance.py:926`
- **Problem:** Final private/resource qualification remains unrun on the current frozen source. The scheduler comparison failed, so composed baseline close cannot proceed.
- **Recommendation:** After scheduler qualification, run the private and resource gates on the same source. Complete baseline close with its one-use independent review and readiness checks.
- **Fix with:** benchmark-checklist
- **Effort:** S

### XSH-35 · P2 · Qualify clock domains and deadline conversions

- **Area:** Runtime contracts
- **Where:** `src/modules/time.rs`, `src/runtime/eval/`, `crates/xsh-registry/`, `docs/templates/SPEC.md`
- **Problem:** Wall time is epoch milliseconds while `time.monotonic()` returns nanoseconds from a process-local origin, and `time.boottime()` (XSH-53) returns nanoseconds since boot including suspend. All three use integers; Laputa's xinit currently constructs elapsed deadlines from wall time. A mechanical replacement would change units and origin. Unproven: which other deadline consumers mix domains.
- **Recommendation:** Inventory the actual wait/retry/deadline consumers, document units and restart/pause semantics, and qualify one shared elapsed-deadline boundary. Prefer existing Duration and nominal-type machinery over new syntax or a units framework. Keep wall timestamps for calendar/security decisions; never persist a process-local instant as a restart-stable timestamp.
- **Acceptance:** Native tests separate wall and monotonic inputs, cover backward/forward wall jumps, zero/expired budgets, overflow and cancellation; host tests establish the units/origin. Qualify VM pause separately from physical suspend. Migrate the first real consumer with unit-safe conversions and update authored docs before regeneration; no host clock changes.
- **Coordination:** Laputa LAP-27 owns xinit migration and disposable-guest clock tests.
- **Fix with:** make-invalid-states-unrepresentable
- **Effort:** M

### XSH-36 · P2 · Qualify descriptor and process ownership under failure

- **Area:** Resources
- **Where:** `src/modules/`, `src/runtime/eval/`, `crates/xsh-registry/`, `tests/`
- **Problem:** Owned process/resource scopes coexist with bare descriptor APIs. Their presence is not proof of a leak; transfer, inheritance and cancellation behavior across composed operations needs a focused qualification.
- **Recommendation:** Start with spawn plus readiness pipes and one early-terminated stream. Specify who owns each descriptor/child before and after transfer, failed spawn, close and cancellation. Reuse existing ownership/scopes; add an owned wrapper only for an evidenced unsafe common path. Keep raw APIs explicit. Audit process identity separately from a numeric PID; pidfds where supported do not excuse an acquisition race or an unproved fallback.
- **Acceptance:** Repeated failed spawns, early consumer exit, signal interruption, partial I/O and cleanup failure leave no owned children, zombies or leaked descriptors. Verify close-on-exec and intentional inheritance, bounded cleanup, and that signaling cannot target a recycled identity. Record backend limitations instead of treating no-ops as enforcement.
- **Coordination:** Laputa LAP-34 exercises the service lifecycle; retain the host-process/bounded-stream concurrency model, not a new async runtime.
- **Fix with:** lifecycle-concurrency
- **Effort:** M

### XSH-37 · P2 · Qualify confined filesystem operations for privileged consumers

- **Area:** Host authority
- **Where:** `crates/xsh-root/`, `src/modules/`, `src/runtime/eval/`, `tests/`
- **Problem:** `xsh-root` confines pathname resolution, but lexical Path validation and an `fs` effect are not filesystem authorization. Unproven: which extraction/publication paths retain the trusted directory anchor throughout mutation.
- **Recommendation:** Trace the operations needed by Laputa LAP-33 and reuse Root at the smallest shared host boundary. Specify trusted-anchor acquisition, relative paths, symlinks, hard links, mount/special-file policy and destination publication. Extend only missing operations required by that consumer; do not advertise Root or effects as a process sandbox.
- **Acceptance:** Adversarial fixtures cover ancestor/symlink replacement, traversal, archive link ordering, special files, no-clobber conflicts and interrupted publication. Outside sentinels remain unchanged and prior committed outputs survive failures. Unsupported platform enforcement fails explicitly. Distinguish atomic visibility from durable publication and test their contracts separately.
- **Coordination:** Laputa LAP-33 owns package adoption; LAP-28 owns build-process isolation.
- **Fix with:** isolate-effects
- **Effort:** M

### XSH-43 · P2 · Stress process and stream lifetimes with signal storms

- **Area:** Runtime verification
- **Where:** `src/runtime/process/`, `src/runtime/eval/`, `crates/xshi/`, `tests/`, `dev/`, `docs/TESTING.md`
- **Problem:** XSH-36 tests focused descriptor and child ownership, but deterministic stress of process trees and cancellation at adversarial timing boundaries is not established. Existing success-path tests cannot rule out orphans, double cleanup or inherited terminal state.
- **Recommendation:** Build a bounded, seed-replayable Linux/macOS signal-storm test over existing process scopes, pipelines, `defer`/`errdefer`, live streams and interactive PTYs. Inject SIGINT/SIGTERM, SIGPIPE/EPIPE, EOF, child exits and interruption during spawn/cleanup; use real OS children alongside narrow native fakes. Reuse the existing ownership model and test runner; add no new scope syntax or daemon.
- **Acceptance:** Fixed seeds and retained minimized reproducers check one-time release, child reaping, process-group identity, pipeline status, cleanup order, terminal restoration where possible, bounded cancellation and no foreign-process signaling. Deliberately broken cleanup fails the oracle. Distinguish SIGKILL/non-cooperative shutdown and unsupported host behavior; do not claim impossible cleanup guarantees.
- **Coordination:** XSH-36 owns the resource contracts; Laputa LAP-36 supplies the xinit/PID 1 consumer, and LAP-34 owns its first hermetic lifecycle replay.
- **Fix with:** lifecycle-concurrency
- **Effort:** M

### XSH-55 · P2 · Build the native SSH client

- **Area:** Remote execution
- **Where:** new `crates/xsh-ssh`, a thin adapter beside `crates/xsh-net`'s, `docs/rfc/XSH-55-native-ssh-client.md`
- **Problem:** XSH has no SSH transport, so Laputa's remote administration and xsh-infra have nothing typed to build on. The runtime must not shell out to `ssh`.
- **Recommendation:** Follow the RFC: a reusable client over a pinned import of ssh-build's `oxish-proto` and `oxish-graviola`, Graviola as the only cryptographic provider, mandatory host verification, and a small typed exec/stream API with execution certainty.
- **Acceptance:** The RFC's required tests pass against disposable OpenSSH servers, with ssh-build's OpenSSH client as oracle, on the executed Linux musl and macOS arm64 lanes; the client is labeled experimental.
- **Coordination:** Starts after the active campaigns per the [roadmap sequence](docs/ROADMAP.md#sequence). Laputa R6 and XSH-56 consume it.
- **Fix with:** security
- **Effort:** L

### XSH-56 · P2 · Build xsh-infra over the native SSH client

- **Area:** Remote execution consumer
- **Where:** new repository `laputa-systems/xsh-infra`, `docs/rfc/XSH-56-xsh-infra.md`
- **Problem:** No substantial XSH application exercises remote reconciliation; it would show whether XSH supports such an application without moving its logic into Rust.
- **Recommendation:** Follow the RFC: a small pyinfra-inspired tool written in XSH with typed resources, plan/apply/check, per-host failure data, safe file staging and a remote lock, using only XSH-55 for transport.
- **Acceptance:** The RFC's per-resource and integration scenarios pass against disposable Alpine SSH hosts, with pyinfra as a differential reference on equivalent hosts; XSH defects found get minimal reproducers in XSH.
- **Coordination:** Pure planner and resource work may start before XSH-55; remote execution waits for it.
- **Fix with:** isolate-effects
- **Effort:** L

### XSH-3 · P3 · Finish the deferred consolidation acceptance matrix

- **Area:** Tests
- **Where:** `docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md`, `dev/performance/check.py`
- **Problem:** Canonical, supplemental, profile, observer, forced and comparative checks passed on R13. Scheduler pairs failed. Focused repairs need qualification, and UDP failures and full-corpus timeouts remain unexplained. Private/resource close remains unrun.
- **Recommendation:** Complete G1's remaining queue on one qualified source, then publish the accepted baseline and retire obsolete infrastructure under XSH-19.
- **Unmet:** Full combined qualification remains manual-only on a stable source. The known XSH-29 deadline causes have focused written/desugared proof; that does not supply full-corpus acceptance. The replacement image and repaired launcher are available, and historical passes do not qualify them.
- **Fix with:** orchestrate
- **Effort:** M

### XSH-14 · P3 · Reuse import-graph checks across lint candidates

- **Area:** Performance
- **Where:** `crates/xsht/src/lint.rs`, `crates/xsht/src/cli`
- **Problem:** Each lint candidate probe parses, loads, and checks the unchanged import graph again. Large fix runs repeat the same work for every candidate.
- **Recommendation:** Measure shared checking, grouped candidates, and clean-corpus probe skipping. Keep the smallest design that preserves diagnostics and fix safety.
- **Unmet:** Collect guarded CPU timing when the host is quiet. Matched ef7 release products and candidate `97e361171` are preserved in `.work/wt/finish-lint`, outside integration. Current diagnostics and focused controls match, the release RSS gate passes all three workloads, and binary size grows 98,216 bytes. Unrelated proof/build work prevented timing before the first sample; no speedup is qualified.
- **Fix with:** make-it-fast
- **Effort:** M

### XSH-19 · P3 · Retire consolidation-only gate infrastructure after acceptance

- **Area:** Build
- **Where:** `dev/consolidation/winddown/gate-tools/`, `dev/performance/check.py`, `dev/util-linux/acceptance.py`, `docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md`
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
- **Problem:** Low-level descriptor/socket operations exist; the portable owned stream-I/O contract needed to replace Laputa's Python QMP helper has not been established. Do not assume an entirely new socket subsystem is needed.
- **Recommendation:** Probe current APIs on Linux and macOS, then fill only the missing connect/read/write/deadline/cancellation boundary using XSH-35/XSH-36. Framing and QMP policy belong in XSH at the consumer, not a QMP-specific Rust intrinsic. Reuse existing JSON/codec contracts rather than starting another parser.
- **Acceptance:** Fragmented/coalesced reads, partial writes, EOF, oversized input, timeout and cancellation are bounded and release resources on both supported host paths. A local socket fixture and Laputa LAP-35 demonstrate the same primitives; no futures runtime or callbacks are required.
- **Fix with:** isolate-effects
- **Effort:** M

### XSH-40 · P3 · Reduce one system failure to a hermetic native replay

- **Area:** Tests
- **Where:** `crates/xsht/`, `src/trace/`, `tests/`
- **Problem:** Scoped mocks, call recording and structured traces exist, but their ability to reproduce a real service or publication failure with no ambient-host fallback is unproven. A trace alone is not a replay contract.
- **Recommendation:** Select one counterexample from Laputa LAP-34. Use existing XSHT facilities to record necessary ordered boundary inputs/results and expected effects, then shrink the sequence to an ordinary native regression. Define input completeness, ordering and redaction. Add a minimal reusable test helper only if existing fixtures cannot express the case; no generic replay platform or campaign controller.
- **Acceptance:** The reduced case reproduces offline, fails when the repaired invariant is deliberately broken, and rejects missing/mismatched observations rather than consulting the host. Test fixtures contain no secrets. Corroborate with the real guest; mock agreement does not establish OS conformance or power-loss durability.
- **Fix with:** root-cause
- **Effort:** M

### XSH-41 · P3 · Qualify a strict configuration consumer after the Deser cutover

- **Area:** Data boundaries
- **Where:** `src/modules/json.rs`, `src/runtime/eval/`, `crates/xsh-registry/`, `tests/`
- **Problem:** The accepted Deser cutover already owns direct values, the shared validator, explicit lexical typing and structured codec failures. A separate implementation here would duplicate that work; application-level configuration admission still needs a real consumer proof.
- **Recommendation:** After the relevant campaign APIs land, qualify one Laputa service/network data schema using existing prepared schemas and exact validation. Keep trusted executable modules distinct from decoded data. Self-describing formats use decode plus validation; lexical CSV/INI conversions remain explicit as prescribed by the campaign. Reuse the canonical failure representation and preserve field paths; no second schema system or implicit coercion.
- **Acceptance:** Unknown/duplicate fields, missing versus null values, invalid variants/ranges and unsupported representations fail before application. Valid common-domain inputs yield equivalent normalized values without claiming all formats are interchangeable. Report any necessary boundary repair to the campaign owner while it is active; do not widen its seven-codec inventory.
- **Coordination:** Laputa LAP-5 owns the service schema and operational checks; its immediate unknown-field fix need not wait for Deser.
- **Fix with:** single-source-of-truth
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
- **Recommendation:** After XSH-40's initial replay slice, reuse its ordered, complete, secret-redacted observation contract for one additional system-report or Laputa PM/network consumer. Separate pure policy/model derivation from narrow live host reads only where that reduces existing machinery. Use existing `FsRoot`, typed mocks and trace boundaries; don't introduce a second runtime or generic host-injection framework.
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
- **Where:** `docs/DESIGN.md`, `docs/ARCHITECTURE.md`, `docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md`, one selected codec or host-capability change
- **Problem:** Binary-size/startup gates already exist, but they do not expose ownership proliferation, parallel representations, repeated semantics or Rust/native glue added for one XSH-facing capability. A raw line-count ceiling would punish reuse of trustworthy dependencies and tests.
- **Recommendation:** On one real accepted change (the Deser cutover or a bounded host API), record before/after first-party source ownership, runtime variants and operations, representations/conversions, duplicated invariants, required crates, product size and startup, and one end-to-end XSH consumer. Identify machinery retired and any complexity moved outward; use a small review table in the owning existing document, not a separate metrics framework.
- **Acceptance:** Independent reviewer can trace added, retained and deleted responsibilities to code and tests; no compatibility or verifier boundary is counted as removable merely because it duplicates a check. Record a concrete simplification or reject the abstraction on evidence, without blocking unrelated fixes on a new arbitrary LOC quota.
- **Coordination:** Laputa LAP-39 applies the same method to one userspace consumer; existing performance gates remain authoritative.
- **Fix with:** ultratriage
- **Effort:** S
