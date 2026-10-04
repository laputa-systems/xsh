Implement a practical, incremental hardening pass on XSH’s type/effect checking and execution boundaries.

The goal is not a fully verified language, an academic formalization project, or absolute soundness. We want concrete improvements: fewer opportunities for checker/runtime disagreement, earlier detection of invalid internal states, better tests for interacting features, and a few well-supported correctness properties.

Borrow the useful industrial techniques: explicit contracts around trusted primitives, validation of lowered programs, small independently checkable analyses, targeted verification of implementation kernels, and instrumented execution. Apply them at XSH’s scale.

Make actual implementation improvements, not just recommendations. Use judgment about the best implementation and scope. The sequence below describes the direction and useful outcomes, not a mandated architecture.

## Working style and constraints

Read the repository’s contributor instructions and inspect the current implementation before choosing changes. Some of the work below may already exist; strengthen or connect it rather than duplicating it.

Start with:
- `AGENTS.md`, `docs/ARCHITECTURE.md`, `docs/TESTING.md`, and `TODO.md`.
- The relevant sections of `docs/templates/SPEC.md` and their snippets.
- The checker, lowering/verifier, execution, registry, and soundness-fuzzer code.

Likely entry points include `src/sema/check/infer_effects.rs`,
`src/sema/constraints.rs`, `src/runtime/eval/indexed/full.rs`, and
`crates/xsh-fuzz`. Treat these as navigation hints and verify their current roles.

Preserve the existing architecture. Do not add a second production frontend, competing inference engine, fallback interpreter, or wholesale replacement IR. Reuse the current checker facts, executable representation, verifier, and reference evaluator where appropriate.

Use effects as the first narrow end-to-end slice. They are a good place to connect a clear contract, a bounded analysis, executable validation, and runtime observations. Expand into types, propagation, and cleanup where a small change gives meaningful additional coverage.

Work through A–E in small, buildable increments. This is not a waterfall: a later phase may reveal an invariant worth adding to an earlier one. Prefer a few finished improvements over a large framework with incomplete coverage.

Keep production performance central. Avoid re-running inference, retaining frontend structures after preparation, expensive recursive validation on every ordinary operation, or introducing substantial allocations into hot paths. Use existing benchmarks or focused measurements for the paths you change. Expensive observation belongs behind an opt-in test/debug mechanism.

Do not add ecosystem-wide compatibility gates, external corpus management, release procedures, assurance dashboards, or a comprehensive specification-to-code inventory. Do not redesign user-facing language semantics merely to simplify a proof.

## A. Make a few important contracts precise and testable

Begin with a focused inspection of the existing effect contract and the facts carried from checking into execution.

Identify the most consequential ambiguities, assumptions, or missing checks. Do not inventory the entire language. Pick the rules that the implementation work will actually touch.

Useful questions include:

- How are known empty effects, unknown effects, unrestricted callables, and error-recovery states distinguished?
- What must remain true when a checked call is lowered?
- Where are effects accounted for when work executes later, such as stream consumption, omitted defaults, or deferred cleanup?
- Which failures are ordinary, specified runtime outcomes, and which indicate that checking or lowering broke a promise?

Clarify the relevant specification wording only where necessary. Preserve XSH’s actual distinctions: an empty effect clause is not automatically equivalent to `pure`, and absence of an outward `error` effect is not a promise that execution cannot fail in any way.

Attach evidence directly to the rules you touch. Small rule anchors, focused comments, executable examples, and named regression tests are sufficient. Use existing documentation/snippet machinery. No new traceability database or elaborate metadata format is needed.

Every implemented invariant should have a test that demonstrates valid behavior and a test that would catch its violation. Where a suspected problem is found, distinguish a reachable bug from an internal assumption that simply needs defending.

Outcome: a small set of clear, enforceable contracts that directly motivate the following code changes—not a standalone documentation project.

## B. Strengthen the existing executable validation boundary

Inspect what the current verifier actually guarantees and what it merely trusts from checking or lowering.

Choose a small number of missing semantic checks with a good benefit-to-cost ratio. Structural validity is important, but “this slot exists” is different from “using this slot here is justified.”

Potential targets include consistency among stored types, call signatures, argument/result representations, effect requirements, and propagation targets. Another useful target is preventing unresolved or recovery-only facts from reaching an accepted executable program.

Do not assume these checks are absent. Find actual gaps.

The guiding principle is:

    Make semantic decisions once; validate their consequences before execution.

The verifier should not repeat overload resolution or become another source-language type checker. It should use the finalized representation and whatever compact evidence is genuinely needed to check a decision.

Be selective about new metadata. First use information already present. Do not preserve an entire checker state or introduce a large certificate format for one additional assertion. If a desired check requires rebuilding most of the frontend, choose a narrower invariant.

Failure behavior should be explicit. Missing semantic information in an otherwise runnable program must not silently become “no effects,” a guessed type, or an executable placeholder. At the same time, preserve useful frontend error recovery: erroneous source should still receive ordinary diagnostics rather than trigger an internal crash.

Test the boundary itself. Where possible, construct a valid lowered object, deliberately corrupt one covered property, and require rejection before execution. Include a valid counterpart so the check does not simply reject everything unfamiliar.

Keep the scope honest: type/effect consistency checks do not prove that lowering preserves all behavior. A well-typed instruction can still compute the wrong answer. Continue using existing semantic and differential tests for that.

Outcome: the current verifier catches additional meaningful mistakes, with targeted negative tests and acceptable preparation cost.

## C. Give effect analysis a small, trustworthy foundation

Study the actual effect-analysis algorithm and isolate its essential properties. Separate correctness of the solver from correctness of the information supplied to it.

A solver can be correct on its input graph while the checker forgets to add an important dependency. Address both, without claiming either establishes the entire language’s effect soundness.

For the solver, useful properties include conservative propagation, correct treatment of declared versus inferred contracts, preservation of unknown dependencies, correct error masking, and independence of semantic results from graph traversal order.

Account for the implementation’s real termination behavior. If effect memberships only grow over a finite domain, explain that argument briefly. Also inspect auxiliary state, especially diagnostic witnesses or dependency chains, so it cannot keep changing indefinitely after the effect sets stabilize.

Prefer a small independent executable model for comparison where that gives useful leverage. Generate bounded graphs containing cycles, mutually recursive groups, unknown dependencies, and captured-error edges. Compare semantic results, not just whether both implementations terminate. Exercise invalid internal inputs separately from valid graphs.

For dependency collection, add focused source-level tests. Pay particular attention to calls and work that executes later. Do not treat wrapping creation in `try` as automatically proving that every later execution has its errors captured. Follow the language’s actual boundaries.

Pursue a small machine-checked result when the setup is proportionate and the claim maps closely to production behavior. A finite-domain exhaustive check may be the right choice for one operation; a proof assistant may be worthwhile for another. Do not introduce a substantial formal-methods stack merely to tick a box.

For claims that remain unproved, a concise argument plus strong independent tests is still useful. Be precise about the evidence: testing all graphs up to a chosen size is not a proof for arbitrary graphs, and proving a model is not automatically proving the Rust implementation.

Outcome: better-defined effect analysis, stronger independent tests, fixes for any discovered gaps, and a narrowly scoped stronger correctness result where practical.

## D. Verify selected Rust implementation kernels where it pays off

Look for small production functions whose correctness matters disproportionately and whose behavior suits exhaustive or symbolic checking.

Good candidates might include effect-set operations, constraint rollback, checked index/range arithmetic, or the function that combines a body outcome with cleanup failures. These are suggestions, not a checklist.

Choose one or two promising targets after inspection. Prefer properties that ordinary example-based tests cover poorly, especially boundary cases and state restoration after failure.

Use an appropriate Rust verification tool, such as Kani, if it fits the repository and the selected code. Keep the integration isolated from normal builds. Prefer harnesses that exercise actual production functions rather than a rewritten toy version that only resembles them.

Do not restructure large parts of the implementation to satisfy a verification tool. A small extraction of a genuinely useful pure helper may improve the code; a parallel verification-only implementation usually does not.

State assumptions and bounds precisely. Verification under a bounded structure size, loop count, or modeled environment is valuable, but it is not an unbounded result. A tool limitation, exhausted bound, or unsupported operation must not be reported as success.

If tool integration becomes disproportionate, stop expanding it. Preserve the useful production refactoring and add focused exhaustive/property tests instead. Clearly state which stronger checks were not completed. Do not let this phase block the rest of the hardening work.

Outcome: additional confidence in a few consequential pieces of the actual Rust implementation, without burdening ordinary development or runtime performance.

## E. Add opt-in runtime invariant checking and connect it to fuzzing

Extend the existing execution and testing machinery so selected static promises can be checked against what execution actually does.

Do not build another interpreter. Use the current executor and existing reference evaluator for their respective roles.

Start narrowly. Choose observations that correspond to the contracts improved in A–D. Possibilities include checking a call result against its checked signature, recording tracked host operations against the applicable effect contract, validating propagation targets, or checking a small set of cleanup/resource state transitions.

Not every observation needs to be implemented in this pass.

Make attribution explicit for delayed work. A stream, deferred block, or other suspended computation may be created in one place and execute in another. Instrumentation must follow the specified obligations rather than simply charging every operation to whichever frame happens to be convenient.

Observe at meaningful boundaries. Checking a registry label against the same registry label is not independent evidence that the native implementation performs the declared effects. Where feasible, observe actual host-operation entry points and test their classification separately. Be clear about what still remains trusted.

Distinguish expected runtime rejection from a broken internal promise. A failed validation of untrusted data is ordinary behavior. A supposedly concrete operation receiving an impossible representation is different. Do not make the instrumentation “pass” by classifying every discrepancy as an allowed dynamic failure.

Use an opt-in mechanism appropriate to the architecture. Disabled instrumentation should have negligible cost and should not retain unnecessary state. Enabled checks should identify the violated invariant, relevant operation, and source location when available.

Connect the checks to the existing soundness fuzzer and shrinking infrastructure. Add a small number of high-value feature combinations rather than simply increasing random seed counts. Effects with local error capture, delayed execution with consumption, and propagation with cleanup are promising starting points.

Use mocks or isolated fixtures for effectful tests, with containment independent of the checker being tested. Preserve minimized regressions. Where execution is nondeterministic, assert the permitted properties rather than imposing a single accidental ordering.

Outcome: an opt-in way to catch selected checker/lowering/runtime disagreements during tests and fuzzing, with concrete new coverage and no material normal-execution penalty.

## Completion and reporting

Follow the current repository’s testing instructions. Run focused tests throughout and the appropriate broader gates at the end. Use existing performance measurements where relevant; do not build a benchmark platform for this task.

The result should be a coherent set of core improvements, not five equally large subsystems. It is fine for one phase to produce a small check and another to uncover a substantive fix. Formal work should earn its place through concrete leverage.

Finish with a compact report describing what changed, the invariants now checked, the tests and verification actually run, any performance evidence, and important remaining limits. Distinguish production checks, tests, bounded verification, and proofs.

Do not claim XSH is now “sound” or “verified.” Explain exactly what became harder to get wrong.
