#![allow(clippy::single_call_fn)]

use super::expr::is_path_like_type;
use super::{
    CheckedApiCall, CheckedCommandArgument, CheckedModuleCommand, Checker, FunctionParamSig,
    FxHashSet, ModuleFnSig, Span, Type,
    command_arg_can_be_path_like_arena, command_bool_flag_name_arena,
};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{ArenaCallArg, ArenaCallArgKind, ArenaCommandArg, ArenaProgram, ExprId};

#[derive(Clone, Copy)]
pub(super) struct CheckedCallArgument<'a> {
    pub ty: &'a Type,
    pub span: Span,
}

#[derive(Clone, Default)]
pub(super) struct CheckedArgumentContext {
    pub(super) expected: Option<Type>,
    pub(super) schema: crate::sema::constants::SchemaExpectation,
}

#[derive(Clone)]
pub(super) struct CheckedArgumentValues {
    pub(super) arguments: Vec<crate::sema::arguments::ExpandedArgument>,
    pub(super) contexts: Vec<CheckedArgumentContext>,
}

#[derive(Clone)]
pub(super) struct CallSpliceItemContexts {
    pub(super) expression: ExprId,
    pub(super) items: Vec<(Type, Option<crate::sema::constants::SchemaExpectation>)>,
    pub(super) authority: CallSpliceAuthority,
}

#[derive(Clone, Copy)]
pub(super) enum CallSpliceAuthority { Immediate, KnownCallable }

#[derive(Clone, Copy)]
enum CallArgumentAuthority<'a> {
    Immediate,
    KnownCallable {
        instance: Option<&'a crate::sema::constraints::InstantiatedScheme>,
        deferred: &'a std::cell::Cell<bool>,
    },
}

impl<'a> CallArgumentAuthority<'a> {
    fn instance(self) -> Option<&'a crate::sema::constraints::InstantiatedScheme> {
        match self { Self::Immediate => None, Self::KnownCallable { instance, .. } => instance }
    }
}

pub(super) struct CheckedBoundArguments {
    pub(super) values: CheckedArgumentValues,
    pub(super) binding: crate::sema::arguments::StaticArgumentBinding,
}

pub(super) enum SelectedMethodArguments {
    Invalid,
    Bound(CheckedBoundArguments),
    Values {
        values: CheckedArgumentValues,
        binding: crate::sema::arguments::StaticArgumentBinding,
        schemas: Vec<Option<crate::sema::constants::SchemaExpectation>>,
    },
}

fn expanded_checked_argument(argument: CheckedCallArgument<'_>) -> crate::sema::arguments::ExpandedArgument {
    crate::sema::arguments::ExpandedArgument {
        entry_index: 0,
        name: None,
        value: crate::sema::arguments::ArgumentValueSource::CheckedType,
        ty: argument.ty.clone(),
        span: argument.span,
    }
}

pub(super) fn module_sig_accepts_arity(arg_count: usize, sig: &ModuleFnSig) -> bool {
    let required = sig.params.iter().filter(|param| !param.defaulted).count();
    arg_count >= required && arg_count <= sig.params.len()
}

fn function_arity_error(params: &[FunctionParamSig], found: usize) -> Option<String> {
    let required = params
        .iter()
        .filter(|param| !param.defaulted && !param.rest)
        .count();
    let max = if params.iter().any(|param| param.rest) {
        usize::MAX
    } else {
        params.len()
    };
    if (required..=max).contains(&found) {
        return None;
    }
    let expected = if required == max {
        format!(
            "{required} argument{}",
            if required == 1 { "" } else { "s" }
        )
    } else if max == usize::MAX {
        format!("at least {required} arguments")
    } else {
        format!("{required} to {max} arguments")
    };
    Some(format!(
        "incorrect function arity: expected {expected}, found {found}"
    ))
}

fn static_positional_list_elements(
    arena: &ArenaProgram,
    expression: ExprId,
) -> Option<Vec<ExprId>> {
    let crate::syntax::arena::ArenaExprKind::List(elements) =
        arena.arena.expr(expression).kind
    else {
        return None;
    };
    let elements = arena.arena.list_elements(elements).collect::<Vec<_>>();
    elements
        .iter()
        .all(|element| element.splice_span.is_none())
        .then(|| elements.into_iter().map(|element| element.value).collect())
}

fn static_positional_argument_count(arena: &ArenaProgram, args: &[ArenaCallArg]) -> Option<usize> {
    args.iter().try_fold(0usize, |count, arg| match arg.kind {
        ArenaCallArgKind::Positional(_) => Some(count.saturating_add(1)),
        ArenaCallArgKind::Splice { value, .. } => Some(count.saturating_add(
            static_positional_list_elements(arena, value)?.len(),
        )),
        ArenaCallArgKind::Named { .. } | ArenaCallArgKind::NamedSpread { .. } => None,
    })
}

fn callable_parameter_types(params: &[FunctionParamSig]) -> Vec<crate::sema::types::CallableParamType> {
    params
        .iter()
        .map(|param| crate::sema::types::CallableParamType {
            name: param.name,
            ty: param.ty.clone(),
            defaulted: param.defaulted,
            rest: param.rest,
        })
        .collect()
}

impl Checker {
    pub(super) fn check_function_call_arguments_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        params: &[FunctionParamSig],
        span: Span,
        checked_argument: Option<CheckedCallArgument<'_>>,
    ) {
        self.check_function_call_arguments_with_authority_arena(arena, source, args, params, span, checked_argument, CallArgumentAuthority::Immediate);
    }

    fn check_function_call_arguments_with_authority_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        params: &[FunctionParamSig],
        span: Span,
        checked_argument: Option<CheckedCallArgument<'_>>,
        authority: CallArgumentAuthority<'_>,
    ) {
        if let Some(argument) = checked_argument {
            assert!(args.is_empty(), "a checked argument replaces source arguments");
            let expanded = [expanded_checked_argument(argument)];
            let params_for_binding = callable_parameter_types(params);
            let arity = function_arity_error(params, 1);
            let binding = match crate::sema::arguments::bind_static_arguments(
                &params_for_binding,
                &expanded,
            ) {
                Ok(binding) => binding,
                Err(error) => {
                    let message = arity.unwrap_or(error.message);
                    self.error(span, &message, DiagnosticCode::CheckArity);
                    if let Some(param) = params.first() {
                        let expected = self.function_argument_type(param, span);
                        self.expect_function_call_argument(authority, &expected, argument.ty, argument.span);
                    }
                    return;
                }
            };
            for (slot, argument) in binding.argument_slots.into_iter().zip(expanded) {
                let expected = self.function_argument_type(&params[slot], span);
                self.expect_function_call_argument(authority, &expected, &argument.ty, argument.span);
            }
            return;
        }
        self.check_function_arg_list_arena(arena, source, args, params, span, authority);
    }

    pub(super) fn check_function_contract_arguments_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        contract: &super::FunctionCallContract,
        span: Span,
        checked_argument: Option<CheckedCallArgument<'_>>,
        expected_context: Option<&Type>,
    ) -> bool {
        let owner = self.stream_origin_owner.map_or(crate::sema::constraints::CallableApplicationScope::Invocation, crate::sema::constraints::CallableApplicationScope::Declaration);
        let constructor_arguments = self.type_constraints.begin_constructor_arguments(owner);
        let diagnostics_before_arguments = self.diagnostics.len();
        if contract.instantiation.is_some() && contract.return_ty.contains_inference()
            && let Some(expected) = expected_context
            && !expected.contains_inference() && !expected.contains_recovery()
        {
            use crate::sema::arguments::{ArgumentValueSource, bind_static_arguments, expand_named_arguments};
            if let Ok(expanded) = expand_named_arguments(arena, args, |_| None)
                && let Ok(binding) = bind_static_arguments(&callable_parameter_types(&contract.params), &expanded)
            {
                let inputs = expanded.iter().zip(binding.argument_slots).filter_map(|(argument, slot)| {
                    let ArgumentValueSource::Expression(expression) = argument.value else { return None; };
                    matches!(arena.arena.expr(expression).kind, crate::syntax::arena::ArenaExprKind::List(_))
                        .then(|| contract.params[slot].ty.clone())
                }).collect::<Vec<_>>();
                if !inputs.is_empty() {
                    let (_, instance) = contract.instantiation.as_ref().expect("guard requires a selected instance");
                    if let Err(conflict) = self.type_constraints.constrain_call_input_context(instance, &inputs, expected, span) {
                        self.report_inferred_type_conflict(conflict);
                    }
                }
            }
        }
        let deferred = std::cell::Cell::new(false);
        self.check_function_call_arguments_with_authority_arena(
            arena,
            source,
            args,
            &contract.params,
            span,
            checked_argument,
            CallArgumentAuthority::KnownCallable { instance: contract.instantiation.as_ref().map(|(_, authority)| authority), deferred: &deferred },
        );
        if deferred.get() { self.retain_known_call_arguments(arena, args, contract, span, checked_argument, owner); }
        let argument_error = self.diagnostics[diagnostics_before_arguments..]
            .iter()
            .any(|diagnostic| {
                diagnostic.severity == crate::diagnostic::Severity::Error
                    && diagnostic.labels.iter().any(|label| {
                        label.style == crate::diagnostic::LabelStyle::Primary
                            && label.span.source_id == span.source_id
                            && span.start() <= label.span.start()
                            && label.span.end() <= span.end()
                    })
            });
        if argument_error {
            if let Err(error) = self.type_constraints.reject_constructor_arguments(constructor_arguments) {
                self.error(span, &format!("constructor argument scope cannot close: {error:?}"), DiagnosticCode::CheckLocalInference);
            }
            return false;
        }
        match self.type_constraints.solve_callable_applications() {
            Ok(()) => {
                if let Err(error) = self.type_constraints.finish_constructor_arguments(constructor_arguments) {
                    self.error(span, &format!("constructor error inference failed: {error:?}"), DiagnosticCode::CheckLocalInference);
                    return false;
                }
                if let Some(instantiation) = &contract.instantiation {
                    let (callable, authority) = instantiation;
                    let validations = match self.type_constraints.key_validation_wrapper_requirements_for_instantiation(callable, authority) {
                        Ok(validations) => validations,
                        Err(error) => {
                            self.error(span, &format!("index validation escaped its callable authority: {error:?}"), DiagnosticCode::CheckLocalInference);
                            return false;
                        }
                    };
                    for requirement in validations {
                        if self.boundary.retry_attempt_depth > 0
                            && let Some(errors) = self.boundary.error_boundary_errors.last_mut()
                        {
                            errors.push((crate::sema::wrapper_operation::WrapperErrorContribution::KeyValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Source(requirement)), span));
                        }
                    }
                    self.record_pending_call_instantiation(span, instantiation.clone());
                }
                true
            }
            Err(error) => {
                if let Err(error) = self.type_constraints.reject_constructor_arguments(constructor_arguments) {
                    self.error(span, &format!("constructor argument scope cannot close: {error:?}"), DiagnosticCode::CheckLocalInference);
                }
                let code = if matches!(error, crate::sema::constraints::CallableApplicationError::Effects(crate::sema::constraints::EffectConstraintError::Mismatch)) {
                    DiagnosticCode::CheckEffectViolation
                } else if let crate::sema::constraints::CallableApplicationError::Operation(error) = &error {
                    match error {
                        crate::sema::constraints::SealedOperationError::UnsupportedTypePredicate { domain, .. } => match domain {
                            crate::sema::sealed_operation::TypeDomain::SortKey => DiagnosticCode::CheckStreamSort,
                            crate::sema::sealed_operation::TypeDomain::CountKey => DiagnosticCode::CheckStreamCountKey,
                            crate::sema::sealed_operation::TypeDomain::CollectItem => DiagnosticCode::CheckYieldStream,
                            crate::sema::sealed_operation::TypeDomain::MinMaxItem => DiagnosticCode::CheckDynamicBoundary,
                        },
                        crate::sema::constraints::SealedOperationError::UnsupportedMembership { .. } => DiagnosticCode::CheckMembershipType,
                        crate::sema::constraints::SealedOperationError::UnsupportedIndex { .. } => DiagnosticCode::CheckIndexType,
                        crate::sema::constraints::SealedOperationError::UnsupportedSlice { .. } => DiagnosticCode::CheckSliceType,
                        crate::sema::constraints::SealedOperationError::UnsupportedRecordSpread { .. } => DiagnosticCode::CheckSpreadNotRecord,
                        crate::sema::constraints::SealedOperationError::Resolution(_)
                        | crate::sema::constraints::SealedOperationError::Constraint(_)
                        | crate::sema::constraints::SealedOperationError::MapKey(_)
                        | crate::sema::constraints::SealedOperationError::Unsupported { .. } => DiagnosticCode::CheckOperatorType,
                    }
                } else if let crate::sema::constraints::CallableApplicationError::Wrapper(crate::sema::constraints::WrapperConstraintError::Invalid { operation, .. }) = &error {
                    match operation {
                        crate::sema::wrapper_operation::WrapperOperationKind::Field => DiagnosticCode::CheckNullSafeField,
                        crate::sema::wrapper_operation::WrapperOperationKind::Fallback => DiagnosticCode::CheckResultFallback,
                        crate::sema::wrapper_operation::WrapperOperationKind::Handler => DiagnosticCode::CheckFallbackBlockResult,
                        crate::sema::wrapper_operation::WrapperOperationKind::Capture => DiagnosticCode::CheckTryError,
                        crate::sema::wrapper_operation::WrapperOperationKind::KeyValidation => DiagnosticCode::CheckTryError,
                        crate::sema::wrapper_operation::WrapperOperationKind::Propagation | crate::sema::wrapper_operation::WrapperOperationKind::ProcCompletion => DiagnosticCode::CheckTryError,
                    }
                } else { DiagnosticCode::CheckCallableMismatch };
                self.error(span, &format!("callable application requirement failed: {error}"), code);
                false
            }
        }
    }

    fn retain_known_call_arguments(
        &mut self,
        arena: &ArenaProgram,
        args: &[ArenaCallArg],
        contract: &super::FunctionCallContract,
        span: Span,
        checked_argument: Option<CheckedCallArgument<'_>>,
        owner: crate::sema::constraints::CallableApplicationScope,
    ) {
        use crate::sema::arguments::{ArgumentValueSource, StaticArgumentKind};
        use crate::sema::callable_application::{CallableApplicationArgument, KnownArgumentParameter, KnownCallArguments};
        let expanded = if let Some(argument) = checked_argument {
            vec![expanded_checked_argument(argument)]
        } else {
            let expanded = match crate::sema::arguments::expand_named_arguments(arena, args, |expression| {
                self.expr_types.get(&arena.arena.expr(expression).span).cloned()
            }) {
                Ok(expanded) => expanded,
                Err(error) => {
                    self.error(error.span, &error.message, DiagnosticCode::CheckNamedSpread);
                    return;
                }
            };
            let mut flattened = Vec::new();
            for argument in expanded {
                if let ArgumentValueSource::PositionalSplice(expression) = argument.value
                    && let Some(elements) = static_positional_list_elements(arena, expression)
                {
                    for expression in elements {
                        let span = arena.arena.expr(expression).span;
                        let Some(ty) = self.expr_types.get(&span).cloned() else {
                            self.error(span, "static splice argument has no checked type", DiagnosticCode::CheckLocalInference);
                            return;
                        };
                        flattened.push(crate::sema::arguments::ExpandedArgument { entry_index: argument.entry_index, name: None, value: ArgumentValueSource::Expression(expression), ty, span });
                    }
                } else { flattened.push(argument); }
            }
            flattened
        };
        let parameters = callable_parameter_types(&contract.params);
        if let Err(error) = crate::sema::arguments::bind_static_arguments(&parameters, &expanded) {
            self.error(error.span, &error.message, DiagnosticCode::CheckNamedArg);
            return;
        }
        let parameters = parameters.into_iter().map(|parameter| KnownArgumentParameter {
            layout: crate::sema::arguments::ArgumentParameterLayout { name: parameter.name, defaulted: parameter.defaulted, rest: parameter.rest },
            ty: parameter.ty,
        }).collect::<Vec<_>>().into();
        let arguments = expanded.into_iter().map(|argument| CallableApplicationArgument {
            kind: match (argument.name, argument.value) {
                (Some(name), _) => StaticArgumentKind::Named(name),
                (None, ArgumentValueSource::PositionalSplice(_)) => StaticArgumentKind::Splice,
                _ => StaticArgumentKind::Positional,
            },
            ty: argument.ty,
            span: argument.span,
        }).collect::<Vec<_>>().into();
        if let Err(error) = self.type_constraints.register_known_call_arguments(owner, KnownCallArguments { parameters, arguments, span }) {
            self.error(span, &format!("known argument relationship failed: {error}"), DiagnosticCode::CheckCallableMismatch);
        }
    }

    fn expect_function_call_argument(
        &mut self,
        authority: CallArgumentAuthority<'_>,
        expected: &Type,
        actual: &Type,
        span: Span,
    ) {
        let pending_method_result = if matches!(actual, Type::Inference(_)) {
            match self.type_constraints.unresolved_method_result(actual) {
                Ok(pending) => pending,
                Err(error) => {
                    self.report_inferred_type_conflict(crate::sema::constraints::ConstraintConflict {
                        expected: expected.clone(), actual: actual.clone(), initializer: None,
                        established: None, contribution: span, resolution_error: Some(error),
                    });
                    return;
                }
            }
        } else { false };
        if let CallArgumentAuthority::KnownCallable { deferred, .. } = authority
            && (matches!(self.type_constraints.resolve(expected), Ok(Type::Optional(_)))
                || pending_method_result)
            && matches!(self.type_constraints.resolve(actual), Ok(Type::Inference(_)))
        { deferred.set(true); return; }
        if !self.scheme_callable_types_compatible(expected, actual) {
            let exact_null = match self.type_constraints.call_argument_has_closed_null_evidence(actual) {
                Ok(evidence) => evidence,
                Err(error) => {
                    self.report_inferred_type_conflict(crate::sema::constraints::ConstraintConflict {
                        expected: expected.clone(), actual: actual.clone(), initializer: None,
                        established: None, contribution: span, resolution_error: Some(error),
                    });
                    return;
                }
            };
            if let Some(authority) = authority.instance().filter(|instance|
                exact_null || actual.contains_any() || !instance.wrappers.is_empty() || !instance.applications.is_empty()) {
                if let Err(conflict) = self.type_constraints.constrain_checked_call_argument(authority, expected, actual, span) {
                    if conflict.resolution_error.is_some() || !actual.contains_any() {
                        self.report_inferred_type_conflict(conflict);
                    } else {
                        self.reject_dynamic_use("a generic function argument", None, span);
                    }
                    return;
                }
            } else {
                self.expect_type(expected, actual, span);
            }
        }
        if let Err(error) = self.type_constraints.solve_wrapper_operations() {
            let code = match &error {
                crate::sema::constraints::WrapperConstraintError::Invalid { operation, .. } => match operation {
                    crate::sema::wrapper_operation::WrapperOperationKind::Field => DiagnosticCode::CheckNullSafeField,
                    crate::sema::wrapper_operation::WrapperOperationKind::Fallback => DiagnosticCode::CheckResultFallback,
                    crate::sema::wrapper_operation::WrapperOperationKind::Handler => DiagnosticCode::CheckFallbackBlockResult,
                    crate::sema::wrapper_operation::WrapperOperationKind::Capture => DiagnosticCode::CheckTryError,
                    crate::sema::wrapper_operation::WrapperOperationKind::KeyValidation => DiagnosticCode::CheckTryError,
                    crate::sema::wrapper_operation::WrapperOperationKind::Propagation | crate::sema::wrapper_operation::WrapperOperationKind::ProcCompletion => DiagnosticCode::CheckTryError,
                },
                crate::sema::constraints::WrapperConstraintError::Effects(_) => DiagnosticCode::CheckEffectViolation,
                _ => DiagnosticCode::CheckCallableMismatch,
            };
            self.error(span, &format!("wrapper argument relationship failed: {error}"), code);
        }
    }

    pub(super) fn check_api_arg_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        index: usize,
        expected: Option<&Type>,
    ) -> Type {
        let Some(arg) = args.get(index) else {
            return Type::Unknown;
        };
        let previous = self.boundary.expected_schema.take();
        self.boundary.expected_schema =
            expected.map(|_| crate::sema::constants::SchemaExpectation::default());
        let actual = self.check_call_arg_arena(arena, source, &arg.kind, expected);
        self.boundary.expected_schema = previous;
        if let Some(expected) = expected {
            self.expect_type(expected, &actual, call_arg_span_arena(arena, &arg.kind));
        }
        actual
    }

    pub(super) fn check_optional_api_arg_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        arg: Option<&ArenaCallArgKind>,
        expected: Option<&Type>,
    ) -> Type {
        let Some(arg) = arg else {
            return Type::Unknown;
        };
        let actual = self.check_call_arg_arena(arena, source, arg, expected);
        if let Some(expected) = expected {
            self.expect_type(expected, &actual, call_arg_span_arena(arena, arg));
        }
        actual
    }

    pub(super) fn check_module_overload_args_arena<'a>(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        _module: &str,
        _name: &str,
        args: &[ArenaCallArg],
        overloads: &'a [ModuleFnSig],
        span: Span,
    ) -> &'a ModuleFnSig {
        let actuals = args
            .iter()
            .enumerate()
            .map(|(index, arg)| {
                let expected = common_module_overload_expected_arena(args, overloads, index);
                let previous = self.boundary.expected_schema.take();
                if overloads.len() == 1 && expected.is_some() {
                    self.boundary.expected_schema =
                        Some(crate::sema::constants::SchemaExpectation::default());
                }
                let actual = self.check_call_arg_arena(arena, source, &arg.kind, expected.as_ref());
                self.boundary.expected_schema = previous;
                actual
            })
            .collect::<Vec<_>>();
        let matches = overloads
            .iter()
            .filter(|sig| module_overload_matches_arena(args, &actuals, sig))
            .collect::<Vec<_>>();
        if let Some(sig) = matches.first() {
            if matches.len() > 1 && actuals.iter().all(|ty| !matches!(ty, Type::Unknown)) {
                self.error(
                    span,
                    "ambiguous standard API overload",
                    DiagnosticCode::CheckAmbiguousOverload,
                );
            }
            return sig;
        }

        let arity_matches = overloads
            .iter()
            .filter(|sig| module_sig_accepts_arity(args.len(), sig))
            .collect::<Vec<_>>();
        if arity_matches.is_empty() {
            self.error(
                span,
                "incorrect standard API arity",
                DiagnosticCode::CheckArity,
            );
            return &overloads[0];
        }
        if arity_matches
            .iter()
            .all(|sig| !module_sig_accepts_names_arena(args, sig))
        {
            if let Some((_, arg)) = args.iter().enumerate().find(|(index, arg)| {
                arity_matches
                    .iter()
                    .all(|sig| !module_sig_accepts_arg_name_at_arena(&arg.kind, *index, sig))
            }) {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    "unexpected named parameter",
                    DiagnosticCode::CheckNamedArg,
                );
            } else {
                self.error(
                    span,
                    "unexpected named parameter",
                    DiagnosticCode::CheckNamedArg,
                );
            }
            return arity_matches[0];
        }

        let mut dynamic_boundary = false;
        for (index, (arg, actual)) in args.iter().zip(&actuals).enumerate() {
            if let Some(expected) = common_module_overload_expected_arena(args, overloads, index)
                && actual.any_flows_to_concrete(&expected)
            {
                self.expect_type(&expected, actual, call_arg_span_arena(arena, &arg.kind));
                dynamic_boundary = true;
            }
        }
        if dynamic_boundary {
            return arity_matches[0];
        }

        self.error(
            span,
            "no standard API overload matches argument types",
            DiagnosticCode::CheckTypeMismatch,
        );
        arity_matches[0]
    }

    pub(super) fn check_module_overload_with_checked_argument<'a>(
        &mut self,
        argument: CheckedCallArgument<'_>,
        overloads: &'a [ModuleFnSig],
        span: Span,
    ) -> Option<&'a ModuleFnSig> {
        let arity_matches = overloads
            .iter()
            .filter(|sig| module_sig_accepts_arity(1, sig))
            .collect::<Vec<_>>();
        if arity_matches.is_empty() {
            self.error(
                span,
                "incorrect standard API arity",
                DiagnosticCode::CheckArity,
            );
            return None;
        }
        let matches = arity_matches
            .iter()
            .copied()
            .filter(|sig| {
                sig.params.first().is_some_and(|param| {
                    module_arg_matches_param_arena(argument.ty, &param.ty)
                })
            })
            .collect::<Vec<_>>();
        if let Some(sig) = matches.first() {
            if matches.len() > 1 && !matches!(argument.ty, Type::Unknown) {
                self.error(
                    span,
                    "ambiguous standard API overload",
                    DiagnosticCode::CheckAmbiguousOverload,
                );
            }
            return Some(*sig);
        }
        let expected = arity_matches
            .iter()
            .filter_map(|sig| sig.params.first().map(|param| &param.ty))
            .try_fold(None, |expected: Option<&Type>, ty| match expected {
                Some(current) if current != ty => Err(()),
                Some(current) => Ok(Some(current)),
                None => Ok(Some(ty)),
            })
            .ok()
            .flatten();
        if let Some(expected) = expected
            && argument.ty.any_flows_to_concrete(expected)
        {
            self.expect_type(expected, argument.ty, argument.span);
            return Some(arity_matches[0]);
        }
        self.error(
            span,
            "no standard API overload matches argument types",
            DiagnosticCode::CheckTypeMismatch,
        );
        Some(arity_matches[0])
    }

    pub(super) fn check_stream_callable_module_overload(
        &mut self,
        argument: CheckedCallArgument<'_>,
        overloads: &[ModuleFnSig],
        span: Span,
    ) -> bool {
        let matches = overloads
            .iter()
            .filter(|signature| {
                module_sig_accepts_arity(1, signature)
                    && signature
                        .params
                        .first()
                        .is_some_and(|parameter| argument.ty.matches_expected(&parameter.ty))
            })
            .count();
        if matches == 1 {
            true
        } else {
            self.error(
                span,
                "stage callable requires a unique checked one-item signature",
                DiagnosticCode::CheckStreamCallableSignature,
            );
            false
        }
    }

    pub(super) fn check_expr_arg_list_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        params: &[Type],
        span: Span,
    ) {
        if args.len() != params.len() {
            self.error(span, "incorrect function arity", DiagnosticCode::CheckArity);
        }
        for (arg, expected) in args.iter().zip(params) {
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, Some(expected));
            self.expect_type(expected, &actual, call_arg_span_arena(arena, &arg.kind));
        }
    }

    fn check_parameter_arg_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        arg: &ArenaCallArgKind,
        expected: &Type,
        parameter: &FunctionParamSig,
    ) -> Type {
        let schema = if parameter.rest {
            parameter
                .schema_expectation
                .as_ref()
                .and_then(|schema| {
                    schema
                        .children
                        .get(&crate::sema::constants::SchemaComponent::Item)
                })
                .cloned()
        } else {
            parameter.schema_expectation.clone()
        };
        let previous = self.boundary.expected_schema.replace(schema.unwrap_or_default());
        let actual = self.check_call_arg_arena(arena, source, arg, Some(expected));
        self.boundary.expected_schema = previous;
        actual
    }

    fn static_splice_item_context(
        &mut self,
        params: &[FunctionParamSig],
        first_slot: usize,
        item_count: usize,
        span: Span,
    ) -> Option<(
        Type,
        Vec<(Type, Option<crate::sema::constants::SchemaExpectation>)>,
    )> {
        let mut common_base = None;
        let mut contexts = Vec::with_capacity(item_count);
        for offset in 0..item_count {
            let slot = first_slot.saturating_add(offset);
            let parameter = params
                .get(slot)
                .or_else(|| params.last().filter(|parameter| parameter.rest))?;
            let (ty, schema) = if parameter.rest {
                let schema = parameter
                    .schema_expectation
                    .as_ref()
                    .and_then(|schema| {
                        schema
                            .children
                            .get(&crate::sema::constants::SchemaComponent::Item)
                    })
                    .cloned();
                (self.function_argument_type(parameter, span), schema)
            } else {
                (parameter.ty.clone(), parameter.schema_expectation.clone())
            };
            let base = ty.unvalidated().clone();
            if matches!(base, Type::Any | Type::Unknown | Type::Invalid) {
                return None;
            }
            if let Some(common_base) = &common_base {
                if common_base != &base {
                    return None;
                }
            } else {
                common_base = Some(base);
            }
            contexts.push((ty, schema));
        }
        common_base.map(|base| (base, contexts))
    }

    fn check_static_splice_with_item_contexts(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        expression: ExprId,
        item_ty: Type,
        item_contexts: Vec<(Type, Option<crate::sema::constants::SchemaExpectation>)>,
        authority: CallArgumentAuthority<'_>,
    ) -> Type {
        let expected = Type::List(Box::new(item_ty.clone()));
        let previous = self
            .call_splice_item_contexts
            .replace(CallSpliceItemContexts { expression, items: item_contexts, authority: match authority { CallArgumentAuthority::Immediate => CallSpliceAuthority::Immediate, CallArgumentAuthority::KnownCallable { .. } => CallSpliceAuthority::KnownCallable } });
        let actual = self.check_expr_arena(arena, source, expression, Some(&expected));
        let contexts = std::mem::replace(&mut self.call_splice_item_contexts, previous)
            .expect("a checked static splice retains its item contexts");
        if matches!(authority, CallArgumentAuthority::KnownCallable { .. })
            && let crate::syntax::arena::ArenaExprKind::List(elements) = arena.arena.expr(expression).kind
        {
            for (element, (expected, _)) in arena.arena.list_elements(elements).zip(contexts.items) {
                let span = arena.arena.expr(element.value).span;
                if let Some(actual) = self.expr_types.get(&span).cloned() {
                    self.expect_function_call_argument(authority, &expected, &actual, span);
                }
            }
        }
        actual
    }

    fn check_function_arg_list_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        params: &[FunctionParamSig],
        span: Span,
        authority: CallArgumentAuthority<'_>,
    ) {
        if args
            .iter()
            .any(|arg| matches!(arg.kind, ArenaCallArgKind::Named { .. }))
        {
            use crate::sema::arguments::{bind_static_arguments, expand_named_arguments};
            let callable = callable_parameter_types(params);
            let expanded = expand_named_arguments(arena, args, |_| None)
                .expect("named spreads expanded before function binding");
            match bind_static_arguments(&callable, &expanded).inspect(|binding| {
                self.argument_bindings.insert(
                    span,
                    super::CheckedArguments {
                        callable_entry: None,
                        argument_slots: binding.argument_slots.clone(),
                    },
                );
            }) {
                Err(error) => {
                    self.error(error.span, &error.message, DiagnosticCode::CheckNamedArg);
                    for arg in args {
                        self.check_call_arg_arena(arena, source, &arg.kind, None);
                    }
                }
                Ok(binding) => {
                    for (arg, slot) in args.iter().zip(binding.argument_slots) {
                        let param = &params[slot];
                        let expected = if param.rest {
                            match &param.ty {
                                Type::List(item) => item.as_ref(),
                                other => other,
                            }
                        } else {
                            &param.ty
                        };
                        if let ArenaCallArgKind::Splice { value, .. } = arg.kind {
                            let actual = self.check_expr_arena(arena, source, value, None);
                            self.expect_function_call_argument(authority,
                                &Type::List(Box::new(expected.clone())),
                                &actual,
                                call_arg_span_arena(arena, &arg.kind),
                            );
                        } else {
                            let actual = self.check_parameter_arg_arena(
                                arena, source, &arg.kind, expected, param,
                            );
                            self.expect_function_call_argument(authority,
                                expected,
                                &actual,
                                call_arg_span_arena(arena, &arg.kind),
                            );
                        }
                    }
                }
            }
            return;
        }
        let has_splice = args.iter().any(|arg| {
            matches!(
                arg.kind,
                ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. }
            )
        });
        let argument_count = static_positional_argument_count(arena, args);
        if let Some(message) = argument_count
            .or_else(|| (!has_splice).then_some(args.len()))
            .and_then(|count| function_arity_error(params, count))
        {
            self.error(
                span,
                &message,
                DiagnosticCode::CheckArity,
            );
        }

        let mut index = 0;
        let mut can_check_following_positionals = true;
        let mut pending_static_items = Vec::new();
        let mut pending_static_item_index = 0;
        for (param_index, param) in params.iter().enumerate() {
            if param.rest {
                let item_ty = self.function_argument_type(param, span);
                for (ty, item_span) in &pending_static_items[pending_static_item_index..] {
                    if let Some(ty) = ty {
                        self.expect_function_call_argument(authority, &item_ty, ty, *item_span);
                    }
                }
                for arg in &args[index..] {
                    match &arg.kind {
                        ArenaCallArgKind::Splice { value, span }
                        | ArenaCallArgKind::NamedSpread { value, span } => {
                            let splice_span = arena.arena.span(*span);
                            let actual = self.check_expr_arena(arena, source, *value, None);
                            match actual.into_unvalidated() {
                                Type::List(item) if authority.instance().is_some() && item.contains_any() => {
                                    self.expect_function_call_argument(authority, &item_ty, &item, splice_span);
                                }
                                Type::List(item) if item.matches_expected(&item_ty) => {}
                                Type::List(_) => self.error(
                                    splice_span,
                                    "splice item type does not match rest parameter",
                                    DiagnosticCode::CheckTypeMismatch,
                                ),
                                Type::Any => self.reject_dynamic_use("an argument splice", None, arena.arena.expr(*value).span),
                                Type::Unknown => {}
                                _ => self.error(
                                    splice_span,
                                    "`@` splices require List values",
                                    DiagnosticCode::CheckSpliceTarget,
                                ),
                            }
                        }
                        _ => {
                            let actual = self.check_parameter_arg_arena(
                                arena, source, &arg.kind, &item_ty, param,
                            );
                            self.expect_function_call_argument(authority,
                                &item_ty,
                                &actual,
                                call_arg_span_arena(arena, &arg.kind),
                            );
                        }
                    }
                }
                return;
            }
            if let Some((ty, item_span)) = pending_static_items.get(pending_static_item_index) {
                if let Some(ty) = ty {
                    self.expect_function_call_argument(authority, &param.ty, ty, *item_span);
                }
                pending_static_item_index += 1;
                continue;
            }
            while let Some(arg) = args.get(index) {
                if let ArenaCallArgKind::Splice { value, span }
                | ArenaCallArgKind::NamedSpread { value, span } = &arg.kind
                {
                    let splice_span = arena.arena.span(*span);
                    let static_elements = if matches!(arg.kind, ArenaCallArgKind::Splice { .. })
                        && can_check_following_positionals
                    {
                        static_positional_list_elements(arena, *value)
                    } else {
                        None
                    };
                    let item_context = static_elements
                        .as_ref()
                        .filter(|elements| !elements.is_empty())
                        .and_then(|elements| {
                            self.static_splice_item_context(
                                params,
                                param_index,
                                elements.len(),
                                splice_span,
                            )
                        });
                    let contextually_checked = item_context.is_some();
                    let actual = if let Some((item_ty, item_contexts)) = item_context {
                        self.check_static_splice_with_item_contexts(
                            arena,
                            source,
                            *value,
                            item_ty,
                            item_contexts,
                            authority,
                        )
                    } else {
                        self.check_expr_arena(arena, source, *value, None)
                    };
                    if let Some(elements) = static_elements
                        && let Type::List(item) = actual.clone().into_unvalidated()
                    {
                        if elements.is_empty() {
                            index += 1;
                            continue;
                        }
                        pending_static_items.extend(elements.iter().map(|element| {
                            (
                                (!contextually_checked).then(|| item.as_ref().clone()),
                                arena.arena.expr(*element).span,
                            )
                        }));
                        if let Some((ty, item_span)) =
                            pending_static_items.get(pending_static_item_index)
                        {
                            if let Some(ty) = ty {
                                self.expect_function_call_argument(authority, &param.ty, ty, *item_span);
                            }
                            pending_static_item_index += 1;
                        }
                        index += 1;
                        break;
                    }
                    if actual == Type::Any {
                        self.reject_dynamic_use("an argument splice", None, arena.arena.expr(*value).span);
                    } else if !matches!(
                        actual.unvalidated(),
                        Type::List(_) | Type::Unknown
                    ) {
                        self.error(
                            splice_span,
                            "`@` splices require List values",
                            DiagnosticCode::CheckSpliceTarget,
                        );
                    }
                    can_check_following_positionals = false;
                    index += 1;
                    break;
                }
                if let ArenaCallArgKind::Named { name, .. } = &arg.kind
                    && *name != param.name
                {
                    self.error(
                        call_arg_span_arena(arena, &arg.kind),
                        "unexpected named parameter",
                        DiagnosticCode::CheckNamedArg,
                    );
                }
                let expected = can_check_following_positionals.then_some(&param.ty);
                let actual = if let Some(expected) = expected {
                    self.check_parameter_arg_arena(arena, source, &arg.kind, expected, param)
                } else {
                    self.check_call_arg_arena(arena, source, &arg.kind, None)
                };
                if let Some(expected) = expected {
                    self.expect_function_call_argument(authority, expected, &actual, call_arg_span_arena(arena, &arg.kind));
                }
                index += 1;
                break;
            }
        }
    }

    fn function_argument_type(&mut self, param: &FunctionParamSig, span: Span) -> Type {
        if !param.rest {
            return param.ty.clone();
        }
        match &param.ty {
            Type::List(item) => item.as_ref().clone(),
            Type::Any | Type::Unknown => param.ty.clone(),
            _ => {
                self.error(
                    span,
                    "rest parameter requires List",
                    DiagnosticCode::CheckRestType,
                );
                Type::Unknown
            }
        }
    }

    pub(super) fn check_module_sig_args_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        sig: &ModuleFnSig,
        span: Span,
    ) {
        self.check_module_sig_args_with_schema_arena(arena, source, args, sig, span, &[]);
    }

    pub(super) fn check_module_sig_args_with_schema_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        sig: &ModuleFnSig,
        span: Span,
        schemas: &[Option<crate::sema::constants::SchemaExpectation>],
    ) {
        self.check_module_sig_args_with_checked_argument_arena(
            arena, source, args, sig, span, schemas, None,
        );
    }

    pub(super) fn check_module_sig_args_with_checked_argument_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        sig: &ModuleFnSig,
        span: Span,
        schemas: &[Option<crate::sema::constants::SchemaExpectation>],
        checked_argument: Option<CheckedCallArgument<'_>>,
    ) {
        let _ = self.check_retained_signature_arguments_arena(arena, source, args, sig, span, schemas, checked_argument);
    }

    pub(super) fn check_retained_signature_arguments_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        sig: &ModuleFnSig,
        span: Span,
        schemas: &[Option<crate::sema::constants::SchemaExpectation>],
        checked_argument: Option<CheckedCallArgument<'_>>,
    ) -> Option<CheckedBoundArguments> {
        let params = crate::sema::builtin_templates::callable_parameters(sig);
        let mut expanded = match crate::sema::arguments::expand_named_arguments(arena, args, |_| None) {
            Ok(expanded) => expanded,
            Err(error) => {
                self.error(error.span, &error.message, DiagnosticCode::CheckNamedSpread);
                return None;
            }
        };
        if let Some(argument) = checked_argument {
            assert!(args.is_empty(), "a checked argument replaces source arguments");
            expanded.push(expanded_checked_argument(argument));
        }
        let binding = match crate::sema::arguments::bind_static_arguments(&params, &expanded) {
            Ok(binding) => binding,
            Err(error) => {
                let required = params
                    .iter()
                    .filter(|parameter| !parameter.defaulted)
                    .count();
                let code = if expanded.len() < required || expanded.len() > params.len() {
                    DiagnosticCode::CheckArity
                } else {
                    DiagnosticCode::CheckNamedArg
                };
                self.error(
                    if args.is_empty() { span } else { error.span },
                    &error.message,
                    code,
                );
                for arg in args {
                    self.check_call_arg_arena(arena, source, &arg.kind, None);
                }
                if let Some(argument) = checked_argument
                    && let Some(param) = params.first()
                {
                    let expected = self
                        .type_constraints
                        .resolve(&param.ty)
                        .unwrap_or_else(|_| param.ty.clone());
                    self.expect_type(&expected, argument.ty, argument.span);
                }
                return None;
            }
        };
        let mut contexts = Vec::with_capacity(expanded.len());
        self.check_bound_api_argument_labels(arena, span, &expanded, &binding, sig);
        if let Some(argument) = checked_argument {
            if let Some(&slot) = binding.argument_slots.first() {
                let expected = self
                    .type_constraints
                    .resolve(&params[slot].ty)
                    .unwrap_or_else(|_| params[slot].ty.clone());
                let schema = schemas.get(slot).cloned().flatten().unwrap_or_default();
                let previous_schema = self.boundary.expected_schema.replace(schema.clone());
                let actual = argument.ty;
                self.boundary.expected_schema = previous_schema;
                self.expect_api_argument(&expected, actual, argument.span);
                expanded[0].ty = actual.clone();
                contexts.push(CheckedArgumentContext { expected: Some(expected), schema });
            }
            return Some(CheckedBoundArguments { values: CheckedArgumentValues { arguments: expanded, contexts }, binding });
        }
        for (index, (arg, &slot)) in args.iter().zip(&binding.argument_slots).enumerate() {
            let expected = self
                .type_constraints
                .resolve(&params[slot].ty)
                .unwrap_or_else(|_| params[slot].ty.clone());
            let schema = schemas.get(slot).cloned().flatten().unwrap_or_default();
            let previous_schema = self.boundary.expected_schema.replace(schema.clone());
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, Some(&expected));
            self.boundary.expected_schema = previous_schema;
            self.expect_api_argument(&expected, &actual, call_arg_span_arena(arena, &arg.kind));
            expanded[index].ty = actual;
            contexts.push(CheckedArgumentContext { expected: Some(expected), schema });
        }
        (contexts.len() == expanded.len()).then_some(CheckedBoundArguments {
            values: CheckedArgumentValues { arguments: expanded, contexts }, binding,
        })
    }

    fn check_bound_api_argument_labels(
        &mut self,
        arena: &ArenaProgram,
        span: Span,
        expanded: &[crate::sema::arguments::ExpandedArgument],
        binding: &crate::sema::arguments::StaticArgumentBinding,
        sig: &ModuleFnSig,
    ) {
        for (value, &slot) in expanded.iter().zip(&binding.argument_slots) {
            if value.name.is_some()
                || sig.params[slot].label != crate::modules::signature::LabelRule::Required
            {
                continue;
            }
            let label = sig.params[slot].name;
            let mut diagnostic = crate::diagnostic::Diagnostic::error(format!(
                "this argument is passed by position; it takes the label `{label}`"
            ))
                .with_code(DiagnosticCode::CheckNamedArg)
                .with_label(crate::diagnostic::Label::primary(
                    value.span,
                    format!("write `{label}: ...`"),
                ))
                .with_note(format!(
                    "`{label}:` says which operand this is, so two operands of one type cannot be swapped unnoticed"
                ));
            if let crate::sema::arguments::ArgumentValueSource::Expression(expression) = value.value {
                let start = arena.paren_groups.iter()
                    .filter(|(candidate, group)| *candidate == expression
                        && group.source_id == value.span.source_id
                        && span.start() <= group.start() && group.end() <= span.end())
                    .map(|(_, group)| group.start())
                    .min().unwrap_or(value.span.start());
                diagnostic = diagnostic.with_fix_hint(crate::diagnostic::FixHint::replacement(
                    Span::new(value.span.source_id, start, start),
                    format!("pass `{label}` by name"),
                    format!("{label}: "),
                ));
            }
            self.diagnostics.push(diagnostic);
        }
    }

    fn expect_api_argument(&mut self, expected: &Type, actual: &Type, span: Span) {
        if expected == &Type::Path && is_path_like_type(actual) || expected.accepts_readonly_argv_argument(actual) { return; }
        self.expect_type(expected, actual, span);
    }

    pub(super) fn check_argument_values_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        contexts: Vec<CheckedArgumentContext>,
    ) -> Option<CheckedArgumentValues> {
        assert_eq!(args.len(), contexts.len());
        for (argument, context) in args.iter().zip(&contexts) {
            let previous = self.boundary.expected_schema.replace(context.schema.clone());
            self.check_call_arg_arena(arena, source, &argument.kind, context.expected.as_ref());
            self.boundary.expected_schema = previous;
        }
        match crate::sema::arguments::expand_named_arguments(arena, args, |expression| {
            self.expr_types.get(&arena.arena.expr(expression).span).cloned()
        }) {
            Ok(arguments) => {
                let contexts = arguments.iter().map(|argument| contexts[argument.entry_index].clone()).collect();
                Some(CheckedArgumentValues { arguments, contexts })
            }
            Err(error) => {
                self.error(error.span, &error.message, DiagnosticCode::CheckNamedSpread);
                None
            }
        }
    }

    pub(super) fn retain_original_method_arguments(
        &self,
        arena: &ArenaProgram,
        source: &super::method::HostCallSource,
        values: &mut CheckedArgumentValues,
    ) {
        let super::method::HostCallSource::Expression(call) = source else { return; };
        let crate::syntax::arena::ArenaExprKind::Call { args, .. } = arena.arena.expr(*call).kind else { panic!("method evidence belongs to its original call") };
        let originals = crate::sema::arguments::expand_named_arguments(arena, arena.arena.call_args(args), |expression| {
            self.expr_types.get(&arena.arena.expr(expression).span).cloned()
        }).expect("checked original method arguments retain their static layout");
        assert_eq!(originals.len(), values.arguments.len());
        for (original, checked) in originals.into_iter().zip(&mut values.arguments) {
            assert_eq!(original.name, checked.name);
            checked.entry_index = original.entry_index;
            checked.value = original.value;
            checked.span = original.span;
        }
    }

    pub(super) fn complete_checked_method_arguments(
        &mut self,
        arena: &ArenaProgram,
        span: Span,
        values: CheckedArgumentValues,
        binding: crate::sema::arguments::StaticArgumentBinding,
        signature: &ModuleFnSig,
        schemas: &[Option<crate::sema::constants::SchemaExpectation>],
    ) -> Result<CheckedBoundArguments, CheckedArgumentValues> {
        assert_eq!(values.arguments.len(), values.contexts.len());
        assert_eq!(values.arguments.len(), binding.argument_slots.len());
        if values.contexts.iter().zip(&binding.argument_slots).any(|(context, &slot)| {
            context.schema != schemas.get(slot).cloned().flatten().unwrap_or_default()
        }) {
            return Err(values);
        }
        self.check_bound_host_arguments(arena, span, &values.arguments, &binding, signature);
        Ok(CheckedBoundArguments { values, binding })
    }

    pub(super) fn check_bound_host_arguments(
        &mut self,
        arena: &ArenaProgram,
        span: Span,
        arguments: &[crate::sema::arguments::ExpandedArgument],
        binding: &crate::sema::arguments::StaticArgumentBinding,
        signature: &ModuleFnSig,
    ) {
        assert_eq!(arguments.len(), binding.argument_slots.len());
        self.check_bound_api_argument_labels(arena, span, arguments, binding, signature);
        for (argument, &slot) in arguments.iter().zip(&binding.argument_slots) {
            let parameter = &signature.params[slot];
            let expected = self.type_constraints.resolve(&parameter.ty).unwrap_or_else(|_| parameter.ty.clone());
            let actual = self.type_constraints.resolve(&argument.ty).unwrap_or_else(|_| argument.ty.clone());
            self.expect_api_argument(&expected, &actual, argument.span);
        }
    }

    pub(super) fn check_call_arg_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        arg: &ArenaCallArgKind,
        expected: Option<&Type>,
    ) -> Type {
        match arg {
            ArenaCallArgKind::Positional(value) => {
                self.check_expr_arena(arena, source, *value, expected)
            }
            ArenaCallArgKind::Splice { value, span }
            | ArenaCallArgKind::NamedSpread { value, span } => {
                let span = arena.arena.span(*span);
                self.error(
                    span,
                    "`@` splice is not valid here",
                    DiagnosticCode::CheckCallSplice,
                );
                self.check_expr_arena(arena, source, *value, None)
            }
            ArenaCallArgKind::Named { name, value, span } => {
                let first_diagnostic = self.diagnostics.len();
                let ty = self.check_expr_arena(arena, source, *value, expected);
                let argument_span = arena.arena.span(*span);
                let value_span = arena.arena.expr(*value).span;
                if *name == "ARGV" && value_span.start() == argument_span.start() {
                    // A punned argument writes its parameter label and binding
                    // once. Expand the value while retaining the original label.
                    for diagnostic in &mut self.diagnostics[first_diagnostic..] {
                        if diagnostic.code == Some(DiagnosticCode::CheckCompatibilityVocabulary) {
                            for hint in &mut diagnostic.fix_hints {
                                if hint.span == Some(value_span)
                                    && hint.replacement.as_deref() == Some("args")
                                {
                                    hint.span = Some(argument_span);
                                    hint.replacement = Some("ARGV: args".to_string());
                                }
                            }
                        }
                    }
                }
                ty
            }
        }
    }

    pub(super) fn check_path_like_arg_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        arg: Option<&ArenaCallArgKind>,
        span: Span,
    ) {
        let Some(arg) = arg else {
            self.error(span, "incorrect function arity", DiagnosticCode::CheckArity);
            return;
        };
        let ty = self.check_call_arg_arena(arena, source, arg, Some(&Type::Path));
        self.check_checked_path_like_argument(&ty, call_arg_span_arena(arena, arg));
    }

    pub(super) fn check_checked_path_like_argument(&mut self, ty: &Type, span: Span) {
        if ty == &Type::Any {
            self.expect_type(&Type::Path, ty, span);
        } else if !is_path_like_type(ty) {
            self.error(
                span,
                "expected Path",
                DiagnosticCode::CheckTypeMismatch,
            );
        }
    }

    pub(super) fn check_module_command_args_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCommandArg],
        sig: &'static ModuleFnSig,
        span: Span,
    ) {
        let mut positionals = Vec::new();
        let mut flags = FxHashSet::default();
        let mut bindings = vec![None; args.len()];
        for (index, arg) in args.iter().enumerate() {
            if let Some(flag) = command_bool_flag_name_arena(arena, source, arg) {
                let arg_span = arena.arena.span(arg.span);
                let Some(slot) = sig.params.iter().position(|param| param.name == flag) else {
                    self.error(
                        arg_span,
                        "unknown module command flag",
                        DiagnosticCode::CheckModuleCommandFlag,
                    );
                    continue;
                };
                let param = &sig.params[slot];
                if !(param.defaulted && param.ty == Type::Bool) {
                    self.error(
                        arg_span,
                        "module command flag must target a defaulted Bool parameter",
                        DiagnosticCode::CheckModuleCommandFlag,
                    );
                    continue;
                }
                if !flags.insert(flag.to_string()) {
                    self.error(
                        arg_span,
                        "duplicate module command flag",
                        DiagnosticCode::CheckModuleCommandFlag,
                    );
                }
                bindings[index] = Some((slot, CheckedCommandArgument::BooleanFlag));
                continue;
            }
            if matches!(
                arg.kind,
                crate::syntax::arena::ArenaCommandArgKind::SpliceName(_)
                    | crate::syntax::arena::ArenaCommandArgKind::SpliceExpr(_)
            ) {
                let arg_span = arena.arena.span(arg.span);
                self.error(
                    arg_span,
                    "module commands do not accept splices",
                    DiagnosticCode::CheckModuleCommandArg,
                );
                continue;
            }
            positionals.push((index, arg));
        }

        let required = sig.params.iter().filter(|param| !param.defaulted).count();
        let max_positionals = sig.params.len().saturating_sub(flags.len());
        if positionals.len() < required || positionals.len() > max_positionals {
            self.error(
                span,
                "incorrect module command arity",
                DiagnosticCode::CheckArity,
            );
        }
        let positional_params = sig
            .params
            .iter()
            .enumerate()
            .filter(|(_, param)| !flags.contains(param.name))
            .collect::<Vec<_>>();
        for ((index, arg), (slot, param)) in positionals.iter().zip(positional_params) {
            let actual = self.check_command_arg_arena(arena, source, arg, Some(&param.ty));
            bindings[*index] = Some((slot, match arg.kind {
                crate::syntax::arena::ArenaCommandArgKind::Word(_) => {
                    CheckedCommandArgument::Word(param.ty.clone())
                }
                _ => CheckedCommandArgument::Value,
            }));
            if param.ty == Type::Path && command_arg_can_be_path_like_arena(arg, &actual) {
                continue;
            }
            let arg_span = arena.arena.span(arg.span);
            self.expect_command_value_conversion(&param.ty, &actual, arg_span);
        }
        if let Some(bindings) = bindings.into_iter().collect::<Option<Vec<_>>>() {
            let (argument_slots, arguments) = bindings.into_iter().unzip();
            self.module_commands.insert(span, CheckedModuleCommand {
                call: CheckedApiCall {
                    receiver: None,
                    sig,
                    params: sig.params.iter().map(|param| param.ty.clone()).collect(),
                    argument_slots,
                },
                arguments,
            });
        }
    }
}

#[allow(dead_code)]
pub(super) fn call_arg_span_arena(arena: &ArenaProgram, kind: &ArenaCallArgKind) -> Span {
    match kind {
        ArenaCallArgKind::Positional(value) => arena.arena.expr(*value).span,
        ArenaCallArgKind::Splice { span, .. }
        | ArenaCallArgKind::NamedSpread { span, .. }
        | ArenaCallArgKind::Named { span, .. } => arena.arena.span(*span),
    }
}

#[allow(dead_code)]
pub(super) fn call_arg_expr_id_arena(kind: &ArenaCallArgKind) -> ExprId {
    match kind {
        ArenaCallArgKind::Positional(value)
        | ArenaCallArgKind::Splice { value, .. }
        | ArenaCallArgKind::NamedSpread { value, .. }
        | ArenaCallArgKind::Named { value, .. } => *value,
    }
}

#[allow(dead_code)]
pub(super) fn common_module_overload_expected_arena(
    args: &[ArenaCallArg],
    overloads: &[ModuleFnSig],
    index: usize,
) -> Option<Type> {
    args.get(index)?;
    let mut expected = None;
    for sig in overloads {
        let Some(bindings) = bind_module_args_arena(args, sig) else {
            continue;
        };
        let Some(param_index) = bindings
            .iter()
            .position(|bound| bound.is_some_and(|arg_index| arg_index == index))
        else {
            continue;
        };
        let param = &sig.params[param_index];
        match &expected {
            Some(current) if current != &param.ty => return None,
            Some(_) => {}
            None => expected = Some(param.ty.clone()),
        }
    }
    expected
}

#[allow(dead_code)]
pub(super) fn module_overload_matches_arena(
    args: &[ArenaCallArg],
    actuals: &[Type],
    sig: &ModuleFnSig,
) -> bool {
    let Some(bindings) = bind_module_args_arena(args, sig) else {
        return false;
    };
    bindings
        .iter()
        .enumerate()
        .all(|(param_index, arg_index)| match arg_index {
            Some(arg_index) => {
                module_arg_matches_param_arena(&actuals[*arg_index], &sig.params[param_index].ty)
            }
            None => sig.params[param_index].defaulted,
        })
}

#[allow(dead_code)]
pub(super) fn module_sig_accepts_names_arena(args: &[ArenaCallArg], sig: &ModuleFnSig) -> bool {
    bind_module_args_arena(args, sig).is_some()
}

#[allow(dead_code)]
pub(super) fn module_sig_accepts_arg_name_at_arena(
    arg: &ArenaCallArgKind,
    index: usize,
    sig: &ModuleFnSig,
) -> bool {
    match arg {
        ArenaCallArgKind::Positional(_) => sig.params.get(index).is_some(),
        ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. } => false,
        ArenaCallArgKind::Named { name, .. } => sig.params.iter().any(|param| param.name == *name),
    }
}

#[allow(dead_code)]
pub(super) fn bind_module_args_arena(
    args: &[ArenaCallArg],
    sig: &ModuleFnSig,
) -> Option<Vec<Option<usize>>> {
    let mut bindings = vec![None; sig.params.len()];
    let mut next_positional = 0usize;
    for (arg_index, arg) in args.iter().enumerate() {
        match &arg.kind {
            ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. } => return None,
            ArenaCallArgKind::Positional(_) => {
                while next_positional < bindings.len() && bindings[next_positional].is_some() {
                    next_positional += 1;
                }
                let binding = bindings.get_mut(next_positional)?;
                *binding = Some(arg_index);
            }
            ArenaCallArgKind::Named { name, .. } => {
                let param_index = sig.params.iter().position(|param| param.name == *name)?;
                if bindings[param_index].is_some() {
                    return None;
                }
                bindings[param_index] = Some(arg_index);
            }
        }
    }
    if sig
        .params
        .iter()
        .zip(&bindings)
        .any(|(param, binding)| !param.defaulted && binding.is_none())
    {
        return None;
    }
    Some(bindings)
}

#[allow(dead_code)]
pub(super) fn module_arg_matches_param_arena(actual: &Type, expected: &Type) -> bool {
    actual.matches_expected(expected) || (expected == &Type::Path && is_path_like_type(actual))
}

#[cfg(test)]
mod nullable_target_tests {
    use super::Checker;
    use crate::sema::{schemes::SchemeType, types::Type};

    #[test]
    fn nullable_argument_deferral_keeps_present_fields_validation_and_effect_authority() {
        for (source, expected) in [
            ("type Item = {value: Int}\npure accepts(value: Item?) -> Int { let _ = value; 0 }\npure read(value: Item?) -> Int { let _ = accepts(value); value.value }\n", crate::diagnostic::DiagnosticCode::CheckFieldAccess),
            ("type Item = {value: Int}\npure accepts(value: Item?) -> Int { let _ = value; 0 }\nlet raw: Any = {value: 7}\nlet invalid = accepts(raw)\n", crate::diagnostic::DiagnosticCode::CheckDynamicBoundary),
            ("type Item = {value: Int}\nproc accepts(value: Item?) [env] -> Int { let _ = env.get(\"XSH_NULLABLE_TARGET\"); let _ = value; 0 }\npure read(value) -> Int { let _ = accepts(value); value.value }\n", crate::diagnostic::DiagnosticCode::CheckPureEffect),
        ] {
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(expected)), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn nullable_argument_relationship_preserves_static_splices_and_named_slots() {
        for argument in ["value", "value: value", "@[value]"] {
            let source = format!("type Item = {{value: Int}}\npure accepts(value: Item?) -> Int {{ let _ = value; 0 }}\npure read(value) -> Int {{ let _ = accepts({argument}); value.value }}\n");
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
            let read = checked.checked_declarations.functions.values().find(|declaration| declaration.name == "read").unwrap();
            let parameter = &read.scheme.as_ref().unwrap().callable().params()[0].ty;
            assert!(matches!(parameter, SchemeType::Record { .. } | SchemeType::Ground(Type::Record(_))), "{argument}: {parameter:?}");
        }
    }

    #[test]
    fn nullable_target_requirements_generalize_before_any_callers() {
        let source = "type Item = {value: Int}\npure accepts(value: Item?) -> Int { let _ = value; 0 }\npure forward(value) { accepts(value) }\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let forward = checked.checked_declarations.functions.values().find(|declaration| declaration.name == "forward").unwrap();
        let scheme = forward.scheme.as_ref().unwrap();
        assert!(matches!(scheme.callable().params()[0].ty, SchemeType::Parameter(_)));
        assert!(scheme.callable().requirements().applications.iter().any(|application| matches!(application, crate::sema::callable_application::CallableApplication::KnownArguments(_))));
    }

    #[test]
    fn nullable_argument_context_keeps_the_checked_record_parameter_present() {
        let source = "type Item = {value: Int}\npure accepts(value: Item?) -> Int { let _ = value; 0 }\npure read(value) -> Int { let _ = accepts(value); value.value }\nlet result: Int = read({value: 7})\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let read = checked.checked_declarations.functions.values().find(|declaration| declaration.name == "read").unwrap();
        let parameter = &read.scheme.as_ref().unwrap().callable().params()[0].ty;
        assert!(matches!(parameter, SchemeType::Record { .. } | SchemeType::Ground(Type::Record(_))), "{parameter:?}");
    }
}

#[cfg(test)]
mod required_label_tests {
    use super::Checker;
    use crate::diagnostic::DiagnosticCode;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    // Exact edit ranges are checker facts, which human CLI output cannot expose.
    #[test]
    fn label_fix_inserts_before_grouping_and_preserves_operand_text() {
        for operand in [r#"p"destination""#, r#"(p"destination")"#, r#"((p"destination"))"#] {
            let source = format!("p\"α-source\".copy({operand})\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            let diagnostic = checked.diagnostics.iter()
                .find(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckNamedArg))
                .expect("required label diagnostic");
            let [hint] = diagnostic.fix_hints.as_slice() else {
                panic!("expected one label edit: {diagnostic:?}");
            };
            let span = hint.span.expect("label edit span");
            let start = source.find(operand).expect("operand in source");
            assert_eq!((span.start(), span.end()), (start, start));
            assert_eq!(hint.replacement.as_deref(), Some("to: "));
            let mut fixed = source.clone();
            fixed.replace_range(span.range(), hint.replacement.as_deref().unwrap());
            assert_eq!(fixed, source.replacen(operand, &format!("to: {operand}"), 1));
            let reparsed = Parser::parse_source_arena_only(SourceId::new(0), &fixed);
            assert!(reparsed.diagnostics.is_empty(), "{:?}", reparsed.diagnostics);
            let rechecked = Checker::check_arena(&reparsed.arena, &fixed);
            assert!(!rechecked.diagnostics.iter()
                .any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckNamedArg)),
                "{:?}", rechecked.diagnostics);
        }
    }
}
