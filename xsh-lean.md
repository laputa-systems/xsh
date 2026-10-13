# XSH semantic soundness campaign

**Status:** Active foundations, authorized 2026-10-13 after preflight. Toolchain setup, private bridge fixtures, provisional inventory, and independent scalar and environment proofs run while consolidation finishes, with disjoint ownership and coordinated build/memory slots. The owner requested wider staggered lanes on 2026-10-13. Consolidation acceptance remains an independent gate. Source freezing, Rust semantic repairs, conformance, and dependent proof closure use the actual post-consolidation tree.
**Prepared:** 2026-10-13. **Checkout:** `/home/josh/d/laputa-systems/xsh`.
**Scope:** Formalize the whole frozen XSH core in Lean, prove its semantic and lowering obligations, and connect those independent statements to the Rust implementation with conformance and certified artifacts. The project now contains foundational word/identity and scalar proofs. They do not establish whole-language soundness or Rust conformance.

## Deliverables

1. A complete, drift-checked inventory of the post-consolidation core, its formal definitions, theorem roots, trusted boundaries, and four separate evidence classes.
2. A total independent Lean model, checker, operational semantics, and proofs for the whole inventoried core, including validation, effects, calls, control, cleanup, resources, streams, raw IR, and lowering.
3. A development-only source/IR export and independently checked translation path, plus actual Rust conformance, fixed adversarial cases, minimized regressions, and kernel-certified canonical artifacts.
4. Reproducible fixed gates, a bounded discovery runner, audit controls, current canonical documentation, and G10 aggregate theorems on a fresh checkout.

## Charter

The implementation remains Rust. Lean is a development and verification tool: no Lean product dependency, ordinary-build prerequisite, production evaluator, compiler service, or runtime fallback. A formal executable oracle is allowed only under `formal/`, and its executable results remain empirical evidence unless a kernel-checked theorem links them to the definitions.

Run G0–G10 to G10. Every core rule stays in scope even when difficult. Development may use explicitly named fragments, but no fragment theorem closes a full-language obligation. Do not turn a universal theorem into a finite corpus claim. Brief progress updates may report evidence or a parked item; they do not pause work for routine approval. Park only the affected item for an irreversible/outside-repository action, a forbidden boundary, an incompatible redesign, or a loop that cannot produce a check; continue independent work and leave a concrete next probe.

Use the repository's `orchestrate` campaign procedure for lane selection, merges, and review. Each lane owns an end-to-end capability, its tests and docs, and an exclusive `formal/Xsh/<domain>/` directory. No nested delegation unless the integrator assigns it. The integrator owns shared semantic interfaces, `formal/Xsh.lean`, `formal/coverage.json`, `formal/README.md`, gate/CI routing, and cross-workstream Rust files. Changes to any shared production path are serialized in its owning lane. Reviewers are read-only and need no build slot.

All worktrees go under `.work/wt/<lane>` and scratch under `.work/tmp`; before the first such write, ensure `.work/` is ignored in `.git/info/exclude`. Set `TMPDIR=.work/tmp` for commands that spill large files. Do not use `/tmp`. Commit coherent campaign work with Conventional Commit messages; pushing is forbidden. Once a lane is integrated and its merged-tree checks pass, remove its worktree and branch. Do not leave generated corpora, traces, or stale pass reports in the repository.

### Evidence and trust

Keep evidence distinct in the ledger:

- `SPEC_PROVED`: kernel-checked theorem over the independent definitions with explicit premises and audited transitive axioms.
- `RUST_CONFORMANCE`: actual Rust and formal behavior agree for named inputs, schedules, binaries, and gates; this is finite evidence.
- `ARTIFACT_CERTIFIED`: a closed proposition about identified source/IR bytes has a kernel-checked certificate using the verified validator.
- `EXTERNAL_ASSUMPTION`: an explicit named primitive, host, platform, or tool boundary contract.

All production proof roots must build and pass a separate fresh kernel recheck. The only allowed foundational axioms are those actually needed from `propext`, `Quot.sound`, and `Classical.choice`. Audit the transitive dependency closure independently: bundled fresh checking does not enforce this allowlist. Reject `sorry`, `admit`, `sorryAx`, project axioms, native-evaluation axioms, unsafe proof escapes, and theorem statements whose definitions smuggle the conclusion into a premise. Keep all required roots under the root import `Xsh`.

For each major theorem, review its elaborated statement and definition closure, including `WellTyped`, `InternalFault`, `HostConforms`, `SemanticWF`, and `TraceCorresponds`. Require inhabited host contracts, accepted/rejected witnesses, malformed artifacts, and mutations that the comparison detects. `OutOfFuel` is a runner result, never a language result. A theorem without a witness and an adversarial counterexample is not accepted.

### Owner decisions — 2026-10-13

1. **Float equality:** XSH `==` and `!=` use IEEE value equality: `+0 == -0`, and every NaN comparison is unequal, including identical NaN bit patterns. Scalar, structural, constant-folded, and fuzzer-semantic comparisons must agree. Preserve raw Float bits in serialization and preserve IEEE total-sort order. The future campaign is authorized to make the smallest runtime/equality repair and add native and formal regressions. Do not apply it during this preflight or current consolidation.
2. **Lexical effect bounds:** Preserve the existing static, lexical `without` contract. Checked/classified calls and callbacks are charged at their lexical source invocation; a delayed stream producer's body obligation is charged at its creation/call origin and its causal obligation follows the suspended work through consumption. This is not a runtime sandbox or a consumer-side effect API. `EFF-LATENT` and `EFF-SEMANTIC` must prove the stated connection. Do not reduce them to a graph-only result, a few calls, or a trace with no required observations. A stronger dynamic-region rule was declined and is out of scope.

### Prescribed decisions

- Pin Lean `4.34.1`, release commit `5045d0056413266e57c625dcd7c365b10e377c52`, with bundled Lake `5.0.0-src`. Use bundled `leanchecker` (named `lean4checker` before Lean 4.28); run `lake build`, then `cd formal && lake env leanchecker --fresh Xsh`. A transitive axiom audit is a separate required gate. Do not install an extra checker package.
- Use standard Lean libraries by default. Add Mathlib only for a named theorem/abstraction that is missing and materially reduces proof burden after comparing a few local lemmas. Before adopting any dependency, pin the exact revision and record license, maintenance, transitive trust, build-time, and peak-memory costs. It must not affect a product dependency, product startup, or product binary size. If a dependency is not worth its proof-build cost, remove it.
- Use a small, untyped source model and a distinct raw IR model. Keep malformed inputs representable until the independent verifier accepts them. Use explicit fuel for executable semantics; keep proofs structurally total. A single shared machine invariant composes the workstreams; do not create unrelated toy evaluators.
- Model `Int` as signed 64-bit and `UInt` as its nonnegative subset at boundaries; `Duration` is unsigned 64-bit milliseconds. Failed typed stores raise the specified uncatchable `type-error`, leave the old store unchanged, and unwind normally. `.require(UInt)` remains a recoverable validation. Keep paths as native bytes, `Str` as UTF-8, and Float as exact 64-bit bits with the equality rule above.
- Distinguish contract observations from diagnostics. Contract equivalence retains values, exact bytes and integers, Float bits where relevant, ordered requests/events, failures and blame, cleanup/resource actions, yielded items, checkpoints, suspension and divergence. Only implementation counters, unstable addresses, and timing overhead may be erased wholesale. Identity renaming must preserve a bijection, aliasing, and lifetime.
- XSH's parser/exporter and host/runtime remain empirical boundaries where not otherwise proved. Do not claim universal Rust, OS, native math-library, or external-process correctness. Name each external contract and never classify a core mechanism as external because it is hard.
- Wire schema v1 has separate closed source and raw-IR documents. Encode bytes/native paths as lowercase hex, stored `u32` words/IDs as eight hex digits, and signed/unsigned 64-bit values and Float bits as sixteen hex digits; signed integers use two's-complement bits. Strings remain exact UTF-8. Preserve raw payload order (low then high word), table order, and `IR_NONE`; name tags with their exact ordinal. Unknown schemas/fields/tags/type IDs, duplicates, missing required metadata, and malformed lengths fail closed. Rust-exported checker facts and witnesses are untrusted candidates, never accepted facts. Hashes are allowed only when binding an actual artifact/proof to its bytes, not as a campaign database.
- Prefer existing `xsh-fuzz` and test-helper boundaries. A narrowly scoped raw-IR development feature may be added only if those cannot expose private data; verify it is absent from normal products and check feature combinations. Do not widen public `libxsh`, export private `FullProgram`, add permanent instruction branches, or add a production crate just for the bridge.
- Changes to the language contract require an evidence-backed defect or proof-boundary need. Minimal demonstrated semantic repairs and test-only hooks are pre-authorized by this campaign, including the Float decision above. Preserve callers and update `docs/templates/SPEC.md`, tests, and generated docs together. An unrelated or major incompatible redesign is parked while independent work continues; never ask during execution. The SPEC template is authoritative; generated `docs/SPEC.md` is a reader artifact.

### Delegated within bounds

The integrator chooses file splits and proof lemmas within the domains and ownership table below. Fixed exhaustive domains, seed sets, worker and resource limits are specified in Acceptance. Each declaration uses Lean's default search limits first. Only after a structural diagnosis may a single theorem use local caps up to `maxHeartbeats 1000000` and `maxRecDepth 10000`; record the reason and keep global heartbeat/recursion settings unchanged. After three refuted hypotheses, bisect the input/history/code/environment. A failing item gets at most three bounded repair attempts or 60 minutes before it is parked with the error, regression, verified facts, hypothesis, and next exact probe. A parked mandatory item prevents G10 completion.

## Baseline

G0a may establish the pinned Lean/Lake/checker invocation, validate the private test-only bridge seam, and prepare schema/scenario fixtures after this preflight closes. Independent value-domain and environment/store proofs and provisional source inventory may overlap on disjoint files; review their statements and reconcile the inventory again at G0b. Keep coordinated build slots. G0b starts after consolidation closes: use the actual post-close source tree, core behavior, and measured loop costs as the baseline. Capture HEAD, clean/dirty state, current binaries and source identity, Lean/Lake/checker versions, Rust target/profile/compiler, and the pinned container. Build a fresh matching `xsh`/`xsht` pair before any process/native observation. Do not let an old verification binary certify changed source.

The known semantic red to preserve is the Float equality disagreement: runtime `FloatValue` equality uses bits, constant equality uses IEEE `f64` equality, and the independent fuzzer accepts either bit equality or IEEE equality. After consolidation closes, run fresh source-level witnesses for `+0/-0`, same-bit NaN, and structural Float values, and record each actual result. They remain visibly red until the authorized minimal repair and must never be entered as a passing baseline.

Record measured startup, exact-name native gate, fresh Rust build, Lean changed-module check, and clean Lean build as each becomes available. Warm changed Lean-module check target: at most 60 seconds. Exact-name Rust/native check target: at most 30 seconds unless an external/process boundary requires more. If a repeated core loop exceeds 120 seconds or twice its measured baseline, repair the loop before broad lane work. Do not report a benchmark claim from one run. Performance is measured and logged during parity work; optimization gates begin only after feature and test parity. Final performance gates run only when `perf-gate-ready` passes immediately before and `perf-gate-ready -1` passes immediately after; if the host remains busy, report `perf gate deferred: host busy`. Memory and binary-size gates always run.

G0 writes the only machine-readable evidence inventory, `formal/coverage.json`, consumed by the gates. It records actual commands/results, versions, fixed-case counts by outcome, theorem roots and axiom audits, statement-drift review, artifact identities only where required, and remaining assumptions/baselines. Do not add `STATUS.md`, duplicate receipts, hashes of unrelated state, or a second register. Keep short append-only decision and phase logs in this document.

## Sequence and ownership

The integrator selects slices using the repository's campaign procedure. Each gate
owns the complete capability, its tests and docs, and an exclusive formal domain.
The integrator owns shared interfaces, the root import, coverage inventory, gate
routing, and cross-workstream Rust paths.

### G0 — Ground (integrator only)

Start G0a after this preflight closes. Validate the pinned toolchain/checker
invocation and prepare a private bridge fixture with exact integer, Float-bit,
and native-byte round trips. The fixture must map arena names to text and Lean
binders deterministically and test parser/printer projection; it must not
export private runtime structures through a product API. FullStore, FullProgram,
and FullVerifier are private and have no portable encoder. Prefer a
`#[cfg(test)]` exporter/decoder beside verifier tests; do not use the
default-enabled `native-tests` feature. G0b starts after consolidation closes:
rebuild the post-close source; inventory every core syntax/type/call/control/
stage/IR variant and the exclusion boundary; retain Float red witnesses;
validate closed wire/scenario forms, proof root, audit controls, gate routing,
and loop costs. Rust's typed `FullTag` cannot carry arbitrary unknown raw tag
bytes: reject those at the Lean transport boundary and cover typed-representable
corruptions in Rust. No unresolved core item may disappear from
`formal/coverage.json`.

### G1 — Semantic foundation (one dedicated owner)

Own `formal/Xsh/{Syntax,Values,Semantics,Proofs}` and the shared invariant.
Define untyped syntax, exact values, environments/stores, closed outcomes,
observations, host contracts, labeled steps, finite-fuel runner, independent
checker slice, and one actual source→lowered artifact→Rust example. Prove
`BASE-ENV`, `BASE-NUM`, `BASE-STEP`, `BASE-PROGRESS`, `BASE-RUNNER`, and
`BASE-VERTICAL`. A well-typed state must be final, step, or have an emitted
valid host request; internal stuck states cannot be relabeled as host waits.
Include valid, specified-failure, rejected and malformed-IR witnesses plus one
kernel-certified artifact. Mutations to arithmetic, a binding ID, and an IR
operand must be detected. Review shared interfaces before parallel lanes.

### G2 — Validated types

After G1 interfaces stabilize, own `formal/Xsh/Validation` and changes to
`src/sema/validated.rs`, `src/sema/check/validated.rs`, and
`src/runtime/eval/validated.rs`. Model `NonEmpty`, byte-lexical `RelPath`,
bounded integers, nominal introduction/provenance, unions, recursive `.require`,
erasure, widening and narrowing. Cover every documented preserving operation
and each non-preserving/filtering case. Prove persistent widening/mutation
separation while resource handles retain shared identity. Obligations:
`VAL-INTRO/PRESERVE/ERASE/WIDEN/STORE/REQUIRE/NARROW/NOMINAL/UNION`. Compare
checker, constant, runtime and compiled-store routes without trusting Rust
answers.

### G3 — Effects

After G1 interfaces stabilize, own `formal/Xsh/Effects` and changes to
`src/sema/check/infer_effects.rs`, `src/sema/check/effect_bounds.rs`, and
`src/sema/check/module_driver.rs`. Prove the finite lattice, implication
closure (`io` asymmetry), unknowns, recursive least fixed point, order
independence, declared contracts, error capture, negative lexical bounds,
diagnostic witnesses, and operation-level semantics. Trace delayed producers,
delegated streams, stored callables/defaults, imports, cleanup and workers to
the source call/creation that pays their obligation; preserve that causal
obligation through suspension. Obligations:
`EFF-LATTICE/SOLVE/ORDER/BOUNDS/CAPTURE/LATENT/WITNESS/SEMANTIC`. Mutate a
missing edge, unknown-as-empty handling, native classification, `io` closure,
`try` host-effect clearing, and producer provenance.

### G7a — Raw IR structure

After G1 and the actual schema inventory, own `formal/Xsh/IR/Raw` and
`src/runtime/eval/indexed.rs`, `src/runtime/eval/indexed/full/schema.rs`, and
`src/runtime/eval/indexed/full/decode.rs`. This structural lane may run with
G2/G3. Model exact tags, payload widths, one-based IDs and absence sentinels,
ranges, pools, patterns, drivers, stages, locations, and arbitrary malformed
tables. Independently define structural well-formedness; prove safe total
decode, finite verifier termination, encoding round-trip and structural
acceptance. Cover invalid/reserved tags, max/zero IDs, overflowed ranges, bad
pools, invalid cycles and slot bounds. A verifier accepting no programs fails.

### G1b — Bridge and transport

After G1, own `formal/Xsh/Conformance` and bridge files in existing dev/test
crates; independent of G2/G3. No portable encoder/decoder exists for private
`FullStore`/`FullProgram`/`FullVerifier`; use a `#[cfg(test)]` exporter and
decoder beside verifier tests, not the default-enabled `native-tests` feature
or a product API. Implement closed v1 source and raw-IR exchange, exact
round trips, explicit source/artifact association, arena name-to-text export,
stable Lean binder mapping, and parser/printer projection checks. Rust's
`FullTag` cannot represent arbitrary unknown tag bytes: Lean rejects unknown
transport tags, and Rust corruption tests cover typed-representable cases.
Compare actual component decisions and one actual Rust source route
incrementally. Candidate facts and witnesses remain untrusted; an invalid
adapter response is not XSH behavior.

There is no general host-injection API. `test.mock` covers selected DNS/network
operations and `NativeTestHost` reaches nested script/trace behavior. Add only
the per-operation fake or private seam required by a concrete fixture; do not
claim broad host injection. Keep generated execution command-free until a
concrete containment seam and independent sandbox test exist; effect checking
alone is not containment. Use typed `TraceEvent` observations and preserve
event order, parent IDs, spans, and payloads. Timing values alone may be
normalized.

### G4 — Complete core typing

After G2 and G3 interfaces, own `formal/Xsh/Typing` and changes to
`src/sema/check.rs`, `src/sema/check/`, `src/sema/arguments.rs`, `src/loader.rs`,
and `src/project.rs`; coordinate a shared Rust file before editing it. Complete
declarative and independent algorithmic checking for expressions, statements,
blocks, patterns, declarations, callables/producers, modules, constants, and
typed stores. Cover bounded local inference, expected/inferred types, callee
and argument evaluation order, source-order argument/default/rest binding,
dynamic validation before frame entry, nominal/program identity, context
save/restore, module prepare/install, exact versus open export/effect contracts,
and entry argument domains. Prove `TYPE-CHECK/MEMBER/CALL/MODULE/FACTS/PRESERVE/
PROGRESS/RECOVERY/ENTRY`. Any resource/stream fragment is temporary and must
close after G5/G6.

### G5 — Control and resources (one coupled lane)

After full G4 typing, own `formal/Xsh/{Control,Resources}` and the runtime
outcome/scope/process owners found in the current architecture. Derive a
complete outcome×handler table from current SPEC and source, including which
failures are catchable and which boundaries are uncaught. Prove propagation
targets; data `Result` versus failure; ordinary versus explicit resource-scope
escape; evaluator/resource identity; aliases, foreign/stale IDs; partial
acquisition unwind; defer binding-at-registration/current-value-at-run;
reverse order; dynamic `errdefer`; primary/secondary errors; retry cleanup;
deadlines/checkpoints/defer masking; cancellation, hook and shutdown
initialization. State resource-action repeat/idempotence rules and make no
filesystem symlink-confinement claim. Eventual-cleanup claims state progress
premises; at-most-once attempts are unconditional. Prove
`CTRL-DEST/RESULT/DEADLINE/SIGNAL`, `RES-OWNER/TRANSFER/EXPLICIT/CONSUME`,
`CLEAN-ORDER/ERROR/ONCE`.

### G6 — Streams and workers (one lane)

After G5's invariant and G3's latent-effect interface, own
`formal/Xsh/Streams` and changes to
`src/runtime/eval/lowered_run/indexed_run/{producer,serial_pipeline,pipeline}.rs`.
Model not-started/running/suspended/exhausted/failed/closed producers, one-pass
aliases, retained program identity, delegation, pull demand, materialization,
every stage schema and callback contract, bounded schedules, input-order output
and failure selection, cancellation and drain. Preserve source order and
exactly-once evaluation for option defaults, shadowing of a user's callback
variable, and fold initialization in its outer scope. Treat `Result` values in
fold accumulators as data unless the operation specifies propagation. Worker
failure selection distinguishes independently failing workers from failures
caused by cancellation or stopping after another failure. Prove
`STREAM-CURSOR/TYPE/DEMAND/CLOSE/LATENT/SOUND` and `PAR-BOUND/OUTCOME/DRAIN`.
No list-only substitute, OS fairness, or data-race claim.

### G7b — Semantic IR

The G7 owner proceeds after G7a and G4–G6 interfaces. Independently define
`SemanticWF`; prove typed operand/slot/callable/control/transfer/guard
provenance and the execution invariant. Prove `IR-TYPED/PROVENANCE/GUARDS/
EXEC-SAFE`. Keep this distinct from structural safety and facts justified only
by checked construction.

### G8 — Desugaring, lowering and simulation

After the relevant G2–G7 interfaces, own `formal/Xsh/Lowering` and changes to
`src/syntax/parser/sugar.rs`, `src/runtime/eval/lower.rs`,
`src/runtime/eval/indexed/full.rs`, and both indexed execution modes. Independently
define sugar expansions and checked-core→raw-IR compiler. Prove hygiene,
one-time operands, source order/blame, fresh slots, no placeholders, checked
facts and guard preservation. Source/IR simulation includes observations,
errors, cleanup, resources, producer states, checkpoints, suspension and
divergence; prove no target-only infinite administrative stutter. Prove the
eligible in-place route (the current architecture cap is 128; revalidate it at
execution) agrees with frame execution. Finish `validateTranslation` soundness
and coverage of every supported core lowering pattern. Obligations:
`LOWER-SUGAR/WF/FACTS/STEP/OBS/DIVERGE`, `MODE-EQUIV`, `CERT-SOUND/COVERAGE`.

### G9 — Conformance and adversarial closure

After G1–G8 capabilities, integrator and read-only reviewers close independent
component checks, actual source execution, and actual lowering artifacts through
both Rust execution modes. Fixed/generated cases, corruption, mutation,
reductions and certificates leave no unexplained frozen-core unsupported input.
Retain each minimized regression and nearest native test. Do not replace the
existing independent fuzz oracle with the same Lean code on both sides.

Kernel-certify the canonical example for every supported lowering scheme, all
designated cross-feature fixtures, and every retained semantic regression in
the certificate domain. Large stress cases may use the compiled validator but
must be labeled empirical, never `ARTIFACT_CERTIFIED`.

### G10 — Composition and clean-room close

After G0–G9, compose `XSH-CHECK-SOUND`, `XSH-SOURCE-SAFE`, `XSH-EFFECT-SAFE`,
`XSH-RESOURCE-SAFE`, `XSH-IR-SAFE`, `XSH-COMPILE-CORRECT`, and
`XSH-CERTIFIED-ARTIFACT` over one common invariant. Rebuild from pinned
tools/sources, recompute inventory, audit every root, run negative controls,
fixed and bounded exhaustive gates, critical mutants, representative fresh
fuzz, matching Rust/native regressions, mode comparisons, certificates,
canonical-doc generation, and final release nonregression. No core item may
remain planned/test-only/assumed where proof is required.

The first independent wave after G1 is G2, G3, G7a structural IR, and G1b
bridge/transport: width four if all report ready. Thereafter launch every
disjoint ready slice as soon as its prerequisites merge; do not impose a fixed
small lane cap. Bound expensive commands with build slots: at most two
concurrent Lean builds, each 4 CPU and 8 GiB; at most one Rust build, 12 CPU and
24 GiB. Total reserved build use is at most 20 CPU and 40 GiB on the 32-CPU/
60-GiB host. Reviewers use no build slot. Run at most one full native suite at
once. At launch, lower concurrency if the host is smaller; never exceed these
limits on the specified host.

Each capability is end-to-end: formal definition/proof, actual exporter or source route, positive and adversarial native/conformance cases, applicable docs, and incrementally updated coverage. Shared ownership and merge rules follow the `orchestrate` campaign procedure; the table supplies the XSH-specific files, prerequisites, and initial maximum width.

## Acceptance contracts

### Semantics and theorem coverage

The model covers the complete frozen core after parsing: sugar and binding identity; imports/module generations; core values/types and validated types; constants; callable signatures/defaults/argument plans; effects and lexical negative bounds; dynamic checks and typed stores; all result/control/recovery forms; resources/cleanup/deadlines/signals; producer/stage/worker behavior; raw indexed representation; verification, lowering, and both execution modes. System-report, applets, applications, native library internals, external programs, OS race freedom, Linux/syscalls, LLVM/Rust compiler, and `xshi`'s separate interactive policy are outside the denominator. A core path shared by `xshi` remains in scope.

For execution, define configurations that account for program identity, syntax/current continuation, environment and typed store, calls/blocks/scopes/cleanup, resources, stream and worker state, evaluator cwd/env overlays, deadlines/signals, outcomes, observations, and pending host request. Prove preservation and progress modulo only specified failures, emitted host requests, legitimate suspension, and divergence. A rejected store commits nothing. Host requests are operation-indexed and responses are constrained; distinguish safety claims from liveness claims and state progress premises for liveness.

For lowering, independent source and IR machines relate every permitted target behavior to the source contract. Deterministic fixed scenarios require observation equivalence; nondeterministic schedules use an explicit allowed-schedule/refinement relation. Do not normalize away missing output, reordered specified events, different failures/blame, aliasing, Float distinctions, or interruption checkpoints. Parser projection, exporter correctness, Rust execution, and native library behavior remain separately identified empirical or external links.

### Adversarial and nonvacuity set

For every core rule require an inhabited positive witness and an adversarial/rejected witness where applicable. Fixed cases cover: signed integer edges and division; UInt store atomicity versus `.require`; bounded ranges, `RelPath` components and NUL; `NonEmpty` filtering and widening mutation; nominal identity and overlapping unions; nested schema failures and no partial publication; shadowing/narrowing; named/default/rest calls; exact/open module contracts and generations; check-before-entry; recursive/unknown/captured/negative/latent effects; result data versus propagation; defer capture, later-failure `errdefer`, cleanup ordering and primary/secondary failures; resource escape/explicit release/aliases/foreign IDs; deadlines, signal hooks and uninitialized `main`; producer creation, aliasing, delegation, over-pull and close order; worker bounds/order/failure/drain; malformed tags/IDs/ranges/pools/provenance/guards; wrong lowering order, lost cleanup/blame, in-place/frame boundaries; and exact wire-byte, unknown schema, duplicate-ID, timeout/out-of-fuel categories.

Retain deliberate cross-feature products for (1) validated value × typed callback × imported module × top-level binding; (2) resource in error cause × nonlocal exit × `try`/`retry` × cleanup; (3) shared producer × negative effect region × early close × defer; (4) UInt/dynamic parameter × failed store × `within` × `errdefer`; (5) bounded parallel imported callback × independent failures × cancellation; and (6) explicit resource `with` × escaped alias × ordinary outer ownership. Name the shared invariant composing each product.

The mutation gate kills at least these 16 classes: missing typed-store guard; missing effect edge; unknown treated as empty; dropped `io` implication; `try` clears host effects; lost latent producer provenance; swapped effectful argument order; refinement retained after filtering; wrong nominal/import provenance; dropped/reversed cleanup; data `Err` treated as propagation; serial source over-pulled; wrong parallel failure selected; out-of-range IR accepted; dynamic guard omitted; observation normalization hides a missing event. Record detecting gate and witness for each. Add a mutant when an actual regression exposes a new class. For compilation, effects, and cleanup, include a deliberately wrong lowerer, missing-edge solver, and bad release ordering respectively. Host contracts must have a concrete inhabited mock and admissible reachable responses.

Fixed enumeration is finite and explicit: at most 2 declarations and 2 modules; 1–6 syntax or instruction nodes; type depth 0–3; 0–2 nested containers with at most 2 elements and 3 bytes per byte/string/path payload; 0–3 arguments; scope depth 0–3; at most 2 resource identities; worker count 1–2; scheduler length 0–6. Enumerate each constructor and each named two-feature interaction within these bounds, not the impossible full Cartesian product. Report candidates considered, filtered with reasons, completed, and result categories; fail on missing constructor coverage or unexpectedly low admission. Proofs remain universal over their mathematical domains.

The fixed generator uses seeds `0x58534800` through `0x58534807` inclusive, at most 128 candidates per seed (1,024 total), plus every canonical witness, retained regression, and named mutant. Run generated source only with a cleared environment, isolated temp root, mock host, no real commands, 2-second child timeout, and 64-KiB per-case output cap. Discovery runs accept an explicit seed, case count, and jobs; use at most 2 workers, 500 candidates per worker, 2 GiB per worker, and 15 minutes per invocation. Timeouts, out-of-fuel, unsupported, rejected, and harness failures have separate counts and never become passes. A divergent fixture is checked by a finite prefix property, not by waiting for process timeout.

### Commands and repository gates

The following are planned new commands, not current commands. `cargo dev formal` has closed operations only: `check`, `test`, `fuzz --seed U64 --cases N --jobs J`, `replay PATH`, `certify PATH`, and `all`. Integrate `formal/`, fixtures and harnesses into `cargo dev gate PATH...`; unknown paths must not bypass a gate. No ordinary Cargo product build invokes Lean. Direct Lake work must work without bootstrapping XSH.

| Command | Required behavior |
|---|---|
| `cd formal && lake build` | Build the complete proof roots and executable definitions. |
| `cd formal && lake env leanchecker --fresh Xsh` | Recheck the complete root after build. |
| `cargo dev formal check` | Toolchain/inventory check, complete Lake build, statement/import/axiom audit, and fresh recheck. |
| `cargo dev formal test` | Fixed component/source/artifact corpus and audit/harness negative controls. |
| `cargo dev formal fuzz --seed U64 --cases N --jobs J` | Bounded reproducible discovery; print resolved limits and keep outcome classes separate. |
| `cargo dev formal replay PATH` | Replay one retained case against matching source, artifact and schema identities. |
| `cargo dev formal certify PATH` | Kernel-check a concrete certificate; reject empirical-only/native-axiom-only results. |
| `cargo dev formal all` | Fixed close gates, designated exhaustive/mutation suites, certificates, and affected repository gates. |

The Lean project lives in `formal/` with `lean-toolchain`, `lakefile.toml`, `Xsh.lean`, `Main.lean`, `README.md`, `coverage.json`, and focused modules. No `STATUS.md` or second campaign plan. `README.md` documents direct proof, test, replay and fresh-check commands. Keep proof dependencies out of `Cargo.toml` product paths. Add `Mathlib` to an exact lock/manifest only if the dependency decision above is met.

Use native XSH tests first. Linux evidence uses `Dockerfile.test`, x86_64 musl, and the current `docs/TESTING.md` flags; process/native suites use the verification profile, with release reserved for final/performance verification. Build only the package/binary needed. Use `cargo dev gate PATH...` for current owner routing and name exact native tests with `xsht test --exact`; Rust tests cover host boundaries native tests cannot express. A phase gate runs the tests it touched. Full formal/native fixed suites run at G1 vertical closure, G8/G9 composition, and G10 close, each for its stated integration risk. Do not run workspace-wide checks by default.

The fixed formal suite has a 30-minute limit, each Lean build slot 8 GiB, each Rust build slot 24 GiB, and discovery limits are above. A clean-room final build has a 45-minute cap; exceeding a cap is infrastructure/inconclusive evidence to diagnose, never a pass. Retain per-obligation Lean defaults; the only local proof cap is the bounded override above. Kill and reap children on timeout or harness failure. Existing source/native resource and product performance bounds remain in force throughout.

## Contract changes

Pre-authorized: a minimal, demonstrated semantic repair that preserves established callers, updates canonical prose/tests/formal obligations, and a test-only development seam needed for real comparison. This includes the IEEE Float repair above. Do not change `without` into a runtime effect system, convert specified runtime failures into recoverable values, remove a core form, broaden supported syntax, change production architecture, or claim a stronger Rust/OS boundary. A major incompatible redesign or unrelated improvement is parked with its counterexample, observable alternatives, compatibility impact, and next decision needed; dependent work stays open and independent work continues. No push, external merge, deployment, or message to another person is in scope.

## Non-goals

No proof target for system-report, core applets, user applications, UI, or `xshi` policy; no universal Rust/LLVM/OS/syscall/filesystem-race/external-command/native-library proof; no runtime sandbox from `without`; no production Lean runtime or evaluator; no speculative AST, JIT, async runtime, public compiler API, task service, parallel coordination daemon, compatibility shim, loose serialization fallback, corpus-only replacement for a theorem, or performance optimization before parity.

## Reference

### Live repository contract and owner map

This map was checked at `120f8b44e98b4fa3cb87ab2ee6fc75da2b3d07c4`; the campaign must resolve symbols again at the post-consolidation start. No old commit or line number is a contract.

| Contract area | Current authority and source owners |
|---|---|
| Language prose and examples | `docs/templates/SPEC.md` and `docs/snippets/spec/`; generate `docs/SPEC.md` with documented docs build. Relevant sections: 4 types/numbers/validated types, 6 control/calls, 8 results/cleanup, 9 producers/effects, 10 scopes/deadlines, 11 resources, 12 signals, 13 streams. |
| Architecture and invariants | `docs/ARCHITECTURE.md`: Pipeline, Invariants, Adding a validated type, Runtime design. Keep checked-once→published facts→lower→`FullVerifier::verify`→indexed execution. |
| Build and test policy | `AGENTS.md`, `docs/TESTING.md`, `Dockerfile.test`. Use the narrowest current owner gate; Linux evidence is x86_64 musl in the pinned container. |
| Grammar, parsing and sugar | `src/syntax/grammar.rs`, `src/syntax/grammar/productions.rs`, `src/syntax/arena.rs`, `src/syntax/parser/sugar.rs`; `ArenaStmtKind::Sugar`, `ArenaProgramBuilder::push_sugar`. |
| Checking, facts and types | `src/sema/check.rs`, `src/sema/check/compact.rs`, `src/sema/check/checked_declarations.rs`, `src/sema/constraints.rs`, `src/sema/validated.rs`, `src/sema/constants.rs`, `src/sema/arguments.rs`, `src/sema/stage_arguments.rs`, `src/loader.rs`, `src/project.rs`. |
| Effects, bounds and streams | `src/sema/check/infer_effects.rs`, `src/sema/check/effect_bounds.rs` (`enter_block_effect_bound`, `check_effect_not_excluded`), `src/sema/check/stream.rs`, `src/sema/check/module_driver.rs`; `AstArena::block_effect_bound`. |
| Lowering, indexed schema and verify | `src/runtime/eval/lower.rs`, `src/runtime/eval/indexed.rs`, `src/runtime/eval/indexed/semantic.rs`, `src/runtime/eval/indexed/full.rs` (`FullVerifier::verify`, `verify_callable_slot_initializers`), `src/runtime/eval/indexed/full/schema.rs`, `src/runtime/eval/indexed/full/decode.rs`. |
| Execution and host lifecycle | `src/runtime/eval.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `indexed_run/explicit_run.rs`, `indexed_run/producer.rs`, `indexed_run/serial_pipeline.rs`, `src/runtime/process.rs`, `src/runtime/run.rs`, `src/runtime/signal.rs`, `src/runtime/value/resource_values.rs`, `src/trace.rs`. |
| Float mismatch | `src/runtime/value.rs::FloatValue::eq`, `src/sema/constants.rs::constant_values_equal` and constant `B::Eq/B::Ne`, `crates/xsh-fuzz/src/eval.rs::values_equal`; current native witnesses include `tests/xsh/basic.xsh` and `tests/xsh/comparison-chain.xsh`. |
| Existing behavior/conformance cases | `tests/xsh/{effects,private-proc-effects,resource-ownership,error-resource-ownership,with-resource,stream-producers,checker-streams,lowering-coverage,signal-hooks}.xsh`; `tests/grammar.rs`; verifier corruption tests in `src/runtime/eval/indexed/full.rs`; independent soundness oracle and harness under `crates/xsh-fuzz/`. |

The existing runtime applies verified indexed execution, uses heap-backed frames and a verifier-bounded in-place path (currently height 128), and retains program ownership for suspended producers. Resource/effect/lowering facts are behavior-bearing; prove their preservation rather than expanding hot rows by default. Re-resolve callable provenance and temporary-writer verifier checks at campaign start.

### Proof and evidence references

Lean's bundled fresh checker is `leanchecker --fresh`; it was introduced in Lean 4.28 and was previously named `lean4checker`. The local preflight environment reports Lean 4.34.1 release commit `5045d0056413266e57c625dcd7c365b10e377c52` and Lake `5.0.0-src`. A smoke invocation on `Init.Prelude` exited 0; this establishes checker availability only, not a project build, theorem, or proof. Run the full `Xsh` root after the formal project exists.

### Append-only decision log

- 2026-10-13 — Owner chose IEEE Float equality across all semantic paths; authorize only the minimal repair and regressions during the future campaign.
- 2026-10-13 — Owner confirmed lexical `without` bounds and producer-origin effect charging; retain causal latent-effect proof, reject runtime-sandbox redesign.
- 2026-10-13 — Pin Lean 4.34.1/Lake 5.0.0-src; bundle the checker and audit axioms separately.
- 2026-10-13 — No Mathlib by default; use only after a named missing theorem and a bounded maintenance/trust/build-cost case.

### Append-only phase log

| Phase | Commit/revision and exact evidence | Open items / next action |
|---|---|---|
| Preflight | Lean 4.34.1 fresh `Init.Prelude` recheck; pinned release hello execution/JSONL trace; private typed-callable verifier regression passed. `mdtool check --untracked xsh-lean.md`: zero errors; planned-path/symbol warnings and known dangling-fixture notes. Independent review found no P1/P2 blocker. | Exporter and project do not yet exist; G0a proves those foundations before dependent work. |
| G0a | Pinned project, module-wide axiom audit, guarded fresh-check runner, and exact scalar foundation merged. Private Rust bridge and provisional inventory have focused checks and independent review; integration fixes remain. | Include every theorem module in the fresh-check root. Finish reference and inventory mutation controls; G0b waits for consolidation close. |
| Staggered foundations | Scalar bit/key round trips, lexical lookup and scope restoration, cell-reference captures, and membership-guarded store laws merged. The provisional inventory checks 77 owner sets and leaves all 72 complete theorem obligations planned; its 13 mutation tests pass. The private bridge has strict JSON limits, table bounds, and ghost-reference rejection tests. | Whole syntax, typing, operational semantics, the common invariant, Rust conformance, and artifact certification remain open. |

## For the owner afterwards

Review theorem statements and `formal/coverage.json`, especially each `EXTERNAL_ASSUMPTION`, actual Rust/artifact identity, and any parked mandatory item. The campaign is complete only with G10 on a fresh identified checkout. After this preflight is accepted, start G0a with disjoint ownership and coordinated build slots; start G0b from the post-consolidation tree.
