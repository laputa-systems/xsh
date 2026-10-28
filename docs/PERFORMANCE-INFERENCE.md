# Inference evidence

Reference for the evidence the [inference campaign](../full-inference-campaign.md)
uses: the G3 matrix status, the frozen and practical corpus views, annotation
erasure measurements, and the classes of annotation that stay. The campaign
document holds the contract and the log.

## G3 capability matrix

This table is the checkable status of the section 13 matrix in
`full-inference-campaign.md` at the `master` revision that carries it. Every
test below is a native `tests/xsh/` test unless it names a Rust test. Names omit
the `test_full_inference_` prefix inside `tests/xsh/full-inference-*.xsh`. A row
lists a positive witness that executes after frontend disposal and a negative
that fails for its specific cause.

| Row | Positive witnesses | Negative witnesses | Recorded limits |
|---|---|---|---|
| Callable kinds | `callables`: `imported_generic_alias_instantiates_int_str_bool`, `private_generic_calls_infer_independent_instances`; `procedures`: `proc_branches_keep_one_int_or_str_result`; `producers`: `unannotated_generic_stream_infers_items_and_closes_early` | `callables`: `explicit_effect_bound_rejects_callback_laundering`; `producers`: `stream_rejects_incompatible_yield_types`; `typed-callables`: `test_typed_callable_fits_the_dynamic_handle_of_its_kind` | Malformed written annotations are covered by the parser and checker suites, not by a G3 witness. |
| Body composition | `bodies`: `body_patterns_guards_and_while`, `body_record_map_set_updates_and_comprehensions`, `body_try_retry_and_captured_error`; `callables`: `generic_bodies_compose_rows_results_optionals_and_loops`; `defaults`: `defaults_and_rest_parameters_keep_body_relationships` | `bodies`: `body_rejects_non_indexable_instance`, `body_rejects_incompatible_generic_branch`; `callables`: `ordering_rejects_operands_outside_its_domain`, `negation_rejects_operands_outside_its_domain`; `procedures`: `only_proc_completions_convert_error_payloads`; `diagnostic-cascades`: one diagnostic per rejected body | A `proc` statement call cannot use a `pure` callback that returns `Unit`; discard with `let _ =`. Generic negation does not accept `UInt`. A `pure` function has no proc-style `Err(Str)` conversion. |
| Callable values | `callables`: `callable_values_in_records_keep_the_outer_scheme`, `unannotated_conditional_selection_infers_one_callable`; `callable-lists`: `callable_list_generalizes_its_outer_relationship`; `captured-callables`: `returned_generic_callable_uses_immutable_capture`; `typing-imported-pure`: `test_imported_callables_pass_directly_as_arguments_to_generic_callees`, `test_imported_callable_arguments_keep_their_module_environment` | `callables`: `private_module_callables_remain_inaccessible`, `dynamic_callable_does_not_gain_a_signature`, `unannotated_callback_parameter_is_monomorphic_across_calls`; `callable-relays`: `conditional_selection_rejects_incompatible_callables`; `callable-lists`: `callable_list_rejects_an_incompatible_shared_contract`; `safety`: `mutable_callable_binding_keeps_one_instance`; `diagnostic-cascades`: `test_failed_callback_call_does_not_reject_later_valid_calls` | An exported callable alias needs an explicit contract (SPEC 9.4). A callback parameter is monomorphic within one call (rank-1). |
| Recursion | `recursive-components`: `self_recursive_scc_generalizes_after_checking`, `mutual_recursive_scc_is_declaration_order_independent`, `recursive_proc_preserves_independent_result_instances`, `recursive_pure_indexes_a_generic_receiver`; `producers`: `recursive_returns_are_inferred_monotype_per_component` | `safety`: `recursive_infinite_type_is_rejected`, `polymorphic_recursion_is_rejected`; `recursive-components`: `recursive_peer_callback_requirements_remain_independent` | The checker output of a chain of `n` forwarding compositions grows quadratically because each scheme carries its callee's applications. See the reduced-composition decision in the log of `full-inference-campaign.md`. |
| Effects | `callables`: `callback_effects_flow_through_lazy_producers`; `effect-union`: `imported_effect_union_preserves_promised_bounds_and_independent_types`; `recursive-components`: `recursive_peer_retains_callback_effects`; `stream-effects`: `stream_effect_is_lazy_through_returned_alias`; `defaults`: `stream_default_effect_is_charged_at_creation` | `callables`: `explicit_effect_bound_rejects_callback_laundering`; `effect-union`: `imported_effect_union_keeps_unknown_dynamic_effects_rejected`, `imported_effect_union_reports_a_missing_capability`; `stream-effects`: `inferred_stream_effect_alias_rejects_empty_bound`; `bodies`: `body_retry_delay_requires_time_effect`; `safety`: `returned_callable_keeps_its_effect_obligation` | None recorded. |
| Streams | `producers`: `recursive_stream_keeps_independent_lazy_item_contexts`, `mutual_streams_share_yield_cells_before_generalizing`; `callable-relays`: `imported_generic_stream_alias_retains_its_scheme_and_lazy_order`; `defaults`: `stream_default_runs_at_first_pull_and_closes_on_stop`; `bodies`: `body_yield_delegation_closes_nested_streams` | `producers`: `live_producer_keeps_its_item_type_monomorphic`; `safety`: `live_producer_keeps_one_incompatible_item_type`, `generic_alias_does_not_duplicate_a_resource_handle`, `iteration_stream_failure_preserves_pull_order_and_cleanup` | None recorded. |
| Rows and locals | `defaults`: `nested_row_projection_preserves_open_fields`; `safety`: `activation_local_mutable_accumulator_is_generic`; `relational-constraints`: `nullable_target_preserves_source_presence`, `optional_guard_preserves_original_immutable_reads`; `callables`: `wrapper_fields_preserve_null_and_optional_field_idempotence` | `defaults`: `nested_row_projection_rejects_a_missing_field`; `callables`: `wrapper_fields_reject_a_second_optional_hop`; `safety`: `shared_mutable_capture_through_record_stays_monomorphic`; `relational-constraints`: `nullable_target_keeps_nullable_field_rejection` | No dedicated witness for a duplicate-label or lacks violation. `{n: 0, ...r}` and a field assignment on a local copy of an unannotated parameter are rejected with the wording "spread must be a record" and "field assignment requires a record value"; `{...r, n: v}` works. A row override needs a lacks constraint, so the rejection is a limit, and only the wording is open. |
| Operation constraints | `callables`: `sealed_binary_arithmetic_instantiates_independent_domains`, `operator_relationships_reuse_per_instantiation`, `ordering_retains_its_domain_for_quantified_operands`, `negation_instantiates_independent_numeric_domains`; `membership`: `membership_preserves_container_domains`; `uint-arithmetic`: `test_inferred_uint_arithmetic_preserves_intrinsic_results` | `callables`: `sealed_binary_arithmetic_rejects_an_unsupported_tuple`, `operator_relationship_rejects_an_unsupported_instance`; `membership`: `membership_rejects_incompatible_domains`; Rust: `checked_dynamic_arguments_do_not_establish_generic_operation_domains` | Generic negation rejects `UInt`. |
| Dynamic boundaries | `inferred-require`: `test_require_infers_target_from_annotated_binding`, `test_require_uses_returns_branches_blocks_and_parameters`; `metamorphic`: `inline_and_named_descriptors_ground_the_same_fields`; `hidden-resources`: `scalar_extra_fields_survive_record_require` | `inferred-require`: `test_require_rejects_unanchored_targets`; `dynamic-error-boundary`: `test_dynamic_errors_require_validation_before_propagation`; `callables`: `dynamic_callable_does_not_gain_a_signature`; `diagnostic-cascades`: `test_dynamic_argument_for_a_field_reader_reports_one_diagnostic` | No witness for a phantom external schema. |
| Prepared programs | Rust, in `runtime::eval::indexed::full`: `generic_pure_sequences_calls_and_captures_execute_after_frontend_drop`, `imported_generic_aliases_execute_after_frontend_drop_on_both_routes`, `generic_proc_imports_preserve_quantified_contract_after_frontend_disposal`, `imported_generic_callable_argument_is_bound_to_its_export_function`; `metamorphic`: `exported_proc_and_stream_schemes_are_client_independent` | Rust: `full_verifier_rejects_forged_generic_call_substitutions`, `full_verifier_rejects_forged_generic_rows_and_body_facts`, `full_verifier_rejects_forged_capture_type_as_generic_argument_evidence`, `full_verifier_rejects_forged_generic_module_exports`, `full_verifier_rejects_forged_statement_callback_result_fact`, `full_verifier_rejects_future_and_stale_visible_driver_slot_identities` | No test asserts that no solver runs at run time; the property is structural. |

Section 13 also asks for metamorphic tests, an independent test-only solver and
regression examples. `metamorphic` holds eight hand-written equivalence tests:
alpha renaming and declaration order, transparent aliases, inline against named
descriptors, named-argument permutations, helper extraction, client
independence, import-root order, and explicit against inferred callables.
Nothing generates equivalent programs. No independent test-only solver compares
success, failure or schemes on generated problems. The two items below are
therefore open.

| Open G3 item | Status |
|---|---|
| Generated metamorphic programs | Not done. |
| Independent test-only solver comparison | Not done. |

G3 is not closed while either item is open.

## Annotation erasure

`dev/performance/corpus/erase_sites.py` measures how many inventory sites a
compiler lets a source tree drop. It erases each site alone, then every site
that passed alone together, and prints counts and the first diagnostic of each
failure. It writes no state file. `inventory.sh` produces the input table.
Run its test directly with `python3 -B -m unittest test_erase_sites` from
`dev/performance/corpus`; the gate plan does not select that directory.

```sh
python3 -B dev/performance/corpus/erase_sites.py SITES.tsv \
  --root xsh=XSH_TREE --root laputa=LAPUTA_TREE --xsht XSHT \
  --scratch .work/tmp/erase --kind parameter --kind return --kind local
```

Measured 2026-10-28 on `master`. On the four practical files (`core/wc.xsh`,
`core/sort.xsh`, `core/awk.xsh`, Laputa `pm/generation.xsh`; 724 sites, 668 type
annotations) the compiler accepts 585 of the 668 type erasures alone. Jointly,
`wc.xsh` and `awk.xsh` accept every site that passed alone, while `sort.xsh` and
`pm/generation.xsh` still fail on interactions. Nine checker and verifier fixes
raised the single-site count from 572 to 585: collect-item predicate keys,
membership against a ground container with a generic loop item, `exit` in a
generic proc, `if let` over an unresolved subject, annotated empty `Set` and
`Map` locals, pattern captures, enum-typed row fields, null-tested optionals,
and a `loop` that cannot break. An intermediate run recorded 581, and the table
below is from that run.

| Remaining single-site failures | Count | Reading |
|---|---:|---|
| Proc return annotation that selects the plain value contract, so callers see `T` and not `Result[T, Error]` | 20 of 23 `check.type-mismatch` returns | Source meaning, not missing inference. The other three need individual review. |
| `check.ambiguous-overload` on a parameter | 22 | Cross-type overloads stay (see Retained annotations). Check the others for genuine ambiguity. |
| `compact.indexed-build` verification or lowering | 22 | Open implementation work; first reductions are in the log of `full-inference-campaign.md`. |

Across every private parameter annotation in the 250 top-level `core/*.xsh`
commands (4,731 sites), the same tool shows 4,141 (87.5%) erasable alone after
those fixes. The 590 failures by first diagnostic are 124
`check.ambiguous-overload`, 53 `check.type-mismatch`, about 200
`compact.indexed-build` verification or lowering failures, 36 "registered
method receiver needs an annotation", 25 "generic callable body requires
immutable bindings…" from the generic-body whitelist, 38 "this function cannot
be compiled yet" lowering limits, and a tail of smaller checker classes.

A `Set[Str]` local from `set.empty()` passed to a generic callee also fails
verification with "generic call argument has no closed checked type". Open
candidate classes among the `compact.indexed-build` failures are generic
argument temporaries without a unique path, incomplete storage contracts,
condition control roles, and stream return provenance. Two `check.conversion`
cases are open.

## Frozen corpus

The frozen 1,040-removal view is the historical baseline. Do not change its
denominator or retained table, and do not mix it with practical-cohort results.

| Item | Value |
|---|---|
| Inventory | 9,194 AST sites in 48 loaded files from eleven roots: ten XSH roots and Laputa `pm/plan_json.xsh` |
| Eligible | 693 local annotations and 2,190 private parameter, return, and effect annotations |
| Accepted view | 499 locals and 541 internal annotations removed; 149 `Map[...] = {}` annotations and all 202 private effect clauses retained |
| Historical goals | 95% of locals and 85% of internal annotations, both unmet and unchanged |
| Retained table and recipe | `15243e97f6fd751016ab28690ada2cadb6fc973a` (`dev/consolidation/typing-corpus/retained.tsv` at that commit). The current `retained.tsv` and `witnesses.tsv` belong to a later 877-removal checkpoint and are not 1,040 evidence |
| Sources | XSH `ddc17d3d53c819c343f9ecaa738ac67d59e9033a`, Laputa `ca658ce1da9d89c55209c25268dfdf687ff4fdc6` |
| View digests | Original `e3429dcf2abe9524754a28458651443bc56523b50130268a1c1ff44c03e36899`, reduced `b67b498467a1dfe613573690288a8c514b9c03d0f895a4a3b20e4fd3b55968f9`, each 14,287 entries |
| Accepting compiler | `0fe1469d1808b5f963bd841165af6c10f2c3e93b`, tree `79fdb956ee01e3bf4cced9b85ff109bcb8b7d662`. Products: `xsh` `38b848c828591d6aa46a902d0404dfe42298e36cc2a6c6512eb2af9c1f460736`, `xsht` `23ca0dd9ccb8b9723f41d199ab41d4a242c90c96277ce83e13b002e67b23a2b2` |
| Behavior witnesses | One paired truncated-source witness in the 1,038 checkpoint. The 1,040 checkpoint adds `tests/xsh/system-report.xsh::test_system_report_memory_reports_malformed_and_oversized_meminfo_fields`, which covers both removed `meminfo_issue` parameters |

**Status.** Both views reconstruct with matching digests. Rebuilt with the
current compiler, the original XSH view fails with 441 errors from later
language changes (validated `RelPath` literals, record field access, proc
`Result` inference). Checking the frozen views therefore needs a
source-qualified compiler pair built from the frozen revision, as the frozen
adapter requires. The frozen sources are not migrated.

Reconstruct the views from the exact recorded sources:

```sh
scratch="$PWD/.work/tmp/performance-inference-audit"
mkdir -p "$scratch/original/xsh" "$scratch/original/laputa"
git archive ddc17d3d53c819c343f9ecaa738ac67d59e9033a | tar -x -C "$scratch/original/xsh"
git -C "$LAPUTA_ROOT" archive ca658ce1da9d89c55209c25268dfdf687ff4fdc6 | tar -x -C "$scratch/original/laputa"
git show 15243e97:dev/consolidation/typing-corpus/retained.tsv > "$scratch/retained-1040.tsv"
python3 dev/consolidation/typing-corpus/cohort.py \
  "$scratch/original" --reduced "$scratch/reduced-1040" --retain "$scratch/retained-1040.tsv"
python3 dev/consolidation/typing-corpus/apply_witness_fixture.py \
  "$scratch/original/xsh" "$scratch/reduced-1040/xsh"
python3 dev/consolidation/typing-corpus/validate.py \
  "$scratch/original" "$scratch/reduced-1040" --retain "$scratch/retained-1040.tsv"
python3 dev/consolidation/typing-corpus/view_digest.py \
  "$scratch/original" "$scratch/reduced-1040"
```

Check them with the frozen adapter and an explicit compiler product pair:

```sh
bash dev/performance/corpus/check_views.sh --frozen \
  "$COMPILER_ROOT" "$COMPILER_REV" \
  "$XSH_ROOT" ddc17d3d53c819c343f9ecaa738ac67d59e9033a \
  "$LAPUTA_ROOT" ca658ce1da9d89c55209c25268dfdf687ff4fdc6 \
  "$XSH_PRODUCT" "$XSHT_PRODUCT" "$XSH_SHA256" "$XSHT_SHA256" \
  "$RETAINED" "$VIEWS" \
  --selector xsh 'core/tests/test-wc.xsh::test_wc_characters_and_invalid_bytes'
```

Repeat `--selector PROJECT SELECTOR` (`xsh` or `laputa`) for each affected
behavior. Each selector must pass exactly once with zero skips in each view.
All source roots must be clean checkouts at their revisions. Products must be
under the compiler checkout's `.work/`. The retained table and `VIEWS` must be
under the invoking checkout's `.work/tmp/`, and `VIEWS` holds
`original/{xsh,laputa}` and `reduced/{xsh,laputa}`.

The adapter checks the ten XSH roots and `pm/plan_json.xsh` in both views and
compares checked interfaces with `xsht check --compare-with ORIGINAL_ROOT
--candidate-root CANDIDATE_ROOT PATH...`. Each comparison must exit 0 with no
error diagnostic and print exactly one line, `xsht check interface
qualification: N callable, M CLI`, with at least one nonzero count. The
comparison covers callable and CLI parsing interfaces, registered-operation
selections, published elaborations, and concrete per-declaration effect
summaries. It does not capture built-in operator selections or symbolic
callback-effect relations. Source revisions, product hashes, and both view
trees are rechecked before and after execution.

## Retained annotations

The 1,843 retained eligible rows of the frozen inventory partition into these
classes. The exact rows are `retained.tsv` at `15243e97`.

| Class | Rows | Reason | Status |
|---|---:|---|---|
| Local Map selector | 150 | 149 annotations select a Map for an empty literal, because `{}` is a Record. Site 3598 selects a Map for a nonempty literal | Purposeful. Source meaning |
| Cross-type overload | 2 `replace` controls | A `Str` receiver distinguishes string replacement from Regex replacement | Purposeful. Negative controls |
| Historical Proc inference frontier | 758 (549 parameter types, 209 explicit non-Unit returns) | The checker at `15243e97` retained them | Open. Reclassify under the current system, separating intentional contracts from missing inference. Not a permanent floor |
| Joint effect proof pending | 202 | All clauses remain in the source. Individual lint suggestions do not prove joint removal | Open. Compare each dependency component jointly |
| Interpolation observation | 1 (site 2850) | The body only interpolates the parameter. A generalized display relationship may suffice | Open |
| Source-context observations | 17 | Ten nullable branch joins, and one each for an empty-list accumulator, later-produced nullable record state, schema projection with a typed call, a single visible variant spelling, and a shared variant spelling, plus two enum-membership operands | Open. Cause review pending |
| Historical diagnostics only | 713 | A diagnostic family shows where the old erasure failed, not which relationship was missing | Open. Needs source-level witnesses |

Two further classes come from the current practical cohort.

- **Plain-value Proc return.** An explicit non-Result return annotation on a
  proc selects the plain value contract, so callers see `T` and not
  `Result[T, Error]`. Thirty-nine frozen Proc returns are of this kind, and 20
  of the 23 current proc-return `check.type-mismatch` failures. This is source
  meaning, not missing inference.
- **Return schema.** Erasing the returns of `core/wc.xsh::byte_count`,
  `add_bytes`, `count_data` and `add_counts` turns UInt fields into inferred
  arithmetic results and loses their return schemas. Keep these four.

**Cross-type overloads.** `byte_at` (`Str` and `Bytes`), `get` (`List` and
`Map`) and `replace` (`Str` and `Regex`) on an unresolved receiver report
`check.ambiguous-overload`. Section 9 makes these in-scope: a generic helper keeps
a finite sealed requirement over the registry's domains and discharges it at each
concrete use, as ordering and negation already do. They are open inference
work, not retained annotations, and they are not registry method families
(only `len`, `is_empty`, `write`, `parent` and `normalize` are). Removing the
`data` annotation from `wc.continuation`, `wc.within` or `wc.byte_words`
currently produces both `check.ambiguous-overload` and `check.result-fallback`;
`wc.sequence_width.data` stays reserved.

**Effect ceilings.** The 202 clauses split into 142 individual lint candidates
and 60 without a suggestion (ten Laputa helpers, 38 `compare_live_*` helpers,
twelve others). Source review finds no clause that is a generic callback
ceiling. The surplus declared permissions are fixed-body effect-summary
candidates. The `StageRunner` and `SystemReportModelEncoder` module contracts
are interface annotations outside this class. Erasing all 202 alone passes
`core/date.xsh` and `core/lib/system_report_live.xsh`, and adding fourteen Proc
erasures makes both fail with unresolved Proc contracts. Decide each clause by
a joint comparison, never by a lint result.

**Accounting ceilings.** If all 150 Map selectors remain, at most 543 of 693
locals (78.35%) can go. If all 758 Proc annotations remain, at most 1,432 of
2,190 internal sites (65.39%) can go, or 1,230 (56.16%) with all 202 effect
clauses. These are conditional accounting scenarios, not language limits.

## Practical cohort

The practical cohort is `core/wc.xsh`, `core/sort.xsh`, `core/awk.xsh`, and
Laputa `pm/generation.xsh`. It is separate from the frozen cohort. Its
inventory holds 724 spans: 668 type annotations and 56 effect clauses. Use
explicit clean revisions, a compiler checkout, and a product pair with their
digests:

```sh
INVENTORY="$PWD/.work/tmp/performance-practical-sites.tsv"
VIEWS="$PWD/.work/tmp/performance-practical-views"
bash dev/performance/corpus/inventory.sh \
  "$COMPILER_SOURCE" "$COMPILER_REV" \
  "$XSH_ROOT" "$XSH_REV" "$LAPUTA_ROOT" "$LAPUTA_REV" "$INVENTORY"
bash dev/performance/corpus/prepare_views.sh \
  "$XSH_ROOT" "$XSH_REV" "$LAPUTA_ROOT" "$LAPUTA_REV" \
  "$INVENTORY" "$SITE_IDS" "$VIEWS"
bash dev/performance/corpus/check_views.sh \
  "$COMPILER_SOURCE" "$COMPILER_REV" \
  "$XSH_ROOT" "$XSH_REV" "$LAPUTA_ROOT" "$LAPUTA_REV" \
  "$XSH_PRODUCT" "$XSHT_PRODUCT" "$XSH_SHA256" "$XSHT_SHA256" \
  "$INVENTORY" "$SITE_IDS" "$VIEWS"
```

`SITE_IDS` is a comma-separated list of exact `project|path|kind|line|owner`
selectors from the inventory. The scripts reject dirty or mismatched checkouts.
`apply_spans.py` rejects exported, entry, Proc, Stream, Map-typed, and
`.require()` target annotations. `check_views.sh` checks 12 XSH and three Laputa
roots in both views, runs six exact behavior selectors in each, and compares
the revealed signatures of six callables. Set `XSH_LAPUTA_CORPUS` to the
isolated Laputa checkout. There is no implicit fallback to the canonical
checkout.

`check.sh` is the behavior-only boundary for the utility roots, `wc`, `sort`,
`awk`, and the Laputa plan and generation modules:

```sh
bash dev/performance/corpus/check.sh "$XSH_SOURCE" "$LAPUTA_SOURCE" \
  "$XSH_PRODUCT" "$XSHT_PRODUCT" "$XSH_SHA256" "$XSHT_SHA256" \
  "$XSH_ROOT" "$LAPUTA_ROOT"
```

Each exact test must report `1 passed; 0 failed; 0 skipped`. Run the adapter
tests with `XSH_LAPUTA_CORPUS="$LAPUTA_ROOT" python3 dev/performance/corpus/test_check.py`
and `test_check_views.py`, and the witness tests with
`PYTHONDONTWRITEBYTECODE=1 python3 dev/performance/corpus/test_frozen_witnesses.py`.
`frozen_witnesses.py` writes one minimal source record per retained site of the
frozen views and edits neither view.
