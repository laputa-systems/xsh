#![allow(clippy::single_call_fn)]

use super::expr::is_path_like_arena_expr;
use super::{
    ApiArgCheck, BTreeMap, CallableParamType, Checker, FxHashSet, MethodReceiver, ModuleExportType,
    Name, QualifiedName, Span, Type, UnaryOp, api_spec, call_arg_expr_id_arena,
    call_arg_span_arena, standard_record_type,
};
use crate::syntax::arena::{
    ArenaCallArg, ArenaCallArgKind, ArenaExprKind, ArenaProgram, ArenaRange, ArenaRecordFieldKind,
    ExprId,
};
use crate::syntax::node::Effect;
use xsh_registry::types::BuiltinTypeName;

fn args_parse_form_position(form: &str) -> (bool, bool) {
    let mut has_option = false;
    for token in form.split_whitespace() {
        if token.starts_with('-') {
            has_option = true;
        } else if !has_option {
            return (true, token.starts_with("..."));
        }
    }
    (false, false)
}

fn args_parse_type_from_default(ty: Type) -> Option<(Type, bool)> {
    match ty {
        Type::List(inner) => Some((*inner, true)),
        Type::Str | Type::Int | Type::Bool | Type::Path | Type::Duration => Some((ty, false)),
        _ => None,
    }
}

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

fn args_parse_type_from_name(text: &str) -> Option<(Type, bool)> {
    let text = text.trim();
    if let Some(inner) = text
        .strip_prefix("List[")
        .and_then(|value| value.strip_suffix(']'))
    {
        let (inner, inner_repeated) = args_parse_type_from_name(inner)?;
        if inner_repeated {
            return None;
        }
        return Some((inner, true));
    }
    let ty = match BuiltinTypeName::parse(text)? {
        BuiltinTypeName::Str => Type::Str,
        BuiltinTypeName::Int | BuiltinTypeName::UInt => Type::Int,
        BuiltinTypeName::Bool => Type::Bool,
        BuiltinTypeName::Path => Type::Path,
        BuiltinTypeName::Duration => Type::Duration,
        _ => return None,
    };
    Some((ty, false))
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

fn literal_bool_arena(kind: &ArenaExprKind) -> Option<bool> {
    match kind {
        ArenaExprKind::Bool(value) => Some(*value),
        _ => None,
    }
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
        self.check_callee_effects(caller_effs, callee_effects, callee_name, span);
    }

    pub(super) fn expect_json_compatible(&mut self, ty: &Type, span: Span) {
        if !ty.is_json_compatible() {
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
    ) -> Type {
        let callee_kind = arena.arena.expr(callee).kind;
        let args = arena.arena.call_args(args_range);

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
                } else if let Some(caller_effs) = self.current_effects.clone() {
                    self.check_callee_effects(&caller_effs, &sig.effects, &name.as_str(), span);
                }
                self.check_function_arg_list_arena(arena, source, args, &sig.params, span);
                return sig.return_ty;
            }
            if let Some(sig) = self.pures.get(&name).cloned() {
                self.check_function_arg_list_arena(arena, source, args, &sig.params, span);
                return sig.return_ty;
            }
            if let Some(sig) = self.streams.get(&name).cloned() {
                if self.in_pure {
                    self.error(
                        span,
                        "stream producer is not allowed in pure functions",
                        "check.pure-effect",
                    );
                } else if let Some(caller_effs) = self.current_effects.clone() {
                    self.check_callee_effects(&caller_effs, &sig.effects, &name.as_str(), span);
                }
                self.check_function_arg_list_arena(arena, source, args, &sig.params, span);
                return sig.return_ty;
            }
            return self.check_constructor_call_arena(arena, source, &name.as_str(), args, span);
        }

        if let ArenaExprKind::Field { base, name } = callee_kind {
            let base_kind = arena.arena.expr(base).kind;
            if let ArenaExprKind::Ident(module) = base_kind {
                if self.error_families.contains_key(&module) {
                    return self.check_error_variant_constructor_arena(
                        arena, source, module, name, args, span,
                    );
                }
                let qualified = QualifiedName::new(module, name);
                if let Some(sig) = self.qualified_pures.get(&qualified).cloned() {
                    self.check_function_arg_list_arena(arena, source, args, &sig.params, span);
                    return sig.return_ty;
                }
                if let Some(sig) = self.qualified_procs.get(&qualified).cloned() {
                    if self.in_pure {
                        self.error(
                            span,
                            "effectful proc is not allowed in pure functions",
                            "check.pure-effect",
                        );
                    } else if let Some(caller_effs) = self.current_effects.clone() {
                        self.check_callee_effects(
                            &caller_effs,
                            &sig.effects,
                            &qualified.to_string(),
                            span,
                        );
                    }
                    self.check_function_arg_list_arena(arena, source, args, &sig.params, span);
                    return sig.return_ty;
                }
                if let Some(sig) = self.qualified_streams.get(&qualified).cloned() {
                    if self.in_pure {
                        self.error(
                            span,
                            "stream producer is not allowed in pure functions",
                            "check.pure-effect",
                        );
                    } else if let Some(caller_effs) = self.current_effects.clone() {
                        self.check_callee_effects(
                            &caller_effs,
                            &sig.effects,
                            &qualified.to_string(),
                            span,
                        );
                    }
                    self.check_function_arg_list_arena(arena, source, args, &sig.params, span);
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
                if api_spec().module(&module.as_str()).is_some() {
                    if let Some(caller_effs) = self.current_effects.clone()
                        && let Some(required) =
                            Effect::from_module_call(&module.as_str(), &name.as_str())
                        && !Self::effects_covers(&caller_effs, &required)
                    {
                        self.error(
                            span,
                            &format!(
                                "`{module}.{name}` requires the `{}` effect",
                                required.as_str()
                            ),
                            "check.effect-violation",
                        );
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
            if let Type::Module(exports) = &base_ty
                && let Some(export) = exports.get(&name)
            {
                match export {
                    ModuleExportType::Proc { sig, .. } => {
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
                Type::Optional(inner) => (*inner, true),
                Type::Result(ok, _) => (*ok, false),
                Type::Unknown => return Type::Unknown,
                _ => {
                    self.error(
                        span,
                        "`?.` requires an Optional or Result value",
                        "check.null-safe-field",
                    );
                    return Type::Unknown;
                }
            };
            let return_ty = self.check_method_dispatch_arena(
                arena,
                source,
                inner_ty,
                &name.as_str(),
                args,
                span,
            );
            return if wrap_optional {
                Type::Optional(Box::new(return_ty))
            } else {
                return_ty
            };
        }

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
        for param in params {
            if param.rest {
                let item_ty = match &param.ty {
                    Type::List(item) => item.as_ref().clone(),
                    Type::Any | Type::Unknown => param.ty.clone(),
                    _ => {
                        self.error(span, "rest parameter requires List", "check.rest-type");
                        Type::Unknown
                    }
                };
                for arg in &args[index..] {
                    let actual =
                        self.check_call_arg_arena(arena, source, &arg.kind, Some(&item_ty));
                    self.expect_type(&item_ty, &actual, call_arg_span_arena(arena, &arg.kind));
                }
                return;
            }
            let Some(arg) = args.get(index) else {
                continue;
            };
            if let ArenaCallArgKind::Named { name, .. } = &arg.kind
                && *name != param.name
            {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    "unexpected named parameter",
                    "check.named-arg",
                );
            }
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, Some(&param.ty));
            self.expect_type(&param.ty, &actual, call_arg_span_arena(arena, &arg.kind));
            index += 1;
        }
    }

    pub(super) fn check_constructor_call_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        name: &str,
        args: &[ArenaCallArg],
        span: Span,
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
                let ty = args.first().map_or(Type::Unit, |arg| {
                    self.check_call_arg_arena(arena, source, &arg.kind, None)
                });
                Type::Result(Box::new(ty), Box::new(Type::Error))
            }
            "Err" => {
                let err = args.first().map_or(Type::Error, |arg| {
                    self.check_call_arg_arena(arena, source, &arg.kind, None)
                });
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
                self.check_expr_arg_list_arena(arena, source, args, &[Type::Str], span);
                Type::Result(Box::new(Type::Str), Box::new(Type::Error))
            }
            "Path" => {
                self.check_expr_arg_list_arena(arena, source, args, &[Type::Str], span);
                Type::Path
            }
            "range" if args.len() == 1 || args.len() == 2 => Type::Stream(Box::new(Type::Int)),
            _ => {
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
                ArenaCallArgKind::Splice { .. } => {
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
        let Some(overloads) = module_sig.function_overloads(name) else {
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
        let return_ty = if module == "cli" && name == "parse" {
            self.infer_args_parse_return_arena(arena, source, args)
                .map(|ty| Type::Result(Box::new(ty), Box::new(Type::Error)))
                .unwrap_or_else(|| sig.return_ty.clone())
        } else {
            sig.return_ty.clone()
        };
        if self.options.strict_dynamic && module == "record" && name == "require" {
            self.check_contract_literal_args_arena(arena, args);
        }
        return_ty
    }

    fn infer_args_parse_return_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
    ) -> Option<Type> {
        let schema = args
            .iter()
            .enumerate()
            .find_map(|(index, arg)| match &arg.kind {
                ArenaCallArgKind::Named { name, value, .. } if name == "schema" => Some(*value),
                ArenaCallArgKind::Positional(value) if index == 1 => Some(*value),
                _ => None,
            })?;
        let ArenaExprKind::Record(fields_range) = arena.arena.expr(schema).kind else {
            return None;
        };
        let mut output = BTreeMap::new();
        for field in arena.arena.record_fields(fields_range) {
            let ArenaRecordFieldKind::Named { name, value, .. } = &field.kind else {
                return None;
            };
            let field_ty = self.infer_args_parse_field_type_arena(arena, source, *value)?;
            output.insert(*name, field_ty);
        }
        Some(Type::Record(output))
    }

    fn infer_args_parse_field_type_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        descriptor: ExprId,
    ) -> Option<Type> {
        let descriptor_expr = arena.arena.expr(descriptor);
        match &descriptor_expr.kind {
            ArenaExprKind::Str(type_name_id) => {
                let type_name = arena.arena.string_literal(*type_name_id).clone();
                let (ty, repeated) = args_parse_type_from_name(&type_name)?;
                if repeated {
                    Some(Type::List(Box::new(ty)))
                } else if matches!(ty, Type::Bool) {
                    Some(Type::Bool)
                } else {
                    Some(Type::Optional(Box::new(ty)))
                }
            }
            ArenaExprKind::Record(fields_range) => {
                let mut type_name = None;
                let mut repeated = false;
                let mut required = false;
                let mut positional = false;
                let mut flag = None;
                let mut default_ty = None;
                for field in arena.arena.record_fields(*fields_range) {
                    let ArenaRecordFieldKind::Named { name, value, .. } = &field.kind else {
                        return None;
                    };
                    let value_expr = arena.arena.expr(*value);
                    match name.as_str().as_str() {
                        "kind" | "type" => {
                            let ArenaExprKind::Str(value_id) = value_expr.kind else {
                                return None;
                            };
                            type_name = Some(arena.arena.string_literal(value_id).clone());
                        }
                        "repeated" => repeated = literal_bool_arena(&value_expr.kind)?,
                        "required" => required = literal_bool_arena(&value_expr.kind)?,
                        "positional" => positional = literal_bool_arena(&value_expr.kind)?,
                        "flag" => flag = Some(literal_bool_arena(&value_expr.kind)?),
                        "default" => {
                            default_ty = Some(self.check_expr_arena(arena, source, *value, None));
                        }
                        "form" => {
                            let ArenaExprKind::Str(value_id) = value_expr.kind else {
                                return None;
                            };
                            let form = arena.arena.string_literal(value_id).clone();
                            let (form_positional, form_repeated) = args_parse_form_position(&form);
                            positional = positional || form_positional;
                            repeated = repeated || form_repeated;
                        }
                        _ => {}
                    }
                }
                let (ty, type_repeated) = if let Some(type_name) = &type_name {
                    args_parse_type_from_name(type_name)?
                } else if let Some(default_ty) = default_ty.clone() {
                    args_parse_type_from_default(default_ty)?
                } else {
                    (Type::Str, false)
                };
                repeated = repeated || type_repeated;
                required = required || (positional && !repeated);
                let flag = flag.unwrap_or(matches!(ty, Type::Bool) && !repeated && !positional);
                if repeated {
                    Some(Type::List(Box::new(ty)))
                } else if flag || required || default_ty.is_some() {
                    Some(ty)
                } else {
                    Some(Type::Optional(Box::new(ty)))
                }
            }
            _ => None,
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
                ArenaCallArgKind::Splice { .. } => false,
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
                ArenaCallArgKind::Splice { .. } => {
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
            self.check_optional_api_arg_arena(arena, source, slots[offset + 2], Some(expected));
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
            for item in arena.arena.expr_ids(items) {
                let item_ty = self.check_expr_arena(arena, source, item, None);
                if !process_command_argv_item_type_is_valid(&item_ty) {
                    self.error(
                        arena.arena.expr(item).span,
                        "process.command_argv argv items must be Str or Path",
                        "check.type-mismatch",
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
