use super::*;
use super::super::super::generic::PreparedNativeArgumentLineage;
use crate::runtime::eval::indexed::TypeId as GroundTypeId;

impl FullBuilder {
    pub(in crate::runtime::eval::indexed::full) fn prepare_native_argument_lineages(&self,
        arguments: &mut [PreparedInvocationArgument], owner: InstructionOwner,
        signature: SignatureId, supplied_slots: &[u32],
    ) -> Result<Box<[PreparedNativeArgumentLineage]>, IrBuildError> {
        if arguments.len() != supplied_slots.len() { return Err(native_problem("native_argument_lineage_slot_count")); }
        arguments.iter_mut().zip(supplied_slots).enumerate().map(|(ordinal, (argument, &slot))| {
            let (source_instruction, wrappers) = self.argument_initializer_lineage(argument.instruction, owner)?;
            let source_type = argument.ty;
            let formal = self.store.semantic.signature_param(signature, slot as usize)
                .map_err(|_| native_problem("native_argument_original_formal"))?.1;
            let material_type = if let Some(wrapper) = wrappers.iter().find(|wrapper| matches!(wrapper.kind, super::super::super::generic::ValueInitializerWrapperKind::CheckedValue)) {
                let check = checked_wrapper_type(&self.store, wrapper).map_err(|cause| IrBuildError::verification("native_argument_original_unsigned_check", cause))?;
                if check != formal { return Err(native_problem("native_argument_unsigned_check_changes_selected_formal")); }
                TypeRef::Ground(check)
            } else { source_type };
            argument.ty = material_type;
            Ok(PreparedNativeArgumentLineage {
                ordinal: u32::try_from(ordinal).map_err(|_| native_problem("native_argument_lineage_ordinal_overflow"))?,
                instruction: argument.instruction, source_instruction, source_type, material_type, wrappers,
            })
        }).collect()
    }
}

fn checked_wrapper_type(store: &FullStore, wrapper: &super::super::super::generic::ValueInitializerWrapper) -> Result<GroundTypeId, IrVerifyError> {
    let mut cursor = FullCursor::new(&wrapper.payload);
    cursor.raw()?;
    let decoder = FullDecoder {
        store, owner: 0, instruction_range: 0..store.tags.len(), instruction_states: None,
        block_states: None, slot_count: 0, pattern_ceiling: Cell::new(0), pattern_tree: None, verified: false,
    };
    let check = LoweredTypeCheck::decode(&decoder, &mut cursor)?;
    Span::decode(&decoder, &mut cursor)?;
    cursor.finish()?;
    if !check.ty.has_unsigned_constraint() || check.schema.is_some() {
        return Err(IrVerifyError::new("native checked argument has no original unsigned predicate"));
    }
    GroundTypeId::from_raw(wrapper.payload[1]).ok_or_else(|| IrVerifyError::new("native checked argument has an invalid type descriptor"))
}

impl FullVerifier {
    pub(in crate::runtime::eval::indexed::full) fn verify_native_argument_lineages(store: &FullStore,
        generic: &GenericEvidenceStore, source: &NativeCallSource,
    ) -> Result<(), IrVerifyError> {
        if source.argument_lineages.len() != source.expected.arguments.len() {
            return Err(IrVerifyError::new("native call loses its original supplied argument lineage"));
        }
        for (ordinal, (lineage, argument)) in source.argument_lineages.iter().zip(&source.expected.arguments).enumerate() {
            if lineage.ordinal as usize != ordinal || lineage.instruction != argument.instruction || lineage.material_type != argument.ty {
                return Err(IrVerifyError::new("native call changes its original supplied argument lineage"));
            }
            Self::verify_argument_initializer_lineage(store, generic, lineage.instruction,
                lineage.source_instruction, &lineage.wrappers, source.owner)?;
            let checked = lineage.wrappers.iter().find(|wrapper| matches!(wrapper.kind, super::super::super::generic::ValueInitializerWrapperKind::CheckedValue));
            if let Some(wrapper) = checked {
                let target = checked_wrapper_type(store, wrapper)?;
                let slot = *source.expected.binding.supplied_slots.get(ordinal).ok_or_else(|| IrVerifyError::new("native checked argument loses its selected formal slot"))?;
                if lineage.material_type != TypeRef::Ground(target)
                    || target != store.semantic.signature_param(source.expected.signature, slot as usize)?.1 {
                    return Err(IrVerifyError::new("native checked argument changes its original selected unsigned target"));
                }
            } else if lineage.source_type != lineage.material_type {
                return Err(IrVerifyError::new("native argument narrowing lacks its original checked wrapper"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::Evaluator;
    use crate::runtime::value::Value;
    use crate::sema::operation_graph::PreparedLanguageOperation;

    fn fixture() -> FullProgram {
        super::super::super::operation_prepare::tests::source_fixture("pure push(values: List[UInt], value: Int) -> List[UInt] { values.push(value) }\npure other(values: List[UInt], value: Int) -> List[UInt] { values.push(value) }\npure comparison(left: Int, right: Int) -> Bool { left == right }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
    }

    #[test]
    fn direct_native_unsigned_argument_keeps_authored_int_and_checked_uint_on_both_routes() {
        crate::runtime::eval::run_eval(|| {
            let program = Arc::new(fixture());
            program.symbol_owner().with_current(|| {
                let generic = program.generic_evidence().unwrap();
                let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::ListPush, .. })).collect::<Vec<_>>();
                assert_eq!(calls.len(), 2);
                for (_, proof) in calls {
                    let source = generic.native_call_source(proof.source).unwrap();
                    let [lineage] = source.argument_lineages.as_ref() else { panic!("one checked supplied item") };
                    let (TypeRef::Ground(original), TypeRef::Ground(material)) = (lineage.source_type, lineage.material_type) else { panic!("closed source and checked material") };
                    assert_eq!(program.store.semantic.to_type(original).unwrap(), Type::Int);
                    assert_eq!(program.store.semantic.to_type(material).unwrap(), Type::UInt);
                    assert_eq!(proof.contract.arguments[0].ty, lineage.material_type);
                    assert!(lineage.wrappers.iter().any(|wrapper| matches!(wrapper.kind, super::super::super::super::generic::ValueInitializerWrapperKind::CheckedValue)));
                }
            });
            for recursive in [false, true] {
                for value in [3, -1] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let key = program.symbol_owner().with_current(|| LoweredFunctionKey::Name(Name::intern("push")));
                    let arguments = [Value::List(vec![Value::Int(1)]), Value::Int(value)];
                    let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).unwrap();
                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call);
                    if value >= 0 { assert_eq!(result.unwrap(), Value::List(vec![Value::Int(1), Value::Int(value)])); }
                    else { assert_eq!(result.unwrap_err().kind, "type-error"); }
                }
            }
        });
    }

    #[test]
    fn direct_native_unsigned_argument_refuses_removed_foreign_and_joint_wrapper_rewrites() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture();
            program.symbol_owner().with_current(|| {
                let generic = program.generic_evidence().unwrap();
                let calls = generic.ground_native_calls().filter(|(_, proof)| matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::ListPush, .. })).collect::<Vec<_>>();
                let (id, proof) = calls[0];
                let source = generic.native_call_source(proof.source).unwrap();
                let lineage = &source.argument_lineages[0];
                let mut removed = program.clone();
                removed.store.tags[lineage.instruction as usize] = FullTag::ExprParam;
                removed.store.data[lineage.instruction as usize] = removed.store.data[lineage.source_instruction as usize];
                let mut foreign = program.clone();
                foreign.store.generic.as_deref_mut().unwrap().test_native_call_source_mut(proof.source).unwrap().argument_lineages[0] = generic.native_call_source(calls[1].1.source).unwrap().argument_lineages[0].clone();
                let mut joint = program.clone();
                let TypeRef::Ground(original) = lineage.source_type else { panic!("authored Int") };
                let range = joint.store.data[lineage.instruction as usize].range();
                joint.store.extra[range.start as usize + 1] = original.raw();
                let evidence = joint.store.generic.as_deref_mut().unwrap();
                evidence.test_ground_native_call_mut(id).unwrap().contract.arguments[0].ty = lineage.source_type;
                let changed = evidence.ground_native_call(id).unwrap().contract.clone();
                let source = evidence.test_native_call_source_mut(proof.source).unwrap();
                source.expected = changed;
                source.argument_lineages[0].material_type = lineage.source_type;
                source.argument_lineages[0].wrappers.iter_mut().find(|wrapper| matches!(wrapper.kind,
                    super::super::super::super::generic::ValueInitializerWrapperKind::CheckedValue)).unwrap().payload[1] = original.raw();
                for changed in [removed, foreign, joint] {
                    assert!(FullVerifier::verify(&changed).is_err());
                    for recursive in [false, true] {
                        let changed = Arc::new(changed.clone());
                        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*changed.sources).clone());
                        evaluator.indexed_program = Some(Arc::clone(&changed));
                        let key = LoweredFunctionKey::Name(Name::intern("push"));
                        let arguments = [Value::List(vec![Value::Int(1)]), Value::Int(-1)];
                        let call = || evaluator.call_indexed_direct(key, LoweredFunctionKind::Pure, &arguments, Span::new(changed.store.source_id, 0, 0)).unwrap();
                        assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(key, recursive, call).unwrap_err().kind, "indexed-ir");
                    }
                }
            });
        });
    }
}
