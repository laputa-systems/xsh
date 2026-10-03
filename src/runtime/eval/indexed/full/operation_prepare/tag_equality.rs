use super::*;
use crate::sema::check::{ExpressionIdentity, QualifiedNominalIdentity, NominalDeclaration, NominalMemberKind, SolvedNominalMember};
use crate::sema::inference::EffectSet;
use crate::sema::operation_graph::OperationArgumentOrder;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildTagEqualitySource {
    origin: ExpressionIdentity,
    operands: [ExpressionIdentity; 2],
    rows: [BuildExprId; 2],
    op: BinaryOp,
    span: Span,
    operand_spans: [Span; 2],
    family: QualifiedNominalIdentity,
    family_name: Name,
    members: Box<[(QualifiedNominalIdentity, Arc<SolvedNominalMember>)]>,
    requirement: ScopedRequirementRoot,
    selected_roots: [ScopedRoot; 2],
    source_roots: [ScopedRoot; 2],
    result_root: ScopedRoot,
    selected_result_root: ScopedRoot,
    instruction: Option<u32>,
    instructions: Option<[u32; 2]>,
    owner: Option<InstructionOwner>,
}

impl BuildTagEqualitySource {
    pub(in crate::runtime::eval) fn from_authored(arena: &crate::syntax::arena::ArenaProgram, solved: &crate::sema::check::SolvedTypes,
        origin: ExpressionIdentity, left: BuildExprId, right: BuildExprId) -> Result<Option<Self>, IrBuildError> {
        let crate::syntax::arena::ArenaExprKind::Binary { op: op @ (BinaryOp::Eq | BinaryOp::Ne), left: authored_left, right: authored_right } = arena.arena.expr(origin.expression).kind else { return Ok(None); };
        let Some(operation) = solved.operations.get(&origin) else { return Ok(None); };
        let graph = &solved.graph;
        let Some(selected) = graph.candidate_evidence(operation.requirement).map_err(|_| unprepared("tag_equality_original_candidate"))? else { return Ok(None); };
        let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(graph, selected.candidate)
            .map_err(|_| unprepared("tag_equality_original_authority"))? else { return Ok(None); };
        if metadata.operation != (PreparedLanguageOperation::Equality { op }) { return Ok(None); }
        let scope = solved.operation_scope(crate::sema::check::ProducerFlowSource::Expression(origin), operation).map_err(|_| unprepared("tag_equality_original_scope"))?;
        let requirement = ScopedRequirementRoot { requirement: operation.requirement, scope };
        graph.validate_requirement_scoped(requirement).map_err(|_| unprepared("tag_equality_original_requirement"))?;
        let [Some(first), Some(second)] = selected.actual_arguments.as_slice() else { return Ok(None); };
        let selected_roots = [ScopedRoot { ty: *first, scope }, ScopedRoot { ty: *second, scope }];
        let [Type::Tag(first_name), Type::Tag(second_name)] = selected_roots.map(|root| graph_ground_type(graph, root.ty).unwrap_or(Type::Invalid)) else { return Ok(None); };
        if first_name != second_name { return Ok(None); }
        let family_for = |root: ScopedRoot| -> Result<QualifiedNominalIdentity, IrBuildError> {
            graph.validate_scoped(root).map_err(|_| unprepared("tag_equality_original_root"))?;
            solved.nominals.get(&graph.resolved(root.ty).map_err(|_| unprepared("tag_equality_original_nominal_root"))?).copied()
                .ok_or_else(|| unprepared("tag_equality_original_nominal_family"))
        };
        let family = family_for(selected_roots[0])?;
        if !matches!(family, QualifiedNominalIdentity::Source { declaration: NominalDeclaration::Type(_), member: None, .. })
            || family_for(selected_roots[1])? != family { return Err(unprepared("tag_equality_original_nominal_family")); }
        let members = solved.checked_nominal_family_members(family).map_err(|_| unprepared("tag_equality_original_family_members"))?;
        if members.iter().any(|(_, member)| member.kind != NominalMemberKind::Tag || member.family != first_name
            || !member.fields.is_empty() || !member.facets.is_empty() || member.scope.is_some()) { return Ok(None); }
        for &(identity, ref member) in &members {
            if nominal_family(identity) != Some(family) || !matches!(identity, QualifiedNominalIdentity::Source { member: Some(name), .. } if name == member.member)
                || family_for(ScopedRoot { ty: member.tested, scope: member.scope })? != family {
                return Err(unprepared("tag_equality_original_family_member"));
            }
        }
        let operands = [authored_left, authored_right].map(|expression| ExpressionIdentity {
            source: arena.arena.expr(expression).span.source_id, namespace: origin.namespace, expression });
        let source_root = |expression| -> Result<ScopedRoot, IrBuildError> {
            if solved.expression_owners.get(&expression).copied() != operation.caller { return Err(unprepared("tag_equality_original_operand_owner")); }
            let root = ScopedRoot { ty: *solved.expressions.get(&expression).ok_or_else(|| unprepared("tag_equality_original_expression"))?,
                scope: solved.expression_scope(expression, operation.caller).map_err(|_| unprepared("tag_equality_original_expression_scope"))? };
            graph.validate_scoped(root).map_err(|_| unprepared("tag_equality_original_expression_root"))?;
            Ok(root)
        };
        let source_roots = [source_root(operands[0])?, source_root(operands[1])?];
        let result_root = source_root(origin)?;
        let selected_result_root = ScopedRoot { ty: selected.result, scope };
        graph.validate_scoped(selected_result_root).map_err(|_| unprepared("tag_equality_original_result_root"))?;
        let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation.requirement).map_err(|_| unprepared("tag_equality_original_call"))? else { return Err(unprepared("tag_equality_original_call")); };
        let call = graph.operation_call(call).map_err(|_| unprepared("tag_equality_original_call"))?;
        if operation.receiver.is_some() || call.receiver.is_some() || operation.actual_arguments.as_slice() != [*first, *second]
            || call.arguments.as_slice() != [Some(*first), Some(*second)] || call.result != operation.result
            || graph_ground_type(graph, result_root.ty).map_err(|_| unprepared("tag_equality_original_result"))? != Type::Bool
            || graph_ground_type(graph, selected_result_root.ty).map_err(|_| unprepared("tag_equality_original_result"))? != Type::Bool {
            return Err(unprepared("tag_equality_original_relationship"));
        }
        for root in source_roots {
            if family_for(root)? != family || graph_ground_type(graph, root.ty).map_err(|_| unprepared("tag_equality_original_operand"))? != Type::Tag(first_name) {
                return Err(unprepared("tag_equality_original_operand_family"));
            }
        }
        let span = arena.arena.expr(origin.expression).span;
        if span.source_id != origin.source || operands.iter().any(|operand| operand.source != origin.source) { return Err(unprepared("tag_equality_original_source")); }
        Ok(Some(Self { origin, operands, rows: [left, right], op, span,
            operand_spans: [arena.arena.expr(authored_left).span, arena.arena.expr(authored_right).span], family, family_name: first_name,
            members: members.into_boxed_slice(), requirement, selected_roots, source_roots, result_root, selected_result_root,
            instruction: None, instructions: None, owner: None }))
    }
}

fn nominal_family(identity: QualifiedNominalIdentity) -> Option<QualifiedNominalIdentity> {
    match identity { QualifiedNominalIdentity::Source { source, namespace, declaration: declaration @ NominalDeclaration::Type(_), .. } =>
        Some(QualifiedNominalIdentity::Source { source, namespace, declaration, member: None }), _ => None }
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedTagEquality {
    original: BuildTagEqualitySource,
    payload: Box<[u32]>,
    operand_tags: [FullTag; 2],
    operand_payloads: [Box<[u32]>; 2],
}

impl PreparedTagEquality {
    pub(in crate::runtime::eval) fn retained_bytes(&self) -> usize {
        (self.payload.len() + self.operand_payloads.iter().map(|payload| payload.len()).sum::<usize>()) * std::mem::size_of::<u32>()
            + self.original.members.len() * std::mem::size_of::<(QualifiedNominalIdentity, Arc<SolvedNominalMember>)>()
            + self.original.members.len() * (std::mem::size_of::<SolvedNominalMember>() + 2 * std::mem::size_of::<usize>())
    }
}

fn tag_equality_span(store: &FullStore, raw: u32) -> Result<Span, IrVerifyError> {
    let location = IrLocationId::from_raw(raw).ok_or_else(|| IrVerifyError::new("tag equality source location is missing"))?;
    let span = store.locations.get(location.index()).ok_or_else(|| IrVerifyError::new("tag equality source location is missing"))?;
    let source = *store.location_sources.get(location.index()).ok_or_else(|| IrVerifyError::new("tag equality source location is missing"))?;
    Ok(Span::new(source, span.start as usize, span.start as usize + span.len as usize))
}

impl FullBuilder {
    pub(super) fn supports_original_tag_equality(&self, instruction: u32, origin: ExpressionIdentity, owner: InstructionOwner) -> bool {
        self.tag_equality_rows.iter().any(|original| original.origin == origin && original.instruction == Some(instruction) && original.owner == Some(owner))
    }

    pub(in crate::runtime::eval::indexed::full) fn stage_original_tag_equality(&self, expression: BuildExprId, instruction: u32,
        scratch: &BuildScratch, original: &BuildTagEqualitySource) -> Result<Option<BuildTagEqualitySource>, IrBuildError> {
        if self.store.tags.get(instruction as usize) != Some(&FullTag::ExprBinary) { return Ok(None); }
        let Some(BuildExprRow::Binary { op, left, right, span }) = scratch.expressions.get(expression.index()) else { return Err(unprepared("tag_equality_original_row")); };
        if *op != original.op || [*left, *right] != original.rows || *span != original.span
            || self.active_expression_origins.get(&expression) != Some(&original.origin) { return Err(unprepared("tag_equality_original_row")); }
        let raw = self.current_owner.ok_or_else(|| unprepared("tag_equality_original_owner"))?;
        let owner = if let Some(driver) = driver_owner_index(raw) { InstructionOwner::Driver(driver as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| unprepared("tag_equality_original_owner"))?) };
        let instructions = original.rows.map(|row| self.active_encoded_expressions.get(&row).copied());
        let [Some(left), Some(right)] = instructions else { return Err(unprepared("tag_equality_original_operand_instruction")); };
        for (row, origin) in original.rows.iter().zip(&original.operands) {
            if self.active_expression_origins.get(row) != Some(origin) { return Err(unprepared("tag_equality_original_operand_origin")); }
        }
        let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| unprepared("tag_equality_original_packet"))?;
        if words.len() != 4 || words[1..3] != [left, right] || self.store.binary_ops.get(words[0] as usize) != Some(&original.op)
            || tag_equality_span(&self.store, words[3]).map_err(|_| unprepared("tag_equality_original_span"))? != original.span { return Err(unprepared("tag_equality_original_packet")); }
        let mut original = original.clone();
        original.instruction = Some(instruction); original.instructions = Some([left, right]); original.owner = Some(owner);
        Ok(Some(original))
    }

    pub(super) fn prepare_tag_equality(&self, instruction: u32, origin: ExpressionIdentity, owner: InstructionOwner,
        operation: &crate::sema::check::SolvedOperation) -> Result<PreparedTagEquality, IrBuildError> {
        let original = self.tag_equality_rows.iter().find(|original| original.origin == origin && original.instruction == Some(instruction) && original.owner == Some(owner))
            .ok_or_else(|| unprepared("tag_equality_original_missing"))?;
        let solved = self.solved.as_ref().ok_or_else(|| unprepared("tag_equality_original_graph"))?;
        match (owner, operation.caller) {
            (InstructionOwner::Function(function), Some(caller)) if self.declaration_functions.get(&caller) == Some(&function) => {},
            (InstructionOwner::Driver(_), None) => {},
            _ => return Err(unprepared("tag_equality_original_caller")),
        }
        if original.requirement.requirement != operation.requirement || original.selected_result_root.ty != operation.result
            || operation.actual_arguments.as_slice() != original.selected_roots.map(|root| root.ty) { return Err(unprepared("tag_equality_original_requirement")); }
        let members = solved.checked_nominal_family_members(original.family).map_err(|_| unprepared("tag_equality_original_members"))?;
        if members.len() != original.members.len() || members.iter().zip(original.members.iter()).any(|((identity, member), (original_identity, original_member))|
            identity != original_identity || !Arc::ptr_eq(member, original_member)) { return Err(unprepared("tag_equality_original_members")); }
        let payload = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| unprepared("tag_equality_original_payload"))?.into();
        let instructions = original.instructions.ok_or_else(|| unprepared("tag_equality_original_operands"))?;
        let operand_tags = instructions.map(|instruction| self.store.tags[instruction as usize]);
        let operand_payloads = [self.store.payload(self.store.data[instructions[0] as usize].range()).map_err(|_| unprepared("tag_equality_original_operand_packet"))?.into(),
            self.store.payload(self.store.data[instructions[1] as usize].range()).map_err(|_| unprepared("tag_equality_original_operand_packet"))?.into()];
        Ok(PreparedTagEquality { original: original.clone(), payload, operand_tags, operand_payloads })
    }
}

#[cfg(test)]
mod tests;

impl FullVerifier {
    pub(in crate::runtime::eval::indexed) fn verify_prepared_tag_equality_contract(pools: &SemanticPools, operation: &PreparedOperation) -> Result<(), IrVerifyError> {
        let original = &operation.tag_equality.as_ref().ok_or_else(|| IrVerifyError::new("tag equality lacks its original nominal receipt"))?.original;
        let PreparedOperationAuthority::Language { authority, operation: PreparedLanguageOperation::Equality { op }, argument_order: OperationArgumentOrder::SourceOrder, statement_result_is_unit: false, .. } = operation.authority else { return Err(IrVerifyError::new("tag equality changes its selected authority")); };
        if op != original.op || authority != match op { BinaryOp::Eq => "language.binary.Eq", BinaryOp::Ne => "language.binary.Ne", _ => return Err(IrVerifyError::new("tag equality changes its operator")) }
            || operation.receiver.is_some() || operation.arguments.len() != 2 || operation.binding.supplied_slots.as_ref() != [0, 1]
            || !operation.binding.default_slots.is_empty() || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
            || operation.binding.operands.as_ref() != original.instructions.ok_or_else(|| IrVerifyError::new("tag equality loses its original operands"))?
            || operation.effects.creation != EffectSet::EMPTY || !operation.effects.inputs.is_empty() || !operation.effects.outputs.is_empty()
            || operation.membership_lowering.is_some() || operation.fallback_lowering.is_some() || operation.original_integer_addition.is_some()
            || operation.range_lowering.is_some() || operation.literal_comparison_slot.is_some() {
            return Err(IrVerifyError::new("tag equality changes its original argument or effect contract"));
        }
        for argument in &operation.arguments {
            let Some(TypeRef::Ground(ty)) = argument else { return Err(IrVerifyError::new("tag equality loses its closed operand type")); };
            if pools.to_type(*ty)? != Type::Tag(original.family_name) { return Err(IrVerifyError::new("tag equality changes its nominal operand family")); }
        }
        let TypeRef::Ground(result) = operation.result else { return Err(IrVerifyError::new("tag equality loses its closed result type")); };
        if pools.to_type(result)? != Type::Bool || nominal_family(original.family) != Some(original.family) || original.members.is_empty()
            || original.members.iter().any(|(identity, member)| nominal_family(*identity) != Some(original.family)
                || !matches!(identity, QualifiedNominalIdentity::Source { member: Some(name), .. } if *name == member.member)
                || member.kind != NominalMemberKind::Tag || member.family != original.family_name || !member.fields.is_empty()
                || !member.facets.is_empty() || member.scope.is_some()) {
            return Err(IrVerifyError::new("tag equality changes its original closed nominal roster"));
        }
        Ok(())
    }

    pub(in crate::runtime::eval::indexed::full) fn verify_tag_equality_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32,
        owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(operation) = generic.operation_at(instruction)? else { return Ok(false); };
        let Some(receipt) = &operation.tag_equality else { return Ok(false); };
        Self::verify_prepared_tag_equality_contract(&store.semantic, operation)?;
        let original = &receipt.original;
        let source = generic.operation_source(operation.source)?;
        if *expected != Type::Bool || original.instruction != Some(instruction) || original.owner != Some(owner)
            || source.instruction != instruction || source.owner != owner || source.origin != OperationSourceOrigin::Expression(original.origin)
            || source.expected != operation.authority || source.identity != operation.authority.identity()
            || generic.registered_instruction_origin(instruction, false) != Some((source.origin, owner))
            || store.tags.get(instruction as usize) != Some(&FullTag::ExprBinary) {
            return Err(IrVerifyError::new("tag equality changes its original source, owner or consumer"));
        }
        let words = store.payload(store.data[instruction as usize].range())?;
        if words != receipt.payload.as_ref() || words.len() != 4 || store.binary_ops.get(words[0] as usize) != Some(&original.op)
            || tag_equality_span(store, words[3])? != original.span {
            return Err(IrVerifyError::new("tag equality changes its original physical operator, operands or location"));
        }
        let instructions = original.instructions.ok_or_else(|| IrVerifyError::new("tag equality loses its original operand instructions"))?;
        for (index, operand) in instructions.into_iter().enumerate() {
            if generic.registered_instruction_origin(operand, false) != Some((OperationSourceOrigin::Expression(original.operands[index]), owner))
                || store.tags.get(operand as usize) != Some(&receipt.operand_tags[index])
                || store.payload(store.data[operand as usize].range())? != receipt.operand_payloads[index].as_ref() {
                return Err(IrVerifyError::new("tag equality changes its original operand source or physical packet"));
            }
            if receipt.operand_tags[index] == FullTag::ExprField {
                let packet = &receipt.operand_payloads[index];
                if packet.len() != 3 || tag_equality_span(store, packet[2])? != original.operand_spans[index] {
                    return Err(IrVerifyError::new("tag equality changes its original field location"));
                }
            }
            if receipt.operand_tags[index] == FullTag::ExprTag {
                let tag = generic.tag_constructor_at(operand)?.ok_or_else(|| IrVerifyError::new("tag equality loses its original member constructor"))?;
                if nominal_family(tag.original.authority) != Some(original.family) || !tag.fields.is_empty() || !tag.parameters.is_empty()
                    || !original.members.iter().any(|(identity, member)| *identity == tag.original.authority && member.member == tag.original.member
                        && match (&member.wire, &tag.original.wire) { (None, None) => true, (Some(left), Some(right)) => Arc::ptr_eq(left, right), _ => false }) {
                    return Err(IrVerifyError::new("tag equality changes its qualified original member"));
                }
            }
            Self::verify_generic_source(store, generic, operand, owner, &Type::Tag(original.family_name), instance, active)?;
        }
        Ok(true)
    }
}

impl FullExecution<'_> {
    pub(in crate::runtime::eval) fn tag_equality(&self, instruction: u32) -> Result<(), IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("tag equality belongs to another body")); }
        self.decoder.store.verify_generic_owner()?;
        let Some(generic) = self.generic_evidence() else { return Ok(()); };
        let Some(operation) = generic.operation_at(instruction)? else { return Ok(()); };
        if operation.tag_equality.is_none() { return Ok(()); }
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("tag equality owner is invalid"))?) };
        FullVerifier::verify_tag_equality_operand(self.decoder.store, generic, instruction, owner, &Type::Bool, self.instantiation, &mut Vec::new())?;
        Ok(())
    }

    pub(in crate::runtime::eval) fn tag_equality_values(&self, instruction: u32, left: &crate::runtime::eval::LoweredValue,
        right: &crate::runtime::eval::LoweredValue) -> Result<(), IrVerifyError> {
        let Some(generic) = self.generic_evidence() else { return Ok(()); };
        let Some(operation) = generic.operation_at(instruction)? else { return Ok(()); };
        let Some(receipt) = &operation.tag_equality else { return Ok(()); };
        for value in [left, right] {
            let crate::runtime::eval::LoweredValue::Tag(tag) = value else { return Err(IrVerifyError::new("tag equality operand is not its checked nominal value")); };
            if tag.type_name != receipt.original.family_name || !tag.fields.is_empty()
                || !receipt.original.members.iter().any(|(_, member)| member.member.as_str().as_str() == tag.name.as_ref()
                    && match (&member.wire, &tag.wire) { (None, None) => true, (Some(left), Some(right)) => Arc::ptr_eq(left, right), _ => false }) {
                return Err(IrVerifyError::new("tag equality operand changes its original family, member, payload or wire mapping"));
            }
        }
        Ok(())
    }
}
