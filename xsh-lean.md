# XSH semantic soundness campaign

**Status:** resumed by the owner on 2026-10-29. G0b is recorded: the source
is frozen and the first finite Rust conformance cases have run. Next is G1b's
test-only bridge and the connected milestone below.

**Resume:** complete one connected
compiler proof milestone (see [the typing and formal successor
requirements](dev/consolidation/CAMPAIGN.md#typing-and-formal-successor-requirements))
while retaining the full-core obligations and the post-consolidation source
freeze (G0b). Typing acceptance does not wait for this campaign. A typing-only
resume leaves Lean paused.

**State:** `formal/coverage.json` is the only inventory. It lists 793
fragment theorem roots across 92 owners, all `SPEC_PROVED` finite models of
selected behavior. None establishes source, raw-IR, Rust, OS, or clock
correspondence. All 77 universal obligations stay planned. G1's five base
theorems (`Xsh.Proofs.Base.{Env,Num,Step,Progress,Runner}`) are fragment
roots: they hold for the base expression machine, which is not the frozen
core.
`source_identity` records the frozen commit `65fcb0f42` and its tree, and
`finite_conformance.cases` holds the four Float equality tests, run once each
with a fresh verification `xsh`/`xsht` pair built from that commit in the
pinned container. Each
fragment's exclusions are recorded beside its roots. `python3
formal/scripts/check-inventory.py` validates the inventory against this plan.

**Open items:**

- BASE-VERTICAL stays planned. G1b's first stage exists: a test-only exporter
  and decoder (`formal_bridge/arithmetic.rs`) emit the real compiler's
  lowering of `tests/fixtures/runtime/formal-bridge-integer-locals.xsh`, an
  integer `let` and an arithmetic row over its slot, and
  `Xsh.Conformance.Vertical.Certificate` kernel-checks that exact
  source/raw-IR document pair. Left open: the Lean side has no source parser
  and no lowering relation, so the claim starts at the supplied IR rows and
  the source is bound by identifier only; the new Rust test postdates the
  frozen commit, so no `RUST_CONFORMANCE` case exists for it; the artifact
  has no failure outcome or malformed-IR witness inside it; the coverage
  schema has no `ARTIFACT_CERTIFIED` row, so the certificate is recorded as a
  `SPEC_PROVED` fragment root; and the IR subset is two integer functions
  without parameters, calls or control flow.
- G1's base theorems cover the base machine only: integer expressions,
  scalar literals, identifiers and one host request family (`time.now`). The
  machine has no assignment or declaration form, so typed store writes are
  proved on the environment API, not through machine steps. Float arithmetic
  and Float conversions are not modeled; the Float clauses state only exact
  bits, IEEE value equality and the total order. BASE-ENV, BASE-NUM,
  BASE-STEP, BASE-PROGRESS and BASE-RUNNER close only when their theorems hold
  for the frozen core's syntax, values and host request families.
- The record-update fragment models the checker's path-prefix overlap rule.
  `docs/SPEC.md` does not state that rule. Add it to the SPEC template or
  record an owner decision.
- `Xsh.Paths.Fields` does not match every Rust `Path.name`, `Path.ext`, and
  `Path.parent` edge case, such as `foo/.` and `foo/..`. It claims no API
  correspondence.
- No Goose implementation exists, and none is a prerequisite for any
  obligation. See [Replacement implementation
  contract](#replacement-implementation-contract).

**Scope:** formalize the whole frozen XSH core in Lean, prove its semantic and
lowering obligations, and connect those independent statements to the Rust
implementation with conformance and certified artifacts. Every semantic claim
starts after parsing, except where `SRC-ADMIT` evidence covers the source bytes
and module inputs for an admitted domain. A proof that begins with supplied
syntax is a claim about that syntax, not about the original source program.
`formal/Xsh.lean` imports every current module under `formal/Xsh/`. Root
reachability does not establish adequacy.


## Deliverables

1. A complete, drift-checked inventory of the post-consolidation core, its formal definitions, theorem roots, trusted boundaries, and four separate evidence classes.
2. A total independent Lean model, checker, operational semantics, and proofs for the whole inventoried core, including validation, effects, calls, control, cleanup, resources, streams, raw IR, and lowering.
3. A development-only source/IR export and independently checked translation path, plus actual Rust conformance, fixed adversarial cases, minimized regressions, and kernel-certified canonical artifacts.
4. Reproducible fixed gates, a bounded discovery runner, audit controls, current canonical documentation, and G10 aggregate theorems on a fresh checkout.
5. A replacement contract: frontend and runtime compatibility conditions, the artifact-validation and source-admission boundaries, and a refinement template, all stated against the independent semantics so that a second implementation can supply candidates to the same checks. A second implementation is not a deliverable.

## Charter

Rust is the production implementation and the first comparison implementation. The independent Lean contract is written so that a later implementation of the core can be compared with it without reusing Rust's representations; a Goose frontend is the first intended experiment. The SPEC template and recorded owner decisions define the contract. When Rust and the contract disagree, retain the counterexample and investigate it; resolve it by repairing the implementation or by an owner decision that changes the contract, never by redefining the oracle to reproduce Rust. Lean is a development and verification tool: no Lean product dependency, ordinary-build prerequisite, production evaluator, compiler service, or runtime fallback. A formal executable oracle is allowed only under `formal/`, and its executable results remain empirical evidence unless a kernel-checked theorem links them to the definitions.

Run G0–G10 to G10. Every core rule stays in scope even when difficult. Development may use explicitly named fragments, but no fragment theorem closes a full-language obligation. Do not turn a universal theorem into a finite corpus claim. Brief progress updates may report evidence or a parked item; they do not pause work for routine approval. Park only the affected item for an irreversible/outside-repository action, a forbidden boundary, an incompatible redesign, or a loop that cannot produce a check; continue independent work and leave a concrete next probe.

Use the repository's `orchestrate` campaign procedure for lane selection, merges, and review. Each lane owns an end-to-end capability, its tests and docs, and an exclusive `formal/Xsh/<domain>/` directory. No nested delegation unless the integrator assigns it. The integrator owns shared semantic interfaces, `formal/Xsh.lean`, `formal/coverage.json`, `formal/README.md`, gate/CI routing, and cross-workstream Rust files. Changes to any shared production path are serialized in its owning lane. Reviewers are read-only and need no build slot.

### Evidence and trust

Keep evidence distinct in the ledger:

- `SPEC_PROVED`: kernel-checked theorem over the independent definitions with explicit premises and audited transitive axioms.
- `RUST_CONFORMANCE`: actual Rust and formal behavior agree for named inputs, schedules, binaries, and gates; this is finite evidence.
- `ARTIFACT_CERTIFIED`: a closed proposition about identified source/IR bytes has a kernel-checked certificate using the verified validator.
- `EXTERNAL_ASSUMPTION`: an explicit named primitive, host, platform, or tool boundary contract.

Each class names its subject. A `SPEC_PROVED` theorem about a Lean model of an implementation proves that model, and the record says so. `RUST_CONFORMANCE` is reserved for evidence that actually ran Rust. An `ARTIFACT_CERTIFIED` record names its artifact kind (original source bytes with their module inputs, source document, raw-IR document, candidate fact set, or source/IR pair), the artifact identity, and the exact proposition checked. A certificate for emitted IR does not prove that the producing compiler is correct on other inputs, and does not prove that Rust or any other runtime executes that IR correctly.

Evidence about a future implementation records its revision, artifact kind and identity, theorem or property, checker and toolchain identity, supported domain and preconditions, the trusted extraction, compilation, runtime, and host boundaries, and the adjacent boundaries that only tests support. Compiler pins and hashes establish identity and reproducibility, not refinement. Schema 2 has no field for a non-Rust implementation. The first such evidence needs a versioned, tested schema change that keeps existing records' meaning; it may add an implementation-indexed class but must not record non-Rust runs as `RUST_CONFORMANCE`, rename an existing class, or start a parallel ledger.

All production proof roots must build and pass a separate fresh kernel recheck. The only allowed foundational axioms are those actually needed from `propext`, `Quot.sound`, and `Classical.choice`. Audit the transitive dependency closure independently: bundled fresh checking does not enforce this allowlist. Reject `sorry`, `admit`, `sorryAx`, project axioms, native-evaluation axioms, unsafe proof escapes, and theorem statements whose definitions smuggle the conclusion into a premise. Keep all required roots under the root import `Xsh`.

For each major theorem, review its elaborated statement and definition closure, including `WellTyped`, `InternalFault`, `HostConforms`, `SemanticWF`, and `TraceCorresponds`. Require inhabited host contracts, accepted/rejected witnesses, malformed artifacts, and mutations that the comparison detects. `OutOfFuel` is a runner result, never a language result. A theorem without a witness and an adversarial counterexample is not accepted.

### Owner decisions — 2026-10-13

1. **Float equality:** The XSH language semantics for Float `==` and `!=` use IEEE value equality: `+0 == -0`, and every NaN comparison is unequal, including identical NaN bit patterns. Scalar, structural, constant-folded, and fuzzer-semantic comparisons must agree. Preserve raw Float bits in serialization and preserve IEEE total-sort order. This decision authorized the minimal runtime/equality repair and native and formal regressions; the runtime, constant-folding, and fuzz-oracle repair is now integrated. The independent Lean scalar model does not establish Rust conformance.
2. **Lexical effect bounds:** Preserve the existing static, lexical `without` contract. Checked/classified calls and callbacks are charged at their lexical source invocation; a delayed stream producer's body obligation is charged at its creation/call origin and its causal obligation follows the suspended work through consumption. This is not a runtime sandbox or a consumer-side effect API. `EFF-LATENT` and `EFF-SEMANTIC` must prove the stated connection. Do not reduce them to a graph-only result, a few calls, or a trace with no required observations. A stronger dynamic-region rule was declined and is out of scope.

### Prescribed decisions

- Pin the toolchain named in `formal/lean-toolchain`, currently Lean `4.35.0` (commit `7e8881d96` moved it from the preflight's `4.34.1`, release commit `5045d0056413266e57c625dcd7c365b10e377c52`, with bundled Lake `5.0.0-src`). Use bundled `leanchecker` (named `lean4checker` before Lean 4.28); run `lake build`, then `cd formal && lake env leanchecker --fresh Xsh`. A transitive axiom audit is a separate required gate. Do not install an extra checker package.
- Use standard Lean libraries by default. Add Mathlib only for a named theorem/abstraction that is missing and materially reduces proof burden after comparing a few local lemmas. Before adopting any dependency, pin the exact revision and record license, maintenance, transitive trust, build-time, and peak-memory costs. It must not affect a product dependency, product startup, or product binary size. If a dependency is not worth its proof-build cost, remove it.
- Use a small, untyped source model and a distinct raw IR model. Keep malformed inputs representable until the independent verifier accepts them. Use explicit fuel for executable semantics; keep proofs structurally total. A single shared machine invariant composes the workstreams; do not create unrelated toy evaluators.
- Model `Int` as signed 64-bit and `UInt` as its nonnegative subset at boundaries; `Duration` is unsigned 64-bit milliseconds. Failed typed stores raise the specified uncatchable `type-error`, leave the old store unchanged, and unwind normally. `.require(UInt)` remains a recoverable validation. Keep paths as native bytes, `Str` as UTF-8, and Float as exact 64-bit bits with the equality rule above.
- Distinguish contract observations from diagnostics. Contract equivalence retains values, exact bytes and integers, Float bits where relevant, ordered requests/events, failures and blame, cleanup/resource actions, yielded items, checkpoints, suspension and divergence. Only implementation counters, unstable addresses, and timing overhead may be erased wholesale. Identity renaming must preserve a bijection on observable identities, together with their aliasing and lifetime. An identity is observable when the contract's observations can distinguish it: aliasing equality, nominal and module identity comparisons, stale-handle rejection, and resource actions. Internal slot, row, pool, arena, and allocation identities may be renamed, shared, or reused when every observation is unchanged; a reused slot must never make a previously invalid handle observably valid. The contract does not require identical internal sharing.
- XSH's parser/exporter and host/runtime remain empirical boundaries where not otherwise proved, including the parser outside SRC-ADMIT's admitted domains. Do not claim universal Rust, OS, native math-library, or external-process correctness. Name each external contract and never classify a core mechanism as external because it is hard.
- Wire schema v1 has separate closed source and raw-IR documents. Encode bytes/native paths as lowercase hex, stored `u32` words/IDs as eight hex digits, and signed/unsigned 64-bit values and Float bits as sixteen hex digits; signed integers use two's-complement bits. Strings remain exact UTF-8. Preserve raw payload order (low then high word), table order, and `IR_NONE`; name tags with their exact ordinal. Unknown schemas/fields/tags/type IDs, duplicates, missing required metadata, and malformed lengths fail closed. Rust-exported checker facts and witnesses are untrusted candidates, never accepted facts. Hashes are allowed only when binding an actual artifact/proof to its bytes, not as a campaign database.
- Prefer existing `xsh-fuzz` and test-helper boundaries. A narrowly scoped raw-IR development feature may be added only if those cannot expose private data; verify it is absent from normal products and check feature combinations. Do not widen public `libxsh`, export private `FullProgram`, add permanent instruction branches, or add a production crate just for the bridge.
- Changes to the language contract require an evidence-backed defect or proof-boundary need. Minimal demonstrated semantic repairs and test-only hooks are pre-authorized by this campaign, including the Float decision above. Preserve callers and update `docs/templates/SPEC.md`, tests, and generated docs together. An unrelated or major incompatible redesign is parked while independent work continues; never ask during execution. The SPEC template is authoritative; generated `docs/SPEC.md` is a reader artifact.

### Delegated within bounds

The integrator chooses file splits and proof lemmas within the domains and ownership table below. Fixed exhaustive domains, seed sets, worker and resource limits are specified in Acceptance. Each declaration uses Lean's default search limits first. Only after a structural diagnosis may a single theorem use local caps up to `maxHeartbeats 1000000` and `maxRecDepth 10000`; record the reason and keep global heartbeat/recursion settings unchanged. After three refuted hypotheses, bisect the input/history/code/environment. A failing item gets at most three bounded repair attempts or 60 minutes before it is parked with the error, regression, verified facts, hypothesis, and next exact probe. A parked mandatory item prevents G10 completion.

## Baseline

G0a established the pinned Lean/Lake/checker setup, private test-only bridge, provisional source inventory, and initial scalar foundation. G0b starts after consolidation closes: use the actual post-close source tree, core behavior, and measured loop costs as the baseline. Capture HEAD, clean/dirty state, current binaries and source identity, Lean/Lake/checker versions, Rust target/profile/compiler, and the pinned container. Build a fresh matching `xsh`/`xsht` pair before any process/native observation. Do not let an old verification binary certify changed source.

Coverage schema 2 keeps `source_state` provisional and `source_identity` null
through consolidation; `finite_conformance.cases` stays empty until G0b freezes
the source. At freeze, record an existing full Git commit and its exact root
tree; the commit may be an ancestor of `HEAD`. Each named `RUST_CONFORMANCE`
case carries a stable `test_id` as an exact argv element. The only accepted
commands are `xsht test --exact <test_id>` and
`cargo test --profile <profile> --target <target> -p <package> --lib
<test_id> -- --exact` or the corresponding `--test <target>` form. Source tests
require separate `xsht` and `xsh` SHA-256 roles; Cargo tests require separate
`cargo` and `cargo-test:<package>:<target>` roles. The expected result is a
predeclared normalized oracle with exactly `test_id`, `exit_code`, `passed`,
`failed`, `skipped`, and `filtered`; it requires one pass with no failure or
skip. For `xsht`, `filtered` is null because its summary has no such count;
Cargo's filtered-out count is retained. The actual result retains captured
`exit_code`, `stdout`, and `stderr`. The checker parses the named result line
and summary, ignores duration values only, maps Cargo ignored tests to
`skipped`, then compares the normalized outcome with the oracle. It rejects
zero-test success and requires exactly one named pass, with no failures, skips,
or ignored tests. The agreement is finite evidence for the named case only.
Record only cases that have actually run; all universal theorem obligations
remain independently planned until their complete proofs and required evidence
pass.

Float equality follows owner decision 1: the runtime, constant-folding, and fuzz-oracle comparisons implement IEEE value equality and keep raw bits and total-sort order. `formal/Xsh/Values/FloatEquality.lean` proves the comparator laws, and `tests/xsh/float-equality.xsh` holds the runtime and constant-folding witnesses, including signed zero, same-bit NaN, and nested values. The Lean model establishes no Rust conformance.

Record measured startup, exact-name native gate, fresh Rust build, Lean changed-module check, and clean Lean build as each becomes available. Warm changed Lean-module check target: at most 60 seconds. Exact-name Rust/native check target: at most 30 seconds unless an external/process boundary requires more. If a repeated core loop exceeds 120 seconds or twice its measured baseline, repair the loop before broad lane work. Do not report a benchmark claim from one run. Performance is measured during parity work; optimization gates begin only after feature and test parity.

G0 writes the only machine-readable evidence inventory, `formal/coverage.json`, consumed by the gates. It records actual commands/results, versions, fixed-case counts by outcome, theorem roots and axiom audits, statement-drift review, artifact identities only where required, and remaining assumptions/baselines. Do not add `STATUS.md`, duplicate receipts, or a second register. Keep the short log at the end of this document.

## Sequence and ownership

The integrator selects slices using the repository's campaign procedure. Each gate
owns the complete capability, its tests and docs, and an exclusive formal domain.
The integrator owns shared interfaces, the root import, coverage inventory, gate
routing, and cross-workstream Rust paths.

### G0 — Ground (integrator only)

G0a: the pinned toolchain, private typed test bridge, and provisional
inventory are in place. Keep validating their controls and keep every formal
module reachable from the root. G1b owns the bridge and transport rules.
G0b starts after consolidation closes: rebuild the post-close source;
inventory every core syntax/type/call/control/stage/IR variant and the
exclusion boundary; run the Float equality regressions in
`tests/xsh/float-equality.xsh` and record their results; validate closed
wire/scenario forms, proof root, audit controls, gate routing, and loop costs.
No unresolved core item may disappear from `formal/coverage.json`.

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

The semantic state is representation-independent: no rule reads Rust enum
layout, addresses, allocation order, arena organization, reference counts, or
internal IDs. Each typed host request family states its semantic inputs and
outputs, errors and propagation, ownership and borrowing duration, retention of
buffers, values or callbacks, reentrancy and permitted calling threads,
cancellation and partial completion, handle validity and stale-identity
rejection, and the caller's and callee's obligations. These contracts stay
independent of any C layout; derive a concrete ABI only for an experiment that
needs one. They do not cover native library or operating-system internals.

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

The canonical widening rule carries a validated value to its base or an
eligible structural view. A validated-to-validated assignment requires the
exact same validation identity, so distinct bounded validations do not fit
even when one accepted interval contains the other. The interval-containment
fragment is an independent mathematical relation and does not prove that rule.

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

Source admission (`SRC-ADMIT`): for a declared grammar domain, connect exact
source bytes and every module input to the independent parse tree and its
binding projection. Cover the encoding rules, literal interpretation, parse
success and failure, source positions and contractual parse diagnostics, and
module-resolution assumptions. The step from that tree to core, including
parser-performed desugaring, stays with G8's sugar obligation. The G10
theorems close it over the admitted domains they list; a source-level claim
outside those domains stays labeled as starting at syntax. Discharge it per
admitted production set by an independent parse/projection proof or by a sound
witness checker over the original bytes. A source hash binds identity only; it
does not show that a syntax tree is the parse of those bytes. Parser/printer
round trips and corpus agreement are empirical evidence. Expand the admitted
productions only as each connected frontend stage needs them, and state every
report's starting boundary.


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
PROGRESS/RECOVERY/ENTRY`. Soundness alone admits a checker that rejects every
program, so also prove `TYPE-COMPLETE/DIAG`: within the specified inference
envelope the algorithmic checker accepts every declaratively well-typed program
and infers the schemes and effects the contract specifies, and every rejected
program reports a diagnostic category and span that the contract fixes and that
name a rule the program violates. Where no principal scheme exists, the
contract records its choice or the limit. The envelope is the specified
language, not unrestricted inference. Making a further diagnostic category
contractual is a language-contract change. TYPE-DIAG concerns rejection reports
only. Keep
invalid programs, contractual limits, features outside a declared domain,
internal faults, and runner timeouts or fuel exhaustion distinct. Any resource/stream fragment is temporary and must
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
A structurally valid, well-typed target that computes a different result must
fail translation validation.

`CERT-FACTS`: the fact validator accepts a candidate frontend's checked facts
and derivations exactly when they form a valid derivation under the declarative
rules for that source and module environment and their observable content
(resolved declarations, nominal and module identity, selected operations,
schemes, effects, guards, and evaluation order) equals what the contract
determines. Alternative derivations with that content are accepted. Within the
declared domain every such derivation is accepted, with a non-vacuity witness. It does not trust the candidate to resolve the
declaration, select the operation, classify every effect, insert every guard,
preserve module identity, keep evaluation order, or implement the source
computation.

`IMPL-REFINE`: every behavior of a concrete machine from a corresponding
initial state decodes to a behavior the semantic machine permits for the same
program and host responses: observation-equal for deterministic cases, within
the permitted-behavior relation otherwise, with errors, host requests,
termination, and divergence preserved. Prove it from local premises: a state
and representation invariant, a decoding relation, initial correspondence,
one-step preservation, step correspondence in which each concrete step matches
zero or more semantic steps and zero-step runs decrease a well-founded measure,
local progress (each invariant state is final, steps, or emits a decoded host
request), and final-result and observation decoding. Derive the required
premises from the semantic features in the admitted domain, not from the
implementation's choices, and never assume conformance as a premise. The raw
indexed machine is the first instance; state its existing step-simulation and
mode-equivalence obligations as instances rather than proving them twice.
Require an inhabited instance and a mutant that drops a premise.

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

### Connected milestone (successor Phase F)

The integrator owns this milestone and composes slices from G1, G1b, G2 and G3
interfaces, G4, G7b, G8, and the fact validator; it opens no new formal domain.
Its path is resolved lexical names, constraints, substitutions and schemes,
elaborated typed core, validated indexed instructions, and observable
execution, for variables, functions and application, immutable and mutable
local bindings, records and projection, Bool and Result control, and the
relevant effect boundaries. Each stage states its exact domain; the domain is
non-empty and contains the milestone's accepted and rejected programs, and
reports give accepted and rejected counts.

The milestone requires the independent contract and its connected theorems;
actual emitted Rust facts and raw IR in the closed exchange documents; the Rust
structural verifier together with the milestone's independent Lean fact and
translation validators, which do not exist yet; precisely labeled execution
evidence; the parser, exporter, compiler, runtime, and host assumptions it rests
on; and a documented way for another implementation to submit candidates to the
same checks. Running foreign raw IR on the Rust executor needs a test-only
decoder into the verified representation. Its programs reject these
corruptions: a wrong binding or nominal identity, a wrong selected operation,
forged effect facts, a missing validation, substituted source or module input,
a well-typed target that computes a different result, a missing or reordered
observation, invalid reuse or lifetime where the slice has them, and an
unsupported or timed-out run presented as agreement.

The milestone is ready for a Goose experiment when this path works through the
current Rust implementation, its assumptions are explicit, and a candidate can
target it. That readiness closes no full-core obligation and certifies no
Goose implementation.

Each capability is end-to-end: formal definition/proof, actual exporter or source route, positive and adversarial native/conformance cases, applicable docs, and incrementally updated coverage. Shared ownership and merge rules follow the `orchestrate` campaign procedure; the table supplies the XSH-specific files, prerequisites, and initial maximum width.

## Acceptance contracts

### Semantics and theorem coverage

The model covers the complete frozen core after parsing, and from source bytes where SRC-ADMIT evidence applies: sugar and binding identity; imports/module generations; core values/types and validated types; constants; callable signatures/defaults/argument plans; effects and lexical negative bounds; dynamic checks and typed stores; all result/control/recovery forms; resources/cleanup/deadlines/signals; producer/stage/worker behavior; raw indexed representation; verification, lowering, and both execution modes. System-report, applets, applications, native library internals, external programs, OS race freedom, Linux/syscalls, LLVM/Rust compiler, and `xshi`'s separate interactive policy are outside the denominator. A core path shared by `xshi` remains in scope.

For execution, define configurations that account for program identity, syntax/current continuation, environment and typed store, calls/blocks/scopes/cleanup, resources, stream and worker state, evaluator cwd/env overlays, deadlines/signals, outcomes, observations, and pending host request. Prove preservation and progress modulo only specified failures, emitted host requests, legitimate suspension, and divergence. A rejected store commits nothing. Host requests are operation-indexed and responses are constrained; distinguish safety claims from liveness claims and state progress premises for liveness.

For lowering, independent source and IR machines relate every permitted target behavior to the source contract. Deterministic fixed scenarios require observation equivalence; nondeterministic schedules use an explicit allowed-schedule/refinement relation. Do not normalize away missing output, reordered specified events, different failures/blame, aliasing, Float distinctions, or interruption checkpoints. Parser projection outside SRC-ADMIT's admitted domains, exporter correctness, Rust execution, and native library behavior remain separately identified empirical or external links.

### Layers and representation

Six layers are separate, and each adjacent pair is joined by a named relation:

1. Original source bytes and module inputs. SRC-ADMIT relates them to the core.
2. The independent parsed core language and its meaning.
3. Checked facts, types, schemes, effects, and elaboration. G4 and CERT-FACTS relate them to the core.
4. The meaning of executable IR, independent of any encoding. G8's lowering obligations and CERT-SOUND relate layer 3 to it, and G7b defines it.
5. The Rust indexed program representation and its exact validation rules: tags, payload widths, one-based IDs and sentinels, ranges, pools, ownership, and provenance stay exact concrete obligations. G7a's structural decoding and G7b's semantic well-formedness map accepted tables to layer 4.
6. An implementation's execution state: frames, slots, stores, and suspended work. IMPL-REFINE relates it to the semantic machine.

Language meaning never depends on layer 5 or 6 details. Layer 5 stays exact; it is the first concrete instance, not a weakened portable form. Another implementation may use packed records, relative references, phase-owned storage, or different frame layouts when it establishes the same relations.

### Replacement implementation contract

This contract assembles existing obligations into what a second frontend or runtime must satisfy. The independent contract is authoritative throughout.

**Frontend.** For a declared, non-empty source or core domain and module environment that contains the milestone programs, a compatible frontend agrees on acceptance and rejection; infers equivalent public types, schemes, and effects; preserves lexical, nominal, module, and binding identities; selects equivalent operations and overloads; and produces elaboration with the same observable content that CERT-FACTS checks, including validations, argument binding, and default evaluation order. Rejections carry the contractual diagnostic category and source span (TYPE-DIAG, and SRC-ADMIT for parse failures); message text and internal numbering are not contractual, and any normalization of diagnostics is specified with its reason. An unsupported feature, crash, timeout, fuel exhaustion, or empty output is never counted as a matching rejection or success.

**Runtime.** A compatible runtime preserves values and numerical distinctions; structured errors, propagation, and blame; evaluation and effect order; cleanup and resource identity; producer demand, suspension, cancellation, and worker behavior; checkpoints and admissible host interactions; and termination and divergence (G1, G5–G8). Deterministic behavior requires the specified observation equivalence; nondeterministic behavior uses the explicit permitted-behavior relation with its liveness premises. Wall-clock timing, native allocation events, and internal instruction counts need not match; language-visible limits and resource behavior must.

**Artifact validation.** A candidate supplies checked facts, derivations, and a target artifact in the closed exchange documents; the independent validator checks them against the source and core contract (CERT-FACTS, CERT-SOUND). Three claims stay separate: the validator's soundness theorem over its admitted domain; a checked certificate for one identified source/target pair; and empirical execution of that artifact by some implementation. Exchange and adapter work stays inside development and test boundaries.

**Runtime refinement.** IMPL-REFINE supplies the template. For compact storage, the semantic features in the admitted domain determine which obligations apply among tags, bounds, offsets, lengths and arithmetic overflow; stable references and alias identity; region ownership, escape, and reclamation; slot reuse and stale handles; closures and environments that outlive calls; frames, continuations, and suspended computations; and observable resource cleanup as distinct from unobservable reclamation. An implementation cannot opt out of an obligation its domain's features require; the contract adds no bijection on internal storage and no identical sharing.

**Implementation boundary.** A theorem about a Lean model of a packed or region-based runtime proves that model. A claim about Goose source, generated C, or native execution needs an identified connection: faithful extraction from a verified model, validation of each generated artifact, or an explicitly trusted compilation or translation path recorded as an external assumption. No such connection exists today.

**Goose considerations, observed 2026-10-28.** Recheck these when an experiment starts; none changes XSH semantics. Goose has no heap, collector, reference counting, or destructors; values live on compiler-managed data stacks or in pools whose `free` releases no memory, so a reused slot can be read as a different value of its type, which is the stale-handle case above. Recursive functions cannot keep growable data across a recursive call. There are no escaping closures or function pointers. Threads are separate programs that exchange only flat values through typed queues. `return E from f` propagates through a hidden discriminant rather than unwinding. Goose compiles a whole program to one C file. `extern fn` admits scalars, flat fixed-size values, references, and slices, rejects callbacks, and forbids C from keeping borrowed arguments; `export fn` functions must be called from the thread that called `goose_init`. Sources: the project [README](https://github.com/aardappel/goose) and [reference](https://github.com/aardappel/goose/blob/master/docs/reference.md). The first experiment should be a frontend that emits facts and raw IR for the Rust structural verifier and the connected milestone's independent validators, and keeps the Rust executor through a test-only decoder; it may reuse the Rust parser if its reports start at that boundary.

### Requirement map

| Requirement | Existing owner | Gap or clarification | Evidence required | Belongs to |
|---|---|---|---|---|
| Source bytes and modules to core syntax | G1b bridge, G8 sugar | Missing: the scope began after parsing | SRC-ADMIT proof or witness checker per admitted production set | Current campaign |
| Frontend acceptance agrees, not only soundness | G4 TYPE-CHECK | Missing completeness within the inference envelope | TYPE-COMPLETE | Current campaign |
| Contractual diagnostics | G4 TYPE-RECOVERY, Architecture invariants | Missing category and span obligation | TYPE-DIAG | Current campaign |
| Candidate facts not trusted | G1b, Phase F | Missing: no validator for another frontend's facts | CERT-FACTS | Current campaign |
| Wrong-but-typed target rejected | G8 CERT-SOUND | Clarified | CERT-SOUND with a wrong-computation mutant | Current campaign |
| Representation-independent semantics | G1, prescribed observation rule | Clarified, including observable identities | Review of G1 state and G7a decoding | Current campaign |
| Runtime refinement template | G8 LOWER-STEP, MODE-EQUIV | Missing generic theorem from local premises | IMPL-REFINE and one concrete instance | Current campaign |
| Host interface contracts | G1 host contracts, G5 resources | Clarified fields; no ABI | Inhabited contracts and EXTERNAL_ASSUMPTION records | Current campaign |
| Evidence subject and identity | Evidence and trust | Clarified; future schema change named, not made | Records that name their subject | Current campaign |
| Connected path and readiness | Connected milestone section, successor Phase F | Clarified as the first milestone, with an owner | Connected theorems, emitted artifacts, corruption rejections | Current campaign |
| Goose frontend facts and IR | none | Needs a candidate | Certificates for its artifacts through CERT-FACTS and CERT-SOUND | Future implementation |
| Goose runtime refinement | none | Needs a candidate and a connection route | IMPL-REFINE instance plus an extraction, validation, or trusted-compilation link | Future implementation |
| Goose host boundary | none | Needs a concrete experiment | A derived ABI meeting the G1 host contracts | Future implementation |

### Adversarial and nonvacuity set

For every core rule require an inhabited positive witness and an adversarial/rejected witness where applicable. Fixed cases cover: signed integer edges and division; UInt store atomicity versus `.require`; bounded ranges, `RelPath` components and NUL; `NonEmpty` filtering and widening mutation; nominal identity and overlapping unions; nested schema failures and no partial publication; shadowing/narrowing; named/default/rest calls; exact/open module contracts and generations; check-before-entry; recursive/unknown/captured/negative/latent effects; result data versus propagation; defer capture, later-failure `errdefer`, cleanup ordering and primary/secondary failures; resource escape/explicit release/aliases/foreign IDs; deadlines, signal hooks and uninitialized `main`; producer creation, aliasing, delegation, over-pull and close order; worker bounds/order/failure/drain; malformed tags/IDs/ranges/pools/provenance/guards; wrong lowering order, lost cleanup/blame, in-place/frame boundaries; and exact wire-byte, unknown schema, duplicate-ID, timeout/out-of-fuel categories.

Retain deliberate cross-feature products for (1) validated value × typed callback × imported module × top-level binding; (2) resource in error cause × nonlocal exit × `try`/`retry` × cleanup; (3) shared producer × negative effect region × early close × defer; (4) UInt/dynamic parameter × failed store × `within` × `errdefer`; (5) bounded parallel imported callback × independent failures × cancellation; and (6) explicit resource `with` × escaped alias × ordinary outer ownership. Name the shared invariant composing each product.

The mutation gate kills at least these 16 classes: missing typed-store guard; missing effect edge; unknown treated as empty; dropped `io` implication; `try` clears host effects; lost latent producer provenance; swapped effectful argument order; refinement retained after filtering; wrong nominal/import provenance; dropped/reversed cleanup; data `Err` treated as propagation; serial source over-pulled; wrong parallel failure selected; out-of-range IR accepted; dynamic guard omitted; observation normalization hides a missing event. The replacement contract adds nine classes, for at least 25 in total: a forged binding or nominal identity, a wrong selected operation, a forged effect fact, a well-typed target that computes a different result, an unsupported or timed-out run reported as agreement, a parse accepted for different bytes, a checker that rejects every program, a fact validator that trusts the candidate, and a refinement proof with a dropped premise. Record detecting gate and witness for each. Add a mutant when an actual regression exposes a new class. For compilation, effects, and cleanup, include a deliberately wrong lowerer, missing-edge solver, and bad release ordering respectively. Host contracts must have a concrete inhabited mock and admissible reachable responses.

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

No proof target for system-report, core applets, user applications, UI, or `xshi` policy; no universal Rust/LLVM/OS/syscall/filesystem-race/external-command/native-library proof; no runtime sandbox from `without`; no production Lean runtime or evaluator; no speculative AST, JIT, async runtime, public compiler API, task service, parallel coordination daemon, compatibility shim, loose serialization fallback, corpus-only replacement for a theorem, or performance optimization before parity. No Goose port, Goose installation, proof of the Goose compiler, C compiler, LLVM, linker, or CPU, generic backend or plugin framework, public `libxsh` ABI, permanent production exporter, new product artifact format, or language change made for a temporary Goose limitation. Validating every production compilation is a separate deployment decision outside this plan.

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
| Float equality implementation and witnesses | language runtime comparison uses `src/runtime/eval.rs::lowered_values_semantically_equal`; `src/runtime/value.rs::FloatValue::eq` keeps raw-bit representation equality for storage and codecs. Constant-folded `B::Eq/B::Ne` use `src/sema/constants.rs::constant_values_equal`; the fuzz oracle uses `crates/xsh-fuzz/src/eval.rs::values_equal`; Lean comparator laws are in `formal/Xsh/Values/FloatEquality.lean`; runtime and constant-folded regressions are in `tests/xsh/float-equality.xsh` (`test_float_runtime_equality_uses_ieee_values`, `test_float_constant_equality_uses_ieee_values`). Run these against the post-consolidation source and record finite evidence; they do not establish Rust conformance by themselves. |
| Existing behavior/conformance cases | `tests/xsh/{effects,private-proc-effects,resource-ownership,error-resource-ownership,with-resource,stream-producers,checker-streams,lowering-coverage,signal-hooks}.xsh`; `tests/grammar.rs`; verifier corruption tests in `src/runtime/eval/indexed/full.rs`; independent soundness oracle and harness under `crates/xsh-fuzz/`. |

The existing runtime applies verified indexed execution, uses heap-backed frames and a verifier-bounded in-place path (currently height 128), and retains program ownership for suspended producers. Resource/effect/lowering facts are behavior-bearing; prove their preservation rather than expanding hot rows by default. Re-resolve callable provenance and temporary-writer verifier checks at campaign start.

### Log

- 2026-10-29: G1b added the first vertical stage. `formal_bridge/arithmetic.rs`
  exports and decodes a closed raw-IR scope for two integer functions, runs
  them through the verified Rust path, and rejects wire, program and
  observation corruptions. `Xsh.Conformance.Transport.Json` and
  `Xsh.Conformance.Vertical.{Artifact,Meaning,Certificate}` decode the
  exchanged text, give the IR rows a base-machine meaning, and certify the
  artifact (`base_vertical_artifact_certified`) with rejected corruptions. The
  three byte-level theorems use `decide +kernel`: plain `decide` exceeds the
  recursion limit on a 2.3 KB document. The Lean decoder reads ASCII code
  points only and rejects other text. BASE-VERTICAL stays planned; see the
  open items.
- 2026-10-29: G1 proved base theorems for BASE-ENV
  (`Xsh.Proofs.Base.Env.base_env`), BASE-NUM (`Xsh.Proofs.Base.Num.base_num`),
  BASE-STEP (`Xsh.Proofs.Base.Step.base_step`), BASE-PROGRESS
  (`Xsh.Proofs.Base.Progress.base_progress`) and BASE-RUNNER
  (`Xsh.Proofs.Base.Runner.base_runner`). Each has an inhabited witness and
  rejected mutants in its module. The integrator recorded them as fragment
  roots, not closed obligations, because the base machine is not the frozen
  core and the existing base-machine roots already disclaim whole-language
  progress. The inventory schema gained a closed row
  status naming a root theorem; the axiom audit rejects a closed root that is
  absent, not a theorem, or outside the project modules.
- 2026-10-29: The owner resumed Lean. G0b froze the source at `65fcb0f42`
  (tree `6947bd3de`). The four `tests/xsh/float-equality.xsh` tests passed
  through `xsht test --exact` with verification `xsh` and `xsht` built from that
  commit in the pinned container, and are the first `RUST_CONFORMANCE` cases.
  At that commit the inventory check, its 51 tests, the axiom audit over 12,948
  declarations and its self-test, and the paranoid kernel recheck passed. The
  host was loaded (around 20 on 32 cores), so the loop costs are diagnostic:
  an incremental `lake build` took 19 s wall and the fresh recheck 478 s.
- 2026-10-28: The owner revised the scope so the independent contract supports a second implementation, with Goose as the first intended experiment. SRC-ADMIT, TYPE-COMPLETE, TYPE-DIAG, CERT-FACTS, and IMPL-REFINE joined the plan as planned obligations, raising the count from 72 to 77. The pause and the source-freeze gate are unchanged.

## For the owner afterwards

Review theorem statements and `formal/coverage.json`, especially each `EXTERNAL_ASSUMPTION`, actual Rust and artifact identity, and any parked mandatory item. The campaign is complete only with G10 on a fresh identified checkout. Keep each full theorem obligation planned until its complete proof and required evidence pass.
