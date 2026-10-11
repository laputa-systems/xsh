use super::{
    ArenaBuilderEntryKind, ArenaCallArg, ArenaCallArgKind, ArenaCommandArg, ArenaCommandArgKind,
    ArenaExprKind, ArenaExprOrRun, ArenaFmtPart, ArenaPipeStageKind, ArenaRange,
    ArenaRecordFieldKind, ArenaSpawnTarget, ArenaStmtKind, ArenaStreamStage, ArenaWordPart,
    AstArena, BinaryOp, BlockId, Diagnostic, DiagnosticCode, ExprId, FixHint, FxHashMap, Label,
    Linter, Name, Severity, Type, UnaryOp, is_module_call,
};

pub(super) fn list_splice_element_type_is_precise(ty: &Type) -> bool {
    match ty {
        Type::Bool
        | Type::Int
        | Type::Float
        | Type::Duration
        | Type::Str
        | Type::Bytes
        | Type::Path => true,
        Type::List(item) => list_splice_element_type_is_precise(item),
        _ => false,
    }
}

pub(super) fn pipeline_argument_expr(arg: &ArenaCallArg) -> Option<ExprId> {
    match arg.kind {
        ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. } => Some(value),
        _ => None,
    }
}

pub(super) fn list_update_argument_stable(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Null
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Regex(_) => true,
        ArenaExprKind::List(items) | ArenaExprKind::Set(items) => arena
            .list_element_exprs(items)
            .all(|item| list_update_argument_stable(arena, item)),
        _ => false,
    }
}

/// Whether `text` spells `name` as a whole identifier. A nested block can
/// assign a binding only by writing its name, so a missing spelling proves it
/// leaves the binding alone; a spelling inside a string or comment merely errs
/// toward the conservative answer.
pub(super) fn mentions_identifier(text: &str, name: &str) -> bool {
    let identifier_char = |ch: char| ch.is_alphanumeric() || ch == '_';
    text.match_indices(name).any(|(start, _)| {
        !text[..start]
            .chars()
            .next_back()
            .is_some_and(identifier_char)
            && !text[start + name.len()..]
                .chars()
                .next()
                .is_some_and(identifier_char)
    })
}

/// Whether evaluating `expr` can assign the variable spelled `name`.
///
/// `x = x.push(a)` reads `x` before it evaluates `a`, and `x += [a]` reads it
/// after, so the two spellings agree exactly when `a` leaves `x` alone. A
/// local variable is visible only to its own callable, and an expression can
/// assign it only through statements in a block nested inside the expression
/// (a callback, a `try` or value block, a builder or command interpolation);
/// calls cannot reach it, because nested declarations are rejected and a
/// callable value never captures a caller's local. Module-level variables are
/// not covered: any proc call may assign them. `text` is the source of the
/// whole expression, which stands in for forms whose own span does not cover
/// everything they evaluate (commands and builders).
pub(super) fn expr_may_assign_local(
    arena: &AstArena,
    source: &str,
    expr: ExprId,
    name: &str,
    text: &str,
) -> bool {
    let nested_statements = matches!(
        arena.expr(expr).kind,
        ArenaExprKind::Run(_)
            | ArenaExprKind::Spawn(_)
            | ArenaExprKind::Wait(_)
            | ArenaExprKind::BuilderCall { .. }
    ) && mentions_identifier(text, name);
    nested_statements
        || expr_child_blocks(arena, expr).into_iter().any(|block| {
            source
                .get(arena.span(arena.block(block).span).range())
                .is_none_or(|block_text| mentions_identifier(block_text, name))
        })
        || expr_child_exprs(arena, expr)
            .into_iter()
            .any(|child| expr_may_assign_local(arena, source, child, name, text))
}

/// Enumerate the immediate child expressions of an expression for structural
/// traversal (mirrors the old `visitor::walk_expr` descent).
pub(super) fn expr_child_exprs(arena: &AstArena, expr: ExprId) -> Vec<ExprId> {
    let mut out = Vec::new();
    match arena.expr(expr).kind {
        ArenaExprKind::ValuePipelineCall { input, call, .. } => {
            out.push(input);
            out.push(call);
        }

        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            for part in arena.fmt_parts(parts).collect::<Vec<_>>() {
                if let ArenaFmtPart::Expr(e, _) = part {
                    out.push(e);
                }
            }
        }
        ArenaExprKind::List(items) | ArenaExprKind::Set(items) => {
            out.extend(arena.list_element_exprs(items))
        }
        ArenaExprKind::ListComp { expr, qualifiers }
        | ArenaExprKind::SetComp { expr, qualifiers } => {
            out.extend(arena.comp_qualifiers(qualifiers).iter().map(|q| q.expr()));
            out.push(expr);
        }
        ArenaExprKind::MapComp {
            key,
            value,
            qualifiers,
        } => {
            out.extend(arena.comp_qualifiers(qualifiers).iter().map(|q| q.expr()));
            out.push(key);
            out.push(value);
        }
        ArenaExprKind::Record(fields) => {
            for field in arena.record_fields(fields) {
                match field.kind {
                    ArenaRecordFieldKind::Computed { key, value, .. } => {
                        out.push(key);
                        out.push(value);
                    }
                    ArenaRecordFieldKind::Named { value, .. }
                    | ArenaRecordFieldKind::Path { value, .. } => out.push(value),
                    ArenaRecordFieldKind::Spread { expr, .. } => out.push(expr),
                    ArenaRecordFieldKind::Shorthand { .. } => {}
                }
            }
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            for branch in arena.if_expr_branches(branches) {
                out.push(branch.condition);
                out.push(branch.value);
            }
            out.push(else_value);
        }
        ArenaExprKind::Match { value, arms }
        | ArenaExprKind::PatternTest { value, arms }
        | ArenaExprKind::PatternCondition { value, arms } => {
            out.push(value);
            for arm in arena.match_expr_arms(arms) {
                out.extend(arm.guard);
                out.push(arm.value);
            }
        }
        ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => out.push(expr),
        ArenaExprKind::ComparisonChain(pairs) => out.extend(arena.comparison_chain_operands(pairs)),
        ArenaExprKind::Binary { left, right, .. } => {
            out.push(left);
            out.push(right);
        }
        ArenaExprKind::Call { callee, args } => {
            out.push(callee);
            for arg in arena.call_args(args) {
                match arg.kind {
                    ArenaCallArgKind::Positional(e)
                    | ArenaCallArgKind::Named { value: e, .. }
                    | ArenaCallArgKind::Splice { value: e, .. }
                    | ArenaCallArgKind::NamedSpread { value: e, .. } => out.push(e),
                }
            }
        }
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
            out.push(base);
        }
        ArenaExprKind::Index { base, index, .. } => {
            out.push(base);
            out.push(index);
        }
        ArenaExprKind::Slice {
            base, start, end, ..
        } => {
            out.push(base);
            out.extend(start);
            out.extend(end);
        }
        ArenaExprKind::Pipeline { input, stages } => {
            out.push(input);
            for stage in arena.pipe_stages(stages).to_vec() {
                match stage.kind {
                    ArenaPipeStageKind::Expr(e) => out.push(e),
                    ArenaPipeStageKind::Stream(stage) => {
                        for arg in arena.call_args(stage.args) {
                            match arg.kind {
                                ArenaCallArgKind::Positional(e)
                                | ArenaCallArgKind::Named { value: e, .. }
                                | ArenaCallArgKind::Splice { value: e, .. }
                                | ArenaCallArgKind::NamedSpread { value: e, .. } => out.push(e),
                            }
                        }
                    }
                }
            }
        }
        ArenaExprKind::StructuredPipeline { input, stages } => {
            out.push(input);
            for stage in arena.stream_stages(stages).to_vec() {
                for arg in arena.call_args(stage.args) {
                    match arg.kind {
                        ArenaCallArgKind::Positional(e)
                        | ArenaCallArgKind::Named { value: e, .. }
                        | ArenaCallArgKind::Splice { value: e, .. }
                        | ArenaCallArgKind::NamedSpread { value: e, .. } => out.push(e),
                    }
                }
            }
        }
        ArenaExprKind::Spawn(form) => {
            if let ArenaSpawnTarget::Command(e) = form.target {
                out.push(e);
            }
        }
        ArenaExprKind::Wait(form) => out.push(form.target),
        ArenaExprKind::BuilderCall { call, .. } => out.push(call),
        ArenaExprKind::Require { value, .. } | ArenaExprKind::Convert { value, .. } => {
            out.push(value)
        }
        ArenaExprKind::ErrorContext { message, .. }
        | ArenaExprKind::ContextScope { input: message, .. } => out.push(message),
        ArenaExprKind::TempDirScope { path, .. } => out.extend(path),
        ArenaExprKind::Retry { delays, .. } => out.extend(arena.expr_ids(delays)),
        ArenaExprKind::ResourceScope { bindings, .. } => out.extend(
            arena
                .with_bindings(bindings)
                .iter()
                .map(|binding| binding.initializer),
        ),
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Regex(_)
        | ArenaExprKind::Ident(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::EnvString(_)
        | ArenaExprKind::EnvPathList
        | ArenaExprKind::Run(_)
        | ArenaExprKind::Capture(_)
        | ArenaExprKind::ValueBlock(_)
        | ArenaExprKind::Loop { .. }
        | ArenaExprKind::Collect { .. } => {}
    }
    out
}

/// Enumerate the immediate child statement-blocks of an expression.
pub(super) fn expr_child_blocks(arena: &AstArena, expr: ExprId) -> Vec<BlockId> {
    let mut out = Vec::new();
    match arena.expr(expr).kind {
        ArenaExprKind::Capture(block)
        | ArenaExprKind::ValueBlock(block)
        | ArenaExprKind::Loop { block }
        | ArenaExprKind::Collect { block }
        | ArenaExprKind::Retry { block, .. }
        | ArenaExprKind::ErrorContext { block, .. }
        | ArenaExprKind::ContextScope { block, .. }
        | ArenaExprKind::TempDirScope { block, .. }
        | ArenaExprKind::ResourceScope { block, .. } => out.push(block),
        ArenaExprKind::Pipeline { stages, .. } => {
            for stage in arena.pipe_stages(stages).to_vec() {
                if let ArenaPipeStageKind::Stream(stage) = stage.kind
                    && let Some(block) = stage.block
                {
                    out.push(block);
                }
            }
        }
        ArenaExprKind::StructuredPipeline { stages, .. } => {
            for stage in arena.stream_stages(stages).to_vec() {
                if let Some(block) = stage.block {
                    out.push(block);
                }
            }
        }
        _ => {}
    }
    out
}

// Unsigned schema checks retain the nonnegative constraint even when the
// selected storage has only an Int runtime tag.
pub(super) fn type_has_unsigned_constraint(ty: &Type) -> bool {
    match ty {
        Type::UInt => true,
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) => {
            type_has_unsigned_constraint(inner)
        }
        Type::Map(key, value) | Type::Result(key, value) => {
            type_has_unsigned_constraint(key) || type_has_unsigned_constraint(value)
        }
        Type::Record(fields) => fields.values().any(type_has_unsigned_constraint),
        _ => false,
    }
}

pub(super) fn type_has_contextual_collection_domain(ty: &Type) -> bool {
    match ty {
        Type::UInt | Type::Optional(_) | Type::Record(_) => true,
        Type::List(inner) | Type::Stream(inner) => type_has_contextual_collection_domain(inner),
        Type::Map(key, value) | Type::Result(key, value) => {
            type_has_contextual_collection_domain(key)
                || type_has_contextual_collection_domain(value)
        }
        _ => false,
    }
}

pub(super) fn type_mentions_path(ty: &Type) -> bool {
    match ty {
        Type::Path => true,
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) => {
            type_mentions_path(inner)
        }
        Type::Map(key, value) | Type::Result(key, value) => {
            type_mentions_path(key) || type_mentions_path(value)
        }
        _ => false,
    }
}

pub(super) fn expr_is_dynamic_require_boundary(arena: &AstArena, expr: ExprId) -> bool {
    let expr = match arena.expr(expr).kind {
        ArenaExprKind::Try(inner) => inner,
        _ => expr,
    };
    let ArenaExprKind::Call { callee, .. } = arena.expr(expr).kind else {
        return false;
    };
    is_module_call(arena, callee, "module", "load")
        || is_module_call(arena, callee, "json", "decode")
        || is_module_call(arena, callee, "json", "read")
}

pub(super) fn expr_references_name(arena: &AstArena, expr: ExprId, name: Name) -> bool {
    let refs = |id: ExprId| expr_references_name(arena, id, name);
    match arena.expr(expr).kind {
        ArenaExprKind::ValuePipelineCall { input, call, .. } => refs(input) || refs(call),

        ArenaExprKind::Ident(candidate) => candidate == name,
        ArenaExprKind::List(items) | ArenaExprKind::Set(items) => {
            arena.list_element_exprs(items).any(refs)
        }
        ArenaExprKind::ListComp { expr, qualifiers }
        | ArenaExprKind::SetComp { expr, qualifiers } => {
            refs(expr)
                || arena
                    .comp_qualifiers(qualifiers)
                    .iter()
                    .any(|q| refs(q.expr()))
        }
        ArenaExprKind::MapComp {
            key,
            value,
            qualifiers,
        } => {
            refs(key)
                || refs(value)
                || arena
                    .comp_qualifiers(qualifiers)
                    .iter()
                    .any(|q| refs(q.expr()))
        }
        ArenaExprKind::Record(fields) => {
            arena
                .record_fields(fields)
                .iter()
                .any(|field| match field.kind {
                    ArenaRecordFieldKind::Computed { key, value, .. } => refs(key) || refs(value),
                    ArenaRecordFieldKind::Named { value, .. }
                    | ArenaRecordFieldKind::Path { value, .. } => refs(value),
                    ArenaRecordFieldKind::Spread { expr, .. } => refs(expr),
                    ArenaRecordFieldKind::Shorthand { name: field, .. } => field == name,
                })
        }
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            arena.fmt_parts(parts).any(|part| match part {
                ArenaFmtPart::Expr(expr, _) => refs(expr),
                ArenaFmtPart::Text(_) => false,
            })
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            arena
                .if_expr_branches(branches)
                .iter()
                .any(|branch| refs(branch.condition) || refs(branch.value))
                || refs(else_value)
        }
        ArenaExprKind::Match { value, arms }
        | ArenaExprKind::PatternTest { value, arms }
        | ArenaExprKind::PatternCondition { value, arms } => {
            refs(value)
                || arena
                    .match_expr_arms(arms)
                    .iter()
                    .any(|arm| arm.guard.is_some_and(refs) || refs(arm.value))
        }
        ArenaExprKind::Unary { expr, .. }
        | ArenaExprKind::Try(expr)
        | ArenaExprKind::Require { value: expr, .. }
        | ArenaExprKind::Convert { value: expr, .. } => refs(expr),
        ArenaExprKind::ComparisonChain(pairs) => arena.comparison_chain_operands(pairs).any(refs),
        ArenaExprKind::Binary { left, right, .. } => refs(left) || refs(right),
        ArenaExprKind::Call { callee, args } => {
            refs(callee)
                || arena.call_args(args).iter().any(|arg| match arg.kind {
                    ArenaCallArgKind::Positional(expr)
                    | ArenaCallArgKind::Named { value: expr, .. }
                    | ArenaCallArgKind::Splice { value: expr, .. }
                    | ArenaCallArgKind::NamedSpread { value: expr, .. } => refs(expr),
                })
        }
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => refs(base),
        ArenaExprKind::Index { base, index, .. } => refs(base) || refs(index),
        ArenaExprKind::Slice {
            base, start, end, ..
        } => refs(base) || start.is_some_and(refs) || end.is_some_and(refs),
        ArenaExprKind::Pipeline { input, stages } => {
            refs(input)
                || arena
                    .pipe_stages(stages)
                    .to_vec()
                    .iter()
                    .any(|stage| match stage.kind {
                        ArenaPipeStageKind::Expr(expr) => refs(expr),
                        ArenaPipeStageKind::Stream(ref stage) => {
                            stream_stage_references_name(arena, stage, name)
                        }
                    })
        }
        ArenaExprKind::StructuredPipeline { input, stages } => {
            refs(input)
                || arena
                    .stream_stages(stages)
                    .to_vec()
                    .iter()
                    .any(|stage| stream_stage_references_name(arena, stage, name))
        }
        ArenaExprKind::Run(run) => arena
            .run_segments(arena.run_form(run).segments)
            .to_vec()
            .iter()
            .any(|segment| {
                arena
                    .command_args(segment.args)
                    .iter()
                    .any(|arg| command_arg_references_name(arena, arg, name))
                    || command_arg_references_name(arena, &segment.target, name)
            }),
        ArenaExprKind::Spawn(form) => match form.target {
            ArenaSpawnTarget::Run(run) => arena
                .run_segments(arena.run_form(run).segments)
                .to_vec()
                .iter()
                .any(|segment| {
                    command_arg_references_name(arena, &segment.target, name)
                        || arena
                            .command_args(segment.args)
                            .iter()
                            .any(|arg| command_arg_references_name(arena, arg, name))
                }),
            ArenaSpawnTarget::Command(expr) => refs(expr),
        },
        ArenaExprKind::Wait(form) => refs(form.target),
        ArenaExprKind::BuilderCall { call, block } => {
            refs(call)
                || arena
                    .builder_entries(arena.builder_block(block).entries)
                    .to_vec()
                    .iter()
                    .any(|entry| match entry.kind {
                        ArenaBuilderEntryKind::Field { value, .. } => refs(value),
                        ArenaBuilderEntryKind::Entry { args, block, .. } => {
                            arena
                                .command_args(args)
                                .iter()
                                .any(|arg| command_arg_references_name(arena, arg, name))
                                || block.is_some_and(|block| {
                                    arena
                                        .builder_entries(arena.builder_block(block).entries)
                                        .to_vec()
                                        .iter()
                                        .any(|entry| match entry.kind {
                                            ArenaBuilderEntryKind::Field { value, .. } => {
                                                refs(value)
                                            }
                                            _ => false,
                                        })
                                })
                        }
                        ArenaBuilderEntryKind::Task { .. } | ArenaBuilderEntryKind::Stmt(_) => {
                            false
                        }
                    })
        }
        ArenaExprKind::ValueBlock(_)
        | ArenaExprKind::Collect { .. }
        | ArenaExprKind::ErrorContext { .. }
        | ArenaExprKind::ContextScope { .. }
        | ArenaExprKind::TempDirScope { .. }
        | ArenaExprKind::ResourceScope { .. } => true,
        ArenaExprKind::Capture(_) | ArenaExprKind::Loop { .. } | ArenaExprKind::Retry { .. } => {
            false
        }
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Regex(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::EnvString(_)
        | ArenaExprKind::EnvPathList => false,
    }
}

pub(super) fn stream_stage_references_name(arena: &AstArena, stage: &ArenaStreamStage, name: Name) -> bool {
    arena
        .call_args(stage.args)
        .iter()
        .any(|arg| match arg.kind {
            ArenaCallArgKind::Positional(expr)
            | ArenaCallArgKind::Named { value: expr, .. }
            | ArenaCallArgKind::Splice { value: expr, .. }
            | ArenaCallArgKind::NamedSpread { value: expr, .. } => {
                expr_references_name(arena, expr, name)
            }
        })
        || stage.block.is_some_and(|block| {
            arena
                .stmt_ids(arena.block(block).statements)
                .any(|stmt| match arena.stmt(stmt).kind {
                    ArenaStmtKind::Expr(expr) => expr_references_name(arena, expr, name),
                    ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr))) => {
                        expr_references_name(arena, expr, name)
                    }
                    _ => false,
                })
        })
}

pub(super) fn command_arg_references_name(arena: &AstArena, arg: &ArenaCommandArg, name: Name) -> bool {
    match arg.kind {
        ArenaCommandArgKind::SpliceName(candidate) => candidate == name,
        ArenaCommandArgKind::SpliceExpr(expr) | ArenaCommandArgKind::Typed(expr) => {
            expr_references_name(arena, expr, name)
        }
        ArenaCommandArgKind::Word(parts) => arena.word_parts(parts).any(|part| match part {
            ArenaWordPart::Interpolation(expr) | ArenaWordPart::Shorthand(expr) => {
                expr_references_name(arena, expr, name)
            }
            ArenaWordPart::Bare(_) | ArenaWordPart::Quoted(_) => false,
        }),
    }
}

pub(super) fn is_safe_const_expr(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::ValuePipelineCall { .. } => false,

        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Regex(_) => true,
        ArenaExprKind::List(items) | ArenaExprKind::Set(items) => arena
            .list_element_exprs(items)
            .all(|item| is_safe_const_expr(arena, item)),
        ArenaExprKind::Record(fields) => {
            arena
                .record_fields(fields)
                .iter()
                .all(|field| match field.kind {
                    ArenaRecordFieldKind::Computed { .. } => false,
                    ArenaRecordFieldKind::Named { value, .. }
                    | ArenaRecordFieldKind::Path { value, .. } => is_safe_const_expr(arena, value),
                    ArenaRecordFieldKind::Shorthand { .. }
                    | ArenaRecordFieldKind::Spread { .. } => false,
                })
        }
        ArenaExprKind::Unary { expr, .. } => is_safe_const_expr(arena, expr),
        ArenaExprKind::ComparisonChain(pairs) => arena
            .comparison_chain_operands(pairs)
            .all(|operand| is_safe_const_expr(arena, operand)),
        ArenaExprKind::Binary { left, right, .. } => {
            is_safe_const_expr(arena, left) && is_safe_const_expr(arena, right)
        }
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => arena
            .fmt_parts(parts)
            .all(|part| matches!(part, ArenaFmtPart::Text(_))),
        ArenaExprKind::Ident(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::ListComp { .. }
        | ArenaExprKind::SetComp { .. }
        | ArenaExprKind::MapComp { .. }
        | ArenaExprKind::If { .. }
        | ArenaExprKind::Match { .. }
        | ArenaExprKind::PatternTest { .. }
        | ArenaExprKind::PatternCondition { .. }
        | ArenaExprKind::Call { .. }
        | ArenaExprKind::Field { .. }
        | ArenaExprKind::NullSafeField { .. }
        | ArenaExprKind::Index { .. }
        | ArenaExprKind::Slice { .. }
        | ArenaExprKind::EnvString(_)
        | ArenaExprKind::EnvPathList
        | ArenaExprKind::Pipeline { .. }
        | ArenaExprKind::StructuredPipeline { .. }
        | ArenaExprKind::Run(_)
        | ArenaExprKind::Spawn(_)
        | ArenaExprKind::Wait(_)
        | ArenaExprKind::BuilderCall { .. }
        | ArenaExprKind::Try(_)
        | ArenaExprKind::Require { .. }
        | ArenaExprKind::Convert { .. }
        | ArenaExprKind::Capture(_)
        | ArenaExprKind::ValueBlock(_)
        | ArenaExprKind::ErrorContext { .. }
        | ArenaExprKind::ContextScope { .. }
        | ArenaExprKind::TempDirScope { .. }
        | ArenaExprKind::ResourceScope { .. }
        | ArenaExprKind::Loop { .. }
        | ArenaExprKind::Collect { .. }
        | ArenaExprKind::Retry { .. } => false,
    }
}

pub(super) fn expr_may_have_effects(arena: &AstArena, expr: ExprId) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::ValuePipelineCall { input, call, .. } => expr_may_have_effects(arena, input) || expr_may_have_effects(arena, call),

        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_) | ArenaExprKind::Regex(_)
        | ArenaExprKind::Ident(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::EnvPathList => false,
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => arena
            .fmt_parts(parts)
            .any(|part| matches!(part, ArenaFmtPart::Expr(expr, _) if expr_may_have_effects(arena, expr))),
        ArenaExprKind::List(items) | ArenaExprKind::Set(items) => arena.list_element_exprs(items).any(|item| expr_may_have_effects(arena, item)),
        ArenaExprKind::Record(fields) => arena.record_fields(fields).iter().any(|field| match field.kind {
            ArenaRecordFieldKind::Computed { key, value, .. } => expr_may_have_effects(arena, key) || expr_may_have_effects(arena, value),
            ArenaRecordFieldKind::Named { value, .. } | ArenaRecordFieldKind::Path { value, .. } => expr_may_have_effects(arena, value),
            ArenaRecordFieldKind::Spread { expr, .. } => expr_may_have_effects(arena, expr),
            ArenaRecordFieldKind::Shorthand { .. } => false,
        }),
        ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => {
            expr_may_have_effects(arena, expr)
        }
        ArenaExprKind::ComparisonChain(pairs) => arena.comparison_chain_operands(pairs).any(|operand| expr_may_have_effects(arena, operand)),
        ArenaExprKind::Binary { left, right, .. } => {
            expr_may_have_effects(arena, left) || expr_may_have_effects(arena, right)
        }
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
            expr_may_have_effects(arena, base)
        }
        ArenaExprKind::Index { base, index, .. } => {
            expr_may_have_effects(arena, base) || expr_may_have_effects(arena, index)
        }
        ArenaExprKind::Slice { base, start, end, .. } => {
            expr_may_have_effects(arena, base)
                || start.is_some_and(|start| expr_may_have_effects(arena, start))
                || end.is_some_and(|end| expr_may_have_effects(arena, end))
        }
        ArenaExprKind::Call { callee, args } => {
            !pure_method_call_for_prefer_in(arena, callee)
                || arena
                    .call_args(args)
                    .iter()
                    .any(|arg| call_arg_may_have_effects(arena, arg))
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            arena.if_expr_branches(branches).iter().any(|branch| {
                expr_may_have_effects(arena, branch.condition)
                    || expr_may_have_effects(arena, branch.value)
            }) || expr_may_have_effects(arena, else_value)
        }
        ArenaExprKind::Match { value, arms } | ArenaExprKind::PatternTest { value, arms } | ArenaExprKind::PatternCondition { value, arms } => {
            expr_may_have_effects(arena, value)
                || arena.match_expr_arms(arms).iter().any(|arm| {
                    arm.guard
                        .is_some_and(|guard| expr_may_have_effects(arena, guard))
                        || expr_may_have_effects(arena, arm.value)
                })
        }
        ArenaExprKind::ListComp { .. }
        | ArenaExprKind::SetComp { .. }
        | ArenaExprKind::MapComp { .. }
        | ArenaExprKind::EnvString(_)
        | ArenaExprKind::Pipeline { .. }
        | ArenaExprKind::StructuredPipeline { .. }
        | ArenaExprKind::Run(_)
        | ArenaExprKind::Spawn(_)
        | ArenaExprKind::Wait(_)
        | ArenaExprKind::BuilderCall { .. }
        | ArenaExprKind::Require { .. }
        | ArenaExprKind::Convert { .. }
        | ArenaExprKind::Capture(_)
        | ArenaExprKind::ValueBlock(_)
        | ArenaExprKind::ErrorContext { .. }
        | ArenaExprKind::ContextScope { .. }
        | ArenaExprKind::TempDirScope { .. }
        | ArenaExprKind::ResourceScope { .. }
        | ArenaExprKind::Loop { .. }
        | ArenaExprKind::Collect { .. }
        | ArenaExprKind::Retry { .. } => true,
    }
}

pub(super) fn call_arg_may_have_effects(arena: &AstArena, arg: &ArenaCallArg) -> bool {
    match arg.kind {
        ArenaCallArgKind::Positional(expr)
        | ArenaCallArgKind::Named { value: expr, .. }
        | ArenaCallArgKind::Splice { value: expr, .. }
        | ArenaCallArgKind::NamedSpread { value: expr, .. } => expr_may_have_effects(arena, expr),
    }
}

pub(super) fn pure_method_call_for_prefer_in(arena: &AstArena, callee: ExprId) -> bool {
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return false;
    };
    matches!(
        name.as_str().as_str(),
        "display"
            | "name"
            | "stem"
            | "ext"
            | "parent"
            | "join"
            | "len"
            | "lower"
            | "upper"
            | "trim"
            | "starts_with"
            | "ends_with"
            | "split"
            | "fields"
            | "words"
            | "replace"
    ) && !expr_may_have_effects(arena, base)
}

pub(super) fn migration_inert(arena: &AstArena, expr: ExprId) -> bool {
    migration_literal(arena, expr)
        || matches!(
            arena.expr(expr).kind,
            ArenaExprKind::Ident(_) | ArenaExprKind::Item
        )
}

pub(super) fn migration_literal(arena: &AstArena, expr: ExprId) -> bool {
    matches!(
        arena.expr(expr).kind,
        ArenaExprKind::Null
            | ArenaExprKind::Bool(_)
            | ArenaExprKind::Int(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::Str(_)
            | ArenaExprKind::Bytes(_)
            | ArenaExprKind::PathStr(_)
    )
}

pub(super) fn migration_reorder_safe(arena: &AstArena, left: ExprId, right: ExprId) -> bool {
    migration_literal(arena, left)
        || migration_literal(arena, right)
        || (migration_inert(arena, left) && migration_inert(arena, right))
}

pub(super) fn migration_arguments_optional(
    arena: &AstArena,
    args: ArenaRange,
    params: &[&str],
) -> Option<Vec<Option<ExprId>>> {
    let mut result = vec![None; params.len()];
    for (position, arg) in arena.call_args(args).iter().enumerate() {
        let (position, expr) = match arg.kind {
            ArenaCallArgKind::Positional(expr) => (position, expr),
            ArenaCallArgKind::Named { name, value, .. } => {
                (params.iter().position(|param| name == *param)?, value)
            }
            _ => return None,
        };
        if position >= result.len() || result[position].is_some() {
            return None;
        }
        result[position] = Some(expr);
    }
    Some(result)
}

pub(super) fn migration_arguments(arena: &AstArena, args: ArenaRange, params: &[&str]) -> Option<Vec<ExprId>> {
    migration_arguments_optional(arena, args, params)?
        .into_iter()
        .collect()
}

pub(super) fn same_ordering_operand(arena: &AstArena, left: ExprId, right: ExprId) -> bool {
    match (arena.expr(left).kind, arena.expr(right).kind) {
        (ArenaExprKind::Ident(left), ArenaExprKind::Ident(right)) => left == right,
        (ArenaExprKind::Int(left), ArenaExprKind::Int(right)) => arena
            .int_literal(left)
            .value()
            .zip(arena.int_literal(right).value())
            .is_some_and(|(left, right)| left == right),
        (ArenaExprKind::Float(left), ArenaExprKind::Float(right)) => arena
            .float_literal(left)
            .value()
            .zip(arena.float_literal(right).value())
            .is_some_and(|(left, right)| left.to_bits() == right.to_bits()),
        (ArenaExprKind::Str(left), ArenaExprKind::Str(right)) => {
            arena.string_literal(left) == arena.string_literal(right)
        }
        _ => false,
    }
}

pub(super) fn inert_constant_initializer(arena: &AstArena, value: ExprId) -> bool {
    match arena.expr(value).kind {
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Regex(_) => true,
        ArenaExprKind::List(items) | ArenaExprKind::Set(items) => {
            arena.list_elements(items).all(|item| {
                item.splice_span.is_none() && inert_constant_initializer(arena, item.value)
            })
        }
        ArenaExprKind::Record(fields) => {
            arena
                .record_fields(fields)
                .iter()
                .all(|field| match field.kind {
                    ArenaRecordFieldKind::Named { value, .. } => {
                        inert_constant_initializer(arena, value)
                    }
                    _ => false,
                })
        }
        _ => false,
    }
}

impl<'a> Linter<'a> {
    pub(super) fn pipeline_argument_stable(&self, value: ExprId) -> bool {
        match self.arena.expr(value).kind {
            ArenaExprKind::Ident(name) => {
                !self.assigned_names.contains(&name)
                    && self
                        .scopes
                        .iter()
                        .rev()
                        .find_map(|scope| scope.get(name.as_str().as_str()))
                        .is_some_and(|binding| !binding.mutable)
            }
            ArenaExprKind::Null
            | ArenaExprKind::Bool(_)
            | ArenaExprKind::Int(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::Str(_)
            | ArenaExprKind::PathStr(_)
            | ArenaExprKind::Bytes(_)
            | ArenaExprKind::Regex(_) => true,
            _ => false,
        }
    }

    pub(super) fn pipeline_ordinary_call(&self, value: ExprId) -> Option<(ExprId, ArenaRange)> {
        let call = match self.arena.expr(value).kind {
            ArenaExprKind::Try(inner) => inner,
            _ => value,
        };
        let ArenaExprKind::Call { callee, args } = self.arena.expr(call).kind else {
            return None;
        };
        // A guarded receiver could skip the original input argument entirely.
        // Direct callable names have no receiver evaluation to move across.
        if !matches!(self.arena.expr(callee).kind, ArenaExprKind::Ident(_)) {
            return None;
        }
        if self
            .arena
            .call_args(args)
            .iter()
            .any(|arg| pipeline_argument_expr(arg).is_none())
        {
            return None;
        }
        Some((callee, args))
    }

    pub(super) fn lint_optional_postfix(&mut self, expr: ExprId) {
        let expression = self.arena.expr(expr);
        let ArenaExprKind::If {
            branches,
            else_value,
        } = expression.kind
        else {
            return;
        };
        let [branch] = self.arena.if_expr_branches(branches) else {
            return;
        };
        let ArenaExprKind::Binary { op, left, right } = self.arena.expr(branch.condition).kind
        else {
            return;
        };
        if !matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            return;
        }
        let receiver = if matches!(self.arena.expr(right).kind, ArenaExprKind::Null) {
            left
        } else if matches!(self.arena.expr(left).kind, ArenaExprKind::Null) {
            right
        } else {
            return;
        };
        let ArenaExprKind::Ident(name) = self.arena.expr(receiver).kind else {
            return;
        };
        if self.assigned_names.contains(&name)
            || !matches!(
                self.expr_types.get(&self.arena.expr(receiver).span),
                Some(Type::Optional(_))
            )
        {
            return;
        }
        let (present, absent) = if op == BinaryOp::Eq {
            (else_value, branch.value)
        } else {
            (branch.value, else_value)
        };
        let single_value = |value| {
            if let ArenaExprKind::ValueBlock(block) = self.arena.expr(value).kind {
                let mut statements = self.arena.stmt_ids(self.arena.block(block).statements);
                let statement = statements.next()?;
                if statements.next().is_some() {
                    return None;
                }
                match self.arena.stmt(statement).kind {
                    ArenaStmtKind::Expr(value) => Some(value),
                    _ => None,
                }
            } else {
                Some(value)
            }
        };
        let Some(present) = single_value(present) else {
            return;
        };
        let Some(absent) = single_value(absent) else {
            return;
        };
        let present_expr = self.arena.expr(present);
        let Some(present_ty) = self.expr_types.get(&present_expr.span) else {
            return;
        };
        let absent_is_null = matches!(self.arena.expr(absent).kind, ArenaExprKind::Null);
        if matches!(
            present_ty,
            Type::Null | Type::Any | Type::Unknown | Type::Invalid
        ) || (!absent_is_null
            && (matches!(present_ty, Type::Optional(_))
                || self.expr_types.get(&self.arena.expr(absent).span) != Some(present_ty)))
        {
            return;
        }
        let (base, insertion) = match present_expr.kind {
            ArenaExprKind::Field { base, .. } => (base, self.arena.expr(base).span.end()),
            ArenaExprKind::Call { callee, .. } => match self.arena.expr(callee).kind {
                ArenaExprKind::Field { base, .. } => (base, self.arena.expr(base).span.end()),
                _ => return,
            },
            ArenaExprKind::Index {
                base,
                guarded: false,
                ..
            }
            | ArenaExprKind::Slice {
                base,
                guarded: false,
                ..
            } => (base, self.arena.expr(base).span.end()),
            _ => return,
        };
        if !matches!(self.arena.expr(base).kind, ArenaExprKind::Ident(found) if found == name) {
            return;
        }
        let Some(original) = self
            .source
            .get(expression.span.start()..expression.span.end())
        else {
            return;
        };
        if original.contains('#') {
            return;
        }
        let Some(before) = self.source.get(present_expr.span.start()..insertion) else {
            return;
        };
        let Some(after) = self.source.get(insertion..present_expr.span.end()) else {
            return;
        };
        let absent_span = self.arena.expr(absent).span;
        let Some(fallback) = self.source.get(absent_span.start()..absent_span.end()) else {
            return;
        };
        let replacement = if absent_is_null {
            format!("({before}?{after})")
        } else {
            format!("({before}?{after} ?? ({fallback}))")
        };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "explicit null branch can use a guarded postfix",
            )
            .with_code(DiagnosticCode::LintPreferOptionalPostfix)
            .with_label(Label::secondary(
                expression.span,
                "guard the receiver and retain the lazy fallback",
            ))
            .with_fix_hint(FixHint::replacement(
                expression.span,
                "use guarded postfix and fallback",
                replacement,
            )),
        );
    }

    pub(super) fn lint_proven_nonnull_fallback(&mut self, expr: ExprId) {
        let expression = self.arena.expr(expr);
        let ArenaExprKind::Binary {
            op: BinaryOp::ResultFallback,
            left,
            right,
        } = expression.kind
        else {
            return;
        };
        if !self
            .proven_nonnull_fallback_receivers
            .contains(&self.arena.expr(left).span)
            || !inert_constant_initializer(self.arena, right)
            || matches!(self.arena.expr(right).kind, ArenaExprKind::Regex(_))
            || self
                .source
                .get(expression.span.range())
                .is_none_or(|source| source.contains('#'))
        {
            return;
        }
        let Some(expected) = self.expr_types.get(&self.arena.expr(left).span) else {
            return;
        };
        let Some(value) = xsh::frontend::check::LiteralConstant::analyze(
            self.arena,
            right,
            &FxHashMap::default(),
        ) else {
            return;
        };
        if !value.matches_data_type(expected) {
            return;
        }
        let Some(replacement) = self.source.get(self.arena.expr(left).span.range()) else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "this Optional receiver is proved present",
            )
            .with_code(DiagnosticCode::LintRedundantOptionalFallback)
            .with_label(Label::secondary(
                expression.span,
                "the fallback cannot be reached",
            ))
            .with_fix_hint(FixHint::replacement(
                expression.span,
                "use the proved present value",
                replacement,
            )),
        );
    }

    pub(super) fn proven_absence_lookup(&self, expr: ExprId) -> bool {
        let node = self.arena.expr(expr);
        if self.expr_types.get(&node.span) != Some(&Type::Optional(Box::new(Type::Int))) {
            return false;
        }
        match node.kind {
            ArenaExprKind::Ident(name) => {
                self.scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(name.as_str().as_str()))
                    .is_some_and(|binding| binding.absence_lookup && !binding.mutable)
                    && !self.assigned_names.contains(&name)
            }
            ArenaExprKind::Call { callee, .. } => {
                let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
                    return false;
                };
                match self.expr_types.get(&self.arena.expr(base).span) {
                    Some(Type::Str) => matches!(name.as_str().as_str(), "find" | "byte_at"),
                    Some(Type::Bytes) => name == "byte_at",
                    _ => false,
                }
            }
            _ => false,
        }
    }

    pub(super) fn is_negative_one_literal(&self, expr: ExprId) -> bool {
        let ArenaExprKind::Unary {
            op: UnaryOp::Neg,
            expr,
        } = self.arena.expr(expr).kind
        else {
            return false;
        };
        matches!(self.arena.expr(expr).kind, ArenaExprKind::Int(value) if self.arena.int_literal(value).value() == Some(1))
    }

    // Host-backed records may materialize other metadata during `get`. Only
    // locally constructed ordinary records and their immutable snapshots prove
    // that selecting one guaranteed field cannot drop a metadata failure.
    pub(super) fn proven_materialized_record(&self, expr: ExprId) -> bool {
        if !matches!(
            self.expr_types.get(&self.arena.expr(expr).span),
            Some(Type::Record(_))
        ) {
            return false;
        }
        match self.arena.expr(expr).kind {
            ArenaExprKind::Record(_) => true,
            ArenaExprKind::Ident(name) => {
                self.scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(name.as_str().as_str()))
                    .is_some_and(|binding| binding.materialized_record && !binding.mutable)
                    && !self.assigned_names.contains(&name)
            }
            ArenaExprKind::Call { callee, .. } => self
                .record_constructors
                .resolve_call(self.arena, callee, None)
                .is_some(),
            _ => false,
        }
    }

    pub(super) fn proven_immutable_byte_length(&self, expr: ExprId) -> Option<usize> {
        match self.arena.expr(expr).kind {
            ArenaExprKind::Bytes(bytes) => Some(self.arena.bytes_literal(bytes).len()),
            ArenaExprKind::Ident(name) if !self.assigned_names.contains(&name) => {
                let binding = self
                    .scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(name.as_str().as_str()))?;
                (!binding.mutable)
                    .then_some(binding.immutable_byte_length)
                    .flatten()
            }
            _ => None,
        }
    }
}
