use super::{CallableAlias, Checker, FunctionParamSig, Name, Span, Type};
use crate::diagnostic::DiagnosticCode;
use crate::sema::types::TypedCallable;
use crate::syntax::arena::{ArenaCallArg, ArenaCallArgKind, ArenaExprKind, ArenaProgram, ExprId, StmtId};
use std::sync::Arc;

/// The callable type a slot expects, directly or as the present value of an
/// optional.
fn expected_typed_callable(expected: Option<&Type>) -> Option<Arc<TypedCallable>> {
    match expected? {
        Type::Callable(target) => Some(target.clone()),
        Type::Optional(inner) => match inner.as_ref() {
            Type::Callable(target) => Some(target.clone()),
            _ => None,
        },
        _ => None,
    }
}

impl Checker {
    pub(super) fn name_typed_callable(
        &mut self,
        arena: &ArenaProgram,
        id: ExprId,
        actual: Type,
        expected: Option<&Type>,
    ) -> Type {
        if !matches!(actual, Type::Proc | Type::Pure) {
            return actual;
        }
        let Some(target) = expected_typed_callable(expected) else {
            if expected.is_some_and(|expected| matches!(self.type_constraints.resolve(expected), Ok(Type::Inference(_)))) {
                return self.materialize_callable_argument(arena, id, actual);
            }
            return actual;
        };
        let span = arena.arena.expr(id).span;
        let named = self.resolve_callable_alias_target(arena, id);
        self.fit_named_callable(named, &actual, target, span)
    }

    fn materialize_callable_argument(&mut self, arena: &ArenaProgram, id: ExprId, actual: Type) -> Type {
        let Some(named) = self.resolve_callable_alias_target(arena, id) else { return actual; };
        let span = arena.arena.expr(id).span;
        let contract = self.function_call_contract(&named.signature, span);
        let effects = match contract.effects {
            super::InvocationEffects::Checked(effects) => effects,
            super::InvocationEffects::Pending(_) => {
                self.error(span, "callable value has no settled declaration effect contract", DiagnosticCode::CheckLocalInference);
                return Type::Invalid;
            }
        };
        if let Some(instance) = contract.instantiation.clone() { self.record_pending_call_instantiation(span, instance); }
        let callable = Type::Callable(Arc::new(TypedCallable {
            pure: named.pure,
            sig: super::CallableType {
                params: contract.params.into_iter().map(|parameter| super::CallableParamType {
                    name: parameter.name, ty: parameter.ty, defaulted: parameter.defaulted, rest: parameter.rest,
                }).collect(),
                return_ty: Box::new(contract.return_ty),
                effects,
            },
        }));
        self.record_selected_function_value(span, &named.signature);
        self.record_expr_type(span, callable.clone());
        callable
    }

    pub(super) fn check_inferred_local_callable_call(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        callee: ExprId,
        args: &[ArenaCallArg],
        span: Span,
        checked_argument: Option<super::args::CheckedCallArgument<'_>>,
    ) -> Option<Type> {
        if !matches!(arena.arena.expr(callee).kind, ArenaExprKind::Ident(_)) { return None; }
        let super::ValueBindingLookup::Found(binding) = self.lookup_value_binding_for_expr(arena, callee) else { return None; };
        let ty = binding.ty.clone();
        if !matches!(self.type_constraints.resolve(&ty), Ok(Type::Inference(_))) { return None; }
        let ty = self.check_expr_arena(arena, source, callee, None);
        self.check_inferred_callable_application(arena, source, &ty, args, span, checked_argument)
    }

    pub(super) fn check_inferred_callable_application(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        callee: &Type,
        args: &[ArenaCallArg],
        span: Span,
        checked_argument: Option<super::args::CheckedCallArgument<'_>>,
    ) -> Option<Type> {
        use crate::sema::arguments::{ArgumentValueSource, StaticArgumentKind};
        use crate::sema::callable_application::{CallableApplication, CallableApplicationArgument, CallableKindRequirement};
        use crate::sema::constraints::CallableApplicationScope;
        use crate::sema::effects::{EffectContract, EffectSet};
        if !matches!(self.type_constraints.resolve(callee), Ok(Type::Inference(_))) { return None; }
        let arguments = if let Some(argument) = checked_argument {
            assert!(args.is_empty(), "a checked argument replaces source arguments");
            vec![CallableApplicationArgument { kind: StaticArgumentKind::Positional, ty: argument.ty.clone(), span: argument.span }]
        } else {
            let mut checked = std::collections::BTreeMap::new();
            for argument in args {
                let value = match argument.kind {
                    ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. }
                    | ArenaCallArgKind::Splice { value, .. } | ArenaCallArgKind::NamedSpread { value, .. } => value,
                };
                let mut ty = self.check_expr_arena(arena, source, value, None);
                if matches!(ty, Type::Pure | Type::Proc) { ty = self.materialize_callable_argument(arena, value, ty); }
                checked.insert(value, ty);
            }
            match crate::sema::arguments::expand_named_arguments(arena, args, |value| Some(checked.get(&value).expect("every application source argument is checked").clone())) {
                Ok(arguments) => arguments.into_iter().map(|argument| CallableApplicationArgument {
                    kind: match (argument.name, argument.value) {
                        (Some(name), _) => StaticArgumentKind::Named(name),
                        (None, ArgumentValueSource::PositionalSplice(_)) => StaticArgumentKind::Splice,
                        _ => StaticArgumentKind::Positional,
                    }, ty: argument.ty, span: argument.span,
                }).collect(),
                Err(error) => {
                    self.error(error.span, &error.message, DiagnosticCode::CheckCallSplice);
                    return Some(Type::Invalid);
                }
            }
        };
        let (scope, level) = match self.stream_origin_owner {
            Some(owner) => {
                let Some(declaration) = self.declaration_inference.pending(super::FunctionDeclarationId::new(owner)) else {
                    self.error(span, "callable application has no declaration inference scope", DiagnosticCode::CheckLocalInference);
                    return Some(Type::Invalid);
                };
                (CallableApplicationScope::Declaration(owner), declaration.solver_level())
            }
            None => (CallableApplicationScope::Invocation, 0),
        };
        let previous = self.type_constraints.source_callable_application(scope, span).cloned();
        let result = previous.as_ref().map(|application| application.result.clone()).unwrap_or_else(|| self.type_constraints.fresh_at_level(span, level));
        let effects = previous.as_ref().map(|application| application.effects.clone()).unwrap_or_else(|| {
            if self.boundary.in_pure { EffectContract::finite(EffectSet::default()) }
            else { EffectContract::variable(self.type_constraints.fresh_effect_at_level(span, level)) }
        });
        let application = CallableApplication {
            callee: callee.clone(), arguments: arguments.into(), result: result.clone(), effects,
            proc_returns: previous.map(|application| application.proc_returns).unwrap_or_else(|| Arc::from([])), kind: if self.boundary.in_pure { CallableKindRequirement::Pure } else { CallableKindRequirement::Immediate }, span,
        };
        let invocation_effects = application.effects.clone();
        if let Err(error) = self.type_constraints.register_source_callable_application(scope, application) {
            self.error(span, &format!("callable application requirement failed: {error}"), DiagnosticCode::CheckCallableMismatch);
            return Some(Type::Invalid);
        }
        if !self.boundary.in_pure {
            self.check_callable_invocation_contract(&crate::sema::types::CallableEffects::from_contract(invocation_effects), "inferred callable application", span);
            self.invalidate_mutable_narrowings();
        }
        Some(result)
    }

    pub(super) fn tail_typed_callable(
        &mut self,
        name: Name,
        statement: StmtId,
        span: Span,
        expected: Option<&Type>,
    ) -> Option<Type> {
        let target = expected_typed_callable(expected)?;
        let binding = match self.lookup_value_binding_for_tail(name, statement) {
            super::ValueBindingLookup::Found(binding) => Some(binding),
            super::ValueBindingLookup::Missing => return None,
            super::ValueBindingLookup::NoSite => None,
        };
        let (named, actual, selected_signature) = if let Some(binding) = binding {
            let actual = self.type_constraints.resolve(&binding.ty).ok()?;
            if !matches!(actual, Type::Proc | Type::Pure) {
                return None;
            }
            (binding.callable_alias.clone(), actual, None)
        } else if let Some(declaration) = self.procs.get(&name).copied()
            && let Some(signature) = self.local_function_signature(declaration)
        {
            (
                Some(CallableAlias {
                    name,
                    signature: signature.clone(),
                    pure: false,
                }),
                Type::Proc,
                Some(signature),
            )
        } else {
            let declaration = *self.pures.get(&name)?;
            let signature = self.local_function_signature(declaration)?;
            (
                Some(CallableAlias {
                    name,
                    signature: signature.clone(),
                    pure: true,
                }),
                Type::Pure,
                Some(signature),
            )
        };
        let fitted = self.fit_named_callable(named, &actual, target, span);
        if matches!(fitted, Type::Callable(_))
            && let Some(signature) = selected_signature.as_ref()
        {
            self.record_selected_function_value(span, signature);
        }
        Some(fitted)
    }

    fn fit_named_callable(
        &mut self,
        named: Option<CallableAlias>,
        actual: &Type,
        target: Arc<TypedCallable>,
        span: Span,
    ) -> Type {
        let expected = Type::Callable(target.clone());
        let Some(named) = named else {
            self.error(
                span,
                &format!(
                    "a dynamic `{actual}` has no checked signature and cannot be used as `{expected}`; name a function or a value of that type"
                ),
                DiagnosticCode::CheckCallableMismatch,
            );
            return Type::Invalid;
        };
        let Some(signature) = super::decl::callable_type_from_function_signature(&named.signature)
        else {
            self.error(
                span,
                "a quantified function cannot be stored as a callable value",
                DiagnosticCode::CheckCallableMismatch,
            );
            return Type::Invalid;
        };
        // Effect summaries are not solved while they are being collected, and
        // assigning a callable requires no effect by itself.
        if let Some(reason) = target.mismatch(named.pure, &signature, !self.collecting_effects) {
            self.error(
                span,
                &format!("`{}` does not fit `{expected}`: {reason}", named.name),
                DiagnosticCode::CheckCallableMismatch,
            );
            return Type::Invalid;
        }
        self.expr_types.insert(span, expected.clone());
        expected
    }

    pub(super) fn typed_callable_local_callee(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        callee: ExprId,
    ) -> Option<Arc<TypedCallable>> {
        let ArenaExprKind::Ident(_) = arena.arena.expr(callee).kind else {
            return None;
        };
        let binding = match self.lookup_value_binding_for_expr(arena, callee) {
            super::ValueBindingLookup::Found(binding) => binding,
            super::ValueBindingLookup::Missing | super::ValueBindingLookup::NoSite => {
                return None;
            }
        };
        if !matches!(
            self.type_constraints.resolve(&binding.ty),
            Ok(Type::Callable(_))
        ) {
            return None;
        }
        match self.check_expr_arena(arena, source, callee, None) {
            Type::Callable(callable) => Some(callable),
            _ => None,
        }
    }

    /// Checks `callee(args)` where the callee has a callable type: every
    /// argument against the type's parameters, the type's effect bound
    /// against the caller, and the result as the type's return type. The call
    /// is published for lowering under the call expression's span.
    pub(super) fn check_typed_callable_call(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        callee: ExprId,
        args: &[ArenaCallArg],
        callable: &Arc<TypedCallable>,
        span: Span,
    ) -> Type {
        self.check_typed_callable_call_with_checked_argument(
            arena, source, callee, args, callable, span, None,
        )
    }

    pub(super) fn check_typed_callable_call_with_checked_argument(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        callee: ExprId,
        args: &[ArenaCallArg],
        callable: &Arc<TypedCallable>,
        span: Span,
        checked_argument: Option<super::args::CheckedCallArgument<'_>>,
    ) -> Type {
        let return_ty = callable.sig.return_ty.as_ref().clone();
        let kind = if callable.pure { crate::sema::types::CallableKind::Pure } else { crate::sema::types::CallableKind::Proc };
        let effects = crate::sema::types::CallableEffects::from_contract(callable.sig.effects.contract().clone().for_invocation(kind));
        let name = match arena.arena.expr(callee).kind {
            ArenaExprKind::Ident(name) | ArenaExprKind::Field { name, .. } => name.to_string(),
            _ => "callable value".to_string(),
        };
        let contract = super::FunctionCallContract {
            params: callable.sig.params.iter().map(|parameter| FunctionParamSig {
                name: parameter.name, ty: parameter.ty.clone(), schema_expectation: None,
                defaulted: parameter.defaulted, rest: parameter.rest,
            }).collect(),
            return_ty: return_ty.clone(), effects: super::InvocationEffects::Checked(effects),
            effect_declaration: None, definition_target: None, instantiation: None,
        };
        if !callable.pure {
            if self.boundary.in_pure {
                self.error(span, "effectful proc is not allowed in pure functions", DiagnosticCode::CheckPureEffect);
            } else { self.check_function_invocation_effects(&contract, &name, span); }
        }
        self.check_function_contract_arguments_arena(arena, source, args, &contract, span, checked_argument);
        if !callable.pure { self.invalidate_mutable_narrowings(); }
        self.record_invocation_propagation(&contract.effects, &return_ty, span);
        self.typed_callable_calls.insert(span, callable.clone());
        return_ty
    }

    /// Rejects a type that a runtime test would have to check a callable
    /// signature for. A handle does not carry its signature, so the test
    /// could only confirm the kind and would then vouch for the rest.
    pub(super) fn reject_typed_callable_test(&mut self, tested: &Type, span: Span) -> bool {
        if !tested.contains_typed_callable() {
            return false;
        }
        self.error(
            span,
            &format!(
                "`{tested}` cannot be tested at run time: a callable value does not carry its signature; test for `Proc` or `Pure` and call it dynamically"
            ),
            DiagnosticCode::CheckCallableType,
        );
        true
    }
}

#[cfg(test)]
mod inferred_callable_tests {
    use crate::diagnostic::Severity;
    use crate::sema::check::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    fn accepts(source: &str) {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
        assert!(errors.is_empty(), "{source}\n{errors:?}");
    }

    #[test]
    fn inferred_callable_application_source_identity_survives_constraint_probes() {
        let source = "pure apply(action, value) { action(value) }\npure number(payload: Int) -> Int { payload }\npure text(input: Str) -> Str { input }\nlet number_result: Int = apply(number, 7)\nlet text_result: Str = apply(text, \"value\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), "{:?}", checked.diagnostics);
        let declaration = checked.checked_declarations.functions.values().find(|declaration| declaration.name == "apply").unwrap();
        let scheme = declaration.scheme.as_ref().unwrap();
        let [application] = scheme.callable().requirements().applications.as_slice() else { panic!("one checked application source owns one requirement") };
        assert!(matches!(scheme.params()[0].ty, crate::sema::schemes::SchemeType::Parameter(_)));
        assert_eq!(application.callee, scheme.params()[0].ty);
        assert_eq!(application.arguments[0].ty, scheme.params()[1].ty);
        assert_eq!(application.result, *scheme.return_type());
        assert_eq!(application.arguments[0].kind, crate::sema::arguments::StaticArgumentKind::Positional);
    }

    #[test]
    fn inferred_callable_positional_uses_preserve_independent_types_and_labels() {
        accepts("pure apply(action, value) { action(value) }\npure number(payload: Int) -> Int { payload }\npure text(input: Str) -> Str { input }\nlet number_result: Int = apply(number, 7)\nlet text_result: Str = apply(text, \"value\")\n");
    }

    #[test]
    fn inferred_callable_clients_do_not_choose_the_declaration_shape() {
        accepts("pure text(input: Str) -> Str { input }\npure number(payload: Int) -> Int { payload }\nlet text_result: Str = apply(text, \"value\")\nlet number_result: Int = apply(number, 7)\npure apply(action, value) { action(value) }\n");
    }

    #[test]
    fn inferred_callable_default_calls_keep_declaration_ownership() {
        accepts("proc make() [] -> Int { 1 }\nproc choose(marker, x: Int = make()) [] { marker }\nlet integer: Int = choose(7)?\nlet text: Str = choose(\"value\")?\n");
    }

    #[test]
    fn inferred_callable_proc_uses_accept_supported_kinds_in_either_client_order() {
        for calls in [
            "let pure_result: Result[Int] = invoke(plain, 7)\nlet proc_result: Result[Int] = invoke(environmental, 8)",
            "let proc_result: Result[Int] = invoke(environmental, 8)\nlet pure_result: Result[Int] = invoke(plain, 7)",
        ] {
            accepts(&format!("pure plain(payload: Int) -> Int {{ payload }}\nproc environmental(input: Int) [env] -> Int {{ let _ = env.get(\"XSH_INFERRED_CALLBACK\"); input }}\nproc invoke(action, value) {{ action(value) }}\n{calls}\n"));
        }
    }

    #[test]
    fn inferred_callable_proc_formal_returns_keep_result_values_as_data() {
        accepts("pure identity(payload) { payload }\nproc keep(action, value) { let _ = action(value); value }\nlet integer: Int = keep(identity, 7)?\nlet nested: Result[Int] = keep(identity, Ok(7))?\n");
    }

    #[test]
    fn inferred_callable_proc_result_aliases_preserve_application_envelopes() {
        accepts("pure plain(payload: Int) -> Int { payload }\npure result(input: Int) -> Result[Int] { Ok(input) }\nproc invoke(action, value) { let returned = action(value); returned }\nlet integer: Result[Int] = invoke(plain, 7)\nlet retained: Result[Int] = invoke(result, 8)\n");
    }

    #[test]
    fn inferred_callable_returned_values_keep_the_same_constraint_binding() {
        accepts("pure relay(action) { action }\npure apply(action, value) { let selected = relay(action); selected(value) }\npure number(payload: Int) -> Int { payload }\npure text(input: Str) -> Str { input }\nlet number_result: Int = apply(number, 7)\nlet text_result: Str = apply(text, \"value\")\n");
    }

    #[test]
    fn inferred_callable_aliases_keep_the_selected_callable_relationship() {
        accepts("pure apply(action, value) { let selected = action; selected(value) }\npure number(payload: Int) -> Int { payload }\npure text(input: Str) -> Str { input }\nlet numeric = number\nlet textual = text\nlet number_result: Int = apply(numeric, 7)\nlet text_result: Str = apply(textual, \"value\")\n");
    }

    #[test]
    fn inferred_callable_forwarding_retains_the_same_qualified_application() {
        accepts("pure invoke(action, value) { action(value) }\npure forwarded(action, value) { invoke(action, value) }\npure number(payload: Int) -> Int { payload }\npure text(input: Str) -> Str { input }\nlet integer: Int = forwarded(number, 7)\nlet string: Str = forwarded(text, \"value\")\n");
    }

    #[test]
    fn inferred_callable_discarded_forwarding_keeps_full_returned_layout() {
        accepts("pure invoke(action, value) { action(value) }\npure keep(action, value) { let _ = invoke(action, value); action }\npure scaled(payload: Int, scale: Int = 2) -> Int { payload * scale }\nlet returned = keep(scaled, 7)\nlet integer: Int = returned(payload: 3, scale: 4)\n");
    }

    #[test]
    fn inferred_callable_result_patterns_constrain_the_application_result() {
        accepts("pure number(payload: Int) -> Result[Int] { Ok(payload) }\npure text(input: Str) -> Result[Str] { Ok(input) }\nproc relay(action, value) { match action(value) { Ok(result) => Ok(result); Err(problem) => Err(problem) } }\nlet integer: Result[Int] = relay(number, 7)\nlet string: Result[Str] = relay(text, \"value\")\n");
    }

    #[test]
    fn inferred_callable_result_patterns_accept_nominal_callbacks_with_the_ordinary_join() {
        accepts("error First = invalid(message: Str)\nerror Second = missing(message: Str)\npure number(payload: Int) -> Result[Int, First] { Err(First.invalid(message: \"number\")) }\npure text(input: Str) -> Result[Str, Second] { Err(Second.missing(message: \"text\")) }\nproc relay(action, value) { match action(value) { Ok(result) => Ok(result); Err(problem) => Err(problem) } }\nlet integer: Result[Int] = relay(number, 7)\nlet string: Result[Str] = relay(text, \"value\")\n");
    }

    #[test]
    fn inferred_callable_result_patterns_preserve_enum_constructor_selection() {
        accepts("enum Shape { Ok(Int), Empty }\npure read(value: Shape) -> Int { match value { Ok(payload) => payload; Empty => 0 } }\nlet result: Int = read(Ok(7))\n");
    }

    #[test]
    fn inferred_callable_result_patterns_reject_plain_callback_results() {
        let source = "pure number(payload: Int) -> Int { payload }\nproc relay(action, value) { match action(value) { Ok(result) => Ok(result); Err(problem) => Err(problem) } }\nlet integer = relay(number, 7)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), "a plain Int callback cannot satisfy a Result-pattern requirement");
    }

    #[test]
    fn inferred_callable_result_patterns_keep_the_ordinary_error_join() {
        let source = "error First = invalid(message: Str)\nproc relay(value: Result[Int, First]) { match value { Ok(result) => Ok(result); Err(problem) => Err(problem) } }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), "{:?}", checked.diagnostics);
        let declaration = checked.checked_declarations.functions.values().find(|declaration| declaration.name == "relay").unwrap();
        assert_eq!(declaration.scheme.as_ref().unwrap().return_type().closed_ground_type(), Some(crate::sema::types::Type::Result(Box::new(crate::sema::types::Type::Int), Box::new(crate::sema::types::Type::Error))));
        accepts("error First = invalid(message: Str)\nproc relay(value: Result[Int, First]) -> Result[Int, First] { match value { Ok(result) => Ok(result); Err(problem) => Err(problem) } }\n");
    }

    #[test]
    fn inferred_callable_recursive_proc_arms_keep_the_existing_result_join() {
        for returned in ["Int", "Result[Int]"] {
            let body = if returned == "Int" { "payload" } else { "Ok(payload)" };
            let source = format!("pure number(payload: Int) -> {returned} {{ {body} }}\nproc first(action: pure(payload: Int) -> {returned}, value: Int, count: Int) [] -> Result[Int] {{ if count == 0 {{ action(value) }} else {{ second(action, value, count - 1) }} }}\nproc second(action: pure(payload: Int) -> {returned}, value: Int, count: Int) [] -> Result[Int] {{ first(action, value, count) }}\nlet result: Result[Int] = first(number, 7, 2)\n");
            accepts(&source);
        }
    }

    #[test]
    fn inferred_callable_mutual_proc_components_keep_effect_scopes_separate() {
        accepts("proc left(action, value, count: Int) { if count == 0 { action(value) } else { right(action, value, count - 1) } }\nproc right(action, value, count: Int) { left(action, value, count) }\npure number(payload: Int) -> Int { payload }\npure text(input: Str) -> Str { input }\nlet integer: Result[Int] = left(number, 7, 2)\nlet string: Result[Str] = right(text, \"value\", 2)\n");
    }

    #[test]
    fn inferred_callable_recursive_uses_are_monomorphic_within_the_component() {
        accepts("pure repeated(action, value, count: Int) { if count == 0 { value } else { repeated(action, action(value), count - 1) } }\npure number(payload: Int) -> Int { payload }\npure text(input: Str) -> Str { input }\nlet number_result: Int = repeated(number, 7, 2)\nlet text_result: Str = repeated(text, \"value\", 2)\n");
    }

    #[test]
    fn inferred_callable_capture_masks_only_the_captured_error_contribution() {
        accepts("proc fallible(payload: Int) [env, error] -> Int { let _ = env.get(\"XSH_INFERRED_CALLBACK\"); assert payload > 0; payload }\nproc output(input: Int) [io] -> Int { print $input; input }\nproc combine(left, right, value) { let _ = try { left(value) }; right(value) }\nproc bounded() [env, io] -> Result[Int] { combine(fallible, output, 7) }\n");
    }

    #[test]
    fn inferred_callable_uncaptured_use_retains_the_original_error_contribution() {
        accepts("proc fallible(payload: Int) [env, error] -> Int { let _ = env.get(\"XSH_INFERRED_CALLBACK\"); assert payload > 0; payload }\nproc twice(action, value) { let _ = try { action(value) }; action(value) }\nproc bounded() [env, error] -> Result[Int] { twice(fallible, 7) }\n");
    }
}
