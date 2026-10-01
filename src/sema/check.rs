#![allow(clippy::single_call_fn)]

use crate::diagnostic::Severity;
pub(crate) use crate::diagnostic::{Diagnostic, FixHint, Label};
pub(crate) use crate::modules::{ApiArgCheck, MethodReceiver, MethodSig, ModuleFnSig, api_spec};
use crate::runtime::signal::{normalize_hook_signal, signal_rejection_message};
pub(crate) use crate::sema::records::standard_record_type;
pub(crate) use crate::sema::types::{CallableParamType, CallableType, ModuleExportType, Type};
pub(crate) use crate::source::Span;
pub(crate) use crate::symbol::{Name, QualifiedName};
use crate::syntax::arena::{ArenaProgram, ArenaStmtKind, TypeExprId};
pub(crate) use crate::syntax::node::{BinaryOp, CoreCommand, Effect, RunKind, UnaryOp};

pub(crate) use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
pub use callable_alias::StaticCallableAlias;
use callable_alias::CallableAlias;

#[path = "check/args.rs"]
mod args;
#[path = "check/callable_alias.rs"]
mod callable_alias;
#[path = "check/builder.rs"]
mod builder;
#[path = "check/call.rs"]
mod call;
mod constructor_arguments;
mod constructor_application;
pub use constructor_application::{ConstructorAuthority, ConstructorDefaultIdentity, ConstructorValueSource, SolvedConstructorApplication, SolvedConstructorArgument, SolvedConstructorDefault, SolvedConstructorParameter};
mod record_require;
#[path = "check/command.rs"]
mod command;
#[path = "check/compact.rs"]
mod compact;
#[path = "check/decl.rs"]
mod decl;
#[path = "check/expr.rs"]
mod expr;
#[path = "check/expected.rs"]
mod expected;
pub use expected::RequirementTarget;
#[path = "check/infer_effects.rs"]
mod infer_effects;
#[path = "check/return_join.rs"]
mod return_join;
#[path = "check/infer_param.rs"]
mod infer_param;
#[path = "check/local_inference.rs"]
mod local_inference;
#[path = "check/method.rs"]
mod method;
#[path = "check/pattern.rs"]
mod pattern;
#[path = "check/proof.rs"]
mod proof;
#[path = "check/stmt.rs"]
mod stmt;
#[path = "check/stream.rs"]
mod stream;
#[path = "check/types.rs"]
mod types;
#[path = "check/solved.rs"]
mod solved;
#[path = "check/generic.rs"]
mod generic;
mod dependency;
mod registry;
mod registry_boundaries;
mod standard_operation;
mod stage_operation;
mod callable;
mod language_operation;
mod command_operation;
mod wait_operation;
mod run_operation;
mod operation_catalog;
pub(crate) use operation_catalog::{SolvedOperationAuthority, SolvedOperationCatalog};
mod iteration_operation;
mod record_update;
mod module_projection;
pub use module_projection::{ModuleProjectionKind, SolvedModuleProjection};
mod schema_validation;
pub use schema_validation::{SolvedSchemaValidation, SolvedSchemaExpectation, SolvedSchemaApplication, SchemaValidationMode};
mod producer;
#[path = "check/producer_eval.rs"]
pub(crate) mod producer_eval;
pub use producer::{ProducerFlowField, ProducerFlowGraph, ProducerFlowId, ProducerFlowKind, ProducerFlowOperationAlternative, ProducerFlowOperationTransfer, ProducerFlowNode, ProducerFlowSource};
pub use solved::{NominalDeclaration, QualifiedNominalIdentity, BindingIdentity, CallBinding, ComprehensionIdentity, DeclarationIdentity, ExpressionIdentity, StageIdentity, StageCallback, SolvedStage, SolvedComprehensionClause, ProducerEffects, ProducerPath, ProducerPathComponent, ProducerProfile, ReturnElaboration, SolvedBinding, SolvedCall, SolvedCallable, SolvedExpressionCallable, SolvedInvocation, SolvedOperation, SolvedProjection, SolvedRecordUpdate, SolvedRecordUpdateReplacement, RecordUpdateValueSource, SolvedTypes, StatementIdentity};
pub use solved::RegistryArgumentCoercion;

#[cfg(test)]
#[path = "check/generic_tests.rs"]
mod generic_tests;

#[cfg(test)]
#[path = "check/native_callable_tests.rs"]
mod native_callable_tests;
#[cfg(test)]
#[path = "check/native_command_tests.rs"]
mod native_command_tests;
#[cfg(test)]
#[path = "check/native_family_tests.rs"]
mod native_family_tests;
#[cfg(test)]
#[path = "check/call_contract_source_tests.rs"]
mod call_contract_source_tests;

#[cfg(test)]
#[path = "check/equality_operation_tests.rs"]
mod equality_operation_tests;

#[cfg(test)]
#[path = "check/source_language_contract_tests.rs"]
mod source_language_contract_tests;

#[cfg(test)]
#[path = "check/source_permission_contract_tests.rs"]
mod source_permission_contract_tests;

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

pub use self::compact::{
    CompactBodyProbeOutput, CompactDeclOutput, CompactFunctionSig, CompactTypeDefInfo,
};
use self::expr::expr_ty_auto_propagates;
use self::stmt::block_has_exit_point_arena;
use self::types::{
    collection_item_ty, result_types,
    tail_type_matches_expected,
};

/// The checked purpose of a statement remains fixed when its value is unused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatementPosition {
    Statement,
    Value,
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
    pub check_options: CheckOptions,
    pub solved: Arc<SolvedTypes>,
    pub static_callable_aliases: BTreeMap<Span, StaticCallableAlias>,
    pub local_binding_types: BTreeMap<Span, Type>,
    pub prepared_constants: super::constants::PreparedConstants,
    pub record_constructors: RecordConstructors,
    pub wire_enums: crate::sema::wire_enums::PreparedWireEnums,
    pub(crate) cli_entry: Option<crate::sema::cli_entry::CliEntryPlan>,
    pub error_families: FxHashMap<Name, ErrorFamilyInfo>,
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
    pub terminating_call_spans: BTreeSet<Span>,
    /// Checked boolean expressions consumed as assertion statements.
    pub assertion_spans: BTreeSet<Span>,
    pub assertion_effect_spans: BTreeSet<Span>,
    pub statement_expression_spans: BTreeSet<Span>,
    pub membership_migration_spans: BTreeSet<Span>,
    pub standard_call_spans: BTreeMap<Span, (String, String)>,
    pub statically_resolved_call_spans: BTreeSet<Span>,
    /// Ordinary blocks whose checked paths cannot reach their enclosing continuation.
    pub definitely_exiting_block_spans: BTreeSet<Span>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CheckOptions {
    pub interactive_commands: Option<fn(&str) -> bool>,
    pub reveal_types: bool,
    pub migration_diagnostics: bool,
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
    // An import alias keeps the namespace that actually declares its exports.
    static_namespace: Option<Name>,
    callable_alias: Option<CallableAlias>,
    graph_callable: Option<SolvedExpressionCallable>,
    value_scheme: Option<crate::sema::inference::SchemeId>,
    producer_flow: Option<ProducerFlowId>,
    producer_binding: Option<(BindingIdentity, u32)>,
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
            callable_alias: None,
            graph_callable: None,
            value_scheme: None,
            producer_flow: None, producer_binding: None,
            static_namespace: None,
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
        Self {
            callable_alias: None,
            graph_callable: None,
            value_scheme: None,
            producer_flow: None, producer_binding: None,
            static_namespace: None,
            ty,
            mutable: true,
            pure_local_mutation: true,
            unrefined_ty: None,
            proof: proof::BindingProof::default(),
            boolean_proof: None,
            schema_expectation: None,
        }
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

#[derive(Clone, Debug)]
pub(super) enum TypeDefBody {
    Parameterized(usize),
    Declared(Arc<ArenaProgram>, crate::syntax::arena::TypeDefId),
    Resolved(Type),
    Alias(TypeAnnRef),
    RecordSchema(Vec<SchemaField>),
    ModuleContract(Vec<ModuleContractEntry>),
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
    pub fields: BTreeMap<Name, Type>,
    pub facets: Vec<Name>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuilderKind {
    ProcessCommand,
}

#[derive(Clone, Debug, Default)]
pub(super) struct UserModuleSig {
    namespace: Option<Name>,
    values: BTreeMap<Name, Type>,
    procs: FxHashMap<Name, FunctionSig>,
    pures: FxHashMap<Name, FunctionSig>,
    streams: FxHashMap<Name, FunctionSig>,
    types: BTreeMap<Name, TypeDefBody>,
    resolved_types: BTreeMap<Name, Type>,
    tag_variants: BTreeMap<Name, TagVariantInfo>,
    error_families: BTreeMap<Name, ErrorFamilyInfo>,
}

#[derive(Clone)]
struct StreamItemFact {
    ty: Type,
    producer_flow: Option<ProducerFlowId>,
}

#[derive(Clone)]
struct LoopValueBoundary {
    depth: usize,
    declaration: Option<DeclarationIdentity>,
    deferred: bool,
    stream_depth: usize,
    expected: Option<Type>,
    contributions: Vec<(Type, Span)>,
    producers: Vec<ProducerFlowId>,
}

#[derive(Clone)]
pub struct Checker {
    generic: std::rc::Rc<std::cell::RefCell<generic::GenericState>>,
    graph_generation: bool,
    current_generic: Option<DeclarationIdentity>,
    diagnostic_effect_facts: BTreeMap<DeclarationIdentity, FunctionEffectFact>,
    stage_callback_effects: Option<crate::sema::inference::EffectSummary>,
    current_expression: Option<crate::syntax::arena::ExprId>,
    current_statement: Option<crate::syntax::arena::StmtId>,
    constructor_spread_contexts: BTreeMap<ExpressionIdentity, constructor_arguments::ConstructorSpreadContext>,
    graph_argument_depth: usize,
    stage_ground_call_adapter: bool,
    principal_callable_initializer: bool,
    local_initializer_level: Option<u32>,
    erased_literal_context: Option<(Vec<crate::syntax::arena::ExprId>, bool)>,
    // Mixed argv admission belongs only to this original literal expression.
    command_argv_literal_context: Option<crate::syntax::arena::ExprId>,
    local_inference: local_inference::LocalInference,
    pub(super) type_constraints: super::constraints::TypeConstraints,
    static_callable_aliases: BTreeMap<Span, StaticCallableAlias>,
    record_constructors: RecordConstructors,
    expected_schema: Option<super::constants::SchemaExpectation>,
    return_schema: Option<super::constants::SchemaExpectation>,
    record_constructor_instances: BTreeMap<Span, super::constants::CheckedRecordConstructor>,
    requirement_targets: BTreeMap<Span, RequirementTarget>,
    requirement_expected_targets: BTreeMap<Span, RequirementTarget>,
    constructor_group_depth: usize,
    pending_record_constructors: Vec<constructor_application::PendingRecordConstructor>,
    pending_constructor_expressions: BTreeMap<ExpressionIdentity, (Type, Option<DeclarationIdentity>)>,
    prepared_constants: super::constants::PreparedConstants,
    wire_enums: crate::sema::wire_enums::PreparedWireEnums,
    cli_entry: Option<crate::sema::cli_entry::CliEntryPlan>,
    condition_proofs: FxHashMap<crate::syntax::arena::ExprId, Arc<proof::ConditionNarrowings>>,
    block_exit_bindings: FxHashMap<crate::syntax::arena::BlockId, FxHashMap<Name, Binding>>,
    proven_nonnull_fallback_receivers: BTreeSet<Span>,
    current_namespace: Option<Name>,
    scopes: Vec<FxHashMap<Name, Binding>>,
    context_scope_depths: Vec<usize>,
    /// A statement-shaped scope consumes its body only in a value tail.
    context_scope_tail_value: bool,
    procs: FxHashMap<Name, FunctionSig>,
    pures: FxHashMap<Name, FunctionSig>,
    streams: FxHashMap<Name, FunctionSig>,
    qualified_procs: FxHashMap<QualifiedName, FunctionSig>,
    qualified_pures: FxHashMap<QualifiedName, FunctionSig>,
    qualified_streams: FxHashMap<QualifiedName, FunctionSig>,
    type_defs: FxHashMap<Name, TypeDefBody>,
    type_namespaces: FxHashMap<Name, BTreeMap<Name, Type>>,
    tag_variants: FxHashMap<Name, TagVariantInfo>,
    error_families: FxHashMap<Name, ErrorFamilyInfo>,
    error_facets: FxHashSet<Name>,
    resolving_types: Vec<Name>,
    user_modules: FxHashMap<String, UserModuleSig>,
    diagnostics: Vec<Diagnostic>,
    annotation_facts: Vec<AnnotationFact>,
    reveal_types: Vec<Diagnostic>,
    reveal_requests: BTreeMap<ExpressionIdentity, Span>,
    expr_types: BTreeMap<Span, Type>,
    stream_stage_types: BTreeMap<(Option<Name>, Span), CheckedStreamStage>,
    projections: BTreeMap<Span, CheckedProjection>,
    statement_positions: BTreeMap<Span, StatementPosition>,
    pattern_test_types: FxHashMap<crate::syntax::arena::PatternId, Type>,
    terminating_call_spans: BTreeSet<Span>,
    assertion_spans: BTreeSet<Span>,
    assertion_effect_spans: BTreeSet<Span>,
    statement_expression_spans: BTreeSet<Span>,
    membership_migration_spans: BTreeSet<Span>,
    standard_call_spans: BTreeMap<Span, (String, String)>,
    statically_resolved_call_spans: BTreeSet<Span>,
    definitely_exiting_block_spans: BTreeSet<Span>,
    options: CheckOptions,
    function_return_types: BTreeMap<Span, Type>,
    parameter_types: BTreeMap<Span, Type>,
    pipeline_hole_types: BTreeMap<Span, Type>,
    inferred_returns: Option<Vec<(Type, Span)>>,
    inferred_propagations: Vec<(Type, Span)>,
    // Only propagation evaluated while initializing the current With reaches its handler.
    with_initializer_errors: Option<Vec<Type>>,
    inference_reachable: bool,
    current_return: Option<Type>,
    current_yield: Option<Type>,
    in_pure: bool,
    current_effects: Option<Vec<Effect>>,
    effect_owner: Option<EffectDeclarationId>,
    last_status_available: bool,
    stream_items: Vec<StreamItemFact>,
    loop_depth: usize,
    loop_value_boundaries: Vec<LoopValueBoundary>,
    block_depth: usize,
    retry_attempt_depth: usize,
    error_boundary_errors: Vec<Vec<(Type, Span)>>,
    error_boundary_producer_flows: Vec<Vec<ProducerFlowId>>,
    module_depth: usize,
    in_signal_hook: bool,
    in_defer_block: bool,
    root_signal_hooks: FxHashMap<Name, Span>,
    current_exported: bool,
}

impl Checker {
    pub(crate) fn prepare_regex_literals(program: &ArenaProgram) -> Vec<Diagnostic> {
        program.arena.regex_literals.iter().filter_map(|literal| {
            crate::modules::regex::prepare_literal(literal).as_ref().err().map(|message| {
                Diagnostic::error(format!("invalid regex literal: {message}"))
                    .with_code("check.regex-literal")
                    .with_label(Label::primary(literal.span, "invalid regular expression"))
            })
        }).collect()
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

    /// Check a mutable view of an arena-backed bundle while reusing an owned
    /// program for type references. Tooling can change the root statement
    /// range and module list between checks without cloning the full arena.
    pub fn check_arena_with_options_and_type_program(
        program: &crate::syntax::arena::ArenaProgram,
        source: &str,
        options: CheckOptions,
        type_program: Arc<crate::syntax::arena::ArenaProgram>,
    ) -> CheckOutput {
        program.symbol_owner().with_current(|| {
            let mut checker = Self::new(options);
            checker.check_program_arena_with_type_program(program, source, type_program);
            let solved = checker.freeze_solved_types();
            checker.render_reveal_requests(&solved);
            let callable_effects = checker.callable_effects_from_solved(&solved, program);
            let function_effect_facts = checker.function_effect_facts_from_solved(&solved, program);
            CheckOutput {
                check_options: options,
                solved,
                static_callable_aliases: checker.static_callable_aliases,
                local_binding_types: checker.local_inference.checked_bindings,
                prepared_constants: checker.prepared_constants,
                record_constructors: checker.record_constructors,
                wire_enums: checker.wire_enums,
                cli_entry: checker.cli_entry,
                error_families: checker.error_families,
                proven_nonnull_fallback_receivers: checker.proven_nonnull_fallback_receivers,
                diagnostics: checker.diagnostics,
                annotation_facts: checker.annotation_facts,
                function_return_types: checker.function_return_types,
                record_constructor_instances: checker.record_constructor_instances,
                parameter_types: checker.parameter_types,
                requirement_targets: checker.requirement_targets,
                requirement_expected_targets: checker.requirement_expected_targets,
                reveal_types: checker.reveal_types,
                expr_types: checker.expr_types,
                stream_stage_types: checker.stream_stage_types,
                projections: checker.projections,
                statement_positions: checker.statement_positions,
                function_effect_facts,
                callable_effects,
                terminating_call_spans: checker.terminating_call_spans,
                assertion_spans: checker.assertion_spans,
                assertion_effect_spans: checker.assertion_effect_spans,
                statement_expression_spans: checker.statement_expression_spans,
                membership_migration_spans: checker.membership_migration_spans,
                standard_call_spans: checker.standard_call_spans,
                statically_resolved_call_spans: checker.statically_resolved_call_spans,
                definitely_exiting_block_spans: checker.definitely_exiting_block_spans,
            }
        })
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
            CheckOptions {
                interactive_commands: Some(interactive_commands),
                reveal_types: false,
                migration_diagnostics: false,
            },
        )
    }

    /// Check a multi-module program assembled from separately parsed arenas.
    ///
    /// `main` is the entry arena+source; each `(key, name, arena, source)` in
    /// `modules` is linked with the entry source in one arena, with matching
    /// imports resolved by module key or name. Declaring scopes and private
    /// schema dependencies remain available during concrete instantiation.
    pub fn check_arena_with_modules(
        main: (&crate::syntax::arena::ArenaProgram, &str),
        modules: &[(&str, &str, &crate::syntax::arena::ArenaProgram, &str)],
    ) -> CheckOutput {
        main.0.symbol_owner().with_current(|| {
            let mut checker = Self::new(CheckOptions::default());
            // One arena preserves declaration identities and private type dependencies across modules.
            let mut builder = crate::syntax::arena::ArenaProgramBuilder::with_token_capacity_and_symbols(
                main.1.len(), main.0.symbol_owner().clone(),
            );
            let entry_source = main.0.statement_ids().next().map(|id| main.0.arena.stmt(id).span.source_id)
                .unwrap_or(crate::source::SourceId::new(0));
            let entry = crate::syntax::parser::Parser::parse_source_into_arena_builder(entry_source, main.1, &mut builder);
            checker.diagnostics.extend(entry.diagnostics);
            for (index, (key, name, arena, source)) in modules.iter().enumerate() {
                let source_id = arena.statement_ids().next().map(|id| arena.arena.stmt(id).span.source_id)
                    .unwrap_or(crate::source::SourceId::new(index + 1));
                let fragment = crate::syntax::parser::Parser::parse_source_into_arena_builder(source_id, source, &mut builder);
                checker.diagnostics.extend(fragment.diagnostics);
                builder.push_arena_module((*key).to_string(), Name::intern(name), fragment.statements);
            }
            let mut main_program = builder.finish_with_statements(entry.statements);
            for index in 0..main_program.arena.use_stmts.len() {
                let import = &main_program.arena.use_stmts[index];
                let path = main_program.arena.names(import.path).map(|name| name.to_string()).collect::<Vec<_>>().join(".");
                // The legacy single-module fixture form accepts the caller's import spelling.
                let resolved = modules.iter().find(|(key, name, ..)| *key == path || *name == path)
                    .or_else(|| if modules.len() == 1 { modules.first() } else { None });
                if let Some((key, ..)) = resolved {
                    main_program.arena.use_stmts[index].resolved = Some(std::sync::Arc::from(*key));
                }
            }

            checker.check_program_arena(&main_program, main.1);
            let solved = checker.freeze_solved_types();
            checker.render_reveal_requests(&solved);
            let callable_effects = checker.callable_effects_from_solved(&solved, &main_program);
            let function_effect_facts = checker.function_effect_facts_from_solved(&solved, &main_program);
            CheckOutput {
                check_options: CheckOptions::default(),
                solved,
                static_callable_aliases: checker.static_callable_aliases,
                local_binding_types: checker.local_inference.checked_bindings,
                prepared_constants: checker.prepared_constants,
                record_constructors: checker.record_constructors,
                wire_enums: checker.wire_enums,
                cli_entry: checker.cli_entry,
                error_families: checker.error_families,
                proven_nonnull_fallback_receivers: checker.proven_nonnull_fallback_receivers,
                diagnostics: checker.diagnostics,
                annotation_facts: checker.annotation_facts,
                function_return_types: checker.function_return_types,
                record_constructor_instances: checker.record_constructor_instances,
                parameter_types: checker.parameter_types,
                requirement_targets: checker.requirement_targets,
                requirement_expected_targets: checker.requirement_expected_targets,
                reveal_types: checker.reveal_types,
                expr_types: checker.expr_types,
                stream_stage_types: checker.stream_stage_types,
                projections: checker.projections,
                statement_positions: checker.statement_positions,
                function_effect_facts,
                callable_effects,
                terminating_call_spans: checker.terminating_call_spans,
                assertion_spans: checker.assertion_spans,
                assertion_effect_spans: checker.assertion_effect_spans,
                statement_expression_spans: checker.statement_expression_spans,
                membership_migration_spans: checker.membership_migration_spans,
                standard_call_spans: checker.standard_call_spans,
                statically_resolved_call_spans: checker.statically_resolved_call_spans,
                definitely_exiting_block_spans: checker.definitely_exiting_block_spans,
            }
        })
    }

    pub(crate) fn new(options: CheckOptions) -> Self {
        let mut checker = Self {
            generic: std::rc::Rc::new(std::cell::RefCell::new(generic::GenericState::default())),
            graph_generation: true,
            current_generic: None,
            diagnostic_effect_facts: BTreeMap::new(),
            stage_callback_effects: None,
            current_expression: None,
            current_statement: None,
            constructor_spread_contexts: BTreeMap::new(),
            graph_argument_depth: 0,
            stage_ground_call_adapter: false,
            principal_callable_initializer: false,
            local_initializer_level: None,
            erased_literal_context: None,
            command_argv_literal_context: None,
            static_callable_aliases: BTreeMap::new(),
            scopes: vec![FxHashMap::default()],
            context_scope_depths: Vec::new(),
            context_scope_tail_value: false,
            procs: FxHashMap::default(),
            pures: FxHashMap::default(),
            streams: FxHashMap::default(),
            qualified_procs: FxHashMap::default(),
            qualified_pures: FxHashMap::default(),
            qualified_streams: FxHashMap::default(),
            type_defs: FxHashMap::default(),
            local_inference: local_inference::LocalInference::default(),
            type_constraints: super::constraints::TypeConstraints::default(),
            record_constructors: RecordConstructors::default(),
            expected_schema: None,
            return_schema: None,
            record_constructor_instances: BTreeMap::new(),
            requirement_targets: BTreeMap::new(),
            requirement_expected_targets: BTreeMap::new(),
            constructor_group_depth: 0,
            pending_record_constructors: Vec::new(),
            pending_constructor_expressions: BTreeMap::new(),
            prepared_constants: super::constants::PreparedConstants::default(),
            wire_enums: crate::sema::wire_enums::PreparedWireEnums::default(),
            cli_entry: None,
            condition_proofs: FxHashMap::default(),
            block_exit_bindings: FxHashMap::default(),
            proven_nonnull_fallback_receivers: BTreeSet::default(),
            current_namespace: None,
            type_namespaces: FxHashMap::default(),
            tag_variants: FxHashMap::default(),
            error_families: FxHashMap::default(),
            error_facets: FxHashSet::default(),
            resolving_types: Vec::new(),
            user_modules: FxHashMap::default(),
            diagnostics: Vec::new(),
            annotation_facts: Vec::new(),
            reveal_types: Vec::new(),
            reveal_requests: BTreeMap::new(),
            expr_types: BTreeMap::new(),
            stream_stage_types: BTreeMap::new(),
            projections: BTreeMap::new(),
            statement_positions: BTreeMap::new(),
            pattern_test_types: FxHashMap::default(),
            terminating_call_spans: BTreeSet::new(),
            assertion_spans: BTreeSet::new(),
            assertion_effect_spans: BTreeSet::new(),
            statement_expression_spans: BTreeSet::new(),
            membership_migration_spans: BTreeSet::new(),
            standard_call_spans: BTreeMap::new(),
            statically_resolved_call_spans: BTreeSet::new(),
            definitely_exiting_block_spans: BTreeSet::new(),
            options,
            function_return_types: BTreeMap::new(),
            parameter_types: BTreeMap::new(),
            pipeline_hole_types: BTreeMap::new(),
            inferred_returns: None,
            inferred_propagations: Vec::new(),
            with_initializer_errors: None,
            inference_reachable: true,
            current_return: None,
            current_yield: None,
            in_pure: false,
            current_effects: None,
            effect_owner: None,
            last_status_available: false,
            stream_items: Vec::new(),
            loop_depth: 0,
            loop_value_boundaries: Vec::new(),
            block_depth: 0,
            retry_attempt_depth: 0,
            error_boundary_errors: Vec::new(),
            error_boundary_producer_flows: Vec::new(),
            module_depth: 0,
            in_signal_hook: false,
            in_defer_block: false,
            root_signal_hooks: FxHashMap::default(),
            current_exported: false,
        };
        checker.register_builtin_process_error_family();
        checker.define_standard_values();
        checker
    }

    fn register_builtin_process_error_family(&mut self) {
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
                .collect::<BTreeMap<_, _>>();
            let mut variants = BTreeMap::new();
            for variant in family.variants {
                for facet in variant.facets {
                    self.error_facets.insert(Name::intern(facet));
                }
                variants.insert(
                    Name::intern(variant.name),
                    ErrorVariantInfo {
                        fields: fields.clone(),
                        facets: variant.facets.iter().map(Name::intern).collect(),
                    },
                );
            }
            let family_name = if family.name == "ProcessError" {
                Name::PROCESS_ERROR
            } else {
                Name::intern(family.name)
            };
            {
                let mut state = self.generic.borrow_mut();
                state.nominal_declarations.insert((None, generic::ResolvedNominal::ErrorFamily(family_name)), QualifiedNominalIdentity::Builtin { family: family_name, member: None });
                for (variant, info) in &variants {
                    state.nominal_declarations.insert((None, generic::ResolvedNominal::ErrorVariant { family: family_name, variant: *variant }), QualifiedNominalIdentity::Builtin { family: family_name, member: Some(*variant) });
                    for facet in &info.facets {
                        state.nominal_declarations.insert((None, generic::ResolvedNominal::ErrorFacet(*facet)), QualifiedNominalIdentity::Builtin { family: *facet, member: None });
                    }
                }
            }
            self.error_families
                .insert(family_name, ErrorFamilyInfo { variants });
        }
    }

    /// Removed vocabulary is recoverable for tooling, but always rejects execution.
    /// A fix is attached only after name resolution proves the canonical target.
    pub(super) fn removed_compatibility_name(&mut self, span: Span, old: &str, canonical: &str, fix: bool) {
        let message = format!("`{old}` was removed; use `{canonical}`");
        let mut diagnostic = Diagnostic::error(&message)
            .with_code("check.compatibility-vocabulary")
            .with_label(Label::primary(span, &message));
        if fix {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(span, "use the canonical spelling", canonical));
        } else if old == "ARGV" {
            diagnostic = diagnostic.with_note("A local `args` binding shadows script arguments; rename it or capture script arguments before entering that scope.");
        } else if old == "ls" {
            diagnostic = diagnostic.with_note("Use the standard module call `fs.children(...)`; standard API members are not first-class callable values.");
        }
        self.diagnostics.push(diagnostic);
    }

    pub(crate) fn define_standard_values(&mut self) {
        self.define_builtin_value("args", Binding::new(Type::List(Box::new(Type::Str)), false));
    }

    pub(crate) fn check_program_arena(
        &mut self,
        program: &crate::syntax::arena::ArenaProgram,
        source: &str,
    ) {
        self.check_program_arena_with_type_program(program, source, Arc::new(program.clone()));
    }

    pub(crate) fn check_program_arena_with_type_program(
        &mut self,
        program: &crate::syntax::arena::ArenaProgram,
        source: &str,
        type_program: Arc<crate::syntax::arena::ArenaProgram>,
    ) {
        #[cfg(test)]
        generic_tests::record_source_check_pass();
        self.diagnostics.extend(Self::prepare_regex_literals(program));
        self.record_constructors = RecordConstructors::collect(program);
        self.prepared_constants = super::constants::PreparedConstants::collect(program, &self.record_constructors);
        self.diagnostics.extend(self.prepared_constants.diagnostics.clone());
        self.record_constructors.apply_prepared_defaults(program, &self.prepared_constants);
        for (expression, fact) in &self.prepared_constants.record_constructor_instances {
            self.record_constructor_instances.insert(program.arena.expr(*expression).span, fact.clone());
        }
        let (wire_enums, wire_diagnostics) = crate::sema::wire_enums::PreparedWireEnums::prepare(program, |expr|
            self.prepared_constants.analyze_expression(&program.arena, expr));
        self.wire_enums = wire_enums;
        self.diagnostics.extend(wire_diagnostics);
        self.collect_user_modules_arena(program, type_program.clone(), source);
        self.collect_type_imports_arena(program, program.statement_ids());
        self.collect_definitions_arena(program, type_program, source, program.statement_ids());
        self.prepare_graph_components(program);
        for stmt in program.statement_ids() {
            self.check_stmt_arena(program, source, stmt);
        }
        self.resolve_checked_types();
        let (cli_entry, diagnostics) = crate::sema::cli_entry::validate_cli_entry(program,
            |parameter| self.checked_parameter_type(program, parameter).unwrap_or_else(|| self.record_constructors.resolve_type(&program.arena, parameter.ty, None)),
            |ty| self.record_constructors.cli_parser_type(&program.arena, ty),
            |expr| self.prepared_constants.analyze_expression(&program.arena, expr));
        self.cli_entry = cli_entry;
        self.diagnostics.extend(diagnostics);
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
                "check.missing-module-doc",
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
                "check.duplicate-module-doc",
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
                "check.orphan-doc-comment",
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
                    "check.missing-public-doc",
                );
            }
        }
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
                    "check.effect-violation",
                );
            }
            Some(callee_effs) => {
                for eff in callee_effs {
                    if *eff == Effect::Error && self.retry_attempt_depth > 0 {
                        continue;
                    }
                    if !Self::effects_covers(caller_effs, eff) {
                        self.error(
                            span,
                            &format!(
                                "effect `{}` required by `{callee_name}` is not in caller's declared effects",
                                eff.as_str()
                            ),
                            "check.effect-violation",
                        );
                    }
                }
            }
        }
    }

    fn lookup(&self, name: Name) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|scope| scope.get(&name))
    }

    fn define(&mut self, name: Name, binding: Binding, span: Span) {
        self.check_standard_module_shadow(&name.as_str(), span);
        self.current_scope_mut().insert(name, binding);
    }

    fn define_builtin_value(&mut self, name: &str, binding: Binding) {
        self.current_scope_mut().insert(Name::intern(name), binding);
    }

    pub(crate) fn check_standard_module_shadow(&mut self, name: &str, span: Span) {
        if name == "args" {
            if self.scopes.len() == 1 {
                self.error(
                    span,
                    "name `args` shadows the built-in script-arguments binding",
                    "check.standard-module-shadow",
                );
            }
            return;
        }
        if name != "error" && api_spec().is_standard_module(name) {
            let message = format!("name `{name}` shadows the standard module `{name}`");
            self.error(span, &message, "check.standard-module-shadow");
        }
    }

    fn render_reveal_requests(&mut self, solved: &SolvedTypes) {
        let query = crate::frontend::query::SolvedQuery::new(solved, solved.symbol_owner());
        for (identity, span) in std::mem::take(&mut self.reveal_requests) {
            let message = match query.reveal(identity) {
                Ok(rendered) => format!("revealed type: {rendered}"),
                Err(_) => "revealed type is unavailable because no solved source contract was established".to_string(),
            };
            self.reveal_types.push(Diagnostic::new(Severity::Note, message)
                .with_code("check.reveal-type")
                .with_label(Label::primary(span, "expression has this checked type")));
        }
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

    pub(crate) fn error(&mut self, span: Span, message: &str, code: &str) {
        self.diagnostics.push(
            Diagnostic::error(message)
                .with_code(code)
                .with_label(Label::primary(span, message)),
        );
    }

    pub(crate) fn warning(&mut self, span: Span, message: &str, code: &str) {
        self.diagnostics.push(
            Diagnostic::warning(message)
                .with_code(code)
                .with_label(Label::primary(span, message)),
        );
    }
}
