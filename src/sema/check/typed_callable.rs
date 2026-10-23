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
        if matches!(actual, Type::SchemeCallable(_)) {
            if let Some(target) = expected_typed_callable(expected) {
                let materialized = self.materialize_callable_value(arena, id, actual);
                if materialized == Type::Invalid { return materialized; }
                let expected = Type::Callable(target);
                let diagnostics = self.diagnostics.len();
                self.expect_type(&expected, &materialized, arena.arena.expr(id).span);
                if self.diagnostics.len() != diagnostics { return Type::Invalid; }
                self.record_expr_type(arena.arena.expr(id).span, expected.clone());
                return expected;
            }
            if expected.is_some_and(|expected| matches!(self.type_constraints.resolve(expected), Ok(Type::Inference(_)))) {
                return self.materialize_callable_value(arena, id, actual);
            }
            return actual;
        }
        if !matches!(actual, Type::Function(_)) {
            return match expected {
                Some(expected) => self.materialize_contextual_storage(arena, id, actual, expected),
                None => actual,
            };
        }
        let Some(target) = expected_typed_callable(expected) else {
            if expected.is_some_and(|expected| matches!(self.type_constraints.resolve(expected), Ok(Type::Inference(_)))) {
                return self.materialize_callable_value(arena, id, actual);
            }
            if (expected.is_none() || matches!(expected, Some(Type::SchemeCallable(_))))
                && let Some(named) = self.resolve_callable_alias_target(arena, id)
                && let Some(retained) = self.retain_named_scheme_callable(arena, &named, id)
            { return retained; }
            return actual;
        };
        let span = arena.arena.expr(id).span;
        let named = self.resolve_callable_alias_target(arena, id);
        self.fit_named_callable(named, &actual, target, span)
    }

    pub(super) fn materialize_callable_value(&mut self, arena: &ArenaProgram, id: ExprId, actual: Type) -> Type {
        let span = arena.arena.expr(id).span;
        if let Some(named) = self.resolve_callable_alias_target(arena, id) {
            return self.materialize_named_callable(named, span);
        }
        if let Type::SchemeCallable(relationship) = actual {
            return self.materialize_scheme_callable(relationship, span);
        }
        actual
    }

    fn materialize_scheme_callable(&mut self, relationship: crate::sema::types::SchemeCallableId, span: Span) -> Type {
        let (ty, instance) = match self.fresh_scheme_callable_value(relationship, span) {
            Ok(value) => value,
            Err(message) => {
                self.error(span, &message, DiagnosticCode::CheckCallableMismatch);
                return Type::Invalid;
            }
        };
        self.record_pending_value_instantiation(span, instance);
        let previous = self.scheme_callable_calls.insert(crate::sema::schemes::InstantiationSite::value(span), relationship);
        assert!(previous.is_none(), "a scheme callable value instantiates once");
        self.record_expr_type(span, ty.clone());
        ty
    }

    pub(super) fn fresh_scheme_callable_value(
        &mut self,
        relationship: crate::sema::types::SchemeCallableId,
        span: Span,
    ) -> Result<(Type, super::PendingCallInstantiation), String> {
        let relationship_contract = self.scheme_callable_relationship(relationship)
            .map_err(|error| format!("callable value relationship is not owned: {error:?}"))?;
        let scheme = relationship_contract.canonical().scheme();
        let contract = self.fresh_scheme_call_contract(scheme, None, None, span);
        let super::InvocationEffects::Checked(effects) = contract.effects else {
            return Err("callable value has no settled declaration effect contract".into());
        };
        let Some(instance) = contract.instantiation else {
            return Err("scheme callable value has no quantified contract".into());
        };
        let ty = Type::Callable(Arc::new(TypedCallable {
            kind: scheme.kind(),
            sig: super::CallableType {
                params: contract.params.into_iter().map(|parameter| super::CallableParamType {
                    name: parameter.name, ty: parameter.ty, defaulted: parameter.defaulted, rest: parameter.rest,
                }).collect(),
                return_ty: Box::new(contract.return_ty), effects,
            },
        }));
        Ok((ty, instance))
    }

    fn materialize_named_callable(&mut self, named: CallableAlias, span: Span) -> Type {
        let contract = self.function_call_contract(&named.signature, span);
        let effects = match contract.effects {
            super::InvocationEffects::Checked(effects) => effects,
            super::InvocationEffects::Pending(_) => {
                self.error(span, "callable value has no settled declaration effect contract", DiagnosticCode::CheckLocalInference);
                return Type::Invalid;
            }
        };
        if let Some(instance) = contract.instantiation.clone() { self.record_pending_value_instantiation(span, instance); }
        let callable = Type::Callable(Arc::new(TypedCallable {
            kind: named.kind,
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
        use crate::sema::callable_application::{CallableApplicationArgument, CallableKindRequirement};
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
                if matches!(ty, Type::Function(_) | Type::SchemeCallable(_)) { ty = self.materialize_callable_value(arena, value, ty); }
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
        let application = crate::sema::callable_application::CallableInvocation {
            callee: callee.clone(), arguments: arguments.into(), result: result.clone(), effects,
            proc_returns: previous.map(|application| application.proc_returns).unwrap_or_else(|| Arc::from([])), kind: if self.boundary.in_pure { CallableKindRequirement::Pure } else { CallableKindRequirement::Effectful }, span,
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
        let target = expected_typed_callable(expected);
        if target.is_none() && expected.is_some() { return None; }
        let binding = match self.lookup_value_binding_for_tail(name, statement) {
            super::ValueBindingLookup::Found(binding) => Some(binding),
            super::ValueBindingLookup::Missing => return None,
            super::ValueBindingLookup::NoSite => None,
        };
        let (named, actual, selected_signature) = if let Some(binding) = binding {
            let actual = self.type_constraints.resolve(&binding.ty).ok()?;
            if !matches!(actual, Type::Function(_)) {
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
                    kind: crate::sema::types::CallableKind::Proc,
                }),
                Type::Function(crate::sema::types::CallableKind::Proc),
                Some(signature),
            )
        } else {
            let (declaration, kind) = if let Some(declaration) = self.pures.get(&name) {
                (*declaration, crate::sema::types::CallableKind::Pure)
            } else {
                (*self.streams.get(&name)?, crate::sema::types::CallableKind::Stream)
            };
            let signature = self.local_function_signature(declaration)?;
            (
                Some(CallableAlias { name, signature: signature.clone(), kind }),
                Type::Function(kind),
                Some(signature),
            )
        };
        let Some(target) = target else {
            let named = named?;
            if named.signature.scheme().callable().is_quantified() { return None; }
            return Some(self.materialize_named_callable(named, span));
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
        if named.signature.scheme().callable().is_quantified() {
            let actual = self.materialize_named_callable(named, span);
            if actual == Type::Invalid { return Type::Invalid; }
            let diagnostic_count = self.diagnostics.len();
            self.expect_type(&expected, &actual, span);
            if self.diagnostics.len() != diagnostic_count { return Type::Invalid; }
            self.expr_types.insert(span, expected.clone());
            return expected;
        }
        let Some(signature) = super::decl::callable_type_from_function_signature(&named.signature)
        else {
            self.error(
                span,
                "a quantified function cannot be stored as a callable value",
                DiagnosticCode::CheckCallableMismatch,
            );
            return Type::Invalid;
        };
        if expected.contains_inference() {
            let actual = Type::Callable(Arc::new(TypedCallable {
                kind: named.kind,
                sig: signature,
            }));
            let diagnostic_count = self.diagnostics.len();
            self.expect_type(&expected, &actual, span);
            if self.diagnostics.len() != diagnostic_count { return Type::Invalid; }
            self.expr_types.insert(span, expected.clone());
            return expected;
        }
        if let Some(reason) = target.mismatch(named.kind, &signature, !self.collecting_effects) {
            self.error(
                span,
                &format!("`{}` does not fit `{expected}`: {reason}", named.name),
                DiagnosticCode::CheckCallableMismatch,
            );
            return Type::Invalid;
        }
        if self.collecting_effects && !self.defer_named_callable_fit(&named, target, signature, span) {
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
        let return_ty = self.check_callable_contract_call_with_checked_argument(
            arena, source, callee, args, callable, span, checked_argument,
        );
        self.typed_callable_calls.insert(span, callable.clone());
        return_ty
    }

    pub(super) fn check_callable_contract_call_with_checked_argument(
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
        let kind = callable.kind;
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
        let mut invocation = None;
        if !(callable.kind == crate::sema::types::CallableKind::Pure) {
            if self.boundary.in_pure {
                self.error(span, "effectful proc is not allowed in pure functions", DiagnosticCode::CheckPureEffect);
            } else { invocation = self.check_function_invocation_effects(&contract, &name, span); }
        }
        if !self.check_function_contract_arguments_arena(arena, source, args, &contract, span, checked_argument) {
            self.reject_invocation_return(invocation, &contract.effects, span);
            return Type::Invalid;
        }
        if !(callable.kind == crate::sema::types::CallableKind::Pure) { self.invalidate_mutable_narrowings(); }
        self.record_invocation_propagation(&contract.effects, &return_ty, span, invocation);
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
    use crate::diagnostic::{DiagnosticCode, Severity};
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

    fn checked(source: &str) -> crate::sema::check::CheckOutput {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        Checker::check_arena(&parsed.arena, source)
    }

    #[test]
    fn checked_module_call_parameters_supply_contextual_require_targets() {
        accepts(r#"
type Observation = {target: Path?}
type Collector = module {
    export pure current() -> Observation
    export pure consume(target: Path) -> Str?
}
proc inspect() [fs, error] -> Str? {
    let collector = module.load(p"collector.xsh")?.require(Collector)?
    let fallback = collector.current()
    collector.consume(fallback.target.require()?)
}
"#);
    }

    #[test]
    fn checked_call_parameter_contexts_cover_aliases_named_and_rest_arguments() {
        accepts(r#"
type Collector = module { export pure consume(target: Path) -> Str? }
type Action = pure(target: Path) -> Str?
pure count_paths(...targets: List[Path]) -> Int { targets.len() }
proc call_alias(collector: Collector, raw: Any) [error] -> Str? {
    let consume = collector.consume
    consume(raw.require()?)
}
proc call_named(action: Action, raw: Any) [error] -> Str? {
    action(target: raw.require()?)
}
proc call_rest(raw: Any) [error] -> Int {
    let action = count_paths
    action(raw.require()?)
}
"#);
    }

    #[test]
    fn checked_call_parameter_contexts_reject_unresolved_and_uncheckable_targets() {
        for source in [
            "type Action = pure(value: Any) -> Int\nproc inspect(action: Action, raw: Any) [error] -> Int { action(raw.require()?) }\n",
            "type Action = pure(value: Int) -> Int\ntype Consumer = pure(action: Action) -> Int\nproc inspect(consume: Consumer, raw: Any) [error] -> Int { consume(raw.require()?) }\n",
            "pure ignore(value) { 0 }\nproc inspect(raw: Any) [error] -> Int { ignore(raw.require()?) }\n",
        ] {
            let output = checked(source);
            assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckRequireTarget)), "{source}\n{:?}", output.diagnostics);
        }
    }

    #[test]
    fn checked_call_parameter_contexts_retain_nominal_schema_instances() {
        let source = "type Box[T] = {value: Int}\npure consume(value: Box[Str]) -> Int { value.value }\nlet incoming: Any = {value: 7}\nlet _ = consume(incoming.require()?)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let _symbols = parsed.arena.symbol_owner().enter();
        let output = Checker::check_arena(&parsed.arena, source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let target = output.requirement_targets.iter().find(|(span, _)| &source[span.range()] == "incoming.require()").unwrap().1;
        assert_eq!(target.context.instances.len(), 1);
        assert_eq!(target.context.instances[0].arguments, [crate::sema::types::Type::Str]);
        assert_eq!(parsed.arena.arena.type_def(target.context.instances[0].definition).name.as_str().as_str(), "Box");
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
        let application = application.invocation().unwrap();
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
    fn grounded_callable_branch_tails_preserve_the_selected_contract() {
        for flag in [true, false] {
            for (first, second) in [("increment", "decrement"), ("decrement", "increment")] {
                accepts(&format!("pure increment(value: Int) -> Int {{ value + 1 }}\npure decrement(value: Int) -> Int {{ value - 1 }}\nlet selected = if {flag} {{ let candidate = {first}; candidate }} else {{ let candidate = {second}; candidate }}\nlet result: Int = selected(value: 3)\n"));
            }
        }
    }

    #[test]
    fn grounded_callable_branch_tails_reject_incompatible_contracts() {
        for (left, right, reversible) in [
            ("pure left(value: Int) -> Int { value }", "pure right(value: Str) -> Str { value }", true),
            ("pure left(value: Int) -> Int { value }", "pure right(other: Int) -> Int { other }", true),
            ("pure left(value: Int = 1) -> Int { value }", "pure right(value: Int) -> Int { value }", false),
            ("pure left(...value: List[Int]) -> Int { 1 }", "pure right(value: Int) -> Int { value }", true),
            ("proc left(value: Int) [] -> Int { value }", "proc right(value: Int) [env] -> Int { let _ = env.get(\"XSH_CALLABLE_BRANCH\"); value }", false),
        ] {
            for reverse in [false, true].into_iter().filter(|reverse| !reverse || reversible) {
                let (first, second) = if reverse { ("right", "left") } else { ("left", "right") };
                let source = format!("{left}\n{right}\nlet selected = if true {{ let candidate = {first}; candidate }} else {{ let candidate = {second}; candidate }}\nlet invalid = selected(42)\n");
                let output = checked(&source);
                assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckCallableMismatch)), "{source}\n{:?}", output.diagnostics);
                assert!(!output.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckUnresolvedCall)), "{source}\n{:?}", output.diagnostics);
            }
        }
    }

    #[test]
    fn grounded_callable_branch_tails_do_not_authenticate_dynamic_values() {
        for ty in ["Pure", "Any"] {
            for declaration in ["let", "var"] {
                let source = format!("pure increment(value: Int) -> Int {{ value + 1 }}\n{declaration} unchecked: {ty} = increment\nlet selected = if true {{ let candidate = increment; candidate }} else {{ let candidate = unchecked; candidate }}\nlet invalid = selected(3)\n");
                let output = checked(&source);
                assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error && matches!(diagnostic.code, Some(DiagnosticCode::CheckCallableMismatch | DiagnosticCode::CheckDynamicBoundary))), "{source}\n{:?}", output.diagnostics);
            }
        }
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

    #[test]
    fn mutable_callable_reassignment_keeps_its_inferred_contract() {
        let source = "pure add(value: Int, amount: Int = 1, ...rest: List[Int]) -> Int { value + amount + rest.len() }\npure subtract(value: Int, amount: Int = 1, ...rest: List[Int]) -> Int { value - amount + rest.len() }\npure exercise() -> Int { var operation = add; let labelled = operation(value: 40); let spread = operation(40, 1, 2, 3); operation = subtract; let reassigned = operation(value: 43); labelled + spread + reassigned }\n";
        let checked = checked(source);
        assert!(
            !checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error),
            "{source}\n{:?}", checked.diagnostics
        );
        let calls = checked.typed_callable_calls.values().collect::<Vec<_>>();
        assert_eq!(calls.len(), 3);
        assert!(calls.iter().all(|call| call.as_ref() == calls[0].as_ref()));
        let signature = &calls[0].sig;
        assert_eq!(signature.params.len(), 3);
        assert_eq!(signature.params[0].ty, crate::sema::types::Type::Int);
        assert!(!signature.params[0].defaulted);
        assert_eq!(signature.params[1].ty, crate::sema::types::Type::Int);
        assert!(signature.params[1].defaulted);
        assert_eq!(
            signature.params[2].ty,
            crate::sema::types::Type::List(Box::new(crate::sema::types::Type::Int))
        );
        assert!(signature.params[2].rest);
        assert_eq!(signature.return_ty.as_ref(), &crate::sema::types::Type::Int);
    }

    #[test]
    fn quantified_callable_targets_choose_independent_instances() {
        let source = "pure identity(value) { value }\nlet integer_identity: pure(value: Int) -> Int = identity\nlet text_identity: pure(value: Str) -> Str = identity\nlet integer: Int = integer_identity(42)\nlet text: Str = text_identity(\"demo\")\nlet alias = identity\nlet direct_integer: Int = alias(43)\nlet direct_text: Str = alias(\"more\")\n";
        let checked = checked(source);
        assert!(
            !checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error),
            "{source}\n{:?}", checked.diagnostics
        );
        let calls = checked.typed_callable_calls.values().collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        assert!(calls.iter().any(|call| {
            call.sig.params[0].ty == crate::sema::types::Type::Int
                && call.sig.return_ty.as_ref() == &crate::sema::types::Type::Int
        }));
        assert!(calls.iter().any(|call| {
            call.sig.params[0].ty == crate::sema::types::Type::Str
                && call.sig.return_ty.as_ref() == &crate::sema::types::Type::Str
        }));
    }

    #[test]
    fn stream_producer_alias_keeps_its_lazy_call_contract() {
        let source = "stream values(value: Int) -> Stream[Int] { yield value }\nproc make() -> Result[Stream[Int]] { let create = values; Ok(create(42)) }\n";
        let checked = checked(source);
        assert!(
            !checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error),
            "{source}\n{:?}", checked.diagnostics
        );
        let invocations = checked.selected_function_calls.iter()
            .filter(|(span, _)| !checked.selected_function_value_references.contains(span))
            .collect::<Vec<_>>();
        assert_eq!(invocations.len(), 1);
        assert_eq!(&source[invocations[0].0.range()], "create(42)");
        assert_eq!(invocations[0].1.name, crate::symbol::Name::intern("values"));
    }

    #[test]
    fn mutable_inferred_proc_keeps_its_solved_effect_contract() {
        let source = "proc read(value: Int) { let _ = env.get(\"XSH_MUTABLE_CALLABLE\"); value }\nproc read_again(value: Int) { let _ = env.get(\"XSH_MUTABLE_CALLABLE\"); value }\nproc exercise(value: Int) { var operation = read; let first = operation(value); operation = read_again; operation(value) }\n";
        let checked = checked(source);
        assert!(
            !checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error),
            "{source}\n{:?}", checked.diagnostics
        );
        let expected = crate::sema::effects::CheckedEffects::finite([crate::syntax::node::Effect::Env]);
        assert_eq!(checked.callable_effects.get("read"), Some(&expected));
        assert_eq!(checked.callable_effects.get("read_again"), Some(&expected));
        assert_eq!(checked.callable_effects.get("exercise"), Some(&expected));
        let calls = checked.typed_callable_calls.values().collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        assert!(calls.iter().all(|call| {
            call.sig.effects.concrete().expect("solved callable effect contract")
                == Some(vec![crate::syntax::node::Effect::Env])
        }));
    }

    #[test]
    fn mutable_generic_callable_instantiates_once_and_rejects_a_later_type_change() {
        let source = "pure identity(value) { value }\npure exercise() { var current = identity; let integer: Int = current(42); current = identity; let text = current(\"demo\"); }\n";
        let checked = checked(source);
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
        assert_eq!(errors.len(), 1, "{source}\n{:?}", checked.diagnostics);
        assert_eq!(
            errors[0].code,
            Some(DiagnosticCode::CheckTypeMismatch),
            "{source}\n{:?}", checked.diagnostics
        );
        assert!(
            !checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckUnresolvedCall)),
            "{source}\n{:?}", checked.diagnostics
        );
    }

    #[test]
    fn nested_mutable_callable_storage_instantiates_visible_leaves_once() {
        for source in [
            "pure identity(value) { value }\nlet original = {copy: identity}\nvar held = original\nlet integer = held.copy(7)\nlet text = held.copy(\"demo\")\n",
            "pure identity(value) { value }\npure consume(operations) { let integer = operations.copy(7); operations.copy(\"demo\") }\nlet operations = {copy: identity}\nlet result = consume(operations)\n",
        ] {
            let checked = checked(source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error
                && matches!(diagnostic.code, Some(DiagnosticCode::CheckTypeMismatch | DiagnosticCode::CheckCallableMismatch))),
                "a shared callback is monomorphic within one activation: {source}\n{:?}", checked.diagnostics);
        }
        accepts("pure first(value) { value }\npure second(value) { value }\nlet original = {copy: first, other: second}\nvar held = original\nheld.copy = second\nlet integer = held.copy(7)\nlet text = held.other(\"demo\")\n");
        accepts("pure identity(value) { value }\npure passive(value) { value }\nlet operations = passive({copy: identity})\nlet integer = operations.copy(7)\nlet text = operations.copy(\"demo\")\n");
    }

    #[test]
    fn mutable_generic_callable_compacts_value_instances_separately_from_calls() {
        let source = "pure identity(value) { value }\npure exercise() -> Int { var operation = identity; let first: Int = operation(41); operation = identity; first + operation(42) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let mut sources = crate::source::SourceMap::new();
        let source_id = sources.add_file("mutable-generic-callable-facts.xsh", source);
        let checked = Checker::check_arena_with_sources_and_options(
            &parsed.arena,
            &sources,
            source_id,
            crate::sema::check::CheckOptions {
                embedded_bodies: true,
                ..crate::sema::check::CheckOptions::default()
            },
        );
        assert!(
            !checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error),
            "{source}\n{:?}", checked.diagnostics
        );
        let declarations = Checker::compact_declarations(&parsed.arena, checked);
        assert!(declarations.diagnostics.is_empty(), "{source}\n{:?}", declarations.diagnostics);
        let expression_at = |start: usize, end: usize| {
            let span = crate::source::Span::new(source_id, start, end);
            (0..parsed.arena.arena.expr_tags.len())
                .map(crate::syntax::arena::ExprId::from_index)
                .find(|id| parsed.arena.arena.expr(*id).span == span)
                .unwrap_or_else(|| panic!("missing expression at {span:?}"))
        };
        let initial_start = source.find("var operation = identity").unwrap() + "var operation = ".len();
        let assigned_start = source.find("operation = identity").unwrap() + "operation = ".len();
        let identity_end = "identity".len();
        let initial = expression_at(initial_start, initial_start + identity_end);
        let assigned = expression_at(assigned_start, assigned_start + identity_end);
        let initial_target = declarations.bodies.selected_function_calls.get(&initial).unwrap();
        let assigned_target = declarations.bodies.selected_function_calls.get(&assigned).unwrap();
        assert_eq!(initial_target.name.as_str(), "identity");
        assert_eq!(assigned_target, initial_target);
        let initial_instance = declarations.bodies.call_instantiations.get(&crate::sema::schemes::InstantiationSite::value(initial)).unwrap();
        let assigned_instance = declarations.bodies.call_instantiations.get(&crate::sema::schemes::InstantiationSite::value(assigned)).unwrap();
        assert_eq!(initial_instance, assigned_instance);
        let initial_leaves = initial_instance.instances().collect::<Vec<_>>();
        assert_eq!(initial_leaves.len(), 1);
        let initial_instance = initial_leaves[0];
        assert_eq!(initial_instance.type_arguments().len(), 1);
        assert_eq!(
            initial_instance.type_arguments().values().next().unwrap().closed_ground_type(),
            Some(crate::sema::types::Type::Int)
        );
        assert!(initial_instance.row_arguments().is_empty());
        assert!(initial_instance.effect_arguments().is_empty());
        for fragment in ["operation(41)", "operation(42)"] {
            let start = source.find(fragment).unwrap();
            let call = expression_at(start, start + fragment.len());
            assert!(declarations.bodies.typed_callable_calls.contains_key(&call));
            assert!(!declarations.bodies.selected_function_calls.contains_key(&call));
            assert_eq!(
                declarations.bodies.expr_types.get(&call),
                Some(&crate::sema::types::Type::Int)
            );
        }
    }

    #[test]
    fn mutable_callable_reassignment_rejects_parameter_return_and_effect_changes() {
        for source in [
            "pure number(value: Int) -> Int { value }\npure text(value: Str) -> Str { value }\npure exercise() { var operation = number; operation = text; }\n",
            "pure number(value: Int) -> Int { value }\npure text(value: Int) -> Str { \"demo\" }\npure exercise() { var operation = number; operation = text; }\n",
            "proc plain(value: Int) [] -> Int { value }\nproc environmental(value: Int) [env] -> Int { let _ = env.get(\"XSH_MUTABLE_CALLABLE\"); value }\nproc exercise() [] { var operation = plain; operation = environmental; }\n",
        ] {
            let checked = checked(source);
            let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
            assert_eq!(errors.len(), 1, "{source}\n{:?}", checked.diagnostics);
            assert_eq!(
                errors[0].code,
                Some(DiagnosticCode::CheckCallableMismatch),
                "{source}\n{:?}", checked.diagnostics
            );
            assert!(
                !checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckUnresolvedCall)),
                "{source}\n{:?}", checked.diagnostics
            );
        }
    }
}
