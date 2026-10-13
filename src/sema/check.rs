#![allow(clippy::single_call_fn)]

pub(crate) use crate::diagnostic::{Diagnostic, FixHint, Label};
use crate::diagnostic::{DiagnosticCode, Severity};
pub(crate) use crate::modules::{ApiArgCheck, MethodReceiver, MethodSig, ModuleFnSig, api_spec};
use crate::runtime::signal::{normalize_hook_signal, signal_rejection_message};
pub(crate) use crate::sema::records::standard_record_type;
pub(crate) use crate::sema::types::{CallableParamType, CallableType, ModuleExportType, Type};
pub(crate) use crate::source::Span;
use crate::source::{SourceId, SourceMap};
pub(crate) use crate::symbol::{Name, QualifiedName};
use crate::syntax::arena::{ArenaCallArgKind, ArenaExprKind, ArenaProgram, ArenaRange, ArenaStmtKind, ArenaUserModule, BlockId, ExprId, StmtId, TypeExprId};
pub(crate) use crate::syntax::node::{BinaryOp, CoreCommand, Effect, RunKind, UnaryOp};

use callable_alias::CallableAlias;
pub use callable_alias::StaticCallableAlias;
pub(crate) use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Borrowed reachable scopes keep preparation on the original stable arena IDs.
pub struct CheckBundleView<'a> {
    pub(super) program: &'a ArenaProgram,
    pub(super) sources: Option<&'a SourceMap>,
    pub(super) modules: Vec<&'a ArenaUserModule>,
    pub(super) root_statements: ArenaRange,
    pub(super) root_source_id: Option<SourceId>,
    source_ids: FxHashSet<SourceId>,
    all_sources: bool,
    pub(super) entry_context: Option<Arc<()>>,
}

impl<'a> CheckBundleView<'a> {
    pub(super) fn new(
        program: &'a ArenaProgram,
        sources: Option<&'a SourceMap>,
        entry_source_id: Option<SourceId>,
    ) -> Self {
        Self::for_root(program, program.statements, sources, entry_source_id)
    }

    /// Select an entry range and its resolved dependency closure from one immutable arena.
    pub fn for_root(
        program: &'a ArenaProgram,
        root_statements: ArenaRange,
        sources: Option<&'a SourceMap>,
        entry_source_id: Option<SourceId>,
    ) -> Self {
        let mut modules = Vec::new();
        let mut visited = FxHashSet::default();
        for module in program.modules.iter().filter(|module| module.internal) {
            Self::order_module(
                program,
                &module.key,
                root_statements,
                &mut visited,
                &mut modules,
            );
        }
        let mut roots = Vec::new();
        let mut root_imports = FxHashSet::default();
        Self::enqueue_imports(
            program,
            program.arena.stmt_ids(root_statements),
            &mut root_imports,
            &mut roots,
        );
        roots.sort_unstable();
        for key in roots {
            Self::order_module(
                program,
                &key,
                root_statements,
                &mut visited,
                &mut modules,
            );
        }
        let source_ids = entry_source_id.into_iter()
            .chain(program.module_doc_for(root_statements).map(|span| span.source_id))
            .chain(program.arena.stmt_ids(root_statements).map(|id| program.arena.stmt(id).span.source_id))
            .chain(modules.iter().flat_map(|module| program.module_statements(module)
                .map(|id| program.arena.stmt(id).span.source_id)))
            .chain(modules.iter().filter_map(|module| program.module_doc_for(module.statements)
                .map(|span| span.source_id)))
            .collect();
        Self {
            program,
            sources,
            modules,
            root_statements,
            root_source_id: entry_source_id.or_else(|| {
                program
                    .arena
                    .stmt_ids(root_statements)
                    .next()
                    .map(|statement| program.arena.stmt(statement).span.source_id)
                    .or_else(|| program.module_doc_for(root_statements).map(|span| span.source_id))
            }),
            source_ids,
            all_sources: false,
            entry_context: None,
        }
    }

    /// Select one source range for linting against checked facts from its entry.
    pub fn for_module(
        program: &'a ArenaProgram,
        root_statements: ArenaRange,
        sources: Option<&'a SourceMap>,
        entry_source_id: Option<SourceId>,
    ) -> Self {
        let source_ids = entry_source_id
            .into_iter()
            .chain(program.module_doc_for(root_statements).map(|span| span.source_id))
            .chain(program.arena.stmt_ids(root_statements).map(|id| program.arena.stmt(id).span.source_id))
            .collect();
        Self {
            program,
            sources,
            modules: Vec::new(),
            root_statements,
            root_source_id: entry_source_id.or_else(|| {
                program
                    .arena
                    .stmt_ids(root_statements)
                    .next()
                    .map(|statement| program.arena.stmt(statement).span.source_id)
                    .or_else(|| program.module_doc_for(root_statements).map(|span| span.source_id))
            }),
            source_ids,
            all_sources: false,
            entry_context: None,
        }
    }

    fn for_module_owner(
        program: &'a ArenaProgram,
        module: &'a ArenaUserModule,
        sources: Option<&'a SourceMap>,
        source_id: SourceId,
    ) -> Self {
        let source_ids = program
            .module_statements(module)
            .map(|statement| program.arena.stmt(statement).span.source_id)
            .chain(program.module_doc_for(module.statements).map(|span| span.source_id))
            .chain(std::iter::once(source_id))
            .collect();
        Self {
            program,
            sources,
            modules: vec![module],
            root_statements: ArenaRange::default(),
            root_source_id: Some(source_id),
            source_ids,
            all_sources: false,
            entry_context: None,
        }
    }

    pub(super) fn whole_program(program: &'a ArenaProgram) -> Self {
        Self {
            program,
            sources: None,
            modules: program.modules.iter().collect(),
            root_statements: program.statements,
            root_source_id: None,
            source_ids: FxHashSet::default(),
            all_sources: true,
            entry_context: None,
        }
    }

    fn order_module(
        program: &'a ArenaProgram,
        key: &str,
        root_statements: ArenaRange,
        visited: &mut FxHashSet<Arc<str>>,
        ordered: &mut Vec<&'a ArenaUserModule>,
    ) {
        let Some(module) = program
            .modules
            .iter()
            .find(|module| module.key == key && module.statements != root_statements)
        else {
            return;
        };
        let module_key: Arc<str> = Arc::from(module.key.as_str());
        if !visited.insert(module_key) {
            return;
        }
        let mut dependencies = Vec::new();
        Self::enqueue_imports(
            program,
            program.module_statements(module),
            &mut FxHashSet::default(),
            &mut dependencies,
        );
        dependencies.sort_unstable();
        for dependency in dependencies {
            Self::order_module(
                program,
                &dependency,
                root_statements,
                visited,
                ordered,
            );
        }
        ordered.push(module);
    }

    fn enqueue_imports(
        program: &ArenaProgram,
        statements: impl IntoIterator<Item = StmtId>,
        reachable: &mut FxHashSet<Arc<str>>,
        pending: &mut Vec<Arc<str>>,
    ) {
        for statement in statements {
            let ArenaStmtKind::Use(use_id) = program.arena.stmt(statement).kind else {
                continue;
            };
            if let Some(key) = program.arena.use_stmt(use_id).resolved.as_ref()
                && reachable.insert(key.clone())
            {
                pending.push(key.clone());
            }
        }
    }

    pub(super) fn contains_source(&self, source_id: SourceId) -> bool {
        self.all_sources || self.source_ids.contains(&source_id)
    }

    /// The expressions whose span lies in a source of this view, in table
    /// order. A pass over one module's rows must not cost the whole shared
    /// arena, so this reads the arena's per-source index.
    pub(super) fn expr_ids(&self) -> Vec<ExprId> {
        self.view_ids(self.program.arena.expr_tags.len(), ExprId::from_index, |rows, source| {
            rows.expr_ids(source).collect()
        })
    }

    /// The statements whose span lies in a source of this view, in table order.
    pub(super) fn stmt_ids(&self) -> Vec<StmtId> {
        self.view_ids(self.program.arena.stmt_tags.len(), StmtId::from_index, |rows, source| {
            rows.stmt_ids(source).collect()
        })
    }

    /// The blocks whose span lies in a source of this view, in table order.
    pub(super) fn block_ids(&self) -> Vec<BlockId> {
        self.view_ids(self.program.arena.blocks.len(), BlockId::from_index, |rows, source| {
            rows.block_ids(source).collect()
        })
    }

    /// Indices into the record field table of the fields that carry a span
    /// in a source of this view.
    pub(super) fn record_field_indices(&self) -> Vec<usize> {
        if self.all_sources {
            return (0..self.program.arena.record_fields.len()).collect();
        }
        let rows = self.program.arena.source_rows();
        let mut indices = self
            .source_ids
            .iter()
            .flat_map(|&source| rows.record_field_indices(source))
            .collect::<Vec<_>>();
        indices.sort_unstable();
        indices
    }

    fn view_ids<I: Ord>(
        &self,
        table_len: usize,
        from_index: fn(usize) -> I,
        in_source: impl Fn(&crate::syntax::arena::SourceRows, SourceId) -> Vec<I>,
    ) -> Vec<I> {
        if self.all_sources {
            return (0..table_len).map(from_index).collect();
        }
        let rows = self.program.arena.source_rows();
        let mut ids = self
            .source_ids
            .iter()
            .flat_map(|&source| in_source(&rows, source))
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids
    }

    /// Return the immutable arena that owns every statement and module ID in this view.
    pub fn program(&self) -> &'a ArenaProgram {
        self.program
    }

    /// Return the entry statement range selected by this view.
    pub fn root_statements(&self) -> ArenaRange {
        self.root_statements
    }

    pub(super) fn root_source_id(&self) -> Option<SourceId> {
        self.root_source_id
    }

    /// Iterate the entry's top-level statements.
    pub fn statement_ids(&self) -> impl Iterator<Item = StmtId> + '_ {
        self.program.arena.stmt_ids(self.root_statements)
    }

    /// Iterate reachable user modules in dependency order and embedded modules.
    pub fn modules(&self) -> impl Iterator<Item = &'a ArenaUserModule> + '_ {
        self.modules.iter().copied()
    }
}

#[path = "check/args.rs"]
mod args;
#[path = "check/builder.rs"]
mod builder;
#[path = "check/cache_trace.rs"]
mod cache_trace;
use cache_trace::CacheTrace;
#[path = "check/call.rs"]
mod call;
#[path = "check/callable_alias.rs"]
mod callable_alias;
mod collect;
mod checked_declarations;
pub use checked_declarations::{CheckedDeclarations, CheckedErrorDeclaration, CheckedFunctionDeclaration,
    CheckedFunctionKind, CheckedTypeDeclaration, InstantiatedTypeExpressions, ResolvedTypeExpressions};
mod scopecheck;
pub use scopecheck::{BindingDeclaration, BindingId, BindingKind, ResolvedBindings};
#[path = "check/command.rs"]
mod command;
#[path = "check/compact.rs"]
mod compact;
#[path = "check/decl.rs"]
mod decl;
#[path = "check/expected.rs"]
mod expected;
#[path = "check/expr.rs"]
mod expr;
mod record_require;
pub use expected::RequirementTarget;
pub use record_require::RecordRequireMigration;
#[path = "check/removed_fs.rs"]
mod removed_fs;
pub use removed_fs::call_receiver_text;
#[path = "check/effect_bounds.rs"]
mod effect_bounds;
#[path = "check/infer_effects.rs"]
mod infer_effects;
#[path = "check/infer_param.rs"]
mod infer_param;
#[path = "check/infer_return.rs"]
mod infer_return;
#[path = "check/inferred_variant.rs"]
mod inferred_variant;
#[path = "check/local_inference.rs"]
mod local_inference;
#[path = "check/module_driver.rs"]
mod module_driver;
pub use module_driver::CheckSession;
#[path = "check/module_record.rs"]
mod module_record;
#[path = "check/method.rs"]
mod method;
pub(crate) use method::nearest_name;
#[path = "check/conversion.rs"]
mod conversion;
#[path = "check/path_literal.rs"]
mod path_literal;
#[path = "check/pattern.rs"]
mod pattern;
#[path = "check/proof.rs"]
mod proof;
mod public_result;
#[path = "check/resource.rs"]
mod resource;
pub use resource::{ResourceTransferFacts, ResourceTransferSite};
#[path = "check/set.rs"]
mod set;
#[path = "check/stmt.rs"]
mod stmt;
#[path = "check/stream.rs"]
mod stream;
#[path = "check/typed_callable.rs"]
mod typed_callable;
#[path = "check/types.rs"]
mod types;
#[path = "check/validated.rs"]
mod validated;
pub use conversion::Conversion;
#[path = "check/text_pattern.rs"]
mod text_pattern;
pub use text_pattern::{TextHoleKind, TextHoleValue, TextPattern, split_text};

use self::args::{
    call_arg_expr_id_arena, call_arg_span_arena, common_module_overload_expected_arena,
    module_overload_matches_arena, module_sig_accepts_arg_name_at_arena, module_sig_accepts_arity,
    module_sig_accepts_names_arena,
};
use self::command::{
    command_arg_can_be_path_like_arena, command_bool_flag_name_arena,
    command_stmt_asserts_success_arena, command_ty_auto_propagates,
};
pub use super::constants::RecordConstructors;
pub use super::projection::{CheckedProjection, ProjectionOperation};

pub use self::infer_effects::{EffectDeclarationId, FunctionEffectFact};
use self::infer_effects::{DeferredEffectCheck, EffectGraph, EffectSummary};
use self::module_record::{ArgsDomain, ModuleRecordCache, ModuleRecords};

pub use self::compact::{
    CompactBodyFacts, CompactDeclOutput, CompactFunctionSig, CompactTypeDefInfo,
};
use self::expr::expr_ty_auto_propagates;
use self::stmt::block_has_exit_point_arena;
use self::types::{collection_item_ty, result_types, tail_type_matches_expected};

/// The checked purpose of a statement remains fixed when its value is unused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatementPosition {
    Statement,
    Value,
}

/// A registered method or module function call's selected overload: its
/// registry signature, concrete parameter types, and each source argument
/// entry's parameter slot. Module functions have no receiver.
#[derive(Clone, Debug)]
pub struct CheckedApiCall {
    pub receiver: Option<MethodReceiver>,
    pub sig: &'static ModuleFnSig,
    pub params: Vec<Type>,
    pub argument_slots: Vec<usize>,
}

/// A command retains its selected API and source argument treatment; only
/// literal boolean flags change into a value without evaluating an operand.
#[derive(Clone, Debug)]
pub struct CheckedModuleCommand {
    pub call: CheckedApiCall,
    pub arguments: Vec<CheckedCommandArgument>,
}

#[derive(Clone, Debug)]
pub enum CheckedCommandArgument {
    BooleanFlag,
    Value,
    Word(Type),
}

/// A user callable call's or structured stage's source argument entries bound
/// to parameter slots. A stage's `callable_entry` is its named callable
/// descriptor; its slots describe the remaining configuration entries.
#[derive(Clone, Debug, Default)]
pub struct CheckedArguments {
    pub callable_entry: Option<usize>,
    pub argument_slots: Vec<usize>,
}

/// A stage retains its checked input context and output contract independently
/// of the enclosing pipeline's final result.
#[derive(Clone, Debug)]
pub struct CheckedStreamStage {
    pub input: Type,
    pub output: Type,
}

#[derive(Clone, Debug, Default)]
pub struct CheckOutput {
    pub checked_declarations: CheckedDeclarations,
    pub resolved_type_expressions: ResolvedTypeExpressions,
    pub resolved_bindings: Arc<ResolvedBindings>,
    pub static_callable_aliases: BTreeMap<Span, StaticCallableAlias>,
    pub local_binding_types: BTreeMap<Span, Type>,
    pub prepared_constants: super::constants::PreparedConstants,
    /// Optional receivers whose checked presence proof makes their fallback unreachable.
    pub proven_nonnull_fallback_receivers: BTreeSet<Span>,
    pub diagnostics: Vec<Diagnostic>,
    pub annotation_facts: Vec<AnnotationFact>,
    pub function_return_types: BTreeMap<Span, Type>,
    pub record_constructor_instances: BTreeMap<Span, super::constants::CheckedRecordConstructor>,
    pub parameter_types: BTreeMap<Span, Type>,
    pub requirement_targets: BTreeMap<Span, RequirementTarget>,
    pub requirement_expected_targets: BTreeMap<Span, RequirementTarget>,
    pub reveal_types: Vec<Diagnostic>,
    pub expr_types: BTreeMap<Span, Type>,
    pub stream_stage_types: BTreeMap<(Option<Name>, Span), CheckedStreamStage>,
    pub projections: BTreeMap<Span, CheckedProjection>,
    pub statement_positions: BTreeMap<Span, StatementPosition>,
    pub callable_effects: FxHashMap<String, Option<Vec<Effect>>>,
    pub function_effect_facts: BTreeMap<EffectDeclarationId, FunctionEffectFact>,
    pub assertion_effect_spans: BTreeSet<Span>,
    pub statement_expression_spans: BTreeSet<Span>,
    /// Expression statements whose `Result[Unit]` value propagates its failure
    /// instead of becoming a body's value: every statement outside a tail, a
    /// tail whose body yields `Unit`, and the direct tail of a `Result[Unit]`
    /// function.
    pub propagating_statements: BTreeSet<Span>,
    /// `let _ = VALUE` statements whose value comes from a call registered as
    /// discardable, where `VALUE` alone would be a statement that drops it.
    /// A binding at a tail that is read as its block's value is not here.
    pub discardable_bindings: BTreeSet<Span>,
    /// Expressions in a control position of a condition whose `Result[Bool]`
    /// propagates its failure and leaves the `Bool`: the condition itself, or
    /// an operand of `!`, `and`, or `or` that is in a control position.
    /// Lowering evaluates each as it evaluates the expression under a `?`.
    pub propagating_conditions: BTreeSet<Span>,
    /// `expr?` in a control position of a condition where `expr` is a
    /// `Result[Bool]`: the position propagates without the `?`.
    pub redundant_condition_propagations: BTreeSet<Span>,
    /// Value-position run forms whose `Result` is the value and that are not
    /// written under `try`, keyed by the run form's span.
    /// Spliced `run` targets whose type is a list without a validation, by
    /// the splice's span: the command vectors that may be empty.
    pub unvalidated_command_vectors: BTreeMap<Span, Type>,
    pub membership_migration_spans: BTreeSet<Span>,
    pub standard_call_spans: BTreeMap<Span, (String, String)>,
    pub statically_resolved_call_spans: BTreeSet<Span>,
    pub api_calls: BTreeMap<Span, CheckedApiCall>,
    pub module_commands: BTreeMap<Span, CheckedModuleCommand>,
    /// Keyed by call expression or stage span.
    pub argument_bindings: BTreeMap<Span, CheckedArguments>,
    /// Ordinary blocks whose checked paths cannot reach their enclosing continuation.
    pub definitely_exiting_block_spans: BTreeSet<Span>,
    /// `with` error handlers retain the common nominal error of their checked inputs.
    pub handler_input_types: BTreeMap<Span, Type>,
    /// Leading-dot variants keyed by the constructing expression: the call for
    /// `.Name(args)`, the member expression for a bare `.Name`.
    pub inferred_variants: BTreeMap<Span, InferredVariant>,
    /// String literals that took the `Path` type from their expected type;
    /// lowering builds a `Path` constant for each.
    pub path_literals: BTreeSet<Span>,
    /// Qualified variant constructors whose expected type selects the same
    /// variant, keyed by expression, with the qualifier span a leading dot replaces.
    pub redundant_variant_qualifiers: BTreeMap<Span, Span>,
    /// The schema field each record constructor argument supplies, in source
    /// order, keyed by call expression.
    pub record_constructor_fields: BTreeMap<Span, Vec<Name>>,
    /// Constructor calls of error variants whose declared payload is exactly
    /// `message: Str`, the field a variant without a payload already carries,
    /// keyed by call expression.
    pub message_payload_constructors: BTreeMap<Span, MessagePayloadConstructor>,
    /// Removed `record.require` calls in imported modules that the named
    /// schema's `.require` replaces unchanged, keyed by call expression. The
    /// checker holds only the root file's text, so whoever holds the module's
    /// text builds the edit from this.
    pub record_require_migrations: BTreeMap<Span, RecordRequireMigration>,
    /// What each `.Name` pattern stands for, keyed by pattern.
    pub inferred_variant_patterns: BTreeMap<Span, InferredVariantPattern>,
    /// How each well-formed error constructor call binds its arguments, keyed
    /// by call expression.
    pub error_constructors: BTreeMap<Span, CheckedErrorConstructor>,
    /// Conditional bindings whose subject is an optional: the `guard let`
    /// statement, or the `let pattern = subject` condition of an `if` or
    /// `while`. A `null` subject takes the failure path and any other value is
    /// bound; lowering reads this instead of deciding from the subject's type.
    pub optional_binding_spans: BTreeSet<Span>,
    /// Each `yield` that appends to a `collect` expression, keyed by the
    /// `yield` statement, with the span of that expression. Lowering reads
    /// the target here instead of working out which block a yield is in.
    pub collect_yields: BTreeMap<Span, Span>,
    /// Calls whose callee is a value of a callable type, keyed by call
    /// expression, with the signature the call was checked against. Lowering
    /// reads the signature here instead of working out what the callee is.
    pub typed_callable_calls: BTreeMap<Span, Arc<crate::sema::types::TypedCallable>>,
    /// List index expressions whose index is a negative integer literal,
    /// with the distance from the end it names: `items[-2]` is 2. Only these
    /// count from the end; lowering reads this instead of looking at the
    /// index expression again.
    pub from_end_indexes: BTreeMap<Span, u32>,
    /// The operation each `value as TARGET` expression performs, selected
    /// from the conversion table. Lowering builds the operation from this
    /// and never looks at the operand's type or the target again.
    pub conversions: BTreeMap<Span, Conversion>,
    /// The resource kind of each binding of a managed `with` scope, in binding
    /// order, keyed by the scope. A scope with a rejected binding has none.
    pub resource_scopes: BTreeMap<Span, Vec<crate::modules::ManagedResource>>,
    /// Whether each checked value sink can transfer a host resource into a
    /// different runtime ownership scope. Missing entries indicate an
    /// unpublished sink fact, not a scalar value.
    pub resource_transfers: ResourceTransferFacts,
    /// Each text pattern, compiled: its literal segments and what each hole
    /// makes of the text it matches. Lowering copies this into the program.
    pub text_patterns: BTreeMap<Span, TextPattern>,
    /// Embedded implementation bodies were checked, so every body lowering
    /// builds has published facts.
    pub embedded_bodies_checked: bool,
}

impl CheckOutput {
    pub fn resource_transfer(&self, site: ResourceTransferSite) -> Option<bool> {
        self.resource_transfers.get(&site).copied()
    }

    pub(super) fn merge_module_facts(&mut self, mut module: Self) {
        // Each checked module owns command-word bindings resolved from its body.
        // The entry record already owns aggregate constructor facts; module records
        // also carry dependency views that would publish their instances repeatedly.
        self.prepared_constants
            .merge_cli_plan_facts(&module.prepared_constants);
        self.resolved_bindings = Arc::new(
            self.resolved_bindings
                .merge_visible(&module.resolved_bindings),
        );
        self.checked_declarations
            .types
            .extend(module.checked_declarations.types);
        self.checked_declarations
            .errors
            .extend(module.checked_declarations.errors);
        self.checked_declarations
            .functions
            .extend(module.checked_declarations.functions);
        self.resolved_type_expressions
            .annotations
            .extend(module.resolved_type_expressions.annotations);
        self.resolved_type_expressions
            .templates
            .extend(module.resolved_type_expressions.templates);
        self.resolved_type_expressions
            .instantiations
            .extend(module.resolved_type_expressions.instantiations);
        self.static_callable_aliases
            .append(&mut module.static_callable_aliases);
        self.local_binding_types
            .append(&mut module.local_binding_types);
        self.proven_nonnull_fallback_receivers
            .append(&mut module.proven_nonnull_fallback_receivers);
        self.diagnostics.append(&mut module.diagnostics);
        self.annotation_facts.append(&mut module.annotation_facts);
        self.function_return_types
            .append(&mut module.function_return_types);
        self.record_constructor_instances
            .append(&mut module.record_constructor_instances);
        self.parameter_types.append(&mut module.parameter_types);
        self.requirement_targets
            .append(&mut module.requirement_targets);
        self.requirement_expected_targets
            .append(&mut module.requirement_expected_targets);
        self.reveal_types.append(&mut module.reveal_types);
        self.expr_types.append(&mut module.expr_types);
        self.stream_stage_types
            .append(&mut module.stream_stage_types);
        self.projections.append(&mut module.projections);
        self.statement_positions
            .append(&mut module.statement_positions);
        for (identity, fact) in module.function_effect_facts {
            if let Some(existing) = self.function_effect_facts.get(&identity) {
                assert_eq!(
                    existing, &fact,
                    "function effect fact {identity:?} has conflicting module records"
                );
            } else {
                self.function_effect_facts.insert(identity, fact);
            }
        }
        self.assertion_effect_spans
            .append(&mut module.assertion_effect_spans);
        self.statement_expression_spans
            .append(&mut module.statement_expression_spans);
        self.propagating_statements
            .append(&mut module.propagating_statements);
        self.discardable_bindings
            .append(&mut module.discardable_bindings);
        self.propagating_conditions
            .append(&mut module.propagating_conditions);
        self.redundant_condition_propagations
            .append(&mut module.redundant_condition_propagations);
        self.unvalidated_command_vectors
            .append(&mut module.unvalidated_command_vectors);
        self.membership_migration_spans
            .append(&mut module.membership_migration_spans);
        self.standard_call_spans
            .append(&mut module.standard_call_spans);
        self.statically_resolved_call_spans
            .append(&mut module.statically_resolved_call_spans);
        self.api_calls.append(&mut module.api_calls);
        self.module_commands.append(&mut module.module_commands);
        self.argument_bindings.append(&mut module.argument_bindings);
        self.definitely_exiting_block_spans
            .append(&mut module.definitely_exiting_block_spans);
        self.handler_input_types
            .append(&mut module.handler_input_types);
        self.inferred_variants.append(&mut module.inferred_variants);
        self.path_literals.append(&mut module.path_literals);
        self.redundant_variant_qualifiers
            .append(&mut module.redundant_variant_qualifiers);
        self.record_constructor_fields
            .append(&mut module.record_constructor_fields);
        self.message_payload_constructors
            .append(&mut module.message_payload_constructors);
        self.record_require_migrations
            .append(&mut module.record_require_migrations);
        self.error_constructors
            .append(&mut module.error_constructors);
        self.optional_binding_spans
            .append(&mut module.optional_binding_spans);
        self.collect_yields.append(&mut module.collect_yields);
        self.typed_callable_calls
            .append(&mut module.typed_callable_calls);
        self.from_end_indexes.append(&mut module.from_end_indexes);
        self.conversions.append(&mut module.conversions);
        self.resource_scopes.append(&mut module.resource_scopes);
        self.resource_transfers.extend(module.resource_transfers);
        self.text_patterns.append(&mut module.text_patterns);
        self.inferred_variant_patterns
            .append(&mut module.inferred_variant_patterns);
    }
}

/// The qualified pattern a target-typed `.Name` pattern was resolved to from
/// the type of the value it matches, so later stages never resolve the bare
/// name again.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InferredVariantPattern {
    /// An enum variant, by a constructor spelling visible where the pattern
    /// is written: `Binary`, or `kinds.Binary` for an imported enum.
    Tag { constructor: Name },
    /// A variant of this error family, as `Type::ErrorFamily` names it.
    Error { family: Name },
}

impl InferredVariantPattern {
    /// The pattern the qualified spelling parses to, given the `.Name`
    /// pattern as parsed.
    pub fn qualify(
        &self,
        kind: &crate::syntax::arena::ArenaPatternKind,
    ) -> crate::syntax::arena::ArenaPatternKind {
        use crate::syntax::arena::ArenaPatternKind;
        match (self, kind) {
            (Self::Tag { constructor }, ArenaPatternKind::Constructor { arg, .. }) => {
                ArenaPatternKind::Constructor {
                    name: *constructor,
                    arg: *arg,
                }
            }
            (Self::Tag { constructor }, ArenaPatternKind::ErrorVariant { fields, .. }) => {
                match constructor.as_str().split_once('.') {
                    Some((namespace, variant)) => ArenaPatternKind::ErrorVariant {
                        family: Name::intern(namespace),
                        variant: Name::intern(variant),
                        fields: *fields,
                    },
                    None => ArenaPatternKind::Binding(*constructor),
                }
            }
            (
                Self::Error { family },
                ArenaPatternKind::ErrorVariant {
                    variant, fields, ..
                },
            ) => ArenaPatternKind::ErrorVariant {
                family: *family,
                variant: *variant,
                fields: *fields,
            },
            _ => kind.clone(),
        }
    }
}

/// The argument binding of one error constructor call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedErrorConstructor {
    /// The payload field each argument supplies, in source order.
    pub fields: Vec<Name>,
    /// The call omitted the message of a variant declared without a payload;
    /// the message is then the family and variant name.
    pub default_message: bool,
}

/// One constructor call of a variant declared as `Variant(message: Str)`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessagePayloadConstructor {
    /// The checked family name, as `Type::ErrorFamily` carries it.
    pub family: Name,
    pub variant: Name,
    /// The `message: value` (or `message:`) argument and its value, when the
    /// call names the field instead of passing it positionally.
    pub named_message: Option<(Span, Span)>,
}

/// A leading-dot variant resolved against its expected type. It carries the
/// declaration the equivalent qualified spelling names, so later stages never
/// resolve the bare name again.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InferredVariant {
    Tag {
        type_name: Name,
        variant: Name,
        field_types: Vec<Type>,
    },
    /// `family` is the checked family name, as `Type::ErrorFamily` carries it.
    Error { family: Name, variant: Name },
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CheckOptions {
    pub interactive_commands: Option<fn(&str) -> bool>,
    pub reveal_types: bool,
    /// Check every embedded implementation body. A check whose output feeds
    /// lowering needs their facts; other checks only need the bodies whose
    /// inferred returns shape a signature.
    pub embedded_bodies: bool,
}

impl CheckOptions {
    /// Publish the checked facts required to prepare a program for execution.
    pub fn for_execution() -> Self {
        Self {
            embedded_bodies: true,
            ..Self::default()
        }
    }

    /// Publish the facts and revealed types shown by `xsht check`.
    pub fn for_check_cli() -> Self {
        Self {
            reveal_types: true,
            embedded_bodies: true,
            ..Self::default()
        }
    }

    /// Check interactive command statements using the caller's command set.
    pub fn for_interactive_commands(interactive_commands: fn(&str) -> bool) -> Self {
        Self {
            interactive_commands: Some(interactive_commands),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnotationFact {
    pub kind: AnnotationFactKind,
    pub ty: Type,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnnotationFactKind {
    Binding {
        span: Span,
        initializer: Span,
        exported: bool,
    },
    DefaultedParam {
        span: Span,
        default: Span,
    },
    InferredPureReturn {
        body: Span,
    },
    ExportedProcReturn {
        body: Span,
    },
}

#[derive(Clone, Debug)]
pub(super) struct Binding {
    id: Option<BindingId>,
    recovery: bool,
    static_namespace: bool,
    callable_alias: Option<CallableAlias>,
    ty: Type,
    mutable: bool,
    pure_local_mutation: bool,
    unrefined_ty: Option<Type>,
    proof: proof::BindingProof,
    boolean_proof: Option<Arc<proof::ConditionNarrowings>>,
    schema_expectation: Option<super::constants::SchemaExpectation>,
}

impl Binding {
    fn new(ty: Type, mutable: bool) -> Self {
        Self {
            id: None,
            recovery: false,
            callable_alias: None,
            static_namespace: false,
            ty,
            mutable,
            pure_local_mutation: false,
            unrefined_ty: None,
            proof: proof::BindingProof::default(),
            boolean_proof: None,
            schema_expectation: None,
        }
    }

    fn pure_local_var(ty: Type) -> Self {
        Self { pure_local_mutation: true, ..Self::new(ty, true) }
    }
}

#[derive(Clone, Debug)]
pub(super) struct FunctionSig {
    effect_declaration: Option<EffectDeclarationId>,
    inferred_effects: bool,
    explicit_return: bool,
    is_alias: bool,
    definition: Option<Span>,
    params: Vec<FunctionParamSig>,
    return_ty: Type,
    return_schema: Option<super::constants::SchemaExpectation>,
    effects: Option<Vec<Effect>>,
}

#[derive(Clone, Debug)]
pub(super) struct FunctionParamSig {
    name: Name,
    ty: Type,
    schema_expectation: Option<super::constants::SchemaExpectation>,
    defaulted: bool,
    rest: bool,
}

#[derive(Clone, Debug)]
pub struct TagVariantInfo {
    pub type_name: Name,
    pub field_count: usize,
    pub field_types: Vec<Type>,
}

#[derive(Clone, Debug)]
pub(super) struct TypeAnnRef {
    program: Arc<ArenaProgram>,
    id: TypeExprId,
}

impl TypeAnnRef {
    pub(super) fn new(program: Arc<ArenaProgram>, id: TypeExprId) -> Self {
        Self { program, id }
    }
}

/// An `Any` expression a `.require(T)?` fix may follow.
#[derive(Clone, Debug)]
pub(super) struct DynamicRequireReceiver {
    /// The expression must be parenthesized before the suffix.
    grouped: bool,
    /// The target a bare `.require()` infers at this position, which the
    /// fix leaves implicit, as `lint.inferred-require-target` asks.
    inferred: Option<Type>,
}

#[derive(Clone, Debug)]
pub(super) enum TypeDefBody {
    Parameterized(usize),
    Declared(Arc<ArenaProgram>, crate::syntax::arena::TypeDefId),
    Resolved(Type),
    Alias(TypeAnnRef),
    RecordSchema(Vec<SchemaField>),
    ModuleContract {
        entries: Vec<ModuleContractEntry>,
        exact: bool,
    },
    TagUnion(Vec<TagVariant>),
}

#[derive(Clone, Debug)]
pub(super) struct SchemaField {
    name: Name,
    ty: TypeAnnRef,
}

#[derive(Clone, Debug)]
pub(super) struct ModuleContractEntry {
    name: Name,
    optional: bool,
    kind: ModuleContractEntryKind,
}

#[derive(Clone, Debug)]
pub(super) enum ModuleContractEntryKind {
    Value(TypeAnnRef),
    Proc {
        params: Vec<ContractParam>,
        effects: Option<Vec<Effect>>,
        return_ty: TypeAnnRef,
    },
    Pure {
        params: Vec<ContractParam>,
        return_ty: TypeAnnRef,
    },
}

#[derive(Clone, Debug)]
pub(super) struct ContractParam {
    source: crate::syntax::arena::ArenaParam,
    name: Name,
    ty: TypeAnnRef,
    defaulted: bool,
    rest: bool,
}

#[derive(Clone, Debug)]
pub(super) struct TagVariant {
    type_name: Name,
    name: Name,
    fields: Vec<TypeAnnRef>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorFamilyInfo {
    pub variants: BTreeMap<Name, ErrorVariantInfo>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorVariantInfo {
    pub fields: ErrorPayloadFields,
    pub facets: Vec<Name>,
    /// The variant was declared without a payload. `fields` is then the one
    /// `message: Str` every error carries, and the constructor takes it as a
    /// single optional positional argument instead of by name.
    pub implicit_message: bool,
}

impl ErrorVariantInfo {
    /// The facts of a variant from the payload fields its declaration wrote.
    pub fn declared(payload: impl IntoIterator<Item = (Name, Type)>, facets: Vec<Name>) -> Self {
        let fields = ErrorPayloadFields::from_declared(payload);
        if fields.is_empty() {
            Self {
                fields: ErrorPayloadFields::from_declared([(Name::intern("message"), Type::Str)]),
                facets,
                implicit_message: true,
            }
        } else {
            Self {
                fields,
                facets,
                implicit_message: false,
            }
        }
    }
}

/// A variant's payload fields in the order the declaration wrote them, which
/// is the order positional constructor arguments fill them. Names are unique.
/// A name-sorted map here would make `V(path, owner)` bind by spelling.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ErrorPayloadFields(Vec<(Name, Type)>);

impl ErrorPayloadFields {
    /// Keeps the first field of each name; the declaration check reports the
    /// duplicates.
    pub fn from_declared(declared: impl IntoIterator<Item = (Name, Type)>) -> Self {
        let mut fields: Vec<(Name, Type)> = Vec::new();
        for (name, ty) in declared {
            if fields.iter().all(|(existing, _)| *existing != name) {
                fields.push((name, ty));
            }
        }
        Self(fields)
    }

    pub fn get(&self, name: &Name) -> Option<&Type> {
        self.0
            .iter()
            .find_map(|(field, ty)| (field == name).then_some(ty))
    }

    pub fn contains_key(&self, name: &Name) -> bool {
        self.get(name).is_some()
    }

    /// Field names in declaration order.
    pub fn keys(&self) -> impl Iterator<Item = &Name> {
        self.0.iter().map(|(name, _)| name)
    }

    pub fn values(&self) -> impl Iterator<Item = &Type> {
        self.0.iter().map(|(_, ty)| ty)
    }

    /// The first two of the leading `count` fields that some value fits both
    /// of, so exchanging positional arguments between them could still check.
    pub fn positional_conflict(&self, count: usize) -> Option<(Name, Name)> {
        let filled = &self.0[..count.min(self.0.len())];
        filled
            .iter()
            .enumerate()
            .find_map(|(index, (left, left_ty))| {
                filled[index + 1..]
                    .iter()
                    .find(|(_, right_ty)| {
                        crate::sema::constants::types_may_share_a_value(left_ty, right_ty)
                    })
                    .map(|(right, _)| (*left, *right))
            })
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuilderKind {
    ProcessCommand,
}

/// What the item `.` denotes inside the innermost callback block being
/// checked. Only callback blocks push a frame; branch, scope, and handler
/// blocks nested in a callback see the callback's item.
#[derive(Clone, Debug)]
enum ItemFrame {
    /// A one-parameter callback written without `|name|`, whose only
    /// parameter is spelled `.`. `used` records whether the body spelled it.
    Implicit { ty: Type, used: bool, stage: bool },
    /// A callback whose parameters are written out, or that takes two: `.`
    /// would be a second spelling of a named value, so it is rejected.
    Named { param: NamedItem, stage: bool },
}

/// How a callback that rejects `.` spells its item instead.
#[derive(Clone, Copy, Debug)]
enum NamedItem {
    Param(Name),
    Discarded,
    /// `fold`/`reduce`: the item is the second of `|acc, item|`.
    Accumulated,
}

/// The scope binding that holds an implicit callback item, so `.` narrows
/// like a named parameter (`if .health != null { use(.health) }`). No source
/// spelling can name it.
fn item_binding() -> Name {
    Name::intern(".")
}

impl ItemFrame {
    fn is_stage(&self) -> bool {
        match self {
            Self::Implicit { stage, .. } | Self::Named { stage, .. } => *stage,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct UserModuleSig {
    values: BTreeMap<Name, Type>,
    procs: FxHashMap<Name, FunctionSig>,
    pures: FxHashMap<Name, FunctionSig>,
    streams: FxHashMap<Name, FunctionSig>,
    types: BTreeMap<Name, TypeDefBody>,
    resolved_types: BTreeMap<Name, Type>,
    tag_variants: BTreeMap<Name, TagVariantInfo>,
    error_families: BTreeMap<Name, ErrorFamilyInfo>,
}

/// A producer accepts failures even in callbacks that cannot emit its items.
/// Keeping its return contract distinct from its yield permission prevents a
/// callback boundary from accidentally forbidding error propagation.
#[derive(Clone, PartialEq)]
enum ReturnContext {
    Function(Type),
    Producer,
}

impl ReturnContext {
    fn ty(&self) -> &Type {
        match self {
            Self::Function(ty) => ty,
            Self::Producer => &Type::Unit,
        }
    }

    fn accepts_failure(&self) -> bool {
        match self {
            Self::Function(ty) => ty.is_result(),
            Self::Producer => true,
        }
    }
}

/// State whose lifetime belongs to the body being checked. Crossing a callable
/// boundary replaces and restores this value whole, so a producer's yields or
/// an enclosing loop cannot become control targets in another body.
#[derive(Clone, Default)]
struct BoundaryContext {
    current_return: Option<ReturnContext>,
    current_yield: Option<Type>,
    in_pure: bool,
    current_effects: Option<Vec<Effect>>,
    effect_owner: Option<EffectDeclarationId>,
    return_schema: Option<super::constants::SchemaExpectation>,
    expected_schema: Option<super::constants::SchemaExpectation>,
    /// The `collect` expression a `yield` here appends to.
    collect_scope: Option<collect::CollectScope>,
    // Only propagation evaluated while initializing the current With reaches its handler.
    with_initializer_errors: Option<Vec<Type>>,
    error_boundary_errors: Vec<Vec<(Type, Span)>>,
    retry_attempt_depth: usize,
    /// `retry` attempts are a subset of the error boundaries.
    retry_block_depth: usize,
    /// A producer cannot suspend while its deadline remains open.
    within_block_depth: usize,
    loop_depth: usize,
    block_depth: usize,
    in_defer_block: bool,
    in_signal_hook: bool,
    item_frames: Vec<ItemFrame>,
    context_scope_depths: Vec<usize>,
    /// A statement-shaped scope consumes its body only in a value tail.
    context_scope_tail_value: bool,
    // The effects enclosing `without` regions exclude, outermost first.
    excluded_effects: Vec<effect_bounds::ExcludedEffect>,
    last_status_available: bool,
    /// The direct function tail consumes a Result[Unit] as a statement and
    /// propagates its failure rather than handing that failure back as data.
    result_unit_function_tail: Option<crate::syntax::arena::StmtId>,
    /// The next expression is a control position of a condition.
    control_condition: bool,
    /// Control positions extend through the operands of `!`, `and`, and `or`.
    in_control_position: bool,
}

#[derive(Clone)]
pub struct Checker {
    checked_declarations: CheckedDeclarations,
    resolved_type_expressions: ResolvedTypeExpressions,
    resolved_bindings: Arc<ResolvedBindings>,
    resolved_command_sites: FxHashMap<(Span, Name), BindingId>,
    boundary: BoundaryContext,
    local_inference: local_inference::LocalInference,
    pub(super) type_constraints: super::constraints::TypeConstraints,
    static_callable_aliases: BTreeMap<Span, StaticCallableAlias>,
    typed_callable_calls: BTreeMap<Span, Arc<crate::sema::types::TypedCallable>>,
    argument_projection_types: FxHashMap<crate::syntax::arena::ExprId, Type>,
    argument_projection_sources:
        FxHashMap<crate::syntax::arena::ExprId, crate::syntax::arena::ExprId>,
    argument_projection_contexts:
        FxHashMap<crate::syntax::arena::ExprId, (Type, crate::sema::constants::SchemaExpectation)>,
    record_constructors: RecordConstructors,
    record_constructor_instances: BTreeMap<Span, super::constants::CheckedRecordConstructor>,
    requirement_targets: BTreeMap<Span, RequirementTarget>,
    requirement_expected_targets: BTreeMap<Span, RequirementTarget>,
    constructor_group_depth: usize,
    pending_record_constructors: Vec<(Span, super::constants::SchemaInstance, Type)>,
    prepared_constants: super::constants::PreparedConstants,
    wire_enums: crate::sema::wire_enums::PreparedWireEnums,
    condition_proofs: FxHashMap<crate::syntax::arena::ExprId, Arc<proof::ConditionNarrowings>>,
    block_exit_bindings: FxHashMap<crate::syntax::arena::BlockId, FxHashMap<Name, Binding>>,
    proven_nonnull_fallback_receivers: BTreeSet<Span>,
    /// How a `check.dynamic-boundary` fix appends `.require(T)?` to each
    /// checked `Any` expression.
    dynamic_require_receivers: FxHashMap<Span, DynamicRequireReceiver>,
    current_namespace: Option<Name>,
    bundle_root_statements: Option<ArenaRange>,
    bundle_module_keys: Option<Arc<[String]>>,
    scopes: Vec<FxHashMap<Name, Binding>>,
    procs: FxHashMap<Name, FunctionSig>,
    byte_script_arguments: bool,
    pures: FxHashMap<Name, FunctionSig>,
    streams: FxHashMap<Name, FunctionSig>,
    // Imported signatures and module contracts change only while
    // declarations are collected. Sharing them lets the speculative copy made
    // for each inferred body skip every signature in the program.
    qualified_procs: Arc<FxHashMap<QualifiedName, FunctionSig>>,
    qualified_pures: Arc<FxHashMap<QualifiedName, FunctionSig>>,
    qualified_streams: Arc<FxHashMap<QualifiedName, FunctionSig>>,
    type_defs: FxHashMap<Name, TypeDefBody>,
    type_namespaces: FxHashMap<Name, BTreeMap<Name, Type>>,
    tag_variants: FxHashMap<Name, TagVariantInfo>,
    error_families: FxHashMap<Name, ErrorFamilyInfo>,
    error_facets: FxHashSet<Name>,
    resolving_types: Vec<Name>,
    user_modules: Arc<FxHashMap<String, UserModuleSig>>,
    diagnostics: Vec<Diagnostic>,
    annotation_facts: Vec<AnnotationFact>,
    reveal_types: Vec<Diagnostic>,
    expr_types: BTreeMap<Span, Type>,
    stream_stage_types: BTreeMap<(Option<Name>, Span), CheckedStreamStage>,
    projections: BTreeMap<Span, CheckedProjection>,
    statement_positions: BTreeMap<Span, StatementPosition>,
    pattern_test_types: FxHashMap<crate::syntax::arena::PatternId, Type>,
    assertion_effect_spans: BTreeSet<Span>,
    statement_expression_spans: BTreeSet<Span>,
    /// Calls whose signature is marked discardable, and each `?` applied to
    /// one. The ignored-value rule reads the propagated span: the value may
    /// be dropped in statement position only after its failure has left.
    discardable_values: BTreeSet<Span>,
    propagating_statements: BTreeSet<Span>,
    discardable_bindings: BTreeSet<Span>,
    propagating_conditions: BTreeSet<Span>,
    redundant_condition_propagations: BTreeSet<Span>,
    unvalidated_command_vectors: BTreeMap<Span, Type>,
    membership_migration_spans: BTreeSet<Span>,
    standard_call_spans: BTreeMap<Span, (String, String)>,
    statically_resolved_call_spans: BTreeSet<Span>,
    api_calls: BTreeMap<Span, CheckedApiCall>,
    module_commands: BTreeMap<Span, CheckedModuleCommand>,
    argument_bindings: BTreeMap<Span, CheckedArguments>,
    definitely_exiting_block_spans: BTreeSet<Span>,
    handler_input_types: BTreeMap<Span, Type>,
    inferred_variants: BTreeMap<Span, InferredVariant>,
    path_literals: BTreeSet<Span>,
    redundant_variant_qualifiers: BTreeMap<Span, Span>,
    record_constructor_fields: BTreeMap<Span, Vec<Name>>,
    message_payload_constructors: BTreeMap<Span, MessagePayloadConstructor>,
    record_require_migrations: BTreeMap<Span, RecordRequireMigration>,
    error_constructors: BTreeMap<Span, CheckedErrorConstructor>,
    optional_binding_spans: BTreeSet<Span>,
    collect_yields: BTreeMap<Span, Span>,
    from_end_indexes: BTreeMap<Span, u32>,
    conversions: BTreeMap<Span, Conversion>,
    resource_scopes: BTreeMap<Span, Vec<crate::modules::ManagedResource>>,
    resource_transfers: ResourceTransferFacts,
    text_patterns: BTreeMap<Span, TextPattern>,
    inferred_variant_patterns: BTreeMap<Span, InferredVariantPattern>,
    options: CheckOptions,
    function_return_types: BTreeMap<Span, Type>,
    parameter_types: BTreeMap<Span, Type>,
    pipeline_hole_types: BTreeMap<Span, Type>,
    /// Collected on first use; a checker checks one program.
    return_inference_index: Option<Arc<infer_return::ReturnInferenceIndex>>,
    inferred_returns: Option<Vec<(Type, Span)>>,
    inferred_propagations: Vec<(Type, Span)>,
    // A proc return probe records disagreeing completions instead of reporting them.
    return_conflicts: Option<Vec<(Type, Type, Span)>>,
    inference_reachable: bool,
    collecting_effects: bool,
    record_effects: bool,
    defer_effect_diagnostics: bool,
    deferred_effect_checks: Vec<DeferredEffectCheck>,
    authoritative_effect_graph: bool,
    module_diagnostic_position: usize,
    effect_graph: EffectGraph,
    effect_summaries: BTreeMap<EffectDeclarationId, EffectSummary>,
    /// Set by a probe that read a callee's effects from a module-value type
    /// instead of recording a graph edge, so the probe must repeat until the
    /// summaries it read are the solved ones.
    provisional_effects_read: std::cell::Cell<bool>,
    module_depth: usize,
    root_signal_hooks: FxHashMap<Name, Span>,
    current_exported: bool,
    /// The final top-level statement of the script, whose `Int` value is
    /// consumed as the exit status rather than discarded.
    exit_status_statement: Option<Span>,
    /// The statements written before a postfix `when` or `unless`. A binding
    /// takes no postfix guard, so a fix that turns one of these into
    /// `let _ = ...` would not parse.
    guarded_statements: FxHashSet<Span>,
    /// The `if`, `match`, and block expressions and tail statements checked
    /// so far. Each checks its own tails against what is expected of it, so
    /// a mismatch one of them reports is not reported again for the whole.
    branch_constructs: FxHashSet<Span>,
    /// The expression of the expression statement entered last, tail or
    /// not. A diagnostic whose fix replaces an expression with a statement
    /// compares its span with this before it checks anything nested.
    statement_root: Option<Span>,
}

impl Checker {
    fn enter_callable_boundary(&mut self) -> BoundaryContext {
        std::mem::take(&mut self.boundary)
    }

    fn enter_callback_boundary(&mut self) -> BoundaryContext {
        let mut callback = self.boundary.clone();
        callback.current_yield = None;
        callback.collect_scope = None;
        callback.loop_depth = 0;
        std::mem::replace(&mut self.boundary, callback)
    }

    fn leave_callback_boundary(&mut self, mut enclosing: BoundaryContext) {
        // Callback failures still occur inside the lexical error boundary;
        // its accumulated error types survive checking the callback body.
        enclosing.error_boundary_errors = std::mem::take(&mut self.boundary.error_boundary_errors);
        enclosing.with_initializer_errors = self.boundary.with_initializer_errors.take();
        self.boundary = enclosing;
    }

    fn leave_cleanup_boundary(&mut self, mut enclosing: BoundaryContext) {
        // Cleanup failures reach the lexical error boundary, but they are
        // not failures of the initializer that registered the cleanup.
        enclosing.error_boundary_errors = std::mem::take(&mut self.boundary.error_boundary_errors);
        self.boundary = enclosing;
    }

    fn prepare_regex_literals(view: &CheckBundleView<'_>) -> Vec<Diagnostic> {
        let program = view.program;
        program
            .arena
            .regex_literals
            .iter()
            .filter(|literal| view.contains_source(literal.span.source_id))
            .filter_map(|literal| {
                crate::modules::regex::prepare_literal(literal)
                    .as_ref()
                    .err()
                    .map(|message| {
                        Diagnostic::error(format!("invalid regex literal: {message}"))
                            .with_code(DiagnosticCode::CheckRegexLiteral)
                            .with_label(Label::primary(literal.span, "invalid regular expression"))
                    })
            })
            .collect()
    }

    pub fn check_arena(program: &crate::syntax::arena::ArenaProgram, source: &str) -> CheckOutput {
        Self::check_arena_with_options(program, source, CheckOptions::default())
    }

    pub fn check_arena_with_options(
        program: &crate::syntax::arena::ArenaProgram,
        source: &str,
        options: CheckOptions,
    ) -> CheckOutput {
        Self::check_arena_with_options_and_type_program(
            program,
            source,
            options,
            Arc::new(program.clone()),
        )
    }

    /// Check an arena-backed program while resolving type references in a
    /// separately owned program.
    pub fn check_arena_with_options_and_type_program(
        program: &crate::syntax::arena::ArenaProgram,
        source: &str,
        options: CheckOptions,
        type_program: Arc<crate::syntax::arena::ArenaProgram>,
    ) -> CheckOutput {
        Self::check_arena_with_options_and_type_program_and_sources(
            program,
            source,
            options,
            type_program,
            None,
            program.source_text_source_id(),
        )
    }

    /// Check a loaded program using the text belonging to each source span.
    /// The entry text also controls the builtin `args` type, while imported
    /// modules are checked against their own source files.
    pub fn check_arena_with_sources_and_options(
        program: &crate::syntax::arena::ArenaProgram,
        sources: &SourceMap,
        entry_source_id: SourceId,
        options: CheckOptions,
    ) -> CheckOutput {
        Self::check_arena_with_sources_options_and_type_program(
            program,
            sources,
            entry_source_id,
            options,
            Arc::new(program.clone()),
        )
    }

    /// Check a source-loaded view while resolving type references in an owned
    /// program, using the text and identity for each referenced source span.
    pub fn check_arena_with_sources_options_and_type_program(
        program: &crate::syntax::arena::ArenaProgram,
        sources: &SourceMap,
        entry_source_id: SourceId,
        options: CheckOptions,
        type_program: Arc<crate::syntax::arena::ArenaProgram>,
    ) -> CheckOutput {
        let source = sources
            .get(entry_source_id)
            .expect("entry source id must exist in the source map")
            .text();
        Self::check_arena_with_options_and_type_program_and_sources(
            program,
            source,
            options,
            type_program,
            Some(sources),
            Some(entry_source_id),
        )
    }

    fn check_arena_with_options_and_type_program_and_sources(
        program: &crate::syntax::arena::ArenaProgram,
        source: &str,
        options: CheckOptions,
        type_program: Arc<crate::syntax::arena::ArenaProgram>,
        sources: Option<&SourceMap>,
        entry_source_id: Option<SourceId>,
    ) -> CheckOutput {
        let view = CheckBundleView::new(program, sources, entry_source_id);
        Self::check_bundle_view_with_type_program(&view, source, options, type_program)
    }

    /// Check an immutable borrowed bundle view while reusing an owned program
    /// for type references. Prepared facts stay tied to the selected root and
    /// reachable modules.
    pub fn check_bundle_view_with_type_program(
        view: &CheckBundleView<'_>,
        source: &str,
        options: CheckOptions,
        type_program: Arc<crate::syntax::arena::ArenaProgram>,
    ) -> CheckOutput {
        Self::check_bundle_in_session(
            view,
            source,
            options,
            type_program,
            view.sources,
            &mut ModuleRecordCache::default(),
        )
    }

    fn into_output(mut self) -> CheckOutput {
        let callable_effects = self.callable_effects();
        let checked_declarations = self.take_checked_declarations();
        CheckOutput {
            checked_declarations,
            resolved_type_expressions: self.resolved_type_expressions,
            static_callable_aliases: self.static_callable_aliases,
            typed_callable_calls: self.typed_callable_calls,
            local_binding_types: self.local_inference.checked_bindings,
            resolved_bindings: self.resolved_bindings,
            prepared_constants: self.prepared_constants,
            proven_nonnull_fallback_receivers: self.proven_nonnull_fallback_receivers,
            diagnostics: self.diagnostics,
            annotation_facts: self.annotation_facts,
            function_return_types: self.function_return_types,
            record_constructor_instances: self.record_constructor_instances,
            parameter_types: self.parameter_types,
            requirement_targets: self.requirement_targets,
            requirement_expected_targets: self.requirement_expected_targets,
            reveal_types: self.reveal_types,
            expr_types: self.expr_types,
            stream_stage_types: self.stream_stage_types,
            projections: self.projections,
            statement_positions: self.statement_positions,
            function_effect_facts: self.effect_graph.facts(&self.effect_summaries),
            callable_effects,
            assertion_effect_spans: self.assertion_effect_spans,
            statement_expression_spans: self.statement_expression_spans,
            propagating_statements: self.propagating_statements,
            discardable_bindings: self.discardable_bindings,
            propagating_conditions: self.propagating_conditions,
            redundant_condition_propagations: self.redundant_condition_propagations,
            unvalidated_command_vectors: self.unvalidated_command_vectors,
            membership_migration_spans: self.membership_migration_spans,
            standard_call_spans: self.standard_call_spans,
            statically_resolved_call_spans: self.statically_resolved_call_spans,
            api_calls: self.api_calls,
            module_commands: self.module_commands,
            argument_bindings: self.argument_bindings,
            definitely_exiting_block_spans: self.definitely_exiting_block_spans,
            handler_input_types: self.handler_input_types,
            inferred_variants: self.inferred_variants,
            path_literals: self.path_literals,
            redundant_variant_qualifiers: self.redundant_variant_qualifiers,
            record_constructor_fields: self.record_constructor_fields,
            message_payload_constructors: self.message_payload_constructors,
            record_require_migrations: self.record_require_migrations,
            error_constructors: self.error_constructors,
            optional_binding_spans: self.optional_binding_spans,
            collect_yields: self.collect_yields,
            from_end_indexes: self.from_end_indexes,
            conversions: self.conversions,
            resource_scopes: self.resource_scopes,
            resource_transfers: self.resource_transfers,
            text_patterns: self.text_patterns,
            inferred_variant_patterns: self.inferred_variant_patterns,
            embedded_bodies_checked: self.options.embedded_bodies,
        }
    }

    pub fn check_arena_interactive(
        program: &crate::syntax::arena::ArenaProgram,
        source: &str,
    ) -> CheckOutput {
        Self::check_arena_interactive_with_commands(program, source, |_| false)
    }

    pub fn check_arena_interactive_with_commands(
        program: &crate::syntax::arena::ArenaProgram,
        source: &str,
        interactive_commands: fn(&str) -> bool,
    ) -> CheckOutput {
        Self::check_arena_with_options(
            program,
            source,
            CheckOptions::for_interactive_commands(interactive_commands),
        )
    }

    pub(crate) fn new(options: CheckOptions) -> Self {
        let mut checker = Self {
            checked_declarations: CheckedDeclarations::default(),
            resolved_type_expressions: ResolvedTypeExpressions::default(),
            boundary: BoundaryContext::default(),
            static_callable_aliases: BTreeMap::new(),
            typed_callable_calls: BTreeMap::new(),
            scopes: vec![FxHashMap::default()],
            procs: FxHashMap::default(),
            byte_script_arguments: false,
            pures: FxHashMap::default(),
            streams: FxHashMap::default(),
            qualified_procs: Arc::default(),
            qualified_pures: Arc::default(),
            qualified_streams: Arc::default(),
            type_defs: FxHashMap::default(),
            local_inference: local_inference::LocalInference::default(),
            type_constraints: super::constraints::TypeConstraints::default(),
            argument_projection_types: FxHashMap::default(),
            argument_projection_sources: FxHashMap::default(),
            argument_projection_contexts: FxHashMap::default(),
            record_constructors: RecordConstructors::default(),
            record_constructor_instances: BTreeMap::new(),
            requirement_targets: BTreeMap::new(),
            requirement_expected_targets: BTreeMap::new(),
            constructor_group_depth: 0,
            pending_record_constructors: Vec::new(),
            resolved_bindings: Arc::default(),
            resolved_command_sites: FxHashMap::default(),
            prepared_constants: super::constants::PreparedConstants::default(),
            wire_enums: crate::sema::wire_enums::PreparedWireEnums::default(),
            condition_proofs: FxHashMap::default(),
            block_exit_bindings: FxHashMap::default(),
            proven_nonnull_fallback_receivers: BTreeSet::default(),
            dynamic_require_receivers: FxHashMap::default(),
            current_namespace: None,
            bundle_root_statements: None,
            bundle_module_keys: None,
            type_namespaces: FxHashMap::default(),
            tag_variants: FxHashMap::default(),
            error_families: FxHashMap::default(),
            error_facets: FxHashSet::default(),
            resolving_types: Vec::new(),
            user_modules: Arc::default(),
            diagnostics: Vec::new(),
            annotation_facts: Vec::new(),
            reveal_types: Vec::new(),
            expr_types: BTreeMap::new(),
            stream_stage_types: BTreeMap::new(),
            projections: BTreeMap::new(),
            statement_positions: BTreeMap::new(),
            pattern_test_types: FxHashMap::default(),
            assertion_effect_spans: BTreeSet::new(),
            statement_expression_spans: BTreeSet::new(),
            discardable_values: BTreeSet::new(),
            propagating_statements: BTreeSet::new(),
            discardable_bindings: BTreeSet::new(),
            propagating_conditions: BTreeSet::new(),
            redundant_condition_propagations: BTreeSet::new(),
            unvalidated_command_vectors: BTreeMap::new(),
            membership_migration_spans: BTreeSet::new(),
            standard_call_spans: BTreeMap::new(),
            statically_resolved_call_spans: BTreeSet::new(),
            api_calls: BTreeMap::new(),
            module_commands: BTreeMap::new(),
            argument_bindings: BTreeMap::new(),
            definitely_exiting_block_spans: BTreeSet::new(),
            handler_input_types: BTreeMap::new(),
            inferred_variants: BTreeMap::new(),
            path_literals: BTreeSet::new(),
            redundant_variant_qualifiers: BTreeMap::new(),
            record_constructor_fields: BTreeMap::new(),
            message_payload_constructors: BTreeMap::new(),
            record_require_migrations: BTreeMap::new(),
            error_constructors: BTreeMap::new(),
            optional_binding_spans: BTreeSet::new(),
            collect_yields: BTreeMap::new(),
            from_end_indexes: BTreeMap::new(),
            conversions: BTreeMap::new(),
            resource_scopes: BTreeMap::new(),
            resource_transfers: ResourceTransferFacts::default(),
            text_patterns: BTreeMap::new(),
            inferred_variant_patterns: BTreeMap::new(),
            options,
            function_return_types: BTreeMap::new(),
            parameter_types: BTreeMap::new(),
            pipeline_hole_types: BTreeMap::new(),
            return_inference_index: None,
            inferred_returns: None,
            inferred_propagations: Vec::new(),
            return_conflicts: None,
            inference_reachable: true,
            collecting_effects: false,
            record_effects: false,
            defer_effect_diagnostics: false,
            deferred_effect_checks: Vec::new(),
            authoritative_effect_graph: false,
            module_diagnostic_position: 0,
            effect_graph: EffectGraph::default(),
            effect_summaries: BTreeMap::new(),
            provisional_effects_read: std::cell::Cell::new(false),
            module_depth: 0,
            root_signal_hooks: FxHashMap::default(),
            current_exported: false,
            exit_status_statement: None,
            guarded_statements: FxHashSet::default(),
            branch_constructs: FxHashSet::default(),
            statement_root: None,
        };
        checker.register_builtin_process_error_family();
        checker.checked_declarations.builtin_error_families = checker.error_families.clone();
        checker.define_standard_values();
        checker
    }

    fn register_builtin_process_error_family(&mut self) {
        // The built-in facet vocabulary is valid in patterns everywhere; user
        // `error` declarations extend it within their scope.
        self.error_facets.extend(
            xsh_registry::errors::ErrorFacet::ALL
                .iter()
                .map(|facet| Name::intern(facet.name())),
        );
        for family in xsh_registry::errors::builtin_error_families() {
            let fields = family
                .fields
                .iter()
                .map(|field| {
                    (
                        Name::intern(field.name),
                        crate::modules::signature::convert_type(&field.ty),
                    )
                })
                .collect::<Vec<_>>();
            let mut variants = BTreeMap::new();
            for variant in family.variants {
                variants.insert(
                    Name::intern(variant.name),
                    ErrorVariantInfo::declared(
                        fields.clone(),
                        variant
                            .facets
                            .iter()
                            .map(|facet| Name::intern(facet.name()))
                            .collect(),
                    ),
                );
            }
            let family_name = if family.name == "ProcessError" {
                Name::PROCESS_ERROR
            } else {
                Name::intern(family.name)
            };
            self.error_families
                .insert(family_name, ErrorFamilyInfo { variants });
        }
    }

    /// Removed vocabulary is recoverable for tooling, but always rejects execution.
    /// A fix is attached only after name resolution proves the canonical target.
    pub(super) fn removed_compatibility_name(
        &mut self,
        span: Span,
        old: &str,
        canonical: &str,
        fix: bool,
    ) {
        let message = format!("`{old}` was removed; use `{canonical}`");
        let mut diagnostic = Diagnostic::error(&message)
            .with_code(DiagnosticCode::CheckCompatibilityVocabulary)
            .with_label(Label::primary(span, &message));
        if fix {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "use the canonical spelling",
                canonical,
            ));
        } else if old == "ARGV" {
            diagnostic = diagnostic.with_note("A local `args` binding shadows script arguments; rename it or capture script arguments before entering that scope.");
        } else if old == "ls" {
            diagnostic = diagnostic.with_note("Use the standard module call `fs.children(...)`; standard API members are not first-class callable values.");
        }
        self.diagnostics.push(diagnostic);
    }

    pub(crate) fn define_standard_values(&mut self) {
        let item = if self.byte_script_arguments { Type::Bytes } else { Type::Str };
        self.define_builtin_value("args", Binding::new(Type::List(Box::new(item)), false));
    }

    fn check_program_arena_with_type_program_and_sources(
        &mut self,
        bundle: &CheckBundleView<'_>,
        source: &str,
        type_program: Arc<crate::syntax::arena::ArenaProgram>,
        records: &ModuleRecords,
    ) {
        self.resolved_command_sites.clear();
        let program = bundle.program;
        self.bundle_root_statements = Some(bundle.root_statements);
        self.bundle_module_keys = Some(
            bundle
                .modules
                .iter()
                .map(|module| module.key.clone())
                .collect::<Vec<_>>()
                .into(),
        );
        debug_assert_eq!(
            records.root().args_domain(),
            if self.byte_script_arguments {
                ArgsDomain::Bytes
            } else {
                ArgsDomain::Str
            }
        );
        records.install(self, program);
        let statements = records.root().statements();
        if !self.collecting_effects {
            self.diagnostics
                .extend(crate::syntax::grouping::grouping_diagnostics_in_range(
                    program,
                    bundle.root_statements,
                    source,
                ));
        }
        self.module_diagnostic_position = self.diagnostics.len();
        self.collect_type_imports_arena(program, statements.iter().copied());
        self.collect_definitions_arena(program, type_program, source, statements.iter().copied());
        self.infer_default_parameter_types(program, source, statements);
        self.infer_local_pure_returns(&bundle, source, statements);
        self.infer_default_parameter_types(program, source, statements);
        // A signature CLI `main`, or a `proc main` that the last statement does
        // not call, runs after the top level and supplies the exit status, so
        // no statement does.
        let has_cli_main = statements
            .iter()
            .any(|&stmt| matches!(program.arena.stmt(stmt).kind, ArenaStmtKind::CliMain(_)));
        let main_runs_after = has_cli_main
            || statements
                .iter()
                .any(|&stmt| defines_proc_main(program, stmt))
                && !statements
                    .last()
                    .is_some_and(|&stmt| calls_main(program, stmt));
        let proc_main_auto_dispatch = !has_cli_main
            && statements.iter().any(|&stmt| defines_proc_main(program, stmt))
            && (!statements.last().is_some_and(|&stmt| calls_main(program, stmt))
                || calls_main_with_implicit_spread_args(program, &statements, &self.resolved_bindings));
        if self.options.interactive_commands.is_none() && proc_main_auto_dispatch {
            let (span, has_unbindable_required_param) = {
                let signature = self
                    .procs
                    .get(&Name::intern("main"))
                    .expect("root proc main signature is collected before entry checking");
                let span = signature
                    .definition
                    .expect("root proc main signature has a source definition");
                let has_unbindable_required_param = signature.params.iter().any(|parameter| {
                    !parameter.rest
                        && !parameter.defaulted
                        && !matches!(&parameter.ty, Type::Str | Type::Path)
                });
                (span, has_unbindable_required_param)
            };
            if has_unbindable_required_param {
                self.error(
                    span,
                    "proc main must use the spread form `(...argv: List[Str])` to receive script arguments; a fixed parameter of this type cannot bind script arguments in the compact runtime",
                    DiagnosticCode::CompactMainMissingSpread,
                );
            }
        }
        if self.options.interactive_commands.is_none() && main_runs_after {
            if let Some(signature) = self.procs.get(&Name::intern("main")) {
                let has_bytes = signature.params.iter().any(|parameter| {
                    parameter.ty == Type::Bytes || matches!(&parameter.ty,
                        Type::List(item) if item.as_ref() == &Type::Bytes)
                });
                let single_byte_rest = matches!(signature.params.as_slice(), [parameter]
                    if parameter.rest && !parameter.defaulted
                    && matches!(&parameter.ty, Type::List(item) if item.as_ref() == &Type::Bytes));
                if has_bytes && !single_byte_rest {
                    if let Some(span) = signature.definition {
                        self.error(span,
                            "byte script arguments require a single spread parameter `(...argv: List[Bytes])`",
                            DiagnosticCode::CompactMainArgs);
                    }
                }
            }
        }
        self.exit_status_statement = statements
            .last()
            .filter(|_| !main_runs_after)
            .map(|&stmt| program.arena.stmt(stmt).span);
        for &stmt in statements {
            self.check_stmt_arena(program, source, stmt);
        }
        self.check_public_result_types(program, statements);
        self.exit_status_statement = None;
        self.resolve_checked_types();
        let instances: Vec<_> = self.record_constructor_instances.iter().map(|(&span, fact)| (span, fact.clone())).collect();
        for (span, fact) in instances { self.publish_constructor_type_expressions(program, span, &fact); }
        let (entries, diagnostics) = crate::sema::cli_entry::validate_cli_entry(
            bundle,
            |parameter| {
                self.checked_parameter_type(program, parameter)
                    .unwrap_or_else(|| {
                        self.record_constructors
                            .resolve_type(&program.arena, parameter.ty, None)
                    })
            },
            |ty| self.record_constructors.cli_parser_type(&program.arena, ty),
            |expr| {
                self.prepared_constants
                    .analyze_expression(&program.arena, expr)
            },
        );
        self.checked_declarations.cli_entries = entries;
        self.diagnostics.extend(diagnostics);
        if !self.collecting_effects {
            let resource_check = resource::check_root_owner(self, bundle);
            self.diagnostics.extend(resource_check.diagnostics);
            self.resource_transfers = resource_check.transfers;
        }
    }

    pub(crate) fn check_public_module_docs(
        program: &ArenaProgram,
        _source: &str,
    ) -> Vec<Diagnostic> {
        let mut checker = Self::new(CheckOptions::default());
        let statements = program.statement_ids().collect::<Vec<_>>();
        checker.check_public_docs(program, program.statements, &statements);
        checker.diagnostics
    }

    fn check_public_docs(
        &mut self,
        program: &ArenaProgram,
        statement_range: crate::syntax::arena::ArenaRange,
        statements: &[crate::syntax::arena::StmtId],
    ) {
        // Arena docs are accumulated across the root and imported modules;
        // report source-trivia diagnostics only for the module being checked.
        let source_id = statements
            .first()
            .map(|statement| program.arena.stmt(*statement).span.source_id);
        let docs = &program.docs;
        let exports = statements
            .iter()
            .copied()
            .filter(|statement| {
                matches!(
                    program.arena.stmt(*statement).kind,
                    ArenaStmtKind::Export(_)
                )
            })
            .collect::<Vec<_>>();
        let Some(first_export) = exports.first().copied() else {
            return;
        };

        if program.module_doc_for(statement_range).is_none() {
            self.error(
                program.arena.stmt(first_export).span,
                "exported modules require a preceding ##! module doc comment",
                DiagnosticCode::CheckMissingModuleDoc,
            );
        }
        for doc in docs
            .duplicate_modules
            .iter()
            .filter(|doc| Some(doc.source_id) == source_id)
        {
            self.error(
                *doc,
                "modules may declare only one ##! module doc comment",
                DiagnosticCode::CheckDuplicateModuleDoc,
            );
        }
        for doc in docs
            .orphaned
            .iter()
            .filter(|doc| Some(doc.source_id) == source_id)
        {
            self.error(
                *doc,
                "doc comments must immediately precede an export or appear as the module ##! doc",
                DiagnosticCode::CheckOrphanDocComment,
            );
        }
        for export in exports {
            if !docs
                .exports
                .iter()
                .any(|(statement, _)| *statement == export)
            {
                self.error(
                    program.arena.stmt(export).span,
                    "exported declarations require preceding ## doc comments",
                    DiagnosticCode::CheckMissingPublicDoc,
                );
            }
        }
    }

    fn callable_effects(&self) -> FxHashMap<String, Option<Vec<Effect>>> {
        let mut effects = FxHashMap::default();
        for (name, sig) in &self.procs {
            effects.insert(name.to_string(), sig.effects.clone());
        }
        for (name, sig) in &self.streams {
            effects.insert(name.to_string(), sig.effects.clone());
        }
        for (name, sig) in self.qualified_procs.iter() {
            effects.insert(name.to_string(), sig.effects.clone());
        }
        for (name, sig) in self.qualified_streams.iter() {
            effects.insert(name.to_string(), sig.effects.clone());
        }
        effects
    }

    pub(crate) fn effects_covers(caller: &[Effect], required: &Effect) -> bool {
        if caller.contains(required) {
            return true;
        }
        // io subsumes fs, net, process, env but not time or error
        caller.contains(&Effect::Io)
            && matches!(
                required,
                Effect::Fs | Effect::Net | Effect::Process | Effect::Env
            )
    }

    pub(crate) fn check_callee_effects(
        &mut self,
        caller_effs: &[Effect],
        callee_effects: &Option<Vec<Effect>>,
        callee_name: &str,
        span: Span,
    ) {
        match callee_effects {
            None => {
                self.error(
                    span,
                    &format!(
                        "callable `{callee_name}` has an unknown or unrestricted effect contract; use a named callable with checked effects or establish an explicit checked contract at its declaration",
                    ),
                    DiagnosticCode::CheckEffectViolation,
                );
            }
            Some(callee_effs) => {
                for eff in callee_effs {
                    if *eff == Effect::Error && self.boundary.retry_attempt_depth > 0 {
                        continue;
                    }
                    if !Self::effects_covers(caller_effs, eff) {
                        self.error(
                            span,
                            &format!(
                                "effect `{}` required by `{callee_name}` is not in caller's declared effects",
                                eff.as_str()
                            ),
                            DiagnosticCode::CheckEffectViolation,
                        );
                    }
                }
            }
        }
    }

    fn lookup(&self, name: Name) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|scope| scope.get(&name))
    }

    // Probe checkers share immutable lexical facts. Command names need source
    // text, so their selected identities accumulate locally and publish once.
    fn publish_command_binding_sites(&mut self) {
        let sites = std::mem::take(&mut self.resolved_command_sites);
        if !sites.is_empty() {
            Arc::make_mut(&mut self.resolved_bindings).value_sites.extend(sites);
        }
    }

    fn lookup_value_binding(&self, name: Name, span: Span) -> Option<&Binding> {
        match self.resolved_command_sites.get(&(span, name)).copied()
            .or_else(|| self.resolved_bindings.at(span, name))
        {
            Some(id) => self.scopes.iter().rev().find_map(|scope| scope.get(&name).filter(|binding| binding.id == Some(id))),
            None => self.lookup(name).filter(|binding| binding.recovery),
        }
    }

    fn define(&mut self, name: Name, mut binding: Binding, span: Span) {
        binding.id = self.resolved_bindings.declaration_sites.get(&(span, name)).copied();
        self.check_standard_module_shadow(&name.as_str(), span);
        self.current_scope_mut().insert(name, binding);
    }

    fn define_builtin_value(&mut self, name: &str, mut binding: Binding) {
        if name == "args" { binding.id = Some(BindingId::args()); }
        self.current_scope_mut().insert(Name::intern(name), binding);
    }

    pub(crate) fn check_standard_module_shadow(&mut self, name: &str, span: Span) {
        if name == "args" {
            if self.scopes.len() == 1 {
                self.error(
                    span,
                    "name `args` shadows the built-in script-arguments binding",
                    DiagnosticCode::CheckStandardModuleShadow,
                );
            }
            return;
        }
        if name != "error" && api_spec().is_standard_module(name) {
            let message = format!("name `{name}` shadows the standard module `{name}`");
            self.error(span, &message, DiagnosticCode::CheckStandardModuleShadow);
        }
    }

    pub(crate) fn reveal_type(&mut self, ty: &Type, span: Span) {
        let message = format!("revealed type: {ty}");
        self.reveal_types.push(
            Diagnostic::new(Severity::Note, message)
                .with_code(DiagnosticCode::CheckRevealType)
                .with_label(Label::primary(span, "expression has this type")),
        );
    }

    fn current_scope(&self) -> &FxHashMap<Name, Binding> {
        self.scopes.last().expect("checker always has a scope")
    }

    fn current_scope_mut(&mut self) -> &mut FxHashMap<Name, Binding> {
        self.scopes.last_mut().expect("checker always has a scope")
    }

    pub(crate) fn push_scope(&mut self) {
        self.scopes.push(FxHashMap::default());
    }

    pub(crate) fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub(crate) fn error(&mut self, span: Span, message: &str, code: DiagnosticCode) {
        self.diagnostics.push(
            Diagnostic::error(message)
                .with_code(code)
                .with_label(Label::primary(span, message)),
        );
    }

    /// Reports the value of a managed `with` binding whose type is not a
    /// built-in resource. Every type but a resource is rejected, an unresolved
    /// one included: the scope must know how to release what it binds.
    pub(crate) fn reject_unmanaged_resource(&mut self, ty: &Type, span: Span) {
        let known = crate::modules::ManagedResource::ALL
            .into_iter()
            .filter(|kind| kind.release_op().is_some())
            .map(|kind| format!("`{}`", kind.type_name()))
            .collect::<Vec<_>>()
            .join(" and ");
        let message = format!(
            "`with` without `else` binds a resource it releases when the block ends, and `{ty}` is not one; the resource types are {known}"
        );
        let mut diagnostic = Diagnostic::error(message.clone())
            .with_code(DiagnosticCode::CheckWithResource)
            .with_label(Label::primary(span, message));
        if let Type::Result(ok, _) = ty
            && crate::modules::managed_resource_for(ok).is_some()
        {
            diagnostic = diagnostic.with_note("propagate the `Result` with `?` to bind its value");
        } else {
            diagnostic = diagnostic.with_note(
                "to group fallible bindings under one handler, write `with ... { ... } else { ... }`",
            );
        }
        self.diagnostics.push(diagnostic);
    }

    pub(crate) fn warning(&mut self, span: Span, message: &str, code: DiagnosticCode) {
        self.diagnostics.push(
            Diagnostic::warning(message)
                .with_code(code)
                .with_label(Label::primary(span, message)),
        );
    }
}

fn defines_proc_main(program: &ArenaProgram, stmt: crate::syntax::arena::StmtId) -> bool {
    match program.arena.stmt(stmt).kind {
        ArenaStmtKind::Export(inner) => defines_proc_main(program, inner),
        ArenaStmtKind::ProcDef(def) => {
            let def = program.arena.function_def(def);
            !def.test_declaration && def.name == "main"
        }
        _ => false,
    }
}

fn calls_main(program: &ArenaProgram, stmt: crate::syntax::arena::StmtId) -> bool {
    use crate::syntax::arena::ArenaExprKind;
    let ArenaStmtKind::Expr(mut expr) = program.arena.stmt(stmt).kind else {
        return false;
    };
    while let ArenaExprKind::Try(inner) = program.arena.expr(expr).kind {
        expr = inner;
    }
    matches!(program.arena.expr(expr).kind, ArenaExprKind::Call { callee, .. }
        if matches!(program.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "main"))
}

fn calls_main_with_implicit_spread_args(
    program: &ArenaProgram,
    statements: &[StmtId],
    resolved_bindings: &ResolvedBindings,
) -> bool {
    let Some((_, stmt)) = statements.iter().copied().enumerate().next_back() else {
        return false;
    };
    let ArenaStmtKind::Expr(mut expr) = program.arena.stmt(stmt).kind else {
        return false;
    };
    while let ArenaExprKind::Try(inner) = program.arena.expr(expr).kind {
        expr = inner;
    }
    let ArenaExprKind::Call { callee, args } = program.arena.expr(expr).kind else {
        return false;
    };
    if !matches!(program.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == Name::intern("main")) {
        return false;
    }
    let [argument] = program.arena.call_args(args) else {
        return false;
    };
    let ArenaCallArgKind::Splice { value, .. } = &argument.kind else {
        return false;
    };
    if !matches!(program.arena.expr(*value).kind, ArenaExprKind::Ident(name) if name == Name::intern("args")) {
        return false;
    }
    let args_binding = resolved_bindings
        .at(program.arena.expr(*value).span, Name::intern("args"))
        .expect("implicit main argument splice has a resolved args binding");
    args_binding == BindingId::args()
}

/// Only a single byte rest parameter opts an entry into raw OS arguments.
/// Parameter facts come from checking, so aliases use their resolved types.
pub(crate) fn byte_argument_main(
    program: &ArenaProgram,
    parameter_types: &BTreeMap<Span, Type>,
) -> bool {
    byte_argument_main_with_statements(program, program.statement_ids(), parameter_types)
}

pub(crate) fn byte_argument_main_for_root(
    program: &ArenaProgram,
    root_statements: ArenaRange,
    parameter_types: &BTreeMap<Span, Type>,
) -> bool {
    byte_argument_main_with_statements(
        program,
        program.arena.stmt_ids(root_statements),
        parameter_types,
    )
}

fn byte_argument_main_with_statements(
    program: &ArenaProgram,
    mut statements: impl Iterator<Item = StmtId>,
    parameter_types: &BTreeMap<Span, Type>,
) -> bool {
    fn entry(program: &ArenaProgram, id: crate::syntax::arena::StmtId) -> Option<crate::syntax::arena::FunctionDefId> {
        match program.arena.stmt(id).kind {
            ArenaStmtKind::Export(inner) => entry(program, inner),
            ArenaStmtKind::ProcDef(id) => {
                let definition = program.arena.function_def(id);
                (!definition.test_declaration && definition.name == "main").then_some(id)
            }
            _ => None,
        }
    }
    let Some(definition) = statements.find_map(|id| entry(program, id)) else {
        return false;
    };
    let parameters = program.arena.params(program.arena.function_def(definition).params);
    let [parameter] = parameters else { return false; };
    parameter.rest && parameter.default.is_none()
        && matches!(parameter_types.get(&program.arena.span(parameter.span)),
            Some(Type::List(item)) if item.as_ref() == &Type::Bytes)
}
