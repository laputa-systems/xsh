#![allow(clippy::single_call_fn)]

use super::{
    BTreeMap, BinaryOp, Checker, Diagnostic, Effect, Label, Name, RunKind, Span, Type, UnaryOp,
    api_spec, block_has_exit_point_arena, collection_item_ty, merge_collection_item_ty,
};
use crate::syntax::arena::{
    ArenaExprKind, ArenaExprOrRun, ArenaFmtPart, ArenaProgram, ArenaRange, ArenaRecordFieldKind,
    ArenaSpawnForm, ArenaSpawnTarget, ArenaWaitForm, BindingTargetId, BlockId, ExprId, RunFormId,
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
impl Checker {
    pub(super) fn lookup_expr_ident(&mut self, name: Name, span: Span) -> Type {
        if let Some(binding) = self.lookup(name) {
            return binding.ty.clone();
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
            return Type::Record(BTreeMap::new());
        }
        self.error(span, "unresolved name", "check.unresolved-name");
        Type::Unknown
    }

    pub(super) fn check_env_get(&mut self, kind: EnvGetKind, span: Span) -> Type {
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

    pub(super) fn check_expr_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        id: ExprId,
        expected: Option<&Type>,
    ) -> Type {
        let expr = arena.arena.expr(id);
        let ty = match &expr.kind {
            ArenaExprKind::Null => Type::Null,
            ArenaExprKind::Bool(_) => Type::Bool,
            ArenaExprKind::Int(_) => Type::Int,
            ArenaExprKind::Float(_) => Type::Float,
            ArenaExprKind::Duration(_) => Type::Duration,
            ArenaExprKind::Str(_) => Type::Str,
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
            ArenaExprKind::Ident(name) => self.lookup_expr_ident(*name, expr.span),
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
            ArenaExprKind::If {
                branches,
                else_value,
            } => self.check_if_expr_arena(arena, source, *branches, *else_value, expected),
            ArenaExprKind::Unary { op, expr: inner } => {
                self.check_unary_arena(arena, source, *op, *inner)
            }
            ArenaExprKind::Binary { op, left, right } => {
                self.check_binary_arena(arena, source, *op, *left, *right)
            }
            ArenaExprKind::Field { base, name } => {
                self.check_field_arena(arena, source, *base, *name, expr.span)
            }
            ArenaExprKind::NullSafeField { base, name } => {
                self.check_null_safe_field_arena(arena, source, *base, *name, expr.span)
            }
            ArenaExprKind::Index { base, index } => {
                self.check_index_arena(arena, source, *base, *index, expr.span)
            }
            ArenaExprKind::Slice { base, start, end } => {
                self.check_slice_arena(arena, source, *base, *start, *end, expr.span)
            }
            ArenaExprKind::EnvGet { kind, .. } => self.check_env_get(*kind, expr.span),
            ArenaExprKind::EnvPathList => Type::EnvPathList,
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
                let ty = self.check_expr_arena(arena, source, *inner, None);
                self.check_propagation(&ty, expr.span)
            }
            ArenaExprKind::Require { value, schema } => {
                self.check_expr_arena(arena, source, *value, None);
                let schema_ty = self.type_from_arena(arena, *schema);
                Type::Result(Box::new(schema_ty), Box::new(Type::Error))
            }
            ArenaExprKind::Call { callee, args } => {
                self.check_call_arena(arena, source, *callee, *args, expr.span)
            }
            ArenaExprKind::Match { value, arms } => {
                self.check_match_expr_arena(arena, source, *value, *arms, expected, expr.span)
            }
            ArenaExprKind::ListComp {
                expr: body,
                target,
                iter,
                condition,
            } => self
                .check_list_comp_arena(arena, source, *body, *target, *iter, *condition, expr.span),
            ArenaExprKind::MapComp {
                key,
                value,
                target,
                iter,
                condition,
            } => self.check_map_comp_arena(
                arena, source, *key, *value, *target, *iter, *condition, expr.span,
            ),
            ArenaExprKind::Loop { block } => {
                self.check_loop_arena(arena, source, *block, expr.span)
            }
            ArenaExprKind::Retry { delays, block } => {
                self.check_retry_arena(arena, source, *delays, *block, expr.span)
            }
            ArenaExprKind::Run(run_id) => self.check_run_expr_arena(arena, source, *run_id),
            ArenaExprKind::Spawn(form) => self.check_spawn_form_arena(arena, source, form),
            ArenaExprKind::Wait(form) => self.check_wait_form_arena(arena, source, form),
            ArenaExprKind::BuilderCall { call, block } => {
                self.check_builder_call_arena(arena, source, *call, *block, expr.span)
            }
        };
        self.expr_types.insert(expr.span, ty.clone());
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
        range: ArenaRange,
        expected: Option<&Type>,
        _span: Span,
    ) -> Type {
        let items: Vec<ExprId> = arena.arena.expr_ids(range).collect();
        if items.is_empty() {
            if let Some(Type::List(item)) = expected {
                return Type::List(item.clone());
            }
            if let Some(Type::Any) = expected {
                return Type::List(Box::new(Type::Any));
            }
            // Defer to context — empty list type is refined by later checks.
            return Type::List(Box::new(Type::Unknown));
        }

        if let Some(Type::List(item_ty)) = expected {
            for &item in &items {
                let actual = self.check_expr_arena(arena, source, item, Some(item_ty));
                let item_span = arena.arena.expr(item).span;
                self.expect_type(item_ty, &actual, item_span);
            }
            return Type::List(item_ty.clone());
        }

        let mut first = self.check_expr_arena(arena, source, items[0], None);
        for &item in &items[1..] {
            let item_ty = self.check_expr_arena(arena, source, item, None);
            let item_span = arena.arena.expr(item).span;
            if let Some(merged) = merge_list_literal_item_ty(&first, &item_ty) {
                first = merged;
            } else {
                self.expect_type(&first, &item_ty, item_span);
            }
        }
        Type::List(Box::new(first))
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
        if fields.is_empty()
            && let Some(Type::Map(item)) = expected
        {
            return Type::Map(item.clone());
        }
        if matches!(
            expected,
            Some(Type::Status | Type::ProcessHandle | Type::NetJob)
        ) {
            for field in fields {
                match &field.kind {
                    ArenaRecordFieldKind::Spread { expr, .. }
                    | ArenaRecordFieldKind::Named { value: expr, .. } => {
                        self.check_expr_arena(arena, source, *expr, None);
                    }
                    ArenaRecordFieldKind::Shorthand { name, span } => {
                        let field_span = arena.arena.span(*span);
                        self.lookup_expr_ident(*name, field_span);
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
                    let ty = self.check_expr_arena(arena, source, *value, field_expected);
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
                    let ty = self.lookup_expr_ident(*name, field_span);
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
            let condition =
                self.check_expr_arena(arena, source, branch.condition, Some(&Type::Bool));
            let cond_span = arena.arena.expr(branch.condition).span;
            self.expect_type(&Type::Bool, &condition, cond_span);
            let branch_expected = expected.or(inferred.as_ref());
            let actual = self.check_expr_arena(arena, source, branch.value, branch_expected);
            if let Some(branch_expected) = branch_expected {
                let value_span = arena.arena.expr(branch.value).span;
                self.expect_type(branch_expected, &actual, value_span);
            }
            if inferred.is_none() {
                inferred = Some(actual);
            }
        }
        let else_expected = expected.or(inferred.as_ref());
        let else_ty = self.check_expr_arena(arena, source, else_value, else_expected);
        if let Some(else_expected) = else_expected {
            let else_span = arena.arena.expr(else_value).span;
            self.expect_type(else_expected, &else_ty, else_span);
        }
        expected.cloned().or(inferred).unwrap_or(else_ty)
    }

    fn check_list_comp_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        body: ExprId,
        target: BindingTargetId,
        iter: ExprId,
        condition: Option<ExprId>,
        span: Span,
    ) -> Type {
        let iter_ty = self.check_expr_arena(arena, source, iter, None);
        let item_ty = match iter_ty {
            Type::List(item) => *item,
            Type::Stream(item) => *item,
            Type::Any | Type::Unknown => Type::Any,
            Type::Result(ok, _) => match *ok {
                Type::List(item) => *item,
                Type::Stream(item) => *item,
                _ => Type::Unknown,
            },
            _ => {
                let iter_span = arena.arena.expr(iter).span;
                self.error(
                    iter_span,
                    "list comprehension iterates over List or Stream values",
                    "check.listcomp-iterator",
                );
                Type::Unknown
            }
        };
        self.push_scope();
        self.define_binding_target_arena(arena, target, &item_ty, false, span);
        if let Some(cond) = condition {
            let cond_ty = self.check_expr_arena(arena, source, cond, None);
            if !matches!(
                cond_ty,
                Type::Bool | Type::Status | Type::Any | Type::Unknown
            ) {
                let cond_span = arena.arena.expr(cond).span;
                self.error(
                    cond_span,
                    "list comprehension condition must be Bool",
                    "check.listcomp-condition",
                );
            }
        }
        let elem_ty = self.check_expr_arena(arena, source, body, None);
        self.pop_scope();
        Type::List(Box::new(elem_ty))
    }

    fn check_map_comp_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        key: ExprId,
        value: ExprId,
        target: BindingTargetId,
        iter: ExprId,
        condition: Option<ExprId>,
        span: Span,
    ) -> Type {
        let iter_ty = self.check_expr_arena(arena, source, iter, None);
        let item_ty = match iter_ty {
            Type::List(item) => *item,
            Type::Stream(item) => *item,
            Type::Any | Type::Unknown => Type::Any,
            Type::Result(ok, _) => match *ok {
                Type::List(item) => *item,
                Type::Stream(item) => *item,
                _ => Type::Unknown,
            },
            _ => {
                let iter_span = arena.arena.expr(iter).span;
                self.error(
                    iter_span,
                    "map comprehension iterates over List or Stream values",
                    "check.mapcomp-iterator",
                );
                Type::Unknown
            }
        };
        self.push_scope();
        self.define_binding_target_arena(arena, target, &item_ty, false, span);
        if let Some(cond) = condition {
            let cond_ty = self.check_expr_arena(arena, source, cond, None);
            if !matches!(
                cond_ty,
                Type::Bool | Type::Status | Type::Any | Type::Unknown
            ) {
                let cond_span = arena.arena.expr(cond).span;
                self.error(
                    cond_span,
                    "map comprehension condition must be Bool",
                    "check.mapcomp-condition",
                );
            }
        }
        let key_ty = self.check_expr_arena(arena, source, key, Some(&Type::Str));
        let key_span = arena.arena.expr(key).span;
        self.expect_type(&Type::Str, &key_ty, key_span);
        let value_ty = self.check_expr_arena(arena, source, value, None);
        self.pop_scope();
        Type::Map(Box::new(value_ty))
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

    fn check_retry_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        delays: ArenaRange,
        block: BlockId,
        span: Span,
    ) -> Type {
        let delay_ids: Vec<ExprId> = arena.arena.expr_ids(delays).collect();
        for &delay in &delay_ids {
            let ty = self.check_expr_arena(arena, source, delay, Some(&Type::Duration));
            let delay_span = arena.arena.expr(delay).span;
            self.expect_type(&Type::Duration, &ty, delay_span);
        }
        if !delay_ids.is_empty() {
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
        self.retry_attempt_depth += 1;
        let body_ty = self.check_tail_block_arena(arena, source, block, None);
        self.retry_attempt_depth -= 1;
        self.pop_scope();

        match body_ty {
            Type::Result(ok, err) => Type::Result(ok, err),
            Type::Invalid | Type::Unknown => Type::Result(Box::new(body_ty), Box::new(Type::Error)),
            ty => Type::Result(Box::new(ty), Box::new(Type::Error)),
        }
    }

    fn check_run_expr_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        run_id: RunFormId,
    ) -> Type {
        let run_span = arena.arena.span(arena.arena.run_form(run_id).span);
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
                let ty = self.check_expr_arena(arena, source, expr_id, Some(&Type::Command));
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
            if let Some(guard) = arm.guard {
                let guard_ty = self.check_expr_arena(arena, source, guard, Some(&Type::Bool));
                let guard_span = arena.arena.expr(guard).span;
                self.expect_type(&Type::Bool, &guard_ty, guard_span);
            }
            let arm_expected = expected.or(inferred.as_ref());
            let actual = self.check_expr_arena(arena, source, arm.value, arm_expected);
            if let Some(arm_expected) = arm_expected {
                let value_span = arena.arena.expr(arm.value).span;
                self.expect_type(arm_expected, &actual, value_span);
            }
            if inferred.is_none() {
                inferred = Some(actual);
            }
            self.pop_scope();
        }
        self.check_tag_exhaustiveness_arena(
            arena,
            &value_ty,
            arm_list
                .iter()
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
                if matches!(ty, Type::Float) {
                    Type::Float
                } else {
                    self.expect_type(&Type::Int, &ty, span);
                    Type::Int
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn check_binary_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        op: BinaryOp,
        left: ExprId,
        right: ExprId,
    ) -> Type {
        let left_span = arena.arena.expr(left).span;
        let right_span = arena.arena.expr(right).span;
        match op {
            BinaryOp::ResultFallback => {
                let left_ty = self.check_expr_arena(arena, source, left, None);
                let value_ty = if let Some(ok_ty) = left_ty.result_ok().cloned() {
                    ok_ty
                } else if let Some(inner) = left_ty.optional_inner().cloned() {
                    inner
                } else {
                    self.error(
                        left_span,
                        "`??` requires a Result or Optional value",
                        "check.result-fallback",
                    );
                    self.check_expr_arena(arena, source, right, None);
                    return Type::Unknown;
                };
                let right_ty = self.check_expr_arena(arena, source, right, Some(&value_ty));
                self.expect_type(&value_ty, &right_ty, right_span);
                value_ty
            }
            BinaryOp::Or => {
                let left_ty = self.check_expr_arena(arena, source, left, None);
                let right_ty = if left_ty.is_result() {
                    self.check_expr_arena(arena, source, right, None)
                } else {
                    self.check_expr_arena(arena, source, right, Some(&Type::Bool))
                };
                if left_ty.is_result() || right_ty.is_result() {
                    self.error(
                        left_span,
                        "`or` is only for Bool values; use `??` for Result fallback",
                        "check.result-fallback",
                    );
                    return Type::Bool;
                }
                self.expect_type(&Type::Bool, &left_ty, left_span);
                self.expect_type(&Type::Bool, &right_ty, right_span);
                Type::Bool
            }
            BinaryOp::And => {
                let left_ty = self.check_expr_arena(arena, source, left, Some(&Type::Bool));
                let right_ty = self.check_expr_arena(arena, source, right, Some(&Type::Bool));
                self.expect_type(&Type::Bool, &left_ty, left_span);
                self.expect_type(&Type::Bool, &right_ty, right_span);
                Type::Bool
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                let left_ty = self.check_expr_arena(arena, source, left, None);
                let right_ty = self.check_expr_arena(arena, source, right, Some(&left_ty));
                self.expect_type(&left_ty, &right_ty, right_span);
                Type::Bool
            }
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                let left_ty = self.check_expr_arena(arena, source, left, None);
                let right_ty = self.check_expr_arena(arena, source, right, Some(&left_ty));
                if !matches!(
                    left_ty,
                    Type::Int | Type::Float | Type::Str | Type::Any | Type::Unknown
                ) {
                    self.error(
                        left_span,
                        "comparison requires Int, Float, or Str",
                        "check.operator-type",
                    );
                }
                self.expect_type(&left_ty, &right_ty, right_span);
                Type::Bool
            }
            BinaryOp::In | BinaryOp::NotIn => {
                let left_ty = self.check_expr_arena(arena, source, left, None);
                let right_ty = self.check_expr_arena(arena, source, right, None);
                match &right_ty {
                    Type::List(item) => {
                        self.expect_type(item, &left_ty, left_span);
                    }
                    Type::Str => {
                        self.expect_type(&Type::Str, &left_ty, left_span);
                    }
                    Type::Bytes => {
                        self.expect_type(&Type::Bytes, &left_ty, left_span);
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
                    Type::EnvPathList => {
                        let left_kind = arena.arena.expr(left).kind;
                        if !is_path_like_arena_expr(&left_kind, &left_ty) {
                            self.error(
                                left_span,
                                "env.PATH membership requires Path",
                                "check.membership-type",
                            );
                        }
                    }
                    Type::Any | Type::Unknown => {}
                    _ => self.error(
                        right_span,
                        "membership requires List, Str, Bytes, Path, or env.PATH",
                        "check.membership-type",
                    ),
                }
                Type::Bool
            }
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                let left_ty = self.check_expr_arena(arena, source, left, None);
                let right_ty = self.check_expr_arena(arena, source, right, Some(&left_ty));
                match left_ty {
                    Type::Float if !matches!(op, BinaryOp::Rem) => {
                        self.expect_type(&Type::Float, &right_ty, right_span);
                        Type::Float
                    }
                    Type::Str if matches!(op, BinaryOp::Add) => {
                        self.expect_type(&Type::Str, &right_ty, right_span);
                        Type::Str
                    }
                    Type::List(_)
                        if matches!((&left_ty, &right_ty), (Type::List(_), Type::List(_))) =>
                    {
                        self.diagnostics.push(
                            Diagnostic::error("list concatenation does not use `+`")
                                .with_code("check.operator-type")
                                .with_label(Label::primary(
                                    left_span,
                                    "`+` is defined for numbers and strings, not lists",
                                ))
                                .with_note("use `.extend(other)` to concatenate lists"),
                        );
                        Type::List(Box::new(merge_collection_item_ty(
                            collection_item_ty(&left_ty),
                            collection_item_ty(&right_ty),
                        )))
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
        if matches!(&base_expr.kind, ArenaExprKind::Ident(module) if module == "env")
            && name == "PATH"
        {
            return Type::EnvPathList;
        }
        let base_ty = self.check_expr_arena(arena, source, base, None);
        match base_ty {
            Type::Record(fields) => match fields.get(&name) {
                Some(ty) => ty.clone(),
                None if fields.is_empty() => Type::Any,
                None => {
                    if self.options.strict_dynamic {
                        self.warning(
                            span,
                            "unknown field on known record type",
                            "check.unknown-field",
                        );
                    }
                    Type::Any
                }
            },
            Type::Module(exports) => match exports.get(&name) {
                Some(export) => export.field_type(),
                None if exports.is_empty() => Type::Any,
                None => {
                    if self.options.strict_dynamic {
                        self.warning(
                            span,
                            "unknown export on known module contract",
                            "check.unknown-field",
                        );
                    }
                    Type::Any
                }
            },
            Type::Status => match name.as_str().as_str() {
                "ok" | "success" => Type::Bool,
                "kind" => Type::Str,
                "segments" => Type::List(Box::new(Type::Record(BTreeMap::new()))),
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
            Type::Error | Type::ErrorFamily(_) | Type::ErrorVariant { .. } => {
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
            Type::Result(ok, _) => (*ok, false),
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
            Type::Record(fields) => match fields.get(&name) {
                Some(ty) => ty.clone(),
                None if fields.is_empty() => Type::Any,
                None => {
                    if self.options.strict_dynamic {
                        self.warning(
                            span,
                            "unknown field on known record type",
                            "check.unknown-field",
                        );
                    }
                    Type::Any
                }
            },
            Type::Error | Type::ErrorFamily(_) | Type::ErrorVariant { .. } => {
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
        if wrap_optional {
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

    fn check_index_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        base: ExprId,
        index: ExprId,
        span: Span,
    ) -> Type {
        let base_ty = self.check_expr_arena(arena, source, base, None);
        let index_span = arena.arena.expr(index).span;
        match base_ty {
            Type::List(item) => {
                let index_ty = self.check_expr_arena(arena, source, index, Some(&Type::Int));
                self.expect_type(&Type::Int, &index_ty, index_span);
                *item
            }
            Type::Record(_) => {
                let index_ty = self.check_expr_arena(arena, source, index, Some(&Type::Str));
                self.expect_type(&Type::Str, &index_ty, index_span);
                Type::Any
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
        }
    }

    fn check_slice_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        base: ExprId,
        start: Option<ExprId>,
        end: Option<ExprId>,
        span: Span,
    ) -> Type {
        let base_ty = self.check_expr_arena(arena, source, base, None);
        if let Some(start) = start {
            let ty = self.check_expr_arena(arena, source, start, Some(&Type::Int));
            let start_span = arena.arena.expr(start).span;
            self.expect_type(&Type::Int, &ty, start_span);
        }
        if let Some(end) = end {
            let ty = self.check_expr_arena(arena, source, end, Some(&Type::Int));
            let end_span = arena.arena.expr(end).span;
            self.expect_type(&Type::Int, &ty, end_span);
        }
        match base_ty {
            Type::List(_) => base_ty,
            Type::Str => Type::Str,
            Type::Any => Type::Any,
            Type::Unknown => Type::Unknown,
            _ => {
                self.error(span, "slicing requires List or Str", "check.slice-type");
                Type::Unknown
            }
        }
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
                ArenaStmtKind::Let { initializer, .. } | ArenaStmtKind::Var { initializer, .. } => {
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
            "guard let x = Ok(1) else |e| {\n  let z = e\n}\nlet y = x",
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
        assert_stmts_arena_match_raised("env {\n  FOO = \"bar\"\n} {\n  print \"in env\"\n}");
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
        assert_stmts_arena_match_raised("type Kind = A | B | C");
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
