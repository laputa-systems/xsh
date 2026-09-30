# Ergonomics migration audit: proposals 4–7

This audit records the source-migration coverage observed on 2026-09-29 for
`ergonomics-4.md` through `ergonomics-7.md`. Language support and automatic
migration are separate conclusions: a feature can execute correctly while its
illustrative before/after transformation remains manual or unsupported.

The tables identify every numbered proposal, the actual example shape, the
implemented rule, and the nearest evidence. **Supported** means the example's
migration has a checked fix under the stated conditions. **Subset** means the
rule handles a narrower family. **Missing** identifies a bounded migration
that has no implementation or cannot reach the documented result. **Manual**
means the proposal intentionally changes policy or requires evidence the
current tool cannot establish. Refusal is not itself a language failure.

Unless another file is named, lint functions are in `crates/xsht/src/lint.rs`
and tooling test symbols are in `crates/xsht/tests/lint.rs`. Native modules
under `tests/xsh/` establish runtime contracts; their presence is not evidence
that every migration example is autofixable.

## Local boundaries and declarations: ergonomics-4

| Proposal and example | Feature and exact migration coverage | Rule, owner, and evidence |
|---|---|---|
| 1. Local Result capture: `read_port()` helper → `try { ... } ?? 8080` | `try` is implemented. **Missing:** the exact helper-elimination example receives no relevant fix. Empty-retry migration is also absent. Keeping `retry []` preserves its trace metadata; moving `return` into a local capture must preserve its enclosing function target. | No helper-elimination rule. `try_capture_migration_does_not_erase_retry_metadata_or_lexical_returns`; `tests/xsh/try-capture.xsh`. |
| 2. Bare lexical blocks: lock/cleanup statement scope; scratch/index value block | Bare statement and value blocks are implemented. The examples themselves are canonical forms, not before/after pairs. **Supported subset:** `if true { ... }` → retained bare braces only in checked statement position. The fix does not splice bindings into the parent or move cleanup earlier. Field-shaped braces remain records. | `lint.lexical-block`, `lint_lexical_block`; migration/refusal/convergence cases in `tests/xsh/lexical-blocks.xsh`. |
| 3. Core assertion: `test.eq(actual, expected, message: f"package $name")?` → `assert ...` | Core assertions are implemented. **Subset:** literal-message statement `test.ok/eq/ne` calls can migrate. The exact formatted-message example receives a diagnostic without a fix. Its old message is eager; the new message runs only after failure. Collection/dynamic equality and consumed Results need separate equivalence evidence. | `lint.core-assert`, `lint_core_assert`; `core_assert_lint_fixes_literal_context_and_refuses_eager_or_consumed_results`; `tests/xsh/assert.xsh`. |
| 4. Explicit test: `proc test_normalizes_name() -> Result[Unit]` → same-name `test`; context header example | Native declarations are implemented. **Supported:** a harness-only proc with the checked legacy signature can retain its exact name/effects and move its immutable TestContext parameter into the block header. The rule requires a native-test file and declines other source references or signature comments. Ordinary callable helpers remain procs. | `lint.legacy-test-proc`, `lint_legacy_test_declarations`; `native_test_declaration_migration_preserves_context_effects_and_is_idempotent`, `native_test_declaration_migration_declines_callers_and_ordinary_files`; CLI `native_test_declaration_*`; `tests/xsh/test-declarations.xsh`. |
| 5. Explicit enum: `type Mode = Fast \| Thorough \| Custom(Int)` → `enum Mode`; singleton `Token` | Enums and singleton enums are implemented. **Supported:** parser recovery supplies exact edits for removed union syntax. Genuine aliases are not inferred to be singleton enums. The CLI migrates and rechecks imported sources rather than retaining executable compatibility syntax. | `parse.enum-migration` → `lint.enum-declaration` in `crates/xsht/src/edit.rs::migration_lint_code`; CLI `enum_migration_fix_preserves_comments_aliases_and_imports`, `enum_migration_fix_retains_unrelated_checker_errors`; `tests/xsh/enum-declarations.xsh`. |
| 6. Prepared constants: literal `format_version` and `retry_delays` lets → consts | Constants are implemented. **Supported subset:** the exact module-level scalar/List example migrates when checked data is inert. Local literals are deliberately not all promoted. **Missing safe class:** module Path literals are excluded by the rule's inert-expression predicate even though the language supports prepared Paths. Arithmetic, const references, shorthand/spread records, and constructors also exceed this rule's narrow syntactic subset. | `lint.prefer-const`, `lint_prepared_constants`, `inert_constant_initializer`; `prepared_constant_fix_preserves_comments_and_converges`; `tests/xsh/constants.xsh`. |
| 7. Selective retry: `retry [...] on (FetchError.Busy \| FetchError.Timeout)` | Selection is implemented; the example is already canonical. **Manual:** there is no hand-written selective-loop migration. Delay initialization, attempt counts, classification, cleanup, and cancellation must agree before such a fix is safe. Adding a filter to unconditional retry changes policy. | `linter_does_not_add_selective_filters_to_unconditional_retry`; `tests/xsh/retry.xsh`. |

## Host boundaries and composition: ergonomics-5

| Proposal and example | Feature and exact migration coverage | Rule, owner, and evidence |
|---|---|---|
| 1. Signature CLI: `cli main(root: Path, jobs: Int = 4, verbose: Bool = false)` | Signature entrypoints are implemented; this is a canonical example. **Subset:** only a literal, default-only option schema with matching help, bindings, and initializer-free entry is automatically migrated. Required positionals in explicit descriptors also accept long-option spellings, so blindly replacing them with required signature positionals would change argv policy. | `lint.prefer-signature-cli`, `crates/xsht/src/lint_cli_entry.rs::signature_cli_migration`; `signature_cli_literal_schema_fix_preserves_bindings_and_converges`, `signature_cli_migration_retains_comments_and_advanced_cli_policy`; CLI `signature_cli_safe_fix_preserves_process_results_and_is_idempotent`; `tests/xsh/signature-cli.xsh`. |
| 2. Bytes stdin: `run.bytes cat < b"hello\n" ?`; captured digest input | Bytes redirection is implemented. **Manual:** the canonical examples need no migration. Temp-file handoffs are not replaced automatically because lifetime, seekability, reuse, metadata, and failure timing may differ. | Native Bytes-input cases in `tests/xsh/run.xsh` and `tests/xsh/stdlib/process.xsh`; Rust `bytes_stdin_*` process/runtime fixtures. |
| 3. Lexical context: repeated `Result.context("build", message)` calls → `ctx message { ... }` | Context blocks are implemented. **Manual by contract:** the example deliberately annotates every outbound body failure. A blanket fix would also change context kind, eager message evaluation count, label timing, and possibly the annotated failure set. No repeated-context rule exists. | `tests/xsh/error-context-blocks.xsh`, `tests/xsh/result-contexts.xsh`; no corresponding region-migration lint. |
| 4. Scope values: placeholder `revision`, `cd $repo`, assignment from `run.text` → scoped initializer; typed env overlay | Value scopes and typed overlays are implemented. **Subset:** a single fresh assignment from an inert scalar or recognized env/fs call can become a scope initializer. **Missing exact case:** the documented `run.text` assignment/command-word cd scaffold emits no scope-value fix. The implemented fix retains `var` when preserving the original binding contract; it does not promise the example's `var`→`let` change. Removed double-block env syntax has exact parser migration. | `lint.prefer-context-scope-value`, `lint_context_scope_scaffolds`; `context_scope_scaffold_fix_preserves_checked_value_type_and_converges`, `context_scope_scaffold_declines_cleanup_comments_and_placeholder_reads`, `context_scope_environment_migration_preserves_comments_and_rechecks`; `parse.env-scope-migration` → `lint.env-scope`; `tests/xsh/context-scopes.xsh`. |
| 5. Lossless Path interpolation: `fp"${root}/${relative_name}"` and compound native argv | Native-byte interpolation is implemented. **Unsafe baseline repaired:** command `.display()` deletion, `f"${path}"` → native argv, and text-to-Path roundtrip reductions could change deliberate display text into native non-UTF-8 bytes. Explicit display is now manual guidance; text/native rewrites require the UTF-8 proof described below. Human text remains text. | `lint.redundant-path-display`, `lint.redundant-command-fmt`, `lint.redundant-path-parse`, `lint.path-constructor`; `path_expression_has_utf8_bytes`, `text_path_interpolation_preserves_bytes`; `linter_command_text_path_conversions_preserve_native_byte_boundaries`, `linter_path_text_roundtrips_preserve_native_byte_boundaries`, `linter_path_constructor_utf8_text_fix_rechecks_and_converges`; native `tests/xsh/stdlib/path.xsh`. |
| 6. Typed Map: decimal PID string keys → `Map[Int, ProcessEntry]` | Typed scalar keys are implemented. **Manual by contract:** the exact domain map may be reviewed, but there is no broad encoding-removal fix. Numeric ordering, serialization, prefixes, sentinel keys, and Path display identity can differ. Externally defined textual keys remain textual. | `tests/xsh/typed-map-keys.xsh`; the numeric mount-index use is explained in `docs/ARCHITECTURE.md`; no string-key-domain migration rule. |
| 7. Duration arithmetic: `time.millis(base_ms * attempt)` → `250ms * attempt` | Arithmetic is implemented. **Subset:** `time.millis/seconds` with bounded nonnegative literal arguments can use checked multiplication. The exact runtime multiplication is not fixed: clamping/saturation and checked overflow or negative multipliers are different contracts. | `lint.duration-arithmetic`, `lint_duration_conversion`; `duration_arithmetic_conversion_fix_rechecks_and_converges`, `duration_arithmetic_conversion_retains_clamping_saturation_unknowns_and_comments`, `duration_arithmetic_conversion_refuses_custom_module_alias`; `tests/xsh/duration-arithmetic.xsh`. |
| 8. Named stream callback: transparent unary map/where blocks → named callables | Named stage references are implemented. **Supported:** both exact wrappers migrate when the checker resolves the unary call. The rule retains stage configuration order and refuses comments, extra arguments, dynamic targets, altered item expressions, and `?` wrappers. Result-valued mapping and per-item propagation are not interchangeable. | `lint.stage-callable`, `lint_stage_callable_wrapper`; `stage_callable_wrapper_fix_rechecks_and_converges`, `stage_callable_wrapper_fix_requires_exact_item_stable_name_and_no_propagation`; `tests/xsh/stage-functions.xsh`. |
| 9. Block strings: one escaped-newline formatted service string → formatted block literal | Block strings are implemented. **Subset:** constant Str concatenations with escaped newlines migrate after a decoded-value witness. **Missing exact case:** the single formatted-string example is outside this rule. Dynamic interpolation, CRLF, and trailing consumers are conservatively excluded rather than reconstructed. | `lint.prefer-block-string`, `lint_block_string_concatenation`; `block_string_concatenation_fix_rechecks_exact_bytes_and_converges`, `block_string_concatenation_fix_retains_dynamic_interpolation_comments_crlf_and_consumers`; `tests/xsh/block-strings.xsh`. |

## Proofs, capabilities, and identity: ergonomics-6

| Proposal and example | Feature and exact migration coverage | Rule, owner, and evidence |
|---|---|---|
| 1. Record proof/Boolean alias: `available` guard establishes a non-null vendor field | Proof provenance is implemented; the canonical example gains checker precision without a source rewrite. **Subset:** an unreachable `??` with matching inert fallback data can be removed. There is no general repeated-null-scaffolding elimination rule. Mutation, parent replacement, recovery joins, and captures remain proof boundaries. | `lint.redundant-optional-fallback`, `lint_proven_nonnull_fallback`; `record_proof_fallback_fix_requires_checked_presence_and_inert_data`; `src/sema/check/proof.rs`; `tests/xsh/proof-provenance.xsh`. |
| 2. Private effect summary: remove `[fs, error]` from `read_manifest` | Private effect inference is implemented. **Subset/opt-in:** `prefer-inferred-private-effects` enables removal after equivalent checked facts. Without that option the exact example is not automatically changed. Public/entry contracts and meaningful upper bounds remain. The rule currently refuses any source containing `#`, including unrelated comments. | `lint.prefer-inferred-private-effects`, `lint_inferred_proc_effects`; `private_proc_effects_lint_does_not_reinsert_inferred_annotations`, `private_proc_effects_removal_is_opt_in_checked_and_convergent`, `private_proc_effects_removal_retains_bounds_docs_and_entry_contracts`; `tests/xsh/private-proc-effects.xsh`. |
| 3. Parametric records: `Observation[T]`, concrete name/count aliases | Parametric records are implemented; these are canonical declarations. **Manual:** there is no automatic abstraction of repeated schemas. Similar syntax is insufficient to prove identical optionality, provenance, or public meaning. | `src/sema/records.rs`; `tests/xsh/parametric-records.xsh`; no schema-merging lint. |
| 4. Str-backed enum: explicit observed/absent/unsupported wire mappings | Wire enums and explicit schema-directed conversion are implemented. **Manual:** no generalized codec-ladder deletion exists. Rejection policy, primitive/domain errors, versions, payloads, and wire strings must match. Ordinary enum conversion is not implied. | `src/sema/wire_enums.rs`; `tests/xsh/wire-enums.xsh`; system-report wire golden/rejection coverage. |
| 5. FsRoot receiver: `fs.close_root/root_read_text(root, ...)` → receiver methods | Root methods are implemented. **Supported:** the exact first-evaluated root calls migrate. Receiver promotion retains remaining source argument order. Reordered named receiver, spreads, comments, non-FsRoot values and forged capabilities require refusal; multi-capability operations remain module functions. | `lint.fs-root-receiver`, `lint_fs_root_receiver`; `fs_root_receiver_fix_preserves_named_argument_text_and_refuses_reordered_receiver`, `fs_root_receiver_refuses_user_record_methods_and_forged_capabilities`, `fs_root_receiver_cli_fix_checks_an_isolated_fixture_and_converges`; `tests/xsh/stdlib/fs_root_methods.xsh`. |
| 6. Typed causes: `Err(BuildError.CompileFailed(...), cause: failure)` | Typed causes are implemented; the example is canonical. **Manual guidance is implemented:** `check.error-cause` recognizes a narrow handler flattening `failure.message`. It adds no automatic cause because diagnostic enhancement changes observable error metadata. | `src/sema/check/call.rs::warn_flattened_error_translation_arena`; `tests/sema.rs::typed_cause_flattened_handler_guidance_has_no_automatic_fix`; `tests/xsh/typed-causes.xsh`. |
| 7. Direct scalar iteration: `name.split("")` → `name`; whole byte-index scan → Bytes | Scalar iteration is implemented. **Supported/subset:** the exact checked Str split loop/comprehension migrates. Bytes migration requires a full immutable source, one initial byte access, and no later use of the index. Adapter variables used elsewhere, offsets, partial ranges, and source mutation remain explicit. | `lint.prefer-scalar-iteration`, `lint_scalar_split_iteration`, `lint_byte_iteration`; `scalar_iteration_fixes_recheck_and_converge_with_comments_and_scopes`, `scalar_iteration_fixes_refuse_used_adapters_offsets_mutation_and_partial_ranges`; `tests/xsh/scalar-iteration.xsh`. |
| 8. Absence/fallback: lookup-origin `!= -1` → `!= null`; `get(key, 0)` → `get(key) ?? 0` | Nullable lookup APIs are implemented. **Supported subset:** exact immutable lookup-origin comparisons and compatible positional fallbacks migrate, including checked immutable parameter and alias reads. A fallback can be eagerly evaluated in the old call; moving effects, failure, mutable reads, or named-order evaluation is refused. List/Map get remains Result, preserving present-null values. | `lint.lookup-absence`, `lint.lookup-fallback`, `proven_absence_lookup`, `lint_removed_lookup_fallback`; `absence_lookup_literal_fallback_fix_rechecks_and_converges`, `absence_lookup_path_literal_fallback_fix_rechecks_and_converges`, `absence_lookup_immutable_fallback_fix_preserves_typed_parameter_and_alias`, `absence_lookup_fallback_fix_refuses_eager_effects_failure_comments_and_named_order`, `absence_lookup_sentinel_*`; `tests/xsh/absence-lookups.xsh`. |
| 9. Callable alias: imported exported `compile` forwarder → `export let compile = compiler.compile` | Signature-preserving aliases are implemented. **Missing exact case:** the migration rule accepts only local Ident callees, explicit wrapper/target return types, and a bare Call. The documented imported target, defaulted return and propagated call are excluded. Supported local forwarders require matching labels/rest/effects/return spelling and equal preparation-time default values. Ordinary runtime defaults are not proved equivalent by this rule. | `lint.prefer-callable-alias`, `crates/xsht/src/lint_callable_alias.rs::lint_callable_aliases`, `same_signature`; `callable_alias_forwarder_fix_preserves_signature_and_converges`, `callable_alias_forwarder_fix_retains_policy_comments_and_argument_order`; `tests/xsh/callable-aliases.xsh`. |
| 10. Accepted exits: `run --accept=[0,1] ...`, captured text variant | Accepted completion policies are implemented; examples are canonical. **Manual by contract:** no custom status-handler migration exists. Branch behavior, true status values, diagnostics and error translation remain application policy. Explicit accept must not normalize actual status or accept signal termination. | `explicit_accept_policy_keeps_propagation_and_custom_status_handlers`; `tests/xsh/run-accept.xsh`, `tests/xsh/run-accept-stream-capture.xsh`. |

## Type inference and removed vocabulary: ergonomics-7

| Proposal and example | Feature and exact migration coverage | Rule, owner, and evidence |
|---|---|---|
| 1. Inferred require: return-boundary `.require(Manifest)` → `.require()` | Expected schema targets are implemented. **Supported:** the exact independently anchored return case can remove only the schema argument when checked target/type context agrees. Validation, conversion and Result handling remain. Unanchored calls retain the argument; comments anywhere in the call currently cause conservative refusal. | `lint.inferred-require-target`, `lint_inferred_require_target`; `inferred_require_target_fix_preserves_validation_and_converges`, `inferred_require_target_fix_rejects_unanchored_and_different_instances`, `inferred_require_formatting_round_trip_and_comments_preserve_the_operation`; `tests/xsh/inferred-require.xsh`. |
| 2. Local holes: `var objects: List[Path] = []` → unannotated accumulator | Local inference is implemented. **Supported subset:** within a function, empty List and mutable-null annotations can be removed after whole-binding type and expression-fact comparison. The illustrative loop contributes static constraints even if it executes zero times. **Missing imported case:** the rule's standalone recheck loses imported contributions. Empty Map annotations are not covered by this deletion rule; `{}` remains a Record without a map context. | `lint.needless-annotation`, `lint_solved_local_annotation`, `local_annotation_removal_preserves_contract`; `local_inference_annotation_fix_preserves_all_checked_expression_types_and_converges`, `local_inference_annotation_fix_requires_identical_material_contract_and_preserves_comments`; `tests/xsh/local-inference.xsh`. |
| 3. Default parameter types: annotations on `defaults.jobs/timeout` → omitted types | Semantic parameter inference is implemented. **Supported subset:** the exact local-const example has redundant builtin annotations that can migrate. **Missing imported case:** independently graph-checked imported defaults cannot pass the rule's standalone baseline probe. User-schema, optional-domain, null/empty, conversion and ambiguous contexts remain annotated. | `lint.default-param-type`, `lint_default_parameter_annotation`, `checked_return_removal_facts`; `default_parameter_annotation_fixes_recheck_preserve_comments_and_converge`, `default_parameter_annotation_keeps_domains_context_and_ambiguous_defaults`; `tests/xsh/default-parameters.xsh`. |
| 4. Constructor inference: annotated `Observation[Int]` literal → unannotated `Observation(...)` | Generic constructor inference is implemented. **Partial exact migration:** `lint.prefer-record-constructor` replaces the RHS but retains the schema annotation, so the exact unannotated after is not reached. A separate concrete-alias→generic-constructor rule proves the same instantiated arguments. Null-only/empty-only, conversion-dependent and conflicting evidence must retain context. | `lint.prefer-record-constructor`, `lint_record_constructor`; `lint.prefer-generic-record-constructor`, `lint_generic_alias_constructor`; `generic_record_constructor_alias_fix_rechecks_and_converges`, `generic_record_constructor_alias_fix_preserves_conversion_and_ambiguous_evidence`; `tests/xsh/generic-constructors.xsh`. |
| 5. Constant-key projection: `config.get("workers")?.require(Int)?` → get with its Result handling | Projection precision is implemented. **Supported:** identity-only require removal reaches the exact typed-field case while retaining receiver/key evaluation and get propagation. Keyed width changes, unsigned constraints, genuinely dynamic receivers, and schema-directed conversions are refused. A selected-value type does not erase opaque receiver access failures. | `lint.redundant-require`, `require_replacement`; `constant_key_projection_identity_require_fix_preserves_boundaries`; `src/sema/projection.rs`; `tests/xsh/constant-key-projections.xsh`. |
| 6. Constant CLI descriptor: `cli.parse(args, option_schema)` retains option field types | Descriptor precision is implemented; the example already uses the canonical form. **Optional cleanup:** no obligatory hoisting lint is requested or implemented. Generic redundant-require removal can apply to an identity-only checked result. This audit did not find a focused constant-descriptor require-removal acceptance test, so full migration coverage is not claimed. | `tests/xsh/stdlib/cli_constants.xsh`, `tests/xsh/stdlib/cli_commands_constants.xsh`; descriptor checker/plan tests; `lint.redundant-require`. |
| 7. Builtin templates: List/Map receiver/argument/result relationships | Signature templates are implemented. These are descriptions of existing operations, not before/after source migrations. **Feature only:** no autofix is required. Consistent checked facts across call spellings remain the correctness criterion. | `src/sema/builtin_templates.rs`; `tests/xsh/builtin-templates.xsh`. |
| 8. Retired record.require: required-only string schema → named schema and `.require(Type)` | The parallel public string-validator surface is removed. **Supported subset:** diagnosed literal/constant required-only contracts can migrate. Missing optional fields, callable contracts, dynamic policies and distinct error identity are not interchangeable with ordinary schemas. | `check.removed-record-require` → `lint.removed-record-require`; `src/sema/check/record_require.rs`; CLI `removed_record_require_cli_fix_rechecks_and_converges_in_stages`, `removed_record_require_cli_fix_preserves_unrelated_errors_and_comments`, `mixed_enum_and_record_require_migration_*`. |
| 9. Checked dynamic boundaries: explicit validation; rejected unchecked annotated JSON | The single checked contract is implemented. The unchecked example intentionally fails rather than migrating by guessed validation. **Manual command maintenance:** `xsht check --strict` has an actionable removed-option error; this audit found no flag-removal migration. `xsht api --strict` is a separate retained command policy. | CLI `check_strict_option_reports_default_dynamic_policy_before_loading`; ordinary checker dynamic-boundary coverage; `tests/xsh/inferred-require.xsh`. |
| 10. Compatibility names: ARGV, run.builtin modes, fs.ls, Str.count_bytes | Removed vocabulary has finite parser/checker migration metadata. **Supported:** fixes target the removed predeclared/standard names and preserve actual run mode/options/argv, comments and source spelling. User names, external argv, character counts and unrelated filesystem APIs are not replacement targets. | `parse.compatibility-vocabulary`, `check.compatibility-vocabulary` → `lint.compatibility-vocabulary`; `crates/xsht/src/edit.rs::migration_lint_code`; `tests/xsh/compatibility-vocabulary.xsh`. |

## Verified gaps and bounded follow-up work

An isolated Rust harness called `Parser`, `parse_load_check_text`, `Checker`, and
`Linter` against the debug library artifacts. It supplied the checked expression,
return, statement-position and requirement facts needed by the relevant rules.
It did not invoke lint/format CLI commands or modify repository source. These
fixtures parsed and checked without errors before their migration was assessed:

| Fixture | Observed migration result | Why it is bounded follow-up work |
|---|---|---|
| Imported defaults: `use helper; pure choose(jobs: Int = helper.defaults.jobs) -> Int { jobs }` | No `lint.default-param-type` fix, despite `helper` exporting a documented const `{jobs: 4}`. | Preserve the checked module graph during equivalence checking; deleting an annotation must not remove the only type/conversion anchor. |
| Imported local constraints: `var items: List[Int] = []; items += [helper.defaults.jobs]` inside an explicitly typed function | No local `lint.needless-annotation` fix. The standalone `[1]` equivalent did produce one. | Recheck the same imported graph and compare the material binding type, expression facts and conversions. |
| Imported `compile` forwarding proc with omitted return annotation and `helper.compile(source, output)?` | No `lint.prefer-callable-alias` fix. | Resolve the qualified callable identity and prove Result[Unit] wrapping/propagation, defaults, effects, captures and initializer timing. The deleted wrapper traceback frame is an intentional diagnostic change. |
| `var revision = ""; cd $repo { revision = run.text git rev-parse HEAD ? }` | No scope-value diagnostic/fix. | A single-assignment run capture can be considered only after proving placeholder observations, restoration/error timing, handlers, cleanup and later writes. |
| `let measured: Observation[Int] = {state: Observed, value: 12}` | Constructor RHS fix only; schema annotation remained. | Removing that annotation additionally requires an independently inferred identical instance, including defaults/conversions and all later consumers. |
| A one-use `read_port` helper matching the local-capture example | No helper-elimination fix. | Restrict any implementation to proved call count/captures and unchanged propagation/lexical exits; do not introduce a general inliner. |
| Single formatted escaped-newline service string | No `lint.prefer-block-string` fix. | Preserve literal pieces, interpolation source order/count, raw escapes, exact line endings and final-newline behavior. |

These omissions do not justify loosening validation, treating imported facts as
ambient guesses, using purity as a timing proof, or replacing an intentional
runtime conversion with an inferred type alone.

### Imported probes and performance

`checked_return_removal_facts` reparses a single source and calls
`Checker::check_arena`. `local_annotation_removal_preserves_contract` does the
same for local annotation candidates. Those probes currently lack the loaded
graph that made the original imported source valid. The cached failed baseline
now prevents repeated candidate probes; that is a performance repair, not an
implementation of imported-source autofix equivalence.

The callable-alias pass similarly checks only eligible local forwarders. Its
original preparation is lazy and cached across candidates. This avoids paying
for complete preparation when no eligible forwarder exists; it does not add
qualified/default-return/runtime-default migration coverage.

Deterministic guards are
`annotation_probe_tests::annotation_probes_stop_after_original_signature_cannot_be_checked`,
`lint_callable_alias::tests::callable_aliases_skip_original_preparation_without_exact_local_forwarding`,
`lint_callable_alias::tests::callable_aliases_cache_original_preparation_across_forwarding_candidates`,
and `effect_fact_tests::checked_empty_effect_facts_avoid_duplicate_frontend_checking`.
`LintOptions::function_effect_facts_checked` distinguishes a valid supplied empty
effect-fact set from a request to perform effect analysis again.

`crates/xsht/tests/lint_performance.rs::repository_lint_is_clean_within_wall_budget`
is the repository gate with a 60-second wall budget excluding compilation and
fixture setup. Its companion tests reject imported diagnostics, honor configured
fixture exclusions, preserve source bytes, and terminate/reap an overdue child.
This audit's focused verification did not run that lint CLI gate; its existence
must not be reported as a measured performance result.

## Repaired Path text/native-byte boundary

The unsafe baseline offered a non-dangerous `lint.redundant-path-display` edit
for this checked program:

```xsh
let raw = Path.parse_bytes(b"raw\xffname")?
run printf "%s" "--target=${raw.display()}" ?
```

Executing the original and proposed replacement both exited successfully, but
their output differed. The original encoded `--target=raw` followed by UTF-8
replacement-character bytes `ef bf bd` and `name`; the replacement sent native
`ff` in that position. Successful parsing and rechecking therefore did not
establish semantic equivalence. Related command f-string and text-to-Path
reductions crossed the same boundary.

The repaired rules retain explicit command `.display()` as manual guidance,
keep command f-string text when a Path's encoding is unknown, and refuse
text-to-native Path interpolation rewrites without byte-equivalence evidence.
`text_path_interpolation_preserves_bytes` admits checked non-Path primitive text
pieces and syntactically UTF-8 Path literals; erased/dynamic values do not supply
that proof. Known native `fp"${path}"` interpolation can still be simplified
without introducing a text conversion. No filesystem observation or runtime
encoding test is introduced by the linter.

The tooling regressions pin refusal and retained safe construction:

- `linter_command_text_path_conversions_preserve_native_byte_boundaries`
- `linter_path_text_roundtrips_preserve_native_byte_boundaries`
- `linter_path_constructor_utf8_text_fix_rechecks_and_converges`
- `linter_retains_explicit_path_display_in_command_args`
- `linter_retains_path_display_parse_roundtrips_without_utf8_proof`
- `linter_autofixes_redundant_type_driven_roundtrips`
- `linter_autofixes_single_value_command_fstrings`

Native
`tests/xsh/stdlib/path.xsh::test_path_text_conversions_remain_distinct_from_native_arguments`
checks actual process bytes for explicit display, formatted text, text-to-Path
casts, compound formatting and direct native Path arguments. It complements
`test_path_interpolation_retains_native_bytes_and_text_boundaries`, which already
establishes that displayed Path text is not a recoverable lossless Path.

## Verification boundary

The new command-boundary tooling regression failed against the original five
unsafe edits before the source repair. After repair, all seven focused tooling
checks listed above passed, including ordinary rechecking and convergence where
the test owns those contracts. The command
`target/debug/xsht test --jobs 1 tests/xsh/stdlib/path.xsh` passed all eight native
Path tests, including the new byte-boundary fixture. An isolated runtime
before/after witness also observed the differing bytes described above.

The remaining matrix entries were established by source/test inspection and the
isolated positive/negative API witnesses identified above. This audit does not
claim a full tooling, native-stdlib, Linux, repository-lint, or performance gate
result. Formatting, lint CLI verification and generated documentation remain
outside these focused checks.
