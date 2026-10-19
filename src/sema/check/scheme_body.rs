use super::{BindingId, FxHashSet, ResolvedBindings};
use crate::source::Span;
use crate::syntax::arena::{ArenaCallArgKind, ArenaExprKind, ArenaProgram, ArenaStmtKind, BlockId};

#[derive(Clone, Copy)]
pub(super) enum BodyCompletion { Function, Value }

pub(super) fn supports_generalized_body(
    program: &ArenaProgram,
    body: BlockId,
    bindings: &ResolvedBindings,
    parameters: &FxHashSet<BindingId>,
    captures: &FxHashSet<BindingId>,
    tails: &std::collections::BTreeMap<crate::syntax::arena::StmtId, crate::sema::schemes::SchemeType>,
    positions: &std::collections::BTreeMap<Span, super::StatementPosition>,
    checked_calls: &FxHashSet<Span>,
    registered_calls: &std::collections::BTreeMap<crate::syntax::arena::ExprId, std::sync::Arc<crate::sema::schemes::RegisteredCallPlan>>,
    types: &std::collections::BTreeMap<crate::syntax::arena::ExprId, crate::sema::schemes::SchemeType>,
    result_constructors: &std::collections::BTreeMap<Span, super::CheckedResultConstructor>,
    completion: BodyCompletion,
) -> bool {
    use crate::syntax::arena::{ArenaBindingTargetKind, ArenaExprOrRun};
    let block = program.arena.block(body);
    if !program.arena.block_params(block.params).is_empty() { return false; }
    let statements = program.arena.stmt_ids(block.statements).collect::<Vec<_>>();
    let Some((&tail, prefix)) = statements.split_last() else { return false };
    let mut available = parameters.clone();
    for &statement in prefix {
        let ArenaStmtKind::Let { target, initializer: ArenaExprOrRun::Expr(value), .. } = program.arena.stmt(statement).kind else { return false };
        let ArenaBindingTargetKind::Name(name) = program.arena.binding_target(target).kind else { return false };
        if !is_body_expression(program, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, result_constructors, value) { return false; }
        let Some(binding) = bindings.declaration_sites.get(&(program.arena.stmt(statement).span, name)) else { return false };
        available.insert(*binding);
    }
    let tail_statement = program.arena.stmt(tail);
    if positions.get(&tail_statement.span) == Some(&super::StatementPosition::Statement) {
        match tail_statement.kind {
            ArenaStmtKind::Expr(expression) if types.get(&expression).is_some_and(|ty| ty.closed_ground_type() == Some(crate::sema::types::Type::Unit)) => {}
            ArenaStmtKind::Expr(_) | ArenaStmtKind::TailBareIdent(_) => return false,
            _ => {}
        }
    }
    match tail_statement.kind {
        ArenaStmtKind::TailBareIdent(_) => bindings.tails.get(&tail).is_some_and(|identity| available.contains(identity) || captures.contains(identity) && tails.get(&tail).is_some_and(|ty| ty.closed_ground_type().is_some_and(|ty| !ty.may_hold_resource()))),
        ArenaStmtKind::Expr(expression) => is_body_expression(program, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, result_constructors, expression),
        ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expression))) if matches!(completion, BodyCompletion::Function) => is_body_expression(program, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, result_constructors, expression),
        ArenaStmtKind::If { branches, else_block: Some(else_block) } => {
            program.arena.if_branches(branches).iter().all(|branch| {
                is_body_expression(program, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, result_constructors, branch.condition)
                    && supports_generalized_body(program, branch.block, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, result_constructors, completion)
            }) && supports_generalized_body(program, else_block, bindings, &available, captures, tails, positions, checked_calls, registered_calls, types, result_constructors, completion)
        }
        _ => false,
    }
}

fn is_body_expression(
    program: &ArenaProgram,
    bindings: &ResolvedBindings,
    available: &FxHashSet<BindingId>,
    captures: &FxHashSet<BindingId>,
    tails: &std::collections::BTreeMap<crate::syntax::arena::StmtId, crate::sema::schemes::SchemeType>,
    positions: &std::collections::BTreeMap<Span, super::StatementPosition>,
    checked_calls: &FxHashSet<Span>,
    registered_calls: &std::collections::BTreeMap<crate::syntax::arena::ExprId, std::sync::Arc<crate::sema::schemes::RegisteredCallPlan>>,
    types: &std::collections::BTreeMap<crate::syntax::arena::ExprId, crate::sema::schemes::SchemeType>,
    result_constructors: &std::collections::BTreeMap<Span, super::CheckedResultConstructor>,
    expression: crate::syntax::arena::ExprId,
) -> bool {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        match program.arena.expr(expression).kind {
            ArenaExprKind::Ident(_) => {
                if !bindings.expressions.get(&expression).is_some_and(|binding| available.contains(binding)
                    || captures.contains(binding) && types.get(&expression).is_some_and(|ty| ty.closed_ground_type().is_some_and(|ty| !ty.may_hold_resource()))) { return false; }
            }
            ArenaExprKind::Null | ArenaExprKind::Bool(_) | ArenaExprKind::Int(_) | ArenaExprKind::Float(_) | ArenaExprKind::Str(_) | ArenaExprKind::Bytes(_) | ArenaExprKind::Duration(_) | ArenaExprKind::Regex(_) | ArenaExprKind::PathStr(_) => {}
            ArenaExprKind::Field { base, .. } => pending.push(base),
            ArenaExprKind::List(items) => {
                for item in program.arena.list_elements(items) {
                    if item.splice_span.is_some() { return false; }
                    pending.push(item.value);
                }
            }
            ArenaExprKind::Record(fields) => {
                for field in program.arena.record_fields(fields) {
                    let crate::syntax::arena::ArenaRecordFieldKind::Named { value, .. } = field.kind else { return false; };
                    pending.push(value);
                }
            }
            ArenaExprKind::If { branches, else_value } => {
                for branch in program.arena.if_expr_branches(branches) { pending.push(branch.condition); pending.push(branch.value); }
                pending.push(else_value);
            }
            ArenaExprKind::ValueBlock(block) => {
                if !supports_generalized_body(program, block, bindings, available, captures,
                    tails, positions, checked_calls, registered_calls, types, result_constructors, BodyCompletion::Value)
                { return false; }
            }

            ArenaExprKind::Call { callee, args } => {
                if let Some(plan) = registered_calls.get(&expression) {
                    if plan.receiver.is_some() {
                        let ArenaExprKind::Field { base, .. } = program.arena.expr(callee).kind else { return false };
                        pending.push(base);
                    }
                } else if !checked_calls.contains(&program.arena.expr(expression).span)
                    && !result_constructors.contains_key(&program.arena.expr(expression).span) { return false; }
                for argument in program.arena.call_args(args) {
                    match argument.kind {
                        ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. } => pending.push(value),
                        _ => return false,
                    }
                }
            }
            _ => return false,
        }
    }
    true
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
