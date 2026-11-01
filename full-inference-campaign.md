# Close the remaining inference gaps

**Status:** active correctness work. Full inference qualification remains open.
All performance work and readiness checks are deferred.

The latest product is `f5f23168c`: one producer obligation per conditional
Result argument. `core/cpio.xsh` infers `hex_text.value`; `core/chroot.xsh`
infers `set_identity.opts`. Normal optimized verification products pass 188
selected pinned Linux native cases. The 54 focused Rust controls also pass.
Wider verification-profile lowering checks pass 1592 of 1598 cases. All six
failures also reproduce on `f29b246f9` and are active work below. The debug-only
type-agreement check was not run.

The [completion contract and evidence](docs/PERFORMANCE-INFERENCE.md) owns the
lasting requirements, capability matrix, corpus accounting, and deferred
performance budgets. The [SPEC](docs/SPEC.md) owns language semantics.
[Architecture](docs/ARCHITECTURE.md) owns representation and verification
invariants. Earlier implementation history stays in Git.

## Active work

| Item | Current evidence | Close when |
|---|---|---|
| Pure error boundaries | Wider lowering checks expose four stream-error capture failures. `enforce_pure_error_boundary` clears propagation for all pure non-Result returns, including upstream stream errors. Two UInt controls expect caller-side capture that conflicts with the current checked-key rule. | Preserve the SPEC's checked-key boundary while allowing stream failures to reach caller capture; reconcile the two stale controls without weakening the checked-key contract. Existing regressions include `typed_pure_pipeline_collector_preserves_observed_stream_failure` and `generic_index_operation_errors_forward_into_capture`. |
| Remaining `nc` verifier paths | Erasing `lookup.opts` loses a guarded optional field's narrowed argument type. Erasing `connect_bounded.target` loses a record producer contract across conditional `linux.connect` calls. | Infer, lower, and execute both reduced relationships and real callers, with forged-IR controls and command behavior checks. `bind_listener.port` already checks erased on the current compiler. |
| Capability coverage | The evidence matrix still lists generated metamorphic programs and an independent test-only solver comparison as unfinished. | Add generated equivalence and independent solver comparisons, including failure-specific mutation controls. Recheck other recorded limits against current source. |
| Corpus closure | The last frozen Laputa replay on `f29b246f9` passes 56/57 parameter erasures alone and the 56-site joint. The private applied view is 38 sites. Broader residual classifications are older snapshots. | Refresh current residual classes, justify retained contracts, and compare joint frozen and practical views with behavior witnesses for changed call paths. |
| Integrated acceptance | Focused slices pass. They do not establish complete current-source campaign qualification. | Complete the required matrix and corpus evidence, then run the current verification owners on the integrated source and remove superseded inference machinery. |

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
The eight core assign-target annotations stay retained pending a justified
sealed `IndexStore` design. Do not change historical cohort denominators.

Keep this work in XSH and isolated Laputa source views. Do not mutate live
systems, ordinary sibling checkouts, CI, toolchain pins, kernels, releases,
or remote repositories. New syntax, production dependencies, persistent cache
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
