use super::*;
use crate::sema::check::Checker;

fn ground_builder() -> (FullBuilder, Arc<SourceMap>) {
    let source = "pure identity(value: Int) -> Int { value }\npure replacement(value: Int) -> Int { value + 1 }\npure observed() -> List[Int] { [1, 2] |> map(identity) }\n";
    ground_builder_with_source(source)
}

fn ground_builder_with_source(source: &str) -> (FullBuilder, Arc<SourceMap>) {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "ground-stage-call-proof.xsh", crate::loader::entry_source_from_text("ground-stage-call-proof.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let builder = parsed.arena.symbol_owner().clone().with_current(|| {
        let mut builder = FullBuilder::new(source_id);
        builder.solved = Some(Arc::clone(&bodies.solved));
        builder.reserve_function_keys(super::super::super::super::lower::compact_function_keys(&parsed.arena)).unwrap();
        super::super::super::super::lower::lower_compact_function_units_into(
            &parsed.arena, &declarations, &bodies, source, &sources,
            super::super::super::super::lower::StdlibLowerLinkage::Local,
            |mut unit| {
                builder.predeclare(&[&unit])?;
                let body = unit.take_lowered_body().unwrap();
                let function = builder.function_ids[&unit.key()];
                builder.current_owner = Some(function.raw());
                builder.current_slot_count = body.slot_count as u32;
                builder.encode_body(function, &body)?;
                builder.current_owner = None;
                builder.current_slot_count = 0;
                Ok(())
            }).unwrap();
        builder
    });
    (builder, Arc::new(sources))
}

#[test]
fn ground_stage_callback_rejects_a_different_original_declaration_before_publication() {
    let (mut builder, _) = ground_builder();
    let solved = Arc::clone(builder.solved.as_ref().unwrap());
    solved.symbol_owner().with_current(|| {
        assert_eq!(builder.generic_stage_call_rows.len(), 1);
        let instruction = builder.generic_stage_call_rows[0].0 as usize;
        let replacement = builder.function_ids[&LoweredFunctionKey::Name(Name::intern("replacement"))];
        let words = builder.store.data[instruction].range().bounds(builder.store.extra.len()).unwrap();
        let original = IrFunctionId::from_raw(builder.store.extra[words.start]).unwrap();
        assert_eq!(builder.store.functions[original.index()].signature, builder.store.functions[replacement.index()].signature);
        builder.store.extra[words.start] = replacement.raw();
        assert!(builder.prepare_generic_expressions().is_err(), "same-signature callback target escaped its original declaration proof");
    });
}

#[test]
fn ground_stage_callback_rejects_a_different_target_after_frontend_drop() {
    let (builder, sources) = ground_builder();
    let symbols = builder.solved.as_ref().unwrap().symbol_owner().clone();
    let mut program = symbols.clone().with_current(|| builder.finish(sources, symbols)).unwrap();
    program.symbol_owner().clone().with_current(|| {
        assert_eq!(program.generic_evidence().unwrap().ground_stage_calls().count(), 1);
        let replacement = IrFunctionId::new(program.function_view(LoweredFunctionKey::Name(Name::intern("replacement")), LoweredFunctionKind::Pure).unwrap().unwrap().index).unwrap();
        let map = program.store.stages.iter().position(|tag| *tag == FullStageTag::Map).unwrap();
        let words = program.store.payload(program.store.stage_data[map].range()).unwrap();
        let call = words[1] as usize;
        let words = program.store.data[call].range().bounds(program.store.extra.len()).unwrap();
        let original = IrFunctionId::from_raw(program.store.extra[words.start]).unwrap();
        assert_ne!(original, replacement);
        assert_eq!(program.store.functions[original.index()].signature, program.store.functions[replacement.index()].signature);
        program.store.extra[words.start] = replacement.raw();
        assert!(FullVerifier::verify(&program).is_err(), "ground callback lost its original target after frontend drop");
    });
}

#[test]
fn ground_stage_callback_rejects_missing_foreign_misplaced_and_rewritten_proofs() {
    let (builder, sources) = ground_builder();
    let symbols = builder.solved.as_ref().unwrap().symbol_owner().clone();
    let program = symbols.clone().with_current(|| builder.finish(sources, symbols)).unwrap();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (id, proof) = generic.ground_stage_calls().next().unwrap();
        let source = generic.stage_call_source(proof.source).unwrap();
        let mut missing = program.store.clone();
        missing.generic.as_deref_mut().unwrap().test_remove_ground_stage_calls();
        assert!(FullVerifier::verify_generic_evidence(&missing).unwrap_err().message.contains("missing its proof"));
        missing.generic.as_deref_mut().unwrap().test_remove_stage_call_sources();
        assert!(FullVerifier::verify_generic_evidence(&missing).unwrap_err().message.contains("original stage callback instruction"));
        let mut origin = program.store.clone();
        origin.generic.as_deref_mut().unwrap().test_stage_call_source_mut(proof.source).unwrap().origin.pipeline.namespace = Some(Name::intern("forged"));
        assert!(FullVerifier::verify_generic_evidence(&origin).unwrap_err().message.contains("original instruction"));
        let mut misplaced = program.store.clone();
        misplaced.generic.as_deref_mut().unwrap().test_stage_call_source_mut(proof.source).unwrap().instruction = proof.contract.argument_sources.iter().flatten().copied().next().unwrap();
        assert!(FullVerifier::verify_generic_evidence(&misplaced).is_err());
        let replacement = IrFunctionId::new(program.function_view(LoweredFunctionKey::Name(Name::intern("replacement")), LoweredFunctionKind::Pure).unwrap().unwrap().index).unwrap();
        let mut rewritten = program.store.clone();
        let metadata = rewritten.generic.as_deref_mut().unwrap();
        metadata.test_stage_call_source_mut(proof.source).unwrap().expected.target = replacement;
        metadata.test_ground_stage_call_mut(id).unwrap().contract.target = replacement;
        let payload = rewritten.data[source.instruction as usize].range().bounds(rewritten.extra.len()).unwrap();
        rewritten.extra[payload.start] = replacement.raw();
        assert!(FullVerifier::verify_generic_evidence(&rewritten).unwrap_err().message.contains("another declaration"));
        let mut opcode = program.store.clone();
        opcode.tags[source.instruction as usize] = FullTag::ExprInt;
        assert!(FullVerifier::verify_generic_evidence(&opcode).is_err());
        let mut owner = program.store.clone();
        owner.generic.as_deref_mut().unwrap().test_stage_call_source_mut(proof.source).unwrap().owner = InstructionOwner::Function(replacement);
        assert!(FullVerifier::verify_generic_evidence(&owner).is_err());
        let mut builder = super::super::super::generic::GenericEvidenceBuilder::default();
        let foreign_source = builder.add_stage_call_source(source.clone()).unwrap();
        let mut foreign = program.store.clone();
        foreign.generic.as_deref_mut().unwrap().test_ground_stage_call_mut(id).unwrap().source = foreign_source;
        assert!(FullVerifier::verify_generic_evidence(&foreign).unwrap_err().message.contains("foreign program"));
    });
}

#[test]
fn ground_stage_callback_checkpoint_retires_sources_and_preserves_current_serials() {
    let (builder, sources) = ground_builder();
    let symbols = builder.solved.as_ref().unwrap().symbol_owner().clone();
    let program = symbols.clone().with_current(|| builder.finish(sources, symbols)).unwrap();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (_, proof) = generic.ground_stage_calls().next().unwrap();
        let source = generic.stage_call_source(proof.source).unwrap();
        let mut builder = super::super::super::generic::GenericEvidenceBuilder::default();
        for (_, function) in generic.checked_functions() { builder.add_checked_function(*function).unwrap(); }
        builder.register_instruction_origin(source.instruction, super::super::super::generic::OperationSourceOrigin::Stage(source.origin), source.owner).unwrap();
        let checkpoint = builder.checkpoint();
        let old_source = builder.add_stage_call_source(source.clone()).unwrap();
        let mut current = proof.clone(); current.source = old_source;
        let old_call = builder.add_ground_stage_call(current.clone()).unwrap();
        let retired = builder.checkpoint();
        builder.rewind(checkpoint).unwrap();
        let current_source = builder.add_stage_call_source(source.clone()).unwrap();
        current.source = current_source;
        let current_call = builder.add_ground_stage_call(current).unwrap();
        assert!(builder.rewind(retired).is_err());
        let owners = program.store.generic_instruction_owners().unwrap();
        let store = builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap();
        assert!(store.stage_call_source(old_source).is_err());
        assert!(store.ground_stage_call(old_call).is_err());
        assert!(store.stage_call_source(current_source).is_ok());
        assert!(store.ground_stage_call(current_call).is_ok());
        assert!(store.stage_call_source(proof.source).is_err());
    });
}

fn fixture() -> FullProgram {
    let source = "pure identity(value) { value }\npure observed() -> List[Int] { [1, 2] |> map(identity) }\nlet result = observed()\n";
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "stage-call-proof.xsh", crate::loader::entry_source_from_text("stage-call-proof.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let stage = bodies.solved.stage_operations.values().next().unwrap();
    let Some(crate::sema::check::StageCallback::Callable { requirement, instance: Some(_), .. }) = &stage.callback else { panic!("original stage callback certificate is required") };
    assert!(bodies.solved.graph.invocation_evidence(*requirement).unwrap().unwrap().unique_plan().is_some());
    let counters = bodies.solved.graph.counters().clone();
    let solved = Arc::downgrade(&bodies.solved);
    let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap();
    assert_eq!(bodies.solved.graph.counters(), &counters);
    drop(parsed); drop(declarations); drop(bodies);
    assert!(solved.upgrade().is_none());
    program
}

#[test]
fn ground_stage_callback_rejects_changed_sequence_carrier() {
    let (builder, sources) = ground_builder();
    let symbols = builder.solved.as_ref().unwrap().symbol_owner().clone();
    let program = symbols.clone().with_current(|| builder.finish(sources, symbols)).unwrap();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (id, proof) = generic.ground_stage_calls().next().unwrap();
        assert_ne!(proof.contract.input_sequence, proof.contract.result_sequence);
        assert_eq!(proof.contract.input_item, proof.contract.result_item);
        let mut changed = program.store.clone();
        let metadata = changed.generic.as_deref_mut().unwrap();
        metadata.test_ground_stage_call_mut(id).unwrap().contract.result_sequence = proof.contract.input_sequence;
        metadata.test_stage_call_source_mut(proof.source).unwrap().expected.result_sequence = proof.contract.input_sequence;
        assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "a materialized List replaced the selected stage's Stream output");
    });
}

#[test]
fn ground_stage_chain_uses_original_changed_item_result_and_order() {
    let source = "pure mark(value: Int) -> Bool { value == 1 }\npure keep(value: Bool) -> Bool { value }\npure observed() -> List[Bool] { [1, 2] |> map(mark) |> where(keep) }\n";
    let (builder, sources) = ground_builder_with_source(source);
    let symbols = builder.solved.as_ref().unwrap().symbol_owner().clone();
    let program = symbols.clone().with_current(|| builder.finish(sources, symbols)).unwrap();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let calls = generic.ground_stage_calls().collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        let (first_id, first) = calls[0];
        let (_, next) = calls[1];
        assert_eq!(first.contract.result_item, next.contract.input_item);
        assert_ne!(first.contract.input_item, first.contract.result_item);
        let mut changed = program.store.clone();
        let metadata = changed.generic.as_deref_mut().unwrap();
        let wrong = first.contract.input_item;
        metadata.test_ground_stage_call_mut(first_id).unwrap().contract.result_item = wrong;
        metadata.test_stage_call_source_mut(first.source).unwrap().expected.result_item = wrong;
        assert!(FullVerifier::verify_generic_evidence(&changed).is_err());
        let pipeline = program.store.tags.iter().position(|tag| *tag == FullTag::ExprPipeline).unwrap();
        let words = program.store.payload(program.store.data[pipeline].range()).unwrap();
        let stages = program.store.blocks[IrBlockId::from_raw(words[1]).unwrap().index()].instructions.bounds(program.store.extra.len()).unwrap();
        let mut reversed = program.store.clone();
        reversed.extra.swap(stages.start + 1, stages.start + 2);
        assert!(FullVerifier::verify_generic_evidence(&reversed).unwrap_err().message.contains("order"));
        let first_stage = program.store.extra[stages.start + 1] as usize;
        let mut opcode = program.store.clone();
        opcode.stages[first_stage] = FullStageTag::Where;
        assert!(FullVerifier::verify_generic_evidence(&opcode).unwrap_err().message.contains("selected authority"));
    });
}

#[test]
fn ground_stage_callback_preserves_exact_default_slot_and_call_opcode() {
    let source = "pure offset(value: Int, amount: Int = 2, extra: Int = 3) -> Int { value + amount + extra }\npure observed() -> List[Int] { [1, 2] |> map(offset) }\n";
    let (builder, sources) = ground_builder_with_source(source);
    let symbols = builder.solved.as_ref().unwrap().symbol_owner().clone();
    let program = symbols.clone().with_current(|| builder.finish(sources, symbols)).unwrap();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (_, proof) = generic.ground_stage_calls().next().unwrap();
        assert_eq!(proof.contract.supplied_slots.as_ref(), [0]);
        assert_eq!(proof.contract.default_slots.as_ref(), [1, 2]);
        let source = generic.stage_call_source(proof.source).unwrap();
        assert_eq!(program.store.tags[source.instruction as usize], FullTag::ExprCall);
        let words = program.store.payload(program.store.data[source.instruction as usize].range()).unwrap();
        let args = program.store.blocks[IrBlockId::from_raw(words[1]).unwrap().index()].instructions.bounds(program.store.extra.len()).unwrap();
        assert_eq!(program.store.extra[args.start + 3..args.end], [2, 1, 2, 2]);
        let mut wrong = program.store.clone();
        wrong.extra[args.start + 4] = 0;
        assert!(FullVerifier::verify_generic_evidence(&wrong).unwrap_err().message.contains("parameter slot"));
        let mut out_of_range = program.store.clone();
        out_of_range.extra[args.start + 4] = 3;
        assert!(FullVerifier::verify_generic_evidence(&out_of_range).is_err());
        let mut duplicate = program.store.clone();
        duplicate.extra[args.start + 6] = 1;
        assert!(FullVerifier::verify_generic_evidence(&duplicate).is_err());
        let mut fast = program.store.clone();
        fast.tags[source.instruction as usize] = FullTag::ExprDirectPureCall;
        assert!(FullVerifier::verify_generic_evidence(&fast).unwrap_err().message.contains("call omission"));
    });
}

#[test]
fn original_stage_callback_retains_prepared_call_after_frontend_drop() {
    let program = fixture();
    program.symbol_owner().with_current(|| {
        let callback = program.function_view(LoweredFunctionKey::Name(Name::intern("identity")), LoweredFunctionKind::Pure).unwrap().unwrap();
        let scope = callback.generic_scope().unwrap();
        let generic = program.generic_evidence().unwrap();
        let call = generic.calls().iter().find(|call| call.target == scope).expect("original stage callback has no prepared call proof");
        assert_eq!(generic.call_arguments(call.instruction).len(), 1);
        let argument = &generic.call_arguments(call.instruction)[0];
        assert_eq!(program.store.tags[argument.source_instruction.unwrap() as usize], FullTag::ExprParam);
        FullVerifier::verify(&program).unwrap();
    });
}

#[test]
fn original_stage_item_proof_rejects_changed_slot_input_owner_and_callback() {
    let program = fixture();
    program.symbol_owner().with_current(|| {
        let map = program.store.stages.iter().position(|tag| *tag == FullStageTag::Map).unwrap();
        let map_words = program.store.stage_data[map].range().bounds(program.store.extra.len()).unwrap();
        let callback = program.store.extra[map_words.start + 1];
        let source = program.generic_evidence().unwrap().call_arguments(callback)[0].source_instruction.unwrap() as usize;
        let pipeline = program.store.tags.iter().position(|tag| *tag == FullTag::ExprPipeline).unwrap();
        let pipeline_words = program.store.data[pipeline].range().bounds(program.store.extra.len()).unwrap();
        let input = program.store.extra[pipeline_words.start] as usize;
        let owners = program.store.generic_instruction_owners().unwrap();
        let mut slot = program.store.clone();
        slot.extra[map_words.start] += 1;
        assert!(FullVerifier::verify_generic_evidence(&slot).unwrap_err().message.contains("item slot"));
        let mut input_type = program.store.clone();
        let literal = input_type.tags.iter().enumerate().find(|(instruction, tag)| **tag == FullTag::ExprInt && owners[*instruction] == owners[input]).unwrap().0;
        input_type.tags[literal] = FullTag::ExprBool;
        assert!(FullVerifier::verify_generic_evidence(&input_type).is_err());
        let mut owner = program.store.clone();
        let foreign = owners.iter().enumerate().find(|(_, owner)| owner.is_some() && **owner != owners[pipeline]).unwrap().0;
        owner.extra[pipeline_words.start] = foreign as u32;
        assert!(FullVerifier::verify_generic_evidence(&owner).unwrap_err().message.contains("input has another owner"));
        let mut callback_source = program.store.clone();
        callback_source.tags[source] = FullTag::ExprInt;
        assert!(FullVerifier::verify_generic_evidence(&callback_source).unwrap_err().message.contains("original item parameter"));
        let mut callback_value = program.store.clone();
        callback_value.extra[map_words.start + 1] = source as u32;
        assert!(FullVerifier::verify_generic_evidence(&callback_value).is_err());
    });
}
