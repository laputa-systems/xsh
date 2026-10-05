#![allow(clippy::single_call_fn)]

use super::{
    BTreeMap, Checker, CoreCommand, ErrorFamilyInfo, ErrorVariantInfo, FxHashSet, Name,
    QualifiedName, Span, TagVariantInfo, Type, TypeAnnRef, TypeDefBody, UserModuleSig, api_spec,
};
use crate::diagnostic::DiagnosticCode;
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
            if module.internal {
                if self.options.embedded_bodies || program.module_statements(module).any(|id| {
                    let kind = match program.arena.stmt(id).kind {
                        ArenaStmtKind::Export(inner) => program.arena.stmt(inner).kind,
                        other => other,
                    };
                    matches!(kind, ArenaStmtKind::PureDef(id) if program.arena.function_def(id).return_ty_defaulted)
                }) {
                    self.check_user_module_arena(program, type_program.clone(), source, module);
                }
                // Embedded implementations are not user modules: they have no
                // `use` path, no module contract, and no public documentation
                // obligations. Their bodies are checked with programs that lower
                // them and validated against the registry by `script_impls`.
                continue;
            }
            let sig = self.check_user_module_arena(program, type_program.clone(), source, module);
            Arc::make_mut(&mut self.user_modules).insert(module.key.clone(), sig);
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

    fn check_test_declaration_names_arena(
        &mut self,
        program: &ArenaProgram,
        statements: &[StmtId],
        module: bool,
    ) {
        let mut tests = FxHashSet::default();
        for &statement in statements {
            let (kind, span) = exported_stmt_kind_arena(program, statement);
            if let ArenaStmtKind::ProcDef(id) = kind {
                let def = program.arena.function_def(id);
                if def.test_declaration && !tests.insert(def.name) && module {
                    self.error(
                        span,
                        "duplicate module test name",
                        DiagnosticCode::CheckDuplicateName,
                    );
                }
            }
        }
        for &statement in statements {
            let (kind, span) = exported_stmt_kind_arena(program, statement);
            let mut names = Vec::new();
            match kind {
                ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Var { target, .. } => {
                    let mut targets = vec![target];
                    while let Some(target) = targets.pop() {
                        match program.arena.binding_target(target).kind {
                            ArenaBindingTargetKind::Name(name) => names.push(name),
                            ArenaBindingTargetKind::Record { fields, .. } => targets.extend(
                                program
                                    .arena
                                    .destructure_fields(fields)
                                    .iter()
                                    .map(|field| field.target),
                            ),
                        }
                    }
                }
                ArenaStmtKind::Use(id) => {
                    let declaration = program.arena.use_stmt(id);
                    names.extend(
                        declaration
                            .alias
                            .or_else(|| program.arena.names(declaration.path).last()),
                    );
                }
                ArenaStmtKind::ProcDef(id)
                | ArenaStmtKind::PureDef(id)
                | ArenaStmtKind::StreamDef(id)
                    if module && !program.arena.function_def(id).test_declaration =>
                {
                    names.push(program.arena.function_def(id).name);
                }
                ArenaStmtKind::TypeDef(id) if module => names.push(program.arena.type_def(id).name),
                ArenaStmtKind::ErrorDef(id) if module => {
                    names.push(program.arena.error_def(id).name)
                }
                _ => {}
            }
            if names.iter().any(|name| tests.contains(name)) {
                self.error(
                    span,
                    "name conflicts with a test declaration",
                    DiagnosticCode::CheckDuplicateName,
                );
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
        self.check_test_declaration_names_arena(program, &stmt_ids, false);
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
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if self.type_defs.contains_key(&def.name) {
                        self.error(
                            span,
                            "type name conflicts with an imported type",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate top-level name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    self.check_enum_constructor_names(program, def, &mut names);
                    let body =
                        type_def_body_arena(type_program.clone(), def_id, self.current_namespace);
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
                                    type_name: variant.type_name,
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
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate top-level name",
                            DiagnosticCode::CheckDuplicateName,
                        );
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
                            DiagnosticCode::CheckCoreCommandShadow,
                        );
                    }
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate top-level name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                }
                ArenaStmtKind::PureDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate top-level name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                }
                ArenaStmtKind::StreamDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate top-level name",
                            DiagnosticCode::CheckDuplicateName,
                        );
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
                    if !def.test_declaration {
                        self.procs.insert(def.name, sig);
                    }
                }
                ArenaStmtKind::PureDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    let mut sig = self.function_sig_arena(program, source, def_id);
                    if def.return_ty_defaulted {
                        sig.return_ty = Type::Unknown;
                    }
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

    fn check_enum_constructor_names(
        &mut self,
        program: &ArenaProgram,
        def: &ArenaTypeDef,
        names: &mut FxHashSet<Name>,
    ) {
        let ArenaTypeDefBody::TagUnion(variants) = def.body else {
            return;
        };
        for variant in program.arena.tag_variants(variants) {
            let span = program.arena.span(variant.span);
            self.check_standard_module_shadow(&variant.name.as_str(), span);
            if !names.insert(variant.name)
                || (self.current_namespace.is_none()
                    && self.tag_variants.contains_key(&variant.name))
            {
                self.error(
                    span,
                    "duplicate enum constructor name",
                    DiagnosticCode::CheckDuplicateName,
                );
            }
            if is_builtin_or_standard_record_type_name(variant.name.as_str()) {
                self.error(
                    span,
                    "enum constructor conflicts with a built-in type",
                    DiagnosticCode::CheckDuplicateName,
                );
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
        self.prepare_local_inference(program);
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
        let saved_namespace = self.current_namespace;
        self.current_namespace = Some(module.name);
        let saved_scopes = self.scopes.clone();
        self.scopes = vec![FxHashMap::default()];
        self.module_depth += 1;
        self.define_standard_values();

        let stmt_ids: Vec<StmtId> = program.module_statements(module).collect();
        self.check_test_declaration_names_arena(program, &stmt_ids, true);
        if !module.internal {
            self.check_public_docs(program, module.statements, &stmt_ids);
            self.check_public_result_types(program, &stmt_ids);
        }
        for statement in &stmt_ids {
            if matches!(
                program.arena.stmt(*statement).kind,
                ArenaStmtKind::CliMain(_)
            ) {
                self.error(
                    program.arena.stmt(*statement).span,
                    "`cli main` is only permitted in the entry module",
                    DiagnosticCode::CheckCliEntry,
                );
            }
        }
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
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if self.type_defs.contains_key(&def.name) || !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate module type name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    self.check_enum_constructor_names(program, def, &mut names);
                    self.type_defs.insert(
                        def.name,
                        type_def_body_arena(type_program.clone(), def_id, self.current_namespace),
                    );
                }
                ArenaStmtKind::ErrorDef(def_id) => {
                    let def = program.arena.error_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate module type name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    self.register_error_family_arena(program, source, def_id);
                }
                ArenaStmtKind::ProcDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate module name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    let sig = self.function_sig_arena(program, source, def_id);
                    if !def.test_declaration {
                        self.procs.insert(def.name, sig);
                    }
                }
                ArenaStmtKind::PureDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate module name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    let mut sig = self.function_sig_arena(program, source, def_id);
                    if def.return_ty_defaulted {
                        sig.return_ty = Type::Unknown;
                    }
                    self.pures.insert(def.name, sig);
                }
                ArenaStmtKind::StreamDef(def_id) => {
                    let def = program.arena.function_def(def_id);
                    self.check_standard_module_shadow(&def.name.as_str(), span);
                    if !names.insert(def.name) {
                        self.error(
                            span,
                            "duplicate module name",
                            DiagnosticCode::CheckDuplicateName,
                        );
                    }
                    let sig = self.function_sig_arena(program, source, def_id);
                    self.streams.insert(def.name, sig);
                }
                _ => {}
            }
        }

        // Inferred bodies need constructor signatures before any statement
        // bodies are checked, just as root declarations do.
        for &stmt_id in &stmt_ids {
            if let ArenaStmtKind::TypeDef(def_id) = exported_stmt_kind_arena(program, stmt_id).0 {
                let def = program.arena.type_def(def_id);
                if matches!(def.body, ArenaTypeDefBody::TagUnion(_)) {
                    self.check_type_def_arena(
                        program,
                        source,
                        def,
                        program.arena.stmt(stmt_id).span,
                    );
                }
            }
        }
        self.infer_default_parameter_types(program, source, &stmt_ids);
        self.infer_local_pure_returns(program, source, &stmt_ids);
        self.infer_default_parameter_types(program, source, &stmt_ids);
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
                    DiagnosticCode::CheckModuleTopLevel,
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
                        }
                        | ArenaStmtKind::Const {
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
                                    DiagnosticCode::CheckExportDestructure,
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
                                if let Some(alias) = &binding.callable_alias {
                                    if alias.pure {
                                        exports.pures.insert(name, alias.signature.clone());
                                    } else {
                                        exports.procs.insert(name, alias.signature.clone());
                                    }
                                } else {
                                    exports.values.insert(name, binding.ty.clone());
                                }
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
                                type_def_body_arena(
                                    type_program.clone(),
                                    def_id,
                                    self.current_namespace,
                                ),
                            );
                            if def.type_parameters.is_empty() {
                                exports
                                    .resolved_types
                                    .insert(def.name, self.type_from_name(def.name, inner.span));
                            }
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
                }
                | ArenaStmtKind::Const {
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
        self.current_namespace = saved_namespace;
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
        let effect_declaration = self.effect_declaration_id(program, def.body);
        FunctionSig {
            effect_declaration: Some(effect_declaration),
            inferred_effects: self.effect_graph.is_inferred(effect_declaration),
            explicit_return: !def.return_ty_defaulted,
            is_alias: false,
            definition: Some(program.arena.span(program.arena.block(def.body).span)),
            params: program
                .arena
                .params(def.params)
                .iter()
                .map(|param| FunctionParamSig {
                    name: param.name,
                    ty: self
                        .checked_parameter_type(program, param)
                        .unwrap_or_else(|| {
                            if param.ty_defaulted {
                                Type::Unknown
                            } else {
                                self.type_from_arena(program, param.ty)
                            }
                        }),
                    schema_expectation: self
                        .record_constructors
                        .annotation_expectation(&program.arena, param.ty, self.current_namespace)
                        .ok(),
                    defaulted: param.default.is_some(),
                    rest: param.rest,
                })
                .collect(),
            return_ty: self.type_from_arena(program, def.return_ty),
            return_schema: (!def.return_ty_defaulted)
                .then(|| {
                    self.record_constructors
                        .annotation_expectation(
                            &program.arena,
                            def.return_ty,
                            self.current_namespace,
                        )
                        .ok()
                })
                .flatten(),
            effects: self.effective_function_effects(program, def),
        }
    }
}

pub(super) fn callable_type_from_function_signature(sig: &FunctionSig) -> super::CallableType {
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

/// A module names its own error families bare, while its importer spells
/// them through the namespace (`mod.E`), as constructors and patterns do.
fn qualify_imported_error_types(mut module: UserModuleSig, namespace: Name) -> UserModuleSig {
    let families = module
        .error_families
        .keys()
        .copied()
        .collect::<FxHashSet<_>>();
    if families.is_empty() {
        return module;
    }
    for ty in module.values.values_mut() {
        qualify_error_type(ty, namespace, &families);
    }
    for sig in module
        .procs
        .values_mut()
        .chain(module.pures.values_mut())
        .chain(module.streams.values_mut())
    {
        for param in &mut sig.params {
            qualify_error_type(&mut param.ty, namespace, &families);
        }
        qualify_error_type(&mut sig.return_ty, namespace, &families);
    }
    module
}

fn qualify_error_type(ty: &mut Type, namespace: Name, families: &FxHashSet<Name>) {
    match ty {
        Type::ErrorFamily(family) | Type::ErrorVariant { family, .. }
            if families.contains(family) =>
        {
            *family = Name::intern(format!("{namespace}.{family}"));
        }
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) | Type::Set(inner) => {
            qualify_error_type(inner, namespace, families)
        }
        Type::Map(left, right) | Type::Result(left, right) => {
            qualify_error_type(left, namespace, families);
            qualify_error_type(right, namespace, families);
        }
        Type::Record(fields) => {
            for field in fields.values_mut() {
                qualify_error_type(field, namespace, families);
            }
        }
        Type::Validated(validated) => {
            validated.rewrite_base(|base| qualify_error_type(base, namespace, families))
        }
        _ => {}
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
    // The checker has every export of a statically imported module, so its
    // type is the module's whole surface.
    Type::Module(std::sync::Arc::new(crate::sema::types::ModuleType::exact(
        exports,
    )))
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
            self.error(
                span,
                "unknown user module",
                DiagnosticCode::CheckUnknownModule,
            );
            return;
        };
        let Some(namespace) = alias.or_else(|| path.last().copied()) else {
            self.error(
                span,
                "empty module path",
                DiagnosticCode::CheckUnknownModule,
            );
            return;
        };
        let module = qualify_imported_error_types(module, namespace);
        self.import_user_module_types(key, Some(namespace), span, false);
        let mut binding = Binding::new(module_type_from_user_signature(&module), false);
        binding.static_namespace = true;
        self.define(namespace, binding, span);
        for (name, sig) in &module.procs {
            Arc::make_mut(&mut self.qualified_procs)
                .insert(QualifiedName::new(namespace, *name), sig.clone());
        }
        for (name, sig) in &module.pures {
            Arc::make_mut(&mut self.qualified_pures)
                .insert(QualifiedName::new(namespace, *name), sig.clone());
        }
        for (name, sig) in &module.streams {
            Arc::make_mut(&mut self.qualified_streams)
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
                self.error(
                    span,
                    "unknown user module",
                    DiagnosticCode::CheckUnknownModule,
                );
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
                        DiagnosticCode::CheckDuplicateName,
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
                    self.error(
                        span,
                        "duplicate imported type",
                        DiagnosticCode::CheckDuplicateName,
                    );
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
                                type_name: variant.type_name,
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
                                    type_name: variant.type_name,
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
                    DiagnosticCode::CheckDuplicateName,
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
            | ArenaStmtKind::Const { .. }
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
    namespace: Option<Name>,
) -> TypeDefBody {
    let def = program.arena.type_def(id);
    if !def.type_parameters.is_empty() {
        return TypeDefBody::Parameterized(def.type_parameters.len());
    }
    if matches!(
        def.body,
        ArenaTypeDefBody::Alias(_) | ArenaTypeDefBody::RecordSchema(_)
    ) {
        return TypeDefBody::Declared(program, id);
    }
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
        ArenaTypeDefBody::ModuleContract { entries, exact } => TypeDefBody::ModuleContract {
            exact,
            entries: program
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
        },
        ArenaTypeDefBody::TagUnion(variants) => TypeDefBody::TagUnion(
            program
                .arena
                .tag_variants(variants)
                .iter()
                .map(|variant| TagVariant {
                    type_name: namespace.map_or_else(
                        || crate::sema::wire_enums::declaring_enum_name(&program, id),
                        |namespace| {
                            crate::sema::wire_enums::nominal_enum_name(
                                Some(namespace),
                                program.arena.type_def(id).name,
                            )
                        },
                    ),
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
            source: param.clone(),
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
                self.error(span, &message, DiagnosticCode::CheckHyphenatedModuleAlias);
                return;
            }
            self.import_user_module(key, alias, &path_names, span);
            return;
        }
        if path_names.len() != 1 || api_spec().module(&path_names[0].as_str()).is_none() {
            self.error(
                span,
                "only standard modules can be imported",
                DiagnosticCode::CheckUnknownModule,
            );
            return;
        }
        if let Some(alias) = alias {
            let message = format!(
                "standard module `{}` cannot be aliased as `{alias}`",
                path_names[0]
            );
            self.error(span, &message, DiagnosticCode::CheckStandardModuleAlias);
        }
    }

    pub(super) fn check_type_def_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        def: &ArenaTypeDef,
        span: Span,
    ) {
        // A nominal identity names one record type. Resolution rejects the
        // same declaration where the type is used; a declaration nothing
        // uses is reported here.
        if def.nominal
            && (!matches!(def.body, ArenaTypeDefBody::RecordSchema(_))
                || !def.type_parameters.is_empty())
        {
            self.error(
                span,
                &format!(
                    "`nominal type {}` must be a record schema without type parameters",
                    def.name
                ),
                DiagnosticCode::CheckSchema,
            );
            return;
        }
        // A range bounds one integer type, so its declaration has no
        // parameter to apply.
        if def.bounds.is_some() && !def.type_parameters.is_empty() {
            self.error(
                span,
                &format!(
                    "`type {}` has a range, so it takes no type parameters",
                    def.name
                ),
                DiagnosticCode::CheckSchema,
            );
            return;
        }
        if !def.type_parameters.is_empty() {
            let mut names = FxHashSet::default();
            for parameter in arena.arena.names(def.type_parameters) {
                if !names.insert(parameter) {
                    self.error(
                        span,
                        "duplicate type parameter",
                        DiagnosticCode::CheckTypeParameters,
                    );
                }
                if Type::builtin_from_name(&parameter.as_str()).is_some()
                    || standard_record_type(&parameter.as_str()).is_some()
                    || matches!(
                        parameter.as_str().as_str(),
                        "List"
                            | "Map"
                            | "Stream"
                            | "Result"
                            | "Module"
                            | "Optional"
                            | "Unknown"
                            | "Union"
                            | "NonEmpty"
                            | "Set"
                    )
                {
                    self.error(
                        span,
                        "type parameter name is reserved",
                        DiagnosticCode::CheckTypeParameters,
                    );
                }
            }
            let namespace = self.current_namespace;
            match self
                .record_constructors
                .template_field_types(&arena.arena, def, namespace)
            {
                Ok(fields) => {
                    if let ArenaTypeDefBody::RecordSchema(schema_fields) = def.body {
                        let mut field_names = FxHashSet::default();
                        for field in arena.arena.schema_fields(schema_fields) {
                            if !field_names.insert(field.name) {
                                self.error(
                                    arena.arena.span(field.span),
                                    "duplicate schema field",
                                    DiagnosticCode::CheckDuplicateRecordField,
                                );
                            }
                            if let Some(default) = field.default {
                                let allowed = self
                                    .record_constructors
                                    .definition(namespace, def.name)
                                    .and_then(|id| self.record_constructors.defaults(id))
                                    .is_some_and(|defaults| defaults.contains_key(&field.name));
                                if !allowed {
                                    self.error(arena.arena.expr(default).span, "record default must be a literal or a previously declared immutable literal constant", DiagnosticCode::CheckRecordDefault);
                                }
                                if let Some(expected) = fields.get(&field.name) {
                                    let actual = self.check_expr_arena(
                                        arena,
                                        source,
                                        default,
                                        Some(expected),
                                    );
                                    self.expect_type(
                                        expected,
                                        &actual,
                                        arena.arena.expr(default).span,
                                    );
                                }
                            }
                        }
                        if field_names.is_empty() {
                            self.error(
                                span,
                                "record schema needs at least one field",
                                DiagnosticCode::CheckSchema,
                            );
                        }
                    }
                }
                Err(error) => self.error(span, &error.message, error.code),
            }
            return;
        }
        match &def.body {
            ArenaTypeDefBody::Alias(ty) => {
                let base = self.type_from_arena(arena, *ty);
                // Resolution rejects the same bounds where the type is used;
                // a declaration nothing uses is reported here.
                if let Err(error) =
                    crate::sema::constants::bounded_alias_type(&arena.arena, def, base)
                {
                    self.error(span, &error.message, error.code);
                }
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
                            DiagnosticCode::CheckDuplicateRecordField,
                        );
                    }
                    let expected = self.type_from_arena(arena, field.ty);
                    if let Some(default) = field.default {
                        let definition = self
                            .record_constructors
                            .definition(self.current_namespace, def.name);
                        let allowed = definition
                            .and_then(|id| self.record_constructors.defaults(id))
                            .is_some_and(|defaults| defaults.contains_key(&field.name));
                        if !allowed {
                            self.error(arena.arena.expr(default).span,
                                "record default must be a literal or a previously declared immutable literal constant",
                                DiagnosticCode::CheckRecordDefault);
                        }
                        let actual = self.check_expr_arena(arena, source, default, Some(&expected));
                        self.expect_type(&expected, &actual, arena.arena.expr(default).span);
                    }
                }
                if field_list.is_empty() {
                    self.error(
                        span,
                        "record schema needs at least one field",
                        DiagnosticCode::CheckSchema,
                    );
                }
            }
            ArenaTypeDefBody::ModuleContract { entries, .. } => {
                let entry_list = arena.arena.module_contract_entries(*entries);
                let mut names = FxHashSet::default();
                for entry in entry_list {
                    if !names.insert(entry.name) {
                        let entry_span = arena.arena.span(entry.span);
                        self.error(
                            entry_span,
                            "duplicate module contract export",
                            DiagnosticCode::CheckDuplicateName,
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
                                self.infer_checked_parameter(arena, source, param);
                            }
                            self.type_from_arena(arena, *return_ty);
                        }
                    }
                }
                if entry_list.is_empty() {
                    self.error(
                        span,
                        "module contract needs at least one export",
                        DiagnosticCode::CheckModuleContract,
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
                            type_name: crate::sema::wire_enums::nominal_enum_name(
                                self.current_namespace.or(arena.root_nominal_namespace),
                                def.name,
                            ),
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
                    DiagnosticCode::CheckDuplicateName,
                );
            }
            let mut fields = FxHashSet::default();
            for field in arena.arena.error_fields(variant.fields) {
                if !fields.insert(field.name) {
                    let field_span = arena.arena.span(field.span);
                    self.error(
                        field_span,
                        "duplicate error payload field",
                        DiagnosticCode::CheckDuplicateRecordField,
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
                .collect::<Vec<_>>();
            let facets: Vec<Name> = arena.arena.names(variant.facets).collect();
            for facet in &facets {
                self.error_facets.insert(*facet);
            }
            variants.insert(variant.name, ErrorVariantInfo::declared(fields, facets));
        }
        self.error_families
            .insert(def.name, ErrorFamilyInfo { variants });
    }
}

pub(super) fn is_builtin_or_standard_record_type_name(name: impl AsRef<str>) -> bool {
    let name = name.as_ref();
    Type::from_name(name) != Type::Unknown || standard_record_type(name).is_some()
}
