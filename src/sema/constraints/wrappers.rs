use super::{Binding, CallableApplicationScope, ConstraintChange, ConstraintConflict, ConstraintResolutionError, EffectConstraintError, EffectVariableId, RowTail, SchemeOperationError, TypeConstraints, Variable, MAX_TYPE_DEPTH};
use crate::sema::effects::{EffectContract, EffectPredicate, EffectSet};
use crate::sema::types::Type;
use crate::sema::wrapper_operation::{map_wrapper_groups, WrapperBinding, WrapperBoundary, WrapperCompletionContribution, WrapperCompletionContributions, WrapperGroupId, WrapperGroupRef, WrapperProjection, WrapperRequirement, WrapperRequirementKind, WrapperSelection, FallbackKind, fallback_projection, guarded_field_projection};
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
    Requirement(WrapperOperationId, WrapperRequirement<Type, EffectVariableId>),
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
                WrapperRequirementKind::ProcCompletion { contributions: WrapperCompletionContributions::Ready(contributions), .. } => bytes += contributions.len() * size_of::<WrapperCompletionContribution<Type>>(),
                _ => {}
            }
        }
        bytes
    }

    pub(super) fn rollback(&mut self, change: WrapperChange) {
        match change {
            WrapperChange::Fresh(id) => { self.entries.remove(&id).expect("wrapper belongs to its journal"); }
            WrapperChange::Source(site) => { self.sources.remove(&site).expect("wrapper source belongs to its journal"); }
            WrapperChange::Requirement(id, previous) => self.entries.get_mut(&id).unwrap().requirement = previous,
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
    fn wrapper_dependencies(&self, owner: CallableApplicationScope, operation: &WrapperRequirement<Type, super::EffectVariableId>) -> Result<BTreeSet<WrapperDependency>, ConstraintResolutionError> {
        enum Pending<'a> { Type(&'a Type, usize), Row(RowTail, usize) }
        let mut pending = operation.types().into_iter().map(|ty| Pending::Type(ty, 0)).collect::<Vec<_>>();
        let mut dependencies = BTreeSet::new();
        if let WrapperRequirementKind::Capture { contributions, .. } = &operation.kind {
            for contribution in contributions.iter() {
                let id = match contribution {
                    crate::sema::wrapper_operation::WrapperErrorContribution::KeyValidation(crate::sema::wrapper_operation::WrapperOperationRef::Source(id)) => *id,
                    crate::sema::wrapper_operation::WrapperErrorContribution::KeyValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(_)) => return Err(ConstraintResolutionError::ForeignVariable),
                    crate::sema::wrapper_operation::WrapperErrorContribution::KeyValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Source(requirement)) => {
                        let entry = self.wrappers.entries.get(requirement).ok_or(ConstraintResolutionError::ForeignVariable)?;
                        if entry.owner != owner { return Err(ConstraintResolutionError::ForeignVariable); }
                        let WrapperRequirementKind::KeyValidation { operation: crate::sema::wrapper_operation::WrapperOperationRef::Source(id), boundary } = &entry.requirement.kind else { return Err(ConstraintResolutionError::ForeignVariable); };
                        match boundary {
                            WrapperBoundary::Return(returned) | WrapperBoundary::ErrorReturn(returned) => pending.push(Pending::Type(returned, 0)),
                            WrapperBoundary::Captured | WrapperBoundary::ProcCompletion { .. } | WrapperBoundary::Producer | WrapperBoundary::TopLevel => {}
                        }
                        *id
                    }
                    crate::sema::wrapper_operation::WrapperErrorContribution::KeyValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Ordinal(_)) => return Err(ConstraintResolutionError::ForeignVariable),
                    _ => continue,
                };
                {
                    let indexed = self.operations.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?;
                    if indexed.owner != owner { return Err(ConstraintResolutionError::ForeignVariable); }
                    if let Some((receiver, key)) = indexed.operation.key_operands() {
                        pending.push(Pending::Type(receiver, 0));
                        pending.push(Pending::Type(key, 0));
                    } else {
                        return Err(ConstraintResolutionError::ForeignVariable);
                    }
                }
            }
        }
        if let WrapperRequirementKind::ProcCompletion { contributions: WrapperCompletionContributions::Ready(contributions), .. } = &operation.kind {
            for contribution in contributions.iter() {
                let crate::sema::wrapper_operation::WrapperCompletionContribution::KeyValidation(crate::sema::wrapper_operation::WrapperOperationRef::Source(id)) = contribution else { continue };
                let indexed = self.operations.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
                if indexed.owner != owner { return Err(ConstraintResolutionError::ForeignVariable); }
                let Some((receiver, key)) = indexed.operation.key_operands() else { return Err(ConstraintResolutionError::ForeignVariable); };
                pending.push(Pending::Type(receiver, 0));
                pending.push(Pending::Type(key, 0));
            }
        }
        if let WrapperRequirementKind::KeyValidation { operation, .. } = &operation.kind {
            let crate::sema::wrapper_operation::WrapperOperationRef::Source(id) = operation else { return Err(ConstraintResolutionError::ForeignVariable) };
                let indexed = self.operations.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            if indexed.owner != owner { return Err(ConstraintResolutionError::ForeignVariable); }
            if let Some((receiver, key)) = indexed.operation.key_operands() {
                pending.push(Pending::Type(receiver, 0));
                pending.push(Pending::Type(key, 0));
            } else {
                return Err(ConstraintResolutionError::ForeignVariable);
            }
        }
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
        if let WrapperRequirementKind::Field { effects, .. } = &operation.kind
            && let EffectContract::Known(expression) = self.resolve_effect(effects)?
        {
            for (id, _) in expression.variables() { dependencies.insert(WrapperDependency::Effect(*id)); }
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

    pub(super) fn optional_wrapper_call_argument_type(
        &self, instance: &super::InstantiatedScheme, expected: &Type, actual: &Type,
    ) -> Result<Option<Type>, ConstraintResolutionError> {
        if actual.contains_inference() || actual.contains_any() || actual.contains_recovery()
            || matches!(actual, Type::Optional(_) | Type::Result(_, _))
        { return Ok(None); }
        let expected = self.resolve(expected)?;
        if !matches!(expected, Type::Inference(_) | Type::Optional(_)) { return Ok(None); }
        for id in &instance.wrappers {
            let entry = self.wrappers.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            let WrapperRequirementKind::Fallback { source, fallback, result } = &entry.requirement.kind else { continue; };
            if self.resolve(source)? != expected { continue; }
            if let Type::Optional(payload) = &expected {
                if *actual != Type::Null && actual.matches_expected(payload) && !actual.matches_stored(payload) { return Ok(None); }
                return Ok(Some(expected));
            }
            let result = self.resolve(result)?;
            let payload = if !result.contains_inference() {
                result
            } else if *actual != Type::Null {
                actual.clone()
            } else {
                let fallback = self.resolve(fallback)?;
                if !fallback.contains_inference() && super::has_anchor(&fallback, super::AnchorEvidence::Inference)? { fallback } else { result }
            };
            if *actual != Type::Null && !payload.contains_inference() && actual.matches_expected(&payload) && !actual.matches_stored(&payload) { return Ok(None); }
            return Ok(Some(Type::Optional(Box::new(payload))));
        }
        Ok(None)
    }

    pub(crate) fn wrapper_result_at(&self, owner: CallableApplicationScope, span: Span) -> Option<&Type> {
        let entry = self.wrappers.entries.get(&self.wrapper_at(owner, span)?)?;
        Some(match &entry.requirement.kind {
            WrapperRequirementKind::Field { result, .. } | WrapperRequirementKind::Fallback { result, .. }
            | WrapperRequirementKind::Handler { result, .. } | WrapperRequirementKind::Capture { result, .. } => result,
            WrapperRequirementKind::KeyValidation { .. } => return None,
            WrapperRequirementKind::ProcCompletion { returned, .. } => returned,
        })
    }

    fn local_proc_completion_wrapper_id(&self, owner: CallableApplicationScope) -> Result<Option<WrapperOperationId>, ConstraintResolutionError> {
        let mut source_group = None;
        for ((source_owner, _), id) in &self.wrappers.sources {
            if *source_owner != owner { continue; }
            let entry = self.wrappers.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            let group = match &entry.requirement.kind {
                WrapperRequirementKind::Field { boundary: WrapperBoundary::ProcCompletion { group: WrapperGroupRef::Source(group), .. }, .. }
                | WrapperRequirementKind::KeyValidation { boundary: WrapperBoundary::ProcCompletion { group: WrapperGroupRef::Source(group), .. }, .. } => Some(*group),
                _ => None,
            };
            if let Some(group) = group
                && source_group.replace(group).is_some_and(|previous| previous != group)
            {
                return Err(ConstraintResolutionError::ForeignVariable);
            }
        }
        source_group.map(|group| self.group_wrapper_id(group, owner)).transpose()
    }

    pub(crate) fn ensure_proc_completion_group(
        &mut self,
        owner: crate::sema::check::EffectDeclarationId,
        span: Span,
        level: u32,
    ) -> Result<(WrapperGroupId, Type), WrapperConstraintError> {
        let scope = CallableApplicationScope::Declaration(owner);
        if let Some(id) = self.local_proc_completion_wrapper_id(scope)? {
            return match &self.wrappers.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?.requirement.kind {
                WrapperRequirementKind::ProcCompletion { group: WrapperGroupRef::Source(group), returned, .. } => Ok((*group, returned.clone())),
                _ => Err(WrapperConstraintError::Invalid { message: "proc completion source changed its wrapper kind".into(), operation: crate::sema::wrapper_operation::WrapperOperationKind::ProcCompletion }),
            };
        }
        let group = WrapperGroupId::fresh();
        let returned = Type::Result(
            Box::new(self.fresh_at_level(span, level)),
            Box::new(self.fresh_at_level(span, level)),
        );
        self.register_wrapper(scope, WrapperRequirement {
            kind: WrapperRequirementKind::ProcCompletion {
                group: WrapperGroupRef::Source(group),
                returned: returned.clone(),
                contributions: WrapperCompletionContributions::Pending,
            },
            span,
        })?;
        Ok((group, returned))
    }

    pub(crate) fn proc_completion_return(&self, owner: crate::sema::check::EffectDeclarationId) -> Result<Option<Type>, ConstraintResolutionError> {
        let Some(id) = self.local_proc_completion_wrapper_id(CallableApplicationScope::Declaration(owner))? else { return Ok(None) };
        match &self.wrappers.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?.requirement.kind {
            WrapperRequirementKind::ProcCompletion { returned, .. } => Ok(Some(returned.clone())),
            _ => Err(ConstraintResolutionError::ForeignVariable),
        }
    }

    pub(crate) fn finish_proc_completion_group(
        &mut self,
        owner: crate::sema::check::EffectDeclarationId,
        propagated: &[(Type, Span)],
        candidates: &[Type],
        application_sources: &[crate::sema::constraints::CallableApplicationReference],
    ) -> Result<bool, WrapperConstraintError> {
        let owner_scope = CallableApplicationScope::Declaration(owner);
        let Some(group_entry) = self.local_proc_completion_wrapper_id(owner_scope)? else { return Ok(false) };
        let group = match &self.wrappers.entries.get(&group_entry).ok_or(ConstraintResolutionError::ForeignVariable)?.requirement.kind {
            WrapperRequirementKind::ProcCompletion { group: WrapperGroupRef::Source(group), .. } => *group,
            _ => return Err(WrapperConstraintError::Invalid { message: "proc completion source changed its wrapper kind".into(), operation: crate::sema::wrapper_operation::WrapperOperationKind::ProcCompletion }),
        };
        let mut contributions = propagated.iter().map(|(error, _)| WrapperCompletionContribution::PropagatedError(error.clone())).collect::<Vec<_>>();
        let mut field_returns = Vec::new();
        for candidate in candidates {
            let candidate = self.resolve(candidate)?;
            if matches!(candidate, Type::Result(..)) { contributions.push(WrapperCompletionContribution::ReturnedResult(candidate)); }
        }
        for reference in application_sources {
            if reference.scope != owner_scope {
                return Err(WrapperConstraintError::Invalid { message: "proc completion application source belongs to another declaration".into(), operation: crate::sema::wrapper_operation::WrapperOperationKind::ProcCompletion });
            }
            contributions.push(WrapperCompletionContribution::ApplicationSource(self.referenced_callable_application_result(*reference)?));
        }
        for ((source_owner, _), id) in &self.wrappers.sources {
            if *source_owner != owner_scope { continue; }
            let entry = self.wrappers.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            if let WrapperRequirementKind::Field { source, boundary: WrapperBoundary::ProcCompletion { returned, group: WrapperGroupRef::Source(field_group) }, .. } = &entry.requirement.kind
                && *field_group == group
            {
                contributions.push(WrapperCompletionContribution::FieldSource(source.clone()));
                field_returns.push((returned.clone(), entry.requirement.span));
            }
            if let WrapperRequirementKind::KeyValidation {
                operation: crate::sema::wrapper_operation::WrapperOperationRef::Source(operation),
                boundary: WrapperBoundary::ProcCompletion { group: WrapperGroupRef::Source(field_group), .. },
            } = &entry.requirement.kind && *field_group == group {
                let indexed = self.operations.entries.get(operation).ok_or(ConstraintResolutionError::ForeignVariable)?;
                let resolved = indexed.operation.try_map(|ty| self.resolve(ty))?;
                let Some((receiver, key)) = resolved.key_operands() else { return Err(ConstraintResolutionError::ForeignVariable.into()); };
                if crate::sema::sealed_operation::may_require_uint_key_validation(&receiver, &key) {
                    contributions.push(WrapperCompletionContribution::KeyValidation(crate::sema::wrapper_operation::WrapperOperationRef::Source(*operation)));
                }
            }
        }
        let previous = self.wrappers.entries[&group_entry].requirement.clone();
        let WrapperRequirementKind::ProcCompletion { group: group_ref, returned, .. } = &previous.kind else { unreachable!() };
        let group_ref = *group_ref;
        let group_returned = (*returned).clone();
        let next = WrapperRequirement {
            kind: WrapperRequirementKind::ProcCompletion {
                group: group_ref,
                returned: (*returned).clone(),
                contributions: WrapperCompletionContributions::Ready(contributions.into()),
            },
            span: previous.span,
        };
        let dependencies = self.wrapper_dependencies(owner_scope, &next)?;
        self.apply_journaled(|this, changes| {
            for (field_returned, span) in field_returns {
                this.constrain_wrapper_type(&group_returned, &field_returned, span, changes)?;
            }
            this.wrappers.entries.get_mut(&group_entry).unwrap().requirement = next;
            changes.push(ConstraintChange::Wrapper(WrapperChange::Requirement(group_entry, previous)));
            for dependency in dependencies { this.wrappers.watch(dependency, group_entry, changes); }
            this.wrappers.enqueue(group_entry, changes);
            Ok::<_, WrapperConstraintError>(())
        })?;
        self.solve_wrapper_operations()?;
        Ok(true)
    }

    pub(crate) fn register_wrapper(&mut self, owner: CallableApplicationScope, requirement: WrapperRequirement<Type, EffectVariableId>) -> Result<WrapperOperationId, WrapperConstraintError> {
        let dependencies = self.wrapper_dependencies(owner, &requirement)?;
        let field_effects = match &requirement.kind {
            WrapperRequirementKind::Field { effects, .. } => Some(effects.clone()),
            _ => None,
        };
        let id = WrapperOperationId(NonZeroU64::new(NEXT_WRAPPER.fetch_add(1, Ordering::Relaxed)).expect("wrapper identities are nonzero"));
        self.apply_journaled(|this, changes| {
            if let Some(effects) = &field_effects {
                this.constrain_effect_inner(
                    EffectPredicate::Covered(
                        effects.clone(),
                        EffectContract::finite(EffectSet::singleton(crate::syntax::node::Effect::Error)),
                    ),
                    changes,
                )
                .map_err(WrapperConstraintError::Effects)?;
            }
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

    fn key_validation_boundary_error(
        &self,
        owner: CallableApplicationScope,
        reference: crate::sema::wrapper_operation::WrapperRequirementRef,
    ) -> Result<Option<Option<Type>>, WrapperConstraintError> {
        let crate::sema::wrapper_operation::WrapperRequirementRef::Source(id) = reference else { return Err(ConstraintResolutionError::ForeignVariable.into()) };
        let entry = self.wrappers.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?;
        if entry.owner != owner { return Err(ConstraintResolutionError::ForeignVariable.into()); }
        let WrapperRequirementKind::KeyValidation { operation: crate::sema::wrapper_operation::WrapperOperationRef::Source(operation), boundary } = &entry.requirement.kind else {
            return Err(ConstraintResolutionError::ForeignVariable.into());
        };
        let operation_entry = self.operations.entries.get(operation).ok_or(ConstraintResolutionError::ForeignVariable)?;
        if operation_entry.owner != owner { return Err(ConstraintResolutionError::ForeignVariable.into()); }
        let operation = operation_entry.operation.try_map(|ty| self.resolve(ty)).map_err(WrapperConstraintError::Resolution)?;
        let Some((receiver, key)) = operation.key_operands() else { return Err(ConstraintResolutionError::ForeignVariable.into()); };
        if receiver.contains_inference() || key.contains_inference() { return Ok(None); }
        if !crate::sema::sealed_operation::may_require_uint_key_validation(&receiver, &key) {
            return Ok(Some(None));
        }
        if !crate::sema::sealed_operation::requires_uint_key_validation(&receiver, &key) {
            return Ok(None);
        }
        let escaped = match boundary {
            WrapperBoundary::Captured | WrapperBoundary::ProcCompletion { .. } => None,
            WrapperBoundary::Producer | WrapperBoundary::TopLevel => Some(Type::Error),
            WrapperBoundary::Return(returned) | WrapperBoundary::ErrorReturn(returned) => {
                let returned = self.resolve(returned).map_err(WrapperConstraintError::Resolution)?;
                if returned.contains_inference() { return Ok(None); }
                match crate::sema::sealed_operation::result_error_contract(&returned) {
                    Some(expected) if Type::Error.matches_expected(expected) => None,
                    Some(_) => return Err(WrapperConstraintError::Invalid {
                        message: "UInt map key validation error is outside the callable Result contract".into(),
                        operation: crate::sema::wrapper_operation::WrapperOperationKind::KeyValidation,
                    }),
                    None => Some(Type::Error),
                }
            }
        };
        Ok(Some(escaped))
    }

    fn capture_error_contributions(
        &self,
        owner: CallableApplicationScope,
        contributions: &[crate::sema::wrapper_operation::WrapperErrorContribution<Type>],
    ) -> Result<Option<Vec<Type>>, WrapperConstraintError> {
        let mut errors = Vec::new();
        for contribution in contributions {
            match contribution {
                crate::sema::wrapper_operation::WrapperErrorContribution::Explicit(_)
                | crate::sema::wrapper_operation::WrapperErrorContribution::Source(_) => {
                    let Some(mut values) = crate::sema::wrapper_operation::capture_error_contributions(std::slice::from_ref(contribution)).map_err(|error| WrapperConstraintError::Invalid { message: format!("capture error source is invalid: {error:?}"), operation: crate::sema::wrapper_operation::WrapperOperationKind::Capture })? else { return Ok(None) };
                    errors.append(&mut values);
                }
                crate::sema::wrapper_operation::WrapperErrorContribution::KeyValidation(reference) => {
                    let crate::sema::wrapper_operation::WrapperOperationRef::Source(id) = reference else { return Err(ConstraintResolutionError::ForeignVariable.into()) };
                    let entry = self.operations.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
                    if entry.owner != owner { return Err(ConstraintResolutionError::ForeignVariable.into()); }
                    let operation = entry.operation.try_map(|ty| self.resolve(ty)).map_err(WrapperConstraintError::Resolution)?;
                    let Some((receiver, key)) = operation.key_operands() else { return Err(ConstraintResolutionError::ForeignVariable.into()) };
                    if crate::sema::sealed_operation::requires_uint_key_validation(&receiver, &key) {
                        errors.push(Type::Error);
                    } else if crate::sema::sealed_operation::may_require_uint_key_validation(&receiver, &key) {
                        return Ok(None);
                    }
                }
                crate::sema::wrapper_operation::WrapperErrorContribution::KeyValidationBoundary(reference) => {
                    match self.key_validation_boundary_error(owner, *reference)? {
                        None => return Ok(None),
                        Some(Some(error)) => errors.push(error),
                        Some(None) => {}
                    }
                }
            }
        }
        Ok(Some(errors))
    }

    fn completion_error_contributions(
        &self,
        owner: CallableApplicationScope,
        contributions: &[WrapperCompletionContribution<Type>],
    ) -> Result<Option<Vec<Type>>, WrapperConstraintError> {
        let mut errors = Vec::new();
        for contribution in contributions {
            match contribution {
                WrapperCompletionContribution::KeyValidation(crate::sema::wrapper_operation::WrapperOperationRef::Source(id)) => {
                    let entry = self.operations.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
                    if entry.owner != owner { return Err(ConstraintResolutionError::ForeignVariable.into()); }
                    let operation = entry.operation.try_map(|ty| self.resolve(ty)).map_err(WrapperConstraintError::Resolution)?;
                    let Some((receiver, key)) = operation.key_operands() else { return Err(ConstraintResolutionError::ForeignVariable.into()); };
                    if receiver.contains_inference() || key.contains_inference() { return Ok(None); }
                    if crate::sema::sealed_operation::may_require_uint_key_validation(&receiver, &key) {
                        if crate::sema::sealed_operation::requires_uint_key_validation(&receiver, &key) { errors.push(Type::Error); }
                        else { return Ok(None); }
                    }
                }
                WrapperCompletionContribution::KeyValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(_)) => return Err(ConstraintResolutionError::ForeignVariable.into()),
                _ => {
                    let Some(mut values) = crate::sema::wrapper_operation::completion_error_contributions(std::slice::from_ref(contribution)).map_err(|error| WrapperConstraintError::Invalid {
                        message: format!("proc completion source is invalid: {error:?}"),
                        operation: crate::sema::wrapper_operation::WrapperOperationKind::ProcCompletion,
                    })? else { return Ok(None) };
                    errors.append(&mut values);
                }
            }
        }
        Ok(Some(errors))
    }

    pub(crate) fn constrain_wrapper_returns(&mut self, owner: crate::sema::check::EffectDeclarationId, returned: &Type, span: Span) -> Result<(), WrapperConstraintError> {
        let boundaries = self.wrappers.sources.iter().filter_map(|((source_owner, _), id)| {
            if *source_owner != CallableApplicationScope::Declaration(owner) { return None; }
            match &self.wrappers.entries[id].requirement.kind {
                WrapperRequirementKind::Field { boundary: WrapperBoundary::Return(ty) | WrapperBoundary::ErrorReturn(ty), .. } => Some(ty.clone()),
                WrapperRequirementKind::KeyValidation { boundary: WrapperBoundary::Return(ty) | WrapperBoundary::ErrorReturn(ty), .. } => Some(ty.clone()),
                _ => None,
            }
        }).collect::<Vec<_>>();
        let completion = self.local_proc_completion_wrapper_id(CallableApplicationScope::Declaration(owner))?
            .and_then(|id| self.wrappers.entries.get(&id))
            .and_then(|entry| match &entry.requirement.kind { WrapperRequirementKind::ProcCompletion { returned, .. } => Some(returned.clone()), _ => None });
        self.commit_constraint_transaction(|this| {
            this.apply_journaled(|this, changes| {
                for boundary in boundaries { this.constrain_wrapper_type(&boundary, returned, span, changes)?; }
                if let Some(completion) = &completion { this.constrain_wrapper_type(completion, returned, span, changes)?; }
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
                let owner = this.wrappers.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?.owner;
                let dependencies = this.wrapper_dependencies(owner, &requirement)?;
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
                        if let WrapperBoundary::ProcCompletion { returned, .. } = boundary {
                            let resolved = this.resolve_with_changes(returned, 0, changes)?;
                            if !matches!(resolved, Type::Result(..)) { return Err(invalid("guarded field has no inferred proc completion envelope".into())); }
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
                        let Some(errors) = this.capture_error_contributions(owner, contributions).map_err(|error| invalid(format!("capture error source is invalid: {error:?}")))? else { continue; };
                        if *expected {
                            for error in errors { this.constrain_wrapper_type(result, &error, requirement.span, changes)?; }
                        } else {
                            this.constrain_wrapper_projection(result, &Type::captured_error_type(errors), requirement.span, changes)?;
                        }
                        WrapperSelection::Capture
                    }
                    WrapperRequirementKind::KeyValidation { operation, boundary } => {
                        let crate::sema::wrapper_operation::WrapperOperationRef::Source(operation) = operation else { return Err(ConstraintResolutionError::ForeignVariable.into()); };
                        let wrapper_owner = this.wrappers.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?.owner;
                        let operation_entry = this.operations.entries.get(operation).ok_or(ConstraintResolutionError::ForeignVariable)?;
                        if operation_entry.owner != wrapper_owner { return Err(ConstraintResolutionError::ForeignVariable.into()); }
                        let operation = operation_entry.operation.try_map(|ty| this.resolve_with_changes(ty, 0, changes).map_err(WrapperConstraintError::Resolution))?;
                        let Some((receiver, key)) = operation.key_operands() else { return Err(ConstraintResolutionError::ForeignVariable.into()); };
                        if receiver.contains_inference() || key.contains_inference() { continue; }
                        if crate::sema::sealed_operation::requires_uint_key_validation(&receiver, &key) {
                            match boundary {
                                WrapperBoundary::Return(returned) | WrapperBoundary::ErrorReturn(returned) => {
                                    let returned = this.resolve_with_changes(returned, 0, changes)?;
                                    if matches!(returned, Type::Inference(_)) { continue; }
                                    if let Some(expected) = crate::sema::sealed_operation::result_error_contract(&returned) {
                                        if expected.contains_inference() {
                                            this.constrain_wrapper_type(expected, &Type::Error, requirement.span, changes)?;
                                        } else if !Type::Error.matches_expected(expected) {
                                            return Err(invalid("UInt map key validation error is outside the callable Result contract".into()));
                                        }
                                    }
                                }
                                WrapperBoundary::ProcCompletion { returned, .. } => {
                                    let returned = this.resolve_with_changes(returned, 0, changes)?;
                                    if !matches!(returned, Type::Result(..)) { return Err(invalid("UInt map key validation has no inferred proc completion envelope".into())); }
                                }
                                WrapperBoundary::Captured | WrapperBoundary::Producer | WrapperBoundary::TopLevel => {}
                            }
                        }
                        WrapperSelection::KeyValidation
                    }
                    WrapperRequirementKind::ProcCompletion { contributions: WrapperCompletionContributions::Pending, .. } => continue,
                    WrapperRequirementKind::ProcCompletion { returned, contributions: WrapperCompletionContributions::Ready(contributions), .. } => {
                        let Some(errors) = this.completion_error_contributions(owner, contributions).map_err(|error| invalid(format!("proc completion source is invalid: {error:?}")))? else { continue; };
                        let Type::Result(_, expected) = returned else { return Err(invalid("proc completion has no Result return".into())); };
                        let actual = Type::captured_error_type(errors);
                        if expected.contains_inference() || actual.contains_inference() {
                            this.constrain_wrapper_type(expected, &actual, requirement.span, changes)?;
                        } else if actual != expected.as_ref().clone() {
                            return Err(invalid("proc completion error contributions disagree with its returned envelope".into()));
                        }
                        WrapperSelection::ProcCompletion
                    }
                };
                if let Some(canonical) = crate::sema::wrapper_operation::closed_wrapper_selection(&resolved).map_err(|error| invalid(format!("wrapper relationship is invalid: {error:?}")))?
                    && canonical != selected { return Err(invalid("wrapper selection disagrees with its canonical relationship".into())); }
                let previous = this.wrappers.entries.get_mut(&id).unwrap().selection.replace(selected);
                if previous != Some(selected) { changes.push(ConstraintChange::Wrapper(WrapperChange::Selection(id, previous))); }
            }
            Ok(())
        })
    }

    pub(super) fn generalize_wrapper(&self, id: WrapperOperationId, level: u32, generalization: &mut super::Generalization) -> Result<WrapperRequirement<crate::sema::schemes::SchemeType, crate::sema::schemes::SchemeEffectParameterId>, SchemeOperationError> {
        let requirement = &self.wrappers.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?.requirement;
        let owner = self.wrappers.entries.get(&id).ok_or(ConstraintResolutionError::ForeignVariable)?.owner;
        let mut generalized = requirement.try_map(generalization, |context, ty| self.generalize_type(ty, level, context, 0), |context, contract| self.generalize_effect_contract(contract, level, context))?;
        if let WrapperRequirementKind::KeyValidation { operation, .. } = &mut generalized.kind {
            let crate::sema::wrapper_operation::WrapperOperationRef::Source(operation_id) = operation else {
                return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedSealedOperation));
            };
            let ordinal = generalization.operations.get(operation_id).copied().ok_or(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedSealedOperation))?;
            *operation = crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(ordinal);
        }
        if let WrapperRequirementKind::Capture { contributions, .. } = &mut generalized.kind {
            let mut retained = Vec::with_capacity(contributions.len());
            for contribution in contributions.iter() {
                match contribution {
                    crate::sema::wrapper_operation::WrapperErrorContribution::KeyValidation(crate::sema::wrapper_operation::WrapperOperationRef::Source(operation_id)) => {
                        let entry = self.operations.entries.get(operation_id).ok_or(ConstraintResolutionError::ForeignVariable)?;
                        let operation = entry.operation.try_map(|ty| self.resolve(ty))?;
                        let Some((receiver, key)) = operation.key_operands() else {
                            return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedSealedOperation));
                        };
                        if crate::sema::sealed_operation::may_require_uint_key_validation(&receiver, &key) {
                            let ordinal = generalization.operations.get(operation_id).copied().ok_or(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedSealedOperation))?;
                            retained.push(crate::sema::wrapper_operation::WrapperErrorContribution::KeyValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(ordinal)));
                        }
                    }
                    crate::sema::wrapper_operation::WrapperErrorContribution::KeyValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(_)) => return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedSealedOperation)),
                    crate::sema::wrapper_operation::WrapperErrorContribution::KeyValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Source(requirement_id)) => {
                        let entry = self.wrappers.entries.get(requirement_id).ok_or(ConstraintResolutionError::ForeignVariable)?;
                        if entry.owner != owner { return Err(ConstraintResolutionError::ForeignVariable.into()); }
                        let crate::sema::wrapper_operation::WrapperRequirementKind::KeyValidation {
                            operation: crate::sema::wrapper_operation::WrapperOperationRef::Source(operation_id), ..
                        } = &entry.requirement.kind else {
                            return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedWrapper));
                        };
                        let indexed = self.operations.entries.get(operation_id).ok_or(ConstraintResolutionError::ForeignVariable)?;
                        if indexed.owner != owner { return Err(ConstraintResolutionError::ForeignVariable.into()); }
                        let operation = indexed.operation.try_map(|ty| self.resolve(ty))?;
                        let Some((receiver, key)) = operation.key_operands() else {
                            return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedSealedOperation));
                        };
                        if crate::sema::sealed_operation::may_require_uint_key_validation(&receiver, &key) {
                            let ordinal = generalization.wrappers.get(requirement_id).copied().ok_or(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedWrapper))?;
                            retained.push(crate::sema::wrapper_operation::WrapperErrorContribution::KeyValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Ordinal(ordinal)));
                        }
                    }
                    crate::sema::wrapper_operation::WrapperErrorContribution::KeyValidationBoundary(crate::sema::wrapper_operation::WrapperRequirementRef::Ordinal(_)) => return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedWrapper)),
                    _ => retained.push(contribution.clone()),
                }
            }
            *contributions = retained.into();
        }
        if let WrapperRequirementKind::ProcCompletion { contributions: WrapperCompletionContributions::Ready(contributions), .. } = &mut generalized.kind {
            let mut retained = Vec::with_capacity(contributions.len());
            for contribution in contributions.iter() {
                match contribution {
                    crate::sema::wrapper_operation::WrapperCompletionContribution::KeyValidation(crate::sema::wrapper_operation::WrapperOperationRef::Source(operation_id)) => {
                        let entry = self.operations.entries.get(operation_id).ok_or(ConstraintResolutionError::ForeignVariable)?;
                        if entry.owner != owner { return Err(ConstraintResolutionError::ForeignVariable.into()); }
                        let operation = entry.operation.try_map(|ty| self.resolve(ty))?;
                        let Some((receiver, key)) = operation.key_operands() else {
                            return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedSealedOperation));
                        };
                        if crate::sema::sealed_operation::may_require_uint_key_validation(&receiver, &key) {
                            let ordinal = generalization.operations.get(operation_id).copied().ok_or(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedSealedOperation))?;
                            retained.push(crate::sema::wrapper_operation::WrapperCompletionContribution::KeyValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(ordinal)));
                        }
                    }
                    crate::sema::wrapper_operation::WrapperCompletionContribution::KeyValidation(crate::sema::wrapper_operation::WrapperOperationRef::Ordinal(_)) => return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedSealedOperation)),
                    _ => retained.push(contribution.clone()),
                }
            }
            *contributions = retained.into();
        }
        map_wrapper_groups(&mut generalized, |group| match group {
            WrapperGroupRef::Source(group) => {
                let id = self.group_wrapper_id(group, owner)?;
                generalization.wrappers.get(&id).copied().map(WrapperGroupRef::Ordinal)
                    .ok_or(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedWrapper))
            }
            WrapperGroupRef::Ordinal(_) => Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedWrapper)),
        })?;
        if matches!(&generalized.kind, WrapperRequirementKind::ProcCompletion { contributions: WrapperCompletionContributions::Pending, .. }) {
            return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedWrapper));
        }
        Ok(generalized)
    }

    pub(super) fn generalize_wrappers(&self, owners: &BTreeSet<crate::sema::check::EffectDeclarationId>, level: u32, generalization: &mut super::Generalization) -> Result<Vec<WrapperRequirement<crate::sema::schemes::SchemeType, crate::sema::schemes::SchemeEffectParameterId>>, SchemeOperationError> {
        let mut ids = self.wrappers.entries.iter().filter_map(|(&id, entry)| {
            let retained_key_validation = if let WrapperRequirementKind::KeyValidation { operation: crate::sema::wrapper_operation::WrapperOperationRef::Source(operation), .. } = &entry.requirement.kind {
                self.operations.entries.get(operation).and_then(|entry| entry.operation.try_map(|ty| self.resolve(ty)).ok()).is_some_and(|operation| operation.key_operands().is_some_and(|(receiver, key)| crate::sema::sealed_operation::may_require_uint_key_validation(receiver, key)))
            } else { false };
            ((!entry.selection.is_some() || retained_key_validation) && matches!(entry.owner, CallableApplicationScope::Declaration(owner) if owners.contains(&owner))).then_some(id)
        }).collect::<BTreeSet<_>>();
        let referenced_groups = ids.iter().filter_map(|id| match &self.wrappers.entries[id].requirement.kind {
            WrapperRequirementKind::Field { boundary: WrapperBoundary::ProcCompletion { group: WrapperGroupRef::Source(group), .. }, .. } => Some((self.wrappers.entries[id].owner, *group)),
            WrapperRequirementKind::KeyValidation { boundary: WrapperBoundary::ProcCompletion { group: WrapperGroupRef::Source(group), .. }, .. } => Some((self.wrappers.entries[id].owner, *group)),
            _ => None,
        }).collect::<BTreeSet<_>>();
        for (owner, group) in referenced_groups { ids.insert(self.group_wrapper_id(group, owner)?); }
        let mut ids = ids.into_iter().collect::<Vec<_>>();
        ids.sort_by_key(|id| (!matches!(&self.wrappers.entries[id].requirement.kind, WrapperRequirementKind::ProcCompletion { .. }), *id));
        for (ordinal, id) in ids.iter().enumerate() {
            generalization.wrappers.insert(*id, u32::try_from(ordinal).map_err(|_| ConstraintResolutionError::TypeDepth)?);
        }
        let mut requirements = Vec::with_capacity(ids.len());
        for id in ids { requirements.push(self.generalize_wrapper(id, level, generalization)?); }
        Ok(requirements)
    }

    pub(super) fn validate_generalized_wrappers(&self, callable: &crate::sema::schemes::CallableScheme, scope: &super::CallableGeneralizationScope) -> Result<Vec<WrapperOperationId>, SchemeOperationError> {
        let mut wrappers = scope.wrappers.iter().map(|(&id, &ordinal)| (ordinal, id)).collect::<Vec<_>>();
        wrappers.sort_unstable();
        if wrappers.len() != callable.requirements().wrappers.len() { return Err(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedWrapper)); }
        let mut generalization = self.callable_generalization(callable, scope);
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

    fn group_wrapper_id(&self, group: WrapperGroupId, owner: CallableApplicationScope) -> Result<WrapperOperationId, ConstraintResolutionError> {
        self.wrappers.entries.iter().find_map(|(&id, entry)| {
            (entry.owner == owner && matches!(&entry.requirement.kind, WrapperRequirementKind::ProcCompletion { group: WrapperGroupRef::Source(actual), .. } if *actual == group)).then_some(id)
        }).ok_or(ConstraintResolutionError::ForeignVariable)
    }

    pub(super) fn wrapper_bindings(&self, instance: &super::InstantiatedScheme, generalization: &super::Generalization) -> Result<Vec<WrapperBinding>, SchemeOperationError> {
        instance.wrappers.iter().map(|id| {
            let entry = self.wrappers.entries.get(id).ok_or(ConstraintResolutionError::ForeignVariable)?;
            let forwarded = generalization.wrappers.get(id).copied();
            if matches!(entry.requirement.kind, WrapperRequirementKind::ProcCompletion { .. } | WrapperRequirementKind::KeyValidation { .. })
                && let Some(ordinal) = forwarded
            {
                return Ok(WrapperBinding::Forwarded(ordinal));
            }
            match entry.selection {
                Some(selection) => Ok(WrapperBinding::Selected(selection)),
                None => forwarded.map(WrapperBinding::Forwarded)
                    .ok_or(SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedWrapper)),
            }
        }).collect()
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::{Name, SymbolOwner};

    #[test]
    fn wrapper_bindings_follow_the_callers_published_scope() {
        let source = concat!(
            "pure keep(marker, items: Map[UInt, Int]?, key: Int) { let _ = items?[key]; marker }\n",
            "pure number(items: Map[UInt, Int]?, key: Int) -> Int { keep(7, items, key) }\n",
            "pure text(items: Map[UInt, Int]?, key: Int) -> Str { keep(\"value\", items, key) }\n",
            "pure relay(marker, items: Map[UInt, Int]?, key: Int) { keep(marker, items, key) }\n",
        );
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let checked = crate::sema::check::Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            for (name, call, quantified) in [
                ("number", "keep(7, items, key)", false),
                ("text", "keep(\"value\", items, key)", false),
                ("relay", "keep(marker, items, key)", true),
            ] {
                let declaration = checked.checked_declarations.functions.values().find(|declaration| declaration.name == Name::intern(name)).unwrap();
                let scheme = declaration.scheme.as_ref().unwrap();
                assert_eq!(scheme.callable().is_quantified(), quantified);
                assert_eq!(declaration.scheme_body_facts.is_some(), quantified);
                assert_eq!(scheme.callable().requirements().operations.len(), 1);
                assert_eq!(scheme.callable().requirements().wrappers.len(), 1);
                let evidence = checked.call_instantiations.iter().find(|(site, _)| site.role == crate::sema::schemes::InstantiationRole::Call && &source[site.source.range()] == call).unwrap().1;
                let instances = evidence.instances().collect::<Vec<_>>();
                assert_eq!(instances.len(), 1);
                if quantified {
                    assert_eq!(instances[0].operation_bindings(), &[crate::sema::schemes::SealedOperationBinding::Forwarded(0)]);
                    assert_eq!(instances[0].wrapper_bindings(), &[WrapperBinding::Forwarded(0)]);
                } else {
                    assert_eq!(instances[0].operation_bindings(), &[crate::sema::schemes::SealedOperationBinding::Selected(crate::sema::sealed_operation::SealedOperationSignature::IndexMap)]);
                    assert_eq!(instances[0].wrapper_bindings(), &[WrapperBinding::Selected(WrapperSelection::KeyValidation)]);
                }
            }
        });
    }

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
