#![allow(clippy::single_call_fn)]

use super::decl::is_builtin_or_standard_record_type_name;
use super::{
    BTreeMap, BinaryOp, Checker, CoreCommand, Diagnostic, ErrorFamilyInfo, ErrorVariantInfo,
    FxHashMap, FxHashSet, Label, Name, TagVariantInfo, Type, UnaryOp, api_spec,
};
use crate::sema::types::{CallableParamType, CallableType, ModuleExportType};
use crate::symbol::QualifiedName;
use crate::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBlock, ArenaBuilderEntryKind, ArenaCommand,
    ArenaCommandArgKind, ArenaErrorDef, ArenaExprKind, ArenaExprOrRun,
    ArenaModuleContractEntryKind, ArenaPipeStageKind, ArenaProgram, ArenaRecordFieldKind,
    ArenaStmtKind, ArenaTypeDef, ArenaTypeDefBody, BlockId, ErrorDefId, ExprId, FunctionDefId,
    StmtId, TypeDefId, TypeExprId,
};
use crate::syntax::node::{Effect, EnvGetKind};

#[derive(Clone, Debug, Default)]
pub struct CompactDeclOutput {
    pub diagnostics: Vec<Diagnostic>,
    pub types: FxHashMap<Name, CompactTypeDefInfo>,
    pub tag_variants_by_name: FxHashMap<Name, TagVariantInfo>,
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
    pub return_ty: Type,
    pub effects: Option<Vec<Effect>>,
}

#[derive(Clone, Debug, Default)]
pub struct CompactBodyProbeOutput {
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
}

impl Checker {
    pub fn check_compact_declarations(program: &ArenaProgram) -> CompactDeclOutput {
        let mut collector = CompactDeclCollector {
            diagnostics: Vec::new(),
            names: FxHashSet::default(),
            output: CompactDeclOutput::default(),
        };
        collector.collect_program(program);
        let mut output = collector.output;
        output.diagnostics = collector.diagnostics;
        output
    }

    pub fn probe_compact_bodies(
        program: &ArenaProgram,
        declarations: &CompactDeclOutput,
    ) -> CompactBodyProbeOutput {
        let mut probe = CompactBodyProbe {
            program,
            declarations,
            output: CompactBodyProbeOutput::default(),
            scopes: vec![FxHashMap::default()],
            stream_items: Vec::new(),
        };
        probe.seed_declarations();
        probe.check_program();
        probe.output
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
            for stmt in program.module_statements(module) {
                self.collect_decl_stmt(program, stmt, Some(module.name));
            }
        }
    }

    fn collect_decl_stmt(&mut self, program: &ArenaProgram, id: StmtId, namespace: Option<Name>) {
        let stmt = program.arena.stmt(id);
        let span = stmt.span;
        match stmt.kind {
            ArenaStmtKind::Export(inner) => self.collect_decl_stmt(program, inner, namespace),
            ArenaStmtKind::TypeDef(def) => self.collect_type_def(program, def, span),
            ArenaStmtKind::ErrorDef(def) => self.collect_error_def(program, def, span, namespace),
            ArenaStmtKind::ProcDef(def) => {
                self.collect_function_def(program, def, CompactFunctionKind::Proc, span, namespace);
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
    ) {
        let def = program.arena.type_def(id);
        self.output.type_defs += 1;
        self.check_top_level_name(def.name, span, "type name conflicts with a built-in type");
        let info = self.collect_type_def_body(program, def);
        self.output.types.insert(def.name, info);
    }

    fn collect_type_def_body(
        &mut self,
        program: &ArenaProgram,
        def: &ArenaTypeDef,
    ) -> CompactTypeDefInfo {
        match def.body {
            ArenaTypeDefBody::Alias(ty) => CompactTypeDefInfo::Alias(ty),
            ArenaTypeDefBody::RecordSchema(fields) => {
                let mut names = FxHashSet::default();
                let fields = program.arena.schema_fields(fields);
                self.output.schema_fields += fields.len();
                let mut record = BTreeMap::new();
                for field in fields {
                    if !names.insert(field.name) {
                        self.error(
                            program.arena.span(field.span),
                            "duplicate schema field",
                            "check.duplicate-record-field",
                        );
                    }
                    record.insert(field.name, Type::from_arena(&program.arena, field.ty));
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
                    self.output.tag_variants_by_name.insert(
                        variant.name,
                        TagVariantInfo {
                            type_name: def.name,
                            field_count: field_types.len(),
                            field_types,
                        },
                    );
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
        self.check_top_level_name(
            def.name,
            span,
            "error family name conflicts with a built-in type",
        );
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
        self.output.function_defs += 1;
        self.check_standard_module_shadow(def.name.as_str(), span);
        if kind == CompactFunctionKind::Proc && CoreCommand::from_name(&def.name).is_some() {
            self.error(
                span,
                "proc name conflicts with a core command",
                "check.core-command-shadow",
            );
        }
        if !self.names.insert(def.name) {
            self.error(span, "duplicate top-level name", "check.duplicate-name");
        }
        let sig = self.function_sig(program, id);
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

    fn function_sig(&mut self, program: &ArenaProgram, id: FunctionDefId) -> CompactFunctionSig {
        let def = program.arena.function_def(id);
        let params = self.param_sigs(program, def.params);
        let return_ty = Type::from_arena(&program.arena, def.return_ty);
        let effects = def
            .effects
            .map(|effects| program.arena.effects(effects).collect::<Vec<_>>());
        CompactFunctionSig {
            params,
            return_ty,
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
                ty: Type::from_arena(&program.arena, param.ty),
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
        self.check_standard_module_shadow(name.as_str(), span);
        if !self.names.insert(name) {
            self.error(span, "duplicate top-level name", "check.duplicate-name");
        }
    }

    fn check_standard_module_shadow(&mut self, name: &str, span: crate::source::Span) {
        if name == "args" {
            return;
        }
        if api_spec().is_standard_module(name) {
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

struct CompactBodyProbe<'a> {
    program: &'a ArenaProgram,
    declarations: &'a CompactDeclOutput,
    output: CompactBodyProbeOutput,
    scopes: Vec<FxHashMap<Name, CompactBinding>>,
    stream_items: Vec<Type>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompactBinding {
    ty: Type,
    mutable: bool,
}

impl CompactBinding {
    fn new(ty: Type, mutable: bool) -> Self {
        Self { ty, mutable }
    }
}

impl CompactBodyProbe<'_> {
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

    fn check_program(&mut self) {
        for stmt in self.program.statement_ids() {
            self.check_stmt(stmt);
        }
        for module in &self.program.modules {
            for stmt in self.program.module_statements(module) {
                self.check_stmt(stmt);
            }
        }
    }

    fn check_stmt(&mut self, id: StmtId) {
        self.output.statements += 1;
        let stmt = self.program.arena.stmt(id);
        match stmt.kind {
            ArenaStmtKind::Use(_)
            | ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_)
            | ArenaStmtKind::Continue
            | ArenaStmtKind::TailBareIdent(_) => {
                self.output.supported_statements += 1;
            }
            ArenaStmtKind::Export(inner) => {
                self.output.supported_statements += 1;
                self.check_stmt(inner);
            }
            ArenaStmtKind::Let {
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
                let expected = ty.map(|ty| Type::from_arena(&self.program.arena, ty));
                let actual = self.check_expr_or_run(initializer);
                let mutable = matches!(self.program.arena.stmt(id).kind, ArenaStmtKind::Var { .. });
                self.define_binding_target(target, expected.unwrap_or(actual), mutable);
            }
            ArenaStmtKind::Assign { target, value, .. } => {
                self.output.supported_statements += 1;
                self.check_assign_target(target, stmt.span);
                self.check_expr_or_run(value);
            }
            ArenaStmtKind::ProcDef(def)
            | ArenaStmtKind::PureDef(def)
            | ArenaStmtKind::StreamDef(def) => {
                self.output.supported_statements += 1;
                self.check_function(def);
            }
            ArenaStmtKind::Return(value) => {
                self.output.supported_statements += 1;
                if let Some(value) = value {
                    self.check_expr_or_run(value);
                }
            }
            ArenaStmtKind::Yield(value) | ArenaStmtKind::Defer(value) => {
                self.output.supported_statements += 1;
                self.check_expr_or_run(value);
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                self.output.supported_statements += 1;
                for branch in self.program.arena.if_branches(branches) {
                    self.check_expr(branch.condition);
                    self.check_block(branch.block);
                }
                if let Some(block) = else_block {
                    self.check_block(block);
                }
            }
            ArenaStmtKind::While { condition, block } => {
                self.output.supported_statements += 1;
                self.check_expr(condition);
                self.check_block(block);
            }
            ArenaStmtKind::For {
                target,
                iter,
                block,
            } => {
                self.output.supported_statements += 1;
                let iter_ty = self.check_expr(iter);
                self.push_scope();
                self.define_binding_target(target, collection_item_type(&iter_ty), false);
                self.check_block_in_current_scope(block);
                self.pop_scope();
            }
            ArenaStmtKind::Loop { block } => {
                self.output.supported_statements += 1;
                self.check_block(block);
            }
            ArenaStmtKind::Break { value } => {
                self.output.supported_statements += 1;
                if let Some(value) = value {
                    self.check_expr(value);
                }
            }
            ArenaStmtKind::Match { value, arms } => {
                self.output.supported_statements += 1;
                self.check_expr(value);
                for arm in self.program.arena.match_arms(arms) {
                    if let Some(guard) = arm.guard {
                        self.check_expr(guard);
                    }
                    self.check_block(arm.block);
                }
            }
            ArenaStmtKind::Command(command) => {
                self.output.supported_statements += 1;
                self.output.commands += 1;
                self.check_command(command);
            }
            ArenaStmtKind::Expr(expr) => {
                self.output.supported_statements += 1;
                self.check_expr(expr);
            }
            ArenaStmtKind::SignalHook(hook) => {
                self.output.supported_statements += 1;
                let hook = self.program.arena.signal_hook(hook);
                self.check_block(hook.body);
            }
            ArenaStmtKind::With {
                bindings,
                body,
                else_param,
                else_block,
            } => {
                self.output.supported_statements += 1;
                self.push_scope();
                for binding in self.program.arena.with_bindings(bindings) {
                    let ty = self.check_expr(binding.initializer);
                    self.current_scope_mut()
                        .insert(binding.name, CompactBinding::new(ty, false));
                }
                self.check_block_in_current_scope(body);
                self.pop_scope();
                self.push_scope();
                if let Some(param) = else_param {
                    self.current_scope_mut()
                        .insert(param, CompactBinding::new(Type::Error, false));
                }
                self.check_block_in_current_scope(else_block);
                self.pop_scope();
            }
            ArenaStmtKind::Guard {
                target,
                ty,
                initializer,
                else_param,
                else_block,
            } => {
                self.output.supported_statements += 1;
                self.output.bindings += 1;
                let expected = ty.map(|ty| Type::from_arena(&self.program.arena, ty));
                let actual = self.check_expr_or_run(initializer);
                self.define_binding_target(target, expected.unwrap_or(actual), false);
                self.push_scope();
                if let Some(param) = else_param {
                    self.current_scope_mut()
                        .insert(param, CompactBinding::new(Type::Error, false));
                }
                self.check_block_in_current_scope(else_block);
                self.pop_scope();
            }
            ArenaStmtKind::GuardedStmt {
                stmt, condition, ..
            } => {
                self.output.supported_statements += 1;
                self.check_expr(condition);
                self.push_scope();
                self.check_stmt(stmt);
                self.pop_scope();
            }
        }
    }

    fn check_function(&mut self, id: FunctionDefId) {
        self.output.functions += 1;
        let def = self.program.arena.function_def(id);
        self.push_scope();
        for param in self.program.arena.params(def.params) {
            let ty = Type::from_arena(&self.program.arena, param.ty);
            self.current_scope_mut()
                .insert(param.name, CompactBinding::new(ty, false));
            if let Some(default) = param.default {
                self.check_expr(default);
            }
        }
        self.check_block_in_current_scope(def.body);
        self.pop_scope();
    }

    fn check_block(&mut self, id: BlockId) {
        self.push_scope();
        self.check_block_in_current_scope(id);
        self.pop_scope();
    }

    fn check_block_in_current_scope(&mut self, id: BlockId) {
        self.output.blocks += 1;
        let ArenaBlock {
            params, statements, ..
        } = self.program.arena.block(id);
        for param in self.program.arena.block_params(*params) {
            self.current_scope_mut()
                .insert(param.name, CompactBinding::new(Type::Any, false));
        }
        for stmt in self.program.arena.stmt_ids(*statements) {
            self.check_stmt(stmt);
        }
    }

    fn check_expr_or_run(&mut self, value: ArenaExprOrRun) -> Type {
        match value {
            ArenaExprOrRun::Expr(expr) => self.check_expr(expr),
            ArenaExprOrRun::Run(run) => {
                self.output.runs += 1;
                self.check_run(run);
                Type::Result(Box::new(Type::Status), Box::new(Type::ProcessError))
            }
        }
    }

    fn check_expr(&mut self, id: ExprId) -> Type {
        self.output.expressions += 1;
        let ty = match self.program.arena.expr(id).kind {
            ArenaExprKind::Null => Type::Null,
            ArenaExprKind::Bool(_) => Type::Bool,
            ArenaExprKind::Int(_) => Type::Int,
            ArenaExprKind::Float(_) => Type::Float,
            ArenaExprKind::Duration(_) => Type::Duration,
            ArenaExprKind::Str(_) => Type::Str,
            ArenaExprKind::FmtString(parts) => {
                self.check_fmt_parts(parts);
                Type::Str
            }
            ArenaExprKind::PathStr(_) => Type::Path,
            ArenaExprKind::PathFmtString(parts) => {
                self.check_fmt_parts(parts);
                Type::Path
            }
            ArenaExprKind::GlobStr(_) => Type::List(Box::new(Type::Path)),
            ArenaExprKind::Bytes(_) => Type::Bytes,
            ArenaExprKind::Ident(name) => self.lookup_name(name),
            ArenaExprKind::LastStatus => Type::Status,
            ArenaExprKind::List(items) => self.check_list(items),
            ArenaExprKind::Record(fields) => self.check_record(fields),
            ArenaExprKind::Unary { op, expr } => self.check_unary(op, expr),
            ArenaExprKind::Binary { op, left, right } => self.check_binary(op, left, right),
            ArenaExprKind::Call { callee, args } => self.check_call(callee, args),
            ArenaExprKind::Field { base, name } => self.check_field(base, name),
            ArenaExprKind::NullSafeField { base, name } => {
                Type::Optional(Box::new(self.check_field(base, name)))
            }
            ArenaExprKind::Index { base, index } => {
                let base = self.check_expr(base);
                self.check_expr(index);
                index_type(&base)
            }
            ArenaExprKind::Slice { base, start, end } => {
                let ty = self.check_expr(base);
                if let Some(start) = start {
                    self.check_expr(start);
                }
                if let Some(end) = end {
                    self.check_expr(end);
                }
                match ty {
                    Type::List(_) | Type::Str | Type::Path | Type::Bytes => ty,
                    _ => Type::Unknown,
                }
            }
            ArenaExprKind::EnvGet { kind, .. } => match kind {
                EnvGetKind::Str => Type::Str,
                EnvGetKind::Path => Type::Path,
                EnvGetKind::PathList => Type::EnvPathList,
            },
            ArenaExprKind::EnvPathList => Type::EnvPathList,
            ArenaExprKind::Try(expr) => self
                .check_expr(expr)
                .result_ok()
                .cloned()
                .unwrap_or(Type::Unknown),
            ArenaExprKind::Require { value, schema } => {
                self.check_expr(value);
                Type::Result(
                    Box::new(Type::from_arena(&self.program.arena, schema)),
                    Box::new(Type::Error),
                )
            }
            ArenaExprKind::Run(run) => {
                self.output.runs += 1;
                self.check_run(run);
                Type::Result(Box::new(Type::Status), Box::new(Type::ProcessError))
            }
            ArenaExprKind::Spawn(form) => {
                match form.target {
                    crate::syntax::arena::ArenaSpawnTarget::Run(run) => {
                        self.output.runs += 1;
                        self.check_run(run);
                    }
                    crate::syntax::arena::ArenaSpawnTarget::Command(command) => {
                        self.check_expr(command);
                    }
                }
                Type::ProcessHandle
            }
            ArenaExprKind::Wait(form) => {
                self.check_expr(form.target);
                Type::Result(Box::new(Type::Status), Box::new(Type::ProcessError))
            }
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                let mut ty = None;
                for branch in self.program.arena.if_expr_branches(branches) {
                    self.check_expr(branch.condition);
                    ty = Some(merge_types(ty, self.check_expr(branch.value)));
                }
                merge_types(ty, self.check_expr(else_value))
            }
            ArenaExprKind::Loop { block } => {
                self.check_block(block);
                Type::Unknown
            }
            ArenaExprKind::Retry { delays, block } => {
                for delay in self.program.arena.expr_ids(delays) {
                    self.check_expr(delay);
                }
                self.check_block(block);
                Type::Result(Box::new(Type::Unknown), Box::new(Type::Error))
            }
            ArenaExprKind::ListComp {
                expr,
                target,
                iter,
                condition,
            } => {
                let iter_ty = self.check_expr(iter);
                self.push_scope();
                self.define_binding_target(target, collection_item_type(&iter_ty), false);
                if let Some(condition) = condition {
                    self.check_expr(condition);
                }
                let item = self.check_expr(expr);
                self.pop_scope();
                Type::List(Box::new(item))
            }
            ArenaExprKind::MapComp {
                key,
                value,
                target,
                iter,
                condition,
            } => {
                let iter_ty = self.check_expr(iter);
                self.push_scope();
                self.define_binding_target(target, collection_item_type(&iter_ty), false);
                self.check_expr(key);
                if let Some(condition) = condition {
                    self.check_expr(condition);
                }
                let item = self.check_expr(value);
                self.pop_scope();
                Type::Map(Box::new(item))
            }
            ArenaExprKind::Match { value, arms } => {
                self.check_expr(value);
                let mut ty = None;
                for arm in self.program.arena.match_expr_arms(arms) {
                    if let Some(guard) = arm.guard {
                        self.check_expr(guard);
                    }
                    ty = Some(merge_types(ty, self.check_expr(arm.value)));
                }
                ty.unwrap_or(Type::Unknown)
            }
            ArenaExprKind::Pipeline { input, stages } => {
                self.check_expr(input);
                self.check_pipe_stages(stages);
                Type::Unknown
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                self.check_expr(input);
                self.check_stream_stages(stages);
                Type::Unknown
            }
            ArenaExprKind::BuilderCall { call, block } => {
                self.check_expr(call);
                self.check_builder_block(block);
                Type::Unknown
            }
            ArenaExprKind::Item => self.stream_items.last().cloned().unwrap_or(Type::Any),
        };
        self.output.typed_expressions += 1;
        self.output.expr_types.insert(id, ty.clone());
        ty
    }

    fn check_list(&mut self, range: crate::syntax::arena::ArenaRange) -> Type {
        let mut item_ty = None;
        for item in self.program.arena.expr_ids(range) {
            item_ty = Some(merge_types(item_ty, self.check_expr(item)));
        }
        Type::List(Box::new(item_ty.unwrap_or(Type::Unknown)))
    }

    fn check_record(&mut self, range: crate::syntax::arena::ArenaRange) -> Type {
        let mut fields = BTreeMap::new();
        for field in self.program.arena.record_fields(range) {
            match &field.kind {
                ArenaRecordFieldKind::Named { name, value, .. } => {
                    fields.insert(*name, self.check_expr(*value));
                }
                ArenaRecordFieldKind::Shorthand { name, .. } => {
                    fields.insert(*name, self.lookup_name(*name));
                }
                ArenaRecordFieldKind::Spread { expr, .. } => {
                    if let Type::Record(spread) = self.check_expr(*expr) {
                        fields.extend(spread);
                    }
                }
            }
        }
        Type::Record(fields)
    }

    fn check_fmt_parts(&mut self, range: crate::syntax::arena::ArenaRange) {
        for part in self.program.arena.fmt_parts(range) {
            if let crate::syntax::arena::ArenaFmtPart::Expr(expr, _) = part {
                self.check_expr(expr);
            }
        }
    }

    fn check_unary(&mut self, op: UnaryOp, expr: ExprId) -> Type {
        let ty = self.check_expr(expr);
        match op {
            UnaryOp::Not => Type::Bool,
            UnaryOp::Neg if matches!(ty, Type::Float) => Type::Float,
            UnaryOp::Neg if matches!(ty, Type::Int | Type::Duration) => ty,
            UnaryOp::Neg => Type::Unknown,
        }
    }

    fn check_binary(&mut self, op: BinaryOp, left: ExprId, right: ExprId) -> Type {
        let left = self.check_expr(left);
        let right = self.check_expr(right);
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
            BinaryOp::Add if matches!((&left, &right), (Type::Str, Type::Str)) => Type::Str,
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                numeric_result_type(left, right)
            }
            BinaryOp::ResultFallback => left.result_ok().cloned().unwrap_or(left),
        }
    }

    fn check_call(&mut self, callee: ExprId, args: crate::syntax::arena::ArenaRange) -> Type {
        let callee_expr = self.program.arena.expr(callee);
        let callee_ty = self.check_expr(callee);
        for arg in self.program.arena.call_args(args) {
            match &arg.kind {
                crate::syntax::arena::ArenaCallArgKind::Positional(value)
                | crate::syntax::arena::ArenaCallArgKind::Splice { value, .. }
                | crate::syntax::arena::ArenaCallArgKind::Named { value, .. } => {
                    self.check_expr(*value);
                }
            }
        }
        if let ArenaExprKind::Ident(name) = callee_expr.kind {
            if let Some(sig) = self.declarations.pures.get(&name) {
                return sig.return_ty.clone();
            }
            if let Some(sig) = self.declarations.procs.get(&name) {
                return sig.return_ty.clone();
            }
            if let Some(variant) = self.declarations.tag_variants_by_name.get(&name) {
                return Type::Tag(variant.type_name);
            }
        }
        if let Some(return_ty) = self.compact_module_call_type(callee_expr.kind, args) {
            return return_ty;
        }
        match callee_ty {
            Type::Pure | Type::Proc => Type::Unknown,
            _ => Type::Unknown,
        }
    }

    fn compact_module_call_type(
        &self,
        callee: ArenaExprKind,
        args: crate::syntax::arena::ArenaRange,
    ) -> Option<Type> {
        let (ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name }) =
            callee
        else {
            return None;
        };
        let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind else {
            return None;
        };
        let args = self.program.arena.call_args(args);
        for sig in api_spec().module_overloads(module.as_str(), name.as_str())? {
            let mut bindings = vec![false; sig.params.len()];
            let mut next_positional = 0usize;
            let mut matched = true;
            for arg in args {
                match arg.kind {
                    crate::syntax::arena::ArenaCallArgKind::Splice { .. } => {
                        matched = false;
                        break;
                    }
                    crate::syntax::arena::ArenaCallArgKind::Positional(_) => {
                        while next_positional < bindings.len() && bindings[next_positional] {
                            next_positional += 1;
                        }
                        let Some(binding) = bindings.get_mut(next_positional) else {
                            matched = false;
                            break;
                        };
                        *binding = true;
                    }
                    crate::syntax::arena::ArenaCallArgKind::Named { name, .. } => {
                        let Some(param_index) = sig
                            .params
                            .iter()
                            .position(|param| param.name == name.as_str())
                        else {
                            matched = false;
                            break;
                        };
                        if bindings[param_index] {
                            matched = false;
                            break;
                        }
                        bindings[param_index] = true;
                    }
                }
            }
            if matched
                && sig
                    .params
                    .iter()
                    .zip(&bindings)
                    .all(|(param, binding)| param.defaulted || *binding)
            {
                return Some(sig.return_ty.clone());
            }
        }
        None
    }

    fn check_field(&mut self, base: ExprId, name: Name) -> Type {
        match self.check_expr(base) {
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

    fn check_command(&mut self, id: crate::syntax::arena::CommandStmtId) {
        match &self.program.arena.command_stmt(id).command {
            ArenaCommand::Proc { args, .. } => {
                for arg in self.program.arena.command_args(*args) {
                    self.check_command_arg(arg);
                }
            }
            ArenaCommand::Core {
                args, env, block, ..
            } => {
                for assignment in self.program.arena.env_assignments(*env) {
                    match assignment.value {
                        crate::syntax::arena::ArenaEnvAssignmentValue::Expr(expr) => {
                            self.check_expr(expr);
                        }
                        crate::syntax::arena::ArenaEnvAssignmentValue::CommandArg(ref arg) => {
                            self.check_command_arg(arg);
                        }
                    }
                }
                for arg in self.program.arena.command_args(*args) {
                    self.check_command_arg(arg);
                }
                if let Some(block) = block {
                    self.check_block(*block);
                }
            }
            ArenaCommand::Run(run) => {
                self.output.runs += 1;
                self.check_run(*run);
            }
        }
    }

    fn check_run(&mut self, id: crate::syntax::arena::RunFormId) {
        for segment in self
            .program
            .arena
            .run_segments(self.program.arena.run_form(id).segments)
        {
            if let Some(timeout) = segment.timeout {
                self.check_expr(timeout);
            }
            if let Some(cpu_max) = segment.cpu_max {
                self.check_expr(cpu_max);
            }
            for assignment in self.program.arena.env_assignments(segment.env) {
                match assignment.value {
                    crate::syntax::arena::ArenaEnvAssignmentValue::Expr(expr) => {
                        self.check_expr(expr);
                    }
                    crate::syntax::arena::ArenaEnvAssignmentValue::CommandArg(ref arg) => {
                        self.check_command_arg(arg);
                    }
                }
            }
            self.check_command_arg(&segment.target);
            for arg in self.program.arena.command_args(segment.args) {
                self.check_command_arg(arg);
            }
            for redirection in self.program.arena.redirections(segment.redirections) {
                match &redirection.target {
                    crate::syntax::arena::ArenaRedirectionTarget::Path(arg)
                    | crate::syntax::arena::ArenaRedirectionTarget::Fd(arg) => {
                        self.check_command_arg(arg);
                    }
                }
            }
        }
    }

    fn check_command_arg(&mut self, arg: &crate::syntax::arena::ArenaCommandArg) {
        match &arg.kind {
            ArenaCommandArgKind::Typed(expr) | ArenaCommandArgKind::SpliceExpr(expr) => {
                self.check_expr(*expr);
            }
            ArenaCommandArgKind::Word(parts) => {
                for part in self.program.arena.word_parts(*parts) {
                    match part {
                        crate::syntax::arena::ArenaWordPart::Shorthand(expr)
                        | crate::syntax::arena::ArenaWordPart::Interpolation(expr) => {
                            self.check_expr(expr);
                        }
                        crate::syntax::arena::ArenaWordPart::Bare(_)
                        | crate::syntax::arena::ArenaWordPart::Quoted(_) => {}
                    }
                }
            }
            ArenaCommandArgKind::SpliceName(_) => {}
        }
    }

    fn check_pipe_stages(&mut self, stages: crate::syntax::arena::ArenaRange) {
        for stage in self.program.arena.pipe_stages(stages) {
            match &stage.kind {
                ArenaPipeStageKind::Expr(expr) => {
                    self.check_expr(*expr);
                }
                ArenaPipeStageKind::Stream(stream) => {
                    self.check_stream_stage(stream);
                }
            }
        }
    }

    fn check_stream_stages(&mut self, stages: crate::syntax::arena::ArenaRange) {
        for stage in self.program.arena.stream_stages(stages) {
            self.check_stream_stage(stage);
        }
    }

    fn check_stream_stage(&mut self, stream: &crate::syntax::arena::ArenaStreamStage) {
        for option in self.program.arena.stream_options(stream.options) {
            if let Some(value) = option.value {
                self.check_expr(value);
            }
        }
        for arg in self.program.arena.call_args(stream.args) {
            match &arg.kind {
                crate::syntax::arena::ArenaCallArgKind::Positional(expr)
                | crate::syntax::arena::ArenaCallArgKind::Splice { value: expr, .. }
                | crate::syntax::arena::ArenaCallArgKind::Named { value: expr, .. } => {
                    self.check_expr(*expr);
                }
            }
        }
        if let Some(block) = stream.block {
            self.stream_items.push(Type::Any);
            self.check_block(block);
            self.stream_items.pop();
        }
    }

    fn check_builder_block(&mut self, id: crate::syntax::arena::BuilderBlockId) {
        let block = self.program.arena.builder_block(id);
        for entry in self.program.arena.builder_entries(block.entries) {
            match entry.kind {
                ArenaBuilderEntryKind::Field { value, .. } => {
                    self.check_expr(value);
                }
                ArenaBuilderEntryKind::Entry { args, block, .. } => {
                    for arg in self.program.arena.command_args(args) {
                        self.check_command_arg(arg);
                    }
                    if let Some(block) = block {
                        self.check_builder_block(block);
                    }
                }
                ArenaBuilderEntryKind::Task { block, .. } => {
                    self.check_block(block);
                }
                ArenaBuilderEntryKind::Stmt(stmt) => {
                    self.check_stmt(stmt);
                }
            }
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
                    self.current_scope_mut()
                        .insert(field.name, CompactBinding::new(field_ty, mutable));
                }
            }
        }
    }

    fn check_assign_target(
        &mut self,
        id: crate::syntax::arena::AssignTargetId,
        span: crate::source::Span,
    ) {
        if let Some(name) = self.assign_target_root_name(id) {
            match self.lookup_binding(name) {
                Some(binding) if !binding.mutable => {
                    self.error(
                        span,
                        "assignment to immutable `let` binding",
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
                self.check_expr(*index);
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
            return binding.ty.clone();
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

fn collection_item_type(ty: &Type) -> Type {
    match ty {
        Type::List(item) | Type::Stream(item) | Type::Map(item) => item.as_ref().clone(),
        Type::Str => Type::Str,
        Type::Bytes => Type::Int,
        Type::Unknown | Type::Invalid | Type::Any => ty.clone(),
        _ => Type::Unknown,
    }
}

fn index_type(ty: &Type) -> Type {
    match ty {
        Type::List(item) | Type::Map(item) | Type::Stream(item) => item.as_ref().clone(),
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
    } else if matches!(left, Type::Int) && matches!(right, Type::Int) {
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
