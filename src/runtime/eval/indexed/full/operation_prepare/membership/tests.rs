use super::*;
use crate::runtime::eval::Evaluator;
use crate::runtime::value::Value;

const SOURCE: &str = "pure identity(value) { value }\npure typed(value: Bool) -> Bool { value }\npure text(needle: Str, container: Str) -> Bool { identity(typed(needle in container)) }\npure bytes(needle: Bytes, container: Bytes) -> Bool { typed(identity(needle not in container)) }\n";

// Host fixtures can dispose the frontend and mutate sealed executable receipts.
fn build_source(source: &str) -> FullProgram {
    use crate::sema::check::Checker;
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
        "original-membership.xsh", crate::loader::entry_source_from_text("original-membership.xsh", source.to_owned()), Vec::new());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let source_id = SourceMap::files(&sources).first().unwrap().id();
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    assert!(bodies.solved.operations.values().any(|operation| {
        let graph = &bodies.solved.graph;
        graph.candidate_evidence(operation.requirement).unwrap().is_some_and(|selected| matches!(
            bodies.solved.operation_catalog.candidate(graph, selected.candidate).unwrap(),
            crate::sema::check::SolvedOperationAuthority::Language(metadata) if matches!(metadata.operation, PreparedLanguageOperation::Membership { .. })))
    }));
    let weak = Arc::downgrade(&bodies.solved);
    let counters = bodies.solved.graph.counters().clone();
    let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id);
    assert_eq!(&counters, bodies.solved.graph.counters());
    drop(parsed); drop(declarations); drop(bodies);
    assert!(weak.upgrade().is_none());
    program.unwrap()
}

fn fixture() -> FullProgram { build_source(SOURCE) }

#[test]
fn original_membership_results_flow_through_typed_and_generic_callers_after_frontend_disposal_on_both_routes() {
    crate::runtime::eval::run_eval(|| {
        let program = Arc::new(fixture());
        program.symbol_owner().with_current(|| {
            FullVerifier::verify(&program).unwrap();
            assert_eq!(program.generic_evidence().unwrap().operations().filter(|(_, operation)| matches!(operation.authority,
                PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Membership { .. }, .. })).count(), 2);
            for recursive in [false, true] {
                for (name, arguments, expected) in [
                    ("text", [Value::Str(Arc::from("bc")), Value::Str(Arc::from("abcd"))], true),
                    ("text", [Value::Str(Arc::from("z")), Value::Str(Arc::from("abcd"))], false),
                    ("bytes", [Value::Bytes(b"bc".to_vec()), Value::Bytes(b"abcd".to_vec())], false),
                    ("bytes", [Value::Bytes(b"z".to_vec()), Value::Bytes(b"abcd".to_vec())], true),
                ] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let function = LoweredFunctionKey::Name(Name::intern(name));
                    let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure,
                        &arguments, Span::new(program.store.source_id, 0, 0)).expect("membership function exists");
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call);
                    assert_eq!(result.unwrap(), Value::Bool(expected));
                }
            }
        });
    });
}

#[test]
fn original_membership_preserves_all_closed_container_and_needle_domains() {
    crate::runtime::eval::run_eval(|| {
    let mut source = String::from("pure identity(value) { value }\n");
    for operator in ["in", "not in"] {
        let suffix = if operator == "in" { "present" } else { "absent" };
        for (name, needle, container) in [
            ("list", "Int", "List[Int]"), ("map", "Int", "Map[Int, Str]"),
            ("text", "Str", "Str"), ("bytes", "Bytes", "Bytes"),
            ("record", "Str", "Record"), ("row", "Str", "{left: Int}"),
            ("path_text", "Str", "Path"), ("path_path", "Path", "Path"),
        ] {
            source.push_str(&format!("pure {name}_{suffix}(needle: {needle}, container: {container}) -> Bool {{ identity(needle {operator} container) }}\n"));
        }
        source.push_str(&format!("proc env_paths_{suffix}(needle: Path) [env] -> Bool {{ identity(needle {operator} env.PATH) }}\n"));
    }
    let program = Arc::new(build_source(&source));
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        assert_eq!(generic.operations().filter(|(_, operation)| matches!(operation.authority,
            PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Membership { .. }, .. })).count(), 18);
        for (_, operation) in generic.operations() {
            if !matches!(operation.authority, PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Membership { .. }, .. }) { continue; }
            let source = generic.operation_source(operation.source).unwrap();
            let words = program.store.payload(program.store.data[source.instruction as usize].range()).unwrap();
            assert_eq!(&words[1..3], operation.binding.operands.as_ref());
            assert!(operation.receiver.is_some());
            assert_eq!(operation.arguments.len(), 1);
        }
        let path = |bytes: &[u8]| Value::Path(crate::runtime::value::PathValue::new(bytes.to_vec()).unwrap());
        let cases = [
            ("list", [Value::Int(3), Value::List(vec![Value::Int(2), Value::Int(3)])]),
            ("map", [Value::Int(3), Value::Map(std::collections::BTreeMap::from([(crate::map_key::MapKey::Int(3), Value::Str(Arc::from("three")))]))]),
            ("text", [Value::Str(Arc::from("bc")), Value::Str(Arc::from("abcd"))]),
            ("bytes", [Value::Bytes(b"bc".to_vec()), Value::Bytes(b"abcd".to_vec())]),
            ("record", [Value::Str(Arc::from("left")), Value::Record(crate::runtime::value::RecordMap::from([(Arc::from("left"), Value::Null)]))]),
            ("row", [Value::Str(Arc::from("left")), Value::Record(crate::runtime::value::RecordMap::from([(Arc::from("left"), Value::Int(3))]))]),
            ("path_text", [Value::Str(Arc::from("bin")), path(b"/usr/bin")]),
            ("path_path", [path(b"bin"), path(b"/usr/bin")]),
        ];
        for recursive in [false, true] {
            for suffix in ["present", "absent"] {
                for (name, arguments) in &cases {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let function = LoweredFunctionKey::Name(Name::intern(&format!("{name}_{suffix}")));
                    let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure,
                        arguments, Span::new(program.store.source_id, 0, 0)).expect("closed membership function exists");
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call);
                    assert_eq!(result.unwrap(), Value::Bool(suffix == "present"));
                }
            }
        }
    });
    });
}

#[test]
fn original_membership_refuses_missing_foreign_and_joint_same_type_rewrites() {
    crate::runtime::eval::run_eval(|| {
        let program = fixture();
        let foreign = fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, operation) = generic.operations().find(|(_, operation)| matches!(operation.authority,
                PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Membership { domain: MembershipDomain::Str, .. }, .. })).unwrap();
            let source = generic.operation_source(operation.source).unwrap();
            let range = program.store.data[source.instruction as usize].range();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_operations();
            assert!(FullVerifier::verify(&missing).is_err());
            let mut foreign_source = program.clone();
            foreign_source.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().source = foreign.generic_evidence().unwrap().operations().next().unwrap().1.source;
            assert!(FullVerifier::verify(&foreign_source).is_err());
            let mut swapped = program.clone();
            swapped.store.extra.swap(range.start as usize + 1, range.start as usize + 2);
            swapped.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().binding.operands.swap(0, 1);
            assert!(FullVerifier::verify(&swapped).unwrap_err().message.contains("original receipt"));
            let mut changed_operator = program.clone();
            let index = changed_operator.store.extra[range.start as usize] as usize;
            changed_operator.store.binary_ops[index] = BinaryOp::NotIn;
            let evidence = changed_operator.store.generic.as_deref_mut().unwrap();
            let PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Membership { negated, .. }, .. } = &mut evidence.test_operation_mut(id).unwrap().authority else { unreachable!() };
            *negated = true;
            let PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Membership { negated, .. }, .. } = &mut evidence.test_operation_source_mut(operation.source).unwrap().expected else { unreachable!() };
            *negated = true;
            assert!(FullVerifier::verify(&changed_operator).is_err());
            let mut wrong_result = program.clone();
            wrong_result.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().result = operation.receiver.unwrap();
            assert!(FullVerifier::verify(&wrong_result).is_err());
            let mut wrong_owner = program.clone();
            wrong_owner.store.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().owner = InstructionOwner::Driver(0);
            assert!(FullVerifier::verify(&wrong_owner).is_err());
            let mut wrong_location = program.clone();
            let words = wrong_location.store.payload(range).unwrap();
            let location = IrLocationId::from_raw(words[3]).unwrap().index();
            wrong_location.store.location_sources[location] = SourceId::new(999);
            assert!(FullVerifier::verify(&wrong_location).is_err());
            let mut wrong_span = program.clone();
            wrong_span.store.locations[location].start += 1;
            assert!(FullVerifier::verify_generic_evidence(&wrong_span.store).is_err());
        });
    });
}

#[test]
fn original_unsigned_map_membership_keeps_the_original_key_validation_and_material_source() {
    crate::runtime::eval::run_eval(|| {
        let source = "pure identity(value) { value }\npure present(needle: Int, container: Map[UInt, Str]) -> Bool { identity(needle in container) }\npure absent(needle: UInt, container: Map[UInt, Str]) -> Bool { identity(needle not in container) }\n";
        let program = Arc::new(build_source(source));
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, operation) = generic.operations().find(|(_, operation)| matches!(operation.authority,
                PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Membership { negated: false, domain: MembershipDomain::Map }, .. })).unwrap();
            let original = operation.membership_lowering.as_ref().unwrap();
            let guard = original.uint_key.as_ref().unwrap();
            assert_ne!(original.needle_instruction, operation.binding.operands[0]);
            let mut replaced_material = (*program).clone();
            let range = replaced_material.store.data[guard.require_instruction as usize].range();
            replaced_material.store.extra[range.start as usize] = operation.binding.operands[1];
            assert!(FullVerifier::verify(&replaced_material).is_err());
            let mut removed_validation = (*program).clone();
            let instruction = generic.operation_source(operation.source).unwrap().instruction;
            let range = removed_validation.store.data[instruction as usize].range();
            removed_validation.store.extra[range.start as usize + 1] = original.needle_instruction;
            removed_validation.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().binding.operands[0] = original.needle_instruction;
            assert!(FullVerifier::verify(&removed_validation).is_err());
            for recursive in [false, true] {
                for (name, key, expected) in [("present", 3, true), ("present", 4, false), ("absent", 3, false), ("absent", 4, true)] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let arguments = [Value::Int(key), Value::Map(std::collections::BTreeMap::from([(crate::map_key::MapKey::Int(3), Value::Str(Arc::from("three")))]))];
                    let function = LoweredFunctionKey::Name(Name::intern(name));
                    let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure, &arguments,
                        Span::new(program.store.source_id, 0, 0)).expect("unsigned membership function exists");
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call);
                    assert_eq!(result.unwrap(), Value::Bool(expected));
                }
            }
        });
    });
}
