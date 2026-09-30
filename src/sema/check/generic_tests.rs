use super::Checker;
use crate::source::SourceId;
use crate::syntax::parser::Parser;
use crate::sema::inference::{Atom, TypeNode};

#[test]
fn written_result_normalizes_each_completion_before_generalization() {
    let source = "pure choose(value: Int = 0) -> Result[Int] { if value == 0 { 1 } else { Ok(value) } }\npure early(value: Int) -> Result[Int] { if value == 0 { return 1 }\nOk(value) }\npure bare(value: Int) -> Result[Int] { value }\nlet first: Result[Int] = choose()\nlet second: Result[Int] = early(2)\nlet third: Result[Int] = bare(3)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    for declaration in checked.solved.declarations.values() {
        assert_eq!(declaration.return_elaboration, super::ReturnElaboration::Value);
    }
    assert_eq!(checked.solved.result_wrappings.len(), 2);
    assert_eq!(checked.solved.result_statement_wrappings.len(), 1);
    checked.solved.validate().unwrap();
}

#[test]
fn identity_definition_is_independent_of_callers() {
    let source = "pure identity(value) { value }\nlet integer: Int = identity(7)\nlet text: Str = identity(\"seven\")\nlet boolean: Bool = identity(false)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.solved.declarations.len(), 1);
    assert_eq!(checked.solved.calls.len(), 3);
    assert_eq!(checked.solved.graph.counters().instantiations, 3);
    let declaration = checked.solved.declarations.values().next().unwrap();
    let scheme = checked.solved.graph.scheme(declaration.scheme).unwrap();
    assert_eq!(scheme.quantifiers.len(), 1);
    let TypeNode::Arrow(arrow) = checked.solved.graph.node(scheme.body).unwrap() else { panic!("callable scheme must retain an arrow") };
    assert_eq!(arrow.params[0].ty, arrow.result);
    let mut actual = Vec::new();
    for call in checked.solved.calls.values() {
        assert_eq!(call.substitutions.len(), 1);
        let ty = checked.solved.graph.resolved(call.substitutions[0]).unwrap();
        actual.push(checked.solved.graph.node(ty).unwrap().clone());
    }
    assert_eq!(actual, vec![TypeNode::Atom(Atom::Int), TypeNode::Atom(Atom::Str), TypeNode::Atom(Atom::Bool)]);
    checked.solved.validate().unwrap();
}

#[test]
fn annotated_equivalents_publish_the_same_graph_operations() {
    let source = "type Entry = {name: Str}\npure identity(value: Int) -> Int { value }\npure name(value: Entry) -> Str { value.name }\npure plus(left: Int, right: Int) -> Int { left + right }\nlet integer: Int = identity(7)\nlet text: Str = name({name: \"n\", extra: false})\nlet sum: Int = plus(2, 3)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.solved.declarations.len(), 3);
    assert_eq!(checked.solved.calls.len(), 3);
    assert_eq!(checked.solved.projections.len(), 1);
    assert_eq!(checked.solved.additions.len(), 1);
    assert_eq!(checked.solved.graph.counters().instantiations, 3);
    assert!(checked.solved.declarations.values().all(|declaration| checked.solved.graph.scheme(declaration.scheme).unwrap().quantifiers.is_empty()));
    checked.solved.validate().unwrap();
}

#[test]
fn ground_addition_settles_before_ordinary_method_consumption() {
    let source = "pure width(value: List[Int]) -> Int { let joined = value + [2]; joined.len() }\nlet value: Int = width([1])\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}; expressions: {:?}", checked.diagnostics, checked.expr_types);
}

#[test]
fn legacy_empty_record_field_is_anchored_by_ground_callable_contract() {
    let source = "type Stats = {blobs: Map[Any]}\npure with_blobs(stats: Stats, blobs: Map[Any]) -> Stats { {blobs} }\npure count() -> Stats { let stats = {blobs: map.empty()}; let blobs: Map[Any] = map.empty(); with_blobs(stats, blobs) }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.solved.declarations.len(), 1);
    assert_eq!(checked.solved.calls.len(), 1);
    let call = checked.solved.calls.values().next().unwrap();
    let actual = checked.solved.graph.export_type(call.actual_arguments[0]).unwrap();
    let super::Type::Record(fields) = actual else { panic!("actual argument retains its record shape") };
    assert_eq!(fields.len(), 1);
    assert_eq!(fields.values().next().unwrap(), &super::Type::Map(Box::new(super::Type::Str), Box::new(super::Type::Any)));
    checked.solved.validate().unwrap();
}

#[test]
fn open_projection_retains_actual_record_layout_per_call() {
    let source = "pure name(entry) { entry.name }\nlet narrow: Str = name({name: \"n\"})\nlet wide: Int = name({extra: false, name: 4})\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.solved.projections.len(), 1);
    assert_eq!(checked.solved.calls.len(), 2);
    let widths: Vec<_> = checked.solved.calls.values().map(|call| {
        let ty = checked.solved.graph.resolved(call.actual_arguments[0]).unwrap();
        let TypeNode::Record(row) = checked.solved.graph.node(ty).unwrap() else { panic!("record argument") };
        checked.solved.graph.row_data(*row).unwrap().fields.len()
    }).collect();
    assert_eq!(widths, vec![1, 2]);
    checked.solved.validate().unwrap();
}

#[test]
fn record_arguments_keep_literal_constructor_facts_before_binding() {
    let source = "pure name(entry) { entry.person.name }\nlet entry = {person: {age: 7, name: \"n\"}, extra: false}\nlet value: Str = name(entry)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let constructors = checked.solved.expressions.keys().filter(|identity| matches!(parsed.arena.arena.expr(identity.expression).kind, crate::syntax::arena::ArenaExprKind::Record(_))).count();
    assert_eq!(constructors, 2);
    checked.solved.validate().unwrap();
}

#[test]
fn sealed_add_is_instantiated_without_rechecking_body() {
    let source = "pure add(left, right) { left + right }\nlet integer: Int = add(2, 3)\nlet floating: Float = add(2.0, 3.0)\nlet text: Str = add(\"a\", \"b\")\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.solved.additions.len(), 1);
    assert_eq!(checked.solved.calls.len(), 3);
    assert_eq!(checked.solved.graph.counters().instantiations, 3);
    for call in checked.solved.calls.values() {
        assert_eq!(call.requirements.len(), 1);
        assert!(checked.solved.graph.discharge(call.requirements[0]).unwrap().is_some());
    }
    checked.solved.validate().unwrap();
}

#[test]
fn omitted_bool_and_quantified_tails_have_fixed_value_elaboration() {
    let source = "proc answer() { false }\npure identity(value) { value }\nlet returned: Bool = answer()\nlet payload: Result[Int] = identity(Ok(7))\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.solved.declarations.len(), 2);
    for declaration in checked.solved.declarations.values() {
        assert_eq!(declaration.return_elaboration, super::ReturnElaboration::Value);
    }
    assert!(checked.assertion_spans.is_empty());
}

#[test]
fn generic_non_tail_value_requires_explicit_discard() {
    let rejected = "pure consume(value) { (value)\n1 }\nlet result: Int = consume(false)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), rejected);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, rejected);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.non-tail-expression")), "{:?}", checked.diagnostics);
    let accepted = "pure consume(value) { let _ = value\n1 }\nlet result: Int = consume(false)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), accepted);
    let checked = Checker::check_arena(&parsed.arena, accepted);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
}

#[test]
fn result_wrapper_is_chosen_before_payload_instantiation() {
    let source = "proc wrap(value, gate: Result[Unit]) { gate?\nvalue }\nlet nested: Result[Result[Int]] = wrap(Ok(7), Ok())\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declaration = checked.solved.declarations.values().next().unwrap();
    assert_eq!(declaration.return_elaboration, super::ReturnElaboration::ImplicitResult);
    let scheme = checked.solved.graph.scheme(declaration.scheme).unwrap();
    let TypeNode::Arrow(arrow) = checked.solved.graph.node(scheme.body).unwrap() else { panic!("arrow") };
    assert!(matches!(checked.solved.graph.node(arrow.result).unwrap(), TypeNode::Result(_, _)));
}

#[test]
fn forwarding_preserves_row_and_operation_requirements() {
    let source = "pure name(entry) { entry.person.name }\npure forward(entry) { name(entry) }\npure add(left, right) { left + right }\npure plus(left, right) { add(left, right) }\nlet text: Str = forward({person: {extra: 7, name: \"n\"}, outer: false})\nlet integer: Int = plus(2, 3)\nlet floating: Float = plus(2.0, 3.0)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.solved.declarations.len(), 4);
    assert_eq!(checked.solved.projections.len(), 2);
    assert_eq!(checked.solved.additions.len(), 1);
    assert_eq!(checked.solved.calls.len(), 5);
    assert_eq!(checked.solved.graph.counters().instantiations, 5);
    checked.solved.validate().unwrap();
}

#[test]
fn callers_cannot_train_missing_fields_or_add_domains() {
    for source in [
        "pure name(entry) { entry.name }\nlet bad = name({other: 1})\n",
        "pure add(left, right) { left + right }\nlet bad = add(true, false)\n",
        "pure add(left, right) { left + right }\nlet bad = add(1, 2.0)\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-relationship")), "{source}: {:?}", checked.diagnostics);
    }
}

#[test]
fn written_result_payload_selects_wrapping_at_the_declaration() {
    for (source, plan) in [
        ("pure wrap(value: Int, unused) -> Result[Int] { value }\nlet result: Result[Int] = wrap(7, false)\n", super::ReturnElaboration::Value),
        ("pure wrap(value: Result[Int], unused) -> Result[Int] { value }\nlet result: Result[Int] = wrap(Ok(7), false)\n", super::ReturnElaboration::Value),
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        assert_eq!(checked.solved.declarations.values().next().unwrap().return_elaboration, plan);
    }
}

#[test]
fn unique_builtin_method_constrains_the_definition_receiver() {
    let source = "pure parse(value) { value.parse_int()? }\nlet result: Result[Int] = parse(\"7\")\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declaration = checked.solved.declarations.values().next().unwrap();
    let scheme = checked.solved.graph.scheme(declaration.scheme).unwrap();
    assert!(scheme.quantifiers.is_empty());
    let TypeNode::Arrow(arrow) = checked.solved.graph.node(scheme.body).unwrap() else { panic!("arrow") };
    assert!(matches!(checked.solved.graph.node(checked.solved.graph.resolved(arrow.params[0].ty).unwrap()).unwrap(), TypeNode::Atom(Atom::Str)));
    assert_eq!(declaration.return_elaboration, super::ReturnElaboration::ImplicitResult);
}

#[test]
fn self_recursion_forwards_its_declaration_binders() {
    let source = "pure repeat(value, count: Int) { if count == 0 { return value }\nrepeat(value, count - 1) }\nlet number: Int = repeat(7, 2)\nlet text: Str = repeat(\"n\", 2)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let (owner, declaration) = checked.solved.declarations.iter().next().unwrap();
    let binders = checked.solved.graph.scheme_type_binders(declaration.scheme).unwrap();
    assert_eq!(binders.len(), 1);
    let call = checked.solved.calls.values().find(|call| call.caller == Some(*owner)).unwrap();
    assert_eq!(call.substitutions, binders);
    assert_eq!(checked.solved.graph.counters().instantiations, 2);
    checked.solved.validate().unwrap();
}

#[test]
fn recursive_branch_tails_share_the_definition_relationship() {
    let source = "pure repeat(value, count: Int) { if count == 0 { value } else { repeat(value, count - 1) } }\nprint ${repeat(7, 3)} ${repeat(false, 2)}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    checked.solved.validate().unwrap();
}
