#![allow(clippy::single_call_fn)]

use super::{
    BTreeMap, BinaryOp, Checker, Effect, Name, RunKind, Span, Type, UnaryOp,
    api_spec, block_has_exit_point_arena, collection_item_ty,
};
use crate::syntax::arena::{
    ArenaCompQualifier, ArenaExprKind, ArenaExprOrRun, ArenaFmtPart, ArenaProgram, ArenaRange, ArenaRecordFieldKind,
    ArenaSpawnForm, ArenaSpawnTarget, ArenaWaitForm, BlockId, ExprId, PatternId, RunFormId,
};
use crate::syntax::node::EnvGetKind;

pub(super) fn expr_ty_auto_propagates(ty: &Type) -> bool {
    ty.is_result_unit()
}

/// Arena-native mirror of [`is_path_like_expr`] for expressions that have not
/// been raised to the old AST.
pub(super) fn is_path_like_arena_expr(kind: &ArenaExprKind, ty: &Type) -> bool {
    matches!(ty, Type::Path | Type::Any | Type::Unknown) || matches!(kind, ArenaExprKind::Str(_))
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
        Type::Result(ok, _) | Type::List(ok) | Type::Optional(ok) => capture_success_underconstrained(ok),
        Type::Map(key, value) => capture_success_underconstrained(key) || capture_success_underconstrained(value),
        Type::Record(fields) => fields.values().any(capture_success_underconstrained),
        _ => false,
    }
}

impl Checker {
    /// Retains independently declared schema context for expected slots. Structural
    /// record values alone never identify a schema application or its unused arguments.
    pub(super) fn schema_expectation_for_expr(&self, arena: &ArenaProgram, expression: ExprId) -> Option<super::super::constants::SchemaExpectation> {
        use super::super::constants::{SchemaComponent, SchemaExpectation};
        match arena.arena.expr(expression).kind {
            ArenaExprKind::Ident(name) => self.lookup(name)?.schema_expectation.clone(),
            ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name } => {
                self.schema_expectation_for_expr(arena, base)?.value_context().children.get(&SchemaComponent::Field(name)).cloned()
            }
            ArenaExprKind::Index { base, .. } => {
                let context = self.schema_expectation_for_expr(arena, base)?;
                let ty = self.expr_types.get(&arena.arena.expr(base).span)?;
                let component = match ty { Type::List(_) => SchemaComponent::Item, Type::Map(_, _) => SchemaComponent::Value, _ => return None };
                context.value_context().children.get(&component).cloned()
            }
            ArenaExprKind::Try(inner) => self.schema_expectation_for_expr(arena, inner)?.children.get(&SchemaComponent::Success).cloned(),
            ArenaExprKind::Require { schema, .. } => {
                let context = match schema {
                    Some(schema) => self.record_constructors.annotation_expectation(&arena.arena, schema, self.current_namespace).ok()?,
                    None => self.requirement_targets.get(&arena.arena.expr(expression).span)?.context.clone(),
                };
                let mut result = SchemaExpectation::default();
                result.children.insert(SchemaComponent::Success, context);
                Some(result)
            }
            ArenaExprKind::Call { callee, .. } => match arena.arena.expr(callee).kind {
                ArenaExprKind::Ident(name) => self.procs.get(&name).or_else(|| self.pures.get(&name)).or_else(|| self.streams.get(&name))?.return_schema.clone(),
                ArenaExprKind::Field { base, name } => {
                    let ArenaExprKind::Ident(namespace) = arena.arena.expr(base).kind else { return None; };
                    let qualified = crate::symbol::QualifiedName::new(namespace, name);
                    self.qualified_procs.get(&qualified).or_else(|| self.qualified_pures.get(&qualified)).or_else(|| self.qualified_streams.get(&qualified))?.return_schema.clone()
                }
                _ => None,
            },
            _ => None,
        }
    }

    pub(super) fn lookup_expr_ident(&mut self, name: Name, span: Span) -> Type {
        if name == "_" {
            self.error(span, "`_` is only a whole argument placeholder in an immediate value pipeline call", "check.pipeline-hole");
            return Type::Invalid;
        }
        if let Some(binding) = self.lookup(name) {
            let alias = binding.callable_alias.clone();
            let ty = self.type_constraints.resolve(&binding.ty).unwrap_or(Type::Invalid);
            if let Some(alias) = alias { self.record_callable_alias(span, &alias); }
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
            return Type::ErasedRecord;
        }
        if name == "ARGV" && !self.streams.contains_key(&name) {
            let shadowed_args = self.scopes.iter().skip(1).any(|scope| scope.contains_key(&Name::intern("args")));
            self.removed_compatibility_name(span, "ARGV", "args", !shadowed_args);
            return Type::List(Box::new(Type::Str));
        }
        self.error(span, "unresolved name", "check.unresolved-name");
        Type::Unknown
    }

    fn lookup_record_shorthand(&mut self, name: Name, span: Span) -> Type {
        let ty = self.lookup_expr_ident(name, span);
        if name == "ARGV"
            && let Some(diagnostic) = self.diagnostics.last_mut()
            && diagnostic.code.as_deref() == Some("check.compatibility-vocabulary")
        {
            for hint in &mut diagnostic.fix_hints {
                if hint.span == Some(span) && hint.replacement.as_deref() == Some("args") {
                    hint.replacement = Some("ARGV: args".to_string());
                }
            }
        }
        ty
    }

    pub(super) fn check_env_get(&mut self, kind: EnvGetKind, span: Span) -> Type {
        self.require_effect(Effect::Env, span, "environment lookup");
        if self.in_pure {
            self.error(
                span,
                "environment lookup is not allowed in pure functions",
                "check.pure-effect",
            );
        }
        match kind {
            EnvGetKind::Str => Type::Result(Box::new(Type::Str), Box::new(Type::Error)),
            EnvGetKind::Path => Type::Result(Box::new(Type::Path), Box::new(Type::Error)),
            EnvGetKind::PathList => Type::Result(
                Box::new(Type::List(Box::new(Type::Path))),
                Box::new(Type::Error),
            ),
        }
    }

    pub(super) fn check_process_effect(&mut self, span: Span, form: &str) {
        self.record_required_effect(Effect::Process);
        if self.in_pure {
            self.error(
                span,
                &format!("{form} forms are not allowed in pure functions"),
                "check.pure-run",
            );
        } else if let Some(effs) = &self.current_effects
            && !Self::effects_covers(effs, &Effect::Process)
        {
            self.error(
                span,
                &format!("{form} requires the `process` effect"),
                "check.effect-violation",
            );
        }
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
        &mut self, arena: &ArenaProgram, source: &str, value: ArenaExprOrRun,
        expected: Option<&Type>, schema: Option<crate::sema::constants::SchemaExpectation>,
    ) -> Type {
        let previous = std::mem::replace(&mut self.expected_schema, schema);
        let actual = self.check_expr_or_run_arena(arena, source, value, expected);
        self.expected_schema = previous;
        actual
    }

    pub(super) fn check_expr_arena(
        &mut self, arena: &ArenaProgram, source: &str, id: ExprId, expected: Option<&Type>,
    ) -> Type {
        let previous = self.expected_schema.clone();
        if expected.is_none() { self.expected_schema = None; }
        let resolved = expected.and_then(|ty| self.type_constraints.resolve(ty).ok());
        let actual = self.check_expr_arena_inner(arena, source, id, resolved.as_ref().or(expected));
        self.expected_schema = previous;
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
            let actual = if let Some((expected, schema)) = self.argument_projection_contexts.remove(&record) {
                self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(record), Some(&expected), Some(schema))
            } else { self.check_expr_arena(arena, source, record, None) };
            if let Type::Record(fields) = actual {
                for (projection, ty) in &mut self.argument_projection_types {
                    if let ArenaExprKind::Field { base, name } = arena.arena.expr(*projection).kind
                        && base == record && let Some(actual) = fields.get(&name) { *ty = actual.clone(); }
                }
            }
        }
        if let Some(ty) = self.argument_projection_types.get(&id) { return ty.clone(); }
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
            ArenaExprKind::Int(_) => Type::Int,
            ArenaExprKind::Float(_) => Type::Float,
            ArenaExprKind::Duration(_) => Type::Duration,
            ArenaExprKind::Str(_) => Type::Str,
            ArenaExprKind::Regex(_) => Type::Regex,
            ArenaExprKind::PathStr(_) => Type::Path,
            ArenaExprKind::GlobStr(_) => {
                if self.in_pure {
                    self.error(
                        expr.span,
                        "glob expansion is not allowed in pure functions",
                        "check.pure-effect",
                    );
                }
                Type::List(Box::new(Type::Path))
            }
            ArenaExprKind::FmtString(parts) => self.check_fmt_string_arena(arena, source, *parts),
            ArenaExprKind::PathFmtString(parts) => {
                self.check_fmt_string_arena(arena, source, *parts);
                Type::Path
            }
            ArenaExprKind::Bytes(_) => Type::Bytes,
            ArenaExprKind::Ident(name) => {
                if *name == "_" {
                    self.pipeline_hole_types.get(&expr.span).cloned().unwrap_or_else(|| {
                        self.error(expr.span, "`_` is only a whole argument placeholder in an immediate value pipeline call", "check.pipeline-hole");
                        Type::Invalid
                    })
                } else {
                    // Dollar command identifiers include their sigil in the
                    // expression span. The removed binding edit replaces only
                    // its four-byte name, preserving command interpolation.
                    let name_span = if *name == "ARGV" && expr.span.end() - expr.span.start() == "ARGV".len() + 1 {
                        Span::new(expr.span.source_id, expr.span.start() + 1, expr.span.end())
                    } else { expr.span };
                    self.lookup_expr_ident(*name, name_span)
                }
            }
            ArenaExprKind::ValuePipelineCall { input, call, hole } => {
                let input_ty = self.check_expr_arena(arena, source, *input, None);
                let hole_span = arena.arena.expr(*hole).span;
                let previous = self.pipeline_hole_types.insert(hole_span, input_ty);
                let ty = self.check_expr_arena(arena, source, *call, expected);
                match previous { Some(previous) => { self.pipeline_hole_types.insert(hole_span, previous); }, None => { self.pipeline_hole_types.remove(&hole_span); } }
                ty
            }
            ArenaExprKind::Item => self.stream_item_types.last().cloned().unwrap_or_else(|| {
                self.error(
                    expr.span,
                    "`.` is valid only in stream stage blocks",
                    "check.stream-item",
                );
                Type::Unknown
            }),
            ArenaExprKind::LastStatus => {
                if !self.last_status_available {
                    self.error(expr.span, "`$?` is not set", "check.last-status");
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
                let ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(*message), Some(&Type::Str), None);
                self.expect_type(&Type::Str, &ty, arena.arena.expr(*message).span);
                self.push_scope();
                let ty = self.check_tail_block_arena(arena, source, *block, expected);
                self.pop_scope();
                ty
            }
            ArenaExprKind::ContextScope { kind, input, block, value_body } => {
                use crate::syntax::arena::ContextScopeKind;
                let tail_value = std::mem::replace(&mut self.context_scope_tail_value, false);
                if self.in_pure { self.error(expr.span, "context scopes are not allowed in pure functions", "check.pure-effect"); }
                self.require_effect(crate::syntax::node::Effect::Env, expr.span, "context scopes");
                let input_type = self.check_expr_arena(arena, source, *input, None);
                match kind {
                    ContextScopeKind::Cwd => if !matches!(input_type, Type::Path | Type::Str | Type::Unknown | Type::Invalid) {
                        self.error(arena.arena.expr(*input).span, "cwd scope requires Path or Str", "check.context-scope-input");
                    },
                    ContextScopeKind::Env => {
                        let values = match &input_type {
                            Type::Record(fields) => Some(fields.values().collect::<Vec<_>>()),
                            Type::Map(key, value) if **key == Type::Str => Some(vec![value.as_ref()]),
                            Type::Unknown | Type::Invalid => None,
                            _ => { self.error(arena.arena.expr(*input).span, "environment overlay requires a Record or string-keyed Map", "check.context-scope-input"); None },
                        };
                        if let Some(values) = values && values.into_iter().any(|ty| !ty.can_be_argv_item()) {
                            self.error(arena.arena.expr(*input).span, "environment values must convert to one scalar argv item", "check.env-value");
                        }
                    }
                }
                self.context_scope_depths.push(self.scopes.len());
                self.push_scope();
                let body_type = if *value_body || tail_value || matches!(expected, Some(Type::Result(ok, _)) if **ok != Type::Unit) {
                    let expected = match expected { Some(Type::Result(ok, _)) => Some(ok.as_ref()), _ => None };
                    self.check_tail_block_arena(arena, source, *block, expected)
                } else { self.check_block_arena(arena, source, *block); Type::Unit };
                self.pop_scope();
                self.context_scope_depths.pop();
                if !body_type.can_escape_context_scope() {
                    self.error(expr.span, "a live producer or host handle cannot escape a restored context", "check.context-scope-escape");
                }
                self.context_scope_tail_value = tail_value;
                Type::Result(Box::new(body_type), Box::new(Type::Error))
            }
            ArenaExprKind::ValueBlock(block) => {
                if let Some(param) = arena.arena.block_params(arena.arena.block(*block).params).first() {
                    self.error(arena.arena.span(param.span), "parameter value blocks require a Result fallback", "check.fallback-block-context");
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
                    let ArenaExprKind::Binary { left, right, .. } = arena.arena.expr(pair).kind else { unreachable!() };
                    let left_ty = previous.take().unwrap_or_else(|| self.check_expr_arena(arena, source, left, None));
                    let right_ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(right), Some(&left_ty), None);
                    if !matches!(left_ty, Type::Int | Type::UInt | Type::Float | Type::Duration | Type::Str | Type::Any | Type::Unknown) {
                        self.error(arena.arena.expr(left).span, "comparison requires Int, Float, Str, or Duration", "check.operator-type");
                    }
                    self.expect_type(&left_ty, &right_ty, arena.arena.expr(right).span);
                    previous = Some(right_ty);
                }
                Type::Bool
            }
            ArenaExprKind::Binary { op, left, right } => {
                self.check_binary_arena(arena, source, *op, *left, *right, expected)
            }
            ArenaExprKind::Field { base, name } => {
                self.check_field_arena(arena, source, *base, *name, expr.span)
            }
            ArenaExprKind::NullSafeField { base, name } => {
                self.check_null_safe_field_arena(arena, source, *base, *name, expr.span)
            }
            ArenaExprKind::Index { base, index, guarded } => {
                self.check_index_arena(arena, source, *base, *index, *guarded, expr.span)
            }
            ArenaExprKind::Slice { base, start, end, guarded } => {
                self.check_slice_arena(arena, source, *base, *start, *end, *guarded, expr.span)
            }
            ArenaExprKind::EnvGet { kind, .. } => self.check_env_get(*kind, expr.span),
            ArenaExprKind::EnvPathList => { self.require_effect(Effect::Env, expr.span, "environment path lookup"); Type::EnvPathList },
            ArenaExprKind::Pipeline { .. } => {
                self.error(
                    expr.span,
                    "pipeline sugar was not desugared",
                    "check.desugar",
                );
                Type::Unknown
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                self.check_structured_pipeline_arena(arena, source, *input, *stages)
            }
            ArenaExprKind::Try(inner) => {
                let inner_expected = expected.cloned().map(|ty| Type::Result(Box::new(ty), Box::new(Type::Error)));
                let schema = self.expected_schema.clone().map(|schema| {
                    let mut wrapped = crate::sema::constants::SchemaExpectation::default();
                    wrapped.children.insert(crate::sema::constants::SchemaComponent::Success, schema);
                    wrapped
                });
                let ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(*inner), inner_expected.as_ref(), schema);
                self.check_propagation(&ty, expr.span)
            }
            ArenaExprKind::Require { value, schema } => {
                self.requirement_targets.remove(&expr.span);
                self.requirement_expected_targets.remove(&expr.span);
                let inferred = super::expected::infer_requirement_target(&arena.arena, expected, self.expected_schema.as_ref(), &self.type_constraints);
                if let Some(inferred) = &inferred { self.requirement_expected_targets.insert(expr.span, inferred.clone()); }
                self.check_expr_arena(arena, source, *value, None);
                let target = if let Some(schema) = schema {
                    let ty = self.type_from_arena(arena, *schema);
                    let context = self.record_constructors.annotation_expectation(&arena.arena, *schema, self.current_namespace).unwrap_or_default();
                    Some(super::expected::requirement_target(&arena.arena, ty, context))
                } else { inferred };
                if let Some(target) = target {
                    let ty = target.ty.clone();
                    self.requirement_targets.insert(expr.span, target);
                    Type::Result(Box::new(ty), Box::new(Type::Error))
                } else {
                    self.error(expr.span, "cannot infer require target; supply a schema or an independently typed boundary", "check.require-target");
                    Type::Invalid
                }
            }
            ArenaExprKind::Call { callee, args } => {
                let may_mutate = matches!(arena.arena.expr(*callee).kind, ArenaExprKind::Ident(name) if self.lookup(name).is_some_and(|binding| binding.ty == Type::Proc));
                let diagnostics_before = self.diagnostics.len();
                let result = self.check_call_arena(arena, source, *callee, *args, expr.span, expected);
                if self.diagnostics.len() == diagnostics_before && self.stage_callable_is_static(arena, *callee) { self.statically_resolved_call_spans.insert(expr.span); }
                let erased_proc_call = matches!(arena.arena.expr(*callee).kind, ArenaExprKind::Field { base, name } if name == "call"
                    && self.expr_types.get(&arena.arena.expr(base).span) == Some(&Type::Proc));
                if may_mutate || erased_proc_call { self.invalidate_mutable_narrowings(); }
                result
            }
            ArenaExprKind::PatternCondition { value, arms } => {
                let value_ty = self.check_expr_arena(arena, source, *value, None);
                let pattern = arena.arena.match_expr_arms(*arms)[0].pattern;
                if super::stmt::patterns_are_exhaustive_arena(arena, &value_ty, std::iter::once(pattern), &self.type_defs, &self.tag_variants) {
                    self.error(expr.span, "pattern condition cannot fail; bind the subject with `let` instead", "check.irrefutable-pattern-condition");
                }
                self.push_scope();
                self.check_pattern_arena(arena, source, pattern, &value_ty);
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
            ArenaExprKind::ListComp { expr: body, qualifiers } => self.check_list_comp_arena(arena, source, *body, *qualifiers, expected, expr.span),
            ArenaExprKind::MapComp { key, value, qualifiers } => self.check_map_comp_arena(arena, source, *key, *value, *qualifiers, expected, expr.span),
            ArenaExprKind::Loop { block } => {
                self.check_loop_arena(arena, source, *block, expr.span)
            }
            ArenaExprKind::Capture(block) => self.check_capture_arena(arena, source, *block, expected, expr.span),
            ArenaExprKind::Retry { delays, pattern, block } => {
                self.check_retry_arena(arena, source, *delays, *pattern, *block, expr.span)
            }
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
    ) -> Type {
        for part in arena.arena.fmt_parts(range) {
            if let ArenaFmtPart::Expr(expr_id, _) = part {
                let ty = self.check_expr_arena(arena, source, expr_id, None);
                if !ty.can_display() && !matches!(ty, Type::Any | Type::Unknown) {
                    let span = arena.arena.expr(expr_id).span;
                    self.error(
                        span,
                        "value cannot be displayed in fmt string",
                        "check.display-conversion",
                    );
                }
            }
        }
        Type::Str
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
        let expected_item = match expected { Some(Type::List(item)) => Some(item.as_ref()), _ => None };
        let mut inferred = expected_item.cloned().unwrap_or(Type::Unknown);
        for item in arena.arena.list_elements(range) {
            let item_expected = expected_item;
            let span = item.splice_span.map(|span| arena.arena.span(span)).unwrap_or(arena.arena.expr(item.value).span);
            let actual = if item.splice_span.is_some() {
                let list_expected = item_expected.cloned().map(|ty| Type::List(Box::new(ty)));
                let actual = self.check_expr_arena(arena, source, item.value, list_expected.as_ref());
                match actual {
                    Type::List(ty) => *ty,
                    Type::Unknown => Type::Unknown,
                    _ => {
                        self.error(span, "list literal splice requires List; handle Results explicitly and collect Streams explicitly", "check.list-splice-type");
                        Type::Unknown
                    }
                }
            } else {
                let schema = self.expected_schema.as_ref().and_then(|schema| schema.value_context().children.get(&crate::sema::constants::SchemaComponent::Item)).cloned();
                self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(item.value), item_expected, schema)
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
        &mut self, arena: &ArenaProgram, source: &str, expression: ExprId,
        expected: Option<&Type>, component: crate::sema::constants::SchemaComponent,
    ) -> Type {
        let schema = self.expected_schema.as_ref().and_then(|schema| schema.value_context().children.get(&component)).cloned();
        self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(expression), expected, schema)
    }

    fn check_map_literal_arena(&mut self, arena: &ArenaProgram, source: &str, range: ArenaRange, expected: Option<&Type>) -> Type {
        let (expected_key, expected_item) = match expected { Some(Type::Map(key, item)) => (Some(key.as_ref()), Some(item.as_ref())), _ => (None, None) };
        let mut inferred_key = expected_key.cloned().unwrap_or(Type::Unknown);
        let mut inferred = expected_item.cloned().unwrap_or(Type::Unknown);
        for field in arena.arena.record_fields(range) {
            let (key_ty, actual, span) = match field.kind {
                ArenaRecordFieldKind::Computed { key, value, span } => {
                    let key_ty = self.check_schema_child_expr_arena(arena, source, key, expected_key, crate::sema::constants::SchemaComponent::Key);
                    if !key_ty.is_map_key() && !key_ty.is_recovery() {
                        self.error(arena.arena.expr(key).span, "Map keys require Str, Int, UInt, Bool, Bytes, Path, or Duration", "check.map-key-type");
                    }
                    (key_ty, self.check_schema_child_expr_arena(arena, source, value, expected_item, crate::sema::constants::SchemaComponent::Value), arena.arena.span(span))
                }
                ArenaRecordFieldKind::Path { value, span, .. } => {
                    self.check_schema_child_expr_arena(arena, source, value, expected_item, crate::sema::constants::SchemaComponent::Value);
                    self.error(arena.arena.span(span), "map literals do not permit static record update paths", "check.map-update-path");
                    continue;
                }
                ArenaRecordFieldKind::Named { value, span, .. } => (Type::Str, self.check_schema_child_expr_arena(arena, source, value, expected_item, crate::sema::constants::SchemaComponent::Value), arena.arena.span(span)),
                ArenaRecordFieldKind::Shorthand { name, span } => (Type::Str, self.lookup_record_shorthand(name, arena.arena.span(span)), arena.arena.span(span)),
                ArenaRecordFieldKind::Spread { expr, span } => {
                    let ty = self.check_expr_arena(arena, source, expr, expected);
                    match ty { Type::Map(key, item) => (*key, *item, arena.arena.span(span)), Type::Unknown => (Type::Unknown, Type::Unknown, arena.arena.span(span)), _ => {
                        self.error(arena.arena.span(span), "map literal spreads require Map", "check.map-spread-type");
                        (Type::Unknown, Type::Unknown, arena.arena.span(span))
                    }}
                }
            };
            if inferred_key == Type::Unknown { inferred_key = key_ty; }
            else { self.expect_type(&inferred_key, &key_ty, span); }
            if let Some(expected_item) = expected_item { self.expect_type(expected_item, &actual, span); }
            else if inferred == Type::Unknown { inferred = actual; }
            else if let Some(merged) = merge_list_literal_item_ty(&inferred, &actual) { inferred = merged; }
            else { self.expect_type(&inferred, &actual, span); }
        }
        if inferred_key == Type::Unknown { inferred_key = Type::Str; }
        Type::Map(Box::new(inferred_key), Box::new(inferred))
    }

    fn check_record_update_arena(&mut self, arena: &ArenaProgram, source: &str, range: ArenaRange, span: Span) -> Type {
        fn requires_validation(actual: &Type, expected: &Type) -> bool {
            if actual.any_flows_to_concrete(expected) { return true; }
            match (actual, expected) {
                (Type::Record(actual), Type::Record(expected)) if !expected.is_empty() => actual.is_empty() || expected.iter().any(|(name, expected)| actual.get(name).is_some_and(|actual| requires_validation(actual, expected))),
                (Type::List(actual), Type::List(expected)) | (Type::Optional(actual), Type::Optional(expected)) => requires_validation(actual, expected),
                (Type::Map(actual_key, actual_value), Type::Map(expected_key, expected_value)) => requires_validation(actual_key, expected_key) || requires_validation(actual_value, expected_value),
                (Type::Result(actual, error), Type::Result(expected, expected_error)) => requires_validation(actual, expected) || requires_validation(error, expected_error),
                _ => false,
            }
        }
        let fields = arena.arena.record_fields(range);
        let base_ty = match fields.first().map(|field| &field.kind) {
            Some(ArenaRecordFieldKind::Spread { expr, .. }) => self.check_expr_arena(arena, source, *expr, None),
            _ => {
                self.error(span, "nested record updates require one leading record spread", "check.record-update-base");
                Type::Unknown
            }
        };
        if !matches!(&base_ty, Type::Record(fields) if !fields.is_empty()) {
            self.error(span, "nested record updates require a statically known record shape", "check.record-update-shape");
        }
        let mut targets: Vec<Vec<Name>> = Vec::new();
        for (index, field) in fields.iter().enumerate() {
            let (path, value, field_span) = match &field.kind {
                ArenaRecordFieldKind::Spread { expr, span } => {
                    if index != 0 {
                        self.error(arena.arena.span(*span), "nested record updates permit only the leading spread", "check.record-update-base");
                        self.check_expr_arena(arena, source, *expr, None);
                    }
                    continue;
                }
                ArenaRecordFieldKind::Computed { key, value, span } => {
                    self.check_expr_arena(arena, source, *key, Some(&Type::Str));
                    self.check_expr_arena(arena, source, *value, None);
                    self.error(arena.arena.span(*span), "nested record updates do not permit computed map keys", "check.map-update-path");
                    continue;
                }
                ArenaRecordFieldKind::Path { path, value, span } => (arena.arena.names(*path).collect::<Vec<_>>(), Some(*value), arena.arena.span(*span)),
                ArenaRecordFieldKind::Named { name, value, span } => (vec![*name], Some(*value), arena.arena.span(*span)),
                ArenaRecordFieldKind::Shorthand { name, span } => (vec![*name], None, arena.arena.span(*span)),
            };
            if targets.iter().any(|prior| prior.starts_with(&path) || path.starts_with(prior)) {
                self.error(field_span, "record update targets must be disjoint", "check.record-update-overlap");
            }
            targets.push(path.clone());
            let mut selected = Some(&base_ty);
            for name in &path {
                selected = match selected {
                    Some(Type::Record(fields)) if !fields.is_empty() => fields.get(name),
                    _ => None,
                };
                if selected.is_none() { break; }
            }
            if selected.is_none() {
                self.error(field_span, "every update target must select an existing field through known records", "check.record-update-field");
            }
            let actual = match value {
                Some(value) => self.check_expr_arena(arena, source, value, selected),
                None => self.lookup_record_shorthand(path[0], field_span),
            };
            if let Some(selected) = selected {
                if requires_validation(&actual, selected) {
                    self.error(field_span, "record update replacements require a checked field type", "check.record-update-value");
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
        if fields.iter().any(|field| matches!(field.kind, ArenaRecordFieldKind::Path { .. })) {
            return self.check_record_update_arena(arena, source, range, span);
        }
        if matches!(expected, Some(Type::Map(_, _))) || fields.iter().any(|field| matches!(field.kind, ArenaRecordFieldKind::Computed { .. })) {
            return self.check_map_literal_arena(arena, source, range, expected);
        }
        if matches!(
            expected,
            Some(Type::Status | Type::ProcessHandle | Type::NetJob | Type::FsRoot)
        ) {
            for field in fields {
                match &field.kind {
                ArenaRecordFieldKind::Computed { .. } => unreachable!("computed fields select Map checking"),
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
                    "check.type-mismatch",
                );
                return Type::Unknown;
            }
            if matches!(expected, Some(Type::FsRoot)) {
                self.error(span, "`FsRoot` is an opaque runtime capability and cannot be constructed with a record literal; obtain it from a filesystem root factory", "check.type-mismatch");
                return Type::Unknown;
            }
            if matches!(expected, Some(Type::NetJob)) {
                self.error(
                    span,
                    "`NetJob` is a runtime-only type and cannot be constructed with a record literal; obtain it from `net.start`",
                    "check.type-mismatch",
                );
                return Type::Unknown;
            }
            self.error(
                span,
                "`Status` is a runtime-only type and cannot be constructed with a record literal; obtain it from `process.run`, `run.status`, or similar",
                "check.type-mismatch",
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
                ArenaRecordFieldKind::Computed { .. } => unreachable!("computed fields select Map checking"),
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
                                "check.spread-not-record",
                            );
                        }
                    }
                }
                ArenaRecordFieldKind::Path { .. } => unreachable!("record updates use their own checker"),
                ArenaRecordFieldKind::Named { name, value, span } => {
                    let field_span = arena.arena.span(*span);
                    last_span = field_span;
                    if record.contains_key(name) && !has_spread {
                        self.error(
                            field_span,
                            "duplicate record field",
                            "check.duplicate-record-field",
                        );
                    }
                    if let Some(expected_fields) = expected_fields
                        && !expected_fields.contains_key(name)
                    {
                        self.error(field_span, "unknown schema field", "check.schema-field");
                    }
                    let field_expected = expected_fields.and_then(|fields| fields.get(name));
                    let schema = self.expected_schema.as_ref().and_then(|schema| schema.value_context().children.get(&crate::sema::constants::SchemaComponent::Field(*name))).cloned();
                    let ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(*value), field_expected, schema);
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
                            "check.duplicate-record-field",
                        );
                    }
                    if let Some(expected_fields) = expected_fields
                        && !expected_fields.contains_key(name)
                    {
                        self.error(field_span, "unknown schema field", "check.schema-field");
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
                    self.error(last_span, "missing schema field", "check.schema-field");
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
            let narrowings = self.check_condition_arena(arena, source, branch.condition, "check.if-condition");
            self.push_scope();
            self.apply_narrowings(&narrowings.when_true);
            self.bind_pattern_condition_arena(arena, source, branch.condition);
            let infer_branches = self.inferred_returns.is_some() && expected.is_none();
            let branch_expected = if infer_branches { None } else { expected.or(inferred.as_ref()) };
            let actual = self.check_expr_arena(arena, source, branch.value, branch_expected);
            if let Some(branch_expected) = branch_expected {
                let value_span = arena.arena.expr(branch.value).span;
                self.expect_type(branch_expected, &actual, value_span);
            }
            if actual != Type::Unknown {
                inferred = Some(if infer_branches {
                    inferred.map_or(actual.clone(), |previous| self.unify_inferred_returns(previous, actual, arena.arena.expr(branch.value).span))
                } else { inferred.unwrap_or(actual) });
            }
            self.pop_scope();
        }
        self.push_scope();
        if arena.arena.if_expr_branches(branches).len() == 1 {
            let narrowings = self.infer_condition_narrowings_arena(arena, arena.arena.if_expr_branches(branches)[0].condition);
            self.apply_narrowings(&narrowings.when_false);
        }
        let infer_branches = self.inferred_returns.is_some() && expected.is_none();
        let else_expected = if infer_branches { None } else { expected.or(inferred.as_ref()) };
        let else_ty = self.check_expr_arena(arena, source, else_value, else_expected);
        if let Some(else_expected) = else_expected {
            let else_span = arena.arena.expr(else_value).span;
            self.expect_type(else_expected, &else_ty, else_span);
        }
        self.pop_scope();
        if infer_branches {
            inferred.map_or(else_ty.clone(), |previous| self.unify_inferred_returns(previous, else_ty, arena.arena.expr(else_value).span))
        } else { expected.cloned().or(inferred).unwrap_or(else_ty) }
    }

    fn check_comp_qualifiers_arena(&mut self, arena: &ArenaProgram, source: &str, qualifiers: ArenaRange, map: bool) -> usize {
        let mut scopes = 0;
        for qualifier in arena.arena.comp_qualifiers(qualifiers) {
            match *qualifier {
                ArenaCompQualifier::For { target, iter, span } => {
                    let iter_ty = self.check_expr_arena(arena, source, iter, None);
                    if matches!(&iter_ty, Type::Result(ok, _) if matches!(ok.as_ref(), Type::Map(_, _) | Type::Str | Type::Bytes)) {
                        self.check_propagation(&iter_ty, arena.arena.expr(iter).span);
                    }
                    let item_ty = iter_ty.iteration_item_type().unwrap_or_else(|| {
                        if matches!(iter_ty, Type::Any | Type::Unknown) { Type::Any } else {
                            self.error(arena.arena.expr(iter).span, "comprehension iterates over List, Stream, Map, Str, or Bytes values", if map { "check.mapcomp-iterator" } else { "check.listcomp-iterator" });
                            Type::Unknown
                        }
                    });
                    self.push_scope();
                    scopes += 1;
                    self.define_binding_target_arena(arena, target, &item_ty, false, span);
                }
                ArenaCompQualifier::If { condition, .. } => {
                    let ty = self.check_expr_arena(arena, source, condition, None);
                    if !matches!(ty, Type::Bool | Type::Status | Type::Any | Type::Unknown) {
                        self.error(arena.arena.expr(condition).span, "comprehension condition must be Bool or Status", if map { "check.mapcomp-condition" } else { "check.listcomp-condition" });
                    }
                }
            }
        }
        scopes
    }

    fn check_list_comp_arena(&mut self, arena: &ArenaProgram, source: &str, body: ExprId, qualifiers: ArenaRange, expected: Option<&Type>, _span: Span) -> Type {
        let scopes = self.check_comp_qualifiers_arena(arena, source, qualifiers, false);
        let expected_item = match expected { Some(Type::List(item)) => Some(item.as_ref()), _ => None };
        let elem_ty = self.check_expr_arena(arena, source, body, expected_item);
        if let Some(expected_item) = expected_item {
            self.expect_type(expected_item, &elem_ty, arena.arena.expr(body).span);
        }
        for _ in 0..scopes { self.pop_scope(); }
        Type::List(Box::new(expected_item.cloned().unwrap_or(elem_ty)))
    }

    fn check_map_comp_arena(&mut self, arena: &ArenaProgram, source: &str, key: ExprId, value: ExprId, qualifiers: ArenaRange, expected: Option<&Type>, _span: Span) -> Type {
        let scopes = self.check_comp_qualifiers_arena(arena, source, qualifiers, true);
        let (expected_key, expected_value) = match expected { Some(Type::Map(key, value)) => (Some(key.as_ref()), Some(value.as_ref())), _ => (None, None) };
        let key_ty = self.check_expr_arena(arena, source, key, expected_key);
        if !key_ty.is_map_key() && !key_ty.is_recovery() {
            self.error(arena.arena.expr(key).span, "Map comprehension keys require an ordered scalar key", "check.map-key-type");
        }
        if let Some(expected_key) = expected_key {
            self.expect_type(expected_key, &key_ty, arena.arena.expr(key).span);
        }
        let value_ty = self.check_expr_arena(arena, source, value, expected_value);
        if let Some(expected_value) = expected_value {
            self.expect_type(expected_value, &value_ty, arena.arena.expr(value).span);
        }
        for _ in 0..scopes { self.pop_scope(); }
        Type::Map(Box::new(expected_key.cloned().unwrap_or(key_ty)), Box::new(expected_value.cloned().unwrap_or(value_ty)))
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
                "check.loop-no-break",
            );
        }
        Type::Unknown
    }

    fn check_capture_arena(
        &mut self, arena: &ArenaProgram, source: &str, block: BlockId,
        expected: Option<&Type>, span: Span,
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
        let body = if let Some(expected_ok) = expected_ok { if capture_success_underconstrained(&body) && body.matches_expected(expected_ok) { expected_ok.clone() } else { body } } else { body };
        if capture_success_underconstrained(&body) {
            self.error(span, "cannot infer try success type; annotate Result success type", "check.try-success-type");
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
        span: Span,
    ) -> Type {
        let delay_ids: Vec<ExprId> = arena.arena.expr_ids(delays).collect();
        for &delay in &delay_ids {
            let ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(delay), Some(&Type::Duration), None);
            let delay_span = arena.arena.expr(delay).span;
            self.expect_type(&Type::Duration, &ty, delay_span);
        }
        if !delay_ids.is_empty() {
            self.record_required_effect(Effect::Time);
            if self.in_pure {
                self.error(
                    span,
                    "retry delays are not allowed in pure functions",
                    "check.pure-effect",
                );
            } else if let Some(effs) = &self.current_effects
                && !Self::effects_covers(effs, &Effect::Time)
            {
                self.error(
                    span,
                    "`retry` with delays requires the `time` effect",
                    "check.effect-violation",
                );
            }
        }

        self.push_scope();
        self.begin_error_boundary();
        let body_ty = self.check_tail_block_arena(arena, source, block, None);
        let error_ty = self.end_error_boundary(None);
        self.pop_scope();

        let result_ty = match body_ty {
            Type::Result(ok, err) => Type::Result(ok, err),
            Type::Invalid | Type::Unknown => Type::Result(Box::new(body_ty), Box::new(Type::Error)),
            ty => Type::Result(Box::new(ty), Box::new(error_ty)),
        };
        if let Some(pattern) = pattern {
            let Type::Result(_, error_ty) = &result_ty else { unreachable!() };
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
                for child in arena.arena.pattern_ids(children) { self.check_retry_selection_shape(arena, child); }
            }
            ArenaPatternKind::Literal(_) | ArenaPatternKind::Record { .. } | ArenaPatternKind::List { .. } | ArenaPatternKind::Tuple(_) | ArenaPatternKind::Constructor { .. } => {
                self.error(arena.arena.span(node.span), "retry selection requires a nominal error, facet, type or wildcard pattern", "check.retry-pattern");
            }
            _ => {}
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
                "check.pure-run",
            );
        } else if let Some(effs) = &self.current_effects
            && !Self::effects_covers(effs, &Effect::Process)
        {
            self.error(
                run_span,
                "`run` requires the `process` effect",
                "check.effect-violation",
            );
        }
        self.check_run_arena(arena, source, run_id)
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
                        "check.spawn-run-shape",
                    );
                }
                if let Some(segment) = segments.first()
                    && !matches!(segment.kind, RunKind::Plain | RunKind::Status)
                {
                    let segment_span = arena.arena.span(segment.span);
                    self.error(
                        segment_span,
                        "`spawn run` supports only `run` and `run.status` forms",
                        "check.spawn-run-kind",
                    );
                }
            }
            ArenaSpawnTarget::Command(expr_id) => {
                let ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(expr_id), Some(&Type::Command), None);
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
                    "check.wait-target",
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
                "check.empty-match",
            );
            return expected.cloned().unwrap_or(Type::Unknown);
        }
        let mut inferred = None;
        for arm in arm_list {
            self.push_scope();
            self.check_pattern_arena(arena, source, arm.pattern, &value_ty);
            self.warn_flattened_error_handler_arena(arena, arm.value, arm.pattern, &value_ty);
            if let Some(guard) = arm.guard {
                let guard_ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(guard), Some(&Type::Bool), None);
                let guard_span = arena.arena.expr(guard).span;
                self.expect_type(&Type::Bool, &guard_ty, guard_span);
            }
            let infer_branches = self.inferred_returns.is_some() && expected.is_none();
            let arm_expected = if infer_branches { None } else { expected.or(inferred.as_ref()) };
            let actual = self.check_expr_arena(arena, source, arm.value, arm_expected);
            if let Some(arm_expected) = arm_expected {
                let value_span = arena.arena.expr(arm.value).span;
                self.expect_type(arm_expected, &actual, value_span);
            }
            if actual != Type::Unknown {
                inferred = Some(if infer_branches {
                    inferred.map_or(actual.clone(), |previous| self.unify_inferred_returns(previous, actual, arena.arena.expr(arm.value).span))
                } else { inferred.unwrap_or(actual) });
            }
            self.pop_scope();
        }
        if !super::stmt::patterns_are_exhaustive_arena(arena, &value_ty, arm_list.iter().filter(|arm| arm.guard.is_none()).map(|arm| arm.pattern), &self.type_defs, &self.tag_variants) {
            self.error(span, "value-producing match must be exhaustive", "check.match-value-exhaustive");
        }
        self.check_list_match_coverage_arena(arena, &value_ty, arm_list.iter().map(|arm| (arm.pattern, arena.arena.span(arm.span), arm.guard.is_some())), span);
        self.check_tag_exhaustiveness_arena(
            arena,
            &value_ty,
            arm_list
                .iter()
                .filter(|arm| arm.guard.is_none())
                .map(|a| (a.pattern, arena.arena.span(a.span)))
                .collect(),
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
        let ty = self.check_expr_arena(arena, source, inner, None);
        let span = arena.arena.expr(inner).span;
        match op {
            UnaryOp::Not => {
                if !matches!(ty, Type::Bool | Type::Status | Type::Any | Type::Unknown) {
                    self.expect_type(&Type::Bool, &ty, span);
                }
                Type::Bool
            }
            UnaryOp::Neg => {
                if ty == Type::Any { return Type::Any; }
                if matches!(ty, Type::Float) {
                    Type::Float
                } else {
                    self.expect_type(&Type::Int, &ty, span);
                    Type::Int
                }
            }
        }
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
        if params.len() != 1 {
            self.error(arena.arena.expr(expression).span, "error fallback block requires exactly one parameter", "check.fallback-block-params");
        }
        let (value_ty, error_ty) = match result_ty {
            Type::Result(value, error) => (value.as_ref().clone(), error.as_ref().clone()),
            _ => {
                self.error(left_span, "error fallback block requires a Result value", "check.fallback-block-result");
                (Type::Unknown, Type::Unknown)
            }
        };
        let value_ty = if value_ty == Type::Unknown { expected.cloned().unwrap_or(value_ty) } else { value_ty };
        self.push_scope();
        if let [param] = params {
            if param.name != "_" {
                if matches!(&error_ty, Type::Error | Type::ProcessError | Type::ErrorFamily(_) | Type::ErrorVariant { .. }) {
                    if let Some(value) = Self::single_error_handler_value_arena(arena, block) {
                        self.warn_flattened_error_translation_arena(arena, value, param.name);
                    }
                }
                self.define(param.name, super::Binding::new(error_ty, false), arena.arena.span(param.span));
            }
        }
        // Every handler tail is a value; Unit-success handlers still require Unit.
        let context = (!matches!(value_ty, Type::Unit) && !value_ty.is_result_unit()).then_some(&value_ty);
        let actual = self.check_tail_block_contents_arena(arena, source, block, context);
        if !matches!(actual, Type::Unknown) {
            self.expect_type(&value_ty, &actual, arena.arena.expr(expression).span);
        }
        self.pop_scope();
        let result = if value_ty == Type::Unknown { actual.clone() } else { value_ty };
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
                    return self.check_result_fallback_block_arena(arena, source, right, block, &left_ty, left_span, expected);
                }
                let value_ty = if let Some(ok_ty) = left_ty.result_ok().cloned() {
                    ok_ty
                } else if let Some(inner) = left_ty.optional_inner().cloned() {
                    inner
                } else if self.proof_subject_arena(arena, left).is_some_and(|(name, path, _)| {
                    self.lookup(name).and_then(|binding| binding.unrefined_ty.as_ref())
                        .and_then(|ty| super::proof::projected_type(ty, &path)).is_some_and(|ty| matches!(ty, Type::Optional(_)))
                }) {
                    self.proven_nonnull_fallback_receivers.insert(left_span);
                    left_ty.clone()
                } else {
                    self.error(
                        left_span,
                        "`??` requires a Result or Optional value",
                        "check.result-fallback",
                    );
                    self.check_expr_arena(arena, source, right, None);
                    return Type::Unknown;
                };
                // The unreachable fallback is checked, but cannot mutate its success continuation.
                let saved_scopes = self.proven_nonnull_fallback_receivers.contains(&left_span).then(|| self.scopes.clone());
                let right_ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(right), Some(&value_ty), None);
                if let Some(scopes) = saved_scopes { self.scopes = scopes; }
                self.expect_type(&value_ty, &right_ty, right_span);
                self.type_constraints.resolve(&value_ty).unwrap_or(Type::Invalid)
            }
            BinaryOp::Or => {
                let left_ty = self.check_expr_arena(arena, source, left, None);
                let facts = self.infer_condition_narrowings_arena(arena, left);
                self.push_scope();
                self.apply_narrowings(&facts.when_false);
                let right_ty = if left_ty.is_result() {
                    self.check_expr_arena(arena, source, right, None)
                } else {
                    self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(right), Some(&Type::Bool), None)
                };
                self.pop_scope();
                if left_ty.is_result() || right_ty.is_result() {
                    self.error(
                        left_span,
                        "`or` is only for Bool values; use `??` for Result fallback",
                        "check.result-fallback",
                    );
                    return Type::Bool;
                }
                if left_ty != Type::Any { self.expect_type(&Type::Bool, &left_ty, left_span); }
                if right_ty != Type::Any { self.expect_type(&Type::Bool, &right_ty, right_span); }
                Type::Bool
            }
            BinaryOp::And => {
                let left_ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(left), Some(&Type::Bool), None);
                let facts = self.infer_condition_narrowings_arena(arena, left);
                self.push_scope();
                self.apply_narrowings(&facts.when_true);
                let right_ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(right), Some(&Type::Bool), None);
                self.pop_scope();
                if left_ty != Type::Any { self.expect_type(&Type::Bool, &left_ty, left_span); }
                if right_ty != Type::Any { self.expect_type(&Type::Bool, &right_ty, right_span); }
                Type::Bool
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                let left_ty = self.check_expr_arena(arena, source, left, None);
                let right_ty = self.check_expr_arena(arena, source, right, None);
                if left_ty != Type::Any && right_ty != Type::Any
                    && !left_ty.matches_expected(&right_ty) {
                    self.expect_type(&left_ty, &right_ty, right_span);
                }
                Type::Bool
            }
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                let left_ty = self.check_expr_arena(arena, source, left, None);
                let right_ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(right), Some(&left_ty), None);
                if !matches!(
                    left_ty,
                    Type::Int | Type::UInt | Type::Float | Type::Duration | Type::Str | Type::Any | Type::Unknown
                ) {
                    self.error(
                        left_span,
                        "comparison requires Int, Float, Str, or Duration",
                        "check.operator-type",
                    );
                }
                if left_ty != Type::Any && right_ty != Type::Any {
                    self.expect_type(&left_ty, &right_ty, right_span);
                }
                Type::Bool
            }
            BinaryOp::In | BinaryOp::NotIn => {
                let left_ty = self.check_expr_arena(arena, source, left, None);
                let right_ty = self.check_expr_arena(arena, source, right, None);
                match &right_ty {
                    Type::Map(key, _) => { self.expect_type(key, &left_ty, left_span); }
                    Type::List(item) => {
                        if left_ty != Type::Any { self.expect_type(item, &left_ty, left_span); }
                    }
                    Type::Str => {
                        if left_ty != Type::Any { self.expect_type(&Type::Str, &left_ty, left_span); }
                    }
                    Type::Bytes => {
                        if left_ty != Type::Any { self.expect_type(&Type::Bytes, &left_ty, left_span); }
                    }
                    Type::ErasedRecord | Type::Record(_) => {
                        self.expect_type(&Type::Str, &left_ty, left_span);
                    }
                    Type::Path => {
                        if !matches!(left_ty, Type::Str | Type::Path | Type::Any | Type::Unknown) {
                            self.error(
                                left_span,
                                "Path membership requires Str or Path",
                                "check.membership-type",
                            );
                        }
                    }
                    // env.PATH entries are exact Path values; a Str literal is
                    // not promoted here, so `"/bin" in env.PATH` is rejected
                    // instead of silently comparing Str against Path.
                    Type::EnvPathList => {
                        if !matches!(left_ty, Type::Path | Type::Any | Type::Unknown) {
                            self.error(
                                left_span,
                                "env.PATH membership requires Path; write a path literal such as p\"/opt/bin\"",
                                "check.membership-type",
                            );
                        }
                    }
                    Type::Any | Type::Unknown => {}
                    _ => self.error(
                        right_span,
                        "membership requires List, Map, Record, Str, Bytes, Path, or env.PATH",
                        "check.membership-type",
                    ),
                }
                Type::Bool
            }
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                let left_ty = self.check_expr_arena(arena, source, left, expected);
                let right_expected = if matches!(&left_ty, Type::List(item) if **item == Type::Unknown) {
                    expected
                } else {
                    Some(&left_ty)
                };
                let right_ty = self.check_expr_arena(arena, source, right, right_expected);
                if left_ty == Type::Any || right_ty == Type::Any { return Type::Any; }
                if left_ty == Type::Duration || right_ty == Type::Duration {
                    return match (op, &left_ty, &right_ty) {
                        (BinaryOp::Add | BinaryOp::Sub, Type::Duration, Type::Duration)
                        | (BinaryOp::Mul, Type::Duration, Type::Int)
                        | (BinaryOp::Mul, Type::Int, Type::Duration)
                        | (BinaryOp::Div, Type::Duration, Type::Int) => Type::Duration,
                        (BinaryOp::Div, Type::Duration, Type::Duration) => Type::Int,
                        _ => {
                            self.error(left_span, "invalid Duration arithmetic dimensions", "check.operator-type");
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
                    _ => {
                        self.expect_type(&Type::Int, &left_ty, left_span);
                        self.expect_type(&Type::Int, &right_ty, right_span);
                        Type::Int
                    }
                }
            }
        }
    }

    fn check_field_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        base: ExprId,
        name: Name,
        span: Span,
    ) -> Type {
        if let Some(ty) = self.check_env_typed_field_arena(arena, source, base, span) {
            return ty;
        }
        let base_expr = arena.arena.expr(base);
        if matches!(base_expr.kind, ArenaExprKind::Ident(module) if module == "fs" && self.lookup(module).is_none())
            && name == "ls"
        {
            self.removed_compatibility_name(
                Span::new(span.source_id, span.end() - 2, span.end()), "ls", "children", false,
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
                return Type::Tag(info.type_name);
            }
        }
        let base_ty = self.check_expr_arena(arena, source, base, None);
        match base_ty {
            Type::ErasedRecord | Type::DynamicModule => Type::Any,
            Type::Record(fields) => match fields.get(&name) {
                Some(ty) => ty.clone(),
                None => {
                    self.error(
                        span,
                        "unknown field on known record type",
                        "check.unknown-field",
                    );
                    Type::Unknown
                }
            },
            Type::Module(exports) => match exports.get(&name) {
                Some(export) => export.field_type(),
                None => {
                    self.error(
                        span,
                        "unknown export on known module contract",
                        "check.unknown-field",
                    );
                    Type::Unknown
                }
            },
            Type::Status => match name.as_str().as_str() {
                "ok" | "success" => Type::Bool,
                "kind" => Type::Str,
                "segments" => Type::List(Box::new(Type::ErasedRecord)),
                _ => Type::Unknown,
            },
            Type::ProcessHandle => match name.as_str().as_str() {
                "pid" => Type::Int,
                "command" => Type::Str,
                "argv" => Type::List(Box::new(Type::Str)),
                "detached" => Type::Bool,
                _ => Type::Unknown,
            },
            Type::Path => match name.as_str().as_str() {
                "parent" => Type::Path,
                "name" | "ext" => Type::Str,
                _ => Type::Unknown,
            },
            Type::Digest => match name.as_str().as_str() {
                "algorithm" => Type::Str,
                "bytes" => Type::Bytes,
                _ => Type::Unknown,
            },
            Type::Regex => match name.as_str().as_str() {
                "pattern" => Type::Str,
                _ => Type::Unknown,
            },
            Type::Error | Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ErrorFacet(_) => {
                match name.as_str().as_str() {
                    "message" => Type::Str,
                    "kind" => {
                        self.error(
                            span,
                            "error `.kind` was removed; match exact variants or facets instead",
                            "check.error-removed",
                        );
                        Type::Str
                    }
                    _ => Type::Unknown,
                }
            }
            Type::ProcessError => match name.as_str().as_str() {
                "message" => Type::Str,
                "kind" => Type::Str,
                _ => Type::Unknown,
            },
            _ => {
                if !matches!(base_ty, Type::Any | Type::Unknown) {
                    self.error(
                        span,
                        "field access requires a record-like value",
                        "check.field-access",
                    );
                }
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
        let (inner, wrap_optional) = match base_ty {
            Type::Optional(inner) => (*inner, true),
            Type::Result(_, _) => (self.check_propagation(&base_ty, span), false),
            Type::Any => return Type::Any,
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
        let field_ty = match &inner {
            Type::ErasedRecord => Type::Any,
            Type::Record(fields) => match fields.get(&name) {
                Some(ty) => ty.clone(),
                None => {
                    self.error(
                        span,
                        "unknown field on known record type",
                        "check.unknown-field",
                    );
                    Type::Unknown
                }
            },
            Type::Error | Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ErrorFacet(_) => {
                match name.as_str().as_str() {
                    "message" => Type::Str,
                    "kind" => {
                        self.error(
                            span,
                            "error `.kind` was removed; match exact variants or facets instead",
                            "check.error-removed",
                        );
                        Type::Str
                    }
                    _ => Type::Unknown,
                }
            }
            Type::ProcessError => match name.as_str().as_str() {
                "message" => Type::Str,
                "kind" => Type::Str,
                _ => Type::Unknown,
            },
            Type::ProcessHandle => match name.as_str().as_str() {
                "pid" => Type::Int,
                "command" => Type::Str,
                "argv" => Type::List(Box::new(Type::Str)),
                "detached" => Type::Bool,
                _ => Type::Unknown,
            },
            Type::Any => Type::Any,
            _ => Type::Unknown,
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
                "check.pure-effect",
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
                    "check.unknown-env-namespace",
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
                self.error(span, "guarded indexing requires a checked Optional or Result receiver", "check.null-safe-index");
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
        let (base_ty, lift) = self.checked_postfix_receiver(base_ty, guarded, span);
        let result = match base_ty {
            Type::Map(key, item) => {
                let index_ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(index), Some(&key), None);
                self.expect_type(&key, &index_ty, index_span);
                *item
            }
            Type::List(item) => {
                let index_ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(index), Some(&Type::Int), None);
                self.expect_type(&Type::Int, &index_ty, index_span);
                *item
            }
            receiver @ (Type::ErasedRecord | Type::Record(_) | Type::Module(_)) => {
                let index_ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(index), Some(&Type::Str), None);
                self.expect_type(&Type::Str, &index_ty, index_span);
                if let Some(projection) = crate::sema::projection::resolve_constant_key_projection(
                    &arena.arena, &self.prepared_constants, base, &receiver, index,
                    crate::sema::projection::ProjectionOperation::Index,
                ) {
                    let ty = projection.value_type.clone();
                    self.projections.insert(span, projection);
                    ty
                } else { Type::Any }
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
                self.error(span, "indexing requires List or Record", "check.index-type");
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
        let (base_ty, lift) = self.checked_postfix_receiver(base_ty, guarded, span);
        if let Some(start) = start {
            let ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(start), Some(&Type::Int), None);
            let start_span = arena.arena.expr(start).span;
            self.expect_type(&Type::Int, &ty, start_span);
        }
        if let Some(end) = end {
            let ty = self.check_expr_with_schema_arena(arena, source, ArenaExprOrRun::Expr(end), Some(&Type::Int), None);
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
                self.error(span, "slicing requires List, Str, or Bytes", "check.slice-type");
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
                ArenaStmtKind::Let { initializer, .. } | ArenaStmtKind::Const { initializer, .. } | ArenaStmtKind::Var { initializer, .. } => {
                    initializer
                }
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
        assert_arena_matches_raised("let x = abort(1)");
        assert_arena_matches_raised("let x = abort(1, force: true)");
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
