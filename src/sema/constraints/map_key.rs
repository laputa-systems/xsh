use super::{ConstraintResolutionError, TypeConstraints, TypeVariableId, Variable};
use crate::sema::types::Type;
use crate::source::Span;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MapKeyConstraintError {
    Resolution(ConstraintResolutionError),
    Invalid(Type),
}

impl From<ConstraintResolutionError> for MapKeyConstraintError {
    fn from(error: ConstraintResolutionError) -> Self { Self::Resolution(error) }
}

impl TypeConstraints {
    pub(crate) fn require_map_key(&mut self, ty: &Type, origin: Span) -> Result<(), MapKeyConstraintError> {
        self.apply_journaled(|constraints, changes| {
            match constraints.resolve_with_changes(ty, 0, changes)? {
                Type::Inference(id) => {
                    let root = constraints.representative(id, Some(changes))?;
                    if !constraints.conversion_requirements(root)?.is_satisfiable(true) { return Err(MapKeyConstraintError::Invalid(Type::Inference(root))); }
                    if let Some(record) = constraints.record_binding(root)? {
                        return Err(MapKeyConstraintError::Invalid(super::row_type(&record.shape)));
                    }
                    constraints.record(root, changes);
                    let Some(Variable::Representative(variable)) = constraints.variables.get_mut(&root) else {
                        unreachable!("a map key requirement belongs to its existing representative")
                    };
                    variable.map_key = Some(variable.map_key.map_or(origin, |previous| super::earlier_span(previous, origin)));
                    Ok(())
                }
                actual if actual.is_map_key() => Ok(()),
                actual => Err(MapKeyConstraintError::Invalid(actual)),
            }
        })
    }

    pub(crate) fn map_key_requirement_rejecting(&self, variable: &Type, value: &Type) -> Option<Span> {
        let Type::Inference(id) = variable else { return None };
        if matches!(value, Type::Inference(_)) || value.is_map_key() { return None; }
        self.map_key_requirement(*id).ok().flatten()
    }

    pub(super) fn map_key_requirement(&self, id: TypeVariableId) -> Result<Option<Span>, ConstraintResolutionError> {
        let root = self.representative_root(id)?;
        let Some(Variable::Representative(variable)) = self.variables.get(&root) else {
            unreachable!("a map key requirement belongs to its existing representative")
        };
        Ok(variable.map_key)
    }

    pub(super) fn generalized_map_keys(&self, generalization: &super::Generalization) -> Result<Vec<crate::sema::schemes::SchemeTypeParameterId>, super::SchemeOperationError> {
        let mut parameters = Vec::new();
        for (&variable, &parameter) in &generalization.type_ids {
            if self.map_key_requirement(variable)?.is_some() { parameters.push(parameter); }
        }
        parameters.sort();
        parameters.dedup();
        Ok(parameters)
    }
}

impl From<MapKeyConstraintError> for super::SchemeOperationError {
    fn from(error: MapKeyConstraintError) -> Self {
        match error {
            MapKeyConstraintError::Resolution(error) => Self::Resolution(error),
            MapKeyConstraintError::Invalid(_) => Self::Build(crate::sema::schemes::SchemeBuildError::MapKey),
        }
    }
}
