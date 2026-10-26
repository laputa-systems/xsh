use super::{FullBuilder, FullCallableContract, IrBuildError, IrFunctionId, IrRange, LoweredFunctionKey, QualifiedName, Type, TypeId};
use crate::sema::scheme_callable::{SchemeCallableError, SchemeCallableId};
use std::collections::BTreeMap;
use crate::sema::check::SchemeBinderCorrespondence;
use crate::sema::schemes::{CallableRequirements, CallableScheme, SchemeEffectParameterId, SchemeRowParameterId, SchemeTypeParameterId};

type NamespaceBinders = (Vec<SchemeTypeParameterId>, Vec<SchemeRowParameterId>, Vec<SchemeEffectParameterId>);

fn namespace_binders(scheme: &CallableScheme) -> NamespaceBinders {
    let mut result = (scheme.type_parameters().to_vec(), scheme.row_parameters().iter().map(|row| row.id).collect::<Vec<_>>(), scheme.requirements().effect_parameters.clone());
    let mut pending = scheme.requirements().guards.iter().rev().collect::<Vec<_>>();
    while let Some(guard) = pending.pop() {
        result.0.extend(&guard.type_parameters);
        result.1.extend(guard.row_parameters.iter().map(|row| row.id));
        result.2.extend(&guard.requirements.effect_parameters);
        pending.extend(guard.requirements.guards.iter().rev());
    }
    result
}

fn namespace_mapping<K: Copy + Ord>(mapping: &SchemeBinderCorrespondence, select: impl Copy + Fn(&SchemeBinderCorrespondence) -> &BTreeMap<K, K>) -> Result<BTreeMap<K, K>, IrBuildError> {
    let mut result = select(mapping).clone();
    let mut pending = mapping.guards().iter().rev().collect::<Vec<_>>();
    while let Some(guard) = pending.pop() {
        for (&source, &target) in select(&guard.bindings) {
            if result.insert(source, target).is_some() { return Err(FullBuilder::scheme_callable_build_error()); }
        }
        pending.extend(guard.bindings.guards().iter().rev());
    }
    Ok(result)
}

fn namespace_calls<'a>(left: &'a CallableRequirements, right: &'a CallableRequirements, mapping: &'a SchemeBinderCorrespondence) -> Result<Vec<(&'a CallableScheme, &'a CallableScheme, &'a SchemeBinderCorrespondence)>, IrBuildError> {
    let mut calls = Vec::new();
    let mut pending = vec![(left, right, mapping)];
    while let Some((left, right, mapping)) = pending.pop() {
        if left.guard_calls.len() != right.guard_calls.len() || left.guard_calls.len() != mapping.calls().len()
            || left.guards.len() != right.guards.len() || left.guards.len() != mapping.guards().len()
        { return Err(FullBuilder::scheme_callable_build_error()); }
        for ((left, right), mapped) in left.guard_calls.iter().zip(&right.guard_calls).zip(mapping.calls()) {
            if mapped.canonical != left.target || mapped.actual != right.target { return Err(FullBuilder::scheme_callable_build_error()); }
            calls.push((left.callee.as_ref(), right.callee.as_ref(), mapped.bindings.as_ref()));
        }
        for ((left, right), mapped) in left.guards.iter().zip(&right.guards).zip(mapping.guards()).rev() {
            if mapped.canonical != left.scope || mapped.actual != right.scope { return Err(FullBuilder::scheme_callable_build_error()); }
            pending.push((&left.requirements, &right.requirements, &mapped.bindings));
        }
    }
    Ok(calls)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeCallableValue {
    pub(super) canonical: IrFunctionId,
    pub(super) targets: IrRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeCallableTarget {
    pub(super) function: IrFunctionId,
    pub(super) types: IrRange,
    pub(super) rows: IrRange,
    pub(super) effects: IrRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeCallableCall {
    pub(super) instruction: u32,
    pub(super) relationship: u32,
}

#[derive(Clone, Copy)]
pub(super) struct NamespaceCorrespondence {
    pub(super) canonical_function: IrFunctionId,
    pub(super) actual_function: IrFunctionId,
    pub(super) canonical: super::schemes::FullSchemeId,
    pub(super) actual: super::schemes::FullSchemeId,
    pub(super) types: IrRange,
    pub(super) rows: IrRange,
    pub(super) effects: IrRange,
}

pub(super) fn correspondence_namespaces(store: &super::FullStore, canonical: IrFunctionId, target: FullSchemeCallableTarget,
    budget: &mut super::verify_schemes::SchemeInstantiationBudget,
) -> Result<Vec<NamespaceCorrespondence>, super::IrVerifyError> {
    let invalid = || super::IrVerifyError::new("scheme callable scoped correspondence is invalid");
    let scheme = |function: IrFunctionId| match store.functions.get(function.index()).ok_or_else(invalid)?.contract {
        FullCallableContract::Scheme(id) => Ok(id), _ => Err(invalid()),
    };
    let mut pending = vec![(canonical, target.function, 0)];
    let mut seen = std::collections::BTreeSet::new();
    let mut offsets = [0u32; 3];
    let mut result = Vec::new();
    while let Some((canonical_function, actual_function, depth)) = pending.pop() {
        budget.charge(1)?;
        if depth > crate::sema::constraints::MAX_TYPE_DEPTH { return Err(invalid()); }
        let (canonical, actual) = (scheme(canonical_function)?, scheme(actual_function)?);
        if !seen.insert((canonical, actual)) { continue; }
        let left = store.scheme(canonical)?;
        let right = store.scheme(actual)?;
        let left_guards = &store.scheme_guards[left.guards.bounds(store.scheme_guards.len()).ok_or_else(invalid)?];
        let right_guards = &store.scheme_guards[right.guards.bounds(store.scheme_guards.len()).ok_or_else(invalid)?];
        budget.charge(left_guards.len() + right_guards.len())?;
        if left_guards.len() != right_guards.len() { return Err(invalid()); }
        let mut counts = [left.type_parameters.len, left.row_parameters.len, left.effect_parameters.len];
        let mut other = [right.type_parameters.len, right.row_parameters.len, right.effect_parameters.len];
        let mut calls = vec![(left.guard_calls, right.guard_calls)];
        for (left, right) in left_guards.iter().zip(right_guards) {
            if left.id != right.id || left.parent != right.parent || left.binding_ordinal != right.binding_ordinal { return Err(invalid()); }
            for (index, (left, right)) in [(left.type_parameters, right.type_parameters), (left.row_parameters, right.row_parameters), (left.effect_parameters, right.effect_parameters)].into_iter().enumerate() {
                counts[index] = counts[index].checked_add(left.len).ok_or_else(invalid)?;
                other[index] = other[index].checked_add(right.len).ok_or_else(invalid)?;
            }
            calls.push((left.guard_calls, right.guard_calls));
        }
        if counts != other { return Err(invalid()); }
        let mut take = |index: usize, range: IrRange| {
            range.bounds(store.scheme_callable_binders.len()).ok_or_else(invalid)?;
            let end = offsets[index].checked_add(counts[index]).ok_or_else(invalid)?;
            if end > range.len { return Err(invalid()); }
            let result = IrRange { start: range.start.checked_add(offsets[index]).ok_or_else(invalid)?, len: counts[index] };
            offsets[index] = end;
            Ok(result)
        };
        result.push(NamespaceCorrespondence { canonical_function, actual_function, canonical, actual, types: take(0, target.types)?, rows: take(1, target.rows)?, effects: take(2, target.effects)? });
        let mut children = Vec::new();
        for (left, right) in calls {
            let left = &store.scheme_guard_calls[left.bounds(store.scheme_guard_calls.len()).ok_or_else(invalid)?];
            let right = &store.scheme_guard_calls[right.bounds(store.scheme_guard_calls.len()).ok_or_else(invalid)?];
            budget.charge(left.len() + right.len())?;
            if left.len() != right.len() { return Err(invalid()); }
            for (left, right) in left.iter().zip(right) {
                if left.role != right.role || left.target != right.target { return Err(invalid()); }
                let (_, left) = super::verify_schemes::guard_call_source_target(store, *left, budget)?;
                let (_, right) = super::verify_schemes::guard_call_source_target(store, *right, budget)?;
                children.push((left, right, depth + 1));
            }
        }
        pending.extend(children.into_iter().rev());
    }
    if offsets != [target.types.len, target.rows.len, target.effects.len] { return Err(invalid()); }
    Ok(result)
}

impl FullBuilder {
    pub(super) fn intern_scheme_callable_value(&mut self, source: SchemeCallableId) -> Result<(u32, SchemeCallableId), IrBuildError> {
        if let Some(&index) = self.scheme_callable_ids.get(&source) {
            let id = self.store.semantic.scheme_callable_id(index).map_err(|_| Self::scheme_callable_build_error())?;
            return Ok((index, id));
        }
        let relationship = self.source_scheme_callables.as_ref().ok_or_else(Self::scheme_callable_build_error)?
            .relationship(source).map_err(|_| Self::scheme_callable_build_error())?;
        let index = u32::try_from(self.pending_scheme_callable_values.len()).ok().filter(|&index| index != super::IR_NONE)
            .ok_or_else(Self::scheme_callable_build_error)?;
        let destination = self.store.semantic.register_scheme_callable()?;
        self.pending_scheme_callable_values.push(relationship);
        self.scheme_callable_ids.insert(source, index);
        Ok((index, destination))
    }

    fn scheme_callable_build_error() -> IrBuildError { IrBuildError::format("scheme_callable_relationship", None, 0, 0) }

    pub(super) fn intern_checked_type(&mut self, source: &Type) -> Result<TypeId, IrBuildError> {
        if !source.contains_scheme_callable() { return self.semantic.intern_type(&mut self.store.semantic, source); }
        let destination = crate::sema::scheme_callable::map_type_ids(source, &mut |id| {
            self.intern_scheme_callable_value(id).map(|(_, id)| id).map_err(|_| SchemeCallableError::UnknownRelationship)
        }).map_err(|_| Self::scheme_callable_build_error())?;
        self.semantic.intern_type(&mut self.store.semantic, &destination)
    }

    pub(super) fn finish_scheme_callable_values(&mut self) -> Result<(), IrBuildError> {
        for index in self.store.scheme_callable_values.len()..self.pending_scheme_callable_values.len() {
            let relationship = self.pending_scheme_callable_values[index].clone();
            let canonical = relationship.canonical().scheme();
            let start = self.store.scheme_callable_targets.len();
            for target in relationship.targets() {
                let key = match target.target().namespace {
                    Some(namespace) => LoweredFunctionKey::Qualified(QualifiedName::new(namespace, target.target().name)),
                    None => LoweredFunctionKey::Name(target.target().name),
                };
                let function = self.function_ids.get(&key).copied().ok_or_else(Self::scheme_callable_build_error)?;
                if self.source_function_bodies.get(target.target().definition.index()).zip(self.lowered_function_bodies.get(function.index())).is_none_or(|(source, actual)| source != actual) { return Err(Self::scheme_callable_build_error()); }
                let header = self.store.functions.get(function.index()).ok_or_else(Self::scheme_callable_build_error)?;
                if !matches!(header.contract, FullCallableContract::Scheme(_)) { return Err(Self::scheme_callable_build_error()); }
                let actual = target.contract().scheme();
                let mut encoded = (Vec::new(), Vec::new(), Vec::new());
                let mut seen = std::collections::BTreeSet::new();
                let mut pending = vec![(canonical.callable().as_ref(), actual.callable().as_ref(), target.correspondence(), 0)];
                while let Some((canonical, actual, mapping, depth)) = pending.pop() {
                    if depth > crate::sema::constraints::MAX_TYPE_DEPTH { return Err(Self::scheme_callable_build_error()); }
                    if !seen.insert((canonical as *const CallableScheme as usize, actual as *const CallableScheme as usize)) { continue; }
                    let source = namespace_binders(canonical);
                    let destination = namespace_binders(actual);
                    fn append<K: Copy + Ord>(source: &[K], destination: &[K], mapping: &BTreeMap<K, K>, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
                        if source.len() != destination.len() || source.len() != mapping.len() { return Err(FullBuilder::scheme_callable_build_error()); }
                        let ordinals = destination.iter().enumerate().map(|(ordinal, id)| (*id, ordinal as u32)).collect::<BTreeMap<_, _>>();
                        if ordinals.len() != destination.len() { return Err(FullBuilder::scheme_callable_build_error()); }
                        for parameter in source {
                            output.push(*mapping.get(parameter).and_then(|id| ordinals.get(id)).ok_or_else(FullBuilder::scheme_callable_build_error)?);
                        }
                        Ok(())
                    }
                    append(&source.0, &destination.0, &namespace_mapping(mapping, SchemeBinderCorrespondence::types)?, &mut encoded.0)?;
                    append(&source.1, &destination.1, &namespace_mapping(mapping, SchemeBinderCorrespondence::rows)?, &mut encoded.1)?;
                    append(&source.2, &destination.2, &namespace_mapping(mapping, SchemeBinderCorrespondence::effects)?, &mut encoded.2)?;
                    pending.extend(namespace_calls(canonical.requirements(), actual.requirements(), mapping)?.into_iter().rev().map(|(left, right, mapping)| (left, right, mapping, depth + 1)));
                }
                let mut range = |values: Vec<u32>| {
                    let start = self.store.scheme_callable_binders.len();
                    self.store.scheme_callable_binders.extend(values);
                    super::table_range(start, self.store.scheme_callable_binders.len())
                };
                let types = range(encoded.0)?;
                let rows = range(encoded.1)?;
                let effects = range(encoded.2)?;
                self.store.scheme_callable_targets.push(FullSchemeCallableTarget { function, types, rows, effects });
            }
            let targets = super::table_range(start, self.store.scheme_callable_targets.len())?;
            let canonical = self.store.scheme_callable_targets[start].function;
            self.store.scheme_callable_targets[start..].sort_by_key(|target| target.function);
            self.store.scheme_callable_values.push(FullSchemeCallableValue { canonical, targets });
        }
        Ok(())
    }
}

impl super::FullProgram {
    pub(in crate::runtime::eval) fn has_scheme_callable_values(&self) -> bool { !self.store.scheme_callable_values.is_empty() }

    pub(in crate::runtime::eval) fn scheme_callable_value_accepts(&self, expected: SchemeCallableId, actual: u32, target: usize, kind: crate::sema::types::CallableKind) -> Result<bool, super::IrVerifyError> {
        let expected = self.store.semantic.scheme_callable_index(expected)?;
        self.validate_scheme_callable_target(actual, target, kind)?;
        self.validate_scheme_callable_target(expected, target, kind)?;
        Ok(true)
    }

    pub(in crate::runtime::eval) fn scheme_callable_site(&self, instruction: u32) -> Option<u32> {
        self.store.scheme_callable_calls.binary_search_by_key(&instruction, |site| site.instruction).ok().map(|index| self.store.scheme_callable_calls[index].relationship)
    }

    pub(in crate::runtime::eval) fn scheme_callable_expression(&self, instruction: u32) -> Result<Option<u32>, super::IrVerifyError> {
        let Ok(index) = self.store.checked_expr_types.binary_search_by_key(&instruction, |fact| fact.instruction) else { return Ok(None); };
        let fact = self.store.checked_expr_types[index];
        if !fact.is_contract() { return Ok(None); }
        match self.store.semantic.to_type(fact.type_id)? {
            Type::SchemeCallable(id) => self.store.semantic.scheme_callable_index(id).map(Some),
            _ => Ok(None),
        }
    }

    pub(in crate::runtime::eval) fn validate_scheme_callable_target(&self, relationship: u32, target: usize, kind: crate::sema::types::CallableKind) -> Result<(), super::IrVerifyError> {
        let relationship = self.store.scheme_callable_values.get(relationship as usize).ok_or_else(|| super::IrVerifyError::new("scheme callable relationship is out of bounds"))?;
        let range = relationship.targets.bounds(self.store.scheme_callable_targets.len()).ok_or_else(|| super::IrVerifyError::new("scheme callable target range is invalid"))?;
        if !self.store.scheme_callable_targets[range].iter().any(|row| row.function.index() == target) { return Err(super::IrVerifyError::new("scheme callable actual target is not a member")); }
        let metadata = self.store.function_metadata.get(target).ok_or_else(|| super::IrVerifyError::new("scheme callable target metadata is absent"))?;
        if metadata.callable_kind != kind { return Err(super::IrVerifyError::new("scheme callable actual kind disagrees with its target")); }
        Ok(())
    }
}

pub(super) fn reorder_scoped_binders<T>(store: &super::FullStore, mapping: IrRange, source: std::ops::Range<usize>, target: std::ops::Range<usize>, values: Vec<T>, budget: &mut super::verify_schemes::SchemeInstantiationBudget) -> Result<Vec<T>, super::IrVerifyError> {
    let invalid = || super::IrVerifyError::new("scheme callable binder leaves its owned scope");
    let mapping = mapping.bounds(store.scheme_callable_binders.len()).ok_or_else(invalid)?;
    let mapping = store.scheme_callable_binders[mapping].get(source.clone()).ok_or_else(invalid)?;
    budget.charge(mapping.len())?;
    if source.len() != target.len() || source.len() != values.len() { return Err(invalid()); }
    let mut result = std::iter::repeat_with(|| None).take(values.len()).collect::<Vec<_>>();
    for (&ordinal, value) in mapping.iter().zip(values) {
        let ordinal = ordinal as usize;
        if !target.contains(&ordinal) { return Err(invalid()); }
        if result[ordinal - target.start].replace(value).is_some() { return Err(invalid()); }
    }
    result.into_iter().map(|value| value.ok_or_else(invalid)).collect()
}

pub(super) fn relationship_type_fits(store: &super::FullStore, actual: &Type, expected: &Type, budget: &mut super::verify_schemes::SchemeInstantiationBudget, depth: usize) -> Result<bool, super::IrVerifyError> {
    if depth > 128 { return Err(super::IrVerifyError::new("scheme callable relationship nesting is too deep")); }
    budget.charge(1)?;
    let nested = |actual, expected, budget: &mut super::verify_schemes::SchemeInstantiationBudget| relationship_type_fits(store, actual, expected, budget, depth + 1);
    Ok(match (actual, expected) {
        (Type::SchemeCallable(actual), Type::SchemeCallable(expected)) => {
            if actual == expected { return Ok(true); }
            let actual = store.semantic.scheme_callable_index(*actual)?;
            let expected = store.semantic.scheme_callable_index(*expected)?;
            let actual = store.scheme_callable_values.get(actual as usize).ok_or_else(|| super::IrVerifyError::new("source callable relationship is out of bounds"))?;
            let expected = store.scheme_callable_values.get(expected as usize).ok_or_else(|| super::IrVerifyError::new("expected callable relationship is out of bounds"))?;
            let actual = actual.targets.bounds(store.scheme_callable_targets.len()).ok_or_else(|| super::IrVerifyError::new("source callable target range is invalid"))?;
            let expected = expected.targets.bounds(store.scheme_callable_targets.len()).ok_or_else(|| super::IrVerifyError::new("expected callable target range is invalid"))?;
            let expected = &store.scheme_callable_targets[expected];
            for target in &store.scheme_callable_targets[actual] {
                budget.charge(1)?;
                let (mut left, mut right) = (0, expected.len());
                while left < right {
                    budget.charge(1)?;
                    let middle = left + (right - left) / 2;
                    if expected[middle].function < target.function { left = middle + 1; } else { right = middle; }
                }
                if expected.get(left).is_none_or(|row| row.function != target.function) { return Ok(false); }
            }
            true
        }
        (Type::List(actual), Type::List(expected)) | (Type::Optional(actual), Type::Optional(expected)) | (Type::Stream(actual), Type::Stream(expected)) => nested(actual, expected, budget)?,
        (Type::Result(actual, actual_error), Type::Result(expected, expected_error)) | (Type::Map(actual, actual_error), Type::Map(expected, expected_error)) => nested(actual, expected, budget)? && nested(actual_error, expected_error, budget)?,
        (Type::Record(actual), Type::Record(expected)) => {
            for (name, expected) in expected.iter() {
                budget.charge(1)?;
                let Some(actual) = actual.get(name) else { return Ok(false); };
                if !nested(actual, expected, budget)? { return Ok(false); }
            }
            true
        }
        (Type::Validated(actual), Type::Validated(expected)) if actual.validation() == expected.validation() => nested(actual.base(), expected.base(), budget)?,
        _ => { budget.charge_clone(actual)?; budget.charge_clone(expected)?; actual.matches_expected(expected) },
    })
}
