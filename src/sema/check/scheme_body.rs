use super::{BindingId, FxHashSet, ResolvedBindings};
use crate::source::Span;
use crate::syntax::arena::{ArenaCallArgKind, ArenaExprKind, ArenaProgram, ArenaStmtKind, BlockId};

#[derive(Clone, Copy)]
pub(super) enum BodyCompletion { Function, Procedure, Value, Statement, ProcedureStatement, Producer }

pub(super) fn matches_constructor_producer(
    program: &ArenaProgram,
    expression: crate::syntax::arena::ExprId,
    kind: crate::sema::schemes::NonmaterialConstructor,
) -> bool {
    match kind {
        crate::sema::schemes::NonmaterialConstructor::Record => matches!(program.arena.expr(expression).kind, ArenaExprKind::Record(_)),
        crate::sema::schemes::NonmaterialConstructor::EmptyList => matches!(program.arena.expr(expression).kind, ArenaExprKind::List(elements) if elements.is_empty()),
        crate::sema::schemes::NonmaterialConstructor::EmptyMap => matches!(program.arena.expr(expression).kind, ArenaExprKind::Call { args, .. } if args.is_empty()),
    }
}

pub(super) fn supports_generalized_body(
    program: &ArenaProgram,
    prepared: &crate::sema::constants::PreparedConstants,
    command_sites: &std::collections::BTreeMap<(Span, super::Name), BindingId>,
    body: BlockId,
    bindings: &ResolvedBindings,
    parameters: &FxHashSet<BindingId>,
    captures: &FxHashSet<BindingId>,
    tails: &std::collections::BTreeMap<crate::syntax::arena::StmtId, crate::sema::schemes::SchemeType>,
    positions: &std::collections::BTreeMap<Span, super::StatementPosition>,
    checked_calls: &FxHashSet<Span>,
    registered_calls: &std::collections::BTreeMap<crate::syntax::arena::ExprId, std::sync::Arc<crate::sema::schemes::RegisteredCallPlan>>,
    types: &std::collections::BTreeMap<crate::syntax::arena::ExprId, crate::sema::schemes::SchemeType>,
    constructor_producers: &std::collections::BTreeMap<crate::syntax::arena::ExprId, crate::sema::schemes::NonmaterialConstructor>,
    result_constructors: &std::collections::BTreeMap<Span, super::CheckedResultConstructor>,
    completion: BodyCompletion,
) -> bool {
    use crate::syntax::arena::ArenaExprOrRun;
    let nested_completion = match completion { BodyCompletion::Producer => BodyCompletion::Producer, BodyCompletion::Procedure | BodyCompletion::ProcedureStatement => BodyCompletion::ProcedureStatement, _ => BodyCompletion::Statement };
    let block = program.arena.block(body);
    if !program.arena.block_params(block.params).is_empty() { return false; }
    let statements = program.arena.stmt_ids(block.statements).collect::<Vec<_>>();
    let Some((&tail, prefix)) = statements.split_last() else { return matches!(completion, BodyCompletion::Statement | BodyCompletion::ProcedureStatement | BodyCompletion::Producer) };
    let mut available = parameters.clone();
    let prefix = if matches!(completion, BodyCompletion::Statement | BodyCompletion::ProcedureStatement | BodyCompletion::Producer) { statements.as_slice() } else { prefix };
    for &statement in prefix {
        let node = program.arena.stmt(statement);
        match node.kind {
            ArenaStmtKind::Let { target, initializer: ArenaExprOrRun::Expr(value), .. }
            | ArenaStmtKind::Var { target, initializer: ArenaExprOrRun::Expr(value), .. } => {
                if !is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, value)
                    || !declare_target_bindings(program, bindings, target, &mut available)
                { return false; }
            }
            ArenaStmtKind::Assign { target, value: ArenaExprOrRun::Expr(value), .. } => {
                let Some(name) = super::assignment_target_root_name(program, target) else { return false };
                if !bindings.value_sites.get(&(node.span, name)).is_some_and(|binding| available.contains(binding))
                    || !is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, value)
                { return false; }
                let mut selected = target;
                loop {
                    match program.arena.assign_target(selected).kind {
                        crate::syntax::arena::ArenaAssignTargetKind::Name(_) => break,
                        crate::syntax::arena::ArenaAssignTargetKind::Field { base, .. } => selected = base,
                        crate::syntax::arena::ArenaAssignTargetKind::Index { base, index } => {
                            if !is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, index) { return false; }
                            selected = base;
                        }
                        crate::syntax::arena::ArenaAssignTargetKind::Env(_) => return false,
                    }
                }
            }
            ArenaStmtKind::Expr(expression) => {
                let supported = if let ArenaExprKind::ValueBlock(block) = program.arena.expr(expression).kind {
                    supports_generalized_body(program, prepared, command_sites, block, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, nested_completion)
                } else { is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, expression) };
                if !supported { return false; }
            }
            ArenaStmtKind::Yield(ArenaExprOrRun::Expr(value)) | ArenaStmtKind::YieldDelegate(value) if matches!(completion, BodyCompletion::Producer) => {
                if !is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, value) { return false; }
            }
            ArenaStmtKind::Return(None) if matches!(completion, BodyCompletion::Producer) => {}
            ArenaStmtKind::Defer(ArenaExprOrRun::Expr(value), _) if matches!(completion, BodyCompletion::Producer) => {
                let supported = if let ArenaExprKind::ValueBlock(block) = program.arena.expr(value).kind {
                    supports_generalized_body(program, prepared, command_sites, block, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, BodyCompletion::Statement)
                } else { is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, value) };
                if !supported { return false; }
            }
            ArenaStmtKind::Break { value } => {
                if value.is_some_and(|value| !is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, value)) { return false; }
            }
            ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(value))) => {
                let supported = if matches!(completion, BodyCompletion::Procedure | BodyCompletion::ProcedureStatement)
                    && positions.get(&node.span) == Some(&super::StatementPosition::Statement)
                { is_proc_completion_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, value) }
                else { is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, value) };
                if !supported { return false; }
            }
            ArenaStmtKind::If { branches, else_block } => {
                if !program.arena.if_branches(branches).iter().all(|branch| {
                    is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, branch.condition)
                        && supports_generalized_body(program, prepared, command_sites, branch.block, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, nested_completion)
                }) || else_block.is_some_and(|block| !supports_generalized_body(program, prepared, command_sites, block, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, nested_completion)) { return false; }
            }
            ArenaStmtKind::Match { value, arms } => {
                if !is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, value)
                    || !program.arena.match_arms(arms).iter().all(|arm| {
                        let available = pattern_bindings(program, bindings, &available, arm.pattern);
                        arm.guard.is_none_or(|guard| is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, guard))
                            && supports_generalized_body(program, prepared, command_sites, arm.block, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, nested_completion)
                    }) { return false; }
            }
            ArenaStmtKind::For { target, iter, block } => {
                if !is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, iter) { return false; }
                let mut inner = available.clone();
                if !declare_target_bindings(program, bindings, target, &mut inner)
                    || !supports_generalized_body(program, prepared, command_sites, block, bindings, &inner, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, nested_completion)
                { return false; }
            }
            ArenaStmtKind::Loop { block } => {
                if !supports_generalized_body(program, prepared, command_sites, block, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, nested_completion) { return false; }
            }
            ArenaStmtKind::While { condition, block } => {
                if !is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, condition)
                    || !supports_generalized_body(program, prepared, command_sites, block, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, nested_completion) { return false; }
            }
            ArenaStmtKind::Command(_) => {
                if !checked_calls.contains(&node.span) { return false; }
                let mut supported = true;
                let mut children = Vec::new();
                program.arena.for_each_stmt_child(statement, crate::syntax::arena::SugarView::Core, |child| children.push(child));
                let mut nested_spans = children.iter().filter_map(|child| match *child {
                    crate::syntax::arena::ArenaChild::Expr(expression) => Some(program.arena.expr(expression).span),
                    crate::syntax::arena::ArenaChild::Block(block) => Some(program.arena.span(program.arena.block(block).span)),
                    _ => None,
                }).collect::<Vec<_>>();
                nested_spans.sort_unstable();
                let mut nested_position = 0;
                let start = (Span::at(node.span.source_id, node.span.start()), super::Name::UNKNOWN);
                let end = (Span::at(node.span.source_id, node.span.end()), super::Name::UNKNOWN);
                for (&(site, _), binding) in command_sites.range(start..end) {
                    if site.end() > node.span.end() { return false; }
                    while nested_position < nested_spans.len() && nested_spans[nested_position].end() <= site.start() { nested_position += 1; }
                    if nested_spans.get(nested_position).is_some_and(|nested| nested.start() <= site.start() && site.end() <= nested.end()) { continue; }
                    let constant_type = prepared.binding_values.get(binding).and_then(|expression| prepared.types.get(expression));
                    if !(available.contains(binding) || captures.contains(binding)
                        || bindings.declarations.get(binding).is_some_and(|declaration| matches!(declaration.kind, super::BindingKind::Constant(_)))
                            && constant_type.is_some_and(|ty| !ty.contains_inference() && !ty.contains_recovery() && !ty.may_hold_resource())) { return false; }
                }
                for child in children {
                    supported &= match child {
                        crate::syntax::arena::ArenaChild::Expr(expression) => is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, expression),
                        crate::syntax::arena::ArenaChild::Block(block) => supports_generalized_body(program, prepared, command_sites, block, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, nested_completion),
                        _ => false,
                    };
                }
                if !supported { return false; }
            }
            ArenaStmtKind::Continue => {}
            _ => return false,
        }
    }
    if matches!(completion, BodyCompletion::Statement | BodyCompletion::ProcedureStatement | BodyCompletion::Producer) { return true; }
    let tail_statement = program.arena.stmt(tail);
    if positions.get(&tail_statement.span) == Some(&super::StatementPosition::Statement) {
        match tail_statement.kind {
            ArenaStmtKind::Expr(expression) if types.get(&expression).is_some_and(|ty| ty.closed_ground_type() == Some(crate::sema::types::Type::Unit)) => {}
            ArenaStmtKind::Expr(_) | ArenaStmtKind::TailBareIdent(_) => return false,
            _ => {}
        }
    }
    match tail_statement.kind {
        ArenaStmtKind::TailBareIdent(_) => bindings.tails.get(&tail).is_some_and(|identity| read_binding_is_available(bindings, &available, captures, identity, tails.get(&tail))),
        ArenaStmtKind::Expr(expression) => is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, expression),
        ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expression))) if matches!(completion, BodyCompletion::Function | BodyCompletion::Procedure) => {
            if matches!(completion, BodyCompletion::Procedure) && positions.get(&tail_statement.span) == Some(&super::StatementPosition::Statement) {
                is_proc_completion_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, expression)
            } else { is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, expression) }
        },
        ArenaStmtKind::If { branches, else_block: Some(else_block) } => {
            program.arena.if_branches(branches).iter().all(|branch| {
                is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, branch.condition)
                    && supports_generalized_body(program, prepared, command_sites, branch.block, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, completion)
            }) && supports_generalized_body(program, prepared, command_sites, else_block, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, completion)
        }
        ArenaStmtKind::Match { value, arms } => {
            is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, value)
                && program.arena.match_arms(arms).iter().all(|arm| {
                    let available = pattern_bindings(program, bindings, &available, arm.pattern);
                    arm.guard.is_none_or(|guard| is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, guard))
                        && supports_generalized_body(program, prepared, command_sites, arm.block, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, completion)
                })
        }
        _ => false,
    }
}

fn read_binding_is_available(
    bindings: &ResolvedBindings,
    available: &FxHashSet<BindingId>,
    captures: &FxHashSet<BindingId>,
    binding: &BindingId,
    ty: Option<&crate::sema::schemes::SchemeType>,
) -> bool {
    available.contains(binding)
        || (captures.contains(binding)
            || bindings.declarations.get(binding).is_some_and(|declaration| matches!(declaration.kind, super::BindingKind::Constant(_))))
            && ty.is_some_and(|ty| ty.closed_ground_type().is_some_and(|ty| !ty.may_hold_resource()))
}

fn declare_target_bindings(program: &ArenaProgram, bindings: &ResolvedBindings, target: crate::syntax::arena::BindingTargetId, available: &mut FxHashSet<BindingId>) -> bool {
    let mut pending = vec![target];
    while let Some(target) = pending.pop() {
        match program.arena.binding_target(target).kind {
            crate::syntax::arena::ArenaBindingTargetKind::Name(name) => {
                if name.as_str() != "_" {
                    let Some(binding) = bindings.target(target) else { return false };
                    available.insert(binding);
                }
            }
            crate::syntax::arena::ArenaBindingTargetKind::Record { fields, .. } => pending.extend(program.arena.destructure_fields(fields).iter().map(|field| field.target)),
        }
    }
    true
}

fn pattern_bindings(program: &ArenaProgram, bindings: &ResolvedBindings, available: &FxHashSet<BindingId>, pattern: crate::syntax::arena::PatternId) -> FxHashSet<BindingId> {
    use crate::syntax::arena::{ArenaChild, ArenaPatternKind};
    let mut available = available.clone();
    let mut pending = vec![pattern];
    while let Some(pattern) = pending.pop() {
        let node = program.arena.pattern(pattern);
        let declaration = match node.kind {
            ArenaPatternKind::Binding(name) | ArenaPatternKind::TestName { name, .. } => Some((program.arena.span(node.span), name)),
            ArenaPatternKind::Type { binding: Some(name), .. } | ArenaPatternKind::TextHole { binding: Some(name), .. } => Some((program.arena.span(node.span), name)),
            ArenaPatternKind::Alias { name, name_span, .. } => Some((program.arena.span(name_span), name)),
            _ => None,
        };
        if let Some(site) = declaration && let Some(binding) = bindings.declaration_sites.get(&site) { available.insert(*binding); }
        program.arena.for_each_pattern_child(pattern, |child| if let ArenaChild::Pattern(pattern) = child { pending.push(pattern); });
    }
    available
}

fn comprehension_bindings(
    program: &ArenaProgram,
    bindings: &ResolvedBindings,
    available: &FxHashSet<BindingId>,
    qualifiers: crate::syntax::arena::ArenaRange,
    mut expression: impl FnMut(&FxHashSet<BindingId>, crate::syntax::arena::ExprId) -> bool,
) -> Option<FxHashSet<BindingId>> {
    let mut available = available.clone();
    for qualifier in program.arena.comp_qualifiers(qualifiers) {
        match *qualifier {
            crate::syntax::arena::ArenaCompQualifier::For { target, iter, .. } => {
                if !expression(&available, iter) || !declare_target_bindings(program, bindings, target, &mut available) { return None; }
            }
            crate::syntax::arena::ArenaCompQualifier::If { condition, .. } => {
                if !expression(&available, condition) { return None; }
            }
        }
    }
    Some(available)
}

fn is_proc_completion_expression(
    program: &ArenaProgram,
    prepared: &crate::sema::constants::PreparedConstants,
    command_sites: &std::collections::BTreeMap<(Span, super::Name), BindingId>,
    bindings: &ResolvedBindings,
    available: &FxHashSet<BindingId>,
    captures: &FxHashSet<BindingId>,
    tails: &std::collections::BTreeMap<crate::syntax::arena::StmtId, crate::sema::schemes::SchemeType>,
    positions: &std::collections::BTreeMap<Span, super::StatementPosition>,
    checked_calls: &FxHashSet<Span>,
    registered_calls: &std::collections::BTreeMap<crate::syntax::arena::ExprId, std::sync::Arc<crate::sema::schemes::RegisteredCallPlan>>,
    types: &std::collections::BTreeMap<crate::syntax::arena::ExprId, crate::sema::schemes::SchemeType>,
    constructor_producers: &std::collections::BTreeMap<crate::syntax::arena::ExprId, crate::sema::schemes::NonmaterialConstructor>,
    result_constructors: &std::collections::BTreeMap<Span, super::CheckedResultConstructor>,
    expression: crate::syntax::arena::ExprId,
) -> bool {
    if types.contains_key(&expression) {
        return is_body_expression(program, prepared, command_sites, bindings, available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, expression);
    }
    if positions.get(&program.arena.expr(expression).span) != Some(&super::StatementPosition::Statement) { return false; }
    match program.arena.expr(expression).kind {
        ArenaExprKind::If { branches, else_value } => {
            program.arena.if_expr_branches(branches).iter().all(|branch| {
                is_body_expression(program, prepared, command_sites, bindings, available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, branch.condition)
                    && is_proc_completion_expression(program, prepared, command_sites, bindings, available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, branch.value)
            }) && is_proc_completion_expression(program, prepared, command_sites, bindings, available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, else_value)
        }
        ArenaExprKind::Match { value, arms } => {
            is_body_expression(program, prepared, command_sites, bindings, available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, value)
                && program.arena.match_expr_arms(arms).iter().all(|arm| {
                    let available = pattern_bindings(program, bindings, available, arm.pattern);
                    arm.guard.is_none_or(|guard| is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, guard))
                        && is_proc_completion_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, arm.value)
                })
        }
        ArenaExprKind::ValueBlock(block) => supports_generalized_body(program, prepared, command_sites, block, bindings, available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, BodyCompletion::Procedure),
        _ => false,
    }
}

pub(super) fn is_body_expression(
    program: &ArenaProgram,
    prepared: &crate::sema::constants::PreparedConstants,
    command_sites: &std::collections::BTreeMap<(Span, super::Name), BindingId>,
    bindings: &ResolvedBindings,
    available: &FxHashSet<BindingId>,
    captures: &FxHashSet<BindingId>,
    tails: &std::collections::BTreeMap<crate::syntax::arena::StmtId, crate::sema::schemes::SchemeType>,
    positions: &std::collections::BTreeMap<Span, super::StatementPosition>,
    checked_calls: &FxHashSet<Span>,
    registered_calls: &std::collections::BTreeMap<crate::syntax::arena::ExprId, std::sync::Arc<crate::sema::schemes::RegisteredCallPlan>>,
    types: &std::collections::BTreeMap<crate::syntax::arena::ExprId, crate::sema::schemes::SchemeType>,
    constructor_producers: &std::collections::BTreeMap<crate::syntax::arena::ExprId, crate::sema::schemes::NonmaterialConstructor>,
    result_constructors: &std::collections::BTreeMap<Span, super::CheckedResultConstructor>,
    expression: crate::syntax::arena::ExprId,
) -> bool {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        if prepared.values.contains_key(&expression) {
            let Some(expected) = prepared.types.get(&expression) else { return false };
            if expected.may_hold_resource() || types.get(&expression).and_then(|ty| ty.closed_ground_type()).as_ref() != Some(expected) { return false; }
            continue;
        }
        if let Some(&kind) = constructor_producers.get(&expression) {
            if !matches_constructor_producer(program, expression, kind) { return false; }
            if kind != crate::sema::schemes::NonmaterialConstructor::Record { continue; }
        }
        match program.arena.expr(expression).kind {
            ArenaExprKind::Ident(_) => {
                if !bindings.expressions.get(&expression).is_some_and(|binding| read_binding_is_available(bindings, available, captures, binding, types.get(&expression))) { return false; }
            }
            ArenaExprKind::Null | ArenaExprKind::Bool(_) | ArenaExprKind::Int(_) | ArenaExprKind::Float(_) | ArenaExprKind::Str(_) | ArenaExprKind::Bytes(_) | ArenaExprKind::Duration(_) | ArenaExprKind::Regex(_) | ArenaExprKind::PathStr(_) => {}
            ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => pending.push(base),
            ArenaExprKind::Index { base, index, .. } => {
                if !types.contains_key(&expression) { return false; }
                pending.push(base); pending.push(index);
            }
            ArenaExprKind::Slice { base, start, end, .. } => {
                if !types.contains_key(&expression) { return false; }
                pending.push(base);
                pending.extend(start);
                pending.extend(end);
            }
            ArenaExprKind::Binary { left, right, .. } => {
                if !types.contains_key(&expression) { return false; }
                pending.push(left); pending.push(right);
            }
            ArenaExprKind::ComparisonChain(pairs) => {
                if !types.contains_key(&expression) { return false; }
                pending.extend(program.arena.comparison_chain_operands(pairs));
            }
            ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => {
                if !types.contains_key(&expression) { return false; }
                pending.push(expr);
            }
            ArenaExprKind::List(items) => {
                for item in program.arena.list_elements(items) {
                    pending.push(item.value);
                }
            }
            ArenaExprKind::Record(fields) => {
                for field in program.arena.record_fields(fields) {
                    match field.kind {
                        crate::syntax::arena::ArenaRecordFieldKind::Named { value, .. } => pending.push(value),
                        crate::syntax::arena::ArenaRecordFieldKind::Spread { expr, .. } => pending.push(expr),
                        crate::syntax::arena::ArenaRecordFieldKind::Computed { key, value, .. } => { pending.push(key); pending.push(value); }
                        crate::syntax::arena::ArenaRecordFieldKind::Path { value, .. } => pending.push(value),
                        crate::syntax::arena::ArenaRecordFieldKind::Shorthand { name, span } => {
                            let Some(binding) = bindings.value_sites.get(&(program.arena.span(span), name)) else { return false };
                            if !available.contains(binding) && !captures.contains(binding) { return false; }
                        }
                    }
                }
            }
            ArenaExprKind::Set(items) => pending.extend(program.arena.list_elements(items).map(|item| item.value)),
            ArenaExprKind::ListComp { expr, qualifiers } | ArenaExprKind::SetComp { expr, qualifiers } => {
                let Some(inner) = comprehension_bindings(program, bindings, available, qualifiers, |available, expression| is_body_expression(program, prepared, command_sites, bindings, available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, expression)) else { return false };
                if !is_body_expression(program, prepared, command_sites, bindings, &inner, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, expr) { return false; }
            }
            ArenaExprKind::MapComp { key, value, qualifiers } => {
                let Some(inner) = comprehension_bindings(program, bindings, available, qualifiers, |available, expression| is_body_expression(program, prepared, command_sites, bindings, available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, expression)) else { return false };
                if ![key, value].into_iter().all(|expression| is_body_expression(program, prepared, command_sites, bindings, &inner, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, expression)) { return false; }
            }
            ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
                for part in program.arena.fmt_parts(parts) {
                    if let crate::syntax::arena::ArenaFmtPart::Expr(value, _) = part { pending.push(value); }
                }
            }
            ArenaExprKind::If { branches, else_value } => {
                if !types.contains_key(&expression) { return false; }
                for branch in program.arena.if_expr_branches(branches) { pending.push(branch.condition); pending.push(branch.value); }
                pending.push(else_value);
            }
            ArenaExprKind::ValueBlock(block) | ArenaExprKind::Capture(block) => {
                if !supports_generalized_body(program, prepared, command_sites, block, bindings, available, captures,
                    tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, BodyCompletion::Value)
                { return false; }
            }
            ArenaExprKind::Loop { block } => {
                if !supports_generalized_body(program, prepared, command_sites, block, bindings, available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, BodyCompletion::Statement) { return false; }
            }
            ArenaExprKind::Match { value, arms } => {
                if !types.contains_key(&expression) { return false; }
                pending.push(value);
                for arm in program.arena.match_expr_arms(arms) {
                    let available = pattern_bindings(program, bindings, available, arm.pattern);
                    if !is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, arm.value)
                        || arm.guard.is_some_and(|guard| !is_body_expression(program, prepared, command_sites, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, constructor_producers, result_constructors, guard))
                    { return false; }
                }
            }

            ArenaExprKind::Call { callee, args } => {
                if let Some(plan) = registered_calls.get(&expression) {
                    if plan.receiver.is_some() {
                        let (ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. }) = program.arena.expr(callee).kind else { return false };
                        pending.push(base);
                    }
                } else if !checked_calls.contains(&program.arena.expr(expression).span)
                    && !result_constructors.contains_key(&program.arena.expr(expression).span) { return false; }
                for argument in program.arena.call_args(args) {
                    match argument.kind {
                        ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. }
                        | ArenaCallArgKind::NamedSpread { value, .. } | ArenaCallArgKind::Splice { value, .. } => pending.push(value),
                    }
                }
            }
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod ordinary_body_tests {
    use crate::diagnostic::Severity;
    use crate::sema::check::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    fn accepts(definition: &str, calls: &str) {
        let source = format!("{definition}\n{calls}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
        assert!(errors.is_empty(), "{source}\n{errors:?}");
    }

    #[test]
    fn generic_named_constants_keep_independent_parameter_instances() {
        accepts(
            "const marker = \"constant:\"\npure attach(value) { {marker: marker, value: value} }",
            "let integer = attach(42)\nlet number: Int = integer.value\nlet text = attach(\"demo\")\nlet word: Str = text.value\nlet label: Str = text.marker",
        );
    }

    #[test]
    fn generic_named_constant_tails_keep_independent_parameter_instances() {
        accepts(
            "const marker = \"constant:\"\npure read(value) { marker }",
            "let integer: Str = read(42)\nlet text: Str = read(\"demo\")",
        );
    }

    #[test]
    fn generic_parameter_shadowing_keeps_its_own_binding() {
        accepts(
            "const marker = \"constant:\"\npure attach(marker) { {value: marker} }",
            "let integer = attach(42)\nlet number: Int = integer.value\nlet text = attach(\"demo\")\nlet word: Str = text.value",
        );
    }

    #[test]
    fn generic_match_results_reuse_the_selected_parameter() {
        accepts(
            "pure choose(flag: Bool, value) { match flag { true => value; false => value } }",
            "let number: Int = choose(true, 7)\nlet text: Str = choose(false, \"value\")",
        );
    }

    #[test]
    fn generic_loop_results_reuse_the_break_payload() {
        accepts(
            "pure keep(value) { let result = loop { break value }; result }",
            "let number: Int = keep(7)\nlet text: Str = keep(\"value\")",
        );
    }

    #[test]
    fn generic_local_mutation_preserves_the_slot_parameter() {
        accepts(
            "proc keep(value) [] { var result = value; { result = value }; result }",
            "let number: Int = keep(7)?\nlet text: Str = keep(\"value\")?",
        );
    }

    #[test]
    fn generic_nested_bindings_retain_their_lexical_identity() {
        accepts(
            "pure keep(value) { let result = { let copy = value; copy }; result }",
            "let number: Int = keep(7)\nlet text: Str = keep(\"value\")",
        );
    }

    #[test]
    fn generic_discarded_values_do_not_require_a_named_binding() {
        accepts(
            "pure keep(value) { let _ = \"value\".starts_with(\"v\"); value }",
            "let number: Int = keep(7)\nlet text: Str = keep(\"value\")",
        );
    }

    #[test]
    fn generic_loop_results_exclude_nested_loop_payloads() {
        accepts(
            "pure keep(value) { let result = loop { let inner = loop { break 1 }; let _ = inner; break value }; result }",
            "let number: Int = keep(7)\nlet text: Str = keep(\"value\")",
        );
    }

    #[test]
    fn generic_body_results_reject_incompatible_consumers() {
        for definition in [
            "pure keep(value) { match true { true => value; false => value } }",
            "pure keep(value) { let result = loop { break value }; result }",
            "proc keep(value) [] { var result = value; result = value; result }",
        ] {
            let unwrap = if definition.starts_with("proc ") { "?" } else { "" };
            let source = format!("{definition}\nlet wrong: Str = keep(7){unwrap}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch)), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn generic_mutable_slots_remain_monomorphic_within_each_call() {
        let source = "pure keep(value) { var result = value; result = 7; result }\nlet wrong: Str = keep(\"value\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch)), "{:?}", checked.diagnostics);
    }

    #[test]
    fn generic_immutable_bindings_cannot_be_reassigned() {
        let source = "pure keep(value) { let result = value; result = value; result }\nlet number: Int = keep(7)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckAssignLet)), "{:?}", checked.diagnostics);
    }

    #[test]
    fn generic_branch_writers_share_one_activation_local_contract() {
        accepts(
            "pure choose(flag: Bool, left, right) { var result = left; if flag { result = right }; result }",
            "let number: Int = choose(true, 1, 7)\nlet text: Str = choose(false, \"value\", \"other\")",
        );
    }

    #[test]
    fn generic_guarded_optional_receivers_retain_their_checked_types() {
        accepts(
            "pure keep(value, items: List[Int]?) { let _ = items?[0]; value }",
            "let number: Int = keep(7, null)\nlet text: Str = keep(\"value\", [1])",
        );
    }

    #[test]
    fn generic_guarded_fields_retain_their_checked_types() {
        accepts(
            "type Row = {item: Int}\npure keep(value, row: Row?) { let _ = row?.item; value }",
            "let number: Int = keep(7, null)\nlet text: Str = keep(\"value\", {item: 1})",
        );
    }

    #[test]
    fn generic_guarded_methods_retain_the_selected_contract() {
        accepts(
            "pure keep(value, text: Str?) { let _ = text?.starts_with(\"v\"); value }",
            "let number: Int = keep(7, null)\nlet text: Str = keep(\"value\", \"value\")",
        );
    }

    #[test]
    fn generic_guarded_result_receivers_retain_the_error_contract() {
        accepts(
            "pure keep(value, items: Result[List[Int], Str]) -> Result[Int, Str] { let _ = value; Ok(items?[0]) }",
            "let number: Result[Int, Str] = keep(7, Ok([1]))\nlet text: Result[Int, Str] = keep(\"value\", Err(\"failed\"))",
        );
    }

    #[test]
    fn generic_validated_aggregate_projections_keep_their_base_contract() {
        for operation in ["items[0]", "items[0..1]"] {
            accepts(
                &format!("pure select(value, items: NonEmpty[Int]) {{ let _ = {operation}; value }}"),
                "let number: Int = select(7, [1])\nlet text: Str = select(\"value\", [2])",
            );
        }
    }

    #[test]
    fn generic_list_concatenation_preserves_invariant_unsigned_items() {
        let source = "pure keep(value, unsigned: List[UInt], signed: List[Int]) { let _ = unsigned + signed; value }";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch) && diagnostic.labels.iter().any(|label| label.message.as_deref() == Some("expected List[UInt], found List[Int]"))), "{:?}", checked.diagnostics);
    }

    #[test]
    fn generic_list_from_end_index_preserves_the_element_parameter() {
        accepts(
            "pure select(value) { [value][-1] }",
            "let number: Int = select(7)\nlet text: Str = select(\"value\")",
        );
    }

    #[test]
    fn generic_list_index_preserves_the_element_parameter() {
        accepts(
            "pure select(value) { [value][0] }",
            "let number: Int = select(7)\nlet text: Str = select(\"value\")",
        );
    }

    #[test]
    fn generic_list_slice_preserves_the_element_parameter() {
        accepts(
            "pure select(value) { [value, value][0..1] }",
            "let numbers: List[Int] = select(7)\nlet texts: List[Str] = select(\"value\")",
        );
    }

    #[test]
    fn generic_record_index_preserves_the_selected_field() {
        accepts(
            "pure select(value) { {item: value}[\"item\"] }",
            "let number: Int = select(7)\nlet text: Str = select(\"value\")",
        );
    }

    #[test]
    fn generic_grounded_operators_preserve_unrelated_parameters() {
        accepts(
            "pure select(value, number: Int) { let _ = number + 1; value }",
            "let number: Int = select(7, 1)\nlet text: Str = select(\"value\", 2)",
        );
    }

    #[test]
    fn generic_match_bindings_remain_local_to_the_selected_arm() {
        accepts(
            "pure keep(value) { match value { selected => selected } }",
            "let number: Int = keep(7)\nlet text: Str = keep(\"value\")",
        );
    }

    #[test]
    fn generic_construction_formats_checked_scalar_operands() {
        accepts(
            "pure keep(value, text: Str, count: Int) { let _ = f\"{text}:{count}\"; value }",
            "let number: Int = keep(7, \"item\", 2)\nlet text: Str = keep(\"value\", \"item\", 3)",
        );
    }

    #[test]
    fn generic_construction_formats_checked_path_operands() {
        accepts(
            "pure keep(value, root: Path, name: Str) { let _ = fp\"{root}/{name}\"; value }",
            "let number: Int = keep(7, /tmp, \"item\")\nlet text: Str = keep(\"value\", /tmp, \"other\")",
        );
    }

    #[test]
    fn generic_construction_splices_lists_with_one_payload_binder() {
        accepts(
            "pure copy(value) { [@[value], value] }",
            "let number: List[Int] = copy(7)\nlet text: List[Str] = copy(\"value\")",
        );
    }

    #[test]
    fn generic_construction_spreads_records_with_one_payload_binder() {
        accepts(
            "type Numbers = {item: Int, count: Int}\ntype Words = {item: Str, count: Int}\npure copy(value) { let row = {item: value}; {...row, count: 1} }",
            "let number: Numbers = copy(7)\nlet text: Words = copy(\"value\")",
        );
    }

    #[test]
    fn generic_construction_maps_retain_key_and_payload_domains() {
        accepts(
            "pure copy(value) { {[0]: value} }",
            "let number: Map[Int, Int] = copy(7)\nlet text: Map[Int, Str] = copy(\"value\")",
        );
    }

    #[test]
    fn generic_construction_sets_retain_the_checked_scalar_domain() {
        accepts(
            "pure keep(value, item: Int) { let _ = {1, item}; value }",
            "let number: Int = keep(7, 2)\nlet text: Str = keep(\"value\", 3)",
        );
    }

    #[test]
    fn generic_iteration_record_bindings_retain_renamed_payloads() {
        accepts("pure keep(value) { let {item: copy} = {item: value}; copy }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_iteration_nested_bindings_retain_projected_payloads() {
        accepts("pure keep(value) { let {nested: {item: copy, ..}, ..} = {nested: {item: value, ignored: 1}, extra: true}; copy }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_iteration_shadowed_bindings_keep_lexical_identity() {
        accepts("pure keep(value) { let {item: copy} = {item: value}; let result = { let {item: copy} = {item: copy}; copy }; result }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_iteration_mutable_record_bindings_keep_one_payload_domain() {
        accepts("proc keep(value) [] { var {item: copy} = {item: value}; copy = value; copy }", "let number: Int = keep(7)?\nlet text: Str = keep(\"value\")?");
    }

    #[test]
    fn generic_iteration_list_items_retain_the_payload_binder() {
        accepts("pure keep(value) { var result = value; for item in [value] { result = item }; result }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_iteration_record_items_retain_projected_payloads() {
        accepts("pure keep(value) { var result = value; for {item: copy} in [{item: value}] { result = copy }; result }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_iteration_nested_loops_own_break_and_continue() {
        accepts("pure keep(value) { var result = value; for row in [[value]] { for item in row { result = item; continue }; break }; result }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_iteration_closed_sources_keep_their_item_domains() {
        accepts("pure keep(value) { for item in [1] { let _ = item }; for byte in b\"x\" { let _ = byte }; for letter in \"x\" { let _ = letter }; for entry in {[\"key\"]: value} { let _ = entry.value }; value }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_comprehension_list_items_retain_the_payload_binder() {
        accepts("pure copy(value) { [item for item in [value]] }", "let number: List[Int] = copy(7)\nlet text: List[Str] = copy(\"value\")");
    }

    #[test]
    fn generic_comprehension_nested_targets_keep_sequential_binding_identity() {
        accepts("pure copy(value) { [inner for outer in [value] for {item: inner} in [{item: outer}]] }", "let number: List[Int] = copy(7)\nlet text: List[Str] = copy(\"value\")");
    }

    #[test]
    fn generic_comprehension_shadowed_targets_read_the_previous_binding_first() {
        accepts("pure copy(value) { [item for item in [value] for item in [item]] }", "let number: List[Int] = copy(7)\nlet text: List[Str] = copy(\"value\")");
    }

    #[test]
    fn generic_comprehension_filters_preserve_the_selected_payload_type() {
        accepts("pure copy(value, selected: Bool) { [item for item in [value] if selected] }", "let number: List[Int] = copy(7, true)\nlet text: List[Str] = copy(\"value\", false)");
    }

    #[test]
    fn generic_comprehension_map_values_retain_the_payload_binder() {
        accepts("pure copy(value) { {key: value for key in [\"a\", \"b\"]} }", "let number: Map[Str, Int] = copy(7)\nlet text: Map[Str, Str] = copy(\"value\")");
    }

    #[test]
    fn generic_comprehension_set_items_keep_the_closed_key_domain() {
        accepts("pure copy(value, items: List[Int]) { let unique = {item for item in items}; {value, unique} }", "let number = copy(7, [1, 2])\nlet text = copy(\"value\", [2, 3])");
    }

    #[test]
    fn generic_comprehension_nonempty_mapping_preserves_its_source_validation() {
        accepts("pure copy(value, items: NonEmpty[Int]) { let copied = [item for item in items]; {value, copied} }", "let number = copy(7, [1])\nlet text = copy(\"value\", [2])");
    }

    #[test]
    fn generic_comprehension_result_map_items_retain_the_success_binder() {
        accepts("pure copy(value) { Ok([item.value for item in Ok({[\"key\"]: value})]) }", "let number: Result[List[Int]] = copy(7)\nlet text: Result[List[Str]] = copy(\"value\")");
    }

    #[test]
    fn generic_comprehension_filters_retain_the_ordinary_status_condition() {
        accepts("pure copy(value, selected: Status) { [item for item in [value] if selected] }", "pure number(selected: Status) -> List[Int] { copy(7, selected) }\npure text(selected: Status) -> List[Str] { copy(\"value\", selected) }");
    }

    #[test]
    fn generic_arguments_named_order_preserves_the_payload_binder() {
        accepts("pure select(value, flag: Bool) { let _ = flag; value }\npure call(value) { select(flag: true, value: value) }", "let number: Int = call(7)\nlet text: Str = call(\"value\")");
    }

    #[test]
    fn generic_arguments_named_spreads_preserve_the_payload_binder() {
        accepts("pure select(value, flag: Bool) { let _ = flag; value }\npure call(value) { select(...{flag: true, value: value}) }", "let number: Int = call(7)\nlet text: Str = call(\"value\")");
    }

    #[test]
    fn generic_arguments_local_spreads_keep_the_checked_record_shape() {
        accepts("pure select(value, flag: Bool) { let _ = flag; value }\npure call(value) { let args = {flag: true, value: value}; select(...args) }", "let number: Int = call(7)\nlet text: Str = call(\"value\")");
    }

    #[test]
    fn generic_arguments_rest_calls_preserve_the_fixed_payload() {
        accepts("pure head(value, ...items: List[Int]) { let _ = items; value }\npure call(value) { let _ = head(value); head(value, 1, 2) }", "let number: Int = call(7)\nlet text: Str = call(\"value\")");
    }

    #[test]
    fn generic_arguments_rest_splices_keep_the_actual_list_domain() {
        accepts("pure head(value, ...items: List[Int]) { let _ = items; value }\npure call(value, tail: List[Int]) { head(value, @tail) }", "let number: Int = call(7, [1, 2])\nlet text: Str = call(\"value\", [])");
    }

    #[test]
    fn generic_arguments_omitted_defaults_keep_the_declaration_contract() {
        accepts("pure select(value, flag: Bool = false) { let _ = flag; value }\npure call(value) { select(value) }", "let number: Int = call(7)\nlet text: Str = call(\"value\")");
    }

    #[test]
    fn generic_arguments_explicit_defaults_keep_the_supplied_payload() {
        accepts("pure select(value, flag: Bool = false) { let _ = flag; value }\npure call(value) { select(flag: true, value: value) }", "let number: Int = call(7)\nlet text: Str = call(\"value\")");
    }

    #[test]
    fn generic_arguments_inferred_default_types_remain_independent_of_calls() {
        accepts("pure select(value, flag = false) { let _ = flag; value }", "let number: Int = select(7)\nlet text: Str = select(\"value\", true)");
    }

    #[test]
    fn generic_assignment_list_entries_retain_the_payload_binder() {
        accepts("pure replace(value, other) { var items = [value]; items[0] = other; items[0] }", "let number: Int = replace(7, 8)\nlet text: Str = replace(\"value\", \"other\")");
    }

    #[test]
    fn generic_assignment_record_paths_retain_the_payload_binder() {
        accepts("pure replace(value, other) { var row = {nested: {item: value}, count: 1}; row.nested.item = other; row.nested.item }", "let number: Int = replace(7, 8)\nlet text: Str = replace(\"value\", \"other\")");
    }

    #[test]
    fn generic_assignment_map_entries_retain_the_payload_binder() {
        accepts("pure replace(value, other, key: Str) { var items = {[key]: value}; items[key] = other; items[key] }", "let number: Int = replace(7, 8, \"key\")\nlet text: Str = replace(\"value\", \"other\", \"key\")");
    }

    #[test]
    fn generic_assignment_mixed_selectors_retain_the_selected_payload() {
        accepts("pure replace(value, other, index: Int) { var items = [{nested: [value]}]; items[index].nested[0] = other; items[index].nested[0] }", "let number: Int = replace(7, 8, 0)\nlet text: Str = replace(\"value\", \"other\", 0)");
    }

    #[test]
    fn generic_assignment_compound_lists_keep_invariant_elements() {
        accepts("pure append(value, other) { var items = [value]; items += [other]; items }", "let number: List[Int] = append(7, 8)\nlet text: List[Str] = append(\"value\", \"other\")");
    }

    #[test]
    fn generic_assignment_scalar_compounds_keep_the_checked_operator_domains() {
        accepts("pure update(value, delta: Int) { var count = 20; count += delta; count -= 1; count *= 2; count /= 2; count %= 7; {value, count} }", "let number: Int = update(7, 3).value\nlet text: Str = update(\"value\", 4).value");
    }

    #[test]
    fn generic_assignment_unsigned_fields_keep_the_validation_boundary() {
        for definition in [
            "pure update(value, initial: UInt, delta: Int) { var row = {value, count: initial}; row.count += delta; row }",
            "pure update(value, initial: UInt, delta: Int) { var {item: row} = {item: {value, count: initial}}; row.count += delta; row }",
        ] {
            accepts(definition, "let number: Int = update(7, 1, 2).value\nlet text: Str = update(\"value\", 3, 4).value");
        }
    }

    #[test]
    fn generic_assignment_unsigned_map_keys_preserve_the_payload_binder() {
        accepts("pure put(value, other, key: UInt, index: Int) { var items = {[key]: value}; items[index] = other; items[key] }", "pure number(key: UInt, index: Int) -> Int { put(7, 8, key, index) }\npure text(key: UInt, index: Int) -> Str { put(\"value\", \"other\", key, index) }");
    }

    #[test]
    fn generic_assignment_updates_preserve_earlier_alias_types() {
        accepts("pure replace(value, other) { var items = [value]; let before = items; items[0] = other; {before, after: items} }", "let number: Int = replace(7, 8).before[0]\nlet text: Str = replace(\"value\", \"other\").after[0]");
    }

    #[test]
    fn generic_operators_logical_values_retain_the_payload_binder() {
        accepts("pure keep(value, first: Bool, second: Bool) { let _ = first and second; let _ = first or second; value }", "let number: Int = keep(7, true, false)\nlet text: Str = keep(\"value\", false, true)");
    }

    #[test]
    fn generic_operators_equality_uses_the_same_symbolic_domain() {
        accepts("pure keep(value) { let _ = value == value; let _ = value != value; value }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_operators_ordering_keeps_closed_domains() {
        accepts("pure keep(value, number: Int, text: Str, elapsed: Duration) { let _ = number < 7; let _ = text >= \"a\"; let _ = elapsed <= 1s; value }", "let number: Int = keep(7, 2, \"value\", 2s)\nlet text: Str = keep(\"value\", 3, \"other\", 1s)");
    }

    #[test]
    fn generic_operators_ordering_chains_keep_adjacent_operands() {
        accepts("pure keep(value, number: Int) { let _ = 0 <= number < 7; value }", "let number: Int = keep(7, 2)\nlet text: Str = keep(\"value\", 3)");
    }

    #[test]
    fn generic_operators_list_membership_keeps_the_symbolic_item() {
        accepts("pure keep(value) { let _ = value in [value]; let _ = value not in [value]; value }", "let number: Int = keep(7)\nlet text: Str = keep(\"value\")");
    }

    #[test]
    fn generic_operators_membership_preserves_closed_key_domains() {
        accepts("pure keep(value, keys: Map[UInt, Int], key: UInt, elements: Set[Int]) { let _ = key in keys; let _ = 1 not in elements; let _ = \"a\" in \"value\"; value }", "let number: Int = keep(7, {}, 1, {1, 2})\nlet text: Str = keep(\"value\", {}, 2, {2, 3})");
    }

    #[test]
    fn generic_operators_set_algebra_keeps_the_element_domain() {
        accepts("pure keep(value, left: Set[Int], right: Set[Int]) { let _ = left | right; let _ = left & right; let _ = left - right; value }", "let number: Int = keep(7, {1, 2}, {2, 3})\nlet text: Str = keep(\"value\", {2, 3}, {3, 4})");
    }

    #[test]
    fn generic_record_forms_shorthand_retains_the_parameter_binder() {
        accepts("type Number = {value: Int}\ntype Text = {value: Str}\npure copy(value) { {value} }", "let number: Number = copy(7)\nlet text: Text = copy(\"value\")");
    }

    #[test]
    fn generic_record_forms_shorthand_follows_local_shadowing() {
        accepts("type Number = {item: Int}\ntype Text = {item: Str}\npure copy(value) { let item = value; { let item = 1; let _ = {item} }; {item} }", "let number: Number = copy(7)\nlet text: Text = copy(\"value\")");
    }

    #[test]
    fn generic_record_forms_shorthand_retains_an_immutable_capture() {
        accepts("let shared = 1\ntype Number = {value: Int, shared: Int}\ntype Text = {value: Str, shared: Int}\npure copy(value) { {value, shared} }", "let number: Number = copy(7)\nlet text: Text = copy(\"value\")");
    }

    #[test]
    fn generic_record_forms_map_shorthand_keeps_the_payload_domain() {
        accepts("pure copy(value) { {[\"other\"]: value, value} }", "let number: Map[Str, Int] = copy(7)\nlet text: Map[Str, Str] = copy(\"value\")");
    }

    #[test]
    fn generic_record_forms_set_shorthand_keeps_the_checked_domain() {
        accepts("pure keep(value, first: Int, second: Int) { let items: Set[Int] = {first, second}; let _ = items; value }", "let number: Int = keep(7, 1, 2)\nlet text: Str = keep(\"value\", 2, 3)");
    }

    #[test]
    fn generic_record_forms_nested_updates_preserve_the_payload_binder() {
        accepts("type NumberInner = {item: Int, count: Int}\ntype TextInner = {item: Str, count: Int}\ntype Number = {nested: NumberInner, other: Int}\ntype Text = {nested: TextInner, other: Str}\npure copy(value) { let row = {nested: {item: value, count: 0}, other: value}; {...row, nested.item: value, nested.count: 1} }", "let number: Number = copy(7)\nlet text: Text = copy(\"value\")");
    }

    #[test]
    fn generic_record_forms_typed_updates_preserve_the_existing_shape() {
        accepts("type Inner = {count: Int}\ntype Row = {nested: Inner}\npure keep(value, row: Row) { let updated = {...row, nested.count: 2}; let _ = updated; value }", "let number: Int = keep(7, {nested: {count: 0}})\nlet text: Str = keep(\"value\", {nested: {count: 1}})");
    }

}

#[cfg(test)]
mod tests {
    use super::supports_generalized_body;
    use crate::sema::check::{BindingId, FxHashSet, ResolvedBindings};
    use crate::source::SourceId;
    use crate::syntax::arena::{ArenaCallArgKind, ArenaExprKind, ArenaStmtKind};
    use crate::syntax::parser::Parser;

    fn accepts(source: &str, target_function: &str) -> bool {
        accepts_with_generic_calls(source, target_function, &FxHashSet::default())
    }

    fn accepts_with_generic_calls(
        source: &str,
        target_function: &str,
        generic_checked_calls: &FxHashSet<crate::source::Span>,
    ) -> bool {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let program = &parsed.arena;
            let function = program
                .arena
                .stmt_ids(program.statements)
                .find_map(|statement| match program.arena.stmt(statement).kind {
                    ArenaStmtKind::PureDef(function)
                        if program.arena.function_def(function).name.as_str() == target_function =>
                    {
                        Some(function)
                    }
                    _ => None,
                })
                .expect("target pure function declaration");
            let definition = program.arena.function_def(function);
            let bindings = ResolvedBindings::collect(program);
            let parameters = program
                .arena
                .params(definition.params)
                .iter()
                .filter_map(|parameter| {
                    bindings
                        .declaration_sites
                        .get(&(program.arena.span(parameter.span), parameter.name))
                        .copied()
                })
                .collect::<FxHashSet<BindingId>>();
            supports_generalized_body(
                program,
                &crate::sema::constants::PreparedConstants::default(),
                &std::collections::BTreeMap::new(),
                definition.body,
                &bindings,
                &parameters,
                &FxHashSet::default(),
                &std::collections::BTreeMap::new(),
                &std::collections::BTreeMap::new(),
                generic_checked_calls,
                &std::collections::BTreeMap::new(),
                &std::collections::BTreeMap::new(),
                &std::collections::BTreeMap::new(),
                &std::collections::BTreeMap::new(),
                super::BodyCompletion::Function,
            )
        })
    }

    #[test]
    fn accepts_parameter_reads_and_required_projection_chains() {
        assert!(accepts("pure identity(value: Int) { value }\n", "identity"));
        assert!(accepts(
            "type Inner = { name: Str }\ntype Node = { inner: Inner }\npure project(value: Node) { value.inner.name }\n",
            "project"
        ));
    }

    #[test]
    fn accepts_constructed_and_branching_values_but_rejects_unchecked_operations() {
        assert!(accepts(
            "pure duplicate(value: Int) { [value, value] }\n",
            "duplicate"
        ));
        assert!(accepts(
            "pure construct(value: Int) { { first: value, second: value } }\n",
            "construct"
        ));
        assert!(accepts(
            "pure choose(value: Int) { if true { value } else { value } }\n",
            "choose"
        ));
        assert!(!accepts("pure index(value: Int) { value[0] }\n", "index"));
        assert!(!accepts(
            "pure calculate(value: Int) { value + 1 }\n",
            "calculate"
        ));
        assert!(!accepts(
            "pure inner(value: Int) { value }\npure nested(value: Int) { inner(value) }\n",
            "nested"
        ));
    }

    #[test]
    fn accepts_direct_generic_pure_composition_with_static_call_evidence() {
        let source = "pure identity(value) { value }\npure name_of(value) { value.name }\npure project_identity(value) { identity(name_of(value)) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let program = &parsed.arena;
            let definition = program
                .arena
                .stmt_ids(program.statements)
                .find_map(|statement| match program.arena.stmt(statement).kind {
                    ArenaStmtKind::PureDef(function)
                        if program.arena.function_def(function).name.as_str()
                            == "project_identity" =>
                    {
                        Some(program.arena.function_def(function))
                    }
                    _ => None,
                })
                .expect("target pure function declaration");
            let statement = program
                .arena
                .stmt_ids(program.arena.block(definition.body).statements)
                .next()
                .expect("tail expression");
            let ArenaStmtKind::Expr(outer) = program.arena.stmt(statement).kind else {
                panic!("tail call expression")
            };
            let ArenaExprKind::Call { args, .. } = program.arena.expr(outer).kind else {
                panic!("outer generic call")
            };
            let [argument] = program.arena.call_args(args) else {
                panic!("single composition argument")
            };
            let ArenaCallArgKind::Positional(inner) = argument.kind else {
                panic!("inner generic call argument")
            };
            let mut generic_checked_calls = FxHashSet::default();
            generic_checked_calls.insert(program.arena.expr(outer).span);
            generic_checked_calls.insert(program.arena.expr(inner).span);
            assert!(accepts_with_generic_calls(
                source,
                "project_identity",
                &generic_checked_calls,
            ));
            assert!(!accepts(source, "project_identity"));
        });
    }

    #[test]
    fn accepts_immutable_prefix_and_explicit_return_but_rejects_unproved_capture() {
        assert!(!accepts(
            "var captured = 1\npure read(value: Int) { captured }\n",
            "read"
        ));
        assert!(accepts(
            "pure prefixed(value: Int) { let copy = value\nvalue }\n",
            "prefixed"
        ));
        assert!(accepts(
            "pure returned(value: Int) { return value }\n",
            "returned"
        ));
    }
}
