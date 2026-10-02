use super::*;
mod scoped_methods;
pub(in crate::runtime::eval) use scoped_methods::{ScopedNativeMethodRequirement, ScopedNativeMethodSource, ScopedNativeMethodObligation, ScopedNativeMethodReceiver, ScopedNativeMethodWitness, ScopedNativeMethodSourceId, ScopedNativeMethodWitnessId};
use crate::sema::check::ExpressionIdentity;
use crate::sema::inference::InvocationDefaultTiming;
use std::mem::size_of;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct NativeCallableContract {
    pub authority: PreparedOperationAuthority,
    pub registry_owner: crate::sema::registry_graph::RegistryOwner,
    pub signature: SignatureId,
    pub kind: CallableKind,
    pub effects: PreparedOperationEffects,
    pub input_eligibility: Box<[(usize, crate::sema::inference::Eligibility)]>,
    pub argument_relations: Box<[crate::sema::inference::ArgumentRelation]>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct NativeCallableSource {
    pub origin: ExpressionIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub scope: Option<SchemeScopeId>,
}

/// A native value carries registry authority; it never acquires a user declaration.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedNativeCallableValue {
    pub source: NativeCallableSource,
    pub contract: NativeCallableContract,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct GroundNativeInvocationContract {
    pub callee_instruction: u32,
    pub callee_origin: ExpressionIdentity,
    pub callee_slot: Option<u32>,
    pub callable: NativeCallableValueId,
    pub call: GroundNativeCallContract,
    pub timing: InvocationDefaultTiming,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct NativeInvocationSource {
    pub origin: ExpressionIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub scope: Option<SchemeScopeId>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedNativeInvocationPlan {
    pub source: NativeInvocationSource,
    pub contract: GroundNativeInvocationContract,
}

#[derive(Clone, Debug, Default)]
pub(super) struct NativeCallableEvidence {
    values: Vec<Entry<Arc<PreparedNativeCallableValue>>>,
    original_values: Vec<Arc<PreparedNativeCallableValue>>,
    plans: Vec<Entry<Arc<PreparedNativeInvocationPlan>>>,
    original_plans: Vec<Arc<PreparedNativeInvocationPlan>>,
    methods: scoped_methods::ScopedNativeMethodEvidence,
    value_instructions: Vec<(u32, NativeCallableValueId)>,
    plan_instructions: Vec<(u32, NativeInvocationPlanId)>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct NativeCallableCheckpoint { values: usize, plans: usize, methods: scoped_methods::ScopedNativeMethodCheckpoint }

impl NativeCallableEvidence {
    pub(super) fn retained_bytes(&self) -> usize {
        self.methods.retained_bytes() + self.values.capacity() * size_of::<Entry<Arc<PreparedNativeCallableValue>>>()
            + self.original_values.capacity() * size_of::<Arc<PreparedNativeCallableValue>>()
            + self.plans.capacity() * size_of::<Entry<Arc<PreparedNativeInvocationPlan>>>()
            + self.original_plans.capacity() * size_of::<Arc<PreparedNativeInvocationPlan>>()
            + self.value_instructions.capacity() * size_of::<(u32, NativeCallableValueId)>()
            + self.plan_instructions.capacity() * size_of::<(u32, NativeInvocationPlanId)>()
            + self.values.iter().map(|entry| size_of::<PreparedNativeCallableValue>() + 2 * size_of::<usize>() + entry.value.contract.authority.retained_bytes() + effect_bytes(&entry.value.contract.effects)
                + entry.value.contract.input_eligibility.len() * size_of::<(usize, crate::sema::inference::Eligibility)>() + entry.value.contract.argument_relations.len() * size_of::<crate::sema::inference::ArgumentRelation>()).sum::<usize>()
            + self.plans.iter().map(|entry| size_of::<PreparedNativeInvocationPlan>() + 2 * size_of::<usize>() + entry.value.contract.call.retained_bytes()).sum::<usize>()
    }
    pub(super) fn shrink_to_fit(&mut self) {
        self.methods.shrink_to_fit();
        self.values.shrink_to_fit(); self.original_values.shrink_to_fit();
        self.plans.shrink_to_fit(); self.original_plans.shrink_to_fit();
        self.value_instructions.shrink_to_fit(); self.plan_instructions.shrink_to_fit();
    }
    pub(super) fn checkpoint(&self) -> NativeCallableCheckpoint { NativeCallableCheckpoint { values: self.values.len(), plans: self.plans.len(), methods: self.methods.checkpoint() } }
    pub(super) fn validate_checkpoint(&self, checkpoint: NativeCallableCheckpoint, serial: u64) -> Result<(), IrVerifyError> {
        self.methods.validate_checkpoint(checkpoint.methods, serial)?;
        if checkpoint.values > self.values.len() || checkpoint.plans > self.plans.len()
            || self.values.get(checkpoint.values.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial)
            || self.plans.get(checkpoint.plans.wrapping_sub(1)).is_some_and(|entry| entry.serial >= serial) { return Err(failure("native callable checkpoint references retired entries")); }
        Ok(())
    }
    pub(super) fn rewind_validated(&mut self, checkpoint: NativeCallableCheckpoint) {
        self.methods.rewind_validated(checkpoint.methods);
        self.values.truncate(checkpoint.values); self.original_values.truncate(checkpoint.values);
        self.plans.truncate(checkpoint.plans); self.original_plans.truncate(checkpoint.plans);
        self.value_instructions.clear(); self.plan_instructions.clear();
    }
    pub(super) fn finish_indexes(&mut self, root: u64) {
        self.methods.finish_indexes(root);
        self.value_instructions = self.values.iter().enumerate().map(|(index, entry)| (entry.value.source.instruction, NativeCallableValueId { index: index as u32, proof: OwnerProof { root, serial: entry.serial } })).collect();
        self.plan_instructions = self.plans.iter().enumerate().map(|(index, entry)| (entry.value.source.instruction, NativeInvocationPlanId { index: index as u32, proof: OwnerProof { root, serial: entry.serial } })).collect();
        self.value_instructions.sort_unstable_by_key(|entry| entry.0); self.plan_instructions.sort_unstable_by_key(|entry| entry.0);
    }
}

fn effect_bytes(effects: &PreparedOperationEffects) -> usize {
    effects.inputs.len() * size_of::<(crate::sema::inference::EffectRole, crate::sema::inference::EffectSet)>()
        + effects.outputs.len() * size_of::<(crate::sema::inference::ProducerRole, crate::sema::inference::EffectSet)>()
}

impl GenericEvidenceStore {
    pub fn native_callable_value(&self, id: NativeCallableValueId) -> Result<&PreparedNativeCallableValue, IrVerifyError> {
        let value = owned(self.root, &self.native_callables.values, id.index, id.proof)?;
        let original = self.native_callables.original_values.get(id.index as usize).ok_or_else(|| failure("original native callable value is missing"))?;
        if !Arc::ptr_eq(value, original) { return Err(failure("native callable value changes its original receipt")); }
        Ok(value)
    }
    pub fn native_invocation_plan(&self, id: NativeInvocationPlanId) -> Result<&PreparedNativeInvocationPlan, IrVerifyError> {
        let value = owned(self.root, &self.native_callables.plans, id.index, id.proof)?;
        let original = self.native_callables.original_plans.get(id.index as usize).ok_or_else(|| failure("original native invocation is missing"))?;
        if !Arc::ptr_eq(value, original) { return Err(failure("native invocation changes its original receipt")); }
        Ok(value)
    }
    pub fn native_callable_value_at(&self, instruction: u32) -> Result<Option<NativeCallableValueId>, IrVerifyError> {
        self.native_callables.value_instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| {
            let id = self.native_callables.value_instructions[index].1; self.native_callable_value(id)?; Ok(id)
        }).transpose()
    }
    pub fn native_invocation_plan_at(&self, instruction: u32) -> Result<Option<NativeInvocationPlanId>, IrVerifyError> {
        self.native_callables.plan_instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| {
            let id = self.native_callables.plan_instructions[index].1; self.native_invocation_plan(id)?; Ok(id)
        }).transpose()
    }
    pub fn native_callable_values(&self) -> impl Iterator<Item = (NativeCallableValueId, &PreparedNativeCallableValue)> {
        self.native_callables.values.iter().enumerate().map(|(index, entry)| (NativeCallableValueId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn native_invocation_plans(&self) -> impl Iterator<Item = (NativeInvocationPlanId, &PreparedNativeInvocationPlan)> {
        self.native_callables.plans.iter().enumerate().map(|(index, entry)| (NativeInvocationPlanId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn validate_native_invocation(&self, callable: NativeCallableValueId, plan: NativeInvocationPlanId) -> Result<(), IrVerifyError> {
        self.native_callable_value(callable)?;
        if self.native_invocation_plan(plan)?.contract.callable != callable { return Err(failure("native invocation belongs to another original callable value")); }
        Ok(())
    }
    pub(super) fn verify_native_callable_evidence(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        self.verify_scoped_native_method_evidence(pools, owners)?;
        if self.native_callables.values.len() != self.native_callables.original_values.len() || self.native_callables.plans.len() != self.native_callables.original_plans.len() { return Err(failure("native callable original ledger is incomplete")); }
        let source = |origin, instruction, owner, scope| -> Result<(), IrVerifyError> {
            if owners.get(instruction as usize) != Some(&Some(owner)) || self.registered_instruction_origin(instruction, false) != Some((OperationSourceOrigin::Expression(origin), owner)) { return Err(failure("native callable changes its original source or owner")); }
            if let Some(scope) = scope { if owner != InstructionOwner::Function(self.scope(scope)?.owner) { return Err(failure("native callable changes its lexical scope")); } }
            Ok(())
        };
        let mut values = Vec::new();
        for (id, _) in self.native_callable_values() {
            let value = self.native_callable_value(id)?;
            source(value.source.origin, value.source.instruction, value.source.owner, value.source.scope)?;
            verify_contract(pools, &value.contract)?;
            values.push((value.source.instruction, id));
        }
        let mut plans = Vec::new();
        for (id, _) in self.native_invocation_plans() {
            let plan = self.native_invocation_plan(id)?;
            source(plan.source.origin, plan.source.instruction, plan.source.owner, plan.source.scope)?;
            let contract = &plan.contract;
            let value = self.native_callable_value(contract.callable)?;
            let call = &contract.call;
            if contract.timing != InvocationDefaultTiming::AtCall || call.authority != value.contract.authority || call.registry_owner != value.contract.registry_owner
                || call.signature != value.contract.signature || call.kind != value.contract.kind || call.effects != value.contract.effects
                || call.argument_relations != value.contract.argument_relations || call.input_eligibility != value.contract.input_eligibility
                || call.binding.dynamic.is_some() || call.binding.rest_slot.is_some() || call.result != TypeRef::Ground(pools.signature_return_type(call.signature)?)
                || owners.get(contract.callee_instruction as usize) != Some(&Some(plan.source.owner))
                || self.registered_instruction_origin(contract.callee_instruction, false) != Some((OperationSourceOrigin::Expression(contract.callee_origin), plan.source.owner)) { return Err(failure("native invocation changes its original authority, callee, or selected signature")); }
            let count = pools.signature_param_count(call.signature)?;
            if count > 65536 || call.arguments.len() != call.binding.supplied_slots.len() || call.arguments.len() != call.binding.operands.len() || call.argument_sources.len() != count { return Err(failure("native invocation supplied shape is incomplete")); }
            let mut occupied = std::collections::BTreeSet::new();
            for (ordinal, ((argument, &slot), &instruction)) in call.arguments.iter().zip(&call.binding.supplied_slots).zip(&call.binding.operands).enumerate() {
                if slot as usize >= count || !occupied.insert(slot) || instruction != argument.instruction || call.argument_sources[slot as usize] != Some(instruction)
                    || owners.get(instruction as usize) != Some(&Some(plan.source.owner)) || !self.argument_has_original_source(instruction, plan.source.origin, ordinal, &argument.original, plan.source.owner, argument.ty) { return Err(failure("native invocation loses its original supplied operand")); }
                let TypeRef::Ground(actual) = argument.ty else { return Err(failure("native invocation operand requires scoped preparation")); };
                let (label, formal, _) = pools.signature_param(call.signature, slot as usize)?;
                if argument.original.name.is_some_and(|name| name != label) || pools.signature_parameter_rest(call.signature, slot as usize)?
                    || !native_parameter_accepts(value.contract.argument_relations.get(slot as usize).copied().unwrap_or(crate::sema::inference::ArgumentRelation::Assignable), &pools.to_type(formal)?, &pools.to_type(actual)?, false, 0)? { return Err(failure("native invocation operand changes its selected input contract")); }
            }
            for &slot in &call.binding.default_slots {
                if slot as usize >= count || !occupied.insert(slot) || call.argument_sources[slot as usize].is_some() || !pools.signature_parameter_defaulted(call.signature, slot as usize)? { return Err(failure("native invocation default mask changed")); }
            }
            if occupied.len() != count { return Err(failure("native invocation omits a required operand")); }
            plans.push((plan.source.instruction, id));
        }
        values.sort_unstable_by_key(|entry| entry.0); plans.sort_unstable_by_key(|entry| entry.0);
        if values.windows(2).any(|pair| pair[0].0 == pair[1].0) || plans.windows(2).any(|pair| pair[0].0 == pair[1].0)
            || values != self.native_callables.value_instructions || plans != self.native_callables.plan_instructions { return Err(failure("native callable instruction index is incomplete or ambiguous")); }
        Ok(())
    }
}

// Declared erasure is a property of the published native contract. The actual
// operand keeps its own descriptor and guard proof throughout transport.
pub(super) fn native_parameter_accepts(relation: crate::sema::inference::ArgumentRelation, formal: &crate::sema::types::Type, actual: &crate::sema::types::Type, invariant: bool, depth: usize) -> Result<bool, IrVerifyError> {
    use crate::sema::inference::ArgumentRelation;
    use crate::sema::types::Type;
    if depth > 256 { return Err(failure("native input contract exceeds structural depth")); }
    if relation == ArgumentRelation::Exact { return Ok(formal == actual); }
    if formal == &Type::Any { return Ok(true); }
    if relation != ArgumentRelation::DeclaredErasure { return Ok(parameter_accepts(formal, actual)); }
    Ok(match (formal, actual) {
        (Type::ErasedRecord, Type::Record(_) | Type::ErasedRecord) => true,
        (Type::List(a), Type::List(b)) | (Type::Stream(a), Type::Stream(b)) | (Type::Optional(a), Type::Optional(b)) => native_parameter_accepts(relation, a, b, true, depth + 1)?,
        (Type::Map(a, b), Type::Map(c, d)) | (Type::Result(a, b), Type::Result(c, d)) => native_parameter_accepts(relation, a, c, true, depth + 1)? && native_parameter_accepts(relation, b, d, true, depth + 1)?,
        (Type::Record(a), Type::Record(b)) => {
            if invariant && a.len() != b.len() { return Ok(false); }
            for (name, ty) in a { let Some(actual) = b.get(name) else { return Ok(false); }; if !native_parameter_accepts(relation, ty, actual, true, depth + 1)? { return Ok(false); } }
            true
        },
        _ if invariant => formal == actual,
        _ => parameter_accepts(formal, actual),
    })
}

fn verify_contract(pools: &SemanticPools, contract: &NativeCallableContract) -> Result<(), IrVerifyError> {
    if !matches!(contract.registry_owner, crate::sema::registry_graph::RegistryOwner::Module(_))
        || !matches!(contract.authority, PreparedOperationAuthority::Registry { binding: crate::modules::signature::ImplBinding::Native, semantic_rule: crate::modules::signature::SemanticRule::Standard, .. })
        || pools.signature_closed_effects(contract.signature)? != contract.effects.creation
        || contract.kind == CallableKind::Pure && contract.effects.creation != crate::sema::inference::EffectSet::EMPTY { return Err(failure("native callable original registry protocol is not prepared")); }
    let count = pools.signature_param_count(contract.signature)?;
    let mut inputs = std::collections::BTreeSet::new();
    let mut outputs = std::collections::BTreeSet::new();
    let mut guards = rustc_hash::FxHashSet::default();
    if count > 65536 || contract.argument_relations.len() > count || contract.input_eligibility.len() > 65536
        || contract.effects.inputs.len() > 65536 || contract.effects.outputs.len() > 65536 || contract.effects.creation.0 & !0x7f != 0
        || contract.effects.inputs.iter().any(|&(role, effects)| !inputs.insert(role) || effects.0 & !0x7f != 0)
        || contract.effects.outputs.iter().any(|&(role, effects)| !outputs.insert(role) || effects.0 & !0x7f != 0)
        || contract.input_eligibility.iter().any(|&(slot, predicate)| slot >= count || !guards.insert((slot, predicate))) { return Err(failure("native callable guard or effect payload is invalid")); }
    GenericEvidenceStore::verify_type(pools, pools.signature_return_type(contract.signature)?)?;
    for slot in 0..count { GenericEvidenceStore::verify_type(pools, pools.signature_param(contract.signature, slot)?.1)?; }
    Ok(())
}

impl GenericEvidenceBuilder {
    pub fn add_native_callable_value(&mut self, value: PreparedNativeCallableValue) -> Result<NativeCallableValueId, IrVerifyError> {
        if self.store.native_callables.values.len() >= 2_000_000 { return Err(failure("native callable value limit exceeded")); }
        let index = self.store.native_callables.values.len() as u32;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("native callable serial overflow"))?;
        let value = Arc::new(value);
        self.store.native_callables.original_values.push(Arc::clone(&value)); self.store.native_callables.values.push(Entry { serial, value });
        Ok(NativeCallableValueId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub fn add_native_invocation_plan(&mut self, value: PreparedNativeInvocationPlan) -> Result<NativeInvocationPlanId, IrVerifyError> {
        if self.store.native_callables.plans.len() >= 2_000_000 || value.contract.call.arguments.len() > 65536 { return Err(failure("native invocation limit exceeded")); }
        let index = self.store.native_callables.plans.len() as u32;
        let serial = self.next_serial; self.next_serial = serial.checked_add(1).ok_or_else(|| failure("native invocation serial overflow"))?;
        let value = Arc::new(value);
        self.store.native_callables.original_plans.push(Arc::clone(&value)); self.store.native_callables.plans.push(Entry { serial, value });
        Ok(NativeInvocationPlanId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub(in crate::runtime::eval) fn native_callable_value(&self, id: NativeCallableValueId) -> Result<&PreparedNativeCallableValue, IrVerifyError> { self.store.native_callable_value(id) }
}

#[cfg(test)]
impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn test_native_callable_value_mut(&mut self, id: NativeCallableValueId) -> Result<&mut PreparedNativeCallableValue, IrVerifyError> {
        owned(self.root, &self.native_callables.values, id.index, id.proof)?;
        Ok(Arc::make_mut(&mut self.native_callables.values[id.index as usize].value))
    }
    pub(in crate::runtime::eval) fn test_native_invocation_plan_mut(&mut self, id: NativeInvocationPlanId) -> Result<&mut PreparedNativeInvocationPlan, IrVerifyError> {
        owned(self.root, &self.native_callables.plans, id.index, id.proof)?;
        Ok(Arc::make_mut(&mut self.native_callables.plans[id.index as usize].value))
    }
    pub(in crate::runtime::eval) fn test_remove_native_invocation_plans(&mut self) { self.native_callables.plans.clear(); }
}

#[cfg(test)]
mod declared_record_erasure_tests {
    use super::*;
    use crate::sema::inference::ArgumentRelation;
    use crate::sema::types::Type;

    #[test]
    fn canonical_record_erasure_admits_checked_records_without_erasing_their_fields() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let actual = Type::Record(std::collections::BTreeMap::from([(Name::intern("jobs"), Type::Int)]));
            assert!(native_parameter_accepts(ArgumentRelation::DeclaredErasure, &Type::ErasedRecord, &actual, false, 0).unwrap());
            assert!(native_parameter_accepts(ArgumentRelation::DeclaredErasure, &Type::ErasedRecord, &Type::ErasedRecord, false, 0).unwrap());
        });
    }

    #[test]
    fn canonical_record_erasure_refuses_exact_concrete_records_and_dynamic_laundering() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let actual = Type::Record(std::collections::BTreeMap::from([(Name::intern("jobs"), Type::Int)]));
            assert!(!native_parameter_accepts(ArgumentRelation::Exact, &Type::ErasedRecord, &actual, false, 0).unwrap());
            assert!(!native_parameter_accepts(ArgumentRelation::Assignable, &Type::ErasedRecord, &actual, false, 0).unwrap());
            for relation in [ArgumentRelation::Exact, ArgumentRelation::Assignable, ArgumentRelation::DeclaredErasure] {
                assert!(!native_parameter_accepts(relation, &Type::ErasedRecord, &Type::Any, false, 0).unwrap());
                assert!(!native_parameter_accepts(relation, &Type::ErasedRecord, &Type::Str, false, 0).unwrap());
            }
        });
    }
}
