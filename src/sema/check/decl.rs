#![allow(clippy::single_call_fn)]

use super::{
    BTreeMap, Checker, CoreCommand, ErrorFamilyInfo, ErrorVariantInfo, FxHashSet, Name,
    QualifiedName, Span, TagVariantInfo, Type, TypeAnnRef, TypeDefBody, UserModuleSig, api_spec,
};
use crate::sema::check::{
    Binding, ContractParam, FunctionParamSig, FunctionSig, ModuleContractEntry,
    ModuleContractEntryKind, SchemaField, TagVariant, standard_record_type,
};
use crate::syntax::arena::{
    ArenaBindingTargetKind, ArenaModuleContractEntryKind, ArenaProgram, ArenaRange, ArenaStmtKind,
    ArenaTypeDef, ArenaTypeDefBody, ArenaUserModule, ErrorDefId, FunctionDefId, StmtId, TypeExprId,
};
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[allow(dead_code)]
impl Checker {
    pub(super) fn collect_user_modules_arena(
        &mut self,
        program: &ArenaProgram,
        type_program: Arc<ArenaProgram>,
        source: &str,
    ) {
        for module in &program.modules {
            let sig = self.check_user_module_arena(program, type_program.clone(), source, module);
            self.user_modules.insert(module.key.clone(), sig);
        }
    }

    pub(super) fn collect_type_imports_arena(
        &mut self,
        program: &ArenaProgram,
        statements: impl IntoIterator<Item = StmtId>,
    ) {
        for stmt_id in statements {
            let stmt = program.arena.stmt(stmt_id);
            let ArenaStmtKind::Use(use_id) = stmt.kind else {
                continue;
            };
            let use_stmt = program.arena.use_stmt(use_id);
            if let Some(key) = use_stmt.resolved.as_deref() {
                let namespace = use_stmt
                    .alias
                    .or_else(|| program.arena.names(use_stmt.path).last());
                self.import_user_module_types(key, namespace, stmt.span, true);
            }
        }
    }

    pub(super) fn collect_definitions_arena(
        &mut self,
        program: &ArenaProgram,
        type_program: Arc<ArenaProgram>,
        source: &str,
        statements: impl IntoIterator<Item = StmtId>,
    ) {
        let stmt_ids: Vec<StmtId> = statements.into_iter().collect();
        let mut names = FxHashSet::default();
        for stmt_id in &stmt_ids {
            let (kind, span) = exported_stmt_kind_arena(program, *stmt_id);
            match kind {
                ArenaStmtKind::TypeDef(def_id) => {
                    let def = program.arena.type_def(def_id);
                    if is_builtin_or_standard_record_type_name(def.name.as_str()) {
                        self.error(
                            span,
                            "type name conflicts with a built-in type",
                            "check.duplicate-name",
                        );
                    }
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if self.type_defs.contains_key(&def.name) {
                        self.error(
                            span,
                            "type name conflicts with an imported type",
                            "check.duplicate-name",
                        );
                    }
                    if !names.insert(def.name) {
                        self.error(span, "duplicate top-level name", "check.duplicate-name");
                    }
                    let body = type_def_body_arena(type_program.clone(), def_id);
                    self.type_defs.insert(def.name, body.clone());
                    if let TypeDefBody::TagUnion(variants) = &body {
                        for variant in variants {
                            let field_types = variant
                                .fields
                                .iter()
                                .map(|field| self.type_from_ann(field))
                                .collect();
                            self.tag_variants.insert(
                                variant.name,
                                TagVariantInfo {
                                    type_name: def.name,
                                    field_count: variant.fields.len(),
                                    field_types,
                                },
                            );
                        }
                    }
                }
                ArenaStmtKind::ErrorDef(def_id) => {
                    let def = program.arena.error_def(def_id);
                    if is_builtin_or_standard_record_type_name(def.name.as_str()) {
                        self.error(
                            span,
                            "error family name conflicts with a built-in type",
                            "check.duplicate-name",
                        );
                    }
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(span, "duplicate top-level name", "check.duplicate-name");
                    }
                    self.register_error_family_arena(program, source, def_id);
                }
                ArenaStmtKind::ProcDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if CoreCommand::from_name(&def.name.as_str()).is_some() {
                        self.error(
                            span,
                            "proc name conflicts with a core command",
                            "check.core-command-shadow",
                        );
                    }
                    if !names.insert(def.name) {
                        self.error(span, "duplicate top-level name", "check.duplicate-name");
                    }
                }
                ArenaStmtKind::PureDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(span, "duplicate top-level name", "check.duplicate-name");
                    }
                }
                ArenaStmtKind::StreamDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(span, "duplicate top-level name", "check.duplicate-name");
                    }
                }
                _ => {}
            }
        }
        for stmt_id in stmt_ids {
            let (kind, _) = exported_stmt_kind_arena(program, stmt_id);
            match kind {
                ArenaStmtKind::ProcDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    let sig = self.function_sig_arena(program, source, def_id);
                    self.procs.insert(def.name, sig);
                }
                ArenaStmtKind::PureDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    let sig = self.function_sig_arena(program, source, def_id);
                    self.pures.insert(def.name, sig);
                }
                ArenaStmtKind::StreamDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    let sig = self.function_sig_arena(program, source, def_id);
                    self.streams.insert(def.name, sig);
                }
                ArenaStmtKind::ErrorDef(_) => {}
                _ => {}
            }
        }
    }

    pub(super) fn check_user_module_arena(
        &mut self,
        program: &ArenaProgram,
        type_program: Arc<ArenaProgram>,
        source: &str,
        module: &ArenaUserModule,
    ) -> UserModuleSig {
        let saved_procs = self.procs.clone();
        let saved_pures = self.pures.clone();
        let saved_streams = self.streams.clone();
        let saved_type_defs = self.type_defs.clone();
        let saved_tag_variants = self.tag_variants.clone();
        let saved_type_namespaces = self.type_namespaces.clone();
        let saved_error_families = self.error_families.clone();
        let saved_error_facets = self.error_facets.clone();
        let saved_return = self.current_return.clone();
        let saved_pure = self.in_pure;
        let saved_exported = self.current_exported;
        let saved_module_depth = self.module_depth;
        let saved_scopes = self.scopes.clone();
        self.scopes = vec![FxHashMap::default()];
        self.module_depth += 1;
        self.define_standard_values();

        let stmt_ids: Vec<StmtId> = program.module_statements(module).collect();
        self.check_public_docs(program, module.statements, &stmt_ids);
        self.collect_type_imports_arena(program, stmt_ids.iter().copied());
        let mut names = FxHashSet::default();
        for stmt_id in &stmt_ids {
            let (kind, span) = exported_stmt_kind_arena(program, *stmt_id);
            match kind {
                ArenaStmtKind::TypeDef(def_id) => {
                    let def = program.arena.type_def(def_id);
                    if is_builtin_or_standard_record_type_name(def.name.as_str()) {
                        self.error(
                            span,
                            "type name conflicts with a built-in type",
                            "check.duplicate-name",
                        );
                    }
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if self.type_defs.contains_key(&def.name) || !names.insert(def.name) {
                        self.error(span, "duplicate module type name", "check.duplicate-name");
                    }
                    self.type_defs
                        .insert(def.name, type_def_body_arena(type_program.clone(), def_id));
                }
                ArenaStmtKind::ErrorDef(def_id) => {
                    let def = program.arena.error_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(span, "duplicate module type name", "check.duplicate-name");
                    }
                    self.register_error_family_arena(program, source, def_id);
                }
                ArenaStmtKind::ProcDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    let sig = self.function_sig_arena(program, source, def_id);
                    self.procs.insert(def.name, sig);
                }
                ArenaStmtKind::PureDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    let sig = self.function_sig_arena(program, source, def_id);
                    self.pures.insert(def.name, sig);
                }
                ArenaStmtKind::StreamDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    let sig = self.function_sig_arena(program, source, def_id);
                    self.streams.insert(def.name, sig);
                }
                _ => {}
            }
        }

        let mut exports = UserModuleSig::default();
        for stmt_id in &stmt_ids {
            let stmt = program.arena.stmt(*stmt_id);
            if let ArenaStmtKind::SignalHook(hook_id) = stmt.kind {
                let hook = program.arena.signal_hook(hook_id).clone();
                self.check_signal_hook_arena(program, source, &hook, stmt.span);
                continue;
            }
            if !module_top_level_allowed_arena(program, *stmt_id) {
                self.error(
                    stmt.span,
                    "imported modules cannot run top-level mutation or commands",
                    "check.module-top-level",
                );
                continue;
            }
            match &stmt.kind {
                ArenaStmtKind::Export(inner_id) => {
                    let previous_exported = self.current_exported;
                    self.current_exported = true;
                    let inner = program.arena.stmt(*inner_id);
                    match inner.kind {
                        ArenaStmtKind::Let {
                            target,
                            ty,
                            initializer,
                        } => {
                            if matches!(
                                program.arena.binding_target(target).kind,
                                ArenaBindingTargetKind::Record { .. }
                            ) {
                                self.error(
                                    inner.span,
                                    "destructured exports are not supported",
                                    "check.export-destructure",
                                );
                            }
                            self.check_binding_arena(
                                program,
                                source,
                                target,
                                ty,
                                initializer,
                                false,
                                inner.span,
                            );
                            if let Some(name) = binding_target_simple_name_arena(program, target)
                                && let Some(binding) = self.lookup(name)
                            {
                                exports.values.insert(name, binding.ty.clone());
                            }
                        }
                        ArenaStmtKind::ProcDef(def_id) => {
                            let def = program.arena.function_def(def_id).clone();
                            self.check_function_arena(program, source, &def, false);
                            exports
                                .procs
                                .insert(def.name, self.function_sig_arena(program, source, def_id));
                        }
                        ArenaStmtKind::PureDef(def_id) => {
                            let def = program.arena.function_def(def_id).clone();
                            self.check_function_arena(program, source, &def, true);
                            exports
                                .pures
                                .insert(def.name, self.function_sig_arena(program, source, def_id));
                        }
                        ArenaStmtKind::StreamDef(def_id) => {
                            let def = program.arena.function_def(def_id).clone();
                            self.check_stream_function_arena(program, source, &def);
                            exports
                                .streams
                                .insert(def.name, self.function_sig_arena(program, source, def_id));
                        }
                        ArenaStmtKind::TypeDef(def_id) => {
                            let def = program.arena.type_def(def_id);
                            self.check_type_def_arena(program, source, def, inner.span);
                            exports.types.insert(
                                def.name,
                                type_def_body_arena(type_program.clone(), def_id),
                            );
                            exports
                                .resolved_types
                                .insert(def.name, self.type_from_name(def.name, inner.span));
                            if let ArenaTypeDefBody::TagUnion(variants) = def.body {
                                for variant in program.arena.tag_variants(variants) {
                                    if let Some(info) =
                                        self.tag_variants.get(&variant.name).cloned()
                                    {
                                        exports.tag_variants.insert(variant.name, info);
                                    }
                                }
                            }
                        }
                        ArenaStmtKind::ErrorDef(def_id) => {
                            self.check_error_def_arena(program, source, def_id);
                            let def = program.arena.error_def(def_id);
                            if let Some(family) = self.error_families.get(&def.name).cloned() {
                                exports.error_families.insert(def.name, family);
                            }
                        }
                        ArenaStmtKind::SignalHook(hook_id) => {
                            let hook = program.arena.signal_hook(hook_id).clone();
                            self.check_signal_hook_arena(program, source, &hook, inner.span);
                        }
                        _ => {}
                    }
                    self.current_exported = previous_exported;
                }
                ArenaStmtKind::Use(use_id) => {
                    let use_stmt = program.arena.use_stmt(*use_id);
                    self.check_use_arena(
                        program,
                        use_stmt.path,
                        use_stmt.alias,
                        use_stmt.resolved.as_deref(),
                        stmt.span,
                    );
                }
                ArenaStmtKind::Let {
                    target,
                    ty,
                    initializer,
                } => self.check_binding_arena(
                    program,
                    source,
                    *target,
                    *ty,
                    *initializer,
                    false,
                    stmt.span,
                ),
                ArenaStmtKind::TypeDef(def_id) => {
                    let def = program.arena.type_def(*def_id);
                    self.check_type_def_arena(program, source, def, stmt.span);
                }
                ArenaStmtKind::ErrorDef(def_id) => {
                    self.check_error_def_arena(program, source, *def_id);
                }
                ArenaStmtKind::ProcDef(def_id) => {
                    let def = program.arena.function_def(*def_id).clone();
                    self.check_function_arena(program, source, &def, false);
                }
                ArenaStmtKind::PureDef(def_id) => {
                    let def = program.arena.function_def(*def_id).clone();
                    self.check_function_arena(program, source, &def, true);
                }
                ArenaStmtKind::StreamDef(def_id) => {
                    let def = program.arena.function_def(*def_id).clone();
                    self.check_stream_function_arena(program, source, &def);
                }
                _ => {}
            }
        }

        self.procs = saved_procs;
        self.pures = saved_pures;
        self.streams = saved_streams;
        self.type_defs = saved_type_defs;
        self.tag_variants = saved_tag_variants;
        self.type_namespaces = saved_type_namespaces;
        self.error_families = saved_error_families;
        self.error_facets = saved_error_facets;
        self.current_return = saved_return;
        self.in_pure = saved_pure;
        self.current_exported = saved_exported;
        self.module_depth = saved_module_depth;
        self.scopes = saved_scopes;
        exports
    }

    pub(super) fn function_sig_arena(
        &mut self,
        program: &ArenaProgram,
        _source: &str,
        def_id: FunctionDefId,
    ) -> FunctionSig {
        let def = program.arena.function_def(def_id);
        FunctionSig {
            params: program
                .arena
                .params(def.params)
                .iter()
                .map(|param| FunctionParamSig {
                    name: param.name,
                    ty: self.type_from_arena(program, param.ty),
                    defaulted: param.default.is_some(),
                    rest: param.rest,
                })
                .collect(),
            return_ty: self.type_from_arena(program, def.return_ty),
            effects: def
                .effects
                .map(|effects| program.arena.effects(effects).collect()),
        }
    }
}

fn callable_type_from_function_signature(sig: &FunctionSig) -> super::CallableType {
    super::CallableType {
        params: sig
            .params
            .iter()
            .map(|param| super::CallableParamType {
                name: param.name,
                ty: param.ty.clone(),
                defaulted: param.defaulted,
                rest: param.rest,
            })
            .collect(),
        return_ty: Box::new(sig.return_ty.clone()),
        effects: sig.effects.clone(),
    }
}

fn module_type_from_user_signature(module: &UserModuleSig) -> Type {
    let mut exports = std::collections::BTreeMap::new();
    for (name, ty) in &module.values {
        exports.insert(
            *name,
            super::ModuleExportType::Value {
                ty: ty.clone(),
                optional: false,
            },
        );
    }
    for (name, sig) in &module.procs {
        exports.insert(
            *name,
            super::ModuleExportType::Proc {
                sig: callable_type_from_function_signature(sig),
                optional: false,
            },
        );
    }
    for (name, sig) in &module.pures {
        exports.insert(
            *name,
            super::ModuleExportType::Pure {
                sig: callable_type_from_function_signature(sig),
                optional: false,
            },
        );
    }
    Type::Module(exports)
}

#[allow(dead_code)]
impl Checker {
    pub(super) fn import_user_module(
        &mut self,
        key: &str,
        alias: Option<Name>,
        path: &[Name],
        span: Span,
    ) {
        let Some(module) = self.user_modules.get(key).cloned() else {
            self.error(span, "unknown user module", "check.unknown-module");
            return;
        };
        let Some(namespace) = alias.or_else(|| path.last().copied()) else {
            self.error(span, "empty module path", "check.unknown-module");
            return;
        };
        self.import_user_module_types(key, Some(namespace), span, false);
        self.define(
            namespace,
            Binding::new(module_type_from_user_signature(&module), false),
            span,
        );
        for (name, sig) in &module.procs {
            self.qualified_procs
                .insert(QualifiedName::new(namespace, *name), sig.clone());
        }
        for (name, sig) in &module.pures {
            self.qualified_pures
                .insert(QualifiedName::new(namespace, *name), sig.clone());
        }
        for (name, sig) in &module.streams {
            self.qualified_streams
                .insert(QualifiedName::new(namespace, *name), sig.clone());
        }
    }

    pub(super) fn import_user_module_types(
        &mut self,
        key: &str,
        alias: Option<Name>,
        span: Span,
        diagnose: bool,
    ) {
        let Some(module) = self.user_modules.get(key).cloned() else {
            if diagnose {
                self.error(span, "unknown user module", "check.unknown-module");
            }
            return;
        };
        if let Some(alias) = alias {
            if diagnose {
                self.check_standard_module_shadow(&alias.as_str(), span);
                if self.type_namespaces.contains_key(&alias) {
                    self.error(
                        span,
                        "duplicate imported type namespace",
                        "check.duplicate-name",
                    );
                }
            }
            self.type_namespaces
                .insert(alias, module.resolved_types.clone());
            for (name, info) in module.tag_variants {
                self.tag_variants
                    .insert(Name::intern(format!("{alias}.{name}")), info);
            }
            for (name, family) in module.error_families {
                let qualified = Name::intern(format!("{alias}.{name}"));
                for variant in family.variants.values() {
                    for facet in &variant.facets {
                        self.error_facets
                            .insert(Name::intern(format!("{alias}.{facet}")));
                    }
                }
                self.error_families.insert(qualified, family);
            }
            return;
        }
        let resolved_types = module.resolved_types.clone();
        for (name, body) in module.types {
            let body = match body {
                TypeDefBody::TagUnion(_) => body,
                _ => resolved_types
                    .get(&name)
                    .cloned()
                    .map(TypeDefBody::Resolved)
                    .unwrap_or(body),
            };
            if diagnose {
                if is_builtin_or_standard_record_type_name(name.as_str())
                    || self.type_defs.contains_key(&name)
                {
                    self.error(span, "duplicate imported type", "check.duplicate-name");
                }
                if let TypeDefBody::TagUnion(variants) = &body {
                    for variant in variants {
                        let field_types = variant
                            .fields
                            .iter()
                            .map(|field| self.type_from_ann(field))
                            .collect();
                        self.tag_variants.insert(
                            variant.name,
                            TagVariantInfo {
                                type_name: name,
                                field_count: variant.fields.len(),
                                field_types,
                            },
                        );
                    }
                }
                self.type_defs.insert(name, body);
            } else {
                if let TypeDefBody::TagUnion(variants) = &body {
                    for variant in variants {
                        if !self.tag_variants.contains_key(&variant.name) {
                            let field_types = variant
                                .fields
                                .iter()
                                .map(|field| self.type_from_ann(field))
                                .collect();
                            self.tag_variants.insert(
                                variant.name,
                                TagVariantInfo {
                                    type_name: name,
                                    field_count: variant.fields.len(),
                                    field_types,
                                },
                            );
                        }
                    }
                }
                self.type_defs.entry(name).or_insert(body);
            }
        }
        for (name, family) in module.error_families {
            if diagnose && self.error_families.contains_key(&name) {
                self.error(
                    span,
                    "duplicate imported error family",
                    "check.duplicate-name",
                );
            }
            for variant in family.variants.values() {
                for facet in &variant.facets {
                    self.error_facets.insert(*facet);
                }
            }
            self.error_families.entry(name).or_insert(family);
        }
    }
}

fn exported_stmt_kind_arena(program: &ArenaProgram, stmt_id: StmtId) -> (ArenaStmtKind, Span) {
    let stmt = program.arena.stmt(stmt_id);
    match stmt.kind {
        ArenaStmtKind::Export(inner) => {
            let inner = program.arena.stmt(inner);
            (inner.kind, inner.span)
        }
        kind => (kind, stmt.span),
    }
}

fn module_top_level_allowed_arena(program: &ArenaProgram, stmt_id: StmtId) -> bool {
    matches!(
        &program.arena.stmt(stmt_id).kind,
        ArenaStmtKind::Use(_)
            | ArenaStmtKind::Let { .. }
            | ArenaStmtKind::ProcDef(_)
            | ArenaStmtKind::PureDef(_)
            | ArenaStmtKind::StreamDef(_)
            | ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_)
            | ArenaStmtKind::Export(_)
    )
}

fn binding_target_simple_name_arena(
    program: &ArenaProgram,
    target: crate::syntax::arena::BindingTargetId,
) -> Option<Name> {
    match &program.arena.binding_target(target).kind {
        ArenaBindingTargetKind::Name(name) => Some(*name),
        ArenaBindingTargetKind::Record { .. } => None,
    }
}

fn type_def_body_arena(
    program: Arc<ArenaProgram>,
    id: crate::syntax::arena::TypeDefId,
) -> TypeDefBody {
    let def = program.arena.type_def(id);
    match def.body {
        ArenaTypeDefBody::Alias(ty) => TypeDefBody::Alias(TypeAnnRef::new(program, ty)),
        ArenaTypeDefBody::RecordSchema(fields) => TypeDefBody::RecordSchema(
            program
                .arena
                .schema_fields(fields)
                .iter()
                .map(|field| SchemaField {
                    name: field.name,
                    ty: TypeAnnRef::new(program.clone(), field.ty),
                })
                .collect(),
        ),
        ArenaTypeDefBody::ModuleContract(entries) => TypeDefBody::ModuleContract(
            program
                .arena
                .module_contract_entries(entries)
                .iter()
                .map(|entry| ModuleContractEntry {
                    name: entry.name,
                    optional: entry.optional,
                    kind: match &entry.kind {
                        ArenaModuleContractEntryKind::Value(ty) => {
                            ModuleContractEntryKind::Value(TypeAnnRef::new(program.clone(), *ty))
                        }
                        ArenaModuleContractEntryKind::Proc {
                            params,
                            effects,
                            return_ty,
                        } => ModuleContractEntryKind::Proc {
                            params: params_arena(program.clone(), *params),
                            effects: effects
                                .map(|effects| program.arena.effects(effects).collect()),
                            return_ty: TypeAnnRef::new(program.clone(), *return_ty),
                        },
                        ArenaModuleContractEntryKind::Pure { params, return_ty } => {
                            ModuleContractEntryKind::Pure {
                                params: params_arena(program.clone(), *params),
                                return_ty: TypeAnnRef::new(program.clone(), *return_ty),
                            }
                        }
                    },
                })
                .collect(),
        ),
        ArenaTypeDefBody::TagUnion(variants) => TypeDefBody::TagUnion(
            program
                .arena
                .tag_variants(variants)
                .iter()
                .map(|variant| TagVariant {
                    name: variant.name,
                    fields: program
                        .arena
                        .extra_range(variant.fields)
                        .iter()
                        .map(|raw| {
                            TypeAnnRef::new(program.clone(), TypeExprId::from_index(*raw as usize))
                        })
                        .collect(),
                })
                .collect(),
        ),
    }
}

fn params_arena(program: Arc<ArenaProgram>, range: ArenaRange) -> Vec<ContractParam> {
    program
        .arena
        .params(range)
        .iter()
        .map(|param| ContractParam {
            name: param.name,
            ty: TypeAnnRef::new(program.clone(), param.ty),
            defaulted: param.default.is_some() || param.ty_defaulted,
            rest: param.rest,
        })
        .collect()
}

#[allow(dead_code)]
impl Checker {
    pub(super) fn check_use_arena(
        &mut self,
        arena: &ArenaProgram,
        path: ArenaRange,
        alias: Option<Name>,
        resolved: Option<&str>,
        span: Span,
    ) {
        let path_names: Vec<Name> = arena.arena.names(path).collect();
        if let Some(key) = resolved {
            if alias.is_none()
                && let Some(last) = path_names.last()
                && last.as_str().contains('-')
            {
                let suggested = last.as_str().replace('-', "_");
                let message = format!(
                    "module path segment `{last}` contains a hyphen; \
                             use `as {suggested}` to give it a valid binding name"
                );
                self.error(span, &message, "check.hyphenated-module-alias");
                return;
            }
            self.import_user_module(key, alias, &path_names, span);
            return;
        }
        if path_names.len() != 1 || api_spec().module(&path_names[0].as_str()).is_none() {
            self.error(
                span,
                "only standard modules can be imported",
                "check.unknown-module",
            );
            return;
        }
        if let Some(alias) = alias {
            let message = format!(
                "standard module `{}` cannot be aliased as `{alias}`",
                path_names[0]
            );
            self.error(span, &message, "check.standard-module-alias");
        }
    }

    pub(super) fn check_type_def_arena(
        &mut self,
        arena: &ArenaProgram,
        _source: &str,
        def: &ArenaTypeDef,
        span: Span,
    ) {
        match &def.body {
            ArenaTypeDefBody::Alias(ty) => {
                self.type_from_arena(arena, *ty);
            }
            ArenaTypeDefBody::RecordSchema(fields) => {
                let field_list = arena.arena.schema_fields(*fields);
                let mut names = FxHashSet::default();
                for field in field_list {
                    if !names.insert(field.name) {
                        let field_span = arena.arena.span(field.span);
                        self.error(
                            field_span,
                            "duplicate schema field",
                            "check.duplicate-record-field",
                        );
                    }
                    self.type_from_arena(arena, field.ty);
                }
                if field_list.is_empty() {
                    self.error(
                        span,
                        "record schema needs at least one field",
                        "check.schema",
                    );
                }
            }
            ArenaTypeDefBody::ModuleContract(entries) => {
                let entry_list = arena.arena.module_contract_entries(*entries);
                let mut names = FxHashSet::default();
                for entry in entry_list {
                    if !names.insert(entry.name) {
                        let entry_span = arena.arena.span(entry.span);
                        self.error(
                            entry_span,
                            "duplicate module contract export",
                            "check.duplicate-name",
                        );
                    }
                    match &entry.kind {
                        ArenaModuleContractEntryKind::Value(ty) => {
                            self.type_from_arena(arena, *ty);
                        }
                        ArenaModuleContractEntryKind::Proc {
                            params, return_ty, ..
                        }
                        | ArenaModuleContractEntryKind::Pure { params, return_ty } => {
                            for param in arena.arena.params(*params) {
                                self.type_from_arena(arena, param.ty);
                            }
                            self.type_from_arena(arena, *return_ty);
                        }
                    }
                }
                if entry_list.is_empty() {
                    self.error(
                        span,
                        "module contract needs at least one export",
                        "check.module-contract",
                    );
                }
            }
            ArenaTypeDefBody::TagUnion(variants) => {
                for variant in arena.arena.tag_variants(*variants) {
                    let field_types: Vec<Type> = arena
                        .arena
                        .extra_range(variant.fields)
                        .iter()
                        .map(|raw| {
                            let ty_id = TypeExprId::from_index(*raw as usize);
                            self.type_from_arena(arena, ty_id)
                        })
                        .collect();
                    self.tag_variants.insert(
                        variant.name,
                        TagVariantInfo {
                            type_name: def.name,
                            field_count: field_types.len(),
                            field_types,
                        },
                    );
                }
            }
        }
    }

    pub(super) fn check_error_def_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        id: ErrorDefId,
    ) {
        let def = arena.arena.error_def(id);
        let mut variants = FxHashSet::default();
        for variant in arena.arena.error_variants(def.variants) {
            if !variants.insert(variant.name) {
                let variant_span = arena.arena.span(variant.span);
                self.error(
                    variant_span,
                    "duplicate error variant",
                    "check.duplicate-name",
                );
            }
            let mut fields = FxHashSet::default();
            for field in arena.arena.error_fields(variant.fields) {
                if !fields.insert(field.name) {
                    let field_span = arena.arena.span(field.span);
                    self.error(
                        field_span,
                        "duplicate error payload field",
                        "check.duplicate-record-field",
                    );
                }
                self.type_from_arena(arena, field.ty);
            }
        }
        self.register_error_family_arena(arena, source, id);
    }

    pub(super) fn register_error_family_arena(
        &mut self,
        arena: &ArenaProgram,
        _source: &str,
        id: ErrorDefId,
    ) {
        let def = arena.arena.error_def(id);
        let mut variants = BTreeMap::new();
        for variant in arena.arena.error_variants(def.variants) {
            let fields = arena
                .arena
                .error_fields(variant.fields)
                .iter()
                .map(|field| (field.name, self.type_from_arena(arena, field.ty)))
                .collect();
            let facets: Vec<Name> = arena.arena.names(variant.facets).collect();
            for facet in &facets {
                self.error_facets.insert(*facet);
            }
            variants.insert(variant.name, ErrorVariantInfo { fields, facets });
        }
        self.error_families
            .insert(def.name, ErrorFamilyInfo { variants });
    }
}

pub(super) fn is_builtin_or_standard_record_type_name(name: impl AsRef<str>) -> bool {
    let name = name.as_ref();
    Type::from_name(name) != Type::Unknown || standard_record_type(name).is_some()
}
