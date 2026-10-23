use super::types::Type;
use crate::symbol::Name;
use crate::source::Span;
use super::effects::EffectContract;
use std::sync::Arc;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use crate::sema::constraints::{SealedOperationId, WrapperOperationId};

static NEXT_WRAPPER_GROUP: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct WrapperGroupId(NonZeroU64);

impl WrapperGroupId {
    pub(crate) fn fresh() -> Self {
        let id = NEXT_WRAPPER_GROUP.try_update(Ordering::Relaxed, Ordering::Relaxed, |current| current.checked_add(1))
            .expect("wrapper group identities do not wrap");
        Self(NonZeroU64::new(id).expect("wrapper group identities are nonzero"))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum WrapperGroupRef { Source(WrapperGroupId), Ordinal(u32) }

pub(crate) fn map_wrapper_groups<T, V, E>(
    requirement: &mut WrapperRequirement<T, V>,
    mut map: impl FnMut(WrapperGroupRef) -> Result<WrapperGroupRef, E>,
) -> Result<(), E> {
    match &mut requirement.kind {
        WrapperRequirementKind::Field { boundary: WrapperBoundary::ProcCompletion { group, .. }, .. } => *group = map(*group)?,
        WrapperRequirementKind::IndexValidation { boundary: WrapperBoundary::ProcCompletion { group, .. }, .. } => *group = map(*group)?,
        WrapperRequirementKind::ProcCompletion { group, .. } => *group = map(*group)?,
        _ => {}
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WrapperBoundary<T> {
    Captured,
    Return(T),
    ErrorReturn(T),
    ProcCompletion { returned: T, group: WrapperGroupRef },
    Producer,
    TopLevel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WrapperOperationRef { Source(SealedOperationId), Ordinal(u32) }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WrapperRequirementRef { Source(WrapperOperationId), Ordinal(u32) }

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WrapperErrorContribution<T> {
    Explicit(T),
    Source(T),
    IndexValidation(WrapperOperationRef),
    IndexValidationBoundary(WrapperRequirementRef),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WrapperCompletionContribution<T> {
    PropagatedError(T),
    ReturnedResult(T),
    FieldSource(T),
    ApplicationSource(T),
    IndexValidation(WrapperOperationRef),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WrapperRequirementKind<T, V> {
    Field { source: T, name: Name, result: T, boundary: WrapperBoundary<T>, effects: EffectContract<V> },
    Fallback { source: T, fallback: T, result: T },
    Handler { source: T, error: T, handler: T, result: T },
    Capture { contributions: Arc<[WrapperErrorContribution<T>]>, result: T, expected: bool },
    IndexValidation { operation: WrapperOperationRef, boundary: WrapperBoundary<T> },
    ProcCompletion { group: WrapperGroupRef, returned: T, contributions: WrapperCompletionContributions<T> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WrapperCompletionContributions<T> { Pending, Ready(Arc<[WrapperCompletionContribution<T>]>) }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum WrapperOperationKind { Field, Fallback, Handler, Capture, IndexValidation, ProcCompletion }

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WrapperRequirement<T, V> {
    pub(crate) kind: WrapperRequirementKind<T, V>,
    pub(crate) span: Span,
}

impl<T, V> WrapperRequirement<T, V> {
    pub(crate) fn operation_kind(&self) -> WrapperOperationKind {
        match self.kind {
            WrapperRequirementKind::Field { .. } => WrapperOperationKind::Field,
            WrapperRequirementKind::Fallback { .. } => WrapperOperationKind::Fallback,
            WrapperRequirementKind::Handler { .. } => WrapperOperationKind::Handler,
            WrapperRequirementKind::Capture { .. } => WrapperOperationKind::Capture,
            WrapperRequirementKind::IndexValidation { .. } => WrapperOperationKind::IndexValidation,
            WrapperRequirementKind::ProcCompletion { .. } => WrapperOperationKind::ProcCompletion,
        }
    }

    pub(crate) fn shape(&self) -> WrapperRequirementKind<(), ()> {
        self.try_map(&mut (), |_, _| Ok::<_, std::convert::Infallible>(()), |_, _| Ok(EffectContract::finite(super::effects::EffectSet::default()))).unwrap().kind
    }

    pub(crate) fn types(&self) -> Vec<&T> {
        match &self.kind {
            WrapperRequirementKind::Field { source, result, boundary, .. } => {
                let mut types = vec![source, result];
                if let WrapperBoundary::Return(ty) | WrapperBoundary::ErrorReturn(ty) = boundary { types.push(ty); }
                if let WrapperBoundary::ProcCompletion { returned, .. } = boundary { types.push(returned); }
                types
            }
            WrapperRequirementKind::Fallback { source, fallback, result } => vec![source, fallback, result],
            WrapperRequirementKind::Handler { source, error, handler, result } => vec![source, error, handler, result],
            WrapperRequirementKind::Capture { contributions, result, .. } => contributions.iter().filter_map(|term| match term { WrapperErrorContribution::Explicit(ty) | WrapperErrorContribution::Source(ty) => Some(ty), WrapperErrorContribution::IndexValidation(_) | WrapperErrorContribution::IndexValidationBoundary(_) => None }).chain(std::iter::once(result)).collect(),
            WrapperRequirementKind::IndexValidation { boundary, .. } => match boundary {
                WrapperBoundary::Return(ty) | WrapperBoundary::ErrorReturn(ty) => vec![ty],
                WrapperBoundary::ProcCompletion { returned, .. } => vec![returned],
                WrapperBoundary::Captured | WrapperBoundary::Producer | WrapperBoundary::TopLevel => Vec::new(),
            },
            WrapperRequirementKind::ProcCompletion { returned, contributions, .. } => {
                let mut types = vec![returned];
                if let WrapperCompletionContributions::Ready(terms) = contributions {
                    types.extend(terms.iter().filter_map(|term| match term {
                        WrapperCompletionContribution::PropagatedError(ty)
                        | WrapperCompletionContribution::ReturnedResult(ty)
                        | WrapperCompletionContribution::FieldSource(ty)
                        | WrapperCompletionContribution::ApplicationSource(ty) => Some(ty),
                        WrapperCompletionContribution::IndexValidation(_) => None,
                    }));
                }
                types
            }
        }
    }

    pub(crate) fn try_map<C, U, W, E>(
        &self, context: &mut C, mut ty: impl FnMut(&mut C, &T) -> Result<U, E>,
        mut effects: impl FnMut(&mut C, &EffectContract<V>) -> Result<EffectContract<W>, E>,
    ) -> Result<WrapperRequirement<U, W>, E> {
        let kind = match &self.kind {
            WrapperRequirementKind::Field { source, name, result, boundary, effects: contract } => {
                let boundary = match boundary {
                    WrapperBoundary::Captured => WrapperBoundary::Captured,
                    WrapperBoundary::Return(value) => WrapperBoundary::Return(ty(context, value)?),
                    WrapperBoundary::ErrorReturn(value) => WrapperBoundary::ErrorReturn(ty(context, value)?),
                    WrapperBoundary::ProcCompletion { returned, group } => WrapperBoundary::ProcCompletion { returned: ty(context, returned)?, group: *group },
                    WrapperBoundary::Producer => WrapperBoundary::Producer,
                    WrapperBoundary::TopLevel => WrapperBoundary::TopLevel,
                };
                WrapperRequirementKind::Field { source: ty(context, source)?, name: *name, result: ty(context, result)?, boundary, effects: effects(context, contract)? }
            }
            WrapperRequirementKind::Fallback { source, fallback, result } => WrapperRequirementKind::Fallback { source: ty(context, source)?, fallback: ty(context, fallback)?, result: ty(context, result)? },
            WrapperRequirementKind::Handler { source, error, handler, result } => WrapperRequirementKind::Handler { source: ty(context, source)?, error: ty(context, error)?, handler: ty(context, handler)?, result: ty(context, result)? },
            WrapperRequirementKind::Capture { contributions, result, expected } => {
                let contributions = contributions.iter().map(|term| Ok(match term {
                    WrapperErrorContribution::Explicit(value) => WrapperErrorContribution::Explicit(ty(context, value)?),
                    WrapperErrorContribution::Source(value) => WrapperErrorContribution::Source(ty(context, value)?),
                    WrapperErrorContribution::IndexValidation(operation) => WrapperErrorContribution::IndexValidation(*operation),
                    WrapperErrorContribution::IndexValidationBoundary(requirement) => WrapperErrorContribution::IndexValidationBoundary(*requirement),
                })).collect::<Result<Vec<_>, E>>()?;
                WrapperRequirementKind::Capture { contributions: contributions.into(), result: ty(context, result)?, expected: *expected }
            }
            WrapperRequirementKind::IndexValidation { operation, boundary } => {
                let boundary = match boundary {
                    WrapperBoundary::Captured => WrapperBoundary::Captured,
                    WrapperBoundary::Return(value) => WrapperBoundary::Return(ty(context, value)?),
                    WrapperBoundary::ErrorReturn(value) => WrapperBoundary::ErrorReturn(ty(context, value)?),
                    WrapperBoundary::ProcCompletion { returned, group } => WrapperBoundary::ProcCompletion { returned: ty(context, returned)?, group: *group },
                    WrapperBoundary::Producer => WrapperBoundary::Producer,
                    WrapperBoundary::TopLevel => WrapperBoundary::TopLevel,
                };
                WrapperRequirementKind::IndexValidation { operation: *operation, boundary }
            }
            WrapperRequirementKind::ProcCompletion { group, returned, contributions } => {
                let contributions = match contributions {
                    WrapperCompletionContributions::Pending => WrapperCompletionContributions::Pending,
                    WrapperCompletionContributions::Ready(contributions) => {
                        let contributions = contributions.iter().map(|term| Ok(match term {
                            WrapperCompletionContribution::PropagatedError(value) => WrapperCompletionContribution::PropagatedError(ty(context, value)?),
                            WrapperCompletionContribution::ReturnedResult(value) => WrapperCompletionContribution::ReturnedResult(ty(context, value)?),
                            WrapperCompletionContribution::FieldSource(value) => WrapperCompletionContribution::FieldSource(ty(context, value)?),
                            WrapperCompletionContribution::ApplicationSource(value) => WrapperCompletionContribution::ApplicationSource(ty(context, value)?),
                            WrapperCompletionContribution::IndexValidation(operation) => WrapperCompletionContribution::IndexValidation(*operation),
                        })).collect::<Result<Vec<_>, E>>()?;
                        WrapperCompletionContributions::Ready(contributions.into())
                    }
                };
                WrapperRequirementKind::ProcCompletion { group: *group, returned: ty(context, returned)?, contributions }
            }
        };
        Ok(WrapperRequirement { kind, span: self.span })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum WrapperSelection { Optional, Result, Capture, IndexValidation, ProcCompletion }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, u8)]
pub(crate) enum WrapperBinding { Selected(WrapperSelection), Forwarded(u32) }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum WrapperMode { Optional, Result }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FallbackKind { Value, Handler }

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WrapperProjection<T> {
    Optional(T),
    Result { value: T, error: T },
}

impl<T> WrapperProjection<T> {
    pub(crate) fn mode(&self) -> WrapperMode {
        match self { Self::Optional(_) => WrapperMode::Optional, Self::Result { .. } => WrapperMode::Result }
    }

    pub(crate) fn value(&self) -> &T {
        match self { Self::Optional(value) | Self::Result { value, .. } => value }
    }
}

impl WrapperProjection<Type> {
    pub(crate) fn guarded_result(&self) -> Type {
        match self {
            Self::Optional(value) if !matches!(value, Type::Optional(_)) => Type::Optional(Box::new(value.clone())),
            Self::Optional(value) | Self::Result { value, .. } => value.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WrapperProjectionError {
    UnsupportedSource(Type),
    ResultLeavesOptionalPayload,
    HandlerNeedsResult,
    MissingField { source: Type, name: Name },
    IncompatibleValue { expected: Type, actual: Type },
    IncompatibleBoundary,
    IncompatibleEffects,
}

fn source_projection(source: &Type) -> Result<WrapperProjection<&Type>, WrapperProjectionError> {
    match source {
        Type::Optional(value) => Ok(WrapperProjection::Optional(value)),
        Type::Result(value, error) => Ok(WrapperProjection::Result { value, error }),
        _ => Err(WrapperProjectionError::UnsupportedSource(source.clone())),
    }
}

pub(crate) fn guarded_field_projection(
    source: &Type, name: Name, field_type: impl FnOnce(&Type, Name) -> Option<Type>,
) -> Result<WrapperProjection<Type>, WrapperProjectionError> {
    let source = source_projection(source)?;
    if matches!(&source, WrapperProjection::Result { value, .. } if matches!(value, Type::Optional(_))) {
        return Err(WrapperProjectionError::ResultLeavesOptionalPayload);
    }
    let value = source.value().unvalidated();
    let field = field_type(value, name).ok_or_else(|| WrapperProjectionError::MissingField { source: value.clone(), name })?;
    Ok(match source {
        WrapperProjection::Optional(_) => WrapperProjection::Optional(field),
        WrapperProjection::Result { error, .. } => WrapperProjection::Result { value: field, error: error.clone() },
    })
}

pub(crate) fn fallback_projection(source: &Type, kind: FallbackKind) -> Result<WrapperProjection<Type>, WrapperProjectionError> {
    Ok(match source_projection(source)? {
        WrapperProjection::Optional(_) if kind == FallbackKind::Handler => return Err(WrapperProjectionError::HandlerNeedsResult),
        WrapperProjection::Optional(value) => WrapperProjection::Optional(value.clone()),
        WrapperProjection::Result { value, error } => WrapperProjection::Result { value: value.clone(), error: error.clone() },
    })
}

pub(crate) fn wrapper_field_type(source: &Type, name: Name) -> Option<Type> {
    match source {
        Type::Record(fields) => fields.get(&name).cloned(),
        Type::ErasedRecord | Type::DynamicModule => Some(Type::Any),
        Type::Module(exports) => exports.get(&name)?.field_value_type(),
        _ => super::check::Checker::fixed_field_type(source, name).flatten(),
    }
}

pub(crate) fn closed_wrapper_selection<V: Ord>(requirement: &WrapperRequirement<Type, V>) -> Result<Option<WrapperSelection>, WrapperProjectionError> {
    if requirement.types().iter().any(|ty| ty.contains_inference()) { return Ok(None); }
    let fits = |actual: &Type, expected: &Type| {
        if actual.matches_expected(expected) { Ok(()) }
        else { Err(WrapperProjectionError::IncompatibleValue { expected: expected.clone(), actual: actual.clone() }) }
    };
    let mode = match &requirement.kind {
        WrapperRequirementKind::Field { source, name, result, boundary, effects } => {
            let projected = guarded_field_projection(source, *name, wrapper_field_type)?;
            fits(&projected.guarded_result(), result)?;
            let expected_effects = if matches!(&projected, WrapperProjection::Result { .. }) { super::effects::EffectSet::singleton(crate::syntax::node::Effect::Error) } else { super::effects::EffectSet::default() };
            match effects.closed() {
                Some(actual) if actual == expected_effects => {},
                Some(_) => return Err(WrapperProjectionError::IncompatibleEffects),
                None => return Ok(None),
            }
            if matches!(boundary, WrapperBoundary::ProcCompletion { returned, .. } if !matches!(returned, Type::Result(..))) { return Err(WrapperProjectionError::IncompatibleBoundary); }
            if let WrapperProjection::Result { error, .. } = &projected {
                match boundary {
                    WrapperBoundary::Return(Type::Result(_, expected)) | WrapperBoundary::ErrorReturn(Type::Result(_, expected)) => fits(error, expected)?,
                    WrapperBoundary::Return(_) => return Err(WrapperProjectionError::IncompatibleBoundary),
                    WrapperBoundary::Producer if !error.matches_expected(&Type::Error) && !error.matches_expected(&Type::Str) => return Err(WrapperProjectionError::IncompatibleBoundary),
                    _ => {},
                }
            }
            projected.mode()
        }
        WrapperRequirementKind::Fallback { source, fallback, result } => {
            let projected = fallback_projection(source, FallbackKind::Value)?;
            fits(projected.value(), result)?;
            fits(fallback, projected.value())?;
            projected.mode()
        }
        WrapperRequirementKind::Handler { source, error, handler, result } => {
            let projected = fallback_projection(source, FallbackKind::Handler)?;
            let WrapperProjection::Result { value, error: actual_error } = &projected else { unreachable!() };
            fits(value, result)?;
            fits(actual_error, error)?;
            fits(handler, value)?;
            projected.mode()
        }
        WrapperRequirementKind::Capture { contributions, result, expected } => {
            let Some(errors) = capture_error_contributions(contributions)? else { return Ok(None); };
            if *expected {
                for error in errors { fits(&error, result)?; }
            } else {
                fits(&Type::captured_error_type(errors), result)?;
            }
            return Ok(Some(WrapperSelection::Capture));
        }
        WrapperRequirementKind::IndexValidation { .. } => return Ok(Some(WrapperSelection::IndexValidation)),
        WrapperRequirementKind::ProcCompletion { returned, contributions, .. } => {
            let WrapperCompletionContributions::Ready(contributions) = contributions else { return Ok(None) };
            let Some(errors) = completion_error_contributions(contributions)? else { return Ok(None); };
            let Type::Result(_, expected) = returned else { return Err(WrapperProjectionError::IncompatibleBoundary); };
            let actual = Type::captured_error_type(errors);
            if actual != expected.as_ref().clone() { return Err(WrapperProjectionError::IncompatibleValue { expected: expected.as_ref().clone(), actual }); }
            return Ok(Some(WrapperSelection::ProcCompletion));
        }
    };
    Ok(Some(match mode { WrapperMode::Optional => WrapperSelection::Optional, WrapperMode::Result => WrapperSelection::Result }))
}

pub(crate) fn capture_error_contributions(contributions: &[WrapperErrorContribution<Type>]) -> Result<Option<Vec<Type>>, WrapperProjectionError> {
    let mut errors = Vec::new();
    for term in contributions {
        match term {
            WrapperErrorContribution::Explicit(error) => {
                if error.contains_inference() { return Ok(None); }
                errors.push(error.clone());
            }
            WrapperErrorContribution::Source(source) => {
                if matches!(source, Type::Inference(_)) { return Ok(None); }
                if let WrapperProjection::Result { error, .. } = source_projection(source)? {
                    if error.contains_inference() { return Ok(None); }
                    errors.push(error.clone());
                }
            }
            WrapperErrorContribution::IndexValidation(_) | WrapperErrorContribution::IndexValidationBoundary(_) => return Ok(None),
        }
    }
    Ok(Some(errors))
}

pub(crate) fn completion_error_contributions(contributions: &[WrapperCompletionContribution<Type>]) -> Result<Option<Vec<Type>>, WrapperProjectionError> {
    let mut errors = Vec::new();
    for contribution in contributions {
        match contribution {
            WrapperCompletionContribution::PropagatedError(error) => {
                if error.contains_inference() { return Ok(None); }
                errors.push(error.clone());
            }
            WrapperCompletionContribution::ReturnedResult(source) => {
                if matches!(source, Type::Inference(_)) { return Ok(None); }
                let WrapperProjection::Result { error, .. } = source_projection(source)? else { return Err(WrapperProjectionError::UnsupportedSource(source.clone())); };
                if error.contains_inference() { return Ok(None); }
                errors.push(error.clone());
            }
            WrapperCompletionContribution::FieldSource(source) => {
                if matches!(source, Type::Inference(_)) { return Ok(None); }
                match source_projection(source)? {
                    WrapperProjection::Optional(_) => {}
                    WrapperProjection::Result { error, .. } => {
                        if error.contains_inference() { return Ok(None); }
                        errors.push(error.clone());
                    }
                }
            }
            WrapperCompletionContribution::ApplicationSource(source) => {
                if source.contains_inference() { return Ok(None); }
                if let Type::Result(_, error) = source.unvalidated() {
                    if error.contains_inference() { return Ok(None); }
                    errors.push((**error).clone());
                }
            }
            WrapperCompletionContribution::IndexValidation(_) => return Ok(None),
        }
    }
    Ok(Some(errors))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::SymbolOwner;

    #[test]
    fn wrapper_fallback_selects_from_declared_envelope_without_unwrapping_its_payload() {
        let result = Type::Result(Box::new(Type::Int), Box::new(Type::Str));
        let optional = Type::Optional(Box::new(result.clone()));
        assert_eq!(fallback_projection(&optional, FallbackKind::Value), Ok(WrapperProjection::Optional(result.clone())));
        assert_eq!(fallback_projection(&result, FallbackKind::Value), Ok(WrapperProjection::Result { value: Type::Int, error: Type::Str }));
        assert_eq!(fallback_projection(&optional, FallbackKind::Handler), Err(WrapperProjectionError::HandlerNeedsResult));
        let nullable = Type::Result(Box::new(Type::Optional(Box::new(Type::Int))), Box::new(Type::Str));
        assert_eq!(fallback_projection(&nullable, FallbackKind::Value).unwrap().value(), &Type::Optional(Box::new(Type::Int)));
    }

    #[test]
    fn guarded_field_projection_preserves_nullability_error_identity_and_one_hop() {
        SymbolOwner::new().with_current(|| {
            let name = Name::intern("name");
            let row = Type::Record(std::sync::Arc::new(std::collections::BTreeMap::from([(name, Type::Optional(Box::new(Type::Str)))])));
            let field = |source: &Type, name| match source { Type::Record(fields) => fields.get(&name).cloned(), _ => None };
            let optional = Type::Optional(Box::new(row.clone()));
            let projected = guarded_field_projection(&optional, name, field).unwrap();
            assert_eq!(projected.mode(), WrapperMode::Optional);
            assert_eq!(projected.guarded_result(), Type::Optional(Box::new(Type::Str)));
            let result = Type::Result(Box::new(row), Box::new(Type::Str));
            let projected = guarded_field_projection(&result, name, field).unwrap();
            assert_eq!(projected.mode(), WrapperMode::Result);
            assert_eq!(projected.guarded_result(), Type::Optional(Box::new(Type::Str)));
            assert!(matches!(projected, WrapperProjection::Result { error: Type::Str, .. }));
            let nested = Type::Result(Box::new(optional), Box::new(Type::Str));
            assert_eq!(guarded_field_projection(&nested, name, field), Err(WrapperProjectionError::ResultLeavesOptionalPayload));
            assert!(matches!(guarded_field_projection(&Type::Int, name, field), Err(WrapperProjectionError::UnsupportedSource(Type::Int))));
        });
    }

    #[test]
    fn guarded_field_projection_uses_existing_fixed_field_domains() {
        SymbolOwner::new().with_current(|| {
            let field = |source: &Type, name| crate::sema::check::Checker::fixed_field_type(source, name).flatten();
            let source = Type::Optional(Box::new(Type::Path));
            let projected = guarded_field_projection(&source, Name::intern("name"), field).unwrap();
            assert_eq!(projected.guarded_result(), Type::Optional(Box::new(Type::Str)));
            let source = Type::Result(Box::new(Type::Error), Box::new(Type::Str));
            let projected = guarded_field_projection(&source, Name::intern("message"), field).unwrap();
            assert_eq!(projected, WrapperProjection::Result { value: Type::Str, error: Type::Str });
        });
    }

    #[test]
    fn application_completion_source_contributes_only_its_original_result_error() {
        let result = Type::Result(Box::new(Type::Int), Box::new(Type::Str));
        assert_eq!(completion_error_contributions(&[WrapperCompletionContribution::ApplicationSource(result)]), Ok(Some(vec![Type::Str])));
        assert_eq!(completion_error_contributions(&[WrapperCompletionContribution::ApplicationSource(Type::Int)]), Ok(Some(Vec::new())));
    }
}
