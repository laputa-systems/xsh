use super::{Checker, SolvedOperationAuthority, Type};
use crate::sema::inference::{Atom, EffectSet, EffectSummary, TypeNode};
use crate::sema::operation_graph::PreparedLanguageOperation;
use crate::source::SourceId;
use crate::syntax::parser::Parser;

#[test]
fn source_boolean_consumers_keep_fixed_operand_types_and_permissions() {
    for operator in ["and", "or"] {
        for (permissions, accepted) in [("time, env", true), ("time", false), ("env", false)] {
            let source = format!("pure combined(left, right) {{ left {operator} right }}\npure forwarded(left, right) {{ combined(left, right) }}\nproc clock() [time] -> Bool {{ let _ = time.now(); true }}\nproc setting() [env] -> Bool {{ let _ = env.get(\"SETTING\"); false }}\nproc observed() [{permissions}] -> Bool {{ forwarded(clock(), setting()) }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(62), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            if accepted {
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let identities = checked.solved.expressions.keys().filter(|identity| matches!(&source[parsed.arena.arena.expr(identity.expression).span.range()], "left" | "right")
                    || source[parsed.arena.arena.expr(identity.expression).span.range()] == format!("left {operator} right")).copied().collect::<Vec<_>>();
                assert_eq!(identities.len(), 5, "both the fixed consumer and its forwarder retain their original operands");
                let declaration = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "combined").unwrap().1;
                let TypeNode::Arrow(signature) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("the source function owns its fixed Bool signature") };
                assert_eq!(signature.effects, EffectSummary::Closed(EffectSet::EMPTY));
                assert_eq!(signature.params.len(), 2);
                assert!(signature.params.iter().all(|parameter| checked.solved.graph.export_type(parameter.ty).unwrap() == Type::Bool));
                assert!(checked.solved.graph.scheme(declaration.scheme).unwrap().quantifiers.is_empty());
                drop(parsed);
                checked.solved.validate().unwrap();
                for identity in identities {
                    assert_eq!(identity.source, SourceId::new(62));
                    assert_eq!(checked.solved.graph.export_type(checked.solved.expressions[&identity]).unwrap(), Type::Bool);
                }
            } else {
                assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{permissions}: {:?}", checked.diagnostics);
            }
        }
        for arguments in ["1, true", "false, 1", "Ok(true), false"] {
            let source = format!("pure combined(left, right) {{ left {operator} right }}\npure forwarded(left, right) {{ combined(left, right) }}\nlet invalid = forwarded({arguments})\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(62), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            assert!(!Checker::check_arena(&parsed.arena, &source).diagnostics.is_empty(), "invalid fixed Bool operand accepted: {source}");
        }
    }
}

#[test]
fn source_negation_forwards_integer_float_and_unsigned_guards_without_caller_training() {
    for reverse in [false, true] {
        let calls = if reverse { "let floating: Float = forwarded(2.0)\nlet unsigned: UInt = 2\nlet signed: Int = forwarded(unsigned)\nlet integer: Int = forwarded(2)\n" } else { "let integer: Int = forwarded(2)\nlet unsigned: UInt = 2\nlet signed: Int = forwarded(unsigned)\nlet floating: Float = forwarded(2.0)\n" };
        let source = format!("pure negated(value) {{ -value }}\npure forwarded(value) {{ negated(value) }}\n{calls}");
        let parsed = Parser::parse_source_arena_only(SourceId::new(63), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.operations.len(), 1);
        let (identity, operation) = checked.solved.operations.iter().next().unwrap();
        assert_eq!(identity.source, SourceId::new(63));
        assert!(checked.solved.declarations[&operation.caller.unwrap()].source_requirements.contains(&operation.requirement));
        assert_eq!(operation.effects, EffectSummary::Closed(EffectSet::EMPTY));
        let selected = checked.solved.calls.values().filter(|call| call.caller.is_none()).flat_map(|call| &call.requirements)
            .filter(|&&requirement| checked.solved.graph.requirement_origin(requirement).unwrap() == operation.requirement)
            .map(|&requirement| checked.solved.graph.candidate_evidence(requirement).unwrap().unwrap().candidate).collect::<Vec<_>>();
        assert_eq!(selected.len(), 3);
        drop(parsed);
        checked.solved.validate().unwrap();
        let domains = selected.into_iter().map(|candidate| {
            let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, candidate).unwrap() else { panic!("negation cannot select a foreign authority") };
            assert_eq!(metadata.authority, "language.unary.Neg");
            let PreparedLanguageOperation::Unary { operand, .. } = metadata.operation else { panic!("negation retains its actual unary instruction") };
            format!("{operand:?}")
        }).collect::<std::collections::BTreeSet<_>>();
        assert_eq!(domains, [Atom::Int, Atom::UInt, Atom::Float].into_iter().map(|domain| format!("{domain:?}")).collect());
    }
    for value in ["true", "\"word\"", "null", "[1]"] {
        let source = format!("pure negated(value) {{ -value }}\npure forwarded(value) {{ negated(value) }}\nlet invalid = forwarded({value})\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(63), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(!Checker::check_arena(&parsed.arena, &source).diagnostics.is_empty(), "unsupported negation accepted: {value}");
    }
}

#[test]
fn source_fallback_forwards_optional_and_result_relationships_with_exact_authority() {
    for reverse in [false, true] {
        let calls = [
            "let absent: Int? = null\nlet first: Int = forwarded(absent, 1)\n",
            "let present: Str? = \"word\"\nlet second: Str = forwarded(present, \"fallback\")\n",
            "let success: Result[Int, Str] = Ok(2)\nlet third: Int = forwarded(success, 3)\n",
            "let failure: Result[Str, Int] = Err(7)\nlet fourth: Str = forwarded(failure, \"fallback\")\n",
        ];
        let mut calls = calls.to_vec();
        if reverse { calls.reverse(); }
        let source = format!("pure chosen(value, fallback) {{ value ?? fallback }}\npure forwarded(value, fallback) {{ chosen(value, fallback) }}\n{}", calls.concat());
        let parsed = Parser::parse_source_arena_only(SourceId::new(64), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let (identity, operation) = checked.solved.operations.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == "value ?? fallback").unwrap();
        assert_eq!(identity.source, SourceId::new(64));
        assert!(checked.solved.declarations[&operation.caller.unwrap()].source_requirements.contains(&operation.requirement));
        assert_eq!(operation.effects, EffectSummary::Closed(EffectSet::EMPTY));
        let candidates = checked.solved.calls.values().filter(|call| call.caller.is_none()).flat_map(|call| &call.requirements)
            .filter(|&&requirement| checked.solved.graph.requirement_origin(requirement).unwrap() == operation.requirement)
            .map(|&requirement| checked.solved.graph.candidate_evidence(requirement).unwrap().unwrap().candidate).collect::<Vec<_>>();
        assert_eq!(candidates.len(), 4);
        drop(parsed);
        checked.solved.validate().unwrap();
        let authorities = candidates.into_iter().map(|candidate| {
            let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, candidate).unwrap() else { panic!("fallback cannot select a foreign authority") };
            assert!(matches!(metadata.operation, PreparedLanguageOperation::Fallback { .. }));
            metadata.authority
        }).collect::<std::collections::BTreeSet<_>>();
        assert_eq!(authorities, ["language.binary.ResultFallback.Optional", "language.binary.ResultFallback.Result"].into());
    }
    for source in [
        "pure chosen(value, fallback) { value ?? fallback }\npure forwarded(value, fallback) { chosen(value, fallback) }\nlet invalid = forwarded(1, 2)\n",
        "pure chosen(value, fallback) { value ?? fallback }\npure forwarded(value, fallback) { chosen(value, fallback) }\nlet value: Int? = null\nlet invalid = forwarded(value, \"wrong\")\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(64), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(!Checker::check_arena(&parsed.arena, source).diagnostics.is_empty(), "invalid fallback accepted: {source}");
    }
}

#[test]
fn source_slices_forward_each_container_and_keep_latent_item_permissions() {
    let definitions = "pure portion(values) { values[1..3] }\npure forwarded(values) { portion(values) }\n";
    for reverse in [false, true] {
        let mut calls = ["let numbers: List[Int] = forwarded([1, 2, 3])\n", "let text: Str = forwarded(\"word\")\n", "let binary: Bytes = forwarded(b\"word\")\n"].to_vec();
        if reverse { calls.reverse(); }
        let source = format!("{definitions}{}", calls.concat());
        let parsed = Parser::parse_source_arena_only(SourceId::new(65), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.operations.len(), 1);
        let operation = checked.solved.operations.values().next().unwrap();
        assert!(checked.solved.declarations[&operation.caller.unwrap()].source_requirements.contains(&operation.requirement));
        assert_eq!(operation.binding.supplied_slots, [0, 1, 2]);
        assert!(operation.binding.default_slots.is_empty());
        let candidates = checked.solved.calls.values().filter(|call| call.caller.is_none()).flat_map(|call| &call.requirements)
            .filter(|&&requirement| checked.solved.graph.requirement_origin(requirement).unwrap() == operation.requirement)
            .map(|&requirement| checked.solved.graph.candidate_evidence(requirement).unwrap().unwrap().candidate).collect::<Vec<_>>();
        assert_eq!(candidates.len(), 3);
        drop(parsed);
        checked.solved.validate().unwrap();
        let authorities = candidates.into_iter().map(|candidate| {
            let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, candidate).unwrap() else { panic!("slice cannot select a foreign authority") };
            assert!(matches!(metadata.operation, PreparedLanguageOperation::Slice(_)));
            metadata.authority
        }).collect::<std::collections::BTreeSet<_>>();
        assert_eq!(authorities, ["language.slice.List", "language.slice.Str", "language.slice.Bytes"].into());
    }
    for (permissions, accepted) in [("time, env, error", true), ("time, error", false), ("env, error", false)] {
        let source = format!("stream delayed() [time, env] -> Stream[Int] {{ defer {{ let _ = env.get(\"SETTING\") }}; let _ = time.now(); yield 1 }}\n{definitions}let selected = forwarded([delayed(), delayed()])\nproc consumed() [{permissions}] -> List[Int] {{ selected[0].collect() }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(65), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        if accepted { assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics); drop(parsed); checked.solved.validate().unwrap(); }
        else { assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{permissions}: {:?}", checked.diagnostics); }
    }
    for source in ["pure portion(values) { values[1..3] }\nlet invalid = portion(1)\n", "pure portion(values) { values[false..3] }\nlet invalid = portion([1, 2])\n"] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(65), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(!Checker::check_arena(&parsed.arena, source).diagnostics.is_empty(), "invalid slice accepted: {source}");
    }
}

#[test]
fn source_environment_path_membership_charges_only_the_reached_lookup_permission() {
    for (operator, authority) in [("in", "language.binary.In.EnvPathList"), ("not in", "language.binary.NotIn.EnvPathList")] {
        let definitions = format!("pure member(needle, container) {{ needle {operator} container }}\npure forwarded(needle, container) {{ member(needle, container) }}\nproc contains(paths: EnvPathList) [] -> Bool {{ forwarded(Path(\"root\"), paths) }}\n");
        for (permissions, accepted) in [("env", true), ("", false)] {
            let source = format!("{definitions}proc observed() [{permissions}] -> Bool {{ contains(env.PATH) }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(66), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            if accepted {
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let operation = checked.solved.operations.values().next().unwrap();
                assert_eq!(operation.effects, EffectSummary::Closed(EffectSet::EMPTY));
                let selected = checked.solved.calls.values().flat_map(|call| &call.requirements)
                    .filter(|&&requirement| checked.solved.graph.requirement_origin(requirement).unwrap() == operation.requirement)
                    .filter_map(|&requirement| checked.solved.graph.candidate_evidence(requirement).unwrap().map(|evidence| evidence.candidate)).collect::<Vec<_>>();
                assert!(!selected.is_empty(), "the typed path-list consumer closes the generic source requirement");
                let contains = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "contains").unwrap().1;
                let TypeNode::Arrow(signature) = checked.solved.graph.node(contains.signature).unwrap() else { panic!("the existing path-list value has a fixed consumer signature") };
                assert_eq!(signature.effects, EffectSummary::Closed(EffectSet::EMPTY));
                drop(parsed);
                checked.solved.validate().unwrap();
                for candidate in selected {
                    let SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, candidate).unwrap() else { panic!("path-list membership retains its language authority") };
                    assert_eq!(metadata.authority, authority);
                    assert!(matches!(metadata.operation, PreparedLanguageOperation::Membership { .. }));
                }
            } else {
                assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation") && diagnostic.message.contains("env")), "{operator} [{permissions}] source {source}: {:?}", checked.diagnostics);
            }
        }
    }
}

#[test]
fn source_erased_callable_calls_keep_fixed_result_envelopes_and_opaque_effects() {
    let source = "proc erased_pure(label: Pure) { label.call(1) }\nproc erased_proc(action: Proc) { action.call(1) }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(67), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let expected = [
        ("erased_pure", "label", "label.call(1)", Type::Pure, Type::Any, EffectSummary::Closed(EffectSet::EMPTY)),
        ("erased_proc", "action", "action.call(1)", Type::Proc, Type::Result(Box::new(Type::Any), Box::new(Type::Error)), EffectSummary::Unknown),
    ];
    let mut retained = Vec::new();
    for (name, receiver_source, call_source, receiver_type, result_type, effects) in expected {
        let (owner, declaration) = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == name).unwrap();
        let TypeNode::Arrow(signature) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("the explicit erased kind owns a fixed callable signature") };
        assert_eq!(checked.solved.graph.export_type(signature.params[0].ty).unwrap(), receiver_type);
        assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), effects);
        assert!(checked.solved.graph.scheme(declaration.scheme).unwrap().quantifiers.is_empty());
        for (text, ty) in [(receiver_source, receiver_type), (call_source, result_type)] {
            let (identity, _) = checked.solved.expressions.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == text).unwrap();
            assert_eq!(identity.source, owner.source);
            assert_eq!(checked.solved.expression_owners[identity], *owner);
            retained.push((*identity, ty));
        }
    }
    drop(parsed);
    checked.solved.validate().unwrap();
    for (identity, expected) in retained {
        assert_eq!(identity.source, SourceId::new(67));
        assert_eq!(checked.solved.graph.export_type(checked.solved.expressions[&identity]).unwrap(), expected);
    }
    for source in [
        "proc restricted(action: Proc) [] { action.call(1) }\n",
        "pure restricted(action: Proc) { action.call(1) }\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(67), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code.as_deref(), Some("check.effect-violation" | "check.pure-effect"))), "the erased invocation cannot acquire a finite or pure contract: {:?}", checked.diagnostics);
    }

    for source in ["proc allowed(label: Pure) [] -> Any { label.call(1) }\n", "pure allowed(label: Pure) -> Any { label.call(1) }\n"] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(67), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "the erased Pure kind guarantees EMPTY invocation effects: {:?}", checked.diagnostics);
        drop(parsed);
        checked.solved.validate().unwrap();
    }
}

#[test]
fn source_direct_assignment_keeps_a_monomorphic_slot_and_definition_owned_value_relationship() {
    use super::StatementPosition;
    use crate::syntax::arena::ArenaBindingTargetKind;

    let definitions = "pure replaced(initial, replacement) { var stored = initial; stored = replacement; stored }\npure forwarded(initial, replacement) { replaced(initial, replacement) }\n";
    for reverse in [false, true] {
        let mut calls = ["let integer: Int = forwarded(1, 2)\n", "let floating: Float = forwarded(1.0, 2.0)\n", "let text: Str = forwarded(\"before\", \"after\")\n", "let items: List[Bool] = forwarded([true], [false])\n"].to_vec();
        if reverse { calls.reverse(); }
        let source = format!("{definitions}{}", calls.concat());
        let parsed = Parser::parse_source_arena_only(SourceId::new(68), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let (owner, declaration) = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "replaced").unwrap();
        let (target, binding) = checked.solved.bindings.iter().find(|(identity, _)| matches!(parsed.arena.arena.binding_target(identity.target).kind, ArenaBindingTargetKind::Name(name) if name == "stored")).unwrap();
        assert_eq!(binding.owner, Some(*owner));
        assert!(binding.mutable);
        assert!(binding.scheme.is_none(), "each mutable slot has one lifetime contract");
        let TypeNode::Arrow(signature) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("the source declaration owns its value relationship") };
        assert_eq!(signature.effects, EffectSummary::Closed(EffectSet::EMPTY));
        let slot = checked.solved.graph.resolved(binding.ty).unwrap();
        assert!(signature.params.iter().all(|parameter| checked.solved.graph.resolved(parameter.ty).unwrap() == slot));
        assert_eq!(checked.solved.graph.resolved(signature.result).unwrap(), slot);
        assert!(matches!(checked.solved.graph.node(slot).unwrap(), TypeNode::Rigid { .. }));
        assert_eq!(checked.solved.graph.scheme(declaration.scheme).unwrap().quantifiers.len(), 1);
        let (assignment, role) = checked.solved.statements.iter().find(|(identity, _)| &source[parsed.arena.arena.stmt(identity.statement).span.range()] == "stored = replacement;").unwrap();
        assert_eq!(*role, StatementPosition::Statement);
        assert_eq!(assignment.source, SourceId::new(68));
        assert_eq!(checked.solved.statement_owners[assignment], *owner);
        let (target, assignment, owner, slot) = (*target, *assignment, *owner, slot);
        let counters = checked.solved.graph.counters().clone();
        drop(parsed);
        checked.solved.validate().unwrap();
        assert_eq!(checked.solved.bindings[&target].owner, Some(owner));
        assert_eq!(checked.solved.graph.resolved(checked.solved.bindings[&target].ty).unwrap(), slot);
        assert_eq!(checked.solved.statement_owners[&assignment], owner);
        assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        assert_eq!(checked.solved.graph.counters().unifications, counters.unifications);
        assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
    }
    for calls in [
        "let invalid = forwarded(1, 2.0)\n",
        "let initial: List[Int?] = [1]\nlet replacement: List[Int] = [2]\nlet invalid = forwarded(initial, replacement)\n",
        "let initial: Map[Str, Int?] = {[\"key\"]: 1}\nlet replacement: Map[Str, Int] = {[\"key\"]: 2}\nlet invalid = forwarded(initial, replacement)\n",
    ] {
        let source = format!("{definitions}{calls}");
        let parsed = Parser::parse_source_arena_only(SourceId::new(68), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(!Checker::check_arena(&parsed.arena, &source).diagnostics.is_empty(), "a stored lifetime cannot widen between callers or container domains: {source}");
    }
}

#[test]
fn source_pattern_predicates_keep_fixed_result_and_dynamic_input_contracts_after_frontend_disposal() {
    let source = "pure selected(value: Result[Int, Str]) -> Bool { value is Ok(7) }\npure forwarded(value: Result[Int, Str]) -> Bool { selected(value) }\npure dynamic(value: Any) -> Bool { value is Str }\nlet present: Bool = forwarded(Ok(7))\nlet absent: Bool = forwarded(Err(\"missing\"))\nlet word: Bool = dynamic(\"word\")\nlet number: Bool = dynamic(1)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(69), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let mut identities = Vec::new();
    for expression in ["value is Ok(7)", "value is Str"] {
        let (identity, _) = checked.solved.expressions.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == expression).unwrap();
        let owner = checked.solved.expression_owners[identity];
        let declaration = &checked.solved.declarations[&owner];
        let TypeNode::Arrow(signature) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("the predicate owns its fixed source signature") };
        assert_eq!(signature.effects, EffectSummary::Closed(EffectSet::EMPTY));
        assert_eq!(checked.solved.graph.export_type(signature.result).unwrap(), Type::Bool);
        assert!(checked.solved.graph.scheme(declaration.scheme).unwrap().quantifiers.is_empty());
        assert_eq!(checked.solved.graph.export_type(signature.params[0].ty).unwrap(), if expression == "value is Str" { Type::Any } else { Type::Result(Box::new(Type::Int), Box::new(Type::Str)) });
        identities.push((*identity, owner));
    }
    drop(parsed);
    checked.solved.validate().unwrap();
    for (identity, owner) in identities {
        assert_eq!(identity.source, SourceId::new(69));
        assert_eq!(checked.solved.expression_owners[&identity], owner);
        assert_eq!(checked.solved.graph.export_type(checked.solved.expressions[&identity]).unwrap(), Type::Bool);
    }
    for source in ["pure invalid(value: Result[Int, Str]) { value is Ok(payload) }\n", "pure invalid(value: Int) { value is Int }\n"] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(69), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(!Checker::check_arena(&parsed.arena, source).diagnostics.is_empty(), "predicate syntax cannot bind a payload or invent dynamic erasure: {source}");
    }
    for (permissions, accepted) in [("time", true), ("", false)] {
        let source = format!("proc subject() [time] -> Result[Int, Str] {{ let _ = time.now(); Ok(7) }}\nproc observed() [{permissions}] -> Bool {{ subject() is Ok(7) }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(69), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        if accepted { assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics); }
        else { assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics); }
    }
}

#[test]
fn source_optional_postfix_retains_original_lifted_contracts_and_reached_bound_permissions() {
    let source = "type Row = {value: Int}\npure indexed(values: List[Int]?) -> Int? { values?[0] }\npure sliced(values: List[Int]?) -> List[Int]? { values?[0..1] }\npure projected(record: Row?) -> Int? { record?.value }\npure forwarded(values: List[Int]?) -> Int? { indexed(values) }\nlet absent: List[Int]? = null\nlet present: List[Int]? = [1, 2]\nlet first: Int? = forwarded(absent)\nlet second: Int? = forwarded(present)\nlet missing: List[Int]? = sliced(absent)\nlet subset: List[Int]? = sliced(present)\nlet row: Row? = Row(value: 7)\nlet field: Int? = projected(row)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(70), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let mut retained = Vec::new();
    for (expression, expected) in [("values?[0]", Type::Optional(Box::new(Type::Int))), ("values?[0..1]", Type::Optional(Box::new(Type::List(Box::new(Type::Int))))), ("record?.value", Type::Optional(Box::new(Type::Int)))] {
        let (identity, _) = checked.solved.expressions.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == expression).unwrap();
        let owner = checked.solved.expression_owners[identity];
        let declaration = &checked.solved.declarations[&owner];
        let TypeNode::Arrow(signature) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("the optional consumer owns its original lifted signature") };
        assert_eq!(signature.effects, EffectSummary::Closed(EffectSet::EMPTY));
        assert_eq!(checked.solved.graph.export_type(signature.result).unwrap(), expected);
        assert!(matches!(checked.solved.graph.node(checked.solved.graph.resolved(signature.params[0].ty).unwrap()).unwrap(), TypeNode::Optional(_)));
        retained.push((*identity, owner, expected));
    }
    drop(parsed);
    checked.solved.validate().unwrap();
    for (identity, owner, expected) in retained {
        assert_eq!(identity.source, SourceId::new(70));
        assert_eq!(checked.solved.expression_owners[&identity], owner);
        assert_eq!(checked.solved.graph.export_type(checked.solved.expressions[&identity]).unwrap(), expected);
    }
    for source in ["pure invalid(value: Int?) { value?[0] }\n", "pure invalid(values: List[Int]?) { values?[false] }\n", "type Row = {value: Int}\npure invalid(value: Row?) { value?.missing }\n"] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(70), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(!Checker::check_arena(&parsed.arena, source).diagnostics.is_empty(), "optional lifting cannot manufacture an index or field domain: {source}");
    }
    for (permissions, accepted) in [("time", true), ("", false)] {
        let source = format!("proc bound() [time] -> Int {{ let _ = time.now(); 0 }}\nproc observed(values: List[Int]?) [{permissions}] -> Int? {{ values?[bound()] }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(70), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        if accepted { assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics); }
        else { assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics); }
    }
}

#[test]
fn source_record_literals_keep_independent_field_relationships_and_producer_paths() {
    let definitions = "pure paired(first, second) { {first: first, second: second} }\npure forwarded(first, second) { paired(first, second) }\n";
    for reverse in [false, true] {
        let mut calls = ["let number_word = forwarded(1, \"word\")\nlet number: Int = number_word.first\nlet word: Str = number_word.second\n", "let flag_items = forwarded(true, [1, 2])\nlet flag: Bool = flag_items.first\nlet items: List[Int] = flag_items.second\n"].to_vec();
        if reverse { calls.reverse(); }
        let source = format!("{definitions}{}", calls.concat());
        let parsed = Parser::parse_source_arena_only(SourceId::new(71), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let (owner, declaration) = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "paired").unwrap();
        let TypeNode::Arrow(signature) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("the source record owns its independent field relationship") };
        assert_eq!(signature.effects, EffectSummary::Closed(EffectSet::EMPTY));
        assert_eq!(checked.solved.graph.scheme(declaration.scheme).unwrap().quantifiers.len(), 2);
        let TypeNode::Record(row) = checked.solved.graph.node(checked.solved.graph.resolved(signature.result).unwrap()).unwrap() else { panic!("the result is the original closed record shape") };
        let fields = checked.solved.graph.row_data(*row).unwrap();
        assert!(fields.tail.is_none());
        assert_eq!(fields.fields.len(), 2);
        for (name, parameter) in [("first", &signature.params[0]), ("second", &signature.params[1])] {
            let field = fields.fields.iter().find(|field| field.label == name).unwrap();
            assert_eq!(checked.solved.graph.resolved(field.ty).unwrap(), checked.solved.graph.resolved(parameter.ty).unwrap());
        }
        assert_ne!(checked.solved.graph.resolved(signature.params[0].ty).unwrap(), checked.solved.graph.resolved(signature.params[1].ty).unwrap());
        let (identity, result) = checked.solved.expressions.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == "{first: first, second: second}").unwrap();
        assert_eq!(checked.solved.expression_owners[identity], *owner);
        let TypeNode::Record(expression_row) = checked.solved.graph.node(checked.solved.graph.resolved(*result).unwrap()).unwrap() else { panic!("the original expression retains its record shape") };
        let expression_fields = checked.solved.graph.row_data(*expression_row).unwrap();
        assert!(expression_fields.tail.is_none());
        assert_eq!(expression_fields.fields.len(), fields.fields.len());
        for field in &fields.fields {
            let expression_field = expression_fields.fields.iter().find(|candidate| candidate.label == field.label).unwrap();
            assert_eq!(checked.solved.graph.resolved(expression_field.ty).unwrap(), checked.solved.graph.resolved(field.ty).unwrap());
        }
        let (identity, owner) = (*identity, *owner);
        drop(parsed);
        checked.solved.validate().unwrap();
        assert_eq!(identity.source, SourceId::new(71));
        assert_eq!(checked.solved.expression_owners[&identity], owner);
    }
    let source = format!("{definitions}let invalid: Int = forwarded(\"word\", 1).first\n");
    let parsed = Parser::parse_source_arena_only(SourceId::new(71), &source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert!(!Checker::check_arena(&parsed.arena, &source).diagnostics.is_empty(), "the desired field cannot train the source record relation");
    for (permissions, accepted) in [("time, error", true), ("env, error", false)] {
        let source = format!("stream clocked() [time] -> Stream[Int] {{ let _ = time.now(); yield 1 }}\nstream configured() [env] -> Stream[Int] {{ let _ = env.get(\"SETTING\"); yield 2 }}\n{definitions}let fields = forwarded(clocked(), configured())\nproc observed() [{permissions}] -> List[Int] {{ fields.first.collect() }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(71), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        if accepted { assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics); drop(parsed); checked.solved.validate().unwrap(); }
        else { assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation") && diagnostic.message.contains("time")), "{:?}", checked.diagnostics); }
    }
}

#[test]
fn source_direct_yield_retains_original_eligibility_and_cold_caller_receipts() {
    use super::StatementPosition;
    use crate::sema::inference::{Eligibility, RequirementTemplate};

    let definitions = "stream repeated(item) [] { yield item }\nstream forwarded(item) [] { yield @repeated(item) }\n";
    for reverse in [false, true] {
        let mut calls = ["let integers: Stream[Int] = forwarded(7)\n", "let words: Stream[Str] = forwarded(\"word\")\n", "let nested_data = forwarded({values: [1, 2]})\n"].to_vec();
        if reverse { calls.reverse(); }
        let source = format!("{definitions}{}", calls.concat());
        let parsed = Parser::parse_source_arena_only(SourceId::new(72), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let (owner, declaration) = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "repeated").unwrap();
        let (statement, role) = checked.solved.statements.iter().find(|(identity, _)| &source[parsed.arena.arena.stmt(identity.statement).span.range()] == "yield item").unwrap();
        assert_eq!(*role, StatementPosition::Statement);
        assert_eq!(checked.solved.statement_owners[statement], *owner);
        let requirements = declaration.source_requirements.iter().filter_map(|&requirement| match checked.solved.graph.requirement_template(requirement).unwrap() {
            RequirementTemplate::Eligibility { predicate: Eligibility::YieldItem, ty } => Some((requirement, ty)),
            _ => None,
        }).collect::<Vec<_>>();
        assert_eq!(requirements.len(), 1);
        let (requirement, item) = requirements[0];
        let reason = checked.solved.graph.reason_data(checked.solved.graph.requirement_reason(requirement).unwrap()).unwrap();
        assert_eq!(&source[reason.span.range()], "item");
        assert_eq!(reason.span.source_id, SourceId::new(72));
        let TypeNode::Arrow(signature) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("the source yield owns its item relationship") };
        let TypeNode::Stream(result_item) = checked.solved.graph.node(checked.solved.graph.resolved(signature.result).unwrap()).unwrap() else { panic!("direct yield returns a stream with the checked item") };
        assert_eq!(checked.solved.graph.resolved(item).unwrap(), checked.solved.graph.resolved(signature.params[0].ty).unwrap());
        assert_eq!(checked.solved.graph.resolved(item).unwrap(), checked.solved.graph.resolved(*result_item).unwrap());
        let receipts = checked.solved.calls.values().filter(|call| call.caller.is_none()).flat_map(|call| &call.requirements)
            .filter(|&&instance| checked.solved.graph.requirement_origin(instance).unwrap() == requirement).copied().collect::<Vec<_>>();
        assert_eq!(receipts.len(), 3);
        let (statement, owner) = (*statement, *owner);
        let counters = checked.solved.graph.counters().clone();
        drop(parsed);
        checked.solved.validate().unwrap();
        assert_eq!(statement.source, SourceId::new(72));
        assert_eq!(checked.solved.statement_owners[&statement], owner);
        for receipt in receipts {
            assert!(checked.solved.graph.eligibility_satisfied(receipt).unwrap());
            assert!(matches!(checked.solved.graph.requirement_template(receipt).unwrap(), RequirementTemplate::Eligibility { predicate: Eligibility::YieldItem, .. }));
        }
        assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
        assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
    }
    let source = format!("stream live() [] -> Stream[Int] {{ yield 1 }}\n{definitions}let invalid = forwarded(live())\n");
    let parsed = Parser::parse_source_arena_only(SourceId::new(72), &source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert!(Checker::check_arena(&parsed.arena, &source).diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.yield-stream")), "a live stream must be delegated rather than yielded as an item");
}

#[test]
fn source_explicit_dynamic_boundaries_keep_closed_any_contracts_without_desired_type_training() {
    for (parameters, expression, result, calls, invalid) in [
        ("left: Any, right: Any", "left + right", Type::Any, ["1, 2", "\"a\", \"b\""], "pure rejected(left: Any, right: Any) -> Int { left + right }\n"),
        ("needle: Any, container: Any", "needle in container", Type::Bool, ["1, [1, 2]", "\"a\", \"abc\""], "pure rejected(needle: Any, container: Any) -> Int { needle in container }\n"),
        ("value: Any", "value.answer", Type::Any, ["{answer: 1}", "{answer: \"word\"}"], "pure rejected(value: Any) -> Int { value.answer }\n"),
    ] {
        for reverse in [false, true] {
            let mut calls = calls.to_vec(); if reverse { calls.reverse(); }
            let result_name = if result == Type::Bool { "Bool" } else { "Any" };
            let source = format!("pure boundary({parameters}) -> {result_name} {{ {expression} }}\n{}", calls.iter().enumerate().map(|(index, arguments)| format!("let value_{index}: {result_name} = boundary({arguments})\n")).collect::<String>());
            let parsed = Parser::parse_source_arena_only(SourceId::new(73), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let (owner, declaration) = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "boundary").unwrap();
            let TypeNode::Arrow(signature) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("the explicit erased boundary owns its fixed signature") };
            assert!(signature.params.iter().all(|parameter| checked.solved.graph.export_type(parameter.ty).unwrap() == Type::Any));
            assert_eq!(checked.solved.graph.export_type(signature.result).unwrap(), result);
            assert_eq!(signature.effects, EffectSummary::Closed(EffectSet::EMPTY));
            assert!(checked.solved.graph.scheme(declaration.scheme).unwrap().quantifiers.is_empty());
            let (identity, _) = checked.solved.expressions.iter().find(|(identity, _)| &source[parsed.arena.arena.expr(identity.expression).span.range()] == expression).unwrap();
            assert_eq!(checked.solved.expression_owners[identity], *owner);
            let (identity, owner) = (*identity, *owner);
            drop(parsed);
            checked.solved.validate().unwrap();
            assert_eq!(identity.source, SourceId::new(73));
            assert_eq!(checked.solved.expression_owners[&identity], owner);
            assert_eq!(checked.solved.graph.export_type(checked.solved.expressions[&identity]).unwrap(), result);
        }
        let parsed = Parser::parse_source_arena_only(SourceId::new(73), invalid);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(!Checker::check_arena(&parsed.arena, invalid).diagnostics.is_empty(), "a desired Int cannot refine this explicit boundary: {invalid}");
    }
    for (permissions, accepted) in [("time", true), ("", false)] {
        let source = format!("proc operand() [time] -> Any {{ let _ = time.now(); 1 }}\nproc observed() [{permissions}] -> Any {{ operand() + operand() }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(73), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        if accepted { assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics); }
        else { assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics); }
    }
}

#[test]
fn source_context_scopes_keep_fixed_input_result_and_permission_contracts_after_frontend_disposal() {
    use crate::syntax::arena::{ArenaExprKind, ArenaStmtKind};
    for expression in ["cd (p\".\") { 7 }", "cd (\".\") { 7 }", "env ({X: \"value\", COUNT: 2}) { 7 }"] {
        let source = format!("proc scoped() [env] -> Result[Int] {{ {expression} }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(74), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{expression}: {:?}", checked.diagnostics);
        let (owner, declaration) = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "scoped").unwrap();
        let TypeNode::Arrow(signature) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("the context scope owns its fixed signature") };
        assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet::ENV));
        assert!(checked.solved.graph.scheme(declaration.scheme).unwrap().quantifiers.is_empty());
        let (identity, ty) = checked.solved.expressions.iter().find(|(identity, _)| matches!(parsed.arena.arena.expr(identity.expression).kind, ArenaExprKind::ContextScope { .. })).unwrap();
        assert_eq!(checked.solved.expression_owners[identity], *owner);
        let ArenaExprKind::ContextScope { input, block, .. } = parsed.arena.arena.expr(identity.expression).kind else { unreachable!() };
        let input_identity = super::ExpressionIdentity { expression: input, ..*identity };
        let tail = parsed.arena.arena.stmt_ids(parsed.arena.arena.block(block).statements).last().unwrap();
        assert!(matches!(parsed.arena.arena.stmt(tail).kind, ArenaStmtKind::Expr(_)));
        let tail_identity = super::StatementIdentity { statement: tail, source: identity.source, namespace: identity.namespace };
        assert_eq!(checked.solved.statements[&tail_identity], super::StatementPosition::Value);
        let result = checked.solved.graph.export_type(*ty).unwrap();
        assert!(matches!(result, Type::Result(ref item, ref error) if **item == Type::Int && **error == Type::Error));
        let (identity, owner, input_type) = (*identity, *owner, checked.solved.graph.export_type(checked.solved.expressions[&input_identity]).unwrap());
        let counters = checked.solved.graph.counters().clone();
        drop(parsed);
        checked.solved.validate().unwrap();
        assert_eq!(identity.source, SourceId::new(74));
        assert_eq!(checked.solved.expression_owners[&identity], owner);
        assert_eq!(checked.solved.graph.export_type(checked.solved.expressions[&identity]).unwrap(), result);
        assert_eq!(checked.solved.graph.export_type(checked.solved.expressions[&input_identity]).unwrap(), input_type);
        assert_eq!(checked.solved.statements[&tail_identity], super::StatementPosition::Value);
        assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
        assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        let denied = format!("proc scoped() [] -> Result[Int] {{ {expression} }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(74), &denied);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(Checker::check_arena(&parsed.arena, &denied).diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")));
    }
    for source in [
        "proc invalid() [env] { cd (1) { 7 } }\n",
        "proc invalid() [env] { env ({X: null}) { 7 } }\n",
        "pure invalid() { cd (p\".\") { 7 } }\n",
        "stream live() [] -> Stream[Int] { yield 1 }\nproc invalid() [env] { cd (p\".\") { live() } }\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(74), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(!Checker::check_arena(&parsed.arena, source).diagnostics.is_empty(), "invalid context input or escaping producer must be refused: {source}");
    }
}

#[test]
fn source_cli_descriptors_keep_original_boundary_and_static_plan_after_frontend_disposal() {
    use super::registry_boundaries::RegistryBoundaryKind;
    use crate::modules::RuntimeOp;
    for (declarations, call, expected) in [
        ("const schema = {count: {kind: \"Int\", default: 7}}\n", "cli.parse(argv: [], schema: schema)", RuntimeOp::CliParse),
        ("const schema = {count: {kind: \"Int\", default: 7}}\n", "cli.parse_full(argv: [], schema: schema)", RuntimeOp::CliParseFull),
        ("const schema = {count: {kind: \"Int\", default: 7}}\n", "cli.applet(argv: [], schema: schema)", RuntimeOp::CliApplet),
        ("const commands = {build: {positionals: [\"root\"], types: {root: \"Path\"}}}\n", "cli.commands(commands: commands, argv: [\"build\", \"workspace\"])", RuntimeOp::CliCommands),
    ] {
        let source = format!("{declarations}let output = {call}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(75), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{call}: {:?}", checked.diagnostics);
        let _symbols = checked.solved.symbol_owner().enter();
        let (identity, boundary) = checked.solved.registry_boundaries.iter().next().unwrap();
        assert_eq!(&source[parsed.arena.arena.expr(identity.expression).span.range()], call);
        assert_eq!(identity.source, SourceId::new(75));
        assert!(identity.namespace.is_none());
        assert!(boundary.caller.is_none());
        let RegistryBoundaryKind::CliDescriptor { operation, plan } = &boundary.kind else { panic!("the checked static descriptor owns its original plan") };
        assert_eq!(*operation, expected);
        assert!(plan.matches_operation(expected));
        let expected_type = plan.return_type(expected == RuntimeOp::CliParseFull);
        assert_eq!(checked.solved.graph.export_type(boundary.result).unwrap(), expected_type);
        let identity = *identity;
        let requirement = boundary.requirement.unwrap();
        assert_eq!(checked.solved.operations[&identity].requirement, requirement);
        let counters = checked.solved.graph.counters().clone();
        drop(parsed);
        checked.solved.validate().unwrap();
        let boundary = &checked.solved.registry_boundaries[&identity];
        let RegistryBoundaryKind::CliDescriptor { operation, plan } = &boundary.kind else { unreachable!() };
        assert_eq!(*operation, expected);
        assert!(plan.matches_operation(expected));
        assert_eq!(checked.solved.graph.export_type(boundary.result).unwrap(), expected_type);
        assert_eq!(boundary.requirement, Some(requirement));
        assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
        assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
    }
    for source in [
        "const schema = {count: {kind: \"UnknownKind\"}}\nlet invalid = cli.parse([], schema)\n",
        "proc invalid(schema: Record) [] -> Int { cli.parse([], schema) }\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(75), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(!Checker::check_arena(&parsed.arena, source).diagnostics.is_empty(), "a malformed or dynamic descriptor cannot promise the desired concrete type: {source}");
    }
}

#[test]
fn source_dynamic_pipeline_refuses_erased_outer_shape_but_preserves_typed_dynamic_items() {
    use crate::sema::inference::RequirementTemplate;
    for reverse in [false, true] {
        let mut calls = ["[1, 2]", "[\"word\"]"];
        if reverse { calls.reverse(); }
        let source = format!("pure typed(raw: List[Any]) -> List[Any] {{ raw |> map {{ . }} }}\n{}", calls.iter().enumerate().map(|(index, argument)| format!("let value_{index}: List[Any] = typed({argument})\n")).collect::<String>());
        let parsed = Parser::parse_source_arena_only(SourceId::new(76), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
        let (owner, declaration) = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "typed").unwrap();
        let TypeNode::Arrow(signature) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("the written List retains its outer source shape") };
        assert_eq!(checked.solved.graph.export_type(signature.params[0].ty).unwrap(), Type::List(Box::new(Type::Any)));
        assert_eq!(checked.solved.graph.export_type(signature.result).unwrap(), Type::List(Box::new(Type::Any)));
        assert_eq!(checked.solved.graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet::EMPTY));
        assert!(checked.solved.graph.scheme(declaration.scheme).unwrap().quantifiers.is_empty());
        let (identity, stage) = checked.solved.stage_operations.iter().next().unwrap();
        assert_eq!(stage.operation.caller, Some(*owner));
        assert_eq!(identity.pipeline.source, SourceId::new(76));
        assert_eq!(&source[parsed.arena.arena.expr(identity.pipeline.expression).span.range()], "raw |> map { . }");
        let receiver = stage.operation.receiver.unwrap();
        assert_eq!(checked.solved.graph.export_type(receiver).unwrap(), Type::List(Box::new(Type::Any)));
        let RequirementTemplate::Operation { call, .. } = checked.solved.graph.requirement_template(stage.operation.requirement).unwrap() else { panic!("the typed stage owns its checked source requirement") };
        assert_eq!(checked.solved.graph.operation_call(call).unwrap().receiver, Some(receiver));
        let identity = *identity;
        let counters = checked.solved.graph.counters().clone();
        drop(parsed);
        checked.solved.validate().unwrap();
        let stage = &checked.solved.stage_operations[&identity];
        assert_eq!(checked.solved.graph.export_type(stage.operation.receiver.unwrap()).unwrap(), Type::List(Box::new(Type::Any)));
        assert_eq!(checked.solved.graph.export_type(checked.solved.expressions[&identity.pipeline]).unwrap(), Type::List(Box::new(Type::Any)));
        assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
        assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        let invalid = format!("pure erased(raw: Any) -> List[Any] {{ raw |> map {{ . }} }}\n{}", calls.iter().enumerate().map(|(index, argument)| format!("let value_{index}: List[Any] = erased({argument})\n")).collect::<String>());
        let parsed = Parser::parse_source_arena_only(SourceId::new(76), &invalid);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &invalid);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch") && diagnostic.message.contains("Any")), "concrete callers cannot establish the explicit Any receiver's finite source shape: {:?}", checked.diagnostics);
        assert!(checked.solved.stage_operations.is_empty(), "an invalid erased source cannot publish checked pull/close authority");
    }
}
