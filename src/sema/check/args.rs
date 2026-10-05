#![allow(clippy::single_call_fn)]

use super::expr::is_path_like_type;
use super::{
    Checker, FunctionParamSig, FxHashSet, ModuleFnSig, Span, Type,
    command_arg_can_be_path_like_arena, command_bool_flag_name_arena,
};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{ArenaCallArg, ArenaCallArgKind, ArenaCommandArg, ArenaProgram, ExprId};

pub(super) fn module_sig_accepts_arity(arg_count: usize, sig: &ModuleFnSig) -> bool {
    let required = sig.params.iter().filter(|param| !param.defaulted).count();
    arg_count >= required && arg_count <= sig.params.len()
}

/// Arena-native mirror of every function above, operating on the arena's
/// call-argument representation instead of the old recursive AST's. Not
/// ported: `check_module_command_args` (a different construct — bareword
/// command args, not call expressions).
#[allow(dead_code)]
impl Checker {
    pub(super) fn check_standard_arg_shape_arena(
        &mut self,
        arena: &ArenaProgram,
        args: &[ArenaCallArg],
        names: &[&str],
        span: Span,
    ) {
        if args.len() != names.len() {
            self.error(
                span,
                "incorrect standard API arity",
                DiagnosticCode::CheckArity,
            );
        }
        for (index, arg) in args.iter().enumerate() {
            if let ArenaCallArgKind::Named { name, .. } = &arg.kind
                && names.get(index).is_none_or(|expected| name != expected)
            {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    "unexpected named parameter",
                    DiagnosticCode::CheckNamedArg,
                );
            }
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
        let previous = self.expected_schema.take();
        self.expected_schema =
            expected.map(|_| crate::sema::constants::SchemaExpectation::default());
        let actual = self.check_call_arg_arena(arena, source, &arg.kind, expected);
        self.expected_schema = previous;
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
                let previous = self.expected_schema.take();
                if overloads.len() == 1 && expected.is_some() {
                    self.expected_schema =
                        Some(crate::sema::constants::SchemaExpectation::default());
                }
                let actual = self.check_call_arg_arena(arena, source, &arg.kind, expected.as_ref());
                self.expected_schema = previous;
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
        let previous = std::mem::replace(&mut self.expected_schema, schema);
        let actual = self.check_call_arg_arena(arena, source, arg, Some(expected));
        self.expected_schema = previous;
        actual
    }

    pub(super) fn check_function_arg_list_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        params: &[FunctionParamSig],
        span: Span,
    ) {
        if args
            .iter()
            .any(|arg| matches!(arg.kind, ArenaCallArgKind::Named { .. }))
        {
            use crate::sema::arguments::{bind_static_arguments, expand_named_arguments};
            let callable = params
                .iter()
                .map(|param| crate::sema::types::CallableParamType {
                    name: param.name,
                    ty: param.ty.clone(),
                    defaulted: param.defaulted,
                    rest: param.rest,
                })
                .collect::<Vec<_>>();
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
                            self.expect_type(
                                &Type::List(Box::new(expected.clone())),
                                &actual,
                                call_arg_span_arena(arena, &arg.kind),
                            );
                        } else {
                            let actual = self.check_parameter_arg_arena(
                                arena, source, &arg.kind, expected, param,
                            );
                            self.expect_type(
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
        let required = params
            .iter()
            .filter(|param| !param.defaulted && !param.rest)
            .count();
        let max = if params.iter().any(|param| param.rest) {
            usize::MAX
        } else {
            params.len()
        };
        if !has_splice && (args.len() < required || args.len() > max) {
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
            self.error(
                span,
                &format!(
                    "incorrect function arity: expected {expected}, found {}",
                    args.len()
                ),
                DiagnosticCode::CheckArity,
            );
        }

        let mut index = 0;
        let mut can_check_following_positionals = true;
        for param in params {
            if param.rest {
                let item_ty = match &param.ty {
                    Type::List(item) => item.as_ref().clone(),
                    Type::Any => Type::Any,
                    Type::Unknown => Type::Unknown,
                    _ => {
                        self.error(
                            span,
                            "rest parameter requires List",
                            DiagnosticCode::CheckRestType,
                        );
                        Type::Unknown
                    }
                };
                for arg in &args[index..] {
                    match &arg.kind {
                        ArenaCallArgKind::Splice { value, span }
                        | ArenaCallArgKind::NamedSpread { value, span } => {
                            let splice_span = arena.arena.span(*span);
                            let actual = self.check_expr_arena(arena, source, *value, None);
                            match actual {
                                Type::List(item) if item.matches_expected(&item_ty) => {}
                                Type::List(_) => self.error(
                                    splice_span,
                                    "splice item type does not match rest parameter",
                                    DiagnosticCode::CheckTypeMismatch,
                                ),
                                Type::Any | Type::Unknown => {}
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
                            self.expect_type(
                                &item_ty,
                                &actual,
                                call_arg_span_arena(arena, &arg.kind),
                            );
                        }
                    }
                }
                return;
            }
            let Some(arg) = args.get(index) else {
                continue;
            };
            if let ArenaCallArgKind::Splice { value, span }
            | ArenaCallArgKind::NamedSpread { value, span } = &arg.kind
            {
                let splice_span = arena.arena.span(*span);
                let actual = self.check_expr_arena(arena, source, *value, None);
                if !matches!(actual, Type::List(_) | Type::Any | Type::Unknown) {
                    self.error(
                        splice_span,
                        "`@` splices require List values",
                        DiagnosticCode::CheckSpliceTarget,
                    );
                }
                can_check_following_positionals = false;
                index += 1;
                continue;
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
                self.expect_type(expected, &actual, call_arg_span_arena(arena, &arg.kind));
            }
            index += 1;
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
        let params = crate::sema::builtin_templates::callable_parameters(sig);
        let expanded = match crate::sema::arguments::expand_named_arguments(arena, args, |_| None) {
            Ok(expanded) => expanded,
            Err(error) => {
                self.error(error.span, &error.message, DiagnosticCode::CheckNamedSpread);
                return;
            }
        };
        let binding = match crate::sema::arguments::bind_static_arguments(&params, &expanded) {
            Ok(binding) => binding,
            Err(error) => {
                let required = params
                    .iter()
                    .filter(|parameter| !parameter.defaulted)
                    .count();
                let code = if args.len() < required || args.len() > params.len() {
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
                return;
            }
        };
        for (arg, slot) in args.iter().zip(binding.argument_slots) {
            let expected = self
                .type_constraints
                .resolve(&params[slot].ty)
                .unwrap_or_else(|_| params[slot].ty.clone());
            let previous_schema = self.expected_schema.clone();
            self.expected_schema = schemas
                .get(slot)
                .cloned()
                .flatten()
                .or_else(|| Some(crate::sema::constants::SchemaExpectation::default()));
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, Some(&expected));
            self.expected_schema = previous_schema;
            if expected == Type::Path && is_path_like_type(&actual) {
                continue;
            }
            self.expect_type(&expected, &actual, call_arg_span_arena(arena, &arg.kind));
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
        if !is_path_like_type(&ty) {
            self.error(
                call_arg_span_arena(arena, arg),
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
        sig: &ModuleFnSig,
        span: Span,
    ) {
        let mut positionals = Vec::new();
        let mut flags = FxHashSet::default();
        for arg in args {
            if let Some(flag) = command_bool_flag_name_arena(arena, source, arg) {
                let arg_span = arena.arena.span(arg.span);
                let Some(param) = sig.params.iter().find(|param| param.name == flag) else {
                    self.error(
                        arg_span,
                        "unknown module command flag",
                        DiagnosticCode::CheckModuleCommandFlag,
                    );
                    continue;
                };
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
            positionals.push(arg);
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
            .filter(|param| !flags.contains(param.name))
            .collect::<Vec<_>>();
        for (arg, param) in positionals.iter().zip(positional_params) {
            let actual = self.check_command_arg_arena(arena, source, arg, Some(&param.ty));
            if param.ty == Type::Path && command_arg_can_be_path_like_arena(arg, &actual) {
                continue;
            }
            let arg_span = arena.arena.span(arg.span);
            self.expect_command_value_conversion(&param.ty, &actual, arg_span);
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
            Some(arg_index) => module_arg_matches_param_arena(
                &actuals[*arg_index],
                &sig.params[param_index].ty,
            ),
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
