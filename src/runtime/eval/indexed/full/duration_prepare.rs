use super::*;
use super::super::generic::{OperationSource, OperationSourceOrigin, PreparedOperation, PreparedOperationAuthority, PreparedOperationBinding, PreparedOperationEffects, graph_ground_type};
use crate::sema::inference::{EffectSummary, OperationBinding, RequirementTemplate};
use crate::sema::operation_graph::{ArithmeticDomain, PreparedLanguageOperation};

fn unprepared(construct: &'static str) -> IrBuildError { IrBuildError::format(construct, None, 0, 0) }

impl FullBuilder {
    pub(super) fn prepare_duration_operations(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.as_ref().cloned() else { return Ok(()); };
        for (instruction, expression, owner) in self.generic_expression_rows.clone() {
            let Some(operation) = solved.operations.get(&expression) else { continue; };
            let graph = &solved.graph;
            let Some(selected) = graph.candidate_evidence(operation.requirement).map_err(|_| unprepared("operation_candidate_owner"))? else { continue; };
            let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).map_err(|_| unprepared("operation_candidate_authority"))? else { continue; };
            let supported = matches!(metadata.operation, PreparedLanguageOperation::Arithmetic {
                domain: ArithmeticDomain::DurationPair | ArithmeticDomain::DurationScale { .. } | ArithmeticDomain::DurationRatio, ..
            });
            if !supported { continue; }
            let authority = PreparedOperationAuthority::Language { identity: metadata.identity, authority: metadata.authority,
                operation: metadata.operation, argument_order: metadata.argument_order, statement_result_is_unit: metadata.statement_result_is_unit };
            let scope = operation.caller.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            let source = self.generic_evidence_mut().add_operation_source(OperationSource {
                origin: OperationSourceOrigin::Expression(expression), identity: metadata.identity, expected: authority.clone(), instruction, owner, scope,
            }).map_err(|_| unprepared("operation_source_allocation"))?;
            let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation.requirement).map_err(|_| unprepared("operation_requirement"))? else { return Err(unprepared("operation_requirement_kind")); };
            let call = graph.operation_call(call).map_err(|_| unprepared("operation_call_owner"))?;
            if call.binding != OperationBinding::Slots || !selected.callback_invocations.is_empty()
                || operation.binding.dynamic.is_some() || operation.binding.rest_slot.is_some()
                || !operation.argument_coercions.is_empty() { return Err(unprepared("operation_binding_not_prepared")); }
            let ground = |builder: &mut FullBuilder, ty| {
                let ty = graph_ground_type(graph, ty).map_err(|_| unprepared("operation_ground_type"))?;
                builder.intern_generic_ground_type(&ty).map(TypeRef::Ground)
            };
            let receiver = call.receiver.map(|ty| ground(self, ty)).transpose()?;
            let arguments = selected.actual_arguments.iter().map(|ty| ty.map(|ty| ground(self, ty)).transpose()).collect::<Result<Vec<_>, _>>()?;
            let result = ground(self, selected.result)?;
            let closed = |summary| match graph.closed_effect_summary(summary).map_err(|_| unprepared("operation_effect_owner"))? {
                EffectSummary::Closed(bits) => Ok(bits), _ => Err(unprepared("operation_latent_effect_not_prepared")),
            };
            let effects = PreparedOperationEffects {
                creation: closed(selected.effects)?,
                inputs: call.effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(),
                outputs: call.output_effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(),
            };
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| unprepared("operation_instruction_payload"))?;
            let tag = self.store.tags[instruction as usize];
            if tag != FullTag::ExprBinary || words.len() != 4 { return Err(unprepared("duration_instruction_not_prepared")); }
            let operands = Box::new([words[1], words[2]]);
            let fallback_lowering = None;
            let slots = |slots: &[usize]| slots.iter().map(|&slot| u32::try_from(slot).map_err(|_| unprepared("operation_binding_slot_overflow"))).collect::<Result<Vec<_>, _>>().map(Vec::into_boxed_slice);
            self.generic_evidence_mut().add_operation(PreparedOperation {
                source,
                authority,
                receiver, arguments: arguments.into_boxed_slice(), result, effects, fallback_lowering, original_integer_addition: None, range_lowering: None, literal_comparison_slot: None, membership_lowering: None,
                binding: PreparedOperationBinding { supplied_slots: slots(&operation.binding.supplied_slots)?,
                    default_slots: slots(&operation.binding.default_slots)?, rest_slot: None, dynamic: None, operands },
            }).map_err(|_| unprepared("operation_proof_allocation"))?;
        }
        for (instruction, expression, owner) in self.generic_expression_rows.clone() {
            let Some(&requirement) = solved.additions.get(&expression) else { continue; };
            let graph = &solved.graph;
            let Some(evidence) = graph.discharge(requirement).map_err(|_| unprepared("duration_add_original_requirement"))? else { continue; };
            if evidence.operation != crate::sema::inference::SealedOperation::AddDuration { continue; }
            let RequirementTemplate::Add { left, right, result } = graph.requirement_template(requirement).map_err(|_| unprepared("duration_add_original_requirement"))? else { return Err(unprepared("duration_add_original_requirement")); };
            if solved.owner != graph.owner() || evidence.requirement != requirement
                || solved.expressions.get(&expression).is_none_or(|&source| graph.resolved(source).ok() != graph.resolved(result).ok()) {
                return Err(unprepared("duration_add_original_source"));
            }
            for (source, checked) in [(left, evidence.left), (right, evidence.right), (result, evidence.result)] {
                if graph.resolved(source).map_err(|_| unprepared("duration_add_original_relationship"))?
                    != graph.resolved(checked).map_err(|_| unprepared("duration_add_original_relationship"))?
                    || graph_ground_type(graph, checked).map_err(|_| unprepared("duration_add_original_domain"))? != Type::Duration {
                    return Err(unprepared("duration_add_original_relationship"));
                }
            }
            let declaration = solved.expression_owners.get(&expression).copied();
            if declaration.is_some_and(|declaration| self.declaration_functions.get(&declaration).copied().map(InstructionOwner::Function) != Some(owner))
                || (declaration.is_none() && !matches!(owner, InstructionOwner::Driver(_))) {
                return Err(unprepared("duration_add_original_owner"));
            }
            let scope = declaration.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            let authority = PreparedOperationAuthority::Sealed { operation: evidence.operation };
            let source = self.generic_evidence_mut().add_operation_source(OperationSource {
                origin: OperationSourceOrigin::Expression(expression), identity: authority.identity(), expected: authority.clone(), instruction, owner, scope,
            }).map_err(|_| unprepared("duration_add_source_allocation"))?;
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| unprepared("duration_add_original_payload"))?;
            if self.store.tags[instruction as usize] != FullTag::ExprBinary || words.len() != 4
                || words.first().and_then(|&index| self.store.binary_ops.get(index as usize)) != Some(&BinaryOp::Add) {
                return Err(unprepared("duration_add_original_instruction"));
            }
            let operands = Box::new([words[1], words[2]]);
            let ty = TypeRef::Ground(self.intern_generic_ground_type(&Type::Duration)?);
            self.generic_evidence_mut().add_operation(PreparedOperation {
                source, authority, receiver: None, arguments: Box::new([Some(ty), Some(ty)]), result: ty,
                effects: PreparedOperationEffects { creation: crate::sema::inference::EffectSet::EMPTY, inputs: Box::new([]), outputs: Box::new([]) },
                binding: PreparedOperationBinding { supplied_slots: Box::new([0, 1]), default_slots: Box::new([]), rest_slot: None, dynamic: None, operands },
                fallback_lowering: None, original_integer_addition: None, range_lowering: None, literal_comparison_slot: None, membership_lowering: None,
            }).map_err(|_| unprepared("duration_add_proof_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(super) fn verify_duration_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(operation) = generic.operation_at(instruction)? else { return Ok(false); };
        if !GenericEvidenceStore::is_duration_operation(operation) { return Ok(false); }
        GenericEvidenceStore::verify_duration_operation_contract(&store.semantic, operation)?;
        let source = generic.operation_source(operation.source)?;
        let op = match operation.authority {
            PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Arithmetic { op, .. }, .. } => op,
            PreparedOperationAuthority::Sealed { operation: crate::sema::inference::SealedOperation::AddDuration } => BinaryOp::Add,
            _ => return Err(IrVerifyError::new("Duration operation loses its checked arithmetic authority")),
        };
        let OperationSourceOrigin::Expression(origin) = source.origin else { return Err(IrVerifyError::new("Duration operation source identity has another kind")); };
        if source.instruction != instruction || source.owner != owner || operation.authority != source.expected
            || generic.registered_instruction_origin(instruction, false) != Some((source.origin, owner))
            || store.tags.get(instruction as usize) != Some(&FullTag::ExprBinary) {
            return Err(IrVerifyError::new("Duration operation changes its original source, owner or authority"));
        }
        let words = store.payload(store.data[instruction as usize].range())?;
        if words.len() != 4 || words.first().and_then(|&index| store.binary_ops.get(index as usize)) != Some(&op)
            || words.get(1..3) != Some(operation.binding.operands.as_ref())
            || words.get(3).and_then(|&location| IrLocationId::from_raw(location)).and_then(|location| store.location_sources.get(location.index())) != Some(&origin.source) {
            return Err(IrVerifyError::new("Duration operation changes its original operator, operands or source location"));
        }
        let TypeRef::Ground(result) = operation.result else { return Err(IrVerifyError::new("Duration operation result is not ground")); };
        if store.semantic.to_type(result)? != *expected { return Err(IrVerifyError::new("Duration operation changes its checked result domain")); }
        for (&operand, reference) in operation.binding.operands.iter().zip(operation.arguments.iter()) {
            let Some(TypeRef::Ground(ty)) = reference else { return Err(IrVerifyError::new("Duration operation operand is not ground")); };
            Self::verify_generic_source(store, generic, operand, owner, &store.semantic.to_type(*ty)?, instance, active)?;
        }
        Ok(true)
    }
}

#[cfg(test)]
#[path = "duration_prepare/tests.rs"]
mod tests;
