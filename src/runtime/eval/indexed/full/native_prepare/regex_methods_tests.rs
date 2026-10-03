use super::*;
use super::super::tests::{fixture, program_name, run_with_large_stack};
use crate::runtime::eval::Evaluator;
use crate::runtime::value::{RecordMap, Value};

const SOURCE: &str = r#"
pure retain(value) { value }
pure captures(text: Str) -> List[Str] { return retain(rx"([A-Z]+)-(\d+)".captures(text)) }
pure other_captures(text: Str) -> List[Str] { return retain(rx"([A-Z]+)-(\d+)".captures(text)) }
pure matches(text: Str) -> Bool { return retain(rx"([A-Z]+)-(\d+)".matches(text)) }
pure find(text: Str) { return retain(rx"[A-Z]+".find(text)) }
pure replace(text: Str) -> Str { return retain(rx"([A-Z]+)-(\d+)".replace(replacement: "$1:$2", text: text)) }
pure text_replace(text: Str) -> Str { return retain(text.replace(from: "ERR", to: "WARN")) }
"#;

fn regex_program() -> FullProgram { fixture("prepared-regex-methods.xsh", SOURCE) }

fn invoke(program: &Arc<FullProgram>, name: &str, text: &str, recursive: bool) -> Result<Value, crate::runtime::value::RuntimeError> {
    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
    evaluator.indexed_program = Some(program.clone());
    let key = LoweredFunctionKey::Name(program_name(program, name));
    let arguments = [Value::Str(Arc::from(text))];
    crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, || evaluator.call_indexed_direct(
        key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).expect("prepared regex worker exists"))
}

fn match_record(start: i64, end: i64, text: &str) -> Value {
    Value::Record(RecordMap::from([
        (Arc::from("start"), Value::Int(start)),
        (Arc::from("end"), Value::Int(end)),
        (Arc::from("text"), Value::Str(Arc::from(text))),
    ]))
}

#[test]
fn closed_regex_methods_keep_original_registry_receiver_modes_and_result_carriers_after_frontend_drop_on_both_routes() {
    run_with_large_stack(|| {
        let program = Arc::new(regex_program());
        program.symbol_owner().with_current(|| {
            let calls = program.generic_evidence().unwrap().ground_native_calls().filter(|(_, proof)|
                proof.contract.registry_owner == RegistryOwner::Method(crate::modules::signature::MethodReceiver::Regex)).collect::<Vec<_>>();
            assert_eq!(calls.len(), 5, "every authored closed Regex method has an independently prepared source receipt");
            for operation in [RuntimeOp::RegexCaptures, RuntimeOp::RegexMatches, RuntimeOp::RegexFind, RuntimeOp::RegexReplace] {
                let (_, proof) = calls.iter().find(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: actual, .. } if actual == operation)).unwrap();
                let receiver = proof.contract.receiver.as_ref().unwrap();
                let TypeRef::Ground(actual) = receiver.ty else { panic!("compiled Regex receiver is closed"); };
                assert_eq!(program.store.semantic.to_type(actual).unwrap(), Type::Regex);
                assert_eq!(proof.contract.kind, crate::runtime::eval::indexed::generic::CallableKind::Pure);
                assert_eq!(proof.contract.effects.creation, crate::sema::inference::EffectSet::EMPTY);
                assert!(proof.contract.binding.default_slots.is_empty());
                assert!(proof.contract.binding.rest_slot.is_none());
                assert!(proof.contract.binding.dynamic.is_none());
            }
            let replace = calls.iter().find(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::RegexReplace, .. })).unwrap().1;
            assert_eq!(replace.contract.binding.supplied_slots.as_ref(), &[2, 1]);
            assert_eq!(replace.contract.arguments[0].original.name, Some(Name::intern("replacement")));
            assert_eq!(replace.contract.arguments[1].original.name, Some(Name::intern("text")));
        });
        for recursive in [false, true] {
            for (name, text, expected) in [
                ("captures", "ERR-42", Value::List(vec![Value::Str(Arc::from("ERR-42")), Value::Str(Arc::from("ERR")), Value::Str(Arc::from("42"))])),
                ("captures", "no match", Value::List(vec![])),
                ("matches", "ERR-42", Value::Bool(true)),
                ("matches", "no match", Value::Bool(false)),
                ("find", "é ERR-42 OK-7", Value::List(vec![match_record(3, 6, "ERR"), match_record(10, 12, "OK")])),
                ("replace", "ERR-42 OK-7", Value::Str(Arc::from("ERR:42 OK:7"))),
            ] {
                assert_eq!(invoke(&program, name, text, recursive).unwrap(), expected);
            }
        }
    });
}

#[test]
fn closed_regex_methods_refuse_missing_foreign_and_jointly_rewritten_same_result_operation_sources() {
    run_with_large_stack(|| {
        let program = regex_program();
        let foreign_program = regex_program();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let captures = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::RegexCaptures, .. })).collect::<Vec<_>>();
            let (id, proof) = captures[0];
            let source = generic.native_call_source(proof.source).unwrap();
            let other_receiver = captures[1].1.contract.receiver.as_ref().unwrap();
            let range = program.store.data[source.instruction as usize].range();
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            let mut foreign_source = program.clone();
            foreign_source.store.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().source = foreign_program.generic_evidence().unwrap().ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority,
                PreparedOperationAuthority::Registry { operation: RuntimeOp::RegexCaptures, .. })).unwrap().1.source;
            let mut receiver = program.clone();
            receiver.store.extra[range.start as usize] = other_receiver.instruction;
            let evidence = receiver.store.generic.as_deref_mut().unwrap();
            evidence.test_ground_native_call_mut(id).unwrap().contract.receiver = Some(other_receiver.clone());
            evidence.test_ground_native_call_mut(id).unwrap().contract.argument_sources[0] = Some(other_receiver.instruction);
            let changed = evidence.ground_native_call(id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(proof.source).unwrap().expected = changed;
            for changed in [missing, foreign_source, receiver] {
                assert!(FullVerifier::verify(&changed).is_err());
                let changed = Arc::new(changed);
                for recursive in [false, true] {
                    assert_eq!(invoke(&changed, "captures", "ERR-42", recursive).unwrap_err().kind, "indexed-ir");
                }
            }
            let (replace_id, replace) = generic.ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::RegexReplace, .. })).unwrap();
            let text_replace = generic.ground_native_calls().find(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TextReplace, .. })).unwrap().1;
            let replace_source = generic.native_call_source(replace.source).unwrap();
            let TypeRef::Ground(regex_result) = replace.contract.result else { panic!("Regex replacement result is closed"); };
            let TypeRef::Ground(text_result) = text_replace.contract.result else { panic!("text replacement result is closed"); };
            assert_eq!(program.store.semantic.to_type(regex_result).unwrap(), program.store.semantic.to_type(text_result).unwrap());
            let mut rewritten = program.clone();
            let evidence = rewritten.store.generic.as_deref_mut().unwrap();
            let changed = &mut evidence.test_ground_native_call_mut(replace_id).unwrap().contract;
            changed.authority = text_replace.contract.authority.clone();
            changed.registry_owner = text_replace.contract.registry_owner;
            changed.signature = text_replace.contract.signature;
            changed.receiver.as_mut().unwrap().ty = text_replace.contract.receiver.as_ref().unwrap().ty;
            changed.receiver.as_mut().unwrap().source_type = text_replace.contract.receiver.as_ref().unwrap().source_type;
            let contract = evidence.ground_native_call(replace_id).unwrap().contract.clone();
            evidence.test_native_call_source_mut(replace.source).unwrap().expected = contract;
            assert_eq!(rewritten.store.payload(rewritten.store.data[replace_source.instruction as usize].range()).unwrap(), program.store.payload(program.store.data[replace_source.instruction as usize].range()).unwrap(), "both selected operations use the same authored replace packet and Str result shape");
            let failure = FullVerifier::verify(&rewritten).unwrap_err();
            assert!(failure.message.contains("original receipt"), "agreement among public same-result operation and source copies cannot replace the sealed registry selection: {}", failure.message);
            let rewritten = Arc::new(rewritten);
            for recursive in [false, true] {
                assert_eq!(invoke(&rewritten, "replace", "ERR-42", recursive).unwrap_err().kind, "indexed-ir");
            }
        });
    });
}
