# Close the remaining inference gaps

**Status:** active correctness work. Full inference qualification remains open.
All performance work and readiness checks are deferred.

Selector writes, fresh map factories, pipeline binders, empty argument domains,
nullable match joins, and registered dynamic results are repaired. Their product
passes 129 body cases, eight producer cases, 21 typed-map-key cases, and 1,638
runtime-owner cases. Required module returns pass the 11-case require suite.
Final combined inference qualification remains open.

The [completion contract and evidence](docs/PERFORMANCE-INFERENCE.md) owns the
lasting requirements, capability matrix, corpus accounting, and deferred
performance budgets. The [SPEC](docs/SPEC.md) owns language semantics.
[Architecture](docs/ARCHITECTURE.md) owns representation and verification
invariants. Earlier implementation history stays in Git.

## Active work

| Item | Current evidence | Close when |
|---|---|---|
| Input and field constraints | Copied mutable inputs narrow before known nullable formals can preserve their bounds. Earlier field projections block later nominal Error constraints, and optional field equality still overconstrains its operands. | Retain the required input, storage, comparison, and field relationships without widening invariant containers or changing nominal domains. |
| Method contracts | Known-receiver write overloads reject arguments whose domains become known later. Fully inferred Map.push fails generalization. | Carry registered overloads through generalization and independent verification. |
| Corpus residuals | The prior 4,680-site private core scan passed 4,587 erasures individually, with several failing joint views; it needs replay on the combined product. Btop's 39 sites remain unmeasured. The frozen 1,040-removal baseline remains separate. | Reduce genuine inference failures, justify retained source contracts, and close joint views without changing historical denominators. |
| Practical source views | Laputa passes 56 of 57 parameter erasures individually and all 56 jointly. All 38 private parameter sites pass. | Compare practical core and isolated Laputa views jointly, including behavior and interface witnesses. Retain the exported `write_generation_plan.path_value` contract. |
| Integrated acceptance | Generated native and fuzz equivalence families and independent type, row, and effect solver comparisons pass on accepted products. The debug-only type-agreement check remains unrun. | Replay affected evidence on the final integrated compiler, run required verification owners, and remove superseded inference machinery. |

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

Keep inference changes in XSH and isolated Laputa source views. The separately
authorized registry migration and h12tiny upgrade apply to both main checkouts.
Do not change live systems, CI, toolchain pins, kernels, or releases. Git pushes
remain unauthorized. New syntax, production dependencies, persistent cache
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
