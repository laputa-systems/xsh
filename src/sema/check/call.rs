#![allow(clippy::single_call_fn)]

use super::expr::is_path_like_arena_expr;
use super::{
    ApiArgCheck, BTreeMap, CallableParamType, Checker, Diagnostic, FxHashSet, Label,
    MethodReceiver, ModuleExportType, Name, QualifiedName, Span, Type, UnaryOp, api_spec,
    call_arg_expr_id_arena, call_arg_span_arena, standard_record_type,
};
use crate::syntax::arena::{
    ArenaCallArg, ArenaCallArgKind, ArenaExprKind, ArenaProgram, ArenaRange, ArenaRecordFieldKind,
    ExprId,
};
use crate::syntax::node::Effect;
use xsh_registry::types::BuiltinTypeName;

fn contract_type_is_valid(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return false;
    }
    if BuiltinTypeName::parse(text) == Some(BuiltinTypeName::Unknown) {
        return false;
    }
    if BuiltinTypeName::parse(text) == Some(BuiltinTypeName::Any) {
        return true;
    }
    if let Some((params, return_ty)) = contract_proc_signature(text) {
        return params.iter().all(|param| contract_type_is_valid(param))
            && contract_type_is_valid(return_ty);
    }
    for name in ["List", "Map", "Stream"] {
        if let Some(inner) = contract_generic_body(text, name) {
            return !inner.is_empty()
                && contract_split_types(inner).len() == 1
                && contract_type_is_valid(inner);
        }
    }
    if let Some(inner) = contract_generic_body(text, "Result") {
        let parts = contract_split_types(inner);
        return matches!(parts.len(), 1 | 2)
            && parts.iter().all(|part| contract_type_is_valid(part));
    }
    Type::builtin_from_name(text).is_some_and(|ty| !matches!(ty, Type::Unknown))
        || standard_record_type(text).is_some()
}

fn process_command_argv_item_type_is_valid(ty: &Type) -> bool {
    matches!(ty, Type::Str | Type::Path | Type::Any | Type::Unknown)
}

fn contract_proc_signature(text: &str) -> Option<(Vec<&str>, &str)> {
    let rest = text.strip_prefix("Proc(")?;
    let close = rest.find(") -> ")?;
    if rest[close + 5..].contains(") -> ") {
        return None;
    }
    let params = &rest[..close];
    let return_ty = &rest[close + 5..];
    let parsed_params = if params.trim().is_empty() {
        Vec::new()
    } else {
        contract_split_types(params)
    };
    Some((parsed_params, return_ty.trim()))
}

fn contract_generic_body<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.strip_prefix(name)?
        .strip_prefix('[')?
        .strip_suffix(']')
        .map(str::trim)
}

fn contract_split_types(text: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (index, ch) in text.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth < 0 {
                    return Vec::new();
                }
            }
            ',' if depth == 0 => {
                items.push(text[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Vec::new();
    }
    items.push(text[start..].trim());
    items
}

#[allow(dead_code)]
impl Checker {
    fn check_module_callable_effects(
        &mut self,
        caller_effs: &[Effect],
        callee_effects: &Option<Vec<Effect>>,
        callee_name: &str,
        span: Span,
    ) {
        self.record_effect_contract(callee_effects, callee_name);
        self.check_callee_effects(caller_effs, callee_effects, callee_name, span);
    }

    pub(super) fn expect_json_compatible(&mut self, ty: &Type, span: Span) {
        if !ty.is_json_compatible_with(&|name| self.wire_enums.mappings.contains_key(&name)) {
            self.error(
                span,
                "value is not JSON-compatible; convert Path, Bytes, Status, Result, and errors explicitly",
                "check.json-compatible",
            );
        }
    }
}

/// Arena-native mirror of `check_call` and its callees — fully native, no
/// raise-fallback branches remain (the `Path` constructor and generic
/// method-dispatch cases now go through `method.rs`'s arena-native
/// `check_registered_method_arena`/`check_method_dispatch_arena`).
#[allow(dead_code)]
impl Checker {
    pub(super) fn check_call_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        callee: ExprId,
        args_range: ArenaRange,
        span: Span,
        expected_context: Option<&Type>,
    ) -> Type {
        let callee_kind = arena.arena.expr(callee).kind;
        let args = arena.arena.call_args(args_range);
        if args.iter().any(|arg| matches!(arg.kind, ArenaCallArgKind::NamedSpread { .. })) {
            return self.check_spread_call_arena(arena, source, callee, args_range, span, expected_context);
        }
        if let Some(alias) = self.resolve_callable_alias_call(arena, callee) {
            self.record_callable_alias(arena.arena.expr(callee).span, &alias);
            if let ArenaExprKind::Field { base, name } = callee_kind
                && name == "call" && self.resolve_callable_alias_call(arena, base).is_some() {
                self.static_callable_aliases.get_mut(&arena.arena.expr(callee).span).unwrap().method_call = true;
            }
            if !alias.pure {
                if self.in_pure { self.error(span, "effectful proc is not allowed in pure functions", "check.pure-effect"); }
                else { self.check_resolved_callable_effects(&alias.signature, &alias.name.to_string(), span); }
            }
            self.check_function_arg_list_arena(arena, source, args, &alias.signature.params, span);
            if !alias.pure { self.invalidate_mutable_narrowings(); }
            self.record_callee_propagation(&alias.signature.effects, &alias.signature.return_ty, span);
            return alias.signature.return_ty;
        }

        if let Some(definition) = self.record_constructors.resolve_call(
            &arena.arena, callee, self.current_namespace,
        ) {
            return self.check_inferred_record_constructor_arena(arena, source, callee, definition, args, expected_context, span);
        }

        if let ArenaExprKind::Ident(name) = callee_kind {
            if name == "reveal_type" {
                return self.check_reveal_type_call_arena(arena, source, args, span);
            }
            if let Some(sig) = self.procs.get(&name).cloned() {
                if self.in_pure {
                    self.error(
                        span,
                        "effectful proc is not allowed in pure functions",
                        "check.pure-effect",
                    );
                } else {
                    self.check_resolved_callable_effects(&sig, &name.as_str(), span);
                }
                self.check_function_arg_list_arena(arena, source, args, &sig.params, span);
                // A procedure may change mutable lexical captures before the next statement.
                self.invalidate_mutable_narrowings();
                self.record_callee_propagation(&sig.effects, &sig.return_ty, span);
                return sig.return_ty;
            }
            if let Some(sig) = self.pures.get(&name).cloned() {
                self.check_function_arg_list_arena(arena, source, args, &sig.params, span);
                self.record_callee_propagation(&sig.effects, &sig.return_ty, span);
                return sig.return_ty;
            }
            if let Some(sig) = self.streams.get(&name).cloned() {
                if self.in_pure {
                    self.error(
                        span,
                        "stream producer is not allowed in pure functions",
                        "check.pure-effect",
                    );
                } else {
                    self.check_resolved_callable_effects(&sig, &name.as_str(), span);
                }
                self.check_function_arg_list_arena(arena, source, args, &sig.params, span);
                self.record_callee_propagation(&sig.effects, &sig.return_ty, span);
                return sig.return_ty;
            }
            return self.check_constructor_call_arena(arena, source, &name.as_str(), args, span, expected_context);
        }

        if let ArenaExprKind::Field { base, name } = callee_kind {
            let base_kind = arena.arena.expr(base).kind;
            if let ArenaExprKind::Field {
                base: family_base,
                name: family,
            } = base_kind
                && let ArenaExprKind::Ident(namespace) = arena.arena.expr(family_base).kind
            {
                let qualified_family = Name::intern(format!("{namespace}.{family}"));
                if self.error_families.contains_key(&qualified_family) {
                    return self.check_error_variant_constructor_arena(
                        arena,
                        source,
                        qualified_family,
                        name,
                        args,
                        span,
                    );
                }
            }
            if let ArenaExprKind::Ident(module) = base_kind {
                if module.as_str() == "error" && name.as_str() == "fail" {
                    let params = [super::FunctionParamSig { name: Name::intern("message"), ty: Type::Str, schema_expectation: None, defaulted: false, rest: false }];
                    self.check_function_arg_list_arena(arena, source, args, &params, span);
                    return Type::Result(Box::new(Type::Unit), Box::new(Type::Error));
                }
                if self.error_families.contains_key(&module) {
                    return self.check_error_variant_constructor_arena(
                        arena, source, module, name, args, span,
                    );
                }
                let qualified_tag = Name::intern(format!("{module}.{name}"));
                if self.tag_variants.contains_key(&qualified_tag) {
                    return self.check_constructor_call_arena(
                        arena,
                        source,
                        &qualified_tag.as_str(),
                        args,
                        span,
                        expected_context,
                    );
                }
                let qualified = QualifiedName::new(module, name);
                if let Some(sig) = self.qualified_pures.get(&qualified).cloned() {
                    self.check_function_arg_list_arena(arena, source, args, &sig.params, span);
                    self.record_callee_propagation(&sig.effects, &sig.return_ty, span);
                    return sig.return_ty;
                }
                if let Some(sig) = self.qualified_procs.get(&qualified).cloned() {
                    if self.in_pure {
                        self.error(
                            span,
                            "effectful proc is not allowed in pure functions",
                            "check.pure-effect",
                        );
                    } else {
                        self.check_resolved_callable_effects(&sig, &qualified.to_string(), span);
                    }
                    self.check_function_arg_list_arena(arena, source, args, &sig.params, span);
                    self.record_callee_propagation(&sig.effects, &sig.return_ty, span);
                    return sig.return_ty;
                }
                if let Some(sig) = self.qualified_streams.get(&qualified).cloned() {
                    if self.in_pure {
                        self.error(
                            span,
                            "stream producer is not allowed in pure functions",
                            "check.pure-effect",
                        );
                    } else {
                        self.check_resolved_callable_effects(&sig, &qualified.to_string(), span);
                    }
                    self.check_function_arg_list_arena(arena, source, args, &sig.params, span);
                    self.record_callee_propagation(&sig.effects, &sig.return_ty, span);
                    return sig.return_ty;
                }
                if module == "Path" {
                    return self.check_registered_method_arena(
                        arena,
                        source,
                        MethodReceiver::PathConstructor,
                        &name.as_str(),
                        args,
                        span,
                        &Type::Path,
                        "check.unknown-module-api",
                    );
                }
                // A local binding takes precedence over a standard module name.
                // Without this guard, `let path = ...; path.read_text()` is
                // checked as the module call `path.read_text`, producing a
                // misleading unknown-module-api diagnostic instead of checking
                // the valid Path method.
                if self.lookup(module).is_none() && api_spec().module(&module.as_str()).is_some() {
                    if let Some(required) = api_spec().module_required_effect(&module.as_str(), &name.as_str()) {
                        self.require_effect(required, span, &format!("`{module}.{name}`"));
                    }
                    return self.check_module_call_arena(
                        arena,
                        source,
                        &module.as_str(),
                        &name.as_str(),
                        args,
                        span,
                    );
                }
            }
            let base_ty = self.check_expr_arena(arena, source, base, None);
            let guarded_base = match arena.arena.expr(base).kind {
                ArenaExprKind::NullSafeField { .. }
                | ArenaExprKind::Index { guarded: true, .. }
                | ArenaExprKind::Slice { guarded: true, .. } => true,
                ArenaExprKind::Call { callee, .. } => matches!(arena.arena.expr(callee).kind, ArenaExprKind::NullSafeField { .. }),
                _ => false,
            };
            if guarded_base && matches!(base_ty, Type::Optional(_)) {
                self.error(span, "a nullable postfix result needs its own `?.` method hop", "check.optional-method");
                return Type::Unknown;
            }

            if let Type::Module(exports) = &base_ty
                && let Some(export) = exports.get(&name)
            {
                match export {
                    ModuleExportType::Proc { sig, .. } => {
                        self.record_effect_contract(&sig.effects, &name.as_str());
                        if self.in_pure {
                            self.error(
                                span,
                                "effectful proc is not allowed in pure functions",
                                "check.pure-effect",
                            );
                        } else if let Some(caller_effs) = self.current_effects.clone() {
                            self.check_module_callable_effects(
                                &caller_effs,
                                &sig.effects,
                                &name.as_str(),
                                span,
                            );
                        }
                        self.check_module_callable_arg_list_arena(
                            arena,
                            source,
                            args,
                            &sig.params,
                            span,
                        );
                        self.record_callee_propagation(&sig.effects, &sig.return_ty, span);
                        return sig.return_ty.as_ref().clone();
                    }
                    ModuleExportType::Pure { sig, .. } => {
                        self.check_module_callable_arg_list_arena(
                            arena,
                            source,
                            args,
                            &sig.params,
                            span,
                        );
                        self.record_callee_propagation(&sig.effects, &sig.return_ty, span);
                        return sig.return_ty.as_ref().clone();
                    }
                    ModuleExportType::Value { .. } => {}
                }
            }
            return self.check_method_dispatch_arena(
                arena,
                source,
                base_ty,
                &name.as_str(),
                args,
                span,
            );
        }

        if let ArenaExprKind::NullSafeField { base, name } = callee_kind {
            let base_ty = self.check_expr_arena(arena, source, base, None);
            let (inner_ty, wrap_optional) = match base_ty {
                Type::Optional(inner) if !matches!(*inner, Type::Any | Type::Unknown) => (*inner, true),
                Type::Result(_, _) => (self.check_propagation(&base_ty, span), false),
                _ => {
                    self.error(
                        span,
                        "`?.` requires an Optional or Result value",
                        "check.null-safe-field",
                    );
                    return Type::Unknown;
                }
            };
            if matches!(inner_ty, Type::Optional(_)) {
                self.error(span, "Result propagation leaves an Optional receiver; guard the next hop explicitly", "check.optional-method");
                return Type::Unknown;
            }
            let return_ty = self.check_method_dispatch_arena(
                arena,
                source,
                inner_ty,
                &name.as_str(),
                args,
                span,
            );
            return if wrap_optional && !matches!(return_ty, Type::Optional(_)) {
                Type::Optional(Box::new(return_ty))
            } else {
                return_ty
            };
        }

        self.record_effect_contract(&None, "unresolved call target");
        self.error(span, "unsupported call target", "check.call-target");
        Type::Unknown
    }

    fn check_reveal_type_call_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        span: Span,
    ) -> Type {
        if args.len() != 1 {
            self.error(span, "incorrect function arity", "check.arity");
        }
        for arg in args {
            if matches!(arg.kind, ArenaCallArgKind::Named { .. }) {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    "unexpected named parameter",
                    "check.named-arg",
                );
            }
        }
        let revealed = args
            .first()
            .map(|arg| self.check_call_arg_arena(arena, source, &arg.kind, None))
            .unwrap_or(Type::Unknown);
        for arg in args.iter().skip(1) {
            self.check_call_arg_arena(arena, source, &arg.kind, None);
        }
        if self.options.reveal_types {
            if args.len() == 1
                && let Some(arg) = args.first()
                && matches!(arg.kind, ArenaCallArgKind::Positional(_))
            {
                self.reveal_type(&revealed, call_arg_span_arena(arena, &arg.kind));
            }
        } else {
            self.error(
                span,
                "`reveal_type` is available only through `xsht check`",
                "check.reveal-type",
            );
        }
        Type::Unit
    }

    fn check_module_callable_arg_list_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        params: &[CallableParamType],
        span: Span,
    ) {
        let params = params.iter().map(|param| super::FunctionParamSig {
            name: param.name, ty: param.ty.clone(), schema_expectation: None, defaulted: param.defaulted, rest: param.rest,
        }).collect::<Vec<_>>();
        self.check_function_arg_list_arena(arena, source, args, &params, span);
    }

    fn check_spread_call_arena(
        &mut self, arena: &ArenaProgram, source: &str, callee: ExprId,
        args_range: ArenaRange, span: Span, expected_context: Option<&Type>,
    ) -> Type {
        use crate::sema::arguments::{ArgumentValueSource, expand_named_arguments};
        use crate::syntax::arena::ArenaCallArgInput;
        let args = arena.arena.call_args(args_range);
        // Discover finite field sets without applying argument flow changes
        // to the real checker. Actual checks run at each source entry below.
        let mut probe = self.clone();
        let receiver_type = match arena.arena.expr(callee).kind {
            ArenaExprKind::Field { base, .. } => Some(probe.check_expr_arena(arena, source, base, None)),
            _ => None,
        };
        let mut checked = super::FxHashMap::default();
        for arg in args {
            let value = call_arg_expr_id_arena(&arg.kind);
            let ty = probe.check_expr_arena(arena, source, value, None);
            checked.insert(value, ty);
        }
        let expanded = match expand_named_arguments(arena, args, |id| checked.get(&id).cloned()) {
            Ok(expanded) => expanded,
            Err(error) => { self.error(error.span, &error.message, "check.named-spread"); return Type::Invalid; }
        };
        let statically_named = self.resolve_callable_alias_call(arena, callee).is_some() || match arena.arena.expr(callee).kind {
            ArenaExprKind::Ident(name) => self.procs.contains_key(&name) || self.pures.contains_key(&name)
                || self.streams.contains_key(&name)
                || self.record_constructors.resolve_call(&arena.arena, callee, self.current_namespace).is_some(),
            ArenaExprKind::Field { base, name } => {
                let static_namespace = matches!(arena.arena.expr(base).kind, ArenaExprKind::Ident(namespace)
                    if api_spec().module(&namespace.as_str()).is_some() || self.error_families.contains_key(&namespace))
                    || self.record_constructors.resolve_call(&arena.arena, callee, self.current_namespace).is_some();
                let positional_tag = matches!(arena.arena.expr(base).kind, ArenaExprKind::Ident(namespace)
                    if self.tag_variants.contains_key(&Name::intern(format!("{namespace}.{name}"))));
                !positional_tag && (static_namespace || match receiver_type.clone().unwrap_or(Type::Unknown) {
                    Type::Module(exports) => exports.get(&name).is_some_and(|export| matches!(export, ModuleExportType::Pure { .. } | ModuleExportType::Proc { .. })),
                    Type::Any | Type::Unknown | Type::DynamicModule | Type::Pure | Type::Proc | Type::Optional(_) | Type::Result(_, _) => false,
                    _ => true,
                })
            },
            _ => false,
        };
        if !statically_named {
            self.error(span, "named argument spreading requires a statically checked callable signature", "check.named-spread");
            return Type::Invalid;
        }
        let mut temporary = arena.clone();
        let mut inputs = Vec::new();
        let mut projections = Vec::new();
        let mut supplied = super::FxHashSet::default();
        let mut checked_entries = super::FxHashSet::default();
        for arg in expanded {
            if let Some(name) = arg.name && !supplied.insert(name) {
                self.error(arg.span, &format!("parameter `{name}` supplied more than once"), "check.named-arg");
            }
            let value = match arg.value {
                ArgumentValueSource::Expression(value) | ArgumentValueSource::PositionalSplice(value) => value,
                ArgumentValueSource::RecordField { record, field } => {
                    let id = temporary.arena.append_argument_projection(record, field, arg.span);
                    self.argument_projection_types.insert(id, arg.ty);
                    if checked_entries.insert(arg.entry_index) { self.argument_projection_sources.insert(id, record); }
                    projections.push(id); id
                }
            };
            inputs.push(if let Some(name) = arg.name {
                ArenaCallArgInput::Named { name, value, span: arg.span }
            } else if matches!(arg.value, ArgumentValueSource::PositionalSplice(_)) {
                ArenaCallArgInput::Splice { value, span: arg.span }
            } else { ArenaCallArgInput::Positional(value) });
        }
        let args = temporary.arena.append_call_arguments(&inputs);
        let result = self.check_call_arena(&temporary, source, callee, args, span, expected_context);
        for id in projections { self.argument_projection_types.remove(&id); self.argument_projection_sources.remove(&id); }
        result
    }

    fn check_inferred_record_constructor_arena(
        &mut self, arena: &ArenaProgram, source: &str, callee: ExprId,
        record_definition: crate::syntax::arena::TypeDefId, args: &[ArenaCallArg],
        expected_context: Option<&Type>, span: Span,
    ) -> Type {
        let inference = match self.record_constructors.begin_constructor_inference(
            &arena.arena, callee, self.current_namespace, span, expected_context,
            self.expected_schema.as_ref(), &mut self.type_constraints,
        ) {
            Ok(inference) => inference,
            Err(error) => { self.error(span, &error.message, error.code); return Type::Invalid; }
        };
        self.constructor_group_depth += 1;
        let actual = self.check_record_constructor_arena(arena, source, record_definition, args, inference.ty, Some(&inference.expectation), span);
        self.pending_record_constructors.push((span, inference.instance, actual.clone()));
        self.constructor_group_depth -= 1;
        if self.constructor_group_depth != 0 { return actual; }

        // Nested occurrences can share variables with their surrounding field
        // expectations. Publish only after every supplied field contributed.
        let pending = std::mem::take(&mut self.pending_record_constructors);
        for (call_span, instance, _) in pending {
            match self.record_constructors.finish_constructor_inference(&arena.arena, &instance, &self.type_constraints) {
                Ok(fact) => {
                    self.expr_types.insert(call_span, fact.ty.clone());
                    self.record_constructor_instances.insert(call_span, fact);
                }
                Err(error) => self.error(call_span, &error.message, error.code),
            }
        }
        for ty in self.expr_types.values_mut() {
            if ty.contains_inference() {
                *ty = self.type_constraints.resolve(ty).ok().filter(|resolved| !resolved.contains_inference()).unwrap_or(Type::Invalid);
            }
        }
        self.record_constructor_instances.get(&span).map_or(Type::Invalid, |fact| fact.ty.clone())
    }

    fn check_record_constructor_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        definition: crate::syntax::arena::TypeDefId,
        args: &[ArenaCallArg],
        expected: Type,
        schema: Option<&crate::sema::constants::SchemaExpectation>,
        span: Span,
    ) -> Type {
        let Type::Record(fields) = &expected else { return Type::Invalid; };
        let defaults = self.record_constructors.defaults(definition).cloned().unwrap_or_default();
        let mut supplied = super::FxHashSet::default();
        for arg in args {
            let ArenaCallArgKind::Named { name, .. } = arg.kind else {
                self.error(call_arg_span_arena(arena, &arg.kind),
                    "record constructors require named fields", "check.record-constructor");
                self.check_call_arg_arena(arena, source, &arg.kind, None);
                continue;
            };
            if !supplied.insert(name) {
                self.error(call_arg_span_arena(arena, &arg.kind),
                    "duplicate constructor field", "check.record-constructor");
            }
            let field_type = fields.get(&name);
            if field_type.is_none() {
                self.error(call_arg_span_arena(arena, &arg.kind),
                    "unknown constructor field", "check.record-constructor");
            }
            let context = schema.and_then(|schema| schema.children.get(&crate::sema::constants::SchemaComponent::Field(name))).cloned();
            let previous = std::mem::replace(&mut self.expected_schema, context);
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, field_type);
            self.expected_schema = previous;
            if let Some(field_type) = field_type {
                self.expect_type(field_type, &actual, call_arg_span_arena(arena, &arg.kind));
            }
        }
        for name in fields.keys() {
            if !supplied.contains(name) && !defaults.contains_key(name) {
                self.error(span, &format!("missing required constructor field `{name}`"),
                    "check.record-constructor");
            }
        }
        expected
    }

    pub(super) fn check_constructor_call_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        name: &str,
        args: &[ArenaCallArg],
        span: Span,
        expected_context: Option<&Type>,
    ) -> Type {
        if let Some(info) = self.tag_variants.get(&Name::intern(name)).cloned() {
            if args.len() != info.field_count {
                self.error(
                    span,
                    &format!(
                        "tag constructor `{name}` expects {} argument(s), got {}",
                        info.field_count,
                        args.len()
                    ),
                    "check.arity",
                );
            }
            for (arg, expected_ty) in args.iter().zip(info.field_types.iter()) {
                let actual = self.check_call_arg_arena(arena, source, &arg.kind, Some(expected_ty));
                self.expect_type(expected_ty, &actual, call_arg_span_arena(arena, &arg.kind));
            }
            return Type::Tag(info.type_name);
        }
        match name {
            "Ok" => {
                let expected = expected_context.and_then(Type::result_ok);
                let schema = self.expected_schema.as_ref().and_then(|schema| schema.children.get(&crate::sema::constants::SchemaComponent::Success)).cloned();
                let previous = std::mem::replace(&mut self.expected_schema, schema);
                let ty = args.first().map_or(Type::Unit, |arg| self.check_call_arg_arena(arena, source, &arg.kind, expected));
                self.expected_schema = previous;
                Type::Result(Box::new(ty), Box::new(Type::Error))
            }
            "Err" => {
                let expected = match expected_context { Some(Type::Result(_, error)) => Some(error.as_ref()), _ => None };
                let previous = std::mem::replace(&mut self.expected_schema, None);
                let err = args.first().map_or(Type::Error, |arg| self.check_call_arg_arena(arena, source, &arg.kind, expected));
                self.expected_schema = previous;
                Type::Result(Box::new(Type::Unknown), Box::new(err))
            }
            "Error" => {
                self.error(
                    span,
                    "`Error(kind: ...)` was removed; construct a declared error variant such as `FsError.NotFound(...)`",
                    "check.error-removed",
                );
                for arg in args {
                    self.check_call_arg_arena(arena, source, &arg.kind, None);
                }
                Type::Error
            }
            "ProcessError" => {
                if self.options.migration_diagnostics {
                    self.warning(
                        span,
                        "`ProcessError(...)` is produced by process APIs and is not a source constructor",
                        "check.migration-error",
                    );
                }
                for arg in args {
                    self.check_call_arg_arena(arena, source, &arg.kind, None);
                }
                Type::ProcessError
            }
            "abort" => {
                self.check_abort_call_arena(arena, source, args, span);
                Type::Unit
            }
            "env" => {
                self.require_effect(Effect::Env, span, "environment lookup");
                self.check_expr_arg_list_arena(arena, source, args, &[Type::Str], span);
                Type::Result(Box::new(Type::Str), Box::new(Type::Error))
            }
            "Path" => {
                self.check_expr_arg_list_arena(arena, source, args, &[Type::Str], span);
                Type::Path
            }
            "range" if args.len() == 1 || args.len() == 2 => Type::Stream(Box::new(Type::Int)),
            _ => {
                self.record_effect_contract(&None, name);
                self.error(
                    span,
                    "unresolved pure function call",
                    "check.unresolved-call",
                );
                Type::Unknown
            }
        }
    }

    pub(super) fn check_abort_call_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        span: Span,
    ) {
        self.terminating_call_spans.insert(span);
        if args.is_empty() || args.len() > 2 {
            self.error(
                span,
                "abort expects status and optional force",
                "check.arity",
            );
        }
        if let Some(arg) = args.first() {
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, Some(&Type::Int));
            self.expect_type(&Type::Int, &actual, call_arg_span_arena(arena, &arg.kind));
        }
        if let Some(arg) = args.get(1) {
            if let ArenaCallArgKind::Named { name, .. } = &arg.kind
                && *name != "force"
            {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    "unexpected named parameter",
                    "check.named-arg",
                );
            }
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, Some(&Type::Bool));
            self.expect_type(&Type::Bool, &actual, call_arg_span_arena(arena, &arg.kind));
        }
    }

    pub(super) fn check_error_variant_constructor_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        family: Name,
        variant: Name,
        args: &[ArenaCallArg],
        span: Span,
    ) -> Type {
        let Some(info) = self
            .error_families
            .get(&family)
            .and_then(|family| family.variants.get(&variant))
            .cloned()
        else {
            self.error(span, "unknown error variant", "check.error-constructor");
            for arg in args {
                self.check_call_arg_arena(arena, source, &arg.kind, None);
            }
            return Type::Error;
        };

        let mut seen = FxHashSet::default();
        let field_names: Vec<_> = info.fields.keys().copied().collect();
        let mut positional_index = 0usize;
        for arg in args {
            let (name, expected) = match &arg.kind {
                ArenaCallArgKind::Named { name, .. } => {
                    let Some(expected) = info.fields.get(name) else {
                        self.error(
                            call_arg_span_arena(arena, &arg.kind),
                            "unknown error payload field",
                            "check.error-constructor",
                        );
                        self.check_call_arg_arena(arena, source, &arg.kind, None);
                        continue;
                    };
                    (*name, expected.clone())
                }
                ArenaCallArgKind::Positional(_) => {
                    let Some(name) = field_names.get(positional_index).copied() else {
                        self.error(
                            call_arg_span_arena(arena, &arg.kind),
                            "too many error constructor arguments",
                            "check.arity",
                        );
                        self.check_call_arg_arena(arena, source, &arg.kind, None);
                        continue;
                    };
                    positional_index += 1;
                    let expected = info.fields.get(&name).cloned().unwrap_or(Type::Unknown);
                    (name, expected)
                }
                ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. } => {
                    self.error(
                        call_arg_span_arena(arena, &arg.kind),
                        "error constructors do not accept argument splices",
                        "check.splice-target",
                    );
                    self.check_call_arg_arena(arena, source, &arg.kind, None);
                    continue;
                }
            };
            if !seen.insert(name) {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    "duplicate error payload field",
                    "check.error-constructor",
                );
            }
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, Some(&expected));
            self.expect_type(&expected, &actual, call_arg_span_arena(arena, &arg.kind));
        }
        for name in info.fields.keys() {
            if !seen.contains(name) {
                self.error(
                    span,
                    "missing error payload field",
                    "check.error-constructor",
                );
            }
        }
        Type::ErrorVariant { family, variant }
    }

    pub(super) fn check_module_call_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        module: &str,
        name: &str,
        args: &[ArenaCallArg],
        span: Span,
    ) -> Type {
        let Some(module_sig) = api_spec().module(module) else {
            self.error(span, "unknown module", "check.unknown-module");
            return Type::Unknown;
        };
        if module == "env" && name == "get_path" {
            self.error(
                span,
                "`env.get_path` is not supported; use `env.Path.NAME`",
                "check.unsupported-api",
            );
            return Type::Result(Box::new(Type::Path), Box::new(Type::Error));
        }
        if module == "path" && name == "display" {
            self.error(
                span,
                "`path.display` is not supported; use `path_value.display()`",
                "check.unsupported-api",
            );
            return Type::Str;
        }
        let migration = if module == "fs" {
            xsh_registry::signature::legacy_fs_root_method(name).map(|method| {
                self.error(span, &format!("`fs.{name}` was removed; use an FsRoot receiver's `{method}` method"), "check.unsupported-api");
                api_spec().method_overloads(MethodReceiver::FsRoot, method).expect("root receiver registry")
                    .iter().map(|method| {
                        let mut sig = method.sig.clone();
                        sig.params.insert(0, crate::modules::signature::ParamSig { name: "root", ty: Type::FsRoot, defaulted: false });
                        sig
                    }).collect::<Vec<_>>()
            })
        } else { None };
        let Some(overloads) = module_sig.function_overloads(name).or(migration.as_deref()) else {
            self.error(span, "unknown module API", "check.unknown-module-api");
            return Type::Unknown;
        };
        if module == "process" && name == "command_argv" {
            return self.check_process_command_argv_call_arena(arena, source, args, span);
        }
        let (sig, args_checked) = if overloads.len() == 1 {
            (&overloads[0], false)
        } else {
            (
                self.check_module_overload_args_arena(
                    arena, source, module, name, args, overloads, span,
                ),
                true,
            )
        };
        if self.in_pure && !sig.pure {
            self.error(
                span,
                "effectful module API is not allowed in pure functions",
                "check.pure-effect",
            );
        }
        match sig.arg_check {
            ApiArgCheck::JsonCompatible => {
                self.check_json_api_args_arena(arena, source, name, args, span);
            }
            ApiArgCheck::HashVerifyFile => {
                self.check_hash_verify_file_args_arena(arena, source, args, span);
            }
            ApiArgCheck::Standard => {
                if !args_checked {
                    self.check_module_sig_args_arena(arena, source, args, sig, span);
                }
            }
            ApiArgCheck::PathLikeSingle | ApiArgCheck::ResultContext => {
                if !args_checked {
                    self.check_module_sig_args_arena(arena, source, args, sig, span);
                }
            }
        }
        let return_ty = if sig.semantic_rule == crate::modules::signature::SemanticRule::CliDescriptor {
            self.infer_cli_descriptor_return_arena(arena, args, sig.op).unwrap_or_else(|| sig.return_ty.clone())
        } else { sig.return_ty.clone() };
        if self.options.strict_dynamic && module == "record" && name == "require" {
            self.check_contract_literal_args_arena(arena, args);
        }
        return_ty
    }

    fn infer_cli_descriptor_return_arena(
        &mut self,
        arena: &ArenaProgram,
        args: &[ArenaCallArg],
        op: xsh_registry::RuntimeOp,
    ) -> Option<Type> {
        let schema = crate::modules::cli::descriptor_argument(args)?;
        let applet = op == xsh_registry::RuntimeOp::CliApplet;
        match self.prepared_constants.cli_descriptor_plan(&arena.arena, schema, applet)? {
            Ok(plan) => Some(plan.return_type(op == xsh_registry::RuntimeOp::CliParseFull)),
            Err(error) => {
                let span = error.span.unwrap_or(arena.arena.expr(schema).span);
                self.error(span, &error.message, "check.cli-descriptor");
                None
            }
        }
    }

    pub(super) fn check_contract_literal_args_arena(
        &mut self,
        arena: &ArenaProgram,
        args: &[ArenaCallArg],
    ) {
        for (index, arg) in args.iter().enumerate() {
            let is_contract_position = match &arg.kind {
                ArenaCallArgKind::Named { name, .. } => {
                    matches!(name.as_str().as_str(), "required" | "optional")
                }
                ArenaCallArgKind::Positional(_) => index == 1 || index == 2,
                ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. } => false,
            };
            if !is_contract_position {
                continue;
            }
            let expr_id = call_arg_expr_id_arena(&arg.kind);
            let ArenaExprKind::Record(fields_range) = arena.arena.expr(expr_id).kind else {
                continue;
            };
            for field in arena.arena.record_fields(fields_range) {
                match &field.kind {
                    ArenaRecordFieldKind::Named { value, span, .. } => {
                        let field_span = arena.arena.span(*span);
                        let value_expr = arena.arena.expr(*value);
                        let ArenaExprKind::Str(text_id) = value_expr.kind else {
                            self.warning(
                                field_span,
                                "contract field type must be a string literal",
                                "check.contract-type",
                            );
                            continue;
                        };
                        let text = arena.arena.string_literal(text_id).clone();
                        if !contract_type_is_valid(&text) {
                            self.warning(
                                value_expr.span,
                                "malformed contract type string",
                                "check.contract-type",
                            );
                        }
                    }
                    ArenaRecordFieldKind::Shorthand { span, .. }
                    | ArenaRecordFieldKind::Computed { span, .. }
                    | ArenaRecordFieldKind::Path { span, .. }
                    | ArenaRecordFieldKind::Spread { span, .. } => {
                        let field_span = arena.arena.span(*span);
                        self.warning(
                            field_span,
                            "contract records must use literal field type strings",
                            "check.contract-type",
                        );
                    }
                }
            }
        }
    }

    pub(super) fn check_process_command_argv_call_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        span: Span,
    ) -> Type {
        let names = [
            "target",
            "argv",
            "cwd",
            "env",
            "stdin",
            "stdout",
            "stderr",
            "stdout_append",
            "stderr_append",
            "timeout",
            "detach",
            "new_session",
            "ignore_hup",
            "cpu_max",
        ];
        if !(2..=names.len()).contains(&args.len()) {
            self.error(span, "incorrect standard API arity", "check.arity");
        }
        let mut slots: [Option<&ArenaCallArgKind>; 14] = [None; 14];
        let mut next_positional = 0;
        for arg in args {
            match &arg.kind {
                ArenaCallArgKind::Named { name, .. } => {
                    let Some(index) = names.iter().position(|expected| *expected == name.as_str())
                    else {
                        self.error(
                            call_arg_span_arena(arena, &arg.kind),
                            "unexpected named parameter",
                            "check.named-arg",
                        );
                        continue;
                    };
                    if slots[index].is_some() {
                        self.error(
                            call_arg_span_arena(arena, &arg.kind),
                            "duplicate named parameter",
                            "check.named-arg",
                        );
                    }
                    slots[index] = Some(&arg.kind);
                }
                ArenaCallArgKind::Positional(_) => {
                    while next_positional < slots.len() && slots[next_positional].is_some() {
                        next_positional += 1;
                    }
                    if next_positional >= slots.len() {
                        self.error(
                            call_arg_span_arena(arena, &arg.kind),
                            "unexpected positional argument",
                            "check.arity",
                        );
                    } else {
                        slots[next_positional] = Some(&arg.kind);
                        next_positional += 1;
                    }
                }
                ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. } => {
                    self.error(
                        call_arg_span_arena(arena, &arg.kind),
                        "invalid argument splice",
                        "check.splice-target",
                    );
                }
            }
        }

        if slots[0].is_none() || slots[1].is_none() {
            self.error(span, "incorrect standard API arity", "check.arity");
        }

        let target_ty = self.check_optional_api_arg_arena(arena, source, slots[0], None);
        if !matches!(
            target_ty,
            Type::Str | Type::Path | Type::Any | Type::Unknown
        ) {
            self.error(
                slots[0]
                    .map(|k| call_arg_span_arena(arena, k))
                    .unwrap_or(span),
                "expected Str or Path",
                "check.type-mismatch",
            );
        }
        self.check_process_command_argv_argv_arena(arena, source, slots[1], span);
        let expected = [
            Type::Path,
            Type::Record(BTreeMap::new()),
            Type::Path,
            Type::Path,
            Type::Path,
            Type::Bool,
            Type::Bool,
            Type::Duration,
            Type::Bool,
            Type::Bool,
            Type::Bool,
            Type::Int,
        ];
        for (offset, expected) in expected.iter().enumerate() {
            if offset + 2 == 4 {
                let actual = self.check_optional_api_arg_arena(arena, source, slots[4], None);
                if let Some(arg) = slots[4] && actual != Type::Bytes {
                    self.expect_type(&Type::Path, &actual, call_arg_span_arena(arena, arg));
                }
            } else { self.check_optional_api_arg_arena(arena, source, slots[offset + 2], Some(expected)); }
        }
        if let Some(arg) = slots[13] {
            let expr_id = call_arg_expr_id_arena(arg);
            self.check_static_positive_call_int_arena(arena, expr_id, "cpu_max must be positive");
        }
        Type::Command
    }

    fn check_process_command_argv_argv_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        arg: Option<&ArenaCallArgKind>,
        span: Span,
    ) {
        let Some(arg) = arg else {
            self.error(span, "incorrect standard API arity", "check.arity");
            return;
        };

        let expr_id = call_arg_expr_id_arena(arg);
        let expr = arena.arena.expr(expr_id);

        if let ArenaExprKind::List(items) = expr.kind {
            if items.is_empty() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "process.command_argv argv must include argv[0], the child program name",
                    )
                    .with_code("check.process-argv-empty")
                    .with_label(Label::primary(expr.span, "argv is empty"))
                    .with_note("include the child program name as the first argv item"),
                );
                return;
            }
            for (index, item) in arena.arena.list_elements(items).enumerate() {
                if index == 0 && item.splice_span.is_none() { continue; }
                let actual = self.check_expr_arena(arena, source, item.value, None);
                let item_ty = if item.splice_span.is_some() {
                    match actual {
                        Type::List(ty) => *ty,
                        _ => {
                            self.error(arena.arena.span(item.splice_span.unwrap()), "list literal splice requires List", "check.list-splice-type");
                            Type::Unknown
                        }
                    }
                } else { actual };
                if !process_command_argv_item_type_is_valid(&item_ty) {
                    self.error(arena.arena.expr(item.value).span, "process.command_argv argv items must be Str or Path", "check.type-mismatch");
                }
            }
            return;
        }

        let actual = self.check_call_arg_arena(arena, source, arg, None);
        match actual {
            Type::List(item) if process_command_argv_item_type_is_valid(&item) => {}
            Type::Any | Type::Unknown => {}
            Type::List(_) => self.error(
                call_arg_span_arena(arena, arg),
                "process.command_argv argv must contain Str or Path items",
                "check.type-mismatch",
            ),
            _ => self.error(
                call_arg_span_arena(arena, arg),
                "process.command_argv argv must be a List",
                "check.type-mismatch",
            ),
        }
    }

    fn check_static_positive_call_int_arena(
        &mut self,
        arena: &ArenaProgram,
        expr_id: ExprId,
        message: &str,
    ) {
        let expr = arena.arena.expr(expr_id);
        match &expr.kind {
            ArenaExprKind::Int(value_id)
                if arena
                    .arena
                    .int_literal(*value_id)
                    .value()
                    .is_some_and(|value| value <= 0) =>
            {
                self.error(expr.span, message, "check.named-arg");
            }
            ArenaExprKind::Unary {
                op: UnaryOp::Neg,
                expr: inner,
            } if matches!(arena.arena.expr(*inner).kind, ArenaExprKind::Int(_)) => {
                self.error(expr.span, message, "check.named-arg");
            }
            _ => {}
        }
    }

    pub(super) fn check_hash_verify_file_args_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        span: Span,
    ) {
        if args.len() != 2 {
            self.error(
                span,
                "verify_file requires path and checksum",
                "check.arity",
            );
            return;
        }
        let path_ty = self.check_call_arg_arena(arena, source, &args[0].kind, Some(&Type::Path));
        let path_expr_id = call_arg_expr_id_arena(&args[0].kind);
        let path_kind = arena.arena.expr(path_expr_id).kind;
        if !is_path_like_arena_expr(&path_kind, &path_ty) {
            self.expect_type(
                &Type::Path,
                &path_ty,
                call_arg_span_arena(arena, &args[0].kind),
            );
        }
        let ArenaCallArgKind::Named { name, .. } = &args[1].kind else {
            self.error(
                call_arg_span_arena(arena, &args[1].kind),
                "checksum argument must be named",
                "check.named-arg",
            );
            self.check_call_arg_arena(arena, source, &args[1].kind, Some(&Type::Str));
            return;
        };
        if !matches!(name.as_str().as_str(), "md5" | "sha1" | "sha256" | "sha512") {
            self.error(
                call_arg_span_arena(arena, &args[1].kind),
                "unsupported checksum algorithm",
                "check.named-arg",
            );
        }
        let actual = self.check_call_arg_arena(arena, source, &args[1].kind, Some(&Type::Str));
        self.expect_type(
            &Type::Str,
            &actual,
            call_arg_span_arena(arena, &args[1].kind),
        );
    }

    pub(super) fn check_json_api_args_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        name: &str,
        args: &[ArenaCallArg],
        span: Span,
    ) {
        match name {
            "encode" => {
                if !(1..=2).contains(&args.len()) {
                    self.error(span, "incorrect standard API arity", "check.arity");
                    return;
                }
                self.check_named_arg_arena(arena, &args[0].kind, "value");
                let actual = self.check_call_arg_arena(arena, source, &args[0].kind, None);
                self.expect_json_compatible(&actual, call_arg_span_arena(arena, &args[0].kind));
                if let Some(arg) = args.get(1) {
                    self.check_named_arg_arena(arena, &arg.kind, "pretty");
                    let actual =
                        self.check_call_arg_arena(arena, source, &arg.kind, Some(&Type::Bool));
                    self.expect_type(&Type::Bool, &actual, call_arg_span_arena(arena, &arg.kind));
                }
            }
            "encode_lines" => {
                if args.len() != 1 {
                    self.error(span, "incorrect standard API arity", "check.arity");
                    return;
                }
                self.check_named_arg_arena(arena, &args[0].kind, "values");
                let expected = Type::List(Box::new(Type::Any));
                let actual =
                    self.check_call_arg_arena(arena, source, &args[0].kind, Some(&expected));
                self.expect_type(
                    &expected,
                    &actual,
                    call_arg_span_arena(arena, &args[0].kind),
                );
                self.expect_json_compatible(&actual, call_arg_span_arena(arena, &args[0].kind));
            }
            "write" => {
                if !(2..=3).contains(&args.len()) {
                    self.error(span, "incorrect standard API arity", "check.arity");
                    return;
                }
                self.check_named_arg_arena(arena, &args[0].kind, "path");
                self.check_named_arg_arena(arena, &args[1].kind, "value");
                let path_ty =
                    self.check_call_arg_arena(arena, source, &args[0].kind, Some(&Type::Path));
                let path_expr_id = call_arg_expr_id_arena(&args[0].kind);
                let path_kind = arena.arena.expr(path_expr_id).kind;
                if !is_path_like_arena_expr(&path_kind, &path_ty) {
                    self.expect_type(
                        &Type::Path,
                        &path_ty,
                        call_arg_span_arena(arena, &args[0].kind),
                    );
                }
                let value_ty = self.check_call_arg_arena(arena, source, &args[1].kind, None);
                self.expect_json_compatible(&value_ty, call_arg_span_arena(arena, &args[1].kind));
                if let Some(arg) = args.get(2) {
                    self.check_named_arg_arena(arena, &arg.kind, "pretty");
                    let actual =
                        self.check_call_arg_arena(arena, source, &arg.kind, Some(&Type::Bool));
                    self.expect_type(&Type::Bool, &actual, call_arg_span_arena(arena, &arg.kind));
                }
            }
            "write_lines" => {
                if args.len() != 2 {
                    self.error(span, "incorrect standard API arity", "check.arity");
                    return;
                }
                self.check_named_arg_arena(arena, &args[0].kind, "path");
                self.check_named_arg_arena(arena, &args[1].kind, "values");
                let path_ty =
                    self.check_call_arg_arena(arena, source, &args[0].kind, Some(&Type::Path));
                let path_expr_id = call_arg_expr_id_arena(&args[0].kind);
                let path_kind = arena.arena.expr(path_expr_id).kind;
                if !is_path_like_arena_expr(&path_kind, &path_ty) {
                    self.expect_type(
                        &Type::Path,
                        &path_ty,
                        call_arg_span_arena(arena, &args[0].kind),
                    );
                }
                let expected = Type::List(Box::new(Type::Any));
                let value_ty =
                    self.check_call_arg_arena(arena, source, &args[1].kind, Some(&expected));
                self.expect_type(
                    &expected,
                    &value_ty,
                    call_arg_span_arena(arena, &args[1].kind),
                );
                self.expect_json_compatible(&value_ty, call_arg_span_arena(arena, &args[1].kind));
            }
            "set" => {
                if args.len() != 3 {
                    self.error(span, "incorrect standard API arity", "check.arity");
                    return;
                }
                self.check_named_arg_arena(arena, &args[0].kind, "value");
                self.check_named_arg_arena(arena, &args[1].kind, "path");
                self.check_named_arg_arena(arena, &args[2].kind, "replacement");
                let value_ty = self.check_call_arg_arena(arena, source, &args[0].kind, None);
                self.expect_json_compatible(&value_ty, call_arg_span_arena(arena, &args[0].kind));
                let path_ty = self.check_call_arg_arena(
                    arena,
                    source,
                    &args[1].kind,
                    Some(&Type::List(Box::new(Type::Any))),
                );
                self.expect_type(
                    &Type::List(Box::new(Type::Any)),
                    &path_ty,
                    call_arg_span_arena(arena, &args[1].kind),
                );
                let replacement_ty = self.check_call_arg_arena(arena, source, &args[2].kind, None);
                self.expect_json_compatible(
                    &replacement_ty,
                    call_arg_span_arena(arena, &args[2].kind),
                );
            }
            _ => {}
        }
    }

    pub(super) fn check_named_arg_arena(
        &mut self,
        arena: &ArenaProgram,
        arg: &ArenaCallArgKind,
        expected: &str,
    ) {
        if let ArenaCallArgKind::Named { name, .. } = arg
            && name != expected
        {
            self.error(
                call_arg_span_arena(arena, arg),
                "unexpected named parameter",
                "check.named-arg",
            );
        }
    }
}
