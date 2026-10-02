use super::*;
use super::tests::source_fixture;
use super::literal_comparison::PreparedComparisonLiteral;

#[test]
fn original_scalar_literal_comparisons_keep_typed_slots_and_literal_values_on_both_routes() {
    crate::runtime::eval::run_eval(|| {
        for (ty, literal, equal, unequal) in [
            ("Int", "7", crate::runtime::value::Value::Int(7), crate::runtime::value::Value::Int(8)),
            ("Str", "\"needle\"", crate::runtime::value::Value::Str("needle".into()), crate::runtime::value::Value::Str("other".into())),
            ("Bool", "true", crate::runtime::value::Value::Bool(true), crate::runtime::value::Value::Bool(false)),
        ] {
            for (operator, opcode) in [("==", BinaryOp::Eq), ("!=", BinaryOp::Ne)] {
                for reversed in [false, true] {
                    let expression = if reversed { format!("{literal} {operator} value") } else { format!("value {operator} {literal}") };
                    let saved_expression = expression.replace("value", "held");
                    let source = format!("pure compare(value: {ty}) -> Bool {{ {expression} }}\npure guarded(value: {ty}) -> Int {{ if {expression} {{ 1 }} else {{ 2 }} }}\npure preserve(value: {ty}) -> {ty} {{ value }}\npure saved(value: {ty}) -> Bool {{ let held: {ty} = preserve(value); if {saved_expression} {{ true }} else {{ false }} }}\npure control() -> Int {{ 3 }}\n");
                    let expected = PreparedLanguageOperation::Equality { op: opcode };
                    let program = Arc::new(source_fixture(&source, expected));
                    let foreign = source_fixture(&source, expected);
                    program.symbol_owner().with_current(|| {
                        let generic = program.generic_evidence().unwrap();
                        assert!(generic.operations().any(|(_, operation)| operation.literal_comparison_slot.as_ref().is_some_and(|recipe|
                            matches!(recipe.receiver, super::super::super::generic::NativeScalarReceiver::Binding { .. }))), "saved scalar condition retains its original immutable binding authority");
                        let (id, operation) = generic.operations().find(|(_, operation)| operation.literal_comparison_slot.is_some()).expect("literal equality keeps a genuine original slot proof");
                        let original = generic.operation_source(operation.source).unwrap();
                        let recipe = operation.literal_comparison_slot.as_ref().unwrap();
                        assert_eq!(program.store.tags[original.instruction as usize], FullTag::BoolLiteralCompareSlot);
                        assert!(operation.binding.operands.is_empty());
                        assert_eq!(recipe.argument, if reversed { 1 } else { 0 });
                        let mut missing = (*program).clone();
                        missing.store.generic.as_deref_mut().unwrap().test_remove_operations();
                        assert!(FullVerifier::verify(&missing).is_err());
                        let mut missing_recipe = (*program).clone();
                        missing_recipe.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().literal_comparison_slot = None;
                        assert!(FullVerifier::verify(&missing_recipe).is_err());
                        let mut other = (*program).clone();
                        other.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().source = foreign.generic_evidence().unwrap().operations().find(|(_, operation)| operation.literal_comparison_slot.is_some()).unwrap().1.source;
                        assert!(FullVerifier::verify(&other).is_err());
                        let mut changed_slot = (*program).clone();
                        let range = changed_slot.store.data[original.instruction as usize].range();
                        changed_slot.store.extra[range.start as usize + 1] += 1;
                        assert!(FullVerifier::verify(&changed_slot).is_err());
                        let replacement = if opcode == BinaryOp::Eq { BinaryOp::Ne } else { BinaryOp::Eq };
                        let mut changed_opcode = (*program).clone();
                        let operator_index = recipe.payload[0] as usize;
                        changed_opcode.store.binary_ops[operator_index] = replacement;
                        assert!(FullVerifier::verify(&changed_opcode).is_err());
                        let mut changed_literal = (*program).clone();
                        let value = recipe.payload[2] as usize;
                        let range = changed_literal.store.value_data[value].range();
                        match recipe.literal {
                            PreparedComparisonLiteral::Int(_) => changed_literal.store.extra[range.start as usize] += 1,
                            PreparedComparisonLiteral::Bool(_) => changed_literal.store.extra[range.start as usize] ^= 1,
                            PreparedComparisonLiteral::Str(_) => {
                                let string = IrStringId::from_raw(recipe.literal_payload[0]).unwrap();
                                let range = changed_literal.store.strings[string.index()];
                                changed_literal.store.string_bytes[range.start as usize] = b'x';
                            }
                            PreparedComparisonLiteral::Null => unreachable!(),
                        }
                        assert!(FullVerifier::verify(&changed_literal).is_err(), "literal pool mutation cannot rewrite a selected comparison");
                        let mut coforged = changed_opcode;
                        let proof = coforged.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap();
                        let PreparedOperationAuthority::Language { operation, .. } = &mut proof.authority else { unreachable!() };
                        *operation = PreparedLanguageOperation::Equality { op: replacement };
                        proof.literal_comparison_slot.as_mut().unwrap().payload[0] = operator_index as u32;
                        let source_id = proof.source;
                        let source = coforged.store.generic.as_deref_mut().unwrap().test_operation_source_mut(source_id).unwrap();
                        let PreparedOperationAuthority::Language { operation, .. } = &mut source.expected else { unreachable!() };
                        *operation = PreparedLanguageOperation::Equality { op: replacement };
                        assert!(FullVerifier::verify(&coforged).is_err(), "source, recipe and opcode cannot replace original selected equality authority");
                        for recursive in [false, true] {
                            for (value, equal_to_literal) in [(equal.clone(), true), (unequal.clone(), false)] {
                                let decision = if opcode == BinaryOp::Eq { equal_to_literal } else { !equal_to_literal };
                                for name in ["compare", "guarded", "saved", "control"] {
                                    let function = LoweredFunctionKey::Name(Name::intern(name));
                                    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                                    evaluator.indexed_program = Some(Arc::clone(&program));
                                    let arguments = if name == "control" { vec![] } else { vec![value.clone()] };
                                    let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).expect("scalar equality function exists");
                                    let value = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call).unwrap();
                                    let expected = match name { "compare" | "saved" => crate::runtime::value::Value::Bool(decision), "guarded" => crate::runtime::value::Value::Int(if decision { 1 } else { 2 }), _ => crate::runtime::value::Value::Int(3) };
                                    assert_eq!(value, expected);
                                }
                            }
                        }
                    });
                }
            }
        }
    });
}
