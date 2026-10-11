#![allow(clippy::single_call_fn)]
#[path = "lint_callable_alias.rs"]
mod lint_callable_alias;

#[path = "lint_cli_entry.rs"]
mod cli_entry;

#[path = "lint_try_capture.rs"]
mod lint_try_capture;

#[path = "lint_context_scope.rs"]
mod context_scope;

#[path = "lint_item_shorthand.rs"]
mod item_shorthand;

#[path = "lint_tempdir_scope.rs"]
mod tempdir_scope;

#[path = "lint_prefer_repeat.rs"]
mod prefer_repeat;

#[path = "lint_prefer_atomically.rs"]
mod prefer_atomically;
#[path = "lint_prefer_collect.rs"]
mod prefer_collect;
#[path = "lint_prefer_tempdir.rs"]
mod prefer_tempdir;
#[path = "lint_prefer_wait_until.rs"]
mod prefer_wait_until;
#[path = "lint_prefer_with_scope.rs"]
mod prefer_with_scope;
#[path = "lint_prefer_within.rs"]
mod prefer_within;
#[path = "lint_redundant_use_alias.rs"]
mod redundant_use_alias;

#[path = "lint_prefer_inferred_proc_return.rs"]
mod inferred_proc_return;
pub use inferred_proc_return::ReturnProofContext;
#[path = "lint_implicit_message.rs"]
mod lint_implicit_message;

#[path = "lint_inferred_variant_pattern.rs"]
mod lint_inferred_variant_pattern;
#[path = "lint_path_text_query.rs"]
mod lint_path_text_query;

#[path = "lint_path_display_equality.rs"]
mod lint_path_display_equality;

#[cfg(test)]
#[path = "lint_needless_annotation_tests.rs"]
mod lint_needless_annotation_tests;

#[path = "lint_path_display_sink.rs"]
mod lint_path_display_sink;

#[path = "lint_write_lines.rs"]
mod lint_write_lines;

#[path = "lint_list_any_union.rs"]
mod lint_list_any_union;
#[path = "lint_read_lines.rs"]
mod lint_read_lines;
#[path = "lint_size_literal.rs"]
mod lint_size_literal;

#[path = "lint_prefer_match_else.rs"]
mod lint_prefer_match_else;

#[path = "lint_optional_binding.rs"]
mod lint_optional_binding;
#[path = "lint_run_argv.rs"]
mod lint_run_argv;

#[path = "lint_redundant_propagation.rs"]
mod lint_redundant_propagation;

#[path = "lint_redundant_scope_propagation.rs"]
mod lint_redundant_scope_propagation;

#[path = "lint_prefer_for_index.rs"]
mod lint_prefer_for_index;
#[path = "lint_prefer_propagation.rs"]
mod lint_prefer_propagation;

#[path = "lint_env_path_list.rs"]
mod lint_env_path_list;

#[path = "lint_prefer_fail.rs"]
mod lint_prefer_fail;
#[path = "lint_prefer_test_expect.rs"]
mod lint_prefer_test_expect;
#[path = "lint_redundant_discard.rs"]
mod lint_redundant_discard;
#[path = "lint_write_mode.rs"]
mod lint_write_mode;

#[path = "lint_path_kind.rs"]
mod lint_path_kind;
#[path = "lint_prefer_as_conversion.rs"]
mod lint_prefer_as_conversion;
#[path = "lint_prefer_is_empty.rs"]
mod lint_prefer_is_empty;
#[path = "lint_prefer_negative_index.rs"]
mod lint_prefer_negative_index;
#[path = "lint_prefer_text_pattern.rs"]
mod lint_prefer_text_pattern;

#[path = "lint_argument_label.rs"]
mod lint_argument_label;
#[path = "lint_empty_sentinel.rs"]
mod lint_empty_sentinel;
#[path = "lint_prefer_non_empty_argv.rs"]
mod lint_prefer_non_empty_argv;
#[path = "lint_prefer_rel_path.rs"]
mod lint_prefer_rel_path;
#[path = "lint_prefer_set.rs"]
mod lint_prefer_set;
#[path = "lint_prefer_typed_callable.rs"]
mod lint_prefer_typed_callable;

#[cfg(test)]
#[path = "lint_fix_grouping_tests.rs"]
mod fix_grouping_tests;
#[cfg(test)]
#[path = "lint_literal_migration_tests.rs"]
mod literal_migration_tests;
#[cfg(test)]
#[path = "lint_redundant_default_remove_tests.rs"]
mod redundant_default_remove_tests;
#[cfg(test)]
#[path = "lint_unused_type_tests.rs"]
mod unused_type_tests;

#[path = "lint_walk.rs"]
mod walk;

#[path = "lint_flow.rs"]
mod flow;
use flow::{FlowSummary, stmt_flow};

#[path = "lint_reachability.rs"]
mod reachability;
use reachability::CallableReachability;

#[path = "lint_probes.rs"]
mod probes;
use probes::{
    CheckedReturnRemovalFacts, CheckedLocalAnnotationFacts, standalone_annotation_imports_available,
    checked_return_removal_facts, checked_return_type_shape,
};

#[path = "lint_fixes.rs"]
mod fixes;
use fixes::{
    span_may_contain_comment, minimize_fix_grouping, widen_over_grouping, scan_effect_list_span,
    scan_before_arrow, shift_after_deletion, scan_before_colon, scan_after_type,
    scan_run_propagate_deletion_span, span_end_after_following_newlines, scan_return_stmt_span,
    scan_back_space, scan_pipe_stage_deletion_span, assert_statement, comma_terminated,
};

#[path = "lint_safety.rs"]
mod safety;
use safety::{
    list_splice_element_type_is_precise, pipeline_argument_expr, list_update_argument_stable,
    mentions_identifier, expr_may_assign_local, expr_child_exprs, expr_child_blocks,
    type_has_unsigned_constraint, type_has_contextual_collection_domain, type_mentions_path,
    expr_is_dynamic_require_boundary, expr_references_name, command_arg_references_name,
    is_safe_const_expr, expr_may_have_effects, migration_inert, migration_reorder_safe,
    migration_arguments_optional, migration_arguments, same_ordering_operand,
    inert_constant_initializer,
};

#[path = "lint_annotations.rs"]
mod annotations;

#[path = "lint_sequence.rs"]
mod sequence;
use sequence::{return_value_is_ok_unit, ok_call_arg};

use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, BTreeSet};
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label, Severity};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::symbols::{Name, Symbol};
use xsh::frontend::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCallArg,
    ArenaCallArgKind, ArenaCommand, ArenaCommandArg, ArenaCommandArgKind, ArenaCompQualifier,
    ArenaEnvAssignment, ArenaEnvAssignmentValue, ArenaExpr, ArenaExprKind, ArenaExprOrRun,
    ArenaFmtPart, ArenaFunctionDef, ArenaMatchExprArm, ArenaModuleContractEntryKind,
    ArenaPatternKind, ArenaPipeStage, ArenaPipeStageKind, ArenaProgram, ArenaRange,
    ArenaRecordField, ArenaRecordFieldKind, ArenaRedirection, ArenaRedirectionTarget,
    ArenaSpawnTarget, ArenaStmtKind, ArenaStreamStage, ArenaSugar, ArenaSugarOperand,
    ArenaTypeDefBody, ArenaTypeExprTag, ArenaWordPart, AssignTargetId, AstArena, BindingTargetId,
    BlockId, BuilderBlockId, CommandStmtId, DeferTrigger, ExprId, FunctionDefId, PatternId,
    RunFormId, StmtId, SugarForm, TypeExprId,
};
use xsh::frontend::syntax::node::{
    AssignOp, BinaryOp, CoreCommand, Effect, RunKind, StreamStageKind, UnaryOp,
    parse_command_word_reference,
};

/// One `push` or `extend` call in a chain that appends to a list, with the
/// parenthesized argument text's span.
struct ListUpdate {
    method: Name,
    argument: ExprId,
    parenthesized: Span,
}

fn insertion_sort_by<T>(items: &mut [T], mut compare: impl FnMut(&T, &T) -> std::cmp::Ordering) {
    for index in 1..items.len() {
        let mut current = index;
        while current > 0 && compare(&items[current], &items[current - 1]).is_lt() {
            items.swap(current, current - 1);
            current -= 1;
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LintOutput {
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug)]
pub struct LintOptions {
    pub prefer_inferred_pure_returns: bool,
    pub prefer_inferred_private_effects: bool,
    pub prefer_env_string: bool,
    pub prefer_item_shorthand: bool,
    pub prefer_tempdir_scope: bool,
    /// The checked constructor calls of this file's `Variant(message: Str)`
    /// variants, which `lint.prefer-implicit-message` reads. `None` when the
    /// caller has no check of the file or the check reported an error: the
    /// facts of such a program do not cover every call, so the rule is silent.
    pub message_payload_constructors:
        Option<BTreeMap<Span, xsh::frontend::check::MessagePayloadConstructor>>,
    /// Opt in to `lint.prefer-inferred-proc-return`.
    pub prefer_inferred_proc_returns: bool,
    /// The project's `check --annotate` writes return annotations, so the
    /// two rules that remove one stay off, whether a setting or `--only`
    /// asks for them: the two tools would undo each other.
    pub annotation_policy_writes_returns: bool,
    /// Opt in to the advice of `lint.prefer-set` for a `Map[K, Bool]` that
    /// is not provably a set; the fix for one that is needs no opt-in.
    pub prefer_set: bool,
    /// Opt in to `lint.prefer-text-pattern`, which has no fix: it notes
    /// where text is taken apart by position.
    pub prefer_text_pattern: bool,
    /// Opt in to `lint.prefer-rel-path`.
    pub prefer_rel_path: bool,
    /// Opt in to `lint.prefer-with-scope`, which has no fix and reports
    /// every handle whose release is deferred where it is opened.
    pub prefer_with_scope: bool,
    /// The file and module roots a return-annotation proof loads imports
    /// with. Without them only a file with no user imports is provable.
    pub return_proof: Option<ReturnProofContext>,
    pub runless: bool,
    pub runless_except: Vec<String>,
    pub interactive_command_replacement: Option<fn(&str) -> Option<&'static str>>,
    pub function_return_types: BTreeMap<Span, Type>,
    pub expr_types: BTreeMap<Span, Type>,
    pub proven_nonnull_fallback_receivers: BTreeSet<Span>,
    pub requirement_targets: BTreeMap<Span, xsh::frontend::check::RequirementTarget>,
    pub requirement_expected_targets: BTreeMap<Span, xsh::frontend::check::RequirementTarget>,
    pub statement_positions: BTreeMap<Span, xsh::frontend::check::StatementPosition>,
    pub callable_effects: FxHashMap<String, Option<Vec<Effect>>>,
    pub function_effect_facts: BTreeMap<
        xsh::frontend::check::EffectDeclarationId,
        xsh::frontend::check::FunctionEffectFact,
    >,
    /// A checked source without callable declarations supplies a valid empty
    /// fact set. Distinguish it from a caller requesting effect analysis.
    pub function_effect_facts_checked: bool,
    pub assertion_effect_spans: BTreeSet<Span>,
    pub statement_expression_spans: BTreeSet<Span>,
    /// Expression statements whose `Result[Unit]` value propagates instead of
    /// becoming a body's value; empty without checked facts.
    pub propagating_statements: BTreeSet<Span>,
    /// The checker's `let _ = VALUE` statements whose binding is redundant.
    pub discardable_bindings: BTreeSet<Span>,
    /// `expr?` in a control position of a condition, where the position
    /// propagates the `Result[Bool]` without the `?`; empty without checked
    /// facts.
    pub redundant_condition_propagations: BTreeSet<Span>,
    /// Spliced `run` targets typed as a list without a validation; empty
    /// without checked facts.
    pub unvalidated_command_vectors: BTreeMap<Span, Type>,
    pub membership_migration_spans: BTreeSet<Span>,
    pub standard_call_spans: BTreeMap<Span, (String, String)>,

    pub statically_resolved_call_spans: BTreeSet<Span>,
    pub definitely_exiting_block_spans: BTreeSet<Span>,
    /// Qualified variant constructors whose expected type selects the same
    /// variant, with the qualifier a leading dot replaces.
    pub redundant_variant_qualifiers: BTreeMap<Span, Span>,
    pub dead_code: bool,
    pub native_test_file: bool,
    /// `xsht lint --only`: retain only diagnostics with these codes, which
    /// also restricts the fixes derived from them.
    pub only: Option<Vec<DiagnosticCode>>,
    /// Rules the project configuration exempts this file from
    /// (`[lint.RULE] exclude`). Their diagnostics are dropped, and with them
    /// the fixes derived from them.
    pub excluded_rules: Vec<DiagnosticCode>,
}

/// One match arm as the adjacent-arm lint sees it: pattern, guard, body, span.
type PatternArm = (PatternId, Option<ExprId>, Result<BlockId, ExprId>, Span);

/// Whether `only` (the `--only` selection, if any) admits a diagnostic code.
pub fn lint_code_selected(only: Option<&[DiagnosticCode]>, code: Option<DiagnosticCode>) -> bool {
    only.is_none_or(|only| code.is_some_and(|code| only.contains(&code)))
}

impl Default for LintOptions {
    fn default() -> Self {
        Self {
            prefer_inferred_pure_returns: false,
            prefer_inferred_private_effects: true,
            prefer_env_string: true,
            prefer_item_shorthand: true,
            prefer_tempdir_scope: true,
            message_payload_constructors: None,
            prefer_inferred_proc_returns: false,
            annotation_policy_writes_returns: false,
            prefer_set: false,
            prefer_text_pattern: false,
            prefer_rel_path: false,
            prefer_with_scope: false,
            return_proof: None,
            runless: false,
            runless_except: Vec::new(),
            interactive_command_replacement: None,
            function_return_types: BTreeMap::default(),
            expr_types: BTreeMap::default(),
            proven_nonnull_fallback_receivers: BTreeSet::default(),
            requirement_targets: BTreeMap::default(),
            requirement_expected_targets: BTreeMap::default(),
            statement_positions: BTreeMap::default(),
            callable_effects: FxHashMap::default(),
            function_effect_facts: BTreeMap::default(),
            function_effect_facts_checked: false,
            assertion_effect_spans: BTreeSet::default(),
            statement_expression_spans: BTreeSet::default(),
            propagating_statements: BTreeSet::default(),
            discardable_bindings: BTreeSet::default(),
            redundant_condition_propagations: BTreeSet::default(),
            unvalidated_command_vectors: BTreeMap::default(),
            membership_migration_spans: BTreeSet::default(),
            standard_call_spans: BTreeMap::default(),
            statically_resolved_call_spans: BTreeSet::default(),
            definitely_exiting_block_spans: BTreeSet::default(),
            redundant_variant_qualifiers: BTreeMap::default(),
            dead_code: true,
            native_test_file: false,
            only: None,
            excluded_rules: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
struct Binding {
    mutable: bool,
    span: Span,
    used: bool,
    report_unused: bool,
    comparison_stable: bool,
    absence_lookup: bool,
    materialized_record: bool,
    immutable_byte_length: Option<usize>,
}

pub struct Linter<'a> {
    arena: &'a AstArena,
    /// The source's parenthesized expressions, and where every expression is
    /// written, built when the first rule asks for an expression's context.
    paren_groups: &'a [(ExprId, Span)],
    expr_positions: std::cell::OnceCell<lint_redundant_propagation::ExprPositions>,
    record_constructors: xsh::frontend::check::RecordConstructors,
    prefer_inferred_pure_returns: bool,
    prefer_inferred_private_effects: bool,
    prefer_env_string: bool,
    prefer_item_shorthand: bool,
    prefer_tempdir_scope: bool,
    prefer_inferred_proc_returns: bool,
    prefer_text_pattern: bool,
    prefer_rel_path: bool,
    prefer_with_scope: bool,
    return_proof: Option<ReturnProofContext>,
    proc_return_candidates: Vec<inferred_proc_return::ProcReturnCandidate>,
    return_removal_before: Option<Option<CheckedReturnRemovalFacts>>,
    local_annotation_before: Option<Option<CheckedLocalAnnotationFacts>>,
    source: &'a str,
    source_cst: std::sync::OnceLock<xsh::frontend::syntax::cst::SyntaxTree>,
    runless: bool,
    runless_except: Vec<String>,
    interactive_command_replacement: Option<fn(&str) -> Option<&'static str>>,
    scopes: Vec<FxHashMap<String, Binding>>,
    diagnostics: Vec<Diagnostic>,
    checked_function_returns: BTreeMap<Span, Type>,
    expr_types: BTreeMap<Span, Type>,
    proven_nonnull_fallback_receivers: BTreeSet<Span>,
    requirement_targets: BTreeMap<Span, xsh::frontend::check::RequirementTarget>,
    requirement_expected_targets: BTreeMap<Span, xsh::frontend::check::RequirementTarget>,
    statement_positions: BTreeMap<Span, xsh::frontend::check::StatementPosition>,
    result_unit_functions: Vec<bool>,
    result_path_functions: Vec<bool>,
    result_return_ok_types: Vec<Option<Type>>,
    function_return_types: Vec<Type>,
    /// The body of each function being linted, beside its entry in
    /// `function_return_types`.
    function_bodies: Vec<BlockId>,
    checked_effects: BTreeMap<
        xsh::frontend::check::EffectDeclarationId,
        xsh::frontend::check::FunctionEffectFact,
    >,
    assertion_effect_spans: BTreeSet<Span>,
    statement_expression_spans: BTreeSet<Span>,
    membership_migration_spans: BTreeSet<Span>,
    standard_call_spans: BTreeMap<Span, (String, String)>,
    statement_call_spans: BTreeMap<Span, Span>,
    whole_statement_call_spans: BTreeMap<Span, Span>,
    whole_statement_values: std::sync::OnceLock<FxHashSet<ExprId>>,
    guarded_statement_depth: usize,
    propagating_statements: BTreeSet<Span>,
    discardable_bindings: BTreeSet<Span>,
    redundant_condition_propagations: BTreeSet<Span>,
    /// Empty without checked facts.
    unvalidated_command_vectors: BTreeMap<Span, Type>,
    /// Inside a proc or pure body: whether `?` may replace `return Err(e)`
    /// there without changing the function's effect contract.
    propagation_function: Option<bool>,
    /// How many forms lie between the enclosing function body and the
    /// statement being visited through which `return` and `?` may leave
    /// differently: any expression, and a `with` statement.
    propagation_boundary_depth: usize,
    negated_call_spans: BTreeMap<Span, Span>,
    size_products: lint_size_literal::SizeProducts,
    statically_resolved_call_spans: BTreeSet<Span>,
    definitely_exiting_block_spans: BTreeSet<Span>,
    redundant_variant_qualifiers: BTreeMap<Span, Span>,
    /// Where the pattern of each arm of a real `match` starts. A pattern test
    /// or condition also stores its pattern as an arm, and is not one.
    match_arm_head_starts: FxHashSet<usize>,
    inferred_variant_patterns_reported: FxHashSet<Span>,
    dead_code: bool,
    tag_variants: FxHashSet<String>,
    type_declarations: FxHashMap<String, Span>,
    user_type_names: FxHashSet<String>,
    record_type_names: FxHashSet<String>,
    used_type_names: FxHashSet<String>,
    assigned_names: FxHashSet<Name>,
    regex_recovery_context: bool,
    assertion_capture_depth: usize,
    duration_conversion_module_unshadowed: bool,
    list_any_bindings: lint_list_any_union::ListAnyBindings,
    set_like_bindings: lint_prefer_set::SetLikeBindings,
    empty_fallbacks: lint_empty_sentinel::EmptyFallbacks,
    /// `lint.prefer-item-shorthand` and `lint.prefer-match-else` reports,
    /// gathered while callbacks and matches are visited and reported after
    /// the walk in source order.
    item_shorthands: Vec<Diagnostic>,
    catch_all_arms: Vec<Diagnostic>,
    fail_candidates: lint_prefer_fail::Candidates,
    callable_parameters: lint_prefer_typed_callable::CallableParameters,
}

/// A decoded type expression node, mirroring the arena's compact type-expr
/// encoding without referencing the old recursive AST.
enum ArenaTypeExprKind {
    Named(Name),
    Qualified,
    List(TypeExprId),
    Map(Option<TypeExprId>, TypeExprId),
    Stream(TypeExprId),
    Module(TypeExprId),
    Result {
        ok: TypeExprId,
        err: Option<TypeExprId>,
    },
    Optional(TypeExprId),
}

fn type_expr_kind(arena: &AstArena, id: TypeExprId) -> ArenaTypeExprKind {
    let index = id.index();
    let tag = arena.type_expr_tags[index];
    let data = arena.type_expr_data[index];
    match tag {
        ArenaTypeExprTag::Named => {
            ArenaTypeExprKind::Named(Name::from_symbol(Symbol::from_raw(data.lhs)))
        }
        // The rules that read type expressions treat a union, a callable
        // type, and a validated type as they treat any type they cannot see
        // into: a `NonEmpty[T]` is not interchangeable with the `List[T]`
        // those rules reason about.
        ArenaTypeExprTag::Applied
        | ArenaTypeExprTag::Qualified
        | ArenaTypeExprTag::Union
        | ArenaTypeExprTag::Callable
        | ArenaTypeExprTag::NonEmpty
        | ArenaTypeExprTag::Set => ArenaTypeExprKind::Qualified,
        ArenaTypeExprTag::List => {
            ArenaTypeExprKind::List(TypeExprId::from_index(data.lhs as usize))
        }
        ArenaTypeExprTag::Map => ArenaTypeExprKind::Map(
            TypeExprId::from_optional_raw(data.rhs),
            TypeExprId::from_index(data.lhs as usize),
        ),
        ArenaTypeExprTag::Stream => {
            ArenaTypeExprKind::Stream(TypeExprId::from_index(data.lhs as usize))
        }
        ArenaTypeExprTag::Module => {
            ArenaTypeExprKind::Module(TypeExprId::from_index(data.lhs as usize))
        }
        ArenaTypeExprTag::Result => ArenaTypeExprKind::Result {
            ok: TypeExprId::from_index(data.lhs as usize),
            err: TypeExprId::from_optional_raw(data.rhs),
        },
        ArenaTypeExprTag::Optional => {
            ArenaTypeExprKind::Optional(TypeExprId::from_index(data.lhs as usize))
        }
    }
}

impl<'a> Linter<'a> {
    pub fn lint(program: &'a ArenaProgram, source: &'a str, options: LintOptions) -> LintOutput {
        Self::lint_internal(program, source, options, true)
    }

    /// Lint one source range that belongs to a checked workspace bundle.
    ///
    /// Callable reachability is evaluated once for the bundle entry, so an
    /// imported module must not independently report the same reachability
    /// diagnostics when the workspace walks its reachable files.
    pub fn lint_module(
        program: &'a ArenaProgram,
        source: &'a str,
        options: LintOptions,
    ) -> LintOutput {
        Self::lint_internal(program, source, options, false)
    }

    fn lint_internal(
        program: &'a ArenaProgram,
        source: &'a str,
        options: LintOptions,
        include_reachability: bool,
    ) -> LintOutput {
        Self::lint_internal_with_effect_check(
            program,
            source,
            options,
            include_reachability,
            || xsh::frontend::check::Checker::check_arena(program, source),
        )
    }

    fn lint_internal_with_effect_check(
        program: &'a ArenaProgram,
        source: &'a str,
        options: LintOptions,
        include_reachability: bool,
        check_effects: impl FnOnce() -> xsh::frontend::check::CheckOutput,
    ) -> LintOutput {
        // Constructor facts intern qualified enum names after checking. Retain
        // those names in the source program even when the caller is a worker.
        let _symbols = program.symbol_owner().enter();
        let native_test_file = options.native_test_file;
        let only = options.only;
        // Naming a rule in `--only` asks for it as its setting does, whether
        // the setting is an opt-in that was left out or a default that was
        // turned off.
        let named = |code: DiagnosticCode| only.as_deref().is_some_and(|only| only.contains(&code));
        let prefer_set_advice = options.prefer_set || named(DiagnosticCode::LintPreferSet);
        // A project whose `check --annotate` writes return annotations keeps
        // them, however their removal was asked for.
        let removes_returns = !options.annotation_policy_writes_returns;
        let excluded_rules = options.excluded_rules;
        let message_payload_constructors = options.message_payload_constructors;
        let checked_effects =
            if options.function_effect_facts.is_empty() && !options.function_effect_facts_checked {
                check_effects().function_effect_facts
            } else {
                options.function_effect_facts
            };
        let mut linter = Self {
            record_constructors: xsh::frontend::check::RecordConstructors::collect(program),
            arena: &program.arena,
            paren_groups: &program.paren_groups,
            expr_positions: std::cell::OnceCell::new(),
            duration_conversion_module_unshadowed: !(0..program.arena.stmt_tags.len()).any(
                |index| {
                    let ArenaStmtKind::Use(id) = program.arena.stmt(StmtId::from_index(index)).kind
                    else {
                        return false;
                    };
                    let import = program.arena.use_stmt(id);
                    import
                        .alias
                        .or_else(|| program.arena.names(import.path).last())
                        .is_some_and(|name| name == "time")
                },
            ),
            prefer_inferred_pure_returns: removes_returns
                && (options.prefer_inferred_pure_returns
                    || named(DiagnosticCode::LintPreferInferredPureReturn)),
            prefer_inferred_private_effects: options.prefer_inferred_private_effects
                || named(DiagnosticCode::LintPreferInferredPrivateEffects),
            prefer_env_string: options.prefer_env_string
                || named(DiagnosticCode::LintPreferEnvString),
            prefer_item_shorthand: options.prefer_item_shorthand
                || named(DiagnosticCode::LintPreferItemShorthand),
            prefer_tempdir_scope: options.prefer_tempdir_scope
                || named(DiagnosticCode::LintPreferTempdirScope),
            prefer_text_pattern: options.prefer_text_pattern
                || named(DiagnosticCode::LintPreferTextPattern),
            prefer_rel_path: options.prefer_rel_path || named(DiagnosticCode::LintPreferRelPath),
            prefer_with_scope: options.prefer_with_scope
                || named(DiagnosticCode::LintPreferWithScope),
            prefer_inferred_proc_returns: removes_returns
                && (options.prefer_inferred_proc_returns
                    || named(DiagnosticCode::LintPreferInferredProcReturn)),
            return_proof: options.return_proof,
            proc_return_candidates: Vec::new(),
            return_removal_before: None,
            local_annotation_before: None,
            source,
            source_cst: std::sync::OnceLock::new(),
            runless: options.runless,
            runless_except: options.runless_except,
            interactive_command_replacement: options.interactive_command_replacement,
            scopes: vec![FxHashMap::default()],
            diagnostics: Vec::new(),
            checked_function_returns: options.function_return_types,
            expr_types: options.expr_types,
            proven_nonnull_fallback_receivers: options.proven_nonnull_fallback_receivers,
            requirement_targets: options.requirement_targets,
            requirement_expected_targets: options.requirement_expected_targets,
            statement_positions: options.statement_positions,
            result_unit_functions: Vec::new(),
            result_path_functions: Vec::new(),
            result_return_ok_types: Vec::new(),
            function_return_types: Vec::new(),
            function_bodies: Vec::new(),
            checked_effects,
            assertion_effect_spans: options.assertion_effect_spans,
            statement_expression_spans: options.statement_expression_spans,
            membership_migration_spans: options.membership_migration_spans,
            standard_call_spans: options.standard_call_spans,
            statement_call_spans: BTreeMap::new(),
            whole_statement_call_spans: BTreeMap::new(),
            whole_statement_values: std::sync::OnceLock::new(),
            guarded_statement_depth: 0,
            propagating_statements: options.propagating_statements,
            discardable_bindings: options.discardable_bindings,
            redundant_condition_propagations: options.redundant_condition_propagations,
            unvalidated_command_vectors: options.unvalidated_command_vectors,
            propagation_function: None,
            propagation_boundary_depth: 0,
            negated_call_spans: BTreeMap::new(),
            size_products: lint_size_literal::SizeProducts::default(),
            statically_resolved_call_spans: options.statically_resolved_call_spans,
            definitely_exiting_block_spans: options.definitely_exiting_block_spans,
            redundant_variant_qualifiers: options.redundant_variant_qualifiers,
            match_arm_head_starts: FxHashSet::default(),
            inferred_variant_patterns_reported: FxHashSet::default(),
            dead_code: options.dead_code,
            tag_variants: FxHashSet::default(),
            type_declarations: FxHashMap::default(),
            user_type_names: FxHashSet::default(),
            record_type_names: FxHashSet::default(),
            used_type_names: FxHashSet::default(),
            assigned_names: FxHashSet::default(),
            regex_recovery_context: false,
            assertion_capture_depth: 0,
            list_any_bindings: lint_list_any_union::ListAnyBindings::default(),
            set_like_bindings: lint_prefer_set::SetLikeBindings::default(),
            empty_fallbacks: lint_empty_sentinel::EmptyFallbacks::default(),
            item_shorthands: Vec::new(),
            catch_all_arms: Vec::new(),
            fail_candidates: lint_prefer_fail::Candidates::collect(program, source),
            callable_parameters: lint_prefer_typed_callable::CallableParameters::default(),
        };
        linter.define(
            "args",
            Span::new(xsh::frontend::source::SourceId::new(0), 0, 0),
            false,
        );
        let statements: Vec<StmtId> = program.statement_ids().collect();
        if native_test_file {
            linter.lint_legacy_test_declarations(&statements);
        }
        linter.lint_program(&statements);
        linter.lint_defer_block_helpers(&statements);
        let mut item_shorthands = std::mem::take(&mut linter.item_shorthands);
        item_shorthands.sort_by_key(|diagnostic| diagnostic.labels[0].span.start());
        linter.diagnostics.extend(item_shorthands);
        if linter.prefer_tempdir_scope {
            tempdir_scope::lint_tempdir_scopes(&mut linter, program);
        }
        if include_reachability {
            linter
                .diagnostics
                .extend(cli_entry::signature_cli_migration(program, source));
        }
        if include_reachability {
            linter
                .diagnostics
                .extend(lint_try_capture::lint_try_capture_helpers(program, source));
        }
        if include_reachability {
            linter.lint_declaration_reachability(program);
            linter
                .diagnostics
                .extend(lint_callable_alias::lint_callable_aliases(program, source));
        }
        linter
            .diagnostics
            .extend(redundant_use_alias::lint_redundant_use_aliases(
                program, source,
            ));
        // The proof checks the file again, so it runs only when its result
        // can be reported.
        if lint_code_selected(
            only.as_deref(),
            Some(DiagnosticCode::LintPreferInferredProcReturn),
        ) {
            linter.lint_inferred_proc_returns();
        }
        let mut catch_all_arms = std::mem::take(&mut linter.catch_all_arms);
        catch_all_arms.sort_by_key(|diagnostic| diagnostic.labels[0].span.start());
        linter.diagnostics.extend(catch_all_arms);
        let list_any_bindings = std::mem::take(&mut linter.list_any_bindings);
        linter.diagnostics.extend(list_any_bindings.finish());
        let set_like_bindings = std::mem::take(&mut linter.set_like_bindings);
        linter
            .diagnostics
            .extend(set_like_bindings.finish(source, prefer_set_advice));
        let fail_candidates = std::mem::take(&mut linter.fail_candidates);
        linter
            .diagnostics
            .extend(fail_candidates.finish(&program.arena, source));
        // After `lint.prefer-fail`, whose families this rule leaves alone.
        if let Some(constructors) = &message_payload_constructors {
            let implicit_messages = lint_implicit_message::lint_implicit_messages(
                program,
                source,
                constructors,
                &linter.diagnostics,
            );
            linter.diagnostics.extend(implicit_messages);
        }
        {
            let callable_parameters = std::mem::take(&mut linter.callable_parameters);
            let reports = callable_parameters.finish(
                linter.arena,
                source,
                &linter.checked_function_returns,
                &|body| linter.checked_effect_fact(body).cloned(),
            );
            linter.diagnostics.extend(reports);
        }
        linter.diagnostics.retain(|diagnostic| {
            lint_code_selected(only.as_deref(), diagnostic.code)
                && !diagnostic
                    .code
                    .is_some_and(|code| excluded_rules.contains(&code))
        });
        minimize_fix_grouping(program, source, &mut linter.diagnostics);
        LintOutput {
            diagnostics: linter.diagnostics,
        }
    }

    fn lint_legacy_test_declarations(&mut self, statements: &[StmtId]) {
        let tokens = xsh::frontend::syntax::lexer::Lexer::new(
            xsh::frontend::source::SourceId::new(0),
            self.source,
        )
        .lex_compact();
        for &statement in statements {
            let stmt = self.arena.stmt(statement);
            let ArenaStmtKind::ProcDef(id) = stmt.kind else {
                continue;
            };
            let def = self.arena.function_def(id);
            if def.test_declaration || !def.name.as_str().starts_with("test_") {
                continue;
            }
            let body_span = self.arena.span(self.arena.block(def.body).span);
            let Some(return_ty) = self.checked_function_returns.get(&body_span) else {
                continue;
            };
            if return_ty != &Type::Result(Box::new(Type::Unit), Box::new(Type::Error)) {
                continue;
            }
            let params = self.arena.params(def.params);
            if params.len() > 1
                || params.iter().any(|param| {
                    param.rest
                        || param.default.is_some()
                        || !self.arena.type_expr_named(param.ty, "TestContext")
                })
            {
                continue;
            }
            let signature = Span::new(stmt.span.source_id, stmt.span.start(), body_span.start());
            let mut diagnostic = Diagnostic::warning(
                "legacy native test proc requires an explicit test declaration",
            )
            .with_code(DiagnosticCode::LintLegacyTestProc)
            .with_label(Label::primary(
                signature,
                "preserve the exact name to retain the test ID",
            ));
            let references = (0..tokens.token_table.len())
                .filter(|&index| {
                    tokens
                        .token_table
                        .name_at(index)
                        .is_some_and(|name| name.as_str() == def.name.as_str())
                })
                .count();
            let raw = self.source.get(signature.range()).unwrap_or_default();
            if references == 1 && !raw.contains('#') {
                let effects = def
                    .effects
                    .map(|effects| {
                        format!(
                            " [{}]",
                            self.arena
                                .effects(effects)
                                .map(|effect| effect.as_str().to_owned())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })
                    .unwrap_or_default();
                diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                    signature,
                    "register the checked harness entrypoint",
                    format!("test {}{} ", def.name, effects),
                ));
                if let Some(param) = params.first() {
                    let insertion = Span::new(
                        stmt.span.source_id,
                        body_span.start() + 1,
                        body_span.start() + 1,
                    );
                    diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                        insertion,
                        "bind the immutable TestContext header",
                        format!(" |{}|", param.name),
                    ));
                }
            } else {
                diagnostic = diagnostic.with_label(Label::secondary(stmt.span,
                    "extract callable shared work into an ordinary helper; signature comments require manual migration"));
            }
            self.diagnostics.push(diagnostic);
        }
    }

    fn lint_defer_block_helpers(&mut self, statements: &[StmtId]) {
        for &statement in statements {
            let definition_span = self.arena.stmt(statement).span;
            let ArenaStmtKind::ProcDef(definition) = self.arena.stmt(statement).kind else {
                continue;
            };
            let definition = self.arena.function_def(definition);
            if !definition.params.is_empty()
                || !definition.effects.is_some_and(|effects| effects.is_empty())
                || !matches!(type_expr_kind(self.arena, definition.return_ty), ArenaTypeExprKind::Named(name) if name == "Unit")
            {
                continue;
            }
            let body = self.arena.block(definition.body);
            if body.statements.is_empty() {
                continue;
            }
            // Literal output has no captures, fallible operations, or control transfers to reattribute.
            if !self.arena.stmt_ids(body.statements).all(|id| {
                let ArenaStmtKind::Command(command) = self.arena.stmt(id).kind else {
                    return false;
                };
                let ArenaCommand::Core {
                    name,
                    args,
                    env,
                    block,
                } = self.arena.command_stmt(command).command
                else {
                    return false;
                };
                matches!(name, CoreCommand::Print | CoreCommand::Eprint)
                    && env.is_empty()
                    && block.is_none()
                    && self
                        .arena
                        .command_args(args)
                        .iter()
                        .all(|arg| match arg.kind {
                            ArenaCommandArgKind::Word(parts) => {
                                self.arena.word_parts(parts).all(|part| {
                                    matches!(
                                        part,
                                        ArenaWordPart::Bare(_) | ArenaWordPart::Quoted(_)
                                    )
                                })
                            }
                            _ => false,
                        })
            }) {
                continue;
            }
            let references = (0..self.arena.expr_tags.len())
                .filter(|&index| {
                    matches!(
                        self.arena.expr(ExprId::from_index(index)).kind,
                        ArenaExprKind::Ident(name) if name == definition.name
                    )
                })
                .count();
            if references != 1 || (0..self.arena.stmt_tags.len()).any(|index| {
                let ArenaStmtKind::Command(command) = self.arena.stmt(StmtId::from_index(index)).kind else { return false };
                matches!(self.arena.command_stmt(command).command, ArenaCommand::Proc { name, .. } if name == definition.name)
            }) { continue; }
            let deferred = statements.iter().find_map(|&id| {
                let stmt = self.arena.stmt(id);
                let ArenaStmtKind::Defer(ArenaExprOrRun::Expr(call), DeferTrigger::Exit) = stmt.kind else { return None };
                let ArenaExprKind::Call { callee, args } = self.arena.expr(call).kind else { return None };
                (args.is_empty() && matches!(self.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == definition.name)
                    && self.expr_types.get(&self.arena.expr(call).span) == Some(&Type::Unit)
                    && stmt.span.source_id == definition_span.source_id).then_some(stmt.span)
            });
            let Some(deferred) = deferred else { continue };
            let Some(definition_text) = self.source.get(definition_span.range()) else {
                continue;
            };
            let Some(body_text) = self.source.get(self.arena.span(body.span).range()) else {
                continue;
            };
            let preceding_comment = self
                .source
                .get(..definition_span.start())
                .and_then(|text| text.trim_end().lines().last())
                .is_some_and(|line| line.trim_start().starts_with('#'));
            if definition_text.contains('#') || preceding_comment {
                continue;
            }
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    "use a deferred block for this single-use literal cleanup helper",
                )
                .with_code(DiagnosticCode::LintPreferDeferBlock)
                .with_label(Label::secondary(
                    deferred,
                    "register the cleanup body directly",
                ))
                .with_fix_hint(FixHint::replacement(
                    deferred,
                    "register a deferred block",
                    format!("defer {body_text}\n"),
                ))
                .with_fix_hint(FixHint::replacement(
                    definition_span,
                    "remove the unused private helper",
                    String::new(),
                )),
            );
        }
    }

    fn lint_program(&mut self, statements: &[StmtId]) {
        self.collect_assigned_names(statements);
        self.lint_import_blocks(statements);
        self.lint_top_level_const_order(statements);
        self.lint_prepared_constants(statements);
        for &stmt_id in statements {
            let stmt = self.arena.stmt(stmt_id);
            let (inner_id, inner, is_exported) = match &stmt.kind {
                ArenaStmtKind::Export(inner) => (*inner, self.arena.stmt(*inner).kind, true),
                _ => (stmt_id, stmt.kind.clone(), false),
            };
            let _ = inner_id;
            match &inner {
                ArenaStmtKind::TypeDef(def_id) => {
                    let def = self.arena.type_def(*def_id).clone();
                    self.define(def.name.as_str().as_str(), stmt.span, false);
                    self.user_type_names.insert(def.name.to_string());
                    if !def.name.as_str().starts_with('_') && !is_exported {
                        self.type_declarations
                            .insert(def.name.to_string(), stmt.span);
                    }
                    if let ArenaTypeDefBody::RecordSchema(_) = &def.body {
                        self.record_type_names.insert(def.name.to_string());
                    }
                    if let ArenaTypeDefBody::TagUnion(variants) = &def.body {
                        for variant in self.arena.tag_variants(*variants) {
                            if variant.fields.is_empty() {
                                self.tag_variants.insert(variant.name.to_string());
                            }
                        }
                    }
                    self.collect_type_def_refs(&def.body);
                }
                ArenaStmtKind::ProcDef(def_id)
                | ArenaStmtKind::StreamDef(def_id)
                | ArenaStmtKind::PureDef(def_id) => {
                    let name = self.arena.function_def(*def_id).name;
                    self.define(name.as_str().as_str(), stmt.span, false);
                }
                _ => {}
            }
        }
        self.lint_list_comp_suggestions(statements);
        self.lint_stream_producer_suggestions(statements);
        prefer_tempdir::lint_scratch_directories(self, statements, None);
        prefer_atomically::lint_published_files(self, statements, None);
        prefer_within::lint_repeated_timeouts(self, statements);
        prefer_collect::lint_built_lists(self, statements);
        prefer_wait_until::lint_polling_loops(self, statements);
        if self.prefer_with_scope {
            prefer_with_scope::lint_deferred_releases(self, statements);
        }
        self.lint_statement_sequence(statements);
        self.lint_implicit_main(statements);
        self.lint_unused_types();
    }

    fn lint_declaration_reachability(&mut self, program: &'a ArenaProgram) {
        if !self.dead_code {
            return;
        }
        self.diagnostics
            .extend(CallableReachability::new(program).diagnostics());
    }

    fn lint_import_blocks(&mut self, statements: &[StmtId]) {
        let mut index = 0;
        while index < statements.len() {
            if !matches!(
                self.arena.stmt(statements[index]).kind,
                ArenaStmtKind::Use(_)
            ) {
                index += 1;
                continue;
            }
            let start = index;
            while index < statements.len()
                && matches!(
                    self.arena.stmt(statements[index]).kind,
                    ArenaStmtKind::Use(_)
                )
            {
                index += 1;
            }
            self.lint_import_block(&statements[start..index]);
        }
    }

    fn lint_import_block(&mut self, block: &[StmtId]) {
        if block.len() < 2 {
            return;
        }
        let original = block
            .iter()
            .map(|&id| import_sort_key(self.arena, id))
            .collect::<Vec<_>>();
        let mut sorted = original.clone();
        insertion_sort_by(&mut sorted, |left, right| left.cmp(right));
        if sorted == original {
            return;
        }

        let first_span = self.arena.stmt(block[0]).span;
        let last_span = self.arena.stmt(block[block.len() - 1]).span;
        let start = first_span.start();
        let end = last_span.end();
        let span = Span::new(first_span.source_id, start, end);
        let has_comment = self.source[start..end].contains('#');
        let mut diagnostic = Diagnostic::new(Severity::Warning, "import block is not sorted")
            .with_code(DiagnosticCode::LintUnsortedImports)
            .with_label(Label::secondary(
                span,
                "sort this contiguous import block by module path and alias",
            ));
        if has_comment {
            diagnostic =
                diagnostic.with_note("comments in the import block make this a manual fix for now");
        } else {
            let replacement = sorted
                .iter()
                .map(|key| import_text(&key.path, key.alias.as_deref()))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "sort import block",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_prepared_constants(&mut self, statements: &[StmtId]) {
        for &statement in statements {
            let (statement, exported) = match self.arena.stmt(statement).kind {
                ArenaStmtKind::Export(inner) => (inner, true),
                _ => (statement, false),
            };
            let stmt = self.arena.stmt(statement);
            let ArenaStmtKind::Let {
                target,
                initializer: ArenaExprOrRun::Expr(value),
                ..
            } = stmt.kind
            else {
                continue;
            };
            if !matches!(self.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name.as_str() != "_")
                || !inert_constant_initializer(self.arena, value)
                || self.holds_missing_f_prefix(value)
            {
                continue;
            }
            let Some(expected) = self.expr_types.get(&self.arena.expr(value).span) else {
                continue;
            };
            let Some(constant) = xsh::frontend::check::LiteralConstant::analyze(
                self.arena,
                value,
                &FxHashMap::default(),
            ) else {
                continue;
            };
            if !constant.in_type(expected).matches_data_type(expected) {
                continue;
            }
            let start = if exported {
                // Exported bindings include the export prefix in their span.
                // Select the keyword token so spacing and comments stay intact.
                let text = &self.source[stmt.span.range()];
                let lexed = xsh::frontend::syntax::lexer::Lexer::new(stmt.span.source_id, text)
                    .lex_compact();
                let Some(index) = (0..lexed.token_table.len()).find(|&index| {
                    lexed.token_table.keyword_at(index)
                        == Some(xsh::frontend::syntax::token::Keyword::Let)
                }) else {
                    continue;
                };
                let Some(keyword) = lexed.token_table.span_at(index, stmt.span.source_id, text)
                else {
                    continue;
                };
                stmt.span.start() + keyword.start()
            } else {
                stmt.span.start()
            };
            let span = Span::new(stmt.span.source_id, start, start + 3);
            if self.source.get(span.range()) != Some("let") {
                continue;
            }
            self.diagnostics.push(
                Diagnostic::new(Severity::Warning, "module data can be declared as const")
                    .with_code(DiagnosticCode::LintPreferConst)
                    .with_label(Label::secondary(
                        span,
                        "this initializer is inert and can be prepared",
                    ))
                    .with_fix_hint(FixHint::replacement(
                        span,
                        "declare prepared immutable data",
                        "const",
                    )),
            );
        }
    }

    fn lint_top_level_const_order(&mut self, statements: &[StmtId]) {
        let mut saw_function = false;
        for &stmt_id in statements {
            let phase = top_level_phase(self.arena, stmt_id, self.source);
            if phase == TopLevelPhase::SafeConst && saw_function {
                self.diagnostics.push(
                    Diagnostic::new(
                        Severity::Warning,
                        "top-level constant should be grouped after imports",
                    )
                    .with_code(DiagnosticCode::LintOrganizeTopLevelConsts)
                    .with_label(Label::secondary(
                        self.arena.stmt(stmt_id).span,
                        "move this safe immutable binding before top-level functions",
                    )),
                );
            }
            if phase == TopLevelPhase::Function {
                saw_function = true;
            }
        }
    }

    fn lint_unused_types(&mut self) {
        let mut unused: Vec<_> = self
            .type_declarations
            .iter()
            .filter(|(name, _)| !self.used_type_names.contains(*name))
            .map(|(name, &span)| (name.clone(), span))
            .collect();
        insertion_sort_by(&mut unused, |(_, left), (_, right)| {
            left.start().cmp(&right.start())
        });
        for (name, span) in unused {
            let deletion_span = scan_return_stmt_span(self.source, span);
            self.diagnostics.push(
                xsh::diagnostic::Diagnostic::new(
                    xsh::diagnostic::Severity::Warning,
                    format!("unused type declaration `{name}`"),
                )
                .with_code(DiagnosticCode::LintUnusedType)
                .with_label(xsh::diagnostic::Label::secondary(
                    span,
                    "type is declared but never referenced",
                ))
                .with_fix_hint(
                    FixHint::deletion(
                        deletion_span,
                        "remove unused type declaration (apply manually)",
                    )
                    .dangerous(),
                ),
            );
        }
    }

    fn lint_implicit_main(&mut self, statements: &[StmtId]) {
        let has_main_proc = statements.iter().any(|&stmt_id| {
            let stmt = self.arena.stmt(stmt_id);
            let inner = match &stmt.kind {
                ArenaStmtKind::Export(inner) => self.arena.stmt(*inner).kind,
                _ => stmt.kind,
            };
            matches!(&inner, ArenaStmtKind::ProcDef(def_id) if self.arena.function_def(*def_id).name == "main")
        });
        if !has_main_proc {
            return;
        }
        let Some(&last_id) = statements.last() else {
            return;
        };
        let last_stmt = self.arena.stmt(last_id);
        // Check if the last statement is `main(@args)` — parses as an expr statement wrapping a call.
        let is_explicit_call = match &last_stmt.kind {
            ArenaStmtKind::Expr(expr_id) => match self.arena.expr(*expr_id).kind {
                ArenaExprKind::Call { callee, args } => {
                    matches!(self.arena.expr(callee).kind, ArenaExprKind::Ident(n) if n == "main")
                        && args.len() == 1
                        && matches!(
                            &self.arena.call_args(args)[0].kind,
                            ArenaCallArgKind::Splice { value, .. } if matches!(self.arena.expr(*value).kind, ArenaExprKind::Ident(n) if n == "args")
                        )
                }
                _ => false,
            },
            _ => false,
        };
        if !is_explicit_call {
            return;
        }
        let deletion_span = scan_return_stmt_span(self.source, last_stmt.span);
        self.diagnostics.push(
            xsh::diagnostic::Diagnostic::new(
                xsh::diagnostic::Severity::Warning,
                "redundant `main(@args)` — main is invoked implicitly",
            )
            .with_code(DiagnosticCode::LintRedundantMainCall)
            .with_label(xsh::diagnostic::Label::secondary(
                last_stmt.span,
                "main is called automatically after all top-level statements run",
            ))
            .with_fix_hint(FixHint::deletion(
                deletion_span,
                "remove explicit invocation — main is called implicitly",
            )),
        );
    }

    // Propagation spellings, each matched in its own file against the one
    // statement being visited.
    fn lint_propagation(&mut self, statement: StmtId) {
        let facts = lint_redundant_propagation::PropagationFacts {
            expr_types: &self.expr_types,
            statement_positions: &self.statement_positions,
            propagating_statements: &self.propagating_statements,
        };
        let found = [
            lint_redundant_propagation::redundant_propagation(
                self.arena,
                self.source,
                &facts,
                statement,
            ),
            lint_redundant_scope_propagation::redundant_scope_propagation(
                self.arena,
                self.source,
                &facts,
                statement,
            ),
            lint_redundant_propagation::redundant_run_propagation(
                self.arena,
                self.source,
                &facts,
                statement,
            ),
            lint_redundant_propagation::redundant_defer_propagation(
                self.arena,
                self.source,
                &facts,
                statement,
            ),
            self.propagation_function
                .filter(|_| self.propagation_boundary_depth == 0)
                .and_then(|allowed| {
                    lint_prefer_propagation::repropagating_match(
                        self.arena,
                        self.source,
                        &facts,
                        statement,
                        allowed,
                    )
                }),
        ];
        self.diagnostics.extend(found.into_iter().flatten());
    }

    fn lint_core_assert(&mut self, stmt_span: Span, expression: ExprId) {
        if self.assertion_capture_depth != 0 {
            return;
        }
        if self.statement_positions.get(&stmt_span)
            != Some(&xsh::frontend::check::StatementPosition::Statement)
        {
            return;
        }
        let call = match self.arena.expr(expression).kind {
            ArenaExprKind::Try(call) => call,
            ArenaExprKind::Call { .. } => expression,
            _ => return,
        };
        let ArenaExprKind::Call { callee, args } = self.arena.expr(call).kind else {
            return;
        };
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if !matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(module) if module.as_str().as_str() == "test")
        {
            return;
        }
        let operation = name.as_str();
        let arity = match operation.as_str() {
            "ok" => 2,
            "eq" | "ne" => 3,
            _ => return,
        };
        let args = self.arena.call_args(args).to_vec();
        if args.len() != arity {
            return;
        }
        let mut values = Vec::new();
        for (index, arg) in args.iter().enumerate() {
            match arg.kind {
                ArenaCallArgKind::Positional(value) => values.push(value),
                ArenaCallArgKind::Named {
                    name: label, value, ..
                } if index + 1 == arity && label.as_str().as_str() == "message" => {
                    values.push(value)
                }
                _ => return,
            }
        }
        if !matches!(self.expr_types.get(&self.arena.expr(call).span), Some(Type::Result(ok, _)) if **ok == Type::Unit)
        {
            return;
        }
        let condition = if operation.as_str() == "ok" {
            if self.expr_types.get(&self.arena.expr(values[0]).span) != Some(&Type::Bool) {
                return;
            }
            self.source[self.arena.expr(values[0]).span.range()].to_string()
        } else {
            let left = self.expr_types.get(&self.arena.expr(values[0]).span);
            let right = self.expr_types.get(&self.arena.expr(values[1]).span);
            if left != right
                || !matches!(
                    left,
                    Some(
                        Type::Bool
                            | Type::Int
                            | Type::Float
                            | Type::Duration
                            | Type::Str
                            | Type::Bytes
                            | Type::Path
                    )
                )
            {
                return;
            }
            let op = if operation.as_str() == "eq" {
                "=="
            } else {
                "!="
            };
            format!(
                "({}) {op} ({})",
                &self.source[self.arena.expr(values[0]).span.range()],
                &self.source[self.arena.expr(values[1]).span.range()]
            )
        };
        let message = *values.last().unwrap();
        if self.expr_types.get(&self.arena.expr(message).span) != Some(&Type::Str) {
            return;
        }
        let mut diagnostic =
            Diagnostic::warning("use a core assertion for statement assertion context")
                .with_code(DiagnosticCode::LintCoreAssert)
                .with_label(Label::primary(
                    self.arena.expr(expression).span,
                    "assert condition, message",
                ));
        // Only a literal message can be delayed without changing eager effects,
        // failure timing, or observation of a binding changed by an operand.
        if matches!(self.arena.expr(message).kind, ArenaExprKind::Str(_))
            && !self.source[self.arena.expr(expression).span.range()].contains('#')
        {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                self.arena.expr(expression).span,
                "use failure-only assertion context",
                format!(
                    "assert {condition}, {}",
                    &self.source[self.arena.expr(message).span.range()]
                ),
            ));
        } else {
            diagnostic = diagnostic.with_note("the existing message runs eagerly; retain this call or bind operands and then the message in their original order before asserting");
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_path_roundtrip(&mut self, expr: ExprId) {
        let expr_span = self.arena.expr(expr).span;
        if !matches!(self.expr_types.get(&expr_span), Some(Type::Path)) {
            return;
        }
        let Some((label_span, replacement)) = self.path_roundtrip_replacement(expr) else {
            return;
        };
        self.push_path_roundtrip_diagnostic(expr_span, label_span, replacement);
    }

    fn lint_inferred_require_target(&mut self, expr: ExprId) {
        let expression = self.arena.expr(expr);
        let ArenaExprKind::Require {
            schema: Some(schema),
            ..
        } = expression.kind
        else {
            return;
        };
        let Some(actual) = self.requirement_targets.get(&expression.span) else {
            return;
        };
        let Some(expected) = self.requirement_expected_targets.get(&expression.span) else {
            return;
        };
        if actual.ty != expected.ty || actual.context != expected.context {
            return;
        }
        let span = self.arena.type_expr_span(schema);
        // Only the schema token range is removed. Comments inside that range
        // carry author intent and must survive a source rewrite.
        if self
            .source
            .get(expression.span.range())
            .is_none_or(|text| text.contains('#'))
        {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "schema target is supplied by the checked boundary",
            )
            .with_code(DiagnosticCode::LintInferredRequireTarget)
            .with_label(Label::secondary(
                span,
                "the same concrete schema is independently known here",
            ))
            .with_fix_hint(FixHint::replacement(
                span,
                "infer the validation target",
                String::new(),
            )),
        );
    }

    fn lint_redundant_require(&mut self, expr: ExprId) {
        let arena_expr = self.arena.expr(expr);
        let ArenaExprKind::Try(inner) = arena_expr.kind else {
            return;
        };
        let Some((label_span, replacement)) = self.require_replacement(inner) else {
            return;
        };
        self.push_redundant_require_diagnostic(arena_expr.span, label_span, replacement);
    }

    fn lint_result_redundant_require(&mut self, expr: ExprId) {
        let expr_span = self.arena.expr(expr).span;
        let Some(expected_ok) = self
            .result_return_ok_types
            .last()
            .and_then(|ty| ty.as_ref())
        else {
            return;
        };
        let Some(Type::Result(ok, _)) = self.expr_types.get(&expr_span) else {
            return;
        };
        if !ok.matches_expected(expected_ok) {
            return;
        }
        let Some((label_span, replacement)) = self.require_replacement(expr) else {
            return;
        };
        self.push_redundant_require_diagnostic(expr_span, label_span, replacement);
    }

    fn lint_redundant_single_interpolation(&mut self, expr: ExprId) {
        let expr_span = self.arena.expr(expr).span;
        let Some((label_span, replacement, code, message)) =
            self.single_interpolation_replacement(expr)
        else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, message)
                .with_code(code)
                .with_label(Label::secondary(
                    label_span,
                    "the interpolation already has the target type",
                ))
                .with_fix_hint(FixHint::replacement(
                    expr_span,
                    "use the interpolated expression directly",
                    replacement,
                )),
        );
    }

    fn lint_scalar_display_parse_roundtrip(&mut self, expr: ExprId) {
        let arena_expr = self.arena.expr(expr);
        let ArenaExprKind::Try(inner) = arena_expr.kind else {
            return;
        };
        let Some((label_span, replacement)) = self.scalar_display_parse_replacement(inner) else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, "redundant display/parse round trip")
                .with_code(DiagnosticCode::LintRedundantDisplayParse)
                .with_label(Label::secondary(
                    label_span,
                    "this value already has the parsed type",
                ))
                .with_fix_hint(FixHint::replacement(
                    arena_expr.span,
                    "use the original value",
                    replacement,
                )),
        );
    }

    fn lint_block_string_concatenation(&mut self, expr: ExprId) {
        fn collect(arena: &AstArena, expr: ExprId, output: &mut String, links: &mut usize) -> bool {
            match arena.expr(expr).kind {
                ArenaExprKind::Str(text) => {
                    output.push_str(arena.string_literal(text));
                    true
                }
                ArenaExprKind::Binary {
                    op: BinaryOp::Add,
                    left,
                    right,
                } => {
                    *links += 1;
                    collect(arena, left, output, links) && collect(arena, right, output, links)
                }
                _ => false,
            }
        }
        let span = self.arena.expr(expr).span;
        let mut value = String::new();
        let mut links = 0;
        if !collect(self.arena, expr, &mut value, &mut links)
            || links == 0
            || !value.contains('\n')
            || value.contains('\r')
        {
            return;
        }
        // Trailing spaces or tabs on a line would become invisible trailing
        // whitespace in a block string, which editors and formatters strip.
        if value.split('\n').any(|line| line.ends_with([' ', '\t'])) {
            return;
        }
        // Report the outermost literal chain once; its operands are part of the same rewrite.
        if self.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::LintPreferBlockString)
                && diagnostic.labels.iter().any(|label| {
                    label.span.source_id == span.source_id
                        && label.span.start() <= span.start()
                        && span.end() <= label.span.end()
                })
        }) {
            return;
        }
        let diagnostic = Diagnostic::new(
            Severity::Warning,
            "constant multiline concatenation can use a block string",
        )
        .with_code(DiagnosticCode::LintPreferBlockString)
        .with_label(Label::secondary(
            span,
            "all pieces are literal text in source order",
        ));
        // Comments and a closing delimiter that cannot stand alone are layout
        // facts: they decide only whether the rewrite is offered.
        let span = widen_over_grouping(self.source, span);
        let suffix = self.source[span.end()..]
            .split(['\r', '\n'])
            .next()
            .unwrap_or("");
        if span_may_contain_comment(self.source, span)
            || !suffix.bytes().all(|byte| matches!(byte, b' ' | b'\t'))
        {
            self.diagnostics.push(diagnostic.with_note(
                "comments, trailing code, and expression consumers need a manual rewrite",
            ));
            return;
        }
        let line_start = self.source[..span.start()]
            .rfind(['\r', '\n'])
            .map_or(0, |offset| offset + 1);
        let indent: String = self.source[line_start..span.start()]
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .collect();
        let margin = format!("{indent}  ");
        let escaped = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('\0', "\\0");
        let content = escaped
            .split('\n')
            .map(|line| format!("{margin}{line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let replacement = format!("\"\"\"\n{content}\n{margin}\"\"\"");
        let witness = format!("let block_value = {replacement}\n");
        let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
            xsh::frontend::source::SourceId::new(0),
            &witness,
        );
        let reproduced = parsed.diagnostics.is_empty() && parsed.arena.statement_ids().next().is_some_and(|statement| matches!(
            parsed.arena.arena.stmt(statement).kind,
            ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(candidate), .. }
                if matches!(parsed.arena.arena.expr(candidate).kind, ArenaExprKind::Str(text) if parsed.arena.arena.string_literal(text).as_ref() == value)
        ));
        self.diagnostics.push(if reproduced {
            diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "preserve the exact decoded text",
                replacement,
            ))
        } else {
            diagnostic.with_note(
                "a block literal at this indentation would not reproduce the decoded text",
            )
        });
    }

    fn lint_redundant_newline_triple_string(&mut self, expr: ExprId) {
        let arena_expr = self.arena.expr(expr);
        let ArenaExprKind::Str(value_id) = arena_expr.kind else {
            return;
        };
        let value = self.arena.string_literal(value_id).clone();
        if value.as_ref() != "\n" {
            return;
        }
        let expr_span = arena_expr.span;
        let Some(source) = self.source.get(expr_span.range()) else {
            return;
        };
        if source != "\"\"\"\n\n\n\"\"\"" {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "single-newline triple string can be written as `\"\\n\"`",
            )
            .with_code(DiagnosticCode::LintRedundantNewlineTripleString)
            .with_label(Label::secondary(
                expr_span,
                "use the escaped newline literal",
            ))
            .with_fix_hint(FixHint::replacement(
                expr_span,
                "replace with `\"\\n\"`",
                "\"\\n\"".to_string(),
            )),
        );
    }

    fn lint_dollar_in_expression_string(&mut self, expr: ExprId) {
        let arena_expr = self.arena.expr(expr);
        let ArenaExprKind::Str(_) = arena_expr.kind else {
            return;
        };
        let expr_span = arena_expr.span;
        let Some(source) = self.source.get(expr_span.range()) else {
            return;
        };
        // Raw strings are the documented way to keep `$` as literal text; the
        // trap is a plain `$name` inside an ordinary (non-raw) expression string.
        if source.starts_with('r') {
            return;
        }
        let bytes = source.as_bytes();
        let mut index = 0;
        while index < bytes.len() && bytes[index] == b'"' {
            index += 1;
        }
        let mut end = bytes.len();
        while end > index && bytes[end - 1] == b'"' {
            end -= 1;
        }
        while index < end {
            match bytes[index] {
                b'\\' => {
                    // `\$` is an explicit literal dollar; skip the escape and its
                    // target rather than treating it as an interpolation marker.
                    index += 2;
                }
                b'$' => {
                    let name_start = index + 1;
                    if bytes
                        .get(name_start)
                        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
                    {
                        let mut name_end = name_start;
                        while name_end < end
                            && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'_')
                        {
                            name_end += 1;
                        }
                        let name = &source[name_start..name_end];
                        if self.is_binding_in_scope_or_assigned(name) {
                            let dollar_span = Span::new(
                                expr_span.source_id,
                                expr_span.start() + index,
                                expr_span.start() + name_end,
                            );
                            self.diagnostics.push(
                                Diagnostic::new(
                                    Severity::Warning,
                                    format!(
                                        "expression string literals never interpolate; `${name}` is literal text here"
                                    ),
                                )
                                .with_code(DiagnosticCode::LintDollarInExpressionString)
                                .with_label(Label::primary(
                                    dollar_span,
                                    format!(
                                        "use `f\"...{{{name}}}...\"` or `+` concatenation to interpolate `{name}`"
                                    ),
                                ))
                                .with_note(
                                    "write `r\"...\"` or `\\$` when a literal dollar sign is intended",
                                ),
                            );
                        }
                        index = name_end;
                        continue;
                    }
                    index += 1;
                }
                _ => index += 1,
            }
        }
    }

    /// `"{name}"` without an `f` prefix is literal text, the most common
    /// f-string mistake. Fires only when every `{...}` is a dotted name and
    /// the literal would be a valid f-string, so the fix changes nothing else.
    fn lint_missing_f_prefix(&mut self, expr: ExprId) {
        let Some((label, first, replacement)) = self.missing_f_prefix(expr) else {
            return;
        };
        let span = self.arena.expr(expr).span;
        let path = replacement.starts_with("fp");
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("`{{{first}}}` is literal text without the `f` prefix"),
            )
            .with_code(DiagnosticCode::LintMissingFPrefix)
            .with_label(Label::primary(
                label,
                format!("`{first}` is a binding in scope"),
            ))
            .with_fix_hint(FixHint::replacement(
                span,
                if path {
                    "use `fp\"...\"` to interpolate"
                } else {
                    "use `f\"...\"` to interpolate"
                },
                replacement,
            )),
        );
    }

    /// The `{name}` label span, the name, and the prefixed literal for a
    /// plain or path string literal that is missing its `f` prefix.
    fn missing_f_prefix(&self, expr: ExprId) -> Option<(Span, String, String)> {
        let arena_expr = self.arena.expr(expr);
        let path = match arena_expr.kind {
            ArenaExprKind::Str(_) => false,
            ArenaExprKind::PathStr(_) => true,
            _ => return None,
        };
        let span = arena_expr.span;
        let source = self.source.get(span.range())?;
        // A string holding XSH source, such as a generated script, uses its
        // braces as code.
        if source.contains("f\\\"") || source.contains("f\"") && source.starts_with("\"\"\"") {
            return None;
        }
        let names = xsh::frontend::syntax::literal::dotted_names_if_formatted(source)?;
        if !names
            .iter()
            .all(|(name, _)| self.is_binding_in_scope_or_assigned(name))
        {
            return None;
        }
        let (first, range) = names[0].clone();
        let label = Span::new(
            span.source_id,
            span.start() + range.start - 1,
            span.start() + range.end + 1,
        );
        let replacement = if path {
            format!("fp{}", &source[1..])
        } else {
            format!("f{source}")
        };
        Some((label, first.to_string(), replacement))
    }

    /// Whether inert data holds a string that `lint.missing-f-prefix` may
    /// turn into an f-string, which a `const` cannot hold. Module scopes are
    /// not built yet when constants are judged, so any `{name}` counts.
    fn holds_missing_f_prefix(&self, value: ExprId) -> bool {
        match self.arena.expr(value).kind {
            ArenaExprKind::Str(_) | ArenaExprKind::PathStr(_) => self
                .source
                .get(self.arena.expr(value).span.range())
                .and_then(xsh::frontend::syntax::literal::dotted_names_if_formatted)
                .is_some(),
            ArenaExprKind::List(items) | ArenaExprKind::Set(items) => self
                .arena
                .list_elements(items)
                .any(|item| self.holds_missing_f_prefix(item.value)),
            ArenaExprKind::Record(fields) => {
                self.arena
                    .record_fields(fields)
                    .iter()
                    .any(|field| match field.kind {
                        ArenaRecordFieldKind::Named { value, .. } => {
                            self.holds_missing_f_prefix(value)
                        }
                        _ => false,
                    })
            }
            _ => false,
        }
    }

    fn is_binding_in_scope_or_assigned(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .any(|scope| scope.contains_key(name))
            || self
                .assigned_names
                .iter()
                .any(|assigned| assigned.as_str().as_str() == name)
    }

    fn lint_json_encode_decode_roundtrip(&mut self, expr: ExprId) {
        let Some(label_span) = self.json_encode_decode_label(expr) else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "JSON encode/decode round trip is usually redundant",
            )
            .with_code(DiagnosticCode::LintJsonRoundtrip)
            .with_label(Label::secondary(
                label_span,
                "review whether JSON normalization is intentional",
            )),
        );
    }

    fn lint_empty_map_initializer(&mut self, ty: Option<TypeExprId>, initializer: &ArenaExprOrRun) {
        let is_map = ty.is_some_and(|ty| {
            matches!(type_expr_kind(self.arena, ty), ArenaTypeExprKind::Map(_, _))
        });
        if !is_map {
            return;
        }
        let ArenaExprOrRun::Expr(expr) = initializer else {
            return;
        };
        if !is_map_empty_call(self.arena, *expr) {
            return;
        }
        let expr_span = self.arena.expr(*expr).span;
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, "use `{}` for an empty map")
                .with_code(DiagnosticCode::LintPreferEmptyMapLiteral)
                .with_label(Label::secondary(
                    expr_span,
                    "`{}` is the empty-map literal in map-typed contexts",
                ))
                .with_fix_hint(FixHint::replacement(
                    expr_span,
                    "replace `map.empty()` with `{}`",
                    "{}".to_string(),
                )),
        );
    }

    fn lint_list_pattern(
        &mut self,
        branches: ArenaRange,
        _else_block: Option<BlockId>,
        span: Span,
    ) {
        fn conjuncts(arena: &AstArena, expr: ExprId, out: &mut Vec<ExprId>) {
            if let ArenaExprKind::Binary {
                op: BinaryOp::And,
                left,
                right,
            } = arena.expr(expr).kind
            {
                conjuncts(arena, left, out);
                conjuncts(arena, right, out);
            } else {
                out.push(expr);
            }
        }
        fn integer(arena: &AstArena, expr: ExprId) -> Option<usize> {
            let ArenaExprKind::Int(value) = arena.expr(expr).kind else {
                return None;
            };
            usize::try_from(arena.int_literal(value).value()?).ok()
        }
        fn index(arena: &AstArena, expr: ExprId, subject: Name) -> Option<usize> {
            let ArenaExprKind::Index {
                base,
                index,
                guarded: false,
            } = arena.expr(expr).kind
            else {
                return None;
            };
            if !matches!(arena.expr(base).kind, ArenaExprKind::Ident(name) if name == subject) {
                return None;
            }
            integer(arena, index)
        }
        let branches = self.arena.if_branches(branches);
        if branches.len() != 1 {
            return;
        }
        let branch = &branches[0];
        let mut conditions = Vec::new();
        conjuncts(self.arena, branch.condition, &mut conditions);
        let Some(&bound) = conditions.first() else {
            return;
        };
        let ArenaExprKind::Binary { op, left, right } = self.arena.expr(bound).kind else {
            return;
        };
        if !matches!(op, BinaryOp::Eq | BinaryOp::Ge) {
            return;
        }
        let Some(count) = integer(self.arena, right).filter(|count| *count <= 16) else {
            return;
        };
        let ArenaExprKind::Call { callee, args } = self.arena.expr(left).kind else {
            return;
        };
        if !args.is_empty() {
            return;
        }
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if name != "len" {
            return;
        }
        let ArenaExprKind::Ident(subject) = self.arena.expr(base).kind else {
            return;
        };
        if !matches!(
            self.expr_types.get(&self.arena.expr(base).span),
            Some(Type::List(_))
        ) {
            return;
        }
        if self.assigned_names.contains(&subject)
            || !self
                .scopes
                .iter()
                .rev()
                .find_map(|scope| scope.get(subject.as_str().as_str()))
                .is_some_and(|binding| binding.comparison_stable && !binding.mutable)
        {
            return;
        }
        let Some(original) = self.source.get(span.range()) else {
            return;
        };
        if original.contains('#') {
            return;
        }
        let mut elements = vec!["_".to_string(); count];
        for &condition in conditions.iter().skip(1) {
            let ArenaExprKind::Binary {
                op: BinaryOp::Eq,
                left,
                right,
            } = self.arena.expr(condition).kind
            else {
                return;
            };
            let Some(index) = index(self.arena, left, subject).filter(|index| *index < count)
            else {
                return;
            };
            if !matches!(
                self.arena.expr(right).kind,
                ArenaExprKind::Int(_)
                    | ArenaExprKind::Float(_)
                    | ArenaExprKind::Bool(_)
                    | ArenaExprKind::Str(_)
                    | ArenaExprKind::Bytes(_)
                    | ArenaExprKind::Duration(_)
                    | ArenaExprKind::Null
            ) {
                return;
            }
            if elements[index] != "_" {
                return;
            }
            let Some(text) = self.source.get(self.arena.expr(right).span.range()) else {
                return;
            };
            elements[index] = text.to_string();
        }
        let block = self.arena.block(branch.block);
        let statements: Vec<_> = self.arena.stmt_ids(block.statements).collect();
        let mut names = FxHashSet::default();
        let mut extracted = 0;
        let mut body_start = self.arena.span(block.span).start() + 1;
        for statement in statements {
            let statement = self.arena.stmt(statement);
            let ArenaStmtKind::Let {
                target,
                ty: None,
                initializer: ArenaExprOrRun::Expr(value),
            } = statement.kind
            else {
                break;
            };
            let Some(index) = index(self.arena, value, subject).filter(|index| *index < count)
            else {
                break;
            };
            let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind else {
                return;
            };
            if elements[index] != "_"
                || name == subject
                || !names.insert(name)
                || self.tag_variants.contains(name.as_str().as_str())
            {
                return;
            }
            elements[index] = name.as_str().to_string();
            body_start = statement.span.end();
            extracted += 1;
        }
        if extracted == 0 {
            return;
        }
        if op == BinaryOp::Ge {
            elements.push("..".to_string());
        }
        let block_span = self.arena.span(block.span);
        let Some(body) = self.source.get(body_start..block_span.end()) else {
            return;
        };
        let Some(suffix) = self.source.get(block_span.end()..span.end()) else {
            return;
        };
        // Retaining the original branch suffix preserves its else block and value context.
        let replacement = format!(
            "if let [{}] = {} {{\n{}{}",
            elements.join(", "),
            subject,
            body,
            suffix
        );
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "prefer a list pattern for bounded element extraction",
            )
            .with_code(DiagnosticCode::LintPreferListPattern)
            .with_label(Label::secondary(
                span,
                "length establishes every element bound before extraction",
            ))
            .with_fix_hint(FixHint::replacement(
                span,
                "bind the list elements after structural matching",
                replacement,
            )),
        );
    }

    fn lint_comparison_chain(&mut self, expr: ExprId) {
        fn ordering(op: BinaryOp) -> bool {
            matches!(
                op,
                BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
            )
        }
        fn ladder(
            arena: &AstArena,
            expr: ExprId,
            out: &mut Vec<(BinaryOp, ExprId, ExprId)>,
        ) -> bool {
            match arena.expr(expr).kind {
                ArenaExprKind::Binary {
                    op: BinaryOp::And,
                    left,
                    right,
                } => ladder(arena, left, out) && ladder(arena, right, out),
                ArenaExprKind::Binary { op, left, right } if ordering(op) => {
                    out.push((op, left, right));
                    true
                }
                _ => false,
            }
        }
        let span = self.arena.expr(expr).span;
        if self.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::LintPreferComparisonChain)
                && diagnostic.labels.iter().any(|label| {
                    label.span.source_id == span.source_id
                        && label.span.start() <= span.start()
                        && label.span.end() >= span.end()
                })
        }) {
            return;
        }
        if !matches!(
            self.arena.expr(expr).kind,
            ArenaExprKind::Binary {
                op: BinaryOp::And,
                ..
            }
        ) {
            return;
        }
        let mut pairs = Vec::new();
        if !ladder(self.arena, expr, &mut pairs) || pairs.len() < 2 {
            return;
        }
        let stable = |id| match self.arena.expr(id).kind {
            ArenaExprKind::Ident(name) => {
                self.scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(name.as_str().as_str()))
                    .is_some_and(|binding| binding.comparison_stable)
                    && !self.assigned_names.contains(&name)
            }
            ArenaExprKind::Int(_) | ArenaExprKind::Float(_) | ArenaExprKind::Str(_) => true,
            _ => false,
        };
        for adjacent in pairs.windows(2) {
            let shared = adjacent[0].2;
            let repeated = adjacent[1].1;
            if !stable(shared)
                || !stable(repeated)
                || !same_ordering_operand(self.arena, shared, repeated)
            {
                return;
            }
        }
        let Some(original) = self.source.get(span.range()) else {
            return;
        };
        if original.contains('#') {
            return;
        }
        let operand_text = |id| {
            let expr = self.arena.expr(id);
            let text = self.source.get(expr.span.range())?;
            let needs_grouping = matches!(
                expr.kind,
                ArenaExprKind::Binary {
                    op: BinaryOp::ResultFallback
                        | BinaryOp::Or
                        | BinaryOp::And
                        | BinaryOp::Eq
                        | BinaryOp::Ne
                        | BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                        | BinaryOp::In
                        | BinaryOp::NotIn,
                    ..
                } | ArenaExprKind::ComparisonChain(_)
                    | ArenaExprKind::If { .. }
                    | ArenaExprKind::Match { .. }
                    | ArenaExprKind::Pipeline { .. }
                    | ArenaExprKind::StructuredPipeline { .. }
            );
            Some(if needs_grouping {
                format!("({text})")
            } else {
                text.to_string()
            })
        };
        let Some(mut replacement) = operand_text(pairs[0].1) else {
            return;
        };
        for (op, _, right) in pairs {
            let Some(text) = operand_text(right) else {
                return;
            };
            replacement.push_str(match op {
                BinaryOp::Lt => " < ",
                BinaryOp::Le => " <= ",
                BinaryOp::Gt => " > ",
                BinaryOp::Ge => " >= ",
                _ => unreachable!(),
            });
            replacement.push_str(&text);
        }
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, "prefer an ordering comparison chain")
                .with_code(DiagnosticCode::LintPreferComparisonChain)
                .with_label(Label::secondary(
                    span,
                    "the repeated adjacent operand is stable",
                ))
                .with_fix_hint(FixHint::replacement(
                    span,
                    "compare adjacent operands once",
                    replacement,
                )),
        );
    }

    fn lint_result_path_parse_roundtrip(&mut self, expr: ExprId) {
        let expr_span = self.arena.expr(expr).span;
        if !matches!(
            self.expr_types.get(&expr_span),
            Some(Type::Result(ok, _)) if matches!(ok.as_ref(), Type::Path)
        ) {
            return;
        }
        let Some((label_span, replacement)) = self.path_parse_result_replacement(expr) else {
            return;
        };
        self.push_path_roundtrip_diagnostic(expr_span, label_span, replacement);
    }

    fn path_roundtrip_replacement(&self, expr: ExprId) -> Option<(Span, String)> {
        match self.arena.expr(expr).kind {
            ArenaExprKind::Try(inner) => self.path_parse_result_replacement(inner),
            ArenaExprKind::Binary {
                op: BinaryOp::ResultFallback,
                left,
                ..
            } => self.path_parse_result_replacement(left),
            _ => None,
        }
    }

    fn path_parse_result_replacement(&self, expr: ExprId) -> Option<(Span, String)> {
        self.path_parse_call_replacement(expr)
            .or_else(|| self.path_parse_literal_replacement(expr))
    }

    fn path_parse_call_replacement(&self, expr: ExprId) -> Option<(Span, String)> {
        let ArenaExprKind::Call { callee, args } = self.arena.expr(expr).kind else {
            return None;
        };
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return None;
        };
        if name != "parse"
            || !matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "Path")
        {
            return None;
        }
        self.path_display_arg_replacement(args)
    }

    fn path_parse_literal_replacement(&self, expr: ExprId) -> Option<(Span, String)> {
        let ArenaExprKind::Call { callee, args } = self.arena.expr(expr).kind else {
            return None;
        };
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return None;
        };
        if name != "parse"
            || !matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "Path")
        {
            return None;
        }
        self.path_literal_arg_replacement(args)
    }

    fn path_display_arg_replacement(&self, args: ArenaRange) -> Option<(Span, String)> {
        let args = self.arena.call_args(args);
        let [arg] = args else {
            return None;
        };
        let ArenaCallArgKind::Positional(arg) = arg.kind else {
            return None;
        };
        let arg_span = self.arena.expr(arg).span;
        let ArenaExprKind::Call {
            callee: display_callee,
            args: display_args,
        } = self.arena.expr(arg).kind
        else {
            return None;
        };
        if !display_args.is_empty() {
            return None;
        }
        let ArenaExprKind::Field { base, name } = self.arena.expr(display_callee).kind else {
            return None;
        };
        let base_span = self.arena.expr(base).span;
        if name != "display"
            || !matches!(self.expr_types.get(&base_span), Some(Type::Path))
            || !self.path_expression_has_utf8_bytes(base)
        {
            return None;
        }
        let replacement = self.source.get(base_span.range())?.to_string();
        Some((arg_span, replacement))
    }

    fn path_literal_arg_replacement(&self, args: ArenaRange) -> Option<(Span, String)> {
        let args = self.arena.call_args(args);
        let [arg] = args else {
            return None;
        };
        let ArenaCallArgKind::Positional(arg) = arg.kind else {
            return None;
        };
        let arg_span = self.arena.expr(arg).span;
        match self.arena.expr(arg).kind {
            ArenaExprKind::Str(_) => {
                let literal_text = self.source.get(arg_span.range())?;
                Some((arg_span, format!("p{literal_text}")))
            }
            ArenaExprKind::FmtString(parts)
                if self.text_path_interpolation_preserves_bytes(parts) =>
            {
                let literal_text = self.source.get(arg_span.range())?;
                Some((arg_span, path_fmt_literal_text(literal_text)?))
            }
            _ => None,
        }
    }

    fn path_expression_has_utf8_bytes(&self, expr: ExprId) -> bool {
        match self.arena.expr(expr).kind {
            ArenaExprKind::PathStr(_) => true,
            ArenaExprKind::PathFmtString(parts) => {
                self.text_path_interpolation_preserves_bytes(parts)
            }
            _ => false,
        }
    }

    // Text interpolation displays Path values, while path interpolation appends
    // their native bytes. The two agree only when every Path piece is UTF-8.
    fn text_path_interpolation_preserves_bytes(&self, parts: ArenaRange) -> bool {
        self.arena.fmt_parts(parts).all(|part| match part {
            ArenaFmtPart::Text(_) => true,
            ArenaFmtPart::Expr(expr, _) => self
                .expr_types
                .get(&self.arena.expr(expr).span)
                .is_some_and(|ty| {
                    matches!(
                        ty,
                        Type::Str
                            | Type::Bool
                            | Type::Int
                            | Type::Float
                            | Type::Duration
                            | Type::Bytes
                    ) || (ty == &Type::Path && self.path_expression_has_utf8_bytes(expr))
                }),
        })
    }

    fn single_path_interpolation_parts_replacement(
        &self,
        parts: ArenaRange,
    ) -> Option<(Span, String)> {
        let inner = single_interpolation_expr(self.arena, parts)?;
        let inner_span = self.arena.expr(inner).span;
        if !matches!(self.expr_types.get(&inner_span), Some(Type::Path)) {
            return None;
        }
        let replacement = self.source.get(inner_span.range())?.to_string();
        Some((inner_span, replacement))
    }

    fn single_interpolation_replacement(
        &self,
        expr: ExprId,
    ) -> Option<(Span, String, DiagnosticCode, &'static str)> {
        let expr_span = self.arena.expr(expr).span;
        match self.arena.expr(expr).kind {
            ArenaExprKind::FmtString(parts)
                if matches!(self.expr_types.get(&expr_span), Some(Type::Str)) =>
            {
                let inner = single_interpolation_expr(self.arena, parts)?;
                let inner_span = self.arena.expr(inner).span;
                if !matches!(self.expr_types.get(&inner_span), Some(Type::Str)) {
                    return None;
                }
                let replacement = self.source.get(inner_span.range())?.to_string();
                Some((
                    inner_span,
                    replacement,
                    DiagnosticCode::LintRedundantStringInterpolation,
                    "redundant single-value string interpolation",
                ))
            }
            ArenaExprKind::PathFmtString(parts)
                if matches!(self.expr_types.get(&expr_span), Some(Type::Path)) =>
            {
                let (label_span, replacement) =
                    self.single_path_interpolation_parts_replacement(parts)?;
                Some((
                    label_span,
                    replacement,
                    DiagnosticCode::LintRedundantPathInterpolation,
                    "redundant single-value path interpolation",
                ))
            }
            _ => None,
        }
    }

    fn command_single_fmt_replacement(&self, expr: ExprId) -> Option<String> {
        let ArenaExprKind::FmtString(parts) = self.arena.expr(expr).kind else {
            return None;
        };
        let inner = single_interpolation_expr(self.arena, parts)?;
        if simple_command_value_expr(self.arena, inner) {
            if !self.text_path_interpolation_preserves_bytes(parts) {
                return None;
            }
            return Some(command_value_replacement(self.arena, inner));
        }
        None
    }

    fn require_replacement(&self, expr: ExprId) -> Option<(Span, String)> {
        let expr_span = self.arena.expr(expr).span;
        let ArenaExprKind::Require { value, .. } = self.arena.expr(expr).kind else {
            return None;
        };
        let Some(Type::Result(ok, _)) = self.expr_types.get(&expr_span) else {
            return None;
        };
        let value_span = self.arena.expr(value).span;
        let value_ty = self.expr_types.get(&value_span)?;
        if value_ty.is_recovery()
            || value_ty.contains_any()
            || matches!(value_ty, Type::Record(fields) if fields.is_empty())
        {
            return None;
        }
        if expr_is_dynamic_require_boundary(self.arena, value) {
            return None;
        }
        // Assignability permits constrained scalar boundaries and record width
        // changes. Removing validation requires the identical validated shape.
        if value_ty != ok.as_ref() || type_has_unsigned_constraint(ok) {
            return None;
        }
        if self
            .source
            .get(value_span.end()..expr_span.end())?
            .contains('#')
        {
            return None;
        }
        let replacement = self.source.get(value_span.range())?.to_string();
        Some((value_span, replacement))
    }

    fn scalar_display_parse_replacement(&self, expr: ExprId) -> Option<(Span, String)> {
        let expr_span = self.arena.expr(expr).span;
        let ArenaExprKind::Call { callee, args } = self.arena.expr(expr).kind else {
            return None;
        };
        if !args.is_empty() {
            return None;
        }
        let ArenaExprKind::Field {
            base: parse_base,
            name: parse_name,
        } = self.arena.expr(callee).kind
        else {
            return None;
        };
        let (expected_input, expected_output) = match parse_name.as_str().as_str() {
            "parse_int" => (Type::Int, Type::Int),
            "parse_float" => (Type::Float, Type::Float),
            _ => return None,
        };
        if !matches!(self.expr_types.get(&expr_span), Some(ty) if ty.matches_expected(&Type::Result(Box::new(expected_output), Box::new(Type::Error))))
        {
            return None;
        }
        if let Some((span, replacement)) =
            self.scalar_display_method_replacement(parse_base, &expected_input)
        {
            return Some((span, replacement));
        }
        self.scalar_fmt_parse_replacement(parse_base, &expected_input)
    }

    fn scalar_display_method_replacement(
        &self,
        expr: ExprId,
        expected_input: &Type,
    ) -> Option<(Span, String)> {
        let expr_span = self.arena.expr(expr).span;
        let ArenaExprKind::Call {
            callee: display_callee,
            args: display_args,
        } = self.arena.expr(expr).kind
        else {
            return None;
        };
        if !display_args.is_empty() {
            return None;
        }
        let ArenaExprKind::Field {
            base: original,
            name: display_name,
        } = self.arena.expr(display_callee).kind
        else {
            return None;
        };
        let original_span = self.arena.expr(original).span;
        if display_name != "display"
            || !matches!(self.expr_types.get(&original_span), Some(ty) if ty.matches_expected(expected_input))
        {
            return None;
        }
        let replacement = self.source.get(original_span.range())?.to_string();
        Some((expr_span, replacement))
    }

    fn scalar_fmt_parse_replacement(
        &self,
        expr: ExprId,
        expected_input: &Type,
    ) -> Option<(Span, String)> {
        let expr_span = self.arena.expr(expr).span;
        let ArenaExprKind::FmtString(parts) = self.arena.expr(expr).kind else {
            return None;
        };
        let original = single_interpolation_expr(self.arena, parts)?;
        let original_span = self.arena.expr(original).span;
        if !matches!(self.expr_types.get(&original_span), Some(ty) if ty.matches_expected(expected_input))
        {
            return None;
        }
        let replacement = self.source.get(original_span.range())?.to_string();
        Some((expr_span, replacement))
    }

    fn json_encode_decode_label(&self, expr: ExprId) -> Option<Span> {
        let ArenaExprKind::Try(decode_call) = self.arena.expr(expr).kind else {
            return None;
        };
        let ArenaExprKind::Call {
            callee: decode_callee,
            args: decode_args,
        } = self.arena.expr(decode_call).kind
        else {
            return None;
        };
        if !is_module_call(self.arena, decode_callee, "json", "decode") {
            return None;
        }
        let [decode_arg] = self.arena.call_args(decode_args) else {
            return None;
        };
        let ArenaCallArgKind::Positional(decode_arg) = decode_arg.kind else {
            return None;
        };
        let ArenaExprKind::Try(encode_call) = self.arena.expr(decode_arg).kind else {
            return None;
        };
        let encode_span = self.arena.expr(encode_call).span;
        let ArenaExprKind::Call {
            callee: encode_callee,
            args: encode_args,
        } = self.arena.expr(encode_call).kind
        else {
            return None;
        };
        if !is_module_call(self.arena, encode_callee, "json", "encode") || encode_args.is_empty() {
            return None;
        }
        Some(encode_span)
    }

    fn push_redundant_require_diagnostic(
        &mut self,
        span: Span,
        label_span: Span,
        replacement: String,
    ) {
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, "redundant schema require")
                .with_code(DiagnosticCode::LintRedundantRequire)
                .with_label(Label::secondary(
                    label_span,
                    "this expression already has the required type",
                ))
                .with_fix_hint(FixHint::replacement(
                    span,
                    "use the already-typed value",
                    replacement,
                )),
        );
    }

    /// Recognize an explicit text conversion of a checked Path receiver.
    fn path_display_base(&self, expr: ExprId) -> Option<Span> {
        let ArenaExprKind::Call { callee, args } = self.arena.expr(expr).kind else {
            return None;
        };
        if !args.is_empty() {
            return None;
        }
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return None;
        };
        if name != "display" {
            return None;
        }
        let base_span = self.arena.expr(base).span;
        if self.expr_types.get(&base_span) != Some(&Type::Path) {
            return None;
        }
        Some(base_span)
    }

    // Interpolation renders a Path itself: f-strings and `print` display it as
    // text exactly like `.display()`, while fp-strings and command words append
    // its native bytes, which `.display()` would replace with lossy text.
    // Str-typed boundaries (arguments, bindings, JSON) keep `.display()` as the
    // explicit conversion, so only interpolated uses reach this rule.
    fn lint_redundant_path_display(&mut self, expr: ExprId, site: PathDisplaySite) {
        let expr_span = self.arena.expr(expr).span;
        let Some(base_span) = self.path_display_base(expr) else {
            return;
        };
        let base_text = &self.source[base_span.range()];
        let edit = match site {
            PathDisplaySite::Interpolation => Some((expr_span, base_text.to_string())),
            // A command word that loses its call syntax would read as literal
            // text, and a parenthesized bare name is stale command syntax, so
            // the whole word takes the canonical command value spelling.
            PathDisplaySite::CommandWord(word_span) => {
                let value = match self.arena.expr(expr).kind {
                    ArenaExprKind::Call { callee, .. } => match self.arena.expr(callee).kind {
                        ArenaExprKind::Field { base, .. } => {
                            command_value_replacement(self.arena, base)
                        }
                        _ => String::new(),
                    },
                    _ => String::new(),
                };
                Some((
                    word_span,
                    if value.is_empty() {
                        format!("${{{base_text}}}")
                    } else {
                        value
                    },
                ))
            }
            PathDisplaySite::Splice => None,
        }
        .filter(|(span, _)| !self.source[span.range()].contains('#'));
        let mut diagnostic = Diagnostic::new(Severity::Warning, "needless `.display()` on an interpolated Path")
            .with_code(DiagnosticCode::LintRedundantPathDisplay)
            .with_label(Label::secondary(expr_span, "interpolation already renders the Path"))
            .with_note("fp-strings and command words take the Path's native bytes; keep `.display()` where a Str is required");
        if let Some((span, replacement)) = edit {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "interpolate the Path directly",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    /// `Path(...)` rewritten as path syntax. Text pieces that display a Path,
    /// spelled `{p}` or `{p.display()}`, become native path pieces: the
    /// rewrite keeps every UTF-8 path and stops replacing other bytes.
    fn path_constructor_replacement(&self, arg: ExprId) -> Option<String> {
        let arg_span = self.arena.expr(arg).span;
        let arg_text = self.source.get(arg_span.range())?;
        if let Some(base) = self.path_display_base(arg) {
            return self.source.get(base.range()).map(str::to_string);
        }
        match self.arena.expr(arg).kind {
            ArenaExprKind::Str(_) => Some(format!("p{arg_text}")),
            ArenaExprKind::FmtString(parts) => self.path_fmt_from_text_fmt(arg_span, parts),
            _ => single_line_interpolation(arg_text)
                .map(|interpolation| format!("fp\"{interpolation}\"")),
        }
    }

    fn path_fmt_from_text_fmt(&self, fmt_span: Span, parts: ArenaRange) -> Option<String> {
        if let Some(inner) = single_interpolation_expr(self.arena, parts) {
            let inner_span = self.arena.expr(inner).span;
            let base = self.path_display_base(inner).or_else(|| {
                (self.expr_types.get(&inner_span) == Some(&Type::Path)).then_some(inner_span)
            })?;
            return self.source.get(base.range()).map(str::to_string);
        }
        let mut edits = Vec::new();
        for part in self.arena.fmt_parts(parts) {
            let ArenaFmtPart::Expr(expr, spec) = part else {
                continue;
            };
            let expr_span = self.arena.expr(expr).span;
            if spec.is_none()
                && let Some(base) = self.path_display_base(expr)
            {
                edits.push((expr_span, base));
                continue;
            }
            let text_safe = self.expr_types.get(&expr_span).is_some_and(|ty| {
                matches!(
                    ty,
                    Type::Str | Type::Bool | Type::Int | Type::Float | Type::Duration | Type::Path
                )
            });
            if !text_safe {
                return None;
            }
        }
        let mut text = self.source.get(fmt_span.range())?.to_string();
        for (expr_span, base) in edits.into_iter().rev() {
            let base_text = self.source.get(base.range())?;
            text.replace_range(
                expr_span.start() - fmt_span.start()..expr_span.end() - fmt_span.start(),
                base_text,
            );
        }
        path_fmt_literal_text(&text)
    }

    fn push_path_roundtrip_diagnostic(
        &mut self,
        span: Span,
        label_span: Span,
        replacement: String,
    ) {
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, "redundant path display/parse round trip")
                .with_code(DiagnosticCode::LintRedundantPathParse)
                .with_label(Label::secondary(
                    label_span,
                    "this value is already a Path before `.display()`",
                ))
                .with_fix_hint(FixHint::replacement(
                    span,
                    "use the original Path value",
                    replacement,
                )),
        );
    }

    fn pattern_condition_complement(
        &self,
        value: ExprId,
        selected: PatternId,
        complement: PatternId,
    ) -> bool {
        if matches!(
            self.arena.pattern(complement).kind,
            ArenaPatternKind::Wildcard
        ) {
            return true;
        }
        if !matches!(
            self.expr_types.get(&self.arena.expr(value).span),
            Some(Type::Result(_, _))
        ) {
            return false;
        }
        match (
            &self.arena.pattern(selected).kind,
            &self.arena.pattern(complement).kind,
        ) {
            (
                ArenaPatternKind::Constructor {
                    name: left,
                    arg: Some(left_arg),
                },
                ArenaPatternKind::Constructor {
                    name: right,
                    arg: Some(right_arg),
                },
            ) => {
                ((*left == "Ok" && *right == "Err") || (*left == "Err" && *right == "Ok"))
                    && matches!(
                        self.arena.pattern(*left_arg).kind,
                        ArenaPatternKind::Binding(_) | ArenaPatternKind::Wildcard
                    )
                    && matches!(
                        self.arena.pattern(*right_arg).kind,
                        ArenaPatternKind::Wildcard
                    )
            }
            _ => false,
        }
    }

    fn pattern_condition_selected_binds(&self, pattern: PatternId) -> bool {
        match self.arena.pattern(pattern).kind {
            ArenaPatternKind::Binding(_)
            | ArenaPatternKind::Wildcard
            | ArenaPatternKind::Alternation(_)
            | ArenaPatternKind::Tuple(_) => return false,
            ArenaPatternKind::List {
                elements,
                rest: Some(_),
            } if elements.is_empty() => return false,
            ArenaPatternKind::Constructor { name, .. }
                if name != "Ok"
                    && name != "Err"
                    && self
                        .arena
                        .type_defs
                        .iter()
                        .any(|definition| match definition.body {
                            ArenaTypeDefBody::TagUnion(variants) if variants.len() == 1 => {
                                self.arena.tag_variants(variants)[0].name == name
                            }
                            _ => false,
                        }) =>
            {
                return false;
            }
            _ => {}
        }
        !self.pattern_test_fix_is_nonbinding(pattern)
    }

    fn pattern_conditional_replacement(
        &self,
        span: Span,
        replacement: &str,
        expression: bool,
    ) -> Option<String> {
        let prefix = "let __pattern_conditional_value = ";
        let fragment = if expression {
            format!("{prefix}{replacement}\n")
        } else {
            format!("{replacement}\n")
        };
        let formatted = super::format::Formatter::new().format_source(span.source_id, &fragment);
        if !formatted.diagnostics.is_empty() {
            return None;
        }
        let text = formatted.formatted.trim_end();
        let text = if expression {
            text.strip_prefix(prefix)?
        } else {
            text
        };
        let line_start = self.source[..span.start()]
            .rfind('\n')
            .map_or(0, |start| start + 1);
        let indent = &self.source[line_start..span.start()];
        let indent = &indent[..indent.len() - indent.trim_start().len()];
        let mut text = text.replace('\n', &format!("\n{indent}"));
        if !expression {
            let following = self.source.get(span.end()..)?;
            let whitespace_len = following.len() - following.trim_start().len();
            let whitespace = &following[..whitespace_len];
            if !following.trim_start().starts_with('}')
                && !following.trim_start().is_empty()
                && whitespace.matches('\n').count() == 1
            {
                text.push('\n');
            }
        }
        Some(text)
    }

    fn lint_pattern_conditional_stmt(&mut self, value: ExprId, arms: ArenaRange, span: Span) {
        let [selected, complement] = self.arena.match_arms(arms) else {
            return;
        };
        if selected.guard.is_some()
            || complement.guard.is_some()
            || !self.pattern_condition_selected_binds(selected.pattern)
            || !self.pattern_condition_complement(value, selected.pattern, complement.pattern)
        {
            return;
        }
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "two-arm match can use a pattern conditional",
        )
        .with_code(DiagnosticCode::LintPatternConditional)
        .with_label(Label::secondary(
            span,
            "bind the selected pattern with `if let`",
        ));
        if self
            .source
            .get(span.range())
            .is_some_and(|source| !source.contains('#'))
        {
            let subject = self.source.get(self.arena.expr(value).span.range());
            let pattern = self.source.get(
                self.arena
                    .span(self.arena.pattern(selected.pattern).span)
                    .range(),
            );
            let selected_body = self.source.get(
                self.arena
                    .span(self.arena.block(selected.block).span)
                    .range(),
            );
            let other = self.source.get(
                self.arena
                    .span(self.arena.block(complement.block).span)
                    .range(),
            );
            if let (Some(subject), Some(pattern), Some(body), Some(other)) =
                (subject, pattern, selected_body, other)
            {
                let other_empty = self.arena.block(complement.block).statements.is_empty();
                let braced = |text: &str| {
                    if text.trim().starts_with('{') {
                        text.to_string()
                    } else {
                        format!("{{ {text} }}")
                    }
                };
                let body = braced(body);
                let suffix = if other_empty {
                    String::new()
                } else {
                    format!(" else {}", braced(other))
                };
                if let Some(replacement) = self.pattern_conditional_replacement(
                    span,
                    &format!("if let {pattern} = {subject} {body}{suffix}"),
                    false,
                ) {
                    diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                        span,
                        "use a pattern conditional",
                        replacement,
                    ));
                }
            }
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_pattern_conditional_expr(&mut self, expr: ExprId) {
        let expression = self.arena.expr(expr);
        let ArenaExprKind::Match { value, arms } = expression.kind else {
            return;
        };
        let [selected, complement] = self.arena.match_expr_arms(arms) else {
            return;
        };
        if matches!(self.arena.expr(selected.value).kind, ArenaExprKind::Bool(_))
            && matches!(
                self.arena.expr(complement.value).kind,
                ArenaExprKind::Bool(_)
            )
        {
            return;
        }
        if selected.guard.is_some()
            || complement.guard.is_some()
            || !self.pattern_condition_selected_binds(selected.pattern)
            || !self.pattern_condition_complement(value, selected.pattern, complement.pattern)
        {
            return;
        }
        let span = expression.span;
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "two-arm match can use a pattern conditional",
        )
        .with_code(DiagnosticCode::LintPatternConditional)
        .with_label(Label::secondary(
            span,
            "bind the selected pattern with `if let`",
        ));
        if self
            .source
            .get(span.range())
            .is_some_and(|source| !source.contains('#'))
        {
            let subject = self.source.get(self.arena.expr(value).span.range());
            let pattern = self.source.get(
                self.arena
                    .span(self.arena.pattern(selected.pattern).span)
                    .range(),
            );
            let branch = |value| {
                self.source
                    .get(self.arena.expr(value).span.range())
                    .map(|text| {
                        if matches!(self.arena.expr(value).kind, ArenaExprKind::ValueBlock(_)) {
                            text.to_string()
                        } else {
                            format!("{{ {text} }}")
                        }
                    })
            };
            if let (Some(subject), Some(pattern), Some(yes), Some(no)) = (
                subject,
                pattern,
                branch(selected.value),
                branch(complement.value),
            ) && let Some(replacement) = self.pattern_conditional_replacement(
                span,
                &format!("if let {pattern} = {subject} {yes} else {no}"),
                true,
            ) {
                diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                    span,
                    "use a pattern conditional",
                    replacement,
                ));
            }
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_error_fallback_block(&mut self, expr: ExprId) {
        let expression = self.arena.expr(expr);
        let ArenaExprKind::Match { value, arms } = expression.kind else {
            return;
        };
        let [first, second] = self.arena.match_expr_arms(arms) else {
            return;
        };
        if first.guard.is_some() || second.guard.is_some() {
            return;
        }
        let constructor = |pattern| match self.arena.pattern(pattern).kind {
            ArenaPatternKind::Constructor {
                name,
                arg: Some(argument),
            } => Some((name, argument)),
            _ => None,
        };
        let (Some((first_name, first_arg)), Some((second_name, second_arg))) =
            (constructor(first.pattern), constructor(second.pattern))
        else {
            return;
        };
        let (success, success_arg, handler, error_arg) =
            if first_name == "Ok" && second_name == "Err" {
                (first, first_arg, second, second_arg)
            } else if first_name == "Err" && second_name == "Ok" {
                (second, second_arg, first, first_arg)
            } else {
                return;
            };
        let ArenaPatternKind::Binding(success_name) = self.arena.pattern(success_arg).kind else {
            return;
        };
        let identity = match self.arena.expr(success.value).kind {
            ArenaExprKind::Ident(name) => name == success_name,
            ArenaExprKind::ValueBlock(block) => {
                let statements = self
                    .arena
                    .stmt_ids(self.arena.block(block).statements)
                    .collect::<Vec<_>>();
                matches!(statements.as_slice(), [stmt] if match self.arena.stmt(*stmt).kind {
                    ArenaStmtKind::TailBareIdent(name) => name == success_name,
                    ArenaStmtKind::Expr(value) => matches!(self.arena.expr(value).kind, ArenaExprKind::Ident(name) if name == success_name),
                    _ => false,
                })
            }
            _ => false,
        };
        if !identity {
            return;
        }
        let error_name = match self.arena.pattern(error_arg).kind {
            ArenaPatternKind::Binding(name)
                if !self.tag_variants.contains(name.as_str().as_str()) =>
            {
                name.to_string()
            }
            ArenaPatternKind::Wildcard => "_".to_string(),
            _ => return,
        };
        let Some(Type::Result(success_type, _)) = self.expr_types.get(&self.arena.expr(value).span)
        else {
            return;
        };
        if success_type.contains_any()
            || **success_type == Type::Unknown
            || self.expr_types.get(&expression.span) != Some(success_type.as_ref())
        {
            return;
        }
        let Some(original) = self.source.get(expression.span.range()) else {
            return;
        };
        if original.contains('#') {
            return;
        }
        let Some(subject) = self.source.get(self.arena.expr(value).span.range()) else {
            return;
        };
        let Some(handler_source) = self.source.get(self.arena.expr(handler.value).span.range())
        else {
            return;
        };
        let line_indent = |offset: usize| {
            self.source[..offset]
                .rsplit('\n')
                .next()
                .unwrap_or("")
                .chars()
                .take_while(|character| *character == ' ')
                .count()
        };
        let indent = line_indent(expression.span.start());
        let handler = if matches!(
            self.arena.expr(handler.value).kind,
            ArenaExprKind::ValueBlock(_)
        ) {
            let Some(rest) = handler_source.strip_prefix('{') else {
                return;
            };
            let removed_indent =
                line_indent(self.arena.expr(handler.value).span.start()).saturating_sub(indent);
            let mut lines = rest.split('\n');
            let mut body = lines.next().unwrap_or("").to_string();
            for line in lines {
                body.push('\n');
                let trim = line
                    .bytes()
                    .take(removed_indent)
                    .take_while(|byte| *byte == b' ')
                    .count();
                body.push_str(&line[trim..]);
            }
            format!("{{ |{error_name}|{body}")
        } else {
            // Keep continuation lines intact: formatting may wrap an expression
            // handler without changing its lazy fallback behavior.
            format!(
                "{{ |{error_name}|\n{}{handler_source}\n{}}}",
                " ".repeat(indent + 2),
                " ".repeat(indent)
            )
        };
        let subject = match self.arena.expr(value).kind {
            ArenaExprKind::Ident(_)
            | ArenaExprKind::Call { .. }
            | ArenaExprKind::Field { .. }
            | ArenaExprKind::Index { .. }
            | ArenaExprKind::Try(_) => subject.to_string(),
            _ => format!("({subject})"),
        };
        let replacement = format!("{subject} ?? {handler}");
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "identity success match can use an error fallback block",
            )
            .with_code(DiagnosticCode::LintErrorFallbackBlock)
            .with_label(Label::secondary(
                expression.span,
                "retain the lazy error handler with `??`",
            ))
            .with_fix_hint(FixHint::replacement(
                expression.span,
                "use an error fallback block",
                replacement,
            )),
        );
    }

    fn lint_adjacent_pattern_arms(&mut self, arms: Vec<PatternArm>) {
        // Bodies compare by canonical tree, not spelling, so formatting cannot
        // change which arms merge.
        let keys: Vec<String> = arms
            .iter()
            .map(|arm| super::format::canonical_subtree(self.arena, self.source, arm.2))
            .collect();
        let mut start = 0;
        while start + 1 < arms.len() {
            let body_span = match arms[start].2 {
                Ok(block) => self.arena.span(self.arena.block(block).span),
                Err(expr) => self.arena.expr(expr).span,
            };
            let Some(body) = self.source.get(body_span.range()) else {
                return;
            };
            if arms[start].1.is_some() {
                start += 1;
                continue;
            }
            let mut end = start + 1;
            while end < arms.len() && arms[end].1.is_none() && keys[end] == keys[start] {
                end += 1;
            }
            if end == start + 1 {
                start = end;
                continue;
            }
            let span = Span::new(
                arms[start].3.source_id,
                arms[start].3.start(),
                arms[end - 1].3.end(),
            );
            let Some(original) = self.source.get(span.range()) else {
                return;
            };
            if original.contains('#') {
                start = end;
                continue;
            }
            let patterns: Option<Vec<_>> = arms[start..end]
                .iter()
                .map(|(pattern, _, _, _)| {
                    self.source
                        .get(self.arena.span(self.arena.pattern(*pattern).span).range())
                })
                .collect();
            let Some(patterns) = patterns else { return };
            let replacement = format!("{} => {}", patterns.join(" | "), body);
            let mut candidate = self.source.to_string();
            candidate.replace_range(span.range(), &replacement);
            let (_, cst_errors) =
                xsh::frontend::syntax::cst::SyntaxTree::parse(span.source_id, &candidate);
            let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
                span.source_id,
                &candidate,
            );
            if cst_errors.is_empty() && parsed.diagnostics.is_empty() {
                let checked = xsh::frontend::check::Checker::check_arena(&parsed.arena, &candidate);
                // Rechecking the combined pattern proves equal resolved capture types,
                // including nominal constructor payloads and aliases of those types.
                if checked.diagnostics.is_empty() {
                    self.diagnostics.push(
                        Diagnostic::new(
                            Severity::Warning,
                            "adjacent match arms can share an alternative pattern",
                        )
                        .with_code(DiagnosticCode::LintIdenticalMatchArms)
                        .with_label(Label::secondary(
                            span,
                            "unguarded arms share a checked body and capture contract",
                        ))
                        .with_fix_hint(FixHint::replacement(
                            span,
                            "combine the compatible alternatives",
                            replacement,
                        )),
                    );
                }
            }
            start = end;
        }
    }

    fn lint_boolean_match(&mut self, expr: ExprId) {
        let expression = self.arena.expr(expr);
        let ArenaExprKind::Match { value, arms } = expression.kind else {
            return;
        };
        let [selected, complement] = self.arena.match_expr_arms(arms) else {
            return;
        };
        let wildcard_complement = matches!(
            self.arena.pattern(complement.pattern).kind,
            ArenaPatternKind::Wildcard
        );
        let result_complement = matches!(
            self.expr_types.get(&self.arena.expr(value).span),
            Some(Type::Result(_, _))
        ) && match (
            self.arena.pattern(selected.pattern).kind.clone(),
            self.arena.pattern(complement.pattern).kind.clone(),
        ) {
            (
                ArenaPatternKind::Constructor {
                    name: left,
                    arg: Some(left_arg),
                },
                ArenaPatternKind::Constructor {
                    name: right,
                    arg: Some(right_arg),
                },
            ) => {
                ((left == "Ok" && right == "Err") || (left == "Err" && right == "Ok"))
                    && matches!(
                        self.arena.pattern(left_arg).kind,
                        ArenaPatternKind::Wildcard
                    )
                    && matches!(
                        self.arena.pattern(right_arg).kind,
                        ArenaPatternKind::Wildcard
                    )
            }
            _ => false,
        };
        if selected.guard.is_some()
            || complement.guard.is_some()
            || !(wildcard_complement || result_complement)
        {
            return;
        }
        let (ArenaExprKind::Bool(yes), ArenaExprKind::Bool(no)) = (
            self.arena.expr(selected.value).kind,
            self.arena.expr(complement.value).kind,
        ) else {
            return;
        };
        if yes == no {
            return;
        }
        let span = expression.span;
        let mut diagnostic =
            Diagnostic::new(Severity::Warning, "boolean match can use a pattern test")
                .with_code(DiagnosticCode::LintBooleanPatternTest)
                .with_label(Label::secondary(
                    span,
                    "test the selected pattern with `is`",
                ));
        if self.pattern_test_fix_is_nonbinding(selected.pattern)
            && self
                .source
                .get(span.range())
                .is_some_and(|source| !source.contains('#'))
        {
            let subject = self.arena.expr(value).span;
            let pattern = self.arena.span(self.arena.pattern(selected.pattern).span);
            if let (Some(subject), Some(pattern)) = (
                self.source.get(subject.range()),
                self.source.get(pattern.range()),
            ) {
                let pattern = if matches!(
                    self.arena.pattern(selected.pattern).kind,
                    ArenaPatternKind::Alternation(_)
                ) {
                    format!("({pattern})")
                } else {
                    pattern.to_string()
                };
                let replacement = if yes {
                    format!("(({subject}) is {pattern})")
                } else {
                    format!("!(({subject}) is {pattern})")
                };
                diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                    span,
                    "use a non-binding pattern test",
                    replacement,
                ));
            }
        }
        self.diagnostics.push(diagnostic);
    }

    fn pattern_test_fix_is_nonbinding(&self, pattern: PatternId) -> bool {
        match self.arena.pattern(pattern).kind {
            ArenaPatternKind::Wildcard
            | ArenaPatternKind::Literal(_)
            | ArenaPatternKind::Facet(_)
            | ArenaPatternKind::TestName { .. } => true,
            ArenaPatternKind::Binding(name) => self.tag_variants.contains(name.as_str().as_str()),
            ArenaPatternKind::Type { binding, .. } => binding.is_none(),
            ArenaPatternKind::Constructor { arg, .. } => {
                arg.is_none_or(|arg| self.pattern_test_fix_is_nonbinding(arg))
            }
            ArenaPatternKind::Record { fields, .. }
            | ArenaPatternKind::ErrorVariant { fields, .. } => self
                .arena
                .pattern_fields(fields)
                .iter()
                .all(|field| self.pattern_test_fix_is_nonbinding(field.pattern)),
            ArenaPatternKind::Tuple(patterns) => self
                .arena
                .pattern_ids(patterns)
                .all(|pattern| self.pattern_test_fix_is_nonbinding(pattern)),
            ArenaPatternKind::List { elements, rest } => self
                .arena
                .pattern_ids(elements)
                .chain(rest)
                .all(|pattern| self.pattern_test_fix_is_nonbinding(pattern)),
            ArenaPatternKind::Alias { .. } => false,
            ArenaPatternKind::Group(child) => self.pattern_test_fix_is_nonbinding(child),
            ArenaPatternKind::Alternation(items) | ArenaPatternKind::Text(items) => self
                .arena
                .pattern_ids(items)
                .all(|child| self.pattern_test_fix_is_nonbinding(child)),
            ArenaPatternKind::TextHole { binding, .. } => binding.is_none(),
        }
    }

    fn lint_interactive_command(&mut self, name: &str, span: Span) {
        let Some(replacement) = self
            .interactive_command_replacement
            .and_then(|replacement| replacement(name))
        else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Error,
                format!(
                    "`{name}` is for interactive use; use {} in scripts",
                    replacement
                ),
            )
            .with_code(DiagnosticCode::LintInteractiveCommand)
            .with_label(Label::secondary(span, "interactive compatibility command")),
        );
    }

    fn lint_redundant_command_arg_interpolation(&mut self, arg: &ArenaCommandArg) {
        let arg_span = self.arena.span(arg.span);
        let ArenaCommandArgKind::Word(parts) = arg.kind else {
            return;
        };
        let parts: Vec<ArenaWordPart> = self.arena.word_parts(parts).collect();
        let (single_interp, is_shorthand) = match parts.as_slice() {
            [ArenaWordPart::Interpolation(expr)] => (*expr, false),
            [ArenaWordPart::Shorthand(expr)] => (*expr, true),
            _ => return,
        };
        if matches!(self.arena.expr(single_interp).kind, ArenaExprKind::Ident(_)) {
            return;
        }
        if self.can_bare_in_command_arg(single_interp) {
            let mut span = self.arena.expr(single_interp).span;
            if is_shorthand {
                span.set_start(span.start() + 1); // skip `$`
            }
            let Some(replacement) = self.source.get(span.range()) else {
                return;
            };
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    "command args can use expression syntax directly",
                )
                .with_code(DiagnosticCode::LintRedundantCommandInterpolation)
                .with_label(Label::secondary(
                    arg_span,
                    "this interpolation is unnecessary",
                ))
                .with_fix_hint(FixHint::replacement(
                    arg_span,
                    "use the expression directly",
                    replacement.to_string(),
                )),
            );
        }
    }

    fn can_bare_in_command_arg(&self, expr: ExprId) -> bool {
        match self.arena.expr(expr).kind {
            ArenaExprKind::Call { callee, .. } => self.can_bare_chain_base(callee),
            ArenaExprKind::Index { base, index, .. } => {
                self.can_bare_chain_base(base)
                    && matches!(self.arena.expr(index).kind, ArenaExprKind::Int(_))
            }
            ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
                self.command_chain_has_call_or_index(expr) && self.can_bare_chain_base(base)
            }
            _ => false,
        }
    }

    fn can_bare_chain_base(&self, expr: ExprId) -> bool {
        match self.arena.expr(expr).kind {
            ArenaExprKind::Ident(_)
            | ArenaExprKind::Str(_)
            | ArenaExprKind::Int(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::FmtString(_)
            | ArenaExprKind::PathFmtString(_)
            | ArenaExprKind::PathStr(_)
            | ArenaExprKind::GlobStr(_) => true,
            ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
                self.can_bare_chain_base(base)
            }
            ArenaExprKind::Call { callee, .. } => self.can_bare_chain_base(callee),
            ArenaExprKind::Index { base, index, .. } => {
                self.can_bare_chain_base(base)
                    && matches!(self.arena.expr(index).kind, ArenaExprKind::Int(_))
            }
            _ => false,
        }
    }

    fn command_chain_has_call_or_index(&self, expr: ExprId) -> bool {
        match self.arena.expr(expr).kind {
            ArenaExprKind::Call { .. } | ArenaExprKind::Index { .. } => true,
            ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
                self.command_chain_has_call_or_index(base)
            }
            _ => false,
        }
    }
}

impl<'a> Linter<'a> {

    fn lint_list_compound_assignment(
        &mut self,
        target: AssignTargetId,
        value: ArenaExprOrRun,
        span: Span,
    ) {
        let ArenaAssignTargetKind::Name(target_name) = self.arena.assign_target(target).kind else {
            return;
        };
        let Some((scope_depth, binding)) =
            self.scopes
                .iter()
                .enumerate()
                .rev()
                .find_map(|(depth, scope)| {
                    scope
                        .get(target_name.as_str().as_str())
                        .map(|binding| (depth, binding))
                })
        else {
            return;
        };
        if !binding.mutable {
            return;
        }
        let module_level = scope_depth == 0;
        let ArenaExprOrRun::Expr(value) = value else {
            return;
        };
        // `x.push(a).push(b)` is one update; walk from the outermost call to
        // the receiver, collecting each appended argument.
        let mut updates: Vec<ListUpdate> = Vec::new();
        let mut receiver = value;
        while let ArenaExprKind::Call { callee, args } = self.arena.expr(receiver).kind
            && let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind
            && (name == "push" || name == "extend")
            && args.len() == 1
            && let ArenaCallArgKind::Positional(argument) = self.arena.call_args(args)[0].kind
        {
            updates.push(ListUpdate {
                method: name,
                argument,
                parenthesized: Span::new(
                    span.source_id,
                    self.arena.expr(callee).span.end(),
                    self.arena.expr(receiver).span.end(),
                ),
            });
            receiver = base;
        }
        // Only `push` chains have a single `+=` spelling: `extend` operands
        // would need spreads whose grouping depends on the operand.
        if updates.is_empty()
            || (updates.len() > 1 && updates.iter().any(|update| update.method != "push"))
            || !matches!(self.arena.expr(receiver).kind, ArenaExprKind::Ident(name) if name == target_name)
            || !matches!(
                self.expr_types.get(&self.arena.expr(receiver).span),
                Some(Type::List(_))
            )
        {
            return;
        }
        updates.reverse();
        // An expression's own span can omit the input of a pipeline, so the
        // argument text is read back from between the call's parentheses.
        let Some(argument_sources) = updates
            .iter()
            .map(|update| {
                let call = self.source.get(update.parenthesized.range())?;
                let inner = call.trim().strip_prefix('(')?.strip_suffix(')')?.trim();
                Some(inner.strip_suffix(',').unwrap_or(inner).trim_end())
            })
            .collect::<Option<Vec<_>>>()
        else {
            return;
        };
        // The target must not change between the receiver read and the compound
        // assignment's read of the current value. Any proc may assign a
        // module-level variable, so only a call-free argument is known to
        // leave it alone.
        let arguments_preserve_target =
            updates
                .iter()
                .zip(&argument_sources)
                .all(|(update, argument_source)| {
                    if module_level {
                        list_update_argument_stable(self.arena, update.argument)
                    } else {
                        !expr_may_assign_local(
                            self.arena,
                            self.source,
                            update.argument,
                            target_name.as_str().as_str(),
                            argument_source,
                        )
                    }
                });
        if !arguments_preserve_target {
            return;
        }
        let multiline_argument = argument_sources.iter().any(|source| source.contains('\n'));
        let replacement = if updates[0].method == "push" {
            let one_line = format!("{target_name} += [{}]", argument_sources.join(", "));
            // The formatter breaks a list that overflows the line width into
            // one element per line, indented from the statement's line.
            let line_start = self.source[..span.start()].rfind('\n').map_or(0, |i| i + 1);
            let prefix = &self.source[line_start..span.start()];
            let indent: String = prefix.chars().take_while(|ch| ch.is_whitespace()).collect();
            if prefix.chars().count() + one_line.chars().count() > super::format::DEFAULT_LINE_WIDTH
                && !multiline_argument
            {
                let elements: String = argument_sources
                    .iter()
                    .map(|source| format!("{indent}  {source},\n"))
                    .collect();
                format!("{target_name} += [\n{elements}{indent}]")
            } else {
                one_line
            }
        } else {
            format!("{target_name} += {}", argument_sources[0])
        };
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "prefer list compound assignment for a local update",
        )
        .with_code(DiagnosticCode::LintPreferListCompoundAssignment)
        .with_label(Label::secondary(span, "append a list with `+=`"));
        let edit_span = Span::new(
            span.source_id,
            span.start(),
            self.arena.expr(value).span.end(),
        );
        // Replacing the statement would discard a comment between its tokens;
        // a `#` inside a string literal is not one. A multiline argument is
        // copied as written, so its continuation lines keep their
        // indentation and the formatter owns their final layout.
        if span_may_contain_comment(self.source, edit_span) {
            diagnostic =
                diagnostic.with_note("comments inside the update require a manual rewrite");
        } else {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                edit_span,
                "rewrite as list compound assignment",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_nested_value_pipeline(&mut self, value: ExprId) {
        let Some((_, args)) = self.pipeline_ordinary_call(value) else {
            return;
        };
        // Whole statement values need no new parentheses or precedence rules.
        let whole_values = self.whole_statement_values.get_or_init(|| {
            (0..self.arena.stmt_tags.len())
                .filter_map(|raw| {
                    #[cfg(test)]
                    nested_pipeline_index_tests::record_statement();
                    match self.arena.stmt(StmtId::from_index(raw)).kind {
                        ArenaStmtKind::Let {
                            initializer: ArenaExprOrRun::Expr(expr),
                            ..
                        }
                        | ArenaStmtKind::Var {
                            initializer: ArenaExprOrRun::Expr(expr),
                            ..
                        }
                        | ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr)))
                        | ArenaStmtKind::Expr(expr) => Some(expr),
                        _ => None,
                    }
                })
                .collect()
        });
        if !whole_values.contains(&value) {
            return;
        }
        let args = self.arena.call_args(args);
        let inputs = args
            .iter()
            .enumerate()
            .filter_map(|(index, arg)| {
                let expr = pipeline_argument_expr(arg).unwrap();
                self.pipeline_ordinary_call(expr).map(|_| (index, expr))
            })
            .collect::<Vec<_>>();
        let [(index, input)] = inputs.as_slice() else {
            return;
        };
        if args[..*index]
            .iter()
            .any(|arg| !self.pipeline_argument_stable(pipeline_argument_expr(arg).unwrap()))
        {
            return;
        }
        let span = self.arena.expr(value).span;
        let input_span = self.arena.expr(*input).span;
        let Some(original) = self.source.get(span.range()) else {
            return;
        };
        let Some(input_text) = self.source.get(input_span.range()) else {
            return;
        };
        let mut stage = original.to_string();
        stage.replace_range(
            input_span.start() - span.start()..input_span.end() - span.start(),
            "_",
        );
        let replacement = format!("{input_text} |> {stage}");
        if !self.pipeline_rewrite_preserves_types(
            span,
            &replacement,
            value,
            *input,
            span.start(),
            span.start(),
        ) {
            return;
        }
        let mut diagnostic =
            Diagnostic::new(Severity::Warning, "nested calls form a value pipeline")
                .with_code(DiagnosticCode::LintPreferValuePipeline)
                .with_label(Label::secondary(
                    span,
                    "place the retained input at an explicit argument hole",
                ));
        if !original.contains('#') {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "use an explicit value pipeline",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_nested_record_update(&mut self, expr: ExprId) {
        fn selected_path(arena: &AstArena, expr: ExprId, root: Name) -> Option<Vec<Name>> {
            match arena.expr(expr).kind {
                ArenaExprKind::Ident(name) if name == root => Some(Vec::new()),
                ArenaExprKind::Field { base, name } => {
                    let mut path = selected_path(arena, base, root)?;
                    path.push(name);
                    Some(path)
                }
                _ => None,
            }
        }
        fn replacements(
            arena: &AstArena,
            expr: ExprId,
            root: Name,
            prefix: &[Name],
            output: &mut Vec<(Vec<Name>, Option<ExprId>)>,
        ) -> Option<()> {
            let ArenaExprKind::Record(fields) = arena.expr(expr).kind else {
                return None;
            };
            let fields = arena.record_fields(fields);
            let ArenaRecordFieldKind::Spread { expr: spread, .. } = fields.first()?.kind else {
                return None;
            };
            if selected_path(arena, spread, root)?.as_slice() != prefix {
                return None;
            }
            for field in fields.iter().skip(1) {
                let (name, value) = match field.kind {
                    ArenaRecordFieldKind::Named { name, value, .. } => (name, Some(value)),
                    ArenaRecordFieldKind::Shorthand { name, .. } => (name, None),
                    _ => return None,
                };
                let mut path = prefix.to_vec();
                path.push(name);
                if let Some(value) = value
                    && matches!(arena.expr(value).kind, ArenaExprKind::Record(inner) if matches!(arena.record_fields(inner).first().map(|f| &f.kind), Some(ArenaRecordFieldKind::Spread { .. })))
                {
                    replacements(arena, value, root, &path, output)?;
                } else {
                    output.push((path, value));
                }
            }
            Some(())
        }
        let expression = self.arena.expr(expr);
        let ArenaExprKind::Record(fields) = expression.kind else {
            return;
        };
        let Some(ArenaRecordFieldKind::Spread { expr: base, .. }) = self
            .arena
            .record_fields(fields)
            .first()
            .map(|field| &field.kind)
        else {
            return;
        };
        let ArenaExprKind::Ident(root) = self.arena.expr(*base).kind else {
            return;
        };
        let stable = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(root.as_str().as_str()))
            .is_some_and(|binding| !binding.mutable);
        if !stable || self.assigned_names.contains(&root) {
            return;
        }
        let Some(base_ty @ Type::Record(shape)) = self.expr_types.get(&self.arena.expr(*base).span)
        else {
            return;
        };
        if shape.is_empty() {
            return;
        }
        let original = &self.source[expression.span.range()];
        if original.contains('#') {
            return;
        }
        let mut updates = Vec::new();
        if replacements(self.arena, expr, root, &[], &mut updates).is_none()
            || !updates.iter().any(|(path, _)| path.len() > 1)
        {
            return;
        }
        let mut entries = Vec::new();
        for (index, (path, value)) in updates.iter().enumerate() {
            if updates[..index]
                .iter()
                .any(|(prior, _)| path.starts_with(prior) || prior.starts_with(path))
            {
                return;
            }
            let mut selected = base_ty;
            for name in path {
                let label = name.as_str();
                if !label
                    .chars()
                    .next()
                    .is_some_and(|c| c == '_' || c.is_alphabetic())
                    || !label.chars().all(|c| c == '_' || c.is_alphanumeric())
                {
                    return;
                }
                let Type::Record(fields) = selected else {
                    return;
                };
                let Some(field_ty) = fields.get(name) else {
                    return;
                };
                selected = field_ty;
            }
            if let Some(value) = value {
                let Some(actual) = self.expr_types.get(&self.arena.expr(*value).span) else {
                    return;
                };
                if actual.contains_any()
                    || (actual.is_dynamic() && !selected.is_dynamic())
                    || !actual.matches_expected(selected)
                {
                    return;
                }
            }
            let path_text = path
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(".");
            let value_text = value
                .map(|value| &self.source[self.arena.expr(value).span.range()])
                .map(str::to_string)
                .unwrap_or_else(|| path.last().unwrap().to_string());
            entries.push(format!("{path_text}: {value_text}"));
        }
        let replacement = format!("{{...{root}, {}}}", entries.join(", "));
        self.diagnostics.push(
            Diagnostic::warning("nested record spreads can use disjoint update paths")
                .with_code(DiagnosticCode::LintPreferNestedRecordUpdate)
                .with_label(Label::secondary(
                    expression.span,
                    "the repeated record reads are stable and statically known",
                ))
                .with_fix_hint(FixHint::replacement(
                    expression.span,
                    "replace nested spreads with static field paths",
                    replacement,
                )),
        );
    }

    fn lint_stage_callable_wrapper(&mut self, stage: &ArenaStreamStage) {
        if !xsh_registry::stream_parameters::stage_accepts_callable(stage.kind.as_str()) {
            return;
        }
        let Some(block_id) = stage.block else {
            return;
        };
        let block = self.arena.block(block_id);
        let [parameter] = self.arena.block_params(block.params) else {
            return;
        };
        let statements = self.arena.stmt_ids(block.statements).collect::<Vec<_>>();
        let [statement] = statements.as_slice() else {
            return;
        };
        let ArenaStmtKind::Expr(call) = self.arena.stmt(*statement).kind else {
            return;
        };
        let expression = self.arena.expr(call);
        let ArenaExprKind::Call { callee, args } = expression.kind else {
            return;
        };
        if !self
            .statically_resolved_call_spans
            .contains(&expression.span)
        {
            return;
        }
        let [argument] = self.arena.call_args(args) else {
            return;
        };
        let ArenaCallArgKind::Positional(item) = argument.kind else {
            return;
        };
        if !matches!(self.arena.expr(item).kind, ArenaExprKind::Ident(name) if name == parameter.name)
        {
            return;
        }
        let span = self.arena.span(stage.span);
        if self.source[span.range()].contains('#') {
            return;
        }
        let block_span = self.arena.span(block.span);
        let name = &self.source[self.arena.expr(callee).span.range()];
        let arguments = self.arena.call_args(stage.args);
        let replacement = if arguments.is_empty() {
            format!("{}({name})", stage.kind.as_str())
        } else {
            // Existing configuration remains in its written order. Appending a
            // descriptor adds no stage-entry evaluation or runtime binding.
            let prefix = self.source[span.start()..block_span.start()].trim_end();
            let Some(prefix) = prefix.strip_suffix(')') else {
                return;
            };
            format!("{prefix}, block: {name})")
        };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "transparent stream block can name its callable directly",
            )
            .with_code(DiagnosticCode::LintStageCallable)
            .with_label(Label::secondary(span, "use the same checked one-item call"))
            .with_fix_hint(FixHint::replacement(
                span,
                "use named stage callable",
                replacement,
            )),
        );
    }

    fn lint_generic_alias_constructor(
        &mut self,
        expression: ExprId,
        callee: ExprId,
        args: ArenaRange,
    ) {
        let Some(alias) = self
            .record_constructors
            .constructor_definition(self.arena, callee, None)
        else {
            return;
        };
        let Some(definition) = self
            .record_constructors
            .resolve_call(self.arena, callee, None)
        else {
            return;
        };
        if alias == definition
            || !self.arena.type_def(alias).type_parameters.is_empty()
            || self.arena.type_def(definition).type_parameters.is_empty()
            || self.record_constructors.namespace(definition).is_some()
        {
            return;
        }
        let name = self.arena.type_def(definition).name;
        if self.record_constructors.definition(None, name) != Some(definition) {
            return;
        }
        let call = self.arena.expr(expression);
        let Some(original_type) = self.expr_types.get(&call.span) else {
            return;
        };
        if original_type.contains_any()
            || original_type.contains_inference()
            || original_type.is_recovery()
        {
            return;
        }
        let Type::Record(fields) = original_type else {
            return;
        };
        let mut safe = !self.source[call.span.range()].contains('#');
        for argument in self.arena.call_args(args) {
            let ArenaCallArgKind::Named { name, value, .. } = argument.kind else {
                safe = false;
                break;
            };
            let Some(expected) = fields.get(&name) else {
                safe = false;
                break;
            };
            let actual = xsh::frontend::check::LiteralConstant::analyze(
                self.arena,
                value,
                &FxHashMap::default(),
            )
            .map(|value| value.value_type())
            .or_else(|| self.expr_types.get(&self.arena.expr(value).span).cloned());
            safe &= actual.as_ref().is_some_and(|actual| {
                !matches!(actual, Type::Null)
                    && (actual == expected
                        || matches!(expected, Type::Optional(inner) if actual == inner.as_ref()))
            });
        }
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "constructor fields can infer the same concrete schema",
        )
        .with_code(DiagnosticCode::LintPreferGenericRecordConstructor)
        .with_label(Label::secondary(
            call.span,
            "use the parameterized schema constructor",
        ));
        if safe {
            let callee_span = self.arena.expr(callee).span;
            let replacement = name.to_string();
            let mut candidate = self.source.to_string();
            candidate.replace_range(callee_span.range(), &replacement);
            let call_span = Span::new(
                call.span.source_id,
                call.span.start(),
                call.span.end() - callee_span.range().len() + replacement.len(),
            );
            let shape = checked_return_type_shape(original_type);
            let selected = self
                .record_constructors
                .instance_expectation(self.arena, alias, &[])
                .ok()
                .and_then(|context| {
                    context
                        .instances
                        .into_iter()
                        .find(|instance| instance.definition == definition)
                })
                .map(|instance| {
                    instance
                        .arguments
                        .iter()
                        .map(checked_return_type_shape)
                        .collect::<Vec<_>>()
                });
            let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
                call.span.source_id,
                &candidate,
            );
            if parsed.diagnostics.is_empty() {
                let checked = xsh::frontend::check::Checker::check_arena(&parsed.arena, &candidate);
                let equivalent = checked.diagnostics.is_empty()
                    && parsed.arena.symbol_owner().with_current(|| {
                        checked
                            .record_constructor_instances
                            .get(&call_span)
                            .is_some_and(|fact| {
                                checked_return_type_shape(&fact.ty) == shape
                                    && Some(
                                        fact.instance
                                            .arguments
                                            .iter()
                                            .map(checked_return_type_shape)
                                            .collect::<Vec<_>>(),
                                    ) == selected
                            })
                    });
                if equivalent {
                    diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                        callee_span,
                        "infer the same concrete constructor instance",
                        replacement,
                    ));
                }
            }
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_record_constructor(&mut self, ty: Option<TypeExprId>, initializer: &ArenaExprOrRun) {
        let ArenaExprOrRun::Expr(expr) = initializer else {
            return;
        };
        if ty.is_none() {
            if let ArenaExprKind::Call { callee, args } = self.arena.expr(*expr).kind {
                self.lint_generic_alias_constructor(*expr, callee, args);
            }
            return;
        }
        let Some(ty) = ty else {
            return;
        };
        let Some(definition) = self
            .record_constructors
            .resolve_annotation(self.arena, ty, None)
        else {
            return;
        };
        let value = self.arena.expr(*expr);
        let ArenaExprKind::Record(fields) = value.kind else {
            return;
        };
        if !matches!(self.expr_types.get(&value.span), Some(Type::Record(_))) {
            return;
        }
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "schema record can use its named constructor",
        )
        .with_code(DiagnosticCode::LintPreferRecordConstructor)
        .with_label(Label::secondary(
            value.span,
            "construct the declared schema",
        ));
        let mut arguments = Vec::new();
        let mut safe = !self.source[value.span.range()].contains('#');
        let Type::Record(schema_fields) =
            self.record_constructors.resolve_type(self.arena, ty, None)
        else {
            return;
        };
        let Some(Type::Record(checked_fields)) = self.expr_types.get(&value.span) else {
            return;
        };
        safe &= checked_fields.keys().eq(schema_fields.keys())
            && checked_fields
                .values()
                .all(|ty| !matches!(ty, Type::Unknown | Type::Invalid));
        let mut supplied = FxHashSet::default();
        let defaults = self.record_constructors.defaults(definition);
        for field in self.arena.record_fields(fields) {
            match field.kind {
                ArenaRecordFieldKind::Shorthand { name, .. } => {
                    safe &= supplied.insert(name);
                    arguments.push(format!("{name}:"));
                }
                ArenaRecordFieldKind::Named { name, value, .. } => {
                    safe &= supplied.insert(name);
                    if matches!(self.arena.expr(value).kind, ArenaExprKind::Run(_)) {
                        safe = false;
                    }
                    let constant = xsh::frontend::check::LiteralConstant::analyze(
                        self.arena,
                        value,
                        &FxHashMap::default(),
                    );
                    if let Some(constant) = &constant
                        && let Some(expected) = schema_fields.get(&name)
                        && constant.clone().in_type(expected) != *constant
                    {
                        safe = false;
                    }
                    if constant.as_ref().is_some_and(|constant| {
                        defaults.and_then(|values| values.get(&name)) == Some(constant)
                    }) {
                        continue;
                    }
                    // Each expression remains in its original field order,
                    // with the binding annotation retaining schema validation.
                    let span = self.arena.expr(value).span;
                    let text = &self.source[span.range()];
                    if text.contains('#') {
                        safe = false;
                    }
                    if matches!(self.arena.expr(value).kind, ArenaExprKind::Ident(identifier) if identifier == name)
                    {
                        arguments.push(format!("{name}:"));
                    } else {
                        arguments.push(format!("{name}: {text}"));
                    }
                }
                ArenaRecordFieldKind::Spread { .. }
                | ArenaRecordFieldKind::Computed { .. }
                | ArenaRecordFieldKind::Path { .. } => safe = false,
            }
        }
        if safe {
            let constructor = if self.arena.type_expr_tags[ty.index()] == ArenaTypeExprTag::Applied
            {
                TypeExprId::from_index(self.arena.type_expr_data[ty.index()].lhs as usize)
            } else {
                ty
            };
            let name = &self.source[self.arena.type_expr_span(constructor).range()];
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                value.span,
                "use the schema constructor",
                format!("{name}({})", arguments.join(", ")),
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    /// A qualified variant constructor whose expected type already selects
    /// the same declaration, so `.Name` builds the identical value. The
    /// checker publishes the qualifier only outside stream stage blocks,
    /// where `.Name` is not an item read.
    fn lint_inferred_variant(&mut self, expr: ExprId) {
        let span = self.arena.expr(expr).span;
        let Some(qualifier) = self.redundant_variant_qualifiers.get(&span).copied() else {
            return;
        };
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "the expected type already selects this variant",
        )
        .with_code(DiagnosticCode::LintPreferInferredVariant)
        .with_label(Label::secondary(
            qualifier,
            "qualifier the expected type makes redundant",
        ));
        let qualified = self.source.get(qualifier.start()..=qualifier.end());
        let plain = qualified.is_some_and(|text| {
            text.ends_with('.')
                && text[..text.len() - 1]
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
        });
        if plain && !leading_dot_continues_previous_line(self.source, qualifier.start()) {
            diagnostic = diagnostic.with_fix_hint(FixHint::deletion(
                qualifier,
                "select the variant from the expected type",
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    /// A schema constructor whose named arguments already follow the
    /// declaration order, filling fields no single value fits two of. The
    /// positional call binds every argument to the same field and evaluates
    /// them in the same order. A pun (`name:`) already names its field and
    /// value at once, so a call with one stays as written.
    fn lint_positional_constructor(&mut self, expr: ExprId) {
        let call = self.arena.expr(expr);
        let ArenaExprKind::Call { callee, args } = call.kind else {
            return;
        };
        let Some(definition) = self
            .record_constructors
            .resolve_call(self.arena, callee, None)
        else {
            return;
        };
        let args = self.arena.call_args(args);
        let declared = self
            .record_constructors
            .declared_fields(self.arena, definition);
        let in_order = !args.is_empty()
            && args.len() <= declared.len()
            && args.iter().zip(&declared).all(|(arg, (field, _))| {
                matches!(arg.kind, ArenaCallArgKind::Named { name, value, span }
                    if name == *field
                        && self.arena.expr(value).span.start() != self.arena.span(span).start())
            });
        if !in_order
            || self
                .record_constructors
                .positional_conflict(self.arena, definition, args.len())
                .is_some()
            || !matches!(self.expr_types.get(&call.span), Some(Type::Record(_)))
        {
            return;
        }
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "constructor fields can be passed positionally",
        )
        .with_code(DiagnosticCode::LintPreferPositionalConstructor)
        .with_label(Label::secondary(
            call.span,
            "no two of these fields can hold the same value",
        ));
        let first = args.first().map(|arg| self.arena.span(named_arg_span(arg)));
        let last = args.last().map(|arg| self.arena.span(named_arg_span(arg)));
        if let (Some(first), Some(last)) = (first, last)
            && !self.source[first.start()..last.end()].contains('#')
        {
            let mut replacement = String::new();
            let mut cursor = first.start();
            for arg in args {
                let ArenaCallArgKind::Named { value, span, .. } = arg.kind else {
                    return;
                };
                let span = self.arena.span(span);
                let value = self.arena.expr(value).span;
                replacement.push_str(&self.source[cursor..span.start()]);
                replacement.push_str(&self.source[value.start()..span.end()]);
                cursor = span.end();
            }
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                Span::new(first.source_id, first.start(), last.end()),
                "pass the fields in declaration order",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_named_argument_pun(&mut self, arg: &ArenaCallArg) {
        let ArenaCallArgKind::Named { name, value, span } = arg.kind else {
            return;
        };
        let value = self.arena.expr(value);
        let span = self.arena.span(span);
        if value.span.start() == span.start()
            || !matches!(value.kind, ArenaExprKind::Ident(identifier) if identifier == name)
            || !matches!(
                self.expr_types.get(&value.span),
                Some(ty) if !matches!(ty, Type::Unknown | Type::Invalid)
            )
        {
            return;
        }
        // Replacing the explicit identifier introduces no scope or evaluation
        // boundary, so lexical resolution selects the same binding.
        let mut diagnostic =
            Diagnostic::new(Severity::Warning, "named argument repeats its value's name")
                .with_code(DiagnosticCode::LintPreferNamedArgumentPun)
                .with_label(Label::secondary(
                    span,
                    "use the lexical named-argument shorthand",
                ));
        if !self.source[span.range()].contains('#') {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "omit the repeated value name",
                format!("{name}:"),
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_named_argument_forwarding(&mut self, args: ArenaRange, call_span: Span) {
        if self
            .expr_types
            .get(&call_span)
            .is_none_or(|ty| matches!(ty, Type::Unknown | Type::Invalid))
        {
            return;
        }
        let args = self.arena.call_args(args).to_vec();
        let mut start = 0;
        while start < args.len() {
            let ArenaCallArgKind::Named { name, value, span } = args[start].kind else {
                start += 1;
                continue;
            };
            let ArenaExprKind::Field { base, name: field } = self.arena.expr(value).kind else {
                start += 1;
                continue;
            };
            if field != name {
                start += 1;
                continue;
            }
            let ArenaExprKind::Ident(receiver) = self.arena.expr(base).kind else {
                start += 1;
                continue;
            };
            let Some(Type::Record(fields)) =
                self.expr_types.get(&self.arena.expr(base).span).cloned()
            else {
                start += 1;
                continue;
            };
            if fields.len() < 2 {
                start += 1;
                continue;
            }
            let mut supplied = FxHashSet::default();
            let mut end = start;
            let mut edit_end = self.arena.span(span).end();
            let mut exact_types = true;
            while let Some(arg) = args.get(end) {
                let ArenaCallArgKind::Named { name, value, span } = arg.kind else {
                    break;
                };
                let ArenaExprKind::Field { base, name: field } = self.arena.expr(value).kind else {
                    break;
                };
                if name != field
                    || !matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(found) if found == receiver)
                {
                    break;
                }
                if !supplied.insert(name) {
                    exact_types = false;
                }
                exact_types &=
                    self.expr_types.get(&self.arena.expr(value).span) == fields.get(&field);
                edit_end = self.arena.span(span).end();
                end += 1;
            }
            let edit_span = Span::new(call_span.source_id, self.arena.span(span).start(), edit_end);
            let stable = !self.assigned_names.contains(&receiver)
                && self
                    .scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(receiver.as_str().as_str()))
                    .is_some_and(|binding| !binding.mutable);
            if exact_types
                && supplied.len() == fields.len()
                && fields.keys().all(|field| supplied.contains(field))
            {
                let mut diagnostic = Diagnostic::new(
                    Severity::Warning,
                    "record fields can forward through a named argument spread",
                )
                .with_code(DiagnosticCode::LintPreferNamedArgumentSpread)
                .with_label(Label::secondary(
                    edit_span,
                    "forward exactly the checked visible fields",
                ));
                if stable && !self.source[edit_span.range()].contains('#') {
                    let replacement = format!("...{receiver}");
                    let mut candidate = self.source.to_string();
                    candidate.replace_range(edit_span.range(), &replacement);
                    let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
                        call_span.source_id,
                        &candidate,
                    );
                    if parsed.diagnostics.is_empty()
                        && xsh::frontend::check::Checker::check_arena(&parsed.arena, &candidate)
                            .diagnostics
                            .is_empty()
                    {
                        diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                            edit_span,
                            "spread the checked record fields",
                            replacement,
                        ));
                    }
                }
                self.diagnostics.push(diagnostic);
            }
            start = end.max(start + 1);
        }
    }

    fn lint_lookup_sentinel(&mut self, expr: ExprId) {
        let node = self.arena.expr(expr);
        let ArenaExprKind::Binary {
            op: BinaryOp::Eq | BinaryOp::Ne,
            left,
            right,
        } = node.kind
        else {
            return;
        };
        let sentinel = if self.proven_absence_lookup(left) && self.is_negative_one_literal(right) {
            right
        } else if self.proven_absence_lookup(right) && self.is_negative_one_literal(left) {
            left
        } else {
            return;
        };
        if self.source[node.span.range()].contains('#') {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "lookup absence is null rather than a numeric sentinel",
            )
            .with_code(DiagnosticCode::LintLookupAbsence)
            .with_label(Label::secondary(
                node.span,
                "compare the proved lookup result with null",
            ))
            .with_fix_hint(FixHint::replacement(
                self.arena.expr(sentinel).span,
                "use absence",
                "null",
            )),
        );
    }

    /// Removing an eager argument is safe only when that argument cannot fail
    /// or observe or change state. Effectful fallbacks require authored snapshots.
    fn lint_removed_lookup_fallback(&mut self, callee: ExprId, args: ArenaRange, span: Span) {
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        let receiver = self.arena.expr(base);
        let Some(ty) = self.expr_types.get(&receiver.span) else {
            return;
        };
        let expected = match (ty, name.as_str().as_str()) {
            (Type::List(item) | Type::Map(_, item), "get") => item.as_ref(),
            (Type::Str | Type::Bytes, "byte_at") => &Type::Int,
            _ => return,
        };
        let [index, fallback] = self.arena.call_args(args) else {
            return;
        };
        let diagnostic = Diagnostic::new(
            Severity::Warning,
            "lookup fallback arguments have been removed",
        )
        .with_code(DiagnosticCode::LintLookupFallback)
        .with_label(Label::secondary(
            span,
            "use the ordinary fallback expression",
        ));
        let (ArenaCallArgKind::Positional(index), ArenaCallArgKind::Positional(fallback)) =
            (&index.kind, &fallback.kind)
        else {
            self.diagnostics.push(diagnostic.with_note(
                "no automatic fix: named argument evaluation order is not proven equivalent",
            ));
            return;
        };
        let (index, fallback) = (*index, *fallback);
        let fallback_node = self.arena.expr(fallback);
        let immutable_read = matches!(fallback_node.kind, ArenaExprKind::Ident(name)
            if self.scopes.iter().rev().find_map(|scope| scope.get(name.as_str().as_str()))
                .is_some_and(|binding| !binding.mutable));
        let fallback_type = match fallback_node.kind {
            ArenaExprKind::Int(_)
            | ArenaExprKind::Unary {
                op: UnaryOp::Neg, ..
            } => Some(Type::Int),
            ArenaExprKind::Bool(_) => Some(Type::Bool),
            ArenaExprKind::Null => Some(Type::Null),
            ArenaExprKind::Str(_) => Some(Type::Str),
            ArenaExprKind::Bytes(_) => Some(Type::Bytes),
            ArenaExprKind::PathStr(_) => Some(Type::Path),
            ArenaExprKind::Float(_) => Some(Type::Float),
            ArenaExprKind::Duration(_) => Some(Type::Duration),
            ArenaExprKind::List(items) if items.is_empty() => Some(expected.clone()),
            ArenaExprKind::Ident(_) if immutable_read => {
                self.expr_types.get(&fallback_node.span).cloned()
            }
            _ => None,
        };
        let inert = matches!(
            fallback_node.kind,
            ArenaExprKind::Int(_)
                | ArenaExprKind::Bool(_)
                | ArenaExprKind::Null
                | ArenaExprKind::Str(_)
                | ArenaExprKind::Bytes(_)
                | ArenaExprKind::PathStr(_)
                | ArenaExprKind::Float(_)
                | ArenaExprKind::Duration(_)
        ) || matches!(fallback_node.kind, ArenaExprKind::Unary { op: UnaryOp::Neg, expr } if matches!(self.arena.expr(expr).kind, ArenaExprKind::Int(value) if self.arena.int_literal(value).value().and_then(i64::checked_neg).is_some()))
            || matches!(fallback_node.kind, ArenaExprKind::List(items) if items.is_empty() && matches!(expected, Type::List(_)));
        let safe = (inert || immutable_read)
            && fallback_type.is_some_and(|ty| ty.matches_expected(expected))
            && !self.source[span.range()].contains('#');
        self.diagnostics.push(if safe {
            let replacement = format!("(({}).{}({}) ?? ({}))", &self.source[receiver.span.range()], name,
                &self.source[self.arena.expr(index).span.range()], &self.source[fallback_node.span.range()]);
            diagnostic.with_fix_hint(FixHint::replacement(span, "use a lazy inert fallback", replacement))
        } else { diagnostic.with_note("no automatic fix: eager fallback effects, failure, comments, or argument order are not proven equivalent") });
    }

    fn lint_call_style(&mut self, callee: ExprId, args: ArenaRange, span: Span) {
        self.lint_named_argument_forwarding(args, span);
        self.lint_path_constructor(callee, args, span);
        self.lint_duration_conversion(callee, args, span);
        self.lint_removed_lookup_fallback(callee, args, span);
        self.lint_redundant_defaults(callee, args);
        self.lint_prefer_in(callee, args, span);
        self.lint_fs_root_receiver(callee, args, span);
        self.lint_prefer_method(callee, args, span);
        self.lint_join_to_concat(callee, args, span);
        self.lint_prefer_slice(callee, args, span);
    }

    fn bare_field_label(&self, text: &str) -> bool {
        let lexed =
            xsh::frontend::syntax::lexer::Lexer::new(xsh::frontend::source::SourceId::new(0), text)
                .lex_compact();
        lexed.diagnostics.is_empty()
            && lexed
                .token_table
                .label_text_at(0)
                .is_some_and(|name| name.as_ref() == text)
            && lexed.token_table.tag_at(1) == Some(xsh::frontend::syntax::token::TokenTag::Eof)
    }

    fn lint_known_field_access(&mut self, expr: ExprId) {
        let outer = self.arena.expr(expr);
        let ArenaExprKind::Try(call) = outer.kind else {
            return;
        };
        let expression = self.arena.expr(call);
        let ArenaExprKind::Call { callee, args } = expression.kind else {
            return;
        };
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if name != "get" || !self.proven_materialized_record(base) {
            return;
        }
        let [argument] = self.arena.call_args(args) else {
            return;
        };
        let key = match argument.kind {
            ArenaCallArgKind::Positional(key) => key,
            ArenaCallArgKind::Named { name, value, .. } if name == "field" => value,
            _ => return,
        };
        let ArenaExprKind::Str(key) = self.arena.expr(key).kind else {
            return;
        };
        let field = self.arena.string_literal(key);
        if !self.bare_field_label(field) {
            return;
        }
        let Some(Type::Record(fields)) = self.expr_types.get(&self.arena.expr(base).span) else {
            return;
        };
        if fields.contains_key(&Name::intern("get")) {
            return;
        }
        let Some(expected) = fields.get(&Name::intern(field)) else {
            return;
        };
        let Some(Type::Result(success, _)) = self.expr_types.get(&expression.span) else {
            return;
        };
        if success.as_ref() != expected || self.expr_types.get(&outer.span) != Some(expected) {
            return;
        }
        let span = self.expression_source_span(outer.span);
        let receiver_span = self.expression_source_span(self.arena.expr(base).span);
        let cst = self
            .source_cst
            .get()
            .expect("expression span initializes CST");
        if cst.contains_comment(span) {
            return;
        }
        let receiver = &self.source[receiver_span.range()];
        let receiver = if matches!(self.arena.expr(base).kind, ArenaExprKind::Record(_)) {
            format!("({receiver})")
        } else {
            receiver.to_owned()
        };
        self.diagnostics.push(
            Diagnostic::warning(
                "a guaranteed field on an ordinary record can be selected directly",
            )
            .with_code(DiagnosticCode::LintPreferKnownFieldAccess)
            .with_label(Label::secondary(
                span,
                "the selected value retains its exact checked type",
            ))
            .with_fix_hint(FixHint::replacement(
                span,
                "select the known record field",
                format!("{receiver}.{field}"),
            )),
        );
    }

    fn lint_quoted_field_labels(&mut self, fields: ArenaRange) {
        for field in self.arena.record_fields(fields) {
            let ArenaRecordFieldKind::Named { name, span, .. } = field.kind else {
                continue;
            };
            if !self.bare_field_label(name.as_str().as_str()) {
                continue;
            }
            let span = self.arena.span(span);
            let Some(source) = self.source.get(span.range()) else {
                continue;
            };
            let lexed =
                xsh::frontend::syntax::lexer::Lexer::new(span.source_id, source).lex_compact();
            if !lexed.diagnostics.is_empty()
                || lexed.token_table.tag_at(0)
                    != Some(xsh::frontend::syntax::token::TokenTag::String)
                || lexed.token_table.tag_at(1)
                    != Some(xsh::frontend::syntax::token::TokenTag::Colon)
            {
                continue;
            }
            let key = lexed
                .token_table
                .span_at(0, span.source_id, source)
                .unwrap();
            let key = Span::new(
                span.source_id,
                span.start() + key.start(),
                span.start() + key.end(),
            );
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    "identifier-shaped field labels can be bare",
                )
                .with_code(DiagnosticCode::LintPreferBareFieldLabel)
                .with_label(Label::secondary(
                    key,
                    "quoting does not change this explicit key",
                ))
                .with_fix_hint(FixHint::replacement(
                    key,
                    "use the exact bare label",
                    name.as_str().to_string(),
                )),
            );
        }
    }

    fn lint_prepared_regex(&mut self, expr: ExprId) {
        if self.regex_recovery_context {
            return;
        }
        let outer = self.arena.expr(expr);
        let ArenaExprKind::Try(inner) = outer.kind else {
            return;
        };
        let call = self.arena.expr(inner);
        let ArenaExprKind::Call { callee, args } = call.kind else {
            return;
        };
        if !is_module_call(self.arena, callee, "regex", "compile")
            || self.expr_types.get(&outer.span) != Some(&Type::Regex)
            || args.len() != 1
        {
            return;
        }
        let argument = match self.arena.call_args(args)[0].kind {
            ArenaCallArgKind::Positional(value) => value,
            ArenaCallArgKind::Named { name, value, .. } if name == "pattern" => value,
            _ => return,
        };
        let value = self.arena.expr(argument);
        let ArenaExprKind::Str(text) = value.kind else {
            return;
        };
        let pattern = self.arena.string_literal(text);
        // Reparse the proposed raw spelling to prove delimiter and decoded-text identity.
        let replacement = if !pattern.contains('"') && !pattern.contains(['\n', '\r']) {
            format!("rx\"{pattern}\"")
        } else {
            format!("rx\"\"\"{pattern}\"\"\"")
        };
        let candidate = format!("let prepared = {replacement}\n");
        let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
            outer.span.source_id,
            &candidate,
        );
        if !parsed.diagnostics.is_empty()
            || parsed.arena.arena.regex_literals.len() != 1
            || parsed.arena.arena.regex_literals[0].pattern.as_ref() != pattern.as_ref()
            || !xsh::frontend::check::Checker::check_arena(&parsed.arena, &candidate)
                .diagnostics
                .is_empty()
        {
            return;
        }
        let comments = self.source[outer.span.start()..value.span.start()].contains('#')
            || self.source[value.span.end()..outer.span.end()].contains('#');
        let diagnostic = Diagnostic::new(
            Severity::Warning,
            "prepare static regex patterns with a literal",
        )
        .with_code(DiagnosticCode::LintPreferRegexLiteral)
        .with_label(Label::secondary(
            outer.span,
            "this validated pattern is directly propagated",
        ));
        self.diagnostics.push(if comments {
            diagnostic.with_note("no automatic fix: the call contains comments")
        } else {
            diagnostic.with_fix_hint(FixHint::replacement(
                outer.span,
                "use a prepared regex literal",
                replacement,
            ))
        });
    }

    /// `env.get("NAME")` and `env.Str.NAME` read exactly what `e"NAME"`
    /// reads: the same lookup, the same `Result[Str]`, the same failures. Only
    /// literal identifier names are rewritten. `env.get_or` is left alone:
    /// it fails on a value that is not UTF-8, while `e"NAME" ?? fallback`
    /// would fall back, and it returns a `Result` where `??` returns `Str`.
    fn lint_env_string(&mut self, expr: ExprId) {
        if !self.prefer_env_string {
            return;
        }
        let outer = self.arena.expr(expr);
        let name = match outer.kind {
            ArenaExprKind::Call { callee, args }
                if is_module_call(self.arena, callee, "env", "get") && args.len() == 1 =>
            {
                let argument = match self.arena.call_args(args)[0].kind {
                    ArenaCallArgKind::Positional(value) => value,
                    ArenaCallArgKind::Named { name, value, .. } if name == "name" => value,
                    _ => return,
                };
                let ArenaExprKind::Str(text) = self.arena.expr(argument).kind else {
                    return;
                };
                self.arena.string_literal(text).to_string()
            }
            ArenaExprKind::Field { base, name } => {
                let ArenaExprKind::Field {
                    base: module,
                    name: kind,
                } = self.arena.expr(base).kind
                else {
                    return;
                };
                if kind != "Str"
                    || !matches!(self.arena.expr(module).kind, ArenaExprKind::Ident(module) if module == "env")
                {
                    return;
                }
                name.as_str().to_string()
            }
            _ => return,
        };
        // The checked type proves `env` is the module, and the spelling
        // check keeps `$env.Str.NAME` command words and comments untouched.
        let text = &self.source[outer.span.range()];
        if !xsh::frontend::syntax::literal::is_env_string_name(&name)
            || self.scopes.iter().any(|scope| scope.contains_key("env"))
            || self.expr_types.get(&outer.span)
                != Some(&Type::Result(Box::new(Type::Str), Box::new(Type::Error)))
            || !text.starts_with("env.")
            || text.contains('#')
            || self.source[..outer.span.start()].ends_with('$')
        {
            return;
        }
        let replacement = format!("e\"{name}\"");
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "read an environment variable with a literal name as an e-string",
            )
            .with_code(DiagnosticCode::LintPreferEnvString)
            .with_label(Label::secondary(
                outer.span,
                "this reads one named variable",
            ))
            .with_fix_hint(FixHint::replacement(
                outer.span,
                format!("write `{replacement}`"),
                replacement,
            )),
        );
    }

    fn lint_prefer_slice(&mut self, callee: ExprId, args: ArenaRange, span: Span) {
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if name != "slice" || self.expr_types.get(&self.arena.expr(base).span) != Some(&Type::Bytes)
        {
            return;
        }
        let diagnostic = Diagnostic::new(
            Severity::Warning,
            "prefer half-open slicing where bounds are equivalent",
        )
        .with_code(DiagnosticCode::LintPreferSlice)
        .with_label(Label::secondary(
            span,
            "offset/count methods have distinct bounds and error behavior",
        ));
        let diagnostic = if let Some(replacement) = self.byte_slice_replacement(callee, args, span)
        {
            diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "rewrite equivalent byte slice",
                replacement,
            ))
        } else {
            diagnostic.with_note("no automatic fix: bounds, count arithmetic, evaluation order, or comment retention are not proven equivalent")
        };
        self.diagnostics.push(diagnostic);
    }

    fn literal_byte_slice_bounds(
        &self,
        base: ExprId,
        offset: i64,
        length: Option<ExprId>,
    ) -> Option<String> {
        let size = i64::try_from(self.proven_immutable_byte_length(base)?).ok()?;
        if offset < 0 || offset > size {
            return None;
        }
        let Some(length) = length else {
            return Some(format!("{offset}.."));
        };
        match self.arena.expr(length).kind {
            ArenaExprKind::Int(value) => self
                .arena
                .int_literal(value)
                .value()
                .filter(|count| *count >= 0)
                .map(|count| format!("{offset}..{}", offset.saturating_add(count).min(size))),
            ArenaExprKind::Binary {
                op: BinaryOp::Sub,
                left,
                right,
            } if matches!(self.arena.expr(right).kind, ArenaExprKind::Int(value) if self.arena.int_literal(value).value() == Some(offset)) =>
            {
                let ArenaExprKind::Call { callee, args } = self.arena.expr(left).kind else {
                    return None;
                };
                let ArenaExprKind::Field {
                    base: length_base,
                    name,
                } = self.arena.expr(callee).kind
                else {
                    return None;
                };
                // The length read is removable only for the same immutable
                // Bytes value, with both offset and subtraction in range.
                (args.is_empty() && name == "len"
                    && matches!((self.arena.expr(base).kind, self.arena.expr(length_base).kind),
                        (ArenaExprKind::Ident(source), ArenaExprKind::Ident(length_source)) if source == length_source))
                    .then(|| format!("{offset}.."))
            }
            _ => None,
        }
    }

    fn byte_slice_replacement(
        &self,
        callee: ExprId,
        args: ArenaRange,
        span: Span,
    ) -> Option<String> {
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return None;
        };
        let receiver = self.arena.expr(base);
        if name != "slice" || self.expr_types.get(&receiver.span) != Some(&Type::Bytes) {
            return None;
        }
        if args.len() > 2 {
            return None;
        }
        let mut offset = None;
        let mut length = None;
        for (index, arg) in self.arena.call_args(args).iter().enumerate() {
            match arg.kind {
                ArenaCallArgKind::Positional(value) if index == 0 && offset.is_none() => {
                    offset = Some(value);
                }
                ArenaCallArgKind::Positional(value) if index == 1 && length.is_none() => {
                    length = Some(value);
                }
                ArenaCallArgKind::Named { name, value, .. }
                    if name == "offset" && offset.is_none() =>
                {
                    offset = Some(value);
                }
                ArenaCallArgKind::Named { name, value, .. }
                    if name == "length" && length.is_none() =>
                {
                    length = Some(value);
                }
                _ => return None,
            }
        }
        let offset = offset?;
        let constant_offset = match self.arena.expr(offset).kind {
            ArenaExprKind::Int(value) => self
                .arena
                .int_literal(value)
                .value()
                .filter(|value| *value >= 0),
            _ => None,
        };
        let postfix_receiver = matches!(
            receiver.kind,
            ArenaExprKind::Ident(_)
                | ArenaExprKind::Bytes(_)
                | ArenaExprKind::Call { .. }
                | ArenaExprKind::Field { .. }
                | ArenaExprKind::Index { .. }
                | ArenaExprKind::Slice { .. }
        );
        let bounds = match (constant_offset, length) {
            // Zero is always a valid method offset, even for an empty receiver.
            (Some(0), Some(length)) => match self.arena.expr(length).kind {
                ArenaExprKind::Int(count) => self
                    .arena
                    .int_literal(count)
                    .value()
                    .filter(|value| *value >= 0)
                    .map(|count| format!("..{count}")),
                ArenaExprKind::Call { callee, args } if args.is_empty() => {
                    match (receiver.kind.clone(), self.arena.expr(callee).kind) {
                        (
                            ArenaExprKind::Ident(receiver_name),
                            ArenaExprKind::Field { base, name },
                        ) if name == "len"
                            && matches!(self.arena.expr(base).kind,
                                ArenaExprKind::Ident(name) if name == receiver_name) =>
                        {
                            Some("..".to_string())
                        }
                        _ => None,
                    }
                }
                _ => self.literal_byte_slice_bounds(base, 0, Some(length)),
            },
            (Some(0), None) => Some("..".to_string()),
            (Some(offset), length) => self.literal_byte_slice_bounds(base, offset, length),
            _ => None,
        };
        let has_comments = self.source[receiver.span.range()].contains('#')
            || self.source[receiver.span.end()..span.end()].contains('#');
        let bounds = bounds.filter(|_| postfix_receiver && !has_comments)?;
        // Include equivalent nested slices so dropping overlapping edits still converges.
        let receiver_text = match receiver.kind {
            ArenaExprKind::Call { callee, args } => {
                self.byte_slice_replacement(callee, args, receiver.span)
            }
            _ => None,
        }
        .unwrap_or_else(|| self.source[receiver.span.range()].to_string());
        Some(format!("{receiver_text}[{bounds}]"))
    }

    /// Numeric adapters clamp or saturate, so only bounded nonnegative literals
    /// can be replaced by checked scalar multiplication without changing failures.
    fn lint_duration_conversion(&mut self, callee: ExprId, args: ArenaRange, span: Span) {
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if !matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "time")
            || !self.duration_conversion_module_unshadowed
            || self.is_binding_in_scope_or_assigned("time")
        {
            return;
        }
        let unit = match name.as_str().as_str() {
            "millis" => 1,
            "seconds" => 1000,
            _ => return,
        };
        let [arg] = self.arena.call_args(args) else {
            return;
        };
        let ArenaCallArgKind::Positional(value) = arg.kind else {
            return;
        };
        let ArenaExprKind::Int(literal) = self.arena.expr(value).kind else {
            return;
        };
        let Some(amount) = self.arena.int_literal(literal).value() else {
            return;
        };
        if amount < 0
            || (amount as u64).checked_mul(unit).is_none()
            || self.expr_types.get(&span) != Some(&Type::Duration)
            || self.expr_types.get(&self.arena.expr(value).span) != Some(&Type::Int)
            || self.source[span.range()].contains('#')
        {
            return;
        }
        let suffix = if unit == 1 { "1ms" } else { "1s" };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "bounded numeric conversion can use Duration arithmetic",
            )
            .with_code(DiagnosticCode::LintDurationArithmetic)
            .with_label(Label::secondary(span, "multiply by a duration unit"))
            .with_fix_hint(FixHint::replacement(
                span,
                "use checked Duration arithmetic",
                format!("({amount} * {suffix})"),
            )),
        );
    }

    fn lint_path_constructor(&mut self, callee: ExprId, args: ArenaRange, span: Span) {
        let callee_expr = self.arena.expr(callee);
        let is_path_ctor = matches!(callee_expr.kind, ArenaExprKind::Ident(name) if name == "Path");
        if !is_path_ctor {
            return;
        }
        let Some(first) = self.arena.call_args(args).first() else {
            return;
        };
        let ArenaCallArgKind::Positional(expr) = first.kind else {
            return;
        };
        let expr_span = self.arena.expr(expr).span;
        if callee_expr.span == span && expr_span == span {
            return;
        }
        let replacement = self.path_constructor_replacement(expr);
        let message = if matches!(
            self.arena.expr(expr).kind,
            ArenaExprKind::Str(_) | ArenaExprKind::FmtString(_)
        ) {
            "prefer path literal syntax for path construction"
        } else {
            "prefer p-string interpolation over `Path(...)`"
        };
        let mut diagnostic =
            xsh::diagnostic::Diagnostic::new(xsh::diagnostic::Severity::Warning, message)
                .with_code(DiagnosticCode::LintPathConstructor)
                .with_label(xsh::diagnostic::Label::secondary(
                    span,
                    "use path string syntax instead",
                ));
        if let Some(replacement) = replacement {
            diagnostic = diagnostic.with_fix_hint(xsh::diagnostic::FixHint::replacement(
                span,
                "replace with path string",
                replacement,
            ));
        }
        self.diagnostics.push(
            diagnostic.with_note(
                "`Path(...)` remains a cast, but p-strings are the preferred path syntax",
            ),
        );
    }

    fn collect_list_splice_parts(
        &self,
        expr: ExprId,
        expected: &Type,
        parts: &mut Vec<(bool, ExprId)>,
        links: &mut usize,
        literal: &mut bool,
    ) -> Option<()> {
        let node = self.arena.expr(expr);
        if self.expr_types.get(&node.span)? != expected {
            return None;
        }
        match node.kind {
            ArenaExprKind::Binary {
                op: BinaryOp::Add,
                left,
                right,
            } => {
                *links += 1;
                self.collect_list_splice_parts(left, expected, parts, links, literal)?;
                self.collect_list_splice_parts(right, expected, parts, links, literal)?;
            }
            ArenaExprKind::Call { callee, args } => {
                if let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind
                    && name == "extend"
                    && args.len() == 1
                    && let ArenaCallArgKind::Positional(value) = self.arena.call_args(args)[0].kind
                {
                    *links += 1;
                    self.collect_list_splice_parts(base, expected, parts, links, literal)?;
                    self.collect_list_splice_parts(value, expected, parts, links, literal)?;
                } else {
                    parts.push((true, expr));
                }
            }
            ArenaExprKind::List(items) | ArenaExprKind::Set(items) => {
                *literal = true;
                let Type::List(element) = expected else {
                    return None;
                };
                for item in self.arena.list_elements(items) {
                    let item_expected = if item.splice_span.is_some() {
                        expected
                    } else {
                        element.as_ref()
                    };
                    if self.expr_types.get(&self.arena.expr(item.value).span)? != item_expected {
                        return None;
                    }
                    parts.push((item.splice_span.is_some(), item.value));
                }
            }
            _ => parts.push((true, expr)),
        }
        Some(())
    }

    fn map_set_parts(&self, expr: ExprId, element: &Type) -> Option<(ExprId, ExprId, ExprId)> {
        let ArenaExprKind::Call { callee, args } = self.arena.expr(expr).kind else {
            return None;
        };
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return None;
        };
        if name != "set"
            || args.len() != 2
            || !matches!(
                self.expr_types.get(&self.arena.expr(base).span),
                Some(Type::Map(_, _))
            )
        {
            return None;
        }
        let arguments = self.arena.call_args(args);
        let (ArenaCallArgKind::Positional(key), ArenaCallArgKind::Positional(value)) =
            (arguments[0].kind.clone(), arguments[1].kind.clone())
        else {
            return None;
        };
        let Type::Map(expected_key, _) = self.expr_types.get(&self.arena.expr(base).span)? else {
            return None;
        };
        if !expected_key.is_map_key()
            || self.expr_types.get(&self.arena.expr(key).span) != Some(expected_key.as_ref())
            || self.expr_types.get(&self.arena.expr(value).span) != Some(element)
        {
            return None;
        }
        Some((base, key, value))
    }

    fn is_empty_map_literal_source(&self, expr: ExprId) -> bool {
        match self.arena.expr(expr).kind {
            ArenaExprKind::Record(fields) => {
                fields.is_empty()
                    && matches!(
                        self.expr_types.get(&self.arena.expr(expr).span),
                        Some(Type::Map(_, _))
                    )
            }
            ArenaExprKind::Call { callee, args } if args.is_empty() => {
                matches!(self.arena.expr(callee).kind,
                ArenaExprKind::Field { base, name } if name == "empty" && matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "map"))
            }
            _ => false,
        }
    }

    fn map_literal_replacement(&self, entries: &[(ExprId, ExprId)]) -> Option<String> {
        let mut fields = Vec::with_capacity(entries.len());
        for &(key, value) in entries {
            let key = self.source.get(self.arena.expr(key).span.range())?;
            let value = self.source.get(self.arena.expr(value).span.range())?;
            fields.push(format!("[{key}]: {value}"));
        }
        Some(format!("{{{}}}", fields.join(", ")))
    }

    fn map_literal_diagnostic(&mut self, span: Span, replacement: String) {
        let Some(source) = self.source.get(span.range()) else {
            return;
        };
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "prefer a Map literal for fresh Map construction",
        )
        .with_code(DiagnosticCode::LintPreferMapLiteral)
        .with_label(Label::secondary(span, "construct the entries in one Map"));
        if source.contains('#') {
            diagnostic =
                diagnostic.with_note("comments in the initialization require a manual rewrite");
        } else {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "construct one Map literal",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_map_literal_chain(&mut self, expr: ExprId) {
        let Some(Type::Map(key_ty, element)) =
            self.expr_types.get(&self.arena.expr(expr).span).cloned()
        else {
            return;
        };
        if !list_splice_element_type_is_precise(&element) {
            return;
        }
        let mut base = expr;
        let mut entries = Vec::new();
        while let Some((receiver, key, value)) = self.map_set_parts(base, &element) {
            entries.push((key, value));
            base = receiver;
        }
        if entries.is_empty() {
            return;
        }
        entries.reverse();
        let Some(mut replacement) = self.map_literal_replacement(&entries) else {
            return;
        };
        if !self.is_empty_map_literal_source(base) {
            let ArenaExprKind::Record(fields) = self.arena.expr(base).kind else {
                return;
            };
            if self.expr_types.get(&self.arena.expr(base).span)
                != Some(&Type::Map(key_ty.clone(), element.clone()))
            {
                return;
            }
            let mut originals = Vec::new();
            for field in self.arena.record_fields(fields) {
                let (value, span) = match field.kind {
                    ArenaRecordFieldKind::Computed { key, value, span } => {
                        if self.expr_types.get(&self.arena.expr(key).span) != Some(key_ty.as_ref())
                        {
                            return;
                        }
                        (value, span)
                    }
                    ArenaRecordFieldKind::Named { value, span, .. } => (value, span),
                    _ => return,
                };
                if self.expr_types.get(&self.arena.expr(value).span) != Some(element.as_ref()) {
                    return;
                }
                let Some(text) = self.source.get(self.arena.span(span).range()) else {
                    return;
                };
                originals.push(text);
            }
            if !originals.is_empty() {
                replacement = format!(
                    "{{{}, {}}}",
                    originals.join(", "),
                    &replacement[1..replacement.len() - 1]
                );
            }
        }
        self.map_literal_diagnostic(self.arena.expr(expr).span, replacement);
    }

    fn lint_list_splicing(&mut self, expr: ExprId) {
        let node = self.arena.expr(expr);
        if !matches!(
            node.kind,
            ArenaExprKind::Binary {
                op: BinaryOp::Add,
                ..
            } | ArenaExprKind::Call { .. }
        ) {
            return;
        }
        let Some(expected @ Type::List(element)) = self.expr_types.get(&node.span) else {
            return;
        };
        if !list_splice_element_type_is_precise(element) {
            return;
        }
        let mut parts = Vec::new();
        let mut links = 0;
        let mut literal = false;
        if self
            .collect_list_splice_parts(expr, expected, &mut parts, &mut links, &mut literal)
            .is_none()
            || links == 0
            || (links == 1 && !literal)
        {
            return;
        }
        // A simple receiver update already has a compound-assignment spelling.
        if links == 1 && matches!(node.kind, ArenaExprKind::Call { .. }) {
            return;
        }
        let Some(source) = self.source.get(node.span.range()) else {
            return;
        };
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "prefer one list literal with explicit splices for list construction",
        )
        .with_code(DiagnosticCode::LintPreferListSplicing)
        .with_label(Label::secondary(
            node.span,
            "build the elements in one literal",
        ));
        if source.contains('#') {
            diagnostic =
                diagnostic.with_note("comments in the construction require a manual rewrite");
        } else {
            let mut entries = Vec::with_capacity(parts.len());
            for (splice, value) in parts {
                let Some(source) = self.source.get(self.arena.expr(value).span.range()) else {
                    return;
                };
                entries.push(if splice {
                    format!("@{source}")
                } else {
                    source.to_string()
                });
            }
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                node.span,
                "build one spliced list literal",
                format!("[{}]", entries.join(", ")),
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_join_to_concat(&mut self, callee: ExprId, args: ArenaRange, span: Span) {
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if name != "join" {
            return;
        }
        let ArenaExprKind::List(items) = self.arena.expr(base).kind else {
            return;
        };
        if self
            .arena
            .list_elements(items)
            .any(|item| item.splice_span.is_some())
        {
            return;
        }
        let separator_is_empty = args.is_empty()
            || {
                match self.arena.call_args(args).first().map(|a| a.kind.clone()) {
                    Some(ArenaCallArgKind::Positional(e)) => {
                        matches!(self.arena.expr(e).kind, ArenaExprKind::Str(id) if self.arena.string_literal(id).is_empty())
                    }
                    _ => false,
                }
            };
        if !separator_is_empty {
            return;
        }
        if items.is_empty() {
            return;
        }
        let replacement = self
            .arena
            .list_element_exprs(items)
            .map(|item| {
                let s = self.arena.expr(item).span;
                &self.source[s.start()..s.end()]
            })
            .collect::<Vec<_>>()
            .join(" + ");
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "prefer `+` over `[a, b].join(\"\")` for string concatenation",
            )
            .with_code(DiagnosticCode::LintPreferStringConcat)
            .with_label(Label::secondary(span, "use `+` instead"))
            .with_fix_hint(FixHint::replacement(span, "rewrite with `+`", replacement)),
        );
    }

    fn lint_redundant_defaults(&mut self, callee: ExprId, args: ArenaRange) {
        if is_method_call(self.arena, callee, "mkdir") {
            let rooted = matches!(self.arena.expr(callee).kind, ArenaExprKind::Field { base, .. }
                if self.expr_types.get(&self.arena.expr(base).span) == Some(&Type::FsRoot));
            self.lint_redundant_named_bool(
                args,
                "parents",
                !rooted,
                if rooted {
                    "rooted mkdir does not create parent directories by default"
                } else {
                    "mkdir creates parent directories by default"
                },
            );
        } else if self.is_path_remove(callee) {
            self.lint_redundant_named_bool(
                args,
                "missing_ok",
                true,
                "`remove` accepts a missing path by default",
            );
        } else if is_fs_call(self.arena, callee, "touch")
            || is_method_call(self.arena, callee, "touch")
        {
            self.lint_redundant_named_bool(
                args,
                "create",
                true,
                "`touch` creates the file by default",
            );
        } else if is_fs_call(self.arena, callee, "walk")
            || is_method_call(self.arena, callee, "walk")
        {
            self.lint_redundant_named_bool(
                args,
                "gitignore",
                true,
                "`walk` respects .gitignore by default",
            );
        } else if is_fs_call(self.arena, callee, "remove_manifest") {
            self.lint_redundant_named_bool(
                args,
                "prune_dirs",
                true,
                "`remove_manifest` prunes empty directories by default",
            );
        } else if is_fs_call(self.arena, callee, "install")
            || is_method_call(self.arena, callee, "install")
        {
            self.lint_redundant_named_bool(
                args,
                "parents",
                true,
                "`install` creates parent directories by default",
            );
        } else if is_fs_call(self.arena, callee, "install_as")
            || is_method_call(self.arena, callee, "install_as")
        {
            self.lint_redundant_named_bool(
                args,
                "parents",
                true,
                "`install_as` creates parent directories by default",
            );
        } else if is_fs_call(self.arena, callee, "copy_tree")
            || is_method_call(self.arena, callee, "copy_tree")
        {
            self.lint_redundant_named_bool(
                args,
                "parents",
                true,
                "`copy_tree` creates parent directories by default",
            );
        }
    }

    /// `.remove(...)` on a value that is statically a `Path`. A map's
    /// `remove`, a rooted `remove`, and a user function of the same name have
    /// no `missing_ok` default to match.
    fn is_path_remove(&self, callee: ExprId) -> bool {
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return false;
        };
        if name != "remove" {
            return false;
        }
        self.expr_types.get(&self.arena.expr(base).span) == Some(&Type::Path)
    }

    fn lint_fs_root_receiver(&mut self, callee: ExprId, args: ArenaRange, span: Span) {
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        let ArenaExprKind::Ident(module) = self.arena.expr(base).kind else {
            return;
        };
        if module != "fs" || self.scopes.iter().any(|scope| scope.contains_key("fs")) {
            return;
        }
        let Some(method) = xsh_registry::signature::legacy_fs_root_method(&name.as_str()) else {
            return;
        };
        let entries = self.arena.call_args(args);
        let Some(first) = entries.first() else {
            return;
        };
        let receiver = match first.kind {
            ArenaCallArgKind::Positional(value)
            | ArenaCallArgKind::Named { name: _, value, .. } => value,
            _ => return,
        };
        if self.expr_types.get(&self.arena.expr(receiver).span) != Some(&Type::FsRoot) {
            return;
        }
        let receiver_span = self.arena.expr(receiver).span;
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            format!("use FsRoot.{method} for this removed filesystem operation"),
        )
        .with_code(DiagnosticCode::LintFsRootReceiver)
        .with_label(Label::secondary(
            span,
            "preserve receiver and argument evaluation order",
        ));
        let safe_receiver = match first.kind {
            ArenaCallArgKind::Positional(_) => true,
            ArenaCallArgKind::Named { name, .. } => name == "root",
            _ => false,
        };
        let safe_args = entries.iter().all(|entry| {
            !matches!(
                entry.kind,
                ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. }
            )
        });
        if safe_receiver && safe_args && !span_may_contain_comment(self.source, span) {
            let receiver_text = &self.source[receiver_span.range()];
            let receiver_text = match self.arena.expr(receiver).kind {
                ArenaExprKind::Ident(_)
                | ArenaExprKind::Field { .. }
                | ArenaExprKind::Call { .. } => receiver_text.to_string(),
                _ => format!("({receiver_text})"),
            };
            let first_end = call_arg_span(self.arena, first)
                .expect("ordinary capability argument")
                .end();
            let suffix = &self.source[first_end..span.end()];
            let suffix = if entries.len() > 1 {
                let Some(comma) = suffix.find(',') else {
                    return;
                };
                let suffix = &suffix[comma + 1..];
                if suffix.starts_with('\n') {
                    suffix
                } else {
                    suffix.trim_start()
                }
            } else {
                ")"
            };
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "promote the first evaluated capability to its receiver",
                format!("{receiver_text}.{method}({suffix}"),
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_prefer_method(&mut self, callee: ExprId, args: ArenaRange, span: Span) {
        let ArenaExprKind::Field { base, name: func } = self.arena.expr(callee).kind else {
            return;
        };
        let ArenaExprKind::Ident(module) = self.arena.expr(base).kind else {
            return;
        };
        // Check module.func is a known module-function with a method equivalent.
        // The first positional arg becomes the receiver; the rest become method args.
        let min_args = module_func_min_args(module.as_str().as_str(), func.as_str().as_str());
        let Some(min_args) = min_args else {
            return;
        };
        if args.len() < min_args {
            return;
        }
        let arg_list = self.arena.call_args(args).to_vec();
        let Some(ArenaCallArgKind::Positional(receiver_expr)) =
            arg_list.first().map(|a| a.kind.clone())
        else {
            return;
        };
        let receiver_span = self.arena.expr(receiver_expr).span;
        let receiver_text = &self.source[receiver_span.start()..receiver_span.end()];
        let rest: Vec<&str> = arg_list[1..]
            .iter()
            .filter_map(|a| match a.kind {
                ArenaCallArgKind::Positional(e) => {
                    let s = self.arena.expr(e).span;
                    Some(&self.source[s.start()..s.end()])
                }
                _ => None,
            })
            .collect();
        let replacement = if rest.is_empty() {
            format!("{receiver_text}.{func}()")
        } else {
            format!("{receiver_text}.{func}({})", rest.join(", "))
        };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("prefer method form `{replacement}`"),
            )
            .with_code(DiagnosticCode::LintPreferMethod)
            .with_label(Label::secondary(span, "use method syntax instead"))
            .with_fix_hint(FixHint::replacement(
                span,
                "rewrite as method call",
                replacement,
            )),
        );
    }

    fn expression_source_span(&self, span: Span) -> Span {
        self.source_cst
            .get_or_init(|| {
                xsh::frontend::syntax::cst::SyntaxTree::parse(span.source_id, self.source).0
            })
            .expression_source_span(span)
    }

    fn expression_text(&self, expr: ExprId) -> Option<String> {
        let span = self.expression_source_span(self.arena.expr(expr).span);
        let mut text = self.source.get(span.range())?.to_string();
        let cst = self
            .source_cst
            .get()
            .expect("expression span initializes CST");
        let mut edits: Vec<_> = self
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                matches!(
                    diagnostic.code,
                    Some(DiagnosticCode::LintPreferIn | DiagnosticCode::LintCoreAssert)
                )
            })
            .flat_map(|diagnostic| diagnostic.fix_hints.iter())
            .filter_map(|hint| Some((hint.span?, hint.replacement.as_ref()?)))
            .filter(|(edit, _)| {
                edit.source_id == span.source_id
                    && edit.start() >= span.start()
                    && edit.end() <= span.end()
                    && !cst.contains_comment(*edit)
            })
            .collect();
        edits.sort_by_key(|(edit, _)| (edit.start(), std::cmp::Reverse(edit.end())));
        let mut end = span.start();
        let edits: Vec<_> = edits
            .into_iter()
            .filter(|(edit, _)| {
                if edit.start() < end {
                    false
                } else {
                    end = edit.end();
                    true
                }
            })
            .collect();
        for (edit, replacement) in edits.into_iter().rev() {
            text.replace_range(
                edit.start() - span.start()..edit.end() - span.start(),
                replacement,
            );
        }
        Some(text)
    }

    fn lint_prefer_in(&mut self, callee: ExprId, args: ArenaRange, span: Span) {
        if let Some((module, name)) = self.standard_call_spans.get(&span).cloned() {
            if module == "test"
                && matches!(
                    name.as_str(),
                    "ok" | "eq" | "ne" | "contains" | "not_contains"
                )
            {
                self.lint_assertion_helper(args, span, &name);
                return;
            }
            if module == "set" && name == "has" {
                let Some(arguments) = migration_arguments(self.arena, args, &["set", "item"])
                else {
                    return;
                };
                self.lint_membership_replacement(
                    arguments[0],
                    arguments[1],
                    span,
                    self.negated_call_spans.contains_key(&span),
                );
                return;
            }
        }
        if !self.membership_migration_spans.contains(&span) {
            return;
        }
        if matches!(
            self.arena.expr(callee).kind,
            ArenaExprKind::NullSafeField { .. }
        ) {
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    "use canonical membership after explicitly handling the Optional or Result",
                )
                .with_code(DiagnosticCode::LintPreferIn)
                .with_label(Label::secondary(
                    span,
                    "retain null-safe consumption and operand evaluation order",
                )),
            );
            return;
        }
        let ArenaExprKind::Field {
            base: receiver,
            name,
        } = self.arena.expr(callee).kind
        else {
            return;
        };
        let parameter = if name == "has" {
            if matches!(
                self.expr_types.get(&self.arena.expr(receiver).span),
                Some(Type::Map(_, _))
            ) {
                "key"
            } else {
                "field"
            }
        } else if matches!(
            self.expr_types.get(&self.arena.expr(receiver).span),
            Some(Type::List(_))
        ) {
            "item"
        } else {
            "needle"
        };
        let Some(arguments) = migration_arguments(self.arena, args, &[parameter]) else {
            return;
        };
        self.lint_membership_replacement(
            receiver,
            arguments[0],
            span,
            self.negated_call_spans.contains_key(&span),
        );
    }

    fn membership_types_supported(&self, container: ExprId, item: ExprId) -> bool {
        let Some(container) = self.expr_types.get(&self.arena.expr(container).span) else {
            return false;
        };
        let Some(item) = self.expr_types.get(&self.arena.expr(item).span) else {
            return false;
        };
        if matches!(item, Type::Any | Type::Unknown | Type::Invalid) {
            return false;
        }
        match container {
            Type::Str => *item == Type::Str,
            Type::Bytes => *item == Type::Bytes,
            Type::List(expected) => item.matches_expected(expected),
            Type::Map(key, _) => item.matches_expected(key),
            Type::Set(element) => item.matches_expected(element),
            Type::Record(_) => *item == Type::Str,
            _ => false,
        }
    }

    fn lint_membership_replacement(
        &mut self,
        container: ExprId,
        item: ExprId,
        span: Span,
        negated: bool,
    ) {
        let mut diagnostic = Diagnostic::new(Severity::Warning, "use canonical membership syntax")
            .with_code(DiagnosticCode::LintPreferIn)
            .with_label(Label::secondary(
                span,
                "use `in` or `not in`; bind operands in their original order if necessary",
            ));
        if self.membership_types_supported(container, item)
            && let (Some(container_text), Some(item_text)) =
                (self.expression_text(container), self.expression_text(item))
        {
            let operator = if negated { "not in" } else { "in" };
            if migration_reorder_safe(self.arena, container, item) {
                let fix_span = self.expression_source_span(
                    self.negated_call_spans.get(&span).copied().unwrap_or(span),
                );
                diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                    fix_span,
                    "rewrite membership",
                    format!("({item_text}) {operator} ({container_text})"),
                ));
            } else {
                if let Some(statement) = self.whole_statement_call_spans.get(&span).copied() {
                    let container_name = self.migration_temporary_name("container", span.start());
                    let item_name = self.migration_temporary_name("item", span.start());
                    diagnostic = diagnostic.with_fix_hint(FixHint::replacement(self.expression_source_span(statement), "preserve operand order with local bindings", format!("{{ let {container_name} = ({container_text}); let {item_name} = ({item_text}); {item_name} {operator} {container_name} }}")));
                }
            }
        }
        self.diagnostics.push(diagnostic);
    }

    fn migration_temporary_name(&self, role: &str, start: usize) -> String {
        let mut suffix = start;
        loop {
            let name = format!("membership_{role}_{suffix}");
            if !self.source.contains(&name) {
                return name;
            }
            suffix += 1;
        }
    }

    fn lint_assertion_helper(&mut self, args: ArenaRange, span: Span, name: &str) {
        let membership = matches!(name, "contains" | "not_contains");
        let params: &[&str] = match name {
            "ok" => &["condition", "message"],
            "eq" | "ne" => &["left", "right", "message"],
            _ => &["haystack", "needle", "message"],
        };
        let Some(arguments) = migration_arguments_optional(self.arena, args, params) else {
            return;
        };
        let required = if name == "ok" { 1 } else { 2 };
        if arguments[..required].iter().any(Option::is_none) {
            return;
        }
        let custom_message = arguments[required];
        let bare_span = self.statement_call_spans.get(&span).copied();
        if !membership && (bare_span.is_none() || custom_message.is_some()) {
            return;
        }
        let first = arguments[0].unwrap();
        let second = arguments.get(1).and_then(|arg| *arg);
        let supported = if membership {
            self.membership_types_supported(first, second.unwrap())
        } else if name == "ok" {
            self.expr_types.get(&self.arena.expr(first).span) == Some(&Type::Bool)
        } else {
            match (
                self.expr_types.get(&self.arena.expr(first).span),
                self.expr_types.get(&self.arena.expr(second.unwrap()).span),
            ) {
                (Some(left), Some(right)) => {
                    !matches!(left, Type::Any | Type::Unknown | Type::Invalid)
                        && !matches!(right, Type::Any | Type::Unknown | Type::Invalid)
                        && right.matches_expected(left)
                        && !self.expression_depends_on_expected_type(first, false)
                        && !self.expression_depends_on_expected_type(second.unwrap(), false)
                }
                _ => false,
            }
        };
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            if membership {
                "use canonical membership syntax"
            } else {
                "use an `assert` statement"
            },
        )
        .with_code(if membership {
            DiagnosticCode::LintPreferIn
        } else {
            DiagnosticCode::LintCoreAssert
        })
        .with_label(Label::secondary(
            span,
            "preserve custom messages, consumed Results, and argument evaluation order",
        ));
        let source_order: Vec<ExprId> = self
            .arena
            .call_args(args)
            .iter()
            .map(|arg| match arg.kind {
                ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. } => {
                    value
                }
                _ => unreachable!(),
            })
            .collect();
        let expected_order: Vec<ExprId> = arguments.iter().flatten().copied().collect();
        let named_order_safe = source_order == expected_order
            || source_order
                .iter()
                .all(|expr| migration_inert(self.arena, *expr));
        if supported
            && named_order_safe
            && (!membership || migration_reorder_safe(self.arena, first, second.unwrap()))
        {
            let Some(first_text) = self.expression_text(first) else {
                return;
            };
            let second_text = second.and_then(|expr| self.expression_text(expr));
            let predicate = match name {
                "ok" => format!("({first_text})"),
                "eq" => format!("({first_text}) == ({})", second_text.unwrap()),
                "ne" => format!("({first_text}) != ({})", second_text.unwrap()),
                _ => format!(
                    "({}) {} ({first_text})",
                    second_text.unwrap(),
                    if name == "not_contains" {
                        "not in"
                    } else {
                        "in"
                    }
                ),
            };
            let (fix_span, replacement) = if let Some(message) = custom_message {
                let Some(message) = self.expression_text(message) else {
                    return;
                };
                (span, format!("test.ok({predicate}, message: {message})"))
            } else if let Some(bare_span) = bare_span {
                let fix_span = self.expression_source_span(bare_span);
                (
                    fix_span,
                    assert_statement(&predicate, comma_terminated(self.source, fix_span.end())),
                )
            } else {
                (span, format!("test.ok({predicate})"))
            };
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                fix_span,
                "rewrite assertion",
                replacement,
            ));
        }
        if diagnostic.fix_hints.is_empty()
            && supported
            && let Some(statement) = self.whole_statement_call_spans.get(&span).copied()
        {
            let mut bindings = String::new();
            let mut names = BTreeMap::new();
            // Snapshot supplied expressions in source order. Signature slots
            // still select the corresponding values for the predicate.
            for (index, argument) in source_order.iter().enumerate() {
                let Some(text) = self.expression_text(*argument) else {
                    return;
                };
                let name =
                    self.migration_temporary_name(&format!("argument_{index}"), span.start());
                bindings.push_str(&format!("let {name} = ({text}); "));
                names.insert(*argument, name);
            }
            let first = &names[&first];
            let second = second.map(|argument| names[&argument].as_str());
            let predicate = match name {
                "ok" => first.to_string(),
                "eq" => format!("{first} == {}", second.unwrap()),
                "ne" => format!("{first} != {}", second.unwrap()),
                _ => format!(
                    "{} {} {first}",
                    second.unwrap(),
                    if name == "not_contains" {
                        "not in"
                    } else {
                        "in"
                    }
                ),
            };
            let assertion = if let Some(message) = custom_message {
                let suffix = if statement == span { "" } else { "?" };
                format!("test.ok({predicate}, message: {}){suffix}", names[&message])
            } else {
                assert_statement(&predicate, false)
            };
            bindings.push_str(&assertion);
            // Inline match arms accept one expression. A lexical block also
            // keeps operand snapshots local to the original assertion.
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                self.expression_source_span(statement),
                "preserve argument order with local bindings",
                format!("{{ {bindings} }}"),
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_lexical_block(
        &mut self,
        branches: ArenaRange,
        else_block: Option<BlockId>,
        span: Span,
    ) {
        if branches.len() != 1
            || else_block.is_some()
            || self.statement_positions.get(&span)
                != Some(&xsh::frontend::check::StatementPosition::Statement)
        {
            return;
        }
        let branch = self.arena.if_branches(branches)[0].clone();
        if !matches!(
            self.arena.expr(branch.condition).kind,
            ArenaExprKind::Bool(true)
        ) {
            return;
        }
        let block_span = self.arena.span(self.arena.block(branch.block).span);
        if self
            .source
            .get(span.start()..block_span.start())
            .is_none_or(|prefix| prefix.contains('#'))
        {
            return;
        }
        let Some(body) = self.source.get(block_span.range()) else {
            return;
        };
        let parsed =
            xsh::frontend::syntax::parser::Parser::parse_source_arena_only(span.source_id, body);
        if !parsed.diagnostics.is_empty() {
            return;
        }
        let Some(statement) = parsed.arena.statement_ids().next() else {
            return;
        };
        let ArenaStmtKind::Expr(expr) = parsed.arena.arena.stmt(statement).kind else {
            return;
        };
        if !matches!(
            parsed.arena.arena.expr(expr).kind,
            ArenaExprKind::ValueBlock(_)
        ) {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "use a lexical block for an unconditional scope",
            )
            .with_code(DiagnosticCode::LintLexicalBlock)
            .with_label(Label::secondary(
                span,
                "retain the block's binding and cleanup scope",
            ))
            .with_fix_hint(FixHint::replacement(
                Span::new(span.source_id, span.start(), block_span.start()),
                "use a lexical block",
                String::new(),
            )),
        );
    }

    fn lint_prefer_fs_files(&mut self, input: ExprId, stages: ArenaRange) {
        let stage_slice = self.arena.stream_stages(stages);
        let Some(first_stage) = stage_slice.first().cloned() else {
            return;
        };
        let first_stage_span = self.arena.span(first_stage.span);
        if first_stage.kind != StreamStageKind::Where {
            return;
        }
        let Some(block) = first_stage.block else {
            return;
        };
        let block_data = self.arena.block(block);
        let block_stmts: Vec<StmtId> = self.arena.stmt_ids(block_data.statements).collect();
        if !(block_data.params.is_empty() && block_stmts.len() == 1) {
            return;
        }
        let ArenaStmtKind::Expr(expr) = self.arena.stmt(block_stmts[0]).kind else {
            return;
        };
        if !is_kind_eq_file_expr(self.arena, expr) {
            return;
        }
        let input_span = self.arena.expr(input).span;
        let ArenaExprKind::Call { callee, args } = self.arena.expr(input).kind else {
            return;
        };
        if !is_fs_call(self.arena, callee, "walk") {
            return;
        }
        // Extract source text for args to reconstruct fs.files(ARGS)
        let args_src: Vec<&str> = self
            .arena
            .call_args(args)
            .iter()
            .filter_map(|a| {
                let span = match a.kind {
                    ArenaCallArgKind::Positional(e) => self.arena.expr(e).span,
                    ArenaCallArgKind::Named { span, .. }
                    | ArenaCallArgKind::Splice { span, .. }
                    | ArenaCallArgKind::NamedSpread { span, .. } => self.arena.span(span),
                };
                self.source.get(span.start()..span.end())
            })
            .collect();
        let replacement = format!("fs.files({})", args_src.join(", "));
        let replace_span = Span::new(
            input_span.source_id,
            input_span.start(),
            first_stage_span.end(),
        );
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "prefer `fs.files()` over `fs.walk() |> where .kind == \"file\"`",
            )
            .with_code(DiagnosticCode::LintPreferFsFiles)
            .with_label(Label::secondary(
                first_stage_span,
                "this stage is redundant with fs.files",
            ))
            .with_fix_hint(FixHint::replacement(
                replace_span,
                "use fs.files()",
                replacement,
            )),
        );
    }

    fn lint_redundant_stream_stages(&mut self, stages: ArenaRange) {
        for stage in self.arena.stream_stages(stages).to_vec() {
            let stage_span = self.arena.span(stage.span);
            if !stage.args.is_empty() {
                continue;
            }
            let Some(block) = stage.block else {
                continue;
            };
            let block_data = self.arena.block(block);
            let block_stmts: Vec<StmtId> = self.arena.stmt_ids(block_data.statements).collect();
            if !block_data.params.is_empty() || block_stmts.len() != 1 {
                continue;
            }
            let ArenaStmtKind::Expr(expr) = self.arena.stmt(block_stmts[0]).kind else {
                continue;
            };
            let (code, message) = match stage.kind {
                StreamStageKind::Where
                    if matches!(self.arena.expr(expr).kind, ArenaExprKind::Bool(true)) =>
                {
                    (
                        DiagnosticCode::LintRedundantPipelineStage,
                        "redundant `where true` pipeline stage",
                    )
                }
                StreamStageKind::Map
                    if matches!(self.arena.expr(expr).kind, ArenaExprKind::Item) =>
                {
                    (
                        DiagnosticCode::LintRedundantPipelineStage,
                        "redundant `map .` pipeline stage",
                    )
                }
                _ => continue,
            };
            let deletion_span = scan_pipe_stage_deletion_span(self.source, stage_span);
            self.diagnostics.push(
                Diagnostic::new(Severity::Warning, message)
                    .with_code(code)
                    .with_label(Label::secondary(
                        stage_span,
                        "this stage does not change items",
                    ))
                    .with_fix_hint(FixHint::deletion(
                        deletion_span,
                        "remove redundant pipeline stage",
                    )),
            );
        }
    }

    // Only a loop directly over the lines of a checked Path: a loop over a
    // list built from them needs the list, and other `read_text` callables
    // have no lazy counterpart.
    fn lint_prefer_file_lines(&mut self, iter: ExprId) {
        if lint_read_lines::read_text_lines_path(self.arena, iter, &self.expr_types).is_none() {
            return;
        }
        let iter_span = self.arena.expr(iter).span;
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "prefer file-backed lines in line-by-line loops",
            )
            .with_code(DiagnosticCode::LintPreferFileLines)
            .with_label(Label::secondary(
                iter_span,
                "`read_text()?.lines()` reads the full file first; use `path.lines()?` when consuming once",
            )),
        );
    }

    // Path API migrations, each matched in its own file against the one
    // expression being visited.
    fn lint_path_migrations(&mut self, expr: ExprId) {
        let found = [
            lint_path_text_query::path_text_query(self.arena, self.source, &self.expr_types, expr),
            lint_path_display_equality::path_display_equality(
                self.arena,
                self.source,
                &self.expr_types,
                expr,
            ),
            lint_write_lines::write_lines(self.arena, self.source, &self.expr_types, expr),
            lint_read_lines::read_lines(
                self.arena,
                self.source,
                &self.expr_types,
                expr,
                &self.diagnostics,
            ),
        ];
        self.diagnostics.extend(found.into_iter().flatten());
        let sinks = lint_path_display_sink::path_display_sinks(
            self.arena,
            self.source,
            &self.expr_types,
            expr,
        );
        self.diagnostics.extend(sinks);
        let search_paths =
            lint_env_path_list::env_path_lists(self.arena, self.source, &self.expr_types, expr);
        self.diagnostics.extend(search_paths);
        let kind =
            lint_path_kind::path_kind_comparison(self.arena, self.source, &self.expr_types, expr);
        self.diagnostics.extend(kind);
        let fs_is_shadowed = self.scopes.iter().any(|scope| scope.contains_key("fs"));
        let symlink =
            lint_argument_label::positional_symlink(self.arena, self.source, fs_is_shadowed, expr);
        self.diagnostics.extend(symlink);
        if self.prefer_rel_path {
            let rooted =
                lint_prefer_rel_path::unvalidated_rooted_path(self.arena, &self.expr_types, expr);
            self.diagnostics.extend(rooted);
        }
        let emptiness = lint_prefer_is_empty::length_compared_with_zero(
            self.arena,
            self.source,
            &self.expr_types,
            expr,
        );
        self.diagnostics.extend(emptiness);
        let end_index = lint_prefer_negative_index::length_minus_literal_index(
            self.arena,
            self.source,
            &self.expr_types,
            expr,
        );
        self.diagnostics.extend(end_index);
        let bytes_is_shadowed = self.scopes.iter().any(|scope| scope.contains_key("bytes"));
        let conversion = lint_prefer_as_conversion::propagated_conversion(
            self.arena,
            self.source,
            &self.expr_types,
            bytes_is_shadowed,
            expr,
        );
        self.diagnostics.extend(conversion);
    }

    fn lint_redundant_named_bool(
        &mut self,
        args: ArenaRange,
        name: &str,
        value: bool,
        label: &'static str,
    ) {
        let Some((arg_span, deletion_span)) = named_bool_arg_info(self.arena, args, name, value)
        else {
            return;
        };
        // An only argument written on its own line leaves its trailing comma
        // and the line breaks behind, so everything between the parentheses
        // goes with it.
        let deletion_span = if deletion_span == arg_span {
            only_argument_with_layout(self.source, arg_span)
        } else {
            deletion_span
        };
        let val_str = if value { "true" } else { "false" };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("`{name}: {val_str}` is redundant"),
            )
            .with_code(DiagnosticCode::LintRedundantDefault)
            .with_label(Label::secondary(arg_span, label))
            .with_fix_hint(FixHint::deletion(
                deletion_span,
                "remove redundant argument",
            )),
        );
    }

    fn warning(
        &mut self,
        span: Span,
        message: impl Into<String>,
        code: DiagnosticCode,
        label: &'static str,
    ) {
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, message)
                .with_code(code)
                .with_label(Label::secondary(span, label)),
        );
    }

}

fn result_unit_type_expr(arena: &AstArena, ty: TypeExprId) -> bool {
    matches!(
        type_expr_kind(arena, ty),
        ArenaTypeExprKind::Result { ok, .. }
            if matches!(type_expr_kind(arena, ok), ArenaTypeExprKind::Named(name) if name == "Unit")
    )
}

fn result_path_type_expr(arena: &AstArena, ty: TypeExprId) -> bool {
    matches!(
        type_expr_kind(arena, ty),
        ArenaTypeExprKind::Result { ok, .. }
            if matches!(type_expr_kind(arena, ok), ArenaTypeExprKind::Named(name) if name == "Path")
    )
}

fn result_ok_type_expr(arena: &AstArena, ty: TypeExprId) -> Option<Type> {
    match type_expr_kind(arena, ty) {
        ArenaTypeExprKind::Result { ok, .. } => {
            let ty = Type::from_arena(arena, ok);
            if matches!(ty, Type::Any | Type::Unknown | Type::Invalid) {
                None
            } else {
                Some(ty)
            }
        }
        _ => None,
    }
}

fn tail_type_matches_lint_expected(return_ty: &Type, value_ty: &Type) -> bool {
    value_ty.matches_expected(return_ty)
        || matches!(return_ty, Type::Result(ok, _) if value_ty.matches_expected(ok))
}

/// `{source}` as an interpolation in a single-line f-string: an expression
/// that begins with `{` is set off by spaces, and one that spans lines has
/// no single-line spelling.
fn single_line_interpolation(source: &str) -> Option<String> {
    if source.contains(['\n', '\r']) {
        return None;
    }
    Some(if source.starts_with('{') {
        format!("{{ {source} }}")
    } else {
        format!("{{{source}}}")
    })
}

fn single_interpolation_expr(arena: &AstArena, parts: ArenaRange) -> Option<ExprId> {
    let parts: Vec<ArenaFmtPart> = arena.fmt_parts(parts).collect();
    match parts.as_slice() {
        [ArenaFmtPart::Expr(expr, None)] => Some(*expr),
        _ => None,
    }
}

/// Where an interpolated `.display()` appears, which decides how its Path
/// receiver can be spelled once the call is dropped.
#[derive(Clone, Copy)]
enum PathDisplaySite {
    /// `${...}` or `$name...` in a word, or a string interpolation.
    Interpolation,
    /// A whole expression command word such as `print p.display()` or
    /// `print (p.display())`, carrying the word's span.
    CommandWord(Span),
    /// A splice reads list elements, so a Path receiver is not a drop-in.
    Splice,
}

fn path_fmt_literal_text(text: &str) -> Option<String> {
    text.strip_prefix("f\"").map(|rest| format!("fp\"{rest}"))
}

fn direct_call_name(arena: &AstArena, expr: ExprId) -> Option<xsh::frontend::symbols::Name> {
    let expr = match arena.expr(expr).kind {
        ArenaExprKind::Try(inner) => inner,
        _ => expr,
    };
    let ArenaExprKind::Call { callee, .. } = arena.expr(expr).kind else {
        return None;
    };
    let ArenaExprKind::Ident(name) = arena.expr(callee).kind else {
        return None;
    };
    Some(name)
}

fn call_arg_span(arena: &AstArena, arg: &ArenaCallArg) -> Option<Span> {
    match arg.kind {
        ArenaCallArgKind::Positional(expr) => Some(arena.expr(expr).span),
        ArenaCallArgKind::Named { span, .. }
        | ArenaCallArgKind::Splice { span, .. }
        | ArenaCallArgKind::NamedSpread { span, .. } => Some(arena.span(span)),
    }
}

fn named_bool_arg_info(
    arena: &AstArena,
    args: ArenaRange,
    name: &str,
    value: bool,
) -> Option<(Span, Span)> {
    let args = arena.call_args(args).to_vec();
    args.iter().enumerate().find_map(|(i, arg)| {
        let ArenaCallArgKind::Named {
            name: arg_name,
            value: expr,
            span,
        } = arg.kind
        else {
            return None;
        };
        let span = arena.span(span);
        if arg_name != name {
            return None;
        }
        if !matches!(arena.expr(expr).kind, ArenaExprKind::Bool(found) if found == value) {
            return None;
        }
        // Only safe to remove when this is the last argument. xsh resolves named
        // arguments by their positional slot, so removing a non-last arg would shift
        // any subsequent args to the wrong parameter positions.
        if i + 1 < args.len() {
            return None;
        }
        // Deletion span includes the separator (comma + whitespace) so the result
        // is syntactically valid. Prefer consuming the preceding separator; if this
        // is the only argument, just span the arg itself.
        let deletion_span = if i > 0 {
            let prev = call_arg_span(arena, &args[i - 1])?;
            Span::new(prev.source_id, prev.end(), span.end())
        } else {
            span
        };
        Some((span, deletion_span))
    })
}

/// The span from just after `(` to just before `)` around an only argument,
/// when nothing but whitespace and one trailing comma stand between them;
/// otherwise the argument's own span.
fn only_argument_with_layout(source: &str, arg: Span) -> Span {
    let before = source.get(..arg.start()).map(str::trim_end);
    let after = source.get(arg.end()..).map(|rest| {
        let rest = rest.trim_start();
        rest.strip_prefix(',').map_or(rest, str::trim_start)
    });
    match (before, after) {
        (Some(before), Some(after)) if before.ends_with('(') && after.starts_with(')') => {
            Span::new(arg.source_id, before.len(), source.len() - after.len())
        }
        _ => arg,
    }
}

// Returns the minimum expected argument count for a module.function call that
// has a method equivalent, or None if no such mapping exists. The first argument
// is always the receiver, so min_args >= 1.
fn module_func_min_args(module: &str, func: &str) -> Option<usize> {
    match (module, func) {
        ("list", "len") => Some(1),
        ("list", "push") => Some(2),
        ("list", "extend") => Some(2),
        ("list", "contains") => Some(2),
        ("list", "get") => Some(2),
        ("map", "has") => Some(2),
        ("map", "get") => Some(2),
        ("map", "set") => Some(3),
        ("map", "remove") => Some(2),
        ("map", "keys") => Some(1),
        ("map", "values") => Some(1),
        // record.* stays as module functions — designed for Unknown/dynamic data (e.g. json.decode)
        ("text", "trim") => Some(1),
        ("text", "lines") => Some(1),
        ("text", "words") => Some(1),
        ("text", "split") => Some(2),
        ("text", "fields") => Some(1),
        ("text", "join") => Some(2),
        ("text", "replace") => Some(3),
        ("text", "starts_with") => Some(2),
        ("text", "ends_with") => Some(2),
        ("text", "wrap") => Some(2),
        ("text", "translate") => Some(3),
        ("text", "delete") => Some(2),
        ("text", "squeeze") => Some(1),
        ("text", "reverse") => Some(1),
        ("text", "count_lines") => Some(1),
        ("text", "count_words") => Some(1),
        ("text", "count_chars") => Some(1),
        ("text", "count_bytes") => Some(1),
        _ => None,
    }
}

/// Returns true if the expression is `.kind == "file"` — the inline Where
/// expression that `fs.walk |> where .kind == "file"` generates.
fn is_kind_eq_file_expr(arena: &AstArena, expr: ExprId) -> bool {
    let ArenaExprKind::Binary { op, left, right } = arena.expr(expr).kind else {
        return false;
    };
    if op != BinaryOp::Eq {
        return false;
    }
    let ArenaExprKind::Field { base, name } = arena.expr(left).kind else {
        return false;
    };
    if name != "kind" {
        return false;
    }
    if !matches!(arena.expr(base).kind, ArenaExprKind::Item) {
        return false;
    }
    matches!(arena.expr(right).kind, ArenaExprKind::Str(s) if arena.string_literal(s).as_ref() == "file")
}

fn is_fs_call(arena: &AstArena, callee: ExprId, method: &str) -> bool {
    is_module_call(arena, callee, "fs", method)
}

fn is_module_call(arena: &AstArena, callee: ExprId, module_name: &str, method: &str) -> bool {
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return false;
    };
    name == method
        && matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == module_name)
}

fn is_method_call(arena: &AstArena, callee: ExprId, method: &str) -> bool {
    matches!(arena.expr(callee).kind, ArenaExprKind::Field { name, .. } if name == method)
}

fn simple_command_value_expr(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(_) => true,
        ArenaExprKind::Field { base, .. } => simple_command_value_expr(arena, base),
        _ => false,
    }
}

fn is_predeclared_script_args(name: &str) -> bool {
    name == "args"
}

fn literal_command_word(arena: &AstArena, source: &str, arg: &ArenaCommandArg) -> Option<String> {
    let ArenaCommandArgKind::Word(parts) = arg.kind else {
        return None;
    };
    let parts: Vec<ArenaWordPart> = arena.word_parts(parts).collect();
    literal_command_word_parts(arena, source, &parts)
}

fn command_name(arena: &AstArena, source: &str, arg: &ArenaCommandArg) -> Option<String> {
    match arg.kind {
        ArenaCommandArgKind::Word(parts) => {
            let parts: Vec<ArenaWordPart> = arena.word_parts(parts).collect();
            match parts.as_slice() {
                [ArenaWordPart::Bare(text)] | [ArenaWordPart::Quoted(text)] => {
                    arena.text_value(text, source).map(str::to_string)
                }
                [ArenaWordPart::Shorthand(expr)] => {
                    if let ArenaExprKind::Ident(name) = arena.expr(*expr).kind {
                        Some(name.to_string())
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        ArenaCommandArgKind::SpliceName(name) => Some(name.to_string()),
        _ => None,
    }
}

fn literal_command_word_parts(
    arena: &AstArena,
    source: &str,
    parts: &[ArenaWordPart],
) -> Option<String> {
    match parts {
        [ArenaWordPart::Bare(value)] | [ArenaWordPart::Quoted(value)] => {
            arena.text_value(value, source).map(str::to_string)
        }
        _ => None,
    }
}

fn bare_command_word_parts(
    arena: &AstArena,
    source: &str,
    parts: &[ArenaWordPart],
) -> Option<String> {
    match parts {
        [ArenaWordPart::Bare(value)] => arena.text_value(value, source).map(str::to_string),
        _ => None,
    }
}

fn expects_nonzero_status(target: &str) -> bool {
    matches!(target, "false" | "grep" | "test" | "[" | "cmp" | "diff")
}

fn command_value_replacement(arena: &AstArena, expr: ExprId) -> String {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(name) => format!("${name}"),
        ArenaExprKind::Field { base, name } => {
            format!("{}.{name}", command_value_replacement(arena, base))
        }
        _ => String::new(),
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ImportSortKey {
    path: String,
    alias: Option<String>,
}

fn import_sort_key(arena: &AstArena, stmt: StmtId) -> ImportSortKey {
    let ArenaStmtKind::Use(use_id) = arena.stmt(stmt).kind else {
        unreachable!("import block contains only use statements");
    };
    let use_stmt = arena.use_stmt(use_id);
    ImportSortKey {
        path: arena
            .names(use_stmt.path)
            .map(|part| part.to_string())
            .collect::<Vec<_>>()
            .join("."),
        alias: use_stmt.alias.map(|name| name.to_string()),
    }
}

fn import_text(path: &str, alias: Option<&str>) -> String {
    let mut text = format!("use {path}");
    if let Some(alias) = alias {
        text.push_str(" as ");
        text.push_str(alias);
    }
    text
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum TopLevelPhase {
    Use,
    SafeConst,
    Type,
    Function,
    Body,
}

fn top_level_phase(arena: &AstArena, stmt: StmtId, source: &str) -> TopLevelPhase {
    let arena_stmt = arena.stmt(stmt);
    match arena_stmt.kind {
        ArenaStmtKind::Use(_) => TopLevelPhase::Use,
        ArenaStmtKind::TypeDef(_) => TopLevelPhase::Type,
        ArenaStmtKind::ProcDef(_)
        | ArenaStmtKind::CliMain(_)
        | ArenaStmtKind::PureDef(_)
        | ArenaStmtKind::SignalHook(_) => TopLevelPhase::Function,
        ArenaStmtKind::Let {
            target,
            initializer,
            ..
        }
        | ArenaStmtKind::Const {
            target,
            initializer,
            ..
        } if is_safe_top_level_const(arena, target, &initializer, arena_stmt.span, source) => {
            TopLevelPhase::SafeConst
        }
        ArenaStmtKind::Export(inner) => match arena.stmt(inner).kind {
            ArenaStmtKind::Let {
                target,
                initializer,
                ..
            }
            | ArenaStmtKind::Const {
                target,
                initializer,
                ..
            } if is_safe_top_level_const(arena, target, &initializer, arena_stmt.span, source) => {
                TopLevelPhase::SafeConst
            }
            ArenaStmtKind::TypeDef(_) => TopLevelPhase::Type,
            ArenaStmtKind::ProcDef(_)
            | ArenaStmtKind::PureDef(_)
            | ArenaStmtKind::SignalHook(_) => TopLevelPhase::Function,
            _ => TopLevelPhase::Body,
        },
        _ => TopLevelPhase::Body,
    }
}

fn is_safe_top_level_const(
    arena: &AstArena,
    target: BindingTargetId,
    initializer: &ArenaExprOrRun,
    span: Span,
    source: &str,
) -> bool {
    matches!(
        arena.binding_target(target).kind,
        ArenaBindingTargetKind::Name(_)
    ) && !source[span.start()..span.end()].contains('#')
        && matches!(initializer, ArenaExprOrRun::Expr(expr) if is_safe_const_expr(arena, *expr))
}

fn effects_annotation(effects: &FxHashSet<Effect>) -> String {
    let canonical = [
        Effect::Fs,
        Effect::Net,
        Effect::Process,
        Effect::Env,
        Effect::Time,
        Effect::Error,
        Effect::Io,
    ];
    canonical
        .iter()
        .filter(|e| effects.contains(*e))
        .map(|e| e.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn effects_covers_any(declared: &[Effect], required: &Effect) -> bool {
    declared.contains(required)
        || declared.contains(&Effect::Io)
            && matches!(
                required,
                Effect::Fs | Effect::Net | Effect::Process | Effect::Env
            )
}

fn binding_target_contains_name(
    arena: &AstArena,
    target: BindingTargetId,
    candidate: Name,
) -> bool {
    match arena.binding_target(target).kind {
        ArenaBindingTargetKind::Name(name) => name == candidate,
        ArenaBindingTargetKind::Record { fields, .. } => arena
            .destructure_fields(fields)
            .iter()
            .any(|field| binding_target_contains_name(arena, field.target, candidate)),
    }
}

fn format_binding_target(arena: &AstArena, target: BindingTargetId) -> String {
    match arena.binding_target(target).kind.clone() {
        ArenaBindingTargetKind::Name(name) => name.to_string(),
        ArenaBindingTargetKind::Record { fields, rest } => {
            let fields = arena.destructure_fields(fields).to_vec();
            let mut s = String::from("{");
            for (i, f) in fields.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(f.name.as_str().as_str());
                if !matches!(arena.binding_target(f.target).kind, ArenaBindingTargetKind::Name(name) if name == f.name)
                {
                    s.push_str(": ");
                    s.push_str(&format_binding_target(arena, f.target));
                }
            }
            if rest {
                if !fields.is_empty() {
                    s.push_str(", ");
                }
                s.push_str("..");
            }
            s.push('}');
            s
        }
    }
}

fn is_map_empty_call(arena: &AstArena, expr: ExprId) -> bool {
    let ArenaExprKind::Call { callee, args } = arena.expr(expr).kind else {
        return false;
    };
    if !args.is_empty() {
        return false;
    }
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return false;
    };
    name == "empty"
        && matches!(arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "map")
}

fn map_comp_key_can_be_bare(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(_) => true,
        ArenaExprKind::Field { base, .. } => map_comp_key_can_be_bare(arena, base),
        _ => false,
    }
}

/// The span of a named call argument, `name: value` or `name:`.
fn named_arg_span(arg: &ArenaCallArg) -> xsh::frontend::syntax::arena::SpanId {
    match arg.kind {
        ArenaCallArgKind::Named { span, .. }
        | ArenaCallArgKind::Splice { span, .. }
        | ArenaCallArgKind::NamedSpread { span, .. } => span,
        ArenaCallArgKind::Positional(_) => unreachable!("positional arguments have no entry span"),
    }
}

/// Whether a `.name` written at `offset` would begin a line that continues
/// the expression before it. A line beginning with `.name` joins the previous
/// line unless that line opened a bracket or ended an item.
fn leading_dot_continues_previous_line(source: &str, offset: usize) -> bool {
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    if !source[line_start..offset].trim().is_empty() {
        return false;
    }
    !matches!(
        source[..line_start].trim_end().chars().last(),
        None | Some('(' | '[' | '{' | ',')
    )
}

#[cfg(test)]
mod effect_fact_tests {
    use super::*;
    use std::cell::Cell;
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    #[test]
    fn checked_empty_effect_facts_avoid_duplicate_frontend_checking() {
        let source = "print \"ready\"\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty());
        for supplied in [false, true] {
            let checks = Cell::new(0);
            let output = Linter::lint_internal_with_effect_check(
                &parsed.arena,
                source,
                LintOptions {
                    function_effect_facts_checked: supplied,
                    ..LintOptions::default()
                },
                true,
                || {
                    checks.set(checks.get() + 1);
                    Checker::check_arena(&parsed.arena, source)
                },
            );
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
            assert_eq!(
                checks.get(),
                usize::from(!supplied),
                "supplied checked facts: {supplied}"
            );
        }
    }
}

#[cfg(test)]
mod annotation_probe_tests {
    use super::{LintOptions, Linter};
    use std::cell::Cell;
    use xsh::diagnostic::DiagnosticCode;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    thread_local! {
        static PROBES: Cell<usize> = const { Cell::new(0) };
    }

    pub(super) fn record_probe() {
        PROBES.with(|probes| probes.set(probes.get() + 1));
    }

    #[test]
    fn annotation_probes_stop_after_original_signature_cannot_be_checked() {
        let mut source = "use missing\n".to_owned();
        for index in 0..12 {
            source.push_str(&format!(
                "pure choose_{index}(value: Int = 1) -> Int {{ value }}\n"
            ));
        }
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty());
        PROBES.with(|probes| probes.set(0));
        let output = Linter::lint(
            &parsed.arena,
            &source,
            LintOptions {
                prefer_inferred_pure_returns: true,
                function_effect_facts_checked: true,
                ..LintOptions::default()
            },
        );
        assert!(!output.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.code.map(DiagnosticCode::name),
            Some("lint.prefer-inferred-pure-return" | "lint.default-param-type")
        )));
        assert_eq!(
            PROBES.with(Cell::get),
            1,
            "a failed baseline makes every candidate unprovable"
        );
    }

    // A union keeps its members in a side table, so its data words name no
    // child type. Whether the annotation is all builtin is read from the
    // members, wherever the union sits among the file's type expressions.
    #[test]
    fn a_union_return_is_judged_by_its_members() {
        let reported = |source: &str| {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            Linter::lint(
                &parsed.arena,
                source,
                LintOptions {
                    prefer_inferred_pure_returns: true,
                    function_effect_facts_checked: true,
                    ..LintOptions::default()
                },
            )
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferInferredPureReturn))
        };
        // The file's first type expression names a user type.
        assert!(reported(
            "type Alias = Other\n\ntype Other = {name: Str}\n\npure pass(value: Union[Int, Str]) -> Union[Int, Str] {\n  value\n}\n\nlet shown: Alias = {name: \"x\"}\nprint ${pass(1) == 1} ${shown.name}\n"
        ));
        // The file's first type expression is builtin; the union is not.
        assert!(!reported(
            "type Other = {name: Str}\n\npure pass(value: Union[Int, Other]) -> Union[Int, Other] {\n  value\n}\n\nprint ${pass(1) == 1}\n"
        ));
    }
}

#[cfg(test)]
mod nested_pipeline_index_tests {
    use super::{LintOptions, Linter};
    use std::cell::Cell;
    use xsh::diagnostic::DiagnosticCode;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    thread_local! {
        static STATEMENTS: Cell<usize> = const { Cell::new(0) };
    }

    pub(super) fn record_statement() {
        STATEMENTS.with(|statements| statements.set(statements.get() + 1));
    }

    #[test]
    fn nested_pipeline_statement_membership_is_indexed_once() {
        let mut source = "pure identity(value: Int) -> Int { value }\n".to_owned();
        for index in 0..64 {
            source.push_str(&format!("let selected_{index} = identity({index})\n"));
        }
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty());
        STATEMENTS.with(|statements| statements.set(0));
        let output = Linter::lint(
            &parsed.arena,
            &source,
            LintOptions {
                function_effect_facts_checked: true,
                ..LintOptions::default()
            },
        );
        assert!(
            !output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferValuePipeline))
        );
        assert_eq!(
            STATEMENTS.with(Cell::get),
            parsed.arena.arena.stmt_tags.len(),
            "whole statement membership must require one scan for every eligible expression"
        );
    }
}

#[cfg(test)]
mod local_annotation_probe_tests {
    use super::{LintOptions, Linter};
    use std::cell::Cell;
    use xsh::diagnostic::DiagnosticCode;
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    thread_local! {
        static ORIGINALS: Cell<usize> = const { Cell::new(0) };
        static CANDIDATES: Cell<usize> = const { Cell::new(0) };
        static ORIGINAL_CHECKS: Cell<usize> = const { Cell::new(0) };
    }

    pub(super) fn record_original_check() {
        ORIGINAL_CHECKS.with(|checks| checks.set(checks.get() + 1));
    }

    pub(super) fn record_original() {
        ORIGINALS.with(|originals| originals.set(originals.get() + 1));
    }

    pub(super) fn record_candidate() {
        CANDIDATES.with(|candidates| candidates.set(candidates.get() + 1));
    }

    #[test]
    fn exported_nominal_local_annotations_keep_their_declared_domain_without_probes() {
        let source = "##! Rows.\n## A row.\nexport type Row = {value: Int}\n## Produces rows.\nexport pure rows() -> List[Row] {\n  var values: List[Row] = []\n  values = values.push(Row(value: 1))\n  values\n}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty());
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        ORIGINALS.with(|originals| originals.set(0));
        CANDIDATES.with(|candidates| candidates.set(0));
        let output = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                function_effect_facts_checked: true,
                expr_types: checked.expr_types,
                function_return_types: checked.function_return_types,
                ..LintOptions::default()
            },
        );
        assert!(
            !output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintNeedlessAnnotation)),
            "{:?}",
            output.diagnostics
        );
        assert_eq!(ORIGINALS.with(Cell::get), 0);
        assert_eq!(CANDIDATES.with(Cell::get), 0);
    }

    #[test]
    fn standalone_annotation_import_availability_matches_checker_use_errors() {
        for import in [
            "",
            "use json\n",
            "use missing\n",
            "use json.nested\n",
            "use json as renamed\n",
        ] {
            let source = format!("{import}pure count() -> Int {{ 1 }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            let available = super::standalone_annotation_imports_available(&parsed.arena);
            assert_eq!(
                available,
                !checked
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.severity == xsh::diagnostic::Severity::Error),
                "{import}: {:?}",
                checked.diagnostics
            );
        }
    }

    #[test]
    fn local_annotation_probes_cache_original_facts_and_failure() {
        for missing_import in [false, true] {
            let mut source = if missing_import {
                "use missing\n".to_owned()
            } else {
                String::new()
            };
            source.push_str("pure count() -> Int {\n");
            for index in 0..12 {
                source.push_str(&format!(
                    "var values_{index}: List[Int] = []\nvalues_{index} = values_{index}.push(1)\n"
                ));
            }
            source.push_str("values_0.len()\n}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty());
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert_eq!(
                checked.diagnostics.is_empty(),
                !missing_import,
                "{:?}",
                checked.diagnostics
            );
            ORIGINALS.with(|originals| originals.set(0));
            CANDIDATES.with(|candidates| candidates.set(0));
            ORIGINAL_CHECKS.with(|checks| checks.set(0));
            let output = Linter::lint(
                &parsed.arena,
                &source,
                LintOptions {
                    function_effect_facts_checked: true,
                    ..LintOptions::default()
                },
            );
            if missing_import {
                assert!(
                    !output.diagnostics.iter().any(|diagnostic| diagnostic.code
                        == Some(DiagnosticCode::LintNeedlessAnnotation))
                );
            }
            assert_eq!(
                ORIGINALS.with(Cell::get),
                1,
                "the original source is unchanged between candidates"
            );
            assert_eq!(
                CANDIDATES.with(Cell::get),
                if missing_import { 0 } else { 12 }
            );
            assert_eq!(
                ORIGINAL_CHECKS.with(Cell::get),
                usize::from(!missing_import),
                "an unresolved standalone import already proves the original unavailable"
            );
        }
    }
}

#[cfg(test)]
mod user_type_reference_tests {
    use super::{LintOptions, Linter};
    use xsh::diagnostic::DiagnosticCode;
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    #[test]
    fn applied_user_type_references_keep_the_generic_head_and_nominal_arguments() {
        let source = "##! Collected rows.\ntype Row = {value: Int}\ntype CollectedDevices[T] = {values: List[T]}\ntype Unused = {value: Str}\n## Counts rows.\nexport pure count(rows: CollectedDevices[Row]) -> Int { rows.values.len() }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let output = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                function_effect_facts_checked: true,
                ..LintOptions::default()
            },
        );
        let unused = output
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintUnusedType))
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>();
        assert_eq!(unused, vec!["unused type declaration `Unused`"]);
    }
}
