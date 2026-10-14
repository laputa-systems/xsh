# XSH semantic soundness campaign

**Status:** Active staggered foundations, authorized 2026-10-13 after preflight.
The pinned toolchain, finite private bridge fixtures, provisional inventory,
scalar, base-machine, and observed-machine fragments, validation, nominal
identity/provenance,
union-membership, byte-lexical RelPath, and recursive `.require` fragments,
bounded refinements, non-empty filtering, environment/store, effects including
bounded failure witnesses, edge-order laws, and five bounded primitive
classifications, argv expansion, finite ordered-set laws, List access bounds
and bounded List-comprehension execution, finite loop-control traces,
collect-block publication, ordered match-arm selection, and duration
operations, native path components and lexical path normalization, Str-backed
enum wire conversion, JSON compatibility projection,
raw-IR, and transport codec/law fragments are
integrated, along with finite map-presence laws over ordinal keys, checked call
argument ordering, Unicode scalar text slicing, primitive collection-key
ordering, finite Result fallback traces, JSON-tree paths, checked record
updates, finite Map literal evaluation, UTF-8 byte string search and byte
access, lazy optional access, Result `?`
propagation boundaries, and finite `.require(Set[T])` validation and array
conversion, finite Boolean short-circuit evaluation, raw `Bytes` access,
effectful List splices, ordered Set algebra, bounded List pattern matching,
and bounded integer byte codecs.
The XSH language contract defines Float `==` and `!=` by IEEE value equality;
runtime, constant-folding, and fuzz-oracle comparisons implement that rule,
and `Xsh.Values.FloatEquality` defines an executable comparator over raw
encodings with Lean laws for IEEE value equality. A source regression covers
runtime and constant-folded signed-zero, NaN, and nested comparisons. Consolidation
acceptance and the post-consolidation G0b source freeze remain pending gates.
The owner requested wider staggered lanes; continue on disjoint domains with
the coordinated build and memory limits. These independent fragments make no
source or Rust conformance claim and close no full G1, G2, or G3 obligation; all
72 inventoried universal theorem obligations remain planned. The Raw decoder
modules include a finite four-tag schema model with successful-row conditions,
bounded payload ranges, malformed-row rejection witnesses, and a guard
mutation witness. This does not close G7a or establish Rust decoder/verifier
correspondence. The effects checker bridge consumes already-classified
`FactSet` summaries and checks their body-site obligations through the finite
lexical model; it does not establish Rust classification/runtime correspondence
or close `EFF-LATENT`/`EFF-SEMANTIC`. The inventory now names 631 fragment
roots across 89 owners; its 51 Python tests check inventory consistency,
mutation controls, and finite-case evidence boundaries. The observed-machine
fragment wraps the accepted scalar/integer BaseMachine in an ordered
observation trace; progress preserves the link between a pending typed wait
and its emitted request. It adds no source, raw-IR/lowering, Rust, or
concrete-clock correspondence and closes no full BASE-* obligation or G1. The
new RelPath roots cover the independent byte-list model only;
they establish no Rust checker/runtime correspondence, symlink confinement,
or full G2 validation closure. Five observation
fragments pair caller-supplied emissions with classified obligations, require
explicit alignment for permission and pull-sequence correspondence, and use
structural `Expr` site labels that cannot distinguish identical occurrences.
They do not execute producers or establish Rust conformance or full
`EFF-LATENT`/`EFF-SEMANTIC` semantics. The record-update fragment models the
current checker's path-prefix overlap rule; `docs/SPEC.md` currently omits this
rule. No `BASE-*` obligation is complete.

`Xsh.Control.OptionalAccess` models one lazy access hop over an already
evaluated nullable receiver, callback order, and Result propagation before
method arguments; it excludes effects, cleanup, and runtime parity.
`Xsh.Strings.ByteAccess` models `Str` byte lookup and UTF-8-boundary-checked
slices with an explicit length only. It excludes omitted-length slices,
runtime errors, and `UInt8` conversion. Neither fragment establishes Rust
conformance or closes a full G1/G2/G3 obligation.

`Xsh.Iteration.Snapshot` models finite eager values captured for a loop over a
List, finite Map, Str, Bytes, or Result. Its body frame keeps the current item
immutable across source rebinding. It excludes streams and pipelines, loop
control, typechecking, and runtime or Rust correspondence. `Xsh.Loops.ControlFlow`
has twelve roots for finite loop-local outcomes and traces over a preselected
finite item sequence. It covers current-loop continue and break, outer-target
propagation through nested loops, return and failure propagation, repeat-count
evaluation, finite ranges, and exact traces. It excludes checker target
validity, stage callbacks, divergence, effects, cleanup, streams, and runtime
or Rust correspondence. `Xsh.Paths.Fields`
models POSIX lexical name, basename, dirname, parent, and extension queries on
native bytes; it does not match every Rust-backed XSH `Path.name`, `Path.ext`,
or `Path.parent` edge case, including `foo/.`, `foo/..`, and component
normalization, so it establishes no XSH API/Rust correspondence. `Xsh.Paths.Normalize`
has twelve roots for a native-byte lexical stack model. It removes empty and dot
components, cancels ordinary components before `..`, preserves leading relative
parents, clamps parents at an absolute root, and retains component bytes. Its
concrete repeated-normalization example is not a general idempotence law. Runtime
NUL admission, cwd and filesystem resolution, symlinks and confinement, path
fields, construction and interpolation, and runtime or Rust correspondence are
excluded.

`Xsh.Collect.Block` has twelve roots for finite collect events and publication.
Only an already classified escaping transfer is represented; it records its
kind without a target or return/error payload. Loop-local control classification,
effects, cleanup, streams, retry, checker/type inference, AST evaluation, and
runtime/Rust correspondence are excluded. `Xsh.Matches.Selection` has twelve
roots for ordered arm decisions after a single subject evaluation, with
abstract pattern outcomes and a supplied Boolean guard result. It excludes
actual pattern matching, guard expression evaluation, type diagnostics,
exhaustiveness, parsing, and runtime/Rust correspondence.

`Xsh.Json.Encoding`
models bounded compact encoding of a finite JSON tree; it excludes Float
formatting, pretty output, parsing, and I/O, and establishes no runtime or Rust
correspondence.
The earlier 29 roots cover argv word/splice expansion, five named primitive
effect classifications under the represented lexical rules, and ordered-set laws
over finite ordinal keys. They establish no parser, Rust implementation, or full
G1/G3 correspondence. The List-access and duration batch added 29 roots for
List index and slice rules plus checked millisecond-duration operations. The
List fragment excludes parsing,
diagnostic phases, store updates, String/Bytes slicing, and Rust correspondence.
The Duration fragment excludes parsing, evaluation, source spans, compound
assignment, and Rust correspondence; its generalized mathematical-Int
`time.millis` saturation case is unreachable from signed-64-bit XSH `IntValue`
inputs. The inventory remains provisional, with null source identity, no finite
conformance cases, and all 72 universal obligations planned.
**Prepared:** 2026-10-13. **Checkout:** `/home/josh/d/laputa-systems/xsh`.
**Scope:** Formalize the whole frozen XSH core in Lean, prove its semantic and
lowering obligations, and connect those independent statements to the Rust
implementation with conformance and certified artifacts. The current scalar,
syntax, environment/store, validation, effects, argv, ordered-set, base-machine,
raw-IR, transport, UTF-8 byte search, Result propagation, Set validation, JSON
compatibility projection, bounded List-comprehension execution, finite loop
control, and native-byte path normalization modules are partial foundations
only. The search model uses Lean's encoded byte list;
Result propagation assumes an explicit compatible-error embedding; Set
validation assumes a finite ordered key table and target predicate. JSON
compatibility assumes unique record and Map keys, models Set serialization only
for ordered ordinal keys emitted as ordinals, and requires explicit path
conversion. List execution uses abstract finite expressions. None of these
fragments establishes Rust correspondence.
`formal/Xsh.lean` imports every current module under `formal/Xsh/`, but root reachability does not
establish adequacy or close any full-language obligation. These fragments do
not establish Rust conformance.

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

1. **Float equality:** The XSH language semantics for Float `==` and `!=` use IEEE value equality: `+0 == -0`, and every NaN comparison is unequal, including identical NaN bit patterns. Scalar, structural, constant-folded, and fuzzer-semantic comparisons must agree. Preserve raw Float bits in serialization and preserve IEEE total-sort order. This decision authorized the minimal runtime/equality repair and native and formal regressions; the runtime, constant-folding, and fuzz-oracle repair is now integrated. The independent Lean scalar model does not establish Rust conformance.
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

G0a established the pinned Lean/Lake/checker setup, private test-only bridge, provisional source inventory, and initial scalar foundation. Continue reconciling the imported theorem modules, inventory mutation controls, and audit results as foundations grow. G0b starts after consolidation closes: use the actual post-close source tree, core behavior, and measured loop costs as the baseline. Capture HEAD, clean/dirty state, current binaries and source identity, Lean/Lake/checker versions, Rust target/profile/compiler, and the pinned container. Build a fresh matching `xsh`/`xsht` pair before any process/native observation. Do not let an old verification binary certify changed source.

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

The initial Float equality disagreement has been repaired: the XSH language contract defines Float equality by IEEE value equality, and runtime, constant-folded, and fuzz-oracle comparisons implement that rule while raw Float representation and total-sort order stay intact. `formal/Xsh/Values/FloatEquality.lean` defines a Boolean comparator over raw encodings and proves its agreement with IEEE value equality, symmetry, transitivity, and reflexivity exactly for non-NaNs. It also proves that signed-zero equality preserves distinct payloads, wire encodings, and total-sort order. `tests/xsh/float-equality.xsh` now contains the runtime and constant-folding witnesses, including signed zero, same-bit NaN, and nested values. After consolidation closes, run these source witnesses against the frozen source and record the results as finite evidence. The Lean model and laws do not establish Rust conformance.

Record measured startup, exact-name native gate, fresh Rust build, Lean changed-module check, and clean Lean build as each becomes available. Warm changed Lean-module check target: at most 60 seconds. Exact-name Rust/native check target: at most 30 seconds unless an external/process boundary requires more. If a repeated core loop exceeds 120 seconds or twice its measured baseline, repair the loop before broad lane work. Do not report a benchmark claim from one run. Performance is measured and logged during parity work; optimization gates begin only after feature and test parity. Final performance gates run only when `perf-gate-ready` passes immediately before and `perf-gate-ready -1` passes immediately after; if the host remains busy, report `perf gate deferred: host busy`. Memory and binary-size gates always run.

G0 writes the only machine-readable evidence inventory, `formal/coverage.json`, consumed by the gates. It records actual commands/results, versions, fixed-case counts by outcome, theorem roots and axiom audits, statement-drift review, artifact identities only where required, and remaining assumptions/baselines. Do not add `STATUS.md`, duplicate receipts, hashes of unrelated state, or a second register. Keep short append-only decision and phase logs in this document.

## Sequence and ownership

The integrator selects slices using the repository's campaign procedure. Each gate
owns the complete capability, its tests and docs, and an exclusive formal domain.
The integrator owns shared interfaces, the root import, coverage inventory, gate
routing, and cross-workstream Rust paths.

### G0 — Ground (integrator only)

G0a has started after this preflight. The pinned toolchain, private typed test
bridge, and provisional inventory are in place; continue validating their
controls and keep every formal module reachable from the root. The bridge maps
arena names to text and Lean binders deterministically and tests parser/printer
projection without exporting private runtime structures through a product API.
FullStore, FullProgram, and FullVerifier are private and have no portable
encoder. Prefer a `#[cfg(test)]` exporter/decoder beside verifier tests; do not
use the default-enabled `native-tests` feature. G0b starts after consolidation
closes: rebuild the post-close source; inventory every core syntax/type/call/
control/stage/IR variant and the exclusion boundary; retain the signed-zero,
NaN, and structural Float equality regressions in
`tests/xsh/float-equality.xsh` and record their post-repair results; validate
closed wire/scenario forms, proof root, audit controls, gate routing, and loop
costs. Rust's typed `FullTag` cannot carry arbitrary unknown
raw tag bytes: reject those at the Lean transport boundary and cover
typed-representable corruptions in Rust. No unresolved core item may disappear
from `formal/coverage.json`.

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
| Float equality implementation and witnesses | language runtime comparison uses `src/runtime/eval.rs::lowered_values_semantically_equal`; `src/runtime/value.rs::FloatValue::eq` keeps raw-bit representation equality for storage and codecs. Constant-folded `B::Eq/B::Ne` use `src/sema/constants.rs::constant_values_equal`; the fuzz oracle uses `crates/xsh-fuzz/src/eval.rs::values_equal`; Lean comparator laws are in `formal/Xsh/Values/FloatEquality.lean`; runtime and constant-folded regressions are in `tests/xsh/float-equality.xsh` (`test_float_runtime_equality_uses_ieee_values`, `test_float_constant_equality_uses_ieee_values`). Run these against the post-consolidation source and record finite evidence; they do not establish Rust conformance by themselves. |
| Existing behavior/conformance cases | `tests/xsh/{effects,private-proc-effects,resource-ownership,error-resource-ownership,with-resource,stream-producers,checker-streams,lowering-coverage,signal-hooks}.xsh`; `tests/grammar.rs`; verifier corruption tests in `src/runtime/eval/indexed/full.rs`; independent soundness oracle and harness under `crates/xsh-fuzz/`. |

The existing runtime applies verified indexed execution, uses heap-backed frames and a verifier-bounded in-place path (currently height 128), and retains program ownership for suspended producers. Resource/effect/lowering facts are behavior-bearing; prove their preservation rather than expanding hot rows by default. Re-resolve callable provenance and temporary-writer verifier checks at campaign start.

### Proof and evidence references

Lean's bundled fresh checker is `leanchecker --fresh`; it was introduced in Lean 4.28 and was previously named `lean4checker`. The local preflight environment reports Lean 4.34.1 release commit `5045d0056413266e57c625dcd7c365b10e377c52` and Lake `5.0.0-src`. A smoke invocation on `Init.Prelude` exited 0; this establishes checker availability only, not a project build, theorem, or proof. Run the full `Xsh` root after the formal project exists.

### Append-only decision log

- 2026-10-13 — Owner chose IEEE Float equality across all semantic paths; authorize only the minimal repair and regressions during the future campaign.
- 2026-10-13 — The authorized runtime, constant-folding, and fuzz-oracle Float equality repair is integrated. This implementation status does not claim Rust conformance.
- 2026-10-13 — Owner confirmed lexical `without` bounds and producer-origin effect charging; retain causal latent-effect proof, reject runtime-sandbox redesign.
- 2026-10-13 — Pin Lean 4.34.1/Lake 5.0.0-src; bundle the checker and audit axioms separately.
- 2026-10-13 — No Mathlib by default; use only after a named missing theorem and a bounded maintenance/trust/build-cost case.

### Append-only phase log

| Phase | Commit/revision and exact evidence | Open items / next action |
|---|---|---|
| Preflight | Lean 4.34.1 fresh `Init.Prelude` recheck; pinned release hello execution/JSONL trace; private typed-callable verifier regression passed. `mdtool check --untracked xsh-lean.md`: zero errors; planned-path/symbol warnings and known dangling-fixture notes. Independent review found no P1/P2 blocker. | Exporter and project do not yet exist; G0a proves those foundations before dependent work. |
| G0a | Pinned project, module-wide axiom audit, guarded fresh-check runner, and exact scalar foundation merged. Private Rust bridge and provisional inventory have focused checks and independent review; integration fixes remain. | Include every theorem module in the fresh-check root. Finish reference and inventory mutation controls; G0b waits for consolidation close. |
| Staggered foundations | Scalar bit/key round trips, lexical lookup and scope restoration, cell-reference captures, membership-guarded store laws, `NonEmpty` and bounded-integer validation boundaries, finite effect-lattice and recursive-summary fragments, and lexical bounds with delayed-work origins are integrated. The provisional inventory contains 89 owner rows and leaves all 72 complete theorem obligations planned; its 13 mutation tests pass. The private bridge has strict JSON limits, table bounds, and ghost-reference rejection tests. | These fragments do not close G2 or G3, establish Rust conformance, or supply the shared whole-language invariant. Whole syntax, typing, operational semantics, and artifact certification remain open. |
| Rooted machine, raw IR, and transport foundations | Commits `9d475279b`, `7de4804c4`, `7e2e91947`, `1e434d815`, `d8b24f16e`, `9bc73a8a2`, and `0107c8158` add the validated store, integer base machine, raw IR word/table/range/row fragments, and strict transport codecs with accepted-value re-encoding laws; each module is imported by `formal/Xsh.lean`. Follow-ups `8a6b70d42` and `2aa2069c8` preserve exact typed scalar values in the base-machine fragment and constrain controls under pending integer frames, with witnesses for rejected non-integer frame mutations. Arithmetic in that machine remains integer-only. No `BASE-*` obligation is complete; all 72 full theorem obligations remain planned. | Root reachability is recorded, but these fragments do not establish parser/exporter or Rust conformance, whole-language adequacy, artifact certification, G0b source freeze, or full G1 closure. Begin G0b only after consolidation acceptance and freeze the post-consolidation source. |
| Float laws, constant-folding regression, and typed clock wait | Commits `32f52b839`, `c1f50d041`, `7fe558c5e`, `0ab68c5d0`, `ccc19d054`, and `711031963` add 47 inventoried fragment theorem roots, executable Boolean IEEE Float comparison laws, the typed signed epoch-millisecond `time.now()` request/wait/resume/order/mismatch fragment, and a source regression for runtime and constant-folded Float comparisons. `python3 formal/scripts/check-inventory.py` reports 89 owner sets, 47 Lean fragment roots, and 72 planned obligations; `python3 formal/tests/inventory/test_check_inventory.py` passes all 29 tests. These results validate the inventory checker and its cases, not Rust or clock behavior. | Clock accuracy and correspondence with real time are external assumptions; no Rust or OS clock conformance is claimed. Run the Float source regressions against the frozen post-consolidation source. All 72 full obligations remain planned, and no `BASE-*` obligation is complete. |
| Generic finite runner trace validity and response laws | Commits `cf167d187` and `125a9394` prove that `runBaseHostFuel` produces a valid prefix trace and that valid traces preserve base-machine well-formedness. The new runner fragment proves that terminal runs ignore added fuel and response suffixes, waiting runs remain unchanged without a response, a supplied response resumes a wait, and response order determines the returned value. An omitted-response mutation witness shows that a consumed mismatched response cannot be removed from the trace while retaining the waiting-to-failure endpoints. The earlier two-request script still yields responses 2 then 3 and completes with 5; a Boolean response is labeled and rejected with a type-mismatch fault. The inventory names 57 fragment roots; its 29 Python tests remain empirical inventory-checker tests, and all 72 full obligations remain planned. | These proofs cover only the formal finite scripted runner. They do not prove source/whole-language soundness, Rust or OS conformance, concrete host behavior, or clock accuracy; clock correspondence remains external. No `BASE-*` obligation is complete. |
| Raw decoder bounds | The proof source at `2aceba100` establishes successful `readWords?` length, successful payload length and extra-table bounds, and rejection of one concrete out-of-bounds payload range. The inventory records these as three `SPEC_PROVED` fragments, bringing the root count to 60; the checker still reports 89 owner sets and 72 planned obligations, with all 29 inventory tests retained. | These are independent Raw decoder fragments. G7a, complete decoder soundness, and Rust verifier conformance remain open. |
| Finite delayed-effect pull sequences | Three `SPEC_PROVED` roots record that an abstract `DelayedWork` pull yields the ordered finite prefix and corresponding remainder, preserves each charge's creation and body sites, and preserves permission when the creation-context permission premise holds. The inventory names 63 roots; its 29 tests, 89 owner sets, and 72 planned obligations remain unchanged. | Only the permission result assumes the creation-context permission premise. These results establish no Rust checker or stream alias, delegation, or close conformance, and close neither EFF-LATENT nor EFF-SEMANTIC. |
| Closed producer expression bridge | From base `9b200e3dd`, `ExpressionBridge` checks finite classified producer bodies at their own lexical call contexts, preserves checked handles through aliases, records delegated call/body paths, and retains tagged conditional alternatives. Concrete witnesses accept an outside-created producer, reject creation under `without net`, reject a missing alias, an unknown source form, and a restricted delegated/conditional arm, and show that a restricted consumer keeps the producer creation charge. `lake build Xsh.Effects.ExpressionBridge` and `lake build Xsh` pass; the inventory checker reports 89 owner sets, 77 fragment roots, and 72 planned obligations; all 29 inventory tests pass; the axiom allowlist passes for 5,329 project declarations, its negative controls pass, and `leanchecker --fresh Xsh` passes. | This is a closed checker/source fragment only. It does not prove Rust source classification, full stream execution, alias cursor runtime behavior, close/resource semantics, or conformance, and closes neither EFF-LATENT nor EFF-SEMANTIC. |
| Finite delayed-effect observations | After bridge baseline `6d77ab06`, five `SPEC_PROVED` roots in `Xsh.Effects.Lexical` show that positional observation pairs preserve supplied emission order and retain creation site, body site, and required effect; creation permission and equality with a finite pull sequence require explicit obligation alignment; a listed checked-consumer obligation is permitted at its own context. The inventory names 82 fragment roots, 89 owner sets, and 72 planned obligations; all 29 inventory tests remain. | Emissions and classified obligations are caller-supplied. Structural `Expr` site labels cannot distinguish identical occurrences. These results do not execute producers, establish Rust conformance, or close full EFF-LATENT or EFF-SEMANTIC. |
| Finite error capture | `Xsh.Effects.Capture` proves error removal, preservation of other effects, idempotence, and commutation with represented effect closure; clause interpretation is permutation- and duplicate-invariant. Empty-bound witnesses expose mutations that drop named effects or unresolved work, and a captured missing-contract graph retains its unresolved fact. The inventory checker reports 89 owner sets, 95 fragment roots, and 72 planned obligations; all 50 inventory tests and `lake build Xsh.Effects.Capture` pass. | These theorems concern represented finite summaries, closure, and graph edges only. They do not establish Rust checker classification or propagation, complete solver behavior, full EFF-CAPTURE, EFF-LATENT, or EFF-SEMANTIC semantics, or conformance. |
| Checked integer outcome laws | Commit `23d677cd2` adds checked conversion exactness, total signed-range outcome laws for addition, subtraction, multiplication, division, and remainder, plus concrete mutation witnesses for wrapping addition and Euclidean remainder. The inventory records all 8 added public theorem roots, bringing it to 103 fragment roots while retaining 89 owner sets and 72 planned obligations. The inventory checker and all 50 Python inventory tests pass. | These are fragments over the independent Lean integer model only; they establish no Rust arithmetic conformance or checker integration. The inventory remains provisional with null source identity and no finite conformance cases. |
| Nominal identity, provenance, and union membership | Commit `b3e08a564` adds `Xsh.Validation.Nominal`; the root now imports it and the inventory lists all 10 public theorem declarations, including the ID-collapse and provenance-erasure mutation witnesses. The inventory checker reports 89 owner sets, 113 fragment roots, and 72 planned obligations; all 50 Python inventory tests pass. Schema 2 remains provisional with null source identity and no finite conformance cases. | This finite model does not validate record schemas or `.require` inputs, prove union exhaustiveness, or establish source/Rust conformance or full G2. The single `lake build Xsh` invocation stopped before compilation because its relative config path was resolved beneath `.work/`; no axiom audit or fresh kernel check was run. |
| Lexical relative-path boundary | Commits `74ff794f6` and `d5ea8c97d` add and refine `Xsh.Validation.RelPath`, and `Xsh.lean` imports it. The coverage inventory lists all 21 public theorem roots; its checker reports 89 owner sets, 134 fragment roots, and 72 planned obligations, and all 50 Python tests pass. `lake build Xsh` passes; the transitive axiom allowlist passes for 5,668 declarations; the prohibited-term scan and guarded fresh kernel check pass. Schema 2 remains provisional with null source identity and no finite conformance cases. | This is an independent byte-list model only. It does not establish Rust checker/runtime correspondence, symlink-aware filesystem confinement, or full G2/VAL closure. |
| Recursive `.require` validation fragment | Commits `047bf1e58` and `4b0e8748a` add `Xsh.Validation.RecursiveRequire`, and `Xsh.lean` imports it. The finite target grammar uses explicit predecessor fuel, validates required nested fields and list elements, accepts `null` for optional values, preserves extra uniquely named fields, and rejects duplicate record keys. Its 11 `SPEC_PROVED` roots cover nested success and rejection, exact value erasure and provenance, recoverable nonmutation, and mutations that skip nested checks or duplicate-key rejection. The inventory checker reports 89 owner sets, 145 fragment roots, and 72 planned obligations; all 50 Python inventory tests pass. `lake build Xsh` succeeds and the transitive axiom allowlist passes for 5,894 declarations. Schema 2 remains provisional, with null source identity and no finite conformance cases. | This is an independent finite model only. It establishes no Rust correspondence or full G2/VAL proof. |
| List access and checked duration fragments | Commits `819512a71` and `cc08766a3` add `Xsh.Semantics.ListAccess` and `Xsh.Values.DurationOps`; the root imports both. The inventory records exactly 11 and 18 `SPEC_PROVED` roots respectively, bringing it to 232 roots across 89 owners. The inventory checker and all 50 Python tests pass; `lake build Xsh`, the transitive axiom allowlist for 6,434 declarations and its negative controls, the prohibited-term scan, and guarded fresh kernel check pass. Schema 2 remains provisional with null source identity, no finite conformance cases, and all 72 obligations planned. | List access does not model parser or diagnostic phases, store updates, String/Bytes slicing, or Rust behavior. Duration operations do not model parsing, evaluation, spans, compound assignment, or Rust behavior; the generalized mathematical-Int `time.millis` saturation case is unreachable from signed-64-bit `IntValue` inputs. These fragments close no full G1/G2/G3 obligation. |
| Finite map-presence laws | Commit `8485996db` adds `Xsh.Values.MapPresence` with 12 `SPEC_PROVED` roots, and the root imports it. The finite ordinal-key model distinguishes a present `null` from an absent key and covers lookup, `.get`, presence, set, remove, and a mutation witness. The inventory now names 244 roots across 89 owners; all 50 Python inventory tests pass. Schema 2 remains provisional, `source_identity` is null, finite conformance cases are empty, and all 72 obligations remain planned. | The model does not cover key validation, comparators, ordering, iteration, JSON, diagnostic tags, runtime or Rust behavior, or full G1/G2/G3 closure. |
| Call ordering, Unicode text slices, and primitive key order | Commits `84d36234f`, `86e49ff2d`, and `339bc0e0d` add 14, 11, and 11 `SPEC_PROVED` roots in `Xsh.Calls.ArgumentOrder`, `Xsh.Strings.Slice`, and `Xsh.Collections.KeyOrder`; the root imports each once. The call-layout model includes a parameter-sorting mutation that changes source events from `[21, 20]` to `[20, 21]`; the Unicode scalar-list model includes a byte-size-normalization mutation that changes its slice; and the key-order model includes a reversed-Boolean mutation witness. The inventory now names 280 roots across 89 owners; its checker and all 50 Python inventory tests pass. Schema 2 remains provisional with null `source_identity`, no finite conformance cases, and all 72 universal obligations planned. | Call ordering assumes resolved parameters, pre-evaluated event IDs, and checked spread fields. Unicode slicing models Lean `Char` sequences, not XSH UTF-8 storage. Key ordering proves reflexivity, antisymmetry, and transitivity but no totality theorem. These fragments do not establish runtime or Rust correspondence, or close any full G1/G2/G3 obligation. |
| Result fallback and JSON path fragments | Commits `2fd9d978b` and `f01938e39` add 10 and 18 `SPEC_PROVED` roots in `Xsh.Results.Fallback` and `Xsh.Json.Path`; `formal/Xsh.lean` imports each module once. The fallback model covers lazy `??` selection for Result and optional inputs, original-error handler context, fallback-only call traces, nested chains, and an eager-call mutation witness. The JSON model covers finite Float leaves, string-key and nonnegative-index paths, nested read/update/removal, and an append-at-length mutation witness. The inventory checker reports 89 owner sets, 308 roots, and 72 planned obligations; all 50 Python inventory tests pass. `lake build Xsh` succeeds, the axiom allowlist passes for 7,251 project declarations, all three negative controls are rejected, the prohibited-term scan is clean, and `leanchecker --fresh Xsh` passes. Schema 2 remains provisional with null `source_identity` and no finite conformance cases. | Fallback traces record only selected tail calls and do not model general expression or operator evaluation, parsing/checking, cleanup, effects, or concrete runtime errors. JSON represents only String keys and excludes non-Str Map-key validation, key uniqueness policy, source integer-width bounds, parsing, encoding, diagnostics, and runtime behavior. Neither fragment establishes Rust correspondence or full G1/G2/G3 closure. |
| Native path components and enum wire conversion | The reviewed `05b9b45b` and `89c6ce365` proof commits add 14 roots in `Xsh.Paths.Components` and 17 in `Xsh.Enums.WireConversion`; `formal/Xsh.lean` imports each once. The path model reuses `RelPath.PathBytes` and its byte helpers but defines its own `splitOn`/`collectNames` scan, with roots for component identity, boundaries, preserved bytes, prefix/suffix queries, and a raw-byte prefix mutation. The enum model assumes unique, nonempty prepared mappings; uses `NominalId`; layers conversion over shared recursive `.require`; and proves that every map value is visited before publishing a converted value, with a skipped-map-validation mutation witness. The inventory reports 339 roots across 89 owners and 72 planned obligations; all 50 Python tests pass. `lake build Xsh` passes; the axiom allowlist accepts 7,761 declarations, all three negative controls are rejected, the prohibited-term scan is clean, and the guarded fresh kernel check passes. Schema 2 remains provisional, `source_identity` is null, and finite conformance cases remain empty. | The path fragment does not establish Rust scanner or filesystem correspondence. Structural `.require` failures remain pathless; only the additional conversion pass carries `BlamePath`. Enum source declarations are assumed prepared and are not checked. Neither module establishes Rust conformance or full G1/G2/G3 closure. |
| Checked record updates and finite Map literal evaluation | The reviewed `4e049dc2f` and `32328a1f7` proof commits add 12 roots each in `Xsh.Records.Update` and `Xsh.Maps.Literal`; `formal/Xsh.lean` imports each once. The update model covers single base evaluation, source-ordered replacement callbacks, failure short-circuiting without partial publication, snapshot-rooted nested updates, path overlap, and a shallow-update mutation. The Map model covers key-before-value evaluation, written entry/spread order, overlay replacement, short-circuiting, ordered present-key iteration, and a skipped-entry mutation. The inventory reports 363 roots across 89 owners and 72 planned obligations; all 50 Python tests pass. `lake build Xsh` passes; the axiom allowlist accepts 8,279 declarations, all three negative controls are rejected, the prohibited-term scan is clean, and the guarded fresh kernel check passes. Schema 2 remains provisional with null `source_identity`, no finite conformance cases, and all 72 obligations planned. | Record-update overlap mirrors the current checker's path-prefix rule, which `docs/SPEC.md` currently omits. Replacement values are supplied after checking; the fragment does not prove checker or Rust correspondence. Map literals use an already validated finite primitive key domain and abstract evaluator outcomes; they do not model arbitrary effects, parsing, key validation/coercion, or cross-domain conversion. Neither fragment establishes runtime or Rust correspondence or closes full G1/G2/G3 obligations. |


| UTF-8 string search, Result propagation, and Set validation | Reviewed commits `075b3b71b`, `e3e25d986`, and `fc4061faa` add 11 roots in `Xsh.Strings.Search`, 12 in `Xsh.Results.Propagation`, and 10 in `Xsh.Validation.SetRequire`; `formal/Xsh.lean` imports each once. Search models first-match offsets over Lean's UTF-8 byte list, including empty needles and a scalar-index mutation witness. Propagation models nearest try/retry/function boundaries, plain-procedure failure unwinding, and returned Result data; compatible error embedding is an explicit premise. Set validation checks existing members and decoded-array members, preserves membership, iterates in primitive key order, and detects a deduplication mutation. The private `decoded_array_acceptance_iff_typed_nodup` helper is compiled proof support, not a coverage root. The inventory reports 396 roots across 89 owners and 72 planned obligations; all 50 Python tests pass. `lake build Xsh` passes; the axiom allowlist accepts 8,776 declarations, all three negative controls are rejected, the prohibited-term scan is clean, and the guarded fresh kernel check passes. Schema 2 remains provisional with null `source_identity`, no finite conformance cases, and all 72 obligations planned. | String search does not prove XSH storage/runtime, normalization, parsing, raw `Bytes` search, or Rust correspondence. Result propagation assumes a compatible error embedding and does not prove checking, cleanup, or effect behavior. Set validation assumes a finite ordered key table and predicate and excludes JSON parsing and diagnostic text. None establishes Rust correspondence or closes full G1/G2/G3 obligations. |
| JSON compatibility and bounded List-comprehension execution | Reviewed commits `98cfd2b02697` and `71029bfedae5` add fourteen roots each in `Xsh.Json.Compatibility` and `Xsh.Semantics.ListExecution`; the latter retains the established List semantics path. Both proof files remain byte-identical to their reviewed sources. The root imports each module once. The inventory reports 424 roots across 89 owners and 72 planned obligations; all 50 Python tests pass. `lake build Xsh` passes; the transitive axiom allowlist accepts 9,324 project declarations, all three negative controls are rejected, the prohibited-term scan is clean, and the guarded fresh kernel check passes. Schema 2 remains provisional with null `source_identity` and no finite conformance cases. | JSON Set projection covers only ordered ordinal keys and emits their ordinals as JSON integers; it does not model general `Set[T]` serialization. Record and Map entries are assumed unique, Path values need explicit conversion, and JSON text encoding/parsing, diagnostics, runtime, and Rust correspondence are excluded. List execution uses abstract finite expressions and excludes parsing, effects, streams, cleanup, and runtime/Rust correspondence. Neither fragment establishes full G1/G2/G3 closure. |
| Optional access and UTF-8 byte access | Reviewed commits `c727034ed` and `6bd4387525c2` add 12 roots each in `Xsh.Control.OptionalAccess` and `Xsh.Strings.ByteAccess`; both proof files remain byte-identical to those commits, and `Xsh.lean` imports each module once. OptionalAccess covers lazy callback ordering for field, method, index, and range hops plus Result propagation and an eager-argument mutation witness. ByteAccess covers `Str` byte lookup and UTF-8-boundary-checked slicing with an explicit byte length. The inventory reports 448 roots across 89 owners and 72 planned obligations; all 50 Python inventory tests pass. Full `lake build` passes; the transitive axiom allowlist accepts 9,479 declarations, all three negative controls are rejected, the prohibited-term scan is clean, and the guarded fresh kernel check passes. Schema 2 remains provisional with null `source_identity` and no finite conformance cases. | OptionalAccess excludes effects, cleanup, and runtime parity. ByteAccess excludes omitted-length slicing, the `Bytes` API, bracket slicing, parsing, runtime errors, `UInt8` conversion, runtime evaluation, and Rust correspondence. Neither fragment establishes Rust conformance or closes full G1/G2/G3 obligations. |
| Finite loop snapshots, POSIX path fields, and bounded JSON encoding | Reviewed commits `7202ab29f`, `878ad20ae`, and `fa5073974` add 12 roots each in `Xsh.Iteration.Snapshot`, `Xsh.Paths.Fields`, and `Xsh.Json.Encoding`; the proof files remain byte-identical to their reviewed sources, and `Xsh.lean` imports each module once. The inventory reports 484 roots across 89 owners and 72 planned obligations; all 50 Python inventory tests pass. `lake build Xsh` passes; the transitive axiom allowlist passes and all three negative controls are rejected; the prohibited-term scan is clean and the guarded fresh kernel check passes. Schema 2 remains provisional with null `source_identity` and no finite conformance cases. | Snapshot covers finite eager captured values and excludes streams/pipelines, loop control, typechecking, and runtime correspondence. Path Fields is a native-byte POSIX lexical model and does not match all Rust-backed `Path.name`, `Path.ext`, or `Path.parent` edge cases such as `foo/.`, `foo/..`, or component normalization; it makes no XSH API/Rust correspondence claim. JSON Encoding is bounded compact tree encoding; it excludes Float formatting, pretty output, parsing, and I/O, and establishes no runtime or Rust correspondence. |

| Finite loop control and lexical path normalization | Reviewed proof blobs from `2240fabf6` and `8e78cf370` add 12 roots each in `Xsh.Loops.ControlFlow` and `Xsh.Paths.Normalize`; both proof files are byte-identical to those sources, and `Xsh.lean` imports each once. The inventory reports 508 roots across 89 owners and 72 planned obligations; all 50 Python inventory tests pass. `lake build Xsh` passes; the axiom allowlist accepts 10,160 declarations and all three negative controls are rejected; the prohibited-term scan and guarded fresh kernel check pass. Schema 2 remains provisional with null `source_identity` and no finite conformance cases. | Loop control models finite loop-local outcomes and traces; it excludes checker target validity, stage callbacks, divergence, effects, cleanup, streams, and runtime or Rust correspondence. Path normalization is a native-byte lexical stack abstraction; it excludes runtime NUL admission, cwd and filesystem resolution, symlinks and confinement, path fields, construction and interpolation, and runtime or Rust correspondence. Neither fragment closes a full G1/G2/G3 obligation. |

| Collect-block publication and ordered match selection | Reviewed source commits `d5fb43ca` and `75a39dc11` add 12 roots each in `Xsh.Collect.Block` and `Xsh.Matches.Selection`; each proof file is byte-identical to its reviewed source, and `formal/Xsh.lean` imports each module once. Collect models finite yield/delegate event order, independent nested accumulators, normal publication, and kind-only preclassified escaping exits. Match selection models one subject evaluation and ordered abstract pattern decisions with supplied Boolean guard results and complete captures. The inventory reports 532 roots across 89 owners and 72 planned obligations; the inventory checker and all 50 Python tests pass. `lake build Xsh` passes; the axiom allowlist accepts 10,557 declarations and all three negative controls are rejected. The prohibited-term scan and guarded fresh kernel check pass. Schema 2 remains provisional with null `source_identity` and zero finite conformance cases. | Collect exits are already classified, retain only transfer kind, and exclude target identity, return/error payload, loop-local classification, effects, cleanup, streams, retry, checker/type inference, AST evaluation, and runtime/Rust correspondence. Match uses abstract pattern results and a supplied Boolean guard; actual pattern matching, guard expression evaluation, type and exhaustiveness diagnostics, parsing, and runtime/Rust correspondence are excluded. Neither fragment closes a universal obligation. |

| Lean batch integration | Reviewed commits `cd73dbf45`, `53301e4cf`, `8793555a3`, `fe8eb939c`, `e1bda02c1`, `f93a7d169`, and `1892b0b7b`, followed by corrected Integer commit `b7f28bd77`, were cherry-picked in that order. JSON encoding keeps its twelve root names while generalizing control-scalar escaping, Unicode preservation, and arbitrary object-field sorting; enum conversion now validates finite declarations and replaces seventeen old roots with twelve. Five modules add twelve roots each for Boolean short-circuit evaluation, raw `Bytes` access, effectful List splicing, ordered Set algebra, and bounded List matching. `Xsh.ByteCodecs.Integer` adds twelve roots for the independent integer pack/unpack definitions over widths 1, 2, 4, and 8, including lower-width unsigned limits, the signed width-eight limit, endian order, and offset-zero round trips. The root imports each module once. Static inventory checks and all 50 Python inventory tests pass; the inventory has 599 roots, 89 owner sets, and 72 planned obligations. Full `lake build` and the fresh kernel check pass; the transitive axiom allowlist accepts 11,614 project declarations, all three negative controls are rejected, and the prohibited-term scan is clean. Schema 2 stays provisional with null `source_identity`, empty finite-conformance cases, and no full universal obligation closed. | These are `SPEC_PROVED` fragments over independent mathematical models, not Rust conformance: JSON tree encoding excludes Float formatting, parsing, and I/O; `Bytes` is modeled as raw lists without UTF-8 decoding; Boolean and splice expressions use supplied outcomes rather than a general evaluator or checker; Set algebra uses ordinal keys and excludes first-insertion history; List matching supports only recursive scalar/List pattern forms; and Integer byte codecs use independent `IntValue`, `UIntValue`, and `List (Fin 256)` definitions. The Integer fragment does not prove runtime or Rust correspondence, including negative-offset validation. None establishes XSH runtime or Rust correspondence. Schema 2 stays provisional with null `source_identity`, empty finite-conformance cases, and no full universal obligation closed. |

| Raw decoder schema and effects checker bridge | Reviewed source commits `7e3a3db5` and `dc25d4d0` were cherry-picked as `50d786e2b` and `da453be39`. `Xsh.IR.Raw.DecoderSoundness` adds 12 roots for the finite four-tag row schema, successful payload constraints, malformed-row rejection, and a dropped-guard mutation witness. `Xsh.Effects.CheckerBridge` adds 12 roots that project already classified `FactSet` summaries to body-site obligations and exercise lexical creation charging, aliases, delegation, and conditional arms. The root imports both modules once. The inventory checker and all 50 Python tests pass; the inventory has 623 roots across 89 owner sets and all 72 theorem obligations remain planned. The full Lake build and guarded fresh kernel check pass. The axiom allowlist accepts 11,850 project declarations; all three negative controls are rejected; the prohibited-term scan and diff check pass. Schema 2 remains provisional with null `source_identity` and no finite conformance cases. | The decoder is a finite model of four tag kinds, not full raw-IR coverage or Rust decoder/verifier correspondence and does not close G7a. The effects bridge consumes classified summaries with captures fixed to the empty fixture; it proves no Rust classifier, checker, or runtime correspondence and closes neither EFF-LATENT nor EFF-SEMANTIC. |
| Observed scalar/integer machine foundation | Reviewed source commit `c7e25db9` was cherry-picked as `ba30fa948`. `Xsh.Semantics.Foundation.ObservedMachine` and `Xsh.Proofs.Foundation.ObservedMachine` add eight `SPEC_PROVED` roots for accepted evaluated BaseExpr configurations, observed preservation, terminal/typed-host-wait/labeled-step progress, runner non-stuckness, and arithmetic/timeNow witnesses; `formal/Xsh.lean` imports each module once. The inventory checker, all 51 inventory tests, the full Lake root build, the axiom allowlist (11,912 declarations), all three negative controls, prohibited-term scan, guarded fresh checker, and diff check pass. Coverage remains provisional at 631 roots across 89 owners with `source_identity: null`, zero finite conformance cases, and all 72 universal obligations planned. | A pending wait remains tied to its emitted typed request by `BaseHostTrace`. The fragment establishes no source, raw-IR/lowering, Rust, or concrete clock correspondence and closes no full BASE-* obligation or G1. |

## For the owner afterwards

Review theorem statements and `formal/coverage.json`, especially each `EXTERNAL_ASSUMPTION`, actual Rust/artifact identity, and any parked mandatory item. The campaign is complete only with G10 on a fresh identified checkout. Continue the staggered foundations and close G0a controls; start G0b from the post-consolidation tree. Keep all 72 full theorem obligations planned until their complete proofs and required evidence pass.
