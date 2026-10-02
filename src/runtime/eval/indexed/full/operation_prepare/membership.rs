use super::*;
use crate::sema::operation_graph::{MembershipDomain, OperationArgumentOrder};

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedMembershipLowering {
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
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedMembershipUIntKey {
    pub try_payload: Box<[u32]>,
    pub require_instruction: u32,
    pub require_payload: Box<[u32]>,
    pub require_span: Span,
}

impl PreparedMembershipLowering {
    pub(in crate::runtime::eval) fn retained_bytes(&self) -> usize {
        self.instruction_payload.len() * std::mem::size_of::<u32>()
            + self.uint_key.as_ref().map_or(0, |guard| (guard.try_payload.len() + guard.require_payload.len()) * std::mem::size_of::<u32>())
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
    pub(super) fn prepare_membership_lowering(&self, instruction: u32, expression: crate::sema::check::ExpressionIdentity,
        owner: InstructionOwner, operation: &crate::sema::check::SolvedOperation) -> Result<PreparedMembershipLowering, IrBuildError> {
        let solved = self.solved.as_ref().ok_or_else(|| unprepared("membership_original_graph_missing"))?;
        let graph = &solved.graph;
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
        Ok(PreparedMembershipLowering { instruction_payload: words.to_vec().into_boxed_slice(), span, needle_instruction, needle_origin,
            container_origin, requirement, receiver_root, argument_root, needle_root, container_root, result_root, operation_result_root, uint_key })
    }
}

impl FullVerifier {
    pub(in crate::runtime::eval::indexed) fn verify_prepared_membership_contract(pools: &SemanticPools, operation: &PreparedOperation) -> Result<(), IrVerifyError> {
        let PreparedOperationAuthority::Language { authority, operation: PreparedLanguageOperation::Membership { domain, negated },
            argument_order: OperationArgumentOrder::ReceiverThenNeedle, statement_result_is_unit: false, .. } = operation.authority else {
            return Err(IrVerifyError::new("membership loses its selected language authority"));
        };
        let suffix = match domain { MembershipDomain::List => "List", MembershipDomain::Map => "Map", MembershipDomain::Str => "Str",
            MembershipDomain::Bytes => "Bytes", MembershipDomain::Record => "Record", MembershipDomain::Path { .. } => "Path", MembershipDomain::EnvPathList => "EnvPathList" };
        let expected_authority = format!("language.binary.{}.{suffix}", if negated { "NotIn" } else { "In" });
        if authority != expected_authority { return Err(IrVerifyError::new("membership changes its selected builtin authority")); }
        if operation.arguments.len() != 1 || operation.binding.supplied_slots.as_ref() != [0]
            || !operation.binding.default_slots.is_empty() || operation.binding.operands.len() != 2
            || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
            || operation.effects.creation != crate::sema::inference::EffectSet::EMPTY
            || !operation.effects.inputs.is_empty() || !operation.effects.outputs.is_empty()
            || operation.fallback_lowering.is_some() || operation.original_integer_addition.is_some()
            || operation.range_lowering.is_some() || operation.literal_comparison_slot.is_some() {
            return Err(IrVerifyError::new("membership changes its original receiver, argument or effect contract"));
        }
        let original = operation.membership_lowering.as_ref().ok_or_else(|| IrVerifyError::new("membership lacks its original physical lowering"))?;
        if original.instruction_payload.len() != 4 { return Err(IrVerifyError::new("membership changes its original physical packet")); }
        let Some(TypeRef::Ground(container)) = operation.receiver else { return Err(IrVerifyError::new("membership container lacks its checked ground type")); };
        let Some(TypeRef::Ground(needle)) = operation.arguments[0] else { return Err(IrVerifyError::new("membership needle lacks its checked ground type")); };
        let TypeRef::Ground(result) = operation.result else { return Err(IrVerifyError::new("membership result lacks its checked ground type")); };
        let container_type = pools.to_type(container)?;
        let uint_key = matches!(&container_type, Type::Map(key, _) if **key == Type::UInt);
        if original.uint_key.is_some() != uint_key || pools.to_type(result)? != Type::Bool || !membership_types(domain, &container_type, &pools.to_type(needle)?) {
            return Err(IrVerifyError::new("membership changes its selected container, needle or result relationship"));
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
            || generic.registered_instruction_origin(instruction, false) != Some((source.origin, owner))
            || store.tags.get(instruction as usize) != Some(&FullTag::ExprBinary) {
            return Err(IrVerifyError::new("membership changes its original operation, owner or result consumer"));
        }
        let original = operation.membership_lowering.as_ref().ok_or_else(|| IrVerifyError::new("membership lacks its original physical lowering"))?;
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
        // The candidate binds the container as receiver, while the authored
        // binary instruction evaluates the needle before the container.
        for (operand, ty) in [(original.needle_instruction, needle), (words[2], container)] {
            Self::verify_generic_source(store, generic, operand, owner, &store.semantic.to_type(ty)?, instance, active)?;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
