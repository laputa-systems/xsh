#![allow(clippy::single_call_fn)]

use super::{
    Checker, Diagnostic, Label, MethodReceiver, Span, Type, api_spec, call_arg_span_arena,
    common_module_overload_expected_arena, module_overload_matches_arena, module_sig_accepts_arg_name_at_arena,
    module_sig_accepts_arity, module_sig_accepts_names_arena,
};
use crate::sema::check::{ApiArgCheck, MethodSig, ModuleFnSig};
use crate::syntax::arena::{ArenaCallArg, ArenaCallArgKind, ArenaProgram};

/// Registered method calls instantiate receiver relationships before checking
/// arguments. Effects and overload selection remain owned by the same registry.
#[allow(dead_code)]
impl Checker {
    pub(super) fn check_method_dispatch_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        base: crate::syntax::arena::ExprId,
        base_ty: Type,
        name: &str,
        args: &[ArenaCallArg],
        span: Span,
        expected: Option<&Type>,
    ) -> Type {
        if (name == "contains" && matches!(base_ty, Type::Str | Type::Bytes | Type::List(_)))
            || (name == "has" && match &base_ty {
                Type::Map(_, _) | Type::ErasedRecord => true,
                Type::Record(fields) => !fields.contains_key(&crate::sema::check::Name::intern("has")),
                Type::Module(exports) => !exports.contains_key(&crate::sema::check::Name::intern("has")),
                _ => false,
            })
        {
            self.membership_migration_spans.insert(span);
            self.error(span, "standard membership method was removed; use `in` or `not in`", "check.removed-membership");
            let key_ty = match &base_ty { Type::Map(key, _) => Some(key.as_ref()), _ => None };
            for index in 0..args.len() {
                self.check_api_arg_arena(arena, source, args, index, if index == 0 { key_ty } else { None });
            }
            return Type::Bool;
        }
        if base_ty == Type::Any {
            // A dynamic method has no checked signature to bind names
            // against; such a call used to check and then fail preparation.
            if args.iter().any(|arg| matches!(arg.kind, ArenaCallArgKind::Named { .. } | ArenaCallArgKind::NamedSpread { .. })) {
                self.error(
                    span,
                    &format!("named arguments to `{name}` need a checked receiver; validate the value with `.require(T)` first"),
                    "check.dynamic-boundary",
                );
            }
            self.check_opaque_callable_effects(&format!("Any.{name}"), span);
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
                expected,
                self.schema_expectation_for_expr(arena, base),
            );
        }
        if base_ty == Type::EnvPathList {
            // The registered Path parameter would accept a Str literal through
            // literal path promotion; env.PATH entries are not a promotion
            // boundary, so require an explicit Path.
            if matches!(name, "append" | "prepend")
                && let Some(arg) = args.first()
                && matches!(
                    arena.arena.expr(super::args::call_arg_expr_id_arena(&arg.kind)).kind,
                    crate::syntax::arena::ArenaExprKind::Str(_)
                )
            {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    &format!("env.PATH.{name} requires Path; write a path literal such as p\"/opt/bin\""),
                    "check.type-mismatch",
                );
            }
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::EnvPathList,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
                expected,
                self.schema_expectation_for_expr(arena, base),
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
                expected,
                self.schema_expectation_for_expr(arena, base),
            );
        }
        if matches!(base_ty, Type::Int | Type::UInt) {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::Int,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
                expected,
                self.schema_expectation_for_expr(arena, base),
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
                expected,
                self.schema_expectation_for_expr(arena, base),
            );
        }
        if matches!(base_ty, Type::Stream(_)) {
            return self.check_registered_method_arena(
                arena, source, MethodReceiver::Stream, name, args, span, &base_ty,
                "check.unknown-method", expected, self.schema_expectation_for_expr(arena, base),
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
                expected,
                self.schema_expectation_for_expr(arena, base),
            );
        }
        if matches!(base_ty, Type::Map(_, _)) {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::Map,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
                expected,
                self.schema_expectation_for_expr(arena, base),
            );
        }
        if matches!(base_ty, Type::ErasedRecord | Type::Record(_) | Type::Module(_)) {
            let projection = api_spec().method_overloads(MethodReceiver::Record, name)
                .filter(|methods| methods.iter().any(|method| method.sig.semantic_rule == crate::modules::signature::SemanticRule::ConstantKeyProjection))
                .and_then(|_| crate::sema::projection::resolve_get_projection(
                    &arena.arena, &self.prepared_constants, base, &base_ty, args,
                ));
            let result = self.check_registered_method_arena(
                arena, source, MethodReceiver::Record, name, args, span,
                &if matches!(base_ty, Type::Module(_)) { Type::ErasedRecord } else { base_ty.clone() }, "check.unknown-method",
                if projection.is_some() { None } else { expected }, self.schema_expectation_for_expr(arena, base),
            );
            if let Type::Result(_, error) = &result && let Some(projection) = projection {
                let refined = Type::Result(Box::new(projection.value_type.clone()), error.clone());
                self.projections.insert(span, projection);
                if let Some(expected) = expected { self.expect_type(expected, &refined, span); }
                return refined;
            }
            return result;
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
                expected,
                self.schema_expectation_for_expr(arena, base),
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
                expected,
                self.schema_expectation_for_expr(arena, base),
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
                expected,
                self.schema_expectation_for_expr(arena, base),
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
                expected,
                self.schema_expectation_for_expr(arena, base),
            );
        }
        if base_ty == Type::NetJob {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::NetJob,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
                expected,
                self.schema_expectation_for_expr(arena, base),
            );
        }
        if base_ty == Type::FsRoot {
            return self.check_registered_method_arena(
                arena,
                source,
                MethodReceiver::FsRoot,
                name,
                args,
                span,
                &base_ty,
                "check.unknown-method",
                expected,
                self.schema_expectation_for_expr(arena, base),
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
                expected,
                self.schema_expectation_for_expr(arena, base),
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
                expected,
                self.schema_expectation_for_expr(arena, base),
            );
        }
        if base_ty == Type::Proc {
            return self.check_proc_call_method_arena(arena, source, name, args, span);
        }
        if base_ty == Type::Pure {
            return self.check_pure_call_method_arena(arena, source, name, args, span);
        }
        // Every other concrete receiver has no methods. Accepting the call as
        // Unknown let it through to preparation (an internal encode error) or
        // to a runtime type error on a null Optional.
        if matches!(base_ty, Type::Unknown | Type::Invalid | Type::Inference(_) | Type::BuiltinParameter(_) | Type::DynamicModule) {
            return Type::Unknown;
        }
        for arg in args {
            self.check_call_arg_arena(arena, source, &arg.kind, None);
        }
        let diagnostic = if let Type::Optional(inner) = &base_ty {
            Diagnostic::error(format!("method `{name}` needs a present value, found {base_ty}"))
                .with_code("check.optional-method")
                .with_label(Label::primary(span, format!("use `?.{name}(...)` or test for null before calling a {inner} method")))
        } else {
            Diagnostic::error(format!("unknown method `{name}` on {base_ty}"))
                .with_code("check.unknown-method")
                .with_label(Label::primary(span, format!("{base_ty} has no methods")))
        };
        self.diagnostics.push(diagnostic);
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
        self.check_opaque_callable_effects("Proc.call", span);
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
        self.check_opaque_callable_effects("Pure.call", span);
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
        expected: Option<&Type>,
        receiver_schema: Option<crate::sema::constants::SchemaExpectation>,
    ) -> Type {
        let Some(overloads) = api_spec().method_overloads(receiver, name) else {
            self.report_unknown_method(receiver, receiver_ty, name, span, unknown_code);
            return Type::Unknown;
        };
        let (method, _) = self.choose_method_sig_arena(arena, source, name, args, overloads, span);
        let mut instance = match crate::sema::builtin_templates::BuiltinInstantiation::new(
            &method.sig, method.receiver_ty.as_ref(), Some(receiver_ty), &mut self.type_constraints, span,
        ) {
            Ok(instance) => instance,
            Err(conflict) => {
                self.error(span, &format!("method `{name}` requires {}; found {}", conflict.expected, conflict.actual), "check.type-mismatch");
                for arg in args { self.check_call_arg_arena(arena, source, &arg.kind, None); }
                return Type::Invalid;
            }
        };
        // A plain expectation for a Result-returning method only guides its
        // success payload. When the payload disagrees, the consumer still
        // receives a `Result` it did not expect and reports that with both
        // full types, so reporting the payload too would say it twice.
        if let Some(expected) = expected
            && let Err(conflict) = instance.constrain_result(expected, &mut self.type_constraints, span)
            && (expected.is_result() || !method.sig.return_ty.is_result())
        {
            self.expect_type(&conflict.expected, &conflict.actual, span);
        }
        if self.in_pure && !method.sig.pure {
            self.error(
                span,
                "effectful method is not allowed in pure functions",
                "check.pure-effect",
            );
        }
        if let Some(required) = method.sig.effect.clone() {
            self.require_effect(required, span, &format!("method `{name}`"));
        }
        let schemas = crate::sema::builtin_templates::parameter_schema_contexts(&method.sig, method.receiver_ty.as_ref(), receiver_schema.as_ref());
        let mut concrete = method.clone();
        concrete.sig = instance.signature.clone();
        self.check_method_args_arena(arena, source, args, &concrete, false, span, &schemas);
        instance.resolve(&self.type_constraints);
        if let Some(key) = instance.invalid_map_key() {
            self.error(span, &format!("unsupported Map key type {key}"), "check.map-key");
            return Type::Invalid;
        }
        if receiver != MethodReceiver::PathConstructor {
            self.publish_api_call(arena, args, span, Some(receiver), &method.sig, &instance.signature);
        }
        instance.signature.return_ty
    }

    /// Publishes the selected overload's binding for lowering. Spread and
    /// splice entries have no static plan and lower through their own paths.
    pub(super) fn publish_api_call(
        &mut self, arena: &ArenaProgram, args: &[ArenaCallArg], span: Span,
        receiver: Option<MethodReceiver>, sig: &'static ModuleFnSig, concrete: &ModuleFnSig,
    ) {
        let Ok(expanded) = crate::sema::arguments::expand_named_arguments(arena, args, |_| None) else { return; };
        let params = crate::sema::builtin_templates::callable_parameters(concrete);
        let Ok(binding) = crate::sema::arguments::bind_static_arguments(&params, &expanded) else { return; };
        let params = params.into_iter().map(|param| param.ty).collect();
        self.api_calls.insert(span, super::CheckedApiCall { receiver, sig, params, argument_slots: binding.argument_slots });
    }

    fn report_unknown_method(
        &mut self,
        receiver: MethodReceiver,
        receiver_ty: &Type,
        name: &str,
        span: Span,
        code: &str,
    ) {
        let mut diagnostic = Diagnostic::error(format!("unknown method `{name}` on {receiver_ty}"))
            .with_code(code)
            .with_label(Label::primary(
                span,
                format!("`{name}` is not defined for {receiver_ty}"),
            ));
        let mut candidates = api_spec()
            .method_names(receiver)
            .filter(|candidate| method_name_is_nearby(name, candidate))
            .collect::<Vec<_>>();
        if receiver == MethodReceiver::Str && matches!(name, "len" | "length") {
            candidates = vec!["byte_len", "count_chars"];
        }
        if receiver == MethodReceiver::List && matches!(name, "append" | "add") {
            diagnostic = diagnostic.with_note("lists are values: append to a `var` with `items += [value]`, or build a new list with `.push(value)`");
            candidates.clear();
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
        let eligible = overloads.iter().filter(|method| {
            let params = crate::sema::builtin_templates::callable_parameters(&method.sig);
            let Ok(expanded) = crate::sema::arguments::expand_named_arguments(arena, args, |_| None) else { return false; };
            crate::sema::arguments::bind_static_arguments(&params, &expanded).is_ok()
        }).collect::<Vec<_>>();
        if let [method] = eligible.as_slice() { return (method, false); }

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

        let mut dynamic_boundary = false;
        for (index, (arg, actual)) in args.iter().zip(&actuals).enumerate() {
            if let Some(expected) = common_method_overload_expected_arena(args, overloads, index)
                && actual.any_flows_to_concrete(&expected)
            {
                self.expect_type(&expected, actual, call_arg_span_arena(arena, &arg.kind));
                dynamic_boundary = true;
            }
        }
        if dynamic_boundary {
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
        schemas: &[Option<crate::sema::constants::SchemaExpectation>],
    ) {
        match method.sig.arg_check {
            ApiArgCheck::Standard | ApiArgCheck::JsonCompatible => {
                if !args_checked {
                    self.check_module_sig_args_with_schema_arena(arena, source, args, &method.sig, span, schemas);
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
                    self.check_module_sig_args_with_schema_arena(arena, source, args, &method.sig, span, schemas);
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

/// The candidate closest to a misspelled name, if any is near enough to be
/// a plausible typo; ties keep the alphabetically first spelling. Names
/// shorter than three characters are near almost everything, so they get no
/// suggestion.
pub(super) fn nearest_name<S: AsRef<str>>(unknown: &str, candidates: impl Iterator<Item = S>) -> Option<String> {
    if unknown.chars().count() < 3 {
        return None;
    }
    candidates
        .map(|candidate| candidate.as_ref().to_string())
        .filter(|candidate| candidate != unknown && method_name_is_nearby(unknown, candidate))
        .min_by_key(|candidate| (edit_distance(unknown, candidate), candidate.clone()))
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
