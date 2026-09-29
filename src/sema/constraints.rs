use super::types::{CallableType, ModuleExportType, Type};
use crate::source::Span;
use rustc_hash::{FxHashMap, FxHashSet};
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
        self.variables.insert(id, Variable { origin, binding: None, contribution: None });
        Type::Inference(id)
    }

    /// Add one directional expectation without widening concrete operands.
    /// A failed nested constraint rolls back every substitution it attempted.
    pub fn constrain(&mut self, expected: &Type, actual: &Type, contribution: Span) -> Result<(), ConstraintConflict> {
        let provenance = self.provenance(expected).or_else(|| self.provenance(actual));
        let mut changes = Vec::new();
        let result = self.constrain_inner(expected, actual, contribution, &mut changes);
        if let Err((expected, actual, resolution_error)) = result {
            for (id, previous) in changes.into_iter().rev() { self.variables.insert(id, previous); }
            return Err(ConstraintConflict {
                expected, actual, initializer: provenance.map(|value| value.0),
                established: provenance.and_then(|value| value.1), contribution, resolution_error,
            });
        }
        Ok(())
    }

    fn constrain_inner(&mut self, expected: &Type, actual: &Type, contribution: Span, changes: &mut Vec<(TypeVariableId, Variable)>) -> Result<(), (Type, Type, Option<ConstraintResolutionError>)> {
        let mut pending = vec![(expected.clone(), actual.clone())];
        while let Some((expected, actual)) = pending.pop() {
            let expected = self.resolve(&expected).map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
            let actual = self.resolve(&actual).map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
            if expected == actual { continue; }
            match (&expected, &actual) {
                (Type::Inference(left), Type::Inference(right)) => {
                    let (root, alias) = if left < right { (*left, *right) } else { (*right, *left) };
                    self.bind(alias, Type::Inference(root), contribution, changes);
                }
                (Type::Inference(id), value) | (value, Type::Inference(id)) => {
                    if !has_anchor(value) { continue; }
                    let variables = self.variable_ids(value).map_err(|error| (expected.clone(), actual.clone(), Some(error)))?;
                    if variables.contains(id) { return Err((expected, actual, None)); }
                    self.bind(*id, value.clone(), contribution, changes);
                }
                (Type::Optional(_), Type::Null) => {}
                (Type::Optional(left), Type::Optional(right))
                | (Type::List(left), Type::List(right))
                | (Type::Map(left), Type::Map(right))
                | (Type::Stream(left), Type::Stream(right)) => pending.push(((**left).clone(), (**right).clone())),
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
                    || matches!(actual, Type::Any | Type::Unknown | Type::Invalid) => {}
                _ if actual.matches_expected(&expected) => {}
                _ => return Err((expected, actual, None)),
            }
        }
        Ok(())
    }

    fn bind(&mut self, id: TypeVariableId, binding: Type, contribution: Span, changes: &mut Vec<(TypeVariableId, Variable)>) {
        let variable = self.variables.get_mut(&id).expect("constraint variable belongs to this problem");
        changes.push((id, variable.clone()));
        variable.binding = Some(binding);
        variable.contribution.get_or_insert(contribution);
    }

    /// Canonical partial substitution retains unresolved variable identities.
    /// Callers must separately reject unresolved material contracts.
    pub fn resolve(&self, ty: &Type) -> Result<Type, ConstraintResolutionError> {
        self.resolve_depth(ty, 0)
    }

    fn resolve_depth(&self, ty: &Type, depth: usize) -> Result<Type, ConstraintResolutionError> {
        if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
        Ok(match ty {
            Type::Inference(id) => {
                let mut id = *id;
                let mut seen = FxHashSet::default();
                loop {
                    if seen.len() + depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
                    if !seen.insert(id) { return Err(ConstraintResolutionError::TypeDepth); }
                    let variable = self.variables.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?;
                    match &variable.binding {
                        Some(Type::Inference(next)) => id = *next,
                        Some(binding) => break self.resolve_depth(binding, depth + 1)?,
                        None => break Type::Inference(id),
                    }
                }
            }
            Type::List(inner) => Type::List(Box::new(self.resolve_depth(inner, depth + 1)?)),
            Type::Map(inner) => Type::Map(Box::new(self.resolve_depth(inner, depth + 1)?)),
            Type::Stream(inner) => Type::Stream(Box::new(self.resolve_depth(inner, depth + 1)?)),
            Type::Optional(inner) => Type::Optional(Box::new(self.resolve_depth(inner, depth + 1)?)),
            Type::Result(ok, error) => Type::Result(Box::new(self.resolve_depth(ok, depth + 1)?), Box::new(self.resolve_depth(error, depth + 1)?)),
            Type::Record(fields) => Type::Record(fields.iter().map(|(name, ty)| Ok((*name, self.resolve_depth(ty, depth + 1)?))).collect::<Result<_, ConstraintResolutionError>>()?),
            Type::Module(exports) => Type::Module(exports.iter().map(|(name, export)| {
                let export = match export {
                    ModuleExportType::Value { ty, optional } => ModuleExportType::Value { ty: self.resolve_depth(ty, depth + 1)?, optional: *optional },
                    ModuleExportType::Pure { sig, optional } => ModuleExportType::Pure { sig: self.resolve_callable_depth(sig, depth + 1)?, optional: *optional },
                    ModuleExportType::Proc { sig, optional } => ModuleExportType::Proc { sig: self.resolve_callable_depth(sig, depth + 1)?, optional: *optional },
                };
                Ok((*name, export))
            }).collect::<Result<_, ConstraintResolutionError>>()?),
            ty => ty.clone(),
        })
    }

    pub fn resolve_callable(&self, signature: &CallableType) -> Result<CallableType, ConstraintResolutionError> {
        self.resolve_callable_depth(signature, 0)
    }

    fn resolve_callable_depth(&self, signature: &CallableType, depth: usize) -> Result<CallableType, ConstraintResolutionError> {
        let mut signature = signature.clone();
        for parameter in &mut signature.params { parameter.ty = self.resolve_depth(&parameter.ty, depth + 1)?; }
        signature.return_ty = Box::new(self.resolve_depth(&signature.return_ty, depth + 1)?);
        Ok(signature)
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
                Type::List(inner) | Type::Map(inner) | Type::Optional(inner) | Type::Stream(inner) => pending.push((inner, depth + 1)),
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

fn has_anchor(ty: &Type) -> bool {
    if matches!(ty, Type::Inference(_)) { return false; }
    let mut pending = vec![(ty, 0)];
    while let Some((ty, depth)) = pending.pop() {
        if depth > MAX_TYPE_DEPTH { return false; }
        match ty {
            Type::Any | Type::Unknown | Type::Invalid | Type::Null => return false,
            Type::List(inner) | Type::Map(inner) | Type::Optional(inner) | Type::Stream(inner) => pending.push((inner, depth + 1)),
            Type::Result(ok, error) => { pending.push((ok, depth + 1)); pending.push((error, depth + 1)); }
            Type::Record(fields) => {
                if fields.is_empty() { return false; }
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
}
