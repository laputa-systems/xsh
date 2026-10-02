use super::{Checker, ExpressionIdentity};
use crate::sema::arguments::ArgumentValueSource;
use crate::sema::inference::{InvocationPlan, RequirementTemplate};
use crate::source::SourceId;
use crate::syntax::arena::{ArenaCallArgKind, ArenaExprKind};
use crate::syntax::parser::Parser;

#[test]
fn checked_argument_recipes_keep_original_spread_entries_and_dynamic_branch_indices() {
    let source = "type Pair = {left: Int, right: Int = 2}\npure pair(first: Int, second: Int = 2) -> Int { first + second }\npure rested(first: Int = 3, ...others: List[Int]) -> Int { first }\npure choose(flag: Bool) { if flag { (pair) } else { (rested) } }\nlet fields = {second: 9, first: 7}\nlet value = pair(...fields)\nlet defaulted = pair(first: 7)\nlet parts = [7]\nlet callback = choose(true)\nlet dynamic = callback(@parts, first: 9)\nlet pushed = [\"first\"].push(...{item: \"next\"})\nlet record = Pair(...{left: 7})\nlet ok = Ok(value)\nlet empty = Ok()\nlet ranged = range(2)\nlet destination = Path(\"word\")\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(96), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let identity = |text: &str| {
        *checked.solved.expressions.keys().find(|identity| &source[parsed.arena.arena.expr(identity.expression).span.range()] == text).unwrap()
    };
    let spread = identity("pair(...fields)");
    let defaulted = identity("pair(first: 7)");
    let dynamic = identity("callback(@parts, first: 9)");
    let method = identity("[\"first\"].push(...{item: \"next\"})");
    let record = identity("Pair(...{left: 7})");
    let constructors = [identity("Ok(value)"), identity("Ok()"), identity("range(2)"), identity("Path(\"word\")")];
    let call_arguments = |identity: ExpressionIdentity| {
        let ArenaExprKind::Call { args, .. } = parsed.arena.arena.expr(identity.expression).kind else { panic!() };
        parsed.arena.arena.call_args(args)
    };
    let ArenaCallArgKind::NamedSpread { value: spread_record, span: spread_span } = call_arguments(spread)[0].kind else { panic!() };
    let spread_span = parsed.arena.arena.span(spread_span);
    let ArenaCallArgKind::Splice { value: parts, span: splice_span } = call_arguments(dynamic)[0].kind else { panic!() };
    let splice_span = parsed.arena.arena.span(splice_span);
    let ArenaCallArgKind::Named { name: first, value: explicit, span: explicit_span } = call_arguments(dynamic)[1].kind else { panic!() };
    let explicit_span = parsed.arena.arena.span(explicit_span);
    drop(parsed);
    let counters = checked.solved.graph.counters().clone();
    checked.solved.validate().unwrap();

    let recipes = &checked.solved.argument_sources[&spread];
    assert_eq!(recipes.len(), 2);
    assert!(recipes.iter().all(|argument| argument.entry_index == 0 && argument.span == spread_span));
    assert_eq!(recipes.iter().map(|argument| argument.name.unwrap().as_str().to_string()).collect::<Vec<_>>(), ["first", "second"]);
    for argument in recipes {
        assert_eq!(argument.value, ArgumentValueSource::RecordField { record: spread_record, field: argument.name.unwrap() });
    }
    assert_eq!(checked.solved.calls[&spread].binding.supplied_slots, [0, 1]);
    assert_eq!(checked.solved.argument_sources[&defaulted].len(), 1);
    assert_eq!(checked.solved.calls[&defaulted].binding.default_slots, [1]);
    let recipes = &checked.solved.argument_sources[&dynamic];
    assert_eq!(recipes.len(), 2);
    assert_eq!((recipes[0].entry_index, recipes[0].name, recipes[0].value, recipes[0].span), (0, None, ArgumentValueSource::PositionalSplice(parts), splice_span));
    assert_eq!((recipes[1].entry_index, recipes[1].name, recipes[1].value, recipes[1].span), (1, Some(first), ArgumentValueSource::Expression(explicit), explicit_span));
    let invocation = &checked.solved.invocations[&dynamic];
    let RequirementTemplate::CallableInvocation { call } = checked.solved.graph.requirement_template(invocation.requirement).unwrap() else { panic!() };
    assert_eq!(checked.solved.graph.invocation_call(call).unwrap().arguments.len(), recipes.len());
    let InvocationPlan::All { branches } = &checked.solved.graph.invocation_evidence(invocation.requirement).unwrap().unwrap().plan else { panic!() };
    assert_eq!(branches.len(), 2);
    assert!(branches.iter().all(|branch| branch.binding.dynamic.as_ref().unwrap().segments.iter().all(|segment| match segment {
        crate::sema::inference::InvocationArgumentSegment::StaticSlot { argument, .. }
        | crate::sema::inference::InvocationArgumentSegment::DynamicRange { argument, .. } => *argument < recipes.len(),
    })));
    assert!(matches!(checked.solved.argument_sources[&method][0].value, ArgumentValueSource::RecordField { .. }));
    assert!(matches!(checked.solved.argument_sources[&record][0].value, ArgumentValueSource::RecordField { .. }));
    for constructor in constructors { assert!(checked.solved.argument_sources.contains_key(&constructor)); }
    assert!(checked.solved.argument_sources[&constructors[1]].is_empty(), "zero supplied arguments have a retained empty recipe");
    assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
    assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
}
