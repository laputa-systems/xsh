use super::*;
use super::super::generic::{OperationSource, OperationSourceOrigin, PreparedInvocationArgument, PreparedOperation, PreparedOperationAuthority, PreparedOperationBinding, PreparedOperationEffects, PreparedRangeLowering, graph_ground_type};
use crate::sema::inference::{EffectSummary, OperationBinding, RequirementTemplate};
use crate::sema::operation_graph::{PreparedLanguageOperation, ValueConstructor};

fn unprepared(construct: &'static str) -> IrBuildError { IrBuildError::format(construct, None, 0, 0) }

impl FullBuilder {
    pub(super) fn prepare_range_operations(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.as_ref().cloned() else { return Ok(()); };
        for (instruction, expression, owner) in self.generic_expression_rows.clone() {
            let Some(operation) = solved.operations.get(&expression) else { continue; };
            let graph = &solved.graph;
            let Some(selected) = graph.candidate_evidence(operation.requirement).map_err(|_| unprepared("range_original_candidate"))? else { continue; };
            let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).map_err(|_| unprepared("range_original_authority"))? else { continue; };
            let PreparedLanguageOperation::Constructor { kind: ValueConstructor::Range, arity } = metadata.operation else { continue; };
            let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation.requirement).map_err(|_| unprepared("range_original_requirement"))? else { return Err(unprepared("range_original_requirement")); };
            let call = graph.operation_call(call).map_err(|_| unprepared("range_original_call"))?;
            if call.binding != OperationBinding::Slots || call.receiver.is_some() || !(1..=2).contains(&arity)
                || operation.binding.supplied_slots != (0..arity).collect::<Vec<_>>() || !operation.binding.default_slots.is_empty()
                || operation.binding.dynamic.is_some() || operation.binding.rest_slot.is_some() || !operation.argument_coercions.is_empty()
                || !selected.callback_invocations.is_empty() || !call.effect_bindings.is_empty() || !call.output_effect_bindings.is_empty()
                || graph.closed_effect_summary(selected.effects).map_err(|_| unprepared("range_original_effects"))? != EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY) {
                return Err(unprepared("range_original_binding"));
            }
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| unprepared("range_original_payload"))?.to_vec();
            if self.store.tags[instruction as usize] != FullTag::ExprRange || words.len() != 3 { return Err(unprepared("range_original_instruction")); }
            let endpoints = [words[0], words[1]];
            let operands = if arity == 1 { vec![endpoints[1]] } else { endpoints.to_vec() };
            let recipes = solved.argument_sources.get(&expression).ok_or_else(|| unprepared("range_original_recipes"))?;
            if recipes.len() != arity || operation.actual_arguments.len() != arity || selected.actual_arguments.len() != arity { return Err(unprepared("range_original_arity")); }
            let mut arguments = Vec::new();
            let mut types = Vec::new();
            for (ordinal, ((recipe, &operand), &checked)) in recipes.iter().zip(&operands).zip(&operation.actual_arguments).enumerate() {
                let actual = self.original_argument_expression(operand, expression, ordinal, recipe, owner)?;
                let original = solved.expressions.get(&actual).copied().ok_or_else(|| unprepared("range_original_endpoint_type"))?;
                if graph_ground_type(graph, original).map_err(|_| unprepared("range_original_endpoint_type"))? != Type::Int
                    || graph_ground_type(graph, checked).map_err(|_| unprepared("range_checked_endpoint_type"))? != Type::Int
                    || selected.actual_arguments[ordinal].is_none_or(|ty| graph_ground_type(graph, ty).ok() != Some(Type::Int)) { return Err(unprepared("range_original_endpoint_domain")); }
                let ty = TypeRef::Ground(self.intern_generic_ground_type(&Type::Int)?);
                types.push(Some(ty)); arguments.push(PreparedInvocationArgument { original: recipe.clone(), instruction: operand, ty });
            }
            let result_type = graph_ground_type(graph, selected.result).map_err(|_| unprepared("range_original_result"))?;
            if result_type != Type::Stream(Box::new(Type::Int)) || solved.expressions.get(&expression).is_none_or(|&ty| graph_ground_type(graph, ty).ok() != Some(result_type.clone())) { return Err(unprepared("range_original_result")); }
            let result = TypeRef::Ground(self.intern_generic_ground_type(&result_type)?);
            let authority = PreparedOperationAuthority::Language { identity: metadata.identity, authority: metadata.authority, operation: metadata.operation, argument_order: metadata.argument_order, statement_result_is_unit: metadata.statement_result_is_unit };
            let scope = operation.caller.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            let source = self.generic_evidence_mut().add_operation_source(OperationSource { origin: OperationSourceOrigin::Expression(expression), identity: metadata.identity, expected: authority.clone(), instruction, owner, scope }).map_err(|_| unprepared("range_source_allocation"))?;
            self.generic_evidence_mut().add_operation(PreparedOperation { source, authority, receiver: None, arguments: types.into_boxed_slice(), result,
                effects: PreparedOperationEffects { creation: crate::sema::inference::EffectSet::EMPTY, inputs: Box::new([]), outputs: Box::new([]) },
                binding: PreparedOperationBinding { supplied_slots: (0..arity as u32).collect::<Vec<_>>().into_boxed_slice(), default_slots: Box::new([]), rest_slot: None, dynamic: None, operands: operands.into_boxed_slice() },
                fallback_lowering: None, original_integer_addition: None,
                literal_comparison_slot: None, membership_lowering: None, range_lowering: Some(PreparedRangeLowering { payload: words.into_boxed_slice(), endpoints, arguments: arguments.into_boxed_slice() }),
            }).map_err(|_| unprepared("range_proof_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(super) fn verify_range_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(operation) = generic.operation_at(instruction)? else { return Ok(false); };
        if !matches!(operation.authority, PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Constructor { kind: ValueConstructor::Range, .. }, .. }) { return Ok(false); }
        generic.verify_range_operation_contract(&store.semantic, operation)?;
        let source = generic.operation_source(operation.source)?;
        let lowering = operation.range_lowering.as_ref().ok_or_else(|| IrVerifyError::new("range lowering is missing"))?;
        let OperationSourceOrigin::Expression(origin) = source.origin else { return Err(IrVerifyError::new("range source has another kind")); };
        if source.instruction != instruction || source.owner != owner || source.expected != operation.authority
            || generic.registered_instruction_origin(instruction, false) != Some((source.origin, owner))
            || store.tags.get(instruction as usize) != Some(&FullTag::ExprRange)
            || store.payload(store.data.get(instruction as usize).ok_or_else(|| IrVerifyError::new("range instruction is out of bounds"))?.range())? != lowering.payload.as_ref()
            || lowering.payload[2..].first().and_then(|&location| IrLocationId::from_raw(location)).and_then(|location| store.location_sources.get(location.index())) != Some(&origin.source)
            || *expected != Type::Stream(Box::new(Type::Int)) { return Err(IrVerifyError::new("range changes its original source, owner, endpoints or result")); }
        if lowering.arguments.len() == 1 {
            let zero = lowering.endpoints[0] as usize;
            if store.tags.get(zero) != Some(&FullTag::ExprInt) || store.payload(store.data.get(zero).ok_or_else(|| IrVerifyError::new("range zero endpoint is out of bounds"))?.range())? != [0, 0] { return Err(IrVerifyError::new("unary range changes its generated zero endpoint")); }
        }
        for &endpoint in &lowering.endpoints { Self::verify_generic_source(store, generic, endpoint, owner, &Type::Int, instance, active)?; }
        Ok(true)
    }
}

impl FullProgram {
    pub(in crate::runtime::eval) fn verify_range_execution(&self, instruction: u32) -> Result<(), IrVerifyError> {
        let generic = self.generic_evidence().ok_or_else(|| IrVerifyError::new("range execution lacks original evidence"))?;
        let operation = generic.operation_at(instruction)?.ok_or_else(|| IrVerifyError::new("range lacks independently prepared authority"))?;
        let source = generic.operation_source(operation.source)?;
        if !FullVerifier::verify_range_operand(&self.store, generic, instruction, source.owner, &Type::Stream(Box::new(Type::Int)), None, &mut vec![instruction])? { return Err(IrVerifyError::new("range lacks its original constructor proof")); }
        Ok(())
    }
}

// The producer owns only its cursor. Dropping it releases the remaining range
// without enumerating or allocating its unconsumed values.
struct RangeStream { cursor: Option<i64>, end: i64, ascending: bool }
impl crate::runtime::value::LiveStream for RangeStream {
    fn next(&mut self, _span: Span) -> Result<Option<crate::runtime::value::Value>, crate::runtime::value::RuntimeError> {
        let Some(value) = self.cursor else { return Ok(None); };
        if (self.ascending && value >= self.end) || (!self.ascending && value <= self.end) { self.cursor = None; return Ok(None); }
        self.cursor = if self.ascending { value.checked_add(1) } else { value.checked_sub(1) };
        Ok(Some(crate::runtime::value::Value::Int(value)))
    }
}
pub(in crate::runtime::eval) fn range_stream(start: i64, end: i64) -> crate::runtime::eval::LoweredValue {
    crate::runtime::eval::LoweredValue::Stream(Box::new(crate::runtime::value::StreamValue::from_live("range", RangeStream { cursor: Some(start), end, ascending: start <= end })))
}

#[cfg(test)]
#[path = "range_prepare/tests.rs"]
mod tests;

impl<'a> FullExecution<'a> {
    pub(in crate::runtime::eval) fn prepared_range(&self, instruction: u32) -> Result<(), IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("range belongs to another body")); }
        let generic = self.generic_evidence().ok_or_else(|| IrVerifyError::new("range execution lacks its original evidence"))?;
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) } else {
            InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("range execution owner is invalid"))?)
        };
        if !FullVerifier::verify_range_operand(self.decoder.store, generic, instruction, owner, &Type::Stream(Box::new(Type::Int)), self.instantiation, &mut vec![instruction])? { return Err(IrVerifyError::new("range lacks its original constructor proof")); }
        Ok(())
    }
}
