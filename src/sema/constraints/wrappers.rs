use super::{Binding, CallableApplicationScope, ConstraintChange, ConstraintConflict, ConstraintResolutionError, EffectConstraintError, EffectVariableId, RowTail, SchemeOperationError, TypeConstraints, Variable, MAX_TYPE_DEPTH};
use crate::sema::effects::{EffectContract, EffectPredicate, EffectSet};
use crate::sema::types::Type;
use crate::sema::wrapper_operation::{WrapperBinding, WrapperBoundary, WrapperProjection, WrapperRequirement, WrapperRequirementKind, WrapperSelection, FallbackKind, fallback_projection, guarded_field_projection};
#[cfg(test)]
use crate::sema::wrapper_operation::WrapperErrorContribution;
use crate::source::Span;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_WRAPPER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct WrapperOperationId(NonZeroU64);

#[derive(Clone, Debug)]
pub(crate) enum WrapperConstraintError {
    Resolution(ConstraintResolutionError),
    Constraint(ConstraintConflict),
    Effects(EffectConstraintError),
    Invalid { message: String, operation: crate::sema::wrapper_operation::WrapperOperationKind },
}

impl From<ConstraintResolutionError> for WrapperConstraintError {
    fn from(error: ConstraintResolutionError) -> Self { Self::Resolution(error) }
}

impl std::fmt::Display for WrapperConstraintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolution(error) => write!(f, "wrapper type resolution failed: {error:?}"),
            Self::Constraint(error) => write!(f, "wrapper requires {}, found {}", error.expected, error.actual),
            Self::Effects(error) => write!(f, "wrapper effect constraint failed: {error:?}"),
            Self::Invalid { message, .. } => f.write_str(message),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ScopedWrapper {
    pub(super) owner: CallableApplicationScope,
    pub(super) requirement: WrapperRequirement<Type, EffectVariableId>,
    pub(super) selection: Option<WrapperSelection>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum WrapperDependency { Type(super::TypeVariableId), Row(super::RowVariableId), Effect(EffectVariableId) }

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct WrapperOperations {
    pub(super) entries: BTreeMap<WrapperOperationId, ScopedWrapper>,
    sources: BTreeMap<(CallableApplicationScope, Span), WrapperOperationId>,
    watchers: BTreeMap<WrapperDependency, BTreeSet<WrapperOperationId>>,
    pending: VecDeque<WrapperOperationId>,
    queued: BTreeSet<WrapperOperationId>,
}

#[derive(Clone, Debug)]
pub(super) enum WrapperChange {
    Fresh(WrapperOperationId), Source((CallableApplicationScope, Span)),
    Selection(WrapperOperationId, Option<WrapperSelection>), Watcher(WrapperDependency, WrapperOperationId),
    Queued(WrapperOperationId), Dequeued(WrapperOperationId),
}

impl WrapperOperations {
    #[cfg(test)]
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut bytes = self.entries.len() * size_of::<(WrapperOperationId, ScopedWrapper)>()
            + self.entries.values().flat_map(|entry| entry.requirement.types()).map(super::type_owned_payload_bytes).sum::<usize>()
            + self.sources.len() * size_of::<((CallableApplicationScope, Span), WrapperOperationId)>()
            + self.watchers.len() * size_of::<(WrapperDependency, BTreeSet<WrapperOperationId>)>()
            + self.watchers.values().map(|ids| ids.len() * size_of::<WrapperOperationId>()).sum::<usize>()
            + self.pending.capacity() * size_of::<WrapperOperationId>() + self.queued.len() * size_of::<WrapperOperationId>();
        for entry in self.entries.values() {
            match &entry.requirement.kind {
                WrapperRequirementKind::Field { effects, .. } => bytes += effects.retained_bytes().saturating_sub(size_of::<EffectContract<EffectVariableId>>()),
                WrapperRequirementKind::Capture { contributions, .. } => bytes += contributions.len() * size_of::<WrapperErrorContribution<Type>>(),
                _ => {}
            }
        }
        bytes
    }

    pub(super) fn rollback(&mut self, change: WrapperChange) {
        match change {
            WrapperChange::Fresh(id) => { self.entries.remove(&id).expect("wrapper belongs to its journal"); }
            WrapperChange::Source(site) => { self.sources.remove(&site).expect("wrapper source belongs to its journal"); }
            WrapperChange::Selection(id, old) => self.entries.get_mut(&id).unwrap().selection = old,
            WrapperChange::Watcher(dependency, id) => {
                let watchers = self.watchers.get_mut(&dependency).unwrap();
                assert!(watchers.remove(&id));
                if watchers.is_empty() { self.watchers.remove(&dependency); }
            }
            WrapperChange::Queued(id) => { assert_eq!(self.pending.pop_back(), Some(id)); assert!(self.queued.remove(&id)); }
            WrapperChange::Dequeued(id) => { self.pending.push_front(id); assert!(self.queued.insert(id)); }
        }
    }

    fn enqueue(&mut self, id: WrapperOperationId, changes: &mut Vec<ConstraintChange>) {
        if self.queued.insert(id) { self.pending.push_back(id); changes.push(ConstraintChange::Wrapper(WrapperChange::Queued(id))); }
    }

    fn watch(&mut self, dependency: WrapperDependency, id: WrapperOperationId, changes: &mut Vec<ConstraintChange>) {
        if self.watchers.entry(dependency).or_default().insert(id) { changes.push(ConstraintChange::Wrapper(WrapperChange::Watcher(dependency, id))); }
    }

    pub(super) fn enqueue_changes(&mut self, from: usize, changes: &mut Vec<ConstraintChange>) {
        let dependencies = changes[from..].iter().filter_map(|change| match change {
            ConstraintChange::Type(id, _) | ConstraintChange::FreshType(id) => Some(WrapperDependency::Type(*id)),
            ConstraintChange::Row(id, _) => Some(WrapperDependency::Row(*id)),
            ConstraintChange::Effect(super::effect_constraints::EffectChange::Binding(id, _) | super::effect_constraints::EffectChange::FreshVariable(id)) => Some(WrapperDependency::Effect(*id)),
            _ => None,
        }).collect::<BTreeSet<_>>();
        let pending = dependencies.into_iter().filter_map(|dependency| self.watchers.get(&dependency)).flatten().copied().collect::<BTreeSet<_>>();
        for id in pending { self.enqueue(id, changes); }
    }

    pub(super) fn is_pending(&self) -> bool { !self.pending.is_empty() }
}

impl TypeConstraints {
    fn wrapper_dependencies(&self, operation: &WrapperRequirement<Type, super::EffectVariableId>) -> Result<BTreeSet<WrapperDependency>, ConstraintResolutionError> {
        enum Pending<'a> { Type(&'a Type, usize), Row(RowTail, usize) }
        let mut pending = operation.types().into_iter().map(|ty| Pending::Type(ty, 0)).collect::<Vec<_>>();
        let mut dependencies = BTreeSet::new();
        while let Some(node) = pending.pop() {
            match node {
                Pending::Type(ty, depth) => {
                    if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
                    if let Type::Inference(id) = ty {
                        let root = self.representative_root(*id)?;
                        if !dependencies.insert(WrapperDependency::Type(root)) { continue; }
                        let Some(Variable::Representative(variable)) = self.variables.get(&root) else { unreachable!() };
                        match &variable.binding {
                            Some(Binding::Type { ty, .. }) => pending.push(Pending::Type(ty, depth)),
                            Some(Binding::Record(record)) => {
                                pending.extend(record.shape.fields.values().map(|ty| Pending::Type(ty, depth + 1)));
                                pending.push(Pending::Row(record.shape.tail.clone(), depth));
                            }
                            #[cfg(test)]
                            Some(Binding::Rigid { .. }) => {},
                            None => {},
                        }
                    } else {
                        let mut contracts = Vec::new();
                        match ty {
                            Type::Callable(callable) => contracts.push(callable.sig.effects.contract()),
                            Type::Module(exports) => for export in exports.values() {
                                if let crate::sema::types::ModuleExportType::Callable { sig, .. } = export { contracts.push(sig.effects.contract()); }
                            },
                            _ => {}
                        }
                        for contract in contracts {
                            if let EffectContract::Known(expression) = self.resolve_effect(contract)? {
                                dependencies.extend(expression.variables().map(|(id, _)| WrapperDependency::Effect(*id)));
                            }
                        }
                        ty.for_each_child(|child| pending.push(Pending::Type(child, depth + 1)));
                    }
                }
                Pending::Row(tail, depth) => {
                    if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
                    let RowTail::Variable(id) = tail else { continue; };
                    let root = self.row_representative_root(id)?;
                    if !dependencies.insert(WrapperDependency::Row(root)) { continue; }
                    let Some(super::RowVariable::Representative(variable)) = self.rows.get(&root) else { unreachable!() };
                    if let Some(binding) = &variable.binding {
                        pending.extend(binding.shape.fields.values().map(|ty| Pending::Type(ty, depth + 1)));
                        pending.push(Pending::Row(binding.shape.tail.clone(), depth));
                    }
                }
            }
        }
        if let WrapperRequirementKind::Field { effects, .. } = &operation.kind {
            if let EffectContract::Known(expression) = self.resolve_effect(effects)? {
                for (id, _) in expression.variables() { dependencies.insert(WrapperDependency::Effect(*id)); }
            }
        }
        Ok(dependencies)
    }

    pub(crate) fn wrapper_constructor_context_is_deferred(&self, expected: &Type) -> Result<bool, ConstraintResolutionError> {
        let Type::Inference(id) = self.resolve(expected)? else { return Ok(false); };
        let root = self.representative_root(id)?;
        for entry in self.wrappers.entries.values() {
            let constructor = match &entry.requirement.kind {
                WrapperRequirementKind::Fallback { fallback, .. } => fallback,
                WrapperRequirementKind::Handler { handler, .. } => handler,
                _ => continue,
            };
            let mut pending = vec![(constructor.clone(), 0)];
            while let Some((ty, depth)) = pending.pop() {
                if depth > MAX_TYPE_DEPTH { return Err(ConstraintResolutionError::TypeDepth); }
                let ty = self.resolve(&ty)?;
                if let Type::Inference(id) = ty {
                    if self.representative_root(id)? == root { return Ok(true); }
                } else {
                    ty.for_each_child(|child| pending.push((child.clone(), depth + 1)));
                }
            }
        }
        Ok(false)
    }

    pub(crate) fn wrapper_at(&self, owner: CallableApplicationScope, span: Span) -> Option<WrapperOperationId> { self.wrappers.sources.get(&(owner, span)).copied() }

    pub(crate) fn wrapper_result_at(&self, owner: CallableApplicationScope, span: Span) -> Option<&Type> {
        let entry = self.wrappers.entries.get(&self.wrapper_at(owner, span)?)?;
        Some(match &entry.requirement.kind {
            WrapperRequirementKind::Field { result, .. } | WrapperRequirementKind::Fallback { result, .. }
            | WrapperRequirementKind::Handler { result, .. } | WrapperRequirementKind::Capture { result, .. } => result,
        })
    }

    pub(crate) fn register_wrapper(&mut self, owner: CallableApplicationScope, requirement: WrapperRequirement<Type, EffectVariableId>) -> Result<WrapperOperationId, WrapperConstraintError> {
        let dependencies = self.wrapper_dependencies(&requirement)?;
        let id = WrapperOperationId(NonZeroU64::new(NEXT_WRAPPER.fetch_add(1, Ordering::Relaxed)).expect("wrapper identities are nonzero"));
        self.apply_journaled(|this, changes| {
            assert!(this.wrappers.entries.insert(id, ScopedWrapper { owner, requirement, selection: None }).is_none());
            changes.push(ConstraintChange::Wrapper(WrapperChange::Fresh(id)));
            for dependency in dependencies { this.wrappers.watch(dependency, id, changes); }
            this.wrappers.enqueue(id, changes);
            Ok::<_, WrapperConstraintError>(())
        })?;
        Ok(id)
    }

    pub(crate) fn register_source_wrapper(&mut self, owner: CallableApplicationScope, requirement: WrapperRequirement<Type, EffectVariableId>) -> Result<WrapperOperationId, WrapperConstraintError> {
        self.commit_constraint_transaction(|this| {
            if let Some(id) = this.wrapper_at(owner, requirement.span) {
                let previous = this.wrappers.entries[&id].requirement.clone();
                let previous_shape = previous.try_map(&mut (), |_, _| Ok::<_, std::convert::Infallible>(()), |_, _| Ok(EffectContract::<()>::finite(EffectSet::default()))).unwrap();
                let next_shape = requirement.try_map(&mut (), |_, _| Ok::<_, std::convert::Infallible>(()), |_, _| Ok(EffectContract::<()>::finite(EffectSet::default()))).unwrap();
                if previous_shape != next_shape {
                    return Err(WrapperConstraintError::Invalid { message: "wrapper source changed its operation".into(), operation: requirement.operation_kind() });
                }
                this.apply_journaled(|this, changes| {
                    for (expected, actual) in previous.types().into_iter().zip(requirement.types()) { this.constrain_wrapper_type(expected, actual, requirement.span, changes)?; }
                    if let (WrapperRequirementKind::Field { effects: left, .. }, WrapperRequirementKind::Field { effects: right, .. }) = (&previous.kind, &requirement.kind) {
                        this.constrain_effect_inner(EffectPredicate::Equal(left.clone(), right.clone()), changes).map_err(WrapperConstraintError::Effects)?;
                    }
                    Ok::<_, WrapperConstraintError>(())
                })?;
                return Ok(id);
            }
            let span = requirement.span;
            let id = this.register_wrapper(owner, requirement)?;
            this.apply_journaled(|this, changes| {
                assert!(this.wrappers.sources.insert((owner, span), id).is_none());
                changes.push(ConstraintChange::Wrapper(WrapperChange::Source((owner, span))));
                Ok::<_, WrapperConstraintError>(())
            })?;
            Ok(id)
        })
    }

    fn constrain_wrapper_type(&mut self, expected: &Type, actual: &Type, span: Span, changes: &mut Vec<ConstraintChange>) -> Result<(), WrapperConstraintError> {
        self.constrain_inner(expected, actual, span, false, 0, changes).map_err(|(expected, actual, resolution_error)| WrapperConstraintError::Constraint(ConstraintConflict { expected, actual, resolution_error, contribution: span, initializer: None, established: None }))
    }

    fn constrain_wrapper_projection(&mut self, expected: &Type, projected: &Type, span: Span, changes: &mut Vec<ConstraintChange>) -> Result<(), WrapperConstraintError> {
        self.constrain_inner(expected, projected, span, !projected.contains_inference(), 0, changes).map_err(|(expected, actual, resolution_error)| WrapperConstraintError::Constraint(ConstraintConflict { expected, actual, resolution_error, contribution: span, initializer: None, established: None }))
    }

    fn constrain_wrapper_constructor_holes(&mut self, actual: &Type, expected: &Type, span: Span, changes: &mut Vec<ConstraintChange>) -> Result<(), WrapperConstraintError> {
        let mut pending = vec![(actual.clone(), expected.clone())];
        while let Some((actual, expected)) = pending.pop() {
            let actual = self.resolve_with_changes(&actual, 0, changes)?;
            let expected = self.resolve_with_changes(&expected, 0, changes)?;
            match (&actual, &expected) {
                (Type::Inference(_), _) if !expected.contains_inference() => self.constrain_wrapper_projection(&actual, &expected, span, changes)?,
                (Type::Optional(actual), Type::Optional(expected)) | (Type::List(actual), Type::List(expected)) | (Type::Set(actual), Type::Set(expected)) => pending.push(((**actual).clone(), (**expected).clone())),
                (_, Type::Optional(expected)) => pending.push((actual, (**expected).clone())),
                (Type::Result(actual_ok, actual_error), Type::Result(expected_ok, expected_error)) | (Type::Map(actual_ok, actual_error), Type::Map(expected_ok, expected_error)) => {
                    pending.push(((**actual_ok).clone(), (**expected_ok).clone()));
                    pending.push(((**actual_error).clone(), (**expected_error).clone()));
                }
                (Type::Record(actual), Type::Record(expected)) => for (name, expected) in expected.iter() { if let Some(actual) = actual.get(name) { pending.push((actual.clone(), expected.clone())); } },
                _ => {}
            }
        }
        Ok(())
    }

    pub(crate) fn constrain_wrapper_returns(&mut self, owner: crate::sema::check::EffectDeclarationId, returned: &Type, span: Span) -> Result<(), WrapperConstraintError> {
        let boundaries = self.wrappers.sources.iter().filter_map(|((source_owner, _), id)| {
            if *source_owner != CallableApplicationScope::Declaration(owner) { return None; }
            match &self.wrappers.entries[id].requirement.kind {
                WrapperRequirementKind::Field { boundary: WrapperBoundary::Return(ty) | WrapperBoundary::ErrorReturn(ty) | WrapperBoundary::ProcCompletion(ty), .. } => Some(ty.clone()),
                _ => None,
            }
        }).collect::<Vec<_>>();
        self.commit_constraint_transaction(|this| {
            this.apply_journaled(|this, changes| {
                for boundary in boundaries { this.constrain_wrapper_type(&boundary, returned, span, changes)?; }
                Ok::<_, WrapperConstraintError>(())
            })?;
            this.solve_wrapper_operations()
        })
    }

    pub(crate) fn solve_wrapper_operations(&mut self) -> Result<(), WrapperConstraintError> {
        self.commit_constraint_transaction(|this| {
            while this.wrappers.is_pending() { this.solve_pending_wrappers()?; }
            Ok(())
        })
    }

    fn solve_pending_wrappers(&mut self) -> Result<(), WrapperConstraintError> {
        self.apply_journaled(|this, changes| {
            while let Some(id) = this.wrappers.pending.pop_front() {
                assert!(this.wrappers.queued.remove(&id));
                changes.push(ConstraintChange::Wrapper(WrapperChange::Dequeued(id)));
                let requirement = this.wrappers.entries[&id].requirement.clone();
                let dependencies = this.wrapper_dependencies(&requirement)?;
                let level = dependencies.iter().filter_map(|dependency| match dependency { WrapperDependency::Type(id) => Some(this.type_level(*id)), _ => None }).collect::<Result<Vec<_>, _>>()?.into_iter().min();
                if let Some(level) = level { for ty in requirement.types() { this.lower_levels_in_type(ty, level, changes)?; } }
                for dependency in dependencies { this.wrappers.watch(dependency, id, changes); }
                let resolved = requirement.try_map(&mut (&mut *this, &mut *changes), |context, ty| context.0.resolve_with_changes(ty, 0, context.1).map_err(WrapperConstraintError::Resolution), |context, contract| context.0.resolve_effect(contract).map_err(WrapperConstraintError::Resolution))?;
                let invalid = |message: String| WrapperConstraintError::Invalid { message, operation: requirement.operation_kind() };
                let selected = match &resolved.kind {
                    WrapperRequirementKind::Field { source, name, result, boundary, effects } => {
                        if matches!(source, Type::Inference(_)) { continue; }
                        let projection = fallback_projection(source, FallbackKind::Value).map_err(|error| invalid(format!("guarded field source is invalid: {error:?}")))?;
                        let effect = if matches!(&projection, WrapperProjection::Result { .. }) { EffectSet::singleton(crate::syntax::node::Effect::Error) } else { EffectSet::default() };
                        this.constrain_effect_inner(EffectPredicate::Equal(effects.clone(), EffectContract::finite(effect)), changes).map_err(WrapperConstraintError::Effects)?;
                        if matches!(projection.value(), Type::Inference(_)) { continue; }
                        let projected = guarded_field_projection(source, *name, crate::sema::wrapper_operation::wrapper_field_type).map_err(|error| invalid(format!("guarded field projection is invalid: {error:?}")))?;
                        this.constrain_wrapper_projection(result, &projected.guarded_result(), requirement.span, changes)?;
                        if let WrapperBoundary::ProcCompletion(returned) = boundary {
                            let returned = this.resolve_with_changes(returned, 0, changes)?;
                            if matches!(returned, Type::Inference(_)) { continue; }
                            if !matches!(returned, Type::Result(_, error) if *error == Type::Error) { return Err(invalid("guarded field has no inferred proc completion envelope".into())); }
                        }
                        if let WrapperProjection::Result { error, .. } = &projected {
                            match boundary {
                                WrapperBoundary::Return(returned) | WrapperBoundary::ErrorReturn(returned) => {
                                    let returned = this.resolve_with_changes(returned, 0, changes)?;
                                    match returned {
                                        Type::Inference(_) => continue,
                                        Type::Result(_, expected) => {
                                            if expected.contains_inference() || error.contains_inference() { this.constrain_wrapper_type(&expected, error, requirement.span, changes)?; }
                                            else if !error.matches_expected(&expected) { return Err(invalid("guarded field has an incompatible propagated error".into())); }
                                        }
                                        _ if matches!(boundary, WrapperBoundary::ErrorReturn(_)) => {},
                                        _ => return Err(invalid("guarded Result field requires a Result return or a capture boundary".into())),
                                    }
                                }
                                WrapperBoundary::Producer if !error.contains_inference() && !error.matches_expected(&Type::Error) && !error.matches_expected(&Type::Str) => return Err(invalid("guarded field error is not accepted by the producer boundary".into())),
                                _ => {}
                            }
                        }
                        if source.contains_inference() || projected.guarded_result().contains_inference() { continue; }
                        match projected { WrapperProjection::Optional(_) => WrapperSelection::Optional, WrapperProjection::Result { .. } => WrapperSelection::Result }
                    }
                    WrapperRequirementKind::Fallback { source, fallback, result } => {
                        if matches!(source, Type::Inference(_)) { continue; }
                        let projected = fallback_projection(source, FallbackKind::Value).map_err(|error| invalid(format!("fallback source is invalid: {error:?}")))?;
                        this.constrain_wrapper_projection(result, projected.value(), requirement.span, changes)?;
                        if !matches!(fallback, Type::Inference(_)) { this.constrain_wrapper_constructor_holes(fallback, projected.value(), requirement.span, changes)?; }
                        if source.contains_inference() || fallback.contains_inference() { continue; }
                        if !fallback.matches_expected(projected.value()) { return Err(invalid(format!("fallback requires {}, found {fallback}", projected.value()))); }
                        match projected { WrapperProjection::Optional(_) => WrapperSelection::Optional, WrapperProjection::Result { .. } => WrapperSelection::Result }
                    }
                    WrapperRequirementKind::Handler { source, error, handler, result } => {
                        if matches!(source, Type::Inference(_)) { continue; }
                        let projected = fallback_projection(source, FallbackKind::Handler).map_err(|error| invalid(format!("error handler source is invalid: {error:?}")))?;
                        let WrapperProjection::Result { value, error: actual_error } = projected else { unreachable!() };
                        this.constrain_wrapper_projection(result, &value, requirement.span, changes)?;
                        this.constrain_wrapper_projection(error, &actual_error, requirement.span, changes)?;
                        if !matches!(handler, Type::Inference(_)) { this.constrain_wrapper_constructor_holes(handler, &value, requirement.span, changes)?; }
                        if source.contains_inference() || handler.contains_inference() { continue; }
                        if !handler.matches_expected(&value) { return Err(invalid(format!("error handler requires {value}, found {handler}"))); }
                        WrapperSelection::Result
                    }
                    WrapperRequirementKind::Capture { contributions, result, expected } => {
                        let Some(errors) = crate::sema::wrapper_operation::capture_error_contributions(contributions).map_err(|error| invalid(format!("capture error source is invalid: {error:?}")))? else { continue; };
                        if *expected {
                            for error in errors { this.constrain_wrapper_type(result, &error, requirement.span, changes)?; }
                        } else {
                            this.constrain_wrapper_projection(result, &Type::captured_error_type(errors), requirement.span, changes)?;
                        }
                        WrapperSelection::Capture
                    }
                };
                if let Some(canonical) = crate::sema::wrapper_operation::closed_wrapper_selection(&resolved).map_err(|error| invalid(format!("wrapper relationship is invalid: {error:?}")))? {
                    if canonical != selected { return Err(invalid("wrapper selection disagrees with its canonical relationship".into())); }
                }
                let previous = this.wrappers.entries.get_mut(&id).unwrap().selection.replace(selected);
                if previous != Some(selected) { changes.push(ConstraintChange::Wrapper(WrapperChange::Selection(id, previous))); }
            }
            Ok(())
        })
    }

    pub(super) fn generalize_wrapper(&self, id: WrapperOperationId, level: u32, generalization: &mut super::Generalization) -> Result<WrapperRequirement<crate::sema::schemes::SchemeType, crate::sema::schemes::SchemeEffectParameterId>, SchemeOperationError> {
        let requirement = &self.wrappers.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?.requirement;
        requirement.try_map(generalization, |context, ty| self.generalize_type(ty, level, context, 0), |context, contract| self.generalize_effect_contract(contract, level, context))
    }

    pub(super) fn generalize_wrappers(&self, owners: &BTreeSet<crate::sema::check::EffectDeclarationId>, level: u32, generalization: &mut super::Generalization) -> Result<Vec<WrapperRequirement<crate::sema::schemes::SchemeType, crate::sema::schemes::SchemeEffectParameterId>>, SchemeOperationError> {
        let mut requirements = Vec::new();
        for (&id, entry) in &self.wrappers.entries {
            if entry.selection.is_some() || !matches!(entry.owner, CallableApplicationScope::Declaration(owner) if owners.contains(&owner)) { continue; }
            generalization.wrappers.insert(id, u32::try_from(requirements.len()).map_err(|_| ConstraintResolutionError::TypeDepth)?);
            requirements.push(self.generalize_wrapper(id, level, generalization)?);
        }
        Ok(requirements)
    }

    pub(super) fn validate_generalized_wrappers(&self, callable: &crate::sema::schemes::CallableScheme, scope: &super::CallableGeneralizationScope) -> Result<Vec<WrapperOperationId>, SchemeOperationError> {
        let mut wrappers = scope.wrappers.iter().map(|(&id, &ordinal)| (ordinal, id)).collect::<Vec<_>>();
        wrappers.sort_unstable();
        if wrappers.len() != callable.requirements().wrappers.len() { return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedWrapper)); }
        let mut generalization = Self::callable_generalization(callable, scope);
        for (index, &(ordinal, id)) in wrappers.iter().enumerate() {
            if index != ordinal as usize || self.generalize_wrapper(id, 0, &mut generalization)? != callable.requirements().wrappers[index] { return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedWrapper)); }
        }
        if generalization.type_ids != scope.types || generalization.row_ids != scope.rows || generalization.effect_ids != scope.effects { return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::BodyTypeNotBound)); }
        Ok(wrappers.into_iter().map(|(_, id)| id).collect())
    }

    pub(crate) fn generalized_wrapper_ordinal(&self, id: WrapperOperationId, owner: crate::sema::check::EffectDeclarationId, callable: &crate::sema::schemes::CallableScheme, scope: &super::CallableGeneralizationScope) -> Result<Option<u32>, SchemeOperationError> {
        let entry = self.wrappers.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?;
        if entry.owner != CallableApplicationScope::Declaration(owner) || scope.owner != entry.owner { return Err(ConstraintResolutionError::ForeignVariable.into()); }
        self.validate_generalized_wrappers(callable, scope)?;
        Ok(scope.wrappers.get(&id).copied())
    }

    pub(super) fn wrapper_bindings(&self, instance: &super::InstantiatedScheme, generalization: &super::Generalization) -> Result<Vec<WrapperBinding>, SchemeOperationError> {
        instance.wrappers.iter().map(|id| {
            let entry = self.wrappers.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            Ok(match entry.selection {
                Some(selection) => WrapperBinding::Selected(selection),
                None => WrapperBinding::Forwarded(*generalization.wrappers.get(id).ok_or(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedWrapper))?),
            })
        }).collect()
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::{Name, SymbolOwner};

    #[test]
    fn wrapper_sources_keep_envelope_ambiguity_and_journal_descendant_failures() {
        SymbolOwner::new().with_current(|| {
            let span = Span::at(crate::source::SourceId::new(0), 0);
            let owner = CallableApplicationScope::Invocation;
            let mut constraints = TypeConstraints::default();
            let source = constraints.fresh(span);
            let result = constraints.fresh(span);
            constraints.register_wrapper(owner, WrapperRequirement { kind: WrapperRequirementKind::Fallback { source: source.clone(), fallback: Type::Int, result: result.clone() }, span }).unwrap();
            constraints.solve_wrapper_operations().unwrap();
            assert_eq!(constraints.resolve(&source).unwrap(), source);
            assert_eq!(constraints.resolve(&result).unwrap(), result);
            constraints.constrain(&source, &Type::Optional(Box::new(Type::Int)), span).unwrap();
            constraints.solve_wrapper_operations().unwrap();
            assert_eq!(constraints.resolve(&result).unwrap(), Type::Int);

            let mut constraints = TypeConstraints::default();
            let payload = constraints.fresh(span);
            let source = Type::Result(Box::new(payload.clone()), Box::new(Type::Str));
            let effects = EffectContract::variable(constraints.fresh_effect_at_level(span, 1));
            constraints.register_wrapper(owner, WrapperRequirement { kind: WrapperRequirementKind::Field { source, name: Name::intern("name"), result: Type::Bool, boundary: WrapperBoundary::Captured, effects: effects.clone() }, span }).unwrap();
            constraints.constrain(&payload, &Type::Record(std::sync::Arc::new(BTreeMap::from([(Name::intern("name"), Type::Str)]))), span).unwrap();
            let before = constraints.wrappers.clone();
            let before_effects = constraints.resolve_effect(&effects).unwrap();
            assert!(constraints.solve_wrapper_operations().is_err());
            assert_eq!(constraints.wrappers, before);
            assert_eq!(constraints.resolve_effect(&effects).unwrap(), before_effects);
        });
    }

    #[test]
    fn capture_error_relationships_distinguish_absent_and_present_failures() {
        let span = Span::at(crate::source::SourceId::new(0), 0);
        for (source, expected) in [
            (Type::Optional(Box::new(Type::Int)), Type::Error),
            (Type::Result(Box::new(Type::Int), Box::new(Type::Str)), Type::Str),
        ] {
            let mut constraints = TypeConstraints::default();
            let input = constraints.fresh(span);
            let result = constraints.fresh(span);
            constraints.register_wrapper(CallableApplicationScope::Invocation, WrapperRequirement { kind: WrapperRequirementKind::Capture { contributions: vec![WrapperErrorContribution::Source(input.clone())].into(), result: result.clone(), expected: false }, span }).unwrap();
            constraints.solve_wrapper_operations().unwrap();
            assert_eq!(constraints.resolve(&result).unwrap(), result);
            constraints.constrain(&input, &source, span).unwrap();
            constraints.solve_wrapper_operations().unwrap();
            assert_eq!(constraints.resolve(&result).unwrap(), expected);
        }
    }
}
