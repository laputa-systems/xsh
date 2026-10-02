use super::*;

const SOURCE: &str = "let encode = json.encode\nprint ${encode(42)?}\n";

#[test]
fn native_callable_cold_proof_rejects_rewritten_creation_and_invocation_authority() {
    run_with_large_stack(|| {
        let original = fixture("native-callable-cold-proof.xsh", SOURCE);
        let symbols = original.symbol_owner().clone();
        let _symbols = symbols.enter();
        let evidence = original.generic_evidence().unwrap();
        let (value_id, value) = evidence.native_callable_values().next().unwrap();
        let (plan_id, plan) = evidence.native_invocation_plans().next().unwrap();
        assert_eq!(plan.contract.callable, value_id);
        assert_eq!(plan.contract.call.binding.supplied_slots.as_ref(), [0]);
        assert_eq!(plan.contract.call.binding.default_slots.as_ref(), [1]);
        let TypeRef::Ground(actual) = plan.contract.call.arguments[0].ty else { panic!() };
        assert_eq!(original.store.semantic.to_type(actual).unwrap(), crate::sema::types::Type::Int);
        let formal = original.store.semantic.signature_param(value.contract.signature, 0).unwrap().1;
        assert_eq!(original.store.semantic.to_type(formal).unwrap(), crate::sema::types::Type::Any);

        let mut changed = original.clone();
        changed.store.generic.as_deref_mut().unwrap().test_native_callable_value_mut(value_id).unwrap().source.instruction = plan.source.instruction;
        assert_rejected_evidence(&changed, "native creation cannot move to an invocation opcode");

        let mut changed = original.clone();
        changed.store.generic.as_deref_mut().unwrap().test_native_invocation_plan_mut(plan_id).unwrap().contract.call.effects.creation = crate::sema::inference::EffectSet::FS;
        assert_rejected_evidence(&changed, "a visible native plan cannot change its protected effects");

        let mut changed = original.clone();
        let visible = changed.store.generic.as_deref_mut().unwrap();
        visible.test_native_callable_value_mut(value_id).unwrap().contract.input_eligibility = Box::new([]);
        visible.test_native_invocation_plan_mut(plan_id).unwrap().contract.call.binding.default_slots = Box::new([]);
        assert_rejected_evidence(&changed, "rewriting the visible creation and call cannot replace their original receipts");

        let mut changed = original.clone();
        changed.store.generic.as_deref_mut().unwrap().test_remove_native_invocation_plans();
        assert_rejected_evidence(&changed, "a native invocation cannot lose its prepared authority");

        let foreign = fixture("native-callable-foreign-proof.xsh", SOURCE);
        let foreign_evidence = foreign.generic_evidence().unwrap();
        let foreign_value = foreign_evidence.native_callable_values().next().unwrap().0;
        let foreign_plan = foreign_evidence.native_invocation_plans().next().unwrap().0;
        assert!(evidence.native_callable_value(foreign_value).is_err());
        assert!(evidence.native_invocation_plan(foreign_plan).is_err());
        assert!(evidence.validate_native_invocation(foreign_value, plan_id).is_err());
    });
}

#[test]
fn native_callable_encoded_packet_cannot_swap_operand_and_default_positions() {
    run_with_large_stack(|| {
        let original = fixture("native-callable-packet-corruption.xsh", SOURCE);
        let _symbols = original.symbol_owner().enter();
        let (_, plan) = original.generic_evidence().unwrap().native_invocation_plans().next().unwrap();
        let instruction = plan.source.instruction as usize;
        let mut changed = original.clone();
        let block = changed.store.payload(changed.store.data[instruction].range()).unwrap()[1];
        let block = changed.store.blocks[IrBlockId::from_raw(block).unwrap().index()].instructions;
        changed.store.extra[block.start as usize + 1] = 2;
        changed.store.extra[block.start as usize + 2] = 0;
        assert!(FullVerifier::verify(&changed).is_err(), "an original supplied operand cannot become a default omission");
    });
}

#[test]
fn native_callable_preparation_rejects_changed_original_result_and_lexical_caller() {
    run_with_large_stack(|| {
        for mutate_caller in [false, true] {
            let source = "pure unrelated() -> Int { 7 }\nlet encode = json.encode\nprint ${encode(42)?}\n";
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("native-original-source-corruption.xsh", crate::loader::entry_source_from_text("native-original-source-corruption.xsh", source.to_owned()), Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let source_id = sources.files().first().unwrap().id();
            let mut checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let solved = Arc::get_mut(&mut checked.solved).unwrap();
            if mutate_caller {
                let caller = *solved.declarations.keys().next().unwrap();
                solved.registry_references.values_mut().next().unwrap().caller = Some(caller);
            } else {
                let (&expression, invocation) = solved.invocations.iter().next().unwrap();
                let evidence = solved.graph.invocation_evidence(invocation.requirement).unwrap().unwrap();
                let actual = solved.graph.candidate_evidence(evidence.native_alternatives[0].operation).unwrap().unwrap().actual_arguments[0].unwrap();
                solved.expressions.insert(expression, actual);
            }
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
            assert!(evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).is_err(), "a source result or lexical caller cannot be replaced before publishing native authority");
        }
    });
}
