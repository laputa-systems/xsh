#![allow(clippy::single_call_fn)]

use super::expr::is_path_like_arena_expr;
use super::{
    Checker, FunctionParamSig, FxHashSet, ModuleFnSig, Span, Type,
    command_arg_can_be_path_like_arena, command_bool_flag_name_arena,
};
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
            self.error(span, "incorrect standard API arity", "check.arity");
        }
        for (index, arg) in args.iter().enumerate() {
            if let ArenaCallArgKind::Named { name, .. } = &arg.kind
                && names.get(index).is_none_or(|expected| name != expected)
            {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    "unexpected named parameter",
                    "check.named-arg",
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
        let actual = self.check_call_arg_arena(arena, source, &arg.kind, expected);
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
                self.check_call_arg_arena(arena, source, &arg.kind, expected.as_ref())
            })
            .collect::<Vec<_>>();
        let matches = overloads
            .iter()
            .filter(|sig| module_overload_matches_arena(arena, args, &actuals, sig))
            .collect::<Vec<_>>();
        if let Some(sig) = matches.first() {
            if matches.len() > 1 && actuals.iter().all(|ty| !matches!(ty, Type::Unknown)) {
                self.error(
                    span,
                    "ambiguous standard API overload",
                    "check.ambiguous-overload",
                );
            }
            return sig;
        }

        let arity_matches = overloads
            .iter()
            .filter(|sig| module_sig_accepts_arity(args.len(), sig))
            .collect::<Vec<_>>();
        if arity_matches.is_empty() {
            self.error(span, "incorrect standard API arity", "check.arity");
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
                    "check.named-arg",
                );
            } else {
                self.error(span, "unexpected named parameter", "check.named-arg");
            }
            return arity_matches[0];
        }

        self.error(
            span,
            "no standard API overload matches argument types",
            "check.type-mismatch",
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
            self.error(span, "incorrect function arity", "check.arity");
        }
        for (arg, expected) in args.iter().zip(params) {
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, Some(expected));
            self.expect_type(expected, &actual, call_arg_span_arena(arena, &arg.kind));
        }
    }

    pub(super) fn check_function_arg_list_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        params: &[FunctionParamSig],
        span: Span,
    ) {
        let has_splice = args
            .iter()
            .any(|arg| matches!(arg.kind, ArenaCallArgKind::Splice { .. }));
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
            self.error(span, "incorrect function arity", "check.arity");
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
                        self.error(span, "rest parameter requires List", "check.rest-type");
                        Type::Unknown
                    }
                };
                for arg in &args[index..] {
                    match &arg.kind {
                        ArenaCallArgKind::Splice { value, span } => {
                            let splice_span = arena.arena.span(*span);
                            let actual = self.check_expr_arena(arena, source, *value, None);
                            match actual {
                                Type::List(item) if item.matches_expected(&item_ty) => {}
                                Type::List(_) => self.error(
                                    splice_span,
                                    "splice item type does not match rest parameter",
                                    "check.type-mismatch",
                                ),
                                Type::Any | Type::Unknown => {}
                                _ => self.error(
                                    splice_span,
                                    "`@` splices require List values",
                                    "check.splice-target",
                                ),
                            }
                        }
                        _ => {
                            let actual =
                                self.check_call_arg_arena(arena, source, &arg.kind, Some(&item_ty));
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
            if let ArenaCallArgKind::Splice { value, span } = &arg.kind {
                let splice_span = arena.arena.span(*span);
                let actual = self.check_expr_arena(arena, source, *value, None);
                if !matches!(actual, Type::List(_) | Type::Any | Type::Unknown) {
                    self.error(
                        splice_span,
                        "`@` splices require List values",
                        "check.splice-target",
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
                    "check.named-arg",
                );
            }
            let expected = can_check_following_positionals.then_some(&param.ty);
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, expected);
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
        let Some(bindings) = bind_module_args_arena(args, sig) else {
            self.error(span, "incorrect standard API arity", "check.arity");
            for arg in args {
                self.check_call_arg_arena(arena, source, &arg.kind, None);
            }
            return;
        };
        for (index, param) in sig.params.iter().enumerate() {
            let Some(arg_index) = bindings[index] else {
                if !param.defaulted {
                    self.error(span, "incorrect standard API arity", "check.arity");
                }
                continue;
            };
            let arg = &args[arg_index];
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, Some(&param.ty));
            let expr_id = call_arg_expr_id_arena(&arg.kind);
            let kind = arena.arena.expr(expr_id).kind;
            if param.ty == Type::Path && is_path_like_arena_expr(&kind, &actual) {
                continue;
            }
            self.expect_type(&param.ty, &actual, call_arg_span_arena(arena, &arg.kind));
        }
        for (arg_index, arg) in args.iter().enumerate() {
            if !bindings.iter().flatten().any(|bound| *bound == arg_index) {
                if matches!(arg.kind, ArenaCallArgKind::Named { .. }) {
                    self.error(
                        call_arg_span_arena(arena, &arg.kind),
                        "unexpected named parameter",
                        "check.named-arg",
                    );
                }
                self.check_call_arg_arena(arena, source, &arg.kind, None);
            }
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
            ArenaCallArgKind::Splice { value, span } => {
                let span = arena.arena.span(*span);
                self.error(span, "`@` splice is not valid here", "check.call-splice");
                self.check_expr_arena(arena, source, *value, None)
            }
            ArenaCallArgKind::Named { value, .. } => {
                self.check_expr_arena(arena, source, *value, expected)
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
            self.error(span, "incorrect function arity", "check.arity");
            return;
        };
        let ty = self.check_call_arg_arena(arena, source, arg, None);
        let expr_id = call_arg_expr_id_arena(arg);
        let kind = arena.arena.expr(expr_id).kind;
        if !is_path_like_arena_expr(&kind, &ty) {
            self.error(
                call_arg_span_arena(arena, arg),
                "expected Path",
                "check.type-mismatch",
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
                        "check.module-command-flag",
                    );
                    continue;
                };
                if !(param.defaulted && param.ty == Type::Bool) {
                    self.error(
                        arg_span,
                        "module command flag must target a defaulted Bool parameter",
                        "check.module-command-flag",
                    );
                    continue;
                }
                if !flags.insert(flag.to_string()) {
                    self.error(
                        arg_span,
                        "duplicate module command flag",
                        "check.module-command-flag",
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
                    "check.module-command-arg",
                );
                continue;
            }
            positionals.push(arg);
        }

        let required = sig.params.iter().filter(|param| !param.defaulted).count();
        let max_positionals = sig.params.len().saturating_sub(flags.len());
        if positionals.len() < required || positionals.len() > max_positionals {
            self.error(span, "incorrect module command arity", "check.arity");
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
        ArenaCallArgKind::Splice { span, .. } | ArenaCallArgKind::Named { span, .. } => {
            arena.arena.span(*span)
        }
    }
}

#[allow(dead_code)]
pub(super) fn call_arg_expr_id_arena(kind: &ArenaCallArgKind) -> ExprId {
    match kind {
        ArenaCallArgKind::Positional(value)
        | ArenaCallArgKind::Splice { value, .. }
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
    arena: &ArenaProgram,
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
                let arg = &args[*arg_index];
                module_arg_matches_param_arena(
                    arena,
                    &arg.kind,
                    &actuals[*arg_index],
                    &sig.params[param_index].ty,
                )
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
        ArenaCallArgKind::Splice { .. } => false,
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
            ArenaCallArgKind::Splice { .. } => return None,
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
pub(super) fn module_arg_matches_param_arena(
    arena: &ArenaProgram,
    arg: &ArenaCallArgKind,
    actual: &Type,
    expected: &Type,
) -> bool {
    if actual.matches_expected(expected) {
        return true;
    }
    if expected != &Type::Path {
        return false;
    }
    let expr_id = call_arg_expr_id_arena(arg);
    let kind = arena.arena.expr(expr_id).kind;
    is_path_like_arena_expr(&kind, actual)
}
