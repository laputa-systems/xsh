//! Typed callables: `proc(PARAMS) [EFFECTS] -> T` and `pure(PARAMS) -> T`.
//!
//! A typed callable is the dynamic handle of its kind at run time. What makes
//! it typed is decided here, in two places and nowhere else:
//!
//! - A value gets the type only from a function the checker can name (a
//!   declaration, a module export, or a signature-keeping alias) whose
//!   signature fits, or from another value of a fitting callable type.
//! - A call through the type is checked against the type's signature, charges
//!   the type's effect bound to the caller, and is published as a fact that
//!   lowering consumes.
//!
//! The effect bound is an upper bound on every function that can reach the
//! type. A type written without a clause has the unrestricted bound, which is
//! charged as unknown effects and never as none.

use super::{CallableAlias, Checker, FunctionParamSig, FunctionSig, Name, Span, Type};
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
    /// Gives a function named where a callable type is expected that type,
    /// when its signature fits. `actual` is the type the expression has on
    /// its own. A dynamic handle with no signature the checker can name is
    /// rejected: nothing vouches for what it accepts or does.
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
            return actual;
        };
        let span = arena.arena.expr(id).span;
        let named = self.resolve_callable_alias_target(arena, id);
        self.fit_named_callable(named, &actual, target, span)
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
        let effects = match self.type_constraints.resolve_effect(callable.sig.effects.contract()) {
            Ok(contract) => crate::sema::types::CallableEffects::from_contract(contract).concrete(),
            Err(_) => Err(crate::sema::types::UnresolvedCallableEffects),
        };
        let Ok(effects) = effects else {
            self.error(span, "callable effect contract is unresolved", DiagnosticCode::CheckCallableMismatch);
            return Type::Invalid;
        };
        // A callable type has no rest parameter and no defaults, so a call
        // whose argument count is decided at run time cannot be checked.
        if let Some(arg) = args.iter().find(|arg| {
            matches!(
                arg.kind,
                ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. }
            )
        }) {
            self.error(
                super::call_arg_span_arena(arena, &arg.kind),
                "a call through a callable type passes each argument explicitly; it cannot splice or spread them",
                DiagnosticCode::CheckCallableMismatch,
            );
            for arg in args {
                match arg.kind {
                    ArenaCallArgKind::Splice { value, .. }
                    | ArenaCallArgKind::NamedSpread { value, .. } => {
                        self.check_expr_arena(arena, source, value, None);
                    }
                    _ => {
                        self.check_call_arg_arena(arena, source, &arg.kind, None);
                    }
                }
            }
            return return_ty;
        }
        let name = match arena.arena.expr(callee).kind {
            ArenaExprKind::Ident(name) | ArenaExprKind::Field { name, .. } => name.to_string(),
            _ => "callable value".to_string(),
        };
        let signature = FunctionSig::ground(
            if callable.pure {
                crate::sema::types::CallableKind::Pure
            } else {
                crate::sema::types::CallableKind::Proc
            },
            None,
            false,
            None,
            None,
            callable
                .sig
                .params
                .iter()
                .map(|param| FunctionParamSig {
                    name: param.name,
                    ty: param.ty.clone(),
                    schema_expectation: Some(Default::default()),
                    defaulted: false,
                    rest: false,
                })
                .collect(),
            return_ty.clone(),
            None,
            effects,
            false,
            true,
        )
        .expect("a typed callable has a closed contract");
        let contract = self.function_call_contract(&signature, span);
        if !callable.pure {
            if self.boundary.in_pure {
                self.error(
                    span,
                    "effectful proc is not allowed in pure functions",
                    DiagnosticCode::CheckPureEffect,
                );
            } else {
                // With no declaration behind it, the bound itself is the
                // contract: each effect is required of the caller, and a
                // missing clause is recorded as unknown.
                self.check_resolved_callable_effects(&signature, &name, span);
            }
        }
        self.check_function_call_arguments_arena(
            arena,
            source,
            args,
            &contract.params,
            span,
            checked_argument,
        );
        if !callable.pure {
            // A procedure may change mutable lexical captures before the next statement.
            self.invalidate_mutable_narrowings();
        }
        let propagation_effects = if callable.pure {
            &[][..]
        } else {
            contract.effects.as_deref().unwrap_or(&crate::syntax::node::Effect::ALL)
        };
        self.record_callee_propagation(Some(propagation_effects), &return_ty, span);
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
    fn inferred_callable_positional_uses_preserve_independent_types_and_labels() {
        accepts("pure apply(action, value) { action(value) }\npure number(payload: Int) -> Int { payload }\npure text(input: Str) -> Str { input }\nlet number_result: Int = apply(number, 7)\nlet text_result: Str = apply(text, \"value\")\n");
    }

    #[test]
    fn inferred_callable_clients_do_not_choose_the_declaration_shape() {
        accepts("pure text(input: Str) -> Str { input }\npure number(payload: Int) -> Int { payload }\nlet text_result: Str = apply(text, \"value\")\nlet number_result: Int = apply(number, 7)\npure apply(action, value) { action(value) }\n");
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
    fn inferred_callable_returned_values_keep_the_same_constraint_binding() {
        accepts("pure relay(action) { action }\npure apply(action, value) { let selected = relay(action); selected(value) }\npure number(payload: Int) -> Int { payload }\npure text(input: Str) -> Str { input }\nlet number_result: Int = apply(number, 7)\nlet text_result: Str = apply(text, \"value\")\n");
    }

    #[test]
    fn inferred_callable_aliases_keep_the_selected_callable_relationship() {
        accepts("pure apply(action, value) { let selected = action; selected(value) }\npure number(payload: Int) -> Int { payload }\npure text(input: Str) -> Str { input }\nlet numeric = number\nlet textual = text\nlet number_result: Int = apply(numeric, 7)\nlet text_result: Str = apply(textual, \"value\")\n");
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
