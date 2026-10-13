#![allow(clippy::single_call_fn)]

use super::{
    BTreeMap, Checker, Diagnostic, ErrorFamilyInfo, FxHashMap, FxHashSet, Name, Span, TagVariantInfo, Type,
};
use crate::sema::types::CallableParamType;
use crate::symbol::QualifiedName;
use crate::syntax::arena::{
    ArenaExprKind, ArenaProgram, ArenaStmtKind, BlockId, ExprId, StmtId, TypeExprId,
};
use crate::syntax::node::Effect;

#[derive(Clone, Debug, Default)]
pub struct CompactDeclOutput {
    // Namespace and source identity distinguish equal offsets in imported units.
    pub stream_stage_types:
        BTreeMap<(Option<Name>, crate::source::Span), super::CheckedStreamStage>,
    pub static_callable_aliases: BTreeMap<crate::source::Span, super::StaticCallableAlias>,
    pub local_binding_types: BTreeMap<crate::source::Span, Type>,
    pub record_constructors: super::RecordConstructors,
    pub bodies: CompactBodyFacts,
    pub prepared_constants: crate::sema::constants::PreparedConstants,
    pub wire_enums: crate::sema::wire_enums::PreparedWireEnums,
    /// The signature CLI entries of the entry script: one bare `cli main`,
    /// or one entry per subcommand path.
    pub(crate) cli_entries: Vec<crate::sema::cli_entry::CliEntryPlan>,
    pub diagnostics: Vec<Diagnostic>,
    pub function_return_types: BTreeMap<crate::source::Span, Type>,
    /// Full checker signatures are keyed by body span, whose source ID keeps
    /// independently parsed module arenas distinct when their local IDs match.
    pub function_signatures: BTreeMap<Span, CompactFunctionSig>,
    pub function_effect_facts: BTreeMap<super::EffectDeclarationId, super::FunctionEffectFact>,
    pub parameter_types: BTreeMap<crate::source::Span, Type>,
    pub types: FxHashMap<Name, CompactTypeDefInfo>,
    pub record_schema_fields: FxHashMap<Name, BTreeMap<Name, TypeExprId>>,
    pub tag_variants_by_name: FxHashMap<Name, TagVariantInfo>,
    pub qualified_tag_variants: FxHashMap<QualifiedName, TagVariantInfo>,
    pub error_families_by_name: FxHashMap<Name, ErrorFamilyInfo>,
    pub qualified_error_families: FxHashMap<QualifiedName, ErrorFamilyInfo>,
    pub procs: FxHashMap<Name, CompactFunctionSig>,
    pub pures: FxHashMap<Name, CompactFunctionSig>,
    pub streams: FxHashMap<Name, CompactFunctionSig>,
    pub qualified_procs: FxHashMap<QualifiedName, CompactFunctionSig>,
    pub qualified_pures: FxHashMap<QualifiedName, CompactFunctionSig>,
    pub qualified_streams: FxHashMap<QualifiedName, CompactFunctionSig>,
    pub type_defs: usize,
    pub tag_variants: usize,
    pub error_families: usize,
    pub error_variants: usize,
    pub error_fields: usize,
    pub function_defs: usize,
    pub params: usize,
    pub schema_fields: usize,
    pub module_contract_entries: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompactTypeDefInfo {
    Alias(TypeExprId),
    /// An alias of an integer type between the bounds its declaration
    /// writes.
    Bounded(TypeExprId, crate::sema::validated::IntRange),
    Record(BTreeMap<Name, Type>),
    Module(std::sync::Arc<crate::sema::types::ModuleType>),
    TagUnion,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactFunctionSig {
    pub params: Vec<CallableParamType>,
    pub parameter_schemas: Vec<Option<crate::sema::constants::SchemaExpectation>>,
    pub return_ty: Type,
    pub return_type_expr: TypeExprId,
    pub inferred_effects: bool,
    pub effects: Option<Vec<Effect>>,
}

/// Body facts lowering consumes, keyed by arena identity. They are the full
/// checker's published facts, so lowering never re-checks a body.
#[derive(Clone, Debug, Default)]
pub struct CompactBodyFacts {
    pub resolved_type_expressions: super::ResolvedTypeExpressions<ExprId>,
    pub resolved_bindings: std::sync::Arc<super::ResolvedBindings>,
    pub expr_types: FxHashMap<ExprId, Type>,
    pub projections: FxHashSet<ExprId>,
    /// Optional receivers whose checked presence proof makes their fallback unreachable.
    pub proven_nonnull_fallback_receivers: FxHashSet<ExprId>,
    /// Condition operands whose `Result[Bool]` propagates its failure; each
    /// lowers as the same expression under a `?`.
    pub propagating_conditions: FxHashSet<ExprId>,
    pub requirement_targets: FxHashMap<ExprId, super::RequirementTarget>,
    /// Registered method and module function calls keyed by call expression;
    /// a stage callable's plan is keyed by its callee.
    pub api_calls: FxHashMap<ExprId, super::CheckedApiCall>,
    pub module_commands: BTreeMap<Span, super::CheckedModuleCommand>,
    /// User callable calls and structured stages keyed by their source span.
    pub argument_bindings: BTreeMap<Span, super::CheckedArguments>,
    pub statement_positions: FxHashMap<StmtId, super::StatementPosition>,
    /// `with` handler slots retain the common nominal error of their checked inputs.
    pub handler_input_types: FxHashMap<BlockId, Type>,
    /// Leading-dot variants keyed by the constructing call or member expression.
    pub inferred_variants: FxHashMap<ExprId, super::InferredVariant>,
    /// String literals whose expected type made them a `Path`.
    pub path_literals: FxHashSet<ExprId>,
    /// The schema field each record constructor argument supplies, keyed by call.
    pub record_constructor_fields: FxHashMap<ExprId, Vec<Name>>,
    /// What each `.Name` pattern stands for.
    pub inferred_variant_patterns:
        FxHashMap<crate::syntax::arena::PatternId, super::InferredVariantPattern>,
    /// The argument binding of each error constructor, keyed by call.
    pub error_constructors: FxHashMap<ExprId, super::CheckedErrorConstructor>,
    /// `if let` and `while let` conditions that bind the non-null value of an
    /// optional subject: `null` fails the condition before the pattern runs.
    pub optional_binding_conditions: FxHashSet<ExprId>,
    /// `guard let` statements whose subject is an optional, not a `Result`.
    pub optional_binding_guards: FxHashSet<StmtId>,
    /// Each `yield` that appends to a `collect` expression, with that
    /// expression. A `yield` that is not here suspends a producer.
    pub collect_yields: FxHashMap<StmtId, ExprId>,
    /// Calls through a value of a callable type, keyed by call, with the
    /// signature each was checked against.
    pub typed_callable_calls: FxHashMap<ExprId, std::sync::Arc<crate::sema::types::TypedCallable>>,
    /// List index expressions that count from the end, with the distance
    /// their negative literal index names.
    pub from_end_indexes: FxHashMap<ExprId, u32>,
    /// The operation each `value as TARGET` expression performs.
    pub conversions: FxHashMap<ExprId, super::Conversion>,
    /// Resolved built-in `range` call expressions whose bounds can establish
    /// iteration cardinality.
    pub builtin_range_calls: FxHashSet<ExprId>,
    /// Checked sites where failure can leave the current body or error scope.
    pub propagating_error_spans: std::collections::BTreeSet<Span>,
    /// The resource kind of each binding of a managed `with` scope.
    pub resource_scopes: FxHashMap<ExprId, Vec<crate::modules::ManagedResource>>,
    /// Explicit checked ownership transfers keyed by statement or block in
    /// this same arena. Missing entries are unpublished facts.
    pub resource_transfers: super::ResourceTransferFacts,
    /// Each text pattern as the checker compiled it.
    pub text_patterns: FxHashMap<crate::syntax::arena::PatternId, super::TextPattern>,
}

impl CompactBodyFacts {
    pub fn resource_transfer(&self, site: super::ResourceTransferSite) -> Option<bool> {
        self.resource_transfers.get(&site).copied()
    }

    fn collect(
        program: &ArenaProgram,
        checked: &super::CheckOutput,
    ) -> Self {
        let arena = &program.arena;
        let mut facts = Self {
            resolved_type_expressions: super::ResolvedTypeExpressions {
                annotations: checked.resolved_type_expressions.annotations.clone(),
                templates: checked.resolved_type_expressions.templates.clone(),
                instantiations: FxHashMap::default(),
            },
            resolved_bindings: checked.resolved_bindings.clone(),
            argument_bindings: checked.argument_bindings.clone(),
            module_commands: checked.module_commands.clone(),
            resource_transfers: checked.resource_transfers.clone(),
            propagating_error_spans: checked.propagating_error_spans.clone(),
            ..Self::default()
        };
        facts.expr_types.reserve(checked.expr_types.len());
        for index in 0..arena.expr_tags.len() {
            let id = ExprId::from_index(index);
            let expression = arena.expr(id);
            let span = expression.span;
            if checked.builtin_range_calls.contains(&span) {
                facts.builtin_range_calls.insert(id);
            }
            if let Some(ty) = checked.expr_types.get(&span) {
                facts.expr_types.insert(id, ty.clone());
            }
            if let Some(target) = checked.requirement_targets.get(&span) {
                facts.requirement_targets.insert(id, target.clone());
            }
            if checked.projections.contains_key(&span) {
                facts.projections.insert(id);
            }
            if checked.proven_nonnull_fallback_receivers.contains(&span) {
                facts.proven_nonnull_fallback_receivers.insert(id);
            }
            if checked.propagating_conditions.contains(&span) {
                facts.propagating_conditions.insert(id);
            }
            if let Some(call) = checked.api_calls.get(&span) {
                facts.api_calls.insert(id, call.clone());
            }
            if matches!(expression.kind, ArenaExprKind::Call { .. })
                && let Some(instance) = checked.resolved_type_expressions.instantiations.get(&span) {
                facts.resolved_type_expressions.instantiations.insert(id, instance.clone());
            }
            if matches!(expression.kind, ArenaExprKind::Call { .. })
                && let Some(callable) = checked.typed_callable_calls.get(&span)
            {
                facts.typed_callable_calls.insert(id, callable.clone());
            }
            if let Some(variant) = checked.inferred_variants.get(&span) {
                facts.inferred_variants.insert(id, variant.clone());
            }
            if matches!(expression.kind, ArenaExprKind::Str(_))
                && checked.path_literals.contains(&span)
            {
                facts.path_literals.insert(id);
            }
            if let Some(fields) = checked.record_constructor_fields.get(&span) {
                facts.record_constructor_fields.insert(id, fields.clone());
            }
            if let Some(constructor) = checked.error_constructors.get(&span) {
                facts.error_constructors.insert(id, constructor.clone());
            }
            if matches!(expression.kind, ArenaExprKind::PatternCondition { .. })
                && checked.optional_binding_spans.contains(&span)
            {
                facts.optional_binding_conditions.insert(id);
            }
            if matches!(expression.kind, ArenaExprKind::Index { .. })
                && let Some(distance) = checked.from_end_indexes.get(&span)
            {
                facts.from_end_indexes.insert(id, *distance);
            }
            if matches!(expression.kind, ArenaExprKind::Convert { .. })
                && let Some(conversion) = checked.conversions.get(&span)
            {
                facts.conversions.insert(id, *conversion);
            }
            if matches!(expression.kind, ArenaExprKind::ResourceScope { .. })
                && let Some(kinds) = checked.resource_scopes.get(&span)
            {
                facts.resource_scopes.insert(id, kinds.clone());
            }
        }
        if !checked.inferred_variant_patterns.is_empty() {
            for (index, pattern) in arena.patterns.iter().enumerate() {
                if pattern.kind.is_inferred_variant()
                    && let Some(resolved) = checked
                        .inferred_variant_patterns
                        .get(&arena.span(pattern.span))
                {
                    facts.inferred_variant_patterns.insert(
                        crate::syntax::arena::PatternId::from_index(index),
                        resolved.clone(),
                    );
                }
            }
        }
        if !checked.text_patterns.is_empty() {
            for (index, pattern) in arena.patterns.iter().enumerate() {
                if matches!(
                    pattern.kind,
                    crate::syntax::arena::ArenaPatternKind::Text(_)
                ) && let Some(compiled) = checked.text_patterns.get(&arena.span(pattern.span))
                {
                    facts.text_patterns.insert(
                        crate::syntax::arena::PatternId::from_index(index),
                        compiled.clone(),
                    );
                }
            }
        }
        // The checker names a `collect` expression by its span.
        let collects: FxHashMap<Span, ExprId> = if checked.collect_yields.is_empty() {
            FxHashMap::default()
        } else {
            (0..arena.expr_tags.len())
                .map(ExprId::from_index)
                .filter(|id| matches!(arena.expr(*id).kind, ArenaExprKind::Collect { .. }))
                .map(|id| (arena.expr(id).span, id))
                .collect()
        };
        for index in 0..arena.stmt_tags.len() {
            let id = StmtId::from_index(index);
            let statement = arena.stmt(id);
            if let Some(position) = checked.statement_positions.get(&statement.span) {
                facts.statement_positions.insert(id, *position);
            }
            if matches!(statement.kind, ArenaStmtKind::Guard { .. })
                && checked.optional_binding_spans.contains(&statement.span)
            {
                facts.optional_binding_guards.insert(id);
            }
            if matches!(
                statement.kind,
                ArenaStmtKind::Yield(_) | ArenaStmtKind::YieldDelegate(_)
            ) && let Some(collect) = checked.collect_yields.get(&statement.span)
                && let Some(collect) = collects.get(collect)
            {
                facts.collect_yields.insert(id, *collect);
            }
        }
        if !checked.handler_input_types.is_empty() {
            for index in 0..arena.blocks.len() {
                let id = BlockId::from_index(index);
                if let Some(ty) = checked
                    .handler_input_types
                    .get(&arena.span(arena.block(id).span))
                {
                    facts.handler_input_types.insert(id, ty.clone());
                }
            }
        }
        facts
    }
}

impl Checker {
    /// Check `program` for lowering when no entry check has run: embedded
    /// modules, directly prepared programs, and tests. A caller that already
    /// checked the program with `CheckOptions::embedded_bodies` passes that
    /// output to `compact_declarations` instead, so each body is checked once.
    pub fn check_compact_declarations(
        program: &ArenaProgram,
        sources: &crate::source::SourceMap,
        entry_source_id: crate::source::SourceId,
    ) -> CompactDeclOutput {
        let options = super::CheckOptions::for_execution();
        let checked = Checker::check_arena_with_sources_and_options(
            program,
            sources,
            entry_source_id,
            options,
        );
        Self::compact_declarations(program, checked)
    }

    /// The declarations and body facts lowering consumes, re-keyed from the
    /// check that produced the program's diagnostics. Compact execution passes
    /// the same checked boundaries as source checking because it is the same
    /// check.
    pub fn compact_declarations(
        program: &ArenaProgram,
        checked: super::CheckOutput,
    ) -> CompactDeclOutput {
        assert!(
            checked.embedded_bodies_checked,
            "lowering needs facts for embedded implementation bodies"
        );
        program.symbol_owner().with_current(|| {
            let bodies = CompactBodyFacts::collect(program, &checked);
            let mut output = checked.checked_declarations.into_compact();
            output.bodies = bodies;
            output.function_effect_facts = checked.function_effect_facts;
            output.stream_stage_types = checked.stream_stage_types;
            output.static_callable_aliases = checked.static_callable_aliases;
            output.parameter_types = checked.parameter_types;
            output.local_binding_types = checked.local_binding_types;
            output.function_return_types = checked.function_return_types;
            output.prepared_constants = checked.prepared_constants;
            output.diagnostics = checked.diagnostics.into_iter()
                .filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect();
            output
        })
    }
}
