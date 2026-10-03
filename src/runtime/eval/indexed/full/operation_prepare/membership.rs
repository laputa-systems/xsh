use super::*;
use crate::sema::operation_graph::{MembershipDomain, OperationArgumentOrder};
use crate::sema::check::ExpressionIdentity;
use crate::sema::inference::TypeNode;
use super::super::super::generic::NativeScalarReceiver;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildFoldedMembership {
    original: BuildMembershipSource,
    receiver: BuildFoldedNativeReceiver,
    literal: PreparedComparisonLiteral,
    negated: bool,
    positive: BuildBoolId,
    carrier: BuildBoolId,
    span: Span,
}

impl BuildFoldedMembership {
    pub(in crate::runtime::eval) fn from_original(original: &BuildMembershipSource, arena: &crate::syntax::arena::ArenaProgram,
        solved: &crate::sema::check::SolvedTypes, scratch: &BuildScratch, candidate: BuildBoolId) -> Result<Option<Self>, IrBuildError> {
        let Some(operation) = solved.operations.get(&original.origin) else { return Ok(None); };
        let Some(selected) = solved.graph.candidate_evidence(operation.requirement).map_err(|_| unprepared("folded_membership_original_candidate"))? else { return Ok(None); };
        let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate)
            .map_err(|_| unprepared("folded_membership_original_authority"))? else { return Ok(None); };
        let PreparedLanguageOperation::Membership { domain: domain @ (MembershipDomain::List | MembershipDomain::Str), negated } = metadata.operation else { return Ok(None); };
        let Some(BuildExprRow::Param(slot)) = scratch.expressions.get(original.container_row.index()) else { return Ok(None); };
        if scratch.value_binding_uses.contains_key(&original.container_origin) { return Ok(None); }
        let crate::syntax::arena::ArenaExprKind::Ident(name) = arena.arena.expr(original.container_origin.expression).kind else { return Ok(None); };
        let literal = match arena.arena.expr(original.needle_origin.expression).kind {
            crate::syntax::arena::ArenaExprKind::Int(value) => PreparedComparisonLiteral::Int(arena.arena.int_literal(value).value().ok_or_else(|| unprepared("folded_membership_original_literal"))?),
            crate::syntax::arena::ArenaExprKind::Str(value) => PreparedComparisonLiteral::Str(arena.arena.string_literal(value).as_ref().into()),
            _ => return Ok(None),
        };
        let Some(container) = operation.receiver.and_then(|ty| solved.graph.export_type(ty).ok()) else { return Ok(None); };
        if !matches!((domain, &container, &literal),
            (MembershipDomain::List, Type::List(item), PreparedComparisonLiteral::Int(_)) if **item == Type::Int)
            && !matches!((domain, &container, &literal), (MembershipDomain::Str, Type::Str, PreparedComparisonLiteral::Str(_))) { return Ok(None); }
        let positive = if negated {
            let Some(BuildBoolRow::Not(positive)) = scratch.bools.get(candidate.index()) else { return Err(unprepared("folded_membership_original_negation")); };
            *positive
        } else { candidate };
        let span = match (scratch.bools.get(positive.index()), &literal) {
            (Some(BuildBoolRow::ContainsSlot { slot: actual, needle: LoweredValue::Int(value), span }), PreparedComparisonLiteral::Int(expected))
                if actual == slot && value == expected => *span,
            (Some(BuildBoolRow::StrContainsSlot { slot: actual, needle, span }), PreparedComparisonLiteral::Str(expected))
                if actual == slot && needle.as_ref() == expected.as_ref() => *span,
            _ => return Err(unprepared("folded_membership_original_carrier")),
        };
        let crate::syntax::arena::ArenaExprKind::Binary { op, left, right } = arena.arena.expr(original.origin.expression).kind else { return Err(unprepared("folded_membership_original_source")); };
        if op != if negated { BinaryOp::NotIn } else { BinaryOp::In }
            || left != original.needle_origin.expression || right != original.container_origin.expression
            || arena.arena.expr(original.origin.expression).span != span || original.needle_row != original.needle_material_row
            || original.env_getter.is_some() { return Err(unprepared("folded_membership_original_source")); }
        let Some(caller) = operation.caller else { return Ok(None); };
        let declaration = solved.declarations.get(&caller).ok_or_else(|| unprepared("folded_membership_original_declaration"))?;
        let TypeNode::Arrow(signature) = solved.graph.node(solved.graph.resolved(declaration.signature).map_err(|_| unprepared("folded_membership_original_signature"))?)
            .map_err(|_| unprepared("folded_membership_original_signature"))? else { return Err(unprepared("folded_membership_original_signature")); };
        let parameter = signature.params.get(*slot).ok_or_else(|| unprepared("folded_membership_original_parameter"))?;
        if parameter.label != name || operation.receiver.is_none_or(|receiver| solved.graph.export_type(receiver).ok() != solved.graph.export_type(parameter.ty).ok()) {
            return Err(unprepared("folded_membership_original_parameter"));
        }
        Ok(Some(Self { original: original.clone(), receiver: BuildFoldedNativeReceiver { origin: original.container_origin,
            name, slot: u32::try_from(*slot).map_err(|_| unprepared("folded_membership_original_slot"))?, binding: None },
            literal, negated, positive, carrier: candidate, span }))
    }
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) enum PreparedMembershipLowering {
    Binary(PreparedBinaryMembershipLowering),
    Folded(PreparedFoldedMembershipLowering),
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedFoldedMembershipLowering {
    origin: ExpressionIdentity,
    needle_origin: ExpressionIdentity,
    container_origin: ExpressionIdentity,
    receiver: NativeScalarReceiver,
    tag: FullTag,
    payload: Box<[u32]>,
    positive_instruction: u32,
    positive_tag: FullTag,
    positive_payload: Box<[u32]>,
    span: Span,
    literal: PreparedComparisonLiteral,
    literal_tag: Option<FullValueTag>,
    literal_payload: Box<[u32]>,
    requirement: ScopedRequirementRoot,
    receiver_root: ScopedRoot,
    argument_root: ScopedRoot,
    needle_root: ScopedRoot,
    container_root: ScopedRoot,
    result_root: ScopedRoot,
    operation_result_root: ScopedRoot,
}

impl PreparedMembershipLowering {
    pub(in crate::runtime::eval) fn retained_bytes(&self) -> usize {
        match self {
            Self::Binary(original) => original.retained_bytes(),
            Self::Folded(original) => (original.payload.len() + original.positive_payload.len() + original.literal_payload.len()) * std::mem::size_of::<u32>()
                + match &original.literal { PreparedComparisonLiteral::Str(value) => value.len(), _ => 0 },
        }
    }

    #[cfg(test)]
    fn as_binary(&self) -> Option<&PreparedBinaryMembershipLowering> { if let Self::Binary(original) = self { Some(original) } else { None } }
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildMembershipSource {
    pub origin: crate::sema::check::ExpressionIdentity,
    pub needle_origin: crate::sema::check::ExpressionIdentity,
    pub container_origin: crate::sema::check::ExpressionIdentity,
    pub needle_material_row: BuildExprId,
    pub needle_row: BuildExprId,
    pub container_row: BuildExprId,
    pub env_getter: Option<Arc<crate::sema::check::OriginalEnvPathListGetter>>,
    pub encoded_env_getter: Option<PreparedMembershipEnvGetter>,
    pub instruction: Option<u32>,
    pub needle_instruction: Option<u32>,
    pub container_instruction: Option<u32>,
    pub owner: Option<InstructionOwner>,
}

impl BuildMembershipSource {
    pub(in crate::runtime::eval) fn from_authored(arena: &crate::syntax::arena::ArenaProgram, solved: &crate::sema::check::SolvedTypes,
        origin: crate::sema::check::ExpressionIdentity, needle_material_row: BuildExprId, needle_row: BuildExprId,
        container_row: BuildExprId) -> Result<Option<Self>, IrBuildError> {
        let crate::syntax::arena::ArenaExprKind::Binary { op: BinaryOp::In | BinaryOp::NotIn, left, right } = arena.arena.expr(origin.expression).kind else { return Ok(None); };
        if arena.arena.expr(origin.expression).span.source_id != origin.source { return Err(unprepared("membership_authored_source")); }
        let identity = |expression| crate::sema::check::ExpressionIdentity { source: arena.arena.expr(expression).span.source_id,
            namespace: origin.namespace, expression };
        let needle_origin = identity(left);
        let container_origin = identity(right);
        let env_getter = solved.original_env_path_list_getter(container_origin).map_err(|_| unprepared("membership_authored_env_getter"))?;
        Ok(Some(Self { origin, needle_origin, container_origin, needle_material_row, needle_row, container_row,
            env_getter, encoded_env_getter: None, instruction: None, needle_instruction: None, container_instruction: None, owner: None }))
    }
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedMembershipEnvGetter {
    pub source: Arc<crate::sema::check::OriginalEnvPathListGetter>,
    pub instruction: u32,
    pub instruction_payload: Box<[u32]>,
    pub arguments_flags: u8,
    pub arguments_payload: Box<[u32]>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedBinaryMembershipLowering {
    pub instruction_payload: Box<[u32]>,
    pub span: Span,
    pub needle_instruction: u32,
    pub needle_origin: crate::sema::check::ExpressionIdentity,
    pub container_origin: crate::sema::check::ExpressionIdentity,
    pub requirement: ScopedRequirementRoot,
    pub receiver_root: ScopedRoot,
    pub argument_root: ScopedRoot,
    pub needle_root: ScopedRoot,
    pub container_root: ScopedRoot,
    pub result_root: ScopedRoot,
    pub operation_result_root: ScopedRoot,
    pub uint_key: Option<PreparedMembershipUIntKey>,
    pub env_getter: Option<PreparedMembershipEnvGetter>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedMembershipUIntKey {
    pub try_payload: Box<[u32]>,
    pub require_instruction: u32,
    pub require_payload: Box<[u32]>,
    pub require_span: Span,
}

impl PreparedBinaryMembershipLowering {
    pub(in crate::runtime::eval) fn retained_bytes(&self) -> usize {
        self.instruction_payload.len() * std::mem::size_of::<u32>()
            + self.uint_key.as_ref().map_or(0, |guard| (guard.try_payload.len() + guard.require_payload.len()) * std::mem::size_of::<u32>())
            + self.env_getter.as_ref().map_or(0, |getter| (getter.instruction_payload.len() + getter.arguments_payload.len()) * std::mem::size_of::<u32>()
                + std::mem::size_of::<crate::sema::check::OriginalEnvPathListGetter>())
    }
}

fn membership_span(store: &FullStore, location: u32) -> Result<Span, IrVerifyError> {
    let location = IrLocationId::from_raw(location).ok_or_else(|| IrVerifyError::new("membership source location is missing"))?;
    let physical = store.locations.get(location.index()).ok_or_else(|| IrVerifyError::new("membership source location is missing"))?;
    let source = *store.location_sources.get(location.index()).ok_or_else(|| IrVerifyError::new("membership source location is missing"))?;
    Ok(Span::new(source, physical.start as usize, physical.start as usize + physical.len as usize))
}

pub(super) fn membership_types(domain: MembershipDomain, container: &Type, needle: &Type) -> bool {
    match (domain, container, needle) {
        (MembershipDomain::List, Type::List(item), needle) => GenericEvidenceStore::supports_list_index_item(item)
            && GenericEvidenceStore::supports_list_index_item(needle) && needle.matches_expected(item),
        (MembershipDomain::Map, Type::Map(key, value), needle) => key.is_map_key() && needle.is_map_key()
            && GenericEvidenceStore::supports_list_index_item(value) && needle.matches_expected(key),
        (MembershipDomain::Str, Type::Str, Type::Str)
        | (MembershipDomain::Bytes, Type::Bytes, Type::Bytes)
        | (MembershipDomain::Record, Type::ErasedRecord, Type::Str)
        | (MembershipDomain::Path { needle: Atom::Str }, Type::Path, Type::Str)
        | (MembershipDomain::Path { needle: Atom::Path }, Type::Path, Type::Path)
        | (MembershipDomain::EnvPathList, Type::EnvPathList, Type::Path) => true,
        (MembershipDomain::Record, Type::Record(fields), Type::Str) => fields.values().all(GenericEvidenceStore::supports_list_index_item),
        _ => false,
    }
}

impl FullBuilder {
    pub(super) fn supports_original_folded_membership(&self, instruction: u32, expression: ExpressionIdentity,
        owner: InstructionOwner, domain: MembershipDomain) -> bool {
        self.folded_membership_rows.iter().any(|(actual, original, actual_owner)| *actual == instruction
            && original.original.origin == expression && *actual_owner == owner
            && matches!((domain, &original.literal), (MembershipDomain::List, PreparedComparisonLiteral::Int(_))
                | (MembershipDomain::Str, PreparedComparisonLiteral::Str(_))))
    }

    fn prepare_folded_membership_lowering(&self, instruction: u32, expression: ExpressionIdentity,
        owner: InstructionOwner, operation: &crate::sema::check::SolvedOperation) -> Result<PreparedMembershipLowering, IrBuildError> {
        let (_, original, original_owner) = self.folded_membership_rows.iter().find(|(actual, original, actual_owner)|
            *actual == instruction && original.original.origin == expression && *actual_owner == owner)
            .ok_or_else(|| unprepared("folded_membership_original_missing"))?;
        if *original_owner != owner || original.receiver.binding.is_some() { return Err(unprepared("folded_membership_original_owner")); }
        let solved = self.solved.as_ref().ok_or_else(|| unprepared("folded_membership_original_graph"))?;
        let graph = &solved.graph;
        let caller = operation.caller.ok_or_else(|| unprepared("folded_membership_original_caller"))?;
        let InstructionOwner::Function(function) = owner else { return Err(unprepared("folded_membership_original_owner")); };
        if self.declaration_functions.get(&caller) != Some(&function) { return Err(unprepared("folded_membership_original_caller")); }
        let scope = solved.operation_scope(crate::sema::check::ProducerFlowSource::Expression(expression), operation)
            .map_err(|_| unprepared("folded_membership_original_scope"))?;
        let requirement = ScopedRequirementRoot { requirement: operation.requirement, scope };
        graph.validate_requirement_scoped(requirement).map_err(|_| unprepared("folded_membership_original_requirement"))?;
        let receiver_root = ScopedRoot { ty: operation.receiver.ok_or_else(|| unprepared("folded_membership_original_receiver"))?, scope };
        let [argument] = operation.actual_arguments.as_slice() else { return Err(unprepared("folded_membership_original_argument")); };
        let argument_root = ScopedRoot { ty: *argument, scope };
        let operation_result_root = ScopedRoot { ty: operation.result, scope };
        let source_root = |origin| -> Result<ScopedRoot, IrBuildError> {
            if solved.expression_owners.get(&origin).copied() != Some(caller) { return Err(unprepared("folded_membership_original_operand_owner")); }
            let root = ScopedRoot { ty: *solved.expressions.get(&origin).ok_or_else(|| unprepared("folded_membership_original_expression"))?,
                scope: solved.expression_scope(origin, Some(caller)).map_err(|_| unprepared("folded_membership_original_expression_scope"))? };
            graph.validate_scoped(root).map_err(|_| unprepared("folded_membership_original_expression_root"))?;
            Ok(root)
        };
        let result_root = source_root(expression)?;
        let needle_root = source_root(original.original.needle_origin)?;
        let container_root = source_root(original.original.container_origin)?;
        for root in [receiver_root, argument_root, operation_result_root] { graph.validate_scoped(root).map_err(|_| unprepared("folded_membership_original_root"))?; }
        let ground = |root: ScopedRoot| graph_ground_type(graph, root.ty).map_err(|_| unprepared("folded_membership_original_type"));
        let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation.requirement).map_err(|_| unprepared("folded_membership_original_call"))? else { return Err(unprepared("folded_membership_original_call")); };
        let call = graph.operation_call(call).map_err(|_| unprepared("folded_membership_original_call"))?;
        if call.receiver != operation.receiver || call.arguments.as_slice() != [Some(*argument)] || call.result != operation.result
            || ground(receiver_root)? != ground(container_root)? || ground(argument_root)? != ground(needle_root)?
            || ground(needle_root)? != original.literal.ty() || ground(result_root)? != Type::Bool || ground(operation_result_root)? != Type::Bool
            || original.original.needle_origin.source != expression.source || original.original.container_origin.source != expression.source
            || original.original.needle_origin.namespace != expression.namespace || original.original.container_origin.namespace != expression.namespace {
            return Err(unprepared("folded_membership_original_relationship"));
        }
        let declaration = solved.declarations.get(&caller).ok_or_else(|| unprepared("folded_membership_original_declaration"))?;
        let TypeNode::Arrow(signature) = graph.node(graph.resolved(declaration.signature).map_err(|_| unprepared("folded_membership_original_signature"))?)
            .map_err(|_| unprepared("folded_membership_original_signature"))? else { return Err(unprepared("folded_membership_original_signature")); };
        let parameter = signature.params.get(original.receiver.slot as usize).ok_or_else(|| unprepared("folded_membership_original_parameter"))?;
        if parameter.label != original.receiver.name || graph_ground_type(graph, parameter.ty).map_err(|_| unprepared("folded_membership_original_parameter_type"))? != ground(container_root)? {
            return Err(unprepared("folded_membership_original_parameter"));
        }
        let receiver = if let Some(&scope) = self.generic_declarations.get(&caller) {
            NativeScalarReceiver::ScopedParameter { scope, name: original.receiver.name, slot: original.receiver.slot }
        } else {
            NativeScalarReceiver::Parameter { declaration: caller,
                signature: SignatureId::from_raw(self.store.functions[function.index()].signature).ok_or_else(|| unprepared("folded_membership_original_signature"))?,
                name: original.receiver.name, slot: original.receiver.slot }
        };
        let tag = *self.store.tags.get(instruction as usize).ok_or_else(|| unprepared("folded_membership_original_instruction"))?;
        let payload: Box<[u32]> = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| unprepared("folded_membership_original_packet"))?.into();
        let positive_instruction = if original.negated {
            if tag != FullTag::BoolNot || payload.len() != 1 || original.carrier == original.positive { return Err(unprepared("folded_membership_original_negation")); }
            payload[0]
        } else {
            if original.carrier != original.positive { return Err(unprepared("folded_membership_original_carrier")); }
            instruction
        };
        let positive_tag = *self.store.tags.get(positive_instruction as usize).ok_or_else(|| unprepared("folded_membership_original_positive"))?;
        let positive_payload: Box<[u32]> = self.store.payload(self.store.data[positive_instruction as usize].range()).map_err(|_| unprepared("folded_membership_original_positive_packet"))?.into();
        if positive_payload.len() != 3 || positive_payload[0] != original.receiver.slot { return Err(unprepared("folded_membership_original_slot")); }
        let span = membership_span(&self.store, positive_payload[2]).map_err(|_| unprepared("folded_membership_original_span"))?;
        if span.source_id != expression.source || span != original.span { return Err(unprepared("folded_membership_original_span")); }
        let (literal_tag, literal_payload) = match (&original.literal, positive_tag) {
            (PreparedComparisonLiteral::Int(_), FullTag::BoolContainsSlot) => {
                let index = positive_payload[1] as usize;
                let tag = *self.store.values.get(index).ok_or_else(|| unprepared("folded_membership_original_literal"))?;
                let payload = self.store.payload(self.store.value_data.get(index).ok_or_else(|| unprepared("folded_membership_original_literal"))?.range())
                    .map_err(|_| unprepared("folded_membership_original_literal_packet"))?;
                (Some(tag), payload.into())
            }
            (PreparedComparisonLiteral::Str(_), FullTag::BoolStrContainsSlot) => (None, Box::new([]) as Box<[u32]>),
            _ => return Err(unprepared("folded_membership_original_literal_carrier")),
        };
        let recipe = PreparedFoldedMembershipLowering { origin: expression, needle_origin: original.original.needle_origin,
            container_origin: original.original.container_origin, receiver, tag, payload, positive_instruction, positive_tag, positive_payload,
            span, literal: original.literal.clone(), literal_tag, literal_payload, requirement, receiver_root, argument_root, needle_root,
            container_root, result_root, operation_result_root };
        verify_folded_membership_literal(&self.store, &recipe).map_err(|error| IrBuildError::verification("folded_membership_original_literal", error))?;
        Ok(PreparedMembershipLowering::Folded(recipe))
    }

    pub(in crate::runtime::eval::indexed::full) fn stage_original_membership(&self, expression: BuildExprId, instruction: u32,
        scratch: &BuildScratch, original: &BuildMembershipSource) -> Result<Option<BuildMembershipSource>, IrBuildError> {
        if self.store.tags.get(instruction as usize) != Some(&FullTag::ExprBinary) { return Ok(None); }
        let BuildExprRow::Binary { op, left, right, span } = scratch.expressions.get(expression.index()).ok_or_else(|| unprepared("membership_original_row"))? else {
            return Err(unprepared("membership_original_row"));
        };
        let raw = self.current_owner.ok_or_else(|| unprepared("membership_original_owner"))?;
        let owner = if let Some(step) = driver_owner_index(raw) { InstructionOwner::Driver(step as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| unprepared("membership_original_owner"))?) };
        let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| unprepared("membership_original_payload"))?;
        if !matches!(op, BinaryOp::In | BinaryOp::NotIn) || *left != original.needle_row || *right != original.container_row
            || self.active_expression_origins.get(&expression) != Some(&original.origin)
            || self.active_expression_origins.get(&original.needle_material_row) != Some(&original.needle_origin)
            || self.active_expression_origins.get(&original.container_row) != Some(&original.container_origin)
            || words.len() != 4 || self.active_encoded_expressions.get(left) != words.get(1)
            || self.active_encoded_expressions.get(right) != words.get(2)
            || words.first().and_then(|&index| self.store.binary_ops.get(index as usize)) != Some(op)
            || membership_span(&self.store, words[3]).map_err(|_| unprepared("membership_original_location"))? != *span {
            return Err(unprepared("membership_original_children"));
        }
        let mut original = original.clone();
        original.instruction = Some(instruction);
        original.needle_instruction = self.active_encoded_expressions.get(&original.needle_material_row).copied();
        original.container_instruction = self.active_encoded_expressions.get(&original.container_row).copied();
        if original.needle_instruction.is_none() || original.container_instruction.is_none() { return Err(unprepared("membership_original_material_allocation")); }
        original.owner = Some(owner);
        if let Some(getter) = &original.env_getter {
            let BuildExprRow::ModuleCall { op: RuntimeOp::EnvPathList, cli_plan: None, args, span } = scratch.expressions.get(right.index()).ok_or_else(|| unprepared("membership_env_getter_row"))? else {
                return Err(unprepared("membership_env_getter_row"));
            };
            if !args.is_empty() || *span != getter.span || getter.origin != original.container_origin { return Err(unprepared("membership_env_getter_source")); }
            let getter_instruction = *self.active_encoded_expressions.get(right).ok_or_else(|| unprepared("membership_env_getter_instruction"))?;
            if self.store.tags.get(getter_instruction as usize) != Some(&FullTag::ExprModuleCall) { return Err(unprepared("membership_env_getter_instruction")); }
            let packet = self.store.payload(self.store.data[getter_instruction as usize].range()).map_err(|_| unprepared("membership_env_getter_packet"))?;
            if packet.len() != 4 || packet[1] != 0 || self.store.runtime_ops.get(packet[0] as usize) != Some(&RuntimeOp::EnvPathList)
                || membership_span(&self.store, packet[3]).map_err(|_| unprepared("membership_env_getter_location"))? != getter.span {
                return Err(unprepared("membership_env_getter_packet"));
            }
            let arguments = IrBlockId::from_raw(packet[2]).and_then(|id| self.store.blocks.get(id.index())).ok_or_else(|| unprepared("membership_env_getter_arguments"))?;
            let payload = self.store.payload(arguments.instructions).map_err(|_| unprepared("membership_env_getter_arguments"))?;
            if arguments.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST || payload != [0] { return Err(unprepared("membership_env_getter_arguments")); }
            original.encoded_env_getter = Some(PreparedMembershipEnvGetter { source: Arc::clone(getter), instruction: getter_instruction,
                instruction_payload: packet.to_vec().into_boxed_slice(), arguments_flags: arguments.flags,
                arguments_payload: payload.to_vec().into_boxed_slice(), span: *span });
        }
        Ok(Some(original))
    }

    pub(super) fn prepare_membership_lowering(&self, instruction: u32, expression: crate::sema::check::ExpressionIdentity,
        owner: InstructionOwner, operation: &crate::sema::check::SolvedOperation) -> Result<PreparedMembershipLowering, IrBuildError> {
        if self.folded_membership_rows.iter().any(|(actual, original, actual_owner)| *actual == instruction && original.original.origin == expression && *actual_owner == owner) {
            return self.prepare_folded_membership_lowering(instruction, expression, owner, operation);
        }
        let solved = self.solved.as_ref().ok_or_else(|| unprepared("membership_original_graph_missing"))?;
        let graph = &solved.graph;
        let authored = self.membership_rows.iter().find(|source| source.instruction == Some(instruction)
            && source.origin == expression && source.owner == Some(owner)).ok_or_else(|| unprepared("membership_original_source_receipt"))?;
        match (owner, operation.caller) {
            (InstructionOwner::Function(function), Some(caller)) if self.declaration_functions.get(&caller) == Some(&function) => {},
            (InstructionOwner::Driver(_), None) => {},
            _ => return Err(unprepared("membership_original_caller")),
        }
        let scope = solved.operation_scope(crate::sema::check::ProducerFlowSource::Expression(expression), operation).map_err(|_| unprepared("membership_original_scope"))?;
        let requirement = ScopedRequirementRoot { requirement: operation.requirement, scope };
        graph.validate_requirement_scoped(requirement).map_err(|_| unprepared("membership_original_requirement"))?;
        let receiver_root = ScopedRoot { ty: operation.receiver.ok_or_else(|| unprepared("membership_original_receiver"))?, scope };
        let [argument] = operation.actual_arguments.as_slice() else { return Err(unprepared("membership_original_argument")); };
        let argument_root = ScopedRoot { ty: *argument, scope };
        let operation_result_root = ScopedRoot { ty: operation.result, scope };
        let result_root = ScopedRoot { ty: *solved.expressions.get(&expression).ok_or_else(|| unprepared("membership_original_result"))?,
            scope: solved.expression_scope(expression, operation.caller).map_err(|_| unprepared("membership_original_result_scope"))? };
        for root in [receiver_root, argument_root, result_root, operation_result_root] { graph.validate_scoped(root).map_err(|_| unprepared("membership_original_root"))?; }
        let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation.requirement).map_err(|_| unprepared("membership_original_call"))? else { return Err(unprepared("membership_original_call")); };
        let call = graph.operation_call(call).map_err(|_| unprepared("membership_original_call"))?;
        if call.receiver != operation.receiver || call.arguments.as_slice() != [Some(*argument)] || call.result != operation.result
            || graph_ground_type(graph, result_root.ty).map_err(|_| unprepared("membership_original_result_type"))? != Type::Bool
            || graph_ground_type(graph, operation_result_root.ty).map_err(|_| unprepared("membership_original_result_type"))? != Type::Bool {
            return Err(unprepared("membership_original_call_relationship"));
        }
        let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| unprepared("membership_original_instruction"))?;
        if self.store.tags.get(instruction as usize) != Some(&FullTag::ExprBinary) || words.len() != 4 { return Err(unprepared("membership_original_instruction")); }
        let span = membership_span(&self.store, words[3]).map_err(|_| unprepared("membership_original_location"))?;
        if span.source_id != expression.source { return Err(unprepared("membership_original_location")); }
        let mut needle_instruction = words[1];
        let uint_key = if matches!(graph_ground_type(graph, receiver_root.ty).map_err(|_| unprepared("membership_original_receiver_type"))?, Type::Map(key, _) if *key == Type::UInt) {
            if self.store.tags.get(needle_instruction as usize) != Some(&FullTag::ExprTry) { return Err(unprepared("membership_original_uint_key_try")); }
            let try_payload = self.store.payload(self.store.data[needle_instruction as usize].range()).map_err(|_| unprepared("membership_original_uint_key_try"))?;
            let [require_instruction] = try_payload else { return Err(unprepared("membership_original_uint_key_try")); };
            if self.store.tags.get(*require_instruction as usize) != Some(&FullTag::ExprRequire) { return Err(unprepared("membership_original_uint_key_require")); }
            let require_payload = self.store.payload(self.store.data[*require_instruction as usize].range()).map_err(|_| unprepared("membership_original_uint_key_require"))?;
            if require_payload.len() != 5 || require_payload[3] != 0
                || crate::runtime::eval::indexed::TypeId::from_raw(require_payload[1]).and_then(|ty| self.store.semantic.to_type(ty).ok()) != Some(Type::UInt)
                || self.store.string(require_payload[2]).ok() != Some("UInt") { return Err(unprepared("membership_original_uint_key_require")); }
            needle_instruction = require_payload[0];
            Some(PreparedMembershipUIntKey { try_payload: try_payload.to_vec().into_boxed_slice(), require_instruction: *require_instruction,
                require_payload: require_payload.to_vec().into_boxed_slice(), require_span: membership_span(&self.store, require_payload[4]).map_err(|_| unprepared("membership_original_uint_key_location"))? })
        } else { None };
        let origin = |operand| self.generic_expression_rows.iter().find(|&&(instruction, _, actual_owner)| instruction == operand && actual_owner == owner)
            .map(|&(_, origin, _)| origin).ok_or_else(|| unprepared("membership_original_operand_origin"));
        let needle_origin = origin(needle_instruction)?;
        let container_origin = origin(words[2])?;
        if needle_origin != authored.needle_origin || container_origin != authored.container_origin
            || authored.needle_instruction != Some(needle_instruction) || authored.container_instruction != Some(words[2]) {
            return Err(unprepared("membership_original_authored_operands"));
        }
        let root = |origin| {
            let ty = *solved.expressions.get(&origin).ok_or_else(|| unprepared("membership_original_operand_type"))?;
            let scope = solved.expression_scope(origin, operation.caller).map_err(|_| unprepared("membership_original_operand_scope"))?;
            let root = ScopedRoot { ty, scope };
            graph.validate_scoped(root).map_err(|_| unprepared("membership_original_operand_root"))?;
            Ok(root)
        };
        let needle_root = root(needle_origin)?;
        let container_root = root(container_origin)?;
        for (origin, actual, expected) in [(needle_origin, needle_root, argument_root), (container_origin, container_root, receiver_root)] {
            if origin.source != expression.source || origin.namespace != expression.namespace
                || graph_ground_type(graph, actual.ty).map_err(|_| unprepared("membership_original_operand_type"))?
                    != graph_ground_type(graph, expected.ty).map_err(|_| unprepared("membership_original_argument_type"))? {
                return Err(unprepared("membership_original_operand_relationship"));
            }
        }
        let env_getter = authored.encoded_env_getter.clone();
        if let Some(getter) = &env_getter {
            let original = solved.original_env_path_list_getter(container_origin).map_err(|_| unprepared("membership_original_env_getter"))?
                .ok_or_else(|| unprepared("membership_original_env_getter"))?;
            if !Arc::ptr_eq(&original, &getter.source) || original.caller != operation.caller || original.checked != container_root.ty
                || getter.instruction != words[2] || original.creation != crate::sema::inference::EffectSet::ENV {
                return Err(unprepared("membership_original_env_getter_relationship"));
            }
            if let Some(caller) = operation.caller {
                let caller = solved.declarations.get(&caller).ok_or_else(|| unprepared("membership_original_env_getter_caller"))?;
                let EffectSummary::Closed(budget) = graph.closed_effect_summary(caller.effective_effects).map_err(|_| unprepared("membership_original_env_getter_budget"))? else {
                    return Err(unprepared("membership_original_env_getter_budget"));
                };
                if !budget.contains(original.creation) { return Err(unprepared("membership_original_env_getter_budget")); }
            }
        }
        Ok(PreparedMembershipLowering::Binary(PreparedBinaryMembershipLowering { instruction_payload: words.to_vec().into_boxed_slice(), span, needle_instruction, needle_origin,
            container_origin, requirement, receiver_root, argument_root, needle_root, container_root, result_root, operation_result_root, uint_key, env_getter }))
    }
}

impl FullVerifier {
    fn verify_prepared_folded_membership_contract(pools: &SemanticPools, operation: &PreparedOperation) -> Result<(), IrVerifyError> {
        let Some(PreparedMembershipLowering::Folded(original)) = &operation.membership_lowering else { return Err(IrVerifyError::new("folded membership loses its original carrier")); };
        let PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Membership { domain, negated }, .. } = operation.authority else { return Err(IrVerifyError::new("folded membership loses its original selected operation")); };
        let Some(TypeRef::Ground(container)) = operation.receiver else { return Err(IrVerifyError::new("folded membership receiver is not ground")); };
        let Some(TypeRef::Ground(needle)) = operation.arguments[0] else { return Err(IrVerifyError::new("folded membership needle is not ground")); };
        let TypeRef::Ground(result) = operation.result else { return Err(IrVerifyError::new("folded membership result is not ground")); };
        let container = pools.to_type(container)?;
        let needle = pools.to_type(needle)?;
        let domain_matches = matches!((domain, &container, &original.literal),
            (MembershipDomain::List, Type::List(item), PreparedComparisonLiteral::Int(_)) if **item == Type::Int)
            || matches!((domain, &container, &original.literal), (MembershipDomain::Str, Type::Str, PreparedComparisonLiteral::Str(_)));
        let positive_tag = match original.literal { PreparedComparisonLiteral::Int(_) => FullTag::BoolContainsSlot,
            PreparedComparisonLiteral::Str(_) => FullTag::BoolStrContainsSlot, _ => return Err(IrVerifyError::new("folded membership has another original literal kind")) };
        if !domain_matches || needle != original.literal.ty() || pools.to_type(result)? != Type::Bool
            || original.positive_tag != positive_tag || original.positive_payload.len() != 3
            || original.tag != if negated { FullTag::BoolNot } else { positive_tag }
            || negated && (original.payload.as_ref() != [original.positive_instruction])
            || !negated && original.payload != original.positive_payload {
            return Err(IrVerifyError::new("folded membership changes its original container, needle or negation relationship"));
        }
        Ok(())
    }

    fn verify_folded_membership_operand(store: &FullStore, generic: &GenericEvidenceStore, operation: &PreparedOperation,
        instruction: u32, owner: InstructionOwner, expected: &Type) -> Result<bool, IrVerifyError> {
        Self::verify_prepared_folded_membership_contract(&store.semantic, operation)?;
        let Some(PreparedMembershipLowering::Folded(original)) = &operation.membership_lowering else { unreachable!() };
        let source = generic.operation_source(operation.source)?;
        let Some(TypeRef::Ground(container_type)) = operation.receiver else { unreachable!() };
        let PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Membership { negated, .. }, .. } = operation.authority else { unreachable!() };
        if *expected != Type::Bool || source.origin != OperationSourceOrigin::Expression(original.origin)
            || original.needle_origin.source != original.origin.source || original.container_origin.source != original.origin.source
            || original.needle_origin.namespace != original.origin.namespace || original.container_origin.namespace != original.origin.namespace
            || source.instruction != instruction || source.owner != owner || source.expected != operation.authority
            || generic.registered_instruction_origin(instruction, false) != Some((source.origin, owner))
            || store.tags.get(instruction as usize) != Some(&original.tag)
            || store.payload(store.data.get(instruction as usize).ok_or_else(|| IrVerifyError::new("folded membership carrier is missing"))?.range())? != original.payload.as_ref()
            || !negated && original.positive_instruction != instruction
            || negated && original.positive_instruction == instruction
            || store.tags.get(original.positive_instruction as usize) != Some(&original.positive_tag)
            || store.payload(store.data.get(original.positive_instruction as usize).ok_or_else(|| IrVerifyError::new("folded membership positive carrier is missing"))?.range())? != original.positive_payload.as_ref()
            || membership_span(store, original.positive_payload[2])? != original.span || original.span.source_id != original.origin.source {
            return Err(IrVerifyError::new("folded membership changes its original source, opcode, carrier allocation or packet"));
        }
        verify_folded_membership_literal(store, original)?;
        let slot = match original.receiver {
            NativeScalarReceiver::Parameter { declaration, signature, name, slot } => {
                let function = generic.checked_function(declaration)?;
                let (label, parameter, _) = store.semantic.signature_param(signature, slot as usize)?;
                if owner != InstructionOwner::Function(function.target) || function.signature != signature || label != name || parameter != container_type {
                    return Err(IrVerifyError::new("folded membership changes its original receiver parameter"));
                }
                slot
            }
            NativeScalarReceiver::ScopedParameter { scope, name, slot } => {
                let declaration = generic.scope(scope)?;
                let parameter = *declaration.parameters.get(slot as usize).ok_or_else(|| IrVerifyError::new("folded membership receiver parameter is missing"))?;
                if owner != InstructionOwner::Function(declaration.owner) || declaration.parameter_names.get(slot as usize) != Some(&name)
                    || !generic.reference_equals_ground(&store.semantic, scope, parameter, store.semantic.to_type(container_type)?)? {
                    return Err(IrVerifyError::new("folded membership changes its original receiver parameter scope"));
                }
                slot
            }
            _ => return Err(IrVerifyError::new("folded membership changes its original receiver authority kind")),
        };
        if original.positive_payload[0] != slot { return Err(IrVerifyError::new("folded membership changes its original receiver slot")); }
        let owners = store.generic_instruction_owners()?;
        for (index, tag) in store.tags.iter().enumerate() {
            if owners[index] == Some(owner) && matches!(tag, FullTag::StmtAssign | FullTag::StmtAssignInt | FullTag::StmtAssignBool
                | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath)
                && store.payload(store.data[index].range())?.first() == Some(&slot) {
                return Err(IrVerifyError::new("folded membership receiver parameter has an unprepared assignment"));
            }
        }
        Ok(true)
    }

    pub(in crate::runtime::eval::indexed) fn verify_prepared_membership_contract(pools: &SemanticPools, operation: &PreparedOperation) -> Result<(), IrVerifyError> {
        let PreparedOperationAuthority::Language { authority, operation: PreparedLanguageOperation::Membership { domain, negated },
            argument_order: OperationArgumentOrder::ReceiverThenNeedle, statement_result_is_unit: false, .. } = operation.authority else {
            return Err(IrVerifyError::new("membership loses its selected language authority"));
        };
        let suffix = match domain { MembershipDomain::List => "List", MembershipDomain::Map => "Map", MembershipDomain::Str => "Str",
            MembershipDomain::Bytes => "Bytes", MembershipDomain::Record => "Record", MembershipDomain::Path { .. } => "Path", MembershipDomain::EnvPathList => "EnvPathList" };
        let expected_authority = format!("language.binary.{}.{suffix}", if negated { "NotIn" } else { "In" });
        if authority != expected_authority { return Err(IrVerifyError::new("membership changes its selected builtin authority")); }
        let lowering = operation.membership_lowering.as_ref().ok_or_else(|| IrVerifyError::new("membership lacks its original physical lowering"))?;
        let operand_count = if matches!(lowering, PreparedMembershipLowering::Folded(_)) { 0 } else { 2 };
        if operation.arguments.len() != 1 || operation.binding.supplied_slots.as_ref() != [0]
            || !operation.binding.default_slots.is_empty() || operation.binding.operands.len() != operand_count
            || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
            || operation.effects.creation != crate::sema::inference::EffectSet::EMPTY
            || !operation.effects.inputs.is_empty() || !operation.effects.outputs.is_empty()
            || operation.fallback_lowering.is_some() || operation.original_integer_addition.is_some()
            || operation.range_lowering.is_some() || operation.literal_comparison_slot.is_some() {
            return Err(IrVerifyError::new("membership changes its original receiver, argument or effect contract"));
        }
        let PreparedMembershipLowering::Binary(original) = lowering else { return Self::verify_prepared_folded_membership_contract(pools, operation); };
        if original.instruction_payload.len() != 4 { return Err(IrVerifyError::new("membership changes its original physical packet")); }
        let Some(TypeRef::Ground(container)) = operation.receiver else { return Err(IrVerifyError::new("membership container lacks its checked ground type")); };
        let Some(TypeRef::Ground(needle)) = operation.arguments[0] else { return Err(IrVerifyError::new("membership needle lacks its checked ground type")); };
        let TypeRef::Ground(result) = operation.result else { return Err(IrVerifyError::new("membership result lacks its checked ground type")); };
        let container_type = pools.to_type(container)?;
        let uint_key = matches!(&container_type, Type::Map(key, _) if **key == Type::UInt);
        let needle_type = pools.to_type(needle)?;
        let result_type = pools.to_type(result)?;
        if original.uint_key.is_some() != uint_key || original.env_getter.is_some() != (domain == MembershipDomain::EnvPathList) || result_type != Type::Bool || !membership_types(domain, &container_type, &needle_type) {
            return Err(IrVerifyError::new(format!("membership changes its selected container, needle or result relationship: domain {domain:?}, container {container_type:?}, needle {needle_type:?}, result {result_type:?}, UInt guard {}, environment getter {}", original.uint_key.is_some(), original.env_getter.is_some())));
        }
        Ok(())
    }

    pub(in crate::runtime::eval::indexed::full) fn verify_membership_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner,
        expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(operation) = generic.operation_at(instruction)? else { return Ok(false); };
        let PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Membership { negated, .. }, .. } = operation.authority else { return Ok(false); };
        Self::verify_prepared_membership_contract(&store.semantic, operation)?;
        let source = generic.operation_source(operation.source)?;
        let OperationSourceOrigin::Expression(expression) = source.origin else { return Err(IrVerifyError::new("membership loses its original expression")); };
        if *expected != Type::Bool || source.instruction != instruction || source.owner != owner
            || source.expected != operation.authority || source.identity != operation.authority.identity()
            || generic.registered_instruction_origin(instruction, false) != Some((source.origin, owner)) {
            return Err(IrVerifyError::new("membership changes its original operation, owner or result consumer"));
        }
        let lowering = operation.membership_lowering.as_ref().ok_or_else(|| IrVerifyError::new("membership lacks its original physical lowering"))?;
        let PreparedMembershipLowering::Binary(original) = lowering else { return Self::verify_folded_membership_operand(store, generic, operation, instruction, owner, expected); };
        if store.tags.get(instruction as usize) != Some(&FullTag::ExprBinary) { return Err(IrVerifyError::new("membership changes its original binary carrier")); }
        let words = store.payload(store.data[instruction as usize].range())?;
        let op = if negated { BinaryOp::NotIn } else { BinaryOp::In };
        if words.len() != 4 || words.first().and_then(|&index| store.binary_ops.get(index as usize)) != Some(&op)
            || words.get(1..3) != Some(operation.binding.operands.as_ref())
            || IrLocationId::from_raw(words[3]).and_then(|location| store.location_sources.get(location.index())) != Some(&expression.source) {
            return Err(IrVerifyError::new("membership changes its original operator, operands or source location"));
        }
        if words != original.instruction_payload.as_ref() || membership_span(store, words[3])? != original.span
            || generic.registered_instruction_origin(original.needle_instruction, false) != Some((OperationSourceOrigin::Expression(original.needle_origin), owner))
            || generic.registered_instruction_origin(words[2], false) != Some((OperationSourceOrigin::Expression(original.container_origin), owner)) {
            return Err(IrVerifyError::new("membership changes its original physical packet, location or operand identities"));
        }
        if let Some(guard) = &original.uint_key {
            if store.tags.get(words[1] as usize) != Some(&FullTag::ExprTry)
                || store.payload(store.data[words[1] as usize].range())? != guard.try_payload.as_ref()
                || guard.try_payload.as_ref() != [guard.require_instruction]
                || store.tags.get(guard.require_instruction as usize) != Some(&FullTag::ExprRequire)
                || store.payload(store.data[guard.require_instruction as usize].range())? != guard.require_payload.as_ref()
                || guard.require_payload.len() != 5 || guard.require_payload[0] != original.needle_instruction
                || guard.require_payload[3] != 0 || membership_span(store, guard.require_payload[4])? != guard.require_span
                || TypeId::from_raw(guard.require_payload[1]).and_then(|ty| store.semantic.to_type(ty).ok()) != Some(Type::UInt)
                || store.string(guard.require_payload[2]).ok() != Some("UInt") {
                return Err(IrVerifyError::new("membership changes its original unsigned Map key validation"));
            }
        } else if words[1] != original.needle_instruction {
            return Err(IrVerifyError::new("membership changes its original needle source"));
        }
        let Some(TypeRef::Ground(needle)) = operation.arguments[0] else { unreachable!() };
        let Some(TypeRef::Ground(container)) = operation.receiver else { unreachable!() };
        if let Some(getter) = &original.env_getter {
            if getter.instruction != words[2] || getter.source.origin != original.container_origin
                || getter.source.checked != original.container_root.ty || getter.source.span != getter.span
                || getter.source.creation != crate::sema::inference::EffectSet::ENV
                || store.tags.get(getter.instruction as usize) != Some(&FullTag::ExprModuleCall)
                || store.payload(store.data[getter.instruction as usize].range())? != getter.instruction_payload.as_ref() {
                return Err(IrVerifyError::new("membership changes its original environment getter source or allocation"));
            }
            let packet = &getter.instruction_payload;
            if packet.len() != 4 || packet[1] != 0 || store.runtime_ops.get(packet[0] as usize) != Some(&RuntimeOp::EnvPathList)
                || membership_span(store, packet[3])? != getter.span {
                return Err(IrVerifyError::new("membership changes its original environment getter transport or location"));
            }
            let arguments = IrBlockId::from_raw(packet[2]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("membership environment getter argument block is missing"))?;
            if arguments.flags != getter.arguments_flags || arguments.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST
                || store.payload(arguments.instructions)? != getter.arguments_payload.as_ref() || getter.arguments_payload.as_ref() != [0] {
                return Err(IrVerifyError::new("membership changes its original empty environment getter arguments"));
            }
        }
        // The candidate binds the container as receiver, while the authored
        // binary instruction evaluates the needle before the container.
        Self::verify_generic_source(store, generic, original.needle_instruction, owner, &store.semantic.to_type(needle)?, instance, active)?;
        if original.env_getter.is_none() {
            Self::verify_generic_source(store, generic, words[2], owner, &store.semantic.to_type(container)?, instance, active)?;
        }
        Ok(true)
    }
}

fn verify_folded_membership_literal(store: &FullStore, original: &PreparedFoldedMembershipLowering) -> Result<(), IrVerifyError> {
    let index = original.positive_payload.get(1).copied().ok_or_else(|| IrVerifyError::new("folded membership literal reference is missing"))? as usize;
    let matches = match (&original.literal, original.positive_tag) {
        (PreparedComparisonLiteral::Str(expected), FullTag::BoolStrContainsSlot) => original.literal_tag.is_none()
            && original.literal_payload.is_empty() && store.string(index as u32)? == expected.as_ref(),
        (PreparedComparisonLiteral::Int(expected), FullTag::BoolContainsSlot) => {
            let tag = store.values.get(index).copied().ok_or_else(|| IrVerifyError::new("folded membership literal allocation is missing"))?;
            let words = store.payload(store.value_data.get(index).ok_or_else(|| IrVerifyError::new("folded membership literal packet is missing"))?.range())?;
            tag == FullValueTag::Int && original.literal_tag == Some(tag) && words == original.literal_payload.as_ref()
                && matches!(words, [low, high] if ((*low as u64 | (*high as u64) << 32) as i64) == *expected)
        }
        _ => false,
    };
    if !matches { return Err(IrVerifyError::new("folded membership changes its original literal value or encoding")); }
    Ok(())
}

#[cfg(test)]
mod tests;
