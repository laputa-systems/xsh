use super::*;
use std::mem::size_of;
use std::ops::Deref;

#[derive(Clone, Copy, Debug)]
pub struct ScopedRoot { pub ty: TypeId, pub scope: Option<SchemeId> }

/// The context has no mutable accessor after publication. Its bound handles
/// retain exactly the graph that owns their constructors and generic scopes.
#[derive(Debug)]
pub struct SolvedGraph { context: InferenceContext }

impl Deref for SolvedGraph { type Target = InferenceContext; fn deref(&self) -> &Self::Target { &self.context } }

#[derive(Clone, Copy, Debug, Default)]
pub struct RetainedStorage {
    pub types: usize, pub rows: usize, pub variables: usize, pub schemes: usize, pub reasons: usize,
    pub requirements: usize, pub effects: usize, pub origins: usize, pub trail: usize, pub watchers: usize,
    pub type_capacity: usize, pub row_capacity: usize, pub variable_capacity: usize,
    pub type_bytes: usize, pub row_bytes: usize, pub variable_bytes: usize, pub scheme_bytes: usize,
    pub reason_bytes: usize, pub requirement_bytes: usize, pub effect_bytes: usize,
    pub origin_bytes: usize, pub trail_bytes: usize, pub queue_bytes: usize, pub interner_capacity: usize,
}

impl RetainedStorage {
    /// Hash-table allocator overhead is reported separately as a capacity;
    /// vector ownership is counted once, including retained unused capacity.
    pub fn vector_bytes(self) -> usize { self.type_bytes + self.row_bytes + self.variable_bytes + self.scheme_bytes + self.reason_bytes + self.requirement_bytes + self.effect_bytes + self.origin_bytes + self.trail_bytes + self.queue_bytes }
}

impl InferenceContext {
    pub fn retained_storage(&self) -> super::RetainedStorage {
        let mut storage = RetainedStorage {
            types: self.nodes.len(), rows: self.rows.len(), variables: self.metas.len(), schemes: self.schemes.len(), reasons: self.reasons.len(), requirements: self.requirements.len(), effects: self.effects.len(), origins: self.origins.len(), trail: self.trail.len(),
            type_capacity: self.nodes.capacity(), row_capacity: self.rows.capacity(), variable_capacity: self.metas.capacity(),
            type_bytes: self.nodes.capacity() * size_of::<Slot<TypeNode>>(), row_bytes: self.rows.capacity() * size_of::<Slot<Row>>(), variable_bytes: self.metas.capacity() * size_of::<Slot<Meta>>(), scheme_bytes: self.schemes.capacity() * size_of::<Slot<Scheme>>(), reason_bytes: self.reasons.capacity() * size_of::<Slot<Reason>>(), requirement_bytes: self.requirements.capacity() * size_of::<Slot<Requirement>>(), effect_bytes: self.effects.capacity() * size_of::<Slot<EffectVariable>>(), origin_bytes: self.origins.capacity() * size_of::<ConstraintOrigin>(), trail_bytes: self.trail.capacity() * size_of::<Trail>(), queue_bytes: self.queue.capacity() * size_of::<RequirementId>(), interner_capacity: self.atoms.capacity(), ..RetainedStorage::default()
        };
        for slot in &self.nodes { match &slot.value { TypeNode::Arrow(arrow) => storage.type_bytes += arrow.params.capacity() * size_of::<Parameter>(), TypeNode::Module(fields) => storage.type_bytes += fields.capacity() * size_of::<ModuleField>(), _ => {} } }
        for slot in &self.rows { storage.row_bytes += slot.value.fields.capacity() * size_of::<RowField>(); }
        for slot in &self.metas { storage.variable_bytes += meta_owned_bytes(&slot.value); storage.watchers += slot.value.watchers.len(); }
        for slot in &self.schemes { storage.scheme_bytes += slot.value.quantifiers.capacity() * size_of::<Quantifier>() + slot.value.requirements.capacity() * size_of::<RequirementTemplate>() + slot.value.binders.capacity() * size_of::<TypeId>() + slot.value.effect_quantifiers.capacity() * size_of::<EffectQuantifier>() + slot.value.effect_inclusions.capacity() * size_of::<(EffectSummary, EffectSummary)>(); for quantifier in &slot.value.quantifiers { storage.scheme_bytes += quantifier.lacks.capacity() * size_of::<Name>(); } }
        for slot in &self.effects { storage.effect_bytes += effect_owned_bytes(&slot.value); }
        for trail in &self.trail { match trail { Trail::Meta(_, meta) => storage.trail_bytes += meta_owned_bytes(meta), Trail::Effect(_, effect) => storage.trail_bytes += effect_owned_bytes(effect), _ => {} } }
        storage
    }
    pub fn validate_scoped(&self, root: ScopedRoot) -> Result<(), InferenceError> {
        if let Some(scope) = root.scope { self.scheme(scope)?; }
        self.validate_root(root.ty, root.scope, false)
    }
    fn validate_root(&self, root: TypeId, scope: Option<SchemeId>, allow_owned_scopes: bool) -> Result<(), InferenceError> {
        let mut pending = vec![(root, 0usize)]; let mut seen = FxHashSet::default();
        while let Some((ty, depth)) = pending.pop() {
            if depth > self.limits.structural_depth { return Err(InferenceError::Limit("structural depth")); }
            let ty = self.resolved(ty)?; if !seen.insert(ty) { continue; }
            match self.node(ty)? {
                TypeNode::Meta(_) => return Err(InferenceError::Unresolved(ty)),
                TypeNode::Poison | TypeNode::NonCompletion => return Err(InferenceError::Recovery(ty)),
                TypeNode::Rigid { scope: owner, index, kind } => {
                    if !allow_owned_scopes && scope != Some(*owner) { return Err(InferenceError::ScopeEscape); }
                    let quantifier = self.scheme(*owner)?.quantifiers.get(*index as usize).ok_or(InferenceError::InvalidScheme)?;
                    if quantifier.kind != *kind { return Err(InferenceError::KindMismatch); }
                }
                TypeNode::Arrow(arrow) => {
                    if let EffectSummary::Rigid { scope: owner, .. } = self.resolved_effect_summary(arrow.effects)? { if !allow_owned_scopes && scope != Some(owner) { return Err(InferenceError::ScopeEscape); } }
                }
                _ => {}
            }
            for child in self.children(ty)? { pending.push((child, depth + 1)); }
        }
        Ok(())
    }
    /// Bundle publication may include ground roots and declaration roots whose
    /// binders belong to validated schemes. Executable call owners additionally
    /// validate their precise lexical scopes before preparing private evidence.
    pub fn freeze(mut self, roots: &[TypeId]) -> Result<SolvedGraph, InferenceError> {
        self.solve()?;
        for root in roots { self.validate_root(*root, None, true)?; }
        self.finish_freeze()
    }
    pub fn freeze_scoped(mut self, roots: &[ScopedRoot]) -> Result<SolvedGraph, InferenceError> {
        self.solve()?;
        for root in roots { self.validate_scoped(*root)?; }
        self.finish_freeze()
    }
    fn finish_freeze(mut self) -> Result<SolvedGraph, InferenceError> {
        if self.transactions != 0 || !self.trail.is_empty() || !self.queue.is_empty() { return Err(InferenceError::InvalidScheme); }
        for (index, slot) in self.schemes.iter().enumerate() {
            let scope = SchemeId { index: index as u32, generation: slot.generation };
            for template in &slot.value.requirements { for ty in super::schemes::requirement_types(*template) { self.validate_root(ty, Some(scope), false)?; } }
            for (actual, expected) in &slot.value.effect_inclusions {
                for summary in [*actual, *expected] { if let EffectSummary::Rigid { scope: owner, .. } = self.resolved_effect_summary(summary)? { if owner != scope { return Err(InferenceError::ScopeEscape); } } }
            }
        }
        for index in 0..self.nodes.len() {
            let effects = match &self.nodes[index].value {
                TypeNode::Arrow(arrow) => { let summary = self.resolved_effect_summary(arrow.effects)?; Some(if let EffectSummary::Variable(id) = summary { EffectSummary::Closed(self.effect_value(id)?) } else { summary }) }
                _ => None,
            };
            if let Some(effects) = effects { if let TypeNode::Arrow(arrow) = &mut self.nodes[index].value { arrow.effects = effects; } }
        }
        Ok(SolvedGraph { context: self })
    }
}

fn meta_owned_bytes(meta: &Meta) -> usize { meta.lacks.capacity() * size_of::<Name>() + meta.watchers.capacity() * size_of::<RequirementId>() }
fn effect_owned_bytes(effect: &EffectVariable) -> usize { (effect.outgoing.capacity() + effect.incoming.capacity()) * size_of::<EffectId>() }
