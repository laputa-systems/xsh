use super::*;
use crate::sema::operation_graph::PreparedLanguageOperation;
use crate::runtime::eval::Evaluator;
use crate::runtime::value::Value;
use crate::map_key::MapKey;

fn on_large_stack(work: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(work).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}

fn source_fixture(source: &str, expected: PreparedLanguageOperation) -> FullProgram {
    let source = source.to_owned();
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(move ||
        super::super::operation_prepare::tests::source_fixture(&source, expected)
    ).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload))
}

fn wire_operand_program() -> FullProgram {
    source_fixture("enum WireState: Str { Ready = \"ready\", Empty = \"\" }\nconst saved = Ready\npure direct() -> Result[Str] { json.encode(Ready) }\npure constant() -> Result[Str] { json.encode(saved) }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_wire_operands_preserve_nominal_descriptors_after_frontend_drop() {
    on_large_stack(|| {
    let program = Arc::new(wire_operand_program());
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let proofs = generic.ground_native_calls().collect::<Vec<_>>();
        assert_eq!(proofs.len(), 2);
        for (_, proof) in proofs {
            assert_eq!(proof.contract.argument_relations[0], crate::sema::inference::ArgumentRelation::DeclaredErasure);
            assert_eq!(proof.contract.input_eligibility.as_ref(), &[(0, crate::sema::inference::Eligibility::JsonCompatible)]);
            let TypeRef::Ground(actual) = proof.contract.arguments[0].ty else { panic!("closed nominal operand") };
            assert_eq!(program.store.semantic.to_type(actual).unwrap(), Type::Tag(Name::intern("WireState")));
            let (_, formal, _) = program.store.semantic.signature_param(proof.contract.signature, 0).unwrap();
            assert_eq!(program.store.semantic.to_type(formal).unwrap(), Type::Any);
        }
    });
    for recursive in [false, true] {
        for name in ["direct", "constant"] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let name = program.symbol_owner().with_current(|| Name::intern(name));
            let mut call = || evaluator.call_indexed_direct(LoweredFunctionKey::Name(name), LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0)).expect("wire producer exists");
            let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(name), recursive, call);
            assert_eq!(result.unwrap(), Value::ok(Value::Str(Arc::from("\"ready\""))));
        }
    }
    });
}

#[test]
fn direct_native_wire_operands_reject_foreign_nominal_producers() {
    on_large_stack(|| {
    let program = wire_operand_program();
    program.symbol_owner().with_current(|| {
        let mut constant = program.store.clone();
        let value = constant.prepared_constants.iter_mut().find_map(|value| match &mut value.0 { LoweredValue::Tag(tag) => Some(tag), _ => None }).unwrap();
        value.type_name = Name::intern("Foreign.WireState");
        assert!(FullVerifier::verify_generic_evidence(&constant).is_err(), "a constant retains the same concrete nominal obligation as a direct constructor");
        let mut contradictory = program.store.clone();
        let value = contradictory.prepared_constants.iter_mut().find_map(|value| match &mut value.0 { LoweredValue::Tag(tag) => Some(tag), _ => None }).unwrap();
        *Arc::make_mut(value.wire.as_mut().unwrap()).variants.values_mut().next().unwrap() = Arc::from("forged");
        assert!(FullVerifier::verify_generic_evidence(&contradictory).is_err(), "native admission keeps the canonical mapping independently of a rewritten constant");
        let mut missing_wire = program.store.clone();
        missing_wire.wire_enums.clear();
        assert!(FullVerifier::verify_generic_evidence(&missing_wire).is_err(), "native admission requires the canonical wire authority before execution");
    });
    });
}

fn wire_method_program() -> FullProgram {
    source_fixture("enum WireState: Str { Ready = \"ready\", Empty = \"\" }\npure values(raw: Any) -> Result[Str] { let checked = raw.require(Map[UInt, WireState])?; json.encode(checked.values()) }\npure other_values(raw: Any) -> Result[Str] { let checked = raw.require(Map[UInt, WireState])?; json.encode(checked.values()) }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_map_values_keeps_original_receiver_and_executes_both_routes() {
    on_large_stack(|| {
    let program = Arc::new(wire_method_program());
    program.symbol_owner().with_current(|| {
        let (_, proof) = program.generic_evidence().unwrap().ground_native_calls().find(|(_, proof)| proof.contract.receiver.is_some()).unwrap();
        let receiver = proof.contract.receiver.as_ref().unwrap();
        let TypeRef::Ground(ty) = receiver.ty else { panic!("closed map receiver") };
        let item = Type::Tag(Name::intern("WireState"));
        assert_eq!(program.store.semantic.to_type(ty).unwrap(), Type::Map(Box::new(Type::UInt), Box::new(item.clone())));
        let TypeRef::Ground(result) = proof.contract.result else { panic!("closed method result") };
        assert_eq!(program.store.semantic.to_type(result).unwrap(), Type::List(Box::new(item)));
        assert!(proof.contract.arguments.is_empty());
        assert_eq!(proof.contract.argument_sources.as_ref(), &[Some(receiver.instruction)]);
        assert!(matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::MapValues, .. }));
    });
    for recursive in [false, true] {
        for (key, wire, expected) in [(1, "ready", Some("[\"ready\"]")), (-1, "ready", None), (1, "unknown", None)] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let name = program.symbol_owner().with_current(|| Name::intern("values"));
            let arguments = [Value::Map(BTreeMap::from([(MapKey::Int(key), Value::Str(Arc::from(wire)))]))];
            let mut call = || evaluator.call_indexed_direct(LoweredFunctionKey::Name(name), LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).expect("wire method exists");
            let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(name), recursive, call);
            let result = result.unwrap();
            if let Some(expected) = expected { assert_eq!(result, Value::ok(Value::Str(Arc::from(expected)))); }
            else { assert!(matches!(result, Value::Result(crate::runtime::value::ResultValue::Err(_))), "invalid keys or enum values must fail validation before method execution"); }
        }
    }
    });
}

#[test]
fn direct_native_map_values_refuses_foreign_receiver_and_schema() {
    on_large_stack(|| {
    let program = wire_method_program();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let methods = generic.ground_native_calls().filter(|(_, proof)| proof.contract.receiver.is_some()).collect::<Vec<_>>();
        let (id, proof) = methods[0];
        let source = generic.native_call_source(proof.source).unwrap();
        let foreign = methods[1].1.contract.receiver.as_ref().unwrap();
        let mut receiver = program.store.clone();
        let range = receiver.data[source.instruction as usize].range();
        receiver.extra[range.start as usize] = foreign.instruction;
        assert!(FullVerifier::verify_generic_evidence(&receiver).is_err(), "a same-typed receiver from another body cannot replace the original receiver");
        let evidence = receiver.generic.as_deref_mut().unwrap();
        evidence.test_ground_native_call_mut(id).unwrap().contract.receiver = Some(foreign.clone());
        evidence.test_ground_native_call_mut(id).unwrap().contract.argument_sources[0] = Some(foreign.instruction);
        let rewritten = evidence.ground_native_call(id).unwrap().contract.clone();
        evidence.test_native_call_source_mut(proof.source).unwrap().expected = rewritten;
        assert!(FullVerifier::verify_generic_evidence(&receiver).is_err(), "rewritten copies cannot replace the protected original receiver receipt");
        let mut schema = program.clone();
        schema.store.prepared_schemas[0] = Arc::new(super::super::super::super::require::PreparedSchema::Validate(Type::Int));
        assert!(FullVerifier::verify(&schema).is_err(), "a native receiver cannot acquire its map type from a contradictory schema");
    });
    });
}

fn container_method_program() -> FullProgram {
    source_fixture("pure read(table: Map[Str, List[Int]], key: Str) -> Result[List[Int]] { table.get(key) }\npure update(table: Map[Str, List[Int]], key: Str, item: List[Int]) -> Map[Str, List[Int]] { table.set(key, item) }\npure list_read(items: List[List[Int]], index: Int) -> Result[List[Int]] { items.get(index) }\npure append(items: List[List[Int]], item: List[Int]) -> List[List[Int]] { items.push(item) }\npure lengths(table: Map[Str, List[Int]], items: List[List[Int]]) -> Int { table.len() + items.len() }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

fn native_result_record_program() -> FullProgram {
    source_fixture("proc observed() [fs, error] -> Result[Bool] { let root = fs.tempdir()?; defer root.close()?; root.write(p\"data\", \"payload\")?; let report = root.read_result(p\"data\", max_bytes: 1)?; (report.truncated) }\nproc invalid() [fs, error] -> Result[Bool] { let root = fs.tempdir()?; defer root.close()?; let report = root.read_result(p\"data\", max_bytes: -1)?; (report.truncated) }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_result_record_materializes_only_success_before_numeric_projection_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(native_result_record_program());
        assert_eq!(program.generic_evidence().unwrap().native_call_sources().filter(|(_, source)| source.result_record_layout.is_some()).count(), 2);
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                for name in ["observed", "invalid"] {
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, &[], Span::new(program.store.source_id, 0, 0)).unwrap();
                    let actual = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap();
                    if name == "observed" { assert_eq!(actual, Value::ok(Value::Bool(true))); }
                    else { assert!(matches!(actual, Value::Result(crate::runtime::value::ResultValue::Err(_)))); }
                }
            });
        }
    });
}

#[test]
fn direct_native_result_record_rejects_missing_and_rewritten_canonical_success_layouts() {
    on_large_stack(|| {
        let program = native_result_record_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, source) = generic.native_call_sources().find(|(_, source)| source.result_record_layout.is_some()).unwrap();
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_native_call_source_mut(id).unwrap().result_record_layout = None;
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
            let mut wrong = program.store.clone();
            wrong.generic.as_deref_mut().unwrap().test_native_call_source_mut(id).unwrap().result_record_layout.as_mut().unwrap().schema = Arc::new(crate::runtime::eval::require::PreparedSchema::Validate(Type::Bool));
            assert!(FullVerifier::verify_generic_evidence(&wrong).is_err());
            let evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            let layout = source.result_record_layout.as_ref().unwrap();
            let empty = LoweredValue::ResultOk(Box::new(LoweredValue::Record(Arc::new(BTreeMap::new()))));
            assert!(layout.materialize_result(&evaluator, empty, Span::new(program.store.source_id, 0, 0)).is_err(), "a native host record cannot invent missing canonical fields");
        });
    });
}

fn optional_receiver_program() -> FullProgram {
    source_fixture("pure guarded(table: Map[Str, Int]?) -> Map[Str, Int]? { table?.set(value: 3, key: \"left\") }\npure other_guarded(table: Map[Str, Int]?) -> Map[Str, Int]? { table?.set(value: 3, key: \"left\") }\npure direct_guarded(table: Map[Str, Int]?) -> Map[Str, Int]? { table?.set(\"left\", 3) }\npure lazy_guarded(table: Map[Str, Int]?, divisor: Int) -> Map[Str, Int]? { table?.set(value: 10 / divisor, key: \"left\") }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_optional_receiver_keeps_original_carrier_and_lazy_arguments_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(optional_receiver_program());
        assert_eq!(program.generic_evidence().unwrap().original_optional_receiver_guards().count(), 4);
        let original = Value::Map(BTreeMap::from([(MapKey::Str(Arc::from("right")), Value::Int(4))]));
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                for name in ["guarded", "direct_guarded", "lazy_guarded"] {
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    for present in [false, true] {
                        let mut arguments = vec![if present { original.clone() } else { Value::Null }];
                        if name == "lazy_guarded" { arguments.push(Value::Int(if present { 2 } else { 0 })); }
                        let expected = if present { Value::Map(BTreeMap::from([(MapKey::Str(Arc::from("right")), Value::Int(4)), (MapKey::Str(Arc::from("left")), Value::Int(if name == "lazy_guarded" { 5 } else { 3 }))])) } else { Value::Null };
                        let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                        assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), expected);
                    }
                }
            });
        }
    });
}

#[test]
fn direct_native_optional_receiver_rejects_foreign_carrier_and_joint_guard_rewrites() {
    on_large_stack(|| {
        let program = optional_receiver_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let guards = generic.original_optional_receiver_guards().collect::<Vec<_>>();
            let guard = guards[0];
            let foreign = guards.iter().find(|candidate| candidate.owner != guard.owner).unwrap();
            let mut changed = program.store.clone();
            let range = changed.data[guard.wrapper as usize].range();
            changed.extra[range.start as usize] = foreign.carrier;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err());
            let receipt = changed.generic.as_deref_mut().unwrap().test_original_compiler_argument_wrapper_mut(guard.wrapper).unwrap();
            receipt.initializer = foreign.carrier;
            receipt.payload[0] = foreign.carrier;
            receipt.optional_receiver_guard.as_mut().unwrap().carrier = foreign.carrier;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "rewritten guard and physical carrier cannot replace original optional receiver authority");
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_remove_original_compiler_argument_wrappers();
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
        });
    });
}

fn byte_at_program() -> FullProgram {
    source_fixture("pure byte(text: Str, index: Int) -> Int? { text.byte_at(index) }\npure other_byte(text: Str, index: Int) -> Int? { text.byte_at(index) }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_byte_at_keeps_original_receiver_and_index_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(byte_at_program());
        assert_eq!(program.generic_evidence().unwrap().ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextByteAt, .. })).count(), 2);
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                let key = LoweredFunctionKey::Name(Name::intern("byte"));
                for (index, expected) in [(-1, Value::Null), (0, Value::Int(195)), (1, Value::Int(169)), (2, Value::Null)] {
                    let arguments = [Value::Str(Arc::from("é")), Value::Int(index)];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), expected);
                }
            });
        }
    });
}

#[test]
fn direct_native_byte_at_rejects_foreign_index_and_joint_source_rewrites() {
    on_large_stack(|| {
        let program = byte_at_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextByteAt, .. })).collect::<Vec<_>>();
            let (id, proof) = calls[0];
            let foreign = calls[1].1.contract.argument_sources[1].unwrap();
            let source = generic.native_call_source(proof.source).unwrap();
            let mut changed = program.store.clone();
            let range = changed.data[source.instruction as usize].range();
            changed.extra[range.start as usize + 1] = foreign;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err());
            let evidence = changed.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.argument_sources[1] = Some(foreign);
            let altered = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = altered;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err());
        });
    });
}

fn map_keys_program() -> FullProgram {
    source_fixture("pure int_keys(table: Map[Int, Str]) -> List[Int] { table.keys() }\npure uint_keys(table: Map[UInt, Str]) -> List[UInt] { table.keys() }\npure str_keys(table: Map[Str, Int]) -> List[Str] { table.keys() }\npure other_keys(table: Map[Int, Str]) -> List[Int] { table.keys() }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_map_keys_keep_original_key_domains_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(map_keys_program());
        assert_eq!(program.generic_evidence().unwrap().ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::MapKeys, .. })).count(), 4);
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                for (name, table, expected) in [
                    ("int_keys", Value::Map(BTreeMap::from([(MapKey::Int(4), Value::Str(Arc::from("four"))), (MapKey::Int(-2), Value::Str(Arc::from("minus")))])), Value::List(vec![Value::Int(-2), Value::Int(4)])),
                    ("uint_keys", Value::Map(BTreeMap::from([(MapKey::Int(4), Value::Str(Arc::from("four"))), (MapKey::Int(2), Value::Str(Arc::from("two")))])), Value::List(vec![Value::Int(2), Value::Int(4)])),
                    ("str_keys", Value::Map(BTreeMap::from([(MapKey::Str(Arc::from("z")), Value::Int(1)), (MapKey::Str(Arc::from("a")), Value::Int(2))])), Value::List(vec![Value::Str(Arc::from("a")), Value::Str(Arc::from("z"))])),
                ] {
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[table], Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), expected);
                }
            });
        }
    });
}

#[test]
fn direct_native_map_keys_reject_same_typed_foreign_receivers_and_joint_result_rewrites() {
    on_large_stack(|| {
        let program = map_keys_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::MapKeys, .. })).collect::<Vec<_>>();
            let (id, proof) = calls[0];
            let foreign = calls[3].1.contract.receiver.as_ref().unwrap();
            let source = generic.native_call_source(proof.source).unwrap();
            let mut changed = program.store.clone();
            let range = changed.data[source.instruction as usize].range();
            changed.extra[range.start as usize] = foreign.instruction;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err());
            let evidence = changed.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.receiver = Some(foreign.clone());
            evidence.test_ground_native_call_mut(id).unwrap().contract.argument_sources[0] = Some(foreign.instruction);
            evidence.test_ground_native_call_mut(id).unwrap().contract.result = calls[2].1.contract.result.clone();
            let altered = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = altered;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err());
        });
    });
}

fn string_method_program() -> FullProgram {
    source_fixture("pure chain(text: Str) -> Str { text.lower().translate(\"a\", \"z\") }\npure length(text: Str) -> Int { text.lower().translate(\"a\", \"z\").byte_len() }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_string_methods_keep_selected_operations_and_chained_sources_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(string_method_program());
        let operations = program.generic_evidence().unwrap().ground_native_calls().map(|(_, proof)| match proof.contract.authority { PreparedOperationAuthority::Registry { operation, .. } => operation, _ => panic!("selected string method") }).collect::<Vec<_>>();
        assert!(operations.contains(&RuntimeOp::TextLower));
        assert!(operations.contains(&RuntimeOp::TextTranslate));
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                for (name, expected) in [("chain", Value::Str(Arc::from("z b"))), ("length", Value::Int(3))] {
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let arguments = [Value::Str(Arc::from("A B"))];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), expected);
                }
            });
        }
    });
}

#[test]
fn direct_native_string_methods_reject_missing_and_jointly_rewritten_selection() {
    on_large_stack(|| {
        let program = string_method_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextTranslate, .. })).unwrap();
            let source = generic.native_call_source(proof.source).unwrap();
            let mut changed = program.store.clone();
            let range = changed.data[source.instruction as usize].range();
            let lower_name = (0..changed.strings.len() as u32).find(|&word| changed.string(word).ok() == Some("lower")).unwrap();
            changed.extra[range.start as usize + 1] = lower_name;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err());
            let evidence = changed.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.receiver.as_mut().unwrap().method_name = Name::intern("lower");
            let altered = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = altered;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err());
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
        });
    });
}

fn string_parse_int_program() -> FullProgram {
    source_fixture("pure keep(value: Result[Int]) -> Result[Int] { value }\npure parse(text: Str) -> Result[Int] { keep(text.parse_int()) }\npure other(text: Str) -> Result[Int] { keep(text.parse_int()) }\npure lower(text: Str) -> Str { text.lower() }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_parse_int_keeps_selected_result_and_original_receiver_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(string_parse_int_program());
        program.symbol_owner().with_current(|| {
            let calls = program.generic_evidence().unwrap().ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextParseInt, .. })).collect::<Vec<_>>();
            assert_eq!(calls.len(), 2);
            for (_, proof) in calls {
                let TypeRef::Ground(result) = proof.contract.result else { panic!("selected parse result is closed") };
                assert_eq!(program.store.semantic.to_type(result).unwrap(), Type::Result(Box::new(Type::Int), Box::new(Type::Error)));
                assert_eq!(proof.contract.argument_sources.len(), 1);
                assert!(proof.contract.binding.default_slots.is_empty());
                assert_eq!(proof.contract.receiver.as_ref().unwrap().method_name, Name::intern("parse_int"));
            }
            for recursive in [false, true] {
                for (input, expected) in [("0x2a", Some(42)), ("-9", Some(-9)), ("nope", None)] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(program.clone());
                    let key = LoweredFunctionKey::Name(Name::intern("parse"));
                    let arguments = [Value::Str(Arc::from(input))];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    let value = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap();
                    if let Some(expected) = expected { assert_eq!(value, Value::ok(Value::Int(expected))); }
                    else { assert!(matches!(value, Value::Result(crate::runtime::value::ResultValue::Err(_)))); }
                }
            }
        });
    });
}

#[test]
fn direct_native_parse_int_refuses_missing_foreign_and_jointly_rewritten_method_receipts() {
    on_large_stack(|| {
        let program = string_parse_int_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextParseInt, .. })).collect::<Vec<_>>();
            let (id, proof) = calls[0];
            let source = generic.native_call_source(proof.source).unwrap();
            let foreign = calls[1].1.contract.receiver.as_ref().unwrap();
            let range = program.store.data[source.instruction as usize].range();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            let mut other = program.clone();
            other.store.extra[range.start as usize] = foreign.instruction;
            let evidence = other.store.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.receiver = Some(foreign.clone());
            evidence.test_ground_native_call_mut(id).unwrap().contract.argument_sources[0] = Some(foreign.instruction);
            let rewritten = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = rewritten;
            let mut spelling = program.clone();
            spelling.store.extra[range.start as usize + 1] = (0..spelling.store.strings.len() as u32).find(|&word| spelling.store.string(word).ok() == Some("lower")).unwrap();
            let evidence = spelling.store.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.receiver.as_mut().unwrap().method_name = Name::intern("lower");
            let rewritten = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = rewritten;
            for changed in [missing, other, spelling] {
                assert!(FullVerifier::verify(&changed).is_err());
                for recursive in [false, true] {
                    let changed = Arc::new(changed.clone());
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*changed.sources).clone());
                    evaluator.indexed_program = Some(changed.clone());
                    let key = LoweredFunctionKey::Name(Name::intern("parse"));
                    let arguments = [Value::Str(Arc::from("42"))];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(changed.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err().kind, "indexed-ir");
                }
            }
        });
    });
}

fn materialized_lines_program() -> FullProgram {
    source_fixture("pure text_lines(text: Str) -> List[Str] { text.lines().collect() }\npure other_text_lines(text: Str) -> List[Str] { text.lines().collect() }\npure byte_lines(data: Bytes) -> List[Bytes] { data.lines().collect() }\npure size(items: List[Str]) -> Int { items.len() }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_materialized_lines_keep_selected_item_domains_and_collect_identity_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(materialized_lines_program());
        program.symbol_owner().with_current(|| {
            let calls = program.generic_evidence().unwrap().ground_native_calls().collect::<Vec<_>>();
            for (operation, count) in [(RuntimeOp::TextStreamLines, 2), (RuntimeOp::BytesStreamLines, 1), (RuntimeOp::StreamCollect, 3)] {
                assert_eq!(calls.iter().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: selected, .. } if selected == operation)).count(), count);
            }
            for (_, proof) in calls.iter().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::StreamCollect, .. })) {
                assert_eq!(proof.contract.registry_owner, RegistryOwner::Method(crate::modules::signature::MethodReceiver::List));
                assert_eq!(proof.contract.result, proof.contract.receiver.as_ref().unwrap().ty);
                assert!(proof.contract.binding.default_slots.is_empty());
            }
            for recursive in [false, true] {
                for (name, argument, expected) in [
                    ("text_lines", Value::Str(Arc::from("one\r\ntwo\n")), Value::List(vec![Value::Str(Arc::from("one")), Value::Str(Arc::from("two"))])),
                    ("text_lines", Value::Str(Arc::from("")), Value::List(vec![])),
                    ("byte_lines", Value::Bytes(b"\xff\r\n\0\n".to_vec()), Value::List(vec![Value::Bytes(vec![255]), Value::Bytes(vec![0])])),
                    ("byte_lines", Value::Bytes(vec![]), Value::List(vec![])),
                ] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(program.clone());
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let arguments = [argument];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), expected);
                }
            }
        });
    });
}

#[test]
fn direct_native_materialized_lines_refuse_missing_foreign_and_jointly_rewritten_collect_receipts() {
    on_large_stack(|| {
        let program = materialized_lines_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::StreamCollect, .. }) && proof.contract.receiver.as_ref().unwrap().ty == proof.contract.result).collect::<Vec<_>>();
            let (id, proof) = calls[0];
            let source = generic.native_call_source(proof.source).unwrap();
            let foreign = calls.iter().skip(1).find(|(_, candidate)| candidate.contract.result == proof.contract.result).unwrap().1.contract.receiver.as_ref().unwrap();
            let range = program.store.data[source.instruction as usize].range();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            let mut other = program.clone();
            other.store.extra[range.start as usize] = foreign.instruction;
            let evidence = other.store.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.receiver = Some(foreign.clone());
            evidence.test_ground_native_call_mut(id).unwrap().contract.argument_sources[0] = Some(foreign.instruction);
            let rewritten = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = rewritten;
            let mut spelling = program.clone();
            spelling.store.extra[range.start as usize + 1] = (0..spelling.store.strings.len() as u32).find(|&word| spelling.store.string(word).ok() == Some("len")).unwrap();
            let evidence = spelling.store.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.receiver.as_mut().unwrap().method_name = Name::intern("len");
            let rewritten = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = rewritten;
            for changed in [missing, other, spelling] {
                assert!(FullVerifier::verify(&changed).is_err());
                for recursive in [false, true] {
                    let changed = Arc::new(changed.clone());
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*changed.sources).clone());
                    evaluator.indexed_program = Some(changed.clone());
                    let key = LoweredFunctionKey::Name(Name::intern("text_lines"));
                    let arguments = [Value::Str(Arc::from("one\ntwo\n"))];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(changed.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err().kind, "indexed-ir");
                }
            }
        });
    });
}

fn stream_collect_program() -> FullProgram {
    source_fixture("stream values() [] -> Stream[Int] { yield 4; yield 7 }\npure make() -> Stream[Int] { values() }\npure range_collect() -> List[Int] { range(3).collect() }\npure other_range_collect() -> List[Int] { range(7).collect() }\npure empty_collect() -> List[Int] { range(0).collect() }\npure script_collect() -> List[Int] { values().collect() }\npure carrier(items: List[Int]) -> Int { items.len() }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_stream_collect_keeps_selected_stream_carrier_and_lazy_script_ownership_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(stream_collect_program());
        program.symbol_owner().with_current(|| {
            let calls = program.generic_evidence().unwrap().ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::StreamCollect, .. })).collect::<Vec<_>>();
            assert_eq!(calls.len(), 4);
            for (_, proof) in calls {
                assert_eq!(proof.contract.registry_owner, RegistryOwner::Method(crate::modules::signature::MethodReceiver::Stream));
                let TypeRef::Ground(receiver) = proof.contract.receiver.as_ref().unwrap().ty else { panic!("selected Stream receiver is closed") };
                let TypeRef::Ground(result) = proof.contract.result else { panic!("selected collection result is closed") };
                assert_eq!(program.store.semantic.to_type(receiver).unwrap(), Type::Stream(Box::new(Type::Int)));
                assert_eq!(program.store.semantic.to_type(result).unwrap(), Type::List(Box::new(Type::Int)));
                assert!(proof.contract.binding.default_slots.is_empty());
            }
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                for (name, values) in [("range_collect", vec![0, 1, 2]), ("empty_collect", vec![]), ("script_collect", vec![4, 7])] {
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let span = Span::new(program.store.source_id, 0, 0);
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[], span).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), Value::List(values.into_iter().map(Value::Int).collect()));
                }
                let key = LoweredFunctionKey::Name(Name::intern("make"));
                let span = Span::new(program.store.source_id, 0, 0);
                let Value::Stream(stream) = evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[], span).unwrap().unwrap() else { panic!("creation retains a lazy script carrier") };
                assert!(stream.items.is_empty());
                let state = stream.script().unwrap().clone();
                assert!(!state.lock(span).unwrap().finished());
                evaluator.cancel_script_state(state.clone(), span).unwrap();
                assert!(state.lock(span).unwrap().finished());
            }
        });
    });
}

#[test]
fn direct_native_stream_collect_refuses_missing_foreign_and_changed_receiver_carriers() {
    on_large_stack(|| {
        let program = stream_collect_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::StreamCollect, .. })).collect::<Vec<_>>();
            let (id, proof) = calls[0];
            let source = generic.native_call_source(proof.source).unwrap();
            let foreign = calls[1].1.contract.receiver.as_ref().unwrap();
            let list = generic.ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::ListLen, .. })).unwrap().1.contract.receiver.as_ref().unwrap();
            let range = program.store.data[source.instruction as usize].range();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            let mut variants = vec![missing];
            for receiver in [foreign, list] {
                let mut changed = program.clone();
                changed.store.extra[range.start as usize] = receiver.instruction;
                let evidence = changed.store.generic.as_deref_mut().unwrap();
                let altered = &mut evidence.test_ground_native_call_mut(id).unwrap().contract;
                altered.receiver = Some(receiver.clone());
                altered.argument_sources[0] = Some(receiver.instruction);
                let rewritten = evidence.ground_native_call(id).unwrap().contract.clone();
                evidence.test_native_call_source_mut(proof.source).unwrap().expected = rewritten;
                variants.push(changed);
            }
            for changed in variants {
                assert!(FullVerifier::verify(&changed).is_err());
                for recursive in [false, true] {
                    let changed = Arc::new(changed.clone());
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*changed.sources).clone());
                    evaluator.indexed_program = Some(changed.clone());
                    let key = LoweredFunctionKey::Name(Name::intern("range_collect"));
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &[], Span::new(changed.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err().kind, "indexed-ir");
                }
            }
        });
    });
}

fn text_words_split_program() -> FullProgram {
    source_fixture("pure words(text: Str) -> List[Str] { text.words() }\npure split(text: Str, separator: Str) -> List[Str] { text.split(separator) }\npure other_split(text: Str, separator: Str) -> List[Str] { text.split(separator) }\npure limited(text: Str) -> List[Str] { text.split(maxsplit: 1, separator: \",\") }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_text_words_split_keep_original_named_order_and_omitted_limit_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(text_words_split_program());
        program.symbol_owner().with_current(|| {
            let calls = program.generic_evidence().unwrap().ground_native_calls().collect::<Vec<_>>();
            assert_eq!(calls.iter().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextWords, .. })).count(), 1);
            let splits = calls.iter().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextSplit, .. })).collect::<Vec<_>>();
            assert_eq!(splits.len(), 3);
            for (_, proof) in &splits {
                let TypeRef::Ground(result) = proof.contract.result else { panic!("selected split result is closed") };
                assert_eq!(program.store.semantic.to_type(result).unwrap(), Type::List(Box::new(Type::Str)));
                if !proof.contract.binding.default_slots.is_empty() {
                    assert_eq!(proof.contract.binding.default_slots.as_ref(), &[2]);
                    assert_eq!(proof.contract.argument_sources[2], None);
                } else {
                    assert_eq!(proof.contract.binding.supplied_slots.as_ref(), &[2, 1]);
                    assert_eq!(proof.contract.arguments[0].original.name, Some(Name::intern("maxsplit")));
                    assert_eq!(proof.contract.arguments[1].original.name, Some(Name::intern("separator")));
                    assert_eq!(proof.contract.argument_sources[2], Some(proof.contract.arguments[0].instruction));
                    assert_eq!(proof.contract.argument_sources[1], Some(proof.contract.arguments[1].instruction));
                }
            }
            for recursive in [false, true] {
                for (name, arguments, expected) in [
                    ("words", vec![Value::Str(Arc::from(" alpha\tβ\u{a0}gamma\n"))], vec!["alpha", "β", "gamma"]),
                    ("split", vec![Value::Str(Arc::from("a,b,c")), Value::Str(Arc::from(","))], vec!["a", "b", "c"]),
                    ("split", vec![Value::Str(Arc::from("ab")), Value::Str(Arc::from(""))], vec!["a", "b"]),
                    ("limited", vec![Value::Str(Arc::from("a,b,c"))], vec!["a", "b,c"]),
                ] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(program.clone());
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), Value::List(expected.into_iter().map(|value| Value::Str(Arc::from(value))).collect()));
                }
            }
        });
    });
}

#[test]
fn direct_native_text_split_refuses_missing_foreign_and_jointly_rewritten_default_packets() {
    on_large_stack(|| {
        let program = text_words_split_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextSplit, .. })).collect::<Vec<_>>();
            let (id, proof) = calls[0];
            let source = generic.native_call_source(proof.source).unwrap();
            let foreign = calls[1].1.contract.receiver.as_ref().unwrap();
            let supplied = calls.iter().find(|(_, proof)| proof.contract.binding.default_slots.is_empty()).unwrap().1;
            let supplied_source = generic.native_call_source(supplied.source).unwrap();
            let supplied_words = program.store.payload(program.store.data[supplied_source.instruction as usize].range()).unwrap();
            let range = program.store.data[source.instruction as usize].range();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            let mut other = program.clone();
            other.store.extra[range.start as usize] = foreign.instruction;
            let evidence = other.store.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.receiver = Some(foreign.clone());
            evidence.test_ground_native_call_mut(id).unwrap().contract.argument_sources[0] = Some(foreign.instruction);
            let rewritten = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = rewritten;
            let mut default = program.clone();
            default.store.extra[range.start as usize + 2] = supplied_words[2];
            let evidence = default.store.generic.as_deref_mut().unwrap();
            let altered = &mut evidence.test_ground_native_call_mut(id).unwrap().contract;
            altered.binding.default_slots = Box::new([]);
            altered.binding.supplied_slots = vec![1, 2].into_boxed_slice();
            altered.argument_sources[2] = supplied.contract.argument_sources[2];
            let rewritten = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = rewritten;
            for changed in [missing, other, default] {
                assert!(FullVerifier::verify(&changed).is_err());
                for recursive in [false, true] {
                    let changed = Arc::new(changed.clone());
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*changed.sources).clone());
                    evaluator.indexed_program = Some(changed.clone());
                    let key = LoweredFunctionKey::Name(Name::intern("split"));
                    let arguments = [Value::Str(Arc::from("a,b,c")), Value::Str(Arc::from(","))];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(changed.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err().kind, "indexed-ir");
                }
            }
        });
    });
}

fn digest_method_program() -> FullProgram {
    source_fixture("pure hex(digest: Digest) -> Str { digest.hex() }\npure other_hex(digest: Digest) -> Str { digest.hex() }\npure base64(digest: Digest) -> Str { digest.base64() }\npure chained(data: Bytes) -> Str { hash.sha256(data).hex() }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_digest_methods_keep_original_nominal_producer_and_selected_encoding_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(digest_method_program());
        program.symbol_owner().with_current(|| {
            let calls = program.generic_evidence().unwrap().ground_native_calls().filter(|(_, proof)| proof.contract.registry_owner == RegistryOwner::Method(crate::modules::signature::MethodReceiver::Digest)).collect::<Vec<_>>();
            assert_eq!(calls.len(), 4);
            for (_, proof) in calls {
                let TypeRef::Ground(receiver) = proof.contract.receiver.as_ref().unwrap().ty else { panic!("original Digest receiver is closed") };
                let TypeRef::Ground(result) = proof.contract.result else { panic!("original Digest encoding is closed") };
                assert_eq!(program.store.semantic.to_type(receiver).unwrap(), Type::Digest);
                assert_eq!(program.store.semantic.to_type(result).unwrap(), Type::Str);
                assert!(proof.contract.arguments.is_empty());
            }
            let digest = crate::modules::hash::digest_bytes(crate::modules::hash::HashAlgorithm::Sha256, b"");
            for recursive in [false, true] {
                for (name, arguments, expected) in [
                    ("hex", vec![Value::digest(digest.clone())], "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
                    ("base64", vec![Value::digest(digest.clone())], "47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="),
                    ("chained", vec![Value::Bytes(vec![])], "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
                ] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(program.clone());
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), Value::Str(Arc::from(expected)));
                }
            }
        });
    });
}

#[test]
fn direct_native_digest_methods_refuse_missing_foreign_and_jointly_rewritten_encoding_receipts() {
    on_large_stack(|| {
        let program = digest_method_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::DigestHex, .. })).collect::<Vec<_>>();
            let (id, proof) = calls[0];
            let source = generic.native_call_source(proof.source).unwrap();
            let foreign = calls[1].1.contract.receiver.as_ref().unwrap();
            let base64 = generic.ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::DigestBase64, .. })).unwrap().1;
            let range = program.store.data[source.instruction as usize].range();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            let mut other = program.clone();
            other.store.extra[range.start as usize] = foreign.instruction;
            let evidence = other.store.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.receiver = Some(foreign.clone());
            evidence.test_ground_native_call_mut(id).unwrap().contract.argument_sources[0] = Some(foreign.instruction);
            let rewritten = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = rewritten;
            let mut encoding = program.clone();
            encoding.store.extra[range.start as usize + 1] = (0..encoding.store.strings.len() as u32).find(|&word| encoding.store.string(word).ok() == Some("base64")).unwrap();
            let evidence = encoding.store.generic.as_deref_mut().unwrap();
            let altered = &mut evidence.test_ground_native_call_mut(id).unwrap().contract;
            altered.authority = base64.contract.authority.clone();
            altered.receiver.as_mut().unwrap().method_name = Name::intern("base64");
            let rewritten = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = rewritten;
            for changed in [missing, other, encoding] {
                assert!(FullVerifier::verify(&changed).is_err());
                for recursive in [false, true] {
                    let changed = Arc::new(changed.clone());
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*changed.sources).clone());
                    evaluator.indexed_program = Some(changed.clone());
                    let key = LoweredFunctionKey::Name(Name::intern("hex"));
                    let arguments = [Value::digest(crate::modules::hash::digest_bytes(crate::modules::hash::HashAlgorithm::Sha256, b""))];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(changed.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err().kind, "indexed-ir");
                }
            }
        });
    });
}

fn text_bytes_family_program() -> FullProgram {
    source_fixture("pure upper(text: Str) -> Str { text.upper() }\npure other_upper(text: Str) -> Str { text.upper() }\npure lower(text: Str) -> Str { text.lower() }\npure view(text: Str) -> Str { text.trim().upper() }\npure replace(text: Str, from: Str, to: Str) -> Str { text.replace(to: to, from: from) }\npure text_prefix(text: Str, prefix: Str) -> Bool { text.starts_with(prefix) }\npure bytes_prefix(bytes: Bytes, prefix: Bytes) -> Bool { bytes.starts_with(prefix) }\npure decimal(text: Str) -> Result[Int] { text.parse_int_decimal() }\npure decode(bytes: Bytes) -> Result[Str] { bytes.utf8() }\npure hash(bytes: Bytes) -> Str { bytes.sha256().hex() }\npure chunk_count(bytes: Bytes) -> Int { bytes.chunks(2).len() }\npure byte_view(bytes: Bytes) -> Str { bytes.trim().sha256().hex() }\npure bytes_equal(left: Bytes, right: Bytes) -> Bool { left.compare(right).equal }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_text_bytes_family_preserves_numeric_receiver_domains_and_result_shapes_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(text_bytes_family_program());
        program.symbol_owner().with_current(|| {
            let calls = program.generic_evidence().unwrap().ground_native_calls().collect::<Vec<_>>();
            for operation in [RuntimeOp::TextUpper, RuntimeOp::TextTrim, RuntimeOp::TextReplace, RuntimeOp::TextStartsWith, RuntimeOp::BytesStartsWith, RuntimeOp::TextParseIntDecimal, RuntimeOp::BytesUtf8, RuntimeOp::HashSha256, RuntimeOp::BytesChunks, RuntimeOp::BytesTrim, RuntimeOp::BytesCompare] {
                assert!(calls.iter().any(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: actual, .. } if actual == operation)));
            }
            let replace = calls.iter().find(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextReplace, .. })).unwrap().1;
            assert_eq!(replace.contract.binding.supplied_slots.as_ref(), &[2, 1]);
            for recursive in [false, true] {
                for (name, arguments, expected) in [
                    ("upper", vec![Value::Str(Arc::from("aβ"))], Value::Str(Arc::from("AΒ"))),
                    ("view", vec![Value::Str(Arc::from("  aβ  "))], Value::Str(Arc::from("AΒ"))),
                    ("replace", vec![Value::Str(Arc::from("abcabc")), Value::Str(Arc::from("ab")), Value::Str(Arc::from("z"))], Value::Str(Arc::from("zczc"))),
                    ("text_prefix", vec![Value::Str(Arc::from("αβ")), Value::Str(Arc::from("α"))], Value::Bool(true)),
                    ("bytes_prefix", vec![Value::Bytes(vec![255, 0, 1]), Value::Bytes(vec![255, 0])], Value::Bool(true)),
                    ("decimal", vec![Value::Str(Arc::from("42"))], Value::ok(Value::Int(42))),
                    ("decode", vec![Value::Bytes("β".as_bytes().to_vec())], Value::ok(Value::Str(Arc::from("β")))),
                    ("hash", vec![Value::Bytes(vec![])], Value::Str(Arc::from("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"))),
                    ("chunk_count", vec![Value::Bytes(vec![0, 1, 2, 3, 4])], Value::Int(3)),
                    ("byte_view", vec![Value::Bytes(b"  ".to_vec())], Value::Str(Arc::from("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"))),
                    ("bytes_equal", vec![Value::Bytes(vec![255, 0]), Value::Bytes(vec![255, 0])], Value::Bool(true)),
                ] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(program.clone());
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), expected);
                }
            }
        });
    });
}

#[test]
fn direct_native_text_bytes_family_refuses_missing_foreign_and_jointly_rewritten_numeric_authority() {
    on_large_stack(|| {
        let program = text_bytes_family_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextUpper, .. })).collect::<Vec<_>>();
            let (id, proof) = calls[0];
            let source = generic.native_call_source(proof.source).unwrap();
            let foreign = calls[1].1.contract.receiver.as_ref().unwrap();
            let lower = generic.ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextLower, .. })).unwrap().1;
            let range = program.store.data[source.instruction as usize].range();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            let mut other = program.clone();
            other.store.extra[range.start as usize] = foreign.instruction;
            let evidence = other.store.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.receiver = Some(foreign.clone());
            evidence.test_ground_native_call_mut(id).unwrap().contract.argument_sources[0] = Some(foreign.instruction);
            let rewritten = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = rewritten;
            let mut changed = program.clone();
            changed.store.extra[range.start as usize + 1] = (0..changed.store.strings.len() as u32).find(|&word| changed.store.string(word).ok() == Some("lower")).unwrap();
            let evidence = changed.store.generic.as_deref_mut().unwrap();
            let altered = &mut evidence.test_ground_native_call_mut(id).unwrap().contract;
            altered.authority = lower.contract.authority.clone();
            altered.receiver.as_mut().unwrap().method_name = Name::intern("lower");
            let rewritten = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = rewritten;
            for changed in [missing, other, changed] {
                assert!(FullVerifier::verify(&changed).is_err());
                for recursive in [false, true] {
                    let changed = Arc::new(changed.clone());
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*changed.sources).clone());
                    evaluator.indexed_program = Some(changed.clone());
                    let key = LoweredFunctionKey::Name(Name::intern("upper"));
                    let arguments = [Value::Str(Arc::from("ab"))];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(changed.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err().kind, "indexed-ir");
                }
            }
        });
    });
}

fn fs_children_program() -> FullProgram {
    source_fixture("proc defaults(path: Path) [fs, error] -> Int { fs.children(path)?.collect().len() }\nproc other_defaults(path: Path) [fs, error] -> Int { fs.children(path)?.collect().len() }\nproc configured(path: Path) [fs, error] -> Int { fs.children(path, ordered: false, stat: false)?.collect().len() }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_fs_children_keeps_original_specialized_carrier_defaults_and_named_sources_on_both_routes() {
    on_large_stack(|| {
        use std::os::unix::ffi::OsStrExt;
        let program = Arc::new(fs_children_program());
        program.symbol_owner().with_current(|| {
            let calls = program.generic_evidence().unwrap().ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::FsChildren, .. })).collect::<Vec<_>>();
            assert_eq!(calls.len(), 3);
            for (_, proof) in calls {
                let source = program.generic_evidence().unwrap().native_call_source(proof.source).unwrap();
                assert_eq!(program.store.tags[source.instruction as usize], FullTag::ExprFsList);
                assert_eq!(proof.contract.registry_owner, RegistryOwner::Module("fs"));
                let TypeRef::Ground(result) = proof.contract.result else { panic!("selected filesystem stream result is closed") };
                let Type::Result(success, _) = program.store.semantic.to_type(result).unwrap() else { panic!("filesystem children keeps its Result carrier") };
                let Type::Stream(item) = *success else { panic!("filesystem children keeps its Stream success") };
                let Type::Record(fields) = *item else { panic!("filesystem entries retain their checked record domain") };
                assert_eq!(fields.get(&Name::intern("path")), Some(&Type::Path));
                if proof.contract.binding.default_slots.is_empty() {
                    assert_eq!(proof.contract.binding.supplied_slots.as_ref(), &[0, 2, 1]);
                    assert_eq!(proof.contract.arguments[1].original.name, Some(Name::intern("ordered")));
                    assert_eq!(proof.contract.arguments[2].original.name, Some(Name::intern("stat")));
                } else {
                    assert_eq!(proof.contract.binding.default_slots.as_ref(), &[1, 2]);
                    assert_eq!(proof.contract.argument_sources[1..], [None, None]);
                }
            }
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(temp.path().join("file"), b"value").unwrap();
            std::fs::create_dir(temp.path().join("directory")).unwrap();
            for recursive in [false, true] {
                for name in ["defaults", "configured"] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(program.clone());
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let arguments = [Value::Path(crate::runtime::value::PathValue::new(temp.path().as_os_str().as_bytes().to_vec()).unwrap())];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), Value::Int(2));
                }
            }
        });
    });
}

#[test]
fn direct_native_fs_children_refuses_missing_foreign_changed_operation_and_default_packets() {
    on_large_stack(|| {
        use std::os::unix::ffi::OsStrExt;
        let program = fs_children_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::FsChildren, .. })).collect::<Vec<_>>();
            let (id, proof) = calls[0];
            let source = generic.native_call_source(proof.source).unwrap();
            let range = program.store.data[source.instruction as usize].range();
            let foreign = calls[1].1.contract.argument_sources[0].unwrap();
            let supplied = calls.iter().find(|(_, proof)| proof.contract.binding.default_slots.is_empty()).unwrap().1;
            let supplied_source = generic.native_call_source(supplied.source).unwrap();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            let mut other = program.clone();
            other.store.extra[range.start as usize + 1] = foreign;
            let mut operation = program.clone();
            operation.store.extra[range.start as usize] = operation.store.runtime_ops.len() as u32;
            operation.store.runtime_ops.push(RuntimeOp::FsFiles);
            let mut defaults = program.clone();
            defaults.store.data[source.instruction as usize] = program.store.data[supplied_source.instruction as usize];
            let evidence = defaults.store.generic.as_deref_mut().unwrap();
            let altered = &mut evidence.test_ground_native_call_mut(id).unwrap().contract;
            altered.binding.default_slots = Box::new([]);
            altered.argument_sources = supplied.contract.argument_sources.clone();
            let rewritten = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = rewritten;
            let temp = tempfile::tempdir().unwrap();
            for changed in [missing, other, operation, defaults] {
                assert!(FullVerifier::verify(&changed).is_err());
                for recursive in [false, true] {
                    let changed = Arc::new(changed.clone());
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*changed.sources).clone());
                    evaluator.indexed_program = Some(changed.clone());
                    let key = LoweredFunctionKey::Name(Name::intern("defaults"));
                    let arguments = [Value::Path(crate::runtime::value::PathValue::new(temp.path().as_os_str().as_bytes().to_vec()).unwrap())];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, &arguments, Span::new(changed.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err().kind, "indexed-ir");
                }
            }
        });
    });
}

fn text_byte_slice_program() -> FullProgram {
    source_fixture("pure tail(text: Str, offset: Int) -> Str { text.byte_slice(offset) }\npure slice(text: Str, offset: Int, length: Int) -> Str { text.byte_slice(length: length, offset: offset) }\npure other_tail(text: Str, offset: Int) -> Str { text.byte_slice(offset) }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_text_byte_slice_keeps_original_byte_bounds_and_omitted_length_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(text_byte_slice_program());
        program.symbol_owner().with_current(|| {
            let calls = program.generic_evidence().unwrap().ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextByteSlice, .. })).collect::<Vec<_>>();
            assert_eq!(calls.len(), 3);
            assert_eq!(calls.iter().filter(|(_, proof)| proof.contract.binding.default_slots.as_ref() == [2]).count(), 2);
            let supplied = calls.iter().find(|(_, proof)| proof.contract.binding.default_slots.is_empty()).unwrap().1;
            assert_eq!(supplied.contract.binding.supplied_slots.as_ref(), &[2, 1]);
            for recursive in [false, true] {
                for (name, arguments, expected) in [
                    ("tail", vec![Value::Str(Arc::from("aβz")), Value::Int(1)], "βz"),
                    ("slice", vec![Value::Str(Arc::from("aβz")), Value::Int(1), Value::Int(2)], "β"),
                    ("slice", vec![Value::Str(Arc::from("aβz")), Value::Int(0), Value::Int(0)], ""),
                ] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(program.clone());
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), Value::Str(Arc::from(expected)));
                }
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                let key = LoweredFunctionKey::Name(Name::intern("slice"));
                let arguments = [Value::Str(Arc::from("aβz")), Value::Int(2), Value::Int(1)];
                let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err().kind, "text-byte-slice");
            }
        });
    });
}

#[test]
fn direct_native_text_byte_slice_refuses_missing_foreign_and_changed_default_receipts() {
    on_large_stack(|| {
        let program = text_byte_slice_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextByteSlice, .. })).collect::<Vec<_>>();
            let (id, proof) = calls[0];
            let source = generic.native_call_source(proof.source).unwrap();
            let range = program.store.data[source.instruction as usize].range();
            let foreign = calls.iter().skip(1).find(|(_, proof)| !proof.contract.binding.default_slots.is_empty()).unwrap().1.contract.receiver.as_ref().unwrap();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            let mut other = program.clone();
            other.store.extra[range.start as usize] = foreign.instruction;
            let mut defaults = program.clone();
            let evidence = defaults.store.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.binding.default_slots = Box::new([]);
            let rewritten = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = rewritten;
            for changed in [missing, other, defaults] {
                assert!(FullVerifier::verify(&changed).is_err());
                for recursive in [false, true] {
                    let changed = Arc::new(changed.clone());
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*changed.sources).clone());
                    evaluator.indexed_program = Some(changed.clone());
                    let key = LoweredFunctionKey::Name(Name::intern("tail"));
                    let arguments = [Value::Str(Arc::from("aβz")), Value::Int(1)];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(changed.store.source_id, 0, 0)).unwrap();
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err().kind, "indexed-ir");
                }
            }
        });
    });
}

#[test]
fn direct_native_container_methods_preserve_ground_receivers_and_arguments_on_both_routes() {
    on_large_stack(|| {
    let program = Arc::new(container_method_program());
    let operations = program.generic_evidence().unwrap().ground_native_calls().map(|(_, proof)| match proof.contract.authority { PreparedOperationAuthority::Registry { operation, .. } => operation, _ => panic!("selected native method") }).collect::<Vec<_>>();
    for expected in [RuntimeOp::MapGet, RuntimeOp::MapSet, RuntimeOp::ListGet, RuntimeOp::ListPush, RuntimeOp::MapLen, RuntimeOp::ListLen] { assert!(operations.contains(&expected), "source retains selected {expected:?}"); }
    let item = Value::List(vec![Value::Int(4)]);
    let table = Value::Map(BTreeMap::from([(MapKey::Str(Arc::from("left")), item.clone())]));
    let items = Value::List(vec![Value::List(vec![Value::Int(1)]), Value::List(vec![Value::Int(2)])]);
    for recursive in [false, true] {
        for (name, arguments, expected) in [
            ("read", vec![table.clone(), Value::Str(Arc::from("left"))], Value::ok(item.clone())),
            ("update", vec![table.clone(), Value::Str(Arc::from("right")), Value::List(vec![Value::Int(7)])], Value::Map(BTreeMap::from([(MapKey::Str(Arc::from("left")), item.clone()), (MapKey::Str(Arc::from("right")), Value::List(vec![Value::Int(7)]))]))),
            ("list_read", vec![items.clone(), Value::Int(1)], Value::ok(Value::List(vec![Value::Int(2)]))),
            ("append", vec![items.clone(), item.clone()], Value::List(vec![Value::List(vec![Value::Int(1)]), Value::List(vec![Value::Int(2)]), item.clone()])),
            ("lengths", vec![table.clone(), items.clone()], Value::Int(3)),
        ] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let name = program.symbol_owner().with_current(|| Name::intern(name));
            let mut call = || evaluator.call_indexed_direct(LoweredFunctionKey::Name(name), LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).expect("container method exists");
            let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(name), recursive, call);
            assert_eq!(result.unwrap(), expected);
        }
    }
    });
}

fn saved_receiver_program() -> FullProgram {
    source_fixture("pure saved(table: Map[Str, List[Int]], item: List[Int]) -> Map[Str, List[Int]] { table.set(key: \"right\", value: item) }\npure other_saved(table: Map[Str, List[Int]], item: List[Int]) -> Map[Str, List[Int]] { table.set(key: \"right\", value: item) }\npure nested_saved(table: Map[Str, List[Int]], item: List[Int]) -> Map[Str, List[Int]] { table.set(key: \"middle\", value: item).set(key: \"last\", value: item) }\npure direct_chain(table: Map[Str, List[Int]], item: List[Int]) -> Map[Str, List[Int]] { table.set(key: \"middle\", value: item).set(\"last\", item) }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_saved_receivers_keep_authored_sources_and_nested_initialization_on_both_routes() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        let program = Arc::new(saved_receiver_program());
        let generic = program.generic_evidence().unwrap();
        assert_eq!(generic.ground_native_calls().filter(|(_, proof)| proof.contract.receiver.as_ref().is_some_and(|receiver| receiver.saved.is_some())).count(), 5);
        let mut direct_wrappers = 0;
        for (_, proof) in generic.ground_native_calls() {
            let receiver = proof.contract.receiver.as_ref().unwrap();
            let Some(saved) = receiver.saved.as_ref() else {
                assert_ne!(receiver.instruction, receiver.source_instruction);
                assert!(!receiver.source_wrappers.is_empty());
                direct_wrappers += 1;
                continue;
            };
            assert!(generic.registered_instruction_origin(receiver.instruction, false).is_none(), "a generated saved read has no authored expression identity");
            assert_eq!(generic.registered_instruction_origin(saved.initializer_source_instruction, false), Some((OperationSourceOrigin::Expression(receiver.origin), generic.native_call_source(proof.source).unwrap().owner)));
        }
        assert_eq!(direct_wrappers, 1);
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let item = Value::List(vec![Value::Int(7)]);
                let table = BTreeMap::from([(MapKey::Str(Arc::from("left")), Value::List(vec![Value::Int(4)]))]);
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                for name in ["saved", "nested_saved", "direct_chain"] {
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let arguments = [Value::Map(table.clone()), item.clone()];
                    let execute = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, execute).unwrap();
                    let mut expected = table.clone();
                    for key in if name == "saved" { &["right"][..] } else { &["middle", "last"][..] } { expected.insert(MapKey::Str(Arc::from(*key)), item.clone()); }
                    assert_eq!(result, Value::Map(expected));
                }
            });
        }
    }).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}

#[test]
fn direct_native_saved_receivers_reject_missing_foreign_and_jointly_rewritten_transport() {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
        let program = saved_receiver_program();
        let foreign = saved_receiver_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_native_calls().next().unwrap();
            let receiver = proof.contract.receiver.as_ref().unwrap();
            let saved = receiver.saved.as_ref().unwrap();
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_remove_original_compiler_argument_wrappers();
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
            let mut foreign_receipts = program.store.clone();
            foreign_receipts.generic.as_deref_mut().unwrap().test_replace_original_compiler_argument_wrappers(foreign.generic_evidence().unwrap());
            assert!(FullVerifier::verify_generic_evidence(&foreign_receipts).is_err());
            let mut changed = program.store.clone();
            let range = changed.data[receiver.instruction as usize].range();
            changed.extra[range.start as usize] = saved.slot + 1;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err());
            let evidence = changed.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.receiver.as_mut().unwrap().saved.as_mut().unwrap().slot += 1;
            let contract = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = contract;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "rewritten physical and mutable contract copies cannot replace the independent original source");
        });
    }).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}

fn native_path_read_program() -> FullProgram {
    source_fixture("proc read_text(file: Path) [fs] -> Result[Str] { file.read_text() }\nproc other_text(file: Path) [fs] -> Result[Str] { file.read_text() }\nproc read_bytes(file: Path) [fs] -> Result[Bytes] { file.read_bytes() }\nproc propagate_text(file: Path) [fs, error] -> Result[Str] { file.read_text()? }\nproc propagate_bytes(file: Path) [fs, error] -> Result[Bytes] { file.read_bytes()? }\nproc module_text(file: Path) [fs] -> Result[Str] { fs.read_text(file) }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_path_reads_keep_original_selected_carriers_and_hidden_receivers_after_frontend_drop() {
    on_large_stack(|| {
        use std::os::unix::ffi::OsStrExt;
        let program = Arc::new(native_path_read_program());
        program.symbol_owner().with_current(|| {
            let path_methods = program.generic_evidence().unwrap().ground_native_calls().filter(|(_, proof)| proof.contract.registry_owner == RegistryOwner::Method(crate::modules::signature::MethodReceiver::Path)).collect::<Vec<_>>();
            assert_eq!(path_methods.len(), 5);
            for (_, proof) in &path_methods {
                let receiver = proof.contract.receiver.as_ref().unwrap();
                let TypeRef::Ground(actual) = receiver.ty else { panic!("Path receiver is closed") };
                let TypeRef::Ground(source) = receiver.source_type else { panic!("original Path source is closed") };
                assert_eq!(program.store.semantic.to_type(actual).unwrap(), Type::Path);
                assert_eq!(program.store.semantic.to_type(source).unwrap(), Type::Path);
                assert_eq!(proof.contract.argument_sources[0], Some(receiver.instruction));
                assert!(proof.contract.arguments.is_empty());
            }
        });
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("payload");
        std::fs::write(&file, b"hello").unwrap();
        let missing = directory.path().join("missing");
        for recursive in [false, true] {
            for (path, succeeds) in [(&file, true), (&missing, false)] {
                let argument = Value::Path(crate::runtime::value::PathValue::new(path.as_os_str().as_bytes().to_vec()).unwrap());
                for name in ["read_text", "other_text", "read_bytes", "propagate_text", "propagate_bytes", "module_text"] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(program.clone());
                    let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, std::slice::from_ref(&argument), Span::new(program.store.source_id, 0, 0)).unwrap();
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap();
                    if succeeds {
                        let expected = if name.ends_with("bytes") { Value::Bytes(b"hello".to_vec()) } else { Value::Str(Arc::from("hello")) };
                        assert_eq!(result, Value::ok(expected));
                    } else { assert!(matches!(result, Value::Result(crate::runtime::value::ResultValue::Err(_)))); }
                }
            }
        }
    });
}

#[test]
fn direct_native_path_reads_refuse_missing_foreign_opcode_and_receiver_rewrites() {
    on_large_stack(|| {
        let program = native_path_read_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let reads = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::FsReadText, .. }) && proof.contract.receiver.is_some()).collect::<Vec<_>>();
            let (id, proof) = reads[0];
            let source = generic.native_call_source(proof.source).unwrap();
            let foreign = reads.iter().find(|(_, candidate)| generic.native_call_source(candidate.source).unwrap().owner != source.owner).unwrap().1.contract.receiver.as_ref().unwrap();
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
            let mut wrong_opcode = program.store.clone();
            wrong_opcode.tags[source.instruction as usize] = FullTag::ExprPathReadBytes;
            assert!(FullVerifier::verify_generic_evidence(&wrong_opcode).is_err());
            let mut wrong_receiver = program.store.clone();
            let range = wrong_receiver.data[source.instruction as usize].range();
            wrong_receiver.extra[range.start as usize] = foreign.instruction;
            assert!(FullVerifier::verify_generic_evidence(&wrong_receiver).is_err());
            let changed = wrong_receiver.generic.as_deref_mut().unwrap();
            changed.test_ground_native_call_mut(id).unwrap().contract.receiver = Some(foreign.clone());
            changed.test_ground_native_call_mut(id).unwrap().contract.argument_sources[0] = Some(foreign.instruction);
            let rewritten = changed.ground_native_call(id).unwrap().contract.clone();
            changed.test_native_call_source_mut(proof.source).unwrap().expected = rewritten;
            assert!(FullVerifier::verify_generic_evidence(&wrong_receiver).is_err(), "rewritten receipts cannot replace the protected original native source");
        });
    });
}

#[test]
fn direct_native_path_display_keeps_selected_path_operations_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(source_fixture("pure display_path(file: Path) -> Str { file.display() }\npure normalize_path(file: Path) -> Str { file.normalize().display() }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq }));
        program.symbol_owner().with_current(|| {
            let proofs = program.generic_evidence().unwrap().ground_native_calls().collect::<Vec<_>>();
            assert_eq!(proofs.len(), 3);
            assert!(proofs.iter().all(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::PathDisplay | RuntimeOp::PathNormalize, .. })));
        });
        for recursive in [false, true] {
            for (name, expected) in [("display_path", "a/../file.txt"), ("normalize_path", "file.txt")] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let arguments = [Value::Path(crate::runtime::value::PathValue::new(b"a/../file.txt".to_vec()).unwrap())];
                let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                let actual = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap();
                assert_eq!(actual, Value::Str(Arc::from(expected)));
            }
        }
    });
}

fn native_record_argument_program() -> FullProgram {
    source_fixture("pure selected(key: Str) -> Int { let table: Map[Str, Int] = {left: 4}; let arguments = {key: key}; table.get(...arguments) ?? 0 }\npure other_selected(key: Str) -> Int { let table: Map[Str, Int] = {left: 4}; let arguments = {key: key}; table.get(...arguments) ?? 0 }\npure reordered(key: Str, value: Int) -> Map[Str, Int] { let table: Map[Str, Int] = {left: 4}; let arguments = {value: value, key: key}; table.set(...arguments) }\nproc write_record(path: Path, data: Str) [fs] -> Result[Unit] { let arguments = {data: data, path: path}; fs.write(...arguments) }\nproc other_write_record(path: Path, data: Str) [fs] -> Result[Unit] { let arguments = {data: data, path: path}; fs.write(...arguments) }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn direct_native_record_arguments_keep_original_entries_fields_and_formals_on_both_routes() {
    on_large_stack(|| {
        use std::os::unix::ffi::OsStrExt;
        let program = Arc::new(native_record_argument_program());
        program.symbol_owner().with_current(|| {
            let sources = program.generic_evidence().unwrap().native_call_sources().filter(|(_, source)| !source.record_arguments.is_empty()).collect::<Vec<_>>();
            assert_eq!(sources.len(), 5);
            for (_, source) in sources {
                let first = &source.record_arguments[0];
                for field in source.record_arguments.iter() {
                    let argument = &source.expected.arguments[field.ordinal as usize];
                    assert_eq!(field.entry_index as usize, argument.original.entry_index);
                    assert_eq!(source.expected.binding.supplied_slots[field.ordinal as usize], field.formal_slot);
                    assert_eq!(argument.original.name, Some(field.field));
                    assert_eq!(field.record_wrapper, first.record_wrapper, "the authored spread entry is initialized once for all its fields");
                    assert_eq!(field.record_initializer, first.record_initializer);
                    assert!(program.generic_evidence().unwrap().registered_instruction_origin(field.field_initializer, false).is_none());
                    assert!(program.generic_evidence().unwrap().registered_instruction_origin(field.field_read, false).is_none());
                }
            }
        });
        let temp = tempfile::tempdir().unwrap();
        for recursive in [false, true] {
            for (name, arguments, expected) in [
                ("selected", vec![Value::Str(Arc::from("left"))], Value::Int(4)),
                ("other_selected", vec![Value::Str(Arc::from("missing"))], Value::Int(0)),
                ("reordered", vec![Value::Str(Arc::from("right")), Value::Int(7)], Value::Map(BTreeMap::from([(MapKey::Str(Arc::from("left")), Value::Int(4)), (MapKey::Str(Arc::from("right")), Value::Int(7))]))),
            ] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern(name)));
                let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), expected);
            }
            let file = temp.path().join(format!("write-{recursive}"));
            let arguments = [Value::Path(crate::runtime::value::PathValue::new(file.as_os_str().as_bytes().to_vec()).unwrap()), Value::Str(Arc::from("written"))];
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(program.clone());
            let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("write_record")));
            let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
            assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap(), Value::ok(Value::Unit));
            assert_eq!(std::fs::read(&file).unwrap(), b"written");
        }
    });
}

#[test]
fn direct_native_record_arguments_refuse_changed_missing_and_foreign_fields_before_host_writes() {
    on_large_stack(|| {
        use std::os::unix::ffi::OsStrExt;
        let program = native_record_argument_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, source) = generic.native_call_sources().find(|(_, source)| !source.record_arguments.is_empty() && matches!(source.expected.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::FsWrite, .. })).unwrap();
            let field = source.record_arguments.iter().find(|field| field.field == Name::intern("path")).unwrap();
            let data = source.record_arguments.iter().find(|field| field.field == Name::intern("data")).unwrap();
            let foreign = generic.native_call_sources().filter(|(_, candidate)| candidate.owner != source.owner).flat_map(|(_, candidate)| candidate.record_arguments.iter()).find(|candidate| candidate.field == field.field).unwrap();
            let mut changed = program.clone();
            let range = changed.store.data[field.field_initializer as usize].range();
            let data_words = program.store.payload(program.store.data[data.field_initializer as usize].range()).unwrap();
            changed.store.extra[range.start as usize + 1] = data_words[1];
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_native_call_source_mut(id).unwrap().record_arguments = Box::new([]);
            let mut other = program.clone();
            other.store.extra[range.start as usize] = foreign.record_read;
            let mut missing_wrapper = program.clone();
            missing_wrapper.store.generic.as_deref_mut().unwrap().test_remove_original_compiler_argument_wrappers();
            let temp = tempfile::tempdir().unwrap();
            for (case, altered) in [("field", changed), ("receipt", missing), ("record", other), ("allocation", missing_wrapper)] {
                assert!(FullVerifier::verify(&altered).is_err());
                for recursive in [false, true] {
                    let file = temp.path().join(format!("{case}-{recursive}"));
                    std::fs::write(&file, b"kept").unwrap();
                    let arguments = [Value::Path(crate::runtime::value::PathValue::new(file.as_os_str().as_bytes().to_vec()).unwrap()), Value::Str(Arc::from("replaced"))];
                    let altered = Arc::new(altered.clone());
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*altered.sources).clone());
                    evaluator.indexed_program = Some(altered.clone());
                    let key = LoweredFunctionKey::Name(Name::intern("write_record"));
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Proc, &arguments, Span::new(altered.store.source_id, 0, 0)).unwrap();
                    let error = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err();
                    assert_eq!(error.kind, "indexed-ir", "a native spread proof refuses the host dispatch");
                    assert_eq!(std::fs::read(&file).unwrap(), b"kept");
                }
            }
        });
    });
}
