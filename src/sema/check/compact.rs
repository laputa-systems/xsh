#![allow(clippy::single_call_fn)]

use super::decl::is_builtin_or_standard_record_type_name;
use super::{
    BTreeMap, BinaryOp, Checker, CoreCommand, Diagnostic, ErrorFamilyInfo, ErrorVariantInfo,
    FxHashMap, FxHashSet, Label, Name, TagVariantInfo, Type, UnaryOp, api_spec,
};
use crate::sema::types::{CallableParamType, CallableType, ModuleExportType};
use crate::symbol::QualifiedName;
use crate::symbol::Symbol;
use crate::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBlock, ArenaBuilderEntryKind, ArenaCommand,
    ArenaCommandArgKind, ArenaErrorDef, ArenaExprKind, ArenaExprOrRun,
    ArenaModuleContractEntryKind, ArenaPatternKind, ArenaPipeStageKind, ArenaProgram, ArenaRecordFieldKind,
    ArenaStmtKind, ArenaTypeDef, ArenaTypeDefBody, ArenaTypeExprTag, AstArena, BlockId, ErrorDefId,
    ExprId, FunctionDefId, PatternId, StmtId, TypeDefId, TypeExprId,
};
use crate::syntax::node::{Effect, EnvGetKind};

#[derive(Clone, Debug, Default)]
pub struct CompactDeclOutput {
    // Namespace and source identity distinguish equal offsets in imported units.
    pub stream_stage_types: BTreeMap<(Option<Name>, crate::source::Span), super::CheckedStreamStage>,
    pub static_callable_aliases: BTreeMap<crate::source::Span, super::StaticCallableAlias>,
    pub local_binding_types: BTreeMap<crate::source::Span, Type>,
    pub record_constructors: super::RecordConstructors,
    pub record_constructor_types: FxHashMap<ExprId, Type>,
    pub requirement_targets: FxHashMap<ExprId, super::RequirementTarget>,
    pub checked_expr_types: FxHashMap<ExprId, Type>,
    pub prepared_constants: crate::sema::constants::PreparedConstants,
    pub wire_enums: crate::sema::wire_enums::PreparedWireEnums,
    pub(crate) cli_entry: Option<crate::sema::cli_entry::CliEntryPlan>,
    pub diagnostics: Vec<Diagnostic>,
    pub function_return_types: BTreeMap<crate::source::Span, Type>,
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
    Record(BTreeMap<Name, Type>),
    Module(BTreeMap<Name, ModuleExportType>),
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

#[derive(Clone, Debug, Default)]
pub struct CompactBodyProbeOutput {
    /// Return types of checked ordinary one-item calls, keyed by their source descriptor.
    pub stage_callable_types: FxHashMap<ExprId, Type>,
    pub diagnostics: Vec<Diagnostic>,
    pub statements: usize,
    pub supported_statements: usize,
    pub unsupported_statements: usize,
    pub expressions: usize,
    pub typed_expressions: usize,
    pub unsupported_expressions: usize,
    pub bindings: usize,
    pub assignment_targets: usize,
    pub blocks: usize,
    pub functions: usize,
    pub commands: usize,
    pub runs: usize,
    pub unsupported_signal_hooks: usize,
    pub unsupported_with_stmts: usize,
    pub unsupported_guards: usize,
    pub unsupported_guarded_stmts: usize,
    pub unsupported_item_exprs: usize,
    pub unsupported_list_comps: usize,
    pub unsupported_map_comps: usize,
    pub unsupported_match_exprs: usize,
    pub unsupported_pipeline_exprs: usize,
    pub unsupported_structured_pipeline_exprs: usize,
    pub unsupported_builder_call_exprs: usize,
    pub expr_types: FxHashMap<ExprId, Type>,
    pub assertion_spans: std::collections::BTreeSet<crate::source::Span>,
    pub proven_nonnull_fallback_receivers: FxHashSet<ExprId>,
    pub projections: FxHashMap<ExprId, crate::sema::projection::CheckedProjection>,
    pub requirement_targets: FxHashMap<ExprId, super::RequirementTarget>,
    pub statement_positions: FxHashMap<StmtId, super::StatementPosition>,
    pub block_types: FxHashMap<BlockId, Type>,
    // Keep inferred value tails separate from contextual Unit consumption.
    pub value_block_types: FxHashMap<ExprId, Type>,
    // Handler slots retain the common nominal error of their checked inputs.
    pub handler_input_types: FxHashMap<BlockId, Type>,
}

impl Checker {
    // Compact execution must pass the same checked boundaries as normal source
    // checking before representation probes can prepare runtime frames.
    pub fn check_compact_declarations(program: &ArenaProgram) -> CompactDeclOutput {
        program.symbol_owner().with_current(|| {
            // Entry checking owns the `reveal_type` gate; `xsht check` admits it
            // and runtime lowering skips it, so this replay must not reject it.
            let options = super::CheckOptions { reveal_types: true, ..super::CheckOptions::default() };
            let checked = Checker::check_arena_with_options(program, "", options);
            let mut collector = CompactDeclCollector {
                diagnostics: Vec::new(),
                names: FxHashSet::default(),
                output: CompactDeclOutput {
                    record_constructors: super::RecordConstructors::collect(program),
                    requirement_targets: (0..program.arena.expr_tags.len()).filter_map(|index| {
                        let id = ExprId::from_index(index);
                        checked.requirement_targets.get(&program.arena.expr(id).span).cloned().map(|target| (id, target))
                    }).collect(),
                    checked_expr_types: (0..program.arena.expr_tags.len()).filter_map(|index| {
                        let id = ExprId::from_index(index);
                        checked.expr_types.get(&program.arena.expr(id).span).cloned().map(|ty| (id, ty))
                    }).collect(),
                    function_effect_facts: checked.function_effect_facts.clone(),
                    record_constructor_types: (0..program.arena.expr_tags.len()).filter_map(|index| {
                        let expression = program.arena.expr(ExprId::from_index(index));
                        let ArenaExprKind::Call { callee, .. } = expression.kind else { return None; };
                        checked.record_constructor_instances.get(&expression.span).map(|fact| (callee, fact.ty.clone()))
                    }).collect(),
                    stream_stage_types: checked.stream_stage_types.clone(),
                    static_callable_aliases: checked.static_callable_aliases.clone(),
                    parameter_types: checked.parameter_types.clone(),
                    local_binding_types: checked.local_binding_types.clone(),
                    function_return_types: checked.function_return_types.clone(),
                    ..CompactDeclOutput::default()
                },
            };
            collector.output.error_families_by_name = Checker::new(super::CheckOptions::default()).error_families;
            collector.diagnostics.extend(Self::prepare_regex_literals(program));
            collector.collect_program(program);
            let mut output = collector.output;
            output.prepared_constants = crate::sema::constants::PreparedConstants::collect(program, &output.record_constructors);
            collector.diagnostics.extend(output.prepared_constants.diagnostics.clone());
            output.record_constructors.apply_prepared_defaults(program, &output.prepared_constants);
            let (wire_enums, wire_diagnostics) = crate::sema::wire_enums::PreparedWireEnums::prepare(program, |expr|
                output.prepared_constants.analyze_expression(&program.arena, expr));
            output.wire_enums = wire_enums;
            collector.diagnostics.extend(wire_diagnostics);
            let (entry, diagnostics) = crate::sema::cli_entry::validate_cli_entry(program,
                |parameter| output.parameter_types.get(&program.arena.span(parameter.span)).cloned().unwrap_or_else(|| output.record_constructors.resolve_type(&program.arena, parameter.ty, None)),
                |ty| output.record_constructors.cli_parser_type(&program.arena, ty),
                |expr| output.prepared_constants.analyze_expression(&program.arena, expr));
            output.cli_entry = entry;
            collector.diagnostics.extend(diagnostics);
            output.diagnostics = collector.diagnostics;
            output.diagnostics.extend(checked.diagnostics.into_iter().filter(|diagnostic|
                diagnostic.severity == crate::diagnostic::Severity::Error));
            output
        })
    }

    pub fn probe_compact_bodies(
        program: &ArenaProgram,
        declarations: &CompactDeclOutput,
    ) -> CompactBodyProbeOutput {
        program.symbol_owner().with_current(|| {
            let mut output = CompactBodyProbeOutput::default();
            // Almost every non-trivial script has typed expressions, and the arena
            // already knows the exact expression count, so this is a precise upper
            // bound (not every expression ends up typed) that avoids repeated growth
            // reallocations of the type-fact map without guessing.
            output.expr_types.reserve(program.stats().expressions);
            let mut probe = CompactBodyProbe {
                type_constraints: super::super::constraints::TypeConstraints::default(),
                nonmaterial_expressions: FxHashSet::default(),
                expected_schema: None,
                program,
                declarations,
                output,
                scopes: vec![FxHashMap::default()],
                condition_proofs: FxHashMap::default(),
                stream_items: Vec::new(),
                return_types: Vec::new(),
                return_schemas: Vec::new(),
                pipeline_hole_types: FxHashMap::default(),
                current_namespace: None,
                with_initializer_errors: None,
            };
            probe.seed_declarations();
            probe.check_compact_program();
            // The general checker owns statement-use classification, including
            // contextual tails and narrowing; the execution probe carries its facts.
            probe.output.assertion_spans = Checker::check_arena(program, "").assertion_spans;
            probe.resolve_checked_types();
            probe.output
        })
    }
}

struct CompactDeclCollector {
    diagnostics: Vec<Diagnostic>,
    names: FxHashSet<Name>,
    output: CompactDeclOutput,
}

impl CompactDeclCollector {
    fn collect_program(&mut self, program: &ArenaProgram) {
        for stmt in program.statement_ids() {
            self.collect_decl_stmt(program, stmt, None);
        }
        for module in &program.modules {
            // Declaration collisions belong to one lexical module. Separate
            // modules may export distinct schemas with the same local name.
            let entry_names = std::mem::take(&mut self.names);
            for stmt in program.module_statements(module) {
                self.collect_decl_stmt(program, stmt, Some(module.name));
            }
            self.names = entry_names;
        }
    }

    fn collect_decl_stmt(&mut self, program: &ArenaProgram, id: StmtId, namespace: Option<Name>) {
        let stmt = program.arena.stmt(id);
        let span = stmt.span;
        match stmt.kind {
            ArenaStmtKind::Export(inner) => self.collect_decl_stmt(program, inner, namespace),
            ArenaStmtKind::TypeDef(def) => self.collect_type_def(program, def, span, namespace),
            ArenaStmtKind::ErrorDef(def) => self.collect_error_def(program, def, span, namespace),
            ArenaStmtKind::ProcDef(def) => {
                self.collect_function_def(program, def, CompactFunctionKind::Proc, span, namespace);
            }
            ArenaStmtKind::CliMain(def) => {
                self.output.function_defs += 1;
                self.output.params += program.arena.params(program.arena.function_def(def).params).len();
            }
            ArenaStmtKind::PureDef(def) => {
                self.collect_function_def(program, def, CompactFunctionKind::Pure, span, namespace);
            }
            ArenaStmtKind::StreamDef(def) => {
                self.collect_function_def(
                    program,
                    def,
                    CompactFunctionKind::Stream,
                    span,
                    namespace,
                );
            }
            _ => {}
        }
    }

    fn collect_type_def(
        &mut self,
        program: &ArenaProgram,
        id: TypeDefId,
        span: crate::source::Span,
        namespace: Option<Name>,
    ) {
        let def = program.arena.type_def(id);
        self.output.type_defs += 1;
        let internal = namespace.is_some_and(|namespace| program.is_internal_namespace(namespace));
        if !internal {
            self.check_top_level_name(def.name, span, "type name conflicts with a built-in type");
        }
        let info = self.collect_type_def_body(program, def, namespace);
        if !internal {
            self.output.types.insert(def.name, info);
        }
    }

    fn collect_type_def_body(
        &mut self,
        program: &ArenaProgram,
        def: &ArenaTypeDef,
        namespace: Option<Name>,
    ) -> CompactTypeDefInfo {
        match def.body {
            ArenaTypeDefBody::Alias(ty) => CompactTypeDefInfo::Alias(ty),
            ArenaTypeDefBody::RecordSchema(fields) => {
                let mut names = FxHashSet::default();
                let fields = program.arena.schema_fields(fields);
                self.output.schema_fields += fields.len();
                let mut record = BTreeMap::new();
                let mut schema_fields = BTreeMap::new();
                for field in fields {
                    if !names.insert(field.name) {
                        self.error(
                            program.arena.span(field.span),
                            "duplicate schema field",
                            "check.duplicate-record-field",
                        );
                    }
                    record.insert(field.name, Type::from_arena(&program.arena, field.ty));
                    schema_fields.insert(field.name, field.ty);
                }
                if !namespace.is_some_and(|name| program.is_internal_namespace(name)) {
                    self.output
                        .record_schema_fields
                        .insert(def.name, schema_fields);
                }
                CompactTypeDefInfo::Record(record)
            }
            ArenaTypeDefBody::ModuleContract(entries) => {
                let mut names = FxHashSet::default();
                let entries = program.arena.module_contract_entries(entries);
                self.output.module_contract_entries += entries.len();
                let mut exports = BTreeMap::new();
                for entry in entries {
                    if !names.insert(entry.name) {
                        self.error(
                            program.arena.span(entry.span),
                            "duplicate module contract export",
                            "check.duplicate-name",
                        );
                    }
                    match entry.kind {
                        ArenaModuleContractEntryKind::Value(ty) => {
                            exports.insert(
                                entry.name,
                                ModuleExportType::Value {
                                    ty: Type::from_arena(&program.arena, ty),
                                    optional: entry.optional,
                                },
                            );
                        }
                        ArenaModuleContractEntryKind::Proc {
                            params,
                            effects,
                            return_ty,
                        } => {
                            let sig = self.callable_type(program, params, return_ty, effects);
                            exports.insert(
                                entry.name,
                                ModuleExportType::Proc {
                                    sig,
                                    optional: entry.optional,
                                },
                            );
                        }
                        ArenaModuleContractEntryKind::Pure { params, return_ty } => {
                            let sig = self.callable_type(program, params, return_ty, None);
                            exports.insert(
                                entry.name,
                                ModuleExportType::Pure {
                                    sig,
                                    optional: entry.optional,
                                },
                            );
                        }
                    }
                }
                CompactTypeDefInfo::Module(exports)
            }
            ArenaTypeDefBody::TagUnion(variants) => {
                let variants = program.arena.tag_variants(variants);
                self.output.tag_variants += variants.len();
                for variant in variants {
                    let mut field_types = Vec::with_capacity(variant.fields.len());
                    for raw in program.arena.extra_range(variant.fields) {
                        field_types.push(Type::from_arena(
                            &program.arena,
                            crate::syntax::arena::TypeExprId::from_index(*raw as usize),
                        ));
                    }
                    let info = TagVariantInfo {
                        type_name: crate::sema::wire_enums::nominal_enum_name(namespace.or(program.root_nominal_namespace), def.name),
                        field_count: field_types.len(),
                        field_types,
                    };
                    if let Some(namespace) = namespace {
                        self.output
                            .qualified_tag_variants
                            .insert(QualifiedName::new(namespace, variant.name), info);
                    } else {
                        self.output.tag_variants_by_name.insert(variant.name, info);
                    }
                }
                CompactTypeDefInfo::TagUnion
            }
        }
    }

    fn collect_error_def(
        &mut self,
        program: &ArenaProgram,
        id: ErrorDefId,
        span: crate::source::Span,
        namespace: Option<Name>,
    ) {
        let def = program.arena.error_def(id);
        self.output.error_families += 1;
        if !namespace.is_some_and(|namespace| program.is_internal_namespace(namespace)) {
            self.check_top_level_name(
                def.name,
                span,
                "error family name conflicts with a built-in type",
            );
        }
        self.collect_error_variants(program, def, namespace);
    }

    fn collect_error_variants(
        &mut self,
        program: &ArenaProgram,
        def: &ArenaErrorDef,
        namespace: Option<Name>,
    ) {
        let mut variants = FxHashSet::default();
        let error_variants = program.arena.error_variants(def.variants);
        self.output.error_variants += error_variants.len();
        let mut family_variants = BTreeMap::new();
        for variant in error_variants {
            if !variants.insert(variant.name) {
                self.error(
                    program.arena.span(variant.span),
                    "duplicate error variant",
                    "check.duplicate-name",
                );
            }
            let fields = program.arena.error_fields(variant.fields);
            self.output.error_fields += fields.len();
            let mut field_names = FxHashSet::default();
            let mut field_types = BTreeMap::new();
            for field in fields {
                if !field_names.insert(field.name) {
                    self.error(
                        program.arena.span(field.span),
                        "duplicate error payload field",
                        "check.duplicate-record-field",
                    );
                }
                field_types.insert(field.name, Type::from_arena(&program.arena, field.ty));
            }
            let facets = program.arena.names(variant.facets).collect::<Vec<_>>();
            family_variants.insert(
                variant.name,
                ErrorVariantInfo {
                    fields: field_types,
                    facets,
                },
            );
        }
        let info = ErrorFamilyInfo {
            variants: family_variants,
        };
        if let Some(namespace) = namespace {
            self.output
                .qualified_error_families
                .insert(QualifiedName::new(namespace, def.name), info.clone());
        }
        self.output.error_families_by_name.insert(def.name, info);
    }

    fn collect_function_def(
        &mut self,
        program: &ArenaProgram,
        id: FunctionDefId,
        kind: CompactFunctionKind,
        span: crate::source::Span,
        namespace: Option<Name>,
    ) {
        let def = program.arena.function_def(id);
        let internal = namespace.is_some_and(|namespace| program.is_internal_namespace(namespace));
        self.output.function_defs += 1;
        if !internal {
            self.check_standard_module_shadow(&def.name.as_str(), span);
            if kind == CompactFunctionKind::Proc
                && CoreCommand::from_name(&def.name.as_str()).is_some()
            {
                self.error(
                    span,
                    "proc name conflicts with a core command",
                    "check.core-command-shadow",
                );
            }
        }
        if !self.names.insert(def.name) {
            self.error(span, "duplicate top-level name", "check.duplicate-name");
        }
        let sig = self.function_sig(program, id, namespace);
        if let Some(namespace) = namespace {
            let qualified = QualifiedName::new(namespace, def.name);
            match kind {
                CompactFunctionKind::Proc => {
                    self.output.qualified_procs.insert(qualified, sig.clone());
                }
                CompactFunctionKind::Pure => {
                    self.output.qualified_pures.insert(qualified, sig.clone());
                }
                CompactFunctionKind::Stream => {
                    self.output.qualified_streams.insert(qualified, sig.clone());
                }
            }
        }
        if internal {
            // An embedded helper is callable only through its owning
            // implementation namespace, so it must not join the unqualified
            // tables that serve ordinary top-level calls.
            return;
        }
        match kind {
            CompactFunctionKind::Proc => {
                self.output.procs.insert(def.name, sig);
            }
            CompactFunctionKind::Pure => {
                self.output.pures.insert(def.name, sig);
            }
            CompactFunctionKind::Stream => {
                self.output.streams.insert(def.name, sig);
            }
        }
    }

    fn function_sig(&mut self, program: &ArenaProgram, id: FunctionDefId, namespace: Option<Name>) -> CompactFunctionSig {
        let def = program.arena.function_def(id);
        let mut params = self.param_sigs(program, def.params);
        let parameter_schemas = program.arena.params(def.params).iter().zip(&mut params).map(|(syntax, param)| {
            param.ty = self.output.record_constructors.resolve_type(&program.arena, syntax.ty, namespace);
            self.output.record_constructors.annotation_expectation(&program.arena, syntax.ty, namespace).ok()
        }).collect();
        let body_span = program.arena.span(program.arena.block(def.body).span);
        let return_ty = self.output.function_return_types.get(&body_span).cloned()
            .unwrap_or_else(|| Type::from_arena(&program.arena, def.return_ty));
        let effects = self.output.function_effect_facts.get(&super::EffectDeclarationId { namespace, body: body_span })
            .map(|fact| fact.effective.clone())
            .unwrap_or_else(|| def.effects.map(|effects| program.arena.effects(effects).collect::<Vec<_>>()));
        CompactFunctionSig {
            params,
            parameter_schemas,
            return_ty,
            return_type_expr: def.return_ty,
            inferred_effects: self.output.function_effect_facts.get(&super::EffectDeclarationId { namespace, body: body_span }).is_some_and(|fact| fact.inferred),
            effects,
        }
    }

    fn callable_type(
        &mut self,
        program: &ArenaProgram,
        params: crate::syntax::arena::ArenaRange,
        return_ty: crate::syntax::arena::TypeExprId,
        effects: Option<crate::syntax::arena::ArenaRange>,
    ) -> CallableType {
        CallableType {
            params: self.param_sigs(program, params),
            return_ty: Box::new(Type::from_arena(&program.arena, return_ty)),
            effects: effects.map(|effects| program.arena.effects(effects).collect::<Vec<_>>()),
        }
    }

    fn param_sigs(
        &mut self,
        program: &ArenaProgram,
        params: crate::syntax::arena::ArenaRange,
    ) -> Vec<CallableParamType> {
        let params = program.arena.params(params);
        self.output.params += params.len();
        params
            .iter()
            .map(|param| CallableParamType {
                name: param.name,
                ty: self.output.parameter_types.get(&program.arena.span(param.span)).cloned().unwrap_or_else(|| Type::from_arena(&program.arena, param.ty)),
                defaulted: param.default.is_some(),
                rest: param.rest,
            })
            .collect()
    }

    fn check_top_level_name(
        &mut self,
        name: Name,
        span: crate::source::Span,
        builtin_message: &str,
    ) {
        if is_builtin_or_standard_record_type_name(name.as_str()) {
            self.error(span, builtin_message, "check.duplicate-name");
        }
        self.check_standard_module_shadow(&name.as_str(), span);
        if !self.names.insert(name) {
            self.error(span, "duplicate top-level name", "check.duplicate-name");
        }
    }

    fn check_standard_module_shadow(&mut self, name: &str, span: crate::source::Span) {
        if name == "args" {
            return;
        }
        if name != "error" && api_spec().is_standard_module(name) {
            let message = format!("name `{name}` shadows the standard module `{name}`");
            self.error(span, &message, "check.standard-module-shadow");
        }
    }

    fn error(&mut self, span: crate::source::Span, message: &str, code: &str) {
        self.diagnostics.push(
            Diagnostic::error(message)
                .with_code(code)
                .with_label(Label::primary(span, message)),
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CompactFunctionKind {
    Proc,
    Pure,
    Stream,
}

/// Checks executable bodies directly from arena rows. The `check_compact_*`
/// method family distinguishes this probe from the general `Checker` paths.
struct CompactBodyProbe<'a> {
    expected_schema: Option<crate::sema::constants::SchemaExpectation>,
    type_constraints: super::super::constraints::TypeConstraints,
    nonmaterial_expressions: FxHashSet<ExprId>,
    with_initializer_errors: Option<Vec<Type>>,
    current_namespace: Option<Name>,
    program: &'a ArenaProgram,
    declarations: &'a CompactDeclOutput,
    output: CompactBodyProbeOutput,
    scopes: Vec<FxHashMap<Name, CompactBinding>>,
    condition_proofs: FxHashMap<ExprId, std::sync::Arc<super::proof::ConditionNarrowings>>,
    stream_items: Vec<Type>,
    return_types: Vec<Type>,
    return_schemas: Vec<Option<crate::sema::constants::SchemaExpectation>>,
    pipeline_hole_types: FxHashMap<ExprId, Type>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompactBinding {
    ty: Type,
    mutable: bool,
    unrefined_ty: Option<Type>,
    proof: super::proof::BindingProof,
    boolean_proof: Option<std::sync::Arc<super::proof::ConditionNarrowings>>,
}

impl CompactBinding {
    fn new(ty: Type, mutable: bool) -> Self {
        Self { ty, mutable, unrefined_ty: None, proof: super::proof::BindingProof::default(), boolean_proof: None }
    }
}

impl CompactBodyProbe<'_> {
    fn record_inert_discard(&mut self, expression: ArenaExprOrRun) {
        if self.lookup_binding(Name::intern("map")).is_none() && super::local_inference::empty_map_call(self.program, expression)
            && let ArenaExprOrRun::Expr(expression) = expression {
            self.nonmaterial_expressions.insert(expression);
        }
    }

    fn resolve_checked_types(&mut self) {
        let mut reported = std::collections::BTreeSet::new();
        self.output.expr_types.retain(|expression, ty| {
            !self.nonmaterial_expressions.contains(expression)
                || !matches!(self.type_constraints.resolve(ty), Ok(resolved) if resolved.contains_inference())
        });
        for (expression, projection) in &mut self.output.projections {
            super::local_inference::finalize_projection(&self.type_constraints, projection, self.program.arena.expr(*expression).span, &mut reported, &mut self.output.diagnostics);
        }
        for (expression, ty) in &mut self.output.stage_callable_types {
            super::local_inference::finalize_type(&self.type_constraints, ty, self.program.arena.expr(*expression).span, &mut reported, &mut self.output.diagnostics);
        }
        for (expression, ty) in &mut self.output.expr_types {
            super::local_inference::finalize_type(&self.type_constraints, ty, self.program.arena.expr(*expression).span, &mut reported, &mut self.output.diagnostics);
        }
        for (block, ty) in &mut self.output.block_types {
            let tail = self.program.arena.stmt_ids(self.program.arena.block(*block).statements).last();
            if tail.is_some_and(|tail| self.output.statement_positions.get(&tail) == Some(&super::StatementPosition::Statement)
                && matches!(self.program.arena.stmt(tail).kind, ArenaStmtKind::Expr(expression) if self.nonmaterial_expressions.contains(&expression))) {
                *ty = Type::Unit;
            }
            super::local_inference::finalize_type(&self.type_constraints, ty, self.program.arena.span(self.program.arena.block(*block).span), &mut reported, &mut self.output.diagnostics);
        }
        for (expression, ty) in &mut self.output.value_block_types {
            super::local_inference::finalize_type(&self.type_constraints, ty, self.program.arena.expr(*expression).span, &mut reported, &mut self.output.diagnostics);
        }
        for (block, ty) in &mut self.output.handler_input_types {
            super::local_inference::finalize_type(&self.type_constraints, ty, self.program.arena.span(self.program.arena.block(*block).span), &mut reported, &mut self.output.diagnostics);
        }
        for scope in &mut self.scopes {
            for binding in scope.values_mut() {
                binding.ty = self.type_constraints.resolve(&binding.ty).unwrap_or(Type::Invalid);
                if binding.ty.contains_inference() { binding.ty = Type::Invalid; }
                if let Some(ty) = &mut binding.unrefined_ty {
                    *ty = self.type_constraints.resolve(ty).unwrap_or(Type::Invalid);
                    if ty.contains_inference() { *ty = Type::Invalid; }
                }
            }
        }
    }

    fn compact_subject(&self, mut expr: ExprId) -> Option<(Name, Vec<Name>, Type)> {
        let mut path = Vec::new();
        loop {
            match self.program.arena.expr(expr).kind {
                ArenaExprKind::Ident(name) => {
                    path.reverse();
                    let ty = super::proof::projected_type(&self.lookup_binding(name)?.ty, &path)?.clone();
                    return Some((name, path, ty));
                }
                ArenaExprKind::Field { base, name } if path.len() < 128 => { path.push(name); expr = base; }
                _ => return None,
            }
        }
    }

    fn compact_condition_proof(&self, expr: ExprId) -> super::proof::ConditionNarrowings {
        let mut proof = self.condition_proofs.get(&expr).map(|proof| proof.as_ref().clone())
            .unwrap_or_else(|| self.compact_condition_proof_inner(expr));
        let valid = |fact: &super::proof::Narrowing| self.lookup_binding(fact.name).is_some_and(|binding| binding.proof.accepts(fact));
        proof.when_true.retain(valid); proof.when_false.retain(valid);
        proof
    }

    fn compact_condition_proof_inner(&self, expr: ExprId) -> super::proof::ConditionNarrowings {
        use super::proof::ConditionNarrowings as C;
        let true_fact = |name, path, ty| C { when_true: vec![self.lookup_binding(name).unwrap().proof.fact(name, path, ty)], when_false: Vec::new() };
        match self.program.arena.expr(expr).kind {
            ArenaExprKind::Ident(name) => self.lookup_binding(name).and_then(|binding| binding.boolean_proof.as_ref()).map(|proof| proof.as_ref().clone()).unwrap_or_default(),
            ArenaExprKind::Unary { op: UnaryOp::Not, expr } => {
                let proof = self.compact_condition_proof(expr); C { when_true: proof.when_false, when_false: proof.when_true }
            }
            ArenaExprKind::Binary { op: BinaryOp::And, left, right } => self.compact_condition_proof(left).and(self.compact_condition_proof(right)),
            ArenaExprKind::Binary { op: BinaryOp::Or, left, right } => self.compact_condition_proof(left).or(self.compact_condition_proof(right)),
            ArenaExprKind::Binary { op: op @ (BinaryOp::Eq | BinaryOp::Ne), left, right } => {
                let subject = match (self.program.arena.expr(left).kind, self.program.arena.expr(right).kind) {
                    (_, ArenaExprKind::Null) => left, (ArenaExprKind::Null, _) => right, _ => return C::default(),
                };
                let Some((name, path, Type::Optional(inner))) = self.compact_subject(subject) else { return C::default(); };
                let proof = true_fact(name, path, *inner);
                if op == BinaryOp::Ne { proof } else { C { when_true: proof.when_false, when_false: proof.when_true } }
            }
            ArenaExprKind::PatternTest { value, arms } | ArenaExprKind::PatternCondition { value, arms } => {
                let Some((name, path, ty)) = self.compact_subject(value) else { return C::default(); };
                let pattern = self.program.arena.match_expr_arms(arms)[0].pattern;
                let narrowed = match self.program.arena.pattern(pattern).kind {
                    crate::syntax::arena::ArenaPatternKind::TestName { ty, .. }
                    | crate::syntax::arena::ArenaPatternKind::Type { binding: None, ty } => self.type_from_arena(ty),
                    crate::syntax::arena::ArenaPatternKind::ErrorVariant { family, variant, .. } => Type::ErrorVariant { family, variant },
                    crate::syntax::arena::ArenaPatternKind::Facet(facet) => if matches!(ty, Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ProcessError) { ty } else { Type::ErrorFacet(facet) },
                    _ => return C::default(),
                };
                true_fact(name, path, narrowed)
            }
            ArenaExprKind::Binary { op: op @ (BinaryOp::In | BinaryOp::NotIn), left, right } => {
                let Some((name, path, Type::Record(mut fields))) = self.compact_subject(right) else { return C::default(); };
                let ArenaExprKind::Str(field) = self.program.arena.expr(left).kind else { return C::default(); };
                fields.entry(Name::intern(self.program.arena.string_literal(field))).or_insert(Type::Any);
                let proof = true_fact(name, path, Type::Record(fields));
                if op == BinaryOp::In { proof } else { C { when_true: proof.when_false, when_false: proof.when_true } }
            }
            _ => C::default(),
        }
    }

    fn compact_guard_narrowings(&self, expr: ExprId, success: bool) -> Vec<super::proof::Narrowing> {
        let proof = self.compact_condition_proof(expr);
        if success { proof.when_true } else { proof.when_false }
    }

    fn apply_compact_guard_narrowings(&mut self, narrowings: Vec<super::proof::Narrowing>) {
        for fact in narrowings {
            if let Some(mut binding) = self.lookup_binding(fact.name).cloned() {
                if !binding.proof.accepts(&fact) { continue; }
                if binding.unrefined_ty.is_none() { binding.unrefined_ty = Some(binding.ty.clone()); }
                if !super::proof::replace_projection(&mut binding.ty, &fact.path, fact.ty) { continue; }
                self.current_scope_mut().insert(fact.name, binding);
            }
        }
    }

    // Compact facts are consumed only after source checking has validated these transfers.
    fn compact_block_definitely_exits(&self, block: BlockId) -> bool {
        self.program.arena.stmt_ids(self.program.arena.block(block).statements).any(|statement| {
            match self.program.arena.stmt(statement).kind {
                ArenaStmtKind::Return(_) | ArenaStmtKind::Break { .. } | ArenaStmtKind::Continue => true,
                ArenaStmtKind::Expr(expr) => self.compact_expr_definitely_exits(expr),
                ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(expr), .. }
                | ArenaStmtKind::Var { initializer: ArenaExprOrRun::Expr(expr), .. }
                | ArenaStmtKind::Assign { value: ArenaExprOrRun::Expr(expr), .. } => self.compact_expr_definitely_exits(expr),
                ArenaStmtKind::If { branches, else_block } => {
                    for branch in self.program.arena.if_branches(branches) {
                        match self.program.arena.expr(branch.condition).kind {
                            ArenaExprKind::Bool(false) => continue,
                            ArenaExprKind::Bool(true) => return self.compact_block_definitely_exits(branch.block),
                            _ if !self.compact_block_definitely_exits(branch.block) => return false,
                            _ => {}
                        }
                    }
                    else_block.is_some_and(|block| self.compact_block_definitely_exits(block))
                }
                ArenaStmtKind::With { body, else_block, .. } => self.compact_block_definitely_exits(body) && self.compact_block_definitely_exits(else_block),
                _ => false,
            }
        })
    }

    fn compact_expr_definitely_exits(&self, expr: ExprId) -> bool {
        match self.program.arena.expr(expr).kind {
            ArenaExprKind::ValueBlock(block) => self.compact_block_definitely_exits(block),
            ArenaExprKind::Call { callee, .. } => matches!(self.program.arena.expr(callee).kind,
                ArenaExprKind::Ident(name) if name == "abort" && self.lookup_binding(name).is_none()),
            _ => false,
        }
    }

    fn push_compact_deferred_capture_scope(&mut self) {
        let visible = self.scopes.iter().flat_map(|scope| scope.iter())
            .map(|(name, binding)| (*name, binding.clone())).collect::<FxHashMap<_, _>>();
        self.push_scope();
        for (name, mut binding) in visible {
            if binding.mutable {
                binding.proof.mutate(&[]);
                if let Some(original) = binding.unrefined_ty.take() { binding.ty = original; }
                self.current_scope_mut().insert(name, binding);
            }
        }
    }

    fn invalidate_compact_mutable_proofs(&mut self) {
        for scope in &mut self.scopes { for binding in scope.values_mut() {
            if binding.mutable {
                binding.proof.mutate(&[]);
                if let Some(original) = binding.unrefined_ty.take() { binding.ty = original; }
            }
        } }
    }

    fn type_from_arena(&self, id: TypeExprId) -> Type {
        let resolved = self.declarations.record_constructors.resolve_type(&self.program.arena, id, self.current_namespace);
        if !matches!(resolved, Type::Unknown | Type::Invalid) { return resolved; }
        compact_probe_type_from_arena(&self.program.arena, id, self.declarations, 0)
    }

    fn seed_declarations(&mut self) {
        let procs = self
            .declarations
            .procs
            .keys()
            .map(|name| (*name, Type::Proc));
        let pures = self
            .declarations
            .pures
            .keys()
            .map(|name| (*name, Type::Pure));
        let streams = self.declarations.streams.iter().map(|(name, sig)| {
            let item = stream_item_type(&sig.return_ty);
            (*name, Type::Stream(Box::new(item)))
        });
        let bindings = procs.chain(pures).chain(streams).collect::<Vec<_>>();
        let root = self.current_scope_mut();
        for (name, ty) in bindings {
            root.insert(name, CompactBinding::new(ty, false));
        }
    }

    fn check_compact_program(&mut self) {
        for stmt in self.program.statement_ids() {
            self.check_compact_stmt(stmt);
        }
        for module in &self.program.modules {
            self.current_namespace = Some(module.name);
            for stmt in self.program.module_statements(module) {
                self.check_compact_stmt(stmt);
            }
        }
        self.current_namespace = None;
    }

    fn check_compact_error_handler(&mut self, block: BlockId, error_ty: Type) {
        self.output.handler_input_types.insert(block, error_ty.clone());
        self.push_scope();
        for param in self.program.arena.block_params(self.program.arena.block(block).params) {
            if param.name.as_str() != "_" {
                self.current_scope_mut().insert(param.name, CompactBinding::new(error_ty.clone(), false));
            }
        }
        self.check_compact_block_in_current_scope(block);
        self.pop_scope();
    }

    fn check_compact_stmt(&mut self, id: StmtId) {
        self.check_compact_stmt_expected(id, None);
    }

    fn check_compact_stmt_expected(&mut self, id: StmtId, expected: Option<&Type>) {
        self.output.statements += 1;
        let stmt = self.program.arena.stmt(id);
        self.output.statement_positions.insert(id, super::StatementPosition::Statement);
        match stmt.kind {
            ArenaStmtKind::BooleanGuard { condition, else_block } => {
                self.output.supported_statements += 1;
                self.check_compact_expr(condition);
                let success = self.compact_guard_narrowings(condition, true);
                let failure = self.compact_guard_narrowings(condition, false);
                let success_scopes = self.scopes.clone();
                self.push_scope();
                self.apply_compact_guard_narrowings(failure);
                self.check_compact_block(else_block);
                self.pop_scope();
                self.scopes = success_scopes;
                self.apply_compact_guard_narrowings(success);
            }
            ArenaStmtKind::Use(_)
            | ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_)
            | ArenaStmtKind::Continue => {
                self.output.supported_statements += 1;
            }
            ArenaStmtKind::TailBareIdent(name) => {
                self.output.supported_statements += 1;
                if name == "_" { self.error(stmt.span, "`_` is only a whole argument placeholder in an immediate value pipeline call", "check.pipeline-hole"); }
                if let Some(proof) = self.lookup_binding(name).and_then(|binding| binding.boolean_proof.clone()) {
                    self.apply_compact_guard_narrowings(proof.when_true.clone());
                }
            }
            ArenaStmtKind::Export(inner) => {
                self.output.supported_statements += 1;
                self.check_compact_stmt(inner);
            }
            ArenaStmtKind::Let {
                target,
                ty,
                initializer,
            } | ArenaStmtKind::Const {
                target,
                ty,
                initializer,
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer,
            } => {
                self.output.supported_statements += 1;
                self.output.bindings += 1;
                let expected = ty.map(|ty| self.type_from_arena(ty)).or_else(|| self.declarations.local_binding_types.get(&stmt.span).cloned());
                let previous_schema = self.expected_schema.take();
                self.expected_schema = ty.and_then(|ty| self.declarations.record_constructors.annotation_expectation(&self.program.arena, ty, self.current_namespace).ok());
                let actual = match initializer {
                    ArenaExprOrRun::Expr(expr) => self.check_compact_expr_expected(expr, expected.as_ref()),
                    ArenaExprOrRun::Run(run) => self.check_compact_expr_or_run(ArenaExprOrRun::Run(run)),
                };
                if matches!(self.program.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name == "_") {
                    self.record_inert_discard(initializer);
                }
                self.expected_schema = previous_schema;
                let binding_ty = expected.unwrap_or(actual);
                let mutable = matches!(self.program.arena.stmt(id).kind, ArenaStmtKind::Var { .. });
                let boolean_proof = if !mutable && binding_ty == Type::Bool {
                    if let ArenaExprOrRun::Expr(expr) = initializer {
                        if let ArenaExprKind::Ident(name) = self.program.arena.expr(expr).kind { self.lookup_binding(name).and_then(|binding| binding.boolean_proof.clone()) }
                        else { Some(std::sync::Arc::new(self.compact_condition_proof(expr))) }
                    } else { None }
                } else { None };
                self.define_binding_target(target, binding_ty, mutable);
                if let ArenaBindingTargetKind::Name(name) = self.program.arena.binding_target(target).kind {
                    if let Some(binding) = self.current_scope_mut().get_mut(&name) { binding.boolean_proof = boolean_proof; }
                }
            }
            ArenaStmtKind::Assign { target, value, .. } => {
                self.output.supported_statements += 1;
                self.check_compact_assign_target(target, stmt.span);
                let expected = self.compact_assign_target_type(target);
                match value {
                    ArenaExprOrRun::Expr(expr) => { self.check_compact_expr_expected(expr, expected.as_ref()); }
                    ArenaExprOrRun::Run(run) => { self.check_compact_expr_or_run(ArenaExprOrRun::Run(run)); }
                }
                if let Some(name) = self.assign_target_root_name(target) {
                    let mut path = Vec::new(); let mut current = target;
                    loop { match self.program.arena.assign_target(current).kind {
                        ArenaAssignTargetKind::Field { base, name } if path.len() < 128 => { path.push(name); current = base; }
                        ArenaAssignTargetKind::Name(_) => { path.reverse(); break; }
                        _ => { path.clear(); break; }
                    } }
                    if let Some(identity) = self.lookup_binding(name).map(|binding| binding.proof.clone()) {
                        for scope in &mut self.scopes { for binding in scope.values_mut().filter(|binding| binding.proof.same_binding(&identity)) {
                            if binding.mutable {
                                binding.proof.mutate(&path);
                                if let Some(original) = &binding.unrefined_ty { super::proof::restore_projection(&mut binding.ty, original, &path); }
                                if path.is_empty() { binding.unrefined_ty = None; }
                            }
                        } }
                    }
                }
            }
            ArenaStmtKind::ProcDef(def)
            | ArenaStmtKind::CliMain(def)
            | ArenaStmtKind::PureDef(def)
            | ArenaStmtKind::StreamDef(def) => {
                self.output.supported_statements += 1;
                let previous_errors = self.with_initializer_errors.take();
                self.check_compact_function(def);
                self.with_initializer_errors = previous_errors;
            }
            ArenaStmtKind::Return(value) => {
                self.output.supported_statements += 1;
                if let Some(value) = value {
                    let expected = self.return_types.last().cloned();
                    match value {
                        ArenaExprOrRun::Expr(expr) => {
                            let previous = self.expected_schema.clone();
                            self.expected_schema = self.return_schemas.last().cloned().flatten();
                            let explicit_result = super::stmt::tail_expr_uses_result_context_arena(self.program, expr);
                            let context = expected.as_ref().map(|ty| if explicit_result { ty } else { ty.result_ok().unwrap_or(ty) });
                            if !explicit_result && expected.as_ref().is_some_and(Type::is_result) { self.expected_schema = self.expected_schema.as_ref().and_then(|schema| schema.children.get(&crate::sema::constants::SchemaComponent::Success)).cloned(); }
                            self.check_compact_expr_expected(expr, context);
                            self.expected_schema = previous;
                        }
                        ArenaExprOrRun::Run(run) => { self.check_compact_expr_or_run(ArenaExprOrRun::Run(run)); }
                    }
                }
            }
            ArenaStmtKind::Defer(ArenaExprOrRun::Expr(expr))
                if matches!(self.program.arena.expr(expr).kind, ArenaExprKind::ValueBlock(_)) =>
            {
                self.output.supported_statements += 1;
                let ArenaExprKind::ValueBlock(block) = self.program.arena.expr(expr).kind else { unreachable!() };
                let previous_errors = self.with_initializer_errors.take();
                let saved_scopes = self.scopes.clone();
                self.push_compact_deferred_capture_scope();
                self.check_compact_block(block);
                self.scopes = saved_scopes;
                self.with_initializer_errors = previous_errors;
                self.mark_tail_position(block, false);
                self.output.expr_types.insert(expr, Type::Unit);
            }
            ArenaStmtKind::Expr(expr) if matches!(self.program.arena.expr(expr).kind, ArenaExprKind::ErrorContext { .. }) => {
                self.output.supported_statements += 1;
                let ArenaExprKind::ErrorContext { message, block } = self.program.arena.expr(expr).kind else { unreachable!() };
                self.check_compact_expr(message);
                self.check_compact_block(block);
                self.mark_tail_position(block, false);
                self.output.expr_types.insert(expr, Type::Unit);
            }
            ArenaStmtKind::YieldDelegate(value) => {
                self.output.supported_statements += 1;
                self.check_compact_expr(value);
            }
            ArenaStmtKind::Yield(value) | ArenaStmtKind::Defer(value) => {
                self.output.supported_statements += 1;
                self.check_compact_expr_or_run(value);
            }
            ArenaStmtKind::If { branches, else_block } => {
                self.output.supported_statements += 1;
                let initial_scopes = self.scopes.clone();
                let initial = self.scopes.iter().flat_map(|scope| scope.iter()).map(|(name, binding)| (*name, binding.clone())).collect::<FxHashMap<_, _>>();
                let mut reaching = Vec::new(); let mut failure = Vec::new();
                for branch in self.program.arena.if_branches(branches).to_vec() {
                    self.push_scope(); self.apply_compact_guard_narrowings(failure.clone());
                    self.check_compact_expr(branch.condition);
                    let facts = self.compact_condition_proof(branch.condition);
                    let failure_scopes = self.scopes.clone();
                    self.apply_compact_guard_narrowings(facts.when_true);
                    self.bind_compact_pattern_condition(branch.condition);
                    self.check_compact_block_expected_in_current_scope(branch.block, expected);
                    if !self.compact_block_definitely_exits(branch.block) {
                        reaching.push(self.scopes.iter().flat_map(|scope| scope.iter()).map(|(name, binding)| (*name, binding.clone())).collect::<FxHashMap<_, _>>());
                    }
                    failure.extend(facts.when_false); self.scopes = failure_scopes; self.pop_scope();
                }
                self.push_scope(); self.apply_compact_guard_narrowings(failure);
                if let Some(block) = else_block {
                    self.check_compact_block_expected_in_current_scope(block, expected);
                    if !self.compact_block_definitely_exits(block) {
                        reaching.push(self.scopes.iter().flat_map(|scope| scope.iter()).map(|(name, binding)| (*name, binding.clone())).collect());
                    }
                } else { reaching.push(self.scopes.iter().flat_map(|scope| scope.iter()).map(|(name, binding)| (*name, binding.clone())).collect()); }
                self.pop_scope();
                self.scopes = initial_scopes;
                for (name, original) in &initial {
                    for bindings in &reaching {
                        if let Some(binding) = bindings.get(name).filter(|binding| binding.proof.same_binding(&original.proof)) {
                            for path in binding.proof.mutation_paths_since(&original.proof) {
                                for scope in &mut self.scopes { for binding in scope.values_mut().filter(|binding| binding.proof.same_binding(&original.proof) && binding.mutable) {
                                    binding.proof.mutate(&path);
                                    if let Some(original) = &binding.unrefined_ty { super::proof::restore_projection(&mut binding.ty, original, &path); }
                                    if path.is_empty() { binding.unrefined_ty = None; }
                                } }
                            }
                        }
                    }
                }
                if !reaching.is_empty() {
                    let mut facts = Vec::new();
                    for (name, original) in initial {
                        let Some(current) = self.lookup_binding(name) else { continue; };
                        if !current.proof.same_binding(&original.proof) { continue; }
                        let types = reaching.iter().map(|bindings| bindings.get(&name).filter(|binding| binding.proof.same_binding(&original.proof))
                            .map(|binding| binding.ty.clone()).unwrap_or_else(|| original.unrefined_ty.as_ref().unwrap_or(&original.ty).clone())).collect::<Vec<_>>();
                        let ty = super::proof::intersection_type(original.unrefined_ty.as_ref().unwrap_or(&original.ty), &types);
                        if ty != current.ty { facts.push(current.proof.fact(name, Vec::new(), ty)); }
                    }
                    self.apply_compact_guard_narrowings(facts);
                }
            }
            ArenaStmtKind::While { condition, block } => {
                self.output.supported_statements += 1;
                self.check_compact_expr(condition);
                let facts = self.compact_guard_narrowings(condition, true);
                self.push_scope();
                self.apply_compact_guard_narrowings(facts);
                self.bind_compact_pattern_condition(condition);
                self.check_compact_block_in_current_scope(block);
                self.pop_scope();
            }
            ArenaStmtKind::For {
                target,
                iter,
                block,
            } => {
                self.output.supported_statements += 1;
                let iter_ty = self.check_compact_expr(iter);
                self.push_scope();
                self.define_binding_target(target, collection_item_type(&iter_ty), false);
                self.check_compact_block_in_current_scope(block);
                self.pop_scope();
            }
            ArenaStmtKind::Loop { block } => {
                self.output.supported_statements += 1;
                self.check_compact_block(block);
            }
            ArenaStmtKind::Break { value } => {
                self.output.supported_statements += 1;
                if let Some(value) = value {
                    self.check_compact_expr(value);
                }
            }
            ArenaStmtKind::Match { value, arms } => {
                self.output.supported_statements += 1;
                let subject = self.check_compact_expr(value);
                for arm in self.program.arena.match_arms(arms) {
                    self.push_scope();
                    self.bind_compact_pattern(arm.pattern, &subject);
                    if let Some(guard) = arm.guard { self.check_compact_expr(guard); }
                    self.check_compact_block_expected_in_current_scope(arm.block, expected);
                    self.pop_scope();
                }
            }
            ArenaStmtKind::Command(command) => {
                self.output.supported_statements += 1;
                self.output.commands += 1;
                self.check_compact_command(command);
            }
            ArenaStmtKind::Expr(expr) => {
                self.output.supported_statements += 1;
                let ty = self.check_compact_expr(expr);
                self.record_inert_discard(ArenaExprOrRun::Expr(expr));
                if ty == Type::Bool { let facts = self.compact_guard_narrowings(expr, true); self.apply_compact_guard_narrowings(facts); }
                if matches!(self.program.arena.expr(expr).kind, ArenaExprKind::ValueBlock(_)) {
                    self.apply_compact_expected(expr, &Type::Unit);
                }
            }
            ArenaStmtKind::SignalHook(hook) => {
                self.output.supported_statements += 1;
                let hook = self.program.arena.signal_hook(hook);
                let saved_scopes = self.scopes.clone();
                self.push_compact_deferred_capture_scope();
                self.check_compact_block(hook.body);
                self.scopes = saved_scopes;
            }
            ArenaStmtKind::With { bindings, body, else_block } => {
                self.output.supported_statements += 1;
                self.push_scope();
                let mut error_ty = None;
                for binding in self.program.arena.with_bindings(bindings) {
                    let previous_errors = self.with_initializer_errors.replace(Vec::new());
                    let actual = self.check_compact_expr(binding.initializer);
                    let mut errors = self.with_initializer_errors.take().unwrap_or_default();
                    self.with_initializer_errors = previous_errors;
                    if let Type::Result(_, error) = &actual { errors.push((**error).clone()); }
                    for error in errors {
                        error_ty = Some(match error_ty { None => error, Some(previous) if previous == error => previous, Some(_) => Type::Error });
                    }
                    let ty = match actual { Type::Result(ok, _) => *ok, other => other };
                    if binding.name.as_str() != "_" { self.current_scope_mut().insert(binding.name, CompactBinding::new(ty, false)); }
                }
                self.check_compact_block_in_current_scope(body);
                self.pop_scope();
                self.check_compact_error_handler(else_block, error_ty.unwrap_or(Type::Error));
            }
            ArenaStmtKind::Guard { target, ty, initializer, else_block } => {
                self.output.supported_statements += 1;
                self.output.bindings += 1;
                let expected = ty.map(|ty| self.type_from_arena(ty));
                let actual = match initializer { ArenaExprOrRun::Expr(expr) => self.check_compact_expr_expected(expr, expected.as_ref()), _ => self.check_compact_expr_or_run(initializer) };
                let (ok, error) = match actual { Type::Result(ok, error) => (*ok, *error), other => (other, Type::Error) };
                self.check_compact_error_handler(else_block, error);
                self.define_binding_target(target, expected.unwrap_or(ok), false);
            }
            ArenaStmtKind::Assert { condition, message } => {
                self.output.supported_statements += 1;
                self.check_compact_expr(condition);
                if let Some(message) = message { self.check_compact_expr(message); }
                let facts = self.compact_guard_narrowings(condition, true);
                self.apply_compact_guard_narrowings(facts);
            }
            ArenaStmtKind::GuardedStmt {
                stmt, condition, negate,
            } => {
                self.output.supported_statements += 1;
                self.check_compact_expr(condition);
                let facts = self.compact_condition_proof(condition);
                // Source checking validates the control target before compact
                // facts can publish a proof for the skipped branch.
                let exits = self.declarations.diagnostics.is_empty() && matches!(self.program.arena.stmt(stmt).kind,
                    ArenaStmtKind::Return(_) | ArenaStmtKind::Break { .. } | ArenaStmtKind::Continue);
                let continuing_scopes = exits.then(|| self.scopes.clone());
                self.push_scope();
                self.apply_compact_guard_narrowings(if negate { facts.when_false.clone() } else { facts.when_true.clone() });
                self.check_compact_stmt(stmt);
                self.pop_scope();
                if let Some(scopes) = continuing_scopes {
                    self.scopes = scopes;
                    self.apply_compact_guard_narrowings(if negate { facts.when_true } else { facts.when_false });
                }
            }
        }
    }

    fn check_compact_function(&mut self, id: FunctionDefId) {
        self.output.functions += 1;
        let def = self.program.arena.function_def(id);
        let saved_scopes = self.scopes.clone();
        self.push_compact_deferred_capture_scope();
        for param in self.program.arena.params(def.params) {
            if let Some(default) = param.default { self.check_compact_expr(default); }
        }
        for param in self.program.arena.params(def.params) {
            let ty = self.declarations.parameter_types.get(&self.program.arena.span(param.span)).cloned()
                .unwrap_or_else(|| self.type_from_arena(param.ty));
            self.current_scope_mut().insert(param.name, CompactBinding::new(ty, false));
        }
        let body_span = self.program.arena.span(self.program.arena.block(def.body).span);
        let expected = self.declarations.function_return_types.get(&body_span).cloned()
            .unwrap_or_else(|| self.type_from_arena(def.return_ty));
        let previous_schema = self.expected_schema.take();
        self.expected_schema = (!def.return_ty_defaulted).then(|| self.declarations.record_constructors.annotation_expectation(&self.program.arena, def.return_ty, self.current_namespace).ok()).flatten();
        self.return_types.push(expected.clone());
        self.return_schemas.push(self.expected_schema.clone());
        self.check_compact_block_expected_in_current_scope(def.body, Some(&expected));
        self.return_types.pop();
        self.return_schemas.pop();
        self.expected_schema = previous_schema;
        self.apply_compact_capture_block_expected(def.body, &expected);
        if !matches!(expected, Type::Stream(_)) { self.apply_compact_block_expected(def.body, &expected); }
        self.mark_tail_position(def.body, expected != Type::Unit && !expected.is_result_unit() && !matches!(expected, Type::Stream(_)));
        self.pop_scope();
        self.scopes = saved_scopes;
    }

    fn apply_compact_capture_block_expected(&mut self, block: BlockId, expected: &Type) {
        let Some(tail) = self.program.arena.stmt_ids(self.program.arena.block(block).statements).last() else { return; };
        match self.program.arena.stmt(tail).kind {
            ArenaStmtKind::Expr(expr) => self.apply_compact_capture_expected(expr, expected),
            ArenaStmtKind::If { branches, else_block } => {
                for branch in self.program.arena.if_branches(branches).to_vec() { self.apply_compact_capture_block_expected(branch.block, expected); }
                if let Some(block) = else_block { self.apply_compact_capture_block_expected(block, expected); }
            }
            ArenaStmtKind::Match { arms, .. } => {
                for arm in self.program.arena.match_arms(arms).to_vec() { self.apply_compact_capture_block_expected(arm.block, expected); }
            }
            _ => {}
        }
    }

    fn apply_compact_capture_expected(&mut self, expr: ExprId, expected: &Type) {
        match self.program.arena.expr(expr).kind {
            ArenaExprKind::Capture(block) => {
                if let Type::Result(ok, error) = expected {
                    if ok.as_ref() == &Type::Unit {
                        self.mark_tail_position(block, false);
                        if let Some(tail) = self.program.arena.stmt_ids(self.program.arena.block(block).statements).last() {
                            self.output.statement_positions.insert(tail, super::StatementPosition::Statement);
                        }
                    }
                    self.output.expr_types.insert(expr, Type::Result(ok.clone(), error.clone()));
                }
            }
            ArenaExprKind::ValueBlock(block) => self.apply_compact_capture_block_expected(block, expected),
            ArenaExprKind::If { branches, else_value } => {
                for branch in self.program.arena.if_expr_branches(branches).to_vec() { self.apply_compact_capture_expected(branch.value, expected); }
                self.apply_compact_capture_expected(else_value, expected);
            }
            _ => {}
        }
    }

    fn check_compact_block(&mut self, id: BlockId) {
        self.push_scope();
        self.check_compact_block_in_current_scope(id);
        self.pop_scope();
    }

    fn check_compact_block_in_current_scope(&mut self, id: BlockId) {
        self.check_compact_block_expected_in_current_scope(id, None);
    }

    fn check_compact_block_expected_in_current_scope(&mut self, id: BlockId, expected: Option<&Type>) {
        self.output.blocks += 1;
        let ArenaBlock {
            params, statements, ..
        } = self.program.arena.block(id);
        for param in self.program.arena.block_params(*params) {
            self.current_scope_mut()
                .entry(param.name).or_insert_with(|| CompactBinding::new(Type::Any, false));
        }
        let ids = self.program.arena.stmt_ids(*statements).collect::<Vec<_>>();
        for (index, stmt) in ids.iter().copied().enumerate() {
            if index + 1 == ids.len() && let Some(expected) = expected
                && let ArenaStmtKind::Expr(expr) = self.program.arena.stmt(stmt).kind {
                let explicit_result = super::stmt::tail_expr_uses_result_context_arena(self.program, expr);
                let previous = self.expected_schema.clone();
                let context = if explicit_result { expected } else { expected.result_ok().unwrap_or(expected) };
                if !explicit_result && expected.is_result() { self.expected_schema = previous.as_ref().and_then(|schema| schema.children.get(&crate::sema::constants::SchemaComponent::Success)).cloned(); }
                self.output.statements += 1;
                self.output.supported_statements += 1;
                self.check_compact_expr_expected(expr, Some(context));
                if expected == &Type::Unit || expected.is_result_unit() {
                    self.record_inert_discard(ArenaExprOrRun::Expr(expr));
                }
                self.expected_schema = previous;
            } else { self.check_compact_stmt_expected(stmt, (index + 1 == ids.len()).then_some(expected).flatten()); }
        }
        let ty = self.compact_block_tail_type(id);
        self.output.block_types.insert(id, ty);
    }

    fn mark_context_scope_value(&mut self, expr: ExprId) {
        match self.program.arena.expr(expr).kind {
            ArenaExprKind::ContextScope { block, .. } => {
                self.mark_tail_position(block, true);
                self.output.block_types.remove(&block);
                let ty = self.compact_block_tail_type(block);
                self.output.block_types.insert(block, ty.clone());
                self.output.expr_types.insert(expr, Type::Result(Box::new(ty), Box::new(Type::Error)));
            }
            ArenaExprKind::Try(inner) => {
                self.mark_context_scope_value(inner);
                if let Some(Type::Result(ok, _)) = self.output.expr_types.get(&inner).cloned() { self.output.expr_types.insert(expr, *ok); }
            }
            _ => {}
        }
    }

    fn mark_tail_position(&mut self, block: BlockId, consumes_value: bool) {
        let ids = self.program.arena.stmt_ids(self.program.arena.block(block).statements).collect::<Vec<_>>();
        if let Some(&tail) = ids.last() {
            if consumes_value && let ArenaStmtKind::Expr(expr) = self.program.arena.stmt(tail).kind { self.nonmaterial_expressions.remove(&expr); self.mark_context_scope_value(expr); }
            let returns_result = match self.program.arena.stmt(tail).kind {
                ArenaStmtKind::Expr(expr) => self.output.expr_types.get(&expr).is_some_and(Type::is_result),
                _ => false,
            };
            let position = if consumes_value || returns_result { super::StatementPosition::Value } else { super::StatementPosition::Statement };
            self.output.statement_positions.insert(tail, position);
            match self.program.arena.stmt(tail).kind {
                ArenaStmtKind::If { branches, else_block } => {
                    for branch in self.program.arena.if_branches(branches).to_vec() { self.mark_tail_position(branch.block, consumes_value); }
                    if let Some(block) = else_block { self.mark_tail_position(block, consumes_value); }
                }
                ArenaStmtKind::Match { arms, .. } => {
                    for arm in self.program.arena.match_arms(arms).to_vec() { self.mark_tail_position(arm.block, consumes_value); }
                }
                ArenaStmtKind::Expr(expr) if matches!(self.program.arena.expr(expr).kind, ArenaExprKind::ValueBlock(_)) => {
                    let expected = if consumes_value || returns_result { self.output.value_block_types.get(&expr).cloned().unwrap_or(Type::Unknown) } else { Type::Unit };
                    self.apply_compact_expected(expr, &expected);
                }
                _ => {}
            }
        }
    }

    fn compact_block_tail_type(&self, block: BlockId) -> Type {
        if let Some(ty) = self.output.block_types.get(&block) { return ty.clone(); }
        let tail = self.program.arena.stmt_ids(self.program.arena.block(block).statements).last();
        match tail.map(|id| self.program.arena.stmt(id).kind) {
            Some(ArenaStmtKind::Expr(expr)) => self.output.expr_types.get(&expr).cloned().unwrap_or(Type::Unknown),
            Some(ArenaStmtKind::TailBareIdent(name)) => self.declarations.prepared_constants.tail_bindings
                .get(&self.program.arena.stmt(tail.unwrap()).span)
                .and_then(|expr| self.declarations.prepared_constants.types.get(expr)).cloned()
                .unwrap_or_else(|| self.lookup_name(name)),
            Some(ArenaStmtKind::Command(command)) if self.output.statement_positions.get(&tail.unwrap()) == Some(&super::StatementPosition::Value) => {
                let command = self.program.arena.command_stmt(command);
                let ArenaCommand::Run(run) = command.command else { return Type::Unit; };
                let Some(result) = super::command::run_capture_result_type_arena(self.program, run) else { return Type::Unit; };
                if self.program.arena.run_form(run).propagate { result.result_ok().unwrap().clone() } else { result }
            }
            Some(ArenaStmtKind::If { branches, else_block: Some(block) }) => {
                let mut ty = Some(self.compact_block_tail_type(block));
                for branch in self.program.arena.if_branches(branches) { ty = Some(merge_types(ty, self.compact_block_tail_type(branch.block))); }
                ty.unwrap_or(Type::Unknown)
            }
            Some(ArenaStmtKind::Match { arms, .. }) => {
                let mut ty = None;
                for arm in self.program.arena.match_arms(arms) { ty = Some(merge_types(ty, self.compact_block_tail_type(arm.block))); }
                ty.unwrap_or(Type::Unknown)
            }
            Some(ArenaStmtKind::Return(_)) | Some(ArenaStmtKind::Break { .. }) | Some(ArenaStmtKind::Continue) => Type::Unknown,
            _ => Type::Unit,
        }
    }

    fn check_compact_expr_or_run(&mut self, value: ArenaExprOrRun) -> Type {
        match value {
            ArenaExprOrRun::Expr(expr) => self.check_compact_expr(expr),
            ArenaExprOrRun::Run(run) => {
                self.output.runs += 1;
                self.check_compact_run(run);
                Type::Result(Box::new(Type::Status), Box::new(Type::ProcessError))
            }
        }
    }

    fn check_comp_qualifiers(&mut self, qualifiers: crate::syntax::arena::ArenaRange) -> usize {
        let mut scopes = 0;
        for qualifier in self.program.arena.comp_qualifiers(qualifiers).to_vec() {
            match qualifier {
                crate::syntax::arena::ArenaCompQualifier::For { target, iter, .. } => {
                    let iter_ty = self.check_compact_expr(iter);
                    self.push_scope();
                    scopes += 1;
                    self.define_binding_target(target, collection_item_type(&iter_ty), false);
                }
                crate::syntax::arena::ArenaCompQualifier::If { condition, .. } => { self.check_compact_expr(condition); }
            }
        }
        scopes
    }

    fn check_compact_expr_expected(&mut self, id: ExprId, expected: Option<&Type>) -> Type {
        let previous = self.expected_schema.clone();
        if expected.is_none() { self.expected_schema = None; }
        let actual = self.check_compact_expr_inner(id, expected);
        if let Some(expected) = expected { self.apply_compact_expected(id, expected); }
        self.expected_schema = previous;
        self.output.expr_types.get(&id).cloned().unwrap_or(actual)
    }

    fn check_compact_child_expected(&mut self, id: ExprId, expected: Option<&Type>, component: crate::sema::constants::SchemaComponent) -> Type {
        let previous = self.expected_schema.clone();
        self.expected_schema = previous.as_ref().and_then(|schema| schema.value_context().children.get(&component)).cloned();
        let actual = self.check_compact_expr_expected(id, expected);
        self.expected_schema = previous;
        actual
    }

    fn check_compact_expr(&mut self, id: ExprId) -> Type {
        self.check_compact_expr_expected(id, None)
    }

    fn check_compact_expr_inner(&mut self, id: ExprId, expected: Option<&Type>) -> Type {
        self.condition_proofs.remove(&id);
        self.output.expressions += 1;
        if let Some(ty) = self.declarations.prepared_constants.types.get(&id) {
            self.output.expr_types.insert(id, ty.clone());
            return ty.clone();
        }
        let ty = match self.program.arena.expr(id).kind {
            ArenaExprKind::Null => Type::Null,
            ArenaExprKind::Bool(_) => Type::Bool,
            ArenaExprKind::Int(_) => Type::Int,
            ArenaExprKind::Float(_) => Type::Float,
            ArenaExprKind::Duration(_) => Type::Duration,
            ArenaExprKind::Str(_) => Type::Str,
            ArenaExprKind::Regex(_) => Type::Regex,
            ArenaExprKind::FmtString(parts) => {
                self.check_compact_fmt_parts(parts);
                Type::Str
            }
            ArenaExprKind::PathStr(_) => Type::Path,
            ArenaExprKind::PathFmtString(parts) => {
                self.check_compact_fmt_parts(parts);
                Type::Path
            }
            ArenaExprKind::GlobStr(_) => Type::List(Box::new(Type::Path)),
            ArenaExprKind::Bytes(_) => Type::Bytes,
            ArenaExprKind::Ident(name) => {
                if name == "_" {
                    self.pipeline_hole_types.get(&id).cloned().unwrap_or_else(|| {
                        self.error(self.program.arena.expr(id).span, "`_` is only a whole argument placeholder in an immediate value pipeline call", "check.pipeline-hole");
                        Type::Invalid
                    })
                } else { self.lookup_name(name) }
            },
            ArenaExprKind::LastStatus => Type::Status,
            ArenaExprKind::List(items) => self.check_compact_list(items, expected),
            ArenaExprKind::Record(fields) => self.check_compact_record(fields, expected),
            ArenaExprKind::Unary { op, expr } => self.check_compact_unary(op, expr),
            ArenaExprKind::ComparisonChain(pairs) => {
                for pair in self.program.arena.comparison_chain_operands(pairs).collect::<Vec<_>>() {
                    self.check_compact_expr(pair);
                }
                Type::Bool
            }
            ArenaExprKind::Binary { op, left, right } => self.check_compact_binary(op, left, right),
            ArenaExprKind::ValuePipelineCall { input, call, hole } => {
                let input_ty = self.check_compact_expr(input);
                let previous = self.pipeline_hole_types.insert(hole, input_ty);
                let ty = self.check_compact_expr(call);
                match previous { Some(previous) => { self.pipeline_hole_types.insert(hole, previous); }, None => { self.pipeline_hole_types.remove(&hole); } }
                ty
            }
            ArenaExprKind::Call { callee, args } => self.check_compact_call(id, callee, args, expected),
            ArenaExprKind::Field { base, name } => self.check_compact_field(base, name),
            ArenaExprKind::NullSafeField { base, name } => {
                let receiver = self.check_compact_expr(base);
                let (receiver, lift) = compact_postfix_receiver(receiver, true);
                let field = compact_field_type(receiver, name);
                compact_postfix_result(field, lift)
            }
            ArenaExprKind::Index { base, index, guarded } => {
                let base_expr = base;
                let base = self.check_compact_expr(base);
                let (base, lift) = compact_postfix_receiver(base, guarded);
                self.check_compact_expr(index);
                let ty = if let Some(projection) = crate::sema::projection::resolve_constant_key_projection(
                    &self.program.arena, &self.declarations.prepared_constants, base_expr, &base, index,
                    crate::sema::projection::ProjectionOperation::Index,
                ) {
                    let ty = projection.value_type.clone();
                    self.output.projections.insert(id, projection);
                    ty
                } else { index_type(&base) };
                compact_postfix_result(ty, lift)
            }
            ArenaExprKind::Slice { base, start, end, guarded } => {
                let ty = self.check_compact_expr(base);
                let (ty, lift) = compact_postfix_receiver(ty, guarded);
                if let Some(start) = start {
                    self.check_compact_expr(start);
                }
                if let Some(end) = end {
                    self.check_compact_expr(end);
                }
                let result = match ty {
                    Type::List(_) | Type::Str | Type::Path | Type::Bytes => ty,
                    _ => Type::Unknown,
                };
                compact_postfix_result(result, lift)
            }
            ArenaExprKind::EnvGet { kind, .. } => Type::Result(Box::new(match kind {
                EnvGetKind::Str => Type::Str,
                EnvGetKind::Path => Type::Path,
                EnvGetKind::PathList => Type::EnvPathList,
            }), Box::new(Type::Error)),
            ArenaExprKind::EnvPathList => Type::EnvPathList,
            ArenaExprKind::Try(expr) => {
                let previous = self.expected_schema.clone();
                let inner_expected = expected.map(|ty| Type::Result(Box::new(ty.clone()), Box::new(Type::Error)));
                self.expected_schema = previous.clone().map(|schema| crate::sema::constants::SchemaExpectation {
                    instances: Vec::new(), children: BTreeMap::from([(crate::sema::constants::SchemaComponent::Success, schema)]),
                });
                let ty = self.check_compact_expr_expected(expr, inner_expected.as_ref());
                self.expected_schema = previous;
                if let Some(errors) = &mut self.with_initializer_errors && let Type::Result(_, error) = &ty {
                    errors.push((**error).clone());
                }
                ty.result_ok().cloned().unwrap_or(Type::Unknown)
            }
            ArenaExprKind::Require { value, schema } => {
                let target = if let Some(schema) = schema {
                    let context = self.declarations.record_constructors.annotation_expectation(&self.program.arena, schema, self.current_namespace).unwrap_or_default();
                    Some(super::expected::requirement_target(&self.program.arena, self.type_from_arena(schema), context))
                } else {
                    super::expected::infer_requirement_target(&self.program.arena, expected, self.expected_schema.as_ref(), &self.type_constraints)
                        .or_else(|| self.declarations.requirement_targets.get(&id).cloned())
                };
                self.check_compact_expr(value);
                if let Some(target) = target {
                    let ty = Type::Result(Box::new(target.ty.clone()), Box::new(Type::Error));
                    self.output.requirement_targets.insert(id, target);
                    ty
                } else {
                    self.error(self.program.arena.expr(id).span, "`.require()` needs an independently known concrete target type", "check.require-target");
                    Type::Invalid
                }
            }
            ArenaExprKind::Run(run) => {
                self.output.runs += 1;
                self.check_compact_run(run);
                Type::Result(Box::new(Type::Status), Box::new(Type::ProcessError))
            }
            ArenaExprKind::Spawn(form) => {
                match form.target {
                    crate::syntax::arena::ArenaSpawnTarget::Run(run) => {
                        self.output.runs += 1;
                        self.check_compact_run(run);
                    }
                    crate::syntax::arena::ArenaSpawnTarget::Command(command) => {
                        self.check_compact_expr(command);
                    }
                }
                Type::ProcessHandle
            }
            ArenaExprKind::Wait(form) => {
                self.check_compact_expr(form.target);
                Type::Result(Box::new(Type::Status), Box::new(Type::ProcessError))
            }
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                let mut ty = None;
                for branch in self.program.arena.if_expr_branches(branches) {
                    self.check_compact_expr(branch.condition);
                    let facts = self.compact_guard_narrowings(branch.condition, true);
                    self.push_scope();
                    self.apply_compact_guard_narrowings(facts);
                    self.bind_compact_pattern_condition(branch.condition);
                    ty = Some(merge_types(ty, self.check_compact_expr_expected(branch.value, expected)));
                    self.pop_scope();
                }
                merge_types(ty, self.check_compact_expr_expected(else_value, expected))
            }
            ArenaExprKind::ErrorContext { message, block } => {
                self.check_compact_expr(message);
                self.push_scope();
                self.check_compact_block_expected_in_current_scope(block, expected);
                self.mark_tail_position(block, true);
                self.output.block_types.remove(&block);
                let ty = self.compact_block_tail_type(block);
                self.pop_scope();
                ty
            }
            ArenaExprKind::ContextScope { input, block, value_body, .. } => {
                self.check_compact_expr(input);
                self.push_scope();
                self.check_compact_block_in_current_scope(block);
                self.mark_tail_position(block, value_body);
                if !value_body && let Some(tail) = self.program.arena.stmt_ids(self.program.arena.block(block).statements).last() {
                    self.output.statement_positions.insert(tail, super::StatementPosition::Statement);
                }
                self.output.block_types.remove(&block);
                let ty = if value_body { self.compact_block_tail_type(block) } else { Type::Unit };
                self.output.block_types.insert(block, ty.clone());
                self.pop_scope();
                Type::Result(Box::new(ty), Box::new(Type::Error))
            }
            ArenaExprKind::ValueBlock(block) => {
                self.push_scope();
                self.check_compact_block_expected_in_current_scope(block, expected);
                self.mark_tail_position(block, true);
                self.output.block_types.remove(&block);
                let ty = self.compact_block_tail_type(block);
                self.output.value_block_types.insert(id, ty.clone());
                self.output.block_types.insert(block, ty.clone());
                self.pop_scope();
                ty
            }
            ArenaExprKind::Loop { block } => {
                self.check_compact_block(block);
                Type::Unknown
            }
            ArenaExprKind::Capture(block) => {
                self.push_scope();
                self.check_compact_block_in_current_scope(block);
                let mut ty = self.compact_block_tail_type(block);
                if let Some(Type::Result(ok, _)) = expected {
                    if ok.as_ref() == &Type::Unit {
                        ty = Type::Unit;
                        self.mark_tail_position(block, false);
                        if let Some(tail) = self.program.arena.stmt_ids(self.program.arena.block(block).statements).last() {
                            self.output.statement_positions.insert(tail, super::StatementPosition::Statement);
                        }
                    } else { self.mark_tail_position(block, true); }
                } else { self.mark_tail_position(block, true); }
                self.pop_scope();
                Type::Result(Box::new(ty), Box::new(Type::Error))
            }
            ArenaExprKind::Retry { delays, block, .. } => {
                for delay in self.program.arena.expr_ids(delays) {
                    self.check_compact_expr(delay);
                }
                self.push_scope();
                self.check_compact_block_in_current_scope(block);
                self.mark_tail_position(block, true);
                self.output.block_types.remove(&block);
                let ty = self.compact_block_tail_type(block);
                self.pop_scope();
                Type::Result(Box::new(ty), Box::new(Type::Error))
            }
            ArenaExprKind::ListComp { expr, qualifiers } => {
                let scopes = self.check_comp_qualifiers(qualifiers);
                let item = self.check_compact_expr(expr);
                for _ in 0..scopes { self.pop_scope(); }
                Type::List(Box::new(item))
            }
            ArenaExprKind::MapComp { key, value, qualifiers } => {
                let scopes = self.check_comp_qualifiers(qualifiers);
                let key_ty = self.check_compact_expr(key);
                let item = self.check_compact_expr(value);
                for _ in 0..scopes { self.pop_scope(); }
                Type::Map(Box::new(key_ty), Box::new(item))
            }
            ArenaExprKind::Match { value, arms } | ArenaExprKind::PatternTest { value, arms } | ArenaExprKind::PatternCondition { value, arms } => {
                let subject = self.check_compact_expr(value);
                let mut ty = None;
                for arm in self.program.arena.match_expr_arms(arms) {
                    self.push_scope();
                    if !matches!(self.program.arena.expr(id).kind, ArenaExprKind::PatternTest { .. }) { self.bind_compact_pattern(arm.pattern, &subject); }
                    if let Some(guard) = arm.guard { self.check_compact_expr(guard); }
                    let context = if matches!(self.program.arena.expr(id).kind, ArenaExprKind::Match { .. }) { expected } else { None };
                    ty = Some(merge_types(ty, self.check_compact_expr_expected(arm.value, context)));
                    self.pop_scope();
                }
                ty.unwrap_or(Type::Unknown)
            }
            ArenaExprKind::Pipeline { input, stages } => {
                self.check_compact_expr(input);
                self.check_compact_pipe_stages(stages);
                Type::Unknown
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                let input = self.check_compact_expr(input);
                self.check_compact_stream_stages(stages, input)
            }
            ArenaExprKind::BuilderCall { call, block } => {
                self.check_compact_expr(call);
                self.check_compact_builder_block(block);
                Type::Unknown
            }
            ArenaExprKind::Item => self.stream_items.last().cloned().unwrap_or(Type::Any),
        };
        let ty = if let Some(alias) = self.declarations.static_callable_aliases.get(&self.program.arena.expr(id).span) {
            if alias.pure { Type::Pure } else { Type::Proc }
        } else { ty };
        self.output.typed_expressions += 1;
        self.output.expr_types.insert(id, ty.clone());
        if let Some(expected) = expected { self.apply_compact_capture_expected(id, expected); }
        self.output.expr_types.get(&id).cloned().unwrap_or(ty)
    }

    fn check_compact_list(&mut self, range: crate::syntax::arena::ArenaListElementRange, expected: Option<&Type>) -> Type {
        let mut item_ty = None;
        for item in self.program.arena.list_elements(range) {
            let context = match expected { Some(Type::List(item)) => Some(item.as_ref()), _ => None };
            let ty = if item.splice_span.is_some() { self.check_compact_expr_expected(item.value, expected) }
                else { self.check_compact_child_expected(item.value, context, crate::sema::constants::SchemaComponent::Item) };
            let ty = if item.splice_span.is_some() {
                match ty { Type::List(inner) => *inner, _ => Type::Unknown }
            } else { ty };
            item_ty = Some(merge_types(item_ty, ty));
        }
        Type::List(Box::new(item_ty.unwrap_or(Type::Unknown)))
    }

    fn apply_compact_block_expected(&mut self, block: BlockId, expected: &Type) {
        let tail = self.program.arena.stmt_ids(self.program.arena.block(block).statements).last();
        if let Some(tail) = tail {
            match self.program.arena.stmt(tail).kind {
                ArenaStmtKind::Expr(expr) => self.apply_compact_expected(expr, expected),
                ArenaStmtKind::If { branches, else_block } => {
                    for branch in self.program.arena.if_branches(branches).to_vec() { self.apply_compact_block_expected(branch.block, expected); }
                    if let Some(block) = else_block { self.apply_compact_block_expected(block, expected); }
                }
                ArenaStmtKind::Match { arms, .. } => { for arm in self.program.arena.match_arms(arms).to_vec() { self.apply_compact_block_expected(arm.block, expected); } }
                ArenaStmtKind::Command(_) if expected == &Type::Unit || expected.is_result_unit() => {
                    self.output.statement_positions.insert(tail, super::StatementPosition::Statement);
                }
                _ => {}
            }
        }
    }

    // Expected container types determine the meaning of brace literals, rather
    // than converting a Record after its expressions have already been lowered.
    fn apply_compact_expected(&mut self, expr: ExprId, expected: &Type) {
        if let ArenaExprKind::ContextScope { block, value_body, .. } = self.program.arena.expr(expr).kind {
            if let Type::Result(ok, _) = expected && (value_body || **ok != Type::Unit) {
                self.mark_tail_position(block, true);
                self.apply_compact_block_expected(block, ok);
                self.output.block_types.insert(block, ok.as_ref().clone());
                self.output.expr_types.insert(expr, expected.clone());
            }
            return;
        }
        let preserve_result = expected.is_result() && matches!(self.program.arena.expr(expr).kind,
            ArenaExprKind::ValueBlock(_) | ArenaExprKind::ErrorContext { .. } | ArenaExprKind::Match { .. } | ArenaExprKind::If { .. })
            && super::stmt::tail_expr_uses_result_context_arena(self.program, expr);
        let expected = if preserve_result { expected } else { expected.result_ok().unwrap_or(expected) };
        if !matches!(expected, Type::Unit | Type::Unknown | Type::Invalid)
            && let Some(actual) = self.output.expr_types.get(&expr).cloned()
            && actual.contains_inference() {
            let actual = actual.result_ok().unwrap_or(&actual);
            let constrained = if expected.contains_inference() {
                self.type_constraints.constrain(expected, actual, self.program.arena.expr(expr).span)
            } else {
                self.type_constraints.constrain_context(expected, actual, self.program.arena.expr(expr).span)
            };
            if constrained.is_err() { self.output.expr_types.insert(expr, Type::Invalid); }
        }
        match self.program.arena.expr(expr).kind {
            ArenaExprKind::MapComp { key, value, .. } => if let Type::Map(expected_key, item) = expected {
                self.output.expr_types.insert(expr, expected.clone());
                self.apply_compact_expected(key, expected_key);
                self.apply_compact_expected(value, item);
            },
            ArenaExprKind::Record(fields) => {
                if self.program.arena.record_fields(fields).iter().any(|field| matches!(field.kind, ArenaRecordFieldKind::Path { .. })) {
                    return;
                }
                match expected {
                    Type::Map(_, item) => {
                        self.output.expr_types.insert(expr, expected.clone());
                        for field in self.program.arena.record_fields(fields).to_vec() {
                            match field.kind {
                                ArenaRecordFieldKind::Computed { value, .. } | ArenaRecordFieldKind::Named { value, .. } => self.apply_compact_expected(value, item),
                                ArenaRecordFieldKind::Spread { expr, .. } => self.apply_compact_expected(expr, expected),
                                ArenaRecordFieldKind::Shorthand { .. } | ArenaRecordFieldKind::Path { .. } => {}
                            }
                        }
                    }
                    Type::Record(schema) => {
                        for field in self.program.arena.record_fields(fields).to_vec() {
                            if let ArenaRecordFieldKind::Named { name, value, .. } = field.kind && let Some(ty) = schema.get(&name) { self.apply_compact_expected(value, ty); }
                        }
                    }
                    _ => {}
                }
            }
            ArenaExprKind::List(items) => if let Type::List(item) = expected {
                self.output.expr_types.insert(expr, expected.clone());
                for entry in self.program.arena.list_elements(items).collect::<Vec<_>>() {
                    self.apply_compact_expected(entry.value, if entry.splice_span.is_some() { expected } else { item });
                }
            },
            ArenaExprKind::ValueBlock(block) => {
                let consumes_value = preserve_result || expected != &Type::Unit && !expected.is_result_unit();
                self.mark_tail_position(block, consumes_value);
                self.apply_compact_block_expected(block, expected);
                let tail = self.program.arena.stmt_ids(self.program.arena.block(block).statements).last();
                if !consumes_value {
                    if let Some(tail) = tail { self.output.statement_positions.insert(tail, super::StatementPosition::Statement); }
                    self.output.expr_types.insert(expr, Type::Unit);
                    self.output.block_types.insert(block, Type::Unit);
                } else {
                    self.output.block_types.remove(&block);
                    let inferred = self.output.value_block_types.get(&expr).cloned().unwrap_or(Type::Unknown);
                    let actual = match tail.map(|tail| self.program.arena.stmt(tail).kind) {
                        Some(ArenaStmtKind::Expr(value)) => self.output.expr_types.get(&value).cloned().unwrap_or(inferred),
                        _ => inferred,
                    };
                    self.output.expr_types.insert(expr, actual.clone());
                    self.output.block_types.insert(block, actual);
                }
            }
            ArenaExprKind::ErrorContext { block, .. } => {
                self.apply_compact_block_expected(block, expected);
                if *expected != Type::Unit { self.mark_tail_position(block, true); }
                self.output.expr_types.insert(expr, self.compact_block_tail_type(block));
            },
            ArenaExprKind::If { branches, else_value } => {
                for branch in self.program.arena.if_expr_branches(branches).to_vec() { self.apply_compact_expected(branch.value, expected); }
                self.apply_compact_expected(else_value, expected);
            }
            ArenaExprKind::Call { callee, args } if matches!(self.program.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Ok") => {
                if let Some(arg) = self.program.arena.call_args(args).first() && let crate::syntax::arena::ArenaCallArgKind::Positional(value) = arg.kind { self.apply_compact_expected(value, expected); }
            }
            _ => {}
        }
    }

    fn check_compact_record(&mut self, range: crate::syntax::arena::ArenaRange, expected: Option<&Type>) -> Type {
        let updating = self.program.arena.record_fields(range).iter().any(|field| matches!(field.kind, ArenaRecordFieldKind::Path { .. }));
        let (expected_key, expected_item) = match expected { Some(Type::Map(key, value)) => (Some(key.as_ref()), Some(value.as_ref())), _ => (None, None) };
        if !updating && (matches!(expected, Some(Type::Map(_, _))) || self.program.arena.record_fields(range).iter().any(|field| matches!(field.kind, ArenaRecordFieldKind::Computed { .. }))) {
            let mut item = None;
            let mut key_ty = None;
            for field in self.program.arena.record_fields(range).to_vec() {
                let (key, ty) = match field.kind {
                    ArenaRecordFieldKind::Computed { key, value, .. } => (self.check_compact_child_expected(key, expected_key, crate::sema::constants::SchemaComponent::Key), self.check_compact_child_expected(value, expected_item, crate::sema::constants::SchemaComponent::Value)),
                    ArenaRecordFieldKind::Path { value, .. } | ArenaRecordFieldKind::Named { value, .. } => (Type::Str, self.check_compact_child_expected(value, expected_item, crate::sema::constants::SchemaComponent::Value)),
                    ArenaRecordFieldKind::Shorthand { name, .. } => (Type::Str, self.lookup_name(name)),
                    ArenaRecordFieldKind::Spread { expr, .. } => match self.check_compact_expr_expected(expr, expected) { Type::Map(key, item) => (*key, *item), _ => (Type::Unknown, Type::Unknown) },
                };
                key_ty = Some(merge_types(key_ty, key));
                item = Some(merge_types(item, ty));
            }
            return Type::Map(Box::new(key_ty.unwrap_or(Type::Str)), Box::new(item.unwrap_or(Type::Unknown)));
        }

        let mut fields = BTreeMap::new();
        for field in self.program.arena.record_fields(range) {
            match &field.kind {
                ArenaRecordFieldKind::Computed { key, value, .. } => { self.check_compact_expr(*key); self.check_compact_expr(*value); }
                ArenaRecordFieldKind::Path { value, .. } => { self.check_compact_expr(*value); }
                ArenaRecordFieldKind::Named { name, value, .. } => {
                    let field_expected = match expected { Some(Type::Record(fields)) => fields.get(name), _ => None };
                    let value_ty = self.check_compact_child_expected(*value, field_expected, crate::sema::constants::SchemaComponent::Field(*name));
                    if !updating { fields.insert(*name, value_ty); }
                }
                ArenaRecordFieldKind::Shorthand { name, span } => {
                    if *name == "_" { self.error(self.program.arena.span(*span), "`_` is only a whole argument placeholder in an immediate value pipeline call", "check.pipeline-hole"); }
                    let value_ty = self.lookup_name(*name);
                    if !updating { fields.insert(*name, value_ty); }
                }
                ArenaRecordFieldKind::Spread { expr, .. } => {
                    if let Type::Record(spread) = self.check_compact_expr(*expr) {
                        fields.extend(spread);
                    }
                }
            }
        }
        Type::Record(fields)
    }

    fn check_compact_fmt_parts(&mut self, range: crate::syntax::arena::ArenaRange) {
        for part in self.program.arena.fmt_parts(range) {
            if let crate::syntax::arena::ArenaFmtPart::Expr(expr, _) = part {
                self.check_compact_expr(expr);
            }
        }
    }

    fn check_compact_unary(&mut self, op: UnaryOp, expr: ExprId) -> Type {
        let ty = self.check_compact_expr(expr);
        match op {
            UnaryOp::Not => Type::Bool,
            UnaryOp::Neg if matches!(ty, Type::Any) => Type::Any,
            UnaryOp::Neg if matches!(ty, Type::Float) => Type::Float,
            UnaryOp::Neg if matches!(ty, Type::Int | Type::Duration) => ty,
            UnaryOp::Neg => Type::Unknown,
        }
    }

    fn check_compact_binary(&mut self, op: BinaryOp, left: ExprId, right: ExprId) -> Type {
        let left_id = left;
        let right_id = right;
        let left = self.check_compact_expr(left);
        if op == BinaryOp::ResultFallback && !matches!(left, Type::Optional(_) | Type::Result(_, _))
            && self.compact_subject(left_id).is_some_and(|(name, path, _)| {
                self.lookup_binding(name).and_then(|binding| binding.unrefined_ty.as_ref())
                    .and_then(|ty| super::proof::projected_type(ty, &path)).is_some_and(|ty| matches!(ty, Type::Optional(_)))
            }) { self.output.proven_nonnull_fallback_receivers.insert(left_id); }
        let saved_scopes = self.output.proven_nonnull_fallback_receivers.contains(&left_id).then(|| self.scopes.clone());
        let short_circuit = matches!(op, BinaryOp::And | BinaryOp::Or);
        if short_circuit {
            let facts = self.compact_guard_narrowings(left_id, op == BinaryOp::And);
            self.push_scope(); self.apply_compact_guard_narrowings(facts);
        }
        let right = if op == BinaryOp::ResultFallback {
            if let ArenaExprKind::ValueBlock(block) = self.program.arena.expr(right).kind {
                self.push_scope();
                if let ([param], Type::Result(_, error)) = (self.program.arena.block_params(self.program.arena.block(block).params), &left) {
                    self.current_scope_mut().insert(param.name, CompactBinding::new(error.as_ref().clone(), false));
                }
                self.check_compact_block_in_current_scope(block);
                let ty = self.compact_block_tail_type(block);
                self.mark_tail_position(block, true);
                self.output.expr_types.insert(right, ty.clone());
                self.pop_scope();
                ty
            } else { self.check_compact_expr(right) }
        } else { self.check_compact_expr(right) };
        if short_circuit { self.pop_scope(); }
        if let Some(scopes) = saved_scopes { self.scopes = scopes; }
        match op {
            BinaryOp::Or
            | BinaryOp::And
            | BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::In
            | BinaryOp::NotIn => Type::Bool,
            BinaryOp::Add | BinaryOp::Sub if left == Type::Duration && right == Type::Duration => Type::Duration,
            BinaryOp::Mul if matches!((&left, &right), (Type::Duration, Type::Int) | (Type::Int, Type::Duration)) => Type::Duration,
            BinaryOp::Div if left == Type::Duration && right == Type::Int => Type::Duration,
            BinaryOp::Div if left == Type::Duration && right == Type::Duration => Type::Int,
            BinaryOp::Add if matches!((&left, &right), (Type::Str, Type::Str)) => Type::Str,
            BinaryOp::Add if matches!((&left, &right), (Type::List(_), Type::List(_))) => left,
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                if left == Type::Any || right == Type::Any { Type::Any } else { numeric_result_type(left, right) }
            }
            BinaryOp::ResultFallback => match left {
                Type::Result(inner, _) if *inner == Type::Unknown => right,
                Type::Optional(inner) | Type::Result(inner, _) => {
                    if inner.contains_inference() && self.type_constraints.constrain(&inner, &right, self.program.arena.expr(right_id).span).is_err() { return Type::Invalid; }
                    self.type_constraints.resolve(&inner).unwrap_or(Type::Invalid)
                }
                other => other,
            },
        }
    }

    fn check_compact_call(
        &mut self,
        id: ExprId,
        callee: ExprId,
        args: crate::syntax::arena::ArenaRange,
        expected: Option<&Type>,
    ) -> Type {
        let callee_expr = self.program.arena.expr(callee);
        let callee_ty = self.check_compact_expr(callee);
        let signature = match callee_expr.kind {
            ArenaExprKind::Ident(name) => self.declarations.pures.get(&name).or_else(|| self.declarations.procs.get(&name)).cloned(),
            ArenaExprKind::Field { base, name } => if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind {
                let qualified = QualifiedName::new(module, name);
                self.declarations.qualified_pures.get(&qualified).or_else(|| self.declarations.qualified_procs.get(&qualified)).cloned()
            } else { None },
            _ => None,
        };
        let constructor_profile = self.declarations.record_constructors.constructor_definition(&self.program.arena, callee, self.current_namespace).and_then(|definition| {
            let Type::Record(fields) = self.declarations.record_constructors.constructor_type(&self.program.arena, callee, self.current_namespace)? else { return None; };
            let schema = self.declarations.record_constructors.instance_expectation(&self.program.arena, definition, &[]).ok()?;
            Some((fields, schema))
        });
        let mut positional = 0;
        let mut occupied = signature.as_ref().map(|sig| vec![false; sig.params.len()]).unwrap_or_default();
        for arg in self.program.arena.call_args(args).to_vec() {
            let (value, index) = match arg.kind {
                crate::syntax::arena::ArenaCallArgKind::Positional(value) => {
                    while occupied.get(positional) == Some(&true) { positional += 1; }
                    let index = positional; positional += 1; (value, Some(index))
                }
                crate::syntax::arena::ArenaCallArgKind::Named { name, value, .. } => (value, signature.as_ref().and_then(|sig| sig.params.iter().position(|param| param.name == name))),
                crate::syntax::arena::ArenaCallArgKind::Splice { value, .. } | crate::syntax::arena::ArenaCallArgKind::NamedSpread { value, .. } => (value, None),
            };
            let previous = self.expected_schema.clone();
            let context = signature.as_ref().and_then(|sig| index.and_then(|index| sig.params.get(index)));
            self.expected_schema = signature.as_ref().and_then(|sig| index.and_then(|index| sig.parameter_schemas.get(index)).cloned().flatten());
            let ok = matches!(callee_expr.kind, ArenaExprKind::Ident(name) if name == "Ok");
            let err = matches!(callee_expr.kind, ArenaExprKind::Ident(name) if name == "Err");
            let constructor_field = match arg.kind { crate::syntax::arena::ArenaCallArgKind::Named { name, .. } => Some(name), _ => None };
            if let (Some(name), Some((_, schema))) = (constructor_field, &constructor_profile) {
                self.expected_schema = schema.children.get(&crate::sema::constants::SchemaComponent::Field(name)).cloned();
            }
            let expected_arg = context.map(|param| &param.ty).or_else(|| constructor_field.and_then(|name| constructor_profile.as_ref().and_then(|(fields, _)| fields.get(&name)))).or_else(|| if ok { expected.and_then(Type::result_ok) } else if err { expected.and_then(|ty| match ty { Type::Result(_, error) => Some(error.as_ref()), _ => None }) } else { None });
            if ok { self.expected_schema = previous.as_ref().and_then(|schema| schema.children.get(&crate::sema::constants::SchemaComponent::Success)).cloned(); }
            if let Some(index) = index && let Some(entry) = occupied.get_mut(index) { *entry = true; }
            if matches!(arg.kind, crate::syntax::arena::ArenaCallArgKind::NamedSpread { .. })
                && let Some(signature) = &signature && let ArenaExprKind::Record(fields) = self.program.arena.expr(value).kind {
                let mut record = BTreeMap::new();
                let mut schema = crate::sema::constants::SchemaExpectation::default();
                for field in self.program.arena.record_fields(fields) {
                    let name = match field.kind { ArenaRecordFieldKind::Named { name, .. } | ArenaRecordFieldKind::Shorthand { name, .. } => name, _ => continue };
                    if let Some(index) = signature.params.iter().position(|param| param.name == name && !param.rest) {
                        record.insert(name, signature.params[index].ty.clone());
                        if let Some(context) = &signature.parameter_schemas[index] { schema.children.insert(crate::sema::constants::SchemaComponent::Field(name), context.clone()); }
                    }
                }
                self.expected_schema = Some(schema);
                self.check_compact_expr_expected(value, Some(&Type::Record(record)));
            } else if matches!(arg.kind, crate::syntax::arena::ArenaCallArgKind::NamedSpread { .. })
                && let Some((fields, schema)) = &constructor_profile && let ArenaExprKind::Record(source_fields) = self.program.arena.expr(value).kind {
                let supplied = self.program.arena.record_fields(source_fields).iter().filter_map(|field| match field.kind {
                    ArenaRecordFieldKind::Named { name, .. } | ArenaRecordFieldKind::Shorthand { name, .. } => fields.get(&name).map(|ty| (name, ty.clone())), _ => None,
                }).collect();
                self.expected_schema = Some(schema.clone());
                self.check_compact_expr_expected(value, Some(&Type::Record(supplied)));
            } else { self.check_compact_expr_expected(value, expected_arg); }
            self.expected_schema = previous;
        }
        let erased_proc_call = matches!(callee_expr.kind, ArenaExprKind::Field { base, name } if name == "call"
            && self.output.expr_types.get(&base) == Some(&Type::Proc));
        if callee_ty == Type::Proc || erased_proc_call { self.invalidate_compact_mutable_proofs(); }
        if let Some(alias) = self.declarations.static_callable_aliases.get(&callee_expr.span).cloned() {
            self.apply_compact_call_expected(args, &alias.signature.params);
            return *alias.signature.return_ty;
        }
        let params = match callee_expr.kind {
            ArenaExprKind::Ident(name) => self.declarations.pures.get(&name).or_else(|| self.declarations.procs.get(&name)).map(|sig| sig.params.clone()),
            ArenaExprKind::Field { base, name } => if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind {
                let qualified = QualifiedName::new(module, name);
                self.declarations.qualified_pures.get(&qualified).or_else(|| self.declarations.qualified_procs.get(&qualified)).map(|sig| sig.params.clone())
            } else { None },
            _ => None,
        };
        if let Some(params) = params { self.apply_compact_call_expected(args, &params); }
        let erased_proc_call = matches!(callee_expr.kind, ArenaExprKind::Field { base, name } if name == "call"
            && self.output.expr_types.get(&base) == Some(&Type::Proc));
        if callee_ty == Type::Proc || erased_proc_call { self.invalidate_compact_mutable_proofs(); }
        if let Some(_definition) = self.declarations.record_constructors.resolve_call(
            &self.program.arena, callee, self.current_namespace,
        ) {
            let schema = self.declarations.record_constructor_types.get(&callee).cloned().or_else(|| self.declarations.record_constructors.constructor_type(&self.program.arena, callee, self.current_namespace)).unwrap_or(Type::Invalid);
            if let Type::Record(fields) = &schema {
                for arg in self.program.arena.call_args(args).to_vec() {
                    if let crate::syntax::arena::ArenaCallArgKind::Named { name, value, .. } = arg.kind && let Some(expected) = fields.get(&name) { self.apply_compact_expected(value, expected); }
                }
            }
            return schema;
        }
        if let ArenaExprKind::Ident(name) = callee_expr.kind {
            if name == "env" { return Type::Result(Box::new(Type::Str), Box::new(Type::Error)); }
            if name == "Path" && self.program.arena.call_args(args).len() == 1 { return Type::Path; }
            if name == "Ok" || name == "Err" {
                let value = self.program.arena.call_args(args).iter().find_map(|arg| match arg.kind {
                    crate::syntax::arena::ArenaCallArgKind::Positional(value) => self.output.expr_types.get(&value).cloned(),
                    _ => None,
                }).unwrap_or(Type::Unit);
                return if name == "Ok" { Type::Result(Box::new(value), Box::new(Type::Error)) }
                    else { Type::Result(Box::new(Type::Unknown), Box::new(value)) };
            }
        }
        if let ArenaExprKind::Field { base, name: variant } = callee_expr.kind {
            if let ArenaExprKind::Ident(family) = self.program.arena.expr(base).kind {
                if self.declarations.error_families_by_name.get(&family).is_some_and(|info| info.variants.contains_key(&variant)) {
                    return Type::ErrorVariant { family, variant };
                }
            }
        }
        if let ArenaExprKind::Ident(name) = callee_expr.kind {
            if let Some(sig) = self.declarations.pures.get(&name).or_else(|| self.declarations.procs.get(&name)) {
                let result = match &sig.return_ty {
                    Type::Unknown => self.type_from_arena(sig.return_type_expr),
                    Type::Result(ok, error) if **ok == Type::Unknown || **error == Type::Unknown => self.type_from_arena(sig.return_type_expr),
                    _ => sig.return_ty.clone(),
                };
                if self.declarations.procs.contains_key(&name) {
                    self.invalidate_compact_mutable_proofs();
                }
                return result;
            }
            if let Some(variant) = self.declarations.tag_variants_by_name.get(&name) {
                return Type::Tag(variant.type_name);
            }
        }
        if let ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name } = callee_expr.kind
            && let Some(receiver) = self.output.expr_types.get(&base).cloned()
        {
            let guarded = matches!(callee_expr.kind, ArenaExprKind::NullSafeField { .. });
            let (receiver, lift) = compact_postfix_receiver(receiver, guarded);
            if let Type::Module(exports) = receiver
                && let Some(ModuleExportType::Pure { sig, .. } | ModuleExportType::Proc { sig, .. }) = exports.get(&name)
            {
                self.apply_compact_call_expected(args, &sig.params);
                return compact_postfix_result(sig.return_ty.as_ref().clone(), lift);
            }
        }
        if let ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name } = callee_expr.kind
            && api_spec().method_overloads(crate::modules::signature::MethodReceiver::Record, &name.as_str()).is_some_and(|methods| methods.iter().any(|method| method.sig.semantic_rule == crate::modules::signature::SemanticRule::ConstantKeyProjection))
            && let Some(receiver) = self.output.expr_types.get(&base).cloned()
        {
            let guarded = matches!(callee_expr.kind, ArenaExprKind::NullSafeField { .. });
            let (receiver, lift) = compact_postfix_receiver(receiver, guarded);
            if let Some(projection) = crate::sema::projection::resolve_get_projection(
                &self.program.arena, &self.declarations.prepared_constants, base, &receiver,
                self.program.arena.call_args(args),
            ) {
                let ty = Type::Result(Box::new(projection.value_type.clone()), Box::new(Type::Error));
                self.output.projections.insert(id, projection);
                return compact_postfix_result(ty, lift);
            }
            if matches!(receiver, Type::Record(_) | Type::Module(_)) {
                return compact_postfix_result(Type::Result(Box::new(Type::Any), Box::new(Type::Error)), lift);
            }
        }
        if let Some(return_ty) = self.compact_builtin_call_type(callee, args, expected) {
            return return_ty;
        }
        match callee_ty {
            Type::Pure | Type::Proc => Type::Unknown,
            _ => Type::Unknown,
        }
    }

    fn apply_compact_call_expected(&mut self, args: crate::syntax::arena::ArenaRange, params: &[CallableParamType]) {
        let mut occupied = vec![false; params.len()];
        let mut positional = 0;
        for arg in self.program.arena.call_args(args).to_vec() {
            let (value, parameter) = match arg.kind {
                crate::syntax::arena::ArenaCallArgKind::Positional(value) => {
                    while occupied.get(positional) == Some(&true) { positional += 1; }
                    let selected = positional; positional += 1; (value, Some(selected))
                }
                crate::syntax::arena::ArenaCallArgKind::Named { name, value, .. } => (value, params.iter().position(|param| param.name == name)),
                crate::syntax::arena::ArenaCallArgKind::Splice { .. } | crate::syntax::arena::ArenaCallArgKind::NamedSpread { .. } => continue,
            };
            if let Some(index) = parameter && let Some(parameter) = params.get(index) {
                occupied[index] = true;
                self.apply_compact_expected(value, &parameter.ty);
            }
        }
    }

    fn compact_builtin_call_type(
        &mut self, callee: ExprId, args: crate::syntax::arena::ArenaRange,
        expected: Option<&Type>,
    ) -> Option<Type> {
        use crate::sema::arguments::{bind_static_arguments, expand_named_arguments, ArgumentValueSource};
        use crate::sema::builtin_templates::{BuiltinInstantiation, callable_parameters};
        use crate::modules::signature::MethodReceiver;
        let kind = self.program.arena.expr(callee).kind;
        let (ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name }) = kind else { return None; };
        let guarded = matches!(kind, ArenaExprKind::NullSafeField { .. });
        let actual = self.output.expr_types.get(&base).cloned().unwrap_or(Type::Unknown);
        let (receiver, lifted) = compact_postfix_receiver(actual, guarded);
        let class = match &receiver {
            Type::List(_) => Some(MethodReceiver::List), Type::Map(_, _) => Some(MethodReceiver::Map),
            Type::Stream(_) => Some(MethodReceiver::Stream), Type::Result(_, _) => Some(MethodReceiver::Result),
            Type::Record(_) => Some(MethodReceiver::Record), Type::Str => Some(MethodReceiver::Str),
            Type::Bytes => Some(MethodReceiver::Bytes), Type::Int | Type::UInt => Some(MethodReceiver::Int),
            Type::Float => Some(MethodReceiver::Float), Type::Path => Some(MethodReceiver::Path), Type::FsRoot => Some(MethodReceiver::FsRoot),
            Type::Status => Some(MethodReceiver::Status), Type::EnvPathList => Some(MethodReceiver::EnvPathList),
            Type::ProcessHandle => Some(MethodReceiver::ProcessHandle), Type::NetJob => Some(MethodReceiver::NetJob),
            Type::Digest => Some(MethodReceiver::Digest), Type::Regex => Some(MethodReceiver::Regex), _ => None,
        };
        let candidates = if let Some(class) = class {
            api_spec().method_overloads(class, &name.as_str())?.iter()
                .map(|method| (method.sig.clone(), method.receiver_ty.clone(), Some(receiver.clone()))).collect::<Vec<_>>()
        } else if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind {
            api_spec().module_overloads(&module.as_str(), &name.as_str())?.iter()
                .map(|signature| (signature.clone(), None, None)).collect()
        } else { return None; };
        let entries = self.program.arena.call_args(args).to_vec();
        let expanded = expand_named_arguments(self.program, &entries, |id| self.output.expr_types.get(&id).cloned()).ok()?;
        let (signature, template, receiver, binding) = candidates.into_iter().find_map(|(signature, template, receiver)| {
            let params = callable_parameters(&signature);
            bind_static_arguments(&params, &expanded).ok().map(|binding| (signature, template, receiver, binding))
        })?;
        let span = self.program.arena.expr(callee).span;
        let mut instance = BuiltinInstantiation::new(&signature, template.as_ref(), receiver.as_ref(), &mut self.type_constraints, span).ok()?;
        let expected = if lifted { expected.and_then(|ty| if let Type::Optional(inner) = ty { Some(inner.as_ref()) } else { None }) } else { expected };
        let descriptor_result = matches!(signature.semantic_rule, crate::modules::signature::SemanticRule::CliDescriptor | crate::modules::signature::SemanticRule::CliCommands);
        if !descriptor_result && let Some(expected) = expected { instance.constrain_result(expected, &mut self.type_constraints, span).ok()?; }
        let command_sources = crate::modules::cli::command_descriptor_sources(&expanded, &binding.argument_slots, &signature.params.iter().map(|parameter| crate::symbol::Name::intern(parameter.name)).collect::<Vec<_>>());
        for (entry, slot) in expanded.iter().zip(binding.argument_slots) {
            let parameter = self.type_constraints.resolve(&instance.signature.params[slot].ty).ok()?;
            let actual = match entry.value {
                ArgumentValueSource::Expression(value) => self.check_compact_expr_expected(value, Some(&parameter)),
                ArgumentValueSource::RecordField { .. } => entry.ty.clone(),
                ArgumentValueSource::PositionalSplice(_) => return None,
            };
            if self.type_constraints.constrain(&parameter, &actual, entry.span).is_err() { return Some(Type::Invalid); }
        }
        instance.resolve(&self.type_constraints);
        if signature.semantic_rule == crate::modules::signature::SemanticRule::CliCommands
            && let Some((commands, fallback)) = command_sources
            && let Some(plan) = self.declarations.prepared_constants.cli_commands_plan(&self.program.arena, commands, fallback) {
            match plan {
                Ok(plan) => instance.signature.return_ty = plan.return_type(false),
                Err(error) => self.error(error.span.unwrap_or(self.program.arena.expr(callee).span), &error.message, "check.cli-descriptor"),
            }
        }
        if signature.semantic_rule == crate::modules::signature::SemanticRule::CliDescriptor
            && let Some(schema) = crate::modules::cli::descriptor_argument(&entries)
            && let Some(plan) = self.declarations.prepared_constants.cli_descriptor_plan(
                &self.program.arena, schema, signature.op == xsh_registry::RuntimeOp::CliApplet,
            )
        {
            match plan {
                Ok(plan) => instance.signature.return_ty = plan.return_type(signature.op == xsh_registry::RuntimeOp::CliParseFull),
                Err(error) => self.error(error.span.unwrap_or(self.program.arena.expr(schema).span), &error.message, "check.cli-descriptor"),
            }
        }
        if descriptor_result && let Some(expected) = expected { instance.constrain_result(expected, &mut self.type_constraints, span).ok()?; }
        Some(compact_postfix_result(instance.signature.return_ty, lifted))
    }

    fn check_compact_field(&mut self, base: ExprId, name: Name) -> Type {
        match self.check_compact_expr(base) {
            Type::ErasedRecord | Type::DynamicModule => Type::Any,
            Type::Error | Type::ErrorFamily(_) | Type::ErrorVariant { .. } if name == "message" => Type::Str,
            Type::ErrorVariant { family, variant } => self.declarations.error_families_by_name.get(&family)
                .and_then(|info| info.variants.get(&variant)).and_then(|info| info.fields.get(&name)).cloned().unwrap_or(Type::Unknown),
            Type::Record(fields) => fields.get(&name).cloned().unwrap_or(Type::Unknown),
            Type::Module(exports) => exports
                .get(&name)
                .map(ModuleExportType::field_type)
                .unwrap_or(Type::Unknown),
            Type::Optional(inner) => match inner.as_ref() {
                Type::Record(fields) => fields.get(&name).cloned().unwrap_or(Type::Unknown),
                _ => Type::Unknown,
            },
            _ => Type::Unknown,
        }
    }

    fn check_compact_command(&mut self, id: crate::syntax::arena::CommandStmtId) {
        match &self.program.arena.command_stmt(id).command {
            ArenaCommand::Proc { args, .. } => {
                for arg in self.program.arena.command_args(*args) {
                    self.check_compact_command_arg(arg);
                }
            }
            ArenaCommand::Core {
                args, env, block, ..
            } => {
                for assignment in self.program.arena.env_assignments(*env) {
                    match assignment.value {
                        crate::syntax::arena::ArenaEnvAssignmentValue::Expr(expr) => {
                            self.check_compact_expr(expr);
                        }
                        crate::syntax::arena::ArenaEnvAssignmentValue::CommandArg(ref arg) => {
                            self.check_compact_command_arg(arg);
                        }
                    }
                }
                for arg in self.program.arena.command_args(*args) {
                    self.check_compact_command_arg(arg);
                }
                if let Some(block) = block {
                    self.check_compact_block(*block);
                }
            }
            ArenaCommand::Run(run) => {
                self.output.runs += 1;
                self.check_compact_run(*run);
            }
        }
    }

    fn check_compact_run(&mut self, id: crate::syntax::arena::RunFormId) {
        for segment in self
            .program
            .arena
            .run_segments(self.program.arena.run_form(id).segments)
        {
            if let Some(timeout) = segment.timeout {
                self.check_compact_expr(timeout);
            }
            if let Some(cpu_max) = segment.cpu_max {
                self.check_compact_expr(cpu_max);
            }
            if let Some(accept) = segment.accept {
                self.check_compact_expr(accept);
            }
            for assignment in self.program.arena.env_assignments(segment.env) {
                match assignment.value {
                    crate::syntax::arena::ArenaEnvAssignmentValue::Expr(expr) => {
                        self.check_compact_expr(expr);
                    }
                    crate::syntax::arena::ArenaEnvAssignmentValue::CommandArg(ref arg) => {
                        self.check_compact_command_arg(arg);
                    }
                }
            }
            self.check_compact_command_arg(&segment.target);
            for arg in self.program.arena.command_args(segment.args) {
                self.check_compact_command_arg(arg);
            }
            for redirection in self.program.arena.redirections(segment.redirections) {
                match &redirection.target {
                    crate::syntax::arena::ArenaRedirectionTarget::Path(arg)
                    | crate::syntax::arena::ArenaRedirectionTarget::Fd(arg) => {
                        self.check_compact_command_arg(arg);
                    }
                }
            }
        }
    }

    fn check_compact_command_arg(&mut self, arg: &crate::syntax::arena::ArenaCommandArg) {
        match &arg.kind {
            ArenaCommandArgKind::Typed(expr) | ArenaCommandArgKind::SpliceExpr(expr) => {
                self.check_compact_expr(*expr);
            }
            ArenaCommandArgKind::Word(parts) => {
                for part in self.program.arena.word_parts(*parts) {
                    match part {
                        crate::syntax::arena::ArenaWordPart::Shorthand(expr)
                        | crate::syntax::arena::ArenaWordPart::Interpolation(expr) => {
                            self.check_compact_expr(expr);
                        }
                        crate::syntax::arena::ArenaWordPart::Bare(_)
                        | crate::syntax::arena::ArenaWordPart::Quoted(_) => {}
                    }
                }
            }
            ArenaCommandArgKind::SpliceName(_) => {}
        }
    }

    fn check_compact_pipe_stages(&mut self, stages: crate::syntax::arena::ArenaRange) {
        for stage in self.program.arena.pipe_stages(stages) {
            match &stage.kind {
                ArenaPipeStageKind::Expr(expr) => {
                    self.check_compact_expr(*expr);
                }
                ArenaPipeStageKind::Stream(stream) => {
                    self.check_compact_stream_stage(stream);
                }
            }
        }
    }

    fn check_compact_stream_stages(&mut self, stages: crate::syntax::arena::ArenaRange, mut current: Type) -> Type {
        for stage in self.program.arena.stream_stages(stages) {
            let fact = self.declarations.stream_stage_types.get(&(self.current_namespace, self.program.arena.span(stage.span)));
            let input = fact.map(|fact| &fact.input).unwrap_or(&current);
            let item = match input { Type::List(item) | Type::Stream(item) => item.as_ref().clone(), _ => Type::Any };
            self.check_compact_stream_stage_with_item(stage, item);
            current = fact.map(|fact| fact.output.clone()).unwrap_or(Type::Unknown);
        }
        if let Type::Stream(item) = current { Type::List(item) } else { current }
    }

    fn check_compact_stream_stage(&mut self, stream: &crate::syntax::arena::ArenaStreamStage) {
        self.check_compact_stream_stage_with_item(stream, Type::Any);
    }

    fn check_compact_stream_stage_with_item(&mut self, stream: &crate::syntax::arena::ArenaStreamStage, item: Type) {
        for arg in self.program.arena.call_args(stream.args) {
            match &arg.kind {
                crate::syntax::arena::ArenaCallArgKind::Positional(expr)
                | crate::syntax::arena::ArenaCallArgKind::Splice { value: expr, .. } | crate::syntax::arena::ArenaCallArgKind::NamedSpread { value: expr, .. }
                | crate::syntax::arena::ArenaCallArgKind::Named { value: expr, .. } => {
                    self.check_compact_expr(*expr);
                }
            }
        }
        match crate::sema::stage_arguments::stage_callable_argument(self.program, stream, |expr| self.output.expr_types.get(&expr).cloned()) {
            Ok(Some((callee, _))) => {
                let static_name = self.declarations.static_callable_aliases.contains_key(&self.program.arena.expr(callee).span) || match self.program.arena.expr(callee).kind {
                    ArenaExprKind::Ident(name) => !self.scopes.iter().skip(1).any(|scope| scope.contains_key(&name))
                        && (self.declarations.pures.contains_key(&name) || self.declarations.procs.contains_key(&name)
                            || self.current_namespace.is_some_and(|namespace| {
                                let qualified = QualifiedName::new(namespace, name);
                                self.declarations.qualified_pures.contains_key(&qualified) || self.declarations.qualified_procs.contains_key(&qualified)
                            })),
                    ArenaExprKind::Field { base, name } => matches!(self.program.arena.expr(base).kind, ArenaExprKind::Ident(namespace)
                        if !self.scopes.iter().skip(1).any(|scope| scope.contains_key(&namespace))
                            && (api_spec().module_overloads(&namespace.as_str(), &name.as_str()).is_some()
                                || crate::sema::stage_arguments::stage_namespace_owner(self.program, namespace, self.current_namespace)
                                    .is_some_and(|owner| {
                                        let key = QualifiedName::new(owner, name);
                                        self.declarations.qualified_pures.contains_key(&key) || self.declarations.qualified_procs.contains_key(&key)
                                    }))),
                    _ => false,
                };
                if !static_name {
                    self.error(self.program.arena.expr(callee).span, "stage callable must be a statically resolved named function or proc", "check.stream-callable");
                    return;
                }
                let mut temporary = self.program.clone();
                let (_, item_expr, call, _) = temporary.arena.append_stage_callable_block(callee, self.program.arena.expr(callee).span);
                let mut child = CompactBodyProbe {
                    condition_proofs: self.condition_proofs.clone(),
                    type_constraints: self.type_constraints.clone(),
                    nonmaterial_expressions: self.nonmaterial_expressions.clone(),
                    expected_schema: self.expected_schema.clone(),
                    program: &temporary, declarations: self.declarations, output: std::mem::take(&mut self.output),
                    scopes: self.scopes.clone(), stream_items: vec![item.clone()], return_types: self.return_types.clone(), return_schemas: self.return_schemas.clone(),
                    pipeline_hole_types: self.pipeline_hole_types.clone(), current_namespace: self.current_namespace,
                    with_initializer_errors: self.with_initializer_errors.clone(),
                };
                let mut ty = child.check_compact_expr(call);
                match temporary.arena.expr(callee).kind {
                    ArenaExprKind::Ident(name) => {
                        if let Some(namespace) = self.current_namespace {
                            let qualified = QualifiedName::new(namespace, name);
                            if let Some(sig) = self.declarations.qualified_pures.get(&qualified).or_else(|| self.declarations.qualified_procs.get(&qualified)) { ty = sig.return_ty.clone(); }
                        }
                    }
                    ArenaExprKind::Field { base, name } => {
                        if let ArenaExprKind::Ident(namespace) = temporary.arena.expr(base).kind {
                            if let Some(owner) = crate::sema::stage_arguments::stage_namespace_owner(self.program, namespace, self.current_namespace) {
                                let key = QualifiedName::new(owner, name);
                                if let Some(sig) = self.declarations.qualified_pures.get(&key).or_else(|| self.declarations.qualified_procs.get(&key)) { ty = sig.return_ty.clone(); }
                            }
                            if let Type::Module(exports) = self.lookup_name(namespace)
                                && let Some(ModuleExportType::Pure { sig, .. } | ModuleExportType::Proc { sig, .. }) = exports.get(&name) { ty = sig.return_ty.as_ref().clone(); }
                            if let Some(overloads) = api_spec().module_overloads(&namespace.as_str(), &name.as_str()) {
                                let matches = overloads.iter().filter(|sig| !sig.params.is_empty()
                                    && sig.params.iter().skip(1).all(|param| param.defaulted)
                                    && item.matches_expected(&sig.params[0].ty)).collect::<Vec<_>>();
                                if matches.len() == 1 { ty = matches[0].return_ty.clone(); }
                                else { ty = Type::Invalid; }
                            }
                        }
                    }
                    _ => {}
                }
                child.output.expr_types.remove(&item_expr);
                child.output.expr_types.remove(&call);
                self.type_constraints = child.type_constraints;
                self.nonmaterial_expressions = child.nonmaterial_expressions;
                self.output = child.output;
                self.output.stage_callable_types.insert(callee, ty);
                return;
            }
            Err((span, message)) => { self.error(span, &message, "check.stream-callable"); return; }
            Ok(None) => {}
        }
        if let Some(block) = stream.block {
            self.push_compact_deferred_capture_scope();
            if matches!(stream.kind, crate::syntax::node::StreamStageKind::Fold | crate::syntax::node::StreamStageKind::Reduce) {
                use crate::sema::arguments::{expand_named_arguments, bind_static_arguments};
                let parameters = crate::sema::stage_arguments::stage_argument_params(stream.kind.as_str());
                let accumulator = expand_named_arguments(self.program, self.program.arena.call_args(stream.args), |expression| self.output.expr_types.get(&expression).cloned())
                    .ok().and_then(|arguments| {
                        let binding = bind_static_arguments(&parameters, &arguments).ok()?;
                        arguments.into_iter().zip(binding.argument_slots).find_map(|(argument, slot)| (slot == 0).then_some(argument.ty))
                    }).unwrap_or(Type::Unknown);
                for (parameter, ty) in self.program.arena.block_params(self.program.arena.block(block).params).iter().zip([accumulator, item.clone()]) {
                    self.current_scope_mut().insert(parameter.name, CompactBinding::new(ty, false));
                }
            }
            self.stream_items.push(item);
            self.check_compact_block_in_current_scope(block);
            self.mark_tail_position(block, !matches!(stream.kind, crate::syntax::node::StreamStageKind::Each | crate::syntax::node::StreamStageKind::Tee));
            self.stream_items.pop();
            self.pop_scope();
        }
    }

    fn check_compact_builder_block(&mut self, id: crate::syntax::arena::BuilderBlockId) {
        let block = self.program.arena.builder_block(id);
        for entry in self.program.arena.builder_entries(block.entries) {
            match entry.kind {
                ArenaBuilderEntryKind::Field { value, .. } => {
                    self.check_compact_expr(value);
                }
                ArenaBuilderEntryKind::Entry { args, block, .. } => {
                    for arg in self.program.arena.command_args(args) {
                        self.check_compact_command_arg(arg);
                    }
                    if let Some(block) = block {
                        self.check_compact_builder_block(block);
                    }
                }
                ArenaBuilderEntryKind::Task { block, .. } => {
                    self.check_compact_block(block);
                    self.mark_tail_position(block, false);
                }
                ArenaBuilderEntryKind::Stmt(stmt) => {
                    self.check_compact_stmt(stmt);
                }
            }
        }
    }

    fn bind_compact_pattern(&mut self, pattern: PatternId, subject: &Type) {
        match self.program.arena.pattern(pattern).kind {
            ArenaPatternKind::Group(child) => self.bind_compact_pattern(child, subject),
            ArenaPatternKind::Alias { pattern, name, .. } => {
                self.bind_compact_pattern(pattern, subject);
                self.current_scope_mut().insert(name, CompactBinding::new(subject.clone(), false));
            }
            ArenaPatternKind::Alternation(children) => {
                if let Some(child) = self.program.arena.pattern_ids(children).next() { self.bind_compact_pattern(child, subject); }
            }
            ArenaPatternKind::Binding(name) if !self.declarations.tag_variants_by_name.contains_key(&name) => {
                self.current_scope_mut().insert(name, CompactBinding::new(subject.clone(), false));
            }
            ArenaPatternKind::Type { binding: Some(name), ty } => {
                let ty = self.type_from_arena(ty);
                self.current_scope_mut().insert(name, CompactBinding::new(ty, false));
            }
            ArenaPatternKind::List { elements, rest } => {
                let element = match subject { Type::List(element) => element.as_ref().clone(), Type::Any => Type::Any, _ => Type::Unknown };
                let children: Vec<_> = self.program.arena.pattern_ids(elements).collect();
                for child in children { self.bind_compact_pattern(child, &element); }
                if let Some(rest) = rest { self.bind_compact_pattern(rest, &Type::List(Box::new(element))); }
            }
            ArenaPatternKind::Record { fields, .. } => {
                for field in self.program.arena.pattern_fields(fields).to_vec() {
                    let ty = match subject { Type::Record(fields) => fields.get(&field.name).cloned().unwrap_or(Type::Unknown), Type::Any => Type::Any, _ => Type::Unknown };
                    self.bind_compact_pattern(field.pattern, &ty);
                }
            }
            ArenaPatternKind::Constructor { name, arg: Some(arg) } => {
                if let Type::Result(ok, err) = subject && (name == "Ok" || name == "Err") {
                    self.bind_compact_pattern(arg, if name == "Ok" { ok } else { err });
                } else if let Some(info) = self.declarations.tag_variants_by_name.get(&name).cloned() {
                    let children = match self.program.arena.pattern(arg).kind {
                        ArenaPatternKind::Tuple(children) => self.program.arena.pattern_ids(children).collect::<Vec<_>>(),
                        _ => vec![arg],
                    };
                    for (child, ty) in children.into_iter().zip(info.field_types) { self.bind_compact_pattern(child, &ty); }
                } else { self.bind_compact_pattern(arg, &Type::Unknown); }
            }
            ArenaPatternKind::ErrorVariant { family, variant, fields } => {
                let payload = self.declarations.error_families_by_name.get(&family)
                    .and_then(|info| info.variants.get(&variant)).map(|info| info.fields.clone()).unwrap_or_default();
                for field in self.program.arena.pattern_fields(fields).to_vec() {
                    self.bind_compact_pattern(field.pattern, payload.get(&field.name).unwrap_or(&Type::Unknown));
                }
            }
            ArenaPatternKind::Tuple(children) => {
                let children: Vec<_> = self.program.arena.pattern_ids(children).collect();
                for child in children { self.bind_compact_pattern(child, &Type::Unknown); }
            }
            _ => {}
        }
    }

    fn bind_compact_pattern_condition(&mut self, condition: ExprId) {
        if let ArenaExprKind::PatternCondition { value, arms } = self.program.arena.expr(condition).kind {
            let ty = self.output.expr_types.get(&value).cloned().unwrap_or(Type::Unknown);
            self.bind_compact_pattern(self.program.arena.match_expr_arms(arms)[0].pattern, &ty);
        }
    }

    fn define_binding_target(
        &mut self,
        id: crate::syntax::arena::BindingTargetId,
        ty: Type,
        mutable: bool,
    ) {
        match &self.program.arena.binding_target(id).kind {
            ArenaBindingTargetKind::Name(name) => {
                self.current_scope_mut()
                    .insert(*name, CompactBinding::new(ty, mutable));
            }
            ArenaBindingTargetKind::Record { fields, .. } => {
                let record_fields = match &ty {
                    Type::Record(fields) => Some(fields),
                    _ => None,
                };
                let field_rows = self.program.arena.destructure_fields(*fields).to_vec();
                for field in field_rows {
                    let field_ty = record_fields
                        .and_then(|fields| fields.get(&field.name))
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    self.define_binding_target(field.target, field_ty, mutable);
                }
            }
        }
    }

    fn compact_assign_target_type(&self, target: crate::syntax::arena::AssignTargetId) -> Option<Type> {
        match self.program.arena.assign_target(target).kind {
            ArenaAssignTargetKind::Name(name) => Some(self.lookup_name(name)),
            ArenaAssignTargetKind::Field { base, name } => match self.compact_assign_target_type(base)? { Type::Record(fields) => fields.get(&name).cloned(), _ => None },
            ArenaAssignTargetKind::Index { base, .. } => match self.compact_assign_target_type(base)? { Type::Map(_, item) | Type::List(item) => Some(*item), _ => None },
        }
    }

    fn check_compact_assign_target(
        &mut self,
        id: crate::syntax::arena::AssignTargetId,
        span: crate::source::Span,
    ) {
        if let Some(name) = self.assign_target_root_name(id) {
            match self.lookup_binding(name) {
                Some(binding) if !binding.mutable => {
                    self.error(
                        span,
                        "assignment to immutable `let` binding; declare with `var` to allow reassignment",
                        "check.assign-let",
                    );
                }
                Some(_) => {}
                None => self.error(span, "assignment to undefined name", "check.undefined-name"),
            }
        }
        self.walk_assign_target(id);
    }

    fn walk_assign_target(&mut self, id: crate::syntax::arena::AssignTargetId) {
        self.output.assignment_targets += 1;
        match &self.program.arena.assign_target(id).kind {
            ArenaAssignTargetKind::Name(_) => {}
            ArenaAssignTargetKind::Field { base, .. } => self.walk_assign_target(*base),
            ArenaAssignTargetKind::Index { base, index } => {
                self.walk_assign_target(*base);
                self.check_compact_expr(*index);
            }
        }
    }

    fn assign_target_root_name(&self, id: crate::syntax::arena::AssignTargetId) -> Option<Name> {
        match &self.program.arena.assign_target(id).kind {
            ArenaAssignTargetKind::Name(name) => Some(*name),
            ArenaAssignTargetKind::Field { base, .. }
            | ArenaAssignTargetKind::Index { base, .. } => self.assign_target_root_name(*base),
        }
    }

    fn lookup_name(&self, name: Name) -> Type {
        if let Some(binding) = self.lookup_binding(name) {
            return self.type_constraints.resolve(&binding.ty).unwrap_or_else(|_| binding.ty.clone());
        }
        if let Some(variant) = self.declarations.tag_variants_by_name.get(&name)
            && variant.field_count == 0
        {
            return Type::Tag(variant.type_name);
        }
        if self.declarations.procs.contains_key(&name) {
            return Type::Proc;
        }
        if self.declarations.pures.contains_key(&name) {
            return Type::Pure;
        }
        if let Some(sig) = self.declarations.streams.get(&name) {
            return Type::Stream(Box::new(stream_item_type(&sig.return_ty)));
        }
        Type::Unknown
    }

    fn lookup_binding(&self, name: Name) -> Option<&CompactBinding> {
        for scope in self.scopes.iter().rev() {
            if let Some(binding) = scope.get(&name) {
                return Some(binding);
            }
        }
        None
    }

    fn push_scope(&mut self) {
        self.scopes.push(FxHashMap::default());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn current_scope_mut(&mut self) -> &mut FxHashMap<Name, CompactBinding> {
        self.scopes
            .last_mut()
            .expect("compact body probe without root scope")
    }

    fn error(&mut self, span: crate::source::Span, message: &str, code: &str) {
        self.output.diagnostics.push(
            Diagnostic::error(message)
                .with_code(code)
                .with_label(Label::primary(span, message)),
        );
    }
}

fn stream_item_type(ty: &Type) -> Type {
    match ty {
        Type::Stream(item) | Type::List(item) => item.as_ref().clone(),
        Type::Result(ok, _) => stream_item_type(ok),
        Type::Unknown | Type::Invalid => Type::Unknown,
        _ => Type::Unknown,
    }
}

fn compact_probe_type_from_arena(
    arena: &AstArena,
    id: TypeExprId,
    declarations: &CompactDeclOutput,
    depth: usize,
) -> Type {
    if depth > declarations.types.len() {
        return Type::Unknown;
    }
    let index = id.index();
    let tag = arena.type_expr_tags[index];
    let data = arena.type_expr_data[index];
    match tag {
        ArenaTypeExprTag::Applied => declarations.record_constructors.resolve_type(arena, id, None),
        ArenaTypeExprTag::Named => {
            let name = Name::from_symbol(Symbol::from_raw(data.lhs));
            if declarations.error_families_by_name.contains_key(&name) {
                return if name == "ProcessError" { Type::ProcessError } else { Type::ErrorFamily(name) };
            }
            match Type::from_name(&name.as_str()) {
                Type::Unknown => match declarations.types.get(&name) {
                    Some(CompactTypeDefInfo::Alias(alias)) => {
                        compact_probe_type_from_arena(arena, *alias, declarations, depth + 1)
                    }
                    Some(CompactTypeDefInfo::Record(_)) => {
                        compact_probe_record_type(arena, name, declarations, depth + 1)
                    }
                    Some(CompactTypeDefInfo::Module(exports)) => Type::Module(exports.clone()),
                    Some(CompactTypeDefInfo::TagUnion) => Type::Tag(name),
                    None => Type::Unknown,
                },
                ty => ty,
            }
        }
        ArenaTypeExprTag::Qualified => {
            let name = Name::from_symbol(Symbol::from_raw(data.rhs));
            let namespace = Name::from_symbol(Symbol::from_raw(data.lhs));
            if declarations.qualified_error_families.contains_key(&QualifiedName::new(namespace, name)) {
                return Type::ErrorFamily(Name::intern(format!("{namespace}.{name}")));
            }
            match declarations.types.get(&name) {
                Some(CompactTypeDefInfo::Alias(alias)) => {
                    compact_probe_type_from_arena(arena, *alias, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Record(_)) => {
                    compact_probe_record_type(arena, name, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Module(exports)) => Type::Module(exports.clone()),
                Some(CompactTypeDefInfo::TagUnion) => Type::Tag(name),
                None => Type::Unknown,
            }
        }
        ArenaTypeExprTag::List => Type::List(Box::new(compact_probe_type_from_arena(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        ))),
        ArenaTypeExprTag::Map => Type::Map(Box::new(TypeExprId::from_optional_raw(data.rhs).map_or(Type::Str, |id| compact_probe_type_from_arena(arena, id, declarations, depth))), Box::new(compact_probe_type_from_arena(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        ))),
        ArenaTypeExprTag::Stream => Type::Stream(Box::new(compact_probe_type_from_arena(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        ))),
        ArenaTypeExprTag::Module => {
            let inner = compact_probe_type_from_arena(
                arena,
                TypeExprId::from_index(data.lhs as usize),
                declarations,
                depth,
            );
            Type::Module(BTreeMap::from([(
                Name::intern("<schema>"),
                ModuleExportType::Value {
                    ty: inner,
                    optional: false,
                },
            )]))
        }
        ArenaTypeExprTag::Result => Type::Result(
            Box::new(compact_probe_type_from_arena(
                arena,
                TypeExprId::from_index(data.lhs as usize),
                declarations,
                depth,
            )),
            Box::new(
                TypeExprId::from_optional_raw(data.rhs).map_or(Type::Error, |err| {
                    compact_probe_type_from_arena(arena, err, declarations, depth)
                }),
            ),
        ),
        ArenaTypeExprTag::Optional => Type::Optional(Box::new(compact_probe_type_from_arena(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        ))),
    }
}

fn compact_probe_record_type(
    arena: &AstArena,
    name: Name,
    declarations: &CompactDeclOutput,
    depth: usize,
) -> Type {
    if let Some(fields) = declarations.record_schema_fields.get(&name) {
        return Type::Record(
            fields
                .iter()
                .map(|(field, ty)| {
                    (
                        *field,
                        compact_probe_type_from_arena(arena, *ty, declarations, depth),
                    )
                })
                .collect(),
        );
    }
    match declarations.types.get(&name) {
        Some(CompactTypeDefInfo::Record(fields)) => Type::Record(fields.clone()),
        _ => Type::Unknown,
    }
}

fn collection_item_type(ty: &Type) -> Type {
    if let Some(item) = ty.iteration_item_type() { return item; }
    match ty {
        Type::List(item) | Type::Stream(item) | Type::Map(_, item) => item.as_ref().clone(),
        Type::Str => Type::Str,
        Type::Bytes => Type::Int,
        Type::ErasedRecord | Type::Record(_) => Type::Any,
        Type::Unknown | Type::Invalid | Type::Any => ty.clone(),
        _ => Type::Unknown,
    }
}

fn compact_postfix_receiver(ty: Type, guarded: bool) -> (Type, bool) {
    if !guarded {
        return (ty, false);
    }
    match ty {
        Type::Optional(inner) => (*inner, true),
        Type::Result(inner, _) => (*inner, false),
        other => (other, false),
    }
}

fn compact_postfix_result(ty: Type, lift: bool) -> Type {
    if lift && !matches!(ty, Type::Optional(_)) {
        Type::Optional(Box::new(ty))
    } else {
        ty
    }
}

fn compact_field_type(ty: Type, name: Name) -> Type {
    match ty {
        Type::Record(fields) => fields.get(&name).cloned().unwrap_or(Type::Unknown),
        Type::Module(exports) => exports.get(&name).map(ModuleExportType::field_type).unwrap_or(Type::Unknown),
        Type::ProcessHandle => match name.as_str().as_str() {
            "pid" => Type::Int,
            "command" => Type::Str,
            "argv" => Type::List(Box::new(Type::Str)),
            "detached" => Type::Bool,
            _ => Type::Unknown,
        },
        Type::Error | Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ProcessError
            if name == "message" => Type::Str,
        Type::Any => Type::Any,
        _ => Type::Unknown,
    }
}

fn index_type(ty: &Type) -> Type {
    match ty {
        Type::List(item) | Type::Map(_, item) | Type::Stream(item) => item.as_ref().clone(),
        Type::Str => Type::Str,
        Type::Bytes => Type::Int,
        Type::Unknown | Type::Invalid | Type::Any => ty.clone(),
        _ => Type::Unknown,
    }
}

fn numeric_result_type(left: Type, right: Type) -> Type {
    if matches!(left, Type::Float) || matches!(right, Type::Float) {
        Type::Float
    } else if matches!(left, Type::Duration) && matches!(right, Type::Duration | Type::Int) {
        Type::Duration
    } else if matches!(left, Type::Int | Type::UInt) && matches!(right, Type::Int | Type::UInt) {
        Type::Int
    } else {
        Type::Unknown
    }
}

fn merge_types(current: Option<Type>, next: Type) -> Type {
    match current {
        None => next,
        Some(current) if current.matches_expected(&next) => current,
        Some(current) if next.matches_expected(&current) => next,
        Some(Type::Unknown | Type::Invalid) => next,
        Some(_) if matches!(next, Type::Unknown | Type::Invalid) => next,
        Some(_) => Type::Any,
    }
}
