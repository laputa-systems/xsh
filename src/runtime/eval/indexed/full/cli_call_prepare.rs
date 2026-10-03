use super::*;
use super::super::generic::{GroundNativeCallContract, NativeCallSource, PreparedGroundNativeCall, PreparedInvocationArgument, PreparedOperationAuthority, PreparedOperationBinding, PreparedOperationEffects, graph_ground_type};
use crate::sema::inference::{EffectSummary, OperationBinding, RequirementTemplate, TypeNode};
use crate::sema::registry_graph::RegistryOwner;
use crate::modules::signature::{ImplBinding, SemanticRule};

fn cli_problem(construct: &'static str) -> IrBuildError { IrBuildError::format(construct, None, 0, 0) }

pub(super) fn encoded_cli_arguments(store: &FullStore, instruction: u32, parameter_count: usize, original: &Arc<crate::modules::cli::CliDescriptorPlan>) -> Result<(RuntimeOp, Vec<Option<u32>>, u32), IrVerifyError> {
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprModuleCall) { return Err(IrVerifyError::new("CLI call proof is attached to another opcode")); }
    let words = store.payload(store.data[instruction as usize].range())?;
    if words.len() != 5 || words[1] != 1 { return Err(IrVerifyError::new("CLI call descriptor protocol is not prepared")); }
    let operation = *store.runtime_ops.get(words[0] as usize).ok_or_else(|| IrVerifyError::new("CLI call operation is invalid"))?;
    let plan = store.prepared_cli_plans.get(words[2] as usize).ok_or_else(|| IrVerifyError::new("CLI descriptor plan is out of bounds"))?;
    if !Arc::ptr_eq(plan, original) || !plan.matches_operation(operation) {
        return Err(IrVerifyError::new("CLI descriptor plan differs from its original checked authority"));
    }
    let block = IrBlockId::from_raw(words[3]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("CLI call argument block is invalid"))?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("CLI call argument block has another kind")); }
    let mut cursor = FullCursor::new(store.payload(block.instructions)?);
    let count = cursor.raw()? as usize;
    if count > parameter_count || parameter_count > 65536 { return Err(IrVerifyError::new("CLI call argument count exceeds its original signature")); }
    let mut sources = vec![None; parameter_count];
    for source in &mut sources[..count] {
        *source = match cursor.raw()? { 0 => None, 1 => Some(cursor.raw()?), _ => return Err(IrVerifyError::new("CLI call optional argument is invalid")) };
    }
    cursor.finish()?;
    Ok((operation, sources, words[4]))
}

impl FullBuilder {
    pub(super) fn prepare_cli_calls(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        for (instruction, expression, owner) in self.generic_expression_rows.clone() {
            if self.store.tags[instruction as usize] != FullTag::ExprModuleCall { continue; }
            let Some(boundary) = solved.registry_boundaries.get(&expression) else { continue; };
            let Some(plan) = boundary.shared_descriptor() else { continue; };
            let Some(operation) = solved.operations.get(&expression) else { continue; };
            let graph = &solved.graph;
            let Some(selected) = graph.candidate_evidence(operation.requirement).map_err(|_| cli_problem("cli_candidate_owner"))? else { continue; };
            let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).map_err(|_| cli_problem("cli_candidate_authority"))? else { continue; };
            if !matches!(metadata.owner, RegistryOwner::Module(_)) || metadata.binding != ImplBinding::Native
                || !matches!(metadata.semantic_rule, SemanticRule::CliDescriptor | SemanticRule::CliCommands)
                || boundary.descriptor_operation() != Some(metadata.operation)
                || !plan.matches_operation(metadata.operation)
                || boundary.requirement != Some(operation.requirement) || boundary.caller != operation.caller
                || boundary.input != operation.result {
                return Err(cli_problem("cli_original_boundary_authority"));
            }
            let original_scope = solved.expression_scope(expression, operation.caller).map_err(|_| cli_problem("cli_original_requirement_scope"))?;
            graph.validate_requirement_scoped(crate::sema::inference::ScopedRequirementRoot { requirement: operation.requirement, scope: original_scope }).map_err(|_| cli_problem("cli_original_requirement_certificate"))?;
            let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation.requirement).map_err(|_| cli_problem("cli_requirement_owner"))? else { return Err(cli_problem("cli_requirement_kind")); };
            let call = graph.operation_call(call).map_err(|_| cli_problem("cli_call_owner"))?;
            if call.binding != OperationBinding::Slots || call.receiver.is_some() || operation.receiver.is_some()
                || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
                || !operation.argument_coercions.is_empty() || !selected.callback_invocations.is_empty() { continue; }
            let signature = graph.resolved(selected.signature).map_err(|_| cli_problem("cli_signature_owner"))?;
            let TypeNode::Arrow(arrow) = graph.node(signature).map_err(|_| cli_problem("cli_signature_owner"))? else { return Err(cli_problem("cli_signature_kind")); };
            let offset = 0;
            if arrow.kind != metadata.kind || arrow.params.len() != metadata.parameters.len() + offset
                || arrow.params[offset..].iter().zip(&metadata.parameters).any(|(formal, original)| formal.label != original.label || formal.defaulted != original.defaulted || formal.rest) {
                return Err(cli_problem("cli_original_parameter_contract"));
            }
            // A closed CLI result does not make unresolved operands ground.
            // Such a call needs its own scoped argument protocol.
            if arrow.params.iter().map(|parameter| parameter.ty).chain(std::iter::once(selected.result))
                .chain(operation.actual_arguments.iter().copied()).chain(operation.receiver).any(|ty| graph_ground_type(graph, ty).is_err()) { continue; }
            let closed = |summary| match graph.closed_effect_summary(summary).map_err(|_| cli_problem("cli_effect_owner"))? {
                EffectSummary::Closed(bits) => Ok(bits), _ => Err(cli_problem("cli_effect_scope_not_prepared")),
            };
            let effects = PreparedOperationEffects {
                creation: closed(selected.effects)?,
                inputs: call.effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(),
                outputs: call.output_effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(),
            };
            let descriptor = self.intern_checked_callable_type(graph, signature)?;
            let (kind, signature) = self.store.semantic.callable_descriptor(descriptor).map_err(|_| cli_problem("cli_signature_descriptor"))?.ok_or_else(|| cli_problem("cli_signature_descriptor"))?;
            let result_type = graph_ground_type(graph, boundary.result).map_err(|_| cli_problem("cli_result_scope"))?;
            let result = self.intern_generic_ground_type(&result_type)?;
            let Type::Result(success, _) = &result_type else { return Err(cli_problem("cli_original_result_carrier")); };
            let result_layout = crate::runtime::eval::require::PreparedSchema::compile_record_layout(success)
                .ok_or_else(|| cli_problem("cli_original_result_record_layout"))?;
            if graph_ground_type(graph, boundary.input).map_err(|_| cli_problem("cli_canonical_input"))?
                != graph_ground_type(graph, selected.result).map_err(|_| cli_problem("cli_canonical_result"))?
                || result_type != plan.return_type(metadata.operation == RuntimeOp::CliParseFull) {
                return Err(cli_problem("cli_original_result_contract"));
            }
            let original_result = solved.expressions.get(&expression).copied().ok_or_else(|| cli_problem("cli_original_result_missing"))?;
            if graph_ground_type(graph, original_result).map_err(|_| cli_problem("cli_original_result_scope"))? != result_type { return Err(cli_problem("cli_original_result_changed")); }
            let (encoded_operation, argument_sources, location) = encoded_cli_arguments(&self.store, instruction, arrow.params.len(), plan)
                .map_err(|_| cli_problem("cli_encoded_arguments"))?;
            if encoded_operation != metadata.operation || IrLocationId::from_raw(location).and_then(|location| self.store.location_sources.get(location.index())) != Some(&expression.source) {
                return Err(cli_problem("cli_original_opcode_or_source"));
            }
            let recipes = solved.argument_sources.get(&expression).ok_or_else(|| cli_problem("cli_original_recipes_missing"))?;
            if recipes.len() != operation.binding.supplied_slots.len() || recipes.len() != operation.actual_arguments.len() || selected.actual_arguments.len() + offset != arrow.params.len() { return Err(cli_problem("cli_original_binding_count")); }
            let mut arguments = Vec::with_capacity(recipes.len());
            let mut record_arguments = Vec::new();
            let mut descriptor_sources = vec![None; argument_sources.len()];
            let mut operands = Vec::with_capacity(recipes.len());
            for (ordinal, ((recipe, &slot), &checked)) in recipes.iter().zip(&operation.binding.supplied_slots).zip(&operation.actual_arguments).enumerate() {
                let argument = argument_sources.get(slot + offset).copied().flatten().ok_or_else(|| cli_problem("cli_supplied_operand_missing"))?;
                let original = match recipe.value {
                    crate::sema::arguments::ArgumentValueSource::Expression(value) => {
                        let actual = self.original_argument_expression(argument, expression, ordinal, recipe, owner)?;
                        if actual != (crate::sema::check::ExpressionIdentity { expression: value, ..expression }) { return Err(cli_problem("cli_operand_original_source_changed")); }
                        let (material, _) = self.argument_initializer_lineage(argument, owner)?;
                        descriptor_sources[slot + offset] = Some(self.prepared_saved_argument_bindings.get(&material)
                            .map_or(material, |binding| binding.initializer_source_instruction));
                        let original = solved.expressions.get(&actual).copied().ok_or_else(|| cli_problem("cli_operand_original_type_missing"))?;
                        graph_ground_type(graph, original).map_err(|_| cli_problem("cli_operand_original_type_scope"))?
                    }
                    crate::sema::arguments::ArgumentValueSource::RecordField { .. } => {
                        let (receipt, actual) = self.prepare_native_record_argument(expression, ordinal, slot + offset, recipe, argument, owner)?;
                        // The authored spread record owns descriptor data; the
                        // saved projection owns only its original field recipe.
                        descriptor_sources[slot + offset] = Some(receipt.record_source_instruction);
                        record_arguments.push(receipt);
                        actual
                    }
                    _ => return Err(cli_problem("cli_argument_recipe_not_prepared")),
                };
                let selected_argument = selected.actual_arguments.get(slot).copied().flatten().ok_or_else(|| cli_problem("cli_selected_operand_missing"))?;
                if graph_ground_type(graph, checked).map_err(|_| cli_problem("cli_operand_type_scope"))? != original
                    || graph_ground_type(graph, selected_argument).map_err(|_| cli_problem("cli_selected_operand_scope"))? != original { return Err(cli_problem("cli_original_operand_type_changed")); }
                let ty = TypeRef::Ground(self.intern_generic_ground_type(&original)?);
                arguments.push(PreparedInvocationArgument { original: recipe.clone(), instruction: argument, ty });
                operands.push(argument);
            }
            for (slot, argument) in argument_sources[offset..].iter().enumerate() {
                if argument.is_some() != operation.binding.supplied_slots.contains(&slot)
                    || argument.is_none() != operation.binding.default_slots.contains(&slot)
                    || argument.is_none() != selected.actual_arguments[slot].is_none() { return Err(cli_problem("cli_encoded_binding_changed")); }
            }
            let slots = |slots: &[usize]| slots.iter().map(|&slot| u32::try_from(slot + offset).map_err(|_| cli_problem("cli_slot_overflow"))).collect::<Result<Box<[_]>, _>>();
            let template = graph.candidate(selected.candidate).map_err(|_| cli_problem("cli_original_candidate_template"))?;
            let mut rows = Vec::new();
            for (slot, argument) in descriptor_sources.iter().enumerate() {
                let label = self.store.semantic.signature_param(signature, slot).map_err(|_| cli_problem("cli_original_descriptor_slot"))?.0;
                if matches!(label.as_str().as_str(), "schema" | "commands" | "fallback_command") {
                    if let Some(argument) = argument { capture_cli_descriptor_rows(&self.store, *argument, &mut rows, &mut Vec::new())
                        .map_err(|_| cli_problem("cli_original_descriptor_values"))?; }
                }
            }
            let contract = GroundNativeCallContract {
                process_command_argv: None,
                cli_descriptor: Some(super::super::generic::PreparedCliDescriptor { plan: Arc::clone(plan), rows: rows.into(),
                    canonical_result: graph_ground_type(graph, boundary.input).map_err(|_| cli_problem("cli_original_canonical_result"))?, result_layout }),
                authority: PreparedOperationAuthority::Registry { identity: metadata.identity, operation: metadata.operation, binding: metadata.binding,
                    argument_check: metadata.argument_check, semantic_rule: metadata.semantic_rule, lifecycle: metadata.lifecycle, producer_transfer: metadata.producer_transfer.clone() },
                registry_owner: metadata.owner, signature, kind, result: TypeRef::Ground(result), effects, argument_relations: template.argument_relations.clone().into_boxed_slice(), input_eligibility: template.actual_eligibility.clone().into_boxed_slice(), arguments: arguments.into_boxed_slice(),
                binding: PreparedOperationBinding { supplied_slots: slots(&operation.binding.supplied_slots)?, default_slots: slots(&operation.binding.default_slots)?, rest_slot: None, dynamic: None, operands: operands.into_boxed_slice() },
                argument_sources: argument_sources.into_boxed_slice(), receiver: None,
            };
            let scope = operation.caller.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            let source = self.generic_evidence_mut().add_native_call_source(NativeCallSource { origin: expression, instruction, owner, scope, expected: contract.clone(), result_record_layout: None, record_arguments: record_arguments.into_boxed_slice(), argument_lineages: Box::new([]), result_refinement: None }).map_err(|_| cli_problem("cli_source_allocation"))?;
            self.generic_evidence_mut().add_ground_native_call(PreparedGroundNativeCall { source, contract }).map_err(|_| cli_problem("cli_proof_allocation"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "cli_call_prepare/tests.rs"]
mod tests;

fn capture_cli_descriptor_rows(store: &FullStore, instruction: u32, rows: &mut Vec<super::super::generic::CliDescriptorRow>, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
    if active.len() >= 256 || active.contains(&instruction) { return Err(IrVerifyError::new("CLI descriptor source is cyclic or too deep")); }
    if rows.iter().any(|row| row.instruction == instruction) { return Ok(()); }
    if rows.len() >= 65536 { return Err(IrVerifyError::new("CLI descriptor source exceeds its preparation bound")); }
    let tag = *store.tags.get(instruction as usize).ok_or_else(|| IrVerifyError::new("CLI descriptor instruction is invalid"))?;
    let payload = store.payload(store.data[instruction as usize].range())?.to_vec().into_boxed_slice();
    let mut row = super::super::generic::CliDescriptorRow { instruction, tag: tag as u16, payload, block: None, text: None, bytes: None, constant: None };
    active.push(instruction);
    match tag {
        FullTag::ExprNull | FullTag::ExprBool | FullTag::ExprInt | FullTag::ExprDuration => {},
        FullTag::ExprStr => { row.text = Some(Arc::from(store.string(*row.payload.first().ok_or_else(|| IrVerifyError::new("CLI descriptor string is missing"))?)?)); },
        FullTag::ExprPath => { row.bytes = Some(Arc::from(store.bytes(*row.payload.first().ok_or_else(|| IrVerifyError::new("CLI descriptor path is missing"))?)?)); },
        FullTag::ExprPreparedConstant => { row.constant = Some(store.prepared_constants.get(*row.payload.first().ok_or_else(|| IrVerifyError::new("CLI descriptor constant is missing"))? as usize).ok_or_else(|| IrVerifyError::new("CLI descriptor constant is invalid"))?.0.clone()); },
        FullTag::ExprRecord | FullTag::ExprList => {
            let id = row.payload.first().copied().and_then(IrBlockId::from_raw).ok_or_else(|| IrVerifyError::new("CLI descriptor block is invalid"))?;
            let block = store.blocks.get(id.index()).ok_or_else(|| IrVerifyError::new("CLI descriptor block is missing"))?;
            if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("CLI descriptor block has another kind")); }
            let words = store.payload(block.instructions)?;
            row.block = Some((id.raw(), words.to_vec().into_boxed_slice()));
            let mut cursor = FullCursor::new(words);
            let count = cursor.raw()? as usize;
            if count > words.len() { return Err(IrVerifyError::new("CLI descriptor children are incomplete")); }
            for _ in 0..count {
                if tag == FullTag::ExprRecord {
                    if cursor.raw()? != 0 { return Err(IrVerifyError::new("CLI descriptor spread needs its own source proof")); }
                    cursor.raw()?;
                }
                capture_cli_descriptor_rows(store, cursor.raw()?, rows, active)?;
            }
            cursor.finish()?;
        }
        FullTag::ExprCheckedValue => {
            capture_cli_descriptor_rows(store, *row.payload.first().ok_or_else(|| IrVerifyError::new("CLI descriptor wrapper is missing"))?, rows, active)?;
        }
        _ => return Err(IrVerifyError::new("CLI descriptor value requires an original constant source protocol")),
    }
    active.pop();
    rows.push(row);
    Ok(())
}

impl FullVerifier {
    pub(super) fn verify_cli_descriptor_rows(store: &FullStore, descriptor: &super::super::generic::PreparedCliDescriptor) -> Result<(), IrVerifyError> {
        for row in descriptor.rows.iter() {
            if store.tags.get(row.instruction as usize).map(|tag| *tag as u16) != Some(row.tag)
                || store.payload(store.data.get(row.instruction as usize).ok_or_else(|| IrVerifyError::new("CLI descriptor value row is missing"))?.range())? != row.payload.as_ref() {
                return Err(IrVerifyError::new("CLI descriptor changes its original constant value row"));
            }
            if let Some((id, words)) = &row.block {
                let block = IrBlockId::from_raw(*id).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("CLI descriptor loses its original child block"))?;
                if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST || store.payload(block.instructions)? != words.as_ref() {
                    return Err(IrVerifyError::new("CLI descriptor changes its original constant children"));
                }
            }
            if let Some(text) = &row.text {
                if store.string(row.payload[0])? != text.as_ref() { return Err(IrVerifyError::new("CLI descriptor changes its original string value")); }
            }
            if let Some(bytes) = &row.bytes {
                if store.bytes(row.payload[0])? != bytes.as_ref() { return Err(IrVerifyError::new("CLI descriptor changes its original path value")); }
            }
            if let Some(value) = &row.constant {
                if store.prepared_constants.get(row.payload[0] as usize).map(|value| &value.0) != Some(value) {
                    return Err(IrVerifyError::new("CLI descriptor changes its original prepared constant value"));
                }
            }
        }
        Ok(())
    }
}

impl FullVerifier {
    /// Descriptor constants retain an exact value snapshot independently of
    /// their checked ground type. Constant expansion creates no child source
    /// identities; its closed data is validated as one original operand.
    pub(super) fn verify_cli_constant_operand(store: &FullStore, descriptor: &super::super::generic::PreparedCliDescriptor, instruction: u32, expected: &Type) -> Result<bool, IrVerifyError> {
        let Some(row) = descriptor.rows.iter().find(|row| row.instruction == instruction) else { return Ok(false); };
        let Some(value) = &row.constant else { return Ok(false); };
        Self::verify_cli_descriptor_rows(store, descriptor)?;
        let mut work = 0usize;
        if !prepared_constant_matches_type(value, expected, 0, &mut work)? {
            return Err(IrVerifyError::new("CLI descriptor constant disagrees with its original checked operand type"));
        }
        Ok(true)
    }
}

pub(super) fn prepared_constant_matches_type(value: &LoweredValue, expected: &Type, depth: usize, work: &mut usize) -> Result<bool, IrVerifyError> {
    *work = work.checked_add(1).ok_or_else(|| IrVerifyError::new("prepared constant work bound overflow"))?;
    if depth >= 256 || *work > 65536 { return Err(IrVerifyError::new("prepared constant exceeds its verification bound")); }
    if let Type::Optional(inner) = expected {
        return if matches!(value, LoweredValue::Null) { Ok(true) } else { prepared_constant_matches_type(value, inner, depth + 1, work) };
    }
    Ok(match (value, expected) {
        (LoweredValue::Null, Type::Null) | (LoweredValue::Bool(_), Type::Bool) | (LoweredValue::Int(_), Type::Int)
        | (LoweredValue::Float(_), Type::Float) | (LoweredValue::Str(_), Type::Str) | (LoweredValue::Path(_), Type::Path)
        | (LoweredValue::Duration(_), Type::Duration) | (LoweredValue::Bytes(_), Type::Bytes) => true,
        (LoweredValue::Int(value), Type::UInt) => *value >= 0,
        (LoweredValue::Record(values), Type::Record(fields)) => {
            if values.len() < fields.len() { return Ok(false); }
            for (name, ty) in fields {
                let Some(value) = values.get(name.as_str().as_str()) else { return Ok(false); };
                if !prepared_constant_matches_type(value, ty, depth + 1, work)? { return Ok(false); }
            }
            for (name, value) in values.iter() {
                if !fields.contains_key(&Name::intern(name.as_ref())) && !prepared_constant_data(value, depth + 1, work)? { return Ok(false); }
            }
            true
        }
        (LoweredValue::RecordVec(values), Type::Record(fields)) => {
            if values.len() < fields.len() { return Ok(false); }
            let mut seen = std::collections::BTreeSet::new();
            for (name, value) in values.iter() {
                if !seen.insert(*name) { return Ok(false); }
                if let Some(ty) = fields.get(name) {
                    if !prepared_constant_matches_type(value, ty, depth + 1, work)? { return Ok(false); }
                } else if !prepared_constant_data(value, depth + 1, work)? { return Ok(false); }
            }
            if fields.keys().any(|name| !seen.contains(name)) { return Ok(false); }
            true
        }
        (LoweredValue::List(values), Type::List(item)) => {
            for value in values { if !prepared_constant_matches_type(value, item, depth + 1, work)? { return Ok(false); } }
            true
        }
        (LoweredValue::SharedList(values), Type::List(item)) => {
            for value in values.iter() { if !prepared_constant_matches_type(value, item, depth + 1, work)? { return Ok(false); } }
            true
        }
        (LoweredValue::Regex(_), Type::Regex) => true,
        (LoweredValue::Map(values), Type::Map(key, item)) => {
            for (actual, value) in values.iter() {
                let valid = match (actual, key.as_ref()) {
                    (crate::map_key::MapKey::Str(_), Type::Str) | (crate::map_key::MapKey::Int(_), Type::Int)
                    | (crate::map_key::MapKey::Bool(_), Type::Bool) | (crate::map_key::MapKey::Bytes(_), Type::Bytes)
                    | (crate::map_key::MapKey::Path(_), Type::Path) | (crate::map_key::MapKey::Duration(_), Type::Duration) => true,
                    (crate::map_key::MapKey::Int(value), Type::UInt) => *value >= 0,
                    _ => false,
                };
                if !valid || !prepared_constant_matches_type(value, item, depth + 1, work)? { return Ok(false); }
            }
            true
        }
        (LoweredValue::Tag(tag), Type::Tag(owner)) if tag.type_name == *owner => prepared_constant_data(value, depth + 1, work)?,
        (LoweredValue::Record(_) | LoweredValue::RecordVec(_), Type::ErasedRecord) => prepared_constant_data(value, depth + 1, work)?,
        _ => false,
    })
}

fn prepared_constant_children<'a>(children: impl IntoIterator<Item = &'a LoweredValue>, depth: usize, work: &mut usize) -> Result<bool, IrVerifyError> {
    for child in children { if !prepared_constant_data(child, depth + 1, work)? { return Ok(false); } }
    Ok(true)
}

fn prepared_constant_data(value: &LoweredValue, depth: usize, work: &mut usize) -> Result<bool, IrVerifyError> {
    *work = work.checked_add(1).ok_or_else(|| IrVerifyError::new("prepared constant work bound overflow"))?;
    if depth >= 256 || *work > 65536 { return Err(IrVerifyError::new("prepared constant exceeds its verification bound")); }
    match value {
        LoweredValue::Null | LoweredValue::Bool(_) | LoweredValue::Int(_) | LoweredValue::Float(_)
        | LoweredValue::Duration(_) | LoweredValue::Str(_) | LoweredValue::Bytes(_) | LoweredValue::Path(_) | LoweredValue::Regex(_) => Ok(true),
        LoweredValue::List(values) => prepared_constant_children(values, depth, work),
        LoweredValue::SharedList(values) => prepared_constant_children(values.iter(), depth, work),
        LoweredValue::Record(values) => prepared_constant_children(values.values(), depth, work),
        LoweredValue::RecordVec(values) => {
            let mut names = std::collections::BTreeSet::new();
            for (name, value) in values.iter() {
                if !names.insert(*name) || !prepared_constant_data(value, depth + 1, work)? { return Ok(false); }
            }
            Ok(true)
        }
        LoweredValue::Map(values) => prepared_constant_children(values.values(), depth, work),
        LoweredValue::Tag(tag) => {
            if let Some(wire) = &tag.wire {
                if wire.type_name != tag.type_name || !tag.fields.is_empty() || wire.variants.is_empty()
                    || !wire.variants.contains_key(&Name::intern(tag.name.as_ref())) { return Ok(false); }
                *work = work.checked_add(wire.variants.len()).ok_or_else(|| IrVerifyError::new("prepared constant work bound overflow"))?;
                if *work > 65536 { return Err(IrVerifyError::new("prepared constant exceeds its verification bound")); }
                if wire.variants.values().collect::<std::collections::BTreeSet<_>>().len() != wire.variants.len() { return Ok(false); }
            }
            prepared_constant_children(&tag.fields, depth, work)
        }
        _ => Ok(false),
    }
}

pub(super) fn original_dynamic_cli_candidate(solved: &crate::sema::check::SolvedTypes, origin: crate::sema::check::ExpressionIdentity,
    metadata: &crate::sema::registry_graph::RegistryCandidate,
) -> bool {
    if metadata.owner != RegistryOwner::Module("cli") || metadata.binding != ImplBinding::Native
        || !matches!((metadata.semantic_rule, metadata.operation),
            (SemanticRule::CliDescriptor, RuntimeOp::CliParse | RuntimeOp::CliParseFull | RuntimeOp::CliApplet)
            | (SemanticRule::CliCommands, RuntimeOp::CliCommands)) || solved.registry_boundaries.contains_key(&origin) { return false; }
    let Some(operation) = solved.operations.get(&origin) else { return false; };
    if operation.receiver.is_some() { return false; }
    let expected = GroundNativeCallContract::dynamic_cli_result_type(metadata.operation);
    graph_ground_type(&solved.graph, operation.result).is_ok_and(|result| result == expected)
        && solved.expressions.get(&origin).is_some_and(|result| graph_ground_type(&solved.graph, *result).is_ok_and(|result| result == expected))
}
