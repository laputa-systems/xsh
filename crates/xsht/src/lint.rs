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

#[path = "lint_prefer_tempdir.rs"]
mod prefer_tempdir;
#[path = "lint_prefer_atomically.rs"]
mod prefer_atomically;
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

#[path = "lint_path_display_sink.rs"]
mod lint_path_display_sink;

#[path = "lint_write_lines.rs"]
mod lint_write_lines;

#[path = "lint_read_lines.rs"]
mod lint_read_lines;
#[path = "lint_size_literal.rs"]
mod lint_size_literal;
#[path = "lint_list_any_union.rs"]
mod lint_list_any_union;

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

#[path = "lint_prefer_propagation.rs"]
mod lint_prefer_propagation;
#[path = "lint_prefer_for_index.rs"]
mod lint_prefer_for_index;

#[path = "lint_env_path_list.rs"]
mod lint_env_path_list;

#[path = "lint_write_mode.rs"]
mod lint_write_mode;
#[path = "lint_prefer_fail.rs"]
mod lint_prefer_fail;
#[path = "lint_prefer_test_expect.rs"]
mod lint_prefer_test_expect;

#[path = "lint_path_kind.rs"]
mod lint_path_kind;

#[path = "lint_fs_method.rs"]
mod lint_fs_method;

#[path = "lint_prefer_typed_callable.rs"]
mod lint_prefer_typed_callable;
#[path = "lint_explicit_run_capture.rs"]
mod lint_explicit_run_capture;
#[path = "lint_explicit_missing_ok.rs"]
mod lint_explicit_missing_ok;

#[cfg(test)]
#[path = "lint_literal_migration_tests.rs"]
mod literal_migration_tests;

use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, BTreeSet};
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label, Severity};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::symbols::Name;
use xsh::frontend::symbols::Symbol;
use xsh::frontend::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCallArg,
    ArenaCallArgKind, ArenaCommand, ArenaCommandArg, ArenaCommandArgKind, ArenaCompQualifier,
    ArenaEnvAssignment, ArenaEnvAssignmentValue, ArenaExpr, ArenaExprKind, ArenaExprOrRun,
    ArenaFmtPart, ArenaFunctionDef, ArenaMatchExprArm, ArenaModuleContractEntryKind,
    ArenaPatternKind, ArenaPipeStage, ArenaPipeStageKind, ArenaProgram, ArenaRange,
    ArenaRecordField, ArenaRecordFieldKind, ArenaRedirection, ArenaRedirectionTarget,
    ArenaSpawnTarget, ArenaStmtKind, ArenaStreamStage, ArenaSugar, ArenaSugarOperand, ArenaTypeDefBody, ArenaTypeExprTag, SugarForm,
    ArenaWordPart, AssignTargetId, AstArena, BindingTargetId, BlockId, BuilderBlockId,
    CommandStmtId, DeferTrigger, ExprId, FunctionDefId, PatternId, RunFormId, StmtId, TypeExprId,
};
use xsh::frontend::syntax::node::{
    AssignOp, BinaryOp, CoreCommand, Effect, RunKind, StreamStageKind, UnaryOp,
    parse_command_word_reference,
};

fn list_splice_element_type_is_precise(ty: &Type) -> bool {
    match ty {
        Type::Bool
        | Type::Int
        | Type::Float
        | Type::Duration
        | Type::Str
        | Type::Bytes
        | Type::Path => true,
        Type::List(item) => list_splice_element_type_is_precise(item),
        _ => false,
    }
}

fn pipeline_argument_expr(arg: &ArenaCallArg) -> Option<ExprId> {
    match arg.kind {
        ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. } => Some(value),
        _ => None,
    }
}

fn list_update_argument_stable(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Null
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Regex(_) => true,
        ArenaExprKind::List(items) => arena
            .list_element_exprs(items)
            .all(|item| list_update_argument_stable(arena, item)),
        _ => false,
    }
}

/// Whether `text` spells `name` as a whole identifier. A nested block can
/// assign a binding only by writing its name, so a missing spelling proves it
/// leaves the binding alone; a spelling inside a string or comment merely errs
/// toward the conservative answer.
fn mentions_identifier(text: &str, name: &str) -> bool {
    let identifier_char = |ch: char| ch.is_alphanumeric() || ch == '_';
    text.match_indices(name).any(|(start, _)| {
        !text[..start].chars().next_back().is_some_and(identifier_char)
            && !text[start + name.len()..]
                .chars()
                .next()
                .is_some_and(identifier_char)
    })
}

/// One `push` or `extend` call in a chain that appends to a list, with the
/// parenthesized argument text's span.
struct ListUpdate {
    method: Name,
    argument: ExprId,
    parenthesized: Span,
}

/// Whether evaluating `expr` can assign the variable spelled `name`.
///
/// `x = x.push(a)` reads `x` before it evaluates `a`, and `x += [a]` reads it
/// after, so the two spellings agree exactly when `a` leaves `x` alone. A
/// local variable is visible only to its own callable, and an expression can
/// assign it only through statements in a block nested inside the expression
/// (a callback, a `try` or value block, a builder or command interpolation);
/// calls cannot reach it, because nested declarations are rejected and a
/// callable value never captures a caller's local. Module-level variables are
/// not covered: any proc call may assign them. `text` is the source of the
/// whole expression, which stands in for forms whose own span does not cover
/// everything they evaluate (commands and builders).
fn expr_may_assign_local(
    arena: &AstArena,
    source: &str,
    expr: ExprId,
    name: &str,
    text: &str,
) -> bool {
    let nested_statements = matches!(
        arena.expr(expr).kind,
        ArenaExprKind::Run(_)
            | ArenaExprKind::Spawn(_)
            | ArenaExprKind::Wait(_)
            | ArenaExprKind::BuilderCall { .. }
    ) && mentions_identifier(text, name);
    nested_statements
        || expr_child_blocks(arena, expr).into_iter().any(|block| {
            source
                .get(arena.span(arena.block(block).span).range())
                .is_none_or(|block_text| mentions_identifier(block_text, name))
        })
        || expr_child_exprs(arena, expr)
            .into_iter()
            .any(|child| expr_may_assign_local(arena, source, child, name, text))
}

/// Whether replacing `span` could drop a comment. Unlexable text counts as
/// commented so callers stay conservative.
fn span_may_contain_comment(source: &str, span: Span) -> bool {
    let Some(text) = source.get(span.range()) else {
        return true;
    };
    let lexed = xsh::frontend::syntax::lexer::Lexer::new(span.source_id, text).lex_compact();
    !lexed.diagnostics.is_empty()
        || (0..lexed.token_table.len()).any(|index| {
            lexed.token_table.tag_at(index) == Some(xsh::frontend::syntax::token::TokenTag::Comment)
        })
}

/// Fixes build replacements with conservative grouping; removes each pair
/// that `check.redundant-parens` would reject where the fix lands, so applied
/// fixes keep exactly the parentheses the parser needs.
fn minimize_fix_grouping(program: &ArenaProgram, source: &str, diagnostics: &mut [Diagnostic]) {
    let Some(source_id) = program
        .statement_ids()
        .next()
        .map(|id| program.arena.stmt(id).span.source_id)
    else {
        return;
    };
    for diagnostic in diagnostics {
        if !diagnostic.fix_hints.iter().any(|hint| {
            hint.replacement
                .as_deref()
                .is_some_and(|text| text.contains('('))
        }) {
            continue;
        }
        let mut edits: Vec<(usize, Span, String)> = Vec::new();
        for (index, hint) in diagnostic.fix_hints.iter().enumerate() {
            let (Some(span), Some(replacement)) = (hint.span, hint.replacement.as_ref()) else {
                continue;
            };
            if span.source_id == source_id && source.get(span.range()).is_some() {
                edits.push((index, span, replacement.clone()));
            }
        }
        edits.sort_by_key(|(_, span, _)| (span.start(), span.end()));
        if edits
            .windows(2)
            .any(|pair| pair[1].1.start() < pair[0].1.end())
        {
            continue;
        }
        let mut text = String::with_capacity(source.len());
        let mut ranges = Vec::with_capacity(edits.len());
        let mut cursor = 0;
        for (_, span, replacement) in &edits {
            text.push_str(&source[cursor..span.start()]);
            ranges.push(text.len()..text.len() + replacement.len());
            text.push_str(replacement);
            cursor = span.end();
        }
        text.push_str(&source[cursor..]);
        let (minimal, ranges) =
            xsh::frontend::syntax::grouping::remove_redundant_parens(&text, &ranges);
        for ((index, _, _), range) in edits.iter().zip(ranges) {
            diagnostic.fix_hints[*index].replacement = Some(minimal[range].to_owned());
        }
    }
}

/// Widens `span` over grouping parentheses that enclose exactly it, so an edit
/// that yields an atomic literal does not depend on redundant source grouping.
/// Call and index parentheses, and groups holding comments, are never widened.
fn widen_over_grouping(source: &str, mut span: Span) -> Span {
    use xsh::frontend::syntax::token::TokenTag;
    let tokens = xsh::frontend::syntax::lexer::Lexer::new(span.source_id, source)
        .lex_compact()
        .token_table;
    let tag = |index: usize| tokens.tag_at(index);
    loop {
        let Some(first) =
            (0..tokens.len()).find(|&index| tokens.start_at(index) == Some(span.start()))
        else {
            return span;
        };
        let Some(last) =
            (first..tokens.len()).find(|&index| tokens.end_at(index, source) == Some(span.end()))
        else {
            return span;
        };
        let Some(open) = (0..first)
            .rev()
            .find(|&index| tag(index) != Some(TokenTag::Newline))
        else {
            return span;
        };
        let Some(close) =
            (last + 1..tokens.len()).find(|&index| tag(index) != Some(TokenTag::Newline))
        else {
            return span;
        };
        if tag(open) != Some(TokenTag::LParen) || tag(close) != Some(TokenTag::RParen) {
            return span;
        }
        let callee = (0..open)
            .rev()
            .find(|&index| tag(index) != Some(TokenTag::Newline))
            .and_then(tag);
        if matches!(
            callee,
            Some(
                TokenTag::Ident
                    | TokenTag::ProcIdent
                    | TokenTag::DollarIdent
                    | TokenTag::RParen
                    | TokenTag::RBracket
                    | TokenTag::RBrace
                    | TokenTag::Question
                    | TokenTag::String
                    | TokenTag::FmtString
                    | TokenTag::PathString
                    | TokenTag::PathFmtString
            )
        ) {
            return span;
        }
        let (Some(start), Some(end)) = (tokens.start_at(open), tokens.end_at(close, source)) else {
            return span;
        };
        span = Span::new(span.source_id, start, end);
    }
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
    /// Opt in to `lint.prefer-inferred-variant`, which a corpus adopts once
    /// it migrates its qualified variants.
    pub prefer_inferred_variants: bool,
    /// Opt in to `lint.prefer-positional-constructor`.
    pub prefer_positional_constructors: bool,
    /// Opt in to `lint.prefer-implicit-message`, which a corpus adopts once
    /// it migrates its `Variant(message: Str)` declarations.
    pub prefer_implicit_messages: bool,
    /// Opt in to `lint.prefer-inferred-proc-return`.
    pub prefer_inferred_proc_returns: bool,
    /// Opt in to `lint.prefer-typed-callable`.
    pub prefer_typed_callables: bool,
    /// Opt in to `lint.explicit-missing-ok`, which a corpus turns on once,
    /// before the default of `remove` changes.
    pub explicit_missing_ok: bool,
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
    pub terminating_call_spans: BTreeSet<Span>,
    pub assertion_effect_spans: BTreeSet<Span>,
    pub statement_expression_spans: BTreeSet<Span>,
    /// Expression statements whose `Result[Unit]` value propagates instead of
    /// becoming a body's value; empty without checked facts.
    pub propagating_statements: BTreeSet<Span>,
    /// `expr?` in a control position of a condition, where the position
    /// propagates the `Result[Bool]` without the `?`; empty without checked
    /// facts.
    pub redundant_condition_propagations: BTreeSet<Span>,
    /// Value-position run forms whose `Result` is the value and that are not
    /// written under `try`; empty without checked facts.
    pub implicitly_captured_runs: BTreeSet<Span>,
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
            prefer_inferred_variants: false,
            prefer_positional_constructors: false,
            prefer_implicit_messages: false,
            prefer_inferred_proc_returns: false,
            prefer_typed_callables: false,
            explicit_missing_ok: false,
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
            terminating_call_spans: BTreeSet::default(),
            assertion_effect_spans: BTreeSet::default(),
            statement_expression_spans: BTreeSet::default(),
            propagating_statements: BTreeSet::default(),
            redundant_condition_propagations: BTreeSet::default(),
            implicitly_captured_runs: BTreeSet::default(),
            membership_migration_spans: BTreeSet::default(),
            standard_call_spans: BTreeMap::default(),
            statically_resolved_call_spans: BTreeSet::default(),
            definitely_exiting_block_spans: BTreeSet::default(),
            redundant_variant_qualifiers: BTreeMap::default(),
            dead_code: true,
            native_test_file: false,
            only: None,
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
    record_constructors: xsh::frontend::check::RecordConstructors,
    prefer_inferred_pure_returns: bool,
    prefer_inferred_private_effects: bool,
    prefer_env_string: bool,
    prefer_item_shorthand: bool,
    prefer_tempdir_scope: bool,
    prefer_inferred_variants: bool,
    prefer_positional_constructors: bool,
    prefer_inferred_proc_returns: bool,
    prefer_typed_callables: bool,
    explicit_missing_ok: bool,
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
    checked_effects: BTreeMap<
        xsh::frontend::check::EffectDeclarationId,
        xsh::frontend::check::FunctionEffectFact,
    >,
    terminating_call_spans: BTreeSet<Span>,
    assertion_effect_spans: BTreeSet<Span>,
    statement_expression_spans: BTreeSet<Span>,
    membership_migration_spans: BTreeSet<Span>,
    standard_call_spans: BTreeMap<Span, (String, String)>,
    statement_call_spans: BTreeMap<Span, Span>,
    whole_statement_call_spans: BTreeMap<Span, Span>,
    whole_statement_values: std::sync::OnceLock<FxHashSet<ExprId>>,
    guarded_statement_depth: usize,
    propagating_statements: BTreeSet<Span>,
    redundant_condition_propagations: BTreeSet<Span>,
    implicitly_captured_runs: BTreeSet<Span>,
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
        // The rules that read type expressions treat a union and a callable
        // type as they treat any type they cannot see into.
        ArenaTypeExprTag::Applied
        | ArenaTypeExprTag::Qualified
        | ArenaTypeExprTag::Union
        | ArenaTypeExprTag::Callable => ArenaTypeExprKind::Qualified,
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
        let prefer_implicit_messages = options.prefer_implicit_messages;
        let only = options.only;
        let checked_effects =
            if options.function_effect_facts.is_empty() && !options.function_effect_facts_checked {
                check_effects().function_effect_facts
            } else {
                options.function_effect_facts
            };
        let mut linter = Self {
            record_constructors: xsh::frontend::check::RecordConstructors::collect(program),
            arena: &program.arena,
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
            prefer_inferred_pure_returns: options.prefer_inferred_pure_returns,
            prefer_inferred_private_effects: options.prefer_inferred_private_effects,
            prefer_env_string: options.prefer_env_string,
            prefer_item_shorthand: options.prefer_item_shorthand,
            prefer_tempdir_scope: options.prefer_tempdir_scope,
            prefer_inferred_variants: options.prefer_inferred_variants,
            prefer_positional_constructors: options.prefer_positional_constructors,
            // Naming the rule asks for it as the setting does, so its sites
            // can be counted without a configuration file.
            explicit_missing_ok: options.explicit_missing_ok
                || only.as_deref().is_some_and(|only| {
                    only.contains(&DiagnosticCode::LintExplicitMissingOk)
                }),
            prefer_inferred_proc_returns: options.prefer_inferred_proc_returns,
            // Naming the rule in `--only` asks for it as the setting does.
            prefer_typed_callables: options.prefer_typed_callables
                || only.as_deref().is_some_and(|only| {
                    only.contains(&DiagnosticCode::LintPreferTypedCallable)
                }),
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
            checked_effects,
            terminating_call_spans: options.terminating_call_spans,
            assertion_effect_spans: options.assertion_effect_spans,
            statement_expression_spans: options.statement_expression_spans,
            membership_migration_spans: options.membership_migration_spans,
            standard_call_spans: options.standard_call_spans,
            statement_call_spans: BTreeMap::new(),
            whole_statement_call_spans: BTreeMap::new(),
            whole_statement_values: std::sync::OnceLock::new(),
            guarded_statement_depth: 0,
            propagating_statements: options.propagating_statements,
            redundant_condition_propagations: options.redundant_condition_propagations,
            implicitly_captured_runs: options.implicitly_captured_runs,
            propagation_function: None,
            propagation_boundary_depth: 0,
            negated_call_spans: BTreeMap::new(),
            size_products: lint_size_literal::SizeProducts::default(),
            statically_resolved_call_spans: options.statically_resolved_call_spans,
            definitely_exiting_block_spans: options.definitely_exiting_block_spans,
            redundant_variant_qualifiers: options.redundant_variant_qualifiers,
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
        if linter.prefer_item_shorthand {
            linter
                .diagnostics
                .extend(item_shorthand::lint_item_shorthand(program, source));
        }
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
        if linter.prefer_inferred_variants {
            let patterns = lint_inferred_variant_pattern::lint_inferred_variant_patterns(
                program,
                source,
                &linter.redundant_variant_qualifiers,
            );
            linter.diagnostics.extend(patterns);
        }
        if prefer_implicit_messages {
            linter
                .diagnostics
                .extend(lint_implicit_message::lint_implicit_messages(
                    program, source,
                ));
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
        linter
            .diagnostics
            .extend(lint_prefer_match_else::lint_wildcard_catch_all_arms(
                program, source,
            ));
        let list_any_bindings = std::mem::take(&mut linter.list_any_bindings);
        linter.diagnostics.extend(list_any_bindings.finish());
        let fail_candidates = std::mem::take(&mut linter.fail_candidates);
        linter
            .diagnostics
            .extend(fail_candidates.finish(&program.arena, source));
        if linter.prefer_typed_callables {
            let callable_parameters = std::mem::take(&mut linter.callable_parameters);
            let reports = callable_parameters.finish(
                linter.arena,
                source,
                &linter.checked_function_returns,
                &|body| linter.checked_effect_fact(body).cloned(),
            );
            linter.diagnostics.extend(reports);
        }
        linter
            .diagnostics
            .retain(|diagnostic| lint_code_selected(only.as_deref(), diagnostic.code));
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

    fn collect_assigned_names(&mut self, statements: &[StmtId]) {
        for &stmt in statements {
            self.collect_assigned_names_stmt(stmt);
        }
    }

    fn collect_assigned_names_block(&mut self, block: BlockId) {
        let statements = self.arena.block(block).statements;
        for stmt in self.arena.stmt_ids(statements).collect::<Vec<_>>() {
            self.collect_assigned_names_stmt(stmt);
        }
    }

    fn collect_assigned_names_stmt(&mut self, stmt_id: StmtId) {
        match self.arena.stmt(stmt_id).kind {
            ArenaStmtKind::Export(inner) => self.collect_assigned_names_stmt(inner),
            ArenaStmtKind::Assign { target, .. } => self.collect_assigned_names_target(target),
            ArenaStmtKind::ProcDef(def)
            | ArenaStmtKind::CliMain(def)
            | ArenaStmtKind::PureDef(def)
            | ArenaStmtKind::StreamDef(def) => {
                self.collect_assigned_names_block(self.arena.function_def(def).body);
            }
            ArenaStmtKind::SignalHook(hook) => {
                self.collect_assigned_names_block(self.arena.signal_hook(hook).body);
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                for branch in self.arena.if_branches(branches).to_vec() {
                    self.collect_assigned_names_block(branch.block);
                }
                if let Some(block) = else_block {
                    self.collect_assigned_names_block(block);
                }
            }
            ArenaStmtKind::While { block, .. }
            | ArenaStmtKind::For { block, .. }
            | ArenaStmtKind::Loop { block } => self.collect_assigned_names_block(block),
            ArenaStmtKind::Sugar { operands, .. } => {
                for operand in self.arena.sugar_operands(operands).to_vec() {
                    match operand {
                        ArenaSugarOperand::Block(block) => self.collect_assigned_names_block(block),
                        ArenaSugarOperand::Stmt(stmt) => self.collect_assigned_names_stmt(stmt),
                        _ => {}
                    }
                }
            }
            ArenaStmtKind::With {
                body, else_block, ..
            } => {
                self.collect_assigned_names_block(body);
                self.collect_assigned_names_block(else_block);
            }
            ArenaStmtKind::Guard { else_block, .. } => {
                self.collect_assigned_names_block(else_block);
            }
            ArenaStmtKind::Match { arms, .. } => {
                for arm in self.arena.match_arms(arms).to_vec() {
                    self.collect_assigned_names_block(arm.block);
                }
            }
            ArenaStmtKind::Use(_)
            | ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_)
            | ArenaStmtKind::Let { .. }
            | ArenaStmtKind::Const { .. }
            | ArenaStmtKind::Var { .. }
            | ArenaStmtKind::Return(_)
            | ArenaStmtKind::YieldDelegate(_)
            | ArenaStmtKind::Exit(_)
            | ArenaStmtKind::Yield(_)
            | ArenaStmtKind::Defer(..)
            | ArenaStmtKind::Break { .. }
            | ArenaStmtKind::Continue
            | ArenaStmtKind::Command(_)
            | ArenaStmtKind::TailBareIdent(_)
            | ArenaStmtKind::Assert { .. }
            | ArenaStmtKind::Expr(_) => {}
        }
    }

    fn collect_assigned_names_target(&mut self, target: AssignTargetId) {
        match self.arena.assign_target(target).kind {
            ArenaAssignTargetKind::Name(name) => {
                self.assigned_names.insert(name);
            }
            ArenaAssignTargetKind::Env(_) => {}
            ArenaAssignTargetKind::Field { base, .. }
            | ArenaAssignTargetKind::Index { base, .. } => {
                self.collect_assigned_names_target(base);
            }
        }
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

    fn collect_type_expr_refs(&mut self, ty: TypeExprId) {
        if self.arena.type_expr_tags[ty.index()] == ArenaTypeExprTag::Applied {
            let base = TypeExprId::from_index(self.arena.type_expr_data[ty.index()].lhs as usize);
            self.collect_type_expr_refs(base);
            let arguments = self.arena.applied_type_arguments(ty).collect::<Vec<_>>();
            for argument in arguments {
                self.collect_type_expr_refs(argument);
            }
            return;
        }
        match type_expr_kind(self.arena, ty) {
            ArenaTypeExprKind::Named(name) => {
                self.used_type_names.insert(name.to_string());
            }
            ArenaTypeExprKind::Qualified => {}
            ArenaTypeExprKind::List(inner)
            | ArenaTypeExprKind::Stream(inner)
            | ArenaTypeExprKind::Module(inner)
            | ArenaTypeExprKind::Optional(inner) => self.collect_type_expr_refs(inner),
            ArenaTypeExprKind::Map(key, value) => {
                if let Some(key) = key {
                    self.collect_type_expr_refs(key);
                }
                self.collect_type_expr_refs(value);
            }
            ArenaTypeExprKind::Result { ok, err } => {
                self.collect_type_expr_refs(ok);
                if let Some(err) = err {
                    self.collect_type_expr_refs(err);
                }
            }
        }
    }

    fn collect_type_def_refs(&mut self, body: &ArenaTypeDefBody) {
        match body {
            ArenaTypeDefBody::Alias(ty) => self.collect_type_expr_refs(*ty),
            ArenaTypeDefBody::RecordSchema(fields) => {
                for field in self.arena.schema_fields(*fields).to_vec() {
                    self.collect_type_expr_refs(field.ty);
                    if let Some(default) = field.default {
                        self.lint_expr(default);
                    }
                }
            }
            ArenaTypeDefBody::ModuleContract { entries, .. } => {
                for entry in self.arena.module_contract_entries(*entries).to_vec() {
                    match &entry.kind {
                        ArenaModuleContractEntryKind::Value(ty) => self.collect_type_expr_refs(*ty),
                        ArenaModuleContractEntryKind::Proc {
                            params, return_ty, ..
                        }
                        | ArenaModuleContractEntryKind::Pure { params, return_ty } => {
                            for param in self.arena.params(*params).to_vec() {
                                self.collect_type_expr_refs(param.ty);
                            }
                            self.collect_type_expr_refs(*return_ty);
                        }
                    }
                }
            }
            ArenaTypeDefBody::TagUnion(variants) => {
                for variant in self.arena.tag_variants(*variants).to_vec() {
                    if let Some(value) = variant.wire_value {
                        self.lint_expr(value);
                    }
                    for ty in self.arena.extra_range(variant.fields).to_vec() {
                        self.collect_type_expr_refs(TypeExprId::from_index(ty as usize));
                    }
                }
            }
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

    /// Lints a proc or pure body as one whose statements may trade
    /// `return Err(e)` for `?`.
    fn lint_propagating_function(
        &mut self,
        definition: FunctionDefId,
        pure: bool,
        lint: impl FnOnce(&mut Self),
    ) {
        let allowed = lint_prefer_propagation::propagation_allowed(
            self.arena,
            &self.checked_effects,
            definition,
            pure,
        );
        let function = self.propagation_function.replace(allowed);
        let depth = std::mem::take(&mut self.propagation_boundary_depth);
        lint(self);
        self.propagation_function = function;
        self.propagation_boundary_depth = depth;
    }

    fn lint_stmt(&mut self, stmt_id: StmtId, exported: bool) {
        let stmt = self.arena.stmt(stmt_id);
        self.lint_propagation(stmt_id);
        self.fail_candidates.visit_stmt(self.arena, stmt_id);
        match stmt.kind {
            ArenaStmtKind::Use(_) | ArenaStmtKind::TypeDef(_) | ArenaStmtKind::ErrorDef(_) => {}
            ArenaStmtKind::Export(inner) => self.lint_stmt(inner, true),
            ArenaStmtKind::Let {
                target,
                ty,
                initializer,
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer,
            } => {
                self.lint_record_constructor(ty, &initializer);
                self.lint_empty_map_initializer(ty, &initializer);
                if let (Some(_), ArenaExprOrRun::Expr(value)) = (ty, &initializer) {
                    self.size_products.typed_initializer(self.arena, *value);
                }
                if let Some(type_expr) = ty {
                    self.collect_type_expr_refs(type_expr);
                    self.lint_needless_annotation(
                        target,
                        false,
                        type_expr,
                        &initializer,
                        exported,
                        stmt.span,
                    );
                }
                self.lint_expr_or_run(&initializer);
                let absence_lookup = match initializer {
                    ArenaExprOrRun::Expr(value) => self.proven_absence_lookup(value),
                    _ => false,
                };
                let materialized_record = match initializer {
                    ArenaExprOrRun::Expr(value) => self.proven_materialized_record(value),
                    _ => false,
                };
                let immutable_byte_length = match initializer {
                    ArenaExprOrRun::Expr(value) => self.proven_immutable_byte_length(value),
                    _ => None,
                };
                self.define_binding_target(target, stmt.span, true);
                self.declare_list_any_binding(target, ty, &initializer);
                if let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind
                    && let Some(binding) = self
                        .scopes
                        .last_mut()
                        .and_then(|scope| scope.get_mut(name.as_str().as_str()))
                {
                    binding.comparison_stable = true;
                    binding.absence_lookup = absence_lookup;
                    binding.materialized_record = materialized_record;
                    binding.immutable_byte_length = immutable_byte_length;
                }
            }
            ArenaStmtKind::Var {
                target,
                ty,
                initializer,
            } => {
                self.lint_record_constructor(ty, &initializer);
                self.lint_empty_map_initializer(ty, &initializer);
                if let (Some(_), ArenaExprOrRun::Expr(value)) = (ty, &initializer) {
                    self.size_products.typed_initializer(self.arena, *value);
                }
                if let Some(type_expr) = ty {
                    self.collect_type_expr_refs(type_expr);
                    self.lint_needless_annotation(
                        target,
                        true,
                        type_expr,
                        &initializer,
                        exported,
                        stmt.span,
                    );
                }
                self.lint_expr_or_run(&initializer);
                self.define_binding_target(target, stmt.span, true);
                self.declare_list_any_binding(target, ty, &initializer);
                if let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind
                    && let Some(binding) = self
                        .scopes
                        .last_mut()
                        .and_then(|scope| scope.get_mut(name.as_str().as_str()))
                {
                    binding.mutable = true;
                }
            }
            ArenaStmtKind::Assign { target, op, value } => {
                if op == AssignOp::Set {
                    self.lint_list_compound_assignment(target, value, stmt.span);
                }
                if let Some(definition) = lint_list_any_union::assigned_root(self.arena, target)
                    .and_then(|root| self.binding_definition(root))
                {
                    let mut bindings = std::mem::take(&mut self.list_any_bindings);
                    bindings.assign(
                        self.arena,
                        definition,
                        target,
                        op,
                        &value,
                        &self.expr_types,
                        &|name| self.binding_definition(name),
                    );
                    self.list_any_bindings = bindings;
                }
                self.lint_assign_target(target);
                self.lint_expr_or_run(&value);
            }
            ArenaStmtKind::ProcDef(def) | ArenaStmtKind::CliMain(def) => {
                let function = self.arena.function_def(def);
                let entrypoint = matches!(stmt.kind, ArenaStmtKind::CliMain(_))
                    || function.test_declaration
                    || (!exported && self.scopes.len() == 1 && function.name == "main");
                if self.prefer_typed_callables && self.scopes.len() == 1 {
                    self.callable_parameters.define(
                        function.name,
                        def,
                        false,
                        !exported && !entrypoint,
                    );
                }
                if matches!(stmt.kind, ArenaStmtKind::ProcDef(_)) {
                    self.lint_propagating_function(def, false, |linter| {
                        linter.lint_proc_function(def, exported, entrypoint, stmt.span);
                    });
                } else {
                    self.lint_proc_function(def, exported, entrypoint, stmt.span);
                }
                // Test declarations and program entrypoints have no restricted
                // callers, so a clause there is a bound the author chose.
                if !entrypoint {
                    self.lint_effect_annotation(def, stmt.span);
                }
            }
            ArenaStmtKind::PureDef(def) => {
                if self.prefer_typed_callables && self.scopes.len() == 1 {
                    let name = self.arena.function_def(def).name;
                    self.callable_parameters.define(name, def, true, !exported);
                }
                self.lint_inferred_pure_return(def, exported);
                self.lint_propagating_function(def, true, |linter| linter.lint_function(def));
            }
            ArenaStmtKind::StreamDef(def) => {
                self.lint_proc_function(def, exported, false, stmt.span);
                self.lint_effect_annotation(def, stmt.span);
            }
            ArenaStmtKind::SignalHook(hook_id) => {
                let body = self.arena.signal_hook(hook_id).body;
                self.lint_block(body);
            }
            ArenaStmtKind::Return(value) => {
                if let Some(value) = value {
                    self.lint_return_value(&value);
                    self.lint_return_path_parse_roundtrip(&value);
                    self.lint_return_redundant_require(&value);
                    self.lint_expr_or_run(&value);
                }
            }
            ArenaStmtKind::Defer(value, _) => self.lint_expr_or_run(&value),
            ArenaStmtKind::YieldDelegate(value) | ArenaStmtKind::Exit(value) => {
                self.lint_expr(value)
            }
            ArenaStmtKind::Yield(value) => self.lint_expr_or_run(&value),
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                self.lint_list_pattern(branches, else_block, stmt.span);
                self.lint_if_as_guard(branches, else_block, stmt.span);
                self.lint_lexical_block(branches, else_block, stmt.span);
                for branch in self.arena.if_branches(branches).to_vec() {
                    self.lint_pattern_condition_block(branch.condition, branch.block);
                }
                if let Some(block) = else_block {
                    self.lint_block(block);
                }
            }
            ArenaStmtKind::While { condition, block } => {
                self.lint_pattern_condition_block(condition, block);
            }
            ArenaStmtKind::For {
                target,
                iter,
                block,
            } => {
                self.lint_scalar_split_iteration(iter);
                self.lint_byte_iteration(stmt_id, target, iter, block);
                self.lint_map_entry_iteration(stmt_id, target, iter, block);
                self.lint_yield_delegation(stmt.span, target, iter, block);
                self.lint_prefer_file_lines(iter);
                prefer_repeat::lint_counted_loop(self, stmt.span, target, iter, block);
                self.lint_expr(iter);
                self.push_scope();
                self.define_binding_target(target, stmt.span, true);
                self.lint_block_statements(block);
                self.pop_scope();
            }
            ArenaStmtKind::Break { value } => {
                if let Some(expr) = value {
                    self.lint_expr(expr);
                }
            }
            ArenaStmtKind::Continue => {}
            ArenaStmtKind::Loop { block } => self.lint_block(block),
            // The name a `tempdir` binds is in scope for its body only, and
            // the form itself uses it, so it is never an unused binding.
            ArenaStmtKind::Sugar { form: SugarForm::Tempdir, operands, .. } => {
                if let ArenaSugar::Tempdir { name, path, body } =
                    self.arena.sugar(SugarForm::Tempdir, operands)
                {
                    self.lint_expr(path);
                    self.push_scope();
                    self.define_binding_target(name, stmt.span, false);
                    self.lint_block(body);
                    self.pop_scope();
                }
            }
            // The name `atomically replace` binds is in scope for its body
            // only, and the form itself renames it, so it is never an unused
            // binding.
            ArenaStmtKind::Sugar { form: SugarForm::Atomically, operands, .. } => {
                if let ArenaSugar::Atomically { dest, name, body } =
                    self.arena.sugar(SugarForm::Atomically, operands)
                {
                    self.lint_expr(dest);
                    self.push_scope();
                    self.define_binding_target(name, stmt.span, false);
                    self.lint_block(body);
                    self.pop_scope();
                    prefer_atomically::lint_body_that_never_finishes(self, stmt.span, body);
                }
            }
            // Both names an indexed `for` binds are loop bindings, in scope
            // for its body only.
            ArenaStmtKind::Sugar { form: SugarForm::ForIndex, operands, .. } => {
                if let ArenaSugar::ForIndex { index, item, source, body } =
                    self.arena.sugar(SugarForm::ForIndex, operands)
                {
                    self.lint_expr(source);
                    self.push_scope();
                    self.define_binding_target(index, stmt.span, true);
                    self.define_binding_target(item, stmt.span, true);
                    self.lint_block_statements(body);
                    self.pop_scope();
                }
            }
            ArenaStmtKind::Sugar { form, operands, .. } => {
                let guarded = matches!(form, SugarForm::When | SugarForm::Unless);
                for operand in self.arena.sugar_operands(operands).to_vec() {
                    match operand {
                        ArenaSugarOperand::Expr(expr) => self.lint_expr(expr),
                        ArenaSugarOperand::Block(block) => self.lint_block(block),
                        ArenaSugarOperand::Stmt(stmt) => {
                            self.guarded_statement_depth += usize::from(guarded);
                            self.lint_stmt(stmt, false);
                            self.guarded_statement_depth -= usize::from(guarded);
                        }
                        _ => {}
                    }
                }
            }
            ArenaStmtKind::Guard {
                target,
                initializer,
                else_block,
                ..
            } => {
                self.lint_expr_or_run(&initializer);
                self.define_binding_target(target, stmt.span, true);
                self.lint_block(else_block);
            }
            ArenaStmtKind::Assert { condition, message } => {
                self.lint_expr(condition);
                if let Some(message) = message {
                    self.lint_expr(message);
                }
            }
            ArenaStmtKind::Match { value, arms } => {
                self.lint_adjacent_pattern_arms(
                    self.arena
                        .match_arms(arms)
                        .iter()
                        .map(|arm| {
                            (
                                arm.pattern,
                                arm.guard,
                                Ok(arm.block),
                                self.arena.span(arm.span),
                            )
                        })
                        .collect(),
                );
                let old_regex_context = self.regex_recovery_context;
                self.regex_recovery_context = true;
                self.lint_pattern_conditional_stmt(value, arms, stmt.span);
                self.lint_expr(value);
                for arm in self.arena.match_arms(arms).to_vec() {
                    self.push_scope();
                    self.lint_pattern(arm.pattern);
                    if let Some(guard) = arm.guard {
                        self.lint_expr(guard);
                    }
                    self.lint_block_statements(arm.block);
                    self.widen_guard_fix_to_arm_block(arm.block);
                    self.pop_scope();
                }
                self.regex_recovery_context = old_regex_context;
                let str_literal_arms = self
                    .arena
                    .match_arms(arms)
                    .iter()
                    .filter(|arm| {
                        matches!(
                            &self.arena.pattern(arm.pattern).kind,
                            ArenaPatternKind::Literal(expr) if matches!(self.arena.expr(*expr).kind, ArenaExprKind::Str(_))
                        )
                    })
                    .count();
                let has_catch_all = self.arena.match_arms(arms).iter().any(|arm| {
                    matches!(
                        self.arena.pattern(arm.pattern).kind,
                        ArenaPatternKind::Wildcard | ArenaPatternKind::Binding(_)
                    )
                });
                if str_literal_arms >= 3 && !has_catch_all {
                    self.warning(
                        stmt.span,
                        "3+ string-literal match arms — consider defining a tag union type",
                        DiagnosticCode::LintStringlyTypedMatch,
                        "tag unions are safer and exhaustiveness-checked",
                    );
                }
            }
            ArenaStmtKind::Command(command) => self.lint_command_stmt(command),
            ArenaStmtKind::TailBareIdent(name) => {
                self.mark_used(name.as_str().as_str());
                if self.prefer_typed_callables {
                    let hidden = self.local_hides(name);
                    self.callable_parameters.tail_value(name, hidden);
                }
            }
            ArenaStmtKind::Expr(expr) => {
                let span = self.arena.expr(expr).span;
                if self.statement_expression_spans.contains(&span) {
                    let inner = match self.arena.expr(expr).kind {
                        ArenaExprKind::Try(inner)
                        | ArenaExprKind::Unary {
                            op: UnaryOp::Not,
                            expr: inner,
                        } => inner,
                        _ => expr,
                    };
                    if matches!(self.arena.expr(inner).kind, ArenaExprKind::Call { .. }) {
                        self.statement_call_spans
                            .insert(self.arena.expr(inner).span, span);
                        if self.guarded_statement_depth == 0 {
                            self.whole_statement_call_spans
                                .insert(self.arena.expr(inner).span, span);
                        }
                    }
                }
                self.lint_core_assert(stmt.span, expr);
                self.lint_expr(expr);
            }
            ArenaStmtKind::With {
                bindings,
                body,
                else_block,
                ..
            } => {
                let old_regex_context = self.regex_recovery_context;
                self.regex_recovery_context = true;
                for binding in self.arena.with_bindings(bindings).to_vec() {
                    self.lint_expr_or_run(&ArenaExprOrRun::Expr(binding.initializer));
                }
                self.propagation_boundary_depth += 1;
                self.lint_block(body);
                self.lint_block(else_block);
                self.propagation_boundary_depth -= 1;
                self.regex_recovery_context = old_regex_context;
            }
        }
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

    fn lint_inferred_pure_return(&mut self, id: FunctionDefId, exported: bool) {
        let def = self.arena.function_def(id);
        if !self.prefer_inferred_pure_returns || exported || def.return_ty_defaulted {
            return;
        }
        let ty_span = self.arena.type_expr_span(def.return_ty);
        let mut types = vec![def.return_ty];
        while let Some(id) = types.pop() {
            let data = self.arena.type_expr_data[id.index()];
            match self.arena.type_expr_tags[id.index()] {
                ArenaTypeExprTag::Named => {
                    let name = Name::from_symbol(Symbol::from_raw(data.lhs));
                    if Type::builtin_from_name(&name.as_str()).is_none() {
                        return;
                    }
                }
                // A callable type's data words are not a child type id, and
                // its parts may name user types; it is never provably builtin.
                ArenaTypeExprTag::Qualified | ArenaTypeExprTag::Callable => return,
                ArenaTypeExprTag::Result => {
                    types.push(TypeExprId::from_index(data.lhs as usize));
                    if let Some(error) = TypeExprId::from_optional_raw(data.rhs) {
                        types.push(error);
                    }
                }
                _ => types.push(TypeExprId::from_index(data.lhs as usize)),
            }
        }
        let start = scan_before_arrow(self.source, ty_span.start());
        let deletion = Span::new(ty_span.source_id, start, ty_span.end());
        let Some(annotation) = self.source.get(start..ty_span.end()) else {
            return;
        };
        if annotation.contains('#') {
            return;
        }
        if self.return_removal_before.is_none() {
            self.return_removal_before = Some(checked_return_removal_facts(
                self.source,
                ty_span.source_id,
                None,
            ));
        }
        let Some(before) = self.return_removal_before.as_ref().unwrap().as_ref() else {
            return;
        };
        let mut rewritten = self.source.to_string();
        rewritten.replace_range(start..ty_span.end(), "");
        let after = checked_return_removal_facts(
            &rewritten,
            ty_span.source_id,
            Some((start, ty_span.end() - start)),
        );
        if Some(before) != after.as_ref() {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "private pure return type can be inferred exactly",
            )
            .with_code(DiagnosticCode::LintPreferInferredPureReturn)
            .with_label(Label::secondary(
                ty_span,
                "definition and caller types remain identical without this annotation",
            ))
            .with_fix_hint(FixHint::deletion(deletion, "infer the private pure return")),
        );
    }

    fn lint_default_parameter_annotation(
        &mut self,
        param: &xsh::frontend::syntax::arena::ArenaParam,
    ) {
        if param.ty_defaulted
            || param.default.is_none()
            || param.rest
            || self.annotation_refs_user_type(param.ty)
        {
            return;
        }
        let span = self.arena.type_expr_span(param.ty);
        let param_span = self.arena.span(param.span);
        let Some(prefix) = self.source.get(param_span.start()..span.start()) else {
            return;
        };
        let Some(colon) = prefix.rfind(':') else {
            return;
        };
        let start = param_span.start() + colon;
        let Some(annotation) = self.source.get(start..span.end()) else {
            return;
        };
        if annotation.contains('#') {
            return;
        }
        if self.return_removal_before.is_none() {
            self.return_removal_before = Some(checked_return_removal_facts(
                self.source,
                span.source_id,
                None,
            ));
        }
        let Some(before) = self.return_removal_before.as_ref().unwrap().as_ref() else {
            return;
        };
        let mut rewritten = self.source.to_string();
        rewritten.replace_range(start..span.end(), "");
        let after = checked_return_removal_facts(
            &rewritten,
            span.source_id,
            Some((start, span.end() - start)),
        );
        if Some(before) != after.as_ref() {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "default establishes exactly the declared parameter type",
            )
            .with_code(DiagnosticCode::LintDefaultParamType)
            .with_label(Label::secondary(
                span,
                "checked signatures, expression types, effects and conversions remain identical",
            ))
            .with_fix_hint(FixHint::deletion(
                Span::new(span.source_id, start, span.end()),
                "infer the parameter type from its default",
            )),
        );
    }

    fn checked_effect_fact(&self, body: Span) -> Option<&xsh::frontend::check::FunctionEffectFact> {
        let mut matches = self
            .checked_effects
            .iter()
            .filter_map(|(id, fact)| (id.body == body).then_some(fact));
        let fact = matches.next()?;
        matches.next().is_none().then_some(fact)
    }

    /// A private proc or stream whose clause equals the set it would infer
    /// gains nothing from the clause. Exports keep theirs because a clause there
    /// may be a deliberate API contract, and `main` and tests are entry points
    /// whose clause bounds the whole program or test. The checker's solver
    /// proves the inferred set is the declared one across module boundaries;
    /// when the file also checks on its own, every other checked fact must be
    /// unchanged by the deletion too.
    fn lint_inferred_proc_effects(
        &mut self,
        definition: FunctionDefId,
        exported: bool,
        entrypoint: bool,
        statement_span: Span,
    ) {
        if !self.prefer_inferred_private_effects || exported || entrypoint {
            return;
        }
        let def = self.arena.function_def(definition);
        if def.effects.is_none() || def.test_declaration {
            return;
        }
        let body = self.arena.span(self.arena.block(def.body).span);
        if !self
            .checked_effect_fact(body)
            .is_some_and(|fact| fact.redundant_clause)
        {
            return;
        }
        let Some(clause) = scan_effect_list_span(self.arena, def, statement_span, self.source)
        else {
            return;
        };
        // Delete the space before the clause too, leaving `) -> T` formatted.
        let start = self.source[..clause.start()].trim_end_matches(' ').len();
        let span = Span::new(clause.source_id, start, clause.end());
        if self.return_removal_before.is_none() {
            self.return_removal_before = Some(checked_return_removal_facts(
                self.source,
                span.source_id,
                None,
            ));
        }
        if let Some(before) = self.return_removal_before.as_ref().unwrap() {
            let mut rewritten = self.source.to_string();
            rewritten.replace_range(span.start()..span.end(), "");
            let after = checked_return_removal_facts(
                &rewritten,
                span.source_id,
                Some((span.start(), span.end() - span.start())),
            );
            if after.as_ref() != Some(before) {
                return;
            }
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "private effect clause names exactly the inferred effects",
            )
            .with_code(DiagnosticCode::LintPreferInferredPrivateEffects)
            .with_label(Label::secondary(
                clause,
                "checked body, caller contracts, and statement purposes remain equivalent",
            ))
            .with_fix_hint(FixHint::deletion(span, "infer the private effects")),
        );
    }

    fn lint_proc_function(
        &mut self,
        def_id: FunctionDefId,
        exported: bool,
        entrypoint: bool,
        statement_span: Span,
    ) {
        self.lint_inferred_proc_effects(def_id, exported, entrypoint, statement_span);
        self.note_proc_return_candidate(def_id, exported);
        let def = self.arena.function_def(def_id).clone();
        // Without the annotation, a complete `if`/`match` tail may infer a value.
        let branching_tail = self
            .arena
            .stmt_ids(self.arena.block(def.body).statements)
            .last()
            .is_some_and(|tail| {
                matches!(
                    self.arena.stmt(tail).kind,
                    ArenaStmtKind::Match { .. }
                        | ArenaStmtKind::If {
                            else_block: Some(_),
                            ..
                        }
                )
            });
        if !def.return_ty_defaulted
            && !exported
            && !branching_tail
            // An error family in the annotation is a contract the body may
            // rely on (`Err(.Variant(...))`); only the broad form is implied.
            && inferred_proc_return::broad_result_unit(self.arena, def.return_ty)
        {
            let ty_span = self.arena.type_expr_span(def.return_ty);
            let deletion_start = scan_before_arrow(self.source, ty_span.start());
            let deletion_span = Span::new(ty_span.source_id, deletion_start, ty_span.end());
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    "redundant `Result[Unit]` return annotation",
                )
                .with_code(DiagnosticCode::LintRedundantResultUnit)
                .with_label(Label::secondary(
                    ty_span,
                    "a proc without a value tail infers `Result[Unit]`",
                ))
                .with_fix_hint(FixHint::deletion(
                    deletion_span,
                    "remove return type annotation",
                )),
            );
        }
        self.lint_function(def_id);
    }

    fn lint_effect_annotation(&mut self, def_id: FunctionDefId, stmt_span: Span) {
        let def = self.arena.function_def(def_id).clone();
        let body_span = self.arena.span(self.arena.block(def.body).span);
        let Some(fact) = self.checked_effect_fact(body_span) else {
            return;
        };
        if fact.inferred {
            return;
        }
        let Some(required) = &fact.required else {
            return;
        };
        let mut effects: FxHashSet<_> = required.iter().cloned().collect();
        if self.assertion_effect_spans.iter().any(|span| {
            span.source_id == body_span.source_id
                && span.start() >= body_span.start()
                && span.end() <= body_span.end()
        }) {
            effects.insert(Effect::Error);
        }
        if effects.is_empty() {
            return;
        }
        // Every declaration without a clause infers its effects, so only an
        // incomplete clause needs a suggestion.
        let Some(declared_range) = def.effects else {
            return;
        };
        let declared: Vec<Effect> = self.arena.effects(declared_range).collect();
        let missing = effects
            .iter()
            .filter(|effect| !effects_covers_any(&declared, effect))
            .cloned()
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return;
        }
        let mut union = FxHashSet::default();
        for effect in &declared {
            union.insert(effect.clone());
        }
        for effect in missing {
            union.insert(effect);
        }
        let annotation = effects_annotation(&union);
        let Some(effect_span) = scan_effect_list_span(self.arena, &def, stmt_span, self.source)
        else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("proc `{}` is missing declared effects", def.name),
            )
            .with_code(DiagnosticCode::LintMissingEffects)
            .with_label(Label::secondary(
                stmt_span,
                format!("suggest [{annotation}]"),
            ))
            .with_fix_hint(FixHint::replacement(
                effect_span,
                format!("replace effect annotation with `[{annotation}]`"),
                format!("[{annotation}]"),
            )),
        );
    }

    fn lint_function(&mut self, def_id: FunctionDefId) {
        let def = self.arena.function_def(def_id).clone();
        self.push_scope();
        let result_unit = result_unit_type_expr(self.arena, def.return_ty);
        let result_path = result_path_type_expr(self.arena, def.return_ty);
        let result_ok = result_ok_type_expr(self.arena, def.return_ty);
        let body_span = self.arena.span(self.arena.block(def.body).span);
        let return_ty = self
            .checked_function_returns
            .get(&body_span)
            .cloned()
            .unwrap_or_else(|| Type::from_arena(self.arena, def.return_ty));

        self.result_unit_functions.push(result_unit);
        self.result_path_functions.push(result_path);
        self.result_return_ok_types.push(result_ok.clone());
        self.function_return_types.push(return_ty);
        if result_unit {
            self.lint_redundant_bare_return(def.body);
        }
        if result_path {
            self.lint_tail_path_parse_roundtrip(def.body);
        }
        if result_ok.is_some() {
            self.lint_tail_redundant_require(def.body);
            self.lint_tail_redundant_ok_return(def.body, result_ok.as_ref());
        }
        self.lint_redundant_tail_return_binding(def.body);
        self.lint_redundant_tail_return(
            def.body,
            self.function_return_types.last().cloned().as_ref().unwrap(),
        );
        if !def.return_ty_defaulted {
            self.collect_type_expr_refs(def.return_ty);
        }
        for param in self.arena.params(def.params).to_vec() {
            self.lint_default_parameter_annotation(&param);
            if !param.ty_defaulted {
                self.collect_type_expr_refs(param.ty);
            }
            if let Some(default) = param.default {
                self.lint_expr(default);
            }
            self.define(
                param.name.as_str().as_str(),
                self.arena.span(param.span),
                true,
            );
        }
        self.lint_block_statements(def.body);
        self.result_unit_functions.pop();
        self.result_path_functions.pop();
        self.result_return_ok_types.pop();
        self.function_return_types.pop();
        self.pop_scope();
    }

    fn lint_redundant_bare_return(&mut self, body: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        let Some(&last_id) = stmts.last() else {
            return;
        };
        let last_stmt = self.arena.stmt(last_id);
        if matches!(last_stmt.kind, ArenaStmtKind::Return(None)) {
            let deletion_span = scan_return_stmt_span(self.source, last_stmt.span);
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    "redundant `return` at end of `Result[Unit]` function",
                )
                .with_code(DiagnosticCode::LintRedundantBareReturn)
                .with_label(Label::secondary(
                    last_stmt.span,
                    "falling off the end also returns `Ok()`",
                ))
                .with_fix_hint(FixHint::deletion(deletion_span, "remove trailing `return`")),
            );
        }
    }

    fn lint_tail_path_parse_roundtrip(&mut self, body: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        let Some(&last_id) = stmts.last() else {
            return;
        };
        let ArenaStmtKind::Expr(expr) = self.arena.stmt(last_id).kind else {
            return;
        };
        self.lint_result_path_parse_roundtrip(expr);
    }

    fn lint_tail_redundant_require(&mut self, body: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        let Some(&last_id) = stmts.last() else {
            return;
        };
        let ArenaStmtKind::Expr(expr) = self.arena.stmt(last_id).kind else {
            return;
        };
        self.lint_result_redundant_require(expr);
    }

    fn lint_return_path_parse_roundtrip(&mut self, value: &ArenaExprOrRun) {
        if !self.result_path_functions.last().copied().unwrap_or(false) {
            return;
        }
        let ArenaExprOrRun::Expr(expr) = value else {
            return;
        };
        self.lint_result_path_parse_roundtrip(*expr);
    }

    fn lint_return_redundant_require(&mut self, value: &ArenaExprOrRun) {
        let ArenaExprOrRun::Expr(expr) = value else {
            return;
        };
        self.lint_result_redundant_require(*expr);
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
            ArenaExprKind::List(items) => self
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

    fn lint_needless_annotation(
        &mut self,
        target: BindingTargetId,
        mutable: bool,
        ty: TypeExprId,
        initializer: &ArenaExprOrRun,
        exported: bool,
        binding_span: Span,
    ) {
        if self.lint_solved_local_annotation(
            target,
            mutable,
            ty,
            initializer,
            exported,
            binding_span,
        ) {
            return;
        }
        if mutable
            && let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind
            && self.assigned_names.contains(&name)
        {
            return;
        }
        let annotation_ty = Type::from_arena(self.arena, ty);
        if matches!(
            annotation_ty,
            Type::Any | Type::Unknown | Type::Invalid | Type::Optional(_)
        ) {
            return;
        }
        let ArenaExprOrRun::Expr(init_expr_id) = initializer else {
            return;
        };
        // Empty Map factories acquire their key and value types from this
        // annotation. Removing it can also turn a later `{}` fix into a Record.
        if matches!(annotation_ty, Type::Map(_, _)) && is_map_empty_call(self.arena, *init_expr_id)
        {
            return;
        }
        if !self.annotation_is_needless(&annotation_ty, *init_expr_id) {
            return;
        }
        if self.annotation_refs_user_type(ty) {
            return;
        }
        let ty_span = self.arena.type_expr_span(ty);
        let deletion_start = scan_before_colon(self.source, ty_span.start());
        let deletion_end = scan_after_type(self.source, ty_span.end());
        if deletion_start >= deletion_end {
            return;
        }
        // Don't fix across comments
        if self.source[deletion_start..deletion_end].contains('#') {
            return;
        }
        let deletion_span = Span::new(ty_span.source_id, deletion_start, deletion_end);
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, "needless type annotation")
                .with_code(DiagnosticCode::LintNeedlessAnnotation)
                .with_label(Label::secondary(
                    ty_span,
                    "this type annotation is redundant with the initializer",
                ))
                .with_fix_hint(FixHint::deletion(
                    deletion_span,
                    "remove needless annotation",
                )),
        );
    }

    fn lint_solved_local_annotation(
        &mut self,
        target: BindingTargetId,
        mutable: bool,
        annotation: TypeExprId,
        initializer: &ArenaExprOrRun,
        exported: bool,
        binding_span: Span,
    ) -> bool {
        if exported
            || self.function_return_types.is_empty()
            || self.annotation_refs_user_type(annotation)
            || !matches!(self.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name != "_")
        {
            return false;
        }
        let ArenaExprOrRun::Expr(initializer) = *initializer else {
            return false;
        };
        let candidate = match self.arena.expr(initializer).kind {
            ArenaExprKind::List(items) => items.is_empty(),
            ArenaExprKind::Null => mutable,
            _ => false,
        };
        if !candidate {
            return false;
        }
        let annotation_span = self.arena.type_expr_span(annotation);
        let deletion = Span::new(
            annotation_span.source_id,
            scan_before_colon(self.source, annotation_span.start()),
            scan_after_type(self.source, annotation_span.end()),
        );
        if deletion.start() >= deletion.end() || self.source[deletion.range()].contains('#') {
            return true;
        }
        if !self.local_annotation_removal_preserves_contract(deletion, binding_span) {
            return true;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "local type is determined by consistent checked constraints",
            )
            .with_code(DiagnosticCode::LintNeedlessAnnotation)
            .with_label(Label::secondary(
                annotation_span,
                "the whole local binding retains this fixed type without the annotation",
            ))
            .with_fix_hint(FixHint::deletion(
                deletion,
                "remove the redundant local annotation",
            )),
        );
        true
    }

    fn local_annotation_removal_preserves_contract(
        &mut self,
        deletion: Span,
        binding: Span,
    ) -> bool {
        if self.local_annotation_before.is_none() {
            self.local_annotation_before = Some(checked_local_annotation_facts(
                self.source,
                deletion.source_id,
            ));
        }
        let Some(before) = self.local_annotation_before.as_ref().unwrap().as_ref() else {
            return false;
        };
        let Some(old_shape) = before.bindings.get(&binding.start()) else {
            return false;
        };
        let old_facts = before
            .expressions
            .iter()
            .map(|((start, end), shape)| {
                (
                    (
                        shift_after_deletion(*start, deletion),
                        shift_after_deletion(*end, deletion),
                    ),
                    shape.clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut rewritten = self.source.to_string();
        rewritten.replace_range(deletion.range(), "");
        let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
            deletion.source_id,
            &rewritten,
        );
        if !parsed.diagnostics.is_empty() {
            return false;
        }
        #[cfg(test)]
        local_annotation_probe_tests::record_candidate();
        let checked = xsh::frontend::check::Checker::check_arena(&parsed.arena, &rewritten);
        if checked
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
        {
            return false;
        }
        parsed.arena.symbol_owner().with_current(|| {
            let shape = checked
                .local_binding_types
                .iter()
                .find(|(span, _)| span.start() == binding.start())
                .map(|(_, ty)| checked_return_type_shape(ty));
            let facts = checked
                .expr_types
                .iter()
                .filter(|(span, _)| span.source_id == deletion.source_id)
                .map(|(span, ty)| ((span.start(), span.end()), checked_return_type_shape(ty)))
                .collect::<BTreeMap<_, _>>();
            shape.as_ref() == Some(old_shape) && facts == old_facts
        })
    }

    fn annotation_is_needless(&self, annotation: &Type, initializer: ExprId) -> bool {
        let init = self.arena.expr(initializer);
        // Branches and record fields can acquire their types from the binding.
        // Their checked type alone cannot prove that removing context is safe.
        if matches!(
            init.kind,
            ArenaExprKind::If { .. }
                | ArenaExprKind::Match { .. }
                | ArenaExprKind::PatternTest { .. }
                | ArenaExprKind::PatternCondition { .. }
                | ArenaExprKind::Record(_)
        ) {
            return false;
        }
        if matches!(annotation, Type::List(inner) if matches!(inner.as_ref(), Type::Record(_))) {
            return false;
        }
        if self.is_empty_collection(&init) {
            return false;
        }
        if self.expression_depends_on_expected_type(initializer, true) {
            return false;
        }
        let Some(actual) = self.expr_types.get(&init.span) else {
            return false;
        };
        if matches!(actual, Type::Any | Type::Unknown | Type::Invalid) {
            return false;
        }
        *actual == *annotation
    }

    // Checked expression types include conversions and inference supplied by
    // the enclosing contract. Moving an expression out of that context must
    // retain collection element domains and independent validation targets.
    fn expression_depends_on_expected_type(&self, expr: ExprId, removing_annotation: bool) -> bool {
        let expression = self.arena.expr(expr);
        match expression.kind {
            ArenaExprKind::Int(_) => self.expr_types.get(&expression.span) == Some(&Type::UInt),
            // A string literal is a Path only because a Path was expected.
            ArenaExprKind::Str(_) => self.expr_types.get(&expression.span) == Some(&Type::Path),
            ArenaExprKind::List(_)
            | ArenaExprKind::ListComp { .. }
            | ArenaExprKind::MapComp { .. } => {
                (removing_annotation && self.is_empty_collection(&expression))
                    || self
                        .expr_types
                        .get(&expression.span)
                        .is_some_and(type_has_contextual_collection_domain)
                    // A constant's elements carry no checked type of their
                    // own; in a collection of paths a string literal element
                    // is a Path only through the collection's declared type.
                    || (self
                        .expr_types
                        .get(&expression.span)
                        .is_some_and(type_mentions_path)
                        && expr_child_exprs(self.arena, expr).into_iter().any(|child| {
                            matches!(self.arena.expr(child).kind, ArenaExprKind::Str(_))
                        }))
                    || expr_child_exprs(self.arena, expr).into_iter().any(|child| {
                        self.expression_depends_on_expected_type(child, removing_annotation)
                    })
            }
            ArenaExprKind::Record(_)
            | ArenaExprKind::If { .. }
            | ArenaExprKind::Match { .. }
            | ArenaExprKind::PatternTest { .. }
            | ArenaExprKind::PatternCondition { .. }
            | ArenaExprKind::ValueBlock(_)
            | ArenaExprKind::Capture(_)
            | ArenaExprKind::Retry { .. }
            | ArenaExprKind::Loop { .. }
            | ArenaExprKind::ErrorContext { .. }
            | ArenaExprKind::ContextScope { .. }
            | ArenaExprKind::TempDirScope { .. } => true,
            ArenaExprKind::Require { schema, .. } => {
                schema.is_none()
                    || (removing_annotation
                        && self
                            .requirement_targets
                            .get(&expression.span)
                            .is_some_and(|target| {
                                self.requirement_expected_targets.get(&expression.span)
                                    == Some(target)
                            }))
                    || expr_child_exprs(self.arena, expr).into_iter().any(|child| {
                        self.expression_depends_on_expected_type(child, removing_annotation)
                    })
            }
            ArenaExprKind::Call { callee, .. } => {
                // Err has no success value from which to infer its domain; Ok
                // obtains a nondefault error domain only from its boundary.
                matches!(self.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Err"
                    || (name == "Ok" && matches!(self.expr_types.get(&expression.span), Some(Type::Result(_, error)) if error.as_ref() != &Type::Error)))
                    || expr_child_exprs(self.arena, expr).into_iter().any(|child| {
                        self.expression_depends_on_expected_type(child, removing_annotation)
                    })
            }
            _ => expr_child_exprs(self.arena, expr)
                .into_iter()
                .any(|child| self.expression_depends_on_expected_type(child, removing_annotation)),
        }
    }

    fn annotation_refs_user_type(&self, ty: TypeExprId) -> bool {
        self.type_expr_refs_user_type(ty)
    }

    fn type_expr_refs_user_type(&self, ty: TypeExprId) -> bool {
        match type_expr_kind(self.arena, ty) {
            ArenaTypeExprKind::Named(name) => self.user_type_names.contains(name.as_str().as_str()),
            ArenaTypeExprKind::List(inner)
            | ArenaTypeExprKind::Stream(inner)
            | ArenaTypeExprKind::Module(inner)
            | ArenaTypeExprKind::Optional(inner) => self.type_expr_refs_user_type(inner),
            ArenaTypeExprKind::Map(key, value) => {
                key.is_some_and(|key| self.type_expr_refs_user_type(key))
                    || self.type_expr_refs_user_type(value)
            }
            ArenaTypeExprKind::Result { ok, err } => {
                self.type_expr_refs_user_type(ok)
                    || err.is_some_and(|err| self.type_expr_refs_user_type(err))
            }
            // Qualified and applied schemas retain a named declaring domain
            // that cannot be established by comparing their structural shape.
            ArenaTypeExprKind::Qualified => true,
        }
    }

    fn is_empty_collection(&self, init: &ArenaExpr) -> bool {
        matches!(&init.kind, ArenaExprKind::List(items) if items.is_empty())
            || matches!(&init.kind, ArenaExprKind::Record(fields) if fields.is_empty())
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

    fn lint_redundant_tail_return(&mut self, body: BlockId, expected: &Type) {
        if expected == &Type::Unit || expected.is_result_unit() {
            return;
        }
        let Some(tail) = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .last()
        else {
            return;
        };
        let stmt = self.arena.stmt(tail);
        match stmt.kind {
            ArenaStmtKind::If {
                branches,
                else_block: Some(other),
            } => {
                for branch in self.arena.if_branches(branches).to_vec() {
                    self.lint_redundant_tail_return(branch.block, expected);
                }
                self.lint_redundant_tail_return(other, expected);
            }
            ArenaStmtKind::Match { arms, .. } => {
                for arm in self.arena.match_arms(arms).to_vec() {
                    self.lint_redundant_tail_return(arm.block, expected);
                }
            }
            ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(value))) => {
                if self.statement_positions.get(&stmt.span)
                    != Some(&xsh::frontend::check::StatementPosition::Value)
                {
                    return;
                }
                let value_span = self.arena.expr(value).span;
                if !self
                    .expr_types
                    .get(&value_span)
                    .is_some_and(|ty| tail_type_matches_lint_expected(expected, ty))
                {
                    return;
                }
                let start = stmt.span.start();
                let Some(text) = self.source.get(start..value_span.start()) else {
                    return;
                };
                let Some(after_return) = text.strip_prefix("return") else {
                    return;
                };
                let whitespace = after_return.len() - after_return.trim_start().len();
                let prefix = Span::new(
                    stmt.span.source_id,
                    start,
                    start + "return".len() + whitespace,
                );
                if self
                    .diagnostics
                    .iter()
                    .flat_map(|diagnostic| &diagnostic.fix_hints)
                    .filter_map(|fix| fix.span)
                    .any(|span| {
                        span.source_id == prefix.source_id
                            && span.start() < prefix.end()
                            && prefix.start() < span.end()
                    })
                {
                    return;
                }
                if self
                    .source
                    .get(prefix.range())
                    .is_none_or(|text| text.contains('#'))
                {
                    return;
                }
                let (replacement_span, replacement) =
                    if matches!(self.arena.expr(value).kind, ArenaExprKind::Record(_)) {
                        // A bare opening brace after a match arm introduces a body.
                        // Group literal records so removing return preserves an expression.
                        (
                            Span::new(stmt.span.source_id, start, value_span.end()),
                            format!("({})", &self.source[value_span.range()]),
                        )
                    } else {
                        (prefix, String::new())
                    };
                self.diagnostics.push(
                    Diagnostic::warning("tail return can supply its value implicitly")
                        .with_code(DiagnosticCode::LintRedundantTailReturn)
                        .with_label(Label::primary(prefix, "remove the tail return"))
                        .with_fix_hint(FixHint::replacement(
                            replacement_span,
                            "use the tail value",
                            replacement,
                        )),
                );
            }
            _ => {}
        }
    }

    fn lint_redundant_tail_return_binding(&mut self, body: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        if stmts.len() < 2 {
            return;
        }
        let len = stmts.len();
        let binding_stmt = self.arena.stmt(stmts[len - 2]);
        let return_stmt = self.arena.stmt(stmts[len - 1]);
        let (target, ty, initializer) = match binding_stmt.kind {
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(initializer),
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(initializer),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(initializer),
            } => (target, ty, initializer),
            _ => return,
        };
        let ArenaBindingTargetKind::Name(binding_name) = self.arena.binding_target(target).kind
        else {
            return;
        };
        let ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(returned))) = return_stmt.kind else {
            return;
        };
        if !matches!(self.arena.expr(returned).kind, ArenaExprKind::Ident(name) if name == binding_name)
        {
            return;
        }
        if self.annotation_is_record_type(ty) {
            return;
        }
        // A bare conditional at statement position is parsed as control flow,
        // so replacing an explicit return would lose the returned value.
        if matches!(
            self.arena.expr(initializer).kind,
            ArenaExprKind::If { .. }
                | ArenaExprKind::Match { .. }
                | ArenaExprKind::PatternTest { .. }
                | ArenaExprKind::PatternCondition { .. }
                | ArenaExprKind::Binary {
                    op: BinaryOp::ResultFallback,
                    ..
                }
        ) {
            return;
        }
        let initializer_span = self.arena.expr(initializer).span;
        let replacement = match self.source.get(initializer_span.range()) {
            Some(source) => format!("{source}\n"),
            None => return,
        };
        let replacement_span = Span::new(
            binding_stmt.span.source_id,
            binding_stmt.span.start(),
            span_end_after_following_newlines(self.source, return_stmt.span.end()),
        );
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            format!("tail binding `{binding_name}` can be returned implicitly"),
        )
        .with_code(DiagnosticCode::LintRedundantTailReturnBinding)
        .with_label(Label::secondary(
            return_stmt.span,
            "make the initializer the final expression",
        ));
        let between = &self.source[binding_stmt.span.end()..return_stmt.span.start()];
        if !between.contains('#') && self.tail_return_binding_autofix_safe(ty, initializer) {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                replacement_span,
                "replace binding and return with tail expression",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn annotation_is_record_type(&self, annotation: Option<TypeExprId>) -> bool {
        let Some(annotation) = annotation else {
            return false;
        };
        match type_expr_kind(self.arena, annotation) {
            ArenaTypeExprKind::Named(name) => {
                self.record_type_names.contains(name.as_str().as_str())
            }
            _ => false,
        }
    }

    fn tail_return_binding_autofix_safe(
        &self,
        annotation: Option<TypeExprId>,
        initializer: ExprId,
    ) -> bool {
        let Some(annotation) = annotation else {
            return true;
        };
        let expected = Type::from_arena(self.arena, annotation);
        if matches!(expected, Type::Any | Type::Unknown | Type::Invalid) {
            return false;
        }
        if self.expr_is_source_empty_list(initializer)
            && matches!(expected, Type::List(_))
            && self
                .function_return_types
                .last()
                .is_some_and(|return_ty| tail_type_matches_lint_expected(return_ty, &expected))
        {
            return true;
        }
        let initializer_span = self.arena.expr(initializer).span;
        let Some(actual) = self.expr_types.get(&initializer_span) else {
            return false;
        };
        actual.matches_expected(&expected) && expected.matches_expected(actual)
    }

    fn expr_is_source_empty_list(&self, expr: ExprId) -> bool {
        let arena_expr = self.arena.expr(expr);
        matches!(&arena_expr.kind, ArenaExprKind::List(items) if items.is_empty())
            || self
                .source
                .get(arena_expr.span.range())
                .is_some_and(|source| source.trim() == "[]")
    }

    fn lint_return_value(&mut self, value: &ArenaExprOrRun) {
        if !self.result_unit_functions.last().copied().unwrap_or(false)
            || !return_value_is_ok_unit(self.arena, value)
        {
            return;
        }
        let val_span = self.expr_or_run_span(value);
        // Include the whitespace before `Ok()` so deletion leaves a clean `return`.
        let deletion_start = scan_back_space(self.source, val_span.start());
        let deletion_span = Span::new(val_span.source_id, deletion_start, val_span.end());
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "redundant `return Ok()` in `Result[Unit]` function",
            )
            .with_code(DiagnosticCode::LintRedundantOkReturn)
            .with_label(Label::secondary(
                val_span,
                "use bare `return`, or omit the final return",
            ))
            .with_fix_hint(FixHint::deletion(deletion_span, "remove `Ok()`")),
        );
    }

    fn lint_tail_redundant_ok_return(&mut self, body: BlockId, expected_ok: Option<&Type>) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        let Some(&last_id) = stmts.last() else {
            return;
        };
        let last_stmt = self.arena.stmt(last_id);
        let ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr))) = last_stmt.kind else {
            return;
        };
        let expr_span = self.arena.expr(expr).span;
        let Some(ok_expr) = ok_call_arg(self.arena, expr) else {
            return;
        };
        if matches!(
            self.arena.expr(ok_expr).kind,
            ArenaExprKind::If { .. }
                | ArenaExprKind::Match { .. }
                | ArenaExprKind::PatternTest { .. }
                | ArenaExprKind::PatternCondition { .. }
                | ArenaExprKind::Binary {
                    op: BinaryOp::ResultFallback,
                    ..
                }
        ) {
            return;
        }
        // Get the full source text of the return value by slicing from
        // `return Ok(` to the matching `)` at the end, then stripping the
        // `return ` prefix for the tail expression. Using the enclosing
        // statement span keeps grouping parens that the expression AST
        // drops (e.g. `Ok((n * 3) + 1)` → `(n * 3) + 1`).
        let replacement = match self.source.get(last_stmt.span.range()) {
            Some(stmt_source) => {
                let inner = stmt_source
                    .strip_prefix("return Ok(")
                    .and_then(|rest| rest.strip_suffix(")\n").or_else(|| rest.strip_suffix(")")));
                match inner {
                    Some(inner) => format!("{inner}\n"),
                    None => return,
                }
            }
            None => return,
        };
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "redundant `return Ok(...)` at function tail",
        )
        .with_code(DiagnosticCode::LintRedundantOkTail)
        .with_label(Label::secondary(
            expr_span,
            "plain tail values are wrapped in `Ok(...)` automatically",
        ));
        if self.tail_ok_return_autofix_safe(expected_ok, ok_expr) {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                Span::new(
                    last_stmt.span.source_id,
                    last_stmt.span.start(),
                    span_end_after_following_newlines(self.source, last_stmt.span.end()),
                ),
                "use the plain tail value",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn tail_ok_return_autofix_safe(&self, expected_ok: Option<&Type>, ok_expr: ExprId) -> bool {
        let Some(expected_ok) = expected_ok else {
            return false;
        };
        let ok_span = self.arena.expr(ok_expr).span;
        let Some(actual) = self.expr_types.get(&ok_span) else {
            return false;
        };
        actual.matches_expected(expected_ok) && expected_ok.matches_expected(actual)
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
            ArenaPatternKind::Alternation(items) => self
                .arena
                .pattern_ids(items)
                .all(|child| self.pattern_test_fix_is_nonbinding(child)),
        }
    }

    fn lint_pattern(&mut self, pattern: PatternId) {
        let arena_pattern = self.arena.pattern(pattern).clone();
        let span = self.arena.span(arena_pattern.span);
        match arena_pattern.kind {
            ArenaPatternKind::Group(child) => self.lint_pattern(child),
            ArenaPatternKind::Alias {
                pattern,
                name,
                name_span,
            } => {
                self.lint_pattern(pattern);
                self.define(&name.as_str(), self.arena.span(name_span), true);
            }
            ArenaPatternKind::Binding(name) => {
                if !self.tag_variants.contains(name.as_str().as_str()) {
                    self.define(name.as_str().as_str(), span, true);
                }
            }
            ArenaPatternKind::Type { binding, ty } => {
                self.collect_type_expr_refs(ty);
                if let Some(name) = binding {
                    self.define(name.as_str().as_str(), span, true);
                }
            }
            ArenaPatternKind::Constructor { arg, .. } => {
                if let Some(arg) = arg {
                    self.lint_pattern(arg);
                }
            }
            ArenaPatternKind::Record { fields, .. }
            | ArenaPatternKind::ErrorVariant { fields, .. } => {
                for field in self.arena.pattern_fields(fields).to_vec() {
                    self.lint_pattern(field.pattern);
                }
            }
            ArenaPatternKind::List { elements, rest } => {
                for child in self
                    .arena
                    .pattern_ids(elements)
                    .chain(rest)
                    .collect::<Vec<_>>()
                {
                    self.lint_pattern(child);
                }
            }
            ArenaPatternKind::Alternation(patterns) => {
                let children: Vec<_> = self.arena.pattern_ids(patterns).collect();
                let saved = self.scopes.last().cloned().unwrap_or_default();
                for &child in children.iter().skip(1) {
                    if let Some(scope) = self.scopes.last_mut() {
                        *scope = saved.clone();
                    }
                    self.lint_pattern(child);
                }
                if let Some(scope) = self.scopes.last_mut() {
                    *scope = saved;
                }
                if let Some(&child) = children.first() {
                    self.lint_pattern(child);
                }
            }
            ArenaPatternKind::Tuple(patterns) => {
                for pat in self.arena.pattern_ids(patterns).collect::<Vec<_>>() {
                    self.lint_pattern(pat);
                }
            }
            ArenaPatternKind::TestName { ty, .. } => self.collect_type_expr_refs(ty),
            ArenaPatternKind::Wildcard
            | ArenaPatternKind::Literal(_)
            | ArenaPatternKind::Facet(_) => {}
        }
    }

    /// The span the innermost scope records for the binding `name` names.
    fn binding_definition(&self, name: Name) -> Option<Span> {
        let name = name.as_str();
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name.as_str()))
            .map(|binding| binding.span)
    }

    /// Whether a binding inside a function or block hides the top-level
    /// meaning of `name` where the traversal is.
    fn local_hides(&self, name: Name) -> bool {
        self.scopes
            .iter()
            .skip(1)
            .any(|scope| scope.contains_key(name.as_str().as_str()))
    }

    /// Tells `lint.list-any-union` about a name the traversal just defined.
    fn declare_list_any_binding(
        &mut self,
        target: BindingTargetId,
        ty: Option<TypeExprId>,
        initializer: &ArenaExprOrRun,
    ) {
        let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind else {
            return;
        };
        if let Some(definition) = self.binding_definition(name) {
            self.list_any_bindings.declare(
                self.arena,
                definition,
                ty,
                initializer,
                &self.expr_types,
            );
        }
    }

    fn define_binding_target(&mut self, target: BindingTargetId, span: Span, report_unused: bool) {
        match self.arena.binding_target(target).kind.clone() {
            ArenaBindingTargetKind::Name(name) => {
                self.define(name.as_str().as_str(), span, report_unused)
            }
            ArenaBindingTargetKind::Record { fields, .. } => {
                for field in self.arena.destructure_fields(fields).to_vec() {
                    self.define_binding_target(
                        field.target,
                        self.arena.span(field.span),
                        report_unused,
                    );
                }
            }
        }
    }

    fn lint_pattern_condition_block(&mut self, condition: ExprId, block: BlockId) {
        if let ArenaExprKind::PatternCondition { value, arms } = self.arena.expr(condition).kind {
            self.lint_expr(value);
            self.push_scope();
            self.lint_pattern(self.arena.match_expr_arms(arms)[0].pattern);
            self.lint_block_statements(block);
            self.pop_scope();
        } else {
            self.lint_expr(condition);
            self.lint_block(block);
        }
    }

    fn lint_block(&mut self, block: BlockId) {
        self.push_scope();
        self.lint_block_statements(block);
        self.pop_scope();
    }

    fn lint_block_statements(&mut self, block: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(block).statements)
            .collect();
        self.lint_list_comp_suggestions(&stmts);
        prefer_tempdir::lint_scratch_directories(self, &stmts, Some(block));
        prefer_atomically::lint_published_files(self, &stmts, Some(block));
        lint_prefer_for_index::lint_counter_loops(self, &stmts, Some(block));
        prefer_within::lint_repeated_timeouts(self, &stmts);
        self.lint_statement_sequence(&stmts);
    }

    fn lint_record_destructuring(&mut self, stmts: &[StmtId]) {
        fn extraction(linter: &Linter<'_>, stmt: StmtId) -> Option<(Name, Vec<Name>, Name)> {
            let ArenaStmtKind::Let {
                target,
                ty: None,
                initializer: ArenaExprOrRun::Expr(mut value),
            } = linter.arena.stmt(stmt).kind
            else {
                return None;
            };
            let ArenaBindingTargetKind::Name(binding) = linter.arena.binding_target(target).kind
            else {
                return None;
            };
            if binding.as_str() == "_" {
                return None;
            }
            let mut path = Vec::new();
            while let ArenaExprKind::Field { base, name } = linter.arena.expr(value).kind {
                let Some(Type::Record(schema)) =
                    linter.expr_types.get(&linter.arena.expr(base).span)
                else {
                    return None;
                };
                if !schema.contains_key(&name) {
                    return None;
                }
                path.push(name);
                value = base;
            }
            let ArenaExprKind::Ident(root) = linter.arena.expr(value).kind else {
                return None;
            };
            if path.is_empty() || root == binding {
                return None;
            }
            path.reverse();
            Some((root, path, binding))
        }
        fn target(entries: &[(Vec<Name>, Name)], depth: usize) -> Option<String> {
            type Entries = Vec<(Vec<Name>, Name)>;
            let mut fields: Vec<(Name, Entries)> = Vec::new();
            for (path, binding) in entries {
                let field = *path.get(depth)?;
                if let Some((_, children)) = fields.iter_mut().find(|(name, _)| *name == field) {
                    if path.len() == depth + 1
                        || children.iter().any(|(path, _)| path.len() == depth + 1)
                    {
                        return None;
                    }
                    children.push((path.clone(), *binding));
                } else {
                    fields.push((field, vec![(path.clone(), *binding)]));
                }
            }
            let mut output = Vec::new();
            for (field, children) in fields {
                if children[0].0.len() == depth + 1 {
                    let binding = children[0].1;
                    output.push(if field == binding {
                        field.to_string()
                    } else {
                        format!("{field}: {binding}")
                    });
                } else {
                    output.push(format!("{field}: {}", target(&children, depth + 1)?));
                }
            }
            output.push("..".to_string());
            Some(format!("{{{}}}", output.join(", ")))
        }
        let mut index = 0;
        while index < stmts.len() {
            let Some((root, path, binding)) = extraction(self, stmts[index]) else {
                index += 1;
                continue;
            };
            let start = index;
            let mut entries = vec![(path, binding)];
            index += 1;
            while index < stmts.len() {
                let Some((next_root, path, binding)) = extraction(self, stmts[index]) else {
                    break;
                };
                if next_root != root
                    || entries
                        .iter()
                        .any(|(_, name)| *name == next_root || *name == binding)
                {
                    break;
                }
                entries.push((path, binding));
                index += 1;
            }
            if entries.len() < 2 {
                continue;
            }
            let first = self.arena.stmt(stmts[start]).span;
            let last = self.arena.stmt(stmts[index - 1]).span;
            let span = Span::new(first.source_id, first.start(), last.end());
            if self
                .source
                .get(span.range())
                .is_none_or(|source| source.contains('#'))
            {
                continue;
            }
            let Some(pattern) = target(&entries, 0) else {
                continue;
            };
            let suffix = if self
                .source
                .get(span.range())
                .is_some_and(|source| source.ends_with('\n'))
            {
                "\n"
            } else {
                ""
            };
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    "adjacent record field bindings can destructure their source",
                )
                .with_code(DiagnosticCode::LintPreferRecordDestructuring)
                .with_label(Label::secondary(
                    span,
                    "these fields come from the same checked record binding",
                ))
                .with_fix_hint(FixHint::replacement(
                    span,
                    "bind the selected record fields together",
                    format!("let {pattern} = {root}{suffix}"),
                )),
            );
        }
    }

    /// A fresh placeholder can disappear only when no cleanup or body work
    /// observes its earlier initialization. Keep mutation semantics for later use.
    fn lint_context_scope_scaffolds(&mut self, stmts: &[StmtId]) {
        context_scope::lint_command_scope_scaffolds(self, stmts);
        for pair in stmts.windows(2) {
            let declaration = self.arena.stmt(pair[0]);
            let ArenaStmtKind::Var {
                target,
                initializer: ArenaExprOrRun::Expr(initial),
                ..
            } = declaration.kind
            else {
                continue;
            };
            let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind else {
                continue;
            };
            if !matches!(
                self.arena.expr(initial).kind,
                ArenaExprKind::Int(_)
                    | ArenaExprKind::Str(_)
                    | ArenaExprKind::Bool(_)
                    | ArenaExprKind::PathStr(_)
            ) {
                continue;
            }
            let statement = self.arena.stmt(pair[1]);
            let (input, block, cwd) = match statement.kind {
                ArenaStmtKind::Expr(expr) => {
                    let ArenaExprKind::Try(scope) = self.arena.expr(expr).kind else {
                        continue;
                    };
                    let ArenaExprKind::ContextScope {
                        input,
                        block,
                        kind,
                        value_body: false,
                    } = self.arena.expr(scope).kind
                    else {
                        continue;
                    };
                    (
                        input,
                        block,
                        kind == xsh::frontend::syntax::arena::ContextScopeKind::Cwd,
                    )
                }
                ArenaStmtKind::Command(command) => {
                    let command = self.arena.command_stmt(command);
                    let ArenaCommand::Core {
                        name: CoreCommand::Cd,
                        args,
                        block: Some(block),
                        ..
                    } = command.command
                    else {
                        continue;
                    };
                    let [argument] = self.arena.command_args(args) else {
                        continue;
                    };
                    let ArenaCommandArgKind::Typed(input) = argument.kind else {
                        continue;
                    };
                    if !command.propagate {
                        continue;
                    }
                    (input, block, true)
                }
                _ => continue,
            };
            let body: Vec<_> = self
                .arena
                .stmt_ids(self.arena.block(block).statements)
                .collect();
            let [assignment] = body.as_slice() else {
                continue;
            };
            let assignment = self.arena.stmt(*assignment);
            let ArenaStmtKind::Assign {
                target,
                op: AssignOp::Set,
                value: ArenaExprOrRun::Expr(value),
            } = assignment.kind
            else {
                continue;
            };
            if !matches!(self.arena.assign_target(target).kind, ArenaAssignTargetKind::Name(found) if found == name)
                || expr_references_name(self.arena, input, name)
                || expr_references_name(self.arena, value, name)
            {
                continue;
            }
            if !self.expr_types.contains_key(&self.arena.expr(initial).span)
                || self.expr_types.get(&self.arena.expr(initial).span)
                    != self.expr_types.get(&self.arena.expr(value).span)
            {
                continue;
            }
            let edit = Span::new(
                declaration.span.source_id,
                declaration.span.start(),
                statement.span.end(),
            );
            let mut diagnostic =
                Diagnostic::warning("a fresh placeholder can consume the scope value")
                    .with_code(DiagnosticCode::LintPreferContextScopeValue)
                    .with_label(Label::secondary(
                        edit,
                        "initialize directly from the restored context",
                    ));
            // Cleanup captures, signal handlers, and comments can observe or explain the original
            // assignment timing; leave those scaffolds for an explicit edit.
            fn scalar_atom(arena: &AstArena, expr: ExprId) -> bool {
                matches!(
                    arena.expr(expr).kind,
                    ArenaExprKind::Ident(_)
                        | ArenaExprKind::Int(_)
                        | ArenaExprKind::Str(_)
                        | ArenaExprKind::Bool(_)
                        | ArenaExprKind::PathStr(_)
                )
            }
            let stable_input = scalar_atom(self.arena, input)
                || match self.arena.expr(input).kind {
                    ArenaExprKind::Record(fields) => {
                        self.arena
                            .record_fields(fields)
                            .iter()
                            .all(|field| match field.kind {
                                ArenaRecordFieldKind::Named { value, .. } => {
                                    scalar_atom(self.arena, value)
                                }
                                ArenaRecordFieldKind::Shorthand { .. } => true,
                                _ => false,
                            })
                    }
                    _ => false,
                };
            let mut selected_value = value;
            if let ArenaExprKind::Try(inner) = self.arena.expr(selected_value).kind {
                selected_value = inner;
            }
            let stable_value = scalar_atom(self.arena, selected_value)
                || match self.arena.expr(selected_value).kind {
                    ArenaExprKind::Call { callee, args } => match self.arena.expr(callee).kind {
                        ArenaExprKind::Field { base, name: method } => {
                            match self.arena.expr(base).kind {
                                ArenaExprKind::Ident(module) => {
                                    ["env", "fs"].contains(&module.as_str().as_str())
                                        && !self.scopes.iter().any(|scope| {
                                            scope.contains_key(module.as_str().as_str())
                                        })
                                        && xsh::api::api_spec()
                                            .module_overloads(&module.as_str(), &method.as_str())
                                            .is_some()
                                        && self.arena.call_args(args).iter().all(|arg| {
                                            match arg.kind {
                                                ArenaCallArgKind::Positional(value)
                                                | ArenaCallArgKind::Named { value, .. } => {
                                                    scalar_atom(self.arena, value)
                                                }
                                                _ => false,
                                            }
                                        })
                                }
                                _ => false,
                            }
                        }
                        _ => false,
                    },
                    _ => false,
                };
            if stable_input
                && stable_value
                && self.arena.signal_hooks.is_empty()
                && !self.source.contains("defer")
                && !self.source[edit.range()].contains('#')
            {
                let prefix =
                    &self.source[declaration.span.start()..self.arena.expr(initial).span.start()];
                let input = &self.source[self.arena.expr(input).span.range()];
                let value = &self.source[self.arena.expr(value).span.range()];
                let replacement = format!(
                    "{prefix}{} ({input}) {{ {value} }}?\n",
                    if cwd { "cd" } else { "env" }
                );
                let mut candidate = self.source.to_string();
                candidate.replace_range(edit.range(), &replacement);
                let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
                    edit.source_id,
                    &candidate,
                );
                if parsed.diagnostics.is_empty()
                    && xsh::frontend::check::Checker::check_arena(&parsed.arena, &candidate)
                        .diagnostics
                        .is_empty()
                {
                    diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                        edit,
                        "consume the scope tail directly",
                        replacement,
                    ));
                }
            }
            self.diagnostics.push(diagnostic);
        }
    }

    fn lint_statement_sequence(&mut self, stmts: &[StmtId]) {
        self.lint_context_scope_scaffolds(stmts);
        self.lint_record_destructuring(stmts);
        self.lint_fresh_map_initializations(stmts);
        self.lint_list_element_reconstruction(stmts);
        if let Some(&first) = stmts.first() {
            self.lint_negative_if_as_boolean_guard(first);
        }
        lint_optional_binding::lint_null_test_then_binding(self, stmts);
        lint_write_mode::lint_write_then_chmod(self, stmts);
        lint_prefer_test_expect::lint_script_runs(self, stmts);
        let mut flow = FlowSummary::fallthrough();
        let mut reported_dead_region = false;
        for (index, &stmt) in stmts.iter().enumerate() {
            if index > 0 {
                self.lint_linear_value_pipeline(stmts[index - 1], stmt, stmts);
            }
            if self.dead_code && !flow.fallthrough && !reported_dead_region {
                self.warning(
                    self.arena.stmt(stmt).span,
                    "unreachable code",
                    DiagnosticCode::LintDeadCode,
                    "this statement can never execute",
                );
                reported_dead_region = true;
            }
            self.lint_stmt(stmt, false);
            if flow.fallthrough {
                flow = flow.then(stmt_flow(self.arena, stmt, &self.terminating_call_spans));
            }
        }
    }

    fn lint_list_element_reconstruction(&mut self, stmts: &[StmtId]) {
        fn integer(arena: &AstArena, expr: ExprId) -> Option<usize> {
            let ArenaExprKind::Int(value) = arena.expr(expr).kind else {
                return None;
            };
            usize::try_from(arena.int_literal(value).value()?).ok()
        }
        for (position, &stmt) in stmts.iter().enumerate() {
            let node = self.arena.stmt(stmt);
            let ArenaStmtKind::Assign {
                target,
                op: AssignOp::Set,
                value: ArenaExprOrRun::Expr(value),
            } = node.kind
            else {
                continue;
            };
            let ArenaAssignTargetKind::Name(name) = self.arena.assign_target(target).kind else {
                continue;
            };
            let ArenaExprKind::List(items) = self.arena.expr(value).kind else {
                continue;
            };
            let elements: Vec<_> = self.arena.list_elements(items).collect();
            let [prefix, replacement, suffix] = elements.as_slice() else {
                continue;
            };
            if prefix.splice_span.is_none()
                || replacement.splice_span.is_some()
                || suffix.splice_span.is_none()
            {
                continue;
            }
            let ArenaExprKind::Slice {
                base: first,
                start: None,
                end: Some(end),
                guarded: false,
            } = self.arena.expr(prefix.value).kind
            else {
                continue;
            };
            let ArenaExprKind::Slice {
                base: last,
                start: Some(start),
                end: None,
                guarded: false,
            } = self.arena.expr(suffix.value).kind
            else {
                continue;
            };
            if ![first, last].into_iter().all(|expr| matches!(self.arena.expr(expr).kind, ArenaExprKind::Ident(found) if found == name)) { continue; }
            let (Some(index), Some(after)) = (integer(self.arena, end), integer(self.arena, start))
            else {
                continue;
            };
            if index.checked_add(1) != Some(after) {
                continue;
            }
            let Some(list_ty @ Type::List(element)) =
                self.expr_types.get(&self.arena.expr(first).span)
            else {
                continue;
            };
            if !list_splice_element_type_is_precise(element)
                || self.expr_types.get(&self.arena.expr(value).span) != Some(list_ty)
                || self
                    .expr_types
                    .get(&self.arena.expr(replacement.value).span)
                    != Some(element.as_ref())
            {
                continue;
            }
            let mut diagnostic = Diagnostic::warning(
                "prefer an element assignment when the list index is known valid",
            )
            .with_code(DiagnosticCode::LintPreferListElementAssignment)
            .with_label(Label::secondary(node.span, "replace one existing element"));
            // Only an immediately preceding literal declaration proves a current
            // length without removing a read across intervening effects.
            let length = position.checked_sub(1).and_then(|previous| {
                let ArenaStmtKind::Var { target, initializer: ArenaExprOrRun::Expr(initializer), .. } = self.arena.stmt(stmts[previous]).kind else { return None; };
                if !matches!(self.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(found) if found == name) { return None; }
                let ArenaExprKind::List(elements) = self.arena.expr(initializer).kind else { return None; };
                let elements: Vec<_> = self.arena.list_elements(elements).collect();
                elements.iter().all(|element| element.splice_span.is_none()).then_some(elements.len())
            });
            let edit_span = Span::new(
                node.span.source_id,
                node.span.start(),
                self.arena.expr(value).span.end(),
            );
            if length.is_some_and(|length| index < length)
                && list_update_argument_stable(self.arena, replacement.value)
                && self
                    .source
                    .get(edit_span.range())
                    .is_some_and(|text| !text.contains('#'))
            {
                let rhs = &self.source[self.arena.expr(replacement.value).span.range()];
                diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                    edit_span,
                    "update the existing list element",
                    format!("{name}[{index}] = {rhs}"),
                ));
            } else {
                diagnostic = diagnostic.with_note("slice bounds clip but element indices must exist; unproved lengths, effects, and comments require manual review");
            }
            self.diagnostics.push(diagnostic);
        }
    }

    fn lint_scalar_split_iteration(&mut self, iter: ExprId) {
        let ArenaExprKind::Call { callee, args } = self.arena.expr(iter).kind else {
            return;
        };
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        let checked_str = self.expr_types.get(&self.arena.expr(base).span) == Some(&Type::Str)
            || (matches!(self.arena.expr(base).kind, ArenaExprKind::Str(_))
                && matches!(self.expr_types.get(&self.arena.expr(iter).span), Some(Type::List(item)) if **item == Type::Str));
        if name != "split" || !checked_str {
            return;
        }
        let arguments = self.arena.call_args(args);
        if arguments.len() != 1 {
            return;
        }
        let separator = match arguments[0].kind {
            ArenaCallArgKind::Positional(value) => value,
            ArenaCallArgKind::Named { name, value, .. } if name == "separator" => value,
            _ => return,
        };
        let ArenaExprKind::Str(text) = self.arena.expr(separator).kind else {
            return;
        };
        if !self.arena.string_literal(text).is_empty() {
            return;
        }
        let span = self.arena.expr(iter).span;
        if self
            .source
            .get(span.range())
            .is_none_or(|text| text.contains('#'))
        {
            return;
        }
        let callee_span = self.arena.expr(callee).span;
        let Some(dot) = self
            .source
            .get(callee_span.range())
            .and_then(|source| source.rfind('.'))
        else {
            return;
        };
        let suffix = Span::new(span.source_id, callee_span.start() + dot, span.end());
        self.diagnostics.push(
            Diagnostic::warning("iterate over Unicode scalars without a split List")
                .with_code(DiagnosticCode::LintPreferScalarIteration)
                .with_label(Label::secondary(
                    span,
                    "the checked Str source is retained once",
                ))
                .with_fix_hint(FixHint::deletion(suffix, "iterate over the Str directly")),
        );
    }

    fn lint_byte_iteration(
        &mut self,
        stmt_id: StmtId,
        target: BindingTargetId,
        iter: ExprId,
        block: BlockId,
    ) {
        let ArenaBindingTargetKind::Name(index) = self.arena.binding_target(target).kind else {
            return;
        };
        let ArenaExprKind::Call { callee, args } = self.arena.expr(iter).kind else {
            return;
        };
        if !matches!(self.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "range")
            || self.is_binding_in_scope_or_assigned("range")
        {
            return;
        }
        let arguments = self.arena.call_args(args);
        let [argument] = arguments else {
            return;
        };
        let ArenaCallArgKind::Positional(length) = argument.kind else {
            return;
        };
        let ArenaExprKind::Call { callee, args } = self.arena.expr(length).kind else {
            return;
        };
        if !self.arena.call_args(args).is_empty() {
            return;
        }
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if name != "len" {
            return;
        }
        let ArenaExprKind::Ident(source) = self.arena.expr(base).kind else {
            return;
        };
        let Some(binding) = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(source.as_str().as_str()))
        else {
            return;
        };
        if self.assigned_names.contains(&source)
            || source == index
            || binding.mutable
            || !binding.comparison_stable
        {
            return;
        }
        // Constant preparation may replace the length expression before recording its receiver.
        // The resolved immutable initializer supplies the same checked Bytes proof in that case.
        let source_type = self
            .expr_types
            .get(&self.arena.expr(base).span)
            .or_else(|| {
                (0..self.arena.stmt_tags.len()).find_map(|index| {
                    let statement = self.arena.stmt(StmtId::from_index(index));
                    if statement.span != binding.span {
                        return None;
                    }
                    match statement.kind {
                        ArenaStmtKind::Let {
                            initializer: ArenaExprOrRun::Expr(value),
                            ..
                        }
                        | ArenaStmtKind::Const {
                            initializer: ArenaExprOrRun::Expr(value),
                            ..
                        } => self.expr_types.get(&self.arena.expr(value).span),
                        _ => None,
                    }
                })
            });
        if source_type != Some(&Type::Bytes) {
            return;
        }
        let statements: Vec<_> = self
            .arena
            .stmt_ids(self.arena.block(block).statements)
            .collect();
        let Some((&first, remaining)) = statements.split_first() else {
            return;
        };
        let first = self.arena.stmt(first);
        let ArenaStmtKind::Let {
            target,
            ty: None,
            initializer: ArenaExprOrRun::Expr(mut access),
        } = first.kind
        else {
            return;
        };
        let ArenaBindingTargetKind::Name(octet) = self.arena.binding_target(target).kind else {
            return;
        };
        if octet == source || octet == index || octet == "_" {
            return;
        }
        if let ArenaExprKind::Binary {
            op: BinaryOp::ResultFallback,
            left,
            right,
        } = self.arena.expr(access).kind
        {
            if !matches!(
                self.arena.expr(right).kind,
                ArenaExprKind::Int(_)
                    | ArenaExprKind::Unary {
                        op: UnaryOp::Neg,
                        ..
                    }
            ) {
                return;
            }
            if let ArenaExprKind::Unary { expr: value, .. } = self.arena.expr(right).kind
                && !matches!(self.arena.expr(value).kind, ArenaExprKind::Int(_))
            {
                return;
            }
            access = left;
        }
        let ArenaExprKind::Call { callee, args } = self.arena.expr(access).kind else {
            return;
        };
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if name != "byte_at"
            || !matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(name) if name == source)
        {
            return;
        }
        let [argument] = self.arena.call_args(args) else {
            return;
        };
        if !matches!(argument.kind, ArenaCallArgKind::Positional(value) if matches!(self.arena.expr(value).kind, ArenaExprKind::Ident(name) if name == index))
        {
            return;
        }
        // Textual occurrence checks also cover command interpolation and shadowed names.
        // Rejecting harmless strings or comments is preferable to losing a byte offset.
        let mentions_index = |text: &str| {
            text.match_indices(index.as_str().as_str())
                .any(|(offset, matched)| {
                    let identifier_char = |ch: char| ch.is_alphanumeric() || ch == '_';
                    !text[..offset]
                        .chars()
                        .next_back()
                        .is_some_and(identifier_char)
                        && !text[offset + matched.len()..]
                            .chars()
                            .next()
                            .is_some_and(identifier_char)
                })
        };
        if remaining.iter().any(|id| {
            self.source
                .get(self.arena.stmt(*id).span.range())
                .is_none_or(mentions_index)
        }) {
            return;
        }
        let header = Span::new(
            first.span.source_id,
            self.arena.stmt(stmt_id).span.start(),
            self.arena.expr(iter).span.end(),
        );
        let line_start = self.source[..first.span.start()]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        if !self.source[line_start..first.span.start()]
            .trim()
            .is_empty()
        {
            return;
        }
        let deletion = Span::new(
            first.span.source_id,
            line_start,
            span_end_after_following_newlines(self.source, first.span.end()),
        );
        if [header, deletion].iter().any(|span| {
            self.source
                .get(span.range())
                .is_none_or(|source| source.contains('#'))
        }) {
            return;
        }
        let line_end = self.source[first.span.start()..]
            .find('\n')
            .map_or(self.source.len(), |offset| first.span.start() + offset);
        if self.source[first.span.start()..line_end].contains('#') {
            return;
        }
        let edit = Span::new(header.source_id, header.start(), deletion.end());
        let replacement = format!(
            "for {octet} in {source}{}",
            &self.source[header.end()..line_start]
        );
        let mut candidate = self.source.to_owned();
        candidate.replace_range(edit.range(), &replacement);
        let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
            edit.source_id,
            &candidate,
        );
        if !parsed.diagnostics.is_empty()
            || !xsh::frontend::check::Checker::check_arena(&parsed.arena, &candidate)
                .diagnostics
                .is_empty()
        {
            return;
        }
        self.diagnostics.push(
            Diagnostic::warning("iterate over bytes instead of constructing unused offsets")
                .with_code(DiagnosticCode::LintPreferScalarIteration)
                .with_label(Label::secondary(
                    header,
                    "the immutable Bytes source covers the exact full range",
                ))
                .with_fix_hint(FixHint::replacement(
                    edit,
                    "bind each byte directly",
                    replacement,
                )),
        );
    }

    fn lint_map_entry_iteration(
        &mut self,
        stmt_id: StmtId,
        target: BindingTargetId,
        iter: ExprId,
        block: BlockId,
    ) {
        let ArenaBindingTargetKind::Name(key) = self.arena.binding_target(target).kind else {
            return;
        };
        let ArenaExprKind::Call { callee, args } = self.arena.expr(iter).kind else {
            return;
        };
        if !self.arena.call_args(args).is_empty() {
            return;
        }
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if name.as_str() != "keys" {
            return;
        }
        let ArenaExprKind::Ident(map) = self.arena.expr(base).kind else {
            return;
        };
        // Entry iteration retains a value snapshot. A mutable map may change
        // inside a value block before a later key lookup reads its current value.
        if !self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(map.as_str().as_str()))
            .is_some_and(|binding| !binding.mutable)
        {
            return;
        }
        if self.assigned_names.contains(&map)
            || key == map
            || !matches!(
                self.expr_types.get(&self.arena.expr(base).span),
                Some(Type::Map(_, _))
            )
        {
            return;
        }
        let statements: Vec<_> = self
            .arena
            .stmt_ids(self.arena.block(block).statements)
            .collect();
        if statements.len() < 2 {
            return;
        }
        let first = self.arena.stmt(statements[0]);
        let ArenaStmtKind::Let {
            target: value_target,
            ty: None,
            initializer: ArenaExprOrRun::Expr(lookup),
        } = first.kind
        else {
            return;
        };
        let ArenaBindingTargetKind::Name(value) = self.arena.binding_target(value_target).kind
        else {
            return;
        };
        if value == map || value == key || value.as_str() == "_" {
            return;
        }
        let ArenaExprKind::Try(lookup) = self.arena.expr(lookup).kind else {
            return;
        };
        let ArenaExprKind::Call { callee, args } = self.arena.expr(lookup).kind else {
            return;
        };
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if name.as_str() != "get"
            || !matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(name) if name == map)
        {
            return;
        }
        let arguments = self.arena.call_args(args);
        if arguments.len() != 1
            || !matches!(arguments[0].kind, ArenaCallArgKind::Positional(expr) if matches!(self.arena.expr(expr).kind, ArenaExprKind::Ident(name) if name == key))
        {
            return;
        }
        let header = Span::new(
            first.span.source_id,
            self.arena.stmt(stmt_id).span.start(),
            self.arena.expr(iter).span.end(),
        );
        let line_start = self.source[..first.span.start()]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        if !self.source[line_start..first.span.start()]
            .trim()
            .is_empty()
        {
            return;
        }
        let deletion = Span::new(
            first.span.source_id,
            line_start,
            span_end_after_following_newlines(self.source, first.span.end()),
        );
        if [header, deletion].iter().any(|span| {
            self.source
                .get(span.range())
                .is_none_or(|source| source.contains('#'))
        }) {
            return;
        }
        let line_end = self.source[first.span.start()..]
            .find('\n')
            .map_or(self.source.len(), |offset| first.span.start() + offset);
        if self.source[first.span.start()..line_end].contains('#') {
            return;
        }
        let key_field = if key.as_str() == "key" {
            "key".to_string()
        } else {
            format!("key: {key}")
        };
        let value_field = if value.as_str() == "value" {
            "value".to_string()
        } else {
            format!("value: {value}")
        };
        let edit = Span::new(header.source_id, header.start(), deletion.end());
        let replacement = format!(
            "for {{{key_field}, {value_field}}} in {map}{}",
            &self.source[header.end()..line_start]
        );
        self.diagnostics.push(
            Diagnostic::warning("iterate over map entries instead of keys followed by a lookup")
                .with_code(DiagnosticCode::LintPreferMapEntryIteration)
                .with_label(Label::secondary(
                    header,
                    "this checked map stays stable across the loop",
                ))
                .with_fix_hint(FixHint::replacement(
                    edit,
                    "bind the map entry and remove its redundant lookup",
                    replacement,
                )),
        );
    }

    fn lint_list_comp_suggestions(&mut self, stmts: &[StmtId]) {
        for pair in stmts.windows(2) {
            self.lint_suggest_list_comp(pair[0], pair[1]);
            self.lint_suggest_map_comp(pair[0], pair[1]);
        }
    }

    fn lint_stream_producer_suggestions(&mut self, stmts: &[StmtId]) {
        let mut candidates = Vec::new();
        collect_stream_producer_candidates(self.arena, stmts, &mut candidates);
        if candidates.is_empty() {
            return;
        }
        let mut consumed = FxHashSet::default();
        collect_lazy_consumed_calls(self.arena, stmts, &mut consumed);
        for candidate in candidates {
            if !consumed.contains(&candidate.function_name) {
                continue;
            }
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    format!(
                        "proc `{}` builds list `{}` item-by-item and is consumed lazily; consider a `stream` producer",
                        candidate.function_name, candidate.accumulator_name,
                    ),
                )
                .with_code(DiagnosticCode::LintPreferStreamProducer)
                .with_label(Label::secondary(
                    candidate.span,
                    "`yield` can avoid materializing this list for direct stream consumers",
                )),
            );
        }
    }

    fn accumulator_qualifiers(
        &self,
        mut stmt: StmtId,
        accumulator: Name,
    ) -> Option<(Vec<String>, StmtId)> {
        let mut qualifiers = Vec::new();
        loop {
            let (block, loop_body) = match self.arena.stmt(stmt).kind {
                ArenaStmtKind::For {
                    target,
                    iter,
                    block,
                } => {
                    if binding_target_contains_name(self.arena, target, accumulator)
                        || expr_references_name(self.arena, iter, accumulator)
                    {
                        return None;
                    }
                    // Result iterables in a statement loop require explicit propagation;
                    // that spelling survives in the comprehension's iterable expression.
                    let iter_src = self.source.get(self.arena.expr(iter).span.range())?;
                    qualifiers.push(format!(
                        "for {} in {iter_src}",
                        format_binding_target(self.arena, target)
                    ));
                    (block, true)
                }
                ArenaStmtKind::If {
                    branches,
                    else_block: None,
                } => {
                    let [branch] = self.arena.if_branches(branches) else {
                        return None;
                    };
                    if matches!(
                        self.arena.expr(branch.condition).kind,
                        ArenaExprKind::PatternCondition { .. }
                    ) {
                        return None;
                    }
                    if expr_references_name(self.arena, branch.condition, accumulator) {
                        return None;
                    }
                    let condition = self
                        .source
                        .get(self.arena.expr(branch.condition).span.range())?;
                    qualifiers.push(format!("if {condition}"));
                    (branch.block, false)
                }
                _ => break,
            };
            let block = self.arena.block(block);
            if !block.params.is_empty() {
                return None;
            }
            let mut statements = self.arena.stmt_ids(block.statements).peekable();
            // A loop body may skip items before it accumulates. Each leading
            // `continue unless c` keeps exactly the items `if c` keeps, and
            // the guards run in the order the qualifiers do.
            while loop_body
                && let Some(kept) = statements
                    .peek()
                    .and_then(|&guard| self.kept_item_condition(guard, accumulator))
            {
                qualifiers.push(format!("if {kept}"));
                statements.next();
            }
            stmt = statements.next()?;
            if statements.next().is_some() {
                return None;
            }
        }
        if qualifiers
            .first()
            .is_none_or(|clause| !clause.starts_with("for "))
        {
            return None;
        }
        Some((qualifiers, stmt))
    }

    /// The condition under which a loop body goes on past `guard`, when
    /// `guard` is `continue unless CONDITION` or `continue when CONDITION`:
    /// the condition as written, or its negation.
    fn kept_item_condition(&self, guard: StmtId, accumulator: Name) -> Option<String> {
        let guard = self.arena.stmt(guard);
        let ArenaStmtKind::Sugar {
            form: form @ (SugarForm::When | SugarForm::Unless),
            operands,
            ..
        } = guard.kind
        else {
            return None;
        };
        let ArenaSugar::Guarded {
            stmt,
            negate,
            condition,
        } = self.arena.sugar(form, operands)
        else {
            return None;
        };
        if !matches!(self.arena.stmt(stmt).kind, ArenaStmtKind::Continue)
            || expr_references_name(self.arena, condition, accumulator)
        {
            return None;
        }
        // The statement's text keeps grouping that the condition's span can omit.
        let text = self.source.get(guard.span.range())?.trim_end();
        let written = text
            .strip_prefix("continue")?
            .trim_start()
            .strip_prefix(if negate { "unless" } else { "when" })?;
        if !written.starts_with(char::is_whitespace) {
            return None;
        }
        let written = written.trim_start();
        if negate {
            return Some(written.to_string());
        }
        let end = guard.span.start() + text.len();
        self.negated_condition(
            condition,
            Span::new(guard.span.source_id, end - written.len(), end),
        )
    }

    /// `condition` negated in the spelling the formatter prints, for the
    /// shapes whose negation needs no new grouping; `written` is its text.
    fn negated_condition(&self, condition: ExprId, written: Span) -> Option<String> {
        let text = self.source.get(written.range())?;
        match self.arena.expr(condition).kind {
            ArenaExprKind::Unary {
                op: UnaryOp::Not, ..
            } => {
                let operand = text.strip_prefix('!')?.trim_start();
                (!operand.starts_with('(')).then(|| operand.to_string())
            }
            ArenaExprKind::Ident(_)
            | ArenaExprKind::Field { .. }
            | ArenaExprKind::Call { .. }
            | ArenaExprKind::Index { .. }
                if !text.starts_with('(') =>
            {
                Some(format!("! {text}"))
            }
            ArenaExprKind::Binary {
                op: op @ (BinaryOp::Eq | BinaryOp::Ne),
                left,
                right,
            } => {
                let (original, negated) = if op == BinaryOp::Eq {
                    ("==", "!=")
                } else {
                    ("!=", "==")
                };
                let between = self.arena.expr(left).span.end()..self.arena.expr(right).span.start();
                let operator = between.start + self.source.get(between.clone())?.find(original)?;
                Some(format!(
                    "{}{negated}{}",
                    self.source.get(written.start()..operator)?,
                    self.source.get(operator + original.len()..written.end())?
                ))
            }
            _ => None,
        }
    }

    /// The element one accumulating statement appends to the list `name`:
    /// `name = name.push(ELEMENT)` or `name += [ELEMENT]`.
    fn appended_list_element(&self, stmt: StmtId, name: Name) -> Option<ExprId> {
        let ArenaStmtKind::Assign {
            target,
            op,
            value: ArenaExprOrRun::Expr(value),
        } = self.arena.stmt(stmt).kind
        else {
            return None;
        };
        if !matches!(
            self.arena.assign_target(target).kind,
            ArenaAssignTargetKind::Name(found) if found == name
        ) {
            return None;
        }
        match (op, self.arena.expr(value).kind) {
            (AssignOp::Set, ArenaExprKind::Call { callee, args }) => {
                let ArenaExprKind::Field { base, name: method } = self.arena.expr(callee).kind
                else {
                    return None;
                };
                if method != "push"
                    || !matches!(
                        self.arena.expr(base).kind,
                        ArenaExprKind::Ident(found) if found == name
                    )
                {
                    return None;
                }
                match self.arena.call_args(args) {
                    [arg] => match arg.kind {
                        ArenaCallArgKind::Positional(element) => Some(element),
                        _ => None,
                    },
                    _ => None,
                }
            }
            (AssignOp::Add, ArenaExprKind::List(items)) => {
                let mut items = self.arena.list_elements(items);
                match (items.next(), items.next()) {
                    (Some(item), None) if item.splice_span.is_none() => Some(item.value),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn accumulator_annotation(&self, ty: Option<TypeExprId>) -> String {
        ty.and_then(|ty| self.source.get(self.arena.type_expr_span(ty).range()))
            .map(|text| format!(": {text}"))
            .unwrap_or_default()
    }

    fn accumulator_replacement(
        &self,
        span: Span,
        name: Name,
        annotation: &str,
        open: &str,
        projection: &str,
        close: &str,
        qualifiers: &[String],
    ) -> String {
        if qualifiers
            .iter()
            .filter(|clause| clause.starts_with("for "))
            .count()
            == 1
            && qualifiers.len() <= 2
        {
            let one_line = format!(
                "var {name}{annotation} = {open}{projection} {}{close}\n",
                qualifiers.join(" ")
            );
            // The formatter breaks a comprehension that overflows the line.
            let line_start = self.source[..span.start()]
                .rfind('\n')
                .map_or(0, |offset| offset + 1);
            let width = self.source[line_start..span.start()].chars().count()
                + one_line.trim_end().chars().count();
            if width <= super::format::DEFAULT_LINE_WIDTH && !one_line.trim_end().contains('\n') {
                return one_line;
            }
        }
        let line_start = self.source[..span.start()]
            .rfind('\n')
            .map_or(0, |offset| offset + 1);
        let indent = &self.source[line_start..span.start()];
        let mut text = format!("var {name}{annotation} = {open}\n{indent}  {projection}\n");
        for qualifier in qualifiers {
            text.push_str(&format!("{indent}  {qualifier}\n"));
        }
        text.push_str(&format!("{indent}{close}\n"));
        text
    }

    /// A forwarding loop can be replaced only when no body work or binder
    /// conversion is lost and the checked source already has iterable type.
    fn lint_yield_delegation(
        &mut self,
        span: Span,
        target: BindingTargetId,
        iter: ExprId,
        block: BlockId,
    ) {
        let Some(Type::Stream(expected)) = self.function_return_types.last() else {
            return;
        };
        let ArenaBindingTargetKind::Name(binding) = self.arena.binding_target(target).kind else {
            return;
        };
        let statements: Vec<_> = self
            .arena
            .stmt_ids(self.arena.block(block).statements)
            .collect();
        if statements.len() != 1 {
            return;
        }
        let ArenaStmtKind::Yield(ArenaExprOrRun::Expr(value)) = self.arena.stmt(statements[0]).kind
        else {
            return;
        };
        if !matches!(self.arena.expr(value).kind, ArenaExprKind::Ident(name) if name == binding) {
            return;
        }
        // Direct loop pipelines can fuse into a lazy cursor while expression
        // pipelines collect first. Keep that consumer boundary explicit.
        if matches!(
            self.arena.expr(iter).kind,
            ArenaExprKind::Pipeline { .. } | ArenaExprKind::StructuredPipeline { .. }
        ) {
            return;
        }
        let Some(Type::List(item) | Type::Stream(item)) =
            self.expr_types.get(&self.arena.expr(iter).span)
        else {
            return;
        };
        if item != expected || matches!(item.as_ref(), Type::Any | Type::Unknown) {
            return;
        }
        if self
            .source
            .get(span.range())
            .is_none_or(|text| text.contains('#'))
        {
            return;
        }
        let Some(source) = self.source.get(self.arena.expr(iter).span.range()) else {
            return;
        };
        let source = if source.contains('\n') {
            format!("({source})")
        } else {
            source.to_string()
        };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "delegate a transparent forwarding loop with `yield @`",
            )
            .with_code(DiagnosticCode::LintPreferYieldDelegation)
            .with_label(Label::secondary(
                span,
                "this loop only yields its current item",
            ))
            .with_fix_hint(FixHint::replacement(
                span,
                "delegate the iterable",
                format!("yield @{source}"),
            )),
        );
    }

    fn lint_suggest_list_comp(&mut self, var_id: StmtId, for_id: StmtId) {
        let var_stmt = self.arena.stmt(var_id);
        let for_stmt = self.arena.stmt(for_id);
        // Match: var <name> = []
        let ArenaStmtKind::Var {
            target,
            ty,
            initializer: ArenaExprOrRun::Expr(init),
        } = var_stmt.kind
        else {
            return;
        };
        let ArenaBindingTargetKind::Name(var_name) = self.arena.binding_target(target).kind else {
            return;
        };
        let ArenaExprKind::List(items) = self.arena.expr(init).kind else {
            return;
        };
        if !items.is_empty() {
            return;
        }
        let Some((qualifiers, push_stmt_id)) = self.accumulator_qualifiers(for_id, var_name) else {
            return;
        };
        let Some(push_expr) = self.appended_list_element(push_stmt_id, var_name) else {
            return;
        };
        if expr_references_name(self.arena, push_expr, var_name) {
            return;
        }
        if let Some(ty) = ty
            && !matches!(Type::from_arena(self.arena, ty), Type::List(_))
        {
            return;
        }
        let push_span = self.arena.expr(push_expr).span;
        let Some(push_src) = self.source.get(push_span.range()) else {
            return;
        };
        let annotation = self.accumulator_annotation(ty);
        let replacement = self.accumulator_replacement(
            var_stmt.span,
            var_name,
            &annotation,
            "[",
            push_src,
            "]",
            &qualifiers,
        );
        let combined = Span::new(
            var_stmt.span.source_id,
            var_stmt.span.start(),
            span_end_after_following_newlines(self.source, for_stmt.span.end()),
        );
        if self
            .source
            .get(combined.range())
            .is_none_or(|text| text.contains('#'))
        {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!(
                    "use a list comprehension instead of building `{var_name}` with a for loop"
                ),
            )
            .with_code(DiagnosticCode::LintPreferListComp)
            .with_label(Label::secondary(
                for_stmt.span,
                "this for loop only builds a list",
            ))
            .with_fix_hint(FixHint::replacement(
                combined,
                "convert to list comprehension",
                replacement,
            )),
        );
    }

    fn lint_suggest_map_comp(&mut self, var_id: StmtId, for_id: StmtId) {
        let var_stmt = self.arena.stmt(var_id);
        let for_stmt = self.arena.stmt(for_id);
        let ArenaStmtKind::Var {
            target,
            initializer: ArenaExprOrRun::Expr(init),
            ty,
        } = var_stmt.kind
        else {
            return;
        };
        let ArenaBindingTargetKind::Name(var_name) = self.arena.binding_target(target).kind else {
            return;
        };
        let empty_literal = matches!(self.arena.expr(init).kind, ArenaExprKind::Record(fields) if fields.is_empty());
        let map_context = ty
            .is_some_and(|ty| matches!(Type::from_arena(self.arena, ty), Type::Map(_, _)))
            || matches!(
                self.expr_types.get(&self.arena.expr(init).span),
                Some(Type::Map(_, _))
            );
        if !(is_map_empty_call(self.arena, init) || empty_literal && map_context) {
            return;
        }
        let Some((qualifiers, assign_stmt_id)) = self.accumulator_qualifiers(for_id, var_name)
        else {
            return;
        };
        let ArenaStmtKind::Assign {
            target: assign_target,
            op: AssignOp::Set,
            value: ArenaExprOrRun::Expr(value),
        } = self.arena.stmt(assign_stmt_id).kind
        else {
            return;
        };
        let ArenaAssignTargetKind::Index { base, index } =
            self.arena.assign_target(assign_target).kind
        else {
            return;
        };
        let ArenaAssignTargetKind::Name(assign_name) = self.arena.assign_target(base).kind else {
            return;
        };
        if assign_name != var_name {
            return;
        }
        if expr_references_name(self.arena, index, var_name)
            || expr_references_name(self.arena, value, var_name)
        {
            return;
        }
        let index_span = self.arena.expr(index).span;
        let value_span = self.arena.expr(value).span;
        let Some(key_src) = self.source.get(index_span.start()..index_span.end()) else {
            return;
        };
        let Some(value_src) = self.source.get(value_span.start()..value_span.end()) else {
            return;
        };
        let annotation = self.accumulator_annotation(ty);
        let projection = if map_comp_key_can_be_bare(self.arena, index) {
            format!("{key_src}: {value_src}")
        } else {
            format!("[{key_src}]: {value_src}")
        };
        let replacement = self.accumulator_replacement(
            var_stmt.span,
            var_name,
            &annotation,
            "{",
            &projection,
            "}",
            &qualifiers,
        );
        let combined = Span::new(
            var_stmt.span.source_id,
            var_stmt.span.start(),
            span_end_after_following_newlines(self.source, for_stmt.span.end()),
        );
        if self
            .source
            .get(combined.range())
            .is_none_or(|text| text.contains('#'))
        {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("use a map comprehension instead of building `{var_name}` with a for loop"),
            )
            .with_code(DiagnosticCode::LintPreferMapComp)
            .with_label(Label::secondary(
                for_stmt.span,
                "this for loop only builds a map",
            ))
            .with_fix_hint(FixHint::replacement(
                combined,
                "convert to map comprehension",
                replacement,
            )),
        );
    }

    fn lint_command_stmt(&mut self, stmt_id: CommandStmtId) {
        let stmt = self.arena.command_stmt(stmt_id).clone();
        let span = self.arena.span(stmt.span);
        match stmt.command {
            ArenaCommand::Proc { name, args } => {
                self.lint_interactive_command(name.as_str().as_str(), span);
                self.lint_proc_command_args(args);
            }
            ArenaCommand::Core {
                name: _,
                args,
                env,
                block,
            } => {
                self.lint_command_args(args);
                for assignment in self.arena.env_assignments(env).to_vec() {
                    self.lint_env_assignment_value(&assignment.value);
                }
                if let Some(block) = block {
                    self.lint_block(block);
                }
            }
            ArenaCommand::Run(run) => self.lint_run(run),
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
    fn lint_run(&mut self, run_id: RunFormId) {
        let run = self.arena.run_form(run_id).clone();
        if self.runless {
            for segment in self.arena.run_segments(run.segments).to_vec() {
                let seg_span = self.arena.span(segment.span);
                let name = command_name(self.arena, self.source, &segment.target);
                let exempt = name
                    .as_deref()
                    .is_some_and(|n| self.runless_except.iter().any(|e| e == n));
                if !exempt {
                    let label = match &name {
                        Some(n) => format!("`{n}` is an external command"),
                        None => "external command not allowed in runless mode".to_string(),
                    };
                    self.diagnostics.push(
                        Diagnostic::new(
                            Severity::Error,
                            "external command not permitted (--runless)",
                        )
                        .with_code(DiagnosticCode::LintRunless)
                        .with_label(Label::secondary(seg_span, label)),
                    );
                }
            }
        }
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: false,
        }
        .visit_run_form(run_id);
    }

    fn lint_command_args(&mut self, args: ArenaRange) {
        for arg in self.arena.command_args(args).to_vec() {
            self.lint_command_arg(&arg);
        }
    }

    fn lint_proc_command_args(&mut self, args: ArenaRange) {
        for arg in self.arena.command_args(args).to_vec() {
            self.lint_proc_command_arg(&arg);
        }
    }

    fn lint_env_assignment_value(&mut self, value: &ArenaEnvAssignmentValue) {
        match value {
            ArenaEnvAssignmentValue::CommandArg(arg) => self.lint_proc_command_arg(arg),
            ArenaEnvAssignmentValue::Expr(expr) => self.lint_expr(*expr),
        }
    }

    fn lint_list_compound_assignment(
        &mut self,
        target: AssignTargetId,
        value: ArenaExprOrRun,
        span: Span,
    ) {
        let ArenaAssignTargetKind::Name(target_name) = self.arena.assign_target(target).kind else {
            return;
        };
        let Some((scope_depth, binding)) = self
            .scopes
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
        let arguments_preserve_target = updates.iter().zip(&argument_sources).all(
            |(update, argument_source)| {
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
            },
        );
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
            if prefix.chars().count() + one_line.chars().count()
                > super::format::DEFAULT_LINE_WIDTH
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
            diagnostic = diagnostic.with_note("comments inside the update require a manual rewrite");
        } else {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                edit_span,
                "rewrite as list compound assignment",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_assign_target(&mut self, target: AssignTargetId) {
        match self.arena.assign_target(target).kind.clone() {
            ArenaAssignTargetKind::Name(name) => self.mark_used(name.as_str().as_str()),
            ArenaAssignTargetKind::Env(_) => {}
            ArenaAssignTargetKind::Field { base, .. } => self.lint_assign_target(base),
            ArenaAssignTargetKind::Index { base, index } => {
                self.lint_assign_target(base);
                self.lint_expr(index);
            }
        }
    }

    fn lint_command_arg(&mut self, arg: &ArenaCommandArg) {
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: true,
        }
        .visit_command_arg(arg, false);
    }

    fn lint_proc_command_arg(&mut self, arg: &ArenaCommandArg) {
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: true,
        }
        .visit_command_arg(arg, true);
    }

    fn lint_expr_or_run(&mut self, value: &ArenaExprOrRun) {
        match value {
            ArenaExprOrRun::Expr(expr) => self.lint_expr(*expr),
            ArenaExprOrRun::Run(run) => self.lint_run(*run),
        }
    }

    fn pipeline_argument_stable(&self, value: ExprId) -> bool {
        match self.arena.expr(value).kind {
            ArenaExprKind::Ident(name) => {
                !self.assigned_names.contains(&name)
                    && self
                        .scopes
                        .iter()
                        .rev()
                        .find_map(|scope| scope.get(name.as_str().as_str()))
                        .is_some_and(|binding| !binding.mutable)
            }
            ArenaExprKind::Null
            | ArenaExprKind::Bool(_)
            | ArenaExprKind::Int(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::Str(_)
            | ArenaExprKind::PathStr(_)
            | ArenaExprKind::Bytes(_)
            | ArenaExprKind::Regex(_) => true,
            _ => false,
        }
    }

    fn pipeline_ordinary_call(&self, value: ExprId) -> Option<(ExprId, ArenaRange)> {
        let call = match self.arena.expr(value).kind {
            ArenaExprKind::Try(inner) => inner,
            _ => value,
        };
        let ArenaExprKind::Call { callee, args } = self.arena.expr(call).kind else {
            return None;
        };
        // A guarded receiver could skip the original input argument entirely.
        // Direct callable names have no receiver evaluation to move across.
        if !matches!(self.arena.expr(callee).kind, ArenaExprKind::Ident(_)) {
            return None;
        }
        if self
            .arena
            .call_args(args)
            .iter()
            .any(|arg| pipeline_argument_expr(arg).is_none())
        {
            return None;
        }
        Some((callee, args))
    }

    fn pipeline_rewrite_preserves_types(
        &self,
        edit: Span,
        replacement: &str,
        old_value: ExprId,
        old_input: ExprId,
        new_value_start: usize,
        new_input_start: usize,
    ) -> bool {
        let Some(old_type) = self.expr_types.get(&self.arena.expr(old_value).span) else {
            return false;
        };
        let Some(input_type) = self.expr_types.get(&self.arena.expr(old_input).span) else {
            return false;
        };
        if !list_splice_element_type_is_precise(old_type)
            || !list_splice_element_type_is_precise(input_type)
        {
            return false;
        }
        let old_shape = checked_return_type_shape(old_type);
        let input_shape = checked_return_type_shape(input_type);
        let mut rewritten = self.source.to_string();
        rewritten.replace_range(edit.range(), replacement);
        let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
            edit.source_id,
            &rewritten,
        );
        if !parsed.diagnostics.is_empty() {
            return false;
        }
        let checked = xsh::frontend::check::Checker::check_arena(&parsed.arena, &rewritten);
        if !checked.diagnostics.is_empty() {
            return false;
        }
        parsed.arena.symbol_owner().with_current(|| {
            (0..parsed.arena.arena.expr_tags.len()).any(|raw| {
                let expr = parsed.arena.arena.expr(ExprId::from_index(raw));
                let ArenaExprKind::ValuePipelineCall { input, .. } = expr.kind else {
                    return false;
                };
                let input_span = parsed.arena.arena.expr(input).span;
                expr.span.start() == new_value_start
                    && input_span.start() == new_input_start
                    && input_span.range().len() == self.arena.expr(old_input).span.range().len()
                    && checked
                        .expr_types
                        .get(&expr.span)
                        .is_some_and(|ty| checked_return_type_shape(ty) == old_shape)
                    && checked
                        .expr_types
                        .get(&input_span)
                        .is_some_and(|ty| checked_return_type_shape(ty) == input_shape)
            })
        })
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

    fn lint_linear_value_pipeline(
        &mut self,
        previous: StmtId,
        current: StmtId,
        statements: &[StmtId],
    ) {
        let first = self.arena.stmt(previous);
        let second = self.arena.stmt(current);
        let ArenaStmtKind::Let {
            target,
            ty: None,
            initializer: ArenaExprOrRun::Expr(input),
        } = first.kind
        else {
            return;
        };
        let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind else {
            return;
        };
        if (self.pipeline_ordinary_call(input).is_none()
            && !matches!(
                self.arena.expr(input).kind,
                ArenaExprKind::ValuePipelineCall { .. }
            ))
            || self.assigned_names.contains(&name)
        {
            return;
        }
        let value = match second.kind {
            ArenaStmtKind::Let {
                initializer: ArenaExprOrRun::Expr(value),
                ..
            }
            | ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(value)))
            | ArenaStmtKind::Expr(value) => value,
            _ => return,
        };
        let Some((_, args)) = self.pipeline_ordinary_call(value) else {
            return;
        };
        let holes = self.arena.call_args(args).iter().filter_map(|arg| {
            let expr = pipeline_argument_expr(arg).unwrap();
            matches!(self.arena.expr(expr).kind, ArenaExprKind::Ident(candidate) if candidate == name).then_some(expr)
        }).collect::<Vec<_>>();
        let [hole] = holes.as_slice() else {
            return;
        };
        let Some(last) = statements.last() else {
            return;
        };
        let end = self.arena.stmt(*last).span.end();
        let references = (0..self.arena.expr_tags.len())
            .filter(|raw| {
                let expr = self.arena.expr(ExprId::from_index(*raw));
                expr.span.source_id == first.span.source_id
                    && expr.span.start() >= first.span.end()
                    && expr.span.end() <= end
                    && matches!(expr.kind, ArenaExprKind::Ident(candidate) if candidate == name)
            })
            .count();
        let shorthand = self.arena.record_fields.iter().any(|field| matches!(field.kind, ArenaRecordFieldKind::Shorthand { name: candidate, span }
            if candidate == name && self.arena.span(span).source_id == first.span.source_id && self.arena.span(span).start() >= first.span.end() && self.arena.span(span).end() <= end));
        if references != 1 || shorthand {
            return;
        }
        let span = Span::new(first.span.source_id, first.span.start(), second.span.end());
        let value_span = self.arena.expr(value).span;
        let input_span = self.arena.expr(input).span;
        let hole_span = self.arena.expr(*hole).span;
        let Some(input_text) = self.source.get(input_span.range()) else {
            return;
        };
        let Some(value_text) = self.source.get(value_span.range()) else {
            return;
        };
        let mut stage = value_text.to_string();
        stage.replace_range(
            hole_span.start() - value_span.start()..hole_span.end() - value_span.start(),
            "_",
        );
        let pipeline = format!("{input_text} |> {stage}");
        let Some(second_text) = self.source.get(second.span.range()) else {
            return;
        };
        let mut replacement = second_text.to_string();
        replacement.replace_range(
            value_span.start() - second.span.start()..value_span.end() - second.span.start(),
            &pipeline,
        );
        let new_start = first.span.start() + value_span.start() - second.span.start();
        if !self.pipeline_rewrite_preserves_types(
            span,
            &replacement,
            value,
            input,
            new_start,
            new_start,
        ) {
            return;
        }
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "single-use temporary forms a value pipeline",
        )
        .with_code(DiagnosticCode::LintPreferValuePipeline)
        .with_label(Label::secondary(
            span,
            "retain the input directly in the following ordinary call",
        ));
        if self
            .source
            .get(span.range())
            .is_some_and(|source| !source.contains('#'))
        {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "collapse the single-use temporary",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_optional_postfix(&mut self, expr: ExprId) {
        let expression = self.arena.expr(expr);
        let ArenaExprKind::If {
            branches,
            else_value,
        } = expression.kind
        else {
            return;
        };
        let [branch] = self.arena.if_expr_branches(branches) else {
            return;
        };
        let ArenaExprKind::Binary { op, left, right } = self.arena.expr(branch.condition).kind
        else {
            return;
        };
        if !matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            return;
        }
        let receiver = if matches!(self.arena.expr(right).kind, ArenaExprKind::Null) {
            left
        } else if matches!(self.arena.expr(left).kind, ArenaExprKind::Null) {
            right
        } else {
            return;
        };
        let ArenaExprKind::Ident(name) = self.arena.expr(receiver).kind else {
            return;
        };
        if self.assigned_names.contains(&name)
            || !matches!(
                self.expr_types.get(&self.arena.expr(receiver).span),
                Some(Type::Optional(_))
            )
        {
            return;
        }
        let (present, absent) = if op == BinaryOp::Eq {
            (else_value, branch.value)
        } else {
            (branch.value, else_value)
        };
        let single_value = |value| {
            if let ArenaExprKind::ValueBlock(block) = self.arena.expr(value).kind {
                let mut statements = self.arena.stmt_ids(self.arena.block(block).statements);
                let statement = statements.next()?;
                if statements.next().is_some() {
                    return None;
                }
                match self.arena.stmt(statement).kind {
                    ArenaStmtKind::Expr(value) => Some(value),
                    _ => None,
                }
            } else {
                Some(value)
            }
        };
        let Some(present) = single_value(present) else {
            return;
        };
        let Some(absent) = single_value(absent) else {
            return;
        };
        let present_expr = self.arena.expr(present);
        let Some(present_ty) = self.expr_types.get(&present_expr.span) else {
            return;
        };
        let absent_is_null = matches!(self.arena.expr(absent).kind, ArenaExprKind::Null);
        if matches!(
            present_ty,
            Type::Null | Type::Any | Type::Unknown | Type::Invalid
        ) || (!absent_is_null
            && (matches!(present_ty, Type::Optional(_))
                || self.expr_types.get(&self.arena.expr(absent).span) != Some(present_ty)))
        {
            return;
        }
        let (base, insertion) = match present_expr.kind {
            ArenaExprKind::Field { base, .. } => (base, self.arena.expr(base).span.end()),
            ArenaExprKind::Call { callee, .. } => match self.arena.expr(callee).kind {
                ArenaExprKind::Field { base, .. } => (base, self.arena.expr(base).span.end()),
                _ => return,
            },
            ArenaExprKind::Index {
                base,
                guarded: false,
                ..
            }
            | ArenaExprKind::Slice {
                base,
                guarded: false,
                ..
            } => (base, self.arena.expr(base).span.end()),
            _ => return,
        };
        if !matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(found) if found == name) {
            return;
        }
        let Some(original) = self
            .source
            .get(expression.span.start()..expression.span.end())
        else {
            return;
        };
        if original.contains('#') {
            return;
        }
        let Some(before) = self.source.get(present_expr.span.start()..insertion) else {
            return;
        };
        let Some(after) = self.source.get(insertion..present_expr.span.end()) else {
            return;
        };
        let absent_span = self.arena.expr(absent).span;
        let Some(fallback) = self.source.get(absent_span.start()..absent_span.end()) else {
            return;
        };
        let replacement = if absent_is_null {
            format!("({before}?{after})")
        } else {
            format!("({before}?{after} ?? ({fallback}))")
        };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "explicit null branch can use a guarded postfix",
            )
            .with_code(DiagnosticCode::LintPreferOptionalPostfix)
            .with_label(Label::secondary(
                expression.span,
                "guard the receiver and retain the lazy fallback",
            ))
            .with_fix_hint(FixHint::replacement(
                expression.span,
                "use guarded postfix and fallback",
                replacement,
            )),
        );
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

    fn lint_proven_nonnull_fallback(&mut self, expr: ExprId) {
        let expression = self.arena.expr(expr);
        let ArenaExprKind::Binary {
            op: BinaryOp::ResultFallback,
            left,
            right,
        } = expression.kind
        else {
            return;
        };
        if !self
            .proven_nonnull_fallback_receivers
            .contains(&self.arena.expr(left).span)
            || !inert_constant_initializer(self.arena, right)
            || matches!(self.arena.expr(right).kind, ArenaExprKind::Regex(_))
            || self
                .source
                .get(expression.span.range())
                .is_none_or(|source| source.contains('#'))
        {
            return;
        }
        let Some(expected) = self.expr_types.get(&self.arena.expr(left).span) else {
            return;
        };
        let Some(value) = xsh::frontend::check::LiteralConstant::analyze(
            self.arena,
            right,
            &FxHashMap::default(),
        ) else {
            return;
        };
        if !value.matches_data_type(expected) {
            return;
        }
        let Some(replacement) = self.source.get(self.arena.expr(left).span.range()) else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "this Optional receiver is proved present",
            )
            .with_code(DiagnosticCode::LintRedundantOptionalFallback)
            .with_label(Label::secondary(
                expression.span,
                "the fallback cannot be reached",
            ))
            .with_fix_hint(FixHint::replacement(
                expression.span,
                "use the proved present value",
                replacement,
            )),
        );
    }

    fn lint_expr(&mut self, expr: ExprId) {
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: false,
        }
        .visit_expr(expr);
    }

    fn lint_builder_block(&mut self, block: BuilderBlockId) {
        self.push_scope();
        let entries: Vec<_> = self
            .arena
            .builder_entries(self.arena.builder_block(block).entries)
            .to_vec();
        for entry in entries {
            match entry.kind {
                ArenaBuilderEntryKind::Field { value, .. } => self.lint_expr(value),
                ArenaBuilderEntryKind::Entry { args, block, .. } => {
                    for arg in self.arena.command_args(args).to_vec() {
                        self.lint_command_arg(&arg);
                    }
                    if let Some(block) = block {
                        self.lint_builder_block(block);
                    }
                }
                ArenaBuilderEntryKind::Task { block, .. } => self.lint_block_statements(block),
                ArenaBuilderEntryKind::Stmt(stmt) => self.lint_stmt(stmt, false),
            }
        }
        self.pop_scope();
    }

    fn lint_stream_block(&mut self, block: BlockId) {
        self.push_scope();
        for param in self
            .arena
            .block_params(self.arena.block(block).params)
            .to_vec()
        {
            self.define(
                param.name.as_str().as_str(),
                self.arena.span(param.span),
                true,
            );
        }
        self.lint_block_statements(block);
        self.pop_scope();
    }

    fn lint_stream_stage(&mut self, stage: &ArenaStreamStage) {
        self.lint_stage_callable_wrapper(stage);
        for arg in self.arena.call_args(stage.args).to_vec() {
            self.lint_call_arg(&arg);
        }
        if let Some(block) = stage.block {
            let first_diagnostic = self.diagnostics.len();
            self.lint_stream_block(block);
            let span = self.arena.span(self.arena.block(block).span);
            if let Some(text) = self.source.get(span.range())
                && !text.trim_start().starts_with('{')
            {
                // An inline predicate beginning with parentheses is parsed as
                // stage arguments. Keep rewritten expressions inside the same
                // callback by giving its synthetic block explicit delimiters.
                let mut edits: Vec<_> = self.diagnostics[first_diagnostic..]
                    .iter()
                    .filter(|diagnostic| {
                        matches!(
                            diagnostic.code,
                            Some(DiagnosticCode::LintPreferIn | DiagnosticCode::LintCoreAssert)
                        )
                    })
                    .flat_map(|diagnostic| &diagnostic.fix_hints)
                    .filter_map(|hint| Some((hint.span?, hint.replacement.as_ref()?)))
                    .filter(|(edit, _)| edit.start() >= span.start() && edit.end() <= span.end())
                    .collect();
                edits.sort_by_key(|(edit, _)| (edit.start(), std::cmp::Reverse(edit.end())));
                let mut end = span.start();
                edits.retain(|(edit, _)| {
                    if edit.start() < end {
                        return false;
                    }
                    end = edit.end();
                    true
                });
                if !edits.is_empty() {
                    let mut replacement = text.to_string();
                    for (edit, value) in edits.iter().rev() {
                        replacement.replace_range(
                            edit.start() - span.start()..edit.end() - span.start(),
                            value,
                        );
                    }
                    let replacement = format!("{{ {replacement} }}");
                    for diagnostic in &mut self.diagnostics[first_diagnostic..] {
                        if matches!(
                            diagnostic.code,
                            Some(DiagnosticCode::LintPreferIn | DiagnosticCode::LintCoreAssert)
                        ) {
                            for hint in &mut diagnostic.fix_hints {
                                if hint.span.is_some_and(|edit| {
                                    edit.start() >= span.start() && edit.end() <= span.end()
                                }) {
                                    *hint = FixHint::replacement(
                                        span,
                                        "preserve the stream predicate callback",
                                        replacement.clone(),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
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

    fn lint_call_arg(&mut self, arg: &ArenaCallArg) {
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: false,
        }
        .visit_call_arg(arg);
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
        if !self.prefer_inferred_variants {
            return;
        }
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
        if !self.prefer_positional_constructors {
            return;
        }
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

    fn proven_absence_lookup(&self, expr: ExprId) -> bool {
        let node = self.arena.expr(expr);
        if self.expr_types.get(&node.span) != Some(&Type::Optional(Box::new(Type::Int))) {
            return false;
        }
        match node.kind {
            ArenaExprKind::Ident(name) => {
                self.scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(name.as_str().as_str()))
                    .is_some_and(|binding| binding.absence_lookup && !binding.mutable)
                    && !self.assigned_names.contains(&name)
            }
            ArenaExprKind::Call { callee, .. } => {
                let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
                    return false;
                };
                match self.expr_types.get(&self.arena.expr(base).span) {
                    Some(Type::Str) => matches!(name.as_str().as_str(), "find" | "byte_at"),
                    Some(Type::Bytes) => name == "byte_at",
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn is_negative_one_literal(&self, expr: ExprId) -> bool {
        let ArenaExprKind::Unary {
            op: UnaryOp::Neg,
            expr,
        } = self.arena.expr(expr).kind
        else {
            return false;
        };
        matches!(self.arena.expr(expr).kind, ArenaExprKind::Int(value) if self.arena.int_literal(value).value() == Some(1))
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

    // Host-backed records may materialize other metadata during `get`. Only
    // locally constructed ordinary records and their immutable snapshots prove
    // that selecting one guaranteed field cannot drop a metadata failure.
    fn proven_materialized_record(&self, expr: ExprId) -> bool {
        if !matches!(
            self.expr_types.get(&self.arena.expr(expr).span),
            Some(Type::Record(_))
        ) {
            return false;
        }
        match self.arena.expr(expr).kind {
            ArenaExprKind::Record(_) => true,
            ArenaExprKind::Ident(name) => {
                self.scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(name.as_str().as_str()))
                    .is_some_and(|binding| binding.materialized_record && !binding.mutable)
                    && !self.assigned_names.contains(&name)
            }
            ArenaExprKind::Call { callee, .. } => self
                .record_constructors
                .resolve_call(self.arena, callee, None)
                .is_some(),
            _ => false,
        }
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
            .with_label(Label::secondary(outer.span, "this reads one named variable"))
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

    fn proven_immutable_byte_length(&self, expr: ExprId) -> Option<usize> {
        match self.arena.expr(expr).kind {
            ArenaExprKind::Bytes(bytes) => Some(self.arena.bytes_literal(bytes).len()),
            ArenaExprKind::Ident(name) if !self.assigned_names.contains(&name) => {
                let binding = self
                    .scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(name.as_str().as_str()))?;
                (!binding.mutable)
                    .then_some(binding.immutable_byte_length)
                    .flatten()
            }
            _ => None,
        }
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
            ArenaExprKind::List(items) => {
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

    fn lint_fresh_map_initializations(&mut self, statements: &[StmtId]) {
        for (index, &statement) in statements.iter().enumerate() {
            let initializer_stmt = self.arena.stmt(statement);
            let ArenaStmtKind::Var {
                target,
                initializer: ArenaExprOrRun::Expr(initializer),
                ..
            } = initializer_stmt.kind
            else {
                continue;
            };
            let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind else {
                continue;
            };
            let Some(Type::Map(_, element)) = self
                .expr_types
                .get(&self.arena.expr(initializer).span)
                .cloned()
            else {
                continue;
            };
            if !list_splice_element_type_is_precise(&element)
                || !self.is_empty_map_literal_source(initializer)
            {
                continue;
            }
            let mut entries = Vec::new();
            let mut end = initializer_stmt.span.end();
            for &next in &statements[index + 1..] {
                let stmt = self.arena.stmt(next);
                let ArenaStmtKind::Assign {
                    target,
                    op: AssignOp::Set,
                    value: ArenaExprOrRun::Expr(value),
                } = stmt.kind
                else {
                    break;
                };
                if !matches!(self.arena.assign_target(target).kind, ArenaAssignTargetKind::Name(target) if target == name)
                {
                    break;
                }
                let Some((base, key, value)) = self.map_set_parts(value, &element) else {
                    break;
                };
                if !matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(base) if base == name)
                    || expr_references_name(self.arena, key, name)
                    || expr_references_name(self.arena, value, name)
                    || expr_may_have_effects(self.arena, key)
                    || expr_may_have_effects(self.arena, value)
                {
                    break;
                }
                entries.push((key, value));
                end = stmt.span.end();
            }
            if entries.is_empty() {
                continue;
            }
            let Some(literal) = self.map_literal_replacement(&entries) else {
                continue;
            };
            let span = Span::new(
                initializer_stmt.span.source_id,
                initializer_stmt.span.start(),
                end,
            );
            let prefix = &self.source
                [initializer_stmt.span.start()..self.arena.expr(initializer).span.start()];
            let original = &self.source[span.range()];
            let trailing_layout = &original[original.trim_end().len()..];
            self.map_literal_diagnostic(span, format!("{prefix}{literal}{trailing_layout}"));
        }
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
        if is_fs_call(self.arena, callee, "mkdir") || is_method_call(self.arena, callee, "mkdir") {
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

    fn lint_negative_if_as_boolean_guard(&mut self, statement: StmtId) {
        let stmt = self.arena.stmt(statement);
        let ArenaStmtKind::If {
            branches,
            else_block: None,
        } = stmt.kind
        else {
            return;
        };
        let [branch] = self.arena.if_branches(branches) else {
            return;
        };
        let block_span = self.arena.span(self.arena.block(branch.block).span);
        if !self.definitely_exiting_block_spans.contains(&block_span) {
            return;
        }
        let condition = self.arena.expr(branch.condition);
        if self.source[stmt.span.start()..block_span.start()].contains('#') {
            return;
        }
        let source_expr = |expr: ExprId| {
            let span = self.arena.expr(expr).span;
            self.source[span.start()..span.end()].trim()
        };
        let inverse = match condition.kind {
            ArenaExprKind::Unary {
                op: UnaryOp::Not,
                expr,
            } => source_expr(expr).to_string(),
            ArenaExprKind::Binary { op, left, right }
                if matches!(op, BinaryOp::Lt | BinaryOp::Le | BinaryOp::Ne)
                    || (op == BinaryOp::Eq
                        && matches!(self.arena.expr(right).kind, ArenaExprKind::Null)) =>
            {
                let integer_order = matches!(
                    self.expr_types.get(&self.arena.expr(left).span),
                    Some(Type::Int)
                ) && matches!(
                    self.expr_types.get(&self.arena.expr(right).span),
                    Some(Type::Int)
                );
                let operator = match op {
                    BinaryOp::Lt if integer_order => Some(">="),
                    BinaryOp::Le if integer_order => Some(">"),
                    BinaryOp::Ne => Some("=="),
                    BinaryOp::Eq => Some("!="),
                    _ => None,
                };
                if let Some(operator) = operator {
                    let between =
                        self.arena.expr(left).span.end()..self.arena.expr(right).span.start();
                    let original = match op {
                        BinaryOp::Lt => "<",
                        BinaryOp::Le => "<=",
                        BinaryOp::Ne => "!=",
                        BinaryOp::Eq => "==",
                        _ => unreachable!(),
                    };
                    let Some(offset) = self.source[between.clone()].find(original) else {
                        return;
                    };
                    let token_start = between.start + offset;
                    format!(
                        "{}{}{}",
                        &self.source[condition.span.start()..token_start],
                        operator,
                        &self.source[token_start + original.len()..condition.span.end()]
                    )
                } else {
                    format!("! ({})", source_expr(branch.condition))
                }
            }
            _ => return,
        };
        let replacement = format!(
            "guard {inverse} else {}",
            &self.source[block_span.start()..block_span.end()]
        );
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "leading failure branch can use a Boolean guard",
            )
            .with_code(DiagnosticCode::LintBooleanGuard)
            .with_label(Label::secondary(
                stmt.span,
                "continue only when the condition succeeds",
            ))
            .with_fix_hint(FixHint::replacement(
                stmt.span,
                "use explicit guard failure branch",
                replacement,
            )),
        );
    }

    /// A match arm whose braces hold one postfix-guarded statement is printed
    /// without them, `0 => return x when c`. When the guard fix replaces the
    /// only statement of such a block, it replaces the braces too.
    fn widen_guard_fix_to_arm_block(&mut self, block: BlockId) {
        let block = self.arena.block(block);
        let mut statements = self.arena.stmt_ids(block.statements);
        let (Some(only), None) = (statements.next(), statements.next()) else {
            return;
        };
        if !block.params.is_empty() {
            return;
        }
        let stmt_span = self.arena.stmt(only).span;
        let block_span = self.arena.span(block.span);
        let braces_only = self
            .source
            .get(block_span.start()..stmt_span.start())
            .zip(self.source.get(stmt_span.end()..block_span.end()))
            .is_some_and(|(open, close)| open.trim() == "{" && close.trim() == "}");
        if !braces_only {
            return;
        }
        for hint in self
            .diagnostics
            .iter_mut()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferGuard))
            .flat_map(|diagnostic| diagnostic.fix_hints.iter_mut())
            .filter(|hint| hint.span == Some(stmt_span))
        {
            hint.span = Some(block_span);
        }
    }

    fn lint_if_as_guard(&mut self, branches: ArenaRange, else_block: Option<BlockId>, span: Span) {
        if self.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::LintBooleanGuard)
                && diagnostic
                    .fix_hints
                    .iter()
                    .any(|fix| fix.span == Some(span))
        }) {
            return;
        }
        // Only single-branch if with no else
        if branches.len() != 1 || else_block.is_some() {
            return;
        }
        let branch = self.arena.if_branches(branches)[0].clone();
        if matches!(
            self.arena.expr(branch.condition).kind,
            ArenaExprKind::PatternCondition { .. }
        ) {
            return;
        }
        // Only single-statement body
        let branch_stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(branch.block).statements)
            .collect();
        let [only_stmt] = branch_stmts.as_slice() else {
            return;
        };
        let keyword = match self.arena.stmt(*only_stmt).kind {
            ArenaStmtKind::Break { .. } => "break",
            ArenaStmtKind::Continue => "continue",
            ArenaStmtKind::Return(_) => "return",
            ArenaStmtKind::YieldDelegate(_) => "yield",
            ArenaStmtKind::Yield(_) => "yield",
            _ => return,
        };
        // Preserve grouping that expression spans can omit around pipelines and runs.
        let Some(condition) = self
            .source
            .get(span.start() + 2..self.arena.span(self.arena.block(branch.block).span).start())
            .map(str::trim)
        else {
            return;
        };
        let (guard_word, condition) = if matches!(
            self.arena.expr(branch.condition).kind,
            ArenaExprKind::Unary {
                op: UnaryOp::Not,
                ..
            }
        ) && condition.starts_with('!')
        {
            ("unless", condition[1..].trim())
        } else {
            ("when", condition)
        };
        let Some(action) = self.source.get(self.arena.stmt(*only_stmt).span.range()) else {
            return;
        };
        let action = action.trim().trim_end_matches(';').trim_end();
        let payload = action
            .strip_prefix(keyword)
            .unwrap_or_default()
            .trim_start();
        let action = if payload.starts_with("run ") || payload.starts_with("run.") {
            format!("{keyword} ({payload})")
        } else {
            action.to_string()
        };
        let replacement = format!("{action} {guard_word} {condition}");
        // Measure the formatter's spelling so redundant grouping never decides
        // whether the guard is reported; a condition whose line breaks the
        // formatter keeps also keeps its block.
        let canonical = super::format::Formatter::new().format_source(span.source_id, &replacement);
        let replacement = canonical.formatted.trim_end();
        // Keep blocks whose condition or payload needs a readable multiline layout.
        if !canonical.diagnostics.is_empty()
            || replacement.contains('\n')
            || replacement.chars().count() > 88
        {
            return;
        }
        let diagnostic = Diagnostic::new(
            Severity::Warning,
            format!("use `{keyword} {guard_word}` instead of a single-action `if`"),
        )
        .with_code(DiagnosticCode::LintPreferGuard)
        .with_label(Label::secondary(span, "replace with postfix guard"));
        // Replacing the whole branch would discard comments attached to its body.
        self.diagnostics
            .push(if span_may_contain_comment(self.source, span) {
                diagnostic.with_note("comments inside the branch need a manual rewrite")
            } else {
                diagnostic.with_fix_hint(FixHint::replacement(
                    span,
                    format!("use `{keyword} {guard_word}`"),
                    replacement,
                ))
            });
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
        let search_paths = lint_env_path_list::env_path_lists(
            self.arena,
            self.source,
            &self.expr_types,
            expr,
        );
        self.diagnostics.extend(search_paths);
        let fs_is_shadowed = self.scopes.iter().any(|scope| scope.contains_key("fs"));
        let kind = lint_path_kind::path_kind_comparison(
            self.arena,
            self.source,
            &self.expr_types,
            fs_is_shadowed,
            expr,
        );
        self.diagnostics.extend(kind);
        let method = lint_fs_method::fs_function_with_a_path_method(
            self.arena,
            self.source,
            &self.expr_types,
            fs_is_shadowed,
            expr,
        );
        self.diagnostics.extend(method);
        if self.explicit_missing_ok {
            let removal = lint_explicit_missing_ok::implicit_missing_ok(
                self.arena,
                self.source,
                &self.expr_types,
                fs_is_shadowed,
                expr,
            );
            self.diagnostics.extend(removal);
        }
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

    fn define(&mut self, name: &str, span: Span, report_unused: bool) {
        if self.is_defined_in_outer_scope(name) && !is_predeclared_script_args(name) && name != "_"
        {
            self.diagnostics.push(
                Diagnostic::error("binding shadows an outer name")
                    .with_code(DiagnosticCode::LintShadowing)
                    .with_label(Label::secondary(span, "shadowed binding starts here")),
            );
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(
                name.to_string(),
                Binding {
                    mutable: false,
                    span,
                    used: false,
                    comparison_stable: false,
                    absence_lookup: false,
                    materialized_record: false,
                    immutable_byte_length: None,
                    report_unused: report_unused && name != "_",
                },
            );
        }
    }

    fn mark_used(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(name) {
                binding.used = true;
                break;
            }
        }
    }

    fn is_defined_in_outer_scope(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .skip(1)
            .any(|scope| scope.contains_key(name))
    }

    fn push_scope(&mut self) {
        self.scopes.push(FxHashMap::default());
    }

    fn pop_scope(&mut self) {
        let Some(scope) = self.scopes.pop() else {
            return;
        };
        let mut unused: Vec<_> = scope
            .into_iter()
            .filter(|(_, binding)| binding.report_unused && !binding.used)
            .collect();
        insertion_sort_by(&mut unused, |(_, left), (_, right)| {
            left.span.start().cmp(&right.span.start())
        });
        for (name, binding) in unused {
            self.warning(
                binding.span,
                format!("unused local variable `{name}`"),
                DiagnosticCode::LintUnusedLocal,
                "binding is never read",
            );
        }
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

    fn expr_or_run_span(&self, value: &ArenaExprOrRun) -> Span {
        match value {
            ArenaExprOrRun::Expr(expr) => self.arena.expr(*expr).span,
            ArenaExprOrRun::Run(run) => self.arena.span(self.arena.run_form(*run).span),
        }
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

fn return_list_type_expr(arena: &AstArena, ty: TypeExprId) -> bool {
    match type_expr_kind(arena, ty) {
        ArenaTypeExprKind::List(_) => true,
        ArenaTypeExprKind::Result { ok, .. } => {
            matches!(type_expr_kind(arena, ok), ArenaTypeExprKind::List(_))
        }
        _ => false,
    }
}

#[derive(Clone, Debug)]
struct StreamProducerCandidate {
    function_name: xsh::frontend::symbols::Name,
    accumulator_name: xsh::frontend::symbols::Name,
    span: Span,
}

fn collect_stream_producer_candidates(
    arena: &AstArena,
    stmts: &[StmtId],
    out: &mut Vec<StreamProducerCandidate>,
) {
    for &stmt_id in stmts {
        let inner = match arena.stmt(stmt_id).kind {
            ArenaStmtKind::Export(inner) => arena.stmt(inner).kind,
            other => other,
        };
        if let ArenaStmtKind::ProcDef(def_id) = inner {
            let def = arena.function_def(def_id);
            if return_list_type_expr(arena, def.return_ty)
                && let Some((accumulator_name, span)) = stream_producer_candidate(arena, def.body)
            {
                out.push(StreamProducerCandidate {
                    function_name: def.name,
                    accumulator_name,
                    span,
                });
            }
        }
    }
}

fn collect_lazy_consumed_calls(
    arena: &AstArena,
    stmts: &[StmtId],
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    for &stmt_id in stmts {
        lazy_visit_stmt(arena, stmt_id, out);
    }
}

fn lazy_visit_stmt(
    arena: &AstArena,
    stmt_id: StmtId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    match arena.stmt(stmt_id).kind {
        ArenaStmtKind::Use(_)
        | ArenaStmtKind::TypeDef(_)
        | ArenaStmtKind::ErrorDef(_)
        | ArenaStmtKind::Continue
        | ArenaStmtKind::TailBareIdent(_)
        | ArenaStmtKind::Return(None)
        // Old visitor::walk_stmt treats Break (with or without value) as a leaf.
        | ArenaStmtKind::Break { .. } => {}
        ArenaStmtKind::Export(inner) => lazy_visit_stmt(arena, inner, out),
        ArenaStmtKind::Let { initializer, .. } | ArenaStmtKind::Const { initializer, .. } | ArenaStmtKind::Var { initializer, .. } => {
            lazy_visit_expr_or_run(arena, &initializer, out);
        }
        ArenaStmtKind::Assign { target, value, .. } => {
            lazy_visit_assign_target(arena, target, out);
            lazy_visit_expr_or_run(arena, &value, out);
        }
        ArenaStmtKind::Return(Some(v)) | ArenaStmtKind::Defer(v, _) | ArenaStmtKind::Yield(v) => {
            lazy_visit_expr_or_run(arena, &v, out);
        }
        ArenaStmtKind::ProcDef(def) | ArenaStmtKind::CliMain(def) | ArenaStmtKind::PureDef(def) | ArenaStmtKind::StreamDef(def) => {
            lazy_visit_block(arena, arena.function_def(def).body, out);
        }
        ArenaStmtKind::SignalHook(hook) => {
            lazy_visit_block(arena, arena.signal_hook(hook).body, out);
        }
        ArenaStmtKind::If { branches, else_block } => {
            for branch in arena.if_branches(branches).to_vec() {
                lazy_visit_expr(arena, branch.condition, out);
                lazy_visit_block(arena, branch.block, out);
            }
            if let Some(block) = else_block {
                lazy_visit_block(arena, block, out);
            }
        }
        ArenaStmtKind::While { condition, block } => {
            lazy_visit_expr(arena, condition, out);
            lazy_visit_block(arena, block, out);
        }
        ArenaStmtKind::For { iter, block, .. } => {
            if let Some(name) = direct_call_name(arena, iter) {
                out.insert(name);
            }
            lazy_visit_expr(arena, iter, out);
            lazy_visit_block(arena, block, out);
        }
        ArenaStmtKind::Loop { block } => lazy_visit_block(arena, block, out),
        ArenaStmtKind::Sugar { operands, .. } => {
            for operand in arena.sugar_operands(operands) {
                match *operand {
                    ArenaSugarOperand::Expr(expr) => lazy_visit_expr(arena, expr, out),
                    ArenaSugarOperand::Block(block) => lazy_visit_block(arena, block, out),
                    ArenaSugarOperand::Stmt(stmt) => lazy_visit_stmt(arena, stmt, out),
                    _ => {}
                }
            }
        }
        ArenaStmtKind::Guard { initializer, else_block, .. } => {
            lazy_visit_expr_or_run(arena, &initializer, out);
            lazy_visit_block(arena, else_block, out);
        }
        ArenaStmtKind::Assert { condition, message } => {
            lazy_visit_expr(arena, condition, out);
            if let Some(message) = message { lazy_visit_expr(arena, message, out); }
        }
        ArenaStmtKind::With { bindings, body, else_block, .. } => {
            for binding in arena.with_bindings(bindings).to_vec() {
                lazy_visit_expr(arena, binding.initializer, out);
            }
            lazy_visit_block(arena, body, out);
            lazy_visit_block(arena, else_block, out);
        }
        ArenaStmtKind::Match { value, arms } => {
            lazy_visit_expr(arena, value, out);
            for arm in arena.match_arms(arms).to_vec() {
                if let Some(guard) = arm.guard {
                    lazy_visit_expr(arena, guard, out);
                }
                lazy_visit_block(arena, arm.block, out);
            }
        }
        ArenaStmtKind::Expr(expr)
        | ArenaStmtKind::YieldDelegate(expr)
        | ArenaStmtKind::Exit(expr) => {
            lazy_visit_expr(arena, expr, out);
        }
        ArenaStmtKind::Command(cmd_id) => {
            lazy_visit_command(arena, cmd_id, out);
        }
    }
}

fn lazy_visit_assign_target(
    arena: &AstArena,
    target: AssignTargetId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    match arena.assign_target(target).kind.clone() {
        ArenaAssignTargetKind::Name(_) | ArenaAssignTargetKind::Env(_) => {}
        ArenaAssignTargetKind::Field { base, .. } => lazy_visit_assign_target(arena, base, out),
        ArenaAssignTargetKind::Index { base, index } => {
            lazy_visit_assign_target(arena, base, out);
            lazy_visit_expr(arena, index, out);
        }
    }
}

fn lazy_visit_command(
    arena: &AstArena,
    cmd_id: CommandStmtId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    match arena.command_stmt(cmd_id).command.clone() {
        ArenaCommand::Proc { args, .. } => {
            for arg in arena.command_args(args).to_vec() {
                lazy_visit_command_arg(arena, &arg, out);
            }
        }
        ArenaCommand::Core {
            args, env, block, ..
        } => {
            for arg in arena.command_args(args).to_vec() {
                lazy_visit_command_arg(arena, &arg, out);
            }
            for assignment in arena.env_assignments(env).to_vec() {
                match assignment.value {
                    ArenaEnvAssignmentValue::CommandArg(arg) => {
                        lazy_visit_command_arg(arena, &arg, out);
                    }
                    ArenaEnvAssignmentValue::Expr(e) => lazy_visit_expr(arena, e, out),
                }
            }
            if let Some(block) = block {
                lazy_visit_block(arena, block, out);
            }
        }
        ArenaCommand::Run(run) => lazy_visit_run(arena, run, out),
    }
}

fn lazy_visit_run(
    arena: &AstArena,
    run: RunFormId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    let segments = arena.run_form(run).segments;
    for segment in arena.run_segments(segments).to_vec() {
        if let Some(timeout) = segment.timeout {
            lazy_visit_expr(arena, timeout, out);
        }
        if let Some(cpu_max) = segment.cpu_max {
            lazy_visit_expr(arena, cpu_max, out);
        }
        if let Some(accept) = segment.accept {
            lazy_visit_expr(arena, accept, out);
        }
        for assignment in arena.env_assignments(segment.env).to_vec() {
            match assignment.value {
                ArenaEnvAssignmentValue::CommandArg(arg) => {
                    lazy_visit_command_arg(arena, &arg, out);
                }
                ArenaEnvAssignmentValue::Expr(e) => lazy_visit_expr(arena, e, out),
            }
        }
        lazy_visit_command_arg(arena, &segment.target, out);
        for arg in arena.command_args(segment.args).to_vec() {
            lazy_visit_command_arg(arena, &arg, out);
        }
        for redirection in arena.redirections(segment.redirections).to_vec() {
            match redirection.target {
                ArenaRedirectionTarget::Path(arg) | ArenaRedirectionTarget::Fd(arg) => {
                    lazy_visit_command_arg(arena, &arg, out);
                }
            }
        }
    }
}

fn lazy_visit_command_arg(
    arena: &AstArena,
    arg: &ArenaCommandArg,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    match arg.kind {
        ArenaCommandArgKind::Word(parts) => {
            for part in arena.word_parts(parts).collect::<Vec<_>>() {
                if let ArenaWordPart::Interpolation(e) | ArenaWordPart::Shorthand(e) = part {
                    lazy_visit_expr(arena, e, out);
                }
            }
        }
        ArenaCommandArgKind::SpliceExpr(e) | ArenaCommandArgKind::Typed(e) => {
            lazy_visit_expr(arena, e, out);
        }
        ArenaCommandArgKind::SpliceName(_) => {}
    }
}

fn lazy_visit_block(
    arena: &AstArena,
    block: BlockId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    for stmt in arena
        .stmt_ids(arena.block(block).statements)
        .collect::<Vec<_>>()
    {
        lazy_visit_stmt(arena, stmt, out);
    }
}

fn lazy_visit_expr_or_run(
    arena: &AstArena,
    value: &ArenaExprOrRun,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    if let ArenaExprOrRun::Expr(expr) = value {
        lazy_visit_expr(arena, *expr, out);
    }
}

fn lazy_visit_expr(
    arena: &AstArena,
    expr: ExprId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    match arena.expr(expr).kind {
        ArenaExprKind::StructuredPipeline { input, .. } => {
            if let Some(name) = direct_call_name(arena, input) {
                out.insert(name);
            }
        }
        ArenaExprKind::Run(run) => lazy_visit_run(arena, run, out),
        ArenaExprKind::Spawn(form) => {
            if let ArenaSpawnTarget::Run(run) = form.target {
                lazy_visit_run(arena, run, out);
            }
        }
        ArenaExprKind::BuilderCall { block, .. } => {
            lazy_visit_builder_block(arena, block, out);
        }
        _ => {}
    }
    for child in expr_child_exprs(arena, expr) {
        lazy_visit_expr(arena, child, out);
    }
    for block in expr_child_blocks(arena, expr) {
        lazy_visit_block(arena, block, out);
    }
}

fn lazy_visit_builder_block(
    arena: &AstArena,
    block: BuilderBlockId,
    out: &mut FxHashSet<xsh::frontend::symbols::Name>,
) {
    for entry in arena
        .builder_entries(arena.builder_block(block).entries)
        .to_vec()
    {
        match entry.kind {
            ArenaBuilderEntryKind::Field { value, .. } => lazy_visit_expr(arena, value, out),
            ArenaBuilderEntryKind::Entry { args, block, .. } => {
                for arg in arena.command_args(args).to_vec() {
                    lazy_visit_command_arg(arena, &arg, out);
                }
                if let Some(block) = block {
                    lazy_visit_builder_block(arena, block, out);
                }
            }
            ArenaBuilderEntryKind::Task { block, .. } => lazy_visit_block(arena, block, out),
            ArenaBuilderEntryKind::Stmt(stmt) => lazy_visit_stmt(arena, stmt, out),
        }
    }
}

/// Enumerate the immediate child expressions of an expression for structural
/// traversal (mirrors the old `visitor::walk_expr` descent).
fn expr_child_exprs(arena: &AstArena, expr: ExprId) -> Vec<ExprId> {
    let mut out = Vec::new();
    match arena.expr(expr).kind {
        ArenaExprKind::ValuePipelineCall { input, call, .. } => {
            out.push(input);
            out.push(call);
        }

        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            for part in arena.fmt_parts(parts).collect::<Vec<_>>() {
                if let ArenaFmtPart::Expr(e, _) = part {
                    out.push(e);
                }
            }
        }
        ArenaExprKind::List(items) => out.extend(arena.list_element_exprs(items)),
        ArenaExprKind::ListComp { expr, qualifiers } => {
            out.extend(arena.comp_qualifiers(qualifiers).iter().map(|q| q.expr()));
            out.push(expr);
        }
        ArenaExprKind::MapComp {
            key,
            value,
            qualifiers,
        } => {
            out.extend(arena.comp_qualifiers(qualifiers).iter().map(|q| q.expr()));
            out.push(key);
            out.push(value);
        }
        ArenaExprKind::Record(fields) => {
            for field in arena.record_fields(fields) {
                match field.kind {
                    ArenaRecordFieldKind::Computed { key, value, .. } => {
                        out.push(key);
                        out.push(value);
                    }
                    ArenaRecordFieldKind::Named { value, .. }
                    | ArenaRecordFieldKind::Path { value, .. } => out.push(value),
                    ArenaRecordFieldKind::Spread { expr, .. } => out.push(expr),
                    ArenaRecordFieldKind::Shorthand { .. } => {}
                }
            }
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            for branch in arena.if_expr_branches(branches) {
                out.push(branch.condition);
                out.push(branch.value);
            }
            out.push(else_value);
        }
        ArenaExprKind::Match { value, arms }
        | ArenaExprKind::PatternTest { value, arms }
        | ArenaExprKind::PatternCondition { value, arms } => {
            out.push(value);
            for arm in arena.match_expr_arms(arms) {
                out.extend(arm.guard);
                out.push(arm.value);
            }
        }
        ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => out.push(expr),
        ArenaExprKind::ComparisonChain(pairs) => out.extend(arena.comparison_chain_operands(pairs)),
        ArenaExprKind::Binary { left, right, .. } => {
            out.push(left);
            out.push(right);
        }
        ArenaExprKind::Call { callee, args } => {
            out.push(callee);
            for arg in arena.call_args(args) {
                match arg.kind {
                    ArenaCallArgKind::Positional(e)
                    | ArenaCallArgKind::Named { value: e, .. }
                    | ArenaCallArgKind::Splice { value: e, .. }
                    | ArenaCallArgKind::NamedSpread { value: e, .. } => out.push(e),
                }
            }
        }
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
            out.push(base);
        }
        ArenaExprKind::Index { base, index, .. } => {
            out.push(base);
            out.push(index);
        }
        ArenaExprKind::Slice {
            base, start, end, ..
        } => {
            out.push(base);
            out.extend(start);
            out.extend(end);
        }
        ArenaExprKind::Pipeline { input, stages } => {
            out.push(input);
            for stage in arena.pipe_stages(stages).to_vec() {
                match stage.kind {
                    ArenaPipeStageKind::Expr(e) => out.push(e),
                    ArenaPipeStageKind::Stream(stage) => {
                        for arg in arena.call_args(stage.args) {
                            match arg.kind {
                                ArenaCallArgKind::Positional(e)
                                | ArenaCallArgKind::Named { value: e, .. }
                                | ArenaCallArgKind::Splice { value: e, .. }
                                | ArenaCallArgKind::NamedSpread { value: e, .. } => out.push(e),
                            }
                        }
                    }
                }
            }
        }
        ArenaExprKind::StructuredPipeline { input, stages } => {
            out.push(input);
            for stage in arena.stream_stages(stages).to_vec() {
                for arg in arena.call_args(stage.args) {
                    match arg.kind {
                        ArenaCallArgKind::Positional(e)
                        | ArenaCallArgKind::Named { value: e, .. }
                        | ArenaCallArgKind::Splice { value: e, .. }
                        | ArenaCallArgKind::NamedSpread { value: e, .. } => out.push(e),
                    }
                }
            }
        }
        ArenaExprKind::Spawn(form) => {
            if let ArenaSpawnTarget::Command(e) = form.target {
                out.push(e);
            }
        }
        ArenaExprKind::Wait(form) => out.push(form.target),
        ArenaExprKind::BuilderCall { call, .. } => out.push(call),
        ArenaExprKind::Require { value, .. } => out.push(value),
        ArenaExprKind::ErrorContext { message, .. }
        | ArenaExprKind::ContextScope { input: message, .. } => out.push(message),
        ArenaExprKind::Retry { delays, .. } => out.extend(arena.expr_ids(delays)),
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Regex(_)
        | ArenaExprKind::Ident(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::EnvString(_)
        | ArenaExprKind::EnvPathList
        | ArenaExprKind::Run(_)
        | ArenaExprKind::Capture(_)
        | ArenaExprKind::ValueBlock(_)
        | ArenaExprKind::Loop { .. }
        | ArenaExprKind::TempDirScope { .. } => {}
    }
    out
}

/// Enumerate the immediate child statement-blocks of an expression.
fn expr_child_blocks(arena: &AstArena, expr: ExprId) -> Vec<BlockId> {
    let mut out = Vec::new();
    match arena.expr(expr).kind {
        ArenaExprKind::Capture(block)
        | ArenaExprKind::ValueBlock(block)
        | ArenaExprKind::Loop { block }
        | ArenaExprKind::Retry { block, .. }
        | ArenaExprKind::ErrorContext { block, .. }
        | ArenaExprKind::ContextScope { block, .. }
        | ArenaExprKind::TempDirScope { block, .. } => out.push(block),
        ArenaExprKind::Pipeline { stages, .. } => {
            for stage in arena.pipe_stages(stages).to_vec() {
                if let ArenaPipeStageKind::Stream(stage) = stage.kind
                    && let Some(block) = stage.block
                {
                    out.push(block);
                }
            }
        }
        ArenaExprKind::StructuredPipeline { stages, .. } => {
            for stage in arena.stream_stages(stages).to_vec() {
                if let Some(block) = stage.block {
                    out.push(block);
                }
            }
        }
        _ => {}
    }
    out
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

// Unsigned schema checks retain the nonnegative constraint even when the
// selected storage has only an Int runtime tag.
fn type_has_unsigned_constraint(ty: &Type) -> bool {
    match ty {
        Type::UInt => true,
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) => {
            type_has_unsigned_constraint(inner)
        }
        Type::Map(key, value) | Type::Result(key, value) => {
            type_has_unsigned_constraint(key) || type_has_unsigned_constraint(value)
        }
        Type::Record(fields) => fields.values().any(type_has_unsigned_constraint),
        _ => false,
    }
}

fn type_has_contextual_collection_domain(ty: &Type) -> bool {
    match ty {
        Type::UInt | Type::Optional(_) | Type::Record(_) => true,
        Type::List(inner) | Type::Stream(inner) => type_has_contextual_collection_domain(inner),
        Type::Map(key, value) | Type::Result(key, value) => {
            type_has_contextual_collection_domain(key)
                || type_has_contextual_collection_domain(value)
        }
        _ => false,
    }
}

fn type_mentions_path(ty: &Type) -> bool {
    match ty {
        Type::Path => true,
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) => {
            type_mentions_path(inner)
        }
        Type::Map(key, value) | Type::Result(key, value) => {
            type_mentions_path(key) || type_mentions_path(value)
        }
        _ => false,
    }
}

fn expr_is_dynamic_require_boundary(arena: &AstArena, expr: ExprId) -> bool {
    let expr = match arena.expr(expr).kind {
        ArenaExprKind::Try(inner) => inner,
        _ => expr,
    };
    let ArenaExprKind::Call { callee, .. } = arena.expr(expr).kind else {
        return false;
    };
    is_module_call(arena, callee, "module", "load")
        || is_module_call(arena, callee, "json", "decode")
        || is_module_call(arena, callee, "json", "read")
}

fn stream_producer_candidate(
    arena: &AstArena,
    body: BlockId,
) -> Option<(xsh::frontend::symbols::Name, Span)> {
    let stmts: Vec<StmtId> = arena.stmt_ids(arena.block(body).statements).collect();
    let &final_stmt = stmts.last()?;
    stmts.iter().enumerate().find_map(|(index, &stmt)| {
        let (name, span) = empty_list_var(arena, stmt)?;
        let rest = &stmts[index + 1..];
        if !rest.iter().any(|&stmt| stmt_pushes_to(arena, stmt, name)) {
            return None;
        }
        if rest
            .iter()
            .any(|&stmt| stmt_assigns_non_push_to(arena, stmt, name))
        {
            return None;
        }
        if !stmt_returns_value_from(arena, final_stmt, name) {
            return None;
        }
        Some((name, span))
    })
}

fn empty_list_var(arena: &AstArena, stmt: StmtId) -> Option<(xsh::frontend::symbols::Name, Span)> {
    let arena_stmt = arena.stmt(stmt);
    let ArenaStmtKind::Var {
        target,
        initializer: ArenaExprOrRun::Expr(init),
        ..
    } = arena_stmt.kind
    else {
        return None;
    };
    let ArenaBindingTargetKind::Name(name) = arena.binding_target(target).kind else {
        return None;
    };
    let ArenaExprKind::List(items) = arena.expr(init).kind else {
        return None;
    };
    if items.is_empty() {
        Some((name, arena_stmt.span))
    } else {
        None
    }
}

fn stmt_pushes_to(arena: &AstArena, stmt: StmtId, name: xsh::frontend::symbols::Name) -> bool {
    if stmt_is_push_assignment(arena, stmt, name) {
        return true;
    }
    match arena.stmt(stmt).kind {
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            arena
                .if_branches(branches)
                .iter()
                .any(|branch| block_pushes_to(arena, branch.block, name))
                || else_block.is_some_and(|block| block_pushes_to(arena, block, name))
        }
        ArenaStmtKind::While { block, .. }
        | ArenaStmtKind::For { block, .. }
        | ArenaStmtKind::Loop { block } => block_pushes_to(arena, block, name),
        ArenaStmtKind::Sugar { operands, .. } => arena
            .sugar_operands(operands)
            .iter()
            .any(|operand| match *operand {
                ArenaSugarOperand::Block(block) => block_pushes_to(arena, block, name),
                ArenaSugarOperand::Stmt(stmt) => stmt_pushes_to(arena, stmt, name),
                _ => false,
            }),
        ArenaStmtKind::Guard { else_block, .. } => block_pushes_to(arena, else_block, name),
        ArenaStmtKind::Export(stmt) => stmt_pushes_to(arena, stmt, name),
        ArenaStmtKind::With {
            body, else_block, ..
        } => block_pushes_to(arena, body, name) || block_pushes_to(arena, else_block, name),
        ArenaStmtKind::Match { arms, .. } => arena
            .match_arms(arms)
            .iter()
            .any(|arm| block_pushes_to(arena, arm.block, name)),
        _ => false,
    }
}

fn block_pushes_to(arena: &AstArena, block: BlockId, name: xsh::frontend::symbols::Name) -> bool {
    arena
        .stmt_ids(arena.block(block).statements)
        .any(|stmt| stmt_pushes_to(arena, stmt, name))
}

fn stmt_assigns_non_push_to(
    arena: &AstArena,
    stmt: StmtId,
    name: xsh::frontend::symbols::Name,
) -> bool {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Assign { target, .. }
            if assign_target_root_name(arena, target) == Some(name) =>
        {
            !stmt_is_push_assignment(arena, stmt, name)
        }
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            arena
                .if_branches(branches)
                .iter()
                .any(|branch| block_assigns_non_push_to(arena, branch.block, name))
                || else_block.is_some_and(|block| block_assigns_non_push_to(arena, block, name))
        }
        ArenaStmtKind::While { block, .. }
        | ArenaStmtKind::For { block, .. }
        | ArenaStmtKind::Loop { block } => block_assigns_non_push_to(arena, block, name),
        ArenaStmtKind::Sugar { operands, .. } => arena
            .sugar_operands(operands)
            .iter()
            .any(|operand| match *operand {
                ArenaSugarOperand::Block(block) => block_assigns_non_push_to(arena, block, name),
                ArenaSugarOperand::Stmt(stmt) => stmt_assigns_non_push_to(arena, stmt, name),
                _ => false,
            }),
        ArenaStmtKind::Guard { else_block, .. } => {
            block_assigns_non_push_to(arena, else_block, name)
        }
        ArenaStmtKind::Export(stmt) => stmt_assigns_non_push_to(arena, stmt, name),
        ArenaStmtKind::With {
            body, else_block, ..
        } => {
            block_assigns_non_push_to(arena, body, name)
                || block_assigns_non_push_to(arena, else_block, name)
        }
        ArenaStmtKind::Match { arms, .. } => arena
            .match_arms(arms)
            .iter()
            .any(|arm| block_assigns_non_push_to(arena, arm.block, name)),
        _ => false,
    }
}

/// The binding an assignment writes through; an environment variable target
/// has none.
fn assign_target_root_name(
    arena: &AstArena,
    target: AssignTargetId,
) -> Option<xsh::frontend::symbols::Name> {
    match arena.assign_target(target).kind.clone() {
        ArenaAssignTargetKind::Name(name) => Some(name),
        ArenaAssignTargetKind::Env(_) => None,
        ArenaAssignTargetKind::Field { base, .. } | ArenaAssignTargetKind::Index { base, .. } => {
            assign_target_root_name(arena, base)
        }
    }
}

fn block_assigns_non_push_to(
    arena: &AstArena,
    block: BlockId,
    name: xsh::frontend::symbols::Name,
) -> bool {
    arena
        .stmt_ids(arena.block(block).statements)
        .any(|stmt| stmt_assigns_non_push_to(arena, stmt, name))
}

fn stmt_is_push_assignment(
    arena: &AstArena,
    stmt: StmtId,
    name: xsh::frontend::symbols::Name,
) -> bool {
    let ArenaStmtKind::Assign {
        target,
        op: AssignOp::Set,
        value: ArenaExprOrRun::Expr(rhs),
    } = arena.stmt(stmt).kind
    else {
        return false;
    };
    let ArenaAssignTargetKind::Name(assign_name) = arena.assign_target(target).kind else {
        return false;
    };
    if assign_name != name {
        return false;
    }
    let ArenaExprKind::Call { callee, args } = arena.expr(rhs).kind else {
        return false;
    };
    let ArenaExprKind::Field {
        base,
        name: method_name,
    } = arena.expr(callee).kind
    else {
        return false;
    };
    if method_name != "push" || args.len() != 1 {
        return false;
    }
    matches!(arena.expr(base).kind, ArenaExprKind::Ident(base_name) if base_name == name)
}

fn stmt_returns_value_from(
    arena: &AstArena,
    stmt: StmtId,
    name: xsh::frontend::symbols::Name,
) -> bool {
    match arena.stmt(stmt).kind {
        ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr))) => {
            expr_is_ident_or_pipeline_from(arena, expr, name)
        }
        ArenaStmtKind::Expr(expr) => expr_is_ident_or_pipeline_from(arena, expr, name),
        _ => false,
    }
}

fn expr_is_ident_or_pipeline_from(
    arena: &AstArena,
    expr: ExprId,
    name: xsh::frontend::symbols::Name,
) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(ident) => ident == name,
        ArenaExprKind::StructuredPipeline { input, .. } | ArenaExprKind::Pipeline { input, .. } => {
            matches!(arena.expr(input).kind, ArenaExprKind::Ident(ident) if ident == name)
        }
        _ => false,
    }
}

fn return_value_is_ok_unit(arena: &AstArena, value: &ArenaExprOrRun) -> bool {
    let ArenaExprOrRun::Expr(expr) = value else {
        return false;
    };
    let ArenaExprKind::Call { callee, args } = arena.expr(*expr).kind else {
        return false;
    };
    matches!(arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Ok") && args.is_empty()
}

fn ok_call_arg(arena: &AstArena, expr: ExprId) -> Option<ExprId> {
    let ArenaExprKind::Call { callee, args } = arena.expr(expr).kind else {
        return None;
    };
    let [arg] = arena.call_args(args) else {
        return None;
    };
    let ArenaCallArgKind::Positional(arg) = arg.kind else {
        return None;
    };
    matches!(arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Ok").then_some(arg)
}

fn expr_references_name(arena: &AstArena, expr: ExprId, name: Name) -> bool {
    let refs = |id: ExprId| expr_references_name(arena, id, name);
    match arena.expr(expr).kind {
        ArenaExprKind::ValuePipelineCall { input, call, .. } => refs(input) || refs(call),

        ArenaExprKind::Ident(candidate) => candidate == name,
        ArenaExprKind::List(items) => arena.list_element_exprs(items).any(refs),
        ArenaExprKind::ListComp { expr, qualifiers } => {
            refs(expr)
                || arena
                    .comp_qualifiers(qualifiers)
                    .iter()
                    .any(|q| refs(q.expr()))
        }
        ArenaExprKind::MapComp {
            key,
            value,
            qualifiers,
        } => {
            refs(key)
                || refs(value)
                || arena
                    .comp_qualifiers(qualifiers)
                    .iter()
                    .any(|q| refs(q.expr()))
        }
        ArenaExprKind::Record(fields) => {
            arena
                .record_fields(fields)
                .iter()
                .any(|field| match field.kind {
                    ArenaRecordFieldKind::Computed { key, value, .. } => refs(key) || refs(value),
                    ArenaRecordFieldKind::Named { value, .. }
                    | ArenaRecordFieldKind::Path { value, .. } => refs(value),
                    ArenaRecordFieldKind::Spread { expr, .. } => refs(expr),
                    ArenaRecordFieldKind::Shorthand { name: field, .. } => field == name,
                })
        }
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            arena.fmt_parts(parts).any(|part| match part {
                ArenaFmtPart::Expr(expr, _) => refs(expr),
                ArenaFmtPart::Text(_) => false,
            })
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            arena
                .if_expr_branches(branches)
                .iter()
                .any(|branch| refs(branch.condition) || refs(branch.value))
                || refs(else_value)
        }
        ArenaExprKind::Match { value, arms }
        | ArenaExprKind::PatternTest { value, arms }
        | ArenaExprKind::PatternCondition { value, arms } => {
            refs(value)
                || arena
                    .match_expr_arms(arms)
                    .iter()
                    .any(|arm| arm.guard.is_some_and(refs) || refs(arm.value))
        }
        ArenaExprKind::Unary { expr, .. }
        | ArenaExprKind::Try(expr)
        | ArenaExprKind::Require { value: expr, .. } => refs(expr),
        ArenaExprKind::ComparisonChain(pairs) => arena.comparison_chain_operands(pairs).any(refs),
        ArenaExprKind::Binary { left, right, .. } => refs(left) || refs(right),
        ArenaExprKind::Call { callee, args } => {
            refs(callee)
                || arena.call_args(args).iter().any(|arg| match arg.kind {
                    ArenaCallArgKind::Positional(expr)
                    | ArenaCallArgKind::Named { value: expr, .. }
                    | ArenaCallArgKind::Splice { value: expr, .. }
                    | ArenaCallArgKind::NamedSpread { value: expr, .. } => refs(expr),
                })
        }
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => refs(base),
        ArenaExprKind::Index { base, index, .. } => refs(base) || refs(index),
        ArenaExprKind::Slice {
            base, start, end, ..
        } => refs(base) || start.is_some_and(refs) || end.is_some_and(refs),
        ArenaExprKind::Pipeline { input, stages } => {
            refs(input)
                || arena
                    .pipe_stages(stages)
                    .to_vec()
                    .iter()
                    .any(|stage| match stage.kind {
                        ArenaPipeStageKind::Expr(expr) => refs(expr),
                        ArenaPipeStageKind::Stream(ref stage) => {
                            stream_stage_references_name(arena, stage, name)
                        }
                    })
        }
        ArenaExprKind::StructuredPipeline { input, stages } => {
            refs(input)
                || arena
                    .stream_stages(stages)
                    .to_vec()
                    .iter()
                    .any(|stage| stream_stage_references_name(arena, stage, name))
        }
        ArenaExprKind::Run(run) => arena
            .run_segments(arena.run_form(run).segments)
            .to_vec()
            .iter()
            .any(|segment| {
                arena
                    .command_args(segment.args)
                    .iter()
                    .any(|arg| command_arg_references_name(arena, arg, name))
                    || command_arg_references_name(arena, &segment.target, name)
            }),
        ArenaExprKind::Spawn(form) => match form.target {
            ArenaSpawnTarget::Run(run) => arena
                .run_segments(arena.run_form(run).segments)
                .to_vec()
                .iter()
                .any(|segment| {
                    command_arg_references_name(arena, &segment.target, name)
                        || arena
                            .command_args(segment.args)
                            .iter()
                            .any(|arg| command_arg_references_name(arena, arg, name))
                }),
            ArenaSpawnTarget::Command(expr) => refs(expr),
        },
        ArenaExprKind::Wait(form) => refs(form.target),
        ArenaExprKind::BuilderCall { call, block } => {
            refs(call)
                || arena
                    .builder_entries(arena.builder_block(block).entries)
                    .to_vec()
                    .iter()
                    .any(|entry| match entry.kind {
                        ArenaBuilderEntryKind::Field { value, .. } => refs(value),
                        ArenaBuilderEntryKind::Entry { args, block, .. } => {
                            arena
                                .command_args(args)
                                .iter()
                                .any(|arg| command_arg_references_name(arena, arg, name))
                                || block.is_some_and(|block| {
                                    arena
                                        .builder_entries(arena.builder_block(block).entries)
                                        .to_vec()
                                        .iter()
                                        .any(|entry| match entry.kind {
                                            ArenaBuilderEntryKind::Field { value, .. } => {
                                                refs(value)
                                            }
                                            _ => false,
                                        })
                                })
                        }
                        ArenaBuilderEntryKind::Task { .. } | ArenaBuilderEntryKind::Stmt(_) => {
                            false
                        }
                    })
        }
        ArenaExprKind::ValueBlock(_)
        | ArenaExprKind::ErrorContext { .. }
        | ArenaExprKind::ContextScope { .. }
        | ArenaExprKind::TempDirScope { .. } => true,
        ArenaExprKind::Capture(_) | ArenaExprKind::Loop { .. } | ArenaExprKind::Retry { .. } => {
            false
        }
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Regex(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::EnvString(_)
        | ArenaExprKind::EnvPathList => false,
    }
}

fn stream_stage_references_name(arena: &AstArena, stage: &ArenaStreamStage, name: Name) -> bool {
    arena
        .call_args(stage.args)
        .iter()
        .any(|arg| match arg.kind {
            ArenaCallArgKind::Positional(expr)
            | ArenaCallArgKind::Named { value: expr, .. }
            | ArenaCallArgKind::Splice { value: expr, .. }
            | ArenaCallArgKind::NamedSpread { value: expr, .. } => {
                expr_references_name(arena, expr, name)
            }
        })
        || stage.block.is_some_and(|block| {
            arena
                .stmt_ids(arena.block(block).statements)
                .any(|stmt| match arena.stmt(stmt).kind {
                    ArenaStmtKind::Expr(expr) => expr_references_name(arena, expr, name),
                    ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr))) => {
                        expr_references_name(arena, expr, name)
                    }
                    _ => false,
                })
        })
}

fn command_arg_references_name(arena: &AstArena, arg: &ArenaCommandArg, name: Name) -> bool {
    match arg.kind {
        ArenaCommandArgKind::SpliceName(candidate) => candidate == name,
        ArenaCommandArgKind::SpliceExpr(expr) | ArenaCommandArgKind::Typed(expr) => {
            expr_references_name(arena, expr, name)
        }
        ArenaCommandArgKind::Word(parts) => arena.word_parts(parts).any(|part| match part {
            ArenaWordPart::Interpolation(expr) | ArenaWordPart::Shorthand(expr) => {
                expr_references_name(arena, expr, name)
            }
            ArenaWordPart::Bare(_) | ArenaWordPart::Quoted(_) => false,
        }),
    }
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

struct LintExprVisitor<'a, 'b> {
    linter: &'a mut Linter<'b>,
    suppress_expr_autofixes: bool,
}

impl LintExprVisitor<'_, '_> {
    fn visit_comp_qualifiers(&mut self, range: ArenaRange) -> usize {
        let mut scopes = 0;
        for qualifier in self.linter.arena.comp_qualifiers(range).to_vec() {
            if let ArenaCompQualifier::For { iter, .. } = qualifier {
                self.linter.lint_scalar_split_iteration(iter);
            }
            self.visit_expr(qualifier.expr());
            if let ArenaCompQualifier::For { target, .. } = qualifier {
                self.linter.push_scope();
                scopes += 1;
                self.linter
                    .define_binding_target(target, qualifier.span(), false);
            }
        }
        scopes
    }

    fn visit_expr(&mut self, expr: ExprId) {
        self.linter.propagation_boundary_depth += 1;
        self.visit_expression(expr);
        self.linter.propagation_boundary_depth -= 1;
    }

    fn visit_expression(&mut self, expr: ExprId) {
        self.linter
            .fail_candidates
            .visit_expr(self.linter.arena, expr);
        if let Some(diagnostic) = lint_redundant_propagation::redundant_condition_propagation(
            self.linter.arena,
            self.linter.source,
            &self.linter.redundant_condition_propagations,
            expr,
        ) {
            self.linter.diagnostics.push(diagnostic);
        }
        if let ArenaExprKind::Unary {
            op: UnaryOp::Not,
            expr: inner,
        } = self.linter.arena.expr(expr).kind
            && matches!(
                self.linter.arena.expr(inner).kind,
                ArenaExprKind::Call { .. }
            )
        {
            self.linter.negated_call_spans.insert(
                self.linter.arena.expr(inner).span,
                self.linter.arena.expr(expr).span,
            );
        }

        if let Some(diagnostic) =
            self.linter
                .size_products
                .visit(self.linter.arena, self.linter.source, expr)
        {
            self.linter.diagnostics.push(diagnostic);
        }
        if let Some(diagnostic) = lint_run_argv::run_argv_diagnostic(
            self.linter.arena,
            self.linter.source,
            &self.linter.standard_call_spans,
            expr,
        ) {
            self.linter.diagnostics.push(diagnostic);
        }
        if !self.suppress_expr_autofixes {
            self.linter.lint_proven_nonnull_fallback(expr);
        }
        if !self.suppress_expr_autofixes {
            self.linter.lint_nested_record_update(expr);
            self.linter.lint_nested_value_pipeline(expr);

            self.linter.lint_block_string_concatenation(expr);
            self.linter.lint_list_splicing(expr);
            self.linter.lint_map_literal_chain(expr);
            self.linter.lint_prepared_regex(expr);
            self.linter.lint_env_string(expr);
            self.linter.lint_comparison_chain(expr);
            self.linter.lint_lookup_sentinel(expr);

            self.linter.lint_optional_postfix(expr);
            if let ArenaExprKind::Match { arms, .. } = self.linter.arena.expr(expr).kind {
                self.linter.lint_adjacent_pattern_arms(
                    self.linter
                        .arena
                        .match_expr_arms(arms)
                        .iter()
                        .map(|arm| {
                            (
                                arm.pattern,
                                arm.guard,
                                Err(arm.value),
                                self.linter.arena.span(arm.span),
                            )
                        })
                        .collect(),
                );
            }
            self.linter.lint_boolean_match(expr);
            self.linter.lint_pattern_conditional_expr(expr);
            self.linter.lint_error_fallback_block(expr);
            self.linter.lint_path_roundtrip(expr);
            self.linter.lint_redundant_require(expr);
            self.linter.lint_known_field_access(expr);
            self.linter.lint_inferred_require_target(expr);
            self.linter.lint_redundant_single_interpolation(expr);
            self.linter.lint_scalar_display_parse_roundtrip(expr);
            self.linter.lint_json_encode_decode_roundtrip(expr);
            self.linter.lint_inferred_variant(expr);
            self.linter.lint_positional_constructor(expr);
            self.linter.lint_path_migrations(expr);
        }
        let arena_expr = self.linter.arena.expr(expr);
        match arena_expr.kind {
            ArenaExprKind::Ident(name) => {
                self.linter.mark_used(name.as_str().as_str());
                if self.linter.prefer_typed_callables {
                    let hidden = self.linter.local_hides(name);
                    self.linter.callable_parameters.value(expr, name, hidden);
                }
            }
            ArenaExprKind::Call { callee, args } => {
                if self.linter.prefer_typed_callables {
                    let mut parameters = std::mem::take(&mut self.linter.callable_parameters);
                    parameters.call(self.linter.arena, callee, args, &|name| {
                        self.linter.local_hides(name)
                    });
                    self.linter.callable_parameters = parameters;
                }
                if self
                    .linter
                    .record_constructors
                    .resolve_call(self.linter.arena, callee, None)
                    .is_some()
                    && let ArenaExprKind::Ident(name) = self.linter.arena.expr(callee).kind
                {
                    self.linter
                        .used_type_names
                        .insert(name.as_str().to_string());
                }
                self.walk_expr(expr);
                self.linter.lint_call_style(callee, args, arena_expr.span);
            }
            _ => self.walk_expr(expr),
        }
    }

    fn walk_expr(&mut self, expr: ExprId) {
        let arena = self.linter.arena;
        match arena.expr(expr).kind {
            ArenaExprKind::ValuePipelineCall { input, call, .. } => {
                self.visit_expr(input);
                self.visit_expr(call);
            }

            ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
                for part in arena.fmt_parts(parts).collect::<Vec<_>>() {
                    if let ArenaFmtPart::Expr(e, spec) = part {
                        if spec.is_none() {
                            self.linter
                                .lint_redundant_path_display(e, PathDisplaySite::Interpolation);
                        }
                        self.visit_expr(e);
                    }
                }
            }
            ArenaExprKind::List(items) => {
                for item in arena.list_element_exprs(items).collect::<Vec<_>>() {
                    self.visit_expr(item);
                }
            }
            ArenaExprKind::ListComp { expr, qualifiers } => {
                let scopes = self.visit_comp_qualifiers(qualifiers);
                self.visit_expr(expr);
                for _ in 0..scopes {
                    self.linter.pop_scope();
                }
            }
            ArenaExprKind::MapComp {
                key,
                value,
                qualifiers,
            } => {
                let scopes = self.visit_comp_qualifiers(qualifiers);
                self.visit_expr(key);
                self.visit_expr(value);
                for _ in 0..scopes {
                    self.linter.pop_scope();
                }
            }
            ArenaExprKind::Record(fields) => {
                if !self.suppress_expr_autofixes {
                    self.linter.lint_quoted_field_labels(fields);
                }
                for field in arena.record_fields(fields).to_vec() {
                    self.visit_record_field(&field);
                }
            }
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                for branch in arena.if_expr_branches(branches).to_vec() {
                    if let ArenaExprKind::PatternCondition { value, arms } =
                        arena.expr(branch.condition).kind
                    {
                        self.visit_expr(value);
                        self.linter.push_scope();
                        self.linter
                            .lint_pattern(arena.match_expr_arms(arms)[0].pattern);
                        self.visit_expr(branch.value);
                        self.linter.pop_scope();
                    } else {
                        self.visit_expr(branch.condition);
                        self.visit_expr(branch.value);
                    }
                }
                self.visit_expr(else_value);
            }
            ArenaExprKind::PatternCondition { value, .. } => self.visit_expr(value),
            ArenaExprKind::Match { value, arms } | ArenaExprKind::PatternTest { value, arms } => {
                let old = self.linter.regex_recovery_context;
                self.linter.regex_recovery_context = true;
                self.visit_expr(value);
                for arm in arena.match_expr_arms(arms).to_vec() {
                    self.visit_match_expr_arm(&arm);
                }
                self.linter.regex_recovery_context = old;
            }
            ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => self.visit_expr(expr),
            ArenaExprKind::ComparisonChain(pairs) => {
                for operand in arena.comparison_chain_operands(pairs).collect::<Vec<_>>() {
                    self.visit_expr(operand);
                }
            }
            ArenaExprKind::Binary { op, left, right } => {
                let old = self.linter.regex_recovery_context;
                self.linter.regex_recovery_context |= op == BinaryOp::ResultFallback;
                self.visit_expr(left);
                self.visit_expr(right);
                self.linter.regex_recovery_context = old;
            }
            ArenaExprKind::Call { callee, args } => {
                self.visit_expr(callee);
                for arg in arena.call_args(args).to_vec() {
                    self.visit_call_arg(&arg);
                }
            }
            ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
                self.visit_expr(base)
            }
            ArenaExprKind::Index { base, index, .. } => {
                self.visit_expr(base);
                self.visit_expr(index);
            }
            ArenaExprKind::Slice {
                base, start, end, ..
            } => {
                self.visit_expr(base);
                if let Some(start) = start {
                    self.visit_expr(start);
                }
                if let Some(end) = end {
                    self.visit_expr(end);
                }
            }
            ArenaExprKind::Pipeline { input, stages } => {
                self.visit_expr(input);
                for stage in arena.pipe_stages(stages).to_vec() {
                    self.visit_pipe_stage(&stage);
                }
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                self.visit_expr(input);
                for stage in arena.stream_stages(stages).to_vec() {
                    self.visit_stream_stage(&stage);
                }
                // fs.walk(X) |> where .kind == "file" → fs.files(X)
                self.linter.lint_prefer_fs_files(input, stages);
                self.linter.lint_redundant_stream_stages(stages);
            }
            ArenaExprKind::Run(run) => self.visit_run_form(run),
            ArenaExprKind::Spawn(form) => match form.target {
                ArenaSpawnTarget::Run(run) => self.visit_run_form(run),
                ArenaSpawnTarget::Command(expr) => self.visit_expr(expr),
            },
            ArenaExprKind::Wait(form) => self.visit_expr(form.target),
            ArenaExprKind::BuilderCall { call, block } => {
                self.visit_expr(call);
                self.visit_builder_block(block);
            }
            ArenaExprKind::Require { value, schema } => {
                self.visit_expr(value);
                if let Some(schema) = schema {
                    self.linter.collect_type_expr_refs(schema);
                }
            }
            ArenaExprKind::ErrorContext { message, block }
            | ArenaExprKind::ContextScope {
                input: message,
                block,
                ..
            } => {
                self.visit_expr(message);
                self.linter.lint_block(block);
            }
            ArenaExprKind::Capture(block) | ArenaExprKind::ValueBlock(block) => {
                let capturing = matches!(arena.expr(expr).kind, ArenaExprKind::Capture(_));
                if capturing {
                    self.linter.assertion_capture_depth += 1;
                }
                self.linter.push_scope();
                for parameter in arena.block_params(arena.block(block).params) {
                    self.linter.define(
                        parameter.name.as_str().as_str(),
                        arena.span(parameter.span),
                        true,
                    );
                }
                self.linter.lint_block_statements(block);
                self.linter.pop_scope();
                if capturing {
                    self.linter.assertion_capture_depth -= 1;
                }
            }
            ArenaExprKind::Loop { block } => self.linter.lint_block(block),
            // The directory name is the block's parameter.
            ArenaExprKind::TempDirScope { block, .. } => self.linter.lint_stream_block(block),
            ArenaExprKind::Retry {
                delays,
                pattern,
                block,
            } => {
                if let Some(pattern) = pattern {
                    self.linter.lint_pattern(pattern);
                }
                let old = self.linter.regex_recovery_context;
                self.linter.regex_recovery_context = true;
                for delay in arena.expr_ids(delays).collect::<Vec<_>>() {
                    self.visit_expr(delay);
                }
                self.linter.assertion_capture_depth += 1;
                self.linter.lint_block(block);
                self.linter.assertion_capture_depth -= 1;
                self.linter.regex_recovery_context = old;
            }
            ArenaExprKind::Str(_) => {
                self.linter.lint_dollar_in_expression_string(expr);
                self.linter.lint_missing_f_prefix(expr);
                if !self.suppress_expr_autofixes {
                    self.linter.lint_redundant_newline_triple_string(expr);
                }
            }
            ArenaExprKind::PathStr(_) => self.linter.lint_missing_f_prefix(expr),
            ArenaExprKind::Null
            | ArenaExprKind::Bool(_)
            | ArenaExprKind::Int(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::GlobStr(_)
            | ArenaExprKind::Bytes(_)
            | ArenaExprKind::Regex(_)
            | ArenaExprKind::Ident(_)
            | ArenaExprKind::EnvString(_)
            | ArenaExprKind::EnvPathList
            | ArenaExprKind::Item
            | ArenaExprKind::LastStatus => {}
        }
    }

    fn visit_record_field(&mut self, field: &ArenaRecordField) {
        match field.kind {
            ArenaRecordFieldKind::Shorthand { name, .. } => {
                self.linter.mark_used(name.as_str().as_str())
            }
            ArenaRecordFieldKind::Computed { key, value, .. } => {
                self.visit_expr(key);
                self.visit_expr(value);
            }
            ArenaRecordFieldKind::Named { value, .. }
            | ArenaRecordFieldKind::Path { value, .. } => self.visit_expr(value),
            ArenaRecordFieldKind::Spread { expr, .. } => self.visit_expr(expr),
        }
    }

    fn visit_match_expr_arm(&mut self, arm: &ArenaMatchExprArm) {
        self.linter.push_scope();
        self.linter.lint_pattern(arm.pattern);
        if let Some(guard) = arm.guard {
            self.visit_expr(guard);
        }
        self.visit_expr(arm.value);
        self.linter.pop_scope();
    }

    fn visit_call_arg(&mut self, arg: &ArenaCallArg) {
        if !self.suppress_expr_autofixes {
            self.linter.lint_named_argument_pun(arg);
        }
        match arg.kind {
            ArenaCallArgKind::Positional(expr) | ArenaCallArgKind::Named { value: expr, .. } => {
                self.visit_expr(expr);
            }
            ArenaCallArgKind::Splice { value, .. }
            | ArenaCallArgKind::NamedSpread { value, .. } => self.visit_expr(value),
        }
    }

    fn visit_pipe_stage(&mut self, stage: &ArenaPipeStage) {
        match stage.kind {
            ArenaPipeStageKind::Expr(expr) => self.visit_expr(expr),
            ArenaPipeStageKind::Stream(ref stage) => self.visit_stream_stage(stage),
        }
    }

    fn visit_command_arg(&mut self, arg: &ArenaCommandArg, allow_bare_refs: bool) {
        let arg_span = self.linter.arena.span(arg.span);
        match arg.kind {
            ArenaCommandArgKind::SpliceName(name) => self.linter.mark_used(name.as_str().as_str()),
            ArenaCommandArgKind::Word(parts) => {
                self.linter.lint_redundant_command_arg_interpolation(arg);
                let part_list: Vec<ArenaWordPart> = self.linter.arena.word_parts(parts).collect();
                if allow_bare_refs
                    && let Some(text) =
                        bare_command_word_parts(self.linter.arena, self.linter.source, &part_list)
                    && let Some((root, _)) = parse_command_word_reference(&text)
                {
                    self.linter.mark_used(root);
                }
                // ${f"..."} is a Word with a single Interpolation whose expr is an FmtString.
                // Since f"..." is now accepted directly as a typed command arg, the wrapper is
                // redundant.
                if let [ArenaWordPart::Interpolation(expr)] = part_list.as_slice()
                    && matches!(
                        self.linter.arena.expr(*expr).kind,
                        ArenaExprKind::FmtString(_)
                    )
                    && self.linter.source.as_bytes().get(arg_span.start()) == Some(&b'$')
                {
                    let expr_span = self.linter.arena.expr(*expr).span;
                    let replacement =
                        self.linter.source[expr_span.start()..expr_span.end()].to_string();
                    self.linter.diagnostics.push(
                        Diagnostic::new(Severity::Warning, "redundant `${}` around f-string")
                            .with_code(DiagnosticCode::LintRedundantFmtWrapper)
                            .with_span(arg_span)
                            .with_fix_hint(FixHint::replacement(
                                arg_span,
                                "remove the `${}` wrapper",
                                replacement,
                            )),
                    );
                }
                for part in &part_list {
                    if let ArenaWordPart::Interpolation(expr) | ArenaWordPart::Shorthand(expr) =
                        *part
                    {
                        self.linter
                            .lint_redundant_path_display(expr, PathDisplaySite::Interpolation);
                        if matches!(part, ArenaWordPart::Interpolation(_)) {
                            self.visit_command_delimited_expr(expr);
                        } else {
                            self.visit_command_embedded_expr(expr);
                        }
                    }
                }
            }
            ArenaCommandArgKind::SpliceExpr(expr) => {
                self.linter
                    .lint_redundant_path_display(expr, PathDisplaySite::Splice);
                self.visit_command_delimited_expr(expr);
            }
            ArenaCommandArgKind::Typed(expr) => {
                let expr_span = self.linter.arena.expr(expr).span;
                self.linter
                    .lint_redundant_path_display(expr, PathDisplaySite::CommandWord(arg_span));
                // (f"...") → f"..." : f-strings don't need paren wrapping
                if matches!(
                    self.linter.arena.expr(expr).kind,
                    ArenaExprKind::FmtString(_) | ArenaExprKind::PathFmtString(_)
                ) && self.linter.source.as_bytes().get(arg_span.start()) == Some(&b'(')
                {
                    let replacement =
                        self.linter.source[expr_span.start()..expr_span.end()].to_string();
                    self.linter.diagnostics.push(
                        Diagnostic::new(Severity::Warning, "redundant `()` around f-string")
                            .with_code(DiagnosticCode::LintRedundantFmtWrapper)
                            .with_span(arg_span)
                            .with_fix_hint(FixHint::replacement(
                                arg_span,
                                "remove the `()` wrapper",
                                replacement,
                            )),
                    );
                } else if let Some(replacement) = self.linter.command_single_fmt_replacement(expr) {
                    self.linter.diagnostics.push(
                        Diagnostic::new(
                            Severity::Warning,
                            "redundant single-value command f-string",
                        )
                        .with_code(DiagnosticCode::LintRedundantCommandFmt)
                        .with_label(Label::secondary(
                            expr_span,
                            "use command value syntax directly",
                        ))
                        .with_fix_hint(FixHint::replacement(
                            arg_span,
                            "use command value syntax",
                            replacement,
                        )),
                    );
                } else if simple_command_value_expr(self.linter.arena, expr) {
                    let replacement = command_value_replacement(self.linter.arena, expr);
                    self.linter.diagnostics.push(
                        Diagnostic::new(
                            Severity::Warning,
                            "redundant parentheses around a command value",
                        )
                        .with_code(DiagnosticCode::LintCommandValue)
                        .with_label(Label::secondary(arg_span, "a name or field path takes `$`"))
                        .with_fix_hint(FixHint::replacement(
                            arg_span,
                            "write it with `$`",
                            replacement,
                        )),
                    );
                }
                self.visit_command_delimited_expr(expr);
            }
        }
    }

    /// An expression a command argument holds between its own delimiters:
    /// `(EXPR)`, `${EXPR}`, or `@(EXPR)`. Inside them any expression can
    /// replace it, so it is linted like an expression anywhere else. One
    /// written without delimiters (a bare `f"..."` argument, a `$name.field`
    /// shorthand) is a command word as much as an expression, and replacing
    /// it could turn it into a literal word, so its rewrites stay off.
    fn visit_command_delimited_expr(&mut self, expr: ExprId) {
        let start = self.linter.arena.expr(expr).span.start();
        let delimited = start > 0
            && matches!(
                self.linter.source.as_bytes().get(start - 1),
                Some(b'(' | b'{')
            );
        if !delimited {
            return self.visit_command_embedded_expr(expr);
        }
        let old = self.suppress_expr_autofixes;
        self.suppress_expr_autofixes = false;
        self.visit_expr(expr);
        self.suppress_expr_autofixes = old;
    }

    fn visit_command_embedded_expr(&mut self, expr: ExprId) {
        let old = self.suppress_expr_autofixes;
        self.suppress_expr_autofixes = true;
        self.visit_expr(expr);
        self.suppress_expr_autofixes = old;
    }

    fn visit_run_form(&mut self, run: RunFormId) {
        let arena = self.linter.arena;
        if let Some(diagnostic) = lint_explicit_run_capture::explicit_run_capture(
            arena,
            &self.linter.implicitly_captured_runs,
            run,
        ) {
            self.linter.diagnostics.push(diagnostic);
        }
        let run_form = arena.run_form(run).clone();
        for segment in arena.run_segments(run_form.segments).to_vec() {
            let seg_span = arena.span(segment.span);
            if segment.kind == RunKind::Plain
                && segment.accept.is_none()
                && run_form.propagate
                && let Some(target) =
                    literal_command_word(arena, self.linter.source, &segment.target)
                && expects_nonzero_status(&target)
            {
                let deletion_span =
                    scan_run_propagate_deletion_span(self.linter.source, arena.span(run_form.span));
                self.linter.diagnostics.push(
                    Diagnostic::new(
                        Severity::Warning,
                        "remove `?` when inspecting an expected nonzero status",
                    )
                    .with_code(DiagnosticCode::LintRunStatus)
                    .with_label(Label::secondary(
                        seg_span,
                        "nonzero status is expected for this command",
                    ))
                    .with_fix_hint(FixHint::deletion(
                        deletion_span,
                        "remove status propagation",
                    )),
                );
            }
        }
        for segment in arena.run_segments(run_form.segments).to_vec() {
            if let Some(timeout) = segment.timeout {
                self.visit_expr(timeout);
            }
            if let Some(cpu_max) = segment.cpu_max {
                self.visit_expr(cpu_max);
            }
            if let Some(accept) = segment.accept {
                self.visit_expr(accept);
            }
            for assignment in arena.env_assignments(segment.env).to_vec() {
                self.visit_env_assignment(&assignment);
            }
            self.visit_command_arg(&segment.target, false);
            for arg in arena.command_args(segment.args).to_vec() {
                self.visit_command_arg(&arg, false);
            }
            for redirection in arena.redirections(segment.redirections).to_vec() {
                self.visit_redirection(&redirection);
            }
        }
    }

    fn visit_env_assignment(&mut self, assignment: &ArenaEnvAssignment) {
        match assignment.value {
            ArenaEnvAssignmentValue::CommandArg(ref arg) => self.visit_command_arg(arg, true),
            ArenaEnvAssignmentValue::Expr(expr) => self.visit_expr(expr),
        }
    }

    fn visit_redirection(&mut self, redirection: &ArenaRedirection) {
        match redirection.target {
            ArenaRedirectionTarget::Path(ref arg) | ArenaRedirectionTarget::Fd(ref arg) => {
                self.visit_command_arg(arg, false);
            }
        }
    }

    fn visit_stream_stage(&mut self, stage: &ArenaStreamStage) {
        self.linter.lint_stream_stage(stage);
    }

    fn visit_builder_block(&mut self, block: BuilderBlockId) {
        self.linter.lint_builder_block(block);
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

/// The span of a declaration's `[effects]` clause: the bracket that directly
/// follows the closing parenthesis of the parameter list. Scanning tokens
/// keeps comments and bracketed parameter types or defaults from being
/// mistaken for the clause.
fn scan_effect_list_span(
    arena: &AstArena,
    def: &ArenaFunctionDef,
    stmt_span: Span,
    source: &str,
) -> Option<Span> {
    use xsh::frontend::syntax::token::TokenTag;
    let base = stmt_span.start();
    let signature = source.get(base..arena.span(arena.block(def.body).span).start())?;
    let tokens = xsh::frontend::syntax::lexer::Lexer::new(stmt_span.source_id, signature)
        .lex_compact()
        .token_table;
    let mut depth = 0usize;
    let mut parameters_closed = false;
    let mut open = None;
    for index in 0..tokens.len() {
        let tag = tokens.tag_at(index)?;
        if let Some(open) = open {
            match tag {
                TokenTag::RBracket => {
                    return Some(Span::new(
                        stmt_span.source_id,
                        base + open,
                        base + tokens.end_at(index, signature)?,
                    ));
                }
                _ => continue,
            }
        }
        if matches!(tag, TokenTag::Comment | TokenTag::Newline) {
            continue;
        }
        if parameters_closed {
            if tag != TokenTag::LBracket {
                return None;
            }
            open = Some(tokens.start_at(index)?);
            continue;
        }
        match tag {
            TokenTag::LParen => depth += 1,
            TokenTag::RParen => {
                depth = depth.checked_sub(1)?;
                parameters_closed = depth == 0;
            }
            _ => {}
        }
    }
    None
}

/// Scan backward from `ty_start` past whitespace and the `->` arrow to find the
/// start of the ` -> TypeName` annotation so it can be deleted in one span.
fn scan_before_arrow(source: &str, ty_start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = ty_start;
    while i > 0 && bytes[i - 1] == b' ' {
        i -= 1;
    }
    if i >= 2 && bytes[i - 2] == b'-' && bytes[i - 1] == b'>' {
        i -= 2;
        while i > 0 && bytes[i - 1] == b' ' {
            i -= 1;
        }
    }
    i
}

fn shift_after_deletion(offset: usize, deletion: Span) -> usize {
    if offset >= deletion.end() {
        offset - deletion.range().len()
    } else {
        offset
    }
}

fn scan_before_colon(source: &str, ty_start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = ty_start;
    while i > 0 && bytes[i - 1] == b' ' {
        i -= 1;
    }
    if i > 0 && bytes[i - 1] == b':' {
        i -= 1;
        while i > 0 && bytes[i - 1] == b' ' {
            i -= 1;
        }
    }
    i
}

fn scan_after_type(_source: &str, ty_end: usize) -> usize {
    ty_end
}

fn scan_run_propagate_deletion_span(source: &str, run_span: Span) -> Span {
    let bytes = source.as_bytes();
    let mut end = run_span.end();
    while end < bytes.len() && matches!(bytes[end], b' ' | b'\t') {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'?' {
        end += 1;
        Span::new(run_span.source_id, run_span.end(), end)
    } else {
        Span::new(run_span.source_id, run_span.end(), run_span.end())
    }
}

fn span_end_after_following_newlines(source: &str, mut end: usize) -> usize {
    let bytes = source.as_bytes();
    while end < bytes.len() && matches!(bytes[end], b'\r' | b'\n') {
        end += 1;
    }
    end
}

/// Compute a deletion span for a bare `return` statement covering the full source
/// line: leading indentation, the keyword, and the trailing newline.
fn scan_return_stmt_span(source: &str, stmt_span: Span) -> Span {
    let bytes = source.as_bytes();
    let mut start = stmt_span.start();
    while start > 0 && matches!(bytes[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    let mut end = stmt_span.end();
    while end < bytes.len() && bytes[end] == b' ' {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'\r' {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'\n' {
        end += 1;
    }
    Span::new(stmt_span.source_id, start, end)
}

/// Scan backward from `pos` over any spaces, to include the separator whitespace
/// before a token in its deletion span.
fn scan_back_space(source: &str, pos: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = pos;
    while i > 0 && bytes[i - 1] == b' ' {
        i -= 1;
    }
    i
}

fn scan_pipe_stage_deletion_span(source: &str, stage_span: Span) -> Span {
    let bytes = source.as_bytes();
    let mut start = stage_span.start();
    while start > 0 && matches!(bytes[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    if start >= 2 && bytes[start - 2] == b'|' && bytes[start - 1] == b'>' {
        start -= 2;
        while start > 0 && matches!(bytes[start - 1], b' ' | b'\t') {
            start -= 1;
        }
    }
    Span::new(stage_span.source_id, start, stage_span.end())
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

fn is_safe_const_expr(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::ValuePipelineCall { .. } => false,

        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Regex(_) => true,
        ArenaExprKind::List(items) => arena
            .list_element_exprs(items)
            .all(|item| is_safe_const_expr(arena, item)),
        ArenaExprKind::Record(fields) => {
            arena
                .record_fields(fields)
                .iter()
                .all(|field| match field.kind {
                    ArenaRecordFieldKind::Computed { .. } => false,
                    ArenaRecordFieldKind::Named { value, .. }
                    | ArenaRecordFieldKind::Path { value, .. } => is_safe_const_expr(arena, value),
                    ArenaRecordFieldKind::Shorthand { .. }
                    | ArenaRecordFieldKind::Spread { .. } => false,
                })
        }
        ArenaExprKind::Unary { expr, .. } => is_safe_const_expr(arena, expr),
        ArenaExprKind::ComparisonChain(pairs) => arena
            .comparison_chain_operands(pairs)
            .all(|operand| is_safe_const_expr(arena, operand)),
        ArenaExprKind::Binary { left, right, .. } => {
            is_safe_const_expr(arena, left) && is_safe_const_expr(arena, right)
        }
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => arena
            .fmt_parts(parts)
            .all(|part| matches!(part, ArenaFmtPart::Text(_))),
        ArenaExprKind::Ident(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::ListComp { .. }
        | ArenaExprKind::MapComp { .. }
        | ArenaExprKind::If { .. }
        | ArenaExprKind::Match { .. }
        | ArenaExprKind::PatternTest { .. }
        | ArenaExprKind::PatternCondition { .. }
        | ArenaExprKind::Call { .. }
        | ArenaExprKind::Field { .. }
        | ArenaExprKind::NullSafeField { .. }
        | ArenaExprKind::Index { .. }
        | ArenaExprKind::Slice { .. }
        | ArenaExprKind::EnvString(_)
        | ArenaExprKind::EnvPathList
        | ArenaExprKind::Pipeline { .. }
        | ArenaExprKind::StructuredPipeline { .. }
        | ArenaExprKind::Run(_)
        | ArenaExprKind::Spawn(_)
        | ArenaExprKind::Wait(_)
        | ArenaExprKind::BuilderCall { .. }
        | ArenaExprKind::Try(_)
        | ArenaExprKind::Require { .. }
        | ArenaExprKind::Capture(_)
        | ArenaExprKind::ValueBlock(_)
        | ArenaExprKind::ErrorContext { .. }
        | ArenaExprKind::ContextScope { .. }
        | ArenaExprKind::TempDirScope { .. }
        | ArenaExprKind::Loop { .. }
        | ArenaExprKind::Retry { .. } => false,
    }
}

fn expr_may_have_effects(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::ValuePipelineCall { input, call, .. } => expr_may_have_effects(arena, input) || expr_may_have_effects(arena, call),

        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_) | ArenaExprKind::Regex(_)
        | ArenaExprKind::Ident(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::EnvPathList => false,
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => arena
            .fmt_parts(parts)
            .any(|part| matches!(part, ArenaFmtPart::Expr(expr, _) if expr_may_have_effects(arena, expr))),
        ArenaExprKind::List(items) => arena.list_element_exprs(items).any(|item| expr_may_have_effects(arena, item)),
        ArenaExprKind::Record(fields) => arena.record_fields(fields).iter().any(|field| match field.kind {
            ArenaRecordFieldKind::Computed { key, value, .. } => expr_may_have_effects(arena, key) || expr_may_have_effects(arena, value),
            ArenaRecordFieldKind::Named { value, .. } | ArenaRecordFieldKind::Path { value, .. } => expr_may_have_effects(arena, value),
            ArenaRecordFieldKind::Spread { expr, .. } => expr_may_have_effects(arena, expr),
            ArenaRecordFieldKind::Shorthand { .. } => false,
        }),
        ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => {
            expr_may_have_effects(arena, expr)
        }
        ArenaExprKind::ComparisonChain(pairs) => arena.comparison_chain_operands(pairs).any(|operand| expr_may_have_effects(arena, operand)),
        ArenaExprKind::Binary { left, right, .. } => {
            expr_may_have_effects(arena, left) || expr_may_have_effects(arena, right)
        }
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
            expr_may_have_effects(arena, base)
        }
        ArenaExprKind::Index { base, index, .. } => {
            expr_may_have_effects(arena, base) || expr_may_have_effects(arena, index)
        }
        ArenaExprKind::Slice { base, start, end, .. } => {
            expr_may_have_effects(arena, base)
                || start.is_some_and(|start| expr_may_have_effects(arena, start))
                || end.is_some_and(|end| expr_may_have_effects(arena, end))
        }
        ArenaExprKind::Call { callee, args } => {
            !pure_method_call_for_prefer_in(arena, callee)
                || arena
                    .call_args(args)
                    .iter()
                    .any(|arg| call_arg_may_have_effects(arena, arg))
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            arena.if_expr_branches(branches).iter().any(|branch| {
                expr_may_have_effects(arena, branch.condition)
                    || expr_may_have_effects(arena, branch.value)
            }) || expr_may_have_effects(arena, else_value)
        }
        ArenaExprKind::Match { value, arms } | ArenaExprKind::PatternTest { value, arms } | ArenaExprKind::PatternCondition { value, arms } => {
            expr_may_have_effects(arena, value)
                || arena.match_expr_arms(arms).iter().any(|arm| {
                    arm.guard
                        .is_some_and(|guard| expr_may_have_effects(arena, guard))
                        || expr_may_have_effects(arena, arm.value)
                })
        }
        ArenaExprKind::ListComp { .. }
        | ArenaExprKind::MapComp { .. }
        | ArenaExprKind::EnvString(_)
        | ArenaExprKind::Pipeline { .. }
        | ArenaExprKind::StructuredPipeline { .. }
        | ArenaExprKind::Run(_)
        | ArenaExprKind::Spawn(_)
        | ArenaExprKind::Wait(_)
        | ArenaExprKind::BuilderCall { .. }
        | ArenaExprKind::Require { .. }
        | ArenaExprKind::Capture(_)
        | ArenaExprKind::ValueBlock(_)
        | ArenaExprKind::ErrorContext { .. }
        | ArenaExprKind::ContextScope { .. }
        | ArenaExprKind::TempDirScope { .. }
        | ArenaExprKind::Loop { .. }
        | ArenaExprKind::Retry { .. } => true,
    }
}

fn call_arg_may_have_effects(arena: &AstArena, arg: &ArenaCallArg) -> bool {
    match arg.kind {
        ArenaCallArgKind::Positional(expr)
        | ArenaCallArgKind::Named { value: expr, .. }
        | ArenaCallArgKind::Splice { value: expr, .. }
        | ArenaCallArgKind::NamedSpread { value: expr, .. } => expr_may_have_effects(arena, expr),
    }
}

fn pure_method_call_for_prefer_in(arena: &AstArena, callee: ExprId) -> bool {
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return false;
    };
    matches!(
        name.as_str().as_str(),
        "display"
            | "name"
            | "stem"
            | "ext"
            | "parent"
            | "join"
            | "len"
            | "lower"
            | "upper"
            | "trim"
            | "starts_with"
            | "ends_with"
            | "split"
            | "fields"
            | "words"
            | "replace"
    ) && !expr_may_have_effects(arena, base)
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

/// The bundle-level namespace and import table used by callable reachability.
/// Namespace zero is the entry source; every other namespace is one loaded user
/// module. This deliberately follows the loader's resolved module keys instead
/// of trying to recover paths from source spelling.
struct CallableReachabilityModule {
    statements: ArenaRange,
    imports: FxHashMap<Name, usize>,
}

#[derive(Clone, Copy)]
struct ReachableCallable {
    definition: FunctionDefId,
    namespace: usize,
    name: Name,
    span: Span,
    exported: bool,
    proc_entry: bool,
}

#[derive(Default)]
struct CallableEdges {
    direct: FxHashSet<usize>,
    dynamic: FxHashSet<usize>,
}

impl CallableEdges {
    fn all_targets(&self) -> impl Iterator<Item = usize> + '_ {
        self.direct.iter().chain(&self.dynamic).copied()
    }
}

/// A closed-world graph over the checked program bundle's callable
/// declarations. Direct calls are resolved from the AST's already-resolved
/// module imports; function identities used as values are separate dynamic
/// escape edges. The latter are only activated when their enclosing owner is
/// reachable, which avoids keeping a callee alive merely because an unused
/// callable refers to it.
struct CallableReachability<'a> {
    arena: &'a AstArena,
    modules: Vec<CallableReachabilityModule>,
    callables: Vec<ReachableCallable>,
    callable_by_name: FxHashMap<(usize, Name), usize>,
}

impl<'a> CallableReachability<'a> {
    fn new(program: &'a ArenaProgram) -> Self {
        let arena = &program.arena;
        let mut module_keys = FxHashMap::default();
        for (offset, module) in program.modules.iter().enumerate() {
            module_keys.insert(module.key.clone(), offset + 1);
        }

        let mut modules = Vec::with_capacity(program.modules.len() + 1);
        modules.push(CallableReachabilityModule {
            statements: program.statements,
            imports: FxHashMap::default(),
        });
        modules.extend(
            program
                .modules
                .iter()
                .map(|module| CallableReachabilityModule {
                    statements: module.statements,
                    imports: FxHashMap::default(),
                }),
        );

        for module in &mut modules {
            for stmt in arena.stmt_ids(module.statements) {
                let ArenaStmtKind::Use(use_id) = arena.stmt(stmt).kind else {
                    continue;
                };
                let use_stmt = arena.use_stmt(use_id);
                let Some(target) = use_stmt
                    .resolved
                    .as_deref()
                    .and_then(|key| module_keys.get(key))
                    .copied()
                else {
                    continue;
                };
                let alias = use_stmt.alias.or_else(|| arena.names(use_stmt.path).last());
                if let Some(alias) = alias {
                    module.imports.insert(alias, target);
                }
            }
        }

        let mut reachability = Self {
            arena,
            modules,
            callables: Vec::new(),
            callable_by_name: FxHashMap::default(),
        };
        reachability.collect_callable_declarations();
        reachability
    }

    fn collect_callable_declarations(&mut self) {
        let module_statements = self
            .modules
            .iter()
            .enumerate()
            .map(|(namespace, module)| {
                (
                    namespace,
                    self.arena.stmt_ids(module.statements).collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        for (namespace, statements) in module_statements {
            for stmt in statements {
                let Some((definition, exported, proc_entry)) =
                    callable_statement_info(self.arena, stmt)
                else {
                    continue;
                };
                let callable = ReachableCallable {
                    definition,
                    namespace,
                    name: self.arena.function_def(definition).name,
                    span: self.arena.stmt(stmt).span,
                    exported,
                    proc_entry,
                };
                let index = self.callables.len();
                self.callable_by_name
                    .insert((namespace, callable.name), index);
                self.callables.push(callable);
            }
        }
    }

    fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut root_targets = FxHashSet::default();
        for (index, callable) in self.callables.iter().enumerate() {
            if callable.exported
                || (callable.namespace == 0
                    && callable.proc_entry
                    && (callable.name == "main"
                        || self
                            .arena
                            .function_def(callable.definition)
                            .test_declaration))
            {
                root_targets.insert(index);
            }
        }

        for namespace in 0..self.modules.len() {
            let mut scanner = CallableEdgeScanner::new(self, namespace);
            scanner.scan_top_level_initializers();
            root_targets.extend(scanner.edges.all_targets());
            if namespace == 0 {
                scanner.scan_root_signal_hooks();
                root_targets.extend(scanner.edges.all_targets());
            }
        }

        let edges = self
            .callables
            .iter()
            .map(|callable| {
                let mut scanner = CallableEdgeScanner::new(self, callable.namespace);
                scanner.scan_callable(callable.definition);
                scanner.edges
            })
            .collect::<Vec<_>>();
        let mut reachable = vec![false; self.callables.len()];
        let mut pending = root_targets.into_iter().collect::<Vec<_>>();
        while let Some(index) = pending.pop() {
            if reachable[index] {
                continue;
            }
            reachable[index] = true;
            pending.extend(edges[index].all_targets());
        }

        let mut candidates = self
            .callables
            .iter()
            .enumerate()
            .filter(|(index, callable)| !callable.exported && !reachable[*index])
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(_, callable)| callable.span);
        candidates
            .into_iter()
            .map(|(_, callable)| {
                Diagnostic::new(
                    Severity::Warning,
                    format!("unused callable `{}`", callable.name.as_str()),
                )
                .with_code(DiagnosticCode::LintUnusedCallable)
                .with_label(Label::secondary(
                    callable.span,
                    "this unexported callable is not reachable from a bundle entry point",
                ))
            })
            .collect()
    }

    fn resolve_unqualified(&self, namespace: usize, name: Name) -> Option<usize> {
        self.callable_by_name.get(&(namespace, name)).copied()
    }

    fn resolve_qualified(&self, namespace: usize, alias: Name, name: Name) -> Option<usize> {
        let target_namespace = self.modules[namespace].imports.get(&alias)?;
        self.resolve_unqualified(*target_namespace, name)
    }
}

fn callable_statement_info(arena: &AstArena, stmt: StmtId) -> Option<(FunctionDefId, bool, bool)> {
    let mut current = stmt;
    let mut exported = false;
    loop {
        match arena.stmt(current).kind {
            ArenaStmtKind::Export(inner) => {
                exported = true;
                current = inner;
            }
            ArenaStmtKind::ProcDef(definition) => return Some((definition, exported, true)),
            ArenaStmtKind::PureDef(definition) | ArenaStmtKind::StreamDef(definition) => {
                return Some((definition, exported, false));
            }
            _ => return None,
        }
    }
}

struct CallableEdgeScanner<'analysis, 'arena> {
    analysis: &'analysis CallableReachability<'arena>,
    namespace: usize,
    scopes: Vec<FxHashSet<Name>>,
    edges: CallableEdges,
}

impl<'analysis, 'arena> CallableEdgeScanner<'analysis, 'arena> {
    fn new(analysis: &'analysis CallableReachability<'arena>, namespace: usize) -> Self {
        Self {
            analysis,
            namespace,
            scopes: vec![FxHashSet::default()],
            edges: CallableEdges::default(),
        }
    }

    fn arena(&self) -> &AstArena {
        self.analysis.arena
    }

    fn scan_top_level_initializers(&mut self) {
        let statements = self
            .arena()
            .stmt_ids(self.analysis.modules[self.namespace].statements)
            .collect::<Vec<_>>();
        self.scan_sequence(&statements);
    }

    fn scan_root_signal_hooks(&mut self) {
        let statements = self
            .arena()
            .stmt_ids(self.analysis.modules[0].statements)
            .collect::<Vec<_>>();
        for stmt in statements {
            let ArenaStmtKind::SignalHook(hook) = self.arena().stmt(stmt).kind else {
                continue;
            };
            self.scan_block(self.arena().signal_hook(hook).body);
        }
    }

    fn scan_callable(&mut self, definition: FunctionDefId) {
        let definition = self.arena().function_def(definition).clone();
        for param in self.arena().params(definition.params).to_vec() {
            if let Some(default) = param.default {
                self.scan_expr(default);
            }
            self.define(param.name);
        }
        self.scan_block(definition.body);
    }

    fn scan_sequence(&mut self, statements: &[StmtId]) {
        for &stmt in statements {
            self.scan_stmt(stmt);
        }
    }

    fn scan_stmt(&mut self, stmt: StmtId) {
        match self.arena().stmt(stmt).kind {
            ArenaStmtKind::Export(inner) => self.scan_stmt(inner),
            ArenaStmtKind::CliMain(def) => {
                self.push_scope();
                self.scan_callable(def);
                self.pop_scope();
            }
            ArenaStmtKind::Let {
                target,
                initializer,
                ..
            }
            | ArenaStmtKind::Const {
                target,
                initializer,
                ..
            }
            | ArenaStmtKind::Var {
                target,
                initializer,
                ..
            } => {
                self.scan_expr_or_run(initializer);
                self.define_binding_target(target);
            }
            ArenaStmtKind::Assign { target, value, .. } => {
                self.scan_assign_target(target);
                self.scan_expr_or_run(value);
            }
            ArenaStmtKind::Return(Some(value))
            | ArenaStmtKind::Yield(value)
            | ArenaStmtKind::Defer(value, _) => self.scan_expr_or_run(value),
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                for branch in self.arena().if_branches(branches).to_vec() {
                    self.scan_expr(branch.condition);
                    self.scan_block(branch.block);
                }
                if let Some(block) = else_block {
                    self.scan_block(block);
                }
            }
            ArenaStmtKind::While { condition, block } => {
                self.scan_expr(condition);
                self.scan_block(block);
            }
            ArenaStmtKind::For {
                target,
                iter,
                block,
                ..
            } => {
                self.scan_expr(iter);
                self.push_scope();
                self.define_binding_target(target);
                self.scan_block(block);
                self.pop_scope();
            }
            ArenaStmtKind::With {
                bindings,
                body,
                else_block,
            } => {
                let bindings = self.arena().with_bindings(bindings).to_vec();
                self.push_scope();
                for binding in &bindings {
                    self.scan_expr(binding.initializer);
                    if binding.name.as_str() != "_" {
                        self.define(binding.name);
                    }
                }
                self.scan_block(body);
                self.pop_scope();
                self.push_scope();
                self.scan_block(else_block);
                self.pop_scope();
            }
            ArenaStmtKind::Loop { block } => self.scan_block(block),
            ArenaStmtKind::Sugar { form, operands, .. } => {
                match self.arena().sugar(form, operands) {
                    // The name is in scope for the body, not for the path.
                    ArenaSugar::Tempdir { name, path, body } => {
                        self.scan_expr(path);
                        self.push_scope();
                        self.define_binding_target(name);
                        self.scan_block(body);
                        self.pop_scope();
                    }
                    // Likewise for the destination and the temporary path.
                    ArenaSugar::Atomically { dest, name, body } => {
                        self.scan_expr(dest);
                        self.push_scope();
                        self.define_binding_target(name);
                        self.scan_block(body);
                        self.pop_scope();
                    }
                    ArenaSugar::ForIndex {
                        index,
                        item,
                        source,
                        body,
                    } => {
                        self.scan_expr(source);
                        self.push_scope();
                        self.define_binding_target(index);
                        self.define_binding_target(item);
                        self.scan_block(body);
                        self.pop_scope();
                    }
                    _ => {
                        for operand in self.arena().sugar_operands(operands).to_vec() {
                            match operand {
                                ArenaSugarOperand::Expr(expr) => self.scan_expr(expr),
                                ArenaSugarOperand::Block(block) => self.scan_block(block),
                                ArenaSugarOperand::Stmt(stmt) => self.scan_stmt(stmt),
                                _ => {}
                            }
                        }
                    }
                }
            }
            ArenaStmtKind::Guard {
                target,
                initializer,
                else_block,
                ..
            } => {
                self.scan_expr_or_run(initializer);
                self.push_scope();
                self.scan_block(else_block);
                self.pop_scope();
                self.define_binding_target(target);
            }
            ArenaStmtKind::Assert { condition, message } => {
                self.scan_expr(condition);
                if let Some(message) = message {
                    self.scan_expr(message);
                }
            }
            ArenaStmtKind::Match { value, arms } => {
                self.scan_expr(value);
                for arm in self.arena().match_arms(arms).to_vec() {
                    self.push_scope();
                    self.define_pattern(arm.pattern);
                    if let Some(guard) = arm.guard {
                        self.scan_expr(guard);
                    }
                    self.scan_block(arm.block);
                    self.pop_scope();
                }
            }
            ArenaStmtKind::Command(command) => self.scan_command(command),
            ArenaStmtKind::TailBareIdent(name) => self.add_direct_unqualified(name),
            ArenaStmtKind::Expr(expr)
            | ArenaStmtKind::YieldDelegate(expr)
            | ArenaStmtKind::Exit(expr) => self.scan_expr(expr),
            // Callable bodies and hooks have their own entry conditions. A
            // declaration is never executed while its containing initializer
            // runs, so only roots and graph edges scan those bodies.
            ArenaStmtKind::Use(_)
            | ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_)
            | ArenaStmtKind::ProcDef(_)
            | ArenaStmtKind::PureDef(_)
            | ArenaStmtKind::StreamDef(_)
            | ArenaStmtKind::SignalHook(_)
            | ArenaStmtKind::Return(None)
            | ArenaStmtKind::Break { value: None }
            | ArenaStmtKind::Continue => {}
            ArenaStmtKind::Break { value: Some(value) } => self.scan_expr(value),
        }
    }

    fn scan_block(&mut self, block: BlockId) {
        let block = self.arena().block(block).clone();
        self.push_scope();
        for param in self.arena().block_params(block.params).to_vec() {
            self.define(param.name);
        }
        let statements = self.arena().stmt_ids(block.statements).collect::<Vec<_>>();
        self.scan_sequence(&statements);
        self.pop_scope();
    }

    fn scan_expr_or_run(&mut self, value: ArenaExprOrRun) {
        match value {
            ArenaExprOrRun::Expr(expr) => self.scan_expr(expr),
            ArenaExprOrRun::Run(run) => self.scan_run(run),
        }
    }

    fn scan_comp_qualifiers(&mut self, range: ArenaRange) -> usize {
        let mut scopes = 0;
        for qualifier in self.arena().comp_qualifiers(range).to_vec() {
            self.scan_expr(qualifier.expr());
            if let ArenaCompQualifier::For { target, .. } = qualifier {
                self.push_scope();
                scopes += 1;
                self.define_binding_target(target);
            }
        }
        scopes
    }

    fn scan_expr(&mut self, expr: ExprId) {
        match self.arena().expr(expr).kind {
            ArenaExprKind::ValuePipelineCall { input, call, .. } => {
                self.scan_expr(input);
                self.scan_expr(call);
            }

            ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
                for part in self.arena().fmt_parts(parts).collect::<Vec<_>>() {
                    if let ArenaFmtPart::Expr(expr, _) = part {
                        self.scan_expr(expr);
                    }
                }
            }
            ArenaExprKind::Ident(name) => self.add_dynamic_unqualified(name),
            ArenaExprKind::List(items) => {
                for item in self.arena().list_element_exprs(items).collect::<Vec<_>>() {
                    self.scan_expr(item);
                }
            }
            ArenaExprKind::ListComp { expr, qualifiers } => {
                let scopes = self.scan_comp_qualifiers(qualifiers);
                self.scan_expr(expr);
                for _ in 0..scopes {
                    self.pop_scope();
                }
            }
            ArenaExprKind::MapComp {
                key,
                value,
                qualifiers,
            } => {
                let scopes = self.scan_comp_qualifiers(qualifiers);
                self.scan_expr(key);
                self.scan_expr(value);
                for _ in 0..scopes {
                    self.pop_scope();
                }
            }
            ArenaExprKind::Record(fields) => {
                for field in self.arena().record_fields(fields).to_vec() {
                    match field.kind {
                        ArenaRecordFieldKind::Computed { key, value, .. } => {
                            self.scan_expr(key);
                            self.scan_expr(value);
                        }
                        ArenaRecordFieldKind::Named { value, .. }
                        | ArenaRecordFieldKind::Path { value, .. }
                        | ArenaRecordFieldKind::Spread { expr: value, .. } => self.scan_expr(value),
                        ArenaRecordFieldKind::Shorthand { name, .. } => {
                            self.add_dynamic_unqualified(name)
                        }
                    }
                }
            }
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                for branch in self.arena().if_expr_branches(branches).to_vec() {
                    self.scan_expr(branch.condition);
                    self.scan_expr(branch.value);
                }
                self.scan_expr(else_value);
            }
            ArenaExprKind::Match { value, arms }
            | ArenaExprKind::PatternTest { value, arms }
            | ArenaExprKind::PatternCondition { value, arms } => {
                self.scan_expr(value);
                for arm in self.arena().match_expr_arms(arms).to_vec() {
                    self.push_scope();
                    self.define_pattern(arm.pattern);
                    if let Some(guard) = arm.guard {
                        self.scan_expr(guard);
                    }
                    self.scan_expr(arm.value);
                    self.pop_scope();
                }
            }
            ArenaExprKind::Unary { expr, .. }
            | ArenaExprKind::Try(expr)
            | ArenaExprKind::Require { value: expr, .. } => self.scan_expr(expr),
            ArenaExprKind::ComparisonChain(pairs) => {
                for operand in self
                    .arena()
                    .comparison_chain_operands(pairs)
                    .collect::<Vec<_>>()
                {
                    self.scan_expr(operand);
                }
            }
            ArenaExprKind::Binary { left, right, .. }
            | ArenaExprKind::Index {
                base: left,
                index: right,
                ..
            } => {
                self.scan_expr(left);
                self.scan_expr(right);
            }
            ArenaExprKind::Call { callee, args } => {
                if self.add_direct_callee(callee).is_none() {
                    self.scan_expr(callee);
                }
                for arg in self.arena().call_args(args).to_vec() {
                    self.scan_call_arg(&arg);
                }
            }
            ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name } => {
                if self.add_dynamic_qualified_field(base, name).is_none() {
                    self.scan_expr(base);
                }
            }
            ArenaExprKind::Slice {
                base, start, end, ..
            } => {
                self.scan_expr(base);
                if let Some(start) = start {
                    self.scan_expr(start);
                }
                if let Some(end) = end {
                    self.scan_expr(end);
                }
            }
            ArenaExprKind::Pipeline { input, stages } => {
                self.scan_expr(input);
                for stage in self.arena().pipe_stages(stages).to_vec() {
                    self.scan_pipe_stage(&stage);
                }
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                self.scan_expr(input);
                for stage in self.arena().stream_stages(stages).to_vec() {
                    self.scan_stream_stage(&stage);
                }
            }
            ArenaExprKind::Run(run) => self.scan_run(run),
            ArenaExprKind::Spawn(form) => match form.target {
                ArenaSpawnTarget::Run(run) => self.scan_run(run),
                ArenaSpawnTarget::Command(command) => self.scan_expr(command),
            },
            ArenaExprKind::Wait(form) => self.scan_expr(form.target),
            ArenaExprKind::BuilderCall { call, block } => {
                self.scan_expr(call);
                self.scan_builder_block(block);
            }
            ArenaExprKind::ErrorContext { message, block }
            | ArenaExprKind::ContextScope {
                input: message,
                block,
                ..
            } => {
                self.scan_expr(message);
                self.scan_block(block);
            }
            ArenaExprKind::Capture(block)
            | ArenaExprKind::ValueBlock(block)
            | ArenaExprKind::Loop { block }
            | ArenaExprKind::TempDirScope { block, .. } => self.scan_block(block),
            ArenaExprKind::Retry { delays, block, .. } => {
                for delay in self.arena().expr_ids(delays).collect::<Vec<_>>() {
                    self.scan_expr(delay);
                }
                self.scan_block(block);
            }
            ArenaExprKind::Null
            | ArenaExprKind::Bool(_)
            | ArenaExprKind::Int(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::Str(_)
            | ArenaExprKind::PathStr(_)
            | ArenaExprKind::GlobStr(_)
            | ArenaExprKind::Bytes(_)
            | ArenaExprKind::Regex(_)
            | ArenaExprKind::Item
            | ArenaExprKind::LastStatus
            | ArenaExprKind::EnvString(_)
            | ArenaExprKind::EnvPathList => {}
        }
    }

    fn scan_call_arg(&mut self, arg: &ArenaCallArg) {
        match arg.kind {
            ArenaCallArgKind::Positional(expr)
            | ArenaCallArgKind::Named { value: expr, .. }
            | ArenaCallArgKind::Splice { value: expr, .. }
            | ArenaCallArgKind::NamedSpread { value: expr, .. } => self.scan_expr(expr),
        }
    }

    fn scan_pipe_stage(&mut self, stage: &ArenaPipeStage) {
        match &stage.kind {
            ArenaPipeStageKind::Expr(expr) => self.scan_expr(*expr),
            ArenaPipeStageKind::Stream(stage) => self.scan_stream_stage(stage),
        }
    }

    fn scan_stream_stage(&mut self, stage: &ArenaStreamStage) {
        for arg in self.arena().call_args(stage.args).to_vec() {
            self.scan_call_arg(&arg);
        }
        if let Some(block) = stage.block {
            self.scan_block(block);
        }
    }

    fn scan_builder_block(&mut self, block: BuilderBlockId) {
        let entries = self
            .arena()
            .builder_entries(self.arena().builder_block(block).entries)
            .to_vec();
        for entry in entries {
            match entry.kind {
                ArenaBuilderEntryKind::Field { value, .. } => self.scan_expr(value),
                ArenaBuilderEntryKind::Entry { args, block, .. } => {
                    for arg in self.arena().command_args(args).to_vec() {
                        self.scan_command_arg(&arg);
                    }
                    if let Some(block) = block {
                        self.scan_builder_block(block);
                    }
                }
                ArenaBuilderEntryKind::Task { block, .. } => self.scan_block(block),
                ArenaBuilderEntryKind::Stmt(stmt) => self.scan_stmt(stmt),
            }
        }
    }

    fn scan_command(&mut self, command: CommandStmtId) {
        match self.arena().command_stmt(command).command.clone() {
            ArenaCommand::Proc { name, args } => {
                self.add_direct_unqualified(name);
                for arg in self.arena().command_args(args).to_vec() {
                    self.scan_command_arg(&arg);
                }
            }
            ArenaCommand::Core {
                args, env, block, ..
            } => {
                for arg in self.arena().command_args(args).to_vec() {
                    self.scan_command_arg(&arg);
                }
                self.scan_env_assignments(env);
                if let Some(block) = block {
                    self.scan_block(block);
                }
            }
            ArenaCommand::Run(run) => self.scan_run(run),
        }
    }

    fn scan_run(&mut self, run: RunFormId) {
        let segments = self
            .arena()
            .run_segments(self.arena().run_form(run).segments)
            .to_vec();
        for segment in segments {
            if let Some(timeout) = segment.timeout {
                self.scan_expr(timeout);
            }
            if let Some(cpu_max) = segment.cpu_max {
                self.scan_expr(cpu_max);
            }
            if let Some(accept) = segment.accept {
                self.scan_expr(accept);
            }
            self.scan_env_assignments(segment.env);
            self.scan_command_arg(&segment.target);
            for arg in self.arena().command_args(segment.args).to_vec() {
                self.scan_command_arg(&arg);
            }
            for redirection in self.arena().redirections(segment.redirections).to_vec() {
                match redirection.target {
                    ArenaRedirectionTarget::Path(arg) | ArenaRedirectionTarget::Fd(arg) => {
                        self.scan_command_arg(&arg)
                    }
                }
            }
        }
    }

    fn scan_env_assignments(&mut self, assignments: ArenaRange) {
        for assignment in self.arena().env_assignments(assignments).to_vec() {
            match assignment.value {
                ArenaEnvAssignmentValue::CommandArg(arg) => self.scan_command_arg(&arg),
                ArenaEnvAssignmentValue::Expr(expr) => self.scan_expr(expr),
            }
        }
    }

    fn scan_command_arg(&mut self, arg: &ArenaCommandArg) {
        match &arg.kind {
            ArenaCommandArgKind::Word(parts) => {
                for part in self.arena().word_parts(*parts).collect::<Vec<_>>() {
                    match part {
                        ArenaWordPart::Interpolation(expr) | ArenaWordPart::Shorthand(expr) => {
                            self.scan_expr(expr)
                        }
                        ArenaWordPart::Bare(_) | ArenaWordPart::Quoted(_) => {}
                    }
                }
            }
            ArenaCommandArgKind::SpliceName(name) => self.add_dynamic_unqualified(*name),
            ArenaCommandArgKind::SpliceExpr(expr) | ArenaCommandArgKind::Typed(expr) => {
                self.scan_expr(*expr)
            }
        }
    }

    fn scan_assign_target(&mut self, target: AssignTargetId) {
        match self.arena().assign_target(target).kind {
            ArenaAssignTargetKind::Name(_) | ArenaAssignTargetKind::Env(_) => {}
            ArenaAssignTargetKind::Field { base, .. } => self.scan_assign_target(base),
            ArenaAssignTargetKind::Index { base, index } => {
                self.scan_assign_target(base);
                self.scan_expr(index);
            }
        }
    }

    fn define_binding_target(&mut self, target: BindingTargetId) {
        match self.arena().binding_target(target).kind.clone() {
            ArenaBindingTargetKind::Name(name) => self.define(name),
            ArenaBindingTargetKind::Record { fields, .. } => {
                for field in self.arena().destructure_fields(fields).to_vec() {
                    self.define_binding_target(field.target);
                }
            }
        }
    }

    fn define_pattern(&mut self, pattern: PatternId) {
        match self.arena().pattern(pattern).kind.clone() {
            ArenaPatternKind::Group(child) => self.define_pattern(child),
            ArenaPatternKind::Alias { pattern, name, .. } => {
                self.define_pattern(pattern);
                self.define(name);
            }
            ArenaPatternKind::Binding(name) => self.define(name),
            ArenaPatternKind::Type {
                binding: Some(name),
                ..
            } => self.define(name),
            ArenaPatternKind::Record { fields, .. } => {
                for field in self.arena().pattern_fields(fields).to_vec() {
                    self.define_pattern(field.pattern);
                }
            }
            ArenaPatternKind::List { elements, rest } => {
                for child in self
                    .arena()
                    .pattern_ids(elements)
                    .chain(rest)
                    .collect::<Vec<_>>()
                {
                    self.define_pattern(child);
                }
            }
            ArenaPatternKind::Alternation(patterns) | ArenaPatternKind::Tuple(patterns) => {
                for pattern in self.arena().pattern_ids(patterns).collect::<Vec<_>>() {
                    self.define_pattern(pattern);
                }
            }
            ArenaPatternKind::Constructor { arg: Some(arg), .. } => self.define_pattern(arg),
            ArenaPatternKind::ErrorVariant { fields, .. } => {
                for field in self.arena().pattern_fields(fields).to_vec() {
                    self.define_pattern(field.pattern);
                }
            }
            ArenaPatternKind::Literal(_)
            | ArenaPatternKind::Wildcard
            | ArenaPatternKind::Type { binding: None, .. }
            | ArenaPatternKind::Constructor { arg: None, .. }
            | ArenaPatternKind::Facet(_)
            | ArenaPatternKind::TestName { .. } => {}
        }
    }

    fn add_direct_callee(&mut self, callee: ExprId) -> Option<usize> {
        let target = match self.arena().expr(callee).kind {
            // `Checker::check_call_arena` resolves declared callables before
            // ordinary bindings. Match that resolution order here; scopes only
            // matter when an identity is used as a value rather than called.
            ArenaExprKind::Ident(name) => self.analysis.resolve_unqualified(self.namespace, name),
            ArenaExprKind::Field { base, name } => match self.arena().expr(base).kind {
                ArenaExprKind::Ident(alias) => {
                    self.analysis.resolve_qualified(self.namespace, alias, name)
                }
                _ => None,
            },
            ArenaExprKind::NullSafeField { .. } => None,
            _ => None,
        };
        if let Some(target) = target {
            self.edges.direct.insert(target);
        }
        target
    }

    fn add_direct_unqualified(&mut self, name: Name) {
        if let Some(target) = self.analysis.resolve_unqualified(self.namespace, name) {
            self.edges.direct.insert(target);
        }
    }

    fn add_dynamic_unqualified(&mut self, name: Name) {
        if let Some(target) = self.resolve_unqualified(name) {
            self.edges.dynamic.insert(target);
        }
    }

    fn add_dynamic_qualified_field(&mut self, base: ExprId, name: Name) -> Option<usize> {
        let ArenaExprKind::Ident(alias) = self.arena().expr(base).kind else {
            return None;
        };
        let target = self.resolve_qualified(alias, name)?;
        self.edges.dynamic.insert(target);
        Some(target)
    }

    fn resolve_unqualified(&self, name: Name) -> Option<usize> {
        if self.is_bound(name) {
            return None;
        }
        self.analysis.resolve_unqualified(self.namespace, name)
    }

    fn resolve_qualified(&self, alias: Name, name: Name) -> Option<usize> {
        if self.is_bound(alias) {
            return None;
        }
        self.analysis.resolve_qualified(self.namespace, alias, name)
    }

    fn push_scope(&mut self) {
        self.scopes.push(FxHashSet::default());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop().expect("scanner scope underflow");
    }

    fn define(&mut self, name: Name) {
        self.scopes
            .last_mut()
            .expect("scanner always has a scope")
            .insert(name);
    }

    fn is_bound(&self, name: Name) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(&name))
    }
}

/// The normal and abrupt exits reachable from a statement or expression.
///
/// `fallthrough` alone decides whether the next source statement can run. The
/// other bits are retained until the enclosing construct consumes them: a loop
/// consumes its body's `break` and `continue`, while a function boundary
/// consumes `return`. `terminates` denotes a proven path with no normal
/// successor, such as `match-no-arm`, `abort`, or a looping path.
#[derive(Clone, Copy, Debug, Default)]
struct FlowSummary {
    fallthrough: bool,
    returns: bool,
    breaks: bool,
    continues: bool,
    terminates: bool,
}

impl FlowSummary {
    const fn fallthrough() -> Self {
        Self {
            fallthrough: true,
            returns: false,
            breaks: false,
            continues: false,
            terminates: false,
        }
    }

    const fn returning() -> Self {
        Self {
            fallthrough: false,
            returns: true,
            breaks: false,
            continues: false,
            terminates: false,
        }
    }

    const fn breaking() -> Self {
        Self {
            fallthrough: false,
            returns: false,
            breaks: true,
            continues: false,
            terminates: false,
        }
    }

    const fn continuing() -> Self {
        Self {
            fallthrough: false,
            returns: false,
            breaks: false,
            continues: true,
            terminates: false,
        }
    }

    const fn terminating() -> Self {
        Self {
            fallthrough: false,
            returns: false,
            breaks: false,
            continues: false,
            terminates: true,
        }
    }

    fn union(mut self, other: Self) -> Self {
        self.fallthrough |= other.fallthrough;
        self.returns |= other.returns;
        self.breaks |= other.breaks;
        self.continues |= other.continues;
        self.terminates |= other.terminates;
        self
    }

    /// Compose `next` after the normal path in `self`, retaining every abrupt
    /// exit that escaped earlier statements in the sequence.
    fn then(mut self, next: Self) -> Self {
        if self.fallthrough {
            self.fallthrough = next.fallthrough;
            self.returns |= next.returns;
            self.breaks |= next.breaks;
            self.continues |= next.continues;
            self.terminates |= next.terminates;
        }
        self
    }
}

fn block_flow(
    arena: &AstArena,
    block: BlockId,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    let mut flow = FlowSummary::fallthrough();
    for stmt in arena.stmt_ids(arena.block(block).statements) {
        if !flow.fallthrough {
            break;
        }
        flow = flow.then(stmt_flow(arena, stmt, terminating_call_spans));
    }
    flow
}

fn stmt_flow(
    arena: &AstArena,
    stmt: StmtId,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    match arena.stmt(stmt).kind {
        // Control flow is the form's meaning, which only its expansion states.
        ArenaStmtKind::Export(inner) | ArenaStmtKind::Sugar { expansion: inner, .. } => {
            stmt_flow(arena, inner, terminating_call_spans)
        }
        ArenaStmtKind::Let { initializer, .. }
        | ArenaStmtKind::Const { initializer, .. }
        | ArenaStmtKind::Var { initializer, .. } => {
            expr_or_run_flow(arena, &initializer, terminating_call_spans)
        }
        ArenaStmtKind::Assign { target, value, .. } => assign_target_flow(
            arena,
            target,
            terminating_call_spans,
        )
        .then(expr_or_run_flow(arena, &value, terminating_call_spans)),
        ArenaStmtKind::Return(value) => value
            .as_ref()
            .map(|value| expr_or_run_flow(arena, value, terminating_call_spans))
            .unwrap_or_else(FlowSummary::fallthrough)
            .then(FlowSummary::returning()),
        // A deferred expression is registered now and runs only during unwind.
        // It must still be linted, but it cannot make following source dead.
        ArenaStmtKind::Defer(..) => FlowSummary::fallthrough(),
        ArenaStmtKind::Yield(value) => expr_or_run_flow(arena, &value, terminating_call_spans),
        ArenaStmtKind::If {
            branches,
            else_block,
        } => if_stmt_flow(arena, branches, else_block, terminating_call_spans),
        ArenaStmtKind::While { condition, block } => {
            let condition = expr_flow(arena, condition, terminating_call_spans);
            let body = block_flow(arena, block, terminating_call_spans);
            // The condition may be false before the first iteration.
            FlowSummary {
                fallthrough: condition.fallthrough,
                returns: condition.returns | body.returns,
                breaks: false,
                continues: false,
                terminates: condition.terminates | body.terminates,
            }
        }
        ArenaStmtKind::For { iter, block, .. } => {
            let iter = expr_flow(arena, iter, terminating_call_spans);
            let body = block_flow(arena, block, terminating_call_spans);
            // A successful iterator may be empty.
            FlowSummary {
                fallthrough: iter.fallthrough,
                returns: iter.returns | body.returns,
                breaks: false,
                continues: false,
                terminates: iter.terminates | body.terminates,
            }
        }
        ArenaStmtKind::With {
            bindings,
            body,
            else_block,
            ..
        } => with_stmt_flow(arena, bindings, body, else_block, terminating_call_spans),
        ArenaStmtKind::Loop { block } => loop_flow(arena, block, terminating_call_spans),
        ArenaStmtKind::Guard {
            initializer,
            else_block,
            ..
        } => {
            let initializer = expr_or_run_flow(arena, &initializer, terminating_call_spans);
            let else_flow = block_flow(arena, else_block, terminating_call_spans);
            // A successful guard always continues after the statement.
            initializer.then(FlowSummary::fallthrough().union(else_flow))
        }
        ArenaStmtKind::Assert { condition, message } => {
            expr_flow(arena, condition, terminating_call_spans).then(
                FlowSummary::fallthrough().union(match message {
                    Some(message) => expr_flow(arena, message, terminating_call_spans)
                        .then(FlowSummary::terminating()),
                    None => FlowSummary::terminating(),
                }),
            )
        }
        ArenaStmtKind::Break { value } => value
            .map(|value| expr_flow(arena, value, terminating_call_spans))
            .unwrap_or_else(FlowSummary::fallthrough)
            .then(FlowSummary::breaking()),
        ArenaStmtKind::Continue => FlowSummary::continuing(),
        ArenaStmtKind::Match { value, arms } => {
            let value = expr_flow(arena, value, terminating_call_spans);
            if !value.fallthrough {
                return value;
            }
            // Every unmatched or guard-rejected path produces the guaranteed
            // `match-no-arm` runtime error rather than falling through.
            let mut arms_flow = FlowSummary::terminating();
            for arm in arena.match_arms(arms) {
                arms_flow = arms_flow.union(block_flow(arena, arm.block, terminating_call_spans));
            }
            value.then(arms_flow)
        }
        ArenaStmtKind::Command(command) => command_flow(arena, command, terminating_call_spans),
        ArenaStmtKind::TailBareIdent(_) => FlowSummary::fallthrough(),
        ArenaStmtKind::Expr(expr) | ArenaStmtKind::YieldDelegate(expr) => {
            expr_flow(arena, expr, terminating_call_spans)
        }
        // The status is evaluated, and then nothing after the statement runs.
        ArenaStmtKind::Exit(status) => {
            expr_flow(arena, status, terminating_call_spans).then(FlowSummary::terminating())
        }
        ArenaStmtKind::Use(_)
        | ArenaStmtKind::TypeDef(_)
        | ArenaStmtKind::ErrorDef(_)
        | ArenaStmtKind::ProcDef(_)
        | ArenaStmtKind::CliMain(_)
        | ArenaStmtKind::PureDef(_)
        | ArenaStmtKind::StreamDef(_)
        | ArenaStmtKind::SignalHook(_) => FlowSummary::fallthrough(),
    }
}

fn if_stmt_flow(
    arena: &AstArena,
    branches: ArenaRange,
    else_block: Option<BlockId>,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    let mut flow = FlowSummary::default();
    let mut next_condition_reachable = true;
    for branch in arena.if_branches(branches) {
        if !next_condition_reachable {
            break;
        }
        let condition = expr_flow(arena, branch.condition, terminating_call_spans);
        flow = flow.union(FlowSummary {
            fallthrough: false,
            returns: condition.returns,
            breaks: condition.breaks,
            continues: condition.continues,
            terminates: condition.terminates,
        });
        if condition.fallthrough {
            flow = flow.union(block_flow(arena, branch.block, terminating_call_spans));
        }
        next_condition_reachable = condition.fallthrough;
    }
    if next_condition_reachable {
        flow = flow.union(match else_block {
            Some(block) => block_flow(arena, block, terminating_call_spans),
            None => FlowSummary::fallthrough(),
        });
    }
    flow
}

fn with_stmt_flow(
    arena: &AstArena,
    bindings: ArenaRange,
    body: BlockId,
    else_block: BlockId,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    let mut bindings_flow = FlowSummary::fallthrough();
    for binding in arena.with_bindings(bindings) {
        bindings_flow = bindings_flow.then(expr_flow(
            arena,
            binding.initializer,
            terminating_call_spans,
        ));
    }
    bindings_flow.then(
        block_flow(arena, body, terminating_call_spans).union(block_flow(
            arena,
            else_block,
            terminating_call_spans,
        )),
    )
}

fn loop_flow(
    arena: &AstArena,
    block: BlockId,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    let body = block_flow(arena, block, terminating_call_spans);
    FlowSummary {
        fallthrough: body.breaks,
        returns: body.returns,
        breaks: false,
        continues: false,
        terminates: body.terminates || (!body.breaks && (body.fallthrough || body.continues)),
    }
}

fn expr_or_run_flow(
    arena: &AstArena,
    value: &ArenaExprOrRun,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    match value {
        ArenaExprOrRun::Expr(expr) => expr_flow(arena, *expr, terminating_call_spans),
        ArenaExprOrRun::Run(run) => run_flow(arena, *run, terminating_call_spans),
    }
}

fn expr_flow(
    arena: &AstArena,
    expr: ExprId,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    let arena_expr = arena.expr(expr);
    match arena_expr.kind {
        ArenaExprKind::ValuePipelineCall { input, call, .. } => expr_flow(
            arena,
            input,
            terminating_call_spans,
        )
        .then(expr_flow(arena, call, terminating_call_spans)),

        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => arena
            .fmt_parts(parts)
            .fold(FlowSummary::fallthrough(), |flow, part| match part {
                ArenaFmtPart::Expr(expr, _) => {
                    flow.then(expr_flow(arena, expr, terminating_call_spans))
                }
                ArenaFmtPart::Text(_) => flow,
            }),
        ArenaExprKind::List(items) => arena
            .list_element_exprs(items)
            .fold(FlowSummary::fallthrough(), |flow, item| {
                flow.then(expr_flow(arena, item, terminating_call_spans))
            }),
        ArenaExprKind::ListComp { qualifiers, .. } | ArenaExprKind::MapComp { qualifiers, .. } => {
            let first = arena
                .comp_qualifiers(qualifiers)
                .first()
                .expect("comprehension has initial for");
            expr_flow(arena, first.expr(), terminating_call_spans)
        }
        ArenaExprKind::Record(fields) => {
            arena
                .record_fields(fields)
                .iter()
                .fold(FlowSummary::fallthrough(), |flow, field| match field.kind {
                    ArenaRecordFieldKind::Computed { key, value, .. } => flow
                        .then(expr_flow(arena, key, terminating_call_spans))
                        .then(expr_flow(arena, value, terminating_call_spans)),
                    ArenaRecordFieldKind::Named { value, .. }
                    | ArenaRecordFieldKind::Path { value, .. } => {
                        flow.then(expr_flow(arena, value, terminating_call_spans))
                    }
                    ArenaRecordFieldKind::Spread { expr, .. } => {
                        flow.then(expr_flow(arena, expr, terminating_call_spans))
                    }
                    ArenaRecordFieldKind::Shorthand { .. } => flow,
                })
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => if_expr_flow(arena, branches, else_value, terminating_call_spans),
        ArenaExprKind::Match { value, arms }
        | ArenaExprKind::PatternTest { value, arms }
        | ArenaExprKind::PatternCondition { value, arms } => {
            let value = expr_flow(arena, value, terminating_call_spans);
            if !value.fallthrough {
                return value;
            }
            let mut arms_flow = FlowSummary::terminating();
            for arm in arena.match_expr_arms(arms) {
                let arm_flow = arm
                    .guard
                    .map(|guard| expr_flow(arena, guard, terminating_call_spans))
                    .unwrap_or_else(FlowSummary::fallthrough)
                    .then(expr_flow(arena, arm.value, terminating_call_spans));
                arms_flow = arms_flow.union(arm_flow);
            }
            value.then(arms_flow)
        }
        ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => {
            expr_flow(arena, expr, terminating_call_spans)
        }
        ArenaExprKind::ComparisonChain(pairs) => {
            let mut operands = arena.comparison_chain_operands(pairs);
            let first = operands
                .next()
                .map(|operand| expr_flow(arena, operand, terminating_call_spans))
                .unwrap_or_else(FlowSummary::fallthrough);
            let second = operands
                .next()
                .map(|operand| expr_flow(arena, operand, terminating_call_spans))
                .unwrap_or_else(FlowSummary::fallthrough);
            let mut flow = first.then(second);
            for operand in operands {
                flow = flow.then(FlowSummary::fallthrough().union(expr_flow(
                    arena,
                    operand,
                    terminating_call_spans,
                )));
            }
            flow
        }
        ArenaExprKind::Binary {
            op: BinaryOp::ResultFallback,
            left,
            right,
        } => expr_flow(arena, left, terminating_call_spans).then(
            expr_flow(arena, right, terminating_call_spans).union(FlowSummary::fallthrough()),
        ),
        ArenaExprKind::Binary { left, right, .. } => expr_flow(arena, left, terminating_call_spans)
            .then(expr_flow(arena, right, terminating_call_spans)),
        ArenaExprKind::Call { callee, args } => {
            let receiver_flow = expr_flow(arena, callee, terminating_call_spans);
            let guarded = matches!(arena.expr(callee).kind, ArenaExprKind::NullSafeField { .. });
            let mut flow = FlowSummary::fallthrough();
            for arg in arena.call_args(args) {
                flow = flow.then(call_arg_flow(arena, arg, terminating_call_spans));
            }
            if terminating_call_spans.contains(&arena_expr.span) {
                flow = flow.then(FlowSummary::terminating());
            }
            receiver_flow.then(if guarded {
                flow.union(FlowSummary::fallthrough())
            } else {
                flow
            })
        }
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
            expr_flow(arena, base, terminating_call_spans)
        }
        ArenaExprKind::Index {
            base,
            index,
            guarded,
        } => {
            let selected = expr_flow(arena, index, terminating_call_spans);
            expr_flow(arena, base, terminating_call_spans).then(if guarded {
                selected.union(FlowSummary::fallthrough())
            } else {
                selected
            })
        }
        ArenaExprKind::Slice {
            base,
            start,
            end,
            guarded,
        } => {
            let selected = start
                .map(|start| expr_flow(arena, start, terminating_call_spans))
                .unwrap_or_else(FlowSummary::fallthrough)
                .then(
                    end.map(|end| expr_flow(arena, end, terminating_call_spans))
                        .unwrap_or_else(FlowSummary::fallthrough),
                );
            expr_flow(arena, base, terminating_call_spans).then(if guarded {
                selected.union(FlowSummary::fallthrough())
            } else {
                selected
            })
        }
        ArenaExprKind::Pipeline { input, stages } => {
            let mut flow = expr_flow(arena, input, terminating_call_spans);
            for stage in arena.pipe_stages(stages) {
                flow = flow.then(pipe_stage_flow(arena, stage, terminating_call_spans));
            }
            flow
        }
        ArenaExprKind::StructuredPipeline { input, stages } => {
            let mut flow = expr_flow(arena, input, terminating_call_spans);
            for stage in arena.stream_stages(stages) {
                flow = flow.then(stream_stage_flow(arena, stage, terminating_call_spans));
            }
            flow
        }
        ArenaExprKind::Run(run) => run_flow(arena, run, terminating_call_spans),
        ArenaExprKind::Spawn(form) => match form.target {
            ArenaSpawnTarget::Run(run) => run_flow(arena, run, terminating_call_spans),
            ArenaSpawnTarget::Command(command) => expr_flow(arena, command, terminating_call_spans),
        },
        ArenaExprKind::Wait(form) => expr_flow(arena, form.target, terminating_call_spans),
        ArenaExprKind::BuilderCall { call, block } => {
            expr_flow(arena, call, terminating_call_spans).then(builder_block_setup_flow(
                arena,
                block,
                terminating_call_spans,
            ))
        }
        ArenaExprKind::Require { value, .. } => expr_flow(arena, value, terminating_call_spans),
        ArenaExprKind::ContextScope { input, block, .. } => {
            expr_flow(arena, input, terminating_call_spans).then(
                FlowSummary::fallthrough().union(block_flow(arena, block, terminating_call_spans)),
            )
        }
        // Creating the directory may fail before the body runs.
        ArenaExprKind::TempDirScope { block, .. } => {
            FlowSummary::fallthrough().union(block_flow(arena, block, terminating_call_spans))
        }
        ArenaExprKind::ErrorContext { message, block } => expr_flow(
            arena,
            message,
            terminating_call_spans,
        )
        .then(block_flow(arena, block, terminating_call_spans)),
        ArenaExprKind::Capture(block) | ArenaExprKind::ValueBlock(block) => {
            block_flow(arena, block, terminating_call_spans)
        }
        ArenaExprKind::Loop { block } => loop_flow(arena, block, terminating_call_spans),
        // A retry retries failed attempts, but a normally-completing attempt
        // produces the expression's `Result`; it is not an infinite loop.
        ArenaExprKind::Retry { delays, block, .. } => arena
            .expr_ids(delays)
            .fold(FlowSummary::fallthrough(), |flow, delay| {
                flow.then(expr_flow(arena, delay, terminating_call_spans))
            })
            .then(block_flow(arena, block, terminating_call_spans)),
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Regex(_)
        | ArenaExprKind::Ident(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::EnvString(_)
        | ArenaExprKind::EnvPathList => FlowSummary::fallthrough(),
    }
}

fn if_expr_flow(
    arena: &AstArena,
    branches: ArenaRange,
    else_value: ExprId,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    let mut flow = FlowSummary::default();
    let mut next_condition_reachable = true;
    for branch in arena.if_expr_branches(branches) {
        if !next_condition_reachable {
            break;
        }
        let condition = expr_flow(arena, branch.condition, terminating_call_spans);
        flow = flow.union(FlowSummary {
            fallthrough: false,
            returns: condition.returns,
            breaks: condition.breaks,
            continues: condition.continues,
            terminates: condition.terminates,
        });
        if condition.fallthrough {
            flow = flow.union(expr_flow(arena, branch.value, terminating_call_spans));
        }
        next_condition_reachable = condition.fallthrough;
    }
    if next_condition_reachable {
        flow = flow.union(expr_flow(arena, else_value, terminating_call_spans));
    }
    flow
}

fn call_arg_flow(
    arena: &AstArena,
    arg: &ArenaCallArg,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    match arg.kind {
        ArenaCallArgKind::Positional(expr)
        | ArenaCallArgKind::Named { value: expr, .. }
        | ArenaCallArgKind::Splice { value: expr, .. }
        | ArenaCallArgKind::NamedSpread { value: expr, .. } => {
            expr_flow(arena, expr, terminating_call_spans)
        }
    }
}

fn pipe_stage_flow(
    arena: &AstArena,
    stage: &ArenaPipeStage,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    match &stage.kind {
        ArenaPipeStageKind::Expr(expr) => expr_flow(arena, *expr, terminating_call_spans),
        ArenaPipeStageKind::Stream(stage) => {
            stream_stage_flow(arena, stage, terminating_call_spans)
        }
    }
}

fn stream_stage_flow(
    arena: &AstArena,
    stage: &ArenaStreamStage,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    let mut flow = FlowSummary::fallthrough();
    for arg in arena.call_args(stage.args) {
        flow = flow.then(call_arg_flow(arena, arg, terminating_call_spans));
    }
    // Stream-stage blocks execute in their own item region. Their flow does
    // not determine whether the enclosing pipeline expression returns.
    flow
}

fn builder_block_setup_flow(
    arena: &AstArena,
    block: BuilderBlockId,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    let mut flow = FlowSummary::fallthrough();
    for entry in arena.builder_entries(arena.builder_block(block).entries) {
        match &entry.kind {
            ArenaBuilderEntryKind::Field { value, .. } => {
                flow = flow.then(expr_flow(arena, *value, terminating_call_spans));
            }
            ArenaBuilderEntryKind::Entry { args, block, .. } => {
                for arg in arena.command_args(*args) {
                    flow = flow.then(command_arg_flow(arena, arg, terminating_call_spans));
                }
                if let Some(block) = block {
                    flow = flow.then(builder_block_setup_flow(
                        arena,
                        *block,
                        terminating_call_spans,
                    ));
                }
            }
            // Task blocks are independent regions. They are analyzed for their
            // own diagnostics by the linter, not as eager builder setup.
            ArenaBuilderEntryKind::Task { .. } | ArenaBuilderEntryKind::Stmt(_) => {}
        }
    }
    flow
}

fn assign_target_flow(
    arena: &AstArena,
    target: AssignTargetId,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    match arena.assign_target(target).kind {
        ArenaAssignTargetKind::Name(_) | ArenaAssignTargetKind::Env(_) => {
            FlowSummary::fallthrough()
        }
        ArenaAssignTargetKind::Field { base, .. } => {
            assign_target_flow(arena, base, terminating_call_spans)
        }
        ArenaAssignTargetKind::Index { base, index } => assign_target_flow(
            arena,
            base,
            terminating_call_spans,
        )
        .then(expr_flow(arena, index, terminating_call_spans)),
    }
}

fn command_flow(
    arena: &AstArena,
    command: CommandStmtId,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    match arena.command_stmt(command).command.clone() {
        ArenaCommand::Proc { args, .. } => arena
            .command_args(args)
            .iter()
            .fold(FlowSummary::fallthrough(), |flow, arg| {
                flow.then(command_arg_flow(arena, arg, terminating_call_spans))
            }),
        ArenaCommand::Core {
            args, env, block, ..
        } => {
            let mut flow = FlowSummary::fallthrough();
            for arg in arena.command_args(args) {
                flow = flow.then(command_arg_flow(arena, arg, terminating_call_spans));
            }
            for assignment in arena.env_assignments(env) {
                let assignment_flow = match &assignment.value {
                    ArenaEnvAssignmentValue::CommandArg(arg) => {
                        command_arg_flow(arena, arg, terminating_call_spans)
                    }
                    ArenaEnvAssignmentValue::Expr(expr) => {
                        expr_flow(arena, *expr, terminating_call_spans)
                    }
                };
                flow = flow.then(assignment_flow);
            }
            if let Some(block) = block {
                flow.then(block_flow(arena, block, terminating_call_spans))
            } else {
                flow
            }
        }
        ArenaCommand::Run(run) => run_flow(arena, run, terminating_call_spans),
    }
}

fn run_flow(
    arena: &AstArena,
    run: RunFormId,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    let mut flow = FlowSummary::fallthrough();
    for segment in arena.run_segments(arena.run_form(run).segments) {
        if let Some(timeout) = segment.timeout {
            flow = flow.then(expr_flow(arena, timeout, terminating_call_spans));
        }
        if let Some(cpu_max) = segment.cpu_max {
            flow = flow.then(expr_flow(arena, cpu_max, terminating_call_spans));
        }
        if let Some(accept) = segment.accept {
            flow = flow.then(expr_flow(arena, accept, terminating_call_spans));
        }
        for assignment in arena.env_assignments(segment.env) {
            let assignment_flow = match &assignment.value {
                ArenaEnvAssignmentValue::CommandArg(arg) => {
                    command_arg_flow(arena, arg, terminating_call_spans)
                }
                ArenaEnvAssignmentValue::Expr(expr) => {
                    expr_flow(arena, *expr, terminating_call_spans)
                }
            };
            flow = flow.then(assignment_flow);
        }
        flow = flow.then(command_arg_flow(
            arena,
            &segment.target,
            terminating_call_spans,
        ));
        for arg in arena.command_args(segment.args) {
            flow = flow.then(command_arg_flow(arena, arg, terminating_call_spans));
        }
        for redirection in arena.redirections(segment.redirections) {
            let target = match &redirection.target {
                ArenaRedirectionTarget::Path(arg) | ArenaRedirectionTarget::Fd(arg) => arg,
            };
            flow = flow.then(command_arg_flow(arena, target, terminating_call_spans));
        }
    }
    flow
}

fn command_arg_flow(
    arena: &AstArena,
    arg: &ArenaCommandArg,
    terminating_call_spans: &BTreeSet<Span>,
) -> FlowSummary {
    match &arg.kind {
        ArenaCommandArgKind::Word(parts) => {
            arena
                .word_parts(*parts)
                .fold(FlowSummary::fallthrough(), |flow, part| match part {
                    ArenaWordPart::Interpolation(expr) | ArenaWordPart::Shorthand(expr) => {
                        flow.then(expr_flow(arena, expr, terminating_call_spans))
                    }
                    ArenaWordPart::Bare(_) | ArenaWordPart::Quoted(_) => flow,
                })
        }
        ArenaCommandArgKind::SpliceExpr(expr) | ArenaCommandArgKind::Typed(expr) => {
            expr_flow(arena, *expr, terminating_call_spans)
        }
        ArenaCommandArgKind::SpliceName(_) => FlowSummary::fallthrough(),
    }
}

fn migration_inert(arena: &AstArena, expr: ExprId) -> bool {
    migration_literal(arena, expr)
        || matches!(
            arena.expr(expr).kind,
            ArenaExprKind::Ident(_) | ArenaExprKind::Item
        )
}

fn migration_literal(arena: &AstArena, expr: ExprId) -> bool {
    matches!(
        arena.expr(expr).kind,
        ArenaExprKind::Null
            | ArenaExprKind::Bool(_)
            | ArenaExprKind::Int(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::Str(_)
            | ArenaExprKind::Bytes(_)
            | ArenaExprKind::PathStr(_)
    )
}

fn migration_reorder_safe(arena: &AstArena, left: ExprId, right: ExprId) -> bool {
    migration_literal(arena, left)
        || migration_literal(arena, right)
        || (migration_inert(arena, left) && migration_inert(arena, right))
}

fn migration_arguments_optional(
    arena: &AstArena,
    args: ArenaRange,
    params: &[&str],
) -> Option<Vec<Option<ExprId>>> {
    let mut result = vec![None; params.len()];
    for (position, arg) in arena.call_args(args).iter().enumerate() {
        let (position, expr) = match arg.kind {
            ArenaCallArgKind::Positional(expr) => (position, expr),
            ArenaCallArgKind::Named { name, value, .. } => {
                (params.iter().position(|param| name == *param)?, value)
            }
            _ => return None,
        };
        if position >= result.len() || result[position].is_some() {
            return None;
        }
        result[position] = Some(expr);
    }
    Some(result)
}

fn migration_arguments(arena: &AstArena, args: ArenaRange, params: &[&str]) -> Option<Vec<ExprId>> {
    migration_arguments_optional(arena, args, params)?
        .into_iter()
        .collect()
}

fn same_ordering_operand(arena: &AstArena, left: ExprId, right: ExprId) -> bool {
    match (arena.expr(left).kind, arena.expr(right).kind) {
        (ArenaExprKind::Ident(left), ArenaExprKind::Ident(right)) => left == right,
        (ArenaExprKind::Int(left), ArenaExprKind::Int(right)) => arena
            .int_literal(left)
            .value()
            .zip(arena.int_literal(right).value())
            .is_some_and(|(left, right)| left == right),
        (ArenaExprKind::Float(left), ArenaExprKind::Float(right)) => arena
            .float_literal(left)
            .value()
            .zip(arena.float_literal(right).value())
            .is_some_and(|(left, right)| left.to_bits() == right.to_bits()),
        (ArenaExprKind::Str(left), ArenaExprKind::Str(right)) => {
            arena.string_literal(left) == arena.string_literal(right)
        }
        _ => false,
    }
}

#[derive(Eq, PartialEq)]
struct CheckedReturnRemovalFacts {
    expressions: Vec<(usize, usize, String)>,
    statements: Vec<(usize, usize, xsh::frontend::check::StatementPosition)>,
    returns: Vec<(usize, usize, String)>,
    parameters: Vec<(usize, usize, String)>,
    effects: BTreeMap<String, Option<Vec<Effect>>>,
}

// Original facts are independent of each deletion. Keep their source coordinates
// and complete type shapes so every candidate still proves the same contract.
struct CheckedLocalAnnotationFacts {
    bindings: BTreeMap<usize, String>,
    expressions: BTreeMap<(usize, usize), String>,
}

// Standalone proof parses do not load user modules. An invalid unresolved
// top-level use always errors, so its source cannot supply a proof baseline.
fn standalone_annotation_imports_available(program: &ArenaProgram) -> bool {
    program.symbol_owner().with_current(|| {
        program.statement_ids().all(|statement| {
            let kind = match program.arena.stmt(statement).kind {
                ArenaStmtKind::Export(inner) => program.arena.stmt(inner).kind,
                other => other,
            };
            let ArenaStmtKind::Use(import) = kind else {
                return true;
            };
            let import = program.arena.use_stmt(import);
            if import.resolved.is_some() {
                return true;
            }
            let mut path = program.arena.names(import.path);
            let Some(name) = path.next() else {
                return false;
            };
            path.next().is_none()
                && import.alias.is_none()
                && xsh::api::api_spec().module(&name.as_str()).is_some()
        })
    })
}

fn checked_local_annotation_facts(
    source: &str,
    source_id: xsh::frontend::source::SourceId,
) -> Option<CheckedLocalAnnotationFacts> {
    #[cfg(test)]
    local_annotation_probe_tests::record_original();
    let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(source_id, source);
    if !parsed.diagnostics.is_empty() || !standalone_annotation_imports_available(&parsed.arena) {
        return None;
    }
    #[cfg(test)]
    local_annotation_probe_tests::record_original_check();
    let checked = xsh::frontend::check::Checker::check_arena(&parsed.arena, source);
    if checked
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return None;
    }
    parsed.arena.symbol_owner().with_current(|| {
        let mut bindings = BTreeMap::new();
        for (span, ty) in &checked.local_binding_types {
            bindings
                .entry(span.start())
                .or_insert_with(|| checked_return_type_shape(ty));
        }
        let expressions = checked
            .expr_types
            .iter()
            .filter(|(span, _)| span.source_id == source_id)
            .map(|(span, ty)| ((span.start(), span.end()), checked_return_type_shape(ty)))
            .collect();
        Some(CheckedLocalAnnotationFacts {
            bindings,
            expressions,
        })
    })
}

/// Source edits must preserve every checked expression and statement purpose,
/// including caller overload selection, conversions, and implicit Result tails.
fn checked_return_removal_facts(
    source: &str,
    source_id: xsh::frontend::source::SourceId,
    removed: Option<(usize, usize)>,
) -> Option<CheckedReturnRemovalFacts> {
    #[cfg(test)]
    annotation_probe_tests::record_probe();
    let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(source_id, source);
    if !parsed.diagnostics.is_empty() || !standalone_annotation_imports_available(&parsed.arena) {
        return None;
    }
    let checked = xsh::frontend::check::Checker::check_arena(&parsed.arena, source);
    if !checked.diagnostics.is_empty() {
        return None;
    }
    let original_offset = |offset: usize| match removed {
        Some((start, length)) if offset >= start => offset + length,
        _ => offset,
    };
    parsed.arena.symbol_owner().with_current(|| {
        Some(CheckedReturnRemovalFacts {
            expressions: checked
                .expr_types
                .iter()
                .map(|(span, ty)| {
                    (
                        original_offset(span.start()),
                        original_offset(span.end()),
                        checked_return_type_shape(ty),
                    )
                })
                .collect(),
            statements: checked
                .statement_positions
                .iter()
                .map(|(span, position)| {
                    (
                        original_offset(span.start()),
                        original_offset(span.end()),
                        *position,
                    )
                })
                .collect(),
            returns: checked
                .function_return_types
                .iter()
                .map(|(span, ty)| {
                    (
                        original_offset(span.start()),
                        original_offset(span.end()),
                        checked_return_type_shape(ty),
                    )
                })
                .collect(),
            parameters: checked
                .parameter_types
                .iter()
                .map(|(span, ty)| {
                    (
                        original_offset(span.start()),
                        original_offset(span.end()),
                        checked_return_type_shape(ty),
                    )
                })
                .collect(),
            effects: checked
                .callable_effects
                .into_iter()
                .map(|(name, effects)| {
                    let effects = effects.map(|mut effects| {
                        effects.sort_by_key(Effect::as_str);
                        effects.dedup();
                        effects
                    });
                    (name, effects)
                })
                .collect(),
        })
    })
}

// Type display intentionally hides structural fields. Compare their complete
// checked shapes with names rendered under each source's symbol owner.
fn checked_return_type_shape(ty: &Type) -> String {
    match ty {
        Type::ErasedRecord => "ErasedRecord".to_string(),
        Type::Record(fields) => format!(
            "Record{:?}",
            fields
                .iter()
                .map(|(name, ty)| (name.to_string(), checked_return_type_shape(ty)))
                .collect::<BTreeMap<_, _>>()
        ),
        Type::Module(exports) => format!(
            "Module{:?}",
            exports
                .iter()
                .map(|(name, export)| {
                    use xsh::frontend::check::ModuleExportType;
                    let shape = match export {
                        ModuleExportType::Value { ty, optional } => {
                            format!("value:{optional}:{}", checked_return_type_shape(ty))
                        }
                        ModuleExportType::Pure { sig, optional }
                        | ModuleExportType::Proc { sig, optional } => {
                            let params = sig
                                .params
                                .iter()
                                .map(|param| {
                                    (
                                        param.name.to_string(),
                                        checked_return_type_shape(&param.ty),
                                        param.defaulted,
                                        param.rest,
                                    )
                                })
                                .collect::<Vec<_>>();
                            format!(
                                "{}:{optional}:{params:?}:{}:{:?}",
                                if matches!(export, ModuleExportType::Pure { .. }) {
                                    "pure"
                                } else {
                                    "proc"
                                },
                                checked_return_type_shape(&sig.return_ty),
                                sig.effects
                            )
                        }
                    };
                    (name.to_string(), shape)
                })
                .collect::<BTreeMap<_, _>>()
        ),
        Type::List(inner) => format!("List[{}]", checked_return_type_shape(inner)),
        Type::Map(key, inner) => {
            if matches!(key.as_ref(), Type::Str) {
                format!("Map[{}]", checked_return_type_shape(inner))
            } else {
                format!(
                    "Map[{}, {}]",
                    checked_return_type_shape(key),
                    checked_return_type_shape(inner)
                )
            }
        }
        Type::Stream(inner) => format!("Stream[{}]", checked_return_type_shape(inner)),
        Type::Optional(inner) => format!("Optional[{}]", checked_return_type_shape(inner)),
        Type::Result(ok, error) => format!(
            "Result[{}, {}]",
            checked_return_type_shape(ok),
            checked_return_type_shape(error)
        ),
        _ => ty.to_string(),
    }
}

fn inert_constant_initializer(arena: &AstArena, value: ExprId) -> bool {
    match arena.expr(value).kind {
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Regex(_) => true,
        ArenaExprKind::List(items) => arena.list_elements(items).all(|item| {
            item.splice_span.is_none() && inert_constant_initializer(arena, item.value)
        }),
        ArenaExprKind::Record(fields) => {
            arena
                .record_fields(fields)
                .iter()
                .all(|field| match field.kind {
                    ArenaRecordFieldKind::Named { value, .. } => {
                        inert_constant_initializer(arena, value)
                    }
                    _ => false,
                })
        }
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

/// An unbraced match arm ends at a comma, which `assert` would read as its
/// message separator, so such an arm becomes a braced block.
fn assert_statement(condition: &str, comma_terminated: bool) -> String {
    if comma_terminated {
        format!("{{ assert {condition} }}")
    } else {
        format!("assert {condition}")
    }
}

/// Whether the statement whose expression ends at `end` is terminated by a
/// comma (an unbraced match arm).
fn comma_terminated(source: &str, end: usize) -> bool {
    source
        .get(end..)
        .is_some_and(|rest| rest.trim_start_matches([' ', '\t']).starts_with(','))
}
