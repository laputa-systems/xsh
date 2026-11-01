# Close the remaining inference gaps

**Status:** active correctness work. Full inference qualification remains open.
All performance work and readiness checks are deferred.

The [completion contract and evidence](docs/PERFORMANCE-INFERENCE.md) owns the
lasting requirements, capability matrix, corpus accounting, and deferred
performance budgets. The [SPEC](docs/SPEC.md) owns language semantics.
[Architecture](docs/ARCHITECTURE.md) owns representation and verification
invariants. Earlier implementation history stays in Git.

## Active work

| Item | Current evidence | Close when |
|---|---|---|
| Guard composition | Closed on `ef2bf422d`: guarded values reach typed and untyped locals and returns (`e2c2e4736`), inferred `if let` payloads live under a non-null guard, and the `cpio.validate_options` erasure checks. The rewrite passes all 427 `full-inference` native tests, the 1,639 `runtime::eval` verifier tests and the `test-soundness` scenario. Inferred `while let` still reaches `guard_source_duplicated`; baseline rejects that fixture too. | Keep the forged-guard, capture and single-evaluation checks green; reopen for `while let` when a source view needs it. |
| Deferred domains | Qualified error-family matches, target-typed enum variants, and conversions through iteration or slices have paired annotated/inferred failures. Method-to-method and method-to-iteration relationships are integrated. | Resolve canonical domains at checked instantiation, preserve genuine ambiguity and nominal identity, and qualify both execution routes. |
| Corpus residuals | The prior 4,680-site private core scan passed 4,587 erasures individually, with several failing joint views; it needs replay on the combined product. Btop's 39 sites remain unmeasured. The frozen 1,040-removal baseline remains separate. | Reduce genuine inference failures, justify retained source contracts, and close joint views without changing historical denominators. |
| Practical source views | On `2272fc6fb` the applied view (405 private parameter sites in wc, sort, awk and Laputa `pm/generation.xsh`) passes every site alone and all four joint views. Joint-erased `sort -n` produces identical output but uses about 1.8 times the CPU of the annotated source; that cost is recorded for the deferred performance campaign. | Compare joint behavior and interfaces for wc, awk and `pm/generation.xsh`, and rerun the final host-consumer guest drill. Retain exported `write_generation_plan.path_value`. |
| Integrated acceptance | `ef2bf422d` passes all 427 `full-inference` native tests on the host (not yet replayed in the pinned container) and the 1,639 `runtime::eval` verifier tests. Debug-only type agreement remains open. | Run affected verification owners, soundness families and final corpus views on the integrated compiler; remove superseded inference machinery. |

## Constraints

Give each worktree its own Cargo target directory and freeze accepted products
before replay. Prepare independent repairs in isolated worktrees and integrate
each checker, lowering, and verifier slice
together. Keep independent producer, effect, ownership, and forged-IR checks.

Preserve rank-1 inference, monomorphic recursive components, container
invariance, nominal schemas, callable kinds, and explicit public contracts.
Never use `Any`, recovery types, caller training, runtime inference, or relaxed
verification to make a source view pass.

Keep intentional annotations separate from implementation gaps. Private proc
parameters are eligible for the applied view. Exported parameters, private
proc returns, effect clauses, and Map/require locals remain retained there.
The exported Laputa `write_generation_plan.path_value` contract remains.
The eight core assign-target annotations pass individually; their joint source
view still needs replay.
Do not change historical cohort denominators.

Keep inference changes in XSH and isolated Laputa source views. Registry and
h12tiny migration is integrated. The separately authorized
[Asterinas campaign](../laputa/docs/CAMPAIGN-ASTERINAS-COMPATIBILITY.md) owns its
local kernel execution boundary. Do not change live systems, CI, toolchain pins,
or releases. Git pushes remain unauthorized. New syntax, production dependencies, persistent cache
formats, higher-rank inference, user traits, new runtimes, and Lean expansion
remain excluded.

## Acceptance loop

Read `AGENTS.md`, the relevant code and tests, and the owning SPEC section.
Reproduce each gap in a native test before repairing its authority boundary.

Use current prepared verification products for the exact regression first:

```sh
"$XSHT_BIN" test --exact "$SELECTOR"
```

Run a build only when its code inputs changed. Observe successful completion
before launching dependent checks. Keep optimized milestone builds out of the
edit loop and avoid queuing test builds behind an unnecessary product rebuild.
Debug Rust units may cover non-process boundaries. Native and process tests
require verification or release products in the pinned Linux environment.

Inspect the affected gate selection and run its required checks:

```sh
cargo dev gate --plan PATH...
cargo dev gate PATH...
```

Add exact inference-native selectors when path ownership does not select them.
Require the intended nonempty set and zero unexplained skips. Reuse evidence
only while its code, products, dependencies, environment, and scope still match.
Group wider checks at coherent integration milestones. Full runs remain behind
the explicit request boundary documented in [Testing](docs/TESTING.md).

Use the existing `inventory.sh`, `erase_sites.py`, and paired corpus adapters
from [Inference evidence](docs/PERFORMANCE-INFERENCE.md) for annotation claims.
Keep scratch under ignored `.work/`. Commit each accepted product slice.
Rewrite this active status after a landing and keep the implementation log in
Git. Do not restart completed or parked campaigns to increase lane count.
