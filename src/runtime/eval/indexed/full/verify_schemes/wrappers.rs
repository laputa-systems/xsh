use super::*;
use super::applications::ApplicationSubstitution;
use super::super::schemes::{FullWrapper, FullWrapperBoundary, FullWrapperContribution, FullWrapperBinding};
use crate::sema::wrapper_operation::{WrapperRequirement, WrapperRequirementKind as K, WrapperBoundary as B, WrapperErrorContribution as C, WrapperSelection};

pub(super) fn decode(store: &FullStore, scope: &SchemeScope, encoded: FullWrapper, substitution: &ApplicationSubstitution<'_>, budget: &mut SchemeInstantiationBudget) -> Result<WrapperRequirement<SchemeType, SchemeEffectParameterId>, IrVerifyError> {
    budget.charge(1)?;
    let kind = match encoded {
        FullWrapper::Field { source, result, detail } => {
            let field = store.wrapper_fields.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper field detail is out of bounds"))?;
            let boundary = match field.boundary {
                FullWrapperBoundary::Captured => B::Captured,
                FullWrapperBoundary::Return(ty) => B::Return(substitution.ty(store, scope, ty, budget)?),
                FullWrapperBoundary::ErrorReturn(ty) => B::ErrorReturn(substitution.ty(store, scope, ty, budget)?),
                FullWrapperBoundary::ProcCompletion(ty) => B::ProcCompletion(substitution.ty(store, scope, ty, budget)?),
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
        FullWrapper::Capture { result, detail } => {
            let capture = store.wrapper_captures.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper capture detail is out of bounds"))?;
            if capture.reserved != [0; 3] { return Err(IrVerifyError::new("wrapper capture metadata is invalid")); }
            let mut contributions = Vec::new();
            for contribution in bounded(capture.contributions, &store.wrapper_contributions, "wrapper contribution range is invalid")? {
                budget.charge(1)?;
                contributions.push(match contribution {
                    FullWrapperContribution::Explicit(ty) => C::Explicit(substitution.ty(store, scope, *ty, budget)?),
                    FullWrapperContribution::Source(ty) => C::Source(substitution.ty(store, scope, *ty, budget)?),
                });
            }
            K::Capture { result: substitution.ty(store, scope, result, budget)?, contributions: contributions.into(), expected: capture.expected }
        }
    };
    Ok(WrapperRequirement { kind, span: crate::source::Span::at(store.source_id, 0) })
}

fn selection(requirement: &WrapperRequirement<SchemeType, SchemeEffectParameterId>, budget: &mut SchemeInstantiationBudget) -> Result<WrapperSelection, IrVerifyError> {
    let requirement = requirement.try_map(budget, |budget, ty| {
        budget.charge_scheme_clone(ty)?;
        ty.closed_ground_type().ok_or_else(|| IrVerifyError::new("selected wrapper retains a symbolic type"))
    }, |_, effects| Ok(effects.clone()))?;
    crate::sema::wrapper_operation::closed_wrapper_selection(&requirement)
        .map_err(|_| IrVerifyError::new("selected wrapper disagrees with its canonical source relationship"))?
        .ok_or_else(|| IrVerifyError::new("selected wrapper has no complete canonical relationship"))
}

pub(super) fn verify_ground(store: &FullStore, scope: &SchemeScope, substitution: &CallSubstitution, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    for encoded in bounded(scope.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")? {
        selection(&decode(store, scope, *encoded, &ApplicationSubstitution::Ground(substitution), budget)?, budget)?;
    }
    Ok(())
}

pub(super) fn verify_bindings(store: &FullStore, callee: &FullScheme, scope: &SchemeScope, substitution: ApplicationSubstitution<'_>, caller: Option<(&FullScheme, &SchemeScope)>, bindings: &[FullWrapperBinding], budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let requirements = bounded(callee.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?;
    if requirements.len() != bindings.len() { return Err(IrVerifyError::new("generic call wrapper binding is incomplete")); }
    for (encoded, binding) in requirements.iter().zip(bindings) {
        let requirement = decode(store, scope, *encoded, &substitution, budget)?;
        match *binding {
            FullWrapperBinding::Selected(actual) => {
                if actual != selection(&requirement, budget)? { return Err(IrVerifyError::new("selected wrapper mode disagrees with its source envelope")); }
            }
            FullWrapperBinding::Forwarded(ordinal) => {
                let Some((caller, caller_scope)) = caller else { return Err(IrVerifyError::new("forwarded wrapper has no caller owner")); };
                let required = bounded(caller.wrappers, &store.scheme_wrappers, "caller wrapper range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("forwarded wrapper ordinal is out of scope"))?;
                let required = decode(store, caller_scope, *required, &ApplicationSubstitution::Identity, budget)?;
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
            if let FullWrapperBoundary::Return(ty) | FullWrapperBoundary::ErrorReturn(ty) | FullWrapperBoundary::ProcCompletion(ty) = field.boundary { types.push(ty); }
            types
        }
        FullWrapper::Fallback { source, result, backup } => vec![source, result, backup],
        FullWrapper::Handler { source, result, detail } => {
            let handler = store.wrapper_handlers.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper handler detail is out of bounds"))?;
            vec![source, result, handler.error, handler.handler]
        }
        FullWrapper::Capture { result, detail } => {
            let capture = store.wrapper_captures.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper capture detail is out of bounds"))?;
            let mut types = vec![result];
            types.extend(bounded(capture.contributions, &store.wrapper_contributions, "wrapper contribution range is invalid")?.iter().map(|contribution| match contribution { FullWrapperContribution::Explicit(ty) | FullWrapperContribution::Source(ty) => *ty }));
            types
        }
    })
}

pub(super) struct Coverage {
    requirements: Vec<bool>, fields: Vec<bool>, handlers: Vec<bool>, captures: Vec<bool>, contributions: Vec<bool>,
}

impl Coverage {
    pub(super) fn new(store: &FullStore) -> Self {
        Self { requirements: vec![false; store.scheme_wrappers.len()], fields: vec![false; store.wrapper_fields.len()], handlers: vec![false; store.wrapper_handlers.len()], captures: vec![false; store.wrapper_captures.len()], contributions: vec![false; store.wrapper_contributions.len()] }
    }

    fn detail(coverage: &mut [bool], index: u32) -> Result<(), IrVerifyError> {
        let covered = coverage.get_mut(index as usize).ok_or_else(|| IrVerifyError::new("wrapper detail index is invalid"))?;
        if std::mem::replace(covered, true) { return Err(IrVerifyError::new("wrapper detail belongs to multiple requirements")); }
        Ok(())
    }

    pub(super) fn owner(&mut self, store: &FullStore, scheme: &FullScheme, scope: &SchemeScope, owner: usize, effects: &mut effects::EffectCoverage) -> Result<(), IrVerifyError> {
        for requirement in mark_range(scheme.wrappers, &store.scheme_wrappers, &mut self.requirements, "scheme wrapper range is invalid or overlaps")? {
            match *requirement {
                FullWrapper::Field { detail, .. } => {
                    Self::detail(&mut self.fields, detail)?;
                    let field = store.wrapper_fields[detail as usize];
                    if store.string(field.name)?.is_empty() { return Err(IrVerifyError::new("wrapper field name is empty")); }
                    effects.contract(store, owner, scope, field.effects)?;
                }
                FullWrapper::Handler { detail, .. } => Self::detail(&mut self.handlers, detail)?,
                FullWrapper::Capture { detail, .. } => {
                    Self::detail(&mut self.captures, detail)?;
                    let capture = store.wrapper_captures[detail as usize];
                    if capture.reserved != [0; 3] { return Err(IrVerifyError::new("wrapper capture metadata is invalid")); }
                    mark_range(capture.contributions, &store.wrapper_contributions, &mut self.contributions, "wrapper contributions are invalid or overlap")?;
                }
                FullWrapper::Fallback { .. } => {},
            }
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<(), IrVerifyError> {
        if [&self.requirements, &self.fields, &self.handlers, &self.captures, &self.contributions].into_iter().any(|rows| rows.iter().any(|covered| !covered)) {
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
        (FullWrapper::Capture { result, .. }, K::Capture { contributions, .. }) => {
            let mut errors = Vec::new();
            for contribution in contributions.iter() {
                budget.charge(1)?;
                let error = match contribution {
                    C::Explicit(error) => Some(error),
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
                    budget.charge_scheme_clone(error)?;
                    let Some(error) = error.closed_ground_type() else { return Ok(Vec::new()) };
                    errors.push(error);
                }
            }
            vec![(result, SchemeType::Ground(Type::captured_error_type(errors)))]
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
        FullWrapper::Capture { result, detail } => {
            let capture = store.wrapper_captures.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper capture detail is out of bounds"))?;
            let inputs = bounded(capture.contributions, &store.wrapper_contributions, "wrapper contribution range is invalid")?.iter().map(|contribution| match contribution { FullWrapperContribution::Explicit(ty) | FullWrapperContribution::Source(ty) => *ty }).collect();
            (inputs, vec![result])
        }
    })
}


pub(super) fn verify_field_site(store: &FullStore, owner: u32, scope: &SchemeScope, effect_scopes: &VerifiedSchemeScopes, ordinal: u32, source: &SchemeType, name: Name, result: &SchemeType, captured: bool, actual_boundary: &PropagationBoundary, budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
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
    match boundary {
        B::Captured if captured => {
            if !matches!(actual_boundary, PropagationBoundary::CaptureRelationship { .. }) { return Err(IrVerifyError::new("captured wrapper field has no authenticated capture relationship")); }
        },
        B::Return(expected) if !captured && body_types_equal(&expected, &returned) => {},
        B::ErrorReturn(expected) if !captured && body_types_equal(&expected, &returned) && metadata.callable_kind == crate::sema::types::CallableKind::Proc && proof.permits_direct(owner.index(), &crate::syntax::node::Effect::Error)? => {},
        B::ProcCompletion(expected) if !captured && body_types_equal(&expected, &returned) && !store.scheme(id)?.explicit_return && metadata.callable_kind == crate::sema::types::CallableKind::Proc
            && matches!(&returned, SchemeType::Result(_, error) if error.closed_ground_type() == Some(Type::Error)) => {},
        B::Producer if !captured && metadata.callable_kind == crate::sema::types::CallableKind::Stream => {},
        _ => return Err(IrVerifyError::new("wrapper field boundary disagrees with its actual owner")),
    }
    proof.require(owner.index(), &effects, captured || metadata.callable_kind == crate::sema::types::CallableKind::Pure, budget)?;
    Ok(())
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
    let Some(ordinal) = ordinal else { return Ok(PropagationBoundary::ResultError(Arc::new(error.clone()))); };
    let encoded = *bounded(scope.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("capture wrapper ordinal is out of bounds"))?;
    let requirement = decode(store, scope, encoded, &ApplicationSubstitution::Identity, budget)?;
    let K::Capture { result, .. } = requirement.kind else { return Err(IrVerifyError::new("capture instruction has another wrapper relationship")); };
    if !body_types_equal(error, &result) { return Err(IrVerifyError::new("capture result disagrees with its wrapper relationship")); }
    let request = request.ok_or_else(|| IrVerifyError::new("capture relationship has no actual capture request"))?;
    Ok(PropagationBoundary::CaptureRelationship { instruction, request })
}

pub(super) fn actual_transfer(store: &FullStore, scope: &SchemeScope, ordinal: Option<u32>, view: schema::ExprView<'_>, types: &BTreeMap<usize, SchemeType>, budget: &mut SchemeInstantiationBudget) -> Result<Option<C<SchemeType>>, IrVerifyError> {
    match view {
        schema::ExprView::Field { .. } if ordinal.is_some() => {
            let encoded = *bounded(scope.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?.get(ordinal.unwrap() as usize).ok_or_else(|| IrVerifyError::new("wrapper field ordinal is out of bounds"))?;
            let requirement = decode(store, scope, encoded, &ApplicationSubstitution::Identity, budget)?;
            if let K::Field { source, boundary: B::Captured, .. } = requirement.kind { Ok(Some(C::Source(source))) } else { Ok(None) }
        }
        schema::ExprView::Try { value, .. } => {
            let mut source = types.get(&(value.raw() as usize)).ok_or_else(|| IrVerifyError::new("captured propagation has no checked source"))?;
            while let SchemeType::Validated(_, inner) = source { budget.charge(1)?; source = inner; }
            let error = match source {
                SchemeType::Result(_, error) => { budget.charge_scheme_clone(error)?; (**error).clone() },
                SchemeType::Ground(ty) => match ty.unvalidated() { Type::Result(_, error) => { budget.charge_clone(error)?; SchemeType::Ground((**error).clone()) }, _ => return Err(IrVerifyError::new("captured propagation has no Result source")) },
                _ => return Err(IrVerifyError::new("captured propagation has no Result source")),
            };
            Ok(Some(C::Explicit(error)))
        }
        _ => Ok(None)
    }
}
