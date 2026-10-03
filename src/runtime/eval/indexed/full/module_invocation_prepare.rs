use super::*;
use super::super::generic::{CallableKind, ModuleExportContract, ModuleExportParameter, ModuleInvocationSource, ModuleReceiverAllocation, ParameterMode, PreparedInvocationArgument};
use crate::sema::inference::{ConstraintRelation, InvocationDefaultTiming, ScopedRoot, TypeNode};

/// A module binding preserves the authored allocation separately from the
/// export schema. Equal schemas can still contain different loaded programs.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildModuleBindingOrigin {
    pub statement: crate::sema::check::StatementIdentity,
    pub row: BuildStmtId,
    pub slot: usize,
    pub initializer: BuildExprId,
    pub initializer_source: crate::sema::check::ExpressionIdentity,
    pub source_type: ScopedRoot,
    pub initializer_type: ScopedRoot,
}

fn problem(message: &'static str) -> IrBuildError { IrBuildError::format(message, None, 0, 0) }

fn export_contract(pools: &SemanticPools, signature: SignatureId, kind: CallableKind) -> Result<ModuleExportContract, IrVerifyError> {
    let count = pools.signature_param_count(signature)?;
    if count > 65536 { return Err(IrVerifyError::new("module export parameter count exceeds its bound")); }
    let parameters = (0..count).map(|index| {
        let (label, ty, _) = pools.signature_param(signature, index)?;
        Ok(ModuleExportParameter { label: Arc::from(label.as_str().as_str()), ty: pools.to_type(ty)?,
            mode: pools.signature_parameter_mode(signature, index)?, defaulted: pools.signature_parameter_defaulted(signature, index)?, rest: pools.signature_parameter_rest(signature, index)? })
    }).collect::<Result<Vec<_>, IrVerifyError>>()?;
    Ok(ModuleExportContract { kind, parameters: parameters.into_boxed_slice(), result: pools.to_type(pools.signature_return_type(signature)?)?, effects: pools.signature_closed_effects(signature)? })
}

impl FullBuilder {
    pub(super) fn stage_original_module_capture(&mut self, target: IrFunctionId, declaration: Option<crate::sema::check::DeclarationIdentity>, header_index: u32, original: &LoweredTopLevelSlot) -> Result<(), IrBuildError> {
        if self.checked_original_module_capture(declaration, header_index, original)? {
            self.original_module_capture_rows.push((target, declaration.ok_or_else(|| problem("module_capture_original_declaration_missing"))?, header_index, original.clone()));
        }
        Ok(())
    }

    pub(super) fn stage_module_statement_binding(&mut self, row: BuildStmtId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some((binding, original)) = self.active_module_bindings.get(&row).cloned() else { return Ok(()); };
        let (initializer, owner) = self.encoded_original_module_binding(&original, instruction, scratch)?;
        self.generic_evidence_mut().register_instruction_origin(instruction, super::super::generic::OperationSourceOrigin::Statement(original.statement), owner)
            .map_err(|_| problem("module_binding_original_statement_source"))?;
        self.module_binding_rows.push((binding, original, instruction, initializer, owner));
        Ok(())
    }

    fn original_module_receiver_allocation(&self, solved: &crate::sema::check::SolvedTypes, origin: crate::sema::check::ExpressionIdentity,
        instruction: u32, owner: InstructionOwner) -> Result<ModuleReceiverAllocation, IrBuildError> {
        if self.store.tags.get(instruction as usize) != Some(&FullTag::ExprParam) { return Err(problem("module_receiver_original_read_required")); }
        let caller = solved.expression_owners.get(&origin).copied();
        if let Some((declaration, slot)) = super::super::super::BuildIterationBindingOrigin::original_parameter(solved, origin, caller)
            .ok_or_else(|| problem("module_receiver_original_parameter_flow"))? {
            if self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("module_receiver_original_parameter_payload"))? != [slot] {
                return Err(problem("module_receiver_original_parameter_changed"));
            }
            return Ok(ModuleReceiverAllocation::Parameter { declaration, slot });
        }
        let flow = *solved.expression_producer_flows.get(&origin).ok_or_else(|| problem("module_receiver_original_flow_missing"))?;
        let node = solved.producer_flows.node(flow).map_err(|_| problem("module_receiver_original_flow_owner"))?;
        let crate::sema::check::ProducerFlowKind::CapturedBinding { identity, version, input } = node.kind else { return Err(problem("module_receiver_original_binding_required")); };
        if node.source != crate::sema::check::ProducerFlowSource::Expression(origin) || solved.binding_producer_flows.get(&(identity, version)) != Some(&input) {
            return Err(problem("module_receiver_original_binding_flow_changed"));
        }
        let definition = solved.bindings.get(&identity).ok_or_else(|| problem("module_receiver_original_binding_missing"))?;
        if definition.mutable { return Err(problem("module_receiver_mutable_binding_not_prepared")); }
        let lexical = definition.owner.map(|owner| solved.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or_else(|| problem("module_receiver_original_definition_owner_missing"))).transpose()?;
        let source_type = ScopedRoot { ty: definition.ty, scope: definition.scheme.or(lexical) };
        solved.graph.validate_scoped(source_type).map_err(|_| problem("module_receiver_original_binding_scope"))?;
        if definition.owner == caller {
            let rows = self.module_binding_rows.iter().filter(|(binding, _, _, _, actual_owner)| *binding == identity && *actual_owner == owner).collect::<Vec<_>>();
            let [(binding, original, statement, initializer, _)] = rows.as_slice() else { return Err(problem("module_receiver_original_allocation_missing")); };
            if original.source_type != source_type || solved.expressions.get(&original.initializer_source) != Some(&original.initializer_type.ty)
                || solved.expression_owners.get(&original.initializer_source).copied() != definition.owner
                || solved.expression_scope(original.initializer_source, definition.owner).map_err(|_| problem("module_receiver_original_initializer_scope"))? != original.initializer_type.scope {
                return Err(problem("module_receiver_original_initializer_changed"));
            }
            solved.graph.validate_scoped(original.initializer_type).map_err(|_| problem("module_receiver_original_initializer_scope"))?;
            let slot = u32::try_from(original.slot).map_err(|_| problem("module_receiver_original_slot_overflow"))?;
            if self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("module_receiver_original_read_payload"))? != [slot] {
                return Err(problem("module_receiver_original_read_changed"));
            }
            return Ok(ModuleReceiverAllocation::Local { binding: *binding, statement: original.statement, instruction: *statement,
                initializer: *initializer, initializer_origin: original.initializer_source, slot, source_type, initializer_type: original.initializer_type });
        }
        let (InstructionOwner::Function(target), Some(declaration)) = (owner, caller) else { return Err(problem("module_receiver_original_capture_owner")); };
        let rows = self.original_module_capture_rows.iter().filter(|(actual_target, actual_declaration, _, allocation)| *actual_target == target && *actual_declaration == declaration && allocation.lexical_binding == Some(identity)).collect::<Vec<_>>();
        let [(_, _, header_index, allocation)] = rows.as_slice() else { return Err(problem("module_receiver_original_capture_allocation_missing")); };
        if allocation.source_type != Some(source_type) || allocation.mutable { return Err(problem("module_receiver_original_capture_changed")); }
        let slot = u32::try_from(allocation.slot).map_err(|_| problem("module_receiver_original_capture_slot_overflow"))?;
        if self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("module_receiver_original_capture_payload"))? != [slot] {
            return Err(problem("module_receiver_original_capture_read_changed"));
        }
        let header = self.store.captures.get(*header_index as usize).ok_or_else(|| problem("module_receiver_original_capture_header_missing"))?;
        Ok(ModuleReceiverAllocation::Capture { binding: identity, declaration, header_index: *header_index, slot, name: header.name, ty: header.type_id, source_type })
    }

    pub(super) fn checked_original_module_capture(&mut self, declaration: Option<crate::sema::check::DeclarationIdentity>, header_index: u32, original: &LoweredTopLevelSlot) -> Result<bool, IrBuildError> {
        let (Some(declaration), Some(binding), Some(root)) = (declaration, original.lexical_binding, original.source_type) else { return Ok(false); };
        let solved = self.solved.clone().ok_or_else(|| problem("module_capture_original_graph_missing"))?;
        let resolved = solved.graph.resolved(root.ty).map_err(|_| problem("module_capture_original_type_owner"))?;
        if !matches!(solved.graph.node(resolved).map_err(|_| problem("module_capture_original_type_owner"))?, TypeNode::Module(_)) { return Ok(false); }
        if original.mutable || original.host_binding.is_some() { return Ok(false); }
        let definition = solved.bindings.get(&binding).ok_or_else(|| problem("module_capture_original_binding_missing"))?;
        let lexical = definition.owner.map(|owner| solved.declarations.get(&owner).map(|definition| definition.scheme).ok_or_else(|| problem("module_capture_original_definition_owner_missing"))).transpose()?;
        if definition.mutable || definition.owner == Some(declaration) || definition.ty != root.ty || root.scope != definition.scheme.or(lexical) {
            return Err(problem("module_capture_original_binding_changed"));
        }
        solved.declarations.get(&declaration).ok_or_else(|| problem("module_capture_original_declaration_missing"))?;
        solved.graph.validate_scoped(root).map_err(|_| problem("module_capture_original_scope"))?;
        let slot = u32::try_from(original.slot).map_err(|_| problem("module_capture_original_slot_overflow"))?;
        let ty = self.intern_checked_slot_type(original)?;
        let header = self.store.captures.get(header_index as usize).ok_or_else(|| problem("module_capture_original_header_missing"))?;
        if header.slot_and_flags != slot || header.type_id != ty
            || self.store.string(header.name).map_err(|_| problem("module_capture_original_header_name"))? != original.name.as_str().as_str() {
            return Err(problem("module_capture_original_header_changed"));
        }
        Ok(true)
    }

    pub(super) fn encoded_original_module_binding(&self, original: &BuildModuleBindingOrigin, instruction: u32, scratch: &BuildScratch) -> Result<(u32, InstructionOwner), IrBuildError> {
        if !matches!(scratch.statements.get(original.row.index()), Some(BuildStmtRow::Let { slot, value }) if *slot == original.slot && *value == original.initializer)
            || self.store.tags.get(instruction as usize) != Some(&FullTag::StmtLet) {
            return Err(problem("module_binding_original_allocation_changed"));
        }
        let initializer = *self.active_encoded_expressions.get(&original.initializer).ok_or_else(|| problem("module_binding_original_initializer_missing"))?;
        let slot = u32::try_from(original.slot).map_err(|_| problem("module_binding_original_slot_overflow"))?;
        if self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("module_binding_original_payload"))? != [slot, initializer] {
            return Err(problem("module_binding_original_allocation_changed"));
        }
        let raw = self.current_owner.ok_or_else(|| problem("module_binding_original_owner_missing"))?;
        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| problem("module_binding_original_owner_invalid"))?) };
        Ok((initializer, owner))
    }

    pub(super) fn prepare_module_invocations(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        let origins = self.generic_expression_rows.iter().map(|&(instruction, expression, owner)| (instruction, (expression, owner))).collect::<FxHashMap<_, _>>();
        for (instruction, origin, owner) in self.generic_expression_rows.clone() {
            if self.store.tags[instruction as usize] != FullTag::ExprDynamicCall { continue; }
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("module_invocation_payload"))?.to_vec();
            let callee_instruction = *words.first().ok_or_else(|| problem("module_invocation_callee"))?;
            let Some(&(callee_origin, callee_owner)) = origins.get(&callee_instruction) else { continue; };
            let Some(projection) = solved.module_projections.get(&callee_origin) else { continue; };
            if projection.kind != crate::sema::check::ModuleProjectionKind::Field || projection.optional || owner != callee_owner
                || solved.expression_owners.get(&origin).copied() != projection.caller
                || solved.expression_owners.get(&callee_origin).copied() != projection.caller
                || solved.expressions.get(&callee_origin) != Some(&projection.result)
                || solved.expressions.get(&projection.receiver) != Some(&projection.source)
                || projection.result != projection.field_type {
                return Err(problem("module_invocation_original_projection_changed"));
            }
            let field = solved.graph.module_field(projection.source, projection.field).map_err(|_| problem("module_invocation_original_module"))?;
            let relation = ConstraintRelation::ModuleProjection { module: projection.source, label: projection.field, result: projection.field_type, optional: false };
            if field.ty != projection.field_type || field.optional
                || solved.graph.constraint_origins().get(projection.contribution).is_none_or(|constraint| constraint.relation != relation) {
                return Err(problem("module_invocation_original_export_changed"));
            }
            let ty = solved.graph.resolved(projection.field_type).map_err(|_| problem("module_invocation_export_owner"))?;
            let TypeNode::Arrow(arrow) = solved.graph.node(ty).map_err(|_| problem("module_invocation_export_owner"))? else { return Err(problem("module_invocation_export_arrow_required")); };
            if arrow.params.iter().any(|parameter| parameter.defaulted || parameter.rest) { return Err(problem("module_invocation_export_default_or_rest_not_prepared")); }
            let descriptor = self.intern_checked_callable_type(&solved.graph, ty)?;
            let (kind, signature) = self.store.semantic.callable_descriptor(descriptor).map_err(|_| problem("module_invocation_export_descriptor"))?.ok_or_else(|| problem("module_invocation_export_descriptor"))?;
            let contract = export_contract(&self.store.semantic, signature, kind).map_err(|_| problem("module_invocation_export_contract"))?;
            if contract.parameters.iter().any(|parameter| parameter.mode != ParameterMode::PositionalOrNamed) { return Err(problem("module_invocation_named_only_not_prepared")); }
            let (call_signature, supplied, defaults, rest, dynamic) = if let Some(call) = solved.calls.get(&origin) {
                if call.caller != projection.caller { return Err(problem("module_invocation_original_call_owner_changed")); }
                (call.signature, call.binding.supplied_slots.clone(), call.binding.default_slots.clone(), call.binding.rest_slot, call.binding.dynamic.clone())
            } else if let Some(invocation) = solved.invocations.get(&origin) {
                if invocation.caller != projection.caller { return Err(problem("module_invocation_original_call_owner_changed")); }
                let evidence = solved.graph.invocation_evidence(invocation.requirement).map_err(|_| problem("module_invocation_original_evidence"))?.ok_or_else(|| problem("module_invocation_original_evidence"))?;
                let (signature, binding, timing) = evidence.unique_plan().ok_or_else(|| problem("module_invocation_unique_plan"))?;
                if timing != InvocationDefaultTiming::AtCall || !evidence.native_alternatives.is_empty() { return Err(problem("module_invocation_original_timing_or_authority")); }
                (signature, binding.supplied_slots.clone(), binding.default_slots.clone(), binding.rest_slot, binding.dynamic.clone())
            } else { return Err(problem("module_invocation_original_call_missing")); };
            if !defaults.is_empty() || rest.is_some() || dynamic.is_some() || supplied.iter().copied().ne(0..contract.parameters.len()) {
                return Err(problem("module_invocation_binding_not_prepared"));
            }
            let checked = self.intern_checked_callable_type(&solved.graph, call_signature)?;
            if self.store.semantic.callable_descriptor(checked).map_err(|_| problem("module_invocation_call_descriptor"))? != Some((kind, signature)) { return Err(problem("module_invocation_original_signature_changed")); }
            if self.store.tags[callee_instruction as usize] != FullTag::ExprField { return Err(problem("module_invocation_callee_field_required")); }
            let projection_words = self.store.payload(self.store.data[callee_instruction as usize].range()).map_err(|_| problem("module_invocation_projection_payload"))?;
            let receiver_instruction = *projection_words.first().ok_or_else(|| problem("module_invocation_receiver_missing"))?;
            let field = projection_words.get(1).copied().ok_or_else(|| problem("module_invocation_field_missing"))?;
            if origins.get(&receiver_instruction) != Some(&(projection.receiver, owner))
                || self.store.string(field).map_err(|_| problem("module_invocation_field"))? != projection.field.as_str().as_str() {
                return Err(problem("module_invocation_original_receiver_changed"));
            }
            let block = words.get(1).copied().and_then(IrBlockId::from_raw).and_then(|id| self.store.blocks.get(id.index())).ok_or_else(|| problem("module_invocation_arguments"))?;
            let encoded = self.store.payload(block.instructions).map_err(|_| problem("module_invocation_arguments"))?.to_vec();
            let count = *encoded.first().ok_or_else(|| problem("module_invocation_arguments"))? as usize;
            let recipes = solved.argument_sources.get(&origin).ok_or_else(|| problem("module_invocation_original_recipes"))?;
            if count != contract.parameters.len() || count != recipes.len() || encoded.len() != 1 + count * 2 { return Err(problem("module_invocation_original_arity_changed")); }
            let mut arguments = Vec::new();
            for (ordinal, (recipe, encoded)) in recipes.iter().zip(encoded[1..].chunks_exact(2)).enumerate() {
                if encoded[0] != 0 { return Err(problem("module_invocation_splice_or_omission_not_prepared")); }
                let actual = self.original_argument_expression(encoded[1], origin, ordinal, recipe, owner)?;
                let ty = *solved.expressions.get(&actual).ok_or_else(|| problem("module_invocation_original_operand_type"))?;
                let ty = super::super::generic::graph_ground_type(&solved.graph, ty).map_err(|_| problem("module_invocation_operand_scope"))?;
                let ty = TypeRef::Ground(self.intern_generic_ground_type(&ty)?);
                arguments.push(PreparedInvocationArgument { original: recipe.clone(), instruction: encoded[1], ty });
            }
            let scope = solved.expression_scope(callee_origin, projection.caller).map_err(|_| problem("module_invocation_export_scope"))?;
            let receiver_root = ScopedRoot { ty: projection.source, scope: solved.expression_scope(projection.receiver, projection.caller).map_err(|_| problem("module_invocation_receiver_scope"))? };
            let export_root = ScopedRoot { ty: projection.field_type, scope };
            let result_root = ScopedRoot {
                ty: *solved.expressions.get(&origin).ok_or_else(|| problem("module_invocation_original_result_missing"))?,
                scope: solved.expression_scope(origin, projection.caller).map_err(|_| problem("module_invocation_result_scope"))?,
            };
            solved.graph.validate_scoped(receiver_root).map_err(|_| problem("module_invocation_receiver_scope"))?;
            solved.graph.validate_scoped(export_root).map_err(|_| problem("module_invocation_export_scope"))?;
            solved.graph.validate_scoped(result_root).map_err(|_| problem("module_invocation_result_scope"))?;
            solved.graph.validate_scoped(ScopedRoot { ty: call_signature, scope: result_root.scope }).map_err(|_| problem("module_invocation_call_signature_scope"))?;
            let selected = solved.graph.resolved(call_signature).map_err(|_| problem("module_invocation_selected_signature"))?;
            let TypeNode::Arrow(selected) = solved.graph.node(selected).map_err(|_| problem("module_invocation_selected_signature"))? else { return Err(problem("module_invocation_selected_arrow_required")); };
            if super::super::generic::graph_ground_type(&solved.graph, selected.result).map_err(|_| problem("module_invocation_selected_result_requires_scope"))? != contract.result {
                return Err(problem("module_invocation_selected_result_changed"));
            }
            if super::super::generic::graph_ground_type(&solved.graph, result_root.ty).map_err(|_| problem("module_invocation_original_result_requires_scope"))? != contract.result {
                return Err(problem("module_invocation_original_result_changed"));
            }
            let receiver_allocation = self.original_module_receiver_allocation(&solved, projection.receiver, receiver_instruction, owner)?;
            self.generic_evidence_mut().add_module_invocation_source(ModuleInvocationSource { origin, instruction, owner, callee_origin, callee_instruction,
                receiver_origin: projection.receiver, receiver_instruction, receiver_root, receiver_allocation, export_root, result_root, field: Arc::from(projection.field.as_str().as_str()), signature, contract, arguments: arguments.into_boxed_slice() })
                .map_err(|_| problem("module_invocation_source_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(super) fn verify_module_invocation_sources(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for source in generic.module_invocation_sources() { Self::verify_module_invocation_source(store, generic, source)?; }
        Ok(())
    }
    pub(super) fn verify_module_invocation_source(store: &FullStore, generic: &GenericEvidenceStore, source: &ModuleInvocationSource) -> Result<(), IrVerifyError> {
        Self::verify_module_invocation_source_inner(store, generic, source, None, &mut vec![source.instruction])
    }

    pub(super) fn verify_module_invocation_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner,
        expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(source) = generic.module_invocation_source(instruction)? else { return Ok(false); };
        if source.owner != owner || source.contract.result != *expected {
            return Err(IrVerifyError::new("module invocation changes its original result or consumer owner"));
        }
        let already_active = active.last() == Some(&instruction);
        if active.len() >= 256 || (!already_active && active.contains(&instruction)) { return Err(IrVerifyError::new("module invocation result is cyclic or too deep")); }
        if !already_active { active.push(instruction); }
        let result = Self::verify_module_invocation_source_inner(store, generic, source, instance, active);
        if !already_active { active.pop(); }
        result.map(|_| true)
    }

    fn verify_module_invocation_source_inner(store: &FullStore, generic: &GenericEvidenceStore, source: &ModuleInvocationSource,
        instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        if !generic.module_invocation_source(source.instruction)?.is_some_and(|original| std::ptr::eq(original, source)) {
            return Err(IrVerifyError::new("module invocation lacks its original protected source"));
        }
        if store.tags.get(source.instruction as usize) != Some(&FullTag::ExprDynamicCall) || store.tags.get(source.callee_instruction as usize) != Some(&FullTag::ExprField) {
            return Err(IrVerifyError::new("module invocation changes its original call or field opcode"));
        }
        let words = store.payload(store.data[source.instruction as usize].range())?;
        let field = store.payload(store.data[source.callee_instruction as usize].range())?;
        if words.first() != Some(&source.callee_instruction) || field.first() != Some(&source.receiver_instruction)
            || field.get(1).copied().map(|field| store.string(field)).transpose()? != Some(source.field.as_ref())
            || export_contract(&store.semantic, source.signature, source.contract.kind)? != source.contract {
            return Err(IrVerifyError::new("module invocation changes its original receiver, export, or signature"));
        }
        let slot = match source.receiver_allocation {
            ModuleReceiverAllocation::Local { instruction, initializer, slot, .. } => {
                if store.tags.get(instruction as usize) != Some(&FullTag::StmtLet)
                    || store.payload(store.data.get(instruction as usize).ok_or_else(|| IrVerifyError::new("module receiver binding is missing"))?.range())? != [slot, initializer] {
                    return Err(IrVerifyError::new("module receiver changes its original immutable allocation or initializer"));
                }
                slot
            }
            ModuleReceiverAllocation::Capture { declaration, header_index, slot, name, ty, .. } => {
                let function = generic.checked_function(declaration)?;
                if source.owner != InstructionOwner::Function(function.target) { return Err(IrVerifyError::new("module receiver changes its original capture declaration")); }
                let range = store.functions.get(function.target.index()).ok_or_else(|| IrVerifyError::new("module receiver capture function is missing"))?.captures
                    .bounds(store.captures.len()).ok_or_else(|| IrVerifyError::new("module receiver capture range is invalid"))?;
                let header = store.captures.get(header_index as usize).filter(|_| range.contains(&(header_index as usize))).ok_or_else(|| IrVerifyError::new("module receiver changes its original capture header owner"))?;
                if header.slot_and_flags != slot || header.name != name || header.type_id != ty {
                    return Err(IrVerifyError::new("module receiver changes its original capture allocation"));
                }
                slot
            }
            ModuleReceiverAllocation::Parameter { declaration, slot } => {
                if source.owner != InstructionOwner::Function(generic.checked_function(declaration)?.target) {
                    return Err(IrVerifyError::new("module receiver changes its original parameter declaration"));
                }
                slot
            }
        };
        if store.tags.get(source.receiver_instruction as usize) != Some(&FullTag::ExprParam)
            || store.payload(store.data.get(source.receiver_instruction as usize).ok_or_else(|| IrVerifyError::new("module receiver read is missing"))?.range())? != [slot] {
            return Err(IrVerifyError::new("module receiver changes its original allocated read slot"));
        }
        let original_initializer = match source.receiver_allocation { ModuleReceiverAllocation::Local { instruction, .. } => Some(instruction), _ => None };
        Self::verify_module_receiver_slot_writes(store, original_initializer, slot, source.owner)?;
        let block = words.get(1).copied().and_then(IrBlockId::from_raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("module invocation argument block is missing"))?;
        let encoded = store.payload(block.instructions)?;
        if encoded.first().copied() != Some(source.arguments.len() as u32) || encoded.len() != 1 + source.arguments.len() * 2 { return Err(IrVerifyError::new("module invocation changes its original argument arity")); }
        for (ordinal, (argument, encoded)) in source.arguments.iter().zip(encoded[1..].chunks_exact(2)).enumerate() {
            if encoded != [0, argument.instruction] { return Err(IrVerifyError::new("module invocation changes its original argument order or operand")); }
            if !generic.argument_has_original_source(argument.instruction, source.origin, ordinal, &argument.original, source.owner, argument.ty) {
                return Err(IrVerifyError::new("module invocation operand lost its independent original source"));
            }
            let TypeRef::Ground(ty) = argument.ty else { return Err(IrVerifyError::new("module invocation operand is not ground")); };
            Self::verify_generic_source(store, generic, argument.instruction, source.owner, &store.semantic.to_type(ty)?, instance, active)?;
        }
        Ok(())
    }

    pub(super) fn verify_module_receiver_dominance(store: &FullStore, tree: &super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref().filter(|generic| generic.module_invocation_sources().next().is_some()) else { return Ok(()); };
        let index = super::callable_prepare::CallableLexicalIndex::new(store, tree)?;
        for source in generic.module_invocation_sources() {
            if let ModuleReceiverAllocation::Local { instruction, .. } = source.receiver_allocation {
                if !index.dominates(tree, instruction, source.receiver_instruction)? {
                    return Err(IrVerifyError::new("module receiver is outside its original immutable binding scope"));
                }
            }
        }
        Ok(())
    }

    fn verify_module_receiver_slot_writes(store: &FullStore, binding: Option<u32>, slot: u32, owner: InstructionOwner) -> Result<(), IrVerifyError> {
        let range = match owner { InstructionOwner::Function(owner) => store.function_instruction_range(owner.index())?, InstructionOwner::Driver(owner) => store.driver_instruction_range(owner as usize)? };
        for instruction in range {
            if binding == Some(instruction as u32) { continue; }
            if matches!(store.tags[instruction], FullTag::StmtLet | FullTag::StmtLetInt | FullTag::StmtLetBool | FullTag::StmtAssign | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath | FullTag::StmtAssignInt | FullTag::StmtAssignBool)
                && store.payload(store.data[instruction].range())?.first() == Some(&slot) {
                return Err(IrVerifyError::new("module receiver's immutable allocation is written"));
            }
        }
        Ok(())
    }
}

impl FullFunctionView<'_> {
    pub(in crate::runtime::eval) fn module_export_contract(&self) -> Result<ModuleExportContract, IrVerifyError> {
        if self.generic_scope().is_some() { return Err(IrVerifyError::new("module export implementation requires a generic instance")); }
        let function = &self.program.store.functions[self.index];
        let metadata = &self.program.store.function_metadata[self.index];
        let signature = SignatureId::from_raw(function.signature).ok_or_else(|| IrVerifyError::new("module export implementation signature is missing"))?;
        let kind = if metadata.flags & 1 != 0 { CallableKind::Proc } else { CallableKind::Pure };
        self.program.symbol_owner().with_current(|| export_contract(&self.program.store.semantic, signature, kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;

    fn fixture() -> FullProgram {
        let source = "type Plugin = module { export pure render(value: Str) -> Str }\nproc caller() [fs, error] -> Result[Str] { let plugin = module.load(p\"/unused-original-export.xsh\")?.require(Plugin)?; plugin.render(\"hi\") }\n";
        fixture_source(source)
    }

    fn fixture_source(source: &str) -> FullProgram {
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            "module-invocation-proof.xsh", crate::loader::entry_source_from_text("module-invocation-proof.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = sources.files()[0].id();
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let solved = Arc::downgrade(&bodies.solved);
        let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap();
        drop(parsed); drop(declarations); drop(bodies);
        assert!(solved.upgrade().is_none(), "the export proof cannot retain checker ownership");
        program
    }

    #[test]
    fn original_module_invocation_refuses_same_schema_capture_header_substitution() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture_source("type Plugin = module { export pure render(value: Str) -> Str }\nlet first = module.load(p\"/unused-first.xsh\")?.require(Plugin)?\nlet second = module.load(p\"/unused-second.xsh\")?.require(Plugin)?\npure caller() -> Str { let _ = first.render(\"left\"); second.render(\"right\") }\n");
            FullVerifier::verify(&program).unwrap();
            let sources = program.store.generic.as_deref().unwrap().module_invocation_sources().cloned().collect::<Vec<_>>();
            assert_eq!(sources.len(), 2);
            assert_eq!(sources[0].contract, sources[1].contract);
            let ModuleReceiverAllocation::Capture { header_index: first, slot: first_slot, .. } = sources[0].receiver_allocation else { panic!("the first original module is captured"); };
            let ModuleReceiverAllocation::Capture { header_index: second, slot: second_slot, .. } = sources[1].receiver_allocation else { panic!("the second original module is captured"); };
            assert_ne!(first_slot, second_slot);
            let mut read = program.clone();
            let payload = read.store.data[sources[0].receiver_instruction as usize].range().start as usize;
            read.store.extra[payload] = second_slot;
            assert!(FullVerifier::verify(&read).is_err(), "equal export schemas cannot replace the actual captured read allocation");
            let mut header = program.clone();
            header.store.captures[first as usize].name = header.store.captures[second as usize].name;
            assert!(FullVerifier::verify(&header).is_err(), "the captured name belongs to its original module binding");
            let mut joint = read;
            joint.store.captures[first as usize] = joint.store.captures[second as usize];
            assert!(FullVerifier::verify(&joint).is_err(), "joint header and read rewrites cannot replace an original capture allocation");
        });
    }

    #[test]
    fn original_module_invocation_refuses_same_schema_receiver_slot_substitution() {
        struct ModuleFiles(std::path::PathBuf);
        impl Drop for ModuleFiles {
            fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
        }
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let files = ModuleFiles(std::env::temp_dir().join(format!("xsh-module-receiver-source-{}-{stamp}", std::process::id())));
        std::fs::create_dir_all(&files.0).unwrap();
        for name in ["first.xsh", "second.xsh"] {
            std::fs::write(files.0.join(name), "##! A loaded text renderer.\n## Return the supplied text.\nexport pure render(value: Str) -> Str { value }\n").unwrap();
        }
        let source = format!(r#"type Plugin = module {{ export pure render(value: Str) -> Str }}
proc caller() [fs, error] -> Result[Str] {{
  let first = module.load(p"{}")?.require(Plugin)?
  let second = module.load(p"{}")?.require(Plugin)?
  let _ = first.render("left")
  second.render("right")
}}
"#, files.0.join("first.xsh").display(), files.0.join("second.xsh").display());
        crate::runtime::eval::run_eval(|| {
            let program = fixture_source(&source);
            FullVerifier::verify(&program).unwrap();
            let sources = program.store.generic.as_deref().unwrap().module_invocation_sources().cloned().collect::<Vec<_>>();
            assert_eq!(sources.len(), 2);
            assert_eq!(sources[0].contract, sources[1].contract);
            assert_eq!(program.store.tags[sources[0].receiver_instruction as usize], FullTag::ExprParam);
            assert_eq!(program.store.tags[sources[1].receiver_instruction as usize], FullTag::ExprParam);
            let first = program.store.data[sources[0].receiver_instruction as usize].range().start as usize;
            let second = program.store.data[sources[1].receiver_instruction as usize].range().start as usize;
            assert_ne!(program.store.extra[first], program.store.extra[second], "the two real loaded module bindings have distinct original slots");
            let mut changed = program.clone();
            changed.store.extra[first] = changed.store.extra[second];
            assert!(FullVerifier::verify(&changed).is_err(), "an equal-schema module cannot replace the original receiver binding allocation");
        });
    }

    #[test]
    fn original_module_invocation_refuses_missing_foreign_and_joint_projection_rewrites_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let program = fixture();
            FullVerifier::verify(&program).unwrap();
            let source = program.store.generic.as_deref().unwrap().module_invocation_sources().next().unwrap().clone();
            assert_eq!(source.field.as_ref(), "render");
            assert_eq!(source.contract.parameters.len(), 1);
            assert_eq!(source.contract.parameters[0].ty, Type::Str);
            assert_eq!(source.contract.result, Type::Str);
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_clear_module_invocations();
            assert!(FullVerifier::verify(&missing).is_err(), "the callable signature alone cannot replace its original export source");
            let other = fixture();
            let mut foreign = program.clone();
            foreign.store.generic.as_deref_mut().unwrap().test_replace_module_invocations(other.store.generic.as_deref().unwrap());
            assert!(FullVerifier::verify(&foreign).is_err(), "equal export layouts cannot replace the original program owner");
            let mut changed_field = program.clone();
            let field = changed_field.store.data[source.callee_instruction as usize].range().start as usize + 1;
            changed_field.store.extra[field] = source.receiver_instruction;
            assert!(FullVerifier::verify(&changed_field).is_err());
            let mut joint = program.clone();
            let receiver = joint.store.data[source.callee_instruction as usize].range().start as usize;
            joint.store.extra[receiver] = source.arguments[0].instruction;
            joint.store.generic.as_deref_mut().unwrap().test_module_invocation_mut(source.instruction).unwrap().receiver_instruction = source.arguments[0].instruction;
            assert!(FullVerifier::verify(&joint).is_err(), "changing a dependent receipt cannot replace the sealed original module receiver");
            let mut changed_argument = program.clone();
            let words = changed_argument.store.payload(changed_argument.store.data[source.instruction as usize].range()).unwrap();
            let block = IrBlockId::from_raw(words[1]).unwrap();
            let operand = changed_argument.store.blocks[block.index()].instructions.start as usize + 2;
            changed_argument.store.extra[operand] = source.receiver_instruction;
            assert!(FullVerifier::verify(&changed_argument).is_err());
            let mut changed_contract = program.clone();
            changed_contract.store.generic.as_deref_mut().unwrap().test_module_invocation_mut(source.instruction).unwrap().contract.parameters[0].ty = Type::Int;
            assert!(FullVerifier::verify(&changed_contract).is_err(), "the original export arrow remains independent of a dependent signature copy");
        });
    }
}
