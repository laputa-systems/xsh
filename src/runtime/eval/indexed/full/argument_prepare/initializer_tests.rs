use super::*;

fn prepared(name: &str, source: &str) -> FullProgram {
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        name, crate::loader::entry_source_from_text(name, source.to_string()), Vec::new(),
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = sources.files().first().unwrap().id();
    let declarations = crate::sema::check::Checker::check_compact_declarations(&parsed.arena);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    let bodies = crate::sema::check::Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    let sources = Arc::new(sources);
    let prepared = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::clone(&sources), source_id);
    if prepared.as_ref().is_err_and(|error| error.construct == "full_ir_function_blocker") {
        let mut blockers = Vec::new();
        crate::runtime::eval::lower::lower_compact_function_units_into(&parsed.arena, &declarations, &bodies,
            source, &sources, crate::runtime::eval::lower::StdlibLowerLinkage::Local, |unit| {
                if unit.blocker.is_some() { blockers.push((unit.key, unit.blocker, unit.blocker_detail)); }
                Ok(())
            }).unwrap();
        panic!("initializer lowering blockers: {blockers:?}");
    }
    prepared.unwrap()
}

fn builtin_templates() -> FullProgram {
    prepared("initializer-builtin-templates.xsh", include_str!("../../../../../../tests/fixtures/frontend-indexed/builtin-templates.xsh"))
}

#[test]
fn saved_nested_arguments_keep_source_identity_and_validation_after_frontend_disposal() {
    crate::runtime::eval::run_eval(|| {
        let source = "pure accept(value: UInt) -> Int { value }\npure preserve(value: Int) -> Int { value }\npure checked(value: Int) -> Int { preserve(value: accept(value: value)) }\n";
        let program = Arc::new(prepared("checked-argument-lineage.xsh", source));
        program.symbol_owner().with_current(|| {
            FullVerifier::verify(&program).unwrap();
            let generic = program.generic_evidence().unwrap();
            let saved = generic.original_argument_bindings().find(|saved| !saved.initializer_wrappers.is_empty()).unwrap();
            assert_ne!(saved.initializer, saved.initializer_source_instruction);
            assert!(saved.initializer_wrappers.iter().any(|wrapper| matches!(wrapper.kind, ValueInitializerWrapperKind::CompilerArgument { .. })));
            let wrapper = saved.initializer_wrappers.first().unwrap();
            let mut changed = (*program).clone();
            let payload = changed.store.data[wrapper.instruction as usize].range();
            changed.store.extra[payload.start as usize] = wrapper.instruction;
            assert!(FullVerifier::verify(&changed).is_err(), "rewriting validation cannot change its original argument source");
            let mut changed_kind = (*program).clone();
            changed_kind.store.tags[wrapper.instruction as usize] = FullTag::ExprParam;
            assert!(FullVerifier::verify(&changed_kind).is_err());
            let function = LoweredFunctionKey::Name(Name::intern("checked"));
            for recursive in [false, true] {
                for value in [7, -1] {
                    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure,
                        &[crate::runtime::value::Value::Int(value)], Span::new(program.store.source_id, 0, 0)).expect("checked function exists");
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call);
                    if value >= 0 { assert_eq!(result.unwrap(), crate::runtime::value::Value::Int(value)); }
                    else { assert_eq!(result.unwrap_err().kind, "type-error"); }
                }
            }
        });
    });
}

#[test]
fn builtin_initializer_lineage_rejects_changed_compiler_argument_wrappers() {
    crate::runtime::eval::run_eval(|| {
        let program = builtin_templates();
        program.symbol_owner().with_current(|| {
            FullVerifier::verify(&program).unwrap();
            let generic = program.generic_evidence().unwrap();
            let wrappers = generic.value_bindings().flat_map(|(_, binding)| binding.contract.initializer_wrappers.iter()).collect::<Vec<_>>();
            for kind in [FullTag::ExprMatch] {
                let wrapper = wrappers.iter().find(|wrapper| program.store.tags[wrapper.instruction as usize] == kind)
                    .expect("builtin initializers retain saved argument wrappers");
                let mut changed = program.clone();
                let range = changed.store.data[wrapper.instruction as usize].range();
                let child = changed.store.extra[range.start as usize];
                changed.store.extra[range.start as usize] = wrapper.instruction;
                assert_ne!(child, wrapper.instruction);
                let error = FullVerifier::verify_value_bindings(&changed.store, changed.generic_evidence().unwrap()).unwrap_err();
                assert!(error.message.contains("wrapper"), "{}", error.message);
                assert!(FullVerifier::verify(&changed).is_err(), "changing an initializer wrapper must invalidate its prepared lineage");
                let mut changed_kind = program.clone();
                changed_kind.store.tags[wrapper.instruction as usize] = FullTag::ExprParam;
                assert!(FullVerifier::verify_value_bindings(&changed_kind.store, changed_kind.generic_evidence().unwrap()).is_err());
                assert!(FullVerifier::verify(&changed_kind).is_err());
            }
        });
    });
}

#[test]
fn nested_compiler_wrappers_retain_program_owned_receipts_without_original_expression_ids() {
    crate::runtime::eval::run_eval(|| {
        let source = "pure combine(left: Int, right: Int) -> Int { left + right }\npure saved() -> Int { let result = combine(right: 2, left: 7); result }\n";
        let program = prepared("nested-compiler-wrappers.xsh", source);
        let foreign = prepared("nested-compiler-wrappers.xsh", source);
        program.symbol_owner().with_current(|| {
            FullVerifier::verify(&program).unwrap();
            let generic = program.generic_evidence().unwrap();
            assert_eq!(generic.original_compiler_argument_wrappers().count(), 2);
            let wrapper = generic.original_compiler_argument_wrappers().next().unwrap();
            assert!(generic.registered_instruction_origin(wrapper.instruction, false).is_none());
            for saved in generic.original_argument_bindings() {
                assert!(generic.registered_instruction_origin(saved.instruction, false).is_none(), "a generated read cannot impersonate its initializer");
            }
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_original_compiler_argument_wrappers();
            assert!(FullVerifier::verify(&missing).is_err());
            let mut other_program = program.clone();
            other_program.store.generic.as_deref_mut().unwrap().test_replace_original_compiler_argument_wrappers(foreign.generic_evidence().unwrap());
            let error = FullVerifier::verify(&other_program).unwrap_err();
            assert!(error.message.contains("foreign program"), "{}", error.message);
            let mut changed = program.clone();
            changed.store.generic.as_deref_mut().unwrap().test_original_compiler_argument_wrapper_mut(wrapper.instruction).unwrap().body = wrapper.initializer;
            let error = FullVerifier::verify(&changed).unwrap_err();
            assert!(error.message.contains("original receipt"), "{}", error.message);
            let mut changed_selection = program.clone();
            let pattern = changed_selection.store.pattern_data[wrapper.pattern as usize].range();
            changed_selection.store.extra[pattern.start as usize] = wrapper.slot.wrapping_add(1);
            assert!(FullVerifier::verify(&changed_selection).is_err(), "a compiler receipt freezes its original binding slot");
            let mut changed_body = program.clone();
            let payload = changed_body.store.data[wrapper.instruction as usize].range();
            let block = IrBlockId::from_raw(changed_body.store.extra[payload.start as usize + 1]).unwrap();
            let arms = changed_body.store.blocks[block.index()].instructions;
            changed_body.store.extra[arms.start as usize + 3] = wrapper.initializer;
            assert!(FullVerifier::verify(&changed_body).is_err(), "a compiler receipt freezes its original execution body");
            let program = Arc::new(program.clone());
            let function = LoweredFunctionKey::Name(Name::intern("saved"));
            for recursive in [false, true] {
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure,
                    &[], Span::new(program.store.source_id, 0, 0)).expect("saved function exists");
                let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call);
                assert_eq!(result.unwrap(), crate::runtime::value::Value::Int(9));
            }
        });
    });
}

#[test]
fn optional_native_receiver_keeps_original_narrowing_and_lazy_arguments_on_both_routes() {
    crate::runtime::eval::run_eval(|| {
        let source = "pure skip() -> Map[Int]? { let absent: Map[Int]? = null; absent?.set(value: 1 / 0, key: \"absent\") }\npure present() -> Map[Int]? { let table: Map[Int] = {left: 1}; let optional: Map[Int]? = table; optional?.set(key: \"right\", value: 2) }\npure control() -> Int { 3 }\n";
        let program = Arc::new(prepared("optional-native-guard.xsh", source));
        let foreign = prepared("optional-native-guard.xsh", source);
        program.symbol_owner().with_current(|| {
            FullVerifier::verify(&program).unwrap();
            let generic = program.generic_evidence().unwrap();
            assert_eq!(generic.original_optional_receiver_guards().count(), 2);
            let guard = generic.original_optional_receiver_guards().next().unwrap();
            assert!(generic.registered_instruction_origin(guard.read, false).is_none(), "a narrowed compiler read retains no authored expression identity");
            let TypeRef::Ground(source_type) = guard.source_type else { panic!("optional carrier must be ground") };
            let TypeRef::Ground(success_type) = guard.success_type else { panic!("present receiver must be ground") };
            assert_eq!(program.store.semantic.to_type(source_type).unwrap(), Type::Optional(Box::new(program.store.semantic.to_type(success_type).unwrap())));
            let mut missing = (*program).clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_original_compiler_argument_wrappers();
            assert!(FullVerifier::verify(&missing).is_err());
            let mut foreign_receipt = (*program).clone();
            foreign_receipt.store.generic.as_deref_mut().unwrap().test_replace_original_compiler_argument_wrappers(foreign.generic_evidence().unwrap());
            assert!(FullVerifier::verify(&foreign_receipt).is_err());
            let mut changed_contract = (*program).clone();
            let receipt = changed_contract.store.generic.as_deref_mut().unwrap().test_original_compiler_argument_wrapper_mut(guard.wrapper).unwrap();
            receipt.optional_receiver_guard.as_mut().unwrap().source_type = guard.success_type;
            assert!(FullVerifier::verify(&changed_contract).is_err(), "a present receiver cannot replace its original optional carrier descriptor");
            let mut changed_branch = (*program).clone();
            let range = changed_branch.store.data[guard.wrapper as usize].range();
            let block = IrBlockId::from_raw(changed_branch.store.extra[range.start as usize + 1]).unwrap();
            let arms = changed_branch.store.blocks[block.index()].instructions;
            changed_branch.store.extra[arms.start as usize + 3] = guard.body;
            assert!(FullVerifier::verify(&changed_branch).is_err(), "the absent arm cannot execute the selected native call");
            let mut changed_slot = (*program).clone();
            let present_pattern = changed_slot.store.extra[arms.start as usize + 4];
            let pattern_range = changed_slot.store.pattern_data[present_pattern as usize].range();
            changed_slot.store.extra[pattern_range.start as usize] = guard.slot.wrapping_add(1);
            let read_range = changed_slot.store.data[guard.read as usize].range();
            changed_slot.store.extra[read_range.start as usize] = guard.slot.wrapping_add(1);
            assert!(FullVerifier::verify(&changed_slot).is_err(), "joint bind and read rewriting cannot change the original guarded slot");
            for recursive in [false, true] {
                for name in ["skip", "present", "control"] {
                    let function = LoweredFunctionKey::Name(Name::intern(name));
                    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0)).expect("guard fixture function exists");
                    let value = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call).unwrap();
                    match name {
                        "skip" => assert_eq!(value, crate::runtime::value::Value::Null),
                        "present" => {
                            let crate::runtime::value::Value::Map(map) = value else { panic!("present optional method must retain its Map result") };
                            assert_eq!(map.len(), 2);
                            assert!(map.values().any(|value| *value == crate::runtime::value::Value::Int(2)));
                        }
                        _ => assert_eq!(value, crate::runtime::value::Value::Int(3)),
                    }
                }
            }
        });
    });
}
