use super::types::{CallableType, ModuleExportType, Type};
use crate::source::Span;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_VARIABLE: AtomicU64 = AtomicU64::new(1);
const MAX_TYPE_DEPTH: usize = 256;

/// A transient semantic identity. It never names dynamic data or a recovery
/// type, and must be substituted before a checked contract is published.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TypeVariableId(u64);

#[derive(Clone, Debug)]
struct Variable {
    origin: Span,
    binding: Option<Type>,
    contribution: Option<Span>,
    annotation: bool,
}

/// One local or declaration inference problem. Cloned probes share existing
/// variable identities, while new variables in either probe remain distinct.
/// Substitutions and provenance belong to this value, never a global cache.
#[derive(Clone, Debug, Default)]
pub struct TypeConstraints {
    variables: FxHashMap<TypeVariableId, Variable>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstraintResolutionError {
    ForeignVariable,
    TypeDepth,
}

#[derive(Clone, Debug)]
pub struct ConstraintConflict {
    pub expected: Type,
    pub actual: Type,
    pub initializer: Option<Span>,
    pub established: Option<Span>,
    pub contribution: Span,
    pub resolution_error: Option<ConstraintResolutionError>,
}

impl TypeConstraints {
    pub fn fresh(&mut self, origin: Span) -> Type {
        let id = TypeVariableId(NEXT_VARIABLE.fetch_add(1, Ordering::Relaxed));
        self.variables.insert(id, Variable { origin, binding: None, contribution: None, annotation: false });
        Type::Inference(id)
    }

    /// Add one directional expectation without widening concrete operands.
    /// A failed nested constraint rolls back every substitution it attempted.
    pub fn constrain(&mut self, expected: &Type, actual: &Type, contribution: Span) -> Result<(), ConstraintConflict> {
        self.constrain_with_authority(expected, actual, contribution, false)
    }

    /// A checked annotation may deliberately choose Null, Any, or an exact
    /// empty shape. Such choices cannot be inferred from ordinary operands.
    pub fn constrain_annotation(&mut self, expected: &Type, annotation: &Type, contribution: Span) -> Result<(), ConstraintConflict> {
        if !has_anchor(annotation, true) {
            let provenance = self.provenance(expected);
            return Err(ConstraintConflict {
                expected: expected.clone(), actual: annotation.clone(),
                initializer: provenance.map(|value| value.0),
                established: provenance.and_then(|value| value.1), contribution,
                resolution_error: None,
            });
        }
        self.constrain_with_authority(expected, annotation, contribution, true)
    }

    /// A grounded expected context can anchor holes, including an explicitly
    /// dynamic domain, while preserving directional value assignability.
    pub fn constrain_context(&mut self, expected: &Type, actual: &Type, contribution: Span) -> Result<(), ConstraintConflict> {
        self.constrain_with_authority(expected, actual, contribution, true)
    }

    fn constrain_with_authority(&mut self, expected: &Type, actual: &Type, contribution: Span, annotation: bool) -> Result<(), ConstraintConflict> {
        let provenance = self.provenance(expected).or_else(|| self.provenance(actual));
        let mut changes = Vec::new();
        let result = self.constrain_inner(expected, actual, contribution, annotation, &mut changes)
            .and_then(|()| {
                let resolved_expected = self.resolve(expected).map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
                let resolved_actual = self.resolve(actual).map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
                let unresolved = self.variable_ids(&resolved_expected).map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
                let actual_unresolved = self.variable_ids(&resolved_actual).map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
                // Substitution must preserve the same concrete boundary relation,
                // including invariant containers whose elements contain records.
                if unresolved.is_empty() && actual_unresolved.is_empty()
                    && !resolved_actual.matches_expected(&resolved_expected)
                {
                    return Err((resolved_expected, resolved_actual, None));
                }
                Ok(())
            });
        if let Err((expected, actual, resolution_error)) = result {
            for (id, previous) in changes.into_iter().rev() { self.variables.insert(id, previous); }
            return Err(ConstraintConflict {
                expected, actual, initializer: provenance.map(|value| value.0),
                established: provenance.and_then(|value| value.1), contribution, resolution_error,
            });
        }
        Ok(())
    }

    fn constrain_inner(&mut self, expected: &Type, actual: &Type, contribution: Span, annotation: bool, changes: &mut Vec<(TypeVariableId, Variable)>) -> Result<(), (Type, Type, Option<ConstraintResolutionError>)> {
        let mut pending = vec![(expected.clone(), actual.clone())];
        while let Some((expected, actual)) = pending.pop() {
            let contribution_to_variable = matches!(expected, Type::Inference(_)) && !self.annotation_variable(&expected);
            let expected = if matches!(expected, Type::Inference(_)) {
                self.resolve(&expected).map_err(|error| (expected.clone(), actual.clone(), Some(error)))?
            } else { expected };
            let actual = if matches!(actual, Type::Inference(_)) {
                self.resolve(&actual).map_err(|error| (expected.clone(), actual.clone(), Some(error)))?
            } else { actual };
            // A solved inference identity keeps one concrete shape. Contextual
            // record width conversion cannot select a different inferred shape
            // according to which contribution happened to arrive first.
            if contribution_to_variable
                && self.variable_ids(&expected).map_err(|error| (expected.clone(), actual.clone(), Some(error)))?.is_empty()
                && self.variable_ids(&actual).map_err(|error| (expected.clone(), actual.clone(), Some(error)))?.is_empty()
                && !actual.matches_invariant(&expected)
            {
                return Err((expected, actual, None));
            }
            if expected == actual { continue; }
            match (&expected, &actual) {
                (Type::Inference(left), Type::Inference(right)) => {
                    let (root, alias) = if left < right { (*left, *right) } else { (*right, *left) };
                    self.bind(alias, Type::Inference(root), contribution, annotation, changes);
                }
                (Type::Inference(id), value) | (value, Type::Inference(id)) => {
                    if !has_anchor(value, annotation) { continue; }
                    let variables = self.variable_ids(value).map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
                    if variables.contains(id) { return Err((expected, actual, None)); }
                    self.bind(*id, value.clone(), contribution, annotation, changes);
                }
                (Type::Optional(_), Type::Null) => {}
                (Type::Optional(left), Type::Optional(right))
                | (Type::List(left), Type::List(right))
                | (Type::Stream(left), Type::Stream(right)) => pending.push(((**left).clone(), (**right).clone())),
                (Type::Map(left_key, left_value), Type::Map(right_key, right_value)) => {
                    pending.push(((**left_value).clone(), (**right_value).clone()));
                    pending.push(((**left_key).clone(), (**right_key).clone()));
                }
                (Type::Optional(left), right) => pending.push(((**left).clone(), right.clone())),
                (Type::Result(left_ok, left_error), Type::Result(right_ok, right_error)) => {
                    pending.push(((**left_error).clone(), (**right_error).clone()));
                    pending.push(((**left_ok).clone(), (**right_ok).clone()));
                }
                (Type::Record(left), Type::Record(right)) => {
                    for (name, expected_field) in left.iter().rev() {
                        let Some(actual_field) = right.get(name) else { return Err((expected, actual, None)); };
                        pending.push((expected_field.clone(), actual_field.clone()));
                    }
                }
                _ if matches!(expected, Type::Any | Type::Unknown | Type::Invalid)
                    || matches!(actual, Type::Unknown | Type::Invalid) => {}
                _ if actual.matches_expected(&expected) => {}
                _ => return Err((expected, actual, None)),
            }
        }
        Ok(())
    }

    fn annotation_variable(&self, ty: &Type) -> bool {
        let Type::Inference(mut id) = *ty else { return false; };
        for _ in 0..=MAX_TYPE_DEPTH {
            let Some(variable) = self.variables.get(&id) else { return false; };
            if variable.annotation { return true; }
            match variable.binding {
                Some(Type::Inference(next)) => id = next,
                _ => return false,
            }
        }
        false
    }

    fn bind(&mut self, id: TypeVariableId, binding: Type, contribution: Span, annotation: bool, changes: &mut Vec<(TypeVariableId, Variable)>) {
        let variable = self.variables.get_mut(&id).expect("constraint variable belongs to this problem");
        changes.push((id, variable.clone()));
        variable.binding = Some(binding);
        variable.annotation = annotation;
        variable.contribution.get_or_insert(contribution);
    }

    /// Canonical partial substitution retains unresolved variable identities.
    /// Callers must separately reject unresolved material contracts.
    pub fn resolve(&self, ty: &Type) -> Result<Type, ConstraintResolutionError> {
        let mut resolved = ty.clone();
        self.resolve_in_place(&mut resolved)?;
        Ok(resolved)
    }

    /// `resolve` without rebuilding the type: only inference identities are
    /// rewritten. On error the type may be partially rewritten.
    pub fn resolve_in_place(&self, ty: &mut Type) -> Result<(), ConstraintResolutionError> {
        self.resolve_depth(ty, 0)
    }

    fn resolve_depth(&self, ty: &mut Type, depth: usize) -> Result<(), ConstraintResolutionError> {
        if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
        match ty {
            Type::Inference(id) => {
                let mut id = *id;
                let mut seen = FxHashSet::default();
                loop {
                    if seen.len() + depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
                    if !seen.insert(id) { return Err(ConstraintResolutionError::TypeDepth); }
                    let variable = self.variables.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?;
                    match &variable.binding {
                        Some(Type::Inference(next)) => id = *next,
                        Some(binding) => {
                            let mut binding = binding.clone();
                            self.resolve_depth(&mut binding, depth + 1)?;
                            *ty = binding;
                            break;
                        }
                        None => { *ty = Type::Inference(id); break; }
                    }
                }
            }
            Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) => self.resolve_depth(inner, depth + 1)?,
            Type::Map(first, second) | Type::Result(first, second) => {
                self.resolve_depth(first, depth + 1)?;
                self.resolve_depth(second, depth + 1)?;
            }
            Type::Record(fields) => for field in fields.values_mut() { self.resolve_depth(field, depth + 1)?; },
            // Module contracts are shared; copy one only when it must be rewritten.
            Type::Module(exports) if module_has_inference(exports, depth)? => {
                for export in Arc::make_mut(exports).values_mut() {
                    match export {
                        ModuleExportType::Value { ty, .. } => self.resolve_depth(ty, depth + 1)?,
                        ModuleExportType::Pure { sig, .. } | ModuleExportType::Proc { sig, .. } => self.resolve_callable_depth(sig, depth + 1)?,
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn resolve_callable(&self, signature: &CallableType) -> Result<CallableType, ConstraintResolutionError> {
        let mut signature = signature.clone();
        self.resolve_callable_depth(&mut signature, 0)?;
        Ok(signature)
    }

    fn resolve_callable_depth(&self, signature: &mut CallableType, depth: usize) -> Result<(), ConstraintResolutionError> {
        for parameter in &mut signature.params { self.resolve_depth(&mut parameter.ty, depth + 1)?; }
        self.resolve_depth(&mut signature.return_ty, depth + 1)
    }

    pub fn unresolved(&self, ty: &Type) -> Result<Vec<Span>, ConstraintResolutionError> {
        let resolved = self.resolve(ty)?;
        let mut origins = self.variable_ids(&resolved)?.into_iter().map(|id| self.variables[&id].origin).collect::<Vec<_>>();
        origins.sort_by_key(|span| (span.source_id, span.start(), span.end()));
        origins.dedup();
        Ok(origins)
    }

    fn variable_ids(&self, ty: &Type) -> Result<FxHashSet<TypeVariableId>, ConstraintResolutionError> {
        let mut variables = FxHashSet::default();
        let mut pending = vec![(ty, 0)];
        while let Some((ty, depth)) = pending.pop() {
            if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
            match ty {
                Type::Inference(id) => {
                    if !self.variables.contains_key(id) { return Err(ConstraintResolutionError::ForeignVariable); }
                    variables.insert(*id);
                }
                Type::List(inner) | Type::Optional(inner) | Type::Stream(inner) => pending.push((inner, depth + 1)),
            Type::Map(key, value) => { pending.push((key, depth + 1)); pending.push((value, depth + 1)); }
                Type::Result(ok, error) => { pending.push((ok, depth + 1)); pending.push((error, depth + 1)); }
                Type::Record(fields) => pending.extend(fields.values().map(|ty| (ty, depth + 1))),
                Type::Module(exports) => for export in exports.values() {
                    match export {
                        ModuleExportType::Value { ty, .. } => pending.push((ty, depth + 1)),
                        ModuleExportType::Pure { sig, .. } | ModuleExportType::Proc { sig, .. } => {
                            pending.push((&sig.return_ty, depth + 1));
                            pending.extend(sig.params.iter().map(|parameter| (&parameter.ty, depth + 1)));
                        }
                    }
                },
                _ => {}
            }
        }
        Ok(variables)
    }

    fn provenance(&self, ty: &Type) -> Option<(Span, Option<Span>)> {
        let mut earliest = None;
        for mut id in self.variable_ids(ty).ok()? {
            let mut seen = FxHashSet::default();
            let mut origin = self.variables.get(&id)?.origin;
            loop {
                if seen.len() > MAX_TYPE_DEPTH || !seen.insert(id) { return None; }
                let variable = self.variables.get(&id)?;
                if (variable.origin.source_id, variable.origin.start()) < (origin.source_id, origin.start()) {
                    origin = variable.origin;
                }
                match variable.binding {
                    Some(Type::Inference(next)) => id = next,
                    _ => {
                        let candidate = (origin, variable.contribution);
                        if earliest.is_none_or(|value: (Span, Option<Span>)| (origin.source_id, origin.start()) < (value.0.source_id, value.0.start())) {
                            earliest = Some(candidate);
                        }
                        break;
                    }
                }
            }
        }
        earliest
    }
}

fn has_anchor(ty: &Type, annotation: bool) -> bool {
    if matches!(ty, Type::Inference(_)) { return false; }
    let mut pending = vec![(ty, 0)];
    while let Some((ty, depth)) = pending.pop() {
        if depth > MAX_TYPE_DEPTH { return false; }
        match ty {
            Type::Unknown | Type::Invalid => return false,
            Type::Inference(_) if annotation => return false,
            Type::Any | Type::ErasedRecord | Type::Null if !annotation => return false,
            Type::List(inner) | Type::Optional(inner) | Type::Stream(inner) => pending.push((inner, depth + 1)),
            Type::Map(key, value) => { pending.push((key, depth + 1)); pending.push((value, depth + 1)); }
            Type::Result(ok, error) => { pending.push((ok, depth + 1)); pending.push((error, depth + 1)); }
            Type::Record(fields) => {
                if fields.is_empty() && !annotation { return false; }
                pending.extend(fields.values().map(|ty| (ty, depth + 1)));
            }
            Type::Module(exports) => for export in exports.values() {
                match export {
                    ModuleExportType::Value { ty, .. } => pending.push((ty, depth + 1)),
                    ModuleExportType::Pure { sig, .. } | ModuleExportType::Proc { sig, .. } => {
                        pending.push((&sig.return_ty, depth + 1));
                        pending.extend(sig.params.iter().map(|parameter| (&parameter.ty, depth + 1)));
                    }
                }
            },
            _ => {}
        }
    }
    true
}

/// Whether `resolve_depth` would rewrite this type, reporting the depth limit
/// exactly as resolution would when nothing needs rewriting.
fn has_inference(ty: &Type, depth: usize) -> Result<bool, ConstraintResolutionError> {
    if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
    Ok(match ty {
        Type::Inference(_) => true,
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) => has_inference(inner, depth + 1)?,
        Type::Map(first, second) | Type::Result(first, second) => has_inference(first, depth + 1)? || has_inference(second, depth + 1)?,
        Type::Record(fields) => {
            for field in fields.values() { if has_inference(field, depth + 1)? { return Ok(true); } }
            false
        }
        Type::Module(exports) => module_has_inference(exports, depth)?,
        _ => false,
    })
}

fn module_has_inference(exports: &std::collections::BTreeMap<crate::symbol::Name, ModuleExportType>, depth: usize) -> Result<bool, ConstraintResolutionError> {
    for export in exports.values() {
        let found = match export {
            ModuleExportType::Value { ty, .. } => has_inference(ty, depth + 1)?,
            ModuleExportType::Pure { sig, .. } | ModuleExportType::Proc { sig, .. } => {
                let mut found = false;
                for parameter in &sig.params { if has_inference(&parameter.ty, depth + 2)? { found = true; break; } }
                found || has_inference(&sig.return_ty, depth + 2)?
            }
        };
        if found { return Ok(true); }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;

    fn span(start: usize) -> Span { Span::new(SourceId::new(0), start, start + 1) }

    #[test]
    fn aliases_share_one_solution_and_conflicts_retain_source_contributions() {
        let mut constraints = TypeConstraints::default();
        let seed = constraints.fresh(span(1));
        let alias = constraints.fresh(span(2));
        constraints.constrain(&alias, &seed, span(3)).unwrap();
        constraints.constrain(&seed, &Type::Path, span(4)).unwrap();
        assert_eq!(constraints.resolve(&alias).unwrap(), Type::Path);
        let error = constraints.constrain(&seed, &Type::Int, span(5)).unwrap_err();
        assert_eq!(error.initializer, Some(span(1)));
        assert_eq!(error.established, Some(span(4)));
        assert_eq!(error.contribution, span(5));
        assert_eq!(constraints.resolve(&seed).unwrap(), Type::Path);
        let alias_error = constraints.constrain(&alias, &Type::Int, span(6)).unwrap_err();
        assert_eq!(alias_error.initializer, Some(span(1)));
        assert_eq!(alias_error.established, Some(span(4)));
    }

    #[test]
    fn nested_conflicts_roll_back_earlier_variable_bindings() {
        let mut constraints = TypeConstraints::default();
        let item = constraints.fresh(span(1));
        let expected = Type::Result(Box::new(item.clone()), Box::new(Type::Int));
        let actual = Type::Result(Box::new(Type::Path), Box::new(Type::Str));
        assert!(constraints.constrain(&expected, &actual, span(2)).is_err());
        assert_eq!(constraints.resolve(&item).unwrap(), item);
        assert_eq!(constraints.unresolved(&expected).unwrap(), vec![span(1)]);
    }

    #[test]
    fn optional_evidence_and_reversed_alias_order_have_canonical_solutions() {
        for reversed in [false, true] {
            let mut constraints = TypeConstraints::default();
            let inner = constraints.fresh(span(1));
            let alias = constraints.fresh(span(2));
            let (left, right) = if reversed { (&alias, &inner) } else { (&inner, &alias) };
            constraints.constrain(left, right, span(3)).unwrap();
            let nullable = Type::Optional(Box::new(inner.clone()));
            for inert in [Type::Null, Type::Any, Type::Unknown, Type::Invalid] {
                constraints.constrain(&nullable, &inert, span(4)).unwrap();
                assert_eq!(constraints.unresolved(&nullable).unwrap(), vec![span(1)]);
            }
            constraints.constrain(&nullable, &Type::Path, span(5)).unwrap();
            assert_eq!(constraints.resolve(&nullable).unwrap(), Type::Optional(Box::new(Type::Path)));
            assert_eq!(constraints.resolve(&alias).unwrap(), Type::Path);
        }
    }

    #[test]
    fn probes_keep_existing_aliases_but_new_variables_cannot_cross_problems() {
        let mut constraints = TypeConstraints::default();
        let seed = constraints.fresh(span(1));
        let mut probe = constraints.clone();
        let probe_variable = probe.fresh(span(2));
        let other_variable = constraints.fresh(span(2));
        assert_ne!(probe_variable, other_variable);
        assert_eq!(constraints.resolve(&probe_variable), Err(ConstraintResolutionError::ForeignVariable));
        assert!(probe.constrain(&seed, &Type::List(Box::new(seed.clone())), span(3)).is_err());
        assert_eq!(probe.resolve(&seed).unwrap(), seed);
    }

    #[test]
    fn nested_dynamic_and_recovery_operands_do_not_establish_contracts() {
        let mut constraints = TypeConstraints::default();
        let seed = constraints.fresh(span(1));
        for inert in [Type::List(Box::new(Type::Any)), Type::List(Box::new(Type::Unknown)), Type::Optional(Box::new(Type::Null))] {
            constraints.constrain(&seed, &inert, span(2)).unwrap();
            assert_eq!(constraints.resolve(&seed).unwrap(), seed);
        }
        let item = constraints.fresh(span(3));
        constraints.constrain(&seed, &Type::List(Box::new(item.clone())), span(4)).unwrap();
        constraints.constrain(&item, &Type::Path, span(5)).unwrap();
        assert_eq!(constraints.resolve(&seed).unwrap(), Type::List(Box::new(Type::Path)));
    }

    #[test]
    fn checked_annotations_can_choose_types_that_ordinary_evidence_cannot_anchor() {
        for annotation in [Type::Any, Type::Null, Type::Record(Default::default()), Type::List(Box::new(Type::Any))] {
            let mut constraints = TypeConstraints::default();
            let seed = constraints.fresh(span(1));
            constraints.constrain(&seed, &annotation, span(2)).unwrap();
            assert_eq!(constraints.resolve(&seed).unwrap(), seed);
            constraints.constrain_annotation(&seed, &annotation, span(3)).unwrap();
            assert_eq!(constraints.resolve(&seed).unwrap(), annotation);
        }
        let mut constraints = TypeConstraints::default();
        let seed = constraints.fresh(span(1));
        let hole = constraints.fresh(span(2));
        for invalid in [Type::Invalid, Type::List(Box::new(Type::Unknown)), Type::Optional(Box::new(hole))] {
            assert!(constraints.constrain_annotation(&seed, &invalid, span(3)).is_err());
            assert_eq!(constraints.resolve(&seed).unwrap(), seed);
        }
    }

    #[test]
    fn dynamic_values_cannot_satisfy_resolved_concrete_constraints() {
        let mut constraints = TypeConstraints::default();
        for expected in [Type::Int, Type::Optional(Box::new(Type::Int)), Type::List(Box::new(Type::Int))] {
            assert!(constraints.constrain(&expected, &Type::Any, span(1)).is_err());
        }
        let record = Type::Record(std::collections::BTreeMap::from([(crate::symbol::Name::intern("value"), Type::Int)]));
        assert!(constraints.constrain(&record, &Type::ErasedRecord, span(2)).is_err());
        assert!(constraints.constrain(&Type::Any, &record, span(3)).is_ok());
        assert!(constraints.constrain(&Type::ErasedRecord, &record, span(4)).is_ok());
    }

    #[test]
    fn container_record_width_cannot_bypass_invariance_during_substitution() {
        crate::symbol::SymbolOwner::new().with_current(|| {
        let mut constraints = TypeConstraints::default();
        let item = constraints.fresh(span(1));
        let name = crate::symbol::Name::intern("value");
        let expected = Type::List(Box::new(Type::Record(std::collections::BTreeMap::from([(name, item.clone())]))));
        let actual = Type::List(Box::new(Type::Record(std::collections::BTreeMap::from([
            (name, Type::Int), (crate::symbol::Name::intern("extra"), Type::Bool),
        ]))));
        assert!(constraints.constrain(&expected, &actual, span(2)).is_err());
        assert_eq!(constraints.resolve(&item).unwrap(), item);
        });
    }
    #[test]
    fn optional_record_contributions_keep_one_shape_in_either_order() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let name = crate::symbol::Name::intern("value");
            let narrow = Type::Record(std::collections::BTreeMap::from([(name, Type::Int)]));
            let wide = Type::Record(std::collections::BTreeMap::from([
                (name, Type::Int), (crate::symbol::Name::intern("extra"), Type::Bool),
            ]));
            for (first, second) in [(&narrow, &wide), (&wide, &narrow)] {
                let mut constraints = TypeConstraints::default();
                let item = constraints.fresh(span(1));
                let optional = Type::Optional(Box::new(item.clone()));
                constraints.constrain(&optional, first, span(2)).unwrap();
                assert!(constraints.constrain(&optional, second, span(3)).is_err());
                assert_eq!(constraints.resolve(&item).unwrap(), *first);
                // A separately grounded destination may deliberately hide fields.
                assert!(constraints.constrain(&narrow, &wide, span(4)).is_ok());
            }
        });
    }

    #[test]
    fn authoritative_dynamic_destination_accepts_concrete_contributions() {
        let mut constraints = TypeConstraints::default();
        let chosen = constraints.fresh(span(1));
        constraints.constrain_annotation(&chosen, &Type::Any, span(2)).unwrap();
        constraints.constrain(&chosen, &Type::Int, span(3)).unwrap();
        assert_eq!(constraints.resolve(&chosen).unwrap(), Type::Any);
    }

}
