use super::{ConstraintResolutionError, TypeConstraints, TypeVariableId, Variable};
use crate::sema::schemes::{SchemeTypeParameterId, ValueConversion, ValueConversionSet};
use crate::sema::types::Type;
use crate::source::Span;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ConversionConstraintError {
    Resolution(ConstraintResolutionError),
    Invalid { conversion: ValueConversion, actual: Type },
    Incompatible,
}

impl From<ConstraintResolutionError> for ConversionConstraintError {
    fn from(error: ConstraintResolutionError) -> Self { Self::Resolution(error) }
}

impl TypeConstraints {
    pub(crate) fn require_conversion(&mut self, ty: &Type, conversion: ValueConversion, origin: Span) -> Result<(), ConversionConstraintError> {
        self.apply_journaled(|constraints, changes| constraints.require_conversion_inner(ty, conversion, origin, changes))
    }

    pub(super) fn require_conversion_inner(&mut self, ty: &Type, conversion: ValueConversion, _origin: Span, changes: &mut Vec<super::ConstraintChange>) -> Result<(), ConversionConstraintError> {
        match self.resolve_with_changes(ty, 0, changes)? {
            Type::Inference(id) => {
                let root = self.representative(id, Some(changes))?;
                if let Some(record) = self.record_binding(root)? {
                    return Err(ConversionConstraintError::Invalid { conversion, actual: super::row_type(&record.shape) });
                }
                let mut required = self.conversion_requirements(root)?;
                required.insert(conversion);
                if !required.is_satisfiable(self.map_key_requirement(root)?.is_some()) { return Err(ConversionConstraintError::Incompatible); }
                self.record(root, changes);
                let Some(Variable::Representative(variable)) = self.variables.get_mut(&root) else { unreachable!("a conversion requirement belongs to its existing representative") };
                variable.conversions = required;
                Ok(())
            }
            Type::List(item) if conversion == ValueConversion::Env && matches!(item.as_ref(), Type::Inference(_)) => {
                self.constrain_inner(&Type::Path, &item, _origin, false, 0, changes).map_err(|(_, _, resolution)| match resolution {
                    Some(error) => ConversionConstraintError::Resolution(error),
                    None => ConversionConstraintError::Invalid { conversion, actual: Type::List(item) },
                })
            }
            Type::List(item) if matches!(conversion, ValueConversion::ArgvOrList | ValueConversion::ArgvList) => self.require_conversion_inner(&item, ValueConversion::Argv, _origin, changes),
            Type::Union(members) if matches!(conversion, ValueConversion::Argv | ValueConversion::ArgvOrList | ValueConversion::ProcessWord) => {
                let member_conversion = if conversion == ValueConversion::ProcessWord { conversion } else { ValueConversion::Argv };
                for member in members { self.require_conversion_inner(&member, member_conversion, _origin, changes)?; }
                Ok(())
            }
            Type::Validated(value) => self.require_conversion_inner(value.base(), conversion, _origin, changes),
            actual if conversion.accepts(&actual) => Ok(()),
            actual => Err(ConversionConstraintError::Invalid { conversion, actual }),
        }
    }

    pub(crate) fn conversion_requirement_rejecting(&self, variable: &Type, value: &Type) -> Option<ValueConversion> {
        let Type::Inference(id) = variable else { return None };
        if value.contains_inference() || value.contains_recovery() { return None; }
        let required = self.conversion_requirements(*id).ok()?;
        ValueConversion::ALL.into_iter().find(|conversion| required.contains(*conversion) && !conversion.accepts(value))
    }

    pub(super) fn conversion_requirements(&self, id: TypeVariableId) -> Result<ValueConversionSet, ConstraintResolutionError> {
        let root = self.representative_root(id)?;
        let Some(Variable::Representative(variable)) = self.variables.get(&root) else { unreachable!("a conversion requirement belongs to its existing representative") };
        Ok(variable.conversions)
    }

    pub(super) fn generalized_conversions(&self, generalization: &super::Generalization) -> Result<BTreeMap<SchemeTypeParameterId, ValueConversionSet>, super::SchemeOperationError> {
        let mut parameters = BTreeMap::new();
        for (&variable, &parameter) in &generalization.type_ids {
            let conversions = self.conversion_requirements(variable)?;
            if !conversions.is_empty() { parameters.insert(parameter, conversions); }
        }
        Ok(parameters)
    }
}

impl From<ConversionConstraintError> for super::SchemeOperationError {
    fn from(error: ConversionConstraintError) -> Self {
        match error {
            ConversionConstraintError::Resolution(error) => Self::Resolution(error),
            ConversionConstraintError::Invalid { .. } | ConversionConstraintError::Incompatible => Self::Build(crate::sema::schemes::SchemeBuildError::Conversion),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    fn span() -> Span { Span::new(SourceId::new(0), 0, 1) }

    #[test]
    fn conversion_requirements_keep_distinct_domains_and_original_union_cells() {
        for conversion in ValueConversion::ALL {
            for actual in [Type::Str, Type::Path, Type::Int, Type::Bool, Type::Float, Type::List(Box::new(Type::Str)), Type::List(Box::new(Type::Path)), Type::List(Box::new(Type::Float)), Type::Union(vec![Type::Str, Type::Path]), Type::Any] {
                let mut constraints = TypeConstraints::default();
                let value = constraints.fresh(span());
                let alias = constraints.fresh(span());
                constraints.require_conversion(&value, conversion, span()).unwrap();
                constraints.constrain(&alias, &value, span()).unwrap();
                assert_eq!(constraints.constrain(&alias, &actual, span()).is_ok(), conversion.accepts(&actual), "{conversion:?}: {actual}");
                if !conversion.accepts(&actual) { assert_eq!(constraints.resolve(&value).unwrap(), value); }
            }
        }
    }

    #[test]
    fn env_conversion_preserves_the_scalar_and_path_list_domains() {
        for actual in [Type::Str, Type::Int, Type::UInt, Type::Bool, Type::Path, Type::Duration, Type::List(Box::new(Type::Path)), Type::Union(vec![Type::Str, Type::Path])] {
            let mut constraints = TypeConstraints::default();
            let value = constraints.fresh(span());
            constraints.require_conversion(&value, ValueConversion::Env, span()).unwrap();
            constraints.constrain(&value, &actual, span()).unwrap();
            assert_eq!(constraints.resolve(&value).unwrap(), actual);
        }
        for actual in [Type::Any, Type::Unknown, Type::Invalid, Type::Null, Type::Float, Type::Bytes, Type::FsRoot, Type::Result(Box::new(Type::Str), Box::new(Type::Str)), Type::List(Box::new(Type::Int)), Type::List(Box::new(Type::Str)), Type::List(Box::new(Type::Any)), Type::List(Box::new(Type::Unknown)), Type::Optional(Box::new(Type::Str))] {
            let mut constraints = TypeConstraints::default();
            assert!(constraints.require_conversion(&actual, ValueConversion::Env, span()).is_err(), "{actual}");
        }
    }

    #[test]
    fn env_conversion_journals_list_shape_and_preserves_intersections() {
        let mut constraints = TypeConstraints::default();
        let item = constraints.fresh(span());
        let list = Type::List(Box::new(item.clone()));
        assert_eq!(constraints.trial(|candidate| {
            candidate.require_conversion(&list, ValueConversion::Env, span()).unwrap();
            candidate.constrain(&item, &Type::Path, span())
        }), Ok(true));
        assert_eq!(constraints.resolve(&item).unwrap(), item);
        constraints.require_conversion(&list, ValueConversion::Env, span()).unwrap();
        assert_eq!(constraints.resolve(&item).unwrap(), Type::Path);
        for reverse in [false, true] {
            let mut constraints = TypeConstraints::default();
            let value = constraints.fresh(span());
            let requirements = if reverse { [ValueConversion::ArgvList, ValueConversion::Env] } else { [ValueConversion::Env, ValueConversion::ArgvList] };
            for requirement in requirements { constraints.require_conversion(&value, requirement, span()).unwrap(); }
            assert!(constraints.constrain(&value, &Type::List(Box::new(Type::Int)), span()).is_err());
            assert_eq!(constraints.resolve(&value).unwrap(), value);
            constraints.constrain(&value, &Type::List(Box::new(Type::Path)), span()).unwrap();
        }
        let foreign = TypeConstraints::default().fresh(span());
        let mut constraints = TypeConstraints::default();
        assert_eq!(constraints.require_conversion(&Type::List(Box::new(foreign)), ValueConversion::Env, span()), Err(ConversionConstraintError::Resolution(ConstraintResolutionError::ForeignVariable)));
    }

    #[test]
    fn conversion_requirements_journal_nested_items_trials_clones_and_foreign_owners() {
        let mut constraints = TypeConstraints::default();
        let pending = constraints.fresh(span());
        constraints.require_conversion(&pending, ValueConversion::Argv, span()).unwrap();
        constraints.constrain(&pending, &Type::Unknown, span()).unwrap();
        assert_eq!(constraints.resolve(&pending).unwrap(), pending);
        assert!(constraints.constrain(&pending, &Type::Float, span()).is_err());
        assert!(constraints.require_conversion(&Type::Unknown, ValueConversion::Argv, span()).is_err());
        let item = constraints.fresh(span());
        let list = Type::List(Box::new(item.clone()));
        assert_eq!(constraints.trial(|candidate| {
            candidate.require_conversion(&list, ValueConversion::ArgvList, span()).unwrap();
            candidate.constrain(&item, &Type::Int, span())
        }), Ok(true));
        let mut clone = constraints.clone();
        clone.require_conversion(&list, ValueConversion::ArgvList, span()).unwrap();
        assert!(clone.constrain(&item, &Type::Float, span()).is_err());
        constraints.constrain(&item, &Type::Float, span()).unwrap();
        let foreign = TypeConstraints::default().fresh(span());
        assert_eq!(constraints.require_conversion(&foreign, ValueConversion::Display, span()), Err(ConversionConstraintError::Resolution(ConstraintResolutionError::ForeignVariable)));
        let solved = constraints.fresh(span());
        constraints.constrain(&solved, &Type::Float, span()).unwrap();
        let restricted = constraints.fresh(span());
        constraints.require_conversion(&restricted, ValueConversion::Argv, span()).unwrap();
        assert!(constraints.constrain(&restricted, &solved, span()).is_err());
        let item = constraints.fresh(span());
        let source = constraints.fresh(span());
        constraints.require_conversion(&source, ValueConversion::ArgvList, span()).unwrap();
        constraints.constrain(&source, &Type::List(Box::new(item.clone())), span()).unwrap();
        assert!(constraints.constrain(&item, &Type::Float, span()).is_err());
        constraints.constrain(&item, &Type::Str, span()).unwrap();
    }

    #[test]
    fn conversion_requirements_reject_empty_intersections_in_both_orders() {
        for conversion in [ValueConversion::Display, ValueConversion::Argv] {
            for reverse in [false, true] {
                let mut constraints = TypeConstraints::default();
                let value = constraints.fresh(span());
                let (first, second) = if reverse { (ValueConversion::ArgvList, conversion) } else { (conversion, ValueConversion::ArgvList) };
                constraints.require_conversion(&value, first, span()).unwrap();
                assert_eq!(constraints.require_conversion(&value, second, span()), Err(ConversionConstraintError::Incompatible));
                let other = constraints.fresh(span());
                constraints.require_conversion(&other, second, span()).unwrap();
                assert!(constraints.constrain(&value, &other, span()).is_err());
            }
        }
        for reverse in [false, true] {
            let mut constraints = TypeConstraints::default();
            let value = constraints.fresh(span());
            if reverse {
                constraints.require_conversion(&value, ValueConversion::ArgvList, span()).unwrap();
                assert!(constraints.require_map_key(&value, span()).is_err());
            } else {
                constraints.require_map_key(&value, span()).unwrap();
                assert_eq!(constraints.require_conversion(&value, ValueConversion::ArgvList, span()), Err(ConversionConstraintError::Incompatible));
            }
        }
    }

    #[test]
    fn conversion_requirements_generalize_and_instantiate_independently() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            use crate::sema::schemes::{CallableScheme, SchemeParameter, SchemeType};
            let mut constraints = TypeConstraints::default();
            let value = constraints.fresh_at_level(span(), 1);
            constraints.require_conversion(&value, ValueConversion::Display, span()).unwrap();
            let generalized = constraints.generalize_types(&[value], 0).unwrap();
            let parameter = generalized.type_parameters[0];
            let requirements = generalized.requirements;
            let callable = CallableScheme::new(generalized.type_parameters, Vec::new(), vec![SchemeParameter { name: crate::symbol::Name::intern("value"), ty: generalized.types[0].clone(), schema_expectation: None, defaulted: false, rest: false }], SchemeType::Ground(Type::Unit), None, false, requirements.clone()).unwrap();
            for actual in [Type::Int, Type::Str, Type::Float] {
                let instance = constraints.instantiate_callable(&callable, 1, span(), crate::sema::constraints::CallableApplicationScope::Invocation).unwrap();
                constraints.constrain(&instance.params[0], &actual, span()).unwrap();
            }
            let instance = constraints.instantiate_callable(&callable, 1, span(), crate::sema::constraints::CallableApplicationScope::Invocation).unwrap();
            assert!(constraints.constrain(&instance.params[0], &Type::List(Box::new(Type::Int)), span()).is_err());
            for caller in [std::collections::BTreeMap::new(), requirements.conversion_parameters] {
                let result = crate::sema::schemes::CallInstantiation::new(&callable, BTreeMap::from([(parameter, SchemeType::Parameter(parameter))]), BTreeMap::new(), BTreeMap::new(), Vec::new(), Vec::new(), Vec::new(), &[parameter], &[], &[], &[], &[], &[], &[], &caller);
                assert_eq!(result.is_ok(), !caller.is_empty());
            }
        });
    }
}
