# Close the remaining inference gaps

**Status:** active correctness work. Full inference qualification remains open.
All performance work and readiness checks are deferred.

The latest product slice is `f25382afc`: inferred callables authenticate
immutable computed captures through the existing producer proof.
`core/cpio.xsh` now infers `hex_text.value`. Its focused acceptance passes
163 pinned Linux native cases and 18 Rust controls with prepared incremental
verification products. Normal optimized verification replay remains a milestone
check, rather than an edit-loop prerequisite.

The [completion contract and evidence](docs/PERFORMANCE-INFERENCE.md) owns the
lasting requirements, capability matrix, corpus accounting, and deferred
performance budgets. The [SPEC](docs/SPEC.md) owns language semantics.
[Architecture](docs/ARCHITECTURE.md) owns representation and verification
invariants. Earlier implementation history stays in Git.

## Active work

| Item | Current evidence | Close when |
|---|---|---|
| Conditional Result arguments | Erasing `core/chroot.xsh` `set_identity.opts` fails with `generic argument request is duplicated`. The reduced caller below reproduces it. | Register one producer obligation per argument without losing its derived type or weakening duplicate/mismatched-evidence rejection. Execute the regression and real chroot callers. |
| Remaining `nc` verifier paths | Erasing `lookup.opts` fails with `generic call argument does not match its checked parameter slot`. Erasing `connect_bounded.target` fails with `ground record producer has no record contract`. | Infer, lower, and execute the affected relationships, with forged-IR controls and the command's behavior checks. `bind_listener.port` already checks erased on the current compiler. |
| Capability coverage | The evidence matrix still lists generated metamorphic programs and an independent test-only solver comparison as unfinished. | Add generated equivalence and independent solver comparisons, including failure-specific mutation controls. Recheck other recorded limits against current source. |
| Corpus closure | The last frozen Laputa replay on `f29b246f9` passes 56/57 parameter erasures alone and the 56-site joint. The private applied view is 38 sites. Broader residual classifications are older snapshots. | Refresh current residual classes, justify retained contracts, and compare joint frozen and practical views with behavior witnesses for changed call paths. |
| Integrated acceptance | Focused slices pass. They do not establish complete current-source campaign qualification. | Complete the required matrix and corpus evidence, then run the current verification owners on the integrated source and remove superseded inference machinery. |

The current chroot reduction is:

```xsh
type Options = {groups: Str?}
proc set_identity(opts, outer_groups: Result[List[Int]]) [error] {}
proc main() [error] {
  let opts: Options = {groups: null}
  let outer_groups = if opts.groups != null { Ok([1]) } else { Ok([]) }
  set_identity(opts, outer_groups)
}
```

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
