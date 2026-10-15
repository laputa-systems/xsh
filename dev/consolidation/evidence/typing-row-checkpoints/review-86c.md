# Rank-1 solver follow-up review

## Current target: 86c449d9d79aa162ecbc472151e16b69394fc6c5

Tree: `8b670f8590ee57224c2cfa73528a834b192f975c`.

### Verdict: GO

My read-only source review found the four established defects fixed: directional ground/variable and ground/record adapters, flexible/rigid row binding with immutable lacks, and nested path-compression rollback/read-only conflict projection. I found no remaining blocker in that scope.

The owner reports 86c compiled. I independently read the owner's runtime evidence at `.work/tmp/typing-rank1-runtime-86c449d9d79a`: the `constraints-module` run reports 36 passed, 0 failed; all 13 exact-probe status files are `0`. This is owner-produced runtime evidence, not a build or test run by this reviewer.

The earlier HOLD sections below are historical snapshots of intermediate revisions. Their findings are preserved for review history and were superseded by 86c.

## Historical source review: 89bdd1ab7c3bb1f8910131441fdd6a4fb9e79af9

Historical tree: `6e06724810d38f8686753098eebe0cc5fd621aa3`; reviewing `src/sema/constraints.rs` against `dev/consolidation/designs/inference.md`.

### Historical verdict: HOLD

The follow-up fixes the earlier findings. Type representatives now have one `Binding` enum; `ConstraintTypeConflict` preserves rigid terms rather than projecting them to `Type::Unknown`; flexible type variables can bind to rigid IDs, retain the rigid through aliasing, and reject ground or distinct-rigid constraints (`constraints.rs:418-615, 1548-1632`). Type aliasing checks through open record bindings before union (`1287-1338, 1855-2003`); the new regression rejects the recursive alias and verifies type and row state is restored (`2507-2536`). A positive open-row alias test checks both aliases close to the same record through independent clones (`2570+`). The prior non-exhaustive match is also covered by a Variable/Variable arm. Type and row state still share `TypeConstraints` and one rollback journal. I found no evident change to ordinary scalar/type unification; queued tests remain necessary.

Two concrete gaps remain:

1. **Rigid row tails cannot unify with flexible row tails.** `unify_row_tails` handles equal `Rigid/Rigid`, but `Variable/Rigid` and `Rigid/Variable` fall through to the conflict arm (`782-864`). Thus the solver cannot relate an open row metavariable to a row skolem. `add_row_lacks` also returns success for any non-variable tail (`906-913`), so row lacks cannot be checked or propagated against a rigid tail. Add a row-skolem test: unify an empty open row with the same rigid tail, resolve it to that rigid identity, and include the scheme row's lacks constraint so no forbidden field can leak through.

2. **Failed solver constraints can compress parent paths after rollback.** `bind_rigid_inner` calls `resolve_constraint` before it records changes (`548-575`); conflict conversion also calls `resolve_constraint` after the called operation has rolled back (`578-600`). Resolving a bound `Type` or `Record` visits nested `Type::Inference` through `resolve_type_at` and `representative`; `representative` stores compressed parents even when `changes` is `None` (`1555-1632, 1646-1654, 2059-2085`). A failed rigid-vs-ground constraint on a variable bound to a type containing an aliased nested variable can therefore shorten that alias path after rollback. Ensure in-constraint resolution either journals compression or stays read-only, and add a test asserting parent links are unchanged after the failure.

No Cargo command or test was run by this reviewer. Levels/generalization remain next-stage work as directed.

## Historical directionality review: 3545a21d6 (HOLD at that revision)

The root's additional directionality concern is confirmed. `constrain_types` matches both `(Variable, Ground)` and `(Ground, Variable)` at `constraints.rs:495-506`, but always calls `constrain(variable, ground)`. The baseline and current solver contract is directional (`expected`, then `actual`; `constraints.rs:278-287`), and Optional admits a non-optional actual only when Optional is expected (`constraints.rs:1460-1462`; `types.rs:1091-1095`). Minimal counterexample: bind a fresh `a` to `Int`, then call `constrain_types(Ground(Optional(Int)), Variable(a), span)`. Expected behavior is success; this branch calls `constrain(a, Optional(Int))`, resolves `a` to `Int`, and conflicts. Reverse orientation must call `constrain(ground, variable)`.

The `(Ground, Record)`/`(Record, Ground)` pair at `constraints.rs:475-494` has the same defect. It always calls `constrain_rows(row, ground_closed)`, even when the ground record is expected. `unify_rows_inner` forwards common fields in argument order to directional `constrain_inner` (`731-738`). With ground expected `{x: Optional(Int)}` and actual row `{x: a}` where `a` is bound to `Int`, the current order compares expected `Int` to actual `Optional(Int)` and rejects; ground-first succeeds. Keep the `ConstraintType` operand order when creating the closed ground row.

`project_record_field` allocates its field type and row variable before a potentially failing operation (`382-407`). On failure those fresh cells and the fresh-variable counter remain, but both IDs are local and unreachable, and the binding operation rolls back its constraint mutations. This is bounded orphan bookkeeping, not a demonstrated type or identity leak; I do not count it as a contract blocker absent an allocation-rollback invariant.

No Cargo command or test was run for this follow-up.

## Committed fix review (86c449d9)

### Source review verdict: GO

The directional adapters now preserve operand order. `Ground/Variable` calls `constrain(ground, variable)` while `Variable/Ground` calls `constrain(variable, ground)` (`constraints.rs:542-562`). Ground/Record and Record/Ground similarly place the closed ground row on the matching side (`504-540`). The previous Optional probes now have direct counterparts in `constraint_type_ground_and_variable_preserve_operand_direction` and `constraint_type_ground_and_record_preserve_operand_direction` (`2615-2687`): expected Optional(Int) accepts actual Int; the reverse fails.

Flexible/rigid row unification handles both orientations by binding only the flexible tail and retaining the rigid tail (`916-953`). A rigid row carries a globally unique immutable ID and its own immutable lacks set; `RigidRowScope::fresh` is the sole constructor (`constraints.rs:8-10, 24-45`). `add_row_lacks` accepts only constraints already included in the rigid set and never changes it (`995-1009`). The tests cover flexible-to-rigid resolution, clone identity versus distinct skolems, and rejecting both attempted lacks strengthening and forbidden explicit fields in either operand order (`2717-2831`). I found no path that unions rigid tails, changes their lacks, or makes one ID carry contradictory lacks through the module's construction API.

Nested type and row representative lookup now selects read-only, compressing, or journaled access (`132-136, 1205-1217, 1667-1687, 2270-2280`). Constraint execution uses the shared journal for nested resolution (`constraints.rs:613-614, 775-782, 967-968, 1296-1306`); conflict projection uses `ReadOnly` after rollback (`635-657, 1681-1687`). The new nested alias regression checks both type parent links survive a failed Variable/Rigid constraint (`2689-2715`), and the existing failed-unification test checks rollback of rank, binding, and paths (`3049-3086`). I found no remaining post-rollback compression path in the reviewed solver call paths.

The earlier two HOLD findings (flexible/rigid row support and post-failure path compression), plus both direction reversals, are superseded by this fix. The `project_record_field` note remains nonblocking: failure can retain unreachable fresh cells, but I found no escape or semantic effect. This was a source review only; the owner runtime evidence is summarized at the top. Levels and generalization remain next-stage work.
