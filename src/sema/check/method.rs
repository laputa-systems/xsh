#![allow(clippy::single_call_fn)]

use super::{
    Checker, Diagnostic, Label, MethodReceiver, Span, Type, api_spec, call_arg_span_arena,
    collection_item_ty, common_module_overload_expected_arena, map_item_ty,
    merge_collection_item_ty, module_overload_matches_arena, module_sig_accepts_arg_name_at_arena,
    module_sig_accepts_arity, module_sig_accepts_names_arena,
};
use crate::sema::check::{ApiArgCheck, MethodSig};
use crate::syntax::arena::{ArenaCallArg, ArenaProgram};

/// Arena-native mirror of every function above, operating on the arena's
/// call-argument representation instead of the old recursive AST's.
/// The signature metadata's required effect and the collection-item helpers
/// (`collection_item_ty`/`map_item_ty`/`merge_collection_item_ty`) are pure
/// `Type`-level and reused unchanged.
#[allow(dead_code)]
impl Checker {
    pub(super) fn check_method_dispatch_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        base_ty: Type,
        name: &str,
        args: &[ArenaCallArg],
        span: Span,
    ) -> Type {
        if base_ty == Type::Any {
            for arg in args {
                self.check_call_arg_arena(arena, source, &arg.kind, None);
            }
            return Type::Any;
        }
        if let Type::Result(_, _) = &base_ty {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::Result,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
            );
        }
        if base_ty == Type::EnvPathList {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::EnvPathList,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
            );
        }
        if base_ty == Type::Path {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::Path,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
            );
        }
        if base_ty == Type::Int {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::Int,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
            );
        }
        if base_ty == Type::Float {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::Float,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
            );
        }
        if matches!(base_ty, Type::List(_)) {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::List,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
            );
        }
        if matches!(base_ty, Type::Map(_)) {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::Map,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
            );
        }
        if matches!(base_ty, Type::Record(_)) {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::Record,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
            );
        }
        if matches!(base_ty, Type::Module(_)) {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::Record,
                name,
                args,
                span,
                &Type::Record(Default::default()),
                "check.unknown-method",
            );
        }
        if base_ty == Type::Str {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::Str,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
            );
        }
        if base_ty == Type::Bytes {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::Bytes,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
            );
        }
        if base_ty == Type::Status {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::Status,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
            );
        }
        if base_ty == Type::ProcessHandle {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::ProcessHandle,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
            );
        }
        if base_ty == Type::Digest {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::Digest,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
            );
        }
        if base_ty == Type::Regex {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::Regex,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
            );
        }
        if base_ty == Type::Proc {
            return self.check_proc_call_method_arena(arena, source, name, args, span);
        }
        if base_ty == Type::Pure {
            return self.check_pure_call_method_arena(arena, source, name, args, span);
        }
        Type::Unknown
    }

    fn check_proc_call_method_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        name: &str,
        args: &[ArenaCallArg],
        span: Span,
    ) -> Type {
        if name != "call" {
            self.error(span, "unknown method", "check.unknown-method");
            return Type::Unknown;
        }
        if self.in_pure {
            self.error(
                span,
                "effectful method is not allowed in pure functions",
                "check.pure-effect",
            );
        }
        for arg in args {
            self.check_call_arg_arena(arena, source, &arg.kind, None);
        }
        Type::Result(Box::new(Type::Any), Box::new(Type::Error))
    }

    fn check_pure_call_method_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        name: &str,
        args: &[ArenaCallArg],
        span: Span,
    ) -> Type {
        if name != "call" {
            self.error(span, "unknown method", "check.unknown-method");
            return Type::Unknown;
        }
        for arg in args {
            self.check_call_arg_arena(arena, source, &arg.kind, None);
        }
        Type::Any
    }

    pub(super) fn check_registered_method_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        receiver: MethodReceiver,
        name: &str,
        args: &[ArenaCallArg],
        span: Span,
        receiver_ty: &Type,
        unknown_code: &str,
    ) -> Type {
        let Some(overloads) = api_spec().method_overloads(receiver, name) else {
            self.report_unknown_method(receiver, receiver_ty, name, span, unknown_code);
            return Type::Unknown;
        };
        let (method, args_checked) =
            self.choose_method_sig_arena(arena, source, name, args, overloads, span);
        if self.in_pure && !method.sig.pure {
            self.error(
                span,
                "effectful method is not allowed in pure functions",
                "check.pure-effect",
            );
        }
        if let Some(caller_effs) = self.current_effects.clone()
            && let Some(required) = method.sig.effect.clone()
            && !Self::effects_covers(&caller_effs, &required)
        {
            self.error(
                span,
                &format!(
                    "method `{name}` requires the `{}` effect",
                    required.as_str()
                ),
                "check.effect-violation",
            );
        }
        if receiver == MethodReceiver::List {
            return self.check_list_method_call_arena(
                arena,
                source,
                receiver_ty,
                name,
                args,
                method,
                span,
            );
        }
        if receiver == MethodReceiver::Map {
            return self.check_map_method_call_arena(
                arena,
                source,
                receiver_ty,
                name,
                args,
                method,
                span,
            );
        }
        self.check_method_args_arena(arena, source, args, method, args_checked, span);
        method.concrete_return_ty(receiver_ty)
    }

    fn report_unknown_method(
        &mut self,
        receiver: MethodReceiver,
        receiver_ty: &Type,
        name: &str,
        span: Span,
        code: &str,
    ) {
        let mut diagnostic = Diagnostic::error(format!(
            "unknown method `{name}` on {receiver_ty}"
        ))
        .with_code(code)
        .with_label(Label::primary(span, format!("`{name}` is not defined for {receiver_ty}")));
        let mut candidates = api_spec()
            .method_names(receiver)
            .filter(|candidate| method_name_is_nearby(name, candidate))
            .collect::<Vec<_>>();
        if receiver == MethodReceiver::Str && matches!(name, "len" | "length") {
            candidates = vec!["byte_len", "count_bytes", "count_chars"];
        }
        if !candidates.is_empty() {
            diagnostic = diagnostic.with_note(format!(
                "available methods include: {}",
                candidates
                    .into_iter()
                    .map(|candidate| format!("`{candidate}()`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn check_list_method_call_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        receiver_ty: &Type,
        name: &str,
        args: &[ArenaCallArg],
        method: &MethodSig,
        span: Span,
    ) -> Type {
        let item_ty = collection_item_ty(receiver_ty);
        match name {
            "push" => {
                self.check_standard_arg_shape_arena(arena, args, &["item"], span);
                let value_ty = self.check_api_arg_arena(arena, source, args, 0, None);
                let merged = merge_collection_item_ty(item_ty.clone(), value_ty.clone());
                if merged == item_ty && !value_ty.matches_expected(&item_ty) {
                    self.expect_type(
                        &item_ty,
                        &value_ty,
                        call_arg_span_arena(arena, &args[0].kind),
                    );
                }
                Type::List(Box::new(merged))
            }
            "extend" => {
                self.check_standard_arg_shape_arena(arena, args, &["other"], span);
                let actual = self.check_api_arg_arena(arena, source, args, 0, None);
                let actual_item_ty = collection_item_ty(&actual);
                let merged = merge_collection_item_ty(item_ty.clone(), actual_item_ty.clone());
                if merged == item_ty && !actual_item_ty.matches_expected(&item_ty) {
                    let expected = Type::List(Box::new(item_ty));
                    self.expect_type(
                        &expected,
                        &actual,
                        call_arg_span_arena(arena, &args[0].kind),
                    );
                }
                Type::List(Box::new(merged))
            }
            "contains" => {
                self.check_standard_arg_shape_arena(arena, args, &["item"], span);
                self.check_api_arg_arena(arena, source, args, 0, Some(&item_ty));
                Type::Bool
            }
            "get" => {
                let has_fallback = args.len() >= 2;
                if has_fallback {
                    self.check_standard_arg_shape_arena(arena, args, &["index", "fallback"], span);
                } else {
                    self.check_standard_arg_shape_arena(arena, args, &["index"], span);
                }
                self.check_api_arg_arena(arena, source, args, 0, Some(&Type::Int));
                if has_fallback {
                    let fallback_ty =
                        self.check_api_arg_arena(arena, source, args, 1, Some(&item_ty));
                    merge_collection_item_ty(item_ty, fallback_ty)
                } else {
                    Type::Result(Box::new(item_ty), Box::new(Type::Error))
                }
            }
            _ => {
                self.check_method_args_arena(arena, source, args, method, false, span);
                method.concrete_return_ty(receiver_ty)
            }
        }
    }

    fn check_map_method_call_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        receiver_ty: &Type,
        name: &str,
        args: &[ArenaCallArg],
        method: &MethodSig,
        span: Span,
    ) -> Type {
        let item_ty = map_item_ty(receiver_ty);
        match name {
            "set" => {
                self.check_standard_arg_shape_arena(arena, args, &["key", "value"], span);
                self.check_api_arg_arena(arena, source, args, 0, Some(&Type::Str));
                let value_ty = self.check_api_arg_arena(arena, source, args, 1, Some(&item_ty));
                Type::Map(Box::new(merge_collection_item_ty(item_ty, value_ty)))
            }
            "push" => {
                self.check_standard_arg_shape_arena(arena, args, &["key", "value"], span);
                self.check_api_arg_arena(arena, source, args, 0, Some(&Type::Str));
                let list_item_ty = match &item_ty {
                    Type::List(inner) => inner.as_ref().clone(),
                    Type::Any | Type::Unknown => Type::Any,
                    _ => {
                        self.error(
                            span,
                            "map push requires map values to be lists",
                            "check.type-mismatch",
                        );
                        Type::Unknown
                    }
                };
                let value_ty =
                    self.check_api_arg_arena(arena, source, args, 1, Some(&list_item_ty));
                let merged = merge_collection_item_ty(list_item_ty, value_ty);
                Type::Map(Box::new(Type::List(Box::new(merged))))
            }
            "get" => {
                let has_fallback = args.len() >= 2;
                if has_fallback {
                    self.check_standard_arg_shape_arena(arena, args, &["key", "fallback"], span);
                } else {
                    self.check_standard_arg_shape_arena(arena, args, &["key"], span);
                }
                self.check_api_arg_arena(arena, source, args, 0, Some(&Type::Str));
                if has_fallback {
                    let fallback_ty =
                        self.check_api_arg_arena(arena, source, args, 1, Some(&item_ty));
                    merge_collection_item_ty(item_ty, fallback_ty)
                } else {
                    Type::Result(Box::new(item_ty), Box::new(Type::Error))
                }
            }
            "remove" => {
                self.check_standard_arg_shape_arena(arena, args, &["key"], span);
                self.check_api_arg_arena(arena, source, args, 0, Some(&Type::Str));
                receiver_ty.clone()
            }
            "values" => {
                self.check_standard_arg_shape_arena(arena, args, &[], span);
                Type::List(Box::new(item_ty))
            }
            _ => {
                self.check_method_args_arena(arena, source, args, method, false, span);
                method.concrete_return_ty(receiver_ty)
            }
        }
    }

    fn choose_method_sig_arena<'a>(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        _name: &str,
        args: &[ArenaCallArg],
        overloads: &'a [MethodSig],
        span: Span,
    ) -> (&'a MethodSig, bool) {
        if overloads.len() == 1 {
            return (&overloads[0], false);
        }

        let actuals = args
            .iter()
            .enumerate()
            .map(|(index, arg)| {
                let expected = common_method_overload_expected_arena(args, overloads, index);
                self.check_call_arg_arena(arena, source, &arg.kind, expected.as_ref())
            })
            .collect::<Vec<_>>();
        let matches = overloads
            .iter()
            .filter(|method| module_overload_matches_arena(arena, args, &actuals, &method.sig))
            .collect::<Vec<_>>();
        if let Some(method) = matches.first() {
            if matches.len() > 1 && actuals.iter().all(|ty| !matches!(ty, Type::Unknown)) {
                self.error(
                    span,
                    "ambiguous standard API overload",
                    "check.ambiguous-overload",
                );
            }
            return (method, true);
        }

        let arity_matches = overloads
            .iter()
            .filter(|method| module_sig_accepts_arity(args.len(), &method.sig))
            .collect::<Vec<_>>();
        if arity_matches.is_empty() {
            self.error(span, "incorrect standard API arity", "check.arity");
            return (&overloads[0], true);
        }
        if arity_matches
            .iter()
            .all(|method| !module_sig_accepts_names_arena(args, &method.sig))
        {
            if let Some((_, arg)) = args.iter().enumerate().find(|(index, arg)| {
                arity_matches.iter().all(|method| {
                    !module_sig_accepts_arg_name_at_arena(&arg.kind, *index, &method.sig)
                })
            }) {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    "unexpected named parameter",
                    "check.named-arg",
                );
            } else {
                self.error(span, "unexpected named parameter", "check.named-arg");
            }
            return (arity_matches[0], true);
        }

        self.error(
            span,
            "no standard API overload matches argument types",
            "check.type-mismatch",
        );
        (arity_matches[0], true)
    }

    fn check_method_args_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        method: &MethodSig,
        args_checked: bool,
        span: Span,
    ) {
        match method.sig.arg_check {
            ApiArgCheck::Standard | ApiArgCheck::JsonCompatible => {
                if !args_checked {
                    self.check_module_sig_args_arena(arena, source, args, &method.sig, span);
                }
            }
            ApiArgCheck::PathLikeSingle => {
                if args.len() != 1 {
                    self.error(span, "incorrect function arity", "check.arity");
                }
                if let Some(arg) = args.first() {
                    self.check_path_like_arg_arena(arena, source, Some(&arg.kind), span);
                }
            }
            ApiArgCheck::ResultContext => {
                self.check_result_context_args_arena(arena, source, args, span);
            }
            ApiArgCheck::HashVerifyFile => {
                if !args_checked {
                    self.check_module_sig_args_arena(arena, source, args, &method.sig, span);
                }
            }
        }
    }

    fn check_result_context_args_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        args: &[ArenaCallArg],
        span: Span,
    ) {
        if args.is_empty() {
            self.error(span, "context requires a kind", "check.arity");
        }
        if let Some(arg) = args.first() {
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, Some(&Type::Str));
            self.expect_type(&Type::Str, &actual, call_arg_span_arena(arena, &arg.kind));
        }
        if let Some(arg) = args.get(1) {
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, Some(&Type::Str));
            self.expect_type(&Type::Str, &actual, call_arg_span_arena(arena, &arg.kind));
        }
        for arg in args.iter().skip(2) {
            let actual = self.check_call_arg_arena(arena, source, &arg.kind, None);
            if !actual.can_display() && !matches!(actual, Type::Unknown) {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    "context values must be displayable",
                    "check.display-conversion",
                );
            }
        }
    }
}

fn method_name_is_nearby(unknown: &str, candidate: &str) -> bool {
    let distance = edit_distance(unknown, candidate);
    distance <= unknown.chars().count().max(candidate.chars().count()) / 3 + 1
}

fn edit_distance(left: &str, right: &str) -> usize {
    let right = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    for (left_index, left_char) in left.chars().enumerate() {
        let mut current = vec![left_index + 1];
        for (right_index, right_char) in right.iter().enumerate() {
            let cost = usize::from(left_char != *right_char);
            current.push(
                (current[right_index] + 1)
                    .min(previous[right_index + 1] + 1)
                    .min(previous[right_index] + cost),
            );
        }
        previous = current;
    }
    previous[right.len()]
}

#[allow(dead_code)]
fn common_method_overload_expected_arena(
    args: &[ArenaCallArg],
    overloads: &[MethodSig],
    index: usize,
) -> Option<Type> {
    args.get(index)?;
    let mut expected = None;
    for method in overloads {
        let Some(candidate) =
            common_module_overload_expected_arena(args, std::slice::from_ref(&method.sig), index)
        else {
            continue;
        };
        match &expected {
            Some(current) if current != &candidate => return None,
            Some(_) => {}
            None => expected = Some(candidate),
        }
    }
    expected
}
