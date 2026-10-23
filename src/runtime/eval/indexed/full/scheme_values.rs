use super::{FullBuilder, FullCallableContract, IrBuildError, IrFunctionId, IrRange, LoweredFunctionKey, QualifiedName, Type, TypeId};
use crate::sema::scheme_callable::{SchemeCallableError, SchemeCallableId};
use std::collections::BTreeMap;

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

    fn scheme_callable_binder_range<K: Ord + Copy>(
        &mut self, canonical: &[K], actual: &[K], correspondence: &BTreeMap<K, K>,
    ) -> Result<IrRange, IrBuildError> {
        if canonical.len() != actual.len() || correspondence.len() != canonical.len() { return Err(Self::scheme_callable_build_error()); }
        let ordinals = actual.iter().enumerate().map(|(index, &id)| (id, index as u32)).collect::<BTreeMap<_, _>>();
        if ordinals.len() != actual.len() { return Err(Self::scheme_callable_build_error()); }
        let start = self.store.scheme_callable_binders.len();
        for parameter in canonical {
            let target = correspondence.get(parameter).and_then(|id| ordinals.get(id)).copied().ok_or_else(Self::scheme_callable_build_error)?;
            self.store.scheme_callable_binders.push(target);
        }
        super::table_range(start, self.store.scheme_callable_binders.len())
    }

    pub(super) fn finish_scheme_callable_values(&mut self) -> Result<(), IrBuildError> {
        for index in self.store.scheme_callable_values.len()..self.pending_scheme_callable_values.len() {
            let relationship = self.pending_scheme_callable_values[index].clone();
            let canonical = relationship.canonical().scheme();
            let canonical_rows = canonical.row_parameters().iter().map(|row| row.id).collect::<Vec<_>>();
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
                let actual_rows = actual.row_parameters().iter().map(|row| row.id).collect::<Vec<_>>();
                let types = self.scheme_callable_binder_range(canonical.type_parameters(), actual.type_parameters(), target.correspondence().types())?;
                let rows = self.scheme_callable_binder_range(&canonical_rows, &actual_rows, target.correspondence().rows())?;
                let effects = self.scheme_callable_binder_range(&canonical.callable().requirements().effect_parameters, &actual.callable().requirements().effect_parameters, target.correspondence().effects())?;
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

pub(super) fn reorder_binders<T>(store: &super::FullStore, range: IrRange, values: Vec<T>, budget: &mut super::verify_schemes::SchemeInstantiationBudget) -> Result<Vec<T>, super::IrVerifyError> {
    let range = range.bounds(store.scheme_callable_binders.len()).ok_or_else(|| super::IrVerifyError::new("scheme callable binder range is invalid"))?;
    budget.charge(range.len())?;
    if range.len() != values.len() { return Err(super::IrVerifyError::new("scheme callable binder correspondence is incomplete")); }
    let mut result = std::iter::repeat_with(|| None).take(values.len()).collect::<Vec<_>>();
    for (&ordinal, value) in store.scheme_callable_binders[range].iter().zip(values) {
        let slot = result.get_mut(ordinal as usize).ok_or_else(|| super::IrVerifyError::new("scheme callable binder ordinal is out of bounds"))?;
        if slot.replace(value).is_some() { return Err(super::IrVerifyError::new("scheme callable binder correspondence is duplicated")); }
    }
    result.into_iter().map(|value| value.ok_or_else(|| super::IrVerifyError::new("scheme callable binder correspondence is incomplete"))).collect()
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
