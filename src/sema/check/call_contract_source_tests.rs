use super::{Checker, DeclarationIdentity, ExpressionIdentity, ProducerPath, ProducerPathComponent, SolvedTypes};
use crate::frontend::query::{NormalizedEffect, NormalizedProducerPath, NormalizedProducerPathComponent, SolvedQuery};
use crate::sema::inference::{EffectSet, EffectSummary, TypeNode};
use crate::source::SourceId;
use crate::symbol::Name;
use crate::syntax::arena::{ArenaProgram, ArenaProgramBuilder, ExprId};
use crate::syntax::parser::Parser;

fn expression(program: &ArenaProgram, source: &str, spelling: &str, owner: SourceId, namespace: Option<Name>) -> ExpressionIdentity {
    let expressions = (0..program.arena.expr_tags.len()).map(ExprId::from_index).filter(|&id| {
        let span = program.arena.expr(id).span;
        span.source_id == owner && source.get(span.range()) == Some(spelling)
    }).collect::<Vec<_>>();
    assert_eq!(expressions.len(), 1, "one original expression for {spelling}");
    ExpressionIdentity { source: owner, namespace, expression: expressions[0] }
}

fn declaration(program: &ArenaProgram, solved: &SolvedTypes, spelling: &str) -> DeclarationIdentity {
    *solved.declarations.keys().find(|identity| program.arena.function_def(identity.declaration).name == spelling).unwrap()
}

fn assert_profile(solved: &SolvedTypes, identity: ExpressionIdentity, path: Vec<ProducerPathComponent>, pull: EffectSet, close: EffectSet) {
    let profile = &solved.expression_producers[&identity];
    assert_eq!(profile.len(), 1, "the original value retains one precise producer location");
    let effects = &profile[&ProducerPath(path.clone())];
    assert_eq!(effects.pull, EffectSummary::Closed(pull));
    assert_eq!(effects.close, EffectSummary::Closed(close));
    let public_path = NormalizedProducerPath(path.into_iter().map(|component| match component {
        ProducerPathComponent::ResultSuccess => NormalizedProducerPathComponent::ResultSuccess,
        ProducerPathComponent::ResultError => NormalizedProducerPathComponent::ResultError,
        _ => panic!("this fixture uses only Result payload paths"),
    }).collect());
    let query = SolvedQuery::new(solved, solved.symbol_owner());
    let public = query.expression_producers(identity).unwrap();
    let effect_names = |bits: EffectSet| {
        let mut names = Vec::new();
        if bits.0 & EffectSet::ENV.0 != 0 { names.push("env".to_string()); }
        if bits.0 & EffectSet::TIME.0 != 0 { names.push("time".to_string()); }
        names
    };
    assert_eq!(public[&public_path].pull, NormalizedEffect::Closed(effect_names(pull)));
    assert_eq!(public[&public_path].close, NormalizedEffect::Closed(effect_names(close)));
    query.expression_producer_flow(identity).unwrap();
}

#[test]
fn qualified_module_calls_keep_exact_declaration_owners_after_ast_disposal() {
    let module_source = "##! A callable module.\n## Preserve the argument.\nexport pure render(value: Str, suffix: Str = \"!\") -> Str { value + suffix }\n";
    for calls in [
        "let first: Str = left(value: \"first\")\nlet second: Str = right(value: \"second\")\n",
        "let second: Str = right(value: \"second\")\nlet first: Str = left(value: \"first\")\n",
    ] {
        let source = format!("use first as a\nuse second as b\nlet left = a.render\nlet right = b.render\n{calls}");
        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        let entry = Parser::parse_source_into_arena_builder(SourceId::new(70), &source, &mut builder);
        let first = Parser::parse_source_into_arena_builder(SourceId::new(71), module_source, &mut builder);
        let second = Parser::parse_source_into_arena_builder(SourceId::new(72), module_source, &mut builder);
        assert!(entry.diagnostics.is_empty() && first.diagnostics.is_empty() && second.diagnostics.is_empty());
        let (first_import, first_namespace, second_namespace) = builder.symbol_owner().with_current(|| (Name::intern("first"), Name::intern("first-contract-owner"), Name::intern("second-contract-owner")));
        for statement in builder.statement_ids(entry.statements) {
            if let Some((import, path, _)) = builder.use_stmt_for_statement(statement) {
                builder.set_use_resolved(import, std::sync::Arc::from(if path.as_slice() == [first_import] { "first-contract-owner" } else { "second-contract-owner" }));
            }
        }
        builder.push_arena_module("first-contract-owner".to_string(), first_namespace, first.statements);
        builder.push_arena_module("second-contract-owner".to_string(), second_namespace, second.statements);
        let program = builder.finish_with_statements(entry.statements);
        let checked = Checker::check_arena(&program, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let identities = [expression(&program, &source, "left(value: \"first\")", SourceId::new(70), None), expression(&program, &source, "right(value: \"second\")", SourceId::new(70), None)];
        let callees = [expression(&program, &source, "left", SourceId::new(70), None), expression(&program, &source, "right", SourceId::new(70), None)];
        let mut targets = Vec::new();
        for (identity, callee, owner, namespace) in [(identities[0], callees[0], SourceId::new(71), first_namespace), (identities[1], callees[1], SourceId::new(72), second_namespace)] {
            let call = checked.solved.calls.get(&identity).unwrap_or_else(|| panic!("missing checked module call {identity:?}; calls={:?}, invocations={:?}, callables={:?}", checked.solved.calls.keys().collect::<Vec<_>>(), checked.solved.invocations, checked.solved.expression_callables));
            let target = checked.solved.expression_callables[&callee].declaration.expect("a checked qualified alias retains its actual module declaration");
            if let Some(call_target) = call.declaration { assert_eq!(call_target, target); }
            assert_eq!((target.source, target.namespace), (owner, Some(namespace)));
            assert_eq!(program.arena.function_def(target.declaration).name, "render");
            assert_eq!(call.actual_arguments, vec![checked.solved.expressions[&expression(&program, &source, if owner == SourceId::new(71) { "\"first\"" } else { "\"second\"" }, SourceId::new(70), None)]]);
            assert_eq!(call.binding.supplied_slots, vec![0]);
            assert_eq!(call.binding.default_slots, vec![1]);
            targets.push(target);
        }
        assert_ne!(targets[0], targets[1], "equal exported spellings do not merge declaring identities");
        drop(program);
        checked.solved.validate().unwrap();
        let before = checked.solved.graph.counters().instantiations;
        let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        for ((identity, callee), target) in identities.into_iter().zip(callees).zip(targets) {
            let signature = query.declaration(target).unwrap();
            assert_eq!(signature.scheme.ty.shape().callable_signature().unwrap().parameters[0].label, "value");
            assert_eq!(query.expression_callable(callee).unwrap().scheme.ty.shape().callable_signature().unwrap().parameters[0].label, "value");
            assert_eq!(query.call_binding(identity).unwrap().default_slots, vec![1]);
            assert_eq!(query.expression(identity).unwrap().to_string(), "Str");
        }
        assert_eq!(checked.solved.graph.counters().instantiations, before);
    }
}

#[test]
fn module_contract_projected_aliases_keep_their_source_caller_and_written_promises() {
    let source = "type Plugin = module { export pure render(value: Str, suffix: Str = \"!\") -> Str; export proc clock() [time] -> Int }\nproc inspect(plugin: Plugin) [time, error] -> Str { let format = plugin.get(\"render\")?; let clock = plugin[\"clock\"]; let _ = clock(); format(value: \"one\") }\nproc forwarded(plugin: Plugin) [time, error] -> Str { inspect(plugin) }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(73), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let inspect = declaration(&parsed.arena, &checked.solved, "inspect");
    let formatted = expression(&parsed.arena, source, "format(value: \"one\")", SourceId::new(73), None);
    let clock = expression(&parsed.arena, source, "clock()", SourceId::new(73), None);
    for identity in [formatted, clock] {
        let call = checked.solved.calls.get(&identity).unwrap_or_else(|| panic!("missing checked contract call {identity:?}; calls={:?}, invocations={:?}, callables={:?}", checked.solved.calls.keys().collect::<Vec<_>>(), checked.solved.invocations, checked.solved.expression_callables));
        assert_eq!(call.caller, Some(inspect));
        assert_eq!(call.declaration, None, "a contract projection cannot invent an implementation declaration");
        assert_eq!(checked.solved.expression_owners[&identity], inspect);
    }
    assert_eq!(checked.solved.calls[&formatted].binding.default_slots, vec![1]);
    drop(parsed);
    checked.solved.validate().unwrap();
    let before = checked.solved.graph.counters().instantiations;
    let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
    assert_eq!(query.expression(formatted).unwrap().to_string(), "Str");
    assert_eq!(query.expression(clock).unwrap().to_string(), "Int");
    assert_eq!(query.call_binding(formatted).unwrap().supplied_slots, vec![0]);
    assert_eq!(checked.solved.graph.counters().instantiations, before);
    for (invalid, code) in [(source.replace("value: \"one\"", "value: 1"), "check.type-mismatch"), (source.replace("[time, error]", "[error]"), "check.effect-violation")] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(73), &invalid);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &invalid);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some(code)), "{:?}", checked.diagnostics);
    }
}

#[test]
fn forwarded_result_constructors_retain_nested_success_and_data_error_profiles() {
    let definitions = "stream delayed() [time, env] -> Stream[Int] { defer { let _ = env.get(\"UNREAD\") }; let _ = time.now(); yield 1 }\nstream configured() [env] -> Stream[Int] { let _ = env.get(\"UNREAD\"); yield 2 }\npure wrapped(value) { Ok(value) }\npure failed(value) { Err(value) }\npure forwarded_ok(value) { wrapped(value) }\npure forwarded_err(value) { failed(value) }\n";
    for calls in [
        "let kept: Result[Result[Stream[Int]]] = forwarded_ok(Ok(delayed()))\nlet failed_data: Result[Int, Stream[Int]] = forwarded_err(configured())\n",
        "let failed_data: Result[Int, Stream[Int]] = forwarded_err(configured())\nlet kept: Result[Result[Stream[Int]]] = forwarded_ok(Ok(delayed()))\n",
    ] {
        let source = format!("{definitions}{calls}proc consume_ok() [time, env, error] -> List[Int] {{ (kept?)?.collect() }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(74), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let kept = expression(&parsed.arena, &source, "forwarded_ok(Ok(delayed()))", SourceId::new(74), None);
        let failed = expression(&parsed.arena, &source, "forwarded_err(configured())", SourceId::new(74), None);
        let wrapped = declaration(&parsed.arena, &checked.solved, "wrapped");
        let failed_definition = declaration(&parsed.arena, &checked.solved, "failed");
        let constructors = [expression(&parsed.arena, &source, "Ok(value)", SourceId::new(74), None), expression(&parsed.arena, &source, "Err(value)", SourceId::new(74), None)];
        for (identity, caller) in constructors.into_iter().zip([wrapped, failed_definition]) {
            let crate::syntax::arena::ArenaExprKind::Call { args, .. } = parsed.arena.arena.expr(identity.expression).kind else { panic!("original constructor source") };
            let argument = super::call_arg_expr_id_arena(&parsed.arena.arena.call_args(args)[0].kind);
            let argument = ExpressionIdentity { expression: argument, ..identity };
            let operation = &checked.solved.operations[&identity];
            assert_eq!(operation.caller, Some(caller));
            assert_eq!(operation.actual_arguments, vec![checked.solved.expressions[&argument]]);
            assert_eq!(operation.binding.supplied_slots, vec![0]);
        }
        for target in [wrapped, failed_definition] {
            assert!(!checked.solved.graph.scheme(checked.solved.declarations[&target].scheme).unwrap().quantifiers.is_empty());
        }
        drop(parsed);
        checked.solved.validate().unwrap();
        let before = checked.solved.graph.counters().instantiations;
        assert_profile(&checked.solved, kept, vec![ProducerPathComponent::ResultSuccess, ProducerPathComponent::ResultSuccess], EffectSet::TIME, EffectSet::ENV);
        assert_profile(&checked.solved, failed, vec![ProducerPathComponent::ResultError], EffectSet::ENV, EffectSet::EMPTY);
        let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        for identity in constructors { assert_eq!(query.language_operation(identity).unwrap().actual_arguments.len(), 1); }
        assert_eq!(checked.solved.graph.counters().instantiations, before);
        let consumed_error = format!("{source}proc consume_err() [env] -> List[Int] {{ match failed_data {{ Err(rows) => rows.collect(), Ok(_) => [] }} }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(74), &consumed_error);
        let consumed = Checker::check_arena(&parsed.arena, &consumed_error);
        assert!(consumed.diagnostics.is_empty(), "a retained data-error profile must reach its matching payload binding: {:?}", consumed.diagnostics);
        let receiver = expression(&parsed.arena, &consumed_error, "rows", SourceId::new(74), None);
        let subject = expression(&parsed.arena, &consumed_error, "failed_data", SourceId::new(74), None);
        let subject_flow = consumed.solved.expression_producer_flows[&subject];
        let mut receiver_flow = consumed.solved.expression_producer_flows[&receiver];
        for _ in 0..3 {
            let node = consumed.solved.producer_flows.node(receiver_flow).unwrap();
            if let super::ProducerFlowKind::Join { inputs } = &node.kind {
                assert_eq!(inputs.len(), 1, "this pattern binding has one exact payload source");
                receiver_flow = inputs[0];
            } else { break; }
        }
        let payload = consumed.solved.producer_flows.node(receiver_flow).unwrap();
        assert_eq!(payload.source, super::ProducerFlowSource::Expression(subject));
        assert!(matches!(&payload.kind, super::ProducerFlowKind::Project { input, path } if *input == subject_flow && *path == ProducerPath(vec![ProducerPathComponent::ResultError])), "the matched receiver projects the error payload of the original subject flow");
        drop(parsed);
        consumed.solved.validate().unwrap();
        let before = (consumed.solved.graph.counters().instantiations, consumed.solved.graph.counters().work_units);
        assert_profile(&consumed.solved, receiver, vec![], EffectSet::ENV, EffectSet::EMPTY);
        assert_eq!((consumed.solved.graph.counters().instantiations, consumed.solved.graph.counters().work_units), before);
        for invalid in [source.replace("[time, env, error]", "[error]"), consumed_error.replace("consume_err() [env]", "consume_err() []")] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(74), &invalid);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &invalid);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics);
        }
    }
}

#[test]
fn generic_error_capture_preserves_data_error_producers_through_forwarding() {
    let source = "stream configured() [env] -> Stream[Int] { let _ = env.get(\"UNREAD\"); yield 2 }\npure captured(value) { try { value? } }\npure forwarded(value) { captured(value) }\nlet failure: Result[Int, Stream[Int]] = Err(configured())\nlet retained: Result[Int, Stream[Int]] = forwarded(failure)\nproc accepted() [env] -> List[Int] { match retained { Err(rows) => rows.collect(), Ok(_) => [] } }\n";
    let direct = source.replace("forwarded(failure)", "failure");
    let parsed = Parser::parse_source_arena_only(SourceId::new(78), &direct);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, &direct);
    assert!(checked.diagnostics.is_empty(), "the original data error establishes the finite control: {:?}", checked.diagnostics);
    drop(parsed);
    checked.solved.validate().unwrap();
    let denied_direct = direct.replace("proc accepted() [env]", "proc denied() []");
    let parsed = Parser::parse_source_arena_only(SourceId::new(78), &denied_direct);
    let checked = Checker::check_arena(&parsed.arena, &denied_direct);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation") && diagnostic.message.contains("env")), "the direct error retains its required ENV: {:?}", checked.diagnostics);

    let denied = source.replace("proc accepted() [env]", "proc denied() []");
    let parsed = Parser::parse_source_arena_only(SourceId::new(78), &denied);
    let checked = Checker::check_arena(&parsed.arena, &denied);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation") && diagnostic.message.contains("env")), "capture cannot erase the propagated data error's ENV: {:?}", checked.diagnostics);
    assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.message.contains("unknown producer")), "{:?}", checked.diagnostics);

    let parsed = Parser::parse_source_arena_only(SourceId::new(78), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "local capture preserves the propagated error value without consuming its Stream: {:?}", checked.diagnostics);
    let retained = expression(&parsed.arena, source, "forwarded(failure)", SourceId::new(78), None);
    let receiver = expression(&parsed.arena, source, "rows", SourceId::new(78), None);
    let call = &checked.solved.calls[&retained];
    let TypeNode::Arrow(signature) = checked.solved.graph.node(call.signature).unwrap() else { panic!("forwarding keeps its checked callable") };
    assert_eq!(checked.solved.graph.closed_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet::EMPTY));
    drop(parsed);
    checked.solved.validate().unwrap();
    let before = checked.solved.graph.counters().clone();
    assert_profile(&checked.solved, retained, vec![ProducerPathComponent::ResultError], EffectSet::ENV, EffectSet::EMPTY);
    assert_profile(&checked.solved, receiver, Vec::new(), EffectSet::ENV, EffectSet::EMPTY);
    let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
    assert_eq!(query.expression(retained).unwrap().to_string(), "Result[Int, Stream[Int]]");
    query.expression_producer_flow(retained).unwrap();
    assert_eq!(checked.solved.graph.counters(), &before);

    let mut failed_channels = Vec::new();
    for (channel, definitions, input_type) in [
        ("implicit statement", "proc relay(value: Result[Unit, Stream[Int]]) [] -> Result[Unit, Stream[Int]] { value }\nproc captured(value: Result[Unit, Stream[Int]]) [] { try { relay(value); 7 } }\nproc forwarded(value) [] { captured(value) }\n", "Result[Unit, Stream[Int]]"),
        ("Result Map iteration", "pure captured(value: Result[Map[Int], Stream[Int]]) { try { for item in value { let _ = item }; 7 } }\npure forwarded(value) { captured(value) }\n", "Result[Map[Int], Stream[Int]]"),
        ("guarded field", "type Answer = { answer: Int }\npure captured(value: Result[Answer, Stream[Int]]) { try { value?.answer } }\npure forwarded(value) { captured(value) }\n", "Result[Answer, Stream[Int]]"),
        ("guarded index", "pure captured(value: Result[List[Int], Stream[Int]]) { try { value?[0] } }\npure forwarded(value) { captured(value) }\n", "Result[List[Int], Stream[Int]]"),
        ("Retry", "pure captured(value) { retry [] { value? } }\npure forwarded(value) { captured(value) }\n", "Result[Int, Stream[Int]]"),
        ("nested capture and Retry", "pure captured(value) { try { (retry [] { value? })? } }\npure forwarded(value) { captured(value) }\n", "Result[Int, Stream[Int]]"),
        ("outward generic propagation", "pure relayed(value) { value? }\npure captured(value) { try { relayed(value)? } }\npure forwarded(value) { captured(value) }\n", "Result[Int, Stream[Int]]"),
        ("outward implicit proc propagation", "proc relayed(value: Result[Unit, Stream[Int]]) [error] { (value); 7 }\nproc captured(value: Result[Unit, Stream[Int]]) [] { try { relayed(value)? } }\nproc forwarded(value) [] { captured(value) }\n", "Result[Unit, Stream[Int]]"),
    ] {
      let outcome = std::panic::catch_unwind(|| {
        let source = format!("stream configured() [env] -> Stream[Int] {{ let _ = env.get(\"UNREAD\"); yield 2 }}\n{definitions}let failure: {input_type} = Err(configured())\nlet retained: Result[Int, Stream[Int]] = forwarded(failure)\nproc accepted() [env] -> List[Int] {{ match retained {{ Err(rows) => rows.collect(), Ok(_) => [] }} }}\n");
        let denied = source.replace("proc accepted() [env]", "proc denied() []");
        let parsed = Parser::parse_source_arena_only(SourceId::new(78), &denied);
        assert!(parsed.diagnostics.is_empty(), "{channel}: {:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &denied);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation") && diagnostic.message.contains("env")), "{channel} cannot erase the original error producer's ENV: {:?}", checked.diagnostics);
        assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.message.contains("unknown producer")), "{channel}: {:?}", checked.diagnostics);

        let parsed = Parser::parse_source_arena_only(SourceId::new(78), &source);
        assert!(parsed.diagnostics.is_empty(), "{channel}: {:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{channel} preserves the data error without consuming it: {:?}", checked.diagnostics);
        let retained = expression(&parsed.arena, &source, "forwarded(failure)", SourceId::new(78), None);
        let receiver = expression(&parsed.arena, &source, "rows", SourceId::new(78), None);
        let captured = declaration(&parsed.arena, &checked.solved, "captured");
        let operand_spelling = match channel {
            "implicit statement" => "relay(value)",
            "outward generic propagation" | "outward implicit proc propagation" => "relayed(value)",
            _ => "value",
        };
        let operands = checked.solved.expression_owners.iter().filter_map(|(&identity, &owner)| {
            let span = parsed.arena.arena.expr(identity.expression).span;
            (owner == captured && source.get(span.range()) == Some(operand_spelling)).then_some(identity)
        }).collect::<Vec<_>>();
        assert_eq!(operands.len(), 1, "{channel} has one actual failure operand in its declaring body");
        let operand = operands[0];
        let input = checked.solved.expression_producer_flows[&operand];
        let call = &checked.solved.calls[&retained];
        let TypeNode::Arrow(signature) = checked.solved.graph.node(call.signature).unwrap() else { panic!("{channel}: checked forwarding callable") };
        assert_eq!(checked.solved.graph.closed_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet::EMPTY), "{channel} retains creation separately from the error payload");
        drop(parsed);
        checked.solved.validate().unwrap();
        let before = checked.solved.graph.counters().clone();
        assert!(checked.solved.producer_flows.nodes().any(|node| node.source == super::ProducerFlowSource::Expression(operand)
            && matches!(&node.kind, super::ProducerFlowKind::Project { input: actual, path } if *actual == input && *path == ProducerPath(vec![ProducerPathComponent::ResultError]))), "{channel} retains the actual original failure operand and its ResultError projection");
        assert_profile(&checked.solved, retained, vec![ProducerPathComponent::ResultError], EffectSet::ENV, EffectSet::EMPTY);
        assert_profile(&checked.solved, receiver, Vec::new(), EffectSet::ENV, EffectSet::EMPTY);
        let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        assert_eq!(query.expression(retained).unwrap().to_string(), "Result[Int, Stream[Int]]", "{channel}");
        assert_eq!(checked.solved.graph.counters(), &before);

      });
      if outcome.is_err() { failed_channels.push(channel); }
    }

    // Implicit List iteration transports arbitrary failure data as a coarse
    // runtime error; it cannot promise that the original Stream survives.
    let list_source = "stream configured() [env] -> Stream[Int] { let _ = env.get(\"UNREAD\"); yield 2 }\npure captured(value: Result[List[Int], Stream[Int]]) { try { for item in value { let _ = item }; 7 } }\npure forwarded(value) { captured(value) }\nlet failure: Result[List[Int], Stream[Int]] = Err(configured())\nlet retained: Result[Int] = forwarded(failure)\npure accepted() -> Int { match retained { Err(_) => 0, Ok(value) => value } }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(78), list_source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, list_source);
    assert!(checked.diagnostics.is_empty(), "the established List failure transport remains Error: {:?}", checked.diagnostics);
    let retained = expression(&parsed.arena, list_source, "forwarded(failure)", SourceId::new(78), None);
    drop(parsed);
    checked.solved.validate().unwrap();
    let before = checked.solved.graph.counters().clone();
    let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
    assert_eq!(query.expression(retained).unwrap().to_string(), "Result[Int, Error]");
    assert!(query.expression_producers(retained).unwrap().is_empty(), "coarse transport retains no original Stream handle");
    query.expression_producer_flow(retained).unwrap();
    assert_eq!(checked.solved.graph.counters(), &before);
    let precise = list_source.replace("let retained: Result[Int]", "let retained: Result[Int, Stream[Int]]");
    let parsed = Parser::parse_source_arena_only(SourceId::new(78), &precise);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, &precise);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch") && diagnostic.labels.iter().any(|label| label.message.as_deref().is_some_and(|message| message.contains("expected Result[Int, Stream[Int]], found Result[Int, Error]")))), "the coarse List failure cannot masquerade as its original data error: {:?}", checked.diagnostics);
    let denied_relay = "proc relayed(value: Result[Unit, Stream[Int]]) [] { (value); 7 }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(78), denied_relay);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, denied_relay);
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation") && diagnostic.message.contains("error")), "implicit statement propagation retains its authored ERROR budget: {:?}", checked.diagnostics);
    assert!(failed_channels.is_empty(), "capture producer channels failed: {failed_channels:?}");
}

#[test]
fn guarded_value_controls_keep_their_original_checked_relationships() {
    let mut failures = Vec::new();
    for (name, source) in [
        ("optional return", "proc cached(value: Str?) [] -> Str { return value when value != null; return \"missing\" }\n"),
        ("command return", "proc command(selected: Bool) [process] -> Status { return (run.status /usr/bin/true) unless !selected; return run.status /usr/bin/true when unless }\n"),
        ("producer yield", "stream items() [] -> Stream[Int] { yield 1 when true; yield 2 unless false }\n"),
        ("loop break", "let value = loop { break 4 when false; break 5 unless false }\n"),
        ("plain loop break", "let value = loop { break 5 }\n"),
        ("typed loop break", "let value: Int = loop { break 5 }\n"),
        ("discarded loop break", "let _ = loop { break 5 }\n"),
        ("nested loop break", "let value = loop { let inner = loop { break \"inner\" }; let _ = inner; break 5 }\n"),
        ("nested while and for break", "let value = loop { while false { break }; for item in [1] { break }; break 5 }\n"),
        ("nested declaration loop break", "let value = loop { pure nested() -> Str { return loop { break \"inner\" } }; break 5 }\n"),
        ("deferred while break", "let value = loop { defer { while false { break } }; break 5 }\n"),
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(79), source);
        assert!(parsed.diagnostics.is_empty(), "{name}: {:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        if !checked.diagnostics.is_empty() { failures.push((name, checked.diagnostics.clone())); }
        let loops = (0..parsed.arena.arena.expr_tags.len()).map(ExprId::from_index)
            .filter(|&id| matches!(parsed.arena.arena.expr(id).kind, crate::syntax::arena::ArenaExprKind::Loop { .. }))
            .map(|expression| ExpressionIdentity { source: SourceId::new(79), namespace: None, expression }).collect::<Vec<_>>();
        let nested = name == "nested loop break" || name == "nested declaration loop break";
        let expected_types = loops.iter().map(|identity| {
            let span = parsed.arena.arena.expr(identity.expression).span;
            if nested && source.get(span.range()).is_some_and(|spelling| spelling == "loop { break \"inner\" }") { "Str" } else { "Int" }
        }).collect::<Vec<_>>();
        drop(parsed);
        if checked.diagnostics.is_empty() {
            checked.solved.validate().unwrap();
            let before = checked.solved.graph.counters().clone();
            let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
            for (identity, expected) in loops.into_iter().zip(expected_types) {
                assert_eq!(query.expression(identity).unwrap().to_string(), expected, "{name} retains its own targeted break type");
                query.expression_producer_flow(identity).unwrap();
            }
            assert_eq!(checked.solved.graph.counters(), &before);
        }
    }
    assert!(failures.is_empty(), "guarded source relationships failed: {failures:?}");
    for source in [
        "let value = loop { break 4 when false; break \"wrong\" unless false }\n",
        "let value: Str = loop { break 5 }\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(79), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code.as_deref(), Some("check.type-mismatch" | "check.type-relationship" | "check.infer-return" | "check.loop-break-type"))
            && diagnostic.labels.iter().any(|label| label.span.source_id == SourceId::new(79) && !label.span.range().is_empty())), "incompatible original break endpoints require a local type error: {:?}", checked.diagnostics);
    }
}

#[test]
fn forwarded_result_constructors_preserve_nominal_error_joins_and_nested_values() {
    let definitions = "error FirstFailure = Missing(message: Str) | Denied(message: Str)\nerror SecondFailure = Missing(message: Str)\npure failed(value) { Err(value) }\npure forwarded_err(value) { failed(value) }\npure wrapped(value) { Ok(value) }\npure forwarded_ok(value) { wrapped(value) }\npure joined(left, right) { try { let _ = left?; let _ = right?; 7 } }\npure forwarded_join(left, right) { joined(left, right) }\n";
    for (second_family, second_variant, output_error) in [("FirstFailure", "Denied", "FirstFailure"), ("SecondFailure", "Missing", "Error")] {
      for reversed in [false, true] {
        let first_call = "forwarded_err(FirstFailure.Missing(message: \"first\"))";
        let second_call = format!("forwarded_err({second_family}.{second_variant}(message: \"second\"))");
        let first_binding = format!("let first: Result[Int, FirstFailure] = {first_call}\n");
        let second_binding = format!("let second: Result[Int, {second_family}] = {second_call}\n");
        let calls = if reversed { format!("{second_binding}{first_binding}") } else { format!("{first_binding}{second_binding}") };
        let source = format!("{definitions}{calls}let nested: Result[Result[Int, {output_error}]] = forwarded_ok(forwarded_join(first, second))\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(76), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let declarations = [declaration(&parsed.arena, &checked.solved, "joined"), declaration(&parsed.arena, &checked.solved, "forwarded_join")];
        for identity in declarations {
            let scheme = checked.solved.declarations[&identity].scheme;
            let principal = checked.solved.graph.scheme(scheme).unwrap();
            let TypeNode::Arrow(arrow) = checked.solved.graph.node(principal.body).unwrap() else { panic!("joined declaration arrow") };
            let formal_errors = arrow.params.iter().map(|parameter| {
                let TypeNode::Result(_, error) = checked.solved.graph.node(checked.solved.graph.resolved(parameter.ty).unwrap()).unwrap() else { panic!("each propagated formal retains its own Result shape") };
                checked.solved.graph.resolved(*error).unwrap()
            }).collect::<Vec<_>>();
            assert_eq!(formal_errors.len(), 2);
            assert_ne!(formal_errors[0], formal_errors[1], "the error join cannot collapse independent formal failure ports");
            assert!(formal_errors.iter().all(|&error| matches!(checked.solved.graph.node(error).unwrap(), TypeNode::Rigid { scope, .. } if *scope == scheme)), "each error input retains its actual declaration scope rather than grounding to Error or borrowing a forwarded scope");
            let joins = principal.requirements.iter().filter_map(|requirement| match requirement {
                crate::sema::inference::RequirementTemplate::ErrorJoin { join } => Some(checked.solved.graph.error_join(*join).unwrap()),
                _ => None,
            }).collect::<Vec<_>>();
            assert_eq!(joins.len(), 1, "the forwarded declaration retains the whole conditional error relationship");
            assert_eq!(joins[0].inputs.iter().map(|input| checked.solved.graph.resolved(*input).unwrap()).collect::<Vec<_>>(), formal_errors);
            let TypeNode::Result(_, result_error) = checked.solved.graph.node(checked.solved.graph.resolved(arrow.result).unwrap()).unwrap() else { panic!("the local capture retains its Result output") };
            assert_eq!(checked.solved.graph.resolved(joins[0].result).unwrap(), checked.solved.graph.resolved(*result_error).unwrap());
            assert!(joins[0].bound.is_none(), "the inferred join has no caller-supplied error promise");
        }
        let first = expression(&parsed.arena, &source, first_call, SourceId::new(76), None);
        let second = expression(&parsed.arena, &source, &second_call, SourceId::new(76), None);
        let nested = expression(&parsed.arena, &source, "forwarded_ok(forwarded_join(first, second))", SourceId::new(76), None);
        let errors = [first, second].map(|identity| {
            let TypeNode::Result(_, error) = checked.solved.graph.node(checked.solved.graph.resolved(checked.solved.expressions[&identity]).unwrap()).unwrap() else { panic!("constructor result preserves its nominal failure payload") };
            *error
        });
        if second_family == "SecondFailure" { assert_ne!(checked.solved.graph.resolved(errors[0]).unwrap(), checked.solved.graph.resolved(errors[1]).unwrap(), "different nominal families remain distinct before the join"); }
        let joined_call = expression(&parsed.arena, &source, "forwarded_join(first, second)", SourceId::new(76), None);
        let joined_result = checked.solved.expressions[&joined_call];
        let TypeNode::Result(_, error) = checked.solved.graph.node(checked.solved.graph.resolved(joined_result).unwrap()).unwrap() else { panic!("local capture owns the joined Result") };
        assert_eq!(checked.solved.graph.export_type(*error).unwrap().to_string(), output_error);
        drop(parsed);
        checked.solved.validate().unwrap();
        let before = checked.solved.graph.counters().instantiations;
        let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        let first_type = query.expression(first).unwrap();
        let second_type = query.expression(second).unwrap();
        if second_family == "SecondFailure" { assert_ne!(first_type, second_type); }
        assert_eq!(query.expression(nested).unwrap().to_string(), format!("Result[Result[Int, {output_error}], Error]"));
        for identity in declarations { query.declaration(identity).unwrap(); }
        query.expression_producer_flow(nested).unwrap();
        assert_eq!(checked.solved.graph.counters().instantiations, before);
        let invalid = source.replace("let first: Result[Int, FirstFailure]", "let first: Result[Int, SecondFailure]");
        let parsed = Parser::parse_source_arena_only(SourceId::new(76), &invalid);
        let checked = Checker::check_arena(&parsed.arena, &invalid);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{:?}", checked.diagnostics);
        let bounded = format!("{definitions}pure promised(left, right) -> Result[Int] {{ try {{ let _ = left?; let _ = right?; 7 }} }}\npure forwarded_promised(left, right) {{ promised(left, right) }}\n{calls}let bounded: Result[Int] = forwarded_promised(first, second)\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(77), &bounded);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &bounded);
        assert!(checked.diagnostics.is_empty(), "a written Error output admits the nominal join without grounding its independent inputs: {:?}", checked.diagnostics);
        let promised = declaration(&parsed.arena, &checked.solved, "promised");
        let scheme = checked.solved.declarations[&promised].scheme;
        let TypeNode::Arrow(arrow) = checked.solved.graph.node(checked.solved.graph.scheme(scheme).unwrap().body).unwrap() else { panic!("written Result declaration arrow") };
        let inputs = arrow.params.iter().map(|parameter| {
            let TypeNode::Result(_, error) = checked.solved.graph.node(checked.solved.graph.resolved(parameter.ty).unwrap()).unwrap() else { panic!("propagated bounded input") };
            checked.solved.graph.resolved(*error).unwrap()
        }).collect::<Vec<_>>();
        assert_ne!(inputs[0], inputs[1]);
        assert!(inputs.iter().all(|input| matches!(checked.solved.graph.node(*input).unwrap(), TypeNode::Rigid { scope, .. } if *scope == scheme)));
        drop(parsed);
        checked.solved.validate().unwrap();
      }
    }
}

#[test]
fn generic_result_propagation_retains_its_original_operand_and_local_error_port() {
    let definitions = "error FirstFailure = Missing(message: Str)\nerror SecondFailure = Missing(message: Str)\npure projected(value) { try { value? } }\npure forwarded(value) { projected(value) }\nlet first_input: Result[Int, FirstFailure] = Ok(7)\nlet second_input: Result[Str, SecondFailure] = Ok(\"word\")\n";
    for calls in ["let first: Result[Int, FirstFailure] = forwarded(first_input)\nlet second: Result[Str, SecondFailure] = forwarded(second_input)\n", "let second: Result[Str, SecondFailure] = forwarded(second_input)\nlet first: Result[Int, FirstFailure] = forwarded(first_input)\n"] {
        let source = format!("{definitions}{calls}");
        let parsed = Parser::parse_source_arena_only(SourceId::new(78), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let projected = declaration(&parsed.arena, &checked.solved, "projected");
        let scheme = checked.solved.graph.scheme(checked.solved.declarations[&projected].scheme).unwrap();
        assert_eq!(scheme.quantifiers.len(), 2, "success and error belong to the definition independently");
        let TypeNode::Arrow(arrow) = checked.solved.graph.node(scheme.body).unwrap() else { panic!("Result projection declaration arrow") };
        let parts = |ty| {
            let TypeNode::Result(success, error) = checked.solved.graph.node(checked.solved.graph.resolved(ty).unwrap()).unwrap() else { panic!("source postfix determines Result shape") };
            (checked.solved.graph.resolved(*success).unwrap(), checked.solved.graph.resolved(*error).unwrap())
        };
        assert_eq!(parts(arrow.params[0].ty), parts(arrow.result), "the capture preserves the same success and error endpoints without requiring container interning");
        let original = expression(&parsed.arena, &source, "value?", SourceId::new(78), None);
        let crate::syntax::arena::ArenaExprKind::Try(operand) = parsed.arena.arena.expr(original.expression).kind else { panic!("original source propagation") };
        let operand = ExpressionIdentity { expression: operand, ..original };
        assert_eq!(parts(checked.solved.expressions[&operand]), parts(arrow.params[0].ty));
        let TypeNode::Result(success, _) = checked.solved.graph.node(checked.solved.graph.resolved(arrow.params[0].ty).unwrap()).unwrap() else { panic!("source postfix determines Result shape") };
        assert_eq!(checked.solved.graph.resolved(*success).unwrap(), checked.solved.graph.resolved(checked.solved.expressions[&original]).unwrap());
        assert_eq!(checked.solved.declarations[&projected].effective_effects, EffectSummary::Closed(EffectSet::EMPTY));
        drop(parsed);
        checked.solved.validate().unwrap();
        let before = checked.solved.graph.counters().instantiations;
        let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        query.declaration(projected).unwrap();
        query.expression(original).unwrap();
        query.expression_producer_flow(original).unwrap();
        assert_eq!(checked.solved.graph.counters().instantiations, before);
        let invalid = format!("{definitions}let rejected = forwarded(7)\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(78), &invalid);
        let checked = Checker::check_arena(&parsed.arena, &invalid);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{:?}", checked.diagnostics);
    }
}

#[test]
fn result_constructor_arguments_keep_their_reached_creation_effects() {
    for constructor in ["Ok", "Err"] {
        let source = format!("proc construct() [time] -> Result[Int, Int] {{ {constructor}(time.now()) }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(77), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let construct = declaration(&parsed.arena, &checked.solved, "construct");
        let original = expression(&parsed.arena, &source, &format!("{constructor}(time.now())"), SourceId::new(77), None);
        let argument = expression(&parsed.arena, &source, "time.now()", SourceId::new(77), None);
        let operation = &checked.solved.operations[&original];
        assert_eq!(operation.caller, Some(construct));
        assert_eq!(operation.actual_arguments, vec![checked.solved.expressions[&argument]]);
        assert_eq!(checked.solved.graph.closed_effect_summary(checked.solved.declarations[&construct].required_effects).unwrap(), EffectSummary::Closed(EffectSet::TIME));
        drop(parsed);
        checked.solved.validate().unwrap();
        let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        assert_eq!(query.language_operation(original).unwrap().actual_arguments.len(), 1);
        let invalid = source.replace("[time]", "[]");
        let parsed = Parser::parse_source_arena_only(SourceId::new(77), &invalid);
        let checked = Checker::check_arena(&parsed.arena, &invalid);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics);
    }
}

#[test]
fn forwarded_defaults_keep_definition_owned_producers_separate_from_supplied_values() {
    let definitions = "stream delayed() [time, env] -> Stream[Int] { defer { let _ = env.get(\"UNREAD\") }; let _ = time.now(); yield 1 }\nstream configured() [env] -> Stream[Int] { let _ = env.get(\"UNREAD\"); yield 2 }\nproc selected(rows: Stream[Int] = delayed()) [] -> Stream[Int] { rows }\nproc forwarded(rows) [] { selected(rows) }\n";
    for calls in ["let defaulted = selected()\nlet supplied = forwarded(configured())\n", "let supplied = forwarded(configured())\nlet defaulted = selected()\n"] {
        let source = format!("{definitions}{calls}proc consume_default() [time, env] -> List[Int] {{ defaulted.collect() }}\nproc consume_supplied() [env] -> List[Int] {{ supplied.collect() }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(75), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let defaulted = expression(&parsed.arena, &source, "selected()", SourceId::new(75), None);
        let supplied = expression(&parsed.arena, &source, "forwarded(configured())", SourceId::new(75), None);
        let selected = declaration(&parsed.arena, &checked.solved, "selected");
        let TypeNode::Arrow(arrow) = checked.solved.graph.node(checked.solved.declarations[&selected].signature).unwrap() else { panic!("default declaration arrow") };
        assert!(arrow.params[0].defaulted);
        assert_eq!(arrow.effects, EffectSummary::Closed(EffectSet::EMPTY), "constructing the delayed default handle does not consume it");
        assert_eq!(checked.solved.calls[&defaulted].declaration, Some(selected));
        assert_eq!(checked.solved.calls[&defaulted].binding.default_slots, vec![0]);
        assert!(checked.solved.calls[&defaulted].actual_arguments.is_empty());
        assert_eq!(checked.solved.calls[&supplied].binding.supplied_slots, vec![0]);
        drop(parsed);
        checked.solved.validate().unwrap();
        let before = checked.solved.graph.counters().instantiations;
        assert_profile(&checked.solved, defaulted, vec![], EffectSet::TIME, EffectSet::ENV);
        assert_profile(&checked.solved, supplied, vec![], EffectSet::ENV, EffectSet::EMPTY);
        assert_eq!(checked.solved.graph.counters().instantiations, before);
        for invalid in [source.replace("consume_default() [time, env]", "consume_default() [env]"), source.replace("consume_supplied() [env]", "consume_supplied() []")] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(75), &invalid);
            let checked = Checker::check_arena(&parsed.arena, &invalid);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", checked.diagnostics);
        }
    }
}
