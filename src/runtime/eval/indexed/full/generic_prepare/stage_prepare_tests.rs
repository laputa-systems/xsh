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

#[test]
fn stage_pipeline_rejects_missing_foreign_and_jointly_rewritten_receipts() {
    let build = || {
        let (builder, sources) = ground_builder();
        let symbols = builder.solved.as_ref().unwrap().symbol_owner().clone();
        symbols.clone().with_current(|| builder.finish(sources, symbols)).unwrap()
    };
    let program = build();
    let foreign = build();
    program.symbol_owner().with_current(|| {
        let source = program.generic_evidence().unwrap().stage_pipelines().next().unwrap();
        FullVerifier::verify(&program).unwrap();
        let mut missing = program.store.clone();
        missing.generic.as_deref_mut().unwrap().test_remove_stage_pipelines();
        assert!(FullVerifier::verify_generic_evidence(&missing).is_err(), "original pipeline lost its independent receipt");
        let mut substituted = program.store.clone();
        substituted.generic.as_deref_mut().unwrap().test_replace_stage_pipelines(foreign.generic_evidence().unwrap());
        assert!(FullVerifier::verify_generic_evidence(&substituted).unwrap_err().message.contains("foreign program"));
        let mut rewritten = program.store.clone();
        let stage = &source.stages[0];
        rewritten.stages[stage.stage as usize] = FullStageTag::Where;
        let changed = rewritten.generic.as_deref_mut().unwrap().test_stage_pipeline_mut(source.instruction).unwrap();
        changed.stages[0].tag = FullStageTag::Where;
        changed.result = source.input_type;
        assert!(FullVerifier::verify_generic_evidence(&rewritten).is_err(), "coordinated stage and receipt edits replaced the original creation");
    });
}

#[test]
fn stage_pipeline_rejects_same_typed_foreign_input_before_publication() {
    let source = "pure identity(value: Int) -> Int { value }\npure observed() -> List[Int] { let first = [1, 2] |> map(identity); [3, 4] |> map(identity) }\n";
    let (mut builder, sources) = ground_builder_with_source(source);
    let symbols = builder.solved.as_ref().unwrap().symbol_owner().clone();
    symbols.clone().with_current(|| {
        let pipelines = builder.store.tags.iter().enumerate().filter_map(|(row, tag)| (*tag == FullTag::ExprPipeline).then_some(row)).collect::<Vec<_>>();
        assert_eq!(pipelines.len(), 2);
        let first = builder.store.data[pipelines[0]].range().bounds(builder.store.extra.len()).unwrap();
        let other = builder.store.data[pipelines[1]].range().bounds(builder.store.extra.len()).unwrap();
        builder.store.extra[first.start] = builder.store.extra[other.start];
        let error = builder.finish(sources, symbols).unwrap_err();
        assert_eq!(error.construct, "stage_pipeline_original_input_replaced");
    });
}

#[test]
fn stage_pipeline_requires_original_input_producer_relationship() {
    let (mut builder, sources) = ground_builder();
    let solved = Arc::get_mut(builder.solved.as_mut().unwrap()).expect("fixture retains one solved owner");
    solved.stage_operations.values_mut().next().unwrap().input_producer_flow = None;
    let symbols = solved.symbol_owner().clone();
    let error = symbols.clone().with_current(|| builder.finish(sources, symbols)).unwrap_err();
    assert_eq!(error.construct, "stage_pipeline_original_input_flow_missing");
}

#[test]
fn stage_pipeline_retains_original_native_producer_beneath_saved_configuration_arguments() {
    let source = "proc observed(root: Path) [fs, error] -> Int { fs.walk(root, gitignore: false) |> count() }\n";
    let (builder, sources) = ground_builder_with_source(source);
    let symbols = builder.solved.as_ref().unwrap().symbol_owner().clone();
    let program = symbols.clone().with_current(|| builder.finish(sources, symbols)).unwrap();
    program.symbol_owner().with_current(|| {
        let pipeline = program.generic_evidence().unwrap().stage_pipelines().next().unwrap();
        assert_ne!(pipeline.input, pipeline.input_source);
        assert!(!pipeline.input_wrappers.is_empty());
        assert_eq!(program.store.tags[pipeline.input as usize], FullTag::ExprMatch);
        FullVerifier::verify(&program).unwrap();
        let mut missing = program.store.clone();
        missing.generic.as_deref_mut().unwrap().test_remove_original_compiler_argument_wrappers();
        assert!(FullVerifier::verify_generic_evidence(&missing).is_err(), "pipeline input lost its original saved-argument authority");
    });
}

fn fold_callback_fixture() -> FullProgram {
    let source = "pure observed() -> Int { let counts = [\"one\", \"two\", \"one\"] |> fold(map.empty()) { |acc, item| acc.set(item, (acc.get(item) ?? 0) + 1) }; counts.get(\"one\") ?? 0 }\n";
    let (builder, sources) = ground_builder_with_source(source);
    let symbols = builder.solved.as_ref().unwrap().symbol_owner().clone();
    symbols.clone().with_current(|| builder.finish(sources, symbols)).unwrap()
}

#[test]
fn fold_callback_accumulator_and_item_ports_execute_after_frontend_disposal_on_both_routes() {
    super::super::tests::run_with_large_stack(|| {
        let program = Arc::new(fold_callback_fixture());
        program.symbol_owner().with_current(|| {
            let pipeline = program.generic_evidence().unwrap().stage_pipelines().next().unwrap();
            let fold = pipeline.stages[0].callback.as_ref().expect("original fold callback ports");
            assert_eq!(program.store.semantic.to_type(fold.types[0]).unwrap(), Type::Map(Box::new(Type::Str), Box::new(Type::Int)));
            assert_eq!(program.store.semantic.to_type(fold.types[1]).unwrap(), Type::Str);
            assert!(fold.reads.iter().any(|read| read.2 == 0));
            assert!(fold.reads.iter().any(|read| read.2 == 1));
            FullVerifier::verify(&program).unwrap();
        });
        for recursive in [false, true] {
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let name = program.symbol_owner().with_current(|| Name::intern("observed"));
            let mut call = || evaluator.call_indexed_direct(LoweredFunctionKey::Name(name), LoweredFunctionKind::Pure, &[], Span::at(program.store.source_id, 0)).expect("fold function exists");
            let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
            assert_eq!(result.unwrap(), crate::runtime::value::Value::Int(2));
        }
    });
}

#[test]
fn fold_callback_ports_refuse_changed_slots_callback_and_jointly_rewritten_receipts() {
    super::super::tests::run_with_large_stack(|| {
        let program = fold_callback_fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let pipeline = generic.stage_pipelines().next().unwrap();
            let stage = &pipeline.stages[0];
            let fold = stage.callback.as_ref().unwrap();
            let &(read, _, port) = fold.reads.iter().find(|read| read.2 == 0).unwrap();
            let mut slot = program.store.clone();
            let words = slot.data[read as usize].range().bounds(slot.extra.len()).unwrap();
            slot.extra[words.start] = fold.slots[1];
            assert!(FullVerifier::verify_generic_evidence(&slot).is_err(), "accumulator read crossed into the item port");
            let foreign_slot = generic.value_bindings().find(|(_, binding)| binding.contract.binding_type == fold.types[0] && binding.contract.slot != fold.slots[0]).expect("same typed outer map binding").1.contract.slot;
            let mut foreign = program.store.clone();
            let words = foreign.data[read as usize].range().bounds(foreign.extra.len()).unwrap();
            foreign.extra[words.start] = foreign_slot;
            assert!(FullVerifier::verify_generic_evidence(&foreign).is_err(), "same typed outer slot replaced the original accumulator port");
            let mut callback = program.store.clone();
            let words = callback.stage_data[stage.stage as usize].range().bounds(callback.extra.len()).unwrap();
            callback.extra[words.start + 4] = read;
            assert!(FullVerifier::verify_generic_evidence(&callback).is_err(), "another callback row replaced the original body");
            let mut rewritten = program.store.clone();
            let changed = rewritten.generic.as_deref_mut().unwrap().test_stage_pipeline_mut(pipeline.instruction).unwrap();
            changed.stages[0].callback.as_mut().unwrap().slots[port as usize] = fold.slots[1];
            assert!(FullVerifier::verify_generic_evidence(&rewritten).is_err(), "joint source port edits bypassed the original allocation");
            let mut missing = program.store.clone();
            let changed = missing.generic.as_deref_mut().unwrap().test_stage_pipeline_mut(pipeline.instruction).unwrap();
            changed.stages[0].callback = None;
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err(), "fold callback port proof disappeared");
        });
    });
}

fn native_item_callback_fixture(other: bool) -> FullProgram {
    let source = if other {
        "proc observed(root: Path, other: FsEntry) [fs, error] -> Int { let rows = fs.files(root, gitignore: false) |> map { |entry| let data = entry.path.read_bytes()?; data.len() }; rows |> sum }\n"
    } else {
        "proc observed(root: Path) [fs, error] -> Int { let rows = fs.files(root, gitignore: false) |> map { |entry| let data = entry.path.read_bytes()?; data.len() }; rows |> sum }\n"
    };
    let (builder, sources) = ground_builder_with_source(source);
    let symbols = builder.solved.as_ref().unwrap().symbol_owner().clone();
    symbols.clone().with_current(|| builder.finish(sources, symbols)).unwrap()
}

#[test]
fn native_fs_entry_callback_port_executes_after_frontend_disposal_on_both_routes() {
    super::super::tests::run_with_large_stack(|| {
        use std::os::unix::ffi::OsStrExt;
        let program = Arc::new(native_item_callback_fixture(false));
        program.symbol_owner().with_current(|| {
            let pipeline = program.generic_evidence().unwrap().stage_pipelines().find(|pipeline| pipeline.stages[0].callback.is_some()).unwrap();
            let callback = pipeline.stages[0].callback.as_ref().unwrap();
            assert_eq!(callback.slots.len(), 1);
            assert_eq!(program.store.semantic.to_type(callback.types[0]).unwrap(), crate::sema::records::standard_record_type("FsEntry").unwrap());
            assert_eq!(callback.parameters[0].unwrap().0, Name::intern("entry"));
            assert!(!callback.reads.is_empty());
            FullVerifier::verify(&program).unwrap();
        });
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("first"), b"one").unwrap();
        std::fs::write(directory.path().join("second"), b"four").unwrap();
        let argument = crate::runtime::value::Value::Path(crate::runtime::value::PathValue::new(directory.path().as_os_str().as_bytes().to_vec()).unwrap());
        for recursive in [false, true] {
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let name = program.symbol_owner().with_current(|| Name::intern("observed"));
            let mut call = || evaluator.call_indexed_direct(LoweredFunctionKey::Name(name), LoweredFunctionKind::Proc, std::slice::from_ref(&argument), Span::at(program.store.source_id, 0)).expect("native callback function exists");
            let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
            assert_eq!(result.unwrap(), crate::runtime::value::Value::Int(7));
        }
    });
}

#[test]
fn native_fs_entry_callback_port_refuses_missing_foreign_and_same_typed_slot_rewrites() {
    super::super::tests::run_with_large_stack(|| {
        let program = native_item_callback_fixture(true);
        let foreign_program = native_item_callback_fixture(true);
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let pipeline = generic.stage_pipelines().find(|pipeline| pipeline.stages[0].callback.is_some()).unwrap();
            let callback = pipeline.stages[0].callback.as_ref().unwrap();
            let &(read, _, _) = callback.reads.first().unwrap();
            let view = program.function_view(LoweredFunctionKey::Name(Name::intern("observed")), LoweredFunctionKind::Proc).unwrap().unwrap();
            let params = program.store.functions[view.index].params.bounds(program.store.params.len()).unwrap();
            assert_eq!(program.store.params[params.start + 1].type_id, callback.types[0].raw());
            let mut slot = program.store.clone();
            let words = slot.data[read as usize].range().bounds(slot.extra.len()).unwrap();
            slot.extra[words.start] = 1;
            assert!(FullVerifier::verify_generic_evidence(&slot).is_err(), "same typed outer FsEntry parameter replaced the original input port");
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_stage_pipeline_mut(pipeline.instruction).unwrap().stages[0].callback = None;
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err(), "original item callback receipt disappeared");
            let mut foreign = program.store.clone();
            foreign.generic.as_deref_mut().unwrap().test_replace_stage_pipelines(foreign_program.generic_evidence().unwrap());
            assert!(FullVerifier::verify_generic_evidence(&foreign).is_err(), "another program supplied the callback binding authority");
            let mut rewritten = program.store.clone();
            let changed = rewritten.generic.as_deref_mut().unwrap().test_stage_pipeline_mut(pipeline.instruction).unwrap();
            changed.stages[0].callback.as_mut().unwrap().slots[0] = 1;
            changed.stages[0].payload[0] = 1;
            let words = rewritten.stage_data[pipeline.stages[0].stage as usize].range().bounds(rewritten.extra.len()).unwrap();
            rewritten.extra[words.start] = 1;
            let words = rewritten.data[read as usize].range().bounds(rewritten.extra.len()).unwrap();
            rewritten.extra[words.start] = 1;
            assert!(FullVerifier::verify_generic_evidence(&rewritten).is_err(), "coordinated callback port rewrite changed its original allocation");
            let mut input = program.store.clone();
            let words = input.data[pipeline.instruction as usize].range().bounds(input.extra.len()).unwrap();
            input.extra[words.start] = read;
            assert!(FullVerifier::verify_generic_evidence(&input).is_err(), "an item read replaced the original native producer input");
        });
    });
}

fn structured_group_by_fixture() -> FullProgram {
    let (builder, sources) = ground_builder_with_source("pure observed() -> Int { [1, 2, 1] |> group-by { |item| item } |> map { |bucket| bucket.key + bucket.items.len() } |> sum }\n");
    let symbols = builder.solved.as_ref().unwrap().symbol_owner().clone();
    symbols.clone().with_current(|| builder.finish(sources, symbols)).unwrap()
}

#[test]
fn structured_group_by_bucket_projects_after_frontend_disposal_on_both_routes() {
    super::super::tests::run_with_large_stack(|| {
        let program = Arc::new(structured_group_by_fixture());
        program.symbol_owner().with_current(|| FullVerifier::verify(&program).unwrap());
        for recursive in [false, true] {
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let name = program.symbol_owner().with_current(|| Name::intern("observed"));
            let mut call = || evaluator.call_indexed_direct(LoweredFunctionKey::Name(name), LoweredFunctionKind::Pure, &[], Span::at(program.store.source_id, 0)).expect("grouped function exists");
            let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
            assert_eq!(result.unwrap(), crate::runtime::value::Value::Int(6));
        }
    });
}

#[test]
fn structured_group_by_result_row_refuses_missing_foreign_and_rewritten_stage_layouts() {
    super::super::tests::run_with_large_stack(|| {
        let program = structured_group_by_fixture();
        let foreign_program = structured_group_by_fixture();
        program.symbol_owner().with_current(|| {
            let pipeline = program.generic_evidence().unwrap().stage_pipelines().next().unwrap();
            let stage = &pipeline.stages[0];
            let layout = stage.result_record_layout.as_ref().expect("original selected structured result row");
            stage.verify_result_record_layout(&program.store.semantic).unwrap();
            let mut missing = stage.clone();
            missing.result_record_layout = None;
            assert!(missing.verify_result_record_layout(&program.store.semantic).is_err());
            let mut row = stage.clone();
            row.result_record_layout.as_mut().unwrap().record = stage.input;
            assert!(row.verify_result_record_layout(&program.store.semantic).is_err());
            let mut schema = stage.clone();
            schema.result_record_layout.as_mut().unwrap().schema = Arc::new(crate::runtime::eval::require::PreparedSchema::Validate(Type::Bool));
            assert!(schema.verify_result_record_layout(&program.store.semantic).is_err());
            let mut order = stage.clone();
            let crate::runtime::eval::require::PreparedSchema::Record(fields) = Arc::make_mut(&mut order.result_record_layout.as_mut().unwrap().schema) else { unreachable!() };
            fields.reverse();
            assert!(order.verify_result_record_layout(&program.store.semantic).is_err());
            let mut result = stage.clone();
            result.result = stage.input;
            assert!(result.verify_result_record_layout(&program.store.semantic).is_err());
            let mut lost = program.store.clone();
            lost.generic.as_deref_mut().unwrap().test_stage_pipeline_mut(pipeline.instruction).unwrap().stages[0].result_record_layout = None;
            assert!(FullVerifier::verify_generic_evidence(&lost).is_err());
            let mut foreign = program.store.clone();
            foreign.generic.as_deref_mut().unwrap().test_replace_stage_pipelines(foreign_program.generic_evidence().unwrap());
            assert!(FullVerifier::verify_generic_evidence(&foreign).is_err());
            let mut rewritten = program.store.clone();
            let changed = rewritten.generic.as_deref_mut().unwrap().test_stage_pipeline_mut(pipeline.instruction).unwrap();
            changed.stages[0].result = stage.input;
            changed.stages[0].result_record_layout.as_mut().unwrap().record = stage.callback.as_ref().unwrap().types[0];
            changed.stages[0].result_record_layout.as_mut().unwrap().schema = Arc::new(crate::runtime::eval::require::PreparedSchema::Validate(Type::Int));
            assert!(FullVerifier::verify_generic_evidence(&rewritten).is_err(), "coordinated result row edits bypassed original stage authority");
            let mut source = program.store.clone();
            source.generic.as_deref_mut().unwrap().test_stage_pipeline_mut(pipeline.instruction).unwrap().stages[0].origin = pipeline.stages[1].origin;
            assert!(FullVerifier::verify_generic_evidence(&source).is_err(), "another stage supplied the structured result row");
            let children = Arc::new(vec![crate::runtime::eval::LoweredValue::Int(1)]);
            let value = layout.materialize_item(crate::runtime::eval::LoweredValue::Record(Arc::new(BTreeMap::from([
                (Arc::from("items"), crate::runtime::eval::LoweredValue::SharedList(Arc::clone(&children))),
                (Arc::from("key"), crate::runtime::eval::LoweredValue::Int(1)),
            ]))), Span::at(program.store.source_id, 0)).unwrap();
            let crate::runtime::eval::LoweredValue::RecordVec(fields) = value else { panic!("generated stage row remains unordered"); };
            assert_eq!(fields.iter().map(|(name, _)| *name).collect::<Vec<_>>(), vec![Name::intern("items"), Name::intern("key")]);
            let crate::runtime::eval::LoweredValue::SharedList(retained) = &fields[0].1 else { panic!("stage ordering rebuilt the original item collection"); };
            assert!(Arc::ptr_eq(&children, retained));
        });
    });
}

fn fused_par_map_callback_fixture() -> FullProgram {
    let source = "enum Language { Unknown, Known }\nproc observed() [] -> Int { let counts = [{keep: true, language: Known, path: p\"missing\", rel: \"x\"}] |> par-map(jobs: 1) { |candidate| var amount = if candidate.language == Known { 1 } else { 0 }; amount = amount + 1; [{key: candidate.rel, value: amount}] } |> flat-map { |rows| rows } |> reduce-by(sum: true) { |row| row }; counts.get(\"x\") ?? 0 }\n";
    let (builder, sources) = ground_builder_with_source(source);
    let symbols = builder.solved.as_ref().unwrap().symbol_owner().clone();
    symbols.clone().with_current(|| builder.finish(sources, symbols)).unwrap()
}

fn fused_par_map_callback_outer_fixture() -> FullProgram {
    let source = "enum Language { Unknown, Known }\nproc observed() [] -> Int { let other = {keep: true, language: Known, path: p\"missing\", rel: \"x\"}; let counts = [other] |> par-map(jobs: 1) { |candidate| var amount = if candidate.language == Known { 1 } else { 0 }; amount = amount + 1; [{key: candidate.rel, value: amount}] } |> flat-map { |rows| rows } |> reduce-by(sum: true) { |row| row }; counts.get(\"x\") ?? 0 }\n";
    let (builder, sources) = ground_builder_with_source(source);
    let symbols = builder.solved.as_ref().unwrap().symbol_owner().clone();
    symbols.clone().with_current(|| builder.finish(sources, symbols)).unwrap()
}

#[test]
fn fused_par_map_original_candidate_port_executes_after_frontend_disposal_on_both_routes() {
    super::super::tests::run_with_large_stack(|| {
        let program = Arc::new(fused_par_map_callback_fixture());
        program.symbol_owner().with_current(|| FullVerifier::verify(&program).unwrap());
        for recursive in [false, true] {
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let name = program.symbol_owner().with_current(|| Name::intern("observed"));
            let mut call = || evaluator.call_indexed_direct(LoweredFunctionKey::Name(name), LoweredFunctionKind::Proc, &[], Span::at(program.store.source_id, 0)).expect("fused callback function exists");
            let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
            assert_eq!(result.unwrap(), crate::runtime::value::Value::Int(2));
        }
    });
}

#[test]
fn fused_par_map_original_composition_refuses_missing_foreign_erased_and_same_typed_port_rewrites() {
    super::super::tests::run_with_large_stack(|| {
        let program = fused_par_map_callback_outer_fixture();
        let foreign_program = fused_par_map_callback_outer_fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let pipeline = generic.stage_pipelines().next().unwrap();
            assert_eq!(pipeline.stages.len(), 3);
            assert_eq!(pipeline.block_payload.len(), 2);
            let first = &pipeline.stages[0];
            let callback = first.callback.as_ref().unwrap();
            let fusion = first.fusion.as_ref().unwrap();
            assert_eq!(fusion.stages.len(), 3);
            assert_eq!(fusion.identity_flat_map.as_ref().unwrap().stage, pipeline.stages[1].origin);
            assert!(matches!(fusion.identity_flat_map.as_ref().unwrap().read, super::super::super::generic::OperationSourceOrigin::Statement(_)));
            assert!(pipeline.stages[1].callback.is_none());
            assert!(pipeline.stages.iter().all(|stage| stage.stage == first.stage));
            let &(read, _, _) = callback.reads.first().unwrap();
            let outer = generic.value_bindings().find(|(_, binding)| binding.contract.binding_type == callback.types[0] && binding.contract.slot != callback.slots[0]).expect("original outer candidate record allocation").1.contract.slot;
            let mut slot = program.store.clone();
            let words = slot.data[read as usize].range().bounds(slot.extra.len()).unwrap();
            slot.extra[words.start] = outer;
            assert!(FullVerifier::verify_generic_evidence(&slot).is_err(), "same typed outer candidate replaced original fused callback port");
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_stage_pipeline_mut(pipeline.instruction).unwrap().stages[0].fusion = None;
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
            let mut foreign = program.store.clone();
            foreign.generic.as_deref_mut().unwrap().test_replace_stage_pipelines(foreign_program.generic_evidence().unwrap());
            assert!(FullVerifier::verify_generic_evidence(&foreign).is_err());
            let mut erased = program.store.clone();
            let changed = erased.generic.as_deref_mut().unwrap().test_stage_pipeline_mut(pipeline.instruction).unwrap();
            Arc::make_mut(changed.stages[0].fusion.as_mut().unwrap()).identity_flat_map = None;
            assert!(FullVerifier::verify_generic_evidence(&erased).is_err(), "fused worker lost the original erased identity callback");
            let mut read_origin = program.store.clone();
            let changed = read_origin.generic.as_deref_mut().unwrap().test_stage_pipeline_mut(pipeline.instruction).unwrap();
            Arc::make_mut(changed.stages[0].fusion.as_mut().unwrap()).identity_flat_map.as_mut().unwrap().read = super::super::super::generic::OperationSourceOrigin::Expression(pipeline.origin);
            assert!(FullVerifier::verify_generic_evidence(&read_origin).is_err(), "an expression replaced the original erased statement port");
            let mut origin = program.store.clone();
            let changed = origin.generic.as_deref_mut().unwrap().test_stage_pipeline_mut(pipeline.instruction).unwrap();
            Arc::make_mut(changed.stages[0].fusion.as_mut().unwrap()).stages[0] = pipeline.stages[2].origin;
            assert!(FullVerifier::verify_generic_evidence(&origin).is_err(), "another authored operation replaced the fused map origin");
            let mut rewritten = program.store.clone();
            let changed = rewritten.generic.as_deref_mut().unwrap().test_stage_pipeline_mut(pipeline.instruction).unwrap();
            changed.stages[0].callback.as_mut().unwrap().slots[0] = outer;
            for stage in changed.stages.iter_mut() { stage.payload[0] = outer; }
            let words = rewritten.stage_data[first.stage as usize].range().bounds(rewritten.extra.len()).unwrap();
            rewritten.extra[words.start] = outer;
            let words = rewritten.data[read as usize].range().bounds(rewritten.extra.len()).unwrap();
            rewritten.extra[words.start] = outer;
            assert!(FullVerifier::verify_generic_evidence(&rewritten).is_err(), "coordinated source and physical fused port edits bypassed original allocation");
        });
    });
}
