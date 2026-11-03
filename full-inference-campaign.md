# Close the remaining inference gaps

**Status:** complete on `73b59f11d`. The accepted residuals below stay open as
individual work items. All performance work and readiness checks remain deferred.

The [completion contract and evidence](docs/PERFORMANCE-INFERENCE.md) owns the
lasting requirements, capability matrix, corpus accounting, and deferred
performance budgets. The [SPEC](docs/SPEC.md) owns language semantics.
[Architecture](docs/ARCHITECTURE.md) owns representation and verification
invariants. Earlier implementation history stays in Git.

## Closing state

| Item | Closing evidence | Left open |
|---|---|---|
| Guard composition | Closed on `ef2bf422d`: guarded values reach typed and untyped locals and returns (`e2c2e4736`), inferred `if let` payloads live under a non-null guard, and the `cpio.validate_options` erasure checks. Inferred `while let` still reaches `guard_source_duplicated`; baseline rejects that fixture too. | Reopen for `while let` when a source view needs it. |
| Deferred domains | The named cases landed: qualified error-family matches (`5c851b4fc`), target-typed enum variants (`0aa92f1ef`, `98423dbc0`, `d71d0f84a`), conversions through indexed iteration (`33322c4b6`) and through slices of an indexed list (`2cd7cedbe`), each with paired annotated and inferred native tests. An unanchored `as Path` over a generic operand is not ambiguity: the callable requires `Str` or `Bytes` and each call chooses its row (SPEC 6.11). | Keep genuine ambiguity rejected. Both execution routes are exercised only where Rust fixtures call `execute_both`. |
| Record spread soundness | A field written before a record spread must be named by the spread operand's type or written again after it (`check.spread-hides-field`, SPEC 6.4); the verifier rejects the same shape in ground record writers, derived record types and generic record shapes. A loop (`loop`, `while` with its condition, `for`, `retry` with delays) ends, before the first pass, every fact about a binding or path its body assigns and every fact about a mutable binding when its body uses a proc or another effectful callable (SPEC 5.4, `tests/xsh/narrowing-loop-back-edges.xsh`). The cold verifier keeps a field guard across a loop only when the whole loop cannot write the guarded path (not through the path, a prefix, or the whole variable) and, for shared mutable storage, calls no shared writer (`73b59f11d`); forged-IR tests cover each write form, nested loops and the condition, `guard` and `guard let` anchors. | The ground verifier does not prove every record literal against its checked type: ground field types are the checker's decision, and a forged field of an unannotated local record literal reaches a dynamic value check (`invalid lowered binary operation`, or the typed call's callable-kind check). A call through a receiver whose type is unknown at the loop head, or not reachable by a name path, is not counted as effectful, matching the call-time invalidation. |
| Practical source views | On `73b59f11d` the applied view (405 private parameter sites in wc, sort, awk and Laputa `pm/generation.xsh`) passes every site alone (367 core and 38 `pm/generation.xsh`) and all four joint views. | Compare joint behavior and interfaces for wc, awk and `pm/generation.xsh`, rerun the host-consumer guest drill, and find the erased-source CPU gap (joint-erased `sort.xsh` is identical in output and costs about 2.3 times the annotated source's CPU on the faster sort build). Retain exported `write_generation_plan.path_value`. |
| Integrated acceptance | On `73b59f11d`: native suite 4,729 passed, 0 failed, 34 skipped (verification build, 32 jobs); gates `test-sema-unit` 959, `test-sema` 55, `test-lowering` 1,652, `test-type-agreement` 1, `test-soundness` 18 (its fixed 1,500-seed generated set contains seed 866), `test-syntax` 58 and `check-docs` 3, all green in the pinned container; direct `runtime::eval` 1,652 passed; `xsht check core` clean over 929 files. | Debug-only type agreement was not run. |

## Corpus residuals

On `73b59f11d` sources the private core parameter view (250 files, 4,677 sites, Btop excluded) passes 4,668 sites alone, up from 4,587 of 4,680 when the closure lanes began. The per-file joint view of each file's passing sites passes in 245 of 250 files (4,307 sites); `iw` (69), `lsipc` (58), `mdev` (100), `patch` (89) and `pstree` (45) fail. `iw`, `lsipc`, `mdev` and `pstree` each pass once the one annotation named below is kept. Btop's 39 sites remain unmeasured. The frozen 1,040-removal baseline remains separate. Historical cohort denominators are unchanged.

Retained source contracts (4 sites):

- `setfacl:12`: `apply([{kind: .Replace, ...}])` passes a target-typed variant to an erased parameter whose body does not fix `kind` to the enum. Target-typed variants are deferred only for equality (`SealedOperation::TargetVariant`); extending the deferral to argument positions would be a new language rule.
- `date:8`: a `null` default establishes no parameter type.
- `smartctl:447` and `smartctl:468`: the `Any` payload leaves its empty list unanchored (`finish.session`), so the empty list needs its type.

Retained joint annotations: `pstree` `print_user_roots.name` with `has_same_named_user_parent.name` (erased together the callee stays generic over an equality with `Str` and a generic body cannot capture the top-level `List` of process records; keep one). `mdev` keeps `apply_rule`'s `Captures` annotation because the `values: []` argument in `handle_event_rules` has no other anchor. `lsipc` keeps any one of `make_sysv_entry.record`, `make_sysv_entry.selection`, `posix_entry.selection`, `posix_entries.selection`, `row_for.selection` and `selected_rows.selection` (erased together, `main` fails callable generalization with a recovery type). `iw` keeps its annotation pair, now a check diagnostic. A generic `item.mark != null` or `item.mark ?? d` over a plain `Int` field is not a gap: it is the specified optional injection and matches the `Int?` field twin, while the `Int` annotation is the stricter contract (`test_full_inference_null_test_over_a_plain_field_is_the_optional_injection_twin`).

Accepted open residuals, each with its reduction and lead:

- `wget:168` and `cpio:1490` reduce to `let options: Options = {output: value, tries: 1}` over an unresolved `value` followed by `number(value)`, so the result depends on statement order: the assignment to the `Str?` field is deferred, but the literal keeps the field type `value`, and the comparison with `Options` unifies it with `Str?`. Typing the field by the optional it is stored into repairs both sites but regressed `find:205`, `numfmt:1551`, `route:313`, `smartctl:190`, `wc:381` and the `wget` joint view with `generic body expression disagrees with its checked type`, because the verifier compares each record field expression with the literal's field type and has no coercion site for a deferred optional assignment in a record literal. A repair needs that site in the verifier with forged-IR tests, or a per-field deferral in the record-to-schema comparison.
- `namei:205` reduces to `guard let target = entry.link else { ... }` followed by passing `entry` whole to `path_entries`, whose erased `parent` is copied to `var previous` and later receives a plain `Entry`: instantiation binds `parent` to the field-narrowed record, and the store of an `Entry` fails `storage requires Record, found Record`; the annotated `Entry?` parameter accepts both. Passing the unnarrowed record would drop the proof that a whole-record read under a field guard gives a callee, so the join belongs in the store (the narrowed record is a refinement of the stored one).
- `arp:216` reduces to `proc show(name) { var wanted = null; wanted = name; if wanted != null { ... } }`: the deferred assignment `seed? <- name` anchors nothing, so the local publishes the free `seed` (`material type needs an annotation`). Binding the seed to the assigned variable when it is still unresolved at finalization would anchor it, and needs a decision on the nested optional when the variable instantiates to `Str?`.
- `expr:547` (`parse_branch.state`): the ground producer proof finds no executable type for an `ExprBinary` in the erased view. Not reduced; the statement-level reduction kept 500 lines.
- `patch` joint view: fails only when all nine `opts` parameters (`note`, `q`, `run_patches`, `one_patch`, `apply_file`, `backup_name`, `make_backup`, `write_rejects`, `run_hunks`) are erased together. Generalization returns `UnresolvedGuardScope` from `generalize_guard_calls` in `src/sema/constraints/wrappers.rs`: a guard call instance is still `Pending` and no `GuardCallSource` was recorded for it. The failed generalization leaves `run_patches` monomorphic, and `main`'s two calls (one inside `if let dir = opts.directory`, passing the field-narrowed record; one in the `else`, passing `Options`) then also report `inferred types disagree: expected Record, found Record`. A line-level reduction keeps unresolved-name cascades, and a hand-written mimic (`if let` guards, a `cd` scope and two calls of a generic proc) checks on baseline. An experiment that records a guard-instance watermark around `without_pending_call_instantiations` and skips the guard instances created inside it when generalizing makes the joint view pass, which fits instances left over from the discarded return-inference passes; it was not landed because no reduced reproducer pins it, so it has no test that fails without it and the corpus cannot show that no required instance is skipped. The next step is a reproducer that fails on that leftover instance.

Closed during the final lane: a `var` record guarded outside a loop (`if entry.file != null { for ... { total += entry.file } }`) was rejected at build time with `checked optional field receiver has no non-null branch proof` when the function also wrote a sibling field and the program had a top-level `assert`; the checker accepted it and the verifier now does (`73b59f11d`).

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
