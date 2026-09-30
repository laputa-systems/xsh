# Ergonomics audit: proposals 0–3

This audit records the checked implementation and migration coverage for
`ergonomics-0.md` through `ergonomics-3.md`, inspected on 2026-09-29 and rechecked
for literal migration coverage on 2026-09-30. It is an
implementation audit, not a language specification. The contracts remain in
`SPEC.md`, `SPEC-TYPING.md`, and `STREAMS.md`; `TEST-MAP.md` owns verification
routing.

Most proposed transformations have a stable rule and focused acceptance tests.
Feature existence does not imply that every before/after example is an available
autofix. The main exact-example limitations are suffix slicing, declaration-wide
default movement, parameterized cleanup helper inlining, and list reconstruction
without a checked current-length proof. Guaranteed ordinary-record get access
was missing and was added during this audit. An unsafe Map iteration fix was
reproduced and restricted to immutable receivers.

The tables use these statuses:

- **Supported:** the illustrated transformation is recognized when its stated
  checked prerequisites hold. The fix may retain annotations, mutable binding
  spelling, or explicit arguments that the illustration omits.
- **Subset:** a rule exists, but the exact illustration needs additional proof,
  configuration, or a supported source shape.
- **Manual:** the illustrated edit changes a broader contract or lacks a safe
  local equivalence proof; retaining it is deliberate.
- **Policy:** a language, integration, or verification requirement rather than a
  source rewrite.

Unless qualified, rule methods below belong to `crates/xsht/src/lint.rs`, and
Rust test symbols belong to `crates/xsht/tests/lint.rs`. Listed tests are evidence
owners, not a claim that every listed test ran during this audit. The final
verification section records the commands actually run for the two changes
owned by this audit.

## Proposal 0: membership and assertion policies

| Section | Coverage and exact transformation | Owner and evidence |
|---|---|---|
| 1. Statement versus value classification | **Policy.** A checked Bool in statement use asserts; genuine value tails, conditions, callbacks, arguments, explicit discards, and Result handling remain separate. No unused-value heuristic authorizes a rewrite. | Checker statement-use facts, `CheckOutput::assertion_spans`, indexed assertion lowering; `tests/xsh/assertions.xsh`, `tests/xsh/ergonomics.xsh`. |
| 2. Assertion failures | **Policy.** Core `AssertionError` uses nominal Result/error propagation and ordinary unwinding. Restricted effects, retry boundaries, defer ordering, and failing CLI exit behavior require execution coverage independently of lint fixes. | `tests/xsh/assertions.xsh`; `runtime::run::bare_boolean_assertion_is_an_unsuccessful_cli_exit`; `runtime::eval::lowered_ops::assertion_detail_tests`. |
| 3. Canonical membership | **Supported.** Resolved removed standard methods become `in`/`not in`; distinct prefix, regex, filesystem, lookup, and stream APIs remain. Dynamic unsupported operand pairs and user-defined names do not receive guessed operator conversions. | `lint.prefer-in`, `lint_prefer_in`, `membership_types_supported`; `linter_autofixes_removed_membership_using_checked_identity`, `linter_migrates_set_negation_and_stream_item_membership`, `linter_does_not_rewrite_user_fields_named_contains_or_has`. |
| 4. Architecture and diagnostics | **Policy.** The checker prepares assertion facts for indexed execution. Tooling uses checked standard-call identity, source spans, and CST guards; migration is not an executable compatibility API. | `LintOptions::{statement_expression_spans,membership_migration_spans,standard_call_spans}`; `docs/ARCHITECTURE.md`; the independent CLI witness above. |
| 5. Safe source-preserving fixes | **Supported/subset.** Plain assertion helpers migrate only in assertion statement use. Consumed containment Results and custom messages retain a Result-valued `test.ok` call. Operand reversal requires an inert proof or whole-statement snapshots; unsafe nested contexts remain diagnostic-only. | `lint.prefer-bare-assertion`, `lint_assertion_helper`, `lint_membership_replacement`; helper matrix below. |
| 6. Corpus and reference migration | **Policy.** Removed API errors retain migration metadata, and rewritten imported sources are deduplicated. Negative/migration fixtures intentionally retain old spellings. This audit did not rerun a complete maintained-corpus inventory or certify every reference snippet. | `crates/xsht/src/cli/lint.rs`; `cli::membership_migration_after_removal_fixes_shared_import_once_and_is_idempotent`. |
| 7. Verification | **Policy.** Isolated fixtures cover removal, import sharing, messages, operand order, and unrelated errors. Native runtime, registry/API, feature-disabled, and platform gates are separate evidence obligations. No Linux verification was performed here. | `cli::membership_migration_does_not_suppress_unrelated_checker_failure`; `TEST-MAP.md` assertion and tooling gates. |

The goal example, `test.contains(output.stderr, "attempt 3")?` followed by
`test.eq(actual, expected)?`, is represented by the following migration rules.
Checked receiver and operand types still determine whether the exact occurrence
can be fixed.

| Before | Supported replacement and limit | Focused tests |
|---|---|---|
| `container.contains(item)` | `item in container`; checked removed standard identity and supported domain. Operand order must be proved equivalent. | `linter_autofixes_removed_membership_using_checked_identity`, `linter_preserves_membership_operand_order_with_statement_bindings`. |
| `! container.contains(item)` | `item not in container`; preserves original negation span and grouping. | `linter_composes_nested_unicode_membership_and_grouped_receivers`. |
| `mapping.has(key)` | `key in mapping`; checked Map key type, including supported standard set membership migration. | `linter_autofixes_removed_membership_using_checked_identity`, `linter_migrates_set_negation_and_stream_item_membership`. |
| `record.has(field)` | `field in record`; checked Record and Str field name. | `linter_autofixes_removed_membership_using_checked_identity`. |
| `test.ok(condition)?` | Bare condition only in checked assertion statement use, including valid implicit statement handling without `?`. Intentional consumed Results remain calls. | `linter_migrates_assertion_helpers_only_in_statement_use`. |
| `test.eq(actual, expected)?` | `actual == expected` with compatible checked comparison types. Named reordered effectful operands are snapshotted in original order. | `linter_preserves_named_argument_order_and_hygiene`, `linter_named_assertion_snapshots_preserve_runtime_source_order`. |
| `test.ne(actual, unexpected)?` | `actual != unexpected` under the same typing and statement-use proof. | `linter_migrates_assertion_helpers_only_in_statement_use`, `linter_named_assertion_snapshots_preserve_runtime_source_order`. |
| `test.contains(container, item)?` | Membership assertion, or `test.ok(membership, message: ...)` when message/Result use must remain. Legacy `Any` inputs unsupported by `in` remain manual. | `linter_migrates_assertion_helpers_only_in_statement_use`, `linter_leaves_dynamic_and_comment_bearing_migrations_actionable`. |
| `test.not_contains(container, item)?` | Negated membership with the same message, Result, type, and order constraints. | `linter_migrates_assertion_helpers_only_in_statement_use`, `linter_migrates_set_negation_and_stream_item_membership`. |

## Proposal 1: coordinated language forms

| Section | Exact example coverage and refusal boundary | Rule, method, and tests |
|---|---|---|
| 1. Value-producing if/match tails | **Supported.** The illustrated `label` match loses its branch-tail returns. Checked value position and exact compatible tail type are required; lexical returns in callbacks and conditional non-tail returns stay. | `lint.redundant-tail-return`, `lint_redundant_tail_return`; `linter_removes_checked_tail_returns_in_value_branches`, `linter_tail_return_preserves_grouping_and_unicode_comments`, `linter_keeps_conditional_and_callback_lexical_returns`. |
| 2. Named argument punning | **Supported.** `compile(root: root, target: target, jobs: jobs)` becomes punned arguments with the same checked lexical identifiers. Comments inside the replaced argument prevent its fix. | `lint.prefer-named-argument-pun`, `lint_named_argument_pun`; `linter_named_argument_pun_fix_preserves_resolution_comments_and_converges`, `linter_named_argument_pun_requires_checked_identifier_resolution`. |
| 3. List concatenation and compound assignment | **Supported/subset.** Simple mutable `files = files.push(file)` and `.extend(more_files)` fix for stable arguments, including Duration literals. Nested/effectful targets and arguments refuse; multiline/commented shapes receive guidance. Useful expression methods remain. | `lint.prefer-list-compound-assignment`, `lint_list_compound_assignment`; `linter_list_compound_assignment_is_checked_and_converges`, `linter_list_compound_assignment_refuses_unchecked_effectful_and_nested_updates`, `linter_list_compound_assignment_retains_multiline_comments`; library `literal_migration_tests::list_duration_literal_updates_preserve_the_checked_element_domain`. |
| 4. Half-open slicing | **Subset.** `data.slice(0, 16)` fixes to `data[..16]`. The exact suffix fixes when an immutable literal-origin Bytes value or alias proves length at least 16; a merely typed Bytes parameter still refuses. Proven suffixes accept omitted count or the same receiver's `len() - offset`; bounded literal counts become clamped constant end bounds without runtime arithmetic. Unknown/mutable bounds, changed receivers, effects and internal comments stay explicit. | `lint.prefer-slice`, `byte_slice_replacement`, `literal_byte_slice_bounds`, `proven_immutable_byte_length`; `linter_prefer_slice_fixes_proven_byte_bounds_and_converges`, `linter_prefer_slice_retains_uncertain_offsets_counts_and_overflow`; library `literal_origin_byte_suffixes_preserve_bounds_and_converge`, `byte_suffix_migration_retains_unknown_mutable_and_distinct_receivers`; `tests/xsh/slicing.xsh`. |
| 5. Chained ordering comparisons | **Supported/subset.** `0 <= offset and offset < limit` fixes when the shared operand is a stable immutable binding. Repeated calls are not coalesced merely because they are pure; changed mutable reads and comments refuse. | `lint.prefer-comparison-chain`, `lint_comparison_chain`; `linter_comparison_chain_coalesces_stable_operands_and_converges`, `linter_comparison_chain_preserves_calls_mutable_reads_and_comments`. |
| 6. Nested/renamed record destructuring | **Supported.** Adjacent unannotated extractions from `config` combine into nested destructuring, including `target: target_name`. Required checked fields must exist. Meaningful annotations, intermediate bindings, comments, dynamic schemas, and effectful root evaluations remain. | `lint.prefer-record-destructuring`, `lint_record_destructuring`; `linter_record_destructuring_fix_roundtrips_and_converges`, `linter_record_destructuring_retains_annotations_comments_and_effectful_roots`; `tests/xsh/record_binding.xsh`. |
| 7. Multi-clause comprehensions | **Supported/subset.** The fresh `sources` accumulator and transparent nested for/if shape convert. The fix retains `var` and its annotation rather than assuming a later immutable binding contract. Each body must contain only the next qualifier or final accumulation; effects/control transfers outside that shape refuse. | `lint.prefer-list-comp`, `lint.prefer-map-comp`, `accumulator_qualifiers`; `linter_multi_clause_accumulators_have_safe_idempotent_fixes`, `linter_multi_clause_map_accumulator_retains_annotation_and_filters`, `linter_multi_clause_accumulators_keep_uncertain_loops`. |
| 8. Nonbinding pattern tests | **Supported.** `Ok(_) => true / Err(_) => false` becomes a pattern test with checked complementary Result patterns. Captures, guards, body effects/comments, and unsupported complements refuse. | `lint.boolean-pattern-test`, `lint_boolean_match`; `tests/xsh/pattern-tests.xsh::test_pattern_predicate_lint_fixes_converge_and_formatter_retains_syntax`, `test_pattern_predicate_lint_preserves_comments_and_bindings`. |
| 9. Optional postfix operations | **Supported/subset.** The stable Optional name/null-check/trim/default example fixes. The rule also recognizes selected field/index/slice shapes. Present results that could change null fallback behavior, comments, mutation evidence, or unknown receiver wrappers refuse. | `lint.prefer-optional-postfix`, `lint_optional_postfix`; `optional_postfix_fix_preserves_null_fallback_and_converges`, `optional_postfix_fix_refuses_mutation_comments_and_optional_results`; `tests/xsh/optional-postfix.xsh`. |
| 10. Guarded value control flow | **Supported.** A single-action `if cached != null { return cached }` becomes a guarded return. Return/break/yield/delegated-yield retain conditional payload evaluation. Else branches, multiple actions, comments, or an unwieldy one-liner refuse; external run payloads are grouped. | `lint.prefer-guard`, `lint_if_as_guard`; `linter_prefer_guard_supports_value_actions_and_converges`, `linter_prefer_guard_groups_external_run_payload`, `linter_guarded_return_keeps_following_statements_reachable`. |

## Proposal 2: records, patterns, cleanup, and preparation

| Section | Exact example coverage and refusal boundary | Rule, method, and tests |
|---|---|---|
| 1. Schema constructors/defaults | **Subset/manual.** A schema-annotated literal becomes a constructor while retaining its annotation. Proven identical constant fields can disappear only when defaults already exist. Moving the example's `jobs`, `verbose`, and `features` defaults into `BuildOptions` is not automated: it changes construction behavior throughout the declaration's users. | `lint.prefer-record-constructor`, `lint_record_constructor`; `linter_record_constructor_preserves_annotation_comments_and_converges`, `linter_record_constructor_requires_static_schema_and_preserves_constant_bits`; `tests/xsh/record-constructors.xsh`. |
| 2. Typed Map iteration | **Supported/subset.** Immutable `counts.keys()` plus the first unannotated `counts.get(key)?` becomes entry iteration. Mutable maps now refuse independently of incomplete assignment scanning. Lookup annotations/comments, changed receivers, and unknown methods remain. | `lint.prefer-map-entry-iteration`, `lint_map_entry_iteration`; `linter_map_entry_iteration_fix_preserves_spans_and_converges`, `linter_map_entry_iteration_keeps_mutation_annotations_comments_and_unknown_methods`, `linter_map_entry_iteration_preserves_mutation_inside_value_blocks`. |
| 3. List literal splicing | **Supported/subset.** The illustrated `.extend` chain becomes a mixed literal with explicit splices for precise scalar/nested-List element types and unchanged conversions. Elements and effectful list expressions retain order. Comments, conversion-driven shapes, and imprecise elements refuse. A simple local update keeps the compound-assignment rule. | `lint.prefer-list-splicing`, `lint_list_splicing`, `collect_list_splice_parts`; `linter_list_splicing_rechecks_preserves_unicode_and_converges`, `linter_list_splicing_refuses_annotation_conversions`, `linter_list_splicing_retains_nested_elements_and_local_update_policy`. |
| 4. Pattern conditionals/loops | **Supported.** The illustrated two-arm Result match becomes if-let; an empty complement disappears, a meaningful complement stays as else. Selected patterns must bind and be refutable. Match guards and comments refuse. While-let language support does not imply arbitrary loop inference. | `lint.pattern-conditional`, `lint_pattern_conditional_stmt`, `lint_pattern_conditional_expr`; `tests/xsh/pattern-conditionals.xsh::test_pattern_conditional_lint_and_formatter_fixes_are_stable`, `test_pattern_conditional_lint_retains_comments_guards_and_error_bindings`. |
| 5. List patterns | **Supported/subset.** The stable `argv.len() == 2 and argv[0] == "build"` followed by initial `argv[1]` extraction fixes. The bound must lead the conjunction; indices/constants and extraction order must fit the proof, currently with a bound at most 16. Mutable/dynamic Lists, comments, annotation conversions, or unsafe short-circuit ordering refuse. | `lint.prefer-list-pattern`, `lint_list_pattern`; `linter_list_pattern_rewrites_stable_bounded_extraction_and_converges`, `linter_list_pattern_preserves_unsafe_bounds_mutability_annotations_and_comments`. |
| 6. Error fallback blocks | **Supported.** Identity-Ok with one Err handler becomes an error-aware fallback, retaining lazy handler effects and surrounding lexical control. Success transformations, differentiated error patterns, guards, comments, or dynamic success types refuse. | `lint.error-fallback-block`, `lint_error_fallback_block`; `error_fallback_fix_preserves_handler_effects_and_converges`, `error_fallback_fix_refuses_success_transforms_guards_and_error_patterns`, `error_fallback_flow_keeps_success_path_reachable`. |
| 7. Deferred cleanup blocks | **Manual/subset.** The exact parameterized `[fs, error]` cleanup helper is not inlined. The deliberately narrow rule handles only private, single-use, zero-parameter `[] -> Unit` helpers containing literal print/eprint commands. Binding timing, fallible cleanup, captures, and diagnostic attribution block general inlining. | `lint.prefer-defer-block`, `lint_defer_block_helpers`; `linter_defer_block_helper_fix_is_checked_and_idempotent`, `linter_defer_block_helper_refuses_captures_failures_comments_and_multiple_uses`; `tests/xsh/defer-blocks.xsh`. |
| 8. Yield delegation | **Supported/subset.** A transparent item-forwarding loop becomes delegated yield for checked List/Stream input with identical item/binder types. Explicit Result propagation stays. Filters, transforms, additional effects/control/cleanup, comments, and loop pipelines with different consumer materialization remain. | `lint.prefer-yield-delegation`, `lint_yield_delegation`; `yield_delegation_forwarding_fix_is_checked_and_idempotent`, `yield_delegation_fix_preserves_nontransparent_forwarding_loops`; `tests/xsh/yield-delegation.xsh`. |
| 9. Prepared regex literals | **Supported.** The exact valid directly propagated raw-pattern compile call fixes after decoded-text identity and regex validity checks. Invalid/dynamic patterns, consumed Results, custom contexts, recovery branches, and comments retain runtime calls. | `lint.prefer-regex-literal`, `lint_prepared_regex`; `linter_regex_literals_decode_patterns_preserve_comments_and_converge`, `linter_regex_literals_retain_invalid_dynamic_results_contexts_and_recovery`, `linter_regex_literals_retains_compile_calls_in_result_recovery_branches`. |
| 10. Private pure return inference | **Subset, configured.** The Str helper annotation can disappear when `prefer_inferred_pure_returns` is enabled and complete checked return/expression facts remain identical. Configured return-annotation policy takes precedence. Exports, named schemas, contextual empty containers, Result wrapping/conversions, and recursive constraints remain explicit. | `lint.prefer-inferred-pure-return`, `lint_inferred_pure_return`, `checked_return_removal_facts`; `private_pure_return_removal_is_opt_in_exact_and_convergent`, `private_pure_return_removal_retains_context_and_result_boundaries`; `tests/xsh/private-pure-inference.xsh`. |

## Proposal 3: guards, fields, calls, and mutation

| Section | Exact example coverage and refusal boundary | Rule, method, and tests |
|---|---|---|
| 1. Boolean guards | **Supported.** Int `jobs <= 0` becomes `guard jobs > 0` with the same exiting failure branch. Checker-proven termination is required. Float ordering inversion uses explicit negation to retain NaN behavior. Domain failures never become implicit assertions. | `lint.boolean-guard`, `lint_negative_if_as_boolean_guard`; `boolean_guard_fix_keeps_failure_body_comments_and_converges`, `boolean_guard_float_fix_retains_nan_negation`, `boolean_guard_fix_refuses_fallthrough_unchecked_and_binding_forms`. |
| 2. Computed Map literals | **Supported/subset.** Fresh Map initialization followed by adjacent inert set assignments collapses. The fix retains `var` and annotation rather than changing future mutability; functional set chains also work. Intervening observations, effects, accumulator references, comments, or imprecise/converting types refuse. | `lint.prefer-map-literal`, `lint_map_literal_chain`, `lint_fresh_map_initializations`; `linter_map_literal_chains_recheck_and_preserve_unicode_order`, `linter_fresh_map_initialization_retains_annotations_and_refuses_observations`, `linter_map_literal_comment_spans_have_guidance_without_fixes`. |
| 3. Block parameter convention | **Supported.** Exact outside-brace error headers migrate by replacing only the header/brace span. The old spelling is fatal for execution. Body comments survive; comments within the moving header or unrelated errors prevent publishing. | `parse.block-header-migration` mapped to `lint.block-header` by `crates/xsht/src/edit.rs::migration_lint_code`; `src/syntax/parser/stmt.rs::parse_error_handler_block_arena_only`; `tests/xsh/block-parameters.xsh::test_block_header_migration_preserves_comments_and_converges`, `test_block_header_migration_rechecks_imports_and_deduplicates_edits`. |
| 4. Field labels and known access | **Supported/subset; missing access rule repaired.** Literal identifier-shaped quoted keys become bare labels. Exact `row.get("type")?` now becomes `row.type` for a proven ordinary materialized record and guaranteed field with identical checked type. Literal records, immutable aliases, and resolved local constructors supply that proof. Host-backed/unknown records, consumed/handled Results, custom `get` members, comments, and arbitrary key expressions remain. | `lint.prefer-bare-field-label`, `lint_quoted_field_labels`; `lint.prefer-known-field-access`, `lint_known_field_access`, `proven_materialized_record`; `field_label_fixes_preserve_key_bytes_conversions_comments_and_converge`, `field_label_access_fixes_retain_dynamic_results_context_recovery_and_consumers`, `field_label_known_get_fix_preserves_nullable_values_aliases_and_converges`. |
| 5. Nested record updates | **Supported/subset.** The nested immutable config spread shape becomes disjoint dotted updates. Existing fields, checked record intermediates, compatible RHS types, and stable repeated reads are required. Mutable/effectful roots, added fields/spreads, dynamic replacements, comments, and overlapping paths refuse. | `lint.prefer-nested-record-update`, `lint_nested_record_update`; `linter_nested_record_update_fix_rechecks_and_converges`, `linter_nested_record_update_retains_unstable_reads_comments_and_new_fields`; `tests/xsh/record-update.xsh`. |
| 6. Named argument spreads | **Supported/subset.** Exact forwarding fixes only when `options` has exactly the visible fields supplied, is immutable, preserves projected field types, and the candidate rechecks. Extra configuration fields, partial forwarding, mutable/effectful receivers, comments, and changed conversions refuse. | `lint.prefer-named-argument-spread`, `lint_named_argument_forwarding`; `linter_named_argument_spread_requires_exact_stable_visible_fields_and_converges`, `linter_named_argument_spread_requires_checked_record_facts`. |
| 7. Value-pipeline holes | **Supported/subset.** Linear single-use temporaries can collapse in successive passes for directly named ordinary callees and precise scalar/nested-List input/output types. Nested calls require stable earlier arguments. Record/Result/Any retained shapes currently fail the precision gate; optional receivers, other receiver evaluations, comments, extra uses, and changed context refuse. | `lint.prefer-value-pipeline`, `lint_nested_value_pipeline`, `lint_linear_value_pipeline`, `pipeline_rewrite_preserves_types`; `value_pipeline_hole_lint_rewrites_safe_nested_and_linear_calls`, `value_pipeline_hole_lint_retains_effect_order_optional_calls_and_context`. |
| 8. List element assignment | **Subset.** The exact slice-splice reconstruction diagnoses, but fixes only when an immediately preceding literal `var` proves current length, element/List types match exactly, and RHS is stable. A comment saying the index is valid is not proof. Dynamic/refined lengths remain manual; nested assignment language support does not infer arbitrary reconstruction equivalence. | `lint.prefer-list-element-assignment`, `lint_list_element_reconstruction`; `linter_list_element_assignment_exact_bounds_rechecks_and_converges`, `linter_list_element_assignment_refuses_clipped_bounds_effects_and_comments`; `tests/xsh/list-assignment.xsh`. |
| 9. Named stream options | **Supported/subset.** The exact flags become `(jobs: jobs)` and `(desc: true)`; jobs punning can follow on an ordinary lint pass. Old flags are fatal migration syntax. Existing positional argument lists or unsafe comments refuse to avoid changed evaluation order. External argv flags remain external argv. | `parse.stream-option-migration` mapped to `lint.stream-options`; `src/syntax/parser/expr.rs::parse_legacy_stream_stage_flags_arena_only`; `tests/xsh/stream-options.xsh::test_stream_option_migration_is_fatal_and_tooling_fix_is_narrow`, `test_stream_option_migration_refuses_comments_and_unrelated_errors`; `tests/syntax.rs::stream_stage_flag_migration_refuses_ambiguous_argument_lists`. |
| 10. Pattern aliases/alternatives | **Supported/subset.** Adjacent identical Added/Changed bodies merge only after checked capture names/types remain valid. Guards, comments and capture disagreement refuse. Introducing the separate illustrated whole-value alias is authored intent, not an automatic new-binding inference. | `lint.identical-match-arms`, `lint_adjacent_pattern_arms`; `pattern_alternatives_adjacent_arm_fix_is_checked_and_idempotent`, `pattern_alternatives_adjacent_arm_fix_retains_guards_comments_and_capture_types`; `tests/xsh/pattern-aliases.xsh`. |

## Reproduced regressions and fixes

Map key iteration reads the current map on each get; direct entry iteration
retains a snapshot. The assignment-name scan does not descend into every
expression-owned value block. This valid source exposed an unsafe fix:

```xsh
var counts: Map[Int] = {a: 1, b: 2}
for key in counts.keys() {
  let count = counts.get(key)?
  let changed = if true { counts["b"] = 9; 0 } else { 0 }
  print f"$key=$count"
  let _ = changed
}
```

The original prints `a=1` then `b=9`. The offered entry-loop rewrite printed
`a=1` then `b=2`. `linter_map_entry_iteration_preserves_mutation_inside_value_blocks`
executes both witnesses in isolated temporary files using ordinary Rust output
assertions, then requires the lint to decline the unsafe fix. It failed before
the immutable-binding gate was added to `lint_map_entry_iteration`, and passed
afterward. Requiring immutability avoids depending on incomplete mutation
scanning; broader mutable-map support needs an explicit stability proof.

`crates/xsht/src/lint_callable_alias.rs::lint_callable_aliases` also prepared the
entire original program before determining whether any forwarding candidate
existed. The preparation-count regression failed on a source containing only
`let prepared = rx"ready"`: one original checker preparation ran although zero
were needed. Original checking now uses a lazy cached value after finding a
same-kind local callable target and unchanged forwarding of every parameter.
`callable_aliases_skip_original_preparation_without_exact_local_forwarding`
covers no candidate, wrong callable kind, reordered arguments and transformed
arguments. `callable_aliases_cache_original_preparation_across_forwarding_candidates`
requires one original preparation across two valid candidates. Candidate
rewrites still receive their own checks, and existing signature/comment/order
acceptance tests remain unchanged.

## Reopened literal migration regressions

The library-only regressions in
`crates/xsht/src/lint_literal_migration_tests.rs` first failed because the rules
offered no fixes for checked Path literal module data, a Duration literal push,
and four immutable literal-origin Bytes slice forms. The fixes widen only inert
literal eligibility and carry immutable byte-length provenance through lexical
bindings and aliases. Exported literal bindings also needed token-based keyword
selection: their statement span starts at `export`, not `let`.

The tests compare prepared literal data, preserve comments and Unicode source,
recheck candidate types, and require convergence. Refusal cases retain runtime
Path construction/interpolation and local initialization, computed append
arguments, unknown or mutable Bytes receivers, out-of-bounds offsets, distinct
length receivers, negative counts, internal comments, and repeated calls. The
slice acceptance test additionally covers an empty suffix, zero offset, and a
maximal literal count that clamps to the proven length without overflow.

Verification for this reopened pass was
`cargo test -p xsht --lib literal_migration_tests -- --nocapture` (five passed)
and
`cargo test -p xsht --test integration linter_prefer_slice_retains_uncertain_offsets_counts_and_overflow -- --nocapture`
(one passed). These checks use the lint library directly; no lint CLI,
formatter, release build, or corpus rewrite was run. The all-proposal matrix
remains a coverage audit, not a claim that every safe migration class is complete.

## Unfinished improvements

These are narrower coverage opportunities, not permission to weaken equivalence
checks:

- **Checked runtime suffix bounds.** Literal-origin suffixes now migrate, but
  the rule does not consume a checked minimum-length guard on an otherwise
  unknown Bytes parameter. Such a proof must retain negative, out-of-bounds,
  overflow and changing-receiver behavior. The nearest uncertain-bound test
  now uses a Bytes parameter, keeping that refusal independently covered.
- **Refined List lengths.** A checked `values.len() > 1` branch followed by the
  exact reconstruction can establish the index without an immediately preceding
  literal declaration. The current rule does not consume that proof. Tests must
  distinguish clipping slice bounds from strict element writes and invalidate
  refinements after mutation/effects.
- **Precise retained pipeline shapes.** `pipeline_rewrite_preserves_types`
  delegates its precision gate to `list_splice_element_type_is_precise`, which
  excludes Records and Results. Safe handled chains need complete material type,
  nominal identity, conversion and context comparisons before broadening it.
- **Imported candidate context.** Named-spread, identical-arm and pipeline rules
  contain standalone parser/checker candidate rechecks. Loaded user-module facts
  may be absent there even when the CLI later validates the full import graph.
  Imported-schema/callee acceptance fixtures and shared checked-workspace
  candidate validation are needed before claiming equivalent coverage to local
  source.
- **Known access beyond ordinary materialized records.** A known field type
  alone does not prove that removing get cannot drop host metadata work or
  failure. The added rule deliberately requires materialized-record provenance;
  parameters, arbitrary host-returned records and additional receiver shapes
  need stronger runtime guarantees before expanding it.

Moving defaults into a schema, changing an accumulator's future mutability,
inlining a parameterized/fallible cleanup helper, inventing a pattern alias, or
removing meaningful Result handlers are broader author decisions. Their absence
from local autofix coverage is deliberate.

## Verification recorded for these regressions

Both regressions were first observed failing. The preparation-count test was
initially invoked with a short `--exact` filter that selected zero tests; the
corrected filter ran the actual failing witness before the fix. The following
commands subsequently passed:

```sh
cargo test -p xsht --lib callable_aliases_
cargo test -p xsht --test integration callable_alias_forwarder
cargo test -p xsht --test integration linter_map_entry_iteration
```

Results were respectively two unit tests, two existing forwarding acceptance
tests, and three Map iteration acceptance tests. These targeted Rust tests
exercise isolated tooling/process boundaries. No formatter or linter command,
global autofix, pre-commit hook, Linux build, release build, remote push, or full
native/corpus gate was run for these two fixes. Their broader verification
remains routed through `TEST-MAP.md` and the overall task's verification record.
