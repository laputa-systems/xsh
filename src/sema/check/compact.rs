#![allow(clippy::single_call_fn)]

use super::decl::is_builtin_or_standard_record_type_name;
use super::{
    BTreeMap, Checker, CoreCommand, Diagnostic, ErrorFamilyInfo, ErrorVariantInfo, FxHashMap,
    FxHashSet, Label, Name, Span, TagVariantInfo, Type, api_spec,
};
use crate::diagnostic::DiagnosticCode;
use crate::sema::types::{CallableParamType, CallableType, ModuleExportType};
use crate::symbol::QualifiedName;
use crate::syntax::arena::{
    ArenaErrorDef, ArenaExprKind, ArenaModuleContractEntryKind, ArenaProgram, ArenaStmtKind,
    ArenaTypeDef, ArenaTypeDefBody, BlockId, ErrorDefId, ExprId, FunctionDefId, StmtId, TypeDefId,
    TypeExprId,
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
    pub record_constructor_types: FxHashMap<ExprId, Type>,
    pub bodies: CompactBodyFacts,
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
    pub expr_types: FxHashMap<ExprId, Type>,
    pub projections: FxHashSet<ExprId>,
    /// Optional receivers whose checked presence proof makes their fallback unreachable.
    pub proven_nonnull_fallback_receivers: FxHashSet<ExprId>,
    pub requirement_targets: FxHashMap<ExprId, super::RequirementTarget>,
    /// Registered method and module function calls keyed by call expression;
    /// a stage callable's plan is keyed by its callee.
    pub api_calls: FxHashMap<ExprId, super::CheckedApiCall>,
    /// User callable calls and structured stages keyed by their source span.
    pub argument_bindings: BTreeMap<Span, super::CheckedArguments>,
    pub statement_positions: FxHashMap<StmtId, super::StatementPosition>,
    /// `with` handler slots retain the common nominal error of their checked inputs.
    pub handler_input_types: FxHashMap<BlockId, Type>,
    /// Leading-dot variants keyed by the constructing call or member expression.
    pub inferred_variants: FxHashMap<ExprId, super::InferredVariant>,
    /// The schema field each record constructor argument supplies, keyed by call.
    pub record_constructor_fields: FxHashMap<ExprId, Vec<Name>>,
}

impl CompactBodyFacts {
    fn collect(
        program: &ArenaProgram,
        checked: &super::CheckOutput,
    ) -> (Self, FxHashMap<ExprId, Type>) {
        let arena = &program.arena;
        let mut facts = Self {
            argument_bindings: checked.argument_bindings.clone(),
            ..Self::default()
        };
        let mut record_constructor_types = FxHashMap::default();
        facts.expr_types.reserve(checked.expr_types.len());
        for index in 0..arena.expr_tags.len() {
            let id = ExprId::from_index(index);
            let expression = arena.expr(id);
            let span = expression.span;
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
            if let Some(call) = checked.api_calls.get(&span) {
                facts.api_calls.insert(id, call.clone());
            }
            if let Some(variant) = checked.inferred_variants.get(&span) {
                facts.inferred_variants.insert(id, variant.clone());
            }
            if let Some(fields) = checked.record_constructor_fields.get(&span) {
                facts.record_constructor_fields.insert(id, fields.clone());
            }
            if let ArenaExprKind::Call { callee, .. } = expression.kind
                && let Some(fact) = checked.record_constructor_instances.get(&span)
            {
                record_constructor_types.insert(callee, fact.ty.clone());
            }
        }
        for index in 0..arena.stmt_tags.len() {
            let id = StmtId::from_index(index);
            if let Some(position) = checked.statement_positions.get(&arena.stmt(id).span) {
                facts.statement_positions.insert(id, *position);
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
        (facts, record_constructor_types)
    }
}

impl Checker {
    /// Check `program` for lowering when no entry check has run: embedded
    /// modules, directly prepared programs, and tests. A caller that already
    /// checked the program with `CheckOptions::embedded_bodies` passes that
    /// output to `compact_declarations` instead, so each body is checked once.
    pub fn check_compact_declarations(program: &ArenaProgram) -> CompactDeclOutput {
        // An entry check owns the `reveal_type` gate; a program prepared
        // without one must not be rejected for it here. Without the entry text
        // the check cannot judge source spelling and reports nothing for it.
        let options = super::CheckOptions {
            reveal_types: true,
            embedded_bodies: true,
            ..super::CheckOptions::default()
        };
        let checked = Checker::check_arena_with_options(program, "", options);
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
            let (bodies, record_constructor_types) = CompactBodyFacts::collect(program, &checked);
            let mut collector = CompactDeclCollector {
                diagnostics: Vec::new(),
                names: FxHashSet::default(),
                output: CompactDeclOutput {
                    record_constructors: super::RecordConstructors::collect(program),
                    bodies,
                    record_constructor_types,
                    function_effect_facts: checked.function_effect_facts,
                    stream_stage_types: checked.stream_stage_types,
                    static_callable_aliases: checked.static_callable_aliases,
                    parameter_types: checked.parameter_types,
                    local_binding_types: checked.local_binding_types,
                    function_return_types: checked.function_return_types,
                    ..CompactDeclOutput::default()
                },
            };
            collector.output.error_families_by_name =
                Checker::new(super::CheckOptions::default()).error_families;
            collector
                .diagnostics
                .extend(Self::prepare_regex_literals(program));
            collector.collect_program(program);
            let mut output = collector.output;
            output.prepared_constants = crate::sema::constants::PreparedConstants::collect(
                program,
                &output.record_constructors,
            );
            collector
                .diagnostics
                .extend(output.prepared_constants.diagnostics.clone());
            output
                .record_constructors
                .apply_prepared_defaults(program, &output.prepared_constants);
            let (wire_enums, wire_diagnostics) =
                crate::sema::wire_enums::PreparedWireEnums::prepare(program, |expr| {
                    output
                        .prepared_constants
                        .analyze_expression(&program.arena, expr)
                });
            output.wire_enums = wire_enums;
            collector.diagnostics.extend(wire_diagnostics);
            let (entry, diagnostics) = crate::sema::cli_entry::validate_cli_entry(
                program,
                |parameter| {
                    output
                        .parameter_types
                        .get(&program.arena.span(parameter.span))
                        .cloned()
                        .unwrap_or_else(|| {
                            output.record_constructors.resolve_type(
                                &program.arena,
                                parameter.ty,
                                None,
                            )
                        })
                },
                |ty| {
                    output
                        .record_constructors
                        .cli_parser_type(&program.arena, ty)
                },
                |expr| {
                    output
                        .prepared_constants
                        .analyze_expression(&program.arena, expr)
                },
            );
            output.cli_entry = entry;
            collector.diagnostics.extend(diagnostics);
            output.diagnostics = collector.diagnostics;
            output.diagnostics.extend(
                checked
                    .diagnostics
                    .into_iter()
                    .filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error),
            );
            output
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
                self.output.params += program
                    .arena
                    .params(program.arena.function_def(def).params)
                    .len();
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
                            DiagnosticCode::CheckDuplicateRecordField,
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
            ArenaTypeDefBody::ModuleContract { entries, exact } => {
                let mut names = FxHashSet::default();
                let entries = program.arena.module_contract_entries(entries);
                self.output.module_contract_entries += entries.len();
                let mut exports = BTreeMap::new();
                for entry in entries {
                    if !names.insert(entry.name) {
                        self.error(
                            program.arena.span(entry.span),
                            "duplicate module contract export",
                            DiagnosticCode::CheckDuplicateName,
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
                CompactTypeDefInfo::Module(std::sync::Arc::new(crate::sema::types::ModuleType { exports, exact }))
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
                        type_name: crate::sema::wire_enums::nominal_enum_name(
                            namespace.or(program.root_nominal_namespace),
                            def.name,
                        ),
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
                    DiagnosticCode::CheckDuplicateName,
                );
            }
            let fields = program.arena.error_fields(variant.fields);
            self.output.error_fields += fields.len();
            let mut field_names = FxHashSet::default();
            let mut field_types = Vec::with_capacity(fields.len());
            for field in fields {
                if !field_names.insert(field.name) {
                    self.error(
                        program.arena.span(field.span),
                        "duplicate error payload field",
                        DiagnosticCode::CheckDuplicateRecordField,
                    );
                }
                field_types.push((field.name, Type::from_arena(&program.arena, field.ty)));
            }
            let facets = program.arena.names(variant.facets).collect::<Vec<_>>();
            family_variants.insert(
                variant.name,
                ErrorVariantInfo::declared(field_types, facets),
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
                    DiagnosticCode::CheckCoreCommandShadow,
                );
            }
        }
        if !self.names.insert(def.name) {
            self.error(
                span,
                "duplicate top-level name",
                DiagnosticCode::CheckDuplicateName,
            );
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

    fn function_sig(
        &mut self,
        program: &ArenaProgram,
        id: FunctionDefId,
        namespace: Option<Name>,
    ) -> CompactFunctionSig {
        let def = program.arena.function_def(id);
        let mut params = self.param_sigs(program, def.params);
        let parameter_schemas = program
            .arena
            .params(def.params)
            .iter()
            .zip(&mut params)
            .map(|(syntax, param)| {
                param.ty = self.output.record_constructors.resolve_type(
                    &program.arena,
                    syntax.ty,
                    namespace,
                );
                self.output
                    .record_constructors
                    .annotation_expectation(&program.arena, syntax.ty, namespace)
                    .ok()
            })
            .collect();
        let body_span = program.arena.span(program.arena.block(def.body).span);
        let return_ty = self
            .output
            .function_return_types
            .get(&body_span)
            .cloned()
            .unwrap_or_else(|| Type::from_arena(&program.arena, def.return_ty));
        let effects = self
            .output
            .function_effect_facts
            .get(&super::EffectDeclarationId {
                namespace,
                body: body_span,
            })
            .map(|fact| fact.effective.clone())
            .unwrap_or_else(|| {
                def.effects
                    .map(|effects| program.arena.effects(effects).collect::<Vec<_>>())
            });
        CompactFunctionSig {
            params,
            parameter_schemas,
            return_ty,
            return_type_expr: def.return_ty,
            inferred_effects: self
                .output
                .function_effect_facts
                .get(&super::EffectDeclarationId {
                    namespace,
                    body: body_span,
                })
                .is_some_and(|fact| fact.inferred),
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
                ty: self
                    .output
                    .parameter_types
                    .get(&program.arena.span(param.span))
                    .cloned()
                    .unwrap_or_else(|| Type::from_arena(&program.arena, param.ty)),
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
            self.error(span, builtin_message, DiagnosticCode::CheckDuplicateName);
        }
        self.check_standard_module_shadow(&name.as_str(), span);
        if !self.names.insert(name) {
            self.error(
                span,
                "duplicate top-level name",
                DiagnosticCode::CheckDuplicateName,
            );
        }
    }

    fn check_standard_module_shadow(&mut self, name: &str, span: crate::source::Span) {
        if name == "args" {
            return;
        }
        if name != "error" && api_spec().is_standard_module(name) {
            let message = format!("name `{name}` shadows the standard module `{name}`");
            self.error(span, &message, DiagnosticCode::CheckStandardModuleShadow);
        }
    }

    fn error(&mut self, span: crate::source::Span, message: &str, code: DiagnosticCode) {
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
