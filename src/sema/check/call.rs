#![allow(clippy::single_call_fn)]

use super::expr::is_path_like_arena_expr;
use super::{
    ApiArgCheck, BTreeMap, CallableParamType, Checker, Diagnostic, FxHashSet, Label,
    MethodReceiver, ModuleExportType, Name, QualifiedName, Span, Type, UnaryOp, api_spec,
    call_arg_expr_id_arena, call_arg_span_arena,
};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{
    ArenaCallArg, ArenaCallArgKind, ArenaExprKind, ArenaProgram, ArenaRange, ExprId,
};
use crate::syntax::node::Effect;

fn process_command_argv_item_type_is_valid(ty: &Type) -> bool {
    matches!(ty, Type::Str | Type::Path | Type::Any | Type::Unknown)
}

#[allow(dead_code)]
/// Rewrites the positional argument at `argument` to name the field it fills,
/// as a pun when the argument is that name.
fn named_argument_fix(source: &str, argument: Span, field: Name) -> Option<super::FixHint> {
    let text = source.get(argument.range())?;
    let replacement = if field == text {
        format!("{field}:")
    } else {
        format!("{field}: {text}")
    };
    Some(super::FixHint::replacement(
        argument,
        format!("pass `{field}` by name"),
        replacement,
    ))
}

impl Checker {
    pub(super) fn warn_flattened_error_handler_arena(
        &mut self,
        arena: &ArenaProgram,
        value: ExprId,
        pattern: crate::syntax::arena::PatternId,
        subject: &Type,
    ) {
        use crate::syntax::arena::ArenaPatternKind;
        if !self.options.migration_diagnostics {
            return;
        }
        let Type::Result(_, error) = subject else {
            return;
        };
        if !matches!(
            error.as_ref(),
            Type::Error | Type::ProcessError | Type::ErrorFamily(_) | Type::ErrorVariant { .. }
        ) {
            return;
        }
        let ArenaPatternKind::Constructor {
            name,
            arg: Some(arg),
        } = arena.arena.pattern(pattern).kind
        else {
            return;
        };
        if name != "Err" {
            return;
        }
        let ArenaPatternKind::Binding(failure) = arena.arena.pattern(arg).kind else {
            return;
        };
        self.warn_flattened_error_translation_arena(arena, value, failure);
    }

    pub(super) fn single_error_handler_value_arena(
        arena: &ArenaProgram,
        block: crate::syntax::arena::BlockId,
    ) -> Option<ExprId> {
        use crate::syntax::arena::{ArenaExprOrRun, ArenaStmtKind};
        let mut statements = arena.arena.stmt_ids(arena.arena.block(block).statements);
        let statement = statements.next()?;
        if statements.next().is_some() {
            return None;
        }
        match arena.arena.stmt(arena.arena.core_stmt_id(statement)).kind {
            ArenaStmtKind::Expr(value)
            | ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(value))) => Some(value),
            _ => None,
        }
    }

    pub(super) fn warn_flattened_error_translation_arena(
        &mut self,
        arena: &ArenaProgram,
        value: ExprId,
        failure: Name,
    ) {
        if !self.options.migration_diagnostics {
            return;
        }
        let value = match arena.arena.expr(value).kind {
            ArenaExprKind::ValueBlock(block) => {
                match Self::single_error_handler_value_arena(arena, block) {
                    Some(value) => value,
                    None => return,
                }
            }
            _ => value,
        };
        let ArenaExprKind::Call { callee, args } = arena.arena.expr(value).kind else {
            return;
        };
        if !matches!(arena.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Err") {
            return;
        }
        let [arg] = arena.arena.call_args(args) else {
            return;
        };
        let ArenaCallArgKind::Positional(outer) = arg.kind else {
            return;
        };
        let ArenaExprKind::Call { callee, args } = arena.arena.expr(outer).kind else {
            return;
        };
        let ArenaExprKind::Field {
            base,
            name: variant,
        } = arena.arena.expr(callee).kind
        else {
            return;
        };
        let ArenaExprKind::Ident(family) = arena.arena.expr(base).kind else {
            return;
        };
        if !self
            .error_families
            .get(&family)
            .is_some_and(|family| family.variants.contains_key(&variant))
        {
            return;
        }
        let [arg] = arena.arena.call_args(args) else {
            return;
        };
        let message = match arg.kind {
            ArenaCallArgKind::Positional(value) => value,
            ArenaCallArgKind::Named { name, value, .. } if name == "message" => value,
            _ => return,
        };
        let ArenaExprKind::Field { base, name } = arena.arena.expr(message).kind else {
            return;
        };
        if name != "message"
            || !matches!(arena.arena.expr(base).kind, ArenaExprKind::Ident(name) if name == failure)
        {
            return;
        }
        self.warning(arena.arena.expr(value).span,
            &format!("error translation retains only `{failure}.message`; consider `cause: {failure}` to preserve its typed diagnostic chain (no automatic fix)"),
            DiagnosticCode::CheckErrorCause);
    }

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
                DiagnosticCode::CheckJsonCompatible,
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
        if let ArenaExprKind::Field { base, name } = callee_kind
            && matches!(arena.arena.expr(base).kind, ArenaExprKind::Item)
            && !self.item_shorthand_in_scope()
        {
            return self.check_inferred_variant_call(
                arena,
                source,
                name,
                args,
                span,
                expected_context,
            );
        }
        if self.check_removed_record_require_arena(arena, source, callee, args, span) {
            return Type::Invalid;
        }
        if args
            .iter()
            .any(|arg| matches!(arg.kind, ArenaCallArgKind::NamedSpread { .. }))
        {
            // A spread supplies fields by name; positional arguments beside
            // it would bind against fields the spread may also supply.
            if let Some(arg) = args
                .iter()
                .find(|arg| matches!(arg.kind, ArenaCallArgKind::Positional(_)))
                && self
                    .record_constructors
                    .resolve_call(&arena.arena, callee, self.current_namespace)
                    .is_some()
            {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    "a record constructor with a field spread takes only named arguments",
                    DiagnosticCode::CheckRecordConstructor,
                );
                for arg in args {
                    self.check_call_arg_arena(arena, source, &arg.kind, None);
                }
                return Type::Invalid;
            }
            return self.check_spread_call_arena(
                arena,
                source,
                callee,
                args_range,
                span,
                expected_context,
            );
        }
        if let Some(alias) = self.resolve_callable_alias_call(arena, callee) {
            self.record_callable_alias(arena.arena.expr(callee).span, &alias);
            if let ArenaExprKind::Field { base, name } = callee_kind
                && name == "call"
                && self.resolve_callable_alias_call(arena, base).is_some()
            {
                self.static_callable_aliases
                    .get_mut(&arena.arena.expr(callee).span)
                    .unwrap()
                    .method_call = true;
            }
            if !alias.pure {
                if self.in_pure {
                    self.error(
                        span,
                        "effectful proc is not allowed in pure functions",
                        DiagnosticCode::CheckPureEffect,
                    );
                } else {
                    self.check_resolved_callable_effects(
                        &alias.signature,
                        &alias.name.to_string(),
                        span,
                    );
                }
            }
            self.check_function_arg_list_arena(arena, source, args, &alias.signature.params, span);
            if !alias.pure {
                self.invalidate_mutable_narrowings();
            }
            self.record_callee_propagation(
                &alias.signature.effects,
                &alias.signature.return_ty,
                span,
            );
            return alias.signature.return_ty;
        }

        if let Some(definition) =
            self.record_constructors
                .resolve_call(&arena.arena, callee, self.current_namespace)
        {
            return self.check_inferred_record_constructor_arena(
                arena,
                source,
                callee,
                definition,
                args,
                expected_context,
                span,
            );
        }

        if let ArenaExprKind::Ident(name) = callee_kind {
            if name == "reveal_type" {
                return self.check_reveal_type_call_arena(arena, source, args, span);
            }
            // A local binding shadows a function of the same name, and the
            // runtime calls the local; checking the call against the function
            // accepted programs that then failed with a runtime type error.
            if (self.procs.contains_key(&name)
                || self.pures.contains_key(&name)
                || self.streams.contains_key(&name))
                && let Some(binding) = self.lookup(name)
            {
                let ty = binding.ty.clone();
                self.error(
                    span,
                    &format!("local `{name}` of type {ty} shadows the function `{name}` and cannot be called"),
                    DiagnosticCode::CheckCallTarget,
                );
                for arg in args {
                    self.check_call_arg_arena(arena, source, &arg.kind, None);
                }
                return Type::Unknown;
            }
            if let Some(sig) = self.procs.get(&name).cloned() {
                if self.in_pure {
                    self.error(
                        span,
                        "effectful proc is not allowed in pure functions",
                        DiagnosticCode::CheckPureEffect,
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
                        DiagnosticCode::CheckPureEffect,
                    );
                } else {
                    self.check_resolved_callable_effects(&sig, &name.as_str(), span);
                }
                self.check_function_arg_list_arena(arena, source, args, &sig.params, span);
                self.record_callee_propagation(&sig.effects, &sig.return_ty, span);
                return sig.return_ty;
            }
            return self.check_constructor_call_arena(
                arena,
                source,
                &name.as_str(),
                args,
                span,
                expected_context,
            );
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
                    self.note_variant_qualifier(
                        span,
                        arena.arena.expr(callee).span,
                        name,
                        &super::InferredVariant::Error {
                            family: qualified_family,
                            variant: name,
                        },
                        expected_context,
                    );
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
                    let params = [super::FunctionParamSig {
                        name: Name::intern("message"),
                        ty: Type::Str,
                        schema_expectation: None,
                        defaulted: false,
                        rest: false,
                    }];
                    self.check_function_arg_list_arena(arena, source, args, &params, span);
                    return Type::Result(Box::new(Type::Unit), Box::new(Type::Error));
                }
                if self.error_families.contains_key(&module) {
                    self.note_variant_qualifier(
                        span,
                        arena.arena.expr(callee).span,
                        name,
                        &super::InferredVariant::Error {
                            family: module,
                            variant: name,
                        },
                        expected_context,
                    );
                    return self.check_error_variant_constructor_arena(
                        arena, source, module, name, args, span,
                    );
                }
                let qualified_tag = Name::intern(format!("{module}.{name}"));
                if let Some(info) = self.tag_variants.get(&qualified_tag) {
                    let selected = super::InferredVariant::Tag {
                        type_name: info.type_name,
                        variant: name,
                        field_types: Vec::new(),
                    };
                    self.note_variant_qualifier(
                        span,
                        arena.arena.expr(callee).span,
                        name,
                        &selected,
                        expected_context,
                    );
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
                            DiagnosticCode::CheckPureEffect,
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
                            DiagnosticCode::CheckPureEffect,
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
                        DiagnosticCode::CheckUnknownModuleApi,
                        expected_context,
                        None,
                    );
                }
                // A local binding takes precedence over a standard module name.
                // Without this guard, `let path = ...; path.read_text()` is
                // checked as the module call `path.read_text`, producing a
                // misleading unknown-module-api diagnostic instead of checking
                // the valid Path method.
                if self.lookup(module).is_none() && api_spec().module(&module.as_str()).is_some() {
                    let canonical_name = if module == "fs" && name == "ls" {
                        let callee_span = arena.arena.expr(callee).span;
                        self.removed_compatibility_name(
                            Span::new(
                                callee_span.source_id,
                                callee_span.end() - 2,
                                callee_span.end(),
                            ),
                            "ls",
                            "children",
                            true,
                        );
                        Name::intern("children")
                    } else {
                        name
                    };
                    if module == "process" && name == "command" {
                        self.error(
                            span,
                            "`process.command` requires a builder block with one `run` entry",
                            DiagnosticCode::CheckBuilderCall,
                        );
                    }
                    if let Some(required) = api_spec()
                        .module_required_effect(&module.as_str(), &canonical_name.as_str())
                    {
                        self.require_effect(required, span, &format!("`{module}.{name}`"));
                    }
                    return self.check_module_call_arena(
                        arena,
                        source,
                        &module.as_str(),
                        &canonical_name.as_str(),
                        args,
                        span,
                        expected_context,
                    );
                }
            }
            let base_ty = self.check_expr_arena(arena, source, base, None);
            let guarded_base = match arena.arena.expr(base).kind {
                ArenaExprKind::NullSafeField { .. }
                | ArenaExprKind::Index { guarded: true, .. }
                | ArenaExprKind::Slice { guarded: true, .. } => true,
                ArenaExprKind::Call { callee, .. } => matches!(
                    arena.arena.expr(callee).kind,
                    ArenaExprKind::NullSafeField { .. }
                ),
                _ => false,
            };
            if guarded_base && matches!(base_ty, Type::Optional(_)) {
                self.error(
                    span,
                    "a nullable postfix result needs its own `?.` method hop",
                    DiagnosticCode::CheckOptionalMethod,
                );
                return Type::Unknown;
            }

            return self.check_receiver_method_call_arena(
                arena,
                source,
                callee,
                base,
                base_ty,
                name,
                args,
                span,
                expected_context,
            );
        }

        if let ArenaExprKind::NullSafeField { base, name } = callee_kind {
            let base_ty = self.check_expr_arena(arena, source, base, None);
            let (inner_ty, wrap_optional) = match base_ty {
                Type::Optional(inner) if !matches!(*inner, Type::Any | Type::Unknown) => {
                    (*inner, true)
                }
                Type::Result(_, _) => (self.check_propagation(&base_ty, span), false),
                _ => {
                    self.error(
                        span,
                        "`?.` requires an Optional or Result value",
                        DiagnosticCode::CheckNullSafeField,
                    );
                    return Type::Unknown;
                }
            };
            if matches!(inner_ty, Type::Optional(_)) {
                self.error(
                    span,
                    "Result propagation leaves an Optional receiver; guard the next hop explicitly",
                    DiagnosticCode::CheckOptionalMethod,
                );
                return Type::Unknown;
            }
            let method_expected = if wrap_optional {
                expected_context.and_then(|ty| {
                    if let Type::Optional(inner) = ty {
                        Some(inner.as_ref())
                    } else {
                        None
                    }
                })
            } else {
                expected_context
            };
            let return_ty = self.check_receiver_method_call_arena(
                arena,
                source,
                callee,
                base,
                inner_ty,
                name,
                args,
                span,
                method_expected,
            );
            return if wrap_optional && !matches!(return_ty, Type::Optional(_)) {
                Type::Optional(Box::new(return_ty))
            } else {
                return_ty
            };
        }

        self.record_effect_contract(&None, "unresolved call target");
        self.error(
            span,
            "unsupported call target",
            DiagnosticCode::CheckCallTarget,
        );
        Type::Unknown
    }

    /// Checks `receiver.name(args)` once the receiver's type is known: a call
    /// to a module contract's callable export, or a registered method. `.` and
    /// `?.` calls both end here, so a receiver reached through propagation
    /// resolves exactly as one bound to a name first.
    #[allow(clippy::too_many_arguments)]
    fn check_receiver_method_call_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        callee: ExprId,
        base: ExprId,
        base_ty: Type,
        name: Name,
        args: &[ArenaCallArg],
        span: Span,
        expected: Option<&Type>,
    ) -> Type {
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
                            DiagnosticCode::CheckPureEffect,
                        );
                    } else if let Some(caller_effs) = self.current_effects.clone() {
                        self.check_module_callable_effects(
                            &caller_effs,
                            &sig.effects,
                            &name.as_str(),
                            span,
                        );
                    }
                    self.check_callee_not_excluded(&sig.effects, &[], &name.as_str(), span);
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
        let canonical_name = if base_ty == Type::Str && name == "count_bytes" {
            let callee_span = arena.arena.expr(callee).span;
            self.removed_compatibility_name(
                Span::new(
                    callee_span.source_id,
                    callee_span.end() - "count_bytes".len(),
                    callee_span.end(),
                ),
                "count_bytes",
                "byte_len",
                true,
            );
            Name::intern("byte_len")
        } else {
            name
        };
        self.check_method_dispatch_arena(
            arena,
            source,
            base,
            base_ty,
            &canonical_name.as_str(),
            args,
            span,
            expected,
        )
    }

    fn check_reveal_type_call_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        span: Span,
    ) -> Type {
        if args.len() != 1 {
            self.error(span, "incorrect function arity", DiagnosticCode::CheckArity);
        }
        for arg in args {
            if matches!(arg.kind, ArenaCallArgKind::Named { .. }) {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    "unexpected named parameter",
                    DiagnosticCode::CheckNamedArg,
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
                DiagnosticCode::CheckRevealType,
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
        let params = params
            .iter()
            .map(|param| super::FunctionParamSig {
                name: param.name,
                ty: param.ty.clone(),
                schema_expectation: Some(if param.rest {
                    crate::sema::constants::SchemaExpectation {
                        instances: Vec::new(),
                        children: BTreeMap::from([(
                            crate::sema::constants::SchemaComponent::Item,
                            crate::sema::constants::SchemaExpectation::default(),
                        )]),
                    }
                } else {
                    crate::sema::constants::SchemaExpectation::default()
                }),
                defaulted: param.defaulted,
                rest: param.rest,
            })
            .collect::<Vec<_>>();
        self.check_function_arg_list_arena(arena, source, args, &params, span);
    }

    fn check_spread_call_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        callee: ExprId,
        args_range: ArenaRange,
        span: Span,
        expected_context: Option<&Type>,
    ) -> Type {
        use crate::sema::arguments::{ArgumentValueSource, expand_named_arguments};
        use crate::syntax::arena::ArenaCallArgInput;
        let args = arena.arena.call_args(args_range);
        // Discover finite field sets without applying argument flow changes
        // to the real checker. Actual checks run at each source entry below.
        let mut probe = self.constraint_probe();
        let receiver_type = match arena.arena.expr(callee).kind {
            ArenaExprKind::Field { base, .. } => {
                Some(probe.check_expr_arena(arena, source, base, None))
            }
            _ => None,
        };
        let signature = self
            .resolve_callable_alias_call(arena, callee)
            .map(|alias| alias.signature)
            .or_else(|| match arena.arena.expr(callee).kind {
                ArenaExprKind::Ident(name) => self
                    .pures
                    .get(&name)
                    .or_else(|| self.procs.get(&name))
                    .or_else(|| self.streams.get(&name))
                    .cloned(),
                ArenaExprKind::Field { base, name } => {
                    if let ArenaExprKind::Ident(module) = arena.arena.expr(base).kind {
                        let qualified = QualifiedName::new(module, name);
                        self.qualified_pures
                            .get(&qualified)
                            .or_else(|| self.qualified_procs.get(&qualified))
                            .or_else(|| self.qualified_streams.get(&qualified))
                            .cloned()
                    } else {
                        None
                    }
                }
                _ => None,
            });
        let parameters = signature.map(|signature| signature.params).or_else(|| {
            let definition = self.record_constructors.constructor_definition(
                &arena.arena,
                callee,
                self.current_namespace,
            )?;
            let Type::Record(fields) = self.record_constructors.constructor_type(
                &arena.arena,
                callee,
                self.current_namespace,
            )?
            else {
                return None;
            };
            let schema = self
                .record_constructors
                .instance_expectation(&arena.arena, definition, &[])
                .ok()?;
            Some(
                fields
                    .into_iter()
                    .map(|(name, ty)| super::FunctionParamSig {
                        name,
                        ty,
                        schema_expectation: schema
                            .children
                            .get(&crate::sema::constants::SchemaComponent::Field(name))
                            .cloned(),
                        defaulted: false,
                        rest: false,
                    })
                    .collect(),
            )
        });
        let mut checked = super::FxHashMap::default();
        for arg in args {
            let value = call_arg_expr_id_arena(&arg.kind);
            let ty = probe.check_expr_arena(arena, source, value, None);
            if matches!(arg.kind, ArenaCallArgKind::NamedSpread { .. })
                && let (Some(parameters), Type::Record(fields)) = (&parameters, &ty)
            {
                let mut context = crate::sema::constants::SchemaExpectation::default();
                let expected = fields
                    .iter()
                    .map(|(name, actual)| {
                        if let Some(parameter) = parameters
                            .iter()
                            .find(|parameter| parameter.name == *name && !parameter.rest)
                        {
                            if let Some(schema) = &parameter.schema_expectation {
                                context.children.insert(
                                    crate::sema::constants::SchemaComponent::Field(*name),
                                    schema.clone(),
                                );
                            }
                            (*name, parameter.ty.clone())
                        } else {
                            (*name, actual.clone())
                        }
                    })
                    .collect();
                self.argument_projection_contexts
                    .insert(value, (Type::Record(expected), context));
            }
            checked.insert(value, ty);
        }
        let expanded = match expand_named_arguments(arena, args, |id| checked.get(&id).cloned()) {
            Ok(expanded) => expanded,
            Err(error) => {
                self.error(error.span, &error.message, DiagnosticCode::CheckNamedSpread);
                return Type::Invalid;
            }
        };
        let statically_named = self.resolve_callable_alias_call(arena, callee).is_some()
            || match arena.arena.expr(callee).kind {
                ArenaExprKind::Ident(name) => {
                    name == "Err"
                        || self.procs.contains_key(&name)
                        || self.pures.contains_key(&name)
                        || self.streams.contains_key(&name)
                        || self
                            .record_constructors
                            .resolve_call(&arena.arena, callee, self.current_namespace)
                            .is_some()
                }
                ArenaExprKind::Field { base, name } => {
                    let static_namespace = matches!(arena.arena.expr(base).kind, ArenaExprKind::Ident(namespace)
                    if api_spec().module(&namespace.as_str()).is_some() || self.error_families.contains_key(&namespace))
                        || self
                            .record_constructors
                            .resolve_call(&arena.arena, callee, self.current_namespace)
                            .is_some();
                    let positional_tag = matches!(arena.arena.expr(base).kind, ArenaExprKind::Ident(namespace)
                    if self.tag_variants.contains_key(&Name::intern(format!("{namespace}.{name}"))));
                    !positional_tag
                        && (static_namespace
                            || match receiver_type.clone().unwrap_or(Type::Unknown) {
                                Type::Module(exports) => exports.get(&name).is_some_and(|export| {
                                    matches!(
                                        export,
                                        ModuleExportType::Pure { .. }
                                            | ModuleExportType::Proc { .. }
                                    )
                                }),
                                Type::Any
                                | Type::Unknown
                                | Type::DynamicModule
                                | Type::Pure
                                | Type::Proc
                                | Type::Optional(_)
                                | Type::Result(_, _) => false,
                                _ => true,
                            })
                }
                _ => false,
            };
        if !statically_named {
            self.error(
                span,
                "named argument spreading requires a statically checked callable signature",
                DiagnosticCode::CheckNamedSpread,
            );
            return Type::Invalid;
        }
        let mut temporary = arena.clone();
        let mut inputs = Vec::new();
        let mut projections = Vec::new();
        let mut supplied = super::FxHashSet::default();
        let mut checked_entries = super::FxHashSet::default();
        for arg in expanded {
            if let Some(name) = arg.name
                && !supplied.insert(name)
            {
                self.error(
                    arg.span,
                    &format!("parameter `{name}` supplied more than once"),
                    DiagnosticCode::CheckNamedArg,
                );
            }
            let value = match arg.value {
                ArgumentValueSource::Expression(value)
                | ArgumentValueSource::PositionalSplice(value) => value,
                ArgumentValueSource::RecordField { record, field } => {
                    let id = temporary
                        .arena
                        .append_argument_projection(record, field, arg.span);
                    self.argument_projection_types.insert(id, arg.ty);
                    if checked_entries.insert(arg.entry_index) {
                        self.argument_projection_sources.insert(id, record);
                    }
                    projections.push(id);
                    id
                }
            };
            inputs.push(if let Some(name) = arg.name {
                ArenaCallArgInput::Named {
                    name,
                    value,
                    span: arg.span,
                }
            } else if matches!(arg.value, ArgumentValueSource::PositionalSplice(_)) {
                ArenaCallArgInput::Splice {
                    value,
                    span: arg.span,
                }
            } else {
                ArenaCallArgInput::Positional(value)
            });
        }
        let args = temporary.arena.append_call_arguments(&inputs);
        let result =
            self.check_call_arena(&temporary, source, callee, args, span, expected_context);
        for id in projections {
            self.argument_projection_types.remove(&id);
            self.argument_projection_sources.remove(&id);
        }
        for arg in arena.arena.call_args(args_range) {
            self.argument_projection_contexts
                .remove(&call_arg_expr_id_arena(&arg.kind));
        }
        result
    }

    fn check_inferred_record_constructor_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        callee: ExprId,
        record_definition: crate::syntax::arena::TypeDefId,
        args: &[ArenaCallArg],
        expected_context: Option<&Type>,
        span: Span,
    ) -> Type {
        let inference = match self.record_constructors.begin_constructor_inference(
            &arena.arena,
            callee,
            self.current_namespace,
            span,
            expected_context,
            self.expected_schema.as_ref(),
            &mut self.type_constraints,
        ) {
            Ok(inference) => inference,
            Err(error) => {
                self.error(span, &error.message, error.code);
                return Type::Invalid;
            }
        };
        self.constructor_group_depth += 1;
        let actual = self.check_record_constructor_arena(
            arena,
            source,
            record_definition,
            args,
            inference.ty,
            Some(&inference.expectation),
            span,
        );
        self.pending_record_constructors
            .push((span, inference.instance, actual.clone()));
        self.constructor_group_depth -= 1;
        if self.constructor_group_depth != 0 {
            return actual;
        }

        // Nested occurrences can share variables with their surrounding field
        // expectations. Publish only after every supplied field contributed.
        let pending = std::mem::take(&mut self.pending_record_constructors);
        for (call_span, instance, _) in pending {
            match self.record_constructors.finish_constructor_inference(
                &arena.arena,
                &instance,
                &self.type_constraints,
            ) {
                Ok(fact) => {
                    self.record_expr_type(call_span, fact.ty.clone());
                    self.record_constructor_instances.insert(call_span, fact);
                }
                Err(error) => self.error(call_span, &error.message, error.code),
            }
        }
        self.publish_unresolved_expr_types();
        self.record_constructor_instances
            .get(&span)
            .map_or(Type::Invalid, |fact| fact.ty.clone())
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
        let Type::Record(fields) = &expected else {
            return Type::Invalid;
        };
        let defaults = self
            .record_constructors
            .defaults(definition)
            .cloned()
            .unwrap_or_default();
        let mut supplied = super::FxHashSet::default();
        let declared = self
            .record_constructors
            .declared_fields(&arena.arena, definition);
        let mut positional = 0usize;
        let mut named_seen = false;
        let mut arg_fields = Vec::with_capacity(args.len());
        for arg in args {
            let name = match arg.kind {
                ArenaCallArgKind::Named { name, .. } => {
                    named_seen = true;
                    name
                }
                ArenaCallArgKind::Positional(_) if !named_seen => {
                    let Some((name, _)) = declared.get(positional) else {
                        self.error(
                            call_arg_span_arena(arena, &arg.kind),
                            &format!(
                                "too many positional constructor arguments: the schema declares {} field(s)",
                                declared.len()
                            ),
                            DiagnosticCode::CheckRecordConstructor,
                        );
                        self.check_call_arg_arena(arena, source, &arg.kind, None);
                        continue;
                    };
                    positional += 1;
                    *name
                }
                ArenaCallArgKind::Positional(_) => {
                    self.error(
                        call_arg_span_arena(arena, &arg.kind),
                        "positional constructor arguments must come before named ones",
                        DiagnosticCode::CheckRecordConstructor,
                    );
                    self.check_call_arg_arena(arena, source, &arg.kind, None);
                    continue;
                }
                ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. } => {
                    self.error(
                        call_arg_span_arena(arena, &arg.kind),
                        "record constructors take positional or named fields, not list splices",
                        DiagnosticCode::CheckRecordConstructor,
                    );
                    self.check_call_arg_arena(arena, source, &arg.kind, None);
                    continue;
                }
            };
            arg_fields.push(name);
            if !supplied.insert(name) {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    "duplicate constructor field",
                    DiagnosticCode::CheckRecordConstructor,
                );
            }
            let field_type = fields.get(&name);
            if field_type.is_none() {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    "unknown constructor field",
                    DiagnosticCode::CheckRecordConstructor,
                );
            }
            let context = schema
                .and_then(|schema| {
                    schema
                        .children
                        .get(&crate::sema::constants::SchemaComponent::Field(name))
                })
                .cloned();
            let previous = std::mem::replace(&mut self.expected_schema, context);
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, field_type);
            self.expected_schema = previous;
            if let Some(field_type) = field_type {
                self.expect_type(field_type, &actual, call_arg_span_arena(arena, &arg.kind));
            }
        }
        for name in fields.keys() {
            if !supplied.contains(name) && !defaults.contains_key(name) {
                self.error(
                    span,
                    &format!("missing required constructor field `{name}`"),
                    DiagnosticCode::CheckRecordConstructor,
                );
            }
        }
        if let Some((left, right)) =
            self.record_constructors
                .positional_conflict(&arena.arena, definition, positional)
        {
            self.diagnostics.push(
                Diagnostic::error(format!(
                    "fields `{left}` and `{right}` can hold the same value, so they must be passed by name"
                ))
                .with_code(DiagnosticCode::CheckRecordConstructor)
                .with_label(Label::primary(span, "positional arguments could be swapped unnoticed"))
                .with_note(format!(
                    "write `{left}: ...` and `{right}: ...`; positional constructor fields must have types no single value fits both of"
                )),
            );
        }
        if arg_fields.len() == args.len() {
            self.record_constructor_fields.insert(span, arg_fields);
        } else {
            self.record_constructor_fields.remove(&span);
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
            return self.check_tag_constructor_args_arena(
                arena,
                source,
                name,
                info.type_name,
                &info.field_types,
                args,
                span,
            );
        }
        match name {
            "Ok" => {
                let expected = expected_context.and_then(Type::result_ok);
                let schema = self
                    .expected_schema
                    .as_ref()
                    .and_then(|schema| {
                        schema
                            .children
                            .get(&crate::sema::constants::SchemaComponent::Success)
                    })
                    .cloned();
                let previous = std::mem::replace(&mut self.expected_schema, schema);
                let ty = args.first().map_or(Type::Unit, |arg| {
                    self.check_call_arg_arena(arena, source, &arg.kind, expected)
                });
                self.expected_schema = previous;
                let error = match expected_context {
                    Some(Type::Result(_, error)) => error.as_ref().clone(),
                    _ => Type::Error,
                };
                Type::Result(Box::new(ty), Box::new(error))
            }
            "Err" => {
                use crate::sema::arguments::{bind_err_arguments, expand_named_arguments};
                let expected = match expected_context {
                    Some(Type::Result(_, error)) => Some(error.as_ref()),
                    _ => None,
                };
                let previous = self.expected_schema.take();
                let types = args.iter().map(|arg| {
                    let outer = !matches!(arg.kind, ArenaCallArgKind::Named { name, .. } if name == "cause");
                    self.check_call_arg_arena(arena, source, &arg.kind, if outer { expected } else { None })
                }).collect::<Vec<_>>();
                self.expected_schema = previous;
                let expanded = expand_named_arguments(arena, args, |expr| {
                    args.iter().zip(&types).find_map(|(arg, ty)| {
                        let value = match arg.kind {
                            ArenaCallArgKind::Positional(value)
                            | ArenaCallArgKind::Named { value, .. } => value,
                            _ => return None,
                        };
                        (value == expr).then(|| ty.clone())
                    })
                })
                .expect("named spreads are expanded before constructor checking");
                let binding = match bind_err_arguments(&expanded).inspect(|binding| {
                    self.argument_bindings.insert(
                        span,
                        super::CheckedArguments {
                            callable_entry: None,
                            argument_slots: binding.argument_slots.clone(),
                        },
                    );
                }) {
                    Ok(binding) => binding,
                    Err(error) => {
                        self.error(
                            error.span,
                            &error.message,
                            DiagnosticCode::CheckErrArguments,
                        );
                        return Type::Invalid;
                    }
                };
                let mut outer = Type::Error;
                let has_cause = binding.argument_slots.contains(&1);
                for (arg, slot) in expanded.iter().zip(binding.argument_slots) {
                    if slot == 0 {
                        outer = arg.ty.clone();
                    }
                    if slot == 1 || has_cause {
                        self.expect_type(&Type::Error, &arg.ty, arg.span);
                    }
                }
                Type::Result(Box::new(Type::Unknown), Box::new(outer))
            }
            "Error" => {
                self.error(
                    span,
                    "`Error(kind: ...)` was removed; construct a declared error variant such as `FsError.NotFound(...)`",
                    DiagnosticCode::CheckErrorRemoved,
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
                        DiagnosticCode::CheckMigrationError,
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
            "range" => {
                if args.len() != 1 && args.len() != 2 {
                    self.error(
                        span,
                        "range expects one or two integer bounds",
                        DiagnosticCode::CheckArity,
                    );
                }
                for arg in args {
                    if matches!(arg.kind, ArenaCallArgKind::Named { .. }) {
                        self.error(
                            call_arg_span_arena(arena, &arg.kind),
                            "range bounds must be positional arguments",
                            DiagnosticCode::CheckNamedArg,
                        );
                    }
                    let actual =
                        self.check_call_arg_arena(arena, source, &arg.kind, Some(&Type::Int));
                    self.expect_type(&Type::Int, &actual, call_arg_span_arena(arena, &arg.kind));
                }
                Type::Stream(Box::new(Type::Int))
            }
            _ => {
                self.record_effect_contract(&None, name);
                self.report_unresolved_call(arena, source, name, args, span);
                Type::Unknown
            }
        }
    }

    /// Python habits: `print(x)` and `len(xs)`. `print` is a command, and
    /// sizes are methods, so both get the XSH spelling.
    fn report_unresolved_call(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        name: &str,
        args: &[ArenaCallArg],
        span: Span,
    ) {
        let mut diagnostic = Diagnostic::error(format!("unresolved pure function call `{name}`"))
            .with_code(DiagnosticCode::CheckUnresolvedCall)
            .with_label(Label::primary(span, "unresolved pure function call"));
        let argument = match args {
            [arg] if !matches!(arg.kind, ArenaCallArgKind::Named { .. }) => {
                let arg_span = call_arg_span_arena(arena, &arg.kind);
                let ty = self.check_call_arg_arena(arena, source, &arg.kind, None);
                source
                    .get(arg_span.range())
                    .map(|text| (text.to_string(), ty))
            }
            _ => None,
        };
        match (name, argument) {
            ("print" | "eprint", Some((text, _))) => {
                diagnostic = diagnostic
                    .with_note(format!(
                        "`{name}` is a command, not a function; its arguments are words"
                    ))
                    .with_fix_hint(super::FixHint::replacement(
                        span,
                        "pass the value as a typed command argument",
                        format!("{name} ({text})"),
                    ));
            }
            ("print" | "eprint", None) => {
                diagnostic = diagnostic.with_note(format!(
                    "`{name}` is a command, not a function: write `{name} WORD ...`"
                ));
            }
            ("len", Some((text, ty))) => {
                let method = if ty == Type::Str {
                    "count_chars"
                } else {
                    "len"
                };
                let receiver = if text
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
                {
                    text
                } else {
                    format!("({text})")
                };
                diagnostic = diagnostic
                    .with_note(format!("sizes are methods in XSH: `{receiver}.{method}()`"))
                    .with_fix_hint(super::FixHint::replacement(
                        span,
                        format!("call `.{method}()`"),
                        format!("{receiver}.{method}()"),
                    ));
            }
            _ => {
                if let Some(nearby) = self.nearby_visible_name(name) {
                    diagnostic = diagnostic.with_note(format!("did you mean `{nearby}`?"));
                }
            }
        }
        self.diagnostics.push(diagnostic);
    }

    /// A misspelled standard API names the nearest function of its module.
    pub(super) fn report_unknown_module_api(&mut self, module: &str, name: &str, span: Span) {
        let mut diagnostic = Diagnostic::error(format!("unknown module API `{module}.{name}`"))
            .with_code(DiagnosticCode::CheckUnknownModuleApi)
            .with_label(Label::primary(span, "unknown module API"));
        let nearby = api_spec().module(module).and_then(|signature| {
            super::method::nearest_name(
                name,
                signature.functions.iter().map(|function| function.name),
            )
        });
        if let Some(nearby) = nearby {
            diagnostic = diagnostic.with_note(format!("did you mean `{module}.{nearby}`?"));
        }
        self.diagnostics.push(diagnostic);
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
                DiagnosticCode::CheckArity,
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
                    DiagnosticCode::CheckNamedArg,
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
            self.error(
                span,
                "unknown error variant",
                DiagnosticCode::CheckErrorConstructor,
            );
            for arg in args {
                self.check_call_arg_arena(arena, source, &arg.kind, None);
            }
            return Type::Error;
        };

        let mut seen = FxHashSet::default();
        let field_names: Vec<_> = info.fields.keys().copied().collect();
        let mut positional_index = 0usize;
        // The binding lowering consumes; it is published only for a call in
        // which every argument fills one distinct field.
        let mut arg_fields = Vec::with_capacity(args.len());
        let mut well_formed = true;
        let mut named_seen = false;
        // Positional arguments before any named one, and those after.
        let mut leading = Vec::new();
        let mut trailing = Vec::new();
        for arg in args {
            well_formed &= matches!(
                arg.kind,
                ArenaCallArgKind::Named { .. } | ArenaCallArgKind::Positional(_)
            );
            let (name, expected) = match &arg.kind {
                // The message of a variant without a payload has one spelling,
                // so a declaration that later gains a payload cannot silently
                // rebind a `message:` argument.
                ArenaCallArgKind::Named { .. } if info.implicit_message => {
                    self.error(
                        call_arg_span_arena(arena, &arg.kind),
                        &format!(
                            "variant `{variant}` declares no payload; pass its message positionally: `{variant}(\"...\")`"
                        ),
                        DiagnosticCode::CheckErrorConstructor,
                    );
                    self.check_call_arg_arena(arena, source, &arg.kind, None);
                    well_formed = false;
                    continue;
                }
                ArenaCallArgKind::Named { name, .. } => {
                    named_seen = true;
                    let Some(expected) = info.fields.get(name) else {
                        self.error(
                            call_arg_span_arena(arena, &arg.kind),
                            "unknown error payload field",
                            DiagnosticCode::CheckErrorConstructor,
                        );
                        self.check_call_arg_arena(arena, source, &arg.kind, None);
                        well_formed = false;
                        continue;
                    };
                    (*name, expected.clone())
                }
                ArenaCallArgKind::Positional(_) => {
                    let Some(name) = field_names.get(positional_index).copied() else {
                        self.error(
                            call_arg_span_arena(arena, &arg.kind),
                            "too many error constructor arguments",
                            DiagnosticCode::CheckArity,
                        );
                        self.check_call_arg_arena(arena, source, &arg.kind, None);
                        well_formed = false;
                        continue;
                    };
                    positional_index += 1;
                    let argument = (call_arg_span_arena(arena, &arg.kind), name);
                    if named_seen {
                        trailing.push(argument);
                    } else {
                        leading.push(argument);
                    }
                    let expected = info.fields.get(&name).cloned().unwrap_or(Type::Unknown);
                    (name, expected)
                }
                ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. } => {
                    self.error(
                        call_arg_span_arena(arena, &arg.kind),
                        "error constructors do not accept argument splices",
                        DiagnosticCode::CheckSpliceTarget,
                    );
                    self.check_call_arg_arena(arena, source, &arg.kind, None);
                    continue;
                }
            };
            if !seen.insert(name) {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    "duplicate error payload field",
                    DiagnosticCode::CheckErrorConstructor,
                );
                well_formed = false;
            }
            arg_fields.push(name);
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, Some(&expected));
            self.expect_type(&expected, &actual, call_arg_span_arena(arena, &arg.kind));
        }
        let default_message = info.implicit_message && seen.is_empty();
        well_formed &= default_message || field_names.iter().all(|name| seen.contains(name));
        if well_formed {
            self.error_constructors.insert(
                span,
                super::CheckedErrorConstructor {
                    fields: arg_fields,
                    default_message,
                },
            );
        } else {
            self.error_constructors.remove(&span);
        }
        // The rules record constructors follow: a swapped pair of positional
        // arguments must be a type error, and positional arguments lead.
        if let Some((left, right)) = info.fields.positional_conflict(leading.len()) {
            let mut diagnostic = Diagnostic::warning(format!(
                "fields `{left}` and `{right}` can hold the same value, so they must be passed by name"
            ))
            .with_code(DiagnosticCode::CheckPositionalErrorArguments)
            .with_label(Label::primary(span, "positional arguments could be swapped unnoticed"))
            .with_note(format!(
                "write `{left}: ...` and `{right}: ...`; positional payload fields must have types no single value fits both of"
            ));
            for (argument, name) in &leading {
                if let Some(fix) = named_argument_fix(source, *argument, *name) {
                    diagnostic = diagnostic.with_fix_hint(fix);
                }
            }
            self.diagnostics.push(diagnostic);
        }
        for (argument, name) in &trailing {
            let mut diagnostic = Diagnostic::warning(
                "positional error constructor arguments must come before named ones",
            )
            .with_code(DiagnosticCode::CheckPositionalErrorArguments)
            .with_label(Label::primary(*argument, format!("this fills `{name}`")));
            if let Some(fix) = named_argument_fix(source, *argument, *name) {
                diagnostic = diagnostic.with_fix_hint(fix);
            }
            self.diagnostics.push(diagnostic);
        }
        self.message_payload_constructors.remove(&span);
        let message = Name::intern("message");
        if !info.implicit_message
            && info.fields.len() == 1
            && info.fields.get(&message) == Some(&Type::Str)
            && let [arg] = args
            && !xsh_registry::errors::builtin_error_families()
                .iter()
                .any(|builtin| family == builtin.name)
        {
            let named_message = match &arg.kind {
                ArenaCallArgKind::Named { name, value, .. } if *name == message => Some((
                    call_arg_span_arena(arena, &arg.kind),
                    arena.arena.expr(*value).span,
                )),
                _ => None,
            };
            if named_message.is_some() || matches!(arg.kind, ArenaCallArgKind::Positional(_)) {
                self.message_payload_constructors.insert(
                    span,
                    super::MessagePayloadConstructor {
                        family,
                        variant,
                        named_message,
                    },
                );
            }
        }
        // An omitted implicit message defaults to the family and variant name.
        if !info.implicit_message {
            for name in info.fields.keys() {
                if !seen.contains(name) {
                    self.error(
                        span,
                        "missing error payload field",
                        DiagnosticCode::CheckErrorConstructor,
                    );
                }
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
        expected_context: Option<&Type>,
    ) -> Type {
        let Some(module_sig) = api_spec().module(module) else {
            self.error(span, "unknown module", DiagnosticCode::CheckUnknownModule);
            return Type::Unknown;
        };
        if module == "env" && name == "get_path" {
            self.error(
                span,
                "`env.get_path` is not supported; use `env.Path.NAME`",
                DiagnosticCode::CheckUnsupportedApi,
            );
            return Type::Result(Box::new(Type::Path), Box::new(Type::Error));
        }
        if module == "path" && name == "display" {
            self.error(
                span,
                "`path.display` is not supported; use `path_value.display()`",
                DiagnosticCode::CheckUnsupportedApi,
            );
            return Type::Str;
        }
        if (module == "test" && matches!(name, "contains" | "not_contains"))
            || (module == "set" && name == "has")
        {
            self.membership_migration_spans.insert(span);
            self.standard_call_spans
                .insert(span, (module.to_string(), name.to_string()));
            self.error(
                span,
                "standard membership API was removed; use `in` or `not in`",
                DiagnosticCode::CheckRemovedMembership,
            );
            for arg in args {
                self.check_call_arg_arena(arena, source, &arg.kind, None);
            }
            return if module == "test" {
                Type::Result(
                    Box::new(Type::Unit),
                    Box::new(Type::ErrorFamily(Name::intern("AssertionError"))),
                )
            } else {
                Type::Bool
            };
        }
        self.standard_call_spans
            .insert(span, (module.to_string(), name.to_string()));
        let migration = if module == "fs" {
            xsh_registry::signature::legacy_fs_root_method(name).map(|method| {
                self.error(
                    span,
                    &format!("`fs.{name}` was removed; use an FsRoot receiver's `{method}` method"),
                    DiagnosticCode::CheckUnsupportedApi,
                );
                api_spec()
                    .method_overloads(MethodReceiver::FsRoot, method)
                    .expect("root receiver registry")
                    .iter()
                    .map(|method| {
                        let mut sig = method.sig.clone();
                        sig.params.insert(
                            0,
                            crate::modules::signature::ParamSig {
                                name: "root",
                                ty: Type::FsRoot,
                                defaulted: false,
                            },
                        );
                        sig
                    })
                    .collect::<Vec<_>>()
            })
        } else {
            None
        };
        let Some(overloads) = module_sig.function_overloads(name).or(migration.as_deref()) else {
            self.report_unknown_module_api(module, name, span);
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
        let registered = module_sig.function_overloads(name).and_then(|registered| {
            registered
                .iter()
                .find(|candidate| std::ptr::eq(*candidate, sig))
        });
        let mut instance = crate::sema::builtin_templates::BuiltinInstantiation::new(
            sig,
            None,
            None,
            &mut self.type_constraints,
            span,
        )
        .expect("a module signature has no receiver constraint");
        let descriptor_result = matches!(
            sig.semantic_rule,
            crate::modules::signature::SemanticRule::CliDescriptor
                | crate::modules::signature::SemanticRule::CliCommands
        );
        // Descriptor field facts determine the result contract before a typed
        // destination can constrain it. Other builtins still ground templates
        // from their expected result before checking arguments.
        if !descriptor_result
            && let Some(expected) = expected_context
            && let Err(conflict) =
                instance.constrain_result(expected, &mut self.type_constraints, span)
        {
            self.expect_type(&conflict.expected, &conflict.actual, span);
        }
        let sig = &instance.signature;
        if self.in_pure && !sig.pure {
            self.error(
                span,
                "effectful module API is not allowed in pure functions",
                DiagnosticCode::CheckPureEffect,
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
        instance.resolve(&self.type_constraints);
        if let Some(registered) = registered {
            self.publish_api_call(arena, args, span, None, registered, &instance.signature);
        }
        let sig = &instance.signature;
        let return_ty = if sig.semantic_rule == crate::modules::signature::SemanticRule::CliCommands
        {
            let parameters = crate::sema::builtin_templates::callable_parameters(sig);
            let plan = crate::sema::arguments::expand_named_arguments(arena, args, |expression| {
                self.expr_types
                    .get(&arena.arena.expr(expression).span)
                    .cloned()
            })
            .ok()
            .and_then(|expanded| {
                crate::sema::arguments::bind_static_arguments(&parameters, &expanded)
                    .ok()
                    .and_then(|binding| {
                        crate::modules::cli::command_descriptor_sources(
                            &expanded,
                            &binding.argument_slots,
                            &sig.params
                                .iter()
                                .map(|parameter| crate::symbol::Name::intern(parameter.name))
                                .collect::<Vec<_>>(),
                        )
                    })
            })
            .and_then(|(commands, fallback)| {
                self.prepared_constants
                    .cli_commands_plan(&arena.arena, commands, fallback)
            });
            match plan {
                Some(Ok(plan)) => plan.return_type(false),
                Some(Err(error)) => {
                    self.error(
                        error.span.unwrap_or(span),
                        &error.message,
                        DiagnosticCode::CheckCliDescriptor,
                    );
                    sig.return_ty.clone()
                }
                None => sig.return_ty.clone(),
            }
        } else if sig.semantic_rule == crate::modules::signature::SemanticRule::CliDescriptor {
            self.infer_cli_descriptor_return_arena(arena, args, sig.op)
                .unwrap_or_else(|| sig.return_ty.clone())
        } else {
            sig.return_ty.clone()
        };
        if descriptor_result && let Some(expected) = expected_context {
            instance.signature.return_ty = return_ty.clone();
            if let Err(conflict) =
                instance.constrain_result(expected, &mut self.type_constraints, span)
            {
                self.expect_type(&conflict.expected, &conflict.actual, span);
            }
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
        match self
            .prepared_constants
            .cli_descriptor_plan(&arena.arena, schema, applet)?
        {
            Ok(plan) => Some(plan.return_type(op == xsh_registry::RuntimeOp::CliParseFull)),
            Err(error) => {
                let span = error.span.unwrap_or(arena.arena.expr(schema).span);
                self.error(span, &error.message, DiagnosticCode::CheckCliDescriptor);
                None
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
            "accept",
        ];
        if !(2..=names.len()).contains(&args.len()) {
            self.error(
                span,
                "incorrect standard API arity",
                DiagnosticCode::CheckArity,
            );
        }
        let mut slots: [Option<&ArenaCallArgKind>; 15] = [None; 15];
        let mut next_positional = 0;
        for arg in args {
            match &arg.kind {
                ArenaCallArgKind::Named { name, .. } => {
                    let Some(index) = names.iter().position(|expected| *expected == name.as_str())
                    else {
                        self.error(
                            call_arg_span_arena(arena, &arg.kind),
                            "unexpected named parameter",
                            DiagnosticCode::CheckNamedArg,
                        );
                        continue;
                    };
                    if slots[index].is_some() {
                        self.error(
                            call_arg_span_arena(arena, &arg.kind),
                            "duplicate named parameter",
                            DiagnosticCode::CheckNamedArg,
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
                            DiagnosticCode::CheckArity,
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
                        DiagnosticCode::CheckSpliceTarget,
                    );
                }
            }
        }

        if slots[0].is_none() || slots[1].is_none() {
            self.error(
                span,
                "incorrect standard API arity",
                DiagnosticCode::CheckArity,
            );
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
                DiagnosticCode::CheckTypeMismatch,
            );
        }
        self.check_process_command_argv_argv_arena(arena, source, slots[1], span);
        let expected = [
            Type::Path,
            Type::ErasedRecord,
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
            Type::List(Box::new(Type::Int)),
        ];
        for (offset, expected) in expected.iter().enumerate() {
            if offset + 2 == 4 {
                let actual = self.check_optional_api_arg_arena(arena, source, slots[4], None);
                if let Some(arg) = slots[4]
                    && actual != Type::Bytes
                {
                    self.expect_type(&Type::Path, &actual, call_arg_span_arena(arena, arg));
                }
            } else {
                self.check_optional_api_arg_arena(arena, source, slots[offset + 2], Some(expected));
            }
        }
        if let Some(arg) = slots[13] {
            let expr_id = call_arg_expr_id_arena(arg);
            self.check_static_positive_call_int_arena(arena, expr_id, "cpu_max must be positive");
        }
        if let Some(arg) = slots[14] {
            self.check_static_accepted_exit_codes(arena, call_arg_expr_id_arena(arg));
        }
        // The overloads differ only in parameter types, so either binds the
        // entries lowering consumes by parameter name.
        if let Some(sig) = api_spec()
            .module("process")
            .and_then(|module| module.function_overloads("command_argv"))
            .and_then(|overloads| overloads.first())
        {
            self.publish_api_call(arena, args, span, None, sig, sig);
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
            self.error(
                span,
                "incorrect standard API arity",
                DiagnosticCode::CheckArity,
            );
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
                    .with_code(DiagnosticCode::CheckProcessArgvEmpty)
                    .with_label(Label::primary(expr.span, "argv is empty"))
                    .with_note("include the child program name as the first argv item"),
                );
                return;
            }
            for item in arena.arena.list_elements(items) {
                let actual = self.check_expr_arena(arena, source, item.value, None);
                let item_ty = if let Some(splice_span) = item.splice_span {
                    match actual {
                        Type::List(ty) => *ty,
                        _ => {
                            self.error(
                                arena.arena.span(splice_span),
                                "list literal splice requires List",
                                DiagnosticCode::CheckListSpliceType,
                            );
                            Type::Unknown
                        }
                    }
                } else {
                    actual
                };
                if !process_command_argv_item_type_is_valid(&item_ty) {
                    self.error(
                        arena.arena.expr(item.value).span,
                        "process.command_argv argv items must be Str or Path",
                        DiagnosticCode::CheckTypeMismatch,
                    );
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
                DiagnosticCode::CheckTypeMismatch,
            ),
            _ => self.error(
                call_arg_span_arena(arena, arg),
                "process.command_argv argv must be a List",
                DiagnosticCode::CheckTypeMismatch,
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
                self.error(expr.span, message, DiagnosticCode::CheckNamedArg);
            }
            ArenaExprKind::Unary {
                op: UnaryOp::Neg,
                expr: inner,
            } if matches!(arena.arena.expr(*inner).kind, ArenaExprKind::Int(_)) => {
                self.error(expr.span, message, DiagnosticCode::CheckNamedArg);
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
                DiagnosticCode::CheckArity,
            );
            return;
        }
        // Without one path and one other argument, report against the written
        // order so the diagnostics below still name the misplaced argument.
        let [path_index, checksum_index] =
            crate::sema::arguments::bind_hash_verify_file_arguments(args).unwrap_or([0, 1]);
        let (path_arg, checksum_arg) = (&args[path_index], &args[checksum_index]);
        let path_ty = self.check_call_arg_arena(arena, source, &path_arg.kind, Some(&Type::Path));
        let path_expr_id = call_arg_expr_id_arena(&path_arg.kind);
        let path_kind = arena.arena.expr(path_expr_id).kind;
        if !is_path_like_arena_expr(&path_kind, &path_ty) {
            self.expect_type(
                &Type::Path,
                &path_ty,
                call_arg_span_arena(arena, &path_arg.kind),
            );
        }
        let ArenaCallArgKind::Named { name, .. } = &checksum_arg.kind else {
            self.error(
                call_arg_span_arena(arena, &checksum_arg.kind),
                "checksum argument must be named",
                DiagnosticCode::CheckNamedArg,
            );
            self.check_call_arg_arena(arena, source, &checksum_arg.kind, Some(&Type::Str));
            return;
        };
        if !matches!(name.as_str().as_str(), "md5" | "sha1" | "sha256" | "sha512") {
            self.error(
                call_arg_span_arena(arena, &checksum_arg.kind),
                "unsupported checksum algorithm",
                DiagnosticCode::CheckNamedArg,
            );
        }
        let actual = self.check_call_arg_arena(arena, source, &checksum_arg.kind, Some(&Type::Str));
        self.expect_type(
            &Type::Str,
            &actual,
            call_arg_span_arena(arena, &checksum_arg.kind),
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
                    self.error(
                        span,
                        "incorrect standard API arity",
                        DiagnosticCode::CheckArity,
                    );
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
                    self.error(
                        span,
                        "incorrect standard API arity",
                        DiagnosticCode::CheckArity,
                    );
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
                    self.error(
                        span,
                        "incorrect standard API arity",
                        DiagnosticCode::CheckArity,
                    );
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
                    self.error(
                        span,
                        "incorrect standard API arity",
                        DiagnosticCode::CheckArity,
                    );
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
                // Serialization consumes a concrete list without changing its
                // element domain. Dynamic inputs must still establish a list.
                if !matches!(value_ty, Type::List(_)) {
                    self.expect_type(
                        &expected,
                        &value_ty,
                        call_arg_span_arena(arena, &args[1].kind),
                    );
                }
                self.expect_json_compatible(&value_ty, call_arg_span_arena(arena, &args[1].kind));
            }
            "set" => {
                if args.len() != 3 {
                    self.error(
                        span,
                        "incorrect standard API arity",
                        DiagnosticCode::CheckArity,
                    );
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
                DiagnosticCode::CheckNamedArg,
            );
        }
    }
}

#[cfg(test)]
mod error_constructor_tests {
    use crate::sema::check::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    /// The published binding of each error constructor call, in source order.
    fn bindings(source: &str) -> Vec<(Vec<String>, bool)> {
        let program = Parser::parse_source_arena_only(SourceId::new(0), source).arena;
        program.symbol_owner().with_current(|| {
            Checker::check_arena(&program, source)
                .error_constructors
                .values()
                .map(|constructor| {
                    (
                        constructor.fields.iter().map(ToString::to_string).collect(),
                        constructor.default_message,
                    )
                })
                .collect()
        })
    }

    #[test]
    fn checker_publishes_the_field_each_error_argument_fills() {
        let published = bindings(
            "error E = Triple(zulu: Int, mike: Str, alpha: Bool) | Usage\n\
             let a = E.Triple(1, alpha: true, mike: \"m\")\n\
             let b = E.Usage()\n\
             let c = E.Usage(\"x\")\n",
        );
        let names = |fields: &[&str]| fields.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_eq!(
            published,
            vec![
                (names(&["zulu", "alpha", "mike"]), false),
                (names(&[]), true),
                (names(&["message"]), false),
            ]
        );
    }

    #[test]
    fn malformed_error_constructors_publish_no_binding() {
        for call in [
            "E.Triple(1)",
            "E.Triple(1, \"m\", true, 4)",
            "E.Triple(1, zulu: 2, mike: \"m\", alpha: true)",
            "E.Triple(1, \"m\", other: true)",
            "E.Usage(message: \"x\")",
        ] {
            let source = format!(
                "error E = Triple(zulu: Int, mike: Str, alpha: Bool) | Usage\nlet a = {call}\n"
            );
            assert!(bindings(&source).is_empty(), "{call} published a binding");
        }
    }
}
