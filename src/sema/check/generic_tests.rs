use super::Checker;
use crate::source::SourceId;
use crate::syntax::parser::Parser;
use crate::sema::inference::{Atom, TypeNode};

thread_local! {
    static SOURCE_CHECK_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static SOURCE_EXPRESSION_VISITS: std::cell::RefCell<Option<std::collections::BTreeMap<crate::source::Span, usize>>> = const { std::cell::RefCell::new(None) };
}

pub(super) fn record_source_check_pass() {
    SOURCE_CHECK_PASSES.with(|passes| passes.set(passes.get() + 1));
}

pub(super) fn record_source_expression_visit(span: crate::source::Span) {
    SOURCE_EXPRESSION_VISITS.with(|visits| {
        if let Some(visits) = visits.borrow_mut().as_mut() { *visits.entry(span).or_default() += 1; }
    });
}

#[test]
fn record_constructor_named_spread_checks_the_original_value_once() {
    let source = "type Pair = {left: Int, right: Int}\npure pair(left: Int) -> Pair { Pair(...{left: left}, right: 2) }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let start = source.find("{left: left}").unwrap();
    let spread = crate::source::Span::new(SourceId::new(0), start, start + "{left: left}".len());
    SOURCE_EXPRESSION_VISITS.with(|visits| *visits.borrow_mut() = Some(std::collections::BTreeMap::new()));
    let checked = Checker::check_arena(&parsed.arena, source);
    let visits = SOURCE_EXPRESSION_VISITS.with(|visits| visits.borrow_mut().take().unwrap());
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(visits.get(&spread), Some(&1), "the original spread value supplies checked fields without a second semantic pass");
    checked.solved.validate().unwrap();
}

#[test]
fn record_constructor_arguments_retain_nested_producer_permissions() {
    for arguments in ["rows: rows", "...{rows: rows}"] {
        let prefix = format!("type Envelope = {{rows: Stream[Int]}}\nstream delayed() [time] -> Stream[Int] {{ let _ = time.now(); yield 7 }}\npure wrap(rows: Stream[Int]) -> Envelope {{ Envelope({arguments}) }}\nlet retained = wrap(delayed())\n");
        let source = format!("{prefix}proc accepted() [time] -> List[Int] {{ retained.rows.collect() }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        assert_eq!(checked.solved.constructor_applications.len(), 1);
        let identity = *checked.solved.constructor_applications.keys().next().unwrap();
        let flow = checked.solved.expression_producer_flows[&identity];
        assert!(matches!(&checked.solved.producer_flows.node(flow).unwrap().kind, super::ProducerFlowKind::Aggregate { entries } if entries.len() == 1 && matches!(entries[0].path.0.as_slice(), [super::ProducerPathComponent::RecordField(field)] if field.as_str() == "rows")));
        drop(parsed);
        checked.solved.validate().unwrap();
        let source = format!("{prefix}proc denied() [] -> List[Int] {{ retained.rows.collect() }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{source}: {:?}", checked.diagnostics);
    }
}

#[test]
fn declaration_effects_and_prepared_facts_come_from_one_source_check_pass() {
    let source = "proc leaf() -> Int { time.now() }\nproc caller() [time] -> Int { leaf() }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    SOURCE_CHECK_PASSES.with(|passes| passes.set(0));
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(SOURCE_CHECK_PASSES.with(std::cell::Cell::get), 1);
    assert_eq!(checked.callable_effects["leaf"], Some(vec![super::Effect::Time]));
    assert_eq!(checked.function_effect_facts.len(), 2);
    checked.solved.validate().unwrap();
}

#[test]
fn producer_authored_item_contract_owns_omitted_parameters() {
    let source = "stream repeat(item) [] -> Stream[Int] { yield item }\nlet integers: Stream[Int] = repeat(7)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.solved.declarations.len(), 1);
    assert_eq!(checked.solved.calls.len(), 1);
    let declaration = checked.solved.declarations.values().next().unwrap();
    let TypeNode::Arrow(arrow) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("producer keeps its complete authored item signature") };
    assert_eq!(checked.solved.graph.export_type(arrow.params[0].ty).unwrap(), super::Type::Int);
    assert_eq!(checked.solved.graph.export_type(arrow.result).unwrap(), super::Type::Stream(Box::new(super::Type::Int)));
    checked.solved.validate().unwrap();
}

#[test]
fn producer_item_contract_is_solved_from_yields_before_caller_instantiation() {
    let source = "stream repeat(item) { yield item }\nlet integers: Stream[Int] = repeat(7)\nlet texts: Stream[Str] = repeat(\"word\")\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.solved.declarations.len(), 1);
    assert_eq!(checked.solved.calls.len(), 2);
    let declaration = checked.solved.declarations.values().next().unwrap();
    let scheme = checked.solved.graph.scheme(declaration.scheme).unwrap();
    assert_eq!(scheme.quantifiers.len(), 1);
    let TypeNode::Arrow(arrow) = checked.solved.graph.node(scheme.body).unwrap() else { panic!("producer retains its item and parameter relationship") };
    assert_eq!(arrow.kind, crate::sema::inference::CallableKind::Stream);
    let TypeNode::Stream(item) = checked.solved.graph.node(arrow.result).unwrap() else { panic!("producer result retains Stream") };
    assert_eq!(arrow.params[0].ty, *item);
    assert_eq!(declaration.effective_effects, crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY));
    checked.solved.validate().unwrap();
}

#[test]
fn native_test_declarations_publish_the_fixed_statement_consumer_contract() {
    let source = "test statement_contract [error] { |ctx| true }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.solved.declarations.len(), 1);
    let declaration = checked.solved.declarations.values().next().unwrap();
    assert_eq!(declaration.return_elaboration, super::ReturnElaboration::UnitConsuming);
    let TypeNode::Arrow(arrow) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("native contract retains its complete arrow") };
    assert_eq!(arrow.kind, crate::sema::inference::CallableKind::Proc);
    assert!(checked.solved.graph.export_type(arrow.result).unwrap().is_result_unit());
    assert_eq!(checked.assertion_spans.len(), 1);
    checked.solved.validate().unwrap();
}

#[test]
fn recursive_component_forwards_each_called_operation_requirement() {
    let declarations = "pure first(value, again: Bool) { second(value, again) }\npure second(value, again: Bool) { if again { first(value, false) } else { value + value } }\n";
    for (argument, valid) in [("true", false), ("7", true)] {
        let source = format!("{declarations}let output: Int = first({argument}, false)\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        if valid {
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            for declaration in checked.solved.declarations.values() {
                assert_eq!(checked.solved.graph.scheme(declaration.scheme).unwrap().requirements.len(), 1);
            }
            checked.solved.validate().unwrap();
        } else {
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.message.contains("does not support operand domains Bool")), "{:?}", checked.diagnostics);
        }
    }
}

#[test]
fn nominal_graph_facts_retain_the_registered_declaration_identity() {
    let source = "enum Token { Present(Str) }\ntype Alias = Token\nerror Failure = Failed(message: Str)\npure keep(value: Alias) -> Token { value }\npure preserve(value: Failure) -> Failure { value }\nlet token = keep(Present(\"word\"))\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let mut types = Vec::new();
    let mut errors = Vec::new();
    let mut error_members = Vec::new();
    for (ty, identity) in &checked.solved.nominals {
        let super::QualifiedNominalIdentity::Source { source, namespace, declaration, member } = identity else { continue; };
        assert_eq!(*source, SourceId::new(0));
        assert_eq!(*namespace, None);
        match *declaration {
            super::NominalDeclaration::Type(id) => {
                assert_eq!(parsed.arena.arena.type_def(id).name, "Token");
                assert!(matches!(checked.solved.graph.node(*ty).unwrap(), TypeNode::Atom(Atom::Tag(name)) if name == "Token"));
                types.push(*ty);
            }
            super::NominalDeclaration::Error(id) => {
                assert_eq!(parsed.arena.arena.error_def(id).name, "Failure");
                if let Some(member) = member {
                    let receipt = checked.solved.checked_nominal_member(*identity).unwrap();
                    assert_eq!(receipt.kind, super::NominalMemberKind::Error);
                    assert_eq!(receipt.family, "Failure");
                    assert_eq!(receipt.member, *member);
                    assert_eq!(receipt.member, "Failed");
                    assert!(matches!(checked.solved.graph.node(*ty).unwrap(), TypeNode::Atom(Atom::ErrorVariant { family, variant }) if *family == receipt.family && *variant == receipt.member));
                    assert_eq!(checked.solved.graph.resolved(*ty).unwrap(), checked.solved.graph.resolved(receipt.tested).unwrap());
                    assert_eq!(receipt.fields.len(), 1);
                    assert_eq!(receipt.fields[0].0.unwrap(), "message");
                    assert_eq!(checked.solved.graph.node(receipt.fields[0].1).unwrap(), &TypeNode::Atom(Atom::Str));
                    error_members.push(*ty);
                } else {
                    assert!(matches!(checked.solved.graph.node(*ty).unwrap(), TypeNode::Atom(Atom::ErrorFamily(name)) if name == "Failure"));
                    errors.push(*ty);
                }
            }
        }
    }
    assert_eq!(types.len(), 1, "alias resolves to the original nominal declaration");
    assert_eq!(errors.len(), 1);
    assert_eq!(error_members.len(), 1, "the unused member retains its independent original registration");
    checked.solved.validate().unwrap();
}

#[test]
fn transparent_aliases_retain_the_original_definition_scheme() {
    let source = "pure identity(value) { value }\nlet alias = identity\nlet second = alias\nlet integer: Int = second(7)\nlet text: Str = second(\"word\")\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let (owner, declaration) = checked.solved.declarations.iter().next().unwrap();
    let aliases: Vec<_> = checked.solved.bindings.values().filter(|binding| binding.scheme == Some(declaration.scheme)).collect();
    assert_eq!(aliases.len(), 2);
    assert!(aliases.iter().all(|binding| binding.ty == declaration.signature));
    assert_eq!(checked.solved.calls.len(), 2);
    assert!(checked.solved.calls.values().all(|call| call.declaration == Some(*owner) && call.substitutions.len() == 1));
    checked.solved.validate().unwrap();
}

#[test]
fn recursive_component_generalizes_after_all_definition_constraints() {
    let first = "pure first(value, count: Int) { if count == 0 { value } else { second(value, count - 1) } }\n";
    let second = "pure second(value, count: Int) { first(value, count) }\n";
    for declarations in [format!("{first}{second}"), format!("{second}{first}")] {
        let source = format!("{declarations}let integer: Int = first(7, 2)\nlet text: Str = second(\"word\", 2)\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.declarations.len(), 2);
        for declaration in checked.solved.declarations.values() {
            let scheme = checked.solved.graph.scheme(declaration.scheme).unwrap();
            assert_eq!(scheme.quantifiers.len(), 1);
            let TypeNode::Arrow(arrow) = checked.solved.graph.node(scheme.body).unwrap() else { panic!("recursive callable retains its complete arrow") };
            assert_eq!(arrow.params[0].ty, arrow.result);
        }
        assert_eq!(checked.solved.calls.len(), 4);
        checked.solved.validate().unwrap();
    }
}

#[test]
fn default_only_headers_publish_definition_owned_schemes() {
    let source = "pure explicit(value: Int = 4) -> Int { value }\npure omitted(value = 4) -> Int { value }\nlet first: Int = explicit()\nlet second: Int = omitted(7)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.solved.declarations.len(), 2);
    assert_eq!(checked.solved.calls.len(), 2);
    for declaration in checked.solved.declarations.values() {
        let TypeNode::Arrow(arrow) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("defaulted callable retains its complete arrow") };
        assert_eq!(checked.solved.graph.export_type(arrow.params[0].ty).unwrap(), super::Type::Int);
        assert!(arrow.params[0].defaulted);
    }
    checked.solved.validate().unwrap();
}

#[test]
fn local_empty_collection_constraints_publish_one_declaration_graph() {
    let source = "pure gather(value: Int) { var entries = []; let before = entries; entries += [value]; before }\nlet result: List[Int] = gather(7)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.solved.declarations.len(), 1);
    let declaration = checked.solved.declarations.values().next().unwrap();
    let TypeNode::Arrow(arrow) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("collection body retains its solved arrow") };
    assert_eq!(checked.solved.graph.export_type(arrow.result).unwrap(), super::Type::List(Box::new(super::Type::Int)));
    let early_read = (0..parsed.arena.arena.expr_tags.len()).map(crate::syntax::arena::ExprId::from_index)
        .find(|&id| parsed.arena.arena.expr(id).span.start() == source.find("= entries;").unwrap() + 2).unwrap();
    let identity = super::ExpressionIdentity { source: SourceId::new(0), namespace: None, expression: early_read };
    let ty = checked.solved.expressions[&identity];
    assert_eq!(checked.solved.graph.export_type(ty).unwrap(), super::Type::List(Box::new(super::Type::Int)));
    checked.solved.validate().unwrap();
}

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
fn empty_map_record_fields_share_registry_and_declaration_constraints() {
    let source = "type Stats = {blobs: Map[Any]}\npure with_blobs(stats: Stats, blobs: Map[Any]) -> Stats { {blobs} }\npure count() -> Stats { let stats = {blobs: map.empty()}; let blobs: Map[Any] = map.empty(); with_blobs(stats, blobs) }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.solved.declarations.len(), 2);
    assert_eq!(checked.solved.operations.len(), 2);
    let mut concrete = 0;
    let mut generalized = 0;
    for (identity, operation) in &checked.solved.operations {
        if let Some(evidence) = checked.solved.graph.candidate_evidence(operation.requirement).unwrap() {
            concrete += 1;
            assert_eq!(checked.solved.graph.export_type(evidence.result).unwrap(), super::Type::Map(Box::new(super::Type::Str), Box::new(super::Type::Any)));
        } else {
            generalized += 1;
            let scope = checked.solved.expression_value_scopes[identity];
            assert!(!checked.solved.graph.scheme(scope).unwrap().quantifiers.is_empty());
            assert!(!checked.solved.expression_schemes.contains_key(identity), "a descendant instruction has an initializer owner without becoming an independently principal value");
        }
    }
    assert_eq!((concrete, generalized), (1, 1));
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
    assert_eq!(checked.solved.calls.values().filter(|call| call.caller.is_none()).count(), 2);
    assert_eq!(checked.solved.operations.len(), 2);
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

#[test]
fn computed_callable_values_preserve_the_checked_parameter_contract() {
    let prefix = "pure first(value: Int) -> Int { value }\npure second(value: Int) -> Int { value }\n";
    for (argument, accepted) in [("true", false), ("7", true)] {
        let source = format!("{prefix}pure inspect(select: Bool) -> Int {{ let chosen = if select {{ (first) }} else {{ (second) }}; let _ = chosen.call({argument}); 1 }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        if accepted {
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let (identity, invocation) = checked.solved.invocations.iter().find(|(identity, _)| source[parsed.arena.arena.expr(identity.expression).span.range()].starts_with("chosen.call(")).expect("the computed call retains its canonical invocation");
            let evidence = checked.solved.graph.invocation_evidence(invocation.requirement).unwrap().unwrap();
            let crate::sema::inference::InvocationPlan::All { branches } = &evidence.plan else { panic!("the conditional value retains both original user contracts") };
            assert_eq!(branches.len(), 2);
            for branch in branches {
                let TypeNode::Arrow(arrow) = checked.solved.graph.node(checked.solved.graph.resolved(branch.signature).unwrap()).unwrap() else { panic!() };
                assert_eq!(checked.solved.graph.export_type(arrow.params[0].ty).unwrap(), super::Type::Int);
                assert_eq!(branch.binding.supplied_slots, vec![0]);
            }
            assert!(!checked.solved.calls.contains_key(identity), "an ALL invocation has no invented shared static call");
        } else {
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "wrong computed argument must reject against the retained parameter: {:?}", checked.diagnostics);
        }
    }
}

#[test]
fn callable_storage_preserves_monomorphic_slots_and_returned_signatures() {
    for source in [
        "pure first(value: Int) -> Int { value }\npure second(value: Str) -> Str { value }\npure inspect() -> Int { var current = first; current = second; 1 }\n",
        "pure identity(value) { value }\npure inspect() -> Int { var current = identity; let alias = current; let _ = alias.call(7); let _ = alias.call(\"word\"); 1 }\n",
        "pure first(value: Int) -> Int { value }\npure factory() { (first) }\npure inspect() -> Int { let callback = factory(); let _ = callback.call(true); 1 }\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{source}: {:?}", checked.diagnostics);
    }
    let source = "pure first(value: Int) -> Int { value }\npure second(value: Str) -> Str { value }\npure inspect() -> Int { var current: Pure = first; current = second; let _ = current.call(true); 1 }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "authored erasure remains an explicit dynamic boundary: {:?}", checked.diagnostics);
}

#[test]
fn computed_callable_signatures_keep_finite_effect_unions() {
    let prefix = "proc clock(value: Int)[time] -> Int { value }\nproc setting(value: Int)[env] -> Int { value }\npure pick(select: Bool) { if select { (clock) } else { (setting) } }\n";
    for (bound, accepted) in [("time, env", true), ("time", false)] {
        let source = format!("{prefix}proc inspect(select: Bool)[{bound}] -> Int {{ let callback = pick(select); let _ = callback.call(3); 1 }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        if accepted { assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics); }
        else { assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics); }
    }
}

#[test]
fn computed_callable_values_keep_definition_inferred_effect_unions() {
    let prefix = "proc clock(value: Int) -> Int { let _ = time.now(); value }\nproc setting(value: Int) -> Int { let _ = env.get(\"KEY\"); value }\npure pick(select: Bool) { if select { (clock) } else { (setting) } }\n";
    for (bound, accepted) in [("time, env", true), ("time", false)] {
        let source = format!("{prefix}proc inspect(select: Bool)[{bound}] -> Int {{ let callback = pick(select); let _ = callback.call(3); 1 }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        if accepted { assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics); }
        else { assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics); }
    }
}

#[test]
fn symbolic_callable_join_generalizes_at_its_initializer_level() {
    use crate::sema::inference::{Arrow, CallableKind, EffectSummary, Generalization};
    let mut checker = Checker::new(Default::default());
    checker.local_initializer_level = Some(2);
    let (left, right) = {
        let mut state = checker.generic.borrow_mut();
        let graph = &mut state.facts.graph;
        let result = graph.atom(Atom::Int).unwrap();
        let left_effect = graph.fresh_effect_at(2, None).unwrap();
        let right_effect = graph.fresh_effect_at(2, None).unwrap();
        let left = graph.arrow(Arrow { kind: CallableKind::Proc, params: Vec::new(), result, effects: EffectSummary::Variable(left_effect) }).unwrap();
        let right = graph.arrow(Arrow { kind: CallableKind::Proc, params: Vec::new(), result, effects: EffectSummary::Variable(right_effect) }).unwrap();
        (super::Type::Graph(left), super::Type::Graph(right))
    };
    let joined = checker.join_graph_callable_values(&left, &right, crate::source::Span::at(SourceId::new(0), 0)).unwrap();
    assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
    let super::Type::Graph(joined) = joined else { panic!("joined callable retains its graph identity") };
    let mut state = checker.generic.borrow_mut();
    let scheme = state.facts.graph.generalize(joined, 1, Generalization::Allowed, &[]).expect("the local union keeps latent inputs in its owning scheme instead of a captured global budget");
    let scheme = state.facts.graph.scheme(scheme).unwrap();
    assert_eq!(scheme.effect_quantifiers.len(), 3);
    assert_eq!(scheme.effect_quantifiers.iter().filter(|quantifier| quantifier.derived).count(), 1);
    assert_eq!(scheme.effect_inclusions.len(), 2);
}

#[test]
fn recursive_callable_selection_keeps_symbolic_effect_dependencies() {
    let prefix = "pure pick(select: Bool) { if select { (clock) } else { (setting) } }\nproc clock(value: Int) -> Int { let _ = pick(false); let _ = time.now(); value }\nproc setting(value: Int) -> Int { let _ = pick(true); let _ = env.get(\"KEY\"); value }\n";
    for (bound, accepted) in [("time, env", true), ("time", false)] {
        let source = format!("{prefix}proc inspect(select: Bool)[{bound}] -> Int {{ let callback = pick(select); callback.call(3) }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        if accepted {
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            checked.solved.validate().unwrap();
        } else {
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics);
        }
    }
}

#[test]
fn immutable_empty_binding_schemes_instantiate_independently_of_alias_use_order() {
    let prefix = "pure paths(values: List[Path]) -> Int { values.len() }\npure integers(values: List[Int]) -> Int { values.len() }\n";
    for calls in ["paths(entries) + integers(alias)", "integers(alias) + paths(entries)"] {
        let source = format!("{prefix}pure inspect() -> Int {{ let entries = []; let alias = entries; {calls} }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let generalized = checked.solved.bindings.values().filter(|binding| binding.scheme.is_some()).collect::<Vec<_>>();
        assert_eq!(generalized.len(), 2, "safe immutable allocations and aliases retain explicit principal value schemes");
        assert!(generalized.iter().all(|binding| !binding.mutable));
        checked.solved.validate().unwrap();
    }
}

#[test]
fn immutable_callable_aggregates_retain_principal_value_schemes() {
    for (allocation, accessor) in [("{callback: identity}", "boxed.callback"), ("[identity]", "boxed[0]")] {
        for calls in [
            format!("let integer: Int = {accessor}(7)\nlet text: Str = {accessor}(\"word\")"),
            format!("let text: Str = {accessor}(\"word\")\nlet integer: Int = {accessor}(7)"),
        ] {
            let source = format!("pure identity(value) {{ value }}\nlet boxed = {allocation}\n{calls}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            assert!(checked.solved.bindings.values().any(|binding| binding.scheme.is_some_and(|scheme| !checked.solved.graph.scheme(scheme).unwrap().quantifiers.is_empty())), "the allocation has a source-owned principal value scheme");
            checked.solved.validate().unwrap();
        }
    }
}

#[test]
fn callable_value_generation_uses_the_declaring_scope_before_source_order() {
    let source = "let retained: Int = 7\npure inspect() -> Int { let retained: Str = \"wrong\"; let callback = target; callback(false) }\npure target(unused) { retained }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "a caller shadow must not change the declaration-owned captured result: {:?}", checked.diagnostics);
    let (_, target) = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "target").unwrap();
    let TypeNode::Arrow(arrow) = checked.solved.graph.node(target.signature).unwrap() else { panic!("complete target signature") };
    assert_eq!(checked.solved.graph.export_type(arrow.result).unwrap(), super::Type::Int);
}

#[test]
fn omitted_unit_completions_publish_statement_consumption_after_solving() {
    let source = "proc inferred() [error] { print message }\nproc authored() [error] -> Unit { print message }\nproc boolean() [] { false }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    for identity in checked.solved.declarations.keys() {
        let def = parsed.arena.arena.function_def(identity.declaration);
        let tail = parsed.arena.arena.stmt_ids(parsed.arena.arena.block(def.body).statements).last().unwrap();
        let key = super::StatementIdentity { source: identity.source, namespace: identity.namespace, statement: tail };
        let expected = if def.name == "boolean" { super::StatementPosition::Value } else { super::StatementPosition::Statement };
        assert_eq!(checked.solved.statements[&key], expected, "{} keeps its solved completion role", def.name);
        assert_eq!(checked.statement_positions[&parsed.arena.arena.stmt(tail).span], expected);
    }
}

#[test]
fn registry_result_context_cannot_validate_dynamic_json_data() {
    let source = "type Row = {name: Str}\nproc load(input: Path) -> Result[Row] { json.read(input)? }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.dynamic-boundary")), "dynamic data keeps an explicit validation boundary: {:?}", checked.diagnostics);
}

#[test]
fn graph_callable_arguments_keep_dynamic_data_validation_boundaries() {
    let source = "proc needs_name(value: Str) -> Result[Unit] { Ok() }\nlet raw = json.decode(\"\\\"demo\\\"\")?\nneeds_name(raw)?\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.dynamic-boundary")), "named call validates actual dynamic payload: {:?}", checked.diagnostics);
}

#[test]
fn polymorphic_identity_preserves_an_explicit_dynamic_argument_without_validation() {
    let source = "pure identity(value) { value }\nlet raw: Any = 7\nlet retained: Any = identity(raw)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let call = checked.solved.calls.values().next().unwrap();
    let TypeNode::Arrow(arrow) = checked.solved.graph.node(call.signature).unwrap() else { panic!("identity owns a complete instantiated signature") };
    assert_eq!(checked.solved.graph.export_type(arrow.params[0].ty).unwrap(), super::Type::Any);
    assert_eq!(checked.solved.graph.export_type(arrow.result).unwrap(), super::Type::Any);
    checked.solved.validate().unwrap();
}

#[test]
fn explicit_registry_schema_validation_retains_its_original_dynamic_input() {
    let source = "pure identity(value) { value }\nlet raw: Any = {name: \"SETTING\", value: \"present\"}\nlet validated: Result[EnvEntry] = identity(raw).require(EnvEntry)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let (identity, boundary) = checked.solved.registry_boundaries.iter().next().expect("explicit schema validation owns a source boundary");
    assert_eq!(checked.solved.graph.export_type(boundary.input).unwrap(), super::Type::Any);
    assert_eq!(checked.solved.expressions.get(identity).copied(), Some(boundary.result));
    checked.solved.validate().unwrap();
}

#[test]
fn retained_source_facts_count_declaration_requirement_capacity() {
    let source = "pure add(left, right) { left + right }\nlet value: Int = add(1, 2)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let mut solved = std::sync::Arc::try_unwrap(checked.solved).unwrap();
    let before = solved.retained_source_bytes();
    let declaration = solved.declarations.values_mut().next().unwrap();
    let old_capacity = declaration.source_requirements.capacity();
    declaration.source_requirements.reserve_exact(17);
    let added_capacity = declaration.source_requirements.capacity() - old_capacity;
    assert_eq!(solved.retained_source_bytes() - before, added_capacity * std::mem::size_of::<crate::sema::inference::RequirementId>());
}

#[test]
fn producer_flow_generation_retains_distinct_formal_values_after_type_unification() {
    use super::{ProducerFlowKind, ProducerFlowSource};
    let source = "pure identity(value) { value }\npure choose(select: Bool, left, right) { if select { left } else { right } }\nlet number: Int = identity(7)\nlet chosen: Int = choose(false, 1, 2)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    for (identity, declaration) in &checked.solved.declarations {
        let params = parsed.arena.arena.params(parsed.arena.arena.function_def(identity.declaration).params);
        assert_eq!(declaration.parameter_producer_flows.len(), params.len());
        for (index, &flow) in declaration.parameter_producer_flows.iter().enumerate() {
            let node = checked.solved.producer_flows.node(flow).unwrap();
            assert_eq!(node.source, ProducerFlowSource::Parameter { declaration: *identity, index: index as u32 });
            assert!(matches!(node.kind, ProducerFlowKind::Parameter { declaration, index: found } if declaration == *identity && found as usize == index));
        }
        let output = declaration.return_producer_flow.expect("ordinary value completion retains a declaration-owned producer-flow template");
        assert_eq!(checked.solved.producer_flows.node(output).unwrap().source, ProducerFlowSource::DeclarationResult(*identity));
        if parsed.arena.arena.function_def(identity.declaration).name == "choose" {
            assert_ne!(declaration.parameter_producer_flows[1], declaration.parameter_producer_flows[2], "equal types do not equate producer handles");
        }
    }
    assert!(checked.solved.calls.values().all(|call| call.result_producer_flow.is_some()));
    checked.solved.validate().unwrap();
}

#[test]
fn source_producer_creation_and_latent_permissions_publish_separate_roots() {
    use crate::sema::inference::{EffectSet, EffectSummary};
    let source = "stream delayed() [time, env] -> Stream[Int] {\n defer { let _ = env.get(\"UNREAD_SETTING\") }\n let _ = time.now()\n yield 1\n}\nproc create() [] -> Unit { let _ = delayed() }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "creation does not execute the retained producer: {:?}", checked.diagnostics);
    let (_, declaration) = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "delayed").unwrap();
    let TypeNode::Arrow(arrow) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("producer owns an arrow") };
    assert_eq!(arrow.effects, EffectSummary::Closed(EffectSet::EMPTY));
    let effects = declaration.return_producers.get(&super::ProducerPath::default()).expect("latent permissions belong to the returned handle");
    assert_eq!(effects.pull, EffectSummary::Closed(EffectSet::TIME));
    assert_eq!(effects.close, EffectSummary::Closed(EffectSet::ENV));
    let created = checked.solved.calls.values().find(|call| call.declaration.is_some_and(|identity| parsed.arena.arena.function_def(identity.declaration).name == "delayed")).unwrap();
    assert_eq!(created.result_producers[&super::ProducerPath::default()], *effects);
    checked.solved.validate().unwrap();
}

#[test]
fn source_mutable_producer_versions_preserve_immutable_alias_snapshots() {
    use super::{ProducerFlowKind, ProducerFlowSource};
    let source = "stream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 1 }\nstream configured() [env] -> Stream[Int] { let _ = env.get(\"SETTING\"); yield 2 }\nvar current = clocked()\nlet snapshot = current\ncurrent = configured()\nlet latest = current\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let identity = *checked.solved.bindings.keys().find(|identity| matches!(parsed.arena.arena.binding_target(identity.target).kind, crate::syntax::arena::ArenaBindingTargetKind::Name(name) if name == "current")).unwrap();
    let original = checked.solved.binding_producer_flows[&(identity, 0)];
    let replacement = *checked.solved.binding_producer_flows.get(&(identity, 1)).expect("a checked assignment retains its own binding version");
    assert_ne!(original, replacement);
    let reads = checked.solved.producer_flows.nodes().filter_map(|node| match (&node.source, &node.kind) {
        (ProducerFlowSource::Expression(_), ProducerFlowKind::CapturedBinding { identity: found, version, input }) if *found == identity => Some((*version, *input)),
        _ => None,
    }).collect::<Vec<_>>();
    assert_eq!(reads, vec![(0, original), (1, replacement)], "aliases retain the handle read at their own source point");
    checked.solved.validate().unwrap();
}

#[test]
fn source_call_producer_arguments_follow_expanded_named_binding() {
    use super::{ProducerFlowKind, ProducerPath, ProducerPathComponent};
    let source = "pure choose(first: Int, second = 2) -> Int { first }\nlet options = {first: 7}\nlet value: Int = choose(...options)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let _symbols = parsed.arena.symbol_owner().enter();
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let call = checked.solved.calls.values().next().unwrap();
    assert_eq!(call.binding.supplied_slots, vec![0]);
    assert_eq!(call.binding.default_slots, vec![1]);
    let flow = call.result_producer_flow.unwrap();
    let ProducerFlowKind::Apply { arguments, .. } = &checked.solved.producer_flows.node(flow).unwrap().kind else { panic!("source call retains its actual binding") };
    assert_eq!(arguments.len(), 1);
    assert!(matches!(&checked.solved.producer_flows.node(arguments[0]).unwrap().kind,
        ProducerFlowKind::Project { path, .. } if *path == ProducerPath(vec![ProducerPathComponent::RecordField(crate::symbol::Name::intern("first"))])));
    checked.solved.validate().unwrap();
}

#[test]
fn source_call_producer_rest_splice_retains_the_original_list_before_packing() {
    use super::ProducerFlowKind;
    let source = "pure pack(...items: List[Int]) -> List[Int] { items }\nlet values: List[Int] = pack(@[1, 2])\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let call = checked.solved.calls.values().next().unwrap();
    assert_eq!(call.binding.supplied_slots, vec![0]);
    assert_eq!(call.binding.rest_slot, Some(0));
    assert_eq!(checked.solved.graph.export_type(call.actual_arguments[0]).unwrap(), super::Type::List(Box::new(super::Type::Int)));
    let flow = call.result_producer_flow.unwrap();
    let ProducerFlowKind::Apply { arguments, .. } = &checked.solved.producer_flows.node(flow).unwrap().kind else { panic!("source rest call retains its actual argument") };
    assert_eq!(arguments.len(), 1);
    let source_argument = checked.solved.expression_producer_flows.iter().find_map(|(identity, &flow)| matches!(parsed.arena.arena.expr(identity.expression).kind, crate::syntax::arena::ArenaExprKind::List(_)).then_some(flow)).unwrap();
    assert_eq!(arguments[0], source_argument);
    let invocation = checked.solved.invocations.values().next().unwrap();
    let crate::sema::inference::RequirementTemplate::CallableInvocation { call } = checked.solved.graph.requirement_template(invocation.requirement).unwrap() else { panic!() };
    assert_eq!(checked.solved.graph.invocation_call(call).unwrap().arguments[0].kind, crate::sema::inference::InvocationArgumentKind::PositionalSplice);
    checked.solved.validate().unwrap();
}

#[test]
fn inert_error_constructor_preserves_the_err_success_value_scheme() {
    let source = "error Outer = Failed(message: Str)\nerror Inner = Failed(message: Str)\nlet value = Err(cause: Inner.Failed(message: \"inner\"), Outer.Failed(message: \"outer\"))\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let binding = checked.solved.bindings.values().next().unwrap();
    let scheme = binding.scheme.expect("an inert error value keeps its absent success type principal");
    assert_eq!(checked.solved.graph.scheme(scheme).unwrap().quantifiers.len(), 1);
    let TypeNode::Result(_, error) = checked.solved.graph.node(checked.solved.graph.resolved(binding.ty).unwrap()).unwrap() else { panic!("constructor result keeps its nominal outer error") };
    assert!(matches!(checked.solved.graph.node(*error).unwrap(), TypeNode::Atom(Atom::ErrorVariant { .. })));
    checked.solved.validate().unwrap();
}

#[test]
fn omitted_callback_invocation_keeps_a_definition_owned_residual_contract() {
    use crate::sema::inference::RequirementTemplate;
    let source = "pure apply(callback, value) { callback.call(value) }\npure identity(item) { item }\nlet number: Int = apply(identity, 7)\nlet text: Str = apply(identity, \"word\")\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declaration = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "apply").unwrap().1;
    assert!(checked.solved.graph.scheme(declaration.scheme).unwrap().requirements.iter().any(|requirement| matches!(requirement, RequirementTemplate::CallableInvocation { .. })));
    assert_eq!(checked.solved.calls.values().filter(|call| call.declaration.is_some_and(|identity| parsed.arena.arena.function_def(identity.declaration).name == "apply")).count(), 2);
    let mut actual_payloads = Vec::new();
    for (identity, callable) in &checked.solved.expression_callables {
        if !matches!(parsed.arena.arena.expr(identity.expression).kind, crate::syntax::arena::ArenaExprKind::Ident(name) if name == "identity") { continue; }
        let actual = checked.solved.expressions[identity];
        assert_ne!(checked.solved.graph.resolved(actual).unwrap(), checked.solved.graph.resolved(callable.signature).unwrap());
        assert_eq!(checked.solved.graph.scheme(callable.scheme.unwrap()).unwrap().quantifiers.len(), 1);
        let TypeNode::Arrow(arrow) = checked.solved.graph.node(checked.solved.graph.resolved(actual).unwrap()).unwrap() else { panic!("the value use owns its instantiated callable type") };
        actual_payloads.push(checked.solved.graph.export_type(arrow.result).unwrap());
    }
    assert_eq!(actual_payloads, vec![super::Type::Int, super::Type::Str]);
    checked.solved.validate().unwrap();
}

#[test]
fn omitted_callback_invocation_preserves_each_callers_effect_instance() {
    use crate::sema::inference::{EffectSet, EffectSummary};
    let source = "proc apply(callback, value) { callback.call(value) }\nproc clock(value: Int) [time] -> Int { value }\nproc setting(value: Int) [env] -> Int { value }\nproc first() [time] -> Int { apply(clock, 7) }\nproc second() [env] -> Int { apply(setting, 8) }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let (identity, declaration) = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "apply").unwrap();
    let TypeNode::Arrow(arrow) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("forwarder keeps its shared signature") };
    assert!(matches!(checked.solved.graph.resolved_effect_summary(arrow.effects).unwrap(), EffectSummary::Rigid { .. }), "the definition keeps its callback effect relationship");
    let effects: Vec<_> = checked.solved.calls.values().filter(|call| call.declaration == Some(*identity)).map(|call| {
        let TypeNode::Arrow(arrow) = checked.solved.graph.node(call.signature).unwrap() else { panic!("call retains its instantiated arrow") };
        checked.solved.graph.closed_effect_summary(arrow.effects).unwrap()
    }).collect();
    assert_eq!(effects, vec![EffectSummary::Closed(EffectSet::TIME), EffectSummary::Closed(EffectSet::ENV)]);
    checked.solved.validate().unwrap();
}

#[test]
fn omitted_callback_scheme_is_unchanged_by_caller_order_and_domains() {
    let declarations = "proc apply(callback, value) { callback.call(value) }\nproc clock(value: Int) [time] -> Int { value }\nproc setting(value: Str) [env] -> Str { value }\n";
    let callers = ["proc first() [time] -> Int { apply(clock, 7) }\n", "proc second() [env] -> Str { apply(setting, \"word\") }\n"];
    let mut principal = None;
    for suffix in [String::new(), format!("{}{}", callers[0], callers[1]), format!("{}{}", callers[1], callers[0])] {
        let source = format!("{declarations}{suffix}");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let identity = *checked.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == "apply").unwrap();
        let normalized = crate::frontend::query::SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner()).declaration(identity).unwrap().scheme;
        if let Some(principal) = &principal { assert_eq!(&normalized, principal, "callers never train the definition's type or latent effect binders"); }
        else { principal = Some(normalized); }
        checked.solved.validate().unwrap();
    }
}

#[test]
fn omitted_callback_invocation_rejects_payload_kind_and_effect_mismatches() {
    for (source, code) in [
        ("pure apply(callback, value) { callback.call(value) }\npure number(item: Int) -> Int { item }\nlet _ = apply(number, true)\n", "check.type-mismatch"),
        ("pure invoke(callback) { callback.call() }\nproc tick() [time] -> Int { 1 }\nlet _ = invoke(tick)\n", "check.pure-effect"),
        ("proc invoke(callback) { callback.call() }\nproc tick() [time] -> Int { 1 }\nproc restricted() [] -> Int { invoke(tick) }\n", "check.effect-violation"),
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some(code)), "{code}: {:?}", checked.diagnostics);
        assert!(!checked.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code.as_deref(), Some("check.call-target" | "check.unresolved-call"))));
    }
}

#[test]
fn source_producer_identity_preserves_nested_profiles_through_a_projected_alias() {
    use super::{ProducerPath, ProducerEffects};
    use crate::sema::inference::{EffectSet, EffectSummary};
    let source = "pure identity(value) { value }\nstream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 1 }\nlet original = {left: clocked()}\nlet forwarded = identity(original)\nlet selected = forwarded.left\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let selected = *checked.solved.bindings.keys().find(|identity| matches!(parsed.arena.arena.binding_target(identity.target).kind, crate::syntax::arena::ArenaBindingTargetKind::Name(name) if name == "selected")).unwrap();
    assert_eq!(checked.solved.binding_producers.get(&selected).and_then(|profile| profile.get(&ProducerPath::default())), Some(&ProducerEffects { pull: EffectSummary::Closed(EffectSet::TIME), close: EffectSummary::Closed(EffectSet::EMPTY) }));
    checked.solved.validate().unwrap();
}

#[test]
fn retry_capture_retains_a_masked_source_effect_relationship() {
    use crate::sema::inference::{RequirementTemplate, EffectSet};
    let source = "proc clock() [time, error] -> Result[Int] { Ok(1) }\nproc captured() [time] -> Result[Int] { retry [] { clock()? } }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declaration = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "captured").unwrap().1;
    assert!(declaration.source_requirements.iter().any(|&requirement| matches!(checked.solved.graph.requirement_template(requirement), Ok(RequirementTemplate::EffectInclusion { excluded: EffectSet::ERROR, .. }))), "capturing errors retains the other permissions in an authoritative graph relationship");
    checked.solved.validate().unwrap();
}

#[test]
fn captured_producer_iteration_requires_both_pull_and_cleanup_permissions() {
    let source = "stream delayed() [time, env] -> Stream[Int] {\n defer { let _ = env.get(\"SETTING\") }\n let _ = time.now()\n yield 1\n}\nlet rows = delayed()\nproc denied() [time] -> Unit { for value in rows { let _ = value } }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "cleanup remains part of consuming the captured handle: {:?}", checked.diagnostics);
}

#[test]
fn formal_producer_consumption_retains_fresh_per_call_permission_ports() {
    use crate::sema::inference::{EffectSummary, EffectSet};
    let source = "stream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 1 }\nstream configured() [env] -> Stream[Int] { let _ = env.get(\"SETTING\"); yield 2 }\nproc drain(rows: Stream[Int]) { for value in rows { let _ = value } }\nproc timed() [time] -> Unit { let _ = drain(clocked()) }\nproc setup() [env] -> Unit { let _ = drain(configured()) }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let (identity, declaration) = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "drain").unwrap();
    let profile = &declaration.parameter_producers[0];
    assert!(matches!(profile[&super::ProducerPath::default()].pull, EffectSummary::Rigid { .. }));
    let effects: Vec<_> = checked.solved.calls.values().filter(|call| call.declaration == Some(*identity)).map(|call| {
        let TypeNode::Arrow(arrow) = checked.solved.graph.node(call.signature).unwrap() else { panic!("the call retains its arrow") };
        checked.solved.graph.closed_effect_summary(arrow.effects).unwrap()
    }).collect();
    assert_eq!(effects, vec![EffectSummary::Closed(EffectSet::TIME), EffectSummary::Closed(EffectSet::ENV)]);
    checked.solved.validate().unwrap();
}

#[test]
fn registry_collection_transfers_preserve_nested_producer_permissions() {
    use super::{ProducerPath, ProducerPathComponent, ProducerEffects};
    use crate::sema::inference::{EffectSet, EffectSummary};
    let source = "stream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 1 }\nlet original = [clocked()]\nlet grown = original.push(clocked())\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let grown = *checked.solved.bindings.keys().find(|identity| matches!(parsed.arena.arena.binding_target(identity.target).kind, crate::syntax::arena::ArenaBindingTargetKind::Name(name) if name == "grown")).unwrap();
    let item = ProducerPath(vec![ProducerPathComponent::ListItem]);
    assert_eq!(checked.solved.binding_producers.get(&grown).and_then(|profile| profile.get(&item)), Some(&ProducerEffects { pull: EffectSummary::Closed(EffectSet::TIME), close: EffectSummary::Closed(EffectSet::EMPTY) }));
    checked.solved.validate().unwrap();
}

#[test]
fn cli_entry_declaration_retains_the_same_solved_header_and_body_owner() {
    let source = "cli main(verbose = false, ...arguments: List[Str]) [error] { let _ = verbose; let _ = arguments }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declaration = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "main").expect("the entry callable owns its actual graph declaration");
    let TypeNode::Arrow(arrow) = checked.solved.graph.node(declaration.1.signature).unwrap() else { panic!("entry owns an arrow") };
    assert_eq!(checked.solved.graph.export_type(arrow.params[0].ty).unwrap(), super::Type::Bool);
    assert_eq!(checked.solved.graph.export_type(arrow.params[1].ty).unwrap(), super::Type::List(Box::new(super::Type::Str)));
    assert_eq!(declaration.1.return_elaboration, super::ReturnElaboration::UnitConsuming);
    assert_eq!(checked.solved.graph.scheme(declaration.1.scheme).unwrap().quantifiers.len(), 0);
    checked.solved.validate().unwrap();
}

#[test]
fn indexed_producer_value_retains_the_selected_collection_payload_profile() {
    use super::{ProducerEffects, ProducerPath};
    use crate::sema::inference::{EffectSet, EffectSummary};
    let source = "stream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 1 }\nlet rows = [clocked()]\nlet selected = rows[0]\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let selected = *checked.solved.bindings.keys().find(|identity| matches!(parsed.arena.arena.binding_target(identity.target).kind, crate::syntax::arena::ArenaBindingTargetKind::Name(name) if name == "selected")).unwrap();
    assert_eq!(checked.solved.binding_producers.get(&selected).and_then(|profile| profile.get(&ProducerPath::default())), Some(&ProducerEffects { pull: EffectSummary::Closed(EffectSet::TIME), close: EffectSummary::Closed(EffectSet::EMPTY) }));
    checked.solved.validate().unwrap();
}

#[test]
fn propagated_result_success_retains_its_producer_profile() {
    use super::{ProducerEffects, ProducerPath};
    use crate::sema::inference::{EffectSet, EffectSummary};
    let source = "stream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 1 }\nproc selected() [error] -> Result[Stream[Int]] { let retained = Ok(clocked())?; retained }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let retained = *checked.solved.bindings.keys().find(|identity| matches!(parsed.arena.arena.binding_target(identity.target).kind, crate::syntax::arena::ArenaBindingTargetKind::Name(name) if name == "retained")).unwrap();
    assert_eq!(checked.solved.binding_producers.get(&retained).and_then(|profile| profile.get(&ProducerPath::default())), Some(&ProducerEffects { pull: EffectSummary::Closed(EffectSet::TIME), close: EffectSummary::Closed(EffectSet::EMPTY) }));
    checked.solved.validate().unwrap();
}

#[test]
fn yielded_records_retain_nested_producer_permissions_as_item_data() {
    use super::{ProducerEffects, ProducerPath, ProducerPathComponent};
    use crate::sema::inference::{EffectSet, EffectSummary};
    let source = "type Box = {rows: Stream[Int]}\nstream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 1 }\nstream boxed() [] -> Stream[Box] { yield {rows: clocked()} }\nlet boxes = boxed()\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let _symbols = checked.solved.symbol_owner().enter();
    let boxes = *checked.solved.bindings.keys().find(|identity| matches!(parsed.arena.arena.binding_target(identity.target).kind, crate::syntax::arena::ArenaBindingTargetKind::Name(name) if name == "boxes")).unwrap();
    assert_eq!(checked.solved.binding_producers.get(&boxes).and_then(|profile| profile.get(&ProducerPath(vec![ProducerPathComponent::ListItem, ProducerPathComponent::RecordField(crate::symbol::Name::intern("rows"))]))), Some(&ProducerEffects { pull: EffectSummary::Closed(EffectSet::TIME), close: EffectSummary::Closed(EffectSet::EMPTY) }));
    assert_eq!(checked.solved.binding_producers[&boxes][&ProducerPath::default()], ProducerEffects { pull: EffectSummary::Closed(EffectSet::EMPTY), close: EffectSummary::Closed(EffectSet::EMPTY) });
    checked.solved.validate().unwrap();
}

#[test]
fn explicit_producer_validation_preserves_known_source_handle_permissions() {
    let declarations = "stream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 1 }\n";
    let source = format!("{declarations}proc allowed() [time, error] -> Result[List[Int]] {{ let raw: Any = clocked(); let rows = raw.require(Stream[Int])?; rows.collect() }}\n");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, &source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    checked.solved.validate().unwrap();
    let source = format!("{declarations}proc denied() [error] -> Result[List[Int]] {{ let raw: Any = clocked(); let rows = raw.require(Stream[Int])?; rows.collect() }}\n");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
    let checked = Checker::check_arena(&parsed.arena, &source);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics);
}

#[test]
fn inert_error_propagation_does_not_invent_a_success_value() {
    for source in [
        "error LocalError = Failed(message: Str)\nlet result = retry [] on (LocalError) { Err(LocalError.Failed(message: \"local\"))? }\n",
        "error LocalError = First(message: Str) | Second(message: Str)\nlet value = try { if true { let _ = Err(LocalError.First(message: \"first\"))? } else { let _ = Err(LocalError.Second(message: \"second\"))? }; 7 }\nlet narrow: Result[Int, LocalError] = value\n",
        "error LocalError = Failed(message: Str)\nproc choose(flag: Bool) [error] { if flag { Err(LocalError.Failed(message: \"local\"))? } else { 7 } }\nlet value: Result[Int, LocalError] = choose(false)\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        checked.solved.validate().unwrap();
    }
    let source = "error LocalError = Failed(message: Str)\nlet result = try { Err(LocalError.Failed(message: \"local\"))? }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.try-success-type")), "{:?}", checked.diagnostics);
}

#[test]
fn source_splice_bindings_retain_original_list_and_runtime_guards() {
    use crate::sema::inference::InvocationArgumentSegment;
    let source = "pure pair(first: Int, second: Int = 2) -> Int { first + second }\nlet parts = [7]\nlet answer: Int = pair(@parts)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let (identity, call) = checked.solved.calls.iter().find(|(_, call)| call.binding.dynamic.is_some()).unwrap();
    let dynamic = call.binding.dynamic.as_ref().unwrap();
    assert!(call.binding.supplied_slots.is_empty());
    assert_eq!(call.actual_arguments.len(), 1);
    assert!(matches!(checked.solved.graph.node(checked.solved.graph.resolved(call.actual_arguments[0]).unwrap()).unwrap(), TypeNode::List(_)));
    assert_eq!(dynamic.segments, vec![InvocationArgumentSegment::DynamicRange { argument: 0, fixed_slots: vec![0, 1], rest_slot: None }]);
    assert_eq!(dynamic.required_slots, vec![0]);
    assert_eq!(dynamic.conditional_default_slots, vec![1]);
    assert!(dynamic.runtime_arity_guard && dynamic.runtime_duplicate_guard);
    let invocation = &checked.solved.invocations[identity];
    assert_eq!(checked.solved.graph.invocation_evidence(invocation.requirement).unwrap().unwrap().unique_plan().unwrap().1.dynamic.as_ref(), Some(dynamic));
    checked.solved.validate().unwrap();
    let invalid = source.replace("[7]", "[false]");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &invalid);
    let checked = Checker::check_arena(&parsed.arena, &invalid);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{:?}", checked.diagnostics);
}

#[test]
fn captured_producers_retain_permissions_inside_immutable_aggregates() {
    let declarations = "stream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 1 }\n";
    for (effects, accepted) in [("time, error", true), ("error", false)] {
        let source = format!("{declarations}proc consume() [{effects}] -> Result[List[Int]] {{ let captured = try {{ clocked() }}; let holder = {{ rows: captured? }}; holder.rows.collect() }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        if accepted { assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics); checked.solved.validate().unwrap(); }
        else { assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics); }
    }
}

#[test]
fn dynamic_producer_arguments_join_possible_items_and_conditional_defaults() {
    let declarations = "stream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 1 }\nstream configured() [env] -> Stream[Int] { let _ = env.get(\"UNREAD_SETTING\"); yield 2 }\nproc consume(first: Stream[Int], second: Stream[Int] = configured()) { for value in first { let _ = value }; for value in second { let _ = value }; 1 }\n";
    for (effects, accepted) in [("time, env", true), ("time", false)] {
        let source = format!("{declarations}proc caller() [{effects}] -> Int {{ let arguments = [clocked()]; consume(@arguments) }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        if accepted { assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics); checked.solved.validate().unwrap(); }
        else { assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics); }
    }
}

#[test]
fn source_range_constructor_checks_bounds_and_argument_effects() {
    for source in ["let values = range(\"wrong\")\n", "let values = range(missing())\n", "proc denied() [] -> Stream[Int] { range(time.now()) }\n"] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(!checked.diagnostics.is_empty(), "range omitted checking: {source}");
    }
    let source = "proc allowed() [time] -> Stream[Int] { range(time.now()) }\nlet direct = range(1, 3)\nlet destination = Path(\"file\")\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.solved.operations.values().filter(|operation| {
        let crate::sema::inference::RequirementTemplate::Operation { family, .. } = checked.solved.graph.requirement_template(operation.requirement).unwrap() else { return false; };
        checked.solved.graph.family(family).unwrap().iter().all(|candidate| matches!(checked.solved.operation_catalog.candidate(&checked.solved.graph, *candidate).unwrap(), super::SolvedOperationAuthority::Language(metadata) if matches!(metadata.operation, crate::sema::operation_graph::PreparedLanguageOperation::Constructor { .. })))
    }).count(), 3);
    checked.solved.validate().unwrap();
}

#[test]
fn generalized_direct_yield_retains_the_non_stream_item_guard() {
    let declarations = "stream rows() [] -> Stream[Int] { yield 1 }\nstream repeat(item) [] { yield item }\n";
    let invalid = format!("{declarations}let nested = repeat(rows())\n");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &invalid);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, &invalid);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.yield-stream")), "{:?}", checked.diagnostics);
    let valid = format!("{declarations}let integer = repeat(7)\nlet nested_data = repeat({{ rows: rows() }})\nlet dynamic: Any = 7\nlet erased = repeat(dynamic)\n");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &valid);
    let checked = Checker::check_arena(&parsed.arena, &valid);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    checked.solved.validate().unwrap();
}

#[test]
fn computed_map_literals_retain_definition_owned_key_eligibility() {
    let definition = "pure singleton(key, value) { {[key]: value} }\n";
    for calls in ["let integer: Map[Int, Str] = singleton(7, \"word\")\nlet boolean: Map[Bool, Int] = singleton(false, 2)\n", "let boolean: Map[Bool, Int] = singleton(false, 2)\nlet integer: Map[Int, Str] = singleton(7, \"word\")\n"] {
        let source = format!("{definition}{calls}");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert!(checked.solved.declarations.values().next().unwrap().source_requirements.iter().any(|requirement| matches!(checked.solved.graph.requirement_template(*requirement).unwrap(), crate::sema::inference::RequirementTemplate::Eligibility { predicate: crate::sema::inference::Eligibility::MapKey, .. })));
        checked.solved.validate().unwrap();
    }
    let source = format!("{definition}let invalid = singleton(1.5, 2)\n");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
    let checked = Checker::check_arena(&parsed.arena, &source);
    assert!(!checked.diagnostics.is_empty());
}

#[test]
fn retry_completion_keeps_producer_wrapping_fixed_before_instantiation() {
    let declarations = "stream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 1 }\n";
    for completion in ["clocked()", "Ok(clocked())"] {
        for (effects, accepted) in [("time, error", true), ("error", false)] {
            let source = format!("{declarations}proc consume() [{effects}] -> Result[List[Int]] {{ let captured = retry [] {{ {completion} }}; captured?.collect() }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            if accepted { assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics); checked.solved.validate().unwrap(); }
            else { assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics); }
        }
    }
}

#[test]
fn result_constructors_retain_original_payloads_and_canonical_arity() {
    let source = "error LocalError = Failed(message: Str)\nlet empty: Result[Unit] = Ok()\nlet value: Result[Int] = Ok(7)\nlet data: Result[Int, Str] = Err(\"data\")\nlet caused: Result[Int, LocalError] = Err(cause: LocalError.Failed(message: \"cause\"), LocalError.Failed(message: \"outer\"))\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let mut constructors = Vec::new();
    for operation in checked.solved.operations.values() {
        let crate::sema::inference::RequirementTemplate::Operation { family, .. } = checked.solved.graph.requirement_template(operation.requirement).unwrap() else { continue; };
        let candidates = checked.solved.graph.family(family).unwrap();
        let super::SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, candidates[0]).unwrap() else { continue; };
        if let crate::sema::operation_graph::PreparedLanguageOperation::Constructor { kind, arity } = metadata.operation {
            if matches!(kind, crate::sema::operation_graph::ValueConstructor::Ok | crate::sema::operation_graph::ValueConstructor::Err) {
                constructors.push((kind, arity, operation));
            }
        }
    }
    assert_eq!(constructors.len(), 4);
    let caused = constructors.iter().find(|(_, arity, _)| *arity == 2).unwrap().2;
    assert_eq!(caused.binding.supplied_slots, vec![1, 0]);
    assert_eq!(caused.actual_arguments.len(), 2);
    drop(parsed);
    checked.solved.validate().unwrap();
}

#[test]
fn source_list_and_rest_splices_preserve_one_item_producer_path() {
    let definitions = "stream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 1 }\npure pack(...values: List[Stream[Int]]) -> List[Stream[Int]] { values }\nlet source = [clocked()]\n";
    for initializer in ["[@source]", "pack(@source)"] {
        for (effects, accepted) in [("time", true), ("", false)] {
            let source = format!("{definitions}let copied = {initializer}\nproc consume() [{effects}] -> List[Int] {{ copied[0].collect() }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            if accepted { assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics); checked.solved.validate().unwrap(); }
            else { assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{source}: {:?}", checked.diagnostics); }
        }
    }
}

#[test]
fn captured_producer_effect_diagnostic_retains_known_cleanup_bits() {
    let source = "stream delayed() [time, env] -> Stream[Int] { defer { let _ = env.get(\"SETTING\"); }; let _ = time.now(); yield 1 }\nlet rows = delayed()\nproc consume() [time] -> List[Int] { rows.collect() }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation") && diagnostic.message.contains("env")), "{:?}", checked.diagnostics);
}

#[test]
fn generic_display_strings_retain_child_display_eligibility() {
    let declarations = "pure render(value) { f\"value=$value\" }\npure forwarded(value) { render(value) }\n";
    for calls in ["let integer: Str = forwarded(7)\nlet text: Str = forwarded(\"word\")\n", "let text: Str = forwarded(\"word\")\nlet integer: Str = forwarded(7)\n"] {
        let source = format!("{declarations}{calls}");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let render = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "render").unwrap().1;
        assert!(render.source_requirements.iter().any(|requirement| matches!(checked.solved.graph.requirement_template(*requirement).unwrap(), crate::sema::inference::RequirementTemplate::Eligibility { predicate: crate::sema::inference::Eligibility::Display, .. })));
        drop(parsed);
        checked.solved.validate().unwrap();
    }
    let source = format!("{declarations}let rejected = forwarded([1])\n");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
    let checked = Checker::check_arena(&parsed.arena, &source);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.display-conversion")), "{:?}", checked.diagnostics);
}

#[test]
fn nested_graph_boundary_uses_resolved_field_types_before_rejecting_dynamic_data() {
    let symbols = crate::symbol::SymbolOwner::new();
    let _symbols = symbols.enter();
    let span = crate::source::Span::at(SourceId::new(0), 0);
    let mut checker = Checker::new(Default::default());
    let field = crate::symbol::Name::intern("blobs");
    let (key, value) = {
        let mut state = checker.generic.borrow_mut();
        (state.facts.graph.fresh(1, span).unwrap(), state.facts.graph.fresh(1, span).unwrap())
    };
    let expected = super::Type::Record(std::collections::BTreeMap::from([(
        field, super::Type::Map(Box::new(super::Type::Graph(key)), Box::new(super::Type::Graph(value))),
    )]));
    let actual = super::Type::Record(std::collections::BTreeMap::from([(
        field, super::Type::Map(Box::new(super::Type::Str), Box::new(super::Type::Any)),
    )]));
    checker.expect_type(&expected, &actual, span);
    assert!(checker.diagnostics.is_empty(), "an unresolved field retains dynamic data rather than imposing a concrete boundary: {:?}", checker.diagnostics);
    assert!(matches!(checker.generic.borrow().facts.graph.node(checker.generic.borrow().facts.graph.resolved(value).unwrap()).unwrap(), TypeNode::Atom(Atom::Any)));
    let known = {
        let mut state = checker.generic.borrow_mut();
        state.facts.graph.atom(Atom::Int).unwrap()
    };
    let expected = super::Type::Record(std::collections::BTreeMap::from([(
        field, super::Type::Map(Box::new(super::Type::Str), Box::new(super::Type::Graph(known))),
    )]));
    checker.expect_type(&expected, &actual, span);
    assert!(checker.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.dynamic-boundary")), "a fixed field still requires validation: {:?}", checker.diagnostics);
}

#[test]
fn erased_map_assignment_retains_inert_empty_container_source_scope() {
    for (declaration, sibling, mutable_count) in [("", "", 1), ("var children: Map[Any] = {}", ", children", 2)] {
        let source = format!("proc output() [error] -> Result[Unit] {{\n  {declaration}\n  var data: Map[Any] = {{}}\n  data[\"Total\"] = {{reports: []{sibling}}}\n  let encoded = json.encode(data)?\n  print $encoded\n}}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let mutable = checked.solved.bindings.values().filter(|binding| binding.mutable).collect::<Vec<_>>();
        assert_eq!(mutable.len(), mutable_count);
        for binding in mutable {
            assert!(binding.scheme.is_none(), "erasing a stored value must not generalize its mutable destination");
            let TypeNode::Map(_, value) = checked.solved.graph.node(checked.solved.graph.resolved(binding.ty).unwrap()).unwrap() else { panic!("the written map boundary is retained") };
            assert!(matches!(checked.solved.graph.node(checked.solved.graph.resolved(*value).unwrap()).unwrap(), TypeNode::Atom(Atom::Any)));
        }
        let literal = checked.solved.expression_schemes.iter().find(|(identity, _)| {
            let expression = parsed.arena.arena.expr(identity.expression);
            source.get(expression.span.range()).is_some_and(|text| text.starts_with("{reports:"))
        }).expect("the physical literal owns its otherwise unconstrained item type");
        assert_eq!(checked.solved.graph.scheme(*literal.1).unwrap().quantifiers.len(), 1);
        drop(parsed);
        checked.solved.validate().unwrap();
    }
}

#[test]
fn erased_container_literal_scope_keeps_mutable_capture_monomorphic() {
    let declarations = "pure retained(value) { var captured = value; let stored: Any = {reports: [], captured: captured}; captured }\n";
    for calls in ["let number: Int = retained(7)\nlet word: Str = retained(\"word\")\n", "let word: Str = retained(\"word\")\nlet number: Int = retained(7)\n"] {
        let source = format!("{declarations}{calls}");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let (&expression, &scope) = checked.solved.expression_schemes.iter().find(|(identity, _)| {
            matches!(parsed.arena.arena.expr(identity.expression).kind, crate::syntax::arena::ArenaExprKind::Record(_))
        }).unwrap();
        let scheme = checked.solved.graph.scheme(scope).unwrap();
        assert_eq!(scheme.quantifiers.len(), 1);
        assert!(!scheme.captures.is_empty());
        assert_eq!(checked.solved.expression_value_scopes.get(&expression), Some(&scope));
        for (identity, binding) in &checked.solved.bindings {
            if matches!(parsed.arena.arena.binding_target(identity.target).kind, crate::syntax::arena::ArenaBindingTargetKind::Name(name) if name == "stored" || name == "captured") {
                assert!(binding.scheme.is_none());
            }
        }
        drop(parsed);
        checked.solved.validate().unwrap();
    }
    let source = "pure rejected() { var captured = []; let stored: Any = {reports: [], captured: captured}; captured.push(1); captured.push(\"word\"); 0 }\nlet value = rejected()\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code.as_deref(), Some("check.type-mismatch" | "check.type-relationship"))), "{:?}", checked.diagnostics);
}

#[test]
fn ordinary_callable_producer_origins_keep_the_checked_reference_instance() {
    let source = "pure identity(value) { value }\npure invoke(callback, value) { callback(value) }\nlet values: List[Int] = invoke(identity, [1, 2])\n";
    let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(73), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let identity = checked.solved.expression_callables.iter().find_map(|(identity, callable)| {
        let expr = parsed.arena.arena.expr(identity.expression);
        (source[expr.span.range()] == *"identity" && callable.declaration.is_some()
            && checked.solved.expressions[identity] != callable.signature).then_some(*identity)
    }).expect("the passed reference keeps its actual instance independently of the principal signature");
    let flow = checked.solved.expression_producer_flows[&identity];
    let super::ProducerFlowKind::Callable { origin: Some(origin), .. } = checked.solved.producer_flows.node(flow).unwrap().kind else { panic!("ordinary reference retains its immutable source origin") };
    assert_eq!(origin, checked.solved.expressions[&identity]);
    let principal = checked.solved.expression_callables[&identity].signature;
    assert_ne!(origin, principal);
    drop(parsed);
    let before = checked.solved.graph.counters().clone();
    checked.solved.validate().unwrap();
    let solved = std::sync::Arc::get_mut(&mut checked.solved).unwrap();
    let original = solved.expressions.insert(identity, principal).unwrap();
    assert!(matches!(checked.solved.validate(), Err(crate::sema::inference::InferenceError::InvalidScheme)), "a principal signature cannot replace the reference instance without its producer origin");
    std::sync::Arc::get_mut(&mut checked.solved).unwrap().expressions.insert(identity, original);
    checked.solved.validate().unwrap();
    let after = checked.solved.graph.counters();
    assert_eq!(before.attempted_constraints, after.attempted_constraints);
    assert_eq!(before.unifications, after.unifications);
    assert_eq!(before.instantiations, after.instantiations);
}

#[test]
fn conditional_user_callables_keep_each_original_binding_and_default_plan() {
    let source = "pure left(value: Int) -> Int { value }\npure right(value: Int, extra: Int = 2) -> Int { value + extra }\npure choose(flag: Bool) { if flag { (left) } else { (right) } }\nlet callback = choose(true)\nlet value: Int = callback(value: 7)\n";
    let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(74), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let identity = *checked.solved.invocations.keys().find(|identity| source[parsed.arena.arena.expr(identity.expression).span.range()].starts_with("callback(")).expect("the computed conditional call retains its original invocation");
    let evidence = checked.solved.graph.invocation_evidence(checked.solved.invocations[&identity].requirement).unwrap().unwrap();
    let crate::sema::inference::InvocationPlan::All { branches } = &evidence.plan else { panic!("both possible user callables retain their own plans") };
    assert_eq!(branches.len(), 2);
    let mut defaults = Vec::new();
    for branch in branches {
        let crate::sema::inference::CallableAuthority::User { origin, .. } = branch.authority else { panic!("the branch authority is an original user callable") };
        assert!(checked.solved.expression_producer_flows.values().any(|flow| matches!(checked.solved.producer_flows.node(*flow).unwrap().kind, super::ProducerFlowKind::Callable { origin: Some(found), .. } if found == origin)));
        defaults.push(branch.binding.default_slots.clone());
        assert_eq!(branch.binding.supplied_slots, vec![0]);
    }
    defaults.sort();
    assert_eq!(defaults, vec![Vec::<usize>::new(), vec![1]]);
    assert!(!checked.solved.calls.contains_key(&identity), "an ALL invocation has no shared static call plan");
    drop(parsed);
    checked.solved.validate().unwrap();

    let rejected = format!("{source}let missing: Int = callback()\n");
    let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(75), &rejected);
    let checked = Checker::check_arena(&parsed.arena, &rejected);
    assert!(!checked.diagnostics.is_empty(), "every possible branch must admit the invocation");
}

#[test]
fn conditional_user_dynamic_splices_keep_original_sources_and_branch_rest_defaults() {
    use crate::sema::inference::{InvocationArgumentKind, InvocationArgumentSegment, InvocationDefaultTiming, InvocationPlan, RequirementTemplate};
    for (left, right, named) in [("fixed", "rested", false), ("rested", "fixed", false), ("fixed", "rested", true), ("rested", "fixed", true)] {
        let source = format!("pure fixed(first: Int, second: Int = 2) -> Int {{ first + second }}\npure rested(first: Int = 3, ...others: List[Int]) -> Int {{ first }}\npure choose(flag: Bool) {{ if flag {{ ({left}) }} else {{ ({right}) }} }}\nlet callback = choose(true)\nlet parts = [7]\nlet answer: Int = callback(@parts)\n");
        let source = if named { format!("{source}let named: Int = callback(@parts, first: 9)\n") } else { source };
        let parsed = Parser::parse_source_arena_only(SourceId::new(76), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let invocations = checked.solved.invocations.iter().filter(|(identity, _)| source[parsed.arena.arena.expr(identity.expression).span.range()].starts_with("callback(@parts")).collect::<Vec<_>>();
        assert_eq!(invocations.len(), if named { 2 } else { 1 });
        for (identity, invocation) in invocations {
            let RequirementTemplate::CallableInvocation { call } = checked.solved.graph.requirement_template(invocation.requirement).unwrap() else { panic!("the source invocation owns its original ledger") };
            let original = checked.solved.graph.invocation_call(call).unwrap();
            let expression = parsed.arena.arena.expr(identity.expression);
            let crate::syntax::arena::ArenaExprKind::Call { args, .. } = expression.kind else { panic!() };
            let arguments = parsed.arena.arena.call_args(args);
            let crate::syntax::arena::ArenaCallArgKind::Splice { value: spread, .. } = arguments[0].kind else { panic!("the checked splice remains the original source expression") };
            let spread = super::ExpressionIdentity { source: SourceId::new(76), namespace: None, expression: spread };
            assert_eq!(original.arguments[0].kind, InvocationArgumentKind::PositionalSplice);
            assert_eq!(original.arguments[0].ty, checked.solved.expressions[&spread], "the invocation retains the checked source endpoint before any branch binding");
            if arguments.len() == 2 {
                let crate::syntax::arena::ArenaCallArgKind::Named { name, value, .. } = arguments[1].kind else { panic!("the original named argument keeps its source mode") };
                let value = super::ExpressionIdentity { source: SourceId::new(76), namespace: None, expression: value };
                assert_eq!(original.arguments[1].kind, InvocationArgumentKind::Named(name));
                assert_eq!(original.arguments[1].ty, checked.solved.expressions[&value]);
            }

            let flow = checked.solved.expression_producer_flows[identity];
            let super::ProducerFlowKind::Apply { arguments: inputs, .. } = &checked.solved.producer_flows.node(flow).unwrap().kind else { panic!("source argument flows remain attached to the invocation") };
            assert_eq!(inputs[0], checked.solved.expression_producer_flows[&spread]);
            let evidence = checked.solved.graph.invocation_evidence(invocation.requirement).unwrap().unwrap();
            let InvocationPlan::All { branches } = &evidence.plan else { panic!("every possible user branch retains its own plan") };
            assert_eq!(branches.len(), 2);
            let mut rest_slots = Vec::new();
            for branch in branches {
                assert_eq!(branch.timing, InvocationDefaultTiming::AtCall);
                assert!(branch.binding.supplied_slots.is_empty());
                let dynamic = branch.binding.dynamic.as_ref().expect("unknown cardinality remains dynamic for each branch");
                assert!(dynamic.runtime_arity_guard && dynamic.runtime_duplicate_guard);
                if branch.binding.rest_slot.is_some() {
                    assert_eq!(dynamic.segments[0], InvocationArgumentSegment::DynamicRange { argument: 0, fixed_slots: vec![0], rest_slot: Some(1) });
                    assert!(dynamic.required_slots.is_empty());
                    assert_eq!(dynamic.conditional_default_slots, if arguments.len() == 1 { vec![0] } else { vec![] });
                } else {
                    assert_eq!(dynamic.segments[0], InvocationArgumentSegment::DynamicRange { argument: 0, fixed_slots: vec![0, 1], rest_slot: None });
                    assert_eq!(dynamic.required_slots, vec![0]);
                    assert_eq!(dynamic.conditional_default_slots, if arguments.len() == 1 { vec![1] } else { vec![] });
                    assert_eq!(branch.binding.default_slots, if arguments.len() == 1 { vec![] } else { vec![1] });
                }
                if arguments.len() == 2 { assert_eq!(dynamic.segments[1], InvocationArgumentSegment::StaticSlot { argument: 1, slot: 0 }); }
                rest_slots.push(branch.binding.rest_slot);
            }
            rest_slots.sort();
            assert_eq!(rest_slots, vec![None, Some(1)]);
            assert!(!checked.solved.calls.contains_key(identity));
        }
        drop(parsed);
        let before = checked.solved.graph.counters().clone();
        checked.solved.validate().unwrap();
        let after = checked.solved.graph.counters();
        assert_eq!(before.attempted_constraints, after.attempted_constraints);
        assert_eq!(before.unifications, after.unifications);
        assert_eq!(before.instantiations, after.instantiations);
        for invalid in [source.replace("[7]", "[false]"), format!("{source}let denied: Int = callback(@parts, second: 9)\n")] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(77), &invalid);
            let checked = Checker::check_arena(&parsed.arena, &invalid);
            assert!(!checked.diagnostics.is_empty(), "both possible branch contracts must admit the original argument modes");
        }
    }
}

#[test]
fn conditional_user_dynamic_splices_keep_each_callable_kind_and_creation_budget() {
    use crate::sema::inference::{CallableKind, EffectSet, InvocationPlan};
    let definitions = "pure fixed(first: Int, second: Int = 2) -> Int { first + second }\nproc rested(first: Int = 3, ...others: List[Int]) [time] -> Int { let _ = time.now(); first }\npure choose(flag: Bool) { if flag { (fixed) } else { (rested) } }\nlet callback = choose(true)\n";
    let source = format!("{definitions}proc allowed() [time] -> Int {{ let parts = [7]; callback(@parts) }}\n");
    let parsed = Parser::parse_source_arena_only(SourceId::new(78), &source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, &source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let invocation = checked.solved.invocations.iter().find(|(identity, _)| source[parsed.arena.arena.expr(identity.expression).span.range()].starts_with("callback(@parts")).unwrap().1;
    let evidence = checked.solved.graph.invocation_evidence(invocation.requirement).unwrap().unwrap();
    let InvocationPlan::All { branches } = &evidence.plan else { panic!("the original Pure and Proc promises remain distinct") };
    let mut kinds = Vec::new();
    let mut effects = Vec::new();
    for branch in branches {
        let TypeNode::Arrow(arrow) = checked.solved.graph.node(checked.solved.graph.resolved(branch.signature).unwrap()).unwrap() else { panic!() };
        kinds.push(arrow.kind);
        effects.push(checked.solved.graph.resolved_effect_summary(branch.effects).unwrap());
        assert!(branch.binding.dynamic.is_some());
    }
    assert!(kinds.contains(&CallableKind::Pure) && kinds.contains(&CallableKind::Proc));
    assert!(effects.contains(&crate::sema::inference::EffectSummary::Closed(EffectSet::EMPTY)));
    assert!(effects.contains(&crate::sema::inference::EffectSummary::Closed(EffectSet::TIME)));
    drop(parsed);
    checked.solved.validate().unwrap();
    for denied in [format!("{definitions}proc denied() [] -> Int {{ let parts = [7]; callback(@parts) }}\n"), format!("{definitions}pure denied() -> Int {{ let parts = [7]; callback(@parts) }}\n")] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(79), &denied);
        let checked = Checker::check_arena(&parsed.arena, &denied);
        assert!(checked.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code.as_deref(), Some("check.effect-violation" | "check.pure-effect"))), "{:?}", checked.diagnostics);
    }
}

#[test]
fn local_nullable_conditionals_keep_the_concrete_payload_type_in_both_orders() {
    for (left, right) in [("null", "value"), ("value", "null")] {
        let source = format!("pure absent(flag: Bool, value: Int) -> Bool {{ let optional = if flag {{ {left} }} else {{ {right} }}; optional is null }}\nlet answer: Bool = absent(false, 7)\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(80), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        let identity = *checked.solved.expressions.keys().find(|identity| source[parsed.arena.arena.expr(identity.expression).span.range()].starts_with("if flag")).unwrap();
        assert_eq!(checked.solved.graph.export_type(checked.solved.expressions[&identity]).unwrap(), super::Type::Optional(Box::new(super::Type::Int)));
        drop(parsed);
        checked.solved.validate().unwrap();
    }
}

#[test]
fn independent_conditional_branches_preserve_checked_unsigned_result_domains() {
    for expression in ["if flag { good } else { 1 }", "if flag { good } else { value }", "match flag { true => good, false => value }"] {
        let source = format!("pure selected(flag: Bool, value: Int) -> Int {{ let good: UInt = 1; let selected = {expression}; selected }}\nlet answer: Int = selected(false, 7)\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(85), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        let identity = *checked.solved.expressions.keys().find(|identity| &source[parsed.arena.arena.expr(identity.expression).span.range()] == expression).unwrap();
        assert_eq!(checked.solved.graph.export_type(checked.solved.expressions[&identity]).unwrap(), super::Type::UInt);
        if expression.contains("value }") {
            let declaration = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "selected").unwrap().1;
            let TypeNode::Arrow(arrow) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!() };
            assert_eq!(checked.solved.graph.export_type(arrow.params[1].ty).unwrap(), super::Type::Int, "the checked result does not change the source argument's domain");
        }
        drop(parsed);
        checked.solved.validate().unwrap();
    }
}

#[test]
fn nullable_conditional_payloads_remain_principal_and_do_not_nest_optional_layers() {
    let source = "pure nullable(flag: Bool, value) { if flag { null } else { value } }\npure forwarded(flag: Bool, value) { nullable(flag, value) }\nlet integer: Int? = forwarded(false, 7)\nlet optional: Int? = 9\nlet flattened: Int? = forwarded(false, optional)\nlet word: Str? = forwarded(true, \"word\")\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(81), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declaration = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "nullable").unwrap().1;
    assert!(!checked.solved.graph.scheme(declaration.scheme).unwrap().quantifiers.is_empty());
    drop(parsed);
    checked.solved.validate().unwrap();
    let denied = "pure mixed(flag: Bool) { let value = if flag { 1 } else { \"word\" }; value }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(82), denied);
    assert!(!Checker::check_arena(&parsed.arena, denied).diagnostics.is_empty(), "nullable joins do not admit unrelated scalar alternatives");
}

#[test]
fn nullable_conditional_producers_keep_the_original_optional_payload_permissions() {
    use super::{ProducerEffects, ProducerPath, ProducerPathComponent};
    use crate::sema::inference::{EffectSet, EffectSummary};
    for reverse in [false, true] {
        let definitions = "stream clocked() [time] -> Stream[Int] { let _ = time.now(); yield 1 }\npure nullable(flag: Bool, value) { if flag { null } else { value } }\npure forwarded(flag: Bool, value) { nullable(flag, value) }\nlet original = nullable(false, clocked())\n";
        let mut values = ["let raw_rows = forwarded(false, clocked())\n", "let wrapped_rows = forwarded(false, original)\n"].to_vec();
        if reverse { values.reverse(); }
        let source = format!("{definitions}{}proc allowed() [time] -> List[Int]? {{ let _ = raw_rows?.collect(); wrapped_rows?.collect() }}\n", values.concat());
        let parsed = Parser::parse_source_arena_only(SourceId::new(83), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let payload = ProducerPath(vec![ProducerPathComponent::OptionalPayload]);
        for name in ["original", "raw_rows", "wrapped_rows"] {
            let rows = *checked.solved.bindings.keys().find(|identity| matches!(parsed.arena.arena.binding_target(identity.target).kind, crate::syntax::arena::ArenaBindingTargetKind::Name(found) if found == name)).unwrap();
            assert_eq!(checked.solved.binding_producers[&rows].get(&payload), Some(&ProducerEffects { pull: EffectSummary::Closed(EffectSet::TIME), close: EffectSummary::Closed(EffectSet::EMPTY) }));
            assert!(!checked.solved.binding_producers[&rows].keys().any(|path| path.0.starts_with(&[ProducerPathComponent::OptionalPayload, ProducerPathComponent::OptionalPayload])), "one nullable layer preserves the original payload");
        }
        drop(parsed);
        let counters = checked.solved.graph.counters().clone();
        checked.solved.validate().unwrap();
        assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
        for name in ["raw_rows", "wrapped_rows"] {
            let denied = format!("{definitions}{}proc denied() [] -> List[Int]? {{ {name}?.collect() }}\n", values.concat());
            let parsed = Parser::parse_source_arena_only(SourceId::new(84), &denied);
            let checked = Checker::check_arena(&parsed.arena, &denied);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics);
        }
    }
}
