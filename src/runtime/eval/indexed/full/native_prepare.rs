use super::*;
use super::super::generic::{GroundNativeCallContract, NativeCallSource, OperationSourceOrigin, PreparedGroundNativeCall, PreparedInvocationArgument, PreparedOperationAuthority, PreparedOperationBinding, PreparedOperationEffects, graph_ground_type};
use crate::sema::inference::{EffectSummary, OperationBinding, RequirementTemplate, TypeNode};
use crate::sema::registry_graph::RegistryOwner;
use crate::modules::signature::{ImplBinding, SemanticRule};

fn native_problem(construct: &'static str) -> IrBuildError { IrBuildError::format(construct, None, 0, 0) }

fn encoded_native_arguments(store: &FullStore, instruction: u32, parameter_count: usize) -> Result<(RuntimeOp, Vec<Option<u32>>, u32), IrVerifyError> {
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprModuleCall) { return Err(IrVerifyError::new("native call proof is attached to another opcode")); }
    let words = store.payload(store.data[instruction as usize].range())?;
    if words.len() != 4 || words[1] != 0 { return Err(IrVerifyError::new("native call descriptor protocol is not prepared")); }
    let operation = *store.runtime_ops.get(words[0] as usize).ok_or_else(|| IrVerifyError::new("native call operation is invalid"))?;
    let block = IrBlockId::from_raw(words[2]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("native call argument block is invalid"))?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("native call argument block has another kind")); }
    let mut cursor = FullCursor::new(store.payload(block.instructions)?);
    let count = cursor.raw()? as usize;
    if count > parameter_count || parameter_count > 65536 { return Err(IrVerifyError::new("native call argument count exceeds its original signature")); }
    let mut sources = vec![None; parameter_count];
    for source in &mut sources[..count] {
        *source = match cursor.raw()? { 0 => None, 1 => Some(cursor.raw()?), _ => return Err(IrVerifyError::new("native call optional argument is invalid")) };
    }
    cursor.finish()?;
    Ok((operation, sources, words[3]))
}

impl FullBuilder {
    pub(super) fn prepare_native_calls(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        for (instruction, expression, owner) in self.generic_expression_rows.clone() {
            if self.store.tags[instruction as usize] != FullTag::ExprModuleCall { continue; }
            let Some(operation) = solved.operations.get(&expression) else { continue; };
            let graph = &solved.graph;
            let Some(selected) = graph.candidate_evidence(operation.requirement).map_err(|_| native_problem("native_candidate_owner"))? else { continue; };
            let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).map_err(|_| native_problem("native_candidate_authority"))? else { continue; };
            if !matches!(metadata.owner, RegistryOwner::Module(_)) || metadata.binding != ImplBinding::Native
                || metadata.semantic_rule != SemanticRule::Standard { continue; }
            let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation.requirement).map_err(|_| native_problem("native_requirement_owner"))? else { return Err(native_problem("native_requirement_kind")); };
            let call = graph.operation_call(call).map_err(|_| native_problem("native_call_owner"))?;
            if call.binding != OperationBinding::Slots || call.receiver.is_some() || operation.receiver.is_some()
                || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
                || !operation.argument_coercions.is_empty() || !selected.callback_invocations.is_empty() { continue; }
            let signature = graph.resolved(selected.signature).map_err(|_| native_problem("native_signature_owner"))?;
            let TypeNode::Arrow(arrow) = graph.node(signature).map_err(|_| native_problem("native_signature_owner"))? else { return Err(native_problem("native_signature_kind")); };
            if arrow.kind != metadata.kind || arrow.params.len() != metadata.parameters.len()
                || arrow.params.iter().zip(&metadata.parameters).any(|(formal, original)| formal.label != original.label || formal.defaulted != original.defaulted || formal.rest) {
                return Err(native_problem("native_original_parameter_contract"));
            }
            // A closed native result does not make unresolved operands ground.
            // Such a call needs its own scoped argument protocol.
            if arrow.params.iter().map(|parameter| parameter.ty).chain(std::iter::once(selected.result))
                .chain(operation.actual_arguments.iter().copied()).any(|ty| graph_ground_type(graph, ty).is_err()) { continue; }
            let closed = |summary| match graph.closed_effect_summary(summary).map_err(|_| native_problem("native_effect_owner"))? {
                EffectSummary::Closed(bits) => Ok(bits), _ => Err(native_problem("native_effect_scope_not_prepared")),
            };
            let effects = PreparedOperationEffects {
                creation: closed(selected.effects)?,
                inputs: call.effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(),
                outputs: call.output_effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(),
            };
            let descriptor = self.intern_checked_callable_type(graph, signature)?;
            let (kind, signature) = self.store.semantic.callable_descriptor(descriptor).map_err(|_| native_problem("native_signature_descriptor"))?.ok_or_else(|| native_problem("native_signature_descriptor"))?;
            let result_type = graph_ground_type(graph, selected.result).map_err(|_| native_problem("native_result_scope"))?;
            let result = self.intern_generic_ground_type(&result_type)?;
            if self.store.semantic.signature_return_type(signature).map_err(|_| native_problem("native_signature_result"))? != result {
                return Err(native_problem("native_original_result_contract"));
            }
            let original_result = solved.expressions.get(&expression).copied().ok_or_else(|| native_problem("native_original_result_missing"))?;
            if graph_ground_type(graph, original_result).map_err(|_| native_problem("native_original_result_scope"))? != result_type { return Err(native_problem("native_original_result_changed")); }
            let (encoded_operation, argument_sources, location) = encoded_native_arguments(&self.store, instruction, arrow.params.len()).map_err(|_| native_problem("native_encoded_arguments"))?;
            if encoded_operation != metadata.operation || self.store.location_sources.get(location as usize) != Some(&expression.source) { return Err(native_problem("native_original_opcode_or_source")); }
            let recipes = solved.argument_sources.get(&expression).ok_or_else(|| native_problem("native_original_recipes_missing"))?;
            if recipes.len() != operation.binding.supplied_slots.len() || recipes.len() != operation.actual_arguments.len() || selected.actual_arguments.len() != arrow.params.len() { return Err(native_problem("native_original_binding_count")); }
            let mut arguments = Vec::with_capacity(recipes.len());
            let mut operands = Vec::with_capacity(recipes.len());
            for (ordinal, ((recipe, &slot), &checked)) in recipes.iter().zip(&operation.binding.supplied_slots).zip(&operation.actual_arguments).enumerate() {
                let crate::sema::arguments::ArgumentValueSource::Expression(value) = recipe.value else { return Err(native_problem("native_argument_recipe_not_prepared")); };
                let argument = argument_sources.get(slot).copied().flatten().ok_or_else(|| native_problem("native_supplied_operand_missing"))?;
                let actual = self.original_argument_expression(argument, expression, ordinal, recipe, owner)?;
                if actual != (crate::sema::check::ExpressionIdentity { expression: value, ..expression }) { return Err(native_problem("native_operand_original_source_changed")); }
                let original = solved.expressions.get(&actual).copied().ok_or_else(|| native_problem("native_operand_original_type_missing"))?;
                let original = graph_ground_type(graph, original).map_err(|_| native_problem("native_operand_original_type_scope"))?;
                let selected_argument = selected.actual_arguments.get(slot).copied().flatten().ok_or_else(|| native_problem("native_selected_operand_missing"))?;
                if graph_ground_type(graph, checked).map_err(|_| native_problem("native_operand_type_scope"))? != original
                    || graph_ground_type(graph, selected_argument).map_err(|_| native_problem("native_selected_operand_scope"))? != original { return Err(native_problem("native_original_operand_type_changed")); }
                let ty = TypeRef::Ground(self.intern_generic_ground_type(&original)?);
                arguments.push(PreparedInvocationArgument { original: recipe.clone(), instruction: argument, ty });
                operands.push(argument);
            }
            for (slot, argument) in argument_sources.iter().enumerate() {
                if argument.is_some() != operation.binding.supplied_slots.contains(&slot)
                    || argument.is_none() != operation.binding.default_slots.contains(&slot)
                    || argument.is_none() != selected.actual_arguments[slot].is_none() { return Err(native_problem("native_encoded_binding_changed")); }
            }
            let slots = |slots: &[usize]| slots.iter().map(|&slot| u32::try_from(slot).map_err(|_| native_problem("native_slot_overflow"))).collect::<Result<Box<[_]>, _>>();
            let template = graph.candidate(selected.candidate).map_err(|_| native_problem("native_original_candidate_template"))?;
            let contract = GroundNativeCallContract {
                authority: PreparedOperationAuthority::Registry { identity: metadata.identity, operation: metadata.operation, binding: metadata.binding,
                    argument_check: metadata.argument_check, semantic_rule: metadata.semantic_rule, lifecycle: metadata.lifecycle, producer_transfer: metadata.producer_transfer.clone() },
                registry_owner: metadata.owner, signature, kind, result: TypeRef::Ground(result), effects, argument_relations: template.argument_relations.clone().into_boxed_slice(), input_eligibility: template.actual_eligibility.clone().into_boxed_slice(), arguments: arguments.into_boxed_slice(),
                binding: PreparedOperationBinding { supplied_slots: slots(&operation.binding.supplied_slots)?, default_slots: slots(&operation.binding.default_slots)?, rest_slot: None, dynamic: None, operands: operands.into_boxed_slice() },
                argument_sources: argument_sources.into_boxed_slice(),
            };
            let scope = operation.caller.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            let source = self.generic_evidence_mut().add_native_call_source(NativeCallSource { origin: expression, instruction, owner, scope, expected: contract.clone() }).map_err(|_| native_problem("native_source_allocation"))?;
            self.generic_evidence_mut().add_ground_native_call(PreparedGroundNativeCall { source, contract }).map_err(|_| native_problem("native_proof_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(super) fn native_call_result(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner) -> Result<Type, IrVerifyError> {
        let id = generic.ground_native_call_at(instruction)?.ok_or_else(|| IrVerifyError::new("original native call lacks its prepared proof"))?;
        let proof = generic.ground_native_call(id)?;
        let source = generic.native_call_source(proof.source)?;
        if source.instruction != instruction || source.owner != owner
            || generic.registered_instruction_origin(instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), owner)) { return Err(IrVerifyError::new("native call changes its original instruction or owner")); }
        let PreparedOperationAuthority::Registry { operation, .. } = proof.contract.authority else { return Err(IrVerifyError::new("native call lacks its selected registry authority")); };
        let count = store.semantic.signature_param_count(proof.contract.signature)?;
        let (encoded, arguments, location) = encoded_native_arguments(store, instruction, count)?;
        if encoded != operation || arguments.as_slice() != proof.contract.argument_sources.as_ref()
            || store.location_sources.get(location as usize) != Some(&source.origin.source) { return Err(IrVerifyError::new("native call changes its original opcode, operands, defaults, or source")); }
        let TypeRef::Ground(result) = proof.contract.result else { return Err(IrVerifyError::new("native call result is not closed")); };
        store.semantic.to_type(result)
    }

    pub(super) fn verify_native_call_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        if Self::native_call_result(store, generic, instruction, owner)? != *expected { return Err(IrVerifyError::new("native call result disagrees with its consumer")); }
        let id = generic.ground_native_call_at(instruction)?.unwrap();
        let proof = generic.ground_native_call(id)?;
        let already_active = active.last() == Some(&instruction);
        if active.len() >= 256 || (!already_active && active.contains(&instruction)) { return Err(IrVerifyError::new("native call operands are cyclic or too deep")); }
        if !already_active { active.push(instruction); }
        for argument in &proof.contract.arguments {
            let TypeRef::Ground(ty) = argument.ty else { return Err(IrVerifyError::new("native call operand requires a scoped protocol")); };
            Self::verify_generic_source(store, generic, argument.instruction, owner, &store.semantic.to_type(ty)?, None, active)?;
        }
        if !already_active { active.pop(); }
        Ok(())
    }

    pub(super) fn verify_native_calls(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for (_, proof) in generic.ground_native_calls() {
            let source = generic.native_call_source(proof.source)?;
            let result = Self::native_call_result(store, generic, source.instruction, source.owner)?;
            Self::verify_native_call_operand(store, generic, source.instruction, source.owner, &result, &mut Vec::new())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::operation_prepare::tests::source_fixture;
    use crate::sema::operation_graph::PreparedLanguageOperation;

    #[test]
    fn canonical_native_module_result_carrier_requires_prepared_source_proof_after_frontend_drop() {
        let source = "test native_result_carrier [error] { |ctx| let result = test.run_script(ctx, \"let value = 1\\n\")?; result.stdout == \"\" }\n";
        let program = source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        let generic = program.generic_evidence().unwrap();
        assert_eq!(generic.ground_native_calls().count(), 1);
        let (_, proof) = generic.ground_native_calls().next().unwrap();
        assert!(matches!(proof.contract.authority, PreparedOperationAuthority::Registry { operation: RuntimeOp::TestRunScript, .. }));
        assert_eq!(proof.contract.arguments.len(), 2);
        assert_eq!(proof.contract.argument_sources.len(), 6);
        assert_eq!(proof.contract.binding.supplied_slots.as_ref(), &[0, 1]);
        assert_eq!(proof.contract.binding.default_slots.as_ref(), &[2, 3, 4, 5]);
        assert!(proof.contract.argument_sources[2..].iter().all(Option::is_none));
    }

    #[test]
    fn canonical_native_calls_preserve_closed_operands_inside_the_original_generic_scope() {
        let source = "proc compare(unused, ext: Str) -> Bool { let result = mime.lookup_ext(ext); ext == \"txt\" }\nlet result = compare(7, \"txt\")\n";
        let program = source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        let generic = program.generic_evidence().unwrap();
        let (_, proof) = generic.ground_native_calls().next().unwrap();
        let source = generic.native_call_source(proof.source).unwrap();
        let scope = source.scope.expect("an independently closed native call retains its original generic scope");
        assert_eq!(source.owner, InstructionOwner::Function(generic.scope(scope).unwrap().owner));
        assert_eq!(generic.scope(scope).unwrap().quantifiers.len(), 1);
        assert!(matches!(proof.contract.result, TypeRef::Ground(_)));
        assert!(proof.contract.arguments.iter().all(|argument| matches!(argument.ty, TypeRef::Ground(_))));
    }

    #[test]
    fn canonical_native_named_arguments_keep_original_recipes_before_formal_slot_order() {
        let source = "test native_named_result [error] { |ctx| let result = test.run_script(source: \"let value = 1\\n\", ctx: ctx)?; result.stdout == \"\" }\n";
        let program = source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        let (_, proof) = program.generic_evidence().unwrap().ground_native_calls().next().unwrap();
        assert_eq!(proof.contract.binding.supplied_slots.as_ref(), &[1, 0]);
        assert_eq!(proof.contract.arguments[0].original.name.unwrap().as_str().as_str(), "source");
        assert_eq!(proof.contract.arguments[1].original.name.unwrap().as_str().as_str(), "ctx");
    }

    fn native_pair() -> FullProgram {
        source_fixture("test native_result_carrier [error] { |ctx| let first = test.run_script(ctx, \"let value = 1\\n\")?; let second = test.run_script(ctx, \"let value = 2\\n\")?; first.stdout == second.stdout }\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq })
    }

    #[test]
    fn canonical_native_calls_reject_missing_foreign_misplaced_and_rewritten_sources() {
        let program = native_pair();
        let foreign = native_pair();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_native_calls().next().unwrap();
            let source = generic.native_call_source(proof.source).unwrap();
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_remove_ground_native_calls();
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err());
            let mut other_root = program.store.clone();
            other_root.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().source = foreign.generic_evidence().unwrap().ground_native_calls().next().unwrap().1.source;
            assert!(FullVerifier::verify_generic_evidence(&other_root).is_err());
            let mut misplaced = program.store.clone();
            misplaced.generic.as_deref_mut().unwrap().test_native_call_source_mut(proof.source).unwrap().instruction = proof.contract.arguments[1].instruction;
            assert!(FullVerifier::verify_generic_evidence(&misplaced).is_err());
            let mut wrong_owner = program.store.clone();
            wrong_owner.generic.as_deref_mut().unwrap().test_native_call_source_mut(proof.source).unwrap().owner = InstructionOwner::Driver(0);
            assert!(FullVerifier::verify_generic_evidence(&wrong_owner).is_err());
            let mut wrong_source = program.store.clone();
            wrong_source.generic.as_deref_mut().unwrap().test_native_call_source_mut(proof.source).unwrap().origin.namespace = Some(Name::intern("changed"));
            assert!(FullVerifier::verify_generic_evidence(&wrong_source).is_err());
            let mut wrong_opcode = program.store.clone();
            wrong_opcode.tags[source.instruction as usize] = FullTag::ExprCall;
            assert!(FullVerifier::verify_generic_evidence(&wrong_opcode).is_err());
        });
    }

    #[test]
    fn canonical_native_calls_reject_opcode_argument_default_result_and_effect_rewrites() {
        let program = native_pair();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_native_calls().next().unwrap();
            let source = generic.native_call_source(proof.source).unwrap();
            let (_, other) = generic.ground_native_calls().nth(1).unwrap();
            let range = program.store.data[source.instruction as usize].range();
            let mut changed = program.store.clone();
            let replacement = changed.runtime_ops.len() as u32;
            changed.runtime_ops.push(RuntimeOp::TestRunXsh);
            changed.extra[range.start as usize] = replacement;
            assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "an operation with the same Result shape cannot replace the original registry authority");
            let mut both = changed.clone();
            let PreparedOperationAuthority::Registry { operation, .. } = &mut both.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.authority else { unreachable!() };
            *operation = RuntimeOp::TestRunXsh;
            assert!(FullVerifier::verify_generic_evidence(&both).is_err());
            let mut operand = program.store.clone();
            let block = IrBlockId::from_raw(operand.payload(range).unwrap()[2]).unwrap();
            let argument_range = operand.blocks[block.index()].instructions;
            operand.extra[argument_range.start as usize + 4] = other.contract.arguments[1].instruction;
            assert!(FullVerifier::verify_generic_evidence(&operand).is_err(), "another same-typed literal is not the original authored operand");
            let mut wrong_value = program.store.clone();
            wrong_value.tags[proof.contract.arguments[1].instruction as usize] = FullTag::ExprBytes;
            assert!(FullVerifier::verify_generic_evidence(&wrong_value).is_err(), "the original source identity does not bypass the operand's producer proof");
            let mut default = program.store.clone();
            default.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.binding.default_slots[0] = 0;
            assert!(FullVerifier::verify_generic_evidence(&default).is_err());
            let mut result = program.store.clone();
            let boolean = SemanticPoolBuilder::default().intern_type(&mut result.semantic, &Type::Bool).unwrap();
            result.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.result = TypeRef::Ground(boolean);
            assert!(FullVerifier::verify_generic_evidence(&result).is_err());
            let mut effects = program.store.clone();
            effects.generic.as_deref_mut().unwrap().test_ground_native_call_mut(id).unwrap().contract.effects.creation = crate::sema::inference::EffectSet::IO;
            assert!(FullVerifier::verify_generic_evidence(&effects).is_err());
        });
    }

    #[test]
    fn canonical_native_calls_reject_coforged_opcode_proof_and_source_expectation() {
        let program = native_pair();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_native_calls().next().unwrap();
            let source = generic.native_call_source(proof.source).unwrap();
            let mut forged = program.store.clone();
            let replacement = forged.runtime_ops.len() as u32;
            forged.runtime_ops.push(RuntimeOp::TestRunXsh);
            let range = forged.data[source.instruction as usize].range();
            forged.extra[range.start as usize] = replacement;
            let evidence = forged.generic.as_deref_mut().unwrap();
            let PreparedOperationAuthority::Registry { operation, .. } = &mut evidence.test_ground_native_call_mut(id).unwrap().contract.authority else { unreachable!() };
            *operation = RuntimeOp::TestRunXsh;
            let PreparedOperationAuthority::Registry { operation, .. } = &mut evidence.test_native_call_source_mut(proof.source).unwrap().expected.authority else { unreachable!() };
            *operation = RuntimeOp::TestRunXsh;
            let failure = FullVerifier::verify_generic_evidence(&forged).unwrap_err();
            assert!(failure.message.contains("original receipt"), "the original selected registry operation must survive agreement among rewritten copies: {}", failure.message);
        });
    }

    #[test]
    fn canonical_native_call_lifetimes_keep_original_sources_caches_and_retained_payload_owned() {
        use super::super::super::generic::GenericEvidenceBuilder;
        let program = native_pair();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (_, proof) = generic.ground_native_calls().next().unwrap();
            let source = generic.native_call_source(proof.source).unwrap();
            let mut builder = GenericEvidenceBuilder::default();
            assert!(builder.rewind(GenericEvidenceBuilder::default().checkpoint()).is_err());
            builder.register_instruction_origin(source.instruction, OperationSourceOrigin::Expression(source.origin), source.owner).unwrap();
            for argument in &proof.contract.arguments {
                let crate::sema::arguments::ArgumentValueSource::Expression(expression) = argument.original.value else { unreachable!() };
                builder.register_instruction_origin(argument.instruction, OperationSourceOrigin::Expression(crate::sema::check::ExpressionIdentity { expression, ..source.origin }), source.owner).unwrap();
            }
            let checkpoint = builder.checkpoint();
            let retired_source = builder.add_native_call_source(source.clone()).unwrap();
            let retired = builder.add_ground_native_call(PreparedGroundNativeCall { source: retired_source, contract: proof.contract.clone() }).unwrap();
            let stale = builder.checkpoint();
            builder.rewind(checkpoint).unwrap();
            let current_source = builder.add_native_call_source(source.clone()).unwrap();
            let current = builder.add_ground_native_call(PreparedGroundNativeCall { source: current_source, contract: proof.contract.clone() }).unwrap();
            assert!(builder.rewind(stale).is_err());
            let owners = program.store.generic_instruction_owners().unwrap();
            let mut store = builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap();
            assert!(store.native_call_source(retired_source).is_err());
            assert!(store.ground_native_call(retired).is_err());
            assert!(store.native_call_source(proof.source).is_err());
            assert_eq!(store.ground_native_call_at(source.instruction).unwrap(), Some(current));
            let retained = store.retained_bytes();
            assert!(retained >= 2 * (proof.contract.arguments.len() * size_of::<PreparedInvocationArgument>() + proof.contract.argument_sources.len() * size_of::<Option<u32>>()));
            store.shrink_to_fit();
            assert!(store.retained_bytes() <= retained);
            assert_eq!(store.ground_native_call_at(source.instruction).unwrap(), Some(current));
        });
    }
}
