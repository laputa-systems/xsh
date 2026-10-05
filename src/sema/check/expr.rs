#![allow(clippy::single_call_fn)]

use super::{
    BTreeMap, BinaryOp, Checker, Diagnostic, Effect, Label, Name, RunKind, Span, Type, UnaryOp,
    api_spec, block_has_exit_point_arena, collection_item_ty,
};
use crate::diagnostic::{DiagnosticCode, FixHint};
use crate::syntax::arena::{
    ArenaCompQualifier, ArenaExprKind, ArenaExprOrRun, ArenaFmtPart, ArenaProgram, ArenaRange,
    ArenaRecordFieldKind, ArenaSpawnForm, ArenaSpawnTarget, ArenaWaitForm, BlockId, ExprId,
    PatternId, RunFormId,
};
use crate::syntax::literal;

pub(super) fn expr_ty_auto_propagates(ty: &Type) -> bool {
    ty.is_result_unit()
}

/// Whether a checked argument can stand where a `Path` is required: a
/// `Path`, or a dynamic or recovery type whose own diagnostics apply. A string
/// literal qualifies only by having taken the `Path` type from its expected
/// type; a `Str` never does.
pub(super) fn is_path_like_type(ty: &Type) -> bool {
    matches!(ty, Type::Path | Type::Any | Type::Unknown)
}

/// Arena-native span helper for expression-or-run values.
pub(super) fn expr_or_run_span_arena(arena: &ArenaProgram, value: ArenaExprOrRun) -> Span {
    match value {
        ArenaExprOrRun::Expr(id) => arena.arena.expr(id).span,
        ArenaExprOrRun::Run(run_id) => arena.arena.span(arena.arena.run_form(run_id).span),
    }
}

fn merge_list_literal_item_ty(current: &Type, next: &Type) -> Option<Type> {
    if next.matches_expected(current) {
        return Some(current.clone());
    }
    if current.matches_expected(next) {
        return Some(next.clone());
    }
    if matches!(
        (current, next),
        (Type::Str, Type::Path) | (Type::Path, Type::Str)
    ) {
        return Some(Type::Any);
    }
    None
}

#[allow(dead_code)]
fn capture_success_underconstrained(ty: &Type) -> bool {
    match ty {
        Type::Unknown => true,
        Type::Result(ok, _) | Type::List(ok) | Type::Optional(ok) => {
            capture_success_underconstrained(ok)
        }
        Type::Map(key, value) => {
            capture_success_underconstrained(key) || capture_success_underconstrained(value)
        }
        Type::Record(fields) => fields.values().any(capture_success_underconstrained),
        _ => false,
    }
}

impl Checker {
    /// Retains independently declared schema context for expected slots. Structural
    /// record values alone never identify a schema application or its unused arguments.
    pub(super) fn schema_expectation_for_expr(
        &self,
        arena: &ArenaProgram,
        expression: ExprId,
    ) -> Option<super::super::constants::SchemaExpectation> {
        use super::super::constants::{SchemaComponent, SchemaExpectation};
        match arena.arena.expr(expression).kind {
            ArenaExprKind::Ident(name) => self.lookup(name)?.schema_expectation.clone(),
            ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name } => {
                self.schema_expectation_for_expr(arena, base)?
                    .value_context()
                    .children
                    .get(&SchemaComponent::Field(name))
                    .cloned()
            }
            ArenaExprKind::Index { base, .. } => {
                let context = self.schema_expectation_for_expr(arena, base)?;
                let ty = self.expr_types.get(&arena.arena.expr(base).span)?;
                let component = match ty {
                    Type::List(_) => SchemaComponent::Item,
                    Type::Map(_, _) => SchemaComponent::Value,
                    _ => return None,
                };
                context.value_context().children.get(&component).cloned()
            }
            ArenaExprKind::Try(inner) => self
                .schema_expectation_for_expr(arena, inner)?
                .children
                .get(&SchemaComponent::Success)
                .cloned(),
            ArenaExprKind::Require { schema, .. } => {
                let context = match schema {
                    Some(schema) => self
                        .record_constructors
                        .annotation_expectation(&arena.arena, schema, self.current_namespace)
                        .ok()?,
                    None => self
                        .requirement_targets
                        .get(&arena.arena.expr(expression).span)?
                        .context
                        .clone(),
                };
                let mut result = SchemaExpectation::default();
                result.children.insert(SchemaComponent::Success, context);
                Some(result)
            }
            ArenaExprKind::Call { callee, .. } => match arena.arena.expr(callee).kind {
                ArenaExprKind::Ident(name) => self
                    .procs
                    .get(&name)
                    .or_else(|| self.pures.get(&name))
                    .or_else(|| self.streams.get(&name))?
                    .return_schema
                    .clone(),
                ArenaExprKind::Field { base, name } => {
                    let ArenaExprKind::Ident(namespace) = arena.arena.expr(base).kind else {
                        return None;
                    };
                    let qualified = crate::symbol::QualifiedName::new(namespace, name);
                    self.qualified_procs
                        .get(&qualified)
                        .or_else(|| self.qualified_pures.get(&qualified))
                        .or_else(|| self.qualified_streams.get(&qualified))?
                        .return_schema
                        .clone()
                }
                _ => None,
            },
            _ => None,
        }
    }

    pub(super) fn lookup_expr_ident(&mut self, name: Name, span: Span) -> Type {
        if name == "_" {
            self.error(
                span,
                "`_` is only a whole argument placeholder in an immediate value pipeline call",
                DiagnosticCode::CheckPipelineHole,
            );
            return Type::Invalid;
        }
        if let Some(binding) = self.lookup(name) {
            let alias = binding.callable_alias.clone();
            let ty = self
                .type_constraints
                .resolve(&binding.ty)
                .unwrap_or(Type::Invalid);
            if let Some(alias) = alias {
                self.record_callable_alias(span, &alias);
            }
            return ty;
        }
        if let Some(info) = self.tag_variants.get(&name).cloned()
            && info.field_count == 0
        {
            return Type::Tag(info.type_name);
        }
        if self.procs.contains_key(&name) {
            return Type::Proc;
        }
        if self.pures.contains_key(&name) {
            return Type::Pure;
        }
        if api_spec().module(&name.as_str()).is_some() {
            // A namespace has no runtime value; typing it as an erased record
            // let `let host = system` check and then fail preparation.
            self.diagnostics.push(
                Diagnostic::error(format!(
                    "standard module `{name}` is a namespace, not a value"
                ))
                .with_code(DiagnosticCode::CheckModuleMember)
                .with_label(Label::primary(span, "call one of its functions instead")),
            );
            return Type::Unknown;
        }
        if name == "ARGV" && !self.streams.contains_key(&name) {
            let shadowed_args = self
                .scopes
                .iter()
                .skip(1)
                .any(|scope| scope.contains_key(&Name::intern("args")));
            self.removed_compatibility_name(span, "ARGV", "args", !shadowed_args);
            return Type::List(Box::new(Type::Str));
        }
        self.report_unresolved_name(name, span);
        Type::Unknown
    }

    /// Spellings of `null`, `true`, and `false` from other languages get a
    /// fix, an all-caps name is usually an environment variable, and any
    /// other name gets the nearest visible binding or callable.
    fn report_unresolved_name(&mut self, name: Name, span: Span) {
        let text = name.as_str();
        let text: &str = text.as_ref();
        let mut diagnostic = Diagnostic::error(format!("unresolved name `{text}`"))
            .with_code(DiagnosticCode::CheckUnresolvedName)
            .with_label(Label::primary(span, "unresolved name"));
        let literal = match text {
            "None" | "Null" | "NULL" | "nil" | "undefined" => Some("null"),
            "True" | "TRUE" => Some("true"),
            "False" | "FALSE" => Some("false"),
            _ => None,
        };
        if let Some(literal) = literal {
            diagnostic = diagnostic.with_fix_hint(super::FixHint::replacement(
                span,
                format!("XSH spells it `{literal}`"),
                literal,
            ));
        } else if text.len() > 1
            && text
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            diagnostic = diagnostic.with_note(format!(
                "environment variables are not names in XSH; read this one with `env.Str.{text}?`"
            ));
        } else if let Some(nearby) = self.nearby_visible_name(text) {
            diagnostic = diagnostic.with_note(format!("did you mean `{nearby}`?"));
        }
        self.diagnostics.push(diagnostic);
    }

    pub(super) fn nearby_visible_name(&self, unknown: &str) -> Option<String> {
        let scoped = self.scopes.iter().flat_map(|scope| scope.keys().copied());
        let callables = self.procs.keys().chain(self.pures.keys()).copied();
        super::method::nearest_name(
            unknown,
            scoped
                .chain(callables)
                .map(|name| name.as_str().to_string()),
        )
    }

    /// A typo of a known record field names the nearest field.
    pub(super) fn report_unknown_field<'n>(
        &mut self,
        span: Span,
        name: Name,
        fields: impl Iterator<Item = &'n Name>,
    ) {
        let unknown = name.as_str();
        let unknown: &str = unknown.as_ref();
        let mut diagnostic =
            Diagnostic::error(format!("unknown field `{unknown}` on known record type"))
                .with_code(DiagnosticCode::CheckUnknownField)
                .with_label(Label::primary(span, "unknown field on known record type"));
        if let Some(nearby) =
            super::method::nearest_name(unknown, fields.map(|field| field.as_str().to_string()))
        {
            diagnostic = diagnostic.with_note(format!("did you mean `{nearby}`?"));
        }
        self.diagnostics.push(diagnostic);
    }

    fn lookup_record_shorthand(&mut self, name: Name, span: Span) -> Type {
        let ty = self.lookup_expr_ident(name, span);
        if name == "ARGV"
            && let Some(diagnostic) = self.diagnostics.last_mut()
            && diagnostic.code == Some(DiagnosticCode::CheckCompatibilityVocabulary)
        {
            for hint in &mut diagnostic.fix_hints {
                if hint.span == Some(span) && hint.replacement.as_deref() == Some("args") {
                    hint.replacement = Some("ARGV: args".to_string());
                }
            }
        }
        ty
    }

    /// `e"NAME"` reads like `env.Str.NAME`: `Result[Str]`, failing when the
    /// variable is unset or not UTF-8.
    pub(super) fn check_env_string(&mut self, span: Span) -> Type {
        self.require_effect(Effect::Env, span, "environment lookup");
        if self.in_pure {
            self.error(
                span,
                "environment lookup is not allowed in pure functions",
                DiagnosticCode::CheckPureEffect,
            );
        }
        Type::Result(Box::new(Type::Str), Box::new(Type::Error))
    }

    pub(super) fn check_process_effect(&mut self, span: Span, form: &str) {
        self.record_required_effect(Effect::Process);
        if self.in_pure {
            self.error(
                span,
                &format!("{form} forms are not allowed in pure functions"),
                DiagnosticCode::CheckPureRun,
            );
        } else if let Some(effs) = &self.current_effects
            && !Self::effects_covers(effs, &Effect::Process)
        {
            self.error(
                span,
                &format!("{form} requires the `process` effect"),
                DiagnosticCode::CheckEffectViolation,
            );
        }
        self.check_effect_not_excluded(&Effect::Process, span, form);
    }
}

/// Arena-native port of [`Checker::check_expr`] and its callees.
///
/// This is the live arena checker path used by `check_arena_with_options`.
#[allow(dead_code)]
impl Checker {
    pub(super) fn check_expr_or_run_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ArenaExprOrRun,
        expected: Option<&Type>,
    ) -> Type {
        match value {
            ArenaExprOrRun::Expr(id) => self.check_expr_arena(arena, source, id, expected),
            ArenaExprOrRun::Run(run_id) => self.check_run_expr_arena(arena, source, run_id),
        }
    }

    pub(super) fn check_expr_with_schema_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ArenaExprOrRun,
        expected: Option<&Type>,
        schema: Option<crate::sema::constants::SchemaExpectation>,
    ) -> Type {
        let previous = std::mem::replace(&mut self.expected_schema, schema);
        let actual = self.check_expr_or_run_arena(arena, source, value, expected);
        self.expected_schema = previous;
        actual
    }

    pub(super) fn check_expr_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        id: ExprId,
        expected: Option<&Type>,
    ) -> Type {
        let previous = self.expected_schema.clone();
        if expected.is_none() {
            self.expected_schema = None;
        }
        let resolved = expected.and_then(|ty| self.type_constraints.resolve(ty).ok());
        let control = std::mem::take(&mut self.control_condition);
        let outer_control = std::mem::replace(&mut self.in_control_position, control);
        let actual = self.check_expr_arena_inner(arena, source, id, resolved.as_ref().or(expected));
        let actual = self.name_typed_callable(arena, id, actual, resolved.as_ref().or(expected));
        self.in_control_position = outer_control;
        let actual = self.propagate_condition_arena(arena, id, control, actual);
        self.expected_schema = previous;
        if actual == Type::Any {
            self.record_dynamic_require_receiver(arena, source, id, expected);
        }
        actual
    }

    /// Records how a `.require(T)?` fix on this `Any` expression is written:
    /// whether it must be grouped, and the target a bare `.require()` would
    /// infer in its place. The fix inserts text around the span and never
    /// copies it, because linked modules are checked with the entry's source
    /// text. A `$name` command shorthand takes no suffix.
    fn record_dynamic_require_receiver(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        id: ExprId,
        expected: Option<&Type>,
    ) {
        let expr = arena.arena.expr(id);
        if let ArenaExprKind::Ident(name) = expr.kind
            && expr.span.end() - expr.span.start() != name.as_str().len()
        {
            return;
        }
        // The `.require(` follow never consults the source text.
        let context = crate::syntax::grouping::Context {
            slot: crate::syntax::grouping::Slot::Postfix { dotted: false },
            ..crate::syntax::grouping::Context::open(crate::syntax::grouping::Follow::adjacent(
                crate::syntax::grouping::FollowToken::Require,
            ))
        };
        let grouped = crate::syntax::grouping::needs_parens(&arena.arena, source, id, context);
        let schema = expected.and(self.expected_schema.as_ref());
        let inferred = super::expected::infer_requirement_target(
            &arena.arena,
            expected,
            schema,
            &self.type_constraints,
        )
        .map(|target| target.ty);
        self.dynamic_require_receivers.insert(
            expr.span,
            super::DynamicRequireReceiver { grouped, inferred },
        );
    }

    /// A `Result[Bool]` in a control position of a condition cannot be the
    /// condition's value, so its failure propagates and the `Bool` remains.
    /// The decision is published for lowering; outside a control position the
    /// type is unchanged and a `Result` stays data.
    fn propagate_condition_arena(
        &mut self,
        arena: &ArenaProgram,
        id: ExprId,
        control: bool,
        actual: Type,
    ) -> Type {
        let expr = arena.arena.expr(id);
        let result_bool =
            |ty: &Type| matches!(ty, Type::Result(ok, _) if **ok == Type::Bool);
        if control && result_bool(&actual) {
            self.propagating_conditions.insert(expr.span);
            self.record_condition_error(&actual, expr.span);
            return Type::Bool;
        }
        self.propagating_conditions.remove(&expr.span);
        if control
            && let ArenaExprKind::Try(operand) = expr.kind
            && self
                .expr_types
                .get(&arena.arena.expr(operand).span)
                .is_some_and(result_bool)
        {
            self.redundant_condition_propagations.insert(expr.span);
        } else {
            self.redundant_condition_propagations.remove(&expr.span);
        }
        actual
    }

    fn check_expr_arena_inner(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        id: ExprId,
        expected: Option<&Type>,
    ) -> Type {
        if let Some(record) = self.argument_projection_sources.remove(&id) {
            let actual = if let Some((expected, schema)) =
                self.argument_projection_contexts.remove(&record)
            {
                self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(record),
                    Some(&expected),
                    Some(schema),
                )
            } else {
                self.check_expr_arena(arena, source, record, None)
            };
            if let Type::Record(fields) = actual {
                for (projection, ty) in &mut self.argument_projection_types {
                    if let ArenaExprKind::Field { base, name } = arena.arena.expr(*projection).kind
                        && base == record
                        && let Some(actual) = fields.get(&name)
                    {
                        *ty = actual.clone();
                    }
                }
            }
        }
        if let Some(ty) = self.argument_projection_types.get(&id) {
            return ty.clone();
        }
        self.condition_proofs.remove(&id);
        let expr = arena.arena.expr(id);
        if let Some(ty) = self.prepared_constants.types.get(&id) {
            let ty = ty.clone();
            self.record_expr_type(expr.span, ty.clone());
            return ty;
        }
        let ty = match &expr.kind {
            ArenaExprKind::Null => Type::Null,
            ArenaExprKind::Bool(_) => Type::Bool,
            ArenaExprKind::Int(value) => {
                let literal = arena.arena.int_literal(*value);
                // A size literal counts bytes, so it is never negative.
                if literal.is_size() {
                    if literal.value().is_none() {
                        self.error(
                            expr.span,
                            "size literal exceeds 9223372036854775807 bytes",
                            DiagnosticCode::CheckSizeLiteral,
                        );
                    }
                    Type::UInt
                } else {
                    if literal.value().is_none() {
                        self.error(
                            expr.span,
                            "integer literal is outside the 64-bit signed range",
                            DiagnosticCode::CheckIntLiteral,
                        );
                    }
                    Type::Int
                }
            }
            ArenaExprKind::Float(_) => Type::Float,
            ArenaExprKind::Duration(value) => {
                // An unrepresentable literal used to check and then fail
                // preparation, which has no value to encode.
                if arena.arena.duration_literal(*value).millis().is_none() {
                    self.error(
                        expr.span,
                        "duration literal exceeds 18446744073709551615ms",
                        DiagnosticCode::CheckDurationLiteral,
                    );
                }
                Type::Duration
            }
            ArenaExprKind::Str(value) => {
                self.check_string_literal(arena, expr.span, *value, expected)
            }
            ArenaExprKind::Regex(_) => Type::Regex,
            ArenaExprKind::PathStr(_) => Type::Path,
            ArenaExprKind::GlobStr(_) => {
                if self.in_pure {
                    self.error(
                        expr.span,
                        "glob expansion is not allowed in pure functions",
                        DiagnosticCode::CheckPureEffect,
                    );
                }
                Type::List(Box::new(Type::Path))
            }
            ArenaExprKind::FmtString(parts) => {
                self.check_fmt_string_arena(arena, source, *parts, expr.span)
            }
            ArenaExprKind::PathFmtString(parts) => {
                self.check_fmt_string_arena(arena, source, *parts, expr.span);
                Type::Path
            }
            ArenaExprKind::Bytes(_) => Type::Bytes,
            ArenaExprKind::Ident(name) => {
                if *name == "_" {
                    self.pipeline_hole_types.get(&expr.span).cloned().unwrap_or_else(|| {
                        self.error(expr.span, "`_` is only a whole argument placeholder in an immediate value pipeline call", DiagnosticCode::CheckPipelineHole);
                        Type::Invalid
                    })
                } else {
                    // Dollar command identifiers include their sigil in the
                    // expression span. The removed binding edit replaces only
                    // its four-byte name, preserving command interpolation.
                    let name_span = if *name == "ARGV"
                        && expr.span.end() - expr.span.start() == "ARGV".len() + 1
                    {
                        Span::new(expr.span.source_id, expr.span.start() + 1, expr.span.end())
                    } else {
                        expr.span
                    };
                    self.lookup_expr_ident(*name, name_span)
                }
            }
            ArenaExprKind::ValuePipelineCall { input, call, hole } => {
                let input_ty = self.check_expr_arena(arena, source, *input, None);
                let hole_span = arena.arena.expr(*hole).span;
                let previous = self.pipeline_hole_types.insert(hole_span, input_ty);
                let ty = self.check_expr_arena(arena, source, *call, expected);
                match previous {
                    Some(previous) => {
                        self.pipeline_hole_types.insert(hole_span, previous);
                    }
                    None => {
                        self.pipeline_hole_types.remove(&hole_span);
                    }
                }
                ty
            }
            ArenaExprKind::Item => self.check_item_arena(expr.span),
            ArenaExprKind::LastStatus => {
                if !self.last_status_available {
                    self.error(
                        expr.span,
                        "`$?` is not set",
                        DiagnosticCode::CheckLastStatus,
                    );
                }
                Type::Status
            }
            ArenaExprKind::List(items) => {
                self.check_list_arena(arena, source, *items, expected, expr.span)
            }
            ArenaExprKind::Record(fields) => {
                self.check_record_arena(arena, source, *fields, expected, expr.span)
            }
            ArenaExprKind::ErrorContext { message, block } => {
                let ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(*message),
                    Some(&Type::Str),
                    None,
                );
                self.expect_type(&Type::Str, &ty, arena.arena.expr(*message).span);
                self.push_scope();
                let ty = self.check_tail_block_arena(arena, source, *block, expected);
                self.pop_scope();
                ty
            }
            ArenaExprKind::ContextScope {
                kind,
                input,
                block,
                value_body,
            } => {
                use crate::syntax::arena::ContextScopeKind;
                let tail_value = std::mem::replace(&mut self.context_scope_tail_value, false);
                if self.in_pure {
                    self.error(
                        expr.span,
                        "context scopes are not allowed in pure functions",
                        DiagnosticCode::CheckPureEffect,
                    );
                }
                match kind {
                    ContextScopeKind::Cwd | ContextScopeKind::Env => self.require_effect(
                        crate::syntax::node::Effect::Env,
                        expr.span,
                        "context scopes",
                    ),
                    ContextScopeKind::Within => self.require_effect(
                        crate::syntax::node::Effect::Time,
                        expr.span,
                        "`within` scopes",
                    ),
                }
                let input_type = self.check_expr_arena(
                    arena,
                    source,
                    *input,
                    matches!(kind, ContextScopeKind::Within).then_some(&Type::Duration),
                );
                match kind {
                    ContextScopeKind::Within => {
                        if !matches!(
                            input_type,
                            Type::Duration | Type::Unknown | Type::Invalid
                        ) {
                            self.error(
                                arena.arena.expr(*input).span,
                                "`within` requires a Duration",
                                DiagnosticCode::CheckContextScopeInput,
                            );
                        }
                    }
                    ContextScopeKind::Cwd => {
                        if !matches!(
                            input_type,
                            Type::Path | Type::Str | Type::Unknown | Type::Invalid
                        ) {
                            self.error(
                                arena.arena.expr(*input).span,
                                "cwd scope requires Path or Str",
                                DiagnosticCode::CheckContextScopeInput,
                            );
                        }
                    }
                    ContextScopeKind::Env => {
                        let values = match &input_type {
                            Type::Record(fields) => Some(fields.values().collect::<Vec<_>>()),
                            Type::Map(key, value) if **key == Type::Str => {
                                Some(vec![value.as_ref()])
                            }
                            Type::Unknown | Type::Invalid => None,
                            _ => {
                                self.error(
                                    arena.arena.expr(*input).span,
                                    "environment overlay requires a Record or string-keyed Map",
                                    DiagnosticCode::CheckContextScopeInput,
                                );
                                None
                            }
                        };
                        if let Some(values) = values
                            && values
                                .into_iter()
                                .any(|ty| !super::command::can_be_env_value(ty))
                        {
                            self.error(
                                arena.arena.expr(*input).span,
                                "environment values must convert to one scalar argv item or be a List[Path]",
                                DiagnosticCode::CheckEnvValue,
                            );
                        }
                    }
                }
                self.context_scope_depths.push(self.scopes.len());
                self.push_scope();
                let within = usize::from(matches!(kind, ContextScopeKind::Within));
                self.within_block_depth += within;
                let body_type = if *value_body
                    || tail_value
                    || matches!(expected, Some(Type::Result(ok, _)) if **ok != Type::Unit)
                {
                    let expected = match expected {
                        Some(Type::Result(ok, _)) => Some(ok.as_ref()),
                        _ => None,
                    };
                    self.check_tail_block_arena(arena, source, *block, expected)
                } else {
                    self.check_block_arena(arena, source, *block);
                    Type::Unit
                };
                self.within_block_depth -= within;
                self.pop_scope();
                self.context_scope_depths.pop();
                if !body_type.can_escape_context_scope() {
                    self.error(
                        expr.span,
                        "a live producer or host handle cannot escape a restored context",
                        DiagnosticCode::CheckContextScopeEscape,
                    );
                }
                self.context_scope_tail_value = tail_value;
                Type::Result(Box::new(body_type), Box::new(Type::Error))
            }
            ArenaExprKind::TempDirScope { block, value_body } => {
                let tail_value = std::mem::replace(&mut self.context_scope_tail_value, false);
                if self.in_pure {
                    self.error(
                        expr.span,
                        "`tempdir` scopes are not allowed in pure functions",
                        DiagnosticCode::CheckPureEffect,
                    );
                }
                self.require_effect(crate::syntax::node::Effect::Fs, expr.span, "`tempdir`");
                self.push_scope();
                if let Some(param) = arena.arena.block_params(arena.arena.block(*block).params).first()
                    && param.name != "_"
                {
                    self.define(
                        param.name,
                        super::Binding::new(Type::Path, false),
                        arena.arena.span(param.span),
                    );
                }
                let body_type = if *value_body
                    || tail_value
                    || matches!(expected, Some(Type::Result(ok, _)) if **ok != Type::Unit)
                {
                    let expected = match expected {
                        Some(Type::Result(ok, _)) => Some(ok.as_ref()),
                        _ => None,
                    };
                    self.check_tail_block_contents_arena(arena, source, *block, expected)
                } else {
                    self.check_statement_block_contents_arena(arena, source, *block);
                    Type::Unit
                };
                self.pop_scope();
                self.context_scope_tail_value = tail_value;
                Type::Result(Box::new(body_type), Box::new(Type::Error))
            }
            ArenaExprKind::ValueBlock(block) => {
                if let Some(param) = arena
                    .arena
                    .block_params(arena.arena.block(*block).params)
                    .first()
                {
                    self.error(
                        arena.arena.span(param.span),
                        "parameter value blocks require a Result fallback",
                        DiagnosticCode::CheckFallbackBlockContext,
                    );
                }
                self.push_scope();
                let ty = self.check_tail_block_arena(arena, source, *block, expected);
                self.pop_scope();
                ty
            }
            ArenaExprKind::If {
                branches,
                else_value,
            } => self.check_if_expr_arena(arena, source, *branches, *else_value, expected),
            ArenaExprKind::Unary { op, expr: inner } => {
                self.check_unary_arena(arena, source, *op, *inner)
            }
            ArenaExprKind::ComparisonChain(pairs) => {
                let mut previous = None;
                for pair in arena.arena.expr_ids(*pairs) {
                    let ArenaExprKind::Binary { left, right, .. } = arena.arena.expr(pair).kind
                    else {
                        unreachable!()
                    };
                    let left_ty = previous
                        .take()
                        .unwrap_or_else(|| self.check_expr_arena(arena, source, left, None));
                    let right_ty = self.check_expr_with_schema_arena(
                        arena,
                        source,
                        ArenaExprOrRun::Expr(right),
                        Some(&left_ty),
                        None,
                    );
                    if !matches!(
                        left_ty,
                        Type::Int
                            | Type::UInt
                            | Type::Float
                            | Type::Duration
                            | Type::Str
                            | Type::Any
                            | Type::Unknown
                    ) {
                        self.error(
                            arena.arena.expr(left).span,
                            "comparison requires Int, Float, Str, or Duration",
                            DiagnosticCode::CheckOperatorType,
                        );
                    }
                    self.expect_type(&left_ty, &right_ty, arena.arena.expr(right).span);
                    previous = Some(right_ty);
                }
                Type::Bool
            }
            ArenaExprKind::Binary { op, left, right } => {
                self.check_binary_arena(arena, source, *op, *left, *right, expected)
            }
            ArenaExprKind::Field { base, name }
                if matches!(arena.arena.expr(*base).kind, ArenaExprKind::Item)
                    && !self.item_shorthand_in_scope() =>
            {
                self.check_inferred_variant_value(*name, expected, expr.span)
            }
            ArenaExprKind::Field { base, name } => {
                self.check_field_arena(arena, source, *base, *name, expr.span, expected)
            }
            ArenaExprKind::NullSafeField { base, name } => {
                self.check_null_safe_field_arena(arena, source, *base, *name, expr.span)
            }
            ArenaExprKind::Index {
                base,
                index,
                guarded,
            } => self.check_index_arena(arena, source, *base, *index, *guarded, expr.span),
            ArenaExprKind::Slice {
                base,
                start,
                end,
                guarded,
            } => self.check_slice_arena(arena, source, *base, *start, *end, *guarded, expr.span),
            ArenaExprKind::EnvString(_) => self.check_env_string(expr.span),
            ArenaExprKind::EnvPathList => {
                self.require_effect(Effect::Env, expr.span, "environment path lookup");
                Type::EnvPathList
            }
            ArenaExprKind::Pipeline { stages, .. } => {
                self.report_value_stage_not_call(arena, source, expr.span, *stages);
                Type::Unknown
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                self.check_structured_pipeline_arena(arena, source, *input, *stages)
            }
            ArenaExprKind::Try(inner) => {
                let inner_expected = expected
                    .cloned()
                    .map(|ty| Type::Result(Box::new(ty), Box::new(Type::Error)));
                let schema = self.expected_schema.clone().map(|schema| {
                    let mut wrapped = crate::sema::constants::SchemaExpectation::default();
                    wrapped
                        .children
                        .insert(crate::sema::constants::SchemaComponent::Success, schema);
                    wrapped
                });
                let ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(*inner),
                    inner_expected.as_ref(),
                    schema,
                );
                self.note_run_propagated(arena, *inner);
                self.check_propagation(&ty, expr.span)
            }
            ArenaExprKind::Require { value, schema } => {
                self.requirement_targets.remove(&expr.span);
                self.requirement_expected_targets.remove(&expr.span);
                let inferred = super::expected::infer_requirement_target(
                    &arena.arena,
                    expected,
                    self.expected_schema.as_ref(),
                    &self.type_constraints,
                );
                if let Some(inferred) = &inferred {
                    self.requirement_expected_targets
                        .insert(expr.span, inferred.clone());
                }
                self.check_expr_arena(arena, source, *value, None);
                let target = if let Some(schema) = schema {
                    let ty = self.type_from_arena(arena, *schema);
                    let ty = if self
                        .reject_typed_callable_test(&ty, arena.arena.type_expr_span(*schema))
                    {
                        Type::Invalid
                    } else {
                        ty
                    };
                    let context = self
                        .record_constructors
                        .annotation_expectation(&arena.arena, *schema, self.current_namespace)
                        .unwrap_or_default();
                    Some(super::expected::requirement_target(
                        &arena.arena,
                        ty,
                        context,
                    ))
                } else {
                    inferred
                };
                if let Some(target) = target {
                    let ty = target.ty.clone();
                    self.requirement_targets.insert(expr.span, target);
                    Type::Result(Box::new(ty), Box::new(Type::Error))
                } else {
                    self.error(expr.span, "cannot infer require target; supply a schema or an independently typed boundary", DiagnosticCode::CheckRequireTarget);
                    Type::Invalid
                }
            }
            ArenaExprKind::Call { callee, args } => {
                let may_mutate = matches!(arena.arena.expr(*callee).kind, ArenaExprKind::Ident(name) if self.lookup(name).is_some_and(|binding| binding.ty == Type::Proc));
                let diagnostics_before = self.diagnostics.len();
                let result =
                    self.check_call_arena(arena, source, *callee, *args, expr.span, expected);
                if self.diagnostics.len() == diagnostics_before
                    && self.stage_callable_is_static(arena, *callee)
                {
                    self.statically_resolved_call_spans.insert(expr.span);
                }
                let erased_proc_call = matches!(arena.arena.expr(*callee).kind, ArenaExprKind::Field { base, name } if name == "call"
                    && self.expr_types.get(&arena.arena.expr(base).span) == Some(&Type::Proc));
                if may_mutate || erased_proc_call {
                    self.invalidate_mutable_narrowings();
                }
                result
            }
            ArenaExprKind::PatternCondition { value, arms } => {
                let value_ty = self.check_expr_arena(arena, source, *value, None);
                let pattern = arena.arena.match_expr_arms(*arms)[0].pattern;
                // Over an error family only a catch-all counts as unable to
                // fail here. Variant and facet patterns that happen to cover
                // every variant of the family stay accepted conditions: they
                // were before families became closed for `match`, and
                // rejecting them needs a rewrite to offer first.
                let coverage_ty = match &value_ty {
                    Type::ErrorFamily(_) => &Type::Error,
                    other => other,
                };
                let cannot_fail = super::stmt::patterns_are_exhaustive_arena(
                    arena,
                    coverage_ty,
                    std::iter::once(pattern),
                    &self.exhaustiveness_facts(),
                );
                // A pattern that cannot fail on its own still has a failure
                // case over an optional subject: `null`. That is optional
                // binding, and the pattern then sees the non-null value. The
                // fact is rewritten on every check of this condition.
                self.optional_binding_spans.remove(&expr.span);
                let subject_ty = match value_ty {
                    Type::Optional(present) if cannot_fail => {
                        self.optional_binding_spans.insert(expr.span);
                        *present
                    }
                    value_ty => {
                        if cannot_fail {
                            self.error(
                                expr.span,
                                "pattern condition cannot fail; bind the subject with `let` instead",
                                DiagnosticCode::CheckIrrefutablePatternCondition,
                            );
                        }
                        value_ty
                    }
                };
                self.push_scope();
                self.check_pattern_arena(arena, source, pattern, &subject_ty);
                self.pop_scope();
                Type::Bool
            }
            ArenaExprKind::PatternTest { value, arms } => {
                let value_ty = self.check_expr_arena(arena, source, *value, None);
                let pattern = arena.arena.match_expr_arms(*arms)[0].pattern;
                self.check_nonbinding_pattern_arena(arena, source, pattern, &value_ty);
                Type::Bool
            }
            ArenaExprKind::Match { value, arms } => {
                self.check_match_expr_arena(arena, source, *value, *arms, expected, expr.span)
            }
            ArenaExprKind::ListComp {
                expr: body,
                qualifiers,
            } => self.check_list_comp_arena(arena, source, *body, *qualifiers, expected, expr.span),
            ArenaExprKind::MapComp {
                key,
                value,
                qualifiers,
            } => self.check_map_comp_arena(
                arena,
                source,
                *key,
                *value,
                *qualifiers,
                expected,
                expr.span,
            ),
            ArenaExprKind::Loop { block } => {
                self.check_loop_arena(arena, source, *block, expr.span)
            }
            ArenaExprKind::Capture(block) => {
                self.check_capture_arena(arena, source, *block, expected, expr.span)
            }
            ArenaExprKind::Retry {
                delays,
                pattern,
                block,
            } => self.check_retry_arena(
                arena, source, *delays, *pattern, *block, expected, expr.span,
            ),
            ArenaExprKind::Run(run_id) => self.check_run_expr_arena(arena, source, *run_id),
            ArenaExprKind::Spawn(form) => self.check_spawn_form_arena(arena, source, form),
            ArenaExprKind::Wait(form) => self.check_wait_form_arena(arena, source, form),
            ArenaExprKind::BuilderCall { call, block } => {
                self.check_builder_call_arena(arena, source, *call, *block, expr.span)
            }
        };
        self.record_expr_type(expr.span, ty.clone());
        if ty == Type::Bool {
            let proof = self.infer_condition_proof_arena(arena, id);
            self.condition_proofs.insert(id, std::sync::Arc::new(proof));
        }
        ty
    }

    fn check_fmt_string_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        range: ArenaRange,
        span: Span,
    ) -> Type {
        self.check_fmt_dollar_names(source, span);
        for part in arena.arena.fmt_parts(range) {
            if let ArenaFmtPart::Expr(expr_id, _) = part {
                let ty = self.check_expr_arena(arena, source, expr_id, None);
                if ty == Type::Any {
                    self.reject_dynamic_use(
                        "an interpolation",
                        None,
                        arena.arena.expr(expr_id).span,
                    );
                } else if !ty.can_display() && !matches!(ty, Type::Unknown) {
                    let span = arena.arena.expr(expr_id).span;
                    self.report_conversion(
                        span,
                        &ty,
                        "cannot be displayed in fmt string",
                        DiagnosticCode::CheckDisplayConversion,
                    );
                }
            }
        }
        Type::Str
    }

    /// A value stage lowers only as a call (`ArenaProgramBuilder::build_value_pipeline_stage`).
    /// The parser builds a `Pipeline` node only when it cannot, and that node's
    /// first stage is the offending one. An operator expression whose leftmost
    /// operand is a call or a name, such as `xs |> len() == 1`, means the
    /// operator for the whole pipeline, which needs grouping like any pipeline
    /// an operator applies to; any other stage lacks its call.
    fn report_value_stage_not_call(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        pipeline: Span,
        stages: ArenaRange,
    ) {
        use crate::syntax::arena::ArenaPipeStageKind;
        let ast = &arena.arena;
        let Some(&ArenaPipeStageKind::Expr(stage)) =
            ast.pipe_stages(stages).first().map(|stage| &stage.kind)
        else {
            self.error(
                pipeline,
                "a pipeline without a leading value stage reached the checker",
                DiagnosticCode::CheckDesugar,
            );
            return;
        };
        let is_call = |expr: ExprId| {
            let call = match ast.expr(expr).kind {
                ArenaExprKind::Try(inner) => inner,
                _ => expr,
            };
            matches!(ast.expr(call).kind, ArenaExprKind::Call { .. })
        };
        let names_callee = |expr: ExprId| {
            matches!(
                ast.expr(expr).kind,
                ArenaExprKind::Ident(_) | ArenaExprKind::Field { .. }
            )
        };
        let mut operand = stage;
        loop {
            operand = match ast.expr(operand).kind {
                ArenaExprKind::Binary { left, .. } => left,
                ArenaExprKind::ComparisonChain(pairs) => ast
                    .comparison_chain_operands(pairs)
                    .next()
                    .expect("a comparison chain has operands"),
                ArenaExprKind::PatternTest { value, .. } => value,
                _ => break,
            };
        }
        let diagnostic = if operand != stage && (is_call(operand) || names_callee(operand)) {
            let span = Span::new(
                pipeline.source_id,
                pipeline.start(),
                ast.expr(operand).span.end(),
            );
            let diagnostic = Diagnostic::error("group a pipeline that an operator applies to")
                .with_code(DiagnosticCode::CheckAmbiguousGrouping)
                .with_label(Label::primary(span, "add parentheses around this operand"));
            match source.get(span.range()) {
                Some(text) => diagnostic.with_fix_hint(FixHint::replacement(
                    span,
                    "add parentheses",
                    format!("({text})"),
                )),
                None => diagnostic,
            }
        } else {
            let span = ast.expr(stage).span;
            let diagnostic = Diagnostic::error("a value pipeline stage must be a call")
                .with_code(DiagnosticCode::CheckPipelineStage)
                .with_label(Label::primary(
                    span,
                    "call a method or function here, or name a stream stage",
                ));
            match source.get(span.range()).filter(|_| names_callee(stage)) {
                Some(text) => diagnostic.with_fix_hint(FixHint::replacement(
                    span,
                    "call it",
                    format!("{text}()"),
                )),
                None => diagnostic,
            }
        };
        self.diagnostics.push(diagnostic);
    }

    /// `$` is plain text in an f-string, so a shell habit such as
    /// `f"$name"` would print `$` before the value's name instead of the
    /// value. When the name is a binding in scope, that is an error.
    fn check_fmt_dollar_names(&mut self, source: &str, span: Span) {
        let Some(literal::QuotedScan::Terminated(quoted)) = source
            .get(span.range())
            .and_then(|_| literal::scan_quoted_literal(source, span.start(), true))
        else {
            return;
        };
        if quoted.raw || quoted.end != span.end() {
            return;
        }
        for range in literal::fmt_text_dollar_names(source, quoted) {
            let name = &source[range.start + 1..range.end];
            if self.lookup(Name::intern(name)).is_none() {
                continue;
            }
            let dollar = Span::new(span.source_id, range.start, range.end);
            self.diagnostics.push(
                Diagnostic::error(format!(
                    "f-strings interpolate with `{{{name}}}`; `${name}` is literal text"
                ))
                .with_code(DiagnosticCode::CheckFmtDollarName)
                .with_label(Label::primary(
                    dollar,
                    format!("`{name}` is a binding in scope"),
                ))
                .with_fix_hint(FixHint::replacement(
                    dollar,
                    format!("interpolate with `{{{name}}}`"),
                    format!("{{{name}}}"),
                ))
                .with_note("write `\\$` for a literal dollar sign"),
            );
        }
    }

    fn check_list_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        range: crate::syntax::arena::ArenaListElementRange,
        expected: Option<&Type>,
        _span: Span,
    ) -> Type {
        if range.is_empty() && matches!(expected, Some(Type::Any)) {
            return Type::List(Box::new(Type::Any));
        }
        let expected_item = match expected {
            Some(Type::List(item)) => Some(item.as_ref()),
            _ => None,
        };
        let mut inferred = expected_item.cloned().unwrap_or(Type::Unknown);
        for item in arena.arena.list_elements(range) {
            let item_expected = expected_item;
            let span = item
                .splice_span
                .map(|span| arena.arena.span(span))
                .unwrap_or(arena.arena.expr(item.value).span);
            let actual = if item.splice_span.is_some() {
                let list_expected = item_expected.cloned().map(|ty| Type::List(Box::new(ty)));
                let actual =
                    self.check_expr_arena(arena, source, item.value, list_expected.as_ref());
                match actual {
                    Type::List(ty) => *ty,
                    Type::Unknown => Type::Unknown,
                    _ => {
                        self.error(span, "list literal splice requires List; handle Results explicitly and collect Streams explicitly", DiagnosticCode::CheckListSpliceType);
                        Type::Unknown
                    }
                }
            } else {
                let schema = self
                    .expected_schema
                    .as_ref()
                    .and_then(|schema| {
                        schema
                            .value_context()
                            .children
                            .get(&crate::sema::constants::SchemaComponent::Item)
                    })
                    .cloned();
                self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(item.value),
                    item_expected,
                    schema,
                )
            };
            if inferred == Type::Unknown {
                inferred = actual;
            } else if let Some(expected_item) = expected_item {
                self.expect_type(expected_item, &actual, span);
            } else if let Some(merged) = merge_list_literal_item_ty(&inferred, &actual) {
                inferred = merged;
            } else {
                self.expect_type(&inferred, &actual, span);
            }
        }
        Type::List(Box::new(inferred))
    }

    fn check_schema_child_expr_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        expression: ExprId,
        expected: Option<&Type>,
        component: crate::sema::constants::SchemaComponent,
    ) -> Type {
        let schema = self
            .expected_schema
            .as_ref()
            .and_then(|schema| schema.value_context().children.get(&component))
            .cloned();
        self.check_expr_with_schema_arena(
            arena,
            source,
            ArenaExprOrRun::Expr(expression),
            expected,
            schema,
        )
    }

    fn check_map_literal_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        range: ArenaRange,
        expected: Option<&Type>,
    ) -> Type {
        let (expected_key, expected_item) = match expected {
            Some(Type::Map(key, item)) => (Some(key.as_ref()), Some(item.as_ref())),
            _ => (None, None),
        };
        let mut inferred_key = expected_key.cloned().unwrap_or(Type::Unknown);
        let mut inferred = expected_item.cloned().unwrap_or(Type::Unknown);
        for field in arena.arena.record_fields(range) {
            let (key_ty, actual, span) = match field.kind {
                ArenaRecordFieldKind::Computed { key, value, span } => {
                    let key_ty = self.check_schema_child_expr_arena(
                        arena,
                        source,
                        key,
                        expected_key,
                        crate::sema::constants::SchemaComponent::Key,
                    );
                    if !key_ty.is_map_key() && !key_ty.is_recovery() {
                        self.error(
                            arena.arena.expr(key).span,
                            "Map keys require Str, Int, UInt, Bool, Bytes, Path, or Duration",
                            DiagnosticCode::CheckMapKeyType,
                        );
                    }
                    (
                        key_ty,
                        self.check_schema_child_expr_arena(
                            arena,
                            source,
                            value,
                            expected_item,
                            crate::sema::constants::SchemaComponent::Value,
                        ),
                        arena.arena.span(span),
                    )
                }
                ArenaRecordFieldKind::Path { value, span, .. } => {
                    self.check_schema_child_expr_arena(
                        arena,
                        source,
                        value,
                        expected_item,
                        crate::sema::constants::SchemaComponent::Value,
                    );
                    self.error(
                        arena.arena.span(span),
                        "map literals do not permit static record update paths",
                        DiagnosticCode::CheckMapUpdatePath,
                    );
                    continue;
                }
                ArenaRecordFieldKind::Named { name, value, span } => (
                    self.map_label_key_type(name, expected_key, arena.arena.span(span)),
                    self.check_schema_child_expr_arena(
                        arena,
                        source,
                        value,
                        expected_item,
                        crate::sema::constants::SchemaComponent::Value,
                    ),
                    arena.arena.span(span),
                ),
                ArenaRecordFieldKind::Shorthand { name, span } => (
                    Type::Str,
                    self.lookup_record_shorthand(name, arena.arena.span(span)),
                    arena.arena.span(span),
                ),
                ArenaRecordFieldKind::Spread { expr, span } => {
                    let ty = self.check_expr_arena(arena, source, expr, expected);
                    match ty {
                        Type::Map(key, item) => (*key, *item, arena.arena.span(span)),
                        Type::Unknown => (Type::Unknown, Type::Unknown, arena.arena.span(span)),
                        _ => {
                            self.error(
                                arena.arena.span(span),
                                "map literal spreads require Map",
                                DiagnosticCode::CheckMapSpreadType,
                            );
                            (Type::Unknown, Type::Unknown, arena.arena.span(span))
                        }
                    }
                }
            };
            if inferred_key == Type::Unknown {
                inferred_key = key_ty;
            } else {
                self.expect_type(&inferred_key, &key_ty, span);
            }
            if let Some(expected_item) = expected_item {
                self.expect_type(expected_item, &actual, span);
            } else if inferred == Type::Unknown {
                inferred = actual;
            } else if let Some(merged) = merge_list_literal_item_ty(&inferred, &actual) {
                inferred = merged;
            } else {
                self.expect_type(&inferred, &actual, span);
            }
        }
        if inferred_key == Type::Unknown {
            inferred_key = Type::Str;
        }
        Type::Map(Box::new(inferred_key), Box::new(inferred))
    }

    fn check_record_update_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        range: ArenaRange,
        span: Span,
    ) -> Type {
        fn requires_validation(actual: &Type, expected: &Type) -> bool {
            if actual.any_flows_to_concrete(expected) {
                return true;
            }
            match (actual, expected) {
                (Type::Record(actual), Type::Record(expected)) if !expected.is_empty() => {
                    actual.is_empty()
                        || expected.iter().any(|(name, expected)| {
                            actual
                                .get(name)
                                .is_some_and(|actual| requires_validation(actual, expected))
                        })
                }
                (Type::List(actual), Type::List(expected))
                | (Type::Optional(actual), Type::Optional(expected)) => {
                    requires_validation(actual, expected)
                }
                (Type::Map(actual_key, actual_value), Type::Map(expected_key, expected_value)) => {
                    requires_validation(actual_key, expected_key)
                        || requires_validation(actual_value, expected_value)
                }
                (Type::Result(actual, error), Type::Result(expected, expected_error)) => {
                    requires_validation(actual, expected)
                        || requires_validation(error, expected_error)
                }
                _ => false,
            }
        }
        let fields = arena.arena.record_fields(range);
        let base_ty = match fields.first().map(|field| &field.kind) {
            Some(ArenaRecordFieldKind::Spread { expr, .. }) => {
                self.check_expr_arena(arena, source, *expr, None)
            }
            _ => {
                self.error(
                    span,
                    "nested record updates require one leading record spread",
                    DiagnosticCode::CheckRecordUpdateBase,
                );
                Type::Unknown
            }
        };
        if !matches!(&base_ty, Type::Record(fields) if !fields.is_empty()) {
            self.error(
                span,
                "nested record updates require a statically known record shape",
                DiagnosticCode::CheckRecordUpdateShape,
            );
        }
        let mut targets: Vec<Vec<Name>> = Vec::new();
        for (index, field) in fields.iter().enumerate() {
            let (path, value, field_span) = match &field.kind {
                ArenaRecordFieldKind::Spread { expr, span } => {
                    if index != 0 {
                        self.error(
                            arena.arena.span(*span),
                            "nested record updates permit only the leading spread",
                            DiagnosticCode::CheckRecordUpdateBase,
                        );
                        self.check_expr_arena(arena, source, *expr, None);
                    }
                    continue;
                }
                ArenaRecordFieldKind::Computed { key, value, span } => {
                    self.check_expr_arena(arena, source, *key, Some(&Type::Str));
                    self.check_expr_arena(arena, source, *value, None);
                    self.error(
                        arena.arena.span(*span),
                        "nested record updates do not permit computed map keys",
                        DiagnosticCode::CheckMapUpdatePath,
                    );
                    continue;
                }
                ArenaRecordFieldKind::Path { path, value, span } => (
                    arena.arena.names(*path).collect::<Vec<_>>(),
                    Some(*value),
                    arena.arena.span(*span),
                ),
                ArenaRecordFieldKind::Named { name, value, span } => {
                    (vec![*name], Some(*value), arena.arena.span(*span))
                }
                ArenaRecordFieldKind::Shorthand { name, span } => {
                    (vec![*name], None, arena.arena.span(*span))
                }
            };
            if targets
                .iter()
                .any(|prior| prior.starts_with(&path) || path.starts_with(prior))
            {
                self.error(
                    field_span,
                    "record update targets must be disjoint",
                    DiagnosticCode::CheckRecordUpdateOverlap,
                );
            }
            targets.push(path.clone());
            let mut selected = Some(&base_ty);
            for name in &path {
                selected = match selected {
                    Some(Type::Record(fields)) if !fields.is_empty() => fields.get(name),
                    _ => None,
                };
                if selected.is_none() {
                    break;
                }
            }
            if selected.is_none() {
                self.error(
                    field_span,
                    "every update target must select an existing field through known records",
                    DiagnosticCode::CheckRecordUpdateField,
                );
            }
            let actual = match value {
                Some(value) => self.check_expr_arena(arena, source, value, selected),
                None => self.lookup_record_shorthand(path[0], field_span),
            };
            if let Some(selected) = selected {
                if requires_validation(&actual, selected) {
                    self.error(
                        field_span,
                        "record update replacements require a checked field type",
                        DiagnosticCode::CheckRecordUpdateValue,
                    );
                } else {
                    self.expect_type(selected, &actual, field_span);
                }
            }
        }
        base_ty
    }

    fn check_record_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        range: ArenaRange,
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        let fields = arena.arena.record_fields(range);
        if fields
            .iter()
            .any(|field| matches!(field.kind, ArenaRecordFieldKind::Path { .. }))
        {
            return self.check_record_update_arena(arena, source, range, span);
        }
        if matches!(expected, Some(Type::Map(_, _)))
            || fields
                .iter()
                .any(|field| matches!(field.kind, ArenaRecordFieldKind::Computed { .. }))
        {
            return self.check_map_literal_arena(arena, source, range, expected);
        }
        if matches!(
            expected,
            Some(Type::Status | Type::ProcessHandle | Type::NetJob | Type::FsRoot)
        ) {
            for field in fields {
                match &field.kind {
                    ArenaRecordFieldKind::Computed { .. } => {
                        unreachable!("computed fields select Map checking")
                    }
                    ArenaRecordFieldKind::Spread { expr, .. }
                    | ArenaRecordFieldKind::Named { value: expr, .. }
                    | ArenaRecordFieldKind::Path { value: expr, .. } => {
                        self.check_expr_arena(arena, source, *expr, None);
                    }
                    ArenaRecordFieldKind::Shorthand { name, span } => {
                        let field_span = arena.arena.span(*span);
                        self.lookup_record_shorthand(*name, field_span);
                    }
                }
            }
            if matches!(expected, Some(Type::ProcessHandle)) {
                self.error(
                    span,
                    "`ProcessHandle` is a runtime-only type and cannot be constructed with a record literal; obtain it from `spawn`",
                    DiagnosticCode::CheckTypeMismatch,
                );
                return Type::Unknown;
            }
            if matches!(expected, Some(Type::FsRoot)) {
                self.error(span, "`FsRoot` is an opaque runtime capability and cannot be constructed with a record literal; obtain it from a filesystem root factory", DiagnosticCode::CheckTypeMismatch);
                return Type::Unknown;
            }
            if matches!(expected, Some(Type::NetJob)) {
                self.error(
                    span,
                    "`NetJob` is a runtime-only type and cannot be constructed with a record literal; obtain it from `net.start`",
                    DiagnosticCode::CheckTypeMismatch,
                );
                return Type::Unknown;
            }
            self.error(
                span,
                "`Status` is a runtime-only type and cannot be constructed with a record literal; obtain it from `process.run`, `run.status`, or similar",
                DiagnosticCode::CheckTypeMismatch,
            );
            return Type::Unknown;
        }

        let mut record = BTreeMap::new();
        let expected_fields = match expected {
            Some(Type::Record(fields)) if !fields.is_empty() => Some(fields),
            _ => None,
        };
        let mut has_spread = false;
        let mut last_span = span;
        for field in fields {
            match &field.kind {
                ArenaRecordFieldKind::Computed { .. } => {
                    unreachable!("computed fields select Map checking")
                }
                ArenaRecordFieldKind::Spread { expr, span } => {
                    has_spread = true;
                    last_span = arena.arena.span(*span);
                    let ty = self.check_expr_arena(arena, source, *expr, None);
                    match ty {
                        Type::Record(spread_fields) => {
                            for (k, v) in spread_fields {
                                record.entry(k).or_insert(v);
                            }
                        }
                        Type::Any | Type::Unknown => {}
                        _ => {
                            self.error(
                                last_span,
                                "spread must be a record",
                                DiagnosticCode::CheckSpreadNotRecord,
                            );
                        }
                    }
                }
                ArenaRecordFieldKind::Path { .. } => {
                    unreachable!("record updates use their own checker")
                }
                ArenaRecordFieldKind::Named { name, value, span } => {
                    let field_span = arena.arena.span(*span);
                    last_span = field_span;
                    if record.contains_key(name) && !has_spread {
                        self.error(
                            field_span,
                            "duplicate record field",
                            DiagnosticCode::CheckDuplicateRecordField,
                        );
                    }
                    if let Some(expected_fields) = expected_fields
                        && !expected_fields.contains_key(name)
                    {
                        self.error(
                            field_span,
                            "unknown schema field",
                            DiagnosticCode::CheckSchemaField,
                        );
                    }
                    let field_expected = expected_fields.and_then(|fields| fields.get(name));
                    let schema = self
                        .expected_schema
                        .as_ref()
                        .and_then(|schema| {
                            schema
                                .value_context()
                                .children
                                .get(&crate::sema::constants::SchemaComponent::Field(*name))
                        })
                        .cloned();
                    let ty = self.check_expr_with_schema_arena(
                        arena,
                        source,
                        ArenaExprOrRun::Expr(*value),
                        field_expected,
                        schema,
                    );
                    if let Some(field_expected) = field_expected {
                        let value_span = arena.arena.expr(*value).span;
                        self.expect_type(field_expected, &ty, value_span);
                    }
                    record.insert(*name, ty);
                }
                ArenaRecordFieldKind::Shorthand { name, span } => {
                    let field_span = arena.arena.span(*span);
                    last_span = field_span;
                    if record.contains_key(name) && !has_spread {
                        self.error(
                            field_span,
                            "duplicate record field",
                            DiagnosticCode::CheckDuplicateRecordField,
                        );
                    }
                    if let Some(expected_fields) = expected_fields
                        && !expected_fields.contains_key(name)
                    {
                        self.error(
                            field_span,
                            "unknown schema field",
                            DiagnosticCode::CheckSchemaField,
                        );
                    }
                    let ty = self.lookup_record_shorthand(*name, field_span);
                    if let Some(field_expected) =
                        expected_fields.and_then(|fields| fields.get(name))
                    {
                        self.expect_type(field_expected, &ty, field_span);
                    }
                    record.insert(*name, ty);
                }
            }
        }
        if let Some(expected_fields) = expected_fields
            && !has_spread
        {
            for name in expected_fields.keys() {
                if !record.contains_key(name) {
                    self.error(
                        last_span,
                        "missing schema field",
                        DiagnosticCode::CheckSchemaField,
                    );
                }
            }
        }
        Type::Record(record)
    }

    fn check_if_expr_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        branches: ArenaRange,
        else_value: ExprId,
        expected: Option<&Type>,
    ) -> Type {
        let mut inferred = None;
        for branch in arena.arena.if_expr_branches(branches) {
            let narrowings = self.check_condition_arena(
                arena,
                source,
                branch.condition,
                DiagnosticCode::CheckIfCondition,
            );
            self.push_scope();
            self.apply_narrowings(&narrowings.when_true);
            self.bind_pattern_condition_arena(arena, source, branch.condition);
            let infer_branches = self.inferred_returns.is_some() && expected.is_none();
            let branch_expected = if infer_branches {
                None
            } else {
                expected.or(inferred.as_ref())
            };
            let actual = self.check_expr_arena(arena, source, branch.value, branch_expected);
            if let Some(branch_expected) = branch_expected {
                let value_span = arena.arena.expr(branch.value).span;
                self.expect_type(branch_expected, &actual, value_span);
            }
            if actual != Type::Unknown {
                inferred = Some(if infer_branches {
                    inferred.map_or(actual.clone(), |previous| {
                        self.unify_inferred_returns(
                            previous,
                            actual,
                            arena.arena.expr(branch.value).span,
                        )
                    })
                } else {
                    inferred.unwrap_or(actual)
                });
            }
            self.pop_scope();
        }
        self.push_scope();
        if arena.arena.if_expr_branches(branches).len() == 1 {
            let narrowings = self.infer_condition_narrowings_arena(
                arena,
                arena.arena.if_expr_branches(branches)[0].condition,
            );
            self.apply_narrowings(&narrowings.when_false);
        }
        let infer_branches = self.inferred_returns.is_some() && expected.is_none();
        let else_expected = if infer_branches {
            None
        } else {
            expected.or(inferred.as_ref())
        };
        let else_ty = self.check_expr_arena(arena, source, else_value, else_expected);
        if let Some(else_expected) = else_expected {
            let else_span = arena.arena.expr(else_value).span;
            self.expect_type(else_expected, &else_ty, else_span);
        }
        self.pop_scope();
        if infer_branches {
            inferred.map_or(else_ty.clone(), |previous| {
                self.unify_inferred_returns(previous, else_ty, arena.arena.expr(else_value).span)
            })
        } else {
            expected.cloned().or(inferred).unwrap_or(else_ty)
        }
    }

    fn check_comp_qualifiers_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        qualifiers: ArenaRange,
        map: bool,
    ) -> usize {
        let mut scopes = 0;
        for qualifier in arena.arena.comp_qualifiers(qualifiers) {
            match *qualifier {
                ArenaCompQualifier::For { target, iter, span } => {
                    let iter_ty = self.check_expr_arena(arena, source, iter, None);
                    if matches!(&iter_ty, Type::Result(ok, _) if matches!(ok.as_ref(), Type::Map(_, _) | Type::Str | Type::Bytes))
                    {
                        self.note_run_propagated(arena, iter);
                        self.check_propagation(&iter_ty, arena.arena.expr(iter).span);
                    }
                    let item_ty = iter_ty.iteration_item_type().unwrap_or_else(|| {
                        if matches!(iter_ty, Type::Any | Type::Unknown) { Type::Any } else if self.reject_unnarrowed_union(&iter_ty, "iteration", arena.arena.expr(iter).span) { Type::Unknown } else {
                            self.error(arena.arena.expr(iter).span, "comprehension iterates over List, Stream, Map, Str, or Bytes values", if map { DiagnosticCode::CheckMapcompIterator } else { DiagnosticCode::CheckListcompIterator });
                            Type::Unknown
                        }
                    });
                    self.push_scope();
                    scopes += 1;
                    self.define_binding_target_arena(arena, target, &item_ty, false, span);
                }
                ArenaCompQualifier::If { condition, .. } => {
                    let ty = self.check_expr_arena(arena, source, condition, None);
                    if ty == Type::Any {
                        self.expect_type(&Type::Bool, &ty, arena.arena.expr(condition).span);
                    } else if !matches!(ty, Type::Bool | Type::Status | Type::Unknown) {
                        self.error(
                            arena.arena.expr(condition).span,
                            "comprehension condition must be Bool or Status",
                            if map {
                                DiagnosticCode::CheckMapcompCondition
                            } else {
                                DiagnosticCode::CheckListcompCondition
                            },
                        );
                    }
                }
            }
        }
        scopes
    }

    fn check_list_comp_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        body: ExprId,
        qualifiers: ArenaRange,
        expected: Option<&Type>,
        _span: Span,
    ) -> Type {
        let scopes = self.check_comp_qualifiers_arena(arena, source, qualifiers, false);
        let expected_item = match expected {
            Some(Type::List(item)) => Some(item.as_ref()),
            _ => None,
        };
        let elem_ty = self.check_expr_arena(arena, source, body, expected_item);
        if let Some(expected_item) = expected_item {
            self.expect_type(expected_item, &elem_ty, arena.arena.expr(body).span);
        }
        for _ in 0..scopes {
            self.pop_scope();
        }
        Type::List(Box::new(expected_item.cloned().unwrap_or(elem_ty)))
    }

    fn check_map_comp_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        key: ExprId,
        value: ExprId,
        qualifiers: ArenaRange,
        expected: Option<&Type>,
        _span: Span,
    ) -> Type {
        let scopes = self.check_comp_qualifiers_arena(arena, source, qualifiers, true);
        let (expected_key, expected_value) = match expected {
            Some(Type::Map(key, value)) => (Some(key.as_ref()), Some(value.as_ref())),
            _ => (None, None),
        };
        let key_ty = self.check_expr_arena(arena, source, key, expected_key);
        if !key_ty.is_map_key() && !key_ty.is_recovery() {
            self.error(
                arena.arena.expr(key).span,
                "Map comprehension keys require an ordered scalar key",
                DiagnosticCode::CheckMapKeyType,
            );
        }
        if let Some(expected_key) = expected_key {
            self.expect_type(expected_key, &key_ty, arena.arena.expr(key).span);
        }
        let value_ty = self.check_expr_arena(arena, source, value, expected_value);
        if let Some(expected_value) = expected_value {
            self.expect_type(expected_value, &value_ty, arena.arena.expr(value).span);
        }
        for _ in 0..scopes {
            self.pop_scope();
        }
        Type::Map(
            Box::new(expected_key.cloned().unwrap_or(key_ty)),
            Box::new(expected_value.cloned().unwrap_or(value_ty)),
        )
    }

    fn check_loop_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block: BlockId,
        span: Span,
    ) -> Type {
        self.loop_depth += 1;
        self.check_block_arena(arena, source, block);
        self.loop_depth -= 1;
        if !block_has_exit_point_arena(arena, block) {
            self.error(
                span,
                "`loop` has no `break` — will run forever",
                DiagnosticCode::CheckLoopNoBreak,
            );
        }
        Type::Unknown
    }

    fn check_capture_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block: BlockId,
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        let (expected_ok, expected_error) = match expected {
            Some(Type::Result(ok, error)) => (Some(ok.as_ref()), Some(error.as_ref())),
            _ => (None, None),
        };
        self.push_scope();
        self.begin_error_boundary();
        let body = self.check_tail_block_arena(arena, source, block, expected_ok);
        let error = self.end_error_boundary(expected_error);
        self.pop_scope();
        let body = if let Some(expected_ok) = expected_ok {
            if capture_success_underconstrained(&body) && body.matches_expected(expected_ok) {
                expected_ok.clone()
            } else {
                body
            }
        } else {
            body
        };
        if capture_success_underconstrained(&body) {
            self.error(
                span,
                "cannot infer try success type; annotate Result success type",
                DiagnosticCode::CheckTrySuccessType,
            );
        }
        Type::Result(Box::new(body), Box::new(error))
    }

    fn check_retry_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        delays: ArenaRange,
        pattern: Option<PatternId>,
        block: BlockId,
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        let delay_ids: Vec<ExprId> = arena.arena.expr_ids(delays).collect();
        for &delay in &delay_ids {
            let ty = self.check_expr_with_schema_arena(
                arena,
                source,
                ArenaExprOrRun::Expr(delay),
                Some(&Type::Duration),
                None,
            );
            let delay_span = arena.arena.expr(delay).span;
            self.expect_type(&Type::Duration, &ty, delay_span);
        }
        if !delay_ids.is_empty() {
            self.record_required_effect(Effect::Time);
            if self.in_pure {
                self.error(
                    span,
                    "retry delays are not allowed in pure functions",
                    DiagnosticCode::CheckPureEffect,
                );
            } else if let Some(effs) = &self.current_effects
                && !Self::effects_covers(effs, &Effect::Time)
            {
                self.error(
                    span,
                    "`retry` with delays requires the `time` effect",
                    DiagnosticCode::CheckEffectViolation,
                );
            }
            self.check_effect_not_excluded(&Effect::Time, span, "`retry` with delays");
        }

        self.push_scope();
        self.begin_error_boundary();
        self.retry_block_depth += 1;
        let expected_ok = match expected {
            Some(Type::Result(ok, _)) => Some(ok.as_ref()),
            _ => None,
        };
        let body_ty = self.check_tail_block_arena(arena, source, block, expected_ok);
        self.retry_block_depth -= 1;
        let error_ty = self.end_error_boundary(None);
        self.pop_scope();

        let result_ty = match body_ty {
            Type::Result(ok, err) => Type::Result(ok, err),
            Type::Invalid | Type::Unknown => Type::Result(Box::new(body_ty), Box::new(Type::Error)),
            ty => Type::Result(Box::new(ty), Box::new(error_ty)),
        };
        if let Some(pattern) = pattern {
            let Type::Result(_, error_ty) = &result_ty else {
                unreachable!()
            };
            self.check_nonbinding_pattern_arena(arena, source, pattern, error_ty);
            self.check_retry_selection_shape(arena, pattern);
        }
        result_ty
    }

    fn check_retry_selection_shape(&mut self, arena: &ArenaProgram, pattern: PatternId) {
        use crate::syntax::arena::ArenaPatternKind;
        let node = arena.arena.pattern(pattern);
        match node.kind {
            ArenaPatternKind::Group(child) => self.check_retry_selection_shape(arena, child),
            ArenaPatternKind::Alternation(children) => {
                for child in arena.arena.pattern_ids(children) {
                    self.check_retry_selection_shape(arena, child);
                }
            }
            ArenaPatternKind::Literal(_)
            | ArenaPatternKind::Record { .. }
            | ArenaPatternKind::List { .. }
            | ArenaPatternKind::Tuple(_)
            | ArenaPatternKind::Constructor { .. } => {
                self.error(
                    arena.arena.span(node.span),
                    "retry selection requires a nominal error, facet, type or wildcard pattern",
                    DiagnosticCode::CheckRetryPattern,
                );
            }
            _ => {}
        }
    }

    /// A run form whose failure the enclosing form propagates is not
    /// captured: the operand of `?`, the receiver of `?.` or `?[`, and the
    /// subject of a `for` that iterates the text a `Result` holds.
    pub(super) fn note_run_propagated(&mut self, arena: &ArenaProgram, operand: ExprId) {
        if let ArenaExprKind::Run(run_id) = arena.arena.expr(operand).kind {
            let run_span = arena.arena.span(arena.arena.run_form(run_id).span);
            self.implicitly_captured_runs.remove(&run_span);
        }
    }

    fn check_run_expr_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        run_id: RunFormId,
    ) -> Type {
        let run_span = arena.arena.span(arena.arena.run_form(run_id).span);
        self.record_required_effect(Effect::Process);
        if self.in_pure {
            self.error(
                run_span,
                "`run` forms are not allowed in pure functions",
                DiagnosticCode::CheckPureRun,
            );
        } else if let Some(effs) = &self.current_effects
            && !Self::effects_covers(effs, &Effect::Process)
        {
            self.error(
                run_span,
                "`run` requires the `process` effect",
                DiagnosticCode::CheckEffectViolation,
            );
        }
        self.check_effect_not_excluded(&Effect::Process, run_span, "`run`");
        let ty = self.check_run_arena(arena, source, run_id);
        let captured = arena.arena.run_form(run_id).captured;
        if captured && !ty.is_result() && !matches!(ty, Type::Unknown | Type::Invalid) {
            self.error(
                run_span,
                &format!("`try` captures a run form that can fail, and this one yields `{ty}`"),
                DiagnosticCode::CheckTryResult,
            );
        }
        // A `Result` that is this form's value is captured, whether or not
        // `try` says so; an operand of `?` is taken back out below.
        if ty.is_result() && !captured {
            self.implicitly_captured_runs.insert(run_span);
        } else {
            self.implicitly_captured_runs.remove(&run_span);
        }
        ty
    }

    fn check_spawn_form_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        form: &ArenaSpawnForm,
    ) -> Type {
        let form_span = arena.arena.span(form.span);
        self.check_process_effect(form_span, "`spawn`");
        match form.target {
            ArenaSpawnTarget::Run(run_id) => {
                let run = arena.arena.run_form(run_id);
                let run_span = arena.arena.span(run.span);
                let segments = arena.arena.run_segments(run.segments);
                for segment in segments {
                    self.check_run_segment_arena(arena, source, segment);
                }
                if segments.len() != 1 {
                    self.error(
                        run_span,
                        "`spawn run` requires exactly one run segment",
                        DiagnosticCode::CheckSpawnRunShape,
                    );
                }
                if let Some(segment) = segments.first()
                    && !matches!(segment.kind, RunKind::Plain | RunKind::Status)
                {
                    let segment_span = arena.arena.span(segment.span);
                    self.error(
                        segment_span,
                        "`spawn run` supports only `run` and `run.status` forms",
                        DiagnosticCode::CheckSpawnRunKind,
                    );
                }
            }
            ArenaSpawnTarget::Command(expr_id) => {
                let ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(expr_id),
                    Some(&Type::Command),
                    None,
                );
                let expr_span = arena.arena.expr(expr_id).span;
                self.expect_type(&Type::Command, &ty, expr_span);
            }
        }
        Type::Result(Box::new(Type::ProcessHandle), Box::new(Type::ProcessError))
    }

    fn check_wait_form_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        form: &ArenaWaitForm,
    ) -> Type {
        let form_span = arena.arena.span(form.span);
        self.check_process_effect(form_span, "`wait`");
        let expected_list = Type::List(Box::new(Type::ProcessHandle));
        let target_kind = arena.arena.expr(form.target).kind;
        let ty = if matches!(target_kind, ArenaExprKind::List(_)) {
            self.check_expr_arena(arena, source, form.target, Some(&expected_list))
        } else {
            self.check_expr_arena(arena, source, form.target, None)
        };
        let target_span = arena.arena.expr(form.target).span;
        match ty {
            Type::ProcessHandle => {
                Type::Result(Box::new(Type::Status), Box::new(Type::ProcessError))
            }
            Type::List(item) => {
                self.expect_type(&Type::ProcessHandle, &item, target_span);
                Type::Result(
                    Box::new(Type::List(Box::new(Type::Status))),
                    Box::new(Type::ProcessError),
                )
            }
            Type::Any => Type::Result(Box::new(Type::Status), Box::new(Type::ProcessError)),
            Type::Unknown | Type::Invalid => {
                Type::Result(Box::new(Type::Status), Box::new(Type::ProcessError))
            }
            _ => {
                self.error(
                    target_span,
                    "`wait` expects ProcessHandle or List[ProcessHandle]",
                    DiagnosticCode::CheckWaitTarget,
                );
                Type::Result(Box::new(Type::Status), Box::new(Type::ProcessError))
            }
        }
    }

    fn check_match_expr_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        value: ExprId,
        arms: ArenaRange,
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        let value_ty = self.check_expr_arena(arena, source, value, None);
        let arm_list = arena.arena.match_expr_arms(arms);
        if arm_list.is_empty() {
            self.error(
                span,
                "match expressions require at least one arm",
                DiagnosticCode::CheckEmptyMatch,
            );
            return expected.cloned().unwrap_or(Type::Unknown);
        }
        let mut inferred = None;
        for arm in arm_list {
            self.push_scope();
            self.check_pattern_arena(arena, source, arm.pattern, &value_ty);
            self.warn_flattened_error_handler_arena(arena, arm.value, arm.pattern, &value_ty);
            if let Some(guard) = arm.guard {
                let guard_ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(guard),
                    Some(&Type::Bool),
                    None,
                );
                let guard_span = arena.arena.expr(guard).span;
                self.expect_type(&Type::Bool, &guard_ty, guard_span);
            }
            let infer_branches = self.inferred_returns.is_some() && expected.is_none();
            let arm_expected = if infer_branches {
                None
            } else {
                expected.or(inferred.as_ref())
            };
            let actual = self.check_expr_arena(arena, source, arm.value, arm_expected);
            if let Some(arm_expected) = arm_expected {
                let value_span = arena.arena.expr(arm.value).span;
                self.expect_type(arm_expected, &actual, value_span);
            }
            if actual != Type::Unknown {
                inferred = Some(if infer_branches {
                    inferred.map_or(actual.clone(), |previous| {
                        self.unify_inferred_returns(
                            previous,
                            actual,
                            arena.arena.expr(arm.value).span,
                        )
                    })
                } else {
                    inferred.unwrap_or(actual)
                });
            }
            self.pop_scope();
        }
        let unguarded = arm_list
            .iter()
            .filter(|arm| arm.guard.is_none())
            .map(|arm| (arm.pattern, arena.arena.span(arm.span)))
            .collect::<Vec<_>>();
        if !super::stmt::patterns_are_exhaustive_arena(
            arena,
            &value_ty,
            unguarded.iter().map(|(pattern, _)| *pattern),
            &self.exhaustiveness_facts(),
        ) && !self.match_scrutinee_definitely_exits_arena(arena, value)
        {
            self.report_value_match_not_exhaustive(arena, &value_ty, &unguarded, span);
        }
        self.check_list_match_coverage_arena(
            arena,
            &value_ty,
            arm_list
                .iter()
                .map(|arm| (arm.pattern, arena.arena.span(arm.span), arm.guard.is_some())),
            span,
        );
        expected.cloned().or(inferred).unwrap_or(Type::Unknown)
    }

    fn check_unary_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        op: UnaryOp,
        inner: ExprId,
    ) -> Type {
        // The operand of `!` in a control position is in one too.
        self.control_condition = op == UnaryOp::Not && self.in_control_position;
        let ty = self.check_expr_arena(arena, source, inner, None);
        let span = arena.arena.expr(inner).span;
        match op {
            UnaryOp::Not => {
                if !matches!(ty, Type::Bool | Type::Status | Type::Unknown) {
                    self.expect_type(&Type::Bool, &ty, span);
                }
                Type::Bool
            }
            UnaryOp::Neg => {
                if ty == Type::Any {
                    self.reject_dynamic_use("a negation operand", None, span);
                    return Type::Any;
                }
                if matches!(ty, Type::Float) {
                    Type::Float
                } else {
                    self.expect_type(&Type::Int, &ty, span);
                    Type::Int
                }
            }
        }
    }

    /// `.` is the implicit parameter of the innermost one-parameter callback.
    fn check_item_arena(&mut self, span: Span) -> Type {
        let message = match self.item_frames.last_mut() {
            Some(super::ItemFrame::Implicit { ty, used, .. }) => {
                *used = true;
                let ty = ty.clone();
                return self
                    .lookup(super::item_binding())
                    .map_or(ty, |binding| binding.ty.clone());
            }
            Some(super::ItemFrame::Named { param, .. }) => match param {
                super::NamedItem::Param(name) => {
                    format!("this block names its parameter; write `{name}` instead of `.`")
                }
                super::NamedItem::Discarded => {
                    "this block discards its parameter with `_`; name it instead of using `.`"
                        .to_string()
                }
                super::NamedItem::Accumulated => {
                    "`fold` and `reduce` blocks take `|acc, item|`; name the item instead of `.`"
                        .to_string()
                }
            },
            None => "`.` is valid only in a one-parameter callback block".to_string(),
        };
        self.error(span, &message, DiagnosticCode::CheckStreamItem);
        Type::Unknown
    }

    fn check_result_fallback_block_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        expression: ExprId,
        block: BlockId,
        result_ty: &Type,
        left_span: Span,
        expected: Option<&Type>,
    ) -> Type {
        let params = arena.arena.block_params(arena.arena.block(block).params);
        if params.len() > 1 {
            self.error(
                arena.arena.expr(expression).span,
                "error fallback block requires exactly one parameter",
                DiagnosticCode::CheckFallbackBlockParams,
            );
        }
        let (value_ty, error_ty) = match result_ty {
            Type::Result(value, error) => (value.as_ref().clone(), error.as_ref().clone()),
            _ => {
                self.error(
                    left_span,
                    "error fallback block requires a Result value",
                    DiagnosticCode::CheckFallbackBlockResult,
                );
                (Type::Unknown, Type::Unknown)
            }
        };
        let value_ty = if value_ty == Type::Unknown {
            expected.cloned().unwrap_or(value_ty)
        } else {
            value_ty
        };
        self.push_scope();
        if let [param] = params
            && param.name != "_"
        {
            if matches!(
                &error_ty,
                Type::Error | Type::ProcessError | Type::ErrorFamily(_) | Type::ErrorVariant { .. }
            ) && let Some(value) = Self::single_error_handler_value_arena(arena, block)
            {
                self.warn_flattened_error_translation_arena(arena, value, param.name);
            }
            self.define(
                param.name,
                super::Binding::new(error_ty.clone(), false),
                arena.arena.span(param.span),
            );
        }
        // A handler without `|name|` receives the error as its item `.`.
        if params.is_empty() {
            self.define(
                super::item_binding(),
                super::Binding::new(error_ty.clone(), false),
                arena.arena.span(arena.arena.block(block).span),
            );
        }
        self.item_frames.push(match params.first() {
            None => super::ItemFrame::Implicit {
                ty: error_ty,
                used: false,
                stage: false,
            },
            Some(param) => super::ItemFrame::Named {
                param: if param.name == "_" {
                    super::NamedItem::Discarded
                } else {
                    super::NamedItem::Param(param.name)
                },
                stage: false,
            },
        });
        // Every handler tail is a value; Unit-success handlers still require Unit.
        let context =
            (!matches!(value_ty, Type::Unit) && !value_ty.is_result_unit()).then_some(&value_ty);
        let actual = self.check_tail_block_contents_arena(arena, source, block, context);
        if let Some(super::ItemFrame::Implicit { used: false, .. }) = self.item_frames.pop() {
            self.error(
                arena.arena.expr(expression).span,
                "error fallback block requires one parameter: name it with `{ |error| ... }` or use `.`",
                DiagnosticCode::CheckFallbackBlockParams,
            );
        }
        if !matches!(actual, Type::Unknown) {
            self.expect_type(&value_ty, &actual, arena.arena.expr(expression).span);
        }
        self.pop_scope();
        let result = if value_ty == Type::Unknown {
            actual.clone()
        } else {
            value_ty
        };
        self.record_expr_type(arena.arena.expr(expression).span, actual);
        result
    }

    #[allow(clippy::too_many_lines)]
    fn check_binary_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        op: BinaryOp,
        left: ExprId,
        right: ExprId,
        expected: Option<&Type>,
    ) -> Type {
        let left_span = arena.arena.expr(left).span;
        let right_span = arena.arena.expr(right).span;
        match op {
            BinaryOp::ResultFallback => {
                let left_ty = self.check_expr_arena(arena, source, left, None);
                if let ArenaExprKind::ValueBlock(block) = arena.arena.expr(right).kind {
                    return self.check_result_fallback_block_arena(
                        arena, source, right, block, &left_ty, left_span, expected,
                    );
                }
                let value_ty = if let Some(ok_ty) = left_ty.result_ok().cloned() {
                    ok_ty
                } else if let Some(inner) = left_ty.optional_inner().cloned() {
                    inner
                } else if self
                    .proof_subject_arena(arena, left)
                    .is_some_and(|(name, path, _)| {
                        self.lookup(name)
                            .and_then(|binding| binding.unrefined_ty.as_ref())
                            .and_then(|ty| super::proof::projected_type(ty, &path))
                            .is_some_and(|ty| matches!(ty, Type::Optional(_)))
                    })
                {
                    self.proven_nonnull_fallback_receivers.insert(left_span);
                    left_ty.clone()
                } else {
                    self.error(
                        left_span,
                        "`??` requires a Result or Optional value",
                        DiagnosticCode::CheckResultFallback,
                    );
                    self.check_expr_arena(arena, source, right, None);
                    return Type::Unknown;
                };
                // The unreachable fallback is checked, but cannot mutate its success continuation.
                let saved_scopes = self
                    .proven_nonnull_fallback_receivers
                    .contains(&left_span)
                    .then(|| self.scopes.clone());
                let right_ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(right),
                    Some(&value_ty),
                    None,
                );
                if let Some(scopes) = saved_scopes {
                    self.scopes = scopes;
                }
                self.expect_type(&value_ty, &right_ty, right_span);
                self.type_constraints
                    .resolve(&value_ty)
                    .unwrap_or(Type::Invalid)
            }
            BinaryOp::Or => {
                // Both operands of `and` and `or` in a control position are
                // in one too.
                let control = self.in_control_position;
                self.control_condition = control;
                let left_ty = self.check_expr_arena(arena, source, left, None);
                let facts = self.infer_condition_narrowings_arena(arena, left);
                self.push_scope();
                self.apply_narrowings(&facts.when_false);
                self.control_condition = control;
                let right_ty = if left_ty.is_result() {
                    self.check_expr_arena(arena, source, right, None)
                } else {
                    self.check_expr_with_schema_arena(
                        arena,
                        source,
                        ArenaExprOrRun::Expr(right),
                        Some(&Type::Bool),
                        None,
                    )
                };
                self.pop_scope();
                if left_ty.is_result() || right_ty.is_result() {
                    self.error(
                        left_span,
                        "`or` is only for Bool values; use `??` for Result fallback",
                        DiagnosticCode::CheckResultFallback,
                    );
                    return Type::Bool;
                }
                self.expect_type(&Type::Bool, &left_ty, left_span);
                self.expect_type(&Type::Bool, &right_ty, right_span);
                Type::Bool
            }
            BinaryOp::And => {
                let control = self.in_control_position;
                self.control_condition = control;
                let left_ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(left),
                    Some(&Type::Bool),
                    None,
                );
                let facts = self.infer_condition_narrowings_arena(arena, left);
                self.push_scope();
                self.apply_narrowings(&facts.when_true);
                self.control_condition = control;
                let right_ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(right),
                    Some(&Type::Bool),
                    None,
                );
                self.pop_scope();
                self.expect_type(&Type::Bool, &left_ty, left_span);
                self.expect_type(&Type::Bool, &right_ty, right_span);
                Type::Bool
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                // A target-typed variant compares against the other operand's
                // type; only checking order changes, never evaluation order.
                let (left_ty, right_ty) = if self.is_inferred_variant_expr(arena, left)
                    && !self.is_inferred_variant_expr(arena, right)
                {
                    let right_ty = self.check_expr_arena(arena, source, right, None);
                    let left_ty = self.check_expr_arena(arena, source, left, Some(&right_ty));
                    (left_ty, right_ty)
                } else if matches!(arena.arena.expr(left).kind, ArenaExprKind::Str(_))
                    && !matches!(arena.arena.expr(right).kind, ArenaExprKind::Str(_))
                {
                    // A literal on the left takes its type from the right
                    // operand, as one on the right does from the left.
                    let right_ty = self.check_expr_arena(arena, source, right, None);
                    let expected = self.path_literal_expectation(arena, left, &right_ty);
                    let left_ty = self.check_expr_arena(arena, source, left, expected.as_ref());
                    (left_ty, right_ty)
                } else {
                    let left_ty = self.check_expr_arena(arena, source, left, None);
                    let literal = self.path_literal_expectation(arena, right, &left_ty);
                    let expected = literal
                        .as_ref()
                        .or_else(|| self.is_inferred_variant_expr(arena, right).then_some(&left_ty));
                    let right_ty = self.check_expr_arena(arena, source, right, expected);
                    (left_ty, right_ty)
                };
                for (operand, other) in [(right, &left_ty), (left, &right_ty)] {
                    self.note_compared_variant_qualifier(arena, operand, other);
                }
                if left_ty != Type::Any
                    && right_ty != Type::Any
                    && !left_ty.matches_expected(&right_ty)
                {
                    self.expect_type(&left_ty, &right_ty, right_span);
                }
                Type::Bool
            }
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                let left_ty = self.check_expr_arena(arena, source, left, None);
                let right_ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(right),
                    Some(&left_ty),
                    None,
                );
                let ordered = |ty: &Type| {
                    matches!(
                        ty,
                        Type::Int | Type::UInt | Type::Float | Type::Duration | Type::Str
                    )
                };
                if left_ty == Type::Any || right_ty == Type::Any {
                    for (ty, other, span) in [
                        (&left_ty, &right_ty, left_span),
                        (&right_ty, &left_ty, right_span),
                    ] {
                        if *ty == Type::Any {
                            self.reject_dynamic_use(
                                "an ordering operand",
                                ordered(other).then_some(other),
                                span,
                            );
                        }
                    }
                    return Type::Bool;
                }
                if self.reject_unnarrowed_union(&left_ty, "an ordering comparison", left_span) {
                    return Type::Bool;
                }
                if !ordered(&left_ty) && !left_ty.is_recovery() {
                    self.error(
                        left_span,
                        "comparison requires Int, Float, Str, or Duration",
                        DiagnosticCode::CheckOperatorType,
                    );
                }
                self.expect_type(&left_ty, &right_ty, right_span);
                Type::Bool
            }
            BinaryOp::In | BinaryOp::NotIn => {
                // A literal member takes its type from the keys or items it
                // is looked up among, so the container is checked first;
                // evaluation order is unchanged. A Path container keeps a
                // literal as text: that membership is text containment.
                let (left_ty, right_ty) =
                    if matches!(arena.arena.expr(left).kind, ArenaExprKind::Str(_)) {
                        let right_ty = self.check_expr_arena(arena, source, right, None);
                        let expected = match &right_ty {
                            Type::Map(member, _) | Type::List(member) => {
                                self.path_literal_expectation(arena, left, member)
                            }
                            _ => None,
                        };
                        let left_ty = self.check_expr_arena(arena, source, left, expected.as_ref());
                        (left_ty, right_ty)
                    } else {
                        let left_ty = self.check_expr_arena(arena, source, left, None);
                        let right_ty = self.check_expr_arena(arena, source, right, None);
                        (left_ty, right_ty)
                    };
                if left_ty == Type::Any && matches!(right_ty, Type::Path | Type::Any) {
                    self.reject_dynamic_use("a membership operand", None, left_span);
                }
                match &right_ty {
                    Type::Map(key, _) => {
                        self.expect_type(key, &left_ty, left_span);
                    }
                    // List membership is equality with each element, which
                    // is defined for every pair of values, as `==` is.
                    Type::List(item) => {
                        if left_ty != Type::Any {
                            self.expect_type(item, &left_ty, left_span);
                        }
                    }
                    Type::Str => {
                        self.expect_type(&Type::Str, &left_ty, left_span);
                    }
                    Type::Bytes => {
                        self.expect_type(&Type::Bytes, &left_ty, left_span);
                    }
                    Type::ErasedRecord | Type::Record(_) => {
                        self.expect_type(&Type::Str, &left_ty, left_span);
                    }
                    Type::Path => {
                        if !matches!(left_ty, Type::Str | Type::Path | Type::Any | Type::Unknown) {
                            self.error(
                                left_span,
                                "Path membership requires Str or Path",
                                DiagnosticCode::CheckMembershipType,
                            );
                        }
                    }
                    // env.PATH entries are exact Path values; a Str literal is
                    // not promoted here, so `"/bin" in env.PATH` is rejected
                    // instead of silently comparing Str against Path.
                    Type::EnvPathList => {
                        if left_ty == Type::Any {
                            self.expect_type(&Type::Path, &left_ty, left_span);
                        } else if !matches!(left_ty, Type::Path | Type::Unknown) {
                            self.error(
                                left_span,
                                "env.PATH membership requires Path; write a path literal such as p\"/opt/bin\"",
                                DiagnosticCode::CheckMembershipType,
                            );
                        }
                    }
                    Type::Any => {
                        self.reject_dynamic_use("a membership container", None, right_span)
                    }
                    Type::Unknown => {}
                    _ => self.error(
                        right_span,
                        "membership requires List, Map, Record, Str, Bytes, Path, or env.PATH",
                        DiagnosticCode::CheckMembershipType,
                    ),
                }
                Type::Bool
            }
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                // The result type is an operand type only for List
                // concatenation; Duration arithmetic mixes Int and Duration, so
                // an expectation for the result must not constrain an operand
                // such as `(if c { 1s } else { 2s }) / 1ms`.
                let left_expected = expected.filter(|ty| matches!(ty, Type::List(_)));
                let left_ty = self.check_expr_arena(arena, source, left, left_expected);
                let right_expected = match &left_ty {
                    Type::List(item) if **item == Type::Unknown => expected,
                    Type::Duration => None,
                    Type::Int if op == BinaryOp::Mul => None,
                    _ => Some(&left_ty),
                };
                let right_ty = self.check_expr_arena(arena, source, right, right_expected);
                if left_ty == Type::Any || right_ty == Type::Any {
                    return self
                        .reject_dynamic_arithmetic(op, &left_ty, &right_ty, left_span, right_span);
                }
                if left_ty == Type::Duration || right_ty == Type::Duration {
                    return match (op, &left_ty, &right_ty) {
                        (BinaryOp::Add | BinaryOp::Sub, Type::Duration, Type::Duration)
                        | (BinaryOp::Mul, Type::Duration, Type::Int)
                        | (BinaryOp::Mul, Type::Int, Type::Duration)
                        | (BinaryOp::Div, Type::Duration, Type::Int) => Type::Duration,
                        (BinaryOp::Div, Type::Duration, Type::Duration) => Type::Int,
                        _ => {
                            self.error(
                                left_span,
                                "invalid Duration arithmetic dimensions",
                                DiagnosticCode::CheckOperatorType,
                            );
                            Type::Unknown
                        }
                    };
                }
                match left_ty {
                    Type::Float if !matches!(op, BinaryOp::Rem) => {
                        self.expect_type(&Type::Float, &right_ty, right_span);
                        Type::Float
                    }
                    Type::Str if matches!(op, BinaryOp::Add) => {
                        self.expect_type(&Type::Str, &right_ty, right_span);
                        Type::Str
                    }
                    Type::List(ref item) if matches!(op, BinaryOp::Add) => {
                        self.expect_type(&left_ty, &right_ty, right_span);
                        Type::List(Box::new(if **item == Type::Unknown {
                            collection_item_ty(&right_ty)
                        } else {
                            item.as_ref().clone()
                        }))
                    }
                    Type::Path
                    | Type::Bool
                    | Type::Bytes
                    | Type::Str
                    | Type::List(_)
                    | Type::Map(_, _)
                    | Type::Record(_)
                    | Type::Optional(_)
                    | Type::Result(_, _)
                    | Type::Status => {
                        let symbol = match op {
                            BinaryOp::Add => "+",
                            BinaryOp::Sub => "-",
                            BinaryOp::Mul => "*",
                            BinaryOp::Div => "/",
                            _ => "%",
                        };
                        let mut diagnostic =
                            Diagnostic::error(format!("`{symbol}` is not defined for {left_ty}"))
                                .with_code(DiagnosticCode::CheckOperatorType)
                                .with_label(Label::primary(
                                    left_span,
                                    format!("this operand is {left_ty}"),
                                ));
                        let note = match left_ty {
                            Type::Path => Some(
                                "operators never join paths; build the path with an `fp\"...\"` literal",
                            ),
                            Type::Result(_, _) => {
                                Some("unwrap the Result with `?` or `??` before using its value")
                            }
                            Type::Optional(_) => Some(
                                "handle `null` with `??` or a null test before using the value",
                            ),
                            _ => None,
                        };
                        if let Some(note) = note {
                            diagnostic = diagnostic.with_note(note);
                        }
                        self.diagnostics.push(diagnostic);
                        Type::Unknown
                    }
                    _ => {
                        self.expect_type(&Type::Int, &left_ty, left_span);
                        self.expect_type(&Type::Int, &right_ty, right_span);
                        Type::Int
                    }
                }
            }
        }
    }

    /// An `Any` arithmetic operand must be validated first. When the other
    /// operand is a concrete type that the operator combines with itself, that
    /// type is the target, and the operation takes it as its result so the
    /// rejection does not cascade.
    fn reject_dynamic_arithmetic(
        &mut self,
        op: BinaryOp,
        left_ty: &Type,
        right_ty: &Type,
        left_span: Span,
        right_span: Span,
    ) -> Type {
        let homogeneous = |ty: &Type| match ty {
            Type::Int | Type::UInt => true,
            Type::Float => op != BinaryOp::Rem,
            Type::Str => op == BinaryOp::Add,
            _ => false,
        };
        let mut result = Type::Any;
        for (ty, other, span) in [
            (left_ty, right_ty, left_span),
            (right_ty, left_ty, right_span),
        ] {
            if *ty != Type::Any {
                continue;
            }
            let target = homogeneous(other).then_some(other);
            if let Some(target) = target {
                result = target.clone();
            }
            self.reject_dynamic_use("an arithmetic operand", target, span);
        }
        result
    }

    fn check_field_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        base: ExprId,
        name: Name,
        span: Span,
        expected: Option<&Type>,
    ) -> Type {
        if let Some(ty) = self.check_env_typed_field_arena(arena, source, base, span) {
            return ty;
        }
        let base_expr = arena.arena.expr(base);
        if matches!(base_expr.kind, ArenaExprKind::Ident(module) if module == "fs" && self.lookup(module).is_none())
            && name == "ls"
        {
            self.removed_compatibility_name(
                Span::new(span.source_id, span.end() - 2, span.end()),
                "ls",
                "children",
                false,
            );
        }
        if matches!(&base_expr.kind, ArenaExprKind::Ident(module) if module == "env")
            && name == "PATH"
        {
            return Type::EnvPathList;
        }
        if let ArenaExprKind::Ident(namespace) = base_expr.kind {
            let qualified = Name::intern(format!("{namespace}.{name}"));
            if let Some(info) = self.tag_variants.get(&qualified).cloned()
                && info.field_count == 0
            {
                self.note_variant_qualifier(
                    span,
                    span,
                    name,
                    &super::InferredVariant::Tag {
                        type_name: info.type_name,
                        variant: name,
                        field_types: Vec::new(),
                    },
                    expected,
                );
                return Type::Tag(info.type_name);
            }
        }
        // A standard module is a namespace of functions, not a record: a member
        // read without a call used to type as Any and failed preparation.
        if let ArenaExprKind::Ident(module) = base_expr.kind
            && self.lookup(module).is_none()
            && api_spec().module(&module.as_str()).is_some()
        {
            let message = if api_spec()
                .module_overloads(&module.as_str(), &name.as_str())
                .is_some()
            {
                format!("standard module function `{module}.{name}` must be called")
            } else {
                format!("standard module `{module}` has no function `{name}`")
            };
            self.diagnostics.push(
                Diagnostic::error(message)
                    .with_code(DiagnosticCode::CheckModuleMember)
                    .with_label(Label::primary(
                        span,
                        "standard module members are functions, not values",
                    )),
            );
            return Type::Unknown;
        }
        let base_ty = self.check_expr_arena(arena, source, base, None);
        if self.reject_unnarrowed_union(&base_ty, &format!("reading `.{name}`"), span) {
            return Type::Unknown;
        }
        match base_ty {
            Type::ErasedRecord | Type::DynamicModule => Type::Any,
            Type::Record(fields) => match fields.get(&name) {
                Some(ty) => ty.clone(),
                None => {
                    self.report_unknown_field(span, name, fields.keys());
                    Type::Unknown
                }
            },
            Type::Module(exports) => match exports.get(&name) {
                Some(export) => export.field_type(),
                None => {
                    self.error(
                        span,
                        "unknown export on known module contract",
                        DiagnosticCode::CheckUnknownField,
                    );
                    Type::Unknown
                }
            },
            receiver if Self::fixed_field_type(&receiver, name).is_some() => {
                self.checked_fixed_field(&receiver, name, span)
            }
            // A dynamic field is checked at runtime and stays dynamic; it
            // used to type as Unknown, which let it reach any concrete use.
            Type::Any => Type::Any,
            Type::Unknown => Type::Unknown,
            _ => {
                self.error(
                    span,
                    "field access requires a record-like value",
                    DiagnosticCode::CheckFieldAccess,
                );
                Type::Unknown
            }
        }
    }

    /// Fields of runtime values whose field set is fixed. `None` when the
    /// receiver is not such a value; `Some(None)` for an unknown field.
    fn fixed_field_type(receiver: &Type, name: Name) -> Option<Option<Type>> {
        let name = name.as_str();
        let name = name.as_str();
        Some(match receiver {
            Type::Status => match name {
                "ok" | "success" => Some(Type::Bool),
                "kind" => Some(Type::Str),
                "segments" => Some(Type::List(Box::new(Type::ErasedRecord))),
                _ => None,
            },
            Type::ProcessHandle => match name {
                "pid" => Some(Type::Int),
                "command" => Some(Type::Str),
                "argv" => Some(Type::List(Box::new(Type::Str))),
                "detached" => Some(Type::Bool),
                _ => None,
            },
            Type::Path => match name {
                "parent" => Some(Type::Path),
                "name" | "ext" => Some(Type::Str),
                _ => None,
            },
            Type::Digest => match name {
                "algorithm" => Some(Type::Str),
                "bytes" => Some(Type::Bytes),
                _ => None,
            },
            Type::Regex => match name {
                "pattern" => Some(Type::Str),
                _ => None,
            },
            Type::Error
            | Type::ErrorFamily(_)
            | Type::ErrorVariant { .. }
            | Type::ErrorFacet(_)
            | Type::ProcessError => match name {
                "message" | "kind" => Some(Type::Str),
                _ => None,
            },
            _ => return None,
        })
    }

    /// Checks a field of a fixed-field value. An unknown field used to type
    /// as Unknown and fail only at runtime.
    fn checked_fixed_field(&mut self, receiver: &Type, name: Name, span: Span) -> Type {
        let is_error = matches!(
            receiver,
            Type::Error | Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ErrorFacet(_)
        );
        if is_error && name == "kind" {
            self.error(
                span,
                "error `.kind` was removed; match exact variants or facets instead",
                DiagnosticCode::CheckErrorRemoved,
            );
            return Type::Str;
        }
        match Self::fixed_field_type(receiver, name).flatten() {
            Some(ty) => ty,
            None => {
                self.diagnostics.push(
                    Diagnostic::error(format!("unknown field `{name}` on {receiver}"))
                        .with_code(DiagnosticCode::CheckUnknownField)
                        .with_label(Label::primary(
                            span,
                            format!("{receiver} has no field `{name}`"),
                        )),
                );
                Type::Unknown
            }
        }
    }

    fn check_null_safe_field_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        base: ExprId,
        name: Name,
        span: Span,
    ) -> Type {
        let base_ty = self.check_expr_arena(arena, source, base, None);
        if self.reject_unnarrowed_union(&base_ty, &format!("reading `.{name}`"), span) {
            return Type::Unknown;
        }
        let (inner, wrap_optional) = match base_ty {
            Type::Optional(inner) => (*inner, true),
            Type::Result(_, _) => {
                self.note_run_propagated(arena, base);
                (self.check_propagation(&base_ty, span), false)
            }
            Type::Any => return Type::Any,
            Type::Unknown => return Type::Unknown,
            _ => {
                self.error(
                    span,
                    "`?.` requires an Optional or Result value",
                    DiagnosticCode::CheckNullSafeField,
                );
                return Type::Unknown;
            }
        };
        let field_ty = match &inner {
            Type::ErasedRecord => Type::Any,
            Type::Record(fields) => match fields.get(&name) {
                Some(ty) => ty.clone(),
                None => {
                    self.report_unknown_field(span, name, fields.keys());
                    Type::Unknown
                }
            },
            receiver if Self::fixed_field_type(receiver, name).is_some() => {
                self.checked_fixed_field(receiver, name, span)
            }
            Type::Module(exports) => match exports.get(&name) {
                Some(export) => export.field_type(),
                None => {
                    self.error(
                        span,
                        "unknown export on known module contract",
                        DiagnosticCode::CheckUnknownField,
                    );
                    Type::Unknown
                }
            },
            Type::Any | Type::DynamicModule => Type::Any,
            Type::Unknown => Type::Unknown,
            _ => {
                self.error(
                    span,
                    "field access requires a record-like value",
                    DiagnosticCode::CheckFieldAccess,
                );
                Type::Unknown
            }
        };
        if wrap_optional && !matches!(field_ty, Type::Optional(_)) {
            Type::Optional(Box::new(field_ty))
        } else {
            field_ty
        }
    }

    fn check_env_typed_field_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        base: ExprId,
        span: Span,
    ) -> Option<Type> {
        let _ = source;
        let base_expr = arena.arena.expr(base);
        let ArenaExprKind::Field {
            base: namespace,
            name,
        } = &base_expr.kind
        else {
            return None;
        };
        let namespace_kind = arena.arena.expr(*namespace).kind;
        if !matches!(&namespace_kind, ArenaExprKind::Ident(module) if module == "env") {
            return None;
        }
        if self.in_pure {
            self.error(
                span,
                "environment lookup is not allowed in pure functions",
                DiagnosticCode::CheckPureEffect,
            );
        }
        Some(match name.as_str().as_str() {
            "Str" => Type::Result(Box::new(Type::Str), Box::new(Type::Error)),
            "Path" => Type::Result(Box::new(Type::Path), Box::new(Type::Error)),
            "PathList" => Type::Result(
                Box::new(Type::List(Box::new(Type::Path))),
                Box::new(Type::Error),
            ),
            _ => {
                self.error(
                    base_expr.span,
                    "unknown env namespace",
                    DiagnosticCode::CheckUnknownEnvNamespace,
                );
                Type::Unknown
            }
        })
    }

    fn checked_postfix_receiver(&mut self, ty: Type, guarded: bool, span: Span) -> (Type, bool) {
        if !guarded {
            return (ty, false);
        }
        match ty {
            Type::Optional(inner) if !matches!(*inner, Type::Any | Type::Unknown) => (*inner, true),
            Type::Result(_, _) => (self.check_propagation(&ty, span), false),
            _ => {
                self.error(
                    span,
                    "guarded indexing requires a checked Optional or Result receiver",
                    DiagnosticCode::CheckNullSafeIndex,
                );
                (Type::Unknown, false)
            }
        }
    }

    fn lift_postfix_type(ty: Type, lift: bool) -> Type {
        if lift && !matches!(ty, Type::Optional(_)) {
            Type::Optional(Box::new(ty))
        } else {
            ty
        }
    }

    fn check_index_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        base: ExprId,
        index: ExprId,
        guarded: bool,
        span: Span,
    ) -> Type {
        let base_ty = self.check_expr_arena(arena, source, base, None);
        let index_span = arena.arena.expr(index).span;
        if self.reject_unnarrowed_union(&base_ty, "indexing", span) {
            return Type::Unknown;
        }
        if guarded && base_ty.is_result() {
            self.note_run_propagated(arena, base);
        }
        let (base_ty, lift) = self.checked_postfix_receiver(base_ty, guarded, span);
        let result = match base_ty {
            Type::Map(key, item) => {
                let index_ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(index),
                    Some(&key),
                    None,
                );
                self.expect_type(&key, &index_ty, index_span);
                *item
            }
            Type::List(item) => {
                let index_ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(index),
                    Some(&Type::Int),
                    None,
                );
                self.expect_type(&Type::Int, &index_ty, index_span);
                // Only an index written as a negative literal counts from the
                // end. The fact is rewritten on every check of this index.
                self.from_end_indexes.remove(&span);
                if let Some(distance) = negative_literal_distance(arena, index) {
                    self.from_end_indexes.insert(span, distance);
                    if let Some(len) = list_literal_len(arena, base)
                        && len < distance as usize
                    {
                        self.error(
                            index_span,
                            &format!(
                                "index -{distance} is out of range for a list of {len} item(s)"
                            ),
                            DiagnosticCode::CheckIndexOutOfRange,
                        );
                    }
                }
                *item
            }
            receiver @ (Type::ErasedRecord | Type::Record(_) | Type::Module(_)) => {
                let index_ty = self.check_expr_with_schema_arena(
                    arena,
                    source,
                    ArenaExprOrRun::Expr(index),
                    Some(&Type::Str),
                    None,
                );
                self.expect_type(&Type::Str, &index_ty, index_span);
                if let Some(projection) = crate::sema::projection::resolve_constant_key_projection(
                    &arena.arena,
                    &self.prepared_constants,
                    base,
                    &receiver,
                    index,
                    crate::sema::projection::ProjectionOperation::Index,
                ) {
                    let ty = projection.value_type.clone();
                    self.projections.insert(span, projection);
                    ty
                } else {
                    Type::Any
                }
            }
            Type::Any => {
                self.check_expr_arena(arena, source, index, None);
                Type::Any
            }
            Type::Unknown => {
                self.check_expr_arena(arena, source, index, None);
                Type::Unknown
            }
            _ => {
                self.error(
                    span,
                    "indexing requires List or Record",
                    DiagnosticCode::CheckIndexType,
                );
                Type::Unknown
            }
        };
        Self::lift_postfix_type(result, lift)
    }

    fn check_slice_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        base: ExprId,
        start: Option<ExprId>,
        end: Option<ExprId>,
        guarded: bool,
        span: Span,
    ) -> Type {
        let base_ty = self.check_expr_arena(arena, source, base, None);
        if self.reject_unnarrowed_union(&base_ty, "slicing", span) {
            return Type::Unknown;
        }
        if guarded && base_ty.is_result() {
            self.note_run_propagated(arena, base);
        }
        let (base_ty, lift) = self.checked_postfix_receiver(base_ty, guarded, span);
        if let Some(start) = start {
            let ty = self.check_expr_with_schema_arena(
                arena,
                source,
                ArenaExprOrRun::Expr(start),
                Some(&Type::Int),
                None,
            );
            let start_span = arena.arena.expr(start).span;
            self.expect_type(&Type::Int, &ty, start_span);
        }
        if let Some(end) = end {
            let ty = self.check_expr_with_schema_arena(
                arena,
                source,
                ArenaExprOrRun::Expr(end),
                Some(&Type::Int),
                None,
            );
            let end_span = arena.arena.expr(end).span;
            self.expect_type(&Type::Int, &ty, end_span);
        }
        let result = match base_ty {
            Type::List(_) => base_ty,
            Type::Str => Type::Str,
            Type::Bytes => Type::Bytes,
            Type::Any => Type::Any,
            Type::Unknown => Type::Unknown,
            _ => {
                self.error(
                    span,
                    "slicing requires List, Str, or Bytes",
                    DiagnosticCode::CheckSliceType,
                );
                Type::Unknown
            }
        };
        Self::lift_postfix_type(result, lift)
    }
}

#[cfg(test)]
mod arena_tests {
    use super::Checker;
    use crate::sema::check::CheckOptions;
    use crate::source::SourceId;
    use crate::syntax::arena::{ArenaExprOrRun, ArenaProgram, ArenaStmtKind};
    use crate::syntax::parser::Parser;

    fn parse(source: &str) -> ArenaProgram {
        Parser::parse_source_arena_only(SourceId::new(0), source).arena
    }

    /// Find the initializer expression of the first top-level `let`/`var`.
    fn first_binding_initializer(program: &ArenaProgram) -> crate::syntax::arena::ExprId {
        for id in program.arena.stmt_ids(program.statements) {
            let stmt = program.arena.stmt(id);
            let initializer = match stmt.kind {
                ArenaStmtKind::Let { initializer, .. }
                | ArenaStmtKind::Const { initializer, .. }
                | ArenaStmtKind::Var { initializer, .. } => initializer,
                _ => continue,
            };
            if let ArenaExprOrRun::Expr(expr_id) = initializer {
                return expr_id;
            }
        }
        panic!("no top-level `let`/`var` with a plain expression initializer");
    }

    /// Smoke-check `check_expr_arena` on the first binding's initializer.
    fn assert_arena_matches_raised(source: &str) {
        let program = parse(source);
        let id = first_binding_initializer(&program);

        program.symbol_owner().with_current(|| {
            let mut native = Checker::new(CheckOptions::default());
            let _ = native.check_expr_arena(&program, source, id, None);
        });
    }

    /// Smoke-check statement sequences through `check_stmt_arena`.
    /// Running the whole sequence (not just one statement) lets earlier
    /// statements build the scope/bindings later ones need.
    fn assert_stmts_arena_match_raised(source: &str) {
        let program = parse(source);
        let stmt_ids: Vec<_> = program.arena.stmt_ids(program.statements).collect();
        assert!(
            !stmt_ids.is_empty(),
            "no top-level statements in: {source:?}"
        );

        program.symbol_owner().with_current(|| {
            let mut native = Checker::new(CheckOptions::default());
            for &id in &stmt_ids {
                native.check_stmt_arena(&program, source, id);
            }
        });
    }

    #[test]
    fn stmt_let_var_assign() {
        assert_stmts_arena_match_raised("let x = 1\nlet y = x + 1");
        assert_stmts_arena_match_raised("var x = 1\nx = 2");
        assert_stmts_arena_match_raised("var x = 1\nx += 2");
        assert_stmts_arena_match_raised("var x = 1.0\nx *= 2.0");
        assert_stmts_arena_match_raised("let {name, version, ..} = {name: \"a\", version: \"b\"}");
        assert_stmts_arena_match_raised("x = 1");
        assert_stmts_arena_match_raised("let x: Int = \"not an int\"");
    }

    #[test]
    fn stmt_return_yield_defer_break_continue_expr() {
        assert_stmts_arena_match_raised("return 1");
        assert_stmts_arena_match_raised("yield 1");
        assert_stmts_arena_match_raised("defer close()");
        assert_stmts_arena_match_raised("break");
        assert_stmts_arena_match_raised("continue");
        assert_stmts_arena_match_raised("1 + 1");
        assert_stmts_arena_match_raised("let x = []\nprint x");
    }

    #[test]
    fn stmt_loop_and_retry_bodies_use_native_block_checking() {
        assert_stmts_arena_match_raised("let x = loop {\n  let y = 1\n  break y\n}");
        assert_stmts_arena_match_raised("let x = loop {\n  1\n}");
        assert_stmts_arena_match_raised("let x = retry [] {\n  let y = 1\n  y\n}");
        assert_stmts_arena_match_raised("let x = retry [1s] {\n  Ok(1)\n}");
    }

    #[test]
    fn stmt_control_flow() {
        assert_stmts_arena_match_raised(
            "let x = 1\nif x > 0 {\n  let y = 1\n} else {\n  let y = 2\n}",
        );
        assert_stmts_arena_match_raised("if 1 > 0 {\n  let y = 1\n}");
        assert_stmts_arena_match_raised(
            "let x: Optional[Int] = null\nif x != null {\n  let y = x\n}",
        );
        assert_stmts_arena_match_raised("var x = 0\nwhile x < 3 {\n  x += 1\n}");
        assert_stmts_arena_match_raised("for i in [1, 2, 3] {\n  let y = i\n}");
        assert_stmts_arena_match_raised("with x = Ok(1) {\n  let y = x\n} else {\n  let z = 1\n}");
        assert_stmts_arena_match_raised("guard let x = Ok(1) else {\n  let z = 1\n}\nlet y = x");
        assert_stmts_arena_match_raised(
            "guard let x = Ok(1) else { |e|\n  let z = e\n}\nlet y = x",
        );
        assert_stmts_arena_match_raised("var x = 1\nx = 2 when x == 1");
        assert_stmts_arena_match_raised("var x = 1\nx = 2 unless x == 1");
    }

    #[test]
    fn stmt_bare_loop_statement() {
        // `ArenaStmtKind::Loop` (a bare `loop { }` statement) is a distinct
        // node from `ArenaExprKind::Loop` (`let x = loop { }`), already
        // covered by stmt_loop_and_retry_bodies_use_native_block_checking.
        assert_stmts_arena_match_raised("loop {\n  break\n}");
        assert_stmts_arena_match_raised("loop {\n  1\n}");
    }

    #[test]
    fn stmt_commands_and_tail_bare_ident() {
        assert_stmts_arena_match_raised("print \"hello\"");
        assert_stmts_arena_match_raised("print ${1 + 2}");
        assert_stmts_arena_match_raised("print bad_ident");
        assert_stmts_arena_match_raised("cd \"/tmp\" {\n  print \"in tmp\"\n}");
        assert_stmts_arena_match_raised("env ({\n  FOO: \"bar\",\n}) {\n  print \"in env\"\n}");
        assert_stmts_arena_match_raised("run false");
        assert_stmts_arena_match_raised("git status");
        assert_stmts_arena_match_raised("some_unresolved_bareword");
        assert_stmts_arena_match_raised("let x = 1\nprint x");
    }

    #[test]
    fn stmt_declarations() {
        assert_stmts_arena_match_raised("type PackageName = Str");
        assert_stmts_arena_match_raised("type Metric = {ratio: Float, samples: List[Float]}");
        assert_stmts_arena_match_raised("type Metric = {}");
        assert_stmts_arena_match_raised("enum Kind { A, B, C }");
        assert_stmts_arena_match_raised(
            "error FsError = NotFound(file: Path) : NotFound | PermissionDenied(file: Path, op: Str) : PermissionDenied",
        );
        assert_stmts_arena_match_raised("use env");
        assert_stmts_arena_match_raised("use env as e");
        assert_stmts_arena_match_raised("use totally_unknown_module_xyz");
        assert_stmts_arena_match_raised("export let x = 1");
        assert_stmts_arena_match_raised("export let {a, b} = {a: 1, b: 2}");
    }

    #[test]
    fn stmt_function_and_signal_hook_declarations() {
        assert_stmts_arena_match_raised("proc greet(name: Str) -> Result[Unit] {\n  print name\n}");
        assert_stmts_arena_match_raised("pure add(a: Int, b: Int = 1) -> Int {\n  a + b\n}");
        assert_stmts_arena_match_raised(
            "stream nums() -> Stream[Int] {\n  for n in [1, 2, 3] {\n    yield n\n  }\n  return\n}",
        );
        assert_stmts_arena_match_raised("pure bad_return() -> Int {\n  \"not an int\"\n}");
        assert_stmts_arena_match_raised(
            "pure missing_return(x: Int) -> Int {\n  if x > 0 {\n    return 1\n  }\n}",
        );
        assert_stmts_arena_match_raised("on SIGINT [] {\n  print \"bye\"\n}");
        assert_stmts_arena_match_raised("on NOT_A_REAL_SIGNAL [] {\n  print \"bye\"\n}");
    }

    #[test]
    fn literals_and_arithmetic() {
        assert_arena_matches_raised("let x = 1 + 2 * 3");
        assert_arena_matches_raised("let x = 1.5 + 2.5");
        assert_arena_matches_raised("let x = \"a\" + \"b\"");
        assert_arena_matches_raised("let x = 1 + \"b\"");
        assert_arena_matches_raised("let x = -1");
        assert_arena_matches_raised("let x = !true");
    }

    #[test]
    fn fmt_string_and_display() {
        assert_arena_matches_raised("let x = \"value: ${1 + 2}\"");
        assert_arena_matches_raised("let x = \"bad: ${[1, 2]}\"");
    }

    #[test]
    fn list_and_record() {
        assert_arena_matches_raised("let x = [1, 2, 3]");
        assert_arena_matches_raised("let x = [1, \"a\"]");
        assert_arena_matches_raised("let x = {a: 1, b: \"two\"}");
        assert_arena_matches_raised("let x = {a: 1, a: 2}");
    }

    #[test]
    fn field_index_slice_env() {
        assert_arena_matches_raised("let x = {a: 1}.a");
        assert_arena_matches_raised("let x = [1, 2, 3][0]");
        assert_arena_matches_raised("let x = [1, 2, 3][0:1]");
        assert_arena_matches_raised("let x = env.PATH");
        assert_arena_matches_raised("let x = env.Str.HOME");
    }

    #[test]
    fn if_expr_and_try_and_require() {
        assert_arena_matches_raised("let x = if true { 1 } else { 2 }");
        assert_arena_matches_raised("let x = (env.Str.HOME)?");
        assert_arena_matches_raised("let x = require({a: 1}, {a: Int})");
    }

    #[test]
    fn unresolved_name_reports_identically() {
        assert_arena_matches_raised("let x = totally_unresolved_name");
    }

    #[test]
    fn builder_and_structured_pipeline_are_native() {
        assert_arena_matches_raised(
            "let x = process.command {\n  cwd = Path(\"src\")\n  run echo ok\n}",
        );
        assert_arena_matches_raised("let x = [1, 2, 3] |> map { . + 1 }");
        assert_arena_matches_raised("let x = \"a\\nb\" |> text.lines() |> count()");
    }

    #[test]
    fn run_spawn_wait_are_native() {
        assert_stmts_arena_match_raised("let x = run true");
        assert_stmts_arena_match_raised("let x = spawn run true");
        assert_stmts_arena_match_raised("let x = spawn run true | run false");
        assert_stmts_arena_match_raised("let x = spawn 1");
        assert_stmts_arena_match_raised("let x = wait 1");
        assert_stmts_arena_match_raised("let h = spawn run true\nlet s = wait h");
        assert_stmts_arena_match_raised("let hs = [spawn run true]\nlet ss = wait hs");
        assert_stmts_arena_match_raised("run false ?");
        assert_stmts_arena_match_raised("pure f() -> Int {\n  run true\n  1\n}");
    }

    #[test]
    fn comprehensions() {
        assert_arena_matches_raised("let x = [i for i in [1, 2, 3]]");
        assert_arena_matches_raised("let x = [i for i in [1, 2, 3] if i > 1]");
        assert_arena_matches_raised("let x = [i + 1 for i in totally_unresolved_iter]");
        assert_arena_matches_raised("let x = {\"k\": n for n in [1, 2, 3]}");
        assert_arena_matches_raised("let x = {[n]: n for n in [1, 2, 3]}");
        assert_arena_matches_raised("let x = [{a} for a in [{a: 1}]]");
    }

    #[test]
    fn match_expr_and_patterns() {
        assert_arena_matches_raised("let x = match 1 { 1 => 2, _ => 3 }");
        assert_arena_matches_raised("let x = match 1 { n => n + 1 }");
        assert_arena_matches_raised("let x = match Ok(1) { Ok(n) => n, Err(e) => 0 }");
        assert_arena_matches_raised("let x = match 1 { n if n > 0 => 1, _ => 2 }");
        assert_arena_matches_raised("let x = match {a: 1} { {a: n} => n, _ => 0 }");
        assert_arena_matches_raised("let x = match 1 { 1 | 2 => 10, _ => 20 }");
        assert_arena_matches_raised("let x = match unresolved_val { SomeVariant(y) => y, _ => 0 }");
    }

    #[test]
    fn call_constructors_and_unresolved_names() {
        assert_arena_matches_raised("let x = Ok(1)");
        assert_arena_matches_raised("let x = Err(\"boom\")");
        assert_arena_matches_raised("let x = Error(kind: \"x\")");
        assert_arena_matches_raised("let x = ProcessError()");
        assert_arena_matches_raised("let x = env(\"HOME\")");
        assert_arena_matches_raised("let x = Path(\"a/b\")");
        assert_arena_matches_raised("let x = range(1, 10)");
        assert_arena_matches_raised("let x = totally_unresolved_call(1, 2)");
    }

    #[test]
    fn call_error_variant_constructor() {
        assert_arena_matches_raised("let x = ProcessError.NotFound(message: \"boom\")");
        assert_arena_matches_raised(
            "let x = ProcessError.NotFound(message: \"boom\", status: null)",
        );
        assert_arena_matches_raised("let x = ProcessError.Unknown(message: \"boom\")");
    }

    #[test]
    fn call_module_apis_and_static_introspection() {
        assert_arena_matches_raised("let x = json.encode({a: 1, b: \"two\"})");
        assert_arena_matches_raised("let x = json.encode({a: 1}, pretty: true)");
        assert_arena_matches_raised("let x = record.require({a: 1}, {a: \"Str\"})");
        assert_arena_matches_raised(
            "let x = cli.parse(args, {root: {kind: \"Path\", default: Path(\"dest\")}, verbose: {kind: \"Bool\", default: false}})",
        );
    }

    #[test]
    fn call_method_dispatch_and_path_constructor_method() {
        // These exercise check_call_arena's method.rs-backed paths
        // (check_registered_method_arena/check_method_dispatch_arena) —
        // Path(...)-constructor methods, plain-value methods across several
        // receiver types (str/list/map get-with-fallback/int/proc-call), and
        // the `?.`-callee method-dispatch path.
        assert_arena_matches_raised("let x = Path.parse_bytes(b\"abc\")");
        assert_arena_matches_raised("let x = \"hello\".upper()");
        assert_arena_matches_raised("let x = [1, 2, 3].len()");
        assert_arena_matches_raised("let x = [1, 2, 3].get(0)");
        assert_arena_matches_raised("let x = [1, 2, 3].get(0, 99)");
        assert_arena_matches_raised("let x = {a: 1}.get(\"a\")");
        assert_arena_matches_raised("let x = maybe_undefined?.foo()");
    }
}

/// The distance from the end that an index written as a negative integer
/// literal names: 1 for `-1`. `-0` is the first item, not a distance.
fn negative_literal_distance(arena: &ArenaProgram, index: ExprId) -> Option<u32> {
    let ArenaExprKind::Unary {
        op: UnaryOp::Neg,
        expr,
    } = arena.arena.expr(index).kind
    else {
        return None;
    };
    let ArenaExprKind::Int(literal) = arena.arena.expr(expr).kind else {
        return None;
    };
    let distance = u32::try_from(arena.arena.int_literal(literal).value()?).ok()?;
    (distance != 0).then_some(distance)
}

/// The number of items of a list literal with no splice, whose length is
/// known as written.
fn list_literal_len(arena: &ArenaProgram, base: ExprId) -> Option<usize> {
    let ArenaExprKind::List(items) = arena.arena.expr(base).kind else {
        return None;
    };
    let mut len = 0;
    for element in arena.arena.list_elements(items) {
        if element.splice_span.is_some() {
            return None;
        }
        len += 1;
    }
    Some(len)
}
