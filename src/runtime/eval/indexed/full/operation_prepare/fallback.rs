use super::*;
use crate::sema::check::{ExpressionIdentity, SolvedTypes};
use crate::syntax::arena::ArenaExprKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct BuildFallbackCreationCheck {
    origin: ExpressionIdentity,
    right: ExpressionIdentity,
    requirement: ScopedRequirementRoot,
    checked: ScopedRoot,
    right_type: ScopedRoot,
    fallback: BuildExprId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedFallbackCreationCheck {
    pub instruction: u32,
    pub payload: Box<[u32]>,
    pub right: ExpressionIdentity,
    pub right_instruction: u32,
}

impl PreparedFallbackCreationCheck {
    pub(in crate::runtime::eval) fn retained_bytes(&self) -> usize { self.payload.len() * std::mem::size_of::<u32>() }
}

fn creation_contract(solved: &SolvedTypes, origin: ExpressionIdentity, right: ExpressionIdentity) -> Result<Option<(ScopedRequirementRoot, ScopedRoot, ScopedRoot)>, IrVerifyError> {
    let graph = &solved.graph;
    let original = solved.operations.get(&origin).ok_or_else(|| IrVerifyError::new("fallback creation lacks its original operation"))?;
    let Some(selected) = graph.candidate_evidence(original.requirement).map_err(|_| IrVerifyError::new("fallback creation changes its original requirement owner"))? else { return Ok(None); };
    let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).map_err(|_| IrVerifyError::new("fallback creation changes its selected candidate owner"))? else { return Ok(None); };
    let PreparedLanguageOperation::Fallback { result } = metadata.operation else { return Ok(None); };
    let output = graph_ground_type(graph, selected.result).map_err(|_| IrVerifyError::new("fallback creation lacks its original ground result"))?;
    if output != Type::UInt { return Ok(None); }
    if original.actual_arguments.len() != 2 || selected.actual_arguments.len() != 2 || original.receiver.is_some()
        || original.binding.supplied_slots != [0, 1] || !original.binding.default_slots.is_empty() || original.binding.rest_slot.is_some()
        || original.binding.dynamic.is_some() || !original.argument_coercions.is_empty() { return Err(IrVerifyError::new("fallback creation changes its original source binding")); }
    let actual_right = selected.actual_arguments[1].ok_or_else(|| IrVerifyError::new("fallback creation loses its original right operand"))?;
    if graph_ground_type(graph, actual_right).map_err(|_| IrVerifyError::new("fallback creation lacks its original right domain"))? != Type::Int { return Ok(None); }
    let carrier = graph_ground_type(graph, selected.actual_arguments[0].ok_or_else(|| IrVerifyError::new("fallback creation loses its original carrier"))?).map_err(|_| IrVerifyError::new("fallback creation lacks its original carrier domain"))?;
    if !matches!((&carrier, result), (Type::Optional(inner), false) | (Type::Result(inner, _), true) if **inner == Type::UInt) { return Err(IrVerifyError::new("fallback creation changes its original UInt carrier")); }
    let candidate = graph.candidate(selected.candidate).map_err(|_| IrVerifyError::new("fallback creation loses its original candidate"))?;
    let crate::sema::inference::TypeNode::Arrow(signature) = graph.node(graph.resolved(selected.signature).map_err(|_| IrVerifyError::new("fallback creation loses its original signature"))?).map_err(|_| IrVerifyError::new("fallback creation loses its original signature"))? else { return Err(IrVerifyError::new("fallback creation lacks its original callable signature")); };
    if candidate.argument_relations.as_slice() != [crate::sema::inference::ArgumentRelation::Exact, crate::sema::inference::ArgumentRelation::Assignable]
        || signature.params.len() != 2 || graph_ground_type(graph, signature.params[1].ty).map_err(|_| IrVerifyError::new("fallback creation loses its formal right domain"))? != Type::UInt {
        return Err(IrVerifyError::new("fallback creation changes its original Assignable UInt parameter"));
    }
    let caller = original.caller;
    let scope = solved.expression_scope(origin, caller).map_err(|_| IrVerifyError::new("fallback creation changes its original expression scope"))?;
    let requirement = ScopedRequirementRoot { requirement: original.requirement, scope };
    let checked = ScopedRoot { ty: *solved.expressions.get(&origin).ok_or_else(|| IrVerifyError::new("fallback creation loses its original result expression"))?, scope };
    let right_type = ScopedRoot { ty: *solved.expressions.get(&right).ok_or_else(|| IrVerifyError::new("fallback creation loses its original right expression"))?, scope: solved.expression_scope(right, caller).map_err(|_| IrVerifyError::new("fallback creation changes its right expression scope"))? };
    graph.validate_requirement_scoped(requirement).map_err(|_| IrVerifyError::new("fallback creation changes its original requirement scope"))?;
    for root in [checked, right_type] { graph.validate_scoped(root).map_err(|_| IrVerifyError::new("fallback creation changes its original type scope"))?; }
    let resolved = |ty| graph.resolved(ty).map_err(|_| IrVerifyError::new("fallback creation changes its original type owner"));
    if solved.expression_owners.get(&right).copied() != caller || right.source != origin.source || right.namespace != origin.namespace
        || resolved(checked.ty)? != resolved(selected.result)? || resolved(original.result)? != resolved(selected.result)?
        || resolved(right_type.ty)? != resolved(actual_right)? || resolved(original.actual_arguments[1])? != resolved(actual_right)? {
        return Err(IrVerifyError::new("fallback creation changes its original result or authored right source"));
    }
    Ok(Some((requirement, checked, right_type)))
}

impl BuildFallbackCreationCheck {
    pub(in crate::runtime::eval) fn from_original(program: &ArenaProgram, solved: &SolvedTypes, origin: ExpressionIdentity,
        fallback: BuildExprId, checked: &Type,
    ) -> Result<Option<Self>, IrVerifyError> {
        let ArenaExprKind::Binary { op: BinaryOp::ResultFallback, right, .. } = program.arena.expr(origin.expression).kind else { return Ok(None); };
        let right = ExpressionIdentity { expression: right, ..origin };
        let Some((requirement, original_checked, right_type)) = creation_contract(solved, origin, right)? else { return Ok(None); };
        if *checked != Type::UInt { return Err(IrVerifyError::new("fallback creation changes its original UInt validation")); }
        Ok(Some(Self { origin, right, requirement, checked: original_checked, right_type, fallback }))
    }
}

impl FullBuilder {
    pub(in crate::runtime::eval::indexed::full) fn stage_fallback_creation_check(&mut self, expression: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(original) = scratch.fallback_creation_checks.get(&expression) else { return Ok(()); };
        let BuildExprRow::CheckedValue { value, check, .. } = scratch.expressions.get(expression.index()).ok_or_else(|| unprepared("fallback_creation_material_missing"))? else { return Err(unprepared("fallback_creation_material_opcode")); };
        let raw = *self.active_encoded_expressions.get(&original.fallback).ok_or_else(|| unprepared("fallback_creation_original_material_missing"))?;
        if *value != original.fallback || check.ty != Type::UInt { return Err(unprepared("fallback_creation_original_material_changed")); }
        let owner = self.current_owner.ok_or_else(|| unprepared("fallback_creation_owner_missing"))?;
        let owner = if let Some(driver) = driver_owner_index(owner) { InstructionOwner::Driver(driver as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(owner).ok_or_else(|| unprepared("fallback_creation_owner_invalid"))?) };
        self.fallback_creation_check_rows.push((original.clone(), raw, instruction, owner));
        Ok(())
    }

    pub(in crate::runtime::eval::indexed::full) fn prepare_fallback_lowering(&self, instruction: u32, origin: ExpressionIdentity, owner: InstructionOwner, result: bool) -> Result<PreparedFallbackLowering, IrBuildError> {
        let mut lowering = fallback_lowering(&self.store, instruction, result).map_err(|_| unprepared("fallback_lowering_not_prepared"))?;
        let Some(solved) = self.solved.as_ref() else { return Ok(lowering); };
        let mut found = self.fallback_creation_check_rows.iter().filter(|(_, raw, _, _)| *raw == instruction);
        let Some((original, raw, wrapper, checked_owner)) = found.next() else { return Ok(lowering); };
        if found.next().is_some() || *checked_owner != owner || original.origin != origin { return Err(unprepared("fallback_creation_original_owner_changed")); }
        let Some((requirement, checked, right_type)) = creation_contract(solved, origin, original.right).map_err(|error| IrBuildError::verification("fallback_creation_original_contract", error))? else { return Err(unprepared("fallback_creation_original_contract_missing")); };
        if requirement != original.requirement || checked != original.checked || right_type != original.right_type { return Err(unprepared("fallback_creation_original_roots_changed")); }
        if self.prepared_argument_origins.get(raw) != Some(&(origin, owner)) { return Err(unprepared("fallback_creation_original_source_changed")); }
        let operands = fallback_operands(&self.store, instruction, result).map_err(|_| unprepared("fallback_creation_original_operands_changed"))?;
        let right_instruction = operands[1];
        if self.prepared_argument_origins.get(&right_instruction) != Some(&(original.right, owner)) { return Err(unprepared("fallback_creation_original_right_changed")); }
        let words = self.store.payload(self.store.data[*wrapper as usize].range()).map_err(|_| unprepared("fallback_creation_original_guard_payload"))?;
        if self.store.tags.get(*wrapper as usize) != Some(&FullTag::ExprCheckedValue) || words.len() < 2 || words[0] != *raw
            || words.get(1).and_then(|&ty| TypeId::from_raw(ty)).and_then(|ty| self.store.semantic.to_type(ty).ok()) != Some(Type::UInt) {
            return Err(unprepared("fallback_creation_original_guard_changed"));
        }
        let guard = PreparedFallbackCreationCheck { instruction: *wrapper, payload: words.into(), right: original.right, right_instruction };
        match &mut lowering { PreparedFallbackLowering::Result { creation_check, .. } | PreparedFallbackLowering::Optional { creation_check, .. } => *creation_check = Some(guard) }
        Ok(lowering)
    }
}


pub(super) fn fallback_operands(store: &FullStore, instruction: u32, result: bool) -> Result<[u32; 2], IrVerifyError> {
    let words = store.payload(store.data.get(instruction as usize).ok_or_else(|| IrVerifyError::new("fallback instruction is missing"))?.range())?;
    if result {
        if store.tags.get(instruction as usize) != Some(&FullTag::ExprResultFallback) || words.len() != 2 {
            return Err(IrVerifyError::new("Result fallback changes its original lazy instruction"));
        }
        return Ok([words[0], words[1]]);
    }
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprMatch) || words.len() != 3 {
        return Err(IrVerifyError::new("Optional fallback changes its original lazy selection"));
    }
    let block = IrBlockId::from_raw(words[1]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("Optional fallback arms are missing"))?;
    let arms = store.payload(block.instructions)?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST || arms.len() != 7 || arms[0] != 2 || arms[2] != 0 || arms[5] != 0 {
        return Err(IrVerifyError::new("Optional fallback changes its lazy branch order"));
    }
    let null = arms[1] as usize;
    let present = arms[4] as usize;
    let literal = store.payload(store.pattern_data.get(null).ok_or_else(|| IrVerifyError::new("Optional fallback null pattern is missing"))?.range())?;
    let slot = store.payload(store.pattern_data.get(present).ok_or_else(|| IrVerifyError::new("Optional fallback binding pattern is missing"))?.range())?;
    if store.patterns.get(null) != Some(&FullPatternTag::Literal) || literal.len() != 1
        || store.values.get(literal[0] as usize) != Some(&FullValueTag::Null)
        || !store.payload(store.value_data.get(literal[0] as usize).ok_or_else(|| IrVerifyError::new("Optional fallback null value is missing"))?.range())?.is_empty()
        || store.patterns.get(present) != Some(&FullPatternTag::Bind) || slot.len() != 1
        || store.tags.get(arms[6] as usize) != Some(&FullTag::ExprParam)
        || store.payload(store.data.get(arms[6] as usize).ok_or_else(|| IrVerifyError::new("Optional fallback present read is missing"))?.range())? != slot {
        return Err(IrVerifyError::new("Optional fallback changes its original null or present branch"));
    }
    Ok([words[0], arms[3]])
}

// A matching bind/read pair can still overwrite another live slot. Retain the
// original physical selection independently of its operand and result types.
pub(super) fn fallback_lowering(store: &FullStore, instruction: u32, result: bool) -> Result<PreparedFallbackLowering, IrVerifyError> {
    fallback_operands(store, instruction, result)?;
    let words = store.payload(store.data[instruction as usize].range())?;
    let instruction_payload = words.to_vec().into_boxed_slice();
    if result { return Ok(PreparedFallbackLowering::Result { instruction_payload, creation_check: None }); }
    let block = &store.blocks[IrBlockId::from_raw(words[1]).ok_or_else(|| IrVerifyError::new("Optional fallback arms are missing"))?.index()];
    let arms = store.payload(block.instructions)?;
    Ok(PreparedFallbackLowering::Optional {
        instruction_payload, arms_flags: block.flags, arms_payload: arms.to_vec().into_boxed_slice(),
        null_pattern_payload: store.payload(store.pattern_data[arms[1] as usize].range())?.to_vec().into_boxed_slice(),
        present_pattern_payload: store.payload(store.pattern_data[arms[4] as usize].range())?.to_vec().into_boxed_slice(),
        present_payload: store.payload(store.data[arms[6] as usize].range())?.to_vec().into_boxed_slice(),
        creation_check: None,
    })
}

pub(super) fn creation_check(lowering: &PreparedFallbackLowering) -> Option<&PreparedFallbackCreationCheck> {
    match lowering { PreparedFallbackLowering::Result { creation_check, .. } | PreparedFallbackLowering::Optional { creation_check, .. } => creation_check.as_ref() }
}

fn same_selection(left: &PreparedFallbackLowering, right: &PreparedFallbackLowering) -> bool {
    match (left, right) {
        (PreparedFallbackLowering::Result { instruction_payload: left, .. }, PreparedFallbackLowering::Result { instruction_payload: right, .. }) => left == right,
        (PreparedFallbackLowering::Optional { instruction_payload: li, arms_flags: lf, arms_payload: la, null_pattern_payload: ln, present_pattern_payload: lp, present_payload: lv, .. },
            PreparedFallbackLowering::Optional { instruction_payload: ri, arms_flags: rf, arms_payload: ra, null_pattern_payload: rn, present_pattern_payload: rp, present_payload: rv, .. }) =>
            li == ri && lf == rf && la == ra && ln == rn && lp == rp && lv == rv,
        _ => false,
    }
}

impl FullVerifier {
    pub(in crate::runtime::eval::indexed) fn verify_prepared_fallback_contract(pools: &SemanticPools, operation: &PreparedOperation) -> Result<(), IrVerifyError> {
        let PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Fallback { result }, argument_order: crate::sema::operation_graph::OperationArgumentOrder::SourceOrder, statement_result_is_unit: false, .. } = operation.authority else {
            return Err(IrVerifyError::new("fallback lacks its selected language authority"));
        };
        if operation.receiver.is_some() || operation.arguments.len() != 2 || operation.binding.supplied_slots.as_ref() != [0, 1]
            || !operation.binding.default_slots.is_empty() || operation.binding.operands.len() != 2
            || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
            || operation.effects.creation != crate::sema::inference::EffectSet::EMPTY
            || !operation.effects.inputs.is_empty() || !operation.effects.outputs.is_empty() {
            return Err(IrVerifyError::new("fallback changes its original operand, default or effect contract"));
        }
        if !matches!((&operation.fallback_lowering, result), (Some(PreparedFallbackLowering::Result { .. }), true) | (Some(PreparedFallbackLowering::Optional { .. }), false)) {
            return Err(IrVerifyError::new("fallback lacks its original physical selection receipt"));
        }
        let ground = |reference| match reference { TypeRef::Ground(ty) => pools.to_type(ty), _ => Err(IrVerifyError::new("fallback lacks a ground type proof")) };
        let carrier = ground(operation.arguments[0].ok_or_else(|| IrVerifyError::new("fallback carrier proof is missing"))?)?;
        let output = ground(operation.result)?;
        let success = match (&carrier, result) { (Type::Result(success, _), true) | (Type::Optional(success), false) => success.as_ref(), _ => return Err(IrVerifyError::new("fallback changes its selected carrier kind")) };
        if success != &output { return Err(IrVerifyError::new("fallback result differs from its original carrier success type")); }
        let right = ground(operation.arguments[1].ok_or_else(|| IrVerifyError::new("fallback right operand proof is missing"))?)?;
        let creation_check = creation_check(operation.fallback_lowering.as_ref().unwrap());
        if matches!((&right, &output), (Type::Int, Type::UInt)) {
            let guard = creation_check.ok_or_else(|| IrVerifyError::new("fallback right operand differs from its checked result domain without its original UInt creation guard"))?;
            if guard.right_instruction != operation.binding.operands[1]
                || guard.payload.get(1).and_then(|&ty| TypeId::from_raw(ty)).map(|ty| pools.to_type(ty)).transpose()? != Some(Type::UInt) {
                return Err(IrVerifyError::new("fallback creation guard changes its original result or right operand"));
            }
        } else if !right.matches_expected(&output) || creation_check.is_some() {
            return Err(IrVerifyError::new("fallback right operand differs from its checked result domain"));
        }
        Ok(())
    }

    pub(in crate::runtime::eval::indexed::full) fn verify_fallback_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(operation) = generic.operation_at(instruction)? else { return Ok(false); };
        let PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Fallback { result }, .. } = operation.authority else { return Ok(false); };
        Self::verify_prepared_fallback_contract(&store.semantic, operation)?;
        let source = generic.operation_source(operation.source)?;
        if source.instruction != instruction || source.owner != owner || operation.authority != source.expected
            || generic.registered_instruction_origin(instruction, false) != Some((source.origin, owner))
            || fallback_operands(store, instruction, result)?.as_slice() != operation.binding.operands.as_ref()
            || !same_selection(operation.fallback_lowering.as_ref().unwrap(), &fallback_lowering(store, instruction, result)?) {
            return Err(IrVerifyError::new("fallback changes its original source, owner or lazy operands"));
        }
        if !result {
            let OperationSourceOrigin::Expression(origin) = source.origin else { return Err(IrVerifyError::new("fallback source has another original identity kind")); };
            let words = store.payload(store.data[instruction as usize].range())?;
            if words.get(2).and_then(|&location| IrLocationId::from_raw(location)).and_then(|location| store.location_sources.get(location.index())) != Some(&origin.source) {
                return Err(IrVerifyError::new("Optional fallback changes its original encoded source location"));
            }
        }
        if let Some(guard) = creation_check(operation.fallback_lowering.as_ref().unwrap()) {
            let range = match owner { InstructionOwner::Function(function) => store.function_instruction_range(function.index())?, InstructionOwner::Driver(driver) => store.driver_instruction_range(driver as usize)? };
            if !range.contains(&(guard.instruction as usize)) || store.tags.get(guard.instruction as usize) != Some(&FullTag::ExprCheckedValue)
                || store.payload(store.data.get(guard.instruction as usize).ok_or_else(|| IrVerifyError::new("fallback creation guard is missing"))?.range())? != guard.payload.as_ref()
                || guard.payload.first() != Some(&instruction) || guard.right_instruction != operation.binding.operands[1]
                || generic.registered_instruction_origin(guard.right_instruction, false) != Some((OperationSourceOrigin::Expression(guard.right), owner)) {
                return Err(IrVerifyError::new("fallback loses its original UInt creation guard or authored right source"));
            }
        }
        let TypeRef::Ground(output) = operation.result else { return Err(IrVerifyError::new("fallback result is not ground")); };
        if store.semantic.to_type(output)? != *expected { return Err(IrVerifyError::new("fallback operand changes its checked result type")); }
        for (&operand, reference) in operation.binding.operands.iter().zip(operation.arguments.iter()) {
            let Some(TypeRef::Ground(ty)) = reference else { return Err(IrVerifyError::new("fallback operand is not ground")); };
            Self::verify_generic_source(store, generic, operand, owner, &store.semantic.to_type(*ty)?, instance, active)?;
        }
        Ok(true)
    }

}

impl<'a> FullExecution<'a> {
    pub(in crate::runtime::eval) fn prepared_fallback(&self, instruction: u32) -> Result<Option<(LoweredTypeCheck, Span)>, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("fallback belongs to another execution body")); }
        self.decoder.store.verify_generic_owner()?;
        let Some(generic) = self.generic_evidence() else { return Ok(None); };
        let Some(operation) = generic.operation_at(instruction)? else { return Ok(None); };
        if !matches!(operation.authority, PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Fallback { .. }, .. }) { return Ok(None); }
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) } else {
            InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("fallback execution owner is invalid"))?)
        };
        let TypeRef::Ground(result) = operation.result else { return Err(IrVerifyError::new("fallback execution lacks its original ground result")); };
        let expected = self.decoder.store.semantic.to_type(result)?;
        if !FullVerifier::verify_fallback_operand(self.decoder.store, generic, instruction, owner, &expected, self.instantiation, &mut vec![instruction])? {
            return Err(IrVerifyError::new("fallback execution loses its original authority"));
        }
        let Some(guard) = creation_check(operation.fallback_lowering.as_ref().unwrap()) else { return Ok(None); };
        let mut payload = FullCursor::new(&guard.payload);
        payload.raw()?;
        let check = LoweredTypeCheck::decode(&self.decoder, &mut payload)?;
        let span = Span::decode(&self.decoder, &mut payload)?;
        payload.finish()?;
        Ok(Some((check, span)))
    }
}
