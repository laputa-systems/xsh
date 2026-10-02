use super::*;
use super::super::super::generic::OriginalOptionalReceiverGuard;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildOptionalReceiverGuard {
    pub call: crate::sema::check::ExpressionIdentity,
    pub origin: crate::sema::check::ExpressionIdentity,
    pub source_type: crate::sema::inference::ScopedRoot,
    pub success_type: crate::sema::inference::ScopedRoot,
    pub call_source_type: crate::sema::inference::ScopedRoot,
    pub call_result_type: crate::sema::inference::ScopedRoot,
    pub carrier: BuildExprId,
    pub read: BuildExprId,
    pub slot: usize,
    pub wrapper: Option<BuildExprId>,
}

fn optional_guard_words(store: &FullStore, wrapper: u32) -> Result<(&[u32], &[u32], u8), IrVerifyError> {
    if store.tags.get(wrapper as usize) != Some(&FullTag::ExprMatch) { return Err(IrVerifyError::new("optional receiver changes its guard instruction")); }
    let words = store.payload(store.data.get(wrapper as usize).ok_or_else(|| IrVerifyError::new("optional receiver guard is missing"))?.range())?;
    if words.len() != 3 { return Err(IrVerifyError::new("optional receiver changes its guard payload")); }
    let block = IrBlockId::from_raw(words[1]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("optional receiver guard arms are missing"))?;
    let arms = store.payload(block.instructions)?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST || arms.len() != 7 || arms[0] != 2 || arms[2] != 0 || arms[5] != 0 {
        return Err(IrVerifyError::new("optional receiver changes its lazy null and present arms"));
    }
    Ok((words, arms, block.flags))
}

impl FullBuilder {
    pub(in crate::runtime::eval::indexed::full) fn optional_receiver_guard(&self, read: u32) -> Option<&OriginalOptionalReceiverGuard> { self.optional_receiver_guards.get(&read) }

    pub(in crate::runtime::eval::indexed::full) fn stage_optional_receiver_guard(&mut self, expression: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(original) = scratch.optional_receiver_guards.values().find(|guard| guard.wrapper == Some(expression)).cloned() else { return Ok(()); };
        let solved = self.solved.as_ref().cloned().ok_or_else(|| problem("optional_receiver_original_graph_missing"))?;
        let encoded = |expression| self.active_encoded_expressions.get(&expression).copied().ok_or_else(|| problem("optional_receiver_original_encoding_missing"));
        let carrier = encoded(original.carrier)?;
        let read = encoded(original.read)?;
        let slot = u32::try_from(original.slot).map_err(|_| problem("optional_receiver_slot_overflow"))?;
        let (words, arms, _) = optional_guard_words(&self.store, instruction).map_err(|cause| IrBuildError::verification("optional_receiver_guard_encoding_changed", cause))?;
        if words[0] != carrier { return Err(problem("optional_receiver_guard_carrier_changed")); }
        let null_pattern = arms[1];
        let absent = arms[3];
        let body = arms[6];
        let null_pattern_payload = self.store.payload(self.store.pattern_data[null_pattern as usize].range()).map_err(|_| problem("optional_receiver_null_pattern_missing"))?.to_vec().into_boxed_slice();
        let absent_payload = self.store.payload(self.store.data[absent as usize].range()).map_err(|_| problem("optional_receiver_absent_missing"))?.to_vec().into_boxed_slice();
        let read_payload = self.store.payload(self.store.data[read as usize].range()).map_err(|_| problem("optional_receiver_read_missing"))?.to_vec().into_boxed_slice();
        let raw = self.current_owner.ok_or_else(|| problem("optional_receiver_owner_missing"))?;
        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| problem("optional_receiver_owner_invalid"))?) };
        let ground = |builder: &mut FullBuilder, root: crate::sema::inference::ScopedRoot| {
            solved.graph.validate_scoped(root).map_err(|_| problem("optional_receiver_original_scope_changed"))?;
            let ty = super::super::super::generic::graph_ground_type(&solved.graph, root.ty).map_err(|_| problem("optional_receiver_original_type_not_ground"))?;
            builder.intern_generic_ground_type(&ty).map(TypeRef::Ground)
        };
        let guard = OriginalOptionalReceiverGuard {
            call: original.call, origin: original.origin,
            source_type: ground(self, original.source_type)?, success_type: ground(self, original.success_type)?,
            call_source_type: ground(self, original.call_source_type)?, call_result_type: ground(self, original.call_result_type)?,
            carrier, read, owner, wrapper: instruction, body, slot, null_pattern, absent,
            null_pattern_payload, absent_payload, read_payload,
        };
        FullVerifier::verify_optional_receiver_guard_physical(&self.store, &guard, false).map_err(|cause| IrBuildError::verification("optional_receiver_guard_encoding_changed", cause))?;
        if self.optional_receiver_guards.insert(read, guard).is_some() { return Err(problem("optional_receiver_guard_duplicate")); }
        Ok(())
    }

    pub(in crate::runtime::eval::indexed::full) fn prepare_optional_receiver_guards(&mut self) -> Result<(), IrBuildError> {
        let mut guards = self.optional_receiver_guards.values().cloned().collect::<Vec<_>>();
        guards.sort_unstable_by_key(|guard| guard.wrapper);
        for guard in guards {
            FullVerifier::verify_optional_receiver_guard_physical(&self.store, &guard, true).map_err(|cause| IrBuildError::verification("optional_receiver_guard_changed", cause))?;
            let (words, arms, arms_flags) = optional_guard_words(&self.store, guard.wrapper).map_err(|cause| IrBuildError::verification("optional_receiver_guard_changed", cause))?;
            let pattern = arms[4];
            let receipt = OriginalCompilerArgumentWrapper {
                instruction: guard.wrapper, initializer: guard.carrier, pattern, body: guard.body, slot: guard.slot, owner: guard.owner,
                payload: words.to_vec().into_boxed_slice(), arms_flags, arms_payload: arms.to_vec().into_boxed_slice(),
                pattern_payload: self.store.payload(self.store.pattern_data[pattern as usize].range()).map_err(|_| problem("optional_receiver_present_pattern_missing"))?.to_vec().into_boxed_slice(),
                optional_receiver_guard: Some(guard),
            };
            self.generic_evidence_mut().add_original_compiler_argument_wrapper(receipt).map_err(|_| problem("optional_receiver_guard_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    fn verify_optional_receiver_guard_physical(store: &FullStore, guard: &OriginalOptionalReceiverGuard, verify_owner: bool) -> Result<(), IrVerifyError> {
        let (words, arms, _) = optional_guard_words(store, guard.wrapper)?;
        let owner_range = if verify_owner { Some(match guard.owner {
            InstructionOwner::Function(function) => store.function_instruction_range(function.index()),
            InstructionOwner::Driver(driver) => store.driver_instruction_range(driver as usize),
        }?) } else { None };
        if words[0] != guard.carrier || arms[1] != guard.null_pattern || arms[3] != guard.absent || arms[6] != guard.body {
            return Err(IrVerifyError::new("optional receiver changes its original carrier or branch bodies"));
        }
        if IrLocationId::from_raw(words[2]).and_then(|location| store.location_sources.get(location.index())) != Some(&guard.call.source) {
            return Err(IrVerifyError::new("optional receiver changes its original guard location source"));
        }
        if [guard.carrier, guard.read, guard.wrapper, guard.body, guard.absent].into_iter().any(|instruction| owner_range.as_ref().is_some_and(|range| !range.contains(&(instruction as usize)))) {
            return Err(IrVerifyError::new("optional receiver guard crosses its original owner range"));
        }
        if store.tags.get(guard.read as usize) != Some(&FullTag::ExprParam)
            || store.payload(store.data[guard.read as usize].range())? != guard.read_payload.as_ref() || guard.read_payload.as_ref() != [guard.slot] {
            return Err(IrVerifyError::new("optional receiver changes its original guarded read slot"));
        }
        if store.patterns.get(arms[4] as usize) != Some(&FullPatternTag::Bind)
            || store.payload(store.pattern_data[arms[4] as usize].range())? != [guard.slot] {
            return Err(IrVerifyError::new("optional receiver changes its original present binding slot"));
        }
        if store.patterns.get(guard.null_pattern as usize) != Some(&FullPatternTag::Literal)
            || store.payload(store.pattern_data[guard.null_pattern as usize].range())? != guard.null_pattern_payload.as_ref()
            || guard.null_pattern_payload.len() != 1
            || store.values.get(guard.null_pattern_payload[0] as usize) != Some(&FullValueTag::Null)
            || !store.payload(store.value_data[guard.null_pattern_payload[0] as usize].range())?.is_empty() {
            return Err(IrVerifyError::new("optional receiver changes its original null pattern"));
        }
        if store.tags.get(guard.absent as usize) != Some(&FullTag::ExprNull)
            || store.payload(store.data[guard.absent as usize].range())? != guard.absent_payload.as_ref() || !guard.absent_payload.is_empty() {
            return Err(IrVerifyError::new("optional receiver changes its original absent Null value"));
        }
        let ground = |reference| match reference { TypeRef::Ground(ty) => store.semantic.to_type(ty), _ => Err(IrVerifyError::new("optional receiver guard is not ground")) };
        if ground(guard.source_type)? != Type::Optional(Box::new(ground(guard.success_type)?))
            || ground(guard.call_source_type)? != Type::Optional(Box::new(ground(guard.call_result_type)?)) {
            return Err(IrVerifyError::new("optional receiver changes its original carrier or result narrowing"));
        }
        Ok(())
    }

    pub(in crate::runtime::eval::indexed::full) fn verify_optional_receiver_guard<'a>(store: &FullStore, generic: &'a GenericEvidenceStore, read: u32, owner: InstructionOwner) -> Result<Option<&'a OriginalOptionalReceiverGuard>, IrVerifyError> {
        let Some(guard) = generic.original_optional_receiver_guard(read)? else { return Ok(None); };
        let receipt = generic.optional_receiver_wrapper(read)?.ok_or_else(|| IrVerifyError::new("optional receiver guard loses its compiler receipt"))?;
        Self::verify_optional_receiver_guard_physical(store, guard, true)?;
        let (words, arms, flags) = optional_guard_words(store, guard.wrapper)?;
        if guard.owner != owner || words != receipt.payload.as_ref() || arms != receipt.arms_payload.as_ref() || flags != receipt.arms_flags
            || store.payload(store.pattern_data[receipt.pattern as usize].range())? != receipt.pattern_payload.as_ref()
            || generic.registered_instruction_origin(guard.wrapper, false) != Some((OperationSourceOrigin::Expression(guard.call), owner))
            || generic.registered_instruction_origin(guard.read, false).is_some() {
            return Err(IrVerifyError::new("optional receiver loses its original physical guard or source authority"));
        }
        Ok(Some(guard))
    }

    pub(in crate::runtime::eval::indexed::full) fn verify_optional_receiver_guard_result(store: &FullStore, generic: &GenericEvidenceStore,
        instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>,
    ) -> Result<bool, IrVerifyError> {
        let Some(guard) = generic.original_optional_receiver_guard_at(instruction)? else { return Ok(false); };
        Self::verify_optional_receiver_guard(store, generic, guard.read, owner)?.ok_or_else(|| IrVerifyError::new("optional call result loses its original receiver guard"))?;
        let TypeRef::Ground(result) = guard.call_source_type else { return Err(IrVerifyError::new("optional call result is not ground")); };
        let TypeRef::Ground(present) = guard.call_result_type else { return Err(IrVerifyError::new("optional present call result is not ground")); };
        let TypeRef::Ground(carrier) = guard.source_type else { return Err(IrVerifyError::new("optional call carrier is not ground")); };
        if store.semantic.to_type(result)? != *expected { return Err(IrVerifyError::new("optional call result changes its original lifted descriptor")); }
        Self::verify_generic_source(store, generic, guard.carrier, owner, &store.semantic.to_type(carrier)?, instance, active)?;
        Self::verify_generic_source(store, generic, guard.body, owner, &store.semantic.to_type(present)?, instance, active)?;
        Ok(true)
    }

    pub(in crate::runtime::eval::indexed::full) fn verify_optional_receiver_guard_scopes(store: &FullStore, tree: &super::super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref() else { return Ok(()); };
        for guard in generic.original_optional_receiver_guards() {
            Self::verify_optional_receiver_guard(store, generic, guard.read, guard.owner)?;
            if !tree.is_descendant(guard.body, guard.read)? || tree.is_descendant(guard.body, guard.carrier)? {
                return Err(IrVerifyError::new("optional receiver read escapes its original present arm"));
            }
        }
        Ok(())
    }
}
