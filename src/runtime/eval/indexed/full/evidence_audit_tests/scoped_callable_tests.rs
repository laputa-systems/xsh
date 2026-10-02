use super::*;
use crate::runtime::eval::indexed::generic::{GenericEvidenceBuilder, RequirementWitness, UserInvocationAuthority};

const SOURCE: &str = r#"pure apply(callback, value) { callback(value) }
pure integer(operand: Int) -> Int { operand }
pure text(message: Str) -> Str { message }
proc selected_number() [] -> Int { apply(integer, 7) }
proc selected_text() [] -> Str { apply(text, "word") }
"#;

#[test]
fn scoped_callback_instances_preserve_original_source_and_concrete_signatures() {
    run_with_large_stack(|| {
        let program = fixture("scoped-callback-evidence.xsh", SOURCE);
        let _symbols = program.symbol_owner().enter();
        let evidence = program.generic_evidence().unwrap();
        let (source_id, source) = evidence.scoped_invocation_sources().next().unwrap();
        let instances = evidence.instances().filter(|(_, instance)| instance.scope == source.scope).collect::<Vec<_>>();
        assert_eq!(instances.len(), 2);
        let mut labels = std::collections::BTreeSet::new();
        for (instance_id, instance) in instances {
            let RequirementWitness::Invocation(id) = instance.requirements[source.requirement as usize] else { panic!("original callback obligation"); };
            let witness = evidence.scoped_invocation_witness(id).unwrap();
            assert_eq!(witness.source, source_id);
            assert_eq!(witness.binding.supplied_slots.as_ref(), [0]);
            assert!(witness.binding.default_slots.is_empty());
            labels.insert(program.store.semantic.signature_param(witness.signature, 0).unwrap().0);
            assert!(matches!(evidence.scoped_invocation_authority(source.instruction, instance_id).unwrap(), Some(UserInvocationAuthority::Scoped { source: actual_source, instance: actual_instance, witness: actual_witness }) if actual_source == source_id && actual_instance == instance_id && actual_witness == id));
        }
        assert_eq!(labels, [Name::intern("operand"), Name::intern("message")].into_iter().collect());
    });
}

#[test]
fn scoped_callback_cold_proof_rejects_coupled_source_rewrites_and_foreign_witnesses() {
    run_with_large_stack(|| {
        let original = fixture("scoped-callback-cold-corruption.xsh", SOURCE);
        let symbols = original.symbol_owner().clone();
        let _symbols = symbols.enter();
        let evidence = original.generic_evidence().unwrap();
        let (source_id, source) = evidence.scoped_invocation_sources().next().unwrap();
        let source = source.clone();
        let witnesses = evidence.instances().filter(|(_, instance)| instance.scope == source.scope).map(|(id, instance)| {
            let RequirementWitness::Invocation(witness) = instance.requirements[source.requirement as usize] else { panic!("callback witness"); };
            (id, witness)
        }).collect::<Vec<_>>();
        let mut changed = original.clone();
        let altered = Requirement::Invocation { callable: TypeRef::Rigid(0), arguments: Box::new([]), result: TypeRef::Rigid(0), domain: crate::sema::inference::CallableDomain::AnyCallable };
        let store = changed.store.generic.as_deref_mut().unwrap();
        store.test_scope_mut(source.scope).unwrap().requirements[source.requirement as usize] = altered.clone();
        store.test_scoped_invocation_source_mut(source_id).unwrap().expected = altered;
        assert_rejected_evidence(&changed, "rewriting both visible source and scope must not replace the original receipt");

        let mut changed = original.clone();
        let store = changed.store.generic.as_deref_mut().unwrap();
        let foreign_signature = store.scoped_invocation_witness(witnesses[1].1).unwrap().signature;
        store.test_scoped_invocation_witness_mut(witnesses[0].1).unwrap().signature = foreign_signature;
        assert_rejected_evidence(&changed, "an Int frame cannot borrow the Str callback signature");

        let mut changed = original.clone();
        changed.store.generic.as_deref_mut().unwrap().test_scoped_invocation_witness_mut(witnesses[0].1).unwrap().binding.supplied_slots[0] = 1;
        assert_rejected_evidence(&changed, "supplied slots must retain the actual signature's binding");

        let mut changed = original.clone();
        changed.store.generic.as_deref_mut().unwrap().test_remove_scoped_invocation_sources();
        assert_rejected_evidence(&changed, "a typed opcode cannot lose its original scope authority");

        let foreign = fixture("foreign-scoped-callback.xsh", SOURCE);
        let foreign_id = foreign.generic_evidence().unwrap().scoped_invocation_sources().next().unwrap().0;
        assert!(evidence.scoped_invocation_source(foreign_id).is_err());
    });
}

#[test]
fn scoped_callback_sources_and_witnesses_retire_with_builder_rewind() {
    run_with_large_stack(|| {
        let program = fixture("scoped-callback-rewind.xsh", SOURCE);
        let _symbols = program.symbol_owner().enter();
        let evidence = program.generic_evidence().unwrap();
        let (_, source) = evidence.scoped_invocation_sources().next().unwrap();
        let source = source.clone();
        let scope = evidence.scope(source.scope).unwrap();
        let instance = evidence.instances().find(|(_, instance)| instance.scope == source.scope).unwrap().1;
        let RequirementWitness::Invocation(id) = instance.requirements[source.requirement as usize] else { panic!("callback witness"); };
        let witness = evidence.scoped_invocation_witness(id).unwrap().clone();
        let mut builder = GenericEvidenceBuilder::default();
        let checkpoint = builder.checkpoint();
        let original_source = builder.add_scoped_invocation_source(source.clone()).unwrap();
        let mut first = witness.clone(); first.source = original_source;
        let original_witness = builder.add_scoped_invocation_witness(first).unwrap();
        let retained = builder.store_retained_bytes();
        builder.rewind(checkpoint).unwrap();
        let replacement_source = builder.add_scoped_invocation_source(source).unwrap();
        let mut second = witness; second.source = replacement_source;
        let replacement_witness = builder.add_scoped_invocation_witness(second).unwrap();
        assert_ne!(original_source, replacement_source);
        assert_ne!(original_witness, replacement_witness);
        assert!(builder.test_scoped_invocation_source(original_source).is_err());
        assert!(builder.test_scoped_invocation_witness(original_witness).is_err());
        assert!(builder.test_scoped_invocation_source(replacement_source).is_ok());
        assert!(builder.test_scoped_invocation_witness(replacement_witness).is_ok());
        assert_eq!(builder.store_retained_bytes(), retained, "replacement owns one source receipt and one witness payload");
        assert_eq!(scope.requirements.len(), 1);
    });
}

#[test]
fn scoped_forwarded_callback_proof_rejects_same_shaped_body_substitution() {
    run_with_large_stack(|| {
        let source = r#"pure apply(callback, value) { callback(value) }
pure other(callback, value) { callback(value) }
pure both(callback, value) { let _ = apply(callback, value); other(callback, value) }
pure integer(operand: Int) -> Int { operand }
proc selected() [] -> Int { both(integer, 7) }
"#;
        let original = fixture("forwarded-scoped-callback-originals.xsh", source);
        let symbols = original.symbol_owner().clone();
        let _symbols = symbols.enter();
        let evidence = original.generic_evidence().unwrap();
        let sources = evidence.scoped_invocation_sources().map(|(id, source)| (id, source.clone())).collect::<Vec<_>>();
        assert_eq!(sources.len(), 2);
        let bodies = sources.iter().map(|(_, source)| {
            let (instance_id, instance) = evidence.instances().find(|(_, instance)| instance.scope == source.scope).unwrap();
            let RequirementWitness::Invocation(witness) = instance.requirements[source.requirement as usize] else { panic!("original callback witness"); };
            (instance_id, source.requirement as usize, witness)
        }).collect::<Vec<_>>();
        assert_eq!(evidence.scoped_invocation_witness(bodies[0].2).unwrap().descriptor,
            evidence.scoped_invocation_witness(bodies[1].2).unwrap().descriptor);
        assert_ne!(sources[0].1.original_requirement, sources[1].1.original_requirement);

        let mut changed = original.clone();
        changed.store.generic.as_deref_mut().unwrap().test_scoped_invocation_instance_mut(bodies[0].0).unwrap().requirements[bodies[0].1] = RequirementWitness::Invocation(bodies[1].2);
        assert_rejected_evidence(&changed, "equivalent signatures cannot replace a different body's obligation");

        let mut changed = original.clone();
        let obligation = changed.store.generic.as_deref_mut().unwrap().test_scoped_invocation_source_mut(sources[0].0).unwrap().obligations.iter_mut().find(|obligation| obligation.ancestry.len() > 1).unwrap();
        obligation.ancestry[0] = sources[1].1.original_requirement;
        assert_rejected_evidence(&changed, "forwarding must retain the original immediate obligation ancestry");
    });
}
