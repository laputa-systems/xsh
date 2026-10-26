use super::schemes::{FullBinderRanges, FullInstantiation, FullRequirementRanges, SchemeTypeId};
use super::{FullBuilder, IrBuildError, IrRange, IR_NONE};
use crate::sema::guarded_requirements::{GuardBinding, GuardRequirements};
use crate::sema::schemes::{SchemeEffectParameterId, SchemeRowParameterId, SchemeTypeParameterId};
use bbtree::unsync::BBTreeMap as BTreeMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FullGuardScope {
    pub(super) id: u32,
    pub(super) parent: u32,
    pub(super) binding_ordinal: u32,
    pub(super) binding_starts: [u32; 3],
    pub(super) source: SchemeTypeId,
    pub(super) view: SchemeTypeId,
    pub(super) source_instruction: u32,
    pub(super) comparison_instruction: u32,
    pub(super) reads: IrRange,
    pub(super) type_parameters: IrRange,
    pub(super) row_parameters: IrRange,
    pub(super) effect_parameters: IrRange,
    pub(super) effect_predicates: IrRange,
    pub(super) applications: IrRange,
    pub(super) operations: IrRange,
    pub(super) methods: IrRange,
    pub(super) wrappers: IrRange,
    pub(super) guard_calls: IrRange,
    pub(super) effects: Option<FullGuardEffects>,
    pub(super) output: Option<FullGuardOutput>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FullGuardOutput {
    pub(super) outward: SchemeTypeId,
    pub(super) present: SchemeTypeId,
    pub(super) absent: SchemeTypeId,
    pub(super) instruction: u32,
    pub(super) nonnull_when: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FullGuardEffects {
    pub(super) outward: super::schemes::FullEffectContract,
    pub(super) local: super::schemes::FullEffectContract,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FullGuardCallTarget { Scope(u32), Call(u32) }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FullGuardCall {
    pub(super) instruction: u32,
    pub(super) function: u32,
    pub(super) leaf: u32,
    pub(super) role: crate::sema::schemes::InstantiationRole,
    pub(super) target: FullGuardCallTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FullGuardRead {
    pub(super) source: SchemeTypeId,
    pub(super) view: SchemeTypeId,
    pub(super) instruction: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FullGuardBinding {
    NonNull(FullInstantiation),
    Optional(FullInstantiation),
    Null,
    Call(u32),
}

impl FullBuilder {
    pub(super) fn extend_guard_parameter_maps(
        &self,
        guards: &[Arc<GuardRequirements>],
        types: &mut BTreeMap<SchemeTypeParameterId, u32>,
        rows: &mut BTreeMap<SchemeRowParameterId, u32>,
        effects: &mut BTreeMap<SchemeEffectParameterId, u32>,
    ) -> Result<(), IrBuildError> {
        let mut pending = guards.iter().rev().collect::<Vec<_>>();
        while let Some(guard) = pending.pop() {
            for id in &guard.type_parameters {
                let binder = u32::try_from(types.len()).map_err(|_| IrBuildError::format("guard_type_binder_overflow", None, 0, types.len()))?;
                if types.insert(*id, binder).is_some() { return Err(IrBuildError::format("guard_duplicate_type_binder", None, 0, guard.scope.index())); }
            }
            for parameter in &guard.row_parameters {
                let binder = u32::try_from(rows.len()).map_err(|_| IrBuildError::format("guard_row_binder_overflow", None, 0, rows.len()))?;
                if rows.insert(parameter.id, binder).is_some() { return Err(IrBuildError::format("guard_duplicate_row_binder", None, 0, guard.scope.index())); }
            }
            for id in &guard.requirements.effect_parameters {
                let binder = u32::try_from(effects.len()).map_err(|_| IrBuildError::format("guard_effect_binder_overflow", None, 0, effects.len()))?;
                if effects.insert(*id, binder).is_some() { return Err(IrBuildError::format("guard_duplicate_effect_binder", None, 0, guard.scope.index())); }
            }
            pending.extend(guard.requirements.guards.iter().rev());
        }
        Ok(())
    }

    pub(super) fn intern_guard_scopes(
        &mut self,
        guards: &[Arc<GuardRequirements>],
        types: &BTreeMap<SchemeTypeParameterId, u32>,
        rows: &BTreeMap<SchemeRowParameterId, u32>,
        effects: &BTreeMap<SchemeEffectParameterId, u32>,
        mut binding_starts: [usize; 3],
    ) -> Result<IrRange, IrBuildError> {
        let mut encoded = Vec::new();
        let mut pending = guards.iter().enumerate().rev().map(|(ordinal, guard)| (IR_NONE, ordinal, guard)).collect::<Vec<_>>();
        while let Some((parent, binding_ordinal, guard)) = pending.pop() {
            if guard.scope.index() != encoded.len() { return Err(IrBuildError::format("guard_scope_order", None, 0, guard.scope.index())); }
            let FullBinderRanges { types: type_parameters, rows: row_parameters, effects: effect_parameters } = self.intern_scheme_binders(&guard.type_parameters, &guard.row_parameters, &guard.requirements, types, rows, effects)?;
            let source = self.intern_scheme_type(&guard.source, types, rows, effects)?;
            let view = self.intern_scheme_type(&guard.view, types, rows, effects)?;
            let mut reads = Vec::with_capacity(guard.reads.len());
            for read in &guard.reads {
                reads.push(FullGuardRead { source: self.intern_scheme_type(&read.source, types, rows, effects)?, view: self.intern_scheme_type(&read.view, types, rows, effects)?, instruction: IR_NONE });
            }
            let start = self.store.scheme_guard_reads.len();
            self.store.scheme_guard_reads.extend(reads);
            let reads = super::table_range(start, self.store.scheme_guard_reads.len())?;
            let FullRequirementRanges { effect_predicates, applications, operations, methods, wrappers, guard_calls } = self.intern_requirement_tables(&guard.requirements, types, rows, effects)?;
            let effect_exports = guard.effects.as_ref().map(|exports| Ok(FullGuardEffects { outward: self.intern_effect_contract(&exports.outward, effects)?, local: self.intern_effect_contract(&exports.local, effects)? })).transpose()?;
            let output = guard.output.as_ref().map(|output| Ok(FullGuardOutput {
                outward: self.intern_scheme_type(&output.outward, types, rows, effects)?,
                present: self.intern_scheme_type(&output.present, types, rows, effects)?,
                absent: self.intern_scheme_type(&output.absent, types, rows, effects)?,
                instruction: IR_NONE, nonnull_when: output.nonnull_when,
            })).transpose()?;
            encoded.push(FullGuardScope { id: guard.scope.raw(), parent, binding_ordinal: u32::try_from(binding_ordinal).map_err(|_| IrBuildError::format("guard_binding_ordinal", None, 0, binding_ordinal))?, binding_starts: [u32::try_from(binding_starts[0]).map_err(|_| IrBuildError::format("guard_type_ordinal", None, 0, binding_starts[0]))?, u32::try_from(binding_starts[1]).map_err(|_| IrBuildError::format("guard_row_ordinal", None, 0, binding_starts[1]))?, u32::try_from(binding_starts[2]).map_err(|_| IrBuildError::format("guard_effect_ordinal", None, 0, binding_starts[2]))?], source, view, source_instruction: IR_NONE, comparison_instruction: IR_NONE, reads, type_parameters, row_parameters, effect_parameters, effect_predicates, applications, operations, methods, wrappers, guard_calls, effects: effect_exports, output });
            binding_starts[0] += guard.type_parameters.len();
            binding_starts[1] += guard.row_parameters.len();
            binding_starts[2] += guard.requirements.effect_parameters.len();
            pending.extend(guard.requirements.guards.iter().enumerate().rev().map(|(ordinal, child)| (guard.scope.raw(), ordinal, child)));
        }
        let start = self.store.scheme_guards.len();
        self.store.scheme_guards.extend(encoded);
        super::table_range(start, self.store.scheme_guards.len())
    }

    fn guard_row(&self, scope: crate::sema::guarded_requirements::GuardScopeId) -> Result<usize, IrBuildError> {
        let owner = self.active_scheme.ok_or_else(|| IrBuildError::format("guard_scope_owner", None, 0, scope.index()))?;
        let root = self.store.schemes.get(owner.index()).ok_or_else(|| IrBuildError::format("guard_scope_owner", None, 0, scope.index()))?;
        let range = root.guards.bounds(self.store.scheme_guards.len()).ok_or_else(|| IrBuildError::format("guard_scope_range", None, 0, scope.index()))?;
        let index = range.start.checked_add(scope.index()).filter(|index| range.contains(index)).ok_or_else(|| IrBuildError::format("guard_scope_id", None, 0, scope.index()))?;
        if self.store.scheme_guards[index].id != scope.raw() { return Err(IrBuildError::format("guard_scope_id", None, 0, scope.index())); }
        Ok(index)
    }

    pub(super) fn active_requirement_ranges(&self, scope: Option<crate::sema::guarded_requirements::GuardScopeId>) -> Result<FullRequirementRanges, IrBuildError> {
        if let Some(scope) = scope {
            let guard = self.store.scheme_guards[self.guard_row(scope)?];
            return Ok(FullRequirementRanges { effect_predicates: guard.effect_predicates, applications: guard.applications, operations: guard.operations, methods: guard.methods, wrappers: guard.wrappers, guard_calls: guard.guard_calls });
        }
        let owner = self.active_scheme.ok_or_else(|| IrBuildError::format("requirement_pack_owner", None, 0, 0))?;
        let root = self.store.schemes.get(owner.index()).ok_or_else(|| IrBuildError::format("requirement_pack_owner", None, 0, owner.index()))?;
        Ok(FullRequirementRanges { effect_predicates: root.effect_predicates, applications: root.applications, operations: root.operations, methods: root.methods, wrappers: root.wrappers, guard_calls: root.guard_calls })
    }

    pub(super) fn record_guard_membership(&mut self, scope: crate::sema::guarded_requirements::GuardScopeId, instruction: usize) -> Result<(), IrBuildError> {
        self.guard_row(scope)?;
        let function = self.current_owner.and_then(super::IrFunctionId::from_raw).ok_or_else(|| IrBuildError::format("guard_membership_owner", None, 0, instruction))?;
        let instruction = u32::try_from(instruction).map_err(|_| IrBuildError::format("guard_membership_instruction", None, 0, instruction))?;
        self.store.scheme_body_facts.push(super::schemes::FullSchemeBodyFact { instruction, function, value: super::schemes::FullSchemeBodyFactValue::GuardScope(scope.raw()) });
        Ok(())
    }

    pub(super) fn record_guard_site(&mut self, site: crate::sema::guarded_requirements::GuardBodySite, instruction: usize) -> Result<(), IrBuildError> {
        use crate::sema::guarded_requirements::GuardBodySite;
        let scope = match site { GuardBodySite::Source(scope) | GuardBodySite::Comparison(scope) | GuardBodySite::Output(scope) | GuardBodySite::Read { scope, .. } => scope };
        let index = self.guard_row(scope)?;
        let instruction = u32::try_from(instruction).ok().filter(|instruction| *instruction != IR_NONE).ok_or_else(|| IrBuildError::format("guard_source_instruction", None, 0, instruction))?;
        let target = match site {
            GuardBodySite::Source(_) => &mut self.store.scheme_guards[index].source_instruction,
            GuardBodySite::Comparison(_) => &mut self.store.scheme_guards[index].comparison_instruction,
            GuardBodySite::Output(_) => &mut self.store.scheme_guards[index].output.as_mut().ok_or_else(|| IrBuildError::format("guard_output_missing", None, 0, index))?.instruction,
            GuardBodySite::Read { ordinal, .. } => {
                let range = self.store.scheme_guards[index].reads.bounds(self.store.scheme_guard_reads.len()).ok_or_else(|| IrBuildError::format("guard_read_range", None, 0, index))?;
                let read = range.start.checked_add(ordinal as usize).filter(|index| range.contains(index)).ok_or_else(|| IrBuildError::format("guard_read_index", None, 0, index))?;
                &mut self.store.scheme_guard_reads[read].instruction
            }
        };
        if *target != IR_NONE { return Err(IrBuildError::format("guard_source_duplicated", None, 0, instruction as usize)); }
        *target = instruction;
        Ok(())
    }

    pub(super) fn encode_guard_bindings(&mut self, bindings: &[GuardBinding]) -> Result<IrRange, IrBuildError> {
        let mut encoded = Vec::with_capacity(bindings.len());
        for binding in bindings {
            encoded.push(match binding {
                GuardBinding::NonNull(local) => FullGuardBinding::NonNull(self.encode_instantiation(local)?),
                GuardBinding::Optional(local) => FullGuardBinding::Optional(self.encode_instantiation(local)?),
                GuardBinding::Null => FullGuardBinding::Null,
                GuardBinding::Call(call) => FullGuardBinding::Call(call.raw()),
            });
        }
        let start = self.store.call_guard_bindings.len();
        self.store.call_guard_bindings.extend(encoded);
        super::table_range(start, self.store.call_guard_bindings.len())
    }

    pub(super) fn record_guard_call_origins(&mut self, scope: Option<crate::sema::guarded_requirements::GuardScopeId>, instruction: usize, leaf: u32, role: crate::sema::schemes::InstantiationRole, arguments: FullInstantiation) -> Result<(), IrBuildError> {
        let mut ordinals = Vec::new();
        for range in [arguments.guard_bindings, arguments.guard_call_bindings] {
            let range = range.bounds(self.store.call_guard_bindings.len()).ok_or_else(|| IrBuildError::format("guard_call_binding_range", None, 0, instruction))?;
            ordinals.extend(self.store.call_guard_bindings[range].iter().filter_map(|binding| if let FullGuardBinding::Call(ordinal) = binding { Some(*ordinal) } else { None }));
        }
        if ordinals.is_empty() { return Ok(()); }
        let pack = self.active_requirement_ranges(scope)?;
        let range = pack.guard_calls.bounds(self.store.scheme_guard_calls.len()).ok_or_else(|| IrBuildError::format("guard_call_range", None, 0, instruction))?;
        let function = self.current_owner.and_then(super::IrFunctionId::from_raw).ok_or_else(|| IrBuildError::format("guard_call_owner", None, 0, instruction))?;
        for ordinal in ordinals {
            let row = range.start.checked_add(ordinal as usize).filter(|index| range.contains(index)).ok_or_else(|| IrBuildError::format("guard_call_ordinal", None, 0, instruction))?;
            let call = &mut self.store.scheme_guard_calls[row];
            if call.instruction != IR_NONE || call.role != role { return Err(IrBuildError::format("guard_call_origin", None, 0, instruction)); }
            call.instruction = u32::try_from(instruction).ok().filter(|instruction| *instruction != IR_NONE).ok_or_else(|| IrBuildError::format("guard_call_instruction", None, 0, instruction))?;
            call.function = function.raw();
            call.leaf = leaf;
        }
        Ok(())
    }
}
