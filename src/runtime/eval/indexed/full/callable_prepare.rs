use super::*;
use super::super::generic::{CallableKind, CallableValueSource, GroundUserInvocationContract, InvocationSource, PreparedCallableValue, PreparedInvocationArgument, PreparedInvocationPlan, PreparedOperationBinding, UserCallableContract};
use crate::sema::inference::{EffectSet, EffectSummary, InvocationDefaultTiming, SolvedGraph, TypeNode};

fn problem(message: &'static str) -> IrBuildError { IrBuildError::format(message, None, 0, 0) }

pub(super) struct CallableLexicalIndex {
    statements: FxHashMap<u32, (IrBlockId, usize)>,
    parents: FxHashMap<IrBlockId, Option<u32>>,
}

impl CallableLexicalIndex {
    pub(super) fn new(store: &FullStore, tree: &super::super::pattern::PatternTree) -> Result<Self, IrVerifyError> {
        let mut index = Self { statements: FxHashMap::default(), parents: FxHashMap::default() };
        for (raw, block) in store.blocks.iter().enumerate() {
            if block.owner == IR_NONE || block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_STATEMENTS { continue; }
            let block_id = IrBlockId::new(raw).map_err(|_| IrVerifyError::new("callable lexical block index is invalid"))?;
            let words = store.payload(block.instructions)?;
            if words.first().copied().map(|count| count as usize) != Some(words.len().saturating_sub(1)) { return Err(IrVerifyError::new("callable lexical block length is invalid")); }
            let mut parent = None;
            for (ordinal, &statement) in words.iter().skip(1).enumerate() {
                let actual = tree.parent(statement)?;
                if ordinal == 0 { parent = actual; } else if parent != actual { return Err(IrVerifyError::new("callable statement block has conflicting lexical parents")); }
                if index.statements.insert(statement, (block_id, ordinal)).is_some() { return Err(IrVerifyError::new("callable statement belongs to multiple lexical blocks")); }
            }
            index.parents.insert(block_id, parent);
        }
        Ok(index)
    }

    fn position(&self, tree: &super::super::pattern::PatternTree, mut instruction: u32) -> Result<Option<(IrBlockId, usize)>, IrVerifyError> {
        for _ in 0..512 {
            if let Some(&position) = self.statements.get(&instruction) { return Ok(Some(position)); }
            let Some(parent) = tree.parent(instruction)? else { return Ok(None); };
            instruction = parent;
        }
        Err(IrVerifyError::new("callable lexical ancestry exceeds its bound"))
    }

    pub(super) fn dominates(&self, tree: &super::super::pattern::PatternTree, declaration: u32, use_: u32) -> Result<bool, IrVerifyError> {
        let Some(&(binding_block, binding_order)) = self.statements.get(&declaration) else { return Ok(false); };
        let Some((mut block, mut order)) = self.position(tree, use_)? else { return Ok(false); };
        for _ in 0..512 {
            if binding_block == block { return Ok(binding_order < order); }
            let Some(parent) = self.parents.get(&block).copied().flatten() else { return Ok(false); };
            let Some(position) = self.position(tree, parent)? else { return Ok(false); };
            (block, order) = position;
        }
        Err(IrVerifyError::new("callable binding scope exceeds its bound"))
    }
}

pub(super) fn closed_effect_names(effects: EffectSet) -> Vec<crate::syntax::node::Effect> {
    use crate::syntax::node::Effect;
    [(EffectSet::FS, Effect::Fs), (EffectSet::NET, Effect::Net), (EffectSet::PROCESS, Effect::Process),
        (EffectSet::ENV, Effect::Env), (EffectSet::TIME, Effect::Time), (EffectSet::ERROR, Effect::Error), (EffectSet::IO, Effect::Io)]
        .into_iter().filter_map(|(bit, effect)| (effects.0 & bit.0 != 0).then_some(effect)).collect()
}

impl FullBuilder {
    pub(super) fn intern_checked_slot_type(&mut self, slot: &LoweredTopLevelSlot) -> Result<TypeId, IrBuildError> {
        if let Some(root) = slot.source_type {
            let solved = self.solved.clone().ok_or_else(|| problem("storage_original_graph_missing"))?;
            solved.graph.validate_scoped(root).map_err(|_| problem("storage_original_type_scope"))?;
            let ty = solved.graph.resolved(root.ty).map_err(|_| problem("storage_original_type_owner"))?;
            match solved.graph.node(ty).map_err(|_| problem("storage_original_type_owner"))? {
                TypeNode::Arrow(_) => return self.intern_checked_callable_type(&solved.graph, ty),
                TypeNode::NativeCallable(_) => return self.intern_checked_native_callable_type(&solved.graph, ty),
                TypeNode::CallableChoice(_) => return Err(problem("storage_callable_authority_not_prepared")),
                _ => {
                    if let Ok(ty) = super::super::generic::graph_ground_type(&solved.graph, ty) { return self.intern_generic_ground_type(&ty); }
                }
            }
        }
        self.intern_lowered_type(slot.kind)
    }
    pub(in crate::runtime::eval) fn intern_checked_callable_type(&mut self, graph: &SolvedGraph, ty: crate::sema::inference::TypeId) -> Result<TypeId, IrBuildError> {
        let (kind, signature) = self.intern_checked_callable_signature(graph, ty)?;
        self.semantic.intern_callable_descriptor(&mut self.store.semantic, kind, signature)
    }

    // Operation signatures can include the effects of consuming a producer.
    // A callable value adds its own kind constraints to this shared signature.
    pub(in crate::runtime::eval) fn intern_checked_callable_signature(&mut self, graph: &SolvedGraph, ty: crate::sema::inference::TypeId) -> Result<(CallableKind, SignatureId), IrBuildError> {
        let mut ty = graph.resolved(ty).map_err(|_| problem("callable_type_owner"))?;
        if let TypeNode::NativeCallable(callable) = graph.node(ty).map_err(|_| problem("callable_type_owner"))?
            && !callable.alternatives.is_empty()
            && callable.alternatives.iter().all(|authority| matches!(authority, crate::sema::inference::CallableAuthority::User { .. })) {
            ty = graph.resolved(callable.signature).map_err(|_| problem("callable_type_owner"))?;
        }
        let TypeNode::Arrow(arrow) = graph.node(ty).map_err(|_| problem("callable_type_owner"))? else { return Err(problem("callable_type_protocol_not_prepared")); };
        let kind = match arrow.kind { crate::sema::inference::CallableKind::Pure => CallableKind::Pure, crate::sema::inference::CallableKind::Proc => CallableKind::Proc, _ => return Err(problem("callable_value_kind_not_prepared")) };
        let EffectSummary::Closed(effects) = graph.closed_effect_summary(arrow.effects).map_err(|_| problem("callable_effect_owner"))? else { return Err(problem("callable_value_requires_effect_scope")); };
        let mut parameters = Vec::with_capacity(arrow.params.len());
        for parameter in &arrow.params {
            let ty = super::super::generic::graph_ground_type(graph, parameter.ty).map_err(|_| problem("callable_parameter_requires_scope"))?;
            let ty = self.semantic.intern_type(&mut self.store.semantic, &ty)?;
            parameters.push((parameter.label, ty, u32::from(parameter.defaulted) | u32::from(parameter.rest) << 1));
        }
        let result = super::super::generic::graph_ground_type(graph, arrow.result).map_err(|_| problem("callable_result_requires_scope"))?;
        let result = self.semantic.intern_type(&mut self.store.semantic, &result)?;
        let effects = closed_effect_names(effects);
        let signature = self.semantic.intern_signature_parts(&mut self.store.semantic, &parameters, result, Some(&effects))?;
        Ok((kind, signature))
    }

    pub(super) fn checked_user_callable(&mut self, solved: &crate::sema::check::SolvedTypes, expression: crate::sema::check::ExpressionIdentity) -> Result<UserCallableContract, IrBuildError> {
        let callable = solved.expression_callables.get(&expression).ok_or_else(|| problem("callable_original_expression_missing"))?;
        let declaration = callable.declaration.ok_or_else(|| problem("callable_user_authority_missing"))?;
        let target = *self.declaration_functions.get(&declaration).ok_or_else(|| problem("callable_original_target_missing"))?;
        if self.generic_declarations.contains_key(&declaration) { return Err(problem("callable_user_instance_not_prepared")); }
        let ty = self.intern_checked_callable_type(&solved.graph, callable.signature)?;
        let (kind, signature) = self.store.semantic.callable_descriptor(ty).map_err(|_| problem("callable_descriptor_invalid"))?.ok_or_else(|| problem("callable_descriptor_missing"))?;
        if self.store.functions[target.index()].signature != signature.raw() { return Err(problem("callable_original_signature_changed")); }
        let creation = self.store.semantic.signature_closed_effects(signature).map_err(|_| problem("callable_effects_not_closed"))?;
        Ok(UserCallableContract { declaration, target, signature, kind, creation })
    }

    pub(super) fn prepare_callable_values(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        let mut prepared_bindings = BTreeMap::new();
        let origins = self.generic_expression_rows.iter().map(|&(instruction, expression, owner)| (instruction, (expression, owner))).collect::<FxHashMap<_, _>>();
        for (instruction, origin, owner) in self.generic_expression_rows.clone() {
            if self.store.tags[instruction as usize] != FullTag::ExprFunctionRef { continue; }
            let contract = self.checked_user_callable(&solved, origin)?;
            let source = self.generic_evidence_mut().add_callable_source(CallableValueSource { origin, instruction, owner, scope: None, expected: contract }).map_err(|_| problem("callable_source_capacity"))?;
            self.generic_evidence_mut().add_callable_value(PreparedCallableValue { source, contract }).map_err(|_| problem("callable_value_capacity"))?;
        }
        for (binding, original, instruction, initializer, owner) in self.callable_binding_rows.clone() {
            let initializer_type = solved.expressions.get(&original.initializer_source).copied().ok_or_else(|| problem("callable_binding_original_initializer_missing"))?;
            let initializer_type = solved.graph.resolved(initializer_type).map_err(|_| problem("callable_binding_original_initializer_owner"))?;
            if let TypeNode::NativeCallable(callable) = solved.graph.node(initializer_type).map_err(|_| problem("callable_binding_original_initializer_owner"))?
                && (callable.alternatives.is_empty() || callable.alternatives.iter().any(|authority| !matches!(authority, crate::sema::inference::CallableAuthority::User { .. }))) { continue; }
            solved.graph.validate_scoped(original.source_type).map_err(|_| problem("callable_binding_type_scope"))?;
            let source_binding = solved.bindings.get(&binding).ok_or_else(|| problem("callable_binding_original_missing"))?;
            if source_binding.ty != original.source_type.ty { return Err(problem("callable_binding_original_type_changed")); }
            let (material, _) = self.argument_initializer_lineage(initializer, owner)?;
            let contract = match self.original_conditional_callable_contract(material, original.initializer_source, owner)? {
                Some(contract) => contract,
                None => self.checked_user_callable(&solved, original.initializer_source)?,
            };
            let descriptor = self.intern_checked_callable_type(&solved.graph, original.source_type.ty)?;
            if self.store.semantic.callable_descriptor(descriptor).map_err(|_| problem("callable_binding_descriptor_owner"))? != Some((contract.kind, contract.signature)) {
                return Err(problem("callable_binding_signature_changed"));
            }
            let slot = u32::try_from(original.slot).map_err(|_| problem("callable_binding_slot_overflow"))?;
            if prepared_bindings.insert(binding, contract).is_some() { return Err(problem("callable_binding_duplicate_original")); }
            self.generic_evidence_mut().add_original_callable_binding(super::super::generic::OriginalCallableBinding {
                binding, statement: original.statement, instruction, owner, slot, initializer,
                initializer_source: original.initializer_source, contract,
            }).map_err(|_| problem("callable_binding_capacity"))?;
        }
        for use_ in self.callable_use_rows.clone() {
            if !prepared_bindings.contains_key(&use_.binding) { continue; }
            self.generic_evidence_mut().add_original_callable_use(use_).map_err(|_| problem("callable_use_capacity"))?;
        }
        self.prepare_callable_receivers(&solved, &origins, &prepared_bindings)?;
        for (instruction, origin, owner) in self.generic_expression_rows.clone() {
            if self.store.tags[instruction as usize] != FullTag::ExprDynamicCall { continue; }
            if solved.invocations.get(&origin).is_some_and(|invocation| solved.graph.invocation_evidence(invocation.requirement).ok().flatten().is_some_and(|evidence| !evidence.native_alternatives.is_empty())) { continue; }
            if self.generic.as_ref().is_some_and(|generic| generic.scoped_invocation_sources().any(|(_, source)| source.instruction == instruction)) { continue; }
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("invocation_payload"))?;
            let callee_instruction = *words.first().ok_or_else(|| problem("invocation_callee_missing"))?;
            let (callee_origin, callee_owner) = if let Some(&(origin, owner)) = origins.get(&callee_instruction) { (origin, owner) }
                else if let Some((receiver, _, owner, _)) = self.callable_receiver_rows.iter().find(|(_, instruction, _, _)| *instruction == callee_instruction) { (receiver.origin, *owner) }
                else { return Err(problem("invocation_original_callee_missing")); };
            if callee_owner != owner { return Err(problem("invocation_callee_owner")); }
            // A method invocation can retain its promise on the method field
            // while the lowered callee reads the original local binding.
            let callable = if let Some(use_) = self.callable_use_rows.iter().find(|use_| use_.instruction == callee_instruction) {
                if use_.origin != callee_origin || use_.owner != owner { return Err(problem("invocation_original_local_source_changed")); }
                *prepared_bindings.get(&use_.binding).ok_or_else(|| problem("invocation_original_local_binding_missing"))?
            } else { self.checked_user_callable(&solved, callee_origin)? };
            let (signature, supplied_slots, default_slots, rest_slot, dynamic, timing, effects) = if let Some(call) = solved.calls.get(&origin) {
                let signature = call.signature;
                (signature, call.binding.supplied_slots.clone(), call.binding.default_slots.clone(), call.binding.rest_slot, call.binding.dynamic.clone(), InvocationDefaultTiming::AtCall, callable.creation)
            } else if let Some(invocation) = solved.invocations.get(&origin) {
                let evidence = solved.graph.invocation_evidence(invocation.requirement).map_err(|_| problem("invocation_evidence_owner"))?.ok_or_else(|| problem("invocation_requires_instance"))?;
                let (signature, binding, timing) = evidence.unique_plan().ok_or_else(|| problem("invocation_all_not_prepared"))?;
                let EffectSummary::Closed(effects) = solved.graph.closed_effect_summary(evidence.effects).map_err(|_| problem("invocation_effect_owner"))? else { return Err(problem("invocation_effect_scope_not_prepared")); };
                (signature, binding.supplied_slots.clone(), binding.default_slots.clone(), binding.rest_slot, binding.dynamic.clone(), timing, effects)
            } else { return Err(problem("invocation_original_binding_missing")); };
            let descriptor = self.intern_checked_callable_type(&solved.graph, signature)?;
            if self.store.semantic.callable_descriptor(descriptor).map_err(|_| problem("invocation_signature_owner"))? != Some((callable.kind, callable.signature)) { return Err(problem("invocation_original_signature_changed")); }
            if rest_slot.is_some() || dynamic.is_some() || timing != InvocationDefaultTiming::AtCall { return Err(problem("invocation_binding_protocol_not_prepared")); }
            let count = self.store.semantic.signature_param_count(callable.signature).map_err(|_| problem("invocation_signature_owner"))?;
            let sources = self.encoded_sources_for_function(instruction, count, callable.target)?;
            let recipes = solved.argument_sources.get(&origin).ok_or_else(|| problem("invocation_original_recipes_missing"))?;
            if supplied_slots.len() != recipes.len() { return Err(problem("invocation_recipe_binding_count")); }
            let mut arguments = Vec::with_capacity(recipes.len());
            let mut operands = Vec::with_capacity(recipes.len());
            for (ordinal, (&slot, recipe)) in supplied_slots.iter().zip(recipes).enumerate() {
                let crate::sema::arguments::ArgumentValueSource::Expression(expression) = recipe.value else { return Err(problem("invocation_recipe_protocol_not_prepared")); };
                let instruction = sources.get(slot).copied().flatten().ok_or_else(|| problem("invocation_supplied_operand_missing"))?;
                let actual = self.original_argument_expression(instruction, origin, ordinal, recipe, owner)?;
                let ty = solved.expressions.get(&actual).copied().ok_or_else(|| problem("invocation_operand_type_missing"))?;
                let ty = super::super::generic::graph_ground_type(&solved.graph, ty).map_err(|_| problem("invocation_operand_requires_scope"))?;
                let ty = self.semantic.intern_type(&mut self.store.semantic, &ty)?;
                arguments.push(PreparedInvocationArgument { original: recipe.clone(), instruction, ty: TypeRef::Ground(ty) });
                operands.push(instruction);
            }
            let result = TypeRef::Ground(self.store.semantic.signature_return_type(callable.signature).map_err(|_| problem("invocation_result_owner"))?);
            let contract = GroundUserInvocationContract { callee_instruction, callee_origin, callable, result, effects, arguments: arguments.into_boxed_slice(),
                binding: PreparedOperationBinding { supplied_slots: supplied_slots.iter().map(|&slot| slot as u32).collect(), default_slots: default_slots.iter().map(|&slot| slot as u32).collect(), rest_slot: None, dynamic: None, operands: operands.into_boxed_slice() }, timing };
            let source = self.generic_evidence_mut().add_invocation_source(InvocationSource { origin, instruction, owner, scope: None, expected: contract.clone() }).map_err(|_| problem("invocation_source_capacity"))?;
            self.generic_evidence_mut().add_invocation_plan(PreparedInvocationPlan { source, contract }).map_err(|_| problem("invocation_plan_capacity"))?;
        }
        Ok(())
    }
}

pub(super) fn checked_storage_kind(pools: &super::super::semantic::SemanticPools, ty: TypeId) -> Result<LoweredType, IrVerifyError> {
    if let Some((kind, _)) = pools.callable_descriptor(ty)? {
        return Ok(match kind { CallableKind::Pure => LoweredType::Pure, CallableKind::Proc => LoweredType::Proc });
    }
    lowered_type_from_type(&pools.to_type(ty)?)
}

impl FullVerifier {
    pub(super) fn verify_local_callable_dominance(store: &FullStore, tree: &super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref().filter(|generic| generic.has_local_callable_bindings() || generic.original_callable_receivers().next().is_some()) else { return Ok(()); };
        for receiver in generic.original_callable_receivers().filter(|receiver| receiver.capture.is_some()) {
            Self::verify_captured_saved_callable_receiver(store, tree, receiver)?;
        }
        let index = CallableLexicalIndex::new(store, tree)?;
        for use_ in generic.original_callable_uses() {
            let binding = generic.original_callable_binding(use_.binding).ok_or_else(|| IrVerifyError::new("callable read lost its original binding"))?;
            if let Some(receiver) = generic.original_callable_receiver(use_.instruction)? {
                Self::verify_saved_callable_receiver(store, tree, &index, binding, receiver)?;
                continue;
            }
            if store.tags.get(use_.instruction as usize) != Some(&FullTag::ExprParam)
                || store.payload(store.data[use_.instruction as usize].range())? != [binding.slot]
                || !index.dominates(tree, binding.instruction, use_.instruction)? {
                return Err(IrVerifyError::new("local callable read is outside its original binding scope"));
            }
        }
        Ok(())
    }

    fn verify_original_callable_initializer(store: &FullStore, generic: &GenericEvidenceStore, binding: &super::super::generic::OriginalCallableBinding) -> Result<(), IrVerifyError> {
        if store.tags.get(binding.instruction as usize) != Some(&FullTag::StmtLet)
            || store.payload(store.data[binding.instruction as usize].range())? != [binding.slot, binding.initializer] {
            return Err(IrVerifyError::new("local callable changes its original allocation or initializer"));
        }
        let mut initializer = binding.initializer;
        let mut seen = Vec::new();
        let range = match binding.owner { InstructionOwner::Function(owner) => store.function_instruction_range(owner.index())?, InstructionOwner::Driver(owner) => store.driver_instruction_range(owner as usize)? };
        while store.tags.get(initializer as usize) == Some(&FullTag::ExprCheckedValue) {
            if !range.contains(&(initializer as usize)) || seen.len() >= 256 || seen.contains(&initializer) { return Err(IrVerifyError::new("local callable initializer is foreign, cyclic, or too deep")); }
            seen.push(initializer);
            initializer = *store.payload(store.data[initializer as usize].range())?.first().ok_or_else(|| IrVerifyError::new("local callable initializer wrapper is empty"))?;
        }
        if !range.contains(&(initializer as usize)) { return Err(IrVerifyError::new("local callable creation belongs to another body")); }
        if Self::verify_conditional_callable_initializer(store, generic, initializer, binding.initializer_source, binding.owner, binding.contract)? { return Ok(()); }
        let value = generic.callable_value_at(initializer)?.ok_or_else(|| IrVerifyError::new("local callable initializer lacks its original creation proof"))?;
        let proof = generic.callable_value(value)?;
        let source = generic.callable_source(proof.source)?;
        if source.owner != binding.owner || source.origin != binding.initializer_source || proof.contract != binding.contract {
            return Err(IrVerifyError::new("local callable initializer changes its original creation authority"));
        }
        Ok(())
    }

    pub(super) fn verify_callable_values(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        let header = |contract: UserCallableContract| {
            let function = store.functions.get(contract.target.index()).ok_or_else(|| IrVerifyError::new("callable target is out of bounds"))?;
            let metadata = store.function_metadata.get(contract.target.index()).ok_or_else(|| IrVerifyError::new("callable target metadata is missing"))?;
            if function.signature != contract.signature.raw() || metadata.flags & 4 != 0
                || (metadata.flags & 1 != 0) != (contract.kind == CallableKind::Proc) { return Err(IrVerifyError::new("callable target changes its checked signature or kind")); }
            Ok((function, metadata))
        };
        for binding in generic.original_callable_bindings() { Self::verify_original_callable_initializer(store, generic, binding)?; }
        for (_, proof) in generic.callable_values() {
            let source = generic.callable_source(proof.source)?;
            let instruction = source.instruction as usize;
            if store.tags.get(instruction) != Some(&FullTag::ExprFunctionRef) { return Err(IrVerifyError::new("callable value proof is attached to another opcode")); }
            let (function, metadata) = header(proof.contract)?;
            let words = store.payload(store.data[instruction].range())?;
            let name = Name::intern(store.string(function.name)?).symbol().raw();
            let pure = u32::from(proof.contract.kind == CallableKind::Pure);
            let matches = if metadata.owner == IR_NONE { words == [0, name, pure] } else {
                words == [1, Name::intern(store.string(metadata.owner)?).symbol().raw(), name, pure]
            };
            if !matches { return Err(IrVerifyError::new("callable value disagrees with its original encoded target or kind")); }
        }
        for (_, proof) in generic.invocation_plans() {
            let source = generic.invocation_source(proof.source)?;
            let instruction = source.instruction as usize;
            if store.tags.get(instruction) != Some(&FullTag::ExprDynamicCall) { return Err(IrVerifyError::new("invocation plan is attached to another opcode")); }
            let contract = &proof.contract;
            let (function, _) = header(contract.callable)?;
            let words = store.payload(store.data[instruction].range())?;
            if words.first() != Some(&contract.callee_instruction) { return Err(IrVerifyError::new("invocation changes its original callee instruction")); }
            let callee = contract.callee_instruction as usize;
            if store.tags.get(callee) != Some(&FullTag::ExprParam) { return Err(IrVerifyError::new("ground user invocation callee carrier is not prepared")); }
            let slot = *store.payload(store.data[callee].range())?.first().ok_or_else(|| IrVerifyError::new("invocation callee slot is missing"))?;
            let ty = if let Some(receiver) = generic.original_callable_receiver(contract.callee_instruction)? {
                Self::verify_saved_callable_receiver_authority(store, generic, receiver, contract.callable, contract.callee_origin, source.owner)?;
                if receiver.slot != slot { return Err(IrVerifyError::new("saved callable receiver changes its original slot")); }
                None
            } else { match source.owner {
                InstructionOwner::Driver(step) => {
                    let step = store.driver_steps.get(step as usize).ok_or_else(|| IrVerifyError::new("invocation driver is missing"))?;
                    let slots = step.slots.bounds(store.driver_slots.len()).ok_or_else(|| IrVerifyError::new("invocation driver slots are invalid"))?;
                    let binding = store.driver_slots[slots].iter().find(|binding| binding.slot == slot).ok_or_else(|| IrVerifyError::new("invocation callee binding is missing"))?;
                    if binding.flags & DRIVER_SLOT_MUTABLE != 0 { return Err(IrVerifyError::new("mutable callable binding has no prepared assignment proof")); }
                    Some(binding.type_id)
                }
                InstructionOwner::Function(owner) => {
                    if let Some(use_) = generic.original_callable_use(contract.callee_instruction) {
                        let binding = generic.original_callable_binding(use_.binding).ok_or_else(|| IrVerifyError::new("local callable lost its original binding"))?;
                        if use_.origin != contract.callee_origin || use_.owner != source.owner || binding.slot != slot || binding.contract != contract.callable {
                            return Err(IrVerifyError::new("local callable read changes its original binding or authority"));
                        }
                        None
                    } else {
                        let caller = &store.functions[owner.index()];
                        let captures = caller.captures.bounds(store.captures.len()).ok_or_else(|| IrVerifyError::new("invocation captures are invalid"))?;
                        let binding = store.captures[captures].iter().find(|capture| capture.slot_and_flags & !(1 << 31) == slot).ok_or_else(|| IrVerifyError::new("local callable binding has no prepared creation proof"))?;
                        if binding.slot_and_flags & (1 << 31) != 0 { return Err(IrVerifyError::new("mutable callable capture has no prepared assignment proof")); }
                        Some(binding.type_id)
                    }
                }
            } };
            if let Some(ty) = ty && store.semantic.callable_descriptor(ty)? != Some((contract.callable.kind, contract.callable.signature)) { return Err(IrVerifyError::new("invocation callee storage changes its original signature")); }
            let block = words.get(1).copied().and_then(IrBlockId::from_raw).and_then(|block| store.blocks.get(block.index())).ok_or_else(|| IrVerifyError::new("invocation argument block is missing"))?;
            let args = store.payload(block.instructions)?;
            let count = args.first().copied().ok_or_else(|| IrVerifyError::new("invocation argument count is missing"))? as usize;
            let parameters = function.params.bounds(store.params.len()).ok_or_else(|| IrVerifyError::new("invocation parameters are invalid"))?;
            let parameters = &store.params[parameters];
            if count > parameters.len() || args.len() != 1 + count * 2 { return Err(IrVerifyError::new("invocation encoded argument arity is invalid")); }
            let mut supplied = vec![None; parameters.len()];
            for (&slot, argument) in contract.binding.supplied_slots.iter().zip(&contract.arguments) { supplied[slot as usize] = Some(argument); }
            for (slot, parameter) in parameters.iter().enumerate() {
                let encoded = if slot < count { (args[1 + slot * 2], args[2 + slot * 2]) } else { (2, slot as u32) };
                match supplied[slot] {
                    Some(argument) => {
                        if encoded != (0, argument.instruction) { return Err(IrVerifyError::new("invocation changes its original supplied argument")); }
                        let TypeRef::Ground(ty) = argument.ty else { return Err(IrVerifyError::new("invocation argument is not grounded")); };
                        Self::verify_generic_source(store, generic, argument.instruction, source.owner, &store.semantic.to_type(ty)?, None, &mut Vec::new())?;
                    }
                    None => if encoded != (2, slot as u32) || parameter.flags & 2 == 0 { return Err(IrVerifyError::new("invocation changes its original default slot")); },
                }
            }
        }
        for (instruction, tag) in store.tags.iter().enumerate() {
            if generic.registered_instruction_origin(instruction as u32, false).is_none() { continue; }
            match tag {
                FullTag::ExprFunctionRef if generic.callable_value_at(instruction as u32)?.is_none() => return Err(IrVerifyError::new("original callable creation lacks its prepared value proof")),
                FullTag::ExprDynamicCall if generic.invocation_plan_at(instruction as u32)?.is_none() && generic.scoped_invocation_source_at(instruction as u32)?.is_none() && generic.native_invocation_plan_at(instruction as u32)?.is_none() => return Err(IrVerifyError::new("original invocation lacks its prepared plan")),
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;

    #[test]
    fn saved_user_arguments_reject_original_recipe_slot_wrapper_and_owner_changes_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let program = source_fixture("pure combine(left: Int, right: Int) -> Int { left + right }\nlet alias = combine\nlet result = alias(right: 4, left: 2)\n");
            let generic = program.store.generic.as_deref().unwrap();
            let saved = generic.original_argument_bindings().next().expect("actual saved original argument").clone();
            assert_eq!(generic.original_argument_bindings().count(), 2);
            let mut missing = program.clone();
            missing.store.generic.as_deref_mut().unwrap().test_remove_original_argument_bindings();
            assert!(FullVerifier::verify(&missing).is_err());
            let mut wrong_slot = program.clone();
            let raw = wrong_slot.store.data[saved.instruction as usize].range().start as usize;
            wrong_slot.store.extra[raw] += 1;
            assert!(FullVerifier::verify(&wrong_slot).is_err());
            let mut wrong_recipe = program.clone();
            wrong_recipe.store.generic.as_deref_mut().unwrap().test_original_argument_binding_mut(saved.instruction).unwrap().ordinal += 1;
            assert!(FullVerifier::verify(&wrong_recipe).is_err());
            let mut wrong_owner = program.clone();
            wrong_owner.store.generic.as_deref_mut().unwrap().test_original_argument_binding_mut(saved.instruction).unwrap().owner = InstructionOwner::Driver(u32::MAX);
            assert!(FullVerifier::verify(&wrong_owner).is_err());
            let mut wrong_wrapper = program.clone();
            let raw = wrong_wrapper.store.data[saved.wrapper as usize].range().start as usize;
            wrong_wrapper.store.extra[raw] = saved.instruction;
            assert!(FullVerifier::verify(&wrong_wrapper).is_err());
        });
    }

    #[test]
    fn saved_user_arguments_reject_coupled_authored_entry_rewrites_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let program = source_fixture("pure combine(left: Int, right: Int) -> Int { left + right }\nlet alias = combine\nlet result = alias(right: 4, left: 2)\n");
            let generic = program.store.generic.as_deref().unwrap();
            let saved = generic.original_argument_bindings().next().unwrap().clone();
            let (plan, proof) = generic.invocation_plans().find(|(_, proof)| proof.contract.arguments.iter().any(|argument| argument.instruction == saved.instruction)).unwrap();
            let source = proof.source;
            let ordinal = proof.contract.arguments.iter().position(|argument| argument.instruction == saved.instruction).unwrap();
            let mut rewritten = program.clone();
            let generic = rewritten.store.generic.as_deref_mut().unwrap();
            generic.test_original_argument_binding_mut(saved.instruction).unwrap().recipe.entry_index += 10;
            generic.test_invocation_source_mut(source).unwrap().expected.arguments[ordinal].original.entry_index += 10;
            generic.test_invocation_plan_mut(plan).unwrap().contract.arguments[ordinal].original.entry_index += 10;
            let error = FullVerifier::verify(&rewritten).unwrap_err();
            assert!(error.message.contains("original prepared receipt"), "saved arguments must retain their original authored entry even when every dependent call receipt is rewritten: {}", error.message);
        });
    }

    fn fixture() -> FullProgram {
        source_fixture("pure increment(value: Int) -> Int { value + 1 }\npure replacement(value: Int) -> Int { value + 2 }\nlet alias = increment\n")
    }

    #[test]
    fn local_callable_binding_rejects_same_signature_slot_and_authority_rewrites_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let program = source_fixture("pure increment(value: Int) -> Int { value + 1 }\npure replacement(value: Int) -> Int { value + 2 }\npure selected(value: Int) -> Int { let first = increment; let second = replacement; first(value) }\n");
            let generic = program.store.generic.as_deref().unwrap();
            let use_ = *generic.original_callable_uses().next().expect("actual local callable read");
            let original = generic.original_callable_binding(use_.binding).unwrap();
            let replacement = generic.original_callable_bindings().find(|binding| binding.contract.target != original.contract.target).unwrap();
            assert_eq!(original.contract.signature, replacement.contract.signature);
            let replacement_slot = replacement.slot;
            let replacement_contract = replacement.contract;
            let plan_id = generic.invocation_plans().find(|(_, proof)| proof.contract.callee_instruction == use_.instruction).unwrap().0;
            let source_id = generic.invocation_plan(plan_id).unwrap().source;

            let mut wrong_slot = program.clone();
            let raw = wrong_slot.store.data[use_.instruction as usize].range().start as usize;
            wrong_slot.store.extra[raw] = replacement_slot;
            assert!(FullVerifier::verify(&wrong_slot).is_err());

            let mut rewritten = wrong_slot;
            let generic = rewritten.store.generic.as_deref_mut().unwrap();
            generic.test_invocation_plan_mut(plan_id).unwrap().contract.callable = replacement_contract;
            generic.test_invocation_source_mut(source_id).unwrap().expected.callable = replacement_contract;
            assert!(FullVerifier::verify(&rewritten).is_err(), "the original binding table must reject a same-signature authority rewrite");

            let mut wrong_initializer = program.clone();
            let raw = wrong_initializer.store.data[original.instruction as usize].range().start as usize + 1;
            wrong_initializer.store.extra[raw] = replacement.initializer;
            assert!(FullVerifier::verify(&wrong_initializer).is_err());
        });
    }

    fn source_fixture(source: &str) -> FullProgram {
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            "callable-proof.xsh", crate::loader::entry_source_from_text("callable-proof.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = SourceMap::files(&sources).first().unwrap().id();
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let graph = Arc::downgrade(&bodies.solved);
        let counters = bodies.solved.graph.counters().clone();
        let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap();
        assert_eq!(bodies.solved.graph.counters(), &counters);
        drop(parsed); drop(declarations); drop(bodies);
        assert!(graph.upgrade().is_none());
        program
    }

    #[test]
    fn ground_user_callable_rejects_same_signature_encoded_target_after_frontend_drop() {
        std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
            let program = fixture();
            program.symbol_owner().with_current(|| {
                let generic = program.generic_evidence().unwrap();
                let (_, value) = generic.callable_values().next().unwrap();
                let source = generic.callable_source(value.source).unwrap();
                let replacement = generic.checked_functions().map(|(_, function)| function).find(|function|
                    Name::intern(program.store.string(program.store.functions[function.target.index()].name).unwrap()).as_str() == "replacement").unwrap();
                assert_eq!(replacement.signature, value.contract.signature);
                let mut changed = program.store.clone();
                let range = changed.data[source.instruction as usize].range().bounds(changed.extra.len()).unwrap();
                assert_eq!(changed.extra[range.start], 0);
                changed.extra[range.start + 1] = Name::intern("replacement").symbol().raw();
                assert!(FullVerifier::verify_generic_evidence(&changed).is_err(), "same-signature encoded target was accepted");
            });
        }).unwrap().join().unwrap();
    }

    #[test]
    fn ground_user_callable_rejects_missing_foreign_misplaced_and_rewritten_authority() {
        std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
            let program = fixture();
            program.symbol_owner().with_current(|| {
                let generic = program.generic_evidence().unwrap();
                let (id, proof) = generic.callable_values().next().unwrap();
                let source = generic.callable_source(proof.source).unwrap();
                let mut missing = program.store.clone();
                missing.generic.as_deref_mut().unwrap().test_remove_callable_values();
                assert!(FullVerifier::verify_generic_evidence(&missing).unwrap_err().message.contains("lacks its prepared value proof"));
                let mut builder = super::super::super::generic::GenericEvidenceBuilder::default();
                let foreign_source = builder.add_callable_source(source.clone()).unwrap();
                let mut foreign = program.store.clone();
                foreign.generic.as_deref_mut().unwrap().test_callable_value_mut(id).unwrap().source = foreign_source;
                assert!(FullVerifier::verify_generic_evidence(&foreign).unwrap_err().message.contains("foreign program"));
                let mut misplaced = program.store.clone();
                misplaced.generic.as_deref_mut().unwrap().test_callable_source_mut(proof.source).unwrap().instruction = 0;
                assert!(FullVerifier::verify_generic_evidence(&misplaced).is_err());
                let replacement = generic.checked_functions().map(|(_, function)| function).find(|function|
                    Name::intern(program.store.string(program.store.functions[function.target.index()].name).unwrap()).as_str() == "replacement").unwrap();
                let mut rewritten = program.store.clone();
                let evidence = rewritten.generic.as_deref_mut().unwrap();
                evidence.test_callable_source_mut(proof.source).unwrap().expected.target = replacement.target;
                evidence.test_callable_value_mut(id).unwrap().contract.target = replacement.target;
                let range = rewritten.data[source.instruction as usize].range().bounds(rewritten.extra.len()).unwrap();
                rewritten.extra[range.start + 1] = Name::intern("replacement").symbol().raw();
                assert!(FullVerifier::verify_generic_evidence(&rewritten).unwrap_err().message.contains("another checked declaration"));
                let mut wrong_kind = program.store.clone();
                let range = wrong_kind.data[source.instruction as usize].range().bounds(wrong_kind.extra.len()).unwrap();
                let end = range.end - 1;
                wrong_kind.extra[end] ^= 1;
                assert!(FullVerifier::verify_generic_evidence(&wrong_kind).is_err());
            });
        }).unwrap().join().unwrap();
    }

    #[test]
    fn ground_user_callable_checkpoint_retires_creation_handles() {
        std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
            let program = fixture();
            program.symbol_owner().with_current(|| {
                let generic = program.generic_evidence().unwrap();
                let (_, proof) = generic.callable_values().next().unwrap();
                let source = generic.callable_source(proof.source).unwrap();
                let mut builder = super::super::super::generic::GenericEvidenceBuilder::default();
                for (_, function) in generic.checked_functions() { builder.add_checked_function(*function).unwrap(); }
                builder.register_instruction_origin(source.instruction, super::super::super::generic::OperationSourceOrigin::Expression(source.origin), source.owner).unwrap();
                let checkpoint = builder.checkpoint();
                let old_source = builder.add_callable_source(source.clone()).unwrap();
                let old_value = builder.add_callable_value(PreparedCallableValue { source: old_source, contract: proof.contract }).unwrap();
                let retired = builder.checkpoint();
                builder.rewind(checkpoint).unwrap();
                let current_source = builder.add_callable_source(source.clone()).unwrap();
                let current = builder.add_callable_value(PreparedCallableValue { source: current_source, contract: proof.contract }).unwrap();
                assert!(builder.rewind(retired).is_err());
                let store = builder.finish(&program.store.semantic, program.store.functions.len(), &program.store.generic_instruction_owners().unwrap()).unwrap();
                assert!(store.callable_source(old_source).is_err());
                assert!(store.callable_value(old_value).is_err());
                assert!(store.callable_value(current).is_ok());
                assert!(store.callable_value(generic.callable_values().next().unwrap().0).is_err());
            });
        }).unwrap().join().unwrap();
    }

    #[test]
    fn ground_user_invocation_retains_original_supplied_default_and_creation_contract() {
        std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
            let program = source_fixture("pure increment(value: Int, offset: Int = 1) -> Int { value + offset }\nlet alias = increment\nlet result = alias(41)\n");
            program.symbol_owner().with_current(|| {
                let generic = program.generic_evidence().unwrap();
                let (value, _) = generic.callable_values().next().unwrap();
                let (id, proof) = generic.invocation_plans().next().unwrap();
                let source = generic.invocation_source(proof.source).unwrap();
                assert_eq!(&*proof.contract.binding.supplied_slots, &[0]);
                assert_eq!(&*proof.contract.binding.default_slots, &[1]);
                assert_eq!(proof.contract.timing, InvocationDefaultTiming::AtCall);
                assert!(generic.validate_user_invocation(value, id).is_ok());
                assert_eq!(program.callable_value(generic.callable_source(generic.callable_value(value).unwrap().source).unwrap().instruction).unwrap(), Some(value));
                assert_eq!(program.invocation_plan(source.instruction).unwrap(), Some(id));
                let mut missing = program.store.clone();
                missing.generic.as_deref_mut().unwrap().test_remove_invocation_plans();
                assert!(FullVerifier::verify_generic_evidence(&missing).unwrap_err().message.contains("lacks its prepared plan"));
                let words = program.store.payload(program.store.data[source.instruction as usize].range()).unwrap();
                let block_index = IrBlockId::from_raw(words[1]).unwrap().index();
                let block = program.store.blocks[block_index];
                let range = block.instructions.bounds(program.store.extra.len()).unwrap();
                assert_eq!(program.store.extra[range.start], 1);
                let mut wrong_default = program.store.clone();
                let start = wrong_default.extra.len() as u32;
                wrong_default.extra.extend([2, 0, proof.contract.arguments[0].instruction, 2, 0]);
                wrong_default.blocks[block_index].instructions = IrRange::new(start, 5);
                assert!(FullVerifier::verify_generic_evidence(&wrong_default).is_err());
                let mut wrong_callee = program.store.clone();
                let range = wrong_callee.data[source.instruction as usize].range().bounds(wrong_callee.extra.len()).unwrap();
                wrong_callee.extra[range.start] = proof.contract.arguments[0].instruction;
                assert!(FullVerifier::verify_generic_evidence(&wrong_callee).is_err());
                let mut wrong_argument = program.store.clone();
                wrong_argument.tags[proof.contract.arguments[0].instruction as usize] = FullTag::ExprBool;
                assert!(FullVerifier::verify_generic_evidence(&wrong_argument).is_err());
                let mut foreign_builder = super::super::super::generic::GenericEvidenceBuilder::default();
                let foreign_source = foreign_builder.add_invocation_source(source.clone()).unwrap();
                let mut foreign = program.store.clone();
                foreign.generic.as_deref_mut().unwrap().test_invocation_plan_mut(id).unwrap().source = foreign_source;
                assert!(FullVerifier::verify_generic_evidence(&foreign).is_err());
                let descriptor = program.store.driver_slots.iter().find_map(|slot| program.store.semantic.callable_descriptor(slot.type_id).unwrap().map(|_| slot.type_id)).unwrap();
                assert!(program.store.semantic.to_type(descriptor).is_err());
            });
        }).unwrap().join().unwrap();
    }
}
