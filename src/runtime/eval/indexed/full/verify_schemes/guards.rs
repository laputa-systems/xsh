use super::{IrVerifyError, SchemeScope};
use super::super::{guard_schemes::FullGuardScope, IrRange, IR_NONE};
use crate::sema::guarded_requirements::GuardScopeId;
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BinderPack {
    pub(super) types: Range<usize>,
    pub(super) rows: Range<usize>,
    pub(super) effects: Range<usize>,
}

#[derive(Clone, Debug)]
pub(super) struct GuardPack {
    pub(super) metadata: FullGuardScope,
    pub(super) bindings: BinderPack,
}

#[derive(Clone, Debug)]
pub(super) struct GuardCensus {
    pub(super) root: BinderPack,
    pub(super) root_applications: IrRange,
    pub(super) root_operations: IrRange,
    pub(super) root_methods: IrRange,
    pub(super) root_wrappers: IrRange,
    pub(super) scopes: Vec<GuardPack>,
    pub(super) memberships: BTreeMap<usize, GuardScopeId>,
    pub(super) children: BTreeMap<Option<GuardScopeId>, Vec<GuardScopeId>>,
}

#[derive(Clone, Debug, Default)]
pub(super) enum GuardScopes {
    #[default]
    Root,
    Scoped(Arc<GuardCensus>),
}

impl GuardScopes {
    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        match self {
            Self::Root => 0,
            Self::Scoped(census) => size_of::<GuardCensus>() + 2 * size_of::<usize>()
                + census.scopes.capacity() * size_of::<GuardPack>()
                + census.memberships.len() * size_of::<(usize, GuardScopeId)>()
                + census.children.len() * size_of::<(Option<GuardScopeId>, Vec<GuardScopeId>)>()
                + census.children.values().map(|children| children.capacity() * size_of::<GuardScopeId>()).sum::<usize>(),
        }
    }
}

impl GuardCensus {
    pub(super) fn scope(&self, id: GuardScopeId) -> Result<&GuardPack, IrVerifyError> {
        self.scopes.get(id.index()).filter(|scope| scope.metadata.id == id.raw())
            .ok_or_else(|| IrVerifyError::new("guard scope is outside its callable"))
    }

    pub(super) fn parent(&self, id: GuardScopeId) -> Result<Option<GuardScopeId>, IrVerifyError> {
        let parent = self.scope(id)?.metadata.parent;
        if parent == IR_NONE { return Ok(None); }
        let parent = GuardScopeId::from_index(parent as usize).map_err(|_| IrVerifyError::new("guard parent scope is invalid"))?;
        if parent.index() >= id.index() { return Err(IrVerifyError::new("guard scope ancestry is forward or cyclic")); }
        Ok(Some(parent))
    }

    pub(super) fn visible(&self, mut active: Option<GuardScopeId>, required: GuardScopeId) -> Result<bool, IrVerifyError> {
        for _ in 0..=crate::sema::constraints::MAX_TYPE_DEPTH {
            let Some(scope) = active else { return Ok(false); };
            if scope == required { return Ok(true); }
            active = self.parent(scope)?;
        }
        Err(IrVerifyError::new("guard scope ancestry is too deep"))
    }
}

impl SchemeScope {
    pub(super) fn owned_requirements(&self, root: &super::FullScheme) -> Result<super::super::schemes::FullRequirementRanges, IrVerifyError> {
        if let GuardScopes::Scoped(census) = &self.guards {
            if root.applications != census.root_applications || root.operations != census.root_operations
                || root.methods != census.root_methods || root.wrappers != census.root_wrappers
            { return Err(IrVerifyError::new("guard requirement pack belongs to another callable")); }
            if let Some(id) = self.active_guard {
                let metadata = census.scope(id)?.metadata;
                return Ok(super::super::schemes::FullRequirementRanges { applications: metadata.applications, operations: metadata.operations, methods: metadata.methods, wrappers: metadata.wrappers, effect_predicates: metadata.effect_predicates });
            }
        }
        Ok(super::super::schemes::FullRequirementRanges { applications: root.applications, operations: root.operations, methods: root.methods, wrappers: root.wrappers, effect_predicates: root.effect_predicates })
    }

    pub(super) fn binding_pack(&self) -> Result<BinderPack, IrVerifyError> {
        match &self.guards {
            GuardScopes::Root => Ok(BinderPack { types: 0..self.type_parameters.len(), rows: 0..self.row_parameters.len(), effects: 0..self.effects.parameters.len() }),
            GuardScopes::Scoped(census) => match self.active_guard {
                Some(id) => Ok(census.scope(id)?.bindings.clone()),
                None => Ok(census.root.clone()),
            },
        }
    }

    pub(super) fn with_guard(&self, guard: Option<GuardScopeId>) -> Result<Self, IrVerifyError> {
        let mut view = self.clone();
        match &self.guards {
            GuardScopes::Root if guard.is_some() => return Err(IrVerifyError::new("callable has no guard scope")),
            GuardScopes::Root => {},
            GuardScopes::Scoped(census) => {
                let ranges = match guard {
                    Some(id) => {
                        let scope = census.scope(id)?;
                        (scope.metadata.applications, scope.metadata.operations, scope.metadata.methods, scope.metadata.wrappers)
                    }
                    None => (census.root_applications, census.root_operations, census.root_methods, census.root_wrappers),
                };
                (view.applications, view.operations, view.methods, view.wrappers) = ranges;
            }
        }
        view.active_guard = guard;
        Ok(view)
    }

    pub(super) fn at_instruction(&self, instruction: usize) -> Result<Self, IrVerifyError> {
        let guard = match &self.guards {
            GuardScopes::Root => None,
            GuardScopes::Scoped(census) => census.memberships.get(&instruction).copied(),
        };
        self.with_guard(guard)
    }

    fn binder_guard(&self, index: usize, select: impl Fn(&BinderPack) -> &Range<usize>) -> Result<Option<GuardScopeId>, IrVerifyError> {
        let GuardScopes::Scoped(census) = &self.guards else { return Ok(None); };
        if select(&census.root).contains(&index) { return Ok(None); }
        let position = census.scopes.partition_point(|scope| select(&scope.bindings).end <= index);
        let required = census.scopes.get(position).filter(|scope| select(&scope.bindings).contains(&index))
            .ok_or_else(|| IrVerifyError::new("binder has no root or guard owner"))?;
        let required = GuardScopeId::from_index(required.metadata.id as usize).map_err(|_| IrVerifyError::new("guard binder owner is invalid"))?;
        if census.visible(self.active_guard, required)? { Ok(Some(required)) }
        else { Err(IrVerifyError::new("guard-local binder is outside its active scope")) }
    }

    pub(super) fn check_type_binder(&self, index: usize) -> Result<Option<GuardScopeId>, IrVerifyError> {
        if index >= self.type_parameters.len() { return Err(IrVerifyError::new("scheme type parameter is out of scope")); }
        self.binder_guard(index, |pack| &pack.types)
    }

    pub(super) fn check_row_binder(&self, index: usize) -> Result<Option<GuardScopeId>, IrVerifyError> {
        if index >= self.row_parameters.len() { return Err(IrVerifyError::new("scheme row parameter is out of scope")); }
        self.binder_guard(index, |pack| &pack.rows)
    }

    pub(super) fn check_effect_binder(&self, index: usize) -> Result<Option<GuardScopeId>, IrVerifyError> {
        if index >= self.effects.parameters.len() { return Err(IrVerifyError::new("scheme effect parameter is out of scope")); }
        self.binder_guard(index, |pack| &pack.effects)
    }
    pub(super) fn check_guard_visibility(&self, required: Option<GuardScopeId>) -> Result<(), IrVerifyError> {
        let Some(required) = required else { return Ok(()); };
        let GuardScopes::Scoped(census) = &self.guards else { return Err(IrVerifyError::new("guard-local type escaped its callable")); };
        if census.visible(self.active_guard, required)? { Ok(()) }
        else { Err(IrVerifyError::new("guard-local binder is outside its active scope")) }
    }

    pub(super) fn combine_guard_requirements(&self, left: Option<GuardScopeId>, right: Option<GuardScopeId>) -> Result<Option<GuardScopeId>, IrVerifyError> {
        let (Some(left), Some(right)) = (left, right) else { return Ok(left.or(right)); };
        let GuardScopes::Scoped(census) = &self.guards else { return Err(IrVerifyError::new("guard-local type has no callable scope")); };
        if census.visible(Some(left), right)? { Ok(Some(left)) }
        else if census.visible(Some(right), left)? { Ok(Some(right)) }
        else { Err(IrVerifyError::new("type refers to sibling guard scopes")) }
    }

}

pub(super) fn verify_body_sources(
    store: &super::FullStore, decoder: &super::FullDecoder<'_>, scope: &SchemeScope,
    types: &BTreeMap<usize, super::SchemeType>, prefix: Option<&super::ProducerPrefixContext<'_>>,
    budget: &mut super::SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    let GuardScopes::Scoped(census) = &scope.guards else { return Ok(()); };
    let prefix = prefix.ok_or_else(|| IrVerifyError::new("guarded body has no physical prefix proof"))?;
    for pack in &census.scopes {
        budget.charge(1)?;
        let id = GuardScopeId::from_index(pack.metadata.id as usize).map_err(|_| IrVerifyError::new("guard scope id is invalid"))?;
        let parent = census.parent(id)?;
        let parent_scope = scope.with_guard(parent)?;
        let source = pack.metadata.source_instruction as usize;
        let comparison = pack.metadata.comparison_instruction as usize;
        if scope.at_instruction(source)?.active_guard != parent || scope.at_instruction(comparison)?.active_guard != parent {
            return Err(IrVerifyError::new("guard source or comparison is outside its parent scope"));
        }
        let expected = super::decode_scheme_type(store, &parent_scope, pack.metadata.source, budget, 0)?;
        if types.get(&source).is_none_or(|actual| !super::body_types_equal(actual, &expected)) {
            return Err(IrVerifyError::new("guard source disagrees with its checked producer"));
        }
        if !prefix.actual_null_comparison(decoder, comparison, source, &mut |amount| budget.charge(amount))? {
            return Err(IrVerifyError::new("guard comparison does not test its actual source for null"));
        }
    }
    for (&instruction, &id) in &census.memberships {
        budget.charge(1)?;
        if decoder.instruction_parent_of(instruction)?.is_some_and(|parent| census.memberships.get(&parent) == Some(&id)) { continue; }
        let source = census.scope(id)?.metadata.source_instruction as usize;
        if !prefix.guarded_nonnull_scope(decoder, instruction, source, &mut |amount| budget.charge(amount))? {
            return Err(IrVerifyError::new("guard-local body fact has no dominating non-null source proof"));
        }
    }
    Ok(())
}

pub(super) fn authenticates_read(
    store: &super::FullStore, decoder: &super::FullDecoder<'_>, scope: &SchemeScope,
    instruction: usize, actual: &super::SchemeType, claimed: &super::SchemeType,
    prefix: Option<&super::ProducerPrefixContext<'_>>, budget: &mut super::SchemeInstantiationBudget,
) -> Result<bool, IrVerifyError> {
    let GuardScopes::Scoped(census) = &scope.guards else { return Ok(false); };
    let Some(prefix) = prefix else { return Ok(false); };
    let mut active = scope.active_guard;
    while let Some(id) = active {
        budget.charge(1)?;
        let pack = census.scope(id)?;
        let parent_scope = scope.with_guard(census.parent(id)?)?;
        let local_scope = scope.with_guard(Some(id))?;
        let source = super::decode_scheme_type(store, &parent_scope, pack.metadata.source, budget, 0)?;
        let view = super::decode_scheme_type(store, &local_scope, pack.metadata.view, budget, 0)?;
        if super::body_types_equal(actual, &source) && super::body_types_equal(claimed, &view)
            && prefix.guarded_nonnull_read(decoder, instruction, pack.metadata.source_instruction as usize, &mut |amount| budget.charge(amount))?
        { return Ok(true); }
        for read in super::bounded(pack.metadata.reads, &store.scheme_guard_reads, "guard read range is invalid")? {
            budget.charge(1)?;
            if instruction != read.instruction as usize { continue; }
            let source = super::decode_scheme_type(store, &parent_scope, read.source, budget, 0)?;
            let view = super::decode_scheme_type(store, &local_scope, read.view, budget, 0)?;
            if super::body_types_equal(actual, &source) && super::body_types_equal(claimed, &view) { return Ok(true); }
        }
        active = census.parent(id)?;
    }
    Ok(false)
}

pub(super) fn verify_ground_bindings(
    store: &super::FullStore, root: &super::FullScheme, scope: &SchemeScope,
    arguments: super::super::schemes::FullInstantiation, substitution: &super::CallSubstitution,
    type_coverage: &mut [bool], row_coverage: &mut [bool], field_coverage: &mut [bool],
    budget: &mut super::SchemeInstantiationBudget,
) -> Result<(), IrVerifyError> {
    use super::super::guard_schemes::FullGuardBinding;
    use crate::sema::guarded_requirements::{non_null_source, NonNullSource};
    let GuardScopes::Scoped(census) = &scope.guards else {
        return if arguments.guard_bindings.len == 0 { Ok(()) } else { Err(IrVerifyError::new("unguarded callable has guard bindings")) };
    };
    budget.charge(substitution.types.len() + substitution.rows.iter().map(BTreeMap::len).sum::<usize>() + substitution.effects.len())?;
    let mut pending = vec![(None, arguments.guard_bindings, Arc::new(substitution.clone()))];
    while let Some((parent_id, encoded, parent)) = pending.pop() {
        let children = census.children.get(&parent_id).map(Vec::as_slice).unwrap_or(&[]);
        let bindings = super::bounded(encoded, &store.call_guard_bindings, "guard binding range is invalid")?;
        if children.len() != bindings.len() { return Err(IrVerifyError::new("guard call bindings are incomplete")); }
        for (&id, binding) in children.iter().zip(bindings) {
            budget.charge(1)?;
            let pack = census.scope(id)?;
            let source = super::instantiate_bound_type(store, pack.metadata.source, &parent, budget, 0)?;
            let actual = non_null_source(&source).map_err(|_| IrVerifyError::new("guard call source has no closed non-null classification"))?;
            let (local_arguments, payload) = match (actual, binding) {
                (NonNullSource::Null, FullGuardBinding::Null) => continue,
                (NonNullSource::NonNull(payload), FullGuardBinding::NonNull(local))
                | (NonNullSource::Optional(payload), FullGuardBinding::Optional(local)) => (*local, payload),
                _ => return Err(IrVerifyError::new("guard call binding disagrees with its actual source classification")),
            };
            let local_scope = scope.with_guard(Some(id))?;
            let types = super::mark_range(local_arguments.type_arguments, &store.call_type_arguments, type_coverage, "guard type arguments are invalid or overlap")?;
            let rows = super::mark_range(local_arguments.row_arguments, &store.call_row_arguments, row_coverage, "guard row arguments are invalid or overlap")?;
            let effects = super::bounded(local_arguments.effect_arguments, &store.call_effect_arguments, "guard effect arguments are invalid")?;
            let local = super::build_pack_substitution(store, &local_scope, types, rows, effects, field_coverage, Some(Arc::clone(&parent)))?;
            let actual_view = super::instantiate_bound_type(store, pack.metadata.view, &local, budget, 0)?;
            if &actual_view != payload { return Err(IrVerifyError::new("guard local payload disagrees with its actual source")); }
            let mut evidence = super::SchemeBinderEvidence::default();
            let mut ancestor = parent_id;
            evidence.types.extend(census.root.types.clone());
            evidence.rows.extend(census.root.rows.clone());
            while let Some(ancestor_id) = ancestor {
                let ancestor_pack = census.scope(ancestor_id)?;
                evidence.types.extend(ancestor_pack.bindings.types.clone());
                evidence.rows.extend(ancestor_pack.bindings.rows.clone());
                ancestor = census.parent(ancestor_id)?;
            }
            super::verify_scheme_argument_evidence(store, &local_scope, pack.metadata.view, payload, &local, &mut evidence, budget, 0)?;
            for read in super::bounded(pack.metadata.reads, &store.scheme_guard_reads, "guard read range is invalid")? {
                let actual = super::instantiate_bound_type(store, read.source, &parent, budget, 0)?;
                let view = super::instantiate_bound_type(store, read.view, &local, budget, 0)?;
                if actual != view { return Err(IrVerifyError::new("guard identity read changes its actual source type")); }
                super::verify_scheme_argument_evidence(store, &local_scope, read.view, &actual, &local, &mut evidence, budget, 0)?;
            }
            let substitution = super::applications::ApplicationSubstitution::Ground(&local);
            super::effects::verify_prepared_predicates(store, &local_scope, root, &local, budget)?;
            super::applications::verify_bindings(store, root, &local_scope, substitution, None,
                super::bounded(local_arguments.application_bindings, &store.call_application_bindings, "guard application bindings are invalid")?, budget)?;
            let operation_bindings = super::bounded(local_arguments.operation_bindings, &store.call_operation_bindings, "guard operation bindings are invalid")?;
            super::operations::verify_bindings(store, root, &local_scope, substitution, None, operation_bindings, budget)?;
            super::methods::verify_bindings(store, root, &local_scope, substitution, None,
                super::bounded(local_arguments.method_bindings, &store.call_method_bindings, "guard method bindings are invalid")?, budget)?;
            super::wrappers::verify_bindings(store, root, &local_scope, substitution, None, operation_bindings,
                super::bounded(local_arguments.wrapper_bindings, &store.call_wrapper_bindings, "guard wrapper bindings are invalid")?, budget)?;
            super::applications::extend_call_binder_evidence(store, root, &local_scope, substitution, &mut evidence, budget)?;
            if !pack.bindings.types.clone().all(|binder| evidence.types.contains(&binder))
                || !pack.bindings.rows.clone().all(|binder| evidence.rows.contains(&binder))
            { return Err(IrVerifyError::new("guard local substitution has no independently checked input evidence")); }
            pending.push((Some(id), local_arguments.guard_bindings, Arc::new(local)));
        }
    }
    Ok(())
}
