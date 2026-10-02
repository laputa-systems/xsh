use super::*;
use crate::sema::check::{BindingIdentity, DeclarationIdentity, ExpressionIdentity};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct NativeScalarSourceId { index: u32, proof: OwnerProof }

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum NativeScalarReceiver {
    Parameter { declaration: DeclarationIdentity, signature: SignatureId, name: Name, slot: u32 },
    ScopedParameter { scope: SchemeScopeId, name: Name, slot: u32 },
    Binding { binding: BindingIdentity, application: ValueBindingId, slot: u32 },
    ByteLengthExpression { instruction: u32 },
    MethodExpression { instruction: u32, name: Name },
}

/// A folded lookup preserves the selected nullable call and its literal alternative.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct ByteAtFallbackComposite {
    pub call_origin: ExpressionIdentity,
    pub index_origin: ExpressionIdentity,
    pub index_instruction: u32,
    pub index_type: GroundTypeId,
    pub fallback_origin: ExpressionIdentity,
    pub fallback_type: GroundTypeId,
    pub fallback_value: i64,
    pub fallback_instruction: Option<u32>,
    pub fallback_authority: PreparedOperationAuthority,
    pub fallback_signature: SignatureId,
    pub result: GroundTypeId,
}

/// A native scalar retains its authored receiver and selected operation.
/// Folded instructions keep the receiver's authority when its read becomes a slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct NativeScalarSource {
    pub origin: ExpressionIdentity,
    pub receiver_origin: ExpressionIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub receiver: NativeScalarReceiver,
    pub receiver_type: GroundTypeId,
    pub contract: GroundNativeCallContract,
    pub byte_at_fallback: Option<Box<ByteAtFallbackComposite>>,
    pub payload: Box<[u32]>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct NativeScalarEvidence {
    sources: Vec<Entry<Arc<NativeScalarSource>>>,
    originals: Vec<Arc<NativeScalarSource>>,
    instructions: Vec<(u32, usize)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct NativeScalarCheckpoint { sources: usize }

impl NativeScalarEvidence {
    pub(super) fn checkpoint(&self) -> NativeScalarCheckpoint { NativeScalarCheckpoint { sources: self.sources.len() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: NativeScalarCheckpoint, serial_limit: u64) -> Result<(), IrVerifyError> {
        if checkpoint.sources > self.sources.len() || self.sources.get(checkpoint.sources.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial_limit) {
            return Err(failure("native scalar checkpoint references retired or replaced entries"));
        }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: NativeScalarCheckpoint) { self.sources.truncate(checkpoint.sources); self.originals.truncate(checkpoint.sources); self.instructions.clear(); }
    pub(super) fn finish(&mut self) {
        self.instructions = self.sources.iter().enumerate().map(|(index, entry)| (entry.value.instruction, index)).collect();
        self.instructions.sort_unstable_by_key(|entry| entry.0);
    }
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.sources.capacity() * size_of::<Entry<Arc<NativeScalarSource>>>() + self.originals.capacity() * size_of::<Arc<NativeScalarSource>>()
            + self.sources.len() * (size_of::<NativeScalarSource>() + 2 * size_of::<usize>())
            + self.sources.iter().map(|entry| entry.value.contract.retained_bytes() + entry.value.payload.len() * size_of::<u32>() + entry.value.byte_at_fallback.as_ref().map_or(0, |_| size_of::<ByteAtFallbackComposite>())).sum::<usize>()
            + self.instructions.capacity() * size_of::<(u32, usize)>()
    }
    pub(super) fn shrink_to_fit(&mut self) { self.sources.shrink_to_fit(); self.originals.shrink_to_fit(); self.instructions.shrink_to_fit(); }
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn verify_byte_at_fallback_contract(pools: &SemanticPools, source: &NativeScalarSource, composite: &ByteAtFallbackComposite) -> Result<(), IrVerifyError> {
        use crate::sema::types::Type;
        let contract = &source.contract;
        if !matches!(contract.authority, PreparedOperationAuthority::Registry { operation: crate::modules::signature::RuntimeOp::TextByteAt, binding: crate::modules::signature::ImplBinding::Native, semantic_rule: crate::modules::signature::SemanticRule::Standard, .. })
            || contract.registry_owner != crate::sema::registry_graph::RegistryOwner::Method(crate::modules::signature::MethodReceiver::Str) || contract.kind != CallableKind::Pure
            || contract.receiver.is_some() || contract.cli_descriptor.is_some() || contract.process_command_argv.is_some() || contract.effects.creation != crate::sema::inference::EffectSet::EMPTY
            || !contract.effects.inputs.is_empty() || !contract.effects.outputs.is_empty() || pools.signature_closed_effects(contract.signature)? != contract.effects.creation
            || pools.signature_param_count(contract.signature)? != 2 || pools.to_type(pools.signature_param(contract.signature, 0)?.1)? != Type::Str
            || pools.to_type(pools.signature_param(contract.signature, 1)?.1)? != Type::Int || pools.to_type(source.receiver_type)? != Type::Str
            || contract.result != TypeRef::Ground(pools.signature_return_type(contract.signature)?) || pools.to_type(pools.signature_return_type(contract.signature)?)? != Type::Optional(Box::new(Type::Int))
            || contract.arguments.len() != 1 || contract.arguments[0].instruction != composite.index_instruction || contract.arguments[0].ty != TypeRef::Ground(composite.index_type)
            || !matches!(contract.arguments[0].original.value, crate::sema::arguments::ArgumentValueSource::Expression(expression) if expression == composite.index_origin.expression)
            || contract.argument_sources.as_ref() != [Some(composite.index_instruction)] || contract.binding.supplied_slots.as_ref() != [0] || !contract.binding.default_slots.is_empty()
            || contract.binding.rest_slot.is_some() || contract.binding.dynamic.is_some() || contract.binding.operands.as_ref() != [composite.index_instruction]
            || pools.to_type(composite.index_type)? != Type::Int || pools.to_type(composite.fallback_type)? != Type::Int || pools.to_type(composite.result)? != Type::Int {
            return Err(failure("folded byte lookup changes its original native call contract"));
        }
        if !matches!(composite.fallback_authority, PreparedOperationAuthority::Language { operation: crate::sema::operation_graph::PreparedLanguageOperation::Fallback { result: false }, argument_order: crate::sema::operation_graph::OperationArgumentOrder::SourceOrder, statement_result_is_unit: false, .. })
            || pools.signature_param_count(composite.fallback_signature)? != 2 || pools.to_type(pools.signature_param(composite.fallback_signature, 0)?.1)? != Type::Optional(Box::new(Type::Int))
            || pools.to_type(pools.signature_param(composite.fallback_signature, 1)?.1)? != Type::Int || pools.to_type(pools.signature_return_type(composite.fallback_signature)?)? != Type::Int
            || pools.signature_closed_effects(composite.fallback_signature)? != crate::sema::inference::EffectSet::EMPTY {
            return Err(failure("folded byte lookup changes its original optional fallback contract"));
        }
        Ok(())
    }

    pub fn has_native_scalars(&self) -> bool { !self.native_scalars.sources.is_empty() || !self.native_scalars.originals.is_empty() }
    pub fn native_scalar_source(&self, id: NativeScalarSourceId) -> Result<&NativeScalarSource, IrVerifyError> {
        let value = owned(self.root, &self.native_scalars.sources, id.index, id.proof)?;
        if !self.native_scalars.originals.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(value, original)) { return Err(failure("native scalar differs from its original receipt")); }
        Ok(value.as_ref())
    }
    pub fn native_scalar_sources(&self) -> impl Iterator<Item = (NativeScalarSourceId, &NativeScalarSource)> {
        self.native_scalars.sources.iter().enumerate().map(|(index, entry)| (NativeScalarSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn native_scalar_at(&self, instruction: u32) -> Result<Option<NativeScalarSourceId>, IrVerifyError> {
        let Some(index) = self.native_scalars.instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| self.native_scalars.instructions[index].1) else { return Ok(None); };
        let entry = self.native_scalars.sources.get(index).ok_or_else(|| failure("native scalar instruction index is stale"))?;
        let id = NativeScalarSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } };
        self.native_scalar_source(id)?; Ok(Some(id))
    }
    pub(super) fn verify_native_scalar_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        if self.native_scalars.sources.len() != self.native_scalars.originals.len() { return Err(failure("native scalar original ledger is incomplete")); }
        let mut instructions = Vec::new();
        for (id, _) in self.native_scalar_sources() {
            let source = self.native_scalar_source(id)?;
            if owners.get(source.instruction as usize) != Some(&Some(source.owner)) || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), source.owner))
                || source.receiver_origin.source != source.origin.source || source.receiver_origin.namespace != source.origin.namespace {
                return Err(failure("native scalar changes its original owner or receiver source"));
            }
            let contract = &source.contract;
            if let Some(composite) = &source.byte_at_fallback {
                Self::verify_byte_at_fallback_contract(pools, source, composite)?;
                if owners.get(composite.index_instruction as usize) != Some(&Some(source.owner)) || self.registered_instruction_origin(composite.index_instruction, false) != Some((OperationSourceOrigin::Expression(composite.index_origin), source.owner)) {
                    return Err(failure("folded byte lookup changes its original index source"));
                }
                if [composite.call_origin, composite.index_origin, composite.fallback_origin].into_iter().any(|origin| origin.source != source.origin.source || origin.namespace != source.origin.namespace) {
                    return Err(failure("folded byte lookup crosses original source domains"));
                }
                if let Some(instruction) = composite.fallback_instruction {
                    if owners.get(instruction as usize) != Some(&Some(source.owner)) || self.registered_instruction_origin(instruction, false).is_some() { return Err(failure("folded byte lookup changes its original literal material")); }
                }
            } else {
            if !matches!(contract.authority, PreparedOperationAuthority::Registry { operation: crate::modules::signature::RuntimeOp::TextByteLen | crate::modules::signature::RuntimeOp::TextCountLines | crate::modules::signature::RuntimeOp::TextCountWords | crate::modules::signature::RuntimeOp::TextCountChars, binding: crate::modules::signature::ImplBinding::Native, semantic_rule: crate::modules::signature::SemanticRule::Standard, .. })
                || contract.registry_owner != crate::sema::registry_graph::RegistryOwner::Method(crate::modules::signature::MethodReceiver::Str)
                || contract.kind != CallableKind::Pure || contract.receiver.is_some() || contract.cli_descriptor.is_some() || contract.process_command_argv.is_some()
                || contract.effects.creation != crate::sema::inference::EffectSet::EMPTY || pools.signature_closed_effects(contract.signature)? != contract.effects.creation
                || !contract.arguments.is_empty() || !contract.argument_sources.is_empty() || !contract.binding.supplied_slots.is_empty() || !contract.binding.default_slots.is_empty()
                || contract.binding.rest_slot.is_some() || contract.binding.dynamic.is_some() || !contract.binding.operands.is_empty()
                || pools.signature_param_count(contract.signature)? != 1 || pools.to_type(pools.signature_param(contract.signature, 0)?.1)? != crate::sema::types::Type::Str
                || pools.to_type(source.receiver_type)? != crate::sema::types::Type::Str || contract.result != TypeRef::Ground(pools.signature_return_type(contract.signature)?)
                || pools.to_type(pools.signature_return_type(contract.signature)?)? != crate::sema::types::Type::Int {
                return Err(failure("native scalar changes its original selected contract"));
            }
            }
            match source.receiver {
                NativeScalarReceiver::Parameter { declaration, signature, name, slot } => {
                    let function = self.checked_function(declaration)?;
                    let (label, ty, _) = pools.signature_param(signature, slot as usize)?;
                    if source.owner != InstructionOwner::Function(function.target) || function.signature != signature || label != name || ty != source.receiver_type {
                        return Err(failure("native scalar receiver changes its original parameter"));
                    }
                }
                NativeScalarReceiver::ScopedParameter { scope, name, slot } => {
                    let scope = self.scope(scope)?;
                    if source.owner != InstructionOwner::Function(scope.owner) || scope.parameters.get(slot as usize) != Some(&TypeRef::Ground(source.receiver_type)) || scope.parameter_names.get(slot as usize) != Some(&name) {
                        return Err(failure("native scalar receiver changes its original scoped parameter"));
                    }
                }
                NativeScalarReceiver::Binding { binding, application, slot } => {
                    let application = self.value_binding(application)?;
                    if self.value_binding_source(application.source)?.binding.named() != Some(binding) || application.contract.owner != source.owner || application.contract.slot != slot || application.contract.binding_type != source.receiver_type {
                        return Err(failure("native scalar receiver changes its original immutable binding"));
                    }
                }
                NativeScalarReceiver::ByteLengthExpression { instruction } | NativeScalarReceiver::MethodExpression { instruction, .. } => {
                    if owners.get(instruction as usize) != Some(&Some(source.owner))
                        || self.registered_instruction_origin(instruction, false) != Some((OperationSourceOrigin::Expression(source.receiver_origin), source.owner)) {
                        return Err(failure("native scalar changes its original material receiver expression"));
                    }
                    if matches!(source.receiver, NativeScalarReceiver::ByteLengthExpression { .. }) && !matches!(contract.authority, PreparedOperationAuthority::Registry { operation: crate::modules::signature::RuntimeOp::TextByteLen, .. }) {
                        return Err(failure("native scalar material byte length changes its selected operation"));
                    }
                }
            }
            instructions.push((source.instruction, id.index as usize));
        }
        instructions.sort_unstable_by_key(|entry| entry.0);
        if instructions.windows(2).any(|pair| pair[0].0 == pair[1].0) || instructions != self.native_scalars.instructions { return Err(failure("native scalar instruction index is ambiguous or stale")); }
        Ok(())
    }
}

impl GenericEvidenceBuilder {
    pub fn add_native_scalar_source(&mut self, value: NativeScalarSource) -> Result<NativeScalarSourceId, IrVerifyError> {
        if self.store.native_scalars.sources.len() >= 2_000_000 { return Err(failure("native scalar evidence exceeds its work limit")); }
        let index = self.store.native_scalars.sources.len() as u32;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("native scalar serial overflow"))?;
        let value = Arc::new(value); self.store.native_scalars.originals.push(Arc::clone(&value)); self.store.native_scalars.sources.push(Entry { serial, value });
        Ok(NativeScalarSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub(in crate::runtime::eval) fn native_scalar_binding_sources(&self) -> impl Iterator<Item = Result<(BindingIdentity, ValueBindingId), IrVerifyError>> {
        self.store.value_bindings().filter_map(|(id, application)| match self.store.value_binding_source(application.source) {
            Ok(source) => source.binding.named().map(|binding| Ok((binding, id))),
            Err(error) => Some(Err(error)),
        })
    }
}

#[cfg(test)]
impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn test_native_scalar_mut(&mut self, id: NativeScalarSourceId) -> Result<&mut NativeScalarSource, IrVerifyError> {
        self.native_scalar_source(id)?; Ok(Arc::make_mut(&mut self.native_scalars.sources[id.index as usize].value))
    }
    pub(in crate::runtime::eval) fn test_remove_native_scalars(&mut self) { self.native_scalars.sources.clear(); self.native_scalars.instructions.clear(); }
}
