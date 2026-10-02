use super::*;

const SOURCE: &str = r#"pure prefix(value, pattern) { value.starts_with(pattern) }
pure forwarded(value, pattern) { prefix(pattern: pattern, value: value) }
pure observed() -> Bool { forwarded("alpha", "al") and forwarded(b"beta", b"be") }
"#;

fn fixture(source: &str) -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "scoped-native-method.xsh", crate::loader::entry_source_from_text("scoped-native-method.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let checked = crate::sema::check::Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    for declaration in checked.solved.declarations.values() {
        let scheme = checked.solved.graph.scheme(declaration.scheme).unwrap();
        for (&origin, requirement) in scheme.requirement_origins.iter().zip(&scheme.requirements) {
            if let RequirementTemplate::Operation { family, call } = requirement {
                let operation = checked.solved.graph.operation_call(*call).unwrap();
                let candidates = checked.solved.graph.family(*family).unwrap().iter().map(|&candidate| checked.solved.operation_catalog.candidate(&checked.solved.graph, candidate).unwrap()).collect::<Vec<_>>();
                assert!(checked.solved.graph.candidate_evidence(origin).unwrap().is_none());
                assert!(operation.receiver.is_some());
                assert_eq!(operation.arguments.len(), 1);
                assert_eq!(candidates.len(), 2);
                assert!(candidates.iter().all(|candidate| matches!(candidate,
                    crate::sema::check::SolvedOperationAuthority::Registry(metadata)
                    if matches!((metadata.owner, metadata.operation),
                        (crate::sema::registry_graph::RegistryOwner::Method(crate::modules::signature::MethodReceiver::Str), RuntimeOp::TextStartsWith)
                        | (crate::sema::registry_graph::RegistryOwner::Method(crate::modules::signature::MethodReceiver::Bytes), RuntimeOp::BytesStartsWith)))));
            }
        }
    }
    let weak = Arc::downgrade(&checked.solved);
    let counters = checked.solved.graph.counters().clone();
    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
    evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
    assert_eq!(checked.solved.graph.counters(), &counters);
    drop(checked); drop(parsed);
    assert!(weak.upgrade().is_none());
    let program = evaluator.indexed_program.as_ref().unwrap().as_ref().clone();
    FullVerifier::verify(&program).unwrap();
    program
}

#[test]
fn canonical_str_bytes_parameter_methods_keep_definition_owned_requirements() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(fixture(SOURCE));
        let generic = program.generic_evidence().unwrap();
        let source = generic.scoped_native_method_sources().next().unwrap().1;
        assert_eq!(generic.scoped_native_method_sources().count(), 1);
        assert_eq!(source.obligations.len(), 2);
        assert_eq!(generic.native_callable_values().count(), 0);
        assert_eq!(generic.native_invocation_plans().count(), 0);
        for recursive in [false, true] {
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(program.clone());
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("observed")));
            let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0)).unwrap();
            let target = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("prefix")));
            let value = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(target, recursive, call);
            assert_eq!(value.unwrap(), crate::runtime::value::Value::Bool(true));
        }
    });
}

#[test]
fn scoped_native_method_refuses_same_domain_rewrites_and_missing_receipts() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture(SOURCE);
        let generic = program.generic_evidence().unwrap();
        let (source_id, source) = generic.scoped_native_method_sources().next().unwrap();
        let (instance_id, _) = generic.instances().find(|(_, instance)| instance.scope == source.scope).unwrap();
        assert!(generic.scoped_native_method_operation(&program.store.semantic, source.instruction, Some(instance_id)).unwrap().is_some());
        let mut changed = program.clone();
        let range = changed.store.data[source.instruction as usize].range().bounds(changed.store.extra.len()).unwrap();
        changed.store.extra[range.start] = source.arguments[0].instruction;
        assert!(FullVerifier::verify(&changed).is_err());
        assert!(FullVerifier::verify_scoped_native_method_instruction(&changed.store, changed.generic_evidence().unwrap(), source.instruction).is_err());
        changed.store.generic.as_deref_mut().unwrap().test_scoped_native_method_source_mut(source_id).unwrap().payload[0] = source.arguments[0].instruction;
        assert!(FullVerifier::verify(&changed).is_err());
        assert!(changed.generic_evidence().unwrap().scoped_native_method_operation(&changed.store.semantic, source.instruction, Some(instance_id)).is_err());
        let mut missing = program.clone();
        missing.store.generic.as_deref_mut().unwrap().test_remove_scoped_native_method_sources();
        assert!(FullVerifier::verify(&missing).is_err());
        assert!(missing.generic_evidence().unwrap().scoped_native_method_operation(&missing.store.semantic, source.instruction, Some(instance_id)).is_err());
        let foreign = fixture(SOURCE);
        let foreign_source = foreign.generic_evidence().unwrap().scoped_native_method_sources().next().unwrap().0;
        assert!(generic.scoped_native_method_source(foreign_source).is_err());
    });
}

#[test]
fn scoped_native_method_refuses_another_declaration_frame_and_original_body() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture(r#"pure prefix(value, pattern) { value.starts_with(pattern) }
pure sibling(value, pattern) { value.starts_with(pattern) }
pure observed() -> Bool { prefix("alpha", "al") and sibling("beta", "be") }
"#);
        let generic = program.generic_evidence().unwrap();
        let sources = generic.scoped_native_method_sources().collect::<Vec<_>>();
        assert_eq!(sources.len(), 2);
        let (first_id, first) = sources[0];
        let (second_id, second) = sources[1];
        let (first_instance, member) = generic.instances().find(|(_, instance)| instance.scope == first.scope).unwrap();
        let (second_instance, _) = generic.instances().find(|(_, instance)| instance.scope == second.scope).unwrap();
        assert!(generic.scoped_native_method_operation(&program.store.semantic, first.instruction, Some(first_instance)).unwrap().is_some());
        assert!(generic.scoped_native_method_operation(&program.store.semantic, first.instruction, Some(second_instance)).is_err());
        let RequirementWitness::NativeMethod(witness) = member.requirements[first.requirement as usize] else { panic!("original native method has its typed witness"); };
        assert_eq!(generic.scoped_native_method_witness(witness).unwrap().source, first_id);
        let mut changed = program.clone();
        changed.store.generic.as_deref_mut().unwrap().test_scoped_native_method_witness_mut(witness).unwrap().source = second_id;
        assert!(FullVerifier::verify(&changed).is_err());
        assert!(changed.generic_evidence().unwrap().scoped_native_method_operation(&changed.store.semantic, first.instruction, Some(first_instance)).is_err());
    });
}

#[test]
fn unused_scoped_native_method_preserves_pending_family_without_concrete_witnesses() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture("pure unused(value, pattern) { value.starts_with(pattern) }\n");
        let generic = program.generic_evidence().unwrap();
        assert_eq!(generic.scoped_native_method_sources().count(), 1);
        assert_eq!(generic.instances().count(), 0);
        let source = generic.scoped_native_method_sources().next().unwrap().1;
        assert_eq!(source.expected.candidates.len(), 2);
        assert_eq!(source.obligations.len(), 1);
        assert!(generic.scoped_native_method_operation(&program.store.semantic, source.instruction, None).is_err());
    });
}
