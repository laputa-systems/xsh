use super::{Checker, SolvedOperationAuthority, StatementPosition, Type};
use crate::sema::inference::{Atom, EffectSet, EffectSummary, SealedOperation, TypeNode};
use crate::sema::operation_graph::{ArithmeticDomain, PreparedLanguageOperation};
use crate::source::SourceId;
use crate::syntax::parser::Parser;

fn assert_effect_refusal(source: &str, source_id: usize) {
    let parsed = Parser::parse_source_arena_only(SourceId::new(source_id), source);
    assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{source}: {:?}", checked.diagnostics);
}

fn domain_type(name: &str) -> Type {
    match name {
        "Int" => Type::Int, "UInt" => Type::UInt, "Float" => Type::Float,
        "Str" => Type::Str, "Bytes" => Type::Bytes, "Bool" => Type::Bool,
        "Path" => Type::Path, "Duration" => Type::Duration,
        "List[Int]" => Type::List(Box::new(Type::Int)),
        "List[Str]" => Type::List(Box::new(Type::Str)),
        _ => panic!("fixture domain has no closed type: {name}"),
    }
}

#[test]
fn arithmetic_each_domain_retains_original_receipts_and_reached_operand_permissions() {
    let cases = [
        ("+", "Int", "7", "Int", "2", "Int", "AddInt"),
        ("+", "UInt", "7", "UInt", "2", "Int", "AddInt"),
        ("+", "UInt", "7", "Int", "2", "Int", "AddInt"),
        ("+", "Int", "7", "UInt", "2", "Int", "AddInt"),
        ("+", "Float", "7.0", "Float", "2.0", "Float", "AddFloat"),
        ("+", "Str", "\"a\"", "Str", "\"b\"", "Str", "AddStr"),
        ("+", "List[Int]", "[1]", "List[Int]", "[2]", "List[Int]", "AddList"),
        ("+", "List[Str]", "[\"a\"]", "List[Str]", "[\"b\"]", "List[Str]", "AddList"),
        ("+", "Duration", "7s", "Duration", "2s", "Duration", "AddDuration"),
        ("-", "Int", "7", "Int", "2", "Int", "integer"),
        ("-", "UInt", "7", "UInt", "2", "Int", "integer"),
        ("-", "UInt", "7", "Int", "2", "Int", "integer"),
        ("-", "Int", "7", "UInt", "2", "Int", "integer"),
        ("-", "Float", "7.0", "Float", "2.0", "Float", "float"),
        ("-", "Duration", "7s", "Duration", "2s", "Duration", "duration"),
        ("*", "Int", "7", "Int", "2", "Int", "integer"),
        ("*", "UInt", "7", "UInt", "2", "Int", "integer"),
        ("*", "UInt", "7", "Int", "2", "Int", "integer"),
        ("*", "Int", "7", "UInt", "2", "Int", "integer"),
        ("*", "Float", "7.0", "Float", "2.0", "Float", "float"),
        ("*", "Duration", "7s", "Int", "2", "Duration", "duration_scale"),
        ("*", "Int", "2", "Duration", "7s", "Duration", "duration_scale_reverse"),
        ("/", "Int", "7", "Int", "2", "Int", "integer"),
        ("/", "UInt", "7", "UInt", "2", "Int", "integer"),
        ("/", "UInt", "7", "Int", "2", "Int", "integer"),
        ("/", "Int", "7", "UInt", "2", "Int", "integer"),
        ("/", "Float", "7.0", "Float", "2.0", "Float", "float"),
        ("/", "Duration", "7s", "Int", "2", "Duration", "duration_scale"),
        ("/", "Duration", "7s", "Duration", "2s", "Int", "duration_ratio"),
        ("%", "Int", "7", "Int", "2", "Int", "integer"),
        ("%", "UInt", "7", "UInt", "2", "Int", "integer"),
        ("%", "UInt", "7", "Int", "2", "Int", "integer"),
        ("%", "Int", "7", "UInt", "2", "Int", "integer"),
    ];
    for (operator, left, lhs, right, rhs, result, authority) in cases {
        for permissions in ["time, env", "time", "env"] {
            let source = format!("pure operate(left, right) {{ left {operator} right }}\npure forwarded(left, right) {{ operate(left, right) }}\nproc clock() [time] -> {left} {{ let _ = time.now(); {lhs} }}\nproc setting() [env] -> {right} {{ let _ = env.get(\"SETTING\"); {rhs} }}\nproc observed() [{permissions}] -> {result} {{ forwarded(clock(), setting()) }}\n");
            if permissions != "time, env" { assert_effect_refusal(&source, 77); continue; }
            let parsed = Parser::parse_source_arena_only(SourceId::new(77), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let (identity, _) = checked.solved.expressions.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == format!("left {operator} right")).unwrap();
            let identity = *identity;
            let owner = checked.solved.expression_owners[&identity];
            for (name, domain) in [("clock", left), ("setting", right)] {
                let declaration = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == name).unwrap().1;
                let TypeNode::Arrow(signature) = checked.solved.graph.node(declaration.signature).unwrap() else { unreachable!() };
                assert_eq!(checked.solved.graph.export_type(signature.result).unwrap(), domain_type(domain));
            }
            let requirement = if operator == "+" { checked.solved.additions[&identity] } else { checked.solved.operations[&identity].requirement };
            assert!(checked.solved.declarations[&owner].source_requirements.contains(&requirement));
            let observed = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "observed").unwrap().1;
            let TypeNode::Arrow(signature) = checked.solved.graph.node(observed.signature).unwrap() else { unreachable!() };
            assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0)));
            let receipts = checked.solved.calls.values().flat_map(|call| &call.requirements).filter(|&&instance| checked.solved.graph.requirement_origin(instance).unwrap() == requirement)
                .filter(|&&instance| if operator == "+" { checked.solved.graph.discharge(instance).unwrap().is_some() } else { checked.solved.graph.candidate_evidence(instance).unwrap().is_some() }).copied().collect::<Vec<_>>();
            assert_eq!(receipts.len(), 1, "the reached concrete caller selects the original source relationship: {source}");
            let counters = checked.solved.graph.counters().clone();
            drop(parsed);
            checked.solved.validate().unwrap();
            assert_eq!(identity.source, SourceId::new(77));
            assert_eq!(checked.solved.expression_owners[&identity], owner);
            for receipt in receipts {
                assert_eq!(checked.solved.graph.requirement_origin(receipt).unwrap(), requirement);
                if operator == "+" {
                    let evidence = checked.solved.graph.discharge(receipt).unwrap().unwrap();
                    let expected = match authority { "AddInt" => SealedOperation::AddInt, "AddFloat" => SealedOperation::AddFloat, "AddStr" => SealedOperation::AddStr, "AddList" => SealedOperation::AddList, "AddDuration" => SealedOperation::AddDuration, _ => unreachable!() };
                    assert_eq!(evidence.operation, expected);
                    if evidence.operation == SealedOperation::AddInt {
                        for operand in [evidence.left, evidence.right] { assert!(matches!(checked.solved.graph.export_type(operand).unwrap(), Type::Int | Type::UInt)); }
                    } else {
                        assert_eq!(checked.solved.graph.export_type(evidence.left).unwrap(), domain_type(left));
                        assert_eq!(checked.solved.graph.export_type(evidence.right).unwrap(), domain_type(right));
                    }
                    assert_eq!(checked.solved.graph.export_type(evidence.result).unwrap(), domain_type(result));
                } else {
                    let evidence = checked.solved.graph.candidate_evidence(receipt).unwrap().unwrap();
                    let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap() else { panic!("arithmetic retains its exact language authority") };
                    let PreparedLanguageOperation::Arithmetic { domain, .. } = metadata.operation else { panic!("the receipt must authorize arithmetic") };
                    let kind = match domain { ArithmeticDomain::Integer { left: actual_left, right: actual_right } => {
                        assert_eq!(actual_left, if left == "UInt" { Atom::UInt } else { Atom::Int });
                        assert_eq!(actual_right, if right == "UInt" { Atom::UInt } else { Atom::Int });
                        "integer"
                    }, ArithmeticDomain::Float => "float", ArithmeticDomain::DurationPair => "duration", ArithmeticDomain::DurationScale { duration_left: true } => "duration_scale", ArithmeticDomain::DurationScale { duration_left: false } => "duration_scale_reverse", ArithmeticDomain::DurationRatio => "duration_ratio", _ => panic!("unexpected arithmetic domain") };
                    assert_eq!(kind, authority);
                    assert_eq!(checked.solved.operations[&identity].effects, EffectSummary::Closed(EffectSet::EMPTY));
                }
            }
            assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
            assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        }
    }
}

#[test]
fn assignment_each_domain_keeps_monomorphic_storage_and_reached_operand_permissions() {
    let cases = [
        ("=", "Int", "7", "Int", "2", "Int", "Set"),
        ("=", "UInt", "7", "UInt", "2", "UInt", "Set"),
        ("=", "Float", "7.0", "Float", "2.0", "Float", "Set"),
        ("=", "Str", "\"a\"", "Str", "\"b\"", "Str", "Set"),
        ("=", "List[Int]", "[1]", "List[Int]", "[2]", "List[Int]", "Set"),
        ("=", "Duration", "7s", "Duration", "2s", "Duration", "Set"),
        ("+=", "Int", "7", "Int", "2", "Int", "Add"),
        ("+=", "UInt", "7", "Int", "2", "UInt", "Add"),
        ("+=", "Float", "7.0", "Float", "2.0", "Float", "Add"),
        ("+=", "List[Int]", "[1]", "List[Int]", "[2]", "List[Int]", "Add"),
        ("+=", "List[Str]", "[\"a\"]", "List[Str]", "[\"b\"]", "List[Str]", "Add"),
        ("+=", "Duration", "7s", "Duration", "2s", "Duration", "Add"),
        ("-=", "Int", "7", "Int", "2", "Int", "Sub"),
        ("-=", "UInt", "7", "Int", "2", "UInt", "Sub"),
        ("-=", "Float", "7.0", "Float", "2.0", "Float", "Sub"),
        ("-=", "Duration", "7s", "Duration", "2s", "Duration", "Sub"),
        ("*=", "Int", "7", "Int", "2", "Int", "Mul"),
        ("*=", "UInt", "7", "Int", "2", "UInt", "Mul"),
        ("*=", "Float", "7.0", "Float", "2.0", "Float", "Mul"),
        ("*=", "Duration", "7s", "Int", "2", "Duration", "Mul"),
        ("/=", "Int", "7", "Int", "2", "Int", "Div"),
        ("/=", "UInt", "7", "Int", "2", "UInt", "Div"),
        ("/=", "Float", "7.0", "Float", "2.0", "Float", "Div"),
        ("/=", "Duration", "7s", "Int", "2", "Duration", "Div"),
        ("/=", "Path", "p\"root\"", "Str", "\"child\"", "Path", "Div"),
        ("/=", "Path", "p\"root\"", "Path", "p\"child\"", "Path", "Div"),
        ("%=", "Int", "7", "Int", "2", "Int", "Rem"),
        ("%=", "UInt", "7", "Int", "2", "UInt", "Rem"),
    ];
    for (operator, left, lhs, right, rhs, result, authority) in cases {
        for permissions in ["time, env", "time", "env"] {
            let source = format!("pure updated(left, right) {{ var total = left; total {operator} right; total }}\npure forwarded(left, right) {{ updated(left, right) }}\nproc clock() [time] -> {left} {{ let _ = time.now(); {lhs} }}\nproc setting() [env] -> {right} {{ let _ = env.get(\"SETTING\"); {rhs} }}\nproc observed() [{permissions}] -> {result} {{ forwarded(clock(), setting()) }}\n");
            if permissions != "time, env" { assert_effect_refusal(&source, 78); continue; }
            let parsed = Parser::parse_source_arena_only(SourceId::new(78), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let (statement, _) = checked.solved.statements.iter().find(|(identity, _)| source[parsed.arena.arena.stmt(identity.statement).span.range()].trim().trim_end_matches(';') == format!("total {operator} right")).unwrap();
            let statement = *statement;
            let owner = checked.solved.statement_owners[&statement];
            assert_eq!(checked.solved.statements[&statement], StatementPosition::Statement);
            let (binding, storage) = checked.solved.bindings.iter().find(|(identity, _)| matches!(parsed.arena.arena.binding_target(identity.target).kind, crate::syntax::arena::ArenaBindingTargetKind::Name(name) if name == "total")).unwrap();
            assert_eq!(storage.owner, Some(owner));
            assert!(storage.scheme.is_none(), "each mutable slot is monomorphic throughout its source lifetime");
            let binding = *binding;
            let TypeNode::Arrow(signature) = checked.solved.graph.node(checked.solved.declarations[&owner].signature).unwrap() else { unreachable!() };
            assert_eq!(checked.solved.graph.resolved(storage.ty).unwrap(), checked.solved.graph.resolved(signature.params[0].ty).unwrap());
            assert_eq!(checked.solved.graph.resolved(storage.ty).unwrap(), checked.solved.graph.resolved(signature.result).unwrap());
            assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet::EMPTY));
            let observed = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "observed").unwrap().1;
            let TypeNode::Arrow(signature) = checked.solved.graph.node(observed.signature).unwrap() else { unreachable!() };
            assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0)));
            let requirement = checked.solved.statement_operations.get(&statement).map(|operation| operation.requirement);
            let receipts = requirement.map(|requirement| checked.solved.calls.values().flat_map(|call| &call.requirements).filter(|&&instance| checked.solved.graph.requirement_origin(instance).unwrap() == requirement)
                .filter(|&&instance| checked.solved.graph.candidate_evidence(instance).unwrap().is_some()).copied().collect::<Vec<_>>()).unwrap_or_default();
            assert_eq!(receipts.len(), usize::from(operator != "="));
            let counters = checked.solved.graph.counters().clone();
            drop(parsed);
            checked.solved.validate().unwrap();
            assert_eq!(statement.source, SourceId::new(78));
            assert_eq!(checked.solved.statement_owners[&statement], owner);
            assert!(checked.solved.bindings[&binding].scheme.is_none());
            for receipt in receipts {
                assert_eq!(checked.solved.graph.requirement_origin(receipt).unwrap(), requirement.unwrap());
                let evidence = checked.solved.graph.candidate_evidence(receipt).unwrap().unwrap();
                checked.solved.symbol_owner().with_current(|| assert_eq!(checked.solved.graph.candidate(evidence.candidate).unwrap().public_label.as_str().as_str(), format!("language.assignment.{authority}")));
                let TypeNode::Arrow(selected) = checked.solved.graph.node(checked.solved.graph.resolved(evidence.signature).unwrap()).unwrap() else { unreachable!() };
                assert_eq!(checked.solved.graph.export_type(selected.params[0].ty).unwrap(), domain_type(left));
                assert_eq!(checked.solved.graph.export_type(selected.params[1].ty).unwrap(), domain_type(right));
                assert_eq!(checked.solved.graph.export_type(selected.result).unwrap(), domain_type(result));
                assert_eq!(checked.solved.statement_operations[&statement].effects, EffectSummary::Closed(EffectSet::EMPTY));
            }
            assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
            assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        }
    }
    let source = "pure updated(left, right) { var total = left; total += right; total }\npure forwarded(left, right) { updated(left, right) }\nlet unsupported: Str = forwarded(\"a\", \"b\")\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(78), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert!(Checker::check_arena(&parsed.arena, source).diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "text replacement and ordinary concatenation do not invent a text compound-assignment domain");
}

#[test]
fn list_addition_and_compound_storage_retain_item_pull_and_cleanup_permissions() {
    for body in ["left + right", "var total = left; total += right; total"] {
        for permissions in ["time, env", "time", "env"] {
            let source = format!("stream delayed() [time, env] -> Stream[Int] {{ defer {{ let _ = env.get(\"CLOSED\") }}; let _ = time.now(); yield 1 }}\npure combined(left, right) {{ {body} }}\npure forwarded(left, right) {{ combined(left, right) }}\nproc observed() [{permissions}] -> Unit {{ let empty: List[Stream[Int]] = []; let items = forwarded([delayed()], empty); let _ = items[0].collect() }}\n");
            if permissions != "time, env" { assert_effect_refusal(&source, 79); continue; }
            let parsed = Parser::parse_source_arena_only(SourceId::new(79), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let observed = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "observed").unwrap().1;
            let TypeNode::Arrow(signature) = checked.solved.graph.node(observed.signature).unwrap() else { unreachable!() };
            assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0)));
            assert!(checked.solved.expression_producer_flows.keys().any(|identity| &source[parsed.arena.arena.expr(identity.expression).span.range()] == "items"));
            drop(parsed);
            checked.solved.validate().unwrap();
        }
    }
}

#[test]
fn membership_each_receiver_and_key_domain_retains_operand_permissions_and_cold_authority() {
    use crate::sema::operation_graph::MembershipDomain;
    let cases = [
        ("Int", "1", "List[Int]", "[1, 2]", "List"),
        ("UInt", "1", "List[UInt]", "[1, 2]", "List"),
        ("Float", "1.0", "List[Float]", "[1.0, 2.0]", "List"),
        ("Str", "\"a\"", "List[Str]", "[\"a\", \"b\"]", "List"),
        ("Duration", "1s", "List[Duration]", "[1s, 2s]", "List"),
        ("List[Int]", "[1]", "List[List[Int]]", "[[1], [2]]", "List"),
        ("Str", "\"a\"", "Str", "\"abc\"", "Str"),
        ("Bytes", "b\"a\"", "Bytes", "b\"abc\"", "Bytes"),
        ("Str", "\"answer\"", "Shape", "{answer: 7}", "Record"),
        ("Str", "\"child\"", "Path", "p\"root/child\"", "Path"),
        ("Path", "p\"child\"", "Path", "p\"root/child\"", "Path"),
        ("Str", "\"key\"", "Map[Str, Int]", "{[\"key\"]: 1}", "Map"),
        ("Int", "1", "Map[Int, Int]", "{[1]: 1}", "Map"),
        ("UInt", "1", "Map[UInt, Int]", "{[1]: 1}", "Map"),
        ("Bool", "true", "Map[Bool, Int]", "{[true]: 1}", "Map"),
        ("Bytes", "b\"key\"", "Map[Bytes, Int]", "{[b\"key\"]: 1}", "Map"),
        ("Path", "p\"key\"", "Map[Path, Int]", "{[p\"key\"]: 1}", "Map"),
        ("Duration", "1s", "Map[Duration, Int]", "{[1s]: 1}", "Map"),
    ];
    for operator in ["in", "not in"] {
        for (needle, item, receiver, container, domain) in cases {
            for permissions in ["time, env", "time", "env"] {
                let source = format!("type Shape = {{answer: Int}}\npure member(needle, container) {{ needle {operator} container }}\npure forwarded(needle, container) {{ member(needle, container) }}\nproc clock() [time] -> {needle} {{ let _ = time.now(); {item} }}\nproc setting() [env] -> {receiver} {{ let _ = env.get(\"SETTING\"); {container} }}\nproc observed() [{permissions}] -> Bool {{ forwarded(clock(), setting()) }}\n");
                if permissions != "time, env" { assert_effect_refusal(&source, 80); continue; }
                let parsed = Parser::parse_source_arena_only(SourceId::new(80), &source);
                assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
                let (identity, operation) = checked.solved.operations.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == format!("needle {operator} container")).unwrap();
                let identity = *identity;
                let requirement = operation.requirement;
                let owner = operation.caller.unwrap();
                assert_eq!(checked.solved.expression_owners[&identity], owner);
                assert!(checked.solved.declarations[&owner].source_requirements.contains(&requirement));
                assert_eq!(operation.effects, EffectSummary::Closed(EffectSet::EMPTY));
                let receipts = checked.solved.calls.values().flat_map(|call| &call.requirements).filter(|&&instance| checked.solved.graph.requirement_origin(instance).unwrap() == requirement)
                    .filter_map(|&instance| checked.solved.graph.candidate_evidence(instance).unwrap().map(|evidence| (instance, evidence.candidate))).collect::<Vec<_>>();
                assert_eq!(receipts.len(), 1);
                let observed = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "observed").unwrap().1;
                let TypeNode::Arrow(signature) = checked.solved.graph.node(observed.signature).unwrap() else { unreachable!() };
                assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0)));
                let counters = checked.solved.graph.counters().clone();
                drop(parsed);
                checked.solved.validate().unwrap();
                assert_eq!(identity.source, SourceId::new(80));
                for (receipt, candidate) in receipts {
                    assert_eq!(checked.solved.graph.requirement_origin(receipt).unwrap(), requirement);
                    let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, candidate).unwrap() else { panic!("membership retains its actual language authority") };
                    let PreparedLanguageOperation::Membership { negated, domain: selected } = metadata.operation else { panic!("the receipt authorizes membership") };
                    assert_eq!(negated, operator == "not in");
                    let selected_domain = match selected { MembershipDomain::List => "List", MembershipDomain::Map => "Map", MembershipDomain::Str => "Str", MembershipDomain::Bytes => "Bytes", MembershipDomain::Record => "Record", MembershipDomain::Path { .. } => "Path", MembershipDomain::EnvPathList => "EnvPathList" };
                    assert_eq!(selected_domain, domain);
                }
                assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
                assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
            }
        }
    }
}

#[test]
fn each_run_mode_keeps_process_and_propagation_permissions_and_argv_refusals() {
    use crate::syntax::node::RunKind;
    for (form, kind) in [
        ("run", RunKind::Plain), ("run.status", RunKind::Status),
        ("run.text", RunKind::CaptureText), ("run.bytes", RunKind::CaptureBytes),
        ("run.capture --text", RunKind::CaptureTextRecord), ("run.capture --bytes", RunKind::CaptureBytesRecord),
        ("run.stream --text", RunKind::StreamText), ("run.stream --bytes", RunKind::StreamBytes),
    ] {
        for propagate in [false, true] {
            let permissions = if propagate { "process, error" } else { "process" };
            let suffix = if propagate { " ?" } else { "" };
            let source = format!("proc observe() [{permissions}] -> Unit {{ let value = {form} true{suffix}; let _ = value }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(81), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            assert_eq!(checked.solved.run_operations.len(), 1);
            let (identity, run) = checked.solved.run_operations.iter().next().unwrap();
            assert_eq!(identity.source, SourceId::new(81));
            assert_eq!(run.kind, kind);
            assert_eq!(run.propagate, propagate);
            assert_eq!(run.usage, super::run_operation::RunUse::Value);
            assert_eq!(run.operation.effects, EffectSummary::Closed(EffectSet(EffectSet::PROCESS.0 | if propagate { EffectSet::ERROR.0 } else { 0 })));
            let evidence = checked.solved.graph.candidate_evidence(run.operation.requirement).unwrap().unwrap();
            let identity = *identity;
            let parent = run.parent.clone();
            let counters = checked.solved.graph.counters().clone();
            drop(parsed);
            checked.solved.validate().unwrap();
            assert_eq!(checked.solved.run_operations[&identity].parent, parent);
            let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap() else { panic!("run retains its fixed syntax authority") };
            assert_eq!(metadata.operation, PreparedLanguageOperation::Run { kind, policy: false, propagate });
            assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
            assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
            for denied in if propagate { vec!["process", "error", ""] } else { vec![""] } {
                let source = format!("proc observe() [{denied}] -> Unit {{ let value = {form} true{suffix}; let _ = value }}\n");
                assert_effect_refusal(&source, 81);
            }
        }
        for permissions in ["process, time, env", "process, time", "process, env", "time, env"] {
            let source = format!("proc argument() [time, env] -> Str {{ let _ = time.now(); let _ = env.get(\"SETTING\"); \"word\" }}\nproc observe() [{permissions}] -> Unit {{ let input = argument(); let output = {form} true $input; let _ = output }}\n");
            if permissions != "process, time, env" { assert_effect_refusal(&source, 81); continue; }
            let parsed = Parser::parse_source_arena_only(SourceId::new(81), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let run = checked.solved.run_operations.values().next().unwrap();
            assert_eq!(run.kind, kind);
            assert_eq!(run.operation.effects, EffectSummary::Closed(EffectSet::PROCESS));
            let observed = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "observe").unwrap().1;
            let TypeNode::Arrow(signature) = checked.solved.graph.node(observed.signature).unwrap() else { unreachable!() };
            assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet(EffectSet::PROCESS.0 | EffectSet::TIME.0 | EffectSet::ENV.0)));
            drop(parsed);
            checked.solved.validate().unwrap();
        }
        for argument in ["b\"bad\"", "1.5", "{answer: 1}", "[[1]]", "rows()"] {
            let invalid = format!("stream rows() [] -> Stream[Int] {{ yield 1 }}\nproc observe() [process] -> Unit {{ let invalid = {argument}; let output = {form} true $invalid; let _ = output }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(81), &invalid);
            assert!(parsed.diagnostics.is_empty(), "{invalid}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &invalid);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.argv-conversion")), "{invalid}: {:?}", checked.diagnostics);
        }
        let invalid = format!("proc observe() [process] -> Unit {{ let output = {form} --accept=[] true; let _ = output }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(81), &invalid);
        assert!(parsed.diagnostics.is_empty(), "{invalid}: {:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &invalid);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.accept-policy")), "{invalid}: {:?}", checked.diagnostics);
    }
}

#[test]
fn each_process_stream_mode_keeps_live_pull_and_close_authority_separate_from_creation() {
    use super::ProducerFlowKind;
    for form in ["run.stream --text", "run.stream --bytes"] {
        for policy in [false, true] {
            for permissions in ["process, error", "process", "error", ""] {
                let source = format!("let rows = {form} {} true ?\nproc consume() [{permissions}] -> Unit {{ for item in rows {{ let _ = item; break }} }}\n", if policy { "--accept=[0]" } else { "" });
                if policy && permissions != "process, error" { assert_effect_refusal(&source, 82); continue; }
                let parsed = Parser::parse_source_arena_only(SourceId::new(82), &source);
                assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
                let (identity, run) = checked.solved.run_operations.iter().next().unwrap();
                assert_eq!(identity.source, SourceId::new(82));
                assert_eq!(run.policy, policy);
                assert_eq!(run.operation.effects, EffectSummary::Closed(EffectSet(EffectSet::PROCESS.0 | EffectSet::ERROR.0)));
                let ProducerFlowKind::Known(profile) = &checked.solved.producer_flows.node(run.producer_flow).unwrap().kind else { panic!("the original process cursor owns its checked profile") };
                assert_eq!(profile.len(), 1);
                let effects = profile.values().next().unwrap();
                assert_eq!(effects.pull, EffectSummary::Closed(if policy { EffectSet(EffectSet::PROCESS.0 | EffectSet::ERROR.0) } else { EffectSet::EMPTY }));
                assert_eq!(effects.close, EffectSummary::Closed(if policy { EffectSet::PROCESS } else { EffectSet::EMPTY }));
                let identity = *identity;
                let producer = run.producer_flow;
                let requirement = run.operation.requirement;
                let evidence = checked.solved.graph.candidate_evidence(requirement).unwrap().unwrap();
                let counters = checked.solved.graph.counters().clone();
                drop(parsed);
                checked.solved.validate().unwrap();
                assert_eq!(checked.solved.run_operations[&identity].producer_flow, producer);
                let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap() else { panic!("the cursor must retain its original process authority") };
                assert!(matches!(metadata.operation, PreparedLanguageOperation::Run { policy: selected, propagate: true, .. } if selected == policy));
                assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
                assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
            }
        }
    }
}

#[test]
fn plain_context_commands_keep_statement_roles_finite_permissions_and_scalar_arguments() {
    use crate::syntax::arena::ArenaStmtKind;
    for command in ["cd p\".\" { let _ = 1 }", "env X=\"value\" COUNT=2 { let _ = 1 }", "env X=$snapshot { let _ = 1 }"] {
        let source = format!("proc change() [env, error] -> Unit {{ let snapshot = p\"child\"; {command} }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(83), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        let (identity, role) = checked.solved.statements.iter().find(|(identity, _)| matches!(parsed.arena.arena.stmt(identity.statement).kind, ArenaStmtKind::Command(_))).unwrap();
        assert_eq!(*role, StatementPosition::Statement);
        let identity = *identity;
        let owner = checked.solved.statement_owners[&identity];
        let TypeNode::Arrow(signature) = checked.solved.graph.node(checked.solved.declarations[&owner].signature).unwrap() else { unreachable!() };
        assert_eq!(checked.solved.graph.export_type(signature.result).unwrap(), Type::Unit);
        assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet(EffectSet::ENV.0 | EffectSet::ERROR.0)));
        assert!(checked.solved.graph.scheme(checked.solved.declarations[&owner].scheme).unwrap().quantifiers.is_empty());
        let counters = checked.solved.graph.counters().clone();
        drop(parsed);
        checked.solved.validate().unwrap();
        assert_eq!(identity.source, SourceId::new(83));
        assert_eq!(checked.solved.statement_owners[&identity], owner);
        assert_eq!(checked.solved.statements[&identity], StatementPosition::Statement);
        assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
        for denied in ["env", "error", ""] {
            let source = format!("proc change() [{denied}] -> Unit {{ let snapshot = p\"child\"; {command} }}\n");
            assert_effect_refusal(&source, 83);
        }
    }
    for source in [
        "proc invalid() [env, error] -> Unit { let value = 1; cd $value { let _ = 1 } }\n",
        "proc invalid() [env, error] -> Unit { let value = null; env X=$value { let _ = 1 } }\n",
        "proc invalid() [env, error] -> Unit { let value = {answer: 1}; env X=$value { let _ = 1 } }\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(83), source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        assert!(!Checker::check_arena(&parsed.arena, source).diagnostics.is_empty(), "plain context commands cannot erase their scalar input contract: {source}");
    }
}

#[test]
fn every_explicit_dynamic_binary_variant_keeps_fixed_results_and_operand_permissions() {
    for operator in ["+", "-", "*", "/", "%", "==", "!=", "<", "<=", ">", ">=", "in", "not in"] {
        let result = if matches!(operator, "+" | "-" | "*" | "/" | "%") { "Any" } else { "Bool" };
        for reverse in [false, true] {
            let arguments = if matches!(operator, "in" | "not in") { ["1, [1, 2]", "\"a\", \"abc\""] } else { ["1, 2", "3.0, 4.0"] };
            let mut arguments = arguments.to_vec(); if reverse { arguments.reverse(); }
            let source = format!("pure boundary(left: Any, right: Any) -> {result} {{ left {operator} right }}\n{}", arguments.iter().enumerate().map(|(index, arguments)| format!("let value_{index}: {result} = boundary({arguments})\n")).collect::<String>());
            let parsed = Parser::parse_source_arena_only(SourceId::new(84), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let (owner, declaration) = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "boundary").unwrap();
            let TypeNode::Arrow(signature) = checked.solved.graph.node(declaration.signature).unwrap() else { unreachable!() };
            assert!(signature.params.iter().all(|parameter| checked.solved.graph.export_type(parameter.ty).unwrap() == Type::Any));
            let expected = if result == "Any" { Type::Any } else { Type::Bool };
            assert_eq!(checked.solved.graph.export_type(signature.result).unwrap(), expected);
            assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet::EMPTY));
            assert!(checked.solved.graph.scheme(declaration.scheme).unwrap().quantifiers.is_empty());
            let (identity, _) = checked.solved.expressions.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == format!("left {operator} right")).unwrap();
            assert_eq!(checked.solved.expression_owners[identity], *owner);
            let identity = *identity;
            let owner = *owner;
            drop(parsed);
            checked.solved.validate().unwrap();
            assert_eq!(identity.source, SourceId::new(84));
            assert_eq!(checked.solved.expression_owners[&identity], owner);
            assert_eq!(checked.solved.graph.export_type(checked.solved.expressions[&identity]).unwrap(), expected);
        }
        let denied_type = format!("pure rejected(left: Any, right: Any) -> Int {{ left {operator} right }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(84), &denied_type);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(!Checker::check_arena(&parsed.arena, &denied_type).diagnostics.is_empty(), "a desired Int cannot train the explicit dynamic result: {operator}");
        for permissions in ["time, env", "time", "env"] {
            let right = if matches!(operator, "in" | "not in") { "[1, 2]" } else { "2" };
            let source = format!("proc clock() [time] -> Any {{ let _ = time.now(); 1 }}\nproc setting() [env] -> Any {{ let _ = env.get(\"SETTING\"); {right} }}\nproc observed() [{permissions}] -> {result} {{ clock() {operator} setting() }}\n");
            if permissions != "time, env" { assert_effect_refusal(&source, 84); }
            else {
                let parsed = Parser::parse_source_arena_only(SourceId::new(84), &source);
                assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
                drop(parsed);
                checked.solved.validate().unwrap();
            }
        }
    }
}

#[test]
fn each_run_mode_forwards_argv_eligibility_without_generalizing_its_fixed_syntax() {
    for form in ["run", "run.status", "run.text", "run.bytes", "run.capture --text", "run.capture --bytes", "run.stream --text", "run.stream --bytes"] {
        let definitions = format!("proc execute(value) [process] -> Unit {{ let output = {form} true $value; let _ = output }}\nproc forwarded(value) [process] -> Unit {{ execute(value) }}\n");
        for reverse in [false, true] {
            let mut arguments = ["\"word\"", "p\"word\"", "7", "unsigned", "true", "1s", "dynamic", "[p\"first\", p\"second\"]"];
            if reverse { arguments.reverse(); }
            let source = format!("{definitions}let unsigned: UInt = 1\nlet dynamic: Any = 1\n{}", arguments.iter().map(|argument| format!("let _ = forwarded({argument})\n")).collect::<String>());
            let parsed = Parser::parse_source_arena_only(SourceId::new(85), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let (identity, run) = checked.solved.run_operations.iter().next().unwrap();
            let owner = run.operation.caller.unwrap();
            assert_eq!(identity.source, SourceId::new(85));
            assert_eq!(run.arguments.len(), 1);
            let requirement = run.arguments[0].requirement;
            assert!(checked.solved.declarations[&owner].source_requirements.contains(&requirement));
            assert_eq!(run.operation.effects, EffectSummary::Closed(EffectSet::PROCESS));
            let receipts = checked.solved.calls.values().filter(|call| call.caller.is_none()).flat_map(|call| &call.requirements)
                .filter(|&&instance| checked.solved.graph.requirement_origin(instance).unwrap() == requirement).copied().collect::<Vec<_>>();
            assert_eq!(receipts.len(), arguments.len());
            let identity = *identity;
            let kind = run.kind;
            let evidence = checked.solved.graph.candidate_evidence(run.operation.requirement).unwrap().unwrap();
            let counters = checked.solved.graph.counters().clone();
            drop(parsed);
            checked.solved.validate().unwrap();
            for receipt in receipts { assert!(checked.solved.graph.eligibility_satisfied(receipt).unwrap()); }
            assert_eq!(checked.solved.run_operations[&identity].operation.caller, Some(owner));
            let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap() else { panic!("argv forwarding retains the written Run mode") };
            assert_eq!(metadata.operation, PreparedLanguageOperation::Run { kind, policy: false, propagate: false });
            assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
            assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        }
        for argument in ["b\"bad\"", "1.5", "{answer: 1}", "[[1]]", "rows()"] {
            let source = format!("stream rows() [] -> Stream[Int] {{ yield 1 }}\n{definitions}let _ = forwarded({argument})\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(85), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            assert!(!Checker::check_arena(&parsed.arena, &source).diagnostics.is_empty(), "caller arguments cannot change the fixed native argv eligibility: {source}");
        }
    }
}

#[test]
fn each_unary_domain_retains_original_authority_and_reached_operand_permissions() {
    for (expression, domain, result, authority) in [("-value", "Int", "Int", "language.unary.Neg"), ("-value", "UInt", "Int", "language.unary.Neg"), ("-value", "Float", "Float", "language.unary.Neg"), ("!value", "Bool", "Bool", "language.unary.Not"), ("!value", "Status", "Bool", "language.unary.Not")] {
        for permissions in ["time, env", "time", "env"] {
            let source = format!("pure apply(value) {{ {expression} }}\npure forwarded(value) {{ apply(value) }}\nproc clock(value: {domain}) [time] -> {domain} {{ let _ = time.now(); value }}\nproc setting(value: {domain}) [env] -> {domain} {{ let _ = env.get(\"SETTING\"); value }}\nproc observed(input: {domain}) [{permissions}] -> {result} {{ forwarded(setting(clock(input))) }}\n");
            if permissions != "time, env" { assert_effect_refusal(&source, 86); continue; }
            let parsed = Parser::parse_source_arena_only(SourceId::new(86), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let (identity, operation) = checked.solved.operations.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == expression).unwrap();
            let (identity, requirement) = (*identity, operation.requirement);
            let owner = checked.solved.expression_owners[&identity];
            assert!(checked.solved.declarations[&owner].source_requirements.contains(&requirement));
            assert_eq!(operation.effects, EffectSummary::Closed(EffectSet::EMPTY));
            let receipts = checked.solved.calls.values().flat_map(|call| &call.requirements).filter(|&&instance| checked.solved.graph.requirement_origin(instance).unwrap() == requirement).filter_map(|&instance| checked.solved.graph.candidate_evidence(instance).unwrap().map(|evidence| evidence.candidate)).collect::<Vec<_>>();
            assert_eq!(receipts.len(), 1);
            let observed = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "observed").unwrap().1;
            let TypeNode::Arrow(signature) = checked.solved.graph.node(observed.signature).unwrap() else { unreachable!() };
            assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0)));
            let counters = checked.solved.graph.counters().clone();
            drop(parsed);
            checked.solved.validate().unwrap();
            assert_eq!(identity.source, SourceId::new(86));
            assert_eq!(checked.solved.expression_owners[&identity], owner);
            let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, receipts[0]).unwrap() else { panic!("unary authority is canonical") };
            assert_eq!(metadata.authority, authority);
            let PreparedLanguageOperation::Unary { operand, .. } = metadata.operation else { panic!("unary syntax retains a unary instruction") };
            assert_eq!(format!("{operand:?}"), domain);
            assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
            assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        }
    }
}

#[test]
fn each_slice_domain_retains_original_authority_and_reached_receiver_and_bound_permissions() {
    for (domain, value, authority) in [("Str", "\"abc\"", "language.slice.Str"), ("Bytes", "b\"abc\"", "language.slice.Bytes"), ("List[Int]", "[1, 2, 3]", "language.slice.List")] {
        for permissions in ["time, env", "time", "env"] {
            let source = format!("pure portion(values, lower, upper) {{ values[lower..upper] }}\npure forwarded(values, lower, upper) {{ portion(values, lower, upper) }}\nproc clock() [time] -> {domain} {{ let _ = time.now(); {value} }}\nproc setting() [env] -> Int {{ let _ = env.get(\"SETTING\"); 1 }}\nproc observed() [{permissions}] -> {domain} {{ forwarded(clock(), setting(), setting()) }}\n");
            if permissions != "time, env" { assert_effect_refusal(&source, 87); continue; }
            let parsed = Parser::parse_source_arena_only(SourceId::new(87), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let (identity, operation) = checked.solved.operations.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == "values[lower..upper]").unwrap();
            let (identity, requirement) = (*identity, operation.requirement);
            let owner = checked.solved.expression_owners[&identity];
            assert!(checked.solved.declarations[&owner].source_requirements.contains(&requirement));
            assert_eq!(operation.effects, EffectSummary::Closed(EffectSet::EMPTY));
            assert_eq!(operation.binding.supplied_slots, [0, 1, 2]);
            let candidates = checked.solved.calls.values().flat_map(|call| &call.requirements).filter(|&&instance| checked.solved.graph.requirement_origin(instance).unwrap() == requirement).filter_map(|&instance| checked.solved.graph.candidate_evidence(instance).unwrap().map(|evidence| evidence.candidate)).collect::<Vec<_>>();
            assert_eq!(candidates.len(), 1);
            let observed = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "observed").unwrap().1;
            let TypeNode::Arrow(signature) = checked.solved.graph.node(observed.signature).unwrap() else { unreachable!() };
            assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0)));
            let counters = checked.solved.graph.counters().clone();
            drop(parsed);
            checked.solved.validate().unwrap();
            assert_eq!(identity.source, SourceId::new(87));
            assert_eq!(checked.solved.expression_owners[&identity], owner);
            let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, candidates[0]).unwrap() else { panic!("slice authority is canonical") };
            assert_eq!(metadata.authority, authority);
            assert!(matches!(metadata.operation, PreparedLanguageOperation::Slice(_)));
            assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
            assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        }
    }
}

#[test]
fn each_map_key_domain_retains_original_eligibility_and_reached_key_and_value_permissions() {
    use crate::sema::inference::{Eligibility, RequirementTemplate};
    for (domain, key) in [("Str", "\"key\""), ("Int", "1"), ("UInt", "1"), ("Bool", "true"), ("Bytes", "b\"key\""), ("Path", "p\"key\""), ("Duration", "1s")] {
        for permissions in ["time, env", "time", "env"] {
            let source = format!("pure keyed(key, value) {{ {{[key]: value}} }}\npure forwarded(key, value) {{ keyed(key, value) }}\nproc clock() [time] -> {domain} {{ let _ = time.now(); {key} }}\nproc setting() [env] -> Int {{ let _ = env.get(\"SETTING\"); 7 }}\nproc observed() [{permissions}] -> Map[{domain}, Int] {{ forwarded(clock(), setting()) }}\n");
            if permissions != "time, env" { assert_effect_refusal(&source, 88); continue; }
            let parsed = Parser::parse_source_arena_only(SourceId::new(88), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let (identity, _) = checked.solved.expressions.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == "{[key]: value}").unwrap();
            let identity = *identity;
            let owner = checked.solved.expression_owners[&identity];
            let TypeNode::Map(key_type, value_type) = checked.solved.graph.node(checked.solved.graph.resolved(checked.solved.expressions[&identity]).unwrap()).unwrap() else { unreachable!() };
            let TypeNode::Arrow(signature) = checked.solved.graph.node(checked.solved.declarations[&owner].signature).unwrap() else { unreachable!() };
            assert_eq!(checked.solved.graph.resolved(signature.params[0].ty).unwrap(), checked.solved.graph.resolved(*key_type).unwrap());
            assert_eq!(checked.solved.graph.resolved(signature.params[1].ty).unwrap(), checked.solved.graph.resolved(*value_type).unwrap());
            assert_eq!(signature.effects, EffectSummary::Closed(EffectSet::EMPTY));
            let requirements = checked.solved.declarations[&owner].source_requirements.iter().filter(|&&requirement| matches!(checked.solved.graph.requirement_template(requirement).unwrap(), RequirementTemplate::Eligibility { predicate: Eligibility::MapKey, .. })).copied().collect::<Vec<_>>();
            assert_eq!(requirements.len(), 1);
            let requirement = requirements[0];
            let reason = checked.solved.graph.reason_data(checked.solved.graph.requirement_reason(requirement).unwrap()).unwrap();
            assert_eq!(&source[reason.span.range()], "key");
            assert_eq!(reason.span.source_id, SourceId::new(88));
            let receipts = checked.solved.calls.values().flat_map(|call| &call.requirements).filter(|&&instance| checked.solved.graph.requirement_origin(instance).unwrap() == requirement).filter(|&&instance| checked.solved.graph.eligibility_satisfied(instance).unwrap()).copied().collect::<Vec<_>>();
            assert_eq!(receipts.len(), 1);
            let observed = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "observed").unwrap().1;
            let TypeNode::Arrow(signature) = checked.solved.graph.node(observed.signature).unwrap() else { unreachable!() };
            assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0)));
            let counters = checked.solved.graph.counters().clone();
            drop(parsed);
            checked.solved.validate().unwrap();
            assert_eq!(checked.solved.expression_owners[&identity], owner);
            let RequirementTemplate::Eligibility { predicate: Eligibility::MapKey, ty } = checked.solved.graph.requirement_template(receipts[0]).unwrap() else { panic!("exact source key guard is retained") };
            assert_eq!(checked.solved.graph.export_type(ty).unwrap(), domain_type(domain));
            assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
            assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        }
    }
}

#[test]
fn explicit_dynamic_projections_keep_original_source_ledgers_and_closed_cold_results() {
    for (parameters, expression, calls) in [
        ("value: Any", "value.answer", ["{answer: 1}", "{answer: \"word\"}"]),
        ("value: Any", "value?.answer", ["null", "{answer: \"word\"}"]),
        ("value: Any, key: Any", "value[key]", ["[1, 2], 0", "{answer: \"word\"}, \"answer\""]),
        ("value: Any, lower: Int, upper: Int", "value[lower..upper]", ["[1, 2], 0, 1", "\"word\", 0, 1"]),
    ] {
        for reverse in [false, true] {
            let mut calls = calls.to_vec(); if reverse { calls.reverse(); }
            let source = format!("pure projected({parameters}) -> Any {{ {expression} }}\npure forwarded({parameters}) -> Any {{ projected({}) }}\n{}", parameters.split(", ").map(|parameter| parameter.split(':').next().unwrap()).collect::<Vec<_>>().join(", "), calls.iter().enumerate().map(|(index, arguments)| format!("let value_{index}: Any = forwarded({arguments})\n")).collect::<String>());
            let parsed = Parser::parse_source_arena_only(SourceId::new(89), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let (owner, declaration) = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "projected").unwrap();
            let TypeNode::Arrow(signature) = checked.solved.graph.node(declaration.signature).unwrap() else { unreachable!() };
            assert_eq!(checked.solved.graph.export_type(signature.params[0].ty).unwrap(), Type::Any);
            assert_eq!(checked.solved.graph.export_type(signature.result).unwrap(), Type::Any);
            assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet::EMPTY));
            assert!(checked.solved.graph.scheme(declaration.scheme).unwrap().quantifiers.is_empty());
            let (identity, _) = checked.solved.expressions.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == expression).unwrap();
            let (identity, owner) = (*identity, *owner);
            assert_eq!(checked.solved.expression_owners[&identity], owner);
            let ledger = checked.solved.expressions.iter().filter(|(identity, _)| checked.solved.expression_owners.get(identity) == Some(&owner)).map(|(identity, _)| (*identity, source[parsed.arena.arena.expr(identity.expression).span.range()].to_owned())).collect::<Vec<_>>();
            assert!(ledger.iter().any(|(_, text)| text == "value"));
            for name in parameters.split(", ").skip(1).map(|parameter| parameter.split(':').next().unwrap()) { assert!(ledger.iter().any(|(_, text)| text == name), "each reached operand retains its original expression identity"); }
            assert!(!checked.solved.operations.contains_key(&identity), "an explicit dynamic boundary does not invent a sealed operation arm");
            let counters = checked.solved.graph.counters().clone();
            drop(parsed);
            checked.solved.validate().unwrap();
            let query = crate::frontend::query::SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
            assert_eq!(query.expression(identity).unwrap().to_string(), "Any");
            for (operand, _) in ledger { assert_eq!(operand.source, SourceId::new(89)); assert_eq!(checked.solved.expression_owners[&operand], owner); query.expression(operand).unwrap(); }
            assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
            assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        }
        let invalid = format!("pure rejected({parameters}) -> Int {{ {expression} }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(89), &invalid);
        assert!(parsed.diagnostics.is_empty(), "{invalid}: {:?}", parsed.diagnostics);
        let rejected = Checker::check_arena(&parsed.arena, &invalid);
        assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.dynamic-boundary")), "desired Int cannot train the fixed Any boundary: {invalid}: {:?}", rejected.diagnostics);
    }
    for source in ["pure rejected(value: Any, key: Any) -> Any { value?[key] }\n", "pure rejected(value: Any) -> Any { value?[0..1] }\n", "pure rejected(value: Any) -> Any { value[false..1] }\n"] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(89), source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        assert!(Checker::check_arena(&parsed.arena, source).diagnostics.iter().any(|diagnostic| matches!(diagnostic.code.as_deref(), Some("check.null-safe-index" | "check.type-mismatch"))), "erased input does not establish checked Optional shape or integer slice bounds");
    }
}

#[test]
fn explicit_dynamic_projection_operands_keep_each_reached_permission() {
    for (parameters, expression, arguments) in [
        ("value: Any", "value.answer", "setting(clock())"),
        ("value: Any", "value?.answer", "setting(clock())"),
        ("value: Any, key: Any", "value[key]", "clock(), key()"),
        ("value: Any, lower: Int, upper: Int", "value[lower..upper]", "clock(), bound(), bound()"),
    ] {
        for permissions in ["time, env", "time", "env"] {
            let names = parameters.split(", ").map(|parameter| parameter.split(':').next().unwrap()).collect::<Vec<_>>().join(", ");
            let source = format!("pure projected({parameters}) -> Any {{ {expression} }}\npure forwarded({parameters}) -> Any {{ projected({names}) }}\nproc clock() [time] -> Any {{ let _ = time.now(); {{answer: 1}} }}\nproc setting(value: Any) [env] -> Any {{ let _ = env.get(\"SETTING\"); value }}\nproc key() [env] -> Any {{ let _ = env.get(\"SETTING\"); \"answer\" }}\nproc bound() [env] -> Int {{ let _ = env.get(\"SETTING\"); 0 }}\nproc observed() [{permissions}] -> Any {{ forwarded({arguments}) }}\n");
            if permissions != "time, env" { assert_effect_refusal(&source, 90); continue; }
            let parsed = Parser::parse_source_arena_only(SourceId::new(90), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let observed = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "observed").unwrap().1;
            let TypeNode::Arrow(signature) = checked.solved.graph.node(observed.signature).unwrap() else { unreachable!() };
            assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0)));
            let identity = *checked.solved.expressions.keys().find(|identity| &source[parsed.arena.arena.expr(identity.expression).span.range()] == expression).unwrap();
            let counters = checked.solved.graph.counters().clone();
            drop(parsed);
            checked.solved.validate().unwrap();
            assert_eq!(crate::frontend::query::SolvedQuery::new(&checked.solved, checked.solved.symbol_owner()).expression(identity).unwrap().to_string(), "Any");
            assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
            assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        }
    }
}

#[test]
fn explicit_dynamic_get_keeps_opaque_effects_and_erased_record_get_keeps_its_checked_envelope() {
    for reverse in [false, true] {
        let mut calls = ["{answer: 1}, \"answer\"", "[\"word\"], 0"].to_vec(); if reverse { calls.reverse(); }
        let source = format!("proc getter(value: Any, key: Any) -> Any {{ value.get(key) }}\nproc forwarded(value: Any, key: Any) -> Any {{ getter(value, key) }}\n{}pure record_getter(value: Record, key: Str) -> Result[Any] {{ value.get(key) }}\n", calls.iter().enumerate().map(|(index, arguments)| format!("let value_{index}: Any = forwarded({arguments})\n")).collect::<String>());
        let parsed = Parser::parse_source_arena_only(SourceId::new(91), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        let mut retained = Vec::new();
        let mut operands = Vec::new();
        for (name, result, effects) in [("getter", Type::Any, EffectSummary::Unknown), ("forwarded", Type::Any, EffectSummary::Unknown), ("record_getter", Type::Result(Box::new(Type::Any), Box::new(Type::Error)), EffectSummary::Closed(EffectSet::EMPTY))] {
            let (owner, declaration) = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == name).unwrap();
            let TypeNode::Arrow(signature) = checked.solved.graph.node(declaration.signature).unwrap() else { unreachable!() };
            assert_eq!(checked.solved.graph.export_type(signature.result).unwrap(), result);
            assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), effects);
            assert!(checked.solved.graph.scheme(declaration.scheme).unwrap().quantifiers.is_empty());
            for (identity, _) in checked.solved.expressions.iter().filter(|(identity, _)| checked.solved.expression_owners.get(identity) == Some(owner) && &source[parsed.arena.arena.expr(identity.expression).span.range()] == "value.get(key)") { retained.push((*identity, *owner, result.clone())); }
            if name != "forwarded" {
                let ledger = checked.solved.expressions.keys().filter(|identity| checked.solved.expression_owners.get(identity) == Some(owner) && matches!(&source[parsed.arena.arena.expr(identity.expression).span.range()], "value" | "key")).map(|identity| (*identity, *owner)).collect::<Vec<_>>();
                assert_eq!(ledger.len(), 2, "receiver and key each retain their own original source expression");
                operands.extend(ledger);
            }
        }
        assert_eq!(retained.len(), 2);
        let counters = checked.solved.graph.counters().clone();
        drop(parsed);
        checked.solved.validate().unwrap();
        let query = crate::frontend::query::SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        for (identity, owner, result) in retained { assert_eq!(identity.source, SourceId::new(91)); assert_eq!(checked.solved.expression_owners[&identity], owner); assert_eq!(checked.solved.graph.export_type(checked.solved.expressions[&identity]).unwrap(), result); query.expression(identity).unwrap(); }
        for (identity, owner) in operands { assert_eq!(identity.source, SourceId::new(91)); assert_eq!(checked.solved.expression_owners[&identity], owner); query.expression(identity).unwrap(); }
        assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
        assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
    }
    for source in ["proc rejected(value: Any, key: Any) [] -> Any { value.get(key) }\n", "proc rejected(value: Any, key: Any) [time, env, error] -> Any { value.get(key) }\n", "proc rejected(value: Any, key: Any) -> Int { value.get(key) }\n", "pure rejected(value: Record, key: Str) -> Result[Int] { value.get(key) }\n"] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(91), source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let rejected = Checker::check_arena(&parsed.arena, source);
        assert!(rejected.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code.as_deref(), Some("check.effect-violation" | "check.pure-effect" | "check.type-mismatch" | "check.dynamic-boundary"))), "opaque method effects or erased payload cannot be trained by a written promise: {source}: {:?}", rejected.diagnostics);
    }
}

#[test]
fn pure_dynamic_get_cannot_hide_its_opaque_effect_contract() {
    let source = "pure rejected(value: Any, key: Any) -> Any { value.get(key) }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(92), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    let diagnostic = checked.diagnostics.iter().find(|diagnostic| matches!(diagnostic.code.as_deref(), Some("check.effect-violation" | "check.pure-effect"))).expect("a method on Any retains opaque effects and cannot establish a pure body");
    assert!(diagnostic.labels.iter().any(|label| label.span.source_id == SourceId::new(92) && &source[label.span.range()] == "value.get(key)"), "opaque effect refusal belongs to the original method call: {diagnostic:?}");
}

#[test]
fn greater_comparisons_keep_each_ordered_domain_and_reached_operand_permissions() {
    for operator in [">", ">="] {
        for (left_domain, left_value, right_domain, right_value) in [("Int", "1", "Int", "1"), ("UInt", "1", "UInt", "1"), ("Int", "1", "UInt", "1"), ("UInt", "1", "Int", "1"), ("Float", "1.0", "Float", "1.0"), ("Duration", "1s", "Duration", "1s"), ("Str", "\"word\"", "Str", "\"word\"")] {
            for permissions in ["time, env", "time", "env"] {
                let source = format!("pure compared(left, right) {{ left {operator} right }}\npure forwarded(left, right) {{ compared(left, right) }}\nproc clock() [time] -> {left_domain} {{ let _ = time.now(); {left_value} }}\nproc setting() [env] -> {right_domain} {{ let _ = env.get(\"SETTING\"); {right_value} }}\nproc observed() [{permissions}] -> Bool {{ forwarded(clock(), setting()) }}\n");
                if permissions != "time, env" { assert_effect_refusal(&source, 93); continue; }
                let parsed = Parser::parse_source_arena_only(SourceId::new(93), &source);
                assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
                let (identity, operation) = checked.solved.operations.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == format!("left {operator} right")).unwrap();
                let (identity, requirement) = (*identity, operation.requirement);
                let owner = checked.solved.expression_owners[&identity];
                assert!(checked.solved.declarations[&owner].source_requirements.contains(&requirement));
                assert_eq!(operation.effects, EffectSummary::Closed(EffectSet::EMPTY));
                let candidates = checked.solved.calls.values().flat_map(|call| &call.requirements).filter(|&&instance| checked.solved.graph.requirement_origin(instance).unwrap() == requirement).filter_map(|&instance| checked.solved.graph.candidate_evidence(instance).unwrap().map(|evidence| evidence.candidate)).collect::<Vec<_>>();
                assert_eq!(candidates.len(), 1);
                let counters = checked.solved.graph.counters().clone();
                drop(parsed);
                checked.solved.validate().unwrap();
                assert_eq!(identity.source, SourceId::new(93));
                let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, candidates[0]).unwrap() else { panic!("ordering keeps its canonical language authority") };
                assert_eq!(metadata.authority, if operator == ">" { "language.binary.Gt" } else { "language.binary.Ge" });
                let PreparedLanguageOperation::Ordering { left, right, .. } = metadata.operation else { unreachable!() };
                assert_eq!(format!("{left:?}"), left_domain); assert_eq!(format!("{right:?}"), right_domain);
                crate::frontend::query::SolvedQuery::new(&checked.solved, checked.solved.symbol_owner()).expression(identity).unwrap();
                assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
                assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
            }
        }
    }
}

#[test]
fn list_and_every_map_key_index_keep_receiver_and_key_permissions_after_frontend_disposal() {
    for (domain, container, key_domain, key, authority) in [("List[Int]", "[7]", "Int", "0", "language.index.List"), ("Map[Str, Int]", "{[\"key\"]: 7}", "Str", "\"key\"", "language.index.Map"), ("Map[Int, Int]", "{[1]: 7}", "Int", "1", "language.index.Map"), ("Map[UInt, Int]", "{[1]: 7}", "UInt", "1", "language.index.Map"), ("Map[Bool, Int]", "{[true]: 7}", "Bool", "true", "language.index.Map"), ("Map[Bytes, Int]", "{[b\"key\"]: 7}", "Bytes", "b\"key\"", "language.index.Map"), ("Map[Path, Int]", "{[p\"key\"]: 7}", "Path", "p\"key\"", "language.index.Map"), ("Map[Duration, Int]", "{[1s]: 7}", "Duration", "1s", "language.index.Map")] {
        for permissions in ["time, env", "time", "env"] {
            let source = format!("pure indexed(values, key) {{ values[key] }}\npure forwarded(values, key) {{ indexed(values, key) }}\nproc clock() [time] -> {domain} {{ let _ = time.now(); {container} }}\nproc setting() [env] -> {key_domain} {{ let _ = env.get(\"SETTING\"); {key} }}\nproc observed() [{permissions}] -> Int {{ forwarded(clock(), setting()) }}\n");
            if permissions != "time, env" { assert_effect_refusal(&source, 94); continue; }
            let parsed = Parser::parse_source_arena_only(SourceId::new(94), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let (identity, operation) = checked.solved.operations.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == "values[key]").unwrap();
            let (identity, requirement) = (*identity, operation.requirement);
            let owner = checked.solved.expression_owners[&identity];
            assert!(checked.solved.declarations[&owner].source_requirements.contains(&requirement));
            assert_eq!(operation.effects, EffectSummary::Closed(EffectSet::EMPTY));
            let candidates = checked.solved.calls.values().flat_map(|call| &call.requirements).filter(|&&instance| checked.solved.graph.requirement_origin(instance).unwrap() == requirement).filter_map(|&instance| checked.solved.graph.candidate_evidence(instance).unwrap().map(|evidence| evidence.candidate)).collect::<Vec<_>>();
            assert_eq!(candidates.len(), 1);
            let counters = checked.solved.graph.counters().clone();
            drop(parsed);
            checked.solved.validate().unwrap();
            assert_eq!(identity.source, SourceId::new(94));
            let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, candidates[0]).unwrap() else { panic!("index keeps its canonical language authority") };
            assert_eq!(metadata.authority, authority);
            assert!(matches!(metadata.operation, PreparedLanguageOperation::Index { .. }));
            assert!(matches!(crate::frontend::query::SolvedQuery::new(&checked.solved, checked.solved.symbol_owner()).expression(identity).unwrap().shape(), crate::frontend::query::NormalizedShape::Binder { .. }), "the generic definition retains its independent item result rather than a caller Int");
            assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
            assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        }
    }
}

#[test]
fn list_literals_forward_one_item_relationship_without_widening_and_keep_item_permissions() {
    for reverse in [false, true] {
        let mut calls = ["let numbers: List[Int] = forwarded(1, 2)\n", "let words: List[Str] = forwarded(\"one\", \"two\")\n", "let nested: List[List[Bool]] = forwarded([true], [false])\n"].to_vec(); if reverse { calls.reverse(); }
        let source = format!("pure paired(first, second) {{ [first, second] }}\npure forwarded(first, second) {{ paired(first, second) }}\n{}", calls.concat());
        let parsed = Parser::parse_source_arena_only(SourceId::new(95), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        let (identity, ty) = checked.solved.expressions.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == "[first, second]").unwrap();
        let (identity, ty) = (*identity, *ty);
        let owner = checked.solved.expression_owners[&identity];
        let TypeNode::Arrow(signature) = checked.solved.graph.node(checked.solved.declarations[&owner].signature).unwrap() else { unreachable!() };
        let TypeNode::List(item) = checked.solved.graph.node(checked.solved.graph.resolved(ty).unwrap()).unwrap() else { unreachable!() };
        for parameter in &signature.params { assert_eq!(checked.solved.graph.resolved(parameter.ty).unwrap(), checked.solved.graph.resolved(*item).unwrap()); }
        let TypeNode::List(result_item) = checked.solved.graph.node(checked.solved.graph.resolved(signature.result).unwrap()).unwrap() else { unreachable!() };
        assert_eq!(checked.solved.graph.resolved(*result_item).unwrap(), checked.solved.graph.resolved(*item).unwrap());
        assert_eq!(signature.effects, EffectSummary::Closed(EffectSet::EMPTY));
        let counters = checked.solved.graph.counters().clone();
        drop(parsed);
        checked.solved.validate().unwrap();
        let query = crate::frontend::query::SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        assert!(query.expression(identity).unwrap().to_string().starts_with("List["));
        assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
        assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
    }
    for arguments in ["1, \"two\"", "[1], [\"two\"]", "1, null"] {
        let source = format!("pure paired(first, second) {{ [first, second] }}\npure forwarded(first, second) {{ paired(first, second) }}\nlet denied = forwarded({arguments})\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(95), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        assert!(!Checker::check_arena(&parsed.arena, &source).diagnostics.is_empty(), "different caller item domains cannot widen a principal list item to Any: {source}");
    }
    for permissions in ["time, env", "time", "env"] {
        let source = format!("pure paired(first, second) {{ [first, second] }}\npure forwarded(first, second) {{ paired(first, second) }}\nproc clock() [time] -> Int {{ let _ = time.now(); 1 }}\nproc setting() [env] -> Int {{ let _ = env.get(\"SETTING\"); 2 }}\nproc observed() [{permissions}] -> List[Int] {{ forwarded(clock(), setting()) }}\n");
        if permissions != "time, env" { assert_effect_refusal(&source, 95); continue; }
        let parsed = Parser::parse_source_arena_only(SourceId::new(95), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        let observed = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "observed").unwrap().1;
        let TypeNode::Arrow(signature) = checked.solved.graph.node(observed.signature).unwrap() else { unreachable!() };
        assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0)));
        drop(parsed); checked.solved.validate().unwrap();
    }
    for permissions in ["time, env", "time", "env"] {
        let source = format!("stream delayed() [time, env] -> Stream[Int] {{ defer {{ let _ = env.get(\"SETTING\") }}; let _ = time.now(); yield 1 }}\npure paired(first, second) {{ [first, second] }}\npure forwarded(first, second) {{ paired(first, second) }}\nlet rows = forwarded(delayed(), delayed())\nproc consumed() [{permissions}] -> List[Int] {{ rows[0].collect() }}\n");
        if permissions != "time, env" { assert_effect_refusal(&source, 95); continue; }
        let parsed = Parser::parse_source_arena_only(SourceId::new(95), &source);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        drop(parsed); checked.solved.validate().unwrap();
    }
}

#[test]
fn assertion_refusals_keep_fixed_condition_message_and_error_permission_contracts() {
    let source = "proc asserted(flag: Bool, message: Str) [error] -> Unit { assert flag, message }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(96), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let (identity, role) = checked.solved.statements.iter().find(|(identity, _)| source[parsed.arena.arena.stmt(identity.statement).span.range()].trim() == "assert flag, message").unwrap();
    assert_eq!(*role, StatementPosition::Statement);
    let identity = *identity;
    let owner = checked.solved.statement_owners[&identity];
    let counters = checked.solved.graph.counters().clone();
    drop(parsed);
    checked.solved.validate().unwrap();
    assert_eq!(identity.source, SourceId::new(96));
    crate::frontend::query::SolvedQuery::new(&checked.solved, checked.solved.symbol_owner()).declaration(owner).unwrap();
    assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
    assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
    for (invalid, code) in [("proc denied() [error] -> Unit { assert 1, \"message\" }\n", "check.assert-condition"), ("proc denied() [error] -> Unit { assert true, 1 }\n", "check.assert-message"), ("proc denied() [] -> Unit { assert true, \"message\" }\n", "check.effect-violation")] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(96), invalid);
        assert!(parsed.diagnostics.is_empty(), "{invalid}: {:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, invalid);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some(code)), "{invalid}: {:?}", checked.diagnostics);
    }
}

#[test]
fn named_stage_descriptors_keep_callback_permissions_and_reached_receiver_permissions() {
    use super::StageCallback;
    for permissions in ["time, env", "time", "env"] {
        let source = format!("proc clocked(item: Int) [time] -> Int {{ let _ = time.now(); item }}\nproc values() [env] -> List[Int] {{ let _ = env.get(\"SETTING\"); [1] }}\nproc mapped(input: List[Int]) [time] -> List[Int] {{ input |> map(clocked) }}\nproc forwarded(input: List[Int]) [time] -> List[Int] {{ mapped(input) }}\nproc observed() [{permissions}] -> List[Int] {{ forwarded(values()) }}\n");
        if permissions != "time, env" { assert_effect_refusal(&source, 97); continue; }
        let parsed = Parser::parse_source_arena_only(SourceId::new(97), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        let (identity, stage) = checked.solved.stage_operations.iter().next().unwrap();
        assert_eq!(checked.solved.stage_operations.len(), 1);
        let identity = *identity;
        let StageCallback::Callable { expression, requirement, declaration: Some(declaration), .. } = stage.callback.as_ref().unwrap() else { panic!("named descriptor preserves its exact original declaration") };
        assert_eq!(&source[parsed.arena.arena.expr(*expression).span.range()], "clocked");
        assert_eq!(parsed.arena.arena.function_def(declaration.declaration).name, "clocked");
        let (callback, requirement, declaration) = (*expression, *requirement, *declaration);
        let stage_requirement = stage.operation.requirement;
        let evidence = checked.solved.graph.candidate_evidence(stage_requirement).unwrap().unwrap();
        let candidate = evidence.candidate;
        let counters = checked.solved.graph.counters().clone();
        drop(parsed);
        checked.solved.validate().unwrap();
        assert_eq!(identity.pipeline.source, SourceId::new(97));
        assert_eq!(checked.solved.stage_operations[&identity].operation.requirement, stage_requirement);
        let SolvedOperationAuthority::Stage(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, candidate).unwrap() else { panic!("descriptor retains stage authority") };
        assert_eq!(metadata.form.callback_kind, Some(crate::sema::inference::CallableKind::Proc));
        checked.solved.graph.requirement_template(requirement).unwrap();
        let query = crate::frontend::query::SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        query.declaration(declaration).unwrap();
        query.expression(super::ExpressionIdentity { expression: callback, ..identity.pipeline }).unwrap();
        assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
        assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
    }
}

#[test]
fn each_static_cli_descriptor_keeps_reached_argv_permissions_and_its_original_plan() {
    use super::registry_boundaries::RegistryBoundaryKind;
    use crate::modules::RuntimeOp;
    for (declarations, call, expected) in [("const schema = {count: {kind: \"Int\", default: 7}}\n", "cli.parse(argv: clock() + setting(), schema: schema)", RuntimeOp::CliParse), ("const schema = {count: {kind: \"Int\", default: 7}}\n", "cli.parse_full(argv: clock() + setting(), schema: schema)", RuntimeOp::CliParseFull), ("const schema = {count: {kind: \"Int\", default: 7}}\n", "cli.applet(argv: clock() + setting(), schema: schema)", RuntimeOp::CliApplet), ("const commands = {build: {positionals: [\"root\"], types: {root: \"Path\"}}}\n", "cli.commands(commands: commands, argv: clock() + setting())", RuntimeOp::CliCommands)] {
        for permissions in ["time, env", "time", "env"] {
            let source = format!("{declarations}proc clock() [time] -> List[Str] {{ let _ = time.now(); [] }}\nproc setting() [env] -> List[Str] {{ let _ = env.get(\"SETTING\"); [] }}\nproc observed() [{permissions}] -> Unit {{ let output = {call}; let _ = output }}\n");
            if permissions != "time, env" { assert_effect_refusal(&source, 98); continue; }
            let parsed = Parser::parse_source_arena_only(SourceId::new(98), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let (identity, boundary) = checked.solved.registry_boundaries.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == call).unwrap();
            let identity = *identity;
            let RegistryBoundaryKind::CliDescriptor { operation, plan } = &boundary.kind else { unreachable!() };
            assert_eq!(*operation, expected); assert!(plan.matches_operation(expected));
            assert!(boundary.caller.is_some());
            let requirement = boundary.requirement.unwrap();
            let counters = checked.solved.graph.counters().clone();
            drop(parsed);
            checked.solved.validate().unwrap();
            let RegistryBoundaryKind::CliDescriptor { operation, plan } = &checked.solved.registry_boundaries[&identity].kind else { unreachable!() };
            assert_eq!(*operation, expected); assert!(plan.matches_operation(expected));
            assert_eq!(checked.solved.registry_boundaries[&identity].requirement, Some(requirement));
            crate::frontend::query::SolvedQuery::new(&checked.solved, checked.solved.symbol_owner()).expression(identity).unwrap();
            assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
            assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        }
    }
}

#[test]
fn fixed_path_construction_keeps_reached_argument_permissions_and_refuses_other_domains() {
    for permissions in ["time, env", "time", "env"] {
        let source = format!("pure constructed(value: Str) -> Path {{ Path(value) }}\npure forwarded(value: Str) -> Path {{ constructed(value) }}\nproc clock() [time] -> Str {{ let _ = time.now(); \"root\" }}\nproc setting() [env] -> Str {{ let _ = env.get(\"SETTING\"); \"child\" }}\nproc observed() [{permissions}] -> Path {{ forwarded(clock() + setting()) }}\n");
        if permissions != "time, env" { assert_effect_refusal(&source, 99); continue; }
        let parsed = Parser::parse_source_arena_only(SourceId::new(99), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        let (identity, operation) = checked.solved.operations.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == "Path(value)").unwrap();
        let (identity, requirement) = (*identity, operation.requirement);
        assert_eq!(operation.effects, EffectSummary::Closed(EffectSet::EMPTY));
        let candidate = checked.solved.graph.candidate_evidence(requirement).unwrap().unwrap().candidate;
        let counters = checked.solved.graph.counters().clone();
        drop(parsed);
        checked.solved.validate().unwrap();
        let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, candidate).unwrap() else { panic!("Path constructor preserves its canonical source authority") };
        assert_eq!(metadata.authority, "language.constructor.Path");
        assert!(matches!(metadata.operation, PreparedLanguageOperation::Constructor { kind: crate::sema::operation_graph::ValueConstructor::Path, arity: 1 }));
        assert_eq!(crate::frontend::query::SolvedQuery::new(&checked.solved, checked.solved.symbol_owner()).expression(identity).unwrap().to_string(), "Path");
        assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
        assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
    }
    for value in ["1", "true", "[\"root\"]", "null"] {
        let source = format!("let denied = Path({value})\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(99), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        assert!(!Checker::check_arena(&parsed.arena, &source).diagnostics.is_empty(), "a fixed Path constructor input does not infer a wider dynamic conversion domain: {source}");
    }
}

#[test]
fn forwarded_pattern_presence_keeps_an_independent_input_inside_a_known_optional_shape() {
    for reverse in [false, true] {
        let mut calls = ["let number: Bool = forwarded(false, 7)\n", "let word: Bool = forwarded(true, \"word\")\n", "let nested: Bool = forwarded(false, [true])\n"].to_vec(); if reverse { calls.reverse(); }
        let source = format!("pure absent(flag: Bool, value) -> Bool {{ let optional = if flag {{ null }} else {{ value }}; optional is null }}\npure forwarded(flag: Bool, value) -> Bool {{ absent(flag, value) }}\n{}", calls.concat());
        let parsed = Parser::parse_source_arena_only(SourceId::new(100), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        let (owner, declaration) = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "absent").unwrap();
        assert!(!checked.solved.graph.scheme(declaration.scheme).unwrap().quantifiers.is_empty());
        let identity = *checked.solved.expressions.keys().find(|identity| &source[parsed.arena.arena.expr(identity.expression).span.range()] == "optional is null").unwrap();
        let owner = *owner;
        assert_eq!(checked.solved.expression_owners[&identity], owner);
        let counters = checked.solved.graph.counters().clone();
        drop(parsed);
        checked.solved.validate().unwrap();
        let query = crate::frontend::query::SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        assert_eq!(query.expression(identity).unwrap().to_string(), "Bool");
        query.declaration(owner).unwrap();
        assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
        assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
    }
}

#[test]
fn forwarded_optional_index_and_slice_keep_generic_items_inside_a_source_known_optional_shape() {
    for (expression, calls) in [("optional?[0]", ["let number: Int? = forwarded(false, [7])\n", "let word: Str? = forwarded(true, [\"word\"])\n"]), ("optional?[0..1]", ["let numbers: List[Int]? = forwarded(false, [7])\n", "let words: List[Str]? = forwarded(true, [\"word\"])\n"])] {
        for reverse in [false, true] {
            let mut calls = calls.to_vec(); if reverse { calls.reverse(); }
            let source = format!("pure selected(flag: Bool, values) {{ let optional = if flag {{ null }} else {{ values }}; {expression} }}\npure forwarded(flag: Bool, values) {{ selected(flag, values) }}\n{}", calls.concat());
            let parsed = Parser::parse_source_arena_only(SourceId::new(101), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let (identity, operation) = checked.solved.operations.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == expression).unwrap();
            let (identity, requirement) = (*identity, operation.requirement);
            let owner = checked.solved.expression_owners[&identity];
            assert!(!checked.solved.graph.scheme(checked.solved.declarations[&owner].scheme).unwrap().quantifiers.is_empty());
            assert!(checked.solved.declarations[&owner].source_requirements.contains(&requirement));
            let candidates = checked.solved.calls.values().filter(|call| call.caller.is_none()).flat_map(|call| &call.requirements).filter(|&&instance| checked.solved.graph.requirement_origin(instance).unwrap() == requirement).filter_map(|&instance| checked.solved.graph.candidate_evidence(instance).unwrap().map(|evidence| evidence.candidate)).collect::<Vec<_>>();
            assert_eq!(candidates.len(), 2);
            let counters = checked.solved.graph.counters().clone();
            drop(parsed);
            checked.solved.validate().unwrap();
            for candidate in candidates { let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, candidate).unwrap() else { unreachable!() }; assert_eq!(metadata.authority, if expression == "optional?[0]" { "language.index.List" } else { "language.slice.List" }); }
            assert!(matches!(crate::frontend::query::SolvedQuery::new(&checked.solved, checked.solved.symbol_owner()).expression(identity).unwrap().shape(), crate::frontend::query::NormalizedShape::Optional(_)));
            assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
            assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        }
    }
}
