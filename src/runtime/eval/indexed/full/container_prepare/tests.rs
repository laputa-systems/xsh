use super::*;
use crate::runtime::eval::Evaluator;
use crate::runtime::value::Value;
use crate::map_key::MapKey;
use crate::sema::operation_graph::PreparedLanguageOperation;

fn on_large_stack(work: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(work).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}

fn fixture() -> FullProgram {
    std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| super::super::operation_prepare::tests::source_fixture(
        "pure named(value: Int) -> Map[Str, List[Int]] { {left: [value]} }\npure computed(key: Str, value: Int) -> Map[Str, List[Int]] { {[key]: [value]} }\npure empty() -> List[List[Int]] { [] }\npure nested(value: Int) -> List[List[Int]] { [[value], []] }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n",
        PreparedLanguageOperation::Equality { op: BinaryOp::Eq })).unwrap().join().unwrap_or_else(|payload| std::panic::resume_unwind(payload))
}

fn checked_operand_fixture() -> FullProgram {
    super::super::operation_prepare::tests::source_fixture(
        "error ContainerFailure = Invalid(values: List[UInt]) | InvalidOptional(values: List[UInt?])\npure checked(value: Int) -> Int { let rejected = ContainerFailure.Invalid(values: [value]); 0 }\npure checked_optional(value: Int) -> Int { let rejected = ContainerFailure.InvalidOptional(values: [value]); 0 }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n",
        PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

fn list_build_fixture() -> FullProgram {
    super::super::operation_prepare::tests::source_fixture(
        "pure spliced(before: Int, middle: List[Int], after: Int) -> List[Int] { [before, @[], @middle, after] }\npure joined(middle: List[Str]) -> Str { [\"left\", @middle, \"right\"].join(\":\") }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n",
        PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
}

#[test]
fn list_build_declared_erasure_retains_original_finite_splice_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(super::super::operation_prepare::tests::source_fixture(
            "pure erased(middle: List[Str]) -> List[Any] { [1, @middle, true] }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n",
            PreparedLanguageOperation::Equality { op: BinaryOp::Eq }));
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, source) = generic.ground_containers().find(|(_, source)| source.operands.len() == 3).unwrap();
            assert_eq!(program.store.semantic.to_type(source.result).unwrap(), crate::sema::types::Type::List(Box::new(crate::sema::types::Type::Any)));
            let splice = &source.operands[1];
            assert_eq!(splice.role, ContainerOperandRole::ListSplice(1));
            assert_eq!(program.store.semantic.to_type(splice.source_type).unwrap(), crate::sema::types::Type::List(Box::new(crate::sema::types::Type::Str)));
            assert_eq!(splice.ty, splice.source_type);
            let mut changed = program.store.clone();
            changed.generic.as_deref_mut().unwrap().test_ground_container_mut(id).unwrap().operands[1].source_type = source.result;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "erasure cannot replace the original finite producer type");
        });
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                let key = LoweredFunctionKey::Name(Name::intern("erased"));
                let arguments = [Value::List(vec![Value::Str(Arc::from("middle"))])];
                let execute = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                let actual = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, execute);
                assert_eq!(actual.unwrap(), Value::List(vec![Value::Int(1), Value::Str(Arc::from("middle")), Value::Bool(true)]));
            });
        }
    });
}

#[test]
fn list_build_original_scalar_and_finite_splice_producers_execute_both_routes_after_frontend_disposal() {
    on_large_stack(|| {
        let program = Arc::new(list_build_fixture());
        let source = program.generic_evidence().unwrap().ground_containers().find(|(_, source)| source.operands.len() == 4 && source.operands[1].role == ContainerOperandRole::ListSplice(1)).unwrap().1;
        assert_eq!(source.operands.iter().map(|operand| operand.role).collect::<Vec<_>>(), [ContainerOperandRole::ListItem(0), ContainerOperandRole::ListSplice(1), ContainerOperandRole::ListSplice(2), ContainerOperandRole::ListItem(3)]);
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                for (name, arguments, expected) in [
                    ("spliced", vec![Value::Int(1), Value::List(vec![Value::Int(2), Value::Int(3)]), Value::Int(4)], Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4)])),
                    ("joined", vec![Value::List(vec![Value::Str(Arc::from("middle"))])], Value::Str(Arc::from("left:middle:right"))),
                ] {
                    let key = LoweredFunctionKey::Name(Name::intern(name));
                    let execute = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    let actual = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, execute);
                    assert_eq!(actual.unwrap(), expected);
                }
            });
        }
    });
}

#[test]
fn list_build_original_producers_reject_flags_order_children_and_agreeing_receipt_rewrites() {
    on_large_stack(|| {
        let program = list_build_fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, source) = generic.ground_containers().find(|(_, source)| source.operands.len() == 4 && source.operands[1].role == ContainerOperandRole::ListSplice(1)).unwrap();
            let block = IrBlockId::from_raw(source.instruction_payload[0]).unwrap();
            let range = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();
            let mut changed_flag = program.store.clone();
            changed_flag.extra[range.start + 1 + 3] = 0;
            assert!(FullVerifier::verify_generic_evidence(&changed_flag).is_err(), "an original finite splice cannot become a scalar item through a flag rewrite");
            let receipt = changed_flag.generic.as_deref_mut().unwrap().test_ground_container_mut(id).unwrap();
            receipt.block_payload[4] = 0;
            receipt.operands[1].role = ContainerOperandRole::ListItem(1);
            assert!(FullVerifier::verify_generic_evidence(&changed_flag).is_err(), "agreeing flag and role copies cannot replace the protected original splice receipt");
            let mut reordered = program.store.clone();
            reordered.extra[range.start + 2 + 3] = source.operands[2].instruction;
            reordered.extra[range.start + 2 + 6] = source.operands[1].instruction;
            assert!(FullVerifier::verify_generic_evidence(&reordered).is_err(), "same-typed finite producers cannot exchange original evaluation order");
            let mut item_as_splice = program.store.clone();
            item_as_splice.extra[range.start + 2 + 3] = source.operands[0].instruction;
            assert!(FullVerifier::verify_generic_evidence(&item_as_splice).is_err(), "an Int item cannot acquire finite List authority");
            let mut changed_type = program.store.clone();
            changed_type.generic.as_deref_mut().unwrap().test_ground_container_mut(id).unwrap().operands[0].ty = source.result;
            assert!(FullVerifier::verify_generic_evidence(&changed_type).is_err(), "a scalar producer cannot acquire its container's checked type");
        });
    });
}

#[test]
fn ground_container_checked_operand_keeps_original_source_and_validation_on_both_routes() {
    on_large_stack(|| {
        let program = Arc::new(checked_operand_fixture());
        assert!(program.generic_evidence().unwrap().ground_containers().any(|(_, source)| source.creation_check.is_some() && source.operands.iter().any(|operand| operand.source_type == operand.ty)));
        for recursive in [false, true] {
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(program.clone());
                for name in ["checked", "checked_optional"] {
                let key = LoweredFunctionKey::Name(Name::intern(name));
                for (value, succeeds) in [(3, true), (-1, false)] {
                    let args = [Value::Int(value)];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &args, Span::new(program.store.source_id, 0, 0)).unwrap();
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call);
                    if succeeds { assert_eq!(result.unwrap(), Value::Int(0)); } else { assert!(result.is_err()); }
                }
                }
            });
        }
    });
}

#[test]
fn ground_container_checked_operand_rejects_validation_and_joint_source_rewrites() {
    on_large_stack(|| {
        let program = checked_operand_fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, source) = generic.ground_containers().find(|(_, source)| source.creation_check.is_some()).unwrap();
            let operand = &source.operands[0];
            let wrapper = source.creation_check.as_ref().unwrap();
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_ground_container_mut(id).unwrap().creation_check = None;
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err(), "whole-value validation remains part of the original construction receipt");
            let mut opcode = program.store.clone();
            opcode.tags[wrapper.instruction as usize] = FullTag::ExprList;
            assert!(FullVerifier::verify_generic_evidence(&opcode).is_err(), "raw construction cannot replace validation");
            let mut changed = program.store.clone();
            let range = changed.data[wrapper.instruction as usize].range();
            changed.extra[range.start as usize + 1] = operand.source_type.raw();
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err());
            let receipt = changed.generic.as_deref_mut().unwrap().test_ground_container_mut(id).unwrap();
            receipt.creation_check.as_mut().unwrap().payload[1] = operand.source_type.raw();
            receipt.result = operand.source_type;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "rewritten material and proof cannot replace the original operand receipt");
        });
    });
}

#[test]
fn ground_container_original_operands_execute_after_frontend_disposal_on_both_routes() {
    on_large_stack(|| {
    let program = Arc::new(fixture());
    assert!(program.generic_evidence().unwrap().ground_containers().count() >= 7);
    for recursive in [false, true] {
        program.symbol_owner().with_current(|| {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            for (name, arguments, expected) in [
                ("named", vec![Value::Int(4)], Value::Map(BTreeMap::from([(MapKey::Str(Arc::from("left")), Value::List(vec![Value::Int(4)]))]))),
                ("computed", vec![Value::Str(Arc::from("right")), Value::Int(7)], Value::Map(BTreeMap::from([(MapKey::Str(Arc::from("right")), Value::List(vec![Value::Int(7)]))]))),
                ("empty", vec![], Value::List(vec![])),
                ("nested", vec![Value::Int(9)], Value::List(vec![Value::List(vec![Value::Int(9)]), Value::List(vec![])])),
            ] {
                let mut execute = || evaluator.call_indexed_direct(LoweredFunctionKey::Name(Name::intern(name)), LoweredFunctionKind::Pure,
                    &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                let actual = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(LoweredFunctionKey::Name(Name::intern(name)), recursive, execute);
                assert_eq!(actual.unwrap(), expected);
            }
        });
    }
    });
}

#[test]
fn ground_container_original_operands_reject_missing_changed_and_foreign_receipts() {
    on_large_stack(|| {
    let program = fixture();
    let foreign = fixture();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (id, source) = generic.ground_containers().find(|(_, source)| !source.operands.is_empty()).unwrap();
        let mut missing = program.store.clone();
        missing.generic.as_deref_mut().unwrap().test_remove_ground_containers();
        assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
        let mut opcode = program.store.clone();
        opcode.tags[source.instruction as usize] = FullTag::ExprInt;
        assert!(FullVerifier::verify_generic_evidence(&opcode).is_err());
        let mut receipt = program.store.clone();
        receipt.generic.as_deref_mut().unwrap().test_ground_container_mut(id).unwrap().operands[0].instruction = source.instruction;
        assert!(FullVerifier::verify_generic_evidence(&receipt).is_err(), "rewriting the mutable proof cannot replace its original receipt");
        let foreign_id = foreign.generic_evidence().unwrap().ground_containers().next().unwrap().0;
        assert!(generic.ground_container_source(foreign_id).is_err());
    });
    });
}

#[test]
fn ground_container_original_operands_reject_retired_handles_and_replaced_checkpoints_atomically() {
    on_large_stack(|| {
    let program = fixture();
    program.symbol_owner().with_current(|| {
        let generic = program.generic_evidence().unwrap();
        let (foreign, source) = generic.ground_containers().find(|(_, source)| source.operands.iter().all(|operand| matches!(operand.origin, ContainerOperandOrigin::Expression(_)))).unwrap();
        let mut builder = super::super::super::generic::GenericEvidenceBuilder::default();
        let mut source = source.clone();
        source.scope = None;
        for (instruction, origin) in std::iter::once((source.instruction, source.origin)).chain(source.operands.iter().map(|operand| {
            let ContainerOperandOrigin::Expression(origin) = operand.origin else { unreachable!() }; (operand.instruction, origin)
        })) {
            builder.register_instruction_origin(instruction, super::super::super::generic::OperationSourceOrigin::Expression(origin), source.owner).unwrap();
        }
        let checkpoint = builder.checkpoint();
        let retired = builder.add_ground_container(source.clone()).unwrap();
        let stale = builder.checkpoint();
        builder.rewind(checkpoint).unwrap();
        let replacement = builder.add_ground_container(source).unwrap();
        assert!(builder.rewind(stale).is_err());
        let owners = program.store.generic_instruction_owners().unwrap();
        let mut store = builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap();
        assert!(store.ground_container_source(retired).is_err());
        assert!(store.ground_container_source(foreign).is_err());
        assert!(store.ground_container_source(replacement).is_ok(), "a rejected stale checkpoint preserves replacement evidence");
        let bytes = store.retained_bytes();
        store.shrink_to_fit();
        assert!(store.retained_bytes() <= bytes);
        assert!(store.ground_container_source(replacement).is_ok());
    });
    });
}
