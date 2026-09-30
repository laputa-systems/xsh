use std::collections::BTreeSet;
use xsh::diagnostic::{Diagnostic, FixHint, Label};
use xsh::frontend::check::{CheckOutput, Checker, Type};
use xsh::frontend::source::Span;
use xsh::frontend::symbols::Name;
use xsh::frontend::syntax::arena::{ArenaBindingTargetKind, ArenaCallArgKind, ArenaExprKind, ArenaExprOrRun, ArenaProgram, ArenaStmtKind, ExprId};
use xsh::frontend::syntax::node::BinaryOp;
use xsh::frontend::syntax::parser::Parser;

/// Eliminate a closed, straight-line fallible helper at its sole eager fallback
/// use. Restricting the body to immutable locals and scalar methods excludes
/// captures, callable defaults, resource escape, and lexical control transfers.
pub(super) fn lint_try_capture_helpers(program: &ArenaProgram, source: &str) -> Vec<Diagnostic> {
    let before = std::cell::LazyCell::new(|| Checker::check_arena(program, source));
    let statements = program.statement_ids().collect::<Vec<_>>();
    let mut diagnostics = Vec::new();
    for pair in statements.windows(2) {
        let definition = program.arena.stmt(pair[0]);
        let ArenaStmtKind::ProcDef(id) = definition.kind else { continue; };
        let helper = program.arena.function_def(id);
        if helper.test_declaration || helper.name == "main" || !helper.params.is_empty() || helper.return_ty_defaulted { continue; }
        let use_statement = program.arena.stmt(pair[1]);
        let ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(initializer), .. } = use_statement.kind else { continue; };
        let ArenaExprKind::Binary { op: BinaryOp::ResultFallback, left: call, right: fallback } = program.arena.expr(initializer).kind else { continue; };
        let ArenaExprKind::Call { callee, args } = program.arena.expr(call).kind else { continue; };
        if !args.is_empty() || !matches!(program.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == helper.name) { continue; }
        if !matches!(program.arena.expr(fallback).kind, ArenaExprKind::Int(_) | ArenaExprKind::Str(_) | ArenaExprKind::Bool(_) | ArenaExprKind::Float(_) | ArenaExprKind::Duration(_)) { continue; }
        let Some(definition_text) = source.get(definition.span.range()) else { continue; };
        let preceding_comment = source.get(..definition.span.start()).and_then(|text| text.trim_end().lines().last()).is_some_and(|line| line.trim_start().starts_with('#'));
        if definition_text.contains('#') || preceding_comment || name_occurrences(source, &helper.name.as_str()) != 2 { continue; }
        if !before.diagnostics.is_empty() { return Vec::new(); }
        let call_span = program.arena.expr(call).span;
        let Some(Type::Result(ok, error)) = before.expr_types.get(&call_span) else { continue; };
        if !matches!(ok.as_ref(), Type::Int | Type::Str | Type::Bool | Type::Float | Type::Duration) || **error != Type::Error { continue; }
        let body = program.arena.block(helper.body);
        let body_span = program.arena.span(body.span);
        if !before.function_effect_facts.iter().any(|(id, fact)| id.body == body_span
            && fact.required.as_ref().zip(fact.effective.as_ref()).is_some_and(|(required, effective)|
                required.len() == effective.len() && required.iter().all(|effect| effective.contains(effect)))) { continue; }
        let Some(body_text) = source.get(body_span.range()) else { continue; };
        let mut locals = BTreeSet::new();
        let mut propagated = false;
        let mut remaining = 128;
        let body_statements = program.arena.stmt_ids(body.statements).collect::<Vec<_>>();
        if body_statements.is_empty() || !body_statements.iter().enumerate().all(|(index, statement)| {
            let statement = program.arena.stmt(*statement);
            match statement.kind {
                ArenaStmtKind::Let { target, ty: None, initializer: ArenaExprOrRun::Expr(value) } if index + 1 < body_statements.len() => {
                    let ArenaBindingTargetKind::Name(name) = program.arena.binding_target(target).kind else { return false; };
                    if name == "_" || !closed_scalar_expression(program, &before, value, &locals, &mut propagated, &mut remaining) { return false; }
                    locals.insert(name)
                }
                ArenaStmtKind::Expr(value) if index + 1 == body_statements.len() => closed_scalar_expression(program, &before, value, &locals, &mut propagated, &mut remaining),
                _ => false,
            }
        }) || !propagated { continue; }
        let replacement = format!("try {body_text}");
        let mut rewritten = source.to_owned();
        rewritten.replace_range(call_span.range(), &replacement);
        rewritten.replace_range(definition.span.range(), "");
        let parsed = Parser::parse_source_arena_only(definition.span.source_id, &rewritten);
        if !parsed.diagnostics.is_empty() { continue; }
        let after = Checker::check_arena(&parsed.arena, &rewritten);
        if !after.diagnostics.is_empty() { continue; }
        let edits = CaptureEdits { definition: definition.span, call: call_span, body: body_span, replacement_len: replacement.len() };
        if !same_checked_facts(program, &parsed.arena, &before, &after, edits, helper.name) { continue; }
        diagnostics.push(Diagnostic::warning("use a local Result capture for this single-use closed helper")
            .with_code("lint.prefer-try-capture")
            .with_label(Label::secondary(definition.span, "the body stays at its eager fallback use; the helper trace and traceback frame are removed"))
            .with_fix_hint(FixHint::replacement(call_span, "capture the fallible body locally", replacement))
            .with_fix_hint(FixHint::replacement(definition.span, "remove the private helper with no remaining references", String::new())));
    }
    diagnostics
}

fn name_occurrences(source: &str, name: &str) -> usize {
    let part = |character: char| character.is_alphanumeric() || character == '_' || character == '-';
    source.match_indices(name).filter(|(index, _)| {
        !source[..*index].chars().next_back().is_some_and(part)
            && !source[*index + name.len()..].chars().next().is_some_and(part)
    }).count()
}

fn closed_scalar_expression(program: &ArenaProgram, checked: &CheckOutput, value: ExprId, locals: &BTreeSet<Name>, propagated: &mut bool, remaining: &mut usize) -> bool {
    if *remaining == 0 { return false; }
    *remaining -= 1;
    match program.arena.expr(value).kind {
        ArenaExprKind::Int(_) | ArenaExprKind::Str(_) | ArenaExprKind::Bool(_) | ArenaExprKind::Float(_) | ArenaExprKind::Duration(_) | ArenaExprKind::PathStr(_) => true,
        ArenaExprKind::Ident(name) => locals.contains(&name),
        ArenaExprKind::Try(inner) => {
            *propagated = true;
            closed_scalar_expression(program, checked, inner, locals, propagated, remaining)
        }
        ArenaExprKind::Call { callee, args } => {
            let ArenaExprKind::Field { base, name } = program.arena.expr(callee).kind else { return false; };
            let permitted = match checked.expr_types.get(&program.arena.expr(base).span) {
                Some(Type::Path) => name == "read_text",
                Some(Type::Str) => matches!(name.as_str().as_ref(), "trim" | "parse_int"),
                _ => false,
            };
            permitted && closed_scalar_expression(program, checked, base, locals, propagated, remaining)
                && program.arena.call_args(args).iter().all(|argument| match argument.kind {
                    ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. } => closed_scalar_expression(program, checked, value, locals, propagated, remaining),
                    _ => false,
                })
        }
        _ => false,
    }
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
        if span.source_id != self.definition.source_id { return Some(span); }
        let removed = self.definition.range().len() as isize;
        let total = self.replacement_len as isize - self.call.range().len() as isize - removed;
        if self.body.start() <= span.start() && span.end() <= self.body.end() {
            let delta = self.call.start() as isize - removed + 4 - self.body.start() as isize;
            return Some(Span::new(span.source_id, span.start().checked_add_signed(delta)?, span.end().checked_add_signed(delta)?));
        }
        if self.definition.start() <= span.start() && span.end() <= self.definition.end() { return None; }
        if self.call.start() <= span.start() && span.end() <= self.call.end() {
            return (span == self.call).then(|| Span::new(span.source_id, self.call.start() - self.definition.range().len(), self.call.start() - self.definition.range().len() + self.replacement_len));
        }
        let map = |position: usize| {
            let delta = if position >= self.call.end() { total } else if position >= self.definition.end() { -removed } else { 0 };
            position.checked_add_signed(delta)
        };
        Some(Span::new(span.source_id, map(span.start())?, map(span.end())?))
    }
}

fn same_checked_facts(before_program: &ArenaProgram, after_program: &ArenaProgram, before: &CheckOutput, after: &CheckOutput, edits: CaptureEdits, helper: Name) -> bool {
    let shape = |program: &ArenaProgram, ty: &Type| program.symbol_owner().with_current(|| super::checked_return_type_shape(ty));
    let helper_name = helper.to_string();
    before.expr_types.iter().all(|(span, ty)| edits.map(*span).is_none_or(|mapped|
        after.expr_types.get(&mapped).is_some_and(|other| shape(before_program, ty) == shape(after_program, other))))
        // Unannotated immutable body locals take their initializer types, which
        // are compared above. The callable-local inference map is not populated
        // for the corresponding straight-line capture block.
        && before.local_binding_types.iter().filter(|(span, _)| !(edits.body.start() <= span.start() && span.end() <= edits.body.end())).all(|(span, ty)| edits.map(*span).is_none_or(|mapped|
            after.local_binding_types.get(&mapped).is_some_and(|other| shape(before_program, ty) == shape(after_program, other))))
        && before.statement_positions.iter().all(|(span, position)| edits.map(*span).is_none_or(|mapped| after.statement_positions.get(&mapped) == Some(position)))
        && before.assertion_spans.iter().all(|span| edits.map(*span).is_none_or(|mapped| after.assertion_spans.contains(&mapped)))
        && after.assertion_spans.iter().all(|mapped| before.assertion_spans.iter().any(|span| edits.map(*span) == Some(*mapped)))
        && before.callable_effects.iter().filter(|(name, _)| *name != &helper_name).all(|(name, effects)| after.callable_effects.get(name) == Some(effects))
}

#[cfg(test)]
#[path = "lint_try_capture_tests.rs"]
mod tests;
