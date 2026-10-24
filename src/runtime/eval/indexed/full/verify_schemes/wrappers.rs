use super::{FullStore, SchemeScope, SchemeInstantiationBudget, SchemeType, SchemeEffectParameterId, IrVerifyError, Name, bounded, Type, body_types_equal, VerifiedSchemeScopes, PropagationBoundary, IrFunctionId, FullCallableContract, decode_scheme_type, effects, CallSubstitution, body_type_matches, FullScheme, BTreeMap, BTreeSet, SchemeTypeId, mark_range, scheme_type_contains_any, SchemeRowSubstitution, SchemeRowTail, SymbolicCallSubstitution, verify_symbolic_argument_evidence, SymbolicEvidenceSource, ScopedEffectProof, FullDecoder, schema, Arc};
use super::applications::ApplicationSubstitution;
use super::super::schemes::{FullWrapper, FullWrapperBoundary, FullWrapperContribution, FullWrapperBinding, FullOperationBinding, FullSealedOperation};
use crate::sema::wrapper_operation::{WrapperRequirement, WrapperRequirementKind as K, WrapperBoundary as B, WrapperCompletionContribution as PC, WrapperErrorContribution as C, WrapperSelection};

pub(super) fn decode(store: &FullStore, scope: &SchemeScope, encoded: FullWrapper, substitution: &ApplicationSubstitution<'_>, budget: &mut SchemeInstantiationBudget) -> Result<WrapperRequirement<SchemeType, SchemeEffectParameterId>, IrVerifyError> {
    budget.charge(1)?;
    let kind = match encoded {
        FullWrapper::Field { source, result, detail } => {
            let field = store.wrapper_fields.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper field detail is out of bounds"))?;
            let boundary = match field.boundary {
                FullWrapperBoundary::Captured => B::Captured,
                FullWrapperBoundary::Return(ty) => B::Return(substitution.ty(store, scope, ty, budget)?),
                FullWrapperBoundary::ErrorReturn(ty) => B::ErrorReturn(substitution.ty(store, scope, ty, budget)?),
                FullWrapperBoundary::ProcCompletion { returned, group } => B::ProcCompletion { returned: substitution.ty(store, scope, returned, budget)?, group: crate::sema::wrapper_operation::WrapperGroupRef::Ordinal(group) },
                FullWrapperBoundary::Producer => B::Producer,
                FullWrapperBoundary::TopLevel => B::TopLevel,
            };
            K::Field { source: substitution.ty(store, scope, source, budget)?, result: substitution.ty(store, scope, result, budget)?, name: Name::intern(store.string(field.name)?), boundary, effects: substitution.contract(store, scope, field.effects, budget)? }
        }
        FullWrapper::Fallback { source, result, backup } => K::Fallback { source: substitution.ty(store, scope, source, budget)?, result: substitution.ty(store, scope, result, budget)?, fallback: substitution.ty(store, scope, backup, budget)? },
        FullWrapper::Handler { source, result, detail } => {
            let handler = store.wrapper_handlers.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper handler detail is out of bounds"))?;
            K::Handler { source: substitution.ty(store, scope, source, budget)?, result: substitution.ty(store, scope, result, budget)?, error: substitution.ty(store, scope, handler.error, budget)?, handler: substitution.ty(store, scope, handler.handler, budget)? }
        }
        FullWrapper::IndexValidation { operation, detail } => {
            let validation = store.wrapper_index_validations.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper index-validation detail is out of bounds"))?;
            let boundary = match validation.boundary {
                FullWrapperBoundary::Captured => B::Captured,
                FullWrapperBoundary::Return(ty) => B::Return(substitution.ty(store, scope, ty, budget)?),
                FullWrapperBoundary::ErrorReturn(ty) => B::ErrorReturn(substitution.ty(store, scope, ty, budget)?),
                FullWrapperBoundary::ProcCompletion { returned, group } => B::ProcCompletion { returned: substitution.ty(store, scope, returned, budget)?, group: crate::sema::wrapper_operation::WrapperGroupRef::Ordinal(group) },
                FullWrapperBoundary::Producer => B::Producer,
                FullWrapperBoundary::TopLevel => B::TopLevel,
            };
            K::IndexValidation { operation: crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(operation), boundary }
        }
        FullWrapper::Capture { result, detail } => {
            let capture = store.wrapper_captures.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper capture detail is out of bounds"))?;
            if capture.reserved != [0; 3] { return Err(IrVerifyError::new("wrapper capture metadata is invalid")); }
            let mut contributions = Vec::new();
            for contribution in bounded(capture.contributions, &store.wrapper_contributions, "wrapper contribution range is invalid")? {
                budget.charge(1)?;
                contributions.push(match contribution {
                    FullWrapperContribution::Explicit(ty) => C::Explicit(substitution.ty(store, scope, *ty, budget)?),
                    FullWrapperContribution::Source(ty) => C::Source(substitution.ty(store, scope, *ty, budget)?),
                    FullWrapperContribution::IndexValidation(operation) => {
                        let encoded = *bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")?.get(*operation as usize).ok_or_else(|| IrVerifyError::new("capture index operation ordinal is out of scope"))?;
                        if !matches!(encoded, super::super::schemes::FullSealedOperation::Index { .. }) { return Err(IrVerifyError::new("capture index contribution names another operation")); }
                        C::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(*operation))
                    }
                    FullWrapperContribution::IndexValidationBoundary(wrapper) => {
                        let encoded = *bounded(scope.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?.get(*wrapper as usize).ok_or_else(|| IrVerifyError::new("capture index boundary ordinal is out of scope"))?;
                        if !matches!(encoded, FullWrapper::IndexValidation { .. }) { return Err(IrVerifyError::new("capture index boundary names another wrapper")); }
                        C::IndexValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Ordinal(*wrapper))
                    }
                    FullWrapperContribution::PropagatedError(_) | FullWrapperContribution::ReturnedResult(_) | FullWrapperContribution::FieldSource(_) | FullWrapperContribution::ApplicationSource(_) => return Err(IrVerifyError::new("proc completion contribution appears in a capture")),
                });
            }
            K::Capture { result: substitution.ty(store, scope, result, budget)?, contributions: contributions.into(), expected: capture.expected }
        }
        FullWrapper::ProcCompletion { returned, group, detail } => {
            let completion = store.wrapper_completions.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper completion detail is out of bounds"))?;
            let mut contributions = Vec::new();
            for contribution in bounded(completion.contributions, &store.wrapper_contributions, "wrapper completion contribution range is invalid")? {
                budget.charge(1)?;
                contributions.push(match contribution {
                    FullWrapperContribution::PropagatedError(ty) => PC::PropagatedError(substitution.ty(store, scope, *ty, budget)?),
                    FullWrapperContribution::ReturnedResult(ty) => PC::ReturnedResult(substitution.ty(store, scope, *ty, budget)?),
                    FullWrapperContribution::FieldSource(ty) => PC::FieldSource(substitution.ty(store, scope, *ty, budget)?),
                    FullWrapperContribution::ApplicationSource(ty) => PC::ApplicationSource(substitution.ty(store, scope, *ty, budget)?),
                    FullWrapperContribution::IndexValidation(ordinal) => {
                        let operation = *bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")?.get(*ordinal as usize).ok_or_else(|| IrVerifyError::new("completion index operation ordinal is out of scope"))?;
                        if !matches!(operation, super::super::schemes::FullSealedOperation::Index { .. }) { return Err(IrVerifyError::new("completion index contribution names another operation")); }
                        PC::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(*ordinal))
                    }
                    FullWrapperContribution::IndexValidationBoundary(_) | FullWrapperContribution::Explicit(_) | FullWrapperContribution::Source(_) => return Err(IrVerifyError::new("capture contribution appears in a proc completion")),
                });
            }
            K::ProcCompletion { group: crate::sema::wrapper_operation::WrapperGroupRef::Ordinal(group), returned: substitution.ty(store, scope, returned, budget)?, contributions: crate::sema::wrapper_operation::WrapperCompletionContributions::Ready(contributions.into()) }
        }
    };
    Ok(WrapperRequirement { kind, span: crate::source::Span::at(store.source_id, 0) })
}

fn resolve_capture_index_contributions(
    store: &FullStore,
    scope: &SchemeScope,
    requirement: &mut WrapperRequirement<SchemeType, SchemeEffectParameterId>,
    substitution: &ApplicationSubstitution<'_>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let K::Capture { contributions, .. } = &mut requirement.kind else { return Ok(()) };
    let mut resolved = Vec::with_capacity(contributions.len());
    for contribution in contributions.iter() {
        match contribution {
            C::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(ordinal)) => {
                if let Some(contribution) = resolve_index_validation(store, scope, *ordinal, substitution, budget)? { resolved.push(contribution); }
            }
            C::IndexValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Ordinal(ordinal)) => {
                if let Some(contribution) = resolve_index_validation_boundary(store, scope, *ordinal, substitution, budget)? { resolved.push(contribution); }
            }
            _ => resolved.push(contribution.clone()),
        }
    }
    *contributions = resolved.into();
    Ok(())
}

fn resolve_index_validation_boundary(
    store: &FullStore,
    scope: &SchemeScope,
    ordinal: u32,
    substitution: &ApplicationSubstitution<'_>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<Option<C<SchemeType>>, IrVerifyError> {
    let encoded = *bounded(scope.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("capture index boundary ordinal is out of scope"))?;
    let requirement = decode(store, scope, encoded, substitution, budget)?;
    let K::IndexValidation { operation: crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(operation), boundary } = requirement.kind else {
        return Err(IrVerifyError::new("capture index boundary names another wrapper"));
    };
    let encoded_operation = *bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")?.get(operation as usize).ok_or_else(|| IrVerifyError::new("capture index operation ordinal is out of scope"))?;
    let operation = super::operations::decode(store, scope, encoded_operation, substitution, budget)?;
    let crate::sema::sealed_operation::SealedOperation::Index { receiver, key, .. } = operation else { return Err(IrVerifyError::new("capture index boundary names another operation")); };
    if !super::operations::index_may_require_uint_validation(&receiver, &key) { return Ok(None); }
    if receiver.closed_ground_type().is_none() || key.closed_ground_type().is_none() {
        return Ok(Some(C::IndexValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Ordinal(ordinal))));
    }
    if !super::operations::index_requires_uint_validation(&receiver, &key) { return Ok(None); }
    match boundary {
        B::Captured | B::ProcCompletion { .. } => Ok(None),
        B::Producer | B::TopLevel => Ok(Some(C::Explicit(SchemeType::Ground(Type::Error)))),
        B::Return(returned) | B::ErrorReturn(returned) => match super::operations::result_error_type(&returned) {
            Some(Some(error)) if Type::Error.matches_expected(&error) => Ok(None),
            Some(Some(_)) | Some(None) => Err(IrVerifyError::new("UInt map key validation error is outside the callable Result contract")),
            None => Ok(Some(C::Explicit(SchemeType::Ground(Type::Error)))),
        },
    }
}

fn resolve_index_validation(
    store: &FullStore,
    scope: &SchemeScope,
    ordinal: u32,
    substitution: &ApplicationSubstitution<'_>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<Option<C<SchemeType>>, IrVerifyError> {
    let encoded = *bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("capture index operation ordinal is out of scope"))?;
    let operation = super::operations::decode(store, scope, encoded, substitution, budget)?;
    let crate::sema::sealed_operation::SealedOperation::Index { receiver, key, .. } = operation else { return Err(IrVerifyError::new("capture index contribution names another operation")); };
    if let (Some(receiver), Some(key)) = (receiver.closed_ground_type(), key.closed_ground_type()) {
        return Ok(crate::sema::sealed_operation::index_requires_uint_key_validation(&receiver, &key)
            .then_some(C::Explicit(SchemeType::Ground(Type::Error))));
    }
    Ok(super::operations::index_may_require_uint_validation(&receiver, &key)
        .then_some(C::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(ordinal))))
}

fn map_forwarded_index_references(
    store: &FullStore,
    scope: &SchemeScope,
    caller_scope: &SchemeScope,
    requirement: &mut WrapperRequirement<SchemeType, SchemeEffectParameterId>,
    substitution: &ApplicationSubstitution<'_>,
    operation_bindings: &[FullOperationBinding],
    wrapper_bindings: &[FullWrapperBinding],
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    if let K::IndexValidation { operation, .. } = &mut requirement.kind {
        budget.charge(1)?;
        let crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(ordinal) = operation else {
            return Err(IrVerifyError::new("forwarded index wrapper contains a source operation identity"));
        };
        let binding = operation_bindings.get(*ordinal as usize).ok_or_else(|| IrVerifyError::new("index wrapper operation binding is out of scope"))?;
        let FullOperationBinding::Forwarded(caller_ordinal) = *binding else {
            return Err(IrVerifyError::new("forwarded index wrapper has no caller operation"));
        };
        let caller_operation = bounded(caller_scope.operations, &store.scheme_operations, "caller operation range is invalid")?
            .get(caller_ordinal as usize).ok_or_else(|| IrVerifyError::new("forwarded index wrapper operation is out of scope"))?;
        if !matches!(caller_operation, FullSealedOperation::Index { .. }) { return Err(IrVerifyError::new("forwarded index wrapper names another operation")); }
        *operation = crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(caller_ordinal);
        return Ok(());
    }
    let K::Capture { contributions, .. } = &mut requirement.kind else { return Ok(()) };
    let mut mapped = Vec::with_capacity(contributions.len());
    for contribution in contributions.iter() {
        match contribution {
            C::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(ordinal)) => {
                let binding = operation_bindings.get(*ordinal as usize).ok_or_else(|| IrVerifyError::new("capture index operation binding is out of scope"))?;
                match *binding {
                    FullOperationBinding::Forwarded(caller_ordinal) => mapped.push(C::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(caller_ordinal))),
                    FullOperationBinding::Selected(_) => {
                        if let Some(contribution) = resolve_index_validation(store, scope, *ordinal, substitution, budget)? {
                            if matches!(contribution, C::IndexValidation(_)) { return Err(IrVerifyError::new("selected index operation retains a symbolic validation")); }
                            mapped.push(contribution);
                        }
                    }
                }
            }
            C::IndexValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Ordinal(ordinal)) => {
                let binding = wrapper_bindings.get(*ordinal as usize).ok_or_else(|| IrVerifyError::new("capture index boundary binding is out of scope"))?;
                match *binding {
                    FullWrapperBinding::Forwarded(caller_ordinal) => {
                        let caller_wrapper = *bounded(caller_scope.wrappers, &store.scheme_wrappers, "caller wrapper range is invalid")?
                            .get(caller_ordinal as usize)
                            .ok_or_else(|| IrVerifyError::new("forwarded capture index boundary is out of scope"))?;
                        if !matches!(caller_wrapper, FullWrapper::IndexValidation { .. }) {
                            return Err(IrVerifyError::new("forwarded capture index boundary names another wrapper"));
                        }
                        mapped.push(C::IndexValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Ordinal(caller_ordinal)));
                    }
                    FullWrapperBinding::Selected(_) => {
                        if let Some(contribution) = resolve_index_validation_boundary(store, scope, *ordinal, substitution, budget)? {
                            if matches!(contribution, C::IndexValidationBoundary(_)) { return Err(IrVerifyError::new("selected index boundary retains a symbolic requirement")); }
                            mapped.push(contribution);
                        }
                    }
                }
            }
            _ => mapped.push(contribution.clone()),
        }
    }
    *contributions = mapped.into();
    Ok(())
}

fn selection(
    store: &FullStore,
    scope: &SchemeScope,
    mut requirement: WrapperRequirement<SchemeType, SchemeEffectParameterId>,
    substitution: &ApplicationSubstitution<'_>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<WrapperSelection, IrVerifyError> {
    resolve_capture_index_contributions(store, scope, &mut requirement, substitution, budget)?;
    if let K::IndexValidation { operation: crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(operation), boundary } = &requirement.kind {
        validate_index_validation_boundary(store, scope, *operation, boundary, substitution, budget)?;
    }
    if let K::ProcCompletion { returned, contributions: crate::sema::wrapper_operation::WrapperCompletionContributions::Ready(contributions), .. } = &requirement.kind {
        let Some(actual_error) = completion_error_join(store, scope, contributions, substitution, budget)? else {
            return Err(IrVerifyError::new("selected proc completion has unresolved index validation"));
        };
        let Some(expected_error) = super::body::completion_result_error(returned, budget)? else {
            return Err(IrVerifyError::new("selected proc completion has no Result return"));
        };
        if !body_types_equal(&actual_error, &expected_error) {
            return Err(IrVerifyError::new("selected proc completion disagrees with its source errors"));
        }
        return Ok(WrapperSelection::ProcCompletion);
    }
    let requirement = requirement.try_map(budget, |budget, ty| {
        budget.charge_scheme_clone(ty)?;
        ty.closed_ground_type().ok_or_else(|| IrVerifyError::new("selected wrapper retains a symbolic type"))
    }, |_, effects| Ok(effects.clone()))?;
    crate::sema::wrapper_operation::closed_wrapper_selection(&requirement)
        .map_err(|_| IrVerifyError::new("selected wrapper disagrees with its canonical source relationship"))?
        .ok_or_else(|| IrVerifyError::new("selected wrapper has no complete canonical relationship"))
}

fn validate_index_validation_boundary(
    store: &FullStore,
    scope: &SchemeScope,
    ordinal: u32,
    boundary: &B<SchemeType>,
    substitution: &ApplicationSubstitution<'_>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let encoded = *bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("index validation operation ordinal is out of scope"))?;
    let operation = super::operations::decode(store, scope, encoded, substitution, budget)?;
    let crate::sema::sealed_operation::SealedOperation::Index { receiver, key, .. } = operation else { return Err(IrVerifyError::new("index validation wrapper names another operation")); };
    if !super::operations::index_may_require_uint_validation(&receiver, &key)
        || receiver.closed_ground_type().is_none()
        || key.closed_ground_type().is_none()
        || !super::operations::index_requires_uint_validation(&receiver, &key)
    { return Ok(()); }
    if let B::Return(returned) | B::ErrorReturn(returned) = boundary
        && super::operations::result_error_type(returned).is_some_and(|error| error.is_none_or(|error| !Type::Error.matches_expected(&error)))
    { return Err(IrVerifyError::new("UInt map key validation error is outside the callable Result contract")); }
    Ok(())
}

pub(super) fn verify_index_validation_site(
    store: &FullStore,
    owner: u32,
    scope: &SchemeScope,
    effect_scopes: &VerifiedSchemeScopes,
    wrapper_ordinal: Option<u32>,
    operation_ordinal: u32,
    receiver: &SchemeType,
    key: &SchemeType,
    expected_return: &SchemeType,
    actual_boundary: &PropagationBoundary,
    completion_group: Option<u32>,
    budget: &mut SchemeInstantiationBudget,
) -> Result<Option<PC<SchemeType>>, IrVerifyError> {
    if !super::operations::index_may_require_uint_validation(receiver, key) {
        if wrapper_ordinal.is_some() { return Err(IrVerifyError::new("index validation wrapper has no matching input domain")); }
        return Ok(None);
    }
    let Some(wrapper_ordinal) = wrapper_ordinal else {
        return Err(IrVerifyError::new("UInt map index has no retained error boundary"));
    };
    let encoded = *bounded(scope.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?
        .get(wrapper_ordinal as usize)
        .ok_or_else(|| IrVerifyError::new("index validation wrapper ordinal is out of scope"))?;
    let FullWrapper::IndexValidation { operation: actual_operation, .. } = encoded else {
        return Err(IrVerifyError::new("index instruction has another wrapper relationship"));
    };
    if actual_operation != operation_ordinal {
        return Err(IrVerifyError::new("index validation wrapper names another operation"));
    }
    let requirement = decode(store, scope, encoded, &ApplicationSubstitution::Identity, budget)?;
    let K::IndexValidation {
        operation: crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(actual_operation),
        boundary,
    } = requirement.kind else {
        return Err(IrVerifyError::new("index instruction has another wrapper relationship"));
    };
    if actual_operation != operation_ordinal {
        return Err(IrVerifyError::new("index validation wrapper names another operation"));
    }
    let captured = matches!(actual_boundary, PropagationBoundary::CaptureRelationship { .. });
    if matches!(boundary, B::Captured) != captured {
        return Err(IrVerifyError::new("index validation wrapper disagrees with its actual error boundary"));
    }
    let function_id = IrFunctionId::from_raw(owner).ok_or_else(|| IrVerifyError::new("index validation has no callable owner"))?;
    let function = store.functions.get(function_id.index()).ok_or_else(|| IrVerifyError::new("index validation owner is out of bounds"))?;
    let metadata = store.function_metadata.get(function_id.index()).ok_or_else(|| IrVerifyError::new("index validation owner metadata is missing"))?;
    let FullCallableContract::Scheme(scheme_id) = function.contract else {
        return Err(IrVerifyError::new("index validation has no scheme owner"));
    };
    let scheme = store.scheme(scheme_id)?;
    let owner_return = decode_scheme_type(store, scope, scheme.return_type, budget, 0)?;
    if !body_types_equal(&owner_return, expected_return) {
        return Err(IrVerifyError::new("index validation return differs from its callable owner"));
    }
    let mut completion = None;
    match &boundary {
        B::Captured => {}
        B::Return(returned) => {
            if captured || !body_types_equal(returned, &owner_return) {
                return Err(IrVerifyError::new("index validation return boundary disagrees with its callable owner"));
            }
        }
        B::ErrorReturn(returned) => {
            if captured
                || !body_types_equal(returned, &owner_return)
                || metadata.callable_kind != crate::sema::types::CallableKind::Proc
                || !effects::ScopedEffectProof::new(store, effect_scopes)
                    .permits_direct(function_id.index(), &crate::syntax::node::Effect::Error)?
            {
                return Err(IrVerifyError::new("index validation error effect is not owned by its callable"));
            }
        }
        B::ProcCompletion { returned, group } => {
            let crate::sema::wrapper_operation::WrapperGroupRef::Ordinal(group) = group else {
                return Err(IrVerifyError::new("index validation has an unresolved completion group"));
            };
            if captured
                || *group >= scheme.wrappers.len
                || completion_group != Some(*group)
                || scheme.explicit_return
                || metadata.callable_kind != crate::sema::types::CallableKind::Proc
                || !body_types_equal(returned, &owner_return)
            {
                return Err(IrVerifyError::new("index validation completion boundary disagrees with its callable owner"));
            }
            let completion_requirement = completion_requirement(store, scope, *group, budget)?;
            if !matches!(completion_requirement.kind, K::ProcCompletion { returned: actual, .. } if body_types_equal(&actual, &owner_return)) {
                return Err(IrVerifyError::new("index validation completion group has another owner"));
            }
            completion = Some(*group);
        }
        B::Producer => {
            if captured || metadata.callable_kind != crate::sema::types::CallableKind::Stream {
                return Err(IrVerifyError::new("index validation producer boundary disagrees with its callable owner"));
            }
        }
        B::TopLevel => {
            return Err(IrVerifyError::new("index validation top-level boundary has a callable owner"));
        }
    }
    if let B::Return(returned) | B::ErrorReturn(returned) = &boundary
        && super::operations::index_requires_uint_validation(receiver, key)
        && super::operations::result_error_type(returned).is_some_and(|error| error.is_none_or(|error| !Type::Error.matches_expected(&error)))
    {
        return Err(IrVerifyError::new("UInt map key validation error is outside the callable Result contract"));
    }
    Ok(completion.map(|_| PC::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(operation_ordinal))))
}

pub(super) fn verify_ground(store: &FullStore, scope: &SchemeScope, substitution: &CallSubstitution, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    for encoded in bounded(scope.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")? {
        selection(store, scope, decode(store, scope, *encoded, &ApplicationSubstitution::Ground(substitution), budget)?, &ApplicationSubstitution::Ground(substitution), budget)?;
    }
    Ok(())
}

fn completion_requirement(store: &FullStore, scope: &SchemeScope, ordinal: u32, budget: &mut SchemeInstantiationBudget) -> Result<WrapperRequirement<SchemeType, SchemeEffectParameterId>, IrVerifyError> {
    let encoded = *bounded(scope.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("proc completion group ordinal is out of scope"))?;
    let requirement = decode(store, scope, encoded, &ApplicationSubstitution::Identity, budget)?;
    if !matches!(requirement.kind, K::ProcCompletion { group: crate::sema::wrapper_operation::WrapperGroupRef::Ordinal(actual), .. } if actual == ordinal) {
        return Err(IrVerifyError::new("proc completion group ordinal is invalid"));
    }
    Ok(requirement)
}

pub(super) fn completion_accepts_propagation(store: &FullStore, scope: &SchemeScope, ordinal: u32, error: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let requirement = completion_requirement(store, scope, ordinal, budget)?;
    let K::ProcCompletion { returned, contributions: crate::sema::wrapper_operation::WrapperCompletionContributions::Ready(contributions), .. } = requirement.kind else {
        return Err(IrVerifyError::new("proc propagation has an unresolved completion group"));
    };
    let SchemeType::Result(_, expected) = returned else { return Err(IrVerifyError::new("proc completion group has no Result return")); };
    if !contributions.iter().any(|contribution| matches!(contribution, PC::PropagatedError(actual) if body_types_equal(actual, error))) {
        return Err(IrVerifyError::new("proc propagation source is absent from its completion group"));
    }
    if !body_type_matches(error, &expected, budget)? { return Err(IrVerifyError::new("proc propagation source disagrees with its completion error")); }
    Ok(())
}

pub(super) fn completion_accepts_returned_result(
    store: &FullStore, scope: &SchemeScope, ordinal: u32, source: &SchemeType,
    expected_return: &SchemeType, budget: &mut SchemeInstantiationBudget,
) -> Result<bool, IrVerifyError> {
    let requirement = completion_requirement(store, scope, ordinal, budget)?;
    let K::ProcCompletion {
        returned,
        contributions: crate::sema::wrapper_operation::WrapperCompletionContributions::Ready(contributions),
        ..
    } = requirement.kind else { return Err(IrVerifyError::new("proc completion group is unresolved")); };
    if returned != *expected_return { return Ok(false); }
    let Some(actual_ok) = super::body::completion_result_success(source, budget)? else { return Ok(false); };
    let Some(expected_ok) = super::body::completion_result_success(expected_return, budget)? else { return Ok(false); };
    if !actual_ok.matches_body_return(&expected_ok) { return Ok(false); }
    for contribution in contributions.iter() {
        budget.charge(1)?;
        if matches!(contribution, PC::ReturnedResult(actual) if actual == source) { return Ok(true); }
    }
    Ok(false)
}

pub(super) fn verify_completion_groups(store: &FullStore, scheme: &FullScheme, scope: &SchemeScope, wrapper_sites: &BTreeMap<usize, u32>, expected_return: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<Option<u32>, IrVerifyError> {
    let wrappers = bounded(scheme.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?;
    let mut groups = BTreeSet::new();
    let mut saw_site = false;
    let mut referenced_groups = BTreeSet::new();
    for (ordinal, encoded) in wrappers.iter().copied().enumerate() {
        budget.charge(1)?;
        match encoded {
            FullWrapper::ProcCompletion { group: actual, .. } => {
                if saw_site || actual as usize != ordinal || !groups.insert(actual) {
                    return Err(IrVerifyError::new("proc completion group ordering is invalid"));
                }
                let requirement = decode(store, scope, encoded, &ApplicationSubstitution::Identity, budget)?;
                let K::ProcCompletion { returned: actual_return, contributions: crate::sema::wrapper_operation::WrapperCompletionContributions::Ready(_), .. } = &requirement.kind else { return Err(IrVerifyError::new("proc completion group is unresolved")); };
                if !matches!(actual_return, SchemeType::Result(..)) { return Err(IrVerifyError::new("proc completion group has no Result return")); }
            }
            FullWrapper::Field { detail, .. } => {
                saw_site = true;
                let field = store.wrapper_fields.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper field detail is out of bounds"))?;
                if let FullWrapperBoundary::ProcCompletion { returned, group: actual_group } = field.boundary {
                    let requirement = completion_requirement(store, scope, actual_group, budget)?;
                    let K::ProcCompletion { returned: group_return, .. } = requirement.kind else { unreachable!() };
                    let returned = ApplicationSubstitution::Identity.ty(store, scope, returned, budget)?;
                    if !body_types_equal(&returned, &group_return) { return Err(IrVerifyError::new("proc completion field changes its owner return")); }
                    referenced_groups.insert(actual_group);
                }
            }
            FullWrapper::IndexValidation { detail, .. } => {
                saw_site = true;
                let validation = store.wrapper_index_validations.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper index-validation detail is out of bounds"))?;
                if let FullWrapperBoundary::ProcCompletion { returned, group: actual_group } = validation.boundary {
                    let requirement = completion_requirement(store, scope, actual_group, budget)?;
                    let K::ProcCompletion { returned: group_return, .. } = requirement.kind else { unreachable!() };
                    let returned = ApplicationSubstitution::Identity.ty(store, scope, returned, budget)?;
                    if !body_types_equal(&returned, &group_return) { return Err(IrVerifyError::new("proc completion index changes its owner return")); }
                    referenced_groups.insert(actual_group);
                }
            }
            _ => saw_site = true,
        }
    }
    for group in groups.iter() {
        if !referenced_groups.contains(group) { return Err(IrVerifyError::new("proc completion group has no source relationship")); }
    }
    let mut owner_group = None;
    for &ordinal in wrapper_sites.values() {
        budget.charge(1)?;
        let encoded = *wrappers.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("body wrapper ordinal is out of bounds"))?;
        let group = match encoded {
            FullWrapper::Field { detail, .. } => {
                let field = store.wrapper_fields.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper field detail is out of bounds"))?;
                match field.boundary { FullWrapperBoundary::ProcCompletion { group, .. } => Some(group), _ => None }
            }
            FullWrapper::IndexValidation { detail, .. } => {
                let validation = store.wrapper_index_validations.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper index-validation detail is out of bounds"))?;
                match validation.boundary { FullWrapperBoundary::ProcCompletion { group, .. } => Some(group), _ => None }
            }
            _ => None,
        };
        if let Some(group) = group && owner_group.replace(group).is_some_and(|previous| previous != group) {
            return Err(IrVerifyError::new("generic body has multiple owner completion groups"));
        }
    }
    if let Some(group) = owner_group {
        if scheme.explicit_return || scheme.callable_kind != crate::sema::types::CallableKind::Proc || !saw_site {
            return Err(IrVerifyError::new("proc completion group has an invalid owner"));
        }
        let requirement = completion_requirement(store, scope, group, budget)?;
        let K::ProcCompletion { returned, .. } = requirement.kind else { unreachable!() };
        if !body_types_equal(&returned, expected_return) { return Err(IrVerifyError::new("proc completion group disagrees with its owner return")); }
    }
    Ok(owner_group)
}

pub(super) fn verify_completion_contributions(store: &FullStore, scope: &SchemeScope, ordinal: u32, actual: &[PC<SchemeType>], budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let requirement = completion_requirement(store, scope, ordinal, budget)?;
    let K::ProcCompletion { returned, contributions: crate::sema::wrapper_operation::WrapperCompletionContributions::Ready(expected), .. } = requirement.kind else {
        return Err(IrVerifyError::new("proc completion group is unresolved"));
    };
    if actual.len() != expected.len() { return Err(completion_contribution_mismatch(actual, &expected)); }
    let mut covered = vec![false; expected.len()];
    let mut candidates = BTreeMap::<String, Vec<usize>>::new();
    for (index, contribution) in expected.iter().enumerate() {
        budget.charge(1)?;
        let key = format!("{contribution:?}");
        budget.charge(key.len())?;
        candidates.entry(key).or_default().push(index);
    }
    for contribution in actual {
        budget.charge(1)?;
        let key = format!("{contribution:?}");
        budget.charge(key.len())?;
        let Some(indices) = candidates.get(&key) else { return Err(completion_contribution_mismatch(actual, &expected)); };
        let mut found = None;
        for &index in indices {
            budget.charge(1)?;
            if !covered[index] && contribution == &expected[index] { found = Some(index); break; }
        }
        let Some(index) = found else { return Err(completion_contribution_mismatch(actual, &expected)); };
        covered[index] = true;
    }
    if covered.iter().any(|covered| !covered) { return Err(completion_contribution_mismatch(actual, &expected)); }
    let SchemeType::Result(_, returned_error) = returned else { return Err(IrVerifyError::new("proc completion group has no Result return")); };
    if let Some(actual_error) = completion_error_join(store, scope, &expected, &ApplicationSubstitution::Identity, budget)?
        && !body_types_equal(&actual_error, &returned_error)
    { return Err(IrVerifyError::new("proc completion error join disagrees with its actual body")); }
    Ok(())
}

fn completion_contribution_mismatch(actual: &[PC<SchemeType>], expected: &[PC<SchemeType>]) -> IrVerifyError {
    let sample = |contributions: &[PC<SchemeType>]| {
        let rendered = contributions.iter().take(16).map(|contribution| format!("{contribution:?}")).collect::<Vec<_>>().join(", ");
        format!("{} [{}{}]", contributions.len(), rendered, if contributions.len() > 16 { ", …" } else { "" })
    };
    IrVerifyError::new(format!("proc completion contributions differ: actual {}, expected {}", sample(actual), sample(expected)))
}

fn completion_error_join(store: &FullStore, scope: &SchemeScope, contributions: &[PC<SchemeType>], substitution: &ApplicationSubstitution<'_>, budget: &mut SchemeInstantiationBudget) -> Result<Option<SchemeType>, IrVerifyError> {
    use crate::sema::wrapper_operation::WrapperProjection as P;
    let mut errors = Vec::new();
    for contribution in contributions {
        budget.charge(1)?;
        match contribution {
            PC::PropagatedError(error) => errors.push(clone_projection_type(error, budget)?),
            PC::ReturnedResult(source) => {
                let Some(P::Result { error, .. }) = source_parts(source, budget)? else { return Err(IrVerifyError::new("proc completion return source is not a Result")); };
                errors.push(error);
            }
            PC::FieldSource(source) => match source_parts(source, budget)? {
                Some(P::Optional(_)) => {}
                Some(P::Result { error, .. }) => errors.push(error),
                None => return Ok(None),
            },
            PC::ApplicationSource(source) => match source_parts(source, budget)? {
                Some(P::Result { error, .. }) => errors.push(error),
                Some(P::Optional(_)) => {}
                None => return Ok(None),
            },
            PC::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(ordinal)) => {
                let encoded = *bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")?.get(*ordinal as usize).ok_or_else(|| IrVerifyError::new("completion index operation ordinal is out of scope"))?;
                let operation = super::operations::decode(store, scope, encoded, substitution, budget)?;
                let crate::sema::sealed_operation::SealedOperation::Index { receiver, key, .. } = operation else { return Err(IrVerifyError::new("completion index contribution names another operation")); };
                if super::operations::index_may_require_uint_validation(&receiver, &key) {
                    if receiver.closed_ground_type().is_none() || key.closed_ground_type().is_none() { return Ok(None); }
                    if super::operations::index_requires_uint_validation(&receiver, &key) { errors.push(SchemeType::Ground(Type::Error)); }
                }
            }
            PC::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Source(_)) => return Err(IrVerifyError::new("source operation reference appears in a verified scheme")),
        }
    }
    if errors.is_empty() { return Ok(Some(SchemeType::Ground(Type::Error))); }
    budget.charge(errors.len())?;
    if errors.iter().all(|error| body_types_equal(error, &errors[0])) { return Ok(Some(clone_projection_type(&errors[0], budget)?)); }
    let mut ground = Vec::with_capacity(errors.len());
    for error in &errors {
        budget.charge_scheme_clone(error)?;
        let Some(error) = error.closed_ground_type() else { return Ok(None); };
        ground.push(error);
    }
    Ok(Some(SchemeType::Ground(Type::captured_error_type(ground))))
}

pub(super) fn verify_bindings(store: &FullStore, callee: &FullScheme, scope: &SchemeScope, substitution: ApplicationSubstitution<'_>, caller: Option<(&FullScheme, &SchemeScope)>, operation_bindings: &[FullOperationBinding], bindings: &[FullWrapperBinding], budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let requirements = bounded(callee.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?;
    if requirements.len() != bindings.len() { return Err(IrVerifyError::new("generic call wrapper binding is incomplete")); }
    if bounded(callee.operations, &store.scheme_operations, "scheme operation range is invalid")?.len() != operation_bindings.len() { return Err(IrVerifyError::new("generic call operation binding is incomplete")); }
    let mut group_forwarding = BTreeMap::new();
    let mut forwarded_groups = BTreeSet::new();
    if let Some((caller, _)) = caller {
        let caller_wrappers = bounded(caller.wrappers, &store.scheme_wrappers, "caller wrapper range is invalid")?;
        for (callee_ordinal, (encoded, binding)) in requirements.iter().zip(bindings).enumerate() {
            let FullWrapper::ProcCompletion { group, .. } = encoded else { continue; };
            if *group as usize != callee_ordinal { return Err(IrVerifyError::new("callee proc completion group ordinal is invalid")); }
            match *binding {
                FullWrapperBinding::Forwarded(ordinal) => {
                    if !matches!(caller_wrappers.get(ordinal as usize), Some(FullWrapper::ProcCompletion { group: actual, .. }) if *actual == ordinal)
                        || !forwarded_groups.insert(ordinal)
                    { return Err(IrVerifyError::new("forwarded proc completion group has an invalid caller owner")); }
                    group_forwarding.insert(*group, ordinal);
                }
                FullWrapperBinding::Selected(_) => {}
            }
        }
    }
    for (encoded, binding) in requirements.iter().zip(bindings) {
        let requirement = decode(store, scope, *encoded, &substitution, budget)?;
        match *binding {
            FullWrapperBinding::Selected(actual) => {
                if actual != selection(store, scope, requirement, &substitution, budget)? { return Err(IrVerifyError::new("selected wrapper mode disagrees with its source envelope")); }
            }
            FullWrapperBinding::Forwarded(ordinal) => {
                let Some((caller, caller_scope)) = caller else { return Err(IrVerifyError::new("forwarded wrapper has no caller owner")); };
                let required = bounded(caller.wrappers, &store.scheme_wrappers, "caller wrapper range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("forwarded wrapper ordinal is out of scope"))?;
                let required = decode(store, caller_scope, *required, &ApplicationSubstitution::Identity, budget)?;
                let mut requirement = requirement;
                map_forwarded_index_references(store, scope, caller_scope, &mut requirement, &substitution, operation_bindings, bindings, budget)?;
                crate::sema::wrapper_operation::map_wrapper_groups(&mut requirement, |group| match group {
                    crate::sema::wrapper_operation::WrapperGroupRef::Ordinal(callee_group) => group_forwarding.get(&callee_group).copied()
                        .map(crate::sema::wrapper_operation::WrapperGroupRef::Ordinal)
                        .ok_or_else(|| IrVerifyError::new("forwarded wrapper refers to an unforwarded proc completion group")),
                    crate::sema::wrapper_operation::WrapperGroupRef::Source(_) => Err(IrVerifyError::new("forwarded wrapper contains a source group identity")),
                })?;
                if requirement.shape() != required.shape() || !requirement.types().into_iter().zip(required.types()).all(|(actual, expected)| body_types_equal(actual, expected))
                    || match (&requirement.kind, &required.kind) { (K::Field { effects: actual, .. }, K::Field { effects: expected, .. }) => actual != expected, _ => false }
                { return Err(IrVerifyError::new("forwarded wrapper differs from its caller relationship")); }
            }
        }
    }
    Ok(())
}

pub(super) fn encoded_types(store: &FullStore, encoded: FullWrapper) -> Result<Vec<SchemeTypeId>, IrVerifyError> {
    Ok(match encoded {
        FullWrapper::Field { source, result, detail } => {
            let field = store.wrapper_fields.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper field detail is out of bounds"))?;
            let mut types = vec![source, result];
            if let FullWrapperBoundary::Return(ty) | FullWrapperBoundary::ErrorReturn(ty) = field.boundary { types.push(ty); }
            if let FullWrapperBoundary::ProcCompletion { returned, .. } = field.boundary { types.push(returned); }
            types
        }
        FullWrapper::Fallback { source, result, backup } => vec![source, result, backup],
        FullWrapper::Handler { source, result, detail } => {
            let handler = store.wrapper_handlers.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper handler detail is out of bounds"))?;
            vec![source, result, handler.error, handler.handler]
        }
        FullWrapper::IndexValidation { detail, .. } => {
            let validation = store.wrapper_index_validations.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper index-validation detail is out of bounds"))?;
            match validation.boundary {
                FullWrapperBoundary::Return(ty) | FullWrapperBoundary::ErrorReturn(ty) => vec![ty],
                FullWrapperBoundary::ProcCompletion { returned, .. } => vec![returned],
                FullWrapperBoundary::Captured | FullWrapperBoundary::Producer | FullWrapperBoundary::TopLevel => Vec::new(),
            }
        }
        FullWrapper::Capture { result, detail } => {
            let capture = store.wrapper_captures.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper capture detail is out of bounds"))?;
            let mut types = vec![result];
            for contribution in bounded(capture.contributions, &store.wrapper_contributions, "wrapper contribution range is invalid")? {
                types.push(match contribution {
                    FullWrapperContribution::Explicit(ty) | FullWrapperContribution::Source(ty) => *ty,
                    FullWrapperContribution::IndexValidation(_) | FullWrapperContribution::IndexValidationBoundary(_) => continue,
                    FullWrapperContribution::PropagatedError(_) | FullWrapperContribution::ReturnedResult(_) | FullWrapperContribution::FieldSource(_) | FullWrapperContribution::ApplicationSource(_) => return Err(IrVerifyError::new("proc completion contribution appears in a capture")),
                });
            }
            types
        }
        FullWrapper::ProcCompletion { returned, detail, .. } => {
            let completion = store.wrapper_completions.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper completion detail is out of bounds"))?;
            let mut types = vec![returned];
            for contribution in bounded(completion.contributions, &store.wrapper_contributions, "wrapper completion contribution range is invalid")? {
                types.push(match contribution {
                    FullWrapperContribution::PropagatedError(ty) | FullWrapperContribution::ReturnedResult(ty) | FullWrapperContribution::FieldSource(ty) | FullWrapperContribution::ApplicationSource(ty) => *ty,
                    FullWrapperContribution::Explicit(_) | FullWrapperContribution::Source(_) | FullWrapperContribution::IndexValidationBoundary(_) => return Err(IrVerifyError::new("capture contribution appears in a proc completion")),
                    FullWrapperContribution::IndexValidation(_) => continue,
                });
            }
            types
        }
    })
}

pub(super) struct Coverage {
    requirements: Vec<bool>, fields: Vec<bool>, handlers: Vec<bool>, captures: Vec<bool>, completions: Vec<bool>, index_validations: Vec<bool>, contributions: Vec<bool>,
}

impl Coverage {
    pub(super) fn new(store: &FullStore) -> Self {
        Self { requirements: vec![false; store.scheme_wrappers.len()], fields: vec![false; store.wrapper_fields.len()], handlers: vec![false; store.wrapper_handlers.len()], captures: vec![false; store.wrapper_captures.len()], completions: vec![false; store.wrapper_completions.len()], index_validations: vec![false; store.wrapper_index_validations.len()], contributions: vec![false; store.wrapper_contributions.len()] }
    }

    fn detail(coverage: &mut [bool], index: u32) -> Result<(), IrVerifyError> {
        let covered = coverage.get_mut(index as usize).ok_or_else(|| IrVerifyError::new("wrapper detail index is invalid"))?;
        if std::mem::replace(covered, true) { return Err(IrVerifyError::new("wrapper detail belongs to multiple requirements")); }
        Ok(())
    }

    pub(super) fn owner(&mut self, store: &FullStore, scheme: &FullScheme, scope: &SchemeScope, owner: usize, effects: &mut effects::EffectCoverage) -> Result<(), IrVerifyError> {
        for (ordinal, requirement) in mark_range(scheme.wrappers, &store.scheme_wrappers, &mut self.requirements, "scheme wrapper range is invalid or overlaps")?.iter().enumerate() {
            match *requirement {
                FullWrapper::Field { detail, .. } => {
                    Self::detail(&mut self.fields, detail)?;
                    let field = store.wrapper_fields[detail as usize];
                    if store.string(field.name)?.is_empty() { return Err(IrVerifyError::new("wrapper field name is empty")); }
                    effects.contract(store, owner, scope, field.effects)?;
                }
                FullWrapper::Handler { detail, .. } => Self::detail(&mut self.handlers, detail)?,
                FullWrapper::IndexValidation { operation, detail } => {
                    Self::detail(&mut self.index_validations, detail)?;
                    if operation >= scope.operations.len { return Err(IrVerifyError::new("wrapper index operation ordinal is out of scope")); }
                    if !matches!(bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")?.get(operation as usize), Some(FullSealedOperation::Index { .. })) {
                        return Err(IrVerifyError::new("wrapper index requirement names another operation"));
                    }
                }
                FullWrapper::Capture { detail, .. } => {
                    Self::detail(&mut self.captures, detail)?;
                    let capture = store.wrapper_captures[detail as usize];
                    if capture.reserved != [0; 3] { return Err(IrVerifyError::new("wrapper capture metadata is invalid")); }
                    mark_range(capture.contributions, &store.wrapper_contributions, &mut self.contributions, "wrapper contributions are invalid or overlap")?;
                }
                FullWrapper::ProcCompletion { group, detail, .. } => {
                    if group as usize != ordinal { return Err(IrVerifyError::new("wrapper completion owner ordinal is invalid")); }
                    Self::detail(&mut self.completions, detail)?;
                    let completion = store.wrapper_completions[detail as usize];
                    mark_range(completion.contributions, &store.wrapper_contributions, &mut self.contributions, "wrapper completion contributions are invalid or overlap")?;
                }
                FullWrapper::Fallback { .. } => {},
            }
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<(), IrVerifyError> {
        if [&self.requirements, &self.fields, &self.handlers, &self.captures, &self.completions, &self.index_validations, &self.contributions].into_iter().any(|rows| rows.iter().any(|covered| !covered)) {
            return Err(IrVerifyError::new("wrapper tables contain unowned rows"));
        }
        Ok(())
    }
}


pub(super) fn verify_fallback_site(store: &FullStore, scope: &SchemeScope, ordinal: u32, source: &SchemeType, backup: &SchemeType, result: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let encoded = *bounded(scope.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("body wrapper ordinal is out of bounds"))?;
    let requirement = decode(store, scope, encoded, &ApplicationSubstitution::Identity, budget)?;
    let K::Fallback { source: expected_source, fallback: expected_backup, result: expected_result } = requirement.kind else { return Err(IrVerifyError::new("fallback instruction has another wrapper relationship")); };
    if !body_types_equal(source, &expected_source) || !body_type_matches(backup, &expected_backup, budget)? || !body_types_equal(result, &expected_result) {
        return Err(IrVerifyError::new("fallback instruction disagrees with its wrapper relationship"));
    }
    Ok(())
}


fn clone_projection_type(ty: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<SchemeType, IrVerifyError> {
    budget.charge_scheme_clone(ty)?;
    Ok(ty.clone())
}

fn source_parts(source: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<Option<crate::sema::wrapper_operation::WrapperProjection<SchemeType>>, IrVerifyError> {
    use crate::sema::wrapper_operation::WrapperProjection as P;
    Ok(Some(match source {
        SchemeType::Optional(value) => P::Optional(clone_projection_type(value, budget)?),
        SchemeType::Result(value, error) => P::Result { value: clone_projection_type(value, budget)?, error: clone_projection_type(error, budget)? },
        SchemeType::Ground(Type::Optional(value)) => { budget.charge_clone(value)?; P::Optional(SchemeType::Ground((**value).clone())) },
        SchemeType::Ground(Type::Result(value, error)) => {
            budget.charge_clone(value)?;
            budget.charge_clone(error)?;
            P::Result { value: SchemeType::Ground((**value).clone()), error: SchemeType::Ground((**error).clone()) }
        }
        _ => return Ok(None),
    }))
}

fn result_error_id(store: &FullStore, id: SchemeTypeId) -> Result<SchemeTypeId, IrVerifyError> {
    let node = store.scheme_types.get(id.index()).ok_or_else(|| IrVerifyError::new("wrapper result type is out of bounds"))?;
    if node.tag != super::super::schema::FullSchemeTypeTag::Result { return Err(IrVerifyError::new("wrapper completion return is not a Result")); }
    SchemeTypeId::from_raw(node.second)
}

fn optional_type(ty: &SchemeType) -> bool {
    matches!(ty, SchemeType::Optional(_) | SchemeType::Ground(Type::Optional(_)))
}

fn projected_field(mut source: &SchemeType, name: Name, budget: &mut SchemeInstantiationBudget) -> Result<Option<SchemeType>, IrVerifyError> {
    while let SchemeType::Validated(_, base) = source { budget.charge(1)?; source = base; }
    match source {
        SchemeType::Record { fields, .. } => fields.get(&name).map(|ty| clone_projection_type(ty, budget)).transpose(),
        SchemeType::Ground(ty) => {
            budget.charge_clone(ty)?;
            Ok(crate::sema::wrapper_operation::wrapper_field_type(ty.unvalidated(), name).map(SchemeType::Ground))
        }
        _ => Ok(None),
    }
}

fn output_projections(store: &FullStore, scope: &SchemeScope, encoded: FullWrapper, substitution: &ApplicationSubstitution<'_>, budget: &mut SchemeInstantiationBudget) -> Result<Vec<(SchemeTypeId, SchemeType)>, IrVerifyError> {
    use crate::sema::wrapper_operation::WrapperProjection as P;
    let actual = decode(store, scope, encoded, substitution, budget)?;
    Ok(match (encoded, actual.kind) {
        (FullWrapper::Field { result, .. }, K::Field { source, name, .. }) => {
            let Some(source) = source_parts(&source, budget)? else { return Ok(Vec::new()) };
            if matches!(&source, P::Result { value, .. } if optional_type(value)) { return Ok(Vec::new()); }
            let Some(field) = projected_field(source.value(), name, budget)? else { return Ok(Vec::new()) };
            let field = if matches!(source, P::Optional(_)) && !optional_type(&field) { SchemeType::Optional(Box::new(field)) } else { field };
            vec![(result, field)]
        }
        (FullWrapper::Fallback { result, backup, .. }, K::Fallback { source, .. }) => {
            let Some(source) = source_parts(&source, budget)? else { return Ok(Vec::new()) };
            vec![(result, clone_projection_type(source.value(), budget)?), (backup, clone_projection_type(source.value(), budget)?)]
        }
        (FullWrapper::Handler { result, detail, .. }, K::Handler { source, .. }) => {
            let handler = store.wrapper_handlers.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper handler detail is out of bounds"))?;
            let Some(P::Result { value, error }) = source_parts(&source, budget)? else { return Ok(Vec::new()) };
            vec![(result, clone_projection_type(&value, budget)?), (handler.error, error), (handler.handler, value)]
        }
        (FullWrapper::IndexValidation { .. }, K::IndexValidation { .. }) => Vec::new(),
        (FullWrapper::Capture { result, .. }, K::Capture { contributions, .. }) => {
            let mut errors = Vec::new();
            for contribution in contributions.iter() {
                budget.charge(1)?;
                let error = match contribution {
                    C::Explicit(error) => {
                        budget.charge_scheme_clone(error)?;
                        let Some(error) = error.closed_ground_type() else { return Ok(Vec::new()) };
                        Some(error)
                    }
                    C::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(operation)) => {
                        let encoded = *bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")?.get(*operation as usize).ok_or_else(|| IrVerifyError::new("capture index operation ordinal is out of bounds"))?;
                        let operation = super::operations::decode(store, scope, encoded, substitution, budget)?;
                        let crate::sema::sealed_operation::SealedOperation::Index { receiver, key, .. } = operation else { return Err(IrVerifyError::new("capture index reference names another operation")); };
                        let Some(receiver_ground) = receiver.closed_ground_type() else { return Ok(Vec::new()) };
                        let Some(key_ground) = key.closed_ground_type() else { return Ok(Vec::new()) };
                        if crate::sema::sealed_operation::index_requires_uint_key_validation(&receiver_ground, &key_ground) {
                            errors.push(Type::Error);
                        } else if super::operations::index_may_require_uint_validation(&receiver, &key) {
                            return Ok(Vec::new());
                        }
                        continue;
                    }
                    C::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Source(_)) => return Err(IrVerifyError::new("source operation reference appears in a verified scheme")),
                    C::IndexValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Ordinal(ordinal)) => {
                        match resolve_index_validation_boundary(store, scope, *ordinal, substitution, budget)? {
                            Some(C::Explicit(error)) => {
                                let Some(error) = error.closed_ground_type() else { return Ok(Vec::new()) };
                                Some(error)
                            }
                            Some(C::IndexValidationBoundary(_)) => return Ok(Vec::new()),
                            Some(_) => return Err(IrVerifyError::new("capture index boundary resolved to another contribution")),
                            None => continue,
                        }
                    }
                    C::IndexValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Source(_)) => return Err(IrVerifyError::new("source wrapper reference appears in a verified scheme")),
                    C::Source(source) => match source_parts(source, budget)? {
                        Some(P::Optional(_)) => continue,
                        Some(P::Result { error, .. }) => {
                            budget.charge_scheme_clone(&error)?;
                            let Some(error) = error.closed_ground_type() else { return Ok(Vec::new()) };
                            errors.push(error);
                            continue;
                        }
                        None => return Ok(Vec::new()),
                    },
                };
                if let Some(error) = error {
                    errors.push(error);
                }
            }
            vec![(result, SchemeType::Ground(Type::captured_error_type(errors)))]
        }
        (FullWrapper::ProcCompletion { returned, detail, .. }, K::ProcCompletion { .. }) => {
            let completion = store.wrapper_completions.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper completion detail is out of bounds"))?;
            let returned_error = result_error_id(store, returned)?;
            let mut outputs = Vec::new();
            for contribution in bounded(completion.contributions, &store.wrapper_contributions, "wrapper completion contribution range is invalid")? {
                budget.charge(1)?;
                match contribution {
                    FullWrapperContribution::PropagatedError(error) => outputs.push((returned_error, substitution.ty(store, scope, *error, budget)?)),
                    FullWrapperContribution::ReturnedResult(source_id) => {
                        let source = substitution.ty(store, scope, *source_id, budget)?;
                        let Some(P::Result { error, .. }) = source_parts(&source, budget)? else { return Err(IrVerifyError::new("wrapper completion return source is not a Result")); };
                        outputs.push((returned_error, error));
                    }
                    FullWrapperContribution::FieldSource(source_id) => {
                        let source = substitution.ty(store, scope, *source_id, budget)?;
                        match source_parts(&source, budget)? {
                            Some(P::Optional(_)) => {}
                            Some(P::Result { error, .. }) => outputs.push((returned_error, error)),
                            None => {}
                        }
                    }
                    FullWrapperContribution::ApplicationSource(source_id) => {
                        let source = substitution.ty(store, scope, *source_id, budget)?;
                        if let Some(P::Result { error, .. }) = source_parts(&source, budget)? {
                            outputs.push((returned_error, error));
                        }
                    }
                    FullWrapperContribution::IndexValidation(ordinal) => {
                        let encoded = *bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")?.get(*ordinal as usize).ok_or_else(|| IrVerifyError::new("completion index operation ordinal is out of scope"))?;
                        let operation = super::operations::decode(store, scope, encoded, substitution, budget)?;
                        let crate::sema::sealed_operation::SealedOperation::Index { receiver, key, .. } = operation else { return Err(IrVerifyError::new("completion index contribution names another operation")); };
                        if super::operations::index_may_require_uint_validation(&receiver, &key) {
                            if receiver.closed_ground_type().is_none() || key.closed_ground_type().is_none() { return Ok(Vec::new()); }
                            if super::operations::index_requires_uint_validation(&receiver, &key) {
                                outputs.push((returned_error, SchemeType::Ground(Type::Error)));
                            }
                        }
                    }
                    FullWrapperContribution::Explicit(_) | FullWrapperContribution::Source(_) | FullWrapperContribution::IndexValidationBoundary(_) => return Err(IrVerifyError::new("capture contribution appears in a proc completion")),
                }
            }
            outputs
        }
        _ => return Err(IrVerifyError::new("wrapper projection has another operation relationship")),
    })
}

pub(super) enum BinderProjections {
    Static,
    Dynamic { binders: BTreeSet<usize>, outputs: Vec<BTreeSet<usize>> },
}

impl BinderProjections {
    pub(super) fn permits(&self, wrapper: usize, binder: usize) -> bool {
        match self {
            Self::Static => true,
            Self::Dynamic { binders, outputs } => !binders.contains(&binder) || outputs[wrapper].contains(&binder),
        }
    }
}

pub(super) fn binder_projections(store: &FullStore, scheme: &FullScheme, scope: &SchemeScope, substitution: &ApplicationSubstitution<'_>, budget: &mut SchemeInstantiationBudget) -> Result<BinderProjections, IrVerifyError> {
    let wrappers = bounded(scheme.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?;
    let mut dynamic = BTreeSet::new();
    if wrappers.is_empty() { return Ok(BinderProjections::Static); }
    match substitution {
        ApplicationSubstitution::Ground(substitution) => {
            for (binder, ty) in substitution.types.iter().enumerate() {
                budget.charge_clone(ty)?;
                if ty.contains_any() { dynamic.insert(binder); }
            }
        }
        ApplicationSubstitution::Symbolic(substitution) => {
            for (binder, ty) in substitution.types.iter().enumerate() {
                if scheme_type_contains_any(ty, budget)? { dynamic.insert(binder); }
            }
        }
        ApplicationSubstitution::Identity => return Err(IrVerifyError::new("wrapper binder projection has no call substitution")),
    }
    if dynamic.is_empty() { return Ok(BinderProjections::Static); }
    let ground;
    let symbolic = match substitution {
        ApplicationSubstitution::Ground(substitution) => {
            let types = substitution.types.iter().map(|ty| { budget.charge_clone(ty)?; Ok(SchemeType::Ground(ty.clone())) }).collect::<Result<Vec<_>, IrVerifyError>>()?;
            let rows = substitution.rows.iter().map(|fields| {
                let fields = fields.iter().map(|(&name, ty)| { budget.charge_clone(ty)?; Ok((name, SchemeType::Ground(ty.clone()))) }).collect::<Result<BTreeMap<_, _>, IrVerifyError>>()?;
                Ok(SchemeRowSubstitution { fields, tail: SchemeRowTail::Closed })
            }).collect::<Result<Vec<_>, IrVerifyError>>()?;
            budget.charge(substitution.effects.len())?;
            ground = SymbolicCallSubstitution { types, rows, effects: substitution.effects.clone() };
            &ground
        }
        ApplicationSubstitution::Symbolic(substitution) => substitution,
        ApplicationSubstitution::Identity => return Err(IrVerifyError::new("wrapper binder projection has no call substitution")),
    };
    let mut outputs = Vec::new();
    for encoded in wrappers {
        budget.charge(1)?;
        let mut evidence = BTreeSet::new();
        for (root, projection) in output_projections(store, scope, *encoded, substitution, budget)? {
            let parameter = decode_scheme_type(store, scope, root, budget, 0)?;
            verify_symbolic_argument_evidence(&parameter, &projection, scope, symbolic, SymbolicEvidenceSource::Projection, &mut evidence, budget, 0)?;
        }
        outputs.push(evidence);
    }
    Ok(BinderProjections::Dynamic { binders: dynamic, outputs })
}

pub(super) fn evidence_roots(store: &FullStore, encoded: FullWrapper) -> Result<(Vec<SchemeTypeId>, Vec<SchemeTypeId>), IrVerifyError> {
    Ok(match encoded {
        FullWrapper::Field { source, result, .. } => (vec![source], vec![result]),
        FullWrapper::Fallback { source, result, backup } => (vec![source], vec![result, backup]),
        FullWrapper::Handler { source, result, detail } => {
            let handler = store.wrapper_handlers.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper handler detail is out of bounds"))?;
            (vec![source], vec![result, handler.error, handler.handler])
        }
        FullWrapper::IndexValidation { .. } => (Vec::new(), Vec::new()),
        FullWrapper::Capture { result, detail } => {
            let capture = store.wrapper_captures.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper capture detail is out of bounds"))?;
            let mut inputs = Vec::new();
            for contribution in bounded(capture.contributions, &store.wrapper_contributions, "wrapper contribution range is invalid")? {
                inputs.push(match contribution {
                    FullWrapperContribution::Explicit(ty) | FullWrapperContribution::Source(ty) => *ty,
                    FullWrapperContribution::IndexValidation(_) | FullWrapperContribution::IndexValidationBoundary(_) => continue,
                    FullWrapperContribution::PropagatedError(_) | FullWrapperContribution::ReturnedResult(_) | FullWrapperContribution::FieldSource(_) | FullWrapperContribution::ApplicationSource(_) => return Err(IrVerifyError::new("proc completion contribution appears in a capture")),
                });
            }
            (inputs, vec![result])
        }
        FullWrapper::ProcCompletion { returned, detail, .. } => {
            let completion = store.wrapper_completions.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper completion detail is out of bounds"))?;
            let mut inputs = Vec::new();
            for contribution in bounded(completion.contributions, &store.wrapper_contributions, "wrapper completion contribution range is invalid")? {
                inputs.push(match contribution {
                    FullWrapperContribution::PropagatedError(ty) | FullWrapperContribution::ReturnedResult(ty) | FullWrapperContribution::FieldSource(ty) | FullWrapperContribution::ApplicationSource(ty) => *ty,
                    FullWrapperContribution::Explicit(_) | FullWrapperContribution::Source(_) | FullWrapperContribution::IndexValidationBoundary(_) => return Err(IrVerifyError::new("capture contribution appears in a proc completion")),
                    FullWrapperContribution::IndexValidation(_) => continue,
                });
            }
            (inputs, vec![result_error_id(store, returned)?])
        }
    })
}


pub(super) fn verify_field_site(store: &FullStore, owner: u32, scope: &SchemeScope, effect_scopes: &VerifiedSchemeScopes, ordinal: u32, source: &SchemeType, name: Name, result: &SchemeType, captured: bool, actual_boundary: &PropagationBoundary, budget: &mut SchemeInstantiationBudget) -> Result<Option<PC<SchemeType>>, IrVerifyError> {
    let encoded = *bounded(scope.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("body wrapper ordinal is out of bounds"))?;
    let requirement = decode(store, scope, encoded, &ApplicationSubstitution::Identity, budget)?;
    let K::Field { source: expected_source, name: expected_name, result: expected_result, boundary, effects } = requirement.kind else { return Err(IrVerifyError::new("field instruction has another wrapper relationship")); };
    if name != expected_name || !body_types_equal(source, &expected_source) || !body_types_equal(result, &expected_result) { return Err(IrVerifyError::new("field instruction disagrees with its wrapper relationship")); }
    let owner = IrFunctionId::from_raw(owner).ok_or_else(|| IrVerifyError::new("wrapper field has no function owner"))?;
    let declaration = store.functions.get(owner.index()).ok_or_else(|| IrVerifyError::new("wrapper field owner is out of bounds"))?;
    let metadata = store.function_metadata.get(owner.index()).ok_or_else(|| IrVerifyError::new("wrapper field owner metadata is missing"))?;
    let FullCallableContract::Scheme(id) = declaration.contract else { return Err(IrVerifyError::new("wrapper field has no scheme owner")); };
    let returned = decode_scheme_type(store, scope, store.scheme(id)?.return_type, budget, 0)?;
    let proof = ScopedEffectProof::new(store, effect_scopes);
    let completion_source = match boundary {
        B::Captured if captured => {
            if !matches!(actual_boundary, PropagationBoundary::CaptureRelationship { .. }) { return Err(IrVerifyError::new("captured wrapper field has no authenticated capture relationship")); }
            None
        },
        B::Return(expected) if !captured && body_types_equal(&expected, &returned) => None,
        B::ErrorReturn(expected) if !captured && body_types_equal(&expected, &returned) && metadata.callable_kind == crate::sema::types::CallableKind::Proc && proof.permits_direct(owner.index(), &crate::syntax::node::Effect::Error)? => None,
        B::ProcCompletion { returned: expected, group } if !captured && body_types_equal(&expected, &returned) && !store.scheme(id)?.explicit_return && metadata.callable_kind == crate::sema::types::CallableKind::Proc => {
            let crate::sema::wrapper_operation::WrapperGroupRef::Ordinal(group) = group else { return Err(IrVerifyError::new("proc completion field has a source group identity")); };
            let completion = completion_requirement(store, scope, group, budget)?;
            let K::ProcCompletion { contributions: crate::sema::wrapper_operation::WrapperCompletionContributions::Ready(contributions), .. } = completion.kind else { return Err(IrVerifyError::new("proc completion field has an unresolved owner group")); };
            if !contributions.iter().any(|contribution| matches!(contribution, PC::FieldSource(actual) if body_types_equal(actual, source))) {
                return Err(IrVerifyError::new("proc completion field source is absent from its owner group"));
            }
            budget.charge_scheme_clone(source)?;
            Some(PC::FieldSource(source.clone()))
        },
        B::Producer if !captured && metadata.callable_kind == crate::sema::types::CallableKind::Stream => None,
        _ => return Err(IrVerifyError::new("wrapper field boundary disagrees with its actual owner")),
    };
    proof.require(owner.index(), &effects, captured || metadata.callable_kind == crate::sema::types::CallableKind::Pure, budget)?;
    Ok(completion_source)
}


pub(super) fn verify_handler_site(store: &FullStore, decoder: &FullDecoder<'_>, scope: &SchemeScope, ordinal: u32, source: &SchemeType, result: &SchemeType, arms: &[(u32, Option<u32>, u32)], types: &BTreeMap<usize, SchemeType>, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let encoded = *bounded(scope.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("handler wrapper ordinal is out of bounds"))?;
    let requirement = decode(store, scope, encoded, &ApplicationSubstitution::Identity, budget)?;
    let K::Handler { source: expected_source, error, handler, result: expected_result } = requirement.kind else { return Err(IrVerifyError::new("handler instruction has another wrapper relationship")); };
    if !body_types_equal(source, &expected_source) || !body_types_equal(result, &expected_result) { return Err(IrVerifyError::new("handler instruction disagrees with its wrapper relationship")); }
    let [(success_pattern, None, success), (error_pattern, None, handled)] = arms else { return Err(IrVerifyError::new("wrapper handler has no canonical result branches")); };
    let schema::PatternView::ResultOk { slot: Some(success_slot), unit_only: false } = decoder.checked_pattern_view(*success_pattern)? else { return Err(IrVerifyError::new("wrapper handler has no canonical success binding")); };
    let schema::PatternView::ResultErr { slot: Some(_), unit_only: false } = decoder.checked_pattern_view(*error_pattern)? else { return Err(IrVerifyError::new("wrapper handler has no canonical error binding")); };
    if !matches!(decoder.expression_view(*success as usize)?, schema::ExprView::Param(slot) if slot == success_slot) { return Err(IrVerifyError::new("wrapper handler changes its success payload")); }
    let mut source = source;
    while let SchemeType::Validated(_, inner) = source { budget.charge(1)?; source = inner; }
    let actual_error = match source {
        SchemeType::Result(_, error) => { budget.charge_scheme_clone(error)?; (**error).clone() },
        SchemeType::Ground(ty) => match ty.unvalidated() { Type::Result(_, error) => { budget.charge_clone(error)?; SchemeType::Ground((**error).clone()) }, _ => return Err(IrVerifyError::new("wrapper handler has no result source")) },
        _ => return Err(IrVerifyError::new("wrapper handler has no result source")),
    };
    let handled = types.get(&(*handled as usize)).ok_or_else(|| IrVerifyError::new("wrapper handler has no actual fallback type"))?;
    if !body_types_equal(&actual_error, &error) || !body_types_equal(handled, &handler) { return Err(IrVerifyError::new("wrapper handler error or fallback disagrees with its relationship")); }
    Ok(())
}

pub(super) fn capture_boundary(store: &FullStore, scope: &SchemeScope, ordinal: Option<u32>, instruction: usize, request: Option<usize>, error: &SchemeType, budget: &mut SchemeInstantiationBudget) -> Result<PropagationBoundary, IrVerifyError> {
    let Some(ordinal) = ordinal else {
        return Ok(match request {
            Some(request) => PropagationBoundary::CaptureRelationship { instruction, request },
            None => PropagationBoundary::ResultError(Arc::new(error.clone())),
        });
    };
    let encoded = *bounded(scope.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("capture wrapper ordinal is out of bounds"))?;
    let requirement = decode(store, scope, encoded, &ApplicationSubstitution::Identity, budget)?;
    let K::Capture { result, .. } = requirement.kind else { return Err(IrVerifyError::new("capture instruction has another wrapper relationship")); };
    if !body_types_equal(error, &result) { return Err(IrVerifyError::new("capture result disagrees with its wrapper relationship")); }
    let request = request.ok_or_else(|| IrVerifyError::new("capture relationship has no actual capture request"))?;
    Ok(PropagationBoundary::CaptureRelationship { instruction, request })
}

pub(super) fn actual_transfer(store: &FullStore, decoder: &FullDecoder<'_>, scope: &SchemeScope, effect_scopes: &VerifiedSchemeScopes, ordinal: Option<u32>, operation_ordinal: Option<u32>, instruction: usize, view: schema::ExprView<'_>, types: &BTreeMap<usize, SchemeType>, budget: &mut SchemeInstantiationBudget) -> Result<Vec<C<SchemeType>>, IrVerifyError> {
    match view {
        schema::ExprView::Index { base, index, .. } => {
            let (receiver, key) = if let Some(operation) = operation_ordinal {
                let encoded = *bounded(scope.operations, &store.scheme_operations, "scheme operation range is invalid")?.get(operation as usize).ok_or_else(|| IrVerifyError::new("captured index operation ordinal is out of bounds"))?;
                let operation = super::operations::decode(store, scope, encoded, &ApplicationSubstitution::Identity, budget)?;
                let crate::sema::sealed_operation::SealedOperation::Index { receiver, key, .. } = operation else { return Err(IrVerifyError::new("captured index operation ordinal names another operation")); };
                (receiver, key)
            } else {
                let receiver = types.get(&(base.raw() as usize)).ok_or_else(|| IrVerifyError::new("captured index has no checked receiver"))?.clone();
                let key = match index {
                    crate::runtime::eval::IndexOperand::Ordinary(index) => types.get(&(index.raw() as usize)).ok_or_else(|| IrVerifyError::new("captured index has no checked key"))?.clone(),
                    crate::runtime::eval::IndexOperand::NegativeLiteral(_) => SchemeType::Ground(Type::Int),
                };
                (receiver, key)
            };
            if !super::operations::index_may_require_uint_validation(&receiver, &key) { return Ok(Vec::new()); }
            if super::operations::index_requires_uint_validation(&receiver, &key) {
                return Ok(vec![C::Explicit(SchemeType::Ground(Type::Error))]);
            }
            let Some(operation) = operation_ordinal else { return Err(IrVerifyError::new("captured index may validate a UInt key but has no retained operation")); };
            Ok(vec![C::IndexValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(operation))])
        }
        schema::ExprView::Call { .. }
        | schema::ExprView::DirectPureCall { .. }
        | schema::ExprView::SelfCall { .. }
        | schema::ExprView::DynamicCall { .. } => call_index_validation_transfers(store, decoder, scope, effect_scopes, instruction, budget),
        schema::ExprView::Field { .. } if ordinal.is_some() => {
            let encoded = *bounded(scope.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?.get(ordinal.unwrap() as usize).ok_or_else(|| IrVerifyError::new("wrapper field ordinal is out of bounds"))?;
            let requirement = decode(store, scope, encoded, &ApplicationSubstitution::Identity, budget)?;
            if let K::Field { source, boundary: B::Captured, .. } = requirement.kind { Ok(vec![C::Source(source)]) } else { Ok(Vec::new()) }
        }
        schema::ExprView::Try { value, .. } => {
            let mut source = types.get(&(value.raw() as usize)).ok_or_else(|| IrVerifyError::new("captured propagation has no checked source"))?;
            while let SchemeType::Validated(_, inner) = source { budget.charge(1)?; source = inner; }
            let error = match source {
                SchemeType::Result(_, error) => { budget.charge_scheme_clone(error)?; (**error).clone() },
                SchemeType::Ground(ty) => match ty.unvalidated() { Type::Result(_, error) => { budget.charge_clone(error)?; SchemeType::Ground((**error).clone()) }, _ => return Err(IrVerifyError::new("captured propagation has no Result source")) },
                _ => return Err(IrVerifyError::new("captured propagation has no Result source")),
            };
            Ok(vec![C::Explicit(error)])
        }
        _ => Ok(Vec::new())
    }
}

fn call_index_validation_transfers(store: &FullStore, decoder: &FullDecoder<'_>, caller_scope: &SchemeScope, effect_scopes: &VerifiedSchemeScopes, instruction: usize, budget: &mut SchemeInstantiationBudget) -> Result<Vec<C<SchemeType>>, IrVerifyError> {
    let Ok(instruction) = u32::try_from(instruction) else { return Ok(Vec::new()); };
    budget.charge(1)?;
    let Ok(index) = store.call_instantiations.binary_search_by_key(&instruction, |instance| instance.instruction) else { return Ok(Vec::new()); };
    let instantiation = store.call_instantiations[index];
    let target = match decoder.expression_view(instruction as usize)? {
        schema::ExprView::Call { .. } | schema::ExprView::DirectPureCall { .. } | schema::ExprView::SelfCall { .. } => {
            store.checked_call_targets.binary_search_by_key(&instruction, |target| target.instruction).ok()
                .map(|index| store.checked_call_targets[index].function)
                .ok_or_else(|| IrVerifyError::new("captured generic call has no checked target"))?
        }
        schema::ExprView::DynamicCall { callee, .. } => {
            if store.checked_call_targets.binary_search_by_key(&instruction, |target| target.instruction).is_ok() {
                return Err(IrVerifyError::new("captured scheme-value call also has a direct target"));
            }
            let call = store.scheme_callable_calls.binary_search_by_key(&instruction, |call| call.instruction)
                .map_err(|_| IrVerifyError::new("captured scheme-value call has no callable relationship"))?;
            let call = store.scheme_callable_calls[call];
            if super::scheme_callable_source_relationship(store, decoder, callee.raw(), &effect_scopes.scopes, budget)? != call.relationship {
                return Err(IrVerifyError::new("captured scheme-value call disagrees with its callable relationship"));
            }
            store.scheme_callable_values.get(call.relationship as usize)
                .ok_or_else(|| IrVerifyError::new("captured scheme-value relationship is out of bounds"))?.canonical
        }
        _ => return Err(IrVerifyError::new("captured call transfer has another expression kind")),
    };
    let function = store.functions.get(target.index()).ok_or_else(|| IrVerifyError::new("captured generic call target is out of bounds"))?;
    let FullCallableContract::Scheme(callee_id) = function.contract else { return Err(IrVerifyError::new("captured generic call target has no scheme")); };
    let callee = store.scheme(callee_id)?;
    let callee_scope = effect_scopes.get(callee_id)?;
    let type_rows = bounded(instantiation.arguments.type_arguments, &store.call_type_arguments, "generic call type argument range is invalid")?;
    let row_rows = bounded(instantiation.arguments.row_arguments, &store.call_row_arguments, "generic call row argument range is invalid")?;
    let effect_rows = bounded(instantiation.arguments.effect_arguments, &store.call_effect_arguments, "generic call effect argument range is invalid")?;
    let substitution = super::build_symbolic_substitution(store, callee_scope, caller_scope, type_rows, row_rows, effect_rows, None, budget)?;
    let bindings = bounded(instantiation.arguments.wrapper_bindings, &store.call_wrapper_bindings, "generic call wrapper binding range is invalid")?;
    let wrappers = bounded(callee.wrappers, &store.scheme_wrappers, "generic call wrapper range is invalid")?;
    if bindings.len() != wrappers.len() { return Err(IrVerifyError::new("generic call wrapper binding is incomplete")); }
    let mut transfers = Vec::new();
    budget.charge(wrappers.len())?;
    for (ordinal, (encoded, binding)) in wrappers.iter().zip(bindings).enumerate() {
        let FullWrapper::IndexValidation { .. } = encoded else { continue; };
        let requirement = decode(store, callee_scope, *encoded, &ApplicationSubstitution::Symbolic(&substitution), budget)?;
        let K::IndexValidation { operation: crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(_), boundary } = requirement.kind else {
            return Err(IrVerifyError::new("captured index wrapper has another requirement"));
        };
        if matches!(boundary, B::Captured | B::ProcCompletion { .. }) { continue; }
        match *binding {
            FullWrapperBinding::Forwarded(caller_ordinal) => {
                let caller_wrapper = *bounded(caller_scope.wrappers, &store.scheme_wrappers, "caller wrapper range is invalid")?
                    .get(caller_ordinal as usize)
                    .ok_or_else(|| IrVerifyError::new("forwarded capture index boundary is out of scope"))?;
                if !matches!(caller_wrapper, FullWrapper::IndexValidation { .. }) {
                    return Err(IrVerifyError::new("forwarded capture index boundary names another wrapper"));
                }
                transfers.push(C::IndexValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Ordinal(caller_ordinal)));
            }
            FullWrapperBinding::Selected(_) => {
                if let Some(contribution) = resolve_index_validation_boundary(store, callee_scope, ordinal as u32, &ApplicationSubstitution::Symbolic(&substitution), budget)? {
                    match contribution {
                        C::Explicit(error) => transfers.push(C::Explicit(error)),
                        C::IndexValidationBoundary(_) => return Err(IrVerifyError::new("selected capture index boundary retains symbolic inputs")),
                        _ => return Err(IrVerifyError::new("selected capture index boundary has another contribution")),
                    }
                }
            }
        }
    }
    budget.charge(transfers.len())?;
    Ok(transfers)
}
