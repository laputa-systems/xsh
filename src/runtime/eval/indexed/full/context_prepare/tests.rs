use super::*;
use crate::sema::check::Checker;
use crate::source::{SourceId, SourceMap};

fn fixture() -> FullProgram {
    let source = "proc scoped() [env] -> Result[Int] { let first = env ({X: \"one\"}) { 7 }; env ({X: \"two\"}) { 9 } }\n";
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("context-proof.xsh", crate::loader::entry_source_from_text("context-proof.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let weak = Arc::downgrade(&checked.solved);
    let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap();
    drop(parsed); drop(checked); drop(declarations); drop(bodies);
    assert!(weak.upgrade().is_none(), "context receipts must not retain the checker graph");
    FullVerifier::verify(&program).unwrap();
    assert_eq!(program.generic_evidence().unwrap().context_producers().count(), 2);
    program
}

fn change_word(program: &mut FullProgram, instruction: u32, word: usize, value: u32) {
    let range = program.store.data[instruction as usize].range().bounds(program.store.extra.len()).unwrap();
    assert!(word < range.len());
    program.store.extra[range.start + word] = value;
}

#[test]
fn context_producers_reject_jointly_rewritten_equal_type_sibling_inputs_and_bodies() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        let receipts = program.generic_evidence().unwrap().context_producers().cloned().collect::<Vec<_>>();
        let (left, right) = (&receipts[0], &receipts[1]);
        assert_eq!(left.owner, right.owner);
        assert_eq!(left.result, right.result);
        assert_eq!(left.input.ty, right.input.ty);
        let mut input = program.clone();
        change_word(&mut input, left.instruction, 1, right.input.instruction);
        let receipt = input.store.generic.as_mut().unwrap().test_context_producer_mut(left.instruction).unwrap();
        receipt.input = right.input.clone();
        receipt.payload[1] = right.input.instruction;
        assert!(FullVerifier::verify(&input).is_err(), "a sibling input cannot acquire another context's parent");
        let mut rewritten_receipt = program.clone();
        rewritten_receipt.store.generic.as_mut().unwrap().test_context_producer_mut(left.instruction).unwrap().input = right.input.clone();
        let error = FullVerifier::verify(&rewritten_receipt).unwrap_err();
        assert!(error.message.contains("original receipt"), "{}", error.message);
        let mut body = program.clone();
        change_word(&mut body, left.instruction, 2, right.body.raw());
        let receipt = body.store.generic.as_mut().unwrap().test_context_producer_mut(left.instruction).unwrap();
        receipt.body = right.body;
        receipt.statements = right.statements.clone();
        receipt.tail = right.tail.clone();
        receipt.payload[2] = right.body.raw();
        assert!(FullVerifier::verify(&body).is_err(), "a sibling body cannot acquire another context's parent");
    });
}

#[test]
fn context_producers_reject_foreign_source_owner_result_and_missing_receipts() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        let receipt = program.generic_evidence().unwrap().context_producers().next().unwrap().clone();
        for mutation in 0..4 {
            let mut changed = program.clone();
            let original = changed.store.generic.as_mut().unwrap().test_context_producer_mut(receipt.instruction).unwrap();
            match mutation {
                0 => original.origin.source = SourceId::new(123),
                1 => original.owner = InstructionOwner::Driver(0),
                2 => original.result = original.input.ty,
                3 => {
                    let super::super::super::generic::OperationSourceOrigin::Expression(origin) = &mut original.tail.as_mut().unwrap().origin else { unreachable!() };
                    origin.source = SourceId::new(123);
                },
                _ => unreachable!(),
            }
            assert!(FullVerifier::verify(&changed).is_err(), "context source mutation {mutation} must reject");
        }
        let mut missing = program.clone();
        missing.store.generic.as_mut().unwrap().test_remove_context_producers();
        assert!(FullVerifier::verify(&missing).is_err(), "visible ledger removal cannot retire original context authority");
        let mut payload = program.clone();
        change_word(&mut payload, receipt.instruction, 0, 0);
        assert!(FullVerifier::verify(&payload).is_err(), "a valid kind word cannot replace the checked overlay protocol");
    });
}

fn capture_fixture() -> FullProgram {
    let source = "proc scoped() [env, process, error] -> Result[Str] { let first: Result[Str] = env ({X: \"one\"}) { run.text sh -c \"printf left\" ? }; env ({X: \"two\"}) { run.text sh -c \"printf right\" ? } }\n";
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("context-capture-proof.xsh", crate::loader::entry_source_from_text("context-capture-proof.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let weak = Arc::downgrade(&checked.solved);
    for (identity, run) in &checked.solved.run_operations {
        let ProducerFlowSource::Statement(parent) = run.parent else { panic!("a command tail retains its original statement parent: {:?}", run.parent) };
        let crate::syntax::arena::ArenaStmtKind::Command(command) = parsed.arena.arena.stmt(parent.statement).kind else { panic!("the run parent must be its authored command statement") };
        assert!(matches!(parsed.arena.arena.command_stmt(command).command, crate::syntax::arena::ArenaCommand::Run(original) if original == identity.run), "the run cannot acquire its enclosing let or another command's source parent");
        let ty = checked.solved.graph.export_type(run.operation.result).unwrap();
        assert_eq!(ty, Type::Str, "the propagated text run keeps its checked success result");
    }
    for (identity, ty) in &checked.solved.expressions {
        if matches!(parsed.arena.arena.expr(identity.expression).kind, crate::syntax::arena::ArenaExprKind::ContextScope { .. }) {
            assert_eq!(checked.solved.graph.export_type(*ty).unwrap(), Type::Result(Box::new(Type::Str), Box::new(Type::Error)), "the context retains the checked text result independently of its receiver");
        }
    }
    let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap();
    drop(parsed); drop(checked); drop(declarations); drop(bodies);
    assert!(weak.upgrade().is_none(), "run receipts must not retain the checker graph");
    FullVerifier::verify(&program).unwrap();
    assert_eq!(program.generic_evidence().unwrap().run_producers().count(), 2);
    program
}

#[test]
fn context_capture_producers_retain_original_run_result_and_statement_tail_after_frontend_drop() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(capture_fixture());
        for recursive in [false, true] {
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone())
                .with_env_var(b"X".to_vec(), b"outer".to_vec());
            let original = evaluator.env.snapshot_clone();
            evaluator.indexed_program = Some(Arc::clone(&program));
            let name = program.symbol_owner().with_current(|| Name::intern("scoped"));
            let key = super::super::super::super::LoweredFunctionKey::Name(name);
            let call = || evaluator.call_indexed_direct(key, super::super::super::super::LoweredFunctionKind::Proc, &[], Span::new(program.store.source_id, 0, 0)).unwrap();
            let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap();
            assert_eq!(result, crate::runtime::value::Value::ok(crate::runtime::value::Value::Str(Arc::from("right"))));
            assert_eq!(evaluator.env.snapshot_clone(), original);
        }
    });
}

#[test]
fn context_capture_producers_reject_jointly_rewritten_run_capture_continuation_and_authority() {
    crate::runtime::eval::run_eval(|| {
        let program = capture_fixture();
        let runs = program.generic_evidence().unwrap().run_producers().cloned().collect::<Vec<_>>();
        let (left, right) = (&runs[0], &runs[1]);
        assert_eq!(left.result, right.result);
        assert_eq!(left.carrier, right.carrier);
        let mut redirected = program.clone();
        change_word(&mut redirected, left.continuation, 0, right.capture);
        let receipt = redirected.store.generic.as_mut().unwrap().test_run_producer_mut(left.capture).unwrap();
        receipt.capture = right.capture;
        receipt.payload = right.payload.clone();
        receipt.continuation_payload[0] = right.capture;
        receipt.operands = right.operands.clone();
        receipt.blocks = right.blocks.clone();
        receipt.texts = right.texts.clone();
        assert!(FullVerifier::verify(&redirected).is_err(), "equal carrier types cannot give a run its sibling's capture");
        let mut authority = program.clone();
        let receipt = authority.store.generic.as_mut().unwrap().test_run_producer_mut(left.capture).unwrap();
        let super::super::super::generic::PreparedOperationAuthority::Language { operation, .. } = &mut receipt.authority else { unreachable!() };
        *operation = crate::sema::operation_graph::PreparedLanguageOperation::Run { kind: RunKind::CaptureBytes, policy: false, propagate: true };
        assert!(FullVerifier::verify(&authority).is_err(), "a checked text run cannot become a bytes producer");
        let mut literal = program.clone();
        change_word(&mut literal, left.operands[0].instruction, 0, right.operands[0].payload[0]);
        literal.store.generic.as_mut().unwrap().test_run_producer_mut(left.capture).unwrap().operands[0].payload[0] = right.operands[0].payload[0];
        assert!(FullVerifier::verify(&literal).is_err(), "a rewritten operand payload cannot replace the original argv producer");
    });
}
