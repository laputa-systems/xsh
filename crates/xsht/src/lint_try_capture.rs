use std::collections::{BTreeMap, BTreeSet};
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::{CheckBundleView, CheckOptions, CheckOutput, Type};
use xsh::frontend::source::Span;
use xsh::frontend::symbols::Name;
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaCallArgKind, ArenaChild, ArenaExprKind, ArenaExprOrRun,
    ArenaProgram, ArenaStmtKind, ExprId,
};
use xsh::frontend::syntax::node::BinaryOp;

/// Eliminate a closed, straight-line fallible helper at its sole eager fallback
/// use. Restricting the body to immutable locals and scalar methods excludes
/// captures, callable defaults, resource escape, and lexical control transfers.
pub(super) fn lint_try_capture_helpers(
    view: &CheckBundleView<'_>,
    source: &str,
    check_original: Option<std::sync::Arc<CheckOutput>>,
) -> Vec<Diagnostic> {
    let program = view.program();
    let Some(before) = check_original else {
        return Vec::new();
    };
    let statements = view.statement_ids().collect::<Vec<_>>();
    let mut diagnostics = Vec::new();
    for pair in statements.windows(2) {
        let definition = program.arena.stmt(pair[0]);
        let ArenaStmtKind::ProcDef(id) = definition.kind else {
            continue;
        };
        let helper = program.arena.function_def(id);
        if helper.test_declaration
            || helper.name == "main"
            || !helper.params.is_empty()
            || helper.return_ty_defaulted
        {
            continue;
        }
        let use_statement = program.arena.stmt(pair[1]);
        let ArenaStmtKind::Let {
            initializer: ArenaExprOrRun::Expr(initializer),
            ..
        } = use_statement.kind
        else {
            continue;
        };
        let ArenaExprKind::Binary {
            op: BinaryOp::ResultFallback,
            left: call,
            right: fallback,
        } = program.arena.expr(initializer).kind
        else {
            continue;
        };
        let ArenaExprKind::Call { callee, args } = program.arena.expr(call).kind else {
            continue;
        };
        if !args.is_empty()
            || !matches!(program.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == helper.name)
        {
            continue;
        }
        if !matches!(
            program.arena.expr(fallback).kind,
            ArenaExprKind::Int(_)
                | ArenaExprKind::Str(_)
                | ArenaExprKind::Bool(_)
                | ArenaExprKind::Float(_)
                | ArenaExprKind::Duration(_)
        ) {
            continue;
        }
        let Some(definition_text) = source.get(definition.span.range()) else {
            continue;
        };
        let preceding_comment = source
            .get(..definition.span.start())
            .and_then(|text| text.trim_end().lines().last())
            .is_some_and(|line| line.trim_start().starts_with('#'));
        if definition_text.contains('#')
            || preceding_comment
            || name_occurrences(source, &helper.name.as_str()) != 2
        {
            continue;
        }
        if !before.diagnostics.is_empty() {
            return Vec::new();
        }
        let call_span = program.arena.expr(call).span;
        let Some(Type::Result(ok, error)) = before.expr_types.get(&call_span) else {
            continue;
        };
        if !matches!(
            ok.as_ref(),
            Type::Int | Type::Str | Type::Bool | Type::Float | Type::Duration
        ) || **error != Type::Error
        {
            continue;
        }
        let body = program.arena.block(helper.body);
        let body_span = program.arena.span(body.span);
        if !before.function_effect_facts.iter().any(|(id, fact)| {
            id.body == body_span
                && fact
                    .required
                    .as_ref()
                    .zip(fact.effective.as_ref())
                    .is_some_and(|(required, effective)| {
                        required.len() == effective.len()
                            && required.iter().all(|effect| effective.contains(effect))
                    })
        }) {
            continue;
        }
        let Some(body_text) = source.get(body_span.range()) else {
            continue;
        };
        let mut locals = BTreeSet::new();
        let mut propagated = false;
        let mut remaining = 128;
        let body_statements = program.arena.stmt_ids(body.statements).collect::<Vec<_>>();
        if body_statements.is_empty()
            || !body_statements
                .iter()
                .enumerate()
                .all(|(index, statement)| {
                    let statement = program.arena.stmt(*statement);
                    match statement.kind {
                        ArenaStmtKind::Let {
                            target,
                            ty: None,
                            initializer: ArenaExprOrRun::Expr(value),
                        } if index + 1 < body_statements.len() => {
                            let ArenaBindingTargetKind::Name(name) =
                                program.arena.binding_target(target).kind
                            else {
                                return false;
                            };
                            if name == "_"
                                || !closed_scalar_expression(
                                    program,
                                    &before,
                                    value,
                                    &locals,
                                    &mut propagated,
                                    &mut remaining,
                                )
                            {
                                return false;
                            }
                            locals.insert(name)
                        }
                        ArenaStmtKind::Expr(value) if index + 1 == body_statements.len() => {
                            closed_scalar_expression(
                                program,
                                &before,
                                value,
                                &locals,
                                &mut propagated,
                                &mut remaining,
                            )
                        }
                        _ => false,
                    }
                })
            || !propagated
        {
            continue;
        }
        let replacement = format!("try {body_text}");
        let mut rewritten = source.to_owned();
        rewritten.replace_range(call_span.range(), &replacement);
        rewritten.replace_range(definition.span.range(), "");
        let snapshot = super::probes::LintProbeTransport::standalone(
            definition.span.source_id,
            &rewritten,
            CheckOptions::default(),
        );
        if !snapshot.parse_diagnostics.is_empty() {
            continue;
        }
        let Some(after) = snapshot.checked.as_ref() else {
            continue;
        };
        if !after.diagnostics.is_empty() {
            continue;
        }
        let edits = CaptureEdits {
            definition: definition.span,
            call: call_span,
            body: body_span,
            replacement_len: replacement.len(),
        };
        if !same_checked_facts(program, &snapshot.program, &before, after, edits) {
            continue;
        }
        diagnostics.push(Diagnostic::warning("use a local Result capture for this single-use closed helper")
            .with_code(DiagnosticCode::LintPreferTryCapture)
            .with_label(Label::secondary(definition.span, "the body stays at its eager fallback use; the helper trace and traceback frame are removed"))
            .with_fix_hint(FixHint::replacement(call_span, "capture the fallible body locally", replacement))
            .with_fix_hint(FixHint::replacement(definition.span, "remove the private helper with no remaining references", String::new())));
    }
    diagnostics
}

fn name_occurrences(source: &str, name: &str) -> usize {
    let part =
        |character: char| character.is_alphanumeric() || character == '_' || character == '-';
    source
        .match_indices(name)
        .filter(|(index, _)| {
            !source[..*index].chars().next_back().is_some_and(part)
                && !source[*index + name.len()..]
                    .chars()
                    .next()
                    .is_some_and(part)
        })
        .count()
}

fn closed_scalar_expression(
    program: &ArenaProgram,
    checked: &CheckOutput,
    value: ExprId,
    locals: &BTreeSet<Name>,
    propagated: &mut bool,
    remaining: &mut usize,
) -> bool {
    let mut pending = vec![value];
    while let Some(expr) = pending.pop() {
        if *remaining == 0 {
            return false;
        }
        *remaining -= 1;
        let kind = program.arena.expr(expr).kind;
        let children = match kind {
            ArenaExprKind::Int(_)
            | ArenaExprKind::Str(_)
            | ArenaExprKind::Bool(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::PathStr(_) => continue,
            ArenaExprKind::Ident(name) if locals.contains(&name) => continue,
            ArenaExprKind::Try(_) => {
                let mut children = Vec::new();
                let mut unsupported_child = false;
                program.arena.for_each_expr_child(expr, |child| match child {
                    ArenaChild::Expr(child) => children.push(child),
                    ArenaChild::Stmt(_)
                    | ArenaChild::Pattern(_)
                    | ArenaChild::Block(_)
                    | ArenaChild::BindingTarget(_)
                    | ArenaChild::AssignTarget(_)
                    | ArenaChild::TypeExpr(_)
                    | ArenaChild::BuilderBlock(_) => unsupported_child = true,
                });
                if unsupported_child || children.len() != 1 {
                    return false;
                }
                *propagated = true;
                children
            }
            ArenaExprKind::Call { callee, args } => {
                let ArenaExprKind::Field { base, name } = program.arena.expr(callee).kind else {
                    return false;
                };
                let permitted = match checked.expr_types.get(&program.arena.expr(base).span) {
                    Some(Type::Path) => name == "read_text",
                    Some(Type::Str) => matches!(name.as_str().as_ref(), "trim" | "parse_int"),
                    _ => false,
                };
                if !permitted {
                    return false;
                }
                let Some(argument_values) = program
                    .arena
                    .call_args(args)
                    .iter()
                    .map(|argument| match argument.kind {
                        ArenaCallArgKind::Positional(value)
                        | ArenaCallArgKind::Named { value, .. } => Some(value),
                        ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. } => {
                            None
                        }
                    })
                    .collect::<Option<Vec<_>>>()
                else {
                    return false;
                };

                let mut call_children = Vec::new();
                let mut unsupported_child = false;
                program.arena.for_each_expr_child(expr, |child| match child {
                    ArenaChild::Expr(child) => call_children.push(child),
                    ArenaChild::Stmt(_)
                    | ArenaChild::Pattern(_)
                    | ArenaChild::Block(_)
                    | ArenaChild::BindingTarget(_)
                    | ArenaChild::AssignTarget(_)
                    | ArenaChild::TypeExpr(_)
                    | ArenaChild::BuilderBlock(_) => unsupported_child = true,
                });
                if unsupported_child
                    || call_children.first() != Some(&callee)
                    || call_children.get(1..) != Some(argument_values.as_slice())
                {
                    return false;
                }
                let mut receiver = Vec::new();
                let mut unsupported_receiver_child = false;
                program.arena.for_each_expr_child(callee, |child| match child {
                    ArenaChild::Expr(child) => receiver.push(child),
                    ArenaChild::Stmt(_)
                    | ArenaChild::Pattern(_)
                    | ArenaChild::Block(_)
                    | ArenaChild::BindingTarget(_)
                    | ArenaChild::AssignTarget(_)
                    | ArenaChild::TypeExpr(_)
                    | ArenaChild::BuilderBlock(_) => unsupported_receiver_child = true,
                });
                if unsupported_receiver_child || receiver.as_slice() != [base] {
                    return false;
                }
                let mut children = vec![base];
                children.extend(call_children.into_iter().skip(1));
                children
            }
            ArenaExprKind::Null
            | ArenaExprKind::GlobStr(_)
            | ArenaExprKind::FmtString(_)
            | ArenaExprKind::PathFmtString(_)
            | ArenaExprKind::Bytes(_)
            | ArenaExprKind::Regex(_)
            | ArenaExprKind::Item
            | ArenaExprKind::LastStatus
            | ArenaExprKind::List(_)
            | ArenaExprKind::ListComp { .. }
            | ArenaExprKind::MapComp { .. }
            | ArenaExprKind::Set(_)
            | ArenaExprKind::SetComp { .. }
            | ArenaExprKind::Record(_)
            | ArenaExprKind::If { .. }
            | ArenaExprKind::Match { .. }
            | ArenaExprKind::PatternTest { .. }
            | ArenaExprKind::PatternCondition { .. }
            | ArenaExprKind::Unary { .. }
            | ArenaExprKind::ComparisonChain(_)
            | ArenaExprKind::Binary { .. }
            | ArenaExprKind::ValuePipelineCall { .. }
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
            | ArenaExprKind::Capture(_)
            | ArenaExprKind::Require { .. }
            | ArenaExprKind::Convert { .. }
            | ArenaExprKind::Collect { .. }
            | ArenaExprKind::Loop { .. }
            | ArenaExprKind::Retry { .. }
            | ArenaExprKind::ValueBlock(_)
            | ArenaExprKind::ErrorContext { .. }
            | ArenaExprKind::ContextScope { .. }
            | ArenaExprKind::TempDirScope { .. }
            | ArenaExprKind::ResourceScope { .. }
            | ArenaExprKind::Ident(_) => return false,
        };
        pending.extend(children.into_iter().rev());
    }
    true
}

#[derive(Clone, Copy)]
struct CaptureEdits {
    definition: Span,
    call: Span,
    body: Span,
    replacement_len: usize,
}

impl CaptureEdits {
    fn map(self, span: Span) -> Option<Span> {
        if span.source_id != self.definition.source_id {
            return Some(span);
        }
        let removed = self.definition.range().len() as isize;
        let total = self.replacement_len as isize - self.call.range().len() as isize - removed;
        if self.body.start() <= span.start() && span.end() <= self.body.end() {
            let delta = self.call.start() as isize - removed + 4 - self.body.start() as isize;
            return Some(Span::new(
                span.source_id,
                span.start().checked_add_signed(delta)?,
                span.end().checked_add_signed(delta)?,
            ));
        }
        if self.definition.start() <= span.start() && span.end() <= self.definition.end() {
            return None;
        }
        if self.call.start() <= span.start() && span.end() <= self.call.end() {
            return (span == self.call).then(|| {
                Span::new(
                    span.source_id,
                    self.call.start() - self.definition.range().len(),
                    self.call.start() - self.definition.range().len() + self.replacement_len,
                )
            });
        }
        let map = |position: usize| {
            let delta = if position >= self.call.end() {
                total
            } else if position >= self.definition.end() {
                -removed
            } else {
                0
            };
            position.checked_add_signed(delta)
        };
        Some(Span::new(
            span.source_id,
            map(span.start())?,
            map(span.end())?,
        ))
    }
}

fn same_checked_facts(
    before_program: &ArenaProgram,
    after_program: &ArenaProgram,
    before: &CheckOutput,
    after: &CheckOutput,
    edits: CaptureEdits,
) -> bool {
    let shape = |program: &ArenaProgram, ty: &Type| {
        program
            .symbol_owner()
            .with_current(|| super::checked_return_type_shape(ty))
    };
    let source_id = edits.definition.source_id;
    let before_effects = before
        .function_effect_facts
        .iter()
        .filter(|(id, _)| id.body.source_id == source_id && id.body != edits.body)
        .filter_map(|(id, fact)| {
            Some(((id.namespace.clone(), edits.map(id.body)?), fact.effective.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let after_effects = after
        .function_effect_facts
        .iter()
        .filter(|(id, _)| id.body.source_id == source_id)
        .map(|(id, fact)| ((id.namespace.clone(), id.body), fact.effective.clone()))
        .collect::<BTreeMap<_, _>>();
    before.expr_types.iter().filter(|(span, _)| span.source_id == source_id).all(|(span, ty)| edits.map(*span).is_none_or(|mapped|
        after.expr_types.get(&mapped).is_some_and(|other| shape(before_program, ty) == shape(after_program, other))))
        // Unannotated immutable body locals take their initializer types, which
        // are compared above. The callable-local inference map is not populated
        // for the corresponding straight-line capture block.
        && before.local_binding_types.iter().filter(|(span, _)| span.source_id == source_id && !(edits.body.start() <= span.start() && span.end() <= edits.body.end())).all(|(span, ty)| edits.map(*span).is_none_or(|mapped|
            after.local_binding_types.get(&mapped).is_some_and(|other| shape(before_program, ty) == shape(after_program, other))))
        && before.statement_positions.iter().filter(|(span, _)| span.source_id == source_id).all(|(span, position)| edits.map(*span).is_none_or(|mapped| after.statement_positions.get(&mapped) == Some(position)))
        && before_effects == after_effects
}

#[cfg(test)]
#[path = "lint_try_capture_tests.rs"]
mod tests;
