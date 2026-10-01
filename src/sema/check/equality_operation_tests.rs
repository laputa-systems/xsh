use super::{Checker, SolvedOperationAuthority, Type};
use crate::sema::inference::{EffectSet, EffectSummary};
use crate::sema::operation_graph::PreparedLanguageOperation;
use crate::source::SourceId;
use crate::syntax::node::BinaryOp;
use crate::syntax::parser::Parser;

#[test]
fn generalized_equality_source_domains_keep_exact_authority_after_frontend_disposal() {
    let domains = [
        ("Int", "1", "Int", "2"),
        ("UInt", "1", "Int", "2"),
        ("Float", "1.0", "Float", "2.0"),
        ("Bool", "true", "Bool", "false"),
        ("Str", "\"one\"", "Str", "\"two\""),
        ("Bytes", "b\"one\"", "Bytes", "b\"two\""),
        ("Path", "p\"one\"", "Path", "p\"two\""),
        ("Duration", "1s", "Duration", "2s"),
        ("Int?", "null", "Int?", "1"),
        ("Int?", "1", "Null", "null"),
        ("Null", "null", "Int?", "1"),
        ("List[Int]", "[1]", "List[Int]", "[2]"),
        ("List[Int?]", "[null]", "List[Int?]", "[1]"),
        ("Map[Str, Int]", "{[\"one\"]: 1}", "Map[Str, Int]", "{[\"two\"]: 2}"),
        ("Choice", "Selected(1)", "ChoiceAlias", "Selected(2)"),
    ];
    for (operator, expected) in [("==", BinaryOp::Eq), ("!=", BinaryOp::Ne)] {
        for reverse in [false, true] {
            let mut selected = domains.to_vec();
            if reverse { selected.reverse(); }
            let mut source = format!("enum Choice {{ Selected(Int) }}\ntype ChoiceAlias = Choice\npure compared(left, right) {{ left {operator} right }}\npure forwarded(left, right) {{ compared(left, right) }}\n");
            for (index, (left, lhs, right, rhs)) in selected.iter().enumerate() {
                source.push_str(&format!("let left_{index}: {left} = {lhs}\nlet right_{index}: {right} = {rhs}\nlet direct_{index}: Bool = compared(left_{index}, right_{index})\nlet forwarded_{index}: Bool = forwarded(left_{index}, right_{index})\n"));
            }
            let parsed = Parser::parse_source_arena_only(SourceId::new(56), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{operator}: {:?}", checked.diagnostics);
            let source_expression = format!("left {operator} right");
            let (identity, operation) = checked.solved.operations.iter().find(|(identity, _)|
                source[parsed.arena.arena.expr(identity.expression).span.range()] == source_expression).expect("one original comparison expression retains its operation");
            let identity = *identity;
            let requirement = operation.requirement;
            let caller = operation.caller.expect("the definition owns equality");
            assert_eq!(identity.source, SourceId::new(56));
            assert_eq!(checked.solved.expression_owners.get(&identity), Some(&caller));
            assert!(checked.solved.declarations[&caller].source_requirements.contains(&requirement));
            assert_eq!(operation.actual_arguments.len(), 2);
            assert_ne!(checked.solved.graph.resolved(operation.actual_arguments[0]).unwrap(), checked.solved.graph.resolved(operation.actual_arguments[1]).unwrap(), "equality does not identify independent operand variables");
            assert_eq!(operation.binding.supplied_slots, [0, 1]);
            assert_eq!(operation.effects, EffectSummary::Closed(EffectSet::EMPTY));
            assert_eq!(checked.solved.graph.export_type(operation.result).unwrap(), Type::Bool);
            assert!(checked.solved.graph.candidate_evidence(requirement).unwrap().is_none(), "the source definition does not choose one observed operand instance");
            let evidence = checked.solved.calls.values().filter(|call| call.caller.is_none())
                .flat_map(|call| &call.requirements).filter(|&&id| checked.solved.graph.requirement_origin(id).unwrap() == requirement)
                .map(|&id| checked.solved.graph.candidate_evidence(id).unwrap().expect("each concrete caller discharges the exact original equality requirement")).collect::<Vec<_>>();
            assert_eq!(evidence.len(), domains.len() * 2);
            let counters = checked.solved.graph.counters().clone();
            drop(parsed);
            checked.solved.validate().unwrap();
            for selected in evidence {
                let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, selected.candidate).unwrap() else { panic!("equality cannot select foreign operation authority") };
                assert_eq!(metadata.authority, format!("language.binary.{expected:?}"));
                assert_eq!(metadata.operation, PreparedLanguageOperation::Equality { op: expected });
            }
            let query = crate::frontend::query::SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
            let normalized = query.language_operation(identity).unwrap();
            assert_eq!(normalized.binding.supplied_slots, [0, 1]);
            assert!(normalized.semantic_parity(&query.language_operation(identity).unwrap()).unwrap());
            assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
            assert_eq!(checked.solved.graph.counters().unifications, counters.unifications);
        }
    }
}

#[test]
fn generalized_equality_rejects_container_and_nominal_contract_mismatches() {
    for operator in ["==", "!="] {
        for (left, lhs, right, rhs) in [
            ("Int", "1", "Float", "1.0"),
            ("Bool", "true", "Str", "\"true\""),
            ("List[Int?]", "[null]", "List[Int]", "[1]"),
            ("Map[Str, Int?]", "{[\"one\"]: null}", "Map[Str, Int]", "{[\"one\"]: 1}"),
            ("First", "FirstValue(1)", "Second", "SecondValue(1)"),
        ] {
            let source = format!("enum First {{ FirstValue(Int) }}\nenum Second {{ SecondValue(Int) }}\npure compared(left, right) {{ left {operator} right }}\npure forwarded(left, right) {{ compared(left, right) }}\nlet left: {left} = {lhs}\nlet right: {right} = {rhs}\nlet invalid: Bool = forwarded(left, right)\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(57), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{source}: {:?}", checked.diagnostics);
        }
    }
}

#[test]
fn equality_operand_permissions_remain_independent_of_the_operator() {
    for operator in ["==", "!="] {
        for (permissions, accepted) in [("time, env", true), ("time", false), ("env", false)] {
            let source = format!("pure compared(left, right) {{ left {operator} right }}\npure forwarded(left, right) {{ compared(left, right) }}\nproc clock() [time] -> Int {{ let _ = time.now(); 1 }}\nproc setting() [env] -> Int {{ let _ = env.get(\"SETTING\"); 2 }}\nproc observed() [{permissions}] -> Bool {{ forwarded(clock(), setting()) }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(58), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            if accepted {
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let operators = checked.solved.operations.iter().filter(|(identity, _)| source[parsed.arena.arena.expr(identity.expression).span.range()] == format!("left {operator} right")).collect::<Vec<_>>();
                assert_eq!(operators.len(), 1);
                assert_eq!(operators[0].1.effects, EffectSummary::Closed(EffectSet::EMPTY));
                drop(parsed);
                checked.solved.validate().unwrap();
            } else {
                assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{permissions}: {:?}", checked.diagnostics);
            }
        }
    }
}

#[test]
fn equality_keeps_distinct_imported_nominal_owners_with_equal_member_names() {
    use crate::symbol::Name;
    use crate::syntax::arena::ArenaProgramBuilder;

    for operator in ["==", "!="] {
        for left_owner in ["first", "second"] {
            for same_owner in [true, false] {
                let right_owner = if same_owner { left_owner } else if left_owner == "first" { "second" } else { "first" };
                let right = format!("{right_owner}.Value(2)");
                let source = format!("use first\nuse second\npure compared(left, right) {{ left {operator} right }}\npure forwarded(left, right) {{ compared(left, right) }}\nlet result: Bool = forwarded({left_owner}.Value(1), {right})\n");
                let module_source = "##! A module-owned nominal token.\n## A token payload.\nexport enum Token { Value(Int) }\n";
                let mut builder = ArenaProgramBuilder::with_token_capacity(128);
                let entry = Parser::parse_source_into_arena_builder(SourceId::new(59), &source, &mut builder);
                let first = Parser::parse_source_into_arena_builder(SourceId::new(60), module_source, &mut builder);
                let second = Parser::parse_source_into_arena_builder(SourceId::new(61), module_source, &mut builder);
                assert!(entry.diagnostics.is_empty() && first.diagnostics.is_empty() && second.diagnostics.is_empty());
                let (first_namespace, second_namespace, first_import) = builder.symbol_owner().with_current(|| (Name::intern("first-owner"), Name::intern("second-owner"), Name::intern("first")));
                for statement in builder.statement_ids(entry.statements) {
                    if let Some((import, module, _)) = builder.use_stmt_for_statement(statement) {
                        let resolved = if module.as_slice() == [first_import] { "first-owner" } else { "second-owner" };
                        builder.set_use_resolved(import, std::sync::Arc::from(resolved));
                    }
                }
                builder.push_arena_module("first-owner".to_string(), first_namespace, first.statements);
                builder.push_arena_module("second-owner".to_string(), second_namespace, second.statements);
                let program = builder.finish_with_statements(entry.statements);
                let checked = Checker::check_arena(&program, &source);
                if same_owner {
                    assert!(checked.diagnostics.is_empty(), "{operator}: {:?}", checked.diagnostics);
                    let operation = checked.solved.operations.values().find(|operation| operation.caller.is_some()).unwrap();
                    let source_requirement = operation.requirement;
                    let evidence = checked.solved.calls.values().filter(|call| call.caller.is_none()).flat_map(|call| &call.requirements)
                        .filter(|&&requirement| checked.solved.graph.requirement_origin(requirement).unwrap() == source_requirement)
                        .map(|&requirement| checked.solved.graph.candidate_evidence(requirement).unwrap().unwrap()).collect::<Vec<_>>();
                    assert_eq!(evidence.len(), 1);
                    drop(program);
                    checked.solved.validate().unwrap();
                    assert!(matches!(checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence[0].candidate).unwrap(), SolvedOperationAuthority::Language(metadata) if metadata.authority == format!("language.binary.{}", if operator == "==" { "Eq" } else { "Ne" })));
                } else {
                    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "equal member spellings cannot erase declaring module identity: {:?}", checked.diagnostics);
                }
            }
        }
    }
}
