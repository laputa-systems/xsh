use super::*;
use super::applications::ApplicationSubstitution;
use super::super::schemes::{FullWrapper, FullWrapperBoundary, FullWrapperContribution, FullWrapperBinding};
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
        FullWrapper::Capture { result, detail } => {
            let capture = store.wrapper_captures.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper capture detail is out of bounds"))?;
            if capture.reserved != [0; 3] { return Err(IrVerifyError::new("wrapper capture metadata is invalid")); }
            let mut contributions = Vec::new();
            for contribution in bounded(capture.contributions, &store.wrapper_contributions, "wrapper contribution range is invalid")? {
                budget.charge(1)?;
                contributions.push(match contribution {
                    FullWrapperContribution::Explicit(ty) => C::Explicit(substitution.ty(store, scope, *ty, budget)?),
                    FullWrapperContribution::Source(ty) => C::Source(substitution.ty(store, scope, *ty, budget)?),
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
                    FullWrapperContribution::Explicit(_) | FullWrapperContribution::Source(_) => return Err(IrVerifyError::new("capture contribution appears in a proc completion")),
                });
            }
            K::ProcCompletion { group: crate::sema::wrapper_operation::WrapperGroupRef::Ordinal(group), returned: substitution.ty(store, scope, returned, budget)?, contributions: crate::sema::wrapper_operation::WrapperCompletionContributions::Ready(contributions.into()) }
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
            _ => saw_site = true,
        }
    }
    for group in groups.iter().copied() {
        if !referenced_groups.contains(&group) { return Err(IrVerifyError::new("proc completion group has no field source")); }
    }
    let mut owner_group = None;
    for &ordinal in wrapper_sites.values() {
        budget.charge(1)?;
        let encoded = *wrappers.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("body wrapper ordinal is out of bounds"))?;
        let FullWrapper::Field { detail, .. } = encoded else { continue; };
        let field = store.wrapper_fields.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper field detail is out of bounds"))?;
        if let FullWrapperBoundary::ProcCompletion { group, .. } = field.boundary
            && owner_group.replace(group).is_some_and(|previous| previous != group)
        { return Err(IrVerifyError::new("generic body has multiple owner completion groups")); }
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
    if let Some(actual_error) = completion_error_join(&expected, budget)?
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

fn completion_error_join(contributions: &[PC<SchemeType>], budget: &mut SchemeInstantiationBudget) -> Result<Option<SchemeType>, IrVerifyError> {
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

pub(super) fn verify_bindings(store: &FullStore, callee: &FullScheme, scope: &SchemeScope, substitution: ApplicationSubstitution<'_>, caller: Option<(&FullScheme, &SchemeScope)>, bindings: &[FullWrapperBinding], budget: &mut SchemeInstantiationBudget) -> Result<(), IrVerifyError> {
    let requirements = bounded(callee.wrappers, &store.scheme_wrappers, "scheme wrapper range is invalid")?;
    if requirements.len() != bindings.len() { return Err(IrVerifyError::new("generic call wrapper binding is incomplete")); }
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
                if actual != selection(&requirement, budget)? { return Err(IrVerifyError::new("selected wrapper mode disagrees with its source envelope")); }
            }
            FullWrapperBinding::Forwarded(ordinal) => {
                let Some((caller, caller_scope)) = caller else { return Err(IrVerifyError::new("forwarded wrapper has no caller owner")); };
                let required = bounded(caller.wrappers, &store.scheme_wrappers, "caller wrapper range is invalid")?.get(ordinal as usize).ok_or_else(|| IrVerifyError::new("forwarded wrapper ordinal is out of scope"))?;
                let required = decode(store, caller_scope, *required, &ApplicationSubstitution::Identity, budget)?;
                let mut requirement = requirement;
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
        FullWrapper::Capture { result, detail } => {
            let capture = store.wrapper_captures.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper capture detail is out of bounds"))?;
            let mut types = vec![result];
            for contribution in bounded(capture.contributions, &store.wrapper_contributions, "wrapper contribution range is invalid")? {
                types.push(match contribution {
                    FullWrapperContribution::Explicit(ty) | FullWrapperContribution::Source(ty) => *ty,
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
                    FullWrapperContribution::Explicit(_) | FullWrapperContribution::Source(_) => return Err(IrVerifyError::new("capture contribution appears in a proc completion")),
                });
            }
            types
        }
    })
}

pub(super) struct Coverage {
    requirements: Vec<bool>, fields: Vec<bool>, handlers: Vec<bool>, captures: Vec<bool>, completions: Vec<bool>, contributions: Vec<bool>,
}

impl Coverage {
    pub(super) fn new(store: &FullStore) -> Self {
        Self { requirements: vec![false; store.scheme_wrappers.len()], fields: vec![false; store.wrapper_fields.len()], handlers: vec![false; store.wrapper_handlers.len()], captures: vec![false; store.wrapper_captures.len()], completions: vec![false; store.wrapper_completions.len()], contributions: vec![false; store.wrapper_contributions.len()] }
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
        if [&self.requirements, &self.fields, &self.handlers, &self.captures, &self.completions, &self.contributions].into_iter().any(|rows| rows.iter().any(|covered| !covered)) {
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
                    FullWrapperContribution::Explicit(_) | FullWrapperContribution::Source(_) => return Err(IrVerifyError::new("capture contribution appears in a proc completion")),
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
        FullWrapper::Capture { result, detail } => {
            let capture = store.wrapper_captures.get(detail as usize).ok_or_else(|| IrVerifyError::new("wrapper capture detail is out of bounds"))?;
            let mut inputs = Vec::new();
            for contribution in bounded(capture.contributions, &store.wrapper_contributions, "wrapper contribution range is invalid")? {
                inputs.push(match contribution {
                    FullWrapperContribution::Explicit(ty) | FullWrapperContribution::Source(ty) => *ty,
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
                    FullWrapperContribution::Explicit(_) | FullWrapperContribution::Source(_) => return Err(IrVerifyError::new("capture contribution appears in a proc completion")),
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
