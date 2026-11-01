# Close the remaining inference gaps

**Status:** active correctness work. Full inference qualification remains open.
All performance work and readiness checks are deferred.

The latest accepted compiler product is `07572e8f5`. Its verification product
passes 1,601 runtime-owner Rust cases and 148 pinned Linux native cases.
The stream and UInt key boundary repairs pass their regression controls.
Three private `nc` parameters now infer, including guarded record arguments,
record spreads, and record unions. Forged producer substitutions still reject.

The [completion contract and evidence](docs/PERFORMANCE-INFERENCE.md) owns the
lasting requirements, capability matrix, corpus accounting, and deferred
performance budgets. The [SPEC](docs/SPEC.md) owns language semantics.
[Architecture](docs/ARCHITECTURE.md) owns representation and verification
invariants. Earlier implementation history stays in Git.

## Active work

| Item | Current evidence | Close when |
|---|---|---|
| Retained method families | A fully generic `.get` receiver rejects an `Int` key when instantiated as `Map[UInt, Str]`. Known unsigned receivers already validate keys. | Preserve conditional key validation, result inference, lexical error boundaries, and physical producer authentication through retained selection. |
| Mutation inference | Eight private parameters fail assignment-target checking. Unresolved row fields and indexed stores lack the complete checker, lowering, and verifier relationship promised by the SPEC. | Infer row and List or Map writes, including compound and nested writes, while preserving invariant storage, scope, ordering, and checked keys. |
| Corpus residuals | A fresh 4,680-site private core scan passes 4,587 erasures individually, with several failing joint views. Btop's 39 sites remain unmeasured. The frozen 1,040-removal baseline remains separate. | Reduce genuine inference failures, justify retained source contracts, and close joint views without changing historical denominators. |
| Practical source views | Laputa passes 56 of 57 parameter erasures individually and all 56 jointly. All 38 private parameter sites pass. | Compare practical core and isolated Laputa views jointly, including behavior and interface witnesses. Retain the exported `write_generation_plan.path_value` contract. |
| Integrated acceptance | Generated native and fuzz equivalence families and independent type, row, and effect solver comparisons pass on accepted products. The debug-only type-agreement check remains unrun. | Replay affected evidence on the final integrated compiler, run required verification owners, and remove superseded inference machinery. |

## Constraints

Use one coupled compiler writer for schemes, checking, lowering, verification,
and prepared execution. Delegate only disjoint work. Keep independent
producer, effect, ownership, and forged-IR checks.

Preserve rank-1 inference, monomorphic recursive components, container
invariance, nominal schemas, callable kinds, and explicit public contracts.
Never use `Any`, recovery types, caller training, runtime inference, or relaxed
verification to make a source view pass.

Keep intentional annotations separate from implementation gaps. Private proc
parameters are eligible for the applied view. Exported parameters, private
proc returns, effect clauses, and Map/require locals remain retained there.
The exported Laputa `write_generation_plan.path_value` contract remains.
The eight core assign-target annotations remain implementation work.
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
